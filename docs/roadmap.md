# Host VM RAM Manager Roadmap

## Destination and authority

The destination is one host-wide RAM manager that arbitrates a configured
memory pool across managed VMs. Each VM has an explicit minimum guarantee,
maximum bound, priority, device identity, and guest-demand provider. The host
knows every member's assessed demand, qualified reclaim floor, authoritative
current allocation, and in-flight request before it grants any target.

Windows-native pressure work is the first guest-demand provider, not the
top-level allocation architecture. Windows remains measurement-only: its
service collects and normalises native APIs, notifications, and counters. A
host adapter turns that evidence into a per-VM demand/pressure assessment. A
future Linux adapter can use Linux-native evidence to produce the same host
assessment contract without adopting Windows counters or schemas.

The host-pool manager joins all per-VM assessments with live allocation,
reserves minimum guarantees, arbitrates scarce discretionary capacity by
priority, and emits one granted target per VM. Only the host owns pool
allocation and resize actuation. Per-VM reconcilers safely pursue those grants;
they do not independently spend host memory.

Priority is dormant while the pool can satisfy all eligible growth. It neither
reserves capacity above a VM's minimum nor prevents a lower-priority VM from
growing into free RAM. It becomes active only when requests contend for
insufficient free capacity or when unmet higher-priority demand may justify
safe reclaim from a strictly lower-priority VM.

The target data flow is:

```text
guest-OS-native telemetry
    -> OS-specific host validation and per-VM demand assessment
    -> host-wide VM RAM pool and durable reservation ledger
    -> minimum guarantees + priorities + demand + safe reclaimability
    -> per-VM total-RAM `pool_grant`
    -> device-scoped per-VM target (`desired`)
    -> per-VM desired/requested/current reconciliation
    -> journaled virtio-mem actuation
```

The following safety mechanisms remain architectural invariants:

- exact VM and device identity;
- fresh, ordered, versioned telemetry with replay protection;
- alias-scoped live `current` as allocation authority;
- distinct `desired`, `requested`, and `current` state;
- configured total-RAM pool/member minimums and maximums plus provider safety
  margins, growth step, shrink step, hysteresis, and history window;
- device alignment, compatibility attestation, and host headroom;
- no ordinary resize while `requested != current`;
- write-before-command intent, immediate live reread, no blind replay, and
  durable recovery latches; and
- fail-closed shrink whenever required evidence is missing or ambiguous.

The current fixed-headroom estimator remains implemented during migration. It
is a temporary comparison baseline and qualification bridge, not a second
supported policy. It cannot authorize fallback shrink. Once the replacement
pressure path has passed its named qualification and rollback window, the old
estimator and its dedicated configuration are removed.

## Where the previous roadmap diverged

The previous WN0-WN10 sequence correctly protected host actuation authority and
separated Windows measurement from policy, but it still described a smarter
single-VM controller as the destination. In particular:

- per-VM assessment flowed directly to `desired` instead of first requesting a
  host-pool grant;
- pool size, member minimums, and priorities were not a first-class
  configuration model;
- the existing pure `global_pool` planner and durable reservation work appeared
  as optional later multi-VM safety work rather than the mandatory parent
  controller;
- that prototype derives an allocatable amount from host-physical baseline,
  cache, and emergency reserves and carries separate growth/reclaim priorities;
  it does not yet implement the intended explicit VM-pool limit plus one
  per-member priority policy;
- the prototype accounts the managed allocation input without defining how
  non-reclaimable base RAM contributes to a VM's total pool charge, so its
  minimum is not yet the total-RAM guarantee expressed by the target model;
- cross-VM contention and reclaim-for-transfer had no end-to-end milestone;
- the prototype's host-pressure reclaim can run without an unmet
  higher-priority recipient, which is not the intended demand-driven transfer
  model;
- Windows raw telemetry was the architectural input, with no OS-neutral
  per-guest demand-provider boundary;
- WN10 could reach a release decision while multi-VM pool qualification was
  conditional; and
