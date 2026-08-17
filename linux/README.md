# Linux Controller Service

Go service that monitors Windows memory availability and adjusts virtio-mem allocation.

## Structure

- `cmd/controller` - Main service entry point
- `pkg/qemu` - QEMU Guest Agent client
- `pkg/libvirt` - libvirt interaction
- `pkg/metrics` - Memory calculation and logging

## Build

```bash
go build -o virtio-mem-controller ./cmd/controller
```

## Test

```bash
go test ./...
go vet ./...
gofmt -w .
```

## Development

Requires:
- Go 1.20+
- libvirt libraries (libvirt-devel on RHEL)
- QEMU with Guest Agent support

## Running

```bash
./virtio-mem-controller --interval 10s --min-memory 8 --max-memory 28
```

See [../../BACKLOG.md](../../BACKLOG.md) for task assignments and [../../docs/architecture.md](../../docs/architecture.md) for design details.
