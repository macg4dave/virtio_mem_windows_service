# Known Issues

## Open Issues

| ID | Description | Status | Component | Priority |
| --- | ----------- | ------ | --------- | -------- |
| ISSUE-002 | Hysteresis tuning for memory allocation | Open | Linux | Medium |
| ISSUE-003 | Error handling for libvirt communication | Open | Linux | High |
| ISSUE-004 | Full-device virtio-mem test risked exhausting host memory | Open; safety guard added 2026-08-18 | Host validation | Critical |
| ISSUE-008 | Classic Event Log text rendering is unreliable without a registered message resource; XML `EventData` contains the correct bounded message | Open; XML query documented | Windows observability | Medium |
| ISSUE-011 | Signed `viomem.sys` exposes no supported user-mode diagnostic query; its state message is filtered kernel-debug output | Open diagnostic limitation; does not block host allocation accounting or simulation | Windows observability | Medium |
| ISSUE-013 | M10d v2 identity/provenance and read-side replay/size checks are implemented, but the JSON-lines sink still lacks ACL provisioning, durable acknowledgement/handoff, and retention/rotation | Open; M10d partially addressed | Demand delivery | High |
| ISSUE-015 | Windows shrink retry behavior plus active-controller rejection, non-convergence, reboot, cancellation, and restart paths lack a complete deterministic/live recovery matrix | Open; M10b, automatic shrink must become default-off until qualified | Host recovery | High |

M10a3 live evidence on 2026-09-07 narrowed ISSUE-015: one 2 MiB growth
converged, but the recovery shrink remained at `requested=1073741824`,
`current=1075838976` for 60 ordered samples over 300 seconds. No second request
was sent. This proves the no-progress case is real on the installed Windows
driver and strengthens the M10b default-off and bounded-recovery requirement.
A follow-up 3 GiB-to-2 GiB probe removed 257 blocks (514 MiB) immediately but
left 255 blocks (510 MiB) above target for the rest of the same bound. The
larger request therefore rules out a simple minimum-control-size explanation
and provides concrete partial-progress-without-retry evidence.

The selected M10b qualification policy now bounds this risk explicitly: exact
same-target notifications after 30/60/120 seconds without block progress, no
more than three notifications, and an immutable 300-second deadline. A stall
is latched without exiting the worker. Separate live evidence must prove that
re-notification wakes the driver and that a one-shot abandon-to-current request
converges safely after two stable samples. Until both pass, automatic shrink
and re-notification remain independently disabled by default.

## Resolved Issues

| ID | Description | Status | Fix Reference | Date Resolved |
| --- | ----------- | ------ | -------------- | ------------- |
| ISSUE-012 | Production Windows telemetry samples were discarded pending the selected host-side allocation join | Resolved | TASK-020 raw publisher and fresh alias-scoped `current` host join | 2026-09-08 |
| ISSUE-001 | Host controller depended on unavailable `guest-get-memory-stats` | Resolved by configurable `dommemstat` default; upstream audit confirms the command is not upstream QGA and the retained adapter is custom/experimental | `host/src/dommemstat.rs`, `VIRTIO_MEM_STATS_SOURCE` config | 2026-08-18; clarified 2026-09-05 |
| ISSUE-005 | Virtio-mem rollback left `requested` and `current` divergent after the earlier 1 GiB test | Resolved after the updated Windows driver was installed; fresh XML reports `requested=0 KiB` and `current=0 KiB` | Fresh read-only `virsh dumpxml win11_gpu` convergence check | 2026-08-18 |
| ISSUE-007 | Invalid service configuration was loaded before SCM dispatcher attachment, causing Windows error 1053 without status or Event Log context | Resolved by dispatching SCM before configuration loading; live invalid-config recovery emitted event 2000 and exit code 1 | `windows/src/main.rs` startup-route regression and M7 live validation | 2026-09-04 |
| ISSUE-009 | Shared virtio-mem state validation rejected the live fully-unplugged `requested=current=0` state | Resolved by allowing zero observed state while retaining positive-target validation | `VirtioMemState` and live XML regression tests; M9 live CLI validation | 2026-09-05 |
| ISSUE-010 | Host policy rejected a converged allocation below its configured minimum, preventing the installed controller from bootstrapping a fully unplugged device | Resolved with one aligned request to the configured minimum; normal policy remains one block at a time and above-maximum state fails closed | `plan_resize` regression test and M9b live zero-to-1-GiB systemd convergence | 2026-09-05 |
| ISSUE-014 | Static workload approval omitted audited configuration and could survive drift | Resolved by a version-1 SHA-256 attestation checked against fresh allocation-neutral domain XML/QEMU argv, QMP properties/version, libvirt version, and bound review declarations before every resize | `host/src/attestation.rs` exact-match, tamper, allocation-progress, and drift tests | 2026-09-07 |
| ISSUE-006 | `dommemstat` treated balloon `actual` as a whole-guest bound and ignored `last-update` | Resolved by provenance-only `actual`, required `unused`/`available`, bounded freshness/future skew, and strict per-source advancement | `host/src/dommemstat.rs` injected-clock and semantic regression tests; TASK-025 | 2026-09-08 |

