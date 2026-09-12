# Known Issues

This file tracks current product limitations. Run transcripts and resolved
experiments are intentionally excluded; current evidence belongs to an xtask
run directory or the deployment manifest.

| ID | Status | Issue | Required resolution |
| --- | --- | --- | --- |
| ISSUE-002 | Superseded by redesign | The fixed-headroom memory policy was not fully qualified and is no longer the intended release algorithm. | Preserve prior evidence as historical; qualify the Windows-native pressure policy through the revised QA gates. |
| ISSUE-003 | Open | Libvirt transport and uncertain-command failures need broader operational coverage. | Complete the fault/recovery QA tasks without weakening no-replay or latch behavior. |
| ISSUE-008 | Open | Classic Windows Event Log descriptions depend on a packaged message resource. | Package and verify the resource while preserving structured EventData and bounded messages. |
| ISSUE-011 | Accepted limitation | The signed Windows virtio-mem driver has no supported user-mode status query used by this project. | Keep optional diagnostics separate from allocation authority; driver work requires a separately approved design and external ownership. |
| ISSUE-016 | Open redesign | The implemented fixed-headroom controller is not a Windows memory-pressure model and has not completed live reclaim qualification. | Complete the native signal probe, shadow assessment, and revised applied qualification before enabling unattended actuation. |
| ISSUE-017 | Open | Supported Windows APIs expose pressure state and evidence but no general KVM-ready recommended-RAM byte target. | Validate a minimal host mapping from committed demand plus proportional buffer, with native pressure signals controlling urgency and reclaim eligibility. |

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
