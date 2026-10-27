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
    allocate_pool, reconcile, CommandOwnership, PoolMember, ReconcileAction, ReconcileDirection,
    ReconcileInput, VirtioMemState, MIN_BLOCK_SIZE_BYTES,
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
    /// Original execution start; absent legacy pending commands fail closed.
    #[serde(default)]
    pub started_unix_millis: Option<u64>,
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

    fn request_resize_guarded(
        &self,
        expected: VirtioMemState,
        target: u64,
        stop: &AtomicBool,
        authorize: &dyn Fn() -> Result<(), String>,
    ) -> Result<(), ResizeSinkError> {
        let _ = expected;
        authorize().map_err(ResizeSinkError::Rejected)?;
        if stop.load(Ordering::Acquire) {
            return Err(ResizeSinkError::Rejected("shutdown requested".to_owned()));
        }
        self.request_resize(target)
    }

    fn supersede_shrink_guarded(
        &self,
        expected: VirtioMemState,
        target: u64,
        stop: &AtomicBool,
    ) -> Result<(), ResizeSinkError> {
        if stop.load(Ordering::Acquire) {
            return Err(ResizeSinkError::Rejected("shutdown requested".to_owned()));
        }
        self.supersede_shrink(expected.requested_bytes, expected.current_bytes, target)
    }

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

/// One VM's phase-one result. Only `Ready` may act, and only after the pool
/// has allocated the round.
enum Observed {
    /// Nothing to decide this round. The pool charge and block size, from
    /// live state or, when that is unreadable, the last known charge; `None`
    /// before the VM was ever read.
    Held {
        charge: Option<(u64, u64)>,
    },
    Ready(Box<Ready>),
    Stop(HostRuntimeError),
}

struct Ready {
    control: DurableControlState,
    state: VirtioMemState,
    estimate: DemandEstimate,
}

/// One VM's sources, sink, and settings. The runtime drives it in two phases
/// per round so the pool can see every member's demand before anyone acts.
pub struct VmController<G, S, R> {
    demand_source: G,
    state_source: S,
    resize_sink: R,
    config: HostConfig,
}

impl<G, S, R> VmController<G, S, R> {
    pub fn new(demand_source: G, state_source: S, resize_sink: R, config: HostConfig) -> Self {
        Self {
            demand_source,
            state_source,
            resize_sink,
            config,
        }
    }
}

pub struct HostRuntime<G, S, R, H> {
    vms: Vec<VmController<G, S, R>>,
    host_memory: H,
    /// Shared device-byte budget; `None` leaves each VM bounded only by its
    /// own maximum and the host headroom check.
    pool_bytes: Option<u64>,
    poll_interval: Duration,
}

/// Loop-local bookkeeping. Nothing here grants authority to actuate: that
/// comes only from the durable control state.
#[derive(Default)]
struct LoopState {
    /// Monotonic bound within this process, combined with the durable original
    /// start time so restarts cannot extend an operation.
    pending_since: Option<Instant>,
    operation_counter: u64,
    unowned_divergence_reported: bool,
    state_unavailable_reported: bool,
    /// Last charge read from live state, kept while the VM is unreadable.
    last_charge: Option<(u64, u64)>,
    outstanding_growth: Option<u64>,
    /// This VM failed and is latched; it is only watched for its pool charge
    /// until the daemon restarts.
    stopped: bool,
    /// After a shrink settled short, no new shrink starts before this, so a
    /// guest that keeps some memory is not asked again every timeout.
    shrink_paused_until: Option<Instant>,
}

impl LoopState {
    fn next_operation_id(&mut self) -> String {
        self.operation_counter = self.operation_counter.saturating_add(1);
        format!("{}-{}", std::process::id(), self.operation_counter)
    }

    fn pending_elapsed(&mut self, timeout: Duration, intent: Option<&CommandIntent>) -> bool {
        let now = crate::raw_telemetry::UnixClock::now_unix_millis(
            &crate::raw_telemetry::SystemUnixClock,
        );
        let Some(started) = intent.and_then(|intent| intent.started_unix_millis) else {
            return true;
        };
        if now.map_or(true, |now| {
            now < started || u128::from(now - started) >= timeout.as_millis()
        }) {
            return true;
        }
        self.pending_since
            .get_or_insert_with(Instant::now)
            .elapsed()
            >= timeout
    }
}

/// Host memory granted to growth within one round, so two VMs growing in the
/// same round cannot both count the same available host RAM.
struct HostBudget {
    committed: u64,
}

