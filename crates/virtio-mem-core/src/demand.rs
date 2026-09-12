use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LEGACY_RAW_TELEMETRY_VERSION: u16 = 2;
pub const RAW_TELEMETRY_VERSION: u16 = 3;
pub const WINDOWS_NATIVE_TELEMETRY_VERSION: u16 = 1;
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

/// Whether a supported Windows facility can produce a telemetry group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryCapability {
    Supported,
    Unavailable,
}

/// Fixed capability set negotiated with a schema-v3 producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsTelemetryCapabilities {
    pub memory_resource_notifications: TelemetryCapability,
    pub reusable_memory: TelemetryCapability,
    pub paging_activity: TelemetryCapability,
}

/// Availability of one optional signal group in this observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetrySignalStatus {
    Supported,
    Unavailable,
    Failed,
    Warming,
}

/// Bounded progress for a rate signal that requires multiple observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryWarmup {
    pub samples_collected: u16,
    pub samples_required: u16,
}

/// An optional signal with explicit availability, failure, and warm-up state.
///
/// The representation is fixed-size for allocation-free producer updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionalTelemetrySignal<T> {
    pub status: TelemetrySignalStatus,
    pub observed_monotonic_millis: u64,
    pub value: Option<T>,
    pub error_code: Option<u32>,
    pub warmup: Option<TelemetryWarmup>,
}

impl<T> OptionalTelemetrySignal<T> {
    pub fn supported(observed_monotonic_millis: u64, value: T) -> Self {
        Self {
            status: TelemetrySignalStatus::Supported,
            observed_monotonic_millis,
            value: Some(value),
            error_code: None,
            warmup: None,
        }
    }

    pub fn unavailable(observed_monotonic_millis: u64) -> Self {
        Self {
            status: TelemetrySignalStatus::Unavailable,
            observed_monotonic_millis,
            value: None,
            error_code: None,
            warmup: None,
        }
    }

    pub fn failed(observed_monotonic_millis: u64, error_code: u32) -> Self {
        Self {
            status: TelemetrySignalStatus::Failed,
            observed_monotonic_millis,
            value: None,
            error_code: Some(error_code),
            warmup: None,
        }
    }

    pub fn warming(
        observed_monotonic_millis: u64,
        samples_collected: u16,
        samples_required: u16,
    ) -> Self {
        Self {
            status: TelemetrySignalStatus::Warming,
            observed_monotonic_millis,
            value: None,
            error_code: None,
            warmup: Some(TelemetryWarmup {
                samples_collected,
                samples_required,
            }),
        }
    }

    fn validate(&self, capability: TelemetryCapability) -> Result<(), DemandError> {
        let shape_is_valid = match self.status {
            TelemetrySignalStatus::Supported => {
                self.value.is_some() && self.error_code.is_none() && self.warmup.is_none()
            }
            TelemetrySignalStatus::Unavailable => {
                self.value.is_none() && self.error_code.is_none() && self.warmup.is_none()
            }
            TelemetrySignalStatus::Failed => {
                self.value.is_none()
                    && self.error_code.is_some_and(|error_code| error_code != 0)
                    && self.warmup.is_none()
            }
            TelemetrySignalStatus::Warming => {
                self.value.is_none()
                    && self.error_code.is_none()
                    && self.warmup.is_some_and(|warmup| {
                        warmup.samples_required > 0
                            && warmup.samples_collected < warmup.samples_required
                    })
            }
        };
        if !shape_is_valid {
            return Err(DemandError::InvalidTelemetrySignalState);
        }
        match capability {
            TelemetryCapability::Supported if self.status != TelemetrySignalStatus::Unavailable => {
                Ok(())
            }
            TelemetryCapability::Unavailable
                if self.status == TelemetrySignalStatus::Unavailable =>
            {
                Ok(())
            }
            _ => Err(DemandError::TelemetryCapabilityMismatch),
        }
    }
}

/// Categorical state of the paired low/high Windows memory notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryResourceNotificationState {
    Low,
    Neutral,
    High,
}

/// Snapshot detail that distinguishes immediately reusable and modified pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReusableMemorySnapshot {
    pub standby_reserve_bytes: u64,
    pub free_zero_bytes: u64,
    pub modified_bytes: u64,
}

/// Optional formatted rate evidence. Rates are scaled to milli-events/second
/// so the wire representation remains deterministic and integer-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PagingActivitySnapshot {
    pub sample_interval_millis: u64,
    pub pages_output_milli_events_per_second: u64,
    pub page_reads_milli_events_per_second: u64,
    pub pages_input_milli_events_per_second: u64,
    pub hard_faults_milli_events_per_second: u64,
}

