# Engineering Standards

## Language Policy

This project is restricted to exactly three languages:

- Go for the Linux controller
- Rust for the Windows service
- Bash for automation and scripting

No additional languages are permitted in source code, scripts, build tooling, or infrastructure definitions.

## Code Style

### Go

- Format: `gofmt`
- Lint: `go vet ./...`
- Test coverage: Aim for >80%
- Package structure: `cmd/`, `pkg/`, `internal/`
- Min version: Go 1.20+

### Rust

- Format: `rustfmt`
- Lint: `cargo clippy`
- Test coverage: Aim for >80%
- Edition: 2021
- Min version: Rust 1.70+
- Build: `cargo build --release`
- Test: `cargo test`

### Bash

- Format: `shfmt`
- Lint: `shellcheck`
- Shebang: `#!/bin/bash`
- Error handling: `set -euo pipefail`
- Target: Bash 4.0+

## Documentation Standards

- All features must be documented in [docs/feature-matrix.md](feature-matrix.md)
- API changes must update [docs/api-contract.md](api-contract.md)
- Data model changes must update [docs/data-model.md](data-model.md)
- Every completed task updates [BACKLOG.md](../BACKLOG.md) status

## Commit Standards

- Messages must reference task ID from BACKLOG.md
- No secrets, credentials, or private keys
- Keep commits atomic and focused
- Update docs in the same commit as code changes

## Service Boundary Rules

Respect the [architecture.md](architecture.md) service boundaries:

- Linux controller does not access Windows registry/filesystem
- Windows service does not invoke Linux commands
- All communication via QEMU Guest Agent interface
