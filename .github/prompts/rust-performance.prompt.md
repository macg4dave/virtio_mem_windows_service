---
name: rust-performance
description: Measure and improve Rust performance safely
---

Read the hot path, correctness contract, and applicable validation procedure.

Task: `<measurable performance goal>`

- Establish a repeatable baseline and success criterion before changing code.
- Preserve correctness, bounds, cancellation, and error behavior.
- Prefer simple measured improvements; add deterministic benchmarks or
  regression tests and avoid live effects in microbenchmarks.

Report measurements and run focused tests, `cargo xtask gate local`, and any
separate platform measurement workflow.
