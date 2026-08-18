# Testing Strategy

## Local Testing

All testing is performed locally. No CI pipeline is currently configured.

### Privilege and password policy

The normal Rust validation path does not require root and should be run as the
regular development user:

- `cargo fmt --all -- --check`
- `cargo build --workspace --release`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `bash -n scripts/*.sh`

Do not wrap these commands in `sudo`; doing so can create root-owned build
artifacts and hides permission problems rather than fixing them.

Only live host operations such as installing a systemd unit under `/etc`,
writing `/usr/local/libexec`, managing a system service, or changing libvirt
authorization normally require administrative setup. Avoid repeated password
prompts by having the host administrator perform that setup once, using an
explicit non-login `virtio-mem-host` service account with only the required
libvirt access. The account should run the controller directly; it should not
be granted broad passwordless `sudo` access and the controller should not run
as root.

When several approved read-only host checks require elevated access, batch them
into one process and authenticate once. Approve and run one complete
read-only report invocation rather than prefixing each `virsh` command with
`sudo`. Commands inside that report must not invoke nested `sudo`. Do not
depend on the sudo timestamp cache to avoid prompts; the single outer `sudo`
invocation is the batching guarantee.

After that one-time setup, run read-only probes and the controller under the
approved account or authorization context. If a particular test genuinely
needs root, ask for approval first with the complete command, protected target,
expected mutation, and rollback behavior. Once approved, run the entire test
script once under `sudo` rather than adding `sudo` to individual subcommands.
Never automate or collect the password; the operator types it directly into
the terminal. Do not use `sudo -S`, modify sudoers, or weaken host permissions
just to make a test pass.

Any live resize, VM lifecycle operation, service installation/removal, or edit
to a server-side file remains an explicit operator-approved action separate
from the unprivileged test suite.

See [`dependencies.md`](dependencies.md) for the complete toolchain and host/
guest prerequisite matrix.

### Rust Service Testing

#### VS Code workflow

The repository includes `.vscode/tasks.json` and `.vscode/launch.json` so the
normal validation path can be run without an administrator terminal:

- **Rust: full local gate** — format check, workspace tests, Clippy, and a
  release build in sequence.
- **Rust: test workspace** — focused `cargo test --workspace --all-features`.
- **Rust: clippy workspace** — warnings-as-errors linting.
- **Rust: format check** — repository-wide rustfmt verification.
- **Rust: debug interactive service** — launches the non-SCM `run` mode through
  CodeLLDB; stop it with the debugger stop control or cancellation path.

The SCM tasks are deliberately separate and marked **(elevated)**. Open VS
Code itself as Administrator before using them. They invoke `sc.exe` explicitly;
do not type `sc` in PowerShell because `sc` is an alias for `Set-Content` and
will create files named `start`, `query`, or `stop` instead of querying SCM.
Use **SCM: query service** after start and stop to observe transitions. Never
combine these tasks with live resize or VM lifecycle operations.

```bash
cd windows
cargo build --release
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
```

The Rust service tests should cover QGA response parsing, threshold boundaries, minimum/maximum safe ranges, and invalid configuration cases. They do not require a live VM.

Service configuration is loaded from the versioned JSON file at
`C:\ProgramData\VirtioMemService\config.json`. A missing file uses validated
defaults; malformed files, unsupported schema versions, invalid durations, and
unsafe values fail startup. Local tests cover round-trip persistence,
missing-file defaults, schema rejection, and empty-path validation. Production
installation must provision the directory and least-privilege ACLs before
writing configuration or demand reports.
Schema version 2 includes `qga_operation_timeout_millis`, defaulting to 5000
ms. The timeout bounds the complete named-pipe request (connect, write, flush,
and response read); a timed-out request is returned as an explicit transport
error and no resize is attempted for that cycle.

### RHEL host controller testing

#### Read-only memory decision preview

Use `scripts/preview-memory-decision.sh` to see whether the configured policy
would grow, shrink, wait, or leave the guest unchanged. The script reads the
explicit VM's state, virtio-mem XML, and QGA memory statistics, then mirrors the
shared policy thresholds and block alignment checks. It never invokes
`update-memory-device` and is safe to use before enabling the systemd unit.

