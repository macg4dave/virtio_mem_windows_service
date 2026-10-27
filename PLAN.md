# virtio-mem Windows service — Plan

This file replaces all previous docs (readme, BACKLOG, PROJECT_STATUS,
IMPLEMENTATION_PLAN, `docs/*`, and the `.github/prompts/*` set). They described
a 17-milestone roadmap and ~40 QA gates. Very little of that had run against a real VM.
Git history still has them if anything needs recovering.

## What this project is for

Share one host RAM pool between several KVM guests. The host watches each
guest OS for memory pressure and grows a pressured guest from the pool. Each
VM has a weight, so important VMs win when guests contend for the same RAM.
Guests give RAM back to the pool when their OS no longer needs it.

The mechanism, per guest, is resizing a virtio-mem device from the host:

```text
Windows guest                         RHEL/libvirt host
-------------                         -----------------
service samples memory  --(file)-->   daemon reads it over QGA guest-file-read
(available, commit)                   computes a target size
                                      virsh update-memory-device --requested-size
                                      waits for live `current` to converge
```

The idea is simple and the mechanism is standard (libvirt
`update-memory-device`, QGA file reads). The implementation is not simple.

Phases 1–3 make the per-guest loop trustworthy on one VM, because the pool
depends on each guest growing and giving back reliably. Phase 4 is the pool
itself; its source is written and awaits live testing. Giving RAM back is the biggest
platform risk: Windows virtio-mem unplug has already stalled once (Phase 0).

## Honest current state (2026-10-26)

| Item | Reality |
| --- | --- |
| Code size | 29,641 Rust source lines, including tests, comments, and blanks; not a production-code count |
| Non-Windows tests | 2026-10-27: core 51, host 94, xtask 44 pass with `--locked` |
| `windows` crate | `cargo clippy -p virtio-mem-service --all-targets --target x86_64-pc-windows-gnu` passes on Linux (type-check only; no MinGW linker installed, so no binary). Native service behaviour requires Windows execution |
| Automatic resizing | Earlier "never enabled" claim was wrong. Archived workload run `qualification-1789231813766-72447` records 1,098,907,648 bytes of peak growth but zero accepted reclaim; the later passing run explicitly required no resizing. The installed host unit is currently disabled/inactive |
| Multi-VM pool | Wired: `core::pool::allocate_pool` (pure) plus the host runtime's two-phase round. Unit-tested only; no live multi-VM run yet. The old `global_pool`/`pool_ledger` (~2.4k lines) were deleted |
| Host config | One TOML file (`--config FILE`): global timing/host settings, optional `pool_bytes` (required with more than one VM), and a `[[vm]]` table per VM with its `priority`. Unknown keys are refused. `status`/`clear-latch` take `--vm NAME` when the file lists several VMs |

Growth has occurred in an earlier automatic workload run. That does not prove
reliable shrink or unattended operation. Those outcomes still require direct
evidence; a passing observe-only/no-resize run is not a resizing qualification.

## Facts worth keeping

- Virtio-mem is asynchronous: `requested` is the target, `current` is what the
  guest actually plugged. A successful `virsh` call doesn't mean it happened.
  Always poll live XML for `current`.
- Windows virtio-mem driver is tech preview. **Unplug (shrink) can be partial,
  slow, or stall.** Grow is the safe direction.
- Libvirt `dommemstat actual` is balloon state, not virtio-mem allocation.
  `unused`/`available` from the balloon stats can serve as demand hints.
- Upstream QGA has no `guest-get-memory-stats`; the custom-command path in
  `host/src/qga.rs` won't work on a stock guest agent.
- Windows has no "recommended RAM" API. Usable signals: available physical,
  commit total vs. commit limit, and the low/high memory resource
  notifications.
- Targets must be aligned to the device block size and kept within
  `[min, max]` of the device. Never request the full device size blindly.
- Never send a new resize while the previous one hasn't converged. If it
  stalls, stop and log; don't retry in a loop.
- Leave the host enough RAM (check `/proc/meminfo` MemAvailable before growing).

## Plan

### Phase 0 — Initial platform investigation (interrupted)

