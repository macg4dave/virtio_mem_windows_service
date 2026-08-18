use std::env;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostConfigError {
    #[error("required environment variable is missing or empty: {0}")]
    Missing(&'static str),
    #[error("environment variable {name} must be a positive decimal integer: {value}")]
    InvalidPositiveInteger { name: &'static str, value: String },
    #[error("VIRTIO_MEM_ALIAS contains unsupported characters")]
    InvalidAlias,
    #[error("lower threshold must not exceed upper threshold")]
    InvalidThresholdOrder,
    #[error("minimum memory must not exceed maximum memory")]
    InvalidMemoryRange,
    #[error("host-controller durations must be greater than zero")]
    InvalidDuration,
    #[error("VIRTIO_MEM_STATS_SOURCE must be 'dommemstat' or 'qga': {0}")]
    InvalidStatsSource(String),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    pub vm_name: String,
    pub alias: String,
    pub min_memory_bytes: u64,
    pub max_memory_bytes: u64,
    pub lower_threshold_bytes: u64,
    pub upper_threshold_bytes: u64,
    pub poll_interval: Duration,
    pub command_timeout: Duration,
    pub convergence_timeout: Duration,
    pub virsh_binary: String,
    pub stats_source: StatsSource,
    pub host_min_headroom_bytes: u64,
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
        let config = Self {
            vm_name,
            alias,
            min_memory_bytes: positive("VIRTIO_MEM_MIN_MEMORY_BYTES")?,
            max_memory_bytes: positive("VIRTIO_MEM_MAX_MEMORY_BYTES")?,
            lower_threshold_bytes: positive("VIRTIO_MEM_LOWER_THRESHOLD_BYTES")?,
            upper_threshold_bytes: positive("VIRTIO_MEM_UPPER_THRESHOLD_BYTES")?,
            poll_interval: Duration::from_secs(positive("VIRTIO_MEM_POLL_INTERVAL_SECONDS")?),
            command_timeout: Duration::from_secs(positive("VIRTIO_MEM_COMMAND_TIMEOUT_SECONDS")?),
            convergence_timeout: Duration::from_secs(positive(
                "VIRTIO_MEM_CONVERGENCE_TIMEOUT_SECONDS",
            )?),
            virsh_binary: env::var("VIRTIO_MEM_VIRSH_BINARY")
                .unwrap_or_else(|_| "virsh".to_owned()),
            stats_source,
            host_min_headroom_bytes: positive("VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES")?,
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
        if self.poll_interval.is_zero()
            || self.command_timeout.is_zero()
            || self.convergence_timeout.is_zero()
        {
            return Err(HostConfigError::InvalidDuration);
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_aliases() {
        let config = HostConfig {
            vm_name: "guest".to_owned(),
            alias: "bad;alias".to_owned(),
            min_memory_bytes: 1,
            max_memory_bytes: 2,
            lower_threshold_bytes: 1,
            upper_threshold_bytes: 2,
            poll_interval: Duration::from_secs(1),
            command_timeout: Duration::from_secs(1),
            convergence_timeout: Duration::from_secs(1),
            virsh_binary: "virsh".to_owned(),
            stats_source: StatsSource::DomMemStat,
            host_min_headroom_bytes: 1,
        };
        assert_eq!(config.validate(), Err(HostConfigError::InvalidAlias));
    }
}
