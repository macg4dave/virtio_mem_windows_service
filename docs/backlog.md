# Backlog

Open work only. [Roadmap](roadmap.md) owns phase status; completed implementation
is not repeated here. P0 blocks safe use of the affected path, P1 is required
for phase acceptance, and P2 is conditional follow-up. Order tasks by their
dependencies, then priority; IDs remain stable when tasks are removed.

Source implementation and the single-VM deployment are complete (OPS-001
installed `win11_gpu` with resizing enabled; QUAL-001's observe-only baseline
was skipped). Everything below needs live evidence on the running deployment.

## Phase 2 — Single-VM qualification

### CFG-001 — Verify the consolidated configuration upgrade (P1)

The consolidated host configuration and Windows schema 7 are now installed.
The native MSVC gate passed 65 tests; installed binaries match their candidates,
backups were retained, the old shrink latch was recovered without deleting its
checkpoint, and the pending guest request resolved. The live recorder works
with optional telemetry omitted. An initial `.host-status` sharing violation
stopped after restarting QEMU Guest Agent; qualify recurrence and recovery.
Evidence: `.artifacts/install/config-install-*` and Windows deploy evidence.

The VM was subsequently started with 8 MiB device blocks, and fresh attestation
was installed. Windows recognised the new device geometry; the controller
reported converged with no latch. Workload/reclaim results from 2 MiB blocks do not
qualify the changed geometry; include 8 MiB blocks in the remaining trials.
A subsequent growth regression caused by validating an old result against the
new block size was fixed and deployed. A bounded resident workload verified
completed growth of 1 GiB plus 8 MiB; retain reclaim and cold-boot qualification
as open work. Evidence: `.artifacts/ram-growth-fix/`.

The saved startup request is now zero; verify the next cold boot starts with
no plugged device RAM before any guest policy request. No live forced unplug
was used to achieve this configuration change.

Remaining: native ACL checks, optional guest/host diagnostic outputs including
an unwritable output path, and explicit refusal of unsupported custom legacy
settings during an upgrade. Exercise independent user-set min/max bounds under
growth, reclaim and restart. The installed source lower bound is 6 GiB, but the
VM still has 8 GiB boot RAM; a VM-definition change and reboot would be needed
to make 6 GiB physically reachable and are not part of this installation.

Complete when recorded native and live evidence identifies the exact builds,
shows independent per-VM bounds and demonstrates telemetry-free operation.

### WIN-001 — Verify the installed Windows service (P1)

Native build and tests pass via `make win11-gate` (65 tests, including the
request-state worker tests) and the service runs under SCM on `win11_gpu`.
Remaining: verify stop and restart, panic/timeout recovery, quoted executable
paths, effective ACLs on pre-existing publication and configuration files, and
Event Log insertion data as well as rendered messages. Confirm that QGA (as
LocalSystem) can replace `<telemetry_path>.host-status` while the service
reads it, and that the service treats a mid-write view as absent rather than
failing. A guest-agent read of that file does block the host's replacement
(seen 27 October), so `cargo xtask watch` does not read it.

Complete when each check has recorded evidence against an identified build.

### QUAL-002 — Demonstrate growth and reclaim under workload (P1)

Growth, auto-widening and large-window reclaim have been seen on the installed
controller (roadmap). Remaining: authorised bounded application runs with Windows
reclaim steps that cover the surplus (viomem searches only a window of the
requested size), conservative guest timing and an explicit host
reserve. Exercise renewed pressure after reclaim, partial/empty results,
retransmissions, host and Windows service restarts while a request is
outstanding, and a stalled removal that settles short.

Complete when evidence records repeated growth/reclaim timings, actual returned
bytes, correlated results, final live state and recovery outcomes. Qualify
zero-device requests explicitly. Confirm acceptable paging/guest stability and
preserved ownership/latches. A no-resize pass does not satisfy this task.

### QUAL-003 — Qualify the installed single-VM controller (P1)

Depends on QUAL-002. The systemd deployment is already running; the week
starts once QUAL-002 passes and a Windows build with the zero-byte removal
backoff is installed, with no reinstall or latch clear during it.

Complete when a week of resizing needs no manual intervention and evidence
shows host reserve protection, guest stability and memory returned when idle.

## Phase 3 — Shared-pool qualification

### POOL-002 — Qualify multiple Windows guests sharing RAM (P1)

Depends on QUAL-003. Use at least two authorised test VMs in one shared config
with alternating and competing load.

Complete when live evidence demonstrates strict-priority grants from free
bytes, partial grants and empty-pool responses without a remainder queue,
release before reuse, host reserve protection across rounds and continued
operation of healthy members when another stalls or becomes unreadable,
including startup with an unreadable member (charged the whole pool until its
first read).

## Evidence-dependent follow-up

### TUNE-001 — Choose evidence for faster reclaim (P2)

After Phase 2, use trial logs to identify a measurable indication that Windows
can safely request reclaim faster, and whether restart warm-up of the guest
estimator matters. Complete when the metric and acceptance threshold are agreed
from evidence, or the conservative settings are retained with a reason. Any
subsequent policy change needs its own scoped task.
