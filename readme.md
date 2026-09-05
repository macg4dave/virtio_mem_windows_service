# virtio-mem Windows Service

> A safety-first Rust foundation for observing Windows guest memory demand and
> coordinating bounded virtio-mem changes through a Linux host controller.

**Current phase:** Phase 2 — Core Functionality
**Project status:** Single-VM host actuation and service lifecycles are live
validated; trustworthy Windows demand publication and recovery hardening remain.

---

## What this project is for

This project explores predictable memory coordination for a Windows 11 guest
running on QEMU/KVM with a `virtio-mem` device.

The long-term goal is a system that can:

- observe Windows memory pressure using native telemetry;
- produce versioned, canonical-byte demand recommendations;
- validate virtio-mem state and alignment before any change;
- grow or reclaim memory in bounded, convergent steps; and
- coordinate multiple guests without allowing one guest to consume the host's
  safety reserve.

The project deliberately separates **measurement**, **policy**, and
**actuation**. The Windows service reports demand. The host-side controller is
the resize authority.

## Current status

The following capabilities are implemented and locally tested:

- shared Rust memory policy with bounds, alignment, hysteresis, and
  `requested != current` convergence protection;
- Windows QEMU Guest Agent named-pipe boundary and response parser;
- wakeable polling and portable service lifecycle state machine;
- validated versioned JSON configuration;
- Windows SCM dispatcher and local service registration commands;
- native `GlobalMemoryStatusEx` and `GetPerformanceInfo` telemetry;
- versioned advisory demand reports with aligned targets and safe floors;
- append-only JSON-lines report output and a stoppable demand worker;
- Rust host controller with bounded `virsh` adapters, XML validation,
  `dommemstat` fallback, and host/device headroom gates.

The latest platform-specific gates pass 22 shared-core tests, 29 RHEL-host
tests, and 64 native-Windows tests. These are separate supported-platform
results, not one cross-platform workspace run. Release builds, formatting,
Clippy warnings-as-errors, and Bash syntax validation also pass in their
documented gates.

### Important limitations

- The Windows entry point collects native telemetry but does not yet publish
  demand reports. Current-allocation ownership must be decided before the
  existing calculator can be wired without guessing.
- Demand report version 1 has no freshness, VM/session identity, sequence, or
  allocation-provenance envelope, and its JSON-lines sink has no retention or
  rotation policy.
- Windows SCM lifecycle and the first bounded recovery restart are
  live-verified under `LocalService`; ProgramData ACL, formatted Event Log
  message-resource packaging, and workload validation remain open.
- The connected guest's QGA does not provide `guest-get-memory-stats`; the
  `dommemstat` fallback is live verified and is the host default.
- Live resize remains subject to fresh XML validation and the
  `requested == current` convergence gate before every request.
- The installed single-VM controller is active. Its workload compatibility
  approval is currently a static boolean and must be bound to a live
  configuration fingerprint before configuration drift can be trusted.

See the [roadmap](docs/roadmap.md) for milestone status and exit gates.

## Architecture

```text
┌──────────────────────── Windows 11 guest ────────────────────────┐
│                                                                   │
│  Native memory telemetry ──► Advisory demand report               │
│  Windows service             (Rust, canonical bytes)               │
└───────────────────────────────────────────────────────────────────┘
                             │ future versioned report transport
                             ▼
┌──────────────────────────── RHEL host ────────────────────────────────┐
│  Rust host controller                                                 │
│    ├─ observes host-side QGA health or dommemstat                     │
│    ├─ validates live virtio-mem XML                                   │
│    ├─ checks host headroom                                             │
│    └─ issues one aligned request and waits for convergence             │
│                                                                        │
│  Temporary Bash wrappers: explicit inspection and guarded test flows     │
└────────────────────────────────────────────────────────────────────────┘
```

### Safety boundaries

- Windows code does not invoke Linux commands, `virsh`, or libvirt.
- Host automation is explicit-scope and read-only unless a live action is
  deliberately approved.
- The controller never sends a follow-up resize while `requested` and
  `current` differ.
- Memory values cross internal boundaries as checked `u64` byte counts.
- Live resize tests are opt-in, bounded, aligned, and reversible by default.
- Direct `viomem.sys` user-mode control remains deferred until a supported
  interface is proven.
- Host snapshot, validation, dry-run, and explicitly applied resize policy is
  owned by the Rust host CLI; the duplicate Bash resize helper has been
  removed.

## Quick start

### 1. Check prerequisites

