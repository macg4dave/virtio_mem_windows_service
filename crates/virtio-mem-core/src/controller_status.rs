//! Versioned read-only controller status contract.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ControlHealth, ReconcileDirection, VirtioMemState};

pub const CONTROLLER_STATUS_VERSION: u16 = 1;
pub const MAX_CONTROLLER_STATUS_BYTES: usize = 64 * 1024;
const MAX_IDENTITY_BYTES: usize = 256;
const MAX_REASON_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReclaimReadiness {
    Cold,
    Warming,
    Ready,
    Paused,
    BlockedInFlight,
    BlockedLatched,
    BlockedRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityState {
    Unknown,
    Available,
    AtEffectiveMaximum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOwnership {
    None,
    OwnedPending,
    OwnedConvergedUnresolved,
    UnownedPending,
    RecordedNotApplied,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryState {
    NotRequired,
    ReviewRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedTelemetryIdentity {
    pub session_id: String,
    pub sequence: u64,
    pub monotonic_millis: u64,
    pub observed_unix_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerCommandStatus {
    pub operation_id: String,
    pub direction: ReconcileDirection,
    pub prior_requested_bytes: u64,
    pub prior_current_bytes: u64,
    pub target_bytes: u64,
    pub telemetry_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerStatusSnapshot {
    pub version: u16,
    pub observed_unix_millis: u64,
    pub vm_name: String,
    pub device_alias: String,
    pub device_size_bytes: u64,
    pub block_size_bytes: u64,
    pub desired_bytes: u64,
    pub safe_floor_bytes: u64,
    pub effective_maximum_bytes: u64,
    pub requested_bytes: u64,
    pub current_bytes: u64,
    pub accepted_telemetry: Option<AcceptedTelemetryIdentity>,
    pub history_ready: bool,
    pub reclaim_readiness: ReclaimReadiness,
    pub capacity_state: CapacityState,
    pub command_ownership: CommandOwnership,
    pub command: Option<ControllerCommandStatus>,
    pub control_health: ControlHealth,
    pub actuation_latched: bool,
    pub latch_reason: Option<String>,
    pub recovery_state: RecoveryState,
    pub recovery_reason: Option<String>,
    pub last_latch_clear_reason: Option<String>,
    pub policy_fingerprint_sha256: String,
    pub compatibility_fingerprint_sha256: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ControllerStatusError {
    #[error("controller status exceeds its byte limit")]
    Oversized,
    #[error("controller status JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("controller status version is unsupported: {0}")]
    UnsupportedVersion(u16),
    #[error("controller status is invalid: {0}")]
    Invalid(&'static str),
}

impl ControllerStatusSnapshot {
    pub fn validate(&self) -> Result<(), ControllerStatusError> {
        if self.version != CONTROLLER_STATUS_VERSION {
            return Err(ControllerStatusError::UnsupportedVersion(self.version));
        }
        validate_identity(&self.vm_name)?;
        validate_identity(&self.device_alias)?;
        validate_sha256(&self.policy_fingerprint_sha256)?;
        validate_sha256(&self.compatibility_fingerprint_sha256)?;
        VirtioMemState {
            size_bytes: self.device_size_bytes,
            block_size_bytes: self.block_size_bytes,
            requested_bytes: self.requested_bytes,
            current_bytes: self.current_bytes,
        }
        .validate()
        .map_err(|_| ControllerStatusError::Invalid("live device state is invalid"))?;
        if self.desired_bytes == 0
            || self.safe_floor_bytes == 0
            || self.effective_maximum_bytes == 0
            || self.safe_floor_bytes > self.desired_bytes
            || self.desired_bytes > self.effective_maximum_bytes
            || self.effective_maximum_bytes >= self.device_size_bytes
            || !self.desired_bytes.is_multiple_of(self.block_size_bytes)
            || !self.safe_floor_bytes.is_multiple_of(self.block_size_bytes)
            || !self
                .effective_maximum_bytes
                .is_multiple_of(self.block_size_bytes)
        {
            return Err(ControllerStatusError::Invalid(
                "target values violate bounds or alignment",
            ));
        }
        if let Some(identity) = &self.accepted_telemetry {
            validate_identity(&identity.session_id)?;
            if identity.observed_unix_millis == 0 {
                return Err(ControllerStatusError::Invalid(
                    "accepted telemetry has no wall-clock time",
                ));
            }
        }
        if self.history_ready != matches!(self.reclaim_readiness, ReclaimReadiness::Ready)
            && matches!(
                self.reclaim_readiness,
                ReclaimReadiness::Cold | ReclaimReadiness::Warming | ReclaimReadiness::Ready
            )
        {
            return Err(ControllerStatusError::Invalid(
                "history and reclaim readiness disagree",
            ));
        }
        if matches!(self.reclaim_readiness, ReclaimReadiness::Cold)
            && self.accepted_telemetry.is_some()
        {
            return Err(ControllerStatusError::Invalid(
                "cold reclaim status contains accepted telemetry",
            ));
        }
        let command_required = matches!(
            self.command_ownership,
            CommandOwnership::OwnedPending
                | CommandOwnership::OwnedConvergedUnresolved
                | CommandOwnership::RecordedNotApplied
                | CommandOwnership::Conflict
        );
        if command_required != self.command.is_some() {
            return Err(ControllerStatusError::Invalid(
                "command ownership and command detail disagree",
            ));
        }
        if matches!(self.command_ownership, CommandOwnership::None)
            && self.requested_bytes != self.current_bytes
        {
            return Err(ControllerStatusError::Invalid(
                "converged ownership contains divergent live state",
            ));
        }
        if matches!(self.command_ownership, CommandOwnership::UnownedPending)
            && self.requested_bytes == self.current_bytes
        {
            return Err(ControllerStatusError::Invalid(
                "unowned pending status is already converged",
            ));
        }
        if let Some(command) = &self.command {
            validate_identity(&command.operation_id)?;
            validate_identity(&command.telemetry_identity)?;
            if command.target_bytes == 0
                || !command.target_bytes.is_multiple_of(self.block_size_bytes)
            {
                return Err(ControllerStatusError::Invalid("command target is invalid"));
            }
        }
        validate_optional_reason(self.latch_reason.as_deref())?;
        validate_optional_reason(self.recovery_reason.as_deref())?;
        validate_optional_reason(self.last_latch_clear_reason.as_deref())?;
        if self.actuation_latched != self.latch_reason.is_some() {
            return Err(ControllerStatusError::Invalid(
                "latch state and reason disagree",
            ));
        }
        if matches!(self.recovery_state, RecoveryState::ReviewRequired)
            != self.recovery_reason.is_some()
        {
            return Err(ControllerStatusError::Invalid(
                "recovery state and reason disagree",
            ));
        }
        Ok(())
    }
}

pub fn parse_controller_status(
    bytes: &[u8],
) -> Result<ControllerStatusSnapshot, ControllerStatusError> {
    if bytes.len() > MAX_CONTROLLER_STATUS_BYTES {
        return Err(ControllerStatusError::Oversized);
    }
    let status: ControllerStatusSnapshot = serde_json::from_slice(bytes)
        .map_err(|error| ControllerStatusError::InvalidJson(error.to_string()))?;
    status.validate()?;
    Ok(status)
}

fn validate_identity(value: &str) -> Result<(), ControllerStatusError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(ControllerStatusError::Invalid(
            "identity is empty, oversized, or contains control characters",
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), ControllerStatusError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ControllerStatusError::Invalid(
            "fingerprint is not hexadecimal SHA-256",
        ));
    }
    Ok(())
}

fn validate_optional_reason(value: Option<&str>) -> Result<(), ControllerStatusError> {
    if value.is_some_and(|value| {
        value.trim().is_empty()
            || value.len() > MAX_REASON_BYTES
            || value.chars().any(char::is_control)
    }) {
        return Err(ControllerStatusError::Invalid(
            "reason is empty, oversized, or contains control characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    fn status() -> ControllerStatusSnapshot {
        ControllerStatusSnapshot {
            version: CONTROLLER_STATUS_VERSION,
            observed_unix_millis: 1,
            vm_name: "guest".to_owned(),
            device_alias: "memory0".to_owned(),
            device_size_bytes: 16 * GIB,
            block_size_bytes: 2 * MIB,
            desired_bytes: 8 * GIB,
            safe_floor_bytes: 7 * GIB,
            effective_maximum_bytes: 15 * GIB,
            requested_bytes: 8 * GIB,
            current_bytes: 8 * GIB,
            accepted_telemetry: Some(AcceptedTelemetryIdentity {
                session_id: "session-a".to_owned(),
                sequence: 4,
                monotonic_millis: 40,
                observed_unix_millis: 1_000,
            }),
            history_ready: true,
            reclaim_readiness: ReclaimReadiness::Ready,
            capacity_state: CapacityState::Available,
            command_ownership: CommandOwnership::None,
            command: None,
            control_health: ControlHealth::Converged,
            actuation_latched: false,
            latch_reason: None,
            recovery_state: RecoveryState::NotRequired,
            recovery_reason: None,
            last_latch_clear_reason: None,
            policy_fingerprint_sha256: "a".repeat(64),
            compatibility_fingerprint_sha256: "b".repeat(64),
        }
    }

    #[test]
    fn round_trips_a_valid_versioned_snapshot() {
        let expected = status();
        let bytes = serde_json::to_vec(&expected).expect("encode status");
        assert_eq!(parse_controller_status(&bytes), Ok(expected));
    }

    #[test]
    fn rejects_unknown_fields_versions_and_inconsistent_states() {
        let mut value = serde_json::to_value(status()).expect("status value");
        value["unexpected"] = serde_json::json!(true);
        assert!(matches!(
            parse_controller_status(&serde_json::to_vec(&value).expect("encode")),
            Err(ControllerStatusError::InvalidJson(_))
        ));

        let mut invalid = status();
        invalid.version += 1;
        assert_eq!(
            invalid.validate(),
            Err(ControllerStatusError::UnsupportedVersion(2))
        );
        let mut invalid = status();
        invalid.command_ownership = CommandOwnership::OwnedPending;
        assert_eq!(
            invalid.validate(),
            Err(ControllerStatusError::Invalid(
                "command ownership and command detail disagree"
            ))
        );
        let mut invalid = status();
        invalid.actuation_latched = true;
        assert_eq!(
            invalid.validate(),
            Err(ControllerStatusError::Invalid(
                "latch state and reason disagree"
            ))
        );
    }

    #[test]
    fn rejects_oversized_input_before_decoding() {
        assert_eq!(
            parse_controller_status(&vec![b' '; MAX_CONTROLLER_STATUS_BYTES + 1]),
            Err(ControllerStatusError::Oversized)
        );
    }
}
