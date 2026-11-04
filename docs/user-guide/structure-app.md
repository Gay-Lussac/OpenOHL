# OHL-Structure

**OHL-Structure** (the program `ohl-structure`), the structure
application of OpenOHL, edits one structure file (`.ohls`) in a window of its
own, from level 1 (a stick figure: the points where strings hang) to
level 3 (a full design: every member with its cross-section, the supports).
See [Building structures](../development/structure-building.md) for
what the levels mean and what comes next.

```bash
ohl-structure examples/synthetic-line/Structures/type-02.ohls
```

Options:

| Option | What |
|---|---|
| `--insulators DIR` | the insulator assemblies strings may take; default: the file's folder and an `Insulators` folder there |
| `--ahead DEG` | which plan angle means ahead along the line, for the compass; default 0 |
| `--sections FILE\|DIR` | the [cross-section library](section-library.md) (a `.ohlsec` file or a folder of them) the members' areas come from; default: the `Sections` folder beside the file |
| `--structure ID` | the placed structure whose loads these are: values typed over the load table are kept for it (below) |
| `--line-values FILE` | the line's values at that placed structure (a line-values file, written by the line application); read again when it changes |
| `--version` | prints the version and the build id — the git commit it was built from, e.g. `ohl-structure 0.0.1 (build 1a2b3c4d5e6f)` — and stops |

## Starting it

On its own: double-click **Start OHL-Structure.command** in the OpenOHL
folder (macOS; it builds the program and starts it), or

```bash
cargo run --release -p ohl-structure-app --bin ohl-structure
```

