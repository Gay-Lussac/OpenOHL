# Tools: Structures

*Built, in active development (not approved yet).* The six tools of the
**Structures** theme place and change structures: **Add structure**
stands a new one on the line, **Move structure** moves one, **Replace
structure** puts another structure type in its place, **Link attachment
point** links an attachment point to a joint of its type, **Add
string** hangs a new string on a structure (or puts a spacer between two
wires), and **Add post insulator** stands a post insulator on it. This page explains each tool, its windows and every field. The
words structure, structure type, attachment point, string, wire point and
set are those of the [Glossary](glossary.md#structures). The tutorial
places structures and looks at their strings in [3. The first
structures](../tutorial/03-structures.md) and [4. Strings on the
structures](../tutorial/04-strings.md).

## Add structure

![The Add structure window: id, type, where it stands, its orientation, and a 3D preview with a compass](../images/tools/tool-add-structure.png)

**What it is for.** Add structure stands a new structure where you
click: on an alignment point (tied to it, turned along the line) or on
the ground. Its type brings its attachment points, strings and wire
points with it.

**How to use it**

1. Hold **Add structure** (slot 6 of the default hotbar).
2. Click an **alignment point** without a structure, or the **ground**,
   in the 3D view or a cross-section (or right-click an alignment point:
   **Add structure here…**). The **Add structure** window opens.
3. Choose its **type**, check where it stands, then **Apply** (Enter).

**The window**

| Field | What it does |
|---|---|
| **id** | Its name; the next after the line's last structure is offered (`T7` → `T8`). |
| **type** (?) | A structure type of the library: the button opens the [structure picker](desktop-app.md#structure-picker), where the extension can be chosen too. Offered at first: the type of the nearest structure. *(none: points given here)*: a structure without a type, whose attachment points you give below and whose strings you add with Add string. |
| **extension** | *(base)* or one of the type's extensions (shown when the type has some). |
| **height adjustment** | How far the type's feet stand above the ground, m (foundations; may be negative). |
| **description** | Free text. |
| **soil resistivity** | **measured**: the soil resistivity at this structure, Ω·m (100 Ω·m when first ticked). The line parameters take on each span the geometric mean of its two structures' values. Unticked: not measured (the line's value is used). |
| **earthing resistance** | **measured**: the resistance of its earthing to remote earth, Ω (10 Ω when first ticked; used by Magnetic field, earth wires through the structures). Unticked: the line's value, else 10 Ω assumed. |
| **routes** (?) | The routes it is on. Standing on an alignment (on a point, or within 5 m of one): that alignment's routes, by itself. Away from every alignment (a crossing line): tick its route(s) here, or **+ New route…**. |

**Where it stands**

- **on an alignment point**: chosen in the list of the alignments'
  points (`PX — alignment #3 — E …, N …`; a point under another
  structure is greyed). Clicked on an alignment point, or within 5 m of
  a free one, it is chosen for you. The structure stands on the point,
  tied to it: the alignment follows the structure when it moves.
  **base on the ground** (ticked: on the terrain there; unticked: at the
  point's own height).
- **at coordinates**: **E**, **N**, **z** (m), and **base on the
  ground** (z then follows the terrain).
- **Also on other points of this alignment** (folded; new structures on
  an alignment point only): the alignment's other points, each with a
  tick (**All**, **None**; a point under a structure already is left
  out) and a **Type** — *(the type above)* or another (say, a strain
  type at the angle points). Every point ticked gets a structure too,
  in the same step, each tied to its point and turned along the line
  there.

**Orientation** (?)

| Field | What it does |
|---|---|
| **along the line** | Ticked: it faces ahead along the alignment, its cross-arms on the bisector of the line angle there. Ticked by itself where there is a line. |
| (degrees) | Typed when *along the line* is unticked: the direction of its right (its local x axis, the cross-arms), counter-clockwise from east. |

**The preview** on the right shows the structure in its own axes with a
compass on the ground — the angles as the project counts them, ahead
(green), right (red) and north (blue). Drag turns it, right-drag pans,
the wheel zooms, a double-click fits it.

**A structure without a type** also has **Points where strings hang**
(?): each attachment point's label, its place **across** (to the right
looking ahead), **along** (ahead) and **up** (from its base), m, a
description, **×** to take it away (not while strings hang from it);
**+ point** adds one.

**Good to know**

- With no structure type in the library the window says so: without a
  type a structure holds no wire until strings are added by hand.
  **New structure from a template…** opens that window (this one stays
  open).
- With no ground under the click, the window opens on the first free
  alignment point.
- Everything can be changed later: the strings with [Edit](tools-general.md#edit-on-a-structure),
  the placement in **Properties › Placement…**.
- **Undo**: the structure (and those on the other points ticked, and a
  new route) is one step.

## Move structure

![The Move structure window: E, N, z with how far each moved, base on the ground, Apply and Cancel](../images/tools/tool-move.png)

**What it is for.** Move structure moves a structure to coordinates you
type, showing it there live before you apply.

**How to use it**

1. Press **M** (or hold the tool, slot 2 of the default hotbar).
2. Click a structure in the 3D view or a cross-section (or right-click
   it anywhere: **Move…**). The **Move structure *id*** window opens.
3. Type where it goes. The views show it there at once and its wires
   are solved again as you type.
4. **Apply** (Enter) keeps it; **Cancel** (Escape) puts it back.

**The window**

| Field | What it does |
|---|---|
| **E (m)**, **N (m)** | The project's coordinates of its base, east and north; beside each, how far it moved (Δ). |
| **z (m)** | The height of its base (typed only when *base on the ground* is unticked). |
| **base on the ground** | Ticked: the base stays on the terrain wherever it goes. Unticked at first. |
| moved … horizontally | How far it moved in plan, m. |

**Good to know**

- A structure being moved by another user on the server cannot be moved
  (the status bar says by whom).
- A structure tied to an alignment point takes the point along.
- With a structure's screen open, Move structure held also drags its
  points in the 3D view: see [The structure screen](desktop-app.md#the-structure-screen).
- **Undo**: the move is one step; Edit › Undo (⌘Z) takes it back.

## Replace structure

![The Replace structure window: the old wire points with the new set for each, the attachment points and what each becomes, and what is removed](../images/tools/tool-replace.png)

**What it is for.** Replace structure puts another structure type in a
structure's place: it keeps its id, place, orientation and height
adjustment, and its wires are moved to the new type's wire points.

**How to use it**

1. Hold **Replace structure** (from the inventory or Edit › Tools ›
   Structures; or right-click a structure: **Replace structure…**; or
   **Replace…** in its Properties).
2. Click a structure. The structure picker opens on its current type,
   titled **Replace structure *id* — the new structure type**.
3. Choose the new type (and an extension), then **Choose** (**Cancel**
   changes nothing).
4. If every wire point where something ends is also on the new type,
   nothing else goes and every attachment point has a joint at its very
   place, the structure is replaced at once (the status bar says how many
   wires were rewired). Otherwise the **Replace structure *id* by
   *type*** window opens.

**The window**

The wire points of the old structure, one row each:

| Column | What it shows |
|---|---|
| **set**, **label** | The old wire point. |
| **side**, **height** | Left, right or middle looking ahead; its height, m. |
| **ends here** | The sections, jumpers and conductor-path starts that end there. |
| **new set** | Where they go on the new type: one of its wire points (proposed: the same set, else the one on the same side in the same height order), or **none (removed)**. |

**Attachment points (*n*)**, one row per old attachment point, in a
scroll box:

| Column | What it shows |
|---|---|
| **point**, **side**, **height**, **strings** | The old point and the strings hanging from it. |
| **match** | **exact** (a joint of the new type at its very place: taken), else the **nearest** attach joint, its distance and moment arm, or *attach joints equally near: choose*, or *no attach joint*. |
| **becomes** | Your choice: the joint offered (the point stays where it is, the distance kept as its offset), another attach joint of the new type, the new type's own point of the same label, or **remove** (with what hangs only from it). |

Under them, **Removed or dropped with it**: everything that would go —
sections, the jumpers at them or on taps on them, their spacers, the old
structure's own points, strings and wire points, jumper clamps on wire
points the new type lacks. The sections and jumpers removed are drawn
**red** in the 3D view while the window is open.

**Replace** applies it; it waits until every attachment point has a
choice (nothing is dropped silently). **Cancel**, **Escape** or the
window's × change nothing. The window takes no Enter.

**Good to know**

- The extension is kept when the new type has one of the same name;
  otherwise the structure is placed without one, and the window (or the
  status bar) says so.
- Two old wire points onto one new one, or a set the new type lacks, is
  refused with the reason.
- **Undo**: the replacement and everything removed with it are one
  step.

## Link attachment point

**What it is for.** Link attachment point links an attachment point
(where a string hangs) to a **joint** of its structure's type, where the
point is: its distance from the joint is kept as its **offset**, a rigid
link whose moment the structure analysis applies at the joint
([Glossary](glossary.md#structures)).

**How to use it**

1. Hold **Link attachment point** (from the inventory or Edit › Tools ›
   Structures; or right-click an attachment point: **Link to a
   joint…**). While it is held, attachment points can be clicked.
2. Click an **attachment point** of a structure with a type. The status
   bar says how many joints it may be linked to, and its type's **attach
   joints** are ringed in the 3D view.
3. Move the pointer over a ringed joint: the offset (x, y, z in the
   structure's axes), its length and its moment arm (its length across
   the vertical) are written beside it.
4. Click the ringed joint: the point is linked there, where it is.

**Keys.** **Escape**, or a click elsewhere, cancels the point chosen.

**Good to know**

- Only **attach joints** take a link. Another joint under the pointer
  says *not an attach joint: set its kind to attach in OHL-Structure*.
- A structure without a type has no joints (its points are their own);
  a wire point is not linked (it hangs from its string).
- Relinking a linked point is the same two clicks. **Unlink** is in the
  point's Properties; an unlinked point is drawn red and its structure's
  loads are refused until it is linked again.
- A linked point moved later keeps its joint; its offset follows.
- **Undo**: one step.

## Add string

![The Add strings window: the set and shape, the new string with what it hangs from, its length and direction, and its wire point](../images/tools/tool-add-string.png)

**What it is for.** Add string hangs a new string on a structure — from
an attachment point, or below a wire point. Clicking two span points it
puts a **spacer** between two wires instead.

**How to use it**

1. Hold **Add string** (slot 7 of the default hotbar). While it is held,
   attachment points, wire points, strings and span points can be
   clicked.
2. Click:

   | Clicked | What opens |
   |---|---|
   | an attachment point | **Add strings to *structure***: one string hanging from it, its wire point 3 m straight down, at the next free set |
   | a wire point | its strings' window, with a new string hanging below it at the next free set (two tension strings from a string's wire point are made this way) |
   | a structure, or one of its strings | **Add strings to *structure***: one string from the first attachment point no string hangs from yet (a structure without points: a new point 20 m up), at the next free set |
   | a span point, then a span point of another wire in the same span | **Add spacer** (below) |

3. Set the string, then **Apply** (Enter).

**The window** is the strings window explained under [Edit on a
string](tools-general.md#edit-on-a-string), with two fields more at its
top for a new string:

| Field | What it does |
|---|---|
| **set** | The set number of its wire point (the next free one is offered). Apply is refused when the structure has a wire point with this set already. |
| **shape** | **replace with…**: replaces the strings by a shape — *One string*, *Two strings to one wire point*, *Two strings to one wire point, a string below*, *A string, two tension strings from its wire point*, and the three post insulators ([Add post insulator](#add-post-insulator)). |

**Add spacer.** Click a span point, then a span point of another wire of
the same span: the **Add spacer** window opens, with the fields
explained under [Edit on a spacer](tools-general.md#edit-on-a-spacer). A
bare end point gets the spacer clamp of the object templates. The first
span point again, or **Escape**, cancels. Both sections must be solved
by finite elements (not the ruling span).

**Good to know**

- New strings hang from the points the structure has; a new attachment
  point is made only when you ask for one (**+ point**) or a shape needs
  one. On a structure with a type, a new point is put on one of its
  type's attach joints.
- A new string's wire point gets a set, so a section or jumper can end
  there. For a suspension string that should hold a jumper, untick its
  wire point's **set**.
- On a structure with a type, the strings added are the structure's own,
  over its type's.
- **Undo**: Apply is one step.

## Add post insulator

![The Add post insulator window: the post standing on the point clicked, its ratings, its jumper exit angle, its direction and length, its wire point](../images/tools/tool-add-post.png)

**What it is for.** Add post insulator (2026-11-14) stands a **post
insulator** on a structure: an insulator standing on a point of the
structure — its base, earthed — and holding a wire point at its top. A
**fixed** post holds the wire point rigidly as a cantilever (a station
post upright, a line post horizontal); a **hinged** post (a braced line
post) turns about its base and is held by a brace string to its top. How
a post carries its load and how it is checked is explained in [Post
insulators](../theory/post-insulators.md).

**How to use it**

1. Hold **Add post insulator** (Edit › Tools › Structures, or the
   inventory; put it in a hotbar slot to keep it at hand). While it is
   held, structures and their points can be clicked.
2. Click:

   | Clicked | What opens |
   |---|---|
   | a point of a structure | **Add post insulator to *structure***: an upright post standing on it, its wire point 1.5 m above, at the next free set |
   | a structure, or one of its strings | the same, on the first point no string hangs from yet (a structure without points: a new point 20 m up) |

   The right-click menu of a structure or a point offers **Add post
   insulator…** too.
3. Choose its **shape**, set its length, direction and ratings, then
   **Apply** (Enter).

**The window** is the strings window ([Edit on a
string](tools-general.md#edit-on-a-string)) with the post's own fields:

| Field | What it does |
|---|---|
| **shape** | **Post insulator, upright** (a station post: its wire point straight above), **horizontal** (a line post: its wire point level, out to the side of the structure the point is on) or **braced** (a hinged horizontal post and a brace from a point 1.5 m above its base to its top). 1.5 m is a length to start from: set the real one. |
| **stands on** | The point its base stands on. |
| **weight**, **wind area** | Its own weight and the area the wind blows on: at its middle for a fixed post (a load of its base with half its length as lever), half at each end for a hinged one. |
| **ratings along it**: **compression**, **tension** | What it is rated for along its axis (its data sheet). Ticked: checked in Checks › String checks; unticked: not given (a library post's, when the string names one). |
| **post insulator** | Ticked: it is a post. Unticked, it becomes an ordinary string (its post fields cleared). |
| **hinged base (braced)** | It turns about its base and only pushes or pulls: another string at its wire point (its brace) must hold it — Apply is refused without one. |
| **cantilever rating** | The load across it at its top it is rated for; compared with its bending moment at its base over its length (Checks › String checks). Fixed posts only. |
| **jumper exit angle**, **in plan** | Where its clamp holds a jumper ending at its wire point: the angle from the horizontal (negative: down; unticked: level) and the direction in plan in the structure's axes (0 ahead, 90 right; unticked: towards the jumper's other end). Fixed to the structure: it does not turn with the wind. |
| **down/up**, **direction**, **length** | Where its wire point is from its base: 180° straight up, 90° level; the direction in plan; its length. |
| **wire point**, **set**, **clamp** | Its wire point at its top, with the set a section or jumper ends at. |

**Good to know**

- A fixed post holds its wire point alone: another string ending at the
  same wire point is refused (a braced post is a hinged one).
- A post is never a tension string.
- Opening a post with **Edit** shows the same fields: they are kept when
  you Apply.
- A post standing on the ground (on a pedestal of its own) and a rigid
  conductor carried on posts are not built yet.
- **Undo**: Apply is one step.

