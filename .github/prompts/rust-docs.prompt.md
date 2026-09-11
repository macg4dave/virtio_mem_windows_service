---
name: rust-docs
description: Keep Rust and repository documentation current
---

Read actual code, the normative contract, and current command help.

Task: `<documentation outcome>`

- Document supported behavior, units, invariants, configuration, errors, and
  ownership; remove superseded procedures and transient test values.
- Keep one authoritative procedure and link to it instead of duplicating it.
- Update contracts, boards, matrices, and status only when their stated scope
  changes.
- Validate examples with the owning Rust tests or safest applicable `xtask`
  mode.
