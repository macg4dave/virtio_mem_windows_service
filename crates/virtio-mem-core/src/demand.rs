use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const RAW_TELEMETRY_VERSION: u16 = 2;
pub const DEMAND_REPORT_VERSION: u16 = 1;

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

/// The native source used for the guest memory counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetrySource {
    WindowsNativeMemoryApis,
}

/// Declares that this guest record carries no allocation and requires a host join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationProvenance {
    HostLiveLibvirtCurrentRequired,
}

/// The raw-telemetry record. Allocation remains host-owned and is not
/// present in this guest-produced envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTelemetryEnvelope {
    pub version: u16,
    pub vm_name: String,
    pub service_name: String,
    pub session_id: String,
    pub observed_unix_millis: u64,
    pub monotonic_millis: u64,
    pub sequence: u64,
    pub telemetry_source: TelemetrySource,
    pub allocation_provenance: AllocationProvenance,
    pub memory: MemoryTelemetrySnapshot,
}

impl RawTelemetryEnvelope {
    pub fn new(
        vm_name: impl Into<String>,
        service_name: impl Into<String>,
        session_id: impl Into<String>,
        observed_unix_millis: u64,
        monotonic_millis: u64,
        sequence: u64,
        memory: MemoryTelemetrySnapshot,
    ) -> Self {
        Self {
            version: RAW_TELEMETRY_VERSION,
            vm_name: vm_name.into(),
            service_name: service_name.into(),
            session_id: session_id.into(),
            observed_unix_millis,
            monotonic_millis,
            sequence,
            telemetry_source: TelemetrySource::WindowsNativeMemoryApis,
            allocation_provenance: AllocationProvenance::HostLiveLibvirtCurrentRequired,
            memory,
        }
    }

