# Upstream Virtio-mem Audit

## Conclusions used by this project

- Virtio-mem has asynchronous requested and plugged/current state. A successful
  host command does not prove guest convergence.
- Live alias-scoped libvirt `current` is the repository's allocation authority.
- Upstream QGA does not define `guest-get-memory-stats`; the repository's
  adapter for that shape is experimental.
- Libvirt `dommemstat actual` is balloon state, not whole-guest or virtio-mem
  allocation. Fresh `unused` and `available` may inform demand only.
- QEMU/libvirt device geometry, backend configuration, memory slots, topology,
  and required QOM properties must be reviewed and attested before actuation.
- Windows virtio-mem unplug may be partial or delayed. Controllers must observe
  progress, avoid overlap, preserve a safe floor, and latch ambiguity.
- Multiple devices are possible upstream, but independent local controllers do
  not provide atomic shared-host reservation.

## Repository interpretation

Windows remains measurement-only. The host owns compatibility, target
calculation, allocation accounting, and mutation. Diagnostic driver output is
optional and never part of the accounting contract.

The current support statement is a trusted development integration, not
production or untrusted-guest support. Stack upgrades require a fresh audit and
new attestation rather than reuse of recorded versions or machine-specific
evidence.

## Source policy

Architecture claims should cite the applicable current Virtio specification,
QEMU documentation/source, libvirt documentation/source, QGA schema, and
virtio-win source during a compatibility review. Exact versions and hashes
belong in the generated attestation or research evidence for that review, not
in reusable deployment instructions.
