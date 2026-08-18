# Implementation Plan

`BACKLOG.md` is the execution source of truth. `docs/roadmap.md` defines the
milestones and phase gates. This document summarizes the current dependency
ordered implementation plan without duplicating task status tables.

## Phase 2 — Core functionality

### 1. Complete guest transport and lifecycle hardening

- Add bounded connect, write, flush, and read deadlines to the Windows QGA
   named-pipe client. **Implemented:** version-2 configuration now carries a
   5-second default operation deadline; Windows uses overlapped connect,
   write, and read operations with `CancelIoEx` cancellation. The synchronous
   flush API is intentionally avoided because it cannot be cancelled.
- **Implemented:** enforce the configured shutdown timeout during worker
   termination and return a typed timeout failure when cancellation does not
   converge; real SCM observation remains.
- Preserve explicit transport, parser, cancellation, and startup failures.
- Keep the Windows service free of Linux, libvirt, and host-side commands.

**Evidence:** deterministic timeout and shutdown tests, followed by the local
workspace quality gate.

### 2. Complete concrete Windows runtime wiring

- **Implemented:** connect `ServiceConfig` to `NamedPipeGuestAgent` and acquire
   validated QGA memory stats during worker initialization and each poll.
- Provide a trustworthy current-allocation provider; do not infer it from
   configured limits or unrelated QGA totals.
- Construct the advisory `DemandServiceWorker` from the service entry point.
- Keep demand reports advisory and separate from host resize authority.

**Evidence:** local `run` mode exercises the configured worker and fails
visibly when an adapter fails.

### 3. Finish installation and recovery operations

- Provision the selected least-privilege account and ProgramData ACLs.
- Validate install → start → observe → stop → remove on a Windows guest.
- Configure bounded recovery only for unexpected failures.
- Verify service status transitions and event-log visibility.

**Dependency:** real Windows service-manager permissions and guest QGA channel
verification.

## Host-side validation path

### 4. Complete the virtio-mem compatibility gate

- Verify `dommemstat` fields on the target guest.
- Confirm live XML alias, size, block, `requested`, and `current` values.
- Verify `dynamic-memslots` and `unplugged-inaccessible` requirements where
   supported.
- Rule out documented incompatible workloads and device classes.
- Add maximum-value and unit-conversion round-trip coverage.

### 5. Validate the one-VM host controller

- Install the templated systemd service under the approved service account.
- Exercise one reversible aligned resize through the installed service.
- Confirm convergence suppression, host headroom checks, bounded failures,
   signal handling, and restart behavior.

**Gate:** no automatic resize until live QGA/dommemstat, XML compatibility, and
convergence evidence pass.

## Phase 3 — Global arbitration

### 6. Prove cross-layer state mapping

Observe the same controlled operation through Windows driver state and
libvirt/QEMU state. Do not treat `requested_size`/`plugged_size` as equivalent
to `requested`/`current` until the mapping is documented and validated.

### 7. Build hermetic global-pool simulation

- Model host reserve, actual VM allocations, pool-free capacity, stale reports,
   and in-flight operations.
- Add independent growth and reclaim priorities.
- Simulate `NORMAL`, `CAUTION`, `PRESSURE`, `CRITICAL`, and `EMERGENCY` states.
- Prove aligned, bounded reclaim and stop-on-pressure behavior.

### 8. Add controlled reclaim and actuation

- Add rolling demand history and conservative safe floors.
- Reclaim one aligned step at a time and wait for convergence.
- Fail closed on stale or inconsistent evidence.
- Keep direct driver IOCTL work deferred unless a separate signed-driver track
   proves a supported interface.

## Phase 4 and operations

After Phase 3 gates pass, implement recovery classification, structured
observability, metrics, health checks, restart behavior, release packaging,
rollback, and repeatable deployment procedures. Track this work in M12/M13 of
`docs/roadmap.md` rather than creating a second task numbering scheme.

## Validation gates

The local gate is:

- `cargo fmt --all -- --check`
- `cargo build --workspace --all-features --release`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `bash -n scripts/*.sh`

Live host and guest operations are separate, explicit-scope validation. They
must not be substituted with local test success, and protected mutations
require the approval and safety procedure documented in `docs/testing.md`.
