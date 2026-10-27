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

The guest-driven flow is implemented in source and operationally unqualified.
The native Windows build and test gate passed on ice101 on 27 October; the new
flow has not been deployed or exercised against live VM memory changes.

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

The Linux local gate passes: core 63, host 73 and xtask 50 tests, formatting,
release builds, Clippy with warnings denied and whitespace checks. Windows
cross-target Clippy passes with warnings denied, including the worker tests,
which run only on Windows; the updated native gate passes all 64 Windows tests.
A Windows GNU binary is still blocked by the missing
`x86_64-w64-mingw32-gcc` linker.

### Installed resizing controller — 27 October 2026

Both sides are installed on `win11_gpu` and resize on demand. Windows build
`e2b5982a6df8-dirty.f9731111acbb` (native gate: 54/3/7 tests) runs as
`VirtioMemService`; `virtio-mem-host@win11_gpu.service` is enabled, running
`/etc/virtio-mem-host/win11_gpu.toml` with `observe_only = false`, a 20 GiB
pool and 4 GiB host headroom. Evidence: `.artifacts/install/`.

Fixed in this pass: the guest discarded its reclaim window whenever the device
was converging on its own request, so every 64 MiB removal step waited a
further full window; and `--elevate` started sudo in a background process
group, where it could not read the password from the terminal.

Live results: earlier foreground runs grew 1 GiB to 2.82 GiB in four completed
grants of 5–6 s each. Under the installed controller, the first idle reclaim
(64 MiB) returned only 2 MiB in the 300 s convergence bound; the host settled
it once and reported `partial`, Windows resolved it and paused removals, and
no latch was set. This matches the 26 October observation (10 MiB) and points
at guest-side unplug by the VirtIO viomem driver 100.102.104.29400 on
Windows 11 build 26300. Growth is qualified in practice; reclaim is not.

Driver investigation (upstream `viomem` source at `f7c8beb`, 26 October): each
device notification wakes one removal attempt, which asks Windows to hot-remove
whole blocks inside a window starting at the lowest plugged block and only
`plugged - requested` bytes long. The inherited 64 MiB `shrink_step_bytes`
confined every attempt to the oldest, least removable 64 MiB, which explains
the 2 MiB results; retries of the same small window returned nothing. With the
controller holding, manual windows of 1 GiB and 2.2 GiB returned 90 MiB and
1.17 GiB within seconds (2.82 → 1.56 GiB device). The guest manifest now uses
a 2 GiB step. Upstream issue #1696 (`RtlFindLongestRunSet` never resets its
run counter) can still exclude lower removable memory after holes; it limits
but does not prevent reclaim. The host also settles a shrink that has not
moved for three polls (observed: 17 s) instead of holding the guest's single
request slot for the full timeout.

Release leaves holes that break growth (27 October, 09:07–09:20). A grow from
1.56 to 4 GiB plugged nothing: viomem plugs at the longest free run, which is
the tail above the highest plugged block, and QEMU rejects plug requests
beyond `addr + usable_region_size` (`requested + 256 MiB` on x86-64) with
`VIRTIO_MEM_RESP_ERROR`. Once the holes below the highest plugged block exceed
256 MiB, no grow of any size can succeed until those holes close or a VM reset
unplugs everything. A removal ladder (2 MiB–1 GiB) run in that state returned
at most 6 MiB per request, so its sizes are not evidence. Requesting 0
returned 448 MiB from the top of the region, after which a 512 MiB grow
plugged at once. No upstream fix exists (issue #1696 covers only the run
counter); [the draft reports](prissue.md) describe all three driver issues.
Evidence: `.artifacts/reclaim-ladder/`.

Also fixed on 27 October: libvirt omits `requested-size` from the generated
QEMU argv when the request is zero, so the attestation's argv hash drifted
whenever the live request was zero. At 09:23 the resize sink refused the
settle of a zero-target shrink for that reason, which latched the VM. The
scrubber now removes the whole member; the attestation was regenerated and
the latch cleared with the device converged. An earlier refusal at 09:04
reported the same field while the live request was non-zero; its cause is
unconfirmed, so drift errors now name the attested and live values.

Release and growth characterised (27 October, 09:40–10:05). Growth was capped
because QEMU's usable region only grows: plugs land above the highest plugged
block and fail at the largest-ever `requested + 256 MiB`; the cap reproduced
at 1638 and 1640 MiB. Widening it with the VM paused for 0.14 s removed the
cap: 256 MiB steps to 3.17 GiB and a 1 GiB grow then plugged. After widening,
a release ladder (use and free 3 GiB, then release) returned nothing for
64 MiB–1 GiB, 900 MiB for 2 GiB and 2.83 GiB for 4 GiB, and growth back to
4.25 GiB worked after every rung. The host now widens automatically after a
grow that plugs nothing; the guest step is 3 GiB. Evidence:
`.artifacts/reclaim-ladder/*-widened/`. A growth that never
plugs now settles like a stalled shrink instead of latching after the
convergence timeout.

### Notification-assisted pressure and reclaim — 27 October 2026

Implemented the pressure review recommendations in source: one system-wide
`GetPerformanceInfo` snapshot, native Low as a bounded growth trigger, separate
physical/commit reserve guards, cancellation-aware kernel notification waits
and sustained-Low backoff. Native High plus a continuous surplus history is now
required for RAM release; unknown/Neutral/Low observations, sample failures,
stale or changing geometry and device convergence reset reclaim evidence.
History is monotonic and conservatively compressed; transient numeric failures
retry without publishing stale data. Notification creation can recover without
restarting the service. Durable single-request ownership and partial-removal
backoff are preserved. See development for the precise rules and limitations.

Source checks: core 63, host 73 and xtask 50 tests; native Windows service/CLI/
workload 64 tests, including the collection-to-durable-request path, notification
wait cancellation, failure recovery and sustained-Low backoff. Formatting,
release builds and Clippy with warnings denied pass. Evidence logs:
`.artifacts/pressure-local-gate.log`, `.artifacts/pressure-windows-gate.log`.
No live service deployment, restart, memory change or workload trial was done.

### Native Windows build — 27 October 2026

`make win11-gate` passed on `ice101.lan` using Rust 1.97.1,
`x86_64-pc-windows-msvc` and Visual Studio 2022 17.14.43. Source was revision
`5bf782fff89631d93d0ed15c8471b9755fdb1d89` plus the current working-tree changes.
Release service and workload binaries built; native tests passed (48 library,
3 service CLI, 7 workload CLI), including durable request recovery and the
shutdown/stop interleaving tests. Formatting and Clippy with warnings denied
passed. Both executables were fetched to `.artifacts/windows/win11_gpu/` and
their SHA-256 values matched Windows. Local evidence is
`.artifacts/windows/win11_gpu/native-build-evidence.json`.

SSH now works. The preset initially failed because its remote workspace did
not exist; creating that directory allowed the full gate to pass. No installed
service was changed or restarted, and no VM memory was resized. Live SCM, ACL,
QGA sharing and Event Log checks remain outstanding under WIN-001.

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
request retries, QGA status delivery and renewed pressure. Windows reclaim
steps must be block aligned and large enough to cover the surplus: viomem only
searches a window of the requested size, so small steps return little (see
Installed resizing controller above). Keep conservative reclaim timing.
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
