---
name: rust-api
description: Change a Rust public API without contract or documentation drift
---

Read `docs/api-contract.md`, `docs/data-model.md` when relevant, `docs/architecture.md`, `docs/feature-matrix.md`, and `BACKLOG.md`.

Task:
"""
<public API or contract change>
"""

Rules:

1. Prefer additive, backward-compatible APIs and keep visibility minimal.
2. Preserve QEMU Guest Agent request/response behavior unless the task explicitly changes it.
3. Update Rust doc comments, `docs/api-contract.md`, `docs/data-model.md`, or `docs/feature-matrix.md` when applicable.
4. For breaking changes, include migration notes, compatibility rationale, and tests for old and new behavior where possible.
5. Use structured error types and avoid exposing implementation details unnecessarily.
6. Add tests that demonstrate the public contract, including invalid input and boundary behavior.
7. Do not invent OpenAPI or other schema artifacts; use the repository's documented contracts.

Run focused tests and `cargo xtask gate local`; when Windows code changed, run
and report `cargo xtask windows all` separately. Report exact results.

Shell safety:

- Run local contract validation first. Task-scoped beta installation, service lifecycle, live integration, and bounded reversible resize validation are authorized by default for an unambiguous target.
- Give the execution notice and follow `.github/copilot-instructions.md` for safety gates, rollback, privilege batching, and password handling. Reboots, deletions, persistent configuration changes, disabled safety controls, and unrelated mutations still require explicit approval.