- fallback and legacy migration language did not give every obsolete path a
  firm deletion gate.

This roadmap keeps WN4 as the current bounded implementation slice. WN0-WN10
now qualify the Windows demand-provider and per-VM safety components. HPM0-HPM6
then make the host pool the only allocation authority and deliver the complete
multi-VM product. These are dependent component and system tracks within one
roadmap, not alternative controller designs.

## Decision model

The new controller has three deliberately separate outputs.

### Memory requirement

This is a byte estimate used to construct the provider's eventual total-RAM
`demand_target`. Its first shadow policy is based on current Windows committed
memory plus a configurable demand safety margin. The current single-VM host
converts the total visible-memory requirement into a virtio-mem requirement
using the configured visible base, then applies the configured minimum,
maximum, device geometry, alignment, and host-capacity bounds.

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

WN3 retains the existing device-scoped shadow result so it can be compared with
the current controller. HPM0 defines the checked adapter that combines the
host-derived non-reclaimable base and that result into the total-RAM
`GuestDemandReport.demand_target`; guest telemetry is not allocation authority.

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

### Host-pool grant

The OS-specific host adapter exports total-RAM `demand_target` and `safe_floor`
with pressure and shrink eligibility. It does not export `desired`. The
host-pool manager protects every enabled VM's aligned minimum, then grants all
eligible growth that fits without consulting priority. When requests exceed
free capacity, priority orders the contenders for the remainder. A still-unmet
higher-priority request may trigger reclaim only from a strictly lower-priority
VM whose demand evidence is shrink-safe, and only to
`max(minimum, safe_floor)`. Underutilisation without contention does not trigger
priority reclaim.

The pool's per-VM total-RAM grant is converted, using the host-derived base and
fresh geometry, into device-scoped `desired`. Growth consumes a durable
reservation before command dispatch. Reclaim does not become spendable capacity
until live `current` confirms it. If sufficient safe capacity is unavailable,
the unmet demand remains explicitly constrained.

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

### Windows demand-provider track

| Milestone | Status | Outcome | Depends on |
| --- | --- | --- | --- |
| WN0 | Complete | Audit and clean up the fixed-headroom controller direction | Existing implementation |
| WN1 | Complete | Define the Windows-native telemetry contract and capability model | WN0 |
| WN2 | Complete | Collect authoritative Windows pressure notifications | WN1 |
| WN3 | Complete | Separate requirement, pressure, and shrink-safety assessment in shadow mode | WN2 |
| WN4 | Planned | Integrate pressure-aware bounded growth | WN3 |
| WN5 | Planned | Integrate shrink blocking and conservative reclaim | WN4 |
| WN6 | Planned | Add only qualified trend and rate evidence | WN5 |
| WN7 | Planned | Qualify realistic Windows memory behavior and automate the workload matrix | WN6 |
| WN8 | Planned | Run unattended repeated-cycle and endurance qualification | WN7 |
| WN9 | Planned | Stabilise Windows provider configuration and remove superseded demand paths | WN8 |
| WN10 | Planned | Record the Windows demand-provider readiness decision | WN9 |

### Host-pool manager track

| Milestone | Status | Outcome | Depends on |
| --- | --- | --- | --- |
| HPM0 | Complete | Define the host-pool, member-policy, and OS-neutral demand-report contracts | WN3 contract |
| HPM1 | Planned | Add one durable atomic reservation ledger around the pure pool planner | HPM0 |
| HPM2 | Planned | Run one host-wide coordinator in shadow mode across configured members | HPM1, WN3 |
| HPM3 | Planned | Require a durable pool grant for every growth action | HPM2, qualified provider growth |
| HPM4 | Planned | Reclaim safely from donors and transfer released capacity under contention | HPM3, qualified provider reclaim |
| HPM5 | Planned | Add and qualify another guest-OS provider through the common demand contract | HPM4 |
| HPM6 | Planned | Remove superseded paths and make the multi-VM host manager release decision | HPM5 |

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
- Preserve the visible requirement and device-scoped shadow target distinctly
  so HPM0 can define a checked total-RAM `demand_target` without mistaking a
  guest request for granted capacity.

