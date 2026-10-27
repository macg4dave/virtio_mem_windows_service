# Roadmap

Share a host RAM pool between Windows KVM guests. **Windows decides when to
request or return RAM and sends explicit add/remove byte amounts.** Linux
validates requests, allocates free pool memory by strict VM priority and applies
virtio-mem device changes. A larger `priority` wins; Linux does not estimate
Windows demand or decide when a guest should reclaim memory.

If a guest asks for 2 GiB and only 1 GiB is free, Linux grants 1 GiB and returns
the partial result. With no free bytes it reports that the pool is empty.
Windows reassesses and can send another request. Linux does not queue the
unfulfilled remainder. Released bytes become free only when observed returned.

This document owns direction, phase status and qualification evidence.
[The backlog](backlog.md) owns open tasks and completion criteria.
[Development](development.md) explains architecture, configuration, the
viomem driver behaviour that shapes reclaim, and validation.

## Current state — 27 October 2026

The guest-driven flow is implemented, installed on `win11_gpu` and resizing
under the enabled `virtio-mem-host@win11_gpu.service`
(`/etc/virtio-mem-host/host_list.toml` and sibling settings,
`observe_only = false`, 15 GiB device pool,
4 GiB host headroom). Install evidence: `.artifacts/install/`. The live loop
can be watched read-only with
`cargo xtask watch --config deploy/config_files/host_list.toml` and recorded for
stress tests with `cargo xtask metrics` (see the deploy README).

Source checks pass: core 72, host 73 and xtask 69 tests
(`.artifacts/install/config-install-local-gate.log`), release builds,
formatting and Clippy with warnings denied. The native MSVC gate on ice101
passed all 65 Windows tests with Rust 1.97.1
(`.artifacts/install/config-install-windows-gate.log`). The Windows service
and workload executables were linked and hash-verified there; the absent local
MinGW linker does not block the native build.

Live findings so far (details and rules in development):

- **Growth works.** Foreground and installed runs grew in 1 GiB-class grants of
  5–6 s each. After release left holes, growth plugged nothing until QEMU's
  usable region was widened; the host now widens automatically once after a
  grow that plugs nothing.
- **Reclaim works only with large windows.** viomem removes whole blocks from
  a window starting at the lowest plugged block and `plugged - requested`
  long. 64 MiB steps returned 2 MiB; after use, 2 GiB returned 900 MiB and
  4 GiB returned 2.83 GiB. The guest step is now 3 GiB, and a removal that
  stops moving settles `partial` after three polls instead of the full timeout.
  Evidence: `.artifacts/reclaim-ladder/`.
- **The oldest memory does not return.** At 11:02 the device held 1.24 GiB;
  Windows was idle (High signal, 5.7 GiB available) and asked to release all
  of it every ~5 minutes, and each request returned 0 bytes. Upstream viomem
  issue #1696 can also exclude removable memory after holes. In source, each
  consecutive zero-byte removal now doubles the guest's removal pause (up to
  `max_removal_pause_windows`, 16 on `win11_gpu`) until the allocation changes.
  Windows build `1c7c44c9a813-dirty.77f7aaafe765` with that rule and
  configuration schema 6 was deployed at 11:42 on 27 October
  (`.artifacts/windows/win11_gpu/deploy.json`); the native test gate was not
  rerun for it.
- **Fixed during install:** reclaim evidence discarded during convergence;
  `--elevate` unable to prompt for sudo; attestation drift when libvirt omits
  a zero `requested-size` (which latched the VM once at 09:23); domain XML
  drift after a VM restart (domain id, dynamic SELinux label).
- **Latched 12:21–(recovery) on 27 October.** A short-settling shrink's
  pre-command check saw the QEMU argv change: libvirtd's tap `fd`/`vhostfd`
  numbers differ between argv generations. The controller then held
  requested 76 MiB / current 1068 MiB. The scrubber now ignores those
  numbers; the regenerated attestation and `deploy/config_files/recover-latch.sh`
  await install. The "4 hours without crashes" run before 15:30 was mostly
  latched, so it does not count as a stability run.

Not yet established: native SCM/ACL/Event Log checks, QGA replacing
`.host-status` while the service reads it, restarts with a request
outstanding, workload-driven growth/reclaim cycles and stability over time.
The archived pre-request-model workload runs (peak growth 1.10 GB, no accepted
reclaim) do not qualify the current flow.

## Configuration consolidation — source complete

Administrator settings now use one `global-settings.toml`, one `host_list.toml`
and one `<vm_name>_settings.toml` per VM. Core owns parsing, defaults and
validation. VM settings expose total `min_ram`/`max_ram`, installation identity
and optional diagnostic output; the former fixed-RAM configuration assumption
is removed. Host state/attestation paths and service/control identity are
derived. Policy knobs and overlapping timing/freshness settings have been
replaced by conservative defaults and shared polling.

Telemetry diagnostics are optional on both sides and failures do not prevent
control. Policy sampling and the internal request channel continue without
telemetry settings. Host deployment snapshots/hashes/backs up the whole source
set; Windows receives generated schema-7 runtime settings. Old administrator
formats fail with migration guidance; existing state and deployment backups
are retained.

Installed on 27 October: Windows build `af0cc3a6b6a3-dirty.b03ef5350961`
with schema 7, total-RAM limits 6–27 GiB and optional telemetry omitted; the
new host binary, host list, sibling settings and fresh compatibility attestation
were installed after sudo authentication in the operator terminal. Host and
Windows executable hashes match their candidates. Evidence is in
`.artifacts/install/config-install-*.json`, native/local gate logs and
`.artifacts/windows/win11_gpu/deploy.json`.