### M8/V1 read-only evidence — 2026-08-18

- `guest-info` succeeded against `win11_gpu` over the connected
  `org.qemu.guest_agent.0` channel.
- QGA version is `110.0.2`; `guest-get-memory-stats` remains unavailable.
- Three `dommemstat` samples succeeded with valid `actual`, `unused`, and
  `available` fields, so the configured host fallback is usable.
- This evidence does not prove Windows SCM state, QGA Windows service ACLs,
  or native driver memory-stat support.

### M8/V1 fresh read-only evidence — 2026-09-04

- One approved privileged batch read only `qemu:///system` and `win11_gpu`;
  no VM, service, XML, or memory state was changed.
- Three QGA `guest-info` requests succeeded in 77–124 ms against QGA
  `110.0.2`; ping, Windows 11 x64 OS identity, and hostname `ICE101` also
  succeeded.
- QGA still reports `guest-get-memory-stats` as unavailable. Three
  `dommemstat` samples succeeded in 84–129 ms with numeric `actual`, `unused`,
  and `available` fields.
- Live XML reports the host channel connected and `ua-virtiomem0` converged at
  `requested=current=0`, with a 20 GiB maximum and 2 MiB block size.
- The Windows service uses native telemetry and does not open the QGA pipe.
  An isolated `qemu-ga` stop/start recovered, followed by a graceful QGA-mode
  guest reboot and a complete successful post-reboot probe. Windows reported
  the new boot time as 23:54:19.
- The reboot-initiating QGA request lost its response as the guest shut down;
  recovery was established from the new Windows boot time, running QGA
  service, and subsequent host-side QGA/fallback/XML checks rather than from
  the initiating command response alone.

### V2 live virtio-mem inspection — 2026-08-18

- `ua-virtiomem0` is the unique selected virtio-mem device.
- Live XML reports a 20 GiB maximum, 2 MiB block size, and converged
  `requested=current=1 GiB` across three read-only rechecks.
- Shared memfd backing is present, but `dynamic-memslots` and
  `unplugged-inaccessible` are not exposed in the captured XML; compatibility
  remains unknown pending a supported QEMU/libvirt inspection path.

### M9 live Rust XML-adapter evidence — 2026-09-05

- The CLI snapshot matched the direct live XML SHA-256
  `04d1b8f989ea49354038c2576530d0a3c73798b4531a21938fb1fc15ef2fb1d8`.
- Validation selected `ua-virtiomem0` exactly and reported
  `size_bytes=21474836480`, `block_size_bytes=2097152`, and
  `requested_bytes=current_bytes=0`.
- A nonexistent alias was rejected. A one-block dry run without `--apply`
  failed closed because live compatibility evidence remains unknown.
- The post-check live XML hash was identical; no resize or XML mutation was
  issued. M9a remains responsible for compatibility and workload evidence.

### M9a live compatibility evidence — 2026-09-05

