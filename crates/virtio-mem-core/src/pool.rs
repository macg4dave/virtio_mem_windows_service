//! Shared-pool allocation across VMs, as a pure function of one round's inputs.
//!
//! The pool budgets virtio-mem device bytes. A member's charge is
//! `max(requested, current)` read from live state, so memory a guest is still
//! giving back stays charged until its `current` actually drops: a stalled
//! release is never handed to another VM. No pool state is persisted; every
//! round is recomputed from live state.
//!
//! Allocation never forces a shrink by itself. It only lowers each member's
//! ceiling, which the per-VM reconciler then applies to its desired size:
//! - growth comes only from free pool bytes, highest priority first;
//! - when a member is still short after that, lower-priority members that may
//!   shrink get a ceiling below their charge (never below their safe floor),
//!   and their normal, stepped shrink frees the bytes for a later round.

use thiserror::Error;

/// One member's inputs for a round, in device bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolMember {
    /// Larger is served first. Equal priorities never reclaim from each other.
    pub priority: u32,
    /// `max(requested, current)`, or `None` when live state was unreadable.
    pub charged_bytes: Option<u64>,
    /// The member's own desired size; equal to its charge when it holds.
    pub desired_bytes: u64,
    pub safe_floor_bytes: u64,
    pub block_bytes: u64,
    /// Whether this member may shrink this round to make room for others.
    pub can_donate: bool,
}

/// One member's result for a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolAllocation {
    /// The member's desired size must be clamped to at most this.
    pub ceiling_bytes: u64,
    /// Growth the member wanted but the pool could not grant this round.
    pub unmet_bytes: u64,
    /// Bytes this member is asked to give back for higher-priority members.
    pub reclaim_bytes: u64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PoolError {
    #[error("pool member block size must be a non-zero power of two")]
    InvalidBlock,
    #[error("pool accounting overflowed")]
    Overflow,
}

/// Allocates one round. Results are in the same order as `members`.
pub fn allocate_pool(
    pool_bytes: u64,
    members: &[PoolMember],
) -> Result<Vec<PoolAllocation>, PoolError> {
    if members
        .iter()
        .any(|member| member.block_bytes == 0 || !member.block_bytes.is_power_of_two())
    {
        return Err(PoolError::InvalidBlock);
    }
    // An unreadable member's charge is unknown, so nothing may grow: any free
    // figure could already be spent by that member.
    let Some(charges) = members
        .iter()
        .map(|member| member.charged_bytes)
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(members
            .iter()
            .map(|member| {
                let charged = member.charged_bytes.unwrap_or(member.desired_bytes);
                PoolAllocation {
                    ceiling_bytes: charged,
                    unmet_bytes: member.desired_bytes.saturating_sub(charged),
                    reclaim_bytes: 0,
                }
            })
            .collect());
    };
    let charged_total = charges
        .iter()
        .try_fold(0_u64, |total, charge| total.checked_add(*charge))
        .ok_or(PoolError::Overflow)?;
    let mut free = pool_bytes.saturating_sub(charged_total);

    // Highest priority first; configuration order breaks ties.
    let mut order = (0..members.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| members[*right].priority.cmp(&members[*left].priority));

    let mut allocations = charges
        .iter()
        .map(|charge| PoolAllocation {
            ceiling_bytes: *charge,
            unmet_bytes: 0,
            reclaim_bytes: 0,
        })
        .collect::<Vec<_>>();
    for &index in &order {
        let member = &members[index];
        let wanted = member.desired_bytes.saturating_sub(charges[index]);
        let granted = align_down(wanted.min(free), member.block_bytes);
        free -= granted;
        allocations[index].ceiling_bytes = charges[index] + granted;
        allocations[index].unmet_bytes = wanted - granted;
    }

    // Lowest-priority donors give first; a donor serves recipients in order.
    for &recipient in &order {
        let mut unmet = allocations[recipient].unmet_bytes;
        for &donor in order.iter().rev() {
            if unmet == 0 {
                break;
            }
            let giver = &members[donor];
            if giver.priority >= members[recipient].priority || !giver.can_donate {
                continue;
            }
            let alignment = giver.block_bytes.max(members[recipient].block_bytes);
            let floor = align_up(giver.safe_floor_bytes, giver.block_bytes)?;
            let spare = allocations[donor].ceiling_bytes.saturating_sub(floor);
            let bytes = align_down(spare.min(unmet), alignment);
            if bytes == 0 {
                continue;
            }
            allocations[donor].ceiling_bytes -= bytes;
            allocations[donor].reclaim_bytes += bytes;
            unmet -= bytes;
        }
    }
    Ok(allocations)
}

fn align_down(value: u64, alignment: u64) -> u64 {
    value - value % alignment
}

