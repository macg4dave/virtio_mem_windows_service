---
name: rust-docs
description: Update Rust API and repository documentation accurately
---

Read `readme.md`, `docs/architecture.md`, `docs/engineering-standards.md`, `docs/testing.md`, and `BACKLOG.md`.

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

Validate Rust examples with the appropriate `cargo test` command from `windows/`, then report exact results and any live-VM blocker.

Shell safety:

- Documentation validation is read-only by default and must not edit/delete server files or change VM, libvirt, or systemd state without explicit approval.
- Never use `sudo`, `su`, or `doas` without current-turn approval naming the complete command, target, mutation, and rollback. After approval run the whole script once under `sudo`; never automate or collect the password.
