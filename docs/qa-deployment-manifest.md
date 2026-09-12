# QA Deployment Manifest

> Historical deployment evidence: this manifest describes the fixed-headroom
> controller candidate and remains useful for the completed deployment baseline,
> transport, reconciliation,
> and platform facts. It does not qualify the Windows-native pressure-aware
> policy adopted on 2026-09-12. AR labels below are retained only as historical
> evidence labels; current execution status is in `BACKLOG.md`.

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

The typed QA-T002 inventory confirms that the installed unit also loads
`/etc/virtio-mem-host/win11_gpu-memory-quanta.conf` through a drop-in that is
not present in the repository unit. Both installed environment files are
root-owned; the primary file is mode `0600`, the drop-in configuration is mode
`0644`, and together they select the legacy `guest-stats` path while omitting
the current quantitative target-policy inputs. Their hashes and exact contents
are retained in the ignored typed inventory evidence. The absent
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

QA-T002 is complete. Evidence is stored at
`.artifacts/deployment/qa-t002-host-inventory.json`; it confirms the exact unit,
binary, drop-in, environment files, hashes, ownership, contents, and inactive
controller state.

QA-T004 through QA-T008 are complete. The production transport uses bounded
standard QGA guest-file reads with durable host acknowledgement. The native
Windows candidate hash
`52d1692f83ed2b935214b3bf0a3f90c47b4adbe9f7a11464fba2e51bab58713c`
is installed as `VirtioMemService` under `LocalService`; versioned config,
protected ACLs, normal SCM error control, and advancing same-session telemetry
are recorded in `.artifacts/deployment/qa-t005-windows-apply.json`. Bounded
atomic-replace retries tolerate QGA readers that temporarily deny delete
sharing; repeated live reads left the service running with exit code zero.

Fresh allocation-neutral calibration recorded a stable visible base of
`8518938624` bytes with live requested/current both `2155872256` bytes. The
reviewed live attestation and applied host deployment are retained under
`.artifacts/deployment/`. The installed host binary hash is
`f549a438300166be67577323ae609354295fdc6b5c9c4a251e5f2be9278cd2a3`;
the obsolete split configuration was archived to the recorded rollback path.
The final no-actuation preflight proved fresh transport, durable replay
handling across reader reconstruction, Windows session rollover, current
attestation, sufficient host headroom, and unchanged allocation. The host
controller remains disabled and inactive for AR2.

## AR2 deployment correction and safe handoff

QA-T009 exposed that an unquoted Windows path in the first AR2 host instance
file was transformed by systemd before the controller received it. The typed
deployment path was rerun with the required quoted QGA path. Applied evidence
in `.artifacts/deployment/qa-t009-host-apply-final.json` records installed host
binary SHA-256
`30015edaac9659f1543a0b12431d6a53a0c95e9996398ea24640349ff3dd7d6d`,
configuration SHA-256
`e5d068a9824c1647466aaa432f50c7bab211d16eeb0d663530d3508564e4d539`,
and the disabled/inactive fail-stop state.

A subsequent resident attempt observed controller-issued growth from
`2155872256` to `4297064448` bytes, but the detached observer then lost Polkit
authorization and did not produce a terminal result. That attempt does not
close QA-T009. The controller guard now owns the complete bounded libvirt/QGA
observation batch beneath one elevation.

Applied run `qualification-1789215769941-25346` subsequently completed all
workload phases and 142 observation samples without observer warnings, then
failed its predeclared growth criterion with requested/current remaining
`1073741824` bytes. Its controller log recorded 149 fail-closed
`demand_input_invalid` decisions because a new producer session had advanced
past sequence zero while the controller was inactive. Cleanup succeeded and
the unit returned disabled/inactive with converged live state. Qualification
now supports an explicit bounded restart of only the named Windows telemetry
service after controller ownership begins and requires the first matching
accepted decision before workload launch. Durable acknowledgement remains
untouched. A new applied resident run remains required.

