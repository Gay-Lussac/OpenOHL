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

![A structure seen along the line: a suspension string swung by the wind towards the body, the conductor at its clamp with an electrified grading ring, grounded fittings at the string's top, and the smallest distance from the electrified parts to the nearest member against the required distance](../images/theory/internal-clearance.svg)

At each structure with a body (detail level 2 or 3), in each weather case
of a rule, as solved (the strings' swing included):

- the **conductors** near it: each span's wire from its clamp out to the
  rule's reach (default 20 m, in plan along the solved wire), a bundle
  subconductor by subconductor;
- its **jumpers**;
- its strings' **live ends** (their wire points, as points);
- the **electrified volumes** on its strings (second part).

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
**Said where it applies** (2026-11-08): per structure, *NOT ON THE SAFE
SIDE: T3: 4 string(s) without volumes — their fittings' size (clamps,
horns, yokes) not counted, the conductor measured from its clamp …*,
with the first ten labels.

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

**A torus** (a grading ring, 2026-10-31, owner: "the builder window with
a torus"; built, in active development, not approved yet) is the one
volume that is **not convex**, so GJK cannot measure it whole. It is its
centre circle — radius \\(R = (D - t)/2\\), \\(D\\) its outer diameter,
\\(t\\) its tube's — grown by the tube's radius \\(r = t/2\\): every point
of it lies within \\(r\\) of the circle.

- **Against a point or a sphere: exact.** From a point at distance
  \\(\rho\\) from the ring's axis and \\(h\\) above its plane, the nearest
  point of the circle is \\(\sqrt{(\rho - R)^2 + h^2}\\) away; the
  distance to the torus is that less \\(r\\) (and a sphere's radius).
- **Against a segment, a triangle, a box, a cylinder or cone, or another
  torus: by chords, conservative.** The circle is replaced by the polygon
  of \\(n\\) equal chords inscribed in it. A point of the circle lies at most
  the **sagitta** \\(s = R\,(1 - \cos(\pi/n))\\) from its chord, so every
  point of the torus lies within \\(r + s\\) of a chord: each chord is
  measured as a convex segment by GJK, grown by \\(r + s\\), and the least
  of them taken. The distance found is **never more than the exact one
  (conservative)** and at most \\(2s\\) less (a chord lies inside the circle
  by at most \\(s\\), and is grown by \\(s\\) more). \\(n\\) is chosen from
  \\(R\\) so that \\(s \le 0.25\\) mm (at least 24 chords): the distance is
  **at most 0.5 mm below the exact one**. Two tori: both circles by their
  chords, at most 1 mm below. *Conservative assumption:* the chords'
  margin is added in full, everywhere along the ring, where the exact
  shortfall is smaller between the chords' ends.

Measured against the exact distances (tests: by hand — a segment through
the hole, a plane over the ring, a box beside it, two stacked rings, two
linked rings — and against 60 random segments, each sampled at 20 000
points in closed form), the distance found was never above the exact one
and never more than \\(2s\\) below.

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
  cylinder's, a cone's, a box's length, a torus's: the ring square to the
  string, around it) lies along the axis the freedoms give.

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
2026-10-19). An optional `altitude` term (2026-11-09) makes the
distances grow with the height above sea level, taken at the **highest
point of the structure** (its members and grounded shapes) or of the
part measured, whichever is higher — *conservative*: never below any
point measured ([Distances growing with the altitude](clearances.md#distances-growing-with-the-altitude)).
With **Use local conditions** ticked the cases
are solved again on the line with the structures' wind for clearances
and wire ice on their spans, and a structure's **wind study** is taken
there (below). No distance is built in.

## Wind study at a structure

*Built, in active development, not approved yet* (owner, 2026-11-04:
"make the wind study local"). A structure's local conditions may give a
wind study's pressure \\( q_s \\) (`local.wind_study`, Pa) for the
conductors' distances to it. With **Use local conditions** ticked, each
weather case \\( c \\) of the rules with wind (\\( q_c > 0 \\)) that the check's
`[check] wind_study_cases` takes (none listed: every one) is solved
again on the line as written with its pressure replaced:

\\[ q = \max(q_s,\ q_{min}) \\]

\\( q_{min} \\) the check's least (`[check] wind_study_min`; none: no
bound) — a value below it is raised to it, said. The structure's rows in
that case are taken from this run only (named `<case> (wind study q
Pa)`); the other structures keep the case's run. The spans' own local
winds apply on \\( q \\); the structures' **wind clearance** factors do not
(the study replaces the wind there). A wind study raised to the least is
on the safe side (more wind, more swing); a study lower than the case is
the engineer's to justify (a design code's bound, e.g. the least of the
LeV's explanations on ESTI 240 Art. 33). Before 2026-11-04 a weather case
group's `structure_wind` column gave it, for the group's cases.

## Cost and exit clause

Per structure and case, every part near it is measured against every
member and volume there, pruned by axis-aligned boxes: a member or volume
farther in its box than the nearest found so far is not looked at. A
structure takes milliseconds; one budget step is taken per structure in
each case and rule, and the run stops at the time limit (the command
line's `--time-limit`, the window's ten minutes) with what it says.

## Validation

Unit tests in `crates/ohl-clearance/src/internal.rs`, on invented
geometry:

- `nearest_on_a_synthetic_body`: a vertical member with a wire 3 m from
  it, and a box member with a wire 2 m from its face: the exact
  distances, to the member's line and to its surface.
- `the_example_line`: on the example line, a vertical member 2 m beside
  each wire point: every part found, the string ends 2 m off, the
  conductors that less their radius; the other structures listed as
  level 1; overrides taking a structure out of a case; a required
  distance by voltage.
- `the_volumes_on_the_strings`: the volumes placed on a solved string
  (the horns, the yoke of a double string once, its top fittings twice),
  a shape overridden as insulated left out, the distance with wind.
- `internal_rules_in_a_rule_file`: the rule tables read and written
  back, their defaults, bad values refused.
