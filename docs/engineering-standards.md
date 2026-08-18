# Engineering Standards

## Language Policy

This project is restricted to exactly two languages:

- Rust for any required service or program logic
- Bash for automation and scripting

No additional languages are permitted in source code, scripts, build tooling, or infrastructure definitions.

## Code Style

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

### Windows service lifecycle

- Keep SCM start/stop callbacks short; never perform an unbounded poll or blocking QEMU Guest Agent operation directly in a callback.
- Make start, running, stop-pending, stopped, and failed states explicit at the SCM boundary. Do not report running before the worker is ready.
- Use one cancellation path for operator stop and system shutdown. A normal cancellation must not be logged or exited as a crash.
- Make shutdown idempotent: stop scheduling new work, allow in-flight work to finish within a bounded deadline, release channel resources, and then exit.
- Treat an unexpected worker failure as a failed service, preserve its error context in Windows event logging, and return a non-zero process result so configured SCM recovery can restart it. Never leave a silent zombie process.
- Define and validate a stable service identity, executable path, startup mode, recovery policy, and least-privilege service account during installation.
- Prefer configuration files or other documented persistent configuration for multiple settings; avoid undocumented or security-sensitive startup arguments.
- Keep service identity, QGA pipe path, polling interval, shutdown timeout, and
  service account in validated configuration; use `LocalService` by default
  and require an explicit documented reason to elevate.
- Cancellation waits must be wakeable; do not use an uninterruptible sleep for
  the polling interval.

### RHEL host-controller lifecycle

- Run one explicitly configured VM and virtio-mem alias per systemd instance;
  do not implement broad VM discovery or implicit multi-target scheduling.
- Invoke `virsh` through fixed argument vectors with a finite timeout; never
  use a shell, string interpolation, or an unbounded external command.
- Refresh and validate the selected live XML immediately before a resize. Do
  not issue a request while `requested != current` or replay one after restart.
- Use `SIGTERM` and `SIGINT` for one wakeable cancellation path. Operational
  failures must produce contextual journal output and a non-zero process exit.
- Configure an explicit non-login service account and verify its least-privilege
  libvirt authorization before enabling the unit. Do not silently run as root.

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

- Windows service does not invoke Linux commands
- Host automation remains separate from guest runtime logic
- All communication stays on the QEMU Guest Agent interface and validated host tooling
