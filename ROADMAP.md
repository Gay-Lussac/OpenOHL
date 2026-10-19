# Roadmap

*Step back, 2026-10-18.* This page has two parts. **Part 1** is the
roadmap to discuss: where things stand, the open points waiting on a
decision, what is unfinished or approximate, and the proposed stages.
**Part 2** is the detailed record of what is built, by phase.

**What the marks mean:** `[x]` = built and in active development, **not
yet approved by the owner** (nothing is approved yet); `[ ]` = not built.
An item the owner approves will be marked **✓ approved** with the date.
Everything is pre-alpha: results must be verified by a qualified engineer.

**How to comment:** write under the `> **Owner:**` line of any item (one
per item). Leave the rest as it is; it is rewritten from your comments.
Items that touch private data (the Swiss plugin, the existing lines) are in
`local-data/ROADMAP-private.md`, in the same form.

Guiding principles:
- **Quality first, then speed.** Speed work may only simplify what is
  drawn, never a computed number.
- **Headless engine first, GUI second.** Every calculation usable from the
  command line and covered by validation cases before it gets a user
  interface.
- **Plugins stay independent of the core**; limits like string ratings are
  data that plugins may check.
- **Multi-user from day one**: every edit is a command applied by a session.

---

# Part 1 — Roadmap to discuss

## Where it stands

- **Two applications on one engine**: **OHL-Design** (`ohl-design`: the
  line, sag-tension, clearances, layout, drawings) and **OHL-Structure**
  (`ohl-structure`: pylons and their loading), with the `ohl` command line.
  Project renamed OpenOHL (formerly OLS CAD).
- Phases 0 to 5 of Part 2 are built as a base, all in active development (none approved yet); the structure work
  (levels 1–3, poles, extensions, generators, loads per placed structure,
  FE with joint stabilisation and tension-only members, section library,
  exports) is in place with the limits listed below.
- 69 validation cases pass; the private Swiss plugin and the importers of
  another program's files live outside the public code.

### Just finished (2026-10-18)
- **Start-user improvements** in both apps: the review's quick wins, a
  **Next steps** page, the **starter template project**
  (`examples/starter/`: invented libraries, no line), **Open the example
  (copy)**, clearance windows that always have a rule (its distance to
  set) with an **Add a rule** helper, tool windows that can be docked,
  Help › User guide / Start from scratch / About in both apps.
- **Icons in the apps**: window icons, tool icons in the hotbar,
  inventory and menus, the OpenOHL mark on the start pages and in About.
- **Spring reactions**: a sprung joint now reports the spring's force
  (models without springs unchanged; two new validation cases, 69 in all).

Nothing is running now.

> **Owner:**

## Open points — waiting on a decision

### O1. Tutorial "Start from scratch"
A tutorial of about 10 short chapters on the starter template: route,
alignment, first basic structures, strings on the structures, the cable
file and stringing, jumpers; then results, checks, OHL-Structure, saving.
Proposed to start once the starter template has landed.
> **Owner:** Later: the tutorial comes once most features are done, just
> before a release. Other work goes first.

### O2. Rename the project folder on disk
`…/Claude/OLS CAD` → `…/Claude/OpenOHL`, as the last step when nothing is
running and the apps are closed (the agents' worktrees and the saved
session notes follow the path).
> **Owner:** Done 2026-10-18 (folder, repository and website renamed;
> formats, crates, plugins and local data renamed ols → ohl).

### O3. Usability review — items not yet decided
From the review of 2026-10-18 (quick wins are being done now):
- **3D and Plan side by side** by default (today Plan is a tab behind 3D).
- **One name per object** across both apps and the docs (string /
  insulator / attachment point / structure point …), with a glossary.
- **"String all sets…"**: one command to string every set along an
  alignment or from one structure to another (preview, undo).
- **Option "tools act on left-click"** (today a right-click uses a tool;
  left-click selects). Default unchanged.
- **A searchable coordinate-system picker** (EPSG list).
> **Owner:** (2026-10-19)
> 1. 3D and Plan: leave as is, the layout is fine.
> 2. One name per object: yes, same object same name everywhere; unify.
> 3. "String all sets…": only if it is robust (how does it know which set
>    goes where?); otherwise not. Decided: not done (matching sets by
>    number does not work between suspension and tension structures).
> 4. Tools on left-click option: go ahead.
> 5. Searchable coordinate-system picker: yes.

### O4. Code checks of members and joints (e.g. SIA 263)
A plugin interface for code checks: what the core hands over (member
forces, buckling lengths, sections, joint details), what comes back
(usage per member and joint, formulas). To be agreed before it is built.
> **Owner:** (2026-10-19) Can be done in parallel with other work. First
> code: SIA 263 for steel. Members first, then joints. The theory of the
> checks goes in a theory page of the book.

### O5. Tension-only parts that stay nearly free
Today: kept active and flagged *approximate* (see U1). Options when this is
treated: a very low compression stiffness instead of tension-only (your
idea), a damped or energy-based active-set update, and telling the user
which part is nearly free.
> **Owner:** (2026-10-19) What matters: it always solves, close to
> reality. In reality such a member takes a very light compression. Also
> consider treating tension-only members as cables. The complex
> active-set update can go on the list for later.
> Decided: a tension-only member carries compression up to its own buckling
> load (always solves; members at that load shown in the results). Not as
> sagging cables. The complex active-set update: later.

### O6. Clearances to the structure body (levels 2 and 3)
None at level 1. The tests for levels 2 and 3 are still to be defined with
you (what is checked, against which part of the body, in which cases).
> **Owner:** (2026-10-19)
> 1. What is checked: the conductors, and **basic volumes on the strings**:
>    spheres, cylinders, boxes and cones, each **grounded**, **insulated**
>    or **electrified**; distances are computed from these as well as from
>    the conductors. Each shape is placed along the string's length, with
>    its rotations (degrees of freedom) **free or blocked** to mimic real
>    strings; the freedoms are along the string's own axis, so they do not
>    depend on how the string is defined in the pylon. For double or triple
>    strings: the attachment point of each, the configuration and the
>    spacing.
> 2. Against every member of the body.
> 3. Cases as for the other clearances: weather cases chosen, wind stepped.
> 4. Distances rule based; a rule may give a different distance with wind
>    than in still air.

