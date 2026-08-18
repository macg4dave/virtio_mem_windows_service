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
