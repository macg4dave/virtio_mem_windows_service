use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const MAX_RAW_TELEMETRY_RECORD_BYTES: usize = 64 * 1024;
pub const RAW_TELEMETRY_RETENTION_FILES: usize = 3;

pub use virtio_mem_core::{
    DemandCalculator, DemandError, DemandLimits, DemandPolicyConfig, DemandRecommendation,
    DemandReport, DemandState, MemoryTelemetrySnapshot, RawTelemetryEnvelope,
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DemandAgentError {
    #[error("collect demand telemetry: {0}")]
    Telemetry(#[from] DemandError),
    #[error("publish demand report: {0}")]
    Publication(String),
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

/// Publishes raw telemetry without accepting allocation or resize input.
pub trait RawTelemetryPublisher {
    fn publish(&mut self, envelope: &RawTelemetryEnvelope) -> Result<(), String>;
}

/// Appends complete raw telemetry envelopes as newline-delimited JSON records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonLinesRawTelemetryPublisher {
    path: PathBuf,
}

impl JsonLinesRawTelemetryPublisher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl RawTelemetryPublisher for JsonLinesRawTelemetryPublisher {
    fn publish(&mut self, envelope: &RawTelemetryEnvelope) -> Result<(), String> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create raw telemetry directory: {error}"))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| format!("open raw telemetry {}: {error}", self.path.display()))?;
        let mut encoded = serde_json::to_vec(envelope)
            .map_err(|error| format!("encode raw telemetry: {error}"))?;
        encoded.push(b'\n');
        file.write_all(&encoded)
            .map_err(|error| format!("write raw telemetry: {error}"))?;
        file.flush()
            .map_err(|error| format!("flush raw telemetry: {error}"))
    }
}

/// Publishes one complete current record through an atomic replace and keeps a
/// bounded set of previous records for diagnostics. Readers never consume a
/// partially written current record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicRawTelemetryPublisher {
    path: PathBuf,
    retention_files: usize,
}

