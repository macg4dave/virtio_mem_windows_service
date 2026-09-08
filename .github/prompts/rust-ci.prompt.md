---
name: rust-ci
description: Update Rust validation and Bash automation without reducing quality gates
---

Read `docs/testing.md`, `docs/engineering-standards.md`, `BACKLOG.md`, and existing `Makefile`/`scripts/` files first.

Rules:

- Keep automation in Bash and preserve `#!/bin/bash` plus `set -euo pipefail`.
- Keep Rust validation reproducible and run formatting, tests, Clippy, and release build checks where supported.
- Do not hide failures, ignore exit codes, or depend on Go/Python/Node.js/PowerShell.
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

- Run formatting, Cargo tests, Clippy, and builds as the normal user whenever possible.
- Beta build/test work may install its candidate, exercise the relevant service lifecycle, inspect live VM/libvirt state, and run a bounded reversible resize by default. Give the execution notice and follow the live-system rules in `.github/copilot-instructions.md`.
- Batch privileged steps into one task script and one outer `sudo`; never automate or collect the password. Reboots, deletions, persistent configuration changes, disabled safety gates, and unrelated mutations still require explicit approval.
