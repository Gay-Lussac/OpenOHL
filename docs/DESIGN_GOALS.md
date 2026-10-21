# Design goals and core requirements

OpenOHL aims to be **faster and more user-friendly** than existing overhead line design tools. This document records the requirements that every module must respect. Changes go through an ADR.

## 1. Performance first

- The computation core is written in a **compiled, memory-safe language (Rust)** — no interpreted code in hot paths.
- Python is a **scripting and plugin layer only**, never required for the core to run.
- The GUI renders on the GPU and never blocks on computation: calculations run on background threads and results stream in.
- Every performance-sensitive feature ships with a criterion benchmark (so far: units and the catenary). The CI workflow is started by hand only; a full run compares the benchmarks with the parent commit and fails when one is more than 1.5× slower.

### Performance targets (initial, to be refined)

| Operation | Target |
|---|---|
| Application cold start | < 2 s |
| Open a 500-structure / 100 km project (without point cloud) | < 2 s to first interactive frame |
| Re-solve one tension section after an edit (sag-tension, all weather cases) | < 10 ms (interactive while dragging) |
| Clearance check, full 100 km line, all weather cases | < 1 s |
| Finite-element wire system, one 20-span section, one load case | < 100 ms |
| 3D view with LiDAR (≥ 1 billion points, streamed) | ≥ 60 fps, first frame < 1 s |
| Change propagated to other users in a shared session (LAN) | < 100 ms |

### Time budgets and exit clauses

Nothing in OpenOHL may run forever or freeze the application. Every function that can take time must have an **estimated duration** and **exit clauses**, and when it stops early it must **say why**:

- **Iterative solvers** (catenary, change of state, FE, optimisation) always have a maximum iteration count *and* a convergence tolerance. Non-convergence returns an error with diagnostics (iterations done, last residual, inputs) — never an endless loop or a silently wrong result.
- **Long-running tasks** (clearance runs, LiDAR processing, exports, spotting) have a time budget, can be cancelled by the user at any time, and report progress.
- These rules are implemented once in `ohl_core::budget` (`Budget`, `CancelToken`, `Stop`), and every engine uses them.
- **Plugins** run with a time limit per call (default 5 s) and a memory cap (default 256 MiB); a runaway plugin is interrupted and reported (`PluginError::Timeout`) without affecting the rest of the application.
- **Validation cases** have a time limit (`max_seconds`, default 10 s) and their duration is recorded in the report.
- **CI jobs** (run by hand only) have a `timeout-minutes` limit; benchmarks use fixed measurement times.
- Budgets are derived from the performance targets above: when a target says "< 10 ms", the default time limit is set with a safety margin (e.g. 10×) so a limit being hit always indicates a real problem to investigate, not normal variation.

## 2. User-friendliness

- Direct manipulation: drag structures on the profile, wires and clearances update live.
- No modal dialogs for core workflows; property panels with immediate feedback.
- Every number shows its units; switch SI/US customary at any time.
- Every result is explainable: click a value → see the inputs, model and standard clause that produced it.
- Unlimited undo/redo, full project history.
- Sensible defaults and templates; a new user can produce a sag-tension table in minutes.
- Keyboard-driven workflows and a command palette for power users.

## 3. Loads on wires

In addition to self-weight, wind and ice, the core must support **additional loads applied on wires**, in every analysis (sag-tension, clearances, FE wire system, structure loads):

| Type | Unit | Examples |
|---|---|---|
| **Point (concentrated) load** at a position along the span | N (vector: vertical, transverse, longitudinal) | Aircraft warning markers, spacers, dampers, bird diverters, line hardware, line cart / worker, sleeves, signage |
| **Distributed load** over the full span or a sub-interval | N/m (vector) | Partial ice / ice shedding, partial wind, cable-mounted fibre (ADSS lashed), heating strips, de-icing equipment |

Requirements:
- Loads can be defined **per weather/load case** (e.g. worker only in maintenance case), and as **permanent** (present in every case).
- Positions defined by distance from either support, fraction of span, or station.
- Analytic treatment where possible (segmented catenary between point loads); general case through the FE wire solver.
- Loads affect creep/final conditions where physically relevant (permanent loads).
- Exposed in the plugin API so tools can generate loads (e.g. a marker-ball placement tool).

