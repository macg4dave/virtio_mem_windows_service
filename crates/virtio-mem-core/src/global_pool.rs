//! Versioned, OS-neutral host-pool contracts and side-effect-free arbitration.
//!
//! This module produces plans only. It has no durable reservation or actuation
//! authority; those boundaries belong to later host-pool milestones.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const HOST_POOL_POLICY_VERSION: u16 = 1;
pub const HOST_POOL_ARBITRATION_VERSION: u16 = 1;
pub const GUEST_DEMAND_REPORT_VERSION: u16 = 1;
pub const HOST_POOL_PLAN_VERSION: u16 = 1;
pub const MAX_POOL_MEMBERS: usize = 4096;
pub const MAX_DEMAND_REASON_CODES: usize = 32;

/// Exact host-owned identity for one configured VM/device member.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolMemberIdentity {
    pub vm_name: String,
    pub device_alias: String,
}

/// Identity of the OS-specific adapter that produced a common demand report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemandProviderIdentity {
    pub kind: String,
    pub instance_id: String,
}

/// Configuration semantics for one pool member; all bounds are total guest RAM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostPoolMemberPolicy {
    pub identity: PoolMemberIdentity,
    pub minimum_total_bytes: u64,
    pub maximum_total_bytes: u64,
    /// Larger values win only when eligible growth requests contend.
    pub priority: u32,
    pub provider_kind: String,
    pub report_max_age_millis: u64,
}

/// Versioned semantic host-pool policy, independent of a final file format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostPoolPolicy {
    pub version: u16,
    pub arbitration_version: u16,
    pub total_pool_bytes: u64,
    pub members: Vec<HostPoolMemberPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemandReportAvailability {
    Available,
    Degraded,
    Warming,
    Unavailable,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemandReportContinuity {
    Continuous,
    Warming,
    Discontinuous,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestPressureState {
    LowMemory,
    Neutral,
    HighMemory,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestShrinkEligibility {
    Eligible,
    Blocked,
}

/// Provider-neutral demand evidence. It requests capacity but cannot grant it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestDemandReport {
    pub version: u16,
    pub provider_policy_version: u16,
    pub member: PoolMemberIdentity,
    pub provider: DemandProviderIdentity,
    pub observed_unix_millis: u64,
    pub availability: DemandReportAvailability,
    pub continuity: DemandReportContinuity,
    pub demand_target_total_bytes: u64,
    pub effective_maximum_total_bytes: u64,
    pub safe_floor_total_bytes: u64,
    pub pressure: GuestPressureState,
    pub shrink_eligibility: GuestShrinkEligibility,
    pub reason_codes: Vec<String>,
}

/// Host-owned lifecycle state for one member in a coherent snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolMemberLifecycle {
    Enabled,
    Inactive,
    Unavailable,
    Removed,
}

/// Host-derived allocation state joined with an optional provider report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolMemberSnapshot {
    pub identity: PoolMemberIdentity,
    pub lifecycle: PoolMemberLifecycle,
    pub non_reclaimable_base_bytes: u64,
    pub device_size_bytes: u64,
    pub device_block_bytes: u64,
    pub requested_device_bytes: u64,
    pub current_device_bytes: u64,
    pub demand: Option<GuestDemandReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolGrantDisposition {
    Hold,
    Grow,
    Constrained,
    Inactive,
    Unavailable,
    Removed,
}

/// Total-RAM grant produced for one member by a pure plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolGrant {
    pub member: PoolMemberIdentity,
    pub current_total_bytes: u64,
    pub demand_target_total_bytes: u64,
    pub pool_grant_total_bytes: u64,
    pub desired_device_bytes: u64,
    pub unmet_demand_bytes: u64,
    pub disposition: PoolGrantDisposition,
}