- QMP reported `dynamic-memslots=true` and
  `unplugged-inaccessible=on` for `/machine/peripheral/ua-virtiomem0`.
- The device's 2 MiB block matches the host's 2 MiB THP PMD size, and native
  QEMU arguments report `mem-lock=off`.
- Three VFIO devices map to an NVIDIA GPU, its audio function, and an AMD USB
  controller; none is VFIO-NVMe. No RDMA or vhost-user indicator was found,
  and the operator confirmed those workload dependencies are not intended.
- The Rust CLI emitted the exact 2 MiB dry-run argument vector without
  `--apply`. Before/after XML SHA-256 values matched at
  `29878e19597b6ef69f5b18a3490f4804f4fe11710d35d5052cdfdf00ecd7739d`.

### M10a read-only Windows driver discovery — 2026-09-05

- `ice101.lan` runs signed Red Hat `viomem.sys` version
  `100.102.104.29400` for `PCI\\VEN_1AF4&DEV_1058`; the device is started and
  the kernel service is running.
- `pnputil`, the service and Enum registry trees, Event Log enumeration, and
  registered provider enumeration expose identity and lifecycle metadata but
  no `requested_size`, `plugged_size`, block bitmap, or equivalent status.
- The contemporaneous upstream `mm314` tag has the same 8 December 2025 date
  as the installed driver. It creates `GUID_DEVINTERFACE_VIOMEM`, but defines
  no I/O queue, device-control callback, IOCTL, WMI schema, or performance-
  counter surface.
- The upstream worker formats both state fields in a `Memory config` debug
  message. `EVENT_TRACING` is commented out, so the shipped path uses kernel
  debug print rather than a registered WPP provider; the format string is
  present in the installed binary. DbgViewCLI is not persistently installed;
  it is staged only for separately approved bounded captures.
- `systeminfo` reports aggregate total physical memory, but that value is not
  accepted as driver-state evidence because it cannot distinguish requested
  from plugged state or identify the selected virtio-mem device.
- A fresh host snapshot remained converged at
  `requested=current=1073741824` bytes while Windows reported 9,148 MB of
  aggregate physical memory. This is a useful baseline, not cross-layer field
  mapping evidence.
- No resize, reboot, driver/service change, trace session, registry write, or
  host mutation was performed. The missing query limits installed-driver
  diagnosis but no longer blocks the source-established host allocation
  contract.
- Microsoft's signed Sysinternals `dbgviewcli.exe` is the preferred optional
  capture candidate because it can bound and filter kernel `DbgPrint` output.
  It requires Administrator rights and auto-loads `Dbgv.sys`; informational
  messages may also be filtered before capture. Tool qualification and any
  resize require explicit scopes, and the first attempt must not enable boot
  logging, persist debug-filter changes, restart the driver, or reboot.

### M10a1 bounded capture qualification — 2026-09-05

- Microsoft-signed DbgViewCLI 5.02 passed Windows signature verification and
  matching RHEL/Windows SHA-256 checks before execution.
- A 60-second capture used kernel-only mode, a `Memory config` include filter,
  a 1,000-line bound, and a 1 MiB log bound. It stopped on time without resize,
  boot logging, debug-filter changes, `viomem` restart, or reboot.
- No matching line was captured. This qualifies the bounded tool lifecycle but
  does not prove that informational `viomem` output can pass the current Windows
  debug-print filter.
- DbgView initially left its working-directory `Dbgv.sys` image locked after
  deleting the SCM entry. A separately approved two-second recovery capture
  used DbgViewCLI's normal unload path, after which the image, executable,
  process, and `Dbgv` service entry were absent. One empty Sysinternals parent
  registry key remains scheduled for exact cleanup; it has no values or debug
  settings.

### M10a allocation-contract review — 2026-09-05

- Virtio 1.2 defines `requested_size` and `plugged_size` as read-only device
  configuration, and the pinned virtio-win worker reads those values directly.
- QEMU/libvirt expose requested intent and guest-cooperative allocation as
  alias-scoped `requested` and `current`; live libvirt `current` remains the
  host accounting authority.
