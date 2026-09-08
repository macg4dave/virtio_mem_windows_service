# QEMU Guest Agent Setup & Validation

This document provides setup and host-side validation guidance for QEMU Guest
Agent communication. QGA is a host health/operations channel; the Windows
demand service uses native telemetry and does not open the QGA device.

## Prerequisites

- RHEL host with libvirt and QEMU installed
- Windows 11 guest VM (named `win11_gpu` in examples)
- Administrator access on both host and guest
- Network connectivity or shared storage

## Step 1: Verify QEMU Guest Agent on Windows

On Windows 11, check that the QEMU Guest Agent service is installed and running. This project does not embed any PowerShell automation; the service check is a runtime validation step on the guest.

If the service does not exist:

On RHEL 10, the QEMU guest tools/driver ISOs are normally provided by the `virtio-win` package. Check with:

```bash
rpm -ql virtio-win
```

The ISO is typically:

```text
/usr/share/virtio-win/virtio-win.iso
```

Check whether it is installed:

```bash
rpm -q virtio-win
ls -lh /usr/share/virtio-win/
```

Installing packages, editing domain XML, or rebooting the guest mutates
protected resources. Run the following only after explicit approval names the
target and rollback. If `virtio-win` is not installed, the administrator may
use:

```bash
sudo dnf install virtio-win
```

To find any related ISO files regardless of package:

```bash
sudo find /usr/share /var/lib/libvirt -iname "*.iso" 2>/dev/null
```

For a Windows 11 VM, `virtio-win.iso` contains the VirtIO drivers such as balloon, memory, network, storage, and guest agent components.

## Step 2: Configure Guest Agent Channel in libvirt

On the RHEL host:

```bash
# Edit the domain XML for the Windows guest
virsh edit win11_gpu
```

Inside the `<devices>` section, add:

```xml
<channel type='unix'>
  <target type='virtio' name='org.qemu.guest_agent.0'/>
  <address type='virtio-serial' controller='0' bus='0' port='1'/>
</channel>
```

**Important**: Do not remove or modify any existing devices.

Save and exit the editor. If necessary, restart the VM:

```bash
virsh reboot win11_gpu
# Or force restart if needed:
# virsh destroy win11_gpu
# virsh start win11_gpu
```

## Step 3: Test Guest Agent Connectivity

On the RHEL host, verify the guest agent is available:

```bash
# Test basic guest-info command
virsh qemu-agent-command win11_gpu '{"execute":"guest-info"}'

# Expected output: JSON with the installed version and capability list.
```

If you get an error like `"command not found"` or `"timed out"`, verify:

- Windows service is running: `Get-Service QEMU-GA` on Windows
- Channel is properly configured in domain XML
- VM was restarted after XML changes
- QEMU/libvirt versions support Guest Agent

## Step 4: Probe the optional custom memory extension

`guest-get-memory-stats` is not defined by upstream QGA in the reviewed QEMU
9.1, 10.1, or master schemas. Do not upgrade upstream QGA expecting this
command to appear. The following read-only request is only a capability probe
for an explicitly identified custom/downstream guest agent:

```bash
virsh qemu-agent-command win11_gpu '{"execute":"guest-get-memory-stats"}'

# Possible custom-extension output (example):
# {
#   "return": [
#     { "stat": "stat-free", "value": 2147483648 },
#     { "stat": "stat-total", "value": 8589934592 },
#     { "stat": "stat-available", "value": 3221225472 }
#   ]
# }
```

If upstream QGA returns `command ... has not been found`, that is the expected
result and does not indicate broken QGA connectivity. If a custom agent
implements the extension, its repository-specific field meanings are:

- `stat-free`: Free memory in bytes (not including caches)
- `stat-total`: Total allocated memory in bytes
- `stat-available`: Available memory including caches/buffers

## Step 5: Test Command Execution (Optional)

This executes a process inside the protected guest and is not part of the
normal read-only validation path. Use it only after explicit approval names
the guest command and expected effect. To verify command execution capability:

```bash
virsh qemu-agent-command win11_gpu \
'{"execute":"guest-exec","arguments":{"path":"C:\\\\Windows\\\\System32\\\\cmd.exe","arg":["/c","echo hello"],"capture-output":true}}'

# Expected output (PID for async command):
# {"return":{"pid":1234}}

# Then poll for exit status:
# virsh qemu-agent-command win11_gpu \
# '{"execute":"guest-exec-status","arguments":{"pid":1234}}'
```

## Step 6: Document Observed Behavior

