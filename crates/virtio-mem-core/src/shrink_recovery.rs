//! Manual recovery for a stalled Windows shrink: qualify an abandon-to-current
//! target from stable observations before the operator applies it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrinkObservation {
    pub now_millis: u64,
    pub requested_bytes: u64,
    pub current_bytes: u64,
    pub fresh: bool,
    pub guest_running: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

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
