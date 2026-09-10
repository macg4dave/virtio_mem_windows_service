# Leave-It-Running QA Roadmap

## Purpose

This roadmap defines the smallest implementation and qualification program
needed to leave the virtio-mem services running unattended on the trusted
development KVM for an extended period.

The target is deliberately narrower than production readiness:

- one trusted Windows guest: `win11_gpu`;
- one active RHEL controller instance;
- one explicitly configured virtio-mem alias: `ua-virtiomem0`;
- production version-2 raw Windows telemetry;
- host-owned target calculation and actuation; and
- automatic growth and reclaim with fail-closed safety controls.

Multi-VM arbitration, untrusted guests, production support, and direct
`viomem.sys` control are outside this gate. `BACKLOG.md` remains the product
execution source of truth. This document owns only the cross-component QA work
needed for the development "leave it running" decision.

## Current qualification state

Status as of 2026-09-10:

- The current local RHEL-compatible gate passes 62 shared-core, 67 host, and 19
  build-tooling tests, including release builds, formatting, warnings-denied
  Clippy, and diff checks.
- The latest recorded native Windows gate passes 74 tests and produced the
  candidate service artifact with SHA-256
  `cbc8a81aa87ba0a3c2c34ad030d85cb5cd0c7f5c3eeed683708daf8a4a154404`.
- The live VM, QGA, `viomem`, and Windows service are running, and the selected
  virtio-mem device is converged.
- The installed Windows and host binaries do not match the current candidates.
- The installed host controller uses `guest-stats` compatibility mode rather
  than the production raw-telemetry path.
- The Windows ProgramData telemetry/configuration files and the cross-guest
  telemetry transport are not deployed.
- The installed compatibility attestation has drifted and correctly rejects
  resize attempts.
- The installed host service is in a recurring systemd restart cycle caused by
  non-advancing `dommemstat` evidence. Its safety gates prevent mutation, but
  the service is not healthy enough for unattended use.
- Live Windows shrink has shown zero progress for small requests and partial
  progress followed by a stall for larger requests.
- No applied M10g resident/committed qualification or extended repeated-cycle
  soak has completed.

The current decision is therefore **NO-GO for unattended automatic resizing**.
Read-only inspection and guarded manual validation remain suitable for normal
development.

## Non-negotiable safety invariants

Every task and qualification run in this roadmap must preserve these rules:

1. Windows publishes measurements only. It receives no host allocation feed
   and has no resize authority.
2. The RHEL controller is the only resize authority.
3. Live alias-scoped libvirt `current` is authoritative allocation state.
4. No ordinary command is issued while `requested != current`. Only the
   documented upward pending-shrink supersession/freeze path is permitted.
5. Every command is block aligned, bounded, headroom checked, freshly
   attested, journaled before actuation, and resolved by a fresh live reread.
6. Missing, stale, replayed, malformed, cross-VM, or provenance-free telemetry
   cannot authorize reclaim.
7. Ambiguous or stalled actuation latches durably and cannot be retried merely
   because a service restarts.
8. Automatic shrink remains enabled by default in the product. An explicit
   `false` setting may pause actuation during deployment or diagnosis.
9. Live tests capture the initial target and define timeout, cleanup, and
   recovery before mutation. A shrink is never described as inherently
   reversible.
10. Only one controller/device may be active for this qualification.

## Gate model

| Gate | Meaning | Exit condition |
| --- | --- | --- |
| QA-G0: Safe hold | The current unhealthy deployment cannot mutate or produce an uncontrolled restart loop | Controller is stopped or deliberately paused while deployment faults are corrected; VM health is confirmed |
| QA-G1: Deployment coherent | Current binaries, configuration, identity, ACLs, attestation, and telemetry transport agree | Both services run current checksum-recorded artifacts and the host accepts fresh production raw telemetry without resizing |
| QA-G2: Bounded actuation qualified | One complete automatic workload cycle is safe and observable | Resident and committed profiles prove growth, falling demand, reclaim/constrained handling, and renewed-pressure behavior |
| QA-G3: Recovery qualified | Expected faults cannot cause replay, overlap, unsafe movement, or silent loss of service | Restart, stale input, command ambiguity, partial/no progress, and cancellation cases have correlated evidence |
| QA-G4: Leave it running | The complete development stack is stable over repeated cycles and idle periods | Soak criteria pass with no unresolved blocker or unexplained restart/memory transition |

