# Desktop application

`ols-cad` opens a line file or a project and shows it in dockable views that share the selection and the weather case.

```bash
cargo run --release -p ols-app -- examples/synthetic-line/synthetic-line.ols.json \
    --terrain examples/synthetic-line/synthetic-survey.csv \
    --weather examples/synthetic-line/weather-cases.olswc

cargo run --release -p ols-app -- project.ols          # with the project's point cloud
```

| Option | Meaning |
|---|---|
| `--library` | Wire library (file or directory); default: the `*.olsw` files next to the model, else `libraries/cables`. |
| `--structures` | Pylon files; default: the `Structures/` folder next to the model. |
| `--weather` | Weather cases (`.olswc`); default: still air at 15 °C. All cases are solved. |
| `--terrain` | Survey points for the ground. |
| `--pointcloud` | Point cloud in a project (`FILE.ols` or `FILE.ols#name`); default: the project's own. |
| `--points` | Most LiDAR points shown (default 2 000 000). The ground surface is built from all ground points (lowest per 1 m cell) in the corridor. |
| `--corridor` | Half width of the LiDAR corridor around the spans (default 60 m). |
| `--controls` | Navigation scheme: `orbit` or `minecraft` (default: the one used last). |
| `--screenshot FILE.png` (`--focus ID`, `--distance M`) | Load, render the 3D view to an image and exit (with `--focus`, looking at that structure's first attachment). |

## Menus, quick bar and pages

The **menu bar** holds what needs no view:

| Menu | |
|---|---|
| File | the model's file, Quit |
| Edit | Undo, Redo, Delete selected, Clear selection, Go to structure; **Snap: what can be clicked** |
| View | Fit views; the pages to show (tick to open, untick to close); layers; weather case; Customize the quick bar |
| Tools | the tool inventory (E); hold any tool |
| Checks | calculations run **on demand** — they never change the model: Clearances (Run checks, also after every edit, rule file, Show results); more checks to come |
| Libraries | the library editors: wires, insulators, structure types, weather cases (see [Library editors](#library-editors)) |
| Tables | the line as tables: structures, sections, strings, jumpers, electrical systems (see [Tables](#tables)) |
| Plugins | to come (the plugin manager) |
| Help | version, keys and mouse |

Under it, the **quick bar**: the line's name, the weather case, the selection, then the commands chosen for it (default Undo, Redo, Fit views, Run checks). **⚙** (or View › Customize the quick bar) adds any command of the menus — commands, library editors, tools — removes and orders them; the bar is remembered.

**Pages** work like browser tabs: each has a tab with a close button; drag a tab to rearrange, or out of the window to make a floating, resizable window with its own row of tabs (drop more pages on it). Closed pages come back from View. Library editors open together in a floating window.

## Views

- **3D** — ground (shaded), LiDAR points coloured by class role (vegetation green, buildings red, water blue, wires yellow…), structures, insulator strings, wires in their phase colours (bundles as their subconductors, `bundle_spacing` apart — one section, solved and selected as one — held at every clamp by a yoke: a bar for two, a triangle for three, a polygon for more; the back and ahead spans and the jumpers end on its corners), alignments. Navigation: see [Controls](#controls). Height exaggeration in the Model panel.
- **Plan** — alignments, wires and structures from above, north up, with a scale bar. Drag, scroll at the pointer, double-click to centre.
- **Profile** — per route: ground along the centreline, structures and wires by station; heights fit the visible stretch. Scroll zooms along the route, drag moves, double-click shows the whole route.
- **Properties** — the selected structure (position, type, attachments, spans through it) or span (cable, circuit, electrical system and voltage, bundle, horizontal and maximum tension, sag and lowest point in every weather case). Links select related objects.
- **Model** — layers, weather cases, what was loaded and how long it took.
- **Clearances** — clearance violations of the shown weather case (see [Clearances](#clearances)).
- **Properties** of the other kinds: an attachment point (string type, insulator and wire points, length, weight, spans and jumpers there), an alignment point (position, tied structure), a LiDAR point (position, class and roles).

## Editing

A project (`.ols`) opened by a user with the designer or admin role (`--user`, default the login name) can be edited; every edit is a command in the project's log, undoable per user (ADR-0005). A line file, or a user without a role, opens read-only — the status bar says why.

| Action | How |
|---|---|
| Move a structure | **M** (or the Move tool in the hotbar), then right-click a structure in any view: the **Move structure** window shows E, N, z (or "base on the ground"); the change is previewed live (the shown weather case re-solves the sections through it); **Apply** (Enter) makes one command, **Cancel** (Escape) puts it back. |
| Delete | The **Delete** tool and a right-click on a structure, a string (its insulator or wire point), a span (deletes its section) or a jumper — or **Delete** / **Backspace** for the selected one. When it takes other objects with it (a string: the sections through it and their jumpers; a structure: its sections, jumpers, routes…), a window lists them; **Delete all** applies it as one command. A string of a structure type is left out of that structure (`omit` in the line file). |
| Edit | The **Edit** tool and a right-click: on a structure, its window (type and extension from the structure library, orientation, position or base on the ground; explicit strings turn and move with it); on a string, its window (insulator point, wire point and V legs in the structure's own axes — across, along, up from the base —, length, weight, insulator, strain string; on a typed structure the change overrides the type's string, and **Back to the type's string** removes the override); on a span, its section's window (cable from the wire library, circuit, phase, electrical system, bundle and spacing, status *in the blocks* (rolling: one tension in all spans in every weather case; default) or *clipped in* (each span keeps its unstressed length; switching computes them from the condition), sagged to a horizontal tension or to a sag in the ruling span or in a chosen span, at a temperature — see [Stringing and status](line-file.md#stringing-and-status) —, and **Pick points again**). |
| Add a structure | The **Add structure** tool and a right-click on the ground (3D view, under the crosshair in first person; or the plan view): the window proposes the next id, the type and orientation of the nearest structure, the base on the ground. Without a type, give it strings with **Add string**. |
| Add a string | The **Add string** tool and a right-click on a structure: the next free set, phase 1, hanging 3 m from the top; set the points in the structure's axes. |
| String a wire | The **String wire** tool: right-click the strings in line order (a yellow line joins them), **Enter** opens the new section's window — like the selected span's section, or the one through the first string, in the blocks with its condition (or its tension as solved) —, **Backspace** takes the last point back, **Escape** gives up. **Pick points again** in a section's window strings it again through new points (a clipped section with a different number of spans goes back to the blocks). |
| Undo / redo | **⌘Z** / **Shift+⌘Z**, the Edit menu or the quick bar. |

Every window applies one command (**Apply** or Enter; **Cancel** or Escape); when the session refuses it (an id taken, a line that would not be valid…), the window stays open with the reason. While Delete, Edit, Add string or String wire is held, strings can be right-clicked whatever Edit › Snap says.

After an edit only the sections it touched are re-solved, in every weather case (incremental solving); the status bar reports what was re-solved and how long it took (about 0.1 s on the 25 km line), and the Clearances tab checks again.

## Tools: hotbar and inventory

As in Minecraft, the bottom of the 3D view holds a **hotbar** of nine tool slots. Choosing a slot — **1–9**, a click on it, or the **wheel** while the mouse is captured in first person, one slot per wheel notch (a trackpad steps at a steady pace; Ctrl + wheel stays the speed) — only *holds* its tool; a **right-click** on an object uses it (in first person, on what is under the crosshair): Select, Move structure (M), Delete, Go to structure (G), Edit, Add structure, Add string, String wire (see [Editing](#editing)). A left click always selects; a right-*drag* still pans. A window opened by a tool (Move, an editor, the confirmation of a deletion, Go to) frees a captured mouse and opens at the crosshair (or at the mouse outside first person); it can be dragged anywhere, also partly off the screen. **E** opens the **inventory** with every tool (**E** or **Escape** closes it); only while it is open can the hotbar be arranged: drag a tool onto a slot, drag slots to swap them, drag a slot back to the inventory (or right-click the slot) to empty it. The arrangement is remembered. New tools are added to the inventory as they come.

## Clearances

The **Clearances** tab checks the shown weather case with a rule file (`.olsr`, the format of `ols line --rules`; see [Clearance rules](terrain.md#clearance-rules-olsr)): the wires against the ground surface, against the LiDAR points of the rules' classes, and against the line's other wires, with the requirements by voltage of each section's electrical system. Rule files are listed from `--rules FILE`, the `.olsr` files next to the model and `libraries/clearance-rules`; the one given or the only one found is chosen at start.

Checks run **on demand**: **Run checks** (Checks menu or quick bar) or **Check now** in the tab; tick "after every edit" to check again after each edit and change of weather case. The check runs in the background. LiDAR points are read again from the project's point cloud each time, only those of the rules' classes in a corridor around the spans, so nothing more is kept in memory: on the 25 km line with 18.8 million such points a check takes a few seconds. The list shows each violation, worst first: wire (span or jumper, and the other wire for wire rules), rule, vertical or radial clearance against the requirement, the LiDAR class, how many points are too close. Clicking one selects the wire and brings the views to it. The violations are drawn in red in the 3D view (a line from the wire to the point too close; layer "Clearance violations") and circled in the plan.

Rules for other lines and rules across pairs of weather cases are run by `ols line` only; the tab says so.

## Tables

**Tables** shows the line one object per row, in a floating window (each table a page):

| Table | Editable columns | Shown only |
|---|---|---|
| Structures | type, extension, orientation, E, N, z | strings, sections through it |
| Sections | cable, circuit, phase, system, bundle, spacing, value (tension in kN or sag in m, by its condition), temperature | status, condition, points, H and largest sag in the shown weather case |
| Strings | label, insulator point and wire point (across, along, up from the base, in the structure's axes), weight, insulator, strain (yes/no) | length, sections through it |
| Jumpers | cable, length or sag, temperature, bundle and spacing (empty: its section's) | from, to |
| Electrical systems | name, AC/DC, kV, Hz | phases, sections given it |
| Weather cases | name, description, tags (comma separated), temperature °C, ice thickness mm, ice load N/m, ice density kg/m³, wind Pa (empty: none) | order |

Type in a cell like in a spreadsheet: a changed cell turns yellow and stays **pending**; **Apply N changes** makes one command for all of them (one undo, ⌘Z), with the same checks as the editor windows — a cell that does not read (a word for a number, an unknown cable…) is named and nothing is applied; **Revert** drops them. A page with pending changes shows * on its tab and does not close. Click a column title to sort (again: descending, again: unsorted); the filter keeps rows with the text in any cell. Clicking a row's first cell selects the object and brings the views to it. The status, condition and points of a section are changed in its window (Edit tool), new objects with the tools.

Weather cases are not part of the project but the model's case file: **Apply** writes it (comments and layout kept, the values with their units), the model is solved again in the changed cases, and an open Weather cases library page reads it again. It is not undone with ⌘Z (edit the values back). Clicking a case's number shows it. Cases are added and removed in Libraries › Weather cases; without a case file (the default 15 °C case) make one there and choose **Use for the model**.

## Library editors

**Libraries** opens an editor for the wire library (`.olsw`), insulator assemblies (`.olsi`), structure types (`.olss`) or weather cases (`.olswc`) — first the files the model reads: the wire library given or next to the model, `Structures/` next to it, the insulator directory, the weather case file. Choose one of them, **Browse…** for a file anywhere on the computer, or **New file…** to create one (where you choose, from a template). For wires and weather cases the list on the left holds the items (**Add**, **Duplicate**, **Delete**).

**Use for the model** makes the model read the file open: the wire library or the weather case file itself; the folder of an insulator or structure type file. The model is solved again; a wire library without every cable the line uses is refused (the cables missing are named) and the model keeps its own.

Every field is shown as written in the file, with its units (`"28.1 mm"`, `"55 GPa"`, `"-5 °C"`); numbers and lists as TOML (`23e-6`, `["clearance", "loads"]`). **add a field…** offers the fields of that kind not yet given, with an example and their meaning; 🗑 removes one. A structure type's attachments, extensions and body lines are listed below its fields (add, duplicate, remove). Comments and layout of the file are kept.

The file is checked as you type with the library's own reader; the error is shown and **Save** waits until it reads. After saving, the model reads its libraries again, resolves the structure types and solves every weather case; a page with unsaved changes shows * and does not close. 

## Controls

The 3D view is navigated with the mouse and, while the pointer is over it, the keyboard — like Minecraft in creative mode:

| Input | Action |
|---|---|
| W / S | forward / back along the ground, in the direction of view |
| A / D | left / right |
| Space / Shift | up / down |
| Ctrl (held) | four times faster |
| scroll | zoom |
| right or middle drag | pan |
| double-click | fly to the object |
| G | go to a structure: a window with every structure in a list; Enter goes (selects it, brings the 3D view within 150 m and centres the plan), Escape closes |

Two schemes, chosen in the Model panel (Controls):

- **Orbit** — left drag turns the view around the point looked at.
- **Minecraft creative** — first person, as in the game: clicking the 3D view captures the mouse (locked and hidden); moving it looks around (mouse down: look down); a crosshair marks the centre and a click selects what is under it (double-click flies to it); **Ctrl + wheel** sets the speed (the wheel alone scrolls the hotbar). **Escape** frees the mouse to use the panels (switching to another application frees it too).

The speed (m/s) and, for the first-person scheme, the look sensitivity are set there too, and the list shows every key of the scheme. The scheme, speed and sensitivity are remembered for the next start; `--controls orbit` or `--controls minecraft` chooses one at start. Schemes are data (`controls.rs`: actions bound to keys and mouse buttons), so schemes familiar from other software can be added as presets, and later edited and saved.

**Edit › Snap: what can be clicked** chooses what can be clicked: structures, spans, jumpers, attachment points, alignment points, LiDAR points (more kinds will be added); by default structures, spans and jumpers. At about the same distance, a point wins over the line it sits on and a structure over a wire. The choice is remembered.

Clicking an object in any view selects it everywhere: a structure anywhere along the lines it is drawn with (mast or stick drawing, arms, insulator strings), a span anywhere along its wire, a jumper anywhere along it (drawn as a bundle like its section). The properties of a jumper show its cable, section, bundle (and whether it is its own or its section's), length or sag, support string, and results in every weather case. Tabs can be dragged to rearrange the window.

Loading runs in the background. On a real 25 km line with 99.6 million LiDAR points stored in its project, the window is ready in about 8 s (release build): 84 million points decoded in the corridor, 1.5 million shown, a 5-million-triangle ground surface.
