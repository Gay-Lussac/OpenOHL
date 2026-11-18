# The automatic joint layout

*Built 2026-11-18, in active development.* How **Lay out joints**
(OHL-Structure, [user guide](../user-guide/joint-layout.md)) places a
tower joint's member ends so that they fit, and what that does to the
analysis and the checks. The code: `ohl_structure_link::joint_layout`
(the plan of one joint, a pure function), `layout_run` (the run on a
file, its record, following later edits), `clash` (the 3D check).

## What is decided, and what is not

The layout decides, per member end bolting by one leg of an angle onto
the flange of a member at the joint: where its steel stops (`cut`), a
sideways move (`shift`) where needed, what it bolts onto (the flange or a
gusset plate), the member's roll that lays it flat, and the plates
(thickness, outline). It does **not** choose the bolts: an end's bolts
are those given (or its member's design data), else a connection preset
for its class, else two of a bolt picked in the window in one gauge line
— those are **provisional**, and a tower with provisional bolts stays at
level 3 for the joint details until the design codes choose the bolts:
since 2026-11-18 (third round) the core's level reads the layout's list
of provisional bolts, so every application, the command line and the
line's calculation level 4 leave the joint details out and say why.

## The angles' orientation first

The layout works from the members' rolls. A tower imported from an
export that carries no member orientation (the tower program's own
files have it, its export does not), or drawn by hand, has every angle
at the default orientation: its legs stand turned anyhow and much of the
bracing then lies on the inner face of a leg's flange, or its axis out of
the flange's plane. **Orient angles** (`ohl_structure_link::orient`)
turns them as a lattice tower is built, from the tower's shape alone:

- **Legs and chords** (class `leg`, or angles running on in line within
  10° through a joint and lying in no face of the legs — a face's
  diagonal running on through its crossing is bracing of that face, not a
  chord): heel out, a flange in each of the two faces
  meeting at the member. The faces come from the neighbouring legs'
  lines: the section of the tower's legs (for a leg; the chords for a
  chord) through the member's middle, seen along it — those within 70°
  of it whose lines a member joins to its line, lying in one plane with
  both within 5° (a face's bracing, a plan member; a portal's other
  column, joined only through its beam, and an arm's chord joined to a
  leg by the arm's bracing are not neighbours) — is a convex polygon of
  which the member is a corner; the two corners next to it
  give the two faces. Where the faces meet at θ ≠ 90° (seen along the
  member), no angle lies in both: the bisector of the angle's legs is
  put on the bisector of the faces, each flange |θ − 90°| / 2 off its
  face (said per member). For a square or rectangular body this is a
  fraction of a degree; for a triangular crossarm 15° — more than the 5°
  a member is laid flat within, so the bracing there needs a bent plate
  or a hand detail. Faces meeting at less than 45° or more than 135°
  (each flange more than 22.5° off) are taken as no corner: the member
  is left (an assumption of the tool).
- **Plan and hip members** (horizontal within 5°, in no steep face of
  the tower): one leg horizontal, the other pointing down — **an
  assumption** of this tool (no water lying in the angle), not a
  standard's; the horizontal leg towards the tower's middle.
- **Face members** (since 2026-11-18, tuning on real towers): every other
  single angle lying in a steep face of the tower — its two ends within
  20 mm (and 5 mm per metre of its length) of the face's plane, its middle
  between the face's two legs or chords — the face's diagonals,
  horizontals and redundants: one leg in the face, the other pointing out
  of the tower, so that it lies on the outer face of the legs' flanges
  (usual lattice practice: the bracing bolted on the outside of the legs).
  The leg in the face points down from the heel where it can (**an
  assumption** of this tool, as for the plan members: no water lying in
  the angle — not a standard's); an unequal angle's leg bolted (its design
  data) is the one in the face. The 20 mm is a tolerance of this tool, not
  a standard's.
- What lies in no face (a diagonal of a sloping crossarm's bottom face, an
  internal diaphragm) is left to the layout.

A roll is written to a hundredth of a degree; the same tower gives the
same rolls (a run again changes nothing).

## The order: the biggest profiles first

(Owner, 2026-11-18: "pull everything back then line up the biggest
profiles and join them either continuously if they are the same and
aligned or with bolts together if they can fit within each other and if
not with a plate then bring the second biggest profiles back and try to
fit them on a surface of the bigger ones with bolts at first and if too
many are needed add a plate … finally if the angle at which the profile
is coming really doesn't allow for it then add a welded plate".) The
layout follows these steps; what is built and what is not:

| Step | What | Built |
|---|---|---|
| 0 | Every end pulled back: the values a run set taken out first, each end planned from its joint as it is without them | yes (as before) |
| 1a | The biggest pair of angles in line through the joint (the larger legs first): **one part** (`kind = "continuous"`) when they have the same section and are turned alike (the core's rule: in line within 1°, the same roll and reference) and **neither end has bolts** — a splice whose bolts would then go unchecked is left a bolted splice (on the safe side) | yes |
| 1b | The smaller **nested** in the bigger (`face = "nested"`, bolted to it) when its legs fit inside (each at most the bigger's leg less its thickness and root radius) and lie along the bigger's within 5° | yes |
| 1c | Other sections in line: a splice plate or covers | no: left as it is (a splice by hand) |
| 2a | The next members, by class and size, **bolted directly on a flange** of the bigger member where their bolts land on it | yes (as before) |
| 2b | Where not: **onto a plate** lying on that flange (in its plane) | yes (as before; every end of that flange's face onto the plate) |
| 3 | The smaller classes (redundants) the same way, after the bracing | yes (as before: the order bracing, redundants) |
| 4 | An end whose angle of arrival allows no flat fit on a flange or a plate in a flange's plane: a plate in its own plane, **welded** to a plate bolted to the bigger member | no: left for a hand detail ("no turn lays it flat at both its ends") |

Only one pair is joined at a joint (its base), and none where a member
already runs on through it: a pair crossing the base (an X-brace's other
diagonal) bolts onto it. Joining one part where the ends have bolts would
leave a splice's bolts unchecked (NOT ON THE SAFE SIDE if a splice is
there), so such a pair stays a bolted splice. A member joined as one part
is never turned by the layout (its roll stays alike with its partner's).

"Too many bolts", before the codes choose the bolts: the provisional bolts
(the end's own, a preset's, else two of the bolt picked) are tried on the
flange: every bolt clear of the flange's other leg and root by the nut's
corner radius, and the edge distance from its toe (the code's, else the
default minimum). If they do not all land, the end goes onto a plate.

Tried and left out (measured on two real towers, 2026-11-18): some ends of
a flange's face bolted directly and the others onto a plate on the other
side of the work point — more joints then clashed than were laid out;
every end of the face goes onto the plate. On a **built-up** member, where
no plate is laid out, the ends whose bolts land on its angle's flange are
bolted to it and the others left.

## Which member an end bolts onto

An end's `onto` given is kept. Where it is only worked out (the joint's
leg) or missing, the layout names one first — in its planning copy, as it
then writes it — so that the end's leg bolted is the one lying on that
member and a turn laying it flat is found (since 2026-11-18, tuning on
real towers; before, such ends were measured against the leg that was
not bolted and left with "its axis runs out of the face"):

- **Nothing named** (a crossing diagonal, a redundant at a diagonal's
  middle): the member running through the joint (in line with another
  member there, or one part with it), the one part first, then by class
  (leg, bracing, redundant), the wider, the id.
- **The joint's leg, but in neither of its flanges' planes** (a plan
  diagonal meeting a vertical post; within 5°): a single angle running
  through the joint in a flange's plane of which the end's axis lies (the
  horizontal running through), chosen in the same order. The end then
  bolts flat onto that member, as built; none: the joint's leg kept (and
  the end is left for a bent plate or a hand detail, said).

What an end bolts onto is what its restraint is worked out from (the
code's stiffness rule, as for any end bolted onto a member): the member
it is laid out onto, as detailed.

## Turned only where it helps

(Owner, 2026-11-18: "only turn where it helps".) A member lying off what
it bolts onto (more than 5°) is turned by the layout only when

1. one roll lays it flat at all its ends — those at joints not laid out
   in this run too: the turns that make each end flat lie within 5° of
   each other (a turn and a half turn more lay a leg flat alike); the
   middle of them is taken, so a turn never spoils an end that was flat;
2. no end of it at a joint not laid out in this run is laid out with its
   roll now (that joint would change unasked);
3. at least one of its joints is laid out **with** the turn (the member
   among the ends laid out there, after the clash check).

**Half a turn.** An end on the **inner face** of a flange (its outstanding
leg towards the angle's other leg) that cannot be laid out there — it
leaves the joint towards that other leg — is turned half a turn (its
outstanding leg to the other side: on the outer face, flat as before). As
lattice towers are built: an X-brace's two diagonals lie one on the outer
faces of the legs, the other on their inner faces, back to back where
they cross. The half turn is kept for a member only when more of its ends
are planned laid out with it and no joint of it loses an end laid out;
then as any turn (rule 3 below). A leg's or chord's roll is never turned.

A turn failing 3 is not made: the joints are planned again without it,
until every turn left helps (a member's roll unchanged is never worse for
the joints the run did not lay out). A roll the layout set before is
taken back — the member's roll as it was — when none of its joints is
laid out with it any more (both its joints left for a hand detail). A
roll set by Orient angles on a leg or chord is kept (the tower's shape).

## Concentric first

Every end is first placed **concentric**: the members' gauge lines meet
at the work point (the owner's choice, 2026-11-18). This is the usual
detailing practice of lattice towers: members connected directly to each
other with minimum eccentricity (ASCE 10-97, 5.1.3). The eccentricity
that remains — from the gauge line, through which the bolts pass, to the
angle's centroid — is the *normal framing eccentricity* of an angle
bolted by one leg, which ASCE 10-97 takes into the member's effective
slenderness (3.7.4.2, the curves for a concentric load and for normal
framing eccentricity at one or both ends; commentary C3.7.4, which ties
"normal" to a bolt pattern whose centroid lies between the heel and the
centre line of the connected leg) and leaves out of the analysis: the
moments of normal framing eccentricities are not computed in the truss
analysis but covered by the member selection (C2.4).

So a concentric end is **drawn** with its gauge line through the work
point (the record's `gauge` mark: a drawing and clash-check offset only,
like the seat of a leg on a flange), while the analysis keeps the
member's axis through the work point and the check takes the
eccentricity gauge to centroid as before (`member_ends`: one leg bolted,
the end eccentric). Nothing new enters the analysis.

**Assumption, on the safe side where the code's curve applies:** the
gauge used is the end's own (given), else the leg's middle (the drawing's
assumption, one gauge line). Commentary C3.7.4 asks for due consideration
when the bolts' centroid lies outside the heel–centre-line band; a gauge
given beyond the leg's centre line is kept as given and checked as
before — **NOT ON THE SAFE SIDE** where that extra eccentricity is large;
the layout never moves a gauge.

## Pulled back: the cut

In each plane (a flange's outer or inner face), each end is a strip — its
leg bolted, from where its steel stops outwards — and is pulled back
(only along itself) until it clears:

- the flange's heel (the corner of the angle it lies on; on an inner
  face, also the angle's other leg and root, by half the gap);
- the strips of the ends placed before it, each widened by half the gap
  (bracing before redundants, then the wider leg, the larger angle to what
  it bolts onto, the id): the least cut is the farthest point, along this
  end, of the region where the widened strips overlap (an exact
  intersection of half-strips, clipped as convex polygons);
- the bolts already on that face that are not laid out (a leg's splice
  bolts): each a disc of its head's corner radius and half the gap.

The cut is rounded up to the millimetre. A cut beyond the **longest
pull-back** (a setting) or beyond half the member less its bolts does not
fit concentric.

## Moved sideways: the shift

Only where concentric does not fit, the end is moved sideways, a
millimetre at a time either way, the smallest move first, up to the
**shift limit** (a setting). The move is written as the connection's
`shift` (beyond the concentric place). The analysis takes it as a rigid
end offset (an eccentricity the force line passes the work point by):
the moments where a frame member meets the joint, and — where only bars
meet it — left out and said per member and case, **NOT ON THE SAFE
SIDE** there ([Eccentric member ends](structures-fe.md#eccentric-member-ends-rigid-end-offsets)).
The code's slenderness curves cover only the normal framing eccentricity
(above), not this additional one. That is why a shift comes last and is
limited: on the real towers measured the layout needed a shift for
very few ends (see the settings below).

## Onto the flange or onto a plate

When every bolt of a plane lands on the flange — at least the angle's
thickness, root radius and the nut's corner radius from the heel, and
at most the flange's width less the edge distance (the code's minimum
from its plugin, else the structure's default minimum edge distance —
1.5 bolt diameters to start with, `[default_bolt_minimums]`; a default
minimum, assumed, not a code's: it takes no force, so it may be smaller
than a code's would be, **NOT ON THE SAFE SIDE** in that sense) from it —
the ends bolt on directly. A provisional bolt's pitch and end distance
not given are the default minimum spacing and end distance (3 and 1.5
bolt diameters to start with), as the joints are drawn.
Otherwise every end of the plane goes onto one plate on that flange, each
pulled back further where needed so that its bolts lie past the flange's
toe (or heel) by the nut and the gap: the nuts behind the plate clear of
the flange. The plate is as thick as the thickest leg bolted on it, its
outline round the bolts with the code's end and edge distances (the rule
of [plates](joint-plates.md)), cut at the flange's heel by half the gap
so that the plates of a corner's two faces do not overlap. At a corner the
two plates are each bolted to their flange (usual lattice practice) or,
as a choice, welded together. On an **inner face** (since 2026-11-18) the
plate lies between the flange and the members as on the outer one, its
outline cut at the angle's thickness and root radius and half the gap
from the heel — clear of the angle's other leg and its root — and the
clash check sees it like any part. On a built-up leg no plate is laid
out (it is drawn through the work point, where its checks take it): a
hand detail.

## No fit: a hand detail

An end that does not fit is **left exactly as it was** and listed with the
reason (two members within a few degrees: no room on one plate; an end
running towards the angle's other leg on an inner face; bolts that cannot
pass the flange within the longest pull-back; a member that no single
roll lays flat at both its ends); the rest of the joint is laid out.
After writing, the joint details and the [clash check](#the-clash-check)
are worked out again. Where a laid-out part would run into anything, the
members of it — a member, its bolts, or the members on a plate — are left
as they were, with the clash ("laid out, … would run 6 mm into each
other"), and the rest of the joint is planned and checked again without
them (since 2026-11-18; before, the whole joint was left). Of two parts
laid out in one clash, the member is left before the members of a plate,
then the redundant before the bracing, then the larger id. After four
such rounds a joint still clashing is left whole, as it was. Nothing is
forced: what is written runs into nothing but the members left for a
hand detail (whose clashes are said with them).

## The clash check

Every solid at a joint — each member's steel as placed from its cut to
half its length, plates, covers, each bolt's head, shank and nut — is cut
into convex pieces and each pair of pieces of parts that do not bolt to
each other is tested by separating axes (the faces' normals and the
edges' cross products: exact for convex polyhedra). A pair overlapping
by more than 1 mm along every axis is a clash, the depth the least of
those overlaps (the 1 mm: the drawings' rounding and touching faces; the
program's own tolerance, not a standard's).

## Deterministic, and following edits

The same file and settings give the same layout: a run first takes out
the values a run set before (the record; the members' rolls back to what
they were), so that it starts from the same joint; ties are broken by
member id; lengths are rounded as written (a tenth of a millimetre, the
cut up to the millimetre). A run again on its own result changes nothing
(tested, and on the 78 tower files of the real line measured below).

An edit near a laid-out joint (a member's section, roll or bolts, a
plate's thickness, a value at an end, a joint moved, a setting) lays that
joint out again in the same undo step; a value the user changed is theirs
and kept. A plate made thicker moves the members on it out (their seat)
and lengthens their bolts (the grip), as before; the cuts follow.

Only the joints an edit touches are laid out again, and their details are
worked out from the file cut down to what they read (since 2026-11-18,
`ohl_structure_link::reduce`): every joint, the members meeting at them,
the members meeting those members' other joints (an end's conditions
read the other end's: one end eccentric, the other not; what a member
bolts onto holds it by its stiffness against the members at that joint),
the members they name (what they bolt onto, run on into, a crossing
partner), their connections, the plates and splices at their joints and
the layout record's marks of their ends. The details of those joints are
the same as from the whole file — tested on invented towers, and the
files written by the follow compared byte for byte with the follow
working from the whole file on edits of seven real towers (below).

## The settings

None of them is a standard's value (the owner gave none, 2026-11-18):

| Setting | Start | Why |
|---|---|---|
| Gap between steel | 10 mm | invented starting value |
| Longest pull-back | 300 mm | measured on the 78 tower files of a private real line (579 to 3 030 members): with no limit, the concentric cut of the ends laid out is at most 136 mm for 95 % of the ends of every tower (100 mm for half of the towers), the largest 344 mm; 300 mm keeps the long plates to members at small angles |
| Shift limit | 30 mm | measured on the same towers: with the pull-back at 300 mm, 37 of 18 289 ends laid out needed a shift, the largest 20 mm |

The window shows each member's usage before and after for the last run's
cases and the time the layout took.
