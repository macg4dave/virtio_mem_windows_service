# BACKLOG

## 2026-08-17 Documentation Handoff

Imported applicable Windows service guidance from Microsoft Learn into
`docs/architecture.md`, `docs/engineering-standards.md`, `docs/testing.md`,
and `windows/README.md`. The documented lifecycle contract is intended for
the remaining SCM adapter work; no unfinished SCM capability is marked as
complete by this documentation update.

Execution source of truth. Update after every session.

## 2026-08-18 Windows service runtime boundary fix

- Confirmed the running `QEMU-GA` service owns the Windows virtio-serial
  endpoint; a second guest process receives `open ...: 5`.
- Added `NativeTelemetryWorker` and wired interactive/SCM startup to native
  `GlobalMemoryStatusEx`/`GetPerformanceInfo` collection, so Windows service
  startup no longer depends on opening the QGA device.
- Retained the QGA client and parser as explicit adapter/test boundaries;
  RHEL/libvirt remains the owner of host-side QGA requests.
- Added native-worker startup/failure regression tests. Workspace tests now
  pass with 71 tests and no failures.

## 2026-08-18 running-service RHEL handoff

- The guest is running and the QGA channel responds to `guest-info` and
  `guest-get-host-name` (`ICE101`). The RHEL host cannot directly observe
  Windows SCM state; service-running confirmation remains Windows-side
  evidence.
- `guest-get-memory-stats` remains unavailable, but this is not a Windows
  service startup blocker because the current service uses native
  `GlobalMemoryStatusEx`/`GetPerformanceInfo` telemetry and does not open the
  QGA virtio-serial device.
- `virsh dommemstat win11_gpu` remains available. Fresh live XML still reports
  virtio-mem `requested=0 KiB` and `current=18432 KiB`, so host resize testing
  remains blocked until convergence; no resize or other VM mutation was
  attempted.

## 2026-08-18 Windows QGA device-path fix

- Foreground service diagnostics reproduced startup failure as
  `wait for \\.\Global\org.qemu.guest_agent.0: 161`.
