# Drawings and profile exports

OpenOHL draws every route of a line as **plan and profile**: the profile
(ground, side ground, required-clearance line, structures, wires by phase,
violation markers) and above it the plan, straightened along the route so
that it lines up with the profile. The same drawings are written as SVG,
DXF, PDF and PNG; the profile's numbers as CSV and XML.

**Profile first, sheets on top.** The profile is one **continuous** view
along the route: the desktop app's Profile page works on it, with no sheet
breaks, and an export without `--page` writes it as one continuous
drawing. **Sheets** are a separate, optional layer on top of it, for
exports: the sheet format, the scales and where the sheets break belong
to the export settings only (`ohl drawing … --page`, `ohl line --drawings
… --page`, File › Export › Drawings… in the app) and never change the
profile view or any computed number. The app can show them over its views
(the [sheet preview](#sheet-preview), off by default).

## From the command line

`ohl drawing` solves the line exactly like `ohl line` (it takes all of its
options: weather cases, terrain, LiDAR, rules…) and then writes the drawings
asked for:

```bash
ohl drawing examples/synthetic-line/synthetic-line.ohl.json \
  --terrain examples/synthetic-line/synthetic-survey.csv \
  --weather examples/synthetic-line/weather-cases.ohlwc --clearance 7 \
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
| `--break-clearance MM` | clearance on paper between a sheet break and what is drawn of every other structure, 0 to 20 mm (default 2; see [Sheets](#sheets)) |
| `--plan-width M` | the **line width** (also `--line-width`): half width of the plan (default 40 m), what is drawn along the line (see [Line width](#line-width)) |
| `--profile-only` | the profile alone, without the plan |
| `--side-offset`, `--clearance`, `--logo` | as for `ohl line --drawings` |
| `--dpi` | resolution of PNG images (default 150) |
| `--elevations` | the structures' elevations on the plan: `off` (default), `above` or `below` the line ([Elevations](#elevations)) |
| `--elevation-offset` | from the line to the elevations' base, mm on paper (default 10) |
| `--elevation-rotation` | angle between the line and the elevations' height, degrees (default 90: standing perpendicular to the line) |
| `--out DIR` | output folder (created) |

The first weather case is drawn solid, the others dashed (each its own
dash pattern, named in the title block).

`ohl line --drawings DIR` still writes what it always did: an SVG (or SVG
sheets with `--page`) and the profile CSV of every route.

## Structures

A structure whose type is at level 1 (a stick figure,
[detail levels](structure-library.md#detail-levels)) is drawn with its
**symbol**: a mast line from its foot to its highest attachment point in
the profile, a small square in the plan, and its stick drawing (mast and
arms) as before. A structure whose type is at **level 2 or 3** (members,
or a separate `.ohlfe` model) is drawn with its **members** instead of the
symbol, placed as the line places the type (position, orientation,
extension, height adjustment), as lines at the sheet scale on their own
layer `OHL_MEMBERS`:

- in the **profile**, the members projected on the profile plane: the
  structure's silhouette along the route;
- in the **plan**, the members seen from above: the footprint and the
  crossarms;
- in the plan in model space (`R-plan-model.dxf`), the same in project
  coordinates.

A **pole at level 2 or 3** (a `[pole]` outline with diameters at two
heights at least, or a named `.ohlp` file) is drawn with its **outline**
instead of the symbol, on the same layer `OHL_MEMBERS` (the members'
layer: the outline is the pole's body, as the members are a tower's):

- in the **profile**, its silhouette by height: at each height of its
  outline the outline's extent along the route, up one side and down the
  other (a closed line; between the heights the taper is straight);
- in the **plan**, its outline at the ground line and at its top (round
  as a 16-sided polygon, a polygon with its sides), seen from above;
- in `R-plan-model.dxf`, the same two outlines in project coordinates.

The pole is drawn from the ground line up (the part in the ground left
out), at the structure's position, orientation and height adjustment,
raised by its extension. Its stick drawing (the mast line and the arms
to the attachment points) stays on `OHL_STRUCTURES`, as before.

The name, station and attachment points stay on `OHL_STRUCTURES` (the
name above the highest member or the pole's top). In SVG the members and
poles carry `class="OHL_MEMBERS"`. The members' profiles (their sections) are not
drawn on the sheets: at the usual scales they are thinner than the pen.
`ohl drawing` reads the section library from `--sections` or the
`Sections` folder next to the line (only for a type with a separate
model, whose members it needs to read).

## Line width

The **line width** (± m from the alignment, default 40 m) decides what is
drawn along the line, in the plan and in the profile:

- the plan shows the band of the line width each side of the alignment;
- every structure of the run along the alignment is drawn; a structure of
  the route **beside the line** (a connection or termination away from
  the alignment, more than 5 m beside it and not on one of its points) is
  drawn only when it stands within the line width;
- wires are drawn within it: a span out to a structure beyond it is cut
  where it leaves the line width (the crossing interpolated between the
  wire's points);
- violation markers within it.

The profile CSV and XML keep every number (they are not cut at the line
width). In the desktop app the line width is **one setting** for the
drawings and the Profile page: **Line width** in the export window and
**line width ±** on the Profile page are the same value (kept with the
project line, see [In the desktop app](#in-the-desktop-app)); on the
Profile page it also decides how far left and right the ground, the
survey and LiDAR points and the 3D drawings are shown. On the command
line it is `--plan-width` (or `--line-width`). The line width does not
decide where sheets end (below). The LiDAR corridor of the analyses
(`--corridor`, the points read for the ground surface and the clearance
checks) is a separate setting and is not changed by it.

## Elevations

On request (`--elevations above|below`, or **Elevations** in the
app's Export drawings window; off by default) the plan carries each
structure's **elevation** beside the line, so its shape can be read on
the sheet next to where it stands:

- each structure drawn along the line (every one of the run along the
  alignment, one beside the line only within the [line width](#line-width);
  of the sheet's station range on a sheet) is drawn **seen along the line**: its members (levels 2 and 3), its pole's
  silhouette across the line, else its stick drawing (level 1: the mast
  and the arms to its attachment points), true shape at the
  **horizontal scale** (a 30 m tower is 15 mm tall at 1:2000);
- its **base** stands at the structure's station, `--elevation-offset`
  mm on paper (default 10 mm) above or below the line (the route's
  centreline in the plan);
- its height makes `--elevation-rotation` degrees with the line (default
  **90°: standing perpendicular to the line**), pointing away from it:
  above the line the top points up the sheet, below the line the
  elevation is turned half a turn, its top pointing down the sheet; 0°
  lays it along the line;
- a thin dashed **leader** joins where the structure stands in the plan
  to the elevation's base;
- elevations and leaders are on their own layer **`OHL_ELEVATIONS`**
  (SVG `class="OHL_ELEVATIONS"`, each elevation titled `ID elevation`),
  in every format (SVG, DXF sheets, PDF, PNG); they are not clipped to
  the plan panel. Not in the model-space files.

Elevations are never moved: when two would **overlap** each other, or
one would reach into the **profile** band, both are drawn as they are
and the export reports a warning naming them (a continuous drawing as
well as sheets, and the sheet preview); draw them above the line, nearer to it, or at another
scale.

*Reading of the request, to be confirmed:* "rotated 90°" is read as the
elevation standing perpendicular to the line on the sheet (its height
across the line), seen along the line (the crossarms across); the
rotation is the angle between the line and the elevation's height.

## Sheets

With `--page`, each route (or its station range) is split **at
structures**: as many whole spans as fit at the horizontal scale, the
structure that ends a sheet starting the next one (match lines). A station
range starts the first sheet and ends the last one at its stations. Every
sheet has its own height range and a title block: logos, line, weather
cases, route, sheet n/N, station range and structures, scales, format. A
span longer than the sheet, or a profile taller than it, is drawn anyway
and reported as a warning: change the scales or the format.

**Where a sheet may end.** A match line never falls at, or cuts
through, a structure drawn on the sheet:

- by default a sheet ends at a **structure standing on the line**: one of
  the run along the alignment (on one of its points, such as an angle
  pylon, or standing on the alignment between them);
- a structure **beside the line** never ends a sheet, however near it
  stands — a connection or termination of the route away from the
  alignment (more than 5 m beside it and not on one of its points, see
  [Desktop app](desktop-app.md)), within the line width or not: a match
  line there would cut through the span of the line passing it;
- the only other place a sheet may end is an **alignment point without a
  structure** (an angle point without a pylon); the alignment's first and
  last points are no breaks of their own (the sheets start at the first
  structure and end at the last);
- breaks never fall in the middle of a span;
- the place where a sheet ends must be at least the **break clearance on
  paper** — 2 mm by default (4 m at 1:2000, 1 m at 1:500), set with
  `--break-clearance` (0 to 20 mm) or **Sheet breaks** in the export
  window — from what is drawn of **every other structure** — its station,
  its attachment points and its stick drawing, along the route; a
  structure beside the line counts when it is drawn (within the line
  width);
- when the place where the sheet would end is not free, the break moves
  back to the last free one that still fits on the sheet;
- when no free place fits, the sheet runs on to the next free one. It is
  drawn anyway (longer than the paper, like a span that is too long) and
  the warning names the places it could not end at and why:
  `no free sheet break within the 526 m the sheet holds at 1:2000 (T2 is
  within 4.0 m of C1 (0+312.0); C1 stands beside the line, 20.0 m right of
  the alignment (not on it)): the sheet runs on to T3 (640 m), drawn
  anyway`.

The title block names where each sheet starts and ends: a structure, an
alignment point (its id, e.g. `P9`), or the station where a station range
starts or ends. The ends of a station range are kept as given. Lines
without structures beside the line, free alignment points or structures
close to one another are split as before.

The export settings — formats, scales, sheet format, line width, break
clearance, the options and the sheet preview on or off — are kept in the
project in the app (per project line, see [In the desktop
app](#in-the-desktop-app)); on the command line they are the options
given.

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
- **DXF** (release 12, `AC1009`, ASCII): a format most CAD programs read; not tried
  in a given program yet (no DWG file is written). Entities are LINE,
  POLYLINE, TEXT and CIRCLE on layers `OHL_GROUND`, `OHL_WIRES`,
  `OHL_STRUCTURES`, `OHL_MEMBERS`, `OHL_ELEVATIONS`, `OHL_CLEARANCE`, `OHL_GRID`,
  `OHL_TITLE`… Colours are
  the nearest of DXF's indexed colours; dashes are line types `OHL_DASH1`…
  R12 has no line weights, true colours or images, so pens become colours
  only and logos are left out. Three kinds of file:
  - the **sheets** in mm, as printed (what lies outside the panels is cut
    away);
  - `R-plan-model.dxf`: the plan in **project coordinates**, metres —
    centreline, structures (a square and the name; the members at
    levels 2 and 3), wires and markers;
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

## OHL profile XML

A simple, open format for the same numbers (version 1):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<ohlProfile version="1" route="main" name="Main route" length="1260.000"
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
</ohlProfile>
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
horizontal and vertical scale, **line width** (± m, the plan's half
width: the same setting as the Profile page's **line width ±**, see
[Line width](#line-width)), **sheet breaks** (the break clearance, mm on
paper), station range, clearance line, the structures'
[elevations](#elevations) (off, above or below; base offset in mm;
rotation), PNG resolution, the profile alone,
the other solved weather cases (dashed), and the output folder.
**Export** writes the files in the background with a progress window; the
views stay usable and the window stays open for the next export. It draws
the weather case shown (solid). The sheet format and scales chosen here
are for the export only: the Profile page stays one continuous view along
the route, without sheet breaks.

**The settings are kept in the project**, per project line: formats,
sheet format, scales, line width, break clearance, station range,
clearance line, the elevations, PNG resolution, the profile alone, the other cases and
the sheet preview on or off (not the route chosen nor the output folder,
which belongs to the computer). They are written as they change (not
while a value is being dragged or typed) and are there when the project
is opened again; a project without them (an older one) opens with the
defaults (A3 at 1:2000 / 1:200, PDF, ±40 m, 2 mm, the preview off).
Each project line has its own: switching lines takes that line's. Like
the project's other settings (the surface of the checks) they are not
undone with Edit › Undo, a designer changes them, and on a server they
follow everyone (another user's change is read at once); a viewer's
changes stay for the session.

### Sheet preview

**Show the export's sheets over the profile and the plan** — a tick in
the export window, in **View › Sheet preview** and in the Profile page's
**Sheets** menu (a menu of ticks: it stays open; off by default) — shows
the sheets the export would write now, over the continuous views:

- **Profile**: a dashed match line across the view at each end of every
  sheet, the sheets in a strip along the bottom with their numbers (the
  alignment's id first when the profile shows several), and the export's
  warnings above the view (a box that scrolls when they are many): a
  sheet that runs on because no free break fits, a span longer than the
  sheet, a profile taller than it. A warned sheet is amber, and the part
  of a sheet beyond what its paper holds darker amber.
- **Plan**: each sheet's band along its alignment (the line width each
  side, as the sheets' plan shows it), its match lines across the band and
  its number; the structures' [elevations](#elevations) and their
  leaders when they are on, as the sheets draw them; when the export
  draws the profile alone, the plan says so and shows no sheets.

The preview is laid out by the export's own code with the window's
settings (format, scales, line width, break clearance, station range, the
profile alone, the other cases): nothing is computed a second way, and it
follows the settings as they change. Without a sheet format it says that
the export writes one continuous drawing.

## What is not there yet

- No DWG files (DXF only); DXF has no line
  weights, true colours or logos.
- The sheet layout and title block are fixed; company templates will come
  as a layout plugin.
- PDF fonts are not embedded (the standard Helvetica of every PDF reader).
