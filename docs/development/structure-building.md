# Building structures from level 1 to level 3 (draft)

Status: **draft for discussion** (2026-10-17). Nothing here is built; every
point below is a proposal until the project owner decides it. The open
questions are at the end.

Asked by the user: the specification of the structure building interface
needed to take a structure type from level 1 to level 3, and to **compute
and verify a type on its own** — not only through a line.

Related: [Structure detail levels in checks](structure-detail.md) (the
rule for shape-dependent checks), [Structure libraries](../user-guide/structure-library.md),
[Structure analysis](../user-guide/structure-analysis.md), the ROADMAP
sections "Pylons — structures in their own axes, three levels of detail"
and "Structure–line interaction — levels of computation".

## Today

### The three levels, as the code has them

`ols_structures::Detail`, worked out by `StructureType::detail()`
(`crates/ols-structures/src/structures.rs`) from what the file holds; a
`detail = …` written in older files (`stated_detail`) is read and ignored.

| Level | `Detail` | Reached when | What it adds |
|---|---|---|---|
| 1 stick figure | `Stick` | always (no members, no `[[body]]`) | `[[point]]`, `[[wire]]`, `[[string]]` (or version 1 `[[attachment]]`), `height`, `foot`, `[[extension]]`. Drawn as a mast to `height` and a crossarm to every point. |
| 2 sticks of the pylon | `Outline` | any `[[member]]` or any `[[body]]` polyline | `[[node]]` joints and `[[member]]`s without sections, or `[[body]]` polylines; points may sit on a joint (`node`) or along a member (`member`, `at`). |
| 3 full design | `Design` | `model = "….olsfe"` named, **or** `missing_for_design()` empty: members exist, every member has a `section`, at least one joint has a `support` | `[[section]]`, `[[material]]`, member `kind` (truss / frame / cable), `k`, `reference`, `roll`, `pretension`, supports and springs, `[analysis]`, `[[load]]`. |

Two things the levels are **not**, though they are easy to assume:

- **Capacity is not a level.** `[capacity]` (Method 1 allowable spans,
  Method 2 load trees) and `[[wind.band]]` are read at any level; a
  level 1 type with a capacity is fully checked by Methods 1 and 2 in a
  line.
- **A pole is not a level.** A type naming a `pole` (`.olsp`) is checked
  by the pole check, but `detail()` does not look at `pole`: a pole type
  with no members reports level 1 although it is checkable.

### What exists per level

| | Level 1 | Level 2 | Level 3 |
|---|---|---|---|
| **Data** | points, strings, wire points, extensions, foot (`structures.rs`) | `TypeNode`, `TypeMember` (no section), `Outline` | sections and materials as raw `toml::Table`s in the type, parsed by `ols_structures_fe::Model::from_structure_type` (`crates/ols-structures-fe/src/model.rs`); or a separate `.olsfe` |
| **Built by** | pylon builder from `.olspt` templates (`pylon_builder.rs`, `templates.rs`: level 1 only, suspension or strain); type workspace I and V presets (`type_workspace.rs`); placed editor's **Make it a type** (`placed_editor.rs`); `ols structures derive` (version 1 attachments); the PLS importer (local) | type workspace: **Add joint**, **Member** between two picked joints, **Point here** on a joint, drag with a grid snap or onto a joint | type workspace tables: a section chosen per member, a support per joint (combo boxes); sections and materials typed in the **Fields** tab of the library page (`library.rs`) — "No [[section]] yet: add sections and materials in Fields" |
| **Shown** | the line's 3D view, plan and profile sheets (sticks placed by `place_stick`, `ols-line/src/library.rs`); the workspace's own 3D view | the same sticks, with the members; the workspace shows "Level N — name" and what a full design still lacks (`missing_for_design`) | the same; the FE model itself has no 3D view, no deformed shape, no usage colours |
| **Checks** | structure loads at the points, Methods 1 and 2, wind bands, the pole check (a type with `pole`) | as level 1; clearances to the body agreed (computed at 2 and 3) but not built | `fe` check in **Structure loads › Usage** and `ols line --usage-csv` (`ols-loads/src/analysis.rs`): every structure of the type in every load case |
| **On its own** | `ols-cad pylon.olss` opens the type alone (`structure_desk.rs`): edit, no computation | the same | `ols structures fe type.olss` with the model's own `[[load]]`s or `--loads loads.csv --structure T2` (`ols-cli/src/structure_fe.rs`); `ols structures pole`; benchmark `structures.fe` (`benchmarks/structures-fe/frames.toml`, 6 cases) |

