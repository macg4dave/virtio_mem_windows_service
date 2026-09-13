//! Absolute target estimation from raw Windows memory telemetry.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{MemoryTelemetrySnapshot, MIN_HEADROOM_BYTES};

pub const TARGET_ESTIMATOR_STATE_VERSION: u16 = 1;
pub const MAX_TARGET_HISTORY_ENTRIES: usize = 4096;
const MIB: u64 = 1024 * 1024;
#[cfg(test)]
const GIB: u64 = 1024 * MIB;
const MIN_FIXED_BASE_TOLERANCE_BYTES: u64 = 256 * MIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetPolicyConfig {
    pub configured_minimum_bytes: u64,
    pub configured_maximum_bytes: u64,
    pub fixed_visible_base_bytes: u64,
    pub physical_reserve_bytes: u64,
    pub commit_reserve_bytes: u64,
    pub safe_floor_physical_reserve_bytes: u64,
    pub safe_floor_commit_reserve_bytes: u64,
    pub history_window_millis: u64,
    pub maximum_gap_millis: u64,
    pub downward_hysteresis_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetGeometry {
    pub device_size_bytes: u64,
    pub block_size_bytes: u64,
    pub current_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSample {
    pub session_id: String,
    pub observed_unix_millis: u64,
    pub monotonic_millis: u64,
    pub sequence: u64,
    pub memory: MemoryTelemetrySnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstantaneousTarget {
    pub desired_now_bytes: u64,
    pub floor_now_bytes: u64,
    pub effective_maximum_bytes: u64,
    pub capacity_limited: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetEstimate {
    pub desired_bytes: u64,
    pub safe_floor_bytes: u64,
    pub desired_now_bytes: u64,
    pub floor_now_bytes: u64,
    pub effective_maximum_bytes: u64,
    pub history_ready: bool,
    pub capacity_limited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateHistoryEntry {
    pub observed_unix_millis: u64,
    pub desired_now_bytes: u64,
    pub floor_now_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetEstimatorState {
    pub version: u16,
    pub desired_bytes: u64,
    pub safe_floor_bytes: u64,
    pub session_id: String,
    pub last_observed_unix_millis: u64,
    pub last_monotonic_millis: u64,
    pub last_sequence: u64,
    pub history: VecDeque<CandidateHistoryEntry>,
}

impl TargetEstimatorState {
    pub fn cold() -> Self {
        Self {
            version: TARGET_ESTIMATOR_STATE_VERSION,
            desired_bytes: 0,
            safe_floor_bytes: 0,
            session_id: String::new(),
            last_observed_unix_millis: 0,
            last_monotonic_millis: 0,
            last_sequence: 0,
            history: VecDeque::new(),
        }
    }

    pub fn invalidate_history(&mut self) {
        self.history.clear();
        self.safe_floor_bytes = self.desired_bytes;
    }
}

impl Default for TargetEstimatorState {
    fn default() -> Self {
        Self::cold()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TargetEstimatorError {
    #[error("target policy is invalid: {0}")]
    InvalidPolicy(&'static str),
    #[error("target geometry is invalid: {0}")]
    InvalidGeometry(&'static str),
    #[error("observed fixed visible memory base {observed} differs from configured {configured} beyond tolerance {tolerance}")]
    FixedBaseDrift {
        observed: u64,
        configured: u64,
        tolerance: u64,
    },
    #[error("target arithmetic overflow")]
    ArithmeticOverflow,
    #[error("target telemetry ordering is invalid: {0}")]
    InvalidOrdering(&'static str),
    #[error("memory telemetry is invalid: {0}")]
    InvalidTelemetry(String),
    #[error("target estimator state version is unsupported: {0}")]
    UnsupportedStateVersion(u16),
}

impl TargetPolicyConfig {
    pub fn validate(&self) -> Result<(), TargetEstimatorError> {
        if self.configured_minimum_bytes == 0
            || self.configured_maximum_bytes == 0
            || self.physical_reserve_bytes == 0
            || self.commit_reserve_bytes == 0
            || self.safe_floor_physical_reserve_bytes == 0
            || self.safe_floor_commit_reserve_bytes == 0
            || self.history_window_millis == 0
            || self.maximum_gap_millis == 0
            || self.downward_hysteresis_bytes == 0
        {
            return Err(TargetEstimatorError::InvalidPolicy(
                "byte values and durations must be positive",
            ));
        }
        if self.configured_minimum_bytes > self.configured_maximum_bytes {
            return Err(TargetEstimatorError::InvalidPolicy(
                "minimum exceeds configured maximum",
            ));
        }
        if self.safe_floor_physical_reserve_bytes > self.physical_reserve_bytes
            || self.safe_floor_commit_reserve_bytes > self.commit_reserve_bytes
        {
            return Err(TargetEstimatorError::InvalidPolicy(
                "safe-floor reserves exceed normal reserves",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct TargetEstimator {
    config: TargetPolicyConfig,
    state: TargetEstimatorState,
}

impl TargetEstimator {
    pub fn new(config: TargetPolicyConfig) -> Result<Self, TargetEstimatorError> {
        config.validate()?;
        Ok(Self {
            config,
            state: TargetEstimatorState::cold(),
        })
    }

    pub fn restore(
        config: TargetPolicyConfig,
        state: TargetEstimatorState,
    ) -> Result<Self, TargetEstimatorError> {
        config.validate()?;
        if state.version != TARGET_ESTIMATOR_STATE_VERSION {
            return Err(TargetEstimatorError::UnsupportedStateVersion(state.version));
        }
        if state.history.len() > MAX_TARGET_HISTORY_ENTRIES {
            return Err(TargetEstimatorError::InvalidOrdering(
                "checkpoint history exceeds its entry limit",
            ));
        }
        if state.desired_bytes == 0
            && (state.safe_floor_bytes != 0
                || !state.session_id.is_empty()
                || !state.history.is_empty())
        {
            return Err(TargetEstimatorError::InvalidOrdering(
                "cold checkpoint contains initialized state",
            ));
        }
        if state.desired_bytes != 0
            && (state.desired_bytes < config.configured_minimum_bytes
                || state.desired_bytes > config.configured_maximum_bytes
                || state.safe_floor_bytes < config.configured_minimum_bytes
                || state.safe_floor_bytes > state.desired_bytes)
        {
            return Err(TargetEstimatorError::InvalidOrdering(
                "checkpoint targets violate configured bounds",
            ));
        }
        if state.history.iter().any(|entry| {
            entry.floor_now_bytes < config.configured_minimum_bytes
                || entry.floor_now_bytes > entry.desired_now_bytes
                || entry.desired_now_bytes > config.configured_maximum_bytes
        }) || state
            .history
            .iter()
            .zip(state.history.iter().skip(1))
            .any(|(first, second)| first.observed_unix_millis >= second.observed_unix_millis)
        {
            return Err(TargetEstimatorError::InvalidOrdering(
                "checkpoint candidate history is invalid",
            ));
        }
        Ok(Self { config, state })
    }

    pub fn state(&self) -> &TargetEstimatorState {
        &self.state
    }

    /// Reports whether the persisted candidate history spans the configured
    /// window without an excessive sample gap.
    pub fn history_ready(&self) -> bool {
        history_is_ready(
            &self.state.history,
            self.config.history_window_millis,
            self.config.maximum_gap_millis,
        )
    }

    /// Clears reclaim readiness when the producer input fails before a target
    /// sample can be constructed.
    pub fn invalidate_history(&mut self) {
        self.state.invalidate_history();
    }

    pub fn estimate(
        &mut self,
        sample: TargetSample,
        geometry: TargetGeometry,
    ) -> Result<TargetEstimate, TargetEstimatorError> {
        let instantaneous = match calculate_instantaneous(&self.config, &sample.memory, geometry) {
            Ok(value) => value,
            Err(error) => {
                self.state.invalidate_history();
                return Err(error);
            }
        };
        if let Err(error) = self.prepare_sample(&sample) {
            self.state.invalidate_history();
            return Err(error);
        }

        if self.state.desired_bytes == 0 {
            self.state.desired_bytes = geometry.current_bytes.max(instantaneous.desired_now_bytes);
            self.state.safe_floor_bytes = geometry.current_bytes;
        }
        self.state.desired_bytes = self
            .state
            .desired_bytes
            .max(instantaneous.desired_now_bytes)
            .min(instantaneous.effective_maximum_bytes);

        self.state.history.push_back(CandidateHistoryEntry {
            observed_unix_millis: sample.observed_unix_millis,
            desired_now_bytes: instantaneous.desired_now_bytes,
            floor_now_bytes: instantaneous.floor_now_bytes,
        });
        if self.state.history.len() > MAX_TARGET_HISTORY_ENTRIES {
            self.state.history.pop_front();
        }
        prune_history(
            &mut self.state.history,
            sample.observed_unix_millis,
            self.config.history_window_millis,
        );
        let history_ready = history_is_ready(
            &self.state.history,
            self.config.history_window_millis,
            self.config.maximum_gap_millis,
        );
        if history_ready {
            let window_desired = self
                .state
                .history
                .iter()
                .map(|entry| entry.desired_now_bytes)
                .max()
                .ok_or(TargetEstimatorError::InvalidOrdering(
                    "empty qualified history",
                ))?;
            if window_desired
                .checked_add(self.config.downward_hysteresis_bytes)
                .ok_or(TargetEstimatorError::ArithmeticOverflow)?
                <= self.state.desired_bytes
            {
                self.state.desired_bytes = window_desired;
            }
            self.state.safe_floor_bytes = self
                .state
                .history
                .iter()
                .map(|entry| entry.floor_now_bytes)
                .max()
                .ok_or(TargetEstimatorError::InvalidOrdering(
                    "empty qualified history",
                ))?
                .min(self.state.desired_bytes);
        } else {
            self.state.safe_floor_bytes = geometry.current_bytes.min(self.state.desired_bytes);
        }

        self.state.session_id = sample.session_id;
        self.state.last_observed_unix_millis = sample.observed_unix_millis;
        self.state.last_monotonic_millis = sample.monotonic_millis;
        self.state.last_sequence = sample.sequence;

        Ok(TargetEstimate {
            desired_bytes: self.state.desired_bytes,
            safe_floor_bytes: self.state.safe_floor_bytes,
            desired_now_bytes: instantaneous.desired_now_bytes,
            floor_now_bytes: instantaneous.floor_now_bytes,
            effective_maximum_bytes: instantaneous.effective_maximum_bytes,
            history_ready,
            capacity_limited: instantaneous.capacity_limited,
        })
    }

    fn prepare_sample(&mut self, sample: &TargetSample) -> Result<(), TargetEstimatorError> {
        if sample.session_id.trim().is_empty() {
            return Err(TargetEstimatorError::InvalidOrdering(
                "empty producer session",
            ));
        }
        if self.state.session_id.is_empty() {
            return Ok(());
        }
        if sample.observed_unix_millis < self.state.last_observed_unix_millis {
            self.state = TargetEstimatorState::cold();
            return Ok(());
        }
        if sample.session_id != self.state.session_id {
            self.state.history.clear();
            return Ok(());
        }
        if sample.sequence <= self.state.last_sequence
            || sample.monotonic_millis <= self.state.last_monotonic_millis
        {
            return Err(TargetEstimatorError::InvalidOrdering(
                "sample did not advance within the producer session",
            ));
        }
        if sample
            .monotonic_millis
            .saturating_sub(self.state.last_monotonic_millis)
            > self.config.maximum_gap_millis
        {
            self.state.history.clear();
        }
        Ok(())
    }
}

pub fn calculate_instantaneous(
    config: &TargetPolicyConfig,
    memory: &MemoryTelemetrySnapshot,
    geometry: TargetGeometry,
) -> Result<InstantaneousTarget, TargetEstimatorError> {
    let effective_maximum_bytes = calculate_effective_maximum(config, geometry)?;
    memory
        .validate()
        .map_err(|error| TargetEstimatorError::InvalidTelemetry(error.to_string()))?;

    let observed_base = memory
        .physical_total_bytes
        .checked_sub(geometry.current_bytes)
        .ok_or(TargetEstimatorError::InvalidGeometry(
            "physical total is below current allocation",
        ))?;
    let tolerance = MIN_FIXED_BASE_TOLERANCE_BYTES.max(
        geometry
            .block_size_bytes
            .checked_mul(2)
            .ok_or(TargetEstimatorError::ArithmeticOverflow)?,
    );
    if observed_base.abs_diff(config.fixed_visible_base_bytes) > tolerance {
        return Err(TargetEstimatorError::FixedBaseDrift {
            observed: observed_base,
            configured: config.fixed_visible_base_bytes,
            tolerance,
        });
    }

    let desired_raw = candidate(
        memory,
        geometry.current_bytes,
        config.fixed_visible_base_bytes,
        config.physical_reserve_bytes,
        config.commit_reserve_bytes,
    )?
    .max(config.configured_minimum_bytes);
    let floor_raw = candidate(
        memory,
        geometry.current_bytes,
        config.fixed_visible_base_bytes,
        config.safe_floor_physical_reserve_bytes,
        config.safe_floor_commit_reserve_bytes,
    )?
    .max(config.configured_minimum_bytes);
    let capacity_limited = desired_raw > effective_maximum_bytes;
    let desired_now_bytes = align_up(
        desired_raw.min(effective_maximum_bytes),
        geometry.block_size_bytes,
    )?
    .min(effective_maximum_bytes);
    let floor_now_bytes = align_up(
        floor_raw.min(effective_maximum_bytes),
        geometry.block_size_bytes,
    )?
    .min(desired_now_bytes);

    Ok(InstantaneousTarget {
        desired_now_bytes,
        floor_now_bytes,
        effective_maximum_bytes,
        capacity_limited,
    })
}

/// Returns the aligned usable target ceiling after validating policy and live
/// device geometry, without consuming telemetry or changing estimator state.
pub fn calculate_effective_maximum(
    config: &TargetPolicyConfig,
    geometry: TargetGeometry,
) -> Result<u64, TargetEstimatorError> {
    config.validate()?;
    if geometry.device_size_bytes <= MIN_HEADROOM_BYTES {
        return Err(TargetEstimatorError::InvalidGeometry(
            "device size does not leave one GiB of headroom",
        ));
    }
    if geometry.block_size_bytes == 0 || !geometry.block_size_bytes.is_power_of_two() {
        return Err(TargetEstimatorError::InvalidGeometry(
            "block size must be a non-zero power of two",
        ));
    }
    if !geometry
        .current_bytes
        .is_multiple_of(geometry.block_size_bytes)
    {
        return Err(TargetEstimatorError::InvalidGeometry(
            "current allocation is not block aligned",
        ));
    }
    let effective_maximum_bytes = align_down(
        config
            .configured_maximum_bytes
            .min(geometry.device_size_bytes - MIN_HEADROOM_BYTES),
        geometry.block_size_bytes,
    );
    if config.configured_minimum_bytes > effective_maximum_bytes
        || !config
            .configured_minimum_bytes
            .is_multiple_of(geometry.block_size_bytes)
        || geometry.current_bytes > effective_maximum_bytes
    {
        return Err(TargetEstimatorError::InvalidGeometry(
            "minimum or current allocation exceeds the aligned effective maximum",
        ));
    }
    for value in [
        config.physical_reserve_bytes,
        config.commit_reserve_bytes,
        config.safe_floor_physical_reserve_bytes,
        config.safe_floor_commit_reserve_bytes,
        config.downward_hysteresis_bytes,
    ] {
        if !value.is_multiple_of(geometry.block_size_bytes) {
            return Err(TargetEstimatorError::InvalidGeometry(
                "reserves and hysteresis must be block aligned",
            ));
        }
    }
    Ok(effective_maximum_bytes)
}

fn candidate(
    memory: &MemoryTelemetrySnapshot,
    current_bytes: u64,
    fixed_visible_base_bytes: u64,
    physical_reserve_bytes: u64,
    commit_reserve_bytes: u64,
) -> Result<u64, TargetEstimatorError> {
    let physical_used = memory
        .physical_total_bytes
        .checked_sub(memory.physical_available_bytes)
        .ok_or(TargetEstimatorError::ArithmeticOverflow)?;
    let physical_candidate = physical_used
        .checked_add(physical_reserve_bytes)
        .ok_or(TargetEstimatorError::ArithmeticOverflow)?
        .saturating_sub(fixed_visible_base_bytes);
    let commit_headroom = memory
        .commit_limit_bytes
        .checked_sub(memory.commit_total_bytes)
        .ok_or(TargetEstimatorError::ArithmeticOverflow)?;
    let commit_candidate = current_bytes
        .checked_add(commit_reserve_bytes)
        .ok_or(TargetEstimatorError::ArithmeticOverflow)?
        .saturating_sub(commit_headroom);
    Ok(physical_candidate.max(commit_candidate))
}

fn align_down(value: u64, alignment: u64) -> u64 {
    value - (value % alignment)
}

fn align_up(value: u64, alignment: u64) -> Result<u64, TargetEstimatorError> {
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or(TargetEstimatorError::ArithmeticOverflow)
    }
}

fn prune_history(history: &mut VecDeque<CandidateHistoryEntry>, now: u64, window: u64) {
    let boundary = now.saturating_sub(window);
    while history.len() > 1
        && history
            .get(1)
            .is_some_and(|entry| entry.observed_unix_millis <= boundary)
    {
        history.pop_front();
    }
}

fn history_is_ready(
    history: &VecDeque<CandidateHistoryEntry>,
    window: u64,
    maximum_gap: u64,
) -> bool {
    let (Some(first), Some(last)) = (history.front(), history.back()) else {
        return false;
    };
    if last
        .observed_unix_millis
        .saturating_sub(first.observed_unix_millis)
        < window
    {
        return false;
    }
    history.iter().zip(history.iter().skip(1)).all(|(a, b)| {
        b.observed_unix_millis >= a.observed_unix_millis
            && b.observed_unix_millis - a.observed_unix_millis <= maximum_gap
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> TargetPolicyConfig {
        TargetPolicyConfig {
            configured_minimum_bytes: 2 * GIB,
            configured_maximum_bytes: 20 * GIB,
            fixed_visible_base_bytes: 8 * GIB,
            physical_reserve_bytes: 2 * GIB,
            commit_reserve_bytes: 2 * GIB,
            safe_floor_physical_reserve_bytes: GIB,
            safe_floor_commit_reserve_bytes: GIB,
            history_window_millis: 600_000,
            maximum_gap_millis: 60_000,
            downward_hysteresis_bytes: 256 * MIB,
        }
    }

    fn geometry(current: u64) -> TargetGeometry {
        TargetGeometry {
            device_size_bytes: 24 * GIB,
            block_size_bytes: 2 * MIB,
            current_bytes: current,
        }
    }

    fn memory(current: u64, used_dynamic: u64, commit_headroom: u64) -> MemoryTelemetrySnapshot {
        let total = 8 * GIB + current;
        let used = 8 * GIB + used_dynamic;
        MemoryTelemetrySnapshot {
            physical_total_bytes: total,
            physical_available_bytes: total - used,
            memory_load_percent: 50,
            commit_total_bytes: 32 * GIB - commit_headroom,
            commit_limit_bytes: 32 * GIB,
            commit_peak_bytes: 32 * GIB - commit_headroom,
            system_cache_bytes: 0,
            kernel_paged_bytes: 0,
            kernel_nonpaged_bytes: 0,
        }
    }

    fn sample(at: u64, sequence: u64, memory: MemoryTelemetrySnapshot) -> TargetSample {
        TargetSample {
            session_id: "session-a".to_owned(),
            observed_unix_millis: at,
            monotonic_millis: at,
            sequence,
            memory,
        }
    }

    #[test]
    fn calculates_separate_physical_and_commit_candidates() {
        let physical = calculate_instantaneous(
            &config(),
            &memory(8 * GIB, 6 * GIB, 8 * GIB),
            geometry(8 * GIB),
        )
        .expect("physical candidate");
        assert_eq!(physical.desired_now_bytes, 8 * GIB);
        assert_eq!(physical.floor_now_bytes, 7 * GIB);

        let commit = calculate_instantaneous(
            &config(),
            &memory(8 * GIB, GIB, 512 * MIB),
            geometry(8 * GIB),
        )
        .expect("commit candidate");
        assert_eq!(commit.desired_now_bytes, 10 * GIB - 512 * MIB);
        assert_eq!(commit.floor_now_bytes, 9 * GIB - 512 * MIB);
    }

    #[test]
    fn rejects_base_drift_and_effective_maximum_failures() {
        let mut drifted = memory(8 * GIB, 4 * GIB, 4 * GIB);
        drifted.physical_total_bytes += 512 * MIB;
        assert!(matches!(
            calculate_instantaneous(&config(), &drifted, geometry(8 * GIB)),
            Err(TargetEstimatorError::FixedBaseDrift { .. })
        ));
        let too_small = TargetGeometry {
            device_size_bytes: 2 * GIB,
            block_size_bytes: 2 * MIB,
            current_bytes: 2 * GIB,
        };
        assert!(calculate_instantaneous(&config(), &memory(2 * GIB, GIB, GIB), too_small).is_err());
    }

    #[test]
    fn reports_capacity_limited_and_preserves_alignment() {
        let result = calculate_instantaneous(
            &config(),
            &memory(20 * GIB, 20 * GIB, 0),
            geometry(20 * GIB),
        )
        .expect("bounded target");
        assert_eq!(result.effective_maximum_bytes, 20 * GIB);
        assert_eq!(result.desired_now_bytes, 20 * GIB);
        assert!(result.capacity_limited);
    }

    #[test]
    fn grows_immediately_then_lowers_only_after_complete_window() {
        let mut estimator = TargetEstimator::new(config()).expect("valid estimator");
        let initial = estimator
            .estimate(
                sample(1_000_000, 0, memory(8 * GIB, 8 * GIB, GIB)),
                geometry(8 * GIB),
            )
            .expect("growth estimate");
        assert_eq!(initial.desired_bytes, 10 * GIB);
        assert!(!initial.history_ready);
        assert_eq!(initial.safe_floor_bytes, 8 * GIB);

        for sequence in 1..=11 {
            let result = estimator
                .estimate(
                    sample(
                        1_000_000 + sequence * 60_000,
                        sequence,
                        memory(8 * GIB, 2 * GIB, 8 * GIB),
                    ),
                    geometry(8 * GIB),
                )
                .expect("window sample");
            if sequence < 11 {
                assert_eq!(result.desired_bytes, 10 * GIB);
            } else {
                assert!(result.history_ready);
                assert_eq!(result.desired_bytes, 4 * GIB);
                assert_eq!(result.safe_floor_bytes, 3 * GIB);
            }
        }
    }

    #[test]
    fn four_gibibyte_growth_trace_settles_at_two_gibibytes_extra() {
        let mut estimator = TargetEstimator::new(config()).expect("valid estimator");
        let first = estimator
            .estimate(
                sample(1_000_000, 0, memory(8 * GIB, 8 * GIB, 0)),
                geometry(8 * GIB),
            )
            .expect("first growth sample");
        assert_eq!(first.desired_bytes, 10 * GIB);

        let second = estimator
            .estimate(
                sample(1_060_000, 1, memory(10 * GIB, 10 * GIB, 0)),
                geometry(10 * GIB),
            )
            .expect("second growth sample");
        assert_eq!(second.desired_bytes, 12 * GIB);

        for sequence in 2..=12 {
            let result = estimator
                .estimate(
                    sample(
                        1_000_000 + sequence * 60_000,
                        sequence,
                        memory(12 * GIB, 8 * GIB, 8 * GIB),
                    ),
                    geometry(12 * GIB),
                )
                .expect("settling sample");
            if sequence == 12 {
                assert!(result.history_ready);
                assert_eq!(result.desired_bytes, 10 * GIB);
            }
        }
    }

    #[test]
    fn gap_and_new_session_restart_reclaim_warmup() {
        let mut estimator = TargetEstimator::new(config()).expect("valid estimator");
        estimator
            .estimate(
                sample(1_000, 0, memory(8 * GIB, 2 * GIB, 8 * GIB)),
                geometry(8 * GIB),
            )
            .expect("first sample");
        let after_gap = estimator
            .estimate(
                sample(62_000, 1, memory(8 * GIB, 2 * GIB, 8 * GIB)),
                geometry(8 * GIB),
            )
            .expect("gap sample");
        assert!(!after_gap.history_ready);
        let mut restarted = sample(63_000, 0, memory(8 * GIB, 2 * GIB, 8 * GIB));
        restarted.session_id = "session-b".to_owned();
        restarted.monotonic_millis = 1;
        let result = estimator
            .estimate(restarted, geometry(8 * GIB))
            .expect("restart");
        assert!(!result.history_ready);
        assert_eq!(estimator.state().history.len(), 1);
    }

    #[test]
    fn invalid_sample_clears_reclaim_readiness() {
        let mut state = TargetEstimatorState::cold();
        state.desired_bytes = 8 * GIB;
        state.safe_floor_bytes = 4 * GIB;
        state.session_id = "session-a".to_owned();
        state.last_sequence = 2;
        state.last_monotonic_millis = 2;
        state.last_observed_unix_millis = 2;
        state.history.push_back(CandidateHistoryEntry {
            observed_unix_millis: 1,
            desired_now_bytes: 4 * GIB,
            floor_now_bytes: 3 * GIB,
        });
        let mut estimator = TargetEstimator::restore(config(), state).expect("restore");
        assert!(estimator
            .estimate(
                sample(2, 2, memory(8 * GIB, 2 * GIB, 8 * GIB)),
                geometry(8 * GIB)
            )
            .is_err());
        assert!(estimator.state().history.is_empty());
        assert_eq!(
            estimator.state().safe_floor_bytes,
            estimator.state().desired_bytes
        );
    }

    #[test]
    fn detects_candidate_overflow() {
        let mut overflow_config = config();
        overflow_config.commit_reserve_bytes = u64::MAX - (2 * MIB - 1);
        assert_eq!(
            calculate_instantaneous(
                &overflow_config,
                &memory(8 * GIB, 2 * GIB, 8 * GIB),
                geometry(8 * GIB)
            ),
            Err(TargetEstimatorError::ArithmeticOverflow)
        );
    }
}
