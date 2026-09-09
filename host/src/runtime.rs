use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use virtio_mem_core::{
    plan_resize, DemandCalculator, DemandPolicyConfig, DemandReport, MemoryControllerConfig,
    MemoryStats, RawTelemetryEnvelope, ResizeDecision, ShrinkAction, ShrinkObservation,
    ShrinkOperation, ShrinkPolicy, VirtioMemState,
};

use crate::config::HostConfig;
use crate::host_memory::{validate_grow_headroom, HostMemorySource};
use crate::raw_telemetry::RawTelemetrySource;

pub fn evaluate_memory_decision(
    stats: &MemoryStats,
    state: VirtioMemState,
    config: &HostConfig,
) -> Result<ResizeDecision, String> {
    let controller = MemoryControllerConfig {
        min_memory_bytes: config.min_memory_bytes,
        max_memory_bytes: config.max_memory_bytes,
        lower_threshold_bytes: config.lower_threshold_bytes,
        upper_threshold_bytes: config.upper_threshold_bytes,
        block_size_bytes: state.block_size_bytes,
        grow_step_bytes: config.grow_step_bytes,
        shrink_step_bytes: config.shrink_step_bytes,
    };
    plan_resize(
        stats,
        state.requested_bytes,
        state.current_bytes,
        controller,
    )
    .map_err(|error| error.to_string())
}

pub fn evaluate_demand_join(
    envelope: RawTelemetryEnvelope,
    state: VirtioMemState,
    config: &HostConfig,
) -> Result<DemandReport, String> {
    let calculator = DemandCalculator::new(DemandPolicyConfig {
        configured_minimum_bytes: config.min_memory_bytes,
        configured_maximum_bytes: config.max_memory_bytes,
        block_size_bytes: state.block_size_bytes,
        grow_step_bytes: config.grow_step_bytes,
        shrink_step_bytes: config.shrink_step_bytes,
    })
    .map_err(|error| error.to_string())?;
    let report = calculator
        .calculate(envelope.memory, state.current_bytes)
        .map_err(|error| error.to_string())?;
    state
        .validate_target(report.demand.desired_target_bytes)
        .map_err(|error| error.to_string())?;
    Ok(report)
}

pub trait GuestStatsSource {
    fn get_memory_stats(&self) -> Result<MemoryStats, String>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct DemandDecision {
    pub decision: ResizeDecision,
    pub safe_floor_bytes: u64,
}

pub trait DemandSource {
    fn evaluate(
        &self,
        state: VirtioMemState,
        config: &HostConfig,
    ) -> Result<DemandDecision, String>;
}

impl<T: RawTelemetrySource> DemandSource for T {
    fn evaluate(
        &self,
        state: VirtioMemState,
        config: &HostConfig,
    ) -> Result<DemandDecision, String> {
        let report = evaluate_demand_join(self.read()?, state, config)?;
        let decision = if report.demand.desired_target_bytes == state.current_bytes {
            ResizeDecision::NoChange
        } else {
            ResizeDecision::Request {
                requested_bytes: report.demand.desired_target_bytes,
            }
        };
        Ok(DemandDecision {
            decision,
            safe_floor_bytes: report.demand.safe_floor_bytes,
        })
    }
}

pub struct GuestStatsDemandSource<T> {
    source: T,
}

impl<T> GuestStatsDemandSource<T> {
    pub fn new(source: T) -> Self {
        Self { source }
    }
}

impl<T: GuestStatsSource> DemandSource for GuestStatsDemandSource<T> {
    fn evaluate(
        &self,
        state: VirtioMemState,
        config: &HostConfig,
    ) -> Result<DemandDecision, String> {
        let decision = evaluate_memory_decision(&self.source.get_memory_stats()?, state, config)?;
        let safe_floor_bytes = match decision {
            ResizeDecision::Request { requested_bytes }
                if requested_bytes < state.current_bytes =>
            {
                requested_bytes
            }
            _ => state.current_bytes,
        };
        Ok(DemandDecision {
            decision,
            safe_floor_bytes,
        })
    }
}

impl GuestStatsSource for Box<dyn GuestStatsSource> {
    fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        (**self).get_memory_stats()
    }
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

    fn renotify_shrink(&self, _target_bytes: u64) -> Result<(), ResizeSinkError> {
        Err(ResizeSinkError::Rejected(
            "shrink re-notification is unsupported by this resize sink".to_owned(),
        ))
    }
}

