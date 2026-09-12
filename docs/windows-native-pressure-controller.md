# Windows-Native Memory Pressure Controller

## Decision

The project is moving away from fixed physical and commit headroom as the
primary estimate of Windows memory demand. The destination is a Windows-native
pressure-aware controller:

- Windows and Microsoft-supported facilities expose guest memory pressure.
- The Windows service collects, timestamps, qualifies, and normalises those
  signals. It does not request a virtio-mem size.
- The host converts qualified pressure evidence into a bounded absolute target
  and remains the only resize authority.
- Existing freshness, identity, compatibility, host-capacity, alignment,
  journaling, convergence, recovery, and fail-closed rules remain in force.

The implemented fixed-headroom estimator remains useful as a conservative
fallback and safety floor during migration. It is no longer the intended
primary release policy.

## Existing implementation audit

### What directly determines `desired` today

[`NativeMemoryTelemetry::collect`](../windows/src/demand.rs) calls
`GlobalMemoryStatusEx` and `GetPerformanceInfo`. The raw production path is
selected in [`host/src/main.rs`](../host/src/main.rs) and consumed by
`TargetDemandSource` in [`host/src/target_policy.rs`](../host/src/target_policy.rs).
Its candidate calculation is implemented by `candidate` in
[`crates/virtio-mem-core/src/target_controller.rs`](../crates/virtio-mem-core/src/target_controller.rs).

For live virtio-mem allocation `C`, Windows physical total `T`, physical
available `A`, committed bytes `K`, commit limit `L`, configured visible base
`B`, and configured reserves, the current estimator calculates:

```text
physical_used       = T - A
physical_candidate  = physical_used + physical_reserve - B
commit_headroom     = L - K
commit_candidate    = C + commit_reserve - commit_headroom
candidate           = max(configured_minimum,
                          physical_candidate,
                          commit_candidate)
desired_now         = align_up(min(candidate, effective_max), block_size)
```

The same calculation with smaller reserves produces `floor_now`. Growth raises
durable `desired` immediately. Shrink requires a complete, gap-free history
window and downward hysteresis; the window contains calculated candidates, not
a Windows pressure score or demand forecast.

Because valid topology requires approximately `T = B + C`, the physical term
is effectively `C + physical_reserve - A`: it preserves a configured amount of
immediately available memory. The commit term preserves configured commit
headroom. The recorded deployment supplies both reserves as configuration.
They are fixed safety margins, not predictions by the Windows memory manager.

### Counter influence

| Windows value | Collected now | Directly changes the raw-path target | Current role |
| --- | --- | --- | --- |
| Physical total and available | Yes | Yes | Physical-use candidate and topology check |
| Commit total and commit limit | Yes | Yes | Commit-headroom candidate |
| Memory load percent | Yes | No | Range validation/diagnostic only |
| Commit peak | Yes | No | Validation/diagnostic only |
| System cache | Yes | No | Diagnostic only |
| Kernel paged/nonpaged pools | Yes | No | Diagnostic only |
| Standby-list classes, free/zero and modified lists | No | No | Not represented separately |
| Low/high memory resource notification | No | No | Not used |
| Pagefile writes, pages output, hard-fault rates | No | No | Not used |
| Memory compression | No | No | Not used |

`MemoryTelemetrySnapshot::physical_pressure` and `commit_pressure` in
[`crates/virtio-mem-core/src/demand.rs`](../crates/virtio-mem-core/src/demand.rs)
are used by the older threshold-based `DemandCalculator`, but they do not drive
the production raw `TargetDemandSource` path. They are ratios derived from the
same point-in-time totals, not additional operating-system pressure signals.

The current policy can react before physical exhaustion when available memory
or commit headroom falls below its configured reserve. It cannot determine
whether paging is already harming the workload, predict a rising working set,
or distinguish application demand from all resident uses. Windows
`PhysicalAvailable` includes standby, free, and zero pages, so reusable standby
cache is already counted as available. Active cache, modified pages,
applications, and kernel consumers are not distinguished in the physical-use
term; the separately collected system-cache value is ignored by policy.

## Microsoft-supported signal inventory

### Primary operating-system state

