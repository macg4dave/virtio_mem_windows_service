---
agent: agent
description: Review a virtio-mem Windows service diff for contract and documentation drift
---

Review the current diff against:

- `.github/copilot-instructions.md`
- `BACKLOG.md`
- `docs/engineering-standards.md`
- `docs/architecture.md`
- `docs/feature-matrix.md`
- `docs/api-contract.md` if QEMU Guest Agent or public Rust API behavior changed
- `docs/data-model.md` if persistence changed
- `docs/testing.md`
- `docs/build-test-tooling-roadmap.md` when build/test tooling changed

Lead with concrete findings only. For each finding, name:

- the file and line or section
- the contract that drifted
- the likely runtime or maintenance impact
- the smallest correction

Do not approve the change just because tests pass. Tests are evidence, not a substitute for contract review.

Also check that:

- only Rust and task-boundary Bash were added or changed;
- Windows service code does not invoke Linux commands or access host devices;
- parsing validates malformed, missing, inconsistent, and overflowing values;
- new public Rust items have appropriate documentation and tests;
- `BACKLOG.md` and affected docs reflect the change;
- no secrets, credentials, tokens, private keys, or production data were introduced.
- maintained build/test behavior lives in `tools/xtask`, with no duplicate
  tracked Bash implementation.
- code-level behavior has focused Rust tests in its owning crate rather than
  being tested only through xtask or a live workflow;
- higher-level feature/deployment acceptance uses a repeatable xtask workflow
  rather than a maintained manual script; and
- focused Cargo, local aggregate, native Windows, and live/deployment evidence
  are reported as separate layers without turning an unrun layer into a pass.

Shell safety:

- Use read-only review commands unless reproducing a finding requires task-scoped beta installation, service lifecycle, or bounded reversible resize validation; those operations are authorized by default.
- Give the execution notice and follow `.github/copilot-instructions.md` for safety gates, rollback, privilege batching, and password handling. Reboots, deletions, persistent configuration changes, disabled safety controls, and unrelated mutations still require explicit approval.
