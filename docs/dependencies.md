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
| Rust serialization | `serde` with `derive`, `serde_json` | Locked in the workspace `Cargo.lock` | Parse QGA JSON and live QMP compatibility responses |
| Rust errors | `thiserror`, `anyhow` | Locked in `windows/Cargo.lock` | Typed and contextual errors |
| Rust telemetry | `tracing`, `tracing-subscriber` | Locked in `windows/Cargo.lock` | Service logging foundation |
| Windows API | `winapi` features: `processthreadsapi`, `winbase`, `sysinfoapi`, `winnt` | Locked in `windows/Cargo.lock` | Windows process, service, and memory APIs |
| Rust tests | `mockall` | Locked in `windows/Cargo.lock` | Test doubles for future adapters |
| Host OS | RHEL host with libvirt and QEMU | RHEL 10 expected by setup guide | Run VM and live virtio-mem checks |
| Host controller | Rust 1.70+, `systemd`, and `virsh` | RHEL host | Run one Rust controller instance per configured VM/device alias |
| Host CLI | `virsh` | From libvirt client | Query QGA and inspect/update VM state |
| JSON validation | `jq` | Current distribution package | Validate QGA responses in Bash |
| Host shell | Bash | 4.0+ | Run repository scripts |
| Guest OS | Windows 11 x64 under QEMU/KVM | Required | Run the service and QEMU Guest Agent |
| Guest agent | QEMU Guest Agent x64 | Installed and running; observed `110.0.2` on `win11_gpu` | Provide `guest-info`; `guest-get-memory-stats` requires QGA built from upstream QEMU 9.1+ and is not implemented in the observed build |
| Guest channel | Virtio-serial channel `org.qemu.guest_agent.0` | Required | Connect libvirt/QEMU to QGA |
| Guest device | Configured virtio-mem device and known alias | Required for resize tests | Exercise requested/current memory convergence |
| Host virtualization stack | libvirt, QEMU API, hypervisor | Observed `11.10.0` (libvirt), `11.10.0` (QEMU API), `10.1.0` (hypervisor) on the RHEL host | Reported by `virsh version`/`guest-info` during the 2026-08-18 probe |

## Remote Windows build endpoint

The RHEL host is the VS Code control plane, not the Windows linker host. The
Windows service must be built natively in the Windows KVM guest because the
crate targets `x86_64-pc-windows-msvc` and uses Windows APIs. The supported
workflow uses OpenSSH from RHEL to the guest and the checked-in
`scripts/windows-remote-build.sh` wrapper.

The one-time Windows endpoint setup requires:

- Windows 11 x64 with a dedicated build account;
- OpenSSH Server and key-based access from the RHEL development account;
- Rust stable with the MSVC target, Cargo, rustfmt, and Clippy;
- Visual Studio C++ Build Tools with the MSVC x64 workload and Windows SDK;
- Git, `tar.exe`, and `certutil.exe`.

The wrapper requires `VIRTIO_MEM_WINDOWS_SSH`, an SSH config alias. It accepts
`VIRTIO_MEM_WINDOWS_DIR` for the remote workspace and
`VIRTIO_MEM_WINDOWS_ARTIFACTS` for local artifact staging. Do not put private
keys, passwords, or endpoint-specific credentials in the repository or task
definitions. The wrapper transfers Git-tracked and non-ignored working-tree
files, runs locked Cargo commands on Windows, and verifies the downloaded
executable with SHA-256. It does not install the service, edit
ProgramData, change SCM state, call `virsh update-memory-device`, or mutate
the KVM guest.

For pinned, non-interactive milestone runs, the wrapper also accepts
`VIRTIO_MEM_WINDOWS_KNOWN_HOSTS_FILE` and
`VIRTIO_MEM_WINDOWS_IDENTITY_FILE`. The checked-in
`scripts/complete-windows-build-milestone.sh` helper sets these from a verified
temporary host-key file and an optional operator-owned private-key path; it
does not copy either credential into the repository.

On `ice101.lan`, rustup's `.cargo\bin` proxy symlinks return Windows error 448
through OpenSSH. The wrapper avoids changing the guest toolchain by resolving
the active toolchain with `rustup which cargo` and invoking the real Cargo,
rustc, rustdoc, rustfmt, and Clippy executables directly.

## Rust project dependencies

The authoritative workspace manifest is [`../Cargo.toml`](../Cargo.toml). The
package manifests are [`../windows/Cargo.toml`](../windows/Cargo.toml),
[`../host/Cargo.toml`](../host/Cargo.toml), and
[`../crates/virtio-mem-core/Cargo.toml`](../crates/virtio-mem-core/Cargo.toml).
The workspace lockfile [`../Cargo.lock`](../Cargo.lock) records resolved
versions and must be retained for reproducible builds.

### Runtime dependencies

- `serde` with the `derive` feature: deserializes QGA memory-stat responses.
- `serde_json`: parses QGA JSON and host-side live QMP compatibility responses.
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

