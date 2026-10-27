# Roadmap

Share a host RAM pool between Windows KVM guests. Each guest publishes raw
memory telemetry; the host estimates demand and adjusts its virtio-mem device.
When free pool memory is scarce, a larger `priority` wins. Guests release memory
through their own controllers; the pool does not force reclamation.

This document owns direction, phase status and qualification evidence.
[The backlog](backlog.md) owns open tasks and completion criteria.
[Development](development.md) explains architecture, configuration and validation.

## Current state — reviewed 27 October 2026

The implementation is ahead of its live qualification. Source inspection
confirms the following; this documentation review ran no builds or VM tests.

| Area | Status | Evidence and limits |
| --- | --- | --- |
| Shared contracts and policy | Implemented | Core contains telemetry validation, alias-selected XML parsing, target estimation, reconciliation and pool allocation, with unit tests. |
| Windows telemetry service | Implemented; native verification outstanding | Raw sampling, atomic publication, SCM lifecycle and a workload utility exist. The previous plan records a cross-target Clippy pass, not a current Windows binary or native test run. |
| Host controller | Implemented; live qualification partial | TOML configuration, telemetry acceptance, durable intent/latches, host reserve checks and bounded resizing are wired into the runtime. |
| Shared pool | Implemented; live verification outstanding | The runtime observes all VMs, allocates free bytes by strict priority, then acts. No live multi-VM qualification is recorded. |
| Operational tooling | Implemented; shared-config support partial | xtask provides build, deployment, diagnostics and qualification commands. Some workflows still assume one VM per config. |
| Unattended operation | Outstanding | Historical growth is recorded; reliable reclaim and a week of unattended operation are unproven. |

### Evidence still relevant to qualification

These observations were recorded in the former plan, not rechecked during this
review. Re-establish live state before any operational work.

- Archived workload run `qualification-1789231813766-72447` recorded peak growth
  of 1,098,907,648 bytes and no accepted reclaim. A later passing run required
  no resizing and therefore did not qualify automatic resizing.
- On 26 October, `win11_gpu` / `ua-virtiomem0` had a 20 GiB device, 2 MiB blocks
  and a 1 GiB requested/current baseline. A 64 MiB grow converged in about
  4.297 seconds. Restoration reclaimed only 10 MiB before observations stopped
  at about 225.739 seconds after authentication was cancelled. Restoration and
  the full 300-second deadline were not observed; no recovery was recorded.
- QGA file telemetry was fresh at
  `C:\ProgramData\VirtioMemService\telemetry.json`. Balloon statistics also
  advanced, but raw Windows telemetry is the implemented demand source. The
  earlier proposal to select a different source is superseded.
- The installed `virtio-mem-host@win11_gpu.service` was disabled/inactive with
  `Restart=no`. Its old `.conf` was a non-resizing shadow deployment; migration
  must preserve that intent with `observe_only = true`.
- The Windows SSH alias resolved to an unreachable endpoint and the earlier
  workload executable was absent. Deployment readiness remains unverified.
- Local libvirt group membership made the repository polkit rule redundant on
  that host. Dedicated-account access must still be checked for each deployment.

The interrupted manual test grants no continuing permission for live mutations.
Future qualification uses the application through the bounded foreground runner.

## Phase 1 — Finish local readiness (partial)

Completed: one raw-telemetry control path, one host TOML format, shared
checkpointed acceptance/intent, pure policy and unit-test coverage. Retain xtask;
it is the existing operational tool, not a candidate for wholesale deletion.

Remaining: produce the selected Windows binary, run native tests and service
checks, and prepare the deployment migration. Correct the standalone resize
tool's recovery path before relying on that command for qualification.

Exit: local gates and native Windows verification pass; deployment inputs and
recovery procedures are ready for the single-VM trial. Live access is separately
authorised. Phase 1 does not establish reliable unplug behaviour.

## Phase 2 — Qualify one Windows VM (outstanding)

Depends on Phase 1 readiness. First inspect decisions with `observe_only = true`
under `host-run`; then run bounded workload trials with resizing enabled. Verify
growth, reclaim, short-shrink settlement and renewed pressure. Start with small,
block-aligned reclaim steps and a long history window. Test the real systemd
unit only after foreground trials pass.

Exit: recorded convergence and recovery outcomes, acceptable guest memory
behaviour, and a week of real-time sizing on `win11_gpu` without intervention.
The earlier interrupted platform investigation is part of this qualification,
not a separate manual testing phase.

## Phase 3 — Qualify the shared pool (partial)

Pool source is already implemented. Live work depends on Phase 2; operational
tooling must also handle the chosen shared-config deployment layout.

Exit: at least two Windows guests share a bounded pool under alternating and
competing load; strict priority, return of released memory and isolation of a
stuck member are demonstrated without violating host reserve limits.

## Evidence-dependent follow-up

After qualification, assess whether compatibility/diagnostic tooling can be
simplified while retaining its safety contracts. Faster reclaim needs an
observable safety metric first. Memory-resource notifications are already
sampled; using them as a faster growth trigger is a possible policy change.
Virtio-balloon reclaim is an alternative only if unplug evidence justifies it.
Linux guest support waits until the Windows pool works. None of these options
requires a second implementation now.
