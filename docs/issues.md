# Known Issues

## Open Issues

| ID | Description | Status | Component | Priority |
| --- | ----------- | ------ | --------- | -------- |
| ISSUE-002 | Hysteresis tuning for memory allocation | Open | Linux | Medium |
| ISSUE-003 | Error handling for libvirt communication | Open | Linux | High |
| ISSUE-004 | Full-device virtio-mem test risked exhausting host memory | Open; safety guard added 2026-08-18 | Host validation | Critical |
| ISSUE-005 | Virtio-mem rollback from a successful 1 GiB test did not converge within 300 seconds | Open; fresh read-only recheck on `win11_gpu` 2026-08-18 still shows `requested=0`, `current=18432 KiB` | Host validation | Critical |

## Resolved Issues

| ID | Description | Status | Fix Reference | Date Resolved |
| --- | ----------- | ------ | -------------- | ------------- |
| ISSUE-001 | QEMU Guest Agent availability on Windows 11; connected QGA 109.1.0 does not provide `guest-get-memory-stats` | Resolved in code; `dommemstat` fields verified on `win11_gpu` | `host/src/dommemstat.rs`, `VIRTIO_MEM_STATS_SOURCE` config | 2026-08-18 |

## Guidelines

- Report blocking issues immediately with reproduction steps
- Update status and fix reference when resolved
- Link to commit or PR that fixes the issue
- Add regression test for all fixed logic errors

### Read-only recheck — 2026-08-18

- `virsh dommemstat win11_gpu` succeeds and reports `actual=8388608 KiB`, `unused=4137384 KiB`, and `available=8337708 KiB`; the `dommemstat` fallback has the required fields for this guest. Dynamic counters may vary between reads.
- `virsh dumpxml win11_gpu` still reports virtio-mem alias `ua-virtiomem0`, size `20971520 KiB`, block `2048 KiB`, `requested=0 KiB`, and `current=18432 KiB`.
- `guest-info` succeeds, but `guest-get-memory-stats` remains unsupported.
- The checked QGA responses expose no Windows driver `requested_size` or
  `plugged_size` values; driver state remains unverified through this boundary.
- `virtio-mem-host@win11_gpu.service` is not installed and has no journal entries.
- The Windows service cannot be marked SCM-verified from RHEL-only evidence;
  obtain the Windows service status/log result from the guest separately.
- No resize, guest command, reboot, service installation, or systemd/libvirt
  mutation was attempted. Do not issue another resize until convergence and
  the rollback behavior are understood.
