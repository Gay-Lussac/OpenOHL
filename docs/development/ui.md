# Desktop app conventions

Rules every page, menu and window of the desktop app (`crates/ohl-app`)
follows. A change that breaks one needs a reason written next to it.
The check and calculation windows follow, besides, the [check pages'
design guidelines](check-pages.md).

## Names

Every label, tooltip, message and help text uses the terms of the
[Glossary](../user-guide/glossary.md), the same in both applications, the
command line and the manual (owner decision 2026-10-21). A new object gets
its glossary entry (term, meaning, the names it replaces, German) in the
same change. After renaming a heading, run `python3 -I
tools/check_doc_links.py`: mdbook does not check anchors.

## Menus

- **Menus of options stay open while boxes are ticked.** Display options,
  layers, snaps, classes, lines shown — anything where a user may tick
  several boxes in a row — is made with `crate::menu::options_menu`: a
  tick never closes it; only a click outside it, or a command, does.
  Commands in such a menu close it themselves (`ui.close()`), as every
  menu command does. A plain `ui.menu_button` closes on any click inside
  and is only for menus of commands.
- Commands of the menus are `menu::Command`s where they can be, so the
  quick bar can hold them.

## Display preferences

- What a user shows (layers, surfaces, LiDAR classes, colours, notes on
  the wires…) is that user's own: kept in `ViewPrefs`, per project file on
  their computer, never in the project nor sent to a server.
- What changes results (the surface the checks use, rules, cases) belongs
  to the project and goes through the session (logged, undoable).

## Notes on views

- Text written in a view next to an object uses the object's colour, with
  no box around it; notes that would overlap one already written are left
  out (the nearest first in the 3D view).

## Errors and reports in windows

- Every error, list of errors or report shown in a window goes through
  `crate::menu::error_label` (red) or `crate::menu::message`: short text
  is shown as it is, long text (over 300 characters or 4 lines) scrolls in
  a box of fixed height. A window must never grow beyond the screen: its
  Close / OK / Cancel buttons always stay in reach.
- Many files failing for the same reason are reported once, with the
  number of files and a few of their names, not one line per file.
- What cannot work is refused before a job starts (e.g. ECW files without
  a converter), not reported once per file after.

## Tool windows

The windows a tool opens when something is clicked (Add structure, Move
structure, the strings' and sections' windows, the coordinates, Replace
structure, a note…) are laid out alike so that someone who does not
know the program can use them (owner, 2026-11-14; built, in active
development, not approved yet). The shared code is `crate::tool_ui`.

1. **Title**: what happens to what ("Move structure T3", "Add
   structure").
2. **At the top, one plain sentence** of what the window does and what
   to do in it (`tool_ui::intro_of`); the longer explanation behind a
   **(?)** next to it, and a **User guide ↗** button opening the tool's
   section of [Tools](../user-guide/tools.md) (one page per theme of the
   inventory, a heading per tool named as the tool; a test checks every
   tool has one).
3. **Fields**: units in the label or the value; a name whose help was
   only on hover gets a visible **(?)** (`tool_ui::label`); a section's
   explanation behind a (?) at its title (`tool_ui::heading`), not as
   small grey lines under it. Warnings, refusals and what is not on the
   safe side stay visible.
4. **One button row at the bottom** (`tool_ui::buttons`): the action
   first, named by what it does (Apply, Add the point, Delete all,
   Replace, Keep), Cancel last; Enter for the action where the window
   takes it (never for a deletion's confirmation), Escape for Cancel,
   both said on hover.
5. **Nothing else changes** in such a rework: the commands, fields,
   defaults and results stay; every change is still one undo step.

The windows are rendered for review by the test
`tool_windows_rendered_for_review` (`OHL_RENDER_PNG`, made as
`USER=tester`): `tool-<name>.png` is the window alone (the user guide's
pictures in `docs/images/tools/`), `screen-tool-<name>.png` where it
opens.
