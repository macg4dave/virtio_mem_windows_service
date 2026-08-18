# QEMU Guest Agent Setup & Validation

This document provides step-by-step instructions for setting up and validating QEMU Guest Agent communication between the RHEL host and Windows 11 guest.

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

If it is not installed:

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

# Expected output (JSON with capabilities):
# {"return":{"version":"x.x.x","capabilities":["guest-get-memory-stats",...]}}
```

If you get an error like `"command not found"` or `"timed out"`, verify:

- Windows service is running: `Get-Service QEMU-GA` on Windows
- Channel is properly configured in domain XML
- VM was restarted after XML changes
- QEMU/libvirt versions support Guest Agent

## Step 4: Validate Memory Stats API

Test the memory stats endpoint:

```bash
virsh qemu-agent-command win11_gpu '{"execute":"guest-get-memory-stats"}'

# Expected output (example):
# {
#   "return": [
#     { "stat": "stat-free", "value": 2147483648 },
#     { "stat": "stat-total", "value": 8589934592 },
#     { "stat": "stat-available", "value": 3221225472 }
#   ]
# }
```

**Field meanings**:

- `stat-free`: Free memory in bytes (not including caches)
- `stat-total`: Total allocated memory in bytes
- `stat-available`: Available memory including caches/buffers

## Step 5: Test Command Execution (Optional)

To verify command execution capability (used for future diagnostics):

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
- [ ] Response latency for guest-get-memory-stats (milliseconds)
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

The helper validates `guest-info` once and then validates the configured
memory-stat source for the requested number of attempts. When
`guest-get-memory-stats` is unavailable, it fails over to `virsh dommemstat`
and requires numeric `actual` and `unused` fields. It defaults to
`qemu:///system`; set `VIRSH_CONNECT` to use another libvirt URI. It does not
resize memory, restart the VM, or execute commands inside the guest.

## Troubleshooting

### Issue: "timed out"

**Cause**: Guest Agent not responding, service not running, or channel not configured.

**Solution**:

1. Restart QEMU Guest Agent: `Restart-Service QEMU-GA` (Windows)
2. Verify channel in XML: `virsh dumpxml win11_gpu | grep -A 3 channel`
3. Restart VM: `virsh reboot win11_gpu`
4. Check QEMU logs on host: `journalctl -u libvirtd -f`

### Issue: "command not found"

**Cause**: QEMU Guest Agent doesn't support `guest-get-memory-stats` or old version.

**Solution**:

1. Verify agent version supports the command
2. Try `guest-get-fsinfo` as a fallback test command
3. Check QEMU version is 2.12+ (when guest-get-memory-stats was added)

### Issue: JSON parsing errors

**Cause**: Shell escaping issues or malformed commands.

**Solution**:

1. Use single quotes in bash to avoid shell expansion
2. Double-escape backslashes: `\\\\` in Windows paths
3. Use `jq` to format and validate JSON: `... | jq .`

## Example: Manual Integration Checks

The repository helper above replaces the earlier ad-hoc script. Equivalent
manual checks are:

```bash
#!/bin/bash

VM_NAME="win11_gpu"
virsh qemu-agent-command "$VM_NAME" '{"execute":"guest-info"}' | jq .
virsh qemu-agent-command "$VM_NAME" '{"execute":"guest-get-memory-stats"}' | jq .
```

The helper scripts are intended to be executable files in the checkout. If the
checkout does not preserve executable bits, run `chmod +x scripts/*.sh`.

## Success Criteria

You've successfully set up QEMU Guest Agent when:

- [ ] `virsh qemu-agent-command` returns JSON responses (not errors)
- [ ] `guest-get-memory-stats` returns memory values consistently
- [ ] Multiple consecutive commands succeed without timeout
- [ ] Responses are documented and reviewed

## Next Steps

Once validated:

1. Update [docs/api-contract.md](api-contract.md) with observed response formats
2. Add expected latency measurements
3. Document any version-specific workarounds
4. Proceed with the Rust service runtime work in TASK-001.