Review the [dependency matrix](docs/dependencies.md), then run the local
environment check from a Bash-capable host:

```bash
bash scripts/check-environment.sh
```

### 2. Run the local quality gate

```bash
cargo fmt --all -- --check
cargo build -p virtio-mem-core -p virtio-mem-host --all-features --release --locked
cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked
cargo clippy -p virtio-mem-core -p virtio-mem-host --all-targets --all-features --locked -- -D warnings
bash -n scripts/*.sh
```

These checks are hermetic and do not require a live VM. The Windows-only crate
is compiled, tested, and linted by the native Windows gate below.

### 3. Build the Windows service from this RHEL VS Code workspace

The Windows service uses the MSVC target and must be linked by Windows. After
one-time OpenSSH, Rust MSVC, and Visual Studio Build Tools setup in the Windows
KVM guest, run the `Build: all non-mutating gates` task and enter its requested
SSH config alias. The task synchronizes source, builds and tests on Windows,
and stages a checksum-verified
`.vscode-artifacts/windows/virtio-mem-service.exe` on RHEL.

The VS Code task prompts for the SSH alias. For direct terminal use, run
`VIRTIO_MEM_WINDOWS_SSH=ALIAS bash scripts/windows-remote-build.sh all`.

This task does not install or start the service and does not change libvirt,
systemd, QEMU, or guest memory. See [`docs/testing.md`](docs/testing.md) and
[`windows/README.md`](windows/README.md) for endpoint prerequisites.

### 4. Validate a guest-agent connection

On the approved RHEL/libvirt host, use an explicit VM name:

```bash
bash scripts/validate-guest-agent.sh VM_NAME 3
```

Read the [QEMU Guest Agent setup guide](docs/qemu-ga-setup.md) first. The
current guest may report that `guest-get-memory-stats` is unavailable; the host
controller uses the verified `dommemstat` fallback rather than guessing.

### 5. Preview before changing memory

Use the read-only decision preview with the approved host configuration:

```bash
bash scripts/preview-memory-decision.sh VM_NAME VIRTIO_MEM_ALIAS
```

For a live resize, follow the approval and rollback procedure in
[`docs/testing.md`](docs/testing.md). Never add `--apply` casually.

## Repository guide

| Document | Purpose |
| --- | --- |
| [`docs/roadmap.md`](docs/roadmap.md) | Milestones, gates, dependencies, and blockers |
| [`BACKLOG.md`](BACKLOG.md) | Execution source of truth and handoffs |
| [`PROJECT_STATUS.md`](PROJECT_STATUS.md) | Current implementation snapshot |
| [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) | Dependency-ordered implementation plan |
| [`docs/architecture.md`](docs/architecture.md) | Component ownership and boundaries |
| [`docs/future-architecture.md`](docs/future-architecture.md) | Phase 3 global-controller design |
| [`docs/api-contract.md`](docs/api-contract.md) | QGA, demand-report, and resize contracts |
| [`docs/data-model.md`](docs/data-model.md) | Memory state and policy data model |
| [`docs/feature-matrix.md`](docs/feature-matrix.md) | Feature status by component |
| [`docs/testing.md`](docs/testing.md) | Local, host, guest, and live validation procedures |
| [`docs/issues.md`](docs/issues.md) | Known incidents and unresolved issues |
| [`docs/engineering-standards.md`](docs/engineering-standards.md) | Coding and safety standards |

## Repository layout

```text
crates/virtio-mem-core/   Shared byte-based policy and XML/state contracts
windows/                  Windows service, telemetry, SCM, and QGA boundary
host/                     RHEL host controller and bounded libvirt adapters
scripts/                 Bash validation and guarded operational helpers
docs/                    Architecture, contracts, testing, and roadmap
host/systemd/            Example host service configuration
```

## Project principles

1. **Safety before automation:** fail closed on missing, stale, or inconsistent
   state.
2. **Small reversible changes:** resize one aligned step, wait for convergence,
   and preserve rollback evidence.
3. **Clear authority:** Windows measures; the host controller acts.
4. **Hermetic first:** prove policy and failure behavior locally before using a
   live VM.
5. **Rust and Bash only:** Rust owns service logic; Bash owns validation and
   operational helpers.

## Contributing

Before changing code, read the repository rules in
`.github/copilot-instructions.md` and the architecture/testing documents.
Keep changes traceable to the backlog and update affected documentation in the
same session. Do not commit credentials, private keys, production data, or
unapproved live-environment changes.
<!-- End of README. -->
