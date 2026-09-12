# Windows-Native Memory Controller Roadmap

## Destination and authority

The destination is a supported controller that sizes Windows guest memory from
Microsoft-supported Windows memory-management evidence instead of primarily
maintaining fixed physical and commit headroom.

Windows remains measurement-only. The Windows service collects and normalises
native APIs, notifications, and counters. The host joins that evidence with
authoritative live virtio-mem allocation and converts it into bounded targets.
Only the host owns capacity allocation and resize actuation.

The following safety mechanisms remain architectural invariants:

- exact VM and device identity;
- fresh, ordered, versioned telemetry with replay protection;
- alias-scoped live `current` as allocation authority;
- distinct `desired`, `requested`, and `current` state;
- configured minimum, maximum, reserve, safety margin, growth step, shrink
  step, hysteresis, and history window;
- device alignment, compatibility attestation, and host headroom;
- no ordinary resize while `requested != current`;
- write-before-command intent, immediate live reread, no blind replay, and
  durable recovery latches; and
- fail-closed shrink whenever required evidence is missing or ambiguous.

The current fixed-headroom estimator remains implemented during migration. It
is a comparison baseline and conservative fallback-growth guard, not the target
release policy. It cannot authorize fallback shrink.

## Decision model

The new controller has three deliberately separate outputs.

### Memory requirement

This is a byte estimate used to construct `desired`. Its first shadow policy is
based on current Windows committed memory plus a configurable demand safety
margin. The host converts the total visible-memory requirement into a
virtio-mem requirement using the configured visible base, then applies the
configured minimum, maximum, device geometry, alignment, and host-capacity
bounds.

The initial shadow formula is fixed by this roadmap:

```text
demand_margin = clamp(
    ceil(commit_total * configured_demand_margin_ratio),
    configured_demand_margin_minimum,
    configured_demand_margin_maximum
)
visible_requirement = commit_total + demand_margin
device_requirement = saturating_sub(
    visible_requirement,
    configured_visible_base
)
requirement_target = align_up(
    clamp(device_requirement, configured_minimum, effective_maximum),
    live_block_size
)
```

Every operation is checked. Invalid bounds, overflow, inconsistent counters,
or alignment failure rejects the assessment. This formula remains shadow-only
until commit-only, resident, cache, and paging qualification demonstrates that
it is a defensible physical-allocation baseline.

Physical available memory, notification state, and paging activity do not get
added to committed memory as independent demand. They qualify the state and
urgency of the requirement.

### Pressure state

This classifies fresh Windows evidence as low, neutral, high, or unavailable.
Windows low/high memory resource notifications are the primary categorical
signal. Commit risk and reusable-memory evidence can corroborate the state but
must not silently replace an unavailable authoritative signal.

Pressure state can block shrink or accelerate movement toward a requirement.
It does not independently manufacture an unbounded byte target.

### Shrink safety

This separately decides whether a lower target may be considered. Reclaim
requires sustained high/healthy Windows evidence over the configured history
window, adequate commit and physical safety margins, a lower qualified memory
requirement, configured hysteresis, and every existing reconciliation gate.

Low, neutral, unavailable, stale, discontinuous, or contradictory evidence
blocks shrink. Paging or hard-page activity may become an additional blocker
only after its rate semantics have passed the trend milestone.

## Signal roles

| Signal or input | Initial role | Must not do |
| --- | --- | --- |
| Windows committed bytes | Calculate the initial quantitative memory requirement | Be treated as resident working-set size without qualification |
| Windows commit limit/headroom | Indicate commit risk, block shrink, and corroborate growth urgency | Be added to physical demand as a second allocation |
| Low memory resource notification | Authoritative low-available-memory state; block shrink and trigger faster bounded growth | Select an exact target size |
| High memory resource notification | Required categorical evidence for reclaim eligibility | Authorize reclaim by itself |
| Neither notification signalled | Neutral hold state | Authorize shrink |
| Available physical memory | Physical safety context and fallback-growth evidence | Serve as the primary release demand formula |
| Standby/free/zero memory | Identify immediately reusable physical memory and distinguish cache from pressure | Be assumed reclaimable without sustained high/healthy evidence |
| Modified memory | Block or delay reclaim when reusable memory is overstated | Directly calculate a larger target initially |
| Page output rate | After qualification, indicate paging pressure, block shrink, and accelerate bounded growth | Directly calculate required bytes |
| Page input or hard-fault rates | After qualification, corroborate pressure when paired with other evidence | Trigger growth alone |
| Memory load, cache, pools, peak commit, compression | Diagnostic and qualification context initially | Acquire release-critical actuation authority without a later contract change |
| Host `current` and `requested` | Account allocation and in-flight work | Be replaced by Windows telemetry |