The static assessment is complete. The separately approved bounded live check
below was interrupted after repeated authentication prompts. The user explicitly
redirected the work: the program must own the operation, rather than the agent
running a manual loop. Local implementation now proceeds in Phase 1; repeated
grow/shrink and workload qualification are not claimed complete. No more manual
live-VM commands are authorized by the interrupted test.

1. On the test VM, run `virsh update-memory-device VM --alias ALIAS --requested-size X`
   to grow, then to shrink, several times. Watch `virsh dumpxml` `current` and
   Windows Task Manager.
2. Record: does grow work every time? How long does it take? Does shrink
   complete, partially complete, or hang? Under memory load?
3. Check whether `virsh dommemstat VM` returns useful `unused`/`available` on
   Windows with the balloon driver + stats period set. If yes, **we may not
   need a Windows service at all.**

Before live work, identify the exact VM, device alias, baseline requested/current
allocation, block size, bounded targets, host reserve, and rollback or recovery
procedure. State target units explicitly. Observe convergence before issuing
another resize. A stalled operation may prevent ordinary rollback; do not assume
that reversing a target is safe or accepted. Reboots and VM definition changes
require approval.

**Exit:** record repeated grow/shrink outcomes, convergence times, partial or
stalled unplug, behavior under load, and dommemstat usefulness here. Select the
telemetry source and decide whether automatic shrink is viable. If shrink is
unreliable, the product is grow-only with an explicitly reviewed manual or
reboot-time reset; shrink waits until the platform supports it.

### Phase 0 operational evidence (2026-10-26)

- Test VM: `win11_gpu`, device `ua-virtiomem0`, running. Controller unit
  `virtio-mem-host@win11_gpu.service` is disabled/inactive with `Restart=no`.
- Live device size: 21,474,836,480 bytes (20 GiB); block size: 2,097,152 bytes
  (2 MiB). Baseline requested/current: 1,073,741,824 bytes (1 GiB).
- QGA ping succeeds. The existing Windows service publishes fresh v3 telemetry
  at `C:\ProgramData\VirtioMemService\telemetry.json`, with about 4.4 GB
  physical RAM available and a high-memory notification during inspection.
- `virsh dommemstat` returns advancing five-second statistics, including
  `unused`, `available`, and `usable`. Baseline `available` was 9,367,852 KiB;
  it increased to 9,433,388 KiB after the 64 MiB grow. These are useful host-side
  demand inputs; `actual` remained balloon state and is not allocation authority.
- QMP confirms dynamic memslots and unplugged-inaccessible. One initial
  read-only QMP call timed out; a subsequent read succeeded.
- SSH alias `virtio-mem-windows` currently resolves to `ice101.lan` and times out.
  The earlier guest workload executable path is also absent. No workload,
  service restart, deployment, reboot, or VM-definition change was performed.
- Explicit user approval covered at most three cycles from 1 GiB to
  1,140,850,688 bytes (1 GiB + 64 MiB; 1,114,112 KiB to virsh) and back to
  1 GiB (1,048,576 KiB). Require 4 GiB host reserve; allow 120 seconds for growth
  and 300 seconds for restoration. Stop on the first stall/uncertain outcome
  without another resize. No other live mutations are included.
- First growth converged in approximately 4.297 seconds. Restoration reclaimed
  only 10 MiB before remaining unchanged through the last successful observation
  at about 225.739 seconds. Polling then failed because authentication was
  cancelled. This was an interrupted observation, not a completed 300-second
  timeout or a successful restoration. No second cycle or recovery was issued.
- Last observed device state: requested 1,073,741,824 bytes (1 GiB), current
  1,130,364,928 bytes (1 GiB + 54 MiB). The baseline has **not** been confirmed
  restored. A read-only check on 2026-10-26 found the same state: requested
  1,048,576 KiB, current 1,103,872 KiB (1 GiB + 54 MiB). The unplug is still
  unconverged; no recovery has been issued. The user chose to leave it as is:
  the 54 MiB is not worth recovering. The program must still treat the device
  as unconverged and must not stack a resize on it.
