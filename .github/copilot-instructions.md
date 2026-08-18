# Copilot Instructions

This project is written primarily by AI coding agents. Follow the repository operating rules before changing code.

## Language Requirements

⚠️ STRICT RULE: This project may only use Rust and Bash.

- ✅ Allowed: Rust, Bash
- ❌ Forbidden: Go, C#, PowerShell, Python, Node.js, Java, or any other language
- ✅ Rust for any required service or program logic
- ✅ Bash for automation, validation, and helper scripts

## Required Entry Point

Read the architecture and design docs first:

- Project overview: [readme.md](../readme.md)
- Architecture and service boundaries: [docs/architecture.md](../docs/architecture.md)
- Execution board / dev runbook: [BACKLOG.md](../BACKLOG.md)
- Validation and testing strategy: [docs/testing.md](../docs/testing.md)
- Cross-cutting standards: [docs/engineering-standards.md](../docs/engineering-standards.md)

## Prime Directives

- Do not invent architecture. Follow the roadmap, feature matrix, and existing docs.
- Prefer boring, proven solutions over novel ones.
- Keep every feature traceable to documentation.
- Avoid formatting-only churn.
- Never commit secrets, credentials, private keys, tokens, PSKs, or production data.
- Preserve stable IDs and existing API contracts unless the change explicitly updates the contract.
- Add regression tests for fixed logic errors.
- No Go code or Go planning artifacts are permitted in this repository.

## Service Boundaries

**Windows service** owns:

- Running as a service in Windows
- Exposing memory metrics via QEMU Guest Agent
- Receiving and processing memory change requests
- Local performance monitoring

**Windows service** must not:

- Directly invoke Linux commands
- Access host storage or devices
- Change host-level settings

**Bash automation** owns:

- Local validation scripts
- Build helper commands
- Environment checks and operational setup

**Bash scripts** must not:

- Introduce unreviewed privileged actions
- Hide errors or skip `set -euo pipefail`
- Depend on Go toolchains or Go build flows

## Shell and server safety

- Treat the RHEL host, Windows guest, libvirt domains, systemd units, and all
  files outside this repository as protected resources.
- Never delete, edit, move, overwrite, chmod, chown, install, restart, stop,
  reboot, resize, or otherwise mutate a protected resource without explicit
  approval in the current turn naming the target and intended action. A broad
  request to investigate or test is not approval for mutation.
- Read-only discovery is the default. Before any potentially mutating command,
  state its exact scope and expected effect, then wait for approval.
- Never invoke `sudo`, `su`, or `doas` without first asking for approval in the
  current turn. The approval request must name the complete command, protected
  target, expected mutation, and rollback behavior.
- When approval is granted for a privileged script, run the complete script
  once under `sudo` (for example, `sudo bash scripts/example.sh ...`) rather
  than mixing privileged and unprivileged subcommands. Do not add `sudo` to
  individual commands opportunistically or chain unrelated privileged actions.
- Never ask the user to send a password, store credentials, or put a password
  in a script, environment file, command line, or repository. If the terminal
  prompts for sudo authentication, the user must type it directly; do not use
  `sudo -S`, echoed passwords, or password automation.
- Prefer unprivileged, hermetic tests. If root or a privileged capability is
  genuinely required, stop at the permission boundary and ask before running
  the approved whole-script command.
- Use a dedicated least-privilege service account for approved live libvirt or
  systemd validation. Do not add passwordless broad `sudoers` rules or run the
  controller as root merely to make a test pass.
- Do not edit or delete server-side files, service units, VM definitions, or
  guest state from an automation prompt unless the user explicitly authorizes
  that specific change.

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
- Update `docs/testing.md` whenever adding new test procedures, CLI commands, debugging modes, or required validation steps.
  - Document exact commands needed to run tests locally.
  - Explain when to use `run` mode (local debugging) vs `install` mode (real service testing).
  - Include expected output and success criteria for each validation step.
- If a doc update cannot be completed, log the gap in the task card's handoff notes. Never skip silently.
- See [BACKLOG.md](../BACKLOG.md) § "Documentation Freshness Rules" for the full checklist and triggers.

## Validation Rules

- Rust changes: build and validate locally with `cargo build` and `cargo test` when available.
- Run `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` for Rust changes when the toolchain supports them.
- Bash changes: run the script or a focused validation command locally.
- Windows service changes: build and validate locally before committing.
  - Test the `run` command (non-service mode) for worker logic and lifecycle changes.
  - Document any new CLI modes or command-line options in `docs/testing.md`.
  - Report the exact cargo test results in the commit or task notes.
- All testing is performed locally. Document any blocking issues for local validation.
- Stack changes: validate that services can start and communicate correctly.
- If a command cannot run locally, state the exact blocker.

## Rust Engineering Rules

- Use the Rust 2021 edition and preserve the crate's existing MSVC/Windows target assumptions.
- Prefer safe, idiomatic Rust: explicit `Result`/`Option` handling, structured error types, and small testable functions.
- Avoid `unwrap()`, `expect()`, panics, global mutable state, and `unsafe`; if one is necessary, justify it and test the failure path.
- Keep public items minimal. Add documentation for new public APIs and preserve existing API behavior unless the task explicitly changes the contract.
- Prefer borrowing over cloning, but do not trade away clarity for micro-optimizations. Measure before optimizing.
- Keep parsing and policy logic deterministic and independent of live VMs so unit tests remain hermetic.
- Add regression tests for bug fixes, including boundary values, malformed input, error paths, and request convergence where relevant.
- Do not add dependencies casually. Review the dependency's license, maintenance, feature flags, and Windows compatibility before changing `Cargo.toml` or `Cargo.lock`.
- Keep Windows service code, QEMU Guest Agent transport, and pure memory policy logic separated so each can be tested independently.
- Do not use Rust prompts or examples that assume Go, Python, Node.js, PowerShell, OpenAPI, Linux service managers, or unrelated UI frameworks.

## Rust Change Checklist

Before declaring a Rust task complete:

1. Read the relevant architecture, backlog, contract, data-model, and testing documentation.
2. Make the smallest focused change and update tests in the same change.
3. Run formatting, tests, Clippy, and a release build when practical.
4. Update affected documentation and the `BACKLOG.md` task status/handoff notes.
5. Report exact validation results and any environment blocker; never claim a check passed without running it.

## Safety Rules

- Discovery behavior must stay explicit-scope and allowlist friendly.
- Do not add broad or implicit network scans.
- Do not perform remote shell/admin actions unless the user explicitly asks.
- Handle errors explicitly and make operational failures actionable.
