# Testing Strategy

## Local Testing

All testing is performed locally. No CI pipeline is currently configured.

### Privilege and password policy

The normal Rust validation path does not require root and should be run as the
regular development user:

```bash
cargo xtask gate local
```

The tool runs rustfmt, a locked release build, locked tests, warnings-denied
Clippy for the RHEL-compatible crates and tooling, and `git diff --check`.

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
approved task and must not become a general privileged command runner. It must
call `cargo xtask` or the product Rust CLI for reusable parsing, polling, and
safety logic rather than copying those implementations into Bash.

After that one-time setup, run read-only probes and the controller under the
approved account or authorization context. If a task genuinely needs root,
give the execution notice with the complete command, protected target,
expected mutation, and rollback behavior, then run the entire task script once
under `sudo` rather than adding `sudo` to individual subcommands.
Never automate or collect the password; the operator types it directly into
the terminal. Do not use `sudo -S`, modify sudoers, or weaken host permissions
just to make a test pass.

A bounded reversible live resize, task-scoped service lifecycle, and candidate
installation remain separate from the unprivileged suite but are authorized by
the repository beta-validation rules after the required execution notice and
safety gates. Reboots, non-reversible resize, deletion of pre-existing state,
and persistent platform/security changes still require explicit approval.

#### Layered Windows guest-health gate

A running libvirt domain and a successful `guest-ping` prove only that QEMU and
QGA respond; they do not prove that Windows, its service manager, or the test
application is stable. Every guest lifecycle batch must capture the domain
UUID/ID and QEMU PID/start time, then verify QGA plus an independent
authenticated Windows command and each named service/application endpoint.
Inspect pending-reboot/installer state before mutation and stop if unrelated
work can reboot the guest inside the test window.

For a planned guest-only reboot, retain the same QEMU process/domain identity,
observe exactly one Windows boot-marker transition, and correlate Windows
System events. Event IDs 41 (Kernel-Power), 6008 (unexpected shutdown), and
1001 (BugCheck) fail the run. Event 1074 must name the expected initiator; an
additional initiator or second boot also fails the run. Do not perform a later
resize, attestation replacement, or other persistent mutation after any such
failure.

After Windows first becomes reachable, use at least three repeated end-to-end
checks across a bounded quiet window rather than accepting the first QGA
reply. Use a ten-minute default when lifecycle validation precedes a security
or attestation change, unless a shorter bound is justified against all delayed
reboot mechanisms in scope. A typical read-only probe includes:

```bash
virsh -c qemu:///system domstate win11_gpu --reason
virsh -c qemu:///system domid win11_gpu
virsh -c qemu:///system domuuid win11_gpu
virsh -c qemu:///system qemu-agent-command win11_gpu \
  '{"execute":"guest-ping"}'
ssh -F /home/dave/.ssh/config -o BatchMode=yes virtio-mem-windows \
  'cmd.exe /d /s /c "ver & sc.exe query qemu-ga"'
ssh -F /home/dave/.ssh/config -o BatchMode=yes virtio-mem-windows \
  'wevtutil qe System "/q:*[System[(EventID=41 or EventID=6008 or EventID=1001 or EventID=1074)]]" /c:20 /rd:true /f:text' \
  | tr -d '\000'
```

The task script must bound each network command, record the pre-operation event
baseline and boot marker, and select the exact guest services/applications for
the workload under test.

See [`dependencies.md`](dependencies.md) for the complete toolchain and host/
guest prerequisite matrix.

### Rust Service Testing

#### VS Code workflow

The repository includes `.vscode/tasks.json` so the normal non-mutating
validation path can be run from VS Code without an administrator terminal:

- **RHEL: full local gate** — format, native release build, tests, Clippy, and
  diff checks for the shared core, RHEL host controller, and Rust tooling. The
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
default it to `virtio-mem-windows`. When invoking the tool directly, export
that variable in the shell. Optionally set `VIRTIO_MEM_WINDOWS_DIR` and
`VIRTIO_MEM_WINDOWS_ARTIFACTS`; the defaults are documented in
`windows/README.md`. Prefer the aggregate task or `cargo xtask windows all`
for a complete gate so source is synchronized only once. The Rust tool
initializes the Visual Studio MSVC environment using `vswhere.exe`, so the SSH
account must be able to access the installed Build Tools.

