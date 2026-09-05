# Future Architecture: Demand, Arbitration, and Virtio-Mem Actuation

> **Status:** Phase 3 design baseline. This document describes the target
> architecture and research boundaries; it does not claim that the Phase 3
> global controller is implemented. Native telemetry and a version-1 local
> report calculator exist, but production publication does not.

## Purpose

The system separates three concerns that must not compete for authority:

1. **Measurement:** observe Windows and host memory conditions.
2. **Policy:** decide what memory allocation is safe and desirable globally.
3. **Actuation:** request and observe virtio-mem changes.

Phase 2 establishes the Windows demand-agent foundation while retaining the
current single-VM host controller and its conservative convergence rules. The
Virtio contract and pinned implementation sources establish host allocation
semantics; Phase 3 simulation can use that contract while installed-driver
diagnostics and live shrink behavior remain separately qualified.

## Target component model

```text
┌──────────────────────────────┐
│ Windows demand agent         │
│                              │
│ GlobalMemoryStatusEx         │
│ GetPerformanceInfo           │
│ Optional paging/trend data   │
│ Raw telemetry + pressure     │
└──────────────┬───────────────┘
               │ versioned demand report
               ▼
┌──────────────────────────────┐
│ Linux global RAM controller  │
│                              │
│ Host reserve and pressure    │
│ VM pool accounting           │
│ Telemetry/allocation join    │
│ Growth/reclaim arbitration   │
│ Allocation decisions         │
└──────────────┬───────────────┘
               │ per-VM target
               ▼
┌──────────────────────────────┐
│ Per-VM QEMU/libvirt adapter  │
│                              │
│ Validate XML and alignment    │
│ Set requested-size            │
│ Observe current convergence  │
└──────────────┬───────────────┘
               │ virtio-mem protocol
               ▼
┌──────────────────────────────┐
│ viomem.sys + Windows memory  │
│ manager                      │
│                              │
│ Block bitmap                 │
│ Plug/unplug physical memory  │
│ Report actual device state   │
└──────────────────────────────┘
```

### Responsibility rules

| Concern | Owner | Rule |
| --- | --- | --- |
| Windows memory measurement | Windows service | Publish fresh raw observations and optional guest-local pressure; do not calculate from guessed host allocation or allocate globally. |
| Global RAM pool | Linux controller | The only owner of cross-VM capacity and allocation accounting. |
| Per-VM actuation | QEMU/libvirt adapter and virtio-mem | Apply an aligned target asynchronously and report convergence. |
| Host safety | Linux controller and host adapter | Reserve host capacity and fail closed when evidence is stale or incomplete. |
| Driver mechanics | `viomem.sys` | Select physical ranges, interact with the Windows memory manager, and issue block requests. |

The report transport must carry VM/service identity, UTC and monotonic/session
ordering, a boot/service-session identifier, sequence/correlation identifier,
schema version, and source provenance. The controller must reject stale,
replayed, cross-VM, truncated, oversized, or unsupported-version records. A
filesystem spool additionally needs explicit ACL, retention/rotation, partial-
write recovery, and reader-handoff rules.

The Windows service must not invoke Linux commands, mutate libvirt state, or
open an undocumented driver control path as part of the initial design.

The current `win11_gpu` deployment is a fully trusted development/test guest
and Windows virtio-mem remains technology preview. QEMU does not completely
protect unplugged memory from guest access, so a hard QEMU/libvirt cgroup
memory limit is recommended defense-in-depth for this guest and mandatory for
any untrusted or production deployment.

## Phase 2 boundary

Phase 2 delivers a useful guest-side demand agent without breaking the current
resize path:

- collect native Windows memory telemetry;
- calculate a versioned demand state and target recommendation;
- retain host-side QGA health and `dommemstat` observation during migration;
- keep the existing one-VM host controller as the allocation and actuation
  authority;
- preserve byte units, block alignment, host headroom, and
  `requested != current` convergence suppression.

Phase 2 does **not** implement multi-VM arbitration, automatic global reclaim,
or direct `viomem.sys` IOCTLs.

M10c uses one explicit model: the host calculates the recommendation after
joining a fresh, versioned raw Windows telemetry envelope with alias-scoped
live libvirt `current`. The Windows service never guesses allocation, receives
a host-allocation feed, or acquires host-control authority.

