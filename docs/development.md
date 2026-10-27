# Development

This describes the current implementation. Direction, phase status and
qualification evidence are in [the roadmap](roadmap.md); open work is in
[the backlog](backlog.md).

## Architecture

| Location | Responsibility and entry points |
| --- | --- |
| [Core](../crates/virtio-mem-core/src/lib.rs) | Shared wire contracts, device/XML validation, the guest memory policy and pure pool allocation. |
| [Host](../host/src/main.rs) | Linux adapters and the controller. `runtime.rs` owns request execution and command lifecycle; `checkpoint.rs` owns durable control state; `guest_channel.rs` moves guest publications and host status; `resize_sink.rs` validates and executes device effects. |
| [Windows](../windows/src/main.rs) | Native sampling, the guest request policy (`requests.rs`), atomic publication, SCM lifecycle and the workload utility. Console and SCM startup share the worker constructor in `runtime.rs`. |
| [xtask](../tools/xtask/src/main.rs) | Local gates, remote Windows builds, deployment, diagnostics and qualification. These commands can have operational effects. |

Windows decides when to add or remove memory; Linux executes. Each poll:

1. Windows samples memory, reads the host status file, runs the
   [guest policy](../crates/virtio-mem-core/src/guest_policy.rs) and atomically
   publishes its telemetry with at most one unresolved request.
2. The host reads that publication (QGA, or a local file for testing), admits
   the request, grants what is free and resizes the device.
3. The host writes device status, with the correlated result once known, to
   `<telemetry_path>.host-status`. Windows resolves its request from it.

## Memory pressure and RAM release

The native worker takes one `GetPerformanceInfo` snapshot. Physical total,
physical available, commit total/limit/peak, system cache and kernel pools all
come from that call, with checked page-to-byte conversion. Available includes
standby/free/zero pages: cache is not subtracted again. Memory-load percentage
is derived from those physical counters and is diagnostic only. No WMI, PDH,
pagefile resizing, standby-list purging or working-set trimming is used.

The worker also collects paired low/high memory-resource notifications. The
native state reaches `TargetSample` and the guest policy; failed/unavailable
observations are `None`, not a fabricated Neutral or High. The worker waits on
the Low object and a manual-reset cancellation event using
`WaitForMultipleObjects`, with the normal poll interval as a timeout. While Low
or unknown is observed it uses the cancellable timer instead, preventing a
level-triggered Low object from spinning. A subsequent Neutral/High sample
re-arms the Low wait. If creating the stop event fails, bounded 50 ms waits
preserve cancellation. Failed notification creation is retried at a bounded
poll; query failures remain visible and can recover on the next observation.

Three reasons can request growth: native Low, a physical-reserve deficit, or a
commit-reserve deficit. The last two are proactive buffers, not claims that
Windows itself reports Low. Let C be current device bytes, A physical available,
H = commit limit - commit total, Rp the physical reserve and Rc the commit
reserve. The numeric target is max(0, C + Rp - A, C + Rc - H, configured minimum),
rounded up to blocks and capped at the usable device ceiling. Native Low also
requests up to one grow step even if those buffers look sufficient. Commit
headroom is independent of resident usage; more RAM is not assumed to repair a
pagefile or application problem without observing the next sample.

Reclaim uses exactly those physical/commit observations, plus native High:

- Every observation throughout the history window must report High. Low,
  Neutral, failed/unavailable native state, a failed numeric sample, missing or
  stale host geometry, pending device convergence, or a changed inferred fixed
  RAM base restarts reclaim qualification.
- Targets retain the highest numeric demand over the window. Downward
  hysteresis must be met; each removal is bounded by the configured shrink
  step and cannot pass the normal desired target or the safe-floor target
  calculated using its separate reserves. Both physical and commit buffers
  therefore constrain release. Host pool memory is credited only when actually
  returned, never merely requested.
- History uses guest monotonic time, checks every real sample gap and
  conservatively coalesces nearby samples by their maximum targets. Its 4096
  entry bound cannot shorten the qualification window under rapid polling.
  Wall-clock changes still affect host-status freshness and request issuance,
  but cannot create fictitious reclaim history.
