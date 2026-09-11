---
name: build-test-tooling
description: Change the Rust build and higher-level validation control plane
---

Read `docs/testing.md`, `docs/build-test-tooling-roadmap.md`, `tools/README.md`,
and the affected `tools/xtask` modules.

Task: `<tooling behavior to add, change, consolidate, or remove>`

- Keep CLI routing thin and reusable behavior in capability-named Rust modules.
- Preserve Cargo tests as the code-level test system; `xtask` owns only
  aggregate gates and cross-boundary workflows.
- Model explicit targets, configuration, modes, prerequisites, time bounds,
  evidence, cleanup, rollback, and failure stages.
- Remove superseded scripts and entrypoints once their current invariants are
  represented in Rust or deliberately retired.
- Implement privileged boundaries as explicit typed `xtask` re-execution of
  the current prebuilt executable through one outer `sudo`; never generate a
  shell script or run Cargo as root.
- Add deterministic tests for parsing, command construction, failure,
  cancellation, cleanup, rollback, and result classification.
- Keep editor and Make entrypoints as policy-free delegates.

Run focused tooling tests, `cargo xtask gate local`, and the changed workflow
in its safest useful mode. Update the tooling roadmap and command documentation.
