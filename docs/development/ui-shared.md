# A shared UI crate for both applications (plan)

Status: a plan (2026-10-27), not built. Nothing here is approved yet.

OHL-Design (`crates/ohl-app`) and OHL-Structure
(`apps/structure/ohl-structure-app`) each have their own copy of the same
look and widgets. They drift: a fix in one is forgotten in the other.

## What is duplicated today

| Piece | OHL-Design | OHL-Structure | How alike |
|---|---|---|---|
| Look: text sizes, solid scroll bars, value fields unlike buttons | `style.rs` (`apply`, `field_visuals`, `field`) | `style.rs` (`apply`, `field_visuals`, `field`) | the same code, another storage key |
| UI scale menu and storage | `style.rs` (`SCALES`, `load_scale`, `save_scale`, `scale_menu`) | `style.rs` (`SCALES`, `load_scale`) | the same, key `ohl-design-ui-scale` / `ohl-structure-ui-scale` |
| Table headers: sentence case, unit `x [m]` | `style.rs` (`sentence`, `unit_of`, `column_header`) | `style.rs` (`sentence`, `header`, `num`, `num_label`) | the same rule, two signatures |
| Notices (messages that fade, errors that stay) | `notices.rs` | `notices.rs` | the same behaviour |
| Hotbar, inventory, tool strip | `tools.rs` | `tools.rs` ("its drawing copied, not shared") | the drawing alike, the tools differ |
| Check footer with Run/Stop, body scrolling above | `check_help.rs` | (its own panels) | OHL-Structure has no check windows of this kind yet |
| Error popups that scroll, windows that stay closable | `menu.rs` (`error_label`, `message`) | in place | alike |
| Glyph test (every character of the texts has a glyph) | `glyph_tests.rs` | `glyph_tests.rs` | the same test |

## What to share, in order

1. **The look** (`style.rs`: text styles, scroll bars, field visuals,
   `field`, the UI scale list and menu with the storage key as a
   parameter, `sentence` and the unit header). Pure egui, no model: the
   smallest step and the most copied.
2. **Notices** and the scrolling error/message labels.
3. **The hotbar drawing** (slots, inventory grid, drag and drop) with the
   tools as a trait each application implements.
4. **The check footer** (Run/Stop, progress), when OHL-Structure gets
   windows that run.
5. The glyph test as a helper both test suites call.

## Where it lives, and the separation rule

A new crate `crates/ohl-ui` depending on `eframe`/`egui` only (no model,
no engine, no format crate). The separation rule
([Structure application crates](structure-app.md)) keeps the structure
crates apart from `ohl-app` and lets `ohl-structure-app` use, besides
its own crates and the model and format crates, `ohl-render` (drawing
only) and `ohl-plugins`. It does not forbid a UI crate of the core that
holds no engine — `ohl-ui` would be like `ohl-render`, used for drawing
only — but it does not list one either, so **adding it to that list is
the owner's decision** before the first step: "`ohl-structure-app` also
`ohl-render`, `ohl-plugins` and `ohl-ui` (the look and widgets only)".
`ohl-ui` must never depend on an `apps/structure` crate (core crates do
not), and never on `ohl-app`.

## The first step, once agreed

Move the look (item 1) into `crates/ohl-ui::style`; both `style.rs`
re-export it (`pub use ohl_ui::style::*`) with their own storage key, so
no call site changes; the existing style tests of both applications stay
where they are and pass unchanged. The two `style.rs` then hold only
what differs.
