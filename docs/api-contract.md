# API Contract

## QEMU Guest Agent Interface

### GetMemoryStats

Request:
```json
{
  "execute": "guest-get-memory-stats"
}
```

Response:
```json
{
  "return": [
    { "stat": "stat-free", "value": 2147483648 },
    { "stat": "stat-total", "value": 8589934592 },
    ...
  ]
}
```

### Expected Fields

- `stat-free`: Free memory in bytes
- `stat-total`: Total allocated memory in bytes
- `stat-available`: Available memory (including cache)

## Memory Change Request

Input: Target memory size in bytes, aligned to the device block size.
Process: `virsh update-memory-device <vm> --alias <virtio-mem-alias> --requested-size <bytes> --live`

The controller must inspect live virtio-mem XML after every request. `requested` is the desired size and `current` is the size currently active in the guest; they may differ while QEMU converges.

## Stability Rules

- Do not change request/response format without updating this document
- Maintain backward compatibility with existing QEMU Guest Agent versions
- Version any breaking changes to the protocol
- Do not issue another resize while `requested` and `current` differ