## Milestones and tasks

### QA-M0 — Establish a safe deployment baseline

| ID | Task | Owner | Depends on | Status | Required evidence |
| --- | --- | --- | --- | --- | --- |
| QA-T001 | Quiesce the current unhealthy automatic controller while preserving the VM's converged target | Operator + Copilot | — | Ready | Initial `requested`/`current`, VM/QGA/Windows health, unit state, restart count, and explicit no-mutation result |
| QA-T002 | Record the exact installed and candidate host/Windows binary hashes, unit files, configurations, and service identities | Copilot | QA-T001 | Ready | Versioned deployment manifest showing every mismatch and intended replacement |
| QA-T003 | Correct deployment-affecting service defects: durable raw-telemetry acknowledgement flush, SCM service-name consistency, and Windows service error-control value | Copilot | — | Ready | Focused regressions, current local gate, and native Windows gate |

QA-M0 exits when the VM remains healthy and converged, the restart loop is no
longer active, and the exact deployment delta is reviewable. Stopping or
pausing the controller is an operational safety measure, not qualification of
automatic resizing.

### QA-M1 — Deploy the production communication path

| ID | Task | Owner | Depends on | Status | Required evidence |
| --- | --- | --- | --- | --- | --- |
| QA-T004 | Select, document, and implement the least-privilege transport that presents the Windows atomic telemetry record at the configured host path | Copilot + Operator | QA-T002 | Implementation required | Threat/ownership review, bounded failure behavior, identity preservation, atomic host handoff, and no guest actuation authority |
| QA-T005 | Install the current Windows candidate under `LocalService` and verify the ProgramData configuration/telemetry DACL | Operator + Copilot | QA-T003, QA-T004 | Blocked | Installed hash, SCM configuration, exact ACL evidence, service start, two advancing version-2 records, retention, and clean stop/start |
| QA-T006 | Install the current host candidate and repository systemd unit/configuration with production raw demand enabled | Operator + Copilot | QA-T003, QA-T004 | Blocked | Installed hash, one active instance, state/runtime directory ownership, no restart loop, and no `guest-stats` fallback |
| QA-T007 | Calibrate fixed visible base memory and regenerate the M9d attestation against the exact deployed VM/QEMU/libvirt configuration | Operator + Copilot | QA-T006 | Blocked | Recorded baseline, reviewed attestation fingerprint, dry-run decision, and deliberate proof that a changed fingerprint fails closed |
| QA-T008 | Prove the Windows-to-host handoff across independent service restarts without actuation | Copilot + Operator | QA-T005, QA-T006, QA-T007 | Blocked | Fresh/unchanged/new-session handling, retired-session rejection, replay acknowledgement recovery, bounded stale-input logging, and zero resize commands |

The transport is part of product deployment, not a test-only copy step. It
must survive independent guest and host service restarts and must not weaken
the VM/service identity or freshness contract.

QA-M1 exits when the installed host consumes advancing native Windows records
through `TargetDemandSource`, preserves `desired`/`requested`/`current`
separation, and remains stable with actuation deliberately paused.

### QA-M2 — Qualify automatic resize decisions

