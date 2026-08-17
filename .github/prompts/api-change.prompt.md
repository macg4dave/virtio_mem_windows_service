---
agent: agent
description: Make a Roller_hoops API change without contract drift
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
3. Regenerate UI types with `cd ui-node && npm run gen:openapi`.
4. Avoid hand-written DTO drift; alias generated schemas where practical.
5. Add or update Go handler/contract tests and UI route tests as appropriate.
6. Run focused tests, then `npm test`, `npm run build`, and Go tests when available.
7. Update `BACKLOG.md` handoff notes with exact validation.

Keep `core-go` headless and keep `ui-node` out of PostgreSQL.
