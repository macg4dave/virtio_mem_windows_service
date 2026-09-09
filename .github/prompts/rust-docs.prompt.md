---
name: rust-docs
description: Update Rust API and repository documentation accurately
---

Read `readme.md`, `docs/architecture.md`, `docs/engineering-standards.md`,
`docs/testing.md`, and `BACKLOG.md`. Read
`docs/build-test-tooling-roadmap.md` when commands or tooling change.

Task:
"""
<documentation goal>
"""

Rules:

- Document actual behavior only; do not invent runtime capabilities or unsupported commands.
- Add doc comments for new public Rust items and explain units, invariants, errors, and safety assumptions.
- Keep QEMU Guest Agent and memory-controller terminology consistent with existing contracts.
- Update `docs/api-contract.md`, `docs/data-model.md`, `docs/feature-matrix.md`, `docs/roadmap.md`, `docs/issues.md`, or `BACKLOG.md` when their triggers apply.
- Keep examples safe, deterministic, and compatible with Rust/Bash-only repository policy.
- If a code example changes, add or update a test or doctest when practical.
- Avoid unrelated prose or formatting churn.

Validate Rust examples with focused tests and `cargo xtask gate local`, then
report native Windows and live-VM results or blockers separately.

Shell safety:

- Prefer read-only documentation validation, but task-scoped beta installation, service lifecycle, live inspection, and bounded reversible resize validation are authorized by default when needed to verify an example or procedure.
- Give the execution notice and follow `.github/copilot-instructions.md` for safety gates, rollback, privilege batching, and password handling. Reboots, deletions, persistent configuration changes, disabled safety controls, and unrelated mutations still require explicit approval.
