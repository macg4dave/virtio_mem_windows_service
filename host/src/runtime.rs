use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use virtio_mem_core::{
    plan_resize, DemandCalculator, DemandPolicyConfig, DemandReport, MemoryControllerConfig,
    MemoryStats, RawTelemetryEnvelope, ResizeDecision, VirtioMemState,
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

impl GuestStatsSource for Box<dyn GuestStatsSource> {
    fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        (**self).get_memory_stats()
    }
}

pub trait MemoryStateSource {
    fn memory_state(&self) -> Result<VirtioMemState, String>;
}
pub trait ResizeSink {
    fn request_resize(&self, requested_bytes: u64) -> Result<(), String>;
}

#[derive(Debug, Error)]
pub enum HostRuntimeError {
    #[error("raw Windows telemetry failed validation: {0}")]
    RawTelemetry(String),
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
    raw_telemetry: G,
    state_source: S,
    resize_sink: R,
    host_memory: H,
    config: HostConfig,
}

impl<G, S, R, H> HostRuntime<G, S, R, H>
where
    G: RawTelemetrySource,
    S: MemoryStateSource,
    R: ResizeSink,
    H: HostMemorySource,
{
    pub fn new(
        raw_telemetry: G,
        state_source: S,
        resize_sink: R,
        host_memory: H,
        config: HostConfig,
    ) -> Self {
        Self {
            raw_telemetry,
            state_source,
            resize_sink,
            host_memory,
            config,
        }
    }

    pub fn run(&self, stop: &AtomicBool) -> Result<(), HostRuntimeError> {
        let mut pending_since = None;
        while !stop.load(Ordering::Acquire) {
            let state = self
                .state_source
                .memory_state()
                .map_err(HostRuntimeError::MemoryState)?;
            state
                .validate()
                .map_err(|error| HostRuntimeError::MemoryState(error.to_string()))?;
            if state.requested_bytes != state.current_bytes {
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
            let envelope = self
                .raw_telemetry
                .read()
                .map_err(HostRuntimeError::RawTelemetry)?;
            let report = evaluate_demand_join(envelope, state, &self.config)
                .map_err(HostRuntimeError::Controller)?;
            let decision = if report.demand.desired_target_bytes == state.current_bytes {
                ResizeDecision::NoChange
            } else {
                ResizeDecision::Request {
                    requested_bytes: report.demand.desired_target_bytes,
                }
            };
            if let ResizeDecision::Request { requested_bytes } = decision {
                if requested_bytes < state.current_bytes {
                    eprintln!(
                        "virtio-mem-host: calculated advisory shrink to {requested_bytes} bytes; automatic Windows shrink remains disabled pending M10b"
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
                self.resize_sink
                    .request_resize(requested_bytes)
                    .map_err(HostRuntimeError::Resize)?;
                pending_since = Some(Instant::now());
            }
            wait_interruptibly(stop, self.config.poll_interval);
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
            poll_interval: Duration::from_secs(30),
            command_timeout: Duration::from_secs(10),
            convergence_timeout: Duration::from_secs(300),
            virsh_binary: "virsh".to_owned(),
            stats_source: crate::config::StatsSource::DomMemStat,
            stats_max_age: Duration::from_secs(60),
            stats_future_tolerance: Duration::from_secs(5),
            raw_telemetry_path: "guest.telemetry.jsonl".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 4 * GIB,
            compatibility_attestation_path: "reviewed.json".to_owned(),
        }
    }

    fn critical_envelope() -> RawTelemetryEnvelope {
        RawTelemetryEnvelope::new(
            "guest",
            1_000,
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

        assert_eq!(report.demand.desired_target_bytes, 16 * GIB + 8 * MIB);
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

    #[test]
    fn returns_immediately_for_a_cancelled_runtime() {
        let stop = AtomicBool::new(true);
        wait_interruptibly(&stop, Duration::from_secs(60));
    }
}
