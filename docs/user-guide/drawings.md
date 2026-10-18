# Drawings and profile exports

OpenOHL draws every route of a line as **plan and profile**: the profile
(ground, side ground, required-clearance line, structures, wires by phase,
violation markers) and above it the plan, straightened along the route so
that it lines up with the profile. The same drawings are written as SVG,
DXF, PDF and PNG; the profile's numbers as CSV and XML.

## From the command line

`ohl drawing` solves the line exactly like `ohl line` (it takes all of its
options: weather cases, terrain, LiDAR, rules…) and then writes the drawings
asked for:

```bash
ohl drawing examples/synthetic-line/synthetic-line.ols.json \
  --terrain examples/synthetic-line/synthetic-survey.csv \
  --weather examples/synthetic-line/weather-cases.olswc --clearance 7 \
  --route main --format pdf,dxf,csv --page A3 \
  --hscale 1:2000 --vscale 1:500 --from 0+100 --to 1+000 --out drawings
```

| Option | Meaning |
|---|---|
| `--format` | `svg`, `dxf`, `pdf`, `png`, `csv`, `xml` (comma-separated or repeated; default `svg`) |
| `--route ID` | the route to draw (repeatable; default every route) |
| `--from`, `--to` | station range, in metres (`250`) or `km+m` (`0+250`); default the whole route |
| `--hscale`, `--vscale` | horizontal and vertical scale, `1:2000` or `2000` (defaults 1:2000 and 1:200); the exaggeration is their ratio |
| `--plan-scale` | the plan across the line (default the horizontal scale) |
| `--page` | sheets `A0`–`A4` (landscape) or `WIDTHxHEIGHT` in mm; without it one continuous drawing |
| `--profile-only` | the profile alone, without the plan |
| `--side-offset`, `--plan-width`, `--clearance`, `--logo` | as for `ohl line --drawings` |
| `--dpi` | resolution of PNG images (default 150) |
| `--out DIR` | output folder (created) |

The first weather case is drawn solid, the others dashed (each its own
dash pattern, named in the title block).

`ohl line --drawings DIR` still writes what it always did: an SVG (or SVG
sheets with `--page`) and the profile CSV of every route.

## Sheets

With `--page`, each route (or its station range) is split **at
structures**: as many whole spans as fit at the horizontal scale, the
structure that ends a sheet starting the next one (match lines). A station
range starts the first sheet and ends the last one at its stations. Every
sheet has its own height range and a title block: logos, line, weather
cases, route, sheet n/N, station range and structures, scales, format. A
span longer than the sheet, or a profile taller than it, is drawn anyway
and reported as a warning: change the scales or the format.

## Files

For route `R` (`R-profile…` with `--profile-only`):

| Format | One continuous drawing | Sheets |
|---|---|---|
| SVG, PNG, DXF | `R.svg`, `R.png`, `R.dxf` | `R-sheet-01.svg` … |
| PDF | `R.pdf` (one page) | `R.pdf`, a page per sheet |
| DXF model space | `R-plan-model.dxf`, `R-profile-model.dxf` | the same |
| CSV, XML | `R-profile.csv`, `R-profile.xml` | the same |

- **SVG**: true size in mm; wires and ground carry their names as tooltips.
- **PDF**: vector, true size, one page per sheet; texts in the standard
  Helvetica fonts (no fonts embedded; characters outside Windows-1252 show
  as `?`); logos embedded as images.
- **PNG**: the SVG rendered at `--dpi`, with the computer's fonts.
  Images are limited to 40 million pixels: larger ones are drawn at a lower
  resolution (reported).
- **DXF** (AutoCAD R12, ASCII): opens in every CAD program, and imports
  into DWG-based ones (no DWG file is written). Entities are LINE,
  POLYLINE, TEXT and CIRCLE on layers `OLS_GROUND`, `OLS_WIRES`,
  `OLS_STRUCTURES`, `OLS_CLEARANCE`, `OLS_GRID`, `OLS_TITLE`… Colours are
  the nearest AutoCAD colour index; dashes are line types `OLS_DASH1`…
  R12 has no line weights, true colours or images, so pens become colours
  only and logos are left out. Three kinds of file:
  - the **sheets** in mm, as printed (what lies outside the panels is cut
    away);
  - `R-plan-model.dxf`: the plan in **project coordinates**, metres —
    centreline, structures (a square and the name), wires and markers;
  - `R-profile-model.dxf`: the profile in model space, **x = station,
    y = height × exaggeration**, metres, with its grid and labels. Line
    type scale and text heights follow the horizontal scale.

## Profile CSV

`kind,label,station_m,offset_m,z_m`, a row per point, within the station
range (lines cut at its ends):

| kind | label | |
|---|---|---|
| `ground` | `centreline` | ground along the centreline |
| `ground_side` | the offset, e.g. `-10.0` | side ground profiles |
| `clearance` | `+7` | ground plus the required clearance |
| `structure` | its name | its position, z = its base |
| `attachment` | the structure's name | its attachment points |
| `wire` | section and span (and weather case) | wire points |

## OLS profile XML

A simple, open format for the same numbers (version 1):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<olsProfile version="1" route="main" name="Main route" length="1260.000"
            from="100.000" to="1000.000" units="m">
  <scales horizontal="2000" vertical="500"/>
  <ground offset="0">
    <run><p s="100.000" z="503.120"/><p s="104.381" z="503.240"/>…</run>
  </ground>
  <ground offset="-10">…</ground>
  <clearance height="7"><run>…</run></clearance>
  <structure id="T2" s="310.000" o="0.000" base="505.000" top="532.000"
             x="…" y="…">
    <attachment s="310.000" o="-3.000" z="531.000"/>
  </structure>
  <wire label="S1 span 0 (C1 L1)" color="#e6194b"><p s="…" o="…" z="…"/>…</wire>
  <wire label="S1 span 0 (C1 L1) — max temperature" color="#e6194b" dash="3 1.5">…</wire>
  <marker label="…" s="…" o="…" z="…"/>
</olsProfile>
```

- `s` station along the route's centreline, `o` offset from it (right
  positive), `z` height, `x`/`y` project coordinates; all in metres.
- `<ground>` and `<clearance>` hold **runs**: a run ends where there is no
  ground surface.
- A `<wire>` is one span or jumper of one weather case; `dash` is set for
  the cases drawn dashed. A wire leaving the station range is cut at its
  ends (it may then appear as more than one `<wire>`).

## In the desktop app

**File › Export › Drawings…** opens a window: route (or every route),
formats, sheets (one continuous drawing, or A0–A4 split at structures),
horizontal and vertical scale, station range, clearance line, PNG
resolution, the profile alone, the other solved weather cases (dashed),
and the output folder. **Export** writes the files in the background with a
progress window; the views stay usable and the window stays open for the
next export. It draws the weather case shown (solid).

## What is not there yet

- No DWG files (the DXF imports into DWG-based programs); DXF has no line
  weights, true colours or logos.
- The sheet layout and title block are fixed; company templates will come
  as a layout plugin.
- PDF fonts are not embedded (the standard Helvetica of every PDF reader).
