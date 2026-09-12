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

No implementation task is claimed. TASK-036 completed the WN1 contract;
TASK-037 is the next ready native-telemetry slice.

## Paused fixed-headroom qualification

| ID | Prior work | Disposition |
| --- | --- | --- |
| TASK-028 | Qualify the fixed-headroom single-VM target controller | Superseded as the release-policy qualification; retain its artifacts as historical reconciler/platform evidence. |
| QA-T009 | Resident-memory qualification through the prior production path | Paused after partial growth evidence; it cannot qualify the pressure-aware policy. |

The installed host controller remains intentionally disabled and inactive
after the coherent deployment milestone. Historical run details remain in their
artifact directories and deployment manifest rather than this execution board.
Pressure-contract, collection, and shadow tasks do not start it for actuation.
The prior fixed-headroom qualification sequence is superseded by QA-T025 onward;
no historical result authorizes automatic resizing under the new policy.

## Ready

| ID | Work | Depends on |
| --- | --- | --- |
| TASK-037 | Complete WN2: collect authoritative Windows memory-resource notifications and publish their capability, state, and failures without policy actuation | TASK-036 |
| TASK-030 | Expose a versioned, read-only controller status snapshot containing live desired/requested/current, accepted telemetry identity, reclaim readiness, capacity state, command ownership, and latch/recovery details | TASK-029 |
| TASK-032 | Define the global-controller contract and implement a versioned durable reservation ledger around the existing pure pool planner | Existing `global_pool` prototype |

Continue with the first Ready task whose dependencies are satisfied. Claim it
in this file before implementation, then record concise outcome and validation
evidence here when it completes.

## Queued construction

| ID | Work | Depends on |
| --- | --- | --- |
| TASK-038 | Complete WN3: implement separate memory-requirement, pressure-state, and shrink-safety assessments in shadow mode | TASK-037, TASK-030 |
| TASK-039 | Complete WN4: enable pressure-aware bounded growth while reclaim remains disabled | TASK-038, QA-T025–QA-T027 |
| TASK-040 | Complete WN5: add fail-closed shrink blockers and conservative reclaim from sustained qualified evidence | TASK-039, QA-T028 |
| TASK-041 | Complete WN6: add only rate/trend signals that demonstrate value in shadow qualification | TASK-040, QA-T029 |
| TASK-042 | Complete WN7: automate the realistic Windows workload and decision-classification matrix | TASK-041, QA-T030 |
| TASK-043 | Complete WN8: run and review resumable unattended repeated-cycle and endurance qualification | TASK-042, QA-T031–QA-T032 |
| TASK-044 | Complete WN9: freeze versioned configuration, evidence-backed defaults, and migration behavior | TASK-043, QA-T033 |
| TASK-045 | Complete WN10 construction: freeze the candidate and production-readiness evidence index for final review | TASK-044, QA-T034 |
| TASK-031 | Add deterministic fault-scenario execution and machine-readable results for telemetry loss, restart, rejection, ambiguity, and partial progress | TASK-030 |
| TASK-033 | Build the configured host-wide coordinator that applies durable pool grants through per-VM reconcilers without live actuation | TASK-032, WN3 assessment contract |
| TASK-034 | Add release artifact, configuration migration, install/upgrade/rollback, and monitoring surfaces required before packaging qualification | TASK-030, TASK-033 |

These are construction tasks, not qualification passes. They may proceed while
the revised pressure QA gates are pending, but their milestone remains open
until the applicable local, native, deployment, live, recovery, and endurance
gates pass.

## Windows-native controller milestones

Detailed outcomes and exit conditions live in
[docs/roadmap.md](docs/roadmap.md). Milestones summarize dependent task groups;
they do not replace executable task cards.

| Milestone | Status | Work | Depends on |
| --- | --- | --- | --- |
| WN0 | Complete | Audit and clean up the fixed-headroom controller direction | Existing implementation |
| WN1 | Complete | Define the Windows-native telemetry contract | WN0 |
| WN2 | Ready | Add authoritative Windows pressure notifications | WN1 |
| WN3 | Planned | Separate requirement, pressure, and shrink-safety assessment | WN2 |
| WN4 | Planned | Integrate pressure-aware growth | WN3 |
| WN5 | Planned | Integrate shrink blocking and conservative reclaim | WN4 |
| WN6 | Planned | Add qualified trend/rate evidence | WN5 |
| WN7 | Planned | Automate realistic Windows behavior qualification | WN6 |
| WN8 | Planned | Complete unattended endurance qualification | WN7 |
| WN9 | Planned | Stabilise configuration, migration, and defaults | WN8 |
| WN10 | Planned | Record the production-readiness decision | WN9 |

The QA roadmap owns the ordered native, shadow, live, recovery, endurance, and
GO/NO-GO tasks. It requires run-specific workload duration, cycle count,
timeouts, configurable thresholds, and target identity rather than repository
examples. Multi-VM work remains separate until WN3 stabilises the per-VM
assessment and durable host-wide reservation is proved.

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
- TASK-035: audited the fixed-headroom implementation, researched supported
  Microsoft pressure facilities, and established the Windows-native
  pressure-aware architecture and migration roadmap. This is design evidence,
  not implementation or live qualification.
- TASK-036: added the allocation-free schema-v3 Windows-native capability and
  signal contract, explicit schema-v2 fallback, strict validation and migration
  rules, and shared Windows/host fixtures. Focused core/host tests, the local
  aggregate gate, and the native Windows aggregate gate passed on 2026-09-13;
  no service deployment, native pressure collection, or actuation was run.
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
- Microsoft-supported Windows pressure facilities are the primary demand
  evidence. Current fixed reserves become configurable fallback/safety guards,
  not the release sizing algorithm.
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
