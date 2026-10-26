# Repository instructions

Read `PLAN.md` first, then these rules before writing or reviewing Rust,
build tooling, or dependencies. `PLAN.md` is the only project document and
owns purpose, evidence, phases, and acceptance criteria. Work on the current
phase unless the user explicitly changes scope. Apply these standards to new
and changed code; existing violations are cleanup debt, not precedent or a
reason for an unrelated rewrite.

## Architecture and memory contracts

- Windows measures and publishes memory telemetry. The host owns policy,
  target selection, and `virsh` resizing. Windows never resizes.
- Core owns shared contracts, parsing, and pure policy. Keep platform effects
  in the host and Windows crates; keep presentation out of policy.
- Alias-selected live XML owns device allocation: `requested` is intent and
  `current` is observed allocation. Command success does not prove convergence.
- The host owns accepted-sample identity and pending-command state. Derive
  status from that state; previews use the selected policy without writing it.
- Memory quantities are `u64` bytes. Distinguish whole-guest RAM, fixed base
  RAM, and device allocation. Use checked arithmetic and existing exact unit
  conversions; align targets and enforce device/policy bounds. Saturation is
  appropriate only for an intentional, documented floor or ceiling.
- Validate configuration and telemetry at their boundaries. Preserve schema,
  identity, freshness, replay, and ordering checks. Re-read changing live
  state immediately before an effect; this is a race check, not duplicate work.

## Design, ownership, and APIs

- Rust only. Prefer stable, idiomatic Rust and straightforward control flow.
- Find existing implementations and callers before adding code. Reuse,
  improve, combine, or delete first. Each responsibility has one authority.
- Add no module, trait, mode, setting, helper, or dependency without a current
  need. Similar syntax alone does not justify a shared abstraction.
- Borrow by default (`&str`, `&[T]`, `&T`). Reconsider ownership before cloning
  to satisfy the borrow checker. Use `Arc::clone`/`Rc::clone` explicitly.
- Keep state owned by the runtime or service. No global mutable application
  state or `OnceLock` cache as a second authority. Use `Arc`, atomics, locks,
  and channels only for actual thread, signal, or SCM callback requirements;
  do not replace necessary synchronisation with single-threaded `Rc<RefCell<_>>`.
- Use enums for closed sets such as operation direction, control state, and
  telemetry source. Prefer exhaustive matches on project enums. Use standard
  types such as `Duration` and `Path`; add identifier newtypes only when they
  prevent a demonstrated mix-up, not as speculative wrappers.
- Use iterators where clearer than manual index loops. Prefer `.get()` for
  untrusted indices; direct indexing requires an established bound.
- Keep functions and files cohesive. Split by responsibility when it helps;
  do not fragment code or bundle unrelated arguments merely to meet an
  arbitrary line/argument limit. This project has no configured numeric caps.
- Keep items private by default, then `pub(crate)` where needed. Public APIs
  in library crates need real callers or a required external contract. Check
  consumers before removal. Derive traits only when useful; avoid gratuitous
  `Clone` or `Default` on state with invariants.
- Remove superseded paths, exports, settings, and dependencies in the same
  change. Separate equivalent refactoring from deliberate behavior/schema
  changes; preserve required external contracts and recovery behavior.

## Errors, unsafe code, and diagnostics

- Use `Result` and preserve causes and operation context across boundaries.
  Reuse existing error enums and `thiserror`; do not add another error
  framework or a generic error layer without a concrete need.
- Do not introduce `unwrap()`, `expect()`, `panic!`, `todo!`, `unimplemented!`,
  or `dbg!` outside tests. Propagate failures or handle them explicitly.
- Do not silently discard errors with `let _ = ...` or `.ok()`. If ignoring
  an error is correct, explain why at that site and retain the primary failure.
  Never report incomplete work as successful or reset unresolved command state.
- Distinguish rejection before execution from an unknown command outcome.
  Do not retry an uncertain resize or clear a safety latch to recover silently.
- `unsafe` is justified only at required platform/FFI boundaries, including
  Win32 and host OS calls. Keep it scoped; add `SAFETY` comments covering
  pointer validity, lifetime, callbacks, and handle ownership/cleanup. Keep
  policy and parsing safe.
