# M10aX Driver Status-Interface Feasibility

**Status:** Proposal complete; implementation not authorized

**Reviewed:** 2026-09-08

**Target baseline:** virtio-win `mm314`, installed `viomem.sys`
`100.102.104.29400`, Windows 11 x64

## Decision

A narrow, read-only KMDF status interface is technically feasible, but it
should remain a separate upstream/forked-driver project and should not be
implemented or consumed by this repository yet.

M10a3 supplies the required concrete diagnostic need. During a one-block
shrink, host `requested` remained below `current` for 300 seconds while the
driver stayed running and Windows telemetry stayed healthy. A later 1 GiB
shrink made partial progress and then remained divergent. The bounded,
signature-verified kernel captures produced no matching `Memory config`
record. Existing host observation can identify convergence, partial progress,
and a stall, but it cannot determine:

1. whether the installed driver received a configuration notification;
2. whether its worker selected grow, shrink, synchronization, or no-op;
3. whether Windows returned no removable range, the virtqueue/device rejected
   work, or a later step failed; or
4. whether another same-target notification reached the driver.

The operational use is to classify a failed M10b same-target notification and
choose whether to stop, attempt the separately bounded abandon-to-current
recovery, or escalate to graceful domain recreation. None of these fields is
required for host allocation accounting or safe actuation. Alias-scoped live
libvirt `current` remains authoritative. The proposed interface must never
become a resize command, a second allocation source, or a prerequisite for
M11 simulation.

## Evidence and source constraints