- Fixed `windows/src/qga.rs` to skip `WaitNamedPipeW` for the QEMU
  virtio-serial device path under `\\.\Global\`; `WaitNamedPipeW` only accepts
  `\\.\pipe\...` endpoints and returned `ERROR_BAD_PATHNAME`.
- Added a regression test distinguishing Win32 named-pipe paths from the
  QEMU device path. Formatting, Clippy, and all 69 workspace tests pass.
- Rebuild and redeploy the Windows service binary before repeating SCM start.

## 2026-08-18 guest-get-memory-stats clarification

- Confirmed that `guest-get-memory-stats` is already implemented end to end in
  the repository: the Windows named-pipe client sends the newline-delimited
  request, the shared parser validates `stat-free`, `stat-total`, and optional
  `stat-available`, and the host `virsh` adapter sends the same exact command.
- Workspace validation passed with 78 tests and no failures.
- No Rust change is required to add the command. The connected external QEMU
  Guest Agent reports the command as unavailable, so enabling it requires
  installing/upgrading/configuring a QGA build that implements the command on
  the Windows guest. Until then, retain the host `dommemstat` fallback and do
  not claim live QGA memory-stat support.

## 2026-08-18 Windows QGA/build handoff

- Confirmed `windows/src/qga.rs` already sends the newline-delimited
  `guest-get-memory-stats` request with a bounded overlapped named-pipe
  operation; no repository-side QGA request implementation is missing.
- Built the RHEL host controller successfully as
  `target/release/virtio-mem-host`; `cargo test -p virtio-mem-host` passed all
  14 tests.
- A Windows service artifact could not be produced on this RHEL host because
  `x86_64-pc-windows-gnu` is not installed and no MinGW, Clang, LLD, or MSVC
  linker is available. Build `virtio-mem-service` on the Windows guest or a
  Windows build host, then validate the QGA command there.
- The live QGA still does not advertise `guest-get-memory-stats`; installing
  or updating the external Windows QEMU Guest Agent remains required for
  native QGA memory-stat support. The host `dommemstat` fallback remains the
  default and is already implemented.

## 2026-08-18 privileged-probe batching handoff

- Strengthened repository agent guidance so related privileged read-only host
  probes are approved and executed as one batch under one outer `sudo`.
- The agent must not invoke `sudo` once per `virsh`/systemd probe, depend on
  sudo timestamp caching, or retry authorization separately for each command.
- Passwords remain operator-entered directly into the terminal and are never
  requested, stored, or transmitted to the agent.

## 2026-08-18 RHEL read-only validation handoff

- A single unprivileged read-only batch completed without repeated password
  prompts: `bash scripts/check-environment.sh`, `virsh version`, domain state,
  `dommemstat`, XML, QGA capability checks, systemd status, and journal read.
- `bash scripts/check-environment.sh` passed on the RHEL host.
- `bash scripts/validate-guest-agent.sh win11_gpu 3` reached `guest-info`,
  then failed closed because QGA 109.1.0 does not implement
  `guest-get-memory-stats`.
- `virsh dommemstat win11_gpu` succeeded with `actual=8388608 KiB`,
  `unused=4137384 KiB`, and `available=8337708 KiB`; the default
  `dommemstat` source therefore has the required fields for this guest.
- `virtio-mem-host@win11_gpu.service` is not installed and has no journal
  entries. Installing it remains an explicitly approved mutation.
- A fresh read-only recheck still reports `requested=0 KiB` and
  `current=18432 KiB`; the rollback blocker is not resolved. QGA responses do
  not expose Windows driver `requested_size`/`plugged_size` fields, so any new
  driver evidence must come through a separately validated driver observation
  path.
- `virsh dumpxml win11_gpu` reports alias `ua-virtiomem0`, size
  `20971520 KiB`, block `2048 KiB`, `requested=0 KiB`, and
  `current=18432 KiB`. The rollback incident remains unresolved and no
  resize, service, systemd, or VM mutation was attempted.
- Corrected the host XML discovery contract to use `virsh dumpxml <vm>`;
  this libvirt version rejects the unsupported live-option form.
- Hardened `dommemstat` parsing to reject KiB-to-byte overflow instead of
  saturating, with regression tests.

## 2026-08-18 Documentation synchronization

- Synchronized `PROJECT_STATUS.md` and `IMPLEMENTATION_PLAN.md` with the
  current Phase 2 roadmap and backlog; removed obsolete task numbering and
  early-scaffold claims.
- Corrected the feature matrix and API contract to describe implemented native
  telemetry, advisory demand reports, and validated JSON configuration while
  retaining live-validation and production-wiring gaps.
- Corrected roadmap milestone wording so operation deadlines and configured
  shutdown-timeout enforcement remain explicitly open.
- Replaced the root `readme.md` with a user-facing guide covering project
  goals, architecture, current status, safety boundaries, quick start, and
  links to the roadmap and supporting contracts.
- Local documentation and Rust validation remain required before marking any
  implementation milestone complete.
- Added a version-2 QGA operation timeout and native overlapped named-pipe
  boundary for connect/write/read. A timed-out request now calls `CancelIoEx`,
  closes its handles, and returns an explicit transport error without using
  the non-cancellable synchronous flush API.
- Added a bounded `ServiceHost` worker boundary using the configured shutdown
  timeout, typed timeout failure, SCM timeout wiring, and deterministic slow-
  worker tests. Live install/start/stop observation remains separate.

## 2026-08-18 Windows demand-agent handoff

- Added `windows/src/demand.rs` with native `GlobalMemoryStatusEx` and
  `GetPerformanceInfo` collection, checked canonical-byte counters, a versioned
  advisory demand report, bounded provisional pressure states, aligned target
  limits, and a conservative safe floor.
- Added deterministic tests for invalid counters, pressure bounds, state
  classification, target clamping/alignment, and invalid current allocation.
- The collector/report are not wired to host actuation. QGA/dommemstat and the
  existing host controller remain unchanged and authoritative for resize.
- Added a one-cycle `DemandAgent` collection/publication boundary with injected
  publisher, explicit telemetry/publication errors, and deterministic tests.
- Added validated durable JSON-lines output and a generic stoppable advisory
  worker; the configured path defaults under `C:\ProgramData`.
- Added versioned JSON service configuration loading with validated defaults
  when the file is absent, explicit schema rejection, and startup integration.
- Live Windows workload evidence, persistent/event report output, and main
  SCM construction remain open until a trustworthy current-allocation provider
  and its service-account ACLs are validated.

## Documentation Freshness Rules

After completing any task:

1. Update the task card status below
2. Update [docs/feature-matrix.md](docs/feature-matrix.md) if features changed
3. Update [docs/roadmap.md](docs/roadmap.md) if phase status changed
4. Update [docs/issues.md](docs/issues.md) if bugs were resolved
5. Document any handoff notes or blockers in the task card
6. Move completed tasks to the **Completed** section

## Ready Queue

Tasks ready to start (Phase 2 - Core Functionality):

| ID | Title | Owner | Status | Effort | Dependencies |
| --- | --- | --- | --- | --- | --- |
| TASK-002 | QEMU Guest Agent validation | Copilot | Blocked | 2-3 hours | Live RHEL/libvirt host and Windows guest unavailable in this environment |
| TASK-004 | Windows memory polling policy | Copilot | In Progress | 2-3 hours | Parser, policy, adapter-based loop, and stoppable interval scheduler implemented; Windows service hosting remains |
| TASK-005 | Safe QEMU Guest Agent response handling | Copilot | In Progress | 2-3 hours | Parser, typed poll errors, configurable named-pipe client, version-2 operation deadline, and native overlapped cancellation implemented; captured-traffic and live transport validation remain. |
| TASK-007 | Documentation review of libvirt/QEMU virtio-mem constraints | Copilot | Ready | 1-2 hours | No code changes; use official virtio-mem guidance to tighten service and validation docs |
| TASK-009 | Windows native demand-agent foundation | Copilot | In Progress | 2-3 hours | M4/M6 local service foundation; live workload evidence and runtime publication remain |

## In Progress

| ID | Title | Owner | Status | Handoff Notes |
| --- | --- | --- | --- | --- |
| TASK-001 | Rust service scaffolding | Copilot | In Progress | Parser, named-pipe QGA client, wakeable scheduler, portable service host, validated service configuration, SCM dispatcher, install/start/stop/remove commands, canonical byte-based VirtioMemState validation, captured libvirt XML parsing, injectable XML state-provider boundary, and a deterministic local service runtime harness are locally covered; live VM evidence, service registration, and QGA validation remain. |
| TASK-008 | RHEL virtio-mem host controller | Copilot | In Progress | Added the workspace and shared Rust core; bounded argument-safe `virsh` QGA/XML/resize adapters; alias-selected live XML parsing; convergence suppression; signal-driven systemd runtime; unit/configuration artifacts; and regression tests. Workspace format, release build, 70 tests, and Clippy warnings-as-errors pass locally. Live RHEL/libvirt validation, service-account authorization, compatibility gate, and reversible resize evidence remain required before enablement. |
| TASK-009 | Windows native demand-agent foundation | Copilot | In Progress | Native telemetry, canonical-byte validation, version 1 advisory report, provisional five-state demand classification, bounded aligned target recommendations, safe-floor recommendations, durable JSON-lines output, and a generic stoppable worker are implemented. Main SCM construction, trustworthy allocation provider, ProgramData ACL setup, live workload tuning/evidence, event-log integration, and any host integration remain intentionally deferred. |

### 2026-08-18 live KVM handoff

- First read-only checks against `win11_gpu` on the RHEL server passed for
  `virsh`/`jq`; libvirt reports 11.10.0, the guest is running, and the QGA
  channel `org.qemu.guest_agent.0` is connected.
- `guest-info`, `guest-ping`, `guest-get-osinfo`, and
  `guest-get-host-name` returned valid responses. The guest reports QGA
  `109.1.0`, Windows 11 x64, and hostname `ICE101`.
- The repository probe is blocked because this agent does not advertise or
  implement `guest-get-memory-stats`; it returns `has not been found`. No
  guest command execution, reboot, resize, or XML mutation was attempted.
- Read-only XML inspection found virtio-mem alias `ua-virtiomem0`, size 20 GiB,
  block 2 MiB, and `requested=current=0`. Automatic resize remains disabled.
- `qemu-system-x86_64` is not in the current PATH; verify the host package/path
  separately before relying on direct QEMU CLI diagnostics.

### 2026-08-18 Windows SCM validation handoff

- The release service binary built successfully.
- The approved `VirtioMemService` install attempt could not open the Windows
  SCM because the current terminal lacks service-manager permissions.
- A read-only `sc.exe query VirtioMemService` confirmed no partial service
  registration exists. Do not mark F7/M7 complete until an administrator runs
  the documented install → start → observe → stop → remove sequence.
- Follow-up administrator run used `sc.exe` correctly: registration succeeded,
  but `sc.exe start VirtioMemService` returned `ERROR_ACCESS_DENIED (5)` and
  the service remained stopped. `sc.exe stop` returned the expected `1062`
  because it was never started; removal then succeeded. The likely deployment
  blocker is the default `LocalService` account lacking traversal/read access
  to the executable under `C:\Users\Dave\github`; validate this before changing
  the least-privilege account or service security descriptor.
- Elevated VS Code rerun confirmed the same result with explicit `sc.exe`:
  release build, install, query, stop, and removal completed; start failed with
  error 5. Read-only `icacls` confirmed the executable grants access to SYSTEM,
  Administrators, and the developer account, but not `NT AUTHORITY\LocalService`.
  No ACL was changed automatically. The service registration was removed after
  the test.

### 2026-08-18 Windows SCM deployment follow-up

- The release binary was copied to `C:\Program Files\VirtioMemService` and
  `LocalService` was granted recursive read/execute access.
- The service reached `RUNNING` with a reported PID, stopped cleanly, and was
  removed successfully. The final `sc.exe query` returned expected error
  `1060`.
- Event-log visibility, recovery actions, and QGA access under `LocalService`
  remain open.

### 2026-08-18 M6/F8 runtime wiring

- Interactive and SCM workers now construct `NamedPipeGuestAgent` from the
  validated configuration, apply the configured QGA operation timeout, and
  acquire/parse memory statistics during initialization and polling.
- QGA transport/parser failures now fail the worker visibly; no resize sink is
  connected and no virtio-mem `current` allocation is inferred from QGA stats.
- Deterministic worker tests cover successful initial acquisition and explicit
  transport failure. A trustworthy current-allocation provider and production
  resize sink remain required before actuation.

### 2026-08-18 Windows service hardening

- SCM installation now applies the configured service description and bounded
  restart actions at 5, 30, and 60 seconds with a 24-hour reset period.
- Failure actions are enabled for non-crash failures so unexpected worker
  exits can be recovered without treating an intentional stop as a crash.
- Live recovery behavior, Event Log visibility, and QGA access under
  `LocalService` remain validation work; no automatic recovery fault was
  induced during this session.
- Corrected the VS Code SCM task artifact path: workspace release builds are
  emitted under `target\release`, not `windows\target\release`. The corrected
  binary showed the configured description and 5/30/60-second failure actions;
  startup reached `START_PENDING` and then stopped with exit code 1 because the
  guest QGA memory-stat command is unavailable. Cleanup removed the service.

### 2026-08-18 shell-safety and privilege handoff

- Added repository-wide prompt rules requiring explicit current-turn approval
  before editing/deleting protected server files or mutating VM, libvirt, or
  systemd state.
- Prompt rules forbid `sudo`, `su`, `doas`, password collection, and password
  automation. Normal Cargo/Bash validation remains unprivileged.
- Documented the recommended one-time administrator setup for a dedicated
  least-privilege `virtio-mem-host` account instead of repeated root prompts or
  broad passwordless sudo access.
- Added `scripts/preview-memory-decision.sh`, a read-only policy preview that
  reports no-change, blocked, grow, or shrink decisions and never issues a
  live resize command.
- Added `scripts/live-resize-test.sh`, which requires explicit `--apply`, logs
  requested/current convergence over time, and restores the original size by
  default after a successful test.
- Added explicit `--connect` support after the first server shell was found to
  default to the empty `qemu:///session` connection while the VM lives under
  `qemu:///system`.
