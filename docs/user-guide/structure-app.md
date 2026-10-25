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

Its former name `ols-structure` is still built for a transition period:
it says the program is now called `ohl-structure` and runs it.

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
the files opened recently (a click opens one; kept between runs), **New
structure…** and **Plugins…** (the [plugin manager](#plugins)). **New
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
`OHL_STRUCTURE_APP` environment variable names (or, by its former name,
`OLS_STRUCTURE_APP`), then next to `ohl-design`, then on the `PATH`; not
found there, under its former name `ols-structure`. Before it starts it, it runs `ohl-structure
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

- The **Level** panel says **loads of T2 in *line*** (a link to the
  **Line values** panel).
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
- A **status block** at the top of the Load recipes, Load table and Line
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
  with *incomplete: S12 not solved*, Line values marks them in its
  **complete** column, and **FE results** of the case read *not checked —
  incomplete: S12 not solved*, the usage of the loads there are shown
  only in brackets); opened
  alone, *No line values: open this from a placed structure (select it
  in the line, Properties › Open in OHL-Structure…) to get its forces*, with the cases
  stored in the file from the line, if any.
- The **load table** starts with the **line's forces**, without any
  recipe: every case the line transmits × every attachment point, the
  transverse, longitudinal and vertical force as the line computed them
  (factored where the line's case has factors; the unfactored ones, the
  sets and the phases in the note), marked `line` (column **from**: *line
  load case*). The structure's recipes follow as cases of their own (they
  may still take line values); a line case with the name of a recipe is
  called *NAME (line)*. **FE results** can run the line's cases directly.
- **Store in the structure file…** (by the load table, or **Load cases ›
  Store the line's cases in the file…**): a tick per line case; **Store**
  writes the cases ticked into the structure file as `[[load_case]]`
  tables of given values — a `value` and `direction` per point and
  component, `line_case`, and a description saying the line, the place
  and the date — in one undo step. The file then has these forces
  without the line (exports, design elsewhere). Stored again, cases of
  the same names are replaced (asked first: **Replace and store**).
- **Take the line's load cases as they are** (Load cases menu, and in the
  Load recipes panel while the structure has no recipe) writes instead
  one recipe per line case taking the line's factored values at every
  point: the forces then follow the line.
- **Line values** lists, with each case's **source** (load case, or a
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

At the top, the **menus**: **File** (New structure…, Open, Save, Save
as, [Libraries ›](#libraries), Export ›, Close), **Edit** (Undo, Redo,
Clear selection — the same words as in OHL-Design), **View**, then one
menu per part of the work — **Geometry**, **Body**, **Design**, **Load
cases**, **Results**, **In the line** — then **Plugins**
([below](#plugins)) and **Help** (**User guide…**: the manual on this
computer, as in OHL-Design — the book built by `mdbook build`, else its
sources in `docs/`, nothing fetched from the internet —; **Start from
scratch (tutorial)…**: [the tutorial](../tutorial/index.md) of the line
application, your first line from the starter template; **About OHL-Structure**: version, licence);
**Undo**, **Redo** and **Save** at their right (the same place as
OHL-Design's quick bar). Shortcuts are written ⌘ on macOS, Ctrl
elsewhere. Each work menu lists
its **panels** (*Joints…*, *Generators…*, *Pole…*: opened if closed and
brought to the front) and its **actions** (Geometry: *Add a point*, *One
string below the point*, *Two strings to one wire point*, *Link tool*
([below](#attachment-points-joints-and-the-link-tool)), *Delete
what is selected…*; Body: *Add a joint*, *Member between the two joints*,
*Point on the joint*, *Symmetry* ›, *Make symmetric*, *Delete what is
selected…* — *Add a joint*, *Member…*, *Point on the joint*, *Symmetry*,
*Generators…* and *Supports…* have their icons, also on the panels'
buttons (Add joint, Member, Point here, symmetry), tinted with the text
colour; Load cases: *Store the line's cases in the file…*, *Take the
line's load cases as they are*, *Add a load case*, *Load cases from a
design code…* ([Plugins](#plugins)), *Import / Export load
recipes…*, *Import a load table…*, *Export the load table…*; Results:
*Run the cases*, *Cancel the run*, *Structure report…*; In the line: *Read the line's values
again*); an action brings its panel to the front.

Under the menus, the **panels**, docked as in the line application:
drag a panel's tab to move it (beside another, above, below, into
another group, or out as a window of its own), drag the borders to size
them, close one with its ×. **View** ticks each panel to show or hide it
(the menu stays open while ticking), **Rigid links** shows or hides the
rigid links in the 3D view, **Reset layout** puts every panel
back, and **Fit the view** shows the whole structure. The arrangement is
kept for the next start. By default the tables are on the **left** and
the **3D view on the right** (about 45 / 55): the Level panel at the top
left; under it three groups of a few tabs each, so that every tab's
title shows — the geometry and body panels, the design panels, then the
load and result panels. (An arrangement kept from before stays as it
was: **View › Reset layout** gives the new one.) The load recipes'
columns and fields are named in words (*Line case*, *Self weight*, *One
sided*); hovering one gives the key it has in the file
(`line_case`…). The last message (what an action did, or why not) is
at the bottom of the window.

| Menu | Panels |
|---|---|
| — | **Level**, **3D view**, **Libraries** (closed by default; View or File › Libraries) |
| **Geometry** (level 1) | **Points & strings** (Attachment points, Strings), **Wire points & sets** (Wire points, Sets), **Structure fields** |
| **Body** (level 2) | **Joints**, **Members**, **Generators**, **Pole**, **Extensions** |
| **Design** (level 3) | **Supports**, **Member cross-sections**, **Groups**, **Cross-sections & materials**, **Pole design** |
| **Load cases** | **Load recipes**, **Load table** |
| **Results** | **FE results** |
| **In the line** | **Line values** |

The level of the file decides the panel in front when it opens: Points &
strings at level 1, Joints at level 2 (the Pole for a pole), Supports
at level 3 (Pole design for a pole). Panels closed by default (Structure
fields, Pole, Extensions, Cross-sections & materials, Pole design) open from
their menu or from View.

- **Level** panel: **Level N — name** (worked out from the file, never
  stated), the structure's id and name, its file, *\* unsaved*. Under it,
  **for level N + 1:** what the next level still lacks — no member yet,
  a member without a cross-section (one item per member; more than three:
  one line, set per group in Design › Groups), no supported joint, an
  attachment point on no joint of the members (tick it in the peak or
  crossarm generator, or link it), a support joint without a support,
  and the same for each extension with a piece. A supported joint is one
  with a `support`: a structure standing only on `spring`s does not
  reach level 3 (this is how the level is worked out now). Each item is a link: it brings the panel and table
  where it is done to the front (opening it if it was closed) and picks
  its row. **advice (n)** (folded) lists what reads
  but is worth a look, never counted in the level: a point on no joint
  or member (it carries no load in an analysis), a joint on one member
  or none, two members crossing without a joint (X-bracing bolted at the
  crossing is a common case).
- **3D view**, in the structure's own axes: x to the right of the
  line (red), y ahead (green), z up (blue), 0;0 at the centre, the ground
  grid at the level of the feet and a compass. **Attach joints** are
  ringed in orange, **support joints** marked with a green triangle, and
  each attachment point at an offset from its joint is joined to it by
  its **rigid link**, a magenta rod (none at zero offset; View › Rigid
  links). A click picks a joint,
  point, wire point, rigid link (it picks its attachment point, and the
  view writes its joint, offset and length — and, after a run, its
  moment in the case shown) or member, Shift + click picks another; a
  right-click on an attachment point opens its menu (**Link to a
  joint…**, **Unlink**); a drag from a
  joint or point moves it (snapped to the grid set, or onto a joint) — one
  undo step per drag; elsewhere a drag turns the view, a right-drag pans,
  the wheel zooms. **Fit** (and View › Fit the view) shows the whole
  structure. The loads of the case shown (while a load panel was the last
  clicked) or the results (while FE results was) are drawn over the
  structure.
- **Panels of tables**:

| Menu | Tables | Tools |
|---|---|---|
| **Geometry** (level 1) | Attachment points (where it is — typed: a point on a joint stays on it, its offset follows —, its **joint**, its **offset** x, y, z from it — typed for a point on a joint, one undo step —, the offset's length, the strings hanging from it; a right-click on its name: **Link to a joint…**, **Unlink**), Strings (top, wire point, kind, assembly, length, angle, direction, weight, wind area, compression, limits — max load, max swing, across, along, each optional and not checked, an assembly's shown dimmed —, description), Wire points (counterweight), Sets, Structure (name, height, feet, extensions) | **+ Point**, **One string** and **Two strings to one wire point** presets for the points picked, **Link** (the Link tool), **Delete…** (everything picked, lists what goes with each first; one undo step) |
| **Body** (level 2) | Pole (pole structures: the outline by height, [below](#poles-and-extensions)), Joints (x, y, z), Members (ends, length, group, cross-section, made by; a group filter), Generators, Extensions (each one's piece and legs) — Joints: x, y, z, **kind** (*auto* — worked out: attach when a point is linked to it, support with a support —, **attach** or **support**; one undo step; a joint carrying a point cannot be made a support joint), the members meeting it, the points linked to it | **Add joint** (above the joint picked), **Member** between two joints picked, **Point here** on a joint (not on a support joint: strings hang from attach joints), **Delete…**; **symmetry**, **Make symmetric**, **group** of the members picked ([below](#the-body-panels)) |
| **Design** (level 3) | Pole (pole structures: material, cross-section, wall or solid, strength), Supports (**Support the feet** pins the lowest joints; the button is above the table, and in the Design menu), Members (a right-click on a member's name: **Insert a joint** at a fraction of its length — [below](#attachment-points-joints-and-the-link-tool); the analysis settings **stabilise joints** and the **dead-load factor** — see [Structure models](structure-analysis.md#structure-models-ohlfe) —, each member's cross-section, kind (truss, frame, cable or tension — a truss in tension only, carrying compression up to its own buckling load, see [Tension-only members](structure-analysis.md#tension-only-members); one undoable edit, which raises the file to version 5), buckling factor k, effective length lk, the legs of an angle, its own tension and compression capacities and dead-load factor (DLF) — values typed with units, written when the cell is left), Groups (a cross-section and kind per member group, and lk, the capacities and the dead-load factor for all its members), Cross-sections and Materials (their fields typed with units, e.g. `1500 mm2`), and **Cross-section library** (the cross-section library's cross-sections — new, edit, duplicate, delete, their properties computed as typed, each change one undo step; see [Typing cross-sections](section-library.md#typing-cross-sections-in-ohl-structure)); the cross-section lists of Members and Groups hold the library's cross-sections first, then the file's | — |
| **Load cases** (any level) | **Load recipes**: the structure's load cases (name, description, line case, strength, self-weight) and their loads (what, value, pressure, line case, factor, one-sided share, side, direction, points, sets, phases, members, note). **Load table**: the line's forces, then the recipes evaluated, per case and point, member or joint | **+ Case**, **+ Load** (with the line's values the line's vertical load at every point; opened on its own 1 kN down at every point, to change), 🗑; **Import / Export recipes…** (`.ohlslc` or CSV), **Import / Export table…** (CSV), **Store in the structure file…**; the case shown drawn as arrows (**m per kN**) |
| **Results** (level 3): **FE results** | The usage per check (members, pole) with the governing case; per case: the governing member, the largest displacement, Σ loads and Σ reactions, reactions per support, members with axial force and usage, the pole check | **▶ Run** (one case or every case), **Extension** (solved again when changed), **Cancel**, **Report…**; deformed shape (**Deformed ×**), usage colours, reactions in the view |
| **In the line**: **Line values** | Started for a placed structure: the line's values per point and case, the other places of the structure with their governing values ([above](#started-for-a-placed-structure)); opened alone: what it would show | — |

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

- **Link tool** (Geometry › Link tool, or **Link** on the Geometry and
  Body panels): click an attachment point, then an attach joint — the
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
  table): the point keeps its place as a joint of its own, on no member
  (its load then reaches no member: the header lists it until it is
  linked again).
- **The link follows edits** (2026-10-21): an attachment point on a
  joint that is moved — typed in the Attachment points table, or dragged
  in the view — stays on its joint, its offset recomputed (the new
  position less the joint's, in the structure's axes), one undo step;
  the joint does not move. A joint moved takes its points along at
  their offsets. **Offsets** can also be typed directly.
- **Insert a joint on a member** (a right-click on a member's name in
  the Members table of the Design panel): an attach joint at the
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
- **Symmetry** (none, x, y or both): with it on, a joint or member
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

- **Body › Pole…** (a structure of kind pole, or with a pole): the
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
- **Design › Pole design…**: the material (wood, steel, concrete, other), a
  **cross-section** of the cross-section library next to the file (a polygonal
  hollow cross-section with the pole's sides, or a circular hollow one for a
  round pole: its wall, and its material's elasticity and density), or
  the wall typed, or **solid** ticked (`wall = "solid"`: no hollow; it
  stands for the wall at level 3; unticked, the wall is not given again;
  each one undo step; a word other than `solid` typed as the wall is
  refused); fibre stress, moment capacity, elasticity, density,
  drag. Under them what level 3 still lacks, or *The pole check can read
  the pole (level 3)*.
- **Body › Extensions…**: the extensions (**+ Extension**: +3 m, +6 m…),
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

## Load cases

The **Load recipes** panel edits the structure's `[[load_case]]` tables
([Structure load cases](structure-load-cases.md)): pick the case under
**Recipe**, type its fields, and one row per load (`what`, `side` and
`direction` are chosen from a list; a value takes its unit, `2 kN` or
`0.6 kPa`, a plain number is N or Pa; lists are separated by `;`). A
field is written when it is left (Enter, Tab or a click elsewhere), each
change one undo step. When the recipes do not read, the reason is shown
in red and the table below keeps the cases that read.

**Import recipes…** takes the cases of a `.ohlslc` or CSV file: a case
with the name of one in the structure replaces it, the others are added
(one undo step). **Export recipes…** writes them as `.ohlslc` or CSV.

The **Load table** panel shows first the line's forces (started for a
placed structure, [above](#started-for-a-placed-structure)), then the
recipes evaluated: per case (or every case) and per point, member or
joint, the transverse, longitudinal and vertical load in kN, where the
case comes from (**from**: *line load case*, *recipe*, *typed*), where the
value comes from (`line`, `recipe`, `typed`), the member's **area** a
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
menu names the edit. A value dragged in a table, or a point dragged in
the view, is one step. Comments and the order of the file are kept.

**Save** (⌘S, Ctrl+S) writes the file (and the typed loads beside it); **Save as…** (Shift+⌘S, Shift+Ctrl+S) another
one, which is then the file edited. A file that does not read (the error
is shown in red) is not saved. Closing the window with unsaved changes
asks first: **Save and close**, **Close without saving** or **Cancel**.

## Libraries

A structure reads these libraries — each, but the structure templates, where
the line application and `ohl line` keep them for a structure's folder:

| Library | Files | Read from (and imported into) |
|---|---|---|
| Cross-section libraries | `.ohlsec` | `--sections` (a folder), else `Sections/` beside the structure file |
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

**File › Libraries ›**:

- **Import *library* ›** *A file…* or *The files of a folder…*: every
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
- **Export what it uses…**: into a folder chosen, in the line
  application's layout — `structures/` (the structure file as edited,
  its pole and model), `sections/` (each cross-section library with only the
  cross-sections the structure names and the materials they use, comments
  kept), `insulators/` (its assemblies), `structure-templates/` (the template
  it was built from). **Export every library…** writes every file read.
- **Libraries panel…**: the **Libraries** panel (also in View) lists per
  kind the files read and where from, each with its ids (a cross-section
  library's cross-sections, with how many the structure uses), why a file does
  not read, and at the top what the structure **names that no library
  has**: cross-section ids (members, extension members, the pole) not defined
  in the file's own `[[section]]` tables nor in a library, insulator
  assemblies, the pole file, the structure model. A recipes file has
  **Take its cases** (into the structure file, one undo step). **⟳
  Reload** reads the folders again.

## Plugins

**Plugins › Plugin manager…** (also on the start page) lists the plugins
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
| `design-code` | **Load cases › Load cases from a design code…** (also in the Plugins menu): choose the plugin and your parameter file, **▶ Generate** (on a thread; the plugin's time and memory limits apply), read its notes, **Take them as recipes**: one recipe per load case of the code taking the line's factored values in that case at every point (as *Take the line's load cases as they are*; one undo step). Ticked, the weather and load case files it generated are written beside the structure (`<plugin>.ohlwc`, `<plugin>.ohllc`) to give to the line. |
| `check` | listed: member and joint code checks will run here; nothing to run yet |
| `io` | file formats of the line: they take the line's export model, not a structure — listed greyed under File › Export |
| `terrain`, `project`, Python scripts | the line application's |

## Export

**File › Export** writes the structure as it is in the window (unsaved
edits included) for another program. It needs **level 3**: joints,
members with cross-sections, a supported joint.

| Entry | File | Meant for |
|---|---|---|
| **IFC4 — analysis model with load cases, and members (analysis programs)** | `.ifc` | analysis programs reading the IFC structural analysis view (nodes and members), BIM viewers |
| **IFC4 — members with their profiles** | `.ifc` | BIM viewers, coordination |
| **IFC2x3 — members for steel-detailing programs** | `.ifc` | steel-detailing programs that take a reference model in IFC2x3 only |
| **SAF (xlsx) — analysis model with load cases (analysis programs)** | `.xlsx` | analysis programs reading the Structural Analysis Format |
| **CSV tables** | a folder | anything else: `materials`, `sections`, `nodes`, `supports`, `members`, `load_cases`, `loads` |

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

Under the entries:

- **Cross-section library**: by default the window's (`--sections`, else the
  `Sections` folder beside the file — the one the load panels use);
  **Choose library…** takes another file.
- **Loads**: by default the **Load table panel's table**: for a placed
  structure (opened with **Open in OHL-Structure…** from the line) the line's values at
  that place, and the values typed over the table; for a structure opened
  on its own the recipes without line values (loads needing a line value
  are left out; the message says how many). Its `[[load]]` tables are
  case `model`. **Choose load table…** takes a load table written as CSV
  instead ([Structure load cases](structure-load-cases.md)).

The message at the bottom of the window says what was written. What each format
holds and leaves out: [Structure exports](../development/structure-exports.md).

The same from the command line:

```bash
ohl-structure-export examples/synthetic-line/Structures/type-02.ohls --format saf -o type-02.xlsx
ohl-structure-export tower.ohls --format ifc --sections libraries/sections --loads loads.csv -o tower.ifc
```

`--format` is `ifc`, `ifc-members`, `ifc2x3`, `saf` or `csv` (`-o` a
folder); `--sections` takes library files or folders (the first giving an
id wins); `--loads` a load table (CSV; without it the file's own recipes
without line values); `--timestamp` writes the export
time into IFC headers (without it, exports of the same file are
identical byte for byte).
