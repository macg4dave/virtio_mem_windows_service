use crate::controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
use crate::error::PollError;
use crate::stats::parse_memory_stats;

pub trait GuestAgent {
    fn get_memory_stats(&mut self) -> Result<String, String>;
}

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

#[cfg(test)]
mod tests {
    use super::*;

    struct StubGuestAgent {
        response: Result<String, String>,
    }

    impl GuestAgent for StubGuestAgent {
        fn get_memory_stats(&mut self) -> Result<String, String> {
            self.response.clone()
        }
    }

    fn config() -> MemoryControllerConfig {
        MemoryControllerConfig {
            min_memory_bytes: 8,
            max_memory_bytes: 28,
            lower_threshold_bytes: 2,
            upper_threshold_bytes: 6,
            block_size_bytes: 4,
        }
    }

    #[test]
    fn parses_agent_response_and_plans_resize() {
        let agent = StubGuestAgent {
            response: Ok(r#"{"return":[
                    {"stat":"stat-free","value":1},
                    {"stat":"stat-total","value":16}
                ]}"#
            .to_owned()),
        };
        let mut poller = MemoryPoller::new(agent, config());

        assert_eq!(
            poller.poll(16, 16).expect("poll should succeed"),
            ResizeDecision::Request {
                requested_bytes: 20
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
}