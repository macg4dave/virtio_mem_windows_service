---
name: rust-refactor
description: Refactor Rust while preserving required behavior
---

Read the owning module, tests, architecture, and current contract.

Task: `<refactor goal>`

- Preserve required observable behavior and error semantics.
- Remove dead compatibility paths when no current consumer depends on them.
- Prefer cohesive functions, explicit ownership, and injected side effects.
- Keep policy and parsing separate from platform and transport code.
- Keep existing relevant tests and add coverage for newly exposed boundaries.