| ID | Task | Owner | Depends on | Status | Required evidence |
| --- | --- | --- | --- | --- | --- |
| QA-T009 | Run the M10g resident-memory profile through the installed production path | Operator + Copilot | QA-T008 | Blocked | Initial state, workload phases, raw telemetry, estimator state, target decisions, host headroom, request/current transitions, guest health, and final recovery |
| QA-T010 | Run the M10g committed-only profile through the same installed path | Operator + Copilot | QA-T009 | Blocked | Separate physical and commit candidate evidence, bounded growth/reclaim outcome, and no policy use of since-boot commit peak |
| QA-T011 | Prove renewed pressure during an owned pending shrink | Operator + Copilot | QA-T009 | Blocked | One upward freeze/supersession at most, no second lower request, fresh intent journal, and convergence or durable latch |
| QA-T012 | Add a qualification cleanup/recovery result that resolves the captured initial target explicitly | Copilot | QA-T009 | Blocked | The run reports restored, retained-by-policy, constrained-and-latched, or operator-recovery-required; it never silently treats retained growth as success |

A platform shrink need not always reach the requested lower value for the
controller to behave correctly. Zero or partial progress is acceptable only
when the controller preserves the desired target, reports constrained health,
latches at the documented boundary, and neither overlaps nor replays commands.

QA-M2 exits only after both workload profiles have applied-run evidence. A
growth-only result does not pass this milestone.

### QA-M3 — Qualify failure and recovery behavior

| ID | Task | Owner | Depends on | Status | Required evidence |
| --- | --- | --- | --- | --- | --- |
| QA-T013 | Exercise stale/missing/invalid telemetry while converged and while shrink is pending | Operator + Copilot | QA-T009 | Blocked | Non-fatal converged wait; pending-shrink freeze or durable recovery-required latch; no lower request from stale input |
| QA-T014 | Exercise host-controller stop/start with converged state, accepted growth, accepted shrink, and durable latch | Operator + Copilot | QA-T009 | Blocked | No command replay, correct intent resolution from fresh live state, bounded restart, and stable restart counter |
| QA-T015 | Exercise Windows service stop/start and new-session telemetry rollover | Operator + Copilot | QA-T010 | Blocked | Host freezes/waits safely, accepts only the new advancing session, rejects retired-session reuse, and resumes without manual state deletion |
| QA-T016 | Exercise pre-command rejection and command-outcome ambiguity | Operator + Copilot | QA-T013, QA-T014 | Blocked | Rejection performs no mutation; ambiguity is resolved by live reread and latches durably when unknowable |
| QA-T017 | Exercise zero-progress and partial-progress Windows shrink recovery | Operator + Copilot | QA-T011, QA-T012 | Blocked | Constrained state, immutable deadline, no blind re-notification, explicit recovery result, preserved guest/application health |
| QA-T018 | Verify observability and operator response for every terminal state | Copilot + Operator | QA-T013–QA-T017 | Blocked | Correlated operation ID, desired/requested/current, history readiness, latch reason, last-success time, bounded log volume, alert condition, and dry-run-first recovery procedure |

Fault injection must be bounded and must use the documented service/lifecycle
and live-validation safety procedures. Guest or host reboot remains a separate
explicitly approved operation.

QA-M3 exits when each failure produces an actionable state rather than a
restart loop, silent stall, repeated request, or unexplained service stop.

### QA-M4 — Repeated-cycle and endurance qualification

| ID | Task | Owner | Depends on | Status | Required evidence |
| --- | --- | --- | --- | --- | --- |
| QA-T019 | Run at least 20 alternating resident/committed growth-and-release cycles over at least 24 continuous hours | Operator + Copilot | QA-T018 | Blocked | Per-cycle results, no overlap/replay, stable process memory/handles, bounded logs/files, continuous guest/application health, and no unexplained target drift |
| QA-T020 | Run a 72-hour unattended development soak including idle periods and scheduled workload cycles | Operator + Copilot | QA-T019 | Blocked | Continuous health timeline, service restart counts, last-success age, telemetry continuity, all resize/latch transitions, host headroom, and final converged or reviewed-latched state |
| QA-T021 | Review all soak warnings and classify every non-convergence, restart, dropped sample, attestation failure, and operator action | Copilot + Operator | QA-T020 | Blocked | Signed-off issue list with no unknown or unbounded behavior |

