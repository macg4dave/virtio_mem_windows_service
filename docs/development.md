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
  stale host geometry, or a changed inferred fixed RAM base restarts reclaim
  qualification. A device converging on a request contributes no sample but
  keeps the window while Windows still reports High, so successive bounded
  removal steps continue without another full window; a convergence longer
  than the maximum gap restarts it.
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
- A restart starts with cold history. An unresolved durable request is never
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
  never passes the safe floor. There is no removal backoff: the host holds a
  removal until Windows completes it, so a removal Windows cannot evacuate
  simply occupies the request slot rather than being re-asked. It ends short
  only when Windows reports Low, and a Low guest does not reclaim. Estimator
  history is process-local, so a restarted service waits a full window
  before reclaiming.
- **Linux** admits a request only if it is fresh, from the configured guest,
  advances the issue/sequence watermark and no earlier request is unresolved.
  A retransmission of an admitted request is recognised before expiry checks
  and never regranted. Expiry and identity are rechecked at the effect
  boundary, after blocking preparation.
- Wake-ups: `org.virtio-mem.wake.0` is an optional duplex virtio-serial
  doorbell. Windows sends a byte after publishing telemetry; the host sends a
  byte after publishing a changed device state or result. Bytes only trigger
  a fresh read of the durable protocol files and never authorise a resize.
  Libvirt memory-device-size-change events wake convergence observation.
  Missing channels and ended subscriptions fall back to periodic polling.
  Windows waits for native Low pressure, result notifications or the sampling
  deadline; after resolving a request it promptly samples again.
- Growth requests cover the entire aligned shortage up to the VM/device
  maximum. The 1 GiB policy increment is only the minimum response to native
  Low pressure; it no longer caps a request. Reclaim requires 1 second of
  continuous qualifying High observations, and retains safe floors.
- Grants: removals take `min(bytes, current)`. Additions take the least of the
  request, the device ceiling (full size, block aligned), the member's
  pool ceiling (free bytes, larger `priority` first, configuration order on
  ties) and host `MemAvailable` minus pending growth and `host_min_headroom_bytes`.
  A zero addition is `pool_empty`. Nothing unfulfilled is queued.
  There is no fixed per-device reserve. Deploy the host and Windows service
  together when upgrading from the old 1 GiB device-reserve rule: status
  geometry validation rejects a ceiling calculated under the other rule.
  Historical request results retain their original byte counts across device
  block-size changes. Their identity and outcome remain validated, but current
  block alignment applies to live geometry and new targets, not old results.
