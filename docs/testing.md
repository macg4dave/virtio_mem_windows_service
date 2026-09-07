# Testing Strategy

## Local Testing

All testing is performed locally. No CI pipeline is currently configured.

### Privilege and password policy

The normal Rust validation path does not require root and should be run as the
regular development user:

- `cargo fmt --all -- --check`
- `cargo build -p virtio-mem-core -p virtio-mem-host --all-features --release --locked`
- `cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked`
- `cargo clippy -p virtio-mem-core -p virtio-mem-host --all-targets --all-features --locked -- -D warnings`
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

AI agents must use the repository's
`.github/prompts/rhel-privileged-batch.prompt.md` workflow when a task needs
multiple privileged RHEL commands. The agent creates a complete, task-specific
script below the ignored `.vscode-artifacts/privileged-tasks/` directory for
operator review, then runs that script through one outer `sudo bash` command.
The operator enters the password directly once. The batch is scoped to that
approved task and must not become a general privileged command runner.

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

The repository includes `.vscode/tasks.json` so the normal non-mutating
validation path can be run from VS Code without an administrator terminal:

- **RHEL: full local gate** — format and Bash syntax checks plus native release
  build, tests, and Clippy for the shared core and RHEL host controller. The
  Windows-only crate is intentionally validated by the native Windows gate.
- **Windows: check remote toolchain** — verifies SSH, Rust MSVC, the linker,
  archive tooling, and checksum tooling on the Windows build guest.
- **Windows: synchronize source** — transfers the working tree while omitting
  Git metadata, build output, and local artifact staging.
- **Windows: native release build** — runs `cargo build` in the Windows guest.
- **Windows: native tests** — runs the Windows crate tests in the guest.
- **Windows: native format and clippy** — runs rustfmt and warnings-as-errors
  Clippy in the guest.
- **Windows: fetch verified artifact** — downloads the executable and compares
  its Windows-side SHA-256 with the RHEL-side checksum.
- **Windows: full native gate** — checks the endpoint, synchronizes once, then
  builds, tests, lints, and fetches the artifact from that working tree.
- **Build: all non-mutating gates** — runs the RHEL and Windows validation
  tasks and fetches the verified Windows artifact.

The VS Code tasks prompt for the `VIRTIO_MEM_WINDOWS_SSH` SSH config alias and
default it to `virtio-mem-windows`. When invoking the wrapper directly, export
that variable in the shell. Optionally set `VIRTIO_MEM_WINDOWS_DIR` and
`VIRTIO_MEM_WINDOWS_ARTIFACTS`; the defaults are documented in
`windows/README.md`. Prefer the aggregate task or the wrapper's `all` command
for a complete gate so source is synchronized only once. The remote wrapper
initializes the Visual Studio MSVC environment using `vswhere.exe`, so the SSH
account must be able to access the installed Build Tools.

The wrapper resolves the active Cargo toolchain with `rustup which cargo` and
invokes its real executables. This is required on `ice101.lan` because its
`.cargo\bin` rustup proxy symlinks fail through OpenSSH with Windows error 448
even though the underlying MSVC toolchain is healthy. The wrapper also removes
the carriage return from `certutil.exe` output before parsing the remote
SHA-256.

The equivalent terminal entry point is `make all-gates`. It requires
`VIRTIO_MEM_WINDOWS_SSH`; `make build`, `make test`, and `make lint` run only
the RHEL-compatible portion.

To collect TASK-011 milestone evidence, run the fingerprint-pinned helper from
the RHEL checkout. It checks the endpoint first, executes `make all-gates`
twice, and stores both logs and staged executable hashes under the ignored
`.vscode-artifacts/windows/milestone-TIMESTAMP/` directory:

```bash
bash scripts/complete-windows-build-milestone.sh \
  WINDOWS_SSH_ALIAS SHA256:EXPECTED_ED25519_HOST_FINGERPRINT \
  ~/.ssh/OPTIONAL_PRIVATE_KEY
```

Omit the private-key argument when the alias or `ssh-agent` already selects the
correct key. The helper uses a temporary known-hosts file, refuses a host-key
mismatch, and does not modify `~/.ssh/config` or `~/.ssh/known_hosts`. Success
requires both aggregate runs to exit zero and each fetched artifact to pass the
wrapper's Windows/RHEL SHA-256 comparison.

The Windows tasks are deliberately not deployment tasks. They never install,
start, stop, or remove the Windows service and never change RHEL systemd,
libvirt, QEMU, or guest memory. Keep service installation and live resize as
separate operator-approved procedures below.

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

This helper is now a legacy diagnostic, not a faithful preview of the deployed
default: it calls the non-upstream `guest-get-memory-stats` extension and does
not reuse the controller's `dommemstat` source/freshness logic. M9e/TASK-025
must replace it with a Rust decision command before decision-preview output is
used as policy evidence.

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

The current `win11_gpu` guest is expected to return `BLOCKED` because upstream
QGA does not provide `guest-get-memory-stats`. This validates only that the
legacy helper refuses to guess; upgrading upstream QGA will not unblock it.

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

For Windows shrink tests, convergence timeout is not proof that the installed
driver will retry. Source review found no obvious periodic retry timer. Do not
enable unattended shrink until M10b proves the selected bounded policy and the
controller has separate default-off automatic-shrink and re-notification
controls. The qualification profile samples every five seconds, re-notifies
only the immutable target after 30, 60, and 120 seconds without block progress,
allows at most three re-notifications, and never extends the initial 300-second
deadline. Progress may move the no-progress clock but may not replenish either
bound.

Before any repeat live resize, hermetic fake-clock tests must cover convergence,
zero progress, partial progress, exhaustion, stale/invalid/non-monotonic state,
external requested-size change, command ambiguity, cancellation, guest
transition, and restart. They must prove there is one in-flight operation, no
derived target, no replay, and no service-manager restart loop for a latched
stall. A separate dedicated adapter must enforce
`requested == immutable_target < current` for re-notification; the ordinary
resize adapter must retain its converged-state precondition.

Run the native RHEL gate before installing the controller:

```bash
bash scripts/build-rust.sh
```

Host tests are hermetic: cover environment configuration, restricted aliases,
exact `virsh` argument vectors, selected XML aliases, invalid state, command
failures, convergence suppression, and cancellation. They must not require a
VM or invoke a command shell.

Create and validate the `virtio-mem-host` non-login service account's libvirt
permissions before installing `host/systemd/virtio-mem-host@.service` and a
per-instance configuration file. The unit explicitly selects
`qemu:///system` and gives libvirt a writable per-service runtime cache via
`RuntimeDirectory`; do not rely on the service account's default session URI
or home directory. Observe the unit with
`systemctl status virtio-mem-host@INSTANCE` and
`journalctl -u virtio-mem-host@INSTANCE`. Failed QGA, XML, command, and
convergence operations must exit non-zero; a restart rereads live state and
must never replay a prior resize request.

#### Memory-stat source configuration

The connected QGA (`110.0.2` on `win11_gpu`) does not implement the custom
`guest-get-memory-stats` extension, so `VIRTIO_MEM_STATS_SOURCE` defaults to
`dommemstat`, which reads `virsh dommemstat <vm>` (virtio-balloon counters)
instead. Before enabling the systemd unit, confirm with a read-only
`virsh dommemstat win11_gpu` that the domain reports `actual` and `unused`
(and ideally `available`); if the balloon driver does not report these
fields, the controller will fail closed with an explicit `GuestStats` error
rather than guess. Set `VIRTIO_MEM_STATS_SOURCE=qga` only for an exact
custom/downstream agent validated to implement the extension; upstream QGA
version alone is never evidence.

