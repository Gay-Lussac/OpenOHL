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
| `--clearance-csv FILE` | Lowest clearance of every span: `section,span,clearance_m,wire_x,wire_y,wire_z,ground_z,covered`. |

Run it once per weather case that governs clearance (e.g. maximum operating
temperature, ice), with `--temperature`, `--ice-thickness` and
`--wind-pressure`. Blown-out wires are checked over the ground they are blown
over.

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
| `--lidar PATH` | LAS / LAZ file or directory of tiles. |
| `--lidar-classes FILE` | Class scheme (below); default: ASPRS LAS 1.4 standard classes. |
| `--ground-role ROLE` | Classes with this role make the ground surface (default `ground`). |
| `--corridor M` | Only points within M of the spans (default 60 m); tiles outside are not decompressed. |
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

[[class]]
code = 1
name = "Terrain"
roles = ["ground"]

[[class]]
code = 5
name = "Asphalt road"
roles = ["ground", "road"]
```

The ASPRS LAS 1.4 scheme is in [`libraries/lidar-classes/asprs-las-1.4.olsc`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/libraries/lidar-classes/asprs-las-1.4.olsc) (ground: 2, water 9, road surface 11). Used by OLS CAD so far: `ground`. Suggested for clearance rules and later tools: `vegetation`, `building`, `water`, `road`, `railway`, `bridge`, `structure`, `insulator`, `conductor`, `shield_wire`, `load_object`, `other_line_structure`, `other_line_conductor`, `object`, `noise`, `temporary`, `unclassified`.

On a real 25 km line (private data, checked locally), ground built this way from 99.6 million LiDAR points (11.3 million in the utility's ground classes, 22.6 million triangles, about one minute) gave span clearances matching those on the TIN of a commercial program to a median of 2 mm (94 % of 930 spans within 10 cm).

## The ground surface

The points are triangulated (constrained Delaunay, the
[spade](https://crates.io/crates/spade) library) into a TIN. A finished
mesh (an imported TIN, or the output of a terrain-building plugin) can be
used as it is. The ground
height anywhere inside the surveyed area (the convex hull of the points) is
the linear interpolation on the triangle below. Outside it there is no
ground: spans that are not fully over the survey are reported.

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

Planned next (ROADMAP Phase 2): clearance rules by feature code (roads,
buildings, water), shortest (radial) distances, clearance envelopes over
several weather cases, and profiles in plan and profile drawings.

## Example data

[`examples/synthetic-line/synthetic-survey.csv`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/synthetic-survey.csv)
is invented ground for the synthetic example line, generated by
[`make_synthetic_survey.py`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/make_synthetic_survey.py):
smooth ground through the structure bases, an 8 m hill under span T2–T3 and a
road embankment surveyed as two breaklines across span T4–T5.
