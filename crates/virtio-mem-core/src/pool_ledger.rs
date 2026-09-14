//! Durable, versioned host-pool accounting with pre-dispatch command ownership.
//!
//! The ledger does not dispatch resize commands. Callers must durably store a
//! successful transition before making its command visible to a reconciler.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::global_pool::{
    validate_host_pool_policy, HostPoolPlan, HostPoolPolicy, PoolMemberIdentity,
    HOST_POOL_PLAN_VERSION, MAX_POOL_MEMBERS,
};

pub const HOST_POOL_LEDGER_VERSION: u16 = 1;
pub const MAX_HOST_POOL_LEDGER_BYTES: u64 = 1024 * 1024;
const SHA256_HEX_LENGTH: usize = 64;
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolLedgerObservation {
    pub member: PoolMemberIdentity,
    pub observed_current_total_bytes: u64,
    pub owned_requested_total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolCommandDirection {
    Growth,
    Reclaim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolCommandPhase {
    Reserved,
    Dispatched,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolLedgerCommand {
    pub operation_id: String,
    pub plan_generation: u64,
    pub direction: PoolCommandDirection,
    pub phase: PoolCommandPhase,
    pub prior_requested_total_bytes: u64,
    pub prior_current_total_bytes: u64,
    pub target_total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolLedgerMember {
    pub member: PoolMemberIdentity,
    pub minimum_total_bytes: u64,
    pub maximum_total_bytes: u64,
    pub observed_current_total_bytes: u64,
    pub owned_requested_total_bytes: u64,
    pub granted_target_total_bytes: u64,
    pub reserved_growth_bytes: u64,
    pub pending_reclaim_bytes: u64,
    pub command: Option<PoolLedgerCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolLedger {
    pub version: u16,
    pub policy_fingerprint_sha256: String,
    pub plan_generation: u64,
    pub revision: u64,
    pub total_pool_bytes: u64,
    pub members: Vec<PoolLedgerMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PoolCommandRequest {
    pub member: PoolMemberIdentity,
    pub operation_id: String,
    pub direction: PoolCommandDirection,
    pub target_total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolCommandOwner {
    pub member: PoolMemberIdentity,
    pub operation_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolLedgerAccounting {
    pub allocated_bytes: u64,
    pub reserved_growth_bytes: u64,
    pub pending_reclaim_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRecovery {
    NoCommand,
    ReservedNotDispatched,
    NotApplied,
    InProgress,
    Converged,
    Ambiguous,
}

#[derive(Debug, Error)]
pub enum PoolLedgerError {
    #[error("host-pool policy is invalid: {0}")]
    InvalidPolicy(String),
    #[error("pool ledger is invalid: {0}")]
    InvalidLedger(&'static str),
    #[error("pool ledger member is missing, duplicated, or unexpected: {0:?}")]
    InvalidMember(PoolMemberIdentity),
    #[error("pool ledger revision changed: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("pool ledger plan generation must increase")]
    StalePlanGeneration,
    #[error("pool ledger operation id is empty or duplicated")]
    InvalidOperationId,
    #[error("host-pool plan does not match the complete ledger snapshot: {0}")]
    InvalidPlan(&'static str),
    #[error("pool ledger member already has command ownership: {0:?}")]
    CommandAlreadyOwned(PoolMemberIdentity),
    #[error("pool ledger command transition is invalid for member: {0:?}")]
    InvalidCommandTransition(PoolMemberIdentity),
    #[error("pool ledger accounting overflowed")]
    ArithmeticOverflow,
    #[error("pool ledger capacity is exhausted")]
    PoolExhausted,
    #[error("pool ledger is oversized")]
    Oversized,
    #[error("pool ledger checksum is invalid")]
    ChecksumMismatch,
    #[error("pool ledger persistence failed: {0}")]
    Persistence(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PoolLedgerFile {
    version: u16,
    checksum_sha256: String,
    ledger: PoolLedger,
}

impl PoolLedger {
    /// Creates cold accounting only from a complete, settled authoritative snapshot.
    pub fn cold_start(
        policy: &HostPoolPolicy,
        observations: &[PoolLedgerObservation],
    ) -> Result<Self, PoolLedgerError> {
        validate_host_pool_policy(policy)
            .map_err(|error| PoolLedgerError::InvalidPolicy(error.to_string()))?;
        let observation_map = observation_map(policy, observations)?;
        let mut members = Vec::with_capacity(policy.members.len());
        for member_policy in sorted_policy_members(policy) {
            let observation = observation_map
                .get(&member_policy.identity)
                .expect("complete map checked above");
            if observation.observed_current_total_bytes != observation.owned_requested_total_bytes
                || observation.observed_current_total_bytes < member_policy.minimum_total_bytes
                || observation.observed_current_total_bytes > member_policy.maximum_total_bytes
            {
                return Err(PoolLedgerError::InvalidLedger(
                    "cold start requires settled current/requested within member bounds",
                ));
            }
            members.push(PoolLedgerMember {
                member: member_policy.identity.clone(),
                minimum_total_bytes: member_policy.minimum_total_bytes,
                maximum_total_bytes: member_policy.maximum_total_bytes,
                observed_current_total_bytes: observation.observed_current_total_bytes,
                owned_requested_total_bytes: observation.owned_requested_total_bytes,
                granted_target_total_bytes: observation.observed_current_total_bytes,
                reserved_growth_bytes: 0,
                pending_reclaim_bytes: 0,
                command: None,
            });
        }
        let ledger = Self {
            version: HOST_POOL_LEDGER_VERSION,
            policy_fingerprint_sha256: host_pool_policy_fingerprint(policy)?,
            plan_generation: 0,
            revision: 0,
            total_pool_bytes: policy.total_pool_bytes,
            members,
        };
        ledger.validate(policy)?;
        Ok(ledger)
    }

    pub fn validate(&self, policy: &HostPoolPolicy) -> Result<(), PoolLedgerError> {
        validate_host_pool_policy(policy)
            .map_err(|error| PoolLedgerError::InvalidPolicy(error.to_string()))?;
        if self.version != HOST_POOL_LEDGER_VERSION
            || self.total_pool_bytes != policy.total_pool_bytes
            || self.policy_fingerprint_sha256 != host_pool_policy_fingerprint(policy)?
            || self.members.len() != policy.members.len()
            || self.members.len() > MAX_POOL_MEMBERS
        {
            return Err(PoolLedgerError::InvalidLedger(
                "version, policy fingerprint, pool size, or member count does not match",
            ));
        }
        let policies = policy
            .members
            .iter()
            .map(|member| (&member.identity, member))
            .collect::<BTreeMap<_, _>>();
        let mut identities = BTreeSet::new();
        let mut operation_ids = BTreeSet::new();
        for member in &self.members {
            let Some(member_policy) = policies.get(&member.member) else {
                return Err(PoolLedgerError::InvalidMember(member.member.clone()));
            };
            if !identities.insert(member.member.clone())
                || member.minimum_total_bytes != member_policy.minimum_total_bytes
                || member.maximum_total_bytes != member_policy.maximum_total_bytes
                || member.observed_current_total_bytes < member_policy.minimum_total_bytes
                || member.observed_current_total_bytes > member_policy.maximum_total_bytes
                || member.owned_requested_total_bytes < member_policy.minimum_total_bytes
                || member.owned_requested_total_bytes > member_policy.maximum_total_bytes
                || member.granted_target_total_bytes < member_policy.minimum_total_bytes
                || member.granted_target_total_bytes > member_policy.maximum_total_bytes
            {
                return Err(PoolLedgerError::InvalidMember(member.member.clone()));
            }
            validate_member_accounting(member, self.plan_generation, &mut operation_ids)?;
        }
        self.accounting()?;
        Ok(())
    }

    pub fn accounting(&self) -> Result<PoolLedgerAccounting, PoolLedgerError> {
        let mut allocated_bytes = 0_u64;
        let mut reserved_growth_bytes = 0_u64;
        let mut pending_reclaim_bytes = 0_u64;
        for member in &self.members {
            allocated_bytes = allocated_bytes
                .checked_add(member.observed_current_total_bytes)
                .ok_or(PoolLedgerError::ArithmeticOverflow)?;
            reserved_growth_bytes = reserved_growth_bytes
                .checked_add(member.reserved_growth_bytes)
                .ok_or(PoolLedgerError::ArithmeticOverflow)?;
            pending_reclaim_bytes = pending_reclaim_bytes
                .checked_add(member.pending_reclaim_bytes)
                .ok_or(PoolLedgerError::ArithmeticOverflow)?;
        }
        let charged = allocated_bytes
            .checked_add(reserved_growth_bytes)
            .ok_or(PoolLedgerError::ArithmeticOverflow)?;
        let free_bytes = self
            .total_pool_bytes
            .checked_sub(charged)
            .ok_or(PoolLedgerError::PoolExhausted)?;
        Ok(PoolLedgerAccounting {
            allocated_bytes,
            reserved_growth_bytes,
            pending_reclaim_bytes,
            free_bytes,
        })
    }

    /// Atomically stages every actionable command from a complete pure pool plan.
    /// The caller must store the returned ledger before dispatching any command.
    pub fn stage_plan(
        &mut self,
        expected_revision: u64,
        plan_generation: u64,
        plan: &HostPoolPlan,
        owners: &[PoolCommandOwner],
    ) -> Result<(), PoolLedgerError> {
        self.check_revision(expected_revision)?;
        let accounting = self.accounting()?;
        if plan.version != HOST_POOL_PLAN_VERSION
            || !plan.snapshot_complete
            || !plan.snapshot_blockers.is_empty()
            || plan.total_pool_bytes != self.total_pool_bytes
            || plan.charged_before_bytes != accounting.allocated_bytes
            || plan.free_before_bytes != accounting.free_bytes
            || plan.grants.len() != self.members.len()
            || self.members.iter().any(|member| member.command.is_some())
        {
            return Err(PoolLedgerError::InvalidPlan(
                "version, totals, member set, or ownership is not coherent",
            ));
        }
        let ledger_members = self
            .members
            .iter()
            .map(|member| (&member.member, member))
            .collect::<BTreeMap<_, _>>();
        let mut commands = BTreeMap::new();
        let mut grant_members = BTreeSet::new();
        let mut granted_growth_bytes = 0_u64;
        for grant in &plan.grants {
            let Some(member) = ledger_members.get(&grant.member) else {
                return Err(PoolLedgerError::InvalidMember(grant.member.clone()));
            };
            if !grant_members.insert(grant.member.clone())
                || grant.current_total_bytes != member.observed_current_total_bytes
            {
                return Err(PoolLedgerError::InvalidPlan(
                    "grant member set or current does not match the ledger",
                ));
            }
            if grant.pool_grant_total_bytes > grant.current_total_bytes {
                granted_growth_bytes = granted_growth_bytes
                    .checked_add(grant.pool_grant_total_bytes - grant.current_total_bytes)
                    .ok_or(PoolLedgerError::ArithmeticOverflow)?;
                commands.insert(
                    grant.member.clone(),
                    (PoolCommandDirection::Growth, grant.pool_grant_total_bytes),
                );
            }
        }
        let expected_charged_after = accounting
            .allocated_bytes
            .checked_add(granted_growth_bytes)
            .ok_or(PoolLedgerError::ArithmeticOverflow)?;
        if grant_members.len() != ledger_members.len()
            || plan.charged_after_grants_bytes != expected_charged_after
            || plan.free_after_grants_bytes
                != self
                    .total_pool_bytes
                    .checked_sub(expected_charged_after)
                    .ok_or(PoolLedgerError::PoolExhausted)?
        {
            return Err(PoolLedgerError::InvalidPlan(
                "grant totals do not match ledger accounting",
            ));
        }
        for reclaim in &plan.reclaim_for_transfer {
            if !ledger_members.contains_key(&reclaim.donor)
                || !ledger_members.contains_key(&reclaim.recipient)
            {
                return Err(PoolLedgerError::InvalidMember(reclaim.donor.clone()));
            }
            match commands.get_mut(&reclaim.donor) {
                Some((PoolCommandDirection::Reclaim, target)) => {
                    *target = (*target).min(reclaim.donor_target_total_bytes);
                }
                Some((PoolCommandDirection::Growth, _)) => {
                    return Err(PoolLedgerError::InvalidPlan(
                        "one member cannot grow and reclaim in the same plan",
                    ));
                }
                None => {
                    commands.insert(
                        reclaim.donor.clone(),
                        (
                            PoolCommandDirection::Reclaim,
                            reclaim.donor_target_total_bytes,
                        ),
                    );
                }
            }
        }
        if owners.len() != commands.len() {
            return Err(PoolLedgerError::InvalidPlan(
                "every actionable member needs exactly one command owner",
            ));
        }
        let mut owner_map = BTreeMap::new();
        for owner in owners {
            if owner_map
                .insert(owner.member.clone(), owner.operation_id.clone())
                .is_some()
            {
                return Err(PoolLedgerError::InvalidOperationId);
            }
        }
        let requests = commands
            .into_iter()
            .map(|(member, (direction, target_total_bytes))| {
                let operation_id =
                    owner_map
                        .remove(&member)
                        .ok_or(PoolLedgerError::InvalidPlan(
                            "command owner does not match an actionable plan member",
                        ))?;
                Ok(PoolCommandRequest {
                    member,
                    operation_id,
                    direction,
                    target_total_bytes,
                })
            })
            .collect::<Result<Vec<_>, PoolLedgerError>>()?;
        if !owner_map.is_empty() {
            return Err(PoolLedgerError::InvalidPlan(
                "command owner does not match an actionable plan member",
            ));
        }
        self.begin_commands(expected_revision, plan_generation, &requests)
    }

    fn begin_commands(
        &mut self,
        expected_revision: u64,
        plan_generation: u64,
        requests: &[PoolCommandRequest],
    ) -> Result<(), PoolLedgerError> {
        self.check_revision(expected_revision)?;
        if plan_generation <= self.plan_generation {
            return Err(PoolLedgerError::StalePlanGeneration);
        }
        if requests.is_empty() {
            return Err(PoolLedgerError::InvalidLedger(
                "a plan must own at least one command",
            ));
        }
        let mut next = self.clone();
        let mut requested_members = BTreeSet::new();
        let mut operation_ids = next
            .members
            .iter()
            .filter_map(|member| member.command.as_ref())
            .map(|command| command.operation_id.clone())
            .collect::<BTreeSet<_>>();
        for request in requests {
            if !valid_operation_id(&request.operation_id)
                || !operation_ids.insert(request.operation_id.clone())
            {
                return Err(PoolLedgerError::InvalidOperationId);
            }
            if !requested_members.insert(request.member.clone()) {
                return Err(PoolLedgerError::InvalidMember(request.member.clone()));
            }
            let member = next.member_mut(&request.member)?;
            if member.command.is_some() {
                return Err(PoolLedgerError::CommandAlreadyOwned(request.member.clone()));
            }
            if member.owned_requested_total_bytes != member.observed_current_total_bytes {
                return Err(PoolLedgerError::InvalidCommandTransition(
                    request.member.clone(),
                ));
            }
            let current = member.observed_current_total_bytes;
            if request.target_total_bytes < member.minimum_total_bytes
                || request.target_total_bytes > member.maximum_total_bytes
            {
                return Err(PoolLedgerError::InvalidCommandTransition(
                    request.member.clone(),
                ));
            }
            match request.direction {
                PoolCommandDirection::Growth if request.target_total_bytes > current => {
                    member.reserved_growth_bytes = request.target_total_bytes - current;
                    member.pending_reclaim_bytes = 0;
                }
                PoolCommandDirection::Reclaim if request.target_total_bytes < current => {
                    member.reserved_growth_bytes = 0;
                    member.pending_reclaim_bytes = current - request.target_total_bytes;
                }
                _ => {
                    return Err(PoolLedgerError::InvalidCommandTransition(
                        request.member.clone(),
                    ));
                }
            }
            member.granted_target_total_bytes = request.target_total_bytes;
            member.command = Some(PoolLedgerCommand {
                operation_id: request.operation_id.clone(),
                plan_generation,
                direction: request.direction,
                phase: PoolCommandPhase::Reserved,
                prior_requested_total_bytes: current,
                prior_current_total_bytes: current,
                target_total_bytes: request.target_total_bytes,
            });
        }
        next.plan_generation = plan_generation;
        next.bump_revision()?;
        next.accounting()?;
        *self = next;
        Ok(())
    }

    pub fn mark_dispatched(
        &mut self,
        expected_revision: u64,
        member_identity: &PoolMemberIdentity,
        operation_id: &str,
    ) -> Result<(), PoolLedgerError> {
        self.check_revision(expected_revision)?;
        let member = self.member_mut(member_identity)?;
        let command = matching_command_mut(member, operation_id)?;
        if command.phase != PoolCommandPhase::Reserved {
            return Err(PoolLedgerError::InvalidCommandTransition(
                member_identity.clone(),
            ));
        }
        command.phase = PoolCommandPhase::Dispatched;
        member.owned_requested_total_bytes = command.target_total_bytes;
        self.bump_revision()
    }

    pub fn mark_ambiguous(
        &mut self,
        expected_revision: u64,
        member_identity: &PoolMemberIdentity,
        operation_id: &str,
    ) -> Result<(), PoolLedgerError> {
        self.check_revision(expected_revision)?;
        let member = self.member_mut(member_identity)?;
        let command = matching_command_mut(member, operation_id)?;
        command.phase = PoolCommandPhase::Ambiguous;
        self.bump_revision()
    }

    /// Joins fresh authoritative state without ever replaying an owned command.
    pub fn observe_member(
        &mut self,
        expected_revision: u64,
        observation: &PoolLedgerObservation,
    ) -> Result<CommandRecovery, PoolLedgerError> {
        self.check_revision(expected_revision)?;
        let member = self.member_mut(&observation.member)?;
        let Some(command) = member.command.clone() else {
            if observation.observed_current_total_bytes != observation.owned_requested_total_bytes
                || observation.observed_current_total_bytes != member.observed_current_total_bytes
            {
                return Err(PoolLedgerError::InvalidCommandTransition(
                    observation.member.clone(),
                ));
            }
            return Ok(CommandRecovery::NoCommand);
        };
        let recovery = classify_observation(&command, observation);
        if recovery == CommandRecovery::Ambiguous {
            member.command.as_mut().expect("command exists").phase = PoolCommandPhase::Ambiguous;
            self.bump_revision()?;
            return Ok(recovery);
        }
        if recovery == CommandRecovery::ReservedNotDispatched
            || recovery == CommandRecovery::NotApplied
        {
            return Ok(recovery);
        }
        member.observed_current_total_bytes = observation.observed_current_total_bytes;
        member.owned_requested_total_bytes = observation.owned_requested_total_bytes;
        match command.direction {
            PoolCommandDirection::Growth => {
                member.reserved_growth_bytes = command
                    .target_total_bytes
                    .checked_sub(observation.observed_current_total_bytes)
                    .ok_or_else(|| {
                        PoolLedgerError::InvalidCommandTransition(observation.member.clone())
                    })?;
            }
            PoolCommandDirection::Reclaim => {
                member.pending_reclaim_bytes = observation
                    .observed_current_total_bytes
                    .checked_sub(command.target_total_bytes)
                    .ok_or_else(|| {
                        PoolLedgerError::InvalidCommandTransition(observation.member.clone())
                    })?;
            }
        }
        if recovery == CommandRecovery::Converged {
            member.reserved_growth_bytes = 0;
            member.pending_reclaim_bytes = 0;
            member.granted_target_total_bytes = command.target_total_bytes;
            member.command = None;
        }
        self.bump_revision()?;
        Ok(recovery)
    }

    /// Releases ownership only after a caller has resolved that no command applied.
    pub fn resolve_not_applied(
        &mut self,
        expected_revision: u64,
        member_identity: &PoolMemberIdentity,
        operation_id: &str,
    ) -> Result<(), PoolLedgerError> {
        self.check_revision(expected_revision)?;
        let member = self.member_mut(member_identity)?;
        let command = matching_command_mut(member, operation_id)?.clone();
        if member.observed_current_total_bytes != command.prior_current_total_bytes {
            return Err(PoolLedgerError::InvalidCommandTransition(
                member_identity.clone(),
            ));
        }
        member.owned_requested_total_bytes = command.prior_requested_total_bytes;
        member.granted_target_total_bytes = command.prior_current_total_bytes;
        member.reserved_growth_bytes = 0;
        member.pending_reclaim_bytes = 0;
        member.command = None;
        self.bump_revision()
    }

    fn check_revision(&self, expected: u64) -> Result<(), PoolLedgerError> {
        if self.revision != expected {
            return Err(PoolLedgerError::RevisionConflict {
                expected,
                actual: self.revision,
            });
        }
        Ok(())
    }

    fn bump_revision(&mut self) -> Result<(), PoolLedgerError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(PoolLedgerError::ArithmeticOverflow)?;
        Ok(())
    }

    fn member_mut(
        &mut self,
        identity: &PoolMemberIdentity,
    ) -> Result<&mut PoolLedgerMember, PoolLedgerError> {
        self.members
            .iter_mut()
            .find(|member| &member.member == identity)
            .ok_or_else(|| PoolLedgerError::InvalidMember(identity.clone()))
    }
}

pub struct FilePoolLedgerStore {
    path: PathBuf,
}

impl FilePoolLedgerStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self, policy: &HostPoolPolicy) -> Result<Option<PoolLedger>, PoolLedgerError> {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(persistence("inspect", error)),
        };
        if metadata.len() > MAX_HOST_POOL_LEDGER_BYTES {
            return Err(PoolLedgerError::Oversized);
        }
        let bytes = fs::read(&self.path).map_err(|error| persistence("read", error))?;
        let ledger = decode_ledger(&bytes)?;
        ledger.validate(policy)?;
        Ok(Some(ledger))
    }

    pub fn store(
        &self,
        policy: &HostPoolPolicy,
        ledger: &PoolLedger,
    ) -> Result<(), PoolLedgerError> {
        ledger.validate(policy)?;
        let bytes = encode_ledger(ledger)?;
        if bytes.len() as u64 > MAX_HOST_POOL_LEDGER_BYTES {
            return Err(PoolLedgerError::Oversized);
        }
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| persistence("create directory", error))?;
        }
        let temporary = temporary_path(&self.path);
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|error| persistence("create temporary", error))?;
            file.write_all(&bytes)
                .map_err(|error| persistence("write temporary", error))?;
            file.sync_all()
                .map_err(|error| persistence("flush temporary", error))?;
            drop(file);
            fs::rename(&temporary, &self.path).map_err(|error| persistence("publish", error))?;
            if let Some(parent) = self
                .path
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
            {
                fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|error| persistence("flush directory", error))?;
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }
}

pub fn host_pool_policy_fingerprint(policy: &HostPoolPolicy) -> Result<String, PoolLedgerError> {
    validate_host_pool_policy(policy)
        .map_err(|error| PoolLedgerError::InvalidPolicy(error.to_string()))?;
    let mut canonical = policy.clone();
    canonical
        .members
        .sort_by(|left, right| left.identity.cmp(&right.identity));
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| PoolLedgerError::Persistence(format!("encode fingerprint: {error}")))?;
    Ok(sha256_hex(&bytes))
}

fn observation_map<'a>(
    policy: &HostPoolPolicy,
    observations: &'a [PoolLedgerObservation],
) -> Result<BTreeMap<PoolMemberIdentity, &'a PoolLedgerObservation>, PoolLedgerError> {
    if observations.len() != policy.members.len() {
        return Err(PoolLedgerError::InvalidLedger(
            "observation member set is incomplete",
        ));
    }
    let expected = policy
        .members
        .iter()
        .map(|member| member.identity.clone())
        .collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    for observation in observations {
        if !expected.contains(&observation.member)
            || result
                .insert(observation.member.clone(), observation)
                .is_some()
        {
            return Err(PoolLedgerError::InvalidMember(observation.member.clone()));
        }
    }
    Ok(result)
}

fn sorted_policy_members(policy: &HostPoolPolicy) -> Vec<&crate::HostPoolMemberPolicy> {
    let mut members = policy.members.iter().collect::<Vec<_>>();
    members.sort_by(|left, right| left.identity.cmp(&right.identity));
    members
}

fn validate_member_accounting(
    member: &PoolLedgerMember,
    plan_generation: u64,
    operation_ids: &mut BTreeSet<String>,
) -> Result<(), PoolLedgerError> {
    let Some(command) = &member.command else {
        if member.reserved_growth_bytes != 0
            || member.pending_reclaim_bytes != 0
            || member.granted_target_total_bytes != member.observed_current_total_bytes
            || member.owned_requested_total_bytes != member.observed_current_total_bytes
        {
            return Err(PoolLedgerError::InvalidLedger(
                "unowned member has intent or unsettled accounting",
            ));
        }
        return Ok(());
    };
    if !valid_operation_id(&command.operation_id)
        || !operation_ids.insert(command.operation_id.clone())
        || command.plan_generation == 0
        || command.plan_generation > plan_generation
        || command.target_total_bytes != member.granted_target_total_bytes
    {
        return Err(PoolLedgerError::InvalidOperationId);
    }
    match command.phase {
        PoolCommandPhase::Reserved
            if member.owned_requested_total_bytes != command.prior_requested_total_bytes =>
        {
            return Err(PoolLedgerError::InvalidLedger(
                "reserved command changed owned requested target",
            ));
        }
        PoolCommandPhase::Dispatched
            if member.owned_requested_total_bytes != command.target_total_bytes =>
        {
            return Err(PoolLedgerError::InvalidLedger(
                "dispatched command does not own its requested target",
            ));
        }
        _ => {}
    }
    match command.direction {
        PoolCommandDirection::Growth => {
            let expected = command
                .target_total_bytes
                .checked_sub(member.observed_current_total_bytes)
                .ok_or_else(|| PoolLedgerError::InvalidMember(member.member.clone()))?;
            if member.reserved_growth_bytes != expected || member.pending_reclaim_bytes != 0 {
                return Err(PoolLedgerError::InvalidLedger(
                    "growth reservation does not match authoritative current",
                ));
            }
        }
        PoolCommandDirection::Reclaim => {
            let expected = member
                .observed_current_total_bytes
                .checked_sub(command.target_total_bytes)
                .ok_or_else(|| PoolLedgerError::InvalidMember(member.member.clone()))?;
            if member.pending_reclaim_bytes != expected || member.reserved_growth_bytes != 0 {
                return Err(PoolLedgerError::InvalidLedger(
                    "pending reclaim does not match authoritative current",
                ));
            }
        }
    }
    Ok(())
}

fn valid_operation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

fn matching_command_mut<'a>(
    member: &'a mut PoolLedgerMember,
    operation_id: &str,
) -> Result<&'a mut PoolLedgerCommand, PoolLedgerError> {
    member
        .command
        .as_mut()
        .filter(|command| command.operation_id == operation_id)
        .ok_or_else(|| PoolLedgerError::InvalidCommandTransition(member.member.clone()))
}

fn classify_observation(
    command: &PoolLedgerCommand,
    observation: &PoolLedgerObservation,
) -> CommandRecovery {
    if command.phase == PoolCommandPhase::Reserved
        && observation.owned_requested_total_bytes == command.prior_requested_total_bytes
        && observation.observed_current_total_bytes == command.prior_current_total_bytes
    {
        return CommandRecovery::ReservedNotDispatched;
    }
    if observation.owned_requested_total_bytes == command.prior_requested_total_bytes
        && observation.observed_current_total_bytes == command.prior_current_total_bytes
    {
        return CommandRecovery::NotApplied;
    }
    if observation.owned_requested_total_bytes != command.target_total_bytes {
        return CommandRecovery::Ambiguous;
    }
    let progress_valid = match command.direction {
        PoolCommandDirection::Growth => {
            observation.observed_current_total_bytes >= command.prior_current_total_bytes
                && observation.observed_current_total_bytes <= command.target_total_bytes
        }
        PoolCommandDirection::Reclaim => {
            observation.observed_current_total_bytes <= command.prior_current_total_bytes
                && observation.observed_current_total_bytes >= command.target_total_bytes
        }
    };
    if !progress_valid {
        CommandRecovery::Ambiguous
    } else if observation.observed_current_total_bytes == command.target_total_bytes {
        CommandRecovery::Converged
    } else {
        CommandRecovery::InProgress
    }
}

fn encode_ledger(ledger: &PoolLedger) -> Result<Vec<u8>, PoolLedgerError> {
    let payload = serde_json::to_vec(ledger)
        .map_err(|error| PoolLedgerError::Persistence(format!("encode payload: {error}")))?;
    serde_json::to_vec(&PoolLedgerFile {
        version: HOST_POOL_LEDGER_VERSION,
        checksum_sha256: sha256_hex(&payload),
        ledger: ledger.clone(),
    })
    .map_err(|error| PoolLedgerError::Persistence(format!("encode document: {error}")))
}

fn decode_ledger(bytes: &[u8]) -> Result<PoolLedger, PoolLedgerError> {
    if bytes.len() as u64 > MAX_HOST_POOL_LEDGER_BYTES {
        return Err(PoolLedgerError::Oversized);
    }
    let document: PoolLedgerFile = serde_json::from_slice(bytes)
        .map_err(|error| PoolLedgerError::Persistence(format!("decode document: {error}")))?;
    if document.version != HOST_POOL_LEDGER_VERSION
        || document.checksum_sha256.len() != SHA256_HEX_LENGTH
    {
        return Err(PoolLedgerError::InvalidLedger(
            "unsupported document version or checksum encoding",
        ));
    }
    let payload = serde_json::to_vec(&document.ledger)
        .map_err(|error| PoolLedgerError::Persistence(format!("encode payload: {error}")))?;
    if sha256_hex(&payload) != document.checksum_sha256 {
        return Err(PoolLedgerError::ChecksumMismatch);
    }
    Ok(document.ledger)
}

fn temporary_path(path: &Path) -> PathBuf {
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(format!(
        "{}.tmp-{}-{sequence}",
        path.display(),
        std::process::id()
    ))
}

fn persistence(action: &str, error: std::io::Error) -> PoolLedgerError {
    PoolLedgerError::Persistence(format!("{action} pool ledger: {error}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HostPoolMemberPolicy, PoolGrant, PoolGrantDisposition, HOST_POOL_ARBITRATION_VERSION,
        HOST_POOL_POLICY_VERSION,
    };

    const GIB: u64 = 1024 * 1024 * 1024;

    fn identity(name: &str) -> PoolMemberIdentity {
        PoolMemberIdentity {
            vm_name: name.to_owned(),
            device_alias: format!("ua-{name}"),
        }
    }

    fn policy() -> HostPoolPolicy {
        HostPoolPolicy {
            version: HOST_POOL_POLICY_VERSION,
            arbitration_version: HOST_POOL_ARBITRATION_VERSION,
            total_pool_bytes: 24 * GIB,
            members: vec![
                HostPoolMemberPolicy {
                    identity: identity("a"),
                    minimum_total_bytes: 4 * GIB,
                    maximum_total_bytes: 16 * GIB,
                    priority: 2,
                    provider_kind: "synthetic".to_owned(),
                    report_max_age_millis: 1_000,
                },
                HostPoolMemberPolicy {
                    identity: identity("b"),
                    minimum_total_bytes: 4 * GIB,
                    maximum_total_bytes: 16 * GIB,
                    priority: 1,
                    provider_kind: "synthetic".to_owned(),
                    report_max_age_millis: 1_000,
                },
            ],
        }
    }

    fn observations(a: u64, b: u64) -> Vec<PoolLedgerObservation> {
        vec![observation("a", a, a), observation("b", b, b)]
    }

    fn observation(name: &str, current: u64, requested: u64) -> PoolLedgerObservation {
        PoolLedgerObservation {
            member: identity(name),
            observed_current_total_bytes: current,
            owned_requested_total_bytes: requested,
        }
    }

    fn request(
        name: &str,
        operation: &str,
        direction: PoolCommandDirection,
        target: u64,
    ) -> PoolCommandRequest {
        PoolCommandRequest {
            member: identity(name),
            operation_id: operation.to_owned(),
            direction,
            target_total_bytes: target,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "virtio-mem-pool-ledger-{name}-{}-{}",
            std::process::id(),
            TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn cold_start_requires_complete_settled_snapshot_and_canonical_fingerprint() {
        let original = policy();
        let mut reordered = original.clone();
        reordered.members.reverse();
        assert_eq!(
            host_pool_policy_fingerprint(&original).expect("fingerprint"),
            host_pool_policy_fingerprint(&reordered).expect("fingerprint")
        );
        let ledger = PoolLedger::cold_start(&original, &observations(8 * GIB, 8 * GIB))
            .expect("cold ledger");
        assert_eq!(ledger.accounting().expect("accounting").free_bytes, 8 * GIB);
        assert!(PoolLedger::cold_start(
            &original,
            &[
                observation("a", 8 * GIB, 10 * GIB),
                observation("b", 8 * GIB, 8 * GIB)
            ]
        )
        .is_err());
        assert!(PoolLedger::cold_start(&original, &[observation("a", 8 * GIB, 8 * GIB)]).is_err());
    }

    #[test]
    fn reserves_all_growth_before_dispatch_and_rejects_double_spend() {
        let mut ledger =
            PoolLedger::cold_start(&policy(), &observations(8 * GIB, 8 * GIB)).unwrap();
        ledger
            .begin_commands(
                0,
                1,
                &[
                    request("a", "grow-a", PoolCommandDirection::Growth, 12 * GIB),
                    request("b", "grow-b", PoolCommandDirection::Growth, 12 * GIB),
                ],
            )
            .expect("reserve fitting growth");
        let accounting = ledger.accounting().unwrap();
        assert_eq!(accounting.allocated_bytes, 16 * GIB);
        assert_eq!(accounting.reserved_growth_bytes, 8 * GIB);
        assert_eq!(accounting.free_bytes, 0);
        let unchanged = ledger.clone();
        assert!(matches!(
            ledger.begin_commands(
                1,
                2,
                &[request(
                    "a",
                    "again",
                    PoolCommandDirection::Growth,
                    14 * GIB
                )]
            ),
            Err(PoolLedgerError::CommandAlreadyOwned(_))
        ));
        assert_eq!(ledger, unchanged);
    }

    #[test]
    fn stages_exact_actionable_members_from_the_pure_plan() {
        let mut ledger =
            PoolLedger::cold_start(&policy(), &observations(8 * GIB, 8 * GIB)).unwrap();
        let plan = HostPoolPlan {
            version: HOST_POOL_PLAN_VERSION,
            snapshot_complete: true,
            total_pool_bytes: 24 * GIB,
            charged_before_bytes: 16 * GIB,
            charged_after_grants_bytes: 20 * GIB,
            free_before_bytes: 8 * GIB,
            free_after_grants_bytes: 4 * GIB,
            snapshot_blockers: Vec::new(),
            grants: vec![
                PoolGrant {
                    member: identity("a"),
                    current_total_bytes: 8 * GIB,
                    demand_target_total_bytes: 12 * GIB,
                    pool_grant_total_bytes: 12 * GIB,
                    desired_device_bytes: 10 * GIB,
                    unmet_demand_bytes: 0,
                    disposition: PoolGrantDisposition::Grow,
                },
                PoolGrant {
                    member: identity("b"),
                    current_total_bytes: 8 * GIB,
                    demand_target_total_bytes: 8 * GIB,
                    pool_grant_total_bytes: 8 * GIB,
                    desired_device_bytes: 6 * GIB,
                    unmet_demand_bytes: 0,
                    disposition: PoolGrantDisposition::Hold,
                },
            ],
            reclaim_for_transfer: Vec::new(),
        };
        ledger
            .stage_plan(
                0,
                1,
                &plan,
                &[PoolCommandOwner {
                    member: identity("a"),
                    operation_id: "grow-a".to_owned(),
                }],
            )
            .expect("stage plan");
        assert_eq!(ledger.accounting().unwrap().reserved_growth_bytes, 4 * GIB);
        assert!(matches!(
            PoolLedger::cold_start(&policy(), &observations(8 * GIB, 8 * GIB))
                .unwrap()
                .stage_plan(0, 1, &plan, &[]),
            Err(PoolLedgerError::InvalidPlan(_))
        ));
    }

    #[test]
    fn an_over_capacity_plan_is_all_or_nothing() {
        let mut ledger =
            PoolLedger::cold_start(&policy(), &observations(8 * GIB, 8 * GIB)).unwrap();
        let before = ledger.clone();
        assert!(matches!(
            ledger.begin_commands(
                0,
                1,
                &[
                    request("a", "grow-a", PoolCommandDirection::Growth, 14 * GIB),
                    request("b", "grow-b", PoolCommandDirection::Growth, 12 * GIB),
                ]
            ),
            Err(PoolLedgerError::PoolExhausted)
        ));
        assert_eq!(ledger, before);
    }

    #[test]
    fn restart_never_replays_reserved_dispatched_or_ambiguous_growth() {
        let policy = policy();
        let path = temp_path("restart-growth");
        let store = FilePoolLedgerStore::new(&path);
        let mut ledger = PoolLedger::cold_start(&policy, &observations(8 * GIB, 8 * GIB)).unwrap();
        ledger
            .begin_commands(
                0,
                1,
                &[request("a", "grow", PoolCommandDirection::Growth, 12 * GIB)],
            )
            .unwrap();
        store.store(&policy, &ledger).unwrap();
        let mut restarted = store.load(&policy).unwrap().expect("ledger");
        assert_eq!(
            restarted
                .observe_member(1, &observation("a", 8 * GIB, 8 * GIB))
                .unwrap(),
            CommandRecovery::ReservedNotDispatched
        );
        restarted
            .mark_dispatched(1, &identity("a"), "grow")
            .unwrap();
        store.store(&policy, &restarted).unwrap();
        let mut restarted = store.load(&policy).unwrap().expect("ledger");
        assert_eq!(
            restarted
                .observe_member(2, &observation("a", 8 * GIB, 8 * GIB))
                .unwrap(),
            CommandRecovery::NotApplied
        );
        assert_eq!(
            restarted.accounting().unwrap().reserved_growth_bytes,
            4 * GIB
        );
        restarted.mark_ambiguous(2, &identity("a"), "grow").unwrap();
        store.store(&policy, &restarted).unwrap();
        let restarted = store.load(&policy).unwrap().expect("ledger");
        assert_eq!(
            restarted.members[0].command.as_ref().unwrap().phase,
            PoolCommandPhase::Ambiguous
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn partial_growth_replaces_reserved_bytes_with_observed_allocation() {
        let mut ledger =
            PoolLedger::cold_start(&policy(), &observations(8 * GIB, 8 * GIB)).unwrap();
        ledger
            .begin_commands(
                0,
                1,
                &[request("a", "grow", PoolCommandDirection::Growth, 12 * GIB)],
            )
            .unwrap();
        ledger.mark_dispatched(1, &identity("a"), "grow").unwrap();
        assert_eq!(
            ledger
                .observe_member(2, &observation("a", 10 * GIB, 12 * GIB))
                .unwrap(),
            CommandRecovery::InProgress
        );
        let accounting = ledger.accounting().unwrap();
        assert_eq!(accounting.allocated_bytes, 18 * GIB);
        assert_eq!(accounting.reserved_growth_bytes, 2 * GIB);
        assert_eq!(accounting.free_bytes, 4 * GIB);
        assert_eq!(
            ledger
                .observe_member(3, &observation("a", 12 * GIB, 12 * GIB))
                .unwrap(),
            CommandRecovery::Converged
        );
        assert!(ledger.members[0].command.is_none());
    }

    #[test]
    fn reclaim_is_not_free_until_current_falls() {
        let mut ledger =
            PoolLedger::cold_start(&policy(), &observations(12 * GIB, 8 * GIB)).unwrap();
        ledger
            .begin_commands(
                0,
                1,
                &[request(
                    "a",
                    "shrink",
                    PoolCommandDirection::Reclaim,
                    8 * GIB,
                )],
            )
            .unwrap();
        assert_eq!(ledger.accounting().unwrap().free_bytes, 4 * GIB);
        assert_eq!(ledger.accounting().unwrap().pending_reclaim_bytes, 4 * GIB);
        ledger.mark_dispatched(1, &identity("a"), "shrink").unwrap();
        ledger
            .observe_member(2, &observation("a", 10 * GIB, 8 * GIB))
            .unwrap();
        assert_eq!(ledger.accounting().unwrap().free_bytes, 6 * GIB);
        assert_eq!(ledger.accounting().unwrap().pending_reclaim_bytes, 2 * GIB);
    }

    #[test]
    fn explicit_not_applied_resolution_is_required_to_release_reservation() {
        let mut ledger =
            PoolLedger::cold_start(&policy(), &observations(8 * GIB, 8 * GIB)).unwrap();
        ledger
            .begin_commands(
                0,
                1,
                &[request("a", "grow", PoolCommandDirection::Growth, 12 * GIB)],
            )
            .unwrap();
        ledger.mark_dispatched(1, &identity("a"), "grow").unwrap();
        assert_eq!(
            ledger
                .observe_member(2, &observation("a", 8 * GIB, 8 * GIB))
                .unwrap(),
            CommandRecovery::NotApplied
        );
        assert_eq!(ledger.accounting().unwrap().reserved_growth_bytes, 4 * GIB);
        ledger
            .resolve_not_applied(2, &identity("a"), "grow")
            .unwrap();
        assert_eq!(ledger.accounting().unwrap().reserved_growth_bytes, 0);
    }

    #[test]
    fn stale_revision_and_ambiguous_live_state_fail_closed() {
        let mut ledger =
            PoolLedger::cold_start(&policy(), &observations(8 * GIB, 8 * GIB)).unwrap();
        ledger
            .begin_commands(
                0,
                1,
                &[request("a", "grow", PoolCommandDirection::Growth, 12 * GIB)],
            )
            .unwrap();
        assert!(matches!(
            ledger.mark_dispatched(0, &identity("a"), "grow"),
            Err(PoolLedgerError::RevisionConflict { .. })
        ));
        assert_eq!(ledger.revision, 1);
        assert_eq!(
            ledger
                .observe_member(1, &observation("a", 9 * GIB, 10 * GIB))
                .unwrap(),
            CommandRecovery::Ambiguous
        );
        assert_eq!(
            ledger.members[0].command.as_ref().unwrap().phase,
            PoolCommandPhase::Ambiguous
        );
        assert_eq!(ledger.accounting().unwrap().reserved_growth_bytes, 4 * GIB);
    }

    #[test]
    fn checksum_corruption_oversize_and_policy_drift_fail_closed() {
        let policy = policy();
        let path = temp_path("corruption");
        let store = FilePoolLedgerStore::new(&path);
        let ledger = PoolLedger::cold_start(&policy, &observations(8 * GIB, 8 * GIB)).unwrap();
        store.store(&policy, &ledger).unwrap();
        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        document["ledger"]["revision"] = serde_json::json!(7);
        fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(matches!(
            store.load(&policy),
            Err(PoolLedgerError::ChecksumMismatch)
        ));

        fs::write(&path, vec![b'x'; MAX_HOST_POOL_LEDGER_BYTES as usize + 1]).unwrap();
        assert!(matches!(
            store.load(&policy),
            Err(PoolLedgerError::Oversized)
        ));

        store.store(&policy, &ledger).unwrap();
        let mut changed_policy = policy.clone();
        changed_policy.members[0].priority += 1;
        assert!(matches!(
            store.load(&changed_policy),
            Err(PoolLedgerError::InvalidLedger(_))
        ));

        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        document["version"] = serde_json::json!(HOST_POOL_LEDGER_VERSION + 1);
        fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(matches!(
            store.load(&policy),
            Err(PoolLedgerError::InvalidLedger(_))
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn successful_atomic_replace_leaves_no_temporary_file() {
        let policy = policy();
        let path = temp_path("atomic");
        let store = FilePoolLedgerStore::new(&path);
        let mut ledger = PoolLedger::cold_start(&policy, &observations(8 * GIB, 8 * GIB)).unwrap();
        store.store(&policy, &ledger).unwrap();
        ledger
            .begin_commands(
                0,
                1,
                &[request("a", "grow", PoolCommandDirection::Growth, 10 * GIB)],
            )
            .unwrap();
        store.store(&policy, &ledger).unwrap();
        assert_eq!(store.load(&policy).unwrap(), Some(ledger));
        let prefix = format!("{}.tmp-", path.file_name().unwrap().to_string_lossy());
        let parent = path.parent().unwrap();
        assert!(!fs::read_dir(parent).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&prefix)
        }));
        fs::remove_file(path).unwrap();
    }
}
