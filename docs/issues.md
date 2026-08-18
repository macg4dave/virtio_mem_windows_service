# Known Issues

## Open Issues

| ID | Description | Status | Component | Priority |
| --- | ----------- | ------ | --------- | -------- |
| ISSUE-002 | Hysteresis tuning for memory allocation | Open | Linux | Medium |
| ISSUE-003 | Error handling for libvirt communication | Open | Linux | High |
| ISSUE-004 | Full-device virtio-mem test risked exhausting host memory | Open; safety guard added 2026-08-18 | Host validation | Critical |
| ISSUE-005 | Virtio-mem rollback from a successful 1 GiB test did not converge within 300 seconds | Open; reproduced on `win11_gpu` 2026-08-18 | Host validation | Critical |

## Resolved Issues

| ID | Description | Status | Fix Reference | Date Resolved |
| --- | ----------- | ------ | -------------- | ------------- |
| ISSUE-001 | QEMU Guest Agent availability on Windows 11; connected QGA 109.1.0 does not provide `guest-get-memory-stats` | Resolved in code; live `dommemstat` field availability on `win11_gpu` still requires operator verification | `host/src/dommemstat.rs`, `VIRTIO_MEM_STATS_SOURCE` config | 2026-08-18 |

## Guidelines

- Report blocking issues immediately with reproduction steps
- Update status and fix reference when resolved
- Link to commit or PR that fixes the issue
- Add regression test for all fixed logic errors