The tool resolves the active Cargo toolchain with `rustup which cargo` and
invokes its real executables. This is required on `ice101.lan` because its
`.cargo\bin` rustup proxy symlinks fail through OpenSSH with Windows error 448
even though the underlying MSVC toolchain is healthy. The tool also removes
the carriage return from `certutil.exe` output before parsing the remote
SHA-256.

The authoritative terminal entry point is `cargo xtask gate all`. It requires
`VIRTIO_MEM_WINDOWS_SSH`. The Make targets remain compatibility aliases and
contain no independent gate policy.

To collect TASK-011 milestone evidence, run the fingerprint-pinned Rust command
from the RHEL checkout. It checks the endpoint first, executes the aggregate gate
twice, and stores both logs and staged executable hashes under the ignored
`.vscode-artifacts/windows/milestone-TIMESTAMP/` directory:

```bash
cargo xtask windows milestone \
  WINDOWS_SSH_ALIAS SHA256:EXPECTED_ED25519_HOST_FINGERPRINT \
  ~/.ssh/OPTIONAL_PRIVATE_KEY
```

Omit the private-key argument when the alias or `ssh-agent` already selects the
correct key. The command uses a temporary known-hosts file, refuses a host-key
mismatch, and does not modify `~/.ssh/config` or `~/.ssh/known_hosts`. Success
requires both aggregate runs to exit zero and each fetched artifact to pass the
tool's Windows/RHEL SHA-256 comparison.

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

Use the Rust `decision` command to see whether the configured policy would
grow, shrink, wait, or leave the guest unchanged. It loads the same `HostConfig`,
selected `dommemstat`/custom-QGA source, live XML source, freshness checks, and
shared evaluator as one controller cycle. It prints source, counters,
authoritative requested/current allocation, and decision. It has no resize
sink and never invokes `update-memory-device`; exit zero means the decision was
evaluated successfully, not that a resize occurred.

Example with an already-approved, non-secret instance configuration loaded in
the current shell:

```bash
set -a
source /etc/virtio-mem-host/INSTANCE.conf
set +a
target/release/virtio-mem-host decision
```

With `VIRTIO_MEM_STATS_SOURCE=dommemstat`, success requires `actual`, `unused`,
`available`, and `last-update`; the timestamp must be within the configured age
and future-skew bounds. Repeating the command starts a new one-cycle process,
so strict cross-sample advancement is exercised by the long-running controller
and deterministic source tests rather than across separate CLI invocations.

#### Reversible live-resize test

Use `cargo xtask live-resize` for an explicitly scoped, reversible test of
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
cargo xtask live-resize win11_gpu ua-virtiomem0 1073741824
```

On hosts where the default `virsh` connection is `qemu:///session`, add
`--connect qemu:///system` so the test uses the system libvirt instance:

```bash
cargo xtask live-resize win11_gpu ua-virtiomem0 1073741824 --connect qemu:///system
```

Only after confirming the target and giving the required execution notice for
the exact reversible live mutation should the apply form be used:

```bash
sudo /home/dave/github/virtio_mem_windows_service/target/release/virtio-mem-xtask \
  live-resize win11_gpu ua-virtiomem0 1073741824 \
  --connect qemu:///system --apply --timeout 30 \
  --log /tmp/win11_gpu-memory.csv
```

Build the tool as the regular user first (`cargo xtask gate build`) so `sudo`
does not create root-owned Cargo artifacts. The privileged form is valid only
for that exact VM, alias, target, and reversible test. `--keep-target` is
non-reversible and still requires explicit approval. If sudo requests
authentication, type the password directly in the terminal; the agent must
not receive it.

The earlier 20 GiB attempt demonstrated why the full-device guard is required:
the VM already has 8 GiB of base RAM and the host has approximately 30 GiB of
physical memory. Requesting the full 20 GiB virtio-mem device could approach
28 GiB of guest memory before QEMU and host overhead. The attempt was rejected
by `virsh` at the KiB boundary and the VM remained unchanged, but the test is
now blocked before any live command by the explicit full-device and host-headroom
checks.

The Rust tool accepts canonical byte targets but converts them to KiB for
`virsh --requested-size`, whose default unit is KiB. It rejects targets that
cannot be represented as an exact KiB value and never rounds silently.

