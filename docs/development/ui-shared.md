# A shared UI crate for both applications

Status: built for the look, the notices and messages, the long tables, the
artwork's drawing and the glyph test (2026-11-07), in active development, not
approved yet. The hotbar's drawing and the check footer are still to come.

OHL-Design (`crates/ohl-app`) and OHL-Structure
(`apps/structure/ohl-structure-app`) use one copy of the same look and widgets,
`crates/ohl-ui`, so a fix in one reaches the other.

## What is in it

| Module | What | What each application keeps |
|---|---|---|
| `style` | Text sizes, solid scroll bars, value fields unlike buttons, the UI scale list and menu, sentence case and unit headers | Its storage key; OHL-Structure its 22 pt rows and right-aligned value fields (`field`, `num`, `num_label`) |
| `messages` | `message`, `error_label`, `ok_color`: long texts scroll in a box of fixed height | |
| `notices` | Notices that fade, errors that stay | How often the window is drawn while one fades in its tests; OHL-Design the short sentences it makes of an edit's message (`plain`) |
| `rows` | Long tables, only the rows in view | |
| `icons` | SVG files rasterised and kept as textures, the OpenOHL mark | Its own tools or actions and its application icon |
| `glyphs` | The helpers of the glyph test | Its test, on its own source files; `ohl-ui` tests its own texts |

The two applications differed in three small ways, now one (2026-11-07): the
box long errors scroll in is 160 points high in both (OHL-Structure's was 120),
OHL-Structure's UI scale is stored as plain text as OHL-Design's (values kept
before are still read), and `Table::header` writes sentence case in both.

## What is left

1. **The hotbar drawing** (slots, inventory grid, drag and drop) with the
   tools as a trait each application implements: `tools.rs` in each still has
   its own copy, the drawing alike, the tools different.
2. **The check footer** (Run/Stop, progress), when OHL-Structure gets windows
   that run: `check_help.rs` in OHL-Design until then.

## The separation rule

`ohl-ui` depends on `eframe`/`egui`, `resvg` (for the SVG files) and `log` only:
no model, no engine, no format crate. It never depends on an `apps/structure`
crate (core crates do not) and never on `ohl-app`. The owner added it to what
`ohl-structure-app` may use on 2026-11-07
([Structure application crates](structure-app.md)).
