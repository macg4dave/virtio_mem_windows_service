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

Validate from `windows/` with `cargo test`, format check, Clippy, and release build when practical. Report exact results.