#[derive(Debug, Error)]
pub enum HostRuntimeError {
    #[error("memory demand input failed validation: {0}")]
    DemandInput(String),
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

pub struct HostRuntime<G, S, R, H> {
    demand_source: G,
    state_source: S,
    resize_sink: R,
    host_memory: H,
    config: HostConfig,
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
        let mut pending_since = None;
        let runtime_started = Instant::now();
        let mut owned_request = false;
        let mut shrink_operation: Option<ShrinkOperation> = None;
        let mut shrink_operation_id: Option<String> = None;
        let mut operation_counter = 0_u64;
        let mut actuation_latched = false;
        let mut unowned_divergence_reported = false;
        while !stop.load(Ordering::Acquire) {
            let state = match self.state_source.memory_state() {
                Ok(state) => state,
                Err(error) => {
                    if let Some(operation) = shrink_operation.as_mut() {
                        let reason = format!("live state unavailable during owned shrink: {error}");
                        if matches!(
                            operation.require_recovery(reason.clone()),
                            ShrinkAction::Latch { .. }
                        ) {
                            actuation_latched = true;
                            eprintln!(
                                "virtio-mem-host: event=shrink_observation_failed operation_id={} vm={} alias={} reason={reason}",
                                shrink_operation_id.as_deref().unwrap_or("unknown"),
                                self.config.vm_name,
                                self.config.alias
                            );
                        }
                        wait_interruptibly(stop, self.config.poll_interval);
                        continue;
                    }
                    return Err(HostRuntimeError::MemoryState(error));
                }
            };
            if let Err(error) = state.validate() {
                if let Some(operation) = shrink_operation.as_mut() {
                    let reason = format!("invalid live state during owned shrink: {error}");
                    if matches!(
                        operation.require_recovery(reason.clone()),
                        ShrinkAction::Latch { .. }
                    ) {
                        actuation_latched = true;
                        eprintln!(
                            "virtio-mem-host: event=shrink_observation_failed operation_id={} vm={} alias={} reason={reason}",
                            shrink_operation_id.as_deref().unwrap_or("unknown"),
                            self.config.vm_name,
                            self.config.alias
                        );
                    }
                    wait_interruptibly(stop, self.config.poll_interval);
                    continue;
                }
                return Err(HostRuntimeError::MemoryState(error.to_string()));
            }
            if state.requested_bytes != state.current_bytes {
                if let Some(operation) = shrink_operation.as_mut() {
                    let now_millis =
                        u64::try_from(runtime_started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    match operation.observe(ShrinkObservation {
                        now_millis,
                        requested_bytes: state.requested_bytes,
                        current_bytes: state.current_bytes,
                        fresh: true,
                        guest_running: true,
                    }) {
                        ShrinkAction::Renotify {
                            target_bytes,
                            retry_index,
                        } if self.config.shrink_renotification => {
                            match self.resize_sink.renotify_shrink(target_bytes) {
                                Ok(()) => eprintln!(
                                    "virtio-mem-host: event=shrink_renotified operation_id={} vm={} alias={} target_bytes={target_bytes} requested_bytes={} current_bytes={} retry_index={retry_index} elapsed_millis={now_millis} deadline_millis=300000",
                                    shrink_operation_id.as_deref().unwrap_or("unknown"),
                                    self.config.vm_name,
                                    self.config.alias,
                                    state.requested_bytes,
                                    state.current_bytes
                                ),
                                Err(error) => {
                                    let event = match &error {
                                        ResizeSinkError::Rejected(_) => "shrink_renotification_rejected",
                                        ResizeSinkError::CommandUnknown(_) => "shrink_command_unknown",
                                    };
                                    let _ = match &error {
                                        ResizeSinkError::Rejected(_) => {
                                            operation.require_recovery(error.to_string())
                                        }
                                        ResizeSinkError::CommandUnknown(_) => {
                                            operation.uncertain_command_result()
                                        }
                                    };
                                    actuation_latched = true;
                                    eprintln!(
                                        "virtio-mem-host: event={event} operation_id={} vm={} alias={} target_bytes={target_bytes} retry_index={retry_index} reason={error}",
                                        shrink_operation_id.as_deref().unwrap_or("unknown"),
                                        self.config.vm_name,
                                        self.config.alias
                                    );
                                }
                            }
                        }
                        ShrinkAction::Progress { blocks_reclaimed } => {
                            eprintln!(
                                "virtio-mem-host: event=shrink_progress operation_id={} vm={} alias={} target_bytes={} requested_bytes={} current_bytes={} blocks_reclaimed={blocks_reclaimed} elapsed_millis={now_millis} deadline_millis=300000",
                                shrink_operation_id.as_deref().unwrap_or("unknown"),
                                self.config.vm_name,
                                self.config.alias,
                                state.requested_bytes,
                                state.requested_bytes,
                                state.current_bytes
                            );
                        }
                        ShrinkAction::Converged => {
                            eprintln!(
                                "virtio-mem-host: event=shrink_converged operation_id={} vm={} alias={} target_bytes={} requested_bytes={} current_bytes={} elapsed_millis={now_millis}",
                                shrink_operation_id.as_deref().unwrap_or("unknown"),
                                self.config.vm_name,
                                self.config.alias,
                                state.requested_bytes,
                                state.requested_bytes,
                                state.current_bytes
                            );
                        }
                        ShrinkAction::Latch { reason } => {
                            actuation_latched = true;
                            eprintln!(
                                "virtio-mem-host: event=shrink_latched operation_id={} vm={} alias={} target_bytes={} requested_bytes={} current_bytes={} elapsed_millis={now_millis} deadline_millis=300000 reason={reason}",
                                shrink_operation_id.as_deref().unwrap_or("unknown"),
                                self.config.vm_name,
                                self.config.alias,
                                state.requested_bytes,
                                state.requested_bytes,
                                state.current_bytes
                            );
                        }
                        _ => {}
                    }
                    wait_interruptibly(stop, self.config.poll_interval);
                    continue;
                }
                if !owned_request {
                    if !unowned_divergence_reported {
                        eprintln!(
                            "virtio-mem-host: event=ownership_conflict vm={} alias={} requested_bytes={} current_bytes={} reason=unowned_divergence_replay_suppressed",
                            self.config.vm_name,
                            self.config.alias,
                            state.requested_bytes,
                            state.current_bytes
                        );
                        unowned_divergence_reported = true;
                    }
                    wait_interruptibly(stop, self.config.poll_interval);
                    continue;
                }
                let started = pending_since.get_or_insert_with(Instant::now);
                if started.elapsed() >= self.config.convergence_timeout {
                    return Err(HostRuntimeError::ConvergenceTimeout(
                        self.config.convergence_timeout,
                    ));
                }
                wait_interruptibly(stop, self.config.poll_interval);
                continue;
            }
            pending_since = None;
            owned_request = false;
            shrink_operation = None;
            shrink_operation_id = None;
            unowned_divergence_reported = false;
            let demand = self
                .demand_source
                .evaluate(state, &self.config)
                .map_err(HostRuntimeError::DemandInput)?;
            let decision = demand.decision;
            if let ResizeDecision::Request { requested_bytes } = decision {
                if actuation_latched {
                    wait_interruptibly(stop, self.config.poll_interval);
                    continue;
                }
                if requested_bytes < state.current_bytes && !self.config.automatic_windows_shrink {
                    eprintln!(
                        "virtio-mem-host: calculated advisory shrink to {requested_bytes} bytes; automatic Windows shrink is disabled"
                    );
                    wait_interruptibly(stop, self.config.poll_interval);
                    continue;
                }
                if requested_bytes > state.current_bytes {
                    let host_available = self
                        .host_memory
                        .available_bytes()
                        .map_err(HostRuntimeError::HostMemory)?;
                    if let Err(error) = validate_grow_headroom(
                        state.current_bytes,
                        requested_bytes,
                        host_available,
                        self.config.host_min_headroom_bytes,
                    ) {
                        eprintln!(
                            "virtio-mem-host: blocking grow to {requested_bytes} bytes; {error}"
                        );
                        wait_interruptibly(stop, self.config.poll_interval);
                        continue;
                    }
                }
                let candidate_operation_id = if requested_bytes < state.current_bytes {
                    operation_counter = operation_counter.saturating_add(1);
                    Some(format!("{}-{operation_counter}", std::process::id()))
                } else {
                    None
                };
                if let Err(error) = self.resize_sink.request_resize(requested_bytes) {
                    if requested_bytes < state.current_bytes {
                        actuation_latched = true;
                        let event = match &error {
                            ResizeSinkError::Rejected(_) => "shrink_request_rejected",
                            ResizeSinkError::CommandUnknown(_) => "shrink_command_unknown",
                        };
                        eprintln!(
                            "virtio-mem-host: event={event} operation_id={} vm={} alias={} target_bytes={requested_bytes} requested_bytes={} current_bytes={} reason={error}",
                            candidate_operation_id.as_deref().unwrap_or("unknown"),
                            self.config.vm_name,
                            self.config.alias,
                            state.requested_bytes,
                            state.current_bytes
                        );
                        wait_interruptibly(stop, self.config.poll_interval);
                        continue;
                    }
                    return Err(HostRuntimeError::Resize(error.to_string()));
                }
                owned_request = true;
                pending_since = Some(Instant::now());
                if requested_bytes < state.current_bytes {
                    let now_millis =
                        u64::try_from(runtime_started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    shrink_operation = Some(
                        ShrinkOperation::start(
                            ShrinkPolicy::qualification(state.block_size_bytes),
                            now_millis,
                            state.current_bytes,
                            requested_bytes,
                            demand.safe_floor_bytes,
                        )
                        .map_err(HostRuntimeError::Controller)?,
                    );
                    shrink_operation_id = candidate_operation_id;
                    eprintln!(
                        "virtio-mem-host: event=shrink_requested operation_id={} vm={} alias={} target_bytes={requested_bytes} requested_bytes={} current_bytes={} retry_index=0 elapsed_millis={now_millis} deadline_millis=300000",
                        shrink_operation_id.as_deref().unwrap_or("unknown"),
                        self.config.vm_name,
                        self.config.alias,
                        state.requested_bytes,
                        state.current_bytes
                    );
                }
            }
            wait_interruptibly(stop, self.config.poll_interval);
        }
        if let Some(operation) = shrink_operation.as_mut() {
            if matches!(operation.state(), virtio_mem_core::ShrinkState::Observing) {
                let _ = operation.cancel();
                eprintln!(
                    "virtio-mem-host: event=shrink_cancelled operation_id={} vm={} alias={} reason=controller_stop_replay_suppressed",
                    shrink_operation_id.as_deref().unwrap_or("unknown"),
                    self.config.vm_name,
                    self.config.alias
                );
            }
        }
        Ok(())
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

    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn config() -> HostConfig {
        HostConfig {
            vm_name: "guest".to_owned(),
            alias: "memory0".to_owned(),
            min_memory_bytes: 4 * GIB,
            max_memory_bytes: 32 * GIB,
            lower_threshold_bytes: GIB,
            upper_threshold_bytes: 3 * GIB,
            grow_step_bytes: GIB,
            shrink_step_bytes: 64 * MIB,
            poll_interval: Duration::from_secs(30),
            command_timeout: Duration::from_secs(10),
            convergence_timeout: Duration::from_secs(300),
            virsh_binary: "virsh".to_owned(),
            stats_source: crate::config::StatsSource::DomMemStat,
            demand_source: crate::config::DemandSourceMode::Raw,
            stats_max_age: Duration::from_secs(60),
            stats_future_tolerance: Duration::from_secs(5),
            raw_telemetry_path: "guest.telemetry.jsonl".to_owned(),
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 4 * GIB,
            compatibility_attestation_path: "reviewed.json".to_owned(),
            automatic_windows_shrink: false,
            shrink_renotification: false,
        }
    }

    fn critical_envelope() -> RawTelemetryEnvelope {
        RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            "session-a",
            1_000_000,
            10,
            0,
            virtio_mem_core::MemoryTelemetrySnapshot {
                physical_total_bytes: 16 * GIB,
                physical_available_bytes: 2 * GIB,
                memory_load_percent: 88,
                commit_total_bytes: 15 * GIB,
                commit_limit_bytes: 16 * GIB,
                commit_peak_bytes: 15 * GIB,
                system_cache_bytes: 0,
                kernel_paged_bytes: 0,
                kernel_nonpaged_bytes: 0,
            },
        )
    }

    #[test]
    fn demand_join_uses_alias_scoped_live_current_for_host_calculation() {
        let report = evaluate_demand_join(
            critical_envelope(),
            VirtioMemState {
                size_bytes: 40 * GIB,
                block_size_bytes: 2 * MIB,
                requested_bytes: 16 * GIB,
                current_bytes: 16 * GIB,
            },
            &config(),
        )
        .expect("join should pass");

        assert_eq!(report.demand.desired_target_bytes, 17 * GIB);
        assert_eq!(report.memory.physical_total_bytes, 16 * GIB);
    }

    #[test]
    fn demand_join_rejects_target_conflicting_with_live_device_size() {
        assert!(evaluate_demand_join(
            critical_envelope(),
            VirtioMemState {
                size_bytes: 16 * GIB,
                block_size_bytes: 2 * MIB,
                requested_bytes: 16 * GIB,
                current_bytes: 16 * GIB,
            },
            &config(),
        )
        .is_err());
    }

    struct FixedGuestStats(MemoryStats);

    impl GuestStatsSource for FixedGuestStats {
        fn get_memory_stats(&self) -> Result<MemoryStats, String> {
            Ok(MemoryStats {
                free_bytes: self.0.free_bytes,
                available_bytes: self.0.available_bytes,
                total_bytes: self.0.total_bytes,
            })
        }
    }

    #[test]
    fn guest_stats_compatibility_mode_uses_directional_quanta() {
        let demand = GuestStatsDemandSource::new(FixedGuestStats(MemoryStats {
            free_bytes: 4 * GIB,
            available_bytes: 8 * GIB,
            total_bytes: 8 * GIB,
        }));
        let evaluation = demand
            .evaluate(
                VirtioMemState {
                    size_bytes: 40 * GIB,
                    block_size_bytes: 2 * MIB,
                    requested_bytes: 16 * GIB,
                    current_bytes: 16 * GIB,
                },
                &config(),
            )
            .expect("guest stats are valid");

        assert_eq!(
            evaluation,
            DemandDecision {
                decision: ResizeDecision::Request {
                    requested_bytes: 16 * GIB - 64 * MIB,
                },
                safe_floor_bytes: 16 * GIB - 64 * MIB,
            }
        );
    }

    #[test]
    fn returns_immediately_for_a_cancelled_runtime() {
        let stop = AtomicBool::new(true);
        wait_interruptibly(&stop, Duration::from_secs(60));
    }

    struct RepeatedShrinkDemand<'a> {
        evaluations: Cell<u32>,
        stop: &'a AtomicBool,
    }