Some Windows balloon reports may expose `available` above `actual`. The host
parser currently treats that optional counter as out of range and falls back
to `unused`. The upstream audit found this is incorrect: `actual` is balloon
state, not a whole-guest upper bound, so `available > actual` is not inherently
impossible. The parser also ignores `last-update`. M9e must add regression
tests for missing, stale, future, and non-advancing timestamps and for
`unused`/`available > actual` before the source is production-qualified.

`VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES` is a required configuration value: the
controller will not send a grow request unless the RHEL host's
`/proc/meminfo` `MemAvailable` covers the requested delta plus this reserve.
An insufficient-headroom check is not treated as a fatal error; the
controller logs and waits for the next poll interval rather than crashing the
systemd unit.

`VIRTIO_MEM_WORKLOAD_REVIEWED` is also required. Set it to `true` only after
the explicitly scoped VM has been reviewed for vDPA, VFIO-NVMe, RDMA migration,
`mlock`, encrypted/secure virtualization, active balloon resize, vhost-user
backend/version and slot capacity, VFIO mapping budget, backend sparse/reserve/
preallocation/share/core-dump/page-size/NUMA properties, topology, and deployed
versions. `false` preserves unknown workload evidence and blocks resize
preparation. Independently of that
operator statement, the controller refreshes `dynamic-memslots` and
`unplugged-inaccessible` from the selected QOM device through bounded QMP
requests before every prepared resize.

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

When the live device is fully unplugged (`requested=current=0`) and the
configured minimum is 1 GiB, the controller uses its below-minimum bootstrap
rule to request the aligned 1 GiB minimum once. Host-headroom, fresh XML,
fresh QMP compatibility, workload-review, and convergence gates still apply.
After convergence, ordinary policy resumes one block at a time and cannot
shrink below the configured minimum.

The installed-service procedure passed on `win11_gpu` on 2026-09-05. The
controller replaced a stale pre-zero-state binary, started under the dedicated
`virtio-mem-host` account, requested 1 GiB from a converged zero state, reached
`requested=current=1073741824`, and retained that minimum after another poll.
The enabled unit remained active with `NRestarts=0`; the protected backup was
retained and rollback did not run.

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
testing. The production worker is `NativeTelemetryWorker`: it reads and
validates `GlobalMemoryStatusEx`/`GetPerformanceInfo` during initialization and
each poll, then currently discards the sample. The QGA client remains a tested
adapter boundary but is not constructed by interactive or SCM startup. No
demand report or resize is produced by this path.

For SCM validation under the configured `LocalService` account, deploy the
binary to `C:\Program Files\VirtioMemService` and grant that account
read/execute access. Use `sc.exe` explicitly from an elevated VS Code terminal;
PowerShell's `sc` alias is `Set-Content`. This deployment reached `RUNNING`,
stopped cleanly, and removed successfully during live M7 validation. Event IDs
1000–1003, invalid-configuration event 2000, exit codes, and the first 5-second
recovery restart are verified.

Service installation also registers the configured description and bounded
failure actions: restart after 5 seconds, 30 seconds, and 60 seconds, with a
24-hour reset period. Recovery actions are enabled for non-crash failures so
unexpected non-zero worker exits can be recovered; intentional stop remains a
successful zero-exit lifecycle. The wider restart/reboot/interruption matrix
remains M10b work.

The SCM path writes lifecycle and failure records to the Windows Application
Event Log with source `VirtioMemService`. Query the latest records from an
elevated Windows terminal with:

```text
wevtutil.exe qe Application /q:"*[System[Provider[@Name='VirtioMemService']]]" /f:xml /c:20 /rd:true
```

Expected IDs are 1000 (start pending), 1001 (running), 1002 (stop requested),
1003 (clean stop), 2000 (configuration failure), 2001 (worker failure), 2002
(worker exited without cancellation), and 2003 (SCM status-publication
failure). Messages are single-line and bounded. A normal stop must end with
1003 and service exit code zero. A controlled startup/worker failure must emit
a 2xxx event, report non-zero service exit, and follow the configured bounded
restart delays. Preserve the original configuration and restore it before
removing the service; do not combine this test with VM or memory mutation.

