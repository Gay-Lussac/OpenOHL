# Building structures from level 1 to level 3

Status: **specification** (2026-10-17). The project owner decided the
points listed under [Decisions](#decisions-2026-10-17); nothing here is
built unless "Today" or a *Built* mark says so (built means in active
development, not approved yet). The questions still open are at the
end.

Asked by the user: the specification of the interface that takes a
structure from level 1 to level 3, and that **computes and verifies a
structure on its own**, not only through a line.

Related: [Structure detail levels in checks](structure-detail.md) (the
rule for shape-dependent checks), [Structure libraries](../user-guide/structure-library.md),
[Structure analysis](../user-guide/structure-analysis.md), the ROADMAP
sections "Structures in their own axes, three detail levels"
and "Structure–line interaction — levels of computation".

**Words.** This page says **structure** (a structure file, `.ohls`; a
structure definition) for what the code calls a *type*
(`StructureType`, `TypeMember`, …). A **placed structure** is one
structure of the line that uses a structure file. The code names may be
renamed later; they are not renamed now.

## Decisions (2026-10-17)

1. **Levels.** Level 1: only the attachment points are described; the
   structure is a stick figure and cannot be verified. Level 2: the real
   geometry of the structure, but no detailed construction and no code
   check (maximum loads only on its strings, decision 13). Level 3: every part
   and joint detail; the structure can be computed (FE) and verified by a
   code (for example SIA 263 for steel in Switzerland).
2. **Poles have the same three levels.** 1: the attachment points
   without a body. 2: the outside lines of the pole (its volume).
   3: detailed engineering; structural computation possible.
3. **`[[body]]` polylines** make level 2 only when they describe a 3D
   object. Polylines in one plane stay level 1.
4. **A separate structure application.** The structure editor is its own
   program (own binary and window, same workspace crates), with its own
   tabs. It starts from an **Edit** button on a structure in the line
   application, or by opening a `.ohls` directly.
5. **Load cases come from the line.** Each placed structure receives the
   load cases of the line it stands in. Its loads are shown in a table,
   editable, and exported and imported.
6. **Load cases are recipes.** A structure's load case is a list of loads
   and what each is applied to: which value the line gives (e.g. the
   largest tension), a factor, a one-sided share (e.g. 40 % on one side),
   at which points or on which members. The core computes the loads from
   these recipes with the values the line gives at that placed structure.
   A load case may also hold values of its own (not from the line).
   Surface loads on members (wind, snow) are possible. Load cases are
   exported and imported.
7. **Loads per placed structure.** Every place gets its own loads, so the
   earlier question "envelope per component or per whole case" is gone.
   When one structure file is used at several places, each place has its
   own loads; an envelope over the places is only a reporting option.
8. **Replace structure** (new tool): a new structure is put in place of
   another, with its strings. Wire points (sets) that match are rewired
   automatically; for those that differ the user says which goes where;
   when nothing matches the replacement is still possible, after the
   model shows what will be removed and the user confirms.
9. **Shape-dependent checks** must be able to behave differently by
   level; that is why the levels exist. For now no check is changed.
10. **Body wind** is computed from the surface area of the profiles, so
    it needs level 3.
11. **Cross-section library**: loaded for a structure, which refers to it (not
    copied). It is a library shared by a whole project, exported and
    imported, stored like the other library kinds of the project file
    (per project line).
12. **Export** to IFC and to analysis and steel-detailing programs; other tower programs later.
13. **Capacities.** At levels 1 and 2 maximum loads may be given on the
    strings (their insulators and fittings), but not for the structure
    itself: its integrity cannot be checked below level 3. The
    structure's capacity is computed only at level 3. Weather cases and
    the load cases at the suspension points can be defined from level 1,
    and exported, so a structure designed in another program gets its
    loads from the line (an application feature).
14. **Symmetry** is an option for joints and members, to ease building.
    The application must be easy to use.
15. **Structure–line interaction** has four levels, as is common in line design software: 1
    ruling span; 2 FE of the wires on rigid structures (what OpenOHL
    has); 3 FE of the wires with a linear stiffness matrix at each
    structure's suspension points, derived from the structure (level 3
    structures only); 4 the full solve, structures and line iterated
    together.
16. **Principle for level 2 → 3**: every geometry already defined gets
    all its parameters.

17. **Extensions at level 3** carry their own body: each extension has
    its own joints and members (a body piece below the reference level,
    with cross-sections at level 3), so every extension is computed. At levels
    1 and 2 an extension may stay a name and a `dz`.
