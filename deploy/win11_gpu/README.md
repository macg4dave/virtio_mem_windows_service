# win11_gpu deployment

The reviewed configuration for `win11_gpu`. Edit here, then apply. Generated
files (attestation, deploy evidence) stay in `.artifacts/`.

| File | Installed as | Apply |
|---|---|---|
| [host.toml](host.toml) | `/etc/virtio-mem-host/win11_gpu.toml` | `deploy/win11_gpu/install-host.sh` |
| [recover-latch.sh](recover-latch.sh) | — | see below |
| [windows-manifest.toml](windows-manifest.toml) | `C:\ProgramData\VirtioMemService` config + service | `make win11-deploy` |

`make win11-deploy` installs the build last fetched by `make win11-build`.
How make reaches the guest (SSH alias, guest checkout, timeouts) is set in
[windows-build.mk](windows-build.mk). Watch the live loop with
`cargo xtask watch --config deploy/win11_gpu/host.toml`.

## Recording a stress test

```bash
cargo xtask metrics --config deploy/win11_gpu/host.toml [--duration-seconds N]
```

Read-only, like `watch`. On a terminal it opens a live UI: host, device and
Windows panels, a memory chart (host available, Windows available, device
current and requested), request results and latencies, problem counters and
the controller's events. `--plain`, or output that is not a terminal, prints
one line per sample instead. It records until `q`/Ctrl+C or
`--duration-seconds`.

| Key | Action |
|---|---|
| `-` / `+` | Sample period (0.5 s – 120 s; default 5 s) |
| `[` / `]` | Guest-agent read period (1 s – 120 s; default 5 s) |
| space | Pause / resume logging (the UI keeps updating; nothing is written) |
| `n` | Sample now |
| `m` | Type a marker (e.g. "started 12 GB allocation"), Enter saves |
| `s` | Save `summary.json` now |
| `w` | Chart window: 5 min, 15 min, 1 h, whole run |
| `q` | Quit |

Period changes, pauses and markers are written to `events.jsonl` as
`"event": "marker"` entries, so the logs show when each happened. Each run
writes one directory under `.artifacts/metrics/win11_gpu-<UTC time>/` (or
`--output DIR`):

| File | Contents |
|---|---|
| `samples.csv` | Every sample period (`--interval-seconds`): host total/available RAM, headroom over `host_min_headroom_bytes`, swap used, QEMU RSS, controller active, device size/requested/current. Every guest read (`--guest-interval-seconds`, default 5): agent read time and success, publication age, Windows available RAM, load, commit and commit headroom, cache, memory signal, pending request. Sizes in MiB. |
| `events.jsonl` | Every controller journal entry written during the run, with its fields split out, plus markers. |
| `summary.json` | Min/max/mean of the above, time spent with requested ≠ current, guest read failures and stale publications, completed requests by operation and outcome (MiB asked and got), grant→completion and Windows issue→completion latency, refusals, event counts, controller restarts and latches. Rewritten every minute and at exit. |

## Recovering a latched controller

When the controller latches (`actuation_latched` in `watch`/`metrics`), it
stops resizing until an operator clears it. Install the current host build
and attestation, then run

```bash
deploy/win11_gpu/recover-latch.sh "why it is safe to resume"
```

It stops the controller, abandons a stuck shrink (requested set to what the
guest kept) when requested ≠ current, clears the latch and starts the
controller again.

## How much RAM win11_gpu can have

The VM boots with 8 GiB of fixed RAM. The virtio-mem device `ua-virtiomem0`
(20 GiB, 2 MiB blocks) adds RAM on top, so Windows sees between 8 GiB and
8 + 19 = 27 GiB. The host never grants the last 1 GiB of a device. Both
limits come from the VM's libvirt XML, not from these files.

Every `*_bytes` setting in both files takes a size string such as `"8GB"`,
`"8.5GB"`, `"1024MB"` or `"512KB"`, or a plain byte count. Units are binary
(`1GB` = 1024 MB = 2^30 bytes; `GiB` is also accepted). Step sizes must be
multiples of the 2 MiB device block. The Windows deploy writes them to the
guest as byte counts. Size strings in `host.toml` need a host build from
this change or later; `install-host.sh` builds and installs one.

Each setting is explained in a comment beside it in the two files. In short:
`host.toml` sets the pool, the host's own free-RAM floor and VM priority;
`windows-manifest.toml` sets Windows' minimum RAM, how much free RAM it keeps,
step sizes and how long a surplus must last before RAM goes back.

Windows wants enough device RAM to keep `physical_reserve_bytes` available and
`commit_reserve_bytes` of commit headroom, and never less than
`configured_minimum_bytes`. It asks for growth at once and releases only after
a full history window of surplus. The host then grants what the pool,
priority and `host_min_headroom_bytes` allow.

## Adding a second VM to the pool

Give the VM a virtio-mem device, install the Windows service there with its own
manifest, add a `[[vm]]` table with its own `priority`, state path and
attestation path to `host.toml`, and pass one `--attestation VM=PATH` per VM
to `host-deploy`. `pool_bytes` is required once there is more than one VM.