**Proposed details for O6 (to confirm):**
- **The shapes belong to the string type** (the insulator library), so
  every string of that type has them; a structure may override one.
- **Position along the string** from its top (fraction or metres), size,
  and an offset from the string's axis (an arcing horn beside it).
- **Electrical state:** *grounded* = at the structure's potential (top
  fittings, tower-side horn); *electrified* = at the line's potential
  (clamp, conductor-side horn, corona ring, yoke at the bottom);
  *insulated* = the insulator itself (its length counts as insulation, not
  as a clearance). Checked: electrified shapes and conductors against
  grounded shapes and every body member.
- **Freedoms** in the string's own axes: swing across the line, swing
  along it, twist about the axis; each free or blocked.
- **Several strings in one assembly** (double, triple): number,
  arrangement (side by side across the line, one behind the other along
  it, triangle), spacing, and the yoke plate as a shape.
- **The members' real size:** distance to the member's surface (from its
  section's outer dimensions), not to its centre line.
- **Jumpers** at tension structures checked against the body too.
- **Results:** the smallest distance per rule and case, which shape and
  which member, shown in the 3D view; levels 2 and 3 only (level 1 has no
  body).
> Details confirmed by the owner (2026-10-19).

### O7. Structure–line interaction, levels 3 and 4
Level 3: a stiffness matrix at each structure's suspension points from
level 3 structures. Level 4: structures and line iterated until both
converge. Order and timing to decide.
> **Owner:** (2026-10-19) Straight to level 4: level 3 is a simplified
> version of 4 and can easily be derived from it afterwards.
> Also needed (possibly with O7): **anchoring (guy) cables** in the
> structures.

### O8. Exports tried in other programs
IFC4, IFC2x3 and SAF are written from the public specifications but have
not been imported into the target programs yet. A test file opened by
someone with those programs would confirm them.
> **Owner:** (2026-10-19) Both: check the files now with free open-source
> tools (IfcOpenShell for IFC, the SAF schema), and have a test file opened
> in the real programs when someone can.

### O9. Design-code loads still to come
Security / broken-wire and construction loads, and code-specific wind on
structures, in the design-code plugins.
> **Owner:** (2026-10-19) All three, each in its respective design-code
> plugin. The core gets a **broken-wire case** type (the section solved
> with the wire gone) for the plugins to use: approved.

### O10. A pole with both `solid` and a library section
Today *solid* wins (the section still gives elasticity and density).
Alternative: refuse the combination.
> **Owner:** (2026-10-19) Poles need their own user-defined volumes: some
> shapes fall outside the standard library.

**Proposed for O10 (to confirm):** a pole's shape is either a library
section or its **own shape**, never both (the combination is refused with a
clear message). Its own shape is given along the height in segments, each
with a cross-section (circle, polygon with n sides, rectangle, or a drawn
outline), solid or hollow (wall thickness), tapering from bottom to top;
area, inertia, section modulus, weight and wind area are computed from that
geometry, and the material (steel, concrete, wood) is chosen per pole.
> Confirmed by the owner (2026-10-19).

### O11. Validation report
Phase 1 validation against published examples (CIGRE TB 324, textbooks)
is still open; a full validation report is a goal for 1.0. Needs
published reference data (no tables typed from memory).
> **Owner:** (2026-10-19) Later: the owner will say when reference data is
> available.

## Unfinished or approximate (known limitations)

- **U1. Tension-only sets that do not settle** are solved with the
  flipping members kept active; the result is marked *approximate*. Also:
  with a slightly kinked tie, the one-tie rule can hide compression in one
  half. *To be treated later; information may be missing at that point.*
- **U2. Imported lattice towers**: many towers imported from another program's export still fail
  as mechanisms, mostly where a load acts in a direction no member holds;
  the stabilisation reports them. Needs a look at how such joints are
  modelled.
- **U3. Profile sheets**: a sheet break can still fall at a structure that
  stands off the alignment. *Built 2026-10-19, in active development (not
  approved yet)*: sheets break only at structures of the run along the
  alignment, at least 2 mm on paper from what is drawn of every other
  structure (away ones included); the break moves back to the last free
  structure, or the sheet runs on to the next free one with a warning.
  Profile first, sheets on top (owner, 2026-10-19): the app's profile view
  stays continuous; sheet format, scales and breaks belong to the drawing
  exports. A sheet preview in the app is not built yet.
- **U4. OHL-Structure**: the Results tab solves the structure as written
  (not on the chosen extension); no pole check in the app; wind bands not
  raised by an extension; no structure report yet.
- **U5. Section library**: sections typed in OHL-Structure, shielding and
  code drag factors still to come.
- **U6. Body generators**: no redundants, plan bracing, or several points
  on one crossarm yet.
- **U7. Structure screen in OHL-Design**: drag handles for points in the
  line's 3D view built (in active development, not approved yet): points
  and wire points of the structure being edited, held to an axis or plane
  of its axes, snapped, one command each; a point of the type becomes the
  structure's own. Joints are shown but not dragged (the body is edited
  in OHL-Structure; a placed structure has no joints of its own).
- **U8. Level 2 and 3 members**: now drawn (built, in active development,
  not approved yet) in the line's 3D view — as lines, and close by as
  solids of their library profile; a member clicked selects its structure
  and, in the open structure screen, the member — and on the plan and
  profile sheets on their own layer `OHL_MEMBERS` instead of the symbol.
  Left: members with a `[[section]]` table of their own (no profile) stay
  lines; no GPU instancing (the geometry is shared per type and moved on
  the CPU); poles at levels 2 and 3 are still drawn as a mast line.

> **Owner:**

## Proposed stages (to discuss)

1. **Finish and settle** — land the three running pieces; the tutorial
   (O1); the folder rename (O2); docs checked against the apps.
