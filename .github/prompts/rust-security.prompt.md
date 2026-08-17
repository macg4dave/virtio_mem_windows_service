---
name: rust-security
description: Review and harden Rust parsing, validation, and service boundaries
---

Read `docs/architecture.md`, `docs/api-contract.md`, `docs/testing.md`, and `BACKLOG.md` first.

Task:
"""
<security review or hardening goal>
"""

Review for:

- malformed or untrusted QEMU Guest Agent responses;
- integer overflow, underflow, alignment, range, and threshold errors;
- panics, unchecked assumptions, denial-of-service inputs, and information leaks;
- unsafe code, dependency risk, and accidental privilege or host-boundary violations;
- broad discovery, implicit network access, or remote administrative actions.

Rules:

1. Prefer safe Rust and explicit validation with actionable typed errors.
2. Do not add `unsafe` unless unavoidable, documented, narrowly scoped, and tested.
3. Preserve least privilege and the documented Windows-service/host-automation boundary.
4. Add regression tests for every finding fixed, including malformed and boundary inputs.
5. Do not commit secrets, credentials, tokens, private keys, or production data.
6. Keep the fix minimal and update relevant contracts/docs and `BACKLOG.md`.

Validate from `windows/` with tests, format check, Clippy, and release build when practical. Report findings and exact validation results.
