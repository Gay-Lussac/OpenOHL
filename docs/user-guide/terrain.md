# Terrain and ground clearance

OLS CAD builds a ground surface from survey points and checks how far every
solved wire is above it.

```bash
cargo run -p ols-cli -- line examples/synthetic-line/synthetic-line.ols.json --temperature 80 \
  --terrain examples/synthetic-line/synthetic-survey.csv --clearance 7 --html line.html
```

```text
terrain: 2960 points, 5694 triangles — loaded and 10 spans checked in 7.7ms
  lowest vertical ground clearance 4.64 m: section S2 span 1 at (464.0, 6.7)
  spans below the required 7.00 m: 1
    section S2         span   1:   4.64 m at (464.0, 6.7, wire 512.92 m)
```

| Option | Meaning |
|---|---|
| `--terrain FILE` | Survey points (below). Enables the clearance check; the viewer draws the ground. |
| `--clearance M` | Required vertical ground clearance, m: spans below it are listed and marked in the viewer. |
| `--rules FILE` | Clearance rules (`.olsr`, below): vertical and radial clearances to the ground, LiDAR classes, other lines and other wires of this line. `--clearance M` adds a vertical ground rule. |
| `--other-line FILE` | Another line (`.ols.json`) for wire-to-wire clearances; repeatable. Solved in the same weather case with the same libraries. |
| `--clearance-csv FILE` | Every clearance found: `rule,measure,section,span,other_section,other_span,clearance_m,required_m,violations,wire_x,…,target_x,…,class`. |

Run it once per weather case that governs clearance (e.g. maximum operating
temperature, ice), with `--temperature`, `--ice-thickness` and
`--wind-pressure`. Blown-out wires are checked over the ground they are blown
over.

## Plan and profile drawings

`--drawings DIR` writes, for every route, a plan and profile sheet `ROUTE.svg` and the profiles as `ROUTE-profile.csv`:

```bash
ols line project.ols --pointcloud project.ols --temperature 80 --rules rules.olsr \
  --clearance 7 --drawings sheets --side-offset -15 --side-offset 15
```

- **Profile**: ground along the route centreline (exact TIN profile), side ground profiles at `--side-offset` (right positive), the required-clearance line (ground + `--clearance`), structures with their stations, the route's wires and jumpers coloured by phase, and violation markers.
- **Plan**: straightened along the route (station × offset at true scale), aligned with the profile.
- Scales `--hscale 2000 --vscale 200` (1:2000 / 1:200 by default); the plan across the line `--plan-scale 1000` (default: the horizontal scale — along the line the plan always keeps the horizontal scale to stay aligned with the profile); plan half width `--plan-width 40`.
- **Sheets**: `--page A1` (A0–A4 landscape, or `WIDTHxHEIGHT` in mm) splits each route **at structures** — as many whole spans as fit per sheet, the structure ending a sheet starting the next (match line) — into `ROUTE-sheet-NN.svg`, each with its own height range and a title block (line, route, sheet n/N, station range, structures, scales, format, weather case). A span longer than the sheet or a profile taller than it is drawn anyway and reported as a warning: change the scales or the format.
- **Logos**: `--logo company.svg:30` (SVG, PNG or JPEG, width in mm; repeatable) in the title block.

The sheet layout (title block, margins) is built in for now; company templates will come as a layout plugin. For DXF, PDF, PNG and XML, station ranges and the profile alone, see [Drawings and profile exports](drawings.md) (`ols drawing`).

## Clearance rules (`.olsr`)

Every rule names a **target** and the required **vertical** and/or **radial** clearance:

- **vertical**: to the ground, the wire's height above it directly below (negative: the wire is below the ground); to points, the vertical separation from points within `vertical_band` (default 0.5 m) of the wire in plan, above or below it; to other wires, the vertical separation where the wires cross in plan.
- **radial**: the shortest 3D distance between the wire and the target.
- **Bundles** (sections with `bundle_spacing`): to the nearest subconductor — the lowest one for vertical clearances below the wire, the highest above it, each subconductor for radial clearances.

```toml
format = "ols-clearance-rules"
version = 1
name = "Example rules"

[[rule]]
name = "Ground"
target = "ground"
vertical = "7 m"
radial = "6.5 m"

[[rule]]
name = "Vegetation"
target = "points"            # LiDAR points, by role and/or class number
roles = ["vegetation"]
classes = [99]
vertical = "5 m"
radial = "4 m"

[[rule]]
name = "Phase to phase"
target = "same_line"         # other wires of this line: same span and other spans
radial = "3 m"

[[rule]]
name = "Crossed lines"
target = "other_lines"       # wires of the lines given with --other-line
vertical = "4.5 m"
radial = "4.5 m"
```