    impl DemandSource for RepeatedShrinkDemand<'_> {
        fn evaluate(
            &self,
            state: VirtioMemState,
            _config: &HostConfig,
        ) -> Result<DemandDecision, String> {
            let evaluations = self.evaluations.get().saturating_add(1);
            self.evaluations.set(evaluations);
            if evaluations >= 2 {
                self.stop.store(true, Ordering::Release);
            }
            Ok(DemandDecision {
                decision: ResizeDecision::Request {
                    requested_bytes: state.current_bytes - 64 * MIB,
                },
                safe_floor_bytes: state.current_bytes - 64 * MIB,
            })
        }
    }

    struct ConvergedState;

    impl MemoryStateSource for ConvergedState {
        fn memory_state(&self) -> Result<VirtioMemState, String> {
            Ok(VirtioMemState {
                size_bytes: 40 * GIB,
                block_size_bytes: 2 * MIB,
                requested_bytes: 16 * GIB,
                current_bytes: 16 * GIB,
            })
        }
    }

    struct AmbiguousShrinkSink(Cell<u32>);

    impl ResizeSink for &AmbiguousShrinkSink {
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

    #[test]
    fn ambiguous_automatic_shrink_is_not_replayed_on_the_next_poll() {
        let stop = AtomicBool::new(false);
        let demand = RepeatedShrinkDemand {
            evaluations: Cell::new(0),
            stop: &stop,
        };
        let sink = AmbiguousShrinkSink(Cell::new(0));
        let mut runtime_config = config();
        runtime_config.automatic_windows_shrink = true;
        runtime_config.poll_interval = Duration::from_millis(1);
        let runtime = HostRuntime::new(
            demand,
            ConvergedState,
            &sink,
            UnusedHostMemory,
            runtime_config,
        );

        runtime.run(&stop).expect("latched ambiguity is non-fatal");

        assert_eq!(sink.0.get(), 1);
    }

