//! Windows pressure assessment and bounded growth selection.
//!
//! This module has no dependency on an actuation sink. Assessment stays
//! separate from the host-owned decision to apply a bounded growth target.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{MemoryResourceNotificationState, RawTelemetryEnvelope, TelemetrySignalStatus};

pub const PRESSURE_ASSESSMENT_VERSION: u16 = 1;
pub const PRESSURE_POLICY_VERSION: u16 = 1;
pub const PRESSURE_HISTORY_VERSION: u16 = 1;
pub const PRESSURE_SHADOW_COMPARISON_VERSION: u16 = 2;
pub const MAX_PRESSURE_HISTORY_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressurePolicyMode {
    Legacy,
    Shadow,
    Growth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureGrowthMode {
    Normal,
    Urgent,
    Fallback,
    Held,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureGrowthDecision {
    pub mode: PressureGrowthMode,
    pub goal_bytes: u64,
    pub next_target_bytes: Option<u64>,
    pub capacity_limited: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressurePolicyConfig {
    pub demand_margin_ratio_numerator: u64,
    pub demand_margin_ratio_denominator: u64,
    pub demand_margin_minimum_bytes: u64,
    pub demand_margin_maximum_bytes: u64,
    pub downward_hysteresis_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentAllocation {
    pub device_size_bytes: u64,
    pub block_size_bytes: u64,
    pub requested_bytes: u64,
    pub current_bytes: u64,
    pub configured_minimum_bytes: u64,
    pub effective_maximum_bytes: u64,
    pub visible_base_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureHistorySummary {
    pub complete: bool,
    pub healthy: bool,
    pub all_high: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureHistoryEntry {
    pub observed_unix_millis: u64,
    pub state: PressureState,
    pub healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureHistoryState {
    pub version: u16,
    pub session_id: String,
    pub last_sequence: u64,
    pub last_monotonic_millis: u64,
    pub last_observed_unix_millis: u64,
    pub entries: VecDeque<PressureHistoryEntry>,
}

impl Default for PressureHistoryState {
    fn default() -> Self {
        Self {
            version: PRESSURE_HISTORY_VERSION,
            session_id: String::new(),
            last_sequence: 0,
            last_monotonic_millis: 0,
            last_observed_unix_millis: 0,
            entries: VecDeque::new(),
        }
    }
}

impl PressureHistoryState {
    pub fn invalidate(&mut self) {
        self.entries.clear();
    }

    pub fn observe(
        &mut self,
        pressure: &PressureStateAssessment,
        history_window_millis: u64,
        maximum_gap_millis: u64,
    ) -> Result<PressureHistorySummary, PressureAssessmentError> {
        if self.version != PRESSURE_HISTORY_VERSION
            || history_window_millis == 0
            || maximum_gap_millis == 0
            || maximum_gap_millis > history_window_millis
        {
            self.invalidate();
            return Err(PressureAssessmentError::InvalidHistory(
                "version or window configuration is invalid",
            ));
        }
        let input = &pressure.input;
        if !self.session_id.is_empty() && input.session_id == self.session_id {
            if input.sequence <= self.last_sequence
                || input.monotonic_millis <= self.last_monotonic_millis
                || input.observed_unix_millis < self.last_observed_unix_millis
            {
                self.invalidate();
                return Err(PressureAssessmentError::InvalidHistory(
                    "sample did not advance within the producer session",
                ));
            }
            if input.monotonic_millis - self.last_monotonic_millis > maximum_gap_millis {
                self.invalidate();
            }
        } else if !self.session_id.is_empty() {
            self.invalidate();
        }
        self.session_id.clone_from(&input.session_id);
        self.last_sequence = input.sequence;
        self.last_monotonic_millis = input.monotonic_millis;
        self.last_observed_unix_millis = input.observed_unix_millis;
        self.entries.push_back(PressureHistoryEntry {
            observed_unix_millis: input.observed_unix_millis,
            state: pressure.state,
            healthy: pressure.availability == AssessmentAvailability::Available
                && pressure.confidence != AssessmentConfidence::Contradicted,
        });
        if self.entries.len() > MAX_PRESSURE_HISTORY_ENTRIES {
            self.entries.pop_front();
        }
        let boundary = input
            .observed_unix_millis
            .saturating_sub(history_window_millis);
        while self.entries.len() > 1
            && self
                .entries
                .get(1)
                .is_some_and(|entry| entry.observed_unix_millis <= boundary)
        {
            self.entries.pop_front();
        }
        let complete = self
            .entries
            .front()
            .is_some_and(|first| first.observed_unix_millis <= boundary)
            && self
                .entries
                .iter()
                .zip(self.entries.iter().skip(1))
                .all(|(first, second)| {
                    second
                        .observed_unix_millis
                        .saturating_sub(first.observed_unix_millis)
                        <= maximum_gap_millis
                });
        Ok(PressureHistorySummary {
            complete,
            healthy: complete && self.entries.iter().all(|entry| entry.healthy),
            all_high: complete
                && self
                    .entries
                    .iter()
                    .all(|entry| entry.state == PressureState::HighMemory),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentInputIdentity {
    pub session_id: String,
    pub sequence: u64,
    pub monotonic_millis: u64,
    pub observed_unix_millis: u64,
    pub requested_bytes: u64,
    pub current_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentConfidence {
    Experimental,
    Authoritative,
    Corroborated,
    Contradicted,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureState {
    LowMemory,
    Neutral,
    HighMemory,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShrinkSafetyState {
    Eligible,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentReason {
    CommitDemandWithConfiguredMargin,
    NotificationLow,
    NotificationNeutral,
    NotificationHigh,
    NotificationUnavailable,
    NotificationFailed,
    RequirementExceedsCurrent,
    RequirementBelowCurrent,
    CommitHeadroomBelowMargin,
    PhysicalAvailableBelowMargin,
    AllocationInFlight,
    HistoryIncomplete,
    HistoryUnhealthy,
    HistoryNotSustainedHigh,
    PressureNotHigh,
    RequirementNotBelowAllocation,
    HysteresisNotSatisfied,
    AllShrinkConditionsSatisfied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequirementAssessment {
    pub version: u16,
    pub policy_version: u16,
    pub availability: AssessmentAvailability,
    pub confidence: AssessmentConfidence,
    pub input: AssessmentInputIdentity,
    pub demand_margin_bytes: u64,
    pub visible_requirement_bytes: u64,
    pub requirement_bytes: u64,
    pub capacity_limited: bool,
    pub reasons: Vec<AssessmentReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureStateAssessment {
    pub version: u16,
    pub policy_version: u16,
    pub availability: AssessmentAvailability,
    pub confidence: AssessmentConfidence,
    pub input: AssessmentInputIdentity,
    pub state: PressureState,
    pub reasons: Vec<AssessmentReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShrinkSafetyAssessment {
    pub version: u16,
    pub policy_version: u16,
    pub availability: AssessmentAvailability,
    pub confidence: AssessmentConfidence,
    pub input: AssessmentInputIdentity,
    pub state: ShrinkSafetyState,
    pub reasons: Vec<AssessmentReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsPressureAssessment {
    pub version: u16,
    pub policy_version: u16,
    pub memory_requirement: MemoryRequirementAssessment,
    pub pressure: PressureStateAssessment,
    pub shrink_safety: ShrinkSafetyAssessment,
    pub fixed_headroom_candidate_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureShadowComparison {
    pub version: u16,
    pub vm_name: String,
    pub device_alias: String,
    pub policy_fingerprint_sha256: String,
    pub assessment: WindowsPressureAssessment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub growth_decision: Option<PressureGrowthDecision>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PressureAssessmentError {
    #[error("pressure policy is invalid: {0}")]
    InvalidPolicy(&'static str),
    #[error("pressure assessment allocation is invalid: {0}")]
    InvalidAllocation(&'static str),
    #[error("pressure assessment telemetry is invalid: {0}")]
    InvalidTelemetry(String),
    #[error("pressure assessment arithmetic overflow")]
    ArithmeticOverflow,
    #[error("pressure history is invalid: {0}")]
    InvalidHistory(&'static str),
}

pub fn select_pressure_growth(
    assessment: &WindowsPressureAssessment,
    current_bytes: u64,
    effective_maximum_bytes: u64,
    block_size_bytes: u64,
    normal_growth_step_bytes: u64,
    urgent_growth_step_bytes: u64,
    fallback_enabled: bool,
) -> Result<PressureGrowthDecision, PressureAssessmentError> {
    if block_size_bytes == 0
        || assessment.version != PRESSURE_ASSESSMENT_VERSION
        || assessment.policy_version != PRESSURE_POLICY_VERSION
        || assessment.memory_requirement.input != assessment.pressure.input
        || assessment.pressure.input != assessment.shrink_safety.input
        || assessment.memory_requirement.input.current_bytes != current_bytes
        || assessment.memory_requirement.requirement_bytes == 0
        || assessment.memory_requirement.requirement_bytes > effective_maximum_bytes
        || !assessment
            .memory_requirement
            .requirement_bytes
            .is_multiple_of(block_size_bytes)
    {
        return Err(PressureAssessmentError::InvalidAllocation(
            "growth assessment identity, version, or requirement is invalid",
        ));
    }
    if current_bytes > effective_maximum_bytes
        || block_size_bytes == 0
        || !block_size_bytes.is_power_of_two()
        || normal_growth_step_bytes == 0
        || urgent_growth_step_bytes == 0
        || !current_bytes.is_multiple_of(block_size_bytes)
        || !effective_maximum_bytes.is_multiple_of(block_size_bytes)
        || !normal_growth_step_bytes.is_multiple_of(block_size_bytes)
        || !urgent_growth_step_bytes.is_multiple_of(block_size_bytes)
    {
        return Err(PressureAssessmentError::InvalidAllocation(
            "growth bounds, geometry, or movement quanta are invalid",
        ));
    }

    let requirement = assessment.memory_requirement.requirement_bytes;
    if assessment.pressure.state == PressureState::Unavailable
        && fallback_enabled
        && (assessment.fixed_headroom_candidate_bytes == 0
            || assessment.fixed_headroom_candidate_bytes > effective_maximum_bytes
            || !assessment
                .fixed_headroom_candidate_bytes
                .is_multiple_of(block_size_bytes))
    {
        return Err(PressureAssessmentError::InvalidAllocation(
            "fallback growth candidate is invalid",
        ));
    }
    let (mode, raw_goal, step) = match assessment.pressure.state {
        PressureState::LowMemory
            if assessment.pressure.availability == AssessmentAvailability::Available =>
        {
            let relief = current_bytes.saturating_add(urgent_growth_step_bytes);
            (
                PressureGrowthMode::Urgent,
                requirement.max(relief),
                urgent_growth_step_bytes,
            )
        }
        PressureState::Unavailable if fallback_enabled => (
            PressureGrowthMode::Fallback,
            assessment.fixed_headroom_candidate_bytes,
            normal_growth_step_bytes,
        ),
        PressureState::Unavailable => (PressureGrowthMode::Held, current_bytes, 0),
        _ => (
            PressureGrowthMode::Normal,
            requirement,
            normal_growth_step_bytes,
        ),
    };
    let goal_bytes = raw_goal.min(effective_maximum_bytes);
    let capacity_limited =
        assessment.memory_requirement.capacity_limited || raw_goal > effective_maximum_bytes;
    let next_target_bytes = if mode == PressureGrowthMode::Held || goal_bytes <= current_bytes {
        None
    } else {
        Some(
            current_bytes
                .checked_add(step)
                .ok_or(PressureAssessmentError::ArithmeticOverflow)?
                .min(goal_bytes),
        )
    };
    Ok(PressureGrowthDecision {
        mode: if next_target_bytes.is_none() {
            PressureGrowthMode::Held
        } else {
            mode
        },
        goal_bytes: goal_bytes.max(current_bytes),
        next_target_bytes,
        capacity_limited,
    })
}

pub fn assess_windows_pressure(
    envelope: &RawTelemetryEnvelope,
    policy: PressurePolicyConfig,
    allocation: AssessmentAllocation,
    history: PressureHistorySummary,
    fixed_headroom_candidate_bytes: u64,
) -> Result<WindowsPressureAssessment, PressureAssessmentError> {
    validate_inputs(envelope, policy, allocation)?;
    let input = AssessmentInputIdentity {
        session_id: envelope.session_id.clone(),
        sequence: envelope.sequence,
        monotonic_millis: envelope.monotonic_millis,
        observed_unix_millis: envelope.observed_unix_millis,
        requested_bytes: allocation.requested_bytes,
        current_bytes: allocation.current_bytes,
    };
    let proportional = ceil_ratio(
        envelope.memory.commit_total_bytes,
        policy.demand_margin_ratio_numerator,
        policy.demand_margin_ratio_denominator,
    )?;
    let demand_margin_bytes = proportional.clamp(
        policy.demand_margin_minimum_bytes,
        policy.demand_margin_maximum_bytes,
    );
    let visible_requirement_bytes = envelope
        .memory
        .commit_total_bytes
        .checked_add(demand_margin_bytes)
        .ok_or(PressureAssessmentError::ArithmeticOverflow)?;
    let raw_requirement = visible_requirement_bytes.saturating_sub(allocation.visible_base_bytes);
    let bounded = raw_requirement.clamp(
        allocation.configured_minimum_bytes,
        allocation.effective_maximum_bytes,
    );
    let requirement_bytes =
        align_up(bounded, allocation.block_size_bytes)?.min(allocation.effective_maximum_bytes);
    let capacity_limited = raw_requirement > allocation.effective_maximum_bytes;
    let requirement = MemoryRequirementAssessment {
        version: PRESSURE_ASSESSMENT_VERSION,
        policy_version: PRESSURE_POLICY_VERSION,
        availability: AssessmentAvailability::Available,
        confidence: AssessmentConfidence::Experimental,
        input: input.clone(),
        demand_margin_bytes,
        visible_requirement_bytes,
        requirement_bytes,
        capacity_limited,
        reasons: vec![AssessmentReason::CommitDemandWithConfiguredMargin],
    };

    let commit_headroom = envelope.memory.commit_limit_bytes - envelope.memory.commit_total_bytes;
    let physical_low = envelope.memory.physical_available_bytes < demand_margin_bytes;
    let commit_low = commit_headroom < demand_margin_bytes;
    let (state, availability, mut confidence, mut reasons) = pressure_from_notification(envelope);
    if requirement_bytes > allocation.current_bytes {
        reasons.push(AssessmentReason::RequirementExceedsCurrent);
        if state == PressureState::HighMemory {
            confidence = AssessmentConfidence::Contradicted;
        } else if state == PressureState::LowMemory {
            confidence = AssessmentConfidence::Corroborated;
        }
    } else if requirement_bytes < allocation.current_bytes {
        reasons.push(AssessmentReason::RequirementBelowCurrent);
    }
    if commit_low {
        reasons.push(AssessmentReason::CommitHeadroomBelowMargin);
        if state == PressureState::HighMemory {
            confidence = AssessmentConfidence::Contradicted;
        } else if state == PressureState::LowMemory {
            confidence = AssessmentConfidence::Corroborated;
        }
    }
    if physical_low {
        reasons.push(AssessmentReason::PhysicalAvailableBelowMargin);
        if state == PressureState::HighMemory {
            confidence = AssessmentConfidence::Contradicted;
        } else if state == PressureState::LowMemory {
            confidence = AssessmentConfidence::Corroborated;
        }
    }
    let pressure = PressureStateAssessment {
        version: PRESSURE_ASSESSMENT_VERSION,
        policy_version: PRESSURE_POLICY_VERSION,
        availability,
        confidence,
        input: input.clone(),
        state,
        reasons,
    };

    let mut shrink_reasons = Vec::new();
    if allocation.requested_bytes != allocation.current_bytes {
        shrink_reasons.push(AssessmentReason::AllocationInFlight);
    }
    if !history.complete {
        shrink_reasons.push(AssessmentReason::HistoryIncomplete);
    }
    if !history.healthy {
        shrink_reasons.push(AssessmentReason::HistoryUnhealthy);
    }
    if !history.all_high {
        shrink_reasons.push(AssessmentReason::HistoryNotSustainedHigh);
    }
    if state != PressureState::HighMemory {
        shrink_reasons.push(AssessmentReason::PressureNotHigh);
    }
    if requirement_bytes >= allocation.current_bytes {
        shrink_reasons.push(AssessmentReason::RequirementNotBelowAllocation);
    } else if requirement_bytes
        .checked_add(policy.downward_hysteresis_bytes)
        .ok_or(PressureAssessmentError::ArithmeticOverflow)?
        > allocation.current_bytes
    {
        shrink_reasons.push(AssessmentReason::HysteresisNotSatisfied);
    }
    if commit_low || physical_low {
        // These reason codes are shared deliberately: the pressure result uses
        // them as corroboration/contradiction while shrink uses them as gates.
        if commit_low {
            shrink_reasons.push(AssessmentReason::CommitHeadroomBelowMargin);
        }
        if physical_low {
            shrink_reasons.push(AssessmentReason::PhysicalAvailableBelowMargin);
        }
    }
    let shrink_state = if shrink_reasons.is_empty() {
        shrink_reasons.push(AssessmentReason::AllShrinkConditionsSatisfied);
        ShrinkSafetyState::Eligible
    } else {
        ShrinkSafetyState::Blocked
    };
    let shrink_safety = ShrinkSafetyAssessment {
        version: PRESSURE_ASSESSMENT_VERSION,
        policy_version: PRESSURE_POLICY_VERSION,
        availability: if availability == AssessmentAvailability::Available {
            AssessmentAvailability::Available
        } else {
            AssessmentAvailability::Unavailable
        },
        confidence: if shrink_state == ShrinkSafetyState::Eligible {
            AssessmentConfidence::Corroborated
        } else if availability == AssessmentAvailability::Unavailable {
            AssessmentConfidence::Unavailable
        } else {
            confidence
        },
        input,
        state: shrink_state,
        reasons: shrink_reasons,
    };

    Ok(WindowsPressureAssessment {
        version: PRESSURE_ASSESSMENT_VERSION,
        policy_version: PRESSURE_POLICY_VERSION,
        memory_requirement: requirement,
        pressure,
        shrink_safety,
        fixed_headroom_candidate_bytes,
    })
}

fn validate_inputs(
    envelope: &RawTelemetryEnvelope,
    policy: PressurePolicyConfig,
    allocation: AssessmentAllocation,
) -> Result<(), PressureAssessmentError> {
    envelope
        .contract_mode()
        .map_err(|error| PressureAssessmentError::InvalidTelemetry(error.to_string()))?;
    envelope
        .memory
        .validate()
        .map_err(|error| PressureAssessmentError::InvalidTelemetry(error.to_string()))?;
    if policy.demand_margin_ratio_denominator == 0
        || policy.demand_margin_minimum_bytes > policy.demand_margin_maximum_bytes
        || policy.downward_hysteresis_bytes == 0
    {
        return Err(PressureAssessmentError::InvalidPolicy(
            "margin ratio, bounds, or hysteresis are invalid",
        ));
    }
    if allocation.block_size_bytes == 0
        || !allocation.block_size_bytes.is_power_of_two()
        || allocation.configured_minimum_bytes == 0
        || allocation.configured_minimum_bytes > allocation.effective_maximum_bytes
        || allocation.effective_maximum_bytes >= allocation.device_size_bytes
        || allocation.current_bytes > allocation.effective_maximum_bytes
        || allocation.requested_bytes > allocation.effective_maximum_bytes
        || !allocation
            .current_bytes
            .is_multiple_of(allocation.block_size_bytes)
        || !allocation
            .requested_bytes
            .is_multiple_of(allocation.block_size_bytes)
        || !allocation
            .configured_minimum_bytes
            .is_multiple_of(allocation.block_size_bytes)
        || !allocation
            .effective_maximum_bytes
            .is_multiple_of(allocation.block_size_bytes)
    {
        return Err(PressureAssessmentError::InvalidAllocation(
            "bounds or alignment are invalid",
        ));
    }
    Ok(())
}

fn pressure_from_notification(
    envelope: &RawTelemetryEnvelope,
) -> (
    PressureState,
    AssessmentAvailability,
    AssessmentConfidence,
    Vec<AssessmentReason>,
) {
    let Some(native) = envelope.windows_native.as_ref() else {
        return unavailable(AssessmentReason::NotificationUnavailable);
    };
    let signal = &native.memory_resource_notifications;
    match (signal.status, signal.value) {
        (TelemetrySignalStatus::Supported, Some(MemoryResourceNotificationState::Low)) => (
            PressureState::LowMemory,
            AssessmentAvailability::Available,
            AssessmentConfidence::Authoritative,
            vec![AssessmentReason::NotificationLow],
        ),
        (TelemetrySignalStatus::Supported, Some(MemoryResourceNotificationState::Neutral)) => (
            PressureState::Neutral,
            AssessmentAvailability::Available,
            AssessmentConfidence::Authoritative,
            vec![AssessmentReason::NotificationNeutral],
        ),
        (TelemetrySignalStatus::Supported, Some(MemoryResourceNotificationState::High)) => (
            PressureState::HighMemory,
            AssessmentAvailability::Available,
            AssessmentConfidence::Authoritative,
            vec![AssessmentReason::NotificationHigh],
        ),
        (TelemetrySignalStatus::Failed, _) => unavailable(AssessmentReason::NotificationFailed),
        _ => unavailable(AssessmentReason::NotificationUnavailable),
    }
}

fn unavailable(
    reason: AssessmentReason,
) -> (
    PressureState,
    AssessmentAvailability,
    AssessmentConfidence,
    Vec<AssessmentReason>,
) {
    (
        PressureState::Unavailable,
        AssessmentAvailability::Unavailable,
        AssessmentConfidence::Unavailable,
        vec![reason],
    )
}

fn ceil_ratio(
    value: u64,
    numerator: u64,
    denominator: u64,
) -> Result<u64, PressureAssessmentError> {
    let product = value
        .checked_mul(numerator)
        .ok_or(PressureAssessmentError::ArithmeticOverflow)?;
    let quotient = product / denominator;
    let remainder = product % denominator;
    quotient
        .checked_add(u64::from(remainder != 0))
        .ok_or(PressureAssessmentError::ArithmeticOverflow)
}

fn align_up(value: u64, alignment: u64) -> Result<u64, PressureAssessmentError> {
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or(PressureAssessmentError::ArithmeticOverflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllocationProvenance, MemoryTelemetrySnapshot, OptionalTelemetrySignal,
        TelemetryCapability, TelemetrySource, WindowsNativeTelemetry, WindowsTelemetryCapabilities,
        RAW_TELEMETRY_VERSION, WINDOWS_NATIVE_TELEMETRY_VERSION,
    };

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn policy() -> PressurePolicyConfig {
        PressurePolicyConfig {
            demand_margin_ratio_numerator: 1,
            demand_margin_ratio_denominator: 10,
            demand_margin_minimum_bytes: 256 * MIB,
            demand_margin_maximum_bytes: 2 * GIB,
            downward_hysteresis_bytes: 512 * MIB,
        }
    }

    fn allocation() -> AssessmentAllocation {
        AssessmentAllocation {
            device_size_bytes: 32 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 8 * GIB,
            current_bytes: 8 * GIB,
            configured_minimum_bytes: 2 * GIB,
            effective_maximum_bytes: 16 * GIB,
            visible_base_bytes: 4 * GIB,
        }
    }

    fn envelope(notification: Option<MemoryResourceNotificationState>) -> RawTelemetryEnvelope {
        let monotonic_millis = 10_000;
        let signal = notification.map_or_else(
            || OptionalTelemetrySignal::unavailable(monotonic_millis),
            |value| OptionalTelemetrySignal::supported(monotonic_millis, value),
        );
        RawTelemetryEnvelope {
            version: RAW_TELEMETRY_VERSION,
            vm_name: "vm".to_owned(),
            service_name: "service".to_owned(),
            session_id: "session-a".to_owned(),
            observed_unix_millis: 20_000,
            monotonic_millis,
            sequence: 7,
            telemetry_source: TelemetrySource::WindowsNativeMemoryApis,
            allocation_provenance: AllocationProvenance::HostLiveLibvirtCurrentRequired,
            memory: MemoryTelemetrySnapshot {
                physical_total_bytes: 12 * GIB,
                physical_available_bytes: 3 * GIB,
                memory_load_percent: 75,
                commit_total_bytes: 7 * GIB,
                commit_limit_bytes: 16 * GIB,
                commit_peak_bytes: 7 * GIB,
                system_cache_bytes: GIB,
                kernel_paged_bytes: 0,
                kernel_nonpaged_bytes: 0,
            },
            windows_native: Some(WindowsNativeTelemetry {
                version: WINDOWS_NATIVE_TELEMETRY_VERSION,
                capabilities: WindowsTelemetryCapabilities {
                    memory_resource_notifications: if notification.is_some() {
                        TelemetryCapability::Supported
                    } else {
                        TelemetryCapability::Unavailable
                    },
                    reusable_memory: TelemetryCapability::Unavailable,
                    paging_activity: TelemetryCapability::Unavailable,
                },
                memory_resource_notifications: signal,
                reusable_memory: OptionalTelemetrySignal::unavailable(monotonic_millis),
                paging_activity: OptionalTelemetrySignal::unavailable(monotonic_millis),
            }),
        }
    }

    fn history() -> PressureHistorySummary {
        PressureHistorySummary {
            complete: true,
            healthy: true,
            all_high: true,
        }
    }

    #[test]
    fn calculates_only_from_commit_margin_and_bounds() {
        let low = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::Low)),
            policy(),
            allocation(),
            history(),
            9 * GIB,
        )
        .expect("assessment");
        let high = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::High)),
            policy(),
            allocation(),
            history(),
            9 * GIB,
        )
        .expect("assessment");
        assert_eq!(low.memory_requirement, high.memory_requirement);
        assert_eq!(low.memory_requirement.demand_margin_bytes, 7 * GIB / 10 + 1);
        assert_eq!(low.memory_requirement.requirement_bytes, 3_790 * MIB);
        assert_eq!(low.fixed_headroom_candidate_bytes, 9 * GIB);
    }

    #[test]
    fn pressure_is_notification_led_and_marks_contradiction() {
        let low = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::Low)),
            policy(),
            allocation(),
            history(),
            0,
        )
        .expect("low");
        assert_eq!(low.pressure.state, PressureState::LowMemory);
        assert_eq!(low.pressure.confidence, AssessmentConfidence::Authoritative);

        let mut stressed = envelope(Some(MemoryResourceNotificationState::High));
        stressed.memory.commit_total_bytes = 15 * GIB;
        stressed.memory.commit_peak_bytes = 15 * GIB;
        let high =
            assess_windows_pressure(&stressed, policy(), allocation(), history(), 0).expect("high");
        assert_eq!(high.pressure.state, PressureState::HighMemory);
        assert_eq!(high.pressure.confidence, AssessmentConfidence::Contradicted);
        assert!(high
            .pressure
            .reasons
            .contains(&AssessmentReason::CommitHeadroomBelowMargin));
        assert_eq!(
            serde_json::to_string(&PressureState::LowMemory).expect("low state JSON"),
            "\"low_memory\""
        );
        assert_eq!(
            serde_json::to_string(&PressureState::HighMemory).expect("high state JSON"),
            "\"high_memory\""
        );
    }

    #[test]
    fn unavailable_notification_does_not_synthesize_pressure() {
        let result = assess_windows_pressure(&envelope(None), policy(), allocation(), history(), 0)
            .expect("assessment");
        assert_eq!(result.pressure.state, PressureState::Unavailable);
        assert_eq!(
            result.pressure.availability,
            AssessmentAvailability::Unavailable
        );
        assert_eq!(result.shrink_safety.state, ShrinkSafetyState::Blocked);
    }

    #[test]
    fn pressure_growth_selects_normal_urgent_and_fallback_steps() {
        let mut growth_allocation = allocation();
        growth_allocation.requested_bytes = 3 * GIB;
        growth_allocation.current_bytes = 3 * GIB;
        let normal = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::Neutral)),
            policy(),
            growth_allocation,
            history(),
            9 * GIB,
        )
        .expect("normal assessment");
        assert_eq!(
            select_pressure_growth(&normal, 3 * GIB, 10 * GIB, 2 * MIB, GIB, 2 * GIB, true)
                .expect("normal growth"),
            PressureGrowthDecision {
                mode: PressureGrowthMode::Normal,
                goal_bytes: 3_790 * MIB,
                next_target_bytes: Some(3_790 * MIB),
                capacity_limited: false,
            }
        );

        let urgent = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::Low)),
            policy(),
            growth_allocation,
            history(),
            9 * GIB,
        )
        .expect("urgent assessment");
        assert_eq!(
            select_pressure_growth(&urgent, 3 * GIB, 10 * GIB, 2 * MIB, GIB, 2 * GIB, true)
                .expect("urgent growth"),
            PressureGrowthDecision {
                mode: PressureGrowthMode::Urgent,
                goal_bytes: 5 * GIB,
                next_target_bytes: Some(5 * GIB),
                capacity_limited: false,
            }
        );

        let fallback = assess_windows_pressure(
            &envelope(None),
            policy(),
            growth_allocation,
            history(),
            9 * GIB,
        )
        .expect("fallback assessment");
        assert_eq!(
            select_pressure_growth(&fallback, 3 * GIB, 10 * GIB, 2 * MIB, GIB, 2 * GIB, true)
                .expect("fallback growth"),
            PressureGrowthDecision {
                mode: PressureGrowthMode::Fallback,
                goal_bytes: 9 * GIB,
                next_target_bytes: Some(4 * GIB),
                capacity_limited: false,
            }
        );
    }

    #[test]
    fn pressure_growth_holds_without_fallback_and_never_selects_shrink() {
        let unavailable =
            assess_windows_pressure(&envelope(None), policy(), allocation(), history(), 2 * GIB)
                .expect("unavailable assessment");
        let held = select_pressure_growth(
            &unavailable,
            8 * GIB,
            10 * GIB,
            2 * MIB,
            GIB,
            2 * GIB,
            false,
        )
        .expect("held decision");
        assert_eq!(held.mode, PressureGrowthMode::Held);
        assert_eq!(held.goal_bytes, 8 * GIB);
        assert_eq!(held.next_target_bytes, None);

        let high = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::High)),
            policy(),
            allocation(),
            history(),
            2 * GIB,
        )
        .expect("high assessment");
        let no_shrink =
            select_pressure_growth(&high, 8 * GIB, 10 * GIB, 2 * MIB, GIB, 2 * GIB, true)
                .expect("no shrink");
        assert_eq!(no_shrink.mode, PressureGrowthMode::Held);
        assert_eq!(no_shrink.next_target_bytes, None);
        assert_eq!(no_shrink.goal_bytes, 8 * GIB);
    }

    #[test]
    fn shrink_requires_complete_healthy_high_history_and_converged_allocation() {
        let eligible = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::High)),
            policy(),
            allocation(),
            history(),
            0,
        )
        .expect("eligible");
        assert_eq!(eligible.shrink_safety.state, ShrinkSafetyState::Eligible);

        for blocked_history in [
            PressureHistorySummary {
                complete: false,
                ..history()
            },
            PressureHistorySummary {
                healthy: false,
                ..history()
            },
            PressureHistorySummary {
                all_high: false,
                ..history()
            },
        ] {
            let blocked = assess_windows_pressure(
                &envelope(Some(MemoryResourceNotificationState::High)),
                policy(),
                allocation(),
                blocked_history,
                0,
            )
            .expect("blocked");
            assert_eq!(blocked.shrink_safety.state, ShrinkSafetyState::Blocked);
        }

        let mut in_flight = allocation();
        in_flight.requested_bytes += 2 * MIB;
        let blocked = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::High)),
            policy(),
            in_flight,
            history(),
            0,
        )
        .expect("blocked");
        assert!(blocked
            .shrink_safety
            .reasons
            .contains(&AssessmentReason::AllocationInFlight));
    }

    #[test]
    fn rejects_overflow_and_invalid_geometry() {
        let mut overflow = envelope(Some(MemoryResourceNotificationState::Neutral));
        overflow.memory.commit_total_bytes = u64::MAX;
        overflow.memory.commit_limit_bytes = u64::MAX;
        overflow.memory.commit_peak_bytes = u64::MAX;
        assert_eq!(
            assess_windows_pressure(&overflow, policy(), allocation(), history(), 0),
            Err(PressureAssessmentError::ArithmeticOverflow)
        );
        let mut invalid = allocation();
        invalid.current_bytes += 1;
        assert!(matches!(
            assess_windows_pressure(
                &envelope(Some(MemoryResourceNotificationState::Neutral)),
                policy(),
                invalid,
                history(),
                0
            ),
            Err(PressureAssessmentError::InvalidAllocation(_))
        ));
    }

    #[test]
    fn reason_codes_have_stable_golden_json() {
        let result = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::Neutral)),
            policy(),
            allocation(),
            PressureHistorySummary {
                complete: false,
                healthy: true,
                all_high: false,
            },
            0,
        )
        .expect("assessment");
        assert_eq!(
            serde_json::to_string(&result.shrink_safety.reasons).expect("json"),
            r#"["history_incomplete","history_not_sustained_high","pressure_not_high"]"#
        );
    }

    #[test]
    fn history_rewarms_after_restart_gap_and_invalid_ordering() {
        let mut state = PressureHistoryState::default();
        let base = assess_windows_pressure(
            &envelope(Some(MemoryResourceNotificationState::High)),
            policy(),
            allocation(),
            PressureHistorySummary {
                complete: false,
                healthy: false,
                all_high: false,
            },
            0,
        )
        .expect("assessment")
        .pressure;
        let mut sample = base.clone();
        sample.input.observed_unix_millis = 1_000;
        sample.input.monotonic_millis = 1_000;
        sample.input.sequence = 1;
        assert!(!state.observe(&sample, 1_000, 500).expect("first").complete);
        sample.input.observed_unix_millis = 2_000;
        sample.input.monotonic_millis = 2_000;
        sample.input.sequence = 2;
        let over_gap = state.observe(&sample, 1_000, 500).expect("over gap");
        assert!(!over_gap.complete);
        assert_eq!(state.entries.len(), 1);

        sample.input.session_id = "session-b".to_owned();
        sample.input.observed_unix_millis = 2_400;
        sample.input.monotonic_millis = 100;
        sample.input.sequence = 0;
        state.observe(&sample, 1_000, 500).expect("restart");
        assert_eq!(state.entries.len(), 1);

        assert!(matches!(
            state.observe(&sample, 1_000, 500),
            Err(PressureAssessmentError::InvalidHistory(_))
        ));
        assert!(state.entries.is_empty());
    }
}