## Milestone summary

| Milestone | Status | Outcome | Depends on |
| --- | --- | --- | --- |
| WN0 | Complete | Audit and clean up the fixed-headroom controller direction | Existing implementation |
| WN1 | Ready | Define the Windows-native telemetry contract and capability model | WN0 |
| WN2 | Planned | Collect authoritative Windows pressure notifications | WN1 |
| WN3 | Planned | Separate requirement, pressure, and shrink-safety assessment in shadow mode | WN2 |
| WN4 | Planned | Integrate pressure-aware bounded growth | WN3 |
| WN5 | Planned | Integrate shrink blocking and conservative reclaim | WN4 |
| WN6 | Planned | Add only qualified trend and rate evidence | WN5 |
| WN7 | Planned | Qualify realistic Windows memory behavior and automate the workload matrix | WN6 |
| WN8 | Planned | Run unattended repeated-cycle and endurance qualification | WN7 |
| WN9 | Planned | Stabilise configuration, migration, and defaults | WN8 |
| WN10 | Planned | Satisfy the production-readiness and release decision | WN9 |

## WN0 — Audit and controller-direction cleanup

### Goal

Establish the exact behavior of the existing fixed-headroom estimator, preserve
valid safety mechanisms, and remove it as the assumed release sizing model.

### Design changes

- Trace every counter and configuration value that reaches `desired`.
- Classify current values as demand, reserve, diagnostic, allocation, or
  reconciliation state.
- Mark the fixed physical/commit candidates as compatibility behavior.
- Preserve the Windows-measures/host-acts ownership boundary.
- Replace obsolete qualification ordering with this milestone path.

### Likely files and modules

- `windows/src/demand.rs`
- `crates/virtio-mem-core/src/demand.rs`
- `crates/virtio-mem-core/src/target_controller.rs`
- `host/src/target_policy.rs`
- `host/src/runtime.rs`
- `docs/target-controller.md`
- `docs/windows-native-pressure-controller.md`
- project roadmaps, boards, contracts, and status documents

### Tests required

- Existing estimator and reconciler regression tests remain green.
- Documentation/source consistency and whitespace checks pass.
- No native or live result is inferred from source inspection.

### Success criteria

- The implemented formula and unused pressure signals are documented exactly.
- Retained safety logic and redesign targets are explicitly listed.
- Obsolete fixed-headroom qualification is marked historical.
- The next task is measurement-only and cannot resize memory.

### Dependencies

None beyond the existing repository.

### Explicitly not yet

- No new Windows signal collection.
- No schema or target-policy change.
- No live actuation or qualification claim.

## WN1 — Windows-native telemetry contract

### Goal

Define an additive, versioned, allocation-free telemetry contract before
changing collection or policy.

### Design changes

- Define fields for low/high memory-resource notification state, existing
  physical and commit counters, reusable/modified memory detail, and optional
  rate evidence.
- Give every optional signal explicit supported, unavailable, failed, and
  warming states; never encode those states as zero.
- Preserve producer identity, session, sequence, monotonic and wall time,
  provenance, bounds, atomic publication, and durable acknowledgement.
- Define capability negotiation and schema migration behavior.
- Define validation relationships without assigning target authority to the
  Windows service.

### Likely files and modules

- `crates/virtio-mem-core/src/demand.rs`
- `windows/src/demand.rs`
- `windows/src/config.rs`
- `host/src/raw_telemetry.rs`
- `host/src/config.rs`
- `docs/data-model.md`
- `docs/api-contract.md`
- `docs/dependencies.md`

### Tests required

- Serialization round trips and older/newer schema compatibility.
- Missing, unknown, malformed, contradictory, and oversized field cases.
- Capability, warm-up, timestamp, session, sequence, and replay validation.
- Deterministic fixtures for every supported/unavailable/error state.

### Success criteria

- Windows and host crates share one reviewed contract.
- Older telemetry can enter only the explicit fallback path.
- Missing richer evidence cannot authorize shrink.
- Native collection and actuation remain unchanged.

