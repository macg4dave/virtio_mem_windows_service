# Backlog

Open work only. [Roadmap](roadmap.md) owns phase status; completed implementation
is not repeated here. P0 blocks safe use of the affected path, P1 is required
for phase acceptance, and P2 is conditional follow-up. Order tasks by their
dependencies, then priority; IDs remain stable when tasks are removed. Severity
(High/Medium/Low) describes review impact, separately from scheduling priority.

## Phase 1 — Guest requests and local readiness

### REQ-002 — Move demand and reclaim decisions into Windows (P1, High)

The shared request/result contract and zero/lifecycle/publication guards are
implemented. Extend the existing Windows worker to decide add/remove
amounts and consume host results. Reuse suitable core policy and
atomic publication; avoid introducing a second estimator or transport framework.

Complete when Windows owns pressure thresholds, step sizes, reclaim history,
hysteresis and retry/reassessment after partial/empty results. Results are
correlated by request identity, and retransmission cannot mean a fresh request.
Keep one unresolved request per VM, retain its identity across restart, and
resolve its outcome before publishing a new delta. Reject stale, foreign or
superseded results; after a partial/empty result, reassess current demand before
issuing a new request. Windows currently has no result reader or durable request
state, so atomic telemetry replacement alone does not satisfy these criteria.
Extend the shared configuration validation and installation ACLs to the new
request, result and durable pending-state paths; reject path/file aliases.
Test the complete worker-to-request/result workflow with injected sampling and
transport, then perform WIN-001 native verification. No Windows libvirt calls
or direct host-device control are implied.

### REQ-003 — Replace host demand policy with request execution (P1, High)

The shared contract, ownership and command guards are implemented; coordinate
with REQ-002. Replace
[TargetDemandSource](../host/src/target_policy.rs#L341) and the estimator wiring
in `main.rs` with request admission, priority grants and durable results.

Complete when Linux interprets add/remove requests without calculating Windows
pressure or reclaim history, grants available bytes highest priority first,
returns actual/partial/empty outcomes, and never queues an unfulfilled remainder.
Persist execution identity/results so a crash between admission, command and
response cannot duplicate or silently lose an uncertain effect. Retain live
convergence, host reserve checks, command ownership and manual recovery. Recheck
request expiry and pool/host grants after blocking preparation at the guarded
effect boundary. Use the core journal to recognise retransmission before expiry
checking, without regranting an already admitted effect. Remove
superseded host policy settings/state and migrate configs without turning an
observe-only installation into an actuating one. Cover the real wiring across
Windows requests, pure pool allocation and fake host effects.

### HOST-007 — Report current request/execution readiness (P1, Medium)

Stored history now checks sample age before reporting readiness. After REQ-003,
replace estimator status with request/execution/result status. Include observe-only,
stale/expired input, partial/empty results and empty-device state. Clearly label
unavailable process-local context; do not recreate Windows policy in diagnostics.
Update status consumers and schema together. This remains outstanding source work.

### SIM-001 — Consolidate retained host code after policy removal (P2, Medium)

Depends on REQ-003 and the safety fixes. Consolidate concrete duplication:
[checkpoint readers](../host/src/target_policy.rs#L552) and
[normal loading](../host/src/target_policy.rs#L618),
shrink replacement journalling/execution in `observe_owned_shrink` and
`settle_short`. Attestation parsing now uses one bounded 64 KiB reader.

Complete when retained responsibilities have one reader/validator and one
consistent execution path with operation-specific preconditions. Preserve
intentional latch-clear migration differences, Linux durability and manual
`abandon-shrink`. Remove the two-object estimator/control reconstruction along
with host estimation rather than redesigning it first. Review public test-only
conveniences such as `with_external_compatibility` and `HostRuntime::new` against
all callers before restricting/removing them; they are not proven externally
unused. Reuse existing hashing/validation helpers where useful without a generic
utility layer. Further attestation removal needs qualification evidence.

### WIN-001 — Build and verify the Windows service (P1)

Depends on REQ-002 and the integrated request flow in REQ-003. The Windows
source safety fixes are implemented. The local GNU link attempt currently fails
because `x86_64-w64-mingw32-gcc` is absent; cross-target Clippy passes. Produce
an actual binary for the selected GNU or MSVC target and run
native service tests through the existing Windows tooling. Restore a verified
SSH endpoint and workload path as prerequisites; do not assume the old alias works.

Complete when build/test evidence identifies the revision and target, native
SCM startup/stop, sampling and atomic publication pass, and the stop-signal
interleaving test runs on Windows. Include panic/timeout
recovery, QGA sharing collisions, quoted executable paths and effective ACLs on
pre-existing config files. Inspect Event Log insertion data as well as rendered
messages. Cross-target type-checking alone is insufficient; the review's
portable harness does not close this task.

### OPS-001 — Prepare the single-VM deployment migration (P1)

Depends on the retained config/schema after REQ-003 and the host safety fixes.
Review the example TOML, current installed configuration, compatibility
attestation, per-VM state paths and service-account authorisation. Preserve the
old shadow deployment with `observe_only = true`. Use matching host and xtask
builds; account for the checkpoint/status migration in [Development](development.md).

Complete when reviewed deployment inputs and recovery steps are available,
the deployment validation succeeds, old `.conf` backup handling is confirmed,
and account access needs no repeated desktop authentication. Installation and
service changes require operational authorisation; preparation does not start
the controller.

## Phase 2 — Single-VM qualification

### QUAL-001 — Establish an observe-only baseline (P1)

Depends on WIN-001 and OPS-001. Re-establish live VM/device state, host reserve,
request identity and freshness, result delivery and command ownership after
the interrupted trial. Run the integrated application with a reviewed
observe-only config under `host-run`.

Complete when bounded evidence shows valid guest requests and plausible host
grants/results, unchanged requested/current allocation and a clean runner stop.
Record any unresolved device state before proceeding.

### QUAL-002 — Demonstrate growth and reclaim under workload (P1)

Depends on QUAL-001 and all High request/actuation fixes. Use authorised bounded
application runs with small Windows reclaim requests, conservative guest timing
and an explicit host reserve. Exercise renewed pressure during pending reclaim,
partial/empty results, retransmissions and controlled service restart cases.

Complete when evidence records repeated growth/reclaim timings, actual returned
bytes, partial or stalled unplug, correlated actual-byte results, final live
state and recovery outcomes. Qualify zero-device requests explicitly. Confirm
acceptable paging/guest stability and preserved ownership/latches. A no-resize pass does not satisfy this task.

### QUAL-003 — Qualify the installed single-VM controller (P1)

Depends on QUAL-002. Move from clean foreground trials to the reviewed systemd
deployment and observe normal workloads.

Complete when a week of resizing needs no manual intervention and evidence
shows host reserve protection, guest stability and memory returned when idle.

## Phase 3 — Shared-pool qualification

### POOL-001 — Support shared-config operational workflows (P1)

Source implementation can proceed independently of live Phase 2 acceptance.
`host-deploy` and `qualification` call
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
bytes, partial grants and empty-pool responses without a remainder queue,
release before reuse, host reserve protection across rounds and continued
operation of healthy members when another stalls or becomes unreadable. Include
startup with an unreadable member, which currently reserves the whole pool budget.

## Evidence-dependent follow-up

### TUNE-001 — Choose evidence for faster reclaim (P2)

After Phase 2, use trial logs to identify a measurable indication that Windows
can safely request reclaim faster. Complete when the metric and acceptance
threshold are agreed from evidence, or the conservative settings are retained with a reason.
Any subsequent policy change needs its own scoped task.