### Likely files and modules

- new focused assessment module in `crates/virtio-mem-core/src/`
- `crates/virtio-mem-core/src/target_controller.rs`
- `host/src/target_policy.rs`
- `host/src/runtime.rs`
- controller status/evidence types
- `tools/xtask` no-actuation workload qualification and comparison capture
- `docs/target-controller.md`

### Tests required

- Table-driven tests proving each input affects only its assigned output.
- Checked arithmetic, alignment, min/max, safety-margin, and overflow cases.
- Low/neutral/high/unavailable and contradictory-state tests.
- Cold start, history gaps, producer restart, and in-flight allocation tests.
- Golden shadow-decision fixtures with stable reason codes.
- Bounded live shadow tests proving the running mode is explicit, every sampled
  and final allocation is unchanged, and the run's comparisons are retained.

### Success criteria

- Requirement bytes, pressure state, and shrink eligibility are independently
  visible and explainable.
- Notification state cannot invent a byte target.
- Requirement calculation cannot bypass shrink-safety gates.
- Shadow mode cannot reach the resize sink.
- The assessment contract is independent of Windows raw-schema details at the
  host-pool boundary.

### Dependencies

WN2 and the read-only controller status surface.

### Explicitly not yet

- No pressure-aware actuation.
- No shrink under either new or fallback evidence.
- No paging-rate influence.

## WN4 — Pressure-aware growth

### Goal

Qualify that the new Windows assessment can drive safe bounded growth for one
VM while keeping reclaim disabled. This is component evidence for later
pool-authorized growth, not permission for independent multi-VM allocation.

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
- Treat direct single-VM wiring as temporary qualification scaffolding; HPM3
  replaces it with a required durable pool grant.

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
- The evidence is reusable by HPM3 but does not qualify shared-pool allocation.

### Dependencies

WN3 shadow evidence and the applicable native/deployment preflight.

### Explicitly not yet

- No automatic shrink.
- No rate-driven growth.
- No multi-VM allocation without durable host reservation.

## WN5 — Shrink blocking and conservative reclaim

### Goal

Qualify the per-VM shrink-safety and bounded actuation behavior only when
Windows-native evidence shows sustained safety, and prove that renewed pressure
stops or reverses it. The output establishes how far this VM may safely shrink;
HPM4 decides whether shared-pool contention requires that reclaim.

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
- Export shrink eligibility and `safe_floor` to the host-pool contract; do not
  let the guest-specific policy choose a cross-VM donor or recipient.
- Keep direct single-VM reclaim as bounded qualification scaffolding. In the
  destination, no per-VM loop schedules reclaim without a host-pool plan for a
  waiting higher-priority recipient.

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
- Default-on reclaim capability remains subject to all qualification and pause
  gates; it does not mean proactive shrink without pool contention.
- HPM4 can consume the qualified safe floor without duplicating Windows signal
  semantics in the global arbiter.

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

## WN9 — Windows provider configuration and legacy-demand removal

### Goal

Freeze a coherent Windows demand-provider configuration only after
qualification has identified defensible defaults and failure behavior, and
remove the demand algorithms and options it supersedes.

### Design changes

- Version policy configuration and fingerprints.
- Define configurable minimum, maximum, demand safety margin, normal and faster
  growth steps, shrink step, hysteresis, history window, pressure thresholds
  where needed, sample bounds, and any newly specified fail-safe.
- Deprecate fixed physical/commit reserves as primary sizing inputs; retain only
  a newly specified fail-safe if qualification proves one is needed. Do not
  retain the full fixed-headroom estimator as the permanent fallback.