The policy values must be supplied as environment variables matching the host
controller configuration. The script returns status `0` for `NO CHANGE`, `10`
when a resize **would** be requested, and `20` when the decision is blocked or
validation fails. A status of `10` is only a preview result; no memory change
has occurred.

Example with an already-approved, non-secret instance configuration loaded in
the current shell:

```bash
set -a
source /etc/virtio-mem-host/INSTANCE.conf
set +a
bash scripts/preview-memory-decision.sh "$VIRTIO_MEM_VM_NAME" "$VIRTIO_MEM_ALIAS"
```

The current `win11_gpu` guest is expected to return `BLOCKED` until its QEMU
Guest Agent provides `guest-get-memory-stats`; this confirms that the preview
refuses to guess when the required observation is missing.

#### Reversible live-resize test

Use `scripts/live-resize-test.sh` for an operator-approved, reversible test of
one virtio-mem target. It validates the live alias, size, block alignment, and
convergence state before doing anything. Without `--apply`, it is a dry run.
With `--apply`, it issues one live request, records timestamped samples of
`requested`, `current`, domain state, and optional QGA free/total memory, then
waits for convergence. It automatically requests the original size afterward
unless `--keep-target` is explicitly supplied, and exits with failure unless
that rollback also reaches `requested == current`. AI-run tests are capped at a
30-second forward timeout; `--timeout` accepts only 1–30 seconds. Rollback
retains its own 300-second default via `--rollback-timeout`.

The terminal output is intentionally summary-only so an AI review does not
receive hundreds of polling lines. Pass `--log PATH` to retain detailed CSV
samples for later operator review.

The harness has conservative safety gates: the default target cap is 8 GiB,
the host must retain at least 4 GiB of `MemAvailable` after the requested
increase, and a test may never target the full virtio-mem device. These are
additional safety limits, not a substitute for cgroups, host capacity planning,
or an operator review. Override the cap or reserve only for a separately
approved test with a documented reason.

The test also has a fixed 1 GiB retention floor: targets below 1 GiB are
rejected, and automatic rollback never requests less than 1 GiB. The test no
longer attempts a zero-memory rollback.

Example dry run:

```bash
bash scripts/live-resize-test.sh win11_gpu ua-virtiomem0 2097152
```

On hosts where the default `virsh` connection is `qemu:///session`, add
`--connect qemu:///system` so the test uses the system libvirt instance:

```bash
bash scripts/live-resize-test.sh win11_gpu ua-virtiomem0 2097152 --connect qemu:///system
```

Only after confirming the target and obtaining explicit operator approval for a
live mutation should the apply form be used:

```bash
sudo bash scripts/live-resize-test.sh win11_gpu ua-virtiomem0 2097152 --connect qemu:///system --apply --timeout 30 --log /tmp/win11_gpu-memory.csv
```

The `sudo` form is only valid after explicit operator approval for that exact
VM, alias, target, and reversible test. If sudo requests authentication, type
the password directly in the terminal; the agent must not receive it.

The earlier 20 GiB attempt demonstrated why the full-device guard is required:
the VM already has 8 GiB of base RAM and the host has approximately 30 GiB of
physical memory. Requesting the full 20 GiB virtio-mem device could approach
28 GiB of guest memory before QEMU and host overhead. The attempt was rejected
by `virsh` at the KiB boundary and the VM remained unchanged, but the test is
now blocked before any live command by the explicit full-device and host-headroom
checks.

The later approved 1 GiB test demonstrated a separate rollback hazard: the
forward request converged in about 5 seconds, but rollback to zero remained
pending beyond the 300-second rollback timeout. The live XML then reported
`requested=0` and `current=18432 KiB` while the VM remained running. Treat this
as a critical failure: do not issue another resize, reboot, or forced action
automatically. Capture the current XML and operator-approved diagnostics, then
resolve convergence before any new request.

The script accepts canonical byte targets but converts them to KiB for
`virsh --requested-size`, whose default unit is KiB. It rejects targets that
cannot be represented as an exact KiB value and never rounds silently.

