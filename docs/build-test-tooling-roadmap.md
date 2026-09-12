# Build and test tooling roadmap

This is the execution board for repository build, test, deployment-validation,
and live-workflow tooling. Product work remains in `../BACKLOG.md`.

## Target architecture

`cargo xtask` is the single maintained control plane for repository gates,
native-Windows orchestration, artifact verification, environment checks, and
repeatable cross-process/live workflows. Normal Rust tests remain in their
owning crates. Editor and Make entrypoints are thin delegates.

Operational targets, paths, sizes, timings, retry counts, and acceptance
thresholds are explicit validated inputs or are derived from authoritative
current state. No stable command or profile is named after a milestone or VM.

Privileged RHEL work uses a reviewed capability-named xtask command. The
unprivileged process validates its scope, re-executes the current prebuilt
binary through one outer `sudo`, and persists typed evidence without generating
shell or running Cargo as root.

## Milestones

| Milestone | Outcome | Status |
| --- | --- | --- |
| BT-M1 | Rust `xtask` workspace foundation | Complete |
| BT-M2 | Local RHEL-compatible repository gate | Complete |
| BT-M3 | Explicit remote native-Windows gate and artifact verification | Complete |
| BT-M4 | Host readiness, QGA, and reversible live-resize workflows | Complete |
| BT-M5 | Editor, Make, docs, and AI guidance route to current tooling | Complete |
| BT-M6 | Typed deployment manifest and privileged xtask boundary | Ready |
| BT-M7 | Injectable external-effect tests and common result schema | Ready |

## Completed tasks

| Task | Outcome |
| --- | --- |
| BT-T001–BT-T006 | Replaced maintained shell wrappers with capability-named Rust commands and routed repository entrypoints to them |
| BT-T013 | Separated code-level tests, tooling development, and higher-level workflow prompts |
| BT-T014 | Added detached resident/committed automatic-controller qualification with durable JSON/JSONL evidence, production QGA telemetry observation, and a bounded exclusive controller guard |
| BT-T015 | Removed experiment-derived workflow and policy defaults, milestone/VM-specific command profiles, obsolete editor/debug artifacts, generated build output, duplicated AI instructions, historical test procedures, and unused guest-side resize compatibility code; live mutation now delegates to the attestation-aware product CLI |
| BT-T016 | Replaced the QA-T002 privileged Bash inventory with typed `cargo xtask deployment inventory`, bounded fixed-vector inspection, unprivileged atomic JSON evidence, and focused parser/elevation tests |

## Ready queue

| Task | Outcome required | Depends on |
| --- | --- | --- |
| BT-T007 | Versioned deployment/task manifest with explicit identity, inputs, time bounds, effects, evidence, cleanup, and rollback | BT-M4 |
| BT-T008 | Typed privileged xtask execution from the validated manifest | BT-T007 |
| BT-T009 | Reusable typed guest-health and lifecycle evidence | BT-T007 |
| BT-T010 | Injectable process, filesystem, SSH, timeout, cancellation, cleanup, and rollback fault matrix | BT-T007–BT-T009 |
| BT-T011 | Common versioned JSON plus human result model for every higher-level workflow | BT-T010 |
| BT-T012 | Repeatability qualification using an explicitly selected run count and current endpoint configuration | BT-T011 |

## Current gaps

- AR1 inventory, Windows/host installation, service lifecycle, diagnostics,
  calibration, attestation, and no-actuation preflight now have typed xtask
  boundaries. They still need unification under the general versioned task
  manifest and common result model tracked by BT-T007 and BT-T011.
- Guest lifecycle evidence is captured by current qualification preflight but
  is not yet a reusable typed command. The qualification-specific privileged
  guard is typed and bounded but still awaits unification under BT-T007's
  general task manifest.
- Some external effects remain covered only at parser/unit boundaries rather
  than through injected end-to-end fixtures.

## Definition of done

The control plane is complete when every repeatable repository, native,
deployment, service, and live workflow has one capability-named Rust entrypoint
with explicit inputs, deterministic failure tests, versioned evidence, and
defined cleanup/rollback; privileged phases re-execute prebuilt typed Rust
behavior; and unavailable platform layers remain distinct from passing results.