/// Additive Windows-native telemetry carried by schema v3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsNativeTelemetry {
    pub version: u16,
    pub capabilities: WindowsTelemetryCapabilities,
    pub memory_resource_notifications: OptionalTelemetrySignal<MemoryResourceNotificationState>,
    pub reusable_memory: OptionalTelemetrySignal<ReusableMemorySnapshot>,
    pub paging_activity: OptionalTelemetrySignal<PagingActivitySnapshot>,
}

impl WindowsNativeTelemetry {
    /// Explicit fallback emitted until WN2 adds native signal collection.
    pub fn unavailable(observed_monotonic_millis: u64) -> Self {
        Self {
            version: WINDOWS_NATIVE_TELEMETRY_VERSION,
            capabilities: WindowsTelemetryCapabilities {
                memory_resource_notifications: TelemetryCapability::Unavailable,
                reusable_memory: TelemetryCapability::Unavailable,
                paging_activity: TelemetryCapability::Unavailable,
            },
            memory_resource_notifications: OptionalTelemetrySignal::unavailable(
                observed_monotonic_millis,
            ),
            reusable_memory: OptionalTelemetrySignal::unavailable(observed_monotonic_millis),
            paging_activity: OptionalTelemetrySignal::unavailable(observed_monotonic_millis),
        }
    }

    fn validate(
        &self,
        envelope_monotonic_millis: u64,
        memory: &MemoryTelemetrySnapshot,
    ) -> Result<(), DemandError> {
        if self.version != WINDOWS_NATIVE_TELEMETRY_VERSION {
            return Err(DemandError::UnsupportedWindowsNativeTelemetryVersion(
                self.version,
            ));
        }
        self.memory_resource_notifications
            .validate(self.capabilities.memory_resource_notifications)?;
        self.reusable_memory
            .validate(self.capabilities.reusable_memory)?;
        self.paging_activity
            .validate(self.capabilities.paging_activity)?;

        for observed in [
            self.memory_resource_notifications.observed_monotonic_millis,
            self.reusable_memory.observed_monotonic_millis,
            self.paging_activity.observed_monotonic_millis,
        ] {
            if observed > envelope_monotonic_millis {
                return Err(DemandError::TelemetrySignalFromFuture);
            }
        }
        if let Some(reusable) = self.reusable_memory.value {
            let immediately_reusable = reusable
                .standby_reserve_bytes
                .checked_add(reusable.free_zero_bytes)
                .ok_or(DemandError::ArithmeticOverflow)?;
            if immediately_reusable > memory.physical_available_bytes
                || reusable.modified_bytes > memory.physical_total_bytes
            {
                return Err(DemandError::InconsistentOptionalTelemetry);
            }
        }
        if self
            .paging_activity
            .value
            .is_some_and(|paging| paging.sample_interval_millis == 0)
        {
            return Err(DemandError::InvalidTelemetrySampleInterval);
        }
        Ok(())
    }

    fn has_native_pressure_state(&self) -> bool {
        self.memory_resource_notifications.status == TelemetrySignalStatus::Supported
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawTelemetryContractMode {
    LegacyV2Fallback,
    WindowsNativeFallback,
    WindowsNativeSignals,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows_native: Option<WindowsNativeTelemetry>,
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
            windows_native: Some(WindowsNativeTelemetry::unavailable(monotonic_millis)),
        }
    }

