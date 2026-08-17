---
agent: agent
description: Review a Roller_hoops diff for contract and documentation drift
---

Review the current diff against:

- `AGENTS.md`
- `BACKLOG.md`
- `docs/engineering-standards.md`
- `docs/ai-coding-control.md`
- `docs/feature-matrix.md`
- `api/openapi.yaml` and `docs/api-contract.md` if API behavior changed
- `docs/data-model.md` if persistence changed
- `docs/ui-ux.md` if operator workflow changed

Lead with concrete findings only. For each finding, name:

- the file and line or section
- the contract that drifted
- the likely runtime or maintenance impact
- the smallest correction

Do not approve the change just because tests pass. Tests are evidence, not a substitute for contract review.
