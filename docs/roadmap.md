# Automatic-resizing release roadmap

## Destination

The destination is a supported automatic-resizing release in which the host
controller grows and reclaims virtio-mem for every configured Windows guest in
the declared support matrix without routine operator resize commands. Reclaim
is enabled by default. Every change remains bounded by fresh guest demand,
alias-scoped live device state, host capacity, compatibility, durable command
ownership, and convergence.

"Release" in this roadmap means all of the following are true:

- both growth and reclaim have passed representative applied workloads;
- one host-wide authority reserves capacity before granting competing growth;
- stale data, restart, partial progress, and uncertain command outcomes cannot
  cause overlapping or replayed requests;
- operators can install, monitor, pause, recover, upgrade, and roll back the
  supported stack using maintained product or `cargo xtask` entrypoints;
- release artifacts, configuration schemas, compatibility assumptions, and
  evidence are versioned and reviewable; and
- every required code, native Windows, deployment, live, recovery, endurance,
  and release gate has a current result for the candidate.

A single successful resize, a passing unit test, or a long-running process is
not a release. Likewise, default-on reclaim is a product behavior, not
permission to bypass a failed safety gate.

The first supported release remains limited to explicitly configured, trusted
Windows guests on one RHEL/libvirt host. General untrusted-guest or broad
production support additionally requires supported upstream platform behavior,
hard resource isolation, and the security review in AR7. The optional custom
QGA memory command and a Windows driver status API are not release
dependencies.

## Current position

The post-cleanup repository has one Rust implementation path:

- Windows measures and atomically publishes allocation-free telemetry.
- The single-VM host runtime calculates absolute targets and owns libvirt
  actuation, journaling, convergence, and recovery.
- The shared core contains deterministic estimator, reconciler, shrink
  recovery, and global-pool logic.
- `cargo xtask` owns maintained local, native-Windows, live-resize, and
  detached qualification workflows.

The single-VM controller is implemented in code, but the installed candidate
and production telemetry handoff are not yet proven coherent. Applied
automatic growth/reclaim, recovery, and endurance evidence is incomplete, so
unattended automatic resizing remains **NO-GO**.

The shared `global_pool` module is a useful side-effect-free arbitration
prototype with focused tests. It is not yet a host-wide controller: no runtime
collects a complete multi-VM snapshot, owns durable pool reservations, or
passes grants to per-device reconcilers.

[BACKLOG.md](../BACKLOG.md) owns current task status.
[QA-roadmap.md](QA-roadmap.md) owns the ordered single-controller deployment
and qualification tasks.
[build-test-tooling-roadmap.md](build-test-tooling-roadmap.md) owns remaining
workflow infrastructure. This document defines release dependencies and exit
conditions without copying run-specific sizes, endpoints, durations, or
historical evidence.

## Release train

| Milestone | Status | Release outcome | Primary execution source |
| --- | --- | --- | --- |
| AR0 | Complete | Verify the post-cleanup repository as one coherent candidate source tree | `BACKLOG.md`, tooling board |
| AR1 | Complete | Deploy one coherent, least-privilege single-VM stack | QA-G1 / QA-T002–QA-T008 |
| AR2 | Planned | Prove automatic single-VM growth and reclaim under real workloads | QA-G2 / QA-T009–QA-T012 |
| AR3 | Planned | Prove bounded failure recovery and actionable observability | QA-G3 / QA-T013–QA-T018 |
| AR4 | Planned | Pass single-VM endurance and record the scoped qualification decision | QA-G4–QA-G5 / QA-T019–QA-T024 |
| AR5 | Prototype only | Complete the deterministic, durable global controller | Product backlog |
| AR6 | Planned | Integrate and qualify live multi-VM arbitration and actuation | Product backlog and new live QA board |
| AR7 | Planned | Make the supported stack distributable, secure, operable, and upgradeable | Product and tooling backlogs |
| AR8 | Planned | Freeze, qualify, and publish the automatic-resizing release | Release checklist and evidence index |

AR0 through AR4 establish that the existing single-device controller and
platform are safe enough to become an input to multi-VM work. AR5 and AR6 add
host-wide safety; they do not replace the per-VM estimator or reconciler. AR7
turns proven behavior into a supported deliverable. AR8 accepts or rejects the
exact release candidate.