- Define upgrade, downgrade, unsupported-signal, and older-schema behavior.
- Reject ambiguous legacy settings instead of silently reinterpreting them.
- Remove the legacy threshold `DemandCalculator`, `guest-stats` demand mode,
  fixed-headroom primary estimator, and their configuration after the
  replacement growth/reclaim gates and declared rollback window pass.
- Give any temporarily retained older-schema decoder an owner, deployment
  window, removal gate, and deletion task.

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
- Legacy configuration has completed its bounded migration window and is
  removed or hard-rejected; it is not a second supported policy.
- Installation and rollback preserve safe disabled/inactive behavior until
  preflight passes.

### Dependencies

WN8 evidence review.

### Explicitly not yet

- No hidden compatibility aliases with changed meanings.
- No environment-specific values in repository defaults.
- No release until the exact frozen configuration is requalified.

## WN10 — Windows demand-provider readiness decision

### Goal

Accept or reject an immutable Windows demand-provider and per-VM reconciler
candidate for the declared Windows, host, hypervisor, driver, and trust scope.
This is a required component checkpoint for HPM3/HPM4, not the final host RAM
manager release.

### Design changes

- Freeze artifacts, schemas, policy configuration, compatibility assumptions,
  evidence formats, and operator procedures.
- Provide structured health for raw signals, assessment reasons, fallback,
  desired/requested/current, capacity limitation, pending age, command intent,
  latch, and recovery state.
- Complete least-privilege installation, monitoring, alerting, pause, recovery,
  upgrade, downgrade, rollback, and support-boundary documentation.
- Publish known limitations and an indexed qualification record.

### Likely files and modules

- controller status and operational CLI
- deployment and release tooling
- Windows event/resource packaging
- all public contracts, support matrix, runbooks, and release checklist

### Tests required

- Full local and native Windows gates for the frozen revision.
- Installed deployment, telemetry, compatibility, and no-actuation preflight.
- Applied growth, reclaim, rate degradation, recovery, and endurance gates.
- Installation, upgrade, downgrade, rollback, monitoring, and security review.
- Contract fixtures proving the provider output can be consumed without
  Windows-specific fields at the HPM boundary.

### Success criteria

- Every required gate has current evidence for the exact candidate.
- No unexplained warning, transition, fallback, or operator intervention remains.
- Failures are bounded, observable, and covered by a rehearsed response.
- The review records an explicit GO or NO-GO for the Windows provider and
  single-VM safety profile, without claiming host-pool readiness.

### Dependencies

WN9 and every Windows-provider platform, security, operations, and safety gate.

### Explicitly not yet

- No support outside the declared matrix.
- No untrusted-guest claim without a separate threat review and hard isolation.
- No shared-pool or multi-VM release claim; those require HPM0-HPM6.
- No component-readiness decision based on source tests, shadow results, or
  historical evidence alone.

## HPM0 — Host-pool and guest-demand contracts

### Goal

Define the host-wide allocation model and the OS-neutral boundary it consumes,
without changing runtime actuation or selecting a final configuration syntax.

### Design changes

- Define a versioned host-pool policy containing total VM-pool bytes, member
  identity, minimum, maximum, priority, provider kind, and arbitration version.
- Define pool/member quantities as total guest RAM and specify the checked
  conversion between host-derived non-reclaimable base, live virtio-mem
  `current`, pool charge, and device target.
- Define `GuestDemandReport` with total-RAM `demand_target` and safe floor,
  pressure, shrink eligibility, freshness, continuity, provider identity, and
  reason codes.
- Define total-RAM `pool_grant` as the pool output and the checked conversion to
  device-scoped per-VM `desired`.
- Require aligned minimums to fit within the pool and define explicit lifecycle
  semantics for enabled, inactive, unavailable, and removed members.
- Specify that priority is consulted only under contention, creates no
  above-minimum reservation, and permits reclaim only for unmet
  higher-priority demand from strictly lower-priority donors.
- Specify deterministic same-priority contention behavior without creating a
  same-priority donor relationship, embedding deployment values, or committing
  to a file format.
