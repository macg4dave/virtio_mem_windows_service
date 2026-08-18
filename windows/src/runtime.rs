use std::time::Duration;

use crate::controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
use crate::demand::{
    DemandAgent, DemandAgentError, DemandReport, DemandReportPublisher, MemoryTelemetry,
};
use crate::error::{PollError, ServiceLoopError};
use crate::service_host::{ServiceWorker, StopSignal};
use crate::service_loop::{run_polling_loop, MemoryStateProvider, ResizeRequestSink};
use crate::stats::parse_memory_stats;

pub trait GuestAgent {
    fn get_memory_stats(&mut self) -> Result<String, String>;
}

/// Guest-side polling worker wired to the configured QEMU Guest Agent.
///
/// This worker acquires and validates QGA memory statistics only. It does not
/// infer virtio-mem allocation state and does not expose a resize sink.
#[derive(Debug)]
pub struct QgaPollingWorker<A> {
    guest_agent: A,
    interval: Duration,
}

impl<A> QgaPollingWorker<A>
where
    A: GuestAgent,
{
    pub fn new(guest_agent: A, interval: Duration) -> Result<Self, String> {
        if interval.is_zero() {
            return Err("QGA polling interval must be greater than zero".to_owned());
        }
        Ok(Self {
            guest_agent,
            interval,
        })
    }

    pub fn poll_once(&mut self) -> Result<crate::stats::MemoryStats, String> {
        let response = self.guest_agent.get_memory_stats()?;
        crate::stats::parse_memory_stats(&response).map_err(|error| error.to_string())
    }
}

