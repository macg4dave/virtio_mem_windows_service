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
- **Implemented:** M10c publishes a fresh, versioned raw Windows telemetry
   envelope, joins it on the host with alias-scoped live libvirt `current`,
   and calculates the target there. Do not infer allocation
   from limits, QGA totals, balloon `actual`, or aggregate physical memory, and
   do not add a guest resize sink or host-allocation feed to Windows.
- Complete M10d before unattended production delivery. **Implemented:**
   version-2 identity/provenance, dual-clock/sequence ordering, bounded atomic
   handoff/retention, durable restart-safe replay acknowledgement, read-side
   bounds, and LocalService ProgramData ACL provisioning. Installed ACL
   verification remains.
- Keep demand reports advisory and separate from host resize authority.

**Evidence:** local `run` mode exercises the configured worker and fails
visibly when an adapter fails.

### 3. Finish installation and recovery operations

- **Implemented, live verification pending:** provision a protected
  LocalService ProgramData ACL during installation.
- Validate install → start → observe → stop → remove on a Windows guest.
- Configure bounded recovery only for unexpected failures.
- Verify service status transitions and event-log visibility.

**Status:** SCM lifecycle and recovery validation pass. ProgramData ACL code
passes the native build; installed ACL verification and formatted Event Log
message-resource packaging remain.

## Host-side validation path

### 4. Complete the virtio-mem compatibility gate

- Confirm live XML alias, size, block, `requested`, and `current` values.
- Verify `dynamic-memslots` and `unplugged-inaccessible` requirements where
   supported.
- [x] Bind the reviewed live domain, allocation-neutral QEMU command
   line/properties, memory backend, memory-slot and VFIO mapping budgets,
   active balloon-resize state, topology, trust/driver declarations, and
   deployed versions into one M9d SHA-256 fingerprint.
- Rule out or explicitly qualify vDPA, RDMA migration, VFIO-NVMe, `mlock`,
   encrypted/secure virtualization, incompatible vhost-user backends, and
   sparse/preallocated/shared/core-dump/NUMA backend combinations.
- Add maximum-value and unit-conversion round-trip coverage.

### 5. Correct and freshness-qualify host telemetry (M9e)

- [x] Treat `dommemstat actual` as a balloon value, not a whole-guest total or an
   upper bound for `unused`/`available`.
- [x] Parse and enforce bounded `last-update` freshness; reject missing, stale,
   future, and non-advancing evidence where policy requires a fresh sample.
- [x] Keep alias-scoped live libvirt `current` authoritative for allocation.
- [x] Replace the QGA-only Bash decision preview with a Rust command that reuses
   the controller's configured source and freshness checks.
- [x] Retain `guest-get-memory-stats` only as an opt-in custom/downstream adapter;
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

### 8. Preserve the allocation-authority contract and add behavior evidence

**Implemented contract:** Virtio 1.2 and the pinned QEMU/libvirt/virtio-win
sources establish that device `requested_size`/`plugged_size` correspond to
requested intent and plugged allocation. The host uses alias-scoped live
libvirt `requested`/`current`; `current` is authoritative for accounting and
the controller blocks overlap while the pair differs. Driver debug output is
not a second state source.

Continue with these independent evidence tracks:

1. **M10a2 — correlated behavior harness:** define versioned timestamped
   evidence requiring QEMU/libvirt state, Windows health, controller state,
   identity, units, and convergence endpoints. Accept driver trace as optional
   diagnostic input and reject missing or ambiguous required records.
2. **M10a1 — optional diagnostic qualification:** when justified, checksum the
   signed capture tool, run bounded elevated capture without resizing, account
   for debug-message filtering, and prove cleanup without boot logging,
   persistent filter changes, driver restart, or reboot.
3. **M10a3 — optional bounded observation:** only with separate approval and a
   predeclared non-convergence recovery target, preferably after disposable-
   guest rehearsal. Do not describe a shrink as guaranteed rollback.

Use **M10aX** only for a concrete operational diagnostic need that host
observation and bounded tracing cannot meet. It produces a separate signed-
driver interface proposal and does not authorize implementation or install.

### 9. Replace directional polling with a quantitative target controller (M10e-M10g)

The live shrink evidence shows that fixed steps and operation deadlines are
useful bounds, but they do not answer how much memory Windows still needs.
Complete this single-VM redesign before making its behavior a global-pool
primitive:

