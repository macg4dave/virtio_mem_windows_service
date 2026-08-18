# Testing Strategy

## Local Testing

All testing is performed locally. No CI pipeline is currently configured.

See [`dependencies.md`](dependencies.md) for the complete toolchain and host/
guest prerequisite matrix.

### Rust Service Testing

```bash
cd windows
cargo build --release
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
```

The Rust service tests should cover QGA response parsing, threshold boundaries, minimum/maximum safe ranges, and invalid configuration cases. They do not require a live VM.

### RHEL host controller testing

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
testing. The current worker is intentionally an idle, stoppable lifecycle
harness; QGA state acquisition and a resize sink remain separate follow-up
work and must not be inferred from a successful service start.

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
# Inspect the guest's live memory devices and look for requested/current values
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

## Known Blockers

- The Windows-native Rust 1.97.1 MSVC toolchain now passes the full local
  format, release build, test, and Clippy pipeline.
- Live QEMU Guest Agent and libvirt validation requires the RHEL host and
  Windows guest described in `docs/qemu-ga-setup.md`.