/// A dependency only: the recipient is not granted bytes until release is observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReclaimForTransfer {
    pub recipient: PoolMemberIdentity,
    pub donor: PoolMemberIdentity,
    pub bytes: u64,
    pub donor_target_total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolSnapshotBlockerReason {
    AllocationInFlight,
    MemberUnavailable,
    ReportUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolSnapshotBlocker {
    pub member: PoolMemberIdentity,
    pub reason: PoolSnapshotBlockerReason,
}

/// A complete, deterministic, non-durable plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostPoolPlan {
    pub version: u16,
    pub snapshot_complete: bool,
    pub total_pool_bytes: u64,
    pub charged_before_bytes: u64,
    pub charged_after_grants_bytes: u64,
    pub free_before_bytes: u64,
    pub free_after_grants_bytes: u64,
    pub snapshot_blockers: Vec<PoolSnapshotBlocker>,
    pub grants: Vec<PoolGrant>,
    pub reclaim_for_transfer: Vec<ReclaimForTransfer>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HostPoolError {
    #[error("host-pool policy is invalid: {0}")]
    InvalidPolicy(&'static str),
    #[error("pool member identity is empty, malformed, duplicated, missing, or unexpected: {0}")]
    InvalidMemberIdentity(String),
    #[error("pool member {member:?} has invalid memory geometry: {reason}")]
    InvalidMemberGeometry {
        member: PoolMemberIdentity,
        reason: &'static str,
    },
    #[error("pool member {member:?} has an invalid demand report: {reason}")]
    InvalidDemandReport {
        member: PoolMemberIdentity,
        reason: &'static str,
    },
    #[error("pool member {member:?} demand report is stale or from the future")]
    StaleDemandReport { member: PoolMemberIdentity },
    #[error("host-pool accounting overflowed")]
    ArithmeticOverflow,
    #[error("aligned member minimum guarantees exceed the configured pool")]
    MinimumsExceedPool,
    #[error("current member charges exceed the configured pool")]
    PoolOvercommitted,
}

/// Converts alias-scoped device allocation to its total-RAM pool charge.
pub fn total_ram_from_device(base_bytes: u64, device_bytes: u64) -> Result<u64, HostPoolError> {
    base_bytes
        .checked_add(device_bytes)
        .ok_or(HostPoolError::ArithmeticOverflow)
}

/// Converts a total-RAM grant to an aligned alias-scoped device target.
pub fn device_target_from_total_ram(
    base_bytes: u64,
    total_ram_bytes: u64,
    device_size_bytes: u64,
    device_block_bytes: u64,
) -> Result<u64, HostPoolError> {
    if device_block_bytes == 0 || !device_block_bytes.is_power_of_two() {
        return Err(HostPoolError::InvalidPolicy(
            "device block size must be a non-zero power of two",
        ));
    }
    let target = total_ram_bytes
        .checked_sub(base_bytes)
        .ok_or(HostPoolError::InvalidPolicy(
            "total RAM target is below non-reclaimable base",
        ))?;
    if target > device_size_bytes || !target.is_multiple_of(device_block_bytes) {
        return Err(HostPoolError::InvalidPolicy(
            "derived device target is outside or unaligned to device geometry",
        ));
    }
    Ok(target)
}

/// Validates semantic policy invariants without selecting a file format.
pub fn validate_host_pool_policy(policy: &HostPoolPolicy) -> Result<(), HostPoolError> {
    if policy.version != HOST_POOL_POLICY_VERSION
        || policy.arbitration_version != HOST_POOL_ARBITRATION_VERSION
        || policy.total_pool_bytes == 0
        || policy.members.is_empty()
        || policy.members.len() > MAX_POOL_MEMBERS
    {
        return Err(HostPoolError::InvalidPolicy(
            "unsupported version, arbitration version, pool size, or member count",
        ));
    }
    let mut identities = BTreeSet::new();
    let mut minimums = 0_u64;
    for member in &policy.members {
        validate_identity(&member.identity)?;
        if !identities.insert(member.identity.clone()) {
            return Err(HostPoolError::InvalidMemberIdentity(format!(
                "{}/{}",
                member.identity.vm_name, member.identity.device_alias
            )));
        }
        if member.minimum_total_bytes == 0
            || member.minimum_total_bytes > member.maximum_total_bytes
            || member.priority == 0
            || member.report_max_age_millis == 0
            || !valid_token(&member.provider_kind)
        {
            return Err(HostPoolError::InvalidPolicy(
                "member bounds, priority, provider kind, or report age are invalid",
            ));
        }
        minimums = minimums
            .checked_add(member.minimum_total_bytes)
            .ok_or(HostPoolError::ArithmeticOverflow)?;
    }
    if minimums > policy.total_pool_bytes {
        return Err(HostPoolError::MinimumsExceedPool);
    }
    Ok(())
}

/// Computes one plan from an exact full-member snapshot.
pub fn arbitrate_host_pool(
    policy: &HostPoolPolicy,
    now_unix_millis: u64,
    snapshots: &[PoolMemberSnapshot],
) -> Result<HostPoolPlan, HostPoolError> {
    validate_host_pool_policy(policy)?;
    if snapshots.len() != policy.members.len() {
        return Err(HostPoolError::InvalidMemberIdentity(
            "snapshot member set does not match policy".to_owned(),
        ));
    }
    let policy_by_identity = policy
        .members
        .iter()
        .map(|member| (member.identity.clone(), member))
        .collect::<BTreeMap<_, _>>();
    let mut snapshot_by_identity = BTreeMap::new();
    for snapshot in snapshots {
        validate_identity(&snapshot.identity)?;
        if !policy_by_identity.contains_key(&snapshot.identity)
            || snapshot_by_identity
                .insert(snapshot.identity.clone(), snapshot)
                .is_some()
        {
            return Err(HostPoolError::InvalidMemberIdentity(format!(
                "{}/{}",
                snapshot.identity.vm_name, snapshot.identity.device_alias
            )));
        }
    }

    let mut members = Vec::with_capacity(policy.members.len());
    let mut charged_before_bytes = 0_u64;
    let mut snapshot_blockers = Vec::new();
    for member_policy in policy_by_identity.values() {
        let snapshot = snapshot_by_identity
            .get(&member_policy.identity)
            .ok_or_else(|| {
                HostPoolError::InvalidMemberIdentity(format!(
                    "{}/{}",
                    member_policy.identity.vm_name, member_policy.identity.device_alias
                ))
            })?;
        let state = validate_snapshot(member_policy, snapshot, now_unix_millis)?;
        charged_before_bytes = charged_before_bytes
            .checked_add(state.accounted_total_bytes)
            .ok_or(HostPoolError::ArithmeticOverflow)?;
        if let Some(reason) = state.snapshot_blocker {
            snapshot_blockers.push(PoolSnapshotBlocker {
                member: member_policy.identity.clone(),
                reason,
            });
        }
        members.push(state);
    }
    let snapshot_complete = snapshot_blockers.is_empty();
    let free_before_bytes = policy
        .total_pool_bytes
        .checked_sub(charged_before_bytes)
        .ok_or(HostPoolError::PoolOvercommitted)?;

    let mut grant_totals = members
        .iter()
        .map(|member| member.accounted_total_bytes)
        .collect::<Vec<_>>();
    let mut free_after_grants_bytes = free_before_bytes;
    if snapshot_complete {
        let aggregate_growth = members.iter().try_fold(0_u64, |total, member| {
            total
                .checked_add(
                    member
                        .demand_target_bytes
                        .saturating_sub(member.current_total_bytes),
                )
                .ok_or(HostPoolError::ArithmeticOverflow)
        })?;
        let mut growth_order = (0..members.len()).collect::<Vec<_>>();
        if aggregate_growth > free_before_bytes {
            growth_order.sort_by(|left, right| {
                members[*right]
                    .policy
                    .priority
                    .cmp(&members[*left].policy.priority)
                    .then_with(|| {
                        members[*left]
                            .policy
                            .identity
                            .cmp(&members[*right].policy.identity)
                    })
            });
        }
        for index in growth_order {
            let member = &members[index];
            let wanted = member
                .demand_target_bytes
                .saturating_sub(member.current_total_bytes);
            let granted = if aggregate_growth <= free_before_bytes {
                wanted
            } else {
                align_down(
                    wanted.min(free_after_grants_bytes),
                    member.snapshot.device_block_bytes,
                )
            };
            grant_totals[index] = member
                .current_total_bytes
                .checked_add(granted)
                .ok_or(HostPoolError::ArithmeticOverflow)?;
            free_after_grants_bytes -= granted;
        }
    }

    let mut reclaim_for_transfer = Vec::new();
    if snapshot_complete {
        let mut recipients = (0..members.len())
            .filter(|index| grant_totals[*index] < members[*index].demand_target_bytes)
            .collect::<Vec<_>>();
        recipients.sort_by(|left, right| {
            members[*right]
                .policy
                .priority
                .cmp(&members[*left].policy.priority)
                .then_with(|| {
                    members[*left]
                        .policy
                        .identity
                        .cmp(&members[*right].policy.identity)
                })
        });
        let mut donor_targets = members
            .iter()
            .map(|member| member.current_total_bytes)
            .collect::<Vec<_>>();
        for recipient_index in recipients {
            let recipient = &members[recipient_index];
            let mut unmet = recipient.demand_target_bytes - grant_totals[recipient_index];
            let mut donors = (0..members.len())
                .filter(|donor_index| {
                    let donor = &members[*donor_index];
                    donor.policy.priority < recipient.policy.priority
                        && donor.shrink_eligible
                        && donor_targets[*donor_index] > donor.reclaim_floor_bytes
                })
                .collect::<Vec<_>>();
            donors.sort_by(|left, right| {
                members[*left]
                    .policy
                    .priority
                    .cmp(&members[*right].policy.priority)
                    .then_with(|| {
                        members[*left]
                            .policy
                            .identity
                            .cmp(&members[*right].policy.identity)
                    })
            });
            for donor_index in donors {
                if unmet == 0 {
                    break;
                }
                let donor = &members[donor_index];
                let reclaimable = donor_targets[donor_index] - donor.reclaim_floor_bytes;
                let transfer_alignment = donor
                    .snapshot
                    .device_block_bytes
                    .max(recipient.snapshot.device_block_bytes);
                let bytes = align_down(reclaimable.min(unmet), transfer_alignment);
                if bytes == 0 {
                    continue;
                }
                donor_targets[donor_index] -= bytes;
                unmet -= bytes;
                reclaim_for_transfer.push(ReclaimForTransfer {
                    recipient: recipient.policy.identity.clone(),
                    donor: donor.policy.identity.clone(),
                    bytes,
                    donor_target_total_bytes: donor_targets[donor_index],
                });
            }
        }
    }

    let grants = members
        .iter()
        .enumerate()
        .map(|(index, member)| {
            let grant = grant_totals[index];
            let unmet = member.demand_target_bytes.saturating_sub(grant);
            let disposition = match member.snapshot.lifecycle {
                PoolMemberLifecycle::Inactive => PoolGrantDisposition::Inactive,
                PoolMemberLifecycle::Unavailable => PoolGrantDisposition::Unavailable,
                PoolMemberLifecycle::Removed => PoolGrantDisposition::Removed,
                PoolMemberLifecycle::Enabled if member.snapshot_blocker.is_some() => {
                    PoolGrantDisposition::Hold
                }
                PoolMemberLifecycle::Enabled if unmet > 0 => PoolGrantDisposition::Constrained,
                PoolMemberLifecycle::Enabled if grant > member.current_total_bytes => {
                    PoolGrantDisposition::Grow
                }
                PoolMemberLifecycle::Enabled => PoolGrantDisposition::Hold,
            };
            Ok(PoolGrant {
                member: member.policy.identity.clone(),
                current_total_bytes: member.current_total_bytes,
                demand_target_total_bytes: member.demand_target_bytes,
                pool_grant_total_bytes: grant,
                desired_device_bytes: device_target_from_total_ram(
                    member.snapshot.non_reclaimable_base_bytes,
                    grant,
                    member.snapshot.device_size_bytes,
                    member.snapshot.device_block_bytes,
                )?,
                unmet_demand_bytes: unmet,
                disposition,
            })
        })
        .collect::<Result<Vec<_>, HostPoolError>>()?;
    let charged_after_grants_bytes = charged_before_bytes
        .checked_add(free_before_bytes - free_after_grants_bytes)
        .ok_or(HostPoolError::ArithmeticOverflow)?;

    Ok(HostPoolPlan {
        version: HOST_POOL_PLAN_VERSION,
        snapshot_complete,
        total_pool_bytes: policy.total_pool_bytes,
        charged_before_bytes,
        charged_after_grants_bytes,
        free_before_bytes,
        free_after_grants_bytes,
        snapshot_blockers,
        grants,
        reclaim_for_transfer,
    })
}

struct ValidatedMember<'a> {
    policy: &'a HostPoolMemberPolicy,
    snapshot: &'a PoolMemberSnapshot,
    current_total_bytes: u64,
    accounted_total_bytes: u64,
    demand_target_bytes: u64,
    reclaim_floor_bytes: u64,
    snapshot_blocker: Option<PoolSnapshotBlockerReason>,
    shrink_eligible: bool,
}

fn validate_snapshot<'a>(
    policy: &'a HostPoolMemberPolicy,
    snapshot: &'a PoolMemberSnapshot,
    now: u64,
) -> Result<ValidatedMember<'a>, HostPoolError> {
    if snapshot.device_block_bytes == 0
        || !snapshot.device_block_bytes.is_power_of_two()
        || !snapshot
            .device_size_bytes
            .is_multiple_of(snapshot.device_block_bytes)
        || snapshot.current_device_bytes > snapshot.device_size_bytes
        || snapshot.requested_device_bytes > snapshot.device_size_bytes
        || !snapshot
            .current_device_bytes
            .is_multiple_of(snapshot.device_block_bytes)
        || !snapshot
            .requested_device_bytes
            .is_multiple_of(snapshot.device_block_bytes)
    {
        return geometry_error(policy, "invalid device size, block, requested, or current");
    }
    let current_total_bytes = total_ram_from_device(
        snapshot.non_reclaimable_base_bytes,
        snapshot.current_device_bytes,
    )?;
    let requested_total_bytes = total_ram_from_device(
        snapshot.non_reclaimable_base_bytes,
        snapshot.requested_device_bytes,
    )?;
    if policy.minimum_total_bytes < snapshot.non_reclaimable_base_bytes
        || policy.maximum_total_bytes < snapshot.non_reclaimable_base_bytes
        || policy.maximum_total_bytes
            > total_ram_from_device(
                snapshot.non_reclaimable_base_bytes,
                snapshot.device_size_bytes,
            )?
        || !(policy.minimum_total_bytes - snapshot.non_reclaimable_base_bytes)
            .is_multiple_of(snapshot.device_block_bytes)
        || !(policy.maximum_total_bytes - snapshot.non_reclaimable_base_bytes)
            .is_multiple_of(snapshot.device_block_bytes)
        || current_total_bytes > policy.maximum_total_bytes
        || (snapshot.lifecycle == PoolMemberLifecycle::Enabled
            && current_total_bytes < policy.minimum_total_bytes)
    {
        return geometry_error(
            policy,
            "total-RAM bounds do not fit host-derived base and device geometry",
        );
    }

    let settled = snapshot.requested_device_bytes == snapshot.current_device_bytes;
    let movement_eligible = snapshot.lifecycle == PoolMemberLifecycle::Enabled && settled;
    let accounted_total_bytes = match snapshot.lifecycle {
        PoolMemberLifecycle::Enabled => current_total_bytes.max(requested_total_bytes),
        PoolMemberLifecycle::Inactive
        | PoolMemberLifecycle::Unavailable
        | PoolMemberLifecycle::Removed => current_total_bytes
            .max(requested_total_bytes)
            .max(policy.minimum_total_bytes),
    };

    let Some(report) = snapshot.demand.as_ref() else {
        if snapshot.lifecycle == PoolMemberLifecycle::Enabled {
            return Err(HostPoolError::InvalidDemandReport {
                member: policy.identity.clone(),
                reason: "enabled member requires a demand report",
            });
        }
        return Ok(ValidatedMember {
            policy,
            snapshot,
            current_total_bytes,
            accounted_total_bytes,
            demand_target_bytes: accounted_total_bytes,
            reclaim_floor_bytes: accounted_total_bytes,
            snapshot_blocker: (snapshot.lifecycle == PoolMemberLifecycle::Unavailable)
                .then_some(PoolSnapshotBlockerReason::MemberUnavailable),
            shrink_eligible: false,
        });
    };
    validate_report(policy, snapshot, report, now, current_total_bytes)?;
    let report_usable = matches!(
        report.availability,
        DemandReportAvailability::Available | DemandReportAvailability::Degraded
    ) && report.continuity == DemandReportContinuity::Continuous;
    let demand_target_bytes = if movement_eligible && report_usable {
        report
            .demand_target_total_bytes
            .min(report.effective_maximum_total_bytes)
            .min(policy.maximum_total_bytes)
    } else {
        current_total_bytes
    };
    let reclaim_floor_bytes = report
        .safe_floor_total_bytes
        .max(policy.minimum_total_bytes)
        .max(demand_target_bytes.min(current_total_bytes));
    let shrink_eligible = movement_eligible
        && report.availability == DemandReportAvailability::Available
        && report.continuity == DemandReportContinuity::Continuous
        && report.shrink_eligibility == GuestShrinkEligibility::Eligible
        && demand_target_bytes < current_total_bytes;

    Ok(ValidatedMember {
        policy,
        snapshot,
        current_total_bytes,
        accounted_total_bytes,
        demand_target_bytes,
        reclaim_floor_bytes,
        snapshot_blocker: match snapshot.lifecycle {
            PoolMemberLifecycle::Enabled if !settled => {
                Some(PoolSnapshotBlockerReason::AllocationInFlight)
            }
            PoolMemberLifecycle::Enabled if !report_usable => {
                Some(PoolSnapshotBlockerReason::ReportUnavailable)
            }
            PoolMemberLifecycle::Unavailable => Some(PoolSnapshotBlockerReason::MemberUnavailable),
            PoolMemberLifecycle::Enabled
            | PoolMemberLifecycle::Inactive
            | PoolMemberLifecycle::Removed => None,
        },
        shrink_eligible,
    })
}

