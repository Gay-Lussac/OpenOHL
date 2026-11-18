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
as a choice, welded together. On an inner face and on a built-up leg no
plate is laid out (a hand detail).

## No fit: a hand detail

An end that does not fit is **left exactly as it was** and listed with the
reason (two members within a few degrees: no room on one plate; an end
running towards the angle's other leg on an inner face; bolts that cannot
pass the flange within the longest pull-back; a member that no single
roll lays flat at both its ends); the rest of the joint is laid out.
After writing, the joint details and the [clash check](#the-clash-check)
are worked out again: a joint whose laid-out parts would still run into
anything is left as it was, with the clash. Nothing is forced.

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

## The settings

None of them is a standard's value (the owner gave none, 2026-11-18):

| Setting | Start | Why |
|---|---|---|
| Gap between steel | 10 mm | invented starting value |
| Longest pull-back | 300 mm | measured on the 78 tower files of a private real line (579 to 3 030 members): with no limit, the concentric cut of the ends laid out is at most 136 mm for 95 % of the ends of every tower (100 mm for half of the towers), the largest 344 mm; 300 mm keeps the long plates to members at small angles |
| Shift limit | 30 mm | measured on the same towers: with the pull-back at 300 mm, 37 of 18 289 ends laid out needed a shift, the largest 20 mm |

The window shows each member's usage before and after for the last run's
cases and the time the layout took.
