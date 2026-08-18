# Windows Service

Rust service that exposes Windows memory metrics via QEMU Guest Agent.

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
the service executable. The service already sends the QGA
`guest-get-memory-stats` request; the connected guest agent must advertise
that command for runtime collection to succeed.

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
- Windows 11 with QEMU Guest Agent running
- Windows service APIs available in the target environment

The current runtime foundation includes a configurable named-pipe QEMU Guest
Agent client, a mockable memory poller, and an adapter-based single poll
iteration. It also includes a stoppable polling scheduler, portable
`ServiceHost` lifecycle wrapper, and a native SCM dispatcher that shares the
same cancellation signal as the worker. `ServiceConfig` supplies validated
service identity, endpoint, timing, least-privilege defaults, and versioned JSON
loading from `C:\ProgramData\VirtioMemService\config.json`; ACL provisioning
and live KVM channel validation are not implemented yet. The demand
agent foundation additionally collects native Windows memory counters through
`GlobalMemoryStatusEx` and `GetPerformanceInfo`, validates canonical-byte
snapshots, and emits a versioned advisory demand report without issuing a
resize. `DemandAgent` exposes a testable one-cycle collection/publication
boundary; wiring a persistent or event-log report sink into the SCM worker is
still pending. The generic `DemandServiceWorker` and JSON-lines publisher are
available, but the SCM entry point intentionally waits for a validated
current-allocation provider rather than guessing from QGA totals or limits.

## Service hosting rules

The SCM adapter keeps service callbacks bounded and delegates polling to the
stoppable runtime. It must report lifecycle transitions in the
order **start-pending → running → stop-pending → stopped**, distinguish normal
cancellation from failure, and preserve unexpected worker failures as
non-zero process exits so SCM recovery can act. Service registration must use a
stable identity, documented configuration, and the least-privileged account
that can access the QEMU Guest Agent channel.

The required operational verification sequence is **install → start → inspect
logs → stop → remove**. The executable exposes matching `install`, `start`,
`stop`, and `remove` commands; each requires an elevated terminal when SCM
permissions require it. See [`../docs/architecture.md`](../docs/architecture.md)
and [`../docs/testing.md`](../docs/testing.md) for the lifecycle contract and
test matrix.

See [../BACKLOG.md](../BACKLOG.md) for task assignments and
[../docs/architecture.md](../docs/architecture.md) for design details.
