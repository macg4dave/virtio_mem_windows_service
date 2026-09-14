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

TASK-039 / WN4 is claimed for the bounded pressure-aware growth implementation.
The initial slice adds explicit normal, urgent, fallback, held, and
capacity-limited decisions while pressure-policy reclaim remains prohibited.
Focused core/host tests and `cargo xtask gate local` pass on 2026-09-14.
Native deployment and QA-T028 applied qualification remain separate gates.

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

No additional product task is ready while TASK-039 is in progress.

Continue with the first Ready task whose dependencies are satisfied. Claim it
in this file before implementation, then record concise outcome and validation
evidence here when it completes.

## Queued construction

| ID | Work | Depends on |
| --- | --- | --- |
| TASK-040 | Complete WN5: add fail-closed shrink blockers and conservative reclaim from sustained qualified evidence | TASK-039, QA-T028 |
| TASK-041 | Complete WN6: add only rate/trend signals that demonstrate value in shadow qualification | TASK-040, QA-T029 |
| TASK-042 | Complete WN7: automate the realistic Windows workload and decision-classification matrix | TASK-041, QA-T030 |
| TASK-043 | Complete WN8: run and review resumable unattended repeated-cycle and endurance qualification | TASK-042, QA-T031–QA-T032 |
| TASK-044 | Complete WN9: freeze versioned configuration, evidence-backed defaults, and migration behavior | TASK-043, QA-T033 |
| TASK-045 | Complete WN10 construction: freeze the Windows demand-provider candidate and component evidence index | TASK-044, QA-T034 |
| TASK-031 | Add deterministic fault-scenario execution and machine-readable results for telemetry loss, restart, rejection, ambiguity, and partial progress | TASK-030 |
| TASK-046 | Complete HPM2: build the exclusive host-wide coordinator, Windows provider adapter, full-member snapshot, and shadow grants | TASK-033, TASK-038 |
| TASK-047 | Complete HPM3: grant all growth that fits, apply priority only to constrained contenders, require durable reservations, then remove direct per-VM capacity allocation | TASK-046, TASK-039, WN10 growth evidence |
| TASK-048 | Complete HPM4: reclaim only for unmet higher-priority demand from strictly lower-priority safe donors, qualify wait/cancellation/recovery, then remove independent per-VM reclaim | TASK-047, TASK-040, WN10 reclaim evidence |
| TASK-049 | Complete HPM5: implement and qualify a Linux-native demand provider through the common report contract and mixed-provider pool | TASK-048, reviewed Linux provider design |
| TASK-050 | Complete HPM6 construction: freeze host-pool configuration and operations, add multi-VM recovery/endurance support, remove expired legacy paths, and prepare the final release evidence index | TASK-049 |
| TASK-034 | Add release artifact, configuration migration, install/upgrade/rollback, and monitoring surfaces required before packaging qualification | TASK-030, TASK-046; final acceptance at HPM6 |

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
| WN2 | Complete | Add authoritative Windows pressure notifications | WN1 |
| WN3 | Complete | Separate requirement, pressure, and shrink-safety assessment | WN2 |
| WN4 | In Progress | Integrate pressure-aware growth | WN3 |
| WN5 | Planned | Integrate shrink blocking and conservative reclaim | WN4 |
| WN6 | Planned | Add qualified trend/rate evidence | WN5 |
| WN7 | Planned | Automate realistic Windows behavior qualification | WN6 |
| WN8 | Planned | Complete unattended endurance qualification | WN7 |
| WN9 | Planned | Stabilise Windows provider configuration and remove superseded demand paths | WN8 |
| WN10 | Planned | Record the Windows demand-provider readiness decision | WN9 |

## Host-pool manager milestones

These milestones are the mandatory system path above the WN guest-provider
track. The final product release decision belongs to HPM6, not WN10.

| Milestone | Status | Work | Depends on |
| --- | --- | --- | --- |
| HPM0 | Complete | Define pool/member and OS-neutral demand-report contracts | WN3 contract |
| HPM1 | Complete | Add durable atomic reservation and restart-safe pool accounting | HPM0 |
| HPM2 | Planned | Run one full-member host coordinator in shadow mode | HPM1, WN3 |
| HPM3 | Planned | Let every VM grow when capacity is free; arbitrate only constrained growth and remove direct capacity allocation | HPM2, qualified provider growth |
| HPM4 | Planned | Reclaim from strictly lower-priority safe donors for unmet higher-priority demand, then transfer and remove independent reclaim | HPM3, qualified provider reclaim |
| HPM5 | Planned | Add and qualify another guest-OS provider | HPM4 |
| HPM6 | Planned | Complete multi-VM endurance, legacy cleanup, and the host-manager release decision | HPM5 |