The tool never guesses a target, never runs two resize requests at once, and
does not retain a target unless `--keep-target` is explicitly added. A timeout
or interruption attempts to restore the original requested size. Review the
CSV and console samples for convergence latency and QGA observations. The
current guest's missing `guest-get-memory-stats` capability will leave the QGA
columns as unavailable, but XML `requested/current` convergence can still be
observed.

For Windows shrink tests, convergence timeout is not proof that the installed
driver will retry. Source review found no obvious periodic retry timer. All
deployments now default to the selected 64 MiB automatic-reclaim quantum.
Set `VIRTIO_MEM_AUTOMATIC_WINDOWS_SHRINK=false` only for diagnosis or a
deliberate rollout pause; all estimator/reconciler safety gates remain
mandatory. The qualification profile samples every five seconds and re-notifies
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
cargo xtask gate local
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

Windows balloon reports may expose `unused` and `available` above `actual`.
The M9e parser retains `actual` only as balloon provenance, maps `unused` to
free bytes, and maps required `available` to both available and the total-like
legacy bound. It rejects only `unused > available`, overflow, malformed or
duplicate fields, and missing/stale/future/non-advancing `last_update`
(`last-update` is retained as a compatibility spelling).
`VIRTIO_MEM_STATS_MAX_AGE_SECONDS` and
`VIRTIO_MEM_STATS_FUTURE_TOLERANCE_SECONDS` are required positive bounds; the
example uses 60 and 5 seconds. The configured balloon stats period must be
shorter than the controller poll interval so each long-running sample advances.

Production defaults to `VIRTIO_MEM_DEMAND_SOURCE=raw`. The explicit
`guest-stats` compatibility mode is limited to a trusted development instance
without a provisioned M10d transport. Its tests must prove it feeds
`MemoryStats` directly into the shared directional controller and does not
synthesize a raw Windows telemetry envelope.

`VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES` is a required configuration value: the
controller will not send a grow request unless the RHEL host's
`/proc/meminfo` `MemAvailable` covers the requested delta plus this reserve.
An insufficient-headroom check is not treated as a fatal error; the
controller logs and waits for the next poll interval rather than crashing the
systemd unit.

The host service defaults `VIRTIO_MEM_GROW_STEP_BYTES` to 1 GiB and
`VIRTIO_MEM_SHRINK_STEP_BYTES` to 64 MiB. Tests must prove both are positive,
aligned to the live device block, clamped at configured limits, and applied at
most once before `requested == current` is observed again. Critical pressure
still produces only one 1 GiB request; it does not multiply the actuation
quantum.

`VIRTIO_MEM_COMPATIBILITY_ATTESTATION_PATH` is also required. It names a
root/operator-owned, service-readable version-1 attestation created only after
the explicitly scoped VM has been reviewed for vDPA, VFIO-NVMe, RDMA migration,
`mlock`, encrypted/secure virtualization, active balloon resize, vhost-user
backend/version and slot capacity, VFIO mapping budget, backend sparse/reserve/
preallocation/share/core-dump/page-size/NUMA properties, topology, trust, driver,
QEMU, and libvirt versions. Missing or writable-by-service evidence is not a
valid deployment. Before every prepared resize the controller verifies the
attestation SHA-256 and recollects its bounded live XML/QEMU/QMP/version inputs;
any drift blocks actuation.

#### Testing through the installed host service, not the standalone tool

The standalone `cargo xtask live-resize` command is a pre-installation
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
After convergence, ordinary policy resumes one directional quantum at a time and cannot
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
testing. The production `RawTelemetryWorker` reads and validates
`GlobalMemoryStatusEx`/`GetPerformanceInfo`, adds the configured VM name and
Unix observation time, and appends a raw JSON-lines record during
initialization and each poll. The QGA client remains a tested adapter boundary
but is not constructed by interactive or SCM startup. No allocation input,
guest-calculated demand report, or resize is produced by this path.

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
successful zero-exit lifecycle. M10b's bounded restart/interruption/recovery
scope is closed; broader target-controller qualification remains M10g work.

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
and M9e subsequently qualified their semantics and freshness in Rust.

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

# Read-only: after reviewing every positive declaration in review.json, bind
# those declarations to the current live configuration. Review the output,
# then install it at the configured protected path in a separately approved
# operation.
target/release/virtio-mem-host attest \
  "$VM_NAME" "$VIRTIO_MEM_ALIAS" review.json \
  --connect qemu:///system > reviewed-attestation.json