The 24-hour cycle run detects cumulative state and repeated-transition defects.
The 72-hour run establishes that the services also remain healthy when demand
does not conveniently align with a test boundary. Short successful runs cannot
substitute for either result.

### QA-M5 — Leave-it-running decision

| ID | Task | Owner | Depends on | Status | Required evidence |
| --- | --- | --- | --- | --- | --- |
| QA-T022 | Publish the development support profile and exact known limitations | Copilot | QA-T021 | Blocked | One-VM/device scope, pinned versions, trust assumption, cgroup-limit decision, default-on/explicit-disable behavior, and unsupported cases |
| QA-T023 | Publish the operator runbook for health checks, alerts, latch diagnosis, pause, recovery, upgrade, and rollback | Copilot | QA-T018, QA-T021 | Blocked | Commands, expected outputs, decision thresholds, bounded recovery, and escalation conditions |
| QA-T024 | Make and record the final leave-it-running decision | Operator + Copilot | QA-T022, QA-T023 | Blocked | Completed checklist below, artifact/evidence index, open-risk acceptance, and explicit GO or NO-GO |

## Final GO checklist

The development KVM may be left running with automatic resizing only when all
of the following are true:

- [ ] Current checksum-recorded Windows and host artifacts are installed.
- [ ] The installed host uses production raw telemetry, not `guest-stats`.
- [ ] The cross-guest transport and both endpoint ACLs are verified.
- [ ] Fixed-base calibration and compatibility attestation match the live VM.
- [ ] The VM and selected virtio-mem device are initially converged.
- [ ] Exactly one controller/device is active.
- [ ] Resident and committed M10g apply runs pass.
- [ ] Growth, falling demand, reclaim or constrained handling, and renewed
      pressure are all observed correctly.
- [ ] Telemetry loss, service restart, command rejection/ambiguity, and
      zero/partial shrink progress pass their recovery gates.
- [ ] No command is replayed or overlapped across any fault or restart.
- [ ] Durable state and replay acknowledgement survive abrupt-process tests.
- [ ] ProgramData, runtime, state, attestation, and executable permissions are
      least privilege.
- [ ] Health/last-success state, restart count, latch reason, and resize events
      are monitorable with documented alert thresholds.
- [ ] The 24-hour repeated-cycle run and 72-hour unattended soak pass.
- [ ] Final state is converged or intentionally latched with a reviewed reason.
- [ ] No unexplained warning, restart, target transition, or guest health event
      remains.
- [ ] Upgrade, explicit automatic-shrink pause, dry-run latch clearing, and
      rollback procedures are documented and rehearsed.

Any unchecked item keeps the result at **NO-GO**. A fail-closed rejection is a
safety success but does not by itself establish service availability or
long-running readiness.

## Evidence and reporting rules

For every task, record these layers separately:

1. focused Rust tests for changed logic;
2. `cargo xtask gate local` for the RHEL-compatible workspace;
3. the native Windows gate when Windows code or the installed artifact changes;
4. dry-run workflow evidence;
5. bounded live apply evidence; and
6. endurance evidence.

Do not convert an unrun layer into a pass. Store task-scoped evidence under the
ignored `.vscode-artifacts/` hierarchy and summarize durable outcomes in
`BACKLOG.md`, `PROJECT_STATUS.md`, and the affected contract/testing documents.
Do not store credentials, private keys, production data, or raw guest memory
contents in qualification artifacts.

## Dependency path

```text
QA-M0 safe baseline
    -> QA-M1 coherent production deployment
        -> QA-M2 bounded automatic resize
            -> QA-M3 failure and recovery
                -> QA-M4 repeated-cycle and endurance soak
                    -> QA-M5 leave-it-running decision
```
