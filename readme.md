# virtio-mem Windows Service

> A safety-first Rust foundation for observing Windows guest memory demand and
> coordinating bounded virtio-mem changes through a Linux host controller.

**Current phase:** Phase 2 — Core Functionality
**Project status:** Single-VM host actuation and service lifecycles are live
validated; the host-side demand join is implemented, while bounded delivery
and recovery hardening remain.

---

## What this project is for

This project explores predictable memory coordination for a Windows 11 guest
running on QEMU/KVM with a `virtio-mem` device.

The currently validated `win11_gpu` deployment is a fully trusted,
development/test-only KVM guest. Upstream classifies Windows virtio-mem as
unstable technology preview, so this repository does not claim production or
untrusted-guest support.

The long-term goal is a system that can:

- observe Windows memory pressure using native telemetry;
- produce versioned, canonical-byte demand recommendations;
- validate virtio-mem state and alignment before any change;
- calculate a durable absolute target from fresh Windows demand and move
  toward it with bounded, block-aligned 1 GiB growth or 64 MiB reclaim
  actuation; and
- coordinate multiple guests without allowing one guest to consume the host's
  safety reserve.

The project deliberately separates **measurement**, **policy**, and
**actuation**. The Windows service reports demand. The host-side controller is
the resize authority.

## Current status

The following capabilities are implemented and locally tested:

- shared Rust memory policy with bounds, alignment, hysteresis,
  `requested != current` convergence protection, 1 GiB growth quanta, and
  64 MiB reclaim quanta;
- Windows QEMU Guest Agent named-pipe boundary and response parser;
- wakeable polling and portable service lifecycle state machine;
- validated versioned JSON configuration;
- Windows SCM dispatcher and local service registration commands;
- native `GlobalMemoryStatusEx` and `GetPerformanceInfo` telemetry;
- VM-scoped, wall-clock-stamped raw telemetry publication from production
  Windows workers and host-side validation/join with live libvirt `current`;
- versioned advisory demand reports with aligned targets and safe floors;
- append-only JSON-lines report output and a stoppable demand worker;
- Rust host controller with bounded `virsh` adapters, XML validation,
  `dommemstat` fallback, and host/device headroom gates.

The latest gates pass 46 shared-core, 52 host, and 67 native-Windows tests.
These are separate supported-platform results, not one cross-platform
workspace run. Release builds, formatting, Clippy warnings-as-errors, and Bash
syntax validation pass.

### Important limitations

- Windows publishes raw telemetry only; the host owns target calculation after
  joining the record with alias-scoped live libvirt `current`. The M10d v2
  envelope now has VM/service/session identity, millisecond wall/monotonic
  ordering, sequence, provenance, bounded atomic handoff/retention, and durable
  restart-safe replay checks. ProgramData ACL provisioning is implemented but
  still needs installed-guest verification.
- Windows SCM lifecycle and the first bounded recovery restart are
  live-verified under `LocalService`; ProgramData ACL, formatted Event Log
  message-resource packaging, and workload validation remain open.
- `guest-get-memory-stats` is not an upstream QGA command. The connected QGA
  does not provide it; the adapter is retained only for a separately validated
  custom/downstream agent. The host default `dommemstat` source now preserves
  balloon semantics and rejects missing, stale, future, or non-advancing
  `last-update` evidence.
- Live resize remains subject to fresh XML validation and the
  `requested == current` convergence gate before every request.
- The signed Windows driver has no supported user-mode diagnostic state query.
  Optional bounded kernel tracing may help explain notification or shrink
  failures, but it is not an allocation source or a prerequisite for host
  accounting.
- The host controller now requires a version-1 SHA-256 compatibility
  attestation. Before every resize it checks the protected review document and
  recollects allocation-neutral domain XML/QEMU argv, alias-scoped QMP
  properties, and QEMU/libvirt versions. Backend, slot/VFIO, incompatible
  workload/device, balloon, topology, trust, and driver/stack drift blocks
  actuation.
- Windows shrink behavior is size-sensitive: the live 256 MiB ramp reclaimed
  about 2.93 GiB before stalling, while a one-device-block request made no
  progress. A live 64 MiB request also made no progress for 300 seconds and
  latched cleanly. Automatic reclaim now defaults enabled because it is a core
  product capability; freshness, safe floors, bounded 64 MiB requests,
  convergence, and ambiguity/stall latching prevent blind repeated reclaim.
- M10e-M10g will replace the per-poll directional-step policy with an absolute
  desired target and explicit `desired`/`requested`/`current` reconciliation.
  Fixed no-progress deadlines remain health and recovery bounds; they will not
  decide how much memory Windows needs.
- Phase 2 supports one active controller for one explicitly named VM/device on
  this development host. Multi-controller/device actuation waits for M11
  global arbitration.
- Because `win11_gpu` is fully trusted, a hard QEMU/libvirt cgroup memory limit
  is recommended defense-in-depth here. It is mandatory for any future
  production or untrusted guest because QEMU does not fully protect unplugged
  memory from guest access.

See the [roadmap](docs/roadmap.md) for milestone status and exit gates and the
[target-controller contract](docs/target-controller.md) for normative
M10e-M10g policy.

## Architecture

```text
┌──────────────────────── Windows 11 guest ────────────────────────┐
│                                                                   │
│  Native memory telemetry ──► Versioned raw telemetry envelope      │
│  Windows service             (Rust, canonical bytes)               │
└───────────────────────────────────────────────────────────────────┘
                             │ future versioned report transport
                             ▼
┌──────────────────────────── RHEL host ────────────────────────────────┐
│  Rust host controller                                                 │
│    ├─ joins fresh telemetry with live libvirt current                 │
│    ├─ observes host-side QGA health and dommemstat                     │
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
- The controller never sends an ordinary follow-up resize while `requested`
  and `current` differ. M10f adds only a freshly validated upward cancellation
  or supersession of a pending shrink; it never permits a second lower target.
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
controller uses `dommemstat` rather than guessing. That source is live-observed
and freshness-qualified; live libvirt `current` remains allocation authority.

### 5. Preview before changing memory

Use the read-only Rust decision preview with the approved host configuration
loaded into the environment:

```bash
target/release/virtio-mem-host decision
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
| [`docs/target-controller.md`](docs/target-controller.md) | Normative M10e-M10g estimator, reconciler, persistence, and qualification contract |
| [`docs/feature-matrix.md`](docs/feature-matrix.md) | Feature status by component |
| [`docs/testing.md`](docs/testing.md) | Local, host, guest, and live validation procedures |
| [`docs/issues.md`](docs/issues.md) | Known incidents and unresolved issues |
| [`docs/upstream-virtio-mem-audit.md`](docs/upstream-virtio-mem-audit.md) | Pinned upstream findings and resulting project decisions |
| [`docs/driver-status-interface-feasibility.md`](docs/driver-status-interface-feasibility.md) | M10aX proposal and No-Go gates for an optional read-only driver diagnostic interface |
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
