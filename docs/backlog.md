# Backlog

Open work only. [Roadmap](roadmap.md) owns phase status; completed implementation
is not repeated here. P0 blocks safe use of the affected path, P1 is required
for phase acceptance, and P2 is conditional follow-up. Order tasks by their
dependencies, then priority; IDs remain stable when tasks are removed.

Source implementation has no open tasks: REQ-002, REQ-003, HOST-007, SIM-001
and POOL-001 are complete. Everything below needs a native build, operational
authorisation or live evidence.

## Phase 1 — Local readiness

### WIN-001 — Build and verify the Windows service (P1)

Blocked locally: the GNU link fails because `x86_64-w64-mingw32-gcc` is absent;
cross-target Clippy passes. Install MinGW or use the selected MSVC target or the
remote `xtask windows` workflow. Restore a verified SSH endpoint and workload
path first; do not assume the old alias works.

Complete when build/test evidence identifies the revision and target, the
native test suite passes (including the request-state worker tests, which run
only on Windows), and native SCM startup/stop, sampling and atomic publication
pass, including the stop-signal interleaving test. Verify panic/timeout
recovery, quoted executable paths, effective ACLs on pre-existing publication
and configuration files, and Event Log insertion data as well as rendered
messages. Confirm that QGA (as LocalSystem) can replace
`<telemetry_path>.host-status` while the service reads it, and that the service
treats a mid-write view as absent rather than failing.

### OPS-001 — Prepare the single-VM deployment migration (P1)

Review the example TOML, the installed configuration, compatibility
attestation, per-VM state paths and service-account authorisation. Keep
`observe_only = true`. Inspect the installed estimator-era checkpoint: one
recording a latch or command is refused and must be reviewed and removed with
the controller stopped. Prepare a Windows deployment manifest (schema 2) with
a reviewed guest `policy` whose steps match the device block size. Use matching
host, xtask and Windows builds.

Complete when reviewed deployment inputs and recovery steps are available,
`host-deploy` validation succeeds, old `.conf` backup handling is confirmed,
and account access needs no repeated desktop authentication. Installation and
service changes require operational authorisation; preparation does not start
the controller.

## Phase 2 — Single-VM qualification

### QUAL-001 — Establish an observe-only baseline (P1)

Depends on WIN-001 and OPS-001. Re-establish live VM/device state, host reserve
and command ownership after the interrupted trial. Run `xtask preflight` with
the controller inactive, then the integrated application with the reviewed
observe-only config under `host-run`.

Complete when bounded evidence shows the guest issuing plausible requests,
the host logging `action=grant` with plausible would-be grants and publishing
`rejected` observe-only results that Windows resolves, fresh status delivery,
unchanged requested/current allocation and a clean runner stop. Record any
unresolved device state before proceeding.

### QUAL-002 — Demonstrate growth and reclaim under workload (P1)

Depends on QUAL-001. Use authorised bounded application runs with small
Windows reclaim requests, conservative guest timing and an explicit host
reserve. Exercise renewed pressure after reclaim, partial/empty results,
retransmissions, host and Windows service restarts while a request is
outstanding, and a stalled removal that settles short.

Complete when evidence records repeated growth/reclaim timings, actual returned
bytes, correlated results, final live state and recovery outcomes. Qualify
zero-device requests explicitly. Confirm acceptable paging/guest stability and
preserved ownership/latches. A no-resize pass does not satisfy this task.

### QUAL-003 — Qualify the installed single-VM controller (P1)

Depends on QUAL-002. Move from clean foreground trials to the reviewed systemd
deployment and observe normal workloads.

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