impl<G, S, R, H> HostRuntime<G, S, R, H>
where
    G: DemandSource,
    S: MemoryStateSource,
    R: ResizeSink,
    H: HostMemorySource,
{
    /// One VM with no shared pool.
    pub fn new(
        demand_source: G,
        state_source: S,
        resize_sink: R,
        host_memory: H,
        config: HostConfig,
    ) -> Self {
        let poll_interval = config.poll_interval;
        Self {
            vms: vec![VmController::new(
                demand_source,
                state_source,
                resize_sink,
                config,
            )],
            host_memory,
            pool_bytes: None,
            poll_interval,
        }
    }

    /// Several VMs sharing `pool_bytes`. The poll interval is a global setting,
    /// so every member carries the same one.
    pub fn pool(
        vms: Vec<VmController<G, S, R>>,
        host_memory: H,
        pool_bytes: Option<u64>,
    ) -> Result<Self, HostRuntimeError> {
        let poll_interval = vms
            .first()
            .map(|vm| vm.config.poll_interval)
            .ok_or_else(|| HostRuntimeError::Controller("no VMs to control".to_owned()))?;
        Ok(Self {
            vms,
            host_memory,
            pool_bytes,
            poll_interval,
        })
    }

    pub fn run(&self, stop: &AtomicBool) -> Result<(), HostRuntimeError> {
        let mut locals = self
            .vms
            .iter()
            .map(|_| LoopState::default())
            .collect::<Vec<_>>();
        while !stop.load(Ordering::Acquire) {
            if let Step::Stop(error) = self.round(&mut locals, stop)? {
                return Err(error);
            }
            wait_interruptibly(stop, self.poll_interval);
        }
        Ok(())
    }

    /// Observe every VM, allocate the pool, then let each ready VM act,
    /// highest priority first.
    /// A VM that fails (a resize that never converges, a failed grow) is
    /// latched and stops; the others keep running. The daemon stops only once
    /// every VM has.
    fn round(&self, locals: &mut [LoopState], stop: &AtomicBool) -> Result<Step, HostRuntimeError> {
        let mut last_stop = None;
        let mut observed = Vec::with_capacity(self.vms.len());
        for (vm, local) in self.vms.iter().zip(locals.iter_mut()) {
            if stop.load(Ordering::Acquire) {
                return Ok(Step::Wait);
            }
            let seen = if local.stopped {
                vm.observe_stopped(local)
            } else {
                vm.observe(local, stop)?
            };
            match seen {
                Observed::Stop(error) => {
                    vm.stop(local, &error);
                    last_stop = Some(error);
                    observed.push(vm.observe_stopped(local));
                }
                other => observed.push(other),
            }
        }
        if stop.load(Ordering::Acquire) {
            return Ok(Step::Wait);
        }
        let ceilings = self.allocate(&observed)?;
        let mut order = (0..self.vms.len()).collect::<Vec<_>>();
        order.sort_by(|left, right| {
            self.vms[*right]
                .config
                .priority
                .cmp(&self.vms[*left].config.priority)
        });
        let mut host = HostBudget {
            committed: locals.iter().fold(0_u64, |total, local| {
                total.saturating_add(local.outstanding_growth.unwrap_or(u64::MAX))
            }),
        };
        let mut observed = observed.into_iter().map(Some).collect::<Vec<_>>();
        for index in order {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let Some(Observed::Ready(ready)) = observed[index].take() else {
                continue;
            };
            let ceiling = ceilings.as_ref().map(|ceilings| ceilings[index]);
            if let Step::Stop(error) = self.vms[index].act(
                *ready,
                ceiling,
                &self.host_memory,
                &mut host,
                &mut locals[index],
                stop,
            )? {
                self.vms[index].stop(&mut locals[index], &error);
                last_stop = Some(error);
            }
        }
        match last_stop {
            Some(error) if locals.iter().all(|local| local.stopped) => Ok(Step::Stop(error)),
            _ => Ok(Step::Wait),
        }
    }

    fn allocate(&self, observed: &[Observed]) -> Result<Option<Vec<u64>>, HostRuntimeError> {
        let Some(pool_bytes) = self.pool_bytes else {
            return Ok(None);
        };
        let members = self
            .vms
            .iter()
            .zip(observed)
            .map(|(vm, observed)| vm.pool_member(observed, pool_bytes))
            .collect::<Vec<_>>();
        let allocations = allocate_pool(pool_bytes, &members)
            .map_err(|error| HostRuntimeError::Controller(error.to_string()))?;
        for (vm, allocation) in self.vms.iter().zip(&allocations) {
            if allocation.unmet_bytes > 0 {
                eprintln!(
                    "virtio-mem-host: event=pool_exhausted vm={} alias={} priority={} ceiling_bytes={} unmet_bytes={}",
                    vm.config.vm_name,
                    vm.config.alias,
                    vm.config.priority,
                    allocation.ceiling_bytes,
                    allocation.unmet_bytes
                );
            }
        }
        Ok(Some(
            allocations
                .into_iter()
                .map(|allocation| allocation.ceiling_bytes)
                .collect(),
        ))
    }
}

