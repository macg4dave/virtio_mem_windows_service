---
name: rust-project
description: Make a focused, tested Rust change in the virtio-mem Windows service
---

Read first:

- `readme.md`
- `docs/architecture.md`
- `BACKLOG.md`
- `docs/engineering-standards.md`
- `docs/testing.md`

Task:
"""
<one-sentence goal>
"""

Details:

- Files or module: <paths, or let the agent determine them>
- Behavior and acceptance criteria: <expected behavior>
- Tests: <tests to add or update>
- Constraints: <public APIs, service boundaries, or files not to change>

Rules:

1. Make the smallest focused Rust change; use Bash only for automation.
2. Preserve existing contracts and architecture. Do not add Go, Python, Node.js, PowerShell, OpenAPI, or unrelated framework artifacts.
3. Prefer safe idiomatic Rust, explicit `Result`/`Option` handling, structured errors, and dependency-free solutions when practical.
4. Avoid `unwrap()`, `expect()`, panics, global mutable state, and `unsafe` unless justified and covered by tests.
5. Add regression tests for changed behavior, malformed input, error paths, and boundary conditions.
6. Keep pure parsing and resize-policy logic independent from live Windows/QGA effects.
7. Update affected docs and `BACKLOG.md` in the same change.
8. Validate from `windows/` with format check, tests, Clippy, and release build when practical; report exact output and blockers.