- An incomplete/refused removal pauses further removals for a full window.
  A restart starts with cold history. An unresolved durable request is never
  replaced, including by a Low observation: serial ownership and bounded
  removal steps prevent overlapping commands; host convergence/settlement
  resolves the existing request before the guest can issue growth.

A failed numeric API sample publishes nothing, clears reclaim evidence and is
retried on the next bounded poll rather than terminating the worker. Other
errors (configuration, durable state, publication and clock contracts) remain
fatal. Requests log native state, available bytes and commit headroom.

References: [PERFORMANCE_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-performance_information),
[memory-resource notifications](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-creatememoryresourcenotification),
[waitable memory notifications](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject).
Notification-assisted decisions and native waits are unit/integration tested;
this is not qualification of real virtio-mem unplug or guest workload latency.

## Request contract

Owners: [request contract](../crates/virtio-mem-core/src/memory_request.rs),
[guest policy](../crates/virtio-mem-core/src/guest_policy.rs),
[runtime](../host/src/runtime.rs) and [checkpoint](../host/src/checkpoint.rs).

- All quantities are device bytes. Whole-guest RAM includes fixed RAM; Windows
  derives the fixed base per sample as `physical_total - current`.
- A request names its VM, service, session and sequence, an `add` or `remove`
  byte count, and an issue/expiry interval of at most 60 seconds. Reading a
  publication or status is never an acknowledgement.
- **Windows** keeps one unresolved request in `<telemetry_path>.request-state`,
  written durably before the request is first published and retransmitted
  unchanged (including across service restarts and new sessions) until a
  terminal result for that exact request arrives. It plans a new delta only from
  status no further than `host_status_max_age_millis` from its clock. Demand
  rises immediately; reclaim waits for gap-free history and hysteresis and
  never passes the safe floor. After a partial or refused removal, removals
  pause for one history window. Estimator history is process-local, so a
  restarted service waits a full window before reclaiming.
- **Linux** admits a request only if it is fresh, from the configured guest,
  advances the issue/sequence watermark and no earlier request is unresolved.
  A retransmission of an admitted request is recognised before expiry checks
  and never regranted. Expiry and identity are rechecked at the effect
  boundary, after blocking preparation.
- Grants: removals take `min(bytes, current)`. Additions take the least of the
  request, the device ceiling (size minus 1 GiB, block aligned), the member's
  pool ceiling (free bytes, larger `priority` first, configuration order on
  ties) and host `MemAvailable` minus pending growth and `host_min_headroom_bytes`.
  A zero addition is `pool_empty`. Nothing unfulfilled is queued.