| Field | Meaning |
|---|---|
| `target` | `ground`, `points`, `same_line` or `other_lines`. |
| `roles`, `classes` | Points rules: classes by role (resolved with `--lidar-classes`) and/or by number. |
| `vertical`, `radial` | Required clearances (at least one): a distance, or depending on the voltage (below). |
| `pair_voltage` | Wire rules: the voltage of a pair of wires is the `higher` of the two systems' nominal voltages (default), their `sum`, or the `difference` — the actual voltage between the two conductors from phase angles and DC polarities. |
| `vertical_band` | Points count for vertical clearance within this horizontal distance of the wire (default 0.5 m). |
| `search_margin` | Targets are reported within the required distance plus this (default 5 m); crossings of other wires are always reported. |
| `include_same_phase` | `same_line`: also check wires of the same circuit and phase (default: no). |
| `circuits` | `same_line`: `"different"` checks only pairs of wires of different circuits (circuit to circuit, e.g. the two circuits of a double-circuit tower; a phase and an earth wire), `"same"` only pairs of the same circuit (phase to phase); default `"any"`. Combine with `case_pairs` for one circuit iced and the other not. |
| `cases` | Weather cases the rule applies to, by name (`*` `?` wildcards); default: all cases run. |
| `exclude_near_ends` | Exclusion zone: the wire within this horizontal distance of its ends (clamps) is not checked, e.g. `"10 m"` — where wires converge at supports by design. |
| `exclude_spans_at` | Exclusion zone: spans ending at these structures are not checked, e.g. `["con_*"]` for down-leads into cable terminations (wildcards). |
| `case_pairs` | Wire rules: `[["glaze ice", "max temperature"]]` — the first wire in the first case, the second wire in the second (e.g. an iced upper wire sagging towards a hot lower one); checked after all cases are solved. |

### Clearances by voltage

