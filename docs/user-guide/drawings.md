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
| `--page` | sheets of a standard paper size (landscape): ISO `A0`–`A4`, the long A sizes `A3x3` …, `B1`–`B4`, `"ANSI A"`–`"ANSI E"`, `"ARCH A"`–`"ARCH E"`; `WIDTHxHEIGHT` in mm; or `optimised` ([Paper size and orientation](#paper-size-and-orientation)); without it one continuous drawing |
| `--paper-sizes`, `--mixed-sizes`, `--same-paper` | with `--page optimised`: the sizes it may choose from (comma-separated, default `A0,A1,A2,A3,A4`), different sizes of one standard on a route's sheets, the same paper on every route |
| `--break-clearance MM` | clearance on paper between a sheet break and what is drawn of every other structure, 0 to 20 mm (default 2; see [Sheets](#sheets)) |
| `--plan-width M` | the **line width** (also `--line-width`): half width of the plan (default 40 m), what is drawn along the line (see [Line width](#line-width)) |
| `--profile-only` | the profile alone, without the plan |
| `--side-offset`, `--clearance`, `--logo` | as for `ohl line --drawings` |
| `--dpi` | resolution of PNG images (default 150) |
| `--elevations` | the structures' elevations on the plan: `off` (default), `above` or `below` the line ([Elevations](#elevations)) |
| `--elevation-offset` | from the line to the elevations' base, mm on paper (default 10) |
| `--elevation-rotation` | angle between the line and the elevations' height, degrees (default 90: standing perpendicular to the line) |
| `--elevation-scale` | the elevations' own scale (default the horizontal one) |
| `--elevation-labels` | written at each elevation: `number`, `type`, `height`, `string_length` (vertical strings), `dimensions` (base → arms → top), `arm_heights` (comma-separated) |
| `--elevation-stick` | the elevations as schematic stick figures (mast, arms, strings) even where the full structure exists |
| `--sheet-template NAME` | the [sheet template](sheet-templates.md): `default`, a `.ohlsheet` file, the project's or the library folder's by name, or `PLUGIN/PRESET` (with `--plugin DIR`); its page unless `--page` |
| `--field KEY=VALUE`, `--fields FILE`, `--revision "A;DATE;TEXT;BY;CHECKED"` | the template's values and the revision table |
| `--orientation` | `landscape` or `portrait` (with `--page optimised`: the only one allowed; default both) |
| `--version LINE=ROLE[:LABEL]` | another version of the line drawn with it in a role (a project line of a project file, or a line file; repeatable) |
| `--role ROLE` | the role of the line itself (its style) |
| `--corridor-width M` | the corridor's edges at ± M in the plan and the key map, the ground along them in the profile |
| `--sheet-lidar-classes 2,3,5` | LiDAR classes drawn as dots where the template draws them (`[lidar]`), from `--lidar` or `--pointcloud`, within the corridor ([Sheet templates](sheet-templates.md#lidar-points)) |
| `--conductor-clearance` | lines below the lowest conductor from the ground rules (`--rules`, `--clearance`) |
| `--plan-drawings [NAMES]`, `--plan-orthophoto [LAYERS]` | the project's drawings (DXF) and orthophotos under the plan, straightened along the line (alone: the template's names, else every one; or the names, comma-separated); `--no-plan-backgrounds` neither, `--plan-background-dpi` the photo's resolution, `--backgrounds FILE.ohl` the project file holding them (default: the line's) — [Drawings and orthophotos under the plan](sheet-templates.md#drawings-and-orthophotos-under-the-plan) |
| `--key-map-drawings`, `--key-map-orthophoto` | the project's drawings and orthophotos under the sheets' key map, north up (default: the template's `[key_map] drawings`, `orthophoto`), from the same project file as `--backgrounds` |
| `--no-bands` | without the template's band rows |
| `--out DIR` | output folder (created) |

The first weather case is drawn solid, the others dashed (each its own
dash pattern, named in the title block). The elevation options default
to the sheet template's.

`ohl line --drawings DIR` still writes what it always did: an SVG (or SVG
sheets with `--page`) and the profile CSV of every route.

## Structures

A structure whose type is at level 1 (a stick figure,
[detail levels](structure-library.md#detail-levels)) is drawn with its
**symbol**: a shaft line from its foot to its highest attachment point in
the profile, a small square in the plan, and its stick drawing (shaft and
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
layer: the outline is the pole's body, as the members are a lattice tower's):

- in the **profile**, its silhouette by height: at each height of its
  outline the outline's extent along the route, up one side and down the
  other (a closed line; between the heights the taper is straight);
- in the **plan**, its outline at the ground line and at its top (round
  as a 16-sided polygon, a polygon with its sides), seen from above;
- in `R-plan-model.dxf`, the same two outlines in project coordinates.

The pole is drawn from the ground line up (the part in the ground left
out), at the structure's position, orientation and height adjustment,
raised by its extension. Its stick drawing (the shaft line and the arms
to the attachment points) stays on `OHL_STRUCTURES`, as before.

The name, station and attachment points stay on `OHL_STRUCTURES` (the
name above the highest member or the pole's top). In SVG the members and
poles carry `class="OHL_MEMBERS"`. The members' profiles (their sections) are not
drawn on the sheets: at the usual scales they are thinner than the pen.
`ohl drawing` reads the cross-section library from `--sections` or the
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
(`--corridor`, the points read for the terrain surface and the clearance
checks) is a separate setting and is not changed by it.

## Elevations

On request (`--elevations above|below`, or **Elevations** in the
app's Export drawings window; off by default) the plan carries each
structure's **elevation** beside the line, so its shape can be read on
the sheet next to where it stands:

- each structure drawn along the line (every one of the run along the
  alignment, one beside the line only within the [line width](#line-width);
  of the sheet's station range on a sheet; those of the other
  [versions](sheet-templates.md#versions-of-the-line-kinds-and-roles) in
  their role's pen) is drawn **seen along the line**: its members (levels 2 and 3), its pole's
  silhouette across the line, else its stick drawing (level 1: the shaft
  and the arms to its attachment points), true shape at the
  **horizontal scale** (a 30 m structure is 15 mm tall at 1:2000) or at
  `--elevation-scale`;
- as a **stick figure** on request (`--elevation-stick`, the template's
  `stick = true`, **elevations as stick figures** in the export window):
  a schematic figure — the mast from its base to its top, an arm out to
  each of its points, its strings — even where its members, a pole's
  outline or a stick drawing holding a whole lattice exist; easier to
  read at a small scale;
- on request its **labels** (`--elevation-labels`, the template's
  `labels`): its number, type and height above its top (`number`,
  `type`, `height`); a **chain of heights** on one side, base → first
  arm → between the arms → top, with a tick at each level
  (`dimensions`); and on the other side **one line per arm**: its height
  above the base (`arm_heights`) followed by the length of the
  **vertical (suspension) strings** hanging from it (`string_length`;
  strain strings, lying along the line, are not written; lengths within
  5 cm of each other as one range, `string 4.41–4.42 m`). The arms are
  the levels of the structure's own points. The texts of a column are
  never closer than 1.2 × their size: one that would be is moved down,
  joined to its level by a thin leader; the chain and the arm lines
  stand clear of the figure (2.5 and 1.5 mm out). Their words come from
  the template (`[legend.labels]` `elevation_dimension`,
  `elevation_arm`, `elevation_string`);
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

The **plan panel grows** to hold the elevations and their labels (above
or below the axis), so they never reach into the profile and are never
hidden by a match line (they are drawn after the match lines). Elevations
are never moved: when two of the line's own would **overlap**, both are
drawn as they are and the export reports a warning naming them (a
continuous drawing as well as sheets, and the sheet preview); draw them
on the other side or at another scale.

*Reading of the request, to be confirmed:* "rotated 90°" is read as the
elevation standing perpendicular to the line on the sheet (its height
across the line), seen along the line (the crossarms across); the
rotation is the angle between the line and the elevation's height.

## Drawings and orthophotos under the plan

The project's DXF drawings and orthophotos (Terrain › Backgrounds) can
lie under the sheets' plan, **straightened along the line** like the
wires and the structures: the drawings' lines on the layer
`OHL_BACKGROUND_DRAWINGS`, the orthophoto resampled into each sheet's
plan panel under a white veil on `OHL_BACKGROUND_PHOTO` (SVG, PDF, PNG;
DXF R12 has no images and leaves it out, with a warning). The sheet
template switches them on (`[plan_background]`, e.g. a plugin's preset);
the export window's ticks and `--plan-drawings`, `--plan-orthophoto`
switch them on or off over it. Display only: nothing computed and no
sheet break depends on them. Every key, the resolution and what happens
at bends: [Sheet templates](sheet-templates.md#drawings-and-orthophotos-under-the-plan).

## Isoclearance lines

`--isoclearance-interval 5` draws on the plan the ground's **isoclearance
lines**: where its clearance to the nearest wire's surface — over the
weather cases drawn, a windy one blown left and right
(`--isoclearance-swing-arc`: and anywhere along each span's swing between
them) — is 0, 5, 10 … m, up to `--isoclearance-max` (40 m), on a
`--isoclearance-spacing` grid (1 m), over the line width. Each line is
coloured by its clearance (red at 0 to green at the top line) and titled
`isoclearance 10 m`, on its own layer **`OHL_ISOCLEARANCE`** (SVG
`class="OHL_ISOCLEARANCE"`), in the sheets and the plan's model space. It
needs the ground (`--terrain`, `--lidar` or `--pointcloud`). In the
desktop application the export draws the lines of the last
Checks › Vegetation run that computed them. See [Vegetation and danger
trees](analyses.md#vegetation-and-danger-trees).

## Sheets

With `--page` (or a [sheet template](sheet-templates.md) giving a page),
each route (or its station range) is split **at structures**: as many
whole spans as fit at the horizontal scale, the structure that ends a
sheet starting the next one (match lines). A station range starts the
first sheet and ends the last one at its stations. The sheet template
draws the frame, title block and the rest around the panels (the
built-in one: logos, line, weather cases, route, sheet n/N, station range
and structures, scales, format). A span longer than the sheet is drawn
anyway and reported as a warning: change the scales or the format.

- The **profile panel takes the whole height** left above the station
  labels (and the band rows). Its height range fits the ground, the
  clearance line and the wires' lowest points; when the structures' tops
  do not fit too they are cut at the panel's top (a break mark, warned);
  when not even the ground and the wires' lowest points fit, the **datum
  is stepped** at a structure (a double line across the panel, its own
  height labels, warned: `datum stepped at T4 (+5 m)`). The vertical
  scale is never changed and the ground is always on the sheet ([how the
  height range is chosen](../theory/drawings.md)).
- The height labels and grid stay inside the panel.
- The **panels end at the match lines**: what lies beyond belongs to the
  next sheet (the template may show it faded, `beyond_match = "faded"`).
  A short station range gives short panels.
- A **structure at a sheet's end** is drawn whole on both sheets (its
  drawing reaches past the match line), its name inside the panel.

### Paper size and orientation

`--page optimised` (the export window's **optimised from the ticked
sizes**; a template's `format = "optimised"`) chooses the paper size and
orientation per route. Every allowed size in each allowed orientation is
laid out exactly as the export would (the scales are never changed), and
the layout kept is the one with

1. the fewest warnings (a span longer than the sheet, a datum step or
   structure tops cut, elevations overlapping, a sheet that runs on),
2. then the least paper (the sum of the sheets' areas: fuller sheets),
3. then the fewest sheets, then the smallest sheet.

With **different sizes allowed** (`--mixed-sizes`), a route's sheets may
take different sizes of one standard (ISO A with its long sizes, ISO B,
ANSI, ARCH) in one orientation — a short last sheet on a smaller size:
the breaks and the sizes are chosen together. With **the same paper on
every route** (`--same-paper`), one size (or one standard, mixed) is
chosen for all routes together. The export reports the choice per route
(`optimised layout A2 landscape (A2 ×4; 0 warning(s), 1.00 m² of paper,
86 % filled)`); in the export window **Compare sizes** lists every
candidate for the first route, best first.

The sizes offered — and those the optimiser may choose from — are ticked
in the window's **Paper sizes** list (ISO A0–A4 by default; the long A
sizes `A3x3` … `A0x3`, ISO B1–B4, ANSI A–E, ARCH A–E1). The ISO A and B
sizes are those of ISO 216, the long A sizes as listed for ISO 5457 /
IS 10711, the ANSI and ARCH sizes from their inch sizes.

**Where a sheet may end.** A match line never falls at, or cuts
through, a structure drawn on the sheet:

- by default a sheet ends at a **structure standing on the line**: one of
  the run along the alignment (on one of its points, such as an angle
  structure, or standing on the alignment between them);
- a structure **beside the line** never ends a sheet, however near it
  stands — a connection or termination of the route away from the
  alignment (more than 5 m beside it and not on one of its points, see
  [Desktop app](desktop-app.md)), within the line width or not: a match
  line there would cut through the span of the line passing it;
- the only other place a sheet may end is an **alignment point without a
  structure** (an alignment point without a structure); the alignment's first and
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
  as `?`); logos embedded as images, cropped to what they cover; only what
  can be seen is written (lines cut to the page or their panel).
- **PNG**: the SVG rendered at `--dpi`, with the computer's fonts.
  Images are limited to 40 million pixels: larger ones are drawn at a lower
  resolution (reported).
- **DXF** (release 12, `AC1009`, ASCII): a format most CAD programs read; not tried
  in a given program yet (no DWG file is written). Entities are LINE,
  POLYLINE, TEXT and CIRCLE on layers `OHL_GROUND`, `OHL_WIRES`,
  `OHL_STRUCTURES`, `OHL_MEMBERS`, `OHL_ELEVATIONS`, `OHL_ISOCLEARANCE`, `OHL_CLEARANCE`, `OHL_GRID`,
  `OHL_TITLE`… Colours are
  the nearest of DXF's indexed colours; dashes are line types `OHL_DASH1`…
  R12 has no line weights, true colours or images, so pens become colours
  only; SVG logos are written as their outlines (`OHL_TITLE_IMAGE`), PNG
  and JPEG logos are left out with a warning. Texts other than ASCII use
  R12's own codes (`%%d` °, `%%p` ±, `%%c` ⌀, `%%nnn` a Windows-1252
  character). The template's parts have their layers ([Sheet
  templates](sheet-templates.md#images-per-format)). Three kinds of file:
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
  terrain surface.
- A `<wire>` is one span or jumper of one weather case; `dash` is set for
  the cases drawn dashed. A wire leaving the station range is cut at its
  ends (it may then appear as more than one `<wire>`).

## In the desktop app

**File › Export › Drawings…** opens a window: the **sheet template**
(Built-in, Project, Library or a plugin's; **Load file…** keeps a
`.ohlsheet` in the project, **Save as file…** writes the one chosen),
route (or every route), formats, sheets (one continuous drawing, the
template's format, **optimised from the ticked sizes** — with the
orientations allowed, **different sizes allowed**, **same paper on every
route** and **Compare sizes** — or a ticked size split at structures;
**portrait**; the **Paper sizes** list, see [Paper size and
orientation](#paper-size-and-orientation)),
horizontal and vertical scale, **line width** (± m, the plan's half
width: the same setting as the Profile page's **line width ±**, see
[Line width](#line-width)), **sheet breaks** (the break clearance, mm on
paper), station range, clearance line (typed, or **from the ground
rule** of the project: Checks › Ground clearances, else the rule file
chosen; **which ground rule** — by default for each circuit the largest
of its voltage, or one of the project's chosen by name, which the lines
below the lowest conductor take too, kept with the project's drawing
settings; 2026-10-24), **side profiles**, the plan's own scale across the line,
**logos**, the structures' [elevations](#elevations) (off, above or
below; base offset in mm; rotation), PNG resolution; then the template's
**values** (required ones marked `*`, per-sheet values), the
**revisions** (rows added, deleted, or the last version commit), the
**versions of the line** (other project lines in their roles, the line's
own role), **more of the template** (corridor, LiDAR classes, the project's
drawings and orthophotos under the plan, lines below the lowest
conductor, band rows, elevation scale and labels), **Weather cases per
circuit** (each circuit's case drawn and stringing case over the
template's `[cases]`, see [Weather cases](sheet-templates.md#weather-cases-cases);
a case not solved yet is solved, the sheets wait for it), **Preview sheet** (one
sheet drawn by the export in a window of its own, with its warnings; its
number chosen before or after ticking, the route and the sheet chosen
in the preview window too, ◀ ▶; drawn in the background, the window
usable meanwhile, and again only once the settings have stayed unchanged
for half a second — typing a value draws once, not at every key), the
violations of the last clearance check as markers, the profile alone,
the other solved weather cases (dashed, each from its own solution), and
the output folder. A window taller than the screen scrolls.
**Export** writes the files in the background with a progress window; the
views stay usable and the window stays open for the next export. It draws
the weather case shown (solid). The sheet format and scales chosen here
are for the export only: the Profile page stays one continuous view along
the route, without sheet breaks.

**The settings are kept in the project**, per project line: formats,
sheet format, scales, line width, break clearance, station range,
clearance line, the elevations, PNG resolution, the profile alone, the
other cases, side profiles, plan scale, logos (their files), the sheet
template chosen with its values, revisions and versions, the
ticks of more of the template (the backgrounds under the plan among
them), and
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
  of a sheet beyond what its paper holds darker amber. The lines below
  the lowest conductor, when asked for, are drawn over the profile as
  the sheets draw them.
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

## Plans: route plan, situation plans, phase plan

Beside the plan-and-profile sheets (2026-10-24, the owner's request,
roadmap O14; built, in active development (not approved yet)), three
plans in a menu of their own, **Export › Plans**, each written for every
route with the sheet template, title fields, routes and cases of Export ›
Drawings…, on the paper chosen, in PDF, SVG, PNG or DXF:

- **Route plan** — the whole route **north up** at a small scale
  (1:5000 to 1:25 000, 1:10 000 by default), over the project's drawings
  (e.g. municipal boundaries) and orthophotos: the line's axis, its wires,
  its structures numbered, the corridor (at the drawings' line width), a
  north arrow, a scale bar and a legend, in the template's frame and
  title block. The sheets follow the route, each taking what fits.
- **Situation plans** — the same at a large scale (1:500 to 1:2000,
  1:1000 by default), the sheets overlapping (a tenth of their width),
  over the cadastral survey loaded as a drawing (its lines and its texts,
  the parcel numbers) and the orthophotos (under a white veil).
- **Phase plan** — each structure's figure seen along the line (its
  members, else its stick drawing), every wire position a dot in its
  colour labelled with its circuit's **number** (the key under the
  figures lists the circuits) and its phase, drawn only where the
  arrangement of the wires **leaving it ahead** differs from the
  structure before (a transposition, a circuit joining or leaving,
  another arm — compared by level and place, not by height); the first
  and the last always. Between, "from A to B: arrangement unchanged"
  (the template's legend label `phase_unchanged`). Nothing typed twice:
  taken from the line.

The drawings come within 1 km of the line in full detail (5 cm), north
up; the orthophotos are read at the template's background resolution.

From the command line: `ohl drawing … --plans route,situation,phase`
(`--route-plan-scale`, `--situation-scale`; the sheets' `--page`, else
A3) writes `<route>-route-plan`, `-situation-plan`, `-phase-plan` beside
the sheets (the project's drawings and orthophotos read for them).

Not there yet: plan sets (several drawings exported at once), links to
public map services, the corridor's land use.

## What is not there yet

- No DWG files (DXF only); DXF has no line
  weights, true colours or raster logos.
- No title block editor in the app (the template file is edited).
- PDF fonts are not embedded (the standard Helvetica of every PDF reader).
