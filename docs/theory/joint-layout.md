# The automatic joint layout

*Built 2026-11-18, in active development.* How **Lay out joints**
(OHL-Structure, [user guide](../user-guide/joint-layout.md)) places a
tower joint's member ends so that they fit, and what that does to the
analysis and the checks. The code: `ohl_structure_link::joint_layout`
(the plan of one joint, a pure function), `layout_run` (the run on a
file, its record, following later edits), `clash` (the 3D check),
`joint_order` (the body, the arms and the top; L1).

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
  fraction of a degree; for a triangular crossarm 15° — more than the 2°
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
| J2b | An end flat on a flange's inner face but leaving over the heel (through the angle's other leg): a **bent plate** — a flat in its plane folded round the heel onto the other leg, bolted there ([below](#a-bent-plate-round-the-other-leg-j2b)) | yes (2026-11-19), after the half turn onto the outer face; else a hand detail, said |

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
  (leg, bracing, redundant), the wider, then — of two pieces in line not
  one part (J9, 2026-11-19) — the piece on the end's side (whose steel
  lies under its bolts: such pieces each end at the joint), the id.
- **The joint's leg, but in neither of its flanges' planes** (a plan
  diagonal meeting a vertical post; within 2°): a single angle running
  through the joint in a flange's plane of which the end's axis lies (the
  horizontal running through), chosen in the same order. The end then
  bolts flat onto that member, as built; none: the joint's leg kept (and
  the end is left for a bent plate or a hand detail, said).

