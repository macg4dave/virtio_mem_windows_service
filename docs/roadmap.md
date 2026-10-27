# Roadmap

Share a host RAM pool between Windows KVM guests. **Windows decides when to
request or return RAM and sends explicit add/remove byte amounts.** Linux
validates requests, allocates free pool memory by strict VM priority and applies
virtio-mem device changes. A larger `priority` wins; Linux does not estimate
Windows demand or decide when a guest should reclaim memory.

If a guest asks for 2 GiB and only 1 GiB is free, Linux grants 1 GiB and returns
the partial result. With no free bytes it reports that the pool is empty.
Windows reassesses and can send another request. Linux does not queue the
unfulfilled remainder. Released bytes become free only when observed returned.

This document owns direction, phase status and qualification evidence.
[The backlog](backlog.md) owns open tasks and completion criteria.
[Development](development.md) explains architecture, configuration and validation.

## Current state — implementation updated 27 October 2026

The guest-driven flow is still incomplete and the application is unqualified.
Core now defines explicit add/remove requests, correlated actual-byte results,
bootstrap device status, a maximum 60-second admission lifetime and a serializable
replay journal. Admission
survives retransmission and blocks a new session while an earlier request is
unresolved. These contracts are not yet connected to either production worker.
Windows still publishes telemetry and Linux still computes demand.

Source cleanup completed in this implementation pass:

- Zero targets no longer double as uninitialised state. Shared validation,
  estimator restore, repeated-sample fallback, status and manual resize accept zero.
  Fresh unseen telemetry sessions can start at any observed sequence.
- Linux writers hold a domain-UUID ownership guard and a checkpoint file lock.
  Manual mutations require a stopped controller; relative and parent-traversing
  checkpoint paths are rejected, and directory/symlink aliases cannot bypass locking.
- Resize preparation collects compatibility evidence before its final XML check.
  Automatic commands bind full expected device state and recheck cancellation;
  growth headroom is checked again after preparation. Outstanding and uncertain
  growth reserve physical and pool capacity across rounds and restart.
- Host/xtask subprocess capture shares one bounded implementation, with a 4 MiB
  limit per stream (64 MiB for explicitly elevated inventory), a deadline covering
  inherited pipes and process-group cleanup.
  Recovery uses wall elapsed time; durable operation start times prevent restart
  from renewing uncertainty. Missing legacy timing and clock rollback fail closed.
- Windows startup, completion, panic and stop share one host lifecycle. The host
  owns running/stop-pending transitions; a worker exceeding graceful shutdown
  terminates the process with failure for SCM recovery.
- Publication retries native sharing/lock errors with cancellation and configured
  bounds. Backup failures are diagnostic. Installation quotes the executable,
  owns SCM handles, cleans failed setup, and explicitly applies administrator-owned
  read-only configuration ACLs and writable publication ACLs to existing files.
- Runtime and deployment share Windows wire/path validation. Config reads are
  bounded; unused configuration save APIs, duplicate SCM state/conversion and
  handle-cleanup code were removed. Invalid trailing arguments are rejected.
- Standalone live-resize restores only after successful, uncancelled forward
  convergence. Failed/uncertain operations report live state without another command.

The Linux local gate passes: core 54, host 102 and xtask 48 tests, formatting,
release builds, Clippy with warnings denied and whitespace checks. Windows
cross-target Clippy passes; a temporary Linux harness passes 27 portable
worker/publication tests. A Windows GNU binary build is blocked by the
missing `x86_64-w64-mingw32-gcc` linker. No SCM, ACL, QGA-sharing or VM operation
has been tested live in this pass.

### Evidence still relevant to qualification

These observations were recorded in the former plan, not rechecked during this
review. Re-establish live state before any operational work.

- Archived workload run `qualification-1789231813766-72447` recorded peak growth
  of 1,098,907,648 bytes and no accepted reclaim. A later passing run required
  no resizing and therefore did not qualify automatic resizing.
- On 26 October, `win11_gpu` / `ua-virtiomem0` had a 20 GiB device, 2 MiB blocks
  and a 1 GiB requested/current baseline. A 64 MiB grow converged in about
  4.297 seconds. Restoration reclaimed only 10 MiB before observations stopped
  at about 225.739 seconds after authentication was cancelled. Restoration and
  the full 300-second deadline were not observed; no recovery was recorded.
- QGA file telemetry was fresh at
  `C:\ProgramData\VirtioMemService\telemetry.json`. Balloon statistics also
  advanced, but raw Windows telemetry is the implemented demand source. The
  earlier proposal to select a different source is superseded.
- The installed `virtio-mem-host@win11_gpu.service` was disabled/inactive with
  `Restart=no`. Its old `.conf` was a non-resizing shadow deployment; migration
  must preserve that intent with `observe_only = true`.
