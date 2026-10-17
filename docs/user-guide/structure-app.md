# The structure application

`ols-structure` edits one structure file (`.olss`) in a window of its
own, from level 1 (a stick figure: the points where insulators hang) to
level 3 (a full design: every member with its section, the supports).
See [Building structures](../development/structure-building.md) for
what the levels mean and what comes next.

```bash
ols-structure examples/synthetic-line/Structures/type-02.olss
```

Options:

| Option | What |
|---|---|
| `--insulators DIR` | the insulator assemblies strings may take; default: the file's folder and an `Insulators` folder there |
| `--ahead DEG` | which plan angle means ahead along the line, for the compass |
| `--sections FILE\|DIR` | the [section library](section-library.md) (a `.olssec` file or a folder of them) the members' areas come from; default: the `Sections` folder beside the file |
| `--structure ID` | the placed structure whose loads these are: values typed over the load table are kept for it (below) |
| `--line-values FILE` | the line's values at that placed structure (a line-values file, written by the line application); read again when it changes |

From the line application, **Edit…** on a structure (in its properties,
or on the Structure types page) starts it on the structure's file; saved
here, the line follows ([desktop application](desktop-app.md#the-pylon-workspace)).
From a placed structure's properties it also hands over that place's
loads ([below](#started-for-a-placed-structure)).

## Started for a placed structure

**Edit…** in a placed structure's properties (say T2, of type
`type-02`) starts the application with T2's loads:

```bash
ols-structure Structures/type-02.olss --structure T2 \
    --line-values Structures/type-02.T2.line-values.json --sections Sections
```

- The header says **loads of T2 in *line*** (a link to **In the line**).
- The line application computes the loads the wires and strings put on
  T2 in every load case of the line (Criteria › Load cases; as Checks ›
  Structure loads does, in the background) and writes them beside the
  structure file as `<name>.<structure>.line-values.json`. Until they are
  there the header says they are being computed; without a load case
  file it says so. When the line changes while the application is open
  (and stays unchanged for a second), the line application computes them
  again and rewrites the file; the application reads it again within
  half a second and evaluates the recipes with the new values.
- The **load table** takes the line's values: rows that take one are
  marked `line` instead of **no line value**.
- **In the line** lists the line's values per point (with its sets and
  phases) and load case: transverse, longitudinal, vertical, unfactored
  and factored by the line's load case, in kN. **Other places of this
  structure** lists the other placed structures of the same type with
  the governing (largest) factored value per point and direction and its
  case — for comparison only: each place keeps its own loads.
- **Typed values** are kept for T2, not for the type: **Save** writes
  them as `<name>.T2.loads.csv` (for `type-02.olss`:
  `type-02.T2.loads.csv`). In a project, the line application keeps that
  file in the project with T2 (project library kind `loads`, per project
  line), so it is there the next time T2 is opened. Opened alone, the
  structure keeps using `<name>.loads.csv`.

The structure file itself is the type's: saved here, every structure of
the type follows in the line.

## The window

- **Header**: **Level N — name** (worked out from the file, never
  stated), the structure's id and name, its file, *\* unsaved*. Under it,
  **for level N + 1:** what the next level still lacks — no member yet,
  a member without a section (one item per member), no `[[section]]`, no
  supported joint. Each item is a link: it opens the tab and table where
  it is done and picks its row. **advice (n)** (folded) lists what reads
  but is worth a look, never counted in the level: a point on no joint
  or member (it carries no load in an analysis), a joint on one member
  or none, two members crossing without a joint (X-bracing bolted at the
  crossing is a common case).
- **3D view** on top, in the structure's own axes: x to the right of the
  line (red), y ahead (green), z up (blue), 0;0 at the centre, the ground
  grid at the level of the feet and a compass. A click picks a joint,
  point, wire point or member, Shift + click picks another; a drag from a
  joint or point moves it (snapped to the grid set, or onto a joint) — one
  undo step per drag; elsewhere a drag turns the view, a right-drag pans,
  the wheel zooms. **Fit** shows the whole structure.
- **Tabs** underneath:

| Tab | Tables | Tools |
|---|---|---|
| **Geometry** (level 1) | Attachment points, Insulators (top, wire point, kind, assembly, length, angle, direction, weight, wind area, compression, description), Wire points (counterweight), Set points, Structure (name, height, feet, height extensions) | **+ Point**, **I** and **V** presets for the points picked, **Delete…** (lists what goes with it first) |
| **Body** (level 2) | Joints (x, y, z), Members (ends, length, group, section, made by; a group filter), Generators | **Add joint** (above the joint picked), **Member** between two joints picked, **Point here** on a joint, **Delete…**; **symmetry**, **Make symmetric**, **group** of the members picked ([below](#the-body-tab)) |
| **Design** (level 3) | Supports (**Support the feet** pins the lowest joints), Members (section, kind, buckling factor k), Groups (a section and kind per member group), Sections and Materials (their fields typed with units, e.g. `1500 mm2`) | — |
| **Load cases** (any level) | **Recipes**: the structure's load cases (name, description, line case, strength, self-weight) and their loads (what, value, pressure, line case, factor, one-sided share, side, direction, points, sets, phases, members, note). **Load table**: the recipes evaluated, per case and point, member or joint | **+ Case**, **+ Load**, 🗑; **Import / Export recipes…** (`.olsslc` or CSV), **Import / Export table…** (CSV); the case shown drawn as arrows (**m per kN**) |
| **Results** (level 3) | Per case: the governing member, the largest displacement, Σ loads and Σ reactions, reactions per support, members with axial force and usage | **▶ Run** (one case or every case), **Cancel**; deformed shape (**Deformed ×**), usage colours, reactions in the view |
| **In the line** | Started for a placed structure: the line's values per point and case, the other places of the structure with their governing values ([above](#started-for-a-placed-structure)); opened alone: what it would show | — |

## The Body tab

The body is built from a few dimensions with **generators**, then
completed by hand; the file keeps each generator's inputs, so it can be
built again ([format](structure-library.md#body-generators-groups-and-symmetry)).

- **Generators** (a table of the Body tab): **New** › *legs and panels*,
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
    adds one on top. A new body goes up to the lowest point of the
    structure (the waist under the crossarms), in four X panels.
  - *Crossarms*: the points ticked each get an arm from the body face on
    their side, triangular (four chords to the point) or rectangular (a
    tip at the point and one above it); the point is put on the arm's
    tip, so it carries its loads in the analysis.
  - *Peak*: a tip above the top of the body, or one at each earth-wire
    point ticked, joined to the joints at the top.
  - *Copy a face*: the members on one face of a legs-and-panels body (a
    redundant added by hand, say) copied to the opposite face, without
    doubling those there already.
- **Rebuild** replaces only what the generator made. What was set on its
  joints and members (a section, a kind, a group, a support) stays when
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
  `body-bracing-3`, `arm1-top`…). In **Design › Groups** a section and a
  kind are chosen per group and written on each of its members; a member
  joining a group whose members share a section takes it.

## Load cases

The **Recipes** part edits the structure's `[[load_case]]` tables
([Structure load cases](structure-load-cases.md)): pick the case under
**Recipe**, type its fields, and one row per load (`what`, `side` and
`direction` are chosen from a list; a value takes its unit, `2 kN` or
`0.6 kPa`, a plain number is N or Pa; lists are separated by `;`). A
field is written when it is left (Enter, Tab or a click elsewhere), each
change one undo step. When the recipes do not read, the reason is shown
in red and the table below keeps the cases that read.

**Import recipes…** takes the cases of a `.olsslc` or CSV file: a case
with the name of one in the structure replaces it, the others are added
(one undo step). **Export recipes…** writes them as `.olsslc` or CSV.

The **Load table** part shows the recipes evaluated: per case (or every
case) and per point, member or joint, the transverse, longitudinal and
vertical load in kN, where it comes from (`line`, `recipe`, `typed`),
the member's **area** a surface load acts on, and why a row has no
value. Opened on its own, the structure has no
line: every load taking a line value shows **no line value**. Type the
values (drag or click a number), or **Import table…** a CSV with the
table's columns (from **Export table…**, another program or a client's
load tree): what it has beyond the evaluation becomes typed values.

**Typed values** stay when the recipes change and the table is
evaluated again; ⟲ takes one back. They are not part of the structure
file: **Save** writes them beside it as `<name>.loads.csv` (for
`type-02.olss`: `type-02.loads.csv`, the table's CSV with the typed rows
only), and opening the file reads them again; started for a placed
structure, as `<name>.<structure>.loads.csv`. Unsaved typed values count
as unsaved changes.

**Surface loads** (a `pressure` on `members`) take each member's area
from the section library (`--sections`, else the `Sections` folder
beside the file; the line application passes the project's): the
profile's width seen along the pressure's direction (in the member's
cross-section plane) × the member's length × sin θ, θ the angle between
the direction and the member — for a member across the direction,
width × length. The row shows the area used. A member whose section is
not in the library, or a structure whose model does not build, has **no
area** and says why. The line under the buttons says how many sections
were read and from where.

**Arrows** draws the loads of the case shown in the view, ending at
their point (a member's at its middle), **m per kN** long.

## Results

At level 3 (every member with a section and a support, or a `model =
"….olsfe"`) **▶ Run** solves the cases of the load table with the
finite-element model: the case chosen, or every case. Rows without a
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
axial force and usage, most used first. In the view the members are
coloured by usage — red above 100 %, orange from 90 %, green below, grey
without a capacity —, the deformed shape is drawn **Deformed ×** larger
(set from the largest displacement when a run ends) and the reactions
as the loads on the foundations. Results of an earlier file or table are
marked; run again.

## Undo, saving, closing

Every change is one edit of the file, undone with **Ctrl+Z** (⌘Z) or
**⟲ Undo** and made again with **Ctrl+Shift+Z** or **⟳ Redo**; the Edit
menu names the edit. A value dragged in a table, or a point dragged in
the view, is one step. Comments and the order of the file are kept.

**Save** (Ctrl+S) writes the file (and the typed loads beside it); **Save as…** (Ctrl+Shift+S) another
one, which is then the file edited. A file that does not read (the error
is shown in red) is not saved. Closing the window with unsaved changes
asks first: **Save and close**, **Close without saving** or **Cancel**.