Use XML output and inspect each `<EventData><Data>` value. The current classic
event source has no registered message resource, so `/f:text` can show an
unrelated system description even though the event ID, severity, and raw
insertion string are correct. Message-resource packaging remains ISSUE-008.

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

The concrete runtime wiring tests verify that invalid configuration fails
before worker construction with a stage-specific `RuntimeWiringError`. SCM
startup preserves equivalent context for worker construction,
initialization, and service-host execution; these paths do not construct a
guest-side resize sink.

QGA transport tests verify that a zero operation timeout is rejected before a
pipe is opened, the default timeout is five seconds, and Windows timeout values
are clamped to the valid millisecond range. Captured-envelope tests verify the
newline-terminated request, stable response correlation id, required `return`
array, matching response id, and rejection of empty, multiple, or malformed
frames. On Windows, an overlapped I/O timeout calls `CancelIoEx` and closes the
request handles before returning.

The `VirtioMemState` contract tests additionally verify the canonical byte
unit, minimum and power-of-two block size, requested/current/target alignment,
device-size/block alignment, maximum-value boundaries, acceptance of zero
observed state, and rejection of zero targets before a host resize sink is
allowed to issue a request. Unit-boundary
tests verify checked KiB-to-byte conversion, exact byte-to-KiB conversion,
maximum representable round trips, and rejection of lossy conversion.

Polling integration tests also verify that an invalid virtio-mem snapshot is
rejected before QGA polling and that a proposed target is validated before the
resize sink is called.

XML contract tests cover alias extraction, mixed explicit units, unsupported
units, malformed/incomplete state, wrong device model, conversion overflow,
alignment, and device-size bounds. The parser is intentionally tested with
captured XML strings; live `virsh` discovery remains host-side validation work.

M9a compatibility tests cover explicit enabled attributes, disabled attributes,
absent attributes, invalid attribute values, independently completed evidence,
conflicting sources, and live QMP bool/string values. Before every prepared
resize, the host sink queries the selected QOM device and rejects unavailable,
disabled, malformed, or conflicting QMP/XML evidence.

The host memory-stat parser applies the same checked KiB-to-byte rule and
rejects `/proc/meminfo` multiplication overflow. Compatibility settings such
as `dynamic-memslots`/`unplugged-inaccessible` and workload/device exclusions
remain a separate live XML and operator-evidence gate; they are not inferred
from absent XML attributes.

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
`guest-get-host-name`. The guest reported QGA version `110.0.2`, Windows 11
x64, and hostname `ICE101`.

QGA `guest-get-memory-stats` returns `command ... has not been found`, and
`guest-info` does not advertise that command. This is expected for upstream
QGA, not a transport failure; three `dommemstat` samples prove observability,
while M9e still has to qualify their semantics and freshness.

The compatible `virsh dumpxml win11_gpu` inspection found virtio-mem alias
`ua-virtiomem0`, size `20971520 KiB` (20 GiB), block `2048 KiB` (2 MiB), and
`requested=0 KiB` / `current=0 KiB`. The direct `qemu-system-x86_64 --version`
check was unavailable because that binary is not in the current PATH, although
`virsh version` reported libvirt `11.10.0`, QEMU API `11.10.0`, and hypervisor
`10.1.0`.

On 2026-09-04, a controlled stop/start of the Windows `qemu-ga` service and a
graceful QGA-mode reboot of `win11_gpu` both recovered. The complete host probe
passed again after reboot, including three QGA identity reads, three
`dommemstat` samples, connected channel XML, and
`requested=current=0`. The reboot-initiating command may lose its response as
QGA exits; determine success from bounded disappearance/recovery plus a fresh
post-boot probe, never from the initiating response alone.

On the RHEL host, validate the Windows guest agent and live device before enabling automatic updates:

