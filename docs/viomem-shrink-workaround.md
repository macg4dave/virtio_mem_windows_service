# Viomem shrink workaround

The host controller contains a temporary workaround for a bug in the
unpatched Windows `viomem.sys` driver. Without it, a shrink request returns
very little memory to the host and then stops. This page explains what the
driver does wrong, how the workaround gets memory moving again, and why that
works.

The code lives in [host/src/runtime.rs](../host/src/runtime.rs), between the
`VIOMEM WORKAROUND START` and `VIOMEM WORKAROUND END` markers (`extend_shrink`),
plus a marked branch in `observe_pending`. The operator-facing summary is in
[the deployment guide](../deploy/config_files/README.md).

## The driver bug

A virtio-mem shrink works like this: the host lowers the device's
`requested` size, the device sends the guest a configuration notification,
and the guest driver unplugs blocks until `current` (plugged memory) equals
`requested`.

The unpatched Windows driver does not finish that job. On each
configuration notification it:

1. picks one plugged run of blocks (the longest one),
2. caps the removal window at `plugged - requested`,
3. tries once to have Windows evacuate and release that window,
4. stops. It does not move on to other runs or retry until `current`
   reaches `requested`.

Whatever part of the window Windows cannot evacuate (pages in use,
non-movable allocations) stays plugged. Nothing new happens until the next
notification, and since `requested` is already at the target, the host has
no reason to send one. The shrink stalls with `current` stuck above
`requested`.

## What we saw on win11_gpu

These observations, made with 8 MiB blocks, shaped the workaround:

- **Restarting the controller released one block every time.** Running
  `install-host.sh` restarted the controller, which sent a new, lower
  `requested` value. That new value caused one notification, and the driver
  freed about one block (8 MiB). This is where the "8 MB per install"
  pattern came from.
- **Each new low frees about one block.** Lowering `requested` by one block
  freed about the block that was just added to the window. The gap of
  unmovable memory below `current` stayed the same size.
- **Re-sending an old target frees nothing.** An earlier version bounced
  `requested` up one block and straight back down, which gives two
  notifications. That released nothing, so it was removed. The driver only
  makes progress when the target is a value it has not tried yet.
- **Bigger steps help only a little.** Steps of 16 and 32 MiB were tried.
  After a 32 MiB step, `current` often dropped only 8 MiB. The driver mostly
  frees about one block per new low, however far the target drops.

## How the workaround works

The workaround runs only while the controller owns a pending shrink that
has stalled, meaning `current` has not moved for one poll interval
(`command_stalled`). On each stalled poll it calls `extend_shrink`, which
decides between three actions.

### 1. Stop when the goal is reached

The **goal** is the allocation the guest originally asked for: the
allocation when the shrink started (`prior_current_bytes`), minus the
admitted Remove request rounded down to whole blocks. It is never below the
**floor**, which is `min_ram` converted to a device target (minus the guest's
base RAM, rounded up to a block).

If `current` is at or below the goal, the guest has what it asked for. The
controller logs `event=shrink_extension_done` and settles the shrink at
`current`.

### 2. Otherwise, lower the target to a new value

If the goal is not reached, the controller lowers `requested` by one step.
Because this is a value the driver has not tried yet, the device sends a new
notification and the driver makes another removal attempt over a wider
window. This is what gets memory moving again.

The step size adapts:

- The first step of an operation is 64 MiB (one block with 64 MiB blocks).
- If the previous step was fully released, the next step doubles, up to a
  64 MiB maximum.
- If the previous step was only partly released, or not at all, the next
  step drops back to one block.

The step is then clamped so it never:

- goes past the goal (`current - goal`),
- goes below the floor,
- exceeds the guest's spare memory. The whole outstanding removal window,
  including earlier extensions, must fit inside the guest's available memory
  and commit headroom after the built-in reserves.

The controller logs `event=shrink_extended` with the target, step, goal,
floor and what was released since the last step.

### 3. Settle when no lower target is possible

If the target is already at the floor and memory still does not move, no
lower target exists that could trigger another driver attempt. Waiting would
not help. The controller logs
`event=held_shrink_released reason=minimum_target_exhausted` and settles at
`current`. It reports the original request's actual result, so fresh guest
policy can choose what to do next.

## Why it works

The driver does one removal attempt per notification, and only a new target
value causes a useful notification. The workaround supplies exactly that: a
steady series of new, lower targets, one per stalled poll. Each one makes
the driver widen its window and try again, and each try usually frees about
the block it just added.

Stepping down also explains why the target may end up below the goal. The
unmovable gap below `current` does not go away, so to bring `current` down
to the goal the target has to go further down than the goal. The goal check
uses `current`, not `requested`, so the controller stops as soon as the guest
actually has the memory it asked for. It then settles `requested` back up to
`current`.

The block size is the real speed lever. Since the driver frees about one
block per new low, a 64 MiB block returns about eight times as much memory
per step as an 8 MiB block. Bigger steps in the workaround do not give the
same gain. Changing the block size needs a cold VM restart and a new
attestation (see `deploy/config_files/restart-vm-attest.sh`).

## Safety

The workaround never makes the guest give up memory it needs:

- **Native Low memory** from the guest stops the shrink at once and settles
  it (`event=held_shrink_released reason=guest_low_memory`).
- **Missing guest telemetry** also settles it
  (`reason=guest_telemetry_unavailable`).
- **Not enough spare memory** for the whole outstanding window settles it
  (`reason=guest_surplus_exhausted`).
- **Extra steps need native High memory.** When the guest's signal is
  Neutral or Unknown, no new steps are taken, though an existing safe window
  is kept.
- **The floor** (`min_ram`) is never crossed.

Every new target is saved to the durable checkpoint before the command is
sent, like any other resize. If the command is rejected, nothing ran and the
previous target is restored. If the result is unknown, the controller
latches, as it does for any other uncertain command.

## Limits

- Settling is asynchronous, so the target can overshoot the goal by a step
  before the controller sees the new `current`.
- The workaround does not make Windows evacuate memory faster or more
  reliably. Some blocks may never be released. In that case the floor is
  reached and the shrink settles at whatever was freed.

## Removing it

Once a patched viomem driver is deployed, delete the code between the
`VIOMEM WORKAROUND START` and `END` markers in `host/src/runtime.rs`, the
marked branch in `observe_pending`, the `extension` field, and the tests
under `VIOMEM WORKAROUND tests`. A stalled shrink is then handled by the
normal convergence timeout.
