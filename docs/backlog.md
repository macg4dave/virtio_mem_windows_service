# Backlog

Open work only. [Roadmap](roadmap.md) owns phase status; completed implementation
is not repeated here. P0 blocks safe use of the affected path, P1 is required
for phase acceptance, and P2 is conditional follow-up. Order tasks by their
dependencies, then priority; IDs remain stable when tasks are removed.

## Phase 1 — Local readiness

### WIN-001 — Build and verify the Windows service (P1)

Produce an actual binary for the selected GNU or MSVC target and run the native
service tests through the existing Windows tooling. Restore a verified SSH
endpoint and workload path as prerequisites; do not assume the old alias works.

Complete when build/test evidence identifies the revision and target, native
SCM startup/stop, sampling and atomic publication pass, and the stop-signal
interleaving test runs on Windows. Cross-target type-checking alone is insufficient.

### OPS-001 — Prepare the single-VM deployment migration (P1)

Review the example TOML, current installed configuration, compatibility
attestation, per-VM state paths and service-account authorisation. Preserve the
old shadow deployment with `observe_only = true`. Use matching host and xtask
builds; account for the checkpoint/status migration in [Development](development.md).

Complete when reviewed deployment inputs and recovery steps are available,
the deployment validation succeeds, old `.conf` backup handling is confirmed,
and account access needs no repeated desktop authentication. Installation and
service changes require operational authorisation; preparation does not start
the controller.

### REC-001 — Stop unsafe restoration attempts in standalone live-resize (P0)

`tools/xtask/src/live_resize.rs::run` attempts ordinary restoration even when
forward convergence failed. The product resize path rejects an unconverged
device, so this is not a reliable recovery mechanism.

Complete when failed/uncertain forward operations preserve and report live
state without automatically issuing an incompatible restoration; normal
restoration after a converged trial still works. Add adapter-based regression
coverage for convergence, timeout, cancellation and restoration failure, and
describe the operator recovery path. Required before using this command for
qualification; `host-run` is the primary trial runner.

## Phase 2 — Single-VM qualification

### QUAL-001 — Establish an observe-only baseline (P1)

Depends on WIN-001 and OPS-001. Re-establish live VM/device state, host reserve,
telemetry identity and freshness, and command ownership after the interrupted
trial. Run the application with a reviewed observe-only config under `host-run`.

Complete when bounded evidence shows accepted telemetry, plausible target
decisions, unchanged requested/current allocation and a clean runner stop.
Record any unresolved device state before proceeding.

### QUAL-002 — Demonstrate growth and reclaim under workload (P1)

Depends on QUAL-001. Use authorised bounded application runs with small reclaim
steps, a long history window and an explicit host reserve. Exercise renewed
pressure during pending reclaim and controlled telemetry failure/restart cases.

Complete when evidence records repeated growth/reclaim timings, actual returned
bytes, partial or stalled unplug, `shrink_settled_short`, final live state and
recovery outcomes. Confirm acceptable paging/guest stability and preserved
ownership/latches. A no-resize pass does not satisfy this task.

### QUAL-003 — Qualify the installed single-VM controller (P1)

Depends on QUAL-002. Move from clean foreground trials to the reviewed systemd
deployment and observe normal workloads.

Complete when a week of resizing needs no manual intervention and evidence
shows host reserve protection, guest stability and memory returned when idle.

## Phase 3 — Shared-pool qualification

### POOL-001 — Support shared-config operational workflows (P1)

Depends on Phase 2 acceptance. `host-deploy` and `qualification` call
`HostConfig::load_vm(..., None)`, rejecting multi-VM files. `controller-status`
already selects a VM, but assumes `/etc/virtio-mem-host/<instance>.toml` and
ties the instance to VM identity. Review those assumptions together.

Complete when one documented deployment layout supports a shared TOML file,
explicit VM selection for qualification/status, distinct per-VM checkpoints
and correct unit identity. Retain existing single-VM commands, avoid dropping
other members during deployment, correct stale environment-file help wording,
and test invalid/ambiguous selection without live effects.

### POOL-002 — Qualify multiple Windows guests sharing RAM (P1)

Depends on POOL-001 and QUAL-003. Use at least two authorised test VMs with
alternating and competing load.

Complete when live evidence demonstrates strict-priority growth from free
bytes, release before reuse, host reserve protection and continued operation
of healthy members when another stalls or becomes unreadable. Include startup
with an unreadable member, which currently reserves the whole pool budget.

## Evidence-dependent follow-up

### SIM-001 — Assess remaining compatibility/tooling consolidation (P2)

After Phase 2, trace current callers and qualification evidence before deciding
whether attestation or diagnostic layers can be reduced. The old plan's broad
claims of duplicate state are not proof of a current defect.

Complete when specific duplication is either removed with equivalent failure
coverage or explicitly retained for a demonstrated contract. Preserve manual
`abandon-shrink`, command ownership, uncertain-outcome handling and platform
publication semantics. No new compatibility framework is implied.

### TUNE-001 — Choose evidence for faster reclaim (P2)

After Phase 2, use trial logs to identify a measurable indication that reclaim
can safely run faster. Complete when the metric and acceptance threshold are
agreed from evidence, or the conservative settings are retained with a reason.
Any subsequent policy change needs its own scoped task.
