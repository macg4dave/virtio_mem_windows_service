# Build and Test Tooling Roadmap

This roadmap is the execution source of truth for repository build and test
tooling. Product/runtime work remains in [`../BACKLOG.md`](../BACKLOG.md) and
[`roadmap.md`](roadmap.md).

Tooling uses the visually distinct namespaces **BT-Mx** for milestones,
**BT-Txxx** for tasks, and **BT-Bxxx** for blockers. Main-project `M*` and
`TASK-*` codes must never be reused here.

## Status legend

- `[ ]` Not started
- `[~]` In progress or partially complete
- `[x]` Complete with local evidence and documentation
- `[!]` Blocked by an external dependency

## Goal and boundary

The repository will have one Rust control-plane tool, invoked as
`cargo xtask`, for reproducible builds, tests, environment checks, remote
native-Windows validation, artifact verification, and reusable bounded live
probes. Rust owns parsing, validation, command construction, orchestration,
and reusable safety policy.

Bash remains permitted only where its process-level form is itself a safety
boundary: a generated, task-specific, operator-reviewable privileged batch
under the ignored `.vscode-artifacts/privileged-tasks/` directory. Those
batches must call `cargo xtask` or installed Rust binaries for reusable logic;
they must not grow a second XML, JSON, arithmetic, polling, or build system.

Historical evidence and task scripts under `.vscode-artifacts/` are not source
code and are not synchronized by default. They remain immutable evidence until
their related main-project retention decision permits deletion.

## Current position

BT-M0 through BT-M5 are implemented in the current migration. The tracked
`scripts/` wrappers have been replaced by `tools/xtask`, Cargo/VS Code/Make
entry points route to that tool, and current AI instructions use its commands.
BT-M6 and BT-M7 remain: task-batch generation and shared guest-health evidence
need a typed model, followed by fault-injected end-to-end qualification and a
stable machine-readable result schema.

Current local evidence is 62 shared-core, 67 host, and 13 tooling tests plus
locked release builds, formatting, warnings-denied Clippy, diff validation,
and a passing host doctor. The native Windows gate passes 67 tests and verifies
artifact SHA-256
`607c2baed981e4807f082341aacb7d80ec846190c50d3b8f26728656aa55d899`.
No live mutation gate was run for BT-T001–BT-T006.

## Milestone map

| Milestone | Title | Status | Depends on | Exit evidence |
| --- | --- | --- | --- | --- |
| BT-M0 | Inventory and ownership boundary | [x] | — | Every tracked and ignored script is classified; duplicate logic and retained one-off evidence are documented |
| BT-M1 | Rust tooling foundation | [x] | BT-M0 | `tools/xtask` is a workspace member, `cargo xtask help` works, and argument/parser tests pass |
| BT-M2 | Local developer gate | [x] | BT-M1 | Format, release build, test, Clippy, and diff checks run through `cargo xtask gate local` without compiling Windows-only APIs on Linux |
| BT-M3 | Native Windows build gate | [x] | BT-M1 | Explicit SSH check/sync/build/test/lint/fetch/all and fingerprint-pinned two-run evidence are implemented with checksum verification |
| BT-M4 | Host and live validation | [x] | BT-M1 | Host doctor, QGA/dommemstat fallback, and dry-run-default bounded resize reuse Rust JSON/XML/unit validation |
| BT-M5 | Entrypoint and guidance migration | [x] | BT-M2, BT-M3, BT-M4 | Tracked shell wrappers are removed; VS Code, Make, docs, project instructions, and AI prompts point to `cargo xtask` |
| BT-M6 | Privileged-batch consolidation | [ ] | BT-M4, M10g | A typed manifest and generator replace copied task boilerplate while retaining one outer reviewable Bash batch per privileged task |
| BT-M7 | Toolchain qualification and stable output | [ ] | BT-M5, BT-M6 | Fault injection, cross-platform endpoint tests, result schema/versioning, artifact retention, and two consecutive aggregate gates pass |

## Task board

### Completed

