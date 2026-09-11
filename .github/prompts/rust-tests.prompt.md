---
name: rust-tests
description: Add deterministic code-level Rust tests
---

Read the owning Rust module and existing tests.

Task: `<behavior to prove>`

- Prefer unit tests for private parsing, policy, and state logic; use crate
  integration tests for public multi-module behavior and doctests for examples.
- Cover relevant malformed, missing, inconsistent, overflow, alignment,
  threshold, cancellation, and convergence cases.
- Inject network, process, clock, filesystem, platform, and live-system effects.
- Do not add an `xtask` command merely to wrap a Cargo test.
