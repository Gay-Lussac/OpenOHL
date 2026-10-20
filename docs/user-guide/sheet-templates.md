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
| `[page]` | `format` (A0–A4), `orientation` (`landscape`, `portrait`), `width` + `height` (a long format instead), `margin = { left, top, right, bottom }` | the sheet; a wide left margin is the binding edge |
| `[frame]` | `border = [{ inset, width }, …]`, `padding` | the frame's lines, the room inside them |
| `[text]` | `font` (`Helvetica` only for now), `size`, `colour` | the labels' and the title block's texts |
| `[layout]` | `order` (`["plan", "profile", "bands"]` or the plan last), `gap`, `plan_height` (`"auto"` or at least a length), `beyond_match` (`"none"` or `"faded"`), `axis_room`, `station_labels` | the panels |
| `[layers]` | `OHL_GROUND = { width, colour, dash }` … any layer | pens by layer |
| `[[band]]` | `kind`, `height`, `label`, `every`, `field`, `decimals` | rows under the profile ([below](#band-rows)) |
| `[north_arrow]` | `place` (`"plan"`, `"off"`), `size` | one per straight stretch at the plan's top edge |
| `[scale_bar]` | `place` (`"bottom"`, `"plan"`, `"profile"`, `"off"`), `lengths = ["100 m", "10 m"]` | graphic scales along and up |
| `[legend]` | `place` (`"above_title_block"`, `"bottom"`, `"off"`), `title`, `width`, `height`, `column_width`, `size` | made from what each sheet draws |
| `[key_map]` | `place` (`"bottom"`, `"off"`), `width`, `height`, `scale`, `extent` (`"route"`, `"sheet"`), `margin`, `structures`, `north_arrow`, `corridor`, `highlight = { pen }` | a small plan of the route, this sheet marked |
| `[corridor]` | `width`, `label` (`{width}`), `pen` | the corridor's edges at ± `width` from the axis |
| `[elevations]` | `place`, `offset`, `rotation`, `scale`, `labels` (`number`, `type`, `height`, `string_length`), `size`, `pen` | the structures' elevations on the plan |
| `[line_styles]` | `by_kind`, `low_voltage_up_to_kv`, `kind_field`, `construction_field`, `cable_dash`, `[line_styles.kinds.hv|lv|telecom|earth] = { colour, width, dash, label }` | wires by kind |
| `[roles.<role>]` | `label`, `width`, `weight`, `colour`, `dash`, `structure_colour`, `names` | a version of the line in that role |
| `[conductor_clearance]` | `show`, `circuits`, `mode` (`"also"`, `"instead"`), `label` (`{distance}`, `{rule}`, `{circuit}`, `{case}`, `{voltage}`), `pen` | lines below the lowest conductor |
| `[title_block]` | `corner` (`bottom_right`, `bottom_left`), `width` (`"auto"`: across the frame), `columns` (a length, `"*"` the room left, `"logos"` the export's logos), `rows`, `line`, `grid` | a grid in a corner |
| `[[title_block.cell]]` | `at = [row, column]`, `span`, `text`, `size`, `bold`, `align` (`left`, `centre`, `right`), `image` (an image's name, or `"{logos}"`) | its cells |
| `[revisions]` | `place` (`"above_title_block"`, `"off"`), `rows`, `row_height`, `size`, `columns = [{ title, width, value }]` | the last revisions, newest at the top |
| `[[note]]` | `at = [x, y]` (negative: from the right / bottom), `width`, `size`, `bold`, `text` | free texts |
| `[[field]]` | `key`, `label`, `default`, `required`, `per_sheet` | values the user gives |
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

## Band rows

Kinds, all from what the drawing holds (no new engineering number):
`structures` (names and types), `stations`, `ground_height` (at the
structures and every `every`), `span_length`, `structure_height`,
`line_angle` (at the alignment's angle points), `lowest_wire_height` (the
solid case's lowest wire in each span), `ground_clearance` (only from a
clearance check's results; empty otherwise, for now always empty in the
app), `text` (a field of the user, per sheet). The export window and
`--no-bands` switch them off.

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

The title block, its filled fields, the revision table, the band rows,
the legend, the key map, the corridor, the versions and the lines below
the lowest conductor have their own DXF layers (`OHL_TITLE`,
`OHL_TITLE_FIELDS`, `OHL_REVISIONS`, `OHL_BANDS`, `OHL_LEGEND`,
`OHL_KEY_MAP`, `OHL_CORRIDOR`, `OHL_VERSIONS`, `OHL_CONDUCTOR_CLEARANCE`).

## What is not there yet

- Only the PDF standard font Helvetica: embedding another font (a subset
  of an open font, on a template's request) is planned.
- No title block editor in the app: the file is the editor (Save as
  file…, edit, Load file…).
- The key map has no background image (orthophoto).
- No DXF 2000 with image references (PNG/JPEG logos in DXF).