Phase 2 permits one active controller/device on the development host. Per-VM
instances do not share an atomic reservation, so upstream multi-device support
does not authorize multi-controller actuation before the Phase 3 pool exists.

## Phase 3 global pool model

The global controller owns one accounting model:

```text
Physical RAM
├── Host reserve
│   ├── fixed host baseline
│   ├── filesystem/cache allowance
│   ├── emergency reserve
│   └── other host services
└── VM capacity
    ├── VM allocations
    └── unallocated pool
```

The initial equations are:

$$
VMCapacity = TotalRAM - HostReserve
$$

$$
PoolFree = VMCapacity - \sum VM.ActualAllocation
$$

`HostReserve` must not be defined as all of Linux `MemAvailable`. Reclaimable
cache is useful evidence, but consuming it as VM capacity can damage host
stability. The controller should combine a configured baseline with measured
host pressure and an emergency reserve.

The accounting value must be the observed active/plugged allocation, not a
promise represented only by `requested_size`. During convergence, requested
and actual values may differ; the difference remains unavailable for a new
allocation until the host observation confirms release.

## VM policy state

Each VM eventually exposes five ordered values:

```text
configured minimum ─ hard administrative floor
safe floor         ─ conservative pressure reclaim boundary
desired target     ─ normal operating recommendation
actual current     ─ host-observed active allocation
configured maximum ─ administrative/device ceiling
```

The Windows agent may calculate `desired target` and recommend a `safe floor`,
in local version-1 tests, but production target calculation moves to the Linux
controller after the M10c join. A safe floor is advisory until it has been
validated against workload history and actual virtio-mem convergence.

## Arbitration policy

Growth and reclaim are separate priorities:

- **Growth priority:** which VM receives free pool capacity first.
- **Reclaim priority:** which VM is considered first when capacity is scarce.

A high growth priority does not automatically mean a VM is never reclaimable.
The policy must also respect hard minimums, safe floors, stale reports,
in-flight operations, and host pressure.

Per-VM actuation authorization must also be bound to the reviewed live
domain/QEMU configuration. A static workload-review boolean is insufficient:
configuration fingerprint drift must revoke authorization and require review.
The fingerprint includes memory backend/page/NUMA properties, slot and VFIO
mapping budgets, active balloon resizing, incompatible workloads/devices,
topology, trust classification, and deployed versions.

Host-side `dommemstat` is pressure telemetry, not allocation authority:
`actual` is balloon state and can legitimately be below `unused` or
`available`. M9e must validate `last-update`; live alias-scoped libvirt
`current` remains the allocation input.

The reviewed Windows driver is event driven and has no obvious periodic retry
timer for a no-progress shrink. Automatic reclaim is not currently qualified;
M10b must add a default-off control and prove autonomous retry, safe bounded
same-target re-notification, or a controlled failed-shrink recovery path.

The initial global states are:

- `NORMAL`: pool capacity is healthy; honor desired targets where possible.
- `CAUTION`: pool is tightening; restrict low-priority growth.
- `PRESSURE`: pool is exhausted or host pressure is elevated; reclaim toward
  safe floors according to reclaim priority.
- `CRITICAL`: stop growth and prioritize host protection.
- `EMERGENCY`: apply the pre-approved emergency policy and preserve explicit
  operator visibility; no unbounded or undocumented forced action.

Hysteresis and block-sized decisions are required so the controller does not
oscillate between growth and reclaim.

## Confirmed `viomem.sys` integration facts

The upstream Windows driver source currently documents and implements the
following relevant behavior:

- `virtio_mem_config` includes `block_size`, `region_size`,
  `usable_region_size`, `requested_size`, and `plugged_size`.
- The driver maintains a bitmap representing memory blocks.
- `VIRTIO_MEM_F_ACPI_PXM` provides NUMA-aware virtio-mem negotiation; Windows
  chooses physical NUMA placement through its memory manager.
- `VIRTIO_MEM_F_UNPLUGGED_INACCESSIBLE` is negotiated because the driver does
  not access removed memory and the Windows memory manager guarantees removed
  memory is not accessed.