| Task | Milestone | Description | Evidence |
| --- | --- | --- | --- |
| BT-T001 | BT-M0 | Audit tracked scripts, ignored privileged task scripts, editor tasks, Make targets, docs, and AI prompts | Inventory and disposition tables below |
| BT-T002 | BT-M1 | Add the workspace `virtio-mem-xtask` crate and Cargo alias | Hermetic parser/safety tests and `cargo xtask help` |
| BT-T003 | BT-M2 | Consolidate the Linux-supported quality gate | One command preserves format, locked release build/test/Clippy, and diff checks |
| BT-T004 | BT-M3 | Port Windows SSH/MSVC orchestration and milestone evidence capture | Explicit endpoint, pinned-host option, one-sync gate, and cross-host SHA-256 comparison |
| BT-T005 | BT-M4 | Port environment, QGA, and live-resize helpers | Rust parses QGA JSON and libvirt XML; live mutation stays opt-in, bounded, converged, and restored by default |
| BT-T006 | BT-M5 | Migrate repository entry points and remove tracked wrappers | `.vscode/tasks.json`, Make compatibility aliases, project docs, instructions, and prompts use `cargo xtask` |

### Ready queue

| Task | Milestone | Description | Depends on | Blockers | Exit evidence |
| --- | --- | --- | --- | --- | --- |
| BT-T007 | BT-M6 | Define a versioned task manifest for exact VM, service, guest-health, timeout, output, mutation, and rollback scope | BT-T005 | BT-B002 | Schema rejects implicit targets, unbounded waits, nested privilege elevation, and missing rollback |
| BT-T008 | BT-M6 | Generate minimal reviewable privileged Bash batches from a validated manifest | BT-T007 | BT-B001 | Generated batch contains fixed arguments, one outer execution boundary, and no duplicated parsers/policy |
| BT-T009 | BT-M6 | Add reusable layered Windows guest-health capture and quiet-window evaluation | BT-T007, M10g | BT-B002 | Deterministic fixtures cover reboot identity, services, installer/pending-reboot checks, and event IDs 41/6008/1001/1074 |
| BT-T010 | BT-M7 | Add injectable process, filesystem, SSH, checksum, timeout, cancellation, and rollback integration tests | BT-T003–BT-T009 | — | Failure matrix proves non-zero propagation, no implicit endpoint, cleanup, ambiguous-command reporting, and rollback attempts |
| BT-T011 | BT-M7 | Define versioned JSON plus human-readable summaries for gate evidence | BT-T010 | — | RHEL and Windows results stay separate and are reproducibly archived |
| BT-T012 | BT-M7 | Run two consecutive aggregate gates and one bounded dry-run/live-fixture qualification | BT-T011 | BT-B003 | Exact logs, hashes, test counts, platform labels, and any live blocker are recorded |

## Dependency path

```text
BT-M0 → BT-M1 ─┬→ BT-M2 ─┐
               ├→ BT-M3 ─┼→ BT-M5 → BT-M7
               └→ BT-M4 ─┘          ▲
                         └→ BT-M6 ────┘
```

BT-M6 may consume live-health requirements from main-project M10g, but tooling
task/status codes remain independent. Completing a BT milestone never marks a
main-project milestone complete.

## Blockers and decisions

| Blocker | Status | Impact | Resolution |
| --- | --- | --- | --- |
| BT-B001 | Design constraint | Privileged work must remain one exact, operator-reviewable process under one outer `sudo`; a long-running generic privileged daemon is out of scope | Generate a minimal Bash batch from a typed manifest and keep reusable behavior in Rust |
| BT-B002 | Main-project dependency | Guest lifecycle/health semantics are still being qualified under M10g | Freeze a versioned tooling schema only after the required evidence fields and quiet-window rules stabilize |
| BT-B003 | External environment | The complete native Windows gate requires the explicitly configured SSH/MSVC endpoint | Report native Windows separately; never substitute a Linux workspace result |

## Existing script audit

### Tracked `scripts/` wrappers

