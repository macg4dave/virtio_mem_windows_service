# Project Dependencies and Requirements

This document is the canonical dependency checklist for the virtio-mem Windows
service. It covers local development, the Windows guest runtime, and the RHEL
host validation environment.

## Scope and language requirements

- Runtime and service logic: Rust, edition 2021.
- Automation and validation: Bash 4.0+.
- Forbidden project languages and build flows: Go, C#, PowerShell, Python,
  Java, and other languages.
- Do not commit credentials, private keys, tokens, production data, or VM
  secrets.

## Dependency summary

| Area | Required dependencies | Minimum or expected version | Used for |
| --- | --- | --- | --- |
| Rust development | `rustc`, `cargo`, `rustfmt`, `clippy` | Rust 1.70+; edition 2021 | Build, test, format, and lint the service |
| Rust target | Windows x64 target/toolchain | Windows 11 target | Build the guest service |
| Rust serialization | `serde` with `derive`, `serde_json` | Locked in `windows/Cargo.lock` | Parse QEMU Guest Agent JSON |
| Rust errors | `thiserror`, `anyhow` | Locked in `windows/Cargo.lock` | Typed and contextual errors |
| Rust telemetry | `tracing`, `tracing-subscriber` | Locked in `windows/Cargo.lock` | Service logging foundation |
| Windows API | `winapi` features: `processthreadsapi`, `winbase`, `sysinfoapi`, `winnt` | Locked in `windows/Cargo.lock` | Windows process, service, and memory APIs |
| Rust tests | `mockall` | Locked in `windows/Cargo.lock` | Test doubles for future adapters |
| Host OS | RHEL host with libvirt and QEMU | RHEL 10 expected by setup guide | Run VM and live virtio-mem checks |
| Host CLI | `virsh` | From libvirt client | Query QGA and inspect/update VM state |
| JSON validation | `jq` | Current distribution package | Validate QGA responses in Bash |
| Host shell | Bash | 4.0+ | Run repository scripts |
| Guest OS | Windows 11 x64 under QEMU/KVM | Required | Run the service and QEMU Guest Agent |
| Guest agent | QEMU Guest Agent x64 | Installed and running | Provide `guest-info` and `guest-get-memory-stats` |
| Guest channel | Virtio-serial channel `org.qemu.guest_agent.0` | Required | Connect libvirt/QEMU to QGA |
| Guest device | Configured virtio-mem device and known alias | Required for resize tests | Exercise requested/current memory convergence |

## Rust project dependencies

The authoritative manifest is [`../windows/Cargo.toml`](../windows/Cargo.toml).
The lockfile [`../windows/Cargo.lock`](../windows/Cargo.lock) records resolved
versions and must be retained for reproducible builds.

### Runtime dependencies

- `serde` with the `derive` feature: deserializes QGA memory-stat responses.
- `serde_json`: parses the QGA JSON envelope and stat entries.
- `thiserror`: defines typed parser and controller-policy errors.
- `anyhow`: available for application-level contextual errors.
- `tracing`: provides structured event and metric logging.
- `tracing-subscriber`: provides log subscriber configuration.
- `winapi` with `processthreadsapi`, `winbase`, `sysinfoapi`, and `winnt`:
  provides the planned Windows service and memory API surface.

### Development dependencies

- `mockall`: provides mocks for QGA and host-side adapters as those interfaces
  are introduced.

Dependencies are fetched from crates.io by Cargo. No API keys or environment
secrets are required for the current codebase.

## Local Rust setup

Install a complete Rust toolchain for the Windows target, including Cargo,
`rustfmt`, and Clippy. Verify it with:

```bash
rustc --version
cargo --version
rustup component list --installed
```

From the repository root, validate the service with:

```bash
bash scripts/build-rust.sh
```

The script runs:

1. `cargo fmt --all -- --check`
2. `cargo build --release`
3. `cargo test`
4. `cargo clippy --all-targets --all-features -- -D warnings`

The service does not require a live VM for parser and controller-policy unit
tests. A complete native linker/toolchain is required for the full build.

## RHEL host setup

The host must provide libvirt, QEMU, and the command-line clients used by the
validation helpers. Verify the required commands with:

```bash
bash scripts/check-environment.sh
virsh version
qemu-system-x86_64 --version
jq --version
```

On RHEL, the QEMU Guest Agent and VirtIO Windows media are normally obtained
through the `virtio-win` package:

```bash
rpm -q virtio-win
rpm -ql virtio-win
```

If the package is not installed, follow the approved host change process before
installing it. The setup guide documents the expected ISO location and guest
channel configuration: [`qemu-ga-setup.md`](qemu-ga-setup.md).

## Windows guest setup

The guest must have:

- Windows 11 x64 running under QEMU/KVM.
- QEMU Guest Agent installed as a Windows service and running.
- A VirtIO serial device connected to the QGA channel
  `org.qemu.guest_agent.0`.
- A configured virtio-mem device for live memory validation.
- Administrator access during initial driver, agent, and channel setup.

Validate the guest-agent path from the RHEL host with an explicit VM name:

```bash
bash scripts/validate-guest-agent.sh VM_NAME 3
```

The helper checks `guest-info` once and
`guest-get-memory-stats` three times by default. It does not resize memory,
restart the VM, or execute commands inside the guest.

## Live virtio-mem requirements

Resize validation additionally requires:

- The virtio-mem device alias from the live domain XML.
- The device block size.
- Live `requested` and `current` values.
- A reversible, aligned test target within configured minimum and maximum
  limits.
- Permission to inspect the domain XML and issue an explicitly approved live
  update.

The controller policy refuses another request while `requested` and `current`
differ and clamps all targets to safe aligned limits. See
[`api-contract.md`](api-contract.md) and [`data-model.md`](data-model.md).

## Validation matrix

| Validation | Dependencies | Live VM required |
| --- | --- | --- |
| Rust formatting | Rust toolchain and `rustfmt` | No |
| Rust compile | Rust toolchain, Windows target, native linker | No |
| Rust unit tests | Rust toolchain and test dependencies | No |
| Rust Clippy | Rust toolchain and `clippy` | No |
| Bash syntax | Bash | No |
| Host prerequisite check | Bash, `virsh`, `jq` | No, but host tools must exist |
| QGA probe | Bash, `virsh`, `jq`, running QGA | Yes |
| Virtio-mem resize validation | QGA, libvirt, QEMU, virtio-mem alias/device | Yes |
| End-to-end integration | All host and guest dependencies | Yes |

## Environment status

- Windows Rust 1.97.1 MSVC toolchain is installed and the full format, release
  build, test, and Clippy pipeline passes locally.
- WSL has the native compiler/linker development packages installed. The
  Windows-native toolchain is preferred for validating this Windows service.
- QGA and live virtio-mem checks still require the RHEL/libvirt host and
  Windows guest described above.

## Related documentation

- [`readme.md`](../readme.md) — project overview and quick prerequisites.
- [`windows/README.md`](../windows/README.md) — Rust service-specific setup.
- [`qemu-ga-setup.md`](qemu-ga-setup.md) — guest-agent installation and channel setup.
- [`testing.md`](testing.md) — local and live validation strategy.
- [`engineering-standards.md`](engineering-standards.md) — toolchain and language policy.