The pinned [`mm314` `Device.c`](https://github.com/virtio-win/kvm-guest-drivers-windows/blob/mm314/viomem/sys/Device.c)
creates `GUID_DEVINTERFACE_VIOMEM`, but it creates no default I/O queue and
registers no device-control callback. Its
[`ProtoTypes.h`](https://github.com/virtio-win/kvm-guest-drivers-windows/blob/mm314/viomem/sys/ProtoTypes.h)
holds worker state, a block bitmap, and a cached memory configuration. The
[`viomem.c` worker](https://github.com/virtio-win/kvm-guest-drivers-windows/blob/mm314/viomem/sys/viomem.c)
reads the device configuration after wakeup, then chooses add or remove by
comparing `requested_size` with `plugged_size`. Removal may legitimately make
no progress when Windows cannot return a removable range.

Microsoft documents that KMDF device access is controlled by the device
security descriptor and INF configuration, and recommends validating access
for sensitive IOCTLs. A new driver package also needs an independently
verified kernel-mode signature; attestation signing is suitable only for
testing scenarios and is not Windows certification. See Microsoft's
[KMDF device-access guidance](https://learn.microsoft.com/windows-hardware/drivers/wdf/controlling-device-access-in-kmdf-drivers),
[driver security checklist](https://learn.microsoft.com/windows-hardware/drivers/driversecurity/driver-security-checklist),
and [driver-signing options](https://learn.microsoft.com/windows-hardware/drivers/dashboard/driver-signing-offerings).

## Proposed version-1 surface

Add one buffered, query-only IOCTL to an upstreamable virtio-win change. Reuse
the existing device-interface GUID only if upstream confirms that it is an
application contract; otherwise publish a new status-specific interface GUID
so older installations cannot be mistaken for v1-capable drivers.

The request contains only:

- `struct_size` (`u32`);
- `interface_version` (`u16`, exactly `1`);
- `operation` (`u16`, exactly `GET_STATUS`); and
- zeroed `reserved` bytes.

The fixed-width response contains no pointers or variable-length payloads:

- response `struct_size`, interface version, and capability flags;
- a monotonically increasing snapshot sequence;
- device `address`, `region_size`, `usable_region_size`, `block_size`,
  `requested_size`, and `plugged_size`, all unsigned bytes;
- worker lifecycle (`initializing`, `running`, `stopping`, or `failed`);
- last selected action (`none`, `synchronize`, `grow`, `shrink`, or `no-op`);
- last action outcome (`not-attempted`, `complete`, `partial`, `no-progress`,
  `device-busy`, `device-nack`, `device-error`, `Windows-error`, or
  `internal-error`);
- last action's requested, attempted, and completed byte counts;
- sanitized last Windows `NTSTATUS` and virtio response category;
- configuration-wakeup, grow-attempt, shrink-attempt, partial-progress,
  no-progress, and failure counters; and
- monotonic interrupt/wakeup and action-completion timestamps.

All integer layout, byte order, offsets, enum values, reserved bytes, and
overflow behavior must be specified in a standalone public ABI header with
compile-time size/offset assertions. Unknown capability bits and reserved
fields are ignored only when their documented compatibility rule allows it.
Version or size mismatch returns an explicit unsupported/revision error; the
driver never silently truncates a response.

The snapshot is cached diagnostic state. The IOCTL callback must copy one
internally consistent snapshot under a dedicated lock and complete
synchronously. It must not read virtio configuration, inspect the bitmap,
wake the worker, wait on `hostAcknowledge`, allocate unbounded memory, or send
a request to the device. The interface exposes aggregate byte counts and
outcomes only—never physical ranges, bitmap contents, kernel addresses, or
memory contents.

## Driver instrumentation required

The existing worker must update the snapshot at explicit boundaries:

1. increment the wakeup counter when the DPC signals the worker and record
   when the worker consumes that signal;
2. publish the freshly read configuration before branch selection;
3. record the selected branch and attempted byte count;
4. classify no removable MDL as `no-progress`, less-than-requested removal as
   `partial`, and exact completion as `complete`;
5. preserve device `BUSY`, `NACK`, and `ERROR` separately from Windows memory
   manager and internal failures; and
6. increment the snapshot sequence only when a complete new snapshot is
   published.

This instrumentation is essential. Merely returning the cached
`MemoryConfiguration` would duplicate host values and would not meet the
diagnostic need.

## Security and availability

- Use `METHOD_BUFFERED`; validate exact input/output sizes before access and
  zero-initialize the full response, including padding.
- The only accepted operation is `GET_STATUS`; reject all unknown codes,
  nonzero reserved fields, malformed sizes, and unsupported versions.
- Assign an explicit device security descriptor through the package. Version
  1 permits read access only to `SYSTEM` and built-in Administrators. It does
  not grant all `LocalService` processes access.
- Require read access in the IOCTL definition and validate requestor access in
  the handler. Reject kernel pointers and neither trust nor echo caller data.
- Rate-limit only in user mode if needed; the synchronous handler must remain
  constant-space and bounded independently of caller behavior.
- The diagnostic reader uses overlapped I/O, a fixed two-second deadline, and
  cancellation. Timeout, cancellation, access denial, or device removal makes
  that optional sample unavailable and must not block controller recovery.
- Do not expose a write/configuration IOCTL, debug-filter control, bitmap dump,
  retry trigger, same-target notification, or resize operation under this
  interface.

The current Windows service runs as `LocalService`, so it must not consume v1
directly. Initial use is an elevated, one-shot Rust diagnostic tool whose
output can be added as optional M10a2 evidence. Any later unattended consumer
requires a separate review of a stable per-service SID and a narrower ACL; it
must not broaden access to the LocalService group.

## Compatibility rules

- The driver package, ABI header, and diagnostic reader are versioned and
  released together; their source revision, binary/catalog hashes, signer,
  timestamp, target architecture, WDF version, and supported Windows builds
  are recorded.
- Opening the interface and issuing the version query is capability discovery.
  `ERROR_FILE_NOT_FOUND` means the installed driver has no status interface;
  revision mismatch means the reader must stop without fallback to an
  undocumented control code.
- v1 fields keep their offsets and meanings permanently. A larger future
  response uses a new declared size and capability bit; incompatible semantics
  require a new interface version.
- The reader verifies device instance identity and driver version before
  accepting a sample. A multi-device system must bind each result to the exact
  PnP instance; it must never merge by friendly name.
- Host `requested`/`current` and M10a2 identity, clocks, units, and ordering
  remain required. Driver status is optional correlated evidence and is never
  accepted on its own.

## Build, signing, and supply-chain gate

This repository must not vendor or compile the fork. A separate driver project
must:

1. pin the virtio-win source revision and all VirtIO/WDF dependencies;
2. produce reproducible build metadata and hashes for the SYS, INF, CAT, public
   ABI header, symbols, and source archive;
3. pass code review, compiler warnings-as-errors, Static Driver Verifier/code
   analysis, dependency/license review, and malware scanning;
4. sign the complete package for disposable-guest testing and verify the
   catalog and binary chain before installation; and
5. require HLK/certification and an explicit support decision before anything
   beyond a test environment. Attestation signing alone is not a production
   support claim.

Signing keys and certificates never enter this repository, scripts, logs, or
command lines. Driver building and signing remain outside the normal Rust
build/test gate.

## Test gate

### Hermetic driver and reader tests

- exact v1 layout/offset tests on every supported architecture;
- valid query and exact response decoding;
- short, oversized, null, unaligned, wrong-version, unknown-operation,
  nonzero-reserved, and output-buffer-too-small requests;
- access denied for standard user and `LocalService`, access granted only for
  the declared administrators and `SYSTEM` principals;
- concurrent queries during wakeup, grow, shrink, PnP stop, and removal with
  no torn snapshots or use-after-free;
- counter saturation/wrap behavior and timestamp monotonicity;
- classification tests for complete, partial, no-progress, every virtio
  response, Windows failure, initialization failure, and shutdown;
- fuzzing of the IOCTL input and output decoder; and
- reader rejection of the wrong device instance, stale sample, unknown
  capability, unsupported version, and inconsistent byte geometry.

### Disposable-guest integration tests

Use a dedicated snapshot/rebuildable Windows 11 guest with no production data.
Record the baseline package, PnP instance, Secure Boot/test-signing state,
device state, and recovery media before installation. Then verify:

- clean install, boot, interface ACL, query, sleep/resume, shutdown/start,
  driver restart, surprise removal, and uninstall;
- zero, converged grow, converged shrink, one-block no-progress shrink, and
  larger partial-progress shrink;
- correlation between each host operation, M10a2 evidence, and status sequence
  without overlapping resize requests;
- a query storm cannot delay the worker or change guest/host memory state;
- malformed and unauthorized queries leave counters and device state intact;
  and
- the original signed package can be restored and reaches the recorded
  converged baseline after a reboot when required.

Only after those tests pass may a separately approved protected-guest trial be
proposed. That approval must name the exact signed hashes, VM/device, install
and reboot effects, bounded memory operation, evidence paths, recovery target,
and rollback commands.

## Installation and rollback gate

Installation is not authorized by this proposal. A future runbook must capture
the installed OEM INF name, package/binary versions and hashes, signer,
certificate chain, PnP instance, device status, live host state, controller
state, and original package media before mutation. It must stop the controller,
stage and install one named signed package, reboot if required, and verify the
exact device/interface before any memory test.

Rollback must be rehearsed first. It removes only the named experimental OEM
package, reinstalls the recorded original signed package, reboots if required,
verifies `viomem` and the PnP instance, restores the predeclared persistent
memory target, waits for `requested == current`, checks fresh Windows/host
telemetry, and restores the controller to its recorded prior state. Failure to
restore the driver or convergence stops the procedure for operator recovery;
it never triggers repeated package changes or overlapping resize requests.

## Go/no-go gates

Implementation remains **No-Go** until all of the following are true:

- an M10b failure remains operationally ambiguous after bounded host evidence,
  and its driver-side classification changes a documented recovery or
  escalation decision rather than merely adding curiosity data;
- an upstream maintainer or owned fork accepts responsibility for the ABI and
  driver lifecycle;
- a disposable guest, signing route, exact original package, and tested
  rollback are available;
- security review approves the v1 fields, IOCTL validation, and administrator-
  only ACL; and
- the complete hermetic and disposable-guest test plan is funded.

If those gates are met, the first implementation deliverable is the external
driver ABI and its tests—not Rust service integration and not a protected-
guest install. If they are not met, retain bounded host/M10a2 evidence and
graceful domain recreation as the operator-approved terminal recovery path.
