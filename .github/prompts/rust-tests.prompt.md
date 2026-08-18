---
name: rust-tests
description: Add deterministic regression and boundary tests for Rust service logic
---

Read `docs/testing.md`, `docs/architecture.md`, and the relevant Rust module before editing.

Task:
"""
<behavior to test>
"""

Details:

- Code under test: <module/function>
- Files: <test and implementation paths>
- Expected behavior: <acceptance criteria>

Rules:

- Add the smallest deterministic tests that reproduce the behavior.
- Prefer unit tests for parsing, validation, policy, and error paths; do not require a live VM or QEMU Guest Agent.
- Cover empty, malformed, missing, inconsistent, minimum, maximum, threshold, alignment, and convergence cases where applicable.
- Do not weaken assertions or hide failures with `unwrap()` in production code.
- Avoid network, filesystem, timing, and platform-global dependencies unless the test explicitly isolates them.
- Preserve public contracts and service boundaries.

Validate from `windows/` with `cargo test`, `cargo fmt --all -- --check`, and Clippy when available. Report exact results.

Shell safety:

- Keep tests unprivileged and hermetic unless live integration is explicitly requested.
- Do not edit or delete server files, alter VM/libvirt/systemd state, or run mutating commands without current-turn approval naming the exact target and action.
- Never use `sudo`, `su`, or `doas` without current-turn approval naming the complete command, target, mutation, and rollback. After approval, run the complete test script once under `sudo`; never automate or collect the password, and let the user type it directly into the terminal.
