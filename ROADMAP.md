# OLS CAD Roadmap

The roadmap is organised in **phases**. Each phase ends with a usable, tested deliverable. Phases map 1:1 to GitHub **milestones**, and each bullet becomes one or more **issues** labelled by module (`module:*`).

Guiding principles:
- **Headless engine first, GUI second.** Every calculation must be usable from CLI/scripts and covered by validation cases before it gets a user interface.
- **Faster and friendlier than existing tools** — native Rust core, GPU rendering, measurable performance targets ([docs/DESIGN_GOALS.md](docs/DESIGN_GOALS.md)).
- **Multi-user from day one** — every edit is a command applied by a session, so collaboration is built in, not bolted on.

---

## Phase 0 — Foundations
*Goal: a project people can contribute to.*

- [x] Choose license (see ADR-0001)
- [x] Choose tech stack: language, 3D engine (ADR-0002) — GUI toolkit deferred to Phase 3
- [x] Define the plugin / extension mechanism (ADR-0003)
- [x] Define the open project file format (ADR-0004)
- [x] Define the collaboration / session model (ADR-0005)
- [x] Define the rendering, LOD & streaming strategy (ADR-0006)
- [x] Cargo workspace layout, CI (clippy, tests, benchmarks) on Linux/macOS/Windows
- [x] Python bindings skeleton (`ols-py`, PyO3) and WASM plugin host skeleton
- [x] Performance benchmark harness (criterion) with regression gate in CI
- [x] Units system (SI internally, US customary + SI at the I/O boundary)
- [x] Documentation site skeleton (theory manual + API reference)
- [x] Validation/benchmark framework (case format + reference results + tolerance reporting)
- [x] Contribution guide, code of conduct, governance, clean-room policy
- [x] Exit clauses: `ols_core::budget` (iteration/time limits, cancellation), plugin time & memory limits, validation case time limits, CI job timeouts

**Deliverable:** empty-but-wired repo, CI green, ADRs accepted.

## Phase 1 — Calculation core (headless)
*Goal: trustworthy single-section sag-tension engine.*

- [x] `ols-cables`: conductor/cable library — area, diameter, weight, RTS, modulus, thermal coefficients, bimetallic (core/outer) properties
- [x] Stress-strain models: linear elastic, polynomial (initial / final / creep curves, per CIGRE/Aluminum Association practice), bimetallic split, aluminium compression limit
- [x] Creep models: creep-curve method (10-year curve) — *time-based / equivalent-temperature variants still open*
- [x] `ols-sagtension`: exact catenary, parabola approximation, inclined spans, sag/tension/length/slack
- [x] Change-of-state equation; initial / final-after-creep / final-after-load conditions
- [x] Ruling-span (equivalent span) method for tension sections, plus exact equal-H section solution
- [x] Weather cases (temperature, wind, ice, combined) and tension limit criteria (% RTS, catenary constant, H/w); ice as radial thickness + density or as a load per length (density → wind diameter); case files (`.olswc`) with tags, cases chosen per run
- [x] `ols-wireloads`: additional loads on wires — point loads (N, 3D vector) and distributed loads (N/m, full span or sub-interval), permanent or per load case
- [x] Segmented-catenary solution for spans with point loads / partial distributed loads *(vertical plane; transverse components → FE wire model, Phase 4)*
- [x] Stringing charts output (CSV; PDF later with drafting)
- [x] CLI: `ols sagtension job.toml` (report in metric or US units, CSV chart)
- [x] Performance: single section re-solve < 10 ms *(~60 µs for 20 spans; loaded span change of state ~1 ms)*
- [ ] Validation: CIGRE TB 324 examples, textbook cases, hand calcs

**Deliverable:** `ols` CLI + Rust library + Python bindings, reproducible sag-tension tables validated against published references.

## Phase 2 — Line model & terrain
*Goal: a real line described in data, with a ground profile.*

