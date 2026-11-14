# Tool pages: design guidelines

How every **tool window** (the small window a tool opens when something
is clicked) and every **tool's page in the user guide** is built, in
OHL-Design and OHL-Structure alike, so that someone who does not know the
program understands what a tool does and how to use it (owner,
2026-11-14: "user friendlier for humans that don't know the programm";
"write this in the design as for the checks pages, how the tool pages
should be"). They extend the [desktop app conventions](ui.md) and sit
beside the [check pages](check-pages.md), which cover the check and
calculation windows. The shared code is `ohl_ui::tool_window` (both
applications), with OHL-Design's `crate::tool_ui` and OHL-Structure's
`help::tool_intro` opening each one's manual. A window that departs from
these guidelines says why next to the code. Built, in active development,
not approved yet.

## 1. The shape of a tool window

1. **Title**: what happens to what — "Move structure T3", "Add
   structure", "Delete", "New gauge line?".
2. **At the top, one plain sentence** of what the window does and what to
   do in it, in words a line designer uses (`tool_window::intro`). Beside
   it a **(?)** with the longer explanation, and a **User guide ↗**
   button opening the tool's section of the manual (§ 3). Why the manual
   did not open is said under the row.
3. **The fields**, in the order they are filled:
   - units in the label or the value (`E (m)`, `75 m`);
   - a name whose help was only on hover gets a visible **(?)**
     (`tool_window::label`);
   - a section's explanation goes behind a (?) at its title
     (`tool_window::heading`), not as small grey lines under it; a field
     keeps its label, its value and at most one short line.
   - **Always visible**: every warning and refusal, what is not on the
     safe side, what blocks the action, and what a deletion takes with it.
4. **One button row at the bottom** (`tool_window::buttons`), after a
   line: the window's **action first**, named by what it does (Apply, Add
   the point, Delete, Replace, Keep, Add the line and the bolt), strong;
   **Cancel last** (a question that is not a cancellation keeps its own
   word: Stay, Back). A greyed action says on hover why.
5. **Keys** (said on the buttons' hover):
   - **Escape** cancels the window, whatever else would take it (an
     alignment laid out, the tool held, the Detail view): the window is
     closed, nothing changes, what was being laid out stays.
   - **Enter** does the action where it is safe; **a deletion's
     confirmation never takes Enter**.
   - While the window is open, the tool's own keys (Enter, Backspace,
     Escape of a tool laying points out) wait.
6. **Where**: next to the click (in first person, at the crosshair);
   dragged anywhere; a captured mouse freed.
7. **One undo step**: the action is one change of the project or the
   structure, however many objects it touches; a refused change keeps the
   window open with the reason.

## 2. What a tool does not do

- **The tools act in the 3D view** (and a cut's panel, which is the 3D
  view clipped). **The plan and the profile use no tool**: a left-click
  selects, a right-click opens the object's menu, whatever tool is held
  (owner, 2026-11-01 and 2026-11-14).
- **Rights**: a tool that changes the line needs editing rights; Select,
  Go to structure, the measuring tools and Note do not. A viewer keeps
  measurements and notes of their own, deletes and undoes them, and
  nothing else (owner, 2026-11-14).
- **What hangs on an object goes with it**: deleting a point takes
  everything hanging on it (spacers, the jumpers tapped on it and those
  hanging on them), in the same undo step; what is asked first (a
  spread) is asked in a window of this shape.

## 3. Its section in the user guide

Every tool has a section of its own:

- **OHL-Design**: one page per theme of the inventory,
  `docs/user-guide/tools-<theme>.md` (General, Alignments, Structures,
  Wires, Measure), with [Tools](../user-guide/tools.md) as their index (the
  hotbar, the inventory, using a tool, the tool windows, a table of every
  tool).
- **OHL-Structure**: [OHL-Structure tools](../user-guide/structure-tools.md),
  the 3D view's tools, then the Detail view's.
- **The heading is the tool's name**, exactly as the program names it
  (`Tool::name()`): the User guide buttons open it by its anchor, and a
  test fails when a tool has none (`every_tool_has_its_section`, in both
  applications). Where two tools share a name, the second's heading says
  where (*Edit in the Detail view*).

Each section, in this order, in plain words and the
[Glossary](../user-guide/glossary.md)'s names:

1. **Its window's picture** (when it opens one), with alt text naming
   what is in it.
2. **What it is for**: one or two sentences.
3. **How to use it**: numbered steps — how to hold it (slot, key, menu),
   what to click, what opens, what to press. A table where a click on
   different things does different things.
4. **The window**: a table *Field | What it does*, in the window's order,
   each with its unit and default; the buttons and keys after it.
5. **Keys** and **Options** (the strip above the view), where it has
   them.
6. **Good to know**: what it refuses and why, what else changes with it,
   where to change it later, and **Undo** (what one step takes back).

The tutorial shows the tools in use and links to their sections; the
general pages keep a summary and link too. A tool's behaviour changed
means its section changed in the same commit.

## 4. Pictures

- Rendered by tests, never by hand, made as `USER=tester`:
  OHL-Design `tool_windows_rendered_for_review` (`tests/tool_shots.rs`),
  OHL-Structure `tool_windows_rendered_for_review`
  (`tool_window_tests.rs`, and the gauge line in `joint_ui_tests.rs`).
  With `OHL_RENDER_PNG` set, `tool-<name>.png` is the window alone (the
  user guide's pictures, `docs/images/tools/` and
  `docs/images/structure-tools/`), `screen-tool-<name>.png` where it
  opens.
- **Looked at before pushing**: nothing overlapping, nothing cut off, the
  example line's names only.
- A rework of the windows is **reviewed on before and after pictures**
  (kept in `local-data/reviews/`, not committed) and approved by the
  owner.

## 5. Checking a tool against these guidelines

- The window: the sentence, the (?), the User guide button, the fields'
  units and (?), the button row, Escape and Enter as § 1 says.
- The page: a section named as the tool, in the order of § 3, its picture
  current.
- The tests: `every_tool_has_its_section`, the rendering test, and the
  tool's own behaviour tests (one undo step, refusals said).
