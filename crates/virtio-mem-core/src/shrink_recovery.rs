//! Fake-clock-friendly M10b Windows shrink qualification state machine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrinkPolicy {
    pub block_size_bytes: u64,
    pub hard_deadline_millis: u64,
    pub retry_delays_millis: [u64; 3],
}

impl ShrinkPolicy {
    pub const fn qualification(block_size_bytes: u64) -> Self {
        Self {
            block_size_bytes,
            hard_deadline_millis: 300_000,
            retry_delays_millis: [30_000, 60_000, 120_000],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrinkObservation {
    pub now_millis: u64,
    pub requested_bytes: u64,
    pub current_bytes: u64,
    pub fresh: bool,
    pub guest_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShrinkState {
    Observing,
    Converged,
    Stalled,
    RecoveryRequired { reason: String },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShrinkAction {
    Observe,
    Progress {
        blocks_reclaimed: u64,
    },
    Renotify {
        target_bytes: u64,
        retry_index: usize,
    },
    Converged,
    Latch {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbandonAction {
    Wait,
    Ready { target_bytes: u64 },
    Reject { reason: String },
}

/// Qualifies a one-shot abandon-to-current target using two stable samples and
/// a mandatory immediate pre-apply observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbandonToCurrent {
    immutable_shrink_target_bytes: u64,
    block_size_bytes: u64,
    candidate_bytes: Option<u64>,
    stable_samples: u8,
}

impl AbandonToCurrent {
    pub fn new(immutable_shrink_target_bytes: u64, block_size_bytes: u64) -> Result<Self, String> {
        if block_size_bytes == 0 || !immutable_shrink_target_bytes.is_multiple_of(block_size_bytes)
        {
            return Err("invalid abandon-to-current geometry".to_owned());
        }
        Ok(Self {
            immutable_shrink_target_bytes,
            block_size_bytes,
            candidate_bytes: None,
            stable_samples: 0,
        })
    }

    pub fn observe(&mut self, observation: ShrinkObservation) -> AbandonAction {
        if !observation.fresh
            || !observation.guest_running
            || observation.requested_bytes != self.immutable_shrink_target_bytes
            || observation.current_bytes <= self.immutable_shrink_target_bytes
            || !observation
                .current_bytes
                .is_multiple_of(self.block_size_bytes)
        {
            return AbandonAction::Reject {
                reason: "abandon recovery requires fresh stable shrink divergence".to_owned(),
            };
        }
        if self.candidate_bytes == Some(observation.current_bytes) {
            self.stable_samples = self.stable_samples.saturating_add(1);
        } else {
            self.candidate_bytes = Some(observation.current_bytes);
            self.stable_samples = 1;
        }
        if self.stable_samples >= 2 {
            AbandonAction::Ready {
                target_bytes: observation.current_bytes,
            }
        } else {
            AbandonAction::Wait
        }
    }

    pub fn verify_immediately_before_apply(
        &self,
        observation: ShrinkObservation,
    ) -> Result<u64, String> {
        let candidate = self
            .candidate_bytes
            .filter(|_| self.stable_samples >= 2)
            .ok_or_else(|| "two stable samples have not qualified recovery".to_owned())?;
        if !observation.fresh
            || !observation.guest_running
            || observation.requested_bytes != self.immutable_shrink_target_bytes
            || observation.current_bytes != candidate
        {
            return Err("live state changed before abandon-to-current apply".to_owned());
        }
        Ok(candidate)
    }
}

/// Tracks one owned immutable-target operation. State is intentionally not
/// serializable: process restart cannot recreate ownership or replay commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShrinkOperation {
    policy: ShrinkPolicy,
    target_bytes: u64,
    started_millis: u64,
    last_progress_millis: u64,
    last_current_bytes: u64,
    retry_count: usize,
    state: ShrinkState,
}

impl ShrinkOperation {
    pub fn start(
        policy: ShrinkPolicy,
        now_millis: u64,
        initial_current_bytes: u64,
        target_bytes: u64,
        safe_floor_bytes: u64,
    ) -> Result<Self, String> {
        if policy.block_size_bytes == 0
            || policy.hard_deadline_millis == 0
            || policy.retry_delays_millis.contains(&0)
            || !initial_current_bytes.is_multiple_of(policy.block_size_bytes)
            || !target_bytes.is_multiple_of(policy.block_size_bytes)
            || target_bytes >= initial_current_bytes
            || target_bytes < safe_floor_bytes
        {
            return Err("invalid shrink geometry or policy".to_owned());
        }
        Ok(Self {
            policy,
            target_bytes,
            started_millis: now_millis,
            last_progress_millis: now_millis,
            last_current_bytes: initial_current_bytes,
            retry_count: 0,
            state: ShrinkState::Observing,
        })
    }

    /// Creates the only safe post-restart state for an unowned divergence.
    pub fn recovery_required(reason: impl Into<String>) -> Self {
        Self {
            policy: ShrinkPolicy::qualification(1),
            target_bytes: 0,
            started_millis: 0,
            last_progress_millis: 0,
            last_current_bytes: 0,
            retry_count: 0,
            state: ShrinkState::RecoveryRequired {
                reason: reason.into(),
            },
        }
    }

    pub fn state(&self) -> &ShrinkState {
        &self.state
    }

    pub fn retry_count(&self) -> usize {
        self.retry_count
    }

    pub fn cancel(&mut self) -> ShrinkAction {
        self.state = ShrinkState::Cancelled;
        ShrinkAction::Latch {
            reason: "operation cancelled; command replay is forbidden".to_owned(),
        }
    }

    pub fn observe(&mut self, observation: ShrinkObservation) -> ShrinkAction {
        if self.state != ShrinkState::Observing {
            return ShrinkAction::Observe;
        }
        let latch_reason = if !observation.fresh {
            Some("stale or invalid live state")
        } else if !observation.guest_running {
            Some("guest lifecycle transition")
        } else if observation.requested_bytes != self.target_bytes {
            Some("requested target changed outside the owned operation")
        } else if !observation
            .current_bytes
            .is_multiple_of(self.policy.block_size_bytes)
        {
            Some("current allocation is not block aligned")
        } else if observation.current_bytes < self.target_bytes
            || observation.current_bytes > self.last_current_bytes
        {
            Some("current allocation is inconsistent or non-monotonic")
        } else if observation.now_millis < self.last_progress_millis {
            Some("monotonic clock moved backwards")
        } else {
            None
        };
        if let Some(reason) = latch_reason {
            return self.latch(reason);
        }
        if observation.current_bytes == self.target_bytes {
            self.last_current_bytes = observation.current_bytes;
            self.state = ShrinkState::Converged;
            return ShrinkAction::Converged;
        }
        if observation.now_millis.saturating_sub(self.started_millis)
            >= self.policy.hard_deadline_millis
        {
            self.state = ShrinkState::Stalled;
            return ShrinkAction::Latch {
                reason: "immutable shrink deadline expired".to_owned(),
            };
        }
        if observation.current_bytes < self.last_current_bytes {
            let reclaimed = (self.last_current_bytes - observation.current_bytes)
                / self.policy.block_size_bytes;
            self.last_current_bytes = observation.current_bytes;
            self.last_progress_millis = observation.now_millis;
            return ShrinkAction::Progress {
                blocks_reclaimed: reclaimed,
            };
        }
        if let Some(delay) = self.policy.retry_delays_millis.get(self.retry_count) {
            if observation
                .now_millis
                .saturating_sub(self.last_progress_millis)
                >= *delay
                && observation.now_millis.saturating_sub(self.started_millis)
                    < self.policy.hard_deadline_millis
            {
                self.retry_count += 1;
                self.last_progress_millis = observation.now_millis;
                return ShrinkAction::Renotify {
                    target_bytes: self.target_bytes,
                    retry_index: self.retry_count,
                };
            }
        }
        ShrinkAction::Observe
    }

    pub fn uncertain_command_result(&mut self) -> ShrinkAction {
        self.latch("resize command outcome is ambiguous")
    }

    fn latch(&mut self, reason: &str) -> ShrinkAction {
        self.state = ShrinkState::RecoveryRequired {
            reason: reason.to_owned(),
        };
        ShrinkAction::Latch {
            reason: reason.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn operation() -> ShrinkOperation {
        ShrinkOperation::start(
            ShrinkPolicy::qualification(2 * MIB),
            1_000,
            12 * MIB,
            4 * MIB,
            4 * MIB,
        )
        .expect("valid operation")
    }

    fn observation(now: u64, current: u64) -> ShrinkObservation {
        ShrinkObservation {
            now_millis: now,
            requested_bytes: 4 * MIB,
            current_bytes: current,
            fresh: true,
            guest_running: true,
        }
    }

    #[test]
    fn retries_exact_immutable_target_on_bounded_schedule() {
        let mut operation = operation();
        assert_eq!(
            operation.observe(observation(30_999, 12 * MIB)),
            ShrinkAction::Observe
        );
        assert_eq!(
            operation.observe(observation(31_000, 12 * MIB)),
            ShrinkAction::Renotify {
                target_bytes: 4 * MIB,
                retry_index: 1
            }
        );
        assert_eq!(
            operation.observe(observation(91_000, 12 * MIB)),
            ShrinkAction::Renotify {
                target_bytes: 4 * MIB,
                retry_index: 2
            }
        );
        assert_eq!(
            operation.observe(observation(211_000, 12 * MIB)),
            ShrinkAction::Renotify {
                target_bytes: 4 * MIB,
                retry_index: 3
            }
        );
        assert_eq!(operation.retry_count(), 3);
        assert_eq!(
            operation.observe(observation(250_000, 12 * MIB)),
            ShrinkAction::Observe
        );
    }

    #[test]
    fn progress_moves_clock_without_replenishing_retry_budget() {
        let mut operation = operation();
        assert!(matches!(
            operation.observe(observation(31_000, 12 * MIB)),
            ShrinkAction::Renotify { retry_index: 1, .. }
        ));
        assert_eq!(
            operation.observe(observation(40_000, 8 * MIB)),
            ShrinkAction::Progress {
                blocks_reclaimed: 2
            }
        );
        assert_eq!(
            operation.observe(observation(99_999, 8 * MIB)),
            ShrinkAction::Observe
        );
        assert!(matches!(
            operation.observe(observation(100_000, 8 * MIB)),
            ShrinkAction::Renotify { retry_index: 2, .. }
        ));
        assert_eq!(operation.retry_count(), 2);
    }

    #[test]
    fn converges_or_latches_at_original_deadline() {
        let mut converging = operation();
        assert_eq!(
            converging.observe(observation(2_000, 4 * MIB)),
            ShrinkAction::Converged
        );
        assert_eq!(converging.state(), &ShrinkState::Converged);

        let mut stalled = operation();
        assert!(matches!(
            stalled.observe(observation(301_000, 12 * MIB)),
            ShrinkAction::Latch { .. }
        ));
        assert_eq!(stalled.state(), &ShrinkState::Stalled);
    }

    #[test]
    fn stale_external_nonmonotonic_transition_and_ambiguity_latch() {
        let mut stale = operation();
        let mut value = observation(2_000, 12 * MIB);
        value.fresh = false;
        assert!(matches!(stale.observe(value), ShrinkAction::Latch { .. }));

        let mut external = operation();
        value = observation(2_000, 12 * MIB);
        value.requested_bytes = 6 * MIB;
        assert!(matches!(
            external.observe(value),
            ShrinkAction::Latch { .. }
        ));

        let mut increasing = operation();
        assert!(matches!(
            increasing.observe(observation(2_000, 14 * MIB)),
            ShrinkAction::Latch { .. }
        ));

        let mut transition = operation();
        value = observation(2_000, 12 * MIB);
        value.guest_running = false;
        assert!(matches!(
            transition.observe(value),
            ShrinkAction::Latch { .. }
        ));

        let mut ambiguous = operation();
        assert!(matches!(
            ambiguous.uncertain_command_result(),
            ShrinkAction::Latch { .. }
        ));
    }

    #[test]
    fn cancellation_and_restart_never_replay() {
        let mut operation = operation();
        assert!(matches!(operation.cancel(), ShrinkAction::Latch { .. }));
        assert_eq!(
            operation.observe(observation(31_000, 12 * MIB)),
            ShrinkAction::Observe
        );
        assert!(matches!(
            ShrinkOperation::recovery_required("restart").state(),
            ShrinkState::RecoveryRequired { .. }
        ));
    }

    #[test]
    fn rejects_invalid_start_geometry_and_safe_floor() {
        assert!(
            ShrinkOperation::start(ShrinkPolicy::qualification(2 * MIB), 0, 8 * MIB, 9, 0).is_err()
        );
        assert!(ShrinkOperation::start(
            ShrinkPolicy::qualification(2 * MIB),
            0,
            8 * MIB,
            4 * MIB,
            6 * MIB
        )
        .is_err());
    }

    #[test]
    fn abandon_to_current_requires_two_stable_samples_and_immediate_reread() {
        let mut recovery = AbandonToCurrent::new(4 * MIB, 2 * MIB).expect("valid recovery");
        assert_eq!(
            recovery.observe(observation(1, 8 * MIB)),
            AbandonAction::Wait
        );
        assert_eq!(
            recovery.observe(observation(2, 6 * MIB)),
            AbandonAction::Wait
        );
        assert_eq!(
            recovery.observe(observation(3, 6 * MIB)),
            AbandonAction::Ready {
                target_bytes: 6 * MIB
            }
        );
        assert_eq!(
            recovery.verify_immediately_before_apply(observation(4, 6 * MIB)),
            Ok(6 * MIB)
        );
        assert!(recovery
            .verify_immediately_before_apply(observation(4, 8 * MIB))
            .is_err());
    }
}
