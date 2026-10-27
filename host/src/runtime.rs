//! The host control loop. The runtime is the single owner of operation
//! lifecycle: it reads the durable control state each poll, resolves any
//! recorded command against live state, and asks the pure reconciler for the
//! next action. Demand sources only estimate; stores only persist.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use virtio_mem_core::{
    reconcile, CommandOwnership, ReconcileAction, ReconcileDirection, ReconcileInput,
    VirtioMemState,
};

use crate::config::HostConfig;
use crate::host_memory::{validate_grow_headroom, HostMemorySource};

/// One demand evaluation. It carries no action: the reconciler chooses actions
/// from this estimate and the live device state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemandEstimate {
    pub desired_bytes: u64,
    pub safe_floor_bytes: u64,
    pub effective_maximum_bytes: u64,
    pub history_ready: bool,
    /// Identity of a newly accepted telemetry sample; `None` when the sample
    /// is unchanged since the last evaluation.
    pub telemetry_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandIntent {
    pub operation_id: String,
    pub direction: virtio_mem_core::ReconcileDirection,
    pub prior_requested_bytes: u64,
    pub prior_current_bytes: u64,
    pub target_bytes: u64,
    pub telemetry_identity: String,
    pub policy_fingerprint_sha256: String,
    pub compatibility_fingerprint_sha256: String,
}

/// Control state that must survive a restart. Actuation is latched exactly
/// when `latch_reason` is set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DurableControlState {
    pub latch_reason: Option<String>,
    pub intent: Option<CommandIntent>,
}

impl DurableControlState {
    pub fn latched(&self) -> bool {
        self.latch_reason.is_some()
    }
}

pub trait DemandSource {
    fn evaluate(&self, state: VirtioMemState) -> Result<DemandEstimate, String>;

    fn control_state(&self) -> Result<DurableControlState, String>;

    /// Persist control state durably before returning. The store stamps each
    /// intent with the policy and compatibility identity that produced it.
    fn save_control_state(&self, control: &DurableControlState) -> Result<(), String>;
}

/// How live state relates to the recorded command, if any. Shared by the
/// control loop and the read-only status report.
pub fn command_ownership(intent: Option<&CommandIntent>, live: VirtioMemState) -> CommandOwnership {
    let Some(intent) = intent else {
        return if live.requested_bytes == live.current_bytes {
            CommandOwnership::None
        } else {
            CommandOwnership::UnownedPending
        };
    };
    if live.requested_bytes == intent.target_bytes {
        if live.current_bytes == intent.target_bytes {
            CommandOwnership::OwnedConvergedUnresolved
        } else {
            CommandOwnership::OwnedPending
        }
    } else if live.requested_bytes == intent.prior_requested_bytes
        && live.current_bytes == intent.prior_current_bytes
    {
        CommandOwnership::RecordedNotApplied
    } else {
        CommandOwnership::Conflict
    }
}

/// A new intent may replace a pending one only to raise a pending shrink's
/// target toward current allocation (supersession or freeze).
fn valid_intent_replacement(previous: &CommandIntent, next: &CommandIntent) -> bool {
    matches!(
        next.direction,
        ReconcileDirection::SupersedeShrink | ReconcileDirection::FreezeShrink
    ) && matches!(
        previous.direction,
        ReconcileDirection::Shrink | ReconcileDirection::SupersedeShrink
    ) && next.prior_requested_bytes == previous.target_bytes
        && next.target_bytes > previous.target_bytes
        && next.target_bytes <= next.prior_current_bytes
}

pub trait MemoryStateSource {
    fn memory_state(&self) -> Result<VirtioMemState, String>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResizeSinkError {
    #[error("resize rejected before command execution: {0}")]
    Rejected(String),
    #[error("resize command outcome is unknown: {0}")]
    CommandUnknown(String),
}

pub trait ResizeSink {
    fn request_resize(&self, requested_bytes: u64) -> Result<(), ResizeSinkError>;

    fn supersede_shrink(
        &self,
        _prior_requested_bytes: u64,
        _prior_current_bytes: u64,
        _target_bytes: u64,
    ) -> Result<(), ResizeSinkError> {
        Err(ResizeSinkError::Rejected(
            "pending-shrink supersession is unsupported by this resize sink".to_owned(),
        ))
    }
}

#[derive(Debug, Error)]
pub enum HostRuntimeError {
    #[error("live virtio-mem state failed validation: {0}")]
    MemoryState(String),
    #[error("memory controller configuration is invalid: {0}")]
    Controller(String),
    #[error("host available-memory check failed: {0}")]
    HostMemory(String),
    #[error("resize request failed: {0}")]
    Resize(String),
    #[error("virtio-mem request did not converge within {0:?}")]
    ConvergenceTimeout(Duration),
}

/// What the loop does after one poll.
enum Step {
    Wait,
    Stop(HostRuntimeError),
}

pub struct HostRuntime<G, S, R, H> {
    demand_source: G,
    state_source: S,
    resize_sink: R,
    host_memory: H,
    config: HostConfig,
}

/// Loop-local bookkeeping. Nothing here grants authority to actuate: that
/// comes only from the durable control state.
#[derive(Default)]
struct LoopState {
    /// When the currently owned operation was first observed in this process.
    /// A restart starts a fresh deadline; the durable latch is what prevents
    /// a restart from resuming actuation.
    pending_since: Option<Instant>,
    operation_counter: u64,
    unowned_divergence_reported: bool,
}

impl LoopState {
    fn next_operation_id(&mut self) -> String {
        self.operation_counter = self.operation_counter.saturating_add(1);
        format!("{}-{}", std::process::id(), self.operation_counter)
    }