What an end bolts onto is what its restraint is worked out from (the
code's stiffness rule, as for any end bolted onto a member): the member
it is laid out onto, as detailed.

**Two ends bolting onto each other** (the mutual-host guard, the owner's
decision D16 of 2026-11-19, built with J9): where the ends named would
bolt onto each other — or one onto a piece in line with the other (the
pieces of a horizontal onto a redundant standing on them, the redundant
onto the horizontal) — laying out both would cut each back onto the
other, the bolts of one beyond the other's steel: physically impossible
(measured on the private real towers before: 5 128 joints). The one
running through the joint (in line with another of its members), else
the larger (class, the wider leg, the id), stays what the other bolts
onto; its own end is **left as it is**, said ("H1: it would bolt onto V
and V onto H2 (in line with it): … its own end left as it is"). Alone,
the guard leaves about a quarter of the ends laid out before (most of
them pieces in line); with J9's pieces confirmed one part, those run on
and nothing is left for it.

## Turned only where it helps

(Owner, 2026-11-18: "only turn where it helps".) A member lying off what
it bolts onto (more than 2°, `FLAT_OFF`; 5° until the owner's decision of
2026-11-19) is turned by the layout only when

1. one roll lays it flat at all its ends — those at joints not laid out
   in this run too (**one roll for the whole member**, J3 below): the
   turns that make each end flat lie within 2° of each other (a turn and
   a half turn more lay a leg flat alike; an equal-leg angle's other leg a
   quarter turn more); the middle of them is taken, so a turn never spoils
   an end that was flat;
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

### One roll for the whole member (J3)

(Owner, 2026-11-19: "each angle's turn chosen for the whole member", then
the same day "you can do 90 degrees turn or more if necessary also start
with aligning the main legs then diagonals and finally secondary braces
with redundant members being last"; built the same day, in active
development, not approved yet. The code: `ohl_structure_link::whole_roll`,
`layout_run`.) An angle's roll is one value for the member: its two ends
cannot be laid flat each on its own. The turn is chosen from **both ends'
needs together**:

- **Each end's turns.** At each end, the turn that lays its leg bolted
  flat on the flange it bolts onto, and that turn and a half turn more
  (the same leg on the flange, its outstanding leg the other way). For an
  **equal-leg angle** whose leg bolted is not given as both legs, a
  quarter turn more as well: its *other* leg then lies on the flange — so
  **the leg bolted may change between its ends**, as it does on towers
  where a member runs from a horizontal flange to an upright one (a hip
  or plan member, a crossarm's bracing). Both legs of an equal-leg angle
  are alike for its check (one leg bolted, the eccentricity gauge to
  centroid), so nothing in the checks changes with it. An unequal angle's
  leg bolted is its design data (the long or short leg in its check):
  never changed by a quarter turn.
- **Within 2° of each other.** A turn of every end, those lying within
  2° of each other (each end then within 1° of flat after the middle of
  them is taken; `FLAT_OFF` — **the owner's value** (2026-11-19: "2° for
  both"; 5° before), not a standard's: a leg bolted within 2° of its face
  is drawn and checked as flat on it, the bolts pulling it flat — **an
  assumption**, the smaller the nearer the safe side), as before J3.
  **Tried and left out** (measured on the private real towers,
  2026-11-19, at the 5° of then): taking a turn that leaves every end
  within the whole 5° of flat — the ends' turns up
  to 10° apart, an end out of its face's plane counted with it — laid out
  slightly *fewer* member ends (on ten of the towers 16 319 against
  16 336): the ends then lie up to 5° off and run into more after
  writing, and most members whose ends need turns 5° to 10° apart have an
  end whose axis runs out of the flange's plane besides.
- **Any turn, the smallest that does it** (owner: "90 degrees turn or more
  if necessary"): the same legs at both ends first (so a member laid flat
  that way before keeps its turn), the leg changing only where that alone
  serves both ends; of several such turns the smallest. Else **not
  turned** — never forced — and said with both ends' turns ("… by
  different turns (one roll for the whole member: the turn would spoil
  the other end, whichever leg is bolted): not turned"); its ends left
  off their flange go to step 4's welded plate or a hand detail, as
  before.
- **Class by class** (the owner's order): the **legs and chords** are not
  turned to suit what bolts onto them — their roll is the tower's shape
  (Orient angles: heel out, flanges in the faces); then the
  **diagonals** (bracing, class `bracing` or `crossing`, not lying
  horizontal); then the **secondary bracing** — horizontals, plan and hip
  members (bracing lying horizontal within 5°, or a plan member as Orient
  angles turned it) and the members of **no known class** (said: "no
  class given: turned with the secondary bracing"); the **redundants**
  last. Each class's turns are found with the classes before it turned
  (the joints worked out again in detail between classes), so a
  horizontal's turn fits the diagonals as they will be. The same order
  ranks a plane's ends in the search (below) and the planes, and which of
  two members in one clash is left first (the later class).
- **Kept as before**: a roll given by hand (unless *Replace values given
  by hand*), a member joined as one part, a member laid out elsewhere
  with its roll now; the turn made only where it helps (rule 3 above).
- **Orient angles agrees**: it keeps a plan or face member's roll the
  layout chose for both its ends (the record's `rolled`; listed in its
  window, *Rolls the joint layout chose for both ends kept*), and the
  layout starts from Orient angles' roll and departs from it only where
  the ends need it — a run of either after the other changes nothing.
  The **estimated eccentricities** (O45) plan with the same choice (the
  layout's planner run relaxed).

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
  (in the owner's order of the classes — diagonals, secondary bracing,
  redundants, [J3](#one-roll-for-the-whole-member-j3) —, then the wider
  leg, the larger angle to what it bolts onto, the id): the least cut is the farthest point, along this
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

1. **The planes on the biggest members first** (on a leg, then on a
   diagonal, on the secondary bracing, on a redundant — the owner's order
   of the classes, 2026-11-19; then by the member's id): what bolts
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
   in turn, **in the owner's order** (the class — diagonals, secondary
   bracing, redundants —, the wider leg, the larger angle, the id), and the rest searched again (backtracking). A placing
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

At level 3, where the joints are not laid out, the option **Estimated
eccentricities** (O45, 2026-11-19) takes the shifts this planner would
give — run relaxed, nothing written — and the shift limit as an upper
bound where no layout fits: [Estimated eccentricities](estimated-eccentricities.md).

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

**On the steel of what it bolts onto** (J9 and D16, 2026-11-19): where the
member an end bolts onto **ends at the joint** — not one part through it
(`continuous`): pieces in line not confirmed one part, a member whose own
end is laid out here — the end bolts on directly only where every bolt
lies on that member's steel, past where it starts by the end distance (the
code's, else the default minimum): its cut as laid out in this plan, else
as given or worked out, else the work point. Otherwise the plane goes onto
a plate (as for bolts off the flange), else a hand detail. The planes
whose ends bolt onto a member whose own end is laid out here are planned
after that member's plane, so its cut is known. *Conservative*: the
pieces of a member in line are taken to end at the joint until the user
confirms them one part (the joint checks' own rule, O19 stage 4.5); a
bolt within the end distance of a cut is not counted as on the steel.

## A welded plate in the member's own plane (step 4)

(Owner, 2026-11-18: "finaly if the angle at which the profile is coming
really doesn't allow for it then add a weleded plate that is also bolted
to the other profiles with the other part of the plate to fix it"; built
2026-11-19.) An end whose leg bolted lies more than 2° off the flange it
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
  not run down towards the flange more than 2° (`FLAT_OFF`).
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

## A bent plate round the other leg (J2b)

(Owner, 2026-11-19: the example joint of a gusset plate folded along a
line so that its two flats follow two planes, members bolted on it from
both; approved the same day, before J3; built the same day, in active
development.) An end lying on a flange's **inner face** that leaves the
joint **over the heel** — towards the angle's other leg, through which it
would run — fits neither on the flange nor on a flat plate in the
flange's plane (that plate would have to pass through the other leg). It
goes onto a **bent plate**: one flat **in the end's own plane** (where
it lies, beyond the other leg's outer face), folded along the angle's
line — the line of the member it bolts onto — round the heel onto the
**other leg's outer face**, the second flat, bolted to that leg. Typical:
a plan or hip member arriving from inside a tower at a face horizontal
whose second leg points out of the tower.

- **The order.** A flat fit first: the half turn that lays such an end on
  the flange's outer face ([above](#turned-only-where-it-helps)) is
  checked as before the bent plates (the run plans its half turns with
  them off), then the bent plate, then a hand detail. The welded plate of
  step 4 is for an end no roll lays flat; the bent plate here is for an
  end flat on its plane but on the wrong side of the heel. A bent version
  of step 4's plate is not built: with the same limit on its offset it
  would lay out about the ends the welded plate does (its bend's room is
  about the weld's) — a question to the owner (bent before welded, as
  galvanised towers are rarely welded?).
- **Where the end lies.** As it is drawn on the flange's inner face (its
  gauge line through the work point): only pulled back. The plate takes
  its leg's place — the flat in its plane lies where the flange would be,
  beyond the other leg — so it lies **no farther out** than directly on
  the flange (a flat plate would put it a plate's thickness farther):
  nothing new for the analysis, the plate offset of the estimated
  eccentricities 0. Its steel keeps beyond the other leg's outer face the
  plate's thickness, the bend's inside radius and half the gap; its bolts
  the plate's thickness, the radius and their edge distance in the plate
  (the code's, else the default minimum) — **an assumption on the safe
  side**: the plate runs on round the bend, no free edge there.
- **The plate.** As thick as the thickest leg on it (as every plate the
  layout makes); its **inside bend radius two thicknesses**, up to the
  millimetre (`BEND_RADIUS_RATIO`) — **the program's starting value, not a
  standard's**: the least radius a plate may be bent to cold is the
  steel's (its grade, thickness and the bend's direction to its rolling),
  to check by hand; a radius given on the bend is kept. The bend's angle
  is the angle between the two legs (90° for an angle).
- **The flat in the end's plane**: round its bolts with the end and edge
  distances (along and across the member, as a plate is drawn), to the
  bend line — where the two flats' middle planes meet. The ends of one
  plane share it.
- **The flat on the other leg**: from the bend line to that leg's toe, as
  far along as the bend and **two bolts onto the leg** (`[plate.leg_bolts]`,
  provisional: the end's bolt, the default spacing apart about the bend's
  middle, in one gauge line): their head clear of the bend — the inside
  radius and the nut's corner past the flange's thickness — and their nut
  clear of the flange's other leg and root, the edge distance (the
  code's, else the default minimum) from the leg's toe. Where the leg is
  too narrow for that the end is left for a hand detail, said ("no room
  for its own bolts on … other leg (55 mm wide; they need 36 mm from its
  heel and 24 mm from its toe)"). **An assumption of the tool**: the
  head's clearance from the bend is the program's rule, not a standard's.
  The provisional bolts keep the tower at level 3, as every provisional
  bolt.
- **The other leg's outer face.** Where ends are laid out there on a flat
  plate (step 2b), that plate is the bent plate's flat: as thick as the
  thicker of the two, its outline reaching over the flat's place too,
  bolted to the leg as above. Where an end is laid out there directly on
  the leg (step 2a) and the flat would lie under it, or ends there are on
  more than one plate, the end over the heel is left for a hand detail,
  said. The clash check after writing tests the rest, both flats as
  slabs to the bend line (a sharp corner: more steel than the bend has —
  on the safe side for the fit).
- **Its checks.** The two flats are two plates of the file, one part
  (`[plate.bend]`, [data model](../development/data-model.md)): each is
  checked as a flat plate with the members bolted onto it — their bolts,
  bearing, tearing, the Whitmore section — through the plate checks as
  they are; the flat on the leg is held by its own bolts onto the leg (the
  joints' own checks: the holding bolts with the resultant of the
  members' forces on the part and its moment, the leg's bearing under
  them; their tension with prying where the code's plugin works it out,
  [joint groups](joint-groups.md)). The member's force crosses the bend
  square to the flat on the leg; the **bend's own bending** under it is
  checked since 2026-11-19 by the code's plugin ([below](#the-bend-in-bending-2026-11-19));
  a plugin without that check: not checked, **NOT ON THE SAFE SIDE**, said
  on the row. No weld.
- **Its weight**: from the developed area — the two flats to the bend
  line less, per metre of bend, (r + t/2)(2 tan(θ/2) − θ): the straight
  lengths to that line against the arc, both at the middle of the
  thickness (**an assumption**: of the usual lines that keep their length
  the one giving the longest arc, the heavier count).
- **Never forced**, deterministic, followed as every plate: the bend and
  the flats' outlines are automatic values of the record (the flat in the
  members' plane marked `own_plane` and `bend`, the joint details seating
  its members on it square to it), a value given by hand on them kept.

### The bend in bending (2026-11-19)

(The owner's decision of 2026-11-19; `ohl_structure_link` `bends`, the
plugins' `joint-check-bends`.) The section along the bend line — its
width the bend's length, its depth the plate's thickness — is a row of
the joint checks ("plate P1/P2, its bend") per load case.
OpenOHL works out what acts on it from the axial forces Nᵢ of the members
on the flat in their plane (each along its direction dᵢ from the joint;
a the flat's direction square to the bend line, away from it; l along
the bend line; n the flat's normal; o the bend's middle):

- across the bend N = Σ Nᵢ (dᵢ · a); along it V = |Σ Nᵢ (dᵢ · l)|;
- about the bend line M = Σ |Nᵢ (dᵢ · a)| eᵢ + |N| sin θ · d — eᵢ the
  distance of member i's force line out of the flat's middle plane (half
  the plate's thickness and the angle's centroid from the back of its leg
  bolted; another section: its steel's farthest point from its axis),
  θ the angle the plate turns through at the bend, d the lever from the
  bend line to the farthest bolt holding the other flat on the leg (else
  that flat's farthest edge, said);
- in the plate's plane M_in = |Σ Nᵢ ((cᵢ − o) × dᵢ) · n|, cᵢ the centroid
  of member i's bolts.

*Conservative assumptions:* the members' moments about the bend line
added by size (the worst of their signs); the whole force across the
bend turned by the bend's angle and held at the farthest holding bolt,
the largest moment that flat's cantilever can give, **taken at the
bend** (where the flat bears on the leg's face, less); the normal and
the shear stresses taken at one point of the section (the codes'
checks below). **NOT ON THE SAFE SIDE** (by what it leaves out): the
members' axial forces only — a frame member's end moments and shear at
its bolts are not added at the bend (the flats' plate checks take them);
the bend short across its section — no buckling across it (the flats'
own strut checks take theirs).

The plugins apply their code to that section: **ASCE 10-97** Eq. 3.13-1
(P/P_a + M/M_a, P_a = F_y A on the gross section by 3.10.1, the
allowable moments F_y S of the gross elastic section by 3.14.1–3.14.3,
S = w t²/6 and t w²/6), with the shear along the bend by the yield
criterion of von Mises (R. von Mises, *Mechanik der festen Körper im
plastisch-deformablen Zustand*, Göttinger Nachrichten, 1913):
√(σ² + 3 τ²) ≤ F_y — the standard has no rule for it, **on the safe
side**; it gives no least bend radius (said). **SIA 263:2013** 4.3.5.4,
formula (6), the comparison stress of a plane state of stress
√(σ² + 3 τ²) ≤ f_y/γ_M1, the stresses elastic (5.2.1, 5.2.6: W_el —
**on the safe side**, no plastic resistance (5.1.3, W_pl) relied on in a
cold-bent zone); the inside radius against 5.7.1.2, Table 14 (the least
radius where a weld is made within 5 t of a cold bend: 1 t up to 3 mm,
1.5 t up to 6 mm, 2 t up to 12 mm, 3 t up to 24 mm, 10 t beyond) — a
smaller radius on a bent plate with a weld (where, not known) said NOT
ON THE SAFE SIDE, without a weld a note (the table does not apply). The
program's two thicknesses (`BEND_RADIUS_RATIO`) stays the starting
value (the owner's decision of 2026-11-19: no shop value given).

## Splices (step 1c)

*Since O44 J7 (2026-11-19) the layout no longer makes these splices
itself: never written silently, they are proposed with the leg splices
([below](#leg-splices-proposed-j7)) — both pieces of no leg, with bolts,
nothing else on them, as here — and written when the user confirms.*
A splice an older run made is taken out by a run again (as here) and
proposed again. What follows is the rule as it was:

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
too; giving the splice (or confirming the two one part, J9) lets the layout lay the
rest out. Never a splice bolt check dropped: one part is made only where
neither end has bolts (step 1a), else where the user confirms the pieces
one part ([J9](#pieces-in-line-one-part-confirmed-j9); the preview
proposes them).

## Pieces in line: one part, confirmed (J9)

(The owner's decision D15 of 2026-11-19; built, in active development,
not approved yet.) Two pieces of one member line through a joint —
**the same section**, in line within 1° (the core's angle for one part,
`CONTINUOUS_ANGLE_DEG`), the same reference and orientation, their rolls
within 2° — with **nothing given there** of how they are joined are
**proposed one part** (`kind = "continuous"`) in the layout's preview,
for the user to confirm per joint or all at once (*Confirm all*), in one
undo step. It is **never a silent default**: a run only says them; until
confirmed each piece **ends at the joint** — in the joint checks (their
steel from their cut; O19 stage 4.5) and in the layout (above) — *on the
safe side*: a splice, if there is one, keeps its bolts checked; one part,
the pieces' bolts there are not a splice (their members' design bolts
stay at their other ends).

**Pieces of a leg with bolts** (their own or their members' design data)
are leg splice places since J7 (2026-11-19): proposed one part by the
owner's rule and listed with the leg splices
([below](#leg-splices-proposed-j7)).

Not proposed, and said why:

- a **splice given** at the joint naming either piece, a connection's
  `kind` given at either end, an end given by hand to bolt onto something
  else, or bolts given at an end: the joint is detailed as something
  else;
- other sections, references or orientations, or rolls more than 2°
  apart.

**Rolls a little apart.** Orient angles turns each piece to its face on
its own, so pieces of one member often differ by a fraction of a degree
(on the private real towers up to about a degree). The core reads two
pieces one part only with the same roll; confirming writes every piece of
a chain confirmed together the roll half-way between theirs (the mean of
their rolls) — each turns by at most half the 2°. The 2° is the flatness
the joint details allow a leg bolted (`FLAT_OFF`): **the owner's value**
(2026-11-19: "2° for both"; 5° before), not a standard's. Such a roll is then the user's (no longer the
layout's: a later run does not turn it back), and the layout never turns
pieces one part.

Confirmed, the pieces **run on** through the joint: they are not laid out
(nothing to pull back), what bolts onto them bolts onto the member
running through, and the joint checks see one member there (its bolts'
end distances from its steel through the joint, no splice). A run
replacing values given by hand keeps them (one part is what the steel
is, not a layout value). The layout's own values at the two ends (a
previous run's cut, shift, `onto`) are taken out when confirming.

**After writing** (J9 and D16): an end laid directly onto a member is
checked against that member **as written** — laid out in the same run, or
left as it was by the clash rounds (then as the file has it, not as the
plan saw it: e.g. its own `onto`, named only in the planning copy, is
not written for an end left, so its leg bolted may differ): each bolt's
hole within that member's flange, and on its steel where it ends at the
joint and its cut is known — the joint checks' own rule. An end that
would not fit so is left as it was too, said ("laid out, its bolt …
would not lie within its flange … (left as it was)"), and the joint is
planned again without it, as for a clash.

## Leg splices proposed (J7)

(The owner's decisions of 2026-11-19 — "a change of section is a splice;
the same section is one part unless marked; a command proposes the
splice places and the user confirms"; "bracing bolted through the outer
splice cover: yes, after the splice places" —; built the same day, in
active development, not approved yet. The code:
`ohl_structure_link::leg_splices`.) The import gives a leg's splice bolts
as its members' design data: at **both** ends of every leg member, so
where the splices are is not in the data (on the private real towers
every leg piece but a few has them). The layout's preview proposes the
**splice places** of the legs; **nothing is written silently** — *Confirm*
per joint or *Confirm all*, one undo step each. Until confirmed each piece
**ends at the joint**, in the joint checks and in the layout (as for J9's
pieces in line): *on the safe side*.

- **A change of section: a splice.** Two pieces of a leg (both of class
  `leg`) in line through a joint (within 5°, their legs along each other
  within 5° — the nesting tolerance, `NESTED_ANGLE_DEG`, an assumption of
  the tool) of **other sections** (another section id: a change of
  thickness, size or steel) are proposed spliced: a `[[splice]]` at the
  joint, its **covers outer and inner on each leg bolted** (both legs where
  an end bolts by both; the model's flat covers), both ends bolted onto
  it. *Assumed, said in the proposal:*
  - the covers **as thick as the thicker piece's leg** — the program's
    starting value, not a standard's; the covers are then checked as
    plates for their share ([laps and splices](joint-laps-splices.md#covers)),
    which says when that is too thin;
  - the **bolts the import gives** (the pieces' design data); a piece
    without any gets **provisional** bolts (the preset for the class
    `leg`, else the bolt picked — two in one line), listed so: the tower
    stays level 3 until the codes choose them.
- **The same section: one part.** Pieces of one section of a leg with
  bolts are proposed one part — J9's pieces in line (the core's 1°, their
  rolls within 2°), flagged a leg splice place and listed here.
  Confirmed, their bolts are **not checked at that joint** (they are
  checked where the pieces are spliced): **NOT ON THE SAFE SIDE** where a
  splice of one section is really there (a leg longer than a stock
  length) — the owner's rule; to mark such a splice, give it by hand at
  the joint (it is then not proposed one part).
- **Not proposed**, said why: a splice given at the joint naming either
  piece; a `kind`, `onto`, `face` or bolts given by hand at either end;
  not two single angles (a built-up leg's splice: by hand); in line
  within more than 5° (a leg's knee); legs not along each other; the
  smaller angle nesting in the bigger (the layout bolts it nested, step
  1b).
- Two pieces of **no leg** of other sections, both with bolts and nothing
  else bolting onto them there (step 1c's rule), are proposed the same way.

**Confirmed**, a splice is the user's: a run (also one replacing values
given by hand) keeps it and its pieces' `onto`, and keeps their
provisional bolts listed as provisional; it is not proposed again.

### Bracing bolted through the outer cover

Once a splice is confirmed, a member end at the joint lapped on the
**outer face** of a spliced piece's leg where the splice has an **outer
cover** bolts **through the cover** (`Arrival::through`):

- it is **seated on the cover** — moved out of the leg by the cover's
  thickness; its bolts pass through the cover, the piece's leg and the
  inner covers on that leg: their **grip** is its own leg, then the outer
  cover, the leg and the inner cover (the inner cover counted wherever it
  lies under the bolt or not — *on the safe side for the bolt's length*;
  for the drawings and the bolt list only);
- the **outer cover is drawn round its bolts too** (the code's edge and
  end distances in the cover) and **checked as a plate for the end's whole
  force** — its bolts' bearing, block shear, section and distances
  (`plate_checks`): *on the safe side* — the leg under the cover takes
  none of it there;
- the **end's own checks are unchanged**: one shear plane (lapped), the
  bearing on its own leg and on the leg it bolts onto (the cover's
  thickness not added: *on the safe side*);
- **NOT ON THE SAFE SIDE**: (1) the cover is a **packing** between the end
  and the leg: the bolt's bending across it is not checked — neither
  ASCE 10-97 nor SIA 263 gives a reduction for packings (searched, none
  found), other codes do for thick ones; (2) the end lies the cover's
  thickness **farther out of the leg**: that offset is not in the analysis
  (as for an end on a gusset plate: O45 D) nor in the member's check
  beyond the codes' rules for an angle bolted by one leg.

In the layout such an end's plane is the cover's face: the cover is no
obstacle to it, the splice's own bolts (their nuts on the cover) are — the
end is pulled back clear of them, as of any fixed steel. It is bolted
**directly only**: where its bolts do not land on the leg's flange (the
flange's rules, and J9's cut-back rule: every bolt on the piece's steel
past its cut by the end distance), it is **left for a hand detail**, said
("… a plate over the cover is not laid out") — a gusset plate over a
cover is not laid out automatically. An end on the leg's inner face is
not taken through an inner cover (the cover is fixed steel to it, as
before). The clash check takes an end through a cover as lying on it,
its bolts' shanks through it.

**Not confirmed**: no cover, the pieces end at the joint, the ends onto
them laid out as by J9 (onto a plate where their bolts do not land on
the piece's steel past its cut, else a hand detail) — *on the safe side*.

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
then the later class in the owner's order before the earlier (a
redundant before the secondary bracing before a diagonal), then the
larger id. After six
such rounds a joint still clashing is left whole, as it was. Nothing is
forced: what is written runs into nothing but the members left for a
hand detail whose place is not kept free (whose clashes are said with
them; [J2](#members-left-for-a-hand-detail-keep-their-place-j2)).

## Laid out once, applied to the joints like it (J4)

(Owner, 2026-11-19: "lay out once, apply to the joints like it"; built the
same day, in active development, not approved yet. The code:
`ohl_structure_link::joint_kinds`, `apply_detail`.) A lattice tower
repeats its joints: its symmetry (mirror planes, quarter turns about its
axis) and its repeated panels.

- **Joints of one kind.** Two joints are of one kind when one of the
  eight rigid maps keeping the vertical — the identity, the quarter, half
  and three-quarter turns about the vertical, the mirrors in the two
  vertical planes of the axes and in the two diagonal ones —, with the
  move from one joint to the other, carries every member end of the one
  onto a member end of the other: the same **direction** (within 0.002 as
  the length of the difference of the unit vectors, about a tenth of a
  degree), the same **length** (within 2 mm), the same **cross-section
  turned alike** (its outline where it is, within 2 mm — so the rolls
  match), the same **class**. For the layout's run, also what the planner
  reads of each end: its bolts, its leg bolted, what it bolts onto (the
  matched member), where it sits, running on as one part, a cut or shift
  given, its bolts where they are; and no plate or splice at either joint.
  **The tolerances are the tool's, not a standard's** (a drawing's
  rounding). A tower's axis is taken vertical; the maps are about each
  joint's own work point, so the tower's centre need not be known.
  Deterministic: the joints in the file's order, the first of a kind its
  representative, the maps tried in a fixed order.
- **In a run.** One joint of each kind is planned; the others take its
  plan, mapped: the members said as theirs, the plates' outlines, heel
  lines, seams and bend lines moved, mirrored or turned there; the lengths
  as they are (a cut, a shift towards what the end bolts onto, a gauge's
  distance, a bolt along and across a leg: the same on a mirrored or turned
  joint). Every joint is then written and **checked for clashes in its own
  place**, as before; where a copy runs into anything its members are
  pulled back and the joint **planned on its own** (the clash rounds, as
  for any joint) — never forced. **An assumption of the tool**: the code's
  edge distances are taken alike for ends of one section, class and bolts
  (they are worked out per member from its code rules; the same rules
  give the same distances).
- **The command** *Apply this joint's detail to similar joints*
  ([user guide](../user-guide/joint-layout.md#apply-this-joints-detail-to-similar-joints))
  copies a joint's detail as it is in the file — laid out or detailed by
  hand — onto the joints of its kind (matched by their members only): its
  connections and plates replace theirs, mapped as above, the layout's
  record following (automatic stays automatic, given by hand stays the
  user's, a hand detail's words said with that joint's members). Each copy
  is checked for clashes in its own place: a joint where the copy runs
  into more than the source does is left as it was, said; a splice is not
  copied (by hand). One undo step.

## The owner's quick round (L1)

(Owner, 2026-11-19: four ideas for the automatic layout, approved in this
order; built 2026-11-20, in active development, not approved yet. The
code: `ohl_structure_link::joint_order`, `layout_run`, `joint_layout`.)
Each idea has its own switch (`LayoutSettings::bottom_up`, `more_plates`,
`onto_search`, on by default; off: as before, for the comparison), was
measured on the private real towers alone and with the others
([below](#measured-l1-2026-11-20)) and is kept only where it helps.

### Bottom up, then out (a)

(Owner: "try to go bottom up and out. More things align on the body from
the bottom up and then less at the mast ends (arms and top)".) A tower's
joints are sorted into its **body**, its **arms** and its **top** from the
geometry alone, never from names (`joint_order`):

- **The body's outline at each height** is the convex hull of the points
  where the legs' lines pass that height — the members of class `leg`
  running within 30° of the vertical (an arm's chord of class `leg` lies
  flatter; one line: the outline is that point).
- A joint **in the outline** at its height (within 0.10 m: a drawing's
  rounding and a leg's own width) is of the body; **outside** it, of an
  arm — so is a joint at a height the legs do not reach; the body above the
  highest arm joint is the **top** (the peak), and so is what lies above the
  legs' highest point. The 30° and the 0.10 m are the tool's values, not a
  standard's.
- **The order**: the body from the bottom up, then the arms and the top
  working outwards — an arm joint by how far it lies outside the body's
  outline, a top joint by how far above the highest arm joint (or the
  legs) — each tie by height, then the id. No legs found: every joint of
  the body, from the bottom up. Deterministic.

**Where the order matters.** Each joint is planned from its own joint in
detail: the cut, the shift, the plates and the clash rounds of one joint
do not depend on another's, so in most of the run the order changes
nothing. It matters in two places, which now follow it:

1. **The joint a kind is planned at** (J4): the first of the kind in this
   order (before: in the file's order); the others take its plan, mapped,
   checked in their own place as before.
2. **A member's roll** is one value for all its ends (J3). Where no one
   roll lays every end flat and **none of its ends lies flat now**, the
   member was left as it was; now it is turned to lie flat at its end
   **first in the order** — the body's end before an arm's, the lower
   before the higher —, its other ends taking what comes (step 4's welded
   plate or a hand detail), said ("… turned to lie flat at N2, its end
   first bottom up, then out; its others as they come"). The turn is kept
   only where it helps (rule 3 above). **A turn never spoils an end that
   was flat**: tried — turning to the end first in the order whatever the
   others — it laid out fewer joints whole on a sample of the real towers
   (43.6 % against 43.9 % on six of them).

### Slightly more plates (b)

(Owner: "maybe increase slightly the possibility of having plates".)
Today a face goes onto a plate only where the direct bolts do not land.
Now the plate is also tried:

- **In the plan**, where it leaves members for a hand detail: each face
  bolted directly (up to four per joint, `PLATE_TRIES`, the program's
  bound) planned on a plate instead; taken where that lays out **more** of
  the joint's ends — direct bolting wins a tie (the plate's penalty). On the
  real towers this almost never changes a plan: the planner's boxes do not
  hold the nuts behind a flange, which a plate would move away (L2).
- **After writing**, before an end bolted directly is given up: an end
  that would be left as it was — its clash or misfit not cleared by
  pulling it back (three times), or pulled back past its longest
  pull-back — is first put on a plate (its face onto one, once per end),
  kept where the plan then lays it out; the clash rounds check it as any
  end, a round being left to do so. Never forced.

**NOT ON THE SAFE SIDE** as for every plate the layout makes: an end on a
plate lies the plate's thickness farther out of the flange than bolted
directly, an offset not in the analysis (the joint eccentricity of a
gusset, usual for lattice towers, [above](#a-welded-plate-in-the-members-own-plane-step-4);
O45 D); (b) makes more such ends (measured below).

### Redundants and the body's diagonals bolted directly (c): tried, left out

(Owner: "Redundant members are usually bolted into another and diagonals
of the shaft too but it can depend".) Direct bolting onto another member
is already the first choice of every end: a face goes onto a plate only
where its bolts do not all land. Built and measured: on a face going onto
a plate, the redundants and the body's diagonals whose bolts land stayed
bolted directly, the others on the plate. **No gain** on the real towers
(below: 33 joints more whole of 64 760, 16 more not fitting as given): the members bolted directly ran into the plate of the others on
the same flange — the members leave the flange across the plate's place
beyond its toe —, as the mixed face tried on 2026-11-18. Left out; a
redundant bolted onto another member where that lays out more is what (d)
gives.

### What an end bolts onto, searched (d)

Today the member an end bolts onto (`onto`) is a fixed rule before the
plan ([above](#which-member-an-end-bolts-onto)). Now, for an end the
plans leave for a hand detail (its `onto` not given by hand, unless
replaced):

- **The other members it could bolt onto**: the single angles at the
  joint not in line with it (20° to 160°), in a flange's plane of which
  its axis lies (within 2°, `FLAT_OFF`), not bolting onto it nor running on
  into it — in **today's rule's order** (those running through the joint
  first, then the one part, by class, the wider, the piece on its side, the
  id). Today's choice is the first one tried and the tie-break: the joint
  keeps it unless another lays out more.
- **Each pass** names the next member for every such end in the planning
  copy (as the run names `onto` today) and plans those joints again; a
  joint takes the names where its plan lays out **more ends** and leaves no
  end as it is that was not before (an end named onto a member it cannot
  lie on is not laid out). At most **two passes** (`ONTO_TRIES`, the
  program's bound on the extra work).
- **The guard** (for (d) and for (b) in the plan): after the clash rounds,
  a joint whose plan L1 changed and that lays out **fewer ends than
  today's plan of it** is planned again as today (its names taken back) —
  one more pass of the clash rounds over those joints. Today's ends after
  the clash rounds are at most its planned ones, so a joint keeps L1's plan
  only where it does at least as well as today: *on the safe side for the
  comparison* (some gains are given up).
- Not for the planner relaxed (the estimated eccentricities, O45): without
  writing there are no clash rounds to judge by.
- The mutual-host guard (D16) applies as before; what an end bolts onto is
  what its restraint is worked out from, as detailed (unchanged).

### Three more of the joint checks' rules checked after writing (2026-11-20)

Found while measuring L1, on for every run: after writing, the run checks
as the joint checks do (as for an end laid onto a member, J9 and D16):

- the **holes of the ends bolted directly into one member** (or the run it
  is one part of) overlapping each other's or those of the other ends onto
  it as written — an end on a flange's outer face and one on its inner face
  bolted through the same place (holes nearer than their radii together,
  not one hole): the later end (class, then id) left as it was, said
  ("laid out, its holes in … would overlap those of … (left as it was)");
- **a plate's own bolts onto the member it lies on** (a bent plate's flat,
  J2b) outside its outline, or beyond the end of that member's steel
  where it ends at the joint (before its cut), as written: the members on
  the plate (and on what is bent or welded to it) left as they were, said.
  (The planner placed a bent plate's own bolts without that member's cut:
  placed on its steel since 2026-11-20, [below](#the-bent-plates-own-bolts-j2b-fix).)

The joint is planned again without them, as for a clash. Before, such
joints were written and the joint checks said them *does not fit, as
given*. The bent plate's own bolts drawn a few millimetres outside the
flat's outline on some members (where the flat is laid out and where its
bolts are drawn disagreed): looked into with L2 — the member under the
plate was drawn bolted by its other leg (its plate taken to lie on the
joint's leg), [below](#each-plate-on-the-member-it-lies-on-a); the check
stays.

## Clash-aware planning (L2)

(Owner, 2026-11-19: the real 3D shapes while planning, not boxes; before
an end is given up for a clash, the other face or the plate's other side
tried — approved with the other ideas of that day; built 2026-11-20, in
active development, not approved yet. The code: `joint_layout`,
`layout_run`, `joints`.) Before L2 the members left after the clash
rounds were the biggest group of reasons on the private real towers
(about 36 800). Sorted by the two parts of each clash: a plate with a
member's steel about half of them, a bolt's head, nut or shank with a
plate a fifth, a member with a member an eighth, a plate with a plate a
tenth, and the bent plate's own bolts about 1 450 ([measured](#measured-the-bent-plates-own-bolts-and-l2-2026-11-20)).
Looking into them found two things the planner and the drawing did not
agree on, before any shape: a plate's member and a moved member. Three
parts, each a switch (`LayoutSettings::plate_on`, `exact_plan`,
`clash_tries`), on by default; measured alone and together.

### The bent plate's own bolts (J2b fix)

The two bolts holding a bent plate's flat on the other leg
([above](#a-bent-plate-round-the-other-leg-j2b)) were placed about the
middle of the ends' bolts, without looking at the member they bolt onto.
Now:

- **On that member's steel**: where it ends at the joint (not one part
  through it), past where its steel starts by the end distance (the
  code's, else the default minimum) — its cut as laid out in this plan,
  else as given or worked out, else the work point (*conservative*: a cut
  not known is taken at the work point); the pair moved out along it where
  needed, the flat with them.
- **Where that member will be drawn**: where its own end is laid out at
  the same joint, the plate is laid out where it will lie (moved across by
  its move and lifted onto its plate), so that the flat's outline and its
  bolts (drawn from that member) agree. On a member laid out on a welded
  plate in its own plane (step 4): not followed — the bent plate is not
  laid out there, said.
- **Not through a hole** of that member's own end or of an end bolted
  onto it, as written (the joint checks' rule): else the members on the
  plate left as they were, said.

Most of the bolts drawn outside the flat's outline (the question of L1)
came from the next point: the member under the plate drawn bolted by its
other leg.

### Each plate on the member it lies on (a)

The joint details took every plate to lie on the **joint's leg** (the
joint details' rule for a plate given by hand: the leg by class, else the
largest member near the vertical): its plane with one member on it
spanned by that member and the leg, the flange under its worked-out
outline the leg's, and in the clash check the plate and the leg (not
tested against each other). The layout lays plates on the flange of the
member an end bolts onto — at a bracing joint, a diagonal, a horizontal,
a chord of an arm. There the plate was drawn in another plane (its one
member drawn bolted by its other leg), with the leg's flange in its
outline, its clash with its own member reported and its clash with the
leg hidden. Now each plate the layout makes **names the member whose
flange it lies on** (`on`, structure file version 23,
[data model](../development/data-model.md)): drawn, seated, tested in the
clash check and in its own axes for corrections by hand on that
member. A plate given by hand without it: the joint's leg, as before.

Clashes hidden before are now seen — a plate on a member reaching over
the leg at the work point (*on the safe side*: such ends are now left,
said). The joint checks take the plate as it is now drawn (its plane,
its outline over the flange of the member it lies on); their rules are
unchanged.

### What the clash check sees, while planning (b)

The plan of a plane ([pulled back](#pulled-back-the-cut), [searched](#the-search-per-joint-j1))
kept each end clear of the fixed steel in a slab over its face: its leg
and its bolts' heads. Now, as the clash check after writing sees it:

- **Its outstanding leg to its full height**, over the leg's own band
  across the end: the fixed steel and what the planes before laid out, up
  to the leg's width over the face (and the thicker of the two legs
  more); the angle as its two legs, each a box (the root's fillet left
  out: more steel than the angle has — *on the safe side for the fit*).
- **Its bolts' nuts and the shanks beyond them behind the face**: each
  bolt's disc of the nut's corner radius, from the face to the thicker of
  its leg and the flange behind it and the nut and the shank's end (the
  drawing's proportions) — the deeper of the two cases, directly on the
  flange and on a plate (*an assumption on the safe side for the fit*).
- **Each plate's outline as it will be drawn** (the joint details' rule,
  [plates](joint-plates.md)): round the work point, each member's leg over
  its bolts (from its first bolt less the end distance, not before the work
  point, to its last plus it), each bolt's distances in the plate (its
  end and edge distances in a plate of the member's leg's thickness; the
  plate is as thick as the thickest — *on the safe side*: the outline at
  least as large as drawn), and the flange under it across its width as
  far along as those reach; cut at the heel as the run cuts it. Tested in
  its thickness against the fixed steel (not a member on it, the member it
  lies on or one in line with that, nor a member left for a hand detail —
  the run lets a plate lie under those), the plates and covers given by
  hand and what the planes before laid out. A plate that would run into
  something is not laid out: its members whose bolts land on the flange
  bolted directly (c, below), the others left for a hand detail, said
  ("… on a plate on N1, the plate would run into D4 …").
- **What the planes before laid out**, for the planes after: each end's
  legs, its bolts' heads and now their nuts and shanks behind the flange
  or plate, and its plate as drawn (before: the hull of its bolts).
- **A member laid out at the same joint is planned where it will be
  drawn**: the planes on it (their ends bolting onto it, a plate on it)
  come after its own plane (as for its cut, J9) and are now taken where it
  will lie — moved across by its move (its gauge line through the work
  point, a shift) and lifted onto its plate. Before, they were planned
  where it lay before the run, and the bolts and plates on it were drawn
  elsewhere than planned.

Each piece is the clash check's own: its corners, convex, clipped to the
slab and seen from above as a convex polygon; two convex polygons tested by
separating axes in the plane (exact for convex pieces), overlapping by more
than the clash check's tolerance. **Deterministic and bounded**: no new
search (the same placings per joint), the same order and tie-breaks; a run
again identical. **Not followed** (as before, left to the clash rounds
after writing): an end on a welded plate in its own plane (step 4) and
its plate, a bent plate's flat in the members' plane, a member on a
welded plate in its own plane as what others bolt onto.

### Tries before an end is left for a clash (c)

- **Bolted directly instead of on its plate**, where its bolts land on the
  flange: in the plan, where the plate would run into something (above);
  after writing, an end on a plate that would be left as it was for a
  clash, once per end while a round is left, kept where the plan then lays
  it out.
- **The other face of the flange (the half turn)**: an end the
  clash-aware plan leaves for what it, or its plate, would run into is
  tried half a turn round (its outstanding leg to the other side, so that
  it lies on the flange's other face), with the ends the run already
  turns so (an end on an inner face running towards the angle's other
  leg) — kept for a member only where more of its ends are laid out and
  no joint of it loses one, as those.
- **Not built**: the clashing end on a plate of its own on the same face.
  A plate reaches the work point and the flange under it whatever ends are
  on it: a second plate on the face would run into the first and into what
  the first ran into there.

### Two more of the joint checks' rules checked after writing

Found measuring L2 (for every run, as the three of L1,
[above](#three-more-of-the-joint-checks-rules-checked-after-writing-2026-11-20)):
the layout laid out more, and some joints were written that the joint
checks said *does not fit, as given*. Now the run also checks, as written:

- **the bolts of each member on a plate the layout made within the
  plate's outline** (a plate in its member's own plane and a bent plate's
  flat have their outlines laid out in the plan; a member drawn elsewhere
  than planned lies off them): else that member left as it was, said
  ("laid out, its bolts on a plate on N1 would lie outside the plate's
  outline (left as it was)");
- **a member not laid out at the joint bolting onto one that is**: its
  bolts, where they are, still within that member's flange and on its
  steel past its cut (the rule an end laid onto a member left as it was is
  checked by, J9 and D16, the other way round) — else the member laid out
  is left as it was, said ("laid out, D4 (as it is) would then not fit
  onto it: …"). Not asked of a member left for a hand detail: its hand
  detail places its bolts (J2).

The joint is planned again without them, as for a clash. *On the safe
side*: what does not fit is not written.

### Redundants and the body's diagonals bolted directly (L1 c), again

L1 (c) ([above](#redundants-and-the-bodys-diagonals-bolted-directly-c-tried-left-out))
was left out because the members bolted directly ran into the plate of
the others on the same flange — the plan did not see that plate. With the
plate's outline in the plan it does; built again as
`LayoutSettings::direct_first` and measured on top of L2 (below).

## L2's follow-ups (2026-11-20)

(The recommendations given to the owner with L2 — its questions 4, 5 and
6 —, small and on the safe side; built 2026-11-20, in active development,
not approved yet. The code: `joint_layout`, `layout_run`, `joints`.) Two
are switches (`LayoutSettings::short_plates`, `slide_holds`), on by
default; the third is a fix of the record.

### A plate stopping short of what it would run into (a)

A plate's outline worked out reaches the work point and the flange under
it (the rule for plates of 2026-11-02, "keep hidden in the shape of the
corner"). About 9 000 ends were left in L2's plan because their plate
would run into steel already there, mostly near the work point (a member
left as it is there, a mutual host's partner, a built-up member, a piece
in line). Now, where the outline round the work point would run into
something, the plan tries:

1. **Stopping short of the work point**: the outline round each member's
   leg bolted from where its nearer edge crosses the flange's toe (or its
   first bolt less its end distance, whichever is nearer the joint; not
   before the work point) to its last bolt plus the end distance, each
   bolt's ring of its end and edge distances in the plate (the code's,
   else the default minimums — as every plate's outline), and the flange
   under those across its whole width as far along as they reach; cut at
   the heel as before. The plate spans the whole free part between the
   flange and the bolts (the member's force crosses it there) and lies on
   the flange beside the legs.
2. **Cut straight short of what it would still run into**: where the short
   outline still runs into something, a straight cut — the line that
   parts that thing from what the plate must keep (the legs from the toe
   over their bolts and the bolts' rings) farthest, the plate stopping half
   the gap short of it (half of what parts them where that is less); at
   most four cuts (`CUT_TRIES`, the program's bound).
3. Kept where it then runs into nothing and still lies on the flange over
   at least half a square of the flange's width (room to bear on it; the
   program's value, as step 4's flat plate's); else the plate is not laid
   out, as before (said: "… the plate would run into D4 (stopping short of
   the work point too) …").

The plate is convex (a hull, cut by straight lines) and every bolt on it
keeps the code's end and edge distances from its edge: the cuts never
enter the rings. The run marks such a plate `short` in its record (the
joint details draw it so) and writes its outline with the cuts. **After
writing** the run checks, as the joint checks do, that every bolt on such
a plate keeps those distances from the outline as written (the plan and
the drawing may differ by a millimetre); else that member is left as it
was, said. Not for a flat plate with plates in their members' own planes
welded or bent from it (it must reach the seams and the flange from the
work point).

**The plate checks take the outline as cut** ([plates](joint-plates.md)):
the end and edge distances, the block shear's paths and the width the
sections are checked with are measured to the outline as written, so a
plate stopping short is checked as it is. The strut's **free length**
(a plate in compression) runs from the bolts towards the joint to the
plate's edge that way: at least to the flange's toe — the outline keeps the
legs from where they cross it — so the plate's free part beyond the flange
is in it; where the plate lies on the flange it is held on one side only,
by contact (*an assumption*, as before for the plate round the work point,
whose free length reached farther). **NOT ON THE SAFE SIDE** in the same
sense as every plate the layout makes: how the plate is fastened to the
flange it lies on is not modelled (the members' bolts hold it; a flange
running on through the joint has no bolts of its own there) — unchanged by
the short outline.

### The bent plate's own bolts clear of the holes there (b)

J2b's two bolts holding a bent plate's flat on the other leg were placed
about the middle of the ends' bolts, then moved out along the leg past its
cut; about 670 ends were left after writing because the first holding bolt
fell on a hole there (most often the member's own first bolt). Now the
pair is **slid along the leg** — the least slide first, either way, away
from the work point on a tie, at most the longest pull-back — until each of
its bolts keeps the **default minimum spacing** from every hole through that
leg: the member's own end there and the ends bolted onto it, those laid out
in this plan where they will be, the others where they are. *On the safe
side*: the joint checks' rule is only that holes do not overlap; the
spacing keeps the heads and nuts apart too. No place within the longest
pull-back: the bent plate is not laid out, said.

### An `onto` given by hand kept in the record (c)

An end's `onto` given by hand (a member) is kept by the layout; where the
layout puts the end onto a plate on that member, the file then names the
plate. Before, the record took that value as the layout's, so a run again
took it out and the end was planned from the rule — the hand value lost.
Now the record keeps the hand value (`onto=D4` among the end's marks): a
run again puts it back before planning, the window says "onto (yours: D4,
kept)", an edit of the end's `onto` makes the new value the user's, and
*Replace values given by hand* replaces it as before.

## A plate bent by a few degrees (L3)

(Owner, 2026-11-19: "a plate bent by a few degrees" for the ends whose
member needs rolls 5°–20° apart at its two ends — the second group of ends
left after L2; the owner's example joint shows a gusset plate bent at a
corner. Built 2026-11-20, in active development, not approved yet. The
code: `joint_layout` (`Room`, `rooms_for`, `own_geometry`), switch
`LayoutSettings::small_bends`, on by default.) An angle's roll is one value
for the member (J3): where its ends need rolls a few degrees apart, one end
lies flat and the other is off its flange by the difference. Shops bend
the gusset (or the end's own connection plate) by that much so that the
brace lies flat on it.

- **Which end takes the bend.** The rules of J3 and L1 (a) unchanged: a
  turn never spoils an end that lies flat now, so where one end is flat it
  stays flat and the other takes the bend; where none is, the member is
  turned to lie flat at its end first bottom up, then out — the body's end
  before an arm's or the top's, the lower before the higher —, so the end
  on the arms and the top (or the higher one) takes the bend. *Not built*:
  preferring the end with fewer members (a question to the owner).
- **The plate.** As step 4's plate in the member's own plane (its member's
  leg bolted lies on it, its bolts through it; round them with the end and
  edge distances down to the line where its plane meets the flat plate's
  face), but **bent from the flat plate on the flange** along that line —
  one plate, no weld (`[plate.bend]`, structure file version 22, as J2b's;
  no new field): its angle the angle between the member's leg and the
  flange, its inside radius two thicknesses (`BEND_RADIUS_RATIO`, the
  program's starting value as for J2b), its bend line where the two flats'
  middle planes meet. The flat plate on the flange is step 4's (the face's
  other ends on it, covering the bend line and the flange from the work
  point).
- **Away from the flange or towards it.** The flap may turn away from the
  flange (the member on the bend's inside, lifted the least that keeps it
  on the flap) or towards it past the flange's toe (the member on the
  outside, lowered the least that keeps it so): both are tried; of those
  that fit, the one moving the member's axis least from where it lies on a
  flat plate (away from the flange on a tie); then the welded plate of step
  4, as before; else a hand detail, said with why each does not fit.
- **The bend line clear of the bolts.** The bend's curved part reaches
  (r + t) tan(θ/2) either side of the line (r the inside radius, t the
  plate's thickness, θ the angle — the outside radius, *on the safe
  side*); the member's steel keeps half the gap beyond it, its bolts their
  edge distance (the code's in a plate of the member's leg's thickness,
  else the default minimum) beyond it. *An assumption of the tool*: the
  edge distance from a bend's curved part as from a free edge (the plate
  runs on there).
- **Its line of action.** As for step 4 (the same limit): the member's
  axis at its bolts at most **one plate thickness** farther out of the
  flange than on a flat plate — or, turned towards the flange, nearer —
  else not laid out so. The joint eccentricity of a gusset is not in the
  analysis (usual for lattice towers, not a standard's rule): the limit
  keeps the bent plate within it; **NOT ON THE SAFE SIDE** by that offset,
  as for every plate the layout makes. The estimated eccentricities (O45)
  take a bent plate's offset only where it is farther out (nearer: none,
  as a flat plate).
- **The bend in bending.** A row of the joint checks as every bend
  ([above](#the-bend-in-bending-2026-11-19); the owner's decision C12):
  the plugins' `joint-check-bends`, unchanged — the strict check, no
  contact relief (the owner has not decided). For an end whose member lies
  along the bend line (its leg twisted about its axis — the usual case
  here) the force across the bend is small and the bend works mostly in
  shear along it and in its own plane; the flat on the flange has no bolts
  of its own onto the flange: the lever of the force across the bend to
  that flat's farthest edge (said, *on the safe side*). Measured below.
- **Never forced**, deterministic, followed as every plate: the bend and
  the flap's outline are automatic values of the record (`own_plane`,
  `bend`), a radius given by hand kept. Weighed from the developed area,
  drawn in the 3D view, close-up, sheets and IFC as J2b's bent plates (the
  bend's angle any).

## The bent end's turn and the end running back to its bend (L3b)

(Owner, 2026-11-20, L3's question 5 and its follow-ups. Built 2026-11-20,
in active development, not approved yet. The code: `layout_run` (step 2b's
quarter turns), `joint_layout` (`turned_end`, `own_geometry`'s `back`),
switches `LayoutSettings::bend_facing` and `LayoutSettings::bend_back`, on
by default, both with `small_bends`. No format change.)

- **Which way round the flat end lies.** A member whose ends need rolls a
  few degrees apart lies flat at one end; turned half a turn more it still
  lies flat there (its outstanding leg the other way), and an equal-leg
  angle a quarter turn more too (its other leg on the flange — mirrored
  about the heel). At the other end the turn decides whether the
  outstanding leg points away from the flange (a plate bent by as much
  takes it) or towards it (it would stand on the plate). The half turn was
  already tried for such ends (step 2b, kept for a member only where more
  of its ends are planned laid out and its joints lay out no fewer). Looked
  into on the real towers: the two ends' flanges lie within the few degrees
  of each other but on opposite sides of the member, so whichever turn
  faces the bent end's leg the right way faces the flat end's the wrong way
  (it then needs the flange's other face), and half turned about half of
  them (on a sample of the towers) then lie with their axis more than a
  plate's thickness out (step 4's limit: the leg's far edge lowest from the
  bend) — why most such ends stay left. **Built:** for an equal-leg angle
  the half turn did not help, the
  quarter turn that faces its bent end's leg away from the flange (worked
  out from the geometry, `turned_end`: the leg lying flatter there, how far
  off, which way its outstanding leg points on either of the flange's
  faces) is tried in one more planning pass and kept as the half turn is.
  A turn chosen from the geometry alone, without planning (the half turn
  or the other leg wherever it predicts more ends flat or bent), was
  measured and left out: fewer ends laid out (the predicted bent end still
  failing the axis limit and the flat end lost).
- **An end running back towards its bend.** Bent away from the flange,
  the flap rises from the bend line; an end whose member runs back down
  towards that line going away from the joint was refused (its steel would
  cross the line). It now takes the plate bent away too where its steel
  keeps clear of the bend — the curved part and half the gap beyond it, as
  every L3 bend — **wherever it lies over the flange**, to half the gap past
  the flange's toe (or heel); beyond, no flange or flat plate lies under it,
  and its steel may pass the plane of the flat plate's face there (*an
  assumption of the tool*; the clash check checks every solid after
  writing). Its bolts their edge distance beyond the curved part, its axis
  within one plate thickness of where a flat plate puts it — L3's limits,
  unchanged. Bent towards the flange (the other way) was tried already;
  it fails on the axis limit (the member lowered 5–20 mm, more than the
  plate's thickness) for nearly all of these ends. The flap on the joint's
  side of the bend line with the flat plate beyond it is **not built**: the
  flat plate would no longer cover the work point and the other ends on
  it, the flap standing over them.
- **Unchanged:** the bend check (C12, the plugins' strict check), the 20°
  bound (`BEND_SMALL_MAX`), the edge distance from a bend as from a free
  edge, the 2° and 5° tolerances; the tilted ends on an inner face running
  towards the other leg are not laid out (J2b's bent plate is not for
  tilted ends).

## Ends moved to bolt directly; one bolt through the leg looked into (L5)

(Owner, 2026-11-20, on the picture of the plate bent by a few degrees:
"you could move the 2 diagonal a bit forward maybe to have one bolt on the
main leg without them touching each other. The second bolt needs the plate
anyway but maybe shorter." Built 2026-11-20, in active development, not
approved yet. The code: `joint_layout` (`land_moved`, `narrow`, the guard
in `plan_joint_ctx`), the switch `LayoutSettings::direct_moved`, on by
default. **No format change**: a move is the end's `shift`.)

**One bolt through the leg, the rest on a shorter plate — looked into, not
built.** A gusset plate lies between the member and the flange. A bolt of
the member through the plate *and the leg* must lie on the flange as every
bolt bolted directly does (its nut behind the flange clear of the angle's
other leg and root, its edge distance from the toe); a bolt through the
member and the plate only must lie past the flange's toe by its nut and
the gap, its nut behind the plate where the flange is not (the rule of
[plates](#onto-the-flange-or-onto-a-plate)). Between the two, the member's
bolts must therefore lie apart *across the flange* by at least the edge
distance, the nut's corner radius and the gap: for 16 mm bolts and the
default minimums 24 + 13.9 + 10 ≈ 48 mm, for 20 mm bolts about 57 mm. One
gauge line at the default pitch of three diameters (48 and 60 mm) crosses
the flange by the pitch times the sine of the member's angle to it: enough
only for a member nearly square to the flange. Measured on every
thirteenth private real tower (six): of 6 626 member ends lying flat that
go onto a plate, 63 % have **one bolt** (nothing to split); the placement
fits for 248 ends (3.7 %) within a move of 60 mm, 128 of them without a
move, all within 26° of square to the flange (nine in ten within 14°)
and nine in ten with 20 mm bolts.
To write it, the format would have to say which bolts of an end also pass
through the member under the plate (a new field, structure file version
24), and the joint details, the clash check, the joint checks and the bill
of materials follow it — not built for so few ends (a question to the
owner). Found looking into it: a flat plate the layout lays on a leg
running through the joint is held to that leg by no bolt of its own; the
joint checks say the plate's holding *not checked* (as for any plate the
leg does not bolt onto); a bolt through the plate and the leg would hold
it.

**Built: an end moved to bolt directly.** On a face that goes onto a plate
(some end's bolts do not land on the flange where the search placed it),
each end whose bolts do not land is placed again:

1. pulled back further, else
2. moved sideways by the least whole millimetre — "a bit forward" along
   what it bolts onto, away from the other ends — up to **the plate's
   thickness** (the thickest leg of the face's ends, as the plate would
   be),

until every bolt lands on the flange as for any end bolted directly
(clear of the heel's leg and root by the nut, the edge distance from the
toe, on the steel of what it bolts onto past where that starts by the end
distance), clear of the other ends of the face as they lie and of the
joint's fixed steel (the search's own rule, `cut_need`). Where every end of
the face then lands, **the face is bolted directly: no plate**. Otherwise
the face goes onto its plate as before. Moving only some ends of a face
off its plate (the others staying on it) was measured and left out: more
ends laid out, but fewer joints whole and more plates than moving whole
faces.

*The limit, on the safe side relative to the plate it replaces*: on the
plate, the end lies a plate's thickness out of the flange — an
eccentricity of the member's force out of the flange's plane that is
**not** in the analysis (as every gusset's offset: [above](#onto-the-flange-or-onto-a-plate),
O45 D). Moved by at most that thickness, its force passes the work point
by no more than the plate put it out, and **the move is in the analysis**:
it is the end's `shift`, a rigid end offset at level 4 ([Eccentric member
ends](structures-fe.md#eccentric-member-ends-rigid-end-offsets)), and at
level 3 with *Estimated eccentricities* on (O45: a laid-out shift read as
laid out). At level 3 without that option the move is, like every shift,
not in the analysis — **NOT ON THE SAFE SIDE** there, as said for the
shift ([above](#moved-sideways-the-shift)). Measured on every thirteenth
real tower: the new moves 1–8 mm (median 3); the extra elastic stress at
the far toe of the angle from N·e, over the axial stress, e·A/W, median
0.37, at most 0.92 — of the order of the plate's own offset's, which is
left out. Up to the shift limit (30 mm) instead: about a fifth fewer
plates and 1.5 points more joints whole, but moves of median 15 mm and
e·A/W median 1.9, at most 4.8 — **not** chosen (a question to the owner,
with the usage it costs).

*The guard — the clear rule*: each joint is planned with and without the
moves; the plan with them is kept only where it lays out **more ends, or
as many on fewer plates**; else the plan without (no move). Deterministic
(both plans are). With the moves' search a millimetre at a time, a run
takes about a quarter more time (measured below).

Each move is said with the end in the window's notes ("D2: moved 3 mm
sideways to bolt directly onto L1 (no plate): an eccentricity, its shift —
at most the plate's thickness").

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
| Faces bolted directly tried on a plate per joint (L1 b) | 4 | a bound on the extra work; the plate rarely lays out more in the plan (the planner does not see the nuts behind a flange) |
| Passes of other members an end could bolt onto (L1 d) | 2 | a bound on the extra work |
| An end on a plate bolted directly instead, after writing (L2 c) | once per end | a bound on the extra work; the half turn for a clash once per member, with the half turns already tried |
| How deep behind a face an end's nuts are kept clear of steel (L2 b) | the thicker of its leg and the flange, and 1.1 bolt diameters | the drawing's nut (0.8 d) and the shank beyond it (0.3 d), not a standard's: the deeper of bolting directly and on a plate |
| Straight cuts of a plate stopping short (L2's follow-up a) | 4 | a bound on the extra work |
| How much a plate stopping short must lie on the flange (L2's follow-up a) | half a square of the flange's width | room to bear on it, as step 4's flat plate; not a standard's |
| How far off its flange an end is put on a plate bent by as much (L3) | 20° | the owner's range of the ends whose member needs rolls 5°–20° apart; beyond, welded (step 4) |
| How far an end of a face going onto a plate is moved to bolt directly (L5) | the plate's thickness (at most the shift limit) | on the safe side relative to the plate it replaces: no larger an eccentricity than the plate's own offset out of the flange, and in the analysis as a shift; the shift limit measured on six private real towers (about a fifth fewer plates, but moves of 15 mm median): a question to the owner |

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

## Measured (J2b, the bent plate, 2026-11-19)

On the same 78 tower files and settings:

| | J1 and J2 | with the bent plate |
|---|---|---|
| joints laid out whole | 44.1 % | 45.8 % |
| member ends laid out (of those the layout places) | 62.8 % | 64.3 % |
| joints with a hand detail | 49.7 % | 47.9 % |
| clashes at the joints after (of before) | 46.3 % | 44.4 % |
| a run again identical | 78 of 78 | 78 of 78 |
| time of a whole tower (measured one after the other) | | about 5 % more |
| joints that do not fit with the joint checks | 32.3 % | 31.8 % |
| ends the estimated eccentricities bound from above | 24.3 % | 22.0 % |

Bent plates were laid out at about 6 % of the joints. Of the ends over
the heel, about two fifths are still left: most for want of room for the
holding bolts on narrow legs (45–55 mm), some for a clash after writing
(said with it) — never forced. An inside radius of one thickness instead
of two changed almost nothing (on nine of the towers: 0.1 % more ends).
The bolts across a narrow flange that do not fit (the does-not-fit
investigation) are almost all on ends not laid out: a flat plate takes
them wherever the layout lays the end out; they fall as more joints are
laid out, not with the plate's kind.

## Measured (J3 and J4, 2026-11-19)

On the same 78 tower files and settings (the files now with their bolt
grade, version 21; the J2b column measured again on them, the same
numbers):

| | J2b | J3 (the owner's order of the classes) | J3 and J4 |
|---|---|---|---|
| joints laid out whole | 45.8 % | 45.8 % | 45.9 % |
| member ends laid out (of those the layout places) | 64.3 % | 64.2 % | 64.3 % |
| joints with a hand detail | 47.9 % | 47.9 % | 47.9 % |
| clashes at the joints after (of before) | 44.4 % | 44.5 % | 44.4 % |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 |
| joints that took a plan of a joint like them | — | — | 30 % |
| ends the estimated eccentricities bound from above | 22.0 % | | 22.0 % |
| joints that do not fit with the joint checks | 31.9 % | | 31.8 % |
| time of a whole tower (five towers, one after the other) | | about 5 % more | as J3 (±2 %) |

**J3 stops paying here**: whatever the roll, most members left off their
flange need turns at their two ends more than 5° apart (5° to 20° for
seven in ten of them) — the faces they bolt onto are turned against each
other about the member, which no roll removes (step 4's welded plate with
a larger offset, J5, or a bent version of it). A quarter turn changing
the leg bolted served 8 members on the 78 towers; the class by class
order and the legs never turned to suit bracing (816 leg ends now kept
off what they bolt onto, said) changed the shares by less than 0.1 %. J1's search in the
owner's order of the classes against the biggest member first within the
bracing (as before): the same joints whole within 7 of 64 760, 0.06 %
fewer ends — the owner's order kept. Taking a turn leaving every end
within the whole 5° of flat (tried) laid out fewer ends.

**J4** lays out a third of the joints with the plan of a joint like them
(the tower's symmetry: on the square bodies most joints have three or
seven others), a few more of them whole than planned on their own (34
joints, the ties of the search now broken alike), and makes the joints of
one kind identical. **It saves no time measurably**: planning is about 1
to 1.5 % of a run (the joint details worked out in each round and the clash
checks take the rest, and each copy must be checked in its own place).

## Measured (J9 and the mutual-host guard, 2026-11-19)

On the same 78 tower files and settings (kept privately), with the joint
checks' "does not fit" (one case; misfits depend on neither loads nor
code):

| | before | the guard alone | J9, nothing confirmed | J9, every proposal confirmed (an upper bound) |
|---|---|---|---|---|
| joints laid out whole | 46.1 % | 42.1 % | 36.2 % | 41.9 % |
| joints with a hand detail | 47.7 % | 51.7 % | 57.6 % | 50.2 % |
| member ends laid out (of those the layout places) | 64.6 % | 57.3 % | 51.0 % | 56.1 % |
| ends left by the guard | — | 32 391 | 33 684 | 7 195 |
| joints with members bolting onto each other | 10 167 | 0 | 2 | 0 |
| joints that do not fit | 31.8 % | 34.4 % | 35.6 % | 28.8 % |
| … of them as given (the rest as assumed — not detailed) | 6 764 | 2 210 | 839 | 605 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 |

46 819 pairs of pieces in line were proposed, 13 071 not (10 620 pieces of
legs with splice bolts — J7's —, the rest turned more than 5° apart); most
proposals needed one roll (the pieces 0.1° to 1.1° apart after Orient
angles). The guard alone loses a quarter of the ends, as expected; with
the pieces confirmed it leaves a fifth as many. Not confirmed, the layout
treats the pieces as ending at the joint, as the checks do: it writes
almost no misfit of its own (839 joints, from 6 764) but lays out fewer
joints whole — ends bolting across the gap go onto plates, and on a
built-up leg (no plate laid out there) to a hand detail. Confirmed, the
fewest joints do not fit (28.8 %); the joints laid out whole before and
not now were physically impossible (members cut back onto each other,
bolts beyond the steel). The leg splices (J7) are now the main limit
("ends laid out" is not comparable across the columns: pieces one part
run on, nothing to lay out). No change in time measurable.

## Measured (J7a and the 2° flatness, 2026-11-19)

On the same 78 tower files and settings (kept privately), J9's proposals
confirmed in every column, with the joint checks' "does not fit" (one
case):

| | before, at 5° | before, at 2° | J7a, leg places not confirmed | J7a, every leg place confirmed (an upper bound) |
|---|---|---|---|---|
| joints laid out whole | 41.9 % | 39.1 % | 39.1 % | 41.5 % |
| joints with a hand detail | 50.2 % | 53.0 % | 53.0 % | 50.6 % |
| member ends laid out (of those the layout places) | 56.1 % | 53.2 % | 53.1 % | 55.3 % |
| joints that do not fit | 28.8 % | 29.6 % | 29.6 % | 29.8 % |
| … of them as given (the rest as assumed — not detailed) | 605 | 525 | 525 | 475 |
| pieces in line proposed one part (J9) | 46 819 | 46 355 | 46 355 | 46 355 |
| leg pieces of one section proposed one part | — | — | 10 572 | 10 572 |
| leg splices proposed / not | — | — | 503 / 1 349 | 503 written |
| member ends bolted through an outer cover | — | — | — | 1 336 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 |

**The 2° flatness alone** (the owner's decision of 2026-11-19, the second
column against the first) lays out fewer joints whole (−2.8 points) and
fewer ends (−2.9 points): ends lying 2° to 5° off their flange now go to
step 4's welded plate or a hand detail (as said, "its bolted leg lies …°
off the flange"); fewer joints misfit as given (605 → 525), and 464
pieces in line fewer are proposed one part.

**The leg splice places** (the last two columns): confirmed, joints laid
out whole 39.1 % → 41.5 %, ends 53.1 % → 55.3 %, joints misfitting as
given 525 → 475. Not proposed: 624 pairs not of two single angles
(built-up legs), 554 where the smaller angle nests in the bigger, 104
legs meeting at more than 5° (knees), 67 turned apart. Every leg piece at
a change of section had its bolts from the import (no provisional bolts
needed). Of the bracing ends at the splices confirmed, 1 336 bolt through
the outer cover; 1 242 are left for a hand detail because their bolts do
not land on the leg's flange clear of the splice's own bolts (their nuts
on the cover) — the main limit now; plates on a leg's inner face run into
the inner covers (about 450 such clashes said). Joints that do not fit
rise a little (15 793 → 15 847, mostly as assumed): fewer joints are
checked as written (the confirmed pieces run on) and the covers bring
plates of their own to check. No change in time.

## Measured (L1, 2026-11-20)

On the same 78 tower files and settings (kept privately; J9's proposals
and J7a's leg splice places confirmed in every column, the 2° flatness),
with the joint checks' "does not fit" (one case). "After writing" is the
run with the three after-writing checks above and no L1 switch; each L1
column adds its switch to it:

| | before | after writing | (a) | (b) | (d) | (a), (b) and (d) |
|---|---|---|---|---|---|---|
| joints laid out whole | 41.5 % | 41.4 % | 41.7 % | 41.7 % | 42.2 % | 42.7 % |
| … of the body | 44.0 % | 43.9 % | 44.2 % | 44.3 % | 44.6 % | 45.2 % |
| … of the arms and the top | 38.3 % | 38.3 % | 38.6 % | 38.4 % | 39.1 % | 39.5 % |
| joints with a hand detail | 50.6 % | 50.7 % | 50.4 % | 50.4 % | 49.9 % | 49.5 % |
| member ends laid out | 55.3 % | 55.2 % | 55.4 % | 55.5 % | 55.9 % | 56.5 % |
| joints that do not fit | 29.8 % | 29.8 % | 29.7 % | 29.8 % | 29.7 % | 29.7 % |
| … of them as given | 475 | 41 | 41 | 41 | 40 | 41 |
| plates written | 48 570 | 47 963 | 48 412 | 48 687 | 49 945 | 50 756 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 |
| time per tower, median / largest | 12.3 / 18.8 s | 12.2 / 18.7 s | 13.0 / 19.7 s | 16.1 / 23.9 s | 16.1 / 25.7 s | 20.7 / 33.2 s |

Of the tower's joints 56 % are of the body, 44 % of the arms and the top.

- **The after-writing checks** take the joints said *does not fit, as
  given* from 475 to 41 (nine in ten), for 34 joints fewer laid out whole
  (0.1 point): the layout no longer writes those misfits.
- **(a)** lays out 190 more joints whole (+0.3 point) — on the body and on
  the arms alike —, 7 % more time.
- **(b)** lays out 193 more joints whole (+0.3 point) with 724 plates more
  (+1.5 %); about a third more time (the joints planned again with a
  plate).
- **(d)** lays out 485 more joints whole (+0.8 point; the ends left on a
  built-up member 10 337 → 8 712, off their flange 30 860 → 29 224); about
  a third more time.
- **Together**: 799 more joints whole than without them (41.4 % →
  42.7 %), ends 55.2 % → 56.5 %, 2 793 plates more (+5.8 %), no misfit
  more; the time per tower about 1.7 times (median 12.2 → 20.7 s).
- **(c)** measured on its own (against the run with the first two
  after-writing checks): 33 joints more whole (+0.05 point), 16 more
  joints not fitting as given, about 40 % more time — left out.

The gains stay small: the two big groups left are untouched by L1 — the
members left after the clash rounds (about 36 000 hand-detail reasons)
and the ends off their flange that need rolls 5°–20° apart at their two
ends (about 29 000): L2 and L3.

## Measured (the bent plate's own bolts and L2, 2026-11-20)

On the same 78 tower files and settings as L1 (kept privately; J9's
proposals and J7a's leg splice places confirmed in every column, L1's (a),
(b) and (d) on). "Before" is L1's result; "bolts" the bent plate's own
bolts placed on the host's steel alone (measured before the two
after-writing checks of L2); "+ checks" with them (every column after it
has them); "(a)", "(b)", "(c)" each L2 part alone on top of that, "(a),
(b)" the two without the tries; "L2" the three together (the default
now); "+ L1 (c)" with L1 (c) on top:

| | before | bolts | + checks | (a) | (b) | (c) | (a), (b) | L2 | + L1 (c) |
|---|---|---|---|---|---|---|---|---|---|
| joints laid out whole | 42.7 % | 44.1 % | 43.7 % | 45.6 % | 44.7 % | 43.7 % | 48.3 % | 48.6 % | 48.2 % |
| … of the body | 45.2 % | 47.3 % | 47.3 % | 46.6 % | 48.8 % | 47.3 % | 48.1 % | 48.2 % | 47.9 % |
| … of the arms and the top | 39.5 % | 40.1 % | 39.0 % | 44.3 % | 39.4 % | 39.0 % | 48.6 % | 49.0 % | 48.7 % |
| joints with a hand detail | 49.5 % | 48.0 % | 48.4 % | 46.5 % | 47.5 % | 48.4 % | 43.8 % | 43.5 % | 43.9 % |
| member ends laid out | 56.5 % | 57.2 % | 55.9 % | 59.7 % | 56.3 % | 55.8 % | 61.4 % | 61.8 % | 61.9 % |
| joints that do not fit | 29.7 % | 29.3 % | 29.3 % | 26.3 % | 29.7 % | 29.3 % | 24.8 % | 24.7 % | 24.8 % |
| … of them as given | 41 | 39 | 39 | 35 | 38 | 39 | 46 | 42 | 42 |
| plates written | 50 756 | 53 238 | 50 135 | 54 571 | 51 245 | 50 022 | 57 715 | 58 424 | 59 576 |
| members left for a clash (reasons) | 36 837 | 35 943 | 39 652 | 30 447 | 32 845 | 39 561 | 23 730 | 22 960 | 22 990 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 |

The clash-kind reasons before L2, by the two parts named (36 837): a
plate with a member's steel 18 968 (51 %), a bolt's head, nut or shank
with a plate 6 942 (19 %), a member with a member 4 627 (13 %), a plate
with a plate 3 412 (9 %), the bent plate's own bolts 1 456 (4 %), a bolt
with a member's steel 965, a cover with a plate 362. With L2: after
writing, a plate with steel 3 991, a bolt with a plate 3 439, a member with
a member 1 618, a plate with a plate 495, a bolt with steel 876, the
bent plate's own bolts 1 047; said in the plan (a plate that would run
into something, not written) 8 945; the after-writing checks of joint
rules 2 522.

- **The bent plate's own bolts** lay out 949 more joints whole and 1 392
  more ends; the ends left for those bolts 1 456 → 872 (686 of them now for
  a hole of another member there, found once the bolts lie on the steel).
- **The two after-writing checks** (for every run) take 2 384 ends back
  out — joints that would have been written not fitting (most of them
  said *does not fit, as assumed* before): *on the safe side*.
- **(a) Plates on their member**: +1 253 joints whole, +6 753 ends against
  "+ checks", most on the arms and the top (39.0 % → 44.3 %), where the
  plates lie on chords and bracing rather than on the legs; 4 436 plates
  more; the joints not fitting 29.3 % → 26.3 %. A body joint's plates lie
  mostly on the leg: little change there.
- **(b) Clash-aware planning alone** (without (a)): +638 joints whole,
  +699 ends, the body most (47.3 % → 48.8 %); 9 568 ends said in the plan
  (their plate would run into something) rather than written and taken
  back.
- **(c) The tries alone**: nothing (their half turn and their in-plan part
  need (b); after writing alone: 1 joint more whole, 195 ends fewer).
- **Together** (L2) more than the sum: joints whole 42.7 % → 48.6 %
  (+3 834; the arms and the top 39.5 % → 49.0 %), ends 56.5 % → 61.8 %
  (+9 811), the joints that do not fit 29.7 % → 24.7 %, as given 41 → 42
  (members left for a hand detail whose bolts no longer fit the member laid
  out under them, said; their hand detail places them), plates +15 %, a run
  again identical 78 of 78. The tries (c) on top of (a) and (b): 170
  joints more whole, 938 ends more, 4 fewer not fitting as given — kept.
- **L1 (c) on top**: 231 joints fewer whole, 31 ends more, 1 152 plates
  more — left out again (the members bolted directly now keep clear of the
  plate in the plan, but bolting them directly does not lay out more).
- **Time** (every thirteenth tower, six, laid out back to back twice under
  the same load, as L1's times were taken): the median per tower 19.3 s
  before, 18.2 and 16.9 s with L2 — about the same or a little quicker
  (fewer clash rounds: the plan sees what they would find) —, the largest
  21.9 and 21.5 s before, 24.8 and 23.0 s with L2 (about a tenth more).
  The full columns ran several at a time on a shared machine: their times
  are not compared.

What is left: the ends off their flange needing rolls 5°–20° apart (about
29 000, L3); the plates that would run into something at the work point
(about 9 000 said in the plan: on a face with fixed steel at the work
point — a plate always reaches it); the bolts with a plate after writing
(about 3 400: mostly the nuts of members bolted through the flange behind a
plate on its other face); built-up members (about 8 300, J6); the bent
plate's own bolts through another member's hole (about 670).

## Measured (L2's follow-ups and L3, 2026-11-20)

On the same 78 tower files and settings as L2 (kept privately; J9's
proposals and J7a's leg splice places confirmed in every column, L1's (a),
(b), (d) and L2's three parts on). "Before" is L2's result (measured again:
the same to the unit); "(a)" the short plates alone, "(b)" the holding
bolts slid alone, "Part 1" the two with (c); "L3" the plates bent by a few
degrees on top (the default now). (c) changes nothing on these files (they
give no `onto` by hand, and the run replaced values given by hand): the
same as before on every thirteenth tower.

| | before | (a) | (b) | Part 1 | L3 |
|---|---|---|---|---|---|
| joints laid out whole | 48.6 % | 49.0 % | 48.6 % | 49.0 % | 49.7 % |
| … of the body | 48.2 % | 48.9 % | 48.3 % | 49.0 % | 49.5 % |
| … of the arms and the top | 49.0 % | 49.1 % | 49.0 % | 49.1 % | 50.0 % |
| joints with a hand detail | 43.5 % | 43.1 % | 43.5 % | 43.1 % | 42.4 % |
| member ends laid out | 61.8 % | 64.0 % | 61.9 % | 64.0 % | 64.7 % |
| joints that do not fit | 24.7 % | 24.5 % | 24.7 % | 24.5 % | 24.1 % |
| … of them as given | 42 | 34 | 36 | 28 | 28 |
| plates written | 58 424 | 60 379 | 58 641 | 60 543 | 62 936 |
| plates said in the plan to run into steel | 8 290 | 5 396 | 8 333 | 5 439 | 5 458 |
| ends left for a bent plate's own bolts over a hole | 668 | 639 | 129 | 127 | 132 |
| ends off their flange, at most 20° off, left | 17 840 | 17 384 | 17 836 | 17 381 | 15 907 |
| welded plates (step 4) / L3 bent plates written | 2 934 / 0 | 2 758 / 0 | 2 957 / 0 | 2 758 / 0 | 11 / 4 085 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 |

- **(a) Short plates**: 5 533 plates stop short of the work point (cut
  straight where needed); the plates said in the plan to run into steel
  8 290 → 5 396, ends laid out +3 848, joints whole +274. Some of the
  plates written so run into something after writing (plate with steel,
  left: 2 727 → 3 378) — the clash rounds take them back, as for any
  plate. The joints that do not fit as given 42 → 34. The rest of "its
  plate would run into": steel on the members' own path (a member left at
  the work point lying across the legs' band, a piece in line not one
  part), which no cut short of it clears.
- **(b) The holding bolts slid**: +13 joints whole, +134 ends, the joints
  not fitting as given 42 → 36; the ends left for a hole under a holding
  bolt 668 → 129 (with (a): 127) (not looked into yet: holes the plan does not see where
  they will be drawn); 33 bent plates found no place within the longest
  pull-back (said).
- **L3**: 4 085 bent plates written, and the welded plates of step 4
  2 758 → 11 (a bent plate fits wherever the welded one did: its curved
  part takes less room than the weld's gap) — the welds a galvanised tower
  avoids are almost gone. The ends off their flange at most 20° off left
  17 381 → 15 907 (under 5° off 5 817 → 4 647; 5°–10° 6 246 → 5 871; 10°–20°
  about the same): most bent plates are of 2°–5°. Of the ends 5°–20° off
  still left (11 260): their outstanding leg towards the flange (the roll
  that lays the other end flat turns this one's leg the wrong way round)
  4 100; on an inner face running towards the other leg 2 639; the axis
  more than a plate's thickness out (the low edge of the leg the far one
  from the bend) 2 598; running towards the bend away from the joint
  1 600. Joints whole +453, ends +1 136, plates +2 393.
- **The bend check (C12, the plugins' strict check, no relief)** under the
  comparison's one vertical case (64 towers with it): of the L3 bends
  checked (4 035), **3 365 pass (83 %)**, median usage 0.20, 90 % below
  1.49, the largest 6.97 (a 38 mm long bend of a 5 mm plate: the force
  across a short bend held at the far edge of the flat on the flange); by
  angle, under 5° 84 % pass, 5°–10° 81 %, 10°–20° 92 % (53 bends). Usages:
  ≤ 0.25: 2 256; 0.25–0.5: 596; 0.5–0.75: 324; 0.75–1: 189; 1–1.5: 269;
  1.5–2: 178; 2–3: 141; above 3: 82. The bends of J2b (90°, round the other
  leg) under the same case: 50 % pass, median 1.00, the largest 22. One
  vertical case, not the design's: an indication, not a check of the
  towers.
- **Time** (every thirteenth tower, six, laid out back to back twice):
  the median per tower 17.5 and 18.2 s before, 19.3 and 20.1 s with
  everything on (about a tenth more: the short plates' cuts and the bent
  plates' tries), the largest 24.1 and 24.3 s before, 21.4 and 22.0 s
  after. The full columns ran four at a time on a shared machine: their
  times are not compared.

## Measured (L3b, 2026-11-20)

On the same 78 tower files and settings as L3 (kept privately; J9's
proposals and J7a's leg splice places confirmed in every column, L1's (a),
(b), (d), L2's three parts, L2's follow-ups and L3 on). "Before" is L3's
result (measured again: the same to the unit); "(1)" the other leg tried
for the bent end alone, "(2)" the end running back to its bend alone,
"both" the two (the default now).

| | before | (1) | (2) | both |
|---|---|---|---|---|
| joints laid out whole | 32 193 (49.71 %) | 32 232 (49.77 %) | 32 201 (49.72 %) | 32 240 (49.78 %) |
| … of the body | 17 991 (49.5 %) | 18 027 (49.6 %) | 17 991 (49.5 %) | 18 027 (49.6 %) |
| … of the arms and the top | 14 202 (50.0 %) | 14 205 (50.0 %) | 14 210 (50.0 %) | 14 213 (50.0 %) |
| member ends laid out | 115 225 (64.66 %) | 115 424 (64.77 %) | 115 357 (64.74 %) | 115 556 (64.85 %) |
| joints that do not fit, as assumed / as given | 12 932 / 28 | 12 934 / 28 | 12 932 / 28 | 12 934 / 28 |
| plates written | 62 936 | 63 287 | 63 145 | 63 497 |
| ends 5°–20° off left | 11 260 | 11 086 | 11 211 | 11 037 |
| … their outstanding leg towards the flange | 4 100 | 3 948 | 4 108 | 3 955 |
| … running back towards the bend | 1 600 | 1 604 | 1 525 | 1 529 |
| L3 bends written; passing the bend check | 4 085; 83 % | 4 281; 83 % | 4 202; 84 % | 4 398; 84 % |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 |

- **(1)**: about 2 700 equal-leg members tried with their other leg, about
  250 kept: +39 joints whole, +199 ends, the ends with their outstanding
  leg towards the flange 4 100 → 3 948. The rest stay left for the reason
  found above (the axis more than a plate's thickness out, or the flat end
  on the flange's other face). The turn picked from the geometry alone
  (left out): on every thirteenth tower one joint whole fewer and 25 ends
  fewer than before.
- **(2)**: alone, the ends running back to their bend 5°–20° off left
  1 600 → 1 525, +8 joints whole (on the arms and the top), +132 ends, 117
  more L3 bends; on top of (1) the same again. Of the ends still left so,
  most now fail the axis limit by a few millimetres (their steel kept
  clear of the bend to the flange's toe lifts them).
- **The bend check (C12, unchanged)** under the comparison's one vertical
  case: the L3 bends 83 % → 84 % pass (median usage 0.20 → 0.22, the
  largest 6.97 as before); J2b's 50 % as before.
- **The holding bolts over a hole after writing**: 132 → 131 ends left so.
  Looked into: in every case seen the hole is the end of the member the
  plate is held to, not laid out at that joint, drawn after writing
  where the plan did not see it — its `onto` named only in the planning
  copy (about six in ten), or its end off its flange (about a third). Not
  fixed (no simple fix: the plan would need that end as it will be drawn
  without the run's values; a question to the owner).
- **Time**: every thirteenth tower, six, laid out back to back twice: the
  median per tower 19.3 and 19.7 s before, 20.0 and 21.0 s with both on
  (about 3–6 % more: the quarter turns' planning pass), the largest 21.0 and
  21.4 s before, 21.9 and 22.8 s after. The full columns ran up to eight at
  a time on a shared machine: their times are not compared.

## Measured (L5, 2026-11-20)

On the same 78 private tower files and settings as L3b (J9's proposals and
J7a's leg splice places confirmed, every switch before on). "Before" is the
main build of 2026-11-20 measured again (it differs a little from L3b's
column: the work merged since); "L5" with the moves (the default now).

| | before | L5 |
|---|---|---|
| joints laid out whole | 32 158 (49.7 %) | 32 652 (50.4 %) |
| … of the body | 17 909 (49.3 %) | 18 413 (50.7 %) |
| … of the arms and the top | 14 249 (50.1 %) | 14 239 (50.1 %) |
| member ends laid out | 115 759 (65.0 %) | 116 426 (65.3 %) |
| joints that do not fit, as assumed / as given | 12 999 / 26 | 12 926 / 28 |
| plates written | 65 878 | 61 800 (−6.2 %) |
| … their weight (the bill of materials) | 94.2 t (73 towers) | 90.8 t (−3.7 %) |
| shifts written | 346 | 2 333 |
| a run again identical | 78 of 78 | 78 of 78 |

- **The moves on all 78**: 1 987 shifts written that were not; 1 867 of them
  at most 8 mm (median 3 mm) — the moves; 112 above 14 mm, the ordinary
  shifts (up to the shift limit) of plans that changed with them. e·A/W of
  the new shifts median 0.37, nine in ten below 0.67. The 2 more joints not
  fitting as given: at both, a member left for a hand detail whose bolts (as
  they are) no longer fit the member laid out under it (L2's question 3).

- **The moves on every thirteenth tower** (six): the moves written 1–8 mm
  (median 3); the member's extra stress at its far toe from N·e over the
  axial stress (e·A/W, the angle's elastic modulus in the plane of its
  connected leg) median 0.37, at most 0.92. Moving only some ends of a face
  off its plate: more ends (+0.3 points) but fewer joints whole and more
  plates than moving whole faces — left out. Up to the shift limit (30 mm),
  on top of L5: +0.6 points of the joints whole, +0.6 of the ends, 12 %
  fewer plates, but moves of 18 mm median and e·A/W median 2.3, at most 4.8
  — not chosen.
- **One bolt through the leg** (looked into): of 6 626 flat ends going onto
  plates on the six towers, 63 % have one bolt; the placement fits 248
  (3.7 %), 128 without a move — within 26° of square to the flange, nine in
  ten with 20 mm bolts.
- **The bend check under the design cases** (L3's question 4): the line's
  load case library of the comparison (8 cases: −20, 10 and 40 °C; 0 °C with
  20 N/m of ice, ×1.3 on the vertical; 10 °C with 300 Pa of wind ×1.5 from
  the left, the right, ahead and behind, with the wind on the members of the
  body, 300 Pa × 1.5 on their profiles; **checking values, not a design
  code's**), the wire loads from the line's own calculation at the
  structure's points, each case solved as the analysis does (63 of the 78
  towers solved every case; in the others some cases did not settle — "axial
  compression above the elastic buckling load" — and those bends are
  checked with the cases that did), the ASCE plugin's strict check
  (unchanged): L3's bends
  (angle below 45°, 5 008 checked): **80 % pass under the vertical case
  (median usage 0.26), 57 % under the design cases (median 0.83, nine in
  ten below 4.1, the largest 22.9)**; J2b's bends round the other leg
  (4 871): 49 % (median 1.04) → **24 %** (median 2.88). The design cases
  give 2.3 times the vertical case's usage at the median; the ice case
  governs about four bends in ten, the wind cases the rest. 1 212 of the
  L3 bends and 1 273 of J2b's pass under the vertical case and not under
  the design cases.
- **A packing plate under the bent flap** (L3b's question 4; measured
  without L5, the axis allowed two plate thicknesses out instead of one):
  +1.0 point of the joints whole, +0.9 of the ends, the ends ≤ 20° off left
  1 052 → 917 on the six towers; of them those with their outstanding leg
  towards the flange (the packing's target) only 257 → 246, the rest the
  axis-limit group. Not built: the member a further plate's thickness out
  of the flange, an offset not in the analysis (NOT ON THE SAFE SIDE until
  O45 D).
- **Time**: every thirteenth tower, six, laid out back to back twice at a quiet
  moment: the median per tower 19.6 s before, 24.8 s with L5 (both passes
  alike), the largest 21.3 s → 26.7 s — about a quarter more: each joint
  planned twice (the guard) and the moves searched a millimetre at a time.
  The full columns ran beside other work on a shared machine: their
  times are not compared.
