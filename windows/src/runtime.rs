use std::time::Duration;

use crate::controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
use crate::error::{PollError, ServiceLoopError};
use crate::service_host::StopSignal;
use crate::service_loop::{run_polling_loop, MemoryStateProvider, ResizeRequestSink};
use crate::stats::parse_memory_stats;

pub trait GuestAgent {
    fn get_memory_stats(&mut self) -> Result<String, String>;
}

#[derive(Debug)]
pub struct MemoryPoller<A> {
    guest_agent: A,
    config: MemoryControllerConfig,
}

impl<A> MemoryPoller<A>
where
    A: GuestAgent,
{
    pub fn new(guest_agent: A, config: MemoryControllerConfig) -> Self {
        Self {
            guest_agent,
            config,
        }
    }

    pub fn poll(
        &mut self,
        requested_bytes: u64,
        current_bytes: u64,
    ) -> Result<ResizeDecision, PollError> {
        let response = self
            .guest_agent
            .get_memory_stats()
            .map_err(PollError::GuestAgent)?;
        let stats = parse_memory_stats(&response)?;

        Ok(plan_resize(
            &stats,
            requested_bytes,
            current_bytes,
            self.config,
        )?)
    }
}

#[derive(Debug)]
pub struct ServiceRuntime<A, S, R> {
    poller: MemoryPoller<A>,
    state_provider: S,
    resize_sink: R,
    interval: Duration,
    stop: StopSignal,
}

impl<A, S, R> ServiceRuntime<A, S, R>
where
    A: GuestAgent,
    S: MemoryStateProvider,
    R: ResizeRequestSink,
{
    pub fn new(
        poller: MemoryPoller<A>,
        state_provider: S,
        resize_sink: R,
        interval: Duration,
    ) -> Result<Self, ServiceLoopError> {
        if interval.is_zero() {
            return Err(ServiceLoopError::InvalidInterval);
        }

        Ok(Self {
            poller,
            state_provider,
            resize_sink,
            interval,
            stop: StopSignal::new(),
        })
    }

    pub fn request_stop(&self) {
        self.stop.cancel();
    }

    pub fn run(&mut self) -> Result<(), ServiceLoopError> {
        run_polling_loop(
            &mut self.poller,
            &mut self.state_provider,
            &mut self.resize_sink,
            self.interval,
            &self.stop,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtio_mem::VirtioMemState;

    const MIB: u64 = 1024 * 1024;

    struct StubGuestAgent {
        response: Result<String, String>,
    }

    impl GuestAgent for StubGuestAgent {
        fn get_memory_stats(&mut self) -> Result<String, String> {
            self.response.clone()
        }
    }

    struct StubState;

    impl MemoryStateProvider for StubState {
        fn memory_state(&mut self) -> Result<VirtioMemState, String> {
            Ok(VirtioMemState {
                size_bytes: 28 * MIB,
                block_size_bytes: 2 * MIB,
                requested_bytes: 16 * MIB,
                current_bytes: 16 * MIB,
            })
        }
    }

    #[derive(Default)]
    struct StubResize {
        requested: Vec<u64>,
    }

    impl ResizeRequestSink for StubResize {
        fn request_resize(&mut self, requested_bytes: u64) -> Result<(), String> {
            self.requested.push(requested_bytes);
            Ok(())
        }
    }

    fn config() -> MemoryControllerConfig {
        MemoryControllerConfig {
            min_memory_bytes: 8 * MIB,
            max_memory_bytes: 28 * MIB,
            lower_threshold_bytes: 2 * MIB,
            upper_threshold_bytes: 6 * MIB,
            block_size_bytes: 2 * MIB,
        }
    }

    #[test]
    fn parses_agent_response_and_plans_resize() {
        let agent = StubGuestAgent {
            response: Ok(r#"{"return":[
                    {"stat":"stat-free","value":1048576},
                    {"stat":"stat-total","value":16777216}
                ]}"#
            .to_owned()),
        };
        let mut poller = MemoryPoller::new(agent, config());

        assert_eq!(
            poller
                .poll(16 * MIB, 16 * MIB)
                .expect("poll should succeed"),
            ResizeDecision::Request {
                requested_bytes: 18 * MIB
            }
        );
    }

    #[test]
    fn returns_guest_agent_errors_without_panicking() {
        let agent = StubGuestAgent {
            response: Err("transport unavailable".to_owned()),
        };
        let mut poller = MemoryPoller::new(agent, config());

        assert_eq!(
            poller.poll(16, 16),
            Err(PollError::GuestAgent("transport unavailable".to_owned()))
        );
    }

    #[test]
    fn rejects_invalid_agent_responses() {
        let agent = StubGuestAgent {
            response: Ok("not json".to_owned()),
        };
        let mut poller = MemoryPoller::new(agent, config());

        assert!(matches!(
            poller.poll(16, 16),
            Err(PollError::MemoryStats(_))
        ));
    }

    #[test]
    fn rejects_zero_interval_when_constructing_runtime() {
        let poller = MemoryPoller::new(
            StubGuestAgent {
                response: Ok("{\"return\":[{\"stat\":\"stat-free\",\"value\":1},{\"stat\":\"stat-total\",\"value\":16}]}".to_owned()),
            },
            config(),
        );

        assert!(matches!(
            ServiceRuntime::new(poller, StubState, StubResize::default(), Duration::ZERO),
            Err(ServiceLoopError::InvalidInterval)
        ));
    }

    #[test]
    fn runtime_stops_when_cancelled_before_iteration() {
        let poller = MemoryPoller::new(
            StubGuestAgent {
                response: Ok("{\"return\":[{\"stat\":\"stat-free\",\"value\":1},{\"stat\":\"stat-total\",\"value\":16}]}".to_owned()),
            },
            config(),
        );
        let mut runtime = ServiceRuntime::new(
            poller,
            StubState,
            StubResize::default(),
            Duration::from_millis(1),
        )
        .expect("runtime should be constructible");

        runtime.request_stop();
        assert!(runtime.run().is_ok());
    }
}
