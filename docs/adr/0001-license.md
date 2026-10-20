# ADR-0001: License

- **Status:** Superseded by [ADR-0011](0011-license-agpl.md) (2026-10-20: AGPL-3.0-or-later, the plugin interface MPL-2.0)
- **Date:** 2026-10-01

## Context
OLS CAD wants (a) the core to stay open and improvements to flow back, and (b) utilities and consultancies to be able to write in-house or commercial plugins without being forced to publish them.

## Options considered
1. **Apache-2.0** — permissive, patent grant, maximum industry adoption; improvements to the core may stay closed.
2. **MPL-2.0** — file-level copyleft: changes to OLS CAD files must be shared, but plugins/separate files can have any license. Patent grant.
3. **LGPL-3.0** — library copyleft; awkward with Python packaging and static linking of Rust kernels.
4. **GPL-3.0** — strong copyleft; would likely deter proprietary plugins and utility adoption.

## Recommendation
**MPL-2.0** for the core packages (improvements to the core come back, plugins stay free to choose), optionally Apache-2.0 for `ols-sdk` and plugin templates.

## Decision
**MPL-2.0** for the whole repository (accepted 2026-10-01). The core stays open — modifications to OLS CAD files must be shared — while plugins in separate files may use any license, including proprietary. Revisiting Apache-2.0 for `ols-sdk`/plugin templates remains possible later.
