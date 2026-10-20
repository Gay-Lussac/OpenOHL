# Architecture

> Subject to the architecture decisions ([ADR-0001](adr/0001-license.md) to [ADR-0010](adr/0010-version-control.md); the index is under Design › Architecture decision records). Requirements in [Design goals](DESIGN_GOALS.md).

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
│ Clients     Desktop app (ohl-app) · CLI (ohl-cli) · Python (ohl-py)│
│             · web client (served by ohl-server)                    │
├──────────────────────────────────────────────────────────────────┤
│ Session     ohl-session: project file, commands, operation log,    │
│             undo/redo, versions, grants; ohl-server: many users    │
├──────────────────────────────────────────────────────────────────┤
│ Engines     sagtension · wirefe · line · loads · structures-fe ·   │
│             clearance · thermal · spotting · bom · drawing · io    │
├──────────────────────────────────────────────────────────────────┤
│ Domain      cables · weather · wireloads · structures · terrain ·  │
│             lidar · pointcloud · crs                               │
├──────────────────────────────────────────────────────────────────┤
│ Core        ohl-core: units, geometry, project grid, time budgets  │
│             ohl-plugins (WASM host) · ohl-render · ohl-validation  │
└──────────────────────────────────────────────────────────────────┘
```

## Modules

Every core module is a Rust crate under `crates/` (workspace members, with `apps/structure/*`):

| Crate | Responsibility |
|---|---|
| `ohl-core` | Units (SI inside, US customary and SI at the boundary), geometry, the project grid of spatial chunks, time budgets and cancellation (exit clauses), text and file helpers |
| `ohl-cables` | Cable library (`.ohlw`): components, composite properties, stress-strain and creep curves, cable models, electrical data |
| `ohl-weather` | Weather cases (`.ohlwc`: temperature, wind, ice) and the loads they put on wires; code-agnostic |
| `ohl-wireloads` | Additional loads on wires: point loads (N) and distributed loads (N/m), permanent or per load case |
| `ohl-sagtension` | Catenary (also segmented, with point loads), change of state, tension sections (ruling span and exact equal-H), non-linear cables, tension criteria, stringing charts |
| `ohl-wirefe` | Finite-element wire systems: exact elastic catenary elements, truss strings, 3D Newton solver |
| `ohl-line` | Line model (`.ohl.json`): routes, structures, attachments, sections, jumpers, electrical systems; 3D solving (RS and FE, elements following the curves), incremental re-solving, exports |
| `ohl-structures` | Structure type library (`.ohls`) and insulator library (`.ohli`); placement of types into a line; deriving types from a line |
| `ohl-structures-fe` | Structure analysis: 3D truss/frame/cable finite elements (members of a full-design `.ohls`, or `.ohlfe`; P-Delta, skyline solver), member usage, poles (`.ohlp`) and the pole check |
| `ohl-sections` | Section library (`.ohlsec`): standard profiles and materials shared by a project, cross-section properties from the dimensions, projected widths for wind |
| `ohl-loads` | Load cases (`.ohllc`: weather × wire condition × load factors), the loads on the structures at every attachment, wind on structures, capacity checks (Method 1 allowable spans, Method 2 load trees, FE and pole) |
| `ohl-terrain` | Survey points (CSV/XYZ, feature codes, breaklines), TIN, ground heights and exact profiles |
| `ohl-lidar` | LAS/LAZ reading, class schemes (`.ohlc`), corridors, classification (ground filter, height classes) |
| `ohl-pointcloud` | Point clouds in the project file (chunk × class blocks with levels of detail), COPC read and write, point-source interface |
| `ohl-clearance` | Clearances of solved wires by rule (`.ohlr`: ground, points, other lines, same line), envelopes; galloping ellipses, blowout against the right of way, vegetation, as-built wire fits |
| `ohl-thermal` | Thermal rating: IEEE 738, CIGRE TB 601 / TB 207, transient, solar position, ambient cases (`.ohlta`), clearance-limited temperature |
| `ohl-spotting` | Optimum structure spotting: least-cost structure positions, types and extensions along a route profile (dynamic programming) |
| `ohl-bom` | Bill of materials (structures, insulators, cut lengths, hardware) and cost estimate (`.ohlcost`) |
| `ohl-drawing` | Plan and profile drawings per route and profile exports: SVG, DXF, PDF, PNG, CSV, XML |
| `ohl-background` | Backgrounds in the project file: orthophotos (GeoTIFF / world files, JPEG tiles with a zoom pyramid) and DXF drawings |
| `ohl-crs` | Coordinate systems (EPSG, proj4), transformations of points and lines; pure Rust |
| `ohl-io` | Import/export: format traits and registry, built-in formats (CSV, KML/KMZ, OHL XML, LandXML, GeoJSON, Shapefile, GeoPackage, IFC4, XLSX), structures along a route, formats and projects from WebAssembly plugins |
| `ohl-session` | Project file (`.ohl`, SQLite), validated commands, operation log, per-user undo/redo, admin rollback, roles, live incremental solving; version control (revisions, branches, merge — ADR-0010), edit grants and reviews; spatial index |
| `ohl-server` | Server mode (pre-alpha, in active development): a project hosted over QUIC, edits applied in one order and broadcast, clients' replicas, presence, soft locks, invitations; the web client (HTTPS + WebSocket) |
| `ohl-render` | wgpu renderer: lines, points, surfaces in an offscreen texture; orbit camera; chunks with levels of detail, frustum culling and streaming; headless images |
| `ohl-plugins` | Plugin manifests (`ohl-plugin.toml`), compatibility checks, the sandboxed WebAssembly component host (WIT worlds for formats, projects, terrain, design codes) |
| `ohl-validation` | Validation framework: cases (`benchmarks/`), runner and reports (`ohl validate`) |
| `ohl-app` | OHL-Design, the line application `ohl-design` (egui, ADR-0009): 3D, plan, profile, properties, tools, tables, library editors, checks, Python console, plugin manager |
| `ohl-cli` | The `ohl` command-line tool |
| `ohl-py` | Python bindings (PyO3, `import ohl`) for scripts and Python plugins |

The structure application (`apps/structure/`, [its crates](development/structure-app.md))
is kept apart from the core: its crates depend on core crates, never the
other way round, and its tests run on their own.

**The boundary between the programs.** Where a core program needs the
structure side, the programs talk instead of the crates depending on each
other: OHL-Design starts OHL-Structure (`ohl-structure`, found through
`OHL_STRUCTURE_APP`, next to `ohl-design`, or on the `PATH`) with a
line-values file (JSON) and watches the files it saves; `ohl structure
analyse|report` starts the structure side's headless tool
`ohl-structure-report --request -` (found through `OHL_STRUCTURE_REPORT`,
next to `ohl`, or on the `PATH`), writes a JSON request on its input and
reads a JSON reply on its output (`ohl-structure-request` /
`ohl-structure-reply`, [data model](development/data-model.md#structure-request-and-reply-json)).
The JSON formats are the contract; each side has its own types for them,
and `apps/structure/ohl-structure-tests/tests/ohl_structure_cli.rs` builds
both programs and runs them together.

| Crate | Responsibility |
|---|---|
| `ohl-structure-loads` | A structure's load-case recipes (`[[load_case]]`, `.ohlslc`), their evaluation into the load table, CSV of both; plain data, no line |
| `ohl-structure-link` | Where the application meets the core: the line's structure loads as line values, a table case as finite-element loads (later the section library and saving back) |
| `ohl-structure-tests` | Tests of the application with the core (test-only) |
| `ohl-structure-edit` | The edits of a structure file (`toml_edit`, comments kept), undo and redo of whole edits, what each level still lacks (`missing(level)`); no UI. `ohl-app`'s structure screen calls the same edits (on the basics: points, strings, wire points, sets) |
| `ohl-structure-export` | The exports of a level 3 structure ([Structure exports](development/structure-exports.md)): IFC4, IFC2x3, SAF, CSV tables; the tool `ohl-structure-export`; the structure report and the tool `ohl-structure-report`, which also answers `ohl`'s JSON requests (`--request`) |
| `ohl-structure-app` | OHL-Structure, the structure application `ohl-structure` (egui): a `.ohls` in 3D, the Geometry, Body, Design, Load cases, Results and In the line menus at the top, their panels docked and movable under them |

## Session & collaboration

```
 Client A ──┐  commands ►                    ◄ change sets
 Client B ──┼──────────────►  ohl-session  ─────────────► all clients
 Client C ──┘                (authoritative)
                                   │
                              engines recompute affected sections
```

- Desktop single-user = the same session running in-process; joined, the application keeps a replica of the project and applies the server's broadcasts with the same code (`ohl-server`).
- Server mode, pre-alpha (`ohl-server`, [Server mode](user-guide/server.md)): edits are requests answered after the broadcast; no optimistic prediction, whole-project copy on join, QUIC only for the application; browsers join through the web client (HTTPS + WebSocket, the line and server-solved wires as JSON).
- Commands are validated, applied atomically, appended to the log, and broadcast as change sets.
- Recalculation is incremental: a change marks dependent objects dirty (structure → its sections → their clearances) and only those are recomputed.
- See ADR-0005.

## Rendering, LOD & streaming

- World split into a fixed quadtree grid of spatial chunks (project CRS); render distance per layer.
- Terrain, LiDAR points, structures and wires drawn by chunk at three levels of detail; point clouds streamed by node (stored levels of detail, or a COPC octree) under a point budget ([Level of detail](user-guide/desktop-app.md#level-of-detail)).
- Not yet: GPU instancing, structure mesh LODs from the library, wires tessellated on the GPU, sending a joined user only the chunks they view.
- Background loading threads; the UI thread never waits on I/O or computation.
- See ADR-0006.

## Plugin system (ADR-0003)

Three tiers, same API surface:

| Tier | For | Notes |
|---|---|---|
| **Native Rust** | Performance-critical engines, first-party modules | Compiled in, fastest |
| **WebAssembly** | Third-party calculations, design codes, checks | Near-native speed, any language (Rust, C, C++, Zig…), sandboxed — safe to run on a shared server |
| **Python** | Scripts, reports, glue, prototyping, I/O formats | Easiest for engineers; not in hot loops |

WebAssembly plugins implement a WIT world of `crates/ohl-plugins/wit/`: `io-plugin` (file formats, extension point `io`), `terrain-plugin` (`terrain`), `project-plugin` (`project`, projects from other programs), `code-plugin` (`design-code`), `check-plugin` (`check`) and `code-check-plugin` (a design code that also runs checks); the package is `ohl:plugin`, SDK 0.3. Python plugins are scripts run by the application's console. Each plugin declares the interface version it targets in its manifest; see [Writing plugins](development/plugins.md). Further extension points (cable models, reports, solvers, GUI tools) are planned.

## Tech stack (ADR-0002)

- **Core & engines**: Rust (speed, memory safety, background threads), `nalgebra` for linear algebra, a skyline LDLᵀ solver for structure FE.
- **Renderer**: `wgpu` (Vulkan / Metal / DirectX 12; WebGPU in the browser).
- **GUI toolkit**: egui with wgpu ([ADR-0009](adr/0009-gui-toolkit.md)).
- **Networking**: QUIC (`quinn`, TLS via `rustls`) for the application; HTTPS + WebSocket (`tokio-tungstenite`) for the web client.
- **Geospatial**: pure Rust, no native GIS library: `proj4rs` with EPSG definitions, `las`/`laz` for point clouds (COPC read and written), formats written from their public specifications.
- **Scripting**: Python via PyO3 (`ohl-py`), WASM runtime (`wasmtime`) for plugins.
- **Storage**: SQLite (`rusqlite`) for the project file, `zstd` compression of point and surface blocks.
- **Quality**: cargo test, clippy, criterion benchmarks; validation cases (`ohl validate`, 85 cases in `benchmarks/`); pytest for bindings. The checks run locally before each push; the CI workflow is started by hand only (lint, tests, validation cases, docs; macOS, Windows and the benchmarks in a full run).
- **Docs**: mdBook (this book: user guide, theory manual, design) and `cargo doc` (API).