- Admission and command intent are saved together before the command. On
  convergence the result reports the actual movement (`completed` or
  `partial`). A refusal before any command (latched, observe-only, expired,
  replayed, misaligned, or the resize sink's final checks) is published as
  `rejected` and does not latch; only an earlier admission journals it.
- A shrink or growth holds its requested size until the guest reaches it,
  with no stall settle and no convergence timeout; the guest's single request
  slot stays occupied until then. The Windows viomem driver makes one attempt
  per device notification, so a held removal moves again only if Windows
  frees the memory in its window. Two exceptions settle at the current
  allocation as `partial`: a held shrink as soon as the guest publishes
  Windows' Low memory signal (so it can request growth), and a growth that has
  plugged nothing before the one-time widening once it has not moved for one
  poll, because QEMU rejects plugs outside the usable region and the driver
  never retries them.
- **Temporary viomem workaround** (`VIOMEM WORKAROUND` markers in
  `host/src/runtime.rs`; remove once a patched driver is deployed): viomem
  frees about one block (8 MiB on `win11_gpu`) per device notification and
  then stops, however large the gap. Each time a held shrink stops moving, the
  host bounces requested one block above the guest's target and straight back:
  two notifications, and the target never drops below what the guest asked
  for. (Lowering the target a block at a time instead only moved the gap
  down in step and released memory past the request.)
- Windows reclaim size matters. Each viomem attempt asks Windows to hot-remove
  whatever whole blocks it can inside one window starting at the lowest
  plugged block, `plugged - requested` bytes long, and never more. Small removal steps
  confine that window to the oldest, least removable memory: on `win11_gpu`
  64 MiB steps returned 2 MiB each, while one 1 GiB and one 2.2 GiB window
  returned 90 MiB and 1.17 GiB within seconds. Use a `shrink_step_bytes` that
  covers the expected surplus (3 GiB on `win11_gpu`), below the 4 GiB a
  single Windows MDL can describe; the desired target and safe floor still
  bound every removal.
- Released blocks leave holes, and viomem plugs only above the highest
  plugged block. QEMU accepts plugs only below its usable-region edge: the
  largest `requested` ever set plus 256 MiB (x86-64), reset only by a VM reset.
  When a grow settles with nothing plugged, the host widens that edge once:
  with the domain paused it requests the device ceiling and then the current
  allocation again, and resumes only after the restore succeeds (otherwise it
  latches and leaves the domain paused). Widening re-arms when the device
  reaches 0 after a reset.
- Release returns little until the window passes the unmovable low zone: on
  `win11_gpu`, after use, requests up to 1 GiB returned nothing, 2 GiB returned
  900 MiB and 4 GiB 2.83 GiB. The guest uses a 3 GiB `shrink_step_bytes`.
- Settlement past `convergence_timeout_seconds`, an unknown command outcome,
  a recorded command that never applied, or conflicting live state latch the VM and report
  `uncertain`, which Windows never treats as terminal. The original start time
  survives restart; a backwards clock fails closed.
- Unowned divergence is observed, never replayed. Unreadable live state keeps
  the last pool charge, or the whole pool before a first read, and latches if a
  command is owned. Members fail independently; the daemon stops when all have.
- `clear-latch` first settles a stuck shrink at the guest's current
  allocation (it refuses a device still growing), then stamps the current
  attestation and gives any unresolved request a terminal result from
  observed movement.

## Configuration and deployment

The reviewed `win11_gpu` configuration, with every RAM setting explained, is
in [deploy/config_files/](../deploy/config_files/README.md).

Administrator settings have three TOML formats, defined and validated in
[core settings](../crates/virtio-mem-core/src/settings.rs):

- `global-settings.toml`: pool, host headroom, observe-only and shared timing.
- `host_list.toml`: `[[vm]]` entries with `vm_name`, `alias`, `priority` and an
  optional host-local diagnostic `telemetry_path`.
- `<vm_name>_settings.toml`: total guest `min_ram`/`max_ram`, `install_path`,
  `service_account`, `vm_name` and optional Windows diagnostic `telemetry_path`.

Only the global keys have built-in defaults; each VM has independent required
settings. A missing global file uses defaults, but multiple VMs require an
explicit `pool_bytes`. VM names are validated lower-case ASCII identifiers;
filenames must match exactly. Unknown keys and legacy manifests are rejected
with migration guidance. Files are loaded relative to the host list, without
nested imports or VM-level overrides. The deployment README records defaults,
units, migration and adding a VM.

There is no configured fixed-RAM base. The guest derives device bounds from
`min_ram`, `max_ram` and each live sample; the host independently bounds grants.
Device size/block size remain live geometry. Sampling, steps, reserve/floor
pairs and reclaim hysteresis/history use shared built-in defaults;
polling also determines freshness, request expiry and sample-gap bounds.
Per-VM checkpoint and attestation paths are derived from the validated name.
The resize sink requires its compatibility source explicitly; fixed evidence
fixtures exist only in unit tests. Deployment helpers require current explicit
attestation inputs rather than selecting files from previous test runs.

Both diagnostic outputs are entirely optional: no path means no diagnostic
writes, and output errors cannot stop memory control. Required policy sampling
and request/reply traffic use a built-in channel path (historically named
`telemetry.json`), independently of optional telemetry. The publication is
replaced atomically every poll without a disk flush; only the request state is
written durably. Diagnostics cannot alias control files, config or binary.
The host rewrites `.host-status` over the guest agent only when its device
state or result changes, or after a third of the freshness bound.

One controller unit `virtio-mem-host@INSTANCE.service` runs
`/etc/virtio-mem-host/host_list.toml` with sibling settings files. Never run
multiple controllers for the same list/pool. `xtask host-deploy INSTANCE
--config FILE --attestation VM=FILE ...` requires one attestation per VM and
snapshots all configuration files before validation, hashing and installation.
Deployment evidence schema 4 hashes an index of every snapshot file; the index
and snapshot remain in `.artifacts/deploy/`. Installed settings and obsolete
instance configs are backed up; pending requests and latches are retained.
The attested domain XML hash ignores the virtio-mem allocation, the domain
`id` and `type='dynamic'` security labels, which change on every VM start.
The attested QEMU argv hash ignores `requested-size` and the tap `fd`/`vhostfd`
numbers, which are libvirtd's own descriptors and differ between two
generations of an unchanged domain (a mismatch latched `win11_gpu` at 12:21 on
27 October). Attestations made before 27 October 16:00 must be regenerated
(`xtask attestation VM ALIAS REVIEW --output FILE`).
`status`, `clear-latch` and `xtask controller-status` take `--vm NAME`, required
when the config lists several VMs; `xtask qualification` checks that the running
controller's config contains its VM.

The [systemd unit](../host/systemd/virtio-mem-host@.service) runs as
`virtio-mem-host` against `qemu:///system` with `Restart=no`. Non-interactive
libvirt management access is provisioned once; the daemon neither elevates nor
stores passwords. The [polkit rule](../host/systemd/50-virtio-mem-host.rules)
grants connection-wide management to that account. The daemon serves its per-poll
`dumpxml` and guest-agent commands over one long-lived libvirt connection
(`host/src/libvirt.rs`, loaded with `dlopen`): libvirt asks polkit to
authorize every new connection, and a `virsh` process per command at a 1 s
poll exhausted polkitd's file descriptors on 27 October, after which polkit
refused every check and the controller could not start. Resizes,
suspend/resume, version and QMP evidence still run `virsh`. Checkpoint paths must be
absolute without `..`; symlink/hard-link checkpoint files are rejected. The
daemon and operator mutations lock the libvirt domain UUID, and checkpoint
writers hold a sidecar file lock. Do not unlink lock files. Besides running
the daemon, the host binary has two operator commands: read-only `status`
and `clear-latch` (stop the controller first).

Checkpoints are version 2 (journal, intent, latch); any other version is
refused rather than discarded. Guest telemetry is schema version 4: memory
counters, Windows' native signal as one optional `native_pressure`
(`low`/`neutral`/`high`, absent when unreadable) and the pending request.
Controller status is version 5 and includes the admitted request, its result,
`observe_only` and a read-only view of the guest input. Use matching host,
xtask and Windows builds.

Windows loads generated `C:\ProgramData\VirtioMemService\config.json`
(schema 8, [defined in core](../crates/virtio-mem-core/src/windows_service_config.rs)).
It carries derived policy/timing/control details and explicit total-RAM limits;
edit the source TOML rather than this deployment output. The service validates
it before startup. Configuration is bounded to 64 KiB; Windows paths use
absolute drive syntax and ambiguous Win32 forms are rejected. Installation
keeps configuration administrator-owned and read-only to LocalService and
applies explicit ACLs to existing control files. Optional output directories
need write access for the selected service account. The existing automatic
ACL provisioner supports LocalService.

`help` documents recovery when configuration is missing or damaged: in an
administrator terminal, `sc.exe stop SERVICE_NAME` and, when removing the
service, `sc.exe delete SERVICE_NAME`. A worker panic is reported as failure;
a worker exceeding the shutdown bound terminates the process for SCM recovery.
A policy or request-state failure stops the worker. Publication retries
sharing/lock errors, and the access denial `MoveFileExW` reports while the
guest agent holds the file open for a host read, within a bounded retry
window; access still denied at the deadline stops the worker. A missing, mid-write or
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

The presets read the SSH alias, guest workspace, timeouts, manifest and
service name from [windows-build.mk](../deploy/config_files/windows-build.mk).
Configure the alias in
`~/.ssh/config` with the guest's current address, user and identity, and retain
its verified host key. All `VIRTIO_MEM_WINDOWS_*` settings can be overridden
through the environment or Make command line. An unreachable endpoint fails
the check before source sync; the preset does not start the VM or configure SSH.

The configurable remote workflow is `cargo xtask windows sync`, `build`, `test` (or `all`),
then TOML-based `windows deploy` when authorised. Deployment reads a validated
`<vm_name>_settings.toml` and sibling global settings; JSON and schema-2
administrator manifests have been removed. [The win11_gpu settings](../deploy/config_files/win11_gpu_settings.toml)
explains every setting. Required environment values are
`VIRTIO_MEM_WINDOWS_SSH`, `VIRTIO_MEM_WINDOWS_DIR`,
`VIRTIO_MEM_WINDOWS_ARTIFACTS`, `VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS` and
`VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS`. Use `cargo xtask help` for
optional settings and command options. `gate all` includes remote Windows work.

## Live RAM display in Windows 11

Run `tools/windows/Watch-Ram.ps1` in Windows PowerShell 5.1 or PowerShell 7:

```powershell
.\tools\windows\Watch-Ram.ps1
# Faster refresh, or a bounded machine-readable observation:
.\tools\windows\Watch-Ram.ps1 -IntervalMilliseconds 250
.\tools\windows\Watch-Ram.ps1 -Samples 5 -AsObject | ConvertTo-Json
```

No elevation is needed. Ctrl+C stops the monitor. It calls `GetPerformanceInfo`
on every tick. The RAM-only dashboard refreshes in place in a console of at least
80 columns by 30 rows: a blue console with title and status bars, double-line
panels and colour block bars (green, yellow from 70%, red from 90%). It shows
physical total, available/in-use RAM, commit headroom, usage bars, session
minimum/maximum total, minimum available, peak usage and cumulative
added/removed memory. A rolling chart plots total (`━`) and in-use (`█`) RAM
against an absolute GiB scale. The latest three total-RAM changes remain listed
with timestamps, green ▲/red ▼ markers and signed MiB changes. The console's
colours, encoding and cursor are restored on exit. Redirected and `-NoClear`
output prints the same layout in ASCII (`T`, `#`).

History defaults to 60 observations. Set `-HistorySamples 300` for a longer
window (approximately five minutes at the default interval); windows wider
than 60 samples are compressed into columns retaining peak sampled usage and
latest total per bin. This is sampled history, not detection of changes between
samples. Session extrema and cumulative change totals persist beyond the chart
window. Use `-NoClear` for scrolling snapshots; redirected output and terminals
too small for the dashboard use snapshots automatically. `-AsObject` retains
CSV/JSON support and adds the session metrics. GiB uses 1024-based units.
Available includes standby/free/zero pages. It does not resize RAM, clear caches or alter
services. Windows-visible total is distinct from hypervisor requested bytes;
if a requested hot-add has not reached Windows, this display cannot claim it
has. `-AsObject` includes exact byte values and timestamps for CSV/JSON capture.

A copy is staged at `C:\Users\Dave\Watch-Ram.ps1` on ice101 for convenience.
Native Windows PowerShell sampling, object output and plain dashboard rendering
were verified on 27 October 2026. A 65-sample injected API fixture verified
+1 GiB/-1 GiB events, session extrema, cumulative changes and history compression
without changing guest RAM. The colour dashboard's ASCII layout was rechecked
in Windows PowerShell 5.1 on the same day; its interactive colour repaint was
not visually qualified.

## Live host view on Linux

`cargo xtask watch` ([source](../tools/xtask/src/watch.rs)) shows the whole
loop from the host in one refreshing dashboard: host RAM against the
controller floor, the virtio-mem device (requested, plugged, converging),
Windows' own view read over the guest agent (in use, available, commit
headroom, memory signal, pending request, last host result), a device
history chart, and the controller's latest journal events, colour-coded with
repeats collapsed.

