# Desktop app conventions

Rules every page, menu and window of the desktop app (`crates/ols-app`)
follows. A change that breaks one needs a reason written next to it.

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
