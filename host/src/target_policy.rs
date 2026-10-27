//! Host-owned estimator state, checkpointing, and raw-telemetry join.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use virtio_mem_core::{
    calculate_effective_maximum, AcceptedTelemetryIdentity, CapacityState, CommandOwnership,
    ControlHealth, ControllerCommandStatus, ControllerStatusSnapshot, ReclaimReadiness,
    RecoveryState, TargetEstimator, TargetEstimatorState, TargetGeometry, TargetPolicyConfig,
    TargetSample, VirtioMemState, CONTROLLER_STATUS_VERSION,
};

use crate::attestation::CompatibilityAttestation;
use crate::config::HostConfig;
use crate::raw_telemetry::{AcceptedTelemetry, RawTelemetrySource, SampleAcceptance};
use crate::runtime::{
    command_ownership, CommandIntent, DemandEstimate, DemandSource, DurableControlState,
};

const POLICY_CHECKPOINT_VERSION: u16 = 1;
const MAX_POLICY_CHECKPOINT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyCheckpoint {
    version: u16,
    vm_name: String,
    device_alias: String,
    policy_fingerprint_sha256: String,
    compatibility_fingerprint_sha256: String,
    estimator: TargetEstimatorState,
    #[serde(default)]
    actuation_latched: bool,
    #[serde(default)]
    actuation_latch_reason: Option<String>,
    #[serde(default)]
    command_intent: Option<CommandIntent>,
    #[serde(default)]
    last_latch_clear_reason: Option<String>,
    /// The last telemetry record the estimator consumed. Written in the same
    /// checkpoint as the estimator, so acceptance and consumption never
    /// disagree. Older checkpoints without it start with no accepted record.
    #[serde(default)]
    accepted_telemetry: AcceptedTelemetry,
    /// Earlier builds stored shadow-policy history here. It was expendable
    /// history, never control state, so it is accepted on load and dropped.
    #[serde(default, rename = "pressure_shadow", skip_serializing)]
    retired_pressure_shadow: RetiredPressureShadow,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RetiredPressureShadow;

impl<'de> Deserialize<'de> for RetiredPressureShadow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer).map(|_| Self)
    }
}

/// The estimator and the record it last consumed always change together.
#[derive(Debug, Clone)]
struct SampleState {
    estimator: TargetEstimator,
    accepted: AcceptedTelemetry,
}

pub struct TargetDemandSource<R> {
    raw: R,
    sample: Mutex<SampleState>,
    checkpoint_path: PathBuf,
    vm_name: String,
    device_alias: String,
    policy_fingerprint: String,
    compatibility_fingerprint: String,
    control: Mutex<CheckpointControl>,
}

#[derive(Debug, Clone, Default)]
struct CheckpointControl {
    actuation_latched: bool,
    actuation_latch_reason: Option<String>,
    command_intent: Option<CommandIntent>,
    last_latch_clear_reason: Option<String>,
}

impl<R> TargetDemandSource<R> {
    pub fn new(raw: R, config: &HostConfig) -> Result<Self, String> {
        let compatibility_fingerprint =
            compatibility_fingerprint(&config.compatibility_attestation_path)?;
        Self::with_compatibility_fingerprint(raw, config, compatibility_fingerprint)
    }

    fn with_compatibility_fingerprint(
        raw: R,
        config: &HostConfig,
        compatibility_fingerprint: String,
    ) -> Result<Self, String> {
        let policy = target_policy_config(config)?;
        let policy_fingerprint = policy_fingerprint(&policy)?;
        let checkpoint_path = PathBuf::from(&config.policy_state_path);
        let checkpoint = load_matching_checkpoint(
            &checkpoint_path,
            &config.vm_name,
            &config.alias,
            &policy_fingerprint,
            &compatibility_fingerprint,
        )?;
        let control = checkpoint
            .as_ref()
            .map(|checkpoint| CheckpointControl {
                actuation_latched: checkpoint.actuation_latched,
                actuation_latch_reason: checkpoint.actuation_latch_reason.clone(),
                command_intent: checkpoint.command_intent.clone(),
                last_latch_clear_reason: checkpoint.last_latch_clear_reason.clone(),
            })
            .unwrap_or_default();
        let (state, accepted) = checkpoint
            .map(|checkpoint| (checkpoint.estimator, checkpoint.accepted_telemetry))
            .unwrap_or_default();
        let estimator = match TargetEstimator::restore(policy, state) {
            Ok(estimator) => estimator,
            Err(error) => {
                eprintln!(
                    "target policy checkpoint state is invalid; starting with cold reclaim history: {error}"
                );
                TargetEstimator::new(policy).map_err(|error| error.to_string())?
            }
        };
        Ok(Self {
            raw,
            sample: Mutex::new(SampleState {
                estimator,
                accepted,
            }),
            checkpoint_path,
            vm_name: config.vm_name.clone(),
            device_alias: config.alias.clone(),
            policy_fingerprint,
            compatibility_fingerprint,
            control: Mutex::new(control),
        })
    }
}

/// Reads the durable controller checkpoint and joins it with one fresh
/// alias-scoped live state. This function never evaluates demand, resolves an
/// intent, writes the checkpoint, or invokes a resize sink.
pub fn read_controller_status(
    config: &HostConfig,
    live: VirtioMemState,
) -> Result<ControllerStatusSnapshot, String> {
    let observed_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_owned())?
        .as_millis()
        .try_into()
        .map_err(|_| "system clock does not fit controller status".to_owned())?;
    read_controller_status_at(config, live, observed_unix_millis)
}