1. Confirm `guest-info` and the configured QGA or `dommemstat` memory-stat
   source succeed three times.
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
  --requested-size "$TARGET_KIB" \
  --live
```

Expected behavior from the docs:

- The live XML may show `requested != current` while QEMU and the guest are converging.
- The service must wait for convergence before issuing the next resize request.
- `virsh --requested-size` uses KiB by default; convert canonical bytes
  exactly and reject a target that is not divisible by 1024.
- A request that is not an integer multiple of the block size is invalid.
- The host should treat the live XML as authoritative, not just the last command response.

This is the required safety gate for live validation: if `requested` and `current` are still diverged, the controller must not keep sending resize changes.

The Rust host CLI is authoritative for explicit snapshot, validation, dry-run,
and applied resize operations. Build it with the native RHEL gate before use:

```bash
# Read-only: capture live XML after confirming the selected alias exists once
target/release/virtio-mem-host snapshot \
  "$VM_NAME" "$VIRTIO_MEM_ALIAS" --connect qemu:///system > live-memory.xml

# Read-only: print the selected device's canonical-byte state and XML evidence
target/release/virtio-mem-host validate \
  "$VM_NAME" "$VIRTIO_MEM_ALIAS" --connect qemu:///system

# Read-only dry run: print the exact validated virsh argument vector
target/release/virtio-mem-host resize \
  "$VM_NAME" "$VIRTIO_MEM_ALIAS" "$TARGET_BYTES" \
  --workload-reviewed \
  --host-min-headroom-bytes 4294967296 \
  --connect qemu:///system
```

The CLI defaults to `qemu:///system` and accepts a constrained alias. Snapshot
and validate issue only `virsh dumpxml`. Resize defaults to dry-run and does
not issue `update-memory-device` unless `--apply` is present. Both dry-run and
apply require fresh QMP/XML confirmation of `dynamic-memslots` and
`unplugged-inaccessible`, `requested == current`, a block-aligned canonical-
byte target with device headroom, explicit `--workload-reviewed` operator
evidence, and a positive host reserve. Grow operations read
`/proc/meminfo` and fail closed unless `MemAvailable` covers the growth delta
plus `--host-min-headroom-bytes`; shrink operations do not require available
host RAM.

Observed `requested` and `current` may both be zero for a fully unplugged
device; live M9 validation on `win11_gpu` exercises this state. Zero remains
invalid as a resize target. On 2026-09-05, the Rust CLI snapshot matched the
direct live XML hash, canonical-byte validation passed, a wrong alias was
rejected, and a no-`--apply` dry run failed closed on unknown compatibility
evidence. Identical before/after XML hashes confirmed non-mutation.

Live M9a validation on 2026-09-05 confirmed both selected-device QMP
properties, a 2 MiB block/THP match, `mem-lock=off`, and no VFIO-NVMe, RDMA,
or unsupported vhost-user dependency. The operator completed the workload
review. The CLI produced the exact 2 MiB dry-run vector without `--apply`, and
the before/after XML hashes matched. Systemd configuration must set
`VIRTIO_MEM_WORKLOAD_REVIEWED=true` only after the same review; `false` keeps
resize preparation fail-closed.

After reviewing the dry-run vector and obtaining approval for the exact VM,
alias, target, and expected live mutation, repeat the same command with
`--apply`. A successful `virsh` return only means the request was accepted;
observe fresh XML until `requested == current` before any subsequent request.
The CLI performs one request and never loops or retries.

The former `scripts/virtio-mem-host.sh` implementation was removed in M9c, so
that helper no longer duplicates the Rust XML, arithmetic, compatibility, or
resize policy.

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