    fn pending_elapsed(&mut self, timeout: Duration) -> bool {
        self.pending_since
            .get_or_insert_with(Instant::now)
            .elapsed()
            >= timeout
    }
}

impl<G, S, R, H> HostRuntime<G, S, R, H>
where
    G: DemandSource,
    S: MemoryStateSource,
    R: ResizeSink,
    H: HostMemorySource,
{
    pub fn new(
        demand_source: G,
        state_source: S,
        resize_sink: R,
        host_memory: H,
        config: HostConfig,
    ) -> Self {
        Self {
            demand_source,
            state_source,
            resize_sink,
            host_memory,
            config,
        }
    }

    pub fn run(&self, stop: &AtomicBool) -> Result<(), HostRuntimeError> {
        let mut local = LoopState::default();
        while !stop.load(Ordering::Acquire) {
            if let Step::Stop(error) = self.poll(&mut local)? {
                return Err(error);
            }
            wait_interruptibly(stop, self.config.poll_interval);
        }
        Ok(())
    }

    fn poll(&self, local: &mut LoopState) -> Result<Step, HostRuntimeError> {
        let mut control = self
            .demand_source
            .control_state()
            .map_err(HostRuntimeError::Controller)?;
        let state = match self.live_state() {
            Ok(state) => state,
            Err(error) if control.intent.is_some() => {
                self.latch(
                    &mut control,
                    &format!("live state unavailable during an owned operation: {error}"),
                )?;
                return Ok(Step::Wait);
            }
            Err(error) => return Err(HostRuntimeError::MemoryState(error)),
        };
        self.resolve_intent(&mut control, state)?;
        if self.config.observe_only
            && (control.intent.is_some() || state.requested_bytes != state.current_bytes)
        {
            self.log_observe_only(state, "existing_or_unowned_live_intent");
            return Ok(Step::Wait);
        }
        if state.requested_bytes != state.current_bytes {
            return self.observe_pending(&mut control, state, local);
        }
        local.pending_since = None;
        local.unowned_divergence_reported = false;
        self.decide(&mut control, state, local)
    }

    fn live_state(&self) -> Result<VirtioMemState, String> {
        let state = self.state_source.memory_state()?;
        state.validate().map_err(|error| error.to_string())?;
        Ok(state)
    }

    /// Clear an intent the device has converged to; latch on an intent the
    /// device never applied or now contradicts.
    fn resolve_intent(
        &self,
        control: &mut DurableControlState,
        live: VirtioMemState,
    ) -> Result<(), HostRuntimeError> {
        match command_ownership(control.intent.as_ref(), live) {
            CommandOwnership::None
            | CommandOwnership::UnownedPending
            | CommandOwnership::OwnedPending => Ok(()),
            CommandOwnership::OwnedConvergedUnresolved => {
                control.intent = None;
                self.save(control)
            }
            CommandOwnership::RecordedNotApplied => {
                self.latch(control, "recorded command was not applied")
            }
            CommandOwnership::Conflict => self.latch(control, "recorded command result is unknown"),
        }
    }

    fn observe_pending(
        &self,
        control: &mut DurableControlState,
        state: VirtioMemState,
        local: &mut LoopState,
    ) -> Result<Step, HostRuntimeError> {
        let owned_direction = control
            .intent
            .as_ref()
            .filter(|intent| intent.target_bytes == state.requested_bytes)
            .map(|intent| intent.direction);
        match owned_direction {
            Some(ReconcileDirection::Grow) => {
                if local.pending_elapsed(self.config.convergence_timeout) {
                    return Ok(Step::Stop(
                        self.stop_on_convergence_timeout(control, "growth")?,
                    ));
                }
                eprintln!(
                    "virtio-mem-host: event=control_health health=growing vm={} alias={} requested_bytes={} current_bytes={}",
                    self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
                );
                Ok(Step::Wait)
            }
            Some(
                ReconcileDirection::Shrink
                | ReconcileDirection::SupersedeShrink
                | ReconcileDirection::FreezeShrink,
            ) => self.observe_owned_shrink(control, state, local),
            None => {
                if !local.unowned_divergence_reported {
                    eprintln!(
                        "virtio-mem-host: event=ownership_conflict vm={} alias={} requested_bytes={} current_bytes={} reason=unowned_divergence_replay_suppressed",
                        self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
                    );
                    local.unowned_divergence_reported = true;
                }
                Ok(Step::Wait)
            }
        }
    }

    /// A pending shrink may be raised toward current allocation when demand
    /// returns, or frozen at current allocation when telemetry fails.
    fn observe_owned_shrink(
        &self,
        control: &mut DurableControlState,
        state: VirtioMemState,
        local: &mut LoopState,
    ) -> Result<Step, HostRuntimeError> {
        let elapsed = local.pending_elapsed(self.config.convergence_timeout);
        if control.latched() {
            return self.shrink_deadline(control, state, elapsed, None);
        }
        let estimate = match self.demand_source.evaluate(state) {
            Ok(estimate) => estimate,
            Err(error) => {
                let freeze = CommandIntent {
                    operation_id: local.next_operation_id(),
                    direction: ReconcileDirection::FreezeShrink,
                    prior_requested_bytes: state.requested_bytes,
                    prior_current_bytes: state.current_bytes,
                    target_bytes: state.current_bytes,
                    telemetry_identity: "stale-or-invalid".to_owned(),
                    policy_fingerprint_sha256: String::new(),
                    compatibility_fingerprint_sha256: String::new(),
                };
                let operation_id = freeze.operation_id.clone();
                self.record_intent(control, freeze)?;
                let command = self.resize_sink.supersede_shrink(
                    state.requested_bytes,
                    state.current_bytes,
                    state.current_bytes,
                );
                self.latch(
                    control,
                    &format!("telemetry failed during owned shrink: {error}"),
                )?;
                eprintln!(
                    "virtio-mem-host: event=shrink_frozen operation_id={operation_id} vm={} alias={} target_bytes={} command_result={command:?} reason={error}",
                    self.config.vm_name, self.config.alias, state.current_bytes
                );
                return Ok(Step::Wait);
            }
        };
        let reconciled = reconcile(ReconcileInput {
            desired_bytes: estimate.desired_bytes,
            safe_floor_bytes: estimate.safe_floor_bytes,
            effective_maximum_bytes: estimate.effective_maximum_bytes,
            history_ready: estimate.history_ready,
            // An unchanged sample is not stale; staleness surfaces as an
            // evaluation error and freezes the shrink above.
            telemetry_fresh: true,
            automatic_shrink: self.config.automatic_windows_shrink,
            actuation_latched: false,
            owns_pending_shrink: true,
            pending_constrained: elapsed,
            grow_step_bytes: self.config.grow_step_bytes,
            shrink_step_bytes: self.config.shrink_step_bytes,
            live: state,
        })
        .map_err(|error| HostRuntimeError::Controller(error.to_string()))?;
        let ReconcileAction::Request {
            target_bytes,
            direction: ReconcileDirection::SupersedeShrink,
            ..
        } = reconciled.action
        else {
            return self.shrink_deadline(control, state, elapsed, Some(estimate.desired_bytes));
        };
        let supersede = CommandIntent {
            operation_id: local.next_operation_id(),
            direction: ReconcileDirection::SupersedeShrink,
            prior_requested_bytes: state.requested_bytes,
            prior_current_bytes: state.current_bytes,
            target_bytes,
            telemetry_identity: estimate
                .telemetry_identity
                .unwrap_or_else(|| "unchanged-sample".to_owned()),
            policy_fingerprint_sha256: String::new(),
            compatibility_fingerprint_sha256: String::new(),
        };
        let operation_id = supersede.operation_id.clone();
        self.record_intent(control, supersede)?;
        let command = self.resize_sink.supersede_shrink(
            state.requested_bytes,
            state.current_bytes,
            target_bytes,
        );
        eprintln!(
            "virtio-mem-host: event=shrink_superseded operation_id={operation_id} vm={} alias={} prior_requested_bytes={} current_bytes={} target_bytes={target_bytes} command_result={command:?}",
            self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
        );
        if let Err(error) = command {
            self.latch(
                control,
                &format!("pending-shrink supersession failed: {error}"),
            )?;
        }
        Ok(Step::Wait)
    }

    fn shrink_deadline(
        &self,
        control: &mut DurableControlState,
        state: VirtioMemState,
        elapsed: bool,
        desired_bytes: Option<u64>,
    ) -> Result<Step, HostRuntimeError> {
        if !elapsed {
            return Ok(Step::Wait);
        }
        eprintln!(
            "virtio-mem-host: event=allocation_constrained health=latched vm={} alias={} desired_bytes={desired_bytes:?} requested_bytes={} current_bytes={}",
            self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
        );
        Ok(Step::Stop(
            self.stop_on_convergence_timeout(control, "shrink")?,
        ))
    }

    /// Converged device: evaluate demand and let the reconciler choose.
    fn decide(
        &self,
        control: &mut DurableControlState,
        state: VirtioMemState,
        local: &mut LoopState,
    ) -> Result<Step, HostRuntimeError> {
        let estimate = match self.demand_source.evaluate(state) {
            Ok(estimate) => estimate,
            Err(error) => {
                eprintln!(
                    "virtio-mem-host: event=demand_input_invalid vm={} alias={} reason={error}",
                    self.config.vm_name, self.config.alias
                );
                return Ok(Step::Wait);
            }
        };
        let fresh = estimate.telemetry_identity.is_some();
        let reconciled = reconcile(ReconcileInput {
            desired_bytes: estimate.desired_bytes,
            safe_floor_bytes: estimate.safe_floor_bytes,
            effective_maximum_bytes: estimate.effective_maximum_bytes,
            history_ready: estimate.history_ready,
            telemetry_fresh: fresh,
            automatic_shrink: self.config.automatic_windows_shrink,
            actuation_latched: control.latched(),
            owns_pending_shrink: false,
            pending_constrained: false,
            grow_step_bytes: self.config.grow_step_bytes,
            shrink_step_bytes: self.config.shrink_step_bytes,
            live: state,
        })
        .map_err(|error| HostRuntimeError::Controller(error.to_string()))?;
        eprintln!(
            "virtio-mem-host: event=controller_decision vm={} alias={} desired_bytes={} safe_floor_bytes={} effective_maximum_bytes={} history_ready={} requested_bytes={} current_bytes={} health={:?} action={:?}",
            self.config.vm_name,
            self.config.alias,
            estimate.desired_bytes,
            estimate.safe_floor_bytes,
            estimate.effective_maximum_bytes,
            estimate.history_ready,
            state.requested_bytes,
            state.current_bytes,
            reconciled.health,
            reconciled.action,
        );
        let ReconcileAction::Request {
            target_bytes,
            direction,
            ..
        } = reconciled.action
        else {
            if fresh
                && !control.latched()
                && !self.config.automatic_windows_shrink
                && estimate.history_ready
                && estimate.desired_bytes < state.current_bytes
            {
                eprintln!(
                    "virtio-mem-host: calculated advisory shrink toward {} bytes; automatic Windows shrink is disabled",
                    estimate.desired_bytes
                );
            }
            return Ok(Step::Wait);
        };
        if self.config.observe_only {
            self.log_observe_only(state, "resize_decision_suppressed");
            return Ok(Step::Wait);
        }
        if direction == ReconcileDirection::Grow {
            let host_available = self
                .host_memory
                .available_bytes()
                .map_err(HostRuntimeError::HostMemory)?;
            if let Err(error) = validate_grow_headroom(
                state.current_bytes,
                target_bytes,
                host_available,
                self.config.host_min_headroom_bytes,
            ) {
                eprintln!("virtio-mem-host: blocking grow to {target_bytes} bytes; {error}");
                return Ok(Step::Wait);
            }
        }
        let intent = CommandIntent {
            operation_id: local.next_operation_id(),
            direction,
            prior_requested_bytes: state.requested_bytes,
            prior_current_bytes: state.current_bytes,
            target_bytes,
            telemetry_identity: estimate
                .telemetry_identity
                .unwrap_or_else(|| "unchanged-sample".to_owned()),
            policy_fingerprint_sha256: String::new(),
            compatibility_fingerprint_sha256: String::new(),
        };
        let operation_id = intent.operation_id.clone();
        self.record_intent(control, intent)?;
        if let Err(error) = self.resize_sink.request_resize(target_bytes) {
            // Classify against fresh live state when it is readable; the latch
            // below stops actuation whatever the classification.
            if let Ok(live) = self.live_state() {
                self.resolve_intent(control, live)?;
            }
            self.latch(control, &format!("resize command failed: {error}"))?;
            let outcome = match &error {
                ResizeSinkError::Rejected(_) => "rejected",
                ResizeSinkError::CommandUnknown(_) => "command_unknown",
            };
            eprintln!(
                "virtio-mem-host: event=resize_{outcome} operation_id={operation_id} vm={} alias={} direction={direction:?} target_bytes={target_bytes} requested_bytes={} current_bytes={} reason={error}",
                self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
            );
            // A failed grow stops the daemon; a failed shrink keeps observing.
            return Ok(if direction == ReconcileDirection::Grow {
                Step::Stop(HostRuntimeError::Resize(error.to_string()))
            } else {
                Step::Wait
            });
        }
        local.pending_since = Some(Instant::now());
        eprintln!(
            "virtio-mem-host: event=resize_requested operation_id={operation_id} vm={} alias={} direction={direction:?} target_bytes={target_bytes} requested_bytes={} current_bytes={}",
            self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
        );
        Ok(Step::Wait)
    }

    fn record_intent(
        &self,
        control: &mut DurableControlState,
        intent: CommandIntent,
    ) -> Result<(), HostRuntimeError> {
        if control.latched() {
            return Err(HostRuntimeError::Controller(
                "automatic actuation is durably latched".to_owned(),
            ));
        }
        if let Some(previous) = control.intent.as_ref() {
            if !valid_intent_replacement(previous, &intent) {
                return Err(HostRuntimeError::Controller(
                    "a durable command intent is already pending".to_owned(),
                ));
            }
        }
        control.intent = Some(intent);
        self.save(control)
    }

    /// Latch durably. An existing latch keeps its original reason.
    fn latch(
        &self,
        control: &mut DurableControlState,
        reason: &str,
    ) -> Result<(), HostRuntimeError> {
        if control.latched() {
            return Ok(());
        }
        control.latch_reason = Some(reason.to_owned());
        self.save(control)?;
        eprintln!(
            "virtio-mem-host: event=actuation_latched vm={} alias={} reason={reason}",
            self.config.vm_name, self.config.alias
        );
        Ok(())
    }

    fn save(&self, control: &DurableControlState) -> Result<(), HostRuntimeError> {
        self.demand_source
            .save_control_state(control)
            .map_err(HostRuntimeError::Controller)
    }

    /// Every owned operation that misses its convergence deadline ends the same
    /// way: latch durably so a restart cannot resume actuation, then stop.
    fn stop_on_convergence_timeout(
        &self,
        control: &mut DurableControlState,
        operation: &str,
    ) -> Result<HostRuntimeError, HostRuntimeError> {
        self.latch(
            control,
            &format!("owned {operation} exceeded the convergence deadline"),
        )?;
        Ok(HostRuntimeError::ConvergenceTimeout(
            self.config.convergence_timeout,
        ))
    }

    fn log_observe_only(&self, state: VirtioMemState, reason: &str) {
        eprintln!(
            "virtio-mem-host: event=observe_only_non_actuating vm={} alias={} requested_bytes={} current_bytes={} reason={reason}",
            self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
        );
    }
}

fn wait_interruptibly(stop: &AtomicBool, duration: Duration) {
    let deadline = Instant::now() + duration;
    while !stop.load(Ordering::Acquire) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn config() -> HostConfig {
        HostConfig {
            vm_name: "guest".to_owned(),
            alias: "memory0".to_owned(),
            min_memory_bytes: 4 * GIB,
            max_memory_bytes: 32 * GIB,
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
            policy_state_path: "state.json".to_owned(),
            poll_interval: Duration::from_secs(30),
            command_timeout: Duration::from_secs(10),
            convergence_timeout: Duration::from_secs(300),
            virsh_binary: "virsh".to_owned(),
            raw_telemetry_path: "guest.telemetry.jsonl".to_owned(),
            raw_telemetry_transport: crate::config::RawTelemetryTransport::File,
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 4 * GIB,
            compatibility_attestation_path: "reviewed.json".to_owned(),
            automatic_windows_shrink: false,
            observe_only: false,
        }
    }

    fn automatic_shrink_config() -> HostConfig {
        let mut runtime_config = config();
        runtime_config.automatic_windows_shrink = true;
        runtime_config.poll_interval = Duration::from_millis(1);
        runtime_config
    }

    fn state(requested_bytes: u64, current_bytes: u64) -> VirtioMemState {
        VirtioMemState {
            size_bytes: 40 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes,
            current_bytes,
        }
    }

    /// An estimate from a newly accepted sample.
    fn fresh(desired_bytes: u64, safe_floor_bytes: u64) -> DemandEstimate {
        DemandEstimate {
            desired_bytes,
            safe_floor_bytes,
            effective_maximum_bytes: 32 * GIB,
            history_ready: true,
            telemetry_identity: Some("session:1:10:1000".to_owned()),
        }
    }

    fn intent(
        direction: ReconcileDirection,
        prior: u64,
        current: u64,
        target: u64,
    ) -> CommandIntent {
        CommandIntent {
            operation_id: "recorded".to_owned(),
            direction,
            prior_requested_bytes: prior,
            prior_current_bytes: current,
            target_bytes: target,
            telemetry_identity: "session:1:10:1000".to_owned(),
            policy_fingerprint_sha256: String::new(),
            compatibility_fingerprint_sha256: String::new(),
        }
    }

    type Estimator<'a> = Box<dyn Fn(VirtioMemState) -> Result<DemandEstimate, String> + 'a>;

    /// A demand source whose control state behaves like the durable
    /// checkpoint: it survives across runs of the same fake.
    struct FakeDemand<'a> {
        estimate: Estimator<'a>,
        evaluations: Cell<u32>,
        stop_after: Option<(u32, &'a AtomicBool)>,
        control: RefCell<DurableControlState>,
        events: Rc<RefCell<Vec<String>>>,
    }

    impl<'a> FakeDemand<'a> {
        fn new(estimate: impl Fn(VirtioMemState) -> Result<DemandEstimate, String> + 'a) -> Self {
            Self {
                estimate: Box::new(estimate),
                evaluations: Cell::new(0),
                stop_after: None,
                control: RefCell::new(DurableControlState::default()),
                events: Rc::default(),
            }
        }

        fn stopping_after(mut self, evaluations: u32, stop: &'a AtomicBool) -> Self {
            self.stop_after = Some((evaluations, stop));
            self
        }

        fn with_control(self, control: DurableControlState) -> Self {
            *self.control.borrow_mut() = control;
            self
        }

        fn latched(&self) -> bool {
            self.control.borrow().latched()
        }
    }

    impl DemandSource for &FakeDemand<'_> {
        fn evaluate(&self, state: VirtioMemState) -> Result<DemandEstimate, String> {
            let evaluations = self.evaluations.get().saturating_add(1);
            self.evaluations.set(evaluations);
            if let Some((limit, stop)) = self.stop_after {
                if evaluations >= limit {
                    stop.store(true, Ordering::Release);
                }
            }
            (self.estimate)(state)
        }

        fn control_state(&self) -> Result<DurableControlState, String> {
            Ok(self.control.borrow().clone())
        }

        fn save_control_state(&self, control: &DurableControlState) -> Result<(), String> {
            let previous = self.control.borrow().intent.clone();
            if control.intent != previous {
                if let Some(intent) = &control.intent {
                    self.events
                        .borrow_mut()
                        .push(format!("intent:{}", intent.target_bytes));
                }
            }
            *self.control.borrow_mut() = control.clone();
            Ok(())
        }
    }