Record the following for the project documentation:

- [ ] QEMU version: `qemu-system-x86_64 --version`
- [ ] libvirt version: `virsh version`
- [ ] Guest Agent version: (captured from guest-info output)
- [ ] Whether an exact custom/downstream `guest-get-memory-stats` provider is
      installed; otherwise record the expected absence
- [ ] Response latency and `last-update` freshness for the configured host
      memory-stat source
- [ ] Consistency: Run 3+ consecutive queries and verify results
- [ ] Socket stability: Test 100+ rapid consecutive commands

## Step 7: Run the repository validation helper

From the repository root on the RHEL host, run the prerequisite check first:

```bash
bash scripts/check-environment.sh
```

Then run the explicit-scope probe. The VM name is required; no VM is selected
implicitly:

```bash
bash scripts/validate-guest-agent.sh win11_gpu 3
```

The helper validates `guest-info` once, probes the custom memory extension,
and then validates the configured memory-stat source for the requested number
of attempts. When the extension is unavailable, it falls back to
`virsh dommemstat` and requires numeric `actual`, `unused`, and `available`
fields. The Rust host source additionally enforces recent, non-future, advancing
`last-update` before using a sample for policy.
It defaults to
`qemu:///system`; set `VIRSH_CONNECT` to use another libvirt URI. It does not
resize memory, restart the VM, or execute commands inside the guest.

## Troubleshooting

### Issue: "timed out"

**Cause**: Guest Agent not responding, service not running, or channel not configured.

**Solution** (inspection is read-only; restart/reboot requires separate
approval):

1. Restart QEMU Guest Agent: `Restart-Service QEMU-GA` (Windows)
2. Verify channel in XML: `virsh dumpxml win11_gpu | grep -A 3 channel`
3. Restart VM: `virsh reboot win11_gpu`
4. Check QEMU logs on host: `journalctl -u libvirtd -f`

### Issue: "command not found"

**Cause**: Upstream QEMU Guest Agent does not define
`guest-get-memory-stats`; absence is independent of upstream version.

**Solution**:

1. Verify the advertised capability list rather than assuming support from a
   version number.
2. Use the repository helper and its `dommemstat` default when the custom
   command is absent. The Rust host source enforces M9e freshness and correct
   balloon semantics before policy evaluation.
3. Do not treat an absent Windows QGA command as a Windows service failure.

### Issue: JSON parsing errors

**Cause**: Shell escaping issues or malformed commands.

**Solution**:

1. Use single quotes in bash to avoid shell expansion
2. Double-escape backslashes: `\\\\` in Windows paths
3. Use `jq` to format and validate JSON: `... | jq .`

## Example: Manual Integration Checks

The repository helper above replaces the earlier ad-hoc script. Equivalent
manual connectivity and optional-extension checks are:

```bash
#!/bin/bash

VM_NAME="win11_gpu"
virsh qemu-agent-command "$VM_NAME" '{"execute":"guest-info"}' | jq .
virsh qemu-agent-command "$VM_NAME" '{"execute":"guest-get-memory-stats"}' | jq .
```

The helper scripts are intended to be executable files in the checkout. If the
checkout does not preserve executable bits, run `chmod +x scripts/*.sh`.

## Success Criteria

QGA setup is successful for the current project when:

- [ ] `virsh qemu-agent-command` returns JSON responses (not errors)
- [ ] Advertised upstream QGA commands such as `guest-info` return valid JSON
- [ ] The optional custom memory extension is explicitly identified, or its
      expected absence is recorded and `dommemstat` is observable
- [x] M9e validates `dommemstat last-update` freshness and preserves balloon
      `actual` semantics without using it as allocation or total memory
- [ ] Multiple consecutive commands succeed without timeout
- [ ] Responses are documented and reviewed

## Next Steps

Once validated:

1. Update [docs/api-contract.md](api-contract.md) with observed response formats
2. Add expected latency measurements
3. Document any version-specific workarounds
4. Continue with the current Ready Queue in `BACKLOG.md`; TASK-001 is complete.

## Current validated guest

As of 2026-09-04, `win11_gpu` reports QGA `110.0.2`. Repeated `guest-info`
calls, isolated QGA restart recovery, and graceful guest reboot recovery pass.
This build does not implement `guest-get-memory-stats`; repeated numeric
`dommemstat` samples show the default source is observable. They do not prove
freshness or make balloon `actual` authoritative for whole-guest/virtio-mem
allocation; M9e owns that correction. These facts do not imply that the
Windows demand service uses the QGA channel.
