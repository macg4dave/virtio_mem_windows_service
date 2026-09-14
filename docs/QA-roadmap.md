# Host VM RAM manager QA roadmap

## Scope and current decision

The current WN gates cover one trusted development guest, one explicitly
configured virtio-mem device, and one host controller. HPM gates extend that
evidence to one host-wide configured VM RAM pool, multiple explicit members,
priority arbitration, reclaim-for-transfer, and more than one guest-demand
provider. Exact VM, alias, service identities, paths, artifacts, pool size,
priorities, and configuration belong in each deployment manifest and
qualification record, not in this reusable plan.

These gates provide the execution detail for WN1-WN10 and HPM0-HPM6 in the
[host VM RAM manager roadmap](roadmap.md). WN10 is the required Windows-provider
and single-VM safety checkpoint. Only HPM6/QA-T042 can make the final multi-VM
host-manager release decision.

The current decision is **NO-GO for unattended automatic resizing**. The
coherent deployment, production telemetry transport, current attestation, and
native notification collection are complete, but the fixed-headroom
qualification has been paused following the pressure-aware architecture pivot.
The host controller remains in a fail-stop safe hold while notification
workload correlation, shadow-policy, applied behavior, recovery, and endurance
evidence remain incomplete. See
`qa-deployment-manifest.md` for the current inspected deployment delta.

Untrusted guests, cluster-wide pooling, and direct driver control remain
outside this roadmap.

Product construction is tracked separately in `BACKLOG.md`. Recovery,
observability, hermetic host-pool, and release-operability code may be
built while these ordered live gates remain pending. That code does not change
a QA task's status until the corresponding applied gate is run and reviewed.

## Safety invariants

1. Guest services publish measurements only; host provider adapters assess
   demand and the host-pool manager owns allocation and actuation.
2. Alias-scoped live libvirt `current` is allocation authority.
3. Ordinary actuation is prohibited while requested and current differ.
4. Every request is aligned, bounded by current configuration, headroom
   checked, freshly attested, journaled, and resolved by live reread.
5. Missing, stale, replayed, malformed, cross-VM, or provenance-free telemetry
   cannot authorize reclaim.
6. Ambiguous or stalled actuation latches durably and is never blindly retried.
7. Reclaim capability remains default-on; a reviewed explicit pause is allowed.
   Only pool contention for unmet higher-priority demand schedules it after
   HPM4.
8. A live run records initial state, explicit time bounds, cleanup, recovery,
   and final state before mutation.
9. During WN qualification only one controller/device is active. HPM
   qualification uses one exclusive host-wide coordinator; per-VM reconcilers
   cannot act outside its grants.
10. Missing, unsupported, or unwarmed release-critical pressure signals cannot
    authorize shrink; they are never represented as zero.
11. Every enabled member's minimum is reserved before discretionary capacity;
    priority never overrides a minimum or qualified safe floor, reserves no
    above-minimum share, and is ignored while all eligible growth fits.
12. Growth is reserved before dispatch. Reclaimed bytes cannot be granted until
    authoritative live `current` confirms release.
13. Donor reclaim requires a named waiting higher-priority recipient and a
    strictly lower-priority shrink-safe donor. Without one, the recipient waits.

## Gates

