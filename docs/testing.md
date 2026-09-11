# Testing and deployment validation

This document defines the current validation model. Historical probes, copied
commands, milestone profiles, and artifact-specific values are not supported
procedures.

## Selection model

Validation layers are cumulative:

1. Focused Rust unit, integration, regression, or doctests prove code behavior
   in the owning crate.
2. `cargo xtask gate local` verifies the RHEL-compatible workspace.
3. `cargo xtask windows all` separately verifies Windows-only code on the
   explicitly configured native endpoint.
4. Capability-named `cargo xtask` workflows verify process, service,
   deployment, integration, or live behavior.

A higher layer never substitutes for a lower one. Report pass, fail, blocked,
or not-run for each applicable layer.

## Operational input policy

Remote and live workflows do not provide experiment-derived defaults. Supply
the endpoint, target identity, paths, memory bounds, workload sizes, sampling
intervals, retry counts, and time limits for the current run. Derive memory
geometry and current allocation from fresh alias-scoped live XML. Select policy
and timing values from the current deployment contract and record them in the
run evidence.

Format/schema limits and normative product safety constants remain in their
owning Rust modules and contracts. They are not test profiles.

## Focused and local Rust validation

Run the narrowest test while developing, for example:

```bash
cargo test -p virtio-mem-xtask qualification --locked
cargo test -p virtio-mem-host cli --locked
```

Then run:

```bash
cargo xtask gate local
```

The local gate checks formatting, locked release builds and tests,
warnings-denied Clippy, and `git diff --check` for the shared core, host, and
tooling crates. It is not native Windows evidence.

## Native Windows gate

Configure every remote input explicitly:

```bash
export VIRTIO_MEM_WINDOWS_SSH=SSH_TARGET
export VIRTIO_MEM_WINDOWS_DIR=ABSOLUTE_WINDOWS_WORKSPACE
export VIRTIO_MEM_WINDOWS_ARTIFACTS=LOCAL_ARTIFACT_DIRECTORY
export VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS=CONNECT_BOUND
export VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS=OPERATION_BOUND
cargo xtask windows all
```

Optional `VIRTIO_MEM_WINDOWS_KNOWN_HOSTS_FILE` and
`VIRTIO_MEM_WINDOWS_IDENTITY_FILE` select operator-owned SSH material. The tool
checks the endpoint, synchronizes the non-ignored working tree once, builds,
tests, formats, lints, fetches the service executable, and verifies its hash.
It does not install or start a service or change guest memory.

For pinned repeatability evidence, choose the number of aggregate runs rather
than relying on a historical milestone count:

```bash
cargo xtask windows verify SHA256:EXPECTED_HOST_FINGERPRINT --runs RUN_COUNT
```

Evidence is written below the configured artifact directory.

## Windows workload helper

The workload binary accepts no operational defaults:

```text
virtio-mem-workload.exe \
  --workload-id RUN_ID \
  --mode resident|committed \
  --peak-bytes PEAK_BYTES \
  --retained-bytes RETAINED_BYTES \
  --max-allocation-bytes SAFETY_CAP_BYTES \
  --peak-hold-seconds PEAK_HOLD \
  --settled-hold-seconds SETTLED_HOLD \
  --renewed-hold-seconds RENEWED_HOLD \
  --refresh-interval-seconds REFRESH_INTERVAL
```

The cap must be independently selected for the test guest. Resident mode
refreshes committed pages at the configured interval; committed mode does not
touch them. The helper emits flushed versioned JSON lines and has no resize,
QGA, service-installation, or libvirt interface.

## QGA and host readiness

Use an explicit VM, retry count, and command bound:

```bash
cargo xtask qga VM_NAME \
  --attempts ATTEMPT_COUNT \
  --command-timeout-seconds COMMAND_BOUND \
  --connect LIBVIRT_URI
```

The workflow validates `guest-info`. If the optional downstream memory command
is absent, it validates the `dommemstat` fallback shape. Allocation authority
still comes only from alias-scoped live virtio-mem `current`.

## Configuration and deployment