fn validate_report(
    policy: &HostPoolMemberPolicy,
    snapshot: &PoolMemberSnapshot,
    report: &GuestDemandReport,
    now: u64,
    current_total_bytes: u64,
) -> Result<(), HostPoolError> {
    if report.version != GUEST_DEMAND_REPORT_VERSION
        || report.provider_policy_version == 0
        || report.member != snapshot.identity
        || report.provider.kind != policy.provider_kind
        || !valid_token(&report.provider.instance_id)
        || report.reason_codes.is_empty()
        || report.reason_codes.len() > MAX_DEMAND_REASON_CODES
        || report
            .reason_codes
            .iter()
            .any(|reason| !valid_token(reason))
    {
        return report_error(
            policy,
            "version, identity, provider, or reason codes are invalid",
        );
    }
    if report.observed_unix_millis > now
        || now - report.observed_unix_millis > policy.report_max_age_millis
    {
        return Err(HostPoolError::StaleDemandReport {
            member: policy.identity.clone(),
        });
    }
    if report.demand_target_total_bytes < policy.minimum_total_bytes
        || report.demand_target_total_bytes > report.effective_maximum_total_bytes
        || report.effective_maximum_total_bytes > policy.maximum_total_bytes
        || report.safe_floor_total_bytes < policy.minimum_total_bytes
        || report.safe_floor_total_bytes > current_total_bytes
    {
        return report_error(
            policy,
            "total-RAM demand, maximum, or safe floor is outside bounds",
        );
    }
    for value in [
        report.demand_target_total_bytes,
        report.effective_maximum_total_bytes,
        report.safe_floor_total_bytes,
    ] {
        let device_value = value
            .checked_sub(snapshot.non_reclaimable_base_bytes)
            .ok_or_else(|| HostPoolError::InvalidDemandReport {
                member: policy.identity.clone(),
                reason: "report total is below non-reclaimable base",
            })?;
        if !device_value.is_multiple_of(snapshot.device_block_bytes) {
            return report_error(policy, "report totals do not align to device geometry");
        }
    }
    Ok(())
}