Without a file it opens on its **start page** (the OpenOHL mark above
the name; the window has OHL-Structure's icon): **Open a structure…**,
**New structure…**, **Open the example** and **Plugins…** (the [plugin
manager](#plugins)), then the **Recent files** (the last opened first,
each with its folder; a click opens one; kept between runs). A file
that is not there any more is marked *(missing)* and cannot be opened;
the **×** beside a file takes it off the list (the file itself stays).
**Open the example** opens a copy of the example structure (the
example line's lattice tower `type-02`, invented values, level 3) from a
temporary folder: change it freely, the example stays as it is; **File ›
Save as…** keeps it where you choose (the copy is not added to the
recent files). **New
structure…** (also in the File menu) asks an id and a name and what to
start from: **Empty** — one point where a string hangs, a level 1
stick figure to build from — or a **structure template** (`.ohlpt`: the
templates shipped with OpenOHL, or **Another template file (.ohlpt)…**), built
for suspension or strain with the template's default dimensions (the
line application's structure builder sets others). **Create…** asks where
to write the file (`<id>.ohls`; an existing file is not replaced) and
opens it.

From the line application, **Open in OHL-Structure…** on a structure (in its properties)
and **Open this type in OHL-Structure…** (on the [structure
screen](desktop-app.md#the-structure-screen), the Structure library page
and the Structures table) start it on the structure's file; saved here,
the line follows. From a placed structure's properties it also hands
over that place's loads ([below](#started-for-a-placed-structure)). A
structure without a type has no file: the line application asks for a
type id first (**Make it a structure file and edit it…**).

The line application only edits a structure's basics (attachment
points, strings, wire points and sets); its body — joints, members,
cross-sections, materials, supports, generators, the pole, extensions — and
its design are edited here.

Build it next to the line application, from the same code (in the
OpenOHL folder; **Start OHL-Design.command** does this):

```bash
cargo build --release -p ohl-app -p ohl-structure-app
```

The line application looks for `ohl-structure` in the program the
`OHL_STRUCTURE_APP` environment variable names, then next to
`ohl-design`, then on the `PATH`. Before it starts it, it runs `ohl-structure
--version`: when the build differs from its own it starts it anyway and
shows a warning (rebuild both). Not found, or stopped with an error right
away: a window says why — where it looked and how to build it, or what
the application wrote to its error output. The line application's log
(info level) records each start, close and failure.

## Started for a placed structure

**Open in OHL-Structure…** in a placed structure's properties (say T2, of type
`type-02`) starts the application with T2's loads:

```bash
ohl-structure Structures/type-02.ohls --structure T2 \
    --line-values Structures/type-02.T2.line-values.json --sections Sections
```

- The header says **loads of T2 in *line*** (a link to the
  **Line loads** panel).
- The line application computes the loads the wires and strings put on
  T2 in the cases it sends to its structures (Criteria › Loads for
  structures, see [Loads for structures](desktop-app.md#loads-for-structures)):
  its load cases (Criteria › Load cases or Design code; all by default),
  then the weather cases ticked to be sent on their own, each a case
  `weather: <name>` with every factor 1 (none by default). They are
  computed as Checks › Structure loads does, in the background, and
  written beside the structure file as
  `<name>.<structure>.line-values.json` (version 3: each case with its
  source and, per case, the sections and jumpers at the structure that
  did not solve; versions 1 and 2 still read — their cases load cases,
  nothing missing). Until
  they are there the header says they are being computed; with no load
  case and no weather case ticked it says so and suggests ticking weather
  cases; a ticked case no longer in the line is named. When the line changes while the application is open
  (and stays unchanged for a second), the line application computes them
  again and rewrites the file; the application reads it again within
  half a second and evaluates the recipes with the new values.
- A **status block** at the top of the Load case editor, Load table and Line
  values panels says, in plain words, what the line gave or why not:
  *The line gives the forces at 2 point(s) of T2 in 3 case(s): …*; *The
  line's values for T2 are being computed…*; *The line gives no values
  for T7: the project has no load cases. Add them in the line
  application (Criteria › Load cases, or a design-code plugin)…*; a case
  of the line without values (its sections not solved in it); a case
  **incomplete** — *Case ice incomplete: cross-section S12 not solved (why) —
  the line's forces at T2 lack its wires…*: a section or jumper at this
  structure did not solve in that case, so its forces are missing and the
  values too low (that case's rows in the load table start their note
  with *incomplete: S12 not solved*, Line loads marks them in its
  **complete** column, and **FE results** of the case read *not checked —
  incomplete: S12 not solved*, the usage of the loads there are shown
  only in brackets); opened
  alone, one short line *No line values (opened on its own)* — the
  whole sentence (*open this from a placed structure, select it in the
  line, Properties › Open in OHL-Structure…, to get its forces*) on
  hover; its **×** hides it until the next start —, with the cases
  stored in the file from the line, if any. **Store in the structure
  file…** stays, greyed, and says why on hover.
- The **load table** starts with the **line's forces**, without any
  recipe: every case the line transmits × every attachment point, the
  transverse, longitudinal and vertical force as the line computed them
  (factored where the line's case has factors; the unfactored ones, the
  sets and the phases in the note), marked `line` (column **from**: *line
  load case*). The structure's recipes follow as cases of their own (they
  may still take line values); a line case with the name of a recipe is
  called *NAME (line)*. **FE results** can run the line's cases directly.
- **Store in the structure file…** (by the load table, or **Loads ›
  Store the line's cases…**): a tick per line case; **Store**
  writes the cases ticked into the structure file as `[[load_case]]`
  tables of given values — a `value` and `direction` per point and
  component, `line_case`, and a description saying the line, the place
  and the date — in one undo step. The file then has these forces
  without the line (exports, design elsewhere). Stored again, cases of
  the same names are replaced (asked first: **Replace and store**).
- **Take the line's load cases** (Loads menu; *as they are* in the
  Load case editor while the structure has no case) writes instead one
  load case per line result: its line forces at every point, every
  factor 1, and (a structure with members) the wind on the body at the
  pressure of its weather case from its side: the forces then follow the
  line.
- **Line loads** lists, with each case's **source** (load case, or a
  weather case sent on its own: factored = unfactored), the line's values per point (with its sets and
  phases) and load case: transverse, longitudinal, vertical, unfactored
  and factored by the line's load case, in kN. **Other places of this
  structure** lists the other placed structures of the same type with
  the governing (largest) factored value per point and direction and its
  case — for comparison only: each place keeps its own loads.
- **Typed values** are kept for T2, not for the type: **Save** writes
  them as `<name>.T2.loads.csv` (for `type-02.ohls`:
  `type-02.T2.loads.csv`). In a project, the line application keeps that
  file in the project with T2 (project library kind `loads`, per project
  line), so it is there the next time T2 is opened. Opened alone, the
  structure keeps using `<name>.loads.csv`.

The structure file itself is the type's: saved here, every structure of
the type follows in the line.

## The window

At the top, the **menus** (2026-11-03: aligned with OHL-Design's; built,
in active development):

| Menu | Entries |
|---|---|
| **File** | **New structure…** (⌘N), **Open…** (⌘O), **Open recent ›** (a file no longer there greyed out) · **Save** (⌘S), **Save as…** (Shift+⌘S) · **Import ›** (per library kind — *Cross-section libraries*, *Bolt libraries*, *Insulator assemblies*, *Poles*, *Structure models*, *Load-case recipes*, *Structure templates* — each *A file…* or *The files of a folder…*; **Load recipes…**, **Load table…**; [Libraries](#libraries)) · **Export ›** (the file formats…, [below](#export); **Libraries it uses…**, **Every library…**; **Load recipes…**, **Load table…**, **Structure report…**; **Export options…**: the cross-section library, bolt library and load table the formats take) · **Plugins ›** (**Plugin manager…**, [below](#plugins)) · **Close** (the structure: the start page again; greyed out with unsaved changes), **Quit** (⌘Q; asks first with unsaved changes) |
| **Edit** | **Undo** (⌘Z), **Redo** (Shift+⌘Z), each naming what it undoes · **Delete selected…** (Delete or Backspace: everything selected, with what goes with each, asked first; one undo step), **Clear selection** · **Tools ›** (**Tool inventory** (E), then every tool of the 3D view by its group — *Select and edit*, *Geometry*, *Body* —, its hotbar key 1–9 shown, the one held marked; the menu stays open until a tool is chosen) |
| **View** | **Fit the view** · the main panels ticked (**3D view**, **Detail view**, **Inspector**, **To do**, **Libraries**) · **Tables ›** (every other panel, ticked, under a grey heading per menu; one of another structure kind greyed out, why on hover) · **Display options ›** (**Rigid links**, **Joints by their details**, **Volumes on the strings**, **Load arrows**, **Deformed**, **Reactions**, **Usage legend**) · **UI scale ›**, **Reset layout** |
| **Geometry** | **Structure kind ›** · its panels · **Add a point**, **One string below the point**, **Two strings to one wire point** |
| **Body** | its panels (Pole not for a tower without a pole) · **Add a joint**, **Add a member**, **Point on the joint** · **Symmetry ›** (none, or a mirror; **Make symmetric**) |
| **Design** | its panels (Joint and Member details not for a pole, Pole design not for a tower without a pole) · **Support the feet (pinned)** |
| **Loads** | **Load case editor**, **Load table**, **Line loads** · **Add a load case**, **Take the line's load cases**, **Store the line's cases…**, **Load cases from a design code…** ([Plugins](#plugins)), **Read the line's values again** |
| **Results** | **FE results** · **Run the cases**, **Cancel the run** · **Structure report…** |
| **Help** | **User guide…** (the manual on this computer, as in OHL-Design — the book built by `mdbook build`, else its sources in `docs/`, nothing fetched from the internet), **Start from scratch (tutorial)…** ([the tutorial](../tutorial/index.md) of the line application), **Glossary…** · **To do** (the panel) · **Controls ›** (the 3D view's and the Detail view's mouse and keys, the shortcuts) · **About OHL-Structure** (version, licence) |

The conventions, as in OHL-Design: an entry ending in **…** opens a
dialog or asks first; a panel never does (it opens, or comes to the
front). An entry greyed out says why on hover — the work menus without
a structure open say *Open a structure first*. A tick menu (View and
its submenus, Structure kind, Symmetry, Tools) stays open while you
tick; a long one scrolls within the screen. An action is named as the
tool doing the same (*Add a joint* in Body, on the Joints panel and as
the hotbar's tool) and shows that tool's icon; it brings its panel to
the front. **Undo**, **Redo** and **Save** are icon buttons at the
right of the menus (⟲ ⟳ 💾, the icons of OHL-Design's quick bar; their
names and shortcuts on hover). Shortcuts are written ⌘ on macOS, Ctrl
elsewhere.

A **right-click** in the 3D view on a joint, member or point selects it
and opens its menu: its name, **Inspector**, **Open in the Detail view**
(a tower's joint or member), **Point on the joint** (a joint),
**Insert a joint** (a member, at the Insert a joint tool's fraction),
**Link to a joint…** and **Unlink** (a point), **Delete…** (asked
first). On an object of a selection of several, or where there is
nothing, with something selected: the selection's menu (**Inspector**,
**Delete selected…**, **Clear selection**). A right-click on a panel's
**tab**: **Dock on the left (text and edit)**, **Dock on the right
(views)**, **Float in a window of its own** (as OHL-Design's pages).

Under the menus, the **header**: the file's name (its full path and the
structure's id and name on hover), *\* unsaved* when it has changes, the
level as a badge (**Level 3 — full design**), and a button **N to do**
(what the next level lacks and the advice, counted) that brings the
**To do** list to the front; for a placed structure, its loads (a link
to Line loads).

Under the header, the **panels**, docked as in the line application:
drag a panel's tab to move it (beside another, above, below, into
another group, or out as a window of its own), drag the borders to size
them, close one with its ×. **View** ticks each panel to show or hide it
(the menu stays open while ticking), **UI scale ›** sizes the whole
window's text and controls (90 % to 150 %; the menu stays open; kept
for the next start). **View › Display options ›** gathers what the 3D
view draws: **Rigid links** shows or hides the
rigid links, **Joints by their details** (off by default; a
tower's joints coloured green complete, orange partly assumed, red no
connection data — see [Joint details](#joint-details-towers)), **Volumes on the strings** (off by default)
draws the shapes of the strings' insulator assemblies — fittings, horns,
rings — on the strings as they hang in still air, grounded grey,
electrified orange, insulated pale (placed as OHL-Design places them on
the solved strings; see [Volumes on the
strings](structure-library.md#volumes-on-the-strings)), **Load arrows**
(a Loads panel in front; their scale in the load panels), **Deformed**
and **Reactions** (FE results in front; the deformed shape's scale in
FE results) and the **Usage legend**. The load and results panels keep
their own ticks beside the scales (the same settings). **Reset layout** puts every panel
back, and **Fit the view** shows the whole structure. The arrangement is
kept for the next start. By default (2026-11-04) the window has **two
panes**: on the **left the text and edit panels** as tabs — the
**Inspector**, the **To do** list, Points & strings, Joints, Members,
Supports (as many as show their titles; the others — Wire points & sets,
Generators, Groups, the load table, FE results, Line loads, … — open
there too, from their menus, in front); on the **right the 3D view**
with its hotbar. The **Detail view** opens as a **tab on top of the 3D
view** (not a third pane): opening a joint or member in it shows it in
place of the 3D view, its **Joint details** or **Member details** in
front on the left; **‹ 3D view** above the drawing (or **Escape**, once
nothing of the Detail view's own is left to undo) brings the 3D view
back, the Detail view staying open as the tab beside it; closed, the 3D
view shows again. A panel opened from a menu that is not open goes to
its side (a view on the right, the others on the left). Panes you move,
split or float stay yours: the arrangement is kept as you left it.
**Reset layout** brings back the two panes. An arrangement kept by an
older version gives way to this one once when it is that version's
default as it opened (the 3D view in the middle, the Inspector on its
right, the tables below, the Detail view beside the 3D view); one you
had changed is kept. The load recipes'
columns and fields are named in words (*Line case*, *Self weight*, *One
sided*); hovering one gives the key it has in the file
(`line_case`…).

The last message (what an action did, or why not) is at the bottom of
the window, and shows as a **notice** at the bottom right: a success or
information fades after a few seconds, an **error stays** until its
**×** is clicked (a long one scrolls in a box of fixed height, so the ×
stays in reach).

**Tables**: the column headers are in sentence case and carry the unit
(*x [m]*, *Weight [N]*, *Transverse [kN]*); the cells show the bare
numbers, right-aligned, and a number is dragged or typed as before.
Value fields have a darker inset fill and a frame, buttons the raised
fill. A table wider than its panel shows its scroll bar (the first
column scrolls with the others).

| Menu | Panels |
|---|---|
| View (its main panels) | **3D view**, **Detail view**, **Inspector**, **To do** (the Level panel), **Libraries** (closed by default) |
| **Geometry** (level 1) | **Points & strings** (Attachment points, Strings), **Wire points & sets** (Wire points, Sets), **Structure fields** |
| **Body** (level 2) | **Joints**, **Members**, **Generators**, **Pole**, **Extensions** |
| **Design** (level 3) | **Supports**, **Member cross-sections**, **Groups**, **Cross-sections & materials**, **Pole design**, **Joint details**, **Member details** (with the **Detail view**, a tab on top of the 3D view) |
| **Loads** | **Load case editor**, **Load table**, **Line loads** |
| **Results** | **FE results** |

The level of the file decides the panel in front when it opens: Points &
strings at level 1, Joints at level 2 (the Pole for a pole), Supports
at level 3 (Pole design for a pole). Panels closed by default (Structure
fields, Pole, Extensions, Cross-sections & materials, Pole design) open from
their menu or from View.

- **To do** (the Level panel; the header's **N to do** brings it to the
  front): the level is worked out from the file, never stated (the
  header's badge). **For level N + 1** lists what the next level still
  lacks — no member yet,
  a member without a cross-section (one item per member; more than three:
  one line, set per group in Design › Groups), no supported joint, an
  attachment point on no joint of the members (tick it in the peak or
  crossarm generator, or link it), a support joint without a support,
  and the same for each extension with a piece. A supported joint is one
  with a `support`: a structure standing only on `spring`s does not
  reach level 3 (this is how the level is worked out now). Each item is a link: it brings the panel and table
  where it is done to the front (opening it if it was closed), picks
  its row and selects its object. **Advice (n)** (folded while the next
  level lacks something) lists what reads
  but is worth a look, never counted in the level: a point on no joint
  or member (it carries no load in an analysis), a joint on one member
  or none, two members crossing without a joint (X-bracing bolted at the
  crossing is a common case).
- **3D view**, in the structure's own axes: x to the right of the
  line (red), y ahead (green), z up (blue), 0;0 at the centre, the ground
  grid at the level of the feet and a compass. **Attach joints** are
  ringed in orange, **support joints** marked with a green triangle, and
  each attachment point at an offset from its joint is joined to it by
  its **rigid link**, a magenta rod (none at zero offset; View › Display
  options › Rigid links). A click **selects** a joint,
  point, wire point, rigid link (it selects its attachment point, and the
  view writes its joint, offset and length in magenta — and, after a run, its
  moment in the case shown) or member, Shift + click selects another; a
  right-click opens the menu of what is under the pointer, or of the
  selection ([above](#the-window)). The view's notes are written in the
  colour of what they are about, without a box. A click never moves anything: a joint or point
  moves only with the **Move** tool held (below). A drag elsewhere turns
  the view, a right-drag pans, the wheel zooms. The buttons at the top
  left turn it to a **standard view**: **Front** (looking along +y,
  ahead), **Side** (looking along −x, from the right of the line),
  **Top** (from above, ahead up) — these three in parallel projection —
  and **Iso** (from the back left and above, in perspective); **Fit**
  (and View › Fit the view) shows the whole structure as it opens. **?**
  shows the controls and the axes' colours on hover. The loads of the
  case shown (while a load panel was the last clicked) or the results
  (while FE results was) are drawn over the structure; with the members
  coloured by usage, a **legend** at the bottom right gives the colours
  (over 100 %, 90 to 100 %, under 90 %, no capacity).
- **Hotbar and tools** (as in OHL-Design): nine slots at the bottom of
  the 3D view, each a tool; a click on a slot or the keys **1–9** hold
  one, **Escape** lets go of it (Select again), **E** (or the slots'
  **E** button, or Edit › Tools › Tool inventory) opens the **inventory** of
  every tool: drag one onto a slot, drag a slot onto another to swap
  them, or into the inventory to empty it (right-click empties a slot
  too). The slots and the slot held are kept for the next start. The
  tool held is named in a **strip above the view** with what a click
  does and its options. The tools: **Select** (a click selects; the
  default), **Edit** (2026-11-02: a joint, member or point clicked
  opens where it is edited — its fields in the Inspector, brought to the
  front, and its row in its table; a tower's joint or member, once asked, in
  the Detail view with its details on the left — 2026-11-03; with the Detail
  view shown already, there at once, not asked — 2026-11-04), **Move** (drag a joint or point to move it, snapped to the
  grid — **snap** in the strip — or onto a joint; one undo step per
  drag), **Add a point** (on a joint clicked, else at the top of the stick
  figure), **One string below the point** (below the point clicked; **string length** in
  the strip), **Two strings to one wire point** (click two points),
  **Link** (the Link tool, [below](#attachment-points-joints-and-the-link-tool)),
  **Add a joint** (above a joint clicked, or where you click — in the
  plane facing you through the structure's middle, snapped), **Add a
  member** (click two joints; the next member starts at the second),
  **Delete** (what is clicked, asked first); in the inventory also
  **Two strings to one wire point** (in the default hotbar before
  2026-11-02, the Edit tool now in its place), **Insert a joint** (click a member; **at** in the strip)
  and **Symmetry** (the body's symmetry and **Make symmetric** in the
  strip). Every change a tool makes is one undo step. Edit › Tools lists
  them all by group with their keys; the Geometry and Body menus have
  the same actions under the same names, on what is selected.
- **Inspector** (right of the view): what is selected, with its fields
  to edit — a **joint** (x, y, z, its kind, its support, the members
  meeting it, the points linked to it), a **member** (its joints,
  length, group, cross-section, kind, k), an **attachment point** (x, y,
  z, its joint, its offset — the rigid link, with its length and moment
  arm —, its strings; **Link to a joint…**, **Unlink**), a **wire
  point** (where it is, typed when no string reaches it; the lengths of
  the strings ending there; its set; its counterweight). Several of one
  kind: the coordinates they share (one that differs says so) or, for
  members, their cross-section and kind — a change is written to each,
  one undo step. Every change goes through the same edits as the tables
  (one undo step; a value dragged is one step). Nothing selected: a
  one-line hint.
- **Panels of tables**:

| Menu | Tables | Tools |
|---|---|---|
| **Geometry** (level 1) | Attachment points (where it is — typed: a point on a joint stays on it, its offset follows —, its **joint**, its **offset** x, y, z from it — typed for a point on a joint, one undo step —, the offset's length, the strings hanging from it; a right-click on its name: **Link to a joint…**, **Unlink**), Strings (top, wire point, kind, assembly, length, angle, direction, weight, wind area, compression, limits — max load, max swing, across, along, each optional and not checked, an assembly's shown dimmed —, description), Wire points (counterweight), Sets, Structure (name, height, feet, extensions) | **Add a point**, **One string below the point** and **Two strings to one wire point** for the points picked (the string's length: the tool strip of the string tools), **Link** (the Link tool), **Delete selected…** (everything picked, lists what goes with each first; one undo step); the grid a dragged joint or point snaps to is in the Move tool's strip |
| **Body** (level 2) | Pole (pole structures: the outline by height, [below](#poles-and-extensions)), Joints (x, y, z), Members (ends, length, group, cross-section, made by; a group filter), Generators, Extensions (each one's piece and legs) — Joints: x, y, z, **kind** (*auto* — worked out: attach when a point is linked to it, support with a support —, **attach** or **support**; one undo step; a joint carrying a point cannot be made a support joint), the members meeting it, the points linked to it | **Add a joint** (above the joint picked), **Add a member** between two joints picked, **Point on the joint** (not on a support joint: strings hang from attach joints), **Delete selected…**; the symmetry's icon (its option on hover; set in Body › Symmetry or with the Symmetry tool), **group** of the members picked ([below](#the-body-panels)) |
| **Design** (level 3) | Pole (pole structures: material, cross-section, wall or solid, strength), Supports (**Support the feet** pins the lowest joints; the button is above the table, and in the Design menu), Members (a right-click on a member's name: **Insert a joint** at a fraction of its length — [below](#attachment-points-joints-and-the-link-tool); the analysis settings **stabilise joints** and the **dead-load factor** — see [Structure models](structure-analysis.md#structure-models-ohlfe) —, each member's cross-section, kind (truss, frame, cable or tension — a truss in tension only, carrying compression up to its own buckling load, see [Tension-only members](structure-analysis.md#tension-only-members); one undoable edit, which raises the file to version 5), buckling factor k, effective length lk, the legs of an angle, its own tension and compression capacities and dead-load factor (DLF) — values typed with units, written when the cell is left), Groups (a cross-section and kind per member group, and lk, the capacities and the dead-load factor for all its members), Cross-sections and Materials (their fields typed with units, e.g. `1500 mm2`), and **Cross-section library** (the cross-section library's cross-sections — new, edit, duplicate, delete, their properties computed as typed, each change one undo step; see [Typing cross-sections](section-library.md#typing-cross-sections-in-ohl-structure)); the cross-section lists of Members and Groups hold the library's cross-sections first, then the file's | — |
| **Loads** (any level) | **Load case editor**: the structure's load cases (name, description, strength factor, the sentence of what each does), their line forces (a line result, vertical / transverse / longitudinal factors, sets, a tick per point) and loads on the structure itself (self weight, wind on the body, ice and additional weight, point loads), and loads in the older form when a case has some. **Load table**: the line's forces, then the cases evaluated, per case and point, member or joint | **+ Case**, **Duplicate**, **🗑 Case**, **Mirror wind**, **Add line forces**, **Add wind**, **Add ice**, **Add point load**, **+ Load** and **Convert** (older form), 🗑; **Import / Export recipes…** (`.ohlslc` or CSV), **Import / Export table…** (CSV), **Store in the structure file…**; the case shown drawn as arrows (**m per kN**) |
| **Results** (level 3): **FE results** | The usage per check (members, pole) with the governing case; per case: the governing member, the largest displacement, Σ loads and Σ reactions, reactions per support, members with axial force and usage, the pole check | **▶ Run** (one case or every case), **Extension** (solved again when changed), **Cancel**, **Report…**; deformed shape (**Deformed ×**), usage colours, reactions in the view |
| **Loads**: **Line loads** | Started for a placed structure: the line's values per point and case, the other places of the structure with their governing values ([above](#started-for-a-placed-structure)); opened alone: what it would show | — |

## Attachment points, joints and the Link tool

Every attachment point is linked to a **joint** of the structure
([Glossary](glossary.md#structures)): the joint it sits on, or — a
stick figure's point — its own joint. A point cannot be put on a member
(a bar): it needs a joint. It may sit at an **offset** from its joint: a
**rigid link**, drawn as a magenta rod, whose moment the analysis
applies at the joint (where a frame member meets it; [Results](#results)).
A joint has a **kind**: *attach* (strings hang here), *support* (the
structure stands here), or none — set in the Joints table, or worked
out (*auto*).

- **Link tool** (Edit › Tools › Link, **Link** on the Geometry and
  Body panels, the hotbar's Link, or **Link to a joint…** in a point's
  right-click menu): click an attachment point, then an attach joint — the
  point is linked to that joint **where it is**, its distance from the
  joint kept as its offset; one edit, one undo step. While a point is
  chosen, only the **attach joints** are ringed in green and take the
  link (2026-10-21); the one under the pointer is joined to the point
  with the offset and its **moment arm** (its length across the
  vertical, the arm of a vertical load) written beside it. Another joint
  under the pointer says *not an attach joint: set its kind to attach in
  OHL-Structure* — set it in the Joints table (**Kind**), then link. Relinking a linked point is the same two clicks. A
  **right-click** or **Escape** cancels the point chosen; a second one
  lets go of the tool.
- **Unlink** (a point's menu, in the view or the Attachment points
  table, or the Inspector): the point keeps its place as a joint of its own, on no member
  (its load then reaches no member: the header lists it until it is
  linked again).
- **The link follows edits** (2026-10-21): an attachment point on a
  joint that is moved — typed in the Attachment points table or the
  Inspector, or dragged in the view with the Move tool — stays on its joint, its offset recomputed (the new
  position less the joint's, in the structure's axes), one undo step;
  the joint does not move. A joint moved takes its points along at
  their offsets. **Offsets** can also be typed directly.
- **Insert a joint** on a member (a right-click on the member in the 3D
  view, or on its name in the Members table of the Design panel; the
  Insert a joint tool): an attach joint at the
  fraction **at** of its length from its first joint, the member split
  there into two pieces that stay **continuous** — the same cross-section,
  kind and other fields on both sides (a rigid connection, not a hinge),
  the first keeping its id, the second `<id>-2`, each with the whole
  member's buckling length written as `lk` (**conservative**: the joint
  braces nothing; set `lk` when it is braced) — one undo step. Then link
  a point to it with the Link tool. A point on a joint that only bars
  (trusses) meet in a line cannot take a load across them: the results
  refuse it with the fix (make the members frames, or add a member to
  the joint). Files written before with points along a member open with
  such joints inserted (shown as changes not saved).

## The Body panels

The body is built from a few dimensions with **generators**, then
completed by hand; the file keeps each generator's inputs, so it can be
built again ([format](structure-library.md#body-generators-groups-and-symmetry)).

- **Generators** (a panel of the Body menu): **New** › *legs and panels*,
  *crossarms*, *peak* or *copy a face* opens a form with the generator's
  inputs; what it would build is drawn in the view (turquoise) and
  counted under the form, or why it cannot be built is said. **Apply**
  writes it into the file: one undo step. The table lists the
  generators of the file with the joints and members each made:
  **Edit** opens its form again, **Rebuild** builds it again from its
  inputs, **Remove…** removes it and what it made (asks first),
  **Rebuild all** builds every generator again in file order.
  - *Legs and panels*: the widths at the feet and at the waist (across
    and along the line), the level of the feet (default: the file's
    foot), and per panel its top, its bracing (single diagonal, X, K,
    diamond, Warren) and whether it has horizontals at its top; **+ Panel**
    adds one on top. A new body goes up to the lowest arm (the waist
    under the crossarms) in four X panels, then (since 2026-10-24) one
    panel to each higher arm level at the waist's width, so every arm has
    body joints beside it; without arms, four panels to the height. Options
    per panel (all off by default, [format](structure-library.md#body-generators-groups-and-symmetry)):
    a **width** at its top (ticked: the slope changes there, say legs
    then a straight body; the last panel's is the waist), **redundants**
    (*none*, *one* or *two* levels of secondary members in its faces,
    group `body-redundant-<panel>`) and **plan** bracing at its top (*x*
    or *diamond*, group `body-plan-<level>`). Above the table, **plan
    bracing where the slope changes and at the crossarms' levels** puts
    that plan bracing at those levels (a panel's own goes first; since
    2026-10-24 a crossarm applied builds such a body again by itself, its
    own undo step, so the body knows the new level).
  - *Crossarms* (a new one ticks the points off the structure's axis;
    those on it are the peak's): the points ticked each get an arm from the body face on
    their side, triangular (four chords to the point) or rectangular (a
    tip at the point and one above it); the point is put on the arm's
    tip, so it carries its loads in the analysis. **Several points on one
    arm**: the points on one side at one level (an inner and an outer
    phase, the two upper points of two strings to one wire point) make one arm, the outermost at
    its tip and the others each on a cross-section of the arm, every point on
    a joint.
  - *Peak*: a tip above the top of the body, or one at each earth-wire
    point ticked (a new one ticks the points on the axis above the feet),
    joined to the joints at the top. On a pole the crossarm generator
    says so: a pole's points are checked where they are (Pole design).
  - *Copy a face*: the members on one face of a legs-and-panels body (a
    redundant added by hand, say) copied to the opposite face, without
    doubling those there already.
- **Rebuild** replaces only what the generator made. What was set on its
  joints and members (a cross-section, a kind, a group, a support) stays when
  they come again; joints and members made by hand are never touched; a
  joint it no longer makes stays while a member made by hand or a point
  uses it, and is listed under the table (*kept, as made by hand*).
- **Symmetry** (Body › Symmetry, or the Symmetry tool — 2026-11-03: no longer on the Joints and Members panels; none, x, y or both): with it on, a joint or member
  added, moved (dragged in the view or typed in the Joints table) or
  deleted takes its mirror images along — about x = 0 (left and right of
  the line), y = 0 (ahead and back) or both. A joint on a mirror plane
  stays on it. **Make symmetric** adds the mirror images every joint and
  member lacks.
- **Member groups**: pick members (Shift + click adds one, in the view or
  the Members table; **Pick them** picks the members the filter shows),
  type a group and **Set on N member(s)**; **No group** takes them out.
  The generators give their members groups (`body-leg`,
  `body-bracing-3`, `arm1-top`…). In **Design › Groups** a cross-section and a
  kind are chosen per group and written on each of its members; a member
  joining a group whose members share a cross-section takes it.

## Poles and extensions

- **Body › Pole** (a structure of kind pole, or with a pole): the
  pole's cross-section (round, or a polygon with its sides), its
  embedment, and its **outline** — a row per height with its z and
  diameter (dragged or typed; the circumference shown), 🗑 per row,
  **+ Height** (a height half-way up the top step; the first time, the
  feet and the top). The view draws it as a wireframe: a ring at each
  height and lines along the pole. A pole named by a `.ohlp` file is
  shown as such; **Bring it into this file** writes its outline and
  fields into the structure file (one undo step; the pole check gives
  the same numbers). A `.ohlp` without a wall brings none: level 3
  lists it until you type a thickness, tick solid or pick a cross-section. Diameters at two heights make level 2.
- **Design › Pole design**: the material (wood, steel, concrete, other), a
  **cross-section** of the cross-section library next to the file (a polygonal
  hollow cross-section with the pole's sides, or a circular hollow one for a
  round pole: its wall, and its material's elasticity and density), or
  the wall typed, or **solid** ticked (`wall = "solid"`: no hollow; it
  stands for the wall at level 3; unticked, the wall is not given again;
  each one undo step; a word other than `solid` typed as the wall is
  refused); fibre stress, moment capacity, elasticity, density,
  drag. Under them what level 3 still lacks, or *The pole check can read
  the pole (level 3)*.
- **Body › Extensions**: the extensions (**+ Extension**: +3 m, +6 m…),
  the one picked with its `dz` and **show in 3D** — the view then shows
  the structure standing on it, its piece below the feet (lilac) and its
  legs lowered, as the finite-element model has it. Its **piece**: built
  in one go (**Build the piece**: under every foot a joint `dz` lower
  along the foot's leg, with the foot's support, a leg to the foot, the
  faces braced — legs only, single diagonal or X, horizontals on
  request; it replaces the piece there), or by hand (**Joint** at the
  bottom of the piece, **Member between the 2 picked joints**); its
  joints (support) and members (cross-section) in two tables, 🗑 per row. Its
  **legs**: a leg A–D one `dz` longer (**+ Leg**), the `dz` dragged, 🗑.
  Every change is one undo step. The header lists, for level 3, an
  extension member without a cross-section or a piece without a support, and
  as advice an extension without members (*the FE model leaves it
  out*).

## Joint details (towers)

*2026-11-02, stages 0 to 3, 4.1 and 4.2 of the
[joint details](../development/joint-details.md); built, in active
development, not approved yet.*

**Geometry › Structure kind ›** lattice tower, portal, pole or other (the
menu stays open). A lattice tower or a portal has joint details; a pole
its Pole panels. One undo step; nothing is deleted: when a tower with
connections becomes a pole, the message says how many connections and
plates stay in the file, not shown.

**Design › Joint details** shows the joint selected last (in the 3D
view, the Joints table, or picked in its list; the Detail view follows
the selection even when the panel is behind another tab; each joint with
how complete its details are, in its colour): its drawing in the
**Detail view**, a tab on top of the 3D view (2026-11-04: two panes by
default; View › Detail view shows or hides it, a right-click on a joint
or member: **Open in the Detail view**; no window over the view; **‹ 3D
view** above the drawing brings the 3D view back), and its tables in
**Joint details**, in front on the left with the other text and edit
panels (Member details for a member). Joint or Member details found in a
pane with a view are moved to the left when next opened; elsewhere they
stay where you put them.

The **Edit** tool of the 3D view on a tower's joint or member first asks
**Open … in the Detail view?** (2026-11-03): **Open the Detail view**
shows it there, in front, with its details in front on the left;
**Stay** keeps to the Inspector (its fields, as for any other object).
When the Detail view is open already (in front, or the tab behind the 3D
view), it is not asked again: the joint or member clicked is shown there
at once, the Detail view in front, its details in front on the left
(2026-11-04).

- **Close-up**: the members meeting there as their cross-sections (angles
  with their roll, so the leg bolted shows), cut a little beyond the
  joint, the bolts through the leg bolted (red; grey where their layout is
  partly assumed), the plates (blue; the plates of a **welded plate**
  amber, their **weld seams** red where they meet; a plate without
  an outline in the file lies on the leg of the corner member, in the
  face of the bolts onto it, shaped to the
  corner: over the corner leg's width where the members' bolts reach
  along it, over each member's bolted leg from its first bolt to its
  last, and **round every bolt onto it with the design code's distances
  kept from its edge** — the end distance along the member, the edge
  distance across it, worked out in the plate: its own thickness and
  its material's steel (else the member's), the member end's force and
  bolts (ASCE 10-97 4.5, SIA 263 Table 17; with the simple check, the end's own end and
  edge distances, else 1.5 bolt diameters, assumed) — as the IFC export
  lays it); drag to turn it, scroll to zoom, **Reset view**. Bolts are
  drawn on angles only. Every face is drawn in small pieces sorted by
  their own depth, so the bolts stay in sight from every side (before
  2026-11-03 a member's long faces could hide the heads when turned);
  since 2026-11-04 the pieces are cut once per joint and file change —
  finely round the bolts only, growing with the distance from them, at
  most 40 000 — and only sorted again when the view turns, so turning
  stays smooth (measured on a joint of 4 members, 25 bolts and a plate:
  16 ms a frame before, 3.5 ms turning and 0.4 ms still after).
  I/H, hollow and polygonal profiles are drawn by their outline (a box
  before).
- **Face view**: the joint square to a face (x+, x−, y+, y−, or the plan
  from above; the face most of its members lie in first), as a shop
  drawing: the members' outlines, the gauge lines, the bolts, the end
  distance and pitch dimensioned along each gauge line (mm), the gauge,
  a label per member (its id, cross-section, bolts). **Save as SVG…** /
  **Save as PDF…** put it on an A4 sheet at the largest of 1:2, 1:5,
  1:10, 1:20, 1:50 that fits.
- **The connections table**, one row per member end: the member (click:
  select it), its end (i or j), class, the face it lies in, its angle to
  the member it bolts onto (or the joint's leg), how it is held, onto
  what, the leg bolted, the bolts, gauge lines, gauges, pitch, end and
  edge distance, eccentric, restrained, the eccentricity. Each value's
  colour says where it comes from (its hover too):
  - **given** at the member end (`[[connection]]`);
  - **the member's** design data, in italics (both ends alike);
  - **worked out** (blue): onto the joint's leg (the members of class
    leg, else those within 30° of vertical; the largest, or the one in
    line for a leg); at an end with a connection, eccentric from one leg
    bolted (both legs: not); not restrained from one bolt, or from bolts
    onto a gusset plate only (ASCE 10-97 3.7.4.5). Whether two bolts or
    more onto a member restrain the end depends on the stiffness of the
    members at the joint (C3.7.4 (b)): the check works it out and says it
    in its results; this window leaves it not given — an end without a
    connection keeps the member's flags or the assumptions, as the check
    does; the eccentricity from the gauge and the section;
  - **assumed** (orange): as the checks assume when nothing is given
    (one leg bolted, the ends eccentric, not restrained), or for the
    drawing only (gauges evenly across the leg, a pitch of three and an
    end distance of one and a half bolt diameters);
  - **not given** (—).

  The **gauge lines** of an end without a connection giving them come
  from the **holes in one section** of the member's design data (what a
  tower program's export gives, 2026-11-04): one leg bolted, a line per
  hole; both legs bolted, half of them per leg (rounded up); one to three
  lines, worked out, said below the table. Bolts staggered between two
  lines put fewer holes in one section than there are lines: give the
  gauge lines at the end then. No holes given: one line, assumed.

  Above the table, a member whose **leg bolted does not lie flat** on
  what it bolts onto (more than 5° off the plate's face or the flange of
  the leg) is named — "N2: D1 (UA-83x52x6) bolted face 90° off the leg of
  L1 — turn it" — with **Turn it flat**: its roll turned by the smallest
  turn that lays it flat (one undo step; see **Roll** below). The other
  notes on the member ends follow.

**Defining the joints** (stage 3, 2026-11-02). Every value of the
connections table is edited in place and given at that member end: a
list (held, onto — the members meeting there and the joint's plates —,
leg, gauge lines, eccentric, restrained) or a length typed in mm (the
first gauge, pitch, end and edge distance, eccentricity; `30`, `30 mm`
or `0.03 m`). **Not given here** in a list, or right-click a typed
value, clears it: back to the member's design data or what is worked
out. Each change is one undo step.

- **Bolts** (click the bolts of a row, or its **…** menu › Bolt layout):
  the bolt layout form above the table (no window) — a **bolt of the
  bolt libraries** (its diameter, hole and grade the library's; only its
  id is written) or one typed here, how many, diameter, hole, grade, one or two
  gauge lines (one to three; every second line staggered by half a pitch) and their gauges, pitch,
  end and edge distance, starting from what the row shows; it says how
  far the bolts reach along the member and what does not fit the leg (a
  gauge beyond the leg less the edge distance, a pitch no larger than the
  bolt). The minimum distances of a design code are typed: none is
  built in. **Apply** writes them all at that end (bolt points given
  before go).
- A row's **…** menu: **Clear this end**; **Same on every end of group
  …** / **of class …** (what is given here on both ends of every member
  of the same group or class, what they bolt onto kept); **Save as a
  preset…** and **Apply a preset**.
- **Copy to the N joint(s) like it**: every member end of this joint on
  the joints with the same members — cross-section, class and angle to
  what they bolt onto within 1°, so the mirror joints too — their
  connections replaced, what they bolt onto matched, a plate copied to
  each. Each end's own fit goes with it (2026-11-04): where its steel
  **stops**, its **shift** (+ towards what it bolts onto, so the same on
  a mirror joint) and its rolled edge — before, those fell back to
  worked out on the copies.
- **+ Plate** (2026-11-03): a gusset plate at the joint, on the joint's
  leg (10 mm to start: type its thickness); a joint may have several.
  The plates table gives each one's **Welded to** (a menu with a box per
  other plate of the joint — it stays open on a tick; each tick one undo
  step), thickness, material and the members bolted onto it (Onto); its
  id shows its properties above the table; 🗑 removes it (what bolted
  onto it: not given any more; the plates welded to it no longer name
  it).
- A **welded plate** (2026-11-04) is several plates **welded together**
  so that every member bolting onto the joint reaches one of them — not a
  plate of its own kind. **Make a welded plate for every member** builds
  it: one plate per plane the members arriving lie in with the joint's
  leg (within 10°; the legs themselves, modelling members, members held
  otherwise than bolted and splices in line with the leg left out, said),
  each member bolted onto the plate of its plane, each plate welded to
  the ones before it, the plates' outlines worked out round their bolts
  with the design code's end and edge distances in the plate (as for any
  plate without an outline); the joint's plates before are replaced (one
  undo step; its hover lists the plates it will make). The thickness is
  the largest of the joint's plates, at least 10 mm. Two plates of a
  welded plate that do not meet are said. The members bolt onto the
  plates of a welded plate as onto any plate. A file of
  2026-11-03 with `kind = "welded"` plates (then: welded onto the leg)
  reads those plates of a joint as one welded plate (one alone: a plate),
  said under the table, and written as `welded_to` at the next welding
  change.
- **Welds** (2026-11-04): under the plates table a row per pair of
  plates welded to each other — its **kind** (fillet, full penetration),
  its **size a** (the fillet's throat, mm; empty: not given, the weld not
  checked) and its **length** (mm; empty: the seam's length where the
  plates meet, shown in grey as worked out — one side; give the whole
  length, both sides together, when welded on both). Each change is one
  undo step, written as `[[plate.weld]]`; a plate's box (a plate picked)
  has its own welds' rows. The seam's label (`a6 × 220`) is drawn in the
  close-up and the face view.
- **Plate checks** (2026-11-04): after a run (Results › Run), Joint
  details lists the joint's plate checks — per plate each member's bolts
  in it and each weld — with the usage in the case shown, the largest
  over the cases and its case, and what governs with its clause;
  **Each check in case …** opens every check and what was assumed (in a
  scroll box). The 3D view's results mark each joint with plates by its
  worst usage, as the members (orange from 90 %, red above 100 %), and
  the structure report has a table of them. By the structure's capacity
  method: ASCE 10-97 (bearing 4.4, block shear Eq. 3.10-1 applied to the
  plate, net section 3.10.1, distances 4.5; no weld rule in the standard:
  welds not checked, said) or SIA 263 (bearing (76), net and block shear
  (82), (83), sections (38), (39), Table 17's distances, fillet welds
  (87), (88)); the simple check checks neither. A plate in compression:
  plate buckling not checked. Every assumption is on the theory page
  [Gusset plates and their welds](../theory/joint-plates.md).
- **Presets** are kept in `Connections/connection-presets.ohlcp` next to
  the structure file (format `ohl-connection-presets`; a `[[preset]]`
  per name with the fields of a connection but `onto` and `bolt_points`),
  for every structure in the folder; a file that cannot be read is said
  above the table.

**Design › Member details** (2026-11-02) shows a tower's member — the
one selected (the Edit tool opens it on the member clicked, once asked) or picked in
its list — from its own side:

- its **Roll** (2026-11-04: "a correction angle for the members to
  rotate them on their axis as one face need to come in contact with the
  plates"): the degrees typed (Enter), **−90°**, **+90°**, **180°** and
  **Lay flat** — the smallest turn that lays its leg bolted flat on the
  plate or the flange of the leg it bolts onto (at the end lying more
  than 5° off, else the first; a further 180° turns the outstanding leg
  the other way; its hover gives each end's angle off). Each one undo
  step, written as the member's `roll` (with an `orientation`, a turn
  after it). The same control is in the Detail view above a member and
  in the member's Inspector. **The roll turns the member's axes for the
  analysis too**: its bending axes (y, z) and so its stiffness in a
  frame, its member wind and the exports; a truss's axial force does not
  change. Members brought in from another program sometimes lie a
  quarter turn off: Joint details names them;
- its **two ends as rows of the connections table** (the joint at each
  end first): the same `[[connection]]` data the joints show and edit, so
  bolts, gauges, pitch, holes set here are the joint's, and the reverse;
  the bolt layout form and presets as in Joint details;
- its **resistance worked out live** (ASCE 10-97, as the check works it
  out): the tension and compression capacities and what governs each
  (the net section, the bolts' shear or bearing at an end, KL/r), KL/r,
  a slenderness beyond its class's limit, where each end's conditions
  come from and what was assumed — recomputed on every change; and the
  **usage** under the last FE run's largest tension and compression
  (Results › Run first: the bolts and holes do not change the forces,
  so the usage follows each change too);
- in the **Detail view**, its **leg bolted laid flat**: both ends (the
  middle broken out when long), the heel's edge below, the holes and
  their bolts (orange where their place is partly assumed), each end's
  end distance, pitch and gauge.

The Detail view shows the joint or the member selected last. A member
opened from a joint (the Edit tool on it in the face view, or selected)
has **‹ Joint N2** buttons above it — the joint it came from first, then
its other end — and **Escape** (the pointer over the Detail view, or the
Detail view or its details in front) goes back to that joint.

**Where a member lands on the joint** (2026-11-02). By default every
member's axis runs to the joint's work point — the forces meet there —
and its **steel stops** where it fits: before the outstanding leg of the
angle it bolts onto, in the face they bolt in (worked out; **Stops mm**
in the table gives another). A member end can be **shifted sideways**
(**Shift mm**, + towards what it bolts onto) so it fits on a gusset plate
or beside another member: its force then passes beside the work point,
an **eccentricity** the analysis takes into account (the face view draws
each member's axis dashed, to the work point or beside it). A layout's
end distance is measured from where the steel stops.

**The Detail view's tools** (2026-11-03: a toolbar over the bottom of the
drawing, as the 3D view's hotbar — click a slot to hold its tool — and
above the drawing the tool held with what a click does, as the 3D view's
strip; the tools act in the face view and the member view, the close-up
shows them greyed; Escape goes back to Edit):

- **Edit**: a **bolt** clicked — its properties at the top of the details
  below (its place, its kind, the library bolt's values, whether it keeps
  the code's distances) with the **bolt libraries** to change it (a
  member end's bolts are of one kind: the bolt chosen is that end's; one
  undo step), **Bolt layout…** and **Remove this bolt**; it is ringed in
  the face view. A **plate** clicked — what it is welded to (its Welded
  to menu) and the welded plate it belongs to, its thickness, the leg it
  lies on, the members onto it, and its **Plate details / outline**
  (below). A **member** clicked (face view) —
  the member in detail (‹ Joint, Escape: back); in the member view, a
  click beside the bolts opens that end's bolt layout form. Escape lets
  go of what was picked;
- **Add bolt**: a click on a member's leg bolted — a bolt there, placed
  by the **design code of the capacity method** (ASCE 10-97: its minimum
  end distance, spacing and edge distance, 4.5): onto the nearest gauge
  line, at the place clicked or the nearest place along it that keeps the
  distances; where that line has no room near the click, a **new gauge
  line is offered** at the nearest gauge that keeps them (up to three
  lines) — written only when you confirm ("Add the line and the bolt");
  where there is no room at all, it says why. With the simple check
  (no code) the bolt goes on the nearest line, along to 5 mm;
- **Move bolt**: drag a bolt — it moves and is placed as Add bolt places
  it (in the face view along the member only; in the member view along
  and across);
- **Remove bolt**: a bolt clicked goes;
- **Member end**: drag a member — along it, where its steel stops; across
  it (in the face view), its end's shift;
- **Plate around the bolts** (above the drawing, when a plate has no
  outline in the file): the outline it is drawn with — round its bolts
  with the code's distances kept — written into the file, to be edited;
  a plate's properties offer the same for that plate alone.

**A plate corrected by hand** (2026-11-04; built, in active development,
not approved yet). When the automatic outline round the bolts does not
fit — a plate on the wrong side of a flange, a corner that clashes with
another member, an outline the shop wants simpler — a plate picked with
**Edit** shows **Plate details / outline** in its box:

- **Start from the automatic outline** (a plate without an outline in the
  file): the outline drawn round its bolts copied into the file as the
  plate's corners, to be edited (one undo step). From then on the plate
  keeps these corners and their plane whatever the bolts do.
  **Back to automatic** takes the outline out again: drawn round its
  bolts with the code's distances, in the plane of the members bolted
  onto it (one undo step).
- **The corners** in the plate's own plane, in mm: **u** along the
  joint's leg the plate lies on (else the first member bolted onto it),
  **v** square to it in the plane; 0, 0 is the work point laid square onto
  the plate (the box says how far the work point lies off the plate's
  plane). Each value typed and left (Enter, Tab or a click elsewhere)
  moves that corner in the plane; **🗑** takes a corner away (three stay
  at least); **+ corner** adds one midway between the selected corner
  (click its number) and the next, else between the last and the first.
  Each change is one undo step, written as the plate's `outline` in the
  same plane.
- **Plane offset** and **Move the plate**: every corner moved square to
  the plate's plane by the millimetres given (positive along the normal
  whose largest component points along +x, +y or +z), e.g. onto the
  other side of a flange; one undo step.
- Beside them the **edge distances** the plate keeps from each member's
  bolts and, in red, every bolt now nearer its edge than that, so a
  corrected outline shows at once whether it still keeps the code's
  distances; after a run its **checks** (each part's largest usage over
  the cases, that case and what governs). An outline changed after the
  run is checked again at the next run.
- In the **face view**, with Edit and such a plate picked, its corners
  are **handles**: a corner dragged moves in the plate's plane (the point
  of the plane seen under the pointer; one undo step on release). A plate
  seen more than about 45° off square in the face shown says so instead:
  its corners are dragged in a face more square to it, or typed in its
  box.

A bolt added or taken away turns that end's layout into its bolts'
places (`bolt_points`); the bolt layout form makes a regular layout
again. Every action is one undo step.

**The code's distances** (2026-11-03). The member view shows each end's
minimums in one line above the drawing (`ASCE 10-97: end ≥ 21 mm
(4.5.1), spacing ≥ 52 mm (4.5.2), edge ≥ 19 mm (4.5.3, Eq. 4.5-7;
sheared edge) — P 30.2 kN per bolt from the analysis`), the bolt layout
form the same with what of the layout breaks them, and both views ring
in red every bolt that breaks one (the clause on hover). The force per
bolt P is the member's largest axial force over the last analysis
(Results) shared by its bolts; without an analysis, the bolt's shear
capacity (on the safe side); without either, the force terms are left
out (said). The leg's edge is taken as sheared (the larger minimum)
unless the connection says `rolled_edge = true`. Change the capacity
method and the tools follow its code (the simple check has none); a
code added later brings its own minimums.

**How the joint is drawn.** The model's member axes meet at the joint;
a real joint's legs lie back to back. The drawings shift each bolted
member across (along its bolts) so the leg bolted lies flat on the face
of the leg it bolts onto, turned towards it — for the drawings only, the
analysis keeps the axes. The leg bolted is the long or short one given;
otherwise the one that can lie flat against the other member (the
assumed leg follows it). A bolt is drawn as its hexagonal head on the
member's own side, its shank through both legs (the plate, when it bolts
onto one) and its nut. A member bolted onto a plate lies flat on it: its
leg bolted is the one parallel to the plate, which lies on the corner
member's leg in the plane of the members onto it. A bolt is drawn with the sizes of its library bolt
(`head_across_flats`, `head_height`, `nut_height`) or else, for the
drawing only, 1.5 diameters across the flats, a head of 0.65 and a nut
of 0.8 diameters. Members whose angles have no `orientation` lie as the
model turns them by default, often not as on a tower: give their legs'
directions (`orientation = "y-x+"`…) for a true picture.

A joint is **complete** when every member end's condition is given, the
member's or worked out; **partly assumed** when some of it is assumed;
**no connection data** when no end has any. Below level 3 the panel
lists the members meeting at the joint only.

**What the check takes from the table** (stages 4.1 and 4.2,
2026-11-02): the ASCE 10-97 member check takes each end's **eccentric**
and **restrained** exactly as the table shows them (given, the
member's, worked out, assumed — the rules are the check's own), the two
ends together choosing its slenderness curve; and at each end the
**bolts** (count, diameter, hole, shear planes, a library bolt's shear
capacity with the threads in or out of the shear plane and its
strength), the **holes** in one section (the gauge lines) and the **leg
bolted**, for the bolt shear, bearing and net section — the weaker end
governs. The end and edge distances, the pitch, the plates' own checks
and the eccentricity's value are shown but not used yet. A member with
no connection is checked from its design data as before. In the **FE
results**, what governs a member with connections says per end where
each condition comes from (`[joints: N2 eccentric (worked out: one leg
bolted), restrained (worked out: 2 bolts onto L2); N4 …]`) and names
the end that governs (`bolt shear at N2`); the run takes the bolts of
the bolt libraries beside the structure (`Bolts/`).

## Load cases

The **Load case editor** (Loads menu) edits the structure's
`[[load_case]]` tables ([Structure load cases](structure-load-cases.md)):
the cases on the left (name, the first line of the description), the
case on the right:

- **Name**, **Strength factor**, **Description** (several lines), and in
  a frame the sentence of what the case does, made from it (*storm — wind
  from the left: vertical × 1.35, transverse × 1.5, longitudinal × 1.0
  at every point (without A3) · self weight × 1.35 · …*).
- **1. Line forces** (collapsible): a row per line result taken — the
  **line result** (a list of those the line sent, each with its weather
  case's wind on hover; opened on its own, typed by name), the
  **vertical**, **transverse** and **longitudinal** factors, **at**
  (empty: every point, or set numbers), **Points (n off)** — a tick per
  attachment point (its label, sets and phases) turning its line forces
  on or off; the menu stays open on a tick — and 🗑; **Add line forces**.
  Without line results a line says where they come from (OHL-Design ›
  Criteria › Loads for structures).
- **2. Loads on the structure itself** (collapsible): **Self weight**
  (its factor), **Wind on the body** (from ▾ — left, right, ahead,
  behind and the four between —, its own pressure or the pressure of a
  line result ▾, × factor), **Ice and additional weight** (a thickness
  and density, or a load per metre; on `all` or member ids; × factor),
  **Point loads** (a force, a direction ▾, at points or sets), each with
  **Add** and one line saying what it does and where it lands in the
  load table.
- **Loads in the older form**, only when the case has some: its
  `line_case` and older `self_weight`, one row per load (`what`, `side`
  and `direction` chosen from a list; a value takes its unit, `2 kN` or
  `0.6 kPa`, a plain number is N or Pa; lists separated by `;`), **+
  Load**, and **Convert** (the loads into the parts where that is exact;
  the message says what stayed).

Case buttons: **+ Case** (new), **Duplicate**, **🗑 Case**, **Mirror
wind** (a copy with left and right swapped in its name, line results and
body wind). A value typed in the new parts takes its unit or the one
shown (Pa, mm, kg/m³, N/m, kN). A field is written when it is left
(Enter, Tab or a click elsewhere), each change one undo step; the file
is raised to version 7 when a case has the parts. When the cases do not
read, the reason is shown in red.

**Import recipes…** takes the cases of a `.ohlslc` or CSV file: a case
with the name of one in the structure replaces it, the others are added
(one undo step). **Export recipes…** writes them as `.ohlslc`, or CSV
(cases in the older form only; a case with the parts is refused there,
said).

The **Load table** panel shows first the line's forces (started for a
placed structure, [above](#started-for-a-placed-structure)), then the
recipes evaluated: per case (or every case) and per point, member or
joint, the transverse, longitudinal and vertical load in kN, where the
case comes from (**from**: *line load case*, *recipe*, *typed*), where the
value comes from (the part: `line`, `wind`, `ice`, `point`, `recipe`,
`typed`; the **load** column names the row's part, load 2, line 1, wind
1, …), the member's **area** a
surface load acts on, and why a row has no value. Opened on its own, the
structure has no line: every load taking a line value shows **no line
value**. Type the
values (drag or click a number), or **Import table…** a CSV with the
table's columns (from **Export table…**, another program or a client's
load tree): what it has beyond the evaluation becomes typed values.

**Typed values** stay when the recipes change and the table is
evaluated again; ⟲ takes one back. They are not part of the structure
file: **Save** writes them beside it as `<name>.loads.csv` (for
`type-02.ohls`: `type-02.loads.csv`, the table's CSV with the typed rows
only), and opening the file reads them again; started for a placed
structure, as `<name>.<structure>.loads.csv`. Unsaved typed values count
as unsaved changes.

**Surface loads** (a `pressure` on `members`) take each member's area
from the cross-section library (`--sections`, else the `Sections` folder
beside the file; the line application passes the project's): the
profile's width seen along the pressure's direction (in the member's
cross-section plane) × the member's length × sin θ, θ the angle between
the direction and the member — for a member across the direction,
width × length. The row shows the area used. A member whose cross-section is
not in the library, or a structure whose model does not build, has **no
area** and says why. The line under the buttons says how many cross-sections
were read and from where.

**Arrows** draws the loads of the case shown in the view, ending at
their point (a member's at its middle), **m per kN** long.

### The line's load cases: the library (2026-11-03)

Owner, 2026-11-03: "The library's cases" (built, in active development,
not approved yet). A placed structure opened from the line gets **its set
of the line's load case library** (its own, else its type's, else every
case; [Per-pylon sets](loads.md#version-4-removed-conductors-several-broken-wires-the-winds-direction-ice-and-snow-on-the-structures))
as its **load cases**, with the library's factors: each case takes

1. the case's own forces at the attachment points (unfactored) × its
   vertical, transverse and longitudinal factors;
2. the structure's self weight × its vertical factor; the wind on the body
   from the case's direction — any angle (`from = 30.0`, 0° from the
   right, 90° from ahead) — its pressure on the structures (with
   `use_local`, × this structure's `wind_static`) × its factor; ice on
   every member (the case's thickness or this structure's local ice — also
   when the case takes the local conditions, gives no ice of its own and
   its weather case has ice, said in its description —, its density) and snow on every member's plan (q = μ·C_e·C_T·s_k), × the
   vertical factor;
3. its strength factor φ.

The Load cases panel lists them first, *From the line's load case
library* (read here, changed in OHL-Design), then the structure's own
cases as **extra** load cases — for loads the line's cases do not give.
The line's values of a library case are taken by that case only (no row
of its own in the load table). An own case with a library case's name
makes the library's `<name> (library)`. Older line-values files (version
4 and before) and lines without a load case library give none: the own
cases as before. The weather cases ticked in Criteria › Loads for
structures still come as line results (unfactored, for the own cases),
to every structure whatever its set; a broken-wire case sent breaks every
wire it lists, all together, and leaves out its removed conductors
(`high wind — S1 span 1 broken, S1 span 2 broken — wind from the left`).

### The line's wind on the structure body

Each line case comes with the wind on the structure's own body as the
line's checks take it: the load case's own values for the structures
(`[case.structure]` of the `.ohllc`,
[Load cases](loads.md#wind-on-the-structures-apart-from-the-wires-casestructure))
or its weather case's pressure and transverse factor. A line case with
such a wind gets, after its forces at the points, one row per member:
pressure × factor × the member's area (as a surface load, source `line`,
note *the line case's wind on the structure body*), blowing as the
line's wind (−x for wind from the right); a pole takes the same
pressure as its wind. So a line case solved here loads the body with the
same pressure as the line's **fe** and **pole** checks. (How the
pressure becomes member forces still differs: the line's member wind
takes the cross-section library's drag and shielding factors, the surface
loads here the projected area without drag or shielding,
[theory](../theory/structures-fe.md#the-structures-wind-of-a-load-case).) Recipes take it with `what = "structure_wind_factored"` (or
`structure_wind`, unfactored) on `members`; **Take the line's load
cases** adds that load to a case taken from a line case with wind, and
**Store in the structure file** stores it as a `pressure` on every
member. Files written before have no such values: a recipe taking one
shows **no line value**, the line cases no wind rows (as before).

## Results

At level 3 (every member with a cross-section and a joint with a `support`,
or a `model = "….ohlfe"`, or a pole the pole check can read) **▶ Run**
solves the cases of the load table with the finite-element model (and
the pole check, below): the case chosen, or every case. Rows without a
value are left out (and said); the case's strength factor and
self-weight factor come from its recipe (1 for `model` and typed cases).
The run goes on in the background: the window stays usable, **Cancel**
stops it, and each case has at most 30 s and 200 iterations — a case
that does not solve says why (a mechanism, unstable under compression,
or the limit reached with the forces still out of balance).

For each case: the **governing** member (usage in % and what governs),
the **largest displacement**, **Σ loads** (the table's and the
self-weight) against **Σ reactions** with the out-of-balance, the
solver's iterations, the **reactions** per support and every member's
axial force and usage, most used first — a tension-only member marked
*slack*, or *bowed* when it carries its own buckling load (what governs
says `bowed: carries its buckling load 0.83 kN`; the notes list them, see
[Tension-only members](structure-analysis.md#tension-only-members)). In the view the members are
coloured by usage — red above 100 %, orange from 90 %, green below, grey
without a capacity —, the deformed shape is drawn **Deformed ×** larger
(set from the largest displacement when a run ends) and the reactions
as the loads on the foundations. Results of an earlier file or table are
marked; run again.

**Offset moments.** For each attachment point at an offset from its
joint (a rigid link), the case lists the moment r × F of its force in
**Offset moments**: the point, its joint, Mx, My, Mz and |M| in kN·m
(structure axes), and whether it is **applied** at the joint (a frame
member meets it) or **left out** — only trusses meet the joint, which
cannot carry a moment (the note says so). **Make it a frame joint**
beside a left-out moment makes the members meeting that joint frames
(one undo step; their cross-sections then need iy, iz and torsion): run
again to carry the moment. The same moment is written beside the rigid
link picked in the view.

**On an extension.** A structure with extensions has an
**Extension** choice in the panel's header (the same as the 3D view's,
Body › Extensions): the cases are solved on the structure standing on
it — the model the line uses for a placed structure on that extension:
the extension's piece added, the body joints it reaches without their
supports, legs lowered. Choosing another extension solves again; the
header says what was solved (*Solved as written*, *Solved on extension
+3 (the structure 3 m higher)*). An extension without a piece or legs
leaves the model as written (said in a note: standing higher changes no
force); a `model = "….ohlfe"` has no extensions and is solved as written
(said too). A case's **pressure on all members** acts on the
extension's own members as well (the load table lists the body as
written): pressure × the member's area × the load's factor, half to each
joint, the total said in the case's notes. The band wind of the line
(`[[wind.band]]`) rises with the extension in the same way ([Structure
loads](loads.md)); it is never in the model.

**Pole check.** A structure with a pole (`pole = "…"` or a `[pole]`
table the check can read) gets the line's pole check
([Structure analysis](structure-analysis.md)) in every case: the case's
point rows at the structure's points as written (on an extension the
pole and its points rise together, so the moments about the pole's own
ground line are the same; the piece below the feet is not checked), the
horizontal pressures of the case's surface loads on **all members** as
the wind pressure on the pole (times the pole's drag), the case's
strength factor, P-Delta when the pole gives its modulus. Rows on
joints or members are not on the pole (said). The case shows **Pole
check** with the usage, what governs (moment against capacity at a
height), the ground-line moment with its wind part, the P-Delta factor
and the wind used. A pole structure without members has the pole check
only.

**Usage per check.** Over the cases the panel lists each check —
*members (FE)* and *pole* — with its largest usage, the case governing
it and what governs; cases that did not solve are listed under it. Each
case's line shows its usage over its checks.

**Structure report…** (in the header and the Results menu, once the
cases of the file and table as they are have run) writes the results
as a printable HTML page — one file, no external resources, it prints
from any browser — and the members' results beside it as
`<name>.members.csv` (`case,member,axial_n,usage,governs,slack`). The
page holds the structure's id, name, kind, file, level, the extension
solved on and the cross-section library; the cross-sections and materials used
(area, E, f<sub>y</sub>, weight, members, length) and the pole; the load
cases (description, strength and self-weight factors, rows and rows
without a value, Σ loads, status); the usage per check with its
governing case; every member's largest usage with the case governing it
and its axial force there; the reactions per case; the pole check per
case; the notes and flags (incomplete line values, what was left out,
out of balance); the software and its version, the date, and the
warning *Pre-alpha software: results must be verified by a qualified
engineer.*

The same report from the command line, with the tool
`ohl-structure-report` (a tool of its own, as `ohl-structure-export`:
the `ohl` command line never depends on the structure application):

```bash
ohl-structure-report tower.ohls -o tower-report.html
ohl-structure-report tower.ohls --sections libraries/sections \
    --loads tower-loads.csv --extension +3 --case wind -o tower-report.html --date
```

`--sections` (default: the `Sections` folder beside the file), `--loads`
a load table CSV (default: the file's own recipes without line values),
`--extension`, `--case` (default: every case), `--date` (default: no
date, so that reports of the same file are identical). It prints the
usage per check and writes `<name>.members.csv` beside the page.

**From `ohl`.** The command line asks this tool for a structure's
results — the applications talk, the crates stay apart:

```bash
ohl structure analyse tower.ohls --sections libraries/sections --extension +3
ohl structure report tower.ohls --case wind -o tower-report.html
ohl structure analyse tower.ohls --json     # the tool's JSON reply as it is
```

`ohl` starts `ohl-structure-report --request -` — the program
`OHL_STRUCTURE_REPORT` names, else the one next to `ohl` (or in the
folder above), else on the `PATH`, as OHL-Design finds OHL-Structure —,
writes the request (JSON) on its input and reads the reply (JSON) on its
output: per check its largest usage and governing case, per case the
usage of the members and the pole with what governs and the notes, and
for a report the files written
([data model](../development/data-model.md#structure-request-and-reply-json)).
Not found, `ohl` says where it looked and how to build it (`cargo build
--release -p ohl-cli -p ohl-structure-export`). The tool gets
`--timeout` seconds (default 300). The tool answers such a request on
its own too: `ohl-structure-report --request request.json` (or `-` for
the input).

## Undo, saving, closing

Every change is one edit of the file, undone with **⌘Z** (Ctrl+Z) or
**Undo** and made again with **Shift+⌘Z** (Shift+Ctrl+Z) or **Redo**; the Edit
menu names the edit. A value dragged in a table or the Inspector, or a
point dragged in the view with the Move tool, is one step. Comments and the order of the file are kept.

**Save** (⌘S, Ctrl+S) writes the file (and the typed loads beside it); **Save as…** (Shift+⌘S, Shift+Ctrl+S) another
one, which is then the file edited. A file that does not read (the error
is shown in red) is not saved. Closing the window with unsaved changes
asks first: **Save and close**, **Close without saving** or **Cancel** (File › Quit, ⌘Q, the same). **File › Close** closes the structure and shows the start page again; with unsaved changes it is greyed out (save or undo them first).

## Libraries

A structure reads these libraries — each, but the structure templates, where
the line application and `ohl line` keep them for a structure's folder:

| Library | Files | Read from (and imported into) |
|---|---|---|
| Cross-section libraries | `.ohlsec` | `--sections` (a folder), else `Sections/` beside the structure file |
| Bolt libraries | `.ohlb` | `Bolts/` beside the structure file (2026-11-02; [format](structure-library.md)); started from a project, the project's bolt libraries (library kind `bolts`), written there and kept in the project when imported there |
| Insulator assemblies | `.ohli` | `--insulators`, else the structure's folder and its `Insulators/` (imported into `Insulators/`) |
| Poles | `.ohlp` | beside the structure file |
| Structure models | `.ohlfe` | beside the structure file |
| Load-case recipes | `.ohlslc` | beside the structure file |
| Structure templates | `.ohlpt` | `StructureTemplates/` beside the structure file (`PylonTemplates/`, the name before 2026-10-21, still read) |

The structure templates are kept apart in the two applications: OHL-Structure
reads and imports them in `StructureTemplates/` beside the structure file;
the line application's structure builder reads the project's own in a
`Structure templates` folder beside the line. **New structure…** offers only
the templates shipped with OpenOHL (or another template file chosen); a
template chosen gives the id and the name (`<template>-suspension`, its
name), unless typed over. *Empty* asks the kind (lattice tower, pole,
portal, other). The new structure opens on its next step: a lattice
tower's Generators, a pole's outline. **Copy the example cross-sections
and insulators beside it** (ticked by default) puts the shipped examples
into `Sections/` and `Insulators/` beside the new file (a file there is
kept), so the cross-section lists are not empty to start with.

**File › Import ›** and **File › Export ›** (2026-11-03: before, File ›
Libraries):

- **Import › *library kind* ›** *A file…* or *The files of a folder…*: every
  file is read and checked first, then copied into the folder of the
  table (a file of the same name is replaced). A file that does not read
  — a typo, a wrong unit — or a cross-section or assembly whose id another file
  of that folder has already stops the import with nothing copied; the
  reasons are shown in a window (they scroll in a box of fixed height).
  The libraries are read again at once (the load panels' member areas,
  the strings' assemblies, the pole's cross-sections). Started from a project
  (**Open in OHL-Structure…** in the line application), the folders are the project's
  working copy: the line application notices the new files within half
  a second, stores them into the project and reads its libraries again.
  Load-case recipes and structure templates are not project libraries: they
  stay in the working copy and are not stored into the project.
- **Export › Libraries it uses…**: into a folder chosen, in the line
  application's layout — `structures/` (the structure file as edited,
  its pole and model), `sections/` (each cross-section library with only the
  cross-sections the structure names and the materials they use, comments
  kept), `insulators/` (its assemblies), `structure-templates/` (the template
  it was built from). **Export › Every library…** writes every file read.
- **View › Libraries**: the **Libraries** panel lists per
  kind the files read and where from, each with its ids (a cross-section
  library's cross-sections, with how many the structure uses), why a file does
  not read, and at the top what the structure **names that no library
  has**: cross-section ids (members, extension members, the pole) not defined
  in the file's own `[[section]]` tables nor in a library, insulator
  assemblies, the pole file, the structure model. A recipes file has
  **Take its cases** (into the structure file, one undo step). **⟳
  Reload** reads the folders again.

## Plugins

**File › Plugins › Plugin manager…** (also on the start page) lists the plugins
as the line application's does — the plugins installed for you (in the
line application's data folder, `plugins/` — still the folder named "OLS
CAD", kept from before the rename so nothing is lost) and those of the folders added — each with its version,
tier, extension points, **what it does in OHL-Structure** (greyed with
the reason where nothing applies), its status (or why it cannot be used:
another interface version, its entry file not built…), **Details** (what
it describes, loaded in each of its worlds; in a box that scrolls) and,
for an installed one, **Remove**. **Install from a folder…**, **Add a
plugin folder…**, **Reload**. The tick turns a plugin on or off.

The folders added and the plugins turned off are **shared with the line
application** (`settings.toml` in the plugin folder): a plugin added or
turned off in one is so in the other (the other reads it when its plugin
manager is opened, or with Reload).

What applies here:

| Extension point | In OHL-Structure |
|---|---|
| `design-code` | **Loads › Load cases from a design code…**: choose the plugin and your parameter file, **▶ Generate** (on a thread; the plugin's time and memory limits apply), read its notes, **Take them as recipes**: one recipe per load case of the code taking the line's factored values in that case at every point (as *Take the line's load cases*; one undo step). Ticked, the weather and load case files it generated are written beside the structure (`<plugin>.ohlwc`, `<plugin>.ohllc`) to give to the line. |
| `check` | listed: member and joint code checks will run here; nothing to run yet |
| `io` | file formats of the line: they take the line's export model, not a structure — listed greyed under File › Export |
| `terrain`, `project`, Python scripts | the line application's |

## Export

**File › Export ›** (each format's entry ends in "…": it asks where) writes the structure as it is in the window (unsaved
edits included) for another program. It needs **level 3**: joints,
members with cross-sections, a supported joint.

| Entry | File | Meant for |
|---|---|---|
| **IFC4 — analysis model with load cases, and members (analysis programs)** | `.ifc` | analysis programs reading the IFC structural analysis view (nodes and members), BIM viewers |
| **IFC4 — members with their profiles** | `.ifc` | BIM viewers, coordination |
| **IFC2x3 — members for steel-detailing programs** | `.ifc` | steel-detailing programs that take a reference model in IFC2x3 only |
| **SAF (xlsx) — analysis model with load cases (analysis programs)** | `.xlsx` | analysis programs reading the Structural Analysis Format |
| **CSV tables** | a folder | anything else: `materials`, `sections`, `nodes`, `supports`, `members`, `load_cases`, `loads`; with joint details `connections` and `plates` |

Pre-alpha, in active development: the exports are checked with free tools (IfcOpenShell's IFC validation, a reader of the SAF workbook; [Structure exports › Checks with free tools](../development/structure-exports.md#checks-with-free-tools)), but no exported file has been imported into another program yet. Sample files to try in other programs are in `examples/exports/`.

File-format plugins turned on are listed under the entries, greyed: they
write the line's export model, not a structure.

Members carry their cross-sections as real profiles (angles, channels, I
cross-sections, tubes, round bars, polygonal tubes) when the cross-section comes from
the [cross-section library](section-library.md); a cross-section given only by its
values (`[[section]]`) has no shape (IFC: the member's axis only; SAF: a
square of the same area, its values given).

A tension-only member that bends (`bending = true`) is written as a
frame — no format here has a member that is both — with its tension-only
behaviour as information (IFC: the property set `OHL_Member` and the
analysis member's description; SAF: the member's layer; CSV: the
`bending` column); set it up as tension-only in the other program if it
must drop out there. A member not design-checked (`check = false`) is
written as any other, flagged the same way (`Check` false, "not
checked", the `check` column). See
[Structure exports](../development/structure-exports.md#tension-only-members-that-bend-and-members-not-checked).

**Bolts and gusset plates** (a tower with [joint details](#joint-details-towers):
a file with `[[connection]]` or `[[plate]]`): both IFC files carry
them, the IFC2x3 one too. Per joint an assembly named after the joint
holds its bolts (each with its diameter, its length under the head, a
head, a shank and a nut; property set `OHL_Fastener`: joint, member,
end, grip, hole, grade, library bolt) and its plates (the outline
extruded by the thickness; `OHL_Plate`, with `WeldedTo` for a plate of a welded plate). They are where the **Joint
view** draws them: a member bolted onto a leg or a plate is **seated** — its solid
moved across so that its leg lies on the leg it bolts onto, as in the
drawings — when its bolted ends agree on that shift; otherwise its solid
stays on its axis and its bolts move with it. The analysis model (IFC4)
always keeps the axes. A plate without an outline in the file lies
as the Joint view draws it (on the corner leg, around its bolts;
assumed). SAF has no bolts or
plates; CSV gives the connections and plates as the file does. A file
without connections and plates is exported exactly as before.

**File › Export › Export options…** (a window; 2026-11-03: before,
under the entries of the menu), until the program is started again:

- **Cross-section library**: by default the window's (`--sections`, else the
  `Sections` folder beside the file — the one the load panels use);
  **Choose…** takes another file.
- **Bolt library**: by default the window's (the `Bolts` folder beside the
  file, as the Detail view); **Choose…** takes another
  `.ohlb` file. A connection naming a bolt takes its diameter and its
  head's and nut's sizes from it (else the drawing proportions).
- **Loads**: by default the **Load table panel's table**: for a placed
  structure (opened with **Open in OHL-Structure…** from the line) the line's values at
  that place, and the values typed over the table; for a structure opened
  on its own the recipes without line values (loads needing a line value
  are left out; the message says how many). Its `[[load]]` tables are
  case `model`. **Choose…** takes a load table written as CSV
  instead ([Structure load cases](structure-load-cases.md)).

The message at the bottom of the window says what was written. What each format
holds and leaves out: [Structure exports](../development/structure-exports.md).

The same from the command line:

```bash
ohl-structure-export examples/synthetic-line/Structures/type-02.ohls --format saf -o type-02.xlsx
ohl-structure-export tower.ohls --format ifc --sections libraries/sections --bolts libraries/bolts --loads loads.csv -o tower.ifc
```

`--format` is `ifc`, `ifc-members`, `ifc2x3`, `saf` or `csv` (`-o` a
folder); `--sections` takes library files or folders (the first giving an
id wins); `--bolts` bolt library files or folders (default: the `Bolts`
folder beside the file, when there is one); `--loads` a load table (CSV; without it the file's own recipes
without line values); `--timestamp` writes the export
time into IFC headers (without it, exports of the same file are
identical byte for byte).