- Audit the existing pure `global_pool` prototype against these semantics;
  preserve useful checked arithmetic and replace mismatched policy concepts.

### Likely files and modules

- `crates/virtio-mem-core/src/global_pool.rs`
- pressure-assessment and versioned evidence types
- new host-pool contract/config types
- `docs/architecture.md`
- `docs/data-model.md`
- `docs/api-contract.md`
- `docs/target-controller.md`

### Tests required

- Contract serialization and validation fixtures.
- Pool smaller than aligned total-RAM minimums, base memory above a member
  bound, duplicate identity, invalid priority, stale provider report, overflow,
  geometry, and lifecycle cases.
- Provider-neutral fixtures proving Windows-specific fields do not leak into
  arbitration.
- Lower-priority growth while capacity is free; identical unconstrained grants
  under different priority values; deterministic contention and input-order
  independence.
- No reclaim without unmet higher-priority demand, and no equal- or
  higher-priority donor selection.

### Success criteria

- Pool bytes, minimum guarantees, priorities, demand, safe reclaimability, and
  grants each have one named owner and unit.
- Priority changes only a constrained plan; it never creates a standing share
  or strands free capacity.
- A per-VM demand report cannot authorize allocation.
- The current pure planner either conforms to the contract or has explicit
  replacement tasks; its prototype thresholds are not silently promoted.
- No runtime or live target changes.

### Dependencies

The stable completed WN3 assessment boundary.

### Explicitly not yet

- No durable reservation.
- No coordinator process or actuation.
- No final configuration file syntax or defaults.

## HPM1 — Durable atomic pool accounting

### Goal

Make host-pool grants restart-safe and impossible to double-spend before more
than one reconciler can act.

### Design changes

- Add one versioned, bounded, atomically replaced pool ledger bound to the
  complete member and policy fingerprint.
- Record plan generation, observed `current`, owned `requested`, granted target,
  reserved growth, pending reclaim, and per-VM command ownership.
- Reserve growth before dispatch and release it only after failure resolution
  or authoritative observation.
- Count reclaim as free only after live `current` falls.
- Define cold start, stale member, removed member, corrupt ledger, restart,
  partial progress, and ambiguous command recovery.
- Fail closed when a complete coherent accounting snapshot cannot be built.

### Likely files and modules

- `crates/virtio-mem-core/src/global_pool.rs`
- new pool-ledger/checkpoint module
- shared status/evidence types
- host state persistence and fault-injection helpers
- `docs/data-model.md`
- `docs/target-controller.md`

### Tests required

- Atomic-replace, checksum/version, size-bound, fingerprint, and corruption
  cases.
- Crash at every reserve/dispatch/observe boundary.
- No double allocation across concurrent demand, restart, partial progress, or
  ambiguous command outcomes.
- Reclaim remains unavailable until observed in `current`.

### Success criteria

- Every pool byte is either free, currently allocated, or durably reserved
  exactly once.
- Restart reconstructs accounting without replaying a resize.
- Corrupt or incomplete state blocks allocation and reclaim visibly.
- The implementation remains side-effect-free or shadow-only at runtime.

### Dependencies

HPM0.

### Explicitly not yet

- No live multi-VM coordinator.
- No pool-authorized actuation.
- No reclaim-for-transfer.

## HPM2 — Host-wide shadow coordinator

### Goal

Run one coordinator over the full configured member set and compare pool grants
with current per-VM decisions without changing any target.

### Design changes

- Introduce one host-wide service/process as the exclusive future allocation
  authority; it owns membership, provider adapters, snapshots, arbitration,
  the ledger, and dispatch ordering.
- Adapt Windows pressure assessment to `GuestDemandReport`.
- Read fresh alias-scoped `requested`/`current` for every configured member and
  reject incomplete or mixed-generation snapshots.
- Emit per-cycle pool totals, minimum reservations, demand, priority, planned
  donors/recipients, grants, constraints, and reason codes.
- Compare shadow grants with the existing direct per-VM targets and expose all
  divergence; do not make the per-instance services a second pool authority.