- Updated prompt and testing rules: privileged scripts require current-turn
  approval naming the complete command, target, mutation, and rollback, then
  run once as a whole under `sudo`; passwords remain operator-entered only.
- The approved 20 GiB attempt was rejected by `virsh` before mutation because
  the adapter passed canonical bytes to a KiB-valued `--requested-size` option.
  The VM remained at `requested=current=0`; the script and Rust resize sink now
  convert exact byte values to KiB and reject lossy conversions.
- Incident follow-up: the live test now rejects full-device targets, defaults
  to an 8 GiB target cap, and requires 4 GiB host `MemAvailable` headroom after
  the increase. Automatic controller operation remains disabled pending an
  equivalent host-capacity safety gate.
- Incident log review showed the 20 GiB request converged, while rollback was
  still pending for about 90 seconds before the VM later returned to zero.
  Rollback timeout is now a hard test failure; the harness no longer reports
  restoration unless `requested == current` is confirmed.
- Follow-up read-only check: `win11_gpu` is running on `qemu:///system`, its
  QGA channel is connected, and virtio-mem remains `requested=current=0`. The
  host controller systemd unit is not installed/running, and QGA still lacks
  `guest-get-memory-stats`; the Windows service cannot be confirmed through
  the non-command QGA probes. The hardened 20 GiB dry run returned blocked
  before any mutation.
