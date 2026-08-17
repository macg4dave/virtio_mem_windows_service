---
agent: agent
description: Make an API change without contract drift
---

Before editing, read:

- `BACKLOG.md`
- `docs/api-contract.md`
- `api/openapi.yaml`
- `docs/architecture.md`
- `docs/feature-matrix.md`

For the change:

1. Update `api/openapi.yaml` first or alongside handler behavior.
2. Update `docs/api-contract.md`.
3. Regenerate types only if the project includes generated schema tooling.
4. Avoid hand-written DTO drift; alias generated schemas where practical.
5. Add or update Rust validation tests and any relevant automation checks.
6. Run focused tests and the local validation flow for the affected component.
7. Update `BACKLOG.md` handoff notes with exact validation.

Keep the runtime logic Rust-based and keep scripting in Bash where feasible.