The QA roadmap owns the ordered native, shadow, live, recovery, endurance, and
GO/NO-GO tasks. It requires run-specific workload duration, cycle count,
timeouts, configurable thresholds, and target identity rather than repository
examples. WN milestones qualify the Windows demand-provider component;
HPM milestones integrate it beneath the required host-wide allocation
authority. Multi-VM actuation remains blocked until durable reservation is
proved.

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
- TASK-037: added lifetime-owned low/high Windows memory-resource notification
  handles, categorical low/neutral/high publication, explicit creation/query
  failure evidence, contradiction handling, and cancellation-safe cleanup.
  Focused host tests, the local aggregate gate, and the native Windows gate
  including a real API capability probe passed on 2026-09-13; no installed
  service, workload correlation, target-policy change, or actuation was run.
- TASK-030: added the versioned read-only controller-status contract, product
  command, and typed single-elevation inspection workflow. Focused, local, and
  native Windows gates passed. A live `win11_gpu` read on 2026-09-13 recorded
  converged alias-scoped state with no command owner or latch while the unit
  remained inactive and disabled; this is observability evidence, not
  actuation qualification.
- TASK-038: completed the WN3 shadow assessment, host integration, separate
  fingerprint/history/status state, append-only comparison evidence, runtime
  no-actuation boundary, and typed no-resize qualification workflow. Focused,
  local, and native Windows gates passed; the shadow candidate and schema-v3
  producer were deployed, and a schema-v2 fallback resident run passed without
  allocation change. Broader schema-v3 workload correlation was explicitly
  deferred to later pressure-policy qualification and is not release evidence.
- TASK-032: replaced the host-pressure prototype with versioned semantic
  `HostPoolPolicy`, OS-neutral `GuestDemandReport`, total-RAM conversion, and a
  deterministic pure `HostPoolPlan`. The planner grants all fitting growth
  without priority, uses one priority only under contention, and emits reclaim
  only for a named unmet higher-priority recipient from a strictly
  lower-priority safe donor. Focused core tests and the aggregate local gate
  passed on 2026-09-14; no native, deployment, live, recovery, endurance, or
  actuation work applied.
- TASK-033: added a versioned, bounded, checksum-protected and atomically
  replaced pool ledger bound to the canonical complete policy/member
  fingerprint. Revision-checked command batches reserve all growth before
  dispatch, retain pending reclaim until authoritative `current` falls, and
  preserve unique command ownership through reserved, dispatched, partial,
  ambiguous, converged, and explicitly resolved restart states without replay.
  Focused core tests and the aggregate local gate passed on 2026-09-14; no
  native, deployment, live, recovery, endurance, coordinator, or actuation work
  applied.
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

- Guest services publish measurement only. OS-specific host adapters produce
  per-VM demand reports; the host-pool manager owns allocation and resize.
- Microsoft-supported Windows pressure facilities are the primary demand
  evidence. Current fixed reserves become configurable fallback/safety guards,
  not the release sizing algorithm.
- Alias-scoped live libvirt `current` is authoritative allocation state.
- Automatic shrink is enabled by default, but stale input, ambiguity,
  incompatibility, missing headroom, or a recovery latch fails closed.
- The destination configuration has one total VM RAM pool plus explicit
  per-VM minimums, maximums, priorities, identities, and provider kinds.
- One controller/device remains the supported implementation until the pool can
  reserve host capacity atomically; this is a migration checkpoint, not the
  final architecture.
- Priority is dormant while eligible growth fits and never reserves an
  above-minimum share. Under contention it orders requests; reclaim requires a
  waiting higher-priority recipient and a strictly lower-priority safe donor.
- No reclaim crosses a VM's minimum or qualified safe floor, and released bytes
  are unavailable until live `current` confirms them. If no donor is safe, the
  recipient waits.
- Superseded demand, direct-allocation, and independent-reclaim paths are
  deleted at their roadmap retirement gates instead of becoming permanent
  compatibility modes.
- Upstream QGA is a health and identity channel. The experimental custom
  memory command is not a production dependency.
- The Windows driver status interface is deferred; optional diagnostics cannot
  become allocation authority.
- Fixed product policy belongs in the validated configuration and normative
  target-controller contract. Test/deployment parameters belong to each run.