    struct FixedShrinkDemand {
        evaluations: Cell<u32>,
        target_bytes: u64,
    }

    impl DemandSource for &FixedShrinkDemand {
        fn evaluate(
            &self,
            _state: VirtioMemState,
            _config: &HostConfig,
        ) -> Result<DemandDecision, String> {
            self.evaluations
                .set(self.evaluations.get().saturating_add(1));
            Ok(DemandDecision {
                decision: ResizeDecision::Request {
                    requested_bytes: self.target_bytes,
                },
                safe_floor_bytes: self.target_bytes,
            })
        }
    }

    struct ScriptedState<'a> {
        observations: RefCell<VecDeque<Result<VirtioMemState, String>>>,
        stop: &'a AtomicBool,
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

    struct RecordingShrinkSink {
        requests: RefCell<Vec<u64>>,
    }

    impl ResizeSink for &RecordingShrinkSink {
        fn request_resize(&self, requested_bytes: u64) -> Result<(), ResizeSinkError> {
            self.requests.borrow_mut().push(requested_bytes);
            Ok(())
        }
    }

    fn state(requested_bytes: u64, current_bytes: u64) -> VirtioMemState {
        VirtioMemState {
            size_bytes: 40 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes,
            current_bytes,
        }
    }

    fn automatic_shrink_config() -> HostConfig {
        let mut runtime_config = config();
        runtime_config.automatic_windows_shrink = true;
        runtime_config.poll_interval = Duration::from_millis(1);
        runtime_config
    }

