---
name: rust-api
description: Change a public Rust or wire contract intentionally
---

Read `docs/api-contract.md`, `docs/data-model.md`, the owning module, and its
consumers.

Task: `<contract change and migration expectation>`

- Keep public surface minimal and update the normative contract with behavior.
- Treat compatibility as a current requirement only when a real consumer needs
  it; do not preserve obsolete behavior speculatively.
- Test valid, malformed, boundary, and migration cases.
- Update feature/status/task documentation.