`CreateMemoryResourceNotification` exposes system-wide
`LowMemoryResourceNotification` and `HighMemoryResourceNotification` objects.
The Windows memory manager decides when each state is signalled. Microsoft
documents an intermediate range in which neither is signalled. These
notifications are the strongest supported categorical answer to whether
Windows considers memory low or high, but they provide neither a pressure
percentage nor a recommended byte allocation. Microsoft's guidance for the
intermediate range is to keep memory use constant, which supports treating it
as a shrink blocker rather than reclaim permission.

The notification condition is specifically available physical memory, not a
holistic commit or paging score. The collector should create both notification
handles once, query or wait through the service's existing cancellable loop,
report creation/query failures explicitly, and close both handles on shutdown.
Commit and paging evidence remain separate.

Sources: [CreateMemoryResourceNotification](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-creatememoryresourcenotification),
[QueryMemoryResourceNotification](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-querymemoryresourcenotification).

### Quantitative capacity and demand context

`GetPerformanceInfo` exposes current commit charge, commit limit, available
physical pages, system cache, and kernel pools. Microsoft defines available
physical memory as standby plus free plus zero pages: memory that can be used
immediately without a disk write. Commit charge is useful demand evidence, but
it may include address space not yet resident; the commit limit may also change
when a system-managed pagefile grows. It must not be treated as physical
working-set demand.

