# Copilot Instructions

This project is written primarily by AI coding agents. Follow the repository operating rules before changing code.

## Language Requirements

⚠️ STRICT RULE: This project may only use Rust and Bash.

- ✅ Allowed: Rust, Bash
- ❌ Forbidden: Go, C#, PowerShell, Python, Node.js, Java, or any other language
- ✅ Rust for any required service or program logic
- ✅ Rust for maintained build, test, validation, and orchestration tooling
- ✅ Bash only for generated or task-specific process/elevation boundaries

## Required Entry Point

Read the architecture and design docs first:

- Project overview: [readme.md](../readme.md)
- Architecture and service boundaries: [docs/architecture.md](../docs/architecture.md)
- Execution board / dev runbook: [BACKLOG.md](../BACKLOG.md)
- Validation and testing strategy: [docs/testing.md](../docs/testing.md)
- Cross-cutting standards: [docs/engineering-standards.md](../docs/engineering-standards.md)
- Build/test tooling roadmap: [docs/build-test-tooling-roadmap.md](../docs/build-test-tooling-roadmap.md)
- Prompt selection index: [prompts/README.md](prompts/README.md)

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
- Collecting native Windows memory telemetry
- Publishing versioned advisory demand reports
- Local lifecycle, configuration, and observability

**Windows service** must not:

- Directly invoke Linux commands
- Access host storage or devices
- Change host-level settings
- Issue or receive host resize commands

**RHEL host controller** owns QEMU Guest Agent requests, live libvirt/QEMU
state, compatibility checks, host headroom, and virtio-mem actuation. The
Windows service must not open the QGA virtio-serial device because the
installed QEMU Guest Agent process owns that channel.

**Rust tooling (`cargo xtask`)** owns:

- Repository quality gates and native-Windows build/test orchestration
- Environment and dependency checks
- Remote build orchestration and artifact verification
- Higher-level feature, deployment, service/process, component-integration,
  end-to-end, and bounded live-validation workflows
- Reusable parsing, polling, evidence capture, timeout, and rollback logic

**Task-specific Bash batches** own only the exact, reviewable process boundary
needed for one privileged operation. They must not:

- Introduce unreviewed privileged actions
- Hide errors or skip `set -euo pipefail`
- Depend on Go toolchains or Go build flows
- Reimplement Rust XML/JSON parsing, unit conversion, policy, or reusable polling

## Test and Tool Selection

Testing is cumulative. Choose every layer required by the changed behavior;
never use a higher layer as a substitute for a lower one.

1. **Code-level Rust behavior:** add unit, crate integration, or doctests in the
   affected product/tooling crate. Run the narrowest direct `cargo test`
   command while developing. Parsing, policy, state machines, error paths, and
   regressions belong here even when a live workflow also exercises them.
2. **Repository quality gate:** after focused tests pass, run
   `cargo xtask gate local`. It aggregates formatting, locked builds/tests,
   warnings-denied Clippy, and diff checks for the RHEL-compatible workspace.
   It verifies the change set; it does not replace writing or running focused
   Rust tests.
3. **Platform and workflow validation:** use the applicable `cargo xtask`
   command when acceptance crosses a process, service manager, machine,
   component, deployment, or live-system boundary. Native Windows evidence is
   a separate gate. Live and mutating workflows remain separately scoped and
   reported.
4. **Privileged process boundary:** when one approved RHEL workflow needs
   elevation, use the task-specific Bash batch procedure. Bash only establishes
   the reviewable process boundary and invokes prebuilt Rust behavior.

Add or extend `tools/xtask` when a workflow is repeatable, is part of feature
acceptance, spans components/platforms/processes, or needs shared prerequisite,
timeout, evidence, cleanup, or rollback behavior. Extend the existing command
when its lifecycle and safety contract match; add a new stable workflow command
when they differ materially. Do not add an xtask command merely to wrap one
focused `cargo test`, and do not leave reusable workflow policy in prose or a
one-off shell script. Reserve `gate` commands for non-mutating quality gates;
deployment or live mutation must use an explicitly named workflow command.

Follow [docs/testing.md](../docs/testing.md) for workflow organization,
invocation, and result reporting. Use exactly one matching task prompt from
[prompts/README.md](prompts/README.md) as the primary template; combine it with
a domain prompt only when both scopes genuinely apply.

## Shell and live-system safety

The project is in beta qualification. Building, testing, and task-scoped live
validation are authorized by default; an agent must not stop merely to request
another approval for the operations listed below.

- Agents may run repository build, format, lint, unit, integration, install,
  service-lifecycle, and validation commands by default.
- Agents may inspect the documented RHEL host, Windows test guest, libvirt/QEMU
  domain, QEMU Guest Agent, and related systemd units needed by the task.
- Agents may build and install a task's candidate artifacts, start, stop, or
  restart the relevant test service, and run bounded live resize/convergence
  tests by default when the target is uniquely identified by the task or the
  repository's documented test configuration.
- A live resize must use the repository's safety gates: capture the initial
  state, validate the exact VM and device alias, use an aligned bounded target,
  enforce timeouts and convergence checks, and restore the initial requested
  size on failure and at the end of a reversible test. Never bypass an
  attestation, compatibility, headroom, retention-floor, or rollback gate.
- Before a live mutation, report the resolved target, command or task script,
  expected effect, timeout, and rollback. This is an execution notice, not an
  approval request; continue with the operation unless the scope is ambiguous
  or a required safety precondition fails.
