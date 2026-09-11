# Repository operating instructions

Read `readme.md`, `docs/architecture.md`, `BACKLOG.md`, `docs/testing.md`,
`docs/engineering-standards.md`, and the applicable contract before changing
the repository. Use `.github/prompts/README.md` to select a task prompt.

## Language and ownership

- Product code and maintained automation use Rust.
- Bash is limited to a generated or task-specific privileged process boundary.
- Do not add another language, a maintained shell workflow, or duplicated
  parsing, policy, polling, timeout, evidence, cleanup, or rollback logic.
- The Windows service collects native telemetry, publishes versioned
  allocation-free records, and owns only its local lifecycle and observability.
- The host controller owns QGA, libvirt/QEMU state, compatibility, headroom,
  target calculation, and actuation. Windows never receives resize authority.
- `cargo xtask` owns repository gates, remote native-Windows orchestration,
  artifact verification, and repeatable cross-process or live workflows.

## Implementation rules

- Follow the documented architecture and current contracts; remove superseded
  behavior instead of preserving it implicitly.
- Require explicit target identity. Do not discover mutation targets broadly.
- Keep memory quantities as checked byte counts and derive device geometry from
  fresh alias-scoped live state.
- Put operational sizes, time bounds, sampling intervals, workload durations,
  endpoints, paths, and service identities in validated configuration or
  explicit command arguments. Do not copy values from prior runs into defaults.
- Preserve stable public contracts unless the task updates the contract.
- Prefer safe Rust, small testable functions, structured errors, injected side
  effects, and minimal dependencies. Justify and test any `unsafe` code.
- Never commit credentials, private keys, tokens, production data, or guest
  memory contents.

## Validation model

Validation layers are cumulative:

1. Add and run focused Rust tests in the crate that owns changed behavior.
2. Run `cargo xtask gate local` after focused tests pass.
3. Run the applicable capability-named `cargo xtask` workflow when acceptance
   crosses a platform, process, service manager, deployment, or live boundary.
4. Report native Windows, RHEL, deployment, and live results separately. An
   unavailable or unrun layer is not a pass.

Use `gate` only for non-mutating quality aggregation. Every deployment, live,
or privileged workflow belongs in `tools/xtask`; do not create shell scripts
for elevation, orchestration, parsing, evidence, cleanup, or rollback.

## Live-system safety

Task-scoped beta builds, tests, candidate installation, service lifecycle,
read-only inspection, and bounded reversible resize validation are authorized
when the target is unambiguous and the task requires them.

Before mutation, report the resolved target, command, expected effect, selected
time bounds, and rollback. Capture initial state; validate the exact VM and
device alias; preserve attestation, compatibility, headroom, retention-floor,
freshness, and convergence gates; and restore the captured requested size for a
reversible test. Stop on ambiguity or failed safety checks.

Guest health requires hypervisor continuity, QGA, an independent authenticated
guest command, named service/application checks, and bounded crash/reboot
evidence. Do not overlap an installer, update, or pending reboot.

When elevation is needed, build `xtask` unprivileged and use its explicit
elevation option. The unprivileged process must validate the complete scope,
invoke its current prebuilt executable through one outer `sudo`, collect typed
output, and persist evidence as the invoking user. Never run Cargo as root,
accept open-ended privileged commands, or automate credentials.

Explicit current-turn approval is still required for reboot or shutdown,
deleting pre-existing resources, persistent VM/firmware/driver/network/storage/
ACL/security changes, disabling safeguards, non-reversible resize, or unrelated
mutations.

## Documentation and task tracking

`BACKLOG.md` is the product execution board and
`docs/build-test-tooling-roadmap.md` is the tooling board. Update the applicable
task, contracts, `docs/testing.md`, and status documents in the same change.
Document supported commands and behavior, not obsolete procedures or transient
test observations. Record exact validation outcomes and blockers without
turning historical evidence into current defaults.
