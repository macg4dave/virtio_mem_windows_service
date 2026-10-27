# viomem issues found while driving virtio-mem from a host controller

Draft upstream reports for
[virtio-win/kvm-guest-drivers-windows](https://github.com/virtio-win/kvm-guest-drivers-windows),
based on the `viomem` source at master `f7c8beb` (26 October 2026) and live
measurements on 27 October 2026. Issue 2 extends the existing upstream
[#1696](https://github.com/virtio-win/kvm-guest-drivers-windows/issues/1696);
issues 1 and 3 have no upstream report yet.

## Environment

| Item | Value |
| --- | --- |
| Guest | Windows 11 Pro, build 26300 |
| Driver | VirtIO Viomem Driver 100.102.104.29400 (oem11.inf) |
| QEMU | qemu-kvm-10.1.0-16.el10_2.5, x86-64, `pc-q35-rhel10.2.0` |
| Device | `virtio-mem-pci`, region 20 GiB at `0x280000000`, block 2 MiB, `dynamic-memslots=on`, `unplugged-inaccessible` |
| Other | VFIO GPU passthrough; balloon present but inactive |
| Control | Host sets `requested-size` with `virsh update-memory-device --live` |

## 1. Plugging ignores `usable_region_size`, so growth fails permanently after unplug leaves holes

**Summary.** `VirtioMemAddPhysicalMemory()` plugs at the start of the longest
clear run in a bitmap sized to the whole `region_size`
(`viomem.c:136`, `FindFreeMemoryRangeInBitmap()` → `RtlFindLongestRunClear()`
at `viomem.c:969`). After any unplug that leaves holes, the longest clear run
is the tail above the highest plugged block T, so every plug lands at T and
holes are never refilled. QEMU accepts plug requests only inside
`addr + usable_region_size` (`virtio_mem_valid_range()`), answering
`VIRTIO_MEM_RESP_ERROR` otherwise. `usable_region_size` is
`requested_size + 256 MiB` on x86-64 but never shrinks except on system reset
(`virtio_mem_resize_usable_region(..., can_shrink=false)`), so it is the
largest `requested_size` ever set plus 256 MiB. The driver does not retry
with a smaller or lower range after the error.

So once the plugged top reaches the usable edge, **every further grow fails**,
whatever size is requested, even with free blocks below T. The cap on plugged
memory is roughly `usable edge − holes below T`.

**Observed.** With holes of about 2.6 GiB and a usable edge at 4.25 GiB (an
earlier request of 4 GiB), growth stopped at 1638 MiB plugged. After a request
of 0 removed 512 MiB from the top, it stopped again at 1640 MiB. Each failed
grow left `current` unchanged, and Windows-visible RAM always equalled fixed RAM
plus `current`. With the VM paused for 0.14 s, setting `requested-size` to the
device ceiling and straight back to `current` widened the usable region
without plugging anything. Growth then continued in 256 MiB steps to 3.17 GiB
and by +1 GiB at once, and kept working after every release in a later test.

**Expected.** Plug requests stay inside the usable region and fill holes
first.

**Suggested fix.** In `VirtioMemAddPhysicalMemory()`, search only bits below
`usable_region_size / block_size`, prefer the lowest clear run (or several
runs) and cap each request to what fits. Iterate until
`requested_size − plugged_size` is satisfied or no clear block remains. On
`VIRTIO_MEM_RESP_ERROR`, retry with a smaller range instead of giving up.

## 2. `RtlFindLongestRunSet()` never resets its run counter (extends #1696)

**Summary.** The local `RtlFindLongestRunSet()` (`viomem.c:894`) does not reset
`currentRunLength` on a clear bit, so after the first hole it reports a "run"
that spans unplugged blocks. With holes totalling H blocks, the removal window
starts at index H instead of at a real run. #1696 already describes the
consequence for `requested_size = 0`. Here it also shifts every partial removal
window, contributing to issue 3, and it leaves the holes that trigger issue 1.

**Suggested fix.** As proposed in #1696: set `currentRunLength = 0` after
recording a completed run in the clear-bit branch.

## 3. Removal searches one low window per notification, so gradual release returns almost nothing

**Summary.** `VirtioMemRemovePhysicalMemory()` builds one window: the start of
the longest plugged run, `plugged_size − requested_size` bytes long. It then
calls `MmAllocateNodePagesForMdlEx(... MM_ALLOCATE_REQUIRE_CONTIGUOUS_CHUNKS |
MM_ALLOCATE_AND_HOT_REMOVE)` once over that window (`viomem.c:1207`). The window
always starts at the lowest plugged address, which is the oldest hot-added
memory and the most likely to hold pages Windows cannot move. The worker makes
one attempt per wake-up (config change or virtqueue interrupt,
`Device.c:476`). If Windows yields nothing, no request is sent, no interrupt
follows and the driver waits for the next config change.

**Observed.** After the guest used and freed 3 GiB, releases from a 4.25 GiB
device returned nothing for requests of 64 MiB, 128 MiB, 256 MiB, 512 MiB and
1 GiB; 2 GiB returned 900 MiB and 4 GiB returned 2.83 GiB. About the lowest
1.1 GiB of plugged memory held unmovable pages, so any window smaller than that
returned nothing. Earlier, two 64 MiB requests returned one 2 MiB block each. Re-issuing the
same `requested-size` six times, and moving it by one block four times (each
a QEMU config notification), returned nothing more. A 1 GiB window returned
90 MiB, and a 2.2 GiB window returned 1.17 GiB within 3 seconds, from the same
guest state. So the memory is removable; the window confines the search.

**Expected.** A request to unplug N bytes removes up to N bytes from wherever
Windows can release whole blocks.

**Suggested fix.** Iterate over plugged blocks or runs, preferably from the
highest address down, attempting block-sized or modest multi-block chunks
until the difference is satisfied or every candidate has been tried once.
Removing from the top also keeps holes from forming below the highest plugged
block, which avoids issue 1.

## Workarounds in this project

Until these are fixed, the
[host controller and guest policy](development.md#request-contract) work
around them:

- Windows requests reclaim in steps larger than the unmovable low zone
  (3 GiB on `win11_gpu`), not small steps.
- When a grow settles with nothing plugged, the host pauses the VM briefly and
  widens the usable region to the device ceiling (requested = ceiling, then
  back to current), once per VM reset. Never resuming with the ceiling
  requested is essential: the driver would plug all of it.
- The host settles a shrink or a grow that stops moving for three polls,
  instead of waiting for the full timeout or latching, and reports `partial`.
- A VM reset clears all plugged memory and therefore all holes.
