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

The Rust parser requires `stat-free` and `stat-total`. If `stat-available` is
omitted by the guest agent, it falls back to `stat-free`. Values greater than
`stat-total` are rejected as inconsistent.

## Memory Change Request

Input: Target memory size in bytes, aligned to the device block size.
Process: `virsh update-memory-device <vm> --alias <virtio-mem-alias> --requested-size <bytes> --live`

The controller must inspect live virtio-mem XML after every request. `requested` is the desired size and `current` is the size currently active in the guest; they may differ while QEMU converges.

## Stability Rules

- Do not change request/response format without updating this document
- Maintain backward compatibility with existing QEMU Guest Agent versions
- Version any breaking changes to the protocol
- Do not issue another resize while `requested` and `current` differ

## Controller Decision Contract

The Rust controller consumes parsed memory stats plus the live virtio-mem
`requested` and `current` values. It returns one of:

- `NoChange` when memory is within the hysteresis band or a safe limit has
  been reached
- `WaitForConvergence` when a previous resize is still pending
- `Request { requested_bytes }` for one aligned block of growth or removal

The controller never emits a target outside the configured minimum/maximum
range and does not perform the host-side resize itself.
