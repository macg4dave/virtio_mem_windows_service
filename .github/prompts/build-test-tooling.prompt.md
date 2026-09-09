---
name: build-test-tooling
description: Add or change the Rust repository gate and higher-level workflow tooling
---

Read `.github/copilot-instructions.md`, `.github/prompts/README.md`,
`docs/testing.md`, `docs/engineering-standards.md`,
`docs/build-test-tooling-roadmap.md`, `tools/README.md`, and the affected
`tools/xtask` modules. Read the product acceptance contract when the workflow
validates a product feature.

Task:
"""
<tooling command, workflow, fixture, result, or entrypoint change>
"""

Before editing, classify the work:

- extend an existing capability when its prerequisites, target, safety model,
  lifecycle, and evidence contract are unchanged;
- add a stable capability-named command/module when those differ materially;
- update editor or Make entrypoints only as thin delegates with no independent
  policy; or
- remove obsolete scripts/manual steps after their useful invariants and
  acceptance evidence are represented in Rust or deliberately retired.

Rules:

1. Keep CLI routing thin and put reusable behavior in capability-named Rust
   modules. Do not name stable commands after temporary milestone codes.
   Reserve `gate` for non-mutating quality aggregation; use an explicit
   capability name for deployment or live mutation.
2. Preserve normal Cargo tests as the code-level test system. Do not turn
   xtask into a bespoke unit-test runner or add a command that merely wraps one
   focused `cargo test`.
3. Model targets, prerequisites, modes, timeouts, evidence, cleanup, rollback,
   and failure stages explicitly. Never infer an endpoint or mutation target.
4. Default live or deployment mutations to read-only/dry-run where practical.
   Preserve fail-closed safety gates and actionable non-zero failures.
5. Add deterministic Rust tests for parsers, command construction, malformed
   output, scope rejection, timeout/cancellation, result classification,
   cleanup, and rollback decisions. Inject external process, filesystem, SSH,
   time, and platform effects.
6. Keep native Windows, RHEL, deployment, and live results distinct. Do not
   represent an unavailable platform as a pass.
7. Use Bash only for a generated or task-specific privileged process boundary;
   it must invoke prebuilt Rust logic and must not duplicate policy.
8. Update `docs/testing.md`, `tools/README.md`, editor tasks, and the BT roadmap
   when their commands or contracts change. Update the product task separately
   when the workflow supplies feature acceptance evidence.

Validation and reporting:

- Run the focused direct Cargo tests for the changed xtask module.
- Run `cargo xtask gate local` after the focused tests.
- Exercise the changed higher-level command in its safest useful mode; run
  native Windows or live/apply modes only when applicable prerequisites and
  safety requirements are satisfied.
- Report exact commands by layer, target and mode for external workflows,
  pass/fail/blocked/not-run status, evidence paths, final state, and whether
  cleanup or rollback ran.
