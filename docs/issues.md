# Known Issues

## Open Issues

| ID | Description | Status | Component | Priority |
| --- | ----------- | ------ | --------- | -------- |
| ISSUE-002 | Hysteresis tuning for memory allocation | Open | Linux | Medium |
| ISSUE-003 | Error handling for libvirt communication | Open | Linux | High |
| ISSUE-004 | Full-device virtio-mem test risked exhausting host memory | Open; safety guard added 2026-08-18 | Host validation | Critical |
| ISSUE-008 | Classic Event Log text rendering is unreliable without a registered message resource; XML `EventData` contains the correct bounded message | Open; XML query documented | Windows observability | Medium |

## Resolved Issues

| ID | Description | Status | Fix Reference | Date Resolved |
| --- | ----------- | ------ | -------------- | ------------- |
| ISSUE-001 | QEMU Guest Agent availability on Windows 11; connected QGA 110.0.2 does not provide `guest-get-memory-stats` | Resolved in code; `dommemstat` fields verified on `win11_gpu`; guest capability still requires a replacement QGA build | `host/src/dommemstat.rs`, `VIRTIO_MEM_STATS_SOURCE` config | 2026-08-18 |
| ISSUE-005 | Virtio-mem rollback left `requested` and `current` divergent after the earlier 1 GiB test | Resolved after the updated Windows driver was installed; fresh XML reports `requested=0 KiB` and `current=0 KiB` | Fresh read-only `virsh dumpxml win11_gpu` convergence check | 2026-08-18 |
| ISSUE-006 | Windows `dommemstat` reports `available` above balloon `actual` | Resolved by conservative fallback to `unused`; host controller is active on `win11_gpu` | `host/src/dommemstat.rs` regression test and live service validation | 2026-08-18 |
| ISSUE-007 | Invalid service configuration was loaded before SCM dispatcher attachment, causing Windows error 1053 without status or Event Log context | Resolved by dispatching SCM before configuration loading; live invalid-config recovery emitted event 2000 and exit code 1 | `windows/src/main.rs` startup-route regression and M7 live validation | 2026-09-04 |
| ISSUE-009 | Shared virtio-mem state validation rejected the live fully-unplugged `requested=current=0` state | Resolved by allowing zero observed state while retaining positive-target validation | `VirtioMemState` and live XML regression tests; M9 live CLI validation | 2026-09-05 |
| ISSUE-010 | Host policy rejected a converged allocation below its configured minimum, preventing the installed controller from bootstrapping a fully unplugged device | Resolved with one aligned request to the configured minimum; normal policy remains one block at a time and above-maximum state fails closed | `plan_resize` regression test and M9b live zero-to-1-GiB systemd convergence | 2026-09-05 |

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