2. **Usability, second round** — the items of O3 you choose; screenshots
   for the docs generated by a test so they stay current.
3. **Structures checked** — the code-check interface (O4) and a first code;
   clearances to the body (O6); OHL-Structure's open points (U4–U6).
4. **Line and structures together** — interaction levels 3 and 4 (O7).
5. **Towards 1.0** — validation report (O11), stable plugin SDK, plugin
   registry, translations (Phase 6 in Part 2).
6. **Further ahead** — OHL-Foundations, then OHL-Substations (end of Part 2).

> **Owner:**

---

# Part 2 — Detailed record

The phases below record what is built. Phases map to GitHub milestones;
each bullet becomes one or more issues labelled by module (`module:*`).

## Phase 0 — Foundations
*Goal: a project people can contribute to.*

- [x] Choose license (see ADR-0001)
- [x] Choose tech stack: language, 3D engine (ADR-0002) — GUI toolkit deferred to Phase 3
- [x] Define the plugin / extension mechanism (ADR-0003)
- [x] Define the open project file format (ADR-0004)
- [x] Define the collaboration / session model (ADR-0005)
- [x] Define the rendering, LOD & streaming strategy (ADR-0006)
- [x] Cargo workspace layout, CI (clippy, tests, validation cases, docs; macOS, Windows and benchmarks in a full run), started by hand only
- [x] Python bindings skeleton (`ohl-py`, PyO3) and WASM plugin host skeleton
- [x] Performance benchmark harness (criterion) with a regression gate in a full CI run (more than 1.5× slower than the parent commit fails)
- [x] Units system (SI internally, US customary + SI at the I/O boundary)
- [x] Documentation site skeleton (theory manual + API reference)
- [x] Validation/benchmark framework (case format + reference results + tolerance reporting)
- [x] Contribution guide, code of conduct, governance, clean-room policy
- [x] Exit clauses: `ohl_core::budget` (iteration/time limits, cancellation), plugin time & memory limits, validation case time limits, CI job timeouts

**Deliverable:** empty-but-wired repo, CI green, ADRs accepted.

## Phase 1 — Calculation core (headless)
*Goal: trustworthy single-section sag-tension engine.*

- [x] `ohl-cables`: conductor/cable library — area, diameter, weight, RTS, modulus, thermal coefficients, bimetallic (core/outer) properties
- [x] Stress-strain models: linear elastic, polynomial (initial / final / creep curves, per CIGRE/Aluminum Association practice), bimetallic split, aluminium compression limit
- [x] Creep models: creep-curve method (10-year curve) — *time-based / equivalent-temperature variants still open*
- [x] `ohl-sagtension`: exact catenary, parabola approximation, inclined spans, sag/tension/length/slack
- [x] Change-of-state equation; initial / final-after-creep / final-after-load conditions
- [x] Ruling-span (equivalent span) method for tension sections, plus exact equal-H section solution
- [x] Weather cases (temperature, wind, ice, combined) and tension limit criteria (% RTS, catenary constant, H/w); ice as radial thickness + density or as a load per length (density → wind diameter); case files (`.ohlwc`) with tags, cases chosen per run
- [x] `ohl-wireloads`: additional loads on wires — point loads (N, 3D vector) and distributed loads (N/m, full span or sub-interval), permanent or per load case
- [x] Segmented-catenary solution for spans with point loads / partial distributed loads *(vertical plane; transverse components → FE wire model, Phase 4)*
- [x] Stringing charts output (CSV; PDF later with drafting)
- [x] CLI: `ohl sagtension job.toml` (report in metric or US units, CSV chart)
- [x] Performance: single section re-solve < 10 ms *(~60 µs for 20 spans; loaded span change of state ~1 ms)*
- [ ] Validation: CIGRE TB 324 examples, textbook cases, hand calcs

**Deliverable:** `ohl` CLI + Rust library + Python bindings, reproducible sag-tension tables validated against published references.

## Phase 2 — Line model & terrain
*Goal: a real line described in data, with a ground profile.*