Applied run `qualification-1789217084408-38317` proved the corrected privilege
and telemetry-session handoff: the service cycle completed, the controller
accepted the new session, preflight passed, and all resident workload phases
ran. The controller nevertheless failed closed before its first resize because
the installed September 11 attestation's domain XML fingerprint did not match
the VM process started September 12 at 12:54. Requested/current stayed
converged at `1073741824` bytes, so no resize was issued. Fresh read-only
evidence in
`.artifacts/deployment/qa-t009-attestation-after-vm-restart.json`, generated
from the unchanged explicit compatibility review, retains the same QEMU argv,
libvirt version, QEMU version, and required virtio-mem properties; only the
restart-bound domain XML and resulting document fingerprints changed. Its
SHA-256 is
`b07e56356487fd1c5863a448e38268f721275cb53634abb63ead07523a9fc9e1`.
The typed deployment recorded in
`.artifacts/deployment/qa-t009-host-apply-after-vm-restart.json` installed that
exact attestation with matching candidate/installed host binary SHA-256
`30015edaac9659f1543a0b12431d6a53a0c95e9996398ea24640349ff3dd7d6d`
and matching source/installed configuration SHA-256
`614ba3c31e34a48de3c452cf1739801877c9735419592b93c98584b5c93e435f`.
It left the exact unit disabled/inactive with `Restart=no`. Qualification
cleanup now also resets systemd's failed marker after a fail-stop so its
terminal safe state is disabled and inactive.

The first controller start after that deployment exposed a durable recovery
edge before workload launch: the prior attestation rejection had safely
latched a command intent under the old compatibility fingerprint, while the
new attestation changed that fingerprint. Startup therefore refused the
pending mismatched checkpoint and issued no resize. The explicit `clear-latch`
path now permits only a live-validated compatibility-fingerprint migration
when VM, alias, policy, and converged live state still match; it clears intent,
restarts estimator history cold, and preserves the operator reason. This
recovery must be applied before another qualification attempt.

The first clear attempt correctly refused because policy also differed. Hash
comparison identified the exact change: the latched deployment hash
`e5d068a9824c1647466aaa432f50c7bab211d16eeb0d663530d3508564e4d539`
is the current configuration with a 10-second reclaim maximum gap, whereas the
later hash
`614ba3c31e34a48de3c452cf1739801877c9735419592b93c98584b5c93e435f`
uses the reviewed 15-second gap. No other line differs. Recovery therefore
temporarily installs the exact 10-second policy with the new live attestation,
uses the product's explicit converged-state latch clear, then reinstalls the
15-second policy. This preserves the hard refusal on policy migration and
allows the subsequent unmatched but unlatched checkpoint to restart cold.

That recovery completed. The policy-10 deployment and successful transient
service execution cleared the latch under the original policy fingerprint.
`.artifacts/deployment/qa-t009-host-apply-policy15-restored.json` then restored
the reviewed policy and records matching candidate/installed binary SHA-256
`81245cabff0392357b439ad25e1049cc7e1c023eb0166abe0dd1ae127b477d16`,
configuration SHA-256
`614ba3c31e34a48de3c452cf1739801877c9735419592b93c98584b5c93e435f`,
and attestation SHA-256
`b07e56356487fd1c5863a448e38268f721275cb53634abb63ead07523a9fc9e1`.
The unit remains disabled/inactive and requested/current remain converged at
1 GiB pending a fresh resident run.

Applied resident run `qualification-1789231813766-72447` completed with durable
terminal evidence. It observed growth from `1073741824` to `2172649472` bytes
during peak pressure and further upward requests to `2300575744` bytes under
renewed pressure, with all five workload phases, 141 host samples, zero
observer warnings, and successful disabled/inactive cleanup. It failed the
predeclared reclaim criterion: a single transient QGA sharing collision at
17:56:59 invalidated demand input and correctly reset the 300-second reclaim
history. History became ready again only after renewed pressure had raised
desired, so observed reclaim was zero. Final requested/current were converged
at `2300575744` bytes. This is valid failure evidence, not a QA-T009 pass.

Afterward, bounded read-only `virsh` probes timed out while `virtqemud.service`
remained active and both QEMU processes remained present. No libvirt daemon
restart or direct resize was performed. Restoring the captured initial target
and rerunning with a low-demand window robust to one late transient reset
remain pending.