Sources: [PERFORMANCE_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-performance_information),
[GlobalMemoryStatusEx](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-globalmemorystatusex),
[Memory performance information](https://learn.microsoft.com/en-us/windows/win32/memory/memory-performance-information).

The documented `Memory` performance object adds useful detail:

- `Standby Cache Reserve Bytes` plus `Free & Zero Page List Bytes` isolates
  immediately reusable memory more explicitly than physical-used bytes.
- `Modified Page List Bytes` identifies pages that are not immediately reusable
  until written.
- `Pages Output/sec` measures pages written to disk to free physical space.
  Microsoft troubleshooting guidance calls it the best counter for indicating
  that insufficient RAM is causing paging.
- `Page Reads/sec`, `Pages Input/sec`, and hard faults provide supporting
  evidence, but Microsoft warns that they include activity unrelated to a
  pagefile and must not independently imply low memory.
- `Committed Bytes` and `Commit Limit` quantify commit risk.

Use reusable-memory and paging-rate observations as qualification evidence,
then derive any pressure threshold, safety margin, and observation window from
native results for the supported guest. Do not copy example diagnostic values
into product policy.

Sources: [RAM, virtual memory, and pagefile management](https://learn.microsoft.com/en-us/troubleshoot/windows-server/performance/ram-virtual-memory-pagefile-management),
[Determine the appropriate page file size](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/how-to-determine-the-appropriate-page-file-size-for-64-bit-versions-of-windows).

The RAM/pagefile troubleshooting article is explicitly a 32-bit Windows
overview. Its explanation that `Pages/sec` is frequently misinterpreted and
that `Pages Output/sec` isolates pagefile writes remains useful counter
semantics, but its historical sizing discussion is not a policy source for the
supported 64-bit Windows guest.

PDH is the supported programmatic consumer for performance counters. Rate
counters require at least two samples and an interval; the collector must use
formatted values and record sample continuity. `PdhAddEnglishCounterW` avoids
localized counter-name coupling.

Sources: [About performance counters](https://learn.microsoft.com/en-us/windows/win32/perfctrs/about-performance-counters),
[Collecting performance data](https://learn.microsoft.com/en-us/windows/win32/perfctrs/collecting-performance-data),
[PdhAddEnglishCounterW](https://learn.microsoft.com/en-us/windows/win32/api/pdh/nf-pdh-pdhaddenglishcounterw).

### ETW and Microsoft tooling

Windows Performance Recorder and ETW can capture memory-list changes and hard
fault detail. They are appropriate qualification and incident-analysis oracles,
especially for checking whether the lightweight collector classifies workloads
correctly. They are not initially part of the always-on control loop:
collection cost, provider stability, session ownership, and version support
would need explicit review.

Sources: [Introduction to Windows Performance Recorder](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/introduction-to-wpr),
[Recording for basic system diagnosis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/recording-for-basic-system-diagnosis).

Memory compression is relevant diagnostic context, but this review did not
identify a stable, documented, version-qualified public API or counter contract
appropriate for release-critical control. The first native capability probe
must inventory what is exposed on the supported Windows build. Until then,
compression is optional diagnostic evidence and cannot authorize reclaim.

## Signal roles

| Role | Signals | Policy use |
| --- | --- | --- |
| Urgent growth | Windows low-memory notification; dangerously low commit headroom; sustained pages output with low reusable memory | Raise a bounded host target immediately and block shrink |
| Quantitative demand baseline | Committed bytes, commit limit, current allocation, and a validated proportional buffer | Construct a candidate byte target; never add commit and resident demand together |
| Growth corroboration | Low reusable-memory lists, sustained page output, contextual hard-fault/input rates | Increase confidence or urgency only after native threshold and continuity qualification |
| Reclaim eligibility | Windows high-memory notification, sustained healthy reusable-memory lists, ample commit headroom, and absence of paging pressure | Permit only incremental host reclaim after a complete dwell window |
| Shrink blockers | Low or indeterminate notification, missing/unsupported required signals, commit stress, page output, modified-list backlog, discontinuity, or stale evidence | Hold allocation; do not calculate a lower target from the blocker itself |
| Diagnostics | Total page faults, cache and kernel pools, memory load, commit peak, compression, ETW/WPR detail | Explain and qualify decisions; no direct target authority initially |

The high-memory notification is necessary but not sufficient for reclaim. It
does not state how many bytes can be removed, and reclaim itself can create
pressure. Conversely, a low-memory notification is sufficient to prohibit
shrink and request bounded growth, subject to host and device safety.

## Target architecture

### Windows collector

The additive schema-v3 contract is implemented, and WN2 populates its paired
low/high notification group. The complete destination contains:

- low and high memory-resource notification states and API availability;
- existing physical and commit totals;
- standby reserve, free/zero, and modified-list bytes where supported;
- formatted `Pages Output/sec` and supporting hard-page rates with the sampling
  interval and warm-up status;
- per-signal support/error state so unavailable is not encoded as zero;
- existing producer identity, sequence, monotonic/wall timestamps, and bounded
  atomic publication.

The Windows service normalises units and counter semantics only. It does not
publish `desired` or infer host capacity. Reusable-memory and rate groups remain
explicitly unavailable until their later milestones.

### Host assessment and target construction

Add a versioned `WindowsPressureAssessment` between raw telemetry validation and
target estimation. It classifies each accepted sample as urgent grow, grow/hold,
neutral hold, reclaim eligible, or unavailable/fail-closed, and records the
evidence responsible. Classification and byte sizing remain distinct.

The first candidate sizing experiment uses current committed demand plus a
configurable proportional safety margin,
calibrated against Windows workloads and bounded by minimum, maximum, current
allocation, device geometry, and host reserve. The exact mapping from guest
commit to virtio-mem bytes is a hypothesis to validate in shadow mode, not a
Microsoft-prescribed formula. Categorical notification and paging signals set
urgency and block reclaim; they do not invent independent byte estimates.

The present fixed physical and commit reserves become:

1. an emergency growth/fail-safe fallback when richer supported signals are
   temporarily unavailable;
2. a hard lower safety guard while the new model is being qualified; and
3. migration inputs with explicit deprecation and configuration-version rules.

Fallback mode permits conservative growth from fresh basic counters but never
authorizes shrink. The configured minimum and safe floor remain independent of
any historical reserve setting.

### Preserved safety and ownership

Keep the existing raw transport, topology validation, compatibility
attestation, host headroom, alignment, `desired`/`requested`/`current`
separation, no-overlap reconciler, intent journal, restart recovery, latches,
bounded steps, and automatic-shrink safety gates. The global pool continues to
reserve host capacity before eventual multi-VM growth.

Redesign or retire after migration:

- the fixed-reserve `candidate` calculation as primary policy;
- primary-policy semantics of `VIRTIO_MEM_PHYSICAL_RESERVE_BYTES` and
  `VIRTIO_MEM_COMMIT_RESERVE_BYTES`;
- the legacy threshold `DemandCalculator` and `guest-stats` demand mode;
- any qualification claim based only on the old fixed-headroom behavior.

Keep `B` as a topology/identity check, not a demand signal. Keep memory load,
commit peak, cache, and pool counters as diagnostics unless later evidence and
contract review assign them a precise role.

## Staged implementation

The complete dependency-ordered implementation and qualification path is
normative in [roadmap.md](roadmap.md). `BACKLOG.md` owns the current task claim.
This research document defines signal semantics and architecture only; it does
not maintain a second milestone sequence.