Native Windows telemetry is the Windows demand source; host-side QGA health and
`dommemstat` are separate host observation paths. The
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
as a complete version-1 `DemandReport`. Version 1 has no freshness/identity
envelope and the sink has no retention, rotation, acknowledgement, or atomic
handoff contract, so this proves local append/flush behavior only. The default
path is under `C:\ProgramData\VirtioMemService`; installation must provision
the directory and least-privilege ACLs before enabling unattended service
output. The main SCM
worker remains unconnected to publication until M10c implements the selected
host-side join; tests must not substitute a configured minimum, aggregate
physical memory, QGA total, or balloon `actual` for live libvirt `current`.

The native collector calls `GlobalMemoryStatusEx` for physical memory and
`GetPerformanceInfo` for page-based commit/system counters. Page counters are
converted using checked multiplication by the reported page size. Windows API
failures and invalid snapshots return explicit errors. The report is version 1
and serializes canonical-byte values; it is advisory only and has no resize
interface.

### M9d/M9e/M10c/M10d hermetic gates

Before expanding live actuation or enabling unattended demand publication:

- M9d tests must hash the reviewed domain/QEMU configuration, accept an exact
  match, and fail closed on changed aliases, incompatible device classes,
  QMP properties, memory backend/page/NUMA attributes, slot and VFIO mapping
  budgets, balloon-resize state, topology, trust classification, deployed
  versions, or workload declarations.
- M9e tests must accept `unused` and `available` above balloon `actual`, retain
  `available` when otherwise valid, and reject missing, stale, future, or
  non-advancing `last-update`. The Rust decision CLI must use the controller's
  configured source and produce the same decision as one runtime cycle.
- M10c tests must prove the host joins a fresh raw Windows envelope with a
  fresh alias-scoped live libvirt `current` snapshot and calculates the target
  there. Missing, stale, cross-VM, or conflicting state must prevent policy;
  no host allocation feed or guest resize interface is permitted.
- M10d tests must reject unsupported versions, stale timestamps, replayed or
  non-monotonic sequences, wrong VM/session identity, missing provenance,
  truncated lines, oversized records, and partial writes. Retention/rotation
  and reader handoff must have deterministic size and recovery bounds.
- Restart tests must prove a new session cannot reuse ordering state in a way
  that makes an old record appear fresh.

The demand report must be read-only with respect to virtio-mem. A passing local
collector test does not prove that QEMU/libvirt or `viomem.sys` converges.

### Trusted development deployment boundary

`win11_gpu` is a fully trusted development/test KVM guest and the only
currently supported live topology is one active controller managing its one
explicit virtio-mem alias. Validate that no second controller instance is
active before a Phase 2 mutation. Multiple devices may exist upstream, but
multi-controller/device actuation waits for M11 atomic global reservation.

Because QEMU does not completely protect unplugged memory from guest access,
record a hard QEMU/libvirt cgroup memory limit for untrusted or production
tests; absence is a hard failure. For trusted `win11_gpu`, record whether the
limit exists and treat it as recommended defense-in-depth, not a current gate.
This trust exception must not be copied to another VM without an explicit
support-profile change.

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
observable. The Virtio and pinned implementation-source contract establishes
the semantic mapping; driver output is optional diagnostic evidence about the
installed binary, not a second allocation source. Do not add a direct driver
IOCTL or perform an unbounded shrink based on this design document.

### M10a read-only driver-state discovery

From the RHEL control plane, the following commands inspect only the explicit
Windows SSH endpoint. They do not start tracing or change the driver, service,
registry, VM, or memory allocation:

```bash
ssh -o BatchMode=yes -o ConnectTimeout=10 virtio-mem-windows \
  "sc.exe query viomem & driverquery /v /fo csv | findstr /i viomem"

ssh -o BatchMode=yes -o ConnectTimeout=10 virtio-mem-windows \
  'pnputil /enum-devices /instanceid "PCI\\VEN_1AF4&DEV_1058&SUBSYS_11001AF4&REV_01\\4&28aa03cd&0&0012" /properties /drivers'

ssh -o BatchMode=yes -o ConnectTimeout=10 virtio-mem-windows \
  'reg query "HKLM\\SYSTEM\\CurrentControlSet\\Services\\viomem" /s & logman query providers | findstr /i /c:viomem /c:virtio'
```

