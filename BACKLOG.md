# Backlog

This file is the execution source of truth. Detailed qualification work is
tracked in [docs/QA-roadmap.md](docs/QA-roadmap.md); build and test tooling is
tracked in
[docs/build-test-tooling-roadmap.md](docs/build-test-tooling-roadmap.md).
Release milestone outcomes and their dependency order are defined in
[docs/roadmap.md](docs/roadmap.md).
Commands and evidence requirements come from [docs/testing.md](docs/testing.md),
not from completed task notes.

## In progress

| ID | Work | Exit condition |
| --- | --- | --- |
| TASK-028 | Qualify the single-VM target controller | An explicitly configured `cargo xtask qualification` run proves or disproves growth, reclaim, renewed pressure, fault handling, restart safety, and cleanup without a second resize authority. |
| QA-T009 | Run an explicitly configured resident-memory qualification through the production path | Durable evidence satisfies the run's predeclared resident growth, falling-demand, reclaim or constrained-state, guest-health, timing, and cleanup criteria. |

The installed host controller remains intentionally disabled and inactive
after AR1. AR2 qualification may start it only through the explicit bounded
`cargo xtask qualification` workflow with one resize authority.

Current QA-T009 handoff: the corrected production QGA path is deployed. One
resident attempt observed automatic growth before its detached observer lost
Polkit authorization; a later complete run
`qualification-1789215769941-25346` then correctly failed closed because the
telemetry producer's new session had already advanced beyond sequence zero
while the controller was inactive. Neither attempt is qualification evidence.
The guard now owns all fixed libvirt/QGA observation under one bounded
elevation, and qualification can explicitly restart only the named telemetry
service after controller ownership begins and wait for the first accepted
controller decision before workload launch. Applied run
`qualification-1789217084408-38317` proved that handoff and completed every
resident workload phase, but its first resize was rejected before actuation
because the September 11 compatibility attestation predates the VM process
started on September 12. A fresh attestation generated from the unchanged
explicit review now matches the current evidence and was installed by the
typed host deployment recorded in
`.artifacts/deployment/qa-t009-host-apply-after-vm-restart.json`. Source and
installed hashes match, requested/current remain converged at 1 GiB, and the
unit is disabled/inactive. The prior latch was cleared without accepting policy
drift: the exact 10-second policy was temporarily restored, the explicit
converged-state clear ran against the current live attestation, and the reviewed
15-second policy was reinstalled. Restored evidence is in
`.artifacts/deployment/qa-t009-host-apply-policy15-restored.json`. A fresh
applied resident run `qualification-1789231813766-72447` then proved automatic
growth by `1098907648` bytes, renewed-pressure growth, every workload phase,
zero observer warnings, and disabled/inactive cleanup. It did not reclaim: one
transient QGA file-sharing collision reset the 300-second reclaim history late
enough that renewed pressure arrived before history recovered. The run failed
its declared 64 MiB reclaim threshold and does not close QA-T009. Its final
requested/current are converged at `2300575744` bytes. The system libvirt
management daemon stopped answering bounded probes after the run; no daemon
restart has been authorized or performed.

## Ready

| ID | Work | Depends on |
| --- | --- | --- |
| TASK-030 | Expose a versioned, read-only controller status snapshot containing live desired/requested/current, accepted telemetry identity, reclaim readiness, capacity state, command ownership, and latch/recovery details | TASK-029 |
| TASK-032 | Define the global-controller contract and implement a versioned durable reservation ledger around the existing pure pool planner | Existing `global_pool` prototype |

Continue with the first Ready task whose dependencies are satisfied. Claim it
in this file before implementation, then record concise outcome and validation
evidence here when it completes.

## Queued construction

| ID | Work | Depends on |
| --- | --- | --- |
| TASK-031 | Add deterministic fault-scenario execution and machine-readable results for telemetry loss, restart, rejection, ambiguity, and partial progress | TASK-030 |
| TASK-033 | Build the configured host-wide coordinator that applies durable pool grants through per-VM reconcilers without live actuation | TASK-032 |
| TASK-034 | Add release artifact, configuration migration, install/upgrade/rollback, and monitoring surfaces required before packaging qualification | TASK-030, TASK-033 |

