---
name: workflow-validation
description: Validate behavior across a process, platform, deployment, or live boundary
---

Read `docs/testing.md`, `BACKLOG.md`, and the feature acceptance contract.

Task: `<behavior and acceptance criterion to validate>`

- Select the existing capability-named `cargo xtask` workflow.
- If none exists, add focused Cargo tests for code-level behavior or extend
  `tools/xtask` for repeatable cross-boundary behavior; do not create a
  maintained manual script.
- Require explicit target identity, inputs, time bounds, expected effects,
  forbidden effects, success criteria, evidence, cleanup, rollback, and final
  state.
- Diagnose recorded failure stages without bypassing checks through ad hoc
  remote commands.

Report focused tests, local gate, native platform result, workflow mode and
target, configured timing, evidence location, cleanup/rollback, and blockers as
separate results.