- [x] Line data model (`ohl-line`, `.ohl.json`): alignment, structures, attachment sets (insulator + wire points), wire sections, stringing, point loads (stations and offsets per route: see Routes below)
- [x] `ohl-session`: project file (`.ohl`), validated commands, operation log, per-user undo/redo (content-based conflicts), admin rollback, roles, dirty tracking with **incremental re-solving** (`ohl-line::incremental`, `ohl_session::Live`: only the touched sections, identical to a full solve), `.ohl.json` import/export
- [x] Point clouds in the project file (`ohl-pointcloud`, ADR-0007): import from LAS/LAZ, chunk × class blocks, point-source interface (database link as a plugin later)
- [x] Spatial chunk index (ADR-0008): fixed quadtree tile grid (`ohl_core::grid`), loose home tiles, index in the project file kept up to date by every command, area/tile queries (`ohl project tiles`, `ohl project query`), point blocks selected by area and counted per tile (basis for streaming and LOD)
- [x] Routes (DESIGN_GOALS §9): several alignments per line, structures shared between routes (Y pylons, parallel routes rejoining), stationing, offsets and line angles per route
- [x] Coordinate systems (`ohl-crs`, pure Rust proj4rs + EPSG registry): points and whole lines (`ohl crs`), LV95 ↔ WGS84 matching swisstopo REFRAME to < 1 mm *(grid-based datum shifts via a PROJ plugin later)*
- [x] `ohl-terrain`: survey point import (CSV/XYZ, named columns incl. PNEZD), feature codes, breaklines, constrained-Delaunay TIN, ground heights, exact profiles along segments (route centreline and side profiles in the drawings)
- [x] Structure library v1 (`.ohls`, DESIGN_GOALS §10): one file per type, local attachment points, default insulator per set, extensions, placement (position, rotation, extension), derive types from an existing line
- [x] Insulator library basics (`.ohli`, DESIGN_GOALS §11): suspension, V-string, strain assemblies used by the solver (V-strings solved in lines with tension-only legs and slack-leg reporting)
- [ ] *(deferred until 3D pylons)* Insulator assembly model (graph of nodes and elements): suspension (I), V-string, tension/strain (single & multiple), post; **linked strings** via shared nodes (yokes, jumpers, jumper support strings) — see DESIGN_GOALS §6
- [x] Multi-span sections solved in 3D (FE with suspension strings, wind per span, point loads); `ohl line` with 3D HTML viewer and CSV exports — compared with an existing line exported from another program (930 spans, median 1.1 cm)
- [x] Insulator behaviour in the 3D solve: I-string swing, V-string leg forces & slack detection, strain assembly geometry *(from the FE solve; closed-form checks still to add as validation cases)*
- [x] `ohl-clearance` v1: vertical ground clearance of every solved span per weather case, violations in `ohl line --terrain --clearance` and in the viewer
- [x] `ohl-clearance` v2 (DESIGN_GOALS §13): vertical **and** radial clearances; rule files (`.ohlr`) per target — ground, LiDAR classes/roles, other lines, other wires of the same line (same span and between spans)
- [x] Clearance envelopes over several weather cases (lowest clearance and governing case per wire and rule), different cases per wire in wire-to-wire checks (`case_pairs`), rules limited to cases; sheets draw every case
- [x] Exclusion zones in clearance rules: near the wire ends (`exclude_near_ends`) and spans at given structures (`exclude_spans_at`, e.g. cable terminations)
- [x] Electrical systems (`systems`: AC/DC, voltage, frequency, phase angles or pole polarities) assigned once per conductor path (sections joined by jumpers); clearances by voltage — tables or formulas per rule (so per LiDAR class group), pair voltage higher / sum / actual difference between conductors; unknown voltages refused before checking
- [x] Bundled conductors: `bundle` subconductors on a circle with `bundle_spacing` (e.g. 4 × 400 mm) and optional rotation; insulator weight shared by the subconductors in the solve; clearances to the nearest subconductor
- [x] Electrical cable data in the wire library (`[cable.electrical]`: AC resistance by temperature, DC resistance, geometric mean radius, emissivity, solar absorptivity; resistance units) — filled by a local importer from another program's files until an importer is decided
- [ ] Rule editor tool (per LiDAR class)
- [x] Plan & profile views per route (`ohl-drawing`, `ohl line --drawings DIR`): straightened plan + profile with ground, side profiles, clearance line, structures, wires by phase, violation markers — SVG and profile CSV — sheets on A0–A4/custom formats split at structures (never at one away from the alignment nor within 2 mm on paper of another structure; with no free break the sheet runs on, warned), title block with logos, separate plan scale across the line *(DXF, PDF, PNG and XML since Phase 5, `ohl drawing`; layout templates as plugins still to come)*
- [x] Exporter/importer plugin interface (first formats: CSV, KML/KMZ; GeoJSON example plugin)
- [x] Display: phase colours per circuit/phase, configurable per project (by phase label) and per section, legend in the viewer (DESIGN_GOALS §12)
- [x] Jumpers connecting sections at dead-ends or taps (slack catenary between solved clamps, length or sag input, optional support string, weather loads, ground clearance; DESIGN_GOALS §12) *(clearance to the structure body still to come)*

**Deliverable:** load a survey + structure positions, compute wire positions and ground clearance violations from CLI.

## Phase 3 — Desktop application MVP
*Goal: engineers can build and inspect a line interactively.*

