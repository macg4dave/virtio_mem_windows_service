---
name: rust-refactor
description: Refactor Rust code while preserving behavior and service boundaries
---

Read the relevant module, tests, `docs/architecture.md`, and `BACKLOG.md` before editing.

Task:
"""
<refactor goal>
"""

Rules:

- Preserve observable behavior, public APIs, error semantics, and QEMU Guest Agent contracts.
- Keep the patch minimal; do not mix formatting-only churn with the refactor.
- Prefer small cohesive functions, explicit ownership, borrowing, and dependency injection for side effects.
- Keep pure policy/parsing code separate from Windows service and transport code.
- Do not introduce `unsafe`, global mutable state, unnecessary cloning, or new dependencies without a clear reason.
- Keep all existing tests and add tests for any behavior exposed by the refactor.
- Update docs and `BACKLOG.md` if structure, ownership, or behavior changes.

Run focused tests and `cargo xtask gate local`; run the native Windows gate
separately when Windows code changed.

Shell safety:

- Limit edits to the repository files required by the task. Task-scoped beta installation, service lifecycle, live inspection, and bounded reversible resize validation are authorized by default.
- Give the execution notice and follow `.github/copilot-instructions.md` for safety gates, rollback, privilege batching, and password handling. Reboots, deletions, persistent configuration changes, disabled safety controls, and unrelated mutations still require explicit approval.