- Driver debug output echoes device configuration and is useful for diagnosing
  notification, branch, and failure behavior, but is not an independent state
  source. M10c/M10d and hermetic M11 simulation no longer depend on capture.
- Automatic shrink remains blocked on M10b recovery evidence. Any live test
  must use a named recovery target and must not claim shrink is guaranteed
  rollback.

### Whole-roadmap audit — 2026-09-05

- Production Windows startup constructs `NativeTelemetryWorker`, not
  `DemandServiceWorker`; validated samples are currently discarded and no
  demand report is published.
- `DemandAgent` requires a caller-supplied current allocation, while fresh live
  libvirt state is host-owned and no supported Windows driver query exists.
- Demand report version 1 has no timestamp, VM/session identity, sequence, or
  allocation provenance. Its JSON-lines publisher is append-only and has no
  retention, rotation, acknowledgement, or atomic handoff contract.
- The installed host controller uses a static workload-review boolean. Fresh
  XML/QMP checks do not establish that the reviewed workload/device set is
  unchanged, so M9d will bind approval to a configuration fingerprint.
- These gaps block production report ingestion and expansion of automatic
  actuation, but they do not invalidate the completed M7–M9b evidence.

### Upstream virtio-mem audit — 2026-09-05

- The reviewed upstream QGA schemas for QEMU 9.1, 10.1, and master do not
  define `guest-get-memory-stats`; an upstream agent upgrade is not a remedy.
- Libvirt `dommemstat actual` is balloon state, and QEMU excludes virtio-mem
  from balloon size accounting. `available > actual` is not inherently
  invalid. The parser's total-like bounds and missing freshness checks reopen
  ISSUE-006 under M9e.
- `dynamic-memslots` and `unplugged-inaccessible` are only part of the required
  attestation. M9d now binds backend, slot/mapping, incompatible workload,
  balloon-resize, topology, trust, driver, and stack-version evidence and
  rejects drift before resize.
- Source inspection suggests the Windows driver may not periodically retry a
  no-progress shrink. This is an inference requiring M10b live evidence;
  automatic shrinking must gain a default-off control before qualification.
- `win11_gpu` is explicitly a fully trusted development/test KVM guest.
  QEMU/libvirt cgroup containment is recommended defense-in-depth for this VM
  and mandatory for future untrusted or production guests.
- Phase 2 is restricted to one active controller/device on the development
  host. M10c is fixed to a host-side telemetry/libvirt allocation join, and
  M11 owns multi-target reservation and arbitration.
- Full source notes and pinned revisions are in
  [`upstream-virtio-mem-audit.md`](upstream-virtio-mem-audit.md).

## Guidelines

- Report blocking issues immediately with reproduction steps
- Update status and fix reference when resolved
- Link to commit or PR that fixes the issue
- Add regression test for all fixed logic errors

### Initial read-only recheck — 2026-08-18 (before approved deployment)

- `virsh dommemstat win11_gpu` succeeds and reports `actual=8388608 KiB`, `unused=4137384 KiB`, and `available=8337708 KiB`; the `dommemstat` fallback has the required fields for this guest. Dynamic counters may vary between reads.
- `virsh dumpxml win11_gpu` reports the virtio-mem alias, size, block, and
  current requested/current values used by the convergence gate.
- `guest-info` succeeds, but `guest-get-memory-stats` remains unsupported.
- The checked QGA responses expose no Windows driver `requested_size` or
  `plugged_size` values; driver state remains unverified through this boundary.
- `virtio-mem-host@win11_gpu.service` is not installed and has no journal entries.
- The Windows service cannot be marked SCM-verified from RHEL-only evidence;
  obtain the Windows service status/log result from the guest separately.
- A fresh post-driver-update XML check reports `requested=0 KiB` and
  `current=0 KiB`; the previous convergence blocker is resolved. This does
  not by itself prove the driver fields map one-to-one to libvirt fields.
- No resize, guest command, reboot, service installation, or systemd/libvirt
  mutation was attempted.
