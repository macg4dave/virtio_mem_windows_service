//! Host-owned M10e estimator state, checkpointing, and raw-telemetry join.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use virtio_mem_core::{
    ResizeDecision, TargetEstimator, TargetEstimatorState, TargetGeometry, TargetPolicyConfig,
    TargetSample, VirtioMemState,
};

use crate::attestation::CompatibilityAttestation;
use crate::config::HostConfig;
use crate::raw_telemetry::RawTelemetrySource;
use crate::runtime::{
    CommandIntent, DemandDecision, DemandSource, DurableControlState, IntentResolution,
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
}

pub struct TargetDemandSource<R> {
    raw: R,
    estimator: Mutex<TargetEstimator>,
    checkpoint_path: PathBuf,
    vm_name: String,
    device_alias: String,
    policy_fingerprint: String,
    compatibility_fingerprint: String,
    grow_step_bytes: u64,
    shrink_step_bytes: u64,
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
        let state = checkpoint
            .map(|checkpoint| checkpoint.estimator)
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
            estimator: Mutex::new(estimator),
            checkpoint_path,
            vm_name: config.vm_name.clone(),
            device_alias: config.alias.clone(),
            policy_fingerprint,
            compatibility_fingerprint,
            grow_step_bytes: config.grow_step_bytes,
            shrink_step_bytes: config.shrink_step_bytes,
            control: Mutex::new(control),
        })
    }
}

