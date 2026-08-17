use crate::controller::ResizeDecision;
use crate::error::ServiceLoopError;
use crate::runtime::MemoryPoller;
use crate::service_host::StopSignal;
use std::time::Duration;

pub trait MemoryStateProvider {
    fn memory_state(&mut self) -> Result<(u64, u64), String>;
}

pub trait ResizeRequestSink {
    fn request_resize(&mut self, requested_bytes: u64) -> Result<(), String>;
}

/// Run one polling iteration and forward only an approved resize request.
pub fn poll_once<A, S, R>(
    poller: &mut MemoryPoller<A>,
    state_provider: &mut S,
    resize_sink: &mut R,
) -> Result<ResizeDecision, ServiceLoopError>
where
    A: crate::runtime::GuestAgent,
    S: MemoryStateProvider,
    R: ResizeRequestSink,
{
    let (requested_bytes, current_bytes) = state_provider
        .memory_state()
        .map_err(ServiceLoopError::StateProvider)?;
    let decision = poller
        .poll(requested_bytes, current_bytes)
        .map_err(ServiceLoopError::Poll)?;

    if let ResizeDecision::Request { requested_bytes } = decision {
        resize_sink
            .request_resize(requested_bytes)
            .map_err(ServiceLoopError::ResizeRequest)?;
    }

    Ok(decision)
}

/// Run polling until the caller cancels `stop`.
///
/// A polling or resize error stops the loop and is returned to the service
/// host for logging and recovery. The loop does not retry failed operations.
pub fn run_polling_loop<A, S, R>(
    poller: &mut MemoryPoller<A>,
    state_provider: &mut S,
    resize_sink: &mut R,
    interval: Duration,
    stop: &StopSignal,
) -> Result<(), ServiceLoopError>
where
    A: crate::runtime::GuestAgent,
    S: MemoryStateProvider,
    R: ResizeRequestSink,
{
    if interval.is_zero() {
        return Err(ServiceLoopError::InvalidInterval);
    }

    while !stop.is_cancelled() {
        poll_once(poller, state_provider, resize_sink)?;

        if !stop.is_cancelled() {
            stop.wait(interval);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::MemoryControllerConfig;
    use crate::runtime::GuestAgent;

    struct StubGuestAgent;

    impl GuestAgent for StubGuestAgent {
        fn get_memory_stats(&mut self) -> Result<String, String> {
            Ok(r#"{"return":[
                {"stat":"stat-free","value":1},
                {"stat":"stat-total","value":16}
            ]}"#
            .to_owned())
        }
    }

    struct StubState;

    impl MemoryStateProvider for StubState {
        fn memory_state(&mut self) -> Result<(u64, u64), String> {
            Ok((16, 16))
        }
    }

    struct FailingStateProvider;

    impl MemoryStateProvider for FailingStateProvider {
        fn memory_state(&mut self) -> Result<(u64, u64), String> {
            Err("state read failed".to_owned())
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

    struct FailingResize {
        failed: bool,
    }

    impl ResizeRequestSink for FailingResize {
        fn request_resize(&mut self, _requested_bytes: u64) -> Result<(), String> {
            if self.failed {
                Err("resize rejected".to_owned())
            } else {
                Ok(())
            }
        }
    }

    fn poller<A: GuestAgent>(agent: A) -> MemoryPoller<A> {
        MemoryPoller::new(
            agent,
            MemoryControllerConfig {
                min_memory_bytes: 8,
                max_memory_bytes: 28,
                lower_threshold_bytes: 2,
                upper_threshold_bytes: 6,
                block_size_bytes: 4,
            },
        )
    }

    #[test]
    fn forwards_only_controller_resize_requests() {
        let mut poller = poller(StubGuestAgent);
        let mut state = StubState;
        let mut resize = StubResize::default();

        let decision = poll_once(&mut poller, &mut state, &mut resize).expect("poll succeeds");

        assert_eq!(
            decision,
            ResizeDecision::Request {
                requested_bytes: 20
            }
        );
        assert_eq!(resize.requested, vec![20]);
    }

    #[test]
    fn does_not_call_resize_sink_for_no_change() {
        let mut poller = poller(NoChangeGuestAgent);
        let mut state = StubState;
        let mut resize = StubResize::default();

        assert_eq!(
            poll_once(&mut poller, &mut state, &mut resize).expect("poll succeeds"),
            ResizeDecision::NoChange
        );
        assert!(resize.requested.is_empty());
    }

    #[test]
    fn state_provider_failure_returns_explicit_error() {
        let mut poller = poller(StubGuestAgent);
        let mut state = FailingStateProvider;
        let mut resize = StubResize::default();

        assert_eq!(
            poll_once(&mut poller, &mut state, &mut resize),
            Err(ServiceLoopError::StateProvider("state read failed".to_owned()))
        );
    }

    #[test]
    fn resize_sink_failure_returns_explicit_error() {
        let mut poller = poller(StubGuestAgent);
        let mut state = StubState;
        let mut resize = FailingResize { failed: true };

        assert_eq!(
            poll_once(&mut poller, &mut state, &mut resize),
            Err(ServiceLoopError::ResizeRequest("resize rejected".to_owned()))
        );
    }

    #[test]
    fn rejects_zero_polling_interval() {
        let mut poller = poller(StubGuestAgent);
        let mut state = StubState;
        let mut resize = StubResize::default();
        let stop = StopSignal::new();

        assert_eq!(
            run_polling_loop(&mut poller, &mut state, &mut resize, Duration::ZERO, &stop,),
            Err(ServiceLoopError::InvalidInterval)
        );
    }

    #[test]
    fn stopped_loop_does_not_poll() {
        let mut poller = poller(StubGuestAgent);
        let mut state = StubState;
        let mut resize = StubResize::default();
        let stop = StopSignal::new();
        stop.cancel();

        run_polling_loop(
            &mut poller,
            &mut state,
            &mut resize,
            Duration::from_millis(1),
            &stop,
        )
        .expect("stopped loop should exit cleanly");

        assert!(resize.requested.is_empty());
    }

    struct NoChangeGuestAgent;

    impl GuestAgent for NoChangeGuestAgent {
        fn get_memory_stats(&mut self) -> Result<String, String> {
            Ok(r#"{"return":[
                {"stat":"stat-free","value":4},
                {"stat":"stat-total","value":16}
            ]}"#
            .to_owned())
        }
    }
}