# Read-only dry run: print the exact validated virsh argument vector
target/release/virtio-mem-host resize \
  "$VM_NAME" "$VIRTIO_MEM_ALIAS" "$TARGET_BYTES" \
  --attestation reviewed-attestation.json \
  --host-min-headroom-bytes 4294967296 \
  --connect qemu:///system
```

Review and attestation inputs are bounded to 64 KiB and must be UTF-8 strict
JSON; unknown fields and any false, zero, or empty required value fail closed:

```json
{
  "trusted_development_guest": true,
  "workload_reviewed": true,
  "no_vdpa": true,
  "no_rdma_migration": true,
  "no_vfio_nvme": true,
  "no_mlock": true,
  "no_secure_virtualization": true,
  "no_unsupported_vhost_user": true,
  "balloon_resize_inactive": true,
  "memory_slot_budget": 32,
  "vfio_mapping_budget": 8,
  "windows_driver_version": "REVIEWED-INSTALLED-VERSION"
}
```

The CLI defaults to `qemu:///system` and accepts a constrained alias. Snapshot
and validate issue only `virsh dumpxml`. Resize defaults to dry-run and does
not issue `update-memory-device` unless `--apply` is present. Both dry-run and
apply require an integrity-valid exact-match attestation, fresh QMP/XML
confirmation of `dynamic-memslots` and `unplugged-inaccessible`,
`requested == current`, a block-aligned canonical-byte target with device
headroom, and a positive host reserve. Grow operations read
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
the before/after XML hashes matched. That historical static acknowledgement is
superseded by M9d: systemd configuration now names the reviewed attestation,
and absent or drifted evidence keeps resize preparation fail-closed.

After reviewing the dry-run vector and obtaining approval for the exact VM,
alias, target, and expected live mutation, repeat the same command with
`--apply`. A successful `virsh` return only means the request was accepted;
observe fresh XML until `requested == current` before any subsequent request.
The CLI performs one request and never loops or retries.

The former `scripts/virtio-mem-host.sh` implementation was removed in M9c, so
that helper no longer duplicates the Rust XML, arithmetic, compatibility, or
resize policy.

### Build/test tooling

- Put maintained build/test behavior in `tools/xtask` and add deterministic
  Rust tests for parsing, scope, malformed output, boundaries, and errors.
- Check required environment variables and host tooling before remote or live
  work. Never infer an SSH endpoint, VM, device alias, or mutation target.
- Keep Bash only for one generated or task-specific privileged process
  boundary; delegate reusable validation to Rust.

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

The legacy JSON-lines publisher tests read emitted files back as complete
records. Production uses an atomic current-record handoff with three retained
previous records.
The production `RawTelemetryWorker` emits a version-2
`RawTelemetryEnvelope` containing configured VM/service identity, a generated
process-session identifier, Unix and monotonic milliseconds, a strictly increasing
session sequence, explicit telemetry/allocation provenance, and validated raw
counters; assertions prove that allocation and target fields are absent. The default path is under
`C:\ProgramData\VirtioMemService`. Configuration schema version 3 adds the VM
name used by both interactive and SCM publication paths.

The host requires `VIRTIO_MEM_RAW_TELEMETRY_PATH`,
`VIRTIO_MEM_RAW_TELEMETRY_SERVICE_NAME`,
`VIRTIO_MEM_RAW_TELEMETRY_MAX_AGE_SECONDS`, and
`VIRTIO_MEM_RAW_TELEMETRY_FUTURE_TOLERANCE_SECONDS`. Hermetic tests inject the
clock and verify the latest complete record is accepted only for the expected
VM/service and freshness window. They reject replayed/non-monotonic ordering,
retired session reuse, incomplete final lines, records over 64 KiB, and files
   over 1 MiB. The host then obtains alias-scoped live XML and calculates
   through the M10e `TargetEstimator`; configured minimum,
   aggregate physical memory, QGA total, and balloon `actual` are never allocation
   substitutes. Live-device geometry conflicts fail closed. Automatic shrink
   defaults enabled; explicit `false` pauses it, while same-target diagnostic
   re-notification defaults disabled. The M10e adapter retains the existing
   1 GiB growth and 64 MiB reclaim actuation bounds, and M10f implements the
   full three-value reconciler; ambiguous or stalled shrink remains latched rather
   than blindly replayed.