    #[test]
    fn restart_observes_unowned_divergence_without_replaying_a_command() {
        let stop = AtomicBool::new(false);
        let source = ScriptedState {
            observations: RefCell::new(VecDeque::from([Ok(state(16 * GIB - 64 * MIB, 16 * GIB))])),
            stop: &stop,
        };
        let demand = FixedShrinkDemand {
            evaluations: Cell::new(0),
            target_bytes: 16 * GIB - 128 * MIB,
        };
        let sink = RecordingShrinkSink {
            requests: RefCell::new(Vec::new()),
        };
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
    fn state_interruption_during_owned_shrink_latches_without_restart_or_replay() {
        let stop = AtomicBool::new(false);
        let target = 16 * GIB - 64 * MIB;
        let source = ScriptedState {
            observations: RefCell::new(VecDeque::from([
                Ok(state(16 * GIB, 16 * GIB)),
                Err("guest is transitioning".to_owned()),
                Ok(state(target, 16 * GIB)),
            ])),
            stop: &stop,
        };
        let demand = FixedShrinkDemand {
            evaluations: Cell::new(0),
            target_bytes: target,
        };
        let sink = RecordingShrinkSink {
            requests: RefCell::new(Vec::new()),
        };
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

        assert_eq!(demand.evaluations.get(), 1);
        assert_eq!(sink.requests.borrow().as_slice(), &[target]);
    }

    #[test]
    fn external_target_change_latches_after_later_convergence() {
        let stop = AtomicBool::new(false);
        let target = 16 * GIB - 64 * MIB;
        let external_target = target - 2 * MIB;
        let source = ScriptedState {
            observations: RefCell::new(VecDeque::from([
                Ok(state(16 * GIB, 16 * GIB)),
                Ok(state(external_target, 16 * GIB)),
                Ok(state(16 * GIB, 16 * GIB)),
            ])),
            stop: &stop,
        };
        let demand = FixedShrinkDemand {
            evaluations: Cell::new(0),
            target_bytes: target,
        };
        let sink = RecordingShrinkSink {
            requests: RefCell::new(Vec::new()),
        };
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

        assert_eq!(demand.evaluations.get(), 2);
        assert_eq!(sink.requests.borrow().as_slice(), &[target]);
    }

    #[test]
    fn cancellation_of_owned_shrink_does_not_replay_after_restart() {
        let target = 16 * GIB - 64 * MIB;
        let first_stop = AtomicBool::new(false);
        let first_source = ScriptedState {
            observations: RefCell::new(VecDeque::from([
                Ok(state(16 * GIB, 16 * GIB)),
                Ok(state(target, 16 * GIB)),
            ])),
            stop: &first_stop,
        };
        let demand = FixedShrinkDemand {
            evaluations: Cell::new(0),
            target_bytes: target,
        };
        let sink = RecordingShrinkSink {
            requests: RefCell::new(Vec::new()),
        };
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
        let restart_source = ScriptedState {
            observations: RefCell::new(VecDeque::from([Ok(state(target, 16 * GIB))])),
            stop: &restart_stop,
        };
        HostRuntime::new(
            &demand,
            &restart_source,
            &sink,
            UnusedHostMemory,
            automatic_shrink_config(),
        )
        .run(&restart_stop)
        .expect("restart observes but does not replay divergence");

        assert_eq!(demand.evaluations.get(), 1);
        assert_eq!(sink.requests.borrow().as_slice(), &[target]);
    }
}