    pub fn contract_mode(&self) -> Result<RawTelemetryContractMode, DemandError> {
        match (self.version, self.windows_native.as_ref()) {
            (LEGACY_RAW_TELEMETRY_VERSION, None) => Ok(RawTelemetryContractMode::LegacyV2Fallback),
            (RAW_TELEMETRY_VERSION, Some(windows_native)) => {
                windows_native.validate(self.monotonic_millis, &self.memory)?;
                if windows_native.has_native_pressure_state() {
                    Ok(RawTelemetryContractMode::WindowsNativeSignals)
                } else {
                    Ok(RawTelemetryContractMode::WindowsNativeFallback)
                }
            }
            (LEGACY_RAW_TELEMETRY_VERSION, Some(_)) | (RAW_TELEMETRY_VERSION, None) => {
                Err(DemandError::TelemetrySchemaMismatch)
            }
            (version, _) => Err(DemandError::UnsupportedTelemetryVersion(version)),
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
        self.contract_mode()?;
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
            let contract_changed = self.version != previous.version
                || self
                    .windows_native
                    .as_ref()
                    .map(|native| (native.version, native.capabilities))
                    != previous
                        .windows_native
                        .as_ref()
                        .map(|native| (native.version, native.capabilities));
            if contract_changed {
                return Err(DemandError::ChangedTelemetryContract);
            }
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
    #[error("unsupported Windows-native telemetry version: {0}")]
    UnsupportedWindowsNativeTelemetryVersion(u16),
    #[error("raw telemetry schema version and Windows-native extension do not match")]
    TelemetrySchemaMismatch,
    #[error("optional telemetry signal fields contradict its status")]
    InvalidTelemetrySignalState,
    #[error("optional telemetry signal state contradicts its declared capability")]
    TelemetryCapabilityMismatch,
    #[error("optional telemetry signal timestamp exceeds the envelope monotonic timestamp")]
    TelemetrySignalFromFuture,
    #[error("optional telemetry counters contradict the basic memory snapshot")]
    InconsistentOptionalTelemetry,
    #[error("optional telemetry rate sample interval must be greater than zero")]
    InvalidTelemetrySampleInterval,
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
    #[error("raw telemetry schema or capabilities changed within one producer session")]
    ChangedTelemetryContract,
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

    fn windows_native_fixture() -> WindowsNativeTelemetry {
        WindowsNativeTelemetry {
            version: WINDOWS_NATIVE_TELEMETRY_VERSION,
            capabilities: WindowsTelemetryCapabilities {
                memory_resource_notifications: TelemetryCapability::Supported,
                reusable_memory: TelemetryCapability::Supported,
                paging_activity: TelemetryCapability::Supported,
            },
            memory_resource_notifications: OptionalTelemetrySignal::supported(
                9,
                MemoryResourceNotificationState::Neutral,
            ),
            reusable_memory: OptionalTelemetrySignal::supported(
                9,
                ReusableMemorySnapshot {
                    standby_reserve_bytes: GIB,
                    free_zero_bytes: GIB,
                    modified_bytes: GIB,
                },
            ),
            paging_activity: OptionalTelemetrySignal::supported(
                9,
                PagingActivitySnapshot {
                    sample_interval_millis: 1_000,
                    pages_output_milli_events_per_second: 0,
                    page_reads_milli_events_per_second: 1_000,
                    pages_input_milli_events_per_second: 1_000,
                    hard_faults_milli_events_per_second: 2_000,
                },
            ),
        }
    }

    #[test]
    fn round_trips_current_and_legacy_contract_modes() {
        let mut current = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000,
            10,
            0,
            snapshot(),
        );
        current.windows_native = Some(windows_native_fixture());
        let encoded = serde_json::to_string(&current).expect("encode current telemetry");
        let decoded: RawTelemetryEnvelope =
            serde_json::from_str(&encoded).expect("decode current telemetry");
        assert_eq!(decoded, current);
        assert_eq!(
            decoded.contract_mode(),
            Ok(RawTelemetryContractMode::WindowsNativeSignals)
        );

        let mut legacy = current.clone();
        legacy.version = LEGACY_RAW_TELEMETRY_VERSION;
        legacy.windows_native = None;
        let encoded = serde_json::to_string(&legacy).expect("encode legacy telemetry");
        let decoded: RawTelemetryEnvelope =
            serde_json::from_str(&encoded).expect("decode legacy telemetry");
        assert_eq!(
            decoded.contract_mode(),
            Ok(RawTelemetryContractMode::LegacyV2Fallback)
        );
        let encoded_value: serde_json::Value =
            serde_json::from_str(&encoded).expect("decode legacy JSON value");
        assert!(encoded_value.get("windows_native").is_none());
    }

    #[test]
    fn rejects_missing_new_extension_and_newer_schemas() {
        let mut missing = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000,
            10,
            0,
            snapshot(),
        );
        missing.windows_native = None;
        assert_eq!(
            missing.contract_mode(),
            Err(DemandError::TelemetrySchemaMismatch)
        );

        let mut newer = missing;
        newer.version = RAW_TELEMETRY_VERSION + 1;
        assert_eq!(
            newer.contract_mode(),
            Err(DemandError::UnsupportedTelemetryVersion(
                RAW_TELEMETRY_VERSION + 1
            ))
        );
    }

