# OLS CAD — Open Line System Computer Assisted Design

**OLS CAD** is an open-source, modular engineering platform for the design and analysis of overhead power lines — terrain modelling, conductor sag-tension, structure loading, clearance checking and drafting — in the spirit of the commercial tools used across the utility industry, but open, transparent and extensible.

> Status: **pre-alpha.** Phases 0–3 of the [roadmap](ROADMAP.md) are delivered (calculation core, line model and terrain, desktop application with server mode); Phases 4 and 5 (loading and structure checks, advanced modules) are delivered as a base. Engineering results must be verified by a qualified engineer.

## What it does

- **Sag-tension**: exact catenary, change of state, tension sections (ruling span and exact equal-H), non-linear stress-strain and creep, tension criteria, point and distributed loads.
- **Finite-element wire systems**: exact catenary elements, suspension, V- and strain strings, bundles, jumpers, 3D blowout, broken spans; elements that follow the stress-strain curves.
- **Line model**: routes, structure and insulator libraries, sections, jumpers, electrical systems; incremental re-solving.
- **Terrain and LiDAR**: survey points and TINs, LAS/LAZ and COPC, point clouds stored in the project with levels of detail, ground classification, vegetation encroachment, as-built wire fitting.
- **Clearances**: vertical and radial, to the ground, LiDAR classes, other lines and wires of the same line, by voltage; envelopes over weather cases; galloping ellipses and blowout against the right of way.
- **Loads and structures**: load cases, design-code plugins (EN 50341, LeV, IEC 60826, NESC, CSA C22.3, ASCE MOP 74, GO 95; parameters are your data), structure loads, capacity checks (allowable spans, load trees), frame/truss finite elements and pole checks.
- **More engineering**: thermal rating (IEEE 738, CIGRE TB 601 / 207, clearance-limited temperature), optimum structure spotting, bill of materials and cost estimate.
- **Drawings and exchange**: plan and profile sheets (SVG, DXF, PDF, PNG), profile exports; OLS XML, LandXML, GeoJSON, Shapefile, GeoPackage, IFC, KML/KMZ, CSV, XLSX; file-format and project plugins.
- **Projects and collaboration**: a SQLite project file with an operation log and per-user undo, git-like version control (branches, merges, reviews, edit grants), server mode for several users, a browser client.
- **Desktop application** (`ols-cad`): 3D, plan and profile views, editing tools, tables, library editors, Python console, plugin manager.

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
cargo run --release -p ols-app -- examples/synthetic-line/synthetic-line.ols.json \
    --terrain examples/synthetic-line/synthetic-survey.csv \
    --weather examples/synthetic-line/weather-cases.olswc
```

The [documentation book](docs/introduction.md) has the user guide and the theory manual: on macOS double-click `Open the manual.command` (it installs mdBook the first time), or run `mdbook serve --open`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup.

## Project layout

```
OLS-CAD/
├── docs/              Documentation book (mdBook): user guide, theory manual, design, ADRs, development
├── crates/            One Rust crate per module (see docs/ARCHITECTURE.md)
│   ├── ols-core/          Units, geometry, project grid, time budgets (exit clauses)
│   ├── ols-cables/        Cable library (.olsw), stress-strain and creep models
│   ├── ols-weather/       Weather cases (.olswc) and the loads they put on wires
│   ├── ols-wireloads/     Point and distributed loads on wires
│   ├── ols-sagtension/    Catenary, change of state, sections, criteria, non-linear cables
│   ├── ols-wirefe/        Finite-element wire systems (SAPS-style)
│   ├── ols-line/          Line model (.ols.json), 3D solving of sections and jumpers
│   ├── ols-structures/    Structure type (.olss) and insulator (.olsi) libraries
│   ├── ols-structures-fe/ Structure analysis: frames, trusses, guys (.olsfe), poles (.olsp)
│   ├── ols-loads/         Load cases (.olslc), structure loads, capacity checks
│   ├── ols-terrain/       Survey points, TIN, ground heights and profiles
│   ├── ols-lidar/         LAS/LAZ reading, class schemes (.olsc), ground classification
│   ├── ols-pointcloud/    Point clouds in the project file, levels of detail, COPC
│   ├── ols-clearance/     Clearance rules (.olsr), envelopes, galloping, blowout, vegetation, wire fits
│   ├── ols-thermal/       Thermal rating (IEEE 738, CIGRE), ambient cases (.olsta)
│   ├── ols-spotting/      Optimum structure spotting
│   ├── ols-bom/           Bill of materials and cost estimate (.olscost)
│   ├── ols-drawing/       Plan and profile drawings (SVG, DXF, PDF, PNG) and profile exports
│   ├── ols-crs/           Coordinate systems (EPSG, proj4)
│   ├── ols-io/            Import/export formats and their registry
│   ├── ols-session/       Project file (.ols), commands, operation log, undo, versions, grants
│   ├── ols-server/        Server mode (QUIC) and the web client
│   ├── ols-render/        wgpu renderer, levels of detail
│   ├── ols-plugins/       Plugin manifests and the WebAssembly host (WIT interfaces)
│   ├── ols-validation/    Validation framework (`ols validate`)
│   ├── ols-app/           Desktop application `ols-cad` (egui)
│   ├── ols-cli/           `ols` command-line tool
│   └── ols-py/            Python bindings (`import ols`)
├── plugins/           Plugins: codes/ (design codes), examples/ (WASM + Python), tests/
├── libraries/         Shipped libraries: cables (.olsw), LiDAR class schemes (.olsc)
├── benchmarks/        Validation cases with reference results
└── examples/          Sample lines, jobs and input files
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