These are construction tasks, not qualification passes. They may proceed while
QA-T009 is blocked or pending, but their milestone remains open until the
applicable local, native, deployment, live, recovery, and endurance gates pass.

## Automatic-resizing release train

Detailed outcomes and exit conditions live in
[docs/roadmap.md](docs/roadmap.md). Milestones summarize dependent task groups;
they do not replace executable task cards.

| Milestone | Status | Work | Depends on |
| --- | --- | --- | --- |
| AR0 | Complete | Verify the post-cleanup source, documentation, local gate, and native-Windows gate as one candidate | Current cleaned tree |
| AR1 | Complete | Deploy one coherent least-privilege single-VM candidate | AR0, QA-T002, TASK-009 |
| AR2 | In Progress | Prove automatic single-VM growth and reclaim under resident and committed workloads | AR1, TASK-028 |
| AR3 | Construction open | Build, then prove, bounded fault recovery and actionable observability | AR2 for qualification, not implementation |
| AR4 | Planned | Pass single-VM repeated-cycle/endurance gates and record the scoped decision | AR3 |
| AR5 | Construction open | Complete the deterministic durable global controller and atomic reservation model | AR4 only for live AR6 integration |
| AR6 | Planned | Integrate and qualify live multi-VM arbitration and actuation | AR5 |
| AR7 | Planned | Complete distribution, security, upgrade/rollback, monitoring, and support readiness | AR6, remaining tooling tasks |
| AR8 | Planned | Freeze, fully qualify, and publish the supported automatic-resizing release | AR7 |

The QA roadmap owns AR1–AR4's ordered deployment, fault, endurance, and
GO/NO-GO tasks. It deliberately requires run-specific workload duration,
cycle count, timeouts, thresholds, and target identity rather than repository
defaults. AR5 starts from the existing side-effect-free global-pool prototype;
it remains open until durable host-wide reservation and recovery are proved.

## Completed foundations

The following task groups are complete in code and focused tests. This compact
record replaces historical command transcripts and machine-specific evidence:

- TASK-001 through TASK-008: Windows service, adapters, policy, documentation,
  and initial host controller foundations.
- TASK-010 through TASK-025: Rust host tooling, compatibility attestation,
  allocation authority, telemetry freshness and delivery, recovery semantics,
  and roadmap reconciliation.
- TASK-026 and TASK-027: quantitative absolute-target estimation and durable
  desired/requested/current reconciliation.
- TASK-029: transport acquisition failures now block only the current cycle and
  preserve accepted reclaim history for the maximum-gap check; invalid, stale,
  replayed, identity-mismatched, or over-gapped evidence still clears reclaim
  readiness immediately.
- TASK-009 and QA-T001 through QA-T008: native Windows deployment, safe
  baseline, coherent least-privilege host deployment, calibration, current
  attestation, and restart-safe no-actuation preflight.
- BT-T001 through BT-T015: maintained Rust gates, probes, live-resize and
  detached qualification workflows, plus removal of superseded operational
  instructions, generated test artefacts, guest-side resize compatibility code,
  and inherited operational defaults.

Completion means the implementation and its task-scoped tests passed at the
time. It does not substitute for current deployment or qualification evidence.

## Standing decisions

- Windows publishes measurement only. The host owns allocation and resize.
- Alias-scoped live libvirt `current` is authoritative allocation state.
- Automatic shrink is enabled by default, but stale input, ambiguity,
  incompatibility, missing headroom, or a recovery latch fails closed.
- One controller/device is supported until the global pool can reserve host
  capacity atomically.
- Upstream QGA is a health and identity channel. The experimental custom
  memory command is not a production dependency.
- The Windows driver status interface is deferred; optional diagnostics cannot
  become allocation authority.
- Fixed product policy belongs in the validated configuration and normative
  target-controller contract. Test/deployment parameters belong to each run.
