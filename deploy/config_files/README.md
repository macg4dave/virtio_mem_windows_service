# Test configuration files

These configuration files are kept here for testing with `win11_gpu`; their
long-term location will change later. Edit here, then apply. Generated files
(attestation, deploy evidence) stay in `.artifacts/`.

| File | Installed as | Apply |
|---|---|---|
| [global-settings.toml](global-settings.toml) | `/etc/virtio-mem-host/global-settings.toml` | `install-host.sh INSTANCE VM=ATTESTATION ...` |
| [host_list.toml](host_list.toml) | `/etc/virtio-mem-host/host_list.toml` | `install-host.sh INSTANCE VM=ATTESTATION ...` |
| [recover-latch.sh](recover-latch.sh) | — | see below |
| [win11_gpu_settings.toml](win11_gpu_settings.toml) | `C:\ProgramData\VirtioMemService` config + service | `make win11-deploy` |

`make win11-deploy` installs the build last fetched by `make win11-build`.
How make reaches the guest (SSH alias, guest checkout, timeouts) is set in
[windows-build.mk](windows-build.mk). Watch the live loop with
`cargo xtask watch --config deploy/config_files/host_list.toml`.

## Recording a stress test

```bash
cargo xtask metrics --config deploy/config_files/host_list.toml [--duration-seconds N]
```

Read-only, like `watch`. On a terminal it opens a live UI: host, device and
Windows panels, a memory chart (host available, Windows available, device
current and requested), request results and latencies, problem and incident
counters, and the controller's events alongside lifecycle events. A red
`N INCIDENTS` badge in the header means a crash or restart was recorded.
The controller row shows its pid, memory and CPU, or how it exited. `--plain`, or output that is not a terminal, prints
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
| `samples.csv` | Every sample period (`--interval-seconds`): host total/available RAM, headroom over `host_min_headroom_bytes`, swap used, QEMU RSS, controller active, device size/requested/current. Every guest read (`--guest-interval-seconds`, default 5): agent read time and success, publication age, Windows available RAM, load, commit and commit headroom, cache, memory signal, pending request. Appended after those: controller unit state, pid, memory, CPU % and restart count; QEMU pid; libvirt domain state (`virsh domstate --reason`); and per guest read the service session, commit peak and kernel paged/non-paged pools. Sizes in MiB. |
| `events.jsonl` | Every controller journal entry written during the run, with its fields split out, plus markers and `metrics_*` lifecycle events: controller failed/stopped/started/restarted, QEMU exited/started/restarted, domain state changes, guest service session changes, guest publication stalled/resumed, guest agent lost/restored. |
| `incidents.jsonl` | One record per incident, with its bundle path, pid and details (exit status, systemd result, panic text, last controller lines, core dump flag, publication gap). Empty when nothing crashed. |
| `incidents/NNN-TIME-COMPONENT.txt` | Evidence bundle per incident, collected at detection: panic text and last controller lines, `systemctl status`, `coredumpctl info` when the controller dumped core, the unit's last 400 journal lines, the QEMU log tail (root only) for VM incidents, the guest's last and first publication around a service restart, and the 60 `samples.csv` rows before detection. |
| `summary.json` | Min/max/mean of the above, time spent with requested ≠ current, guest read failures and stale publications, publication stall and agent outage durations, completed requests by operation and outcome (MiB asked and got), grant→completion and Windows issue→completion latency, refusals, event counts, controller pids, memory and CPU, samples per domain state, guest service sessions, incident counts and records, and latches. Rewritten every minute, at exit and after each incident. |

An incident is a controller that stops other than cleanly (any systemd
`Result` except `success`, including one already failed when recording
starts), a new guest service session (service restart or guest reboot; the
host cannot tell which), a QEMU process exit or replacement, or a domain in
`crashed`/`panicked` state. A `systemctl stop` is an event, not an incident.
Lifecycle events and incidents are written even while logging is paused.
Windows' own record of a service crash stays in the guest: System log,
Service Control Manager 7031/7034, and Application log, Application Error
1000; guest bundles include a `Get-WinEvent` query for the right time.

## Recovering a latched controller

When the controller latches (`actuation_latched` in `watch`/`metrics`), it
stops resizing until an operator clears it. Install the current host build
and attestation, then run

```bash
deploy/config_files/recover-latch.sh "why it is safe to resume"
```

It stops the controller, clears the latch and starts the controller again.
`clear-latch` first settles a stuck shrink (requested set to what the guest
kept) when requested ≠ current.

## Configuration ownership

- `global-settings.toml`: shared device pool, Linux headroom, observe-only mode
  and three controller timings. Omitted keys use built-in defaults. A missing
  global file uses all defaults; multiple VMs still require an explicit pool.
- `host_list.toml`: one `[[vm]]` per VM with `vm_name`, libvirt device `alias`
  and `priority`. Optional `telemetry_path` writes diagnostics on the Linux host.
- `<vm_name>_settings.toml`: `min_ram`, `max_ram`, `install_path`,
  `service_account`, `vm_name` and optional Windows `telemetry_path`.

The host list loads sibling files; paths do not depend on the working directory.
VM names use lower-case ASCII letters, digits, `-` and `_`, beginning with a
letter or digit, at most 64 characters. The exact filename and internal name
must agree. Duplicate entries and unknown keys fail validation.