The retained descriptor-drift shrink latch was recovered using the guarded
abandon-shrink and clear-latch tools; the checkpoint was preserved. QGA then
could not open `.host-status` for writing until QEMU Guest Agent was restarted,
consistent with a retained open reader. After restart, host replies were fresh,
the pending guest request resolved, both services remained running, and four
metrics samples had zero guest-read failures and no new latches. The current
`Watch-Ram.ps1` copy was hash-checked and exercised natively for object output
and the plain dashboard. No allocation stress test or VM reboot was performed.
The VM still supplies 8 GiB boot RAM, so the configured 6 GiB lower bound does
not enable reducing this VM below its underlying boot-memory allocation.

The VM's persisted virtio-mem block size was then changed from 2 MiB to
8 MiB while it was shut off, with the original XML retained. It was started
for verification: the 20 GiB device now has 2,560 blocks, and Windows
recognised the new geometry. Fresh compatibility evidence was installed and the host
controller restarted with its checkpoint preserved. Status reported converged,
no latch and no recovery required. Evidence and original/candidate XML are in
`.artifacts/block-size-8m/`. No workload comparison has yet qualified reclaim
behaviour with the larger blocks.

A subsequent cleanup removed the stale startup memory request from the
persistent libvirt definition. Future boots start with zero plugged device RAM;
the running allocation remains owned by the guest request protocol. The installer
now requires explicit attestation inputs instead of choosing an old test-run
artifact, and generic Make targets no longer load the local Windows test preset.
The resize adapter now requires an evidence source in its constructor; its
fixed compatibility fixture is compiled only for tests, with no production
default or replacement builder. A duplicated block-alignment error string now
uses the actual validation constant. The fixed 1 GiB device reserve has been removed: targets may reach full
device capacity, while the user sets Linux host reserve protection with
`host_min_headroom_bytes`. Pool and VM RAM limits, alignment, attestation and
guarded reclaim still apply. Evidence and the original VM
XML are in `.artifacts/startup-allocation-cleanup/`.

A growth failure after the block-size change exposed a historical-result
validation bug: the retained 20 MiB partial removal from the 2 MiB device was
rejected against today's 8 MiB geometry, so Windows ignored fresh host status
and issued no requests. Historical results now retain their original byte
counts while current geometry, result identity and outcome remain validated.
A regression test reproduces the 98%-full hold before the fix and growth after
it. The fixed Windows service was installed, and an authorised bounded 5 GiB
resident workload completed in 54 seconds: the host completed a 1 GiB growth
and an 8 MiB top-up. Evidence is in `.artifacts/ram-growth-fix/`. This verifies
growth on the current geometry; it does not qualify reclaim or a cold boot.

Remaining upgrade/ACL/optional-output checks and live RAM-bound qualification
are tracked separately as CFG-001 in the backlog. Source checks and native
builds pass; neither an idle sample nor successful installation qualifies
workload-driven growth/reclaim or long-term stability.

## Retained architecture

Pure contracts, guest policy and pool allocation live in core; platform effects
stay in the host and Windows crates. Alias-scoped XML, the resize sink's final
live/compatibility checks, Linux checkpoint durability, Windows atomic
replacement and the distinction between rejection and uncertain execution are
retained. A slow synchronous host adapter can still delay other members, and
checkpoint errors can still stop the daemon.

### Decisions

Windows owns demand/reclaim; Linux owns priority allocation and execution. The
request rides in the existing atomic publication and the host answers through
a status file beside it. Both sides use the built-in control channel and
derive its sidecar paths; optional diagnostic outputs do not carry authority. Partial/empty
responses end a request without a host remainder queue; Windows reassesses on
its next sample. Zero is supported but requires unplug qualification.

Linux owns each VM exclusively by domain UUID plus checkpoint locks; operator
mutation uses a stopped controller. A command keeps its original start time
with a monotonic process bound; a settle-short command gets its own bound,
once.

Configuration is administrator-owned and read-only to the Windows service. A
worker exceeding graceful shutdown causes failed process exit. Reading a
request or a result is never an acknowledgement: the guest's pending request
and the host's admission are durable before either side publishes or acts.
Refusals before any device command are not journalled and never latch; only an
unknown or contradicted effect latches.

## Phase 1 — Correct ownership and local readiness (complete)

Source implementation, native build/test and the single-VM deployment are
done. The observe-only baseline was skipped: the controller went straight to
resizing on `win11_gpu`. The remaining native Windows checks need the live
guest and moved to Phase 2 (WIN-001).

## Phase 2 — Qualify one Windows VM (in progress)

Running on `win11_gpu` with resizing enabled. Remaining: the native service
checks, workload-driven growth/reclaim with restarts and retransmissions,
installing the zero-byte removal backoff, and a week of real-time
sizing without intervention. Remaining configuration-upgrade and controller qualification work is live;
source consolidation is complete.

Exit: recorded convergence and recovery outcomes, acceptable guest memory
behaviour, and a week of real-time sizing on `win11_gpu` without intervention.

## Phase 3 — Qualify the shared pool (partial)

The pool allocator, priority grants and shared-config tooling are implemented.
Live work depends on Phase 2. Include simultaneous requests, partial grants,
empty-pool responses and retransmissions in addition to alternating workload.

Exit: at least two Windows guests share a bounded pool under alternating and
competing load; strict priority, return of released memory and isolation of a
stuck member are demonstrated without violating host reserve limits.

## Evidence-dependent follow-up

After qualification, assess whether compatibility checks can be simplified while
retaining their safety contracts. Faster Windows reclaim needs an observable
safety metric first. Persisting guest estimator history would shorten reclaim
warm-up after a service restart if trials show it matters. Virtio-balloon
reclaim is an alternative only if unplug evidence justifies it. Linux guest
support waits until the Windows pool works. None of these options requires a
second implementation now.
