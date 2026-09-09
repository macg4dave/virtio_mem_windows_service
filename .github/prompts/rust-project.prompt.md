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

1. Make the smallest focused Rust change; put maintained automation in
   `tools/xtask` and use Bash only for a task-specific privileged boundary.
2. Preserve existing contracts and architecture. Do not add Go, Python, Node.js, PowerShell, OpenAPI, or unrelated framework artifacts.
3. Prefer safe idiomatic Rust, explicit `Result`/`Option` handling, structured errors, and dependency-free solutions when practical.
4. Avoid `unwrap()`, `expect()`, panics, global mutable state, and `unsafe` unless justified and covered by tests.
5. Add regression tests for changed behavior, malformed input, error paths, and boundary conditions.
6. Keep pure parsing and resize-policy logic independent from live Windows/QGA effects.
7. Update affected docs and `BACKLOG.md` in the same change.
8. Run focused direct Cargo tests for changed code, then
   `cargo xtask gate local`. Use `workflow-validation.prompt.md` when feature
   acceptance crosses a component, process, platform, deployment, or live
   boundary. Report each validation layer separately.

Shell safety:

- Repository builds, tests, candidate installation, relevant service lifecycle checks, live inspection, and bounded reversible resize validation are authorized by default for beta work.
- Run hermetic checks first. Before a live mutation, give the execution notice and follow the target, safety-gate, timeout, convergence, rollback, privilege-batching, and password rules in `.github/copilot-instructions.md`.
- Reboots, deletions, persistent configuration changes, disabled safety gates, non-reversible resize, and unrelated mutations still require explicit approval.
