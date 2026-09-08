# Upstream virtio-mem Audit

**Reviewed:** 2026-09-05  
**Documentation snapshot:** `virtio-mem.gitlab.io` commit
[`970413d317e60ee547a98ccddf5b18555e9fc506`](https://gitlab.com/virtio-mem/virtio-mem.gitlab.io/-/commit/970413d317e60ee547a98ccddf5b18555e9fc506)  
**Validated stack:** QEMU hypervisor 10.1.0, libvirt/QEMU API 11.10.0,
Windows `viomem.sys` `100.102.104.29400` corresponding to upstream `mm314`

This audit records upstream constraints that affect this repository. It is a
design and backlog input, not evidence that the corresponding protections are
already implemented. The live `win11_gpu` guest is a fully trusted,
development/test-only KVM workload. Windows virtio-mem remains upstream
technology preview and this project does not claim production support.

## Sources reviewed

- The [virtio-mem overview](https://virtio-mem.gitlab.io/) and its QEMU,
  libvirt, Linux, feature, and developer guides at the pinned documentation
  revision above.
- The [QEMU virtio-mem guide](https://virtio-mem.gitlab.io/user-guide/user-guide-qemu.html),
  including device/backend properties, memory-slot limits, incompatible
  workloads, balloon interaction, migration, and asynchronous resize behavior.
- The [libvirt virtio-mem guide](https://virtio-mem.gitlab.io/user-guide/user-guide-libvirt.html),
  [domain XML reference](https://libvirt.org/formatdomain.html#memory-devices),
  and [`dommemstat` reference](https://www.libvirt.org/manpages/virsh.html#dommemstat).
- The upstream [QEMU Guest Agent protocol reference](https://www.qemu.org/docs/master/interop/qemu-ga-ref.html)
  and QGA schemas for QEMU 9.1, 10.1, and current master.
- The virtio-win [`viomem` driver source](https://github.com/virtio-win/kvm-guest-drivers-windows/tree/master/viomem)
  and the source corresponding to the installed signed driver.
- The [Virtio 1.2 memory-device specification](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html),
  the [libvirt memory-device contract](https://libvirt.org/kbase/memorydevices.html),
  and QEMU's [`MEMORY_DEVICE_SIZE_CHANGE`](https://gitlab.com/qemu-project/qemu/-/blob/master/qapi/machine.json)
  contract.
- Microsoft's [DebugView](https://learn.microsoft.com/sysinternals/downloads/debugview)
  and [kernel debug-message filtering](https://learn.microsoft.com/windows-hardware/drivers/debugger/reading-and-filtering-debugging-messages)
  documentation.

The website is useful design guidance, but it is not a release manifest. It
simultaneously describes Windows support as unstable technology preview and
contains older text saying no public Windows release exists. Compatibility
claims therefore have to be pinned to the deployed QEMU, libvirt, and driver
versions and re-audited after upgrades.

## Findings and repository decisions

### 1. `dommemstat` is useful telemetry, not a total-allocation authority

Libvirt defines `actual` as the current balloon value. QEMU documents that
balloon accounting excludes memory provided through virtio-mem. Consequently,
`actual` is not a valid upper bound for `unused` or `available`, and it is not
the selected virtio-mem device's current allocation. The live observation
`available > actual` is therefore not, by itself, malformed data.

M9e/TASK-025 corrected the parser and now:

- parse and enforce explicit freshness, including missing, stale, future, and
  non-advancing timestamps;
- stop rejecting `unused` or discarding `available` solely because either is
  greater than balloon `actual`;
- keep live alias-scoped libvirt `current` authoritative for virtio-mem
  allocation; and
- replace the Bash preview's QGA-only policy path with a Rust decision command
  that uses the same source, freshness, and validation logic as the controller.

`dommemstat` remains the configured source used by the development controller
and is freshness-qualified for policy input. ISSUE-006 is resolved; live XML
`current` remains the separate allocation authority.

### 2. `guest-get-memory-stats` is not an upstream QGA command

The command is absent from the upstream QGA schemas reviewed for QEMU 9.1,
10.1, and master. Installing or upgrading an upstream QGA package is therefore
not a supported way to obtain it. The repository's request/parser code is
retained only as an experimental custom/downstream adapter and test boundary.
It may be enabled only after the operator identifies and validates the exact
guest-agent implementation that provides that extension.

QGA remains supported for health and identity operations such as
`guest-info`. The host's default memory-stat source remains `dommemstat`,
subject to M9e freshness and semantics work. No Windows production worker
opens the QGA channel.

### 3. The compatibility attestation is broader than two device properties

`dynamic-memslots=on` and `unplugged-inaccessible=on` are necessary inputs,
but they are not a complete compatibility proof. M9d/TASK-019 now binds all
reviewed evidence to a versioned domain/QEMU fingerprint, including:

- vDPA, RDMA migration, VFIO-NVMe, `mlock`, encrypted/secure virtualization,
  and simultaneous virtio-balloon inflation/deflation or set-memory activity;
- vhost-user backend and version, ordering, and memory-slot capacity, including
  `max_mem_regions >= 509` where dynamic memory slots require it;
- libvhost-user/QEMU compatibility versus unsupported DPDK/SPDK cases;
- VFIO DMA-mapping budgets and any required block-size increase;
- memory backend type, page size, sparse-file behavior, `reserve`,
  preallocation, sharing, core-dump, and NUMA placement; and
- deployed QEMU, libvirt, machine type, and guest-driver versions.

The 2026-09-05 `win11_gpu` review remains historical evidence for that exact
trusted development configuration. M9d subsequently completed the full
attestation and drift guard; deploying a new build requires generating and
protecting an attestation for the then-current live configuration.

### 4. Trust and host containment are explicit deployment inputs

QEMU does not yet provide balloon-like protection against a guest accessing
all unplugged memory. For an untrusted or production guest, a hard QEMU/libvirt
cgroup memory limit and the completed compatibility attestation are mandatory
prerequisites. For the fully trusted development/test `win11_gpu` guest, a
hard limit is strongly recommended defense-in-depth rather than a release gate.
That exception must not be generalized to another VM.

### 5. Windows shrink retry behavior is not qualified

The generic QEMU guide says guests usually retry incomplete requests. The
reviewed Windows `viomem` worker is interrupt/event driven and exposes no
obvious periodic retry timer. If an unplug attempt makes no progress, the host
may continue to observe `requested != current` without another driver wakeup.
This is an inference from source and requires live evidence.

M10b/TASK-022 now includes Windows shrink qualification. Automatic shrinking
must remain disabled by default until testing proves one of these bounded
recovery models:

1. the installed driver retries autonomously;
2. an idempotent same-target re-notification safely wakes it; or
3. the controller uses a documented failed-shrink recovery path.

The existing code does not yet implement this default-off shrink switch; the
milestone records the required change and evidence.

### 6. Phase 2 supports one active controller/device topology

Upstream supports multiple virtio-mem devices, commonly one per vNUMA node.
The current per-instance controller has no global reservation coordinator, so
multiple active instances could race against the same host headroom. Phase 2
is therefore restricted to one active controller managing one explicitly
named VM/device alias on this development host. Multi-controller or
multi-device automatic actuation is unsupported until M11 adds global pool
arbitration.

### 7. M10c uses a host-side allocation join

The architecture choice is now fixed: Windows will publish a fresh, versioned
raw telemetry envelope; the host will join it with alias-scoped live libvirt
`current` and calculate the desired target. Windows must not guess allocation,
receive host-control authority, or invoke host tools. Demand report v1 remains
a local calculator/test foundation and is not production-ingestion ready.

### 8. Host allocation semantics do not depend on driver debug capture

The Virtio memory-device specification defines `requested_size` and
`plugged_size` as read-only device-configuration fields: the device changes the
request and must update the plugged count to reflect block state. The reviewed
virtio-win worker reads those fields directly and compares them to select plug
or unplug work. QEMU/libvirt expose requested intent and guest-cooperative
allocation as alias-scoped `requested` and `current`; QEMU emits a memory-device
size-change event when the guest changes the provided size.

The repository therefore treats live libvirt `current` as the authoritative
host allocation value for the pinned stack. The driver's `Memory config` debug
record echoes device configuration; it can validate notification timing,
installed-binary behavior, and failure diagnosis, but it is not an independent
accounting source. Missing user-mode driver telemetry no longer blocks M10c,
M10d, or hermetic M11 simulation.

Bounded kernel capture remains optional and operator-approved. A qualification
attempt must account for Windows debug-print filtering as well as the capture
process and temporary `Dbgv.sys`; a successful no-resize tool run alone does
not prove that informational viomem records can reach the capture buffer. It
must not silently enable boot logging, persist a debug-filter registry change,
restart the driver, or reboot the guest. Automatic shrink remains blocked on
M10b behavioral and recovery evidence, regardless of whether diagnostic trace
is available.

## Preserved invariants

The audit did not change these validated rules:

- memory is represented internally as checked canonical bytes;
- every target is positive, within device size/headroom, and block aligned;
- observed `requested=current=0` is a valid fully unplugged state;
- `requested != current` suppresses overlapping requests;
- live alias-scoped libvirt/QMP state is refreshed before actuation; and
- host reserve and device headroom remain independent safety gates.

The shared 1 MiB minimum block validation is also retained. Although the
website describes the configured block as greater than 1 MiB, the deployed
QEMU implementation accepts and enforces a minimum of 1 MiB; deployed behavior
is the compatibility boundary for this repository.
