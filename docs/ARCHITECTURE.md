# Architecture

> Draft — subject to the ADRs in [adr/](adr/). Requirements in [DESIGN_GOALS.md](DESIGN_GOALS.md).

## Principles

1. **Fast by construction**: the core engine is native Rust; Python is an optional scripting/plugin layer.
2. **Layered**: data model → engines → session/server → clients. Lower layers never depend on upper ones.
3. **Headless engines**: every calculation runs without a GUI (library + CLI), so it can be scripted, tested and run on servers.
4. **Everything is a command**: all edits go through serialisable commands applied by an authoritative session. This gives undo/redo, history, audit and multi-user collaboration with one code path.
5. **Interfaces, not implementations**: modules communicate through traits / documented interfaces. Implementations can be swapped by plugins.
6. **Open data**: open, versioned project format.
7. **Traceability**: every result can report the models, standards and parameters that produced it.
8. **Display ≠ model**: LOD and streaming only affect what is loaded/rendered, never the engineering data used for calculations.

## Layers

```
┌──────────────────────────────────────────────────────────────────┐
│ Clients     Desktop GUI (native, GPU)  ·  CLI  ·  Python scripts  │
│             · future web client                                   │
├──────────────────────────────────────────────────────────────────┤
│ Session     ols-session: authoritative model, command log,        │
│             undo/redo, change broadcast, permissions, presence    │
│             (in-process for single user, network server for many)│
├──────────────────────────────────────────────────────────────────┤
│ Engines     sagtension · wirefe · loads · clearance · structures-fe│
│             rating · spotting · drafting · reports                │
├──────────────────────────────────────────────────────────────────┤
│ Domain      cables · terrain · structures · weather · io          │
├──────────────────────────────────────────────────────────────────┤
│ Core        ols-core: data model, units, geometry, CRS, spatial   │
│             chunk index, plugin registry, change events           │
└──────────────────────────────────────────────────────────────────┘
```

## Modules (Rust crates unless noted)

| Crate | Responsibility |
|---|---|
| `ols-core` | Data model, units, geometry, CRS, spatial chunk index, plugin registry, schema & migrations |
| `ols-session` | Project file (`.ols`), commands, operation log, per-user undo/redo, admin rollback, roles, dirty tracking; later sync protocol, server & client |
| `ols-pointcloud` | Point clouds in the project file (chunk × class blocks), point-source interface |
| `ols-lidar` | LAS/LAZ reading, class schemes (`.olsc`), corridors |
| `ols-crs` | Coordinate systems (EPSG, proj4), transformations of points and lines |
| `ols-io` | Import/export: format traits and registry, built-in formats (CSV, KML/KMZ), formats from WebAssembly plugins |
| `ols-render` | wgpu renderer: lines, points, surfaces in an offscreen texture; orbit camera; headless images |
| `ols-app` | Desktop application `ols-cad` (egui, ADR-0009): 3D, plan, profile, properties |
| `ols-cables` | Cable/conductor library, stress-strain & creep models |
| `ols-weather` | Weather cases, load case definitions (code-agnostic) |
| `ols-wireloads` | Additional loads on wires: point loads (N) and distributed loads (N/m), permanent or per load case |
| `ols-sagtension` | Catenary (incl. segmented catenary with point loads), change of state, ruling span, stringing charts |
| `ols-terrain` | Survey points (CSV/XYZ, feature codes, breaklines), TIN, ground heights and exact profiles; later LiDAR and chunked tiles |
| `ols-line` | Line model (`.ols.json`): routes, structures, attachments, sections; 3D solving and exports |
| `ols-structures` | Structure type library (`.olss`) and insulator assembly library (`.olsi`); placement of types into a line; deriving types from a line |
| `ols-wirefe` | Non-linear FE of wire systems (general loads, broken wire, unbalanced ice) |
| `ols-structures-fe` | 3D frame/truss FE of towers and poles |
| `ols-loads` | Load-case engine; design codes are **plugins** |
| `ols-clearance` | Clearance checks: vertical ground clearance of solved wires (v1); later feature clearances, envelopes |
| `ols-rating` | Thermal rating (IEEE 738, CIGRE) |
| `ols-spotting` | Optimum structure spotting |
| `ols-drafting` | Plan & profile sheets, DXF/PDF |
| `ols-io` | Import/export formats (each format is a plugin) |
| `ols-render` | GPU renderer: chunk streaming, LOD, culling, instancing |
| `ols-gui` | Desktop application |
| `ols-py` | Python bindings (PyO3) for scripting and Python plugins |
| `ols-sdk` | Plugin API, templates, test helpers |

## Session & collaboration

```
 Client A ──┐  commands ►                    ◄ change sets
 Client B ──┼──────────────►  ols-session  ─────────────► all clients
 Client C ──┘                (authoritative)   (only chunks they view)
                                   │
                              engines recompute affected sections
```

- Desktop single-user = the same session running in-process.
- Commands are validated, applied atomically, appended to the log, and broadcast as change sets.
- Recalculation is incremental: a change marks dependent objects dirty (structure → its sections → their clearances) and only those are recomputed.
- See ADR-0005.

## Rendering, LOD & streaming

- World split into a fixed quadtree grid of spatial chunks (project CRS); render distance per layer.
- Terrain quadtree, point-cloud octree (COPC), structure mesh LODs, adaptive wire tessellation, GPU instancing.
- Background loading threads; the UI thread never waits on I/O or computation.
- See ADR-0006.

## Plugin system (proposal — ADR-0003)

Three tiers, same API surface:

| Tier | For | Notes |
|---|---|---|
| **Native Rust** | Performance-critical engines, first-party modules | Compiled in, fastest |
| **WebAssembly** | Third-party calculations, design codes, checks | Near-native speed, any language (Rust, C, C++, Zig…), sandboxed — safe to run on a shared server |
| **Python** | Scripts, reports, glue, prototyping, I/O formats | Easiest for engineers; not in hot loops |

Extension points: `cable_models`, `wire_loads`, `design_codes`, `importers`/`exporters`, `checks`, `reports`, `solvers`, `commands`, `gui_tools`. Each plugin declares the SDK version it targets.

## Tech stack (proposal — ADR-0002)

- **Core & engines**: Rust (speed, memory safety, fearless multithreading via `rayon`), `nalgebra`/`faer` for linear algebra, sparse solvers for FE.
- **Renderer**: `wgpu` (Vulkan / Metal / DirectX 12, and WebGPU for a future browser client).
- **GUI toolkit**: deferred to the start of Phase 3 (prototype-based choice) — candidates: Slint, egui, Qt via cxx-qt. Criteria: polish, docking panels, accessibility, performance.
- **Networking**: QUIC (`quinn`) / WebSocket, binary serialisation.
- **Geospatial**: PROJ, GDAL bindings where needed, COPC/LAZ for point clouds.
- **Scripting**: Python via PyO3 (`ols-py`), WASM runtime (`wasmtime`) for plugins.
- **Quality**: cargo test, proptest, clippy, criterion benchmarks in CI; pytest for bindings.
- **Docs**: mdBook or MkDocs (theory manual + API).
