use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Native, canonical-byte memory observations collected from the Windows guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryTelemetrySnapshot {
    pub physical_total_bytes: u64,
    pub physical_available_bytes: u64,
    pub memory_load_percent: u32,
    pub commit_total_bytes: u64,
    pub commit_limit_bytes: u64,
    pub commit_peak_bytes: u64,
    pub system_cache_bytes: u64,
    pub kernel_paged_bytes: u64,
    pub kernel_nonpaged_bytes: u64,
}

impl MemoryTelemetrySnapshot {
    pub fn validate(&self) -> Result<(), DemandError> {
        if self.physical_total_bytes == 0 {
            return Err(DemandError::ZeroCounter("physical total"));
        }
        if self.commit_limit_bytes == 0 {
            return Err(DemandError::ZeroCounter("commit limit"));
        }
        if self.physical_available_bytes > self.physical_total_bytes {
            return Err(DemandError::InconsistentCounters(
                "physical available exceeds physical total",
            ));
        }
        if self.commit_total_bytes > self.commit_limit_bytes {
            return Err(DemandError::InconsistentCounters(
                "commit total exceeds commit limit",
            ));
        }
        if self.commit_peak_bytes < self.commit_total_bytes {
            return Err(DemandError::InconsistentCounters(
                "commit peak is below commit total",
            ));
        }
        if self.memory_load_percent > 100 {
            return Err(DemandError::InvalidMemoryLoad(self.memory_load_percent));
        }
        Ok(())
    }

    pub fn physical_pressure(&self) -> Result<f64, DemandError> {
        self.validate()?;
        Ok(1.0 - (self.physical_available_bytes as f64 / self.physical_total_bytes as f64))
    }

    pub fn commit_pressure(&self) -> Result<f64, DemandError> {
        self.validate()?;
        Ok(self.commit_total_bytes as f64 / self.commit_limit_bytes as f64)
    }
}