M10e configuration requires `VIRTIO_MEM_FIXED_VISIBLE_BASE_BYTES` and
`VIRTIO_MEM_POLICY_STATE_PATH`. Normal physical/commit reserves default to
2 GiB, safe-floor reserves to 1 GiB, history to 600 seconds, the maximum gap to
twice the materialized poll interval, and downward hysteresis to 256 MiB. The
systemd unit provisions `/var/lib/virtio-mem-host` mode 0700 for the atomic
checkpoint. Success requires the checkpoint to remain VM/alias, policy, and
compatibility-fingerprint matched; a missing, invalid, oversized, mismatched,
or future checkpoint must restart reclaim warm-up without blocking fresh
growth.

Run the M10e focused tests with:

```bash
cargo test -p virtio-mem-core --all-features --locked target_controller
cargo test -p virtio-mem-host --all-features --locked target_policy
```

They prove checked candidate arithmetic, base tolerance, effective maximum,
alignment, explicit capacity limitation, history gaps/session restart,
immediate growth, delayed reclaim, checkpoint matching, and the deterministic
+4 GiB then +2 GiB target trace. M10f additionally proves bounded quanta,
upward-only supersession, stale-during-shrink freeze, partial-progress health,
write-before-command journaling, durable latch, restart/no-replay, and
dry-run-default latch clearing. M10g must continue to follow
[`target-controller.md`](target-controller.md) and report controller/platform
outcomes separately.

### M10f latch inspection and clearing

After stopping the controller instance, resolving the cause of a latched
command, and restoring the selected device to `requested == current`, preview
the configured controller latch clear. The controller must remain stopped so
its in-memory checkpoint cannot race the operator update:

```bash
target/release/virtio-mem-host clear-latch "OPERATOR_REASON"
```

This is a dry run. It loads the configured VM, alias, policy checkpoint, and
compatibility attestation, recollects live compatibility evidence, and refuses
divergent state. Success prints `mode=dry_run` and the exact converged byte
values without changing the checkpoint. Apply only after reviewing that output:

```bash
target/release/virtio-mem-host clear-latch "OPERATOR_REASON" --apply
```

Success prints `mode=applied`; the checkpoint retains the operator-visible
reason, clears the pending intent, and permits later fresh controller cycles to
actuate. A missing/mismatched checkpoint, stale compatibility attestation,
unlatched state, or `requested != current` fails without mutation.

Run the M10c hermetic host gate from the repository root:

```bash
cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked
```

The 2026-09-09 local M10e gate passes 55 shared-core and 60 host tests with zero
failures. It covers the absolute estimator and checkpoint in addition to
durable replay acknowledgement, atomic handoff, bounded retention, the shrink
schedule, latched stalls, cancellation/restart, and one-shot recovery. Run the
native Windows gate with
`VIRTIO_MEM_WINDOWS_SSH=ALIAS cargo xtask windows all`;
success includes the raw publisher and
worker tests, formatting, warnings-as-errors Clippy, and a release build. The
2026-09-08 native gate passed 67 tests and verified artifact SHA-256
`d91e6ccd2a0fdbac1da8bcd96a4e05ecf77964834e20d973c844e44d77d1e9dd`.

M10d's implementation now provisions the LocalService ProgramData DACL and
implements deterministic publisher retention/rotation, durable
acknowledgement, restart-safe replay state, and atomic reader handoff. Native
installation must still verify the resulting ACL; the current guest has no
ProgramData directory because the newly built candidate was not installed.

The native collector calls `GlobalMemoryStatusEx` for physical memory and
`GetPerformanceInfo` for page-based commit/system counters. Page counters are
converted using checked multiplication by the reported page size. Windows API
failures and invalid snapshots return explicit errors. The report is version 1
and serializes canonical-byte values; it is advisory only and has no resize
interface.

### M9d/M9e/M10c/M10d hermetic gates

Before expanding live actuation or enabling unattended demand publication:

- M9d tests hash the reviewed domain/QEMU configuration, accept an exact
  match, and fail closed on changed aliases, incompatible device classes,
  QMP properties, memory backend/page/NUMA attributes, slot and VFIO mapping
  budgets, balloon-resize state, topology, trust classification, deployed
  versions, or workload declarations.
