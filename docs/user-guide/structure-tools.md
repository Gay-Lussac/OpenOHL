# OHL-Structure tools

*Built, in active development (not approved yet).* OHL-Structure, the
structure application, has a hotbar of tools at the bottom of its 3D
view, as OHL-Design does. This page explains how its tools are held and
used, and each tool: what it is for, how to use it, its options and what
to look out for. The window as a whole: [OHL-Structure, the structure
application](structure-app.md). OHL-Design's tools: [Tools](tools.md).

## Holding and using a tool

- **The hotbar**: nine slots at the bottom of the 3D view. A click on a
  slot or the keys **1–9** hold its tool; hover a slot for the tool's
  name, key and what a click does. An empty slot holds Select.
- **The strip above the view** names the tool held, its options (the
  snap grid, a string's length, the symmetry, where a joint is inserted)
  and, in grey, what a click does.
- **Escape** lets go of the tool: Select is held again (with the Link
  tool, the point chosen is dropped first). Over the Detail view, Escape
  first goes back from a member to its joint.
- **The inventory** — **E**, the **E** button beside the slots, or
  **Edit › Tools › Tool inventory**: every tool. Drag one onto a slot,
  drag a slot onto another to swap them, drag a slot into the inventory
  (or right-click it) to empty it. The slots and the slot held are kept
  for the next start.
- **Edit › Tools** lists every tool by group — *Select and edit*,
  *Geometry*, *Body* — with its hotbar key; choosing one holds it.

With **Select**, **Move**, **Symmetry** and **Link** held, a click in
the view selects (Link: picks the point, then the joint); with the other
tools a click **acts** on what is under the pointer, or, on nothing,
where it meets the plane facing you through the structure's middle. A
drag turns the view (with Move held, a drag from a joint or point moves
it). A right-click opens the menu of what is under the pointer.

**Every change a tool makes is one undo step**: **Edit › Undo** (⌘Z;
Ctrl+Z on Windows and Linux) takes it back. The Geometry and Body menus
and panels have the same actions under the same names, on what is
selected.

**The default hotbar**, slots 1 to 9: Select, Edit, Move, Add a point,
One string below the point, Link, Add a joint, Add a member, Delete.
Two strings to one wire point, Insert a joint and Symmetry wait in the
inventory.

| Tool | Group | What a click does |
|---|---|---|
| [Select](#select) | Select and edit | selects (Shift + click: another) |
| [Edit](#edit) | Select and edit | opens where the object is edited (the Inspector; a tower's joint or member: the Detail view) |
| [Move](#move) | Select and edit | a drag moves a joint or point |
| [Delete](#delete) | Select and edit | deletes it with what goes with it, asked first |
| [Add a point](#add-a-point) | Geometry | an attachment point on the joint clicked, or at the top |
| [One string below the point](#one-string-below-the-point) | Geometry | a string below the point clicked |
| [Two strings to one wire point](#two-strings-to-one-wire-point) | Geometry | two strings from two points to one wire point |
| [Link](#link) | Geometry | links an attachment point to a joint |
| [Add a joint](#add-a-joint) | Body | a joint above the joint clicked, or where clicked |
| [Add a member](#add-a-member) | Body | a member between two joints clicked |
| [Insert a joint](#insert-a-joint) | Body | a joint inserted on the member clicked |
| [Symmetry](#symmetry) | Body | (the strip: the body's symmetry, Make symmetric) |

## Select

**What it is for.** Selecting joints, members, attachment points and
wire points, to see and edit them in the Inspector and the tables.

**How to use it**

1. Hold **Select** (slot 1, or Escape).
2. Click an object in the 3D view: it is selected (and its row in its
   table). **Shift + click** adds another, or takes it out again. A
   click on nothing clears the selection.
3. Drag to turn the view.

**Good to know.** Nothing moves with Select: joints and points move
only with the Move tool held.

## Edit

**What it is for.** Edit opens the place where the object clicked is
edited.

**How to use it**

1. Hold **Edit** (slot 2).
2. Click a joint, a member, an attachment point or a wire point:
   - it is selected, its fields are shown in the **Inspector** (brought
     to the front) and its row in its table;
   - on a **tower**, a joint or member first asks **Open … in the Detail
     view?** — **Open the Detail view** shows it there with its details
     on the left, **Stay** keeps to the Inspector. With the Detail view
     open already, it is shown there at once, not asked.

**Good to know.** Edit itself changes nothing: the change is made in the
Inspector, a table or the Detail view (each one undo step). See [Joint
details](structure-app.md#joint-details-towers).

## Move

**What it is for.** Moving a joint, an attachment point or a wire point
by dragging it in the 3D view.

**How to use it**

1. Hold **Move** (slot 3).
2. Drag a joint or point: it moves in the plane facing you, snapped to
   the grid or onto a joint near the pointer.
3. Release: the move is made.

**Options (the strip).** **snap**: the grid a dragged (or added) joint
or point snaps to, m (5 cm at first; 0: none).

**Good to know**

- Members are not dragged; a drag elsewhere turns the view; a click
  selects.
- An attachment point on a joint stays on its joint: dragging it
  changes its offset (see [Attachment points, joints and the Link
  tool](structure-app.md#attachment-points-joints-and-the-link-tool)). A
  joint moved takes its points along.
- With a symmetry set, a joint moved takes its mirror images along.
- **Undo**: one step per drag.

## Delete

**What it is for.** Deleting a joint, member or point with whatever
cannot stay without it.

**How to use it**

1. Hold **Delete** (slot 9).
2. Click the object. The **Delete** window lists it with everything that
   goes with it (a point: its strings).
3. **Delete** deletes them all; **Cancel** (or the window's ×) changes
   nothing.

**Good to know**

- **Delete** or **Backspace** deletes everything selected the same way,
  asked first; **Delete…** in an object's right-click menu too.
- With a symmetry set, the mirror images go along.
- **Undo**: one step.

## Add a point

**What it is for.** Adding an **attachment point**, where strings hang.

**How to use it**

1. Hold **Add a point** (slot 4).
2. Click:
   - a **joint**: a point on it (it moves with the joint) — not on a
     support joint, since strings hang from attach joints (the message
     says so);
   - elsewhere: a point at the top of the stick figure, to be moved in
     the view or the Attachment points table.

**Good to know.** The new point is selected; the message line names it.
**Undo**: one step.

## One string below the point

**What it is for.** Hanging one string below a point, down to a new wire
point.

**How to use it**

1. Hold **One string below the point** (slot 5).
2. Set **string length** in the strip.
3. Click an attachment point (or a wire point): a string of that length
   hangs straight down from it to a new wire point.

**Options (the strip).** **string length**: the length of a new string,
m (4 m at first).

**Good to know.** The new wire point has **no set**: put its set in the
**Sets** table (the message says so). **Undo**: one step.

## Two strings to one wire point

**What it is for.** Two strings from two attachment points to one wire
point below their middle (a V).

**How to use it**

1. Hold **Two strings to one wire point** (from the inventory or Edit ›
   Tools).
2. Set **string length** in the strip.
3. Click the first attachment point, then the second: two strings go to
   one new wire point, the string length below the middle of the two
   points.

**Good to know.** As for one string, the new wire point has no set: put
it in the **Sets** table. A click on nothing between the two clicks
starts again. **Undo**: one step.

## Link

**What it is for.** Linking an attachment point to a joint, where the
point is: its distance from the joint is kept as its offset (a rigid
link, whose moment the analysis applies at the joint).

**How to use it**

1. Hold **Link** (slot 6; or **Link to a joint…** in a point's
   right-click menu).
2. Click an attachment point: the **attach joints** are ringed. The one
   under the pointer shows the offset's length and its moment arm.
3. Click a ringed joint: the point is linked to it.

**Keys.** A **right-click** or **Escape** drops the point chosen; a
second one lets go of the tool.

**Good to know**

- Only attach joints take a link; another joint under the pointer says
  *not an attach joint: set its kind to attach in OHL-Structure* (its
  kind is set in the Joints table).
- A point cannot be put on a member: [Insert a joint](#insert-a-joint)
  on the member first, then link the point.
- **Unlink** is in the point's menu (in the view or the Attachment
  points table) and in the Inspector.
- **Undo**: one step.

## Add a joint

**What it is for.** Adding a joint of the body.

**How to use it**

1. Hold **Add a joint** (slot 7).
2. Click:
   - a **joint**: a new joint 1 m above it;
   - elsewhere: a joint where you click, in the plane facing you through
     the structure's middle, snapped to the grid.

**Options (the strip).** **snap**: the grid, m (5 cm at first; 0:
none).

**Good to know.** With a symmetry set, its mirror images are added too.
The message gives the new joint's id and place. **Undo**: one step.

## Add a member

**What it is for.** Adding a member between two joints.

**How to use it**

1. Hold **Add a member** (slot 8).
2. Click the first joint, then the second: a member joins them.
3. The next member starts at the second joint: click the next joint to
   go on.

**Good to know.** With a symmetry set, its mirror images are added too.
A click on nothing starts again. **Undo**: one step per member.

## Insert a joint

**What it is for.** Inserting a joint on a member, so a point can be
linked there.

**How to use it**

1. Hold **Insert a joint** (from the inventory or Edit › Tools; or
   right-click a member: **Insert a joint**).
2. Set **at** in the strip.
3. Click a member: an **attach** joint is inserted at that fraction of
   its length from its first joint.

**Options (the strip).** **at**: the fraction of the member's length
from its first joint (0.5 at first).

**Good to know**

- The member is split there into two pieces that stay **continuous**
  (the same cross-section, kind and other fields; a rigid connection,
  not a hinge). The second piece is named `<id>-2`.
- Each piece keeps the whole member's buckling length (`lk`):
  **conservative**, as the joint braces nothing; set `lk` when it is
  braced.
- Then link a point to it with the [Link](#link) tool.
- **Undo**: one step.

## Symmetry

**What it is for.** Setting the body's symmetry, so joints and members
take their mirror images along when added, moved or deleted.

**How to use it**

1. Hold **Symmetry** (from the inventory or Edit › Tools; also Body ›
   Symmetry).
2. In the strip choose **none**, **x (left ↔ right)** (about the plane
   x = 0, left and right of the line), **y (ahead ↔ back)** (about
   y = 0) or **both**.
3. **Make symmetric** gives every joint and member the mirror images it
   lacks (greyed until a symmetry is chosen).

**Good to know.** A click in the view selects while Symmetry is held. A
joint on a mirror plane stays on it. Each change is one undo step. See
[The Body panels](structure-app.md#the-body-panels).
