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
| TASK-001 | Linux controller scaffolding | Copilot | In Progress | 4-6 hours | libvirt-devel |
| TASK-002 | Windows service scaffolding (Rust) | Unassigned | Deferred | Re-estimate after native QGA validation | Native QGA gap |
| TASK-003 | QEMU Guest Agent validation | Unassigned | Not Started | 2-3 hours | TASK-001 |

See [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) for detailed deliverables and acceptance criteria.

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

### Memory Allocation Strategy

- **Lower threshold**: 2 GB free → add 2 GB
- **Upper threshold**: 6 GB free → remove 2 GB
- **Min allocation**: 8 GB
- **Max allocation**: 28 GB
- **Poll interval**: 10 seconds (conservative to avoid oscillation)

Rationale: Conservative thresholds prevent thrashing. Asymmetric add/remove prevents rapid cycling.

### Communication Protocol

Use QEMU Guest Agent over Unix socket for reliability. Alternatives rejected:
- `dommemstat`: Limited accuracy (balloon-based)
- Direct registry access: Violates service boundaries
- Custom protocols: Unnecessary complexity