The script never guesses a target, never runs two resize requests at once, and
does not retain a target unless `--keep-target` is explicitly added. A timeout
or interruption attempts to restore the original requested size. Review the
CSV and console samples for convergence latency and QGA observations. The
current guest's missing `guest-get-memory-stats` capability will leave the QGA
columns as unavailable, but XML `requested/current` convergence can still be
observed.

Run the full workspace gate before installing the controller:

```bash
cargo fmt --all -- --check
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Host tests are hermetic: cover environment configuration, restricted aliases,
exact `virsh` argument vectors, selected XML aliases, invalid state, command
failures, convergence suppression, and cancellation. They must not require a
VM or invoke a command shell.

Create and validate the `virtio-mem-host` non-login service account's libvirt
permissions before installing `host/systemd/virtio-mem-host@.service` and a
per-instance configuration file. Observe the unit with
`systemctl status virtio-mem-host@INSTANCE` and
`journalctl -u virtio-mem-host@INSTANCE`. Failed QGA, XML, command, and
convergence operations must exit non-zero; a restart rereads live state and
must never replay a prior resize request.

#### Memory-stat source configuration

The connected QGA (`109.1.0` on `win11_gpu`) does not implement
`guest-get-memory-stats`, so `VIRTIO_MEM_STATS_SOURCE` defaults to
`dommemstat`, which reads `virsh dommemstat <vm>` (virtio-balloon counters)
instead. Before enabling the systemd unit, confirm with a read-only
`virsh dommemstat win11_gpu` that the domain reports `actual` and `unused`
(and ideally `available`); if the balloon driver does not report these
fields, the controller will fail closed with an explicit `GuestStats` error
rather than guess. Set `VIRTIO_MEM_STATS_SOURCE=qga` only for a guest agent
known to implement `guest-get-memory-stats`.

`VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES` is a required configuration value: the
controller will not send a grow request unless the RHEL host's
`/proc/meminfo` `MemAvailable` covers the requested delta plus this reserve.
An insufficient-headroom check is not treated as a fatal error; the
controller logs and waits for the next poll interval rather than crashing the
systemd unit.

#### Testing through the installed host service, not the standalone script

The standalone `scripts/live-resize-test.sh` script is a pre-installation
safety probe and is not a substitute for exercising the actual
`virtio-mem-host` systemd service end to end. Before declaring the host
controller usable:

1. Build the release binary (`cargo build --workspace --release`) and install
   it, the templated systemd unit, and a per-instance configuration file
   under the approved `virtio-mem-host` account — an explicitly
   operator-approved, reversible action, not something to script implicitly.
2. Start the unit and drive one 1 GiB grow from the current converged state
   by adjusting the instance's configured thresholds (or a controlled QGA/
   `dommemstat` stimulus), then observe `systemctl status` and
   `journalctl -u virtio-mem-host@INSTANCE` for the resulting convergence.
3. Confirm the unit blocks further requests while `requested != current`, and
   that it retains at least 1 GiB rather than converging toward zero.
4. Do not attempt this while a prior live test has not converged; check
   current `requested`/`current` via read-only `dumpxml` first.

### Running the Service Locally (Non-Service Mode)

For debugging and testing without installing as a Windows service:

```bash
cd windows
cargo build --release
./target/release/virtio-mem-service run
```

This starts the service worker directly (no admin privileges required, no service manager involvement). The service will:

- Load and validate the default configuration
- Initialize the polling worker
- Begin the main polling loop
- Exit cleanly on `Ctrl+C` or when the worker completes

For deterministic local service testing, the Windows runtime also includes a
built-in in-process harness that exposes a valid memory snapshot and a capture
sink without requiring a live QEMU Guest Agent pipe. This keeps the lifecycle
and policy tests hermetic while the actual guest-channel validation remains a
separate live-KVM gate.

**This mode is ideal for:**

- Local debugging and troubleshooting
- Testing QGA connectivity and responses
- Verifying cancellation and graceful shutdown
- Integration testing without service installation

### Service Installation and Real-Mode Testing

Once the service worker is tested locally and working:

```bash
cd windows
cargo build --release

# Install as Windows service (requires admin privileges)
./target/release/virtio-mem-service install

# Start through SCM
./target/release/virtio-mem-service start

# Observe the service in Services.msc or use:
Get-Service VirtioMemService | Select-Object -Property Status, StartType

