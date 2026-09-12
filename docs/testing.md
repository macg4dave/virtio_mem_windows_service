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

Host instance files use systemd `EnvironmentFile=` syntax. Values containing
Windows backslashes must be single-quoted so systemd preserves them; the typed
host deployment validator rejects an unquoted backslash instead of installing
a path that changes at service start.

Build and hash the candidate with `cargo xtask windows all`. The typed `cargo
xtask windows deploy MANIFEST --output EVIDENCE [--apply]` workflow owns
candidate replacement, versioned configuration, product service lifecycle,
rollback capture, ACL inspection, and advancing-record evidence. Do not combine
service lifecycle with memory mutation.

Use `cargo xtask calibration` for the allocation-neutral, two-sample visible
base measurement and `cargo xtask attestation` to generate the live-bound
document from an explicit reviewed input.

Use `cargo xtask windows service-cycle` for a bounded, evidence-producing
service restart and `cargo xtask windows diagnose-service` for SCM recovery and
structured EventData inspection. The coherent-deployment no-actuation gate is `cargo xtask
preflight`; with explicit identity, paths, bounds, and
`--apply-service-restart`, it proves live QGA delivery, exact durable replay,
producer-session rollover, current attestation, host headroom, rollback
evidence, controller safe hold, and unchanged allocation.

For the host, derive a complete instance file from
`host/systemd/virtio-mem-host.conf.example`, replace every required marker from
reviewed deployment evidence, and validate it with the product CLI before
installation. The checked-in unit is fail-stop and inherits the host service
manager's configured stop timeout; deployment monitoring controls reviewed
restart policy. Candidate installation, inventory, and system service changes
use capability-named typed xtask workflows. `cargo xtask host-deploy INSTANCE
--config CONFIG --attestation ATTESTATION --output EVIDENCE
--command-timeout-seconds N [--apply --elevate]` archives superseded files and
drop-ins, installs one coherent configuration, disables the instance, and
requires it to remain inactive with the fail-stop unit policy.

The production raw-telemetry configuration selects `qga-file`, supplies the
absolute Windows current-record path and a distinct absolute host
acknowledgement path, and keeps the QGA operation within the configured command
timeout. A deployment check must prove bounded open/read/close behavior and
must not substitute SSH, guest execution, or a shared writable directory. The
Windows atomic publisher retries only transient access-denied replacement
collisions for a bounded interval because QGA holds the current record open
without delete sharing; other publication errors remain fatal.

At the controller boundary, a telemetry transport interruption blocks the
current cycle without erasing previously accepted reclaim history. The next
accepted sample must still meet the configured maximum-gap rule. Invalid,
stale, replayed, identity-mismatched, or over-gapped evidence clears reclaim
readiness and requires a complete new history window.

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
  workload executable, protected Windows raw-telemetry path, and telemetry
  freshness/future-clock bounds;
- workload mode, peak/retained/cap byte counts, all hold durations, and the
  resident refresh interval;
- host sampling interval, external-command timeout, hard controller-ownership
  timeout, final observation window, and required observed growth/reclaim
  deltas, including a separate renewed-pressure growth delta; and
- when the telemetry producer must begin a new session after controller
  ownership starts, the explicit service-restart flag and its separate hard
  timeout; and
- output root when the repository artifact directory is not appropriate.

Run `cargo xtask help` for the authoritative option spelling. Start without
`--apply` to validate and print the complete versioned configuration. With
`--apply --elevate`, one outer sudo launches a bounded controller guard. The
guard requires the named unit to begin disabled/inactive, starts only that
unit, verifies automatic shrink is enabled in the running process, and returns
it to inactive on completion, supervisor failure, or the explicit hard
timeout. That same long-lived elevated child performs the run's fixed,
alias-scoped libvirt and QGA observations and publishes typed snapshots under
`/run`; this prevents a detached run from opening repeated Polkit
authorizations. The detached unprivileged supervisor runs the Windows
workload, validates and correlates those snapshots, writes the durable
evidence, and never issues a competing resize request.

`--apply-service-restart --guest-service-cycle-timeout-seconds N` is an
explicit live operation, not a preflight convenience. After the controller
guard is active, it cycles only the named Windows telemetry service through the
maintained authenticated SSH path and waits within `N` seconds for a matching
`controller_decision` from the new session before launching the workload. This
preserves the replay gate when telemetry advanced while the controller was
inactive. The service-cycle timeout is included in the required controller
ownership bound and its evidence is stored in the qualification event stream.

```bash
cargo xtask qualification status RUN_ID
cargo xtask qualification review RUN_ID
```

Each run records configuration, status, events, host metrics, workload phases,
controller logs, observations, and summary as durable JSON/JSONL artifacts.
A successful workload process alone is not qualification success; both guest
health and the configured resize acceptance criteria must pass.

The result analyzer separately grades initial growth, reclaim after the first
peak, and growth after renewed pressure. It rejects an observed lower request
while a prior request remains unconverged and rejects telemetry session changes
or sequence regressions within the workload. Runs for QA-T011 additionally use
`--require-renewed-during-pending-shrink`; this requires the last observed
settled-phase sample before renewed pressure to show a shrink still in
progress. These sampled invariants complement, rather than replace, controller
journal review.

## Windows-native pressure validation

The fixed-headroom qualification path is retained as historical platform and
reconciler evidence; it cannot qualify the replacement demand policy. Follow
QA-T025 onward before enabling pressure-aware actuation.

The first pressure work is measurement-only. On the supported native Windows
build, record API/counter availability, required privileges, low/high resource
notification state, rate-counter warm-up, sampling interval, and every
collection error. Unsupported or unwarmed is not zero and cannot authorize
reclaim.

Shadow validation must cover at least these semantically distinct workloads:

- resident allocation and release;
- committed address space that is not fully resident;
- useful file-cache/standby growth and reuse;
- sustained paging/page-output pressure; and
- telemetry interruption, counter reset, producer restart, and unsupported
  signal behavior.

Capture PerfMon or a bounded WPR/ETW trace as a Microsoft-supported diagnostic
oracle where needed, but do not make ETW an always-on controller dependency.
Compare raw samples, assessment reason, shadow candidate, fallback state, and
legacy candidate on the same timeline. Record false growth, missed pressure,
classification lag, and cache treatment explicitly.

For QA-T026, derive any proposed reusable-memory and paging-rate threshold from
the supported guest, workload, and observation-window evidence. Diagnostic
examples are not repository defaults or automatic pass/fail rules.

Applied qualification begins with growth only. Reclaim stays blocked until the
shadow review shows sustained high/healthy Windows evidence, adequate commit
headroom, no paging blocker, complete continuity, and an accepted fallback
contract. Existing attestation, host headroom, journaling, convergence,
no-overlap, and recovery checks continue unchanged.

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