### Dependencies

WN0.

### Explicitly not yet

- No new API calls in the service runtime.
- No pressure classification, target change, or resize.
- No rate counter is required for baseline operation.

## WN2 — Authoritative Windows pressure signals

### Goal

Collect the smallest authoritative Windows-native pressure state and publish it
through the existing telemetry path.

### Design changes

- Add low and high memory resource notification handles through supported
  Windows APIs.
- Integrate create, query or wait, cancellation, error, and handle-close
  behavior with the existing service lifecycle.
- Represent low, high, neutral, and unavailable states explicitly.
- Retain existing physical and commit collection for context.
- Publish capability and warm-up state without changing host decisions.

### Likely files and modules

- `windows/src/demand.rs`
- Windows runtime/service lifecycle modules
- `crates/virtio-mem-core/src/demand.rs`
- `host/src/raw_telemetry.rs`
- native Windows `xtask` verification and fixtures

### Tests required

- Injected API success, failure, cancellation, and cleanup tests.
- State-transition tests for low, neutral, high, and unavailable.
- Native Windows build/test and capability evidence.
- Atomic publication, restart, ordering, and transport regression tests.

### Success criteria

- The supported Windows build publishes trustworthy notification state.
- Failure is observable and fail closed.
- Existing telemetry transport and service shutdown remain bounded.
- The host records but does not act on the new state.

### Dependencies

WN1.

### Explicitly not yet

- No target calculation from notification state.
- No reclaim.
- No performance-rate collection or custom pressure score.

## WN3 — Separate requirement, pressure, and shrink safety

### Goal

Implement three independently testable host assessments and run them beside
the fixed-headroom policy without actuation.

### Design changes

- Add a versioned memory-requirement result using committed bytes plus the
  configured demand safety margin and visible-base conversion.
- Add a pressure-state result led by Windows resource notifications, with
  explicit corroborating and contradictory evidence.
- Add a shrink-safety result whose default is blocked until a complete healthy
  history exists.
- Record reason codes, input identities, confidence/availability state, and
  policy version for every output.
- Join every assessment with live `requested` and `current`; never treat
  in-flight or unavailable allocation as guest slack.
- Emit shadow comparisons against the fixed-headroom candidate.

### Likely files and modules

- new focused assessment module in `crates/virtio-mem-core/src/`
- `crates/virtio-mem-core/src/target_controller.rs`
- `host/src/target_policy.rs`
- `host/src/runtime.rs`
- controller status/evidence types
- `docs/target-controller.md`

### Tests required

- Table-driven tests proving each input affects only its assigned output.
- Checked arithmetic, alignment, min/max, safety-margin, and overflow cases.
- Low/neutral/high/unavailable and contradictory-state tests.
- Cold start, history gaps, producer restart, and in-flight allocation tests.
- Golden shadow-decision fixtures with stable reason codes.

### Success criteria

- Requirement bytes, pressure state, and shrink eligibility are independently
  visible and explainable.
- Notification state cannot invent a byte target.
- Requirement calculation cannot bypass shrink-safety gates.
- Shadow mode cannot reach the resize sink.

### Dependencies

WN2 and the read-only controller status surface.

### Explicitly not yet

- No pressure-aware actuation.
- No shrink under either new or fallback evidence.
- No paging-rate influence.

## WN4 — Pressure-aware growth

### Goal

Allow the new assessment to grow memory safely while keeping reclaim disabled.

### Design changes

- Move toward the qualified memory requirement by the configured growth step.
- Let authoritative low-memory state trigger a configured faster-growth mode,
  still bounded by the requirement, effective maximum, device geometry, and
  host capacity.
- Permit conservative fallback growth from fresh basic counters when richer
  pressure state is unavailable; label it explicitly.
- Preserve journaling, fresh attestation, live reread, no-overlap,
  capacity-limited health, and durable latches.
- Expose normal, urgent, fallback, held, and capacity-limited growth reasons.

The initial movement rule is:

```text
normal_growth_goal = requirement_target
urgent_growth_goal = max(
    requirement_target,
    saturating_add(current, configured_pressure_growth_step)
)
next_target = min(selected_growth_goal, effective_maximum)
```

The reconciler applies the applicable configured growth-step bound. An urgent
step creates bounded relief; it does not claim that Windows recommended that
exact target.

### Likely files and modules