fn read_controller_status_at(
    config: &HostConfig,
    live: VirtioMemState,
    observed_unix_millis: u64,
) -> Result<ControllerStatusSnapshot, String> {
    live.validate().map_err(|error| error.to_string())?;
    let policy = target_policy_config(config)?;
    let effective_maximum_bytes = calculate_effective_maximum(
        &policy,
        TargetGeometry {
            device_size_bytes: live.size_bytes,
            block_size_bytes: live.block_size_bytes,
            current_bytes: live.current_bytes,
        },
    )
    .map_err(|error| error.to_string())?;
    let policy_fingerprint_sha256 = policy_fingerprint(&policy)?;
    let compatibility_fingerprint_sha256 =
        compatibility_fingerprint(&config.compatibility_attestation_path)?;
    let checkpoint = load_matching_checkpoint(
        Path::new(&config.policy_state_path),
        &config.vm_name,
        &config.alias,
        &policy_fingerprint_sha256,
        &compatibility_fingerprint_sha256,
    )?;
    let (estimator_state, history_ready, control) = if let Some(checkpoint) = checkpoint {
        let estimator = TargetEstimator::restore(policy, checkpoint.estimator.clone())
            .map_err(|error| format!("target policy checkpoint state is invalid: {error}"))?;
        (
            checkpoint.estimator,
            estimator.history_ready(),
            CheckpointControl {
                actuation_latched: checkpoint.actuation_latched,
                actuation_latch_reason: checkpoint.actuation_latch_reason,
                command_intent: checkpoint.command_intent,
                last_latch_clear_reason: checkpoint.last_latch_clear_reason,
            },
        )
    } else {
        (
            TargetEstimatorState::cold(),
            false,
            CheckpointControl::default(),
        )
    };
    let desired_bytes = if estimator_state.desired_bytes == 0 {
        live.current_bytes
    } else {
        estimator_state.desired_bytes
    };
    let safe_floor_bytes = if estimator_state.safe_floor_bytes == 0 {
        live.current_bytes.min(desired_bytes)
    } else {
        estimator_state.safe_floor_bytes
    };
    let accepted_telemetry =
        (!estimator_state.session_id.is_empty()).then_some(AcceptedTelemetryIdentity {
            session_id: estimator_state.session_id,
            sequence: estimator_state.last_sequence,
            monotonic_millis: estimator_state.last_monotonic_millis,
            observed_unix_millis: estimator_state.last_observed_unix_millis,
        });
    let command = control
        .command_intent
        .as_ref()
        .map(|intent| ControllerCommandStatus {
            operation_id: intent.operation_id.clone(),
            direction: intent.direction,
            prior_requested_bytes: intent.prior_requested_bytes,
            prior_current_bytes: intent.prior_current_bytes,
            target_bytes: intent.target_bytes,
            telemetry_identity: intent.telemetry_identity.clone(),
        });
    let command_ownership = command_ownership(control.command_intent.as_ref(), live);
    let latch_reason = control.actuation_latched.then(|| {
        control
            .actuation_latch_reason
            .clone()
            .unwrap_or_else(|| "durable actuation latch has no recorded reason".to_owned())
    });
    let recovery_reason = recovery_reason(command_ownership, latch_reason.as_deref());
    let recovery_state = if recovery_reason.is_some() {
        RecoveryState::ReviewRequired
    } else {
        RecoveryState::NotRequired
    };
    let reclaim_readiness = if control.actuation_latched {
        ReclaimReadiness::BlockedLatched
    } else if recovery_reason.is_some() {
        ReclaimReadiness::BlockedRecovery
    } else if live.requested_bytes != live.current_bytes {
        ReclaimReadiness::BlockedInFlight
    } else if history_ready {
        ReclaimReadiness::Ready
    } else if accepted_telemetry.is_none() {
        ReclaimReadiness::Cold
    } else {
        ReclaimReadiness::Warming
    };
    let control_health = if control.actuation_latched {
        ControlHealth::Latched
    } else if recovery_reason.is_some() {
        ControlHealth::RecoveryRequired
    } else if matches!(command_ownership, CommandOwnership::OwnedPending) {
        match control
            .command_intent
            .as_ref()
            .map(|intent| intent.direction)
        {
            Some(virtio_mem_core::ReconcileDirection::Grow) => ControlHealth::Growing,
            _ => ControlHealth::Shrinking,
        }
    } else {
        ControlHealth::Converged
    };
    let capacity_state = if accepted_telemetry.is_none() {
        CapacityState::Unknown
    } else if desired_bytes == effective_maximum_bytes {
        CapacityState::AtEffectiveMaximum
    } else {
        CapacityState::Available
    };
    let snapshot = ControllerStatusSnapshot {
        version: CONTROLLER_STATUS_VERSION,
        observed_unix_millis,
        vm_name: config.vm_name.clone(),
        device_alias: config.alias.clone(),
        device_size_bytes: live.size_bytes,
        block_size_bytes: live.block_size_bytes,
        desired_bytes,
        safe_floor_bytes,
        effective_maximum_bytes,
        requested_bytes: live.requested_bytes,
        current_bytes: live.current_bytes,
        accepted_telemetry,
        history_ready,
        reclaim_readiness,
        capacity_state,
        command_ownership,
        command,
        control_health,
        actuation_latched: control.actuation_latched,
        latch_reason,
        recovery_state,
        recovery_reason,
        last_latch_clear_reason: control.last_latch_clear_reason,
        policy_fingerprint_sha256,
        compatibility_fingerprint_sha256,
    };
    snapshot.validate().map_err(|error| error.to_string())?;
    Ok(snapshot)
}

fn recovery_reason(ownership: CommandOwnership, latch_reason: Option<&str>) -> Option<String> {
    if let Some(reason) = latch_reason {
        return Some(format!("actuation_latched: {reason}"));
    }
    match ownership {
        CommandOwnership::None | CommandOwnership::OwnedPending => None,
        CommandOwnership::OwnedConvergedUnresolved => Some(
            "recorded command has converged but its durable intent remains unresolved".to_owned(),
        ),
        CommandOwnership::UnownedPending => {
            Some("live requested/current divergence has no durable command owner".to_owned())
        }
        CommandOwnership::RecordedNotApplied => {
            Some("recorded command is not reflected in live state".to_owned())
        }
        CommandOwnership::Conflict => {
            Some("live state conflicts with the durable command intent".to_owned())
        }
    }
}

impl<R: RawTelemetrySource> DemandSource for TargetDemandSource<R> {
    fn evaluate(&self, state: VirtioMemState) -> Result<DemandEstimate, String> {
        let read = self.raw.read();
        let mut sample = self
            .sample
            .lock()
            .map_err(|_| "target sample state lock is poisoned".to_owned())?;
        let classified = read.and_then(|envelope| {
            sample
                .accepted
                .classify(&envelope)
                .map(|acceptance| (envelope, acceptance))
        });
        let envelope = match classified {
            Ok((envelope, SampleAcceptance::New)) => envelope,
            Ok((_, SampleAcceptance::Unchanged)) => {
                let estimator = sample.estimator.state();
                let desired_bytes = if estimator.desired_bytes == 0 {
                    state.current_bytes
                } else {
                    estimator.desired_bytes
                };
                let safe_floor_bytes = if estimator.safe_floor_bytes == 0 {
                    state.current_bytes.min(desired_bytes)
                } else {
                    estimator.safe_floor_bytes.min(desired_bytes)
                };
                return Ok(DemandEstimate {
                    desired_bytes,
                    safe_floor_bytes,
                    effective_maximum_bytes: desired_bytes.max(state.current_bytes),
                    history_ready: false,
                    telemetry_identity: None,
                });
            }
            Err(error) => {
                if error.invalidates_history() {
                    sample.estimator.invalidate_history();
                    self.persist_sample(&sample)?;
                }
                return Err(error.to_string());
            }
        };
        // Work on a copy: the record counts as accepted only once the
        // checkpoint holding it and the estimate it fed is durable.
        let mut next = sample.clone();
        let estimate_result = next.estimator.estimate(
            TargetSample {
                session_id: envelope.session_id.clone(),
                observed_unix_millis: envelope.observed_unix_millis,
                monotonic_millis: envelope.monotonic_millis,
                sequence: envelope.sequence,
                memory: envelope.memory,
            },
            TargetGeometry {
                device_size_bytes: state.size_bytes,
                block_size_bytes: state.block_size_bytes,
                current_bytes: state.current_bytes,
            },
        );
        let estimate = match estimate_result {
            Ok(estimate) => estimate,
            Err(error) => {
                // The estimator may have invalidated its history; keep that,
                // but the record itself stays unaccepted.
                sample.estimator = next.estimator;
                self.persist_sample(&sample)?;
                return Err(error.to_string());
            }
        };
        next.accepted.accept(envelope);
        self.persist_sample(&next)?;
        *sample = next;
        if estimate.capacity_limited {
            eprintln!(
                "target estimator health=capacity_limited desired_now_bytes={} effective_maximum_bytes={}",
                estimate.desired_now_bytes, estimate.effective_maximum_bytes
            );
        }
        let estimator = sample.estimator.state();
        Ok(DemandEstimate {
            safe_floor_bytes: estimate.safe_floor_bytes,
            desired_bytes: estimate.desired_bytes,
            effective_maximum_bytes: estimate.effective_maximum_bytes,
            history_ready: estimate.history_ready,
            telemetry_identity: Some(format!(
                "{}:{}:{}:{}",
                estimator.session_id,
                estimator.last_sequence,
                estimator.last_monotonic_millis,
                estimator.last_observed_unix_millis
            )),
        })
    }