- Desktop user `dave` is now in the `libvirt` group. The system rule
  `/usr/share/polkit-1/rules.d/50-libvirt.rules` grants
  `org.libvirt.unix.manage` to that group, so `pkcheck` and write-capable
  `qemu:///system` connections succeed without a password prompt. The
  `virtio-mem-host` account was already a member, which makes the repository's
  `50-virtio-mem-host.rules` redundant on this host.
- The manual loop opened a fresh read-write libvirt connection for every query,
  triggering repeated desktop authorization. The user reported more than fifty
  password prompts. The loop has exited; no polling/virsh child remains. Future
  validation must use the program with service authorization configured once.

### Phase 1 — Cut to a minimum working loop

**Current phase: local program implementation, following the user's explicit
redirection.** No deployment, permission change, service start/restart, recovery,
or additional live resize has been performed. The interrupted platform checks
remain acceptance work, to be exercised through the application.

Keep:

- Windows service: sample `GlobalMemoryStatusEx` and commit info every N
  seconds, write one JSON line atomically. Install/run as a service.
  **Decided:** raw Windows telemetry is the only demand source; the
  guest-stats/dommemstat path was removed.
- Host daemon, one VM, one device:
  1. read guest telemetry (QGA file read)
  2. read live device XML (`requested`, `current`, block size, max)
  3. compute one bounded, block-aligned device target using the contract below
  4. grow promptly in bounded steps when target exceeds current; shrink only after
     sustained low usage (hysteresis + cooldown), in small steps
  5. one command in flight; wait for `current == requested` with a timeout;
     on timeout, stop resizing and log loudly
  6. refuse to grow if host MemAvailable is below a reserve
- **Done:** one host config file, `host/systemd/virtio-mem-host.toml.example`;
  the environment-variable path is gone. Shrink defaults off.
- Unit tests for the target math and the XML parsing.

### Control loop contract

All sizes are virtio-mem device bytes, not total guest RAM. This describes the
code as it stands; changing any line is a policy change, not a refactor.

- **Telemetry:** the Windows service's raw record (physical total/available,
  commit total/limit), read over QGA. A record must be fresh (`max_age`,
  `future_tolerance`), belong to the configured VM and service, and advance the
  accepted session/sequence; an unchanged record is "not fresh" and never
  causes a resize. Acceptance is stored in the policy checkpoint.
- **Fixed base:** `physical_total - current` must match
  `fixed_visible_base_bytes` within max(256 MiB, 2 blocks); otherwise the
  sample is refused and the estimate history is cleared.
- **Desired now:** max of a physical candidate
  (`used + physical_reserve - fixed_base`) and a commit candidate
  (`current + commit_reserve - commit headroom`), at least `min_memory_bytes`,
  aligned up to the block, capped at the effective maximum.
- **Safe floor now:** the same with the smaller safe-floor reserves, never
  above desired.
- **Effective maximum:** `min(max_memory_bytes, device size - 1 GiB)`, aligned
  down. Minimum, current, reserves and hysteresis must be block aligned.
- **Growth:** desired rises immediately; the device grows by at most
  `grow_step_bytes` per command, only if host MemAvailable stays above
  `host_min_headroom_bytes` after the step (shared across VMs in one round).
- **Give-back:** only after `reclaim_history_seconds` of gap-free history
  (gaps longer than `reclaim_max_gap_seconds` restart it), desired drops to
  the window's peak when that is at least `downward_hysteresis_bytes` lower;
  the floor becomes the window's peak floor. Shrink is off unless
  `automatic_windows_shrink = true`, and moves `shrink_step_bytes` per command.
- **One command in flight:** every command is recorded durably first. Growth
  or shrink must converge within `convergence_timeout_seconds`; otherwise the
  VM latches and the daemon stops. A pending shrink may be raised toward
  `current` when demand returns, or frozen at `current` (and latched) when
  telemetry fails.
- **Restart:** a recorded command is classified against live state:
  converged clears it; not applied or contradicted latches. Live divergence
  with no recorded command is never replayed; the VM only observes. A latch is
  cleared only by `clear-latch`.
- **Missing input:** an evaluation error holds the VM; unreadable live state
  holds it and stops all pool growth that round, except during an owned
  command, where it latches.

Proposed removals, subject to the Phase 0 result and retained contracts:

- Compatibility attestation, if a simpler check proves sufficient. Keep
  `AbandonToCurrent` for the manual `abandon-shrink` recovery.
- Keep `tools/xtask`: the user retains it for AI-driven testing and
  qualification. Consolidate duplication inside it; do not delete commands.
  Direct Cargo commands, listed below, are the local gate.

Preserve command ownership, uncertain-outcome handling,
host reserve checks, and required recovery behavior when removing larger
compatibility/status/recovery layers. Check actual API and CLI consumers before
removing public contracts; do not introduce speculative compatibility layers.

**Exit:** one understandable loop, no superseded implementation paths, passing
non-Windows tests, and a Windows binary build for the selected target
(`x86_64-pc-windows-gnu` or `-msvc`) if the service remains necessary. Native
service behavior still needs separate Windows execution. A few thousand lines
is a simplification aim, not a measured reduction or correctness criterion.

### Phase 1 open items from completed work

- The polkit rule `host/systemd/50-virtio-mem-host.rules` for the dedicated
  `virtio-mem-host` account is prepared but **not installed**. Installation is
  a one-time administrative step; the daemon never elevates itself or stores a
  password. It grants connection-level libvirt management, not a per-VM ACL.
- Manual runs need no password: `dave` is in the `libvirt` group (see Phase 0
  evidence). `sudo` also works, as the startup authorization error says.
- No current Windows binary build or live deployment of the Phase 1 changes.
- Before redeploying the host daemon, the configuration moves from
  `/etc/virtio-mem-host/<instance>.conf` (environment variables) to
  `/etc/virtio-mem-host/<instance>.toml`, and the unit runs
  `virtio-mem-host --config` that file. Copy each value into the TOML keys of
  the example; settings that were removed (demand/stats source, thresholds,
  re-notification, ack path, pressure modes) have no key and are refused if
  added. `xtask host-deploy` installs the `.toml` and moves an old `.conf`
  into the deployment backup. **The deployed `win11_gpu.conf` was a shadow
  (non-resizing) deployment** (it has shadow margin settings and a shadow log
  path), so its new TOML must set `observe_only = true` to stay non-resizing.
  `controller-status` and `qualification` read the same file; qualification
  now requires `automatic_windows_shrink = true` explicitly (the old check let
  an unset value pass).
- After the upgrade, telemetry acceptance lives in the policy checkpoint
  (`accepted_telemetry`); the old ack file is unused. The first record is
  treated as new; retired-session history starts empty, while freshness and
  identity checks still apply.
- Controller status JSON is now version 4 (pressure fields removed). xtask and
  the host binary must be the same build. Existing policy checkpoints that
  carry the retired `pressure_shadow` field still load; it is dropped on save.