- [x] Line data model (`ols-line`, `.ols.json`): alignment, structures, attachment sets (insulator + wire points), wire sections, stringing, point loads *(stations/offsets still to come)*
- [x] `ols-session`: project file (`.ols`), validated commands, operation log, per-user undo/redo (content-based conflicts), admin rollback, roles, dirty tracking with **incremental re-solving** (`ols-line::incremental`, `ols_session::Live`: only the touched sections, identical to a full solve), `.ols.json` import/export
- [x] Point clouds in the project file (`ols-pointcloud`, ADR-0007): import from LAS/LAZ, chunk × class blocks, point-source interface (database link as a plugin later)
- [x] Spatial chunk index (ADR-0008): fixed quadtree tile grid (`ols_core::grid`), loose home tiles, index in the project file kept up to date by every command, area/tile queries (`ols project tiles`, `ols project query`), point blocks selected by area and counted per tile (basis for streaming and LOD)
- [x] Routes (DESIGN_GOALS §9): several alignments per line, structures shared between routes (Y pylons, parallel routes rejoining), stationing, offsets and line angles per route
- [x] Coordinate systems (`ols-crs`, pure Rust proj4rs + EPSG registry): points and whole lines (`ols crs`), LV95 ↔ WGS84 matching swisstopo REFRAME to < 1 mm *(grid-based datum shifts via a PROJ plugin later)*
- [x] `ols-terrain`: survey point import (CSV/XYZ, named columns incl. PNEZD), feature codes, breaklines, constrained-Delaunay TIN, ground heights, exact profiles along segments *(route centreline + side profiles come with the drawings)*
- [x] Structure library v1 (`.olss`, DESIGN_GOALS §10): one file per type, local attachment points, default insulator per set, extensions, placement (position, rotation, extension), derive types from an existing line
- [x] Insulator library basics (`.olsi`, DESIGN_GOALS §11): suspension, V-string, strain assemblies used by the solver (V-strings solved in lines with tension-only legs and slack-leg reporting)
- [ ] *(deferred until 3D pylons)* Insulator assembly model (graph of nodes and elements): suspension (I), V-string, tension/strain (single & multiple), post; **linked strings** via shared nodes (yokes, jumpers, jumper support strings) — see DESIGN_GOALS §6
- [x] Multi-span sections solved in 3D (FE with suspension strings, wind per span, point loads); `ols line` with 3D HTML viewer and CSV exports — checked against a real PLS-CADD line (930 spans, median 1.1 cm)
- [x] Insulator behaviour in the 3D solve: I-string swing, V-string leg forces & slack detection, strain assembly geometry *(from the FE solve; closed-form checks still to add as validation cases)*
- [x] `ols-clearance` v1: vertical ground clearance of every solved span per weather case, violations in `ols line --terrain --clearance` and in the viewer
- [x] `ols-clearance` v2 (DESIGN_GOALS §13): vertical **and** radial clearances; rule files (`.olsr`) per target — ground, LiDAR classes/roles, other lines, other wires of the same line (same span and between spans)
- [x] Clearance envelopes over several weather cases (lowest clearance and governing case per wire and rule), different cases per wire in wire-to-wire checks (`case_pairs`), rules limited to cases; sheets draw every case
- [x] Exclusion zones in clearance rules: near the wire ends (`exclude_near_ends`) and spans at given structures (`exclude_spans_at`, e.g. cable terminations)
- [x] Electrical systems (`systems`: AC/DC, voltage, frequency, phase angles or pole polarities) assigned once per conductor path (sections joined by jumpers); clearances by voltage — tables or formulas per rule (so per LiDAR class group), pair voltage higher / sum / actual difference between conductors; unknown voltages refused before checking
- [x] Bundled conductors: `bundle` subconductors on a circle with `bundle_spacing` (e.g. 4 × 400 mm) and optional rotation; insulator weight shared by the subconductors in the solve; clearances to the nearest subconductor
- [x] Electrical cable data in the wire library (`[cable.electrical]`: AC resistance by temperature, DC resistance, geometric mean radius, emissivity, solar absorptivity; resistance units) — filled from PLS-CADD `.wir`/XML by the local importer until a PLS-CADD importer is decided
- [ ] Rule editor tool (per LiDAR class)
- [x] Plan & profile views per route (`ols-drawing`, `ols line --drawings DIR`): straightened plan + profile with ground, side profiles, clearance line, structures, wires by phase, violation markers — SVG and profile CSV — sheets on A0–A4/custom formats split at structures, title block with logos, separate plan scale across the line *(DXF and layout templates as plugins still to come)*
- [x] Exporter/importer plugin interface (first formats: CSV, KML/KMZ; GeoJSON example plugin)
- [x] Display: phase colours per circuit/phase, configurable per project (by phase label) and per section, legend in the viewer (DESIGN_GOALS §12)
- [x] Jumpers connecting sections at dead-ends or taps (slack catenary between solved clamps, length or sag input, optional support string, weather loads, ground clearance; DESIGN_GOALS §12) *(clearance to the structure body still to come)*

### Phase 2 — next steps (in this order)

The open Phase 2 items above, grouped into four work packages:

1. ✅ **V-strings in line solving + phase colours** — V-string assemblies from the insulator library solved in lines (two tension-only legs from the leg attachment points, free clamp, slack-leg detection), V-string legs drawn in the viewer; wire colours by phase, configurable per project (by phase label) and per section, with a legend.
2. ✅ (v1) **Terrain and clearances** — `ols-terrain` (survey points CSV/XYZ, TIN, profile extraction along each route) and `ols-clearance` v1 (wire-to-ground clearance per weather case, violations report). Synthetic survey data for committed tests; real survey or LiDAR data for local checks if available.
3. ✅ (v1) **Plan and profile drawings per route** — ground profile, wires, structures and clearance lines along a route's stationing; SVG first, then DXF and CSV.
4. ✅ **Jumpers at dead-ends** — slack cable elements between the end of one section and the start of the next (or between circuits), length or sag as input, optional jumper support strings; drawn in 3D and included in clearance checks.

