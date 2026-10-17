# Data model map

A map of the data OLS CAD keeps: the line model, the libraries it refers
to, the files they live in, how objects point at each other, and what a
plugin sees. It is written for plugin builders and contributors; the user
guide pages linked below give the full field lists of each file.

> **Keeping this map up to date.** Any change to a model struct
> (`ols-line`, `ols-structures`, the library crates), a file format
> (line file, OLS XML, library files, the project file's tables) or the
> plugin interface (`crates/ols-plugins/wit/*.wit`, the export model, the
> Python `ols` module) updates this page **in the same commit**. If a
> field is added, renamed or removed, or a reference changes from an id to
> a label, the diagram and the tables below change with it.

Known weak points and proposed changes: [Data model review](data-model-review.md).

All examples on this page are invented (structures `T1`, `T2`, sections
`S1`, a system `sys-A`, …).

## Overview

```text
                                  LINE  (.ols.json / OLS XML / project rows)
 ┌──────────────────────────────────────────────────────────────────────────────────┐
 │ meta: name, crs, display, systems[], creep_stretch, bimetallic, solver,          │
 │       defaults, circuit_starts[]                                                 │
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
                         LIBRARIES (TOML, units in the values)   .olsw   .olsi .olss
   .olsload point and spread load types ◄── LineObject.type, SpreadObject.type   │
   .olswc weather cases ◄── creep_stretch names, LoadCase.weather          ▲     │
   .olslc load cases                                                       └─────┘
                                                        TypeString.insulator ─► .olsi id
   .olss StructureType: point[], wire[], string[], extension[], node[], member[],
                        section[], material[], load[], load_case[], capacity, wind,
                        model ─► .olsfe, pole ─► .olsp (or [pole] in the file),
                        extension[].node[]/member[]/leg[] (pieces below the feet),
                        template (built from ─► .olspt)
   .olsslc structure load cases (load_case[]; structure application) ─► labels of
                        .olss points, sets, member ids; line_case ─► LoadCase.name
   .olspt PylonTemplate: parameter[], attachment[] (slot, across, up),
                        earth[], strings ─► builds a level-1 .olss
   .olssec SectionLibrary: material[], section[] (shape, dimensions,
                        material ─► material id) ◄── member section
                        (.olss / .olsfe; the library first, then [[section]])
```

Arrows point from the referring field to what it names. Everything inside
the box belongs to one line; the libraries are shared and referred to by
**id** (or, for weather cases, by **name**).

## Conventions

### Units

- **Line model** (`.ols.json`, OLS XML, project rows): plain numbers in
  SI — metres, newtons, N·m², m², °C for temperatures. A field name says
  when it is not: `_deg` degrees, `_kv` kilovolts, `_hz` hertz. Colours
  are `#rrggbb`.
- **Library files** (TOML): values carry their unit as text
  (`"12.5 m"`, `"1.4 kN"`, `"400 mm²"`, `"20 °C"`), converted to SI on
  reading (`ols_core::units`; a plain `f64` field reads so through
  `units::si::de`); a plain number is SI. A bare number is otherwise a
  factor or a count. Exceptions, where the key or the file says the unit:
  angles in degrees (`tilt`, `_deg`: a plain number is degrees, text may
  give `"0.17 rad"`, `units::si::de_degrees`), `_kv` keys, prices (in the
  cost file's currency).
- Inside the program every quantity is SI ([Conventions and
  units](../theory/conventions.md)).

### File headers

Every OLS CAD file may start with `format` (its id, e.g. `ols-wire`) and
`version`; both optional. A file of another format is refused; any
version up to the program's is read (older ones converted), a newer one
refused ("written by a newer OLS CAD") — one check for all,
`ols_core::files::check_header` (the line file and OLS XML, check
reports and colour palettes too; plugins' own files, as `.olspd`, the
same way). A key a format does not know is refused, naming it (a typo is
said, not left out); `crates/ols-cli/tests/format_typos.rs` checks every
library format (the structure application's in
`apps/structure/ols-structure-tests/tests/format_typos.rs`). Objects on points are read through `LineObjectFile` /
`ObjectTypeFile` for that (a flattened shape would let keys through), a
size of another shape or a missing size said too.

### Coordinates and angles

| Frame | Axes | Where it is used |
|---|---|---|
| Project (global) | x east, y north, z up, metres, in the line's `crs` | Every point in the line model: structure `position`, structure points, wire points, `members`, alignment points, jumper supports. |
| Structure local | origin at the structure's ground centre; **x to the right** of the line looking ahead, **y ahead**, z up (right-handed) | Structure types (`.olss`), structure loads in the structure's axes, the editors. |

- `Structure.orientation_deg`: the direction of the local **x** axis
  (the right of the line), degrees **counter-clockwise from east**
  (mathematical sense). Ahead (local +y) is a quarter turn
  counter-clockwise from it.
- `ols_structures::structures::to_global(position, rotation_deg, local)`
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

## The line (`ols_line::Line`)

Source: `crates/ols-line/src/lib.rs`. One line per `.ols.json` file and
per project line in a project file. Full field reference: [Line
files](../user-guide/line-file.md).

| Field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `format` | string | — | `"ols-line"` | — |
| `version` | u32 | — | file version (now 5: jumper ends as `JumperEnd`, places with one key, jumper supports as points, a section's `circuit_name`; versions 1–4 are read and upgraded — version 4 files' `from_tap` / `to_tap`, `support` / `supports`, places with several keys, and `loads` / `spread_loads` of before points and objects) | — |
| `name` | string | — | line name | — |
| `crs` | string | — | coordinate reference system, e.g. an EPSG code (informative) | — |
| `routes` | `[Route]` | — | routes: paths of the line, made of alignments | — |
| `alignments` | `[Alignment]` | — | alignments, in order; each on one route, through point ids | route ids, point ids |
| `alignment_points` | `[AlignmentPoint]` | — | alignment points (PIs), each on any number of alignments | structure ids |
| `structures` | `[Structure]` | — | towers and poles | — |
| `sections` | `[Section]` | — | wire sections | structures + sets |
| `jumpers` | `[Jumper]` | — | jumpers between wire points | structures + sets |
| `spacers` | `[Spacer]` | m, N | interphase spacers: an insulator between span points of two sections of one span (`a`, `b`: section id + span point label, each point holding a clamp object — required; files before give bare ends a clamp of no weight, `Line::clamp_spacer_ends`; insulator label or weight, wind area, stiffness; `length` (none: the still-air distance at stringing); `compression` limit, N, from the project defaults when made; both sections by finite elements, ruling span refused); solved by `solve::solve_spacer_group` (the sections joined, in turn, Aitken's relaxation), reported on the section at `a` (`SectionShape::spacers`: ends, axial force, slack, over its limit) | sections + span point labels |
| `defaults` | `Defaults` | N | the project's default values (Criteria › Default values; `ols_line::DEFAULT_FIELDS`; the program's in `Defaults::PROGRAM`, the one place they are given — `ols_thermal` and `ols_clearance` take theirs from it): what new objects start with and what the calculations take where nothing else gives a value — `spacer_compression` (10 000 N), `jumper_exit_angle_deg` (30°), `jumper_stiffness_factor` (0.2), `jumper_clamp_length` (0.1 m, new clamps), `insulator_stiffness` (EA 5·10⁷ N), `air_density_factor` (Q 0.613), `thermal_emissivity` / `_absorptivity` (0.8), `thermal_roughness` (0.1), `thermal_albedo` (0.1), `thermal_time_step` (10 s), `clearance_wind_step` (50 Pa), `clearance_vertical_band` (0.5 m), `clearance_search_margin` (5 m), `clearance_zone_width` / `_depth` (1 m / 0.2 m, new windows); exported / imported as `.olsdef` (`ols-defaults`, TOML, sections `[spacer]`, `[jumper]`, `[insulator]`, `[weather]`, `[thermal]`, `[clearance]`, values in their units); OLS XML `<defaults section_key=…>` (only those not the program's). Display settings stay per user | — |
| `systems` | `[System]` | — | electrical systems | — |
| `display` | `Display` | — | phase colours, `ahead_deg` | phase labels |
| `creep_stretch` | `CreepStretch` | — | weather cases for creep / load stretch | weather case **names** |
| `bimetallic` | `BimetallicModel` | Pa | outer-strand behaviour of bimetallic cables above the transition temperature, for cables that do not say | — |
| `solver` | `SolverOptions` | — | how the calculations are done (Criteria › Calculation options): `jumper_loads` — `estimate` (default: weight, ice and wind of the jumper on the clamps) or `pull` (the solved jumper's end forces, sections and jumpers solved in turn); `taps` — `loop` (default: a jumper's own force at its tap put on what it hangs on, solved in turn until the tap settles) or `once` (the estimate, one calculation); in the project's line settings and OLS XML `<solver taps>` | — |
| `circuit_starts` | `[SectionPoint]` | — | circuit ends that are starts (power flows in); each a point of a section, given once (validated) | structures + sets |
| `condition_cases` | — | — | **not stored**: the weather cases named by `creep_stretch`, set by the program solving the line | — |

**Plugin values** (`fields`, `ols_line::Fields`): structures, sections,
jumpers, systems, alignments and routes carry a map `plugin.column →
text`, the values of the table columns plugins add (`[[column]]` in a
plugin's manifest, [Writing plugins](plugins.md#table-columns)). Empty
maps are not written. Kept with the object through every edit, written
to OLS XML as `<field key value>`; shown (unused) where the plugin is not
installed. A circuit's values are on all its sections. A check plugin
reads its own (`ols_clearance::phase_distance` passes the sections'
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

- a **structure** tied to an alignment point, or within 5 m of an
  alignment (`NEAR_ALIGNMENT`, beside it or within 5 m of an end), is on
  that alignment and its route; a structure giving its own `routes` is on those (stationed on
  the nearest alignment of each); at a junction it is on every route
  meeting there;
- a **section** is on the routes of its structures (one running from a
  route into another is on both); a **jumper** on those of its two ends;
  strings, wire points and counterweights on those of their structure.

`AlignmentView` (id, route, points, structures in station order) is what
the profile, drawings, stationing and spotting work on; each structure on
an alignment gets a station there (`Station`: station, offset, line angle,
ahead span — computed, not stored).

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
| `type` (`structure_type`) | string? | — | structure type | `.olss` `id` |
| `extension` | string? | — | height extension of the type | `.olss` `[[extension]] name` |
| `description` | string | — | free text | — |
| `height_adjust` | f64 | m | typed: feet above the ground at `position` (may be negative) | — |
| `points` | `[StructurePoint]` | — | points where strings hang | — |
| `wires` | `[WirePoint]` | — | wire points | — |
| `strings` | `[InsulatorString]` | — | strings | labels of points / wire points |
| `omit` | `[string]` | — | typed: the type's points, strings or wire points left out | labels in the type |
| `omit_sets` | `[u32]` | — | typed: the type's wire points with these sets left out, with what hangs only for them | sets in the type |
| `members` | `[[x,y,z]]` | m | stick drawing polylines (views and drawings only) | — |

A structure without a type holds its own `points`, `wires`, `strings` and `members`; the desktop app edits them as a structure type in local axes (`to_local`/`to_global`) and writes them back with one `Put`. *Make it a type* writes that document as a `.olss`, sets `type` and empties the structure's own lists. A type built from a pylon template records it in `[template]` (`id`, `role` suspension/strain, `string_length`, `string_weight`, `values` by parameter) and has sets from the template's slots: suspension = slot, strain back = slot + `back_offset` (20), ahead = slot + `ahead_offset` (40). Built again, only the header, `[template]` and the points, strings and wire points the template makes (by label) are replaced; the rest of the file is kept (`ols-app` `pylon_builder::rebuilt`).

| Entity.field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `StructurePoint.label` | string | — | unique among the structure's points **and** wire points | — |
| `StructurePoint.position` | `[x,y,z]` | m (global) | where strings hang | — |
| `StructurePoint.description` | string | — | free text | — |
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
| `InsulatorString.insulator` | string? | — | its **library label**: the insulator assembly it takes weight, wind area and stiffness from (in a project every own string gets one: `Line::catalogue_strings`, the app labelling on opening and with each edit) | `.olsi` `id` |
| `InsulatorString.weight` | f64 | N | half at each end | — |
| `InsulatorString.wind_area` | f64 | m² | half at each end | — |
| `InsulatorString.stiffness` | f64? | N (EA) | default `solve::DEFAULT_INSULATOR_STIFFNESS` | — |
| `InsulatorString.compression` | f64? | N | compression it may take (then two-way, flagged beyond); none: tension only | — |
| `InsulatorString.strain` | bool | — | a strain (dead-end) string, else suspension: its set is a strain point | — |
| `InsulatorString.description` | string | — | free text | — |

The string graph: a V-string is two strings to one wire point; strings
hang from strings (a V with a string below, a TAK). A wire point no string
reaches is fixed to the structure. A string's **length and direction**
place its wire point (`Line::place_strings`, when a line is read or
resolved): top + length × direction; a V's wire point where its first
two legs' lengths meet, the point of that circle nearest where the first
leg's direction points (their directions follow).
Strings without a length (files written before) take it and their
angles from where their wire point is, so nothing moves. There is no
"ahead"/"back" for a string: its angles alone give its direction. Version 1 files gave one `attachments`
entry per set and phase; they are read through `AttachmentV1` and
`set_from_v1(set, phase)` (`set` for phase 1, else
`set + 1000·(phase + 1)`).

**Typed structures.** With a `type`, the file keeps only the overrides:
`Line::resolve` (`crates/ols-line/src/library.rs`) places the type
(`to_global` with `orientation_deg`, the extension, `height_adjust`),
then the structure's own items replace the type's **by label**, a wire
point with a set replaces the type's wire point **with that set** (and
what moves with it), `omit`/`omit_sets` leave items out, weights,
stiffnesses and wind areas come from the insulator library where a
string has none (a V-string assembly's shared by its legs), and the
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
| `cable` | string | — | cable | `.olsw` cable `id` |
| `circuit_name` | string | — | the name of the circuit it is part of (its sections give the same; the circuits themselves are traced, `electrical::circuits`); files before version 5: `circuit` | — |
| `phase_label` | string | — | phase label, e.g. `L1` | `SystemPhase.label`, `Display.phase_colors` key |
| `system` | string? | — | system this conductor path carries; given once per path, the other sections of the path follow; none on the whole path: earthed | `System.id` |
| `bundle` | u32 | — | subconductors (1: single wire) | — |
| `bundle_spacing` | f64? | m | distance between neighbouring subconductors | — |
| `bundle_rotation_deg` | f64? | ° CCW looking ahead | rotation of the arrangement | — |
| `points` | `[SectionPoint]` | — | attachment points in line order; first and last are strain points | structure id + set |
| `stringing` | `Stringing` | — | how it is strung (below) | — |
| `span_points` | `[SpanPoint]` | — | labelled points on spans (`span`, `at`: a place — `fraction` (horizontal, from the back point), `distance` (m) or `station` (m, of the line along the alignment both structures stand on, turned into a fraction when solved, `Line::section_with_fractions`; with no alignment through both, at the back point) — one of them, `{"distance": 30}` (`SpanPlace` enum; files before version 5 giving several are read with a station over a distance over the fraction), each with its `object` (one per point: a body or a clamp, `objects::LineObject`) or bare (a spread's end); nodes of the FE model, reported solved (`SectionShape::span_points`) | span index; spacers and spreads name them |
| `spreads` | `[SpreadObject]` | N/m | objects spread between two points of one span: `from`, `to` (point labels), `type` (library id), `load` (downward, N/m of conductor: the whole bundle's, shared by its subconductors), `per_subconductor` (optional, default false, written only when true: `load` is on each subconductor, the bundle carrying `load` × `bundle`, `SpreadObject::bundle_load`); on the wire pieces they cover (FE; `Section::spread_loads` gives each as the whole bundle's, the model of one subconductor takes its share) | point labels |
| `color` | string? | — | `#rrggbb`, overrides the phase colour | — |
| `calculation` | `Calculation` | — | `{condition, method}` (`condition::Condition` × `Method`), written by its name: condition and method joined (`initial-fe` default, `creep-rs`, `max-sag-fe`, …) | — |

`SectionPoint { structure, set }` names the wire point with set `set` on
structure `structure`. Spans are between consecutive points.

`Stringing` is tagged by `kind`:

| `kind` | Fields | Meaning |
|---|---|---|
| `tension` | `horizontal_tension` (N per wire), `temperature` (°C) | in the blocks: one horizontal tension in every span |
| `sag` | `sag` (m), `temperature` (°C), `span` (index, optional) | in the blocks: sagged in the ruling span or in span `span` |
| `unstressed_lengths` | `lengths` (m, one per span), `temperature` (°C), `sagged` (optional nested `Stringing`) | clipped in: each span keeps its own unstressed length |

`LineObject` (`objects.rs`): `kind` (`body` | `clamp` — a clamp holds a
jumper or a spacer, a hinge passing forces only), `type` (the library
type it was taken from; its values are its own), `weight` (N, whole
bundle), its shape flattened (`shape` = `point` | `sphere` (`diameter`) |
`cylinder` (`diameter`, `length`, `axis` along / across / vertical) |
`box` (`along`, `across`, `height`) | `areas` (`wind_area`, `ice_area`)),
`drag`, `clamp_length`, and on a jumper's clamp `hold` — what holds it up,
making it a **support**: `{"strings": {"structure"?, "wire"}}` (the
strings of a structure to a wire point without a set, an I or a V;
structure default the jumper's `from`) or `{"string": {"point", "length",
"weight"}}` (one tension-only string from a fixed point). Loads in a case (`LineObject::forces`): weight,
the wire's radial ice on its surface (a sphere's shell; a layer on the
other shapes' surfaces), the wind on its iced projected area across the
line, times its drag. A clamp's shape is drawn as given — a cylinder
across the line on a bundle joining its subconductors (drawn only) — and
takes wind and ice as a body's of that shape and size; without a shape
(`point`) it is drawn as a short cylinder along the wire, its
`clamp_length`, and takes no wind nor ice.

Older files (and OLS XML `<load>`, `<spread_load>`) gave point loads
(`loads`: `legacy::SpanLoad` — `span`, place, `vertical`, `label`, `diameter`,
`drag`, `wind_area`, `ice_area`) and spread loads (`spread_loads`: `legacy::SpreadLoad`,
`span`, `start`, `end`, `load`, `label`): read into a point `L1`, `L2`…
holding a body (a sphere of the diameter, or the given areas; the label
its description), and into two bare points `S1`, `S2`… with a spread
between them; written in the new form.

### Point and spread load library (`.olsload`)

`ols-point-spread-loads`, version 1 (`objects::ObjectLibrary`): `name`
and `[[type]]`s — `id`, `name`, `description`, `kind` (`body`, `clamp`,
`spread`), `weight`, the shape as on objects, `drag`, `clamp_length`, a
spread type's `load` (N/m) and `per_subconductor` (optional, spread types
only: its load on each subconductor); sizes, weights, lengths and loads with their
unit (`"600 mm"`, `"70 N"`, `"12.5 N/m"`) or plain SI numbers. Shipped templates (typical values, to check):
`libraries/objects/templates.olsload`. A project's own types are a project library file
of kind `objects` (`project.olsload`), over the templates of the same
ids (Sections › Point and spread load types).

### Jumper and JumperClamp (`lib.rs`, `jumper.rs`)

| Field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `id` | string | — | unique id | — |
| `description` | string | — | free text | — |
| `cable` | string | — | cable | `.olsw` cable `id` |
| `from`, `to` | `JumperEnd` | — | the two ends (different): a wire point `{"structure", "set"}`, or a tap `{"tap": {"span": {"section", "point"}}}` (a span point) / `{"tap": {"jumper": {"jumper", "point"}}}` (another jumper's point); a tap's point holds a clamp object (a hinge: forces only); a span's section by FE; a jumper on a tap is given by its `length` (or rigid); jumpers hanging on each other in a ring refused; solved after what they hang on (`Line::jumper_order`); their loads keyed `solve::LoadAt::{Clamp, Span, Jumper}`. Version 4 files gave a tap as `from_tap` / `to_tap` beside a placeholder `from` / `to` (read: the tap) | structure id + set, or point labels |
| `length` | f64? | m | cut length at `temperature` | — |
| `sag` | f64? | m | or: sag below the chord in still air (no supports) | — |
| `temperature` | f64 | °C | of `length` / `sag` | — |
| `rigid` | bool | — | a tube or bar: no length, sag or supports | — |
| `points` | `[JumperPoint]` | — | labelled points along it — its **supports** among them: a clamp object with a `hold`, placed by a `fraction` (`Jumper::supports` gives those on strings as `JumperClamp`s in order, `Jumper::support` the string from a fixed point as a `JumperSupport`; `set_supports` / `edit_supports` change them); version 4 files gave `support` and `supports` of their own, read into points `S1` and `C1`, `C2`… — (`at`: `JumperPlace` — `fraction` of its length from `from`, `length` along it, m, or `distance`, m horizontally from `from`'s clamp — one of them, `{"length": 3}`; older files giving several: a distance over a length over the fraction), each with its `object` or bare | label unique on the jumper |
| `bending_stiffness` | f64? | N·m² | EI; default estimated from the cable; 0: hinged | — |
| `stiffness_factor` | f64? | 0–1 | share of the solid-bar EI for the estimate | — |
| `angle_from_deg`, `angle_to_deg` | f64? | ° | exit angle at a section's end clamp from its tension string's line (clamp → top; none: the conductor's axis), turned towards the case's load in the conductor's plane (−90…90) | — |
| `color`, `bundle`, `bundle_spacing`, `bundle_rotation_deg` | optional | — | default: those of the first section through `from` (`Line::jumper_section`) | — |

`JumperClamp` (a view of a support point, not stored): `label` (its
point's), `structure` (optional; default the structure of the jumper's
`from`), `wire` (label of a wire point **without a set**, held by one
string — an I — or two — a V — from structure points), `at` (0–1 of the
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
| `System.kind` | `ac` / `dc` | — | default `ac` | — |
| `System.voltage_kv` | f64 | kV | AC: between phases (rms); DC: pole to earth | — |
| `System.frequency_hz` | f64? | Hz | AC frequency | — |
| `System.phases` | `[SystemPhase]` | — | phases (AC) or poles (DC) | — |
| `System.color` | string? | — | colour when wires are drawn by system | — |
| `SystemPhase.label` | string | — | matched with `Section.phase_label` | — |
| `SystemPhase.angle_deg` | f64? | ° | AC phase angle (default evenly spread) | — |
| `SystemPhase.polarity` | i8? | — | DC: +1, −1, 0 (metallic return) | — |
| `SystemPhase.code` | string? | — | line code (dots and dashes) when drawn by system | — |

Derived, never stored:

- **Conductor paths** (`conductor_paths`): sections joined by jumpers or
  by sharing an attachment point. A path carries one phase of one system
  (`Line::section_phases`); contradicting assignments are errors.
- **Circuits** (`circuits`): a conductor path with a direction — its
  starts are the ends listed in `circuit_starts`, else it runs along the
  line from the end that comes first. `Circuit { sections, jumpers,
  starts, ends, chosen, order }`; `directions` gives each section's and
  jumper's direction in its circuit.
- **Potential**: a section's nominal voltage and voltage to earth, from
  which `Potential::between` gives the voltage between two conductors
  (used by clearance rules).

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
| Structure | `type` | StructureType (`.olss`) | `id` |
| Structure | `extension` | the type's extension | `name` |
| Structure | `omit[]` / `omit_sets[]` | the type's items | label / set |
| InsulatorString | `top` | StructurePoint or WirePoint of the **same** structure | label |
| InsulatorString | `wire` | WirePoint of the same structure | label |
| InsulatorString, TypeString | `insulator` | InsulatorAssembly (`.olsi`) | `id` |
| Section | `points[]` | WirePoint | structure `id` + `set` |
| Section, Jumper | `cable` | Cable (`.olsw`) | `id` |
| Section | `system` | System | `id` |
| Section | `phase_label` | SystemPhase of that system; `Display.phase_colors` | label |
| Jumper | `from`, `to` | WirePoint | structure `id` + `set` |
| Jumper | `from`, `to` (a tap) | SpanPoint (`{section, point}`) or JumperPoint (`{jumper, point}`) holding a clamp | section / jumper `id` + point label |
| Jumper point's `hold` (a support) | `structure`, `wire` | WirePoint without a set | `id`, label |
| SpanPoint | `span` | the span between `points[span]` and `points[span + 1]` of its section | **index** |
| SpreadObject | `from`, `to` | SpanPoints of the same section and span | label |
| Spacer | `a`, `b` | SpanPoints (`{section, point}`) | section `id` + point label |
| Spacer | `insulator` | InsulatorAssembly (`.olsi`) | `id` |
| LineObject, SpreadObject | `type` | ObjectType (`.olsload`) — informative: the values are the object's own | `id` |
| Line | `circuit_starts[]` | WirePoint at a circuit end | structure `id` + `set` |
| Line | `creep_stretch` | WeatherCase (`.olswc`) | `name` |
| LoadCase (`.olslc`) | `weather` | WeatherCase | `name` |
| StructureType | `capacity.attachment[].point` | TypePoint | label |
| TypePoint | `node` / `member` | TypeNode / TypeMember | `id` |
| TypeMember (and `.olsfe` `[[member]]`) | `nodes`, `section` | TypeNode; the section library's Section (`.olssec`) first, else `[[section]]` (resolved by `ols_structures_fe::Model::parse_with`; the structure type alone does not check a section id) | `id` |
| StructureType | `model`, `pole` | `.olsfe` file (relative path), `.olsp` id or file name (a `[pole]` table refers to nothing) | — |
| InlinePole (`[pole]`) | `section` | the section library's Section (`.olssec`, a polygonal or circular hollow section; resolved by `ols_structures_fe::pole::Pole::from_inline`) | `id` |
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
(`crates/ols-session/src/lib.rs`, `Command::Delete`): when an object takes
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
circuit starts there; `rename_point` (a span or jumper point's label)
updates its spreads, the spacers on it and the jumpers tapped on it;
`rename_system` updates the sections given it. `replace_structure`
(`crates/ols-session/src/replace.rs`) keeps the structure's id, position,
orientation, height adjustment, description, routes and fields, and moves
the section points, jumper ends and circuit starts at each old set to the
new set mapped (given in `sets`, else the same set when the new structure
has it). Library items are
not renamed through the session: a changed library id leaves dangling
references that `Line::resolve` / validation report.

## Library files

One directory per kind; TOML with units in the values. Each item has an
`id` of letters, digits, `-`, `_` and `.`. The app's library editors
(`crates/ols-app/src/library.rs`, `LibraryKind`) cover the first five.

| Kind (`LibraryKind`) | Extension | Format id | Items per file | Rust type | Guide |
|---|---|---|---|---|---|
| `Cables` | `.olsw` | `ols-wire` | several (`[[cable]]`) | `ols_cables::Cable` | [Wire library](../user-guide/wire-library.md) |
| `Insulators` | `.olsi` | `ols-insulator` | one | `ols_structures::InsulatorAssembly` | [Structure library](../user-guide/structure-library.md#insulator-assemblies-olsi) |
| `StructureTypes` | `.olss` | `ols-structure` | one | `ols_structures::StructureType` | [Structure library](../user-guide/structure-library.md) |
| (pylon templates: shipped `libraries/pylon-templates`, the project's `Pylon templates` folder; not kept in the project file) | `.olspt` | `ols-pylon-template` | one | `ols_structures::templates::PylonTemplate` | [Pylon templates](../user-guide/structure-library.md#pylon-templates-olspt) |
| `WeatherCases` | `.olswc` | `ols-weather-cases` | several (`[[case]]`) | `ols_weather::WeatherCase` | [Weather cases](../user-guide/weather.md) |
| `LoadCases` | `.olslc` | `ols-load-cases` | several (`[[case]]`) | `ols_loads::LoadCase` | [Load cases](../user-guide/loads.md) |
| (section library: project kind `sections`; shipped example `libraries/sections`, never copied into a project) | `.olssec` | `ols-sections` | several (`[[section]]`, `[[material]]`) | `ols_sections::Section`, `ols_sections::Material` (crate `ols-sections`) | [Section library](../user-guide/section-library.md) |
| (structure application; not a library kind yet) | `.olsslc` | `ols-structure-load-cases` | several (`[[load_case]]`) | `ols_structure_loads::LoadCaseRecipe` | [Structure load cases](../user-guide/structure-load-cases.md) |
| (structure application: typed loads; project kind `loads`, beside the structure types) | `*.loads.csv` | CSV, the load table's columns | typed rows (`Override`) of one structure type or one placed structure | `ols_structure_loads::LoadTable` | [Structure load cases](../user-guide/structure-load-cases.md#the-load-table) |
| (line application → structure application, derived, not kept) | `*.line-values.json` | `ols-structure-line-values` (JSON) | one placed structure | `ols_structure_loads::PlacedValues` | [Structure application](../user-guide/structure-app.md#started-for-a-placed-structure) |

Other data files: structure models `.olsfe` (`ols-structure-model`) and
poles `.olsp` (`ols-pole`) next to the `.olss`
([Structure analysis](../user-guide/structure-analysis.md)); clearance
rules `.olsr`; thermal ambient `.olsta`; unit costs `.olscost`; galloping
`.olsg`; LiDAR class schemes `.olsc`; phase distance codes `.olspd`.
A project keeps its structure models, poles, clearance rules, ambient
cases, unit costs and galloping parameters as library files (below).

### Phase distance codes (`.olspd`)

`ols-phase-distance`, version 1: a check plugin's values for the
distance between conductors in the span, read and written by the
plugin (its layout is the plugin's; a Swiss LeV plugin, kept apart from
the public code, defines one). OLS CAD only measures
(`ols_clearance::phase_distance`) and keeps a project's edited copy as
a library file of kind `phase-distance`. The check pairs conductors
between the same two structures (and, when asked, nearby conductors
elsewhere within a distance) and takes l_k from the strings at their
ends (I: its length; V, tension, clamped: 0; the string below a V: its
length; a TAK: its suspension string's), and the plugin columns of
their sections; the settings (the plugin's choices, station spacing,
circuits ticked, nearby pairs and their distance) are not in the file.
Results per pair: the worst station (points, where along each
conductor, d, the required distance and the plugin's values) and the
length of conductor below it; conductors joined by spacers in the
span: each one's largest sag between its clamps. A run's parameters
keep the plugin's name and version and its choices. See [Distance
between conductors: the core's part](phase-distance-check.md).

### Clearance rules (`.olsr`)

Source: `crates/ols-clearance/src/rules.rs` (`RuleSet`, `RuleSet::to_toml`
writes one back). Full reference: [Terrain and clearances](../user-guide/terrain.md).

- `[[rule]]`: a target (LiDAR `classes`, the ground, other wires), a
  distance (`vertical` / `radial`, fixed or by voltage), the weather
  `cases` it applies to, `vertical_band` (zone width each side of the
  wire), `vertical_depth` (the area's depth along the line), `voltages`
  (kV of the wires checked; 0: earthed wires) and `wires` (`sections`,
  `jumpers` or `both`).
- `[wind]` (optional): `steps`, `step` (Pa), `sides` — how the desktop
  clearance windows step the wind up to each case's pressure; `ols line`
  ignores it.
- `[check]` (optional, `CheckScope`): `circuits` (circuit names; empty:
  every circuit), `method` (`fe` / `rs`: every section solved by it;
  absent: each section's own), `search_distance` (m: LiDAR points
  farther from the wires are ignored) — what the desktop clearance
  windows solve and read; `ols line` ignores it.
- The desktop Vertical and Radial clearance windows keep their rules next
  to the model, in `<model>.vertical-clearances.olsr` and
  `<model>.radial-clearances.olsr` (one `[[rule]]` per row and target
  kind). A window row (`ols_clearance::zone::ZoneRule`) has an optional
  `width` of its own (the vertical zone's half width, written as the
  rule's `vertical_band`; none: the window's `Zone.width`); the window's
  `Zone.search` is `[check] search_distance`. Rules refer to LiDAR
  classes by number (`.olsc` schemes) and to weather cases by name.
- **Clearance presets** of design-code plugins are such rule files,
  `<plugin>-<name>.olsr` among the project's rule files, each with a
  weather case file of the same name among its weather case files (the
  cases its rules name); the windows' Load preset… lists them.

### Structure type (`.olss`)

Source: `crates/ols-structures/src/structures.rs`. All positions in
**local axes**. Full reference: [Structure library](../user-guide/structure-library.md).

| Field (TOML key) | Type | Unit | Meaning |
|---|---|---|---|
| `format`, `version` | optional | — | `"ols-structure"`, 1 to 3 (`STRUCTURE_FILE_VERSION`; 3 adds `group`, `made_by`, `[[generator]]`, `[[symmetry]]`, the `[pole]` table and an extension's `node`, `member` and `leg` tables, written by the structure application — it raises a file's `version` when it writes them; older versions read as before) |
| `id`, `name` | string | — | unique id (referred to by `Structure.type`), display name |
| `kind` | `lattice` / `pole` / `portal` / `other` | — | family |
| `height` | length? | m | mast top for the default stick drawing |
| `foot` | length? | m | local z of the feet (default 0: ground) |
| `source` | string? | — | where the data comes from |
| `[[point]]` | `TypePoint` | m | `label`, `description`, and `point` **or** `node` (joint id) **or** `member` + `at` (0–1 from its first joint); in the finite-element model (`Model::member_points`, `Model::point_loads`) a point along a member shares its loads to the member's joints, (1 − `at`) and `at` |
| `[[wire]]` | `TypeWire` | m, N | `label`, `description`, `set?`, `point?` (else it hangs below its strings), `weight?` |
| `[[string]]` | `TypeString` | m, N, m² | `label`, `description`, `top`, `wire`, `insulator?` **or** `length` + `weight`, `compression?`, `wind_area?`, `strain`, `angle?` (from straight down), `direction?` (in plan, 0 ahead); `side` (`ahead`/`back`) in files written before: level, 0° or 180° |
| `[[attachment]]` | `TypeAttachment` | — | version 1: `set`, `phase`, `point`, `insulator` or inline `string`, `side` |
| `[[extension]]` | `Extension`: `name`, `dz`; version 3: `[[extension.node]]` (`TypeNode`), `[[extension.member]]` (`TypeMember`), `[[extension.leg]]` (`LegExtension`: `leg`, `dz`) | m | height extensions; at level 3 a piece below the feet (joints with z from `foot − dz` to `foot`, ids unique with the body's; members between its joints and the body's) and leg extensions. `StructureType::extended(ext) -> ExtendedBody` applies the rule (`crates/ols-structures/src/extensions.rs`, [user guide](../user-guide/structure-library.md#extension-bodies-and-leg-extensions)): the piece added, body joints it reaches lose their support, a leg's support joint slides `dz` lower along its steepest rising member; a placed structure on such an extension is analysed on `Model::from_structure_type_extended` (`ols_loads` keys its models by type and extension) |
| `[[body]]` | `points` | m | outline polylines (stick drawing) |
| `[[node]]` | `id`, `point`, `support?`, `spring?`, `attachment?`, `made_by?` | m | pylon joints (detail 2–3); `made_by`: the `[[generator]]` that wrote it |
| `[[member]]` | `id`, `nodes` (2 ids), `section?`, `kind?` (`truss`/`frame`/`cable`), `reference?`, `roll?`, `k?`, `pretension?`, `group?`, `made_by?` | — | pylon members; `group`: its member group (no table of groups: a section per group is written on each member); `made_by` as for joints. `ols_structures_fe` leaves `group` and `made_by` out of the model |
| `[[generator]]` | tables (`generators`, kept unread by the core): `id`, `kind` (`panels` / `crossarm` / `peak` / `copy_face`) and its inputs | m | a body generator's record, read by `ols-structure-edit` (`body::Generator`): panels `bottom?`, `feet`, `waist` (a length or [across, along]), `panel` [{`top`, `bracing` (`single`/`x`/`k`/`diamond`/`warren`), `horizontal`}]; crossarm `points`, `shape` (`triangular`/`rectangular`); peak `points` or `height`; copy_face `body` (a panels generator), `face` (`x+`/`x-`/`y+`/`y-`) |
| `[[symmetry]]` | tables (`symmetry`, kept unread by the core): `mirror` (`x`/`y`/`xy`), `groups?` | — | mirror planes the structure application keeps joints and members in step with (`body::Symmetry`); the first one is the Body tab's option |
| `[[section]]`, `[[material]]`, `[analysis]`, `[[load]]` | tables | — | read by `ols_structures_fe` for the finite-element check; `[[load]]` is also the structure application's case `model` (a member's section: the section library's first; `[analysis]`: `nonlinear`, `self_weight`, `drag` — the member wind's drag factor, default 1) |
| `[[load_case]]` | tables (`load_cases`, kept unread by the core) | N, Pa | the structure's load-case recipes, read by the structure application (`ols-structure-loads`; below) |
| `capacity` | `spans[]` (by line angle: `wind_span`, `weight_span`, `min_weight_span`), `attachment[]` (`point`, `vertical`, `uplift`, `transverse`, `longitudinal`, `interaction`) | m, N | what it can carry (structure axes) |
| `wind` | `band[]`: `from`, `to`, `area`, `drag` | m, m² | wind on the structure itself |
| `model` | path? | — | separate `.olsfe` model, relative to this file |
| `pole` | `PoleRef`: a string (`File`: a `.olsp` id, or a file name ending in `.olsp`, in the same folder) **or** a `[pole]` table (`Inline(InlinePole)`, version 3) | m, Pa, kg/m³ | the pole: `shape` (`round`/`polygon`), `sides?`, `embedment?`, `[[pole.outline]]` (`z`, `diameter` **or** `circumference`; rising z; a polygon across flats), level 3 `material` (`wood`/`steel`/`concrete`/`other`), `section?` (section library), `wall?`, `fibre_stress?`, `moment_capacity?`, `[[pole.capacity]]` (`height`, `moment`), `elasticity?`, `density?`, `drag?` — the `.olsp` fields; read into one `ols_structures_fe::Pole` by `pole::structure_pole` (`Pole::from_inline` for a table; `Pole::profile`, `Pole::sides`) |

**Detail level** (`StructureType::detail`, never stored — an older
`detail` key is read and ignored): 3 *full design* when `model` is given
or every member has a section and a joint is supported (every extension
piece too: its members with sections and a supported joint,
`StructureType::members_designed`); 2 *outline* when
there are members or body polylines that are not all in one plane
(within 1 mm, `StructureType::body_is_3d`); else 1 *stick figure*. The
pole gives its own level (`StructureType::pole_detail`): a named `.olsp`
or a `[pole]` with material, strength and (steel) wall 3, a `[pole]`
outline with diameters at two heights 2 (`InlinePole::is_3d`,
`InlinePole::missing_for_design`); the structure's level is the higher.
Checks that depend on the structure's shape must say what they do at
each level ([Structure detail levels](structure-detail.md)). The
finite-element model of the structure's own members is built when its
members are designed (`members_designed`), not when a pole makes the
level 3; the pole check runs on a level 3 pole. The meaning of the
levels decided for the structure application (capacities derived only at
level 3) is in [Building structures](structure-building.md).

**Editing a structure file** (`apps/structure/ols-structure-edit`,
[crates](structure-app.md)): every edit of the structure application and
of `ols-app`'s pylon workspace is a function on the file's text
(`toml_edit::DocumentMut`, comments and order kept) given the structure
as last read; `StructureFile` keeps the undo and redo of whole edits.
Body tools (same crate): `generators::generate(t, g) -> Body` (joints,
members, points put on tips; pure, ids from the inputs),
`rebuild::{apply, rebuild, rebuild_all, remove_generator, write}`
(replace only what has the generator's `made_by`; what something else
uses is kept, made by hand, and reported in `Report::kept`),
`symmetry::{add_joint, add_member, move_to, removal, make_symmetric}`
(images matched by position within 1 mm), `groups::{groups, set_group,
set_group_section, set_group_kind}`. `StructureFile` refuses a file whose
`[[generator]]` or `[[symmetry]]` tables do not read (`body::check`).
`missing(t, level) -> Vec<Missing>` lists what the next level lacks
(`Need::Level`: no member, a member without a section — one item per
member —, no `[[section]]`, no supported joint; a pole's outline, its
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
`remove_load_case`; `merge_load_cases` takes the cases of a `.olsslc`
text: same name replaces, others are added). The pole (`pole`:
`add_outline_row`, `set_outline_row` — refused when the heights would
not rise —, `remove_outline_row`, `set_shape`, `set_field`, `remove_pole`,
`bring_in` a `.olsp`'s text as a two-height outline from the feet) and
the extensions (`extension_edits`: `add_joint` — refused outside the
piece's heights —, `add_member`, `remove_joint` with its members,
`remove_member`, `set_field`, `set_leg`, `build_piece`). `geometry_with(t,
insulators, extension)` gives the joints and members as
`StructureType::extended` makes them (`Geometry::extension`: the piece's
ids) and `Geometry::pole` the outline's wireframe.

Placed into a line, a type becomes `PlacedStrings { points, strings,
wires }` (global coordinates) that `Line::resolve` merges with the
structure's own items.

### Section library (`.olssec`)

Source: `crates/ols-sections` (a core crate: the level 3 checks inside
the line read it through `ols-structures-fe`), `SectionLibrary`.
Format `ols-sections`, version 1 (constants `SECTION_FILE_FORMAT`,
`SECTION_FILE_VERSION`, `SECTION_FILE_EXTENSION`). Full reference:
[Section library](../user-guide/section-library.md).

| Field (TOML key) | Type | Unit | Meaning |
|---|---|---|---|
| `format`, `version`, `name`, `description` | optional | — | header |
| `default_material` | id? | — | material of the sections naming none |
| `[[material]]` | `id`, `name`, `description`, `elasticity`, `shear_modulus?` (default E / 2.6), `density?` (0), `yield?`, `ultimate?`, `partial_factors?` (name → number) | Pa, kg/m³ | `ols_sections::Material` |
| `[[section]]` | `id`, `name`, `description`, `shape`, `material?`, dimensions of the shape (`h`, `b`, `t`, `tw`, `tf`, `r`, `d`, `sides`) | m | `ols_sections::Section` (`Shape`) |
| section, given | `area`, `iy`, `iz`, `iu`, `iv`, `it`, `wel_y`, `wel_z`, `wpl_y`, `wpl_z`, `gyration_y/z/u/v`, `shear_area_y/z` | m², m⁴ (`SecondMoment`), m³ (`SectionModulus`), m | replace the computed values (`Given`) |

Shapes: `equal_angle`, `unequal_angle`, `channel`, `i_section`,
`rectangular_hollow`, `circular_hollow`, `round_bar`, `polygonal_hollow`,
`general` (properties given only). `Section::properties()` computes area,
centroid, second moments about the geometric and principal axes, torsion
constant, elastic and plastic moduli, radii of gyration and shear areas;
`Section::wind_width(direction)` the outline's width normal to a wind in
the section's plane (body wind from the profiles, decision 10 of
[Building structures](structure-building.md)). Several files merge in
order (`merge`: project first, then user; the earlier id wins);
`missing_materials()` lists sections whose material is in none.

**Referred to by structures** (structure-building §1.3, §4.2): a
member's `section = "<id>"` (of a `.olss` full design or an `.olsfe`)
resolves in the section library first, then in the structure's own
`[[section]]` tables (`Model::parse_with`, `from_structure_type_with`,
`load_with` take the `SectionLibrary`; the plain forms an empty one). A
library section becomes `ols_structures_fe::Section::from_library`: the
profile's area, iy, iz, torsion constant (iy + iz without one), minor
principal second moment (`i_min`, buckling), elastic moduli, and its
material's E, G, density and yield; it keeps the profile
(`Section::profile`), which gives the member wind
(`Model::member_wind`, `Model::has_profiles`; `[analysis] drag`, default
1). A library id replaces a table of the same id; an id in neither, or a
library section's material missing, is refused, naming it. In a project
the library is the library kind `sections` of the project file, per
project line ([Project file tables](#project-file-tables-cratesols-session));
the app reads it with `Model::section_library` and gives it to the line's
structure checks (`ols_loads::capacity::usage_with_sections`). Member
groups (one section for many members) come later and resolve the same way.

### Insulator assembly (`.olsi`)

Source: `crates/ols-structures/src/insulators.rs`.

| Field | Type | Unit | Meaning |
|---|---|---|---|
| `id`, `name`, `source` | string | — | referred to by `insulator` of strings |
| `kind` | `suspension` / `v_string` / `strain` | — | basic kind |
| `length` | length | m | attachment point to clamp (V: leg length) |
| `weight` | force | N | whole assembly |
| `axial_stiffness` | force | N | EA of a string or leg (default 5·10⁷ N) |
| `wind_area` | area | m² | projected area |
| `half_width` | length? | m | `v_string` only: centre to each leg top |

### Weather and load cases

`WeatherCase` (`ols-weather`): `name` (the key other files use),
`description`, `tags`, `temperature` (cable), `ice_thickness` or
`ice_load`, `ice_density`, `wind_pressure`, `wind_from` (`right`/`left`,
looking ahead), `air_density_factor`, `ambient_temperature`.

`LoadCase` (`ols-loads`): `name`, `description`, `weather` (a weather
case name), `condition` (initial, creep, load, max sag), factors
`vertical`, `transverse`, `longitudinal`, `strength`, `tags`.

### Structure load cases (`[[load_case]]`, `.olsslc`)

The structure application's recipes
(`apps/structure/ols-structure-loads`, [crates](structure-app.md)): in a
`.olss` as `[[load_case]]` (the core keeps them as raw tables,
`StructureType::load_cases`), or on their own in a `.olsslc` file
(`format = "ols-structure-load-cases"`, version 1). Full reference:
[Structure load cases](../user-guide/structure-load-cases.md).

| Item | Fields | Refers to |
|---|---|---|
| `LoadCaseRecipe` (`[[load_case]]`) | `name`, `description`, `line_case`, `strength`, `self_weight`, `load[]` | `line_case` ─► a line `LoadCase.name` |
| `LoadRecipe` (`[[load_case.load]]`) | what: `what` (`LineQuantity`) **or** `value` (N) **or** `pressure` (Pa); how: `line_case`, `factor`, `one_sided`, `side`, `direction` (structure axes); where: `points` (`"all"` or labels), `sets`, `phases`, **or** `members` (`"all"` or ids); `note` | point / wire point labels, sets, phase labels (from the line), member ids of the `.olss` |

Evaluated (`evaluate`) with `LineValues` (per point: sets, phases; values
per line load case, point or set and quantity) into a `LoadTable`
(rows: `case`, `load`, `at` point/member/joint, `label`, `force`
transverse/longitudinal/vertical down N, `source` recipe/line/typed,
`evaluated`, `missing`, `note`, `area` — a surface load's member area,
m²). The table is a result, not stored in the structure file. Typed
values (`Override`: case, load, at, label, force) are kept by the
structure application beside the structure file as **`<stem>.loads.csv`**
(the table's CSV, typed rows only; the structure on its own) or
**`<stem>.<placed structure>.loads.csv`** (started for a placed
structure, `--structure`; the id with characters other than letters,
digits, `-`, `_` as `_`), written on Save and read on open
(`ols_structure_loads::overrides_path`). In a project the line
application keeps both in the project as library kind **`loads`** (beside
the structure types in the working folder; only `*.loads.csv`), so the
values typed for a placed structure are stored with it per project line
— no field of the line file. A table read back (CSV) types what it has
beyond the evaluation (`LoadTable::overrides_over`). `[[load]]` is read
as case `model` (joints). CSV of the table (the `area` column optional
when read) and the recipes; XLSX later.

`ols-structure-link` fills `LineValues` from `ols_loads::structure_loads`
(the six force values; tensions, wire wind and ice not yet), solves
table cases on the structure's model (`analyse_cases` → `CaseResult`:
joints with displacements, members with axial force and usage,
reactions, Σ loads and Σ reactions; members' sections from the section
library first) and gives the members' areas from the section library
(`SectionAreas`: `Model::projected_area` — the profile's width for the
pressure's direction in the member's y–z plane × length × sin θ; a
member whose section is not in the library has no area).

**Line-values file** (`<stem>.<placed structure>.line-values.json`
beside the structure file, `ols_structure_loads::PlacedValues`, JSON,
`format = "ols-structure-line-values"`, version 1): what the line
application hands to the structure application for **Edit…** on a placed
structure — `structure` (placed structure id), `line` (project line, or
the line file's name), `structure_type`, `cases` (the line's load cases),
`values` (`LineValues`), `others` (`OtherPlace`: the other placed
structures of the type with their `LineValues`), `note` (why there are
no values: being computed, no load case file). Derived data, rewritten
when the line changes; never stored in the project.

### Structure exports (IFC, SAF, CSV)

Written by `apps/structure/ols-structure-export` (File › Export of the
structure application, tool `ols-structure-export`); nothing is read
back. `ols_structure_link::export_structure` turns a level 3 `.olss`
(through the FE `Model`), the section library and a `LoadTable` into
plain data, `ExportStructure`: `nodes` (`id`, `point`, `fixed[6]`,
`spring[6]`), `materials` (E, G, density, yield), `sections` (`shape`
from the library or none for a `[[section]]` table, A, Iy, Iz, It, Wpl,
the centroid's offset from the outline box), `members` (joint and
section indices, kind, local axes `[x, y, z]` as the FE model takes
them, length, k, pretension) and `load_cases` (`self_weight` factor,
loads at a joint, at a point along a member, or uniform per metre along
one; structure axes, z up). Formats: IFC4 (members and the structural
analysis model), IFC2x3 Coordination View 2.0 (members, for Tekla
Structures), SAF 2.2.0 xlsx, CSV tables. Mapping and what is lost:
[Structure exports](structure-exports.md).

## Where data lives

### Files

| Store | Format | Content | Reference |
|---|---|---|---|
| Line file `*.ols.json` | JSON, `ols_line::Line` (serde; unknown fields refused) | one line | [Line files](../user-guide/line-file.md) |
| OLS XML | XML, namespace `urn:ols-cad:ols-line:1`, schema `docs/schema/ols-line.xsd` | the same content as the line file, lossless round trip (`crates/ols-io/src/olsxml.rs`): objects are elements, plain values attributes named like the JSON keys, points `x`/`y`/`z` attributes | [OLS XML schema](../schema/ols-line.md) |
| Project file `*.ols` | SQLite (ADR-0004) | project lines, operation log, users, libraries, point clouds, surfaces, versions | [Project files](../user-guide/project.md) |
| Libraries | TOML directories | see above | — |

### Project file tables (`crates/ols-session`)

| Table | Content |
|---|---|
| `ols_meta(key, value)` | settings: `session_schema` (4; a newer one is refused before any table is created or migrated), `grid_size`, the weather file used, … |
| `session_user(name, role)` | `viewer`, `designer`, `admin` |
| `model_object(line, kind, id, ord, version, json)` | one row per object of a project line; `kind` is `meta`, `route`, `alignment`, `alignment_point`, `structure`, `section`, `jumper`, `spacer`, `report` (`ols_session::Kind`) — routes, alignments and alignment points are three kinds of row; span points, spreads and jumper points are inside their section's / jumper's row; `report`: a performed check (below), kept with the line, not part of it; `json` as in the line file; the line's settings are `meta` rows: `line` (`name`, `crs`, `display`; projects before routes and alignments also an `alignment`, converted once when they open) and one row per setting — `systems`, `creep_stretch`, `bimetallic`, `solver`, `defaults`, `circuit_starts` (`{"solver": …}`) — so two users changing different settings keep both and undo their own; a change puts the rows that differ from the line it was made on (`line_settings_change`). Projects written before kept every setting in `line`: read with the rows over it, until a change writes the rows; spacers are in the spatial index (where their ends are), jumpers with their taps; `replace_line` keeps the replacing line's order |
| `oplog(seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by, note, line)` | every applied command with its inverse; `kind` is `do`, `undo`, `redo`, `rollback` or `import` |
| `library` objects (`model_object` kind `library`, id `kind/name`: `{"id", "kind", "name", "text"}`) | the project line's library files: `kind` `cables`, `structures`, `insulators`, `weather`, `loadcases`, `objects` (`project.olsload`, the project's point and spread load types), `phase-distance` (an admin's copy of a `.olspd`), `rules` (`.olsr`), `galloping` (`.olsg`), `ambient` (`.olsta`), `costs` (`.olscost`), `poles` (`.olsp`), `femodels` (`.olsfe`), `sections` (`.olssec`, the section library), `loads` (`*.loads.csv`: values typed over a structure's load table in the structure application, per structure type and per placed structure, [below](#structure-load-cases-load_case-olsslc)); each project line has its own (a new line starts with those of the line it is made from); changed as logged commands (`put_library_file`, `delete_library_file`: undone as any change; `keep_library_files`: what the program does on its own — files next to the project copied in when it opens, an older form converted — logged as `library`, not undone by Undo); by the "library" edit grant; revisions keep them apart (`State::libraries`). Schema 3 kept them in a `library_file(kind, name, time, user, text)` table shared by all lines: copied into every line when the project opens, the table dropped (schema 2 kept frozen lines in `line_snapshot`: made lines, the table dropped; new files have neither table). When the project opens, the app copies in the kinds it lacks from next to the project (never the shipped libraries; `poles`, `femodels`, `loads` from its structure folder; `sections` from the `Sections/` folder next to it, else the files next to it) and writes each line's files to a working folder (`poles`, `femodels`, `loads` next to the `structures`; `sections` in its own folder), which it reads from then on |
| `spatial_index` | plan bounds and tile of every object (ADR-0008) |
| `vc_*`, `edit_grant`, `surface`, point cloud tables | versions and reviews, edit rights, terrain surfaces, LiDAR ([Versions](../user-guide/versions.md), ADR-0007) |

### Performed checks (`ols_session::reports`)

A check's saved run: a `model_object` of kind `report` on the project
line it was made on, so it belongs to that line's branch (commits,
checkout, merges and bundles carry it like any object; saving, a status,
a note, a rename or a deletion is a logged, undoable command). Not part
of `ols_line::Line`: the line file and the views never see it, and a line
replaced (`replace_line`) keeps it. Restricted edit grants do not limit
it. Exported alone as `.olscr` (JSON).

`CheckReport` (`format` `ols-check-report`, `version` 1): `id` (its name,
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

### Commands (the only way to change a project)

`ols_session::Command`, JSON tagged by `op` (snake_case): `put` (`kind`,
`id`, `value` as in the line file, `ord`), `delete` (`kind`, `id`,
`confirm`), `move_structure`, `set_stringing`, `move_alignment_point`,
`insert_alignment_point` (a new point, or an existing `point`: shared),
`remove_alignment_point` (deletes the point: off every alignment),
`tie_alignment_point`, `detach_alignment_point`,
`align_under_structures`, `delete_structure`, `delete_attachment`
(`structure`, `set`), `replace_structure` (`id`; `structure`: the new
one, its `type` and `extension` or its own points, strings and wire
points; `wires`: a typed new structure's wire points as placed,
`{label, set}` — the session does not read the structure library;
`sets`: `{from, to}` old set → new set, `to` left out: removed;
`confirm`), `rename`, `rename_attachment`, `rename_system`,
`replace_line`, `batch`. Each is expanded into puts and deletes, the whole
line is validated, and the log keeps the command, its inverse, the objects
changed and the sections to recalculate. Example (synthetic):

```json
{"op": "batch", "commands": [
  {"op": "move_structure", "id": "T2", "position": [1200.0, 340.0, 455.0]},
  {"op": "set_stringing", "section": "S1",
   "stringing": {"kind": "tension", "horizontal_tension": 18000.0, "temperature": 10.0}}
]}
```

### Where the app reads libraries from (`crates/ols-app/src/model.rs`, `Sources`)

| Source | Default |
|---|---|
| `file` | the line file or project opened |
| `library` (wires) | `*.olsw` next to the file, else the shipped `libraries/cables` |
| `structures` | `Structures/` next to the file, else the shipped structure library |
| `insulators` | the shipped insulator library |
| `weather` | given `.olswc`, else one case at 15 °C |
| `load_cases` | the project's own, else the first `.olslc` next to the file |
| `rules`, `terrain`, `pointcloud`, `class_scheme` | clearance rules, survey points, point cloud, LiDAR classes |

A project line keeps its libraries as `library` objects; on opening, kinds
it lacks are copied in from these places (one `library` log entry), and the
model reads working copies written to a temporary folder — written again
when another project line is chosen with other library files, or a
library change is undone or redone. Per-user view settings (`ViewPrefs`) stay
on the computer, never in the project.

## What a plugin sees

Plugin tiers and manifests: [Writing plugins](plugins.md) and ADR-0003.
The project's own default values (`defaults`) and calculation options
(`solver`) are in the line a plugin receives.
WebAssembly interface: `crates/ols-plugins/wit/plugin.wit` and
`code.wit` (package `ols:plugin@0.1.0`). A WebAssembly plugin never gets
Rust structs or the project file: it exchanges **text in the file formats
above**.

| World (extension point) | Receives | Returns |
|---|---|---|
| `plugin` | — | `plugin-info` (name, version, description) |
| `io-plugin` (`io`) export | the **export model** JSON: `{"format": "ols-export-model", "version": 1, "crs", "line": <line file, structure types placed>, "cases": [{"name", "summary", "wires": [{"section", "span", "circuit", "phase", "color", "system", "voltage_kv", "horizontal_tension_n", "max_tension_n", "sag_m", "points": [[x,y,z]…]}]}]}`, plus options | file bytes |
| `io-plugin` import | file bytes, options (`into`: a line file to update) | a **line file** JSON, validated by the host like any line file |
| `terrain-plugin` (`terrain`) | file bytes | surfaces: `name`, `points` (x, y, z flat, m), `triangles` (indices) |
| `project-plugin` (`project`) | file bytes | `project { line: <line file JSON>, libraries: [{kind, name, text}], notes }`; `kind` as in `library_file` |
| `code-plugin` (`design-code`) | a parameter file (TOML text) | `generated { weather-cases: <.olswc text>, load-cases: <.olslc text>, notes, clearance-presets: [{ name, measure: vertical / radial, rules: <.olsr text>, weather-cases: <.olswc text> }] }` (interface 0.2) |
| `check-plugin` / `code-check-plugin` (`check`) | `checks()`: nothing; `value-tables` / `edit-value`: a values file's text (e.g. `.olspd`); `evaluate`: the values, the user's choices, a `conductor-pair` (same system / span, kV, plugin column values, altitude, diameters, masses, spacer sags) and per station 6 numbers (distance, system angle, both local sags, both string lengths) | the check list (id, title, menu; the app's Checks menu is built from it); tables; the edited file; per station the required distance and the check's own values, the rule applied |

So a plugin that writes a line follows the line file rules on this page:
global coordinates, ids unique per kind, sections and jumpers ending at
wire points with sets, `cable` / `insulator` / `type` naming library ids
that exist (or are returned with it, for a project plugin).

**Python** (`ols` module in the application, [Python
console](../user-guide/desktop-app.md#python-console)): `ols.line()`
gives the working line as a line-file dict; `ols.results(case)` the
solved spans; `ols.apply(command)` / `ols.batch(commands)` change the
project with the session commands above (validated, logged, undoable).
The standalone `ols` package (`crates/ols-py`) offers unit conversion
(`convert`, `quantity_kinds`); scripts outside the application read and
write the files above.
