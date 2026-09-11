# Architecture

## Supported scope

The current system coordinates one explicitly configured trusted Windows guest
and one virtio-mem device from one RHEL host controller. Multi-VM arbitration,
untrusted guests, and production support require the future global-controller
architecture.

## Ownership

### Windows service

The Windows Rust service owns native memory measurement, versioned atomic raw
telemetry publication, local configuration and ACLs, SCM lifecycle, and Windows
observability. It never receives host allocation, calculates a host resize, or
invokes QGA/libvirt/Linux commands. The installed QEMU Guest Agent owns the QGA
virtio-serial channel.

### Host controller

The host Rust service owns raw-telemetry validation and replay protection,
alias-scoped live XML/QMP state, compatibility attestation, host headroom,
absolute target calculation, desired/requested/current reconciliation, intent
journaling, actuation, convergence, latching, and recovery.

Live libvirt `current` is allocation authority. `requested` is device intent;
Windows counters are demand evidence only.

### Shared core

The shared crate owns checked byte units, virtio-mem XML parsing and geometry,
memory policy, target estimation, reconciliation, recovery state machines, and
versioned evidence types. Pure logic remains independent of live systems.

### Build and validation control plane

`cargo xtask` owns aggregate repository gates, remote native-Windows
orchestration, artifact verification, environment checks, QGA readiness,
reversible live-resize orchestration, and unattended qualification. It does not
replace focused Cargo tests or become a second product controller.

Editor and Make entrypoints delegate to `xtask`. Privileged workflows validate
their scope before one `sudo` re-execution of the current prebuilt xtask; Cargo
and evidence persistence remain unprivileged.

## Data flow

```text
Windows native counters
    -> versioned allocation-free telemetry record
    -> least-privilege transport
    -> host identity/freshness/replay validation
    + alias-scoped live libvirt current
    -> absolute desired target
    -> desired/requested/current reconciliation
    -> compatibility and headroom gates
    -> journaled libvirt request
    -> convergence, constrained state, or durable latch
```

The host acknowledges accepted telemetry durably. Missing, stale, malformed,
replayed, cross-VM, or incomplete records cannot authorize reclaim.

## Configuration

Deployment supplies explicit VM/device identity, service identity, paths,
memory policy inputs, timing, source selection, and safety reserves. Device
size, block geometry, requested, and current are derived from fresh live state.
No component infers a deployment from a historical test VM.

The Windows versioned configuration file is required. The host instance file
is validated before service startup. Configuration changes that affect
compatibility require a fresh reviewed attestation.

## Actuation safety

- Only the host issues resize requests.
- Targets are checked byte counts aligned to the current device block.
- An ordinary request is prohibited while requested and current differ.
- Every request uses fresh compatibility, headroom, telemetry, and live-state
  evidence and is journaled before command execution.
- Ambiguous command outcome is resolved by live reread; unknowable or stalled
  state latches durably rather than retrying blindly.
- Automatic reclaim remains default-on with a deliberate pause override.
- Normative target, quantum, history, hysteresis, notification, and recovery
  constants are defined only in `target-controller.md` and owning Rust code.

## Lifecycle and deployment

The Windows service uses wakeable cancellation and configured operation,
polling, and shutdown bounds. Unexpected worker failure remains non-zero and
observable.

The checked-in host unit is fail-stop. Deployment monitoring decides whether
and when a reviewed restart is appropriate; repository defaults do not encode
a retry loop learned from testing.

Candidate build/test and hash verification run through `cargo xtask`. Product
service installation uses the product CLI under the target platform's required
privilege. RHEL elevation uses one reviewed typed xtask re-execution; no shell
script or root Cargo process is part of the workflow.

## Validation

Focused Rust tests prove deterministic behavior. Local, native Windows,
deployment, and live workflows remain separate result layers. Live tests use
explicit targets and run-specific bounds, capture initial and final state, and
define cleanup/rollback before mutation. See `testing.md`.