On `ice101.lan`, these checks identify signed driver
`100.102.104.29400` and a started device, but no state values, WMI/performance
surface, or registered tracing provider. The contemporaneous upstream `mm314`
source has no IOCTL handler and sends its `Memory config` record to kernel
debug output because `EVENT_TRACING` is disabled. `systeminfo` may be used as
a secondary aggregate-memory observation, but it is not an allocation source
and cannot diagnose the selected device's requested and plugged fields.

Do not proceed directly from this discovery to a live resize. Any diagnostic
capture and any resize are distinct approval boundaries unless one explicitly
approved procedure names both. A resize procedure must name the target,
timeouts, required host and guest samples, original controller state, a
non-convergence recovery target, and cleanup. It must not describe a shrink as
guaranteed rollback. If capture tooling requires
installation, registry/debug-policy changes, a driver restart, or a reboot,
those mutations and their rollback must also be named explicitly. Absence of
a supported capture path limits guest-side diagnosis; it does not invalidate
alias-scoped live libvirt `current` as host allocation authority and is not
permission to invent or probe undocumented IOCTLs.

The preferred capture candidate is Microsoft's signed Sysinternals
`dbgviewcli.exe`. Its kernel mode captures `DbgPrint`, supports duration and
line-count bounds, and can filter for `*Memory config:*`. Kernel capture
requires Windows Administrator rights and automatically extracts and loads
the temporary `Dbgv.sys` capture driver, so even this path is not read-only
discovery. A proposed elevated capture must use both `--duration` and
`--max-lines`, disable unrelated Win32 output, write only to a named temporary
guest artifact, and confirm that the capture process and temporary driver have
exited before cleanup. Informational `DbgPrint` records may be filtered before
they reach the capture buffer, so a no-resize run proves tool lifecycle only,
not that viomem records are observable. The first qualification must not enable
boot logging, persist a debug-filter registry change, restart the driver, or
reboot the guest. If a separately approved resize is performed, the host
controller must be stopped so it cannot race the harness and then returned to
its recorded original state.

The allocation-contract and diagnostic work is split into explicit validation
gates:

- **M10a/M10a4:** the Virtio 1.2 and pinned QEMU/libvirt/virtio-win source
  contract makes alias-scoped live libvirt `current` authoritative. Preserve
  version pins and re-audit this conclusion after stack upgrades.
- **M10a1 (optional):** record tool provenance and checksum; run a no-resize
  capture with duration and line limits; confirm the process exits and return
  the temporary driver/artifacts to baseline. Record debug-filter observations
  and do not treat an empty capture as proof that the driver emitted nothing.
- **M10a2:** define a versioned evidence record with wall-clock and monotonic
  ordering, source identity, units, VM/device alias, operation correlation ID,
  and raw-value provenance. Required evidence is QEMU/libvirt state, Windows
  health, and controller state; driver trace is optional. Hermetic tests reject
  absent required layers, mixed operations, unit ambiguity, non-monotonic
  samples, and missing convergence endpoints.
- **M10a3 (optional):** after disposable-guest rehearsal when practical and
  separate approval, stop the active controller and run one bounded operation
  with a predeclared recovery target. Attempt exact state restoration but do
  not classify shrink as guaranteed rollback. Restore the original controller
  state and record any non-convergence as evidence, not a reason to overlap
  requests.
- **M10aX (conditional):** only for a concrete diagnostic need unmet by host
  observation and bounded tracing, write a separate feasibility proposal for
  a versioned read-only driver interface. Cover ACLs, malformed requests,
  timeout, compatibility, build/signing/install, rollback, and disposable-
  guest tests; do not add driver work to the normal Rust gate.

## Known Blockers

- The Windows-native Rust 1.97.1 MSVC toolchain now passes the full local
  format, release build, test, and Clippy pipeline.