- pressure assessment and target-controller modules
- `crates/virtio-mem-core/src/reconciler.rs`
- `host/src/target_policy.rs`
- `host/src/runtime.rs`
- controller status and qualification evidence

### Tests required

- Immediate and stepped growth under normal and low-memory states.
- Faster-growth bounds and transition back to normal growth.
- Host reserve, maximum, alignment, pending request, and capacity limitation.
- Missing/stale signal fallback and no-fallback cases.
- Restart, ambiguous command, no-replay, and partial-progress regressions.

### Success criteria

- Applied growth follows the new requirement and recorded pressure reason.
- Low-memory state reduces response delay without bypassing any safety gate.
- No scenario produces a lower request.
- Fixed-headroom behavior is no longer the primary growth policy.

### Dependencies

WN3 shadow evidence and the applicable native/deployment preflight.

### Explicitly not yet

- No automatic shrink.
- No rate-driven growth.
- No multi-VM allocation without durable host reservation.

## WN5 — Shrink blocking and conservative reclaim

### Goal

Enable reclaim only when Windows-native evidence shows sustained safety and
prove that renewed pressure stops or reverses it.

### Design changes

- Require high-memory notification state throughout the configured history
  window.
- Require a lower qualified memory requirement, adequate physical and commit
  safety margins, configured hysteresis, and a valid safe floor.
- Treat low, neutral, unavailable, stale, discontinuous, contradictory, or
  warming evidence as a shrink blocker.
- Move down only by the configured shrink step without crossing requirement,
  safe floor, configured minimum, or owned in-flight intent.
- Preserve upward freeze, cancellation, and supersession when pressure returns.
- Disable reclaim in fallback mode.

The lower movement goal is:

```text
reclaim_goal = max(
    requirement_target,
    qualified_safe_floor,
    configured_minimum
)
next_target = max(
    reclaim_goal,
    saturating_sub(current, configured_shrink_step)
)
```

This goal is eligible only after the complete shrink-safety decision passes.

### Likely files and modules

- pressure assessment and target-controller modules
- `crates/virtio-mem-core/src/reconciler.rs`
- `crates/virtio-mem-core/src/shrink_recovery.rs`
- `host/src/runtime.rs`
- policy checkpoint and status/evidence types

### Tests required

- Every shrink blocker independently prevents a lower target.
- Complete versus incomplete history and hysteresis boundaries.
- Bounded reclaim, zero/partial progress, and safe-floor enforcement.
- Renewed low/neutral/unavailable state during pending shrink.
- Cancellation, stale-evidence freeze, restart, latch, and no lower overlap.

### Success criteria

- Reclaim occurs only from a complete, explainable Windows-native evidence set.
- Missing richer evidence always holds or grows; it never shrinks.
- Renewed pressure cannot leave an unowned lower request progressing.
- Default-on reclaim remains subject to all qualification and pause gates.

### Dependencies

WN4 growth behavior and a warmed pressure history.

### Explicitly not yet

- No paging-rate thresholds.
- No cache-specific target arithmetic.
- No broad enablement outside bounded single-VM qualification.

## WN6 — Selective trend and rate evidence

### Goal

Add only Windows performance rates that demonstrate clear incremental value
over notifications and snapshots.

### Design changes

- Add supported performance-counter collection behind explicit capabilities.
- Start with page-output evidence; evaluate page-input and hard-fault rates only
  as corroboration.
- Record raw samples, sample interval, warm-up, reset, discontinuity, and
  formatted rate.
- Use qualified page-output pressure to block shrink and optionally select the
  configured faster-growth mode.
- Keep rates out of byte requirement arithmetic.
- Retain modified memory, cache, pools, memory load, peak commit, and
  compression as diagnostics unless evidence justifies a later role.

### Likely files and modules

- Windows performance-counter collector module
- `windows/src/demand.rs`
- telemetry schema and validation
- pressure assessment/history module
- native capability and qualification tooling

### Tests required

- Two-sample warm-up, interval, reset, wrap, missing-counter, and localization
  behavior.
- Sustained versus transient rate traces.
- Rate discontinuity blocking shrink.
- Evidence that generic page-fault activity alone cannot grow or shrink.
- Native Windows counter availability and semantic correlation.

### Success criteria

- Each enabled rate has measured value beyond the notification/snapshot model.
- Rate collection failure degrades explicitly and blocks shrink safely.
- No unqualified rate becomes a release dependency.
- Sampling cost remains within the configured service budget.