Each section belongs to an electrical system ([systems](line-file.md#electrical-systems): AC or DC, voltage, phase angles) through its conductor path, independent of the cable; its spans and jumpers are checked with the requirement for the system's nominal voltage. `vertical` and `radial` can be a distance, a table by voltage, or a formula:

```toml
[[rule]]
name = "Buildings"
target = "points"
roles = ["building"]
# Rows [up to kV, distance]: the first row whose voltage is not exceeded.
vertical = { by_voltage = [[0, "5 m"], [150, "6 m"], [245, "6.5 m"], [420, "7.5 m"]] }
# base + per_kv · (U − above_kv) above above_kv, else base.
radial = { base = "5 m", per_kv = "0.01 m", above_kv = 50 }
```

With one rule per group of LiDAR classes (vegetation, buildings, roads, railways…), each with its own voltage table, the requirement follows both the class and the voltage. Earthed sections (no system: earth wires, OPGW) are checked as 0 kV — give a `0` row for them. Between wires, `pair_voltage = "difference"` uses the voltage actually between the two conductors: two phases of a 380 kV system meet the 380 kV row, the same phase of two parallel systems the 0 kV row, a phase and an earth wire 219 kV. A voltage above a table's last row stops the run with an error naming the rule, before any check. The report prints the voltages found; the CSV's `required_m` and the envelope rows give the requirement of each wire or pair.

### Envelopes over weather cases

With several weather cases (`--weather`), each case is reported in turn, then the **envelope**: for every rule and every wire (or wire pair), the lowest vertical and radial clearance over all cases with the **case that governs** it, and the cases in which a requirement is not met:

```text
envelope over 3 weather cases (glaze ice, max temperature, rime ice + wind) and 1 case pairs (glaze ice × max temperature):
  rule "Ground": lowest vertical 4.64 m (S2 span 1, max temperature); lowest radial 4.63 m (S2 span 1, max temperature)
    below the requirement in some case: 1
      S2 span 1: vertical 4.64 m (max temperature), radial 4.63 m (max temperature) — in: glaze ice, max temperature
```

The clearance CSV holds each case's rows and then the envelope rows (`case = envelope`, last column `governing_case`). The plan and profile sheets draw every case's wires — the first solid, the others dashed, dotted… (named in the title block) — the envelope of the wire positions.

Wires sharing an end point (adjacent spans, a jumper and the sections it joins) are connected and never checked against each other. Point rules are evaluated during the same LiDAR pass that collects the ground. [`examples/synthetic-line/clearance-rules.olsr`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/clearance-rules.olsr) and the crossing line `crossing-line.ols.json` next to it show all wire rules on the synthetic model. Rule values come from the design code or the utility; a rule editor (per LiDAR class) and design-code plugins producing rule sets are planned.

## Survey files

Plain text, one point per line, separated by commas, semicolons, tabs or
spaces. Empty lines and lines starting with `#` are ignored. Coordinates are
metres in the line's coordinate system.

Without a header the columns are `x y z [code] [line]`:

```text
# x, y, z, feature code, breakline id
2600010.00,1200020.00,512.31,GND,
2600030.00,1200020.00,512.80,ROAD,road-left
```

With a header, columns are found by name, so other layouts work too:

| Column | Names |
|---|---|
| x (easting) | `x`, `e`, `east`, `easting` |
| y (northing) | `y`, `n`, `north`, `northing` |
| z (elevation) | `z`, `h`, `elev`, `elevation`, `height` |
| feature code | `code`, `d`, `desc`, `description`, `feature` |
| breakline id | `line`, `breakline`, `string` |

Other columns (e.g. a point number) are ignored, so a PNEZD export only needs
the header `p,n,e,z,d`.

- **Feature codes** are kept with each point (e.g. `GND`, `ROAD`, `TREE`).
  They will select what each clearance rule applies to; v1 uses all points
  as ground.
- **Breaklines**: consecutive points (in file order) with the same non-empty
  breakline id are joined by edges the triangulation must keep — ridges,
  ditches, road edges, embankment toes. Without them a triangle may cut
  across a ditch. A breakline edge that would cross an earlier breakline is
  skipped and counted in the output.
- Points with the same x and y are merged (the first one is kept).

## Ground from LiDAR

```bash
cargo run --release -p ols-cli -- line LINE.ols.json --lidar TILES_DIR \
  --lidar-classes SCHEME.olsc --clearance 8
```

| Option | Meaning |
|---|---|
| `--lidar PATH` | LAS / LAZ file or directory of tiles. (`--pointcloud` instead reads a project's point cloud or a COPC file, only the blocks or octree nodes of the corridor: [Point clouds](project.md#point-clouds).) |
| `--lidar-classes FILE` | Class scheme (below); default: ASPRS LAS 1.4 standard classes. |
| `--ground-role ROLE` | Classes with this role make the ground surface (default `ground`). |
| `--corridor M` | Only points within M of the spans (default 60 m). Every tile is read (the bounds in tile headers are not trusted); points flagged *withheld* are left out. |
| `--max-edge M` | Longest triangle side (default 50 m): no ground is invented across larger gaps. |
| `--thin M` | Optional: keep the lowest ground point per M × M cell. |

`ols lidar info PATH --classes [--scheme FILE]` lists the tiles and counts the points of every class, with the scheme's names and roles.

### Class schemes (`.olsc`)

What a class number means depends on who classified the data — the ASPRS LAS standard, or a utility's or surveyor's own specification. A class scheme names every class and gives it **roles**; tools select classes by role, so any classification works. A class can have several roles, and roles are free words:

```toml
format = "ols-lidar-classes"
version = 1
id = "my-survey"
name = "Survey company X, project Y"
source = "Survey specification v2"   # optional: where the scheme comes from

[[class]]
code = 1
name = "Terrain"
roles = ["ground"]

[[class]]
code = 5
name = "Asphalt road"
roles = ["ground", "road"]
```

The ASPRS LAS 1.4 scheme is in [`libraries/lidar-classes/asprs-las-1.4.olsc`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/libraries/lidar-classes/asprs-las-1.4.olsc) (ground: 2, water 9, road surface 11). Used by OLS CAD: `ground` (the ground surface), `vegetation` (vegetation analysis), `conductor` and `shield_wire` (as-built wire fitting), and any role a clearance rule names; the application also colours points by role. Suggested: `vegetation`, `building`, `water`, `road`, `railway`, `bridge`, `structure`, `insulator`, `conductor`, `shield_wire`, `load_object`, `other_line_structure`, `other_line_conductor`, `object`, `noise`, `temporary`, `unclassified`.

On a real 25 km line (private data, checked locally), ground built this way from 99.6 million LiDAR points (11.3 million in the utility's ground classes, 22.6 million triangles, about one minute) gave span clearances matching those on the TIN of a commercial program to a median of 2 mm (94 % of 930 spans within 10 cm).

A class may have a display **colour**, `color = "#rrggbb"`; without one, its colour comes from its roles (ground brown, vegetation green, buildings red, water blue, wires yellow…). In the application, Terrain › Survey data › **Class colours…** edits them per class and saves the scheme to a file; the scheme used is each user's choice (for a project or every project), so the same point cloud can be read with another scheme (e.g. a utility's own class numbers stored as ASPRS).

## The ground surface

The points are triangulated (constrained Delaunay, the
[spade](https://crates.io/crates/spade) library) into a TIN. A finished
mesh (an imported TIN, or the output of a terrain-building plugin) can be
used as it is. The ground
height anywhere inside the surveyed area (the convex hull of the points) is
the linear interpolation on the triangle below. Outside it there is no
ground: spans that are not fully over the survey are reported.

## Terrain surfaces in the project

A project keeps any number of terrain surfaces (TINs, *triangulated irregular networks*), by name, compressed in the project file. You choose one as **active**: it is the ground — drawn, and used for clearances, profiles and placing structures. When a project with an active surface opens, the ground is read from it (a 12.7-million-triangle surface: about 1 s) instead of being built from the LiDAR every time. Other surfaces can be **shown** too, each in its colour (the *Other surfaces* layer, drawn in chunks with levels of detail like the ground).

Terrain › **Terrain surfaces (TIN)…** manages them:

- **Import…** a file. Its format comes from the extension, or you choose it:

  | Format | Extension | |
  |---|---|---|
  | LandXML | `.xml` | every TIN surface of the file (`<Surface>` with `<Faces>`; points are *northing easting elevation*) |
  | OBJ | `.obj` | a mesh (`v x y z`, `f a b c …`) |
  | Survey points | `.csv`, `.xyz` | triangulated, breaklines kept; option `max_edge=50` leaves out longer triangles |
  | Terrain plugins | as they say | e.g. `plugins/examples/esri-grid` (ESRI ASCII grids, `.asc`) |

  The first surface kept becomes the ground when there is none.
- **Build from the LiDAR ground classes:** the lowest point of each cell (default 1 m), near the line or the whole cloud, triangulated with a longest side (default 50 m).
- **Keep the ground shown as a surface:** the ground built at opening (from the LiDAR or a survey file), kept so the next opening reads it.
- **Active** (radio) and **shown** (tick, colour) are your own choice: remembered on this computer for this project, never written to the project nor sent to a server — everyone working on a hosted project decides what they show.
- **Checks** (radio): the surface the checks (clearances) use, chosen for the project — one for everyone (a project setting; needs edit rights). None chosen: each user's ground. (The clearance checks will be reworked around the LiDAR points.)
- **Rename**, **Export…** (LandXML `.xml` or OBJ `.obj`), **Delete** (confirmed; it cannot be undone — export first to keep a copy).

Reading, building and storing run in the background. With a project joined on a server, surfaces are imported and built on the hosting computer.

From the command line:

```bash
ols surface list project.ols
ols surface import ground.xml --into project.ols --user ann [--name N] [--format F] [--option max_edge=50] [--plugin DIR] [--replace]
ols surface export project.ols "ground" ground.xml
ols surface rename project.ols "ground" "survey 2026" --user ann
ols surface delete project.ols "ground" --user ann
ols surface formats [--plugin DIR]
```

Ground profiles follow the TIN exactly: a profile point at every triangle
edge crossed, with straight ground between them — no sampling step to choose
and no ridge missed between samples.

## Clearance

For every span (and every jumper, reported as `jumper:<id>`) the **vertical** distance from the wire down to the ground is
evaluated under every piece of the solved wire, at every TIN edge crossing,
and the smallest value is reported with its location. The wire is a
polyline of closely spaced points on the exact catenary; between those
points it lies slightly below its chords, so clearances can be high by at
most the chord sag of a piece — a few millimetres.

Several weather cases are checked together with [envelopes](#envelopes-over-weather-cases); exclusion zones near the wire ends are rule fields (`exclude_near_ends`, `exclude_spans_at`); the plan and profile sheets draw the required-clearance line and the violations.

## Example data

[`examples/synthetic-line/synthetic-survey.csv`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/synthetic-survey.csv)
is invented ground for the synthetic example line, generated by
[`make_synthetic_survey.py`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/make_synthetic_survey.py):
smooth ground through the structure bases, an 8 m hill under span T2–T3 and a
road embankment surveyed as two breaklines across span T4–T5.
