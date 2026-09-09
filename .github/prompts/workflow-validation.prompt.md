---
name: workflow-validation
description: Run or add repeatable feature, deployment, service, integration, end-to-end, environment, or live validation
---

Read `.github/copilot-instructions.md`, `.github/prompts/README.md`,
`docs/testing.md`, `BACKLOG.md`, and the product acceptance contract. Read
`docs/build-test-tooling-roadmap.md` and `tools/README.md` when adding or
changing the workflow itself.

Task:
"""
<feature or development workflow to validate>
"""

First classify the acceptance boundary:

- environment/setup readiness;
- native platform build/runtime;
- component or process integration;
- local interactive service behavior;
- installed service/deployment lifecycle;
- end-to-end feature behavior; or
- bounded live-system behavior.

Then select the existing capability-named `cargo xtask` command documented in
`docs/testing.md`. Do not substitute a repository quality gate for a workflow
test, or a live workflow for focused code-level Rust tests.

If no suitable workflow exists:

1. Decide whether the acceptance can be proved by a normal unit, crate
   integration, or doctest. If so, add that test in the owning crate instead.
2. If it crosses components, processes, platforms, deployment, or live state,
   add or extend `tools/xtask` using
   `build-test-tooling.prompt.md`. Do not create a maintained manual script.
3. An exact privileged RHEL boundary may use
   `rhel-privileged-batch.prompt.md`; keep all reusable logic in Rust.

Every new or changed workflow must define:

- the feature/task and acceptance criterion it proves;
- explicit prerequisites and target identity;
- read-only, dry-run, and apply behavior as applicable;
- expected effects and forbidden side effects;
- bounded timeouts, success criteria, and failure stages;
- captured evidence and output location;
- cleanup and rollback, including the final state on rollback failure; and
- deterministic tooling tests using injected boundaries or fixtures.

Execution order:

1. Run the focused Rust tests for changed product and tooling behavior.
2. Run `cargo xtask gate local`.
3. Run environment checks and the selected higher-level workflow.
4. Give the required execution notice before a live mutation. Follow the
   canonical target, privilege, safety, convergence, and rollback rules.
5. Never expand a failed workflow into ad hoc remote commands that bypass its
   checks. Diagnose the recorded stage; update the workflow if its reusable
   behavior is deficient.

Report separately:

- focused Rust test command and result;
- local repository gate result;
- native Windows result when applicable;
- workflow command, resolved target, mode, duration/timeout, and result;
- evidence paths or artifact hashes;
- cleanup/rollback status and final observed state; and
- every blocked or not-run layer with its exact reason.