### Likely files and modules

- new host coordinator and provider-adapter modules
- host configuration/status/runtime surfaces
- pool ledger and planner
- `tools/xtask` shadow evidence collection
- deployment unit/templates

### Tests required

- One-VM degenerate pool, multiple lower-priority VMs growing while capacity is
  available, and multi-VM contention fixtures.
- Aggregate demand below, equal to, and above pool capacity.
- Priority-dormant unconstrained plans, constrained ordering, equal-priority
  tie/no-preemption, minimum reservation, stale member, inactive member, and
  provider degradation.
- Process exclusivity, complete-snapshot, status, restart, and evidence tests.

### Success criteria

- A single coherent shadow plan explains every configured VM and every pool
  byte.
- Aggregate demand exceeding the pool yields deterministic constrained grants.
- Every eligible VM can grow when capacity is available regardless of priority.
- Missing or unsafe donor evidence never appears as reclaimable capacity.
- Shadow mode has no path to the resize sink.

### Dependencies

HPM1 and WN3 host shadow output.

### Explicitly not yet

- No pool-authorized growth or reclaim.
- No removal of the active single-VM path.
- No multi-VM release claim.

## HPM3 — Pool-authorized growth

### Goal

Require every growth action to hold a durable grant from the host-wide pool,
first for one VM and then under concurrent multi-VM demand, while reclaim stays
disabled.

### Design changes

- Make the pool's total-RAM `pool_grant` the only source of per-VM `desired` for
  managed members.
- Atomically reserve discretionary pool capacity before dispatching a growth
  grant.
- Grant every eligible request when capacity is sufficient. Only when free
  capacity is insufficient, order simultaneous contenders by configured
  priority with deterministic same-priority handling.
- Preserve every member's minimum reservation without pre-reserving any
  priority-based share above it.
- Revalidate host physical headroom independently; a host constraint can reduce
  a grant but never expand the configured pool.
- Serialize or otherwise coordinate dispatch so no reconciler bypasses the
  ledger or overlaps an in-flight request.
- Remove direct per-VM host-capacity allocation after the pool path passes its
  one-VM and multi-VM growth gates.

### Likely files and modules

- host coordinator/runtime and service topology
- pool planner and ledger
- per-VM target-policy/reconciler adapter
- controller and pool status/event contracts
- deployment and qualification tooling

### Tests required

- One-VM equivalence and rollback fixtures.
- Lower-priority growth to demand when pool capacity is available.
- Concurrent higher/lower/equal-priority demand with insufficient pool space.
- Reservation, command failure, ambiguity, partial progress, cancellation,
  restart, and host-headroom failure.
- Applied growth with reclaim disabled and no oversubscription at any sample.

### Success criteria

- No managed VM grows without a durable pool grant.
- The sum of observed allocation and outstanding growth reservations never
  exceeds the pool.
- Under contention, priority affects only discretionary bytes and never
  consumes another member's minimum.
- Priority has no effect on unconstrained outcomes and does not strand usable
  pool capacity.
- The superseded direct-growth capacity path is deleted after qualification.

### Dependencies

HPM2 and WN4/WN10 growth evidence for every enabled provider.

### Explicitly not yet

- No automatic donor reclaim or transfer.
- No provider-specific policy inside the arbiter.
- No final endurance or release decision.

## HPM4 — Safe reclaim and capacity transfer

### Goal

When a higher-priority VM has unmet demand after free capacity is exhausted,
safely reclaim from eligible strictly lower-priority VMs and make the observed
release available to that waiting recipient.

### Design changes

- Require a concrete waiting recipient with unmet demand before selecting any
  donor; never reclaim merely to maintain idle pool headroom.
- Select donors only when they are strictly lower priority than the recipient,
  underutilised, shrink-eligible, above safe floor/minimum, geometrically valid,
  and free of conflicting in-flight state.