### Dependencies

WN5 provides a safe snapshot-based controller to compare against.

### Explicitly not yet

- No ETW dependency in the control loop.
- No custom composite pressure score.
- No per-process working-set controller.

## WN7 — Realistic Windows behavior qualification

### Goal

Prove signal semantics and controller decisions across workloads that separate
commit, residency, cache, paging, bursts, and idle recovery.

### Design changes

- Extend the bounded Windows workload and qualification evidence schemas.
- Add resident allocation/release, committed-but-not-resident, useful
  file-cache/standby, dirty/modified memory, paging pressure, burst allocation,
  steady-state, and recovery scenarios.
- Correlate workload phase, raw telemetry, requirement, pressure state,
  shrink-safety result, target decision, live device progress, and guest health.
- Use Microsoft tooling as a bounded diagnostic oracle, not resize authority.
- Automate false-growth, missed-pressure, cache-treatment, fallback, and
  cancellation classification.

### Likely files and modules

- `windows/src/bin/virtio-mem-workload.rs`
- `tools/xtask/src/qualification.rs`
- `tools/xtask/src/main.rs`
- shared qualification/evidence types
- `docs/testing.md`
- `docs/QA-roadmap.md`

### Tests required

- Deterministic analyzer fixtures for every workload phase and failure class.
- Native Windows execution of every collector/workload capability.
- Shadow and applied growth/reclaim runs with explicit run configuration.
- Cache-heavy tests proving useful standby memory is not treated as application
  demand or immediate reclaim permission.
- Guest-health, cleanup, and final-state validation.

### Success criteria

- Every signal role is supported by correlated workload evidence.
- Resident, commit-only, cache, and paging cases produce distinguishable
  explanations.
- No test harness becomes a competing resize authority.
- All unsafe or unexplained classifications fail the milestone.

### Dependencies

WN6 and the maintained single-controller qualification workflow.

### Explicitly not yet

- No endurance claim from short functional runs.
- No thresholds promoted from a single workload.
- No multi-VM live actuation.

## WN8 — Unattended qualification and endurance

### Goal

Demonstrate that pressure-aware growth, reclaim, degradation, and recovery stay
safe across repeated cycles and long-running unattended operation.

### Design changes

- Define cycle count, duration, workload schedule, sampling, timeouts, and
  acceptance criteria as explicit run inputs.
- Preserve versioned status, append-only events, metrics, raw telemetry,
  assessments, policy checkpoints, command journals, guest health, cleanup,
  and final state.
- Include service restart, telemetry interruption, counter degradation,
  capacity limitation, partial progress, and renewed-pressure scenarios.
- Make run status resumable and reviewable without chat context.

### Likely files and modules

- `tools/xtask/src/qualification.rs`
- qualification analyzer and evidence types
- controller status and event outputs
- `docs/testing.md`
- operational recovery documentation

### Tests required

- Detached-run lifecycle, crash recovery, artifact integrity, and review tests.
- Repeated growth/reclaim cycles with no overlapping request.
- Signal loss and recovery during converged and pending states.
- Bounded zero/partial progress and command ambiguity.
- An explicitly reviewed unattended run and evidence index.

### Success criteria

- The declared endurance plan completes without unexplained transition,
  duplicate authority, replay, unsafe reclaim, or lost evidence.
- Every warning and intervention is classified.
- Cleanup and final allocation are explicit and independently verified.
- Duration alone is never treated as success.

### Dependencies

WN7 functional qualification and complete observability.

### Explicitly not yet

- No default tuning freeze before endurance evidence is reviewed.
- No production claim from one environment.
- No automatic recovery that weakens explicit latch handling.

## WN9 — Configuration, migration, and defaults

### Goal

Freeze a coherent pressure-policy configuration only after qualification has
identified defensible defaults and failure behavior.

### Design changes

- Version policy configuration and fingerprints.
- Define configurable minimum, maximum, physical reserve, commit reserve,
  demand safety margin, normal and faster growth steps, shrink step,
  hysteresis, history window, pressure thresholds where needed, sample bounds,
  and fallback mode.
- Deprecate fixed physical/commit reserves as primary sizing inputs; retain only
  explicitly named fallback/safety semantics.
- Define upgrade, downgrade, unsupported-signal, and older-schema behavior.
- Reject ambiguous legacy settings instead of silently reinterpreting them.