| Former file | Overlap found | Disposition |
| --- | --- | --- |
| `scripts/build-rust.sh` | Duplicated Make targets, VS Code tasks, prompt commands, and documentation | Replaced by `cargo xtask gate local`; removed |
| `scripts/check-environment.sh` | Command-presence loop duplicated by several live helpers | Replaced by `cargo xtask doctor host`; removed |
| `scripts/windows-remote-build.sh` | Duplicated VS Code Windows stages and milestone runner orchestration | Merged into `cargo xtask windows check|sync|build|test|lint|fetch|all`; removed |
| `scripts/complete-windows-build-milestone.sh` | Wrapped the Windows helper plus `make all-gates`, repeated SSH and hashing logic | Merged into `cargo xtask windows milestone`; removed |
| `scripts/validate-guest-agent.sh` | QGA/`dommemstat` parsing overlaps M8 read-only evidence and host adapters | Replaced by `cargo xtask qga`; Rust now parses JSON and numeric fallback fields; removed |
| `scripts/live-resize-test.sh` | Reimplemented XML selection, units, target checks, headroom, polling, and rollback already represented in Rust contracts | Replaced by `cargo xtask live-resize`; shared core parsing and validation are authoritative; removed |

The Makefile remains only as a human compatibility shim. It contains no build
or validation policy: its aggregate targets delegate to `cargo xtask`.

### Ignored `.vscode-artifacts/privileged-tasks/` scripts

These are not maintained entry points. They embed historical task targets and
capture evidence, so deleting or silently rewriting them would damage audit
context. Their reusable portions are migration input for BT-M6; their specific
mutation sequences remain separate.

| Scripts reviewed | Function and overlap | Decision |
| --- | --- | --- |
| `m8-win11-gpu-read-only.sh`, `m9-win11-gpu-read-only.sh`, `m9a-win11-gpu-discovery.sh`, `m9a-win11-gpu-validate.sh` | Repeated QGA, XML snapshot/hash, alias validation, and non-mutation checks | Obsolete as reusable tooling; retain as historical evidence. Use `cargo xtask qga` and the Rust host CLI/tooling now |
| `m8-win11-gpu-lifecycle.sh`, `m10b-guest-maintenance-settle.sh`, `m10b-interruption-attestation-validation.sh` | Repeated QGA availability, SSH Windows-service health, boot identity, event-log, installer, quiet-window, and reboot checks | Keep task sequences separate; merge reusable health capture/evaluation under BT-T009 |
| `m9b-win11_gpu.sh`, `deploy-memory-quanta-policy.sh`, `m10b-service-recovery-validation.sh`, `m10ef-candidate-service-live.sh` | Repeated service stop/stage/start/observe/restore and controller-state parsing | Keep exact deployment evidence; replace boilerplate with BT-T007/BT-T008 manifests and generated batches |
| `m10a1-qualify-dbgview.sh`, `m10a1-unload-dbgv-recovery.sh` | Signed diagnostic staging, hashing, Windows driver-service cleanup, and evidence capture | Diagnostic-only and no longer a normal gate; retain until M10a evidence retention expires, then remove as obsolete |
| `m10a3-one-block-observation.sh`, `m10a3-domain-restart-restore-controller.sh`, `m10a3-post-host-reboot-recover-retry.sh`, `m10a3-reboot-clean-retry.sh` | One-off driver observation and progressively corrected recovery/reboot procedures; duplicates state polling and guest checks | Historical only; do not generalize the obsolete driver-capture path. Migrate health/state primitives, not the scenarios |
| `m10b-256m-ramp.sh`, `m10b-grow-2g-shrink-1g-probe.sh`, `m10b-one-block-qualification.sh`, `recover-failed-64m-shrink.sh` | Repeated resize, convergence, service isolation, logging, and recovery | Historical M10 evidence; new ordinary reversible tests use `cargo xtask live-resize`. Complex recovery stays task-specific until BT-M6 |

Non-script files under `.vscode-artifacts/`—logs, hashes, binaries, captured
configuration, and diagnostic packages—are evidence or generated output, not
tooling. They remain ignored and are not moved into `tools/`.

## Definition of done

The Rust toolchain migration is fully consolidated when:

1. all maintained build/test/inspection entry points route through
   `cargo xtask` or a product Rust binary;
2. no tracked Bash file duplicates Rust parsing, policy, command construction,
   polling, checksums, or gate sequencing;
3. privileged Bash is generated from a typed, explicit-scope manifest and is
   used only as the one-process operator review/elevation boundary;
4. local RHEL/core-host and native Windows results are labelled and archived
   independently;
5. hermetic failure injection covers subprocess failure, timeout, malformed
   output, cancellation, ambiguous live mutation, rollback, and cleanup; and
6. `cargo xtask gate all` passes twice against the pinned native-Windows
   endpoint with identical verified artifacts, or reports BT-B003 exactly.
