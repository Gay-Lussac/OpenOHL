# Desktop application

`ols-cad` opens a line file or a project and shows it in dockable views that share the selection and the weather case.

```bash
cargo run --release -p ols-app -- examples/synthetic-line/synthetic-line.ols.json \
    --terrain examples/synthetic-line/synthetic-survey.csv \
    --weather examples/synthetic-line/weather-cases.olswc

cargo run --release -p ols-app -- project.ols          # with the project's point cloud
cargo run --release -p ols-app                         # the start page: File › New project, Open
```

On macOS, double-click **Start OLS CAD.command** at the top of the source tree: it builds and starts the app on its start page (new project, open, the files opened recently). Libraries given on the command line apply to the file given there only; files opened from the File menu use their own (a project's are kept in it).

| Option | Meaning |
|---|---|
| `--library` | Wire library (file or directory); default: the `*.olsw` files next to the model, else `libraries/cables`. |
| `--structures` | Pylon files; default: the `Structures/` folder next to the model. |
| `--weather` | Weather cases (`.olswc`); default: still air at 15 °C. All cases are solved. |
| `--terrain` | Survey points for the ground. |
| `--pointcloud` | Point cloud in a project (`FILE.ols` or `FILE.ols#name`); default: the project's own. |
| `--points` | About how many LiDAR points are shown (default 2 000 000). Points of **every class** of the whole cloud are shown, ground classes and noise included: a class of up to 20 000 points whole, the larger classes thinned alike to the budget. The ground surface is built from all points of the ground classes (lowest per 1 m cell) in the corridor. |
| `--corridor` | Half width of the corridor around the spans the ground surface is built in (default 60 m). |
| `--controls` | Navigation scheme: `orbit` or `minecraft` (default: the one used last). |
| `--screenshot FILE.png` (`--focus ID`, `--distance M`) | Load, render the 3D view to an image and exit (with `--focus`, looking at that structure's first attachment). |

## Menus, quick bar and pages

The **menu bar** holds what needs no view:

| Menu | |
|---|---|
| File | **New project…** (⌘N / Ctrl+N: a line name and coordinate system, then where to save the `.ols` file; it starts with an empty line, you its admin, its libraries from the default folders), **Open…** (⌘O / Ctrl+O: a project `.ols` or a line file `.ols.json`), **Open recent**, **Join a server…**, the file open, Save (⌘S / Ctrl+S), **Import into the line…** and **Export…** (the built-in formats and those of the plugins turned on — see [Import and export](import-export.md); an import is one command, undoable), Export libraries, **Host this project…** / **Leave the server** / **Stop hosting** (see [Server mode](server.md)), **Close** (the project is written through; back to the start page), Quit. Opening or closing with library pages not saved or table changes not applied asks first |
| Edit | Undo, Redo, Delete selected, Clear selection, Go to structure, **History…** (the operation log; admins reverse entries); **Snap: what can be clicked** |
| View | Fit views; the pages to show (tick to open, untick to close), among them **Level of detail** (see [Level of detail](#level-of-detail)) and **People** (who works on the project, see [Server mode](server.md)); layers; Display weather cases; Customize the quick bar |
| Lines | Project lines (saved states of the line to show next to the working line, or to make the working line), Create line, Delete line (see [Project lines](#project-lines)) |
| Tools | the tool inventory (E); hold any tool |
| Checks | calculations run **on demand** — they never change the model: **Compute structure loads** (the loads at every attachment in each load case, see [Structure loads](loads.md)); Clearances (Run checks, also after every edit, rule file, Show results) |
| Terrain | (What you show — the ground surface, others shown, LiDAR classes hidden, the share of points — is yours: remembered on this computer for each project, never written to the project nor sent to a server.) **Terrain surfaces (TIN)…** (the surfaces kept in the project: import, build from LiDAR, the active one = the ground, others shown in their colour, export, delete — see [Terrain surfaces](terrain.md#terrain-surfaces-in-the-project)); the ground and the other surfaces shown or not; **Survey data**: **Import LAS/LAZ…** (files or a folder into the project's point cloud or a new one — coordinate system, chunk size, class scheme — in the background with a progress bar; the project is opened again to show them) and **Export LAS/LAZ…** (every class or the classes shown, the whole survey or around the line; LAS 1.4 / LAZ with the class scheme next to it; see [Point clouds](project.md#point-clouds)); LiDAR points shown or not, and **Survey data display options…** — a window that stays open: every LiDAR class of the point cloud with its points, in a list that scrolls; a click shows or hides a class, **press and drag** up or down to give every class passed the same state; All, None, Invert, by kind (ground, vegetation, buildings, wires and structures, traffic, noise), a filter by name or role; **Points shown near the camera**, 0.1–100 % of the survey (default 25 %) — farther away the Level of detail table's shares and render distance reduce them; changes wait for **Apply** (the points are read again once; **Revert** goes back to what is shown, closing the window drops what is not applied); a hidden class is neither drawn nor clicked; noise is drawn magenta |
| Structures | libraries: structure types, insulators (see [Library editors](#library-editors)); tables: structures, strings, alignment points (see [Tables](#tables)) |
| Sections | library: wires; tables: sections, jumpers, electrical systems; Display weather cases |
| Criteria | **Creep-Stretch…** — the weather case for the final after creep and the weather case(s) for the final after load (the one stretching each wire most is used); one command, undoable; every section is solved again. **Weather cases** — every case in one table, one row each, the columns of a criteria table (name above, unit below, a grid; every row the same widths): name, description, tags, air density factor Q, wind velocity, wind pressure (P = Q·V²: typing the velocity or Q sets the pressure), wind from, ice thickness, density and load, wire and ambient temperature; the weather case files' editor; **Load cases** — the load case files' editor (see [Load cases](loads.md)); Display weather cases |
| Parameters | **Controls** › **3D** (navigation scheme, speed, look sensitivity, every key and mouse action), **Plan**, **Profile** (their mouse actions) |
| Plugins | **Plugin manager…** (see [Plugins](#plugins)), **Python console** (see [Python console](#python-console)) |
| Help | version, keys and mouse |

**Weather cases are solved when needed:** opening a model solves only the weather case shown first. Another case is solved in the background the first time it is shown (Display weather cases, a section's own display case); the status bar says which are being solved. A check or a script that needs a case solves it first. Edits re-solve the cases already solved; the others are solved from the line as it is when they are first shown. The Model page lists the cases not solved yet.

Under it, the **quick bar**: the line's name, **Weather: …** (the weather cases shown — a click opens Display weather cases), the selection, then the commands chosen for it (default Undo, Redo, Fit views, Run checks). **⚙** (or View › Customize the quick bar) adds any command of the menus — commands, library editors, tools — removes and orders them; the bar is remembered.

**Pages** work like browser tabs: each has a tab with a close button; drag a tab to rearrange, or out of the window to make a floating, resizable window with its own row of tabs (drop more pages on it). Closed pages come back from View. Library editors open together in a floating window.

## Views

- **3D** — ground (shaded), LiDAR points coloured by class role (vegetation green, buildings red, water blue, wires yellow…), structures, insulator strings, wires in their phase colours (bundles as their subconductors, `bundle_spacing` apart — one section, solved and selected as one — held at every clamp by a yoke: a bar for two, a triangle for three, a polygon for more; the back and ahead spans and the jumpers end on its corners), alignments. Navigation: see [Controls](#controls). Height exaggeration in the Model panel.
- **Plan** — alignments, wires and structures from above, north up, with a scale bar. Drag, scroll at the pointer, double-click to centre.
- **Profile** — per route: ground along the centreline, structures and wires by station; heights fit the visible stretch. Scroll zooms along the route, drag moves, double-click shows the whole route.
- **Properties** — the selected structure (position, type, attachments, spans through it) or span (cable, circuit, electrical system and voltage, bundle, horizontal and maximum tension, sag and lowest point in every weather case). Links select related objects.
- **Model** — layers, sections or jumpers that failed to solve, by weather case (click one: that case is shown, the object selected and the views brought to it), what was loaded and how long it took.
- **Clearances** — clearance violations of the shown weather case (see [Clearances](#clearances)).
- **Properties** of the other kinds: an attachment point (string type, insulator and wire points, length, weight, spans and jumpers there), an alignment point (position, tied structure), a LiDAR point (position, class and roles).

## Level of detail

The 3D view draws the large layers — terrain, LiDAR points, structures, wires, other weather cases, other project lines — in **chunks**: the 256 m tiles of the project grid (the grid of the project file's spatial index). Every frame, each chunk in view is drawn at a **level of detail** from its distance to the eye; chunks outside the view or beyond the layer's render distance are not drawn, and those far beyond (1.5 × the render distance) leave the GPU. Missing chunks are built nearest first, a few milliseconds per frame, so the view never freezes: until a chunk's level is built, a level already built is drawn. An edit rebuilds the layers of the line, not the terrain and the LiDAR points.

**View › Level of detail** (a page) is the table of these settings, a row per layer, every value typed in, remembered with the window layout:

| Layer | Level 0 | Level 1 (default from) | Level 2 (default from) | Render distance |
|---|---|---|---|---|
| Terrain | every TIN triangle | the surface on an 8 m grid (1.5 km) | a 32 m grid (5 km) | 30 km |
| LiDAR points | the share of the survey chosen in Survey data display options (default 25 %; 100 %: every point) | 25 % of that (500 m) | 5 % of that (1.5 km) | 3 km |
| Structures | lattice, arms, strings | mast, arms and strings (2 km) | the mast line (8 km) | 50 km |
| Wires, other cases, project lines | every point; bundles as subconductors with their yokes | 16 points per span, one line per bundle (1.5 km) | 5 points per span (6 km) | 50 km |

A survey stored in the project is read by tile as tiles come in view, on a background thread straight from the project file (every class; classes of up to 20 000 points whole): on a 25 km line seen from 600 m, 3.2 million points at 100 % appear in 0.2 s without a frame over 11 ms. The last column of the table shows what was drawn in the last frame: the chunks at each level, of how many, and how many are still loading. On a 25 km line seen from 1.5 km, 5 million terrain triangles became 280 000 drawn, 1.5 million LiDAR points 24 000, and 190 000 wire segments 14 000; every chunk builds in under 2 ms.

The level of detail changes what is **drawn** only: calculations, checks, the profile, the plan and picking always use the full model.

## Weather cases shown

**Display weather cases** (the **Weather: …** button of the quick bar, or View) lists the model's weather cases: tick one or several and **Apply to all sections** — every section is drawn in each case ticked (sections' own display cases from the Sections table go), with its jumpers, yokes and **insulator strings, drawn to their clamp as solved in each case** (swung by the wind, moved with the temperature). The first case ticked is the one reported in the properties, clicked, and checked for clearances. A single section can still be given its own display cases in the Sections table.

The model's **weather case file** is remembered with the project (`--weather`, or Libraries › Weather cases › Use for the model): opened again without `--weather`, the project uses the file it used last.

## Project lines

A project keeps, next to the **working line** (the one edited), **project lines**: frozen states of the line — e.g. *as built* before the works and *as planned* — to compare later, or to draw the actual and the planned state.

- **Lines › Create line…** saves the working line as it is now, with a name and a note (kept in the project file; not undone with ⌘Z).
- **Lines › Project lines** lists them: tick one or several to **show** them next to the working line in the 3D view (layer "Other project lines") and the plan, each in its colour and drawn as the working line is: structures, insulator strings to their solved clamps, bundles as their subconductors with their yokes, jumpers — every section in the same display weather cases as in the working line. **make it the working line…** replaces the working line by it — one command, so ⌘Z brings the working line back (save it first with Create line to keep it).
- **Lines › Delete line** deletes a project line (the working line is not touched).

## Saving

Every edit is stored in the project file as it is made (the operation log), so nothing is lost when the app stops. **⌘S** (Ctrl+S on Windows and Linux, or File › Save) writes the project file through, saves the library pages with changes that read (the model reads them again) and says which table changes are still pending (Apply makes them). **⌘Z** / **Shift+⌘Z** (Ctrl+Z / Shift+Ctrl+Z) undo and redo, outside text fields.

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
| Add a jumper | The **Add jumper** tool: right-click a structure — the jumpers missing there are added, between two section ends (first or last points) of the same circuit and phase that no jumper joins yet (sag 15 % of the distance, at 15 °C; ends that cannot be paired are named); or right-click two strings — a jumper between them (the same string again cancels). Edit on a jumper opens its row in the Jumpers table. |
| Undo / redo | **⌘Z** / **Shift+⌘Z** (Ctrl on Windows and Linux), the Edit menu or the quick bar. |

Every window applies one command (**Apply** or Enter; **Cancel** or Escape); when the session refuses it (an id taken, a line that would not be valid…), the window stays open with the reason. While Delete, Edit, Add string or String wire is held, strings can be right-clicked whatever Edit › Snap says.

After an edit only the sections it touched are re-solved, in every weather case (incremental solving); the status bar reports what was re-solved and how long it took (about 0.1 s on the 25 km line), and the Clearances tab checks again.

## Tools: hotbar and inventory

As in Minecraft, the bottom of the 3D view holds a **hotbar** (it goes with the 3D view when another page is in front; the inventory shows it to arrange it) of nine tool slots. Choosing a slot — **1–9**, a click on it, or the **wheel** while the mouse is captured in first person, one slot per wheel notch (a trackpad steps at a steady pace; Ctrl + wheel stays the speed) — only *holds* its tool; a **right-click** on an object uses it (in first person, on what is under the crosshair): Select, Move structure (M), Delete, Go to structure (G), Edit, Add structure, Add string, String wire, Add alignment point, Add jumper (see [Editing](#editing) and [Alignments](#alignments)). A left click always selects; a right-*drag* still pans. A window opened by a tool (Move, an editor, the confirmation of a deletion, Go to) frees a captured mouse and opens at the crosshair (or at the mouse outside first person); it can be dragged anywhere, also partly off the screen. **E** opens the **inventory** with every tool (**E** or **Escape** closes it); only while it is open can the hotbar be arranged: drag a tool onto a slot, drag slots to swap them, drag a slot back to the inventory (or right-click the slot) to empty it. The arrangement is remembered. New tools are added to the inventory as they come.

## Clearances

The **Clearances** tab checks the shown weather case with a rule file (`.olsr`, the format of `ols line --rules`; see [Clearance rules](terrain.md#clearance-rules-olsr)): the wires against the ground surface, against the LiDAR points of the rules' classes, and against the line's other wires, with the requirements by voltage of each section's electrical system. Rule files are listed from `--rules FILE`, the `.olsr` files next to the model and `libraries/clearance-rules`; the one given or the only one found is chosen at start.

Checks run **on demand**: **Run checks** (Checks menu or quick bar) or **Check now** in the tab; tick "after every edit" to check again after each edit and change of weather case. The check runs in the background. LiDAR points are read again from the project's point cloud each time, only those of the rules' classes in a corridor around the spans, so nothing more is kept in memory: on the 25 km line with 18.8 million such points a check takes a few seconds. The list shows each violation, worst first: wire (span or jumper, and the other wire for wire rules), rule, vertical or radial clearance against the requirement, the LiDAR class, how many points are too close. Clicking one selects the wire and brings the views to it. The violations are drawn in red in the 3D view (a line from the wire to the point too close; layer "Clearance violations") and circled in the plan.

Rules for other lines and rules across pairs of weather cases are run by `ols line` only; the tab says so.

## Tables

**Tables** shows the line one object per row (headers with the unit below the name, a grid, every row the same column widths), in a floating window (each table a page): structures, sections, strings, jumpers, electrical systems, alignment points, weather cases. **Every value shown can be typed in**, except counts derived from other rows (shown dimmed) and what a string or an alignment point belongs to (typed in new rows). Where an object leaves a value to its **default**, the value used is shown, dimmed (hover: "default"): a jumper's bundle and spacing (its section's), bending stiffness (estimated from its cable) and clamp exit angles (30°); a section's electrical system (from its conductor path; "earthed") and the weather cases it is drawn in (those shown); a strain string's weight estimated from its position (strings given without a weight). Applying other changes leaves defaults unset; typing over one sets it on the object:

| Table | Columns |
|---|---|
| Structures | id, type, extension, orientation, E, N, z · *strings, sections* |
| Sections | id, cable, circuit, phase, system, bundle, spacing, status (in the blocks / clipped in), condition (tension · sag, ruling span · sag, span N), value (kN or m), temperature, points (`T1 1:1, T2 1:1, …`), display cases, H kN, max sag m, **calculation** (Initial, Creep, Load, Max Sag × RS, FE — also in the section's Edit window; see [Calculations](line-file.md#calculations-condition-and-method)) |
| Strings | structure (new rows), set:phase, label, insulator point and wire point (across, along, up from the base, in the structure's axes), length, weight, insulator, strain · *sections* |
| Jumpers | id, from, to (`T3 2:1`), cable, length or sag, temperature, bundle and spacing (empty: its section's), bending stiffness EI (empty: estimated from the cable; 0: hinged ends), the angles at which it leaves its clamps at from and to, below the conductor (empty: 30°) — see [Jumpers](line-file.md#jumpers) |
| Electrical systems | id, name, AC/DC, kV, Hz, phases (`L1 L2 L3`; angles kept by position) · *sections* |
| Alignment points | route (new rows), #, E, N, z, structure (tied under it, following it; empty: free) |
| Weather cases | #, name, description, tags, air density factor (Pa/(m/s)², empty: 0.613), wind velocity (m/s; typed in, the pressure becomes Q·V²), wind pressure (Pa), wind from (left or right, looking ahead along the line), ice thickness (mm), ice density (kg/m³), ice load (N/m), wire temperature, ambient temperature (°C) — Criteria › Weather cases |

- Cells with a known set of values are **dropdowns**: cables, structure types, systems, insulators, status, condition, AC/DC, strain, wind from, tied structure; **display cases** lists the model's weather cases to tick, several at once.
- A changed cell turns yellow and stays **pending**; **+ Add row** adds an object typed in a green row (🗑 removes it); 🗑 on a row marks it for deletion (struck through; click again to keep it). **Apply N changes** makes one command for all of them (one undo, ⌘Z), with the same checks as the editor windows — a cell that does not read is named and nothing is applied; **Revert** drops them. When deletions take other objects with them (a structure's sections and jumpers, a string's sections…), they are listed first and **Apply and delete all that** confirms.
- **Renaming** a structure, section, jumper or system, or renumbering a string, keeps its place, and what refers to it follows (section points, jumper ends, routes, tied alignment points, sections' system).
- Sections: changing the **condition** needs its value; **status** clipped in computes the lengths of its condition (as the section window does); **points** restring it. **Display cases** draws the section in each weather case listed, separated by `;` (e.g. `max temperature; glaze ice`), in every view; the first one is the one picked and reported in the properties (empty: the case shown) — sections can be shown each in their own cases. **H kN** and **max sag m** give one value per display case (`12.31 / 9.87`); typing one number is a **target** in the first display case: the section's condition (tension or sag) is found that gives it (a section clipped in must go back to the blocks for that).
- **#** orders alignment points and weather cases: `3.5` goes between 3 and 4.
- Weather cases are not part of the project but the model's case file: **Apply** writes it (comments and layout kept, values with their units) and the model is solved again; not undone with ⌘Z. Without a case file (the default 15 °C case) make one in Libraries › Weather cases and choose **Use for the model**.
- Click a column title to sort (again: descending, again: unsorted); the filter keeps rows with the text in any cell. **⤴** selects the row's object and brings the views to it (a weather case: shows it). A page with pending changes shows * on its tab and does not close.

## Alignments

The profile view follows each route's **alignment points** (the line's own alignment when it has no routes); a route without points follows its structures. The points are drawn in the 3D view (cyan, on the ground) and the plan, and can be clicked (Edit › Snap, alignment points). **Add alignment point** (tool) and a right-click on the ground inserts one into the nearest segment of the nearest route — a route following its structures first gets points under them; **Edit** opens its window (E, N, z or on the ground, tied to a structure); **Delete** removes it; the **Alignment points** table holds them all. The profile follows at once.

## Library editors

**A project keeps its libraries.** The wire library, the structure types, the insulator assemblies and the weather case files are stored in the project file (`.ols`), so the project alone is everything needed to open it elsewhere. Opened the first time, each kind the project does not hold yet is copied into it from where it was read (the files given, next to the project, or the default folders); a library given on the command line (`--library`, `--structures`, `--insulators`; `--weather` adds its file) replaces the project's. Every save of a library page or of the weather case table goes into the project. **File › Export libraries…** writes them all as files into a folder (a folder per kind), and **Export…** on a page writes the file open: to share them, or to compare them with other data. A user who may not change the project (a viewer) reads the libraries from their files as before. (The model reads working copies in a temporary folder; the project is what is kept.)

The **Structures** and **Sections** menus open an editor for the wire library (`.olsw`), insulator assemblies (`.olsi`), structure types (`.olss`) or weather cases (`.olswc`) — first the files the model reads: the wire library given or next to the model, `Structures/` next to it, the insulator directory, the weather case file. Choose one of them, **Browse…** for a file anywhere on the computer (in a project: copied into the project), or **New file…** to create one from a template (in a project: in the project, under the name chosen). For wires and weather cases the list on the left holds the items (**Add**, **Duplicate**, **Delete**).

**Use for the model** makes the model read the file open: the wire library or the weather case file itself; the folder of an insulator or structure type file. The model is solved again; a wire library without every cable the line uses is refused (the cables missing are named) and the model keeps its own.

A cable starts with its **model** — a list of the three cable models (non-linear; linear with creep proportional to the creep case tension; linear with a temperature increase) — and the **Bimetallic conductor** tick (a core with its own data; unticked, the core goes): choosing a model adds the fields it needs (the outer and core curves with their creep curves, or the creep temperature shift, and its tension) with example values, fields it does not use are hidden (kept in the file), and the fields a cable or its model needs cannot be removed (their 🗑 is greyed). A bimetallic cable has its own behaviour above the transition temperature, below the tick: its outer strands go into compression up to its maximum virtual compressive stress, or take none (bird cage). Sections written as `[cable.outer]` tables are edited like inline ones.

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

Two schemes, chosen in **Parameters › Controls › 3D** (with the speed, the look sensitivity and every key and mouse action; the 3D view shows no help text over it, only "Esc: free the mouse" in first person). **Parameters › Controls › Plan** and **Profile** list how those views are moved and used (more settings will come there):

- **Orbit** — left drag turns the view around the point looked at.
- **Minecraft creative** — first person, as in the game: clicking the 3D view captures the mouse (locked and hidden); moving it looks around (mouse down: look down); a crosshair marks the centre and a click selects what is under it (double-click flies to it); **Ctrl + wheel** sets the speed (the wheel alone scrolls the hotbar). **Escape** frees the mouse to use the panels (switching to another application frees it too).

The speed (m/s) and, for the first-person scheme, the look sensitivity are set there too, and the list shows every key of the scheme. The scheme, speed and sensitivity are remembered for the next start; `--controls orbit` or `--controls minecraft` chooses one at start. Schemes are data (`controls.rs`: actions bound to keys and mouse buttons), so schemes familiar from other software can be added as presets, and later edited and saved.

**Edit › Snap: what can be clicked** chooses what can be clicked: structures, spans, jumpers, attachment points, alignment points, LiDAR points (more kinds will be added); by default structures, spans and jumpers. At about the same distance, a point wins over the line it sits on and a structure over a wire. The choice is remembered.

Clicking an object in any view selects it everywhere: a structure anywhere along the lines it is drawn with (mast or stick drawing, arms, insulator strings), a span anywhere along its wire, a jumper anywhere along it (drawn as a bundle like its section). The properties of a jumper show its cable, section, bundle (and whether it is its own or its section's), length or sag, support string, and results in every weather case. Tabs can be dragged to rearrange the window.

Loading runs in the background. On a real 25 km line with 99.6 million LiDAR points stored in its project, the window is ready in about 8 s (release build): the whole cloud decoded (every one of its 31 classes shown, 2.1 million points), a 5-million-triangle ground surface.

## Plugins

Plugins › **Plugin manager…** lists the plugins found: those installed for you (in the application's data folder, `plugins/`) and those of the folders you added (used where they are — handy while writing one). For each: its version, kind (WebAssembly or Python), what it provides (a file-format plugin's formats), and why it cannot be used if it cannot (another interface version, permissions asked for that cannot be granted, its entry file missing — e.g. not built). The tick turns a plugin on or off (remembered). **Install from a folder…** copies a plugin into your plugins (a WebAssembly plugin's manifest and `.wasm`; a Python plugin's folder); **Remove** deletes an installed copy (after a confirmation). A plugin name found twice is used from the first place (your plugins, then the folders in order).

File-format plugins add their formats to File › Import into the line / Export, next to the built-in ones. They run on a worker thread, sandboxed, with time and memory limits ([Writing plugins](../development/plugins.md)).

## Python console

Plugins › **Python console** opens a page with a Python interpreter in the application (Python 3.10+, the one OLS CAD was built with). Type code and press Enter (Shift+Enter: a new line; ↑ ↓: history); the value of a last expression is printed. **Run a file…** runs a script as `__main__`; **Plugin scripts** lists the Python plugins turned on. **Stop** interrupts a script (`KeyboardInterrupt`); a script running longer than the **time limit** (default 300 s) is stopped.

The `ols` module drives the open project:

| Call | |
|---|---|
| `ols.line()` | the working line, as in a line file (a dict) |
| `ols.cases()`, `ols.case()` | the weather cases, the one shown |
| `ols.results(case=None)` | the solved sections of a case: per span sag, horizontal and largest tension, mid and low points; what failed |
| `ols.selection()` | the selection, in words |
| `ols.apply(command, what=None)` | a session command (as in the operation log, e.g. `{"op": "move_structure", "id": "T2", "position": [x, y, z]}`): validated, logged, undoable; what it touches is solved again. A refused command raises |
| `ols.batch(commands, what=None)` | several commands as one (all or none, one undo) |
| `ols.undo()`, `ols.redo()` | your last command |
| `ols.status(text)` | text in the status bar |
| `ols.convert(kind, value, unit)` | unit conversion, as in the `ols` package |

```python
import ols
for s in ols.results("everyday")["sections"]:
    print(s["section"], max(p["sag"] for p in s["spans"]))
```

Each call is answered by the application at its next frame, so a script always sees the line as it is now; many edits go faster (and undo at once) in one `ols.batch`. A read-only project refuses edits.