Authorization follows the upstream [libvirt authentication contract](https://libvirt.org/auth.html)
and [`pkcheck` non-interactive/process-identity requirements](https://polkit.pages.freedesktop.org/polkit/pkcheck.1.html).
The non-interactive check is a prerequisite, not a replacement for libvirt's
own authorization.

### Phase 2 — Run it for real, grow-only

- Deploy to the test VM with `enable_shrink = false`.
- Run memory-hungry workloads in the guest. Confirm it grows in time and the
  guest doesn't page heavily or OOM.
- Run for a few days. Watch the logs for stalls.

**Exit:** grow works unattended.

### Phase 3 — Shrink

- **Decided: give RAM back slowly at first.** When the guest no longer needs
  RAM, return it in small block-aligned steps with a long cooldown, one step in
  flight, so a returning workload is never short. Speed it up only once the
  logs show a reliable "safe to give back faster" metric; choosing that metric
  is open work, not a default to guess.
- Verify partial/stalled unplug is detected and the loop stops cleanly
  instead of fighting the guest.
- Only then consider Windows memory-resource notifications as a faster
  "grow now" trigger.

- Decide the give-back mechanism from evidence. If Windows virtio-mem unplug
  stays unreliable, evaluate returning RAM through the virtio-balloon driver
  instead, with virtio-mem used for growth. This is a design decision for the
  user, not an implementation default.

**Exit:** a week of grow and shrink on the test VM without manual intervention.

### Phase 4 — Shared pool across VMs

- **Source done:** one daemon controls every `[[vm]]` in the config, sharing
  `pool_bytes` of device memory. Each poll is one round: every VM observes and
  estimates, `core::pool::allocate_pool` sets a ceiling per VM, then each VM
  acts through its normal reconcile path, highest priority first.
- **Decided: weight means strict priority** (larger wins, 0 is invalid):

  ```toml
  pool_bytes = 17179869184   # 16 GiB shared by all members

  [[vm]]
  name = "win11"
  priority = 100
  # ...the other per-VM keys from host/systemd/virtio-mem-host.toml.example

  [[vm]]
  name = "win10_test"
  priority = 80
  ```

- How the pool behaves:
  - a VM's charge is `max(requested, current)` from live XML, so RAM a guest
    is still giving back is never handed out (no pool state is persisted);
  - growth comes only from free pool bytes, higher priority first;
  - a higher-priority VM still short lowers the ceiling of lower-priority VMs
    that have automatic shrink, fresh telemetry and full history, never below
    their safe floor; they shrink in normal steps and the freed bytes are
    granted in a later round;
  - equal priorities never take from each other;
  - a pending shrink may still be raised toward `current` if its own guest's
    demand returns: guest safety beats a higher-priority request.
- Open decisions for the user:
  - one VM latching on a convergence timeout still stops the whole daemon
    (the per-VM fail-stop rule); a pool may want that VM held instead;
  - a shut-off or unreadable VM freezes all growth (its charge is unknown);
    treating a shut-off domain (`domstate`) as zero charge would lift that.
- `xtask host-deploy`, `controller-status` and `qualification` still handle
  one VM per config file.
- The first pool is Windows-only; Linux members wait until the Windows pool
  works (see "Later").

**Exit:** two or more test VMs share a pool under alternating load; the
higher-weight VM is served first; RAM returns to the pool when load ends.

### Later, only if needed

- Linux guest support (virtio-mem in Linux is mature; this would be easier
  than Windows).

## Architecture assessment (static review, 2026-10-26)

The project needs fewer competing control paths and one owner of resize state.
The existing crate boundaries are useful; the main complexity sits inside them.
This assessment inspected workspace structure, dependencies, public interfaces,
entry points, significant workflows, state/persistence paths, and current
instructions. It is not evidence that the platform works.

### Workspace and execution paths

| Crate | Actual responsibility | Main dependencies |
| --- | --- | --- |
| `virtio-mem-core` | Telemetry and Windows-config contracts, XML parsing, target estimation, reconciliation, pool allocation | Serde, quick-xml, thiserror |
| `virtio-mem-host` | Configuration, telemetry acquisition, policy persistence, host-memory checks, virsh execution, diagnostics | Core, signal-hook, wait-timeout |
| `virtio-mem-service` | Windows sampling, atomic telemetry publication, SCM lifecycle, workload utility | Core, winapi, Serde |
| `virtio-mem-xtask` | Local gates, remote builds, deployment, qualification, live-resize orchestration | Core, host, subprocess utilities |

- **Windows service:** command dispatch -> configuration -> `RawTelemetryWorker`
  -> native memory APIs -> telemetry envelope -> `AtomicRawTelemetryPublisher`.
  SCM and console startup share one worker constructor.
- **Host:** configuration -> per VM: file or QGA file reader -> envelope validation ->
  `TargetDemandSource` (acceptance via `AcceptedTelemetry`, stored in the
  same checkpoint write as the estimator) -> `HostRuntime`, which runs every
  VM in rounds (observe, `allocate_pool`, act) and owns
  the operation lifecycle (durable intent and latch via the checkpoint,
  `command_ownership` resolution, `reconcile` for every action) -> host-memory
  gate -> resize sink -> live XML observation.
- **Operational tooling:** xtask dispatches deployment and qualification,
  invokes local/remote processes, writes evidence, and can start controllers
  or workloads. It is not merely a build-command wrapper.

Useful existing foundations are alias-selected XML, checked unit conversions,
explicit requested/current observations, atomic telemetry publication, and the
distinction between command rejection and an unknown command outcome.

The largest weaknesses are distributed resize/lifecycle rules, diagnostics that
can describe a different policy from the daemon, separate persistence paths for
sample acceptance and commands, experimental configuration combinations,
privileged tooling that has grown into another application, and public
implementations with no production callers found in this workspace.

## Code consolidation register

Source references describe the reviewed snapshot and may move during refactoring.
"No production caller" means none found in this repository. Public exports alone
do not establish whether external consumers exist. All executable changes below
belong to future work after the current phase's exit criteria.

### 14. KEEP / SEPARATE — Different transport and platform contracts

- **Evidence/callers:** `windows/src/demand.rs` uses Windows atomic replacement;
  `host/src/target_policy.rs` publishes durable Linux checkpoints. Windows Event
  Log and host stderr/journal output also have different consumers.
- **Canonical shared foundations:** `parse_virtio_mem_xml_for_alias` at
  `crates/virtio-mem-core/src/virtio_mem_xml.rs:181`, `VirtioMemState`, and checked
  byte/KiB conversions. Keep platform acquisition and publication separate.
- **Benefit:** reuse actual shared rules without hiding important OS semantics.
- **Preservation risk:** a fresh read immediately before a command is justified
  even if an earlier poll read the same resource. Do not remove it as duplication.

### Tooling limitations affecting verification

- `live_resize::run` at `tools/xtask/src/live_resize.rs:208` attempts ordinary
  rollback after forward failure. Ordinary resize preparation rejects an
  unconverged device, so rollback cannot be assumed to recover a stalled
  operation. Preserve the no-stacked-resizes rule and specify operator recovery.

## Responsibility ownership

| Responsibility | Authoritative home |
| --- | --- |
| Wire telemetry schema and intrinsic validation | Core telemetry types |
| Device geometry and XML interpretation | Core device/XML code |
| Target calculation and pure action selection | Core policy |
| Pool allocation across VMs (pure) | Core `pool` |
| Accepted samples, pending operation, timeout, latch | Host controller state |
| External commands and host-resource checks | Host adapters |
| Windows sampling, publication, service lifecycle | Windows crate |
| Presentation | CLI, journal output, Windows Event Log |
| Project evidence and phase acceptance | `PLAN.md` |

Use one implementation per responsibility and preserve meaningful platform
differences. These are the proposed target boundaries; the current code does
not yet consistently follow them.

## Refactoring execution order

The initial manual platform check was interrupted and the user redirected work
to the application. Phase 1 now proceeds locally; platform qualification remains
required before claiming unattended operation. Keep each change contained and
independently reviewable/revertible; do not turn this register into a parallel
milestone system.

| Order | Task | Acceptance and verification |
| --- | --- | --- |
| 0 | Complete the existing platform investigation | Record repeated grow/shrink outcomes, timing, stalls, and dommemstat usefulness here. Separately authorized live work; no implementation expansion. |
| 9 | Finish Windows verification | Build a Windows binary and run the `windows` crate tests on Windows (they type-check on Linux but cannot run there; this includes the stop-signal interleaving test). Then verify native service behaviour. |

For authorized implementation changes, the Linux regression gate while xtask
remains is:

```bash
cargo test -p virtio-mem-core -p virtio-mem-host -p virtio-mem-xtask --locked
```

Run focused checks first, then the non-Windows workspace gate:

```bash
cargo test --workspace --exclude virtio-mem-service --locked
```

Update the commands if xtask is removed. Windows compilation needs the chosen
target/toolchain and an actual binary build, not just `cargo check`. Native
service tests and VM behavior are separate verification steps. No builds or
tests were run to produce or record this assessment; do not interpret these
commands as completed checks or authorization for live tests.

## Expected simplification

The estimator's own ordering check (`prepare_sample`) remains as an invariant
inside core; it no longer competes with a separately persisted record. Preserve required failure and recovery behavior before removing
larger layers. The host still uses SHA-256 for attestation and fingerprints.

The intended result has three clear responsibilities: shared contracts and
pure policy (including pool allocation), a host control loop serving every
pooled VM, and a Windows telemetry service only if needed.
Exact code reduction depends on the platform result and deliberately retained
behaviors. No code-reduction percentage or completed refactor is claimed.