1. `cargo fmt --all -- --check`;
2. a locked release build of `virtio-mem-core` and `virtio-mem-host`;
3. locked tests for those RHEL-compatible packages;
4. warnings-as-errors Clippy for those packages; and
5. Bash syntax validation for every repository script.

The Windows service is intentionally excluded from this native RHEL command
because its SCM adapter requires Windows APIs. The remote native Windows gate
tests that crate and its shared-core dependency without requiring a live VM.

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

The helper checks `guest-info`, then uses `guest-get-memory-stats` when the
agent provides it or validates `dommemstat` three times as the documented
fallback. It does not resize memory, restart the VM, or execute commands
inside the guest.

On `win11_gpu` (QGA `110.0.2`), `guest-get-memory-stats` returns "command has
not been found"; upgrading the guest's `qemu-guest-agent` build to one
compiled from upstream QEMU 9.1+ (for example, a newer `virtio-win` package)
is required for that command to work. The host controller does not depend on
it: `VIRTIO_MEM_STATS_SOURCE` defaults to `dommemstat`, which reads `virsh
dommemstat` and has verified `actual`/`unused`/`available` fields on this
guest. See `docs/issues.md` (ISSUE-001) and `host/src/config.rs`.

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

## QEMU and libvirt operational constraints

The official virtio-mem guidance adds several operational constraints that affect both design and validation:

- `requested-size` must be an integer multiple of the device's `block-size` and cannot exceed the device's maximum size.
- `block-size` is the hotplug granularity and should typically be at least the guest's THP size; a 2 MiB block is the common default for x86 systems.

- The guest can fail to fulfill a shrink request if it cannot free or hotunplug memory reliably; a request can therefore succeed at the host while the guest remains below the target for a time.
- QEMU does not currently provide the same protection for unplugged memory that virtio-balloon does; operators should use cgroups or other host-side limits to avoid memory overcommit.
- `dynamic-memslots=on` is recommended where supported because it reduces metadata and can make unplugged memory inaccessible, but it must be used with `unplugged-inaccessible=on`.
- Some workloads or devices remain incompatible with virtio-mem, including `vdpa`, `RDMA migration`, `vfio-nvme`, `mlock`-based usage, and several vhost-user devices such as DPDK/SPDK.
- The memory backend should generally use sparse storage semantics: `reserve=off` and `prealloc=off` for virtio-mem backends, while the virtio-mem device itself may use `prealloc=on` when appropriate.

The Rust Windows crate also uses `quick-xml` for pure parsing of captured
libvirt snapshots. This parser does not invoke `virsh`, libvirt, or Linux
commands; live discovery remains outside the guest service boundary.

The Rust host CLI requires `virsh` for explicitly scoped live XML reads and
approved updates. Its XML selection, unit conversion, compatibility checks,
and resize policy are implemented in Rust and do not require `xmllint`.
Separate legacy diagnostic harnesses such as `preview-memory-decision.sh` and
`live-resize-test.sh` still require the tools they check at startup; they are
not the authoritative host actuation interface.

## RHEL host-controller deployment

The `host/` crate provides both the systemd controller and the authoritative
explicit host CLI. Each templated systemd instance manages one VM and one
virtio-mem alias; it does not discover domains broadly.

Run the native RHEL gate before installation:

```bash
bash scripts/build-rust.sh
```

Install the release binary at `/usr/local/libexec/virtio-mem-host`, the unit at
`/etc/systemd/system/virtio-mem-host@.service`, and a non-secret instance file
derived from `host/systemd/virtio-mem-host.conf.example` at
`/etc/virtio-mem-host/INSTANCE.conf`. The `virtio-mem-host` service account
must be a non-login account with only the libvirt authorization required for
the explicitly configured VM. Verify that access before enabling the unit; do
not silently change it to run as root.

These constraints are not optional recommendations for a future improvement; they directly affect whether the guest can accept memory changes safely and whether the host-side controller can make valid policy choices.

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
- Windows OpenSSH Server is enabled on the private KVM interface with
  key-based build access; two fingerprint-pinned RHEL-originated aggregate
  build gates completed successfully. Migration to a dedicated least-privilege
  build identity remains operational hardening.
- WSL has the native compiler/linker development packages installed. The
  Windows-native toolchain is preferred for validating this Windows service.
- QGA and live virtio-mem checks still require the RHEL/libvirt host and
  Windows guest described above.
- As of 2026-08-18, `win11_gpu` reports libvirt `11.10.0`, QEMU API `11.10.0`,
  hypervisor `10.1.0`, and QGA `110.0.2`; `guest-info` succeeds repeatedly but
  `guest-get-memory-stats` is unimplemented on this QGA build, so `dommemstat`
  remains the verified default stats source.

## Related documentation

- [`readme.md`](../readme.md) — project overview and quick prerequisites.
- [`windows/README.md`](../windows/README.md) — Rust service-specific setup.
- [`qemu-ga-setup.md`](qemu-ga-setup.md) — guest-agent installation and channel setup.
- [`testing.md`](testing.md) — local and live validation strategy.
- [`engineering-standards.md`](engineering-standards.md) — toolchain and language policy.