impl AtomicRawTelemetryPublisher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            retention_files: RAW_TELEMETRY_RETENTION_FILES,
        }
    }

    pub fn with_retention(path: impl Into<PathBuf>, retention_files: usize) -> Self {
        Self {
            path: path.into(),
            retention_files,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn backup_path(&self, index: usize) -> PathBuf {
        PathBuf::from(format!("{}.{}", self.path.display(), index))
    }
}

impl RawTelemetryPublisher for AtomicRawTelemetryPublisher {
    fn publish(&mut self, envelope: &RawTelemetryEnvelope) -> Result<(), String> {
        let mut encoded = serde_json::to_vec(envelope)
            .map_err(|error| format!("encode raw telemetry: {error}"))?;
        encoded.push(b'\n');
        if encoded.len() > MAX_RAW_TELEMETRY_RECORD_BYTES {
            return Err(format!(
                "raw telemetry record exceeds {MAX_RAW_TELEMETRY_RECORD_BYTES} byte limit"
            ));
        }
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create raw telemetry directory: {error}"))?;
        }

        if self.retention_files > 0 {
            let oldest = self.backup_path(self.retention_files);
            match std::fs::remove_file(&oldest) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("remove oldest raw telemetry backup: {error}")),
            }
            for index in (1..self.retention_files).rev() {
                let from = self.backup_path(index);
                let to = self.backup_path(index + 1);
                match std::fs::rename(&from, &to) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(format!("rotate raw telemetry backup: {error}")),
                }
            }
            if self.path.exists() {
                std::fs::copy(&self.path, self.backup_path(1))
                    .map_err(|error| format!("retain previous raw telemetry record: {error}"))?;
            }
        }

        let temporary = PathBuf::from(format!(
            "{}.tmp-{}",
            self.path.display(),
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("open raw telemetry handoff: {error}"))?;
        file.write_all(&encoded)
            .map_err(|error| format!("write raw telemetry handoff: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("flush raw telemetry handoff: {error}"))?;
        drop(file);
        atomic_replace(&temporary, &self.path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            format!("publish raw telemetry handoff: {error}")
        })
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::shared::minwindef::FALSE;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: Both pointers reference NUL-terminated UTF-16 buffers that live
    // for the complete call, and MoveFileExW does not retain them.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == FALSE
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub trait TelemetryClock {
    fn now_unix_millis(&self) -> Result<u64, String>;
    fn monotonic_millis(&self) -> Result<u64, String>;
}

#[derive(Debug, Clone)]
pub struct SystemTelemetryClock {
    started_at: Instant,
}

impl Default for SystemTelemetryClock {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl TelemetryClock for SystemTelemetryClock {
    fn now_unix_millis(&self) -> Result<u64, String> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
        u64::try_from(duration.as_millis())
            .map_err(|_| "system Unix timestamp does not fit in u64 milliseconds".to_owned())
    }

    fn monotonic_millis(&self) -> Result<u64, String> {
        u64::try_from(self.started_at.elapsed().as_millis())
            .map_err(|_| "process monotonic timestamp does not fit in u64 milliseconds".to_owned())
    }
}

pub fn process_session_id(service_name: &str) -> Result<String, String> {
    if service_name.trim().is_empty() {
        return Err("service name must be non-empty for session identity".to_owned());
    }
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
    Ok(format!(
        "session-{:x}-{:x}",
        duration.as_nanos(),
        std::process::id()
    ))
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
            grow_step_bytes: 2 * GIB,
            shrink_step_bytes: 2 * GIB,
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
                "current allocation must be block aligned"
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
    fn raw_json_lines_publisher_appends_only_raw_vm_scoped_telemetry() {
        let path = std::env::temp_dir().join(format!(
            "virtio-mem-raw-telemetry-{}-publisher.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let envelope = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000_000,
            10,
            0,
            snapshot(2 * GIB, 15 * GIB),
        );
        let mut publisher = JsonLinesRawTelemetryPublisher::new(&path);

        publisher
            .publish(&envelope)
            .expect("raw record should be written");
        let content = std::fs::read_to_string(&path).expect("raw record should be readable");
        let decoded: RawTelemetryEnvelope =
            serde_json::from_str(content.trim()).expect("raw record should parse");

        assert_eq!(decoded, envelope);
        assert!(!content.contains("desired_target_bytes"));
        assert!(!content.contains("current_bytes"));
        std::fs::remove_file(path).expect("test record should be removed");
    }

    #[test]
    fn atomic_publisher_handoffs_complete_records_with_bounded_retention() {
        let path = std::env::temp_dir().join(format!(
            "virtio-mem-raw-telemetry-{}-atomic.jsonl",
            std::process::id()
        ));
        for suffix in ["", ".1", ".2", ".3"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        let mut publisher = AtomicRawTelemetryPublisher::with_retention(&path, 2);
        for sequence in 0..4 {
            let envelope = RawTelemetryEnvelope::new(
                "guest",
                "VirtioMemService",
                "session-a",
                1_000_000 + sequence,
                10 + sequence,
                sequence,
                snapshot(2 * GIB, 15 * GIB),
            );
            publisher.publish(&envelope).expect("atomic publish");
        }
        let current: RawTelemetryEnvelope = serde_json::from_str(
            std::fs::read_to_string(&path)
                .expect("current record")
                .trim(),
        )
        .expect("complete JSON");
        let previous: RawTelemetryEnvelope = serde_json::from_str(
            std::fs::read_to_string(format!("{}.1", path.display()))
                .expect("previous record")
                .trim(),
        )
        .expect("complete retained JSON");
        assert_eq!(current.sequence, 3);
        assert_eq!(previous.sequence, 2);
        assert!(std::fs::metadata(format!("{}.3", path.display())).is_err());
        for suffix in ["", ".1", ".2"] {
            std::fs::remove_file(format!("{}{suffix}", path.display())).expect("remove fixture");
        }
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
