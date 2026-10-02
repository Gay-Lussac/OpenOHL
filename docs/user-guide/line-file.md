# Line files (`.ols.json`)

A **line file** describes a line in 3D: structures with their attachment points, and the wire sections strung through them. It is JSON (the open exchange format of ADR-0004). Example: [`examples/line/synthetic-line.ols.json`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/line/synthetic-line.ols.json).

```bash
cargo run -p ols-cli -- line examples/line/synthetic-line.ols.json \
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
| `alignment` | no | Alignment points (PIs) `[x, y, z]`, in order. |
| `structures` | yes | List of structures. |
| `sections` | yes | List of wire sections. |

## Structures

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id, referenced by sections. |
| `position` | yes | Structure origin `[x, y, z]` (centre at ground). |
| `orientation_deg` | no | Orientation of the transverse axis, informative. |
| `attachments` | yes | Attachment points (below). |

### Attachments

| Field | Required | Meaning |
|---|---|---|
| `set`, `phase` | yes | Identify the attachment (unique per structure). |
| `label` | no | Free text. |
| `insulator_point` | yes | Where the insulator assembly attaches to the structure `[x, y, z]`. |
| `wire_point` | yes | Where the wire is clamped `[x, y, z]`. |
| `insulator_weight` | no (0) | Insulator weight, N (suspension strings). |

The **assembly type follows from the geometry**: a wire point *below* its insulator point, swung less than 45°, is a **suspension string** of length `|wire_point − insulator_point|` that swings freely when solved; any other attachment (and the first and last point of every section) is a **strain point**: the wire is fixed at `wire_point`.

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

### Stringing

```json
{ "kind": "tension", "horizontal_tension": 25000.0, "temperature": 15.0 }
```
Every span is sagged to this horizontal tension (per subconductor) at this temperature, with suspension clamps at their equilibrium position (swung sideways at line angles, plumb along the line).

```json
{ "kind": "unstressed_lengths", "lengths": [306.07, 330.73], "temperature": 15.0 }
```
Unstressed length of each span (per subconductor) at a temperature — e.g. from a previous design or another program.

## How a section is solved

Every span is one exact elastic catenary element (split at point loads), suspension strings are truss elements, wind acts horizontally and perpendicular to each span, and the free clamps and load points are found by Newton–Raphson in 3D (see [finite-element wire systems](../theory/wirefe.md)). Each span's unstressed length scales with \\( e^{\alpha \Delta t} \\) from the stringing temperature.

## Checked against PLS-CADD

On a real 78-structure, 930-span line exported from PLS-CADD (private data, compared locally and not part of the repository), OLS CAD reproduced PLS-CADD's mid-span heights to a median of 1.1 cm (90 % within 3.4 cm) for unloaded spans and 1.7 cm (90 % within 3.9 cm) for spans with marker balls, and mid-span positions to 1 mm, solving all 554 sections in under 20 ms. The largest differences (up to 21 cm) are nearly vertical, nearly taut down-leads, where PLS-CADD's 1 cm rounding of unstressed lengths moves the sag by that much.