# Stop the service
./target/release/virtio-mem-service stop

# Remove the registration after it is stopped
./target/release/virtio-mem-service remove

# Or from Services.msc directly
```

**Service mode validates:**

- SCM integration and lifecycle reporting
- Start/stop behavior through Windows service manager
- Event-log visibility
- Graceful shutdown within the configured timeout
- Non-zero exit code for unexpected worker failures

The executable first attempts the SCM dispatcher when invoked as `run` (the
default command with no arguments). If it is not launched by SCM, it falls
back to the interactive worker host, which is useful for local lifecycle
testing. The current worker is a stoppable QGA acquisition harness: it reads
and validates configured QGA memory statistics during initialization and each
poll. It does not infer virtio-mem `current` allocation or invoke a resize
sink, so successful QGA acquisition must not be treated as resize readiness.

For SCM validation under the configured `LocalService` account, deploy the
binary to `C:\Program Files\VirtioMemService` and grant that account
read/execute access. Use `sc.exe` explicitly from an elevated VS Code terminal;
PowerShell's `sc` alias is `Set-Content`. This deployment reached `RUNNING`,
stopped cleanly, and removed successfully during local validation. Event-log,
recovery, and QGA access evidence remain separate checks.

Service installation also registers the configured description and bounded
failure actions: restart after 5 seconds, 30 seconds, and 60 seconds, with a
24-hour reset period. Recovery actions are enabled for non-crash failures so
unexpected non-zero worker exits can be recovered; intentional stop remains a
successful zero-exit lifecycle. Live recovery and Event Log evidence still
require an installed-service observation.

When using the workspace-level VS Code release task, use
`target\release\virtio-mem-service.exe`. A crate-local build from `windows`
uses `windows\target\release\virtio-mem-service.exe`; do not mix these paths,
because the latter may be an older binary from a prior crate-local build.

### Windows service lifecycle testing

The SCM adapter and portable host must be tested separately from live QEMU
Guest Agent access. At minimum, verify:

1. startup reports pending before initialization and running only after the
  worker is ready;
2. a stop request cancels scheduling, prevents new polls, and reaches stopped;
3. system shutdown follows the same graceful cancellation path;
4. repeated stop requests are harmless and do not deadlock;
5. an unexpected worker error reaches failed and produces a non-zero process
  result for SCM recovery; and
6. a slow stop reports stop-pending and completes within the configured
  shutdown deadline.

The portable Rust tests additionally verify startup failure before `Running`,
atomic stop signaling, cancellation wake-up, configuration validation, and
that a stopped loop performs no new poll.

QGA transport tests verify that a zero operation timeout is rejected before a
pipe is opened, the default timeout is five seconds, and Windows timeout values
are clamped to the valid millisecond range. On Windows, an overlapped I/O
timeout calls `CancelIoEx` and closes the request handles before returning.

The `VirtioMemState` contract tests additionally verify the canonical byte
unit, minimum and power-of-two block size, requested/current/target alignment,
device-size bounds, and rejection of zero values before a host resize sink is
allowed to issue a request.

Polling integration tests also verify that an invalid virtio-mem snapshot is
rejected before QGA polling and that a proposed target is validated before the
resize sink is called.

XML contract tests cover alias extraction, mixed explicit units, unsupported
units, malformed/incomplete state, wrong device model, conversion overflow,
alignment, and device-size bounds. The parser is intentionally tested with
captured XML strings; live `virsh` discovery remains host-side validation work.

`XmlMemoryStateProvider` tests verify that source failures remain explicit and
that valid XML snapshots are converted into the polling state provider without
performing external commands.

Installation validation must also confirm the registered service name,
description, executable path, account, startup mode, and recovery actions.
Verify the sequence **install → start → observe logs → stop → delete** on a
Windows test machine. Do not test recovery by terminating a production
service or by using unbounded restart loops.

### Real VM validation

#### First RHEL-server smoke-test evidence (2026-08-18)

The first read-only checks were run against the explicitly named `win11_gpu`
guest on the RHEL server. The prerequisite script passed, `virsh domstate`
reported `running`, and the connected channel was
`org.qemu.guest_agent.0`.

Successful QGA reads were `guest-info`, `guest-ping`, `guest-get-osinfo`, and
`guest-get-host-name`. The guest reported QGA version `109.1.0`, Windows 11
x64, and hostname `ICE101`.

The standard three-attempt probe is currently blocked at
`guest-get-memory-stats`: the agent returns `command ... has not been found`,
and `guest-info` does not advertise that command. This is a guest-agent
capability issue, not a transport failure. No `guest-exec`, reboot, resize, or
XML mutation was used during this check.

The compatible `virsh dumpxml win11_gpu` inspection found virtio-mem alias
`ua-virtiomem0`, size `20971520 KiB` (20 GiB), block `2048 KiB` (2 MiB), and
`requested=0 KiB` / `current=0 KiB`. The direct `qemu-system-x86_64 --version`
check was unavailable because that binary is not in the current PATH, although
`virsh version` reported libvirt `11.10.0`, QEMU API `11.10.0`, and hypervisor
`10.1.0`.

Do not enable the host controller or attempt a resize until a supported,
validated memory-stat source is available and the V1 gate is complete.

On the RHEL host, validate the Windows guest agent and live device before enabling automatic updates:

1. Confirm `guest-info` and `guest-get-memory-stats` succeed three times.
2. Capture the virtio-mem alias, block size, `requested`, and `current` values from live XML.
3. Perform one reversible aligned live resize manually.
4. Confirm `current` converges before testing another request.
5. Test guest-agent interruption, guest reboot, failed update, and service restart.

For the RHEL controller, complete this additional gate before enabling an
automatic systemd instance:

1. Run the read-only prerequisite and QGA probes as the intended service
  account for the configured VM.
2. Verify the live alias, size, block, requested/current state, and configured
  byte limits.
3. Verify `dynamic-memslots=on` together with `unplugged-inaccessible=on`
  where supported, and rule out documented incompatible device/workload
  classes.
4. Confirm one operator-approved, reversible, block-aligned resize converges
  before another request.
5. Exercise QGA loss, XML/resize command failure, non-convergence, restart,
  and `SIGTERM`; confirm no overlapping or replayed resize occurs.

### Live virtio-mem XML validation

The libvirt/QEMU documentation adds a few required checks before any automated resize logic is considered safe:

```bash
# Inspect the running guest's memory devices and look for requested/current values.
# `dumpxml` has no `--live` option; its default is the live definition for a
# running domain. Use `--inactive` only when intentionally inspecting config.
virsh dumpxml "$VM_NAME" | grep -A20 -B5 "virtio-mem"

