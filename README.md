# OLS CAD — Open Line System Computer Assisted Design

**OLS CAD** is an open-source, modular engineering platform for the design and analysis of overhead power lines — terrain modelling, conductor sag-tension, structure loading, clearance checking and drafting — in the spirit of the commercial tools used across the utility industry, but open, transparent and extensible.

> Status: **Phase 1 — Calculation core** (pre-alpha). Sag-tension and finite-element wire analysis work from the command line (`ols sagtension`). See [ROADMAP.md](ROADMAP.md).

## Why

- **Faster** — native Rust core, multithreaded, GPU rendering with render distance & level-of-detail streaming. Explicit performance targets.
- **More user-friendly** — direct manipulation, live results, explainable numbers, unlimited undo.
- **Collaborative** — open a project as a shared server session; several engineers work on the same line at the same time.
- **Transparency** — every engineering calculation is open, documented, and traceable to its reference (standard, paper, textbook).
- **Modularity** — each domain (cables, sag-tension, loads, structures, clearances, terrain, drafting…) is an independent package with a stable interface, so it can be improved, replaced or validated on its own.
- **Extensibility** — anyone can write a plugin: a new design code, a conductor model, a report, an import format, a custom check — without forking the core.
- **Validation first** — a public benchmark suite compares results against hand calculations, published examples (CIGRE, IEEE, IEC) and reference software.

## Quick start

```bash
cargo test --workspace
cargo run -p ols-cli -- info
cargo run -p ols-cli -- sagtension examples/sagtension/drake-section.toml
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup.

## Project layout

```
OLS-CAD/
├── docs/              Documentation book (mdBook): design, ADRs, theory manual, dev guide
├── crates/            One Rust crate per module (see docs/ARCHITECTURE.md)
│   ├── ols-core/      Data model, units, time budgets (exit clauses)
│   ├── ols-validation/ Validation framework (`ols validate`)
│   ├── ols-plugins/   Plugin manifests + sandboxed WebAssembly host (WIT interface)
│   ├── ols-loads/     Load cases and structure loads
│   ├── ols-server/    Server mode: a project hosted for several users (QUIC)
│   ├── ols-py/        Python bindings (`import ols`)
│   ├── ols-cables/    Conductor library
│   ├── ols-weather/   Weather cases and wire loads
│   ├── ols-wireloads/ Point and distributed loads on wires
│   ├── ols-sagtension/ Catenary, change of state, sections, criteria
│   ├── ols-wirefe/    Finite-element wire systems (SAPS-style)
│   ├── ols-structures-fe/ Structure analysis: towers, frames, guys, poles
│   └── ols-cli/       `ols` command-line tool
├── plugins/           Plugins: examples/ (WASM + Python), tests/ (runaway plugin)
├── benchmarks/        Validation cases with reference results
└── examples/          Sample projects
```

## Documents

- [ROADMAP.md](ROADMAP.md) — phases, milestones and scope
- [docs/DESIGN_GOALS.md](docs/DESIGN_GOALS.md) — performance targets, UX, wire loads, multi-user server, LOD, insulator strings, version control, exports
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — modules, data model, plugin system
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to contribute, clean-room rules
- [GOVERNANCE.md](GOVERNANCE.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [docs/adr/](docs/adr/) — architecture decision records

## License

[Mozilla Public License 2.0](LICENSE). Plugins distributed as separate files may use any license.

## Disclaimer

OLS CAD is an independent project and is not affiliated with, endorsed by, or derived from any commercial software. All product names are trademarks of their respective owners. Engineering results must always be verified by a qualified engineer.
