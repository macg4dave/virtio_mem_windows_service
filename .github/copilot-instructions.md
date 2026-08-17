# Copilot Instructions

This project is written primarily by AI coding agents. Follow the repository operating rules before changing code.

## Language Requirements

⚠️ STRICT RULE: This project may only use Go, Rust, and Bash.

- ✅ Allowed: Go, Rust, Bash
- ❌ Forbidden: C#, PowerShell, Python, Node.js, Java, or any other language
- ✅ Go for the Linux controller
- ✅ Rust for the Windows service
- ✅ Bash for scripts and automation

## Required Entry Point

Read the architecture and design docs first:

- Project overview: [readme.md](../../readme.md)
- Architecture and service boundaries: [docs/architecture.md](../../docs/architecture.md)
- Execution board / dev runbook: [BACKLOG.md](../../BACKLOG.md)
- Validation and testing strategy: [docs/testing.md](../../docs/testing.md)
- Cross-cutting standards: [docs/engineering-standards.md](../../docs/engineering-standards.md)

## Prime Directives

- Do not invent architecture. Follow the roadmap, feature matrix, and existing docs.
- Prefer boring, proven solutions over novel ones.
- Keep every feature traceable to documentation.
- Avoid formatting-only churn.
- Never commit secrets, credentials, private keys, tokens, PSKs, or production data.
- Preserve stable IDs and existing API contracts unless the change explicitly updates the contract.
- Add regression tests for fixed logic errors.

## Service Boundaries

**Linux controller service** (Go) owns:

- Reading Windows memory metrics via QEMU Guest Agent
- Polling and scheduling memory adjustments
- Calculating memory allocation logic
- Communicating with libvirt/QEMU
- Monitoring and logging

**Linux controller** must not:

- Access Windows registry or file system directly
- Execute arbitrary scripts on the Windows guest
- Bypass the QEMU Guest Agent interface

**Windows service** owns:

- Running as a service in Windows
- Exposing memory metrics via QEMU Guest Agent
- Receiving and processing memory change requests
- Local performance monitoring

**Windows service** must not:

- Directly invoke Linux commands
- Access host storage or devices
- Change host-level settings

## Contract Rules

- Keep API/RPC contracts stable: update documentation before or alongside behavior changes.
- Update `docs/api-contract.md` when request/response behavior or RPC interface changes.
- Update `docs/data-model.md` when persistence or state structures change.
- Update `docs/feature-matrix.md` when feature status, ownership, or platform support changes.

## Documentation Freshness

[BACKLOG.md](../BACKLOG.md) is the execution source of truth. Keep it accurate after every session.

- Documentation updates are mandatory, not optional follow-ups. Ship doc changes in the same session as the code they describe.
- After completing any task, update the BACKLOG.md task card status, handoff notes, and Ready Queue row.
- Update `docs/roadmap.md` when phase status changes or scope shifts.
- Update `docs/feature-matrix.md` when features are added, completed, or changed.
- Update `docs/issues.md` when a tracked bug is resolved, with status and fix reference.
- If a doc update cannot be completed, log the gap in the task card's handoff notes. Never skip silently.
- See [BACKLOG.md](../BACKLOG.md) § "Documentation Freshness Rules" for the full checklist and triggers.

## Validation Rules

- Go changes: run `gofmt`, `go vet ./...`, and `go test ./...` when available.
- Windows service changes: build and validate locally before committing.
- All testing is performed locally. Document any blocking issues for local validation.
- Stack changes: validate that services can start and communicate correctly.
- If a command cannot run locally, state the exact blocker.

## Safety Rules

- Discovery behavior must stay explicit-scope and allowlist friendly.
- Do not add broad or implicit network scans.
- Do not perform remote shell/admin actions unless the user explicitly asks.
- Handle errors explicitly and make operational failures actionable.
