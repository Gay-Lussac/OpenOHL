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
- [ ] Stress-strain models: linear elastic, polynomial (initial / final / creep curves, per CIGRE/Aluminum Association practice), bimetallic split
- [ ] Creep models: equivalent temperature, time-based (IEC/CIGRE TB 324 approaches)
- [x] `ols-sagtension`: exact catenary, parabola approximation, inclined spans, sag/tension/length/slack
- [ ] Change-of-state equation; initial / final-after-creep / final-after-load conditions *(linear elastic + thermal done; creep / final conditions pending)*
- [x] Ruling-span (equivalent span) method for tension sections, plus exact equal-H section solution
- [x] Weather cases (temperature, wind, ice, combined) and tension limit criteria (% RTS, catenary constant, H/w)
- [x] `ols-wireloads`: additional loads on wires — point loads (N, 3D vector) and distributed loads (N/m, full span or sub-interval), permanent or per load case
- [x] Segmented-catenary solution for spans with point loads / partial distributed loads *(vertical plane; transverse components → FE wire model, Phase 4)*
- [x] Stringing charts output (CSV; PDF later with drafting)
- [x] CLI: `ols sagtension job.toml` (report in metric or US units, CSV chart)
- [x] Performance: single section re-solve < 10 ms *(~60 µs for 20 spans; loaded span change of state ~1 ms)*
- [ ] Validation: CIGRE TB 324 examples, textbook cases, hand calcs

**Deliverable:** `ols` CLI + Rust library + Python bindings, reproducible sag-tension tables validated against published references.

## Phase 2 — Line model & terrain
*Goal: a real line described in data, with a ground profile.*

- [ ] `ols-core`: project data model — alignment, PIs, stations, structures, attachment sets, wire sections, spans
- [ ] `ols-session`: command system, operation log, undo/redo, incremental recalculation (dirty tracking)
- [ ] Spatial chunk index (quadtree tile grid) (basis for streaming and LOD)
- [ ] Alignment: PIs, line angles, stationing, offsets, coordinate systems (EPSG via PROJ)
- [ ] `ols-terrain`: survey point import (CSV/XYZ), feature codes, breaklines, TIN generation, profile extraction (centreline + side profiles)
- [ ] Structure library v1: simple framing with attachment points (x, y, z)
- [ ] Insulator assembly model (graph of nodes and elements): suspension (I), V-string, tension/strain (single & multiple), post; **linked strings** via shared nodes (yokes, jumpers, jumper support strings) — see DESIGN_GOALS §6
- [ ] Multi-span sections: wire placed between attachment points over terrain
- [ ] Analytic insulator behaviour: I-string swing, V-string leg forces & slack detection, strain string geometry
- [ ] `ols-clearance` v1: ground clearance, feature clearance, clearance envelopes per weather case
- [ ] Plan & profile data output (SVG/DXF/CSV) — no GUI yet
- [ ] Exporter/importer plugin interface (first formats: CSV, KML/KMZ)

**Deliverable:** load a survey + structure positions, compute wire positions and ground clearance violations from CLI.

## Phase 3 — Desktop application MVP
*Goal: engineers can build and inspect a line interactively.*

- [ ] Application shell on top of the in-process `ols-session`
- [ ] `ols-render`: wgpu renderer with chunk streaming, render distance per layer, LOD (terrain quadtree, structure mesh LODs, adaptive wires), culling, instancing
- [ ] Plan view, profile view, 3D view (synchronised selection), live update while dragging
- [ ] Command palette, keyboard workflows, explainable results (click value → inputs & references)
- [ ] Interactive structure spotting (add/move/delete structures on profile)
- [ ] Wire stringing UI (sections, tension criteria, weather cases)
- [ ] Clearance violation display and reports
- [ ] Library editors (cables, insulators, structures, weather cases)
- [ ] Plugin manager UI
- [ ] Python scripting console inside the application
- [ ] **Server mode (alpha)**: host a project, multiple users connect over LAN/Internet, live change broadcast, presence, soft locks, per-user undo, roles & permissions, admin rollback

**Deliverable:** alpha release of the desktop app, including shared multi-user sessions.

## Phase 4 — Loading & structural checks
*Goal: design-code compliant loading.*

- [ ] `ols-loads`: load case framework (weather × load factors × wire condition)
- [ ] Design code plugins: NESC (ANSI C2), CSA C22.3, IEC 60826, EN 50341 (+ national NNAs), ASCE MOP 74, GO 95 — each as a separate plugin
- [ ] Structure capacity Method 1: allowable spans (wind/weight span, line angle)
- [ ] Method 2: interaction diagrams / allowable load trees
- [x] Finite-element wire system (SAPS-style, `ols-wirefe`): exact elastic catenary elements, I- and V-strings (tension-only legs), 3D wind blowout, insulator swing, broken span, point loads, unequal spans *(delivered early, in Phase 1)*
- [ ] FE wire system extensions: strain insulators, jumpers, general linked assemblies from the structure library, bundles, non-linear stress-strain and creep in elements
- [ ] Structure loads output (attachment point loads per load case)
- [ ] Usage / utilisation reports

**Deliverable:** code-checked line design with structure loading reports.

## Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

- [ ] `ols-structures-fe`: 3D frame/truss FE analysis of lattice towers and poles (geometric non-linearity, guys) — standalone module
- [ ] Wood/steel/concrete pole libraries and checks
- [ ] LiDAR: LAS/LAZ/COPC import with octree streaming and point budget (billions of points at 60 fps)
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
