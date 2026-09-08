use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const RAW_TELEMETRY_VERSION: u16 = 1;
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

/// The minimal M10c transport record. M10d extends this contract with service
/// session identity, monotonic ordering, sequence, and allocation provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTelemetryEnvelope {
    pub version: u16,
    pub vm_name: String,
    pub observed_unix_seconds: u64,
    pub memory: MemoryTelemetrySnapshot,
}

impl RawTelemetryEnvelope {
    pub fn new(
        vm_name: impl Into<String>,
        observed_unix_seconds: u64,
        memory: MemoryTelemetrySnapshot,
    ) -> Self {
        Self {
            version: RAW_TELEMETRY_VERSION,
            vm_name: vm_name.into(),
            observed_unix_seconds,
            memory,
        }
    }

    pub fn validate_for(
        &self,
        expected_vm_name: &str,
        now_unix_seconds: u64,
        max_age_seconds: u64,
        future_tolerance_seconds: u64,
    ) -> Result<(), DemandError> {
        if self.version != RAW_TELEMETRY_VERSION {
            return Err(DemandError::UnsupportedTelemetryVersion(self.version));
        }
        if self.vm_name.trim().is_empty() {
            return Err(DemandError::InvalidTelemetryIdentity);
        }
        if self.vm_name != expected_vm_name {
            return Err(DemandError::WrongTelemetryVm {
                expected: expected_vm_name.to_owned(),
                actual: self.vm_name.clone(),
            });
        }
        if max_age_seconds == 0 {
            return Err(DemandError::InvalidTelemetryFreshness);
        }
        if self.observed_unix_seconds > now_unix_seconds.saturating_add(future_tolerance_seconds) {
            return Err(DemandError::FutureTelemetry {
                observed: self.observed_unix_seconds,
                now: now_unix_seconds,
            });
        }
        if now_unix_seconds.saturating_sub(self.observed_unix_seconds) > max_age_seconds {
            return Err(DemandError::StaleTelemetry {
                observed: self.observed_unix_seconds,
                now: now_unix_seconds,
            });
        }
        self.memory.validate()
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
    #[error("raw telemetry VM identity is empty")]
    InvalidTelemetryIdentity,
    #[error("raw telemetry belongs to VM {actual}, expected {expected}")]
    WrongTelemetryVm { expected: String, actual: String },
    #[error("raw telemetry maximum age must be greater than zero")]
    InvalidTelemetryFreshness,
    #[error("raw telemetry observation {observed} is stale at {now}")]
    StaleTelemetry { observed: u64, now: u64 },
    #[error("raw telemetry observation {observed} is in the future at {now}")]
    FutureTelemetry { observed: u64, now: u64 },
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
        if !current_bytes.is_multiple_of(self.config.block_size_bytes) {
            return Err(DemandError::InvalidPolicy(
                "current allocation must be block aligned",
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
        let desired_target_bytes = if current_bytes < self.config.configured_minimum_bytes {
            self.config.configured_minimum_bytes
        } else {
            aligned_target(
                current_bytes,
                desired_steps,
                self.config.block_size_bytes,
                self.config.configured_minimum_bytes,
                self.config.configured_maximum_bytes,
            )?
        };
        let safe_floor_bytes = aligned_target(
            current_bytes,
            -1,
            self.config.block_size_bytes,
            self.config.configured_minimum_bytes,
            self.config.configured_maximum_bytes,
        )?;

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
        let envelope = RawTelemetryEnvelope::new("guest", 995, snapshot());
        assert_eq!(envelope.validate_for("guest", 1_000, 60, 5), Ok(()));
        assert!(envelope.validate_for("other", 1_000, 60, 5).is_err());
        assert!(envelope.validate_for("guest", 1_056, 60, 5).is_err());
        assert!(RawTelemetryEnvelope::new("guest", 1_006, snapshot())
            .validate_for("guest", 1_000, 60, 5)
            .is_err());
    }

    #[test]
    fn calculates_from_zero_live_allocation_without_guessing() {
        let calculator = DemandCalculator::new(DemandPolicyConfig {
            configured_minimum_bytes: 4 * GIB,
            configured_maximum_bytes: 32 * GIB,
            block_size_bytes: 2 * GIB,
        })
        .expect("valid policy");
        let report = calculator
            .calculate(snapshot(), 0)
            .expect("zero is live state");
        assert_eq!(report.demand.desired_target_bytes, 4 * GIB);
        assert_eq!(report.demand.safe_floor_bytes, 4 * GIB);
    }
}
