# Data model map

A map of the data OpenOHL keeps: the line model, the libraries it refers
to, the files they live in, how objects point at each other, and what a
plugin sees. It is written for plugin builders and contributors; the user
guide pages linked below give the full field lists of each file.

> **Keeping this map up to date.** Any change to a model struct
> (`ohl-line`, `ohl-structures`, the library crates), a file format
> (line file, OHL XML, library files, the project file's tables) or the
> plugin interface (`crates/ohl-plugins/wit/*.wit`, the export model, the
> Python `ohl` module) updates this page **in the same commit**. If a
> field is added, renamed or removed, or a reference changes from an id to
> a label, the diagram and the tables below change with it.

Known weak points and proposed changes: [Data model review](data-model-review.md).

All examples on this page are invented (structures `T1`, `T2`, sections
`S1`, a system `sys-A`, …).

## Overview

```text
                                  LINE  (.ohl.json / OHL XML / project rows)
 ┌──────────────────────────────────────────────────────────────────────────────────┐
 │ meta: name, crs, display, systems[], creep_stretch, bimetallic, solver,          │
 │       defaults, circuit_starts[], structure_loads, electrical                    │
 │                                                                                  │
 │  Route {id, name} ◄──route── Alignment ──points[] (ids)──► AlignmentPoint {id}   │
 │     ▲                     (one route)      (a point: 0..n alignments) structure?┐ │
 │     └────────── routes[] (given) ── Structure ◄─────────────────────────────────┘ │
 │                                        │  id, position, orientation_deg          │
 │                                        │  type ─────────────────────────────┐    │
 │                                        ├─ points[]  StructurePoint (label)  │    │
 │                                        ├─ wires[]   WirePoint (label, set?) │    │
 │                                        └─ strings[] InsulatorString         │    │
 │                                              top ─► point/wire label        │    │
 │                                              wire ─► wire label             │    │
 │                                              insulator ──────────────────┐  │    │
 │                                                                          │  │    │
 │  Section ──points[] : SectionPoint {structure, set}──► WirePoint.set     │  │    │
 │   │  cable ───────────────────────────────────────────────────────┐      │  │    │
 │   │  system ─► System.id,  phase_label ─► SystemPhase.label       │      │  │    │
 │   └─ stringing, span_points[] {label, span (index), at, object?}  │      │  │    │
 │        spreads[] {from, to ─► span point labels}, calculation     │      │  │    │
 │                                                                   │      │  │    │
 │  Spacer ── a / b : {section, point} ─► SpanPoint.label            │      │  │    │
 │   └─ insulator ──────────────────────────────────────────────────┼──────┤  │    │
 │                                                                   │      │  │    │
 │  Jumper ── from / to : JumperEnd — SectionPoint ─► WirePoint.set  │      │  │    │
 │   │      or {tap: {span: {section, point}}} ─► SpanPoint          │      │  │    │
 │   │      or {tap: {jumper: {jumper, point}}} ─► JumperPoint       │      │  │    │
 │   │  cable ───────────────────────────────────────────────────────┤      │  │    │
 │   ├─ points[] JumperPoint {label, at, object?}                    │      │  │    │
 │   │     object.hold (a support): strings {structure?, wire ─► WirePoint.label} │
 │   │                              or string {point, length, weight}       │  │    │
 └───────────────────────────────────────────────────────────────────┼──────┼──┼────┘
                                                                     ▼      ▼  ▼
                         LIBRARIES (TOML, units in the values)   .ohlw   .ohli .ohls
   .ohlload point and spread load types ◄── LineObject.type, SpreadObject.type   │
   .ohlwc weather cases ◄── creep_stretch names, LoadCase.weather          ▲     │
   .ohllc load cases                                                       └─────┘
                                                        TypeString.insulator ─► .ohli id
   .ohls StructureType: point[], wire[], string[], extension[], node[], member[],
                        section[], material[], load[], load_case[], capacity, wind,
                        model ─► .ohlfe, pole ─► .ohlp (or [pole] in the file),
                        extension[].node[]/member[]/leg[] (pieces below the feet),
                        template (built from ─► .ohlpt)
   .ohlslc structure load cases (load_case[]; structure application) ─► labels of
                        .ohls points, sets, member ids; line_case ─► LoadCase.name
   .ohlpt PylonTemplate: parameter[], attachment[] (slot, across, up),
                        earth[], strings ─► builds a level-1 .ohls
   .ohlsec SectionLibrary: material[], section[] (shape, dimensions,
                        material ─► material id) ◄── member section
                        (.ohls / .ohlfe; the library first, then [[section]]),
                        wind (drag per shape name, face drag, shielding)
```

Arrows point from the referring field to what it names. Everything inside
the box belongs to one line; the libraries are shared and referred to by
**id** (or, for weather cases, by **name**).

## Conventions

### Units

- **Line model** (`.ohl.json`, OHL XML, project rows): plain numbers in
  SI — metres, newtons, N·m², m², °C for temperatures. A field name says
  when it is not: `_deg` degrees, `_kv` kilovolts, `_hz` hertz. Colours
  are `#rrggbb`.
- **Library files** (TOML): values carry their unit as text
  (`"12.5 m"`, `"1.4 kN"`, `"400 mm²"`, `"20 °C"`), converted to SI on
  reading (`ohl_core::units`; a plain `f64` field reads so through
  `units::si::de`); a plain number is SI. A bare number is otherwise a
  factor or a count. Exceptions, where the key or the file says the unit:
  angles in degrees (`tilt`, `_deg`: a plain number is degrees, text may
  give `"0.17 rad"`, `units::si::de_degrees`), `_kv` keys, prices (in the
  cost file's currency).
- Inside the program every quantity is SI ([Conventions and
  units](../theory/conventions.md)).

### File headers

Every OpenOHL file may start with `format` (its id, e.g. `ohl-wire`) and
`version`; both optional. A file of another format is refused; any
version up to the program's is read (older ones converted), a newer one
refused ("written by a newer OpenOHL") — one check for all,
`ohl_core::files::check_header` (the line file and OHL XML, check
reports and colour palettes too; plugins' own files, as `.ohlpd`, the
same way). A key a format does not know is refused, naming it (a typo is
said, not left out); `crates/ohl-cli/tests/format_typos.rs` checks every
library format (the structure application's in
`apps/structure/ohl-structure-tests/tests/format_typos.rs`). Objects on points are read through `LineObjectFile` /
`ObjectTypeFile` for that (a flattened shape would let keys through), a
size of another shape or a missing size said too.

Typed and read numbers are finite (2026-10-26):
`ohl_core::units::parse_finite` refuses `nan`, `inf` and numbers that
overflow (`1e400`); a quantity with a unit (`"1e400 m"`) is refused as an
invalid number; the structure edits (`ohl-structure-edit`: item fields,
`[analysis]`, the file's own lengths) refuse them instead of writing TOML
`nan`/`inf` or 0 m; LandXML points that are not finite are refused.

Files, extensions and format ids carry the `ohl` names only
(`ohl_core::files`: `check_header`, `has_extension`,
`files_with_extension`, `is_project_file`, `line_file_stem`). The names
of before 2026-10-18 (the `ols` extensions `.ols`, `.ols.json`, `.olsw`,
`.olss`…, the format ids `ols-wire`, `ols-structure`…, OLS XML's
`ols-line`, plugin manifests `ols-plugin.toml`, `OLS_…` environment
variables) are no longer read: removed 2026-10-26 before the first
release (roadmap U12). Plugins of the `ols:plugin` interface (sdk 0.2)
are refused. The plugin interface's versions and what keeps them
(2026-11-07): the manifest's `sdk = "0.3"` is the version; the WIT
package keeps the name `ohl:plugin@0.1.0` until a breaking change; the
released interface is frozen in `crates/ohl-plugins/wit-frozen/0.3/` and
compared on every check ([ADR-0012](../adr/0012-plugin-interface-versions.md);
nothing a plugin sees changed with it). Kept on purpose, being on-disk or protocol names rather
than readers of old files: the project table `ols_meta`, the ALPN
`ols/2`, the data folders `OLS CAD` / `OLS Structure` and the window
storage keys `ols-structure-…` (and the web client's `ols.…` browser
storage keys).

New optional fields raise the format version (review rule 2.12: an older
program then says "newer version", not "unknown field"). The string
limits (`max_load`, the swing limits — optional, **not checked** by
OpenOHL, for plugins and reports) did so: line file **6** (OHL XML follows
it), `.ohls` **4**, `.ohli` **2**. The `.ohls` and `.ohli` writers write
the newer version only when a limit is given (an older program still
reads the rest); the line file is always written at the current version. The glossary
(2026-10-21) made `.ohli` **3**: two strings to one clamp are written
`kind = "suspension"` with `legs = 2` (only such an assembly is written
at 3; `kind = "v_string"` of the versions before is read the same); the
structure templates' format id became `ohl-structure-template`
(`ohl-pylon-template` read), their folders `structure-templates`,
`Structure templates`, `StructureTemplates/` (the names before read);
the sag-tension job's `[fe.two_strings]` reads `[fe.v_string]` too; the
command-line `--wire-type` reads `--cable`. File keys are otherwise
unchanged: the [glossary](../user-guide/glossary.md) gives each term's key.
A new value counts as a new field: the member kind `tension` (a truss in
tension only) made `.ohls` **5** and `.ohlfe` **2**; the structure
application raises a file to 5 when it makes a member `tension`
(`ohl_structure_edit::set_member_kind`), and only then. The body
generators' options (`[[generator]]`: a panel's `width`, `redundants`,
`plan`, a body's `plan_auto`, a crossarm's `one_arm`) made `.ohls`
**6**, though the core keeps those tables unread: an older structure
application would refuse them as unknown fields. The structure
application raises a file to 6 when it writes a generator using one
(`ohl_structure_edit::body::set_generator`, `Generator::uses_options`,
`body::OPTIONS_VERSION`), and only then; a file without a `version`
stays without, and a version is never lowered. Version 6 is one
unreleased raise: it also covers anything else added to `.ohls` before
the next release (another new field then needs no 7): a joint's `kind`
and an attachment point's `offset` (2026-10-21; `ohl_structure_edit::
links`, `LINKS_VERSION`, writes 6 only when it writes one) are such
fields, and so is a member's
`bending` (a `tension` member that also bends: a tower program's
tension-only beam) and `check = false` (a modelling member, not checked)
are such fields, and made `.ohlfe` **3**; nothing in OpenOHL writes them
but importers. The project's
**height datum** (2026-10-21; `ohl_line::HeightDatum`, `Line::height_datum`)
made the line file **7** (written at 7 since; the key only when one is
given; files of 6 and before read without one), OHL XML follows it (root
attributes `height_datum`, `height_datum_epsg`), and the project file's
`session_schema` **5** (the `meta` row `line` may hold it: an older
program refuses the project instead of dropping the datum when it writes
that row; opening a project of schema 4 or before converts nothing, it
has none, and marks it 5). The attachment points' links (2026-10-21:
`StructurePoint` `joint`, `offset`, `unlinked`) are covered by the same
unreleased raise — line file 7 (OHL XML: `<point>` attributes `joint`,
`offset_x`, `offset_y`, `offset_z`, `unlinked`), project schema 5 (the
structure rows carry the keys); `.ohlfe` stays 3 (a separate model's
nodes still name their points by `attachment`); the line-values JSON and
the export model JSON are unchanged (the points keep their labels).
Nothing written before changes when read: links are worked out in
memory and written with the object when it is next saved. The export
model's member `kind` (CSV, IFC `TENSION_MEMBER`, SAF "Tension only")
takes the value `tension` too; `ohl_structure_link::ExportMember` carries
`bending` and `check` (`ExportMember::is_frame`): a tension-only member
that bends is exported as a frame (IFC4 `RIGID_JOINED_MEMBER`, SAF
Standard) with the tension-only behaviour as information (IFC
`OHL_Member` `Kind`, `Bending`; SAF `Layer`), a member not checked is
flagged (`OHL_Member` `Check` false, SAF `Layer`), and `members.csv` has
the columns `bending` and `check` (1/0) after `kind`
([structure exports](structure-exports.md)).
The export model JSON stays version 1 (keys only added: `strings` per
case) and the plugin WIT is unchanged (no record passes strings).
The structures' own wind in a load case (`[case.structure]`) made
`.ohllc` **2** (written at 2 only when a case gives a value there:
`ohl_loads::LoadCases::written_version`, the design-code plugins'
`ohl-code-common`, OHL-Design's library page on save); the line
quantities `structure_wind` and `structure_wind_factored` of the
structure recipes made `.ohlslc` **2** (`StructureLoadCases::new` writes
2 only when a load takes one) and `.ohls` **6** (OHL-Structure raises a
file to 6 after any edit leaving such a load,
`ohl_structure_edit::raise_for_line_pressures`); the line-values file
went to **4** (`structure_wind` in the values, always written). The two parts of a structure load case (`[[load_case.line]]`,
`[load_case.structure]`, 2026-10-25) made `.ohlslc` **3** and `.ohls`
**7** (each written only when a case has them). The shares of the
back and ahead spans' pull (owner, 2026-11-10: `back`, `ahead` of
`[[load_case.line]]`, and the line quantities `pull_back_transverse`,
`pull_back_longitudinal`, `pull_ahead_transverse`,
`pull_ahead_longitudinal`) made `.ohlslc` **4** and `.ohls` **10** (each
written only when a case uses them: `ohl_structure_loads::written_version`,
`ohl_structure_edit::raise_for_line_pressures` → `SPAN_SHARES_VERSION`)
and the line-values file **6** (the four pulls among the values, always
written; **7**, 2026-11-11: each set's part of them at a point holding
several sets, written only when the values hold such parts,
`PlacedValues::needed_version`); a structure taking no load case (`Structure.no_load_cases`)
made the line file **14** (written only by a line with one,
`Line::to_json`; OHL XML `no_load_cases="true"` on `<structure>`).

### Coordinates and angles

| Frame | Axes | Where it is used |
|---|---|---|
| Project (global) | x east, y north, z up, metres, in the line's `crs` | Every point in the line model: structure `position`, structure points, wire points, `members`, alignment points, jumper supports. |
| Structure local | origin at the structure's ground centre; **x to the right** of the line looking ahead, **y ahead**, z up (right-handed) | Structure types (`.ohls`), structure loads in the structure's axes, the editors. |

- `Structure.orientation_deg`: the direction of the local **x** axis
  (the right of the line), degrees **counter-clockwise from east**
  (mathematical sense). Ahead (local +y) is a quarter turn
  counter-clockwise from it.
- `ohl_structures::structures::to_global(position, rotation_deg, local)`
  rotates a local point counter-clockwise by `rotation_deg` and adds
  `position`; `to_local` is its inverse. A placed type's points are also
  raised by the extension's `dz`, the type's `foot` and the structure's
  `height_adjust`.
- `Display.ahead_deg` is **only a display convention**: the user interface
  shows plan directions around a structure (string directions, the
  compass) as angles turning **clockwise seen from above**, and
  `ahead_deg` is the angle that means ahead (default 0: ahead 0°, right
  90°; with 270: right 0°, ahead 270°). A displayed angle `a` points to
  the local direction `(sin φ, cos φ)` with `φ = a − ahead_deg`. No stored
  coordinate depends on it; a plugin reading or writing points ignores it.
- Stations and offsets are horizontal; offsets are positive to the right
  looking ahead. A route's line angle is positive turning right.
- The wind side ("from the right") is taken looking ahead along the
  nearest route segment ([Conventions](../theory/conventions.md)).

## The line (`ohl_line::Line`)

Source: `crates/ohl-line/src/lib.rs`. One line per `.ohl.json` file and
per project line in a project file. Full field reference: [Line
files](../user-guide/line-file.md).

| Field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `format` | string | — | `"ohl-line"` | — |
| `version` | u32 | — | file version (now 12, 2026-10-31: a torus among a string's own `shapes` (`ShapeKind::Torus`, `tube`), written only when one is (`BEFORE_TORUS_VERSION` 11) — 11, 2026-10-27: the electrical settings `electrical`, written only when not the defaults (`BEFORE_ELECTRICAL_VERSION` 10) — 10, 2026-10-25: the simplified in-the-blocks option `solver.blocks = "equal_h"` and a jumper's `tension` (a riser) — 9, 2026-10-25: a section's `wind_factors` and `structure_loads.broken_cases` — 8: a string's own `shapes` — each written only when used, `Line::to_json` writing the oldest version that holds the line (7, 8, 9 or 10; `BEFORE_SHEAVES_VERSION` 9); 7: the optional `height_datum`, `circuits` — 6: a string's optional limits `max_load`, `max_swing_deg`, `max_swing_across_deg`, `max_swing_along_deg`, not checked — 5: jumper ends as `JumperEnd`, places with one key, jumper supports as points, a section's `circuit_name`; versions 1–6 are read and upgraded — version 4 files' `from_tap` / `to_tap`, `support` / `supports`, places with several keys, and `loads` / `spread_loads` of before points and objects) | — |
| `name` | string | — | line name | — |
| `crs` | string | — | coordinate reference system, e.g. an EPSG code (informative) | — |
| `height_datum` | `HeightDatum?` | — | version 7: the heights' vertical datum, `{name, epsg}` — its name (`"LN02"`), the EPSG code of its vertical system (`5728`), either or both (an empty one refused; informative, nothing converted; `HeightDatum::parse` reads them as typed, `label()` → `LN02 (EPSG:5728)`); left out when none. Set in Terrain › Coordinate system › Define… with the CRS (one line-settings edit) or `put` `meta` `line`; the sheets' `{height_datum}` takes `Line::height_datum_label()` (`SheetInfo::height_datum`) first, then the CRS's vertical part (`ohl_drawing::height_datum_of`), then the template's `[if_empty]` | — |
| `routes` | `[Route]` | — | routes: paths of the line, made of alignments | — |
| `alignments` | `[Alignment]` | — | alignments, in order; each on one route, through point ids | route ids, point ids |
| `alignment_points` | `[AlignmentPoint]` | — | alignment points (PIs), each on any number of alignments | structure ids |
| `structures` | `[Structure]` | — | lattice towers and poles | — |
| `sections` | `[Section]` | — | wire sections | structures + sets |
| `jumpers` | `[Jumper]` | — | jumpers between wire points | structures + sets |
| `spacers` | `[Spacer]` | m, N | interphase spacers: an insulator between span points of two sections of one span (`a`, `b`: section id + span point label, each point holding a clamp object — required; files before give bare ends a clamp of no weight, `Line::clamp_spacer_ends`; insulator label or weight, wind area, stiffness; `length` (none: the still-air distance at stringing); `compression` limit, N, from the project defaults when made; both sections by finite elements, ruling span refused); solved by `solve::solve_spacer_group` (the sections joined, in turn, Aitken's relaxation), reported on the section at `a` (`SectionShape::spacers`: ends, axial force, slack, over its limit) | sections + span point labels |
| `defaults` | `Defaults` | N | the project's default values (Criteria › Default values; `ohl_line::DEFAULT_FIELDS`; the program's in `Defaults::PROGRAM`, the one place they are given — `ohl_thermal` and `ohl_clearance` take theirs from it): what new objects start with and what the calculations take where nothing else gives a value — `spacer_compression` (10 000 N), `jumper_exit_angle_deg` (30°), `jumper_stiffness_factor` (0.2), `jumper_clamp_length` (0.1 m, new clamps), `insulator_stiffness` (EA 5·10⁷ N), `air_density_factor` (Q 0.613), `thermal_emissivity` / `_absorptivity` (0.8), `thermal_roughness` (0.1), `thermal_albedo` (0.1), `thermal_time_step` (10 s), `clearance_wind_step` (50 Pa), `clearance_vertical_band` (0.5 m), `clearance_search_margin` (5 m), `clearance_zone_width` / `_depth` (1 m / 0.2 m, new windows); exported / imported as `.ohldef` (`ohl-defaults`, TOML, sections `[spacer]`, `[jumper]`, `[insulator]`, `[weather]`, `[thermal]`, `[clearance]`, values in their units); OHL XML `<defaults section_key=…>` (only those not the program's). Display settings stay per user | — |
| `systems` | `[System]` | — | electrical systems | — |
| `circuits` | `[Circuit]` | — | version 7 (2026-10-21): the circuits, named groups of conductor paths ([below](#electrical-systems-and-circuits-electricalrs)); left out when none; project `meta` row `circuits`; OHL XML `<circuit id name system description>` | system ids |
| `display` | `Display` | — | phase colours, `ahead_deg` | phase labels |
| `creep_stretch` | `CreepStretch` | — | weather cases for creep / load stretch; `hot` (line file 15, 2026-11-11): the time at high temperature, `[HotStage { temperature °C, hours }]` (creep at elevated temperature, below: wire library version 3) | weather case **names** |
| `bimetallic` | `BimetallicModel` | Pa | outer-strand behaviour of bimetallic wire types above the transition temperature, for wire types that do not say. Derived, not stored (2026-11-11): the model each wire type is computed with (`ohl_cables::Cable::bimetallic_model` → `BimetallicUse`: `Unlimited`, `Limited(stress)`, `BirdCage`), the line's notes of it (`ohl_line::bimetallic_notes`) and, per solved case, the flags of the spans past the knee point in `SectionShape::flags` (`ohl_line::bimetallic::past_knee_flag`, from `Mechanics::outer_state`) | — |
| `solver` | `SolverOptions` | — | how the calculations are done (Criteria › Calculation options): `jumper_loads` — `estimate` (default: weight, ice and wind of the jumper on the clamps) or `pull` (the solved jumper's end forces, sections and jumpers solved in turn); `taps` — `loop` (default: a jumper's own force at its tap put on what it hangs on, solved in turn until the tap settles) or `once` (the estimate, one calculation); `loads_per` (2026-10-21, `LoadsPer`, written only when not the default) — `stretched` (default, exact: the finite elements carry the ice and wind per metre of the stretched wire, `CableElement::stretched_load`, a fixed point) or `unstretched` (per unit of unstressed length, the former treatment, as other programs: a Simplification, about T/EA ≈ 0.1 % less; in the solver `solve::Reference::loads_per`, set from the line in `solve_fe`, and the jumpers' `solve_jumper_in`; the closed forms unaffected); in the project's line settings and OHL XML `<solver taps loads_per>`; `ohl line --loads-per` overrides it; `blocks` (2026-10-25, `InTheBlocks`, line file version 10, written only when not the default) — `sheaves` (default: a section in the blocks shares its length over frictionless sheaves, the wire's tension equal on both sides of each suspension clamp; `ohl_sagtension::sheaves::share`, in the solver `solve::over_sheaves` and `settled_over_sheaves`, the stringing state `in_the_blocks_at_stringing` for `clip_lengths` and the curves' zero-strain lengths; `solve::Reference::blocks`, set from the line in `solve_fe`) or `equal_h` (simplified: one horizontal tension, the strings plumb along the line, the model before; the ruling-span method keeps it either way); the sagging table follows it (`SaggingTable::blocks`, `SaggingSpan::horizontal_tension`, `TensionSection::solve_over_sheaves`, `NonlinearSection::solve_over_sheaves`); OHL XML `<solver blocks>`; `ohl line --blocks` overrides it; `ohl sagtension`'s `[sagging] blocks` (default `sheaves`) and the validation calculation `sagtension.sagging`'s input `blocks`; the app's results keep it in `revision::Revision::loads_per` (the line's hash taken without it, so a change is named on its own) and saved check reports in `parameters.loads_per` (vegetation, safety volumes); `local_spans` (2026-11-03, `ohl_line::local::SpanRule`): `worst` (default, not written) or `mean` — how a span takes its two structures' local factors in `local::localized`; set in Line › Criteria › Calculation options (a radio), through the same session command as the others (`Session::solver_command`: a put of the `solver` settings row, one logged undoable command; an oplog row without it reads `worst`) | — |
| `circuit_starts` | `[SectionPoint]` | — | conductor path ends that are starts (power flows in); each a point of a section, given once (validated) | structures + sets |
| `structure_loads` | `StructureLoads` | — | what the line sends to the placed structures, the cases of their line-values file (Criteria › Loads for structures, a tick menu; since 2026-11-03 also each placed structure's set of the load case library, `library_cases` of the line-values file, and a broken-wire case sent breaks all its `broken_wires()` and leaves out its removed conductors, named by `ohl_structure_link::wires_text`): `weather_cases` — the weather cases sent, by name (default none), each a line result unfactored, a wind case twice (`<case> — wind from the left` / `— right`); `condition` — the wires' condition in them; `broken_cases` (version 9, 2026-10-25, roadmap O9) — the line's load cases with a broken wire sent, by name, after the weather cases, each its weather case with that wire broken, unfactored, `<weather case> — <section> span <n> broken` (`ohl_structure_link::structure_cases_with`, `broken_result_name`, tag `broken wire`); `load_cases` — read, not used since 2026-10-25; a name no longer in the line (or no longer a broken-wire case) is skipped and named in the file's note; `type_load_cases` (2026-11-03, per-pylon sets, `BTreeMap<type id, [case name]>`) — a structure type's default load cases of the line's load case library, taken by its structures listing none of their own (`Line::load_cases_for(structure) -> &[String]`: the structure's `load_cases`, else its type's, empty = every case; `Line::takes_load_case(id, case)`, `has_load_case_sets`; names checked against the library when the loads are computed, `ohl_loads::load_case_set_notes`; OHL XML `<type_cases type><case name/></type_cases>` in `<structure_loads>`). Line file `{"structure_loads": {"weather_cases": [...], "condition": "creep", "broken_cases": [...]}}` (left out when default); project `meta` row `structure_loads`; OHL XML `<structure_loads load_cases="all\|listed">` with `<load_case name>`, `<weather_case name>` and `<broken_case name>` | load case and weather case **names** |
| `electrical` | `ElectricalSettings` | — | the line's electrical settings for the line parameters (version 11, 2026-10-27, roadmap O18; Checks › Electrical › Line parameters): `soil_resistivity_ohm_m` (Ω·m, homogeneous earth; none: `DEFAULT_SOIL_RESISTIVITY_OHM_M` 100 Ω·m **assumed**, the IEEE distribution test feeders' value, said in every result), `earth_return` (`carson_series` default, `complex_depth`, `carson_first_terms`), `mean_height` (`solved` default: the solved wire's average over the span; `two_thirds_sag`: attachment height minus 2/3 of the sag), `earthing_resistance_ohm` (Ω, 2026-10-29: the structures' earthing where they give none; none: `DEFAULT_EARTHING_RESISTANCE_OHM` 10 Ω **assumed**, said). Line file `{"electrical": {...}}` (left out when default); project `meta` row `electrical`; OHL XML `<electrical soil_resistivity_ohm_m earthing_resistance_ohm earth_return mean_height/>` | — |
| `condition_cases` | — | — | **not stored**: the weather cases named by `creep_stretch`, set by the program solving the line | — |

**Plugin values** (`fields`, `ohl_line::Fields`): structures, sections,
jumpers, systems, alignments and routes carry a map `plugin.column →
text`, the values of the table columns plugins add (`[[column]]` in a
plugin's manifest, [Writing plugins](plugins.md#table-columns)). Empty
maps are not written. Kept with the object through every edit, written
to OHL XML as `<field key value>`; shown (unused) where the plugin is not
installed. A circuit's values are on all its sections. A check plugin
reads its own (`ohl_clearance::phase_distance` passes the sections'
values to it).

### Route, Alignment and AlignmentPoint (`route.rs`)

| Entity.field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `Route.id` | string | — | unique id (`"route 1"`, `"north"`) | — |
| `Route.name` | string | — | display name | — |
| `Route.description` | string | — | free text | — |
| `Alignment.id` | string | — | unique id (`"alignment 1"`) | — |
| `Alignment.name`, `.description` | string | — | display name, free text | — |
| `Alignment.route` | string | — | the route it belongs to (exactly one; required) | `Route.id` |
| `Alignment.points` | `[string]` | — | its points by id, in order (none while laid out; a profile needs two); a point at most once | `AlignmentPoint.id` |
| `AlignmentPoint.id` | string | — | unique id (`"P12"`) | — |
| `AlignmentPoint.position` | `[x,y,z]` | m | the PI | — |
| `AlignmentPoint.structure` | string? | — | tied: follows that structure, must lie under it within 1 mm in plan | `Structure.id` |
| `AlignmentPoint.description` | string | — | free text | — |

A point's place on each alignment it is on is its index in that
alignment's `points`; a point on several alignments (a junction) is one
object with a place in each; a point on none waits to be used. Its routes
are those of its alignments (`Line::point_routes`). Deleting a point takes
it off every alignment (`Line::delete_alignment_point`); deleting an
alignment keeps its points.

**What is on a route** is worked out, not stored (`Line::structure_routes`,
`Line::wire_routes`, `Line::alignment_views`):

- a **structure** is on an alignment and its route when it is tied to
  one of its points, or within 5 m (`NEAR_ALIGNMENT`) of one of its
  points no structure is tied to, or within 5 m of the alignment between
  its points (beside it or within 5 m of an end) with a span of the line
  running along the alignment to it from a structure on it, repeated
  along a run of such towers (owner, 2026-10-31: another line's towers
  near the alignment are not on it); a structure giving its own `routes` is on those (stationed on
  the nearest alignment of each); at a junction it is on every route
  meeting there;
- a **section** is on the routes of its structures (one running from a
  route into another is on both); a **jumper** on those of its two ends;
  strings, wire points and counterweights on those of their structure.

`AlignmentView` (id, route, points, structures in station order, and
`away`: those of them neither tied to one of its points nor within 5 m of
it — on its route because they give it themselves, such as a connection
beside the line — and those within 5 m but not tied, with a span of the
line along the alignment passing over them and none of their own along
it, such as another line's tower under a span) is what the profile, drawings, stationing and spotting
work on; each structure on an alignment gets a station there (`Station`:
station, offset, line angle, ahead span — computed, not stored). The
**run** along the alignment (`AlignmentView::run`) leaves the structures
away out: line angles and spans ahead are taken along it (a structure
away has 0 for both), and the profile's spans are those between
consecutive structures of the run (`AlignmentView::consecutive`), those
between two structures of the run that stay within 5 m of the alignment
over their whole length (`AlignmentView::along`: another line's structure
standing under a span does not break it), plus those to a structure away
(projected), so a connection whose station falls between two structures
does not break the span between them. The drawings
mark it too (`ohl_drawing::Mark::away`, computed; `Mark::beside`: a
structure of the line within the line width not on the route, drawn with
the spans and jumpers passing within it, cut there) and keep the
alignment's points by station (`RouteDrawing::alignment`,
`AlignmentMark`: id, station, the structure tied to it or none). What
they draw along the line is limited by the **line width**
(`Options::plan_half_width`, `--plan-width` / `--line-width`, default
40 m; `RouteDrawing::shown`): every structure of the run, a structure
away only within it, wires cut at it, markers within it (the profile CSV
and XML keep everything). Their sheets (`ohl_drawing::sheet_layout`,
drawn by `sheets`) end only at a structure of the run (never at one
away, whatever its offset) or at an alignment point without a structure
(between the first and the last structure), and never within
`Options::break_clearance` (mm on paper, default `BREAK_CLEARANCE_MM` =
2, 0 to `MAX_BREAK_CLEARANCE_MM` = 20, `check_break_clearance`;
`--break-clearance`) of what is drawn of any other structure; never in a
span. The app's sheet preview calls `sheet_layout` with the export
window's settings, which the app keeps in the project per project line
(below, `app.drawings.<line>`); its Profile page uses the same line width
(no per-user profile corridor any more: an older `profile_corridor` in
the remembered view settings is ignored).

**Files of version 2** held `routes` that were their own alignment
(`alignment`, `structures`) or followed their structures, and the line's
own `alignment` without routes: `Line::upgrade_routes` (on reading) makes
each a route and an alignment of the same id — points tied under the
structures for one that followed them, the line's own named `main` — and
gives a structure its routes itself where the alignments would not find it
on them. **Files of version 3** held alignments on one or more `routes`
with their points inside (`[x,y,z]` or `{position, structure, routes}`):
the points become alignment points (`P1`…), the same place on several
alignments (1 mm, compatible ties) one shared point; an alignment on
several routes one alignment per route (`a`, `a (route)`); a point's own
routes go to its tied structure. Projects are converted once when they
open (`ols_meta` keys `routes_alignments`, `alignment_points`).

### Structure (`lib.rs`, `assembly.rs`)

| Field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `id` | string | — | unique id | referred to by everything below |
| `position` | `[x,y,z]` | m | origin, centre at ground | — |
| `orientation_deg` | f64 | ° CCW from east | direction of local x (right of the line) | — |
| `type` (`structure_type`) | string? | — | structure type | `.ohls` `id` |
| `extension` | string? | — | extension of the type | `.ohls` `[[extension]] name` |
| `description` | string | — | free text | — |
| `height_adjust` | f64 | m | typed: feet above the ground at `position` (may be negative) | — |
| `weather_groups` | `[string]` | — | its weather case groups (2026-10-25): read and kept, **not read by the checks** since 2026-11-03 (a check run notes them once, `ohl_app::local_solve::Local::notes`); not read by the solve; no table column since 2026-11-03 | `.ohlwc` `[[group]] name` |
| `load_cases` | `[string]` | — | 2026-11-03, per-pylon sets: names of cases of the line's load case library this structure takes (empty: its type's default `structure_loads.type_load_cases`, else every case; `Line::load_cases_for`); `ohl_loads::structure_loads` computes its loads only in those cases; not read by the solve (`incremental::same_but_drawing`); OHL XML `<load_case name/>` in `<structure>` | load case **names** |
| `no_load_cases` | bool | — | 2026-11-10 (owner): the structure takes **no** load case — `Line::takes_load_case` false for every case, `Line::takes_no_load_case`, `Line::structures_without_load_cases`; `ohl_loads::structure_loads` computes no loads for it and says so in every case's `notes` (`ohl_loads::TAKES_NO_LOAD_CASE`), `ohl_loads::capacity` lists it not checked with that note (never a pass), `ohl_structure_link::placed_values_with` sends none of the library's cases and says so in the file's `note` (the line results are still sent); refused together with a non-empty `load_cases` (`Line::validate`); line file **14**, written only when true; not read by the solve (`incremental::same_but_drawing`); OHL XML attribute `no_load_cases="true"`; edited in Properties › Load cases › none (`load_case_sets::structures_command`), shown in the Structures table's load cases column | — |
| `local` | `LocalConditions` | factors; m; Pa; m | 2026-11-03 (`ohl_line::local`): `wind_static`, `wind_clearance`, `wire_ice` (factors), `structure_ice` (m), `snow_ground` (s_k, Pa), `snow_altitude` (h₀ for SIA 261 (10), `local::snow_on_ground`), `wind_study` (Pa, above 0; 2026-11-04: the internal clearances' wind at the structure with Use local conditions, `ohl_app::internal_clearances::wind_study`, bounded by the rule file's `[check] wind_study_min` and taken in its `wind_study_cases`, `CheckScope::takes_wind_study`; the weather case groups' `structure_wind` columns copied into it once, `GroupStudy::{of, pending, command}`, one undoable `Command::Batch` of structure puts); `LocalConditions::snow`, `check`; `local::localized(line, Purpose::{Strength, Clearance})` sets each span's wind and ice factors from its two structures' by `SolverOptions::local_spans` (`SpanRule::{Worst, Mean}`), a span's own kept; `local::has_local`; OHL XML `<local wind_static wind_clearance wire_ice structure_ice_m snow_ground_pa snow_altitude_m wind_study_pa/>`, `<solver local_spans>`; not read by the base solve (`incremental::same_but_drawing`). Desktop (2026-11-03): the Structures table's columns `wind static`, `wind clearance`, `wire ice`, `structure ice mm` (×1000 m), `snow s_k kN/m²` (×1000 Pa), `snow h0 m`, `wind study Pa`; the checks' **Use local conditions** (`ohl_app::local_solve`: `prepare(line, ticked, purpose) -> Local` — the line localized when ticked and `has_local`, else the line as it is with no other solve; `Local::{line, applied, notes, sections(line, library, case), solved(library, case), finish(check, took)}`; the solves go through ohl-line's memo of whole solves, keyed by every field of the localized line, the library and the case, so the checks of one purpose share them; `RUNS` keeps each check's last run time with and without for the switch's hover) | `.ohlwc` none |
| `soil_resistivity_ohm_m` | f64? | Ω·m | soil resistivity measured at the structure (2026-10-27; line file version 11 when set; OHL XML attribute `soil_resistivity_ohm_m`); the line parameters take on each span the geometric mean of its two structures' (one measured: that one; none: the line's `electrical`) | — |
| `earthing_resistance_ohm` | f64? | Ω | resistance of its earthing (footing) to remote earth (2026-10-29; line file version 11 when set; OHL XML attribute `earthing_resistance_ohm`; the Structures table's **earthing Ω**); the magnetic field's earth wire network leads the structure's current into the ground through it (none: the line's `electrical`, else 10 Ω assumed; `Line::earthing_resistance`) | — |
| `points` | `[StructurePoint]` | — | points where strings hang | — |
| `wires` | `[WirePoint]` | — | wire points | — |
| `strings` | `[InsulatorString]` | — | strings | labels of points / wire points |
| `omit` | `[string]` | — | typed: the type's points, strings or wire points left out | labels in the type |
| `omit_sets` | `[u32]` | — | typed: the type's wire points with these sets left out, with what hangs only for them | sets in the type |
| `members` | `[[x,y,z]]` | m | stick drawing polylines (views and drawings only) | — |
| `routes` | `[string]` | — | routes it is on, given by itself (else worked out from the alignments, below) | `Route.id` |

A structure without a type holds its own `points`, `wires`, `strings` and `members`; the desktop app edits them as a structure type in local axes (`to_local`/`to_global`) and writes them back with one `Put`. *Make it a type* writes that document as a `.ohls`, sets `type` and empties the structure's own lists. A type built from a structure template records it in `[template]` (`id`, `role` suspension/strain, `string_length`, `string_weight`, `values` by parameter) and has sets from the template's sets (`slot`): suspension = the set, strain: the first string (180°) the set + `first_offset` (20), the second (0°) the set + `second_offset` (40); files before give `back_offset` / `ahead_offset`. Built again, only the header, `[template]` and the points, strings and wire points the template makes (by label) are replaced; the rest of the file is kept (`ohl-app` `pylon_builder::rebuilt`).

| Entity.field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `StructurePoint.label` | string | — | unique among the structure's points **and** wire points | — |
| `StructurePoint.position` | `[x,y,z]` | m (global) | where strings hang | — |
| `StructurePoint.description` | string | — | free text | — |
| `StructurePoint.joint` | string? | — | line file 7 (2026-10-21): the joint of the structure's type the **attachment point** is linked to (`StructureType::joints`); none on a typed structure: as its type's point of the label, else a joint at its position (`library::link_point`, in `Line::resolve`); not used without a type (each point its own joint) | `[[node]] id`, or a joint the points make (`ohl_structures::point_joints`) |
| `StructurePoint.offset` | `[x,y,z]` | m (structure axes) | from the joint to the point: a rigid link, its moment r × F applied at the joint by the structure analysis; written only when not zero. Resolved: position = joint as placed + offset turned with the structure; a written position kept within `assembly::KEEP_POSITION` (1 µm), so a round trip moves nothing | — |
| `StructurePoint.unlinked` | bool | — | written only when true: kept at `position`, the structure's finite-element check refused until it is linked again; also set when resolved if its joint is not in the type (`StructurePoint::is_linked`) | — |
| `WirePoint.label` | string | — | unique among points and wire points | — |
| `WirePoint.position` | `[x,y,z]` | m (global) | position in still air: **placed from its strings** (`Structure::place_wires`); given only for one no string reaches (fixed) | — |
| `WirePoint.set` | u32? | — | set number, unique on the structure: sections and jumpers end here. None: strings only meet here | — |
| `WirePoint.weight` | f64 | N | counterweight hanging at it (whole bundle) | — |
| `WirePoint.description` | string | — | free text | — |
| `InsulatorString.label` | string | — | unique among the structure's strings | — |
| `InsulatorString.top` | string | — | its top: a structure point or another string's wire point | `StructurePoint.label` / `WirePoint.label` |
| `InsulatorString.wire` | string | — | its bottom | `WirePoint.label` |
| `InsulatorString.length` | f64 | m | top to wire point (0 in files written before: taken from the geometry when read) | — |
| `InsulatorString.angle_deg` | f64 | ° | from straight down (0 hanging, 90 level) | — |
| `InsulatorString.direction_deg` | f64 | ° | in plan, structure axes (0 ahead, 90 right) | — |
| `InsulatorString.insulator` | string? | — | its **library label**: the insulator assembly it takes weight, wind area and stiffness from (in a project every own string with a weight gets one: `Line::catalogue_strings`, the app labelling on opening and with each edit; a string without a weight, 0 N, is left unlabelled — an entry's weight is a real weight — and a strain string with a resolved weight of 0, its own or its assembly's, gets the solver's estimate, 2026-10-21) | `.ohli` `id` |
| `InsulatorString.weight` | f64 | N | half at each end | — |
| `InsulatorString.wind_area` | f64 | m² | half at each end | — |
| `InsulatorString.stiffness` | f64? | N (EA) | default `solve::DEFAULT_INSULATOR_STIFFNESS` | — |
| `InsulatorString.compression` | f64? | N | compression it may take (then two-way, flagged beyond); none: tension only | — |
| `InsulatorString.max_load` | f64? | N | version 6: the tension it is rated for — **not checked** by OpenOHL (`StringLimits`, `InsulatorString::limits`); none: not given; a resolved line fills it from its assembly's `max_load` where none is given | — |
| `InsulatorString.max_swing_deg`, `.max_swing_across_deg`, `.max_swing_along_deg` | f64? | ° | version 6: the swings it may take (from straight down any direction; across; along the line), 0–180 — not checked; none: not given; filled from its assembly's on resolve. Reported with the solved string (`solve::StringState::{limits, swing}`, `jumper::JumperStringShape::{limits, swing}`; `ohl_line::swing(orientation, top, bottom) -> Swing {total_deg, across_deg, along_deg}`) and `Line::string_limits(structure, label)` | — |
| `InsulatorString.strain` | bool | — | a strain (dead-end) string, else suspension: its set is a strain point | — |
| `InsulatorString.description` | string | — | free text | — |
| `InsulatorString.shapes` | `[StringShape]` | m | line file **8** (2026-10-25, roadmap O6): its own volumes, each replacing its assembly's shape of the same name or added (`StringVolumes::with_overrides`; one overridden as `insulated` is not checked); the keys of an `.ohli` `[[shape]]` in SI numbers. `Line::to_json` writes version 8 only when a string has some, else 7 (`BEFORE_SHAPES_VERSION`); a torus among them (`kind = "torus"`, `tube`, 2026-10-31): version 12; OHL XML: `<shape …/>` children of `<string>` (attribute `tube`) | — |

The string graph: two strings may end at one wire point; strings hang
from strings (a string below two, two tension strings from a string's
wire point: combinations, no types). A wire point no string
reaches is fixed to the structure. A string's **length and direction**
place its wire point (`Line::place_strings`, when a line is read or
resolved): top + length × direction; a wire point of two strings where its first
two legs' lengths meet, the point of that circle nearest where the first
leg's direction points (their directions follow).
Strings without a length (files written before) take it and their
angles from where their wire point is, so nothing moves. There is no
"ahead"/"back" for a string: its angles alone give its direction. Version 1 files gave one `attachments`
entry per set and phase; they are read through `AttachmentV1` and
`set_from_v1(set, phase)` (`set` for phase 1, else
`set + 1000·(phase + 1)`).

**Typed structures.** With a `type`, the file keeps only the overrides:
`Line::resolve` (`crates/ohl-line/src/library.rs`) places the type
(`to_global` with `orientation_deg`, the extension, `height_adjust`),
then the structure's own items replace the type's **by label**, a wire
point with a set replaces the type's wire point **with that set** (and
what moves with it), `omit`/`omit_sets` leave items out, weights,
stiffnesses and wind areas come from the insulator library where a
string has none (an assembly of two strings shared by them), and the
wire points are placed from the strings. In a weather case the jumpers
load the clamps of the sections at their ends, as the line's
`solver.jumper_loads` says: an estimate (`jumper::end_loads`: weight,
ice and wind of the jumper between an end and its first support) or
the solved jumper's own pull (`solve::jumper_pull`) — not at stringing. The resolved line is what solvers and exports use; the stored
line and the project rows are not resolved. A typed structure's own strings may hang from the
type's points: as stored, such a top is not checked and its wire point
is not placed (`Structure::check_strings_as_stored`, `place_wires`);
once resolved every structure is checked in full (`check_strings`).

### Section (`lib.rs`, `condition.rs`)

| Field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `id` | string | — | unique id | — |
| `description` | string | — | free text | — |
| `cable` | string | — | wire type | `.ohlw` wire type `id` |
| `circuit_name` | string | — | the name of the circuit (`Line.circuits`) its conductor path is in (its sections give the same; empty: none); files before version 5: `circuit` | `Circuit.name` |
| `phase_label` | string | — | phase label, e.g. `L1` | `SystemPhase.label`, `Display.phase_colors` key |
| `system` | string? | — | system this conductor path carries; given once per path, the other sections of the path follow; none on the whole path: earthed | `System.id` |
| `bundle` | u32 | — | subconductors (1: single wire) | — |
| `bundle_spacing` | f64? | m | distance between neighbouring subconductors | — |
| `bundle_rotation_deg` | f64? | ° CCW looking ahead | rotation of the arrangement | — |
| `points` | `[SectionPoint]` | — | attachment points in line order; first and last are strain points | structure id + set |
| `stringing` | `Stringing` | — | how it is strung (below) | — |
| `span_points` | `[SpanPoint]` | — | labelled points on spans (`span`, `at`: a place — `fraction` (horizontal, from the back point), `distance` (m) or `station` (m, of the line along the alignment both structures stand on, turned into a fraction when solved, `Line::section_with_fractions`; with no alignment through both, at the back point) — one of them, `{"distance": 30}` (`SpanPlace` enum; files before version 5 giving several are read with a station over a distance over the fraction), each with its `object` (one per point: a body or a clamp, `objects::LineObject`) or bare (a spread's end); nodes of the FE model, reported solved (`SectionShape::span_points`) | span index; spacers and spreads name them |
| `spreads` | `[SpreadObject]` | N/m | objects spread between two points of one span: `label` (optional), `from`, `to` (point labels), `type` (library id), `load` (downward, N/m of conductor: the whole bundle's, shared by its subconductors), `per_subconductor` (optional, default false, written only when true: `load` is on each subconductor, the bundle carrying `load` × `bundle`, `SpreadObject::bundle_load`); on the wire pieces they cover (FE; `Section::spread_loads` gives each as the whole bundle's, the model of one subconductor takes its share) | point labels |
| `color` | string? | — | `#rrggbb`, overrides the phase colour | — |
| `weather_groups` | `[string]` | — | its weather case groups (2026-10-25): read and kept, **not read by the checks** since 2026-11-03; not read by the solve; no table column since 2026-11-03 | `.ohlwc` `[[group]] name` |
| `wind_factors` | `[SpanWind]` | — | version 9 (2026-10-25, roadmap O15): local winds, `{span, factor}` — span index (0 = first, as span points), factor 0 to `MAX_WIND_FACTOR` (10) on the weather case's wind pressure there in every case with wind; a span not listed takes 1; each span once (validated); written only when given; kept on their spans by `Section::set_points` (a span split or joined drops its factor). `Section::wind_factor(span)`, `has_local_wind`, `largest_wind_factor` (the ruling span, the permanent stretch), `wind_factor_at_point(k)` (the strings: the larger of the two spans), `set_wind_factor(first, last, f)` (1 removes), `wind_factors_text` / `ohl_line::parse_wind_factors` (`2–4: 1.2; 6: 0.7`, spans from 1, the Sections table's **local winds**); `Line::wind_factor_at_structure`, `jumper_wind_factor` (the jumpers). Taken by the solver (`solve::LocalWinds`: each span's case `WeatherCase::with_wind_factor`), the spacers' wind, the jumpers, the structure loads' jumper strings, the wind span (`capacity::wind_spans`). OHL XML `<wind_factor span factor/>` in `<section>`; session command `set_wind_factor` | span indices |
| `ice_factors` | `[SpanWind]` | — | 2026-11-03: local ice per span, `{span, factor}` (0 to 10, each span once, validated); `Section::ice_factor`, `has_local_ice`, `largest_ice_factor` (the ruling span); `WeatherCase::with_ice_factor`; applied with the local winds in `solve::LocalWinds`; OHL XML `<ice_factor span factor/>`; also set by `ohl_line::local::localized`; `Section::ice_factors_text` / `ohl_line::parse_ice_factors` (the format of local winds; the Sections table's **local ice**, 2026-11-03) | span indices |
| `calculation` | `Calculation` | — | `{condition, method}` (`condition::Condition` × `Method`), written by its name: condition and method joined (`initial-fe` default, `creep-rs`, `max-sag-fe`, …) | — |

`SectionPoint { structure, set }` names the wire point with set `set` on
structure `structure`. Spans are between consecutive points.

`Stringing` is tagged by `kind` (its `temperature` is always given in
files; a section made in the app starts at
`ohl_line::DEFAULT_STRINGING_TEMPERATURE`, 20 °C since 2026-10-21, 15 °C
before):

| `kind` | Fields | Meaning |
|---|---|---|
| `tension` | `horizontal_tension` (N per wire), `temperature` (°C), `condition` (optional) | in the blocks: one horizontal tension in every span |
| `sag` | `sag` (m), `temperature` (°C), `span` (index, optional), `condition` (optional) | in the blocks: sagged in the ruling span or in span `span` |
| `catenary_constant` | `constant` (m), `temperature` (°C), `condition` (optional) | in the blocks (line file **15**, 2026-11-11, roadmap O22 item 3): H = constant × the bare weight per metre of the cable as strung (`solve::strung_cable(..).weight()`), every span |
| `unstressed_lengths` | `lengths` (m, one per span), `temperature` (°C), `sagged` (optional nested `Stringing`) | clipped in: each span keeps its own unstressed length |

**A point's constant force** (line file **15**, 2026-11-11, roadmap O22
item 7): `LineObject::force_across`, `force_along` (N, the whole
bundle's, default 0, not written when 0; `has_force`, `force_text`):
across the span's chord in plan, + to the right looking ahead in the
section's order; along it, + ahead. `solve::build` adds
`(right·across + ahead·along) / bundle` to the point's node. The keys are
not `across` / `along`: those are a box shape's sizes. A jumper point's
object with one is refused (`Line::validate`). The ruling-span method
leaves point loads out: `solve::point_loads_left_out` flags the section
in every weather case ("NOT ON THE SAFE SIDE: point loads (…) left out by
the ruling-span method — use FE"), weights included. OHL XML: `<object
force_across force_along>`. Validation outputs of `line.section`:
`point_{label}_{x,y,z}_m`, `pull_i_{back,ahead}_{x,y,z}_n`.

`condition` (`StrungAt`: `initial` — the default, not written — `creep`,
`load`; line file **15**, 2026-11-11, roadmap O22 item 3): the condition
the value holds in. A final one is worked back to the initial horizontal
tension by `solve::final_stringing::work_back` (Illinois secant on the
section's own solve in still air at the stringing temperature, the wire
alone; ≤ `WORK_BACK_SOLVES` = 60 solves), called from `solve::memo::prepared`
— whose key then also holds the line's `condition_cases`, `bimetallic`
and `solver` — and kept as `Prepared::worked_back`; the reference, the
strain strings' estimated weights and `clip_lengths` are those of the
initial tension found (`Prepared::as_strung`). `solve::worked_back` gives
`WorkedBack { condition, temperature, target, initial, solves, note }` to
reports (`ohl line`: `solve::final_stringing_notes`; the validation
calculation `line.section`: `stringing_target_n`, `stringing_initial_n`).
`Stringing::since_version_15` decides the version written
(`Line::to_json`: 15 only with a final condition or a catenary constant,
also inside `sagged`; else 14 or older). OHL XML: `<stringing
kind="catenary_constant" constant>` and the attribute `condition`. The
Sections table's condition cell: "tension, after creep", "catenary
constant", …; the section form: three kinds and "at condition".

`LineObject` (`objects.rs`): `kind` (`body` | `clamp` — a clamp holds a
jumper or a spacer, a hinge passing forces only), `type` (the library
type it was taken from; its values are its own), `weight` (N, whole
bundle), its shape flattened (`shape` = `point` | `sphere` (`diameter`) |
`cylinder` (`diameter`, `length`, `axis` along / across / vertical) |
`box` (`along`, `across`, `height`) | `areas` (`wind_area`, `ice_area`)),
`drag`, `clamp_length`, and on a jumper's clamp `hold` — what holds it up,
making it a **support**: `{"strings": {"structure"?, "wire"}}` (the
strings of a structure to a wire point without a set, one or two;
structure default the jumper's `from`) or `{"string": {"point", "length",
"weight"}}` (one tension-only string from a fixed point). Loads in a case (`LineObject::forces`): weight,
the wire's radial ice on its surface (a sphere's shell; a layer on the
other shapes' surfaces), the wind on its iced projected area across the
line, times its drag. A clamp's shape is drawn as given — a cylinder
across the line on a bundle joining its subconductors (drawn only) — and
takes wind and ice as a body's of that shape and size; without a shape
(`point`) it is drawn as a short cylinder along the wire, its
`clamp_length`, and takes no wind nor ice.

Older files (and OHL XML `<load>`, `<spread_load>`) gave point loads
(`loads`: `legacy::SpanLoad` — `span`, place, `vertical`, `label`, `diameter`,
`drag`, `wind_area`, `ice_area`) and spread loads (`spread_loads`: `legacy::SpreadLoad`,
`span`, `start`, `end`, `load`, `label`): read into a point `L1`, `L2`…
holding a body (a sphere of the diameter, or the given areas; the label
its description), and into two bare points `S1`, `S2`… with a spread
between them; written in the new form.

### Point and spread load library (`.ohlload`)

`ohl-point-spread-loads`, version 1 (`objects::ObjectLibrary`): `name`
and `[[type]]`s — `id`, `name`, `description`, `kind` (`body`, `clamp`,
`spread`), `weight`, the shape as on objects, `drag`, `clamp_length`, a
spread type's `load` (N/m) and `per_subconductor` (optional, spread types
only: its load on each subconductor); sizes, weights, lengths and loads with their
unit (`"600 mm"`, `"70 N"`, `"12.5 N/m"`) or plain SI numbers. Shipped templates (typical values, to check):
`libraries/objects/templates.ohlload`. A project's own types are a project library file
of kind `objects` (`project.ohlload`), over the templates of the same
ids (Sections › Point and spread load types).

### Jumper and JumperClamp (`lib.rs`, `jumper.rs`)

| Field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `id` | string | — | unique id | — |
| `description` | string | — | free text | — |
| `cable` | string | — | wire type | `.ohlw` wire type `id` |
| `from`, `to` | `JumperEnd` | — | the two ends (different): a wire point `{"structure", "set"}`, or a tap `{"tap": {"span": {"section", "point"}}}` (a span point) / `{"tap": {"jumper": {"jumper", "point"}}}` (another jumper's point); a tap's point holds a clamp object (a hinge: forces only); a span's section by FE; a jumper on a tap is given by its `length` (or rigid); jumpers hanging on each other in a ring refused; solved after what they hang on (`Line::jumper_order`); their loads keyed `solve::LoadAt::{Clamp, Span, Jumper}`. Version 4 files gave a tap as `from_tap` / `to_tap` beside a placeholder `from` / `to` (read: the tap) | structure id + set, or point labels |
| `length` | f64? | m | cut length at `temperature` | — |
| `sag` | f64? | m | or: sag below the chord in still air (no supports) | — |
| `tension` | f64? | N | or (2026-10-25, line file version 10): a riser — the pull it is tensioned to per subconductor at its higher end (`from` when level) in still air at `temperature`; dead-ended between two wire points (no tap, no helping strings, validated); cut from it in its design case (`jumper::riser::design_length`, kept with the sag cuts by `design_key`), solved as a taut wire of bars in any direction (`jumper::riser::shape`; a slack bar refused); its end forces go into the structure loads where no section ends at its wire point (`ohl_loads` `case_loads`: the string tops, or the wire point); `jumper::taut_riser` for validation (`jumper.riser`); OHL XML `<jumper tension>`; the jumper window's *tension* choice | — |
| `temperature` | f64 | °C | of `length` / `sag`; left out of a file: 15 °C, as files were written (the program writes it always; new jumpers made in the app: `DEFAULT_STRINGING_TEMPERATURE`, 20 °C, since 2026-10-21) | — |
| `rigid` | bool | — | a tube or bar: no length, sag or supports | — |
| `points` | `[JumperPoint]` | — | labelled points along it — its **supports** among them: a clamp object with a `hold`, placed by a `fraction` (`Jumper::supports` gives those on strings as `JumperClamp`s in order, `Jumper::support` the string from a fixed point as a `JumperSupport`; `set_supports` / `edit_supports` change them); version 4 files gave `support` and `supports` of their own, read into points `S1` and `C1`, `C2`… — (`at`: `JumperPlace` — `fraction` of its length from `from`, `length` along it, m, or `distance`, m horizontally from `from`'s clamp — one of them, `{"length": 3}`; older files giving several: a distance over a length over the fraction), each with its `object` or bare | label unique on the jumper |
| `bending_stiffness` | f64? | N·m² | EI; default estimated from the wire type; 0: hinged | — |
| `stiffness_factor` | f64? | 0–1 | share of the solid-bar EI for the estimate | — |
| `angle_from_deg`, `angle_to_deg` | f64? | ° | exit angle at a section's end clamp from its tension string's line (clamp → top; none: the conductor's axis), turned towards the case's load in the conductor's plane (−90…90) | — |
| `color`, `bundle`, `bundle_spacing`, `bundle_rotation_deg` | optional | — | default: those of the first section through `from` (`Line::jumper_section`) | — |

`JumperClamp` (a view of a support point, not stored): `label` (its
point's), `structure` (optional; default the structure of the jumper's
`from`), `wire` (label of a wire point **without a set**, held by one
string or two from structure points), `at` (0–1 of the
jumper's length), `clamp_length` (m, default 0.1, at most 1). A support's
clamp is not a tap; its own weight, ice and wind load the jumper when it
has some (the strings' weight and wind are theirs).

A jumper ends at wire points with sets, usually the end points of two
sections (a dead-end jumper) or a branch end and a through wire (a tap).
It refers to the wire point, not to a section; it takes its defaults from
whichever section passes through `from`.

### Electrical systems and circuits (`electrical.rs`)

| Entity.field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `System.id` | string | — | unique id | referred to by `Section.system` |
| `System.name` | string | — | display name | — |
| `System.description` | string | — | free text | — |
| `System.owner` | string | — | who owns it (free text; the phase plan's table of circuits, 2026-10-25) | — |
| `System.number` | string | — | its number in the owner's register (free text; the phase plan's table) | — |
| `System.kind` | `ac` / `dc` | — | default `ac` | — |
| `System.voltage_kv` | f64 | kV | AC: between phases (rms); DC: pole to earth | — |
| `System.highest_voltage_kv` | f64? | kV | highest operating voltage (as `voltage_kv`: AC between phases, DC pole to earth); ≥ the nominal (validated); none: not given — never filled in by the program. Line file **15** (2026-11-11), written only when a system has it or an insulation level (`Line::to_json`, else 14 or older, `BEFORE_SYSTEM_VOLTAGES_VERSION`); OHL XML attribute `highest_voltage_kv` of `<system>`; `Line::system_highest_voltage`. Read by the electric field and the surface gradient: `cross_section::PhaseData::highest_kv`, `electric::circuit_voltage` (given for the run, else this, else the nominal: `VoltageUsed` {`kv`, `source`: `VoltageSource::Given` / `Highest` / `Nominal`}), `electric::voltage_notes` ("voltages used, between phases: …" always; "NOT ON THE SAFE SIDE: nominal voltage used for …" for the circuits at their nominal). Systems table column **max kV**; the Electric field window's grid says the source (`ElectricFieldWindow::voltage_used`) | — |
| `System.insulation_level` | `InsulationLevel`? | — | `low` / `high`; none: high (`System::insulation`). Line file **15** (the same raise); OHL XML attribute `insulation_level`. Read by the internal clearances (2026-11-11): an `[[internal_rule]]` with `insulation` takes only the parts of the systems of that level; no other check of the core reads it. Not in the plugin interface's records (`system-info`, `wire-group`: unchanged, interface 0.3) — a plugin sees it and `highest_voltage_kv` only in the line file JSON it is handed (`export-model`). A plugin's own column of the same name is `System.fields["<plugin>.insulation_level"]`: another key (the table shows it as a plugin column after the table's own). Systems table column **insulation** | — |
| `System.frequency_hz` | f64? | Hz | AC frequency | — |
| `System.current_a` | f64? | A | operating current, rms per phase (DC: per pole), shared by a bundle's subconductors; ≥ 0 (validated); none: not given. Line file **13** (2026-11-04), written only when a system has one (`Line::to_json`, else 12 or older, `BEFORE_CURRENT_VERSION`); OHL XML attribute `current_a` of `<system>`. `Line::section_currents` (per section via its conductor path's system), `Line::system_current`. The default of `ohl_thermal::sections::Current::Line` and of the magnetic field's circuits not given a current (`magnetic::run`, a note says so) | — |
| `System.phases` | `[SystemPhase]` | — | phases (AC) or poles (DC) | — |
| `System.color` | string? | — | colour when wires are drawn by system | — |
| `SystemPhase.label` | string | — | matched with `Section.phase_label` | — |
| `SystemPhase.angle_deg` | f64? | ° | AC phase angle (default evenly spread) | — |
| `SystemPhase.polarity` | i8? | — | DC: +1, −1, 0 (metallic return) | — |
| `SystemPhase.code` | string? | — | line code (dots and dashes) when drawn by system | — |

**Circuits** (`circuits.rs`, line file version 7, 2026-10-21): the
conductor paths that form one circuit, defined by the user.

| Entity.field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `Circuit.id` | string | — | unique id (`--circuits` takes it or the name) | — |
| `Circuit.name` | string | — | unique, no spaces around it | given by its members' `Section.circuit_name` |
| `Circuit.system` | string? | — | the system its conductor paths are of (none: not said) | `System.id` (validated) |
| `Circuit.description` | string | — | free text | — |

Its **members** are the conductor paths whose sections give its name
(`Line::circuit_members`): stored on the sections, so splits, joins and
renames of sections need nothing else. Rules (owner decision 2026-10-21:
circuits follow the physical connections; `Line::circuit_conflicts`,
refused by `Line::check_circuit_change` when a change brings a new one,
noted when a file has one): a conductor path in one circuit; one
conductor path per (system, phase label) in a circuit — two of one phase
not connected are two circuits; a member of the circuit's system. No
merge: two circuits become one only by a connection (the jumper window's
choice of the name that stays). An empty circuit is allowed (noted,
`Line::circuit_notes`). Operations on a copy of the line:
`add_circuit`, `rename_circuit` (members follow; another circuit's name
refused), `delete_circuit` (members in none), `assign_to_circuit` (whole
conductor paths); `ohl_session::line_circuits_change` makes one command
(the `circuits` settings row and the sections changed).
`Line::circuit_names_for` resolves `--circuits` / `--circuit` (id or
name; unknown refused). Files without `circuits`: `Line::adopt_circuits`
(in `read_older`, on every read and in the session) gives every name
found on sections and on no circuit a circuit, its id the name (`-2`…
when taken), no system — the sections unchanged, so everything grouped by
circuit gives the same results.

Derived, never stored:

- **Conductor paths** (`conductor_paths`): sections joined by jumpers or
  by sharing an attachment point. A path carries one phase of one system
  (`Line::section_phases`); contradicting assignments are errors.
- **Conductor paths with a direction** (`circuits`): a conductor path — its
  starts are the ends listed in `circuit_starts`, else it runs along the
  line from the end that comes first. `Circuit { sections, jumpers,
  starts, ends, chosen, order }`; `directions` gives each section's and
  jumper's direction in its conductor path.
- **Potential**: a section's nominal voltage and voltage to earth, from
  which `Potential::between` gives the voltage between two conductors
  (used by clearance rules).

### Cross-sections and line parameters (`ohl-electrical`, derived)

Derived from the line solved in one weather case, never stored (roadmap
O18, 2026-10-27; built, in active development, not approved yet). The
**cross-section** is the record every electrical calculation shares —
the line parameters now; the load flow, the fields and noise and the
plugins later (a plugin extension point comes with the hvlbuzz step; the
plugin interface is unchanged so far). It is `serde::Serialize`, so it can
be written as JSON and passed on as it is.

| Entity.field | Type | Unit | Meaning |
|---|---|---|---|
| `LineSpan.from`, `.to` | string | — | a **span of the line**: two structures next to each other, `from` first along the line (`line_spans`; off the alignments, as most sections between them are drawn, 2026-10-29) |
| `LineSpan.wires` | `[(usize, usize)]` | — | every wire between them: (index in `Line.sections`, span index in its section) |
| `CrossSection.station` | `Station` | — | `from`, `to`, `span_length` (horizontal, m), `at`: `Mean(MeanHeight)` (the span's mean) or `Fraction(f)` (at f × span length from `from`) |
| `CrossSection.ground` | `Ground` | — | `elevation` (m, on the axis at the station; the span's middle for a mean), `slope_across` (rise per m to the right; 0 without terrain), `soil_resistivity_ohm_m`, `soil_resistivity_assumed` |
| `CrossSection.wires` | `[CrossWire]` | — | the line's conductors and earth wires there |
| `CrossSection.other_lines` | `[OtherLineWire]` | — | other lines' wires in the corridor (`line` name, `wire`): **extension point, empty today**; the calculations take them like the line's own |
| `CrossSection.notes` | `[string]` | — | what was assumed or left out |
| `CrossWire.section`, `.span` | string, usize | — | its section and span (0 = first) |
| `CrossWire.offset` | f64 | m | across the line from the span's axis (structure to structure in plan), **positive to the right looking ahead** |
| `CrossWire.height` | f64 | m | above the ground under it |
| `CrossWire.elevation` | f64 | m | absolute |
| `CrossWire.bundle` | `BundleData` | — | `count`, `spacing` (m), `rotation_deg` (as `Section.bundle*`) |
| `CrossWire.wire` | `WireData` | — | `cable` (id), `diameter` (m), `gmr` (m, `None`: not given), `resistance` (Ω/m at `temperature_c`, AC; the DC one with `resistance_is_dc` when no AC is given), `temperature_c` (the weather case's) |
| `CrossWire.phase` | `PhaseData?` | — | `system`, `index` (its place in the system), `label`, `circuit` (name, empty: none), `kind`, `voltage_kv`, `frequency_hz`, `angle_deg`; `None`: earthed (earth wire, OPGW) |

Where the wires are: `cross_section_at` (a station: each solved wire where
it crosses the plane square to the axis) or `span_cross_sections` (each
span's mean by `MeanHeight`). The ground: a terrain when given, else
straight between the structures' feet and level across.

**Line parameters** (`parameters::line_parameters`, `ohl_electrical::run`):
per span `SpanMatrices` (one per AC frequency: `phases` as `PhaseKey`
{circuit, system, index}, `sections`, `z` Ω/m complex, `c` F/m, the
earth wires reduced out); per circuit (`CircuitKey` {circuit name,
system}) `CircuitParameters` {`frequency_hz`, `phases`, `angles`,
`stretches` (consecutive spans with the same sections: `sections`,
`from`, `to`, `spans`, `totals`), `total`}; `Totals` {`length` m
(horizontal), `wire_length` m, `z` Ω, `c` F} (`per_km`: per kilometre of
wire; `sequence`) — since 2026-11-11 the sums are over the wires' own
length: `cross_section::CrossWire::length` (along the solved wire
between its clamps, plus the stretch along the axis between a clamp and
its structure; never below `Station::span_length`), per span the mean of
the circuit's phases (`parameters::wires_length`; a `Coupling`: of both
circuits', its `wire_length` beside `length`); `report::Row::wire_km`
beside `length_km` (the totals: per km × `wire_km`), the parameters CSV
ends with a new column `wire_length_km`; every result carries
`parameters::WIRE_LENGTH_NOTE`; `Sequence` (z1, z2, z0 and c1, c0 as built;
the ideally transposed z1, z0, c1, c0, b1, b0; `unbalance`); `Coupling`
between circuits of one frequency sharing spans (`length`, `z`, `c`,
`z0m`). Methods: [Line parameters](../theory/line-parameters.md).

**Electric field** (`electric::run`, roadmap O18 step 5, 2026-10-29;
derived, never stored). Inputs `electric::Settings`: `model`,
`voltages` [`CircuitVoltage` {`circuit`, `kv` between phases; 0:
voltage-free}], `height`, `width`, `step`, `stations`, `points`,
`limit_kv_m`, `ground_radius` (m, the plane under a point fitted over
it), `ground_model` (`Tiled(TiledFloor {fine, coarse, fine_along,
fine_margin, reach})`, the default since 2026-10-29: a conducting floor
of tiles around each profile, group of points and cut; or `Plane`;
`Floor::solve` / `Floor::field` in a `Frame` for cuts computed later,
`Floor::under` along a volume's path), `cut_tile`, `volume_tile` (m, 8
by default: the tiles under cuts and volumes),
`jumpers`, `piece_length`, `cuts`, `volumes`, `levels` (kV/m).
Results `ElectricField`: `charges` [`WireCharge` {`section`, `from`,
`to`, `frequency_hz`, `kv` to earth, `nc_per_m`}], `profiles`
[`electric::Profile` {`from`, `to`, `at`, `points`, `max_kv_m`,
`max_offset`, `ground_departure` (m: the terrain off the plane under the
largest value within three ground radii), `tiles` (with the tiled
floor)}], `points` [`electric::FieldPoint` {…, `e_kv_m`,
`margin_kv_m`}], `cuts`, `volumes` (the magnetic field's `FieldCut`,
`FieldVolume`, values in kV/m), `notes`. `Sources.ground` is
`Send`-safe since 2026-10-29 (`dyn Fn + Sync`: the field's points run on
all cores). `electric::run_keeping` also returns the 3D model's
`Charges3d` (with the structures' steel of `Settings.steel`
[`SteelMember` {`structure`, `points`, `radius`}] in pieces, 2026-10-31;
the field at a point later: `Charges3d::field` with
`GroundPlane::fitted` over `ground_radius`, on the terrain or else
`electric::feet_ground`), `electric::csv` the results as CSV (`kind`
profile / point / cut, …, `e_kv_m`, `limit_kv_m`, `margin_kv_m`).
OHL-Design's Electric field window (2026-10-29) draws its cuts and
surfaces on the layer `scene::Layer::ElectricField` (*Electric field
(isolines, surfaces)*, `field_overlay::Overlay`, as the magnetic
field's); its inputs are the project setting `app.electric_field.<project
line>` (below). Methods: [Electric field](../theory/electric-field.md).

**Conductor surface gradient** (`surface_gradient::run`, roadmap O18
item 7, 2026-11-04; 2D only; derived, never stored except with a saved
Electric field run). Inputs `surface_gradient::Settings`: `voltages`
[`CircuitVoltage`] (as the electric field's), `stations` [`StationSpec`]
(empty: every span), `ends` (bool, default true: also each span's two
ends, the first cross-section every wire reaches), `charges` (line
charges per subconductor, `CHARGES` 8), `points` (evaluation points round
each, `POINTS` 360). Results `SurfaceGradients` {`charges`, `points`,
`cuts` [`GradientCut` {`from`, `to`, `at` (m), `place` (`lowest`,
`start`, `end`, `given`), `bundles` [`BundleGradient` {`section`,
`circuit` (empty: an earth wire), `phase`, `kv` to earth, `count`,
`diameter` (m), `subconductors` [`SubconductorGradient` {`offset`,
`height` (m), `max_kv_cm`, `angle_deg`, `mean_kv_cm`}],
`average_max_kv_cm`, `max_kv_cm`, `markt_mengele_kv_cm`}],
`potential_error` (share of the largest voltage)}], `notes`}; kV/cm rms.
The solver alone: `surface_gradient::solve` (cylinders
[`Cylinder` {`y`, `z`, `radius`, `volts`}], charges per cylinder,
ground or free space, placement — `PLACEMENT` 0.3 r) → `Solution`
(`potential`, `field`, `surface_field`, `charge`, `potential_error`),
`surface_gradient::surface_gradient`, `markt_mengele`;
`surface_gradient::csv` one row per subconductor. OHL-Design keeps the
option in the Electric field window's project setting (`gradient`,
absent: on) and the results in a saved run's `results.gradient`. CLI
`ohl electrical gradient`. Validation calculations
`electrical.surface_gradient_2d`, `electrical.cylinder_gradient`.
Methods: [Conductor surface gradient](../theory/surface-gradient.md).

**Magnetic field** (`magnetic::run`, roadmap O18 step 4, 2026-10-27;
derived, never stored — the window's inputs live in the window for the
session only). Inputs `Settings`: `model` (`three_d` / `two_d`),
`heights_2d` (`station` / `mean`), `currents` [`CircuitCurrent` {`circuit`
(name, system id or `*`), `amps` A rms, `reverse` (load flow against the
line)}], `earth_wires` (induced currents on), `height`, `width`, `step`
(m), `stations` [`StationSpec` {`from`, `to`, `fraction` (none: the
lowest point)}], `grids` [(from, to)], `points` [`GivenPoint` {`name`,
`x`, `y`, `z`? or `height`?}] (read by `parse_points` from a CSV with a
header `x`, `y`, `z`|`height`, `name`), `limit_ut` (optional, µT; none
built in); since 2026-10-29 `ends` [`EndCurrent` {`circuit`, `structure`, `amps`?
(into the line; none: the rest)}] (a circuit given so: every section's
current by Kirchhoff's current law, `circuit_ends` lists the end
points); since 2026-11-11 (both optional when read, `#[serde(default)]`)
`phase_currents` [`PhaseCurrent` {`circuit`, `phase` (its label),
`amps`, `angle_deg`}] — a circuit named takes every phase from them
(all needed; not with `ends`; `Currents::take_phases`, its net current
in the notes, `CurrentUsed::source` = `PER_PHASE_SOURCE` and `amps` the
largest phase's) — and `earth_currents` [`EarthCurrent` {`section`,
`amps`, `angle_deg`, `frequency_hz`?}] — an earth wire's current typed
in every span (`Currents::take_earth`; a given current to the earth
wires computed beside it, `earth_terms`; refused with `towers`); a
result with a balanced circuit carries `magnetic::BALANCED_NOTE`, `jumpers` (the jumpers carry their phase's current,
3D; default on), `towers` (with `earth_wires`: the earth wires as a
network through the structures' earthing, their currents as paths),
`grid_step` (m; none: `step`),
`piece_length` (m; none: the solver's pieces), `modelled_within` (m; none:
the whole line — cuts and volumes only), `cuts` [`CutSpec` {`at`:
`CutAt` (2026-10-31, "cuts are made on the alignment": `Station`
{`alignment`, `station` m} — the plane perpendicular to the alignment
there, at an angle point along the bisector, `ohl_line::route::plane_at`,
the cross-section panels' own placement — or `InSpan` {`from`, `to`,
`fraction`? (none: the lowest point)}, converted by the run to the
alignment station of that place, `Line::span_station`), `width`,
`above`, `step`, `label`? (2026-10-31: carried to the result, e.g. a
plugin's building; `CutSpec` has a `Default` for struct update
syntax)}], `volumes` [`VolumeSpec` {`from`,
`to`, `width`, `above`, `step`}], `levels` (µT, ascending; display
only). `thermal_currents` gives the circuits' thermal ratings (the
least over a circuit's sections of bundle × ampacity, in an ambient case
and at a temperature given). A circuit given no current takes its
system's operating current (`System.current_a`, 2026-11-04; a note says
so), else 0 A. Results `MagneticField`: `currents`
[`CurrentUsed`], `earth_wires` [`EarthWireCurrent` {`section`, `from`,
`to`, `frequency_hz`, `amps`, `angle_deg`}], `profiles` [`Profile`
{`from`, `to`, `at` m, `span_length`, `along_line` m, `points`, `max_ut`,
`max_offset`, `limit` (`LimitExtent` {`left`, `right`, `beyond`}),
`reach` m, `warnings`}], `grids` [`FieldGrid` {…, `deviation` per level}],
`towers` [`TowerCurrent` {`structure`, `frequency_hz`, `amps`,
`angle_deg`, `resistance_ohm`, `assumed`}] (2026-10-29),
`cuts` [`FieldCut` {`label`?, `alignment`, `station` (where it was
placed), `angle_point`, `from`, `to` (the span its plane crosses at the
axis), `at` (where it crosses the span's chord, m from `from`), `values`: `field_views::CutValues`
{`cut`: `VerticalCut` {`origin`, `right`, `left`, `right_offset`,
`bottom`, `top`, `step`}, `offsets`, `elevations`, `ground` per column,
`values` µT row by row}, `pieces`, `left_out`}], `volumes` [`FieldVolume`
{`from`, `to` (the run's first and last structure: grids and volumes
take a run of spans following each other, 2026-10-29), `values`:
`VolumeValues` {`bounds`: `VolumeBox` {`path`: `field_views::Path`
(points x, y, ground; mitred offsets at its angles), `width`, `bottom`,
`top`, `step`},
`surfaces` [(level, `contour::Mesh` {`vertices`, `triangles`})],
`deviation` per level, `computed`, `full`, `above` per level, `left_out`
per level (thin short pieces of surface left out, 2026-10-31),
`elsewhere` (coarse cells lying wholly in a volume computed before, left
to it: `volume_beside`, 2026-11-10; absent in older saved runs: 0)}, `pieces`,
`left_out`}], `pieces` (`Pieces` {`count`, `longest`, `off_wire`,
`interpolation`}), a `Deviation` being {`points`, `median`, `p90`, `max`}
(relative to the level, or to the whole line's field for `left_out`);
`run_keeping` returns the 3D model's wires too (`Wires3d`: `field`,
`field_block`, `within` (by a `Path` and a width), `segments`); the
inputs' `Sources` carry the solved `jumpers` (2026-10-29); `seconds_per_piece_point` the
speed measured here, `refine` the shorter pieces, `obj` the surfaces as
Wavefront OBJ; `points`, each a
`FieldPoint` {`name`, `x`, `y`, `z`, `offset` (to the right of the span's
axis), `height` (above the ground), `b_ut` (rms of the vector), `major_ut`
(the cycle's largest), `margin_ut` (limit − B)}, `notes`; `csv` writes
them (since 2026-10-31 a last column `label`, a cut's label on its rows;
the electric field's CSV the same). Methods: [Magnetic field](../theory/magnetic-field.md).

**Thermal rating at a current** (`ohl_thermal`, 2026-11-04):
`sections::section_temperatures(line, library, method, ambient,
Current::{Line | Given(A)}, only)` → `[SectionHeat {section, cable,
bundle, current: Result<A per phase, why>, cases: [Result<HeatBalance,
why>] per ambient case}]` (the subconductor's steady temperature at
current / bundle; earthed sections none; `hottest()`, `in_case(k)`);
`limit::clearance_at(line, library, base, rules, targets, [(section,
°C)], budget, progress)` → `[SectionCheck {section, cable, temperature,
result: Result<Option<Governing>, why>}]` (the least margin over the
requirements, met or not; `passes()`), the same solve and check as
`limit::clearance_limits_of`. `Governing` gained `target` (the point on
the ground or the point cloud), `margin()`, `met()`. Not stored: the
window's results and the CLI's CSV (`ohl thermal clearance-check`:
`section, cable, current_a, bundle, temperature_c, ambient_case, status
(pass / fail / no_current / not_checked / error), rule, span, measure,
clearance_m, required_m, margin_m`).

<a id="field-results-json"></a>**Field results JSON** (2026-10-31;
derived, written by `ohl electrical magnetic|electric|fields --json` and
handed to a field-presets plugin's `report`). A run's results are a
**set** (`ohl_electrical::field_results::set_json`; the owner: "Ok for 1
but keep it clean"): `{"format": "ohl-field-results-set", "version": 1,
"case": {"name", "temperature_c"}, "fields": [<one document per field
computed, magnetic first>]}` — one document from `magnetic` or
`electric`, two from `fields`. Each document is `ohl-field-results`
**version 2** (`magnetic_value`, `electric_value`; version 1 had no
alignment on a cut): `{"format": "ohl-field-results", "version": 2,
"quantity": "B" | "E", "unit": "µT" | "kV/m", "model": "three_d" |
"two_d", "case": {"name", "temperature_c"}, "settings": <the run's
Settings without stations, grids, points, cuts, volumes>, "limit",
"cuts": [{"label", "alignment", "station", "angle_point", "from", "to",
"at", "quantity", "unit", "plane": {"origin": [x, y], "right": [dx,
dy]}, "offsets", "elevations", "ground" (per column, null: none known),
"values" (row by row, every point, also under the ground), "isolines":
[{"level", "lines": [[[offset, elevation], …], …]}] (the settings'
levels, `CutValues::isolines`: cut at the ground)}], "points": [{"name",
"x", "y", "z", "offset", "height", "value", "major" (B only),
"margin"}], "notes"}`. Nothing is computed for it: the results as they
are (a value in the CSV is the same to its 5 decimals).

**A cut's document** (`ohl-field-cut` version 1, 2026-11-02;
`ohl_electrical::field_results::split_set`, for a plugin's
`report-cut`): `{"format": "ohl-field-cut", "version": 1, "case",
"line" (the line's name), "label", "alignment", "station",
"angle_point", "from", "to", "at", "plane", "fields": [{"quantity",
"unit", "limit", "offsets", "elevations", "ground", "values",
"isolines"}, …] (each field computed, magnetic first), "points":
[[offset, elevation, class], …] (the plan's classes within its band),
"sections": [{"kind": "tin", "name", "lines": [[[offset, elevation],
…], …]} | {"kind": "dxf", "name", "marks": [[offset, elevation, layer,
"#rrggbb"], …]}]}`; the set handed to `report` keeps each cut without
its grids, `"span": [first, last offset]` in their place.

<a id="field-plan-template"></a>**Field plan template** (`.ohlfp`,
format `ohl-field-plan` version 1, 2026-11-02; source, a project library
kind `field-plans`; `ohl_electrical::field_plan::FieldPlan`, unknown
keys refused): `name`; `[title]` `lines` (`{line}`, `{cut}`, `{field}`,
`{case}` replaced), `logo = [{ file, width }]` (SVG, PNG or JPEG on this
computer, a relative path from the plan's folder; width a length,
default 40 mm; embedded as a data URI when handed to a plugin);
`[cut]` `classes = [{ class, colour }]`, `band` (length, default 1 m),
`background` (`none` | `tin` | `dxf`), `surface` (tin: a project
surface; empty: the field's ground), `drawing` (dxf: a project
background drawing), `layers` (dxf: only these); `[magnetic]`
`limit_ut`, `level = [{ value, colour, width (px, 1.6), label }]`;
`[electric]` `limit_kv_m`, `level`; `[wires]` `phase`, `earth`
(`#rrggbb`); `[weather]` `case` (every wire), `earth_wires` (the wires
of no system), `systems = { ID = CASE }` (per electrical system) — case
names of the run's weather file. Its levels are added to the run's, its
limits taken where none is given; the template (`TEMPLATE`) carries the
Swiss NISV limits (1 µT, 5 kV/m; owner 2026-11-02) as an example to
replace.

**The building-cuts plugin's files** (`plugins/examples/building-cuts`,
2026-10-31; derived, written by its `report` into `--out` from one run's
set; `Q` = `B` or `E`, unit columns `uT` / `kV_m`; a field not run has
no columns and no files): `buildings.csv` (`id, points, area_m2,
centroid_x, centroid_y, ground_z, eaves_z, roof_z, floors, span,
alignment, station_from_m, station_to_m, axis_distance_m,
conductor_distance_m, cuts`, then per field `max_section_Q_…,
max_places_Q_…` [B: `max_places_B_major_uT`] [with a limit: `limit_Q_…,
margin_Q_…`]), `points.csv` (`name, building, floor, place, x, y, z,
height_m`, then per field `Q_…` [B: `B_major_uT`] [`margin_Q_…`]),
`cut-LABEL-Q.csv` (`offset_m, elevation_m, ground_m, Q_…,
below_ground`), `cut-LABEL-Q.svg` (one drawing per cut and field; with
a field plan its title lines and logos, levels' colours, points, TIN or
drawing crossings and wire colours),
`notes.txt`, `buildings.geojson` (features `kind` = `building` (Polygon,
its summary as properties), `cut` (LineString, with its `alignment` and
`station`), `place` (Point)), `buildings.dxf` (R12, optional). Its
presets carry two kinds of **notes** the report reads back: `building
ID: points N; area A m²; centroid X Y; ground Z m; eaves Z m; roof Z m;
floors N; span FROM–TO; alignment ID; stations T0 T1 m; axis D m;
conductor D m` (stations along the alignment) and `cut LABEL: wire NAME
OFFSET Z[ earth]; …` (NAME the circuit and phase, else the section;
fields split at `; `).

### Display and criteria

| Entity.field | Type | Unit | Meaning |
|---|---|---|---|
| `Display.phase_colors` | map phase label → `#rrggbb` | — | wire colours by `phase_label` |
| `Display.ahead_deg` | f64 | °, 0–360 | the angle meaning ahead for plan angles around a structure (clockwise convention, display only) |
| `CreepStretch.creep_case` | string? | — | weather case name for the final after creep |
| `CreepStretch.load_cases` | `[string]` | — | weather case names for the final after load (the heaviest wins) |
| `BimetallicModel.behavior` | enum | — | bird cage or compression (default) |
| `BimetallicModel.max_virtual_compression` | stress? | Pa | compression limit |

## References and ownership

### How objects point at each other

| From | Field | To | Key |
|---|---|---|---|
| Alignment | `route` | Route | `id` |
| Alignment | `points[]` | AlignmentPoint | `id` |
| AlignmentPoint | `structure` | Structure | `id` |
| Structure | `routes[]` | Route | `id` |
| Structure | `type` | StructureType (`.ohls`) | `id` |
| Structure | `extension` | the type's extension | `name` |
| Structure | `omit[]` / `omit_sets[]` | the type's items | label / set |
| InsulatorString | `top` | StructurePoint or WirePoint of the **same** structure | label |
| InsulatorString | `wire` | WirePoint of the same structure | label |
| InsulatorString, TypeString | `insulator` | InsulatorAssembly (`.ohli`) | `id` |
| Section | `points[]` | WirePoint | structure `id` + `set` |
| Section, Jumper | `cable` | Wire type (`.ohlw`) | `id` |
| Section | `system` | System | `id` |
| Section | `phase_label` | SystemPhase of that system; `Display.phase_colors` | label |
| Jumper | `from`, `to` | WirePoint | structure `id` + `set` |
| Jumper | `from`, `to` (a tap) | SpanPoint (`{section, point}`) or JumperPoint (`{jumper, point}`) holding a clamp | section / jumper `id` + point label |
| Jumper point's `hold` (a support) | `structure`, `wire` | WirePoint without a set | `id`, label |
| SpanPoint | `span` | the span between `points[span]` and `points[span + 1]` of its section | **index** |
| SpreadObject | `from`, `to` | SpanPoints of the same section and span | label |
| Spacer | `a`, `b` | SpanPoints (`{section, point}`) | section `id` + point label |
| Spacer | `insulator` | InsulatorAssembly (`.ohli`) | `id` |
| LineObject, SpreadObject | `type` | ObjectType (`.ohlload`) — informative: the values are the object's own | `id` |
| Line | `circuit_starts[]` | WirePoint at a conductor path end | structure `id` + `set` |
| Line | `creep_stretch` | WeatherCase (`.ohlwc`) | `name` |
| LoadCase (`.ohllc`) | `weather` | WeatherCase | `name` |
| StructureType | `capacity.attachment[].point` | TypePoint | label |
| TypePoint | `node` / `member` | TypeNode / TypeMember | `id` |
| TypeMember (and `.ohlfe` `[[member]]`) | `nodes`, `section` | TypeNode; the cross-section library's Section (`.ohlsec`) first, else `[[section]]` (resolved by `ohl_structures_fe::Model::parse_with`; the structure type alone does not check a section id) | `id` |
| StructureType | `model`, `pole` | `.ohlfe` file (relative path), `.ohlp` id or file name (a `[pole]` table refers to nothing) | — |
| InlinePole (`[pole]`) | `section` | the cross-section library's Section (`.ohlsec`, a polygonal or circular hollow section; resolved by `ohl_structures_fe::pole::Pole::from_inline`) | `id` |
| Extension | `member[].nodes` | TypeNode of the body or of the same extension | `id` |
| LegExtension | `leg` | `A`–`D` (a quarter: its lowest supported joint) or a TypeNode of the body or extension | `id` |

Labels are scoped to their structure (or type), span point labels to
their section, jumper point labels to their jumper; ids are unique per
kind in the line; library ids are unique per library. Span points are the
one place a **position in a list** is a reference: `SpanPoint.span` (and
`Stringing::Sag.span`, `UnstressedLengths.lengths`) count the section's
spans, so they must change when a structure is added to or taken out of
the section's `points`. A typed structure's sets and
labels may come from its type: references to them are checked after
`Line::resolve`.

### Ownership and deletion

A structure **owns** its points, wire points, strings and stick drawing;
a section owns its stringing, span points (with their objects) and
spreads; a jumper owns its points (with their objects) and support
clamps. Alignment points are their own objects (an alignment lists
them); spacers are their own objects between two sections' span points.
Everything else is a reference. Deletion goes through the session
(`crates/ohl-session/src/lib.rs`, `Command::Delete`): when an object takes
others with it, the command is refused with the list
(`SessionError::NeedsConfirmation`) unless `confirm` is set, and then all
of it is one log entry, undone together.

| Deleting | Also deletes or changes |
|---|---|
| Structure | its own items; every **section** through it (and their spacers); every **jumper** at it, held by its strings (a support), or at a point of those sections, and the jumpers on **taps** of those sections (and those hanging on them); alignment points tied to it are **detached** (they stay in place) |
| Wire point with a set (`delete_attachment`) | its strings and the set-less wire points only it uses (own items removed; a type's items added to `omit_sets`); the sections through it (and their spacers); the jumpers at it or at those sections, and those on taps of those sections (as for a structure) |
| Section | its spacers; the jumpers at any of its points; the jumpers on taps on its span points, and those hanging on them (`jumpers_hanging`, those hanging first) |
| Span point, jumper point | edited inside its section / jumper (a `put`): a point a spacer, spread or tap uses cannot be removed alone (validation refuses; the table takes them with it; the app's Delete asks for a spread's end whether to delete its whole spread, and for a spread whether to delete its bare points too); relabelled with `rename_point` (`Line::relabel_point`: its spreads, spacers and taps follow; the Span points table uses it too) |
| Spacer | nothing else |
| Route | its alignments on no other route; it is taken out of the alignments, points and structures that stay |
| Alignment (also: its last point) | its points (asked first) |
| Jumper | the jumpers on taps on its points, and those hanging on them |
| Structure replaced (`replace_structure`) | what ends at an old set mapped to no new set: those sections (and their spacers), the jumpers at those sets or at those sections, and those on taps of those sections (as for a wire point); the old structure's own points, strings, wire points and `omit` / `omit_sets` (overrides of its type) and the jumper support clamps on wire points the new structure lacks are dropped — all listed in the confirmation |
| Meta (line settings) | cannot be deleted |

Every deletion (and a section's points changed) also takes the
`circuit_starts` no longer a point of a section out of the line settings
(`starts_follow`, a put of `meta` in the same entry); files written
before drop such starts on reading (`Line::drop_stale_circuit_starts`).
A section editor changing a section's points keeps its span points on
their spans (`Section::set_points`, a span found by its two ends), and
refuses when a span holding points is gone.

Renames follow references: `rename` of a structure updates section
points, jumper ends and clamps, tied alignment points and
`circuit_starts`; of a section, its spacers and the taps on it; of a
jumper, the taps on it; of a route, the alignments, points and structures
on it; a rename is a put under the new id and a delete of the old one;
`rename_attachment` (set renumbering) updates the sections, jumpers and
conductor path starts there; `rename_point` (a span or jumper point's label)
updates its spreads, the spacers on it and the jumpers tapped on it;
`rename_system` updates the sections given it. `replace_structure`
(`crates/ohl-session/src/replace.rs`) keeps the structure's id, position,
orientation, height adjustment, description, routes and fields, and moves
the section points, jumper ends and conductor path starts at each old set to the
new set mapped (given in `sets`, else the same set when the new structure
has it). Library items are
not renamed through the session: a changed library id leaves dangling
references that `Line::resolve` / validation report.

## Library files

One directory per kind; TOML with units in the values. Each item has an
`id` of letters, digits, `-`, `_` and `.`. The app's library editors
(`crates/ohl-app/src/library.rs`, `LibraryKind`) cover the first five.

| Kind (`LibraryKind`) | Extension | Format id | Items per file | Rust type | Guide |
|---|---|---|---|---|---|
| `Cables` | `.ohlw` | `ohl-wire` (version 3: a wire type's optional `[cable.hot_creep]` and `bimetallic_behavior = "gap"`; 2: its optional `reference_temperature`) | several (`[[cable]]`) | `ohl_cables::Cable` | [Wire library](../user-guide/wire-library.md) |
| `Insulators` | `.ohli` | `ohl-insulator` | one | `ohl_structures::InsulatorAssembly` | [Structure library](../user-guide/structure-library.md#insulator-assemblies-ohli) |
| `StructureTypes` | `.ohls` | `ohl-structure` | one | `ohl_structures::StructureType` | [Structure library](../user-guide/structure-library.md) |
| (structure templates: shipped `libraries/structure-templates`, the project's `Structure templates` folder — the folders' names before 2026-10-21, `pylon-templates`, `Pylon templates`, `PylonTemplates/`, still read; not kept in the project file) | `.ohlpt` | `ohl-structure-template` (`ohl-pylon-template` before 2026-10-21, read) | one | `ohl_structures::templates::PylonTemplate` | [Structure templates](../user-guide/structure-library.md#structure-templates-ohlpt) |
| `WeatherCases` | `.ohlwc` | `ohl-weather-cases` | several (`[[case]]`; named groups of them `[[group]]`: `name`, `description`, `cases` by name, `structure_wind?` (a Structures column key whose value was the structure's wind in the group's cases for the internal clearances), `structure_wind_min?` (Pa; a lower value raised to it), 2026-10-25 — both read and kept, **no longer used** since 2026-11-04: the structures' `local.wind_study` and the internal rule file's `[check] wind_study_min`, `wind_study_cases`; `CaseGroup::structure_value` reads a column's value for the copy) | `ohl_weather::WeatherCase` | [Weather cases](../user-guide/weather.md) |
| `LoadCases` | `.ohllc` | `ohl-load-cases` | several (`[[case]]`) | `ohl_loads::LoadCase` | [Load cases](../user-guide/loads.md) |
| (cross-section library: project kind `sections`; shipped example `libraries/sections`, never copied into a project) | `.ohlsec` | `ohl-sections` | several (`[[section]]`, `[[material]]`) | `ohl_sections::Section`, `ohl_sections::Material` (crate `ohl-sections`) | [Cross-section library](../user-guide/section-library.md) |
| (structure application; not a library kind yet) | `.ohlslc` | `ohl-structure-load-cases` | several (`[[load_case]]`) | `ohl_structure_loads::LoadCaseRecipe` | [Structure load cases](../user-guide/structure-load-cases.md) |
| (structure application: typed loads; project kind `loads`, beside the structure types) | `*.loads.csv` | CSV, the load table's columns | typed rows (`Override`) of one structure type or one placed structure | `ohl_structure_loads::LoadTable` | [Structure load cases](../user-guide/structure-load-cases.md#the-load-table) |
| (line application → structure application, derived, not kept) | `*.line-values.json` | `ohl-structure-line-values` (JSON) | one placed structure | `ohl_structure_loads::PlacedValues` | [Structure application](../user-guide/structure-app.md#started-for-a-placed-structure) |

**Wire library version 2** (2026-10-21, owner decision): a wire type's
optional `reference_temperature` (`Cable::reference_temperature`,
`Option<Temperature>`), the data sheet's temperature of its
`mass_per_length`; none: 20 °C (`ohl_cables::DEFAULT_REFERENCE_TEMPERATURE_C`,
`Cable::mass_reference_temperature`). The calculations take the weight per
metre at the stringing temperature from it (`Cable::weight_at`,
`Cable::weight_factor_at` in the multiplicative thermal form,
`Mechanics::weight_factor` in the curves' additive one,
`Cable::with_mass_at`; `ohl_line::solve::strung_cable`,
`ohl_line::jumper::cut_cable`). Files of version 1 read unchanged (their
wire types' mass at 20 °C). The app's wire library editor raises a file to 2
when one of its wire types gives the field (`LibraryPage::save`); without it a
file keeps its version.

**Wire library version 3** (2026-11-11, roadmap O22 item 6; the
mechanism only): a wire type's optional law of creep at elevated
temperature, `[cable.hot_creep]` — `Cable::hot_creep:
Option<ohl_cables::hot_creep::HotCreep>` `{ source (required), form
("power"), stress ("mpa" | "percent_rts"), k, stress_exponent,
time_exponent (0–1), temperature ("none" | "exp" | "power"),
temperature_coefficient, reference_temperature (20 °C), above }`;
`HotCreep::{strain, equivalent_hours, after_stage, stress_of, problem}`.
**No coefficient is built in or shipped**; the family of forms is not
confirmed against the sources (missing) and may change before a release.
Older files read unchanged; an older program leaves a wire type with the
table out, saying so (unknown field). The app's wire library editor raises
a file to 3 when a wire type has the table (`raise_wire_version`). The
JSON schema `docs/schema/ohl-wire-1.schema.json` is version 1's: it holds
neither this nor version 2's field. The line's side:
`condition::CreepStretch::hot: Vec<HotStage { temperature (°C), hours }>`
(line file **15**, written only when given; at most `HOT_STAGES` = 100;
`hot_in_order`, `hot_text`, `hot_problem`; OHL XML `<creep_stretch><hot
temperature hours/>`), applied by `solve::hot_creep_of` in
`solve::final_state(Condition::Creep)`: `FinalState::hot_creep` (unit
strain), `FinalState::notes` (the law's source — the user's coefficients
— or "NOT ON THE SAFE SIDE: … no law …"; they become flags of every
section solved in that state), `FinalState::lengthening()` (what the
models add to the unstressed length: the linear models' stretch and the
hot creep). The memo keys of the final state and of a stringing worked
back hold `creep_stretch.hot`. Validation calculation `cables.hot_creep`.

**Gap-type conductors** (wire library version 3, 2026-11-11, roadmap O22
item 7): `bimetallic_behavior = "gap"` (`BimetallicBehavior::Gap`,
`Cable::is_gap`, `BimetallicUse::Gap`; needs a core; refused as a line's
`BimetallicModel`). `ohl_cables::mechanics`: a component's `offset`
(taken off its mechanical strain; 0 but for a gap type's outer strands),
`Mechanics::with_outer_offset`, `outer_offset`, `gap_offset(tension,
temperature)` (the core alone carries the tension: its strain less the
outer strands' thermal strain). `ohl_sagtension::nonlinear::
NonlinearSection::from_known_state_gap` strings a section so (a fixed
point on the offset and on the weight strain: the weight per metre as the
conductor lies when strung; ≤ `GAP_PASSES` = 20). In the line:
`solve::strung_section` (the one place a section's sag-tension section is
built: `final_state`, `solve_rs`, the sagging table), the finite elements'
curves path with `solve::memo::gap_mechanics` (kept per prepared section;
`curves_of` gives a gap type's components also without curves). The
per-line note `BimetallicUse::Gap.note` ("NOT ON THE SAFE SIDE: gap-type
wire …") through `ohl_line::bimetallic_notes`; no per-span knee flags.
`ohl sagtension` refuses a gap type. The app's wire library editor lists
the option and raises the file to 3.

**Stress-strain curves and the sag-tension job** (2026-10-22). A
component's `stress_strain.max_strain` (optional, strain units; absent:
the curves must rise up to 1 %, and since 2026-10-22 no range warning)
now also says where the results warn: a state beyond
it is flagged (`Mechanics::beyond_range`, `NonlinearSection::range_warnings`,
`SectionShape::flags`), the polynomial still evaluated. The wire format is
unchanged. `MaterialState::Final { permanent }` holds a **signed** set per
component (it was never below zero before). The `ohl sagtension` job file
(no format id; read by `crates/ohl-cli/src/sagtension_job.rs`) takes two
optional additions, old jobs read unchanged: `[[criterion]] condition`
(`initial` default, `creep`, `load`; a wire with curves then takes its
criteria on the non-linear model, a linear wire refuses the last two) and
a `[sagging]` table (`temperature`, `waves`, `clip_temperature`,
`[[sagging.transit]]` with `span`, `below_back`, `from_back`) —
[Sagging outputs](../user-guide/sagging.md). Validation calculations:
`sagtension.nonlinear` gives both final states (`h_final_creep_n`,
`h_final_load_n`), `final_cause` per case (and the old global one as
`final_cause_global`), `load_final_over_initial` and `beyond_range`;
new `sagtension.nonlinear_criteria` and `sagtension.sagging`.

Other data files: structure models `.ohlfe` (`ohl-structure-model`,
version 2: the member kind `tension`; version 3: a member's `bending` and
`check`; version 4, 2026-10-27: `[analysis] single_angle_moments`,
`Analysis::single_angle_moments`, default false — a single angle's frame
end moments left out of its usage, `check::is_single_angle`; a member's
design data `[member.design]`, `Member::design`, module `design`;
`[analysis] capacity` — `Analysis::capacity`, `CapacityMethod::Asce10`
(`"asce10-97"`, the default: module `asce10`, `asce10::capacity` per
member, its assumptions in `MemberUsage::governs` after
`check::ASSUMED`), `Sia263` (`"sia263-2013"`, 2026-11-03: module
`sia263`, `sia263::capacity` → `(asce10::Capacity, Option<Stability>)`,
`Stability::usage` for formula (50), (62)/(63) and frames, design values
with γ_M) or `Simple` (`"simple"`, `check::SIMPLE`);
`check::capacity_note`; `check::axial_member` — single angles and members
with a design class axial unless `single_angle_moments`; bolts with
`count = 0`: holes only; a material's `ultimate`, `Section::ultimate`; in a
structure file the same `[analysis]` key, raised to version 8 by
OHL-Structure when set) and
poles `.ohlp` (`ohl-pole`) next to the `.ohls`
([Structure analysis](../user-guide/structure-analysis.md)); clearance
rules `.ohlr`; thermal ambient `.ohlta`; unit costs `.ohlcost`; galloping
`.ohlg`; LiDAR class schemes `.ohlc`; phase distance codes `.ohlpd`.
A project keeps its structure models, poles, clearance rules, ambient
cases, unit costs and galloping parameters as library files (below).

### Phase distance codes (`.ohlpd`)

`ohl-phase-distance`, version 1: a check plugin's values for the
distance between conductors in the span, read and written by the
plugin (its layout is the plugin's; a national-code plugin kept outside
the repository defines one). OpenOHL only measures
(`ohl_clearance::phase_distance`) and keeps a project's edited copy as
a library file of kind `phase-distance`. The check pairs conductors
between the same two structures (and, when asked, nearby conductors
elsewhere within a distance) and takes l_k from the strings at their
ends (one string: its length; two strings to one wire point, tension, clamped: 0; a string below two: its
length; two tension strings from a string: that string's), and the plugin columns of
their sections; the settings (the plugin's choices, station spacing,
circuits ticked, nearby pairs and their distance) are not in the file.
Results per pair: the worst station (points on the axes, where along
each conductor, d — between the conductors' surfaces since 2026-10, see
below — the required distance and the plugin's values) and the
length of conductor below it; conductors joined by spacers in the
span: each one's largest sag between its clamps. A run's parameters
keep the plugin's name and version and its choices. See [Distance
between conductors: the core's part](phase-distance-check.md).

### Clearance rules (`.ohlr`)

Source: `crates/ohl-clearance/src/rules.rs` (`RuleSet`, `RuleSet::to_toml`
writes one back). Full reference: [Terrain and clearances](../user-guide/terrain.md).

- `[[rule]]`: a target (LiDAR `classes`, the ground, other wires), a
  distance (`vertical` / `radial`, fixed or by voltage), the weather
  `cases` it applies to, `vertical_band` (zone width each side of the
  wire), `vertical_depth` (the area's depth along the line), `voltages`
  (kV of the wires checked; 0: earthed wires) and `wires` (`sections`,
  `jumpers` or `both`); since 2026-10-23 a ground rule's `surface` (a
  terrain surface of the project by name; none: the checks' surface;
  `Rule::surface`, `ZoneRule::surface`; the checks take the named
  surfaces as `ZoneTargets::surfaces` / `CaseTargets::surfaces`); since
  2026-10-27 `screen = true` (`Rule::screen`, `ZoneRule::screen`): the
  vertical requirement is the half width b of a band in plan by the
  wire's voltage, every point within it reported (`Required::band`,
  required `SCREEN_DISTANCE`; `ZoneFinding::band`, `zone::screen_text`).
  Since 2026-11-09 an optional `altitude = { from = "1000 m",
  per_1000_m = 0.14 }` (`rules::Altitude`; `Rule::altitude`,
  `ZoneRule::altitude`, `InternalRule::altitude`): the distances × (1 +
  per_1000_m · (h − from)/1000 m) above `from`. `Rule::required(kv, h)`
  and `InternalRule::required(kv, wind, h)` now take the height;
  `Wire::top()` / `checks::top_of` give a wire's highest point (a
  bundle's upper subconductor), `Wire::required(rule)` the requirement
  there; `Rule::reach` / `reach_in_plan` take the highest wire point of
  the case (`top`). Written only when given: still version 1, older
  files read the same.
  Since 2026-11-11 an optional `to_ground = true` / `false`
  (`Rule::to_ground`, `ZoneRule::to_ground`: `rules::ToGround` —
  `ByTerrain` not given, `Given(bool)`, and `Taken`, the state of a run
  that took it by default, never written; `ToGround::on`, `given`;
  point rules with a vertical requirement, not a screen). **Not given
  (every older file too): by the terrain's classes** — on in a run when
  all the rule's classes are classes the terrain surface is built from,
  the class scheme's ground role (`ClassScheme::codes_with_role("ground")`
  in the desktop and `ohl zones`, `--ground-role` in `ohl line`):
  `rules::to_ground_by_terrain`, `rules::default_to_ground` (rule files:
  the Clearances tab, `ohl line`), `zone::default_to_ground` and
  `ZoneRule::down_to_ground` (the vertical window, `ohl zones`), their
  notes ("… taken by default: …", "… not taken by default: only some of
  its classes …", `rules::NO_TERRAIN_CLASSES_NOTE`) in the results'
  notes. A copy of the rules is resolved for the run: the window's rows
  and the file keep "not given". On: each point stands for the height from the ground up to it —
  a wire within the band at or below the point's height has the
  clearance 0 (`Wire::vertical_gap`, the bounds `Block::vertical_low`;
  on a swing arc `ArcVertical::above`, `ArcPointCheck`). File **version
  2**, written only when a rule gives it, `true` or `false` (`RuleSet::written_version`,
  `BEFORE_TO_GROUND_VERSION` 1; `RULE_FILE_VERSION` 2 read); files
  without it are written as version 1. `rules::beside_notes`: the note
  "NOT ON THE SAFE SIDE: …" of a vertical point rule without it, a
  radial distance or a screen for its classes — in `ZoneReport::notes`
  (`check_zones`), the Clearances tab's `Report::notes` and `ohl line`'s
  output. A vertical window's row shows the three states
  (`clearance_zones::to_ground_ui`, `TO_GROUND_LABEL`, `TO_GROUND_AUTO`,
  `TO_GROUND_ON`, `TO_GROUND_OFF`, `to_ground_auto_label`,
  `TO_GROUND_HELP`; `clearance_zones::terrain_classes`).
- `[wind]` (optional): `steps`, `step` (Pa), `sides`, and since
  2026-10-20 `arc` (bool, default false; written only when true, so a
  file without it is written as before and older programs read it) —
  how the desktop clearance windows and `ohl zones` apply the wind: the
  steps up to each case's pressure, and/or each span's swing arc
  (`WindSteps::arc`; the danger-tree check's `positions::SwingArc`,
  shared). `ohl line` ignores it. A vegetation preset's `[wind] arc =
  true` turns its swing arc on too (its own `swing_arc` decides when
  given). Since 2026-10-21 `band_margin` (a length ≥ 0,
  `WindSteps::band_margin`, default `positions::BAND_MARGIN` 0.15 m;
  written only when not the default, so a file without it is written as
  before; older programs refuse a file with it, an unknown key): the
  swing band's margin, for the clearance windows, `ohl zones`, the
  vegetation panel (a preset's `[wind]`) and the safety volumes; a
  negative one refused. Since 2026-10-21 (the owner's decision)
  `jumper_band_margin` (a length ≥ 0, `WindSteps::jumper_band_margin`,
  default `positions::JUMPER_BAND_MARGIN` 0 m; written only when not 0):
  the jumpers' bands' own margin. `WindSteps::margins()` gives both as
  `positions::BandMargins { span, jumper }` (its `problem()` refuses a
  negative one). `zone::run_zone_check` takes the arc as
  `Option<BandMargins>` (none: no arc), its `ZoneCheckResult` has
  `band_margin: Option<BandMargins>`; `zone_arcs` takes the jumpers
  solved and the margins.
- `[check]` (optional, `CheckScope`): `circuits` (circuit names; empty:
  every circuit), `method` (`fe` / `rs`: every section solved by it;
  absent: each section's own), `search_distance` (m: LiDAR points
  farther from the wires are ignored), and since 2026-10-27 `routes` and
  `alignments` (ids; both empty: the whole line; else only the sections
  and jumpers with a structure on one of them,
  `CheckScope::wires_on` / `ohl_line::Line::wires_on`) — what the
  desktop clearance windows and `ohl zones` solve and read (both through
  `ohl_clearance::zone::run_zone_check`), and since 2026-10-27 the
  circuits, routes and alignments the Clearances window checks with that
  rule file; `ohl line` ignores it (`--clearance-route`,
  `--clearance-alignment` instead). Since 2026-10-28 `cases` (names,
  `*` `?`; empty: every case of the rules, `CheckScope::takes_case`):
  the weather cases the Clearances window runs, and `sections` (ids;
  empty: every section) the sections the Wire tensions, String checks
  and Internal clearances windows take with their circuits and routes,
  kept in their rule files' `[check]` with `local` (2026-11-03, Use local conditions; the old `structure_override`, `section_override` read and ignored), and the Internal clearances window's wind study `wind_study_min` (a pressure, > 0) and `wind_study_cases` (names, `*` `?`; empty: every case of the rules with wind) (2026-11-04, written only when given) (additive, optional:
  a file without it reads as before; `ohl line` and `ohl zones` ignore
  it); that window keeps its wind in `[wind]` (`stepped`, `step`,
  `sides`; since 2026-10-28 `arc`: the wires against each other swung
  together by one wind, `ohl_clearance::wire_arc` — `swings(arcs,
  cases) -> Vec<Swing>` from `zone_arc::zone_arcs`, `Swing::wires(arcs,
  k)` the wires at sample k; `SwingArc::swung` / `swing_reach`; the
  app's `clearance::Input::arc` the bands' margins; the file unchanged).
- The shared results table (2026-10-28): `check_ui::Cell::tip` (optional
  hover text of a cell, app only).
- Wire clearances across cases (2026-10-28, `ohl-clearance`):
  `check_case_pairs(rules, cases, voltages) -> Vec<CasePairCheck>` checks
  the rules between this line's wires with `case_pairs` across the cases
  given (name, wires), as `ohl line --weather` does; `check_case` is
  unchanged (a single case gives the same results). The app's
  `clearance::Report` holds the cases and wind positions checked and per
  rule, measure and wire pair the lowest finding (`Finding::case`,
  `state`, `positions`).
- Speed (2026-10-27, results unchanged): `checks::WireSet::within` lists
  each wire's segments only in the cells as far in plan as a point can
  count (`checks::plan_reach`: the radial reach and the outer radius, the
  vertical band — a screen's by voltage); `Required::radial_reach` (a
  screen: its band, not `SCREEN_DISTANCE`), `Rule::reach_in_plan` (the
  wire-to-wire sets of a rule file holding a screen); a `ZoneRule`'s
  screen only in the vertical measure. `ZoneReport::timing`
  (`ZoneTiming`, `PartTiming`) holds the measured breakdown,
  `zone::breakdown` says it in lines; `zone::time_limit(points)` = 900 s
  + 20 s per million LiDAR points of the rules' classes.
  `ohl_pointcloud::ReadProgress` (`visit_around_with`,
  `PointSource::visit_with`, `Store::visit_with`): the points of the
  blocks listed and decoded so far, for the progress bars.
- `[vegetation]` (optional, 2026-10-20, `VegetationPreset`): settings of
  the vegetation and danger-tree check, each optional — `classes` (LiDAR
  class numbers), `clearance` and `fall_allowance` (a distance, a table
  by voltage or a formula, as `vertical`), `growth_per_year`, `years`,
  `buffer`, `fall_in`, `root_ball`, `fall_through_ground` (since
  2026-11-05), `min_tree_height`, `max_tree_height`,
  `max_base_ground_distance`, `search_distance`, `swing_arc`, and since
  2026-10-21 `cases` (the weather cases the check runs in, names of the
  preset's cases with `*` `?`; written only when given); the wind
  steps are the file's `[wind]`. A file may hold it without any
  `[[rule]]`: a design code's preset of the vegetation check
  ([plugins](plugins.md#vegetation-presets)). Unknown keys are refused.
  Older programs refuse a file with it (an unknown table).
- `checks` (optional, top level, 2026-10-21, `RuleSet::checks`,
  `PresetCheck`): the checks a preset is for, of `vertical`, `radial`,
  `vegetation` (an unknown one refused; written only when given); absent,
  `RuleSet::is_for` tells them from the content (a vertical distance, a
  radial one, `[vegetation]`). Each place takes only its part of a preset
  ([plugins](plugins.md#presets-per-check)). Older programs refuse a file
  with it (an unknown key).
- The desktop Vertical and Radial clearance windows keep their rules next
  to the model, in `<model>.vertical-clearances.ohlr` and
  `<model>.radial-clearances.ohlr` (one `[[rule]]` per row and target
  kind). A window row (`ohl_clearance::zone::ZoneRule`) has its
  `voltages` (kV, 0 for earthed wires; empty: every wire — optional with
  any distance; since 2026-10-21 a list, written as the rule's
  `voltages`, so a rule of several voltages is one row: it was one
  `voltage` per row, a rule listing several read as one row each; the
  file format is unchanged) and an optional `width` of its own (the vertical zone's half width, written as the
  rule's `vertical_band`; none: the window's `Zone.width`); the window's
  `Zone.search` is `[check] search_distance`. Rules refer to LiDAR
  classes by number (`.ohlc` schemes) and to weather cases by name.
- **Results are to the conductor's surface** (derived, never stored):
  `ohl_clearance::Hit::distance` (the CSVs' clearance columns, the
  windows, the sheets), the vegetation distances and fall-in margins,
  galloping gaps, right-of-way offsets and the phase-distance stations'
  d are the distance from the axis less `Wire::allowance` — the
  subconductor's `surface_radius` (half the iced diameter in the case,
  `WeatherCase::iced_diameter`) and the `polyline_error` bound — of each
  wire (`Hit::wire` stays the axis point; what is drawn is
  `Hit::drawn(vertical)` / `ZoneFinding::drawn(measure)`: since
  2026-10-22 a vertical clearance from the target straight up to the
  wire's height, `checks::vertical_mark`, the same x and y at both ends;
  `clearance::Finding::at` in the app holds that point). Both come from the solve:
  (`SpanShape::end_pulls`, 2026-11-10: the wire's pull on each of the
  span's two end clamps, `[Point; 2]`, N, line axes, the whole bundle's —
  the end force of the span's first and last catenary element as solved,
  an output only, derived, never stored or part of a cache key: the
  shapes are not serialised and the memo keys are the inputs; the two
  spans' pulls at a clamp add up to `SectionShape::pulls` there; ruling
  span: the end tension along its curve's end tangent; a broken span:
  zeros.)
  `SpanShape` and `JumperShape` (`ohl-line`) carry `surface_radius` and
  `polyline_error` (`ohl_line::solve::polyline_error`: the largest
  sagitta of a piece on a circle of radius H/w; 0 for a rigid or stiff
  jumper). The radial ground clearance is the samples' shortest
  distance lowered by `checks::sampled_floor`. Until 2026-10 every
  clearance was to the axis, without these.
- **A rule's calculation** (2026-11-01, additive): `[[rule]]`,
  `[[tension_rule]]`, `[[string_rule]]` and the galloping file may hold
  `calculation` — `ohl_line::condition::Calculation` by its file name
  (`max-sag-fe`; the list's label reads too); none: each section's own
  (`ohl_clearance::calculation`: one solve per weather case and
  calculation, the other rules set aside meanwhile). `ZoneRule` carries
  it from the `[[rule]]`; the tension and string rows carry it
  (`calculation`) and their CSV's case column names it ("everyday,
  Initial RS").
- **Lightning shielding rules** (2026-10-31, additive): a rule file
  may hold optional `[[lightning_rule]]` tables — `name`, `cases`,
  `shield_cases` (2026-11-01: the shielding wires' weather cases; none:
  the phases' case), `calculation` and `shield_calculation` (2026-11-01:
  `ohl_line::condition::Calculation` by its file name, `max-sag-fe`;
  none: each section's own), `method` (`angle`: `outer_angle`, `inner_angle` °; `sphere`:
  `striking_distance` m or `stroke_current` kA with `distance_a`,
  `distance_b` of r = A·I^b, and `ground_factor` β), `step` (m, default
  5), `shields` (wire types; none: the earthed wires), `targets` (as the
  tension rules') — read as `RuleSet::lightning_rules`
  (`ohl_clearance::lightning::LightningRule`), written back by
  `RuleSet::to_toml`; `checks` may name `"lightning"`
  (`PresetCheck::Lightning`). The check is
  `ohl_electrical::lightning::check` (per span, cross-sections square to
  it, `cross_section::cross_section_reached` leaving out a wire that does
  not reach a cut): `Report { rows: Vec<Row>, cuts: Vec<Cut>, notes }`;
  a `Row` per phase, span, rule and case (length exposed, worst place,
  depth, margin: the angle needed or r_max with its current, the exposed
  parts as world polylines), a `Cut` per cross-section (its plane, the
  ground, the shielding wires and phases, the covered zone's boundary,
  sphere centres); its CSV `rule, method, case, section, span, from, to,
  phase, span_length_m, exposed_m, worst_along_m, worst_depth, margin,
  current_ka, status`; `ohl lightning --json` writes the `Report`. The app
  keeps the window's rules in `<model>.lightning-rules.ohlr`; the 3D
  layer `Layer::Lightning` (`lightning::scene`).
- **Ice shedding rules** (2026-11-02, additive; roadmap O13): a rule
  file may hold optional `[[ice_shedding_rule]]` tables — `name`,
  `cases` (the iced cases, patterns; none: every case with ice), `bare`
  (a weather case's name; none: each iced case without its ice,
  `WeatherCase::without_ice`), `calculation` (as the tension rules'),
  `method` (`simplified`, the default and only one so far:
  `ice_shedding::Method`), `shedding` (`one`, the default: each span
  alone, its iced tension held; `all`: every span of the wire, the bare
  case solved — `ice_shedding::Shedding`), `factor` (the rebound factor,
  > 0; `ice_shedding::DEFAULT_FACTOR` = 2 when not given, since
  2026-11-02), `distance` (required, as a rule file
  writes a distance: fixed, `by_voltage` or a formula; above 0),
  `pair_voltage` (`higher` default, `sum`, `difference`), `targets` (the
  wires that shed, as the tension rules') — read as
  `RuleSet::ice_shedding_rules`
  (`ohl_clearance::ice_shedding::IceSheddingRule`), written back by
  `RuleSet::to_toml`; `checks` may name `"ice-shedding"`
  (`PresetCheck::IceShedding`). The check is
  `ohl_clearance::ice_shedding::check` (what to solve:
  `ice_shedding::solves`): `IceSheddingReport { rows:
  Vec<IceSheddingRow>, jumped: Vec<JumpedSpan>, notes }` (serde), a row
  per jumping span, other wire of its span, rule and iced case (`rule`,
  `case`, `bare`, `calculation`, `section`, `span`, `other`,
  `other_span` — spans from 0 —, `rise`, `jump`, `before`, `after`,
  `required` m, `kv`, `at` fraction, `points` the closest points), the
  smallest margin first; a `JumpedSpan` per jumped span (`rule`, `case`,
  `section`, `span`, `points`, `short`). Its CSV `rule, case, bare_case,
  section, span, other_section, other_span, factor, rise_m, jump_m,
  before_m, after_m, required_m, margin_m, status, at_fraction, x, y, z,
  other_x, other_y, other_z` (spans from 1); `ohl ice-shedding`. The app
  keeps the window's rules in `<model>.ice-shedding-rules.ohlr`; the 3D
  layer `Layer::IceShedding` (`ice_shedding::scene`); the panel
  `Panel::IceShedding`.
- **Tension rules: other limits, sag to limits** (2026-11-11, roadmap
  O22 item 3): a `[[tension_rule]]` may give, in place of `factor`,
  `tension` (a force) or `catenary_constant` (a length: H over the bare
  weight per metre of the wire as strung, the measure then horizontal) —
  `TensionRule::{tension, catenary_constant}` (`Option<f64>`, N and m),
  `TensionRule::limit() -> RuleLimit`, `measured()`, `allowed(rts,
  weight)`; exactly one limit (`RawTensionRule::resolve`). Rule file
  version **2** (`RULE_FILE_VERSION`), written only by a set with such a
  rule (`RuleSet::to_toml`, else 1). The CSV gains a last column `limit`.
  `tension::sag_to_limits(line, library, rules, weather, sections,
  filter, budget) -> SaggedToLimits { sections: [SagToLimits { section,
  stringing?, governing?, usages, trials, stopped, notes }], line, notes
  }`: per section in the blocks the largest `Stringing::Tension` (its
  temperature and `condition` kept) within the rules, by bisection on
  the rows of `check` (`span_rows`, one implementation), each section
  alone (`solve::solve_one`); no state is kept — `ohl tensions
  --sag-to-limits [--sections] [--write] [--user] [--seconds]` writes the
  stringings back (a line file rewritten from the stored line; a project
  through `Command::SetStringing`). Validation calculation
  `line.sag_to_limits`.
- **Tension rules** (2026-10-22, additive): a rule file (`.ohlr`) may
  hold optional `[[tension_rule]]` tables — `name`, `cases` (patterns;
  none: every case), `factor` (> 0: the allowed tension is factor × the
  wire type's RTS), `measure` (`max`, the default: the tension at the
  span's most tensioned support, `SpanShape::max_tension`; or
  `horizontal`), `targets` (`{ wire_types, systems, circuits }`, each
  optional; none: every wire) — read as `RuleSet::tension_rules`
  (`ohl_clearance::tension::TensionRule`), written back by
  `RuleSet::to_toml`; files without them read unchanged. `checks` may
  name `"tension"` (`PresetCheck::Tension`). The check
  (`tension::check`) gives `TensionReport` rows (rule, case, section,
  span, wire type, tension, RTS, allowed, usage, the marked point), its
  CSV `rule, case, section, span, wire_type, measure, tension_n, rts_n,
  factor, allowed_n, usage_pct, status, x, y, z`. The app keeps the
  window's rules in `<model>.tension-rules.ohlr` among the project's
  clearance rules. Since 2026-10-22 too, a span above its wire's RTS
  adds a warning to `SectionShape::flags` (`solve::rts_flags`,
  `solve::above_rts`).
- **String rules** (2026-10-24, additive): a rule file may hold optional
  `[[string_rule]]` tables — `name`, `cases` (patterns; none: every
  case), `strings` (`all`, the default, `suspension` or `strain`),
  `voltages` (kV of the strings' sections; none: every voltage), `load`,
  `swing`, `uplift` (booleans, default true: the strings' own maximum
  load and swing limits, the suspension strings going slack), and the
  rule's own `max_swing_deg`, `max_swing_across_deg`,
  `max_swing_along_deg` (optional, above 0° and at most 180°) — read as
  `RuleSet::string_rules` (`ohl_clearance::strings`); a preset declares
  the check with `checks = ["strings"]`. The solved strings
  (`SectionShape::strings`, `StringState`) carry `strain` (a dead end as
  the solve treats it), new.
- **Internal clearance rules** (2026-10-25, additive): a rule file may
  hold optional `[[internal_rule]]` tables — `name`, `cases` (patterns;
  none: every case), `voltages` (kV of the parts' sections; none: every
  electrified part), `distance` (a requirement, required), `conductors`,
  `jumpers`, `string_ends` (booleans, default true) and `reach` (a
  length above 0, default `"20 m"`) — read as `RuleSet::internal_rules`
  (`ohl_clearance::internal::InternalRule`), written back by
  `RuleSet::to_toml` (defaults left out); a preset declares the check
  with `checks = ["internal"]` (`PresetCheck::Internal`). The check
  (`internal::check`) takes the solved sections and jumpers per case,
  the bodies (`StructureBody`: per structure its `BodyMember`s — id,
  centre line, solid triangles, project coordinates; the CLI builds them
  from `ohl_structures_fe::drawn::Bodies`, a pole as one member `pole`),
  the rules, the case overrides (`ohl_weather::CaseFilter`; the desktop passes an empty one since 2026-11-03) and a
  budget; it gives `InternalReport` — `rows` (`InternalRow`: rule, case,
  structure, `Part` = `Conductor { section, span }` / `Jumper(id)` /
  `StringEnd { label }`, nearest member, distance to the wire's surface,
  required, `to_surface`, the two nearest points; the smallest margin
  first), `not_checked` (structure, why: level 1) and `notes`. CSV
  `rule, case, structure, part, member, distance_m, required_m,
  margin_m, to, status, x, y, z, member_x, member_y, member_z`.
- **Internal clearances, the insulation condition** (2026-11-11):
  `[[internal_rule]]` takes an optional `insulation = "low"` / `"high"`
  (`InternalRule::insulation`: `ohl_line::electrical::InsulationLevel`?;
  `takes_insulation`, `label` — the rule's name with the condition, in
  the rows, the CSV and the command line —, `describe_parts`). A rule
  with it takes only the parts of sections whose system has that level
  (`System::insulation`, high when not given; in `internal::check` the
  per-rule filter beside `takes_voltage`); without it every part, as
  before. `internal::no_rule_for_insulation`: the note "NOT CHECKED, NOT
  ON THE SAFE SIDE: no internal rule for <circuit> (insulation …)" for a
  circuit whose voltage only rules of the other level take. Rule file
  **version 2**, written only when a rule has the key
  (`RuleSet::written_version`); older files read unchanged. The window's
  rule editor: `internal_clearances::insulation_ui`, `INSULATION_HELP`.
- **Internal clearances, the volumes on the strings** (2026-10-25,
  additive): `[[internal_rule]]` takes `shapes` (default true) and an
  optional `distance_wind` (a requirement for the cases with wind;
  `InternalRule::required(kv, wind)`; since 2026-11-09 with the height,
  `required(kv, wind, h)`, for the optional `altitude`). `internal::check` now takes the
  cases as `SolvedCase { name, wind, sections, jumpers }` and the
  strings with volumes (`StringFit`: structure, label, the assembly's
  `StringVolumes` with the string's own shapes merged, its still-air top
  and wire point, the structure's orientation; `internal::string_fits(line,
  insulators)`); `Part::Shape { label, shape, copy }`;
  `InternalRow::grounded` names a grounded shape nearer than every member
  (`InternalRow::against()`: the member column's text, `shape string S1
  …`). The geometry is `ohl_clearance::volumes` (`Convex`, `separation`
  — GJK, a lower bound —, `place`, `StringAt`, `Placed`, `Convex::mesh`
  for the views).
- **Requirements** (`ohl_clearance::rules::Requirement`): `vertical` /
  `radial` are a distance (`"7 m"`, `Fixed`), a table
  (`{ by_voltage = [[up to kV, "m"], …] }`, `ByVoltage`: voltages ≥ 0
  and increasing, the first row the voltage does not exceed; above the
  last row an error) or a formula
  (`{ base = "m", per_kv = "m", above_kv = kV }`, `Formula`: base +
  per_kv · max(0, U − above_kv); `per_kv` written as a length, m per kV).
  `Requirement::to_toml` writes each form back exactly;
  `Requirement::problem` is what an editor reports (distances above 0,
  rows increasing). Since 2026-10-20 a window row's `distance`
  (`ZoneRule::distance`) is such a `Requirement` too (was a distance in
  m): a rule by voltage stays one row (imported and written as it is),
  each wire checked with its voltage's distance (`WireResult::required`;
  `ZoneFinding::required` is the wire's); a table not reaching a wire's
  voltage is an error of `check_zones`.
- **Swing arcs in the clearance windows** (2026-10-20; derived, never
  stored): `zone::wind_states` adds each windy case's still air when the
  arc is on; `zone::arc_cases` names the positions of each case's arc
  (`ArcCase`: still, full left, full right), `zone::zone_arcs` makes one
  `ZoneArc` per span (its still wire, its `SwingArc` — a band since
  2026-10-21 — its largest radius); `check_zones` takes them (`&[]`: none) and
  `run_zone_check(…, arc, …)` makes them. A finding on the arc has
  `ZoneFinding::arc` = the angle (degrees from the downward vertical
  about the span's axis, positive to the right looking along the span)
  and `state` = its case's still air (`ZoneFinding::position` /
  `wind` say it in words). `ZoneCheckResult` has `arcs` (the cases) and
  `arc_spans`. The `ohl zones` CSV has a last column `arc_deg` (empty
  off the arc; on it `wind_pa` is the case's pressure, `wind_from`
  `arc`).
- **Point and ground checks, speed** (2026-10-20, numbers unchanged bit
  for bit): `checks::PointCheck` keeps `best` (the smallest clearance
  per wire and measure), `PointCheck::only(vertical, radial)` (the zone
  windows work out their one measure), `eval` / `apply` (a point's
  `PointEvent`s worked out on any core from a snapshot, applied in the
  points' order); `checks::GroundMax` (the highest ground per 5 m cell,
  in 64 × 64 tiles) bounds `ground_check` / `ground_check_with`, which
  checks the wires on every core. `checks_reference.rs` keeps the
  earlier code for the tests that compare them.
- The Clearances window's rule editor (`ohl-app` `rule_editor`) edits a rule
  file in place with `toml_edit` (names, `vertical`, `radial`, rules
  added and removed, a value's trailing comment kept; an unchanged
  requirement keeps its text); a project's rule file is written as one
  `put_library_file` (one command, undone by Undo).
- **Danger trees** (2026-10-20; derived, never stored in the project):
  `ohl_clearance::vegetation::VegetationParams` — `clearance`,
  `growth_per_year`, `years`, `buffer` (b), `fall_in`, `root_ball` (k,
  0 = as before), `fall_through_ground` (2026-11-05, default false: the
  trunk rests on the ground; true: it turns on to −90°), `fall_allowance`
  (A), `min_tree_height`,
  `max_tree_height`, `max_base_ground_distance` (`Option`, m: a tree
  base only with a TIN vertex within it in plan; none = no limit), `keep`
  — all inputs of a run (command-line flags, the window's fields), not
  saved. Defaults since 2026-10-20 (`Default`, the command line and the
  application alike): `DEFAULT_ROOT_BALL` 0.05, `DEFAULT_FALL_ALLOWANCE`
  2 m, `DEFAULT_MAX_BASE_GROUND_DISTANCE` 5 m, and
  `DEFAULT_SEARCH_DISTANCE` 50 m for the corridor read (`--corridor`,
  the window's search distance); before, 0, 0, none and 60 m
  (`PROPOSED_ROOT_BALL` 0.1 in the application, removed). Unbalanced ice:
  `positions::UnbalancedIce` — `CaseIce`, `ExtraThickness(m)`,
  `ExtraLoad(N/m per wire)` (until 2026-10-20 `Extra`, the whole
  bundle's), `Concentrated(N, the bundle's)`; `UnbalancedIce::cases`
  gives the section's and the span's weather case, solved by
  `ohl_line::solve::solve_section_span_cases`. The wire
  positions are `ohl_clearance::positions::Positions`: `cases` (names),
  `spans` (`Span`: the span's `Wire`, its `Axis` A₁A₂ through the
  attachments, its positions, its capsule and `Bands`), `positions`
  (`Position`: span, case, `PositionKind` — `case`, `left`, `right`,
  `arc`, `ice`, `ice left`, `ice right`, and since 2026-10-20 `still`,
  `left (step)`, `right (step)` of a stepped wind
  (`PositionOptions::wind_steps`, a `rules::WindSteps`) with its
  `pressure` (Pa) — and its `Shape`: a solved
  `WireShape` or a `SwingArc` with its stations and the three wires'
  points at each), `notes`. Since 2026-10-21 a `SwingArc` is a band:
  `SwingArc::place(s)` gives an `ArcPlace` with `centre`, `radius` (the
  largest of the three, R), `inner` (the smallest, r), `from`, `to`;
  `SwingArc::nearest` is 0 inside the band; `outer` and `allowance()`
  include the band's margin (every distance taken that much smaller):
  `positions::BAND_MARGIN` (0.15 m) by `SwingArc::new`, any ≥ 0 by
  `SwingArc::with_margin` (2026-10-21; `margin()` gives it);
  `thickness()` and `line_at_radius(φ, t)` (t from the
  inner, 0, to the outer radius, 1) are new. `PositionOptions::jumpers`
  (2026-10-21, default false; the safety volumes set it): the jumpers too,
  each a span of its own (section `jumper:<id>`, span 0, its axis from
  its first to its last point as first solved, blown left) at its solved
  positions in every solve (left, right, every step; still air when the
  arc is off) and, with `swing_arc`, a `PositionKind::Arc` position of
  its own (`positions::jumper_band`: the band about the line through its
  end clamps blown left; a jumper without one keeps its still air, kind
  `still`, noted). `PositionOptions::band_margin` (2026-10-21, default
  0.15 m; `PositionOptions` no longer derives `Default`, its `Default`
  sets it): every span's band's margin; `PositionOptions::jumper_band_margin`
  (2026-10-21, default 0 m): every jumper's band's. `VegetationParams::clearance_by_voltage` and
  `allowance_by_voltage` (a `rules::Requirement`, else `clearance` and
  `fall_allowance`) give each span its distance by its wire's voltage
  (`VegetationCheck::required`). Results per span:
  `vegetation::WireVegetation` (counts, `nearest`, `worst_fall`,
  `worst`, and with `keep_all` `all`: every grow-in and fall-in point as a
  `Found` — point, grows, falls, position, fall position, a tree's base),
  each `VegPoint` with its `position`, `fall_position`,
  `fall_point` (indices into `positions`) and the `required` clearance
  applied. Since 2026-11-11 the **questionable points**
  (`vegetation::Questionable`: `point`, `class`, `span` — the nearest in
  plan —, `distance` in plan from its swept volume, `why`:
  `NoBase::NoGroundBelow` / `GroundPointTooFar`): the points without a
  tree base within `VegetationParams::fall_reach` of a span in plan,
  where a ground surface was given — `VegetationCheck::questionable_points`
  (all) and `VegetationCheck::questionable()` (the nearest
  `QUESTIONABLE_KEPT` = 10 000, nearest first, the same however the
  points were shared between threads); `vegetation::questionable_note`
  (the report's "NOT CHECKED, NOT ON THE SAFE SIDE: …") and
  `questionable_csv` (rows appended to the danger CSV, the kind
  `questionable: <why>`). `without_ground` and `ground_too_far` count as
  before. The danger CSV
  (`vegetation::DANGER_CSV_HEADER`) has a row per kept point with its
  case and position; the older per-case CSV (`vegetation::CSV_HEADER`,
  `to_csv`) remains for a single case. Isoclearance lines:
  `isoclearance::IsoParams` (`interval`, `max_level`, `spacing`,
  `coarse`), `Contour` (`level`, `points` on the ground, `closed`);
  written as GeoJSON (LineStrings, `level_m`, project coordinates, `crs`
  when known), CSV (`level_m,line,vertex,x,y,z`) or DXF; on drawings
  `ohl_drawing::RouteDrawing::isoclearance` (traces along the route) on
  the layer `OHL_ISOCLEARANCE` (`ohl_drawing::model::Layer::Isoclearance`).
  The application's layers `GrowIn`, `FallIn`, `Isoclearance`
  (`ohl-app` `scene::Layer`) draw `vegetation::Overlay`; its drawings
  export carries the last run's lines (`drawings::IsoLines`).
- **Clearance presets** of design-code plugins are such rule files,
  `<plugin>-<name>.ohlr` among the project's rule files, each with a
  weather case file of the same name among its weather case files (the
  cases its rules name); the rule tables' Preset dropdown lists them (*This project*).
- **Rule presets** of design-code plugins (optional `rule-presets`
  export, 2026-10-20) are rule files of any targets with a description and
  optional weather cases (`ohl_io::codes::RulePresetChecked`); the app
  reads them from the plugins when a check window with rules
  opens (the rule tables' Preset dropdown; in the Clearances window merged into
  the chosen rule file or replacing its rules, the preset's tables copied
  as written); the project's own presets are read into the same form
  (`ohl-app` `rule_table`, 2026-10-28); `ohl code generate` and `ohl code
  rule-presets --out` write them as `<plugin>-rules-<name>.ohlr` (and
  `.ohlwc`). Applied in the app, a preset's weather cases are matched to
  the project's by physics first, then by name (2026-10-21, `ohl-app`
  `preset_cases::Plan`): the rules rewritten to name an equivalent
  project case, only the others created (`"<name> (preset)"` on a name
  clash).

### Structure type (`.ohls`)

Source: `crates/ohl-structures/src/structures.rs`. All positions in
**local axes**. Full reference: [Structure library](../user-guide/structure-library.md).

| Field (TOML key) | Type | Unit | Meaning |
|---|---|---|---|
| `format`, `version` | optional | — | `"ohl-structure"`, 1 to 7 (`STRUCTURE_FILE_VERSION`; 7 adds a load case's two parts `[[load_case.line]]` and `[load_case.structure]`, written only when a case has them, the file raised after any edit leaving one (`ohl_structure_edit::PARTS_VERSION`); 6 adds the body generators' options in `[[generator]]`, a member's `bending` and `check` (written only when a member uses them) — and anything else added before the next release —, written only when a generator uses one (`ohl_structure_edit::body::OPTIONS_VERSION`), and the recipe quantities `structure_wind` and `structure_wind_factored` in `[[load_case.load]]`, the file raised to the same 6 after any edit leaving such a load (`raise_for_line_pressures`); 5 adds the member kind `tension`, written only when a member is made `tension`; 4 adds a string's optional limits `max_load`, `max_swing`, `max_swing_across`, `max_swing_along`, not checked — written only where given (the structure application raises the file to 4 when one is set, `ohl_structure_edit::raise_version`); 3 adds `group`, `made_by`, `[[generator]]`, `[[symmetry]]`, the `[pole]` table and an extension's `node`, `member` and `leg` tables, written by the structure application — it raises a file's `version` when it writes them; older versions read as before) |
| `id`, `name`, `description` | string | — | unique id (referred to by `Structure.type`), display name, free text |
| `kind` | `lattice` / `pole` / `portal` / `other` | — | family; a lattice tower or portal has joint details (`ohl_structure_edit::is_tower`), a pole the Pole panels; OHL-Structure changes it (Geometry › Structure kind, `set_structure_kind`, 2026-11-02), nothing else changes (`kind_change_note` says what stays unshown) |
| `height` | length? | m | shaft top for the default stick drawing |
| `foot` | length? | m | local z of the feet (default 0: ground) |
| `source` | string? | — | where the data comes from |
| `[[point]]` | `TypePoint` | m | an **attachment point**: `label`, `description`, and `point` **or** `node` (joint id); version 6: `offset` (with `node`: from the joint, local axes — a rigid link; refused with `point`). A point cannot be put on a member (owner decision 2026-10-21): **no more `member` + `at` to define a point**. Files written before that give `member` + `at` (0–1 from its first joint) are read (`StructureType::prepare` → `ohl_structures::on_members`: `plan`, `apply_to_table` for the finite-element tables, `ohl_structure_edit::links::insert_joints_on_members` for the file as edited) as a `[[node]]` of `kind = "attach"` inserted there (id = the first point's label there, `-a`, `-a2`… when taken; points at one place share it; `at` 0 or 1: the end's joint, nothing inserted), written exactly (`"{:?} m"`), after the other nodes; the member split into continuous pieces — the first keeps its id and place, its second joint the inserted one; the further pieces (`<id>-2`, `<id>-3`…, the next free) after the other members, every field of the member copied; each piece's `lk` the whole member's (its own `lk`, else `k` × its length, `k` default 1) — and the point's `node` the joint (its `member`, `at` gone). OHL-Structure and the structure screen write the file so when it is next saved (version 6; shown as changes not saved). Every point is linked to a joint when read (`TypePoint::joint`, `StructureType::joints`, `point_joints`): its `node`; a point at its own position: the node naming it as `attachment`, else a node at that position (within `SAME_JOINT`, 1 µm), else its own joint (`JointSource::Point`; id = its label, `-a`, `-a2`… when a node has it). Several points may share a joint. Only **attach joints** take a link (`ohl_structures::NOT_ATTACH`; `ohl_structure_edit::links::link_point`, `ohl_line::links::linked`, `match_points`' exact match). A linked point **moved** stays on its joint, its `offset` recomputed (`ohl_structure_edit::links::move_point`; the joint moved keeps the offset). In the finite-element model (`Model::joint_points`, `Model::point_load`, `Model::linked_load`, `ohl_structures_fe::links`): the force at the joint, plus r × F where a frame member meets it (only trusses: left out, `PointLoad::note`); a joint carrying a point where only bars meet, in a line or a plane, loaded across them: the solve refused (`Model::bar_joint_error`, `FeError::Invalid`). `Model::member_points`, `Model::split`, `Solution::pieces` and the share to a member's ends are gone. A point on its own joint on a structure with members is a missing item for level 3. `ohl_structure_edit::links::insert_joint` inserts a joint on a member (the same split) |
| `[[wire]]` | `TypeWire` | m, N | `label`, `description`, `set?`, `point?` (else it hangs below its strings), `weight?` |
| `[[string]]` | `TypeString` | m, N, m² | `label`, `description`, `top`, `wire`, `insulator?` **or** `length` + `weight`, `compression?`, `wind_area?`, `strain`, `angle?` (from straight down), `direction?` (in plan, 0 ahead); version 4: `max_load?` (force), `max_swing?`, `max_swing_across?`, `max_swing_along?` (angles) — limits not checked (`TypeString::limits`, `InsulatorLimits`; placed: `PlacedString::limits`, its own only); version 7: `[[string.shape]]` (`TypeString::shapes`, the `.ohli` `[[shape]]` keys with units: its own volumes over its assembly's, `PlacedString::shapes` → `InsulatorString.shapes` on resolve, 2026-10-25); `side` (`ahead`/`back`) in files written before: level, 0° or 180° |
| `[[attachment]]` | `TypeAttachment` | — | version 1: `set`, `phase`, `point`, `insulator` or inline `string`, `side` — still read; `ohl structures derive` (`ohl_structures::derive`) writes version 6 instead (2026-10-21): per attachment a `[[node]]` `A<set>-<phase>` of `kind = "attach"`, a `[[point]]` of that label on it, a `[[wire]]` (label: the attachment's, `W<set>-<phase>` with inline strings; `set` = `set_from_v1`) and a `[[string]]` `S<set>-<phase>` (`insulator`, or inline `length` + `weight`; a strain one `strain = true`, `angle = 90 deg`, `direction` 0 or 180 deg); without a string a `[[wire]]` fixed at its `point` |
| `[[extension]]` | `Extension`: `name`, `dz`; version 3: `[[extension.node]]` (`TypeNode`), `[[extension.member]]` (`TypeMember`), `[[extension.leg]]` (`LegExtension`: `leg`, `dz`) | m | extensions; at level 3 a piece below the feet (joints with z from `foot − dz` to `foot`, ids unique with the body's; members between its joints and the body's) and leg extensions. `StructureType::extended(ext) -> ExtendedBody` applies the rule (`crates/ohl-structures/src/extensions.rs`, [user guide](../user-guide/structure-library.md#extensions-and-leg-extensions)): the piece added, body joints it reaches lose their support, a leg's support joint slides `dz` lower along its steepest rising member; a placed structure on such an extension is analysed on `Model::from_structure_type_extended` (`ohl_loads` keys its models by type and extension) |
| `[[body]]` | `points` | m | outline polylines (stick drawing) |
| `[[node]]` | `id`, `point`, `support?`, `spring?`, `attachment?`, `kind?`, `made_by?` | m | structure joints (detail 2–3); `made_by`: the `[[generator]]` that wrote it; version 6 `kind` (`JointKind`): `attach` (strings hang here) or `support` (the feet; its fixity is `support`) — only these two, none written: worked out (`Joint::kind`: attach when a point is linked to it or it names an `attachment`, support when supported); a point on a `support` joint refused; `kind = "support"` without a `support` a missing item for level 3. `ohl_structures_fe` leaves `kind` out of the model |
| `[[member]]` | `id`, `nodes` (2 ids), `section?`, `kind?` (`truss`/`frame`/`cable`; version 5, `.ohlfe` version 2: `tension`, a truss in tension and in compression up to its own buckling load — `MemberKind::Tension`, `solve::buckling_loads` (π²EI_min/L_k², L_k = `lk` or `k`·L; 0 without a second moment), beyond it bowed (`MemberForces::bowed`, axial −N_cr) or, with 0, slack (`MemberForces::slack`) by the solver's active-set iterations, `Solution::passes`; kept active as a truss only as the last resort when the set does not settle even with its ties split, `Solution::kept_active`, approximate), `bending?` (version 6, `.ohlfe` version 3: a `tension` member that also bends and twists as a frame while it is in the model — `Member::bending`, `Member::is_frame`; a tower program's tension-only beam; bowed or slack: as a truss, without bending; kept active as a frame; refused on another kind), `check?` (version 6, `.ohlfe` version 3: `false` — a modelling member, a tower program's fictitious member: solved, no usage, `Member::check`, `check::NOT_CHECKED`), `reference?`, `roll?` (an `Angle`; since 2026-11-04 also with `orientation`: a turn after it, the member's correction angle — `Member::roll` after `orientation_reference`; OHL-Structure writes it in degrees, `ohl_structure_edit::set_member_roll`, `member_roll`; it turns the member's axes for the analysis, its member wind and the exports), `orientation?` (an angle's legs towards two structure directions, `"x+y-"`, instead of `reference`), `k?`, `lk?` (effective length instead of k × length), `pretension?`, `capacity_tension?`, `capacity_compression?` (forces replacing the computed capacities in the usage), `dead_load_factor?` (on its self-weight, instead of `[analysis]`'s), `design?` (version 8, `.ohlfe` version 4, 2026-10-27: `[member.design]`, `TypeMember::design` raw, read into `ohl_structures_fe::design::MemberDesign` on `Member::design` — `class` (`MemberClass`: leg, bracing, redundant, crossing), `crossing` (partner id → index), `length_ratios` [y, z, v], `end_eccentricity` and `end_restraint` (`Ends`: none, one_end, both_ends), `width_thickness`, `thickness`, `plies`, `connected_leg` (`ConnectedLeg`: both, long, short, none), `bolts` (`Bolts`: `diameter`, `hole?`, `count`, `shear_planes` default 1, `shear_capacity?` per bolt and plane, `ultimate?`, `holes?`, `grade?` — `design::Grade`, "8.8", 2026-11-03), `edge_distance`, `end_distance`, `spacing`; all optional, read by the capacity method), `group?`, `made_by?` | m, N | structure members; `group`: its member group (no table of groups: a section — or `lk`, the capacities, the dead-load factor — per group is written on each member); `made_by` as for joints. `ohl_structures_fe` leaves `group` and `made_by` out of the model |
| `[analysis] capacity = "plugin:<plugin>/<method>"` | A plugin's member check instead of the core's methods (2026-11-11; structure file version 12, `.ohlfe` version 5, written at the newer version only when used — OHL-Structure raises the file when it is chosen): `ohl_structures_fe::PluginMethod` in `Model::capacity_plugin` (`plugin`, `method`, `dir` once a runner found it); `Analysis::capacity` then stays the default but is not used for anything checked: the joints (plates, covers, welds, bolt groups) go to the plugin's optional `joint-check` export and the bolt tools' minimum distances to its `bolt-distances` export (`ohl_member_check::joints`; `PlatePlans::plugin`, `plate_usage_cases`), shown as "NOT CHECKED by <plugin>, NOT ON THE SAFE SIDE" / "no minimum distances from <plugin>" where it gives none; `bolt_rules::rules_in` gives none for such a model. Resolved and run by the core crate `ohl-member-check` (`Plugins/` beside the structure, then the user's plugins; re-exported as `ohl_structure_link::plugin_check`) through the `member-check` plugin interface (`ohl_plugins::member_check`), one call per structure with all its load cases, by OHL-Structure, `ohl structures fe` and the line's structure check (`ohl_loads::capacity::usage_with_sections`); not installed: the run is refused by name (the line: the structure's check fails with the message); no runner: every member `check::PLUGIN_NOT_RUN`. No file format changed for the joints (the same `capacity` key). `BoltRules::code` and `clauses` are owned strings (a plugin's names). |
| `joint_details` (top level) | `StructureType::joint_details: Option<bool>` (version 11, 2026-11-11; written by `ohl_structure_edit::connection_edits::set_joint_details`, the file raised to 11 only when it is set): the calculation takes the joint details — member ends, bolts and `shift` end offsets from the `[[connection]]`s. None/false (default): the simple form, the connections not read by `Model::from_structure_type*` (`ohl_structures_fe::JointUse`, `Model::joint_use`). Honoured only at level 4 (`Detail::Joints`: `ends_without_joint_details()` empty); otherwise not applied and said (`joint_details_not_applied`). The level itself is derived, not stored. `Model::from_structure_type_with_joint_details` always reads them (joint panel, plate checks, exports). |
| `[[connection]]` | `TypeConnection` (`ohl_structures::connections`; version 9, 2026-11-02): `member`, `joint` (the member's end there), `kind?` (`ConnectionKind`: bolted default, welded, pinned, fixed, continuous — 2026-11-05: one part with `onto` through the joint, `StructureType::continuous_with(member, joint)` (given on either end, the other not saying otherwise) and `continuous_problem(member, onto, joint)` (in line within `CONTINUOUS_ANGLE_DEG` = 1°, the same `section`, `roll`, `reference`, `orientation`); otherwise said in `connection_notes`; `connection_edits::set_continuous` writes both ends), `onto?` (a member meeting at the joint, a plate's or a splice's id), `face?` (`Face`: outside default, inside, nested; 2026-11-05), `leg?` (`BoltedLeg`: both, long, short, none), `bolts?` (`ConnectionBolts`: `count`, `bolt?` (a bolt library's id), `diameter?` (one of the two needed), `hole?`, `grade?`, `shear_planes?`, `threads_in_shear?`), `gauge_lines?` (1–3, `MAX_GAUGE_LINES`; 3 since 2026-11-03), `gauges` (one per line), `pitch?`, `end_distance?`, `edge_distance?`, `rolled_edge?` (none: sheared; 2026-11-03), `bolt_points` ([along, across] from the joint and the heel), `other_leg_points` (2026-11-05: with `leg = "both"`, the other leg's bolts the same way, `bolt_points` then the leg on what it bolts onto's; without both: kept, said, not used; `holes` counts both legs' lines; `ohl_structure_link::Arrival::{other_points, other_flat, other_axis}`, `bolts_with_axes`; `connection_edits::set_bolt_points(.., other, ..)` keeps them), `other_onto?` (2026-11-08: with `leg = "both"`, the plate the other leg bolts onto when not `onto` — two plates at 90°, apart or welded; without both, or no plate there: said, not used; `Arrival::{other_onto, other_grip}`, `Arrival::other_leg` (that leg as an end of its own), `joints::bolted_onto` (a plate's member ends: its legs on it), the plate checks: each plate the member's whole force (on the safe side); cleared when its plate is removed), `eccentric?`, `restrained?`, `eccentricity?`, `cut?` (none: worked out to fit, `joints::fit_cut`) (where the steel stops from the work point: drawings only), `shift?` (signed: the end moved sideways to fit, across the member towards what it bolts onto — `connections::shift_direction`; since 2026-11-02, stage 4.4, **used by the analysis**: `EndDesign::shift` → `Member::offsets`, a rigid end offset, see below) | m | a member end saying more than its `[member.design]`; refused: an end given twice, a gauge count not the lines', more than 3 lines, a hole smaller than its bolt, a distance below 0; a member or joint gone: kept, said (`StructureType::connection_notes`), not used. Read by OHL-Structure (`ohl_structure_link::joint_details`), the IFC exports (bolts, `export_joints`), the CSV export (`connections.csv`) and, since 2026-11-02 (stages 4.1 and 4.2 of [joint details](joint-details.md)), the ASCE 10-97 check: `Model::from_structure_type_with_bolts` / `load_with_bolts` read each into `MemberDesign::ends` (`[Option<EndDesign>; 2]`, at the member's first and second joint: `kind`, `onto` (`Onto::{Member, Plate, Other}`), `leg`, `bolts` (`EndBolts`: the connection's values over its library bolt's `diameter`, `hole`, `shear_capacity`, `shear_capacity_shank`, `ultimate`, `grade` (`design::Grade`, 2026-11-03); `threads_in_shear`), `bolt`, `holes` (from the gauge lines or bolt points), distances, `eccentric`, `restrained`, `notes`; `ohl_structures_fe::member_ends::read_connections`); `member_ends::end_conditions` gives each end's eccentric and restrained with their `Source` (given, design data, worked out — only at an end with a connection —, assumed) and the combined `Ends`; `asce10::capacity` takes the connection per end, the weaker governing, `Capacity::joints` (`check::JOINTS`, `[joints: …]` in what governs) saying where each came from; OHL-Structure's table shows `end_conditions` (`ohl_structure_link::joints`). A member without connections is checked as before. The bolt library of a structure read by path is `Bolts/` beside it (`BoltLibrary::beside`, `bolts::BOLTS_FOLDER`); `ohl structures fe --bolts`, `RunInput::bolts` (none: beside `dir`). OHL-Structure's deletions take the connections of the members and joints deleted along (`ohl_structure_edit::remove`), a member split moves its far end's (`links::insert_joint`) |
| `[[splice]]` | `TypeSplice` (`ohl_structures::connections`; 2026-11-05, still version 9): `id` (unique among splices, plates and members), `joint`, `members`, `cover` (`[[splice.cover]]`, `TypeCover`: `face` (`CoverFace`: outer, inner), `leg?` (`CoverLeg`: long, short; none: a flat part, counted on either leg), `thickness`, `material?`, `outline`); `covers_on(leg)`, `shear_planes(leg)` (one per cover, at least 1); `StructureType::splices`, `splice(id, joint)`; refused: an id twice or a plate's or member's, a thickness ≤ 0, two covers on the same face of a leg; a joint or member gone, an end onto a splice not joining it: said. `ohl_structures_fe`: `Onto::Splice { id, planes, covers }` (by the end's leg; both legs: the leg with fewer planes), `EndDesign::face`, `EndDesign::planes` (`member_ends::worked_planes`: a splice's covers; one lapped onto a member or plate); `asce10::end_bolts` takes the bolts' given `shear_planes`, else a splice's, else the member's design data, else the worked-out lap's, else 1 (a given value differing from the worked-out one said). `EndDesign::continuous` (the member it runs on into; `member_ends::link_continuous` after reading the connections — `member_ends::continuous_problem` on the model, the partner's end made when missing; not one part: the kind dropped, said): no connection checked at that end (`end_bolts` none, gross section), concentric worked out, the restraint not. `asce10::onto_leg` (`OntoLeg`: the leg of the member an end with a connection bolts onto): its bearing too, the lower governing (ASCE 10-97 4.4, SIA 263 (76) with the member's own e_1) — [theory](../theory/joint-laps-splices.md). `connection_edits::add_splice` (covers outer and inner on each leg, the ends' `onto` set), `set_cover`, `remove_splice`; `edits::remove` takes a joint's splices and a member out of the splices, and a continuous end with a member gone bolted again; `links::insert_joint` renames the far end in the splices; `ExportStructure::splices`, `splices.csv` (one row per cover: `id`, `joint`, `members` `;`-separated, `cover_face`, `cover_leg`, `thickness_m`, `material`, `outline_m`) and a `face` column last in `connections.csv`; `ExportStructure::parts` (`part_chains`, filled by `export_joints`) as `parts.csv` (`part` — its first member —, `members`, `joints`, `length_m`, `steel_length_m`) and IFC `OHL_Member.Part` / `PartMembers`; `ExportPlate::cover` (`(splice id, "outer, long leg")`): a splice's covers in `joint_plates` (id `S1-outer-long`), in IFC an `IfcPlate` "splice cover" with `OHL_Splice` (`Splice`, `Cover` and the plate's values); the plate checks plan each cover (`plate_checks::cover_plans`: `Plan::Bolted::share` the cover's share by area, `Plan::Unplaced` a member's bolts not placed in a cover; `PlateUsage::plate` = "S1 (outer, long leg)") | m | built, in active development, not approved yet ([proposal](joint-details.md#lap-joints-double-covers-continuous-members-proposal-2026-11-05)): the geometry, the plate-and-leg checks and the app follow (stages 2–5) |
| `[[plate]]` | `TypePlate` (version 9): `id` (unique, not a member's), `joint`, `thickness`, `material?`, `outline` (local axes; empty: drawn around the bolts onto it), `welded_to` (2026-11-04, still version 9 — "anything added before the next release": the plates of the same joint it is welded to; read both ways and through others, `StructureType::welded_assemblies(joint) -> Vec<Vec<id>>`, a **welded plate** = one assembly, several plates welded together so that every member bolting onto the joint reaches one; welded to itself refused, to a plate not of its joint said in `connection_notes`; `connection_edits::weld_pair`, `set_welded_to`, `make_welded_plates` (the joint's plates replaced by one per group of members, each welded to the ones before), `remove_plate` drops it from the others' lists, `copy_ends` maps it among the plates copied), `weld` (`[[plate.weld]]`, `TypePlate::welds: Vec<TypeWeld>`; 2026-11-04, still version 9 — "anything added before the next release": `to` (welds the two as `welded_to` does; `TypePlate::welded_ids` = `welded_to` ∪ the welds' `to`, used by `welded_assemblies` and `connection_notes`), `size?` (the fillet's throat a, SIA 263 6.3.2.6), `length?` (none: the seam's, `welds::Weld::seam_length`, one side), `kind?` (`WeldKind`: `fillet` default, `full_penetration`); `StructureType::weld_between(a, b)`; given on both plates, twice, or a size/length ≤ 0 refused; `connection_edits::set_weld(doc, t, a, b, WeldEdit { size, length, kind })` writes it on the plate already holding it (else the one whose `welded_to` named the other, the name moved into the weld), `weld_pair(.., false)`, `remove_plate` and `copy_ends` keep the welds in step), `kind?` (`PlateKind`: `bolted` default, `welded`; 2026-11-03: welded onto the joint's leg; **read only since 2026-11-04**: the joint's plates of kind `welded` are one welded assembly — one alone a plain plate —, said in `connection_notes`; dropped by the first welding edit at that joint, `add_plate` no longer takes a kind) | m | gusset plates, any number per joint; `plates.csv` in the CSV export (`kind` and `welded_to` (`;`-separated) columns last, as the file gives them), `IfcPlate` in IFC (a plate of a welded plate: `OHL_Plate.WeldedTo`, the others of its assembly; `ExportPlate::assembly`; its welds: `OHL_Plate.Welds`, `ExportPlate::welds`); `plates.csv` also `weld_to`, `weld_kind`, `weld_size_m`, `weld_length_m` (`;`-separated, one per `[[plate.weld]]`). The member checks treat all plates alike (members bolt onto any; `member_ends` reads plates for `Onto::Plate` only). **Plate checks** (2026-11-04): `ohl_structures_fe::plates` (`BoltGroup`, `PlateSteel`, `bolted`, `WeldInput`, `weld`, `Checked`, `PlateCheck`; ASCE 10-97, SIA 263, simple: none), planned by `ohl_structure_link::plate_plans(text, sections, bolts) -> PlatePlans` and checked per case by `plate_usage(plans, axial, strength) -> Vec<PlateUsage>` (`joint`, `plate`, `part: PlatePart::{Member, Weld}`, `force`, `usage?`, `governs`, `checks`, `notes`) into `CaseResult::plates` (`governing_plate`, `joint_plate_usage`; `analyse_structure`, a structure of its own members only), `StructureRun::checks` a `plates` summary ([theory](../theory/joint-plates.md)). `ohl_structure_link::PlateDetail` (OHL-Structure's joint details) carries `through` (2026-11-04: the outline's normal turned away from the members bolted onto it, the close-up draws the plate as a solid of its thickness that way; none: no outline — a plate with no member onto it and no `outline` given has none, `NO_OUTLINE` said in `notes`, and no grip takes it), `welded_to` (direct partners, both ways and the old kind), `assembly` (its welded plate, itself among them), `welds` (`welds::Weld { with, ends, sides, kind, size, length }` — the file's weld data when given, `label()` "a6 × 220" drawn in the close-up and face view: the seam where it meets a plate of its assembly listed after it, `welds::seam` — the stretch of the planes' common line both outlines reach within the thicker plate's thickness + 5 mm; two plates welded directly that do not meet are said in `notes`), `leg` (the joint's leg it lies on), `edges` (the distances its edge keeps from each member end's bolts, in words) and `notes` (bolts nearer its edge than that, an outline given in the file); `ohl_structure_link::PlateFrame::of(joint, plate)` (2026-11-04) its own axes for corrections by hand (`n` the outline's Newell normal, largest component positive; `u` along its `leg`, else its first member, laid into the plane; `v = n × u`; `origin` the work point laid onto the plane; `work_point` its distance off the plane; `to_uv`/`to_3d`/`corners`/`outline`, `meet` a view's ray with the plane), `offset_outline`, `insert_corner`; `connection_edits::set_plate_outline` / `clear_plate_outline` write or take out `outline` (format unchanged); an outline worked out encloses every bolt onto it with an ellipse of the member end's minimum end distance along the member and edge distance across it (`ohl_structures_fe::bolt_rules::rules`, force none: the bolt's shear capacity; the member's leg thickness and steel — the plate's own not yet), the capacity method without rules: the end's own end and edge distances, else 1.5 d assumed |
| `.ohlb` (`Bolts/` beside a structure; project library kind `bolts`; `libraries/bolts/example-bolts.ohlb`) | `ohl_structures::bolts::{BoltLibrary, BoltType}`: `format = "ohl-bolts"`, `version = 1`, `[[bolt]]` `id`, `name?`, `description?`, `diameter`, `hole?`, `grade?`, `ultimate?`, `yield?`, `shear_capacity?` (per plane, threads in), `shear_capacity_shank?`, `tension_capacity?`, `head_across_flats?`, `head_height?`, `nut_height?`, `grip?`, `mass_kg?`; `load_all` (several files, the first of a twice-given id kept, said) | m, Pa, N, kg | bolt libraries (2026-11-02); a connection names a bolt (`bolts.bolt`); `ohl_structure_link::joint_details` takes them (`BoltSet::bolt`, `drawn`), the ASCE 10-97 check their `shear_capacity`, `shear_capacity_shank` and `ultimate` (`EndBolts`, 2026-11-02); OHL-Structure's `LibKind::Bolts` (import, export, what no library has); the IFC exports through it (`--bolts`, File › Export) |
| `.ohlcp` (`Connections/connection-presets.ohlcp` next to a structure) | `format = "ohl-connection-presets"`, `version = 1`, `[[preset]]` with `name` and the fields of a `[[connection]]` but `member`, `joint`, `onto`, `bolt_points` (`ohl_structure_edit::connection_edits::{parse_presets, presets_text, CARRIED}`; each preset read as a connection to check it) | m | OHL-Structure's connection presets (stage 3, 2026-11-02): an end saved as one, one applied to an end (`put_carried`) |
| `[[generator]]` | tables (`generators`, kept unread by the core): `id`, `kind` (`panels` / `crossarm` / `peak` / `copy_face`) and its inputs | m | a body generator's record, read by `ohl-structure-edit` (`body::Generator`): panels `bottom?`, `feet`, `waist` (a length or [across, along]), `panel` [{`top`, `bracing` (`single`/`x`/`k`/`diamond`/`warren`), `horizontal`, `width?` (not the last panel; `Panel::width`), `redundants?` (`none`/`one`/`two`, `body::Redundants`), `plan?` (`none`/`x`/`diamond`, `body::Plan`)}], `plan_auto?` (`body::Plan`; slope changes and crossarm levels); crossarm `points`, `shape` (`triangular`/`rectangular`), `one_arm?` (bool); the optional fields are written only when on and read as off when absent (older files read as before, built the same: `generators_tests.rs` keeps a reference); version 6: the structure application raises the file to 6 when it writes a generator using one (`Generator::uses_options`), so an older one says "newer" instead of refusing them as unknown fields; peak `points` or `height`; copy_face `body` (a panels generator), `face` (`x+`/`x-`/`y+`/`y-`) |
| `[[symmetry]]` | tables (`symmetry`, kept unread by the core): `mirror` (`x`/`y`/`xy`), `groups?` | — | mirror planes the structure application keeps joints and members in step with (`body::Symmetry`); the first one is the Body menu's Symmetry option |
| `[[section]]`, `[[material]]`, `[analysis]`, `[[load]]` | tables | — | read by `ohl_structures_fe` for the finite-element check; `[[load]]` is also the structure application's case `model` (a member's section: the cross-section library's first; `[analysis]`: `nonlinear`, `self_weight`, `drag` — the member wind's drag factor, default 1, `Analysis::drag_given` false then: said not on the safe side —, `stabilise` — joint stabilisation, default true —, `dead_load_factor` — on the members' self-weight, default 1) |
| `[[load_case]]` | tables (`load_cases`, kept unread by the core) | N, Pa | the structure's load-case recipes, read by the structure application (`ohl-structure-loads`; below) |
| `capacity` | `spans[]` (by line angle: `wind_span`, `weight_span`, `min_weight_span`; between two angles interpolated linearly in sin(θ/2), `Capacity::spans_at` — linearly in θ until 2026-10), `attachment[]` (`point`, `vertical`, `uplift`, `transverse`, `longitudinal`, `interaction`) | m, N | what it can carry (structure axes) |
| `wind` | `band[]`: `from`, `to`, `area`, `drag` | m, m² | wind on the structure itself; heights above the ground of the structure as written, raised by a placed structure's extension `dz` |
| `model` | path? | — | separate `.ohlfe` model, relative to this file |
| `pole` | `PoleRef`: a string (`File`: a `.ohlp` id, or a file name ending in `.ohlp`, in the same folder) **or** a `[pole]` table (`Inline(InlinePole)`, version 3) | m, Pa, kg/m³ | the pole: `shape` (`round`/`polygon`), `sides?`, `embedment?`, `[[pole.outline]]` (`z`, `diameter` **or** `circumference`; rising z; a polygon across flats), level 3 `material` (`wood`/`steel`/`concrete`/`other`), `section?` (cross-section library), `wall?` (`PoleWall`: a length, or `"solid"` — no hollow; another word refused naming `wall`; absent: not given, level 3 lacks it; a `.ohlp`'s `wall` reads the same, absent there still solid in the check; typed wins over a section's `t`), `fibre_stress?`, `yield?` and `bend_radius?` (2026-11-03: a hollow steel shaft with its yield and no capacity given checked by ASCE/SEI 48-19, `ohl_structures_fe::asce48`; a section's material gives the yield), `code?` (2026-11-03, also in a `.ohlp`: `asce48-19` default or `sia263-2013`, `ohl_structures_fe::pole::PoleCode`, `sia263::shaft_usage`), `strength_class?` and `moisture_class?` (2026-11-03, also in a `.ohlp`: a solid wood pole with its class C16/C24/C30/D30 and no capacity given checked by SIA 265, `ohl_structures_fe::sia265`; moisture 1–3, absent 3), `moment_capacity?`, `[[pole.capacity]]` (`height`, `moment`), `elasticity?`, `density?`, `drag?` — the `.ohlp` fields; read into one `ohl_structures_fe::Pole` by `pole::structure_pole` (`Pole::from_inline` for a table; `Pole::profile`, `Pole::sides`) |

Joint details, 2026-11-04 (OHL-Structure, `ohl_structure_link`, nothing
stored): `Arrival::flat` (`welds::Flat { off, turn }`) — how far the leg
bolted lies off the face it bolts onto (a plate's seat normal, or the
flange of the member it bolts onto whose face runs most nearly along it)
and the turn of the member's roll that lays it flat (`welds::flat_roll`:
the smallest, within ±90°); more than `FLAT_OFF` (5°) is said in the
arrival's notes ("bolted face 90° off … — turn it"). `Arrival::gauge_lines`
without a connection's `gauge_lines` or bolt points: from the member's
design `bolts.holes` (one leg bolted: a line per hole; `connected_leg`
both: ceil(holes / 2) per leg; clamped 1–3; `Source::WorkedOut`, said in
the notes; none: 1, assumed) — the ASCE 10-97 block shear takes the holes
as the lines (one leg bolted only) as before. `welds::welded_plan(joint) ->
(Vec<PlannedPlate { members, normal }>, notes)`: the plates a welded
plate needs, one per plane of the members arriving with the joint's leg
(`welds::joint_legs`: class leg, else within 30° of vertical; within 10°;
modelling members, members held otherwise than bolted and splices left
out). OHL-Structure's close-up (`close_up::CloseUpCache` in
`JointPanel::close_up`): the pieces cut once per (file revision,
libraries stamp, joint, theme), finely within `NEAR` (20 mm) of a bolt and
growing by `GROWTH` (0.5) of the distance beyond, at most `MAX_PIECES`
(40 000); sorted again and the mesh rebuilt only when the turn, tilt, zoom
or pane changes.

Member solids, 2026-11-05 (OHL-Structure, `ohl_structure_link::member_solids`,
nothing stored; built, in active development, not approved yet): one
builder for the Detail view's close-up and the 3D view —
`section_at(arrival, profile, along)` (the profile's outline turned by
its axes, moved by `Arrival::offset`, `along` m from the joint),
`steel_start` (the cut), `prism_faces(near, far, outline, ey, ez) ->
Vec<SolidFace { triangles, normal }>` (sides and the two ends, normals
out), `ear_clip`; `member_solids(&JointDetails) -> Vec<MemberSolid {
member, section, shaped, joints, work, cut, ends, faces }>` runs each
member's section from the cut at its first joint to the cut at its
second, each end's corners exactly the close-up's (two ends seated
differently: the section moves linearly between them). `Profile::shaped`
(false: a section of values only, a square of its area). The 3D view
(`solids::Solids` in `StructureApp::solids`) keeps them per (file
revision, libraries stamp): up to `AT_ONCE` (400) members at once, more
on a thread of their own; a solid whose work points no longer match its
member's (within 1 mm) is drawn as a stick.

Lap joints, covers, continuous members, 2026-11-05 (OHL-Structure,
`ohl_structure_link`, nothing stored; stages 2 and 4 of the proposal in
joint-details.md; built, in active development, not approved yet):
`Arrival::sits: Valued<Face>` (given, else worked out from the seat;
none onto a plate or splice), `Arrival::continuous: Option<member>`
(`StructureType::continuous_with`; no cut, shift, bolts or layout there),
`Arrival::misfit` (drawn red, said in `notes`), `Arrival::planes:
(Valued<u32>, Option<why>)` (the bolts' `shear_planes` given, else
`EndDesign::planes` worked out, else the member's), `Angle::r` (root
radius). `NESTED_ANGLE_DEG` (5°). `JointDetail::splices: Vec<SpliceDetail
{ id, members, bolted, covers: Vec<CoverDetail { face, leg, thickness,
material, outline, outline_given, through, edges, notes }>, notes }>`
(`CoverDetail::words`); `bolted_cover_leg(arrival)`. `part_chains(&JointDetails)
-> Vec<PartChain { members, joints, length, steel }>` (two members or
more joined by continuous ends; `steel` less the free ends' cuts).
`MemberSolid::{runs_on, misfit}` (no end face where it runs on),
`prism_faces_capped`, `plate_solids(&JointDetails) -> Vec<PlateSolid {
id, joint, cover, thickness, ends, faces, misfit }>` (gusset plates and
covers, drawn in the 3D view); `bolt_solids(&JointDetails) ->
Vec<BoltSolid { joint, member, at, assumed, diameter, faces }>`
(2026-11-05: every bolt of both legs, the 3D view's own GPU scene built
once per solids generation, drawn near once a bolt is 3 pixels wide). `steel_start` may be below 0 (at most half
the length): a member nested in runs on past the work point. The grip
onto a splice: [leg + inner covers on that leg, outer covers]. App:
`joint_tools::Act::{Continuous, AddSplice, Cover, RemoveSplice}`,
`continuous_pairs(t)`, `JointTools::continuous` (the Continuous members
window).

**Detail level** (`StructureType::detail`, never stored — an older
`detail` key is read and ignored): 3 *full design* when `model` is given
or `StructureType::missing_for_design` finds nothing: there are members,
every member has a section and a joint has a `support` (every extension
piece too: its members with sections and a joint with a `support`;
`StructureType::members_designed`). A joint with only a `spring` does not
count, so a structure standing only on springs stays at level 2. 2 *outline* when
there are members or body polylines that are not all in one plane
(within 1 mm, `StructureType::body_is_3d`); else 1 *stick figure*. The
pole gives its own level (`StructureType::pole_detail`): a named `.ohlp`
or a `[pole]` with material, strength and its wall (a thickness, `wall =
"solid"`, or a section) 3, a `[pole]`
outline with diameters at two heights 2 (`InlinePole::is_3d`,
`InlinePole::missing_for_design`); the structure's level is the higher.
Checks that depend on the structure's shape must say what they do at
each level ([Structure detail levels](structure-detail.md)). The
finite-element model of the structure's own members is built when its
members are designed (`members_designed`), not when a pole makes the
level 3; the pole check runs on a level 3 pole. The meaning of the
levels decided for the structure application (capacities derived only at
level 3) is in [Building structures](structure-building.md).

**Drawn bodies** (display only, never stored; `ohl_structures_fe::drawn`):
`Bodies::load(types, sections)` makes, per type at level 2 or 3 and per
extension, a `DrawnBody` — its members as lines in the structure's axes
as placed (`StructureType::stick_parts`: `StructureType::stick` with each
line's member id, the lines `Line::resolve` writes into a structure's
`members`; a type naming a separate `model` takes that model's members,
raised by `StructureType::lift`) and, for a member whose section has a
library profile (`ohl_structures_fe::Section::profile`), a solid: the
profile's outline (`drawn::outline`, fillets left out, round shapes 16
sides, tubes closed) about its centroid, swept along the member in the
finite-element element axes (`reference`, `roll`). `DrawnBody::placed_lines`
/ `placed_triangles` move it to a structure (`to_global` at the position
raised by `height_adjust`, turned by `orientation_deg`). A level 1 type
has none. The desktop app keeps one `Bodies` per model (`Model::bodies`,
made with the libraries) for the 3D view, picking and the drawings; `ohl
drawing` makes its own. In the drawings a structure's members are
`ohl_drawing::Mark::members` (station, offset, z) and `members_xy`,
filled by `RouteDrawing::set_members` and drawn on the layer
`OHL_MEMBERS` (`ohl_drawing::model::Layer::Members`) instead of the
symbol.

A pole at level 2 or 3 adds `DrawnBody::pole` (a `DrawnPole`; a body
with a pole and no members has `members` empty, `has_members()` false):
`drawn::PoleOutline::of` reads the outline — a `[pole]`'s `rings()`
(local z, diameter; `polygon_sides`; the wall typed, or a hollow
`section`'s, else solid) or a `.ohlp` file's taper or `profile` from the
feet up (`ohl_structures_fe::pole::structure_pole`) — and
`DrawnPole::new` raises it by `StructureType::lift` per extension (cut at
the ground line, the lowest diameter held down to it): `rings` (z,
outer diameter) from z = 0, `sides`, `wall`, and the `solid` triangles
(`placed_triangles` includes them). `DrawnBody::placed_pole_rings`
gives the outer outline at each height, closed, as placed. In the
drawings they are `Mark::pole` (station, offset, z) and `pole_xy`,
filled by `RouteDrawing::set_poles`, on `OHL_MEMBERS`; `Mark::has_body`
replaces the symbol for either.

In the desktop app a `scene::Prim` with `near` set (the solids of
members and poles) is built per chunk apart from the levels
(`ChunkIndex::near_keys` / `build_near`) and drawn within
`LodSettings::solids` (its own rule, `LodSettings::solids_rule`; default
`lod::DEFAULT_SOLIDS_DISTANCE`, 400 m), saved per user under the storage
key `lod-solids` (a number of metres; settings saved before it read the
default). The table's rows are saved under `lod` (JSON, a `LodRow` per
layer key: `render_distance`, `from`, `detail`); a row equal to a former
default of its layer (`LodRow::former_default`; the Drawings row before
2026-10-22) reads as the current default. `selection::pole_at` picks a structure by its pole's outline
on the screen.

**Drawing options** (`ohl_drawing::Options`; the app stores them in `DrawingSettings`, below):
`elevations: Elevations { place: ElevationPlace (Off | Above | Below),
offset_mm, rotation_deg }`, default off, 10 mm, 90°: each structure's
elevation on the plan on the layer `OHL_ELEVATIONS`
(`model::Layer::Elevations`); `ohl_drawing::warnings` (and the export's
warnings, per sheet) report elevations that overlap each other or reach
into the profile. `ohl drawing --elevations --elevation-offset
--elevation-rotation`; the app's Profile sheets window
(`DrawingsWindow::elevations`, stored with the drawing settings); `ohl_drawing::elevations` gives them in the route's (station, offset) for the sheet preview, of the structures `RouteDrawing::shown` keeps.
Since 2026-10-20 `Elevations` also has `scale: Option<f64>` (their own
scale denominator), `labels: ElevationLabels { number, type_name,
height, string_length }` and `text_size` (mm); `ElevationDrawing` has
`labels: Vec<([station, offset], text, centred)>` and `role` (another
version's). The plan panel grows to hold them (no more warning about
reaching into the profile; one leaving the plan panel is warned).

**Sheet templates** (2026-10-20; `ohl_drawing::template::SheetTemplate`,
file `.ohlsheet`, format `ohl-sheet-template` version 1, TOML read with
`ohl_core::files::check_header`, unknown keys refused): `page`
(`PageSpec { format, orientation, width, height, margin }`), `frame`,
`text`, `layout` (`order`, `gap`, `plan_height: Size`, `beyond_match:
Beyond`, `axis_room`, `station_labels`), `layers: BTreeMap<layer name,
PenSpec { width, colour, dash }>`, `bands: Vec<BandSpec { kind: BandKind,
height, label, every, field, decimals }>`, `north_arrow`, `scale_bar`,
`legend`, `key_map`, `corridor`, `elevations: ElevationSpec`,
`line_styles: LineStyles { by_kind, low_voltage_up_to_kv, kind_field,
construction_field, cable_dash, kinds }`, `roles: BTreeMap<role,
RoleSpec>`, `conductor_clearance`, `title_block: TitleBlockSpec {
corner, width, columns: Vec<Column (Fixed | Rest | Logos)>, rows, line,
grid, cells: Vec<CellSpec { at, span, text, size, bold, align, image }>
}`, `revisions`, `notes`, `fields: Vec<FieldSpec { key, label, default,
required, per_sheet }>`, `images: Vec<ImageSpec { name, file | data,
media_type }>`; read: `resolved` (images as data URIs) and `source` (the
text; `embedded()` the text with every image as `data`). Lengths are
`Mm` ("10 mm", "1 cm") and `Metres` ("100 m", "1 km"). Placeholders:
`template::COMPUTED_FIELDS`, `MORE_FIELDS`, `REVISION_FIELDS`; a name
neither computed nor declared refuses the template. The built-in one is
`template::BUILTIN` (= `libraries/sheet-templates/default.ohlsheet`,
checked by a test). Every key: the user guide's [Sheet
templates](../user-guide/sheet-templates.md) and
`examples/sheet-templates/example-a1.ohlsheet`. Since 2026-10-22 `[page] format` takes any standard paper size or `"optimised"` with `sizes` (names) and `mixed` (bool) (`SheetTemplate::optimise`); still version 1 (older files read the same). Since 2026-10-22 `[elevations]` takes `stick` (bool) and the labels `dimensions`, `arm_heights`, and `[legend.labels]` the words `elevation_dimension`, `elevation_arm`, `elevation_string`. Since 2026-10-23 `[cases]` (`conductors`, `earth_wires`, `stringing`: a case's name or a temperature, `CasesSpec::pick`), the band kind `stringing` (`RouteDrawing::stringing`, `Stringing { from, to, from_structure, to_structure, tensions: [(system or none, lowest N, highest N)], phases: [(system or none, phase label, lowest N, highest N)] (since 2026-10-24), case }`, set by `RouteDrawing::set_stringing`; the end structures since 2026-10-23) and the words `stringing`, `earth_wires`. Since 2026-10-23 also `[stringing_table]` (`StringingTableSpec`: `place` `"bottom"` / `"above_title_block"` / `"off"`, `rows`, `row_height`, `size`, `columns` with the fields `st.from`, `st.to`, `st.circuit`, `st.phase` (since 2026-10-24), `st.tension`, `st.case`), drawn on the layer `OHL_STRINGING`. Since 2026-10-24 the band kind `sag` and `[sag_labels]` (`SagLabelsSpec`: `show`, `circuits`, `label` with `{sag}`, `{circuit}`, `size`, `gap`); `RouteDrawing::sags: Vec<SpanSag { from, to, sags: [(system or none, sag m, lowest mid-span point (station, z))] }>`, set by `RouteDrawing::set_sags` from the shapes drawn. Since 2026-10-25 `[[note]] from` (`NoteSpec::from`, `NoteFrom`: `"frame"` (default) or `"drawing"`: `at`'s y down from the drawing's lowest row, `layout::Paper::drawing_bottom`)). Since 2026-10-28 `[roles.<role>] removed` (`RoleSpec::removed`, a role's name: what the project does not keep of a version in this role is moved to a version of the same name in that role by `RouteDrawing::split_removed(options, own cases)`, the app's and the CLI's sheets call it after adding the versions; kept within `ohl_drawing::SAME_WITHIN` = 1 m) and the word `ground_cut` (`{height}`); still version 1. Since 2026-11-10 `[layout] order = ["profile", "plan", "bands"]` puts the plan between the profile and the band rows (`LayoutSpec::plan_between`; the bands then start at `Frame::bands_top`, under the plan) and the elevations asked `"above"` in a strip of their own above the profile (`Frame::elev_axis`, `layout::elevation_room`); the other two orders as before; still version 1.

Since 2026-11-10 (owner: where the profile's two side lines run; still
version 1, every key optional, a template without them reads and draws
as before) `[corridor]` takes `profile_lines` (`"corridor"` default,
`"distance"`, `"lowest_arm"`; `CorridorSpec::profile_line_choice`
checks it, an unknown word or `distance` without a positive
`profile_distance` refuses the template), `profile_distance: Metres` and
`profile_circuits: Vec<String>` (circuit names); `[legend.labels]` the
words `profile_distance` (`{side}`, `{distance}`) and
`profile_lowest_arm` (`{side}`). `ohl_drawing::side_lines`:
`ProfileLines { Corridor, Distance(m), LowestArm }` (`parse`, `key`),
`SideLine { right, plan: runs of (station, offset), ground: Profile }`,
`circuits(line) -> [(name, counted by default)]`, `still_case(wind
flags, drawn)`, `LEVEL_TOLERANCE` 0.5 m. `Options` gained
`profile_lines: Option<ProfileLines>` and `profile_circuits:
Option<Vec<String>>` (the export's over the template's:
`Options::profile_line_mode`, `Options::profile_circuits`).
`RouteDrawing` gained `profile_lines: Vec<SideLine>` (near side first;
empty in the `Corridor` mode, where `corridor_ground` is filled as
before; `Distance`: made by `build`, `corridor_ground` then empty;
`LowestArm`: `RouteDrawing::set_lowest_arm_lines(line, route, still
shapes or none, terrain, options)`, called by the app's and the CLI's
sheets after `build`) and `profile_line_notes: Vec<String>` (added to
the first sheet's warnings). Drawn on `OHL_GROUND_SIDE` in the profile
and in the plan in the corridor's `near` / `far` pens. `ohl drawing`
gained `--profile-lines`, `--profile-distance`, `--profile-circuits`;
the app's `SheetChoice` `profile_lines`, `profile_distance`,
`profile_circuits`, and `drawings::Input` `still:
Option<Vec<SectionShape>>` (the still-air case's sections, copied only
in the `LowestArm` mode).

`ohl_drawing::Options` gained `template: Arc<SheetTemplate>`, `fields:
SheetFields { values, per_sheet: key → sheet → value, revisions:
Vec<Revision { index, date, text, by, checked }> }`, `info: SheetInfo {
project, line, crs, date, cases, height_datum }`, `role: Option<String>`
(the line's), `corridor: Option<f64>` (half width, m),
`conductor_clearance: Vec<ConductorClearance { circuit, voltage_kv,
distance, rule, cases }>` (`curves::conductor_clearances(line, rules)`:
per circuit the largest of the ground rules' vertical distances at its
voltage, the rule's weather cases), `bands: bool`; `Options::with_template`
takes the template's elevations and corridor where the options leave
theirs at the defaults. `export::write_route` refuses sheets while a
required field is empty (`SheetTemplate::missing_fields`).

`RouteDrawing` gained `versions: Vec<VersionDrawing { name, role, label,
structures: Vec<Mark>, wires: Vec<Trace> }>` (`RouteDrawing::add_version`:
another line's structures and wires within the line width, projected on
this route's centreline, per weather case) and `clearances: Vec<(station,
m)>` (the band row `ground_clearance`; none: empty). `Trace` gained
`case`, `circuit`, `phase`, `voltage_kv` (its system's; none: earthed)
and `fields` (its section's plugin columns; the template may read the
kind and the construction there); `Mark` gained `type_name` and
`strings: Vec<StringMark { label, top, bottom, length, suspension }>`.
`style::wire_pen` gives a wire's pen and legend words from kind, role and
case; `curves::curves` the lines below the lowest conductor
([theory](../theory/drawings.md)).

Sheet layers added (DXF names): `OHL_TITLE_FIELDS`, `OHL_TITLE_IMAGE`
(SVG logos as outlines), `OHL_REVISIONS`, `OHL_BANDS`, `OHL_LEGEND`,
`OHL_KEY_MAP`, `OHL_CORRIDOR`, `OHL_VERSIONS`,
`OHL_CONDUCTOR_CLEARANCE`. DXF texts use R12's own codes (`%%nnn`,
`%%d`, `%%p`, `%%c`, `%%%`) instead of `\U+XXXX`; `dxf::write_counted`
also says how many raster images were left out.

Project templates: `library` objects of kind `sheet-templates` (name
`NAME.ohlsheet`, text with the images embedded), per project line; the
app's choice key: `default`, `project:NAME`, `library:NAME`,
`plugin:PLUGIN/NAME`, `file:PATH`.

Template keys added the same day (still version 1; every one optional,
the defaults keep the look): `[layout] plan_title`, `profile_title`;
`[[band]] format` (structures rows: `template::STRUCTURE_BAND_FIELDS`
`name`, `type`, `type_id`, `extension`, `station`, `height`);
`[legend] labels: BTreeMap<entry kind, text>` (`template::LEGEND_LABELS`:
kind, English default, its placeholders; `LegendSpec::label`);
`[corridor] near_side` (`right` default, `left`), `near`, `far`
(`PenSpec`, over `pen`; `CorridorSpec::edge_pen`), `profile: bool`;
`[centreline] { pen, ground }` (`CentrelineSpec`); `[lidar] { profile,
plan, classes: Vec<u8>, size, cell, styles: BTreeMap<"2" | "3-5" |
"3,4", LidarStyle { colour, size, label }> }` (`LidarSpec`,
`template::class_codes`); `[conductor_clearance] label_at` (`auto`,
`middle`, `start`, `end`), since 2026-10-24 `envelope: bool` and
`envelope_label` (one line for the whole line, `curves::envelope`); `[if_empty]: BTreeMap<field | "*", text>`
(`SheetTemplate::if_empty`; `template::MAY_BE_EMPTY` the fields left
empty unless named). The title block, revisions and legend now lie
inside the frame's padding (`Paper::new`), texts made smaller to fit
their cells (`furniture::fit_text`, `fit_line`). `Options` gained
`lidar_classes: Option<Vec<u8>>` (the export's choice over the
template's) and `class_names: BTreeMap<u8, (name, Option<rgb>)>` (the
class scheme's); `RouteDrawing` gained `corridor_ground: Vec<(offset,
Profile)>` (near edge first, built when a corridor and the ground are
given) and `lidar: Vec<([station, offset, z], class)>`
(`RouteDrawing::set_lidar`, by station); `Mark` gained `type_id` and
`extension`; `curves::Curve` gained `parts: CurveParts` (the legend in
the template's words: `Curve::legend_with`); `style::Sample::Dots`;
`height_datum_of(crs)`. New layer `OHL_LIDAR`. The app's
`SheetChoice` gained `lidar_classes: Option<String>`; `ohl drawing`
`--sheet-lidar-classes`.

**The key map's backgrounds** (2026-10-23, display only): `[key_map]`
(`template::KeyMapSpec`) gained `box: PenSpec` (field `sheet_box`),
`drawings: bool`, `orthophoto: bool`, `veil: f64` (0–1, default 0.4);
`Options` gained `key_map_drawings`, `key_map_photo: Option<bool>`
(`Options::key_map_drawings_on`, `key_map_photo_on`);
`RouteDrawing::key_background: Arc<Vec<background::KeyMapDrawing { name,
lines: [(xy points, colour)] }>>`, filled by
`RouteDrawing::add_key_map_drawing` (the square 1.6 times the route's box,
points thinned to 1/3000 of it); `SheetChoice` (the drawing settings)
gained `key_map_drawings`, `key_map_photo: Option<bool>`. The plan's
backgrounds are cut at the corridor's edges when a corridor is drawn.

**The plan's backgrounds** (2026-10-23; display only, still template
version 1, every key optional and off by default):
`[plan_background]` (`template::PlanBackgroundSpec`): `drawings:
Choice`, `texts: bool`, `points: bool`, `own_colours: bool`, `pen:
PenSpec` (default 0.13 mm `#8c8c8c`), `orthophoto: Choice`, `veil: f64`
(0–1, default 0.6), `dpi: f64` (50–600, default 200), `quality: u8`
(10–100, default 75); `template::Choice` = `Off` (`false`) | `All`
(`true`) | `Named(Vec<String>)` (a list; drawing names, image layers).
`LEGEND_LABELS` gained `background_drawing` (`{name}`) and `orthophoto`
(`{name}`). `Options` gained `background_drawings: Option<bool>`,
`background_photo: Option<bool>` (the export's switch over the
template's choice: `Options::plan_drawings`, `plan_photos`;
`Choice::switched`), `background_dpi: Option<f64>` (the sheet preview's
60 dpi), and `Options::background_half_width` (`BACKGROUND_REACH` = 3 ×
the line width or the corridor). `RouteDrawing` gained `background:
Arc<Vec<background::PlanDrawing { name, lines: Vec<(Vec<[station,
offset]>, Color)>, points, texts: Vec<PlanText> }>>`
(`RouteDrawing::add_background_drawing(name, &ohl_background::Drawing,
half_width)`: pieces of at most `background::DENSIFY_M` = 2 m
straightened, cut at the width, layers off left out) and `photo:
Option<background::PlanPhoto { name, source: Arc<dyn PhotoSource> }>`
(`RouteDrawing::set_photo`; `PhotoSource::sample(points, metres per
pixel, out)`, implemented by `ohl_background::Mosaic`: the project's
images of chosen layers, tiles read on demand at the pyramid level of
the pixel, bilinear, 512 tiles cached). `model::Image` gained `pixels:
Option<[u32; 2]>` (a photo stretched to its box: SVG
`preserveAspectRatio="none"`, PDF the JPEG as a `/DCTDecode` image, DXF
left out with a `999` comment); new layers `OHL_BACKGROUND_DRAWINGS`,
`OHL_BACKGROUND_PHOTO` (`Layer::ALL` 29). The app's `SheetChoice` gained
`background_drawings`, `background_photo: Option<bool>` (kept in
`DrawingSettings`); `drawings::Input` the drawings and the project file
and layers of the photos (`Input::drawing_with_backgrounds`). `ohl
drawing` / `ohl line`: `--plan-drawings [NAMES]`, `--plan-orthophoto
[LAYERS]`, `--no-plan-backgrounds`, `--plan-background-dpi`,
`--backgrounds FILE.ohl`. `ohl-drawing` now depends on `ohl-background`
and `image` (JPEG).

**Editing a structure file** (`apps/structure/ohl-structure-edit`,
[crates](structure-app.md)): every edit of the structure application and
of `ohl-app`'s structure screen is a function on the file's text
(`toml_edit::DocumentMut`, comments and order kept) given the structure
as last read; `StructureFile` keeps the undo and redo of whole edits.
Body tools (same crate): `generators::generate(t, g) -> Body` (joints,
members, points put on tips; pure, ids from the inputs; numbered members
`<id>-M<n>` count only themselves, members the options add have ids of
their own, and a member cut by an option's joint becomes `<its id>-1`,
`-2`…), `generators::Frustum` (the panels' levels and widths, piecewise
straight between the widths given; `slope_changes`),
`rebuild::{apply, rebuild, rebuild_all, remove_generator, write}`
(replace only what has the generator's `made_by`; what something else
uses is kept, made by hand, and reported in `Report::kept`; a member's
pieces take what was set on it, and back),
`symmetry::{add_joint, add_member, move_to, removal, make_symmetric}`
(images matched by position within 1 mm), `groups::{groups, set_group,
set_group_section, set_group_kind}`. `StructureFile` refuses a file whose
`[[generator]]` or `[[symmetry]]` tables do not read (`body::check`).
`missing(t, level) -> Vec<Missing>` lists what the next level lacks
(`Need::Level`: no member, a member without a section — one item per
member —, no `[[section]]`, no supported joint (a `support`; a `spring`
alone does not count); a pole's outline, its
material, strength, wall (`Place::Pole`); an extension member without a
section or a piece without a supported joint (`Place::Extensions`)) and
advice (an extension without members at level 3: the FE model leaves it
out)
(`Need::Advice`: a point on no joint or member, a joint on one member or
none, members crossing within 1 mm away from their ends without a
joint); each item names its row (`Handle`) or its table (`Place`). It
reads the structure only; `detail()` stays the source of the level.
The recipes are edited by place (`load_cases`, `add_load_case`,
`add_load`, `set_load_case_field`, `set_load_field`, `remove_load`,
`remove_load_case`; `merge_load_cases` takes the cases of a `.ohlslc`
text: same name replaces, others are added; `store_given_cases` writes
the line's forces as cases of given values — a `value` and `direction`
(`x`, `y`, `-z`) per point and non-zero component, `line_case`, a
`description` starting "Stored from the line" — through it;
`take_line_load_cases` one recipe per line case taking its factored
values at every point; `set_case_tables` sets keys of a case — its
parts `line` and `structure` (`PART_KEYS`), Convert's `load`,
`self_weight`, `line_case` — from the case's TOML text,
`LoadCaseRecipe::to_toml`, its other keys and comments kept; the Load
case editor writes every change of the parts so, one undo step). The
pole (`pole`:
`add_outline_row`, `set_outline_row` — refused when the heights would
not rise —, `remove_outline_row`, `set_shape`, `set_field` — `wall` checked as a
length or `solid` —, `set_solid` / `is_solid` (`wall = "solid"`, unset
only when solid), `remove_pole`,
`bring_in` a `.ohlp`'s text as a two-height outline from the feet) and
the extensions (`extension_edits`: `add_joint` — refused outside the
piece's heights —, `add_member`, `remove_joint` with its members,
`remove_member`, `set_field`, `set_leg`, `build_piece`). `geometry_with(t,
insulators, extension)` gives the joints and members as
`StructureType::extended` makes them (`Geometry::extension`: the piece's
ids) and `Geometry::pole` the outline's wireframe.

Placed into a line, a type becomes `PlacedStrings { points, strings,
wires }` (global coordinates) that `Line::resolve` merges with the
structure's own items.

### Cross-section library (`.ohlsec`)

Source: `crates/ohl-sections` (a core crate: the level 3 checks inside
the line read it through `ohl-structures-fe`), `SectionLibrary`.
Format `ohl-sections`, version 1 (constants `SECTION_FILE_FORMAT`,
`SECTION_FILE_VERSION`, `SECTION_FILE_EXTENSION`). Full reference:
[Cross-section library](../user-guide/section-library.md).

| Field (TOML key) | Type | Unit | Meaning |
|---|---|---|---|
| `format`, `version`, `name`, `description` | optional | — | header |
| `default_material` | id? | — | material of the sections naming none |
| `[[material]]` | `id`, `name`, `description`, `elasticity`, `shear_modulus?` (default E / 2.6), `density?` (0), `yield?`, `ultimate?`, `partial_factors?` (name → number) | Pa, kg/m³ | `ohl_sections::Material` |
| `[[section]]` | `id`, `name`, `description`, `shape`, `material?`, dimensions of the shape (`h`, `b`, `t`, `tw`, `tf`, `r`, `d`, `sides`; built up: `gap`, required, ≥ 0, and `back` = `"long"`/`"short"`, a `double_angle`'s legs back to back) | m | `ohl_sections::Section` (`Shape`) |
| section, given | `area`, `iy`, `iz`, `iu`, `iv`, `it`, `wel_y`, `wel_z`, `wpl_y`, `wpl_z`, `gyration_y/z/u/v`, `shear_area_y/z` | m², m⁴ (`SecondMoment`), m³ (`SectionModulus`), m | replace the computed values (`Given`) |
| `[wind]` (optional, one per library) | `source?`, `drag?` (shape name → factor > 0), `face_drag?` { `solidity` (rising, 0–1, ≥ 2 values), `factor` (> 0, one per solidity) }, `shielding?` { `solidity`, `spacing?` (rising, ≥ 0), `factor` (0–1: a flat row, or a row per spacing) } | — (factors, ratios) | `ohl_sections::StructureWind` (`Curve`, `Shielding`; `SectionLibrary::wind`): wind factors for structures as **data** (from a design-code plugin's `structure-wind` or typed; none shipped), linear / bilinear, nothing outside the table's range; a second `[wind]` in a folder is an error (`merge` reports `WIND_TABLE`) |

Shapes: `equal_angle`, `unequal_angle`, `channel`, `i_section`,
`rectangular_hollow`, `circular_hollow`, `round_bar`, `polygonal_hollow`,
`general` (properties given only), and since 2026-11-02 the built-up
`double_angle` (two angles back to back), `star_angle` (two in a star) and
`cross_angle` (four in a cross): `Shape::DoubleAngle { h, b, t, r, gap }`
holds `h` = the legs back to back (along z, picked by `back`), `b` the
outstanding ones; `Shape::part()` the angle and how many,
`Section::part_properties()` one angle's own properties (its minor radius
of gyration, for buckling between the stitch bolts),
`Shape::built_up_outline()` the outer edge for drawings (the gap filled).
Since 2026-11-04 also `double_channel` (`Shape::DoubleChannel { h, b, tw,
tf, r, gap }`: two channels back to back, webs `gap` apart; `part()` a
channel and 2; symmetry `Double`; its outline an I with the gap filled).
The format stays version 1 (a program built before refuses the new
shapes, naming them). `Section::properties()` computes area,
centroid, second moments about the geometric and principal axes, torsion
constant, shear centre and warping constant (thin-walled, from the
midlines; not given in the file), elastic and plastic moduli, radii of
gyration and shear areas; `Shape::symmetry()` the shape's symmetry for
buckling (angle, double, about y, about z, point, closed, unknown: which
torsional modes a code checks — ASCE 10-97 3.9.4–3.9.6 in
`ohl_structures_fe::asce10`);
`Section::wind_width(direction)` the outline's width normal to a wind in
the section's plane (body wind from the profiles, decision 10 of
[Building structures](structure-building.md)). Several files merge in
order (`merge`: project first, then user; the earlier id wins);
`missing_materials()` lists sections whose material is in none.

The format stays version 1 with `[wind]` (a table only added; a program built before it refuses a file holding one, naming the key). The `[wind]` factors are used by `ohl_structures_fe::wind` (the member
wind with lattice faces, their solidity ratios and shielding:
`faces(model)` — each `Face` with `solidity`, `overlap` (m², the bound
on area counted twice), `plates` (m²), `solidity_low()`/`solidity_high()`
— `MemberWindSetup` (`with_plates(Vec<WindPlate>)`: `id`, `node`,
`area`, `normal`, `members`; `MemberWind::plates`: `PlateWindRow`s, their
loads after the members'), `Model::member_wind_with`; `Curve::largest_in`,
`Shielding::largest_in`: the largest factor over solidity bounds; nothing
stored, computed from the model's geometry (the plates from the joint
details, `ohl_structure_link::wind_plates`); theory: [Wind on lattice
structures](../theory/structure-wind.md)) in `ohl_loads`' structure check
and `ohl structures fe`. Sections are typed in OHL-Structure
(`ohl_structure_edit::section_library`: `SectionDraft`, `put_section`,
`remove_section`, `toml_edit` on the library file's text).

**Referred to by structures** (structure-building §1.3, §4.2): a
member's `section = "<id>"` (of a `.ohls` full design or an `.ohlfe`)
resolves in the cross-section library first, then in the structure's own
`[[section]]` tables (`Model::parse_with`, `from_structure_type_with`,
`load_with` take the `SectionLibrary`; the plain forms an empty one). A
library section becomes `ohl_structures_fe::Section::from_library`: the
profile's area, iy, iz, torsion constant (iy + iz without one), minor
principal second moment (`i_min`, buckling), elastic moduli, and its
material's E, G, density and yield; it keeps the profile
(`Section::profile`), which gives the member wind
(`Model::member_wind`, `Model::has_profiles`; `[analysis] drag`, default
1). A library id replaces a table of the same id; an id in neither, or a
library section's material missing, is refused, naming it. In a project
the library is the library kind `sections` of the project file, per
project line ([Project file tables](#project-file-tables-cratesohl-session));
the app reads it with `Model::section_library` and gives it to the line's
structure checks (`ohl_loads::capacity::usage_with_sections`). Member
groups (one section for many members) come later and resolve the same way.

Solving (`ohl_structures_fe::solve`, `analyse`): with `[analysis]
stabilise` (default true; `Options::stabilise`), a solve that fails as a
mechanism or unstable is repeated with the joint directions no member
stiffens held (`ohl_structures_fe::stabilise::Stabilisation`: node,
translation or rotation, unit direction, a structure axis removed exactly
or a spring's stiffness, `relative`: the joints and weights it is held
relative to — the ends of the straight lines of members through it —,
empty when held to the ground); `Solution::stabilised` lists them with the
force each took (`Stabilised`; a relative hold passes it to those joints,
the share `w` each), `stabilisation_note` puts them in words for the
usage notes, the structure application's results and `ohl structures fe`.
A stabilisation held to the ground carrying load (above `HELD_SHARE` of
the loads and `HELD_JOINT_SHARE` of its joint's) fails as
`FeError::Held`; a relative one above the same limits does not (owner,
2026-10-19): `Solution::relative_over` lists it (indices into
`stabilised`) and `stabilisation_note` flags it (`LARGE: …`); a remaining
mechanism is `FeError::Mechanism` with the joints that move (`joints`,
`moving`). Nothing is stored: it is worked out per solve.
`Solution::reactions` (`Reaction`, force and moment on the structure) has
one entry per joint with a support or a spring (`Node::is_support`): a
fixed direction gives the support's force, a sprung one the spring's
force −k·u (a direction both fixed and sprung: the support's, the spring
carrying nothing), less the share a hold to the ground takes at that
joint, with the share a relative hold passes to it; Σ reactions balance the loads
with the self-weight, less what the holds to the ground took.

Tension-only members (`MemberKind::Tension`): `MemberForces::bowed` marks
one shortened beyond its own buckling load (`solve::buckling_loads`), its
`axial` that load (−N_cr) and no more, its end forces those of the
constant force; `slack` one without a buckling load (0). The usage
(`check::member_usage`) checks their compression against
`capacity_compression`, else the allowable compression, else A·f_y
(no Euler term: the solve caps it), what governs starting with
`check::BOWED` for a bowed one; `tension_only_note` names the slack and
the bowed ones (with their loads). The structure application's
`MemberResult` has `bowed` beside `slack`; the validation outputs
`bowed_<member>`. Nothing is stored: worked out per solve.

A model member (`ohl_structures_fe::Member`) carries the file's `lk`,
`capacity_tension`, `capacity_compression` (SI, `None` when not given:
the usage computes them as before, `check::member_usage`) and
`dead_load_factor` (`None`: `Analysis::dead_load_factor`, default 1;
`Model::dead_load_factor(member)` gives the one in force, the solver
multiplies the member's self-weight by it, `Model::weight` sums the
factored weights — the structure application's load sum —,
`Model::steel_weight` the plain ones); a member's
`orientation` is resolved when the file is read into its `reference`
(`model::orientation_reference`), so the solver, the member wind and the
exports see only `reference` and `roll`.

Eccentric member ends (2026-11-02, stage 4.4 of the [joint
details](joint-details.md)): `Member::offsets` (`[[f64; 3]; 2]`, m,
structure axes, from the member's first and second joint to where its
axis ends; zero by default, never written to a file) is set from the
connections' `shift` when a structure file is read
(`ohl_structures_fe::offsets::from_shifts`, after `read_connections`:
r = shift · `shift_direction(along, towards)`, towards the member it
bolts onto, else the joint's leg `member_ends::joint_leg`; a shift
without a direction left out and said in `EndDesign::notes`).
`Model::end_point`, `Model::eccentric_ends` (`EccentricEnd`: `end`,
`node`, `offset`, `e`, `carried` = a frame member meets the joint),
`Model::has_offsets`. The solver ties each offset end rigidly to its
joint where the joint has rotations (`offsets::rigid`, the element
between its offset ends), else leaves it out. Said by
`offsets::eccentricity_words` (in what governs after `check::SHIFTED`,
`[shifted: …]`; `capacity_note` counts them), `eccentricity_note`
(per case: the ends, those left out with their moment; the CLI, the
line's structure check, the structure application's notes) and
`ohl_structure_link::MemberResult::eccentric` (OHL-Structure's FE
results: the member marked *(eccentric)*). Nothing stored.

### Insulator assembly (`.ohli`)

Source: `crates/ohl-structures/src/insulators.rs`.

| Field | Type | Unit | Meaning |
|---|---|---|---|
| `id`, `name`, `source` | string | — | referred to by `insulator` of strings |
| `kind` | `suspension` / `strain` (`v_string` of files before version 3 read as `suspension` with `legs = 2`) | — | basic kind |
| `legs` | u32? | — | version 3 (`INSULATOR_FILE_VERSION`): 2 = two strings to one clamp (suspension only; with `half_width`), read into `AssemblyKind::VString` through `AssemblyFile`; 1 or absent: one string |
| `length` | length | m | attachment point to clamp (two strings: each one's) |
| `weight` | force | N | whole assembly |
| `axial_stiffness` | force | N | EA of a string (default 5·10⁷ N) |
| `wind_area` | area | m² | projected area |
| `half_width` | length? | m | `legs = 2` only: centre to each string's top |
| `max_load` | force? | N | version 2: the tension it is rated for (two strings: each one's) — **not checked** by OpenOHL; strings naming it take it where they give none (`InsulatorAssembly::limits`, `InsulatorLimits`) |
| `max_swing`, `max_swing_across`, `max_swing_along` | angle? | ° | version 2: the swings it may take (from straight down any direction; across; along the line), 0–180°, not checked; taken by its strings where they give none. `to_toml` writes version 5 with a torus among the shapes (2026-10-31, `StringVolumes::has_torus`; `INSULATOR_FILE_VERSION` 5), else 4 with volumes, else 3 for `legs = 2`, else 2 when a limit is given, else 1 |
| `swing_across`, `swing_along`, `twist` | `free` / `blocked` | — | version 4 (2026-10-25, roadmap O6; `InsulatorAssembly::volumes`, `ohl_structures::StringVolumes`, `Freedom`): whether its shapes turn with the solved string's swing across, along and about its axis; default free |
| `strings`, `arrangement`, `spacing` | u32 (1–4), `across` / `along` / `triangle`, length | —, —, m | version 4: several strings in one assembly (`Arrangement`; a triangle of three; several need a spacing above 0, one none); the line's string stands for all, on their middle (`StringVolumes::string_offsets`) |
| `[[shape]]` | `StringShape` | m | version 4: `name` (unique), `kind` (`ShapeKind`: sphere, cylinder, box, cone; **torus**, version 5, 2026-10-31), `state` (`ElectricalState`: grounded, electrified, insulated), `at` (length) or `at_fraction` (0–1), `diameter` (a torus: its **outer** diameter), `end_diameter` (cone), `length` (cylinder, box, cone: `ShapeKind::takes_length`), `width`, `depth` (box), `tube` (torus: its tube's diameter, below half `diameter`; `StringShape::torus_radii` → centre-circle and tube radii) — only the sizes its kind takes —, `offset_across`, `offset_along`, `shared` (one for several strings). Read through `ShapeFile` (also the line file's form, SI numbers). Placed (`ohl_clearance::volumes::place`) as `Convex::Torus {centre, axis, major, minor}` around the string's axis — the one non-convex primitive: `separation` measures it exactly against a point or sphere, else by the chords of its circle (`torus_chords`, sagitta ≤ `TORUS_SAGITTA` 0.25 mm; never more than the exact distance, at most 0.5 mm less). Built in the desktop application's String builder (`crate::string_builder`, 2026-10-31) or the String shapes table (column `tube`) |

### Weather and load cases

`WeatherCase` (`ohl-weather`): `name` (the key other files use),
`description`, `tags`, `temperature` (wire), `ice_thickness` or
`ice_load`, `ice_density` (none: glaze ice, `GLAZE_ICE_DENSITY` 913
kg/m³ — for a thickness its weight, for a load its iced diameter
`WeatherCase::iced_diameter`; until 2026-10 a load without a density
left the wind on the bare diameter), `wind_pressure`, `wind_from`
(`right`/`left`, looking ahead), `air_density_factor`,
`ambient_temperature`. Derived cases (2026-10-20, in memory only):
`without_ice`, `with_more_ice_thickness` (a thickness grows; a load
becomes the load out to the iced diameter plus twice the extra) and
`with_more_ice_load` (the ice weight plus the extra, as a load at the
case's density, else glaze ice); `has_ice`. `same_physics` (2026-10-21): whether two cases put the same weather on the wires (temperature; ice none on both, or the same thickness and density, or the same load and density, glaze ice when none given — a thickness never equals a load; wind pressure, and with wind its side and `air_density_factor`; `ambient_temperature`; within `SAME_PHYSICS_TOLERANCE` 1e-6 relative), never the name, description or tags — how a preset's cases are matched to the project's ([plugins](plugins.md#matching-a-presets-weather-cases)).

**A weather case per span** (2026-10-20, in memory only):
`ohl_line::solve::SpanCases` — `base` (the section's case) and `spans`
(`BTreeMap` span index → its own case); `uniform`, `with_span`, `of`,
`is_uniform`. `solve_section_span_cases(_coupled)` solve a section with
it (finite elements only; the ruling span refuses spans in different
cases); one case for all is `solve_section` bit for bit. In the plain
section model, `ohl_wirefe::State::span_states` (`(span, SpanState {
temperature, cable_load })`, empty: all in the state's).

**Local winds** (2026-10-25, roadmap O15): a section's `wind_factors`
(above) become, in a case with wind, each span's own case with the
pressure times its factor (`ohl_line::solve::LocalWinds`, in memory only,
over a span's own case where it has one); `WeatherCase::with_wind_factor`
(a factor of 1 or no wind: the case unchanged).

**Several broken spans together** (2026-11-03, in memory):
`solve_section_broken_spans(line, section, cable, case, spans, budget,
coupling)` and `solve_line_broken_spans(line, library, case, &[(section,
spans)], budget)`: every span listed gone in one solve; a point between
two broken spans holds no wire; `SectionShape::broken_spans` (all, in
order) beside `broken` (the first). The single-span functions call them.

**A broken wire** (2026-10-25, roadmap O9, in memory):
`ohl_line::solve::solve_section_broken(line, section, cable, case, span,
budget, coupling)` (span 0 = first; FE only) and `solve_line_broken(line,
library, case, section, span, budget)` (the other sections as
`solve_line_with`); the shape's `SectionShape::broken` = `Some(span)`, its
`spans[span]` with no points and no tension, the flag `span n: the wire
broken`; the ends next to the break that hold no wire report their
strings' hanging weight as their `attachment_loads`.

`LoadCase` (`ohl-loads`): `name`, `description`, `weather` (a weather
case name), `condition` (initial, creep, load, max sag), factors
`vertical`, `transverse`, `longitudinal`, `strength`, `tags`;
`broken` (version 3, `[case.broken]`, `BrokenWire`: `section` id, `span`
`BrokenSpan::One(n)` from 1 or `Every` (`"every"`), `set` u32? — a check:
the set of a section point at one of the span's ends; `BrokenWire::spans(line)`
the spans broken from 0, or why not; `describe()` → `S3 span 2 broken`);
written at version 3 only when a case has one (`written_version`);
**version 5** (2026-11-11, roadmap O22 item 7; `LoadCase::uses_version_5`,
written at 5 only then): `BrokenWire.subconductors` u32? — of a bundle,
how many subconductors break in the span (1 to the section's `bundle`;
none or all: the whole phase; `validate` refuses 0, `spans(line)` more
than the bundle; `breaks(spans)` → `ohl_line::solve::Break { span,
subconductors }` for `solve_section_breaks` / `solve_line_breaks`;
`partial_note(line)` the case's note; no stored result changes: the
span's tensions in `SpanShape` are per wire left, its flag
`partial_break_flag`);
**version 4** (2026-11-03; `LoadCase::uses_version_4`, written at 4 only
then): `broken_also` (`[[case.broken]]` after the first, in `broken`;
`broken_wires()`, `has_broken()`, `set_broken(vec)`; the v3 table reads
as one; `validate` refuses `"every"` twice or beside another break of
its section, a break given twice), `removed` (`[[case.removed]]`,
`RemovedWire { section, set? }`, `check(line)`, `describe()`; refused
twice or also broken), `wind_from` (`WindSide`?; `wire_weather(w)`),
`use_local` (bool), `on_structure` (`StructureOptions`: `direction`
`WindDirection::{Right, Ahead, Left, Back, Angle(deg)}` — from, 0° right,
90° ahead, `towards()` (−cos, −sin) in (x right, y ahead); `ice`
`StructureIce::{Local, Thickness(m), NoIce}` (`ice = false` or `"none"`, 2026-11-04; not given: the local ice in a case with `use_local` whose weather case has ice, `takes_local_ice(w)`, `ices_or_snows(w)`); `ice_density` kg/m³?; `snow`;
`snow_shape`, `snow_exposure`, `snow_thermal` (μ, C_e, C_T; defaults
`DEFAULT_SNOW_*` 0.8, 1.0, 1.0); `snow_factor()`). Per structure:
`structure_wind_at(w, s)` (× local `wind_static` with `use_local`),
`structure_ice_at(w, s) -> Result<Option<IceOn {thickness, density}>>`
(Err without a density; the local ice also when `takes_local_ice`), `structure_snow_at(s) -> Option<SnowOn {ground,
factor}>` (`load()` = μ C_e C_T s_k, `snow_load`). `LoadCases::to_toml()`
/ `LoadCase::to_toml(version)` write a file at `written_version`.
`structure` (version 2, `[case.structure]`, `StructureWind`:
`wind_pressure` Pa?, `wind_factor`?) — the wind on the structures' own
bodies when it differs from the wires': each given replaces, for the
structures only, the weather case's pressure or the transverse factor.
`LoadCase::structure_wind(weather) -> BodyWind` (`pressure`, `factor`,
`towards` −1/+1 across the line — with a direction the transverse part
of where it blows —, `direction` (`WindDirection`, version 4),
`own_pressure`, `own_factor`; `factored()`, `origin()`, `towards_xy()`,
`has_longitudinal()`) is the one rule every structure check takes:
the wind bands (`add_structure_wind`), the member wind and the pole wind
of the capacity checks (`analysis.rs`), and the line values sent to
OHL-Structure. The wires and strings never take it.

Results of `ohl_loads::structure_loads` (derived, never stored):
`CaseLoads` per load case — `case`, `loads` (`AttachmentLoad` per
structure and point), `failed` (sections and jumpers not solved, in
words) and `missing` (`MissingWire`: `structure`, `kind`
`WireKind::Section`/`Jumper`, `id`, `reason` — one per structure a
section not solved ends at or hangs on, or whose strings hold a jumper
not solved: that structure's loads in that case are **incomplete**;
`CaseLoads::missing_at`, `is_incomplete`) and `structure_wind`
(`BodyWind` of the case), `notes` (version 4: what the case assumed, in
words). `add_structure_loads(results, line, types, sections, weather)`
(2026-11-03): `add_structure_wind` (the bands, by the case's direction,
× local `wind_static` with `use_local`, only for structures taking the
case) and the ice and snow per structure as loads without a point ("ice
on the structure", "snow on the structure"; `analysis::body_weights` on
the model's members with a profile, `analysis::pole_ice` on a pole's
shaft). `AttachmentLoad::pulls_by_set` (`Vec<SetPulls>`: `set`, `back`,
`ahead`; 2026-11-11): the two pulls below per wire set held at the
point, in order of the sets, adding up to them (the same shares; the
envelope of a broken wire in every span keeps each set's with the
component's break). `AttachmentLoad::pull_back` and `pull_ahead` (`[f64; 2]`,
2026-11-10; transverse, longitudinal in the structure's axes, N,
unfactored): the horizontal pull of the point's back and ahead spans,
along each span in plan (`span_pulls`: of `SpanShape::end_pulls` the
part along clamp to clamp; "back" the span before the point in its
section's order, "ahead" the span after; a clamp held from several
structure points shares it equally, `add_span_pulls`; a share of
`local`, not added to it; the envelope of a broken wire in every span
keeps the pulls of the break giving each component); not in the loads
CSV; sent to OHL-Structure as four line quantities.
`AttachmentLoad::broken_span`

(`Option<[usize; 3]>`, 2026-10-25): in a broken-wire case the broken
span (from 1) giving each component (transverse, longitudinal,
vertical) — with `"every"` the envelope's (the largest factored of each
component by size over the spans, sign kept, the line-axes force made
again of them); the loads CSV's last column `broken_span`.
`CaseLoads::broken_each` (`Vec<BrokenLoads>`, 2026-10-26): with
`"every"` (more than one span) each break's own loads (`span` from 1,
`loads`), empty otherwise; the finite-element and pole checks take each
and keep the worst (the envelope is no bound of a member force), the
allowable-loads check the envelope. `CaseLoads::each_break()` makes a
case of each break (`<case> (span k broken)`, its `broken.span` that
span), which `ohl_structure_link::placed_values` writes into the
line-values file instead of the envelope. The usage classification is
`ohl_structures_fe::check::{classify_usage, UsageClass, NEAR_FULL,
usage_order}` (a usage that is not a number: over, ranked above every
other), re-exported by `ohl-structure-link`. The usage
(`ohl_loads::capacity`, `CaseUsage` → `StructureUsage`) carries the
same per structure (`StructureUsage::missing`), marks each `Check` from
such loads `incomplete: true`, and `StructureUsage::status()` gives
`UsageStatus::{NotChecked, Incomplete, Approximate, Pass, Over}` —
`Incomplete` whenever a wire is missing, whatever the usage;
`Approximate` (after that) when a check has `approximate: true` — the
FE check of a model whose tension-only members did not settle
(`ohl_structures_fe::Solution::kept_active`: those members kept active
as trusses; `StructureUsage::is_approximate`, `approximate_text`). The
usage CSV (`usage_csv`, `ohl line --usage-csv`) ends with an
`incomplete` column: `incomplete: section S12 not solved (why); …`,
then `approximate: fe (…)` for an approximate check, empty when
neither.

### Structure load cases (`[[load_case]]`, `.ohlslc`)

The structure application's recipes
(`apps/structure/ohl-structure-loads`, [crates](structure-app.md)): in a
`.ohls` as `[[load_case]]` (the core keeps them as raw tables,
`StructureType::load_cases`), or on their own in a `.ohlslc` file
(`format = "ohl-structure-load-cases"`, version 4: `back` and `ahead`
of a case's line forces and the line quantities `pull_back_*`,
`pull_ahead_*` (`LineQuantity::is_span_pull`, 17 quantities in
`LineQuantity::ALL`; in a `.ohls` version **10**,
`ohl_structure_edit::SPAN_SHARES_VERSION`); 3: a case's two parts
`line[]` and `structure`; 2: the line quantities `structure_wind*`; 1
to 3 read too; written at the lowest version the cases need,
`written_version`). In a `.ohls` the parts make the file version **7**
(`ohl_structure_edit::PARTS_VERSION`, raised after any edit leaving a
case with `line` or `structure`, `raise_for_line_pressures`). Full
reference: [Structure load cases](../user-guide/structure-load-cases.md);
spec: [structure-load-cases-spec.md](structure-load-cases-spec.md).

| Item | Fields | Refers to |
|---|---|---|
| `LoadCaseRecipe` (`[[load_case]]`) | `name`, `description` (several lines), `strength`, `line[]`, `structure`, and the older form: `line_case`, `self_weight`, `load[]`; `self_weight_factor()`: `structure.self_weight` when given (the case's `self_weight` must then be 1, checked), else `self_weight` | `line_case` ─► a line result (older loads) |
| `LineForces` (`[[load_case.line]]`, 1. line forces) | `result`, `vertical`, `transverse`, `longitudinal` (factors, default 1, always written), `sets[]` (default every point), `off[]` (point labels whose line forces are left out), `back`, `ahead` (f64?, percent ≥ 0, absent = 100; `.ohlslc` 4 / `.ohls` 10, 2026-11-10: the share of the back / ahead spans' solved horizontal pull taken — force(T, L) = sent − (1 − back/100)·pull_back − (1 − ahead/100)·pull_ahead, vertical unchanged, then the factors; `LineForces::span_reductions`, `table::span_share_off`; a load assumption: rows noted "NOT ON THE SAFE SIDE: " + `SPAN_SHARE_ASSUMPTION`; without the pulls from the line the row is "no line value") | `result` ─► a line result name (`PlacedValues::cases`, `<weather> — wind from the left/right`) |
| `OwnLoads` (`[load_case.structure]`, 2. loads on the structure itself) | `self_weight?`, `wind[]`, `ice[]`, `point[]`, `snow[]` (2026-11-03) | — |
| `BodyWindLoad` (`[[load_case.structure.wind]]`) | `from` (`WindFrom`: `left`, `right`, `ahead`, `back`, `ahead_left`, `ahead_right`, `back_left`, `back_right`, or since 2026-11-03 an angle in degrees, `WindFrom::Angle`, written as a number: 0 from the right, 90 from ahead; `from_degrees` gives a name for the eight angles), `pressure` (Pa) **or** `pressure_of` (a line result: its `CaseWind.pressure`, unfactored), `factor` | `pressure_of` ─► a line result |
| `SnowLoad` (`[[load_case.structure.snow]]`, 2026-11-03) | `load` (Pa, on the plan), `members` (`"all"` or ids), `factor`; rows of source `snow`, `Part::Snow` from 5001; `MemberAreas::plan_area` | member ids |
| `IceLoad` (`[[load_case.structure.ice]]`) | `thickness` (m) + `density` (kg/m³) **or** `load` (N/m); `members` (`"all"`, default, or ids); `factor` | member ids |
| `PointLoad` (`[[load_case.structure.point]]`) | `value` (N), `direction`, `points` / `sets` / `phases`, `note` | point labels, sets, phases |
| `LoadRecipe` (`[[load_case.load]]`) | what: `what` (`LineQuantity`; `structure_wind`, `structure_wind_factored` — version 2 of `.ohlslc`, 6 of `.ohls` — are pressures on `members`, `LineQuantity::is_pressure`) **or** `value` (N) **or** `pressure` (Pa); how: `line_case`, `factor`, `one_sided`, `side`, `direction` (structure axes); where: `points` (`"all"` or labels), `sets`, `phases`, **or** `members` (`"all"` or ids); `note` | point / wire point labels, sets, phase labels (from the line), member ids of the `.ohls` |

Evaluated (`evaluate`) with `LineValues` (per point: sets, phases; values
per line load case, point or set and quantity) into a `LoadTable`
(rows: `case`, `load`, `at` point/member/joint, `label`, `force`
transverse/longitudinal/vertical down N, `source`
recipe/line/wind/ice/point/typed — the part a row comes from —,
`evaluated`, `missing`, `note`, `area` — a surface load's member area,
m²). Row numbers (`load`) per part, so typed values keep matching
(`ohl_structure_loads::Part`, `PART_BLOCK` = 1000): older loads 1–999,
line forces 1001…, wind 2001…, ice 3001…, point loads 4001…; a point
turned off keeps its row (force 0, note "off"). The self weight has no
rows (the solve puts it on the members × `self_weight_factor()`). The
members' wind, outline and length come through the `MemberAreas` trait
(`member_ids`, `member_wind` → `BodyWind` (`members`, `joints`: the
gusset plates' wind per joint id, rows `at = joint`, `notes`),
`perimeter`, `length`;
default: not known, the rows say why), implemented by
`ohl_structure_link::SectionAreas` with the section library's `[wind]`
factors (`ohl_structures_fe::wind::MemberWindSetup::wind_iced`: iced
members 2 t wider). Started for a placed structure, the structure application puts
the line's forces first (a row per line case and point, source `line`,
no recipe; a line case named as a recipe is "NAME (line)"). The table
is a result, not stored in the structure file (the line's cases can be
stored as `[[load_case]]` of given values, above). Typed
values (`Override`: case, load, at, label, force) are kept by the
structure application beside the structure file as **`<stem>.loads.csv`**
(the table's CSV, typed rows only; the structure on its own) or
**`<stem>.<placed structure>.loads.csv`** (started for a placed
structure, `--structure`; the id with characters other than letters,
digits, `-`, `_` as `_`), written on Save and read on open
(`ohl_structure_loads::overrides_path`). In a project the line
application keeps both in the project as library kind **`loads`** (beside
the structure types in the working folder; only `*.loads.csv`), so the
values typed for a placed structure are stored with it per project line
— no field of the line file. A table read back (CSV) types what it has
beyond the evaluation (`LoadTable::overrides_over`). `[[load]]` is read
as case `model` (joints). CSV of the table (the `area` column optional
when read) and the recipes; XLSX later.

`LineValues` also carries `structure_wind` (`CaseWind` per line case:
`case`, `pressure` Pa, `factor`, `towards` `-x`/`x`, `origin` — empty
when the line's values, else which of the case's own); a load taking a
structure wind is evaluated on its members as a surface load
(pressure × factor for `_factored`, the wind's direction unless the load
gives one, source `line`; `line_pressure`), and
`resolve_line_pressures` writes those pressures into the recipes for a
run (the extension's piece, the pole's wind). OHL-Structure adds, per
line case with a wind on the body, a recipe of the case's name taking
`structure_wind_factored` on all members (`line_wind_recipes`, rows of
source `line`; on a pole, its wind); stored from the line
(`GivenCase::wind`) as a `pressure` on all members.

`ohl-structure-link` fills `LineValues` from `ohl_loads::structure_loads`
(the six force values and the case's `structure_wind`; tensions, wire
wind and ice not yet), solves
table cases on the structure's model (`analyse_cases` → `CaseResult`:
joints with displacements, members with axial force and usage,
reactions, Σ loads and Σ reactions, and `points` — `PointMoment` per
attachment point at an offset: label, the moment r × F of its rows
summed (N·m, structure axes), `applied` (a frame member meets the
joint) or left out with its `note` (2026-10-21); members' sections from
the section library first) and gives the members' areas from the cross-section library
(`SectionAreas`: `Model::projected_area` — the profile's width for the
pressure's direction in the member's y–z plane × length × sin θ; a
member whose section is not in the library has no area).
`analyse_structure` (`RunInput` → `StructureRun`) solves them on an
extension (`Model::from_structure_type_extended`; a case's pressure on
`members = "all"` put on the extension's own members too) and runs the
pole check (`check_pole`, the case's point rows at the points as
written, its horizontal pressures on all members as the wind on the
pole): `StructureRun` holds `extension` and `dz`, run `notes`, the
`sections` used (`SectionUse`: id, material, A, E, f<sub>y</sub>, weight per
metre, members, length), the `pole` (`PoleInfo`) and per case
`StructureCase` (`fe`: `CaseResult`, `pole`: `PoleResult` — usage,
height, moment, capacity, ground and wind moments, P-Delta factor, wind,
governs, notes); `checks()` gives per check (`members`, `pole`) the
largest usage and its case. Plain data in memory; nothing new in any
file format.

**Line-values file** (`<stem>.<placed structure>.line-values.json`
beside the structure file, `ohl_structure_loads::PlacedValues`, JSON,
`format = "ohl-structure-line-values"`, version 6 or 7; versions 1 to 5
are read too; 7 (2026-11-11, roadmap O22 item 7) adds, at a point
holding more than one wire set, each set's part of the four pull
quantities — `LineValue`s with both `point` and `set`
(`LineValues::get_set`; `AttachmentLoad::pulls_by_set`), which a case's
line forces with `sets` and `back` / `ahead` need to scale some sets'
pulls only; a file without such values is still written at 6
(`PlacedValues::needed_version`, `at_needed_version`), and a point's own
value is the row without a set (`LineValues::get`); 6 (2026-11-10) adds four `LineQuantity` values per point and
case among `values` — `pull_back_transverse`, `pull_back_longitudinal`,
`pull_ahead_transverse`, `pull_ahead_longitudinal` (N, structure axes,
unfactored; `AttachmentLoad::pull_back` / `pull_ahead`; a side without a
span 0; absent in older files) — and `note` may say that the structure
takes no load case in the line; 4 adds `structure_wind` in `values` and in each of `others`;
5 (2026-11-03) adds `library_cases` — the placed structure's set of the
line's load case library as `LoadCaseRecipe`s (line forces with the
case's factors, `self_weight`, body wind from the case's direction with
its pressure × `wind_static` under `use_local`, ice and snow on every
member; `ohl_structure_link::library_recipes`, `placed_values_with`),
its cases' values among `cases` as load cases — and `from_deg` on a
`CaseWind` from an angle): what the line application hands to the structure application
for **Edit…** on a placed structure — `structure` (placed structure id),
`line` (project line, or the line file's name), `structure_type`,
`cases` (the cases the line sends, its `structure_loads`: the load cases
chosen, then the weather cases chosen on their own, `weather: <name>`),
`sources` (version 2, in the order of `cases`: `load_case` or
`weather_case`, `ohl_structure_loads::CaseSource`; absent in version 1:
every case a load case), `values` (`LineValues`), `others`
(`OtherPlace`: the other placed structures of the type with their
`LineValues`; each also with its `missing`), `note` (why there are no
values: being computed, no load case and no weather case chosen;
sections not solved; chosen cases no longer in the line), `missing`
(version 3, absent when empty or in older files: per case the sections
and jumpers at this placed structure that did not solve,
`ohl_structure_loads::MissingWire` — `case`, `kind` `"section"` or
`"jumper"`, `id`, `reason`; filled by
`ohl_structure_link::missing_wires` from `CaseLoads::missing`; the
structure application marks those cases incomplete:
`PlacedValues::incomplete(case)` → `incomplete: S12 not solved`).
Derived data, rewritten when the line changes; never stored in the
project.

### Structure report (HTML, members CSV)

Written by `ohl_structure_export::report` (Results › Structure report…
of the structure application, tool `ohl-structure-report`) from a
`StructureRun`; nothing is read back. One HTML page with no external
resource (styles inline, no scripts, images or fonts) and
`<name>.members.csv`: `case,member,axial_n,usage,governs,slack` (N,
tension positive; usage as a share, empty without a capacity), every
member of every case solved by the FE model, the fields as
`MemberResult` gives them.

### Structure request and reply (JSON)

How the command line asks the structure side for results without
depending on its crates (`ohl structure analyse|report`, `crates/ohl-cli/src/structure_app_run.rs`;
answered by `ohl-structure-report --request FILE|-`,
`ohl_structure_export::request`). The tool is found as OHL-Design finds
OHL-Structure: the program `OHL_STRUCTURE_REPORT` names, else
`ohl-structure-report` next to `ohl` (or in the folder above), else on
the `PATH`. Request on the tool's input, reply on its output; exit status
0 when `ok`. Each side has its own types; the JSON is the contract
(`apps/structure/ohl-structure-tests/tests/ohl_structure_cli.rs`).

| Request (`ohl-structure-request`, version 1) | |
|---|---|
| `format`, `version` | `"ohl-structure-request"`, 1 (a newer one refused) |
| `action` | `analyse` or `report` |
| `structure` | the `.ohls` file (relative paths: from the working folder) |
| `sections` | cross-section library files or folders (empty: `Sections/` beside the file) |
| `loads` | a load table CSV, or null (the file's own recipes without line values) |
| `extension` | the extension to stand on, or null |
| `cases` | the cases to solve (empty: all; an unknown one is an error) |
| `report` | `report`: the `.html` to write (the members' CSV beside it) |
| `date` | `report`: write today's date |

| Reply (`ohl-structure-reply`, version 1) | |
|---|---|
| `format`, `version` | `"ohl-structure-reply"`, 1 |
| `ok`, `error` | the run went through; else why (the rest empty) |
| `software` | `ohl-structure-report <version>` |
| `structure`, `extension`, `dz` | the structure's id, the extension stood on (null: as written), its `dz` (m) |
| `notes` | what applies to every case (`StructureRun::notes`, sections given twice) |
| `checks` | per check (`members`, `pole`): `check`, `usage` (share, null: none), `case`, `governs`, `failed` ([case, why]) |
| `cases` | per case: `name`, `usage` (largest over its checks; null when not finite), `members` and `pole` (null: no such check): `usage`, `governs`, `error` (null: it ran), `notes` |
| `files` | `report`: `report`, `members_csv` (paths); else null |

### Safety volumes (`ohl_clearance::safety_volume`, `ohl_io::safety_volume`)

Computed, not stored in the line. `SafetyParams`: `distance`
(`Requirement`), `margin`, `tolerance`, `grouping` (`Wire`, `Circuit`,
`System` — 2026-10-21: the circuits of one electrical system together;
the wires without a system, earthed wires and sections without one, one
volume `NO_SYSTEM` "(no system)"), `circuits` (names; empty: all),
`systems` (system ids; empty: all; `NO_SYSTEM` takes the wires without a
system, left out when not named; `run` reads `none` as it too unless the
line has a system of that id or name, `no_system_key`),
`spans` (section id, span index from 0; empty: all; a jumper kept when
its first two `ends` are ends of spans taken), `parse_span`
(`SECTION:N`, N from 1),
`system_of` (section id or `jumper:<id>` → system id and name, filled by
`run` from the line, `system_map`; a volume per system is named after
the system and its `circuit` lists its circuits); `WindMode` (`None`, `Steps`, `Arc`, `Both`) with `WindSteps`
gives the positions (`positions::solve_positions`, with
`PositionOptions::jumpers` since 2026-10-21: a volume's `sections` then
names its jumpers, `jumper:<id>`, too; `WindSteps::band_margin` the
spans' bands' margin, `WindSteps::jumper_band_margin` the jumpers'
(2026-10-21), `wind_text` the wind in words with both
(`BandMargins`) — the exports' `Description::wind`). `SafetyVolumes`:
`volumes`, `cases`, `positions`, `notes`; each `SafetyVolume`: `name`,
`circuit`, `phases`, `sections`, `kv`, `distance`, `margin`, `outer`,
`positions`, `arc_error`, `arc_error_median`, `allowance`, `stats`
(`MeshStats`: cell, level, evaluations, cells, vertex range), `simplified`
(`SimplifyStats`), `mesh` (`Mesh`: `vertices` in the line's coordinates,
`triangles` counter-clockwise from outside, closed), `seconds`, `timings`.
Files (`ohl_io::safety_volume::write`, `Format` `dxf`, `ifc`, `kml`,
`kmz`; `Description`: line, crs, distance, wind, date): DXF R12 `3DFACE`s
by layer `SAFETY_<name>` + `.prj` (WKT from `ohl_crs::Crs::wkt`, EPSG
systems); IFC4 `IfcSpatialZone` (since 2026-10-21; before, an
`IfcBuildingElementProxy`): Name, Description, ObjectType `SafetyVolume`,
placement relative to the site, LongName, PredefinedType `.USERDEFINED.`,
referenced in the `IfcSite` by one `IfcRelReferencedInSpatialStructure`
(the site aggregated in the `IfcProject`), with an
`IfcTriangulatedFaceSet` relative to a whole-metre origin, `IfcMapConversion` (eastings, northings, orthogonal
height of that origin; x axis 1, 0; scale 1) to an `IfcProjectedCRS`
(name `EPSG:<code>`), property set `OHL_SafetyVolume`; KML placemarks of a
`MultiGeometry` of polygons (WGS84, absolute heights kept), `ExtendedData`
with the same properties.

### Structure exports (IFC, SAF, CSV)

Written by `apps/structure/ohl-structure-export` (File › Export of the
structure application, tool `ohl-structure-export`); nothing is read
back. `ohl_structure_link::export_structure` turns a level 3 `.ohls`
(through the FE `Model`), the cross-section library and a `LoadTable` into
plain data, `ExportStructure`: `nodes` (`id`, `point`, `fixed[6]`,
`spring[6]`), `materials` (E, G, density, yield), `sections` (`shape`
from the library or none for a `[[section]]` table, A, Iy, Iz, It, Wpl,
the centroid's offset from the outline box), `members` (joint and
section indices, kind, local axes `[x, y, z]` as the FE model takes
them, length, k, pretension) and `load_cases` (`self_weight` factor,
loads at a joint, at a point along a member (not made from attachment points any more: they are on joints), or uniform per metre along
one; structure axes, z up); `connections` and `plates` as the file gives
them. `ohl_structure_link::export_joints` (2026-11-02; the tool's
`ohl_structure_export::structure` calls it, with the bolt library) adds,
for a file with `[[connection]]` or `[[plate]]` only, what
`joint_details` works out: `bolts` (`ExportBolt`: joint, member, end,
`point` on the face between the two parts clamped, `axis` towards the
head, diameter, hole, grade, library bolt, `grip` [leg, onto], `sizes`
[across the flats, head, nut] from the library or `BOLT_DRAWN`,
`beyond`; `length()` under the head), `joint_plates` (`ExportPlate`:
id, joint, thickness, material, `outline` (a face), `normal` through
the thickness, `outline_given`, members) and `body_offsets` (per member,
its solid's shift: the joint view's seat when its bolted ends agree,
else none; empty for a file without connections). IFC4 and IFC2x3 write
them as `IfcElementAssembly` per joint aggregating `IfcMechanicalFastener`
and `IfcPlate` ([structure exports](structure-exports.md#bolts-and-gusset-plates)). Formats: IFC4 (members and the structural
analysis model), IFC2x3 Coordination View 2.0 (members only, for
programs that read that view; its owner history's `IfcPerson` has the
family name `Unknown`, as the schema's rule WR1 needs a name), SAF 2.2.0
xlsx, CSV tables. Mapping and what is lost:
[Structure exports](structure-exports.md); checks with free tools
(`tools/check_structure_exports.py`) and sample files
(`examples/exports/`, made from the invented `every-profile.ohls`; a test
keeps them equal to what the exporter writes):
[Structure exports › Checks with free tools](structure-exports.md#checks-with-free-tools).

## Where data lives

### Files

| Store | Format | Content | Reference |
|---|---|---|---|
| Line file `*.ohl.json` | JSON, `ohl_line::Line` (serde; unknown fields refused) | one line | [Line files](../user-guide/line-file.md) |
| OHL XML | XML, namespace `urn:openohl:ohl-line:1`, schema `docs/schema/ohl-line.xsd` | the same content as the line file, lossless round trip (`crates/ohl-io/src/ohlxml.rs`): objects are elements, plain values attributes named like the JSON keys, points `x`/`y`/`z` attributes | [OHL XML schema](../schema/ohl-line.md) |
| Project file `*.ohl` | SQLite (ADR-0004), session schema 6 | project lines, operation log, users, libraries, versions and reviews, edit rights, spatial index, surfaces, point clouds, backgrounds, settings | [below](#project-file-tables-cratesohl-session), [Project files](../user-guide/project.md) |
| Libraries | TOML directories | see above | — |
| Plugin settings `settings.toml` (in the user's plugin folder, the line application's data folder `plugins/` — storage id "OLS CAD", kept from before OpenOHL) | TOML, `ohl_plugins::PluginSettings` (unknown fields refused) | `folders` (plugin folders used in place), `disabled` (plugin names turned off), since 2026-10-26 `allowed` (`[allowed."name@version"]`: `unsandboxed = true` for a Python plugin, `seconds`, `memory-mib` for a WebAssembly plugin's larger limits — what the user allowed in the plugin manager, `ohl_plugins::Consent`; written only when something is allowed, so a file without it reads as before); read and written by `ohl-design` and `ohl-structure` alike |
| Server key file (`ohl serve --keys FILE`; with `--code` alone `PROJECT.ohl.server-key` beside the project; 2026-10-26) | text, 64 hexadecimal digits (the 256-bit key of the personal invitations), mode 0600, made when missing | personal access code = HMAC-SHA-256(key, `user:NAME`), first 16 bytes in hexadecimal; the general code stays the first 6 bytes of SHA-256(secret, `\n*`) | [Server mode](../user-guide/server.md#invitations) | [Writing plugins](plugins.md#where-plugins-are-found-and-the-shared-settings) |

### Project file tables (`crates/ohl-session`)

A project file (`.ohl`) is **one SQLite database** (ADR-0004). Three
crates open the same file, and each makes the tables it needs when they
are missing (`CREATE TABLE IF NOT EXISTS`):

| Opened by | Its tables | Schema number |
|---|---|---|
| `ohl_session::Session::open` | `ols_meta`, `session_user`, `model_object`, `oplog`, `spatial_index`, `surface`, `vc_blob`, `vc_revision`, `vc_ref`, `edit_grant`, `vc_review`, `vc_review_note` | `ols_meta` `session_schema` = `6` (`ohl_session::SCHEMA_VERSION`) |
| `ohl_pointcloud::Store::open` | `pointcloud`, `pointcloud_source`, `pointcloud_block`, `pointcloud_lod` (and `ols_meta`, if missing) | `ols_meta` `pointcloud_schema` = `2` |
| `ohl_background::Store::open` | `background_image`, `background_tile`, `background_drawing`, `background_layer` (2026-10-25) | none (tables only added) |

The file is kept in SQLite's WAL journal mode: while a program has it
open, `project.ohl-wal` and `project.ohl-shm` lie next to it and hold the
latest changes. `Session::checkpoint` writes them into the file itself;
`Session::copy_to` (`VACUUM INTO`) writes a consistent copy;
`Session::copy_for_others` (what a client joining a server gets, since
2026-10-26) is that copy with the settings holding an absolute path of
this computer cut to the file name. Every connection to a project file
(`ohl_session::harden`, also the point-cloud store) runs with
`trusted_schema` off and SQLite's defensive mode on (2026-10-26; nothing
in the file changes). Copy a project only when no program has
it open, or with SQLite's own backup.

```text
  ols_meta  ── session_schema, pointcloud_schema, grid_size, vc_head,
               routes_alignments, alignment_points, setting.<key> …
  session_user (name → role)          edit_grant (who may edit what)

  vc_ref ── one row per project line (kind 'branch') or tag ('tag')
    │  name ─────────────┬───────────────────────┬──────────────────────┐
    │  revision ──┐      │                       │                      │
    │  base ──────┤      ▼                       ▼                      ▼
    │             │  model_object.line       oplog.line        spatial_index.line
    │             │  (kind, id) → JSON      seq, user, command, (kind, id) → bounds,
    │             │  ord, version, gone     inverse, objects,   home tile
    │             │                         sections, undoes…
    │             ▼
    │        vc_revision (id = SHA-256) ── parents [ids], tree ──► vc_blob (hash → zstd text)
    │             │  seq: the oplog entry it was made after          ▲
    │             └───────────────────────────────────────────────── tree lists objects and
    │                                                                 library files by hash
    └── vc_review (branch → target, revision) ──► vc_review_note (verdicts, comments)

  Shared by every project line (not in revisions, not logged except change_points):
  surface (TINs) · pointcloud ─► pointcloud_source, pointcloud_block ─► pointcloud_lod
  background_image ─► background_tile · background_drawing
```

**Merging** (`ohl_session::versions`, no table of its own; 2026-11-10):
a merge is worked out as a `MergePlan` (`base`: a latest common ancestor,
`Session::merge_bases`; `conflicts`; `notes`, e.g. a history that
crosses) and settled by `Choices` (`Conflict::key` → `Side`: `ours`,
`theirs`, or `both` where `Conflict::keep_both` names the id the other
side's object takes — an object each side made under one id).
`versions::keep_both` gives the few objects renamed as a `Patch` laid over
the other side's state by `merge_states_with` (no state is copied);
`rename_references` is the rename command's rule, shared. `Session::unit`
runs a merge, a revert and Bring in as one unit of work (an SQLite
savepoint; `Step` is a write step that nests in it); the file's schema is
unchanged (still 6), and `both` is a new value of a choice sent to a
server (`ohl-server`), which older servers refuse.

#### Opening a file: schema checks and upgrades

`Session::open` (every program that opens a project: `ohl-design`,
`ohl-structure`, `ohl project …`, `ohl line FILE.ohl`, the server) does,
in this order:

1. A file whose `session_schema` is higher than 6 (or not a number) is
   **refused before anything in it changes**.
2. `PRAGMA journal_mode = WAL`; `ols_meta`, `session_user`, `model_object`
   and `oplog` made when missing.
3. Older files only (they have `ols_meta`): the upgrade steps that come
   before the other tables — `oplog.note` added (schema 1, before its
   first release); schema 2 → 3 (`lines::migrate`): `model_object` copied
   into a new table with the `line` column (every row `main`), `oplog.line`
   added (`main`), an old `spatial_index` without `line` dropped (it is
   built again); schema 6 (`lines::migrate_tombstones`):
   `model_object.gone` added (0 for every row).
4. `spatial_index`, `surface`, the `vc_*` tables and the edit grant and
   review tables made when missing; older files: `vc_ref.description`
   (within schema 3) and `vc_ref.base` (schema 6) added
   (`lines::migrate_descriptions`).
5. `grid_size` written when missing (256 m, `Grid::DEFAULT_SIZE`; fixed
   for the project from then on).
6. `session_schema` written (a new file) or raised to 6 (an older one).
   From then on **an older program refuses the file**, whatever was done
   in it.
7. `versions::init`: the branch `main` and `vc_head` = `branch:main` when
   missing.
8. Schema 4 (`migrate_library_files`): the rows of the old
   `library_file` table copied as `library` objects into every project
   line, the table dropped.
9. The objects read; projects from before routes and alignment points
   are converted once (rows rewritten in place, not logged; the keys
   `routes_alignments` and `alignment_points` say it is done); frozen
   lines of schema 2 (`line_snapshot`) made editable lines, the table
   dropped; branches of schema 2 without a working state get their
   revision as base.

Each step looks for what it changes (a column, a table), so a file of an
in-between build is upgraded too and a step already done does nothing.
Upgrading rewrites no object of a line: schema 5 and 6 add nothing to
convert. `ohl_pointcloud::Store::open` refuses a `pointcloud_schema`
above 2; a file of version 1 is read as it is and gets its levels of
detail with `ohl pointcloud octree` (the version is then raised to 2).

#### The tables, column by column

As made by the code (types as SQLite takes them; `time` columns are UTC
text, `strftime('%Y-%m-%dT%H:%M:%fZ')`, the point-cloud and background
ones `datetime('now')`):

```sql
-- ohl-session (lib.rs, lines.rs)
ols_meta      (key TEXT PRIMARY KEY, value TEXT NOT NULL)
session_user  (name TEXT PRIMARY KEY, role TEXT NOT NULL)            -- viewer | designer | admin
model_object  (line TEXT NOT NULL DEFAULT 'main', kind TEXT NOT NULL, id TEXT NOT NULL,
               ord INTEGER NOT NULL, version INTEGER NOT NULL, json TEXT NOT NULL,
               gone INTEGER NOT NULL DEFAULT 0,                      -- 0 object, 1 tombstone, 2 place
               PRIMARY KEY (line, kind, id))
oplog         (seq INTEGER PRIMARY KEY, time TEXT NOT NULL, user TEXT NOT NULL,
               kind TEXT NOT NULL, command TEXT NOT NULL, inverse TEXT NOT NULL,
               objects TEXT NOT NULL, sections TEXT NOT NULL,
               undoes INTEGER, undone_by INTEGER, note TEXT,
               line TEXT NOT NULL DEFAULT 'main')
-- spatial.rs
spatial_index (line TEXT NOT NULL DEFAULT 'main', kind TEXT NOT NULL, id TEXT NOT NULL,
               level INTEGER NOT NULL, tx INTEGER NOT NULL, ty INTEGER NOT NULL,
               xmin REAL NOT NULL, ymin REAL NOT NULL, xmax REAL NOT NULL, ymax REAL NOT NULL,
               PRIMARY KEY (line, kind, id))
INDEX spatial_tile ON spatial_index (line, level, tx, ty)
-- surfaces.rs
surface       (name TEXT PRIMARY KEY, time TEXT NOT NULL, user TEXT NOT NULL, source TEXT NOT NULL,
               points INTEGER NOT NULL, triangles INTEGER NOT NULL,
               bounds TEXT NOT NULL, data BLOB NOT NULL)
-- versions.rs
vc_blob       (hash TEXT PRIMARY KEY, data BLOB NOT NULL)
vc_revision   (id TEXT PRIMARY KEY, parents TEXT NOT NULL, author TEXT NOT NULL, time TEXT NOT NULL,
               message TEXT NOT NULL, tree TEXT NOT NULL, seq INTEGER NOT NULL)
vc_ref        (name TEXT PRIMARY KEY, kind TEXT NOT NULL, revision TEXT,
               protected INTEGER NOT NULL DEFAULT 0, user TEXT NOT NULL, time TEXT NOT NULL,
               note TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '', base TEXT)
-- collab.rs
edit_grant    (id INTEGER PRIMARY KEY, subject TEXT NOT NULL, kinds TEXT NOT NULL,
               objects TEXT NOT NULL, area TEXT, user TEXT NOT NULL, time TEXT NOT NULL,
               note TEXT NOT NULL DEFAULT '')
vc_review     (id INTEGER PRIMARY KEY, branch TEXT NOT NULL, target TEXT NOT NULL,
               revision TEXT NOT NULL, author TEXT NOT NULL, time TEXT NOT NULL,
               note TEXT NOT NULL, reviewers TEXT NOT NULL, state TEXT NOT NULL)
vc_review_note(seq INTEGER PRIMARY KEY, review INTEGER NOT NULL, user TEXT NOT NULL,
               time TEXT NOT NULL, verdict TEXT NOT NULL, comment TEXT NOT NULL)
-- ohl-pointcloud
pointcloud        (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, crs TEXT NOT NULL DEFAULT '',
                   scale REAL NOT NULL, chunk_size REAL NOT NULL, class_scheme TEXT,
                   points INTEGER NOT NULL DEFAULT 0,
                   xmin REAL, ymin REAL, zmin REAL, xmax REAL, ymax REAL, zmax REAL, created TEXT)
pointcloud_source (cloud INTEGER NOT NULL REFERENCES pointcloud(id) ON DELETE CASCADE,
                   path TEXT NOT NULL, points INTEGER NOT NULL, imported TEXT)
pointcloud_block  (id INTEGER PRIMARY KEY,
                   cloud INTEGER NOT NULL REFERENCES pointcloud(id) ON DELETE CASCADE,
                   cx INTEGER NOT NULL, cy INTEGER NOT NULL, class INTEGER NOT NULL,
                   count INTEGER NOT NULL, z0 INTEGER NOT NULL,
                   xyz BLOB NOT NULL, intensity BLOB, returns BLOB,
                   UNIQUE (cloud, cx, cy, class))
pointcloud_lod    (block INTEGER NOT NULL REFERENCES pointcloud_block(id) ON DELETE CASCADE,
                   level INTEGER NOT NULL, count INTEGER NOT NULL, z1 INTEGER NOT NULL,
                   xyz BLOB NOT NULL, PRIMARY KEY (block, level))
-- ohl-background
background_image  (id INTEGER PRIMARY KEY, layer TEXT NOT NULL, name TEXT NOT NULL,
                   source TEXT NOT NULL, crs TEXT NOT NULL DEFAULT '',
                   x0 REAL NOT NULL, y0 REAL NOT NULL, px REAL NOT NULL, py REAL NOT NULL,
                   width INTEGER NOT NULL, height INTEGER NOT NULL, levels INTEGER NOT NULL,
                   created TEXT, UNIQUE (layer, name))
background_tile   (image INTEGER NOT NULL REFERENCES background_image(id) ON DELETE CASCADE,
                   level INTEGER NOT NULL, tx INTEGER NOT NULL, ty INTEGER NOT NULL,
                   data BLOB NOT NULL, PRIMARY KEY (image, level, tx, ty))
background_layer  (layer TEXT PRIMARY KEY, kind TEXT NOT NULL DEFAULT 'orthophoto')
background_drawing(name TEXT PRIMARY KEY, source TEXT NOT NULL, crs TEXT NOT NULL DEFAULT '',
                   items INTEGER NOT NULL, bounds TEXT NOT NULL, data BLOB NOT NULL, created TEXT)
```

| Table | What it holds | Written by | Read by |
|---|---|---|---|
| `ols_meta` | key–value pairs of the project (below); the table keeps its old name | `Session::open`, `checkout` (`vc_head`), `Session::set_setting`; `ohl_pointcloud` (`pointcloud_schema`) | every open; the applications' settings |
| `session_user` | the users and their roles; the first user added becomes the admin | `Session::set_user` (`ohl project user`; the user of the first import becomes the admin) | every command (`Session::require`) |
| `model_object` | the objects of each project line (below) | every command (`Session::apply_as` → `lines::write_row`), import, `create_line`, `rename_line`, `delete_line`, the upgrades | `Session::open` (all lines at once) |
| `oplog` | every applied command with its inverse, per project line (below) | every command, in the same transaction as its objects | undo, redo, rollback, `ohl project log`, the History window; `MAX(seq)` is the revision's `seq` |
| `spatial_index` | plan bounds and home tile of every object with a place (ADR-0008), per project line | every command (`spatial::write_diff`); a line's rows when it is first worked on | `Session::objects_in` (`ohl project query`, `tiles`), partial loading, the server |
| `surface` | terrain surfaces (TINs) by name; shared by every line, not logged, not undone | `Session::put_surface`, `rename_surface`, `delete_surface` (Terrain menu, `ohl surface import`, `rename`, `delete`; a designer) | the views, the checks (the surface of the checks, a rule's `surface`) |
| `vc_blob` | content-addressed texts of the version control: each object's canonical JSON, each library file, each revision's tree | `commit`, merges, bundle imports (`write_blob`, only when the hash is new) | checkout, diff, merge, lines kept as differences (their base) |
| `vc_revision` | the revisions (commits) | `commit`, `merge`, `revert`, bundle imports | history, diff, merge, `create_line` from a revision |
| `vc_ref` | the branches — the project lines — and tags | `create_line`, `rename_line`, `delete_line`, `set_line_description`, `tag`, protecting, `commit` (moves the branch) | every open (which lines exist), the Project lines window, `ohl project branch` |
| `edit_grant` | limits of a user's or a role's edits (kinds, objects, a plan area) | admins (`ohl project grant`, the Versions window's Rights page) | every command (`check_edit`) |
| `vc_review` | a branch submitted for merging into another | `submit_review`, `decide_review`, `merge_review`; closed when its branch is deleted, renamed with it | the Versions window's Reviews page, `ohl project review` |
| `vc_review_note` | the verdicts and comments of a review, in order | `decide_review` (`approve`, `reject`), `merge_review` (`merge`) | the same |
| `pointcloud` … `pointcloud_lod` | the point clouds (ADR-0007), shared by every line | `ohl pointcloud import`, `octree`, the app's import, `change_points` | the views, the LiDAR checks, `ohl pointcloud export` |
| `background_*` | orthophotos as JPEG tile pyramids, DXF drawings; shared by every line, not logged | Terrain › Backgrounds (and moving the project into another coordinate system) | the views, the sheets (`[plan_background]`, `[key_map]`) |

**Columns in words.**

- `model_object`: `line` the project line (= `vc_ref.name` of a branch);
  `kind` one of `meta`, `route`, `alignment`, `alignment_point`,
  `structure`, `section`, `jumper`, `spacer`, `report`, `library`,
  `annotation` (`ohl_session::Kind`; `point_cloud` is a kind of the log only, it has
  no row); `id` the object's id (`line` for the line's own settings row,
  `kind/name` for a library file); `ord` its place among the objects of
  its kind (the line file's order); `version` counts its changes (1 when
  made) and lets undo see another user's change; `json` the object as in
  the line file; `gone` 0 an object, 1 a **tombstone** (an object of the
  line's base deleted on it: `json` `null`, `ord` and `version` 0), 2 a
  **place** (the base's object unchanged, at another `ord`: `json`
  `null`).
- `oplog`: `seq` the entry's number, in the order applied, over all
  lines; `kind` `do` (a command), `undo`, `redo`, `rollback`, `import`
  (a line imported: cannot be undone), `library` (what the program does
  on its own with the library files: logged, not undone by Undo),
  `reproject` (the project moved into another coordinate system: cannot
  be undone); `command` the command as applied (JSON, a `batch` of the
  primitive puts and deletes it expanded into), `inverse` the command that
  takes it back; `objects` `[[kind, id, version after], …]` (0: deleted);
  `sections` the section ids to solve again; `undoes` the entry an undo,
  redo or rollback reverses, `undone_by` the entry that reversed this one;
  `note` a summary in words of what it did (what a confirmed deletion
  took with it, renames, links), or none.
- `spatial_index`: `level`, `tx`, `ty` the home tile of the project's
  chunk grid (tiles of `grid_size` × 2^level m; `ohl_core::grid`);
  `xmin` … `ymax` the plan bounds, m, project coordinates.
- `surface`: `bounds` JSON `[[xmin, ymin, zmin], [xmax, ymax, zmax]]` (or
  `null`); `data` zstd of the point and triangle counts (u64 each), the
  points (3 × f64) and the triangles (3 × u32), little-endian.
- `vc_blob`: `hash` SHA-256 (hex) of the text; `data` the text compressed
  with zstd.
- `vc_revision`: `id` SHA-256 of the canonical JSON of tree, parents,
  author and message; `parents` a JSON list of ids (none: the first
  revision, two: a merge); `tree` the hash of the tree's blob —
  `{"objects": [[kind, id, ord, hash], …], "libraries": [[kind, name,
  hash], …]}`; `seq` the last `oplog` entry when it was made.
- `vc_ref`: `kind` `branch` or `tag`; `revision` the branch's last
  revision or the tag's (null: nothing committed yet); `protected` 1:
  commits and merges by admins or approved reviews only; `user` who made
  it (empty: the project's first line); `base` (schema 6) the revision a
  line keeps its differences from (null: the line keeps all its objects).
- `edit_grant`: `subject` a user's name or `role:designer`; `kinds` and
  `objects` JSON lists (empty: no limit of that kind); `area` a JSON
  polygon `[[x, y], …]` or null; `user` the admin who gave it.
- `vc_review`: `branch` submitted into `target` at `revision`;
  `reviewers` a JSON list of names; `state` `open`, `approved`,
  `rejected`, `merged` or `closed`.
- `pointcloud`: `scale` the coordinates' unit, m (1 mm by default);
  `chunk_size` the blocks' square, m (a whole number of `scale` units);
  `class_scheme` the `.ohlc` text given at import. `pointcloud_block`: one
  block per chunk (`cx`, `cy`) and class; `z0` its lowest point (scale
  units); `xyz`, `intensity`, `returns` columns encoded by
  `ohl_pointcloud::codec` (integers relative to the chunk, byte planes
  shuffled, zstd). `pointcloud_lod`: the block's points of levels 1 to 3
  (about a 4th, 16th, 64th) and `z1`, its highest point.
- `background_layer` (2026-10-25, roadmap O16): an image layer's kind,
  `orthophoto` or `map` (`ohl_background::LayerKind`); a layer without a
  row is an orthophoto. The route and situation plans and the sheets' key
  map take the maps first (`Store::map_layers`; every layer when there is
  none).
- `background_image`: `x0`, `y0` the image's top-left corner, `px`, `py`
  the pixel size, m; `levels` the pyramid's levels; `background_tile`:
  JPEG tiles of 256 pixels (blank tiles not kept). `background_drawing`:
  `data` zstd of the drawing's JSON (`ohl_background::Drawing`), `bounds`
  JSON as for `surface`.

#### `ols_meta` keys

| Key | Value |
|---|---|
| `session_schema` | `6` (above) |
| `pointcloud_schema` | `2` (when a point cloud store was opened) |
| `grid_size` | the chunk grid's tile size at level 0, m (default 256) |
| `vc_head` | `branch:NAME`: the project line the file opens on (`ohl project checkout`) |
| `routes_alignments`, `alignment_points` | `1`: the one-off conversion to routes, alignments and alignment points is done |
| `setting.<key>` | the applications' settings (below) |

#### The applications' settings (`setting.<key>`)

Kept in `ols_meta` under `setting.<key>` (`Session::setting`,
`Session::set_setting`): not part of the line, not logged, not undone,
not in revisions; on a server `Op::SetSetting`, a designer's whose
edits no grant limits, an admin's on a protected project line, never a
value that is an absolute path (since 2026-10-26; the others follow at
once, the server logs who changed which key).

| Key | Written by | Value |
|---|---|---|
| `app.weather` | `ohl-design` (the weather case file chosen) | the file's name when it is one of the project's own files, else its path; read by `ohl zones` too |
| `app.own-libraries` | `ohl-design`, when a new project is made (File › New project) | `1`: nothing is copied in from files next to the project |
| `app.check_surface` | `ohl-design` (Terrain menu: the surface of the checks) | a surface's name: the ground every user's checks use; read by `ohl zones` and by the engineering impact of `ohl project` diffs and merges |
| `app.drawings.<project line>` | `ohl-design` (Plans & sheets › Profile sheets…) | JSON, below |
| `app.plan_sheets.<route\|situation\|phase>.<project line>` | `ohl-design` (Plans & sheets › Route plan…, Situation plans…, Phase plan…) | JSON (2026-11-08): `{template, fields}` — the plan's own sheet template key (as `app.drawings`' `sheet.template`) and the values of its fields and revisions (`ohl_drawing::SheetFields`) (`plans_window::PlanSheet`), and since 2026-11-11 `name` (the name the plan's files take; left out when empty); none kept: the sheets' template when first read, as the plans took before |
| `app.phase_plans.<project line>` | `ohl-design` (Plans & sheets › Phase plan…) | JSON (2026-11-03): `[{name, alignments: [alignment id…]}]`, the alignments drawn together in one phase plan (`drawings::PhaseGroup`, `PlanRequest::groups`, `ohl_drawing::plans::phase_plan_of`); an alignment in none its own plan |
| `app.magnetic_field.<project line>` | `ohl-design` (Checks › Electrical › Magnetic field…) | JSON: the window's inputs (2026-10-27) — `case` (name), `currents` (circuit label → `[A, reverse]`), `earth_wires`, `height`, `width` (m; absent: 100, 50 until 2026-11-02), `step` (m), `span` (`[from, to, fraction or null]`; absent: the spans ticked), `spans_off` / `spans_on` (2026-10-30: `[[from, to]]`, the spans on an alignment unticked and those on none ticked, the changes from the default), `limit` (µT), `two_d`, `mean_heights`, `points` (`[file name, CSV text]`); since 2026-11-11 `phases` (circuit label → `[[phase label, A, degrees]]`, the circuits given per phase) and `earth_typed` (earth wire section id → `[A, degrees]`), each written only when not empty; since 2026-10-29 `ends` (circuit label → `[[structure, A or null]]`), `towers`, `jumpers`, `piece_length`, `modelled_within` (m; read when `modelled` is absent), `modelled` (2026-11-01: the cut-off, m, 0 the whole line; a new project 2 000), `levels` (`[{value µT, rgb, on}]`), `fill`, `above` (`[[from, to] or null, step]`; since 2026-10-30 written `[null, step]`, over the spans ticked), `in_cuts` (step, m), `volume` (`[[from, to], [from, to], step]`; since 2026-10-30 only its step read, the spans the ticked ones), `volume_step` (m, 2026-10-30), `volume_width` (the volumes' half width, m, 2026-11-01; absent: `width`), `calculation` (2026-11-01: the wires' calculation by its file name; absent: each section's own), `planes` (2026-11-01: 2D, the cuts and profiles; false: 3D, the volumes; absent: 2D), `profiles` (2D: the profiles too; absent: yes), `per_system` (2026-11-01: system id, `""` the earthed wires → `[case name or null, calculation or null]`; null: the window's; `above` and `above_spans` no longer used), `ground_from` (2026-11-01: `"checks"`, `{"surface": name}` or `{"survey": m}`; absent: the checks'); `beside` (profiles beside the structures, m; 2026-10-31); since 2026-10-31 (the span table) `cuts` (`[[[from, to], [station…]]]`, a station `"mid"`, `"lowest"` or `{"from": m}` from the span's first structure), `volume_spans` and `above_spans` (`[[from, to]]`: the spans with a volume, with isolines from above; `volume` and `above` keep only their steps), `cut_width`, `cut_step`, `cut_above` (m; `cut_width` absent: 100, 60 until 2026-11-02); `buildings` (2026-11-03, the electric field's too: the preset handed over by Buildings (fields), `ohl_app::field_preset::Preset` — `plugin`, `name`, `cuts` `[{label, alignment, station, width, above, step}]`, `places` `[{name, x, y, z, height}]`, `levels_ut`, `levels_kv_m`; run with the window's own in 2D); missing fields take the defaults |
| `app.electric_field.<project line>` | `ohl-design` (Checks › Electrical › Electric field…) | JSON: the window's inputs (2026-10-29) — `case` (name), `voltages` (circuit label → kV between phases; absent: the system's nominal; 0: voltage-free), `jumpers`, `height`, `width`, `step` (m), `span` (`[from, to, fraction or null]`; absent: the spans ticked), `spans_off` / `spans_on` (2026-10-30: `[[from, to]]`, the spans on an alignment unticked and those on none ticked, the changes from the default), `limit` (kV/m), `two_d`, `ground_radius` (m), `tiled` (the tiled floor, default; else the plane), `tile` (its fine tiles, m), `cut_tile`, `volume_tile` (m), `volume_margin` (m, 2026-11-01: how far a volume's floor reaches; absent: 100), `points` (`[file name, CSV text]`), `piece_length` (m), `levels` (`[{value kV/m, rgb, on}]`), `fill`, `in_cuts` (step, m), `volume` (`[[from, to], [from, to], step]`; since 2026-10-30 only its step read, the spans the ticked ones), `volume_step` (m, 2026-10-30), `volume_width` (m, 2026-11-01; absent: `width`), `calculation`, `planes`, `profiles`, `per_system`, `ground_from` (2026-11-01, as the magnetic field's), `stretches` (3D: the stretches per span, 2026-11-01; absent: 24), `steel` (the structures' steel earthed, 2026-10-31), `beside` (profiles beside the structures: the distance into the spans, m; 2026-10-31), `cuts`, `volume_spans`, `cut_width`, `cut_step`, `cut_above` (the span table, 2026-10-31: as the magnetic field's); written when a run starts or the window closes; missing fields take the defaults |
| `app.galloping.<project line>`, `app.wirefit.<project line>` | `ohl-design` (Checks › Galloping ellipses…, As-built wires…) | JSON (2026-10-28): the window's scope (`ohl_app::check_ui::Scope`: `circuits`, `routes`, `alignments`, `sections`, as in `app.vegetation`), Galloping's with `local` (bool, 2026-11-03: Use local conditions) beside them; missing or unreadable: the whole line |
| `app.vegetation.<project line>` | `ohl-design` (Checks › Vegetation…) | JSON: the panel's setup (2026-10-27) — `settings` (as a saved run's settings: the same keys as the performed check's `settings`, its `cases` the weather cases ticked, by name), `scope` (`circuits`: names, absent or `null` every circuit; `routes`, `alignments`: ids, both empty the whole line; `sections`: ids, `null` every section — `ohl_app::check_ui::Scope`), `local` (bool, 2026-11-03: Use local conditions; the old `overrides` read and ignored); missing or unreadable fields keep the panel's values |
| `app.safety_volumes.<project line>` | `ohl-design` (Checks › Safety volumes…) | JSON: `settings` (as a saved run's settings, its `cases` by name) (2026-10-27), `local` (bool, 2026-11-03: Use local conditions; the old `overrides` ignored) |
| `app.phase_distance.<project line>`, `app.unequal_wind.<project line>` | `ohl-design` (a check plugin's Distance between conductors…, and under unequal wind) | JSON (2026-10-27): `cases` (names), `circuits` (names, `null` every circuit), `scope` (2026-10-28, `ohl_app::check_ui::Scope`: the circuits and the routes and alignments; a file without it takes the circuits), `spacing`, `nearby`, `max_distance` (m), `choices` (the plugin's choices, indices), `wind_step` (Pa), `low_share`, `calculation` (2026-11-01), `local` (bool, 2026-11-03: Use local conditions; the old `overrides` ignored); missing fields keep the window's values |
| `app.surfaces`, `app.lidar_percent` | earlier builds only | read when the user has no choice of their own yet (the ground and the surfaces shown; the share of the point cloud near the camera, %) |

Since 2026-11-10 `sheet` also holds `profile_lines` (`corridor`, `distance`, `lowest_arm`; none: the template's), `profile_distance` (m; none: the template's, else 10) and `profile_circuits` (circuit names; none: the template's, else every circuit with a system) — each left out when none, so older settings read and write the same. Since 2026-11-11 `sheet` also holds `name` (the name the files take in place of the route's; left out when empty); `ohl_drawing::export::Export` has `name` for it and `export::named` composes it with a route's or plan's own name when several are written.

`app.drawings.<project line>`: the drawing export settings of that project line as JSON (`ohl_app::drawings::DrawingSettings`: `formats` (names), `page` ("" or A0–A4; since 2026-10-22 any standard paper size of `ohl_drawing::paper::PAPER_SIZES`, `WIDTHxHEIGHT`, or `"optimised"`), `paper_sizes` (the sizes ticked, names; 2026-10-22), `mixed_sizes`, `same_paper`, `paper_orientation` (`both`, `landscape`, `portrait`), `hscale`, `vscale`, `range`, `from`, `to`, `profile_only`, `clearance`, `all_cases`, `dpi`, `line_width` (m, also the Profile page's), `break_clearance` (mm), `elevations` ("off", "above" or "below"), `elevation_offset` (mm on paper), `elevation_rotation` (degrees), `preview`; since 2026-10-20 `side_offsets` (m), `plan_scale`, `logos` (`[{file, width}]`), `clearance_rule` (the clearance line from the ground rule), `markers`, `page` may be `"template"`, and `sheet` (`ohl_app::sheet_ui::SheetChoice`: `template` (choice key), `portrait`, `fields` (`values`, `per_sheet`, `revisions`), `versions` (`[{line, role, label}]`), `role`, `corridor`, `conductor_clearance`, `bands`, `elevation_scale`, `elevation_labels`, since 2026-10-22 `elevation_stick` (none: the template's) — labels `dimensions` and `arm_heights` too; since 2026-10-23 `circuit_cases`: per circuit key — a system's name, "" for the earth wires — `{drawn, stringing}`, each a case's name or none; `lidar_classes` (codes, comma separated; none: the template's), `background_drawings`, `background_photo` (the drawings and orthophotos under the plan; 2026-10-23), `key_map_drawings`, `key_map_photo` (under the key map; 2026-10-23) — each on, off or none: the template's); missing fields and values out of range take the defaults; absent: the defaults).

#### The objects of a project line (`model_object`)

One row per object of a project line — or, for a line kept as differences (schema 6, `vc_ref.base` set), per object it changed or added since its base revision, a **tombstone** (`gone` = 1, `json` `null`) per object of the base it deleted, and a **place** (`gone` = 2, `json` `null`: the base's object unchanged, at `ord`) per object of the base it holds at another place ([below](#project-lines-kept-as-differences-schema-6)); `kind` is `meta`, `route`, `alignment`, `alignment_point`, `structure`, `section`, `jumper`, `spacer`, `report` (`ohl_session::Kind`) — routes, alignments and alignment points are three kinds of row; span points, spreads and jumper points are inside their section's / jumper's row; `report`: a performed check (below), kept with the line, not part of it; `json` as in the line file; the line's settings are `meta` rows: `line` (`name`, `crs`, `height_datum` (schema 5, left out when none), `display`; projects before routes and alignments also an `alignment`, converted once when they open) and one row per setting — `systems`, `circuits` (2026-10-21), `creep_stretch`, `bimetallic`, `solver`, `defaults`, `circuit_starts`, `structure_loads` (`{"solver": …}`; `structure_loads` solves nothing again) — so two users changing different settings keep both and undo their own; a change puts the rows that differ from the line it was made on (`line_settings_change`). Projects written before kept every setting in `line`: read with the rows over it, until a change writes the rows; spacers are in the spatial index (where their ends are), jumpers with their taps; `replace_line` keeps the replacing line's order.

#### Library files (`library` objects)

`model_object` rows of kind `library`, id `kind/name`, JSON `{"id",
"kind", "name", "text"}`: the project line's library files: `kind` `cables`, `structures`, `insulators`, `weather`, `loadcases`, `objects` (`project.ohlload`, the project's point and spread load types), `phase-distance` (an admin's copy of a `.ohlpd`), `rules` (`.ohlr`), `galloping` (`.ohlg`), `ambient` (`.ohlta`), `costs` (`.ohlcost`), `poles` (`.ohlp`), `femodels` (`.ohlfe`), `sections` (`.ohlsec`, the cross-section library), `bolts` (`.ohlb`, the bolt libraries; 2026-11-02), `field-plans` (`.ohlfp`, the [field plan templates](#field-plan-template); 2026-11-02; in `field-plans/` of the working copies; also the Buildings (fields) window's setup: project setting `app.building_fields.<line>` — JSON `plugin`, `preset`, `parameters`, `plan`, since 2026-11-03 `building_classes` / `ground_classes` (point cloud classes ticked; absent: the plugin's roles), `building_codes` / `ground_codes` (survey codes ticked); the keys of 2026-11-02 `currents`, `earth_wires`, `towers`, `ground`, `out` are no longer read), `sheet-templates` (`.ohlsheet`, the drawings' sheet templates with their images embedded; 2026-10-20), `plugin-preset-edits` (`<plugin>.toml`, format `ohl-plugin-preset-edits` version 1: `plugin`, `[[edit]]` `item` — `rules:<preset>`, `weather:<preset>`, `sheet:<template>` —, `path` (keys; an array-of-tables element `name=<its name>`, else `#k`), `original` and `value` (TOML values: the plugin's when changed, the project's), `note?`: the project's values of a plugin's presets, applied when the presets and the plugin's sheet templates are read, `crate::preset_edits` in the app; 2026-10-25), `loads` (`*.loads.csv`: values typed over a structure's load table in the structure application, per structure type and per placed structure, [below](#structure-load-cases-load_case-ohlslc)); each project line has its own (a new line starts with those of the line it is made from); changed as logged commands (`put_library_file`, `delete_library_file`: undone as any change; `keep_library_files`: what the program does on its own — files next to the project copied in when it opens, an older form converted — logged as `library`, not undone by Undo); by the "library" edit grant; revisions keep them apart (`State::libraries`). Schema 3 kept them in a `library_file(kind, name, time, user, text)` table shared by all lines: copied into every line when the project opens, the table dropped (schema 2 kept frozen lines in `line_snapshot`: made lines, the table dropped; new files have neither table). When the project opens, the app copies in the kinds it lacks from next to the project (never the shipped libraries; `poles`, `femodels`, `loads` from its structure folder; `sections` from the `Sections/` folder next to it, else the files next to it; `bolts` from `Bolts/` in its structure folder) and writes each line's files to a working folder (`poles`, `femodels`, `loads` next to the `structures`; `bolts` in `structures/Bolts/`, where `BoltLibrary::beside` finds them for OHL-Structure and the structure checks — `ohl_app::model::library_folder`, the CLI's `PROJECT_LIBRARY_KINDS` alike; `sections` in its own folder), which it reads from then on; File › Export › Libraries writes the same folders (bolts in `structures/Bolts/`). Bolt libraries imported in OHL-Structure into `Bolts/` beside a working structure file are stored back (`structure_app::LIBRARY_EXTENSIONS` has `ohlb`; that folder is watched).

#### How project lines, revisions and the log fit together

- A **project line** is a branch: a `vc_ref` row of kind `branch`. Its
  working state is its `model_object` rows (with `line` = its name), laid
  over its base revision when `vc_ref.base` is set; its `spatial_index`
  rows carry the same `line`. `main` is always there.
- Every **command** writes, in one transaction, the objects it changes
  (`model_object`), one `oplog` row (with `line` = the line worked on)
  and the spatial index's changes. Nothing waits to be saved: the file
  is always up to date. Undo and redo find the user's last entry on that
  line (`undone_by` null) and apply its `inverse` as a new entry
  (`undoes` pointing back).
- A **revision** (`vc_revision`) is a frozen state of a line: its tree
  lists every object and library file by the SHA-256 of its text, the
  texts are in `vc_blob` (each once, however many revisions hold it).
  `commit` writes the blobs that are new, the tree, the revision, and
  moves the branch's `vc_ref.revision`; the revision's `seq` names the
  last log entry then. The log itself and the point clouds, surfaces,
  backgrounds, settings and users are not under version control.
- A **line kept as differences** (`vc_ref.base`) is read as its base
  revision's state with its rows over it ([below](#project-lines-kept-as-differences-schema-6)).
- A **tag** is a `vc_ref` row of kind `tag` naming a revision; a
  **review** (`vc_review`) names a branch, its target and the revision
  submitted.
- `vc_head` in `ols_meta` is the line the file opens on.

### Project lines kept as differences (schema 6)

A project line started since schema 6 (`Session::create_line`,
`ohl_session::lines`) records the revision it starts from as its
**base** (`vc_ref.base`: the revision recorded on the line it is made
from, or the revision or tag it is made from) and keeps in
`model_object` only what differs from it: the objects it changes or adds
(with their place and version) and a tombstone for each object of the
base it deletes. Starting a line writes no object row and no spatial
index row; only the library files the line it comes from holds in
another order than a revision places them (by kind and name) get a
**place** row each (`gone` = 2, no JSON: the base's file at its own
place; on a real project 401 such rows, 1.6 kB, where whole copies took
23 MB).

- **Reading a line**: the base revision's objects (version 1) and
  library files (`library` objects placed by kind and name), the line's
  rows laid over them, its places applied, its tombstones taken out. A line without a base
  — the main line, every line of a project of schema 5 or before — keeps
  all its objects and is read as before; nothing is rewritten when an
  older project opens (only `model_object.gone` and `vc_ref.base` are
  added). Branches of schema 2 without a working state of their own get
  their revision as base (with tombstones for its library files: such
  branches always started without them).
- **Writing**: every command writes the objects it changes as before;
  an object deleted from a line with a base leaves a tombstone (from a
  line without one, its row is deleted). Undo, redo, rollback, checkout,
  merge, bring-in, revert, roll-back and bundle imports are commands, so
  they write the same way. The overlay is bounded by the objects the
  line ever changed; it is not compacted onto a newer revision (that
  would reset the versions of the objects it holds).
- **The base is immutable**: revisions are never deleted, so deleting
  or renaming the line a line came from changes nothing of it; renaming
  a line moves its rows, deleting it deletes them.
- **Versions**: objects read from the base are at version 1, so a new
  line's objects start again at version 1 (they kept the source line's
  before); conflicts of undo are judged by content, not by version.
- **Lines made from a revision** (or a bundle's head) start with that
  revision's line **and library files** (owner, 2026-10-23; they started
  without the library files before). A line's library files are read by
  `Session::line_library_files` (the current line's by
  `Session::library_files`); the app reads and solves a project line
  shown with its own where they differ (`Model::line_libraries`).
- **Memory**: an object's JSON is held once (`Arc`) by the lines that
  hold it alike (a line started shares its source's; lines read from the
  file are matched object by object, bit for bit).
- An older program refuses a project of schema 6 (it would read a line
  kept as differences as nearly empty).

### Inspecting a project file with `sqlite3`

The project file is an ordinary SQLite database, so the `sqlite3` shell
reads it. Open it **read-only** (`-readonly`), preferably on a copy or
with the programs closed: a row changed by hand bypasses the session's
checks, the log and undo, and can leave a line the program refuses. Every
change goes through a command (`ohl project apply`).

```bash
sqlite3 -readonly project.ohl
```

```sql
.tables
-- The schema numbers, the grid, the line the file opens on, the settings.
SELECT key, value FROM ols_meta ORDER BY key;
-- The project lines (branches) and tags, their last revision and base.
SELECT name, kind, substr(revision, 1, 10) AS revision, protected, user,
       substr(base, 1, 10) AS base
FROM vc_ref ORDER BY kind, name;
-- Rows per line and kind: objects, tombstones (gone = 1), places (gone = 2).
SELECT line, kind, COUNT(*) AS rows, SUM(gone = 1) AS tombstones, SUM(gone = 2) AS places
FROM model_object GROUP BY line, kind;
-- One object's JSON (fields with json_extract).
SELECT version, json_extract(json, '$.position') FROM model_object
WHERE line = 'main' AND kind = 'structure' AND id = 'T2';
-- What a line kept as differences changed against its base revision.
SELECT o.line, o.kind, o.id, o.version,
       CASE o.gone WHEN 0 THEN 'changed or added' WHEN 1 THEN 'deleted' ELSE 'other place' END AS what
FROM model_object o JOIN vc_ref r ON r.name = o.line AND r.kind = 'branch'
WHERE r.base IS NOT NULL ORDER BY o.line, o.kind, o.id;
-- The operation log, newest first.
SELECT seq, time, user, kind, line, objects, sections, undoes, undone_by, note
FROM oplog ORDER BY seq DESC LIMIT 20;
-- The objects of one log entry, one per row.
SELECT value FROM json_each((SELECT objects FROM oplog WHERE seq = 2));
-- The revisions, newest first.
SELECT substr(id, 1, 10) AS id, parents, author, time, message, seq
FROM vc_revision ORDER BY time DESC;
-- Library files of each line (a line kept as differences: only those it changed).
SELECT line, json_extract(json, '$.kind') AS kind, json_extract(json, '$.name') AS name,
       length(json_extract(json, '$.text')) AS chars
FROM model_object WHERE kind = 'library' AND gone = 0;
-- Objects of a line whose plan bounds touch an area (the spatial index).
SELECT kind, id, level, tx, ty FROM spatial_index
WHERE line = 'main' AND xmax >= 0 AND xmin <= 500 AND ymax >= -50 AND ymin <= 50;
-- Surfaces, point clouds, backgrounds (the point-cloud and background tables
-- exist once their store has opened the file).
SELECT name, user, source, points, triangles FROM surface;
SELECT name, crs, scale, chunk_size, points FROM pointcloud;
SELECT layer, name, width, height, levels FROM background_image;
SELECT name, source, items FROM background_drawing;
```

What `sqlite3` alone cannot show: the blobs (`vc_blob.data`, `surface.data`,
the point blocks, the drawings) are zstd-compressed, and a line kept as
differences has only its changes in `model_object` — its whole state is
the base revision's (in `vc_blob`) with them laid over it. For those use
the programs: `ohl project checkout` and `ohl project export` (a line's
whole state as a line file), `ohl project history`, `diff` and `log`,
`ohl surface export`, `ohl pointcloud info` and `export`.

### Annotations (`ohl_session::annotations`, 2026-10-25)

Measurements kept and notes: a `model_object` of kind `annotation` on the
project line, **apart from the line** like a report — not in
`ohl_line::Line`, its hash, the line file or the line state the checks
compare (`check_state` leaves them out), kept by `replace_line`, not
limited by restricted edit grants; **shared** (every user's, each with
its author); put and deleted by the ordinary `put` / `delete` commands
(logged, undoable). **Deleted with what they are attached to**: a command
that leaves an anchor without its object (a structure, section, span,
jumper, wire point, span point or alignment point deleted) deletes the
annotation in the same log entry, so one undo brings both back.

`Annotation`: `id` (unique on the line; the app writes `user-n`), `kind`
(`dimension`, `leader`, `note`), `tool` (a dimension's measurement:
`distance`, `height`, `stations`, `wire_to_wire`, `clearance`, `wire_to_object`, `angle` — three anchors, the vertex second, 2026-11-04), `anchors` (a dimension: as many as its
measurement takes; a note: one), `text`, `author`, `time` (UTC), `last`
(`value`, `unit`, `case`: the value when kept, for a reader without the
solved line). An `Anchor` is tagged by `on`: `structure` (`structure`:
its base), `on_structure` (`structure`, `offset` from its base: E, N, H —
a point of its members; it follows the structure moved, not turned),
`attachment` (`structure`, `set`), `wire` (`section`, `span`
from 0, `fraction` of the wire's length 0–1, in the case measured),
`jumper` (`jumper`, `fraction`), `span_point` (`section`, `label`),
`alignment_point` (`id`), `lidar` (`at` E, N, H, `class`), `point` (`at`,
`what`). Checked when put: it reads, the id matches, the anchors are as
many as its kind takes, fractions within 0–1. Unknown fields refused.

### Performed checks (`ohl_session::reports`)

A check's saved run: a `model_object` of kind `report` on the project
line it was made on, so it belongs to that line's branch (commits,
checkout, merges and bundles carry it like any object; saving, a status,
a note, a rename or a deletion is a logged, undoable command). Not part
of `ohl_line::Line`: the line file and the views never see it, and a line
replaced (`replace_line`) keeps it. Restricted edit grants do not limit
it. Exported alone as `.ohlcr` (JSON).

`CheckReport` (`format` `ohl-check-report`, `version` 1): `id` (its name,
unique on the line), `check` (e.g. `phase-distance`), `title`, `time`
(UTC), `user`, `app` (and version), `plugin` (name and version, when one
took part), `status` (`draft`, `checked`, `approved`), `note`, `state`
(the line state: `line`, `seq` — its last log entry — and `objects`, a
SHA-256 per object `kind:id` and project library file
`library:kind/name`, compared with now to say what changed since),
`summary`, `parameters` and `results` (check-specific JSON). The
distance between conductors keeps as parameters the plugin's name and
version, the values file's name, text and SHA-256, the plugin's
choices, spacing, nearby pairs, the circuits, the weather cases with
their values and each circuit's plugin-column values; as results the
counts and the pairs below the required distance or within 1 m of it.
The distance under unequal wind (`check` `unequal-wind`, 2026-10-24)
keeps the same, with one weather case, and in the parameters
`wind_step_pa` and `low_share` (the other conductor's least share of
the wind) — both checks since 2026-11-01 also `calculation` when the
window's *Wires by* is set (as the vegetation's); each pair adds `pressure` (the wind on each conductor, Pa)
and `side` (`left` / `right`) and `wires` (the two conductors' points
as measured, each at its wind, to draw the pair), its sag, l_k and
length below 0, its `k` none (written `[null, null]`: serde_json writes
a NaN so). A pair's numbers `null` are read as none (NaN; `sag`, `lk`,
`beta`, `k`, `c` — 2026-11-04: before, such a report was refused when
opened again, so unequal wind reports could not be reopened). A run
again is compared with the saved pairs by case, the two spans and the
two conductors (circuit and phase; before 2026-11-04 by case and spans
only, which merged a span's phase pairs).

The vegetation check (`check` `ohl.vegetation`, title "Vegetation and
danger trees", 2026-10-20; `ohl-app` `vegetation::saved`) keeps as
`parameters`: `version` (1), the settings — `classes`, `cases` (weather
case names), `clearance` and `fall_allowance` (as a rule file writes a
distance: `"5 m"`, `{ by_voltage = … }`, `{ base = … }`),
`growth_per_year`, `years`, `buffer`, `fall_in`, `root_ball`,
`fall_through_ground` (since 2026-11-05; missing: false),
`min_tree_height`, `max_tree_height`, `max_base_ground_distance` (null:
no limit), `search_distance`, `swing_arc`, `band_margin` (m, since
2026-10-21; absent in older runs: 0.15 m), `jumper_band_margin` (m,
since 2026-10-21, kept from the shared wind choice: the check takes no
jumpers), `loads_per` (2026-10-21: the line's calculation option the run
used; absent in older runs), `wind_steps`, `wind_step`
(Pa), `wind_sides`, `unbalanced_ice` (`off`, `case`, `thickness`,
`extra`, `load`) and `unbalanced_ice_value`, `keep`, `calculation`
(2026-11-01: the wires' calculation by its file name; absent: each
section's own), `isoclearance`
(`interval`, `max_level`, `spacing`; null: none) — and what the run read:
`points_state` (`Session::points_state`: the last log entry that changed
points and each cloud's name, count and creation time) and `ground` (the
ground's source and the surface of the checks). Its `results`: `version`
(1), `points` (vegetation points read), `positions`, `notes`, `labels`
(the cases and positions named, e.g. `high wind, left (step) at 300
Pa`), `spans` (`section`, `span`, `required` m, `encroaching`,
`fall_in`, `nearest` {`distance`, `label`}, `worst_fall` {`margin`,
`label`}, `worst` [{`point` [x, y, z], `class`, `distance`, `height`,
`fall_margin`, `margin`, `label`}]), and every point found, packed
(`ohl_session::packing`, `"zstd+base64:…"`): `grow_in` — the count, then
x, y, z in mm (each column as zigzag varints of the difference to the
point before), then each label (varint) — and `fall_in` the same with
the base's z (mm) after z. Since 2026-11-11, only when the run has
questionable points (`results` `version` 2 then, else 1 as before; an
older program refuses a version 2 run instead of showing it without
them; `vegetation::SAVED_VERSION` 2, `BEFORE_QUESTIONABLE_VERSION` 1):
`questionable_points` (how many in all), `questionable_spans`
[[section, span]] and `questionable` [{`point` [x, y, z], `class`,
`place` (index into `questionable_spans`), `distance` m in plan, `why`
(`no_ground_below` / `ground_point_too_far`)}] — in `ohl-app`
`vegetation::Report::questionable`, `questionable_points`,
`questionable_spans` (`QuestionablePoint`). Opened again, it is out of date when the line
state, `points_state` or `ground` differ from now. Isoclearance lines are
not saved (run again to draw them).

The safety volumes (`check` `ohl.safety-volume`, title "Safety
volumes", 2026-10-20; `ohl-app` `safety_volume::saved`) keep as
`parameters`: `version` (1), `cases` (weather case names), `distance` (as
a rule file writes a distance), `margin`, `tolerance` (m), `wind`
(`none`, `steps`, `arc`, `both`), `band_margin` (m, since 2026-10-21;
absent in older runs: 0.15 m), `jumper_band_margin` (m, since
2026-10-21; absent in older runs: their `band_margin`, as their jumpers
took it), `loads_per` (2026-10-21: the line's calculation option the run
used; absent in older runs), `wind_step` (Pa), `wind_sides`, `per`
(`wire`, `circuit`, `system`), `calculation` (2026-11-01, as the
vegetation's), `scope` (`systems`, `circuits` or `spans`,
2026-10-21; absent: circuits), `circuits`, `systems` (ids; since
2026-10-21 `(no system)`, `safety_volume::NO_SYSTEM`, for the wires
without a system) and `spans`
(`SECTION:N`, N from 1) — the ticks of each list
(only the scope's list is used), empty: all. Their `results`: `positions`, `notes`,
`volumes` [{`name`, `circuit`, `phases`, `sections`, `kv` (null:
earthed), `distance`, `margin`, `outer` (the largest conductor outer
radius), `positions`, `arc_error`, `arc_error_median`, `allowance` (how
far the mesh lies outside, measured), `cell`, `level`, `triangles`,
`volume_m3`, `seconds`, `mesh`}] — `mesh` packed (`"zstd+base64:…"`):
the vertex count, x, y, z in mm (each column as zigzag varints of the
difference to the one before), the triangle count, then each triangle's
three corners as such columns; `null` for a run of more than 2 000 000
triangles (run again to see it). Opened again, it is out of date when
the line state differs from now.

The magnetic and electric fields (`check` `ohl.magnetic-field`, title
"Magnetic field", and `ohl.electric-field`, "Electric field",
2026-11-01; `ohl-app` `field_saved`) keep as `parameters`: `version` (1),
`loads_per`, and `inputs` — the window's inputs as kept in the project
(`app.magnetic_field` / `app.electric_field` above). Their `results`:
`case` (the weather case's name), `field` (`ohl_electrical`'s
`MagneticField` / `ElectricField` as serde writes it, the volumes'
surfaces empty) and `meshes` [{`level`, `mesh`}] — each volume's levels
in turn, `mesh` packed as the safety volumes'; `null` for a run of more
than 2 000 000 triangles —; the electric field also `radius` (m) and
`ground_model`. Opened again, it is out of date when the line state or
the calculation option differs from now.

The ice shedding check (`check` `ohl.ice-shedding`, title "Ice
shedding", 2026-11-02; `ohl-app` `ice_shedding`) keeps as `parameters`:
`version` (1), `loads_per`, and `rules` — the rules checked as the text
of a rule file (`RuleSet::to_toml`, `[[ice_shedding_rule]]` tables). Its
`results`: `report` — the `IceSheddingReport` as serde writes it (rows,
jumped spans, notes). Opened again, it is out of date when the line state
or the calculation option differs from now.

### Commands (the only way to change a project)

`ohl_session::Command`, JSON tagged by `op` (snake_case): `put` (`kind`,
`id`, `value` as in the line file, `ord`), `delete` (`kind`, `id`,
`confirm`), `move_structure`, `set_stringing`, `set_wind_factor`
(`section`, `spans` [first, last] from 0, `factor`; 2026-10-25: a put
of the section, 1 removing the factors), `move_alignment_point`,
`insert_alignment_point` (a new point, or an existing `point`: shared),
`remove_alignment_point` (deletes the point: off every alignment),
`tie_alignment_point`, `detach_alignment_point`,
`align_under_structures`, `delete_structure`, `delete_attachment`
(`structure`, `set`), `replace_structure` (`id`; `structure`: the new
one, its `type` and `extension` or its own points, strings and wire
points; `wires`: a typed new structure's wire points as placed,
`{label, set}` — the session does not read the structure library;
`sets`: `{from, to}` old set → new set, `to` left out: removed;
`confirm`), `rename`, `rename_attachment`, `rename_point`, `rename_system`,
`replace_line`, `change_points`, `batch`; (2026-10-21) `link_point`
(`structure`, `label`, `joint`, `offset`, `position`: the attachment
point made the structure's own, linked — also a relink), `unlink_point`
(`structure`, `label`, `position`), `relink_points` (`structure`,
`points`: `StructurePoint`s as they are to be) — each one log entry;
`replace_structure` takes `points` (`{label, joint?, offset?}`: an old
attachment point carried by the new structure linked so, or removed and
listed). The session does not read the structure library: the caller
computes positions and matches (`ohl_line::links`: `placed_joints`,
`rigid_links`, `linked`, `unlinked`, `match_points` — exact, else the
nearest attach joint offered with its offset — `with_points`). Each is expanded into puts and deletes, the whole
line is validated, and the log keeps the command, its inverse, the objects
changed and the sections to recalculate.

`change_points` (`ohl_session::points`) changes points of the project's
point cloud, not objects of the line: `cloud` (its name) and `points`, a
list of `{from, to}`, each a `CloudPoint` exactly as stored — `xyz`
(integers of the cloud's scale: metres / scale, absolute), `class`,
`intensity` (default 0), `returns` (return number × 16 + number of
returns, default 0) and `index` (its place in its block, optional: the
session writes it into the command as applied). `ohl_pointcloud::edit`
removes `from` from the block of its chunk and class (at `index` when it is
there, else the first point equal to it; none: refused), puts `to` into the
block of its chunk and class (at `index` when that keeps the block's order
by x and y, else after the points of the same or a lower x, y; a missing
block is made, an emptied one removed with its levels), keeps the other
points in their order, encodes the blocks again (`Columns::encode_in_order`),
builds their levels of detail again when they had them (a new block: when
the cloud has them) and computes the cloud's count and bounds from the
blocks as an import does. All of it runs in the transaction of the log
entry. The entry's objects are `(point_cloud, name)` (`Kind::PointCloud`,
no `model_object` row; `put` and `delete` refuse it); its inverse is the
changes back in reverse order with the places found — every stored byte
back (a block removed and made again gets a new row id). Undo is refused
when a point the entry left is not there any more. Edit grants limited to
an area check where the points were and where they go. `Applied::points`
lists the changes as applied (an undo: the changes back) for the
applications to follow. A command takes at most 10⁶ points
(`points::MAX_POINTS`). From 100 points on, `points` is written as one
string `"zstd+base64:…"` (`points::packed`): version byte 1, the count
(LEB128 varint), then for `from` and then `to` the columns — x, y, z each
as the zigzag varint of the difference to the point before (the first to
0), the classes (a byte each), the intensities (varints), the returns (a
byte each), the indices (varint of index + 1; 0: none) — compressed with
zstd and base64-encoded (standard alphabet, padded): some 10 bytes a point
instead of some 200; both forms are read, shorter lists are written as
before. The point store keeps a block's points in runs of a few hundred
while changing them, so 10⁶ points change in seconds; the bytes stored are
the same as before. Example (synthetic, a 1 mm cloud):

```json
{"op": "change_points", "cloud": "survey",
 "points": [{"from": {"xyz": [2600123456, 1200045678, 512345], "class": 2, "intensity": 310, "returns": 17},
             "to":   {"xyz": [2600123456, 1200045678, 513845], "class": 5, "intensity": 310, "returns": 17}}]}
```

Example (synthetic):

```json
{"op": "batch", "commands": [
  {"op": "move_structure", "id": "T2", "position": [1200.0, 340.0, 455.0]},
  {"op": "set_stringing", "section": "S1",
   "stringing": {"kind": "tension", "horizontal_tension": 18000.0, "temperature": 10.0}}
]}
```

### Where the app reads libraries from (`crates/ohl-app/src/model.rs`, `Sources`)

| Source | Default |
|---|---|
| `file` | the line file or project opened |
| `library` (wires) | `*.ohlw` next to the file, else the shipped `libraries/cables` |
| `structures` | `Structures/` next to the file, else the shipped structure library |
| `insulators` | the shipped insulator library |
| `weather` | given `.ohlwc`, else one case at 15 °C |
| `load_cases` | the project's own, else the first `.ohllc` next to the file |
| `rules`, `terrain`, `pointcloud`, `class_scheme` | clearance rules, survey points, point cloud, LiDAR classes |

A project line keeps its libraries as `library` objects; on opening, kinds
it lacks are copied in from these places (one `library` log entry), and the
model reads working copies written to a temporary folder — written again
when another project line is chosen with other library files, or a
library change is undone or redone. Per-user view settings (`ViewPrefs`) stay
on the computer, never in the project — since 2026-10-20 with
`vegetation_colors` (the two colours of the vegetation layers, grow-in
points and danger trees, `[[r, g, b], [r, g, b]]`) and `ground_lines`
(View › Lines on the ground beside the line, `crate::ground_lines`:
`on`, `distance` m, `from` `outermost_conductor` | `axis`,
`hidden_routes`, `color` `[r, g, b]`, `width` px, `dashed`, `profile`;
since 2026-10-21 `shown_case` and `cases` (cases ticked of their own:
**no longer read since 2026-10-22**, the lines take the weather cases
displayed, `ground_lines::Displayed`, and the project lines displayed,
`ground_lines::ProjectLine`; old files read unchanged), and `lidar_corridor` (m from the spans' chords, or
null: the model's corridor; taken at most 200 m; wider than the model's:
a LiDAR ground of the lines' own, built in the app and kept per width,
never the model's ground the checks use); since 2026-10-22 `route_lines`
(`envelope`, the default: one envelope over the routes drawn and, since
2026-10-22, the project lines displayed | `each`: each route and project
line its own lines) — every field optional, settings remembered before
read with the case shown; left out while all are the defaults). The
lines themselves are display only: made in the app per set of weather
cases (the plan envelope over each case displayed, both wind sides, the
jumpers counting as conductors; the wind side the views do not show
solved for them in the background, `ground_lines::GroundLines`), since
2026-10-22 round every span of the line as a graph
(`ground_lines::Graph`: every section's spans between two structures,
those to structures away from the alignments included, the jumpers at
their structures or, on a tap, with their span), closed round its ends;
each point tagged with the span side it lies beside
(`GroundLine::tags`, `Drawn::beside`: an alignment and its side, or
none) for the profile; draped
on the model's ground (`Model::ground`: the active surface, else the
ground built from the LiDAR ground classes of the corridor, else the
alignment's heights; `ground_lines::Ground`), never stored.

The structure application reads a structure's libraries beside its file
(`Sections/`, `Insulators/`, poles `.ohlp`, models `.ohlfe`, recipes
`.ohlslc`, `StructureTemplates/`) or from the folders the line application
passes (the working copy); files it imports there (File › Import) are
stored into the project by the line application (`Change::Libraries`).
See [OHL-Structure](../user-guide/structure-app.md#libraries).

## What a plugin sees

Plugin tiers and manifests: [Writing plugins](plugins.md) and ADR-0003.
The project's own default values (`defaults`) and calculation options
(`solver`) are in the line a plugin receives.
WebAssembly interface: `crates/ohl-plugins/wit/plugin.wit` and
`code.wit` (package `ohl:plugin@0.1.0`; a manifest `ohl-plugin.toml`
targets it with `sdk = "0.3"`). A WebAssembly plugin never gets
Rust structs or the project file: it exchanges **text in the file formats
above**.

| World (extension point) | Receives | Returns |
|---|---|---|
| `plugin` | — | `plugin-info` (name, version, description) |
| `io-plugin` (`io`) export | the **export model** JSON: `{"format": "ohl-export-model", "version": 1, "crs", "line": <line file, structure types placed>, "cases": [{"name", "summary", "wires": [{"section", "span", "circuit", "phase", "color", "system", "voltage_kv", "horizontal_tension_n", "max_tension_n", "sag_m", "points": [[x,y,z]…]}], "strings": [{"structure", "label", "by", "force_n", "swing_deg", "swing_across_deg", "swing_along_deg", "max_load_n", "max_swing_deg", "max_swing_across_deg", "max_swing_along_deg"}]}]}` (strings: each once, as solved, with its limits — not checked by OpenOHL; `null`: not given), plus options | file bytes |
| `io-plugin` import | file bytes, options (`into`: a line file to update) | a **line file** JSON, validated by the host like any line file |
| `terrain-plugin` (`terrain`) | file bytes | surfaces: `name`, `points` (x, y, z flat, m), `triangles` (indices) |
| `project-plugin` (`project`) | file bytes | `project { line: <line file JSON>, libraries: [{kind, name, text}], notes }`; `kind` as in the project's `library` objects |
| `code-plugin` (`design-code`) | a parameter file (TOML text) | `generated { weather-cases: <.ohlwc text>, load-cases: <.ohllc text>, notes, clearance-presets: [{ name, measure: vertical / radial, rules: <.ohlr text>, weather-cases: <.ohlwc text> }] }` (interface 0.2; the structures' own wind of a case travels in the `.ohllc` text as `[case.structure]`, interface unchanged) |
| `code-wind-plugin` (`design-code`; optional export `structure-wind`, looked for by the host: a 0.3 plugin without it unchanged) | the same parameter file | `option<string>`: a cross-section library's text with only a `[wind]` table (drag per shape, face drag and shielding by solidity), checked as the cross-section library reads it and written as `<plugin>-wind.ohlsec` into the cross-section library; none: no factors (the member wind keeps its generic drag, no shielding) |
| `code-rules-plugin` / `code-check-rules-plugin` (`design-code`; optional export `rule-presets`, looked for by the host on its own: a 0.3 plugin without it unchanged) | the same parameter file, or an empty text when the user gave none | `result<list<rule-preset { name, description, rules: <.ohlr text>, weather-cases: <.ohlwc text or empty> }>, string>`: presets of any clearance rule set, checked as rule files (each named case among the preset's) |
| any world with the optional export `sheet-templates` (`sheet-templates`; world `sheet-template-plugin`; looked for by the host by name: a 0.3 plugin without it unchanged), or `[[sheet-template]] file = "…"` in the manifest (no code); `[[class-scheme]] file = "…"` (2026-10-27): LiDAR class schemes `.ohlc` offered in the LiDAR import and Class colours (`ohl_plugins::class_schemes`, files only); `[[plan-set]] file = "…"` (2026-10-24): plan sets, `.ohlset` (`format = "ohl-plan-set"`, version 1, `name`, `description`, `[[drawing]] kind` = `sheets` / `route` / `situation` / `phase`, optional `template` (a name), `page`, `scale`, `vertical_scale` (the sheets); `ohl_drawing::plan_set`) | nothing | `result<list<sheet-template { name, description, template: <.ohlsheet text>, images: [{ name, media-type, bytes }] }>, string>`, each read as a template file (one that does not read is shown with its error) |
| any world with the optional export `wire-colours` (`wire-colours`; world `wire-colour-plugin`; looked for by the host by name: a 0.3 plugin without it unchanged; 2026-10-27) | `list<wire-group { circuit, phase, system: option<string>, system-name, owner, number, voltage-kv: option<f64>, frequency-hz: option<f64>, dc, fields: list<(column, value)> (the plugin's own columns on the system and the sections), sections: u32 }>`: one per circuit, phase and system of the line | `result<list<option<string>>, string>`: a `#rrggbb` colour or none per group, in order (another length or another form refused); written as `Section::color` only when the user applies them (Line › Sections › Wire colours from a plugin…, one command) |
| any world with the optional export `system-styles` (`system-styles`; world `system-style-plugin`; looked for by the host by name: a 0.3 plugin without it unchanged; 2026-10-27) | `list<system-info { id, name, owner, number, voltage-kv, frequency-hz: option<f64>, dc, phases: list<string>, fields, sections: u32 }>`: the line's systems | `result<list<system-style { color: option<string>, phases: list<phase-style { code: option<string>, angle-deg: option<f64> }> }>, string>`: written as `System::color`, `SystemPhase::code` and (only when ticked) `SystemPhase::angle_deg` when the user applies them (one command with the wire colours) |
| a check plugin with the optional export `check-weather` (`code.wit`, world `check-weather-export`; looked for by the host by name: a 0.3 plugin without it unchanged; 2026-10-27) | a check's id | `option<weather-hint { temperature-c, wind-pa, ice: bool }>`: the weather case the check's window ticks when it opens with none chosen (the project's case matching it; nothing written) |
| a plugin with the optional export `member-capacity` (`code.wit`, worlds `member-capacity-plugin` / `member-capacity-export`; looked for by name, additive to 0.3; 2026-10-27) | `methods()`; `member-capacities(method, strength, list<member-input>)`: per member id, kind, shape and dimensions, area, iy, iz, iv, wy, wz, E, fy, fu, length, k, lk, its design data as TOML (`capacity::design_toml`), axial force, end moments, the crossing partner's axial force (`capacity::capacity_inputs`) | `list<member-result { compression, compression-governs, tension, tension-governs, notes }>`: used by `check::member_usage_with` in place of the model's method (`WasmHost::member_capacities`); `ohl structures fe --capacity-plugin` |
| any world with the optional export `field-presets` (`plugin.wit`, worlds `field-preset-plugin` / `field-presets-export`; looked for by name, additive to 0.3; 2026-10-31); the manifest's `[field-input]` table (`roles`, `ground-roles`, `ground-cell`, `survey-codes`, `reach`, `max-points`; unknown keys refused; `ohl_plugins::FieldInputTable`) says what the host gathers (`ohl_io::field_input::gather`) | `parameters-template()`; `field-presets(field-input { parameters: TOML, model: <export model JSON, the case run>, points: list<input-point { x, y, z, class (0: survey), code, line }>, ground: x, y, z flat (lowest per cell) })`; `report-cut(parameters, plan: <ohl-field-plan JSON or "">, preset, cut: <ohl-field-cut JSON>)` → `cut-report { files, summary }` once per cut, then `report(parameters, plan, preset, results: <ohl-field-results-set JSON without the cuts' grids>, cuts: <the summaries>)` (2026-11-02) | `result<list<field-preset { name, description, cuts: list<cut { label, alignment, station, width, above, step }> (2026-10-31: on the alignment), points: list<place { name, x, y, z?, height? }>, levels-ut, levels-kv-m, outlines: list<outline { id, ring: x, y flat, ground, top }>, notes }>, string>`, every value checked by the host (`field_presets::check_presets`: the alignment in the line, the station within it, sizes above 0, cut points capped, names unique) before the cuts (`CutSpec.label` from the cut's label) and points are added to the run; `report` → `list<out-file { name, media-type, bytes }>`, names without a folder (`check_files`); `ohl electrical magnetic|electric|fields --preset-plugin` |
| `check-plugin` / `code-check-plugin` (`check`) | `checks()`: nothing; `value-tables` / `edit-value`: a values file's text (e.g. `.ohlpd`); `evaluate`: the values, the user's choices, a `conductor-pair` (same system / span, kV, plugin column values, altitude, diameters, masses, spacer sags) and per station 6 numbers (distance, system angle, both local sags, both string lengths) | the check list (id, title, menu; the app's Checks menu is built from it); tables; the edited file; per station the required distance and the check's own values, the rule applied |

So a plugin that writes a line follows the line file rules on this page:
global coordinates, ids unique per kind, sections and jumpers ending at
wire points with sets, `cable` / `insulator` / `type` naming library ids
that exist (or are returned with it, for a project plugin).

**Python** (`ohl` module in the application, [Python
console](../user-guide/desktop-app.md#python-console)): `ohl.line()`
gives the working line as a line-file dict; `ohl.results(case)` the
solved spans; `ohl.apply(command)` / `ohl.batch(commands)` change the
project with the session commands above (validated, logged, undoable).
The standalone `ohl` package (PyPI `openohl`) (`crates/ohl-py`) offers unit conversion
(`convert`, `quantity_kinds`); scripts outside the application read and
write the files above.