- Admission and command intent are saved together before the command. On
  convergence the result reports the actual movement (`completed` or
  `partial`). A refusal before any command (latched, observe-only, expired,
  replayed, misaligned, or the resize sink's final checks) is published as
  `rejected` and does not latch; only an earlier admission journals it.
- A shrink that stops before its target is settled once at its current
  allocation and reported `partial`. Growth or settlement past
  `convergence_timeout_seconds`, an unknown command outcome, a recorded command
  that never applied, or conflicting live state latch the VM and report
  `uncertain`, which Windows never treats as terminal. The original start time
  survives restart; a backwards clock fails closed.
- Unowned divergence is observed, never replayed. Unreadable live state keeps
  the last pool charge, or the whole pool before a first read, and latches if a
  command is owned. Members fail independently; the daemon stops when all have.
- `clear-latch` requires converged live state, stamps the current attestation
  and gives any unresolved request a terminal result from observed movement.

## Configuration and deployment

[The host TOML example](../host/systemd/virtio-mem-host.toml.example) owns the
key inventory; replace every `REQUIRED_*` marker using reviewed deployment
values. Unknown keys are rejected, including the retired demand-policy keys
(`min_memory_bytes`, steps, reserves, reclaim history and hysteresis), which
now live in the Windows configuration. Global timing/reserve settings apply to
all `[[vm]]` tables; `pool_bytes` is required for multiple VMs. The example
starts with `observe_only = true`: requests are evaluated, logged with their
would-be grant and refused before any command.

One deployment instance is one controller: unit
`virtio-mem-host@INSTANCE.service` runs `/etc/virtio-mem-host/INSTANCE.toml`,
which may list several VMs. Each VM names its own checkpoint (distinct paths are
enforced) and an attestation file directly in `/etc/virtio-mem-host/`.
`xtask host-deploy INSTANCE --config FILE --attestation VM=FILE ...` requires one
attestation per configured VM and installs each at its configured path.
`status`, `clear-latch` and `xtask controller-status` take `--vm NAME`, required
when the config lists several VMs; `xtask qualification` checks that the running
controller's config contains its VM.

The [systemd unit](../host/systemd/virtio-mem-host@.service) runs as
`virtio-mem-host` against `qemu:///system` with `Restart=no`. Non-interactive
libvirt management access is provisioned once; the daemon neither elevates nor
stores passwords. The [polkit rule](../host/systemd/50-virtio-mem-host.rules)
grants connection-wide management to that account. Checkpoint paths must be
absolute without `..`; symlink/hard-link checkpoint files are rejected. The
daemon and operator mutations lock the libvirt domain UUID, and checkpoint
writers hold a sidecar file lock. Do not unlink lock files. Stop the controller
before `resize`, `abandon-shrink` or `clear-latch`.

Checkpoints are version 2 (journal, intent, latch). An estimator-era version 1
file without a latch or command is superseded on the next save; one recording
either is refused: stop the controller, review live state, then remove it.
Controller status is version 5 and includes the admitted request, its result,
`observe_only` and a read-only view of the guest input. Use matching host,
xtask and Windows builds.

Windows loads `C:\ProgramData\VirtioMemService\config.json` (schema 5,
[defined in core](../crates/virtio-mem-core/src/windows_service_config.rs)),
which now carries the guest `policy`: target reserves, history, hysteresis,
minimum, steps, status age and request lifetime. Steps must be multiples of the
device block size. VM/service identity and the telemetry path must match the
host. The publication path also names its backups, handoff files, the host
status and the request state as `<path>.<suffix>`; the configuration may be
none of them. Configuration is bounded to 64 KiB; paths use absolute drive
syntax and ambiguous Win32 forms are rejected. Installation keeps the
configuration administrator-owned and read-only to LocalService and applies
explicit modify-without-delete ACLs to every existing publication file.

`help` documents recovery when configuration is missing or damaged: in an
administrator terminal, `sc.exe stop SERVICE_NAME` and, when removing the
service, `sc.exe delete SERVICE_NAME`. A worker panic is reported as failure;
a worker exceeding the shutdown bound terminates the process for SCM recovery.
A policy or request-state failure stops the worker. Publication retries
sharing/lock errors within the shutdown bound. A missing, mid-write or
momentarily shared host status leaves Windows without fresh geometry; other
status read errors, such as denied access, stop the worker.

Migration from the environment-variable deployment requires copying retained
values from `<instance>.conf` to `<instance>.toml`; `xtask host-deploy` backs up
the old `.conf`. Preserve non-resizing deployments with `observe_only = true`.

## Local validation

Use stable Rust with rustfmt and Clippy. The local gate is implemented in
[xtask](../tools/xtask/src/local.rs):

```bash
cargo xtask gate local
```

It checks formatting, builds release binaries, tests the three non-Windows
packages, runs Clippy on all targets with warnings denied, and checks
whitespace. The guest policy lives in core so these tests cover it.
`make local-gate` and the VS Code tasks delegate to xtask.

If xtask cannot run, the equivalent direct commands are:

```bash
cargo fmt --all -- --check
cargo build --workspace --exclude virtio-mem-service --all-features --release --locked
cargo test --workspace --exclude virtio-mem-service --all-features --locked
cargo clippy --workspace --exclude virtio-mem-service --all-targets --all-features --locked -- -D warnings
git diff --check
```

For Windows, type-check with
`cargo clippy -p virtio-mem-service --target x86_64-pc-windows-gnu --all-targets -- -D warnings`
and build with MinGW available, or use the selected MSVC target. The Windows
crate does not build for Linux, and type-checking does not validate native SCM,
Win32 sampling, ACLs, file sharing with QGA or atomic replacement.

For an AI-led build/test run, use the
[Windows build/test task prompt](../.github/prompts/rust-windows-build-test.prompt.md).
It covers SSH, native gates, QGA/libvirt diagnostics and authorised live testing.

To build on the established `win11_gpu` guest from Linux, run:

```bash
make win11-build
```

This checks the native toolchain, syncs the current working tree (including
uncommitted source), builds the release binaries and downloads the SHA-256
verified service and workload executables to `.artifacts/windows/win11_gpu/`.
`make win11-gate` also runs native tests and Clippy. VS Code offers matching
**Windows: build service on win11_gpu** and **Windows: native gate on win11_gpu**
tasks. These commands do not install or restart the service.

### Build identity

Every remote build is traceable from source to the installed service:

- `sync` stamps the workspace (`virtio-mem-build.json`) with the commit, whether
  the working tree differs from it, and a digest of every synchronised file.
  The build ID is the 12-character commit, or `<commit>-dirty.<digest>` when
  uncommitted changes were synchronised.
- `build` compiles that ID into the service. `virtio-mem-service version`
  prints it, and the Event Log start message (id 1000) includes it. Builds
  outside xtask report `unversioned`.
- `fetch` refuses a candidate that does not report the synchronised ID, then
  writes `build.json` beside the executables: ID, commit, source digest and
  both SHA-256 values. This is the candidate record.
- `windows deploy` installs only a candidate whose hash matches `build.json`,
  records the build ID in its evidence and fails if the installed binary
  reports another ID.
- `make win11-status` (`windows build-status SERVICE --output PATH`) compares
  the installed binary with `build.json` by SHA-256 and reports
  `not_installed`, `current`, `update_available` or `no_candidate`. Binaries
  that predate `version` are reported as unknown builds.
- `windows uninstall SERVICE --output PATH [--apply]` backs up the binary,
  configuration and service registry key under
  `C:\ProgramData\VirtioMemService\deployment-backups\`, then removes the
  registration, binary and configuration. Telemetry and request-state files
  are retained and listed. Without `--apply` it only reports.

The presets use SSH alias `virtio-mem-windows`, workspace
`C:\Users\Dave\github\virtio_mem_windows_service`, a 10-second connection
timeout and a 1,200-second per-operation timeout. Configure the alias in
`~/.ssh/config` with the guest's current address, user and identity, and retain
its verified host key. All `VIRTIO_MEM_WINDOWS_*` settings can be overridden
through the environment or Make command line. An unreachable endpoint fails
the check before source sync; the preset does not start the VM or configure SSH.

The configurable remote workflow is `cargo xtask windows sync`, `build`, `test` (or `all`),
then manifest-based `windows deploy` when authorised. Deployment manifests are
schema 2 and include the guest `policy`. Required environment values are
`VIRTIO_MEM_WINDOWS_SSH`, `VIRTIO_MEM_WINDOWS_DIR`,
`VIRTIO_MEM_WINDOWS_ARTIFACTS`, `VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS` and
`VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS`. Use `cargo xtask help` for
optional settings and command options. `gate all` includes remote Windows work.

## Qualification runner

For an authorised trial, create a reviewed config from the example and run:

```bash
cargo xtask host-run --config FILE --max-run-seconds N --min-host-available-bytes BYTES
```

This builds the release daemon, runs it in the foreground without installing a
unit, logs under `.artifacts/host-run/`, and requests termination on the
deadline, low host memory or Ctrl-C. Add `--sudo` only when the reviewed access
setup requires it. Decisions are logged as `event=controller_decision` with
`action=grant` (and `grant_bytes`) or `action=refuse`. Stopping the runner does
not restore allocation or finish a pending resize; inspect the final state.

`xtask preflight` (controller held inactive) restarts the Windows service and
checks that publication rolls to a valid successor session and that any
unresolved request keeps its identity across the restart.

Keep qualification evidence under `.artifacts/` and summarise only evidence
that affects phase acceptance in the roadmap. The standalone `live-resize`
command restores the original allocation only after forward convergence and
without cancellation; otherwise it reports final live state and issues no
restoration. Inspect fresh `requested`/`current`, stop any controller, then use
`abandon-shrink` for a stable pending shrink or explicit operator recovery.
Subprocess output is capped at 4 MiB per stream (64 MiB for elevated
inventory), with a shared deadline covering inherited pipes and cleanup of the
owned process group.
