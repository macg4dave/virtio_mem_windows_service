---
name: rust-security
description: Review or harden Rust trust boundaries
---

Read the architecture, threat-relevant contract, owning code, and tests.

Task: `<security review or hardening goal>`

Review untrusted input, arithmetic and allocation bounds, denial of service,
unsafe code, secrets, privilege, target identity, replay/freshness, filesystem
ownership, and host/guest authority boundaries. Fix findings with explicit
validation and focused regressions; preserve least privilege and fail closed.