- The Windows SSH alias resolved to an unreachable endpoint and the earlier
  workload executable was absent. Deployment readiness remains unverified.
- Local libvirt group membership made the repository polkit rule redundant on
  that host. Dedicated-account access must still be checked for each deployment.

The interrupted manual test grants no continuing permission for live mutations.
Future qualification uses the application through the bounded foreground runner.

## Retained architecture

Keep pure core policy/allocation, alias-scoped XML, atomic Windows publication,
Linux checkpoint durability and the distinction between rejection and uncertain
execution. The host still has two mutex-protected estimator/control objects and
repeated checkpoint/shrink paths. Remove host estimation under REQ-003 before
consolidating these retained responsibilities; do not build a second policy path.
The attestation parser and bounded reader are now shared.

Windows sampling, worker hosting, SCM integration and Event Log publication remain
separate. Native verification must still check callback lifetime, status ordering,
effective ACLs after upgrades, QGA collisions and rendered Event Log insertion data.
A slow synchronous host adapter can still delay other members; configuration or
checkpoint errors can still fail the daemon. No live reliability claim is made.

## Phase 1 — Correct ownership and finish local readiness (partial)

Completed source foundations include the request/result contract, zero semantics,
exclusive ownership, command guards, growth reservations, bounded subprocesses and
Windows lifecycle/publication/configuration/installation fixes described above.

Remaining implementation order:

1. Wire the durable request producer and result consumer into Windows (REQ-002).
   Reuse core policy and atomic handoff, with one unresolved identity across restart.
2. Replace Linux estimation with admission, priority grants, durable execution and
   result delivery (REQ-003). Recheck expiry and grants at the retained effect
   boundary; remove superseded policy configuration and checkpoint state.
3. Update status for the new boundary (HOST-007), consolidate retained host paths
   (SIM-001), prepare deployment migration (OPS-001), and finish shared-config
   operational source support (POOL-001).
4. Produce the native binary and complete WIN-001 checks before live qualification.

Exit: the guest-driven flow is implemented, available source and native checks
pass, and deployment/recovery inputs are ready. Live access is separately authorised.
This phase does not qualify unplug.

### Decisions

Windows owns demand/reclaim; Linux owns priority allocation and execution. Requests
specify add/remove bytes. Partial/empty responses end that request without a host
remainder queue; Windows reassesses before its next delta. A fresh session can
start at a missed-zero sequence. Zero is supported but requires unplug qualification.

Q1 is resolved by exclusive Linux ownership by domain UUID plus checkpoint locks;
operator mutation uses a stopped controller. Q2 is resolved by retaining the
original command start time and combining it with a monotonic process bound.
Missing legacy timestamps, a backwards clock or an expired bound fail closed.

Configuration is administrator-owned and read-only to the Windows service. A
worker exceeding graceful shutdown causes failed process exit. Neither reading
a request nor reading a result is an acknowledgement: the journal and pending
identity must be durable before either side acts or advances. REQ-002/003 own
connecting those contracts to the existing file/QGA transport.

## Phase 2 — Qualify one Windows VM (outstanding)

Depends on the corrected Phase 1 request flow. First inspect guest requests and
host grants with `observe_only = true` under `host-run`; then run bounded
workload trials with resizing enabled. Verify growth, actual returned bytes,
partial results, request retries and renewed pressure. Windows should initially
request small, block-aligned reclaim steps with conservative timing. Test the
real systemd unit only after foreground trials pass.

Exit: recorded convergence and recovery outcomes, acceptable guest memory
behaviour, and a week of real-time sizing on `win11_gpu` without intervention.
The earlier interrupted platform investigation is part of this qualification,
not a separate manual testing phase.

## Phase 3 — Qualify the shared pool (partial)

The pure pool allocator and outstanding-growth reservation fixes are implemented;
guest request/result integration is not.
Live work depends on Phase 2; operational tooling must also handle the chosen
shared-config deployment layout. Include simultaneous requests, partial grants,
empty-pool responses and retransmissions in addition to alternating workload.

Exit: at least two Windows guests share a bounded pool under alternating and
competing load; strict priority, return of released memory and isolation of a
stuck member are demonstrated without violating host reserve limits.

## Evidence-dependent follow-up

After qualification, assess whether compatibility checks can be simplified while
retaining their safety contracts. Faster Windows reclaim needs an observable
safety metric first. Memory-resource notifications are already sampled; using
them as a faster guest request trigger is a possible Windows policy change.
Virtio-balloon reclaim is an alternative only if unplug evidence justifies it.
Linux guest support waits until the Windows pool works. None of these options
requires a second implementation now.
