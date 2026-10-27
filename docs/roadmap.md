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

The guest-driven flow is implemented in source and unqualified. No part of it
has run against a live VM or a native Windows build.

Completed in this implementation pass:

- **Windows owns demand (REQ-002).** The estimator and step planning moved into
  a core guest policy used by the Windows worker. The worker publishes its one
  unresolved request inside the existing atomic telemetry publication, keeps it
  durably in `<telemetry_path>.request-state` before publishing, retransmits it
  across restarts and resolves it only from a matching terminal result in the
  host status. Policy settings moved to Windows configuration schema 5.
- **Linux executes requests (REQ-003).** The host estimator, reconciler,
  telemetry acceptance and pending-shrink supersession are removed. The runtime
  admits requests through the core journal, grants by device ceiling, priority
  pool and host reserve, saves admission and intent together before the
  command, reports actual/partial/empty/rejected/uncertain results and writes
  status back to the guest over QGA. Refusals before execution no longer latch.
- **Status (HOST-007).** Controller status version 5 reports the admitted
  request and result, observe-only mode and a read-only view of guest input.
- **Consolidation (SIM-001).** One checkpoint type and reader (version 2) serves
  the daemon, `status` and `clear-latch`; the two-object estimator/control
  store, two telemetry source types, a duplicated base64 encoder and
  test-only public conveniences are gone. Estimator-era checkpoints holding a
  latch or command fail closed for review.
- **Shared configs (POOL-001).** One instance runs one config with any number
  of VMs. `host-deploy` installs one attestation per VM at its configured path;
  `controller-status` and qualification select a VM explicitly.

The Linux local gate passes: core 53, host 73 and xtask 50 tests, formatting,
release builds, Clippy with warnings denied and whitespace checks. Windows
cross-target Clippy passes with warnings denied, including the worker tests,
which run only on Windows. A Windows GNU binary is still blocked by the missing
`x86_64-w64-mingw32-gcc` linker.

### Evidence still relevant to qualification

These observations predate the request model and were not rechecked.
Re-establish live state before any operational work.

- Archived workload run `qualification-1789231813766-72447` recorded peak growth
  of 1,098,907,648 bytes and no accepted reclaim. A later passing run required
  no resizing and therefore did not qualify automatic resizing.
- On 26 October, `win11_gpu` / `ua-virtiomem0` had a 20 GiB device, 2 MiB blocks
  and a 1 GiB requested/current baseline. A 64 MiB grow converged in about
  4.297 seconds. Restoration reclaimed only 10 MiB before observations stopped
  at about 225.739 seconds after authentication was cancelled. Restoration and
  the full 300-second deadline were not observed; no recovery was recorded.
- QGA file telemetry was fresh at
  `C:\ProgramData\VirtioMemService\telemetry.json`. QGA writes (now used for
  host status) have not been exercised.
- The installed `virtio-mem-host@win11_gpu.service` was disabled/inactive with
  `Restart=no`. Its old `.conf` was a non-resizing shadow deployment; migration
  must preserve that intent with `observe_only = true`. Its estimator-era
  checkpoint needs review if it records a latch or command.
- The Windows SSH alias resolved to an unreachable endpoint and the earlier
  workload executable was absent. Deployment readiness remains unverified.
- Local libvirt group membership made the repository polkit rule redundant on
  that host. Dedicated-account access must still be checked for each deployment.

The interrupted manual test grants no continuing permission for live mutations.
Future qualification uses the application through the bounded foreground runner.

## Retained architecture

Pure contracts, guest policy and pool allocation live in core; platform effects
stay in the host and Windows crates. Alias-scoped XML, the resize sink's final
live/compatibility checks, Linux checkpoint durability, Windows atomic
replacement and the distinction between rejection and uncertain execution are
retained. A slow synchronous host adapter can still delay other members, and
checkpoint errors can still stop the daemon. No live reliability claim is made.

Native verification must still check callback lifetime, status ordering,
effective ACLs after upgrades, QGA read/write sharing on the publication
directory and rendered Event Log insertion data.

## Phase 1 — Correct ownership and finish local readiness (partial)

Source implementation of the guest-driven flow, status, consolidation and
shared-config tooling is complete. Remaining:

1. Produce the native Windows binary and complete WIN-001 checks.
2. Prepare the deployment migration inputs (OPS-001).

Exit: available source and native checks pass, and deployment/recovery inputs
are ready. Live access is separately authorised. This phase does not qualify
unplug.

### Decisions

Windows owns demand/reclaim; Linux owns priority allocation and execution. The
request rides in the existing atomic publication and the host answers through
a status file beside it, so both sides derive every path from the one
configured telemetry path and no second transport exists. Partial/empty
responses end a request without a host remainder queue; Windows reassesses on
its next sample. Zero is supported but requires unplug qualification.

Q1 is resolved by exclusive Linux ownership by domain UUID plus checkpoint locks;
operator mutation uses a stopped controller. Q2 is resolved by retaining the
original command start time with a monotonic process bound; a settle-short
command gets its own bound, once.

Configuration is administrator-owned and read-only to the Windows service. A
worker exceeding graceful shutdown causes failed process exit. Reading a
request or a result is never an acknowledgement: the guest's pending request
and the host's admission are durable before either side publishes or acts.
Refusals before any device command are not journalled and never latch; only an
unknown or contradicted effect latches.

## Phase 2 — Qualify one Windows VM (outstanding)

Depends on Phase 1. First inspect guest requests and would-be grants with
`observe_only = true` under `host-run`; then run bounded workload trials with
resizing enabled. Verify growth, actual returned bytes, partial results,
request retries, QGA status delivery and renewed pressure. Windows should
initially request small, block-aligned reclaim steps with conservative timing.
Test the real systemd unit only after foreground trials pass.

Exit: recorded convergence and recovery outcomes, acceptable guest memory
behaviour, and a week of real-time sizing on `win11_gpu` without intervention.
The earlier interrupted platform investigation is part of this qualification,
not a separate manual testing phase.

## Phase 3 — Qualify the shared pool (partial)

The pool allocator, priority grants and shared-config tooling are implemented.
Live work depends on Phase 2. Include simultaneous requests, partial grants,
empty-pool responses and retransmissions in addition to alternating workload.

Exit: at least two Windows guests share a bounded pool under alternating and
competing load; strict priority, return of released memory and isolation of a
stuck member are demonstrated without violating host reserve limits.

## Evidence-dependent follow-up

After qualification, assess whether compatibility checks can be simplified while
retaining their safety contracts. Faster Windows reclaim needs an observable
safety metric first. Memory-resource notifications are already sampled; using
them as a faster request trigger is a possible Windows policy change.
Persisting guest estimator history would shorten reclaim warm-up after a
service restart if trials show it matters. Virtio-balloon reclaim is an
alternative only if unplug evidence justifies it. Linux guest support waits
until the Windows pool works. None of these options requires a second
implementation now.
