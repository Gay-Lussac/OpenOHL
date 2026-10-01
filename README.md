# OLS CAD — Open Line System Computer Assisted Design

**OLS CAD** is an open-source, modular engineering platform for the design and analysis of overhead power lines — terrain modelling, conductor sag-tension, structure loading, clearance checking and drafting — in the spirit of the commercial tools used across the utility industry, but open, transparent and extensible.

> Status: **Phase 0 — Foundations** (pre-alpha, nothing usable yet). See [ROADMAP.md](ROADMAP.md).

## Why

- **Faster** — native Rust core, multithreaded, GPU rendering with render distance & level-of-detail streaming. Explicit performance targets.
- **More user-friendly** — direct manipulation, live results, explainable numbers, unlimited undo.
- **Collaborative** — open a project as a shared server session; several engineers work on the same line at the same time.
- **Transparency** — every engineering calculation is open, documented, and traceable to its reference (standard, paper, textbook).
- **Modularity** — each domain (cables, sag-tension, loads, structures, clearances, terrain, drafting…) is an independent package with a stable interface, so it can be improved, replaced or validated on its own.
- **Extensibility** — anyone can write a plugin: a new design code, a conductor model, a report, an import format, a custom check — without forking the core.
- **Validation first** — a public benchmark suite compares results against hand calculations, published examples (CIGRE, IEEE, IEC) and reference software.

## Project layout (planned)

```
OLS-CAD/
├── docs/              Architecture, ADRs, theory manual, validation reports
├── crates/            One Rust crate per module (see docs/ARCHITECTURE.md)
├── plugins/           First-party plugins (design codes, formats, reports)
├── benchmarks/        Validation cases with reference results
└── examples/          Sample projects
```

## Documents

- [ROADMAP.md](ROADMAP.md) — phases, milestones and scope
- [docs/DESIGN_GOALS.md](docs/DESIGN_GOALS.md) — performance targets, UX, wire loads, multi-user server, LOD
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — modules, data model, plugin system
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to contribute, clean-room rules
- [docs/adr/](docs/adr/) — architecture decision records

## Disclaimer

OLS CAD is an independent project and is not affiliated with, endorsed by, or derived from any commercial software. All product names are trademarks of their respective owners. Engineering results must always be verified by a qualified engineer.