| Gate | Exit condition |
| --- | --- |
| QA-G0 Safe hold | Unhealthy or incoherent deployment cannot mutate; guest and device health are recorded |
| QA-G1 Coherent deployment | Current hashed artifacts, configuration, identities, ACLs, attestation, and production telemetry transport agree |
| QA-G2 Native pressure evidence | Supported Windows pressure signals are inventoried and semantically validated under resident, commit, cache, and paging workloads |
| QA-G3 Shadow and applied policy | Explainable shadow targets pass review, then bounded growth and conservative reclaim pass with fallback and renewed-pressure behavior |
| QA-G4 Recovery and endurance | Signal degradation, telemetry loss, interruption, ambiguity, cancellation, partial progress, and repeated cycles remain bounded and observable |
| QA-G5 Windows provider decision | Windows support profile and single-VM safety evidence are reviewed for an explicit component GO or NO-GO |
| QA-G6 Pool contracts and ledger | Pool/member/provider contracts and restart-safe byte accounting pass deterministic and fault-injection gates |
| QA-G7 Shadow arbitration | One coordinator explains priority-neutral unconstrained growth, contended grants, waits, and strictly lower-priority donor plans without actuation |
| QA-G8 Pool growth and transfer | All fitting demand may grow; constrained growth is reservation-bounded and donor reclaim is observed before transfer without crossing minimum/safe floor |
| QA-G9 Provider and endurance | Declared homogeneous and mixed-provider pools pass degradation, restart, recovery, and endurance plans |
| QA-G10 Host-manager decision | Frozen configuration, operations, legacy cleanup, evidence index, and residual risks receive an explicit GO or NO-GO |

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
| QA-T026 | Record native support and failure behavior for Windows memory notifications, then correlate their states with distinct memory workloads | Closed for the WN3 checkpoint by operator decision on 2026-09-14; native API and schema-v3 deployment pass, broader workload correlation deferred to QA-T031 and is not release evidence |
| QA-T027 | Review commit-centred shadow classifications and byte candidates for false growth, missed pressure, cache treatment, capability/warm-up handling, in-flight allocation treatment, continuity, and fallback behavior | Closed for the WN3 checkpoint by operator decision on 2026-09-14; schema-v2 fallback/no-actuation evidence passes, broader schema-v3 classification review deferred to QA-T031 and is not release evidence |
| QA-T028 | Prove pressure-aware bounded growth under low-memory, commit-stress, and paging scenarios with reclaim disabled | Blocked by TASK-039 |
| QA-T029 | Prove conservative reclaim requires sustained high/healthy evidence and stops or reverses under renewed pressure | Blocked by QA-T028 and TASK-040 |
| QA-T030 | Prove each proposed rate/trend signal adds value, has correct interval/warm-up semantics, and degrades without unsafe shrink | Blocked by QA-T029 and TASK-041 |
| QA-T031 | Run the automated resident, commit-only, cache/standby, modified, paging, burst, steady-state, and recovery workload matrix | Blocked by QA-T030 and TASK-042 |
| QA-T032 | Exercise interruption, ambiguity, partial/no progress, restart, status, operator response, cleanup, and fallback without overlap or replay | Blocked by QA-T031 and TASK-030–TASK-031 |
| QA-T033 | Run reviewed repeated-cycle and endurance plans across growth, reclaim, cache, paging, and signal-degradation cases | Blocked by QA-T032 and TASK-043 |
| QA-T034 | Qualify frozen configuration, defaults, schema migration, deployment, upgrade, downgrade, and rollback | Blocked by QA-T033 and TASK-044 |
| QA-T035 | Publish the Windows provider/single-VM support profile and record its component GO or NO-GO decision | Blocked by QA-T034 and TASK-045 |

### Host-pool manager

| ID | Task | Status |
| --- | --- | --- |
| QA-T036 | Validate pool/member and OS-neutral demand-report contracts plus durable ledger accounting, corruption, and restart behavior without actuation | Complete; focused core and aggregate local gates passed 2026-09-14; no native, deployment, live, recovery, endurance, or actuation work |
| QA-T037 | Review full-member shadow plans proving all fitting demand may grow regardless of priority, priority applies only under contention, and donor plans require unmet higher-priority demand | Blocked by QA-T036 and TASK-046 |
| QA-T038 | Prove all fitting one-VM and concurrent multi-VM growth is pool-granted without priority withholding, while constrained grants are durable, deterministic, host-headroom checked, and never oversubscribed with reclaim disabled | Blocked by QA-T037, TASK-047, and applicable provider growth evidence |
| QA-T039 | Prove reclaim occurs only for a named unmet higher-priority recipient from a strictly lower-priority shrink-safe donor, remains bounded by minimum/safe floor, cancels on renewed pressure, and is observed before transfer | Blocked by QA-T038, TASK-048, and applicable provider reclaim evidence |
| QA-T040 | Qualify the additional guest-OS provider and homogeneous/mixed-provider contention, degradation, restart, and recovery behavior | Blocked by QA-T039 and TASK-049 |
| QA-T041 | Run reviewed multi-VM repeated-cycle and endurance plans plus configuration, membership, monitoring, upgrade, downgrade, rollback, and legacy-removal checks | Blocked by QA-T040 and TASK-050 |
| QA-T042 | Publish the host-manager support profile and record the final reviewed GO or NO-GO decision with evidence index | Blocked by QA-T041 |

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

