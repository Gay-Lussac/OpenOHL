# Internal clearances

The internal clearances (roadmap O6; in active development, not approved
yet) are the distances between the electrified parts at a structure and
the structure itself: its body (levels 2 and 3) and the grounded fittings
on its strings. This page says how they are measured and which
assumptions are **conservative** (a distance never more than the exact
one), **on the unsafe side** (a distance that may be more than the real
one) or **approximate** (neither: a simplification of the geometry). The
user's side is in [Clearance rules](../user-guide/terrain.md#internal-clearance-rules-internal_rule)
and [the desktop application](../user-guide/desktop-app.md#internal-clearances).

## What is measured

At each structure with a body (detail level 2 or 3), in each weather case
of a rule, as solved (the strings' swing included):

- the **conductors** near it: each span's wire from its clamp out to the
  rule's reach (default 20 m, in plan along the solved wire), a bundle
  subconductor by subconductor;
- its **jumpers**;
- its strings' **live ends** (their wire points, as points);
- the **electrified volumes** on its strings (second part, 2026-10-25).

Each against **every member** of the body and against the **grounded
volumes** of every string at the structure. Electrified parts are not
measured against each other (they are at the line's potential), grounded
ones not against the body (they are at the structure's). Insulated
volumes (the insulator itself) are drawn, never clearance targets: their
length is insulation, not a clearance. Earth wires are not electrified.

One row per rule, structure, case and part: its smallest distance, the
member or volume it is nearest, the required distance.

## Distances

**Wires** (conductors, jumpers) are their solved polylines. The distance
between two segments, or a segment and a triangle, is exact (closest
points, Ericson's *Real-Time Collision Detection* §5.1). From the
distance between the wire's polyline and the target the subconductor's
radius in the case (iced with ice) is taken off, and the bound of how far
the real catenary lies off its polyline (its largest sagitta, from the
catenary's smallest radius of curvature) — **conservative**: the distance
reported is never more than the exact one to the wire's surface.

**Members** are measured to their **surface** where their solid is known
(a member whose section has a profile in the section library, a pole's
outline: triangles), else to their **centre line** — the member's size
left out, **on the unsafe side** (said per structure and on every row). A
part ending inside a closed solid counts as 0 (into it).

**A string's live end** is a point: the size of its fittings is not
counted unless they are given as volumes — **on the unsafe side**.

**Volumes** are convex solids: spheres, cylinders, cones (truncated or
not) and boxes, and the points, segments and triangles they are measured
against. The distance between two convex solids is found with the
Gilbert–Johnson–Keerthi algorithm (GJK, in van den Bergen's form) on
their support functions (the point of a solid farthest along a
direction). At every step it has a point of the two solids' difference
and a direction \\(v\\): every point of the difference lies at least
\\( v \cdot w / \lVert v \rVert \\) along \\(v\\), where \\(w\\) is the
difference's support against \\(v\\) — a **lower bound** of the distance.
The algorithm stops when the bound is within 1 nm of the distance found,
and reports the bound: **conservative**, and exact to 1 nm. For points,
segments, triangles and boxes (polytopes) it ends in finitely many steps;
for cylinders and cones it converges (each step brings the bound up); it
stops after 200 steps at the latest, still with a lower bound. A sphere is
its centre grown by its radius: the distance to its centre less the
radius, exact. Two solids that touch or overlap are 0 apart (a sphere's
radius beyond that: negative, into it). A volume whose centre lies inside
a member's closed solid counts as 0.

## Placing the volumes on a solved string

The volumes belong to the string's insulator assembly (the string
library); a line's string may override one by name. Each is given by its
position along the string from the top (a fraction of the length or
metres), its size, an offset from the string's axis and its electrical
state.

The string is solved as a straight member from its top to its wire point;
the solve knows no twist and no bending of the string. The volumes are
placed on that solved axis, in the string's **own axes**: the axis (top to
wire point), *across* and *along*.

- **A suspension string.** The volumes' axis takes the solved swing
  across (the string's angle from straight down in the structure's
  across–vertical plane) and along (in the along–vertical plane) where
  that **freedom is free**; where it is **blocked** it keeps the angle the
  string has in still air, as placed in the line. *Across* is the
  structure's across axis where the **twist** is blocked, else the level
  direction across the conductor at the wire point (as solved: towards its
  neighbouring clamps), made square to the axis; *along* is square to
  both (ahead for a string hanging straight down). A string pointing less
  than about 3° below level is not swung by its freedoms. **Approximate:**
  a blocked freedom turns the volumes only, not the string; the solve's
  string swings as it does.
- **A strain string.** Its axis is the conductor's pull, as solved; the
  freedoms are not used. *Across* is level, square to the axis, to the
  structure's right; *along* is square to both, upwards.
- **Where each volume is.** Its centre is on the **solved** axis at its
  distance from the top (a fraction of the solved length, or metres),
  moved by its offsets along *across* and *along*. Its own axis (a
  cylinder's, a cone's, a box's length) lies along the axis the freedoms
  give.

**Several strings in one assembly** (double, triple): the line models them
as one string on their middle. Each string's copy of a volume is moved by
that string's offset — side by side across, one behind the other along,
or at the corners of an equilateral triangle of side *spacing* centred on
the axis — parallel to the modelled string; a **shared** volume (a yoke
plate) is placed once, on the modelled axis. **Approximate:** the strings'
own tops and the yoke's linkage are not modelled; each copy is parallel to
the modelled string. Two strings to one clamp (a V) are two strings of the
line: each leg carries the assembly's volumes.

## Rules

A rule gives the weather cases, the voltages, the parts and the distance
to keep — fixed, by voltage or by a formula — and optionally **another in
the cases with wind** (`distance_wind`: a wind pressure above 0; owner,
2026-10-19). The structure and section overrides take a structure's or a
section's own weather case groups. No distance is built in.

## Cost and exit clause

Per structure and case, every part near it is measured against every
member and volume there, pruned by axis-aligned boxes: a member or volume
farther in its box than the nearest found so far is not looked at. A
structure takes milliseconds; one budget step is taken per structure in
each case and rule, and the run stops at the time limit (the command
line's `--time-limit`, the window's ten minutes) with what it says.