    pub fn validate_for(
        &self,
        expected_vm_name: &str,
        expected_service_name: &str,
        now_unix_millis: u64,
        max_age_millis: u64,
        future_tolerance_millis: u64,
    ) -> Result<(), DemandError> {
        if self.version != RAW_TELEMETRY_VERSION {
            return Err(DemandError::UnsupportedTelemetryVersion(self.version));
        }
        for (field, value) in [
            ("vm_name", self.vm_name.as_str()),
            ("service_name", self.service_name.as_str()),
            ("session_id", self.session_id.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 128 || !value.is_ascii() {
                return Err(DemandError::InvalidTelemetryIdentity(field));
            }
        }
        if self.vm_name != expected_vm_name {
            return Err(DemandError::WrongTelemetryVm {
                expected: expected_vm_name.to_owned(),
                actual: self.vm_name.clone(),
            });
        }
        if self.service_name != expected_service_name {
            return Err(DemandError::WrongTelemetryService {
                expected: expected_service_name.to_owned(),
                actual: self.service_name.clone(),
            });
        }
        if max_age_millis == 0 {
            return Err(DemandError::InvalidTelemetryFreshness);
        }
        if self.observed_unix_millis > now_unix_millis.saturating_add(future_tolerance_millis) {
            return Err(DemandError::FutureTelemetry {
                observed: self.observed_unix_millis,
                now: now_unix_millis,
            });
        }
        if now_unix_millis.saturating_sub(self.observed_unix_millis) > max_age_millis {
            return Err(DemandError::StaleTelemetry {
                observed: self.observed_unix_millis,
                now: now_unix_millis,
            });
        }
        self.memory.validate()
    }

    /// Validates ordering against the last accepted record from this producer.
    pub fn validate_successor(&self, previous: &Self) -> Result<(), DemandError> {
        if self.vm_name != previous.vm_name || self.service_name != previous.service_name {
            return Err(DemandError::ChangedTelemetryIdentity);
        }
        if self.session_id == previous.session_id {
            if self.sequence <= previous.sequence {
                return Err(DemandError::ReplayedTelemetrySequence(self.sequence));
            }
            if self.monotonic_millis <= previous.monotonic_millis {
                return Err(DemandError::NonIncreasingTelemetryMonotonicClock(
                    self.monotonic_millis,
                ));
            }
        } else if self.sequence != 0 {
            return Err(DemandError::InvalidTelemetrySessionStart(self.sequence));
        }
        if self.observed_unix_millis < previous.observed_unix_millis {
            return Err(DemandError::TelemetryWallClockMovedBackwards);
        }
        Ok(())
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
    #[error("unsupported raw telemetry version: {0}")]
    UnsupportedTelemetryVersion(u16),
    #[error("raw telemetry identity field is invalid: {0}")]
    InvalidTelemetryIdentity(&'static str),
    #[error("raw telemetry belongs to VM {actual}, expected {expected}")]
    WrongTelemetryVm { expected: String, actual: String },
    #[error("raw telemetry belongs to service {actual}, expected {expected}")]
    WrongTelemetryService { expected: String, actual: String },
    #[error("raw telemetry maximum age must be greater than zero")]
    InvalidTelemetryFreshness,
    #[error("raw telemetry observation {observed} is stale at {now}")]
    StaleTelemetry { observed: u64, now: u64 },
    #[error("raw telemetry observation {observed} is in the future at {now}")]
    FutureTelemetry { observed: u64, now: u64 },
    #[error("raw telemetry VM or service identity changed")]
    ChangedTelemetryIdentity,
    #[error("raw telemetry sequence is replayed or non-increasing: {0}")]
    ReplayedTelemetrySequence(u64),
    #[error("raw telemetry monotonic clock is non-increasing: {0}")]
    NonIncreasingTelemetryMonotonicClock(u64),
    #[error("new raw telemetry session must begin at sequence zero, got {0}")]
    InvalidTelemetrySessionStart(u64),
    #[error("raw telemetry wall clock moved backwards across records")]
    TelemetryWallClockMovedBackwards,
}

/// Bounds and alignment used by the advisory target calculator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemandPolicyConfig {
    pub configured_minimum_bytes: u64,
    pub configured_maximum_bytes: u64,
    pub block_size_bytes: u64,
    pub grow_step_bytes: u64,
    pub shrink_step_bytes: u64,
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
        if self.grow_step_bytes == 0
            || self.shrink_step_bytes == 0
            || !self.grow_step_bytes.is_multiple_of(self.block_size_bytes)
            || !self.shrink_step_bytes.is_multiple_of(self.block_size_bytes)
        {
            return Err(DemandError::InvalidPolicy(
                "grow and shrink steps must be positive block-aligned values",
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
        if !current_bytes.is_multiple_of(self.config.block_size_bytes) {
            return Err(DemandError::InvalidPolicy(
                "current allocation must be block aligned",
            ));
        }

        let physical_pressure = snapshot.physical_pressure()?;
        let commit_pressure = snapshot.commit_pressure()?;
        let pressure = physical_pressure.max(commit_pressure);
        let state = classify_pressure(pressure);
        let desired_target_bytes = if current_bytes < self.config.configured_minimum_bytes {
            self.config.configured_minimum_bytes
        } else {
            directional_target(current_bytes, state, self.config)?
        };
        let safe_floor_bytes = current_bytes
            .saturating_sub(self.config.shrink_step_bytes)
            .clamp(
                self.config.configured_minimum_bytes,
                self.config.configured_maximum_bytes,
            );

        Ok(DemandReport {
            version: DEMAND_REPORT_VERSION,
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

fn directional_target(
    current_bytes: u64,
    state: DemandState,
    config: DemandPolicyConfig,
) -> Result<u64, DemandError> {
    let target = match state {
        DemandState::Release => current_bytes.saturating_sub(config.shrink_step_bytes),
        DemandState::Stable => current_bytes,
        DemandState::WantMore | DemandState::Pressure | DemandState::Critical => current_bytes
            .checked_add(config.grow_step_bytes)
            .ok_or(DemandError::ArithmeticOverflow)?,
    };
    Ok(target.clamp(
        config.configured_minimum_bytes,
        config.configured_maximum_bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn snapshot() -> MemoryTelemetrySnapshot {
        MemoryTelemetrySnapshot {
            physical_total_bytes: 16 * GIB,
            physical_available_bytes: 2 * GIB,
            memory_load_percent: 88,
            commit_total_bytes: 15 * GIB,
            commit_limit_bytes: 16 * GIB,
            commit_peak_bytes: 15 * GIB,
            system_cache_bytes: 0,
            kernel_paged_bytes: 0,
            kernel_nonpaged_bytes: 0,
        }
    }

    #[test]
    fn validates_fresh_vm_scoped_raw_telemetry() {
        let envelope = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            995_000,
            10,
            0,
            snapshot(),
        );
        assert_eq!(
            envelope.validate_for("guest", "VirtioMemService", 1_000_000, 60_000, 5_000),
            Ok(())
        );
        assert!(envelope
            .validate_for("other", "VirtioMemService", 1_000_000, 60_000, 5_000)
            .is_err());
        assert!(envelope
            .validate_for("guest", "other-service", 1_000_000, 60_000, 5_000)
            .is_err());
        assert!(envelope
            .validate_for("guest", "VirtioMemService", 1_056_000, 60_000, 5_000)
            .is_err());
        assert!(RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_006_000,
            20,
            1,
            snapshot(),
        )
        .validate_for("guest", "VirtioMemService", 1_000_000, 60_000, 5_000)
        .is_err());

        let mut unsupported = envelope.clone();
        unsupported.version = 1;
        assert_eq!(
            unsupported.validate_for("guest", "VirtioMemService", 1_000_000, 60_000, 5_000),
            Err(DemandError::UnsupportedTelemetryVersion(1))
        );

        let mut missing_provenance = serde_json::to_value(&envelope).expect("encode envelope");
        missing_provenance
            .as_object_mut()
            .expect("envelope is an object")
            .remove("allocation_provenance");
        assert!(serde_json::from_value::<RawTelemetryEnvelope>(missing_provenance).is_err());
    }

    #[test]
    fn validates_sequence_and_session_ordering() {
        let first = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000,
            10,
            0,
            snapshot(),
        );
        let next = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_001,
            20,
            1,
            snapshot(),
        );
        assert_eq!(next.validate_successor(&first), Ok(()));
        assert_eq!(
            first.validate_successor(&first),
            Err(DemandError::ReplayedTelemetrySequence(0))
        );

        let restarted = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-b",
            1_002,
            1,
            0,
            snapshot(),
        );
        assert_eq!(restarted.validate_successor(&next), Ok(()));
        let mut invalid_restart = restarted.clone();
        invalid_restart.sequence = 1;
        assert_eq!(
            invalid_restart.validate_successor(&next),
            Err(DemandError::InvalidTelemetrySessionStart(1))
        );
    }

    #[test]
    fn calculates_from_zero_live_allocation_without_guessing() {
        let calculator = DemandCalculator::new(DemandPolicyConfig {
            configured_minimum_bytes: 4 * GIB,
            configured_maximum_bytes: 32 * GIB,
            block_size_bytes: 2 * GIB,
            grow_step_bytes: 2 * GIB,
            shrink_step_bytes: 2 * GIB,
        })
        .expect("valid policy");
        let report = calculator
            .calculate(snapshot(), 0)
            .expect("zero is live state");
        assert_eq!(report.demand.desired_target_bytes, 4 * GIB);
        assert_eq!(report.demand.safe_floor_bytes, 4 * GIB);
    }

    #[test]
    fn uses_one_gibibyte_growth_and_64_mibibyte_reclaim_quanta() {
        const MIB: u64 = 1024 * 1024;
        let calculator = DemandCalculator::new(DemandPolicyConfig {
            configured_minimum_bytes: 4 * GIB,
            configured_maximum_bytes: 32 * GIB,
            block_size_bytes: 2 * MIB,
            grow_step_bytes: GIB,
            shrink_step_bytes: 64 * MIB,
        })
        .expect("valid asymmetric policy");

        let grow = calculator
            .calculate(snapshot(), 16 * GIB)
            .expect("critical pressure grows once");
        assert_eq!(grow.demand.state, DemandState::Critical);
        assert_eq!(grow.demand.desired_target_bytes, 17 * GIB);

        let mut release_snapshot = snapshot();
        release_snapshot.physical_available_bytes = 15 * GIB;
        release_snapshot.memory_load_percent = 6;
        release_snapshot.commit_total_bytes = GIB;
        release_snapshot.commit_peak_bytes = GIB;
        let release = calculator
            .calculate(release_snapshot, 16 * GIB)
            .expect("low pressure releases once");
        assert_eq!(release.demand.state, DemandState::Release);
        assert_eq!(release.demand.desired_target_bytes, 16 * GIB - 64 * MIB);
        assert_eq!(release.demand.safe_floor_bytes, 16 * GIB - 64 * MIB);

        let invalid = DemandPolicyConfig {
            shrink_step_bytes: 63 * MIB,
            ..calculator.config
        };
        assert!(DemandCalculator::new(invalid).is_err());
    }
}
