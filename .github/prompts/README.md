# Prompt selection

Project-wide rules live in `../copilot-instructions.md`; task prompts do not
repeat or override them. Choose one primary prompt by the work being performed.
Add a domain prompt only when the task genuinely has both scopes.

| Task | Primary prompt | Boundary |
| --- | --- | --- |
| Add/fix Rust unit, crate integration, regression, or doctests | `rust-tests.prompt.md` | Code-level behavior in one owning crate; not live or deployment acceptance |
| Run or add feature, deployment, service/process, component-integration, end-to-end, environment, or live validation | `workflow-validation.prompt.md` | Higher-level behavior through `cargo xtask`; code-level tests still apply |
| Add or change `tools/xtask`, its commands, fixtures, result model, or editor/Make routing | `build-test-tooling.prompt.md` | Tool implementation work tracked with BT codes |
| Make ordinary product Rust changes | `rust-project.prompt.md` | Product implementation plus focused tests and applicable validation layers |
| Change a public contract, refactor, optimize, harden, or document | Matching `rust-*` or `api-change` prompt | Domain-specific work; use the canonical test-selection rules |
| Prepare one exact privileged RHEL process boundary | `rhel-privileged-batch.prompt.md` | Task-specific Bash only; reusable behavior stays in Rust |
| Review a diff | `review-drift.prompt.md` | Findings include test-layer and tooling duplication drift |
| Claim work from a board | `start-backlog-task.prompt.md` | Product tasks use `TASK-*`; tooling work uses `BT-T*` |

Examples:

- A parser bug uses `rust-project` or `rust-tests`: add a focused Rust
  regression test, then run the local repository gate. Do not add an xtask
  command just for the parser test.
- A Windows service lifecycle feature uses `rust-project` for implementation
  and `workflow-validation` for installed-service acceptance. It needs both
  code-level tests and higher-level evidence.
- Repeated manual deployment steps use `build-test-tooling` to add or extend a
  safe xtask workflow, then `workflow-validation` to exercise that workflow.
