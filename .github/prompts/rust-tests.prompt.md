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
- Prefer unit tests for parsing, validation, policy, and error paths. Add or run live VM/QEMU Guest Agent coverage when it materially validates the beta acceptance criteria.
- Cover empty, malformed, missing, inconsistent, minimum, maximum, threshold, alignment, and convergence cases where applicable.
- Do not weaken assertions or hide failures with `unwrap()` in production code.
- Avoid network, filesystem, timing, and platform-global dependencies unless the test explicitly isolates them.
- Preserve public contracts and service boundaries.

Validate from `windows/` with `cargo test`, `cargo fmt --all -- --check`, and Clippy when available. Report exact results.

Shell safety:

- Run hermetic tests first, then applicable live beta tests. A task-scoped service lifecycle or bounded reversible resize is authorized by default when the target is unambiguous.
- Give the execution notice and follow `.github/copilot-instructions.md`; use captured initial state, safety gates, timeouts, convergence checks, and rollback.
- Batch privileged steps into one task script and one outer `sudo`; never automate or collect the password. Reboots, deletions, persistent configuration changes, disabled safety gates, and unrelated mutations still require explicit approval.
