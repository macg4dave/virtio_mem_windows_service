# Development

This describes the current implementation. The intended guest-driven
request/result boundary and review findings are in [the roadmap](roadmap.md).
Currently Linux still estimates demand from raw telemetry; moving those
choices into Windows is outstanding work. Core request/result types and the
replay journal are implemented in `memory_request.rs`, but production transport integration is not.

## Architecture

| Location | Responsibility and entry points |
| --- | --- |
| [Core](../crates/virtio-mem-core/src/lib.rs) | Shared telemetry/configuration types, device/XML validation, target estimation, reconciliation and pure pool allocation. |
| [Host](../host/src/main.rs) | Linux adapters and controller wiring. `runtime.rs` owns command lifecycle; `target_policy.rs` joins telemetry to policy and persists checkpoints; `resize_sink.rs` validates and executes effects. |
| [Windows](../windows/src/main.rs) | Native telemetry sampling, atomic publication, SCM lifecycle and the workload utility. Console and SCM startup share the worker constructor in `runtime.rs`. |
| [xtask](../tools/xtask/src/main.rs) | Local gates, remote Windows builds, deployment, diagnostics and qualification. These commands can have operational effects. |

The normal path is Windows sampling → atomic JSON telemetry → QGA file read →
host validation/estimation → pool ceiling → reconciliation → host reserve check
and virsh resize → observation of live XML. Local file telemetry is also
supported; there is no separate balloon-statistics demand policy.

## Control contract

This describes the implemented policy, not proposed cleanup. Its source owners
are [target estimation](../crates/virtio-mem-core/src/target_controller.rs),
[reconciliation](../crates/virtio-mem-core/src/reconciler.rs),
[runtime](../host/src/runtime.rs) and [checkpointing](../host/src/target_policy.rs).

- All targets are device bytes. Whole-guest physical RAM includes fixed RAM;
  the fixed base is derived per sample as `physical_total - current`.
- Alias-selected live XML supplies `requested` (intent), `current` (observed
  allocation), device size and block size. A successful command is not convergence.
- Telemetry must match VM/service identity, freshness and accepted
  session/sequence rules. Unchanged telemetry does not start a new resize.
  Acceptance and estimator consumption are saved in the same checkpoint.
- Instantaneous demand is the larger of the physical candidate
  `used + physical_reserve - fixed_base` and commit candidate
  `current + commit_reserve - commit_headroom`, floored at `min_memory_bytes`,
  aligned upwards and capped. The safe floor uses smaller reserves and cannot
  exceed demand. Arithmetic validation and intentional floors live in core.
- The effective maximum is device size minus 1 GiB, aligned down. There is no
  configured maximum. Minimum, current, reserves and hysteresis must align to
  the block size.
- Demand rises immediately. It falls to the history window's peak only after
  gap-free `reclaim_history_seconds` and sufficient downward hysteresis; the
  floor becomes the peak floor. A gap exceeding `reclaim_max_gap_seconds`
  restarts history. Commands use the configured grow/shrink steps.
- Each command's intent is saved before execution. Normal resizing waits for
  convergence; an owned pending shrink can be raised towards current when
  demand returns. Failed telemetry during owned shrink freezes it at current
  and latches. These are controlled replacements, not arbitrary stacked resizes.
- Growth timeout latches and stops that VM. A short shrink can settle by
  requesting current allocation, then pauses new shrink for one reclaim
  history window while allowing growth. Settlement failure latches.
- On restart, an owned converged command clears; a recorded but unapplied or
  conflicting command latches. Unowned divergence is observed, never replayed.
  Latch clearing is an explicit CLI operation. The original command
  start time survives restart; missing legacy timing, backward clock movement and
  expired bounds fail closed. The short-shrink pause remains process-local.
- Evaluation errors hold the VM. Unreadable live state retains its last pool
  charge and latches if a command is owned. Member failures can be isolated;
  the daemon stops when every member has stopped. Configuration, persistence
  or other propagated runtime errors can still terminate the daemon.
- Growth must leave the configured host reserve, including pending/uncertain
  growth from earlier rounds and restart as well as new grants in this round.
  The pool charges `max(requested, current, durable intent target)`;
  unreadable members retain their last charge, or the whole pool budget before
  their first successful observation. Free bytes go to larger priorities first,
  with configuration order breaking ties. Pool accounting is recomputed, not
  persisted, and never forces another guest to release RAM.

## Configuration and deployment

[The host TOML example](../host/systemd/virtio-mem-host.toml.example) owns the
key inventory; replace every `REQUIRED_*` marker using reviewed deployment
values. Unknown keys are rejected. Global timing/reserve settings apply to all
`[[vm]]` tables; `pool_bytes` is required for multiple VMs. Per-VM policy paths
must be distinct. `observe_only` defaults to false: explicitly set it to true
for trials that must not resize. There is no separate shrink-disable switch.

The [systemd unit](../host/systemd/virtio-mem-host@.service) runs
`virtio-mem-host --config /etc/virtio-mem-host/%i.toml` as `virtio-mem-host`,
using `qemu:///system` and `Restart=no`. Non-interactive libvirt management
access is provisioned once; the daemon neither elevates nor stores passwords.
The [polkit rule](../host/systemd/50-virtio-mem-host.rules) grants connection-wide
management to that account, not a per-VM ACL. Existing group access may already
provide it. Checkpoint paths must be absolute without `..`; symlink/hard-link checkpoint
files are rejected. The daemon and operator mutations lock the libvirt domain
UUID, and checkpoint writers also hold a sidecar file lock until dropped. Do not
unlink lock files. Stop the controller before `resize`, `abandon-shrink` or
`clear-latch`, including commands from another config.