fn validate_identity(identity: &PoolMemberIdentity) -> Result<(), HostPoolError> {
    if !valid_token(&identity.vm_name) || !valid_token(&identity.device_alias) {
        return Err(HostPoolError::InvalidMemberIdentity(format!(
            "{}/{}",
            identity.vm_name, identity.device_alias
        )));
    }
    Ok(())
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

fn geometry_error<T>(
    policy: &HostPoolMemberPolicy,
    reason: &'static str,
) -> Result<T, HostPoolError> {
    Err(HostPoolError::InvalidMemberGeometry {
        member: policy.identity.clone(),
        reason,
    })
}

fn report_error<T>(
    policy: &HostPoolMemberPolicy,
    reason: &'static str,
) -> Result<T, HostPoolError> {
    Err(HostPoolError::InvalidDemandReport {
        member: policy.identity.clone(),
        reason,
    })
}

fn align_down(value: u64, alignment: u64) -> u64 {
    value - value % alignment
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const NOW: u64 = 1_000_000;

    fn identity(name: &str) -> PoolMemberIdentity {
        PoolMemberIdentity {
            vm_name: name.to_owned(),
            device_alias: format!("ua-{name}"),
        }
    }

    fn member(name: &str, priority: u32) -> HostPoolMemberPolicy {
        HostPoolMemberPolicy {
            identity: identity(name),
            minimum_total_bytes: 6 * GIB,
            maximum_total_bytes: 18 * GIB,
            priority,
            provider_kind: "synthetic".to_owned(),
            report_max_age_millis: 10_000,
        }
    }

    fn policy(total: u64, members: Vec<HostPoolMemberPolicy>) -> HostPoolPolicy {
        HostPoolPolicy {
            version: HOST_POOL_POLICY_VERSION,
            arbitration_version: HOST_POOL_ARBITRATION_VERSION,
            total_pool_bytes: total,
            members,
        }
    }

    fn report(name: &str, demand: u64, safe_floor: u64) -> GuestDemandReport {
        GuestDemandReport {
            version: GUEST_DEMAND_REPORT_VERSION,
            provider_policy_version: 1,
            member: identity(name),
            provider: DemandProviderIdentity {
                kind: "synthetic".to_owned(),
                instance_id: format!("provider-{name}"),
            },
            observed_unix_millis: NOW,
            availability: DemandReportAvailability::Available,
            continuity: DemandReportContinuity::Continuous,
            demand_target_total_bytes: demand,
            effective_maximum_total_bytes: 18 * GIB,
            safe_floor_total_bytes: safe_floor,
            pressure: GuestPressureState::Neutral,
            shrink_eligibility: GuestShrinkEligibility::Blocked,
            reason_codes: vec!["synthetic_demand".to_owned()],
        }
    }

    fn snapshot(name: &str, current_total: u64, demand: u64) -> PoolMemberSnapshot {
        PoolMemberSnapshot {
            identity: identity(name),
            lifecycle: PoolMemberLifecycle::Enabled,
            non_reclaimable_base_bytes: 2 * GIB,
            device_size_bytes: 16 * GIB,
            device_block_bytes: 2 * GIB,
            requested_device_bytes: current_total - 2 * GIB,
            current_device_bytes: current_total - 2 * GIB,
            demand: Some(report(name, demand, current_total.min(8 * GIB))),
        }
    }

    fn grant<'a>(plan: &'a HostPoolPlan, name: &str) -> &'a PoolGrant {
        plan.grants
            .iter()
            .find(|grant| grant.member.vm_name == name)
            .expect("grant exists")
    }

    #[test]
    fn serializes_versioned_provider_neutral_contracts() {
        let config = policy(24 * GIB, vec![member("vm", 1)]);
        let encoded = serde_json::to_value(&config).expect("serialize policy");
        assert_eq!(encoded["version"], HOST_POOL_POLICY_VERSION);
        assert_eq!(encoded["members"][0]["provider_kind"], "synthetic");

        let demand = report("vm", 10 * GIB, 6 * GIB);
        let encoded = serde_json::to_value(&demand).expect("serialize report");
        assert_eq!(encoded["demand_target_total_bytes"], 10 * GIB);
        assert!(encoded.get("windows_native").is_none());
        assert!(encoded.get("physical_available_bytes").is_none());
        let decoded: GuestDemandReport =
            serde_json::from_value(encoded).expect("deserialize common report");
        assert_eq!(decoded, demand);
    }

    #[test]
    fn rejects_policy_minimums_duplicates_and_invalid_priority() {
        let duplicate = member("same", 1);
        assert!(matches!(
            validate_host_pool_policy(&policy(20 * GIB, vec![duplicate.clone(), duplicate])),
            Err(HostPoolError::InvalidMemberIdentity(_))
        ));
        assert_eq!(
            validate_host_pool_policy(&policy(10 * GIB, vec![member("a", 1), member("b", 2)])),
            Err(HostPoolError::MinimumsExceedPool)
        );
        assert!(matches!(
            validate_host_pool_policy(&policy(20 * GIB, vec![member("a", 0)])),
            Err(HostPoolError::InvalidPolicy(_))
        ));
    }

    #[test]
    fn checks_total_ram_conversion_base_bounds_geometry_and_overflow() {
        assert_eq!(total_ram_from_device(2 * GIB, 8 * GIB), Ok(10 * GIB));
        assert_eq!(
            device_target_from_total_ram(2 * GIB, 10 * GIB, 16 * GIB, 2 * GIB),
            Ok(8 * GIB)
        );
        assert_eq!(
            total_ram_from_device(u64::MAX, 1),
            Err(HostPoolError::ArithmeticOverflow)
        );
        let mut bad = snapshot("a", 8 * GIB, 8 * GIB);
        bad.non_reclaimable_base_bytes = 8 * GIB;
        assert!(matches!(
            arbitrate_host_pool(&policy(20 * GIB, vec![member("a", 1)]), NOW, &[bad]),
            Err(HostPoolError::InvalidMemberGeometry { .. })
        ));
    }

    #[test]
    fn rejects_stale_report_and_mismatched_member_sets() {
        let pool = policy(24 * GIB, vec![member("a", 1)]);
        let mut stale = snapshot("a", 8 * GIB, 10 * GIB);
        stale.demand.as_mut().expect("report").observed_unix_millis = NOW - 20_000;
        assert!(matches!(
            arbitrate_host_pool(&pool, NOW, &[stale]),
            Err(HostPoolError::StaleDemandReport { .. })
        ));
        assert!(matches!(
            arbitrate_host_pool(&pool, NOW, &[snapshot("other", 8 * GIB, 8 * GIB)]),
            Err(HostPoolError::InvalidMemberIdentity(_))
        ));
    }

    #[test]
    fn grants_all_fitting_growth_without_consulting_priority() {
        let snapshots = vec![
            snapshot("a", 6 * GIB, 10 * GIB),
            snapshot("b", 6 * GIB, 10 * GIB),
        ];
        let low_first = arbitrate_host_pool(
            &policy(24 * GIB, vec![member("a", 1), member("b", 100)]),
            NOW,
            &snapshots,
        )
        .expect("unconstrained plan");
        let high_first = arbitrate_host_pool(
            &policy(24 * GIB, vec![member("a", 100), member("b", 1)]),
            NOW,
            &snapshots,
        )
        .expect("unconstrained plan");
        assert_eq!(grant(&low_first, "a").pool_grant_total_bytes, 10 * GIB);
        assert_eq!(grant(&low_first, "b").pool_grant_total_bytes, 10 * GIB);
        assert_eq!(low_first.grants, high_first.grants);
    }

    #[test]
    fn contention_is_deterministic_and_input_order_independent() {
        let pool = policy(16 * GIB, vec![member("b", 1), member("a", 1)]);
        let first = arbitrate_host_pool(
            &pool,
            NOW,
            &[
                snapshot("a", 6 * GIB, 10 * GIB),
                snapshot("b", 6 * GIB, 10 * GIB),
            ],
        )
        .expect("contended plan");
        let second = arbitrate_host_pool(
            &pool,
            NOW,
            &[
                snapshot("b", 6 * GIB, 10 * GIB),
                snapshot("a", 6 * GIB, 10 * GIB),
            ],
        )
        .expect("contended plan");
        assert_eq!(first, second);
        assert_eq!(grant(&first, "a").pool_grant_total_bytes, 10 * GIB);
        assert_eq!(grant(&first, "b").pool_grant_total_bytes, 6 * GIB);
    }

    #[test]
    fn reclaims_only_for_unmet_strictly_higher_priority_demand() {
        let pool = policy(18 * GIB, vec![member("requester", 10), member("donor", 1)]);
        let requester = snapshot("requester", 6 * GIB, 12 * GIB);
        let mut donor = snapshot("donor", 10 * GIB, 6 * GIB);
        let donor_report = donor.demand.as_mut().expect("report");
        donor_report.safe_floor_total_bytes = 6 * GIB;
        donor_report.shrink_eligibility = GuestShrinkEligibility::Eligible;
        donor_report.pressure = GuestPressureState::HighMemory;
        let plan = arbitrate_host_pool(&pool, NOW, &[donor, requester]).expect("transfer plan");
        assert_eq!(grant(&plan, "requester").unmet_demand_bytes, 4 * GIB);
        assert_eq!(plan.reclaim_for_transfer.len(), 1);
        assert_eq!(plan.reclaim_for_transfer[0].donor.vm_name, "donor");
        assert_eq!(plan.reclaim_for_transfer[0].recipient.vm_name, "requester");
        assert_eq!(plan.reclaim_for_transfer[0].bytes, 4 * GIB);
    }

    #[test]
    fn does_not_reclaim_without_contention_or_from_equal_or_higher_priority() {
        let mut donor = snapshot("donor", 10 * GIB, 6 * GIB);
        donor.demand.as_mut().expect("report").shrink_eligibility =
            GuestShrinkEligibility::Eligible;
        let unconstrained = arbitrate_host_pool(
            &policy(24 * GIB, vec![member("requester", 10), member("donor", 1)]),
            NOW,
            &[snapshot("requester", 6 * GIB, 10 * GIB), donor.clone()],
        )
        .expect("unconstrained plan");
        assert!(unconstrained.reclaim_for_transfer.is_empty());
        let equal = arbitrate_host_pool(
            &policy(16 * GIB, vec![member("requester", 10), member("donor", 10)]),
            NOW,
            &[snapshot("requester", 6 * GIB, 12 * GIB), donor.clone()],
        )
        .expect("equal priority plan");
        assert!(equal.reclaim_for_transfer.is_empty());
        let higher_donor = arbitrate_host_pool(
            &policy(16 * GIB, vec![member("requester", 10), member("donor", 11)]),
            NOW,
            &[snapshot("requester", 6 * GIB, 12 * GIB), donor],
        )
        .expect("higher donor plan");
        assert!(higher_donor.reclaim_for_transfer.is_empty());
    }

    #[test]
    fn lifecycle_states_reserve_minimums_and_unavailable_fails_closed() {
        let pool = policy(20 * GIB, vec![member("active", 10), member("other", 1)]);
        let active = snapshot("active", 6 * GIB, 10 * GIB);
        for lifecycle in [
            PoolMemberLifecycle::Inactive,
            PoolMemberLifecycle::Unavailable,
            PoolMemberLifecycle::Removed,
        ] {
            let mut other = snapshot("other", 6 * GIB, 10 * GIB);
            other.lifecycle = lifecycle;
            other.current_device_bytes = 0;
            other.requested_device_bytes = 0;
            other.demand = None;
            let plan =
                arbitrate_host_pool(&pool, NOW, &[active.clone(), other]).expect("lifecycle plan");
            assert_eq!(
                plan.snapshot_complete,
                lifecycle != PoolMemberLifecycle::Unavailable
            );
            assert_eq!(
                plan.snapshot_blockers.len(),
                usize::from(lifecycle == PoolMemberLifecycle::Unavailable)
            );
            assert_eq!(
                grant(&plan, "active").pool_grant_total_bytes,
                if lifecycle == PoolMemberLifecycle::Unavailable {
                    6 * GIB
                } else {
                    10 * GIB
                }
            );
            assert_eq!(plan.free_before_bytes, 8 * GIB);
            assert!(plan.reclaim_for_transfer.is_empty());
        }
    }

    #[test]
    fn in_flight_or_unusable_provider_evidence_blocks_the_full_snapshot() {
        let pool = policy(20 * GIB, vec![member("a", 10), member("b", 1)]);
        let mut in_flight = snapshot("a", 6 * GIB, 10 * GIB);
        in_flight.requested_device_bytes += 2 * GIB;
        let plan = arbitrate_host_pool(&pool, NOW, &[in_flight, snapshot("b", 6 * GIB, 10 * GIB)])
            .expect("blocked plan");
        assert!(!plan.snapshot_complete);
        assert_eq!(
            plan.snapshot_blockers[0].reason,
            PoolSnapshotBlockerReason::AllocationInFlight
        );
        assert_eq!(grant(&plan, "a").disposition, PoolGrantDisposition::Hold);
        assert!(plan
            .grants
            .iter()
            .all(|grant| grant.disposition != PoolGrantDisposition::Grow));

        let mut warming = snapshot("a", 6 * GIB, 10 * GIB);
        warming.demand.as_mut().expect("report").availability = DemandReportAvailability::Warming;
        let plan = arbitrate_host_pool(&pool, NOW, &[warming, snapshot("b", 6 * GIB, 10 * GIB)])
            .expect("blocked plan");
        assert_eq!(
            plan.snapshot_blockers[0].reason,
            PoolSnapshotBlockerReason::ReportUnavailable
        );
    }
}
