---
agent: agent
description: Make an API change without contract drift
---

Before editing, read:

- `BACKLOG.md`
- `docs/api-contract.md`
- `docs/architecture.md`
- `docs/feature-matrix.md`
- `docs/engineering-standards.md`

For the change:

1. Update `docs/api-contract.md` first or alongside behavior when the QEMU Guest Agent or Rust API contract changes.
2. Update `docs/data-model.md` when state or persistence structures change.
3. Keep public Rust APIs minimal and backward-compatible unless a breaking change is explicit.
4. Add or update deterministic Rust validation tests, including malformed and boundary cases.
5. Run focused direct Cargo tests for the changed contract, then
   `cargo xtask gate local`; run native Windows and any contract-level
   integration workflow separately when applicable.
6. Update `BACKLOG.md` handoff notes with exact validation.

Keep runtime and maintained tooling logic in Rust. Bash is limited to the
task-specific privileged process boundary. Do not invent OpenAPI or
generated-schema files that this repository does not use.

Shell safety:

- Run local checks first. Task-scoped beta installation, service lifecycle, live inspection, and bounded reversible resize validation are authorized by default.
- Give the execution notice and follow `.github/copilot-instructions.md` for safety gates, rollback, privilege batching, and password handling. Reboots, deletions, persistent configuration changes, disabled safety controls, and unrelated mutations still require explicit approval.