- M9e tests accept `unused` and `available` above balloon `actual`, retain
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
the normal Rust build/test gate.

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

The M10a3 and larger-shrink evidence met that condition because neither host
state nor two bounded captures distinguish driver notification, branch, or
failure outcome. The completed
[`driver-status-interface-feasibility.md`](driver-status-interface-feasibility.md)
defines the separate ABI, security, test, signing, disposable-guest, install,
and rollback gates. It authorizes no driver build, install, live query, or Rust
service integration.

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

M10b was closed by operator acceptance on 2026-09-09 using the completed
hermetic matrix and existing bounded live evidence. The clean
active-controller reboot plus refreshed-attestation batch remains optional
follow-up evidence; it has not been recorded as passing.

Run the deterministic recovery matrix before any live operation:

```bash
cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked
cargo clippy -p virtio-mem-core -p virtio-mem-host \
  --all-targets --all-features --locked -- -D warnings
```

The matrix must prove that live-state interruption and an external target
change latch an owned shrink without a fatal worker exit, cancellation emits
no replay, a restarted process only observes unowned divergence, and a
pre-command rejection is distinguishable from an invoked command with an
unknown result. Allocation-only changes to `requested`, `current`, and
libvirt's derived top-level `currentMemory` must leave the M9d domain
fingerprint unchanged. The current gate passes 55 shared-core and 60 host
tests.

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

Use the Rust qualification paths so Bash does not duplicate retry policy:

```bash
target/release/virtio-mem-host qualify-shrink VM ALIAS TARGET_BYTES \
  --attestation REVIEWED_ATTESTATION --connect qemu:///system

target/release/virtio-mem-host qualify-shrink VM ALIAS TARGET_BYTES \
  --attestation REVIEWED_ATTESTATION --apply --connect qemu:///system

target/release/virtio-mem-host abandon-shrink VM ALIAS IMMUTABLE_TARGET_BYTES \
  --attestation REVIEWED_ATTESTATION --apply --connect qemu:///system
```

`qualify-shrink` permits exactly one block and reuses the 30/60/120-second,
three-notification, 300-second shared state machine. `abandon-shrink` requires two unchanged samples five
seconds apart, immediately rereads before apply, sends one request to observed
`current`, and waits at most 30 seconds. Both default to dry-run unless
`--apply` is explicit. The prepared 2026-09-08 live batch could not start
because sudo required interactive host authentication; it made no mutation,
and read-only rechecks confirmed `requested=current=1 GiB` and the controller
still active.

The 2026-09-09 M10b candidate service batch likewise timed out at the
interactive sudo prompt before the script began and was not retried
piecemeal. Its read-only post-check confirmed the previous installed binary
remained in place, the service was active with `NRestarts=31`, and
`win11_gpu/ua-virtiomem0` remained converged at
`requested=current=2105344 KiB`. Run the prepared task-scoped batch only from
an interactive terminal where the operator can answer sudo directly:

```bash
sudo bash /home/dave/github/virtio_mem_windows_service/.vscode-artifacts/privileged-tasks/m10b-service-recovery-validation.sh
```

The first operator invocation failed its converged-state assertion at line 66
because the XML extractor greedily removed the requested/current contents.
This happened before the intended mutation, but prematurely armed rollback
still performed a clean stop/start and retained the previous binary. The unit
returned active with `NRestarts=0` and unchanged convergence. The corrected
batch parses the values explicitly, arms rollback only after creating the
backup, uses an SELinux-restored staged file and atomic rename, verifies the
installed SHA-256 before start, and prints a failed line/status on error.
Treat only its explicit `rollback=not_needed` result plus matching
installed/candidate hashes as successful candidate evidence.

The corrected invocation passed on 2026-09-09. Candidate and installed SHA-256
both matched
`a1c431e67b49ba0373091fb32760cecea38a7e311bdc04a8972a9a44bf2a607c`,
the installed file retained `system_u:object_r:usr_t:s0`, and the service
remained active with `NRestarts=0`. The controller emitted exactly one
operation-correlated `shrink_request_rejected` event for the intentionally
unchanged old attestation. It emitted no `shrink_requested` or
`shrink_command_unknown` event, the VM remained converged at
`requested=current=2105344 KiB`, and no staged or backup file remained.
