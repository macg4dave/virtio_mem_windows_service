use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use virtio_mem_core::{
    parse_memory_stats, plan_resize, MemoryControllerConfig, ResizeDecision, VirtioMemState,
};

use crate::config::HostConfig;
use crate::virsh::VirshError;

pub trait GuestStatsSource {
    fn get_memory_stats(&self) -> Result<String, VirshError>;
}
pub trait MemoryStateSource {
    fn memory_state(&self) -> Result<VirtioMemState, String>;
}
pub trait ResizeSink {
    fn request_resize(&self, requested_bytes: u64) -> Result<(), String>;
}

#[derive(Debug, Error)]
pub enum HostRuntimeError {
    #[error("guest-agent request failed: {0}")]
    GuestAgent(#[from] VirshError),
    #[error("guest-agent response failed validation: {0}")]
    GuestStats(String),
    #[error("live virtio-mem state failed validation: {0}")]
    MemoryState(String),
    #[error("memory controller configuration is invalid: {0}")]
    Controller(String),
    #[error("resize request failed: {0}")]
    Resize(String),
    #[error("virtio-mem request did not converge within {0:?}")]
    ConvergenceTimeout(Duration),
}

pub struct HostRuntime<G, S, R> {
    guest_agent: G,
    state_source: S,
    resize_sink: R,
    config: HostConfig,
}

impl<G, S, R> HostRuntime<G, S, R>
where
    G: GuestStatsSource,
    S: MemoryStateSource,
    R: ResizeSink,
{
    pub fn new(guest_agent: G, state_source: S, resize_sink: R, config: HostConfig) -> Self {
        Self {
            guest_agent,
            state_source,
            resize_sink,
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
            let response = self.guest_agent.get_memory_stats()?;
            let stats = parse_memory_stats(&response)
                .map_err(|error| HostRuntimeError::GuestStats(error.to_string()))?;
            let controller = MemoryControllerConfig {
                min_memory_bytes: self.config.min_memory_bytes,
                max_memory_bytes: self.config.max_memory_bytes,
                lower_threshold_bytes: self.config.lower_threshold_bytes,
                upper_threshold_bytes: self.config.upper_threshold_bytes,
                block_size_bytes: state.block_size_bytes,
            };
            let decision = plan_resize(
                &stats,
                state.requested_bytes,
                state.current_bytes,
                controller,
            )
            .map_err(|error| HostRuntimeError::Controller(error.to_string()))?;
            if let ResizeDecision::Request { requested_bytes } = decision {
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
    #[test]
    fn returns_immediately_for_a_cancelled_runtime() {
        let stop = AtomicBool::new(true);
        wait_interruptibly(&stop, Duration::from_secs(60));
    }
}
