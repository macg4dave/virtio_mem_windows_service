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

Run focused direct Cargo benchmarks/tests, then `cargo xtask gate local`. Run
the native Windows gate and any higher-level measurement workflow separately
when applicable. Report measurements, exact commands, targets/modes, and
blockers.

Shell safety:

- Prefer normal-user benchmarks and tests. Task-scoped beta installation, service lifecycle, live measurement, and bounded reversible resize validation are authorized by default when required by the benchmark.
- Give the execution notice and follow `.github/copilot-instructions.md` for safety gates, rollback, privilege batching, and password handling. Reboots, deletions, persistent configuration changes, disabled safety controls, and unrelated mutations still require explicit approval.