### Likely files and modules

- `host/src/config.rs`
- Windows configuration modules
- pressure and target policy configuration types
- policy fingerprint/checkpoint migration
- deployment templates and typed deployment tooling
- `docs/api-contract.md`
- `docs/data-model.md`
- `docs/target-controller.md`

### Tests required

- Configuration parsing, validation, boundary, and cross-field tests.
- Policy fingerprint and checkpoint migration tests.
- Upgrade, downgrade, rollback, and unsupported-capability tests.
- Default-on shrink plus fail-closed evidence-gate tests.
- Deployment-template validation with no embedded environment-specific values.

### Success criteria

- Every operational choice has one owner, unit, default policy, and validation
  rule.
- Defaults are justified by qualification evidence, not copied examples.
- Legacy configuration has an explicit migration or hard rejection.
- Installation and rollback preserve safe disabled/inactive behavior until
  preflight passes.

### Dependencies

WN8 evidence review.

### Explicitly not yet

- No hidden compatibility aliases with changed meanings.
- No environment-specific values in repository defaults.
- No release until the exact frozen configuration is requalified.

## WN10 — Production readiness and release decision

### Goal

Accept or reject an immutable candidate for the declared Windows, host,
hypervisor, driver, and trust scope.

### Design changes

- Freeze artifacts, schemas, policy configuration, compatibility assumptions,
  evidence formats, and operator procedures.
- Provide structured health for raw signals, assessment reasons, fallback,
  desired/requested/current, capacity limitation, pending age, command intent,
  latch, and recovery state.
- Complete least-privilege installation, monitoring, alerting, pause, recovery,
  upgrade, downgrade, rollback, and support-boundary documentation.
- Require one host-wide durable capacity authority before enabling more than one
  controller; single-VM evidence cannot authorize competing allocation.
- Publish known limitations and an indexed qualification record.

### Likely files and modules

- controller status and operational CLI
- deployment and release tooling
- global capacity/reservation modules if multi-VM is in the release scope
- Windows event/resource packaging
- all public contracts, support matrix, runbooks, and release checklist

### Tests required

- Full local and native Windows gates for the frozen revision.
- Installed deployment, telemetry, compatibility, and no-actuation preflight.
- Applied growth, reclaim, rate degradation, recovery, and endurance gates.
- Installation, upgrade, downgrade, rollback, monitoring, and security review.
- Multi-VM reservation and failure qualification when multi-VM is claimed.

### Success criteria

- Every required gate has current evidence for the exact candidate.
- No unexplained warning, transition, fallback, or operator intervention remains.
- Failures are bounded, observable, and covered by a rehearsed response.
- The review records an explicit GO or NO-GO for a precisely stated support
  profile.

### Dependencies

WN9 and every release-scope platform, security, operations, and capacity gate.

### Explicitly not yet

- No support outside the declared matrix.
- No untrusted-guest claim without a separate threat review and hard isolation.
- No multi-VM claim without durable atomic host reservation.
- No release based on source tests, shadow results, or historical evidence
  alone.

## Superseded roadmap items

The following items are obsolete as release gates and must not be resumed as
if they still represented the target architecture:

- fixed-headroom automatic-behavior qualification as the primary sizing-policy
  proof;
- the prior combined growth-and-reclaim milestone that introduced both policy
  directions before pressure semantics were qualified;
- prior qualification tasks tied only to fixed-reserve targets;
- fixed physical and commit reserves as the main definition of demand;
- the legacy threshold-based demand calculator and legacy demand-source mode,
  after telemetry migration is complete;
- hard-coded workload sizes, durations, counter thresholds, or reserve values
  in roadmap acceptance criteria; and
- any external balloon-driver algorithm, floor constants, or reporting cadence
  as an implementation dependency.

Historical evidence remains useful for transport, reconciliation, compatibility,
platform progress, and failure analysis. It cannot qualify the new requirement,
pressure, or shrink-safety decisions.

## Execution rule

`BACKLOG.md` owns the current task claim. `docs/QA-roadmap.md` owns applied
qualification status. This document owns architecture order and milestone exit
criteria.

For each milestone, implement only its smallest coherent slice, run focused
owning-crate tests, then `cargo xtask gate local`, followed by the applicable
native Windows, deployment, live, recovery, or endurance gate. Update contracts,
boards, feature status, and project status together. An unavailable or unrun
layer remains open.
