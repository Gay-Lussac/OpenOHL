# ADR-0011: Licence — AGPL-3.0-or-later, plugin interface MPL-2.0

- **Status:** Accepted
- **Date:** 2026-10-20
- **Supersedes:** [ADR-0001](0001-license.md)

## Context
ADR-0001 chose MPL-2.0 for the whole repository: a file-level copyleft
in which changes to OpenOHL's own files are shared, but OpenOHL may be
combined with closed code. The owner wants forks to stay open under the
same licence, so that the program cannot be taken over into a closed
product, while utilities and consultancies keep the freedom to write
closed plugins for their own design codes and data.

OpenOHL also runs as a server (`ohl serve`, web clients): a licence that
only covers distribution would let a modified version be offered as a
hosted service without its changes being shared.

## Options considered
1. **Keep MPL-2.0** — file-level copyleft; a closed product may embed it.
2. **GPL-3.0-or-later** — strong copyleft for distributed versions; a
   modified version run only as a service need not be shared.
3. **AGPL-3.0-or-later** — as GPL-3.0, and a modified version offered to
   users over a network must offer them its source too.

For plugins: under a strong copyleft licence a plugin built against the
interface could count as part of the program. Keeping the interface and
the SDK under a weaker licence leaves plugins free to choose theirs.

## Decision
- **AGPL-3.0-or-later** for OpenOHL: the applications, the command line,
  the engine crates, the server, the Python module, the documentation's
  code and the examples (`LICENSE`).
- **MPL-2.0** stays for the **plugin interface** — the WIT files in
  `crates/ohl-plugins/wit/` — and for everything under `plugins/` (the
  shared crate `ohl-code-common`, the design-code plugins, the examples
  and test plugins) (`LICENSE-MPL-2.0`). MPL-2.0 is compatible with the
  AGPL (its secondary-licence clause), and a plugin built on these files
  may keep any licence, closed ones included.
- Every source file names its licence in its header
  (`SPDX-License-Identifier: AGPL-3.0-or-later`, or the MPL-2.0 notice in
  the plugin interface and plugins).
- The owner holds the copyright of the code written so far, so the code
  could be relicensed; contributions from now on are accepted under these
  licences.

## Consequences
- A fork, a modified version or a product built on OpenOHL must be
  published under the AGPL, also when it is only offered as a service.
- Plugins (separate WebAssembly or Python modules using the interface)
  may be closed; the owner's private plugins are not affected either way
  as they are not distributed.
- The dependencies (MIT, Apache-2.0, MPL-2.0 and similar) are compatible
  with AGPL-3.0.
