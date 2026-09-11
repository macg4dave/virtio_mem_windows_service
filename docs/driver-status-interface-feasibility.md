# Windows Driver Status Interface Feasibility

## Decision

Implementation is deferred. The host already has authoritative allocation state
from alias-scoped live libvirt `current`, and current qualification does not
justify owning a Windows kernel-driver fork solely for diagnostics.

## Potential value

A read-only cached status interface could distinguish notification receipt,
branch selection, candidate block counts, partial progress, device response,
and last failure. That information would be advisory diagnosis only. It must
never authorize a resize or replace host allocation state.

## Minimum design conditions

Any future proposal must provide:

- a versioned fixed-width ABI with explicit size negotiation;
- cached reads with no allocation, blocking device transaction, or state
  mutation in the query path;
- administrator and SYSTEM access only;
- hostile-input, concurrency, lifecycle, and compatibility tests;
- externally owned build, signing, installation, provenance, and rollback;
- a disposable-guest trial before the trusted development target;
- an operational question that existing host/Windows evidence cannot answer.

Until all conditions are met, qualification reports constrained or ambiguous
platform behavior from the existing evidence layers and leaves the driver
unchanged.