fn align_up(value: u64, alignment: u64) -> Result<u64, PoolError> {
    match value % alignment {
        0 => Ok(value),
        remainder => value
            .checked_add(alignment - remainder)
            .ok_or(PoolError::Overflow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn member(priority: u32, charged: u64, desired: u64) -> PoolMember {
        PoolMember {
            priority,
            charged_bytes: Some(charged),
            desired_bytes: desired,
            safe_floor_bytes: GIB,
            block_bytes: 2 * MIB,
            can_donate: true,
        }
    }

    fn ceilings(allocations: &[PoolAllocation]) -> Vec<u64> {
        allocations.iter().map(|a| a.ceiling_bytes).collect()
    }

    #[test]
    fn growth_that_fits_is_granted_to_everyone() {
        let allocations = allocate_pool(
            16 * GIB,
            &[member(100, 4 * GIB, 6 * GIB), member(80, 4 * GIB, 6 * GIB)],
        )
        .expect("allocation");
        assert_eq!(ceilings(&allocations), [6 * GIB, 6 * GIB]);
        assert!(allocations
            .iter()
            .all(|a| a.unmet_bytes == 0 && a.reclaim_bytes == 0));
    }

    #[test]
    fn contended_growth_serves_the_higher_priority_first() {
        // 2 GiB free; both want 2 GiB more. Order in the slice does not matter.
        let allocations = allocate_pool(
            10 * GIB,
            &[member(80, 4 * GIB, 6 * GIB), member(100, 4 * GIB, 6 * GIB)],
        )
        .expect("allocation");
        assert_eq!(allocations[1].ceiling_bytes, 6 * GIB);
        assert_eq!(allocations[0].unmet_bytes, 2 * GIB);
    }

    #[test]
    fn unmet_higher_priority_demand_lowers_a_lower_priority_ceiling_to_its_floor() {
        // Pool full. The high VM wants 3 GiB more; the low VM holds 4 GiB with a
        // 2 GiB floor, so it can give 2 GiB and no more.
        let mut low = member(80, 4 * GIB, 4 * GIB);
        low.safe_floor_bytes = 2 * GIB;
        let allocations =
            allocate_pool(8 * GIB, &[member(100, 4 * GIB, 7 * GIB), low]).expect("allocation");
        assert_eq!(
            allocations[0].ceiling_bytes,
            4 * GIB,
            "no growth until freed"
        );
        assert_eq!(allocations[0].unmet_bytes, 3 * GIB);
        assert_eq!(allocations[1].ceiling_bytes, 2 * GIB);
        assert_eq!(allocations[1].reclaim_bytes, 2 * GIB);
    }

    #[test]
    fn equal_priorities_and_non_donors_are_never_reclaimed() {
        let mut no_shrink = member(80, 4 * GIB, 4 * GIB);
        no_shrink.can_donate = false;
        let allocations = allocate_pool(
            8 * GIB,
            &[
                member(100, 4 * GIB, 6 * GIB),
                member(100, 2 * GIB, 2 * GIB),
                no_shrink,
            ],
        )
        .expect("allocation");
        assert_eq!(ceilings(&allocations), [4 * GIB, 2 * GIB, 4 * GIB]);
        assert!(allocations.iter().all(|a| a.reclaim_bytes == 0));
    }

    #[test]
    fn memory_still_being_released_stays_charged() {
        // The low VM was asked to shrink to 2 GiB but still holds 4 GiB; its
        // charge is 4 GiB, so the high VM cannot grow into the pending release.
        let allocations = allocate_pool(
            8 * GIB,
            &[member(100, 4 * GIB, 6 * GIB), member(80, 4 * GIB, 2 * GIB)],
        )
        .expect("allocation");
        assert_eq!(allocations[0].ceiling_bytes, 4 * GIB);
    }

    #[test]
    fn an_unreadable_member_freezes_all_growth() {
        let mut unknown = member(80, 0, 2 * GIB);
        unknown.charged_bytes = None;
        let allocations =
            allocate_pool(64 * GIB, &[member(100, 4 * GIB, 6 * GIB), unknown]).expect("allocation");
        assert_eq!(allocations[0].ceiling_bytes, 4 * GIB);
        assert!(allocations.iter().all(|a| a.reclaim_bytes == 0));
    }

    #[test]
    fn grants_stay_block_aligned_and_an_overcommitted_pool_grants_nothing() {
        let allocations =
            allocate_pool(4 * GIB + 3 * MIB, &[member(100, 4 * GIB, 6 * GIB)]).expect("allocation");
        assert_eq!(allocations[0].ceiling_bytes, 4 * GIB + 2 * MIB);
        let allocations =
            allocate_pool(2 * GIB, &[member(100, 4 * GIB, 6 * GIB)]).expect("allocation");
        assert_eq!(allocations[0].ceiling_bytes, 4 * GIB);
        assert_eq!(
            allocate_pool(
                GIB,
                &[PoolMember {
                    block_bytes: 3,
                    ..member(1, 0, 0)
                }]
            ),
            Err(PoolError::InvalidBlock)
        );
    }
}