    #[test]
    fn validates_every_optional_signal_state() {
        let supported =
            OptionalTelemetrySignal::supported(1, MemoryResourceNotificationState::High);
        assert_eq!(supported.validate(TelemetryCapability::Supported), Ok(()));
        let unavailable =
            OptionalTelemetrySignal::<MemoryResourceNotificationState>::unavailable(1);
        assert_eq!(
            unavailable.validate(TelemetryCapability::Unavailable),
            Ok(())
        );
        let failed = OptionalTelemetrySignal::<MemoryResourceNotificationState>::failed(1, 5);
        assert_eq!(failed.validate(TelemetryCapability::Supported), Ok(()));
        let warming = OptionalTelemetrySignal::<MemoryResourceNotificationState>::warming(1, 1, 2);
        assert_eq!(warming.validate(TelemetryCapability::Supported), Ok(()));

        for fixture in [supported, unavailable, failed, warming] {
            let encoded = serde_json::to_string(&fixture).expect("encode signal state");
            assert_eq!(
                serde_json::from_str::<OptionalTelemetrySignal<MemoryResourceNotificationState>>(
                    &encoded
                )
                .expect("decode signal state"),
                fixture
            );
        }
    }

    #[test]
    fn capability_negotiation_is_stable_within_a_session() {
        let first = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000,
            10,
            0,
            snapshot(),
        );
        let mut changed = first.clone();
        changed.observed_unix_millis += 1;
        changed.monotonic_millis += 1;
        changed.sequence += 1;
        changed
            .windows_native
            .as_mut()
            .expect("current extension")
            .capabilities
            .memory_resource_notifications = TelemetryCapability::Supported;
        changed
            .windows_native
            .as_mut()
            .expect("current extension")
            .memory_resource_notifications = OptionalTelemetrySignal::failed(11, 5);
        assert_eq!(
            changed.validate_successor(&first),
            Err(DemandError::ChangedTelemetryContract)
        );

        changed.session_id = "session-b".to_owned();
        changed.sequence = 0;
        assert_eq!(changed.validate_successor(&first), Ok(()));
    }

    #[test]
    fn rejects_malformed_contradictory_and_oversized_optional_evidence() {
        let mut envelope = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000,
            10,
            0,
            snapshot(),
        );
        let mut rich = windows_native_fixture();
        rich.memory_resource_notifications.error_code = Some(5);
        envelope.windows_native = Some(rich);
        assert_eq!(
            envelope.contract_mode(),
            Err(DemandError::InvalidTelemetrySignalState)
        );

        let mut rich = windows_native_fixture();
        rich.capabilities.memory_resource_notifications = TelemetryCapability::Unavailable;
        envelope.windows_native = Some(rich);
        assert_eq!(
            envelope.contract_mode(),
            Err(DemandError::TelemetryCapabilityMismatch)
        );

        let mut rich = windows_native_fixture();
        rich.reusable_memory = OptionalTelemetrySignal::supported(
            9,
            ReusableMemorySnapshot {
                standby_reserve_bytes: u64::MAX,
                free_zero_bytes: 1,
                modified_bytes: 0,
            },
        );
        envelope.windows_native = Some(rich);
        assert_eq!(
            envelope.contract_mode(),
            Err(DemandError::ArithmeticOverflow)
        );

        let mut malformed = serde_json::to_value(windows_native_fixture()).expect("encode fixture");
        malformed
            .as_object_mut()
            .expect("object")
            .insert("unknown".to_owned(), serde_json::json!(true));
        assert!(serde_json::from_value::<WindowsNativeTelemetry>(malformed).is_err());
    }

    #[test]
    fn rejects_bad_warmup_timestamp_and_rate_interval() {
        let mut envelope = RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000,
            10,
            0,
            snapshot(),
        );
        let mut rich = windows_native_fixture();
        rich.paging_activity = OptionalTelemetrySignal::warming(9, 2, 2);
        envelope.windows_native = Some(rich);
        assert_eq!(
            envelope.contract_mode(),
            Err(DemandError::InvalidTelemetrySignalState)
        );

        let mut rich = windows_native_fixture();
        rich.reusable_memory.observed_monotonic_millis = 11;
        envelope.windows_native = Some(rich);
        assert_eq!(
            envelope.contract_mode(),
            Err(DemandError::TelemetrySignalFromFuture)
        );

        let mut rich = windows_native_fixture();
        rich.paging_activity
            .value
            .as_mut()
            .expect("paging value")
            .sample_interval_millis = 0;
        envelope.windows_native = Some(rich);
        assert_eq!(
            envelope.contract_mode(),
            Err(DemandError::InvalidTelemetrySampleInterval)
        );
    }

    #[test]
    fn windows_native_extension_has_no_heap_owned_fields() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<WindowsNativeTelemetry>();
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
