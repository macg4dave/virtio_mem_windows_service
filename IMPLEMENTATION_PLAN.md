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
   converge; live SCM lifecycle and first recovery restart also pass.
- Preserve explicit transport, parser, cancellation, and startup failures.
- Keep the Windows service free of Linux, libvirt, and host-side commands.

**Evidence:** deterministic timeout and shutdown tests, followed by the local
workspace quality gate.

### 2. Complete concrete Windows runtime wiring

- **Superseded production path:** `NamedPipeGuestAgent` remains a tested legacy
   adapter boundary, but interactive and SCM workers use native Windows
   telemetry and do not open the QGA-owned device.
- Complete M10c using the recorded host-side join: publish a fresh, versioned
   raw Windows telemetry envelope, join it on the host with alias-scoped live
   libvirt `current`, and calculate the target there. Do not infer allocation
   from limits, QGA totals, balloon `actual`, or aggregate physical memory, and
   do not add a guest resize sink or host-allocation feed to Windows.
- Complete M10d before constructing the production publisher: add report
   identity, freshness, ordering, provenance, bounded retention, ACL, and
   partial-record behavior.
- Keep demand reports advisory and separate from host resize authority.

**Evidence:** local `run` mode exercises the configured worker and fails
visibly when an adapter fails.

### 3. Finish installation and recovery operations

- Provision the selected least-privilege account and ProgramData ACLs.
- Validate install → start → observe → stop → remove on a Windows guest.
- Configure bounded recovery only for unexpected failures.
- Verify service status transitions and event-log visibility.

**Status:** SCM lifecycle and recovery validation pass. ProgramData ACLs and
formatted Event Log message-resource packaging remain.

## Host-side validation path

### 4. Complete the virtio-mem compatibility gate

- Confirm live XML alias, size, block, `requested`, and `current` values.
- Verify `dynamic-memslots` and `unplugged-inaccessible` requirements where
   supported.
- Bind the reviewed live domain, QEMU command line/properties, memory backend,
   memory-slot and VFIO mapping budgets, active balloon-resize state, topology,
   and deployed versions into one M9d fingerprint.
- Rule out or explicitly qualify vDPA, RDMA migration, VFIO-NVMe, `mlock`,
   encrypted/secure virtualization, incompatible vhost-user backends, and
   sparse/preallocated/shared/core-dump/NUMA backend combinations.
- Add maximum-value and unit-conversion round-trip coverage.

### 5. Correct and freshness-qualify host telemetry (M9e)

- Treat `dommemstat actual` as a balloon value, not a whole-guest total or an
   upper bound for `unused`/`available`.
- Parse and enforce bounded `last-update` freshness; reject missing, stale,
   future, and non-advancing evidence where policy requires a fresh sample.
- Keep alias-scoped live libvirt `current` authoritative for allocation.
- Replace the QGA-only Bash decision preview with a Rust command that reuses
   the controller's configured source and freshness checks.
- Retain `guest-get-memory-stats` only as an opt-in custom/downstream adapter;
   it is not part of upstream QGA.

### 6. Validate the one-VM host controller

- Install the templated systemd service under the approved service account.
- Exercise one reversible aligned resize through the installed service.
- Confirm convergence suppression, host headroom checks, bounded failures,
   signal handling, and restart behavior.

**Gate:** no expansion of automatic resize until host telemetry is fresh,
live XML and the full compatibility attestation pass, and the device is
converged. Phase 2 permits only one active controller/device on the development
host. A hard QEMU/libvirt memory limit is recommended for trusted `win11_gpu`
and mandatory for any untrusted or production guest.

### 7. Replace the Bash host helper with a Rust CLI (M9c) — implemented

- **Implemented:** Rust subcommands provide read-only `snapshot` and
  `validate` operations.
- **Implemented:** the dry-run resize command reports the validated target and exact
   `virsh` argument vector without mutating the VM.
- **Implemented:** an explicitly gated `resize --apply` command reuses the existing
   Rust XML, compatibility, convergence, unit, and host-headroom contracts.
- **Implemented:** `scripts/virtio-mem-host.sh` was removed after equivalent
  operator documentation and hermetic regression tests passed.
- **Implemented:** no replacement for the removed helper retains a second Bash
  implementation of XML parsing, arithmetic, compatibility, or resize policy.

**Gate:** Rust is the only implementation of host snapshot/validation/resize
policy; the Bash helper is deleted, and the Rust CLI passes the former helper's
read-only and guarded-live regression cases.

## Phase 3 — Global arbitration

### 8. Prove cross-layer state mapping

Observe the same controlled operation through Windows driver state and
libvirt/QEMU state. Do not treat `requested_size`/`plugged_size` as equivalent
to `requested`/`current` until the mapping is documented and validated.

**Blocked:** live inspection of signed driver `100.102.104.29400` found no
supported user-mode state query. Its existing state message is kernel-debug
output. Continue only with explicit approval for a bounded debug capture and
reversible resize, or under a separate signed-driver interface work item.

Complete the mapping through these gates:

1. **M10a1 — observability qualification:** checksum the signed capture tool,
   run a bounded elevated capture without resizing, and prove complete cleanup.
2. **M10a2 — correlated capture harness:** independently define versioned
   timestamped evidence and deterministic rejection of missing or ambiguous
   records; this hermetic work does not depend on privileged capture.
3. **M10a3 — one-block mapping:** under separate approval, stop the controller,
   capture one 2 MiB growth and rollback, then restore its active state.
4. **M10a4 — contract decision:** document authoritative fields and semantics
   for steady, growing, shrinking, failed, and converging states.

Use **M10aX** only if M10a1 proves bounded debug capture is unsuitable. That
conditional milestone produces a separate signed-driver interface proposal;
it does not authorize driver implementation or installation.

### 9. Build hermetic global-pool simulation

Before this phase, complete M9d compatibility-attestation drift protection,
M9e host-stat correctness/freshness, M10c host-side allocation join, M10d
report delivery/freshness, M10a4 state mapping, and the M10b failure/recovery
matrix.

- Model host reserve, actual VM allocations, pool-free capacity, stale reports,
   and in-flight operations.
- Add independent growth and reclaim priorities.
- Simulate `NORMAL`, `CAUTION`, `PRESSURE`, `CRITICAL`, and `EMERGENCY` states.
- Prove aligned, bounded reclaim and stop-on-pressure behavior.

### 10. Add controlled reclaim and actuation

- Add rolling demand history and conservative safe floors.
- Reclaim one aligned step at a time and wait for convergence.
- Keep automatic Windows shrinking disabled by default until M10b proves
   autonomous retry, safe bounded same-target re-notification, or a controlled
   failed-shrink recovery path.
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
