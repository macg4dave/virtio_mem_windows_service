---
agent: agent
description: Start one virtio-mem Windows service backlog task safely
---

Read `.github/copilot-instructions.md`, `BACKLOG.md`,
`docs/build-test-tooling-roadmap.md`, `docs/architecture.md`,
`docs/engineering-standards.md`, `docs/testing.md`, and
`.github/prompts/README.md`.

Then:

1. Identify the selected Ready product or BT tooling task on its owning board,
   or ask which task to claim if none was named.
2. Summarize the owning service, files to touch, docs to read, primary task
   prompt, and required code-test/repository-gate/workflow validation layers.
3. Check `git status --short`.
4. Claim exactly one task on its owning board by changing its status to
   `In Progress` before substantial edits.
5. Keep changes inside the task card scope.
6. Update contracts/docs/tests together.
7. Run the task's validation and report blockers exactly.

For BT-M/BT-T build/test-tooling work, use
`docs/build-test-tooling-roadmap.md` as the task board instead of claiming a
product `TASK-*` code.

Rust-specific requirements:

- Keep service logic and maintained automation in Rust; use Bash only for a
  generated or task-specific privileged boundary.
- Add deterministic regression tests for changed behavior.
- Prefer safe Rust with explicit `Result`/`Option` handling; avoid unjustified `unwrap()`, `expect()`, panics, and `unsafe`.
- Preserve QEMU Guest Agent contracts and the Windows-service/host-automation boundary.
- Run focused direct Cargo tests and then `cargo xtask gate local`; report
  native Windows and higher-level workflows separately when applicable.
- Update affected contracts/docs and `BACKLOG.md` in the same task.

Do not add speculative features. Do not use AI agreement as validation.

Shell safety:

- Check and report the working tree before edits.
- Build, test, install the task candidate, exercise the relevant service lifecycle, inspect the live test system, and run a bounded reversible resize by default when required by the task's beta acceptance criteria.
- Give the live execution notice and follow `.github/copilot-instructions.md`, including safety gates, bounded convergence, rollback, one-script privilege batching, and direct operator password entry. Reboots, deletions, persistent configuration changes, disabled safety controls, and unrelated mutations still require explicit approval.
