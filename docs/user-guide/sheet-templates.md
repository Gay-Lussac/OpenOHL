# Sheet templates (.ohlsheet)

*Built 2026-10-20, in active development (not approved yet).*

A **sheet template** says everything about the plan and profile sheets
that a company or an authority sets: the page and its margins, the frame,
the order and size of the panels, the pens, the band rows under the
profile, the title block (a grid of cells with texts, fields and logos),
the revision table, free notes, the legend, the key map, graphic scales,
north arrows, the corridor, the elevations of the structures, and how
wires and structures are drawn by **kind** (high voltage, low voltage,
telecom, earth wire, overhead or cable) and by the **role** of the version
of the line drawn (existing, project, to be removed …). The drawing
itself — the ground, the wires, the structures, where sheets end — is
computed the same way whatever the template.

Without a template of yours the sheets are drawn with the **built-in**
one: neutral colours, the wires in their phase colours, a title block
across the bottom (it is also `libraries/sheet-templates/default.ohlsheet`,
to copy). `examples/sheet-templates/example-a1.ohlsheet` uses **every
key**, each explained in a comment; its values are invented.

## Where templates come from

- **Built in**: `default`.
- **The project**: templates kept in the project line (File › Export ›
  Drawings… › **Load file…**), their images embedded: the project travels
  whole. Each project line has its own (they are library files of kind
  `sheet-templates`).
