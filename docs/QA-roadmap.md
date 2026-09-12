# Unattended single-controller QA roadmap

## Scope and current decision

This roadmap covers one trusted development guest, one explicitly configured
virtio-mem device, and one host controller. The exact VM, alias, service
identities, paths, artifacts, and configuration belong in the current
deployment manifest and qualification evidence, not in this reusable plan.

These gates provide the execution detail for WN1 through WN10 in the
[automatic-resizing release roadmap](roadmap.md). Passing them is the required
single-VM checkpoint before global-controller work can cross a live boundary;
it is not the final multi-VM release decision.

The current decision is **NO-GO for unattended automatic resizing**. The
coherent deployment, production telemetry transport, current attestation, and
native notification collection are complete, but the fixed-headroom
qualification has been paused following the pressure-aware architecture pivot.
The host controller remains in a fail-stop safe hold while notification
workload correlation, shadow-policy, applied behavior, recovery, and endurance
evidence remain incomplete. See
`qa-deployment-manifest.md` for the current inspected deployment delta.

Multi-VM arbitration, untrusted guests, production support, and direct driver
control are outside this gate.

Product construction is tracked separately in `BACKLOG.md`. Recovery,
observability, hermetic global-controller, and release-operability code may be
built while these ordered live gates remain pending. That code does not change
a QA task's status until the corresponding applied gate is run and reviewed.

## Safety invariants

1. Windows publishes measurements only; the host owns policy and actuation.
2. Alias-scoped live libvirt `current` is allocation authority.
3. Ordinary actuation is prohibited while requested and current differ.
4. Every request is aligned, bounded by current configuration, headroom
   checked, freshly attested, journaled, and resolved by live reread.
5. Missing, stale, replayed, malformed, cross-VM, or provenance-free telemetry
   cannot authorize reclaim.
6. Ambiguous or stalled actuation latches durably and is never blindly retried.
7. Automatic reclaim remains default-on; a reviewed explicit pause is allowed.
8. A live run records initial state, explicit time bounds, cleanup, recovery,
   and final state before mutation.
9. Only one controller/device is active in this qualification scope.
10. Missing, unsupported, or unwarmed release-critical pressure signals cannot
    authorize shrink; they are never represented as zero.

## Gates

| Gate | Exit condition |
| --- | --- |
| QA-G0 Safe hold | Unhealthy or incoherent deployment cannot mutate; guest and device health are recorded |
| QA-G1 Coherent deployment | Current hashed artifacts, configuration, identities, ACLs, attestation, and production telemetry transport agree |
| QA-G2 Native pressure evidence | Supported Windows pressure signals are inventoried and semantically validated under resident, commit, cache, and paging workloads |
| QA-G3 Shadow and applied policy | Explainable shadow targets pass review, then bounded growth and conservative reclaim pass with fallback and renewed-pressure behavior |
| QA-G4 Recovery and endurance | Signal degradation, telemetry loss, interruption, ambiguity, cancellation, partial progress, and repeated cycles remain bounded and observable |
| QA-G5 Decision | Support profile, operator response, evidence index, and residual risks are reviewed for an explicit GO or NO-GO |

## Task board

### Baseline and deployment

| ID | Task | Status |
| --- | --- | --- |
| QA-T001 | Establish fail-stop safe hold and confirm unchanged guest/device health | Complete |
| QA-T002 | Record installed/candidate hashes, units, configuration, identities, and deployment delta | Complete |
| QA-T003 | Correct acknowledged durability and SCM identity/registration defects | Complete |
| QA-T004 | Implement the least-privilege production telemetry transport | Complete |
| QA-T005 | Install and verify the current Windows candidate, configuration, ACLs, telemetry, and lifecycle | Complete |
| QA-T006 | Install and verify the current host candidate, instance configuration, and fail-stop service state | Complete |
| QA-T007 | Calibrate deployment-specific visible base and regenerate/review compatibility attestation | Complete |
| QA-T008 | Prove telemetry handoff and replay/session behavior across independent service restarts without actuation | Complete |

### Historical fixed-headroom sequence

| ID | Task | Status |
| --- | --- | --- |
| QA-T009 | Run an explicitly configured resident-memory qualification through the fixed-headroom path | Paused; historical partial evidence only |
| QA-T010 | Run an explicitly configured committed-memory qualification through the same path | Superseded |
| QA-T011 | Prove renewed pressure during an owned pending shrink without lower-request overlap | Superseded as a policy gate; invariant retained |
| QA-T012 | Record an explicit cleanup/recovery result for the captured initial target | Superseded as a policy gate; cleanup rule retained |
| QA-T013–QA-T018 | Prior fixed-headroom recovery and observability sequence | Superseded by QA-T030–QA-T032 |

### Historical endurance and decision