**Deliverable:** load a survey + structure positions, compute wire positions and ground clearance violations from CLI.

## Phase 3 — Desktop application MVP
*Goal: engineers can build and inspect a line interactively.*

- [x] Application shell (`ols-cad`, egui + wgpu, ADR-0009): background loading and solving, dockable views sharing selection and weather case *(editing through `ols-session` commands next)*
- [x] `ols-render` basics: wgpu lines (pixel widths), points, shaded surfaces with depth and anti-aliasing; origin-relative precision; orbit camera; headless images
- [x] `ols-render`: chunks on the project grid streamed in and out by distance, render distance per layer, three levels of detail (terrain grids, LiDAR shares, structure simplification, thinned wires and bundles), frustum culling, a level-of-detail table per layer in the app
- [ ] `ols-render`: GPU instancing of structures and hardware, occlusion culling, wires tessellated on the GPU from catenary parameters, structure mesh LODs from the library, labels
- [x] Plan view, profile view, 3D view with synchronised selection; properties of structures and spans in every weather case; layers; LiDAR points by class role and ground from LiDAR
- [x] Live update while editing (Move window: the shown case re-solves the touched sections as values change; one command on Apply)
- [x] Tools as a Minecraft-style hotbar (1–9, wheel when captured) with an inventory (E, drag and drop), remembered
- [x] 3D navigation: fly with W A S D, Space/Shift up/down, Ctrl faster; control schemes as data (Orbit; Minecraft creative with captured mouse, crosshair selection, Escape to free) — more schemes and editing later
- [ ] Command palette, keyboard workflows, explainable results (click value → inputs & references)
- [x] Editing through the session in the app: move (drag or typed position) and delete structures (confirmation listing what goes with them), undo/redo, incremental re-solve of every case
- [x] Structures and strings in the app: Add structure (on the ground, type from the library), Add string, Edit (structure, string in the structure's axes, override of a type's string) and Delete (structures, strings, sections, jumpers) tools, one session command each *(spotting on the profile still open)*
- [x] Wire stringing: String wire tool (strings in order → section window: cable, circuit, system, bundle), sections edited and strung again through new points; sagged to a tension or a sag (ruling span or chosen span); status in the blocks (rolling: equal tension in all spans in every case, default) or clipped in (unstressed lengths per span) *(tension criteria by weather case still open)*
- [x] Clearance violation display: Clearances tab (rule file, violations of the shown case re-checked after edits in the background, click to go there), red marks in 3D and plan *(reports: `ols line --clearance-csv`)*
- [x] Library editors (wires, insulators, structure types, weather cases): fields with units, known fields to add, checked as typed, comments kept; files browsed or created anywhere (system file dialog) and used for the model; the model reads its libraries again on save
- [x] Menu bar (Edit with Snap, View pages and layers, Tools, Checks on demand, Libraries, Tables; Plugins to come) and a customizable quick bar; pages closable and reopened from View
- [x] Table editing: structures, sections, strings, jumpers, electrical systems, alignment points, weather cases (written to the case file) — every value typed in, rows added and deleted (dependents confirmed), renames followed by what refers to them, section targets (tension or sag in its display case), per-section display weather case; one command per Apply, sorting, filter *(paste from spreadsheets still open)*
- [x] Alignment points drawn in 3D and plan, selected, edited, deleted and added (tool and table); the profile follows
- [x] Project lines: frozen states of the line (as built, as planned) saved in the project, shown next to the working line in 3D and plan, made the working line (one undoable command) *(in the profile and in drawings still open)*
- [x] Add jumper tool (missing jumpers at a structure, or between two strings); ⌘S / Ctrl+S saves
- [x] Jumpers held by their clamps: bending stiffness (estimated from the cable or set) and the angle each end leaves its clamp at; solved as an inextensible elastic rod; project lines drawn like the working line (bundles, yokes, strings, jumpers)
- [x] Strain assemblies always free members (move with the weather, in the calculation); weights of geometry-only strings estimated from their data position in the stringing state
- [x] Libraries (wires, structure types, insulators, weather cases) kept in the project file; export to files to share or compare; Terrain / Structures / Sections menus; LiDAR classes shown; table defaults shown
- [x] Section calculations: Initial / Creep / Load / Max Sag × RS / FE (Sections table, section editor); Criteria › Creep-Stretch (weather cases for final after creep and after load); cable models nonlinear, linear with creep ∝ creep-case tension, linear with a temperature increase; bimetallic curves per material in the wire library editor
- [x] Plugin manager UI: plugins installed and found in folders, what they provide, why one cannot be used, on/off, install, remove; their formats in File › Import into the line / Export (run on a worker)
- [x] Python scripting console inside the application: embedded interpreter on its own thread, the `ols` module reads the line and results and edits through session commands (undoable), Stop and a time limit, plugin scripts
- [x] **Server mode (alpha)** (`ols-server`, `ols serve`, File › Host / Join): host a project, multiple users connect over LAN/Internet (QUIC, TLS pinned by the invitation), live change broadcast to replicas, presence (People page, plan markers), soft locks (Move window), per-user undo, roles & permissions (personal invitations; admins set roles), admin rollback (Edit › History) *(still open: optimistic prediction, chunk-based interest management, WebSocket fallback, OIDC)*

