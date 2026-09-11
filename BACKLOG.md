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
| QA-T002 | Capture the installed deployment | The deployment manifest contains the exact installed identities, paths, configuration, and hashes needed to decide whether a coherent rollout is possible. |
| TASK-009 | Finish the Windows native demand-agent deployment | The candidate is installed with its required configuration and protected telemetry location, and native publication is observed under the service account. |
| TASK-028 | Qualify the single-VM target controller | An explicitly configured `cargo xtask qualification` run proves or disproves growth, reclaim, renewed pressure, fault handling, restart safety, and cleanup without a second resize authority. |

The installed host controller is intentionally held while its deployment is
known to be incoherent. Do not resume it until QA-T002 and the deployment gate
identify a matching host binary, unit, environment, attestation, Windows
service, and telemetry contract.

## Ready

| ID | Work | Depends on |
| --- | --- | --- |
| QA-T004 | Deploy one coherent candidate stack | QA-T002 and an operator-reviewed deployment manifest |
| QA-T005 | Recreate and review compatibility attestation | QA-T004 |
| QA-T006 | Run the no-actuation preflight | QA-T004 and QA-T005 |

Continue with the first Ready task whose dependencies are satisfied. Claim it
in this file before implementation, then record concise outcome and validation
evidence here when it completes.

## Automatic-resizing release train

Detailed outcomes and exit conditions live in
[docs/roadmap.md](docs/roadmap.md). Milestones summarize dependent task groups;
they do not replace executable task cards.

| Milestone | Status | Work | Depends on |
| --- | --- | --- | --- |
| AR0 | Complete | Verify the post-cleanup source, documentation, local gate, and native-Windows gate as one candidate | Current cleaned tree |
| AR1 | In progress | Deploy one coherent least-privilege single-VM candidate | AR0, QA-T002, TASK-009 |
| AR2 | Planned | Prove automatic single-VM growth and reclaim under resident and committed workloads | AR1, TASK-028 |
| AR3 | Planned | Prove bounded fault recovery and actionable observability | AR2 |
| AR4 | Planned | Pass single-VM repeated-cycle/endurance gates and record the scoped decision | AR3 |
| AR5 | Prototype only | Complete the deterministic durable global controller and atomic reservation model | AR4 |
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
- QA-T001 and QA-T003: safe baseline capture, raw-telemetry durability, SCM
  identity propagation, and service registration error handling.
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
