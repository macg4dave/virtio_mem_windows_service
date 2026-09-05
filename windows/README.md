# Windows Service

Rust service that collects native Windows memory telemetry and will publish
advisory demand reports. It does not own host actuation or the QGA channel.

## Project Rules

This service must use Rust only. No C# or PowerShell code is allowed in this repository.

## Structure

```text
windows/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── controller.rs
│   ├── config.rs
│   ├── demand.rs
│   ├── error.rs
│   ├── qga.rs
│   ├── runtime.rs
│   ├── service_host.rs
│   ├── service_loop.rs
│   ├── stats.rs
│   └── lib.rs
└── README.md
```

## Build

```bash
cargo build --release
```

Build this crate on Windows (or a host with an installed Windows Rust target
and compatible linker). The RHEL development host does not currently contain
the Windows target standard library or a Windows linker, so it cannot produce
the service executable. The QGA client remains an explicit adapter boundary,
but the Windows service does not open the QGA virtio-serial device because
`QEMU-GA` owns it. Service startup uses native Windows telemetry; host-side QGA
requests remain a RHEL/libvirt responsibility.

### Build from VS Code on RHEL

The supported RHEL workflow builds this crate natively in the Windows KVM
guest. Configure an SSH alias and workspace path in the environment, then run
the checked-in VS Code tasks or the Bash wrapper directly:

- `VIRTIO_MEM_WINDOWS_SSH` — required SSH config alias for the Windows guest.
- `VIRTIO_MEM_WINDOWS_DIR` — optional remote path; defaults to
  `C:\Users\Public\virtio-mem-build`.
- `VIRTIO_MEM_WINDOWS_ARTIFACTS` — optional local staging path; defaults to
  `.vscode-artifacts/windows`.
- `VIRTIO_MEM_WINDOWS_KNOWN_HOSTS_FILE` — optional pinned SSH host-key file.
- `VIRTIO_MEM_WINDOWS_IDENTITY_FILE` — optional private-key path for
  non-interactive authentication.

The one-time guest setup requires Rust MSVC, Visual Studio C++ Build Tools with
the Windows SDK, Git, `tar.exe`, `certutil.exe`, and OpenSSH Server. The
wrapper synchronizes Git-tracked and non-ignored working-tree files,
initializes the MSVC environment, runs Cargo on Windows with the workspace
lockfile, requires a successful release build before fetching, and verifies the
fetched executable's SHA-256 checksum. The
VS Code tasks prompt for the SSH alias; the environment variable is required
only for direct wrapper use. It never installs or starts the Windows service
and never changes libvirt state.

For the two-run milestone check, use
`scripts/complete-windows-build-milestone.sh` from the RHEL checkout as
documented in `docs/testing.md`. It verifies a supplied ED25519 host
fingerprint without changing the operator's persistent SSH configuration.

See [`../docs/testing.md`](../docs/testing.md) for the task sequence and the
separate approval gate for deployment.

## Test

```bash
cargo test
```

## Lint

```bash
cargo clippy
cargo fmt --check
```

## Development

See [`../docs/dependencies.md`](../docs/dependencies.md) for the complete
toolchain, crate, host, guest, and validation requirements. The minimum local
runtime requirements are:

- Rust 1.70+
- Windows 11; QEMU Guest Agent is required for host-side health operations,
  not for native service telemetry
- Windows service APIs available in the target environment

The current runtime foundation includes a configurable QEMU Guest Agent client
boundary for explicit adapter/testing use, a mockable memory poller, and an
adapter-based single poll iteration. The Windows SCM and interactive workers
use native `GlobalMemoryStatusEx`/`GetPerformanceInfo` telemetry rather than
opening the QGA virtio-serial device, which is owned by the QEMU Guest Agent
service. It also includes a stoppable polling scheduler, portable
`ServiceHost` lifecycle wrapper, and a native SCM dispatcher that shares the
same cancellation signal as the worker. `ServiceConfig` supplies validated
service identity, legacy adapter endpoint, timing, least-privilege defaults, and
versioned JSON
loading from `C:\ProgramData\VirtioMemService\config.json`; ACL provisioning
and live KVM channel validation are not implemented yet. The demand
agent foundation additionally collects native Windows memory counters through
`GlobalMemoryStatusEx` and `GetPerformanceInfo`, validates canonical-byte
snapshots, and emits a versioned advisory demand report without issuing a
resize. `DemandAgent` exposes a testable one-cycle collection/publication
boundary; wiring its persistent report sink into the SCM worker is still
pending. SCM lifecycle and failure events are separately emitted to the
Windows Application Event Log with stable IDs and bounded messages. The
generic `DemandServiceWorker` and JSON-lines publisher are
available, but the SCM entry point currently runs `NativeTelemetryWorker` and
discards each validated sample. Before publication is wired, the architecture
must decide whether the host joins authoritative libvirt allocation with raw
guest telemetry or supplies a validated allocation feed. Report freshness,
identity, retention, and ACL requirements are also still open.

## Service hosting rules

The SCM adapter keeps service callbacks bounded and delegates polling to the
stoppable runtime. It must report lifecycle transitions in the
order **start-pending → running → stop-pending → stopped**, distinguish normal
cancellation from failure, and preserve unexpected worker failures as
non-zero process exits so SCM recovery can act. Service registration must use a
stable identity, documented configuration, and a least-privileged account
that can call the required native telemetry and Event Log APIs and write only
to its approved ProgramData paths.

A worker exit without a stop/shutdown request is also treated as an unexpected
non-zero failure. Only successful completion after cancellation is reported as
a normal stop. The elevated lifecycle and first 5-second recovery restart are
live-verified. Query XML `EventData` for the bounded insertion strings; classic
formatted descriptions remain pending message-resource packaging.

The required operational verification sequence is **install → start → inspect
logs → stop → remove**. The executable exposes matching `install`, `start`,
`stop`, and `remove` commands; each requires an elevated terminal when SCM
permissions require it. See [`../docs/architecture.md`](../docs/architecture.md)
and [`../docs/testing.md`](../docs/testing.md) for the lifecycle contract and
test matrix.

See [../BACKLOG.md](../BACKLOG.md) for task assignments and
[../docs/architecture.md](../docs/architecture.md) for design details.
