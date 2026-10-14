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

All examples on this page are invented (structures `T1`, `T2`, sections
`S1`, a system `sys-A`, …).

## Overview

```text
                                  LINE  (.ols.json / OLS XML / project rows)
 ┌──────────────────────────────────────────────────────────────────────────────────┐
 │ meta: name, crs, display, systems[], creep_stretch, bimetallic,                  │
 │       circuit_starts[]                                                           │
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
 │   └─ stringing, loads[] (SpanLoad), calculation                   │      │  │    │
 │                                                                   │      │  │    │
 │  Jumper ── from / to : SectionPoint ─► WirePoint.set              │      │  │    │
 │   │  cable ───────────────────────────────────────────────────────┤      │  │    │
 │   └─ supports[] : JumperClamp {structure?, wire ─► WirePoint.label}      │  │    │
 └───────────────────────────────────────────────────────────────────┼──────┼──┼────┘
                                                                     ▼      ▼  ▼
                         LIBRARIES (TOML, units in the values)   .olsw   .olsi .olss
   .olswc weather cases ◄── creep_stretch names, LoadCase.weather          ▲     │
   .olslc load cases                                                       └─────┘
                                                        TypeString.insulator ─► .olsi id
   .olss StructureType: point[], wire[], string[], extension[], node[], member[],
                        section[], material[], load[], capacity, wind, model ─► .olsfe,
                        pole ─► .olsp, template (built from ─► .olspt)
   .olspt PylonTemplate: parameter[], attachment[] (slot, across, up),
                        earth[], strings ─► builds a level-1 .olss
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
  reading (`ols_core::units`). A bare number is a factor or a count.
- Inside the program every quantity is SI ([Conventions and
  units](../theory/conventions.md)).

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
| `version` | u32 | — | file version (now 4; versions 1–3 are read and upgraded) | — |
| `name` | string | — | line name | — |
| `crs` | string | — | coordinate reference system, e.g. an EPSG code (informative) | — |
| `routes` | `[Route]` | — | routes: paths of the line, made of alignments | — |
| `alignments` | `[Alignment]` | — | alignments, in order; each on one route, through point ids | route ids, point ids |
| `alignment_points` | `[AlignmentPoint]` | — | alignment points (PIs), each on any number of alignments | structure ids |
| `structures` | `[Structure]` | — | towers and poles | — |
| `sections` | `[Section]` | — | wire sections | structures + sets |
| `jumpers` | `[Jumper]` | — | jumpers between wire points | structures + sets |
| `systems` | `[System]` | — | electrical systems | — |
| `display` | `Display` | — | phase colours, `ahead_deg` | phase labels |
| `creep_stretch` | `CreepStretch` | — | weather cases for creep / load stretch | weather case **names** |
| `bimetallic` | `BimetallicModel` | Pa | outer-strand behaviour of bimetallic cables above the transition temperature, for cables that do not say | — |
| `circuit_starts` | `[SectionPoint]` | — | circuit ends that are starts (power flows in) | structures + sets |
| `condition_cases` | — | — | **not stored**: the weather cases named by `creep_stretch`, set by the program solving the line | — |

**Plugin values** (`fields`, `ols_line::Fields`): structures, sections,
jumpers, systems, alignments and routes carry a map `plugin.column →
text`, the values of the table columns plugins add (`[[column]]` in a
plugin's manifest, [Writing plugins](plugins.md#table-columns)). Empty
maps are not written. Kept with the object through every edit, written
to OLS XML as `<field key value>`; shown (unused) where the plugin is not
installed. A circuit's values are on all its sections. Known keys:
`lev-ch.insulation_level` (`low` / `high`, the circuits' LeV insulation
level, read by `ols_clearance::phase_distance`).

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

A structure without a type holds its own `points`, `wires`, `strings` and `members`; the desktop app edits them as a structure type in local axes (`to_local`/`to_global`) and writes them back with one `Put`. *Make it a type* writes that document as a `.olss`, sets `type` and empties the structure's own lists. A type built from a pylon template records it in `[template]` (`id`, `role` suspension/strain, `string_length`, `string_weight`, `values` by parameter) and has sets from the template's slots: suspension = slot, strain back = slot + `back_offset` (20), ahead = slot + `ahead_offset` (40).

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
wire points are placed from the strings. In a weather case the jumpers'
weight hangs on the clamps of the sections at their ends
(`jumper::end_weights`: half of the jumper between an end and its first
support; with ice) — not at stringing. The resolved line is what solvers and exports use; the stored
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
| `circuit` | string | — | circuit name (free label) | — |
| `phase_label` | string | — | phase label, e.g. `L1` | `SystemPhase.label`, `Display.phase_colors` key |
| `system` | string? | — | system this conductor path carries; given once per path, the other sections of the path follow; none on the whole path: earthed | `System.id` |
| `bundle` | u32 | — | subconductors (1: single wire) | — |
| `bundle_spacing` | f64? | m | distance between neighbouring subconductors | — |
| `bundle_rotation_deg` | f64? | ° CCW looking ahead | rotation of the arrangement | — |
| `points` | `[SectionPoint]` | — | attachment points in line order; first and last are strain points | structure id + set |
| `stringing` | `Stringing` | — | how it is strung (below) | — |
| `loads` | `[SpanLoad]` | — | point loads in spans | span index |
| `color` | string? | — | `#rrggbb`, overrides the phase colour | — |
| `calculation` | `Calculation` | — | condition and method, kebab-case (`initial-fe` default, `creep-rs`, `max-sag-fe`, …) | — |