Windows atomic replacement and Linux durable checkpoints have
different platform contracts and remain separate implementations.

Windows loads `C:\ProgramData\VirtioMemService\config.json`; its schema is
[defined in core](../crates/virtio-mem-core/src/windows_service_config.rs).
Telemetry location and service identity must match the host configuration.
Windows configuration is bounded to 64 KiB and shared wire/path validation is
also used by deployment. Paths use absolute drive syntax; ambiguous Win32 forms,
collisions with telemetry/backups/temporary names and existing file aliases are
rejected. Poll and shutdown timing must round-trip exactly in milliseconds.
Installation keeps configuration administrator-owned/read-only to LocalService;
publication files alone inherit modify rights. Existing files receive explicit
ACLs. The executable path passed to SCM is quoted.

`help` documents recovery when configuration is missing or damaged: use an
administrator terminal with `sc.exe stop SERVICE_NAME` and, when removing the
service, `sc.exe delete SERVICE_NAME`, using its installed identity. Program
commands reject trailing arguments. A worker panic is reported as failure; a
worker exceeding the shutdown bound terminates the process for SCM recovery.
Only the host thread publishes lifecycle status. Publication retries sharing and
lock errors within the configured bound, wakes on stop, and treats diagnostic
backup errors independently of current publication.

Migration from the former environment-variable deployment requires copying
retained values from `<instance>.conf` to `<instance>.toml`. Removed demand,
pressure, acknowledgement and shadow-margin keys have no TOML equivalent.
`xtask host-deploy` backs up the old `.conf`; preserve non-resizing deployments
with `observe_only = true`. Review ownership/ACLs and attestation before applying.

Telemetry acceptance now lives in `accepted_telemetry` in the policy checkpoint;
the daemon does not use the former acknowledgement file. Upgraded checkpoints
without acceptance start with an empty accepted-session history but still
validate identity and freshness. The retired `pressure_shadow` field loads and
is dropped on save. Controller status is version 4; use matching host/xtask
builds. The preflight tool's `--ack-path` is separate test evidence, not daemon
state, and remains a supported option.

Host `status` and `clear-latch` require `--vm NAME` with a multi-VM config.
`xtask host-deploy` and `qualification` still require single-VM files.
`xtask controller-status` selects the named VM but assumes an instance-named
config path. Shared deployment support is tracked in the backlog.

## Local validation

Use stable Rust with rustfmt and Clippy. The local gate is implemented in
[xtask](../tools/xtask/src/local.rs):

```bash
cargo xtask gate local
```

It checks formatting, builds release binaries, tests the three non-Windows
packages, runs Clippy on all targets with warnings denied, and checks whitespace.
`make local-gate` and the VS Code tasks delegate to xtask. There is no configured
pedantic/restriction lint group or numeric file/function limit.

If xtask cannot run, the equivalent direct commands are:

```bash
cargo fmt --all -- --check
cargo build --workspace --exclude virtio-mem-service --all-features --release --locked
cargo test --workspace --exclude virtio-mem-service --all-features --locked
cargo clippy --workspace --exclude virtio-mem-service --all-targets --all-features --locked -- -D warnings
git diff --check
```

For Windows, select an installed target/toolchain and produce a binary, for
example `cargo build -p virtio-mem-service --target x86_64-pc-windows-gnu --locked`
with MinGW available, or use the selected MSVC target. Type-checking on Linux
does not validate native SCM, Win32 sampling, ACLs or atomic replacement.

The existing remote workflow is `cargo xtask windows sync`, `build`, `test`
(or `all`), then manifest-based `windows deploy` when authorised. Required
environment values are `VIRTIO_MEM_WINDOWS_SSH`, `VIRTIO_MEM_WINDOWS_DIR`,
`VIRTIO_MEM_WINDOWS_ARTIFACTS`, `VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS` and
`VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS`. Use `cargo xtask help` for
optional pinned SSH settings and complete command options. `gate all` includes
remote Windows work; it is not a local-only gate.

## Qualification runner

For an authorised trial, create a reviewed config from the example and run:

```bash
cargo xtask host-run --config FILE --max-run-seconds N --min-host-available-bytes BYTES
```

Replace placeholders with explicit bounds. Add `--sudo` only when the reviewed
access setup requires it. This builds the release daemon, runs it in the
foreground without installing a unit, logs under `.artifacts/host-run/`, and
requests termination on the deadline, low host memory or Ctrl-C. Inspect the
reported shutdown result and final device state; stopping the runner does not
restore allocation or prove that a pending resize has finished.

Keep qualification evidence under `.artifacts/` and summarise only evidence
that affects phase acceptance in the roadmap. The standalone `live-resize`
command restores the original allocation only after forward convergence and
without cancellation. Failure, timeout, cancellation or an uncertain command
reports final live state and issues no automatic restoration. Inspect fresh
`requested`/`current`, stop any controller, then use the reviewed `abandon-shrink`
path for a stable pending shrink or explicit operator recovery for other states.
A restoration failure also requires inspection; do not blindly repeat a delta.
Subprocess output is capped at 4 MiB per stream (64 MiB for elevated inventory),
with a shared deadline covering
inherited pipes and cleanup of the owned process group.