## 4. Collaborative multi-user editing ("server mode")

A model can be opened as a **shared session**, like a game server: one machine hosts the project, several users connect and work on it at the same time, seeing each other's changes live.

This is a **core** requirement, not a GUI feature, because it dictates how the data model works:

- **Authoritative server**: one process owns the model. Clients send *commands* (move structure, change tension, add point load…); the server validates, applies, and broadcasts the resulting *changes*.
- **Single-user mode is the same thing**: the desktop app runs an embedded in-process server. There is only one code path.
- **Command / operation log**: every edit is a serialisable command → gives undo/redo, full history, audit trail ("who changed this tension, when"), and replay for free.
- **Conflict handling**: fine-grained object versioning; optional soft locks on objects/sections being edited; presence (see where others are working).
- **Undo is per user**: each person undoes/redoes only their own changes (applied as inverse commands on top of the current state).
- **Admin rollback**: users with the admin right can roll back other users' changes — a single command, a revision, or everything after a point in time — like an admin on a game server.
- **Roles & permissions** (server-defined, like a Minecraft server's ops/permission levels):
  - Default roles, e.g. *viewer* (read-only), *designer* (edit), *checker/approver* (comment, approve, tag revisions), *admin* (rollback, manage users/roles, lock areas, change project settings, install plugins on the server).
  - Custom roles built from fine-grained permissions (per action type: edit structures, edit libraries, change design criteria, run exports, delete…).
  - Scope: per project, per area/chunk, per object type (e.g. only the lead engineer edits weather cases and design criteria).
  - Some actions are admin-only by default (rollback of others' work, deleting sections, changing project-wide criteria, kicking/banning users).
  - All permission changes and admin actions are recorded in the history.
- **Compute placement**: calculations can run on the server (shared results, thin clients) or locally on clients.
- **Transport**: binary protocol over WebSocket/QUIC, encrypted; works on LAN and Internet. A web client (browser) joins the same session.
- Offline work: a branch carried back in a bundle and merged ([ADR-0010](adr/0010-version-control.md)); automatic sync is a stretch goal.

## 5. Render distance, LOD and streaming

Large projects (hundreds of km, billions of LiDAR points) must open fast and stay smooth:

- **Spatial chunking**: the project world is divided into a fixed grid of map tiles/chunks (quadtree, like Minecraft chunks) — independent of the alignment, so branching lines, parallel circuits and substations work too. Only chunks near the viewpoint are loaded in memory and on the GPU.
- **Render distance** setting (user-adjustable), separately for terrain, point cloud, structures, wires and labels.
- **Level of Detail (LOD)**:
  - Terrain: quadtree tiles with progressively simplified meshes.
  - Point clouds: octree streaming (COPC/LAZ-based), point budget per frame.
  - Structures: several mesh LODs (full lattice → simplified → billboard/icon).
  - Wires: adaptive segment count by distance and curvature.
  - Labels/annotations: decluttering and distance fading.
- **Lazy loading**: opening a project loads metadata and the visible area first; the rest streams in the background.
- **Frustum and occlusion culling**, GPU instancing for repeated structures and hardware.
- The same chunking is used by the server to send clients only what they need.

The **engineering model is never simplified** by LOD: calculations always use full-fidelity data. LOD affects display and loading only.

## 6. Strings and linked assemblies

The core must support all common string types, and strings must be **linkable** to each other to form assemblies:

| Type | Behaviour |
|---|---|
| **Suspension string** | Single pinned string, free to swing transversely and longitudinally |
| **Two strings to one wire point** | From separate structure points to one yoke; restrains transverse swing; each can go slack in compression |
| **Tension / strain (dead-end)** | In-line with the conductor, carries full tension; single or multiple (double/triple) strings |
| **Post insulator** | Rigid/cantilever, line post (fixed or braced) |
| Others via plugins | Y-strings, horizontal V, braced post, 2-part strings, etc. |

**Linking**: strings connect through shared nodes (yoke plates, clamps, attachment points), e.g.:
- Double / quad strings through a yoke plate
- Jumper on a dead-end structure, supported by a jumper suspension string, connecting the two tension strings
- Tension strings and suspension strings acting together on angle/terminal structures
- Weights, counterweights, or spacers hung from a string node (uses the point loads of §3)

**Model**: an insulator assembly is a small **graph** of nodes (structure attachment points, yoke plates, wire attachment) and elements (insulator links, rigid links, pinned links, cable/jumper segments, tension-only elements). Each element has length, weight, wind area, strength and properties: rigid/pinned/tension-only.

- Simple cases (one string's swing, two strings' forces) are solved analytically for speed.
- The general linked case is solved by the FE wire system (`ohl-wirefe`), which treats the assembly as part of the wire model.
- Assemblies are reusable library items, and plugin-extensible.

## 7. Version control of projects (git-like)

Project changes must be tracked like source code in git, built on the command log of §4:

- **Commits / revisions**: group commands into named revisions with author, date and message ("Raised T-12 by 3 m for road crossing").
- **History**: browse every revision; see who changed what, when and why; open the project as it was at any revision.
- **Diff**: compare two revisions or two projects — added/removed/modified objects, changed parameters, and *engineering impact* (changed tensions, clearances, structure loads). Visual diff in plan/profile/3D.
- **Branches**: design alternatives and what-if scenarios as branches; compare branches; merge back with conflict resolution.
- **Tags**: mark issued revisions (e.g. "IFC Rev B — issued for construction").
- **Revert / cherry-pick** individual changes.
- **Export/import of history** for archiving and audit (regulatory traceability).
- Libraries stored as text files remain compatible with plain git.

Built on the command log and stable object IDs: revisions, history, diff with engineering impact, branches, merge, tags and revert are in the project file ([ADR-0010](adr/0010-version-control.md), [Versions](user-guide/versions.md)); visual diff, cherry-pick and history export are still to come.

## 8. Exports and interoperability

Exports and imports are **formats in a registry** (`ohl-io`: built-in importers and exporters, and `io-plugin` WebAssembly plugins), so new formats can be added without touching the core. Formats that must eventually be covered:

| Area | Formats |
|---|---|
| Geospatial / GIS | KML, **KMZ**, Shapefile, GeoJSON, GeoPackage |
| Engineering exchange | **XML** (LandXML and an open OHL XML schema), IFC, CSV |
| CAD / drafting | DXF, DWG-compatible output, SVG, PDF |
| Point clouds | LAS/LAZ/COPC (import and export of classified/as-built points) |
| Reports & tables | CSV, XLSX, PDF, HTML |

### Profile exports

Profiles must be exportable on their own, not only as part of drawing sheets:
- Ground profile (centreline and side profiles), wire profiles per weather case, clearance lines, structures and labels.
- Configurable horizontal/vertical scales and exaggeration, station ranges, sheet splitting.
- Formats: DXF, PDF, SVG, CSV/XML (numeric data), and images.

## 9. Routes, branches and shared structures

A line can have **several routes**. A route is an alignment (PIs) plus the ordered list of structures along it; it defines stationing, offsets and line angles for its structures, and is the basis for profile drawings and clearance reports.

- **A structure may belong to several routes.** A *Y structure* (branch point) is listed by the main route and by the branch; two systems carried by one structure that split onto two parallel structure lines and rejoin later are modelled as parallel routes sharing their end structures.
- **Wire sections are independent of routes**: they reference structures by id, so a section can run along one route, cross onto another at a shared structure, or follow a parallel route.
- Stationing, line angles and offsets are computed **per route**; a shared structure has one station per route it belongs to.
- Lines without explicit routes have one implicit route through all structures in file order (backwards compatible).
- **Alignments are independent of the line design.** The alignment drives the plan and profile drawings; its points are usually placed under the structures and **tied** to them (they follow when a structure moves), but each point can be moved, detached, inserted or removed on its own, so drawings and stationing do not have to change with every design change.

## 9a. Deleting objects

One rule for every deletion: when an object takes others with it, the deletion is **refused with the full list** of what would go (or be changed) and must be confirmed; then everything is deleted as **one logged command** with a note, undoable as a whole.

- A **structure** takes its attachments and strings, the sections through it (a section cannot lose a point), the jumpers at its attachments or at points of those sections; it is taken out of routes (routes left with fewer than two structures are deleted), and alignment points tied to it are detached (they stay in place).
- A **section** takes the jumpers at its points.
- Jumpers and routes take nothing else; the line settings cannot be deleted.
- Undo, redo and rollback replay exactly what was recorded (no new cascade), and restore objects in their original order.

## 10. Structure library (`.ohls`)

Each **structure type** (structure, pole, portal…) is a file of its own (`*.ohls`, "OHL structure", TOML) in a structure library directory (default the project's `Structures/` folder), so it can be reused, shared and version-controlled individually.

- **Local axes**: origin at the ground centre, x transverse, y along the line (ahead), z up.
- **Attachments**: set, phase, label, the insulator attachment point (local), the default insulator assembly (from the insulator library), and for strain sets the side (`ahead` / `back`).
- **Extensions**: body/leg extensions as named height offsets, so one type covers several heights.
- **Body outline** (optional): simple polylines for drawings and the 3D view (detailed meshes and LOD come with the renderer).
- **Placement in a line**: a structure instance references a type, a position, a rotation (local x axis direction) and an extension; global attachment points are computed. Per-attachment overrides remain possible.
- **Deriving types from an existing line**: attachment points of instances are transformed back to local coordinates and identical geometries are grouped into one type — a draft library from any imported line.
- Strength data: allowable spans and load trees (`[capacity]`, Methods 1 and 2), structure models and poles (`ohl-structures-fe`) — see [Structure libraries](user-guide/structure-library.md#capacity) and [Structure analysis](user-guide/structure-analysis.md).

### Simple structure files

How much string detail is useful depends on the structure. A structure file can therefore stand alone: a stick drawing (centre point, shaft, crossarms to the suspension points) and, per attachment, the string's length and a label at its top (insulator attachment point) and bottom (wire attachment point). Each structure is one file; a project keeps them in its `Structures` folder. Detailed assemblies (`.ohli`, linked strings) stay available when a structure needs them.

## 11. Insulator assembly library (`.ohli`)

Each **insulator assembly** is a file of its own (`*.ohli`, TOML) in an insulator library directory (default `libraries/insulators/`).

- **Basic kinds (now)**: `suspension` (one string: length, weight, axial stiffness, wind area), `v_string` (two legs: half width, leg length, leg weight, stiffness; legs tension-only), `strain` (dead-end: length, weight, stiffness; single or multiple strings).
- Structure-type attachments name their default assembly; a structure instance can override it per attachment.
- **Later (DESIGN_GOALS §6)**: general linked assemblies as a graph of nodes and elements — yokes, multiple strings, jumper support strings, counterweights — solved by `ohl-wirefe`.

## 12. Display and connections

- **Phase colours**: wires are displayed in colours by phase (and per circuit); the colour scheme is configurable per project (e.g. L1/L2/L3 conventions differ between utilities) and per section.
- **Jumpers at dead-ends**: jumpers connect the end of one section to the start of the next at a strain structure (or between circuits). They are modelled as short slack wire elements between the two wire points (with optional jumper support strings, §6), are drawn in 3D and included in clearance checks; their length or sag is a design input.

## 13. Clearances

- **Two measures, always**: **vertical** clearance (wire height above a target directly below it) and **radial** clearance (shortest 3D distance between wire and target). Each rule requires one or both.
- **Targets**: the terrain surface (TIN); LiDAR / survey points selected by class or by class role (§ LiDAR class schemes) — vegetation, buildings, roads, water, objects…; wires of **other lines** (crossings, parallel lines); and other wires of the **same line** — within the same span (phase to phase, circuit to circuit) and between spans (e.g. a branch passing under the main line).
- **Rules are data** (`.ohlr` files): a rule names its target and required distances; a later tool (GUI or plugin) edits them per LiDAR class, per crossing, per design code. Design codes (EN 50341, Swiss LeV, NESC…) are plugins that produce rule sets.
- **Per weather case**, and as **envelopes** over several cases (hot, iced, blown-out wires), including different cases for the two wires of a wire-to-wire check (e.g. upper wire iced, lower wire cold).
- **Exclusion zones**: parts of a wire near supports, terminations and cable transitions (where wires converge and come down to the ground by design) can be excluded from a rule, e.g. within a distance of the attachment points.
- Results per span and rule: smallest vertical and radial clearance with their locations, number of violating points or places; marked in the viewer and exported (CSV, drawings).
