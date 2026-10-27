//! Windows-owned memory policy. The guest turns its own telemetry and the
//! host's published device state into explicit add/remove requests and keeps
//! exactly one unresolved request, durably, until the host publishes its
//! correlated terminal result. The host never estimates guest demand.

use serde::{Deserialize, Serialize};

use crate::memory_request::{
    HostMemoryStatus, MemoryOperation, MemoryOutcome, MemoryRequest, MemoryResult, RequestIdentity,
    MAX_REQUEST_LIFETIME_MILLIS, MEMORY_REQUEST_VERSION,
};
use crate::{
    MemoryResourceNotificationState, TargetEstimate, TargetEstimator, TargetGeometry,
    TargetPolicyConfig, TargetSample, VirtioMemState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestMemoryPolicy {
    pub target: TargetPolicyConfig,
    #[serde(with = "crate::units::byte_size")]
    /// Minimum extra RAM requested for native Low pressure when numeric
    /// demand does not already call for a larger increase. Not a growth cap.
    pub grow_step_bytes: u64,
    #[serde(with = "crate::units::byte_size")]
    pub shrink_step_bytes: u64,
    /// Host status further than this from the guest clock, in either
    /// direction, never supplies geometry for a new request.
    pub host_status_max_age_millis: u64,
    pub request_lifetime_millis: u64,
    /// After a removal that falls short, removals pause for one history
    /// window; each removal in a row that returns nothing doubles the pause,
    /// up to this many windows. viomem cannot evacuate the oldest plugged
    /// memory, so an idle guest would otherwise ask for it every window.
    pub max_removal_pause_windows: u32,
}

impl GuestMemoryPolicy {
    pub fn validate(&self) -> Result<(), String> {
        self.target.validate().map_err(|error| error.to_string())?;
        if self.grow_step_bytes == 0
            || self.shrink_step_bytes == 0
            || self.host_status_max_age_millis == 0
        {
            return Err("guest policy steps and status age must be positive".to_owned());
        }
        if self.max_removal_pause_windows == 0 {
            return Err("max_removal_pause_windows must be at least 1".to_owned());
        }
        if self.request_lifetime_millis == 0
            || self.request_lifetime_millis > MAX_REQUEST_LIFETIME_MILLIS
        {
            return Err(format!(
                "request lifetime must be 1..={MAX_REQUEST_LIFETIME_MILLIS} milliseconds"
            ));
        }
        Ok(())
    }
}

/// The next delta for a converged device, or `None` to hold. Growth follows
/// demand immediately; reclaim waits for qualified history and never goes
/// below the safe floor.
pub fn plan_request(
    estimate: &TargetEstimate,
    device: VirtioMemState,
    policy: &GuestMemoryPolicy,
) -> Result<Option<(MemoryOperation, u64)>, String> {
    device.validate().map_err(|error| error.to_string())?;
    if !policy
        .grow_step_bytes
        .is_multiple_of(device.block_size_bytes)
        || !policy
            .shrink_step_bytes
            .is_multiple_of(device.block_size_bytes)
    {
        return Err("guest policy steps must be multiples of the device block size".to_owned());
    }
    if device.requested_bytes != device.current_bytes {
        return Ok(None);
    }
    let current = device.current_bytes;
    if current < estimate.desired_bytes {
        return Ok(Some((
            MemoryOperation::Add,
            estimate.desired_bytes - current,
        )));
    }
    if current > estimate.desired_bytes && estimate.history_ready {
        let target = current
            .saturating_sub(policy.shrink_step_bytes)
            .max(estimate.desired_bytes)
            .max(estimate.safe_floor_bytes);
        if target < current {
            return Ok(Some((MemoryOperation::Remove, current - target)));
        }
    }
    Ok(None)
}

/// What the guest retains across service restarts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestRequestState {
    pub pending: Option<MemoryRequest>,
    /// Issuance watermark: the host refuses requests that do not advance it.
    pub last_issued_unix_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestDecision {
    /// The unresolved request to publish with this sample, if any.
    pub request: Option<MemoryRequest>,
    /// The durable state changed and must be persisted before publishing.
    pub persist: bool,
    /// The pending request this poll resolved.
    pub resolved: Option<MemoryResult>,
}

#[derive(Debug, Clone)]
pub struct GuestRequester {
    policy: GuestMemoryPolicy,
    estimator: TargetEstimator,
    state: GuestRequestState,
    vm_name: String,
    service_name: String,
    session_id: String,
    next_sequence: u64,
    /// Monotonic time before which no removal is requested, after the guest
    /// could not return all that a previous removal asked for.
    remove_paused_until_millis: Option<u64>,
    /// Removals in a row that returned nothing, and the allocation they left.
    /// Any other allocation means the layout changed and the streak ends.
    zero_removals: u32,
    zero_removal_at_bytes: Option<u64>,
}

impl GuestRequester {
    pub fn new(
        policy: GuestMemoryPolicy,
        vm_name: &str,
        service_name: &str,
        session_id: &str,
        state: GuestRequestState,
    ) -> Result<Self, String> {
        policy.validate()?;
        for value in [vm_name, service_name, session_id] {
            if value.trim().is_empty()
                || value.len() > 128
                || !value.is_ascii()
                || value.chars().any(char::is_control)
            {
                return Err(
                    "guest VM, service and session identity must be printable ASCII of at most 128 bytes"
                        .to_owned(),
                );
            }
        }
        if let Some(pending) = &state.pending {
            pending.validate()?;
            if pending.identity.vm_name != vm_name || pending.identity.service_name != service_name
            {
                return Err("retained memory request belongs to another VM or service".to_owned());
            }
            if pending.issued_unix_millis > state.last_issued_unix_millis {
                return Err("retained memory request is newer than its watermark".to_owned());
            }
        }
        Ok(Self {
            estimator: TargetEstimator::new(policy.target).map_err(|error| error.to_string())?,
            policy,
            state,
            vm_name: vm_name.to_owned(),
            service_name: service_name.to_owned(),
            session_id: session_id.to_owned(),
            next_sequence: 0,
            remove_paused_until_millis: None,
            zero_removals: 0,
            zero_removal_at_bytes: None,
        })
    }

    pub fn set_ram_limits(&mut self, minimum: u64, maximum: u64) -> Result<(), String> {
        self.estimator.set_ram_limits(minimum, maximum)
    }

    pub fn invalidate_history(&mut self) {
        self.estimator.invalidate_history();
    }

    pub fn state(&self) -> &GuestRequestState {
        &self.state
    }

    pub fn vm_name(&self) -> &str {
        &self.vm_name
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// One poll. Persist `state()` when `persist` is set before publishing.
    pub fn poll(
        &mut self,
        sample: TargetSample,
        status: Option<&HostMemoryStatus>,
    ) -> Result<GuestDecision, String> {
        let native_low = sample.native_pressure == Some(MemoryResourceNotificationState::Low);
        let now = sample.observed_unix_millis;
        let monotonic = sample.monotonic_millis;
        let status = status.filter(|status| {
            status
                .validate_for(&self.vm_name, &self.service_name)
                .is_ok()
        });
        let fresh =
            status.filter(|status| status.fresh_at(now, self.policy.host_status_max_age_millis));
        // Estimate on every fresh status so reclaim history stays continuous
        // while a request is outstanding. A device converging on our own
        // request gives no geometry for that sample, but only missing/stale
        // status or a non-High observation discards the window; otherwise
        // every bounded removal step would wait a full window again. A
        // convergence longer than the maximum gap still restarts it.
        let converged = fresh.filter(|status| status.requested_bytes == status.current_bytes);
        if let (Some(status), Some(at)) = (converged, self.zero_removal_at_bytes) {
            if status.current_bytes != at {
                self.zero_removals = 0;
                self.zero_removal_at_bytes = None;
                self.remove_paused_until_millis = None;
            }
        }
        if fresh.is_none()
            || (converged.is_none()
                && sample.native_pressure != Some(MemoryResourceNotificationState::High))
        {
            self.estimator.invalidate_history();
        }
        let estimate = converged
            .map(|status| {
                self.estimator.estimate(
                    sample,
                    TargetGeometry {
                        device_size_bytes: status.device_size_bytes,
                        block_size_bytes: status.block_size_bytes,
                        current_bytes: status.current_bytes,
                    },
                )
            })
            .transpose()
            .map_err(|error| error.to_string())?;

        if let Some(pending) = &self.state.pending {
            let resolved = status
                .and_then(|status| status.result.as_ref())
                .filter(|result| &result.request == pending && result.terminal())
                .cloned();
            let Some(resolved) = resolved else {
                return Ok(GuestDecision {
                    request: Some(pending.clone()),
                    persist: false,
                    resolved: None,
                });
            };
            if pending.operation == MemoryOperation::Remove
                && resolved.outcome != MemoryOutcome::Completed
            {
                if resolved.actual_bytes == 0 {
                    self.zero_removals = self.zero_removals.saturating_add(1);
                    self.zero_removal_at_bytes = status.map(|status| status.current_bytes);
                } else {
                    self.zero_removals = 0;
                    self.zero_removal_at_bytes = None;
                }
                let windows = 1u64
                    .checked_shl(self.zero_removals)
                    .unwrap_or(u64::MAX)
                    .min(u64::from(self.policy.max_removal_pause_windows));
                let pause = self
                    .policy
                    .target
                    .history_window_millis
                    .saturating_mul(windows);
                self.remove_paused_until_millis = Some(monotonic.saturating_add(pause));
            }
            // Reassess with the next sample rather than chaining a new delta
            // onto this resolution.
            self.state.pending = None;
            return Ok(GuestDecision {
                request: None,
                persist: true,
                resolved: Some(resolved),
            });
        }

        let (Some(status), Some(mut estimate)) = (fresh, estimate) else {
            return Ok(Self::hold());
        };
        // Windows' own Low classification is an urgent growth trigger even
        // when operator reserves are smaller than Windows' threshold.
        if native_low {
            estimate.desired_bytes = estimate.desired_bytes.max(
                status
                    .current_bytes
                    .saturating_add(self.policy.grow_step_bytes)
                    .min(estimate.effective_maximum_bytes),
            );
        }
        let Some((operation, bytes)) = plan_request(&estimate, status.device(), &self.policy)?
        else {
            return Ok(Self::hold());
        };
        if operation == MemoryOperation::Remove
            && self
                .remove_paused_until_millis
                .is_some_and(|until| monotonic < until)
        {
            return Ok(Self::hold());
        }
        // After a wall-clock rollback, wait until it passes the watermark.
        if now <= self.state.last_issued_unix_millis {
            return Ok(Self::hold());
        }
        let request = MemoryRequest {
            version: MEMORY_REQUEST_VERSION,
            identity: RequestIdentity {
                vm_name: self.vm_name.clone(),
                service_name: self.service_name.clone(),
                session_id: self.session_id.clone(),
                sequence: self.next_sequence,
            },
            operation,
            bytes,
            issued_unix_millis: now,
            expires_unix_millis: now
                .checked_add(self.policy.request_lifetime_millis)
                .ok_or("memory request expiry overflowed")?,
        };
        request.validate()?;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or("memory request sequence exhausted")?;
        self.state = GuestRequestState {
            pending: Some(request.clone()),
            last_issued_unix_millis: now,
        };
        Ok(GuestDecision {
            request: Some(request),
            persist: true,
            resolved: None,
        })
    }

    fn hold() -> GuestDecision {
        GuestDecision {
            request: None,
            persist: false,
            resolved: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryTelemetrySnapshot;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn policy() -> GuestMemoryPolicy {
        GuestMemoryPolicy {
            target: TargetPolicyConfig {
                configured_minimum_bytes: 0,
                physical_reserve_bytes: 2 * GIB,
                commit_reserve_bytes: 2 * GIB,
                safe_floor_physical_reserve_bytes: GIB,
                safe_floor_commit_reserve_bytes: GIB,
                history_window_millis: 60_000,
                maximum_gap_millis: 30_000,
                downward_hysteresis_bytes: 256 * MIB,
            },
            grow_step_bytes: GIB,
            shrink_step_bytes: 256 * MIB,
            host_status_max_age_millis: 30_000,
            request_lifetime_millis: 20_000,
            max_removal_pause_windows: 16,
        }
    }

    fn sample(second: u64, available: u64, physical_total: u64) -> TargetSample {
        TargetSample {
            session_id: "session".to_owned(),
            observed_unix_millis: 1_000_000 + second * 1000,
            monotonic_millis: 1 + second * 1000,
            sequence: second,
            native_pressure: Some(MemoryResourceNotificationState::High),
            memory: MemoryTelemetrySnapshot {
                physical_total_bytes: physical_total,
                physical_available_bytes: available,
                memory_load_percent: 50,
                commit_total_bytes: GIB,
                commit_limit_bytes: physical_total,
                commit_peak_bytes: GIB,
                system_cache_bytes: 0,
                kernel_paged_bytes: 0,
                kernel_nonpaged_bytes: 0,
            },
        }
    }

    fn status(second: u64, current: u64, result: Option<MemoryResult>) -> HostMemoryStatus {
        HostMemoryStatus {
            version: MEMORY_REQUEST_VERSION,
            vm_name: "guest".to_owned(),
            service_name: "service".to_owned(),
            device_alias: "memory0".to_owned(),
            observed_unix_millis: 1_000_000 + second * 1000,
            device_size_bytes: 32 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: current,
            current_bytes: current,
            result,
        }
    }

    fn requester(state: GuestRequestState) -> GuestRequester {
        GuestRequester::new(policy(), "guest", "service", "session", state).unwrap()
    }

    #[test]
    fn pressure_issues_one_durable_add_and_retransmits_it_unchanged() {
        let mut guest = requester(GuestRequestState::default());
        // 4 GiB fixed + 4 GiB device, nothing available: demand needs growth.
        let first = guest
            .poll(sample(1, 0, 8 * GIB), Some(&status(1, 4 * GIB, None)))
            .unwrap();
        let request = first.request.clone().expect("growth request");
        assert!(first.persist);
        assert_eq!(request.operation, MemoryOperation::Add);
        assert_eq!(request.bytes, 2 * GIB);
        assert_eq!(guest.state().pending.as_ref(), Some(&request));

        // No result yet, then a stale status: retransmit the same identity.
        for (second, status) in [(2, Some(status(2, 4 * GIB, None))), (3, None)] {
            let retry = guest
                .poll(sample(second, 0, 8 * GIB), status.as_ref())
                .unwrap();
            assert_eq!(retry.request.as_ref(), Some(&request));
            assert!(!retry.persist);
        }

        // A restarted service keeps the identity from its durable state.
        let mut restarted = GuestRequester::new(
            policy(),
            "guest",
            "service",
            "new-session",
            guest.state().clone(),
        )
        .unwrap();
        let retry = restarted
            .poll(sample(4, 0, 8 * GIB), Some(&status(4, 4 * GIB, None)))
            .unwrap();
        assert_eq!(retry.request, Some(request));
    }

    #[test]
    fn only_a_matching_terminal_result_resolves_and_the_next_poll_reassesses() {
        let mut guest = requester(GuestRequestState::default());
        let request = guest
            .poll(sample(1, 0, 8 * GIB), Some(&status(1, 4 * GIB, None)))
            .unwrap()
            .request
            .unwrap();
        let mut foreign = request.clone();
        foreign.identity.sequence += 1;
        let uncertain = MemoryResult {
            request: request.clone(),
            outcome: MemoryOutcome::Uncertain,
            actual_bytes: 0,
            observed_unix_millis: 1_002_000,
            reason: Some("command outcome unknown".to_owned()),
        };
        for (second, result) in [
            (
                2,
                MemoryResult::rejected(foreign, 1_002_000, "other".to_owned()),
            ),
            (3, uncertain),
        ] {
            let held = guest
                .poll(
                    sample(second, 0, 8 * GIB),
                    Some(&status(second, 4 * GIB, Some(result))),
                )
                .unwrap();
            assert_eq!(held.request.as_ref(), Some(&request));
        }

        let partial = MemoryResult {
            request: request.clone(),
            outcome: MemoryOutcome::Partial,
            actual_bytes: 512 * MIB,
            observed_unix_millis: 1_003_000,
            reason: Some("pool limited".to_owned()),
        };
        let resolved = guest
            .poll(
                sample(4, 0, 8 * GIB + 512 * MIB),
                Some(&status(4, 4 * GIB + 512 * MIB, Some(partial.clone()))),
            )
            .unwrap();
        assert_eq!(resolved.resolved, Some(partial.clone()));
        assert!(resolved.persist && resolved.request.is_none());
        assert_eq!(guest.state().pending, None);

        let next = guest
            .poll(
                sample(5, 0, 8 * GIB + 512 * MIB),
                Some(&status(5, 4 * GIB + 512 * MIB, Some(partial))),
            )
            .unwrap()
            .request
            .expect("renewed pressure issues a new request");
        assert_eq!(next.identity.sequence, request.identity.sequence + 1);
        assert!(next.issued_unix_millis > request.issued_unix_millis);
    }

    #[test]
    fn historical_result_from_smaller_blocks_does_not_block_pressure_growth() {
        let mut guest = requester(GuestRequestState::default());
        guest.set_ram_limits(6 * GIB, 27 * GIB).unwrap();
        let old_request = MemoryRequest {
            version: MEMORY_REQUEST_VERSION,
            identity: RequestIdentity {
                vm_name: "guest".to_owned(),
                service_name: "service".to_owned(),
                session_id: "previous-boot".to_owned(),
                sequence: 0,
            },
            operation: MemoryOperation::Remove,
            bytes: 1068 * MIB,
            issued_unix_millis: 900_000,
            expires_unix_millis: 920_000,
        };
        let mut live = status(
            1,
            GIB,
            Some(MemoryResult {
                request: old_request,
                outcome: MemoryOutcome::Partial,
                actual_bytes: 20 * MIB,
                observed_unix_millis: 919_000,
                reason: Some("guest returned only part of the removal".to_owned()),
            }),
        );
        live.block_size_bytes = 8 * MIB;
        live.device_size_bytes = 20 * GIB;
        // Windows is 98% full. Its native signal need not be Low for numeric
        // reserve pressure to request growth.
        let decision = guest
            .poll(sample(1, 180 * MIB, 9 * GIB), Some(&live))
            .unwrap();
        let request = decision
            .request
            .expect("growth despite a historical 2 MiB-block result");
        assert_eq!(request.operation, MemoryOperation::Add);
        assert_eq!(request.bytes, 1872 * MIB);
        assert!(request.bytes.is_multiple_of(live.block_size_bytes));

        // Current geometry and historical result identity remain validated.
        live.current_bytes += 2 * MIB;
        assert!(live.validate_for("guest", "service").is_err());
        live.current_bytes = GIB;
        live.result.as_mut().unwrap().request.identity.vm_name = "foreign".to_owned();
        assert!(live.validate_for("guest", "service").is_err());
    }

    #[test]
    fn shared_policy_reclaims_after_ten_seconds_of_continuous_high_memory() {
        let mut guest = GuestRequester::new(
            crate::settings::GlobalSettings::default().guest_policy(),
            "guest",
            "service",
            "session",
            GuestRequestState::default(),
        )
        .unwrap();
        for second in 0..10 {
            assert!(guest
                .poll(
                    sample(second, 8 * GIB, 12 * GIB),
                    Some(&status(second, 4 * GIB, None))
                )
                .unwrap()
                .request
                .is_none());
        }
        let request = guest
            .poll(
                sample(10, 8 * GIB, 12 * GIB),
                Some(&status(10, 4 * GIB, None)),
            )
            .unwrap()
            .request
            .expect("ten seconds qualify reclaim");
        assert_eq!(request.operation, MemoryOperation::Remove);
        assert_eq!(request.bytes, 3 * GIB);
    }

    #[test]
    fn stale_or_foreign_status_never_supplies_geometry() {
        let mut guest = requester(GuestRequestState::default());
        let mut foreign = status(1, 4 * GIB, None);
        foreign.vm_name = "other".to_owned();
        for status in [status(100, 4 * GIB, None), foreign] {
            assert_eq!(
                guest.poll(sample(1, 0, 8 * GIB), Some(&status)).unwrap(),
                GuestRequester::hold()
            );
        }
    }

    #[test]
    fn reclaim_waits_for_history_and_pauses_after_a_short_removal() {
        let mut guest = requester(GuestRequestState::default());
        // 4 GiB fixed + 8 GiB device with 10 GiB available: demand is low.
        let mut second = 1;
        let request = loop {
            let decision = guest
                .poll(
                    sample(second, 10 * GIB, 12 * GIB),
                    Some(&status(second, 8 * GIB, None)),
                )
                .unwrap();
            if let Some(request) = decision.request {
                break request;
            }
            assert!(second < 120, "reclaim never became ready");
            second += 10;
        };
        assert!(second > 60, "reclaim must wait for the history window");
        assert_eq!(request.operation, MemoryOperation::Remove);
        assert_eq!(request.bytes, 256 * MIB);

        second += 1;
        let short = MemoryResult {
            request,
            outcome: MemoryOutcome::Partial,
            actual_bytes: 128 * MIB,
            observed_unix_millis: 1_000_000 + second * 1000,
            reason: None,
        };
        let current = 8 * GIB - 128 * MIB;
        guest
            .poll(
                sample(second, 10 * GIB, 12 * GIB - 128 * MIB),
                Some(&status(second, current, Some(short))),
            )
            .unwrap();
        second += 10;
        assert_eq!(
            guest
                .poll(
                    sample(second, 10 * GIB, 12 * GIB - 128 * MIB),
                    Some(&status(second, current, None))
                )
                .unwrap(),
            GuestRequester::hold()
        );
    }

    #[test]
    fn completed_removals_continue_without_rewaiting_the_window() {
        let mut guest = requester(GuestRequestState::default());
        // 4 GiB fixed + 8 GiB device with 10 GiB available: demand is low.
        let mut second = 1;
        let mut current = 8 * GIB;
        let first = loop {
            if let Some(request) = guest
                .poll(
                    sample(second, 10 * GIB, 4 * GIB + current),
                    Some(&status(second, current, None)),
                )
                .unwrap()
                .request
            {
                break request;
            }
            second += 5;
        };
        assert_eq!(first.operation, MemoryOperation::Remove);

        let mut request = first;
        for _ in 0..3 {
            // The device converges on the removal: High throughout.
            second += 5;
            let mut converging = status(second, current, None);
            converging.requested_bytes = current - request.bytes;
            let held = guest
                .poll(
                    sample(second, 10 * GIB, 4 * GIB + current),
                    Some(&converging),
                )
                .unwrap();
            assert_eq!(held.request.as_ref(), Some(&request));

            second += 5;
            current -= request.bytes;
            let done = MemoryResult {
                request: request.clone(),
                outcome: MemoryOutcome::Completed,
                actual_bytes: request.bytes,
                observed_unix_millis: 1_000_000 + second * 1000,
                reason: None,
            };
            let resolved = guest
                .poll(
                    sample(second, 10 * GIB - request.bytes, 4 * GIB + current),
                    Some(&status(second, current, Some(done.clone()))),
                )
                .unwrap();
            assert_eq!(resolved.resolved, Some(done.clone()));

            // The very next converged poll issues the following step.
            second += 5;
            request = guest
                .poll(
                    sample(second, 10 * GIB - request.bytes, 4 * GIB + current),
                    Some(&status(second, current, Some(done))),
                )
                .unwrap()
                .request
                .expect("next removal step without another history window");
            assert_eq!(request.operation, MemoryOperation::Remove);
        }

        // A non-High observation while converging still restarts the window.
        second += 5;
        let mut converging = status(second, current, None);
        converging.requested_bytes = current - request.bytes;
        let mut neutral = sample(second, 10 * GIB, 4 * GIB + current);
        neutral.native_pressure = Some(MemoryResourceNotificationState::Neutral);
        guest.poll(neutral, Some(&converging)).unwrap();
        second += 5;
        current -= request.bytes;
        let done = MemoryResult {
            request: request.clone(),
            outcome: MemoryOutcome::Completed,
            actual_bytes: request.bytes,
            observed_unix_millis: 1_000_000 + second * 1000,
            reason: None,
        };
        guest
            .poll(
                sample(second, 10 * GIB, 4 * GIB + current),
                Some(&status(second, current, Some(done.clone()))),
            )
            .unwrap();
        second += 5;
        assert_eq!(
            guest
                .poll(
                    sample(second, 10 * GIB, 4 * GIB + current),
                    Some(&status(second, current, Some(done)))
                )
                .unwrap(),
            GuestRequester::hold()
        );
    }

    #[test]
    fn zero_byte_removals_back_off_until_the_allocation_changes() {
        // 4 GiB fixed + 8 GiB device with 10 GiB available: demand is low.
        let mut guest = requester(GuestRequestState::default());
        let window = policy().target.history_window_millis / 1000;
        let current = 8 * GIB;
        let mut second = 1;
        // Seconds between a zero-byte result and the next removal request.
        let mut gaps = Vec::new();
        let mut resolved_at = None;
        while gaps.len() < 6 {
            assert!(second < 200 * window, "removal never retried");
            let decision = guest
                .poll(
                    sample(second, 10 * GIB, 4 * GIB + current),
                    Some(&status(second, current, None)),
                )
                .unwrap();
            if let Some(request) = decision.request {
                if let Some(at) = resolved_at.take() {
                    gaps.push(second - at);
                }
                second += 1;
                let nothing = MemoryResult {
                    request,
                    outcome: MemoryOutcome::Partial,
                    actual_bytes: 0,
                    observed_unix_millis: 1_000_000 + second * 1000,
                    reason: None,
                };
                guest
                    .poll(
                        sample(second, 10 * GIB, 4 * GIB + current),
                        Some(&status(second, current, Some(nothing))),
                    )
                    .unwrap();
                resolved_at = Some(second);
            }
            second += 5;
        }
        // 2, 4, 8, then 16 windows (capped), each within one poll interval.
        for (gap, windows) in gaps.iter().zip([2, 4, 8, 16, 16, 16]) {
            assert!(
                (windows * window..windows * window + 6).contains(gap),
                "gap {gap}s, expected {windows} windows"
            );
        }

        // Growth since the last zero-byte removal ends the streak and pause.
        let mut guest = requester(GuestRequestState::default());
        let mut second = 1;
        let request = loop {
            if let Some(request) = guest
                .poll(
                    sample(second, 10 * GIB, 4 * GIB + current),
                    Some(&status(second, current, None)),
                )
                .unwrap()
                .request
            {
                break request;
            }
            second += 5;
        };
        second += 1;
        let nothing = MemoryResult {
            request,
            outcome: MemoryOutcome::Partial,
            actual_bytes: 0,
            observed_unix_millis: 1_000_000 + second * 1000,
            reason: None,
        };
        guest
            .poll(
                sample(second, 10 * GIB, 4 * GIB + current),
                Some(&status(second, current, Some(nothing))),
            )
            .unwrap();
        let grown = current + GIB;
        let start = second;
        let request = loop {
            second += 5;
            assert!(
                second - start < 2 * window,
                "growth did not clear the pause"
            );
            if let Some(request) = guest
                .poll(
                    sample(second, 11 * GIB, 4 * GIB + grown),
                    Some(&status(second, grown, None)),
                )
                .unwrap()
                .request
            {
                break request;
            }
        };
        assert_eq!(request.operation, MemoryOperation::Remove);
    }

    #[test]
    fn plan_requires_converged_aligned_state() {
        let estimate = TargetEstimate {
            desired_bytes: 6 * GIB,
            safe_floor_bytes: 6 * GIB,
            desired_now_bytes: 6 * GIB,
            floor_now_bytes: 6 * GIB,
            effective_maximum_bytes: 32 * GIB,
            history_ready: false,
            capacity_limited: false,
        };
        let mut device = status(1, 4 * GIB, None).device();
        assert_eq!(
            plan_request(&estimate, device, &policy()).unwrap(),
            Some((MemoryOperation::Add, 2 * GIB))
        );
        device.requested_bytes = 5 * GIB;
        assert_eq!(plan_request(&estimate, device, &policy()).unwrap(), None);
        let mut unaligned = policy();
        unaligned.grow_step_bytes = GIB + MIB;
        assert!(plan_request(&estimate, status(1, 4 * GIB, None).device(), &unaligned).is_err());
    }
    #[test]
    fn native_low_grows_even_with_ample_numeric_reserves() {
        let mut requester = requester(GuestRequestState::default());
        let mut observation = sample(1, 6 * GIB, 8 * GIB);
        observation.native_pressure = Some(MemoryResourceNotificationState::Low);
        let decision = requester
            .poll(observation, Some(&status(1, 4 * GIB, None)))
            .unwrap();
        let request = decision.request.unwrap();
        assert_eq!(request.operation, MemoryOperation::Add);
        assert_eq!(request.bytes, policy().grow_step_bytes);
    }

    #[test]
    fn native_low_at_capacity_holds_without_overflow_or_unaligned_request() {
        let mut requester = requester(GuestRequestState::default());
        let mut observation = sample(1, 8 * GIB, 35 * GIB);
        observation.native_pressure = Some(MemoryResourceNotificationState::Low);
        assert!(requester
            .poll(observation, Some(&status(1, 32 * GIB, None)))
            .unwrap()
            .request
            .is_none());
    }

    #[test]
    fn unknown_notifications_still_allow_commit_emergency_growth() {
        let mut requester = requester(GuestRequestState::default());
        let mut observation = sample(1, 6 * GIB, 8 * GIB);
        observation.native_pressure = None;
        observation.memory.commit_total_bytes = 8 * GIB;
        observation.memory.commit_peak_bytes = 8 * GIB;
        let request = requester
            .poll(observation, Some(&status(1, 4 * GIB, None)))
            .unwrap()
            .request
            .unwrap();
        assert_eq!(request.operation, MemoryOperation::Add);
    }

    #[test]
    fn neutral_and_unknown_never_release_despite_sustained_numeric_surplus() {
        for native in [None, Some(MemoryResourceNotificationState::Neutral)] {
            let mut requester = requester(GuestRequestState::default());
            for second in (1..=121).step_by(10) {
                let mut observation = sample(second, 10 * GIB, 12 * GIB);
                observation.native_pressure = native;
                assert!(requester
                    .poll(observation, Some(&status(second, 8 * GIB, None)))
                    .unwrap()
                    .request
                    .is_none());
            }
        }
    }

    #[test]
    fn low_cannot_supersede_an_unresolved_request() {
        let mut requester = requester(GuestRequestState::default());
        let first = requester
            .poll(sample(1, 0, 8 * GIB), Some(&status(1, 4 * GIB, None)))
            .unwrap()
            .request
            .unwrap();
        let mut observation = sample(2, 0, 8 * GIB);
        observation.native_pressure = Some(MemoryResourceNotificationState::Low);
        assert_eq!(
            requester
                .poll(observation, Some(&status(2, 4 * GIB, None)))
                .unwrap()
                .request,
            Some(first)
        );
    }
}
