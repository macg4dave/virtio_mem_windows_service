---
name: rust-project
description: Make a focused tested Rust product change
---

Read the owning module, tests, architecture, contract, and task card.

Task: `<behavior and acceptance criterion>`

- Make the smallest cohesive change and preserve service boundaries.
- Use explicit errors and validated inputs; keep side effects injectable.
- Add focused regression and boundary tests in the owning crate.
- Update affected contracts, status, and task documentation.

Run the focused Cargo tests, then `cargo xtask gate local`, then any separately
applicable native or higher-level workflow.
