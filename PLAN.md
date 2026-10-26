# virtio-mem Windows service — Plan

This file replaces all previous docs (readme, BACKLOG, PROJECT_STATUS,
IMPLEMENTATION_PLAN, `docs/*`, and the `.github/prompts/*` set). They described
a 17-milestone roadmap and ~40 QA gates. Very little of that had run against a real VM.
Git history still has them if anything needs recovering.

## What this project is for

Give a Windows KVM guest more RAM when it needs it, and take RAM back when it
doesn't, by resizing a virtio-mem device from the host:

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

## Honest current state (2026-10-26)

| Item | Reality |
| --- | --- |
| Code size | ~30k lines of Rust for a sampler plus a control loop |
| `virtio-mem-core` + `host` tests | Pass (96 + 85) |
| `tools/xtask` | **Does not compile** at HEAD (`QualificationAnalysis` missing `lower_request_violations`, `tools/xtask/src/qualification.rs`) |
| `windows` crate | Windows-only; can't be built or tested on the Linux dev host |
| Automatic resizing | **Has never been enabled.** The installed host unit was always "disabled/inactive"; runs were shadow/no-resize |
| Multi-VM pool (`global_pool.rs`, `pool_ledger.rs`, ~2.4k lines) | Dead code: nothing outside the core crate uses it |
| `behavior_evidence.rs` (~500 lines) | Only reachable from a host CLI subcommand |
| Host config | ~45 `VIRTIO_MEM_*` env vars, 3 demand modes, 2 transports, shadow/growth/legacy policy modes |

Conclusion: the project spent its effort on contracts, gates and paperwork
around a loop that never closed. Whether Windows virtio-mem grow/shrink behaves
well enough for this is **still unproven**. That is the first thing to find
out.

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

### Phase 0 — Prove the platform by hand (no project code)

1. On the test VM, run `virsh update-memory-device VM --alias ALIAS --requested-size X`
   to grow, then to shrink, several times. Watch `virsh dumpxml` `current` and
   Windows Task Manager.
2. Record: does grow work every time? How long does it take? Does shrink
   complete, partially complete, or hang? Under memory load?
3. Check whether `virsh dommemstat VM` returns useful `unused`/`available` on
   Windows with the balloon driver + stats period set. If yes, **we may not
   need a Windows service at all.**

**Exit:** a short written result. If shrink is unreliable, the product is
grow-only plus a manual or reboot-time reset, and shrink waits until it's fixed.

### Phase 1 — Cut to a minimum working loop

Keep:

- Windows service: sample `GlobalMemoryStatusEx` and commit info every N
  seconds, write one JSON line atomically. Install/run as a service. (Skip
  this if Phase 0 says dommemstat is enough.)
- Host daemon, one VM, one device:
  1. read guest stats (QGA file read or dommemstat)
  2. read live device XML (`requested`, `current`, block size, max)
  3. `target = clamp(commit_used + headroom, min, max)`, aligned to block
  4. grow immediately if `target > current + step`; shrink only after
     sustained low usage (hysteresis + cooldown), in small steps
  5. one command in flight; wait for `current == requested` with a timeout;
     on timeout, stop resizing and log loudly
  6. refuse to grow if host MemAvailable is below a reserve
- One config file (TOML) with about 10 settings: vm, alias, min, max,
  headroom, step, poll interval, shrink cooldown, host reserve, enable_shrink.
- Unit tests for the target math and the XML parsing.

Delete:

- `global_pool`, `pool_ledger`, `behavior_evidence` (dead or CLI-only).
- `pressure_assessment` and the shadow/growth/legacy policy modes in
  `host/src/target_policy.rs`; replace them with the single target formula above.
- `shrink_recovery` until shrink is proven, compatibility attestation,
  controller-status v3, the `GuestStats`/custom QGA memory-command path, and
  the unused parts of `core::demand` (e.g. `DemandCalculator`).
- Most of `tools/xtask`: qualification, deployment, host_deploy, calibration,
  preflight. Keep at most a `cargo xtask windows-build` cross-compile helper.
  Plain `cargo test` is the gate.

**Exit:** the code is a few thousand lines, `cargo test` passes, the Windows
binary cross-compiles (`x86_64-pc-windows-gnu` or `-msvc`), and you can explain the whole thing in five minutes.

### Phase 2 — Run it for real, grow-only

- Deploy to the test VM with `enable_shrink = false`.
- Run memory-hungry workloads in the guest. Confirm it grows in time and the
  guest doesn't page heavily or OOM.
- Run for a few days. Watch the logs for stalls.

**Exit:** grow works unattended.

### Phase 3 — Shrink

- Turn on shrink with conservative cooldowns and small steps.
- Verify partial/stalled unplug is detected and the loop stops cleanly
  instead of fighting the guest.
- Only then consider Windows memory-resource notifications as a faster
  "grow now" trigger.

**Exit:** a week of grow and shrink on the test VM without manual intervention.

### Later, only if needed

- Multiple VMs sharing a host RAM budget with priorities. The deleted pool
  code is in git history if it's ever wanted. Build it only after Phases 2–3
  hold on one VM.
- Linux guest support (virtio-mem in Linux is mature; this would be easier
  than Windows).

## Working rules

- Rust only. Keep it small. No new abstraction until there are two real users of it.
- Ship a feature only after it has worked on a real VM. Contracts and gates
  don't count.
- Run focused `cargo test`, then `cargo test --workspace` (excluding
  `windows` on Linux), before committing.
- Live VM actions: state the VM, alias, target size and rollback before
  running. Ask before reboots or anything irreversible.
