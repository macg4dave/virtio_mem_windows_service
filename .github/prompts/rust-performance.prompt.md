---
name: rust-performance
description: Measure and optimize Rust service performance without sacrificing safety
---

Read `docs/architecture.md`, `docs/testing.md`, `docs/engineering-standards.md`, and `BACKLOG.md` first.

Task:
"""
<performance goal>
"""

Rules:

- Measure before changing code; include a baseline and an observable success criterion.
- Preserve correctness, thresholds, memory units, request convergence, error behavior, and public APIs.
- Prefer simple allocation/ownership improvements before concurrency or new dependencies.
- Do not optimize speculative hot paths or add unsafe code without evidence and a documented safety case.
- Add or update benchmarks only if the repository can run them deterministically; otherwise add regression tests.
- Keep live QEMU Guest Agent and Windows service effects out of microbenchmarks.
- Document the measured result and update `BACKLOG.md` if the task is completed.

Validate from `windows/` with tests, format check, Clippy, and release build. Report measurements, exact commands, and blockers.

Shell safety:

- Performance measurements must not mutate the server, VM, service manager, or files outside the repository without explicit approval naming the exact action.
- Prefer normal-user benchmarks and tests. If a privileged run is required, ask first with the complete command, target, mutation, and rollback, then run the whole script once under `sudo`; never automate or collect the password.
