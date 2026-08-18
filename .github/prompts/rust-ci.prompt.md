---
name: rust-ci
description: Update Rust validation and Bash automation without reducing quality gates
---

Read `docs/testing.md`, `docs/engineering-standards.md`, `BACKLOG.md`, and existing `Makefile`/`scripts/` files first.

Rules:

- Keep automation in Bash and preserve `#!/bin/bash` plus `set -euo pipefail`.
- Keep Rust validation reproducible and run formatting, tests, Clippy, and release build checks where supported.
- Do not hide failures, ignore exit codes, add privileged actions, or depend on Go/Python/Node.js/PowerShell.
- Do not remove existing checks unless explicitly requested and documented.
- Keep Windows-target assumptions explicit; distinguish local Rust validation from live RHEL/libvirt/QEMU validation.
- Make scripts explicit-scope and allowlist friendly; never add broad network scans or remote admin actions.
- Update `docs/testing.md` and `BACKLOG.md` if commands or validation status changes.

Task:
"""
<CI, Makefile, or Bash validation change>
"""

Validate the affected script and run the relevant Rust checks from `windows/`. Report exact output and environment blockers.

Shell safety:

- Run formatting, Cargo tests, Clippy, and builds as the normal user whenever possible; these checks must not silently gain root privileges.
- Do not add or run privileged install, service, VM, libvirt, or filesystem mutation steps without explicit current-turn approval naming the target and action.
- Never invoke `sudo`, `su`, or `doas` without current-turn approval naming the complete command, target, mutation, and rollback. After approval, run the whole script once under `sudo`, not a mixture of privileged subcommands. Never automate or collect the password; the user types it directly into the terminal.