- Further live QEMU Guest Agent, libvirt, tracing, reboot, service, or resize
  validation requires the named RHEL host/Windows guest, explicit scope, and
  the approval procedure above. M7–M9b evidence already passes.
- Optional M10a1/M10a3 diagnostics require explicit protected-guest approval;
  they do not block M9d/M9e, M10c/M10d, or hermetic M11 simulation.

### M10a2 hermetic behavior-evidence validation

Run the shared-core gate without a VM or administrator privileges:

```bash
cargo test -p virtio-mem-core --all-features --locked behavior_evidence
cargo test -p virtio-mem-host --all-features --locked validates_correlated_evidence
cargo clippy -p virtio-mem-core --all-targets --all-features --locked -- -D warnings
```

The focused tests accept correlated records with and without driver trace and
reject missing or unknown units, mixed operation/VM identity, non-increasing
sequence or monotonic time, backwards wall-clock time, missing Windows or
controller layers, absent or divergent convergence endpoints, changed host
geometry, and invalid driver diagnostic values. These are hermetic parser
tests; they do not authorize or perform a live resize.

### M10a3 live one-block result — 2026-09-07

The approved `win11_gpu/ua-virtiomem0` operation started converged at 1 GiB,
grew by exactly one 2 MiB block, and observed convergence at both host fields.
The predeclared recovery request returned `requested` to 1 GiB, while `current`
remained 1 GiB + 2 MiB for 60 samples at five-second intervals. The 300-second
bound expired and no overlapping request was issued. `viomem` and fresh
`dommemstat` telemetry remained healthy. The filtered kernel capture contained
no matching `Memory config` record, so this result establishes host-visible
shrink non-convergence but does not claim driver-internal field correlation.
A separate graceful QGA domain shutdown/start then recreated the device from
the persistent 1 GiB definition. Final checks showed
`requested=current=1073741824`, fresh balloon counters, running `viomem`, no
DbgView residue, and an active controller with `NRestarts=0`.

### M10b larger-shrink probe — 2026-09-07

To test whether the one-block result was caused by too-small control input, an
approved isolated probe grew from 1 GiB to 3 GiB, held for 30 seconds, and then
requested 2 GiB. Growth converged in about two seconds. Shrink immediately
unplugged 257 of 512 requested blocks (514 MiB), reaching `2682257408` bytes,
then made no further progress: 255 blocks (510 MiB) remained above the 2 GiB
target for the rest of 300 seconds. This is partial-progress-without-retry
evidence, not a minimum-size rejection. No overlapping request was issued.
Graceful domain recreation restored the persistent 1 GiB baseline; final
checks showed fresh balloon telemetry, running `viomem`, and an active
controller with `NRestarts=0`.

### M10b bounded retry/recovery qualification

The next live shrink work is two separately approved, controller-isolated
operations after the hermetic state-machine suite passes:

1. Reproduce a bounded no-progress or partial-progress shrink and send only the
   identical target on the 30/60/120-second schedule. Success requires a later
   block decrease or full convergence attributable to a re-notification,
   never more than three re-notifications, and termination by 300 seconds.
2. From a stable incomplete shrink, capture two unchanged fresh samples,
   immediately re-read the alias, and issue one abandon-to-current request to
   the latest aligned `current`. Success requires convergence within 30 seconds
   without replug beyond that pre-apply value, a second recovery command, guest
   restart, or controller restart.

If the first operation shows that an identical libvirt/QEMU property update is
coalesced or does not wake the driver, do not increase the retry count or
frequency. If abandon-to-current fails or races into unexpected growth, latch
actuation off and use the already proven operator-approved graceful domain
recreation fallback. Preserve ordered XML samples, exact command arguments and
statuses, controller events, Windows/service health, and recovery outcome.

Validate an assembled document with the read-only CLI path:

```bash
target/release/virtio-mem-host evidence /path/to/behavior-evidence.json
```