impl<R: RawTelemetrySource> DemandSource for TargetDemandSource<R> {
    fn evaluate(
        &self,
        state: VirtioMemState,
        _config: &HostConfig,
    ) -> Result<DemandDecision, String> {
        let envelope = match self.raw.read() {
            Ok(envelope) => envelope,
            Err(error) => {
                let mut estimator = self
                    .estimator
                    .lock()
                    .map_err(|_| "target estimator state lock is poisoned".to_owned())?;
                estimator.invalidate_history();
                self.persist_estimator(&estimator)?;
                return Err(error);
            }
        };
        let mut estimator = self
            .estimator
            .lock()
            .map_err(|_| "target estimator state lock is poisoned".to_owned())?;
        let estimate_result = estimator.estimate(
            TargetSample {
                session_id: envelope.session_id,
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
        if let Err(error) = estimate_result {
            self.persist_estimator(&estimator)?;
            return Err(error.to_string());
        }
        let estimate = estimate_result.map_err(|error| error.to_string())?;
        self.persist_estimator(&estimator)?;
        if estimate.capacity_limited {
            eprintln!(
                "target estimator health=capacity_limited desired_now_bytes={} effective_maximum_bytes={}",
                estimate.desired_now_bytes, estimate.effective_maximum_bytes
            );
        }
        let actuation_latched = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?
            .actuation_latched;
        let requested_bytes = if actuation_latched {
            None
        } else if estimate.desired_bytes > state.current_bytes {
            Some(
                state
                    .current_bytes
                    .checked_add(self.grow_step_bytes)
                    .ok_or_else(|| "growth quantum overflowed".to_owned())?
                    .min(estimate.desired_bytes)
                    .min(estimate.effective_maximum_bytes),
            )
        } else if estimate.history_ready && estimate.desired_bytes < state.current_bytes {
            Some(
                state
                    .current_bytes
                    .saturating_sub(self.shrink_step_bytes)
                    .max(estimate.desired_bytes)
                    .max(estimate.safe_floor_bytes),
            )
        } else {
            None
        };
        Ok(DemandDecision {
            decision: requested_bytes
                .map(|requested_bytes| ResizeDecision::Request { requested_bytes })
                .unwrap_or(ResizeDecision::NoChange),
            safe_floor_bytes: estimate.safe_floor_bytes,
            desired_bytes: estimate.desired_bytes,
            effective_maximum_bytes: estimate.effective_maximum_bytes,
            history_ready: estimate.history_ready,
            telemetry_identity: Some(format!(
                "{}:{}:{}:{}",
                estimator.state().session_id,
                estimator.state().last_sequence,
                estimator.state().last_monotonic_millis,
                estimator.state().last_observed_unix_millis
            )),
        })
    }

    fn durable_control_state(&self) -> Result<DurableControlState, String> {
        let control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        Ok(DurableControlState {
            actuation_latched: control.actuation_latched,
            intent: control.command_intent.clone(),
        })
    }

    fn record_command_intent(&self, mut intent: CommandIntent) -> Result<(), String> {
        let estimator = self
            .estimator
            .lock()
            .map_err(|_| "target estimator state lock is poisoned".to_owned())?;
        let mut control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        if control.actuation_latched {
            return Err("automatic actuation is durably latched".to_owned());
        }
        intent.policy_fingerprint_sha256 = self.policy_fingerprint.clone();
        intent.compatibility_fingerprint_sha256 = self.compatibility_fingerprint.clone();
        if let Some(previous) = control.command_intent.as_ref() {
            let valid_upward_replacement = matches!(
                intent.direction,
                virtio_mem_core::ReconcileDirection::SupersedeShrink
                    | virtio_mem_core::ReconcileDirection::FreezeShrink
            ) && matches!(
                previous.direction,
                virtio_mem_core::ReconcileDirection::Shrink
                    | virtio_mem_core::ReconcileDirection::SupersedeShrink
            ) && intent.prior_requested_bytes
                == previous.target_bytes
                && intent.target_bytes > previous.target_bytes
                && intent.target_bytes <= intent.prior_current_bytes;
            if !valid_upward_replacement {
                return Err("a durable command intent is already pending".to_owned());
            }
        }
        control.command_intent = Some(intent);
        self.persist_state(&estimator, &control)
    }

    fn resolve_command_intent(&self, live: VirtioMemState) -> Result<IntentResolution, String> {
        let estimator = self
            .estimator
            .lock()
            .map_err(|_| "target estimator state lock is poisoned".to_owned())?;
        let mut control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        let Some(intent) = control.command_intent.clone() else {
            return Ok(IntentResolution::None);
        };
        let resolution = if live.requested_bytes == intent.target_bytes {
            if live.current_bytes == intent.target_bytes {
                control.command_intent = None;
                IntentResolution::Converged
            } else {
                IntentResolution::Accepted
            }
        } else if live.requested_bytes == intent.prior_requested_bytes
            && live.current_bytes == intent.prior_current_bytes
        {
            control.actuation_latched = true;
            control.actuation_latch_reason = Some("recorded command was not applied".to_owned());
            IntentResolution::NotAppliedLatched
        } else {
            control.actuation_latched = true;
            control.actuation_latch_reason = Some("recorded command result is unknown".to_owned());
            IntentResolution::CommandUnknownLatched
        };
        self.persist_state(&estimator, &control)?;
        Ok(resolution)
    }

    fn latch_actuation(&self, reason: &str) -> Result<(), String> {
        let estimator = self
            .estimator
            .lock()
            .map_err(|_| "target estimator state lock is poisoned".to_owned())?;
        let mut control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        control.actuation_latched = true;
        control.actuation_latch_reason = Some(reason.to_owned());
        self.persist_state(&estimator, &control)
    }
}

impl<R> TargetDemandSource<R> {
    fn persist_estimator(&self, estimator: &TargetEstimator) -> Result<(), String> {
        let control = self
            .control
            .lock()
            .map_err(|_| "target control-state lock is poisoned".to_owned())?;
        self.persist_state(estimator, &control)
    }

    fn persist_state(
        &self,
        estimator: &TargetEstimator,
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
                estimator: estimator.state().clone(),
                actuation_latched: control.actuation_latched,
                actuation_latch_reason: control.actuation_latch_reason.clone(),
                command_intent: control.command_intent.clone(),
                last_latch_clear_reason: control.last_latch_clear_reason.clone(),
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
    let mut checkpoint = load_matching_checkpoint(
        &path,
        &config.vm_name,
        &config.alias,
        &policy_fingerprint,
        &compatibility_fingerprint,
    )?
    .ok_or_else(|| "no matching target-controller checkpoint exists".to_owned())?;
    if !checkpoint.actuation_latched {
        return Err("target-controller actuation is not latched".to_owned());
    }
    let message = format!(
        "clear_latch vm={} alias={} requested_bytes={} current_bytes={} reason={}",
        config.vm_name, config.alias, live.requested_bytes, live.current_bytes, reason
    );
    if apply {
        checkpoint.actuation_latched = false;
        checkpoint.actuation_latch_reason = None;
        checkpoint.command_intent = None;
        checkpoint.last_latch_clear_reason = Some(reason.to_owned());
        persist_checkpoint(&path, &checkpoint)?;
    }
    Ok(message)
}

fn target_policy_config(config: &HostConfig) -> Result<TargetPolicyConfig, String> {
    Ok(TargetPolicyConfig {
        configured_minimum_bytes: config.min_memory_bytes,
        configured_maximum_bytes: config.max_memory_bytes,
        fixed_visible_base_bytes: config.fixed_visible_base_bytes,
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
        eprintln!("target policy checkpoint is oversized; starting with cold reclaim history");
        return Ok(None);
    }
    let bytes =
        fs::read(path).map_err(|error| format!("read target policy checkpoint: {error}"))?;
    let checkpoint: PolicyCheckpoint = match serde_json::from_slice(&bytes) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            eprintln!(
                "target policy checkpoint is invalid; starting with cold reclaim history: {error}"
            );
            return Ok(None);
        }
    };
    if checkpoint.version != POLICY_CHECKPOINT_VERSION
        || checkpoint.estimator.version != virtio_mem_core::TARGET_ESTIMATOR_STATE_VERSION
        || checkpoint.vm_name != vm_name
        || checkpoint.device_alias != device_alias
        || checkpoint.policy_fingerprint_sha256 != policy_fingerprint
        || checkpoint.compatibility_fingerprint_sha256 != compatibility_fingerprint
    {
        return Ok(None);
    }
    if checkpoint.command_intent.as_ref().is_some_and(|intent| {
        intent.operation_id.trim().is_empty()
            || intent.telemetry_identity.trim().is_empty()
            || intent.policy_fingerprint_sha256 != checkpoint.policy_fingerprint_sha256
            || intent.compatibility_fingerprint_sha256
                != checkpoint.compatibility_fingerprint_sha256
    }) {
        eprintln!("target policy command intent is invalid; starting with cold reclaim history");
        return Ok(None);
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
    fs::write(&temporary, bytes)
        .map_err(|error| format!("write target policy checkpoint: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("publish target policy checkpoint: {error}")
    })
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
    use crate::config::{DemandSourceMode, StatsSource};
    use serde_json::json;
    use std::time::Duration;
    use virtio_mem_core::{MemoryTelemetrySnapshot, RawTelemetryEnvelope};

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    struct OneEnvelope(Mutex<Option<RawTelemetryEnvelope>>);

    impl RawTelemetrySource for OneEnvelope {
        fn read(&self) -> Result<RawTelemetryEnvelope, String> {
            self.0
                .lock()
                .map_err(|_| "fixture lock poisoned".to_owned())?
                .take()
                .ok_or_else(|| "fixture exhausted".to_owned())
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
        let document = CompatibilityAttestation::new(
            LiveCompatibilityEvidence {
                vm_name: "guest".to_owned(),
                device_alias: "memory0".to_owned(),
                domain_xml_sha256: "a".repeat(64),
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
            min_memory_bytes: 2 * GIB,
            max_memory_bytes: 20 * GIB,
            lower_threshold_bytes: GIB,
            upper_threshold_bytes: 3 * GIB,
            grow_step_bytes: GIB,
            shrink_step_bytes: 64 * MIB,
            fixed_visible_base_bytes: 8 * GIB,
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
            stats_source: StatsSource::DomMemStat,
            demand_source: DemandSourceMode::Raw,
            stats_max_age: Duration::from_secs(60),
            stats_future_tolerance: Duration::from_secs(5),
            raw_telemetry_path: "telemetry.jsonl".to_owned(),
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 4 * GIB,
            compatibility_attestation_path: attestation.display().to_string(),
            automatic_windows_shrink: true,
            shrink_renotification: false,
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
            Some(checkpoint)
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
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn malformed_and_oversized_checkpoints_restart_cold() {
        let (state_path, _) = paths("invalid-checkpoint");
        fs::write(&state_path, b"{invalid").expect("write malformed checkpoint");
        assert!(load_matching_checkpoint(
            &state_path,
            "guest",
            "memory0",
            &"a".repeat(64),
            &"b".repeat(64)
        )
        .expect("malformed checkpoint becomes cold state")
        .is_none());
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
        .expect("oversized checkpoint becomes cold state")
        .is_none());
        fs::remove_file(state_path).expect("remove checkpoint");
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
        let decision = source
            .evaluate(
                VirtioMemState {
                    size_bytes: 24 * GIB,
                    block_size_bytes: 2 * MIB,
                    requested_bytes: 8 * GIB,
                    current_bytes: 8 * GIB,
                },
                &config,
            )
            .expect("evaluate target");
        assert_eq!(
            decision,
            DemandDecision {
                decision: ResizeDecision::Request {
                    requested_bytes: 9 * GIB,
                },
                safe_floor_bytes: 8 * GIB,
                desired_bytes: 10 * GIB,
                effective_maximum_bytes: 20 * GIB,
                history_ready: false,
                telemetry_identity: Some("session-a:0:10:1000000".to_owned()),
            }
        );
        assert!(state_path.is_file());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn command_intent_is_written_before_resolution_and_never_replayed() {
        let (state_path, attestation) = paths("intent");
        let config = config(&state_path, &attestation);
        let source = TargetDemandSource::with_compatibility_fingerprint(
            OneEnvelope(Mutex::new(None)),
            &config,
            "a".repeat(64),
        )
        .expect("construct target source");
        let intent = CommandIntent {
            operation_id: "operation-1".to_owned(),
            direction: virtio_mem_core::ReconcileDirection::Shrink,
            prior_requested_bytes: 8 * GIB,
            prior_current_bytes: 8 * GIB,
            target_bytes: 8 * GIB - 64 * MIB,
            telemetry_identity: "session:1:2:3".to_owned(),
            policy_fingerprint_sha256: policy_fingerprint(
                &target_policy_config(&config).expect("policy"),
            )
            .expect("policy fingerprint"),
            compatibility_fingerprint_sha256: "a".repeat(64),
        };

        source
            .record_command_intent(intent.clone())
            .expect("write intent");
        assert_eq!(
            source
                .durable_control_state()
                .expect("control state")
                .intent,
            Some(intent)
        );
        assert_eq!(
            source
                .resolve_command_intent(VirtioMemState {
                    size_bytes: 24 * GIB,
                    block_size_bytes: 2 * MIB,
                    requested_bytes: 8 * GIB - 64 * MIB,
                    current_bytes: 8 * GIB,
                })
                .expect("accepted command"),
            IntentResolution::Accepted
        );

        let restarted = TargetDemandSource::with_compatibility_fingerprint(
            OneEnvelope(Mutex::new(None)),
            &config,
            "a".repeat(64),
        )
        .expect("restore target source");
        assert!(restarted
            .durable_control_state()
            .expect("restored control state")
            .intent
            .is_some());
        assert_eq!(
            restarted
                .resolve_command_intent(VirtioMemState {
                    size_bytes: 24 * GIB,
                    block_size_bytes: 2 * MIB,
                    requested_bytes: 8 * GIB - 64 * MIB,
                    current_bytes: 8 * GIB - 64 * MIB,
                })
                .expect("resolve convergence"),
            IntentResolution::Converged
        );
        assert!(restarted
            .durable_control_state()
            .expect("cleared control state")
            .intent
            .is_none());
        fs::remove_file(state_path).expect("remove checkpoint");
    }

    #[test]
    fn unchanged_or_unknown_command_results_latch_durably() {
        let (state_path, attestation) = paths("latch");
        let config = config(&state_path, &attestation);
        let source = TargetDemandSource::with_compatibility_fingerprint(
            OneEnvelope(Mutex::new(None)),
            &config,
            "a".repeat(64),
        )
        .expect("construct target source");
        source
            .record_command_intent(CommandIntent {
                operation_id: "operation-1".to_owned(),
                direction: virtio_mem_core::ReconcileDirection::Shrink,
                prior_requested_bytes: 8 * GIB,
                prior_current_bytes: 8 * GIB,
                target_bytes: 8 * GIB - 64 * MIB,
                telemetry_identity: "session:1:2:3".to_owned(),
                policy_fingerprint_sha256: String::new(),
                compatibility_fingerprint_sha256: String::new(),
            })
            .expect("write intent");
        assert_eq!(
            source
                .resolve_command_intent(VirtioMemState {
                    size_bytes: 24 * GIB,
                    block_size_bytes: 2 * MIB,
                    requested_bytes: 8 * GIB,
                    current_bytes: 8 * GIB,
                })
                .expect("resolve unchanged state"),
            IntentResolution::NotAppliedLatched
        );
        assert!(
            source
                .durable_control_state()
                .expect("latched state")
                .actuation_latched
        );

        let restarted = TargetDemandSource::with_compatibility_fingerprint(
            OneEnvelope(Mutex::new(None)),
            &config,
            "a".repeat(64),
        )
        .expect("restore target source");
        assert!(
            restarted
                .durable_control_state()
                .expect("restored latch")
                .actuation_latched
        );
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
}