impl<G, S, R> VmController<G, S, R>
where
    G: DemandSource,
    S: MemoryStateSource,
    R: ResizeSink,
{
    /// Phase one: resolve recorded state and, on a converged device, evaluate
    /// demand. Pending operations are handled here in full; they never need
    /// pool bytes because their charge already includes their target.
    fn observe(
        &self,
        local: &mut LoopState,
        stop: &AtomicBool,
    ) -> Result<Observed, HostRuntimeError> {
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
                return Ok(Observed::Held {
                    charge: local.last_charge,
                });
            }
            Err(error) => {
                // Without an owned operation there is nothing to protect but
                // the pool: hold at the last known charge.
                if !local.state_unavailable_reported {
                    eprintln!(
                        "virtio-mem-host: event=live_state_unavailable vm={} alias={} reason={error}",
                        self.config.vm_name, self.config.alias
                    );
                    local.state_unavailable_reported = true;
                }
                return Ok(Observed::Held {
                    charge: local.last_charge,
                });
            }
        };
        local.outstanding_growth = Some(
            state
                .requested_bytes
                .max(
                    control
                        .intent
                        .as_ref()
                        .map_or(0, |intent| intent.target_bytes),
                )
                .saturating_sub(state.current_bytes),
        );
        local.state_unavailable_reported = false;
        let charge = (
            state.requested_bytes.max(state.current_bytes).max(
                control
                    .intent
                    .as_ref()
                    .map_or(0, |intent| intent.target_bytes),
            ),
            state.block_size_bytes,
        );
        local.last_charge = Some(charge);
        let held = Observed::Held {
            charge: Some(charge),
        };
        self.resolve_intent(&mut control, state)?;
        if self.config.observe_only
            && (control.intent.is_some() || state.requested_bytes != state.current_bytes)
        {
            self.log_observe_only(state, "existing_or_unowned_live_intent");
            return Ok(held);
        }
        if state.requested_bytes != state.current_bytes {
            return Ok(
                match self.observe_pending(&mut control, state, local, stop)? {
                    Step::Wait => held,
                    Step::Stop(error) => Observed::Stop(error),
                },
            );
        }
        local.pending_since = None;
        local.unowned_divergence_reported = false;
        match self.demand_source.evaluate(state) {
            Ok(estimate) => Ok(Observed::Ready(Box::new(Ready {
                control,
                state,
                estimate,
            }))),
            Err(error) => {
                eprintln!(
                    "virtio-mem-host: event=demand_input_invalid vm={} alias={} reason={error}",
                    self.config.vm_name, self.config.alias
                );
                Ok(held)
            }
        }
    }

    fn stop(&self, local: &mut LoopState, error: &HostRuntimeError) {
        local.stopped = true;
        eprintln!(
            "virtio-mem-host: event=vm_stopped vm={} alias={} reason={error}; other VMs continue, restart the daemon after recovery",
            self.config.vm_name, self.config.alias
        );
    }

    /// A stopped VM still holds memory, so the pool keeps charging it.
    fn observe_stopped(&self, local: &mut LoopState) -> Observed {
        if let Ok(state) = self.live_state() {
            match self.demand_source.control_state() {
                Ok(control) => {
                    let requested = state.requested_bytes.max(
                        control
                            .intent
                            .as_ref()
                            .map_or(0, |intent| intent.target_bytes),
                    );
                    local.outstanding_growth = Some(requested.saturating_sub(state.current_bytes));
                    local.last_charge =
                        Some((requested.max(state.current_bytes), state.block_size_bytes));
                }
                Err(_) => {
                    local.outstanding_growth = None;
                    local.last_charge = None;
                }
            }
        }
        Observed::Held {
            charge: local.last_charge,
        }
    }

    /// This VM's input to the pool. Anything that is not ready to act asks
    /// for nothing beyond what it holds. An unreadable VM (crashed, off, or a
    /// failing virsh) keeps the memory it last held: the pool never reclaims
    /// it. A VM never read since startup could hold anything, so it is charged
    /// the whole pool until its first read.
    fn pool_member(&self, observed: &Observed, pool_bytes: u64) -> PoolMember {
        let hold = |charge: Option<(u64, u64)>| {
            let (charged_bytes, block_bytes) = charge.unwrap_or((pool_bytes, MIN_BLOCK_SIZE_BYTES));
            PoolMember {
                priority: self.config.priority,
                charged_bytes,
                desired_bytes: charged_bytes,
                block_bytes,
            }
        };
        match observed {
            Observed::Held { charge } => hold(*charge),
            // A stopping round never reaches allocation.
            Observed::Stop(_) => hold(None),
            Observed::Ready(ready) if ready.control.latched() || self.config.observe_only => hold(
                Some((ready.state.current_bytes, ready.state.block_size_bytes)),
            ),
            Observed::Ready(ready) => PoolMember {
                priority: self.config.priority,
                charged_bytes: ready.state.current_bytes,
                desired_bytes: ready.estimate.desired_bytes,
                block_bytes: ready.state.block_size_bytes,
            },
        }
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
        stop: &AtomicBool,
    ) -> Result<Step, HostRuntimeError> {
        let owned_direction = control
            .intent
            .as_ref()
            .filter(|intent| intent.target_bytes == state.requested_bytes)
            .map(|intent| intent.direction);
        match owned_direction {
            Some(ReconcileDirection::Grow) => {
                if local.pending_elapsed(self.config.convergence_timeout, control.intent.as_ref()) {
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
            ) => self.observe_owned_shrink(control, state, local, stop),
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
        stop: &AtomicBool,
    ) -> Result<Step, HostRuntimeError> {
        let elapsed =
            local.pending_elapsed(self.config.convergence_timeout, control.intent.as_ref());
        if control.latched() {
            return self.shrink_deadline(control, state, elapsed);
        }
        let estimate = match self.demand_source.evaluate(state) {
            Ok(estimate) => estimate,
            Err(error) => {
                let freeze = CommandIntent {
                    operation_id: local.next_operation_id(),
                    started_unix_millis: None,
                    direction: ReconcileDirection::FreezeShrink,
                    prior_requested_bytes: state.requested_bytes,
                    prior_current_bytes: state.current_bytes,
                    target_bytes: state.current_bytes,
                    telemetry_identity: "stale-or-invalid".to_owned(),
                    policy_fingerprint_sha256: String::new(),
                    compatibility_fingerprint_sha256: String::new(),
                };
                let operation_id = freeze.operation_id.clone();
                if stop.load(Ordering::Acquire) {
                    return Ok(Step::Wait);
                }
                self.record_intent(control, freeze)?;
                let command =
                    self.resize_sink
                        .supersede_shrink_guarded(state, state.current_bytes, stop);
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
            if !elapsed {
                return Ok(Step::Wait);
            }
            return self.settle_short(control, state, local, stop);
        };
        let supersede = CommandIntent {
            operation_id: local.next_operation_id(),
            started_unix_millis: None,
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
        if stop.load(Ordering::Acquire) {
            return Ok(Step::Wait);
        }
        self.record_intent(control, supersede)?;
        let command = self
            .resize_sink
            .supersede_shrink_guarded(state, target_bytes, stop);
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

    /// The guest stopped giving memory back before reaching the target. Accept
    /// what it kept: request its current size, which completes the shrink,
    /// and carry on. Only an unknown command outcome latches.
    fn settle_short(
        &self,
        control: &mut DurableControlState,
        state: VirtioMemState,
        local: &mut LoopState,
        stop: &AtomicBool,
    ) -> Result<Step, HostRuntimeError> {
        let settle = CommandIntent {
            operation_id: local.next_operation_id(),
            started_unix_millis: None,
            direction: ReconcileDirection::FreezeShrink,
            prior_requested_bytes: state.requested_bytes,
            prior_current_bytes: state.current_bytes,
            target_bytes: state.current_bytes,
            telemetry_identity: "settled-short".to_owned(),
            policy_fingerprint_sha256: String::new(),
            compatibility_fingerprint_sha256: String::new(),
        };
        let operation_id = settle.operation_id.clone();
        if stop.load(Ordering::Acquire) {
            return Ok(Step::Wait);
        }
        self.record_intent(control, settle)?;
        let command = self
            .resize_sink
            .supersede_shrink_guarded(state, state.current_bytes, stop);
        eprintln!(
            "virtio-mem-host: event=shrink_settled_short operation_id={operation_id} vm={} alias={} target_bytes={} kept_bytes={} command_result={command:?}",
            self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
        );
        if let Err(error) = command {
            self.latch(control, &format!("settling a short shrink failed: {error}"))?;
        }
        local.pending_since = None;
        local.shrink_paused_until = Some(
            Instant::now()
                .checked_add(self.config.reclaim_history)
                .ok_or_else(|| {
                    HostRuntimeError::Controller("reclaim history duration is too large".to_owned())
                })?,
        );
        Ok(Step::Wait)
    }

    fn shrink_deadline(
        &self,
        control: &mut DurableControlState,
        state: VirtioMemState,
        elapsed: bool,
    ) -> Result<Step, HostRuntimeError> {
        if !elapsed {
            return Ok(Step::Wait);
        }
        eprintln!(
            "virtio-mem-host: event=allocation_constrained health=latched vm={} alias={} requested_bytes={} current_bytes={}",
            self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
        );
        Ok(Step::Stop(
            self.stop_on_convergence_timeout(control, "shrink")?,
        ))
    }

    /// Phase two on a converged device: apply the pool ceiling to the estimate
    /// and let the reconciler choose.
    fn act<H: HostMemorySource>(
        &self,
        ready: Ready,
        ceiling: Option<u64>,
        host_memory: &H,
        host: &mut HostBudget,
        local: &mut LoopState,
        stop: &AtomicBool,
    ) -> Result<Step, HostRuntimeError> {
        let Ready {
            mut control,
            state,
            estimate,
        } = ready;
        let control = &mut control;
        let estimate = self.limit_to_pool(estimate, ceiling);
        let fresh = estimate.telemetry_identity.is_some();
        let reconciled = reconcile(ReconcileInput {
            desired_bytes: estimate.desired_bytes,
            safe_floor_bytes: estimate.safe_floor_bytes,
            effective_maximum_bytes: estimate.effective_maximum_bytes,
            history_ready: estimate.history_ready,
            telemetry_fresh: fresh,
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
            return Ok(Step::Wait);
        };
        if self.config.observe_only {
            self.log_observe_only(state, "resize_decision_suppressed");
            return Ok(Step::Wait);
        }
        if direction == ReconcileDirection::Shrink
            && local
                .shrink_paused_until
                .is_some_and(|until| Instant::now() < until)
        {
            return Ok(Step::Wait);
        }
        if direction == ReconcileDirection::Grow {
            let host_available = host_memory
                .available_bytes()
                .map_err(HostRuntimeError::HostMemory)?;
            if let Err(error) = validate_grow_headroom(
                state.current_bytes,
                target_bytes,
                host_available.saturating_sub(host.committed),
                self.config.host_min_headroom_bytes,
            ) {
                eprintln!(
                    "virtio-mem-host: event=grow_blocked vm={} alias={} target_bytes={target_bytes} reason={error}",
                    self.config.vm_name, self.config.alias
                );
                return Ok(Step::Wait);
            }
        }
        let intent = CommandIntent {
            operation_id: local.next_operation_id(),
            started_unix_millis: None,
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
        if stop.load(Ordering::Acquire) {
            return Ok(Step::Wait);
        }
        self.record_intent(control, intent)?;
        let previously_committed = host.committed;
        let growth = target_bytes.saturating_sub(state.current_bytes);
        // Reserve before invoking a command: even a lost/error response may
        // have started growth and must constrain later members in this round.
        host.committed = host.committed.saturating_add(growth);
        local.outstanding_growth = Some(growth);
        let authorize = || {
            if direction == ReconcileDirection::Grow {
                validate_grow_headroom(
                    state.current_bytes,
                    target_bytes,
                    host_memory
                        .available_bytes()?
                        .saturating_sub(previously_committed),
                    self.config.host_min_headroom_bytes,
                )?;
            }
            Ok(())
        };
        if let Err(error) =
            self.resize_sink
                .request_resize_guarded(state, target_bytes, stop, &authorize)
        {
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
        local.outstanding_growth = Some(target_bytes.saturating_sub(state.current_bytes));
        local.pending_since = Some(Instant::now());
        eprintln!(
            "virtio-mem-host: event=resize_requested operation_id={operation_id} vm={} alias={} direction={direction:?} target_bytes={target_bytes} requested_bytes={} current_bytes={}",
            self.config.vm_name, self.config.alias, state.requested_bytes, state.current_bytes
        );
        Ok(Step::Wait)
    }

    /// The pool only caps growth: its ceiling is never below what the VM
    /// holds, so it cannot cause a shrink. The safe floor follows the cap so
    /// the reconciler's ordering holds.
    fn limit_to_pool(&self, estimate: DemandEstimate, ceiling: Option<u64>) -> DemandEstimate {
        let Some(ceiling) = ceiling.filter(|ceiling| *ceiling < estimate.desired_bytes) else {
            return estimate;
        };
        eprintln!(
            "virtio-mem-host: event=pool_limited vm={} alias={} desired_bytes={} ceiling_bytes={ceiling}",
            self.config.vm_name, self.config.alias, estimate.desired_bytes
        );
        DemandEstimate {
            desired_bytes: ceiling,
            safe_floor_bytes: estimate.safe_floor_bytes.min(ceiling),
            ..estimate
        }
    }

    fn record_intent(
        &self,
        control: &mut DurableControlState,
        mut intent: CommandIntent,
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
        intent.started_unix_millis = Some(match control.intent.as_ref() {
            Some(previous) => previous.started_unix_millis.ok_or_else(|| {
                HostRuntimeError::Controller("pending command has no original deadline".to_owned())
            })?,
            None => crate::raw_telemetry::UnixClock::now_unix_millis(
                &crate::raw_telemetry::SystemUnixClock,
            )
            .map_err(HostRuntimeError::Controller)?,
        });
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
    let Some(deadline) = Instant::now().checked_add(duration) else {
        return;
    };
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
            priority: 100,
            min_memory_bytes: 4 * GIB,
            grow_step_bytes: GIB,
            shrink_step_bytes: 64 * MIB,
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
            observe_only: false,
        }
    }

    fn fast_config() -> HostConfig {
        let mut runtime_config = config();
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
            started_unix_millis: Some(
                crate::raw_telemetry::UnixClock::now_unix_millis(
                    &crate::raw_telemetry::SystemUnixClock,
                )
                .unwrap(),
            ),
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
            match self.observations.borrow_mut().pop_front() {
                Some(observation) => observation,
                None => {
                    self.stop.store(true, Ordering::Release);
                    Err("state script exhausted".to_owned())
                }
            }
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
        let demand = FakeDemand::new(|_| Ok(fresh(4 * GIB, 0)));
        let source = ScriptedState::new(&stop, [Ok(state(0, 0)), Ok(state(GIB, 0))]);
        let sink = RecordingSink::default();
        let runtime = HostRuntime::new(&demand, &source, &sink, AmpleHostMemory, fast_config());

        runtime.run(&stop).expect("an empty device can grow");
        assert_eq!(sink.requests.borrow().as_slice(), [GIB]);
    }

    #[test]
    fn stop_during_telemetry_prevents_journalling_and_actuation() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|state: VirtioMemState| {
            Ok(fresh(state.current_bytes + GIB, state.current_bytes))
        })
        .stopping_after(1, &stop);
        let sink = RecordingSink::default();
        HostRuntime::new(
            &demand,
            ConvergedState,
            &sink,
            UnusedHostMemory,
            fast_config(),
        )
        .run(&stop)
        .unwrap();
        assert!(sink.requests.borrow().is_empty());
        assert!(demand.control.borrow().intent.is_none());
    }

    #[test]
    fn pending_growth_reserves_headroom_across_rounds_and_restart() {
        let stop = AtomicBool::new(false);
        let first = FakeDemand::new(|_| Ok(fresh(17 * GIB, 8 * GIB)));
        let second = FakeDemand::new(|_| Ok(fresh(17 * GIB, 8 * GIB)));
        let first_state = ScriptedState::new(
            &stop,
            [Ok(state(16 * GIB, 16 * GIB)), Ok(state(17 * GIB, 16 * GIB))],
        );
        let second_state = ScriptedState::new(
            &stop,
            [Ok(state(16 * GIB, 16 * GIB)), Ok(state(16 * GIB, 16 * GIB))],
        );
        let (first_sink, second_sink) = (RecordingSink::default(), RecordingSink::default());
        let runtime = HostRuntime::pool(
            vec![
                member(
                    &first,
                    &first_state,
                    &first_sink,
                    member_config("first", 100),
                ),
                member(
                    &second,
                    &second_state,
                    &second_sink,
                    member_config("second", 80),
                ),
            ],
            FixedHostMemory(5 * GIB + 512 * MIB),
            Some(64 * GIB),
        )
        .unwrap();
        runtime.run(&stop).unwrap();
        assert_eq!(*first_sink.requests.borrow(), [17 * GIB]);
        assert!(second_sink.requests.borrow().is_empty());
        // Reconstruct the runtime while retaining the durable first intent.
        stop.store(false, Ordering::Release);
        let first_state = ScriptedState::new(&stop, [Ok(state(17 * GIB, 16 * GIB))]);
        let second_state = ScriptedState::new(&stop, [Ok(state(16 * GIB, 16 * GIB))]);
        HostRuntime::pool(
            vec![
                member(
                    &first,
                    &first_state,
                    &first_sink,
                    member_config("first", 100),
                ),
                member(
                    &second,
                    &second_state,
                    &second_sink,
                    member_config("second", 80),
                ),
            ],
            FixedHostMemory(5 * GIB + 512 * MIB),
            Some(64 * GIB),
        )
        .unwrap()
        .run(&stop)
        .unwrap();
        assert!(second_sink.requests.borrow().is_empty());
    }

    #[test]
    fn pending_deadlines_fail_closed_on_legacy_state_and_clock_rollback() {
        let mut pending = intent(ReconcileDirection::Grow, GIB, GIB, 2 * GIB);
        pending.started_unix_millis = None;
        assert!(LoopState::default().pending_elapsed(Duration::from_secs(30), Some(&pending)));
        pending.started_unix_millis = Some(u64::MAX);
        assert!(LoopState::default().pending_elapsed(Duration::from_secs(30), Some(&pending)));
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
            fast_config(),
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
    fn a_shrink_that_stops_short_is_settled_where_the_guest_left_it() {
        let stop = AtomicBool::new(false);
        // Asked to go from 8 GiB to 6 GiB, the guest stays at 8 GiB. Demand
        // agrees with 6 GiB, so nothing raises the target before the deadline.
        let demand =
            FakeDemand::new(|_| Ok(fresh(6 * GIB, 4 * GIB))).with_control(DurableControlState {
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
            [
                Ok(state(6 * GIB, 8 * GIB)),
                Ok(state(6 * GIB, 8 * GIB)),
                Ok(state(8 * GIB, 8 * GIB)),
            ],
        );
        let sink = RecordingSink {
            requests: RefCell::default(),
            events: Some(Rc::clone(&events)),
        };
        let mut runtime_config = fast_config();
        runtime_config.poll_interval = Duration::from_millis(2);
        runtime_config.convergence_timeout = Duration::from_millis(1);
        let runtime = HostRuntime::new(&demand, &source, &sink, UnusedHostMemory, runtime_config);

        runtime.run(&stop).expect("a short shrink is not a failure");
        assert!(!demand.latched());
        assert_eq!(
            *events.borrow(),
            [
                format!("intent:{}", 8 * GIB),
                format!("command:{}", 8 * GIB)
            ],
            "the settle is recorded before it is sent"
        );
        assert_eq!(
            demand.control.borrow().intent,
            None,
            "the settled request converged and was cleared"
        );
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
        let runtime = HostRuntime::new(&demand, &source, &sink, UnusedHostMemory, fast_config());

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
        let runtime = HostRuntime::new(&demand, &source, &sink, UnusedHostMemory, fast_config());

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
        let runtime = HostRuntime::new(&demand, &source, &sink, UnusedHostMemory, fast_config());

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
        let runtime = HostRuntime::new(&demand, &source, &sink, UnusedHostMemory, fast_config());

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
            fast_config(),
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
            fast_config(),
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

    struct FixedHostMemory(u64);

    impl HostMemorySource for FixedHostMemory {
        fn available_bytes(&self) -> Result<u64, String> {
            Ok(self.0)
        }
    }

    fn member_config(name: &str, priority: u32) -> HostConfig {
        HostConfig {
            vm_name: name.to_owned(),
            priority,
            poll_interval: Duration::from_millis(1),
            ..config()
        }
    }

    type Member<'a> = VmController<&'a FakeDemand<'a>, &'a ScriptedState<'a>, &'a RecordingSink>;

    fn member<'a>(
        demand: &'a FakeDemand<'a>,
        state: &'a ScriptedState<'a>,
        sink: &'a RecordingSink,
        config: HostConfig,
    ) -> Member<'a> {
        VmController::new(demand, state, sink, config)
    }

    #[test]
    fn contended_pool_growth_serves_the_higher_priority_vm_first() {
        let stop = AtomicBool::new(false);
        let wants_four_more = |_| Ok(fresh(20 * GIB, 8 * GIB));
        let (low_demand, high_demand) = (
            FakeDemand::new(wants_four_more),
            FakeDemand::new(wants_four_more),
        );
        let low_state = ScriptedState::new(&stop, [Ok(state(16 * GIB, 16 * GIB))]);
        let high_state = ScriptedState::new(&stop, [Ok(state(16 * GIB, 16 * GIB))]);
        let (low_sink, high_sink) = (RecordingSink::default(), RecordingSink::default());
        // Both hold 16 GiB of a 33 GiB pool, so only 1 GiB is free. The
        // low-priority VM is listed first to show order does not matter.
        let runtime = HostRuntime::pool(
            vec![
                member(&low_demand, &low_state, &low_sink, member_config("low", 80)),
                member(
                    &high_demand,
                    &high_state,
                    &high_sink,
                    member_config("high", 100),
                ),
            ],
            AmpleHostMemory,
            Some(33 * GIB),
        )
        .expect("pool runtime");

        runtime.run(&stop).expect("one round");
        assert_eq!(*high_sink.requests.borrow(), [17 * GIB]);
        assert!(low_sink.requests.borrow().is_empty());
    }

    #[test]
    fn a_full_pool_never_takes_memory_from_a_lower_priority_vm() {
        let stop = AtomicBool::new(false);
        let high_demand = FakeDemand::new(|_| Ok(fresh(20 * GIB, 8 * GIB)));
        let low_demand = FakeDemand::new(|_| Ok(fresh(16 * GIB, 8 * GIB)));
        let high_state = ScriptedState::new(&stop, [Ok(state(16 * GIB, 16 * GIB))]);
        let low_state = ScriptedState::new(&stop, [Ok(state(16 * GIB, 16 * GIB))]);
        let (high_sink, low_sink) = (RecordingSink::default(), RecordingSink::default());
        let runtime = HostRuntime::pool(
            vec![
                member(
                    &high_demand,
                    &high_state,
                    &high_sink,
                    member_config("high", 100),
                ),
                member(&low_demand, &low_state, &low_sink, member_config("low", 80)),
            ],
            UnusedHostMemory,
            Some(32 * GIB),
        )
        .expect("pool runtime");

        runtime.run(&stop).expect("one round");
        // Releasing memory is the low VM's own decision; the pool only waits.
        assert!(high_sink.requests.borrow().is_empty());
        assert!(low_sink.requests.borrow().is_empty());
    }

    #[test]
    fn an_unreadable_vm_keeps_its_last_charge_while_others_grow() {
        let stop = AtomicBool::new(false);
        let demand = FakeDemand::new(|_| Ok(fresh(20 * GIB, 8 * GIB)));
        let idle = FakeDemand::new(|_| Ok(fresh(8 * GIB, 4 * GIB)));
        // Round one: 16 + 8 GiB of a 26 GiB pool, so the first VM grows into
        // the free 2 GiB. Round two: the second VM crashes while holding 8 GiB.
        // Its 8 GiB stays charged, so the pool is full and nothing more grows.
        let grows = ScriptedState::new(
            &stop,
            [Ok(state(16 * GIB, 16 * GIB)), Ok(state(18 * GIB, 18 * GIB))],
        );
        let crashes = ScriptedState::new(
            &stop,
            [
                Ok(state(8 * GIB, 8 * GIB)),
                Err("domain is not running".to_owned()),
            ],
        );
        let (sink, other_sink) = (RecordingSink::default(), RecordingSink::default());
        let mut grow_config = member_config("grows", 100);
        grow_config.grow_step_bytes = 4 * GIB;
        let runtime = HostRuntime::pool(
            vec![
                member(&demand, &grows, &sink, grow_config),
                member(&idle, &crashes, &other_sink, member_config("crashed", 80)),
            ],
            AmpleHostMemory,
            Some(26 * GIB),
        )
        .expect("pool runtime");

        runtime
            .run(&stop)
            .expect("an unreadable member is not fatal");
        assert_eq!(*sink.requests.borrow(), [18 * GIB]);
        assert!(other_sink.requests.borrow().is_empty());
    }

    #[test]
    fn vms_growing_in_the_same_round_share_the_host_headroom() {
        let stop = AtomicBool::new(false);
        let wants_more = |_| Ok(fresh(20 * GIB, 8 * GIB));
        let (first_demand, second_demand) =
            (FakeDemand::new(wants_more), FakeDemand::new(wants_more));
        let first_state = ScriptedState::new(&stop, [Ok(state(16 * GIB, 16 * GIB))]);
        let second_state = ScriptedState::new(&stop, [Ok(state(16 * GIB, 16 * GIB))]);
        let (first_sink, second_sink) = (RecordingSink::default(), RecordingSink::default());
        // 5.5 GiB available with a 4 GiB minimum leaves room for one 1 GiB step.
        let runtime = HostRuntime::pool(
            vec![
                member(
                    &first_demand,
                    &first_state,
                    &first_sink,
                    member_config("first", 100),
                ),
                member(
                    &second_demand,
                    &second_state,
                    &second_sink,
                    member_config("second", 80),
                ),
            ],
            FixedHostMemory(5 * GIB + 512 * MIB),
            Some(64 * GIB),
        )
        .expect("pool runtime");

        runtime.run(&stop).expect("one round");
        assert_eq!(*first_sink.requests.borrow(), [17 * GIB]);
        assert!(second_sink.requests.borrow().is_empty());
    }

    #[test]
    fn a_stuck_vm_is_latched_and_held_while_the_others_keep_running() {
        let stop = AtomicBool::new(false);
        let target = 17 * GIB;
        let stuck_demand = FakeDemand::new(|_| panic!("pending growth is not re-evaluated"))
            .with_control(DurableControlState {
                latch_reason: None,
                intent: Some(intent(ReconcileDirection::Grow, 16 * GIB, 16 * GIB, target)),
            });
        // Never converges: one read per round, plus the charge read when it
        // stops in round two.
        let stuck_state = ScriptedState::new(&stop, (0..4).map(|_| Ok(state(target, 16 * GIB))));
        let healthy_demand = FakeDemand::new(|_| Ok(fresh(12 * GIB, 4 * GIB)));
        let healthy_state = ScriptedState::new(
            &stop,
            [
                Ok(state(8 * GIB, 8 * GIB)),
                Ok(state(9 * GIB, 9 * GIB)),
                Ok(state(10 * GIB, 10 * GIB)),
            ],
        );
        let (stuck_sink, healthy_sink) = (RecordingSink::default(), RecordingSink::default());
        let mut stuck_config = member_config("stuck", 100);
        stuck_config.poll_interval = Duration::from_millis(2);
        stuck_config.convergence_timeout = Duration::from_millis(1);
        let runtime = HostRuntime::pool(
            vec![
                member(&stuck_demand, &stuck_state, &stuck_sink, stuck_config),
                member(
                    &healthy_demand,
                    &healthy_state,
                    &healthy_sink,
                    member_config("healthy", 80),
                ),
            ],
            AmpleHostMemory,
            Some(64 * GIB),
        )
        .expect("pool runtime");

        runtime
            .run(&stop)
            .expect("one stuck VM does not stop the daemon");
        assert!(stuck_demand.latched());
        assert!(stuck_sink.requests.borrow().is_empty());
        assert_eq!(
            *healthy_sink.requests.borrow(),
            [9 * GIB, 10 * GIB, 11 * GIB]
        );
    }
}