    fn control_state(&self) -> Result<DurableControlState, String> {
        let control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        Ok(DurableControlState {
            latch_reason: control.actuation_latched.then(|| {
                control
                    .actuation_latch_reason
                    .clone()
                    .unwrap_or_else(|| "durable actuation latch has no recorded reason".to_owned())
            }),
            intent: control.command_intent.clone(),
        })
    }

    fn save_control_state(&self, next: &DurableControlState) -> Result<(), String> {
        let sample = self
            .sample
            .lock()
            .map_err(|_| "target sample state lock is poisoned".to_owned())?;
        let mut control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        control.actuation_latched = next.latched();
        control.actuation_latch_reason = next.latch_reason.clone();
        control.command_intent = next.intent.clone().map(|mut intent| {
            intent.policy_fingerprint_sha256 = self.policy_fingerprint.clone();
            intent.compatibility_fingerprint_sha256 = self.compatibility_fingerprint.clone();
            intent
        });
        self.persist_state(&sample, &control)
    }
}

impl<R> TargetDemandSource<R> {
    fn persist_sample(&self, sample: &SampleState) -> Result<(), String> {
        let control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        self.persist_state(sample, &control)
    }

    fn persist_state(
        &self,
        sample: &SampleState,
        control: &CheckpointControl,
    ) -> Result<(), String> {
        persist_checkpoint(
            &self.checkpoint_path,
            &PolicyCheckpoint {
                version: POLICY_CHECKPOINT_VERSION,
                vm_name: self.vm_name.clone(),
                device_alias: self.device_alias.clone(),
                policy_fingerprint_sha256: self.policy_fingerprint.clone(),
                compatibility_fingerprint_sha256: self.compatibility_fingerprint.clone(),
                estimator: sample.estimator.state().clone(),
                actuation_latched: control.actuation_latched,
                actuation_latch_reason: control.actuation_latch_reason.clone(),
                command_intent: control.command_intent.clone(),
                last_latch_clear_reason: control.last_latch_clear_reason.clone(),
                accepted_telemetry: sample.accepted.clone(),
                retired_pressure_shadow: RetiredPressureShadow,
            },
        )
    }
}

pub fn clear_actuation_latch(
    config: &HostConfig,
    live: VirtioMemState,
    reason: &str,
    apply: bool,
) -> Result<String, String> {
    let reason = reason.trim();
    if reason.is_empty() || reason.len() > 256 || reason.chars().any(char::is_control) {
        return Err(
            "latch-clear reason must be 1..=256 bytes without control characters".to_owned(),
        );
    }
    live.validate().map_err(|error| error.to_string())?;
    if live.requested_bytes != live.current_bytes {
        return Err("clear-latch refuses divergent requested/current state".to_owned());
    }
    let policy = target_policy_config(config)?;
    let policy_fingerprint = policy_fingerprint(&policy)?;
    let compatibility_fingerprint =
        compatibility_fingerprint(&config.compatibility_attestation_path)?;
    let path = PathBuf::from(&config.policy_state_path);
    let mut checkpoint = load_checkpoint_for_latch_clear(
        &path,
        &config.vm_name,
        &config.alias,
        &policy_fingerprint,
    )?;
    if !checkpoint.actuation_latched {
        return Err("target-controller actuation is not latched".to_owned());
    }
    let message = format!(
        "clear_latch vm={} alias={} requested_bytes={} current_bytes={} reason={}",
        config.vm_name, config.alias, live.requested_bytes, live.current_bytes, reason
    );
    if apply {
        checkpoint.compatibility_fingerprint_sha256 = compatibility_fingerprint;
        checkpoint.estimator = TargetEstimatorState::cold();
        checkpoint.actuation_latched = false;
        checkpoint.actuation_latch_reason = None;
        checkpoint.command_intent = None;
        checkpoint.last_latch_clear_reason = Some(reason.to_owned());
        persist_checkpoint(&path, &checkpoint)?;
    }
    Ok(message)
}

fn load_checkpoint_for_latch_clear(
    path: &Path,
    vm_name: &str,
    device_alias: &str,
    policy_fingerprint: &str,
) -> Result<PolicyCheckpoint, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("inspect target policy checkpoint for latch clear: {error}"))?;
    if metadata.len() > MAX_POLICY_CHECKPOINT_BYTES {
        return Err("target policy checkpoint is oversized; refusing latch clear".to_owned());
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("read target policy checkpoint for latch clear: {error}"))?;
    let checkpoint: PolicyCheckpoint = serde_json::from_slice(&bytes).map_err(|error| {
        format!("target policy checkpoint is invalid; refusing latch clear: {error}")
    })?;
    if checkpoint.version != POLICY_CHECKPOINT_VERSION
        || checkpoint.estimator.version != virtio_mem_core::TARGET_ESTIMATOR_STATE_VERSION
        || checkpoint.vm_name != vm_name
        || checkpoint.device_alias != device_alias
        || checkpoint.policy_fingerprint_sha256 != policy_fingerprint
    {
        return Err(
            "target policy checkpoint does not match the current VM, alias, or policy; refusing latch clear"
                .to_owned(),
        );
    }
    Ok(checkpoint)
}

fn target_policy_config(config: &HostConfig) -> Result<TargetPolicyConfig, String> {
    Ok(TargetPolicyConfig {
        configured_minimum_bytes: config.min_memory_bytes,
        physical_reserve_bytes: config.physical_reserve_bytes,
        commit_reserve_bytes: config.commit_reserve_bytes,
        safe_floor_physical_reserve_bytes: config.safe_floor_physical_reserve_bytes,
        safe_floor_commit_reserve_bytes: config.safe_floor_commit_reserve_bytes,
        history_window_millis: duration_millis(config.reclaim_history)?,
        maximum_gap_millis: duration_millis(config.reclaim_max_gap)?,
        downward_hysteresis_bytes: config.downward_hysteresis_bytes,
    })
}