- The live test now separates forward convergence timeout from rollback
  timeout. A 30-second test timeout cannot shorten the default 300-second
  rollback window.
- Approved 1 GiB test evidence: the forward request converged from 0 to 1 GiB
  in about 5 seconds, but rollback to 0 did not converge within the 300-second
  rollback window. The VM currently reports `requested=0` and
  `current=18432 KiB` (18 MiB), remains running, and must not receive another
  resize until convergence and the rollback failure are understood.
- AI-run live tests now have a hard 30-second forward timeout and compact
  terminal output; detailed polling remains in the optional CSV log. Rollback
  retains a separate 300-second default.
- Added a fixed 1 GiB retention floor to the live test: smaller targets are
  rejected and rollback never requests below 1 GiB, avoiding the previous
  zero-memory rollback path.

### 2026-08-18 host controller stats-source and safety-gate hardening

- Fixed ISSUE-001 in code: `HostConfig` now selects a memory-stat source with
  `VIRTIO_MEM_STATS_SOURCE` (`dommemstat` by default, `qga` opt-in). The new
  `host/src/dommemstat.rs` reads virtio-balloon-backed `virsh dommemstat`
  counters (`actual`/`unused`/`available`) so the controller no longer depends
  on the unimplemented `guest-get-memory-stats` QGA command. Whether the
  connected guest's balloon driver actually reports `unused`/`available` on
  `win11_gpu` still needs a live, read-only `virsh dommemstat win11_gpu` check
  before automatic operation is enabled.
