# BACKLOG

## 2026-08-17 Documentation Handoff

Imported applicable Windows service guidance from Microsoft Learn into
`docs/architecture.md`, `docs/engineering-standards.md`, `docs/testing.md`,
and `windows/README.md`. The documented lifecycle contract is intended for
the remaining SCM adapter work; no unfinished SCM capability is marked as
complete by this documentation update.

Execution source of truth. Update after every session.

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
| TASK-005 | Safe QEMU Guest Agent response handling | Copilot | In Progress | 2-3 hours | Parser, typed poll errors, and configurable named-pipe client implemented; live transport validation remains |
| TASK-007 | Documentation review of libvirt/QEMU virtio-mem constraints | Copilot | Ready | 1-2 hours | No code changes; use official virtio-mem guidance to tighten service and validation docs |

## In Progress

| ID | Title | Owner | Status | Handoff Notes |
| --- | --- | --- | --- | --- |
| TASK-001 | Rust service scaffolding | Copilot | In Progress | Parser, named-pipe QGA client, wakeable scheduler, portable service host, validated service configuration, SCM dispatcher, install/start/stop/remove commands, canonical byte-based VirtioMemState validation, captured libvirt XML parsing, injectable XML state-provider boundary, and a deterministic local service runtime harness are locally covered; live VM evidence, service registration, and QGA validation remain. |
| TASK-008 | RHEL virtio-mem host controller | Copilot | In Progress | Added the workspace and shared Rust core; bounded argument-safe `virsh` QGA/XML/resize adapters; alias-selected live XML parsing; convergence suppression; signal-driven systemd runtime; unit/configuration artifacts; and regression tests. Workspace format, release build, 46 tests, and Clippy warnings-as-errors pass locally. Live RHEL/libvirt validation, service-account authorization, compatibility gate, and reversible resize evidence remain required before enablement. |

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
