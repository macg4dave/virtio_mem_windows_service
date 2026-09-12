# Engineering standards

## Language and tooling

- Product and maintained tooling use Rust 2021.
- Bash is limited to a generated or task-specific reviewed privilege boundary
  with `set -euo pipefail`.
- Format with rustfmt, lint with warnings denied, and keep dependencies locked.
- Maintained orchestration, parsing, policy, polling, evidence, cleanup, and
  rollback belong in `tools/xtask` or the owning product crate.

## Rust

- Prefer safe Rust, explicit `Result`/`Option`, structured errors, small
  cohesive functions, and injected external effects.
- Avoid panics, global mutable state, and `unsafe`; justify and test an
  unavoidable exception.
- Keep public items minimal and document units, invariants, errors, and safety
  assumptions.
- Review license, maintenance, features, target support, and lockfile impact
  before adding a dependency.
- Add focused regressions for fixes and cover malformed, boundary, overflow,
  cancellation, and convergence behavior where relevant.

## Configuration

- Do not embed operational VM names, service identities, paths, RAM bounds,
  resize deltas, workload sizes, durations, sleeps, retry counts, sampling
  intervals, or timeouts in prompts, scripts, examples, or workflow defaults.
- Require them through validated configuration or explicit workflow arguments,
  or derive them from fresh system state when a trustworthy source exists.
- Keep format/schema/resource limits in their owning contract. Keep normative
  product safety constants in `docs/target-controller.md`; other docs link to
  that contract instead of copying values.

## Windows service

- SCM callbacks remain bounded and delegate work to the stoppable runtime.
- Report lifecycle transitions accurately and preserve unexpected worker
  failure as a non-zero result.
- Use one wakeable cancellation path and the configured shutdown bound.
- Require versioned configuration for VM/service identity, paths, polling,
  adapter operations, shutdown, and least-privilege account.
- Collect production demand through native Windows APIs. Do not open QGA or
  receive host allocation state.
- Prefer Microsoft-supported memory-manager notifications, documented APIs,
  and documented performance counters over a project-specific pressure model.
  Normalise units, provenance, sampling readiness, and availability; do not
  turn the Windows collector into a resize authority.
- Use ETW/WPR as qualification or diagnostic evidence unless an explicit cost,
  stability, and support review approves an always-on provider contract.

## Host controller

- One explicitly configured controller manages one VM and device alias until
  global arbitration is implemented.
- Use fixed argument vectors and configured finite command bounds; never invoke
  a shell for `virsh`.
- Refresh live XML and compatibility immediately before mutation. Never issue
  an ordinary request while requested and current differ.
- Preserve telemetry provenance, freshness, replay, headroom, alignment,
  retention-floor, intent-journal, and latch gates.
- Automatic reclaim remains default-on with an explicit pause override.
  Detailed target, quantum, history, hysteresis, retry, and recovery behavior
  is normative only in `target-controller.md` and the owning Rust modules.
- Keep pressure classification separate from byte-target construction. Missing
  required pressure evidence blocks shrink, and fixed headroom is a fallback
  safety input rather than the primary release demand model.
- A service failure is fail-stop in the checked-in unit. Deployment monitoring
  owns any reviewed restart policy.

## Documentation and validation

- Update contracts and boards with behavior changes in the same change.
- `BACKLOG.md` owns product tasks; `build-test-tooling-roadmap.md` owns tooling
  tasks.
- Run focused Cargo tests, then `cargo xtask gate local`, then applicable
  native or higher-level workflows. Report every layer separately.
- Keep documentation current and procedural; do not preserve obsolete command
  generations or transient validation values as instructions.