- Added a hard `MIN_HEADROOM_BYTES` (1 GiB) invariant to the shared
  `VirtioMemState::validate_target` in `virtio-mem-core`, so no resize target
  (Windows or host) can ever be validated within 1 GiB of the device's full
  size, independent of operator-configured `max_memory_bytes`.
- Added a host-side memory-headroom gate: `host/src/host_memory.rs` reads
  `/proc/meminfo`'s `MemAvailable`, and `HostRuntime` now skips (does not
  error, just logs and waits) any grow decision unless the RHEL host has
  enough free memory for the requested delta plus the new
  `VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES` reserve, mirroring
  `scripts/live-resize-test.sh --host-reserve-bytes`.
- Workspace `cargo fmt --all -- --check`, `cargo test --workspace`, and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` all
  pass locally after these changes (52 tests).
- Not done in this session (requires live RHEL/libvirt access and explicit
  operator approval, which this session did not have): building/installing
  the `virtio-mem-host` systemd unit, creating the least-privilege
  `virtio-mem-host` account, and running a live 1 GiB test through the
  installed service. See the unresolved rollback incident below before
  attempting any further live resize.
- **Unresolved live incident carried forward:** the last recorded live state
  has `win11_gpu` at `requested=0` and `current=18432 KiB` (18 MiB), not
  converged, after a rollback that did not complete within the 300-second
  window. Do not issue another resize (via script or the host service) until
  an operator confirms current live state and the rollback non-convergence is
  understood.

## Completed

| ID | Title | Owner | Completed | Notes |
| --- | --- | --- | --- | --- |
| TASK-003 | Bash validation helpers | Copilot | 2026-08-17 | Added prerequisite, QGA probe, and Rust validation scripts. |
| TASK-006 | Rust Copilot prompt set | Copilot | 2026-08-17 | Added repository-aware Rust project, API, test, refactor, security, docs, CI, and performance prompts; updated existing prompts and always-on instructions. |
| TASK-007 | Documentation review of libvirt/QEMU virtio-mem constraints | Copilot | 2026-08-18 | Added host-side virtio-mem semantics, compatibility limits, and live validation guidance based on official libvirt and QEMU documentation. |

## Blocked

| ID | Title | Blocker | Owner | Workaround |
| --- | --- | --- | --- | --- |
| TASK-002 | QEMU Guest Agent validation | No live RHEL/libvirt host or Windows guest is attached | Copilot | Run `scripts/validate-guest-agent.sh` on the KVM host. |

## Architecture Decisions

### 2026-08-18 demand-agent and global-controller design review

The architecture review of the Windows driver findings is now incorporated as
the implementation direction for the next phases:

- Phase 2 builds a Windows demand-agent foundation using native
  `GlobalMemoryStatusEx` and `GetPerformanceInfo` telemetry.
- The Windows service reports measurements and recommendations; it does not
  directly allocate host memory or issue Linux/libvirt commands.
- The existing one-VM host controller remains the Phase 2 actuation authority
  and retains host-headroom, alignment, minimum-headroom, and convergence gates.
- Phase 3 adds one Linux global pool authority with host reserve accounting,
  multi-VM growth/reclaim priorities, trend-aware reclaim, and explicit pressure
  states.
- Upstream `viomem.sys` confirms block bitmaps, `requested_size` versus
  `plugged_size`, NUMA feature negotiation, inaccessible unplugged memory, and
  Windows memory-manager hot-add/hot-remove. These are integration boundaries,
  not a reason to duplicate driver mechanics in the Rust service.
- The upstream device interface does not establish a supported user-mode
  IOCTL/status contract for this repository; direct driver access is deferred.
- The upstream `viomem` project is a KMDF/Visual Studio solution with
  VirtIO/WDF library dependencies and Win10/Win11 x86/x64/ARM64 configurations.
  This repository should consume the driver as an external dependency, not
  absorb its kernel build, signing, or installation lifecycle.
- Upstream issue `#1574` is not cited as a viomem unplug defect; it is a
  `vioscsi` TRIM issue. Any viomem wait/non-convergence risk requires separate
  evidence.