fn duration_millis(duration: std::time::Duration) -> Result<u64, String> {
    u64::try_from(duration.as_millis()).map_err(|_| "policy duration is too large".to_owned())
}

fn policy_fingerprint(policy: &TargetPolicyConfig) -> Result<String, String> {
    let bytes = serde_json::to_vec(policy)
        .map_err(|error| format!("serialize target policy fingerprint: {error}"))?;
    Ok(sha256_hex(&bytes))
}

fn compatibility_fingerprint(path: &str) -> Result<String, String> {
    let mut json = String::new();
    fs::File::open(path)
        .map_err(|error| format!("open compatibility attestation for policy state: {error}"))?
        .take(MAX_POLICY_CHECKPOINT_BYTES + 1)
        .read_to_string(&mut json)
        .map_err(|error| format!("read compatibility attestation for policy state: {error}"))?;
    if json.len() as u64 > MAX_POLICY_CHECKPOINT_BYTES {
        return Err("compatibility attestation exceeds policy-state read limit".to_owned());
    }
    CompatibilityAttestation::parse(&json).map(|document| document.fingerprint_sha256)
}

fn load_matching_checkpoint(
    path: &Path,
    vm_name: &str,
    device_alias: &str,
    policy_fingerprint: &str,
    compatibility_fingerprint: &str,
) -> Result<Option<PolicyCheckpoint>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("inspect target policy checkpoint: {error}")),
    };
    if metadata.len() > MAX_POLICY_CHECKPOINT_BYTES {
        return Err(
            "target policy checkpoint is oversized; refusing to discard possible control state"
                .to_owned(),
        );
    }
    let bytes =
        fs::read(path).map_err(|error| format!("read target policy checkpoint: {error}"))?;
    let checkpoint: PolicyCheckpoint = match serde_json::from_slice(&bytes) {
        Ok(checkpoint) => checkpoint,
        Err(error) => return Err(format!(
            "target policy checkpoint is invalid; refusing to discard possible control state: {error}"
        )),
    };
    if checkpoint.version != POLICY_CHECKPOINT_VERSION
        || checkpoint.estimator.version != virtio_mem_core::TARGET_ESTIMATOR_STATE_VERSION
        || checkpoint.vm_name != vm_name
        || checkpoint.device_alias != device_alias
        || checkpoint.policy_fingerprint_sha256 != policy_fingerprint
        || checkpoint.compatibility_fingerprint_sha256 != compatibility_fingerprint
    {
        if checkpoint.actuation_latched || checkpoint.command_intent.is_some() {
            return Err(
                "target policy checkpoint identity or fingerprint changed while control state is pending"
                    .to_owned(),
            );
        }
        return Ok(None);
    }
    if checkpoint.command_intent.as_ref().is_some_and(|intent| {
        intent.operation_id.trim().is_empty()
            || intent.telemetry_identity.trim().is_empty()
            || intent.policy_fingerprint_sha256 != checkpoint.policy_fingerprint_sha256
            || intent.compatibility_fingerprint_sha256
                != checkpoint.compatibility_fingerprint_sha256
    }) {
        return Err(
            "target policy command intent is invalid; refusing to discard possible control state"
                .to_owned(),
        );
    }
    Ok(Some(checkpoint))
}