- Growth uses `MmAddPhysicalMemory` after an accepted block request.
- Removal uses `MmAllocateNodePagesForMdlEx` with
  `MM_ALLOCATE_AND_HOT_REMOVE`, then sends block-level unplug requests and
  updates the bitmap.
- The worker infers whether it should add or remove memory by comparing
  `requested_size` and `plugged_size`.
- The driver has a `STATE` request for synchronizing block state.

These facts support the design decision that the Rust service should not try to
select individual removable pages. The Windows memory manager and `viomem.sys`
are the mechanism; the service supplies policy-relevant observations.

The source creates a device interface, but this repository has not established
that a supported user-mode IOCTL or status API exists. No such API is part of
the Phase 2 contract. Any future interface requires a separate driver source,
build, signing, security, and live-runtime investigation.

## Driver project and integration boundary

The upstream `viomem` directory is a KMDF driver project, not a companion
user-mode service. Its solution includes the `viomem` kernel project and the
VirtIO/WDF library projects. The implementation is split across
`viomem/sys/viomem.c`, `Device.c`, `Driver.c`, `utils.c`, `viomem.h`, and
related protocol/build files.

`Device.c` creates the `GUID_DEVINTERFACE_VIOMEM` device interface, initializes
interrupt and virtqueue state, and starts a kernel worker thread. The reviewed
control path is driven by virtio configuration changes and the internal
worker/virtqueue; creating a device interface alone does not prove that an
application-facing IOCTL protocol exists. The Rust service therefore treats
this interface as a future investigation point, not a Phase 2 dependency.

The upstream build scripts invoke the Visual Studio/WDF project for Win10 and
Win11 targets and support x86, x64, and ARM64 configurations, with a separate
SDV-oriented build path. Any fork or driver modification requires a Windows
driver build environment, test/signing policy, installation and rollback
procedures, and live compatibility testing. It remains a separate kernel-
driver deliverable from this Rust service.

The preferred integration order is:

1. use native Windows APIs for demand telemetry;
2. use QEMU/libvirt for requested-target actuation and current-state
  observation;
3. correlate driver state through supported observation or tracing;
4. only then consider a narrowly scoped read-only driver status interface.

## State terminology and authority

The Phase 2 host contract uses libvirt XML fields `requested` and `current`.
The upstream driver uses `requested_size` and `plugged_size`. These fields are
the read-only device-configuration forms of the same requested and plugged
allocation under the Virtio memory-device specification. The reviewed Windows
worker reads them directly and compares them to choose plug or unplug work.
For the pinned stack, alias-scoped libvirt `current` is the authoritative host
allocation value and `requested != current` denotes an in-flight or stalled
operation.

Kernel-debug capture of the driver's `Memory config` record validates how the
installed binary received and acted on a notification, but it echoes device
configuration and is not a second accounting authority. It is diagnostic
evidence for live behavior and shrink recovery, not a prerequisite for global
pool simulation.

The repository's 1 GiB `MIN_HEADROOM_BYTES` rule remains a device safety
headroom invariant. It is not a universal Windows minimum-memory rule and must
not be presented as a substitute for a configured VM minimum or a host reserve.

## Staged adoption

1. **Phase 2 telemetry:** implement native collection and deterministic demand
   calculation without changing host actuation authority.
2. **State evidence:** pin the Virtio/QEMU/libvirt/driver source contract and
   build a correlated evidence harness; use explicitly approved kernel capture
   only for installed-driver diagnostics.
3. **Phase 3 arbitration:** add one Linux global pool model and simulated
   multi-VM arbitration before connecting live actuation.
4. **Controlled reclaim:** add trend-aware safe floors, one aligned step at a
   time, convergence waits, and stop-on-pressure behavior.
5. **Optional driver interface:** investigate a supported read-only driver
   status interface only if a concrete operational diagnostic need cannot be
   met by QEMU/libvirt observation and bounded tracing.

No automatic shrink policy should be enabled based solely on instantaneous
available memory or on an unverified driver assumption.

## References

- Upstream driver source: `virtio-win/kvm-guest-drivers-windows/viomem/sys/viomem.c`
- Upstream types: `virtio-win/kvm-guest-drivers-windows/viomem/sys/viomem.h`
- Current host contract: [`api-contract.md`](api-contract.md)
- Current execution plan: [`roadmap.md`](roadmap.md)
