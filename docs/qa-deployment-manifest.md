# QA Deployment Manifest

This manifest records the read-only QA-T002 deployment comparison captured on
2026-09-11 for `win11_gpu` and `ua-virtiomem0`. It is not an installation
record and does not authorize actuation.

## RHEL controller

| Item | Installed | Candidate | Result |
| --- | --- | --- | --- |
| Executable SHA-256 | `a1c431e67b49ba0373091fb32760cecea38a7e311bdc04a8972a9a44bf2a607c` | `2039ea08b72798026508f2b30dddc42ba02551903b0503b9013d6147591ac0b4` | Mismatch |
| Unit SHA-256 | `6663fd58e924b32777a95a19e2f666ea9c857fe315709e0a73d21df3097ff4bd` | `922a8411acf045751a9f75ec88efa03589b6dcec2aae893b3f9233f148d1f83f` | Mismatch |
| Service identity | `virtio-mem-host@win11_gpu.service`, user/group `virtio-mem-host` | Same template identity | Matches |
| Enablement | Enabled; manually stopped for QA-T001 | Not installed | Must remain stopped during deployment preparation |
| Runtime state | `inactive/dead`, `MainPID=0`, historical `NRestarts=117` | Not installed | Safe hold established |

The installed unit also loads
`/etc/virtio-mem-host/win11_gpu-memory-quanta.conf` through a drop-in that is
not present in the repository unit. The unprivileged audit could not read the
installed environment files, so their hashes, effective values, ownership,
and intended replacement remain open QA-T002 evidence. The absent
`/run/virtio-mem-host` directory is expected while the unit is stopped and
must be recreated with the documented ownership/mode when the candidate runs.

## Windows service

| Item | Installed | Candidate | Result |
| --- | --- | --- | --- |
| Executable SHA-256 | `5ac0f46e402606a9a71f95318b6338f1649879b1b5389a54e992b3dae9e459d3` | `db0e1805764628c0caf667156e6ccbbee0428c0fe595c9f33f9843fd5f3afa9b` | Mismatch |
| SCM identity | `VirtioMemService`; own process; automatic; `LocalService` | Configurable own-process identity validated against SCM | Compatible, candidate required |
| SCM error control | `0` (`IGNORE`) | `1` (`NORMAL`) | Mismatch fixed by QA-T003 |
| ProgramData config | Absent | Current versioned configuration at the documented path | Deployment required |
| ProgramData telemetry | Absent | Atomic version-2 record with retention | Deployment/transport required |
| Runtime state | `RUNNING`, exit codes zero | Native gate only | Installed replacement not yet validated |

## Captured safety state

- `requested=current=2105344 KiB`; block size is `2048 KiB`.
- The VM is running, QGA `guest-ping` succeeds, and both `qemu-ga` and
  `VirtioMemService` report `RUNNING` after the controller stop.
- No resize command was issued and the allocation did not change during
  QA-T001.

QA-T002 remains in progress until a privileged read-only capture records the
installed host environment files and effective configuration. QA-T004 must
select and implement the production telemetry transport before either current
candidate is installed for QA-M1.