The Windows service requires the versioned configuration file at its product
configuration path. Missing or invalid configuration fails startup; a VM name,
service identity, telemetry path, polling interval, and shutdown timeout are
never inferred from a previous test guest.

Build and hash the candidate with `cargo xtask windows all` before invoking the
service executable's `install`, `start`, `stop`, or `remove` commands in an
appropriately elevated Windows terminal. Validate the configured identity,
least-privilege account, ProgramData ACLs, advancing telemetry records, event
records, clean stop, and final SCM state. Do not combine service lifecycle with
memory mutation.

For the host, derive a complete instance file from
`host/systemd/virtio-mem-host.conf.example`, replace every required marker from
reviewed deployment evidence, and validate it with the product CLI before
installation. The checked-in unit is fail-stop and inherits the host service
manager's configured stop timeout; deployment monitoring controls reviewed
restart policy. Candidate installation, inventory, and system service changes
use capability-named typed xtask workflows. Repeatable parsing, polling,
safety, evidence, cleanup, and rollback remain in Rust tooling.

## Reversible live resize

`cargo xtask live-resize` is the supported test harness. It requires explicit
run bounds and delegates both forward and rollback mutation to the reviewed,
attestation-aware host product binary:

```bash
cargo xtask live-resize VM_NAME DEVICE_ALIAS TARGET_BYTES \
  --forward-timeout-seconds FORWARD_BOUND \
  --rollback-timeout-seconds ROLLBACK_BOUND \
  --sample-interval-seconds SAMPLE_INTERVAL \
  --command-timeout-seconds COMMAND_BOUND \
  --minimum-target-bytes DEPLOYMENT_FLOOR \
  --host-min-headroom-bytes HOST_RESERVE \
  --attestation REVIEWED_ATTESTATION \
  --host-cli HOST_BINARY \
  --connect LIBVIRT_URI \
  --log EVIDENCE_CSV
```

Without `--apply`, the workflow performs product-level dry-run validation only.
With `--apply`, it captures initial state, requires a converged device and QGA,
checks alignment and headroom, applies one request, waits for convergence, and
requests the captured initial allocation on failure and completion.
`--keep-target` is non-reversible and requires explicit authorization.

Before apply, issue the execution notice required by the repository
instructions. If rollback does not converge, report the final live state as a
critical failure; do not issue ad hoc follow-up requests.

## Unattended automatic-controller qualification

`cargo xtask qualification` is the supported resident/committed workload
workflow. A start command must explicitly supply:

- VM, device alias, SSH target, libvirt URI, controller unit, guest service,
  workload executable, and raw-telemetry path;
- workload mode, peak/retained/cap byte counts, all hold durations, and the
  resident refresh interval;
- host sampling interval, external-command timeout, final observation window,
  and required observed growth/reclaim deltas; and
- output root when the repository artifact directory is not appropriate.

Run `cargo xtask help` for the authoritative option spelling. Start without
`--apply` to validate and print the complete versioned configuration. With
`--apply`, the detached supervisor observes the installed controller and does
not issue competing resize requests.

```bash
cargo xtask qualification status RUN_ID
cargo xtask qualification review RUN_ID
```

Each run records configuration, status, events, host metrics, workload phases,
controller logs, observations, and summary as durable JSON/JSONL artifacts.
A successful workload process alone is not qualification success; both guest
health and the configured resize acceptance criteria must pass.

## Privilege and live health

Normal builds and gates run as the development user. A required privileged RHEL
operation uses an explicit xtask elevation option: the unprivileged parent
validates scope, invokes its current prebuilt executable through one outer
`sudo`, and persists the typed result as the invoking user. Never run Cargo as
root or create a privileged shell script.

Before and after live mutation, record domain identity and QEMU continuity,
QGA, an independent authenticated guest command, named services/applications,
installer and pending-reboot state, and bounded crash/reboot evidence. Reboot
and shutdown remain separately authorized operations.

## Reporting

For each applicable layer, report the exact command, target/mode, outcome, and
blocker. A live or deployment report also includes configured time bounds,
evidence paths/hashes, expected and observed effects, cleanup/rollback status,
and final state. Never promote an old run, an unavailable platform, or a dry
run into current apply evidence.