impl<A> ServiceWorker for QgaPollingWorker<A>
where
    A: GuestAgent + Send + 'static,
{
    fn initialize(&mut self, _stop: &StopSignal) -> Result<(), String> {
        self.poll_once()
            .map(|_| ())
            .map_err(|error| format!("initial QGA memory-stat acquisition failed: {error}"))
    }

    fn run(&mut self, stop: &StopSignal) -> Result<(), String> {
        while !stop.is_cancelled() {
            self.poll_once()
                .map_err(|error| format!("QGA memory-stat acquisition failed: {error}"))?;
            if !stop.is_cancelled() {
                stop.wait(self.interval);
            }
        }
        Ok(())
    }
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

/// Advisory demand worker that never exposes a resize sink.
#[derive(Debug)]
pub struct DemandServiceWorker<T, S, P> {
    demand_agent: DemandAgent<T>,
    state_provider: S,
    publisher: P,
    interval: Duration,
}

impl<T, S, P> DemandServiceWorker<T, S, P>
where
    T: MemoryTelemetry,
    S: MemoryStateProvider,
    P: DemandReportPublisher,
{
    pub fn new(
        demand_agent: DemandAgent<T>,
        state_provider: S,
        publisher: P,
        interval: Duration,
    ) -> Result<Self, String> {
        if interval.is_zero() {
            return Err("demand worker interval must be greater than zero".to_owned());
        }
        Ok(Self {
            demand_agent,
            state_provider,
            publisher,
            interval,
        })
    }

    pub fn run_once(&mut self) -> Result<DemandReport, String> {
        let state = self
            .state_provider
            .memory_state()
            .map_err(|error| format!("read current allocation: {error}"))?;
        state
            .validate()
            .map_err(|error| format!("validate current allocation: {error}"))?;
        self.demand_agent
            .collect_and_publish(state.current_bytes, &mut self.publisher)
            .map_err(|error| match error {
                DemandAgentError::Telemetry(error) => format!("demand telemetry: {error}"),
                DemandAgentError::Publication(error) => format!("demand publication: {error}"),
            })
    }

    pub fn publisher(&self) -> &P {
        &self.publisher
    }
}

impl<T, S, P> ServiceWorker for DemandServiceWorker<T, S, P>
where
    T: MemoryTelemetry,
    S: MemoryStateProvider,
    P: DemandReportPublisher,
{
    fn run(&mut self, stop: &StopSignal) -> Result<(), String> {
        while !stop.is_cancelled() {
            self.run_once()?;
            if !stop.is_cancelled() {
                stop.wait(self.interval);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demand::{
        DemandCalculator, DemandPolicyConfig, DemandReportPublisher, MemoryTelemetrySnapshot,
    };
    use crate::virtio_mem::VirtioMemState;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    #[derive(Clone, Copy, Debug)]
    struct TestMemoryStateProvider {
        state: VirtioMemState,
    }

    impl TestMemoryStateProvider {
        fn new(state: VirtioMemState) -> Self {
            Self { state }
        }
    }

    impl MemoryStateProvider for TestMemoryStateProvider {
        fn memory_state(&mut self) -> Result<VirtioMemState, String> {
            Ok(self.state)
        }
    }

    #[derive(Debug, Default)]
    struct TestResizeSink {
        requests: Vec<u64>,
    }

    impl TestResizeSink {
        fn requests(&self) -> Vec<u64> {
            self.requests.clone()
        }
    }

    impl ResizeRequestSink for TestResizeSink {
        fn request_resize(&mut self, requested_bytes: u64) -> Result<(), String> {
            self.requests.push(requested_bytes);
            Ok(())
        }
    }

    struct StubGuestAgent {
        response: Result<String, String>,
    }

    impl GuestAgent for StubGuestAgent {
        fn get_memory_stats(&mut self) -> Result<String, String> {
            self.response.clone()
        }
    }

    #[test]
    fn qga_worker_validates_initial_memory_stats_before_running() {
        let mut worker = QgaPollingWorker::new(
            StubGuestAgent {
                response: Ok(r#"{"return":[
                    {"stat":"stat-free","value":1048576},
                    {"stat":"stat-total","value":16777216}
                ]}"#
                .to_owned()),
            },
            Duration::from_secs(1),
        )
        .expect("worker should be constructible");

        worker
            .initialize(&StopSignal::new())
            .expect("initial QGA stats should validate");
    }

    #[test]
    fn qga_worker_preserves_initial_transport_failure() {
        let mut worker = QgaPollingWorker::new(
            StubGuestAgent {
                response: Err("pipe unavailable".to_owned()),
            },
            Duration::from_secs(1),
        )
        .expect("worker should be constructible");

        assert_eq!(
            worker.initialize(&StopSignal::new()),
            Err("initial QGA memory-stat acquisition failed: pipe unavailable".to_owned())
        );
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

    #[test]
    fn test_runtime_harness_records_resize_requests() {
        let mut provider = TestMemoryStateProvider::new(VirtioMemState {
            size_bytes: 32 * MIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 16 * MIB,
            current_bytes: 16 * MIB,
        });
        let mut sink = TestResizeSink::default();

        assert_eq!(provider.memory_state().unwrap().requested_bytes, 16 * MIB);
        assert!(sink.request_resize(18 * MIB).is_ok());
        assert_eq!(sink.requests(), vec![18 * MIB]);
    }

    #[derive(Clone)]
    struct DemandTelemetryFixture;

    impl MemoryTelemetry for DemandTelemetryFixture {
        fn collect(&self) -> Result<MemoryTelemetrySnapshot, crate::demand::DemandError> {
            Ok(MemoryTelemetrySnapshot {
                physical_total_bytes: 16 * GIB,
                physical_available_bytes: 2 * GIB,
                memory_load_percent: 88,
                commit_total_bytes: 15 * GIB,
                commit_limit_bytes: 16 * GIB,
                commit_peak_bytes: 15 * GIB,
                system_cache_bytes: 0,
                kernel_paged_bytes: 0,
                kernel_nonpaged_bytes: 0,
            })
        }
    }

    #[derive(Default, Debug)]
    struct DemandPublisherFixture {
        reports: Vec<crate::demand::DemandReport>,
    }

    impl DemandReportPublisher for DemandPublisherFixture {
        fn publish(&mut self, report: crate::demand::DemandReport) -> Result<(), String> {
            self.reports.push(report);
            Ok(())
        }
    }

    struct DemandStateFixture;

    impl MemoryStateProvider for DemandStateFixture {
        fn memory_state(&mut self) -> Result<VirtioMemState, String> {
            Ok(VirtioMemState {
                size_bytes: 32 * GIB,
                block_size_bytes: 2 * GIB,
                requested_bytes: 30 * GIB,
                current_bytes: 30 * GIB,
            })
        }
    }

    #[test]
    fn advisory_worker_publishes_using_observed_current_state() {
        let calculator = DemandCalculator::new(DemandPolicyConfig {
            configured_minimum_bytes: 4 * GIB,
            configured_maximum_bytes: 32 * GIB,
            block_size_bytes: 2 * GIB,
        })
        .expect("demand policy should be valid");
        let agent = DemandAgent::new(DemandTelemetryFixture, calculator);
        let mut worker = DemandServiceWorker::new(
            agent,
            DemandStateFixture,
            DemandPublisherFixture::default(),
            Duration::from_secs(1),
        )
        .expect("worker should be valid");

        let report = worker.run_once().expect("worker cycle should succeed");

        assert_eq!(report.demand.state, crate::demand::DemandState::Critical);
        assert_eq!(worker.publisher().reports, vec![report]);
    }

    #[test]
    fn advisory_worker_rejects_zero_interval() {
        let calculator = DemandCalculator::new(DemandPolicyConfig {
            configured_minimum_bytes: 4 * GIB,
            configured_maximum_bytes: 32 * GIB,
            block_size_bytes: 2 * GIB,
        })
        .expect("demand policy should be valid");
        let result = DemandServiceWorker::new(
            DemandAgent::new(DemandTelemetryFixture, calculator),
            DemandStateFixture,
            DemandPublisherFixture::default(),
            Duration::ZERO,
        );

        assert!(result.is_err());
    }
}
