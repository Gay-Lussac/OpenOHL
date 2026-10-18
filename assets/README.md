# Artwork

- `brand/`: the OpenOHL mark (`openohl-mark.svg`, `openohl-mark-plain.svg`
  without its background), the wordmark (light and dark), and the
  application icons `ohl-design-icon.svg` and `ohl-structure-icon.svg`.
  `review-sheet.svg` shows everything on one page.
- `icons/tools/`: the tool icons, 24 × 24, drawn with lines in
  `currentColor` (they take the text colour, light or dark theme).

## Where the applications use them

The applications build these files in (read when they are compiled):
**replacing a file with another of the same name and format** (an SVG,
the same size; tool icons drawn in `currentColor`) **updates the
applications at the next build** — nothing else to change. They are
drawn with resvg in the program (`crates/ols-app/src/icons.rs`,
`apps/structure/ols-structure-app/src/icons.rs`); the tests check that
every file parses.

| File | Used in |
|---|---|
| `brand/ohl-design-icon.svg` | OHL-Design's window (and Dock / taskbar) icon |
| `brand/ohl-structure-icon.svg` | OHL-Structure's window (and Dock / taskbar) icon |
| `brand/openohl-mark.svg` | both start pages (above the name), OHL-Design's Help menu, OHL-Structure's About window |
| `icons/tools/select.svg` … `replace.svg` (14 files: select, move, delete, goto, edit, add-structure, add-string, string-wire, add-alignment, new-alignment, edit-alignment, add-jumper, on-span, replace) | OHL-Design's tools: the hotbar slots, the inventory, the Tools menu, the quick bar |
| `icons/tools/add-joint.svg` | OHL-Structure: Body › Add a joint; Add joint (Joints and Members panels); Joint (Extensions panel) |
| `icons/tools/add-member.svg` | OHL-Structure: Body › Member between the two joints; Member (Joints and Members panels, Extensions panel) |
| `icons/tools/point-on-joint.svg` | OHL-Structure: Body › Point on the joint; Point here |
| `icons/tools/generator.svg` | OHL-Structure: Body › Generators… |
| `icons/tools/symmetry.svg` | OHL-Structure: Body › Symmetry ›; symmetry (Joints and Members panels) |
| `icons/tools/support.svg` | OHL-Structure: Design › Supports… |

The other files (the wordmarks, `openohl-mark-plain.svg`,
`review-sheet.svg`) are for the website and documents. There is no
macOS application bundle yet (the apps start from `cargo run` or the
`.command` files), so no `.icns`.

All are drawn by `make_icons.py` (Python, no dependencies):

```sh
python3 -I assets/make_icons.py .
```

Colours: blue `#0a6cdc`, dark blue `#0d4f9e`, ink `#0b1f3a`, yellow
`#ffd36b`.
