//! Reconciliation of durable policy intent with asynchronous device state.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::VirtioMemState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlHealth {
    Converged,
    Growing,
    Shrinking,
    Constrained,
    RecoveryRequired,
    CommandUnknown,
    Latched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileDirection {
    Grow,
    Shrink,
    SupersedeShrink,
    FreezeShrink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileAction {
    None,
    Request {
        target_bytes: u64,
        direction: ReconcileDirection,
        latch_after_request: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileDecision {
    pub health: ControlHealth,
    pub action: ReconcileAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileInput {
    pub desired_bytes: u64,
    pub safe_floor_bytes: u64,
    pub effective_maximum_bytes: u64,
    pub history_ready: bool,
    pub telemetry_fresh: bool,
    pub automatic_shrink: bool,
    pub actuation_latched: bool,
    pub owns_pending_shrink: bool,
    pub pending_constrained: bool,
    pub grow_step_bytes: u64,
    pub shrink_step_bytes: u64,
    pub live: VirtioMemState,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReconcileError {
    #[error("reconciler input is invalid: {0}")]
    InvalidInput(&'static str),
    #[error("reconciler arithmetic overflow")]
    ArithmeticOverflow,
}

pub fn reconcile(input: ReconcileInput) -> Result<ReconcileDecision, ReconcileError> {
    input
        .live
        .validate()
        .map_err(|_| ReconcileError::InvalidInput("invalid live device state"))?;
    if input.desired_bytes == 0
        || input.safe_floor_bytes == 0
        || input.effective_maximum_bytes == 0
        || input.grow_step_bytes == 0
        || input.shrink_step_bytes == 0
    {
        return Err(ReconcileError::InvalidInput(
            "targets, limits, and movement quanta must be positive",
        ));
    }
    if input.safe_floor_bytes > input.desired_bytes
        || input.desired_bytes > input.effective_maximum_bytes
        || !input
            .desired_bytes
            .is_multiple_of(input.live.block_size_bytes)
        || !input
            .safe_floor_bytes
            .is_multiple_of(input.live.block_size_bytes)
        || !input
            .grow_step_bytes
            .is_multiple_of(input.live.block_size_bytes)
        || !input
            .shrink_step_bytes
            .is_multiple_of(input.live.block_size_bytes)
    {
        return Err(ReconcileError::InvalidInput(
            "targets and movement quanta violate ordering or alignment",
        ));
    }

    let requested = input.live.requested_bytes;
    let current = input.live.current_bytes;
    if input.actuation_latched {
        return Ok(decision(ControlHealth::Latched, ReconcileAction::None));
    }

    if requested > current {
        return Ok(decision(ControlHealth::Growing, ReconcileAction::None));
    }

    if requested < current {
        if !input.owns_pending_shrink {
            return Ok(decision(
                ControlHealth::RecoveryRequired,
                ReconcileAction::None,
            ));
        }
        if !input.telemetry_fresh {
            return Ok(request(
                ControlHealth::RecoveryRequired,
                current,
                ReconcileDirection::FreezeShrink,
                true,
            ));
        }
        if input.desired_bytes <= requested {
            return Ok(decision(
                if input.pending_constrained {
                    ControlHealth::Constrained
                } else {
                    ControlHealth::Shrinking
                },
                ReconcileAction::None,
            ));
        }
        let target_bytes = input.desired_bytes.min(current);
        return Ok(request(
            ControlHealth::Shrinking,
            target_bytes,
            ReconcileDirection::SupersedeShrink,
            false,
        ));
    }

    if !input.telemetry_fresh {
        return Ok(decision(ControlHealth::Converged, ReconcileAction::None));
    }
    if current < input.desired_bytes {
        let target_bytes = current
            .checked_add(input.grow_step_bytes)
            .ok_or(ReconcileError::ArithmeticOverflow)?
            .min(input.desired_bytes)
            .min(input.effective_maximum_bytes);
        return Ok(request(
            ControlHealth::Converged,
            target_bytes,
            ReconcileDirection::Grow,
            false,
        ));
    }
    if current > input.desired_bytes && input.automatic_shrink && input.history_ready {
        let target_bytes = current
            .saturating_sub(input.shrink_step_bytes)
            .max(input.desired_bytes)
            .max(input.safe_floor_bytes);
        return Ok(request(
            ControlHealth::Converged,
            target_bytes,
            ReconcileDirection::Shrink,
            false,
        ));
    }
    Ok(decision(ControlHealth::Converged, ReconcileAction::None))
}

fn decision(health: ControlHealth, action: ReconcileAction) -> ReconcileDecision {
    ReconcileDecision { health, action }
}

fn request(
    health: ControlHealth,
    target_bytes: u64,
    direction: ReconcileDirection,
    latch_after_request: bool,
) -> ReconcileDecision {
    decision(
        health,
        ReconcileAction::Request {
            target_bytes,
            direction,
            latch_after_request,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn input(requested: u64, current: u64, desired: u64) -> ReconcileInput {
        ReconcileInput {
            desired_bytes: desired,
            safe_floor_bytes: 4 * GIB,
            effective_maximum_bytes: 31 * GIB,
            history_ready: true,
            telemetry_fresh: true,
            automatic_shrink: true,
            actuation_latched: false,
            owns_pending_shrink: false,
            pending_constrained: false,
            grow_step_bytes: GIB,
            shrink_step_bytes: 64 * MIB,
            live: VirtioMemState {
                size_bytes: 32 * GIB,
                block_size_bytes: 2 * MIB,
                requested_bytes: requested,
                current_bytes: current,
            },
        }
    }

    #[test]
    fn converged_state_moves_by_bounded_quanta() {
        assert_eq!(
            reconcile(input(8 * GIB, 8 * GIB, 12 * GIB)).expect("grow"),
            request(
                ControlHealth::Converged,
                9 * GIB,
                ReconcileDirection::Grow,
                false
            )
        );
        assert_eq!(
            reconcile(input(8 * GIB, 8 * GIB, 6 * GIB)).expect("shrink"),
            request(
                ControlHealth::Converged,
                8 * GIB - 64 * MIB,
                ReconcileDirection::Shrink,
                false
            )
        );
    }

    #[test]
    fn pending_growth_is_never_overlapped() {
        let mut value = input(10 * GIB, 8 * GIB, 12 * GIB);
        value.owns_pending_shrink = true;
        assert_eq!(
            reconcile(value).expect("observe"),
            decision(ControlHealth::Growing, ReconcileAction::None)
        );
    }

    #[test]
    fn unowned_divergence_requires_recovery() {
        assert_eq!(
            reconcile(input(6 * GIB, 8 * GIB, 5 * GIB)).expect("recover"),
            decision(ControlHealth::RecoveryRequired, ReconcileAction::None)
        );
    }

    #[test]
    fn owned_shrink_never_receives_a_second_lower_target() {
        let mut value = input(6 * GIB, 8 * GIB, 5 * GIB);
        value.owns_pending_shrink = true;
        assert_eq!(
            reconcile(value).expect("observe"),
            decision(ControlHealth::Shrinking, ReconcileAction::None)
        );
    }

    #[test]
    fn renewed_pressure_only_supersedes_upward_as_far_as_current() {
        let mut value = input(6 * GIB, 8 * GIB, 7 * GIB);
        value.owns_pending_shrink = true;
        assert_eq!(
            reconcile(value).expect("raise pending target"),
            request(
                ControlHealth::Shrinking,
                7 * GIB,
                ReconcileDirection::SupersedeShrink,
                false
            )
        );

        value.desired_bytes = 10 * GIB;
        assert_eq!(
            reconcile(value).expect("cancel at current"),
            request(
                ControlHealth::Shrinking,
                8 * GIB,
                ReconcileDirection::SupersedeShrink,
                false
            )
        );
    }

    #[test]
    fn stale_owned_shrink_freezes_at_current_and_requests_a_latch() {
        let mut value = input(6 * GIB, 8 * GIB, 5 * GIB);
        value.owns_pending_shrink = true;
        value.telemetry_fresh = false;
        assert_eq!(
            reconcile(value).expect("freeze"),
            request(
                ControlHealth::RecoveryRequired,
                8 * GIB,
                ReconcileDirection::FreezeShrink,
                true
            )
        );
    }

    #[test]
    fn a_durable_latch_suppresses_every_action() {
        let mut value = input(8 * GIB, 8 * GIB, 12 * GIB);
        value.actuation_latched = true;
        assert_eq!(
            reconcile(value).expect("latched"),
            decision(ControlHealth::Latched, ReconcileAction::None)
        );
    }
}