### Gaps

1. **No computation of a type in the app.** The FE and pole checks run
   only per placed structure, with that structure's loads, from the line.
   The structure desk edits but does not compute.
2. **One load set per type.** `[[load]]` is a list of nodal forces —
   one unnamed case, no factors, no strength factor, no wind. The CLI's
   other source is a CSV exported from a line run.
3. **Points along a member carry no load in the FE model.**
   `from_structure_type` maps only points on a joint (`node`); a point
   with `member`, `at` is reported "no node for point … (its load left
   out)".
4. **Wind on the body is not in the FE model.** The `[[wind.band]]`
   loads go into the structure loads (at mid-height on the centre line)
   but not into the `fe` check (`structure-analysis.md`: "Wind on the
   tower body is not included yet").
5. **Extensions are not in the FE model.** An `[[extension]]` raises the
   placed pylon by `dz`; the model is the same for every extension.
6. **Level 2 has no generator.** Every joint and member is added by
   hand (or as text); no panels, bracing patterns, symmetry (ROADMAP:
   "Symmetry (optional)" open), member groups or copying of a face.
7. **Sections and materials are untyped tables**, edited as TOML, kept
   per type; no shared section library, no standard profiles.
8. **Rebuilding from a template replaces the file.** `PylonBuilder::rebuild`
   writes a new level 1 type: joints and members added since would be
   lost.
9. **No import of a body** from other programs (PLS-TOWER XML is on the
   ROADMAP as a local importer; no DXF or CSV import of joints and
   members) and **no export** of the model other than the file itself
   (IFC writes the stick polylines only, `ols-io/src/ifc.rs`).
10. **No report of a type**: results are console text (`ols structures
    fe`) or one usage line per structure (governing member).
11. **Poles are text files** (`.olsp`), not edited in the app (ROADMAP:
    "poles today: `.olsp` files edited as text").
12. **The level says what is present, not whether it works**: a level 3
    type may still be a mechanism or have a section without a capacity;
    that shows only when solved.

## Goal and principles

A structure type is designed, computed and verified **as an object of its
own**: built from a stick figure up to a full design in one window, with
its own load cases and analyses, its own report — and, in a line, checked
with the line's loads by the same code.

- **One format.** Everything stays in the `.olss`; the levels are how
  complete it is, never separate formats. `.olsfe` and `.olsp` keep
  reading; a separate model can be merged into the type.
- **Levels are derived, never stated.** `detail()` stays the only source;
  the interface shows the level and **what is missing for the next**.
- **Each level is useful on its own.** Level 1 places wires; level 2
  draws and measures; level 3 is analysed. Capacity data (Methods 1, 2,
  wind bands, a pole) can be added at any level.
- **A type checked alone or in the line, by one code path.** The
  analyses take *loads at the type's points* and give the same result
  whether those loads come from the line, a design code or a table.
- **Reproducible.** A type's results follow from the file, its load
  cases and the engine version alone; a report names all three (and the
  file's hash). Same input, same numbers.
- **Self-contained types.** A type carries the sections and materials it
  uses (copied in from libraries, not referred to), so it can be sent and
  checked elsewhere.
- **Plugins stay outside the core.** Design codes remain WASM plugins
  giving cases and factors; mechanics (loads from spans, FE, pole check)
  stay in the core. No plugin-driven change to the engine without asking
  (see Open questions).

## 1. Building the type in the app

One **Structure** window — the ROADMAP's "fully integrated structure
editor with a 3D preview" — grown from today's type workspace and the
structure desk (same window in a project and with `ols-cad pylon.olss`):
the 3D view in local axes on top, tabs underneath. Every edit is one
undoable change of the file as today.

| Tab | Level | Edited in 3D | Edited in tables |
|---|---|---|---|
| **Geometry** | 1 | points, wire points (drag, snap); strings by preset (I, V, V + I, TAK, strain pair) | points, wire points, strings, sets, extensions, `foot`, `height` |
| **Body** | 2 | joints (drag, snap), members between picked joints, points on joints / members; generators (below) | joints, members (with `group`), symmetry |
| **Design** | 3 | supports picked on joints; sections shown by colour or size | sections, materials, member kind, `k`, supports, springs, `[analysis]` |
| **Capacity** | any | — | Method 1 spans, Method 2 load tree, wind bands; a pole's data (instead of the `.olsp` text) |
| **Load cases** | any | loads drawn as arrows at points | the type's own cases (§2) |
| **Results** | 3 / pole | deformed shape, usage colours, reactions | members, nodes, reactions per case; report (§3) |
| **Used in the line** | any | — | structures of this type, their governing loads against the type's cases (§2.4) |

The header always shows: **Level N — name**, and **for level N + 1:**
the list from §1.4, each item clickable to the table row or picked in
3D.

### 1.1 Geometry (level 1)

As today, plus the presets the workspace still lacks (V + string below,
TAK; ROADMAP: "TAK and V + string by hand") and id renames in the tables
(today "ids renamed in Fields").

### 1.2 Body (level 2)

- **By hand**, as today: joints, members, points on them.
- **Generators**, each writing ordinary joints and members (the file
  never depends on the generator to be read):
  - **legs and panels**: the body as a square or rectangular frustum —
    widths at the feet and at the waist, panel levels — and each panel's
    **bracing pattern** (single diagonal, X, K, diamond, Warren, with or
    without horizontals and redundants);
  - **crossarms**: triangular or rectangular arms from a face of the
    body to the type's points (the points then sit on the arm's tip
    joints);
  - **peak / earth-wire peak**;
  - **symmetry**: members and joints copied about x, y or both (ROADMAP
    item);
  - **copy a face** to the opposite face.
- **Member groups** (`group = "leg"`, `"bracing-3"`…): sections assigned
  per group, results reported per group.
- **Body templates**: the generator's inputs kept as a record in the type
  (as `[template]` is for `.olspt` today) so the body can be built again
  from changed dimensions. A rebuild **replaces only what the generator
  made** (members carry the generator that made them), never hand edits
  — and the pylon builder's rebuild (gap 8) keeps the body.
- **Points onto the body**: a level 1 point is moved onto the nearest
  joint or member, or a joint is made under it ("snap the points").

### 1.3 Design (level 3)

- **Sections and materials** in tables with units; added from a **section
  library** (standard angles, tubes, poles — user data, OLS CAD ships
  only synthetic examples) by **copying** into the type.
- Section per member or per group; member kind per member or per group;
  supports picked in 3D (all feet in one click).
- **Pole types**: the pole (`.olsp`) edited in the same window
  (Capacity tab), kept as a file of its own or moved into the type
  (open question).

### 1.4 What is missing for each level

`StructureType::missing_for_design()` grows into
`StructureType::missing(level) -> Vec<Missing>`, each item with a reason
and what to pick (row, joint, member). Two kinds:

- **needed for the level** (the level's definition, unchanged):
  level 2 — a member or a `[[body]]`; level 3 — every member a section,
  a supported joint (or `model`);
- **advice** (shown, not counted in the level): a point not on a joint
  or member (it carries no load in the analysis); a joint with one
  member; members crossing without a joint; a section without a capacity
  (usage cannot be given); a mechanism found by a trial solve (named
  joints, as `FeError` does); an extension the body does not model.

### 1.5 Import and export of the body

- **PLS-TOWER XML** (joints, members, sections) into levels 2 and 3 — the
  ROADMAP's local importer, kept local until the owner decides.
- **DXF 3D lines** → joints (merged within a tolerance) and members (the
  layer as group) — level 2.
- **CSV / XLSX tables** of joints and members (columns mapped, as the
  structure import from CSV today).
- **Merge an `.olsfe`** into the type (one file, ROADMAP: "One file per
  pylon").
- **Export**: the model as `.olsfe`, as CSV tables, as DXF 3D lines, and
  for other analysis programs (§3.2).

## 2. Computing a type on its own

### 2.1 Its own load cases

A type gets named **load cases** of its own, kept in the type
(`[[load_case]]`, §4). Each gives, at the type's points, factored loads
in its axes (transverse along x, longitudinal along y, vertical down),
its strength factor, the factor on self-weight, and optionally wind on
the body (pressure and side). Three sources, all ending as the same
table:

1. **From the line**: the structures of this type, their factored loads
   per load case (today's structure loads), taken as they are for one
   structure, or as the **envelope** over all structures of the type
   (per point and direction, the largest; open question: envelope of
   components or of whole cases).
2. **From design conditions**: the type's design spans and angle (its
   Method 1 data: wind span, weight span, line angle) with the wires and
   tensions it is designed for, under weather and load cases from the
   design-code plugin — the loads computed in the core (mechanics), the
   cases and factors from the plugin (as today: `generate` gives
   `.olswc` and `.olslc`). Broken-wire and construction cases are where
   the code plugins say "still to come" (ROADMAP Phase 4).
3. **Typed** (or pasted): from the client's load tree or another
   program.

Today's `[[load]]` (nodal forces) is read as one typed case named
`model`.

### 2.2 Analyses

Run per type, per case, with the engine used in the line run:

| Analysis | Needs | Exists today |
|---|---|---|
| FE first order / P-Delta | level 3 (or `model`) | yes (`ols_structures_fe::analyse`) |
| Pole check, P-Delta amplification | `pole` | yes (`check_pole`) |
| Method 2 load tree | `capacity.attachment` | yes, per structure in a line; to be run on the type's cases |
| Method 1 allowable spans | `capacity.spans` | yes in a line; on its own only as "the design condition is within the spans" |
| Body wind into the FE model | level 3 + wind bands, or member areas | **no** (gap 4): each band's load shared over the joints in its height range on the windward face (proposal), later from the members' projected areas |
| Loads at points along members | level 3 | **no** (gap 3): shared to the member's two joints by `at` (proposal) |
| Extensions | level 3 + extension members | **no** (gap 5) |
| Capacity **derived** from a level 3 design | level 3 | no: the load tree (Method 2) or allowable spans (Method 1) found by scaling a case until the worst member reaches 100 % — later, open question |

Every run keeps the exit clauses and budgets of
[Performance](performance.md); a case that does not solve is reported
with its reason, as in the line.

### 2.3 Results and reports

- Per case: largest displacement, sum of loads and sum of reactions
  (equilibrium), reactions per support (the foundation loads, for the
  planned foundations module), member forces, usage, what governs.
- In 3D: deformed shape (scale), members coloured by usage (red over
  100 %, orange from 90 %, as on the Usage page), reactions as arrows.
- **Report** of the type (HTML/PDF, CSV tables): the file, its hash,
  level, engine version, cases and their source, the governing member
  per group and per case, the worst ten members, reactions, notes.
- CLI: `ols structures check type.olss [--case …] [--report out.html]
  [--csv …]` — the same results without the app (today's `fe` and
  `pole` remain).

### 2.4 Comparison with the line's use

For a type used in a line: per point and direction, the largest line
load over the type's design load (`Used in the line` tab), the
structures governing it, and the usage of each structure from the line
run next to the type's own usage. A structure above its type's design
loads is named. (This is a report over existing data; it changes no
check.)

## 3. Verification

### 3.1 Checks per level

| Level | Checked when read (errors) | Advice (§1.4) |
|---|---|---|
| 1 | as today: labels and sets unique, strings' tops and wire points known, attachments above the feet, capacity names points of the type | strings to the ground, a set with no section in the line |
| 2 | as today: joints unique, members between two different joints | floating points, dangling joints, crossing members |
| 3 | as today: sections exist; materials, kinds parsed by the FE model | no capacity, mechanism on a trial solve, slenderness above a limit given by the user |

Shape-dependent checks follow [the rule](structure-detail.md): each says
what it does at levels 1, 2 and 3.

### 3.2 Independent verification

- **Hand-check summary** in the report: per governing member its force,
  length, k·L, I, Euler load, capacity formula and numbers; equilibrium
  of the whole (Σ loads = Σ reactions per direction); a statically
  determinate sub-check where the structure allows it (e.g. the ground
  line moment of a pole from its loads).
- **Export to other programs**: the model with its cases in neutral
  tables (nodes, members, sections, supports, loads), and in the input
  format of one or two other analysis programs (which ones: open
  question; written from their public documentation).
- **Re-run**: the report's inputs reproduce its numbers (`ols structures
  check` on the same file and engine version).

### 3.3 Regression and benchmarks

- `benchmarks/structures-fe`: add cases with published references — a
  lattice tower or space truss from a textbook or a published example,
  a guyed mast, a pole by hand calculation (owner to provide or approve
  the references; no tables typed from memory).
- The synthetic example types (`examples/synthetic-line/Structures`):
  one at each level, the level 3 one with its own load cases, its results
  kept as a regression test (tolerances, not exact digits).
- Tests per level for every shape-dependent check.

## 4. Data model and formats

All changes in `.olss` (version 2 → 3, older versions read as before);
none in the line file.

| Change | Where | Why |
|---|---|---|
| member `group` | `TypeMember` | sections, kinds and reports per group |
| generator record (`[body_template]`) and member `made_by` | `StructureType`, `TypeMember` | rebuild without losing hand edits |
| `[[symmetry]]` (optional) | `StructureType` | copies about x, y |
| `[[load_case]]` with `[[load_case.point]]` (label, transverse, longitudinal, vertical), `strength`, `self_weight`, `wind`, `source` | `StructureType` | §2.1; replaces `[[load]]` (read as case `model`) |
| typed `TypeSection`, `TypeMaterial` instead of raw tables | `structures.rs` (shared with `ols-structures-fe`) | tables with units, typos refused (data model review §4.3) |
| extension members (`[[extension.member]]`, joints) | `Extension` | gap 5; open question |
| `missing(level)` | `StructureType` | §1.4 |
| section library file (new, name to decide) | new | §1.3 |

Plugin boundaries: design-code plugins keep their `code-plugin` world
(weather and load cases); loads from design conditions (§2.1 item 2) are
core mechanics. If a code needs structure-specific rules (e.g. a column
curve for angle members, code wind on lattice bodies), that is a new
plugin interface — asked before it is built.

[Data model map](data-model.md), the user guide pages and the XSD (if a
type is written to OLS XML) follow every change.

## 5. Phases

Each phase is a deliverable of its own, checked and committed.

1. **Missing for each level + type computation in the app**: `missing(level)`,
   the header in the workspace; a Results tab running today's FE and pole
   checks on the type's `[[load]]`; loads at points along members shared
   to joints (gap 3).
2. **Type load cases**: `[[load_case]]` (format version 3), typed cases
   and cases taken from a line run (one structure, or the envelope);
   `ols structures check`; the report (HTML, CSV).
3. **Body generators**: legs, panels and bracing patterns, crossarms,
   symmetry, groups, body templates with rebuild that keeps hand edits.
4. **Design tab**: typed sections and materials, section library copied
   in, sections per group, supports in 3D; pole editing in the window.
5. **Body wind and extensions in the FE model** (gaps 4 and 5).
6. **Import / export**: DXF and CSV bodies, `.olsfe` merge, model export
   for other programs; PLS-TOWER XML stays local unless decided.
7. **Design conditions**: loads from design spans with the code plugins'
   cases; comparison with the line's use (§2.4); capacity derived from a
   level 3 design (if wanted).
8. **Benchmarks** with published references, alongside every phase.

## Open questions

1. **Shape-dependent checks per detail level** (open since the
   detail-level rule): for each check that reads the structure's shape —
   clearances to the body, insulator swing to the structure, the FE
   check, body wind — what does it do at levels 1, 2 and 3 (not
   available / approximation / exact) and what is reported when the level
   is too low? In particular: clearance tests at levels 2 and 3 (still
   "to be defined with the project owner").
2. **Poles and the level**: should a type with a `pole` count as level 3
   (it is fully checkable), or should capacity data be shown as its own
   badge next to the level? Should the `.olsp` move into the type?
3. **Level 2 by `[[body]]` polylines**: keep it as level 2 (drawing only,
   no joints), or make level 2 mean joints and members only and treat
   `[[body]]` as a drawing aid?
4. **Type load cases from the line**: one structure's cases, or the
   envelope over all structures of the type — per component (point and
   direction) or per whole case (keeps equilibrium)?
5. **Loads from design conditions** (§2.1 item 2): wanted in the core, or
   is the type's load set always given (by the line or by the client)?
6. **Body wind in the FE model**: from the wind bands shared to joints
   (works with today's data) or from the members' projected areas and a
   code's solidity rules (needs a plugin interface)?
7. **Extensions in the FE model**: extension members in the type, or a
   separate type per extension at level 3?
8. **Section library**: format, shipped content (synthetic only?), and
   copy into the type versus refer to it.
9. **Export for other programs**: which analysis programs' input formats
   (if any) to write for independent verification?
10. **Derived capacity**: compute Method 1 / Method 2 capacities from a
    level 3 design, or always take them from the design documents?
11. **Generators**: which bracing patterns and arm shapes first; is
    symmetry required or optional (ROADMAP says optional)?
12. **Structure–line interaction levels 2 and 3** (ROADMAP, still to be
    defined): should the type's stiffness at its points (from level 3)
    be part of this work or wait for that topic?