| ID | Task | Status |
| --- | --- | --- |
| QA-T019–QA-T024 | Prior fixed-headroom endurance and decision sequence | Superseded by QA-T033–QA-T035 |

### Windows-native pressure policy

| ID | Task | Status |
| --- | --- | --- |
| QA-T025 | Validate the additive schema, capability, warm-up, fallback, compatibility, and fail-closed contract without actuation | Complete; focused, local, and native Windows gates passed 2026-09-13; no deployment or actuation |
| QA-T026 | Record native support and failure behavior for Windows memory notifications, then correlate their states with distinct memory workloads | Ready; native API capability passed, workload correlation outstanding |
| QA-T027 | Review commit-centred shadow classifications and byte candidates for false growth, missed pressure, cache treatment, capability/warm-up handling, in-flight allocation treatment, continuity, and fallback behavior | Blocked by QA-T026 and TASK-038 |
| QA-T028 | Prove pressure-aware bounded growth under low-memory, commit-stress, and paging scenarios with reclaim disabled | Blocked by QA-T027 and TASK-039 |
| QA-T029 | Prove conservative reclaim requires sustained high/healthy evidence and stops or reverses under renewed pressure | Blocked by QA-T028 and TASK-040 |
| QA-T030 | Prove each proposed rate/trend signal adds value, has correct interval/warm-up semantics, and degrades without unsafe shrink | Blocked by QA-T029 and TASK-041 |
| QA-T031 | Run the automated resident, commit-only, cache/standby, modified, paging, burst, steady-state, and recovery workload matrix | Blocked by QA-T030 and TASK-042 |
| QA-T032 | Exercise interruption, ambiguity, partial/no progress, restart, status, operator response, cleanup, and fallback without overlap or replay | Blocked by QA-T031 and TASK-030–TASK-031 |
| QA-T033 | Run reviewed repeated-cycle and endurance plans across growth, reclaim, cache, paging, and signal-degradation cases | Blocked by QA-T032 and TASK-043 |
| QA-T034 | Qualify frozen configuration, defaults, schema migration, deployment, upgrade, downgrade, and rollback | Blocked by QA-T033 and TASK-044 |
| QA-T035 | Publish the support profile and record the final reviewed GO or NO-GO decision with evidence index | Blocked by QA-T034 and TASK-045 |

## Qualification rules

Use `cargo xtask qualification` for automatic-controller workload runs after
the shadow and active pressure fields have been added. Every
run supplies its sizes, safety cap, timings, services, endpoints, paths,
sampling, and initial-growth, reclaim, and renewed-growth acceptance deltas
explicitly and stores the versioned configuration with its evidence. QA-T029
also requires the explicit pending-shrink overlap gate inherited from the
reconciler qualification. Do not promote a prior run's values into a new
profile.

QA-T025 through QA-T027 are measurement/shadow gates and must not actuate
memory. Native notifications, counter availability, rate warm-up, sampling
interval, assessment reason, fallback state, and policy/schema version must be
preserved with their evidence.

QA-T026 records notification semantics without inventing a numeric pressure
score. QA-T030 derives any proposed reusable-memory or paging-rate threshold
from supported guest and workload evidence. Diagnostic examples are not
product defaults and cannot authorize actuation before review.

Use `cargo xtask live-resize` only for a separately scoped reversible manual
test. It is not a substitute for observing the automatic controller through
the production telemetry path.

Endurance duration and cycle count are selected only after the failure modes,
observation frequency, and confidence target are written down. A short smoke
run cannot satisfy a longer plan, but this roadmap does not encode an arbitrary
universal duration.

## Final decision checklist

- [ ] Current hashed Windows and host artifacts are installed.
- [ ] Production raw telemetry and least-privilege transport/ACLs are verified.
- [ ] Calibration and attestation match fresh live state.
- [ ] The device starts converged and exactly one controller is active.
- [ ] Native pressure capabilities and failure states are recorded for the
      supported Windows build.
- [ ] Resident, commit, cache/standby, and paging workloads validate signal
      semantics and shadow classifications.
- [ ] Pressure-aware growth and reclaim runs meet their recorded criteria.
- [ ] Falling demand, reclaim/constrained handling, fallback, and renewed
      pressure are correctly observed.
- [ ] Loss, interruption, ambiguity, and partial/no-progress recovery gates pass.
- [ ] No request is replayed or overlapped.
- [ ] Health, last success, restart count, latch reason, and resize events are
      monitorable against documented alert criteria.
- [ ] The reviewed repeated-cycle and soak plans pass.
- [ ] Final state and every warning/operator action are explained.
- [ ] Pause, recovery, upgrade, and rollback procedures are rehearsed.

Any unchecked item keeps the result at NO-GO. Store raw evidence under the
ignored artifact hierarchy and summarize only durable outcomes in project
status and task documents.
