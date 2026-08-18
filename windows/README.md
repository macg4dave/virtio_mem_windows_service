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
service identity, endpoint, timing, and least-privilege defaults; persistent
loading and live KVM channel validation are not implemented yet.

## Service hosting rules

The eventual SCM adapter must keep service callbacks bounded and delegate
polling to the stoppable runtime. It must report lifecycle transitions in the
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