18. **Today's structure capacity** (`[capacity]`: Method 1 allowable
    spans, Method 2 load trees) when the levels are built: a Method 2
    load tree becomes maximum loads on the strings at levels 1 and 2;
    Method 1 spans (a capacity of the structure itself) only at level 3.
    No check changes until the levels are built.
19. **Interaction levels 3 and 4** treat a structure below level 3 as
    rigid, and the result says so.

20. **A structure file may describe a family.** One file holds a structure
    family; a few parameters chosen per position (extension, leg
    extensions) adapt it to the ground. Leg extensions are part of this.
    In the line, every placed structure is **unique to its position**:
    the parameters chosen there give one concrete structure, with its own
    name, loads, model and results.
21. **Choosing a structure** (line application): a window with a table
    of the available structures (id, name, kind, level, sets, height,
    extensions, description, uses in the line), a filter, and a preview
    on the right; it replaces the drop-down of Add structure and the
    structures table.

## Today

### The three levels, as the code has them

`ohl_structures::Detail`, worked out by `StructureType::detail()`
(`crates/ohl-structures/src/structures.rs`) from what the file holds; a
`detail = …` written in older files (`stated_detail`) is read and ignored.

| Level | `Detail` | Reached when | What it adds |
|---|---|---|---|
| 1 stick figure | `Stick` | always (no members, no `[[body]]`) | `[[point]]`, `[[wire]]`, `[[string]]` (or version 1 `[[attachment]]`), `height`, `foot`, `[[extension]]`. Drawn as a shaft to `height` and a crossarm to every point. |
| 2 sticks of the structure | `Outline` | any `[[member]]`, or `[[body]]` polylines that make a 3D object (`body_is_3d`) | `[[node]]` joints and `[[member]]`s without cross-sections, or `[[body]]` polylines; points may sit on a joint (`node`) or along a member (`member`, `at`). |
| 3 full design | `Design` | `model = "….ohlfe"` named, **or** `missing_for_design()` empty: members exist, every member has a `section`, at least one joint has a `support` (a `spring` alone does not count: a structure standing only on springs stays at level 2), the same for each extension | `[[section]]`, `[[material]]`, member `kind` (truss / frame / cable / tension), `k`, `reference`, `roll`, `pretension`, supports and springs, `[analysis]`, `[[load]]`. |

Where today's code differs from the decisions:

- **Capacity is read at any level.** `[capacity]` (Method 1 allowable
  spans, Method 2 load trees) and `[[wind.band]]` are read at any level;
  a level 1 structure with a capacity is checked by Methods 1 and 2 in a
  line. Decisions 13 and 18 say how this changes when the levels are
  built.
- ~~**Poles have no level.**~~ *Built* (phase 3): `detail()` looks at
  the pole (`pole_detail`): a `[pole]` outline makes level 2, a pole the
  check can read (or a named `.ohlp`) level 3.
- ~~**Any `[[body]]` polyline gives level 2**, also a flat one.~~ *Built*
  (decision 3): only `[[body]]` polylines that make a 3D object
  (`body_is_3d`); a flat face stays level 1.

### What exists per level

| | Level 1 | Level 2 | Level 3 |
|---|---|---|---|
| **Data** | points, strings, wire points, extensions, foot (`structures.rs`) | `TypeNode`, `TypeMember` (no cross-section), `Outline` | a member's cross-section from the project's cross-section library (`.ohlsec`) or the structure's raw `[[section]]` `toml::Table`s, the library first, parsed by `ohl_structures_fe::Model::from_structure_type_with` (`crates/ohl-structures-fe/src/model.rs`); or a separate `.ohlfe` |
| **Built by** | structure builder from `.ohlpt` templates (`pylon_builder.rs`, `templates.rs`: level 1 only, suspension or strain); workspace I and V presets (`type_workspace.rs`); the placed editor's **Make it a type** (`placed_editor.rs`); `ohl structures derive` (version 1 attachments); a local importer of another program's files | workspace: **Add joint**, **Member** between two picked joints, **Point here** on a joint, drag with a grid snap or onto a joint | workspace tables: a cross-section per member, a support per joint; cross-sections and materials typed in the **Fields** tab of the library page (`library.rs`) |
| **Shown** | the line's 3D view, plan and profile sheets (`place_stick`, `ohl-line/src/library.rs`); the workspace's own 3D view | the same, with the members; the workspace shows "Level N — name" and what level 3 still lacks (`missing_for_design`) | the same; no 3D view of the FE model, no deformed shape, no usage colours |
| **Checks** | structure loads at the points, Methods 1 and 2, wind bands, the pole check | as level 1 | `fe` check in **Structure loads › Usage** and `ohl line --usage-csv` (`ohl-loads/src/analysis.rs`), with the member wind on profiles from the cross-section library |
| **On its own** | `ohl-design pylon.ohls` opens the structure alone (`structure_desk.rs`): edit, no computation | the same | `ohl structures fe structure.ohls` with the model's own `[[load]]`s or `--loads loads.csv --structure T2` (`ohl-cli/src/structure_fe.rs`); `ohl structures pole`; benchmark `structures.fe` (`benchmarks/structures-fe/frames.toml`) |

