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
│   ├── service.rs
│   ├── metrics.rs
│   ├── guest_agent.rs
│   └── lib.rs
├── tests/
│   ├── metrics_tests.rs
│   └── guest_agent_tests.rs
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

Requires:
- Rust 1.70+
- Windows 11 with QEMU Guest Agent running
- Windows service APIs available in the target environment

See [../../BACKLOG.md](../../BACKLOG.md) for task assignments and [../../docs/architecture.md](../../docs/architecture.md) for design details.
