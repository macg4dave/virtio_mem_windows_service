# Known Issues

This file tracks current product limitations. Run transcripts and resolved
experiments are intentionally excluded; current evidence belongs to an xtask
run directory or the deployment manifest.

| ID | Status | Issue | Required resolution |
| --- | --- | --- | --- |
| ISSUE-002 | Open | Memory-policy tuning has not been qualified against a current coherent deployment. | Complete TASK-028 with explicit run inputs and review the resulting demand and reclaim evidence. |
| ISSUE-003 | Open | Libvirt transport and uncertain-command failures need broader operational coverage. | Complete the fault/recovery QA tasks without weakening no-replay or latch behavior. |
| ISSUE-008 | Open | Classic Windows Event Log descriptions depend on a packaged message resource. | Package and verify the resource while preserving structured EventData and bounded messages. |
| ISSUE-011 | Accepted limitation | The signed Windows virtio-mem driver has no supported user-mode status query used by this project. | Keep optional diagnostics separate from allocation authority; driver work requires a separately approved design and external ownership. |
| ISSUE-016 | Open qualification | The implemented target controller has not completed current live workload and platform-reclaim qualification. | Complete TASK-028 and QA-T009 onward with explicit run inputs and durable evidence. |

## Standing risk controls

- Never select the full device capacity as a test target. Core device headroom,
  configured target bounds, host reserve, and live preflight all apply.
- A stalled or ambiguous operation latches actuation; it does not trigger blind
  retries or infer a new desired target.
- Current allocation comes from the selected live libvirt device. Balloon and
  Windows telemetry do not replace it.
- The custom QGA memory command is experimental and absent from upstream QGA;
  maintained probes use QGA only for health and identity.
- One controller/device is supported until the global pool provides atomic
  reservation.

Resolved implementation work is summarized in `BACKLOG.md` and covered by
focused regression tests. Do not reintroduce completed incident timelines as
current operating instructions.