- Do not shrink an equal- or higher-priority VM to satisfy the recipient.
- Never grant a donor target below `max(minimum, safe_floor)`.
- Dispatch bounded reclaim first; hold the recipient constrained until released
  bytes are confirmed by authoritative `current` and committed to the ledger.
- Cancel or freeze reclaim when donor pressure returns, without promising those
  bytes to a recipient.
- Handle insufficient reclaimable capacity, zero/partial progress, donor or
  recipient restart, stale evidence, and command ambiguity explicitly.
- Remove independent periodic per-VM reclaim after pool-owned reclaim passes
  qualification.

### Likely files and modules

- global planner and ledger state machines
- host coordinator scheduling/dispatch
- per-VM shrink recovery and reconciler integration
- pool status/events and qualification analyzer
- `docs/target-controller.md`

### Tests required

- Higher-priority recipient versus lower-priority donor, equal-priority
  no-preemption, lower-priority recipient, underutilised-without-contention,
  and no-safe-donor cases.
- Minimum/safe-floor enforcement and alignment across different device blocks.
- Renewed donor pressure before, during, and after partial reclaim.
- Recipient demand disappearance, zero progress, restart, stale input,
  ambiguous outcome, and ledger recovery.
- Applied contention runs proving reclaim-before-transfer and no oversubscription.

### Success criteria

- Reclaimed capacity is never double-counted or granted before observation.
- No VM crosses its minimum or qualified safe floor.
- No donor reclaim occurs without unmet demand from a strictly higher-priority
  recipient.
- Higher-priority demand receives available discretionary capacity according to
  policy, while unsafe or unavailable reclaim leaves it visibly waiting and
  constrained.
- The superseded independent-reclaim path is deleted after qualification.

### Dependencies

HPM3 and WN5/WN10 reclaim evidence for every enabled donor provider.

### Explicitly not yet

- No claim for an unimplemented guest-OS provider.
- No forced reclaim from stale, unavailable, or shrink-unsafe guests.
- No host overcommit or guest swapping policy outside the declared pool model.

## HPM5 — Additional guest-OS provider

### Goal

Prove that guest demand is extensible by adding a second operating-system
provider without changing pool arbitration or giving the guest allocation
authority.

### Design changes

- Select one supported Linux-native demand/pressure source and define its
  provider-specific collection, validation, continuity, and reason semantics.
- Adapt it to the same `GuestDemandReport` contract used by Windows.
- Keep Linux raw evidence and thresholds out of the Windows schema and out of
  global arbitration.
- Add provider capability/version negotiation and explicit unsupported or
  degraded behavior.
- Exercise homogeneous and mixed-provider pool membership.

### Likely files and modules

- new Linux guest provider/adapter modules
- common provider trait and demand-report types
- host coordinator configuration and status
- guest/provider qualification tooling and documentation

### Tests required

- Provider contract conformance and malformed/stale/discontinuous evidence.
- Linux-native workload correlation before granting policy authority.
- Mixed Windows/Linux contention, priority, minimum, reclaim, degradation, and
  restart cases.
- Proof that adding a provider does not change the arbiter's policy semantics.

### Success criteria

- At least two guest OS implementations feed the same pool contract.
- Provider failure constrains only the affected safe actions and never becomes
  fabricated free capacity.
- Pool planning remains deterministic and free of OS-specific branches.
- Support claims name the exact qualified provider/platform combinations.

### Dependencies

HPM4 and a separately reviewed Linux provider design/qualification plan.

### Explicitly not yet

- No promise that every guest OS is supported.
- No generic raw telemetry schema shared by unrelated operating systems.
- No provider loaded without explicit configuration and capability evidence.

## HPM6 — Multi-VM endurance, cleanup, and release decision

### Goal

Accept or reject an immutable host RAM manager candidate after multi-VM
contention, recovery, endurance, operations, and legacy-removal gates pass.

### Design changes

- Freeze the host-pool and member configuration schema, priority semantics,
  provider versions, ledger format, status, alerts, and operator procedures.
