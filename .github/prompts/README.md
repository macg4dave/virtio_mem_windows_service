# Task prompt selection

Project-wide rules live in `../copilot-instructions.md`. Select one primary
prompt for the work being done; prompts contain only task-specific guidance.
The canonical validation, safety, documentation, and reporting rules apply
without being repeated in each prompt.
Add `workflow-validation` only when acceptance also crosses a process,
platform, deployment, or live-system boundary.

| Work | Primary prompt |
| --- | --- |
| Product Rust implementation or bug fix | `rust-project.prompt.md` |
| Rust unit, integration, regression, or doctest work | `rust-tests.prompt.md` |
| Public Rust or wire contract change | `rust-api.prompt.md` |
| Refactor | `rust-refactor.prompt.md` |
| Performance work | `rust-performance.prompt.md` |
| Security review or hardening | `rust-security.prompt.md` |
| Documentation | `rust-docs.prompt.md` |
| `tools/xtask`, editor, Make, or tooling migration | `build-test-tooling.prompt.md` |
| Higher-level validation execution or design | `workflow-validation.prompt.md` |
| Diff review | `review-drift.prompt.md` |
| Board task selection and claim | `start-backlog-task.prompt.md` |
| One exact privileged RHEL boundary | `rhel-privileged-batch.prompt.md` |