## AR0 — Post-cleanup verified baseline

### Objective

Establish that the large cleanup left one buildable, documented, internally
consistent implementation before new controller behavior is added.

### Deliverables

- A source-to-contract audit confirms the remaining Windows, host, core, and
  `xtask` modules have clear owners and no maintained workflow depends on
  deleted guest-side resize or custom-QGA code.
- Current command help, examples, configuration templates, feature status,
  and boards agree with the compiled interfaces.
- Open work is classified as product behavior, tooling, deployment evidence,
  platform qualification, or deferred scope; obsolete milestone-era
  procedures and run values are not restored.
- The candidate source revision and produced artifacts are identifiable, with
  local and native-Windows validation reported independently.

### Verification and exit

- Focused tests pass for any corrected owner.
- `cargo xtask gate local` passes on the cleaned tree.
- `cargo xtask windows all` passes for the same source revision.
- Documentation links and `git diff --check` pass, and any unavailable platform
  gate is explicitly open rather than treated as passed.

AR0 closes only when the cleanup itself is verified. It does not prove an
installed deployment or automatic resize behavior.

## AR1 — Coherent single-VM candidate

### Objective

Create one reviewable candidate in which Windows publication, telemetry
transport, host configuration, live device identity, and compatibility
attestation describe the same VM and artifacts.

### Deliverables

- Complete the installed-state inventory and deployment delta in QA-T002.
- Define and implement the least-privilege production telemetry handoff,
  including identity, ACL, atomicity, bounded retention, acknowledgement, and
  restart behavior.
- Build and install matching Windows and host candidates from the verified
  source revision; validate their versioned configuration before start.
- Verify Windows service identity, protected publication, advancing records,
  clean lifecycle, and guest health independently of QGA alone.
- Verify the host unit and instance configuration in fail-stop mode while
  actuation remains paused.
- Recalibrate the visible base from fresh allocation-neutral state and produce
  a reviewed compatibility attestation for the installed stack.
- Add or finish typed deployment, guest-health, and evidence workflows where a
  repeatable check still depends on manual parsing.

### Verification and exit

QA-T002 through QA-T008 pass. A no-actuation preflight proves exact VM/device
selection, fresh telemetry, replay/session handling, live `requested ==
current`, host headroom, current attestation, single-controller ownership, and
a reviewed rollback path. No memory mutation is needed to close AR1.

## AR2 — Automatic single-VM behavior

### Objective

Demonstrate that the installed controller—not the test harness—automatically
grows and safely reclaims memory from measured Windows demand.

### Deliverables

- Run applied resident-memory and committed-memory workloads through the
  production telemetry path using explicit, reviewed run inputs.
- Correlate workload phases, raw telemetry identity, estimator output,
  `desired`, live `requested`/`current`, host headroom, controller events, and
  guest health in one durable evidence set.
- Prove immediate bounded growth, history-qualified bounded reclaim, default-on
  automatic shrink, capacity-limited behavior, and no lower request while a
  previous request is pending.
- Apply renewed pressure during an owned shrink and prove upward freeze,
  cancellation, or supersession follows the target-controller contract.
- Record cleanup and final-state disposition separately from controller
  success; restore the captured initial target only when that run's approved
  recovery contract requires it.

### Verification and exit

QA-T009 through QA-T012 pass for both workload modes. Each run satisfies its
predeclared growth, reclaim, health, timing, and cleanup criteria. Zero or
partial platform reclaim may demonstrate safe controller behavior, but it does
not satisfy the platform-reclaim part of AR2.

## AR3 — Recovery and observability

### Objective

Prove that every expected fault becomes a bounded, diagnosable state and that
restart never creates a second resize authority or replays uncertain work.

### Deliverables

- Exercise missing, stale, malformed, replayed, cross-session, and
  identity-mismatched telemetry in converged and pending states.
- Exercise Windows and host service interruption, cancellation, pre-command
  rejection, command timeout/ambiguity, and controller restart with durable
  intent present.
- Exercise zero-progress and partial-progress growth/reclaim, stale telemetry
  during shrink, upward cancellation, convergence timeout, and explicit latch
  recovery.