- Qualify repeated simultaneous demand, reclaim-for-transfer, provider loss,
  host pressure, restart, partial progress, ambiguity, membership lifecycle,
  upgrade, downgrade, and rollback.
- Remove remaining direct per-instance allocation entrypoints, old controller
  modes, compatibility aliases, and expired schema decoders.
- Keep only current per-VM assessment/reconciliation/safety components beneath
  the single coordinator.
- Publish the precise supported guest/provider/host matrix and evidence index.

### Likely files and modules

- all host coordinator, provider, pool, reconciler, status, and deployment code
- `tools/xtask` multi-VM qualification and release evidence
- public contracts, configuration templates, runbooks, and status documents

### Tests required

- Full focused, local, and applicable native guest gates for the frozen commit.
- Installed multi-VM growth, reclaim-transfer, recovery, and endurance runs.
- Crash/restart at every ledger/dispatch boundary and loss of each provider.
- Configuration validation, membership change, upgrade/downgrade/rollback,
  monitoring, security, and least-privilege review.
- Repository search proving named superseded modes and settings are gone.

### Success criteria

- One host-wide authority accounts for every managed allocation and reservation.
- Aggregate demand above the pool is resolved or constrained without violating
  minimums, safe floors, priorities, or host safety.
- All fitting demand can grow without priority withholding; every priority-based
  reclaim names a waiting higher-priority recipient and a strictly
  lower-priority safe donor.
- At least the declared provider combinations pass current qualification.
- No temporary compatibility path remains without a separately approved,
  time-bounded exception and deletion task.
- The review records an explicit GO or NO-GO for a precise support profile.

### Dependencies

HPM5, WN10 for each Windows provider version in scope, and every applicable
deployment, recovery, endurance, security, and operations gate.

### Explicitly not yet

- No untrusted-guest support without a separate threat model and isolation
  milestone.
- No dynamic overcommit, NUMA placement, migration, or cluster-wide pooling
  unless separately designed.
- No release from single-VM, shadow-only, or historical evidence.

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

### Retirement gates

| Superseded path | Temporary purpose | Removal gate |
| --- | --- | --- |
| Fixed-headroom estimator and reserve-as-demand settings | WN3 shadow comparison and, only if explicitly selected, bounded WN4/WN5 rollback evidence | Delete in WN9 after replacement growth, reclaim, degradation, and rollback evidence passes |
| Threshold `DemandCalculator` and `guest-stats` demand mode | No new architecture role | Delete when the pressure assessment owns applied single-VM decisions; do not route HPM through it |
| Schema-v2 telemetry decoder | Bounded producer/consumer rollout only | Delete after deployed producers use the current schema and the declared rollback window closes |
| Direct per-VM host-capacity decision | Single-VM component qualification | Delete when HPM3 pool-authorized growth passes |
| Independent per-VM automatic reclaim | Single-VM shrink-safety qualification | Delete when HPM4 pool-owned reclaim passes |
| Per-instance allocation-authority service topology | Incremental deployment scaffold | Delete when the exclusive HPM coordinator and rollback have qualified |

An item that misses its removal gate blocks the corresponding milestone. A new
generic `legacy`, version-selection, or compatibility mode requires a concrete
deployment need, named owner, expiry condition, and deletion task; convenience
alone is not sufficient.

## Execution rule

`BACKLOG.md` owns the current task claim. `docs/QA-roadmap.md` owns applied
qualification status. This document owns architecture order and milestone exit
criteria.

For each milestone, implement only its smallest coherent slice, run focused
owning-crate tests, then `cargo xtask gate local`, followed by the applicable
native Windows, deployment, live, recovery, or endurance gate. Update contracts,
boards, feature status, and project status together. An unavailable or unrun
layer remains open.

WN work may qualify one guest provider and its per-VM safety machinery before
the coordinator is active. HPM work must then consume those outputs rather than
reimplement their OS-specific semantics. No final automatic-resizing release
decision is possible until HPM6; WN10 is a component checkpoint only.