- **The library folder**: `libraries/sheet-templates/*.ohlsheet` (shipped,
  or a company's folder of that name next to the program).
- **Plugins**: a plugin's presets, from its `sheet-templates` export or
  from `.ohlsheet` files its manifest lists ([Writing
  plugins](../development/plugins.md#sheet-template-presets)). A
  coordinator writes an authority's preset this way; nothing in OpenOHL
  names an authority.

`ohl sheet-templates [--project FILE] [--plugin DIR] [--out DIR]` lists
them all (source, name, page, fields, `*` marking the required ones) and
with `--out` writes each as a `.ohlsheet` file with its images embedded.

## The file

TOML, format `ohl-sheet-template`, version 1; unknown keys are refused,
and so is a placeholder that names no field. Lengths on paper carry their
unit (`"10 mm"`, `"1 cm"`; a bare number is mm), lengths in the field too
(`"100 m"`, `"1 km"`). Every table may be left out (its defaults: the
built-in look).

| Table | Keys | What |
|---|---|---|
| (top) | `format`, `version`, `name`, `description` | header |
| `[page]` | `format` (a standard paper size: `A0`–`A4`, `A3x3` …, `B1`–`B4`, `ANSI D`, `ARCH E` …; `WIDTHxHEIGHT`; or `"optimised"`), `orientation` (`landscape`, `portrait`; with `"optimised"` the only one allowed, none: both), `sizes` and `mixed` (with `"optimised"`: the sizes it chooses from, default A0–A4, and different sizes of one standard on a route's sheets — [Paper size and orientation](drawings.md#paper-size-and-orientation)), `width` + `height` (a long format instead), `margin = { left, top, right, bottom }` | the sheet; a wide left margin is the binding edge |
| `[frame]` | `border = [{ inset, width }, …]`, `padding` | the frame's lines, the room inside them |
| `[text]` | `font` (`Helvetica` only for now), `size`, `colour` | the labels' and the title block's texts |
| `[layout]` | `order` (`["plan", "profile", "bands"]` or the plan last), `gap`, `plan_height` (`"auto"` or at least a length), `beyond_match` (`"none"` or `"faded"`), `axis_room`, `station_labels`, `plan_title`, `profile_title` (default `"Plan"`, `"Profile"`; `""` none) | the panels |
| `[layers]` | `OHL_GROUND = { width, colour, dash }` … any layer | pens by layer |
| `[[band]]` | `kind`, `height`, `label`, `every`, `field`, `decimals`, `format` (structures rows) | rows under the profile ([below](#band-rows)) |
| `[cases]` | `conductors`, `earth_wires`, `stringing`: a weather case's name or a temperature | the cases drawn and of the stringing band ([below](#weather-cases-cases)) |
| `[north_arrow]` | `place` (`"plan"`, `"off"`), `size` | one per straight stretch at the plan's top edge |
| `[scale_bar]` | `place` (`"bottom"`, `"plan"`, `"profile"`, `"off"`), `lengths = ["100 m", "10 m"]` | graphic scales along and up |
| `[legend]` | `place` (`"above_title_block"`, `"bottom"`, `"off"`), `title`, `width`, `height`, `column_width`, `size`, `[legend.labels]` ([below](#the-templates-words)) | made from what each sheet draws |
| `[key_map]` | `place` (`"bottom"`, `"off"`), `width`, `height`, `scale`, `extent` (`"route"`, `"sheet"`), `margin`, `structures`, `north_arrow`, `corridor`, `highlight = { pen }` | a small plan of the route, this sheet marked |
| `[corridor]` | `width`, `label` (`{width}`), `pen`, `near_side` (`"right"`, `"left"`), `near`, `far` (pens over `pen`), `profile` | the corridor's edges at ± `width` from the axis, and the ground along them in the profile ([below](#the-corridors-sides-and-the-centreline)) |
| `[centreline]` | `pen`, `ground` | the axis in the plan; the ground on it in the profile (always solid) |
| `[lidar]` | `profile`, `plan`, `classes = [2, 3, 5]`, `size`, `cell`, `[lidar.styles] "2" = { colour, size, label }` (a code, `"3-5"` or `"3,4"`) | LiDAR points as dots ([below](#lidar-points)) |
| `[plan_background]` | `drawings` (`true`, `false` or a list of names), `texts`, `points`, `own_colours`, `pen`, `orthophoto` (`true`, `false` or a list of image layers), `veil`, `dpi`, `quality` | the project's drawings and orthophotos under the plan ([below](#drawings-and-orthophotos-under-the-plan)) |
| `[elevations]` | `place`, `offset`, `rotation`, `scale`, `labels` (`number`, `type`, `height`, `string_length`, `dimensions`, `arm_heights`), `size`, `stick` (schematic stick figures), `pen` | the structures' elevations on the plan ([Elevations](drawings.md#elevations)); their words in `[legend.labels]`: `elevation_dimension`, `elevation_arm`, `elevation_string` |
| `[line_styles]` | `by_kind`, `low_voltage_up_to_kv`, `kind_field`, `construction_field`, `cable_dash`, `[line_styles.kinds.hv|lv|telecom|earth] = { colour, width, dash, label }` | wires by kind |
| `[roles.<role>]` | `label`, `width`, `weight`, `colour`, `dash`, `structure_colour`, `names` | a version of the line in that role |
| `[conductor_clearance]` | `show`, `circuits`, `mode` (`"also"`, `"instead"`), `label` (`{distance}`, `{rule}`, `{circuit}`, `{case}`, `{voltage}`), `label_at` (`"auto"`: clear of structures, wires and ground; `"middle"`, `"start"`, `"end"`), `pen` | lines below the lowest conductor |
| `[title_block]` | `corner` (`bottom_right`, `bottom_left`), `width` (`"auto"`: across the frame), `columns` (a length, `"*"` the room left, `"logos"` the export's logos), `rows`, `line`, `grid` | a grid in a corner |
| `[[title_block.cell]]` | `at = [row, column]`, `span`, `text`, `size`, `bold`, `align` (`left`, `centre`, `right`), `image` (an image's name, or `"{logos}"`) | its cells |
| `[revisions]` | `place` (`"above_title_block"`, `"off"`), `rows`, `row_height`, `size`, `columns = [{ title, width, value }]` | the last revisions, newest at the top |
| `[[note]]` | `at = [x, y]` (negative: from the right / bottom), `width`, `size`, `bold`, `text` | free texts |
| `[[field]]` | `key`, `label`, `default`, `required`, `per_sheet` | values the user gives |
| `[if_empty]` | `height_datum = "…"`, any field, `"*"` (every other) | what a field that comes out empty is written as |
| `[[image]]` | `name`, `file` (next to the template) or `data` (base64), `media_type` | images the cells name |

## Fields

Texts take placeholders `{name}` or `{name:format}`:

- **computed**: `project`, `line` (the project line), `route`, `route_name`,
  `heading`, `title` (the title lines; `title1`–`title3` one each),
  `sheet`, `sheets`, `from`, `to`, `from_structure`, `to_structure`,
  `ends` (`(T1 – T3)`, nothing for a station range), `hscale`, `vscale`,
  `plan_scale`, `exaggeration`, `scales`, `format`, `page_width`,
  `page_height`, `date` (the export's), `cases` (the weather cases drawn),
  `crs`, `program`, `height_datum`;
- **the template's own** (`[[field]]`): given in the export window, or
  `--field KEY=VALUE` (`KEY@SHEET=VALUE` for one sheet), `--fields
  FILE.toml` (`KEY = "value"`, `[per_sheet.KEY] 2 = "value"`,
  `[[revision]]`);
- **revision rows** (`[revisions]` columns): `rev.index`, `rev.date`,
  `rev.text`, `rev.by`, `rev.checked` — the rows kept with the drawing
  settings, or `--revision "A;2026-10-20;first issue;AB;CD"`.

Formats: `{sheet:03}` (zero padded), `{hscale:.0}` (decimals),
`{date:%d.%m.%Y}`, `{from:km}` (`0+640.0`, the default) or `{from:m}`
(`640.0`).

A **required** field left empty refuses the export, naming the fields
(a draft copy of the template can untick `required`).

**No empty gaps.** `{height_datum}` is the project's height datum
(Terrain › Coordinate system › Define…, e.g. `LN02 (EPSG:5728)`; see
[coordinate systems](coordinates.md#in-the-desktop-application-terrain--coordinate-system)),
else the vertical part of the coordinate system when it names one
(`EPSG:2056+5728` → `EPSG:5728`, a proj4 `+geoidgrids=`).
A field that comes out empty is written as `[if_empty]` says — by field
(`height_datum = "heights above the survey's datum"`) or `"*"` for every
other — else "unknown datum" (`height_datum`), "unknown coordinate
system" (`crs`) or "–". `ends` and `title`, `title1`–`title3` may say
nothing and stay empty unless `[if_empty]` names them.

Everything around the panels — the title block, the revision table, the
legend, the key map, the scale bars, the notes — lies **inside the
frame's padding**. A cell's text too wide or too tall for it is made
smaller (to 40 %); a revision cell's to 60 %, then shortened with "…".
A note is broken at the padding; one lying over the title block, the
revisions, the legend, the key map or the scale bars is warned (move
its `at`).

## The template's words

Templates carry their own texts. Every entry the export puts in the
legend has a kind, and `[legend.labels]` gives its words (English
without):

| Kind | Default | Placeholders |
|---|---|---|
| `axis` | axis of the line | |
| `corridor_near`, `corridor_far` | corridor edge, near (far) side ({side}) ±{width} m | `{side}` (right, left), `{width}` |
| `weather_case` | weather case {case} | `{case}` |
| `structure`, `structure_role` | structure, structure ({role}) | `{role}` |
| `clearance_violation` | clearance violation | |
| `datum_step` | datum step (heights read on each side) | |
| `ground` | ground on the axis | |
| `ground_side` | ground beside the axis | `{offset}` |
| `clearance_line` | ground + {distance} m (required clearance) | `{distance}` |
| `conductor_clearance` | lowest conductor of {circuit} ({voltage}) – {distance} m, rule "{rule}", {case}{note} | `{circuit}`, `{voltage}`, `{distance}`, `{rule}`, `{case}`, `{note}` |
| `conductor_clearance_note` | — the rule names no weather case: the case drawn with the largest sag, {case} | `{case}` |
| `match_line` | match line (sheet's end) | |
| `isoclearance` | isoclearance {level} | `{level}` |
| `lidar` | {name} (LiDAR class {class}) | `{name}`, `{class}` (a code or a group's key) |
| `version` | {label}: {words} | `{label}`, `{words}` |
| `background_drawing` | drawing {name} | `{name}` (the drawing's name) |
| `orthophoto` | orthophoto {name} | `{name}` (the image layers) |

The panels' titles are `[layout] plan_title` and `profile_title`; the
wires' words come from `[line_styles.kinds.*] label` and `[roles.*]
label`, the corridor's plan label from `[corridor] label`.

## Band rows

Kinds, all from what the drawing holds (no new engineering number):
`structures` (names and types), `stations`, `ground_height` (at the
structures and every `every`), `span_length`, `structure_height`,
`line_angle` (at the alignment's alignment points), `lowest_wire_height` (the
solid case's lowest wire in each span), `ground_clearance` (only from a
clearance check's results; empty otherwise, for now always empty in the
app), `text` (a field of the user, per sheet), `stringing` (since
2026-10-23: for each tension section — between a section's two end
structures — on the sheet, one line per circuit with its conductors'
horizontal tension in the stringing case, and one for the earth wires,
kN per subconductor, the lowest and highest of its spans as a range,
`380 kV …: 36.6–38.4 kN`; ticks at the sections' ends; the stringing
case from `[cases]` or the export window, none chosen: empty; its words
`[legend.labels]` `stringing` and `earth_wires`). The export window and
`--no-bands` switch them off.

## Weather cases (`[cases]`)

Since 2026-10-23 a template may give the weather cases the sheets draw
and the stringing tension's, by default:

```toml
[cases]
conductors = "80 °C"    # every circuit's phase conductors (the largest sag)
earth_wires = "10 °C"   # the earth wires
stringing = "10 °C"     # the stringing band's tension
```

Each is a weather case's name, or a temperature: the project's first
case at that temperature with no wind and no ice. None given: the case
shown (the stringing band empty). The export window's **Weather cases
per circuit** chooses, per circuit (each system, and the earth wires),
the case drawn and the stringing case over these. Each wire is drawn in
its circuit's case, the jumpers in their circuit's (that of the
section at their first end). `ohl drawing` takes them among the cases
given with `--case` (one missing is an error naming it).

A `structures` row writes its `format` for each structure (default
`"{name} {type}"`): `{name}`, `{type}` (its type and extension),
`{type_id}`, `{extension}`, `{station}`, `{height}` (above its base); a
line break (`"{name}\n{type}"`) makes two lines. What a structure does
not have (an untyped one's type) is left out, the spaces closed up.

## The corridor's sides and the centreline

With a corridor (`[corridor] width`, the export window's tick,
`--corridor-width`) and the ground, the profile also shows the **ground
along both edges of the corridor** (`[corridor] profile = false` leaves
them out). The **near side** is the one nearest a reader of the profile:
by default the **right**, looking along increasing stations — the plan's
lower edge, the side the profile is seen from when it lies under the
plan; `near_side = "left"` turns it round. Each edge has its own pen —
`near` and `far` over `pen` — the same in the plan, the key map and the
profile, so each ground line matches its edge; both are in the legend.
`--side-offset` still adds ground profiles at any offset.

The **ground on the axis** is always drawn solid (a dash of its layer is
ignored); `[centreline] ground = false` alone leaves it out. The axis in
the plan is drawn in `[centreline] pen` (the built-in look: grey,
dash-dot).

## LiDAR points

With `[lidar] profile = true` (by station and height) and/or `plan =
true` (seen from above) the survey's LiDAR points of the chosen classes
(`classes`; the export window's **LiDAR classes**, `--sheet-lidar-classes
2,3,5`) within the corridor — else the line width — are drawn as dots on
the layer `OHL_LIDAR`, under everything else. Each class (or group,
`[lidar.styles] "3-5"`) has its colour and size, default the class
scheme's colour; the legend names each class by the class scheme (a
group by its `label`). Only one dot per class is drawn in each square of
`cell` (0.2 mm) on paper: this thins what is drawn, never a computed
number. The app draws the points it has loaded (the project's point
budget); `ohl drawing` reads them from `--lidar` or `--pointcloud`
(the class scheme from `--lidar-classes`, else the one stored with the
points, else ASPRS).

## Drawings and orthophotos under the plan

The project's backgrounds (Terrain › Backgrounds: DXF drawings and
orthophotos kept in the project file) can be drawn under the plan,
**straightened along the line** like everything else there: a point at
station *s* and offset *o* from the axis is drawn at *s* along the sheet
and *o* across it. They are display only; nothing computed depends on
them, and the sheet breaks do not move.

```toml
[plan_background]
drawings = true            # or ["cadastre", "survey"]; default false
texts = false              # the drawings' texts
points = false             # the drawings' points, as dots
own_colours = false        # else every line in `pen`'s colour
pen = { colour = "#8c8c8c", width = "0.13 mm" }
orthophoto = true          # or ["Orthophoto 2024"] (image layers); default false
veil = 0.6                 # white veil over the photo, 0 (none) to 1
dpi = 200                  # the photo's resolution on paper, 50 to 600
quality = 75               # its JPEG quality, 10 to 100
```

| Key | Default | What |
|---|---|---|
| `drawings` | `false` | the project's drawings: `true` every one, or a list of their names |
| `texts` | `false` | their texts, at their letter height on the ground (at the sheet's scale along the line; left out below 0.5 mm, at most 10 mm), written along the sheet (not turned) |
| `points` | `false` | their points, as dots of 0.5 mm |
| `own_colours` | `false` | each line in the drawing's own colour (white drawn black); else every one in `pen`'s colour |
| `pen` | 0.13 mm, `#8c8c8c` | the drawings' pen (with `own_colours`: its width and dash) |
| `orthophoto` | `false` | the project's orthophotos: `true` every image layer, or a list of layers; where several cover a point, the last (by layer and name) is drawn |
| `veil` | `0.6` | a white veil over the photo, so that the line stays readable: each colour mixed with that share of white |
| `dpi` | `200` | the photo's resolution on paper |
| `quality` | `75` | its JPEG quality |

**Drawings.** Their lines are taken within three times the line width
(or the corridor's half width, when wider) of the axis: each segment is
cut into pieces of at most 2 m before it is straightened, so a long
straight line beside a bend follows the bend on paper, and cut where it
leaves that width. Lines on a layer switched off in the DXF are left
out. On each sheet they are clipped to the plan panel and drawn on the
layer `OHL_BACKGROUND_DRAWINGS` (SVG, DXF, PDF, PNG), under everything
else; the legend lists each drawing. Not in the plan's model-space DXF
(the drawing itself is already in project coordinates there).

**Orthophotos.** Each sheet's plan panel is filled pixel by pixel at
`dpi`: every pixel's station and offset is taken back to the ground
(the centreline's segment at that station, the offset along its normal)
and the photo read there, at the coarsest level of its pyramid that is
not coarser than the pixel (bilinear between its pixels). The image is
veiled, written as one JPEG and drawn under everything else on the
layer `OHL_BACKGROUND_PHOTO`: embedded in SVG, as the JPEG itself in
PDF, drawn in PNG. **DXF R12 has no images**: the DXF sheets leave it
out, with a comment in the file and a warning of the export. An A1 plan
panel of 800 × 150 mm at 200 dpi is about 15 million pixels; a sheet's
photo has at most 40 million (a lower resolution beyond). The sheet
preview of the export window draws it at 60 dpi.

**At bends.** The plan is straightened segment by segment, the
backgrounds exactly like the wires and the structures: on the outside
of a bend a wedge of ground is not shown, on the inside a wedge is shown
twice — the wider the panel and the sharper the bend, the larger. The
axis and everything along it stay true.

The export window's **More of the template** ticks *the project's
drawings under the plan* and *the project's orthophotos under the plan*
switch them on or off over the template (on: the names the template
lists, else every one; kept in the drawing settings). On the command
line: `--plan-drawings` and `--plan-orthophoto` (alone: the template's
names, else every one; or `--plan-drawings cadastre,survey`,
`--plan-orthophoto "Orthophoto 2024"`), `--no-plan-backgrounds`,
`--plan-background-dpi 150`, and `--backgrounds FILE.ohl` for the
project file to read them from (default: the line's, when it is a
project file). Hiding a background in the views does not leave it out
of the sheets.

## Versions of the line, kinds and roles

Several versions of the line can be drawn on one sheet — the project
line worked on and other **project lines** (or line files on the command
line) — each in a **role**: `existing`, `project`, `removal`,
`decommissioned`, `other`, or any role the template defines. Each is
solved in the same weather cases; its structures and wires near the
route (within the line width) are drawn along the route's centreline, in
plan, profile and elevations, on the layer `OHL_VERSIONS`, labelled
(`{name}`, `{voltage}`) and listed in the legend. The line's own role
(`--role`, the window's **This line**) styles the line itself (e.g. new
thick, existing thin). Other lines nearby or crossing (a line file given
with `--version FILE=other`) are drawn the same way, in grey by the
example template, labelled with their name and voltage.

With `[line_styles] by_kind = true` the wires are drawn by **kind**
instead of their phase colour: no system (earthed) is an **earth wire**;
a system up to `low_voltage_up_to_kv` is **low voltage**, above it **high
voltage** (no limit given: every system with a voltage is high voltage);
a section's field named by `kind_field` (a column a plugin adds, e.g.
`plugin.line_kind` = `hv`, `lv`, `telecom` or `earth`) overrides that,
and `construction_field` = `cable` draws a cable dashed. A role's colour,
width (or weight) and dash go over the kind's; another weather case's
dash goes over a solid line. The **legend** lists every pen, symbol and
line type a sheet actually draws.

## Lines below the lowest conductor

With `[conductor_clearance] show = true` (or the export window's tick,
`--conductor-clearance`) the profile carries, for each circuit, a line
**parallel to its lowest conductor, lowered by the ground clearance its
rule requires**: the ground must stay below it. The distance comes from
the project's clearance rules for the ground (`target = "ground"`,
`vertical`; by the circuit's voltage: `by_voltage`, `base` / `per_kv` /
`above_kv`) — the rules of Checks › Ground clearances (a design code's
preset fills them in), else the rule file chosen; `--rules` /
`--clearance` on the command line. Its weather cases are the rule's
(`cases`); a rule naming none takes the drawn case with the largest sag,
and the legend says so. A rule naming cases that are not drawn gives no
line and a warning. Each line is labelled (`label`) and listed in the
legend; `mode = "instead"` leaves out the line above the ground. See
[Plan and profile drawings](../theory/drawings.md#lines-below-the-lowest-conductor).

## Images per format

| Format | Logos and images |
|---|---|
| SVG | embedded |
| PNG | drawn |
| PDF | embedded as images (SVG drawn at about 300 dpi), cropped to what they cover |
| DXF R12 | SVG images as their **outlines** (layer `OHL_TITLE_IMAGE`); PNG and JPEG left out, said in the export's warnings (R12 has no images) |

The orthophoto under the plan: embedded in SVG (JPEG), the JPEG itself
in PDF, drawn in PNG, left out of DXF (said in the export's warnings).

The title block, its filled fields, the revision table, the band rows,
the legend, the key map, the corridor, the versions and the lines below
the lowest conductor have their own DXF layers (`OHL_TITLE`,
`OHL_TITLE_FIELDS`, `OHL_REVISIONS`, `OHL_BANDS`, `OHL_LEGEND`,
`OHL_KEY_MAP`, `OHL_CORRIDOR`, `OHL_VERSIONS`, `OHL_CONDUCTOR_CLEARANCE`,
`OHL_LIDAR`, `OHL_BACKGROUND_DRAWINGS`; the ground along the corridor's edges on `OHL_GROUND_SIDE`).

## What is not there yet

- Only the PDF standard font Helvetica: embedding another font (a subset
  of an open font, on a template's request) is planned.
- No title block editor in the app: the file is the editor (Save as
  file…, edit, Load file…).
- The key map has no background image (orthophoto).
- No DXF 2000 with image references (PNG/JPEG logos and the plan's
  orthophoto in DXF).
- The drawings' texts under the plan are not turned with the drawing.
