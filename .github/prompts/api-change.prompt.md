---
agent: agent
description: Make an API change without contract drift
---

Before editing, read:

- `BACKLOG.md`
- `docs/api-contract.md`
- `docs/architecture.md`
- `docs/feature-matrix.md`
- `docs/engineering-standards.md`

For the change:

1. Update `docs/api-contract.md` first or alongside behavior when the QEMU Guest Agent or Rust API contract changes.
2. Update `docs/data-model.md` when state or persistence structures change.
3. Keep public Rust APIs minimal and backward-compatible unless a breaking change is explicit.
4. Add or update deterministic Rust validation tests, including malformed and boundary cases.
5. Run focused tests and the local validation flow from `windows/`.
6. Update `BACKLOG.md` handoff notes with exact validation.

Keep runtime logic in Rust, automation in Bash, and do not invent OpenAPI or generated-schema files that this repository does not use.

Shell safety:

- Limit changes to approved repository files. Do not edit/delete protected server files or change VM, libvirt, or systemd state without explicit approval naming the exact action.
- Run validation without elevated privileges where possible. If privilege is needed, ask first with the complete command, target, mutation, and rollback, then run the whole script once under `sudo`; never automate or collect password entry.
