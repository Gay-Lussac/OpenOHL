# Roadmap

The roadmap is organised in **phases**. Each phase ends with a usable, tested deliverable. Phases map 1:1 to GitHub **milestones**, and each bullet becomes one or more **issues** labelled by module (`module:*`).

Guiding principles:
- **Headless engine first, GUI second.** Every calculation must be usable from CLI/scripts and covered by validation cases before it gets a user interface.
- **Faster and friendlier than existing tools** — native Rust core, GPU rendering, measurable performance targets ([DESIGN_GOALS.md](https://github.com/Gay-Lussac/OLS-CAD/blob/main/docs/DESIGN_GOALS.md)).
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

- [x] Line data model (`ols-line`, `.ols.json`): alignment, structures, attachment sets (insulator + wire points), wire sections, stringing, point loads (stations and offsets per route: see Routes below)
- [x] `ols-session`: project file (`.ols`), validated commands, operation log, per-user undo/redo (content-based conflicts), admin rollback, roles, dirty tracking with **incremental re-solving** (`ols-line::incremental`, `ols_session::Live`: only the touched sections, identical to a full solve), `.ols.json` import/export
- [x] Point clouds in the project file (`ols-pointcloud`, ADR-0007): import from LAS/LAZ, chunk × class blocks, point-source interface (database link as a plugin later)
- [x] Spatial chunk index (ADR-0008): fixed quadtree tile grid (`ols_core::grid`), loose home tiles, index in the project file kept up to date by every command, area/tile queries (`ols project tiles`, `ols project query`), point blocks selected by area and counted per tile (basis for streaming and LOD)
- [x] Routes (DESIGN_GOALS §9): several alignments per line, structures shared between routes (Y pylons, parallel routes rejoining), stationing, offsets and line angles per route
- [x] Coordinate systems (`ols-crs`, pure Rust proj4rs + EPSG registry): points and whole lines (`ols crs`), LV95 ↔ WGS84 matching swisstopo REFRAME to < 1 mm *(grid-based datum shifts via a PROJ plugin later)*
- [x] `ols-terrain`: survey point import (CSV/XYZ, named columns incl. PNEZD), feature codes, breaklines, constrained-Delaunay TIN, ground heights, exact profiles along segments (route centreline and side profiles in the drawings)
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
- [x] Plan & profile views per route (`ols-drawing`, `ols line --drawings DIR`): straightened plan + profile with ground, side profiles, clearance line, structures, wires by phase, violation markers — SVG and profile CSV — sheets on A0–A4/custom formats split at structures, title block with logos, separate plan scale across the line *(DXF, PDF, PNG and XML since Phase 5, `ols drawing`; layout templates as plugins still to come)*
- [x] Exporter/importer plugin interface (first formats: CSV, KML/KMZ; GeoJSON example plugin)
- [x] Display: phase colours per circuit/phase, configurable per project (by phase label) and per section, legend in the viewer (DESIGN_GOALS §12)
- [x] Jumpers connecting sections at dead-ends or taps (slack catenary between solved clamps, length or sag input, optional support string, weather loads, ground clearance; DESIGN_GOALS §12) *(clearance to the structure body still to come)*

**Deliverable:** load a survey + structure positions, compute wire positions and ground clearance violations from CLI.

## Phase 3 — Desktop application MVP
*Goal: engineers can build and inspect a line interactively.*

- [x] Application shell (`ols-cad`, egui + wgpu, ADR-0009): background loading and solving, dockable views sharing selection and weather case; edits go through `ols-session` commands
- [x] `ols-render` basics: wgpu lines (pixel widths), points, shaded surfaces with depth and anti-aliasing; origin-relative precision; orbit camera; headless images
- [x] `ols-render`: chunks on the project grid streamed in and out by distance, render distance per layer, three levels of detail (terrain grids, LiDAR shares, structure simplification, thinned wires and bundles), frustum culling, a level-of-detail table per layer in the app
- [ ] `ols-render`: GPU instancing of structures and hardware, occlusion culling, wires tessellated on the GPU from catenary parameters, structure mesh LODs from the library, labels
- [x] Plan view, profile view, 3D view with synchronised selection; properties of structures and spans in every weather case; layers; LiDAR points by class role and ground from LiDAR
- [x] Live update while editing (Move window: the shown case re-solves the touched sections as values change; one command on Apply)
- [x] Tools as a Minecraft-style hotbar (1–9, wheel when captured) with an inventory (E, drag and drop), remembered
- [x] 3D navigation: fly with W A S D, Space/Shift up/down, Ctrl faster; control schemes as data (Orbit; Minecraft creative with captured mouse, crosshair selection, Escape to free) — more schemes and editing later
- [ ] Command palette, keyboard workflows, explainable results (click value → inputs & references)
- [x] Editing through the session in the app: move (drag or typed position) and delete structures (confirmation listing what goes with them), undo/redo, incremental re-solve of every case
- [x] Structures and strings in the app: Add structure (on the ground, type from the library), Add string, Edit (structure, string in the structure's axes, override of a type's string) and Delete (structures, strings, sections, jumpers) tools, one session command each *(spotting by hand on the profile still open; automatic spotting: Phase 5)*
- [x] Wire stringing: String wire tool (strings in order → section window: cable, circuit, system, bundle), sections edited and strung again through new points; sagged to a tension or a sag (ruling span or chosen span); status in the blocks (rolling: equal tension in all spans in every case, default) or clipped in (unstressed lengths per span) *(tension criteria by weather case still open)*
- [x] Clearance violation display: Clearances tab (rule file, violations of the shown case re-checked after edits in the background, click to go there), red marks in 3D and plan *(reports: `ols line --clearance-csv`)*
- [x] Library editors (wires, insulators, structure types, weather cases): fields with units, known fields to add, checked as typed, comments kept; files browsed or created anywhere (system file dialog) and used for the model; the model reads its libraries again on save
- [x] Menu bar (File, Edit with Snap, View pages and layers, Lines, Tools, Checks on demand, Terrain, Structures and Sections with their libraries and tables, Criteria, Parameters, Plugins, Help) and a customizable quick bar; pages closable and reopened from View
- [x] Table editing: structures, sections, strings, jumpers, electrical systems, alignment points, weather cases (written to the case file) — every value typed in, rows added and deleted (dependents confirmed), renames followed by what refers to them, section targets (tension or sag in its display case), per-section display weather case; one command per Apply, sorting, filter *(paste from spreadsheets still open)*
- [x] Alignment points drawn in 3D and plan, selected, edited, deleted and added (tool and table); the profile follows
- [x] Project lines: frozen states of the line (as built, as planned) saved in the project, shown next to the working line in 3D and plan, made the working line (one undoable command) *(in the profile and in drawings still open)*
- [x] Add jumper tool (missing jumpers at a structure, or between two strings); ⌘S / Ctrl+S saves
- [x] Jumpers held by their clamps: bending stiffness (estimated from the cable or set) and the angle each end leaves its clamp at; solved as an inextensible elastic rod; project lines drawn like the working line (bundles, yokes, strings, jumpers)
- [x] Jumpers held by support clamps between their ends: each clamp on an I or a V of a structure's strings, free to swing and turn, the jumper straight through it (a rigid piece of the rod, also with EI = 0); tension-only strings (a leg goes slack in the wind); edited in the jumper's window, shown in the tables, properties and 3D view, written to OLS XML; their strings' pull in the structure loads *(strings below other strings, e.g. a V with a string below, not yet)*
- [x] Strain assemblies always free members (move with the weather, in the calculation); weights of geometry-only strings estimated from their data position in the stringing state
- [x] Libraries (wires, structure types, insulators, weather cases) kept in the project file; export to files to share or compare; Terrain / Structures / Sections menus; LiDAR classes shown; table defaults shown
- [x] Section calculations: Initial / Creep / Load / Max Sag × RS / FE (Sections table, section editor); Criteria › Creep-Stretch (weather cases for final after creep and after load); cable models nonlinear, linear with creep ∝ creep-case tension, linear with a temperature increase; bimetallic curves per material in the wire library editor
- [x] Projects from other programs: `project-plugin` WIT world (line + library files + notes), File › New project from another program, `ols project import-from` with clip-in; example plugin `project-bundle`
- [x] Terrain surfaces (TIN) in the project: several, the active one is the ground, others shown in colour; import LandXML / OBJ / survey points and terrain plugins (`terrain-plugin` world, e.g. ESRI ASCII grids), build from LiDAR ground classes, export LandXML / OBJ; `ols surface`
- [x] Plugin manager UI: plugins installed and found in folders, what they provide, why one cannot be used, on/off, install, remove; their formats in File › Import into the line / Export (run on a worker)
- [x] Python scripting console inside the application: embedded interpreter on its own thread, the `ols` module reads the line and results and edits through session commands (undoable), Stop and a time limit, plugin scripts
- [x] **Server mode (alpha)** (`ols-server`, `ols serve`, File › Host / Join): host a project, multiple users connect over LAN/Internet (QUIC, TLS pinned by the invitation), live change broadcast to replicas, presence (People page, plan markers), soft locks (Move window), per-user undo, roles & permissions (personal invitations; admins set roles), admin rollback (Edit › History) *(still open: optimistic prediction, chunk-based interest management, a WebSocket fallback for the application, OIDC)*

**Deliverable:** alpha release of the desktop app, including shared multi-user sessions.

## Phase 4 — Loading & structural checks
*Goal: design-code compliant loading.*

*Status: delivered as a base. Each item says what is simplified or left for later.*

- [x] `ols-loads`: load case framework (weather × load factors × wire condition), `.olslc` files kept in the project, edited in the app
- [x] Design code plugins: NESC (ANSI C2), CSA C22.3, IEC 60826, EN 50341 (+ national NNAs), ASCE MOP 74, GO 95 — each as a separate plugin, plus the Swiss LeV *(base: `code-plugin` world, `plugins/codes/*`, `ols code`, Criteria › Design code; each code's method with every value from the user's parameter file — no code tables shipped; security / broken-wire and construction loads, NESC constant k and code-specific wind on structures still to come)*
- [x] Structure capacity Method 1: allowable spans (wind/weight span, line angle) — `[capacity.spans]` in the structure types, interpolated by line angle, minimum weight span (uplift)
- [x] Method 2: allowable load trees per attachment (vertical, uplift, transverse, longitudinal; linear interaction optional) against factored loads × strength factor *(interaction diagrams of the whole structure later)*
- [x] Finite-element wire system (SAPS-style, `ols-wirefe`): exact elastic catenary elements, I- and V-strings (tension-only legs), 3D wind blowout, insulator swing, broken span, point loads, unequal spans *(delivered early, in Phase 1)*
- [x] FE wire system extensions: strain insulators (free strain members), jumpers (solved between the section ends), bundles, non-linear stress-strain and creep in elements (secant iteration on the curves, permanent set from the material state; FE and RS agree within 0.02 % in tension on a fixed-end span) *(general linked assemblies from the structure library wait for the insulator assembly graph, with 3D pylons)*
- [x] Structure loads output (attachment point loads per load case): line and structure axes, unfactored and factored; Structure loads page, CSV (`ols line --load-cases --loads-csv`) — wind on the structures by height band (`[[wind.band]]`), the strings holding jumpers' support clamps *(the pull of jumpers on their end clamps still open)*
- [x] Usage / utilisation reports: Structure loads › Usage (worst first, governing case, colours), `ols line --usage-csv` *(usage colours in the views later)*

**Deliverable:** code-checked line design with structure loading reports.

## Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

*Status: delivered as a base. Each item says what is simplified or left for later.*

- [x] `ols-structures-fe`: 3D frame/truss FE analysis of lattice towers and poles (geometric non-linearity, guys) — standalone module. *Base:* `.olsfe` models (truss, 3D Euler–Bernoulli frame, tension-only cables with pretension, supports, springs), first order or P-Delta (geometric stiffness, Newton iterations with exit clauses), skyline LDLᵀ with RCM ordering (1000 members in milliseconds), member usage (tension, compression with Euler buckling, linear N + M interaction), attachment nodes fed with each load case's factored loads (`fe` check in the structure usage, `ols structures fe`). *Simplified / later:* no corotational large rotations, no wind on the tower body, no code column curves or angle-section eccentricities, no member-load cases beyond self-weight, no 3D view of the model
- [x] Wood/steel/concrete pole libraries and checks. *Base:* `.olsp` poles (taper by diameter or circumference, fibre stress for wood, moment capacity at the ground line or by height, hollow wall), moment along the pole from the factored attachment loads (both directions, offsets) plus wind on the pole, P-Delta amplification from a tapered frame model (`pole` check, `ols structures pole`). *Simplified / later:* one amplification factor for every height, no foundation/embedment check, no shear or deflection limits, no pole class tables (user data)
- [x] LiDAR basics (`ols-lidar`, delivered early in Phase 2): LAS/LAZ tiles, class schemes with roles (`.olsc`, any classification), ground in a corridor, TIN from LiDAR — matched a commercial TIN's span clearances to a median of 2 mm on a real line
- [x] LiDAR: COPC, octree streaming and point budget (billions of points at 60 fps) — *base*: COPC read directly as a point source (area/corridor/depth queries by octree node; also on `--pointcloud` for `ols line` and the app), imported like LAZ, written on export (in memory), `ols pointcloud info` shows its octree; stored clouds keep levels of detail per block (¼, 1/16, 1/64 of the points; schema 2, `ols pointcloud octree` for older files, ADR-0007 amendment); the 3D view streams nodes on background threads by screen-space error under a per-user point budget (default 5 M) with an LRU GPU cache. Measured (M1 Max, synthetic 25 M points): 5 M points drawn at 16.5 ms a frame, 10 M at 32 ms; a view read in 0.1–0.2 s. Not measured with 10⁹ points (synthetic data only, extrapolated in the user guide); stored-cloud nodes are 256 m squares × 4 levels, not a spatial octree, so very large *areas* hit the budget at the coarsest level; COPC export holds the points in memory; COPC colours/GPS time not kept
- [x] LiDAR: classification, vegetation & encroachment analysis, as-built wire extraction/fitting — base: ground by progressive morphological filter + height classes (`ols lidar classify`), vegetation encroachment with growth and fall-in per span, catenary fits per span with the survey temperature from the model (synthetic tests: sag within 2 cm, temperature within 2 °C); app windows. Still to come: buildings/wires/poles told apart from vegetation, leaning trees
- [x] Thermal rating: IEEE 738, CIGRE TB 601 / TB 207, max operating temperature from clearance — `ols-thermal`: ampacity, temperature, transient, solar position; `.olsta` ambient cases; per-section clearance-limited temperature (ground and point rules); `ols thermal`, Checks › Thermal rating *(base: uniform conductor temperature, no radial gradient; no published TB 601 example in the validation yet)*
- [x] Optimum (automatic) structure spotting — cost-based optimisation: dynamic programming over candidate stations × types/extensions (ground clearance of a design catenary, allowable wind/weight spans, angle points, forbidden zones); `ols spot`, Tools › Spot structures (proposal as a new line file) *(clearance under the centreline only, one tension for the route)*
- [x] Galloping ellipses, blowout, phase-to-phase / circuit-to-circuit clearances — base: ellipses by user coefficients (`.olsg`; Davison / RUS / CIGRE TB 322 forms, no coefficients shipped), swept along the span, per span pair (`ols clearance galloping`, app window); blown-out wires against the right of way (`ols clearance row`); `circuits = same|different` on wire rules with case pairs
- [x] Drafting: plan & profile sheets, title blocks, DXF/DWG-compatible output, PDF (`ols drawing`, File › Export drawings): one format-free drawing model written as SVG, R12 DXF (sheets in mm, plan in project coordinates, profile by station × exaggerated height), vector PDF (a page per sheet) and PNG *(base: no DWG written — the R12 DXF imports into DWG-based programs; DXF has ACI colours, no line weights or logos; PDF uses the standard Helvetica fonts; title block layout still fixed, templates as a plugin to come)*
- [x] Profile exports: ground/wire/clearance profiles with configurable scales and station ranges — DXF, PDF, SVG, CSV/XML, images (DESIGN_GOALS §8) — `--from`/`--to`, `--hscale 1:2000 --vscale 1:200`, `--profile-only`; profile XML documented in the user guide *(base: one weather case solid and the others dashed, as in the drawings)*
- [x] Bill of materials & cost estimation: structures by type/extension, insulator assemblies, cut lengths per cable (bundles, jumpers), spacers/dampers by rule, priced with a `.olscost` file (missing prices listed); `ols bom` (text, CSV, JSON), Checks › Bill of materials
- [x] Collaboration (base, ADR-0010): edit grants per user or role limited to kinds, objects or a plan polygon, checked by the session for every command (so by the server too); review & approval of branches with protected branches; offline work as a branch carried back in a bundle and merged. Not yet: automatic sync of offline copies, custom roles, area grants drawn in the plan
- [x] **Git-like version control** of projects (base, ADR-0010): content-addressed revisions of the line and its libraries in the project file, history browser (Edit › Versions, `ols project history`), diff field by field with engineering impact (touched sections solved on both sides: sag, tension, ground clearance), branches, three-way merge by object and field with conflicts settled per key, tags, revert; through the server's operations. Not yet: cherry-pick, visual diff in the views, history export for audit (DESIGN_GOALS §7)
- [x] Web client (WebGPU) joining server sessions — base: `ols serve --web PORT` / File › Host this project (Web client), HTTPS + WebSocket with the same codes and roles; 3D (WebGPU, Canvas2D fallback), plan and profile of the line with wires solved on the server (one weather case), selection, presence, live operation log; designers move structures. Not yet: terrain/point clouds, weather choice, other edits
- [x] Interop plugins: XML (LandXML + open OLS XML schema), KML/KMZ, IFC, Shapefile/GeoJSON/GeoPackage, CSV, XLSX; import of common neutral formats from other tools (DESIGN_GOALS §8) — base, built into `ols-io` (written from the public specifications, no native GIS library): OLS XML (whole line, lossless, `docs/schema/ols-line.xsd`), LandXML alignments + CgPoints (export; import of points/alignments), GeoJSON / zipped Shapefile / GeoPackage layers (structures, routes, wires per case with tensions and sag), IFC4 (structures as proxies with stick geometry, spans as cable segments, property sets), XLSX report tables; structures imported from CSV (mapped columns), GeoJSON, Shapefile and LandXML points along a route, into a line or a project as one command. Left for later: shapefile `.prj` / GeoPackage WKT for projected systems, GeoPackage spatial index, IFC georeferencing (`IfcMapConversion`) and solid geometry, curves of LandXML alignments (read as chords); DXF and PDF are written by `ols drawing`, not as `ols-io` formats

**Deliverable:** beta release usable on real projects (with independent verification).

## Pylons — structures in their own axes, three levels of detail
*Before 1.0, agreed with the project owner (2026-10-10). A pylon is defined
in its own axes (x right of the line, y ahead, z up; 0;0 at the centre)
and placed in the line by a position, an angle and a height adjustment.*

- [x] Placement: the type's z = 0 may be any reference level (usually under the lowest crossarm); `foot` gives the level of its feet, so the placed pylon stands with its feet on the ground, raised by the structure's `height_adjust` (foundations); extensions lengthen the body below the reference level
- [x] One file per pylon (`.olss`): joints and members in the type itself (sticks), level 3 adds sections, materials, supports and member kinds (truss / frame / cable) to the same members; `.olsfe` models still read. The level is **derived from the content** (1 stick figure → 2 lattice outline as soon as there are members → 3 as soon as every member has a section and material and there are supports); the marker follows
- [x] String points as joints or as a fraction along a member, so strings follow the members
- [ ] Symmetry (optional): members and joints copied about x, y or both
- [x] Structure type workspace in the app: its own 3D view in local axes, tables (joints, members, points, strings, wire points), select / edit / drag with snaps, add members, string presets; used-by count; the line follows on save *(base: I and V presets — TAK and V + string by hand; ids renamed in Fields; drag handles in the line's 3D view still to come)*
- [ ] Level 2 and 3 drawn with their members in 3D and on plan & profile sheets
- [ ] PLS-TOWER geometry from PLS XML (joints, members, sections) into levels 2 and 3 (local importer)
- [ ] Level 2 from LiDAR: a plugin (later); the classes holding pylons chosen by the user from all classes, as in the display options
- [ ] Clearances to the structure body: none for level 1 (LiDAR points only, separate topic); levels 2 and 3 computed — **the tests are to be defined with the project owner**

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
