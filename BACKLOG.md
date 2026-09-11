# Backlog

This file is the execution source of truth. Detailed qualification work is
tracked in [docs/QA-roadmap.md](docs/QA-roadmap.md); build and test tooling is
tracked in
[docs/build-test-tooling-roadmap.md](docs/build-test-tooling-roadmap.md).
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

## Planned

| ID | Work | Depends on |
| --- | --- | --- |
| M11 | Hermetic global-pool simulation using absolute targets | TASK-028 |
| M11a | Target-based controlled reclaim | M11 and successful single-VM safety qualification |
| M12 | Operational observability and recovery hardening | M11a |
| M13 | Release readiness | M12 |

The QA roadmap owns the ordered deployment, fault, endurance, and GO/NO-GO
tasks. It deliberately requires run-specific workload duration, cycle count,
timeouts, thresholds, and target identity rather than repository defaults.

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
  instructions and generated test artefacts.

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