- Expose structured current state, last accepted telemetry, last successful
  decision, desired/requested/current, capacity limitation, pending age,
  restart count, latch reason, command identity, and recovery result.
- Provide maintained health collection and fault-injection workflows with a
  common versioned machine-readable result.
- Package and verify readable Windows Event Log descriptions while preserving
  bounded structured EventData.
- Document and rehearse pause, inspect, clear-latch, abandon-to-current,
  service restart, and escalation procedures.

### Verification and exit

QA-T013 through QA-T018 pass. Every injected fault has an expected terminal or
recoverable state, evidence and alert mapping, and operator action. No case
causes blind retry, duplicate authority, target undershoot, stale-data reclaim,
or unexplained service-manager cycling.

## AR4 — Single-VM endurance and qualification checkpoint

### Objective

Turn functional evidence into a reviewed statement about the selected
single-VM platform before extending actuation to a shared host pool.

### Deliverables

- Define a repeated-cycle plan from the failure modes and observation cadence,
  with workload sizes, timings, cycle count, and acceptance thresholds stored
  as run inputs rather than product defaults.
- Run an unattended soak long enough to exercise the stated risk model while
  preserving controller, workload, host, guest-health, and service evidence.
- Classify every warning, restart, dropped sample, constrained operation,
  attestation rejection, latch, cleanup result, and operator intervention.
- Publish the single-VM support profile, known limitations, monitoring and
  recovery runbook, and indexed evidence.

### Verification and exit

QA-T019 through QA-T024 pass and the review records an explicit GO or NO-GO.
GO qualifies the declared single-VM candidate and permits AR5 work. AR6 may
cross the live multi-VM boundary only after AR5 closes. The checkpoint does not
by itself authorize a multi-VM or general production claim. Any unexplained
transition, missing raw evidence, or unrehearsed recovery keeps the result at
NO-GO.

## AR5 — Durable global controller

### Objective

Convert the existing pure `global_pool` prototype into the single host-wide
authority that atomically arbitrates all configured guests before any live
command is issued.

### Deliverables

- Write a normative global-controller contract covering complete-snapshot
  identity, freshness, pressure policy, host reserves, priorities, fairness,
  starvation bounds, capacity limitation, and degraded modes.
- Replace fixed prototype assumptions with validated policy configuration or
  explicitly documented invariants; keep run-specific values out of defaults.
- Consume each VM's authoritative `current`, pending `requested`, absolute
  `desired`, `safe_floor`, demand freshness, reconciler health, and command
  ownership without recalculating Windows demand globally.
- Introduce reservation epochs or an equivalent durable transaction model so
  competing growth cannot oversubscribe the pool and restart cannot duplicate
  a grant.
- Count reclaimed capacity only after a lower live `current` is observed.
  Planned, requested, or partially completed reclaim is not free capacity.
- Define behavior for stale/missing members, membership changes, one stuck VM,
  host pressure escalation, insufficient reclaim, arithmetic failure, and
  command ambiguity.
- Persist bounded global state with schema/fingerprint checks and explicit
  migration or fail-closed behavior.

### Verification and exit

Deterministic, generated, and fault-injected tests prove conservation of host
capacity, alignment, reservation uniqueness, deterministic ordering,
priority/fairness rules, no stale grant, no double counting, restart/no-replay,
and safe degradation for multiple concurrent demands. The simulator emits a
versioned decision/evidence model. AR5 performs no live multi-VM actuation.

## AR6 — Live multi-VM automatic resizing

### Objective

Wire global grants through the existing per-VM reconcilers and qualify real
competing guests without weakening any single-device invariant.

### Deliverables

- Run one host-wide controller process or equivalently exclusive coordinator;
  independent per-VM services may not race over unreserved host memory.
- Discover no mutation targets: every VM, device alias, policy, priority, and
  isolation boundary is explicitly configured and validated.
- Pass global grants to each existing reconciler, which continues to own block
  alignment, quanta, command intent, convergence, cancellation, and latching.
- Enforce reviewed hypervisor/cgroup limits and a host reserve independently of
  guest telemetry and controller calculation.
- Add detached multi-VM qualification that correlates all guest workloads,
  global decisions, reservations, live allocations, host pressure, health,
  cleanup, and final state without becoming a second resize authority.
