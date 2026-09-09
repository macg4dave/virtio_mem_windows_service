use std::env;
use std::time::Duration;

use thiserror::Error;

pub const DEFAULT_GROW_STEP_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_SHRINK_STEP_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_PHYSICAL_RESERVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_COMMIT_RESERVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_SAFE_FLOOR_PHYSICAL_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_SAFE_FLOOR_COMMIT_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_RECLAIM_HISTORY_SECONDS: u64 = 600;
pub const DEFAULT_DOWNWARD_HYSTERESIS_BYTES: u64 = 256 * 1024 * 1024;
/// Automatic reclaim is a core product capability unless explicitly paused.
pub const DEFAULT_AUTOMATIC_WINDOWS_SHRINK: bool = true;
/// Same-target re-notification remains an explicitly selected diagnostic mode.
pub const DEFAULT_SHRINK_RENOTIFICATION: bool = false;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostConfigError {
    #[error("required environment variable is missing or empty: {0}")]
    Missing(&'static str),
    #[error("environment variable {name} must be a positive decimal integer: {value}")]
    InvalidPositiveInteger { name: &'static str, value: String },
    #[error("environment variable {name} must be an unsigned decimal integer: {value}")]
    InvalidUnsignedInteger { name: &'static str, value: String },
    #[error("VIRTIO_MEM_ALIAS contains unsupported characters")]
    InvalidAlias,
    #[error("lower threshold must not exceed upper threshold")]
    InvalidThresholdOrder,
    #[error("minimum memory must not exceed maximum memory")]
    InvalidMemoryRange,
    #[error("grow and shrink steps must be greater than zero")]
    InvalidResizeStep,
    #[error("host-controller durations must be greater than zero")]
    InvalidDuration,
    #[error("VIRTIO_MEM_STATS_SOURCE must be 'dommemstat' or 'qga': {0}")]
    InvalidStatsSource(String),
    #[error("VIRTIO_MEM_DEMAND_SOURCE must be 'raw' or 'guest-stats': {0}")]
    InvalidDemandSource(String),
    #[error("VIRTIO_MEM_COMPATIBILITY_ATTESTATION_PATH must be non-empty")]
    InvalidAttestationPath,
    #[error("VIRTIO_MEM_RAW_TELEMETRY_PATH must be non-empty")]
    InvalidRawTelemetryPath,
    #[error("VIRTIO_MEM_RAW_TELEMETRY_SERVICE_NAME must be non-empty")]
    InvalidRawTelemetryServiceName,
    #[error("VIRTIO_MEM_POLICY_STATE_PATH must be non-empty")]
    InvalidPolicyStatePath,
    #[error("safe-floor reserves must not exceed normal reserves")]
    InvalidReserveOrder,
    #[error("target policy configuration is invalid: {0}")]
    InvalidTargetPolicy(&'static str),
    #[error("environment variable {name} must be 'true' or 'false': {value}")]
    InvalidBoolean { name: &'static str, value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsSource {
    /// Balloon-driver-backed `virsh dommemstat`; does not require the guest
    /// agent to implement `guest-get-memory-stats`.
    DomMemStat,
    /// QEMU Guest Agent `guest-get-memory-stats`; requires a guest agent
    /// version that implements the command.
    Qga,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandSourceMode {
    Raw,
    GuestStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    pub vm_name: String,
    pub alias: String,
    pub min_memory_bytes: u64,
    pub max_memory_bytes: u64,
    pub lower_threshold_bytes: u64,
    pub upper_threshold_bytes: u64,
    pub grow_step_bytes: u64,
    pub shrink_step_bytes: u64,
    pub fixed_visible_base_bytes: u64,
    pub physical_reserve_bytes: u64,
    pub commit_reserve_bytes: u64,
    pub safe_floor_physical_reserve_bytes: u64,
    pub safe_floor_commit_reserve_bytes: u64,
    pub reclaim_history: Duration,
    pub reclaim_max_gap: Duration,
    pub downward_hysteresis_bytes: u64,
    pub policy_state_path: String,
    pub poll_interval: Duration,
    pub command_timeout: Duration,
    pub convergence_timeout: Duration,
    pub virsh_binary: String,
    pub stats_source: StatsSource,
    pub demand_source: DemandSourceMode,
    pub stats_max_age: Duration,
    pub stats_future_tolerance: Duration,
    pub raw_telemetry_path: String,
    pub raw_telemetry_service_name: String,
    pub raw_telemetry_max_age: Duration,
    pub raw_telemetry_future_tolerance: Duration,
    pub host_min_headroom_bytes: u64,
    pub compatibility_attestation_path: String,
    pub automatic_windows_shrink: bool,
    pub shrink_renotification: bool,
}

impl HostConfig {
    pub fn from_env() -> Result<Self, HostConfigError> {
        let vm_name = required("VIRTIO_MEM_VM_NAME")?;
        let alias = required("VIRTIO_MEM_ALIAS")?;
        let stats_source = match env::var("VIRTIO_MEM_STATS_SOURCE") {
            Ok(value) if value.eq_ignore_ascii_case("qga") => StatsSource::Qga,
            Ok(value) if value.eq_ignore_ascii_case("dommemstat") => StatsSource::DomMemStat,
            Ok(value) if value.trim().is_empty() => StatsSource::DomMemStat,
            Ok(other) => return Err(HostConfigError::InvalidStatsSource(other)),
            Err(_) => StatsSource::DomMemStat,
        };
        let demand_source = match env::var("VIRTIO_MEM_DEMAND_SOURCE") {
            Ok(value) if value.eq_ignore_ascii_case("raw") => DemandSourceMode::Raw,
            Ok(value) if value.eq_ignore_ascii_case("guest-stats") => DemandSourceMode::GuestStats,
            Ok(value) if value.trim().is_empty() => DemandSourceMode::Raw,
            Ok(other) => return Err(HostConfigError::InvalidDemandSource(other)),
            Err(_) => DemandSourceMode::Raw,
        };
        let poll_interval_seconds = positive("VIRTIO_MEM_POLL_INTERVAL_SECONDS")?;
        let default_maximum_gap = poll_interval_seconds.checked_mul(2).ok_or_else(|| {
            HostConfigError::InvalidPositiveInteger {
                name: "VIRTIO_MEM_RECLAIM_MAX_GAP_SECONDS",
                value: "derived value overflowed".to_owned(),
            }
        })?;
        let config = Self {
            vm_name,
            alias,
            min_memory_bytes: positive("VIRTIO_MEM_MIN_MEMORY_BYTES")?,
            max_memory_bytes: positive("VIRTIO_MEM_MAX_MEMORY_BYTES")?,
            lower_threshold_bytes: positive("VIRTIO_MEM_LOWER_THRESHOLD_BYTES")?,
            upper_threshold_bytes: positive("VIRTIO_MEM_UPPER_THRESHOLD_BYTES")?,
            grow_step_bytes: positive_or_default(
                "VIRTIO_MEM_GROW_STEP_BYTES",
                DEFAULT_GROW_STEP_BYTES,
            )?,
            shrink_step_bytes: positive_or_default(
                "VIRTIO_MEM_SHRINK_STEP_BYTES",
                DEFAULT_SHRINK_STEP_BYTES,
            )?,
            fixed_visible_base_bytes: unsigned("VIRTIO_MEM_FIXED_VISIBLE_BASE_BYTES")?,
            physical_reserve_bytes: positive_or_default(
                "VIRTIO_MEM_PHYSICAL_RESERVE_BYTES",
                DEFAULT_PHYSICAL_RESERVE_BYTES,
            )?,
            commit_reserve_bytes: positive_or_default(
                "VIRTIO_MEM_COMMIT_RESERVE_BYTES",
                DEFAULT_COMMIT_RESERVE_BYTES,
            )?,
            safe_floor_physical_reserve_bytes: positive_or_default(
                "VIRTIO_MEM_SAFE_FLOOR_PHYSICAL_RESERVE_BYTES",
                DEFAULT_SAFE_FLOOR_PHYSICAL_RESERVE_BYTES,
            )?,
            safe_floor_commit_reserve_bytes: positive_or_default(
                "VIRTIO_MEM_SAFE_FLOOR_COMMIT_RESERVE_BYTES",
                DEFAULT_SAFE_FLOOR_COMMIT_RESERVE_BYTES,
            )?,
            reclaim_history: Duration::from_secs(positive_or_default(
                "VIRTIO_MEM_RECLAIM_HISTORY_SECONDS",
                DEFAULT_RECLAIM_HISTORY_SECONDS,
            )?),
            reclaim_max_gap: Duration::from_secs(positive_or_default(
                "VIRTIO_MEM_RECLAIM_MAX_GAP_SECONDS",
                default_maximum_gap,
            )?),
            downward_hysteresis_bytes: positive_or_default(
                "VIRTIO_MEM_DOWNWARD_HYSTERESIS_BYTES",
                DEFAULT_DOWNWARD_HYSTERESIS_BYTES,
            )?,
            policy_state_path: required("VIRTIO_MEM_POLICY_STATE_PATH")?,
            poll_interval: Duration::from_secs(poll_interval_seconds),
            command_timeout: Duration::from_secs(positive("VIRTIO_MEM_COMMAND_TIMEOUT_SECONDS")?),
            convergence_timeout: Duration::from_secs(positive(
                "VIRTIO_MEM_CONVERGENCE_TIMEOUT_SECONDS",
            )?),
            virsh_binary: env::var("VIRTIO_MEM_VIRSH_BINARY")
                .unwrap_or_else(|_| "virsh".to_owned()),
            stats_source,
            demand_source,
            stats_max_age: Duration::from_secs(positive("VIRTIO_MEM_STATS_MAX_AGE_SECONDS")?),
            stats_future_tolerance: Duration::from_secs(positive(
                "VIRTIO_MEM_STATS_FUTURE_TOLERANCE_SECONDS",
            )?),
            raw_telemetry_path: required("VIRTIO_MEM_RAW_TELEMETRY_PATH")?,
            raw_telemetry_service_name: required("VIRTIO_MEM_RAW_TELEMETRY_SERVICE_NAME")?,
            raw_telemetry_max_age: Duration::from_secs(positive(
                "VIRTIO_MEM_RAW_TELEMETRY_MAX_AGE_SECONDS",
            )?),
            raw_telemetry_future_tolerance: Duration::from_secs(positive(
                "VIRTIO_MEM_RAW_TELEMETRY_FUTURE_TOLERANCE_SECONDS",
            )?),
            host_min_headroom_bytes: positive("VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES")?,
            compatibility_attestation_path: required("VIRTIO_MEM_COMPATIBILITY_ATTESTATION_PATH")?,
            automatic_windows_shrink: optional_bool(
                "VIRTIO_MEM_AUTOMATIC_WINDOWS_SHRINK",
                DEFAULT_AUTOMATIC_WINDOWS_SHRINK,
            )?,
            shrink_renotification: optional_bool(
                "VIRTIO_MEM_SHRINK_RENOTIFICATION",
                DEFAULT_SHRINK_RENOTIFICATION,
            )?,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), HostConfigError> {
        if self.vm_name.trim().is_empty() {
            return Err(HostConfigError::Missing("VIRTIO_MEM_VM_NAME"));
        }
        if self.alias.is_empty()
            || !self
                .alias
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        {
            return Err(HostConfigError::InvalidAlias);
        }
        if self.lower_threshold_bytes > self.upper_threshold_bytes {
            return Err(HostConfigError::InvalidThresholdOrder);
        }
        if self.min_memory_bytes > self.max_memory_bytes {
            return Err(HostConfigError::InvalidMemoryRange);
        }
        if self.grow_step_bytes == 0 || self.shrink_step_bytes == 0 {
            return Err(HostConfigError::InvalidResizeStep);
        }
        if self.physical_reserve_bytes == 0
            || self.commit_reserve_bytes == 0
            || self.safe_floor_physical_reserve_bytes == 0
            || self.safe_floor_commit_reserve_bytes == 0
            || self.downward_hysteresis_bytes == 0
        {
            return Err(HostConfigError::InvalidTargetPolicy(
                "reserves and hysteresis must be positive",
            ));
        }
        if self.safe_floor_physical_reserve_bytes > self.physical_reserve_bytes
            || self.safe_floor_commit_reserve_bytes > self.commit_reserve_bytes
        {
            return Err(HostConfigError::InvalidReserveOrder);
        }
        if self.poll_interval.is_zero()
            || self.command_timeout.is_zero()
            || self.convergence_timeout.is_zero()
            || self.stats_max_age.is_zero()
            || self.stats_future_tolerance.is_zero()
            || self.raw_telemetry_max_age.is_zero()
            || self.raw_telemetry_future_tolerance.is_zero()
            || self.reclaim_history.is_zero()
            || self.reclaim_max_gap.is_zero()
        {
            return Err(HostConfigError::InvalidDuration);
        }
        if self.reclaim_max_gap > self.reclaim_history {
            return Err(HostConfigError::InvalidTargetPolicy(
                "reclaim maximum gap exceeds history window",
            ));
        }
        if self.compatibility_attestation_path.trim().is_empty() {
            return Err(HostConfigError::InvalidAttestationPath);
        }
        if self.raw_telemetry_path.trim().is_empty() {
            return Err(HostConfigError::InvalidRawTelemetryPath);
        }
        if self.raw_telemetry_service_name.trim().is_empty() {
            return Err(HostConfigError::InvalidRawTelemetryServiceName);
        }
        if self.policy_state_path.trim().is_empty() {
            return Err(HostConfigError::InvalidPolicyStatePath);
        }
        Ok(())
    }
}

fn optional_bool(name: &'static str, default: bool) -> Result<bool, HostConfigError> {
    match env::var(name) {
        Ok(value) => parse_optional_bool(name, Some(value), default),
        Err(env::VarError::NotPresent) => parse_optional_bool(name, None, default),
        Err(env::VarError::NotUnicode(value)) => Err(HostConfigError::InvalidBoolean {
            name,
            value: value.to_string_lossy().into_owned(),
        }),
    }
}

fn parse_optional_bool(
    name: &'static str,
    value: Option<String>,
    default: bool,
) -> Result<bool, HostConfigError> {
    match value {
        None => Ok(default),
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(false),
        Some(value) if value.trim().is_empty() => Ok(default),
        Some(value) => Err(HostConfigError::InvalidBoolean { name, value }),
    }
}

fn required(name: &'static str) -> Result<String, HostConfigError> {
    let value = env::var(name).unwrap_or_default();
    if value.trim().is_empty() {
        Err(HostConfigError::Missing(name))
    } else {
        Ok(value)
    }
}

fn positive(name: &'static str) -> Result<u64, HostConfigError> {
    let value = required(name)?;
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(HostConfigError::InvalidPositiveInteger { name, value })
}

fn unsigned(name: &'static str) -> Result<u64, HostConfigError> {
    let value = required(name)?;
    value
        .parse::<u64>()
        .map_err(|_| HostConfigError::InvalidUnsignedInteger { name, value })
}

fn positive_or_default(name: &'static str, default: u64) -> Result<u64, HostConfigError> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(HostConfigError::InvalidPositiveInteger { name, value }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_shrink_defaults_on_while_renotification_defaults_off() {
        assert_eq!(
            parse_optional_bool(
                "VIRTIO_MEM_AUTOMATIC_WINDOWS_SHRINK",
                None,
                DEFAULT_AUTOMATIC_WINDOWS_SHRINK,
            ),
            Ok(true)
        );
        assert_eq!(
            parse_optional_bool(
                "VIRTIO_MEM_SHRINK_RENOTIFICATION",
                Some(String::new()),
                DEFAULT_SHRINK_RENOTIFICATION,
            ),
            Ok(false)
        );
        assert_eq!(
            parse_optional_bool(
                "VIRTIO_MEM_AUTOMATIC_WINDOWS_SHRINK",
                Some("false".to_owned()),
                DEFAULT_AUTOMATIC_WINDOWS_SHRINK,
            ),
            Ok(false)
        );
    }
    #[test]
    fn rejects_unsafe_aliases() {
        let config = HostConfig {
            vm_name: "guest".to_owned(),
            alias: "bad;alias".to_owned(),
            min_memory_bytes: 1,
            max_memory_bytes: 2,
            lower_threshold_bytes: 1,
            upper_threshold_bytes: 2,
            grow_step_bytes: DEFAULT_GROW_STEP_BYTES,
            shrink_step_bytes: DEFAULT_SHRINK_STEP_BYTES,
            fixed_visible_base_bytes: 1,
            physical_reserve_bytes: DEFAULT_PHYSICAL_RESERVE_BYTES,
            commit_reserve_bytes: DEFAULT_COMMIT_RESERVE_BYTES,
            safe_floor_physical_reserve_bytes: DEFAULT_SAFE_FLOOR_PHYSICAL_RESERVE_BYTES,
            safe_floor_commit_reserve_bytes: DEFAULT_SAFE_FLOOR_COMMIT_RESERVE_BYTES,
            reclaim_history: Duration::from_secs(DEFAULT_RECLAIM_HISTORY_SECONDS),
            reclaim_max_gap: Duration::from_secs(2),
            downward_hysteresis_bytes: DEFAULT_DOWNWARD_HYSTERESIS_BYTES,
            policy_state_path: "state.json".to_owned(),
            poll_interval: Duration::from_secs(1),
            command_timeout: Duration::from_secs(1),
            convergence_timeout: Duration::from_secs(1),
            virsh_binary: "virsh".to_owned(),
            stats_source: StatsSource::DomMemStat,
            demand_source: DemandSourceMode::Raw,
            stats_max_age: Duration::from_secs(60),
            stats_future_tolerance: Duration::from_secs(5),
            raw_telemetry_path: "/run/virtio-mem-host/guest.telemetry.jsonl".to_owned(),
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 1,
            compatibility_attestation_path: "/etc/virtio-mem-host/guest.attestation.json"
                .to_owned(),
            automatic_windows_shrink: false,
            shrink_renotification: false,
        };
        assert_eq!(config.validate(), Err(HostConfigError::InvalidAlias));
    }

    #[test]
    fn rejects_empty_attestation_path() {
        let mut config = HostConfig {
            vm_name: "guest".to_owned(),
            alias: "memory0".to_owned(),
            min_memory_bytes: 1,
            max_memory_bytes: 2,
            lower_threshold_bytes: 1,
            upper_threshold_bytes: 2,
            grow_step_bytes: DEFAULT_GROW_STEP_BYTES,
            shrink_step_bytes: DEFAULT_SHRINK_STEP_BYTES,
            fixed_visible_base_bytes: 1,
            physical_reserve_bytes: DEFAULT_PHYSICAL_RESERVE_BYTES,
            commit_reserve_bytes: DEFAULT_COMMIT_RESERVE_BYTES,
            safe_floor_physical_reserve_bytes: DEFAULT_SAFE_FLOOR_PHYSICAL_RESERVE_BYTES,
            safe_floor_commit_reserve_bytes: DEFAULT_SAFE_FLOOR_COMMIT_RESERVE_BYTES,
            reclaim_history: Duration::from_secs(DEFAULT_RECLAIM_HISTORY_SECONDS),
            reclaim_max_gap: Duration::from_secs(2),
            downward_hysteresis_bytes: DEFAULT_DOWNWARD_HYSTERESIS_BYTES,
            policy_state_path: "state.json".to_owned(),
            poll_interval: Duration::from_secs(1),
            command_timeout: Duration::from_secs(1),
            convergence_timeout: Duration::from_secs(1),
            virsh_binary: "virsh".to_owned(),
            stats_source: StatsSource::DomMemStat,
            demand_source: DemandSourceMode::Raw,
            stats_max_age: Duration::from_secs(60),
            stats_future_tolerance: Duration::from_secs(5),
            raw_telemetry_path: "/run/virtio-mem-host/guest.telemetry.jsonl".to_owned(),
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 1,
            compatibility_attestation_path: "valid".to_owned(),
            automatic_windows_shrink: false,
            shrink_renotification: false,
        };
        config.compatibility_attestation_path = " ".to_owned();
        assert_eq!(
            config.validate(),
            Err(HostConfigError::InvalidAttestationPath)
        );
    }

    #[test]
    fn rejects_empty_raw_telemetry_path() {
        let mut config = HostConfig {
            vm_name: "guest".to_owned(),
            alias: "memory0".to_owned(),
            min_memory_bytes: 1,
            max_memory_bytes: 2,
            lower_threshold_bytes: 1,
            upper_threshold_bytes: 2,
            grow_step_bytes: DEFAULT_GROW_STEP_BYTES,
            shrink_step_bytes: DEFAULT_SHRINK_STEP_BYTES,
            fixed_visible_base_bytes: 1,
            physical_reserve_bytes: DEFAULT_PHYSICAL_RESERVE_BYTES,
            commit_reserve_bytes: DEFAULT_COMMIT_RESERVE_BYTES,
            safe_floor_physical_reserve_bytes: DEFAULT_SAFE_FLOOR_PHYSICAL_RESERVE_BYTES,
            safe_floor_commit_reserve_bytes: DEFAULT_SAFE_FLOOR_COMMIT_RESERVE_BYTES,
            reclaim_history: Duration::from_secs(DEFAULT_RECLAIM_HISTORY_SECONDS),
            reclaim_max_gap: Duration::from_secs(2),
            downward_hysteresis_bytes: DEFAULT_DOWNWARD_HYSTERESIS_BYTES,
            policy_state_path: "state.json".to_owned(),
            poll_interval: Duration::from_secs(1),
            command_timeout: Duration::from_secs(1),
            convergence_timeout: Duration::from_secs(1),
            virsh_binary: "virsh".to_owned(),
            stats_source: StatsSource::DomMemStat,
            demand_source: DemandSourceMode::Raw,
            stats_max_age: Duration::from_secs(60),
            stats_future_tolerance: Duration::from_secs(5),
            raw_telemetry_path: "valid".to_owned(),
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 1,
            compatibility_attestation_path: "valid".to_owned(),
            automatic_windows_shrink: false,
            shrink_renotification: false,
        };
        config.raw_telemetry_path = " ".to_owned();
        assert_eq!(
            config.validate(),
            Err(HostConfigError::InvalidRawTelemetryPath)
        );
    }
}