    struct ConvergedState;

    impl MemoryStateSource for ConvergedState {
        fn memory_state(&self) -> Result<VirtioMemState, String> {
            Ok(state(16 * GIB, 16 * GIB))
        }
    }

    struct ScriptedState<'a> {
        observations: RefCell<VecDeque<Result<VirtioMemState, String>>>,
        stop: &'a AtomicBool,
    }

    impl<'a> ScriptedState<'a> {
        fn new(
            stop: &'a AtomicBool,
            observations: impl IntoIterator<Item = Result<VirtioMemState, String>>,
        ) -> Self {
            Self {
                observations: RefCell::new(observations.into_iter().collect()),
                stop,
            }
        }
    }

    impl MemoryStateSource for &ScriptedState<'_> {
        fn memory_state(&self) -> Result<VirtioMemState, String> {
            let observation = self
                .observations
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "state script exhausted".to_owned())?;
            if self.observations.borrow().is_empty() {
                self.stop.store(true, Ordering::Release);
            }
            observation
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        requests: RefCell<Vec<u64>>,
        events: Option<Rc<RefCell<Vec<String>>>>,
    }

    impl ResizeSink for &RecordingSink {
        fn request_resize(&self, requested_bytes: u64) -> Result<(), ResizeSinkError> {
            self.requests.borrow_mut().push(requested_bytes);
            Ok(())
        }

        fn supersede_shrink(
            &self,
            prior_requested_bytes: u64,
            prior_current_bytes: u64,
            target_bytes: u64,
        ) -> Result<(), ResizeSinkError> {
            assert_eq!(prior_requested_bytes, 6 * GIB);
            assert_eq!(prior_current_bytes, 8 * GIB);
            if let Some(events) = &self.events {
                events.borrow_mut().push(format!("command:{target_bytes}"));
            }
            Ok(())
        }
    }

    struct AmbiguousSink(Cell<u32>);

    impl ResizeSink for &AmbiguousSink {
        fn request_resize(&self, _requested_bytes: u64) -> Result<(), ResizeSinkError> {
            self.0.set(self.0.get().saturating_add(1));
            Err(ResizeSinkError::CommandUnknown(
                "command timed out".to_owned(),
            ))
        }
    }

    struct UnusedHostMemory;

    impl HostMemorySource for UnusedHostMemory {
        fn available_bytes(&self) -> Result<u64, String> {
            Err("shrink must not query host growth headroom".to_owned())
        }
    }

    struct AmpleHostMemory;

    impl HostMemorySource for AmpleHostMemory {
        fn available_bytes(&self) -> Result<u64, String> {
            Ok(64 * GIB)
        }
    }

    #[test]
    fn returns_immediately_for_a_cancelled_runtime() {
        let stop = AtomicBool::new(true);
        wait_interruptibly(&stop, Duration::from_secs(60));
    }

    #[test]
    fn command_ownership_classifies_live_state_against_the_recorded_intent() {
        let shrink = intent(ReconcileDirection::Shrink, 16 * GIB, 16 * GIB, 15 * GIB);
        let cases = [
            (None, state(16 * GIB, 16 * GIB), CommandOwnership::None),
            (
                None,
                state(15 * GIB, 16 * GIB),
                CommandOwnership::UnownedPending,
            ),
            (
                Some(&shrink),
                state(15 * GIB, 16 * GIB),
                CommandOwnership::OwnedPending,
            ),
            (
                Some(&shrink),
                state(15 * GIB, 15 * GIB),
                CommandOwnership::OwnedConvergedUnresolved,
            ),
            (
                Some(&shrink),
                state(16 * GIB, 16 * GIB),
                CommandOwnership::RecordedNotApplied,
            ),
            (
                Some(&shrink),
                state(14 * GIB, 16 * GIB),
                CommandOwnership::Conflict,
            ),
        ];
        for (recorded, live, expected) in cases {
            assert_eq!(command_ownership(recorded, live), expected, "{live:?}");
        }
    }

    #[test]
    fn only_an_upward_shrink_replacement_may_supersede_a_pending_intent() {
        let pending = intent(ReconcileDirection::Shrink, 8 * GIB, 8 * GIB, 6 * GIB);
        let raise = intent(
            ReconcileDirection::SupersedeShrink,
            6 * GIB,
            8 * GIB,
            7 * GIB,
        );
        assert!(valid_intent_replacement(&pending, &raise));
        let lower = intent(
            ReconcileDirection::SupersedeShrink,
            6 * GIB,
            8 * GIB,
            5 * GIB,
        );
        assert!(!valid_intent_replacement(&pending, &lower));
        let grow = intent(ReconcileDirection::Grow, 6 * GIB, 8 * GIB, 7 * GIB);
        assert!(!valid_intent_replacement(&pending, &grow));
    }

    #[test]
    fn converged_runtime_stays_alive_when_demand_input_is_temporarily_invalid() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|_| Err("telemetry has not advanced".to_owned()))
            .stopping_after(1, &stop);
        let sink = AmbiguousSink(Cell::new(0));
        let runtime = HostRuntime::new(&demand, ConvergedState, &sink, UnusedHostMemory, config());

        runtime
            .run(&stop)
            .expect("invalid demand fails closed without restarting the service");
        assert_eq!(sink.0.get(), 0);
    }

    #[test]
    fn unchanged_sample_never_requests_a_resize() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|state: VirtioMemState| {
            Ok(DemandEstimate {
                telemetry_identity: None,
                ..fresh(state.current_bytes + GIB, state.current_bytes)
            })
        })
        .stopping_after(1, &stop);
        let sink = RecordingSink::default();
        let runtime = HostRuntime::new(&demand, ConvergedState, &sink, AmpleHostMemory, config());

        runtime
            .run(&stop)
            .expect("unchanged sample is not an error");
        assert!(sink.requests.borrow().is_empty());
    }

    #[test]
    fn empty_device_grows_before_history_establishes_a_floor() {
        // With nothing plugged and no history, the estimator's safe floor is
        // min(current, desired) = 0. That is a valid input, not an error.
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|_| Ok(fresh(4 * GIB, 0))).stopping_after(1, &stop);
        let source = ScriptedState::new(&stop, [Ok(state(0, 0)), Ok(state(GIB, 0))]);
        let sink = RecordingSink::default();
        let runtime = HostRuntime::new(&demand, &source, &sink, AmpleHostMemory, config());

        runtime.run(&stop).expect("an empty device can grow");
        assert_eq!(sink.requests.borrow().as_slice(), [GIB]);
    }

    #[test]
    fn observe_only_runtime_blocks_a_resize_decision_before_the_sink() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|state: VirtioMemState| {
            Ok(fresh(state.current_bytes + GIB, state.current_bytes))
        })
        .stopping_after(1, &stop);
        let sink = RecordingSink::default();
        let mut runtime_config = config();
        runtime_config.observe_only = true;
        runtime_config.poll_interval = Duration::from_millis(1);
        let runtime = HostRuntime::new(
            &demand,
            ConvergedState,
            &sink,
            UnusedHostMemory,
            runtime_config,
        );

        runtime
            .run(&stop)
            .expect("observe-only runtime stops cleanly");

        assert!(sink.requests.borrow().is_empty());
        assert!(demand.control.borrow().intent.is_none());
    }

    #[test]
    fn default_configuration_keeps_shrink_advisory() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|state: VirtioMemState| {
            let target = state.current_bytes - 64 * MIB;
            Ok(fresh(target, target))
        })
        .stopping_after(1, &stop);
        let sink = RecordingSink::default();
        let mut runtime_config = config();
        runtime_config.automatic_windows_shrink = crate::config::DEFAULT_AUTOMATIC_WINDOWS_SHRINK;
        let runtime = HostRuntime::new(
            &demand,
            ConvergedState,
            &sink,
            UnusedHostMemory,
            runtime_config,
        );

        runtime.run(&stop).expect("advisory shrink stops cleanly");

        assert!(sink.requests.borrow().is_empty());
    }

    #[test]
    fn ambiguous_automatic_shrink_is_not_replayed_on_the_next_poll() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|state: VirtioMemState| {
            let target = state.current_bytes - 64 * MIB;
            Ok(fresh(target, target))
        })
        .stopping_after(2, &stop);
        let sink = AmbiguousSink(Cell::new(0));
        let runtime = HostRuntime::new(
            &demand,
            ConvergedState,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        );

        runtime.run(&stop).expect("latched ambiguity is non-fatal");

        assert_eq!(sink.0.get(), 1);
        assert!(demand.latched());
    }

    #[test]
    fn failed_growth_latches_and_stops_the_daemon() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|state: VirtioMemState| {
            Ok(fresh(state.current_bytes + GIB, state.current_bytes))
        });
        let sink = AmbiguousSink(Cell::new(0));
        let runtime = HostRuntime::new(&demand, ConvergedState, &sink, AmpleHostMemory, config());

        assert!(matches!(
            runtime.run(&stop),
            Err(HostRuntimeError::Resize(_))
        ));
        assert_eq!(sink.0.get(), 1);
        assert!(demand.latched());
    }

    #[test]
    fn owned_growth_timeout_latches_and_stops_without_another_resize() {
        let stop = AtomicBool::new(false);
        let target = 17 * GIB;
        let demand = FakeDemand::new(|_| panic!("pending growth is not re-evaluated"))
            .with_control(DurableControlState {
                latch_reason: None,
                intent: Some(intent(ReconcileDirection::Grow, 16 * GIB, 16 * GIB, target)),
            });
        let source = ScriptedState::new(
            &stop,
            [
                Ok(state(target, 16 * GIB)),
                Ok(state(target, 16 * GIB)),
                Ok(state(target, 16 * GIB)),
            ],
        );
        let sink = RecordingSink::default();
        let mut runtime_config = config();
        runtime_config.poll_interval = Duration::from_millis(2);
        runtime_config.convergence_timeout = Duration::from_millis(1);
        let runtime = HostRuntime::new(&demand, &source, &sink, UnusedHostMemory, runtime_config);

        assert!(matches!(
            runtime.run(&stop),
            Err(HostRuntimeError::ConvergenceTimeout(_))
        ));
        assert!(demand.latched());
        let control = demand.control.borrow();
        assert_eq!(
            control.intent.as_ref().map(|intent| intent.target_bytes),
            Some(target),
            "the unresolved growth intent is retained"
        );
        assert!(sink.requests.borrow().is_empty());
    }

    #[test]
    fn requested_growth_that_never_converges_latches_before_stopping() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|state: VirtioMemState| {
            Ok(fresh(state.current_bytes + GIB, state.current_bytes))
        });
        let source = ScriptedState::new(
            &stop,
            [
                Ok(state(16 * GIB, 16 * GIB)),
                Ok(state(17 * GIB, 16 * GIB)),
                Ok(state(17 * GIB, 16 * GIB)),
                Ok(state(17 * GIB, 16 * GIB)),
            ],
        );
        let sink = RecordingSink::default();
        let mut runtime_config = config();
        runtime_config.poll_interval = Duration::from_millis(2);
        runtime_config.convergence_timeout = Duration::from_millis(1);
        let runtime = HostRuntime::new(&demand, &source, &sink, AmpleHostMemory, runtime_config);

        assert!(matches!(
            runtime.run(&stop),
            Err(HostRuntimeError::ConvergenceTimeout(_))
        ));
        assert_eq!(
            demand.control.borrow().latch_reason.as_deref(),
            Some("owned growth exceeded the convergence deadline")
        );
        assert_eq!(sink.requests.borrow().as_slice(), [17 * GIB]);
    }

    #[test]
    fn owned_shrink_timeout_latches_and_stops_like_growth() {
        let stop = AtomicBool::new(false);
        let target = 16 * GIB - 64 * MIB;
        let demand =
            FakeDemand::new(|_| Ok(fresh(7 * GIB, 4 * GIB))).with_control(DurableControlState {
                latch_reason: None,
                intent: Some(intent(
                    ReconcileDirection::Shrink,
                    16 * GIB,
                    16 * GIB,
                    target,
                )),
            });
        let source = ScriptedState::new(
            &stop,
            [
                Ok(state(target, 16 * GIB)),
                Ok(state(target, 16 * GIB)),
                Ok(state(target, 16 * GIB)),
            ],
        );
        let sink = RecordingSink::default();
        let mut runtime_config = automatic_shrink_config();
        runtime_config.poll_interval = Duration::from_millis(2);
        runtime_config.convergence_timeout = Duration::from_millis(1);
        let runtime = HostRuntime::new(&demand, &source, &sink, UnusedHostMemory, runtime_config);

        assert!(matches!(
            runtime.run(&stop),
            Err(HostRuntimeError::ConvergenceTimeout(_))
        ));
        assert!(demand.latched());
        assert!(sink.requests.borrow().is_empty());
    }

    #[test]
    fn renewed_pressure_journals_before_upward_pending_shrink_supersession() {
        let stop = AtomicBool::new(false);
        let demand =
            FakeDemand::new(|_| Ok(fresh(7 * GIB, 4 * GIB))).with_control(DurableControlState {
                latch_reason: None,
                intent: Some(intent(
                    ReconcileDirection::Shrink,
                    8 * GIB,
                    8 * GIB,
                    6 * GIB,
                )),
            });
        let events = Rc::clone(&demand.events);
        let source = ScriptedState::new(
            &stop,
            [Ok(state(6 * GIB, 8 * GIB)), Ok(state(7 * GIB, 8 * GIB))],
        );
        let sink = RecordingSink {
            requests: RefCell::default(),
            events: Some(Rc::clone(&events)),
        };
        let runtime = HostRuntime::new(
            &demand,
            &source,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        );

        runtime.run(&stop).expect("supersession is non-fatal");

        let events = events.borrow();
        let intent_position = events
            .iter()
            .position(|event| event == &format!("intent:{}", 7 * GIB))
            .expect("new intent");
        let command_position = events
            .iter()
            .position(|event| event == &format!("command:{}", 7 * GIB))
            .expect("supersession command");
        assert!(intent_position < command_position);
        assert!(!events
            .iter()
            .any(|event| event == &format!("command:{}", 5 * GIB)));
        assert!(sink.requests.borrow().is_empty());
    }

    #[test]
    fn restart_observes_unowned_divergence_without_replaying_a_command() {
        let stop = AtomicBool::new(false);
        let source = ScriptedState::new(&stop, [Ok(state(16 * GIB - 64 * MIB, 16 * GIB))]);
        let target = 16 * GIB - 128 * MIB;
        let demand = FakeDemand::new(move |_| Ok(fresh(target, target)));
        let sink = RecordingSink::default();
        let runtime = HostRuntime::new(
            &demand,
            &source,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        );

        runtime
            .run(&stop)
            .expect("recovery observation is non-fatal");

        assert_eq!(demand.evaluations.get(), 0);
        assert!(sink.requests.borrow().is_empty());
    }

    #[test]
    fn state_interruption_during_owned_shrink_latches_without_replay() {
        let stop = AtomicBool::new(false);
        let target = 16 * GIB - 64 * MIB;
        let source = ScriptedState::new(
            &stop,
            [
                Ok(state(16 * GIB, 16 * GIB)),
                Err("guest is transitioning".to_owned()),
                Ok(state(target, 16 * GIB)),
            ],
        );
        let demand = FakeDemand::new(move |_| Ok(fresh(target, target)));
        let sink = RecordingSink::default();
        let runtime = HostRuntime::new(
            &demand,
            &source,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        );

        runtime
            .run(&stop)
            .expect("owned interruption becomes recovery-required observation");

        assert!(demand.latched());
        assert_eq!(
            demand.evaluations.get(),
            1,
            "a latched shrink is only observed"
        );
        assert_eq!(sink.requests.borrow().as_slice(), &[target]);
    }

    #[test]
    fn external_target_change_latches_after_later_convergence() {
        let stop = AtomicBool::new(false);
        let target = 16 * GIB - 64 * MIB;
        let external_target = target - 2 * MIB;
        let source = ScriptedState::new(
            &stop,
            [
                Ok(state(16 * GIB, 16 * GIB)),
                Ok(state(external_target, 16 * GIB)),
                Ok(state(16 * GIB, 16 * GIB)),
            ],
        );
        let demand = FakeDemand::new(move |_| Ok(fresh(target, target)));
        let sink = RecordingSink::default();
        let runtime = HostRuntime::new(
            &demand,
            &source,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        );

        runtime
            .run(&stop)
            .expect("ownership conflict is a non-fatal latch");

        assert!(demand.latched());
        assert_eq!(demand.evaluations.get(), 2);
        assert_eq!(sink.requests.borrow().as_slice(), &[target]);
    }

    #[test]
    fn cancellation_of_owned_shrink_does_not_replay_after_restart() {
        let target = 16 * GIB - 64 * MIB;
        let demand = FakeDemand::new(move |_| Ok(fresh(target, target)));
        let sink = RecordingSink::default();
        let first_stop = AtomicBool::new(false);
        let first_source = ScriptedState::new(
            &first_stop,
            [Ok(state(16 * GIB, 16 * GIB)), Ok(state(target, 16 * GIB))],
        );
        HostRuntime::new(
            &demand,
            &first_source,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        )
        .run(&first_stop)
        .expect("cancellation is a clean stop");

        let restart_stop = AtomicBool::new(false);
        let restart_source = ScriptedState::new(&restart_stop, [Ok(state(target, 16 * GIB))]);
        HostRuntime::new(
            &demand,
            &restart_source,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        )
        .run(&restart_stop)
        .expect("restart observes but does not replay divergence");

        assert_eq!(sink.requests.borrow().as_slice(), &[target]);
        assert_eq!(
            demand
                .control
                .borrow()
                .intent
                .as_ref()
                .map(|intent| intent.target_bytes),
            Some(target),
            "the restarted daemon still owns the pending shrink"
        );
    }
}