1. **M10e — instantaneous estimate (complete 2026-09-09):** implement the checked formula and
   invariants in [`docs/target-controller.md`](docs/target-controller.md).
   Calculate separate physical-availability and commit-headroom candidates,
   take their maximum, validate configured fixed visible base memory against
   `physical_total - current`, and clamp to the aligned effective maximum that
   includes the device's 1 GiB safety headroom. Do not size RAM from pressure
   ratios or the since-boot commit peak.
2. **M10e — stable target (complete 2026-09-09):** calculate normal desired and conservative safe-
   floor candidates with distinct byte reserves. Grow desired immediately;
   lower it only from a complete fresh 10-minute high-water window and a
   256 MiB downward deadband. Persist a bounded atomic checkpoint and require
   warm-up after missing, stale, incompatible, or cross-session history.
3. **M10f — reconciliation (complete 2026-09-09):** model policy `desired`, device `requested`,
   authoritative `current`, and explicit controller health separately. Move
   toward desired by at most 1 GiB growth or 64 MiB reclaim. Permit only an
   upward target while shrink is pending, cancel to current before later
   growth, prohibit a second lower target, and retain partial reclaim as useful
   constrained progress.
4. **M10f — intent and timing (complete 2026-09-09):** write command intent before actuation and
   resolve every success, error, timeout, cancellation, and restart against a
   fresh live reread. Persist ambiguity/stall latches and never replay a
   recorded command. Freeze an owned shrink to current when guest telemetry
   becomes stale; otherwise fail recovery-required. Deadlines update health,
   never demand.
5. **M10g — controller qualification:** use deterministic clocks, boundary
   generation, fakes, and fault injection to prove formulas, history, restart
   warm-up, alignment, bounded actuation, supersession, stale-input freeze,
   constrained progress, journal resolution, and no replay.
6. **M10g — platform qualification:** run a bounded Rust workload covering
   committed-only and resident demand, +4 GiB growth settling at +2 GiB,
   renewed pressure during shrink, zero/partial/full progress, ambiguity,
   cancellation, and restart. Report controller correctness separately from
   the selected stack's ability to reclaim reliably.

Automatic shrink is a default-on product capability from M10e onward and is
already the host configuration default. Qualification does not turn the
feature on; it establishes support evidence. Freshness, warm-up, safe floors,
default-off re-notification, one-request convergence, and durable latching are
the operational safety controls.

### 10. Build hermetic global-pool simulation

Before simulation consumes live-shaped inputs, complete M9e host-stat
correctness/freshness, M10c host-side allocation join, M10d report
delivery/freshness, and the M10e/M10f target model and reconciler. Completed
M9d compatibility-attestation drift protection and M10g single-VM evidence
remain gates for live multi-target actuation, not for hermetic pool simulation.

- Model host reserve, actual VM allocations, pool-free capacity, stale reports,
   and in-flight operations.
- Add independent growth and reclaim priorities.
- Simulate `NORMAL`, `CAUTION`, `PRESSURE`, `CRITICAL`, and `EMERGENCY` states.
- Prove aligned, bounded reclaim and stop-on-pressure behavior.

### 11. Add target-based controlled reclaim and actuation

- Reuse the M10e rolling history, quantitative reserves, and safe floors.
- Move toward each VM's absolute desired target using bounded aligned 1 GiB
  growth or 64 MiB reclaim actuation.
- Reuse M10f upward supersession and constrained-current accounting; never
  issue a second lower target while a shrink remains pending.
- Keep automatic Windows shrinking enabled by default while preserving the
  M10e warm-up/floor gates and M10f durable ambiguity/stall latch. M10g reports
  controller and platform-reclaim qualification separately; failure remains
  explicit rather than silently converting the product to growth-only mode.
- Fail closed on stale or inconsistent evidence.
- Keep direct driver IOCTL work deferred unless a separate signed-driver track
   proves a supported interface.

## Phase 4 and operations

After Phase 3 gates pass, implement recovery classification; estimator,
history, desired/requested/current, capacity-limited, constrained, and durable-
latch observability; metrics; restart/journal recovery; dry-run latch clearing;
default-on upgrade and explicit-disable behavior; release packaging; rollback;
and repeatable deployment procedures. Track this work in M12/M13 of
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