/// Provisional demand levels. They are recommendations and never authorize a resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemandState {
    Release,
    Stable,
    WantMore,
    Pressure,
    Critical,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DemandError {
    #[error("memory counter must be greater than zero: {0}")]
    ZeroCounter(&'static str),
    #[error("inconsistent memory counters: {0}")]
    InconsistentCounters(&'static str),
    #[error("memory load percentage is outside 0..=100: {0}")]
    InvalidMemoryLoad(u32),
    #[error("demand policy value is invalid: {0}")]
    InvalidPolicy(&'static str),
    #[error("memory target arithmetic overflow")]
    ArithmeticOverflow,
    #[error("native Windows memory telemetry is unavailable on this platform")]
    UnsupportedPlatform,
    #[error("GlobalMemoryStatusEx failed: {0}")]
    GlobalMemoryStatus(u32),
    #[error("GetPerformanceInfo failed: {0}")]
    PerformanceInfo(u32),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DemandAgentError {
    #[error("collect demand telemetry: {0}")]
    Telemetry(#[from] DemandError),
    #[error("publish demand report: {0}")]
    Publication(String),
}

/// Bounds and alignment used by the advisory target calculator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemandPolicyConfig {
    pub configured_minimum_bytes: u64,
    pub configured_maximum_bytes: u64,
    pub block_size_bytes: u64,
}

impl DemandPolicyConfig {
    pub fn validate(&self) -> Result<(), DemandError> {
        if self.configured_minimum_bytes == 0
            || self.configured_maximum_bytes == 0
            || self.block_size_bytes == 0
        {
            return Err(DemandError::InvalidPolicy(
                "values must be greater than zero",
            ));
        }
        if self.configured_minimum_bytes > self.configured_maximum_bytes {
            return Err(DemandError::InvalidPolicy("minimum exceeds maximum"));
        }
        if !self.block_size_bytes.is_power_of_two() {
            return Err(DemandError::InvalidPolicy(
                "block size must be a power of two",
            ));
        }
        if !self
            .configured_minimum_bytes
            .is_multiple_of(self.block_size_bytes)
            || !self
                .configured_maximum_bytes
                .is_multiple_of(self.block_size_bytes)
        {
            return Err(DemandError::InvalidPolicy("limits must be block aligned"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DemandReport {
    pub version: u16,
    pub memory: MemoryTelemetrySnapshot,
    pub demand: DemandRecommendation,
    pub limits: DemandLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DemandRecommendation {
    pub state: DemandState,
    pub physical_pressure: f64,
    pub commit_pressure: f64,
    pub desired_target_bytes: u64,
    pub safe_floor_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemandLimits {
    pub configured_minimum_bytes: u64,
    pub configured_maximum_bytes: u64,
}

#[derive(Debug)]
pub struct DemandCalculator {
    config: DemandPolicyConfig,
}

impl DemandCalculator {
    pub fn new(config: DemandPolicyConfig) -> Result<Self, DemandError> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn calculate(
        &self,
        snapshot: MemoryTelemetrySnapshot,
        current_bytes: u64,
    ) -> Result<DemandReport, DemandError> {
        snapshot.validate()?;
        if current_bytes == 0 || !current_bytes.is_multiple_of(self.config.block_size_bytes) {
            return Err(DemandError::InvalidPolicy(
                "current allocation must be positive and block aligned",
            ));
        }

        let physical_pressure = snapshot.physical_pressure()?;
        let commit_pressure = snapshot.commit_pressure()?;
        let pressure = physical_pressure.max(commit_pressure);
        let state = classify_pressure(pressure);
        let desired_steps = match state {
            DemandState::Release => -1_i64,
            DemandState::Stable => 0,
            DemandState::WantMore => 1,
            DemandState::Pressure => 2,
            DemandState::Critical => 4,
        };
        let desired_target_bytes = aligned_target(
            current_bytes,
            desired_steps,
            self.config.block_size_bytes,
            self.config.configured_minimum_bytes,
            self.config.configured_maximum_bytes,
        )?;
        let safe_floor_bytes = aligned_target(
            current_bytes,
            -1,
            self.config.block_size_bytes,
            self.config.configured_minimum_bytes,
            current_bytes,
        )?;

        Ok(DemandReport {
            version: 1,
            memory: snapshot,
            demand: DemandRecommendation {
                state,
                physical_pressure,
                commit_pressure,
                desired_target_bytes,
                safe_floor_bytes,
            },
            limits: DemandLimits {
                configured_minimum_bytes: self.config.configured_minimum_bytes,
                configured_maximum_bytes: self.config.configured_maximum_bytes,
            },
        })
    }
}

/// Publishes an advisory report without granting the publisher resize authority.
pub trait DemandReportPublisher {
    fn publish(&mut self, report: DemandReport) -> Result<(), String>;
}

/// Appends complete versioned reports as newline-delimited JSON records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonLinesDemandReportPublisher {
    path: PathBuf,
}

impl JsonLinesDemandReportPublisher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl DemandReportPublisher for JsonLinesDemandReportPublisher {
    fn publish(&mut self, report: DemandReport) -> Result<(), String> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create demand report directory: {error}"))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| format!("open demand report {}: {error}", self.path.display()))?;
        let mut encoded = serde_json::to_vec(&report)
            .map_err(|error| format!("encode demand report: {error}"))?;
        encoded.push(b'\n');
        file.write_all(&encoded)
            .map_err(|error| format!("write demand report: {error}"))?;
        file.flush()
            .map_err(|error| format!("flush demand report: {error}"))?;
        Ok(())
    }
}

/// Collects and publishes one demand report per caller-selected poll cycle.
///
/// The caller supplies the observed current allocation. This keeps the agent
/// independent from QGA, libvirt, and driver state, while making the state
/// boundary explicit and testable.
#[derive(Debug)]
pub struct DemandAgent<T> {
    telemetry: T,
    calculator: DemandCalculator,
}

impl<T> DemandAgent<T>
where
    T: MemoryTelemetry,
{
    pub fn new(telemetry: T, calculator: DemandCalculator) -> Self {
        Self {
            telemetry,
            calculator,
        }
    }

    pub fn collect_report(&self, current_bytes: u64) -> Result<DemandReport, DemandAgentError> {
        let snapshot = self.telemetry.collect()?;
        self.calculator
            .calculate(snapshot, current_bytes)
            .map_err(DemandAgentError::Telemetry)
    }

    pub fn collect_and_publish<P>(
        &self,
        current_bytes: u64,
        publisher: &mut P,
    ) -> Result<DemandReport, DemandAgentError>
    where
        P: DemandReportPublisher,
    {
        let report = self.collect_report(current_bytes)?;
        publisher
            .publish(report)
            .map_err(DemandAgentError::Publication)?;
        Ok(report)
    }
}

fn classify_pressure(pressure: f64) -> DemandState {
    if pressure < 0.25 {
        DemandState::Release
    } else if pressure < 0.60 {
        DemandState::Stable
    } else if pressure < 0.75 {
        DemandState::WantMore
    } else if pressure < 0.90 {
        DemandState::Pressure
    } else {
        DemandState::Critical
    }
}

fn aligned_target(
    current_bytes: u64,
    steps: i64,
    block_size_bytes: u64,
    minimum_bytes: u64,
    maximum_bytes: u64,
) -> Result<u64, DemandError> {
    let delta = block_size_bytes
        .checked_mul(steps.unsigned_abs())
        .ok_or(DemandError::ArithmeticOverflow)?;
    let target = if steps.is_negative() {
        current_bytes.saturating_sub(delta)
    } else {
        current_bytes
            .checked_add(delta)
            .ok_or(DemandError::ArithmeticOverflow)?
    };
    Ok(target.clamp(minimum_bytes, maximum_bytes))
}

/// Collects native Windows memory counters using the documented system APIs.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeMemoryTelemetry;

pub trait MemoryTelemetry {
    fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError>;
}

impl MemoryTelemetry for NativeMemoryTelemetry {
    #[cfg(windows)]
    fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError> {
        use std::mem::{size_of, zeroed};
        use winapi::shared::minwindef::FALSE;
        use winapi::um::psapi::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
        use winapi::um::sysinfoapi::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

        // SAFETY: The Windows APIs initialize the supplied, correctly sized
        // structures and do not retain the pointers after returning.
        let memory = unsafe {
            let mut value: MEMORYSTATUSEX = zeroed();
            value.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut value) == FALSE {
                return Err(DemandError::GlobalMemoryStatus(
                    std::io::Error::last_os_error()
                        .raw_os_error()
                        .unwrap_or_default() as u32,
                ));
            }
            value
        };
        // SAFETY: The Windows API initializes the supplied structure and only
        // writes within its declared size.
        let performance = unsafe {
            let mut value: PERFORMANCE_INFORMATION = zeroed();
            if GetPerformanceInfo(&mut value, size_of::<PERFORMANCE_INFORMATION>() as u32) == FALSE
            {
                return Err(DemandError::PerformanceInfo(
                    std::io::Error::last_os_error()
                        .raw_os_error()
                        .unwrap_or_default() as u32,
                ));
            }
            value
        };
        let page_size = performance.PageSize as u64;
        let pages_to_bytes = |pages: usize| {
            (pages as u64)
                .checked_mul(page_size)
                .ok_or(DemandError::ArithmeticOverflow)
        };

        let snapshot = MemoryTelemetrySnapshot {
            physical_total_bytes: memory.ullTotalPhys,
            physical_available_bytes: memory.ullAvailPhys,
            memory_load_percent: memory.dwMemoryLoad,
            commit_total_bytes: pages_to_bytes(performance.CommitTotal as usize)?,
            commit_limit_bytes: pages_to_bytes(performance.CommitLimit as usize)?,
            commit_peak_bytes: pages_to_bytes(performance.CommitPeak as usize)?,
            system_cache_bytes: pages_to_bytes(performance.SystemCache as usize)?,
            kernel_paged_bytes: pages_to_bytes(performance.KernelPaged as usize)?,
            kernel_nonpaged_bytes: pages_to_bytes(performance.KernelNonpaged as usize)?,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    #[cfg(not(windows))]
    fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError> {
        Err(DemandError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn policy() -> DemandCalculator {
        DemandCalculator::new(DemandPolicyConfig {
            configured_minimum_bytes: 4 * GIB,
            configured_maximum_bytes: 32 * GIB,
            block_size_bytes: 2 * GIB,
        })
        .expect("policy should be valid")
    }

    fn snapshot(available: u64, commit: u64) -> MemoryTelemetrySnapshot {
        MemoryTelemetrySnapshot {
            physical_total_bytes: 16 * GIB,
            physical_available_bytes: available,
            memory_load_percent: 50,
            commit_total_bytes: commit,
            commit_limit_bytes: 16 * GIB,
            commit_peak_bytes: commit,
            system_cache_bytes: 0,
            kernel_paged_bytes: 0,
            kernel_nonpaged_bytes: 0,
        }
    }

    #[derive(Clone)]
    struct StubTelemetry {
        result: Result<MemoryTelemetrySnapshot, DemandError>,
    }

    impl MemoryTelemetry for StubTelemetry {
        fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError> {
            self.result.clone()
        }
    }

    #[derive(Default)]
    struct StubPublisher {
        reports: Vec<DemandReport>,
        failure: Option<String>,
    }

    impl DemandReportPublisher for StubPublisher {
        fn publish(&mut self, report: DemandReport) -> Result<(), String> {
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            self.reports.push(report);
            Ok(())
        }
    }

    #[test]
    fn rejects_invalid_counters() {
        let mut value = snapshot(8 * GIB, 4 * GIB);
        value.physical_available_bytes = 17 * GIB;
        assert_eq!(
            value.validate(),
            Err(DemandError::InconsistentCounters(
                "physical available exceeds physical total"
            ))
        );

        value = snapshot(8 * GIB, 4 * GIB);
        value.commit_limit_bytes = 0;
        assert_eq!(
            value.validate(),
            Err(DemandError::ZeroCounter("commit limit"))
        );
    }

    #[test]
    fn pressure_ratios_are_bounded() {
        let value = snapshot(4 * GIB, 12 * GIB);
        assert!((value.physical_pressure().unwrap() - 0.75).abs() < f64::EPSILON);
        assert!((value.commit_pressure().unwrap() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn calculates_aligned_bounded_recommendation() {
        let report = policy()
            .calculate(snapshot(2 * GIB, 15 * GIB), 30 * GIB)
            .expect("report should be valid");
        assert_eq!(report.version, 1);
        assert_eq!(report.demand.state, DemandState::Critical);
        assert_eq!(report.demand.desired_target_bytes, 32 * GIB);
        assert_eq!(report.demand.safe_floor_bytes, 28 * GIB);
    }

    #[test]
    fn release_respects_configured_minimum() {
        let report = policy()
            .calculate(snapshot(15 * GIB, GIB), 4 * GIB)
            .expect("report should be valid");
        assert_eq!(report.demand.state, DemandState::Release);
        assert_eq!(report.demand.desired_target_bytes, 4 * GIB);
        assert_eq!(report.demand.safe_floor_bytes, 4 * GIB);
    }

    #[test]
    fn rejects_unaligned_current_allocation() {
        assert_eq!(
            policy().calculate(snapshot(8 * GIB, 4 * GIB), 3 * GIB),
            Err(DemandError::InvalidPolicy(
                "current allocation must be positive and block aligned"
            ))
        );
    }

    #[test]
    fn demand_agent_publishes_advisory_report_only_after_valid_collection() {
        let agent = DemandAgent::new(
            StubTelemetry {
                result: Ok(snapshot(2 * GIB, 15 * GIB)),
            },
            policy(),
        );
        let mut publisher = StubPublisher::default();

        let report = agent
            .collect_and_publish(30 * GIB, &mut publisher)
            .expect("report should publish");

        assert_eq!(publisher.reports, vec![report]);
        assert_eq!(report.demand.state, DemandState::Critical);
    }

    #[test]
    fn demand_agent_does_not_publish_invalid_telemetry() {
        let agent = DemandAgent::new(
            StubTelemetry {
                result: Err(DemandError::ZeroCounter("commit limit")),
            },
            policy(),
        );
        let mut publisher = StubPublisher::default();

        assert_eq!(
            agent.collect_and_publish(30 * GIB, &mut publisher),
            Err(DemandAgentError::Telemetry(DemandError::ZeroCounter(
                "commit limit"
            )))
        );
        assert!(publisher.reports.is_empty());
    }

    #[test]
    fn demand_agent_preserves_publication_failure() {
        let agent = DemandAgent::new(
            StubTelemetry {
                result: Ok(snapshot(2 * GIB, 15 * GIB)),
            },
            policy(),
        );
        let mut publisher = StubPublisher {
            failure: Some("sink unavailable".to_owned()),
            ..StubPublisher::default()
        };

        assert_eq!(
            agent.collect_and_publish(30 * GIB, &mut publisher),
            Err(DemandAgentError::Publication("sink unavailable".to_owned()))
        );
    }

    #[test]
    fn json_lines_publisher_appends_complete_versioned_record() {
        let path = std::env::temp_dir().join(format!(
            "virtio-mem-demand-{}-{}.jsonl",
            std::process::id(),
            "publisher"
        ));
        let _ = std::fs::remove_file(&path);
        let report = policy()
            .calculate(snapshot(2 * GIB, 15 * GIB), 30 * GIB)
            .expect("report should be valid");
        let mut publisher = JsonLinesDemandReportPublisher::new(&path);

        publisher.publish(report).expect("record should be written");
        let content = std::fs::read_to_string(&path).expect("record should be readable");
        let records: Vec<DemandReport> = content
            .lines()
            .map(|line| serde_json::from_str(line).expect("record should be valid JSON"))
            .collect();

        assert_eq!(records, vec![report]);
        assert!(content.ends_with('\n'));
        std::fs::remove_file(path).expect("test record should be removed");
    }

    #[test]
    fn native_collector_has_platform_behavior() {
        #[cfg(not(windows))]
        assert_eq!(
            NativeMemoryTelemetry.collect(),
            Err(DemandError::UnsupportedPlatform)
        );
    }
}
