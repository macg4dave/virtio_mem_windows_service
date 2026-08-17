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

Tasks ready to start (Phase 1 - Foundation):

| ID | Title | Owner | Status | Effort | Dependencies |
|----|-------|-------|--------|--------|--------------|
| TASK-001 | Rust service scaffolding | Copilot | Planned | 4-6 hours | QEMU Guest Agent validation |
| TASK-002 | QEMU Guest Agent validation | Unassigned | Planned | 2-3 hours | None |
| TASK-003 | Bash validation helpers | Unassigned | Planned | 1-2 hours | TASK-001 |

## In Progress

| ID | Title | Owner | Status | Handoff Notes |
|----|----|----|--------|---------------|
| | | | | |

## Completed

| ID | Title | Owner | Completed | Notes |
|----|----|-------|-----------|-------|
| | | | | |

## Blocked

| ID | Title | Blocker | Owner | Workaround |
|----|----|----|-------|-----------|
| | | | | |

## Architecture Decisions

### Runtime language policy

- Rust is the default choice for any service or program logic.
- Bash is used for automation and validation scripts.
- Go is explicitly not used in this repository.

### Communication Protocol

Use QEMU Guest Agent over the validated guest-host interface. Alternatives rejected:
- Direct registry access: violates service boundaries
- Unvalidated custom protocols: adds unnecessary complexity
- Go-based implementation: intentionally excluded