- Keep `main` consistent with existing CLI/SCM exit behavior. Log actionable
  failures through host stderr/journal or Windows Event Log. Format operational
  wording at platform/CLI boundaries. Use British English for new prose and
  preserve API/protocol spelling and stable machine-readable fields.
- Comments explain contracts and non-obvious decisions, not Rust syntax.
  Add module/API docs where the responsibility or safety contract needs them;
  remove stale comments rather than copying obsolete behavior into new docs.

## Lints and dependencies

- The existing local gate runs formatting checks and Clippy with `-D warnings`.
  The manifests do not currently configure pedantic/restriction lint groups.
  Do not claim those groups, numeric caps, or an unsafe ban are enforced.
- Do not add `#[allow(...)]` or broad lint suppressions. For a genuinely
  inapplicable lint, use `#[expect(clippy::lint_name, reason = "...")]` on the
  narrowest item, never a module/crate. Explain correctness, not convenience.
- Fix the cause of a warning. Do not add abstractions to silence a lint;
  review existing suppressions when changing the affected code. Changing
  manifest lint policy is a separate, explicit task.
- Reuse suitable existing dependencies or a few clear lines of `std` code.
  A new crate must solve a current problem: justify maintenance, features,
  platform support, and what code it replaces. Disable unneeded features and
  remove replaced code/dependencies in that change.

## Tests and validation

- The user's requested behavior is the source of truth; tests are evidence.
  Add focused adjacent unit tests for changed target math, parsing, telemetry
  acceptance, control state, and failure behavior. A testable bug fix needs a
  regression test that would fail without the fix.
- Test behavior, not constants or implementation shape. Use fake clocks,
  observations, and injected adapters for policy/recovery tests; controlled
  interleavings for concurrency and temporary files for publication/persistence.
- When a test fails, trace the whole workflow before deciding whether code or
  test is wrong. Update/delete obsolete or implementation-bound tests with a
  stated reason. Never weaken, delete, or ignore a test just to pass the gate,
  or retain poor design and special cases solely to satisfy a test.
- Respect analysis-only tasks. For authorized code changes, run focused
  checks, then `cargo test --workspace --exclude virtio-mem-service --locked`
  on Linux. The required packages are `virtio-mem-core`, `virtio-mem-host`,
  and `virtio-mem-xtask` while it remains. Use direct Cargo commands if xtask
  is broken; record failures honestly. Do not run builds/tests for doc-only work.
- Windows code requires a Windows target. Check/build the actual selected
  target; native SCM, Win32 sampling, ACLs, and atomic replacement need Windows
  execution. Cross-compilation and mocked tests do not establish live behavior.
- Unit tests must not mutate real VMs, remote services, or host configuration.
  Integration/workload tests need explicit authorization and bounded resources.
  Report what ran and what did not. "Works" requires actual VM evidence.

## Live VMs

- Before any live resize, state VM, device alias, target with explicit units,
  baseline allocation, and rollback or recovery procedure. Check host reserve
  before growth. Wait for live `current` to converge; never stack resizes.
- On a stall or uncertain outcome, stop actuation and report it. Do not assume
  that reversing a target will be accepted or recover an unconverged operation.
- Ask before reboots, VM definition changes, or irreversible actions. A code
  or documentation task does not authorize live tests.

## Review and consolidation

Trace entry points through policy, state, persistence, effects, and results.
Compare implementations by semantics and callers. Prioritize correctness,
security, concurrency, failure handling, duplicated authority, and complexity.
Report findings with file/line evidence, trigger, consequence, and callers;
separate verified facts from assumptions.

For each contained consolidation, identify contracts/failures; choose KEEP,
IMPROVE, COMBINE, DELETE, or SEPARATE; select the canonical implementation;
update callers; remove superseded paths; verify behavior and search for
remaining duplication. Do not build a framework for code scheduled for deletion.

## Documentation

Update `PLAN.md` when evidence, state, or the plan changes. Do not create
additional project documents, task boards, roadmaps, or prompt collections.
`AGENTS.md` points here; do not duplicate these rules in tool-specific files.
Preserve unrelated worktree changes.