- [x] Application shell (`ohl-design`, egui + wgpu, ADR-0009): background loading and solving, dockable views sharing selection and weather case; edits go through `ohl-session` commands
- [x] `ohl-render` basics: wgpu lines (pixel widths), points, shaded surfaces with depth and anti-aliasing; origin-relative precision; orbit camera; headless images
- [x] `ohl-render`: chunks on the project grid streamed in and out by distance, render distance per layer, three levels of detail (terrain grids, LiDAR shares, structure simplification, thinned wires and bundles), frustum culling, a level-of-detail table per layer in the app
- [ ] `ohl-render`: GPU instancing of structures and hardware, occlusion culling, wires tessellated on the GPU from catenary parameters, structure mesh LODs from the library, labels
- [x] Plan view, profile view, 3D view with synchronised selection; properties of structures and spans in every weather case; layers; LiDAR points by class role and ground from LiDAR
- [x] Live update while editing (Move window: the shown case re-solves the touched sections as values change; one command on Apply)
- [x] Tools as a Minecraft-style hotbar (1–9, wheel when captured) with an inventory (E, drag and drop), remembered
- [x] 3D navigation: fly with W A S D, Space/Shift up/down, Ctrl faster; control schemes as data (Orbit; Minecraft creative with captured mouse, crosshair selection, Escape to free) — more schemes and editing later
- [ ] Command palette, keyboard workflows, explainable results (click value → inputs & references)
- [x] Editing through the session in the app: move (drag or typed position) and delete structures (confirmation listing what goes with them), undo/redo, incremental re-solve of every case
- [x] Structures and strings in the app: Add structure (on the ground, type from the library), Add string, Edit (structure, string in the structure's axes, override of a type's string) and Delete (structures, strings, sections, jumpers) tools, one session command each *(spotting by hand on the profile still open; automatic spotting: Phase 5)*
- [x] Wire stringing: String wire tool (strings in order → section window: cable, circuit, system, bundle), sections edited and strung again through new points; sagged to a tension or a sag (ruling span or chosen span); status in the blocks (rolling: equal tension in all spans in every case, default) or clipped in (unstressed lengths per span) *(tension criteria by weather case still open)*
- [x] Clearance violation display: Clearances tab (rule file, violations of the shown case re-checked after edits in the background, click to go there), red marks in 3D and plan *(reports: `ohl line --clearance-csv`)*
- [x] Library editors (wires, insulators, structure types, weather cases): fields with units, known fields to add, checked as typed, comments kept; files browsed or created anywhere (system file dialog) and used for the model; the model reads its libraries again on save
- [x] Menu bar (File, Edit with Snap, View pages and layers, Lines, Tools, Checks on demand, Terrain, Structures and Sections with their libraries and tables, Criteria, Parameters, Plugins, Help) and a customizable quick bar; pages closable and reopened from View
- [x] Table editing: structures, sections, strings, jumpers, electrical systems, alignment points, weather cases (written to the case file) — every value typed in, rows added and deleted (dependents confirmed), renames followed by what refers to them, section targets (tension or sag in its display case), per-section display weather case; one command per Apply, sorting, filter *(paste from spreadsheets still open)*
- [x] Alignment points drawn in 3D and plan, selected, edited, deleted and added (tool and table); the profile follows
- [x] Project lines: frozen states of the line (as built, as planned) saved in the project, shown next to the working line in 3D and plan, made the working line (one undoable command) *(in the profile and in drawings still open)*
- [x] Add jumper tool (missing jumpers at a structure, or between two strings); ⌘S / Ctrl+S saves
- [x] Jumpers held by their clamps: bending stiffness (estimated from the cable or set) and the angle each end leaves its clamp at; solved as an inextensible elastic rod; project lines drawn like the working line (bundles, yokes, strings, jumpers)
- [x] Jumpers held by support clamps between their ends: each clamp on an I or a V of a structure's strings, free to swing and turn, the jumper straight through it (a rigid piece of the rod, also with EI = 0); tension-only strings (a leg goes slack in the wind); edited in the jumper's window, shown in the tables, properties and 3D view, written to OHL XML; their strings' pull in the structure loads; a clamp may also hang on a string below another string (e.g. a V with a string below, `chained_top`)
- [x] Strain assemblies always free members (move with the weather, in the calculation); weights of geometry-only strings estimated from their data position in the stringing state
- [x] Libraries (wires, structure types, insulators, weather cases) kept in the project file; export to files to share or compare; Terrain / Structures / Sections menus; LiDAR classes shown; table defaults shown
- [x] Section calculations: Initial / Creep / Load / Max Sag × RS / FE (Sections table, section editor); Criteria › Creep-Stretch (weather cases for final after creep and after load); cable models nonlinear, linear with creep ∝ creep-case tension, linear with a temperature increase; bimetallic curves per material in the wire library editor
- [x] Projects from other programs: `project-plugin` WIT world (line + library files + notes), File › New project from another program, `ohl project import-from` with clip-in; example plugin `project-bundle`
- [x] Terrain surfaces (TIN) in the project: several, the active one is the ground, others shown in colour; import LandXML / OBJ / survey points and terrain plugins (`terrain-plugin` world, e.g. ESRI ASCII grids), build from LiDAR ground classes, export LandXML / OBJ; `ohl surface`
- [x] Plugin manager UI: plugins installed and found in folders, what they provide, why one cannot be used, on/off, install, remove; their formats in File › Import into the line / Export (run on a worker)
- [x] Python scripting console inside the application: embedded interpreter on its own thread, the `ohl` module reads the line and results and edits through session commands (undoable), Stop and a time limit, plugin scripts
- [x] Points on the wires and the objects on them (one object per point from an object library: spacers, spreads, …), jumpers from a point or a tap; spacers in the solve
- [x] View performance: a frame benchmark; what made a long line lag found and fixed
- [x] **Server mode (pre-alpha)** (`ohl-server`, `ohl serve`, File › Host / Join): host a project, multiple users connect over LAN/Internet (QUIC, TLS pinned by the invitation), live change broadcast to replicas, presence (People page, plan markers), soft locks (Move window), per-user undo, roles & permissions (personal invitations; admins set roles), admin rollback (Edit › History) *(still open: optimistic prediction, chunk-based interest management, a WebSocket fallback for the application, OIDC)*

**Deliverable:** alpha release of the desktop app, including shared multi-user sessions.

## Phase 4 — Loading & structural checks
*Goal: design-code compliant loading.*

*Status: built as a base, in active development (not approved yet). Each item says what is simplified or left for later.*

- [x] `ohl-loads`: load case framework (weather × load factors × wire condition), `.ohllc` files kept in the project, edited in the app
- [x] Design code plugins: NESC (ANSI C2), CSA C22.3, IEC 60826, EN 50341 (+ national NNAs), ASCE MOP 74, GO 95 — each as a separate plugin (a Swiss LeV plugin exists separately, not part of the public code) *(base: `code-plugin` world, `plugins/codes/*`, `ohl code`, Criteria › Design code; each code's method with every value from the user's parameter file — no code tables shipped; security / broken-wire and construction loads, NESC constant k and code-specific wind on structures still to come)*
- [x] Structure capacity Method 1: allowable spans (wind/weight span, line angle) — `[capacity.spans]` in the structure types, interpolated by line angle, minimum weight span (uplift)
- [x] Method 2: allowable load trees per attachment (vertical, uplift, transverse, longitudinal; linear interaction optional) against factored loads × strength factor *(interaction diagrams of the whole structure later)*
- [x] Finite-element wire system (SAPS-style, `ohl-wirefe`): exact elastic catenary elements, I- and V-strings (tension-only legs), 3D wind blowout, insulator swing, broken span, point loads, unequal spans *(delivered early, in Phase 1)*
- [x] FE wire system extensions: strain insulators (free strain members), jumpers (solved between the section ends), bundles, non-linear stress-strain and creep in elements (secant iteration on the curves, permanent set from the material state; FE and RS agree within 0.02 % in tension on a fixed-end span) *(general linked assemblies from the structure library wait for the insulator assembly graph, with 3D pylons)*
- [x] Structure loads output (attachment point loads per load case): line and structure axes, unfactored and factored; Structure loads page, CSV (`ohl line --load-cases --loads-csv`) — wind on the structures by height band (`[[wind.band]]`), the strings holding jumpers' support clamps *(the pull of jumpers on their end clamps still open)*
- [x] Usage / utilisation reports: Structure loads › Usage (worst first, governing case, colours), `ohl line --usage-csv`, structures coloured by usage in the plan and 3D views with a legend; a structure whose wires did not solve is **incomplete** (never a pass), one whose FE check kept tension-only members active is **approximate**
- [x] String limits: optional maximum load and swing angles on strings, kept for plugins (no core check)
- [x] Clearance presets from design-code plugins for the Vertical and Radial clearances windows

