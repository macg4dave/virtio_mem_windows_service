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

Validate from `windows/` with `cargo fmt --all -- --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build --release` when available.

Shell safety:

- Limit edits to the repository files required by the task; never alter or delete server-side files or VM/service state without explicit current-turn approval.
- Keep validation unprivileged. If privilege is required, ask first with the complete command, target, mutation, and rollback, then run the whole script once under `sudo`; never automate password entry.
