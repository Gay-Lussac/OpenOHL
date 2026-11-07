# Frozen plugin interfaces

A copy of each released plugin interface, never edited (ADR-0012). The
test `crates/ohl-plugins/tests/compat.rs` compares the live interface
(`../wit/`) with every copy here: each of its interfaces must keep every
type and function exactly as it was, each of its worlds every import and
exactly the exports it had. A change that breaks this is a new major
version: add the new interface as a new copy, keep the host loading
the old one for as long as ADR-0012 says.

- `0.3/` — interface version 0.3 (`sdk = "0.3"`, WIT package
  `ohl:plugin@0.1.0`), frozen on 2026-11-07 with every addition made
  to it until then.
