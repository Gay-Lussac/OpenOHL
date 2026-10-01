# ADR-0003: Plugin mechanism

- **Status:** Proposed
- **Date:** 2026-10-01

## Context
Anyone must be able to add calculations, design codes, checks, formats, reports and GUI tools without forking. Plugins may run on a shared server (ADR-0005), so they must be safe.

## Options considered
1. Native dynamic libraries (`.dll`/`.so`) — Rust has no stable ABI; plugins would break on each compiler/version change; no sandbox.
2. Python only — easy but slow in hot paths, no sandbox.
3. **WASM Component Model + Python + in-tree Rust.**

## Decision (proposed)
| Tier | Use | Notes |
|---|---|---|
| WASM components | Third-party calculations, design codes, checks, solvers, formats | **Stable public ABI** defined in versioned WIT packages (e.g. `ols:cable-model@1.0`); any language (Rust, C/C++, Zig, Go…); sandboxed |
| Python (PyO3) | Scripts, reports, importers/exporters, automation | Not intended for hot loops |
| Native Rust | First-party engines inside this repository | Compiled in; no dynamic loading |

- Manifest `ols-plugin.toml`: name, version, target SDK version, extension points, requested permissions (file system, network). Deny by default.
- Extension points: `cable_models`, `wire_loads`, `insulator_assemblies`, `design_codes`, `importers`, `exporters`, `checks`, `reports`, `solvers`, `commands`, `gui_tools`.
- Traceability: the project records the plugin name + version that produced each result.
- Server mode: plugins run on the server; the server advertises the required plugin set to joining clients; only admins install plugins on a server.

## Consequences
- WIT interfaces become a public contract: semantic versioning and compatibility tests required.
- Slight call overhead for WASM plugins vs native; acceptable, measured in benchmarks.
