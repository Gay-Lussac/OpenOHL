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
| 1c | Other sections in line that do not nest, both with their bolts (a splice where the section changes), nothing else bolting onto them at the joint: each end bolted onto a **splice of covers**, outer and inner on each leg bolted ([below](#splices-step-1c)) | yes (2026-11-19); with an end bolting onto them there: as before (its bolts would pass through a cover: by hand) |
| 2a | The next members, by class and size, **bolted directly on a flange** of the bigger member where their bolts land on it | yes (as before) |
| 2b | Where not: **onto a plate** lying on that flange (in its plane) | yes (as before; every end of that flange's face onto the plate) |
| 3 | The smaller classes (redundants) the same way, after the bracing | yes (as before: the order bracing, redundants) |
| 4 | An end whose angle of arrival allows no flat fit on a flange or a plate in a flange's plane: a plate in its own plane, **welded** to a plate bolted to the bigger member ([below](#a-welded-plate-in-the-members-own-plane-step-4)) | yes (2026-11-19), where its member then lies at most a plate's thickness farther out than on a flat plate; else a hand detail, said |

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
  bolts): each a disc of its head's corner radius and half the gap;
- (since 2026-11-19, J1) the **fixed steel** of the joint where it reaches
  into the end's layer, by the gap: every part not laid out in this plan
  (the members in line, running on, not single angles, given by hand, and
  those left for a hand detail for their own sake — [below](#members-left-for-a-hand-detail-keep-their-place-j2)
  —, their bolts' heads and nuts, the plates and covers given by hand) and
  what the planes before laid out ([the search](#the-search-per-joint-j1)).
  Each piece the clash check cuts the joint into is clipped to the end's
  layer — from its face to two of its legs' (or the flange's, the
  thicker) thicknesses and 0.7 bolt diameters above it (its leg on a
  plate and its bolts' heads) — and seen from above as a convex polygon;
  the least cut is the farthest point of it, along the end, within the
  end's leg widened by the gap. Not what the end bolts onto (nor a member
  running on from it in line), nor what bolts onto the end; a bolt's shank
  through those is not in the way (its head and nut are). **An assumption
  of the tool**: the layer's height is the program's, not a standard's;
  steel above it is left to the clash check after writing, so nothing is
  forced either way.

The cut is rounded up to the millimetre. A cut beyond the **longest
pull-back** (a setting) or beyond half the member less its bolts does not
fit concentric.

## The search per joint (J1)

(Owner, 2026-11-19: "plan ideas to get the automatic layout working",
approved with the recommendations; built the same day, in active
development.) Before, the ends of a plane were placed one after the other
in the owner's order, and one that did not fit was left — though the
others pulled back could have made room for it. Now each joint is
**searched**:

1. **The planes on the biggest members first** (on a leg, then on the
   bracing, then on the redundants; then by the member's id): what bolts
   onto a member laid out on a leg keeps clear of that member as laid out —
   its steel as a box from its cut to half its length, its outstanding leg
   as high as it is wide, its bolts' heads, a plate under it as the hull of
   its bolts with their edge distance. **An assumption, on the safe side for
   the fit** (the boxes hold the angle and more); what the boxes miss is
   caught by the clash check after writing.
2. **In each plane, every order that can matter.** At each step every end
   not placed yet is tried — concentric and pulled back, else the least
   sideways shift that fits ([above](#moved-sideways-the-shift)) —; an end
   that fits nowhere now fits nowhere later (each end placed only adds what
   the others keep clear of), so it is left; each end that fits is placed
   in turn, **in the owner's order** (class, the wider leg, the larger
   angle, the id), and the rest searched again (backtracking). A placing
   already reached in another order is not searched again.
3. **The best**: the most ends placed; then the least eccentricity (the
   moves off concentric, summed); then the first found. The first found
   is the owner's order filled one after the other (as before the
   search), so the owner's order wins every tie — the search only departs
   from it where that lays out more or moves less. Typically a smaller
   member placed first and a bigger one pulled back past it, where the
   smaller one cannot be pulled back far enough within half its length
   less its bolts.
4. **Bounded**: at most **400 placings per joint** (`NODE_LIMIT`, the
   program's starting value — measured on the private real towers: see
   below —, not a standard's); a joint whose search stops there takes the
   best found and says so with each end it leaves ("… (the search stopped
   at its limit of 400 tries)").
5. **Pulled back past a clash** (the 3D search): after writing, a member
   whose laid-out steel, bolts or plate would run into anything is first
   pulled back past it — its least cut the point of the clash along it,
   the depth and the gap beyond its cut — and the joint planned again; up
   to three times per end while rounds are left (six rounds per run, the
   last two leaving such members as they were), then it is left as it was
   with the clash, as before. A cut given by hand is never pulled back.

Deterministic (the same joint and settings give the same plan: every
order and tie fixed by the members' order and ids), no new clash (what is
written runs into nothing but members left for a hand detail whose place
is not kept free, below), values given by hand kept (a cut, a shift, a
roll, what an end bolts onto: the search places round them). A longer cut
changes no member force (the analysis runs on the members' axes); a
shift, as before, is an eccentricity the analysis takes.

## Members left for a hand detail keep their place (J2)

(Owner-approved 2026-11-19.) A joint that is not laid out whole keeps the
ends that fit; the members left are named with why ("N2: by hand R1 (its
place kept free), the rest laid out — R1: too short for its bolts …").
A member left **for its own sake** — it fits nowhere even alone: no roll
lays it flat, on an inner face running towards the other leg, built up,
too short for its bolts, its cut given by hand in the way — **keeps its
place as it is**: every end laid out keeps clear of its steel and bolts as
they are (in the plan, and in the clash check after writing, where a clash
with it counts as any clash), so the joint then needs only that member by
hand. **Not kept free** — said with what it runs into, as before:

- a member left for want of room beside the ends laid out ("no room on one
  plate", pulled back to clear one of them) or for a clash after writing:
  it runs into them as it is, and its hand detail takes it out of their
  way (it is the one that does not fit);
- a plate laid out under a member left (its hand detail may bolt it onto
  that very plate), and the members joined first (one part, nested,
  spliced — the biggest profiles, the owner's step 1).

**An assumption of the tool**: the place kept is the member as it is in
the file, not where its hand detail will put it; a hand detail that moves
it elsewhere may need the ends round it laid out again (Lay out joint…
after the hand detail). Measured on two private real towers, keeping the
place free costs 0.2 and 1.7 % of the ends laid out (against the members
left kept clear of in the plan only); keeping the place of every member
left free — also of those left for want of room, plates under them refused too — cost a quarter to a third of them
(tried, left out).

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

## A welded plate in the member's own plane (step 4)

(Owner, 2026-11-18: "finaly if the angle at which the profile is coming
really doesn't allow for it then add a weleded plate that is also bolted
to the other profiles with the other part of the plate to fix it"; built
2026-11-19.) An end whose leg bolted lies more than 5° off the flange it
bolts onto and that no roll lays flat at both its ends (a hip or plan
member flat on a horizontal at one end and arriving off the leg's flange
at the other, a crossarm's face member at the chord) is laid out on two
plates welded into one: a plate **in its own plane** (its leg bolted lies
on it, its bolts through it) and the **flat plate** of its face on the
flange (as step 2b's, the face's other ends bolted onto it), joined where
their planes meet — a plate bent there, made of two plates welded.

- **Which face.** The flange in whose plane its axis lies most nearly
  (else the other one); on the flange's outer face, or on its **inner
  face** when it comes from the flange's other side (its axis behind the
  outer face — hip and plan members inside a leg's corner) or lies along
  it with its leg bolted facing away. Its leg bolted must face that face
  (else half a turn is tried, as for an inner face; else a hand detail),
  lie at most 75° off it (`TILT_MAX`, the program's starting value: the
  plate would stand nearly square to the flat one), and it must leave the
  joint away from the heel on an inner face (the angle's other leg) and
  not run down towards the flange more than 5°.
- **On the flange's plane.** Its strip is laid out with the others (its
  cut, a shift) as its section's and its plate's footprint on the
  flange's plane, its lengths along it as they lie on that plane; its
  bolts past the flange's toe (or heel) by the nut and the gap as on a
  plate, and its plate past it by half the gap (under it, near the seam,
  the plate lies as low as the flat one).
- **The seam.** The member is moved along the flange's normal (so it
  stays where it was laid out across the flange; the drawing's seat,
  `PlateSeat::lift`) the least that keeps its leg bolted, from where its
  steel stops to past its bolts and to half its length, the gap (the
  weld's room) above the line where its plane meets the flat plate's
  face, and its bolts the edge distance (the code's in a plate of its
  leg's thickness, else the default minimum) from that line. Its plate:
  round its bolts with the end and edge distances (along and across the
  member, as a plate is drawn), down to the seam. The flat plate: round
  its own ends' bolts and the flange from the work point as far along as
  the seams reach, heel to toe, kept to its side of each seam and cut at
  the heel by half the gap; it must reach the flange over at least half a
  square of the flange's width (room for its own bolts to it — the
  program's starting value, not a standard's) and keep every bolt of its
  ends the edge distance from each seam.
- **The weld.** Both plates as thick as the thickest leg on them; welded
  from both sides along the seam, each side a throat of four tenths of
  that thickness in whole millimetres, at least 3 mm (`weld_throat`: a
  partly penetrated weld from both sides, **the layout's choice, not a
  standard's**), its length both sides together (`[[plate.weld]]` with
  `size` and `length`, automatic in the record). SIA 263 6.3.3.5 takes a
  partly penetrated weld as a fillet weld: the weld check of
  [plates](joint-plates.md) applies as it is (its force, its moments about
  the seam). The members of each side lie in their plate's plane, so the
  moment about the seam's own line is nil for one welded plate on each
  side; where two plates in their own planes are welded to one flat
  plate, the moment of the other's member about a seam is not nil and the
  check says it **NOT ON THE SAFE SIDE** as for any weld bent about its
  seam (6.3.6.5: the data say the weld's length on both sides together,
  not that it is two fillets — a field and a check change, not made:
  a question to the owner). ASCE 10-97 has no weld rule: not checked,
  said.
- **Its line of action.** The member lies on its plate, so farther out of
  the flange than on a flat plate: by the gap's and its leg's rise over
  the tilt. The analysis keeps every member's axis through the work point
  (as for any gusset: its thickness and the leg's seat are drawn, not
  analysed). **Assumption, on the safe side by a limit:** a welded plate
  is laid out only where it puts the member's axis, at its bolts, at most
  **one plate thickness** farther out of the flange than a flat plate of
  the same thickness would (about what a filler plate adds); farther, the
  end is left for a hand detail with why ("its axis would lie 25 mm
  farther out of the flange than on a flat plate … not in the analysis").
  The joint eccentricity of a gusset (its thickness, the leg's seat) is
  not in the analysis anywhere — usual for lattice towers, **not a
  standard's rule**; the limit keeps a welded plate within it.
- **Never forced**, deterministic and followed as every plate: the
  plates' outlines and the weld are automatic values of the record (the
  plate in its own plane marked `own_plane`, so the joint details seat
  its member on it), a value given by hand on them is kept, and the clash
  check tests the plate in its own plane against the flange too (it does
  not lie on it).

## Splices (step 1c)

Two angles in line through the joint (within 5°, their legs along each
other within 5°: the nesting tolerance), of **other sections** that do
not nest, **both with their bolts** — where the section changes, a splice
must be — and with **no other end bolting onto them there** (nor onto
nothing named, which would be named one of them) are joined first: a `[[splice]]` at the joint, its covers on
both faces (outer and inner) of each leg bolted (both legs where the
ends bolt by both), as thick as the thicker member's leg, both ends
bolted onto it (`onto` the splice). Its bolts then pass through the
member's leg and two covers: two shear planes worked out from the plies
(a count given still wins, said where it differs), the covers checked
as plates for their share of the force by area — the checks of
[laps and splices](joint-laps-splices.md), unchanged. The members are
drawn on their work lines, butt at the joint (not lapped). A splice the
layout made is automatic (the record's `made`, `splice` and its covers'
thickness): a run again makes it again; one whose covers were changed by
hand is kept, the user's. Where an end bolts onto them at the joint —
on the private real towers every leg splice of other sections has its
bracing there (measured 2026-11-19: tried, every such splice's cover ran
into the bracing and left it, and joints laid out whole before were
lost) — no splice is made: the bracing's bolts would pass through a
cover, a detail by hand; the members stay as given.

**Not known where:** two angles in line of **one section**, turned alike,
with bolts (a leg's splice bolts are its members' design data, at both
ends of every leg member): spliced here, or one part through the joint
and spliced elsewhere — the data do not say. They are left as given
(drawn lapped, as before) and the joint says so ("… where the splice is
is not known …"), first among its reasons when the rest of it is left
too; giving the splice (or the two one part) lets the layout lay the
rest out. Never a splice bolt check dropped: one part is made only where
neither end has bolts (step 1a).

## No fit: a hand detail

An end that does not fit is **left exactly as it was** and listed with the
reason (two members within a few degrees: no room on one plate; an end
running towards the angle's other leg on an inner face; bolts that cannot
pass the flange within the longest pull-back; a member that no single
roll lays flat at both its ends whose welded plate does not fit, with
why); the rest of the joint is laid out. A splice the layout made counts
as a part of the two members it joins: a cover running into anything
leaves them as they were.
After writing, the joint details and the [clash check](#the-clash-check)
are worked out again. Where a laid-out part would run into anything, the
members of it — a member, its bolts, or the members on a plate — are left
as they were, with the clash ("laid out, … would run 6 mm into each
other"), and the rest of the joint is planned and checked again without
them (since 2026-11-18; before, the whole joint was left) — since
2026-11-19 (J1) after being pulled back past the clash up to three times
first ([the search](#the-search-per-joint-j1), point 5). Of two parts
laid out in one clash, the member is left before the members of a plate,
then the redundant before the bracing, then the larger id. After six
such rounds a joint still clashing is left whole, as it was. Nothing is
forced: what is written runs into nothing but the members left for a
hand detail whose place is not kept free (whose clashes are said with
them; [J2](#members-left-for-a-hand-detail-keep-their-place-j2)).

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

Not settings, the program's starting values (J1, 2026-11-19), measured on
the same towers:

| Value | Start | Why |
|---|---|---|
| Placings searched per joint | 400 | never reached on the 78 tower files (the most ends of one plane there: a handful); 1 (no search) gave the same shares there, 2 000 the same layouts on the two towers tried |
| Pulled back past a clash | 3 times per end, 6 rounds | 0 (4 rounds): 33.9 % of the joints laid out whole instead of 44.1 %, 53.9 % of the ends instead of 62.8 %; 2 (5 rounds): on two towers the same joints whole, about 1 % fewer ends, about 10 % quicker |

## Measured (J1 and J2, 2026-11-19)

On the 78 tower files of a private real line (Orient angles, then Lay
out joints, values given by hand replaced; the same files and settings as
the rounds before):

| | before (2026-11-19) | J1 and J2 |
|---|---|---|
| joints laid out whole | 27.6 % | 44.1 % |
| member ends laid out (of those the layout places) | 48.9 % | 62.8 % |
| joints with a hand detail | 66.2 % | 49.7 % |
| clashes at the joints after (of before) | 61.6 % | 46.3 % |
| a run again identical | 78 of 78 | 78 of 78 |
| time of a whole tower (2 190 to 3 030 members, measured one after the other) | 6–9 s | 10–15 s |

Where it came from: the joint's fixed steel and what the planes before
laid out seen in the plan, and the ends pulled back past a clash after
writing. The search over the orders of a plane's ends changed nothing on
these towers (its first path, the owner's order, was the best found at
every joint; it lays out the synthetic joints of the tests that the
owner's order filled one after the other leaves) — kept, as it costs
nothing measurable there, but it **stops paying** on real towers: their
planes hold few ends, and what is left is not a matter of order.

What is left (joints with it; only it, of the joints with a hand detail):
a member that no roll lays flat at both its ends, its welded plate not
fitting (39 %; 13 %) — the turn of the whole member, J3, or a bent
plate —; a clash after writing (39 %; 18 %); an end on an inner face
running towards the other leg (29 %; 13 %); a built-up member (20 %;
13 %); too short for its bolts (6 %; 3 %).

The window shows each member's usage before and after for the last run's
cases and the time the layout took.
