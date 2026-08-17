# BACKLOG

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
| TASK-004 | Windows memory polling policy | Copilot | In Progress | 2-3 hours | Parser and pure resize decision policy implemented; timer/service loop remains |
| TASK-005 | Safe QEMU Guest Agent response handling | Copilot | In Progress | 2-3 hours | Parser and error validation implemented; transport adapter remains |

## In Progress

| ID | Title | Owner | Status | Handoff Notes |
| --- | --- | --- | --- | --- |
| TASK-001 | Rust service scaffolding | Copilot | In Progress | Parser foundation is complete; runtime wiring and a complete Windows Rust build remain. |

## Completed

| ID | Title | Owner | Completed | Notes |
| --- | --- | --- | --- | --- |
| TASK-003 | Bash validation helpers | Copilot | 2026-08-17 | Added prerequisite, QGA probe, and Rust validation scripts. |
| TASK-006 | Rust Copilot prompt set | Copilot | 2026-08-17 | Added repository-aware Rust project, API, test, refactor, security, docs, CI, and performance prompts; updated existing prompts and always-on instructions. |

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