### `[[extension]]` today

An `[[extension]]` has a `name` and a `dz`. A placed structure may name
one (`extension = "+3"`). The body is then longer below the reference
level z = 0, so every point above the feet rises by `dz`. Extensions are
also used by automatic spotting (candidates by structure and extension)
and by the bill of materials (a price per extension). *Built* (U4): the
wind bands rise with the extension (`ohl_loads::add_structure_wind`: the
band's heights plus `dz`; the force unchanged), the rule `loads.md`
states.

*Built* (phase 3, decisions 17 and 20): an extension may carry its own
piece (`[[extension.node]]`, `[[extension.member]]`) and leg extensions
(`[[extension.leg]]`, a leg A–D or a joint id and its `dz`);
`StructureType::extended` applies the rule (the piece added, the body
joints it reaches lose their supports, a leg's support slides `dz` lower
along its leg member — [user
guide](../user-guide/structure-library.md#extensions-and-leg-extensions));
`Model::from_structure_type_extended` builds the model on it and
`ohl-loads` checks each placed structure on its extension's model. An
extension without a piece or legs behaves as before.

### Gaps

1. ~~**No computation of a structure in the app.**~~ *Built* (phase
   4): OHL-Structure's FE results panel solves the load table's cases
   (`ohl_structure_link::analyse_cases`); *built* (U4): on the extension
   chosen and with the line's pole check
   (`ohl_structure_link::analyse_structure`), and the structure report
   (`ohl_structure_export::report`, **Results › Structure report…** and
   the tool `ohl-structure-report`). OHL-Design's structure desk edits
   but does not compute.
2. ~~**One load set per structure.**~~ *Built* (phase 4): load-case
   recipes (`[[load_case]]`); the old `[[load]]` nodal forces are read as
   one case named `model`.
3. ~~**Points along a member carry no load in the FE model.**~~ *Built*:
   a point cannot be put on a member any more (2026-10-21): it needs a
   joint, inserted on the member (the member split, continuous); files
   that placed one along a member are read so. The share of its load to
   the member's two joints is gone.
4. ~~**Rebuilding from a template replaces the file.**~~ *Built*: a
   rebuild keeps what was added by hand (`pylon_builder.rs`, `rebuilt`).
5. ~~**Flat `[[body]]` polylines count as level 2.**~~ *Built*: only a 3D
   body gives level 2 (decision 3, `body_is_3d`).
6. **Wind on the body in the FE model** — *built for the FE check*: the
   member wind on profiles from the cross-section library (pressure × drag ×
   width × length × sin²θ, half to each joint; no shielding, one `drag`
   per structure); the wind bands stay out of the model. Load-case
   recipes (surface loads per case) still to come.
7. ~~**Extensions are not in the FE model.**~~ *Built* (phase 3):
   extension pieces and leg extensions in the model of each placed
   structure.
8. ~~**Level 2 has no generator.**~~ *Built* (phase 3, body part): legs
   and panels with bracing patterns, crossarms, peaks, copying a face,
   rebuild, symmetry and member groups in the structure application.
9. **Cross-sections and materials are untyped tables**, edited as TOML, kept
   per structure — *partly built*: the project's cross-section library
   (`.ohlsec`, a project library kind) is referred to by id, first; the
   structure's own tables remain as raw TOML.
10. **No import of a body** other than a local importer of another program's files, and **no
    export** of the model other than the file itself (IFC writes the
    stick polylines only, `ohl-io/src/ifc.rs`) — *export built*: a level
    3 structure as IFC4, IFC2x3, SAF and CSV ([Structure
    exports](structure-exports.md)).
11. ~~**No report of a structure**: results are console text or one usage
    line per placed structure.~~ *Built* (U4): the structure report — one
    HTML page without external resources (identity, level, extension,
    cross-sections and materials, the pole, load cases, usage per check with
    the governing case, members by their largest usage, reactions, the
    pole check, notes and flags, the software version, the pre-alpha
    warning) and the members' results as CSV, from **Results › Structure
    report…** and `ohl-structure-report` (`ohl_structure_export::report`).
    Not yet: a PDF of its own (the page prints), the input hashes of §3;
    the report's format is to be defined with the owner (reports will be
    needed as for the line). `ohl structure report` asks the tool for it
    in JSON (`--request`).
12. ~~**Poles are text files** (`.ohlp`), not edited in the app, and
    have no level.~~ *Built* (phase 3): a `[pole]` table in the structure
    file (outline by height, round or polygonal; the `.ohlp` fields), its
    levels, edited in Body › Pole and Design › Pole; a `.ohlp` still
    reads and can be brought into the file.
13. **The level says what is present, not whether it works**: a level 3
    structure may still be a mechanism; that shows only when solved.
14. ~~**No way to replace a structure** by another one keeping its
    wires (decision 8).~~ *Built* (§1.6).

## Goal and principles

A structure is designed, computed and verified **as an object of its
own**, in its own application: built from a stick figure up to a full
design, with load cases that take their values from the line, its own
report — and, in a line, checked with the line's loads by the same code.

- **One format.** Everything stays in the `.ohls`; the levels are how
  complete it is, never separate formats. `.ohlfe` and `.ohlp` keep
  reading; a separate model can be merged into the structure.
- **Levels are derived, never stated.** `detail()` stays the only source;
  the interface shows the level and **what is missing for the next**.
- **Each level is useful on its own.** Level 1 places wires; level 2
  gives the real geometry (drawing, distances, maximum loads as data);
  level 3 is computed and code-checked.
- **Level 2 → 3 completes, never redraws.** Every joint, member and
  outline already defined gets all its parameters (decision 16).
- **One code path, alone or in the line.** The analyses take loads at the
  structure's points and members and give the same result whether those
  loads come from the line or are typed in.
- **Reproducible.** Results follow from the structure file, the cross-section
  library, the load cases with their line values, and the engine
  version; a report names all of them (with hashes).
- **Shared libraries.** Cross-sections live in a cross-section library the structure
  refers to (decision 11), not copied into each structure.
- **Plugins stay outside the core.** Design codes and code checks are
  plugins; mechanics (loads from the recipes, FE, pole check) stay in
  the core. No engine change for a plugin without asking the owner.
- **Easy to use.** Symmetry, generators and presets so a body is not
  typed joint by joint (decision 14).

## 1. The structure application

A separate program (decision 4): its own binary and window, built on the
same workspace crates as `ohl-design` (`ohl-structures`,
`ohl-structures-fe`, `ohl-loads`, `ohl-render`, the session commands).
It grows from today's structure workspace and structure desk.

- **Started** from an **Edit** button on a placed structure (or on a
  structure in the library) in the line application, or by opening a
  `.ohls` directly (file association, command line).
- **Started from the line**, it receives the project's cross-section library
  and the placed structure's load values (§2.2), and saves the structure
  back into the project; the line follows on save, as today. Opened
  alone, it works on the file and its libraries next to it.
- Every edit is one undoable change of the file.
- The 3D view in local axes on top, tabs underneath:

| Tab | Level | Edited in 3D | Edited in tables |
|---|---|---|---|
| **Geometry** | 1 | points, wire points (drag, snap); strings by preset (one string, two strings to one wire point, a string below two, two tension strings from a string, a tension pair) | points, wire points, strings, sets, extensions, `foot`, `height` |
| **Body** | 2 | joints (drag, snap), members between picked joints, points on joints and members; outlines of a pole; generators | joints, members (with `group`), symmetry |
| **Design** | 3 | supports picked on joints; cross-sections shown by colour or size | cross-section per member or group (from the cross-section library), materials, member kind, `k`, supports, springs, joint details, `[analysis]` |
| **Load cases** | any | loads as arrows at points, surface loads on members | the recipes and their evaluated loads (§2) |
| **Results** | 3 | deformed shape, usage colours, reactions | members, nodes, reactions per case; report (§3) |
| **Line loads** | any | — | the places using this structure, their loads per case |

The header shows **Level N — name** and **for level N + 1:** the list
from §1.4, each item clickable to its table row or picked in 3D.

### 1.1 Geometry (level 1)

As today, plus the presets the workspace still lacks (a string below two,
two tension strings from a string) and id renames in the tables.

### 1.2 Body (level 2)

- **By hand**, as today: joints, members, points on them.
- **Generators**, each writing ordinary joints and members (the file
  never needs the generator to be read):
  - **legs and panels**: the body as a frustum — widths at the feet and
    the waist, panel levels — and each panel's **bracing pattern**
    (single diagonal, X, K, diamond, Warren, with or without horizontals
    and redundants);
  - **crossarms** from a face of the body to the structure's points
    (several points on one arm: *built*, option `one_arm`);
  - **plan bracing** at chosen levels, and where the slope changes and
    at the crossarms' levels (*built*, options `plan`, `plan_auto`);
  - **peak / earth-wire peak**;
  - **copy a face** to the opposite face.
- **Symmetry** (option, decision 14): joints and members mirrored about
  x, y or both, kept in step while editing.
- **Member groups** (`group = "leg"`, `"bracing-3"`…): cross-sections set per
  group, results reported per group.
- **Body templates**: the generator's inputs kept in the structure, so
  the body can be built again from changed dimensions; a rebuild
  replaces only what the generator made.
- **Points onto the body**: a level 1 point is moved onto the nearest
  joint or member, or a joint is made under it.
- **Poles** (decision 2): the outside lines — diameters or
  circumferences by height, the cross-section shape — instead of the
  `.ohlp` text. Edited in the Pole panel (Body menu).
- **`[[body]]` polylines** give level 2 only when they span 3D
  (decision 3). *Built* (`body_is_3d`).
- **Maximum loads** may be given at level 2 as data (decision 1).

### 1.3 Design (level 3)

- **Cross-sections** from the project's cross-section library (§4.2), referred to by
  id; **materials** in a table with units.
- Cross-section and member kind per member or per group; supports picked in 3D
  (all feet in one click).
- **Joint details** (bolts, plates, eccentricities) as far as the code
  check needs them; their content follows the code check plugin
  (open question 1).
- **Poles**: wall, material and the data the pole check needs.

### 1.4 What is missing for each level

`StructureType::missing_for_design()` grows into
`StructureType::missing(level) -> Vec<Missing>`, each item with a reason
and what to pick (row, joint, member). Two kinds:

- **needed for the level**: level 2 — a member, a 3D body or a pole
  outline; level 3 — every member a cross-section, a supported joint (or
  `model`), every geometry with all its parameters;
- **advice** (shown, not counted in the level): a point not on a joint
  or member; a joint with one member; members crossing without a joint;
  a mechanism found by a trial solve.

### 1.5 Import and export

- **Export** (decision 12): **IFC** (members as solids with their
  profiles), and the formats of steel-detailing and analysis programs
  (model, cross-sections, supports, load cases), written from the public
  documentation of each format; other tower programs later.
  *Built* ([Structure exports](structure-exports.md)): IFC4 with member
  solids and the structural analysis model, IFC2x3 members (for detailing programs
  that convert only IFC2x3), SAF xlsx (analysis programs that read
  SAF), CSV tables.
- **Import**: another program's tower XML stays a local importer; DXF 3D lines and
  CSV / XLSX tables of joints and members into level 2; merge an
  `.ohlfe` into the structure.
- **Load cases** and their loads: CSV / XLSX, both ways (§2.3).

### 1.6 Replace structure (line application)

**Status (2026-10-17): built**, in active development — session command `replace_structure`
(`crates/ohl-session/src/replace.rs`), the Replace structure tool, the
structure's Properties and the Structures table
(`crates/ohl-app/src/replace.rs`); see the user guide (Editing).

A tool of the line application (decision 8): a placed structure is
replaced by another structure from the library, at the same place and
orientation.

1. The new structure comes with its own strings and wire points.
2. Every wire point (set) of the old structure is matched to the new one
   by its set. Where all match, the wires are moved to the new wire
   points and the sections are strung again, without a question.
3. Where they differ, a window lists the old wire points and offers the
   new ones; the user says which goes where. The proposal is filled in
   as far as it can be (same set, else same side and height order).
4. Where nothing is given, the replacement is still possible: the model
   shows what will be removed (wires, jumpers, sections ending there)
   and asks for confirmation.
5. One undoable session command; overrides of the old structure's
   strings are listed and dropped.

## 2. Load cases and computing

### 2.1 Load-case recipes

A structure holds named **load cases** (`[[load_case]]`, §4.1). Each is
a list of **loads**, and each load says:

- **what**: a value the line gives at the placed structure (e.g. the
  largest tension of a wire, its vertical load, wind on the wire, ice
  load), or a value given in the load case itself;
- **how**: a factor, a one-sided share (e.g. 40 % of the largest tension
  on one side only), a direction in the structure's axes;
- **where**: points or wire points of the structure (all of a set, one
  side, one phase), or members for **surface loads** (wind or snow, a
  pressure on the members' area; decision 6).

Examples, as data: *40 % one-sided of the largest tension*, *largest
load under ice*, *wind across the line*, *broken wire at one point*.
The weather and load cases of the line (from the design-code plugins)
give the values; the recipes say how they act on the structure.

### 2.2 Loads at a placed structure

The core evaluates the recipes with the values the line gives at that
placed structure (today's structure loads per load case, at the
suspension points) and gives a **load table**: per load case, per point
or member, transverse, longitudinal and vertical. Each placed structure
has its own table (decision 7). An envelope over the places of one
structure file is a reporting option only.

Opened alone (no line), the recipes that need line values are listed as
"no line value"; the user types the values or imports a table.

### 2.3 The load table

- Shown in the Load table panel and in the line application per placed
  structure.
- **Editable**: a value typed over an evaluated one is kept as given and
  marked as such.
- **Exported and imported** (CSV / XLSX): the recipes and the evaluated
  table, so loads from another program or a client's load tree come in
  as cases of their own.
- Today's `[[load]]` (nodal forces) is read as one case named `model`.

### 2.4 Analyses

Run per structure, per case, with the engine used in the line run:

| Analysis | Needs | Exists today |
|---|---|---|
| FE first order / P-Delta | level 3 (or `model`) | yes (`ohl_structures_fe::analyse`) |
| Pole check, P-Delta amplification | pole at level 3 | yes (`check_pole`) |
| Loads at points along members | level 3 | yes (shared to the member's joints) |
| Surface loads on members (wind, snow) | level 3 (profile areas) | yes, in the load-case recipes: a pressure on the members' area from the cross-section library (`ohl-structure-loads`; `ohl_structure_link::SectionAreas`); rows marked "no area" without one |
| Body wind | level 3 (profile areas, decision 10) | yes, in the FE check: member wind on profiles from the cross-section library (`Model::member_wind`); wind bands kept out of the model |
| Extensions in the model | level 3 extensions (decision 17) | yes: `Model::from_structure_type_extended`, per placed structure in the line run (`ohl-loads`), `ohl structures fe --extension` |
| Capacity derived from the design | level 3 | no: the load tree or allowable spans found by scaling a case until the worst member reaches 100 % |

Every run keeps the exit clauses and budgets of
[Performance](performance.md); a case that does not solve is reported
with its reason.

### 2.5 Results and reports

- Per case: largest displacement, sum of loads and sum of reactions,
  reactions per support (foundation loads), member forces, usage, what
  governs.
- In 3D: deformed shape, members coloured by usage, reactions as arrows.
- **Report** (HTML / PDF, CSV tables): the file and its hash, level,
  engine version, cross-section library, load cases with their source, the
  governing member per group and per case, reactions, notes.
- CLI (built 2026-10-19, in place of the `ohl structures check` planned
  here): `ohl structures analyse structure.ohls [--loads table.csv]` and
  `ohl structures report structure.ohls [--loads table.csv] -o out.html`
  ask OHL-Structure's headless tool `ohl-structure-report` (a program of
  its own, [OHL-Structure](../user-guide/structure-app.md)); `fe` and
  `pole` remain.

## 3. Verification

### 3.1 Checks per level

| Level | Checked when read (errors) | Advice (§1.4) |
|---|---|---|
| 1 | as today: labels and sets unique, strings' tops and wire points known, attachments above the feet | strings to the ground |
| 2 | as today: joints unique, members between two different joints | floating points, dangling joints, crossing members |
| 3 | as today: cross-sections exist; materials and kinds parsed by the FE model | mechanism on a trial solve, slenderness above a user limit |

Shape-dependent checks follow [the rule](structure-detail.md): they must
be able to behave differently at levels 1, 2 and 3 (decision 9). No
check is changed now.

### 3.2 Code checks

Member and joint verification by a code (e.g. SIA 263 for steel) is a
**plugin**: the core gives member forces, lengths, cross-sections and joint
data; the plugin gives usages and the formulas used. Its interface is
open question 1.

### 3.3 Independent verification

- **Hand-check summary** in the report: per governing member its force,
  length, k·L, cross-section values, capacity formula and numbers; equilibrium
  of the whole (Σ loads = Σ reactions per direction).
- **Export to analysis and steel-detailing programs** (§1.5) with the load cases, so the same
  structure can be computed there.
- **Re-run**: the report's inputs reproduce its numbers.

### 3.4 Regression and benchmarks

- `benchmarks/structures-fe`: cases with published references (a
  lattice tower or space truss, a guyed mast, a pole by hand
  calculation); the owner provides or approves the references.
- Synthetic example structures (`examples/synthetic-line/Structures`):
  one at each level, the level 3 one with its load cases and results
  kept as a regression test.
- Tests per level for every shape-dependent check.

## 4. Data model and formats

### 4.1 Structure file

Changes in `.ohls` (version 2 → 3; older versions read as before):

| Change | Where | Why |
|---|---|---|
| member `group` *(built)* | `TypeMember` | cross-sections and reports per group |
| `[[symmetry]]` (optional) *(built: `mirror` x / y / xy, `groups`)* | `StructureType` | mirrored joints and members |
| generator record and member `made_by` *(built: `[[generator]]`, `made_by` on members and joints)* | `StructureType`, `TypeMember`, `TypeNode` | rebuild without losing hand edits |
| pole outline (diameters by height, shape) *(built: `[pole]`)* | `StructureType` | poles at level 2 (decision 2) |
| `[[load_case]]` with `[[load_case.load]]` (what, factor, one-sided share, direction, points or members, surface pressure) *(built; kept raw in `StructureType`, read by `ohl-structure-loads`)* | `StructureType` | §2.1; replaces `[[load]]` (read as case `model`) |
| typed `TypeSection` reference (cross-section library id) and `TypeMaterial` | `structures.rs`, shared with `ohl-structures-fe` | §1.3, §4.2 |
| joint details | `TypeNode` | level 3 code checks (open question 1) |
| extension members *(built: `[[extension.node]]`, `[[extension.member]]`, `[[extension.leg]]`)* | `Extension` | decision 17 |
| `missing(level)` *(built in `ohl-structure-edit`, not in `StructureType`)* | `StructureType` | §1.4 |

The evaluated load table of a placed structure (§2.2) is a result, not
stored in the structure file; values typed over it are stored with the
placed structure in the line: a load file per place, `<stem>.<placed structure>.loads.csv` beside the structure file, kept in the project as library kind `loads` (per project line; the line file is unchanged; [Data model](data-model.md#structure-load-cases-load_case-ohlslc)).

### 4.2 Cross-section library

A new library kind, `.ohlsec` (`ohl-sections`, crate `ohl-sections`,
[Cross-section library](../user-guide/section-library.md); built and referred
to by the FE model, a project library kind `sections`): standard profiles (angles,
tubes, poles…), user data; OpenOHL ships only synthetic examples. A
member's `section = "<id>"` resolves in the cross-section library first,
then in the structure's own `[[section]]` tables (member groups: the
same, when they come). A structure **refers** to it by
id (decision 11). In a project it is stored like the other library kinds
(per project line, see [Project file tables](data-model.md#project-file-tables-cratesohl-session)), so all
structures share one copy; exported and imported like the others.

Plugin boundaries: design-code plugins keep their `code-plugin` world
(weather and load cases); the recipes' evaluation is core mechanics; a
code check of members and joints is a new plugin interface, asked
before it is built.

[Data model map](data-model.md), the user guide pages and the XSD (if a
structure is written to OHL XML) follow every change.

## 5. Phases

Each phase is a deliverable of its own, checked and committed.

1. **First fixes** (*built*): loads at points along members shared to
   the member's joints; template rebuild keeping hand-made joints and
   members; flat `[[body]]` polylines no longer level 2.
2. **Separate structure application**: its own binary on the workspace
   crates; started from **Edit** in the line application or by opening
   a `.ohls`; the tabs of §1 with today's editing; `missing(level)` in
   the header. *Built*: `ohl-structure` (`apps/structure/ohl-structure-app`)
   and `ohl-structure-edit` ([crates](structure-app.md), [user
   guide](../user-guide/structure-app.md)); `missing(level)` lives in
   `ohl-structure-edit`, not in `StructureType`; the mechanism found by a
   trial solve is not among the advice yet; Load cases, Results and In
   the line were placeholders then (built since: phase 4, [crates](structure-app.md)). *Split built* (2026-10-17): the line
   application's structure screen (the Edit tool, **Strings and
   points…**) edits only the basics — attachment points, strings, wire
   points and sets — tables left of the structure's 3D view; the body and
   design are edited only in the structure application (**Edit in the
   structure application…**). Starting it shows why it fails (not found,
   another build via `ohl-structure --version`, its error output when it
   stops at once).
3. **Levels including poles**: pole outline at level 2 and pole design at
   level 3; `detail()` looks at poles; body generators, symmetry, member
   groups, body templates. *Body part built* (format version 3): member
   `group` and `made_by`, `[[generator]]` records (the body templates)
   and `[[symmetry]]`, read by `ohl-structure-edit` (the core keeps the
   last two unread); generators *legs and panels* (single, X, K,
   diamond, Warren; horizontals per panel), *crossarms* (triangular,
   rectangular), *peak*, *copy a face*, each a pure function writing
   ordinary joints and members; rebuild replacing only what a generator
   made (what was set on it kept, what others use kept and reported);
   edits mirrored by the symmetry and **Make symmetric**; *built
   (2026-10-19), off by default:* redundants per panel (one or two
   levels, in every bracing pattern, triangulated), plan bracing per
   level (X, diamond with a cross) and where the slope changes or at
   the crossarms' levels, widths per panel (the slope changing), several
   points on one crossarm (a cross-section of the arm at each inner point);
   groups with a
   cross-section and kind per group written on each member (no `[[group]]`
   table); the Generators panel (preview, Apply = one undo step),
   group column and filter, Shift + click in tables, Design › Groups
   ([user guide](../user-guide/structure-app.md#the-body-panels)). *Poles
   and extensions built*: the `[pole]` table (`ohl_structures::pole`:
   outline by height, round or polygonal, the level 3 fields of a
   `.ohlp`), `detail()` looking at it, the pole check reading it through
   the same conversion as a `.ohlp` (`ohl_structures_fe::pole::
   structure_pole`, `Pole::from_inline`: the example pole gives the same
   numbers), `ohl structures pole --type` with the type's own pole;
   extension pieces and leg extensions (`ohl_structures::extensions`,
   the rule in the user guide) in the FE model of each placed structure;
   `missing(level)` for poles and extensions; the edits (`pole`,
   `extension_edits`, one undo step each) and the app's **Body › Pole**,
   **Design › Pole** (a hollow cross-section of the cross-section library next to
   the file, or the wall typed) and **Body › Extensions** (the piece by
   hand or built in one go, legs, the view standing on an extension)
   ([user guide](../user-guide/structure-app.md#poles-and-extensions)).
   *Built* (U4): the FE results panel solving on an extension, the pole
   check in it, wind bands raised by an extension.
4. **Load-case recipes**: `[[load_case]]` (format version 3), evaluation
   in the core from the line values at each placed structure, the load
   table (editable, CSV / XLSX import and export), surface loads; the
   FE results panel and `ohl structures check`; the report.
5. **Cross-section library**: the library kind in the project file, cross-sections
   referred to by id, typed cross-sections and materials; surface areas from
   the profiles; body wind (decision 10). *Built:* the format and
   properties, the project library kind `sections`, ids resolved in the
   library first by the FE model, the member wind in the FE check.
   *Open:* typed cross-sections edited in the structure application, member
   groups.
6. **Replace structure** in the line application (§1.6).
7. **Exports**: IFC with member solids, steel-detailing and analysis programs; other tower programs later.
   *Built*: `ohl-structure-export` (File › Export, tool
   `ohl-structure-export`): IFC4 (members with profile solids and the
   structural analysis model with load cases), IFC2x3 Coordination View
   2.0 (members, for a detailing program's IFC object conversion), SAF 2.2.0
   (xlsx) and CSV tables ([Structure exports](structure-exports.md)).
   The application exports the Load table panel's table (a placed
   structure's line values and typed values). *Open*: imports tried in
   another program; other tower programs.
8. **Code checks**: the plugin interface for member and joint checks
   (open question 1), then a first code (e.g. SIA 263); capacity
   derived from a level 3 design.
9. **Structure–line interaction levels 3 and 4**: the stiffness matrix
   at the suspension points from level 3 structures, then the coupled
   solve (ROADMAP).
10. **Benchmarks** with published references, alongside every phase.

## Open questions

1. **Code-check plugin interface** (e.g. SIA 263): which data the core
   hands to the plugin (member forces, buckling lengths, cross-sections, joint
   details), what comes back (usage per member and joint, formulas),
   and whether any engine change is needed for it — asked before it is
   built.
2. **Stored load overrides** (settled 2026-10-17, §4.1): a load file per
   place, kept in the project (kind `loads`). When the line changes, the
   typed values stay over the new evaluation (the evaluated value kept
   beside them).
