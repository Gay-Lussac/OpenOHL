# Line files (`.ols.json`)

A **line file** describes a line in 3D: structures with their attachment points, and the wire sections strung through them. It is JSON (the open exchange format of ADR-0004). Example: [`examples/synthetic-line/synthetic-line.ols.json`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/synthetic-line.ols.json).

```bash
cargo run -p ols-cli -- line examples/synthetic-line/synthetic-line.ols.json \
    --temperature 15 --wind-pressure 600 --ice-thickness 0 \
    --html line.html --spans-csv spans.csv --csv wires.csv
```

`--html` writes a standalone 3D viewer (rotate, pan, zoom, vertical exaggeration); `--spans-csv` one row per span (tensions, sag, mid-point, low point); `--csv` every wire point.

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
| `crs` | no | Coordinate system, informative (e.g. `"EPSG:2056"`). |
| `routes` | no | Routes: `{ "id", "name", "alignment": [points…], "structures": [ids…] }`. A structure may belong to several routes (Y pylons, parallel routes rejoining). Without routes, one implicit route runs through all structures in file order along `alignment`. Alignment points are `[x, y, z]` (free) or `{ "position": [x, y, z], "structure": "T12" }` (tied under a structure: it follows the structure; it must lie under it in plan, within 1 mm). |
| `alignment` | no | Alignment points (PIs) of the implicit route (files without `routes`). |
| `structures` | yes | List of structures. |
| `sections` | yes | List of wire sections. |
| `jumpers` | no | Jumpers between attachment points (below). |
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

### Attachments

| Field | Required | Meaning |
|---|---|---|
| `set`, `phase` | yes | Identify the attachment (unique per structure). |
| `label` | no | Free text. |
| `insulator_point` | yes | Where the insulator assembly attaches to the structure `[x, y, z]`. |
| `wire_point` | yes | Where the wire is clamped `[x, y, z]`. |
| `insulator_weight` | no (0) | Insulator weight, N. |
| `insulator` | no | Insulator assembly id from the [insulator library](structure-library.md). With an assembly, a strain attachment is a free member; without, its clamp stays at `wire_point`. |
| `legs` | no | V-string: the two leg attachment points `[[x,y,z], [x,y,z]]` on the structure. Both legs end at `wire_point` (which must hang below both); `insulator_point` is then informative (e.g. their centre). Generated from `v_string` assemblies for typed structures. |

The **assembly type follows from the geometry**: in the middle of a section, an attachment with `legs` is a **V-string** (two tension-only legs of lengths `|wire_point − leg|` to one free clamp, `insulator_weight` split between them); otherwise a wire point *below* its insulator point, swung less than 45°, is a **suspension string** of length `|wire_point − insulator_point|` that swings freely when solved. The first and last points of every section, and other attachments, are **strain points**: a free member of their length when they reference an insulator assembly, otherwise fixed at `wire_point`.

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
| `bundle` | no (1) | Subconductors per phase; geometry and tension are per subconductor. |
| `points` | yes | `{ "structure": id, "set": n, "phase": n }` in line order; spans are between consecutive points. |
| `stringing` | yes | See below. |
| `loads` | no | Point loads: `{ "span": index, "fraction": 0–1 (horizontal, from the back point), "vertical": N, "label": "…" }`. |
| `color` | no | Display colour `#rrggbb`, overriding the phase colour. |

### Colours

Wires are drawn in the section's `color`, else the line's `display.phase_colors` entry for the section's `phase_label`, else a default by the phase number of the section's first point: phase 1 red `#e4572e`, 2 yellow `#f2c14e`, 3 blue `#4c9be8` (then repeating), phase 0 (e.g. shield wires) grey `#b0b8c0`. Utilities' conventions differ, so set them per project:

```json
"display": { "phase_colors": { "L1": "#8b4513", "L2": "#202020", "L3": "#808080" } }
```

### Stringing

```json
{ "kind": "tension", "horizontal_tension": 25000.0, "temperature": 15.0 }
```
Every span is sagged to this horizontal tension (per subconductor) at this temperature, with suspension clamps at their equilibrium position (swung sideways at line angles, plumb along the line).

```json
{ "kind": "unstressed_lengths", "lengths": [306.07, 330.73], "temperature": 15.0 }
```
Unstressed length of each span (per subconductor) at a temperature — e.g. from a previous design or another program.

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
| `color` | no | Display colour; default: the colour of the section through `from`. |

Each end is the **solved clamp** of the section through that attachment (so the jumper follows strain assemblies and swung suspension clamps), or the attachment's `wire_point` if no section uses it. The jumper is one exact catenary element (two with a support string) carrying the weather case's weight, ice and wind (horizontal, perpendicular to the jumper's chord); its length scales with \\( e^{\alpha \Delta t} \\) from `temperature`. Sections are solved first: a jumper's tension (a few hundred N) does not change them. `ols line` prints each jumper's length, sag and tension, includes jumpers in the ground clearance check (as `jumper:<id>`) and draws them in the viewer. Clearance from jumpers to the structure body comes with structure outlines.

## How a section is solved

Every span is one exact elastic catenary element (split at point loads), suspension strings and V-string legs are tension-only truss elements, wind acts horizontally and perpendicular to each span, and the free clamps and load points are found by Newton–Raphson in 3D (see [finite-element wire systems](../theory/wirefe.md)). Each span's unstressed length scales with \\( e^{\alpha \Delta t} \\) from the stringing temperature.

## Checked against PLS-CADD

On a real 78-structure, 930-span line exported from PLS-CADD (private data, compared locally and not part of the repository), OLS CAD reproduced PLS-CADD's mid-span heights to a median of 1.1 cm (90 % within 3.4 cm) for unloaded spans and 1.7 cm (90 % within 3.9 cm) for spans with marker balls, and mid-span positions to 1 mm, solving all 554 sections in under 20 ms. The largest differences (up to 21 cm) are nearly vertical, nearly taut down-leads, where PLS-CADD's 1 cm rounding of unstressed lengths moves the sag by that much.