# Or query the live XML directly when the alias is known
virsh domxml-to-native qemu-xml "$VM_NAME" | grep -A20 -B5 "virtio-mem"
```

For a live update, use the alias explicitly and keep the request aligned to the block size:

```bash
virsh update-memory-device "$VM_NAME" \
  --alias "$VIRTIO_MEM_ALIAS" \
  --requested-size "$TARGET_BYTES" \
  --live
```

Expected behavior from the docs:

- The live XML may show `requested != current` while QEMU and the guest are converging.
- The service must wait for convergence before issuing the next resize request.
- A request that is not an integer multiple of the block size is invalid.
- The host should treat the live XML as authoritative, not just the last command response.

This is the required safety gate for live validation: if `requested` and `current` are still diverged, the controller must not keep sending resize changes.

The repository also provides an explicit host helper:

```bash
# Read-only: capture the selected live virtio-mem XML
bash scripts/virtio-mem-host.sh snapshot "$VM_NAME" "$VIRTIO_MEM_ALIAS" > live-memory.xml

# Opt-in live action: validates convergence, size, block, and alignment first
bash scripts/virtio-mem-host.sh resize "$VM_NAME" "$VIRTIO_MEM_ALIAS" "$TARGET_BYTES"
```

The helper requires `virsh` and `xmllint`, accepts only a constrained alias,
requires exactly one matching virtio-mem device, and never retries or loops.
The `resize` mode is the only mode that issues `virsh update-memory-device`.

### Bash validation helpers

- Run focused shell validation scripts locally before use on a target host.
- Check for required environment variables and host tooling early.
- Prefer explicit error handling and exit codes over silent fallback behavior.

## Validation Checklist

Before committing:

- [ ] All tests pass locally: `cd windows && cargo test`
- [ ] Code is formatted and linted: `cargo fmt --all` and `cargo clippy`
- [ ] Service boundaries are respected
- [ ] Documentation (including `docs/testing.md`) is updated with new procedures
- [ ] No credentials or secrets committed
- [ ] Local non-service (`run`) mode has been tested if code changes affect worker logic
- [ ] Service installation (`install` command) has been tested if SCM code changes

## Phase 2 demand-agent validation

Native Windows telemetry is additive to the current QGA/dommemstat path. The
implemented `windows/src/demand.rs` collector and calculator are tested without
a live VM first:

- validate `GlobalMemoryStatusEx` and `GetPerformanceInfo` results through
  canonical `MemoryTelemetrySnapshot` fixtures;
- reject zero totals, zero commit limits, impossible counters, and overflow;
- verify physical and commit pressure ratios remain within `0.0..=1.0`;
- test the provisional `release`, `stable`, `want_more`, `pressure`, and
  `critical` boundaries;
- verify desired targets are clamped to configured minimum/maximum values;
- verify safe-floor recommendations never authorize a resize by themselves;
- verify all targets remain block-aligned and canonical byte based.
- verify the demand-agent publisher receives exactly one complete report after
  valid collection;
- verify invalid telemetry prevents publication and publisher failures remain
  explicit.

The JSON-lines publisher test reads the emitted file back and parses each line
as a complete version-1 `DemandReport`. The default path is under
`C:\ProgramData\VirtioMemService`; installation must provision the directory
and least-privilege ACLs before enabling durable service output. The main SCM
worker remains unconnected until a real current-allocation provider is
validated; tests must not substitute a configured minimum or QGA total for
that state.

The native collector calls `GlobalMemoryStatusEx` for physical memory and
`GetPerformanceInfo` for page-based commit/system counters. Page counters are
converted using checked multiplication by the reported page size. Windows API
failures and invalid snapshots return explicit errors. The report is version 1
and serializes canonical-byte values; it is advisory only and does not call a
resize sink.

The demand report must be read-only with respect to virtio-mem. A passing local
collector test does not prove that QEMU/libvirt or `viomem.sys` converges.

## Phase 3 driver and global-controller validation

### Driver-source validation boundary

When driver behavior changes or a driver status interface is proposed, validate
the upstream or forked `viomem` solution separately from the Rust workspace:

- build the intended Win10/Win11 architecture with the required Visual
  Studio/WDF environment;
- record whether the build is test-signed or production-signed;
- install only on a disposable test guest with a documented rollback;
- verify the device interface, virtio feature negotiation, block size, and
  plug/unplug behavior;
- test power transitions and driver/service restart behavior;
- prove any IOCTL contract with access-control, malformed-input, timeout, and
  version-compatibility tests before the Rust service consumes it.

The presence of `GUID_DEVINTERFACE_VIOMEM` is not sufficient evidence of a
user-mode IOCTL API. Do not add kernel-driver build or installation steps to
the normal Rust/Bash validation gate.

Before live multi-VM work, validate the state model with hermetic simulations:

- allocate and reclaim several VMs against a fixed host reserve;
- test independent growth and reclaim priorities;
- exercise `NORMAL`, `CAUTION`, `PRESSURE`, `CRITICAL`, and `EMERGENCY`
  transitions with hysteresis;
- ensure in-flight `requested != current` state cannot be counted as free pool;
- verify stale or missing demand reports fail closed;
- test bounded, block-aligned reclaim and stop-on-pressure behavior.

For explicitly approved live validation, capture the driver version and
features, virtio-mem block size, QEMU/libvirt `requested` and `current`, and
the Windows driver's `requested_size` and `plugged_size` when those values are
observable. Do not assume the two naming pairs are equivalent until the same
resize is observed across all layers. Do not add a direct driver IOCTL or
perform an unbounded shrink based on this design document.

## Known Blockers

- The Windows-native Rust 1.97.1 MSVC toolchain now passes the full local
  format, release build, test, and Clippy pipeline.
- Live QEMU Guest Agent and libvirt validation requires the RHEL host and
  Windows guest described in `docs/qemu-ga-setup.md`.