fn persist_checkpoint(path: &Path, checkpoint: &PolicyCheckpoint) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create target policy checkpoint directory: {error}"))?;
    }
    let bytes = serde_json::to_vec(checkpoint)
        .map_err(|error| format!("encode target policy checkpoint: {error}"))?;
    if bytes.len() as u64 > MAX_POLICY_CHECKPOINT_BYTES {
        return Err("target policy checkpoint exceeds size limit".to_owned());
    }
    let temporary = PathBuf::from(format!("{}.tmp-{}", path.display(), std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("open target policy checkpoint: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("write target policy checkpoint: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("flush target policy checkpoint: {error}"))?;
    drop(file);
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("publish target policy checkpoint: {error}")
    })?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("flush target policy checkpoint directory: {error}"))?;
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attestation::{CompatibilityReview, LiveCompatibilityEvidence};
    use crate::raw_telemetry::RawTelemetryError;
    use serde_json::json;
    use std::time::Duration;
    use virtio_mem_core::{MemoryTelemetrySnapshot, RawTelemetryEnvelope};

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    struct OneEnvelope(Mutex<Option<RawTelemetryEnvelope>>);

    impl RawTelemetrySource for OneEnvelope {
        fn read(&self) -> Result<RawTelemetryEnvelope, RawTelemetryError> {
            self.0
                .lock()
                .map_err(|_| {
                    RawTelemetryError::TransportUnavailable("fixture lock poisoned".to_owned())
                })?
                .take()
                .ok_or_else(|| {
                    RawTelemetryError::TransportUnavailable("fixture exhausted".to_owned())
                })
        }
    }

    /// A producer that has not published since: every read returns the same record.
    struct RepeatingEnvelope(RawTelemetryEnvelope);

    impl RawTelemetrySource for RepeatingEnvelope {
        fn read(&self) -> Result<RawTelemetryEnvelope, RawTelemetryError> {
            Ok(self.0.clone())
        }
    }

    fn pressured_envelope() -> RawTelemetryEnvelope {
        RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000_000,
            10,
            0,
            MemoryTelemetrySnapshot {
                physical_total_bytes: 16 * GIB,
                physical_available_bytes: 0,
                memory_load_percent: 100,
                commit_total_bytes: 32 * GIB,
                commit_limit_bytes: 32 * GIB,
                commit_peak_bytes: 32 * GIB,
                system_cache_bytes: 0,
                kernel_paged_bytes: 0,
                kernel_nonpaged_bytes: 0,
            },
        )
    }

    fn eight_gib_live() -> VirtioMemState {
        VirtioMemState {
            size_bytes: 24 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 8 * GIB,
            current_bytes: 8 * GIB,
        }
    }

    struct FailingRaw(RawTelemetryError);

    impl RawTelemetrySource for FailingRaw {
        fn read(&self) -> Result<RawTelemetryEnvelope, RawTelemetryError> {
            Err(self.0.clone())
        }
    }

    fn paths(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "virtio-mem-target-policy-{name}-{}",
            std::process::id()
        ));
        (
            base.with_extension("state.json"),
            base.with_extension("attestation.json"),
        )
    }

    fn write_attestation(path: &Path) -> String {
        write_attestation_with_domain(path, 'a')
    }

    fn write_attestation_with_domain(path: &Path, domain_hash_character: char) -> String {
        let document = CompatibilityAttestation::new(
            LiveCompatibilityEvidence {
                vm_name: "guest".to_owned(),
                device_alias: "memory0".to_owned(),
                domain_xml_sha256: domain_hash_character.to_string().repeat(64),
                qemu_argv_sha256: "b".repeat(64),
                libvirt_version_sha256: "c".repeat(64),
                qemu_version: json!({
                    "qemu": {"major": 10, "minor": 0, "micro": 0},
                    "package": "test"
                }),
                dynamic_memslots: true,
                unplugged_inaccessible: true,
            },
            CompatibilityReview {
                trusted_development_guest: true,
                workload_reviewed: true,
                no_vdpa: true,
                no_rdma_migration: true,
                no_vfio_nvme: true,
                no_mlock: true,
                no_secure_virtualization: true,
                no_unsupported_vhost_user: true,
                balloon_resize_inactive: true,
                memory_slot_budget: 1,
                vfio_mapping_budget: 1,
                windows_driver_version: "test".to_owned(),
            },
        )
        .expect("attestation");
        fs::write(path, document.to_pretty_json().expect("attestation JSON"))
            .expect("write attestation");
        document.fingerprint_sha256
    }

    fn config(state: &Path, attestation: &Path) -> HostConfig {
        HostConfig {
            vm_name: "guest".to_owned(),
            alias: "memory0".to_owned(),
            priority: 100,
            min_memory_bytes: 2 * GIB,
            grow_step_bytes: GIB,
            shrink_step_bytes: 64 * MIB,
            physical_reserve_bytes: 2 * GIB,
            commit_reserve_bytes: 2 * GIB,
            safe_floor_physical_reserve_bytes: GIB,
            safe_floor_commit_reserve_bytes: GIB,
            reclaim_history: Duration::from_secs(600),
            reclaim_max_gap: Duration::from_secs(60),
            downward_hysteresis_bytes: 256 * MIB,
            policy_state_path: state.display().to_string(),
            poll_interval: Duration::from_secs(30),
            command_timeout: Duration::from_secs(10),
            convergence_timeout: Duration::from_secs(300),
            virsh_binary: "virsh".to_owned(),
            raw_telemetry_path: "telemetry.jsonl".to_owned(),
            raw_telemetry_transport: crate::config::RawTelemetryTransport::File,
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 4 * GIB,
            compatibility_attestation_path: attestation.display().to_string(),
            observe_only: false,
        }
    }

    #[test]
    fn checkpoint_round_trip_and_identity_mismatch() {
        let (state_path, _) = paths("checkpoint");
        let checkpoint = PolicyCheckpoint {
            version: POLICY_CHECKPOINT_VERSION,
            vm_name: "guest".to_owned(),
            device_alias: "memory0".to_owned(),
            policy_fingerprint_sha256: "a".repeat(64),
            compatibility_fingerprint_sha256: "b".repeat(64),
            estimator: TargetEstimatorState::cold(),
            actuation_latched: false,
            actuation_latch_reason: None,
            command_intent: None,
            last_latch_clear_reason: None,
            accepted_telemetry: AcceptedTelemetry::default(),
            retired_pressure_shadow: RetiredPressureShadow,
        };
        persist_checkpoint(&state_path, &checkpoint).expect("persist checkpoint");
        assert_eq!(
            load_matching_checkpoint(
                &state_path,
                "guest",
                "memory0",
                &"a".repeat(64),
                &"b".repeat(64)
            )
            .expect("load checkpoint"),
            Some(checkpoint.clone())
        );
        assert!(load_matching_checkpoint(
            &state_path,
            "other",
            "memory0",
            &"a".repeat(64),
            &"b".repeat(64)
        )
        .expect("mismatch is cold state")
        .is_none());

        // Checkpoints from builds with shadow/growth modes carry a retired
        // `pressure_shadow` field, null in legacy mode. Both forms must load.
        for retired in [
            serde_json::Value::Null,
            serde_json::json!({"history": {"entries": []}}),
        ] {
            let mut value = serde_json::to_value(&checkpoint).expect("checkpoint value");
            value["pressure_shadow"] = retired;
            fs::write(&state_path, serde_json::to_vec(&value).expect("encode"))
                .expect("write retired-field checkpoint");
            assert_eq!(
                load_matching_checkpoint(
                    &state_path,
                    "guest",
                    "memory0",
                    &"a".repeat(64),
                    &"b".repeat(64)
                )
                .expect("retired field is accepted"),
                Some(checkpoint.clone())
            );
        }
        assert!(!serde_json::to_string(&checkpoint)
            .expect("encode checkpoint")
            .contains("pressure_shadow"));

        let mut latched = checkpoint;
        latched.actuation_latched = true;
        latched.actuation_latch_reason = Some("ambiguous command".to_owned());
        persist_checkpoint(&state_path, &latched).expect("write latched checkpoint");
        assert!(load_matching_checkpoint(
            &state_path,
            "guest",
            "memory0",
            &"a".repeat(64),
            &"changed".repeat(9)
        )
        .is_err());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn invalid_pending_intent_never_discards_the_checkpoint_or_latch() {
        let (state_path, _) = paths("invalid-pending-intent");
        for latched in [false, true] {
            let checkpoint = PolicyCheckpoint {
                version: POLICY_CHECKPOINT_VERSION,
                vm_name: "guest".to_owned(),
                device_alias: "memory0".to_owned(),
                policy_fingerprint_sha256: "a".repeat(64),
                compatibility_fingerprint_sha256: "b".repeat(64),
                estimator: TargetEstimatorState::cold(),
                actuation_latched: latched,
                actuation_latch_reason: latched.then(|| "uncertain command".to_owned()),
                command_intent: Some(CommandIntent {
                    operation_id: String::new(),
                    direction: virtio_mem_core::ReconcileDirection::Grow,
                    prior_requested_bytes: 8 * GIB,
                    prior_current_bytes: 8 * GIB,
                    target_bytes: 9 * GIB,
                    telemetry_identity: "session:1:10:1000".to_owned(),
                    policy_fingerprint_sha256: "a".repeat(64),
                    compatibility_fingerprint_sha256: "b".repeat(64),
                }),
                last_latch_clear_reason: None,
                accepted_telemetry: AcceptedTelemetry::default(),
                retired_pressure_shadow: RetiredPressureShadow,
            };
            persist_checkpoint(&state_path, &checkpoint).expect("write checkpoint");
            let before = fs::read(&state_path).expect("checkpoint bytes");
            let error = load_matching_checkpoint(
                &state_path,
                "guest",
                "memory0",
                &"a".repeat(64),
                &"b".repeat(64),
            )
            .expect_err("invalid intent must block startup rather than become cold state");
            assert!(error.contains("command intent"));
            assert_eq!(fs::read(&state_path).expect("retained checkpoint"), before);
        }
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn malformed_and_oversized_checkpoints_fail_closed() {
        let (state_path, _) = paths("invalid-checkpoint");
        fs::write(&state_path, b"{invalid").expect("write malformed checkpoint");
        assert!(load_matching_checkpoint(
            &state_path,
            "guest",
            "memory0",
            &"a".repeat(64),
            &"b".repeat(64)
        )
        .is_err());
        fs::write(
            &state_path,
            vec![b'x'; MAX_POLICY_CHECKPOINT_BYTES as usize + 1],
        )
        .expect("write oversized checkpoint");
        assert!(load_matching_checkpoint(
            &state_path,
            "guest",
            "memory0",
            &"a".repeat(64),
            &"b".repeat(64)
        )
        .is_err());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn transport_interruption_preserves_history_while_invalid_evidence_clears_it() {
        let (state_path, attestation_path) = paths("telemetry-failure-history");
        let config = config(&state_path, &attestation_path);
        let compatibility_fingerprint = write_attestation(&attestation_path);
        let mut estimator = TargetEstimatorState::cold();
        estimator.desired_bytes = 8 * GIB;
        estimator.safe_floor_bytes = 6 * GIB;
        estimator.session_id = "session-a".to_owned();
        estimator.last_observed_unix_millis = 1_000_000;
        estimator.last_monotonic_millis = 600_000;
        estimator.last_sequence = 10;
        estimator
            .history
            .push_back(virtio_mem_core::CandidateHistoryEntry {
                observed_unix_millis: 1_000_000,
                desired_now_bytes: 6 * GIB,
                floor_now_bytes: 5 * GIB,
            });
        let checkpoint = PolicyCheckpoint {
            version: POLICY_CHECKPOINT_VERSION,
            vm_name: config.vm_name.clone(),
            device_alias: config.alias.clone(),
            policy_fingerprint_sha256: policy_fingerprint(
                &target_policy_config(&config).expect("policy"),
            )
            .expect("policy fingerprint"),
            compatibility_fingerprint_sha256: compatibility_fingerprint.clone(),
            estimator: estimator.clone(),
            actuation_latched: false,
            actuation_latch_reason: None,
            command_intent: None,
            last_latch_clear_reason: None,
            accepted_telemetry: AcceptedTelemetry::default(),
            retired_pressure_shadow: RetiredPressureShadow,
        };
        persist_checkpoint(&state_path, &checkpoint).expect("checkpoint");
        let live = VirtioMemState {
            size_bytes: 24 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 8 * GIB,
            current_bytes: 8 * GIB,
        };

        let interrupted = TargetDemandSource::with_compatibility_fingerprint(
            FailingRaw(RawTelemetryError::TransportUnavailable(
                "QGA file is temporarily shared".to_owned(),
            )),
            &config,
            compatibility_fingerprint.clone(),
        )
        .expect("interrupted source");
        assert!(interrupted.evaluate(live).is_err());
        let preserved = load_matching_checkpoint(
            &state_path,
            &config.vm_name,
            &config.alias,
            &checkpoint.policy_fingerprint_sha256,
            &compatibility_fingerprint,
        )
        .expect("load preserved checkpoint")
        .expect("matching checkpoint");
        assert_eq!(preserved.estimator, estimator);

        let invalid = TargetDemandSource::with_compatibility_fingerprint(
            FailingRaw(RawTelemetryError::InvalidEvidence(
                "malformed telemetry".to_owned(),
            )),
            &config,
            compatibility_fingerprint.clone(),
        )
        .expect("invalid source");
        assert!(invalid.evaluate(live).is_err());
        let cleared = load_matching_checkpoint(
            &state_path,
            &config.vm_name,
            &config.alias,
            &checkpoint.policy_fingerprint_sha256,
            &compatibility_fingerprint,
        )
        .expect("load cleared checkpoint")
        .expect("matching checkpoint");
        assert!(cleared.estimator.history.is_empty());
        assert_eq!(cleared.estimator.safe_floor_bytes, 8 * GIB);

        fs::remove_file(state_path).expect("remove checkpoint");
        fs::remove_file(attestation_path).expect("remove attestation");
    }

    #[test]
    fn policy_fingerprint_changes_with_reserve() {
        let (state, attestation) = paths("fingerprint");
        let first = target_policy_config(&config(&state, &attestation)).expect("policy");
        let mut second = first;
        second.physical_reserve_bytes += 2 * MIB;
        assert_ne!(policy_fingerprint(&first), policy_fingerprint(&second));
    }

    #[test]
    fn raw_source_uses_absolute_target_and_persists_checkpoint() {
        let (state_path, attestation) = paths("raw-source");
        let config = config(&state_path, &attestation);
        let memory = MemoryTelemetrySnapshot {
            physical_total_bytes: 16 * GIB,
            physical_available_bytes: 0,
            memory_load_percent: 100,
            commit_total_bytes: 32 * GIB,
            commit_limit_bytes: 32 * GIB,
            commit_peak_bytes: 32 * GIB,
            system_cache_bytes: 0,
            kernel_paged_bytes: 0,
            kernel_nonpaged_bytes: 0,
        };
        let raw = OneEnvelope(Mutex::new(Some(RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000_000,
            10,
            0,
            memory,
        ))));
        let source =
            TargetDemandSource::with_compatibility_fingerprint(raw, &config, "a".repeat(64))
                .expect("construct target source");
        let estimate = source
            .evaluate(VirtioMemState {
                size_bytes: 24 * GIB,
                block_size_bytes: 2 * MIB,
                requested_bytes: 8 * GIB,
                current_bytes: 8 * GIB,
            })
            .expect("evaluate target");
        assert_eq!(
            estimate,
            DemandEstimate {
                safe_floor_bytes: 8 * GIB,
                desired_bytes: 10 * GIB,
                effective_maximum_bytes: 23 * GIB,
                history_ready: false,
                telemetry_identity: Some("session-a:0:10:1000000".to_owned()),
            }
        );
        assert!(state_path.is_file());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn unchanged_atomic_snapshot_waits_without_resetting_or_actuating() {
        let (state_path, attestation) = paths("unchanged-raw-source");
        let config = config(&state_path, &attestation);
        let source = TargetDemandSource::with_compatibility_fingerprint(
            RepeatingEnvelope(pressured_envelope()),
            &config,
            "a".repeat(64),
        )
        .expect("construct target source");
        let first = source.evaluate(eight_gib_live()).expect("first record");
        assert!(first.telemetry_identity.is_some());

        let unchanged = source
            .evaluate(eight_gib_live())
            .expect("unchanged snapshot is a normal wait state");
        assert!(unchanged.telemetry_identity.is_none());
        assert!(!unchanged.history_ready);
        assert_eq!(unchanged.desired_bytes, first.desired_bytes);
        assert_eq!(unchanged.safe_floor_bytes, first.safe_floor_bytes);
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn accepted_record_is_recognised_as_unchanged_after_restart() {
        let (state_path, attestation) = paths("accepted-restart");
        let config = config(&state_path, &attestation);
        let construct = || {
            TargetDemandSource::with_compatibility_fingerprint(
                RepeatingEnvelope(pressured_envelope()),
                &config,
                "a".repeat(64),
            )
            .expect("construct target source")
        };
        assert!(construct()
            .evaluate(eight_gib_live())
            .expect("first record")
            .telemetry_identity
            .is_some());
        assert!(construct()
            .evaluate(eight_gib_live())
            .expect("restarted read")
            .telemetry_identity
            .is_none());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn failed_checkpoint_write_does_not_accept_the_record() {
        let (state_path, attestation) = paths("unwritable-checkpoint");
        let config = config(&state_path, &attestation);
        let source = TargetDemandSource::with_compatibility_fingerprint(
            RepeatingEnvelope(pressured_envelope()),
            &config,
            "a".repeat(64),
        )
        .expect("construct target source");
        // A directory where the checkpoint belongs makes the atomic rename fail.
        fs::create_dir(&state_path).expect("block checkpoint path");
        assert!(source.evaluate(eight_gib_live()).is_err());
        assert!(
            source.evaluate(eight_gib_live()).is_err(),
            "an unpersisted record must not read back as accepted and unchanged"
        );

        fs::remove_dir(&state_path).expect("unblock checkpoint path");
        assert!(source
            .evaluate(eight_gib_live())
            .expect("record accepted once durable")
            .telemetry_identity
            .is_some());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn control_state_is_stamped_persisted_and_restored_across_restart() {
        let (state_path, attestation) = paths("intent");
        let config = config(&state_path, &attestation);
        let source = TargetDemandSource::with_compatibility_fingerprint(
            OneEnvelope(Mutex::new(None)),
            &config,
            "a".repeat(64),
        )
        .expect("construct target source");
        let unstamped = CommandIntent {
            operation_id: "operation-1".to_owned(),
            direction: virtio_mem_core::ReconcileDirection::Shrink,
            prior_requested_bytes: 8 * GIB,
            prior_current_bytes: 8 * GIB,
            target_bytes: 8 * GIB - 64 * MIB,
            telemetry_identity: "session:1:2:3".to_owned(),
            policy_fingerprint_sha256: String::new(),
            compatibility_fingerprint_sha256: String::new(),
        };
        let stamped = CommandIntent {
            policy_fingerprint_sha256: policy_fingerprint(
                &target_policy_config(&config).expect("policy"),
            )
            .expect("policy fingerprint"),
            compatibility_fingerprint_sha256: "a".repeat(64),
            ..unstamped.clone()
        };

        source
            .save_control_state(&DurableControlState {
                latch_reason: None,
                intent: Some(unstamped),
            })
            .expect("write intent");
        let restored = || {
            TargetDemandSource::with_compatibility_fingerprint(
                OneEnvelope(Mutex::new(None)),
                &config,
                "a".repeat(64),
            )
            .expect("restore target source")
            .control_state()
            .expect("restored control state")
        };
        assert_eq!(restored().intent, Some(stamped.clone()));

        source
            .save_control_state(&DurableControlState {
                latch_reason: Some("recorded command was not applied".to_owned()),
                intent: Some(stamped),
            })
            .expect("write latch");
        let latched = restored();
        assert_eq!(
            latched.latch_reason.as_deref(),
            Some("recorded command was not applied")
        );

        source
            .save_control_state(&DurableControlState::default())
            .expect("clear control state");
        assert_eq!(restored(), DurableControlState::default());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn clear_latch_is_dry_run_by_default_and_refuses_divergence() {
        let (state_path, attestation_path) = paths("clear-latch");
        let config = config(&state_path, &attestation_path);
        let compatibility_fingerprint = write_attestation(&attestation_path);
        let checkpoint = PolicyCheckpoint {
            version: POLICY_CHECKPOINT_VERSION,
            vm_name: config.vm_name.clone(),
            device_alias: config.alias.clone(),
            policy_fingerprint_sha256: policy_fingerprint(
                &target_policy_config(&config).expect("policy"),
            )
            .expect("policy fingerprint"),
            compatibility_fingerprint_sha256: compatibility_fingerprint,
            estimator: TargetEstimatorState::cold(),
            actuation_latched: true,
            actuation_latch_reason: Some("ambiguous command".to_owned()),
            command_intent: None,
            last_latch_clear_reason: None,
            accepted_telemetry: AcceptedTelemetry::default(),
            retired_pressure_shadow: RetiredPressureShadow,
        };
        persist_checkpoint(&state_path, &checkpoint).expect("checkpoint");
        let converged = VirtioMemState {
            size_bytes: 24 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 8 * GIB,
            current_bytes: 8 * GIB,
        };

        clear_actuation_latch(&config, converged, "operator reviewed", false).expect("dry run");
        assert!(
            load_matching_checkpoint(
                &state_path,
                &config.vm_name,
                &config.alias,
                &checkpoint.policy_fingerprint_sha256,
                &checkpoint.compatibility_fingerprint_sha256,
            )
            .expect("reload")
            .expect("checkpoint")
            .actuation_latched
        );
        assert!(clear_actuation_latch(
            &config,
            VirtioMemState {
                requested_bytes: 8 * GIB - 64 * MIB,
                ..converged
            },
            "operator reviewed",
            true,
        )
        .is_err());
        clear_actuation_latch(&config, converged, "operator reviewed", true).expect("apply clear");
        let cleared = load_matching_checkpoint(
            &state_path,
            &config.vm_name,
            &config.alias,
            &checkpoint.policy_fingerprint_sha256,
            &checkpoint.compatibility_fingerprint_sha256,
        )
        .expect("reload")
        .expect("checkpoint");
        assert!(!cleared.actuation_latched);
        assert_eq!(
            cleared.last_latch_clear_reason.as_deref(),
            Some("operator reviewed")
        );
        fs::remove_file(state_path).expect("remove checkpoint");
        fs::remove_file(attestation_path).expect("remove attestation");
    }

    #[test]
    fn clear_latch_migrates_a_live_valid_compatibility_fingerprint_and_clears_history() {
        let (state_path, attestation_path) = paths("clear-latch-compatibility-migration");
        let config = config(&state_path, &attestation_path);
        let old_compatibility_fingerprint = write_attestation_with_domain(&attestation_path, 'a');
        let mut estimator = TargetEstimatorState::cold();
        estimator.session_id = "old-session".to_owned();
        estimator.last_sequence = 4;
        let checkpoint = PolicyCheckpoint {
            version: POLICY_CHECKPOINT_VERSION,
            vm_name: config.vm_name.clone(),
            device_alias: config.alias.clone(),
            policy_fingerprint_sha256: policy_fingerprint(
                &target_policy_config(&config).expect("policy"),
            )
            .expect("policy fingerprint"),
            compatibility_fingerprint_sha256: old_compatibility_fingerprint,
            estimator,
            actuation_latched: true,
            actuation_latch_reason: Some("attestation changed before command".to_owned()),
            command_intent: None,
            last_latch_clear_reason: None,
            accepted_telemetry: AcceptedTelemetry::default(),
            retired_pressure_shadow: RetiredPressureShadow,
        };
        persist_checkpoint(&state_path, &checkpoint).expect("checkpoint");
        let new_compatibility_fingerprint = write_attestation_with_domain(&attestation_path, 'd');
        let converged = VirtioMemState {
            size_bytes: 24 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 8 * GIB,
            current_bytes: 8 * GIB,
        };

        clear_actuation_latch(&config, converged, "reviewed VM restart attestation", true)
            .expect("apply compatibility migration");

        let cleared = load_matching_checkpoint(
            &state_path,
            &config.vm_name,
            &config.alias,
            &checkpoint.policy_fingerprint_sha256,
            &new_compatibility_fingerprint,
        )
        .expect("reload")
        .expect("migrated checkpoint");
        assert!(!cleared.actuation_latched);
        assert!(cleared.command_intent.is_none());
        assert_eq!(cleared.estimator, TargetEstimatorState::cold());
        assert_eq!(
            cleared.last_latch_clear_reason.as_deref(),
            Some("reviewed VM restart attestation")
        );
        fs::remove_file(state_path).expect("remove checkpoint");
        fs::remove_file(attestation_path).expect("remove attestation");
    }

    #[test]
    fn status_reads_cold_checkpoint_without_writing_or_actuating() {
        let (state_path, attestation_path) = paths("status-cold");
        let config = config(&state_path, &attestation_path);
        write_attestation(&attestation_path);
        let live = VirtioMemState {
            size_bytes: 24 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 8 * GIB,
            current_bytes: 8 * GIB,
        };

        let status = read_controller_status_at(&config, live, 2_000_000)
            .expect("cold status must remain readable");

        assert_eq!(status.version, CONTROLLER_STATUS_VERSION);
        assert_eq!(status.observed_unix_millis, 2_000_000);
        assert_eq!(status.desired_bytes, 8 * GIB);
        assert_eq!(status.safe_floor_bytes, 8 * GIB);
        assert_eq!(status.effective_maximum_bytes, 23 * GIB);
        assert_eq!(status.accepted_telemetry, None);
        assert_eq!(status.reclaim_readiness, ReclaimReadiness::Cold);
        assert_eq!(status.capacity_state, CapacityState::Unknown);
        assert_eq!(status.command_ownership, CommandOwnership::None);
        assert_eq!(status.control_health, ControlHealth::Converged);
        assert_eq!(status.recovery_state, RecoveryState::NotRequired);
        assert!(!state_path.exists(), "status must not create a checkpoint");
        fs::remove_file(attestation_path).expect("remove attestation");
    }

    #[test]
    fn status_exposes_accepted_identity_ready_history_and_capacity_ceiling() {
        let (state_path, attestation_path) = paths("status-populated");
        let config = config(&state_path, &attestation_path);
        let compatibility_fingerprint_sha256 = write_attestation(&attestation_path);
        let mut estimator = TargetEstimatorState::cold();
        estimator.desired_bytes = 20 * GIB;
        estimator.safe_floor_bytes = 19 * GIB;
        estimator.session_id = "session-status".to_owned();
        estimator.last_observed_unix_millis = 1_600_000;
        estimator.last_monotonic_millis = 600_000;
        estimator.last_sequence = 10;
        for index in 0..=10 {
            estimator
                .history
                .push_back(virtio_mem_core::CandidateHistoryEntry {
                    observed_unix_millis: 1_000_000 + index * 60_000,
                    desired_now_bytes: 20 * GIB,
                    floor_now_bytes: 19 * GIB,
                });
        }
        let checkpoint = PolicyCheckpoint {
            version: POLICY_CHECKPOINT_VERSION,
            vm_name: config.vm_name.clone(),
            device_alias: config.alias.clone(),
            policy_fingerprint_sha256: policy_fingerprint(
                &target_policy_config(&config).expect("policy"),
            )
            .expect("policy fingerprint"),
            compatibility_fingerprint_sha256,
            estimator,
            actuation_latched: false,
            actuation_latch_reason: None,
            command_intent: None,
            last_latch_clear_reason: Some("prior review".to_owned()),
            accepted_telemetry: AcceptedTelemetry::default(),
            retired_pressure_shadow: RetiredPressureShadow,
        };
        persist_checkpoint(&state_path, &checkpoint).expect("checkpoint");
        let live = VirtioMemState {
            size_bytes: 21 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 20 * GIB,
            current_bytes: 20 * GIB,
        };

        let status = read_controller_status_at(&config, live, 2_000_000)
            .expect("populated status must be valid");

        assert_eq!(
            status.accepted_telemetry,
            Some(AcceptedTelemetryIdentity {
                session_id: "session-status".to_owned(),
                sequence: 10,
                monotonic_millis: 600_000,
                observed_unix_millis: 1_600_000,
            })
        );
        assert!(status.history_ready);
        assert_eq!(status.reclaim_readiness, ReclaimReadiness::Ready);
        assert_eq!(status.capacity_state, CapacityState::AtEffectiveMaximum);
        assert_eq!(
            status.last_latch_clear_reason.as_deref(),
            Some("prior review")
        );
        fs::remove_file(state_path).expect("remove checkpoint");
        fs::remove_file(attestation_path).expect("remove attestation");
    }

    #[test]
    fn status_classifies_command_ownership_and_recovery_without_resolving_intent() {
        let intent = CommandIntent {
            operation_id: "operation-status".to_owned(),
            direction: virtio_mem_core::ReconcileDirection::Shrink,
            prior_requested_bytes: 8 * GIB,
            prior_current_bytes: 8 * GIB,
            target_bytes: 6 * GIB,
            telemetry_identity: "session:4:40:1000".to_owned(),
            policy_fingerprint_sha256: "a".repeat(64),
            compatibility_fingerprint_sha256: "b".repeat(64),
        };
        let live = |requested_bytes, current_bytes| VirtioMemState {
            size_bytes: 24 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes,
            current_bytes,
        };

        assert_eq!(
            command_ownership(Some(&intent), live(6 * GIB, 7 * GIB)),
            CommandOwnership::OwnedPending
        );
        assert_eq!(
            command_ownership(Some(&intent), live(6 * GIB, 6 * GIB)),
            CommandOwnership::OwnedConvergedUnresolved
        );
        assert_eq!(
            command_ownership(Some(&intent), live(8 * GIB, 8 * GIB)),
            CommandOwnership::RecordedNotApplied
        );
        assert_eq!(
            command_ownership(Some(&intent), live(7 * GIB, 8 * GIB)),
            CommandOwnership::Conflict
        );
        assert_eq!(
            command_ownership(None, live(7 * GIB, 8 * GIB)),
            CommandOwnership::UnownedPending
        );
        assert!(recovery_reason(CommandOwnership::OwnedPending, None).is_none());
        assert!(recovery_reason(CommandOwnership::Conflict, None).is_some());
    }
}