- Prove rolling addition/removal of a paused member and failure isolation for a
  stale, stopped, slow, or latched VM.

### Verification and exit

Applied tests cover simultaneous growth, growth versus reclaim, host pressure,
priority/fairness, partial progress, renewed pressure, one unavailable guest,
coordinator restart, and uncertain commands. Total granted live allocation
never exceeds the allocatable pool; capacity is reused only after observation;
healthy guests remain safe when one guest fails. Repeated-cycle and soak gates
pass for the declared maximum guest count.

## AR7 — Distribution, security, and operations

### Objective

Turn the qualified controller into a supportable product that can be installed
and changed without repository knowledge or ad hoc machine repair.

### Deliverables

- Produce versioned Windows and RHEL artifacts with hashes, provenance,
  dependency/license inventory, and an explicit supported platform matrix.
- Provide maintained install, configuration validation, upgrade, rollback,
  uninstall, and post-install verification paths with least-privilege service
  identities and protected state/telemetry directories.
- Define compatibility and migration behavior for configuration, raw
  telemetry, acknowledgement, policy checkpoint, command journal, global
  reservation, and evidence schemas.
- Complete a threat review for telemetry spoofing/tampering, cross-VM identity,
  local privilege boundaries, command injection, state replacement, resource
  exhaustion, and artifact supply chain.
- Verify hard host/guest resource limits, monitoring integration, alert
  thresholds, log retention, time synchronization assumptions, backup/restore,
  and operator escalation.
- Re-audit the supported QEMU, libvirt, Windows, virtio-mem driver, and service
  stack; unsupported upstream behavior remains an explicit release limitation.
- Ensure maintained `cargo xtask` workflows can create a deployment manifest,
  collect health/evidence, and validate rollback without embedding one lab's
  identities or policy values.

### Verification and exit

Clean-machine install, upgrade from the declared predecessor, configuration
rejection, rollback, uninstall, credential/ACL, tamper, resource-bound, and
monitoring tests pass on every supported platform combination. The operator
runbook is rehearsed by someone other than the implementation author, and no
release-critical step depends on an undocumented command or ignored artifact.

## AR8 — Release candidate and release decision

### Objective

Qualify one immutable candidate and publish it only if its exact artifacts and
support claims satisfy every preceding milestone.

### Deliverables

- Freeze versioned source and artifact identities; rerun the full cumulative
  gate matrix without substituting results from another revision.
- Run the declared single- and multi-VM workloads, recovery matrix, repeated
  cycles, and endurance plan on the supported platform matrix.
- Resolve or explicitly accept every warning, known issue, dependency risk,
  security finding, and platform limitation against written release criteria.
- Publish release notes, install/upgrade/rollback instructions, configuration
  reference, architecture and safety contract, operator runbook, support
  matrix, known limitations, and a machine-readable evidence index.
- Record an independent reviewed GO or NO-GO decision and retain the rollback
  artifacts for the release support window.

### Verification and exit

The release is GO only when every required focused, local, native Windows,
deployment, live, fault, recovery, endurance, packaging, upgrade, rollback,
security, and documentation gate is current and passing for the frozen
candidate. Any missing gate is NO-GO, not a conditional pass.

At GO, normal operation automatically grows and reclaims memory from current
demand under one durable host-wide authority. Operators intervene for policy
change, pause, recovery, upgrade, or exceptional failure—not for routine
resizing.

## Rules for executing this roadmap

1. Claim and complete work through `BACKLOG.md`; keep milestone status here at
   outcome level.
2. Change the owning contract, code, tests, examples, feature status, and
   trackers together when behavior or a public schema changes.
3. Run focused owner tests before `cargo xtask gate local`; then run native,
   deployment, live, and endurance workflows only where their boundaries
   apply. Report each layer separately.
4. Preserve durable JSON/JSONL evidence for long-running or cross-process work.
   A summary without raw evidence cannot close a live milestone.
5. Derive device geometry and allocation from fresh alias-scoped live state.
   Deployment and qualification inputs are explicit and recorded per run.
6. Do not carry old VM names, hashes, sizes, thresholds, timeouts, or workload
   profiles into a new candidate merely because they appeared in prior work.
7. If a safety invariant cannot be proven, hold actuation, preserve evidence,
   and record the milestone as blocked or NO-GO.