**Deliverable:** code-checked line design with structure loading reports.

## Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

*Status: built as a base, in active development (not approved yet). Each item says what is simplified or left for later.*

- [x] `ohl-structures-fe`: 3D frame/truss FE analysis of lattice towers and poles (geometric non-linearity, guys) — standalone module. *Base:* `.ohlfe` models (truss, 3D Euler–Bernoulli frame, tension-only cables with pretension, supports, springs), first order or P-Delta (geometric stiffness, Newton iterations with exit clauses), skyline LDLᵀ with RCM ordering (1000 members in milliseconds), member usage (tension, compression with Euler buckling, linear N + M interaction; a member's own capacities and effective length, angle legs by structure directions, a dead-load factor on the self-weight), attachment nodes fed with each load case's factored loads (`fe` check in the structure usage, `ohl structures fe`, the Results tab of OHL-Structure). *Simplified / later:* no corotational large rotations, wind on the tower body only on profiles from the section library (no shielding, one drag factor), no code column curves or angle-section eccentricities, no member-load cases beyond self-weight and the load-case recipes, no 3D view of the model in OHL-Design
- [x] **Joint stabilisation**: towers drawn as trusses solve — joint directions no member stiffens (coplanar or collinear trusses, a frame chain free to spin) are held when the model would otherwise be a mechanism, and the force they take is reported; a load no member holds is refused; true mechanisms are named with the joints that move
- [x] **Tension-only members** (`kind = "tension"`, the "tension-only" members of tower programs): solved by active-set iterations (compressed members left out, stretched ones put back, first order then P-Delta); members on one line through a joint switch as one tie. *Unfinished — see Known limitations below:* a set that does not settle is solved with its flipping members kept active and the result flagged approximate

### Structure analysis — known limitations (unfinished)
- [ ] **Tension-only sets that do not settle** (recorded 2026-10-18). When the active-set iterations come back to a set they solved before, or take 100 solves, the members going taut and slack in turn are today **kept active as ordinary truss members**, the others iterated on, and the result is flagged: a note *"tension-only members kept active because the set did not settle: … — approximate; to be treated later"* (solution notes, structure usage notes, OHL-Structure's Results, `ohl structures fe`), those members marked *kept active (did not settle)* in what governs, and the structure's usage shown as **approximate** (never a plain pass). Owner's note: *we might be missing information at that point; it should be treated later.* Seen where a pair of diagonals, taken out, leaves a part nearly free (held only by a weak member or a joint slightly off a tie's line). Options to look at when this is treated: the owner's idea of **dropping tension-only for such members and giving them a very low compression stiffness instead** (which seemed to converge more), a damped or energy-based active-set update, and telling the user which part is nearly free
- [x] Wood/steel/concrete pole libraries and checks. *Base:* `.ohlp` poles (taper by diameter or circumference, fibre stress for wood, moment capacity at the ground line or by height, hollow wall), moment along the pole from the factored attachment loads (both directions, offsets) plus wind on the pole, P-Delta amplification from a tapered frame model (`pole` check, `ohl structures pole`). *Simplified / later:* one amplification factor for every height, no foundation/embedment check, no shear or deflection limits, no pole class tables (user data)
- [x] LiDAR basics (`ohl-lidar`, delivered early in Phase 2): LAS/LAZ tiles, class schemes with roles (`.ohlc`, any classification), ground in a corridor, TIN from LiDAR — matched a commercial TIN's span clearances to a median of 2 mm on a real line
- [x] LiDAR: COPC, octree streaming and point budget (billions of points at 60 fps) — *base*: COPC read directly as a point source (area/corridor/depth queries by octree node; also on `--pointcloud` for `ohl line` and the app), imported like LAZ, written on export (in memory), `ohl pointcloud info` shows its octree; stored clouds keep levels of detail per block (¼, 1/16, 1/64 of the points; schema 2, `ohl pointcloud octree` for older files, ADR-0007 amendment); the 3D view streams nodes on background threads by screen-space error under a per-user point budget (default 5 M) with an LRU GPU cache. Measured (M1 Max, synthetic 25 M points): 5 M points drawn at 16.5 ms a frame, 10 M at 32 ms; a view read in 0.1–0.2 s. Not measured with 10⁹ points (synthetic data only, extrapolated in the user guide); stored-cloud nodes are 256 m squares × 4 levels, not a spatial octree, so very large *areas* hit the budget at the coarsest level; COPC export holds the points in memory; COPC colours/GPS time not kept
- [x] LiDAR: classification, vegetation & encroachment analysis, as-built wire extraction/fitting — base: ground by progressive morphological filter + height classes (`ohl lidar classify`), vegetation encroachment with growth and fall-in per span, catenary fits per span with the survey temperature from the model (synthetic tests: sag within 2 cm, temperature within 2 °C); app windows. Still to come: buildings/wires/poles told apart from vegetation, leaning trees
- [x] Thermal rating: IEEE 738, CIGRE TB 601 / TB 207, max operating temperature from clearance — `ohl-thermal`: ampacity, temperature, transient, solar position; `.ohlta` ambient cases; per-section clearance-limited temperature (ground and point rules); `ohl thermal`, Checks › Thermal rating *(base: uniform conductor temperature, no radial gradient; no published TB 601 example in the validation yet)*
- [x] Optimum (automatic) structure spotting — cost-based optimisation: dynamic programming over candidate stations × types/extensions (ground clearance of a design catenary, allowable wind/weight spans, angle points, forbidden zones); `ohl spot`, Tools › Spot structures (proposal as a new line file) *(clearance under the centreline only, one tension for the route)*
- [x] Galloping ellipses, blowout, phase-to-phase / circuit-to-circuit clearances — base: ellipses by user coefficients (`.ohlg`; Davison / RUS / CIGRE TB 322 forms, no coefficients shipped), swept along the span, per span pair (`ohl clearance galloping`, app window); blown-out wires against the right of way (`ohl clearance row`); `circuits = same|different` on wire rules with case pairs
- [x] Drafting: plan & profile sheets, title blocks, DXF/DWG-compatible output, PDF (`ohl drawing`, File › Export drawings): one format-free drawing model written as SVG, R12 DXF (sheets in mm, plan in project coordinates, profile by station × exaggerated height), vector PDF (a page per sheet) and PNG *(base: no DWG written — the R12 DXF imports into DWG-based programs; DXF has ACI colours, no line weights or logos; PDF uses the standard Helvetica fonts; title block layout still fixed, templates as a plugin to come)*
- [x] Profile exports: ground/wire/clearance profiles with configurable scales and station ranges — DXF, PDF, SVG, CSV/XML, images (DESIGN_GOALS §8) — `--from`/`--to`, `--hscale 1:2000 --vscale 1:200`, `--profile-only`; profile XML documented in the user guide *(base: one weather case solid and the others dashed, as in the drawings)*
- [x] Bill of materials & cost estimation: structures by type/extension, insulator assemblies, cut lengths per cable (bundles, jumpers), spacers/dampers by rule, priced with a `.ohlcost` file (missing prices listed); `ohl bom` (text, CSV, JSON), Checks › Bill of materials
- [x] Collaboration (base, ADR-0010): edit grants per user or role limited to kinds, objects or a plan polygon, checked by the session for every command (so by the server too); review & approval of branches with protected branches; offline work as a branch carried back in a bundle and merged. Not yet: automatic sync of offline copies, custom roles, area grants drawn in the plan
- [x] **Git-like version control** of projects (base, ADR-0010): content-addressed revisions of the line and its libraries in the project file, history browser (Edit › Versions, `ohl project history`), diff field by field with engineering impact (touched sections solved on both sides: sag, tension, ground clearance), branches, three-way merge by object and field with conflicts settled per key, tags, revert; through the server's operations. Not yet: cherry-pick, visual diff in the views, history export for audit (DESIGN_GOALS §7)
- [x] Web client (WebGPU) joining server sessions — base: `ohl serve --web PORT` / File › Host this project (Web client), HTTPS + WebSocket with the same codes and roles; 3D (WebGPU, Canvas2D fallback), plan and profile of the line with wires solved on the server (one weather case), selection, presence, live operation log; designers move structures. Not yet: terrain/point clouds, weather choice, other edits
- [x] Interop plugins: XML (LandXML + open OHL XML schema), KML/KMZ, IFC, Shapefile/GeoJSON/GeoPackage, CSV, XLSX; import of common neutral formats from other tools (DESIGN_GOALS §8) — base, built into `ohl-io` (written from the public specifications, no native GIS library): OHL XML (whole line, lossless, `docs/schema/ohl-line.xsd`), LandXML alignments + CgPoints (export; import of points/alignments), GeoJSON / zipped Shapefile / GeoPackage layers (structures, routes, wires per case with tensions and sag), IFC4 (structures as proxies with stick geometry, spans as cable segments, property sets), XLSX report tables; structures imported from CSV (mapped columns), GeoJSON, Shapefile and LandXML points along a route, into a line or a project as one command. Left for later: shapefile `.prj` / GeoPackage WKT for projected systems, GeoPackage spatial index, IFC georeferencing (`IfcMapConversion`) and solid geometry, curves of LandXML alignments (read as chords); DXF and PDF are written by `ohl drawing`, not as `ohl-io` formats

**Deliverable:** beta release usable on real projects (with independent verification).

## Pylons — structures in their own axes, three levels of detail
*Before 1.0, agreed with the project owner (2026-10-10). A pylon is defined
in its own axes (x right of the line, y ahead, z up; 0;0 at the centre)
and placed in the line by a position, an angle and a height adjustment.*

- [x] Placement: the type's z = 0 may be any reference level (usually under the lowest crossarm); `foot` gives the level of its feet, so the placed pylon stands with its feet on the ground, raised by the structure's `height_adjust` (foundations); extensions lengthen the body below the reference level
- [x] One file per pylon (`.ohls`): joints and members in the type itself (sticks), level 3 adds sections, materials, supports and member kinds (truss / frame / cable / tension) to the same members; `.ohlfe` models still read. The level is **derived from the content** (1 stick figure → 2 lattice outline as soon as there are members → 3 as soon as every member has a section and a joint has a `support`; a structure standing only on springs stays at level 2 today); the marker follows
- [x] String points as joints or as a fraction along a member, so strings follow the members
- [x] Symmetry (optional, decided 2026-10-17): members and joints copied about x, y or both, to ease building — `[[symmetry]]`, edits mirrored, **Make symmetric** in OHL-Structure
- [x] Structure type workspace in the app: its own 3D view in local axes, tables (joints, members, points, strings, wire points), select / edit / drag with snaps, add members, string presets; used-by count; the line follows on save *(base: I and V presets — TAK and V + string by hand; ids renamed in Fields; drag handles in the line's 3D view built since 2026-10-19 (U7): points and wire points; joints shown only. Since 2026-10-17 OHL-Design's structure screen edits only the basics — points, strings, wire points — and the body and design are edited in OHL-Structure)*
- [x] **Body generators** (format version 3): legs and panels (single, X, K, diamond, Warren; horizontals), crossarms (triangular, rectangular), peak, copy a face — each writing ordinary joints and members; rebuild replacing only what a generator made; member groups with a section and kind per group *(not yet: redundants in the panels, plan bracing, a crossarm with several points on one arm)*
- [x] **Poles with levels and extension bodies**: a `[pole]` table in the structure file (outline by height, round or polygonal; a solid pole counts), its levels, the pole check reading it as a `.ohlp`; extension pieces and leg extensions in the FE model of each placed structure *(not yet: the pole check and extensions in OHL-Structure's FE results, wind bands raised by an extension)*
- [ ] Level 2 and 3 drawn with their members in 3D and on plan & profile sheets
- [x] **Replace structure** tool: a new structure put in place of another, with its strings; wire points (sets) that match are rewired automatically, for the others the user says which goes where; with no match the replacement still runs after the model shows what will be removed and the user confirms
- [ ] Tower geometry from another program's XML export (joints, members, sections) into levels 2 and 3 (local importer)
- [ ] Level 2 from LiDAR: a plugin (later); the classes holding pylons chosen by the user from all classes, as in the display options
- [ ] Clearances to the structure body: none for level 1 (LiDAR points only, separate topic); levels 2 and 3 computed — **the tests are to be defined with the project owner**
- [x] **Structure application OHL-Structure** (`ohl-structure`, decided 2026-10-17, [spec](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/structure-building.md)): a separate program on the same workspace crates, started from **Edit** on a structure in OHL-Design or by opening a `.ohls`, or on its own (start page, launcher, its own libraries and plugins); its tabs (geometry, body, design, load cases, results, use in the line), undo, what is missing for each level *(not yet: a report of a structure; the mechanism found by a trial solve among the advice)*
- [x] Levels as decided: 1 attachment points only (stick figure, not verifiable); 2 the real geometry (no code check); 3 every part and joint detail, computed (FE) and code-checked (e.g. SIA 263). **Poles** the same: 1 points without body, 2 outside lines, 3 detailed engineering (a wall thickness, or solid). `[[body]]` polylines give level 2 only in 3D. At levels 1 and 2 maximum loads only on the strings, never for the structure itself (its capacity only at level 3) *(the level derived from the content; the code check of level 3 waits for the code-check plugins below)*
- [x] A structure file as a pylon family: body and leg extensions chosen per position adapt it to the ground; each placed structure unique to its position (its own name, loads, model, results) — OHL-Structure started for a placed structure works on its own loads and extension
- [x] Structure picker window: table of structures (name, description, level, extensions, uses) with a filter and a preview, instead of the drop-downs
- [x] **Members of levels 2 and 3 drawn with the line** (U8): in the line's 3D view as placed by the line (position, orientation, extension, height adjustment), as lines and close by as solids of their library profile turned as the FE model turns them (`ohl_structures_fe::drawn`, the geometry made once per type and extension); a click on a member selects its structure and the member in the structure screen; on the plan and profile sheets (SVG, DXF, PDF, PNG) on the layer `OHL_MEMBERS` instead of the symbol, level 1 unchanged *(not yet: profiles of `[[section]]` tables, GPU instancing, poles' outlines)*
- [x] Weather and load cases at the suspension points from level 1, exported for a structure designed in another program — the load cases and weather cases the line sends to placed structures, as the load table (CSV)
- [x] **Load-case recipes** per structure (`[[load_case]]`, `.ohlslc` or CSV): which line value, factor, one-sided share, on which points or members, surface loads (wind, snow); evaluated by the core with the line's values at each placed structure; the loads in a table, editable (typed values kept over the evaluation), exported and imported
- [x] **Section library**: a project library kind the structures refer to (not copied), exported and imported *(base: `.ohlsec` profiles and materials with computed properties; a member's section from the library first, then the structure's tables; the FE check's member wind on the profiles, the wind bands kept out of the model; member areas for surface loads. Later: typed sections edited in OHL-Structure, shielding and code drag factors)*
- [x] Structure exports: IFC4 (member solids and the analysis model with load cases), IFC2x3 for steel-detailing programs, SAF 2.2 (xlsx) for analysis programs, CSV tables *(not yet: the imports tried in another program; other tower programs)*
- [ ] Code checks of members and joints as plugins (e.g. SIA 263), interface agreed with the project owner first
- [ ] **Fully integrated structure editors with a 3D preview** for every detail level (1 stick figure, 2 sticks of the pylon, 3 full design), all in one window: geometry, members, sections, materials, supports, strings and wire points, the type's use in the line, checks — lattice towers and **poles of wood, steel (and concrete)** alike *(base: OHL-Structure, with poles in Body › Pole and Design › Pole; still to come: typed sections and materials, checks per level, the use in the line)*

## Structure–line interaction — levels of computation
*Agreed with the project owner (2026-10-10), levels defined 2026-10-17 as is common in line design software: how far the line and its structures are computed together, chosen per run.*

- [x] **Level 1** — ruling span: sections sagged by the ruling span method on rigid structures
- [x] **Level 2** — FE of the wires on rigid structures (`ohl-wirefe`, what OpenOHL has today); the conductors' forces put on the structures, every structure checked once *(today's structure loads and usage)*
- [ ] **Level 3** — FE of the wires with a linear stiffness matrix at each structure's suspension points, derived from the structure (level 3 structures only)
- [ ] **Level 4** — the full solve: structures and line iterated until both converge (displacements and forces between passes below tolerances; exit clauses, damping, reports of what did not converge)
- [ ] The level chosen per run (app, CLI, load cases), shown with every result; a structure below level 3 is rigid at levels 3 and 4, and the result says so

## Server and version control — more capabilities
- [ ] **Server**: more editing through the web client (structures, strings, sections), terrain and point clouds in the web client, several projects per server, accounts and roles managed in the app, presence and locks per object, a server log and backups
- [ ] **Version control**: cherry-pick, visual diffs in the views, history export for audits, revision comparison of results (sag, clearances, loads), automatic sync of offline copies, custom roles, area grants drawn in the plan

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
- Performance benchmarks against the targets in DESIGN_GOALS.md (large lines: 500+ structures, LiDAR with 10⁹ points); regressions fail a full CI run
- UX reviews with practising line engineers
- Backwards compatibility of the project file format (versioned schema + migrations)

---

## Further ahead
*Two further applications on the same engine, added by the project owner (2026-10-18), in this order. Neither is started.*

1. [ ] **OHL-Foundations** — the foundations of the structures: foundation types per structure (pad and chimney, piles, rock anchors, pole embedment…), their data in their own library files, the loads the structure checks give (level 1 and up) on them, checks by the user's code parameters (uplift, compression, overturning, sliding), bill of materials and costs; the structure's `height_adjust` linked to the foundation's reveal
2. [ ] **OHL-Substations** — substations, where lines end: gantries, busbars and equipment

> **Owner:**
