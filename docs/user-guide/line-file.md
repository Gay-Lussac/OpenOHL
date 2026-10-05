# Line files (`.ols.json`)

A **line file** describes a line in 3D: structures with their attachment points, and the wire sections strung through them. It is JSON (the open exchange format of ADR-0004). Example: [`examples/synthetic-line/synthetic-line.ols.json`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/synthetic-line.ols.json).

```bash
cargo run -p ols-cli -- line examples/synthetic-line/synthetic-line.ols.json \
    --temperature 15 --wind-pressure 600 --ice-thickness 0 \
    --html line.html --spans-csv spans.csv --csv wires.csv
```

`--html` writes a standalone 3D viewer (rotate, pan, zoom, vertical exaggeration); `--spans-csv` one row per span (tensions, sag, mid-point, low point); `--csv` every wire point. More formats (KML/KMZ, CSV of every weather case, plugin formats) with `--export`: see [Import and export](import-export.md).

## Conventions

- Numbers are **SI** (metres, newtons, °C for temperatures).
- Coordinates are in the line's coordinate reference system (`crs`, e.g. `"EPSG:2056"`): x east, y north, z up (elevation).
- Wire properties come from the [wire libraries](wire-library.md) (`cable` = a cable id).

## Top level

| Field | Required | Meaning |
|---|---|---|
| `format` | no (`"ols-line"`) | File identifier. |
| `version` | no (`1`) | Format version; newer versions are rejected. |
| `name` | yes | Line name. |
| `crs` | no | Coordinate system, e.g. `"EPSG:2056"` (see [coordinate systems](coordinates.md)); used by `ols crs line` and exports. |
| `routes` | no | Routes: `{ "id", "name", "alignment": [points…], "structures": [ids…] }`. A structure may belong to several routes (Y pylons, parallel routes rejoining). Without routes, one implicit route runs through all structures in file order along `alignment`. Alignment points are `[x, y, z]` (free) or `{ "position": [x, y, z], "structure": "T12" }` (tied under a structure: it follows the structure; it must lie under it in plan, within 1 mm). |
| `alignment` | no | Alignment points (PIs) of the implicit route (files without `routes`). |
| `structures` | yes | List of structures. |
| `sections` | yes | List of wire sections. |
| `jumpers` | no | Jumpers between attachment points (below). |
| `systems` | no | Electrical systems: phases or poles with their voltage ([below](#electrical-systems)). |
| `display` | no | Display settings: `{ "phase_colors": { "L1": "#8b4513", … } }` — wire colours (`#rrggbb`) by section `phase_label`. |

## Structures

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id, referenced by sections. |
| `position` | yes | Structure origin `[x, y, z]` (centre at ground). |
| `orientation_deg` | no | Direction of the local x axis (right of the line), degrees CCW from east; used to place a structure type. |
| `type` | no | Structure type id from the [structure library](structure-library.md); attachments are then generated. |
| `extension` | no | Height extension of the type. |
| `attachments` | yes, unless `type` | Attachment points (below); with a `type` they override the type's attachments with the same set and phase. |
| `omit` | no | With a `type`: attachments of the type left out, as `[set, phase]` pairs (an explicit attachment with the same set and phase still counts). |
| `members` | no | Stick drawing: polylines of `[x, y, z]` points, for views and drawings only; from the structure type when it is placed. |

### Attachments

| Field | Required | Meaning |
|---|---|---|
| `set`, `phase` | yes | Identify the attachment (unique per structure). |
| `label` | no | Free text. |
| `insulator_point` | yes | Where the insulator assembly attaches to the structure `[x, y, z]`. |
| `wire_point` | yes | Where the wire is clamped `[x, y, z]`. |
| `insulator_weight` | no (0) | Insulator weight, N. |
| `insulator_wind_area` | no (0) | Area of the assembly the wind blows on, m² (from the insulator library when resolved); half its wind load goes on the clamp. |
| `insulator` | no | Insulator assembly id from the [insulator library](structure-library.md). Strain attachments are free members with or without one (below). |
| `legs` | no | V-string: the two leg attachment points `[[x,y,z], [x,y,z]]` on the structure. Both legs end at `wire_point` (which must hang below both); `insulator_point` is then informative (e.g. their centre). Generated from `v_string` assemblies for typed structures. |
| `top_label`, `bottom_label` | no | Labels of the string's top (insulator point) and bottom (wire point). |
| `strain_member` | no (false) | A strain string: in a section's middle or end it is solved as a free member along its conductor (like a library strain assembly) instead of holding the wire at `wire_point`. |

The **assembly type follows from the geometry**: in the middle of a section, an attachment with `legs` is a **V-string** (two tension-only legs of lengths `|wire_point − leg|` to one free clamp, `insulator_weight` split between them); otherwise a wire point *below* its insulator point, swung less than 45°, is a **suspension string** of length `|wire_point − insulator_point|` that swings freely when solved. The first and last points of every section, and other attachments, are **strain assemblies**: a free member of length `|wire_point − insulator_point|` hanging from the insulator point, lined up with its conductor by equilibrium, so it moves with every weather case (droops when the wire is hot, swings with the wind) and its pull is in the calculation. A strain assembly given only as geometry, without `insulator_weight` (imported lines), gets the weight that holds it where `wire_point` puts it in the section's stringing state (still air at the stringing temperature): the moment of the wire's pull at the clamp balances that of the weight at the member's middle; its unstressed length is shortened by its stretch under that pull. A string whose data droops less than the wire pulls it down cannot be held there by any weight: its weight is 0 and it lines up with the wire. A point whose wire and insulator points coincide (within 1 cm) is fixed there. On a 930-span real line, mid-spans match the source's reported points as with clamps held at their data positions (median 1.5 cm, p90 3.3 cm).

A V-string holds its clamp across the line while both legs are in tension; when the wind load leans further from vertical than the windward leg, the leeward leg goes **slack** and the clamp swings about the windward leg's top. `ols line` lists slack legs, and the viewer draws them in magenta.

```json
{ "set": 1, "phase": 1, "insulator_point": [310.0, -6.0, 535.0], "wire_point": [310.0, -6.0, 531.5],
  "legs": [[310.0, -9.0, 535.0], [310.0, -3.0, 535.0]], "insulator_weight": 1000.0 }
```

`cargo run -p ols-cli -- line FILE --stations` prints the stationing, offsets and line angles of every route.

## Sections

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id. |
| `cable` | yes | Cable id from the wire libraries. |
| `circuit`, `phase_label` | no | Labels. |
| `system` | no | Electrical system (id in `systems`) whose phase `phase_label` the section carries. Give it once per conductor path: sections joined by jumpers (or sharing an attachment) follow. None on the whole path: earthed (earth wire, OPGW). Independent of the cable. |
| `bundle` | no (1) | Subconductors per phase. Tension, sag and loads are per subconductor; the solve follows the bundle's centre line with each subconductor's share of the insulator weights (string forces are reported for the whole bundle). |
| `bundle_spacing` | no | Distance between neighbouring subconductors, m (e.g. `0.4` for 400 mm). They sit evenly on a circle around the centre line, across the wire — radius `s / (2·sin(π/n))` — so clearances are measured to the nearest subconductor. Without it a bundle is checked as one wire. |
| `bundle_rotation_deg` | no (0) | Turns the arrangement (counter-clockwise looking ahead). 0: the two lowest subconductors level — two side by side, three with one on top, four in a square. |
| `points` | yes | `{ "structure": id, "set": n, "phase": n }` in line order; spans are between consecutive points. |
| `stringing` | yes | See below. |
| `loads` | no | Point loads: `{ "span": index, "fraction": 0–1 (horizontal, from the back point), "vertical": N, "label": "…" }`. |
| `color` | no | Display colour `#rrggbb`, overriding the phase colour. |
| `calculation` | no (`"initial-fe"`) | How the section is solved: a cable condition and a method — `initial-rs`, `creep-rs`, `load-rs`, `max-sag-rs`, `initial-fe`, `creep-fe`, `load-fe`, `max-sag-fe`. See below. |

### Calculations: condition and method

| Condition | The cable |
|---|---|
| Initial | as strung |
| Creep | after its permanent stretch due to creep (years at the creep weather case) |
| Load | after its permanent stretch due to the heaviest of the load weather cases (the one stretching it most) |
| Max Sag | after creep or after load, whichever sags more in the weather case solved |

The weather cases come from the line's `creep_stretch` (Criteria › Creep-Stretch): `{ "creep_case": "everyday", "load_cases": ["glaze ice", "rime ice + wind"] }` (by name, from the weather case file used); how a cable stretches, from its model ([wire library](wire-library.md#cable-models)). A section after creep or load without its weather case fails, saying so.

| Method | |
|---|---|
| FE | the section's finite-element model: suspension strings swing, each span its own tension; after creep or load the unstressed lengths are longer by the permanent stretch |
| RS | ruling span: one horizontal tension for the whole section (the sag-tension section with the non-linear cable model and the permanent set), every span a catenary between its clamps held where they hang in still air at stringing, in the plane of its load (blown out by the wind); point loads are left out |

RS and FE agree to about 1 % (the FE strings swing to even out the spans' tensions: on the synthetic example 1.2 % of sag at 60 °C).

### Colours

Wires are drawn in the section's `color`, else the line's `display.phase_colors` entry for the section's `phase_label`, else a default by the phase number of the section's first point: phase 1 red `#e4572e`, 2 yellow `#f2c14e`, 3 blue `#4c9be8` (then repeating), phase 0 (e.g. shield wires) grey `#b0b8c0`. Utilities' conventions differ, so set them per project:

```json
"display": { "phase_colors": { "L1": "#8b4513", "L2": "#202020", "L3": "#808080" } }
```

### Stringing and status

A section is **in the blocks** (unclipped, the default for new sections) or **clipped in**. The `kind` says which.

In the blocks, the wire rolls over the suspension clamps (running blocks): in every weather case its unstressed length is shared out so that all its spans carry the **same horizontal tension** — ruling-span behaviour, computed exactly in 3D (point loads are left out of the sharing). Moving or changing a structure sags the section again to its condition. Two conditions:

```json
{ "kind": "tension", "horizontal_tension": 25000.0, "temperature": 15.0 }
```
Sagged to this horizontal tension (per subconductor) at this temperature, still air.

```json
{ "kind": "sag", "sag": 7.5, "temperature": 15.0 }
{ "kind": "sag", "sag": 9.2, "temperature": 15.0, "span": 2 }
```
Sagged to a sag (largest vertical distance below the chord, m) at a temperature, still air: in the ruling span \\( \sqrt{\sum S^3 / \sum S} \\) (level) without `span`, or in span `span` (0 = first). The tension giving that sag is used in every span.

In both, suspension clamps sit at their equilibrium position: swung across the line at line angles, plumb along it.

Clipped in, each span keeps its own unstressed length (per subconductor) at a temperature — the state after clipping in, or lengths from a previous design or another program:

```json
{ "kind": "unstressed_lengths", "lengths": [306.07, 330.73], "temperature": 15.0,
  "sagged": { "kind": "sag", "sag": 7.5, "temperature": 15.0 } }
```
In each weather case the suspension strings swing along the line to balance the spans, so tensions differ a little between spans. A structure moved changes the spans next to it; the strings beyond take up part of it, less and less along the section. `sagged` (optional) keeps the condition it was sagged to, to go back to the blocks. In the app, a section's window switches between *in the blocks* and *clipped in* (clipping in computes the lengths of its condition on the current geometry).

## Jumpers

A jumper is a slack cable joining two attachment points: the ends of two sections at a dead-end structure, or a tap from a branch to a through wire (the example joins the branch's first point at the Y pylon T3 to the main L1 wire).

```json
"jumpers": [
  { "id": "J-T3-branch", "cable": "acsr-drake-26-7",
    "from": { "structure": "T3", "set": 2, "phase": 1 },
    "to":   { "structure": "T3", "set": 1, "phase": 1 },
    "sag": 1.5, "temperature": 15.0 }
]
```

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id. |
| `cable` | yes | Cable id from the wire libraries. |
| `from`, `to` | yes | Attachments `{ "structure", "set", "phase" }`. |
| `length` | one of | Unstressed (cut) length, m, at `temperature`. |
| `sag` | one of | Or the largest vertical distance below the end-to-end chord, m, in still air at `temperature`; the cut length is found from it (jumpers without support). |
| `temperature` | no (15) | Temperature of `length` / `sag`, °C. |
| `support` | no | Jumper support string: `{ "point": [x,y,z], "length": m, "weight": N, "fraction": 0–1 }` — a tension-only string from `point` holding the jumper at `fraction` of its length from `from` (default 0.5). Needs `length`. |
| `bending_stiffness` | no | Bending stiffness EI of the cable, N·m²: where an end is a section's end clamp, the jumper leaves it in a set direction and bends away from it (below). Default: estimated from the cable, \\( EA\\,(D/6)^2/16 \\) (its wires bending each on its own, wire diameter D/6; Drake 26/7: ~48 N·m²). `0`: hinged ends. |
| `angle_from_deg`, `angle_to_deg` | no (30) | Angle at which the jumper leaves the clamp at `from` / `to`, below the conductor's axis, degrees (0: straight on along the conductor; negative: upwards). Only where that end is a section's end clamp. |
| `color` | no | Display colour; default: the colour of the section through `from`. |

**Clamped ends.** Where an end is the first or last point of a section, the clamp holds the jumper: it leaves along the conductor's axis there (the span's tangent at the clamp, continued) turned down by `angle_from_deg` / `angle_to_deg`, and the cable's bending stiffness carries that direction into the jumper. The jumper then does not swing in the wind as if hinged at its ends, and follows its clamps as they move with the sections. It is solved as an inextensible elastic rod in pieces of ~0.12 m (16 to 40; its stretch, ~10⁻⁶ at a jumper's tension, is left out); a support string is then rigid, and a string whose top is out of reach of the point it holds is an error. A `sag` is the clamped jumper's sag in still air. A jumper shorter than the distance its clamps swing apart to (free strain strings in the wind) fails in that weather case, said so ("taut"): its pull holding the clamps back is not modelled yet. An end that is not a section's end (a tap on a through wire) stays hinged; with both ends hinged, or `bending_stiffness` 0, the jumper is the catenary described next.

How much the clamps hold a jumper back depends on their angles: a clamp along the chord between the ends (a straight dead-end with terminals straight on) does not resist a swing about that chord at all, and a jumper leaving its clamps upwards (terminals straight on along spans rising to the structure) swings further than a hinged one, since the bend at the clamps eases as it swings out. Terminals angled down (the default 30°) hold it back.

Each end is the **solved clamp** of the section through that attachment (so the jumper follows strain assemblies and swung suspension clamps), or the attachment's `wire_point` if no section uses it. The jumper is one exact catenary element (two with a support string) carrying the weather case's weight, ice and wind (horizontal, perpendicular to the jumper's chord); its length scales with \\( e^{\alpha \Delta t} \\) from `temperature`. Sections are solved first: a jumper's tension (a few hundred N) does not change them. `ols line` prints each jumper's length, sag and tension, includes jumpers in the ground clearance check (as `jumper:<id>`) and draws them in the viewer. Clearance from jumpers to the structure body comes with structure outlines.


A jumper is drawn and checked as a bundle like its section: `bundle`, `bundle_spacing` and `bundle_rotation_deg` default to those of the section at `from` (the first section through that point) and can be set on the jumper, each on its own (e.g. only a different spacing). A support string's weight is shared by the subconductors.

## Electrical systems

The electrical model is kept apart from the physical wires: a **system** is a group of phases (AC) or poles (DC) with its voltage, and sections point to it.

```json
"systems": [
  { "id": "380-1", "name": "380 kV Alpha–Beta 1", "kind": "ac",
    "voltage_kv": 380, "frequency_hz": 50,
    "phases": [{ "label": "L1", "angle_deg": 0 }, { "label": "L2", "angle_deg": -120 },
               { "label": "L3", "angle_deg": 120 }] },
  { "id": "sbb", "name": "132 kV railway", "voltage_kv": 132, "frequency_hz": 16.7,
    "phases": [{ "label": "UU", "angle_deg": 0 }, { "label": "VV", "angle_deg": 180 }] },
  { "id": "hvdc", "kind": "dc", "voltage_kv": 500,
    "phases": [{ "label": "+", "polarity": 1 }, { "label": "-", "polarity": -1 },
               { "label": "MR", "polarity": 0 }] }
]
```

| Field | Required | Meaning |
|---|---|---|
| `id`, `name` | id | Sections refer to the id. |
| `kind` | no (`ac`) | `ac` or `dc`. |
| `voltage_kv` | yes | AC: nominal voltage between phases (rms). DC: pole voltage to earth. |
| `frequency_hz` | no | AC frequency (50, 60, 16.7…). Systems of different frequencies are not synchronous. |
| `phases` | yes | `label` (matched with the sections' `phase_label`); AC `angle_deg` (default evenly spread: 0°, −120°, +120°; two phases 0°, 180°); DC `polarity` +1, −1 or 0 (default in that order). |

**Conductor paths.** Sections joined by jumpers, or sharing an attachment point, form one conductor path. Setting `system` on one section of the path assigns the whole path to that system's phase `phase_label`; two sections of a path naming different phases is an error that names them. A path without a system is earthed.

**Voltages.** With `n` phases evenly spread, a phase is at `U / (2·sin(π/n))` to earth (`U/√3` for three phases, `U/2` for two). Between two conductors: the phasor difference for AC of the same frequency (`U` between two phases of a system, 0 between the same phase of two parallel systems), the difference of pole voltages for DC, and the sum of the voltages to earth otherwise (different frequencies, AC with DC). Clearance rules can use the nominal voltage or this voltage between conductors ([clearances by voltage](terrain.md#clearances-by-voltage)).

Electrical properties of the cables (resistance, ampacity data) belong to the [wire library](wire-library.md), linked through each section's `cable`.

## How a section is solved

Every span is one exact elastic catenary element (split at point loads), suspension strings and V-string legs are tension-only truss elements, wind acts horizontally and perpendicular to each span, and the free clamps and load points are found by Newton–Raphson in 3D (see [finite-element wire systems](../theory/wirefe.md)). Each span's unstressed length scales with \\( e^{\alpha \Delta t} \\) from the stringing temperature.

## Checked against PLS-CADD

On a real 78-structure, 930-span line exported from PLS-CADD (private data, compared locally and not part of the repository), OLS CAD reproduced PLS-CADD's mid-span heights to a median of 1.1 cm (90 % within 3.4 cm) for unloaded spans and 1.7 cm (90 % within 3.9 cm) for spans with marker balls, and mid-span positions to 1 mm, solving all 554 sections in under 20 ms. The largest differences (up to 21 cm) are nearly vertical, nearly taut down-leads, where PLS-CADD's 1 cm rounding of unstressed lengths moves the sag by that much.