**Deliverable:** alpha release of the desktop app, including shared multi-user sessions.

## Phase 4 — Loading & structural checks
*Goal: design-code compliant loading.*

- [x] `ols-loads`: load case framework (weather × load factors × wire condition), `.olslc` files kept in the project, edited in the app
- [ ] Design code plugins: NESC (ANSI C2), CSA C22.3, IEC 60826, EN 50341 (+ national NNAs), ASCE MOP 74, GO 95 — each as a separate plugin
- [ ] Structure capacity Method 1: allowable spans (wind/weight span, line angle)
- [ ] Method 2: interaction diagrams / allowable load trees
- [x] Finite-element wire system (SAPS-style, `ols-wirefe`): exact elastic catenary elements, I- and V-strings (tension-only legs), 3D wind blowout, insulator swing, broken span, point loads, unequal spans *(delivered early, in Phase 1)*
- [ ] FE wire system extensions: strain insulators, jumpers, general linked assemblies from the structure library, bundles, non-linear stress-strain and creep in elements
- [x] Structure loads output (attachment point loads per load case): line and structure axes, unfactored and factored; Structure loads page, CSV (`ols line --load-cases --loads-csv`) *(wind on structures and jumper loads still open)*
- [ ] Usage / utilisation reports

**Deliverable:** code-checked line design with structure loading reports.

## Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

- [ ] `ols-structures-fe`: 3D frame/truss FE analysis of lattice towers and poles (geometric non-linearity, guys) — standalone module
- [ ] Wood/steel/concrete pole libraries and checks
- [x] LiDAR basics (`ols-lidar`, delivered early in Phase 2): LAS/LAZ tiles, class schemes with roles (`.olsc`, any classification), ground in a corridor, TIN from LiDAR — matched a commercial TIN's span clearances to a median of 2 mm on a real line
- [ ] LiDAR: COPC, octree streaming and point budget (billions of points at 60 fps)
- [ ] LiDAR: classification, vegetation & encroachment analysis, as-built wire extraction/fitting
- [ ] Thermal rating: IEEE 738, CIGRE TB 601 / TB 207, max operating temperature from clearance
- [ ] Optimum (automatic) structure spotting — cost-based optimisation
- [ ] Galloping ellipses, blowout, phase-to-phase / circuit-to-circuit clearances
- [ ] Drafting: plan & profile sheets, title blocks, DXF/DWG-compatible output, PDF
- [ ] Profile exports: ground/wire/clearance profiles with configurable scales and station ranges — DXF, PDF, SVG, CSV/XML, images (DESIGN_GOALS §8)
- [ ] Bill of materials & cost estimation
- [ ] Collaboration: permissions per area/object, review & approval workflow, offline work with sync (stretch)
- [ ] **Git-like version control** of projects: revisions, history browser, diff (incl. engineering impact), branches for design alternatives, merge, tags for issued revisions, revert (DESIGN_GOALS §7)
- [ ] Web client (WebGPU) joining server sessions
- [ ] Interop plugins: XML (LandXML + open OLS XML schema), KML/KMZ, IFC, Shapefile/GeoJSON/GeoPackage, CSV, XLSX; import of common neutral formats from other tools (DESIGN_GOALS §8)

**Deliverable:** beta release usable on real projects (with independent verification).

## Phase 6 — Ecosystem & 1.0
- [ ] Stable plugin SDK (semantic versioning, compatibility guarantees)
- [ ] Plugin registry / index
- [ ] Complete theory manual and validation report
- [ ] Translations (i18n)
- [ ] Public release, governance model, maintainers team

---

## Cross-cutting (every phase)
- Validation cases for every calculation, with references
- Theory documentation written alongside the code
- Performance benchmarks against the targets in DESIGN_GOALS.md (large lines: 500+ structures, LiDAR with 10⁹ points); regressions fail CI
- UX reviews with practising line engineers
- Backwards compatibility of the project file format (versioned schema + migrations)
