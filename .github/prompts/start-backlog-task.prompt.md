---
agent: agent
description: Start one virtio-mem Windows service backlog task safely
---

Read `.github/copilot-instructions.md`, `BACKLOG.md`, `docs/architecture.md`, `docs/engineering-standards.md`, and `docs/testing.md`.

Then:

1. Identify the selected `Ready` backlog task or ask which task to claim if none was named.
2. Summarize the owning service, files to touch, docs to read, and validation commands.
3. Check `git status --short`.
4. Claim exactly one task by changing its status to `In Progress` before substantial edits.
5. Keep changes inside the task card scope.
6. Update contracts/docs/tests together.
7. Run the task's validation and report blockers exactly.

Rust-specific requirements:

- Keep service logic in Rust and automation in Bash only.
- Add deterministic regression tests for changed behavior.
- Prefer safe Rust with explicit `Result`/`Option` handling; avoid unjustified `unwrap()`, `expect()`, panics, and `unsafe`.
- Preserve QEMU Guest Agent contracts and the Windows-service/host-automation boundary.
- Run from `windows/`: `cargo fmt --all -- --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build --release` when available.
- Update affected contracts/docs and `BACKLOG.md` in the same task.

Do not add speculative features. Do not use AI agreement as validation.