`min_ram` and `max_ram` describe **total guest RAM**, in binary size units or
bytes, aligned to the live device block size (64 MiB for `win11_gpu`).
The policy derives the device allocation from observed
RAM and live device state; there is no configured boot/base-RAM value. Both
Windows requests and host grants honour these bounds. The underlying VM must
have enough supported memory capacity; impossible limits fail closed. This
change does not edit libvirt VM definitions.

Guest reserve/floor thresholds, grow/reclaim steps, reclaim history, hysteresis
and stop timeout have conservative built-in defaults. One polling
interval drives host polling and guest sampling; freshness, request lifetime
and maximum sample gaps follow it. Device size and block size come from live
state. `/usr/bin/virsh`, the Windows service name and control-file path are
built in. Per-VM state and attestation paths are derived from the validated name.
The current memory-policy defaults are: 2 GiB physical/commit reserves,
1 GiB safety floors, demand-sized growth (at least 1 GiB for native Low
pressure), 3 GiB reclaim steps, 1-second reclaim
history, 256 MiB hysteresis and 30-second stop timeout.
Freshness/request lifetime is six poll intervals and the maximum sample gap is
three. Windows service identity is `VirtioMemService`.


## Optional telemetry and the control channel

Both optional telemetry paths may be omitted. No diagnostic publisher or output
file is created in that case. When configured, the guest path writes Windows
diagnostics and the host path writes a separate Linux diagnostic file. Output
failures are reported without stopping memory control. Provision write access
for the service account if choosing a custom diagnostic directory.

Memory sampling needed by the guest policy and the request/reply channel remain
active. The internal control channel retains its historical filename
`C:\ProgramData\VirtioMemService\telemetry.json` to preserve pending requests;
it is no longer a telemetry setting. Its `.host-status` and request-state
sidecars are derived automatically. Do not use this path or its sidecars as an
optional diagnostic output. `watch` and `metrics` read the control data when
explicitly run and require no telemetry settings.

## Startup memory

The persistent libvirt definition starts virtio-mem with zero requested bytes.
No startup allocation is stored in these application settings. Windows requests
device RAM when its memory policy needs it; the boot-memory allocation remains
a property of the VM definition. Updating the saved startup setting does not
force-unplug memory from an already running guest.

## Deployment and migration

The installer requires the controller instance and one explicit, current
attestation per VM; it never selects evidence from an old test run:

```bash
deploy/config_files/install-host.sh win11_gpu win11_gpu=/path/to/current-attestation.json
```

`install-host.sh` validates and snapshots the global settings, host list and
all listed VM files, hashes the complete snapshot and installs the same files
under `/etc/virtio-mem-host/`. Existing installed files and obsolete instance
configs are backed up. The unit now reads `host_list.toml`. Run one controller
for the host list: separate controllers cannot share pool accounting.

`make win11-deploy` reads the selected `<vm_name>_settings.toml` and sibling
global settings and generates Windows runtime `config.json` schema 8. This
JSON is a deployment output, not another administrator settings file. Deploy
matching new host and Windows binaries before runtime verification.

The tracked values were carried over: 15 GiB device pool, 4 GiB host headroom,
5/10/300-second timings, priority 100, Windows account and executable path.
The reviewed VM now has explicit total-RAM limits of 6–27 GiB. Its previous
telemetry path remains the internal channel, so no optional output is enabled.

For other deployments, keep copies of the old files before migration. Rename
`host.toml` to `host_list.toml`, use `vm_name` instead of `name`, and rename
`vm-settings.toml` to `<vm_name>_settings.toml` with the new field set. Create
one `<vm_name>_settings.toml` per entry. Remove `global_settings` imports and
move global values into the sibling `global-settings.toml`. Old manifests,
inline VM/global configs and JSON administrator manifests are rejected with a
migration error rather than silently accepted. Review custom policy tuning
against the documented built-in defaults before replacing it. Custom service
names, control paths or checkpoint locations require explicit state migration;
do not discard pending requests, checkpoints or latches to get past validation.
Windows deployment retains configuration/binary/registry backups and refuses
an automatic upgrade from older wire schemas when their policy or channel
settings differ from the generated settings.

## Adding another VM

Create its own `<vm_name>_settings.toml`, add its identity, alias and priority
to `host_list.toml`, and pass one `--attestation VM=PATH` per VM to `host-deploy`.
The installed attestation is `/etc/virtio-mem-host/<vm_name>.attestation.json`;
retained state is `/var/lib/virtio-mem-host/state/<vm_name>-requests.json`.
Deploy the Windows service separately for each guest. `pool_bytes` describes
shared **device** RAM; the per-VM bounds describe total RAM.

### Prompt memory notifications

Add the optional dedicated port while the controller is stopped, then refresh
compatibility attestation before restarting it. Do not reuse the QEMU guest-agent
port. Libvirt supplies the Unix socket path; the host discovers it in domain XML.

```xml
<channel type='unix'>
  <target type='virtio' name='org.virtio-mem.wake.0'/>
  <alias name='ua-virtiomem-wake'/>
</channel>
```

The host service needs the `qemu` supplementary group to connect to libvirt's
socket. Windows deployment provisions LocalService read/write access only on
the named wake-port device instance, backs up its previous security property
and restarts that port if changed. It does not alter the QEMU guest-agent port.
Installations without this port continue polling and report the missing wake
channel. Polling remains a fallback; native Low pressure, result arrival and
libvirt size-change events can wake earlier. Notification bytes carry no RAM
values or authorisation. The same durable protocol and reserve checks apply.