The 2026-09-14 WN3 checkpoint decision closes QA-T026 and QA-T027 only as
dependencies for continued construction. The unrun schema-v3 workload breadth,
notification correlation, false-growth/missed-pressure review, and cache/paging
classification move to QA-T031. This deferral is neither a test pass nor
permission for unattended or release use; QA-T028 and later applied gates must
still supply their own required evidence.

QA-T036 and QA-T037 are contract/shadow gates and must not actuate. HPM applied
runs must record the complete pool configuration, every member's host-derived
base allocation and provider report, authoritative `requested`/`current`, ledger
generation, reservations, total-RAM grants, derived device targets, dispatch
order, observed releases, constraints, and final pool accounting. Sampling only
one VM is not valid pool evidence.

For QA-T039, a planned donor decrease is not free capacity. The evidence must
first show a concrete higher-priority recipient whose eligible demand cannot be
met from free capacity, and a strictly lower-priority donor with qualified
reclaimable memory. It must then show the donor's authoritative `current` fall
and the ledger commit before any recipient growth reservation. Equal/higher
priority, missing, or unsafe donors produce a waiting constrained recipient,
not forced reclaim. An underutilised VM alone is not a reason to shrink.

Use `cargo xtask live-resize` only for a separately scoped reversible manual
test. It is not a substitute for observing the automatic controller through
the production telemetry path.

Endurance duration and cycle count are selected only after the failure modes,
observation frequency, and confidence target are written down. A short smoke
run cannot satisfy a longer plan, but this roadmap does not encode an arbitrary
universal duration.

## Final host-manager decision checklist

- [ ] Current hashed Windows and host artifacts are installed.
- [ ] Production raw telemetry and least-privilege transport/ACLs are verified.
- [ ] Calibration and attestation match fresh live state.
- [ ] Every configured device starts in an accounted state and exactly one
      host-wide allocation authority is active.
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
- [ ] The configured pool, complete member set, aligned minimums, priorities,
      provider kinds, and policy fingerprint are recorded and valid.
- [ ] Aggregate non-reclaimable base plus live virtio-mem allocation and
      outstanding growth reservations never exceeds the configured pool.
- [ ] Concurrent demand is arbitrated deterministically and unmet demand is
      reported as constrained.
- [ ] Lower-priority VMs grow to demand when capacity is available; priority
      creates no standing above-minimum reservation and strands no free RAM.
- [ ] Donor reclaim never crosses a minimum or qualified safe floor, and no
      released byte is transferred before live observation.
- [ ] Every donor is strictly lower priority than a named waiting recipient;
      no contention or no safe donor means no reclaim.
- [ ] Every guest-OS/provider combination in the support claim has native,
      workload-correlation, degradation, and recovery evidence.
- [ ] Direct per-VM allocation, independent reclaim, fixed-headroom demand,
      threshold demand, expired schema decoders, and compatibility aliases are
      absent or covered by an explicit time-bounded exception and deletion task.

Any unchecked item keeps the result at NO-GO. Store raw evidence under the
ignored artifact hierarchy and summarize only durable outcomes in project
status and task documents.