New tracked work is documented in `docs/roadmap.md`,
`docs/future-architecture.md`, `docs/api-contract.md`,
`docs/data-model.md`, and `docs/testing.md`. Existing live rollback issue
ISSUE-005 and the QGA capability blocker remain open and unchanged.

### Phase 2 handoff

- Implement native telemetry behind deterministic fakes first.
- Keep the demand report versioned and canonical-byte based.
- Do not replace QGA/dommemstat or enable new live resize behavior until live
  compatibility evidence exists.

### Phase 3 handoff

- First prove the mapping between driver `plugged_size` and libvirt `current`.
- Build and test global pool arbitration in simulation before live multi-VM
  actuation.
- Require bounded, aligned, convergent reclaim with measured workload history.
- If a driver status interface is needed, create a separate signed-driver
  work item with access-control and rollback evidence before adding Rust calls.

### Runtime language policy

- Rust is the default choice for any service or program logic.
- Bash is used for automation and validation scripts.
- Go is explicitly not used in this repository.

### Communication Protocol

Use QEMU Guest Agent over the validated guest-host interface. Alternatives rejected:

Use QEMU Guest Agent over the validated guest-host interface. Alternatives rejected:

- Direct registry access: violates service boundaries
- Unvalidated custom protocols: adds unnecessary complexity
- Go-based implementation: intentionally excluded

### RHEL host controller

- A Rust controller supervised by a templated systemd unit manages exactly one
  explicitly configured VM and virtio-mem alias per service instance.
- It queries QEMU Guest Agent memory statistics and live libvirt XML, then may
  issue one validated `virsh update-memory-device --live` request.
- It must not discover VMs broadly, invoke a shell, administer Windows
  processes, or issue another request until `requested` equals `current`.
- Failed QGA, XML, or resize operations remain explicit service failures;
  systemd restart behavior must be bounded and must never blindly replay a
  resize request.
