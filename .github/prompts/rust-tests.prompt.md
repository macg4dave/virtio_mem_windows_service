---
name: rust-tests
description: Add code-level Rust unit, integration, regression, or doctests
---

Read `.github/copilot-instructions.md`, `.github/prompts/README.md`,
`docs/testing.md`, `docs/architecture.md`, and the relevant Rust module and
existing tests before editing.

Task:
"""
<behavior to test>
"""

Details:

- Code under test: <module/function>
- Files: <test and implementation paths>
- Expected behavior: <acceptance criteria>

Rules:

- Add the smallest deterministic code-level tests that reproduce the behavior.
- Prefer unit tests beside private parsing, validation, policy, state-machine,
  and error logic. Use crate integration tests for public multi-module behavior
  and doctests for public examples.
- Cover empty, malformed, missing, inconsistent, minimum, maximum, threshold, alignment, and convergence cases where applicable.
- Do not weaken assertions or hide failures with `unwrap()` in production code.
- Avoid network, live VM, service manager, remote host, wall-clock, mutable
  filesystem, and platform-global dependencies. Inject or fake those boundaries.
- Preserve public contracts and service boundaries.
- Do not add an xtask command merely to run this test. If acceptance also
  crosses a process, component, machine, deployment, or live-system boundary,
  use `workflow-validation.prompt.md` in addition to this prompt.

Run the narrowest direct `cargo test -p PACKAGE --all-features --locked
TEST_FILTER` command first, then `cargo xtask gate local`. Run native Windows
and higher-level workflows separately when applicable. Report each layer and
do not treat the aggregate gate as proof that the focused test was selected.

Shell safety:

- Run hermetic tests first, then applicable live beta tests. A task-scoped service lifecycle or bounded reversible resize is authorized by default when the target is unambiguous.
- Give the execution notice and follow `.github/copilot-instructions.md`; use captured initial state, safety gates, timeouts, convergence checks, and rollback.
- Batch privileged steps into one task script and one outer `sudo`; never automate or collect the password. Reboots, deletions, persistent configuration changes, disabled safety gates, and unrelated mutations still require explicit approval.