- For guest lifecycle testing, `virsh domstate` or one QGA reply is not a
  sufficient health check. Before and after the mutation, record hypervisor
  continuity, prove the guest OS can execute an independent authenticated
  command, check the named guest services/application, and inspect bounded OS
  crash/reboot evidence. Refuse to start while an unrelated installer, update,
  or pending reboot can overlap the test. After an expected reboot, require a
  quiet stabilization window with repeated end-to-end checks and no unexpected
  second boot before proceeding to resize, attestation, or other persistent
  work.
- Privileged task-scoped validation is also allowed by default. Batch related
  privileged commands into one task-specific Bash script under the ignored
  `.vscode-artifacts/privileged-tasks/` directory and invoke it once with one
  outer `sudo`, following `.github/prompts/rhel-privileged-batch.prompt.md`.
  Commands inside the script must not invoke `sudo`, `su`, or `doas`. Use
  `cargo xtask` or the product Rust CLIs for reusable validation logic.
- Never ask the user to send a password, store credentials, or put a password
  in a script, environment file, command line, or repository. If the terminal
  prompts for authentication, the user types it directly; do not use `sudo -S`,
  echoed passwords, or password automation.
- Use the dedicated least-privilege service account for live libvirt or systemd
  validation. Do not add broad passwordless `sudoers` rules or run the
  controller as root merely to make a test pass.
- Explicit current-turn approval is still required for host or guest reboot or
  shutdown; deleting a pre-existing VM, service, user, data, or server-side
  file; persistent VM-definition, firmware, driver, network, storage, ACL, or
  security-policy changes; disabling a safety control; non-reversible resize;
  or mutations not required by the selected task. Task-created temporary
  services, files, and test configuration may be removed as part of the same
  task's cleanup. Resolve ambiguity before acting.
- If authorization or a safety check fails, stop and report the boundary. Do
  not weaken controls, retry piecemeal, or expand the task's scope.

## Contract Rules

- Keep API/RPC contracts stable: update documentation before or alongside behavior changes.
- Update `docs/api-contract.md` when request/response behavior or RPC interface changes.
- Update `docs/data-model.md` when persistence or state structures change.
- Update `docs/feature-matrix.md` when feature status, ownership, or platform support changes.

## Documentation Freshness

[BACKLOG.md](../BACKLOG.md) is the product execution source of truth;
[`docs/build-test-tooling-roadmap.md`](../docs/build-test-tooling-roadmap.md) is
the build/test-tooling execution source. Keep the applicable board accurate
after every session.

- Documentation updates are mandatory, not optional follow-ups. Ship doc changes in the same session as the code they describe.
- After completing a product task, update its BACKLOG.md task card status,
  handoff notes, and Ready Queue row. For a BT task, update its BT roadmap row
  and add a concise BACKLOG handoff note when repository-wide commands changed.
- Update `docs/roadmap.md` when phase status changes or scope shifts.
- Update `docs/feature-matrix.md` when features are added, completed, or changed.
- Update `docs/issues.md` when a tracked bug is resolved, with status and fix reference.
- Update `docs/testing.md` whenever adding new test procedures, CLI commands, debugging modes, or required validation steps.
  - Document exact commands needed to run tests locally.
  - Explain when to use `run` mode (local debugging) vs `install` mode (real service testing).
  - Include expected output and success criteria for each validation step.
- Update `docs/build-test-tooling-roadmap.md` for BT-M/BT-T tooling scope,
  dependencies, blockers, or status. Do not add BT tasks to the product
  milestone namespace.
- If a doc update cannot be completed, log the gap in the task card's handoff notes. Never skip silently.
- See [BACKLOG.md](../BACKLOG.md) § "Documentation Freshness Rules" for the full checklist and triggers.

## Validation Rules

- Rust changes: add and run focused direct Cargo tests for the changed behavior,
  then run `cargo xtask gate local`. Neither step substitutes for the other.
- Build/test tooling changes: update the BT roadmap, add deterministic xtask
  unit/integration tests, run their focused Cargo target, and then run
  `cargo xtask gate local`. Exercise the changed high-level workflow separately
  when safe prerequisites are available.
- Task-specific Bash changes: run `bash -n PATH` and the applicable dry run;
  these scripts are not maintained build/test entry points.
- Windows service changes: build and validate locally before committing.
  - Test the `run` command (non-service mode) for worker logic and lifecycle changes.
  - Document any new CLI modes or command-line options in `docs/testing.md`.
  - Report the exact cargo test results in the commit or task notes.
- Run every applicable code-level, repository-gate, platform, and workflow
  layer. Report each separately with the exact command, target/mode, outcome,
  and any environment or authorization blocker.
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
2. Make the smallest focused change and add/update code-level Rust tests in the
   same change.
3. Run the focused direct Cargo tests, then `cargo xtask gate local`, then each
   separately applicable native Windows or higher-level workflow gate.
4. Update affected documentation and the applicable product or BT task
   status/handoff notes.
5. Report exact results by validation layer and any environment blocker; never
   claim a check passed without running it.

## Safety Rules

- Discovery behavior must stay explicit-scope and allowlist friendly.
- Do not add broad or implicit network scans.
- Do not perform remote shell/admin actions unless the user explicitly asks.
- Handle errors explicitly and make operational failures actionable.