`SectionPoint { structure, set }` names the wire point with set `set` on
structure `structure`. Spans are between consecutive points.

`Stringing` is tagged by `kind`:

| `kind` | Fields | Meaning |
|---|---|---|
| `tension` | `horizontal_tension` (N per wire), `temperature` (°C) | in the blocks: one horizontal tension in every span |
| `sag` | `sag` (m), `temperature` (°C), `span` (index, optional) | in the blocks: sagged in the ruling span or in span `span` |
| `unstressed_lengths` | `lengths` (m, one per span), `temperature` (°C), `sagged` (optional nested `Stringing`) | clipped in: each span keeps its own unstressed length |

`SpanLoad`: `span` (index), `fraction` (0–1 from the back point),
`vertical` (N, whole bundle), `label`, `diameter` (m, sphere), `drag`,
`wind_area` (m²), `ice_area` (m²).

### Jumper and JumperClamp (`lib.rs`, `jumper.rs`)

| Field | Type | Unit | Meaning | References |
|---|---|---|---|---|
| `id` | string | — | unique id | — |
| `description` | string | — | free text | — |
| `cable` | string | — | cable | `.olsw` cable `id` |
| `from`, `to` | `SectionPoint` | — | the two ends (different wire points) | structure id + set |
| `length` | f64? | m | cut length at `temperature` | — |
| `sag` | f64? | m | or: sag below the chord in still air (no supports) | — |
| `temperature` | f64 | °C | of `length` / `sag` | — |
| `rigid` | bool | — | a tube or bar: no length, sag or supports | — |
| `support` | `JumperSupport`? | — | one support string from a point: `point` (m), `length` (m), `weight` (N), `fraction` (0–1) | — |
| `supports` | `[JumperClamp]` | — | or: support clamps on a structure's strings, in order from `from` | — |
| `bending_stiffness` | f64? | N·m² | EI; default estimated from the cable; 0: hinged | — |
| `stiffness_factor` | f64? | 0–1 | share of the solid-bar EI for the estimate | — |
| `angle_from_deg`, `angle_to_deg` | f64? | ° | exit angle at a section's end clamp from its tension string's line (clamp → top; none: the conductor's axis), turned towards the case's load in the conductor's plane (−90…90) | — |
| `color`, `bundle`, `bundle_spacing`, `bundle_rotation_deg` | optional | — | default: those of the first section through `from` (`Line::jumper_section`) | — |

`JumperClamp`: `structure` (optional; default the structure of the
jumper's `from`), `wire` (label of a wire point **without a set**, held by
one string — an I — or two — a V — from structure points), `at` (0–1 of
the jumper's length), `clamp_length` (m, default 0.1, at most 1).

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
| JumperClamp | `structure`, `wire` | WirePoint without a set | `id`, label |
| Line | `circuit_starts[]` | WirePoint at a circuit end | structure `id` + `set` |
| Line | `creep_stretch` | WeatherCase (`.olswc`) | `name` |
| LoadCase (`.olslc`) | `weather` | WeatherCase | `name` |
| StructureType | `capacity.attachment[].point` | TypePoint | label |
| TypePoint | `node` / `member` | TypeNode / TypeMember | `id` |
| TypeMember | `nodes`, `section` | TypeNode, `[[section]]` | `id` |
| StructureType | `model`, `pole` | `.olsfe` file (relative path), `.olsp` id | — |

Labels are scoped to their structure (or type); ids are unique per kind in
the line; library ids are unique per library. A typed structure's sets and
labels may come from its type: references to them are checked after
`Line::resolve`.

### Ownership and deletion

A structure **owns** its points, wire points, strings and stick drawing;
a section owns its stringing and span loads; an alignment owns its points.
Everything else is a reference. Deletion goes through the session
(`crates/ols-session/src/lib.rs`, `Command::Delete`): when an object takes
others with it, the command is refused with the list
(`SessionError::NeedsConfirmation`) unless `confirm` is set, and then all
of it is one log entry, undone together.

| Deleting | Also deletes or changes |
|---|---|
| Structure | its own items; every **section** through it; every **jumper** at it, held by its strings (`supports`), or at a point of those sections; alignment points tied to it are **detached** (they stay in place) |
| Wire point with a set (`delete_attachment`) | its strings and the set-less wire points only it uses (own items removed; a type's items added to `omit_sets`); the sections through it; the jumpers at it or at those sections |
| Section | the jumpers at any of its points |
| Route | its alignments on no other route; it is taken out of the alignments, points and structures that stay |
| Alignment (also: its last point) | its points (asked first) |
| Jumper | nothing else |
| Meta (line settings) | cannot be deleted |

Renames follow references: `rename` of a structure updates section
points, jumper ends and tied alignment points; of a route, the alignments,
points and structures on it;
`rename_attachment` (set renumbering) updates the sections and jumpers
there; `rename_system` updates the sections given it. Library items are
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

Other data files: structure models `.olsfe` (`ols-structure-model`) and
poles `.olsp` (`ols-pole`) next to the `.olss`
([Structure analysis](../user-guide/structure-analysis.md)); clearance
rules `.olsr`; thermal ambient `.olsta`; unit costs `.olscost`; galloping
`.olsg`; LiDAR class schemes `.olsc`; phase distance codes `.olspd`.

### Phase distance codes (`.olspd`)

`ols-phase-distance`, version 1 (`ols_clearance::phase_distance`): a
code's values for the distance between conductors in the span,
\( z = k\sqrt{f + l_k} + C \) — `[[voltage]]` rows (`kv`: up to this
nominal voltage; `c1`, `c2`: one value, or two — low and high
insulation level), `[[diagram]]`s (`name`, `wind_pressure`, `line =
[{ w_max, k0, k90 }]`: k linear in the system angle per swing class
w = d/q), `[altitude]` (`from`, `per_1000_m`). The Swiss LeV's values:
`libraries/phase-distance/lev-ch.olspd`. The check pairs conductors
between the same two structures (and, when asked, nearby conductors
elsewhere within a distance) and takes l_k from the strings at their
ends (I: its length; V, tension, clamped: 0; the string below a V: its
length; a TAK: its suspension string's), and each circuit's insulation
level from its sections' `lev-ch.insulation_level` (not set: high); the
settings (diagram, station spacing, circuits ticked, nearby pairs and
their distance) are not in the file. Results per pair: the worst
station (points, where along each conductor, d, z and its terms) and
the length of conductor below z. See [the specification](lev-phase-spacing.md).

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
- The desktop Vertical and Radial clearance windows keep their rules next
  to the model, in `<model>.vertical-clearances.olsr` and
  `<model>.radial-clearances.olsr` (one `[[rule]]` per row and target
  kind). Rules refer to LiDAR classes by number (`.olsc` schemes) and to
  weather cases by name.

### Structure type (`.olss`)

Source: `crates/ols-structures/src/structures.rs`. All positions in
**local axes**. Full reference: [Structure library](../user-guide/structure-library.md).

| Field (TOML key) | Type | Unit | Meaning |
|---|---|---|---|
| `format`, `version` | optional | — | `"ols-structure"`, 1 or 2 |
| `id`, `name` | string | — | unique id (referred to by `Structure.type`), display name |
| `kind` | `lattice` / `pole` / `portal` / `other` | — | family |
| `height` | length? | m | mast top for the default stick drawing |
| `foot` | length? | m | local z of the feet (default 0: ground) |
| `source` | string? | — | where the data comes from |
| `[[point]]` | `TypePoint` | m | `label`, `description`, and `point` **or** `node` (joint id) **or** `member` + `at` (0–1) |
| `[[wire]]` | `TypeWire` | m, N | `label`, `description`, `set?`, `point?` (else it hangs below its strings), `weight?` |
| `[[string]]` | `TypeString` | m, N, m² | `label`, `description`, `top`, `wire`, `insulator?` **or** `length` + `weight`, `compression?`, `wind_area?`, `strain`, `angle?` (from straight down), `direction?` (in plan, 0 ahead); `side` (`ahead`/`back`) in files written before: level, 0° or 180° |
| `[[attachment]]` | `TypeAttachment` | — | version 1: `set`, `phase`, `point`, `insulator` or inline `string`, `side` |
| `[[extension]]` | `name`, `dz` | m | height extensions |
| `[[body]]` | `points` | m | outline polylines (stick drawing) |
| `[[node]]` | `id`, `point`, `support?`, `spring?`, `attachment?` | m | pylon joints (detail 2–3) |
| `[[member]]` | `id`, `nodes` (2 ids), `section?`, `kind?` (`truss`/`frame`/`cable`), `reference?`, `roll?`, `k?`, `pretension?` | — | pylon members |
| `[[section]]`, `[[material]]`, `[analysis]`, `[[load]]` | tables | — | read by `ols_structures_fe` for the finite-element check |
| `capacity` | `spans[]` (by line angle: `wind_span`, `weight_span`, `min_weight_span`), `attachment[]` (`point`, `vertical`, `uplift`, `transverse`, `longitudinal`, `interaction`) | m, N | what it can carry (structure axes) |
| `wind` | `band[]`: `from`, `to`, `area`, `drag` | m, m² | wind on the structure itself |
| `model` | path? | — | separate `.olsfe` model, relative to this file |
| `pole` | id? | — | `.olsp` file in the same folder |

**Detail level** (`StructureType::detail`, never stored — an older
`detail` key is read and ignored): 3 *full design* when `model` is given
or every member has a section and a joint is supported; 2 *outline* when
there are members or body polylines; else 1 *stick figure*. Checks that
depend on the structure's shape must say what they do at each level
([Structure detail levels](structure-detail.md)).

Placed into a line, a type becomes `PlacedStrings { points, strings,
wires }` (global coordinates) that `Line::resolve` merges with the
structure's own items.

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
| `ols_meta(key, value)` | settings: `session_schema` (3), `grid_size`, the weather file used, … |
| `session_user(name, role)` | `viewer`, `designer`, `admin` |
| `model_object(line, kind, id, ord, version, json)` | one row per object of a project line; `kind` is `meta`, `route`, `alignment`, `alignment_point`, `structure`, `section`, `jumper` (`ols_session::Kind`) — routes, alignments and alignment points are three kinds of row; `json` as in the line file; `meta` (id `line`) holds `name`, `crs`, `display`, `systems`, `creep_stretch`, `bimetallic`, `circuit_starts` (projects before routes and alignments also an `alignment`, converted once when they open) |
| `oplog(seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by, note, line)` | every applied command with its inverse; `kind` is `do`, `undo`, `redo`, `rollback` or `import` |
| `line_snapshot` | named saved lines |
| `library_file(kind, name, user, text)` | the project's library files; `kind` `cables`, `structures`, `insulators`, `weather`, `loadcases`; not logged, not undone |
| `spatial_index` | plan bounds and tile of every object (ADR-0008) |
| `vc_*`, `edit_grant`, `surface`, point cloud tables | versions and reviews, edit rights, terrain surfaces, LiDAR ([Versions](../user-guide/versions.md), ADR-0007) |

### Commands (the only way to change a project)

`ols_session::Command`, JSON tagged by `op` (snake_case): `put` (`kind`,
`id`, `value` as in the line file, `ord`), `delete` (`kind`, `id`,
`confirm`), `move_structure`, `set_stringing`, `move_alignment_point`,
`insert_alignment_point` (a new point, or an existing `point`: shared),
`remove_alignment_point` (deletes the point: off every alignment),
`tie_alignment_point`, `detach_alignment_point`,
`align_under_structures`, `delete_structure`, `delete_attachment`
(`structure`, `set`), `rename`, `rename_attachment`, `rename_system`,
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

A project keeps its libraries in `library_file`; on opening, kinds it
lacks are copied in from these places, and the model reads working copies
written to a temporary folder. Per-user view settings (`ViewPrefs`) stay
on the computer, never in the project.

## What a plugin sees

Plugin tiers and manifests: [Writing plugins](plugins.md) and ADR-0003.
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
| `code-plugin` (`design-code`) | a parameter file (TOML text) | `generated { weather-cases: <.olswc text>, load-cases: <.olslc text>, notes }` |

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