```bash
cargo xtask watch --config deploy/config_files/host_list.toml
cargo xtask watch --config deploy/config_files/host_list.toml --interval-seconds 1
cargo xtask watch --config deploy/config_files/host_list.toml --once   # one snapshot
```

The VM, device alias, telemetry path and host floor come from the host
config; `--vm` picks one `[[vm]]` when there are several, and the controller
unit is `virtio-mem-host@<VM>.service` unless `--instance` names another.

It is read-only and reuses the controller's own readers: `virsh --readonly
dumpxml` with the alias-scoped XML parser, guest-agent reads of the Windows
publication decoded as the shared telemetry type, and `journalctl` (events
and the last `request_completed` result). It never reads `.host-status`: a
guest-agent read holds that file open across several commands, and the
controller's write then failed with "being used by another process" (seen
on 27 October while an earlier version read it). Guest reads default to
every second; `--no-guest` skips them. Ctrl+C leaves the last view as
plain text plus the full, unclipped recent events on the normal screen for
copying; `--once` prints the same. `cargo xtask help` lists options for
another VM, alias, unit, telemetry path or host floor.

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
that affects phase acceptance in the roadmap. For manual recovery, inspect
fresh `requested`/`current`, stop any controller, then use `clear-latch`,
which settles a stuck shrink at the guest's current allocation.
Subprocess output is capped at 4 MiB per stream (64 MiB for elevated
inventory), with a shared deadline covering inherited pipes and cleanup of the
owned process group.
