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
there: an overloaded bolt would not be found), so such a pair stays a
bolted splice. A member joined as one part
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
bolted to it and the others left (since L6, J6: onto a plate on the angle
facing them where the member's angles are known and that lays out more,
[below](#plates-on-a-built-up-member-j6)).

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
- **Neither** (since 2026-11-22, L13, [below](#an-ends-host-chosen-by-the-plane-its-axis-lies-in-l13)):
  a member **ending** at the joint in a flange's plane of which the end's
  axis lies — a plan diagonal onto a face horizontal's level leg.

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
given beyond the leg's centre line is kept as given (the layout never
moves a gauge) and, since 2026-11-25, the ASCE 10-97 check adds the
bending of the part beyond the centre line to the curve's compression
(P·Δe about the angle's principal axes, amplified, Eq. 3.12-1's form;
[ASCE 10-97](asce10.md#bolts-beyond-the-legs-centre-line)) — before,
checked as normal framing eccentricity, NOT ON THE SAFE SIDE. What that
still leaves out (the angle's lateral-torsional buckling under the
moment: no rule in the standard) is said on the member. SIA 263 takes
the real gauge in its N·e already.

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
the moments where a frame member meets the joint; where only bars meet
it, the shifted member's own check takes the moment (since 2026-11-18),
and a cable's is left out and said per member and case, **NOT ON THE SAFE
SIDE** there (a cable cannot bend: the moment is missing, so the members
round that joint can carry more than shown; check them by hand) ([Eccentric member ends](structures-fe.md#eccentric-member-ends-rigid-end-offsets)).
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
than a code's would be, **NOT ON THE SAFE SIDE** in that sense: a bolt
nearer the edge than the code allows, with a smaller bearing or
tear-out capacity, would not be found; a code's plugin giving its
minimum, or a larger default, makes it safe) from it —
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
clash check sees it like any part. On a built-up leg no plate was laid
out (it is drawn through the work point, where its checks take it): a
hand detail — since L6 (J6, [below](#plates-on-a-built-up-member-j6)) a
plate on the angle facing the ends where the member's angles are known.

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
  not that it is two fillets — a fillet on one side only bent so can be
  weaker than the check takes; a field and a check change, not made:
  a question to the owner; meanwhile weld such a seam on both sides or
  check it by hand). ASCE 10-97 has no weld rule: not checked,
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
  on the row (a failure of the bend would not be found; a code's plugin
  with `joint-check-bends` checks it). No weld.
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
  of member i's bolts;
- (since 2026-11-25) **a frame member's end shear and moments**, where
  the case's end forces are known (OHL-Structure's runs): its shear
  \( \mathbf V_i \) (its end force on the plate square to its axis; the
  axial part is Nᵢ's, not taken twice) and moment \( \mathbf M_i \),
  carried to its bolts' centroid, \( \mathbf M_i + (\mathbf p - \mathbf c_i)
  \times \mathbf V_i \) (p the joint), and on to the bend's middle,
  \( + (\mathbf c_i - \mathbf o) \times \mathbf V_i \): the shear's part
  across the bend added to N, along it to V, square to the flat to V by
  size; the moment about the bend line to M by size, in the plate's
  plane to M_in, and its twist about the across axis a to M by size —
  *on the safe side*: a thin plate's largest torsion stress is
  \( 3T/(w t^2) \) (St-Venant's thin rectangle; S. Timoshenko & J. N.
  Goodier, *Theory of Elasticity*, 2nd ed., 1951), counted as a
  bending stress \( 6T/(w t^2) \), at least its von Mises equivalent
  \( \sqrt 3 \cdot 3T/(w t^2) \) and its Tresca one. Before, left out
  (said NOT ON THE SAFE SIDE below). No plugin interface changed: the
  same four values reach the plugin, larger.

*Conservative assumptions:* the members' moments about the bend line
added by size (the worst of their signs); the whole force across the
bend turned by the bend's angle and held at the farthest holding bolt,
the largest moment that flat's cantilever can give, **taken at the
bend** (where the flat bears on the leg's face, less); the normal and
the shear stresses taken at one point of the section (the codes'
checks below). **NOT ON THE SAFE SIDE** (by what it leaves out, so the
bend's stress can be higher than checked): the bend short across its
section — no buckling across it (the flats' own strut checks take
theirs); where the end forces are not known (a plate check given axial
forces only, `plate_usage_cases`), a frame member's end moments and shear
are not added at the bend — OHL-Structure's run gives them (above).
Not modelled yet: check such a bend by hand where its buckling may
govern.

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
  length: its bolts then go unchecked, so an overloaded bolt would not be
  found) — the owner's rule; to mark such a splice, give it by hand at
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
- **The end's extra offset** (since 2026-11-25): the end lies the cover's
  thickness **farther out of the leg** than lapped directly. Where the
  joint details are taken (level 4) that offset is an **offset of unknown
  sign** in the member's own check (`eccentricities::cover_offsets`, the
  core's `free_offsets`: along its bolts' axis, the outer covers'
  thickness on that leg; never in the solve) — its moment N·t whole and
  sign-free, *on the safe side* for the member, the joint's other members
  relieved of nothing; said in the run's notes ("… bolted through a
  splice's outer cover …"). Before, left out and said NOT ON THE SAFE
  SIDE. At level 3 the joint details are not read: as an end on a gusset
  plate there (O45 D), the offset is not analysed.
- **NOT ON THE SAFE SIDE**: the cover is a **packing** between the end
  and the leg: the bolt's bending across it is not checked — neither
  ASCE 10-97 nor SIA 263 gives a reduction for packings (searched, none
  found), other codes do for thick ones. A thick packing bends the bolt
  more, so its shear resistance can be lower than checked; until the
  owner chooses a rule (roadmap O44, the question of 2026-11-25), keep
  covers thin under such ends or check their bolts by hand.

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
gusset, usual for lattice towers, [above](#a-welded-plate-in-the-members-own-plane-step-4)):
its moment is missing, so the member's usage there can be too low; planned (roadmap O45 D: the seat offsets as rigid end offsets), meanwhile check such members by hand or keep a margin; (b) makes more such ends (measured below).

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
running on through the joint has no bolts of its own there), so the
plate's free length, and with it its buckling, can be worse than checked
— unchanged by the short outline; not modelled yet: check how such a
plate in compression is held by hand.

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
  as for every plate the layout makes (its moment is missing, so the member's usage there can be too low; planned (roadmap O45 D: the seat offsets as rigid end offsets), meanwhile check such members by hand or keep a margin). The estimated eccentricities (O45)
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
it (since L6, [below](#a-flat-plates-own-bolts-onto-the-member-running-through-l5o),
two provisional bolts of its own).

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
not in the analysis — **NOT ON THE SAFE SIDE** there (its moment is
missing, so the member's usage can be too low; turn *Estimated
eccentricities* on), as said for the shift ([above](#moved-sideways-the-shift)). Measured on every thirteenth
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

## Holding bolts, plates on built-up members and the clashes after writing (L6)

(The owner, 2026-11-20, on L5's questions: "Go ahead with it" — next L5o,
then J6, then a look into the clashes after writing, L5m. Built
2026-11-20, in active development, not approved yet. The code:
`joint_layout` (`hold_flat`, `plate_holds`, `HoldRoom`; the switches
`LayoutSettings::hold_plates` and `built_up_plates`, on by default) and
`joints` (`PlateSeat::on_given`, `PartAngle::r`). **No format change**.)

### A flat plate's own bolts onto the member running through (L5o)

A gusset plate the layout lays on the flange of a member that runs on
through the joint is bolted to its members, but nothing of the joint's legs
bolts onto it: how the plate passes their forces into the member under it
was not given, and the joint checks said its holding *not checked* (NOT ON
THE SAFE SIDE, said). Such a plate now gets **its own provisional bolts
onto that member** (`[plate.leg_bolts]`), as the bent plate's flat has
them (J2b):

- **How many**: two; more where one member on the plate has more bolts —
  the most of one member's. *The program's starting value, not a code's*:
  there are no forces when the layout runs; the joint checks then check
  the plate's holding bolts under the forces of each case (the resultant of
  the members' forces on the plate with its moment about the bolts, shared
  elastically; [joint groups](joint-groups.md)) and the member's bearing
  under them. Where more do not fit, fewer, not fewer than two, said in the
  plan's notes.
- **Where**: one gauge line along the member, the default minimum spacing
  apart; each bolt **within the flange** by the rules of every bolt bolted
  directly (clear of the angle's other leg and root by the nut's corner
  radius, the edge distance from the toe; past the member's cut by the end
  distance where it ends at the joint); **inside the plate's planned
  outline** by the plate's edge and end distances, the larger all round
  (*on the safe side*: the plate drawn round them stays as planned); the
  default minimum spacing from every hole through the flange and every
  bolt on the plate (*on the safe side*: the joint checks' rule is only
  that holes do not overlap); its head clear of the members' legs on that
  face and of the fixed steel over it by half the gap, its nut clear of
  what lies behind the flange; on the flat plate's side of a plate bent or
  welded from it, its thickness and bend beside the seam. The places
  nearest the work point first, the gauge line in the middle of the band
  first, whole steps of 2 mm from the work point (the same places at a
  joint turned or mirrored, so that a joint laid out once per kind matches
  one laid out on its own).
- **Not given**: on a plate stopping short of the work point (its outline,
  drawn round its members' legs and cut short of what it runs into, would
  change with them; measured: members then left after writing — since L9
  given where that outline grown round them still fits, below), on a
  built-up member (`[plate.leg_bolts]` names one angle's leg; not read for
  a built-up member yet), and where two do not fit — the plate laid out
  without them, its holding still *not checked*, said.
- **The guard**: the run's guard on L1 (a joint laid out with fewer ends
  after writing than its plan without L1's b and c — now also without L5o
  and J6 — is planned again without them) keeps them only where they cost
  no end.

### Plates on a built-up member (J6)

An end onto a built-up member (two angles back to back or in a star, four
in a cross) was already **seated on the angle of it facing the end**
(2026-11-11) and bolted to it where its bolts land on that angle's flange.
Where they did not, no plate was laid out: a plate at a built-up leg was
drawn through the work point (where a plate lies enters its checks, so it
was not moved), and the ends were left for a hand detail — about 8 600 ends
on the private real towers, the largest single cause after the clashes.

Now, where the member's **angles are known** (the section's parts), the
face goes **onto a plate on the angle facing its ends**, as on a single
angle: the plate names that member (`on`), and the joint details seat a
plate whose member is given on the angle of a built-up member facing its
members (its flange under the plate that angle's). A plate added by hand
without naming its member stays through the work point, as before.

- **Kept clear of the member's other angles**: the plan's plate check sees
  the built-up member's own steel in the plate's slab (the angle it lies on
  is under its face, or beyond its heel's cut). A plate **between the
  angles**, where the packing lies, was not laid out here (by hand, said);
  since L15 it is for the ends whose leg lies across the gap
  ([below](#a-plate-between-a-built-up-members-angles-l15)), as thick as
  the gap.
- **The guard — the clear rule**: each joint with a built-up member whose
  angles are known is planned with and without it; the plan with it is kept
  only where it lays out **more ends, or as many on fewer plates**.
- **On the safe side, as for direct bolting onto a built-up member**: the
  ends bolt into one angle of the member; the force reaches the others
  through the member's stitch bolts and packing, as the member's design
  assumes (the analysis and the member check take the built-up section
  whole, as before).
- **Found and fixed**: a built-up member's angles had no root radius for
  the bolts landing on them — a nut could sit on the fillet. They now keep
  the section's root radius (on the safe side: fewer ends land directly on
  built-up members than before).

### The clashes after writing, broken down (L5m)

What the after-writing clash check finds (the steps before cannot see it:
the plan's own check uses the outlines as planned, the run checks them as
drawn) was counted on the private real towers by the pair of parts and
what each part's member is in the plan: laid out, left for a hand detail,
or left as it is (in line, running on as one part, not a single angle, the
partner of a member bolting onto it). No change was clearly safe to make
from it alone; the counts and the proposals are in the measured section
below. One was tried and left out: the steel of a member not laid out that
sits on a member laid out, moved with it in the plan — no change on the
sample (the clashes are not of that kind).

## An end seated where its member is drawn; the measuring traced (L7)

(The owner, 2026-11-20, on L6's questions: the clashes after writing next,
the order "not a fixed decision"; trace the difference of the measured
columns; fix the bent plate's bolts drawn off its flat. Built 2026-11-20,
in active development, not approved yet. The code: `joints`
(`member_seat`, `nest`, `SEAT_PASSES`) and `joint_kinds`
(`map_hold_legs`). **No format change**.)

### An end seated on a member that is itself seated (the joint details)

A member end lapped onto an angle is drawn with its leg on the face of that
angle's leg (its *seat*, 2026-11-05). The face was taken where the angle
would lie **on its work line**. But the angle is often drawn off its work
line itself: seated on a third member, shifted, or moved by the layout so
that its gauge line passes through the work point. The end then floated off
the angle by that much, in the drawings and in every check that reads
them, while the automatic layout planned the end, its plate or a plate's
own bolts on the angle where it is drawn.

An example (invented): a redundant R bolts onto a diagonal D, and D onto a
leg L. D is seated on L's face, 28 mm off its work line across the faces.
R was drawn on D's face as if D lay on its work line — 28 mm off D, its leg
through steel or in the air; a plate the layout laid on D for R's
neighbour was planned on D as drawn, and the clash check after writing
found the plate in R.

Now each end is seated on the angle **where that angle is drawn**: the
angle's own offset (its seat, shift and drawing offset) taken across the
face the end lies on (only across: along the face nothing changes), nested
ends in the angle as drawn too. The ends are worked out again with the
offsets of the pass before until no member some end bolts onto moves
(a chain of members each seated on the next needs one pass per link; at
most four, `SEAT_PASSES` — the program's bound, not a standard's). Two
ends bolting onto each other do not follow each other (they would chase).

*On the safe side*: the drawings, the clash check and the joint checks
see each end where it lies on what it bolts onto; before, clashes could be
missed or found where there were none. It changes the details of any file
with such ends, not only laid-out ones (files at level 3 and 4).

### A plate's own bolts named on the right leg at a joint like another

The layout lays out a joint once per kind and maps the plan onto the
joints like it (J4). A flat plate's own bolts name the leg of the member
they bolt into (`long` or `short`, L5o). Under a mirror, an angle's leg
along its section's y can be the matched member's leg along its z: the name
was copied, and the bolts were drawn on the other leg, off the plate. Now
the name is the leg the map carries the representative's leg onto.

### The measured columns traced (L6's question 6)

L5's measured "after" column laid out 32 652 joints whole with 61 800
plates; L6 measured the same code at 32 376 joints and 65 240 plates. The
code did not change between them. The cause is how the sweep ran: the
bundled code plugin (the default member check's bolt minimums: end, edge
and spacing distances) is found beside the program or above the current
folder. L5's later runs started in a folder where that plugin had been
built (for its design-case bend check), its earlier ones and L6's did
not: L5's "after" column used the code's minimums, its "before" column
and L6's the structure's default minimums. **A difference of the
measuring, not a regression and not a merge effect.** On the same
footing, L5's ends moved to bolt directly gave +218 joints laid out whole
(not +494) and 1.0 % fewer plates (not 6.2 %). The program as shipped
always finds the bundled plugin; from L7 on the measurements run with it.

### The bent plate's own bolts off its flat (L6's question 7)

Traced on the private towers: in most cases the member the bent plate's
flat lies on is **not laid out at that joint** (left for a hand detail, or
left as it is), and the run had **named what it bolts onto** in its
planning copy (an end bolting onto nothing named gets the member running
through the joint, L1 d's choices too). The plan sees that member seated on
the member named; the file does not get the name (the member is not laid
out), so it is drawn where the details work it out — elsewhere, up to tens
of millimetres. The flat was planned where the member was seen, its own
bolts are drawn on the member where it is: off the flat. The joint is then
taken back after writing (as before: no wrong part is written).

Tried and left out: writing the name for such members (those something the
plan writes lies on or bolts onto). The flats' bolts then land (on six
towers: 17 → 12 ends left for it), but those members' own assumed bolts
are then counted as placed by the layout (*given*: the decision of
2026-11-19 on what is given), and the joints not fitting as given rise
(34 → 102 on the six towers) for no more ends laid out. The fix proposed:
plan such a member where the file will draw it (the planning copy without
the run's names for the members it does not lay out) — in the roadmap.

### What is left after writing, and in which order

The owner's order of 2026-11-20 (plates into steel left as it is, members
left as they are no longer fitting, heads and nuts against plates) was a
starting point. Measured, the first cause of all three groups is the same:
ends drawn off the member they bolt onto (the seat above). Fixed first,
it removes about a third of the plates into steel and of the nuts against
plates; the members left as they are no longer fitting barely change
(their cause is another: a member left bolting onto one the plan cuts
back). The rest is counted in the measured section and proposed in the
roadmap.

## A joint planned where its members are drawn; the partners' bolts kept (L8)

(L7's proposals 3 to 6 in the order L7 recommended, the owner busy: built
on its own, reported. Built 2026-11-21, in active development, not
approved yet. The code: `layout_run` (`LayoutSettings::as_drawn`,
`AS_DRAWN_PASSES`) and `joint_layout` (`LayoutSettings::partner_bolts`,
`partner_misfit`, `PARTNER_LEFT`). **No format change**.)

### A member named but not laid out, planned where it is drawn (L7's proposal 3)

The run names what an end bolts onto in its planning copy where nothing
names it (the member running through the joint, and L1 d's choices), so
that the end's leg and turn are those on that member. It writes the name
only for an end it lays out. An end it does not lay out (left for a hand
detail, or left as it is) is then drawn without the name — where the
joint details work it out, often tens of millimetres away from where the
plan saw it. Whatever the plan laid on that member — a plate, the ends
bolting onto it, a bent plate's flat with its own bolts — was planned on
it where it no longer is, and the clash check after writing took those
ends back (L6's question 7: the bent flats' bolts outside the flat; also
plates and steel planned clear of a member drawn elsewhere).

Now, after the clash rounds, each joint with such an end is **planned
once more with the names of those ends taken out** of the planning copy —
the member where the file will draw it — and written again. It is kept
only where the joint then lays out more ends, or as many on no more
plates, and every member it laid out or left for a hand detail is still
one of those (no member it left for a hand detail now left as it is,
unsaid); else the names go back and the joint is planned as before. The
name stays where the end has its face given by hand (given for the member
named). At most two such passes (`AS_DRAWN_PASSES`, the program's bound;
the second rarely finds one more joint). Only the joints whose names
changed are worked out again; the others keep their plans of the pass
before (measured: the same layouts as planning every joint again, a
little quicker). Each joint so planned is said in the run's notes.

An example (invented): a redundant R bolts onto nothing named; the run
names the diagonal D running through, so R would lie seated on D. R is
left for a hand detail (too short for its bolts). A plate for the other
ends was planned clear of R where it lay on D; the file draws R on its
work line, 20 mm off, through the plate: after writing the plate's ends
were taken back. Planned again with R where the file draws it, the plate
keeps clear of it and the ends are laid out.

*On the safe side*: the plan sees the steel where the drawings and the
checks will see it; nothing is written that the checks after writing
would not pass (they run as before).

### The bolts of a member left as it is kept on the end it bolts onto (L7's proposal 4)

Of two members bolting onto each other the plan keeps one as the host,
its own end left as it is (the mutual-host guard, 2026-11-19), and lays
the other out onto it. The host's own bolts still go onto the member laid
out. The plan moved that member — its gauge line through the work point,
a shift — and cut it back, without asking whether the host's bolts would
still lie on its flange; after writing they often did not (off its
flange mostly), and the member laid out was taken back.

Now such an end keeps those bolts: a cut and move is taken only where
every hole of the member left as it is lies within the end's flange (the
joint checks' rule), past the end's cut by that bolt's end distance (the
code's, else the structure's default minimum: **assumed** where no code
gives one), and the end's own bolts the default minimum spacing from them
(also assumed). Where no cut within the longest pull-back and no move
within the shift limit does, the end is left for a hand detail in the
plan, said ("a bolt of a member left as it is that bolts onto it would not
stay on it…"), its place not kept free (as an end taken back after
writing) — before, it was laid out and then taken back. A member left by
the run's own rounds is not asked (its hand detail places its bolts, as
after writing). Measured on the 78 private towers: the ends taken back
after writing for it fall by nine tenths, about as many ends are laid out
(a dozen more) on fewer plates, a few dozen fewer joints whole; one joint
(repeated on seven towers of one type) now does not fit as given, said by
the joint checks — a member nested in the partner there carries its bolts
onto the end now left. Tried and left out: planning such a joint again
without the rule and keeping the plan with more ends — it laid out fewer
after writing.

*On the safe side*: the host's bolts stay on steel with their end
distance; the distances are the assumed defaults where no code gives
them, and the joint checks check them as they do every bolt.

### The bolts already there, found wherever the structure lies (found measuring)

The plan keeps an end clear of the bolts of the joint's members it does
not lay out lying on the same face. Those bolts were placed with the
joint's position added once more: they were missed unless the face passed
near the structure's origin (on real towers, almost never). Fixed: the
plan keeps clear of them as intended (*on the safe side*; the clash check
after writing saw them before too). Small on the towers measured.

### Heads and nuts against plates (L7's proposal 5) — tried, not kept

Keeping a member left for a hand detail clear, as high as an end's
outstanding leg reaches and behind the face where its nuts lie (as the
fixed steel is), changed nothing on six towers: not kept. Traced: the
heads and nuts against plates after writing are nearly all (on the 78
private towers, 1 751 of 1 754 ends) the **end's own** bolts against
another plate laid out at the same joint, not a hand member's (where the
plan misses them is the next thing to trace: the plan sees the ends and
plates of one plane against those of the planes before it). In the
roadmap.

### Plates into pieces running on as one part (L7's proposal 6) — traced, not fixed

Traced on one tower: the plate lies on a member onto whose face the run
of two pieces (one part through the joint) is also lapped; the plate's
outline, worked out round its bolts and the work point after writing,
reaches over the run, and runs into both pieces by the run's leg
thickness. The plan's check of the plate against the fixed steel uses its
planned outline. No change was clearly safe to make in the time: in the
roadmap (the plate's planned outline against the run, or the outline cut
short of it after writing as a plate stopping short is).

L5o on plates stopping short: not reached.

## A plate read in its own plane, plates planned as written, short plates held (L9)

(L8's questions 4, 5 and 6 in L8's order, then the run made faster; the
owner busy: built on its own, reported. Built 2026-11-21, in active
development, not approved yet. The code: `joints` (`plate_seats`,
`bolts_near_edge`, the seat passes), `joint_layout`
(`LayoutSettings::reach_clear`, `LayoutSettings::short_holds`,
`short_round_holds`, `poly_need`), `layout_run` (the clashes after
writing) and `ohl_sections::Section::elastic_properties`. **No format
change**.)

### A plate's plane read from its whole outline (L8's question 4)

L8 traced the heads and nuts against plates after writing to an end's own
bolts. Traced further: nearly all of them lie against the end's **own**
plate, its bolts lying along the plate instead of through it. The run
writes a plate's outline (round its bolts, cut at the heel) to a tenth of
a millimetre; the joint details read the plate's plane back from it — from
its **first three corners**. Where the outline runs straight there (often:
its first corners on a cut), those three lie in a line, or nearly: the
plane came out far off. The end on the plate then took its other leg as
the one bolted (the one lying most nearly in that plane), and its bolts lay
along the plate. On the 78 private towers' written files, 12 345 of
56 513 plates' outlines (22 %) were so read more than 2° off, 4 615 more
than 45°.

Now a plate's plane is that of **all its corners** (Newell's method — the
clash check, the drawings and the plate checks already took it so), and a
plate's bolts are measured from its edges in that plane too. Every file
with plates whose outline is given is read so: also files laid out before,
and plates drawn by hand. *On the safe side*: the plate and the ends on it
drawn where they are; the plan was not wrong here — the file read back was.

What is left of the heads and nuts against plates (L9 Q1 in the roadmap):
mostly an end's own nut running 1 to 4 mm into its own plate where two
ends on one plate lie up to a few degrees off each other (each within the
owner's 2° of the flange it bolts onto) — the plate drawn in the plane of
the first end's bolts —, and a few ends on the plate's other face.

### An end on a plate on a member drawn off its line (found with L8's question 5)

The joint details seat each end where the member it bolts onto is drawn
(L7), passing over the joint again while some member moves. Whether to
pass again asked only whether some end bolts onto the moved member
directly. An end on a **plate** lying on that member was not asked: where
nothing else bolted onto the member, the plate and the ends on it were
drawn as if the member lay on its work line — off it by its seat and move
(found: a plate on a member seated on a plate, 31 mm off it, through the
run beside it). Now the member a plate lies on counts as well. *On the
safe side* (as L7): the drawings and checks see the steel where it lies.

An example (invented): D1 lapped on the leg's flange (seated across it);
D2 on a plate lying on D1, nothing bolted on D1 directly. Before, the plate
lay where D1 would lie on its work line — tens of millimetres into the leg;
now on D1's face.

### A welded plate's flat plate planned as written (L8's question 5)

An end no roll lays flat goes onto a plate in its own plane welded (or
bent) to a flat plate on the flange (step 4). The flat plate's outline,
worked out after writing, covers the seams and the flange under them and
under the work point, and the bolts of the ends flat on it. The plan never
checked that outline against the fixed steel — only the outline round the
flat ends' bolts, and not at all where no end lies flat on it. Where a run
(two pieces as one part, a leg) lay on the same face, the flat plate ran
into it by the run's leg thickness: the largest kind of plate clash left
after writing (L7's proposal 6).

Now the plan checks the flat plate **with its outline as the run will
write it** (`LayoutSettings::reach_clear`, on): round the work point, the
seams and the flange under them, kept to the seams' sides, cut at the heel
— against the fixed steel and what the planes before laid out, in its
thickness, as a flat plate's. Where it runs into something it **stops
short of the work point** — the seams and the flange under them only, and
the flat ends' legs from the flange's toe — and is cut straight short of
what it still runs into (each seam and each flat end's bolts with their
distances kept; at most `CUT_TRIES` cuts), kept where it then still lies
on the flange over half a square of its width. Else the ends in their own
planes are left for a hand detail in the plan, said ("that plate would run
into …"); the ends flat on it keep their plate, checked before. Of the two
ways L8 asked about — planned as written, or cut short as a short plate is
— both are used, in that order: the plate as written where it fits, cut
short only where it does not (the measurements below).

And after writing, a flat plate with nothing bolted on it belonged to no
member: a clash of it was nobody's, and stayed written. Now it is its
welded plates' members' (they are taken back for it, as for any clash).
**Before, such clashes were written unsaid — NOT ON THE SAFE SIDE before
L9** (a plate that does not fit was shown as fitting); fixed.

*On the safe side*: the plan sees the plate as written; what the checks
after writing would take back is not planned. Conservative: a plate cut
short may leave a member for a hand detail that a plate shaped by hand
would carry.

### A plate stopping short of the work point held by its own bolts (L5o, L6's question 2)

Since L5o a flat plate on a member running on through the joint has its
own provisional bolts onto that member (`[plate.leg_bolts]`), so that how
it is held is checked; not on a plate stopping short of the work point,
whose outline the bolts would change. Now (`LayoutSettings::short_holds`,
on) such a plate gets them too where its outline, **grown round them as
the joint details draw it** — its ends' short outline, each holding bolt
with the largest end or edge distance its plane's ends keep, and, cut
straight, the code's for the member it lies on (in the plate's thickness)
—, the flange under all of it as far along as those reach, cut at the heel
and by its straight cuts, still runs into nothing and keeps every holding
bolt that far from its edge; else none, said (as before). *An assumption,
on the safe side for the fit*: the plate as large as the largest of those
distances asks.

### Faster, the same results

The run took about 45 s per tower after L8 (median, six towers). Profiled
(the system's sampler on one tower): about half the time planning the
joints (most of it the least cut keeping an end clear of a footprint —
three polygon clips each, with a new list each, for every footprint and
every place tried), a third drawing the joints in detail after each round
(of which a tenth working out every section's plastic moduli, which the
joint details never read), and some looking up a joint's connections in
the file text. Made faster without changing a number:

- the least cut: a footprint wholly on one side of a clipping line is kept
  as it is, or is gone, as the clip would give it — clipped only where the
  line crosses it (and the clip's list made at its size once);
- the sections of the model the joint details build: their plastic moduli
  not worked out there (`Section::elastic_properties`: every other
  property the same, by the same arithmetic);
- a connection with more than two fields is not empty, without looking up
  its joint.

The written towers before and after were compared byte for byte on all 78
towers: the same. The time per tower (six private towers, back to back)
went from 45 s (L8) to 42.5 s with L9's other changes and to 33 s with
these (the median; the largest 50 → 45 → 34.5 s): below.

## Members on either side of a leg's flange; bolts that join nothing; a plate in its flange's plane (L10)

(The owner, 2026-11-21: L9's questions answered "Keep what's built and
move forward"; the owner's photographs of real towers — horizontals bolted
on the outer face of the leg's flange, the diagonal on its inner face,
meeting at one point without a plate —; the owner's picture of two bolts
at a laid-out joint joining nothing. Built 2026-11-21, in active
development, not approved yet. The code: `other_face` and `layout_run`
(`other_faces`; the switches `LayoutSettings::other_face`, `face_pattern`,
`nothing_joined`, on by default), `joint_layout` (`BOLTS_JOIN_NOTHING`),
`joints` (`plate`) and `clash`. **No format change**.)

### Members on either side of a leg's flange (the owner's idea)

Until L10 an end kept the face of the flange its member's roll gave it
(Orient angles puts a face's bracing on the outer faces of the legs); the
layout turned a member half a turn only where an end of it could not be
laid out at all on the face it had (above, *Half a turn*). Now, where two
or more ends lie on **one face of a leg's (or chord's) flange** and go
onto a plate there, or one of them is left for want of room beside
another, the run tries some of them on the flange's **other face** —
half a turn of its member, its outstanding leg to the other side, the same
leg bolted —, all such tries of the tower at once:

- **Which ends**: of a face's ends (single angles flat on the flange,
  their face not given by hand, their member not a leg or chord, not one
  part with another, its roll not given by hand, not turned already in
  this run), with `face_pattern` (on) **those following the photographs'
  pattern** — the diagonals from the outer face onto the inner one (all of
  them, while one end at least stays), a horizontal from the inner face
  onto the outer one —; else one end, the later class first (of two
  members in one clash the later class gives way), the narrower leg, the
  id.
- **The guard** (the owner's weighing rule): a member is kept turned only
  where its joints then lay out **more ends, or as many on fewer plates**,
  and none of them fewer; planned once more with only those kept, until
  each still helps (at most three plans). A half turn moves every end of
  the member, so the guard weighs all its joints.
- **What the end must keep** on the other face is what every end on a
  face keeps (the search's own rules): on the inner face clear of the
  angle's other leg and its root (by half the gap), its bolts on the
  flange at least the angle's thickness, root radius and the nut's corner
  radius from the heel and the edge distance from the toe (the code's,
  else the structure's default minimums — assumed), clear of the bolts
  and nuts of the ends on the other face.

*The offset, and why it is not written as a shift.* On the other face the
end's leg lies the flange's thickness and its own (t_F + t_m) from where
it lay — **across the flange, out of the face**, along its bolts. The
connection's `shift` is a move *in* the face (across the member towards
what it bolts onto, [Moved sideways](#moved-sideways-the-shift)), which
the analysis takes as a rigid end offset; written there, the member would
be drawn and analysed that far beside its place in the face, where it is
not. The move across the flange is a **seat**, as every end's on a face:
the analysis takes the members' axes through the work point and leaves
out every seat across a flange — the outer face's as the inner's, and a
gusset plate's thickness (O45 D) —, **NOT ON THE SAFE SIDE in that sense,
for every end, before as now** (the seat's moment is missing, so a
member's usage can be too low; planned, roadmap O45 D: the seat offsets as
rigid end offsets — meanwhile the codes' curves for angles bolted by one
leg cover the usual framing eccentricity, not more). Measured from the work point across the
flange, an end on the **inner** face lies **nearer** than on the outer
one: the angle's centroid (on the work line) lies inside its heel by c
(about 0.28 of an equal angle's leg), and the end's leg then lies at
c − t_F − t_m/2 instead of c + t_m/2 — nearer whenever t_F ≤ 2c, which
holds for every rolled angle —, and nearer still than on the plate it
saves (a plate's thickness farther out). L5 limited a move in the face to
the plate's thickness, so that its force passes the work point by no more
than the plate would have put it out. The same rule here: the half turn
is **applied only where every end of the member so moved lies no farther
from the work point across the flange than it lies now** (on its plate, or
on the flange) — then less is left out of the analysis than before. The
other way, an end from the inner face onto the outer one (t_F + t_m
farther out), is **only proposed** in the run's notes, with the bending
stress that offset would add at the member's far fibre over its axial
stress (e·A/W, from its outline), not applied.

Measured on the private towers: on every thirteenth (six), the step alone: ends laid
out 9 519 → 9 541, joints whole the same (2 615), plates 5 085 → 5 109,
a run again identical 6/6. With all of L10, the time per tower (six
towers, back to back, twice, load 2 to 3) median 33 → 35 s, the largest
34.5 → 38 s (the step plans the tower once or twice more). The
plan's guard holds joint by joint (tried: weighing the member's joints
together, the same result); the plates written grow where a joint then
lays out more ends (an end that was left now on a bent plate, say), and
where the clash check after writing puts an end bolted directly in the
plan onto a plate. The photographs' pattern against the later class first:
the same ends, 8 more plates — no gain measured; kept (the owner's
pattern, no end lost). On the 78 towers, alone: ends 128 092 → 128 237, joints
whole 36 130 → 36 102, plates 69 295 → 69 320, joints not fitting as
given 53 → 51, a run again identical 78/78; 1 586 ends turned onto the
other face (1 385 onto the inner one), 166 proposed the other way. Small:
on most faces going onto a plate the ends lie too close for either face
alone (the bolts and nuts of the other face in the way), and where the
plan saves a plate the clash check after writing takes some of it back
(to trace next).

### Bolts drawn that join nothing (the owner's picture)

An end left as it is that bolts onto nothing at the joint — of two pieces
in line not confirmed one part, the splice there not given — is drawn
with its bolts where it has them: given at the end, else its member's
design data (often a splice's, on both legs). They join nothing: no
member's or plate's steel under them. The layout counted such a joint as
laid out whole when its other ends were laid out (the owner's picture: two
bolts standing on a leg joined to nothing, a plate beside them that could
not get its own holding bolts for their holes). Now (`nothing_joined`, on)
such a joint is **not whole**: that member by hand, said with why ("its
bolts join nothing — its 4 bolt(s) (its member's design data, 2 on each
leg) are drawn at this joint, but it bolts onto nothing here; by hand:
what it bolts onto or its splice given, or the pieces in line confirmed
one part"). Its bolts are **not** moved onto the layout's plate as the
plate's holding bolts: they are the end's own (in the member's design
data, for a splice's two shear planes, say), checked as such — using them
as the plate's would change what they are and what is checked (a question
to the owner). Confirming the pieces one part (J9) or giving the splice
removes them. Since 2026-11-22 (L14), where a plate of the layout lies on that
member's own flange, its end is bolted onto the plate and its bolts are the
plate's holding bolts ([below](#the-base-members-end-bolts-as-the-gussets-holding-bolts-l14)). On the safe side (a joint said not whole that the drawing
showed whole).

Measured: before, on the 78 private towers 1 289 such ends (3 217 bolts) lay in joints
counted whole (of 13 473 such ends at 9 558 laid-out joints); after, none in a
joint counted whole (on six towers the joints whole 2 615 → 2 533 for it,
nothing else changed; most such ends — nine in ten — already lay in joints
with a hand detail).

### A plate in its flange's plane; an end's own head and nut touching its own plate (L9's question 6)

L9 left about 790 heads and nuts against plates after writing, nearly all
an end's own nut 1–4 mm into its own plate where two ends on one plate lie
a few degrees off each other (each within the owner's 2°). Traced (a
private measuring hook, removed): a plate the layout leaves to the joint
details to draw round its bolts lay in the plane of **its first member's
leg** — up to 2° off the flange it lies on; a second member 1–2° the other
way then lay into the plate by up to 4 mm over its bolts (the members are
seated on the flange at the work point, and a plate's thickness out along
their own leg's normal), and its nut, a grip's length behind its leg, with
it. Measured first, on six towers: the plate drawn in its flange's plane
alone made it worse (every end a little off the flange then had its nut
in the plate); with an end's own head and nut on its own plate touching
it, both together laid out the most.

- **The plate in its flange's plane** (the joint details; every file): a
  plate not given an outline, on a single angle's flange, all of whose
  members lie within the flatness allowed (`FLAT_OFF`, 2°) of that flange,
  is drawn in the flange's plane — its face against the flange's face its
  members lie over, their face a plate's thickness out. Otherwise (a member
  more than 2° off: a hand-made plate, perhaps bent) as before, in its first
  member's plane. The layout writes such a plate's outline so, and reads it
  back in that plane.
- **An end's own head and nut on its own plate** (the clash check): where
  the end's leg lies within 2° of the plate (its bolts' axis within 2° of
  the plate's normal), its head and nut lie on the plate's faces — they
  touch, as on a flange; checked as clashing only beyond that. This is the
  owner's 2° rule (a leg within 2° of its face is drawn and checked as
  flat on it, the bolts pulling it flat — **an assumption**, the smaller
  the nearer the safe side): drawn, such a leg lies up to its 2° off the
  plate, so its nut a few millimetres into it.

Measured: on six towers (with the steps above), the heads and nuts
against plates after writing (ends taken back or left as they were)
58 → 17 with the touching alone, → 5 with the plate in its flange's plane
as well; the joints whole
2 533 → 2 565 → 2 578, ends 9 541 → 9 622 → 9 694, plates 5 109 → 5 131
→ 5 180 (more ends kept on their plates); the plane alone (tried first)
2 531 → 2 187 joints whole — not kept alone. On the 78 towers (all of
L10): nuts into plates after writing 776 → 20 ends, heads 19 → 5, and
taken back for them 47 → 27 and 31 → 21; members left after the clash
rounds 10 619 → 9 438.

### Plates into steel after writing traced (L9's question 7)

Traced (a private measuring hook, removed; two private towers, every
clash of a plate the run made with a member's steel over all the run's
rounds, about 2 900 pairs): seven in ten are against members left for a
hand detail at that joint (said with that member, no end taken back for
it: J2's rule), three in ten against members left as they are, a few
against ends laid out. Of those that take ends back, two in three are
**plates into a piece running on as one part through the joint** — a bent
plate's flat (the most), its flap in the member's own plane, a flat plate
—, mostly at crossings where the other pieces lie 20° to 45° off the
continuous piece's flange (no roll lays them flat) and the joint ends as a
hand detail anyway; then plates into the member the plate's end bolts onto
at its other leg, and others. A plate's own bolts outside its outline:
counted (below), not traced further. **Not fixed**: no kind was clearly
safe to change without the plan seeing that plate as written against that
piece (as L9 did for the welded plates' flat plates) — the next step
(a question to the owner).

### The tests L8 could not write (L9's question 8)

Built from invented joints: the bolts already there kept clear of far from
the structure's origin (an end pulled back further past a lapped leg's
bolts, the same with the whole tower a kilometre away; before L8 they were
missed there); a plate stopping short of the work point (a channel beside
it) given its own holding bolts with `short_holds`, none without. **Not
built**: an end keeping its partner's bolts — on every invented joint
tried (a horizontal in two pieces and a redundant, of narrow and wide
angles, one or two bolts, gauges given) the end goes onto a plate past the
other member's toe, its cut then always past the partner's bolts (left
for L11).

### Both faces of one plate (the owner's second picture): traced, left

The owner asked of a joint on a real tower whether two diagonals could lie
on the two faces of one plate, bolted on different sides, instead of a flat
plate and a bent one. On the same tower laid out as the measurements run
(Orient angles, the pieces in line confirmed one part), that joint lays out
both diagonals bolted directly onto the horizontal's flange, no plate; the
picture's state came from other rolls. Ends on both faces of one plate (the
plate standing beyond the flange's heel, an end on its back face, bolts
taking both plies and the plate where they share holes) are a new kind of
plate for the planner, the clash check and the joint checks: **not built**
(left for L11; built with L11 below).

## Solve the clashes, an end moved to pass the flange on a plate, both faces of one plate, a leg turned against its joints (L11)

(The owner, 2026-11-21, from four pictures of a real tower: a leg splice
whose bracing "need a horizontal shift and a pull back as well as a plate,
then one plate for each plane of the main legs and everything bolted to
the plates"; two diagonals that "could be on the same plate just bolted on
different sides"; leg pieces "rotated along their axis 5° out of what
would be needed for the joints"; and Joint details' list of problems,
"this huge list that's a bit unreadable and where buttons don't seem to do
much". Built 2026-11-21, in active development, not approved yet. The
code: `solve` and `joint_fixes` (moved from the application), `both_faces`,
`leg_roll`, `joint_layout` (`shift_past_toe`, the switches
`LayoutSettings::plate_shift` and `both_faces`, on by default), `joints`
(`PlateSeat::back`), `layout_run` (`LayoutRun::leg_rolls`); the
application's `solve_window` and `joint_panel`. **No format change**.)

### Solve the clashes: one solution of a joint, weighed

A joint's problems were offered one by one, each with the fixes that
worked on a copy (2026-11-08). Now they are listed grouped and closed, and
one window works out the joint's best **whole** solution on copies of the
structure, in this order:

1. the layout of the joint alone (as Lay out joints, values given by hand
   kept), then with the values given by hand replaced;
2. where the layout proposes pieces in line one part or a leg splice place
   at the joint (J9, J7 — never written by a run), the same with those
   confirmed (**marked** in the solution's effects);
3. on the best so far, the fixes of the problems left
   (the joint fixes: the other face, a turn, a gauge or end
   distance, nesting the other way, lapping), one at a time, each kept only
   where the joint has fewer problems after it (at most 24 tries a round,
   four rounds: the program's bounds).

(Between 1 and 2, where the layout leaves something, the same with an end
moved sideways to pass the flange on a plate — below.)

**Weighed** by the clashes at the joint, then its other problems, then the
members left for a hand detail (fewer is better); the first of equal ones.
**Refused**, whatever its counts: a way that pulls back a leg or a member
running on through the joint (one part, or a piece in line with another
there — the owner's example of a main leg's "steel stops at 17 mm" at a
splice), turns a leg, changes a member's section or moves its ends, or
leaves more clashes and problems at the other joints of the members
meeting there (a member turned turns at its other end too). The fixes no
longer pull back such members either. Where nothing is better: said, with
each way's counts and what blocks the layout.

### An end moved sideways to pass the flange on a plate

An end going onto a plate must have its bolts past the flange's edge by
their nut and the gap (on the plate, beyond the flange); where that needed
more than the longest pull-back, the end was left for a hand detail. Now
it is moved sideways — the least whole millimetre within the shift limit —
until its bolts pass within the pull-back, clear of the plane's other ends
as they lie and of the fixed steel, keeping a partner's bolts (L8): the
owner's "horizontal shift and a pull back as well as a plate". The move is
the end's `shift`, an eccentricity the analysis takes as every shift
(level 4, and level 3 with the estimated eccentricities; where only bars
meet the joint the member's own check takes the moment; a cable's left out,
**NOT ON THE SAFE SIDE**, as before: a cable cannot bend, so the moment
is missing and the members there can carry more than shown — check
them by hand). Kept at a joint only where its plan
lays out more ends, or as many on fewer plates (planned with and without).
**Off in a run of Lay out joints, tried per joint by Solve the clashes**:
measured on six private towers it laid out fewer ends at 11 joints and
more at 8 (an end moved sideways takes room its neighbours then miss after
writing), so it is not better in a run; for one joint, where the outcome is
weighed after writing, it is tried as a way of its own.

### One plate per plane; both faces of one plate

**One plate per plane**: two flat plates of a plan on flanges in one plane
(normals within 2°, 1 mm apart at most) of members one part through the
joint (a chord whose pieces are named apart) are one plate, its ends
shared, as thick as the thicker.

**Both faces of one plate**: an end the plan puts on a plate in its own
plane (bent or welded, step 4) whose leg lies flat (within the owner's 2°)
in a flange's plane on the flange's **inner** side and leaves the joint
over the heel goes onto the **back** of the flat plate on that flange's
outer face — past the heel the flange is not there, the plate's back is
free. Its cut: its steel clear of the heel's line by the gap over its leg's
whole width on either side of its gauge line (**on the safe side**: the
leg lies on one side only), its bolts by their nut (the drawn nut's corner
radius) and the gap; at most the longest pull-back and its half length less
its bolts. The flat plate is then no longer cut at the heel; the own-plane
plate, and the flat it was bent from or welded to where nothing else is on
it, go. The run's clash check after writing takes back an end that still
runs into something; and **the run's guard** (`PlanCtx::no_l11`, as L1's):
a joint these steps changed that lays out fewer ends after writing than its
plan without them is planned again without them.

**Drawn there** (the joint details, every file): a plate on a leg's flange
with members on both sides of that flange — some with their own side
(where their bolts' heads are) out of the angle, some into it — has the
latter on its back: seated on the flange's inner face, then out by the
flange's thickness, their leg against the plate's back. Before L11 such a
member, given by hand, was drawn the flange's thickness and the plate's off
the plate, its bolts' grip as if it touched (a drawing that the clash check
could not see; on the safe side now: the member lies where it bolts). A
plate whose members are all on the inner side is still a plate on the inner
face, as before. Its plane and the side it is drawn through its thickness
are those of the members on its front.

### A leg turned against its joints

From the joints themselves: an angle bracing end can lie flat on a leg's
flange only when its axis lies in that flange's plane (its leg bolted
contains its axis). For each end bolting onto a leg piece (directly, onto
a plate on it, or — at a joint where pieces in line are turned alike —
onto the leg there), the plane through the leg's axis and the end's axis
is the plane a flange should lie in; the turn of the leg about its axis
that lays its nearer flange in that plane is that end's estimate
(`flat_roll` of the leg against the plane). An end whose plane lies more
than 15° from both flanges (a plan or hip member arriving between them)
says nothing (the program's value). A piece is **flagged** where at least
two members' ends say a turn, all the same way, within 4° of each other
(twice the owner's 2°: two ends each flat within 2° agree), and their
middle is more than 2°; the turn proposed is the middle. The pieces in line
with it turned alike (their axes within 2°) are named with it and turned
alike in the proposal (one leg). The tower's faces through the neighbouring
legs — what Orient angles turns a leg to — are said beside it as a hint,
never flagged alone (the owner: "this doesn't mean much when we can clearly
see on the joint itself").

**Never applied silently**: the run lists them (`LayoutRun::leg_rolls`,
Lay out joints' preview); Solve the clashes works the joint's solution out
again with the leg turned and offers it ticked off, its counts beside it.

## The joints grouped by what comes in and how it lies (L12)

(The owner, 2026-11-21: "grouping the joints based on how they are laid out
and what kind of members come in to better understand them and then try to
see if we would need to apply per group methods or have it working
differently based on the incoming parts to the joints … The goal is to lay
the joints so that it works most of the time." Built 2026-11-21, in active
development, not approved yet. The code: `joint_classes` (the description
and the group of a joint, a pure function), `leg_roll::piece_rolls`
(L11's leg turn read for any host piece), `layout_run` (the chord turns,
`LayoutSettings::chord_turns`, **off by default**); the application's
Joint details shows a joint's group. **No format change**.)

### A joint described

From the joint's details alone (never from names):

- **The host** — what the other ends bolt onto: of the members with a
  profile, the class first (legs, then bracing and crossing diagonals,
  then redundants: the order the layout ranks what an end bolts onto),
  then the largest steel area as drawn, then the larger leg; with the piece
  in line with it within 10° (its run), where there is one.
- **What runs through** along the host: its two pieces one part; two
  pieces of one section not one part; a splice given; the section changing
  without a splice; nothing (the host ends here).
- **Where it lies**: the body, an arm or the top (L1's order).
- **Every other end**: its class; a plan or hip member (level, in no steep
  face of the tower); single angle or built up, its size against the
  host's; its angle to the host; its bolts; and **how it lies against the
  host's two flanges** — how far its axis runs out of each flange's plane
  (an angle's legs, a built-up member's angles along the same two axes):
  within 2° *in* the plane (the owner's 2° rule: it can lie flat there),
  within 16° *near* (a sloping face against a chord's flange: a triangular
  arm's faces lie about 15° off its chords' flanges), else *out* (a plan
  member at a leg, a member between two faces); and how far its leg
  bolted lies off that flange (its roll). The 10°, 16° and the class order
  are the program's values, not a standard's.

### The groups, and how they were chosen

A joint's group has three parts: **what the ends bolt onto** (a leg running
on, a leg splice, a leg ending here, a built-up leg, a chord running on
through a joint of an arm or the top, a member running on through a joint
of the body, a crossing of two members running on, nothing running
through, a built-up member, no host), **what comes in** (redundants only;
bracing; with a plan member; nothing) and **how the incoming ends lie**
(all in a flange's plane; some a few degrees out; some out of plane; none
to measure). Its name reads so: "leg running on · bracing · all in a
flange's plane".

Chosen on the 78 private real towers (kept privately), each joint
measured as imported and after Orient angles, with what the layout did at
each end and why an end was left. The first grouping took the place in the
tower and what runs through, then the incoming members' classes and
counts and the flanges they use. The place explained less than expected:
one kind of host lays out whole from 0 % to 98 % of its joints depending on
how the ends lie. **The plane of the incoming ends against the host's
flanges divides the joints best**, in every place and in both states:

| how the incoming ends lie | joints after Orient angles | laid out whole after Orient angles | as imported: joints | as imported: whole |
|---|---|---|---|---|
| all in a flange's plane | 41 745 | 80 % | 22 033 | 65 % |
| some a few degrees out (2° to 16°) | 6 623 | 13 % | 23 243 | 16 % |
| some out of plane (over 16°) | 10 918 | 5 % | 14 010 | 3 % |
| none to measure | 5 474 | 12 % | 5 474 | 6 % |

So the plane is a part of the group's name; the number of ends and the
flanges used are not (they matter within a group as **room**: of the
chords' joints with a plan member, all in plane, those with two ends are
91 % whole, with four 59 %, with six or more 16 %). 89 groups occur; the
30 largest hold 88 % of the joints:

| group | joints | share | the same joints whole as imported | whole after Orient angles | ends laid out after Orient | of the ends left by hand | main reasons left |
|---|---|---|---|---|---|---|---|
| chord running on (arm, top) · with plan members · all in a flange's plane | 8 333 | 12.9 % | 34 % | 70 % | 84 % | 8.3 % | roll set at its other end, a plate into steel, clashes after writing |
| member running on (body) · redundants · all in a flange's plane | 7 038 | 10.9 % | 62 % | 98 % | 98 % | 0.4 % | clashes after writing |
| chord running on (arm, top) · bracing · all in a flange's plane | 4 407 | 6.8 % | 75 % | 87 % | 93 % | 1.4 % | a neighbour left as it is, clashes |
| built-up member · redundants · all in a flange's plane | 3 656 | 5.6 % | 46 % | 47 % | 57 % | 6.3 % | its leg on the built-up member's other flange, built-up host |
| leg running on · redundants · all in a flange's plane | 3 262 | 5.0 % | 14 % | 98 % | 98 % | 0.3 % | pull-back |
| chord running on (arm, top) · redundants · all in a flange's plane | 2 937 | 4.5 % | 43 % | 97 % | 97 % | 0.2 % | — |
| built-up leg · redundants · all in a flange's plane | 2 736 | 4.2 % | 4 % | 97 % | 98 % | 0.4 % | — |
| leg running on · bracing · all in a flange's plane | 2 641 | 4.1 % | 52 % | 96 % | 98 % | 0.5 % | pull-back |
| member running on (body) · with plan members · all in a flange's plane | 2 594 | 4.0 % | 24 % | 83 % | 90 % | 1.1 % | bolts joining nothing |
| built-up member · redundants · some a few degrees out | 2 192 | 3.4 % | 15 % | 11 % | 29 % | 2.5 % | out of plane, a plate into steel |
| member running on (body) · with plan members · some a few degrees out | 1 281 | 2.0 % | 16 % | 20 % | 50 % | 3.3 % | out of plane, roll set at its other end |
| nothing runs through · bracing · some out of plane | 1 280 | 2.0 % | 0 % | 0 % | 41 % | 9.4 % | roll set at its other end, bolts joining nothing, too short |
| member running on (body) · with plan members · some out of plane | 1 244 | 1.9 % | 0 % | 0 % | 40 % | 5.8 % | out of plane, roll set at its other end |
| crossing · nothing · none to measure | 1 210 | 1.9 % | 23 % | 51 % | 94 % | 0.2 % | — |
| chord running on (arm, top) · with plan members · some out of plane | 1 182 | 1.8 % | 4 % | 1 % | 52 % | 5.5 % | roll set at its other end |
| built-up member · with plan members · some out of plane | 1 163 | 1.8 % | 3 % | 7 % | 25 % | 6.0 % | out of plane |
| leg running on · with plan members · some out of plane | 970 | 1.5 % | 0 % | 0 % | 71 % | 3.6 % | roll set at its other end (the plan and hip members) |
| nothing runs through · with plan members · all in a flange's plane | 948 | 1.5 % | 12 % | 21 % | 65 % | 2.0 % | bolts joining nothing |
| chord running on (arm, top) · with plan members · some a few degrees out | 877 | 1.4 % | 11 % | 11 % | 50 % | 3.4 % | roll set at its other end, out of plane |
| built-up leg · with plan members · some out of plane | 869 | 1.3 % | 0 % | 0 % | 50 % | 5.5 % | roll set at its other end, built-up host |
| nothing runs through · with plan members · some out of plane | 847 | 1.3 % | 8 % | 22 % | 53 % | 3.5 % | roll set at its other end, out of plane |
| chord running on (arm, top) · bracing · some a few degrees out | 747 | 1.2 % | 29 % | 14 % | 45 % | 1.7 % | roll set at its other end, inner face over the heel |
| built-up member · nothing · none to measure | 742 | 1.1 % | 4 % | 2 % | 33 % | 0.7 % | out of plane |
| chord running on (arm, top) · bracing · some out of plane | 712 | 1.1 % | 18 % | 24 % | 53 % | 2.5 % | roll set at its other end, clashes |
| chord running on (arm, top) · nothing · none to measure | 603 | 0.9 % | 0 % | 0 % | 0 % | 0 % | (nothing to lay out) |
| built-up leg · bracing · all in a flange's plane | 560 | 0.9 % | 13 % | 72 % | 83 % | 0.7 % | pull-back, built-up host |
| built-up member · bracing · none to measure | 489 | 0.8 % | 4 % | 4 % | 49 % | 0.3 % | out of plane |
| built-up member · bracing · all in a flange's plane | 424 | 0.7 % | 39 % | 40 % | 38 % | 1.1 % | built-up host |
| leg splice · with plan members · some out of plane | 416 | 0.6 % | 0 % | 0 % | 47 % | 2.5 % | roll set at its other end, a plate into steel, a neighbour left as it is |
| built-up member · bracing · some out of plane | 406 | 0.6 % | 1 % | 0 % | 29 % | 0.2 % | roll set at its other end, out of plane |

(*Roll set at its other end*: its bolted leg lies off the flange and no one
roll of its member lays both its ends flat, and the welded or bent plate in
its own plane does not fit; *out of plane*: its axis runs out of the face
of the flange it would lie on.)

### What leaves an end for a hand detail

Of the 43 909 ends left for a hand detail after Orient angles, by cause
(each end counted once, in this order):

| cause | ends | share |
|---|---|---|
| F. a leg splice (its cover in the way) | 1 122 | 2.6 % |
| D. its bolts join nothing (pieces in line not one part; at a joint where nothing runs through, the base member's own bolts) | 2 985 | 6.8 % |
| A. its axis more than 2° out of the host's flanges' planes | 19 013 | 43.3 % |
| B. its axis in a flange's plane, its leg bolted not flat there (its roll set at its other end, or its leg lying on the host's other flange) | 8 449 | 19.2 % |
| C. a built-up host (no plate on its angle yet: J6's rest) | 4 415 | 10.1 % |
| E. room: a plate into steel, a clash after writing, the pull-back, too short, a neighbour left as it is | 7 806 | 17.8 % |
| other | 119 | 0.3 % |

**Six in ten are geometric (A and B)**, in every place: the end does not
lie in the plane of the flange it would bolt onto, or its member's roll is
set by its other end. Of the ends out of plane by more than 2° (each
counted whatever its cause): from 2° to 5° 41 % are left (of 12 039 ends),
from 5° to 20° 82 % (of 7 355), from 20° to 45° 62 % (of 13 785). The plan and hip members are the
largest single family: with a plan member at the joint, 57 % of the ends
left (with plan members in 35 % of the joints).

### Per group, or general?

What the groups with most ends left need, measured:

- **Plan and hip members** (the groups "· with plan members · some out of
  plane", 30 % of the ends left): a level member arrives between a leg's
  or a chord's flanges, 30° to 45° out of each. Where a face's horizontal
  ends at the joint its outstanding leg is level (Orient angles turns a
  face member's other leg out of the tower), so the plan member could lie
  flat on it, or on a level plate across both horizontals (a plan gusset,
  the owner's picture of a bent plate at a corner where the horizontals'
  flanges are not level). Today a plan member's host is a member running
  through, else the leg — never a member ending there except in the
  run's search of other members for an end left (L1 d). **General**: an
  end's host chosen by the plane of its axis — the flange of a member at
  the joint, running through or not, in whose plane it lies — before the
  biggest member. Not built (it changes what an end bolts onto everywhere;
  measured first).
- **Built-up hosts** (built-up members and legs, 19 % + 9 % of the ends
  left): an end whose axis lies in the plane of one of the built-up
  member's flanges, its leg bolted parallel to the other (its roll from
  its other end): seated on the flange its leg is parallel to, it is "out
  of the face" by up to 90° (1 547 such ends) — the joint details' seat on
  a built-up member is chosen by the end's leg, not by the plane its axis
  lies in; and no plate is yet laid on a built-up member's angle (J6's
  rest). **General**: the seat chosen by the plane of the axis (as for a
  single angle's flange already), then J6's plates.
- **Nothing runs through** (arm tips, K-braces' apexes, peaks; 18 % of the
  ends left, 16 % whole): the base member's own bolts join nothing (it
  bolts onto nothing), so such a joint cannot be whole; 565 joints (456
  where nothing runs through, 109 where a leg ends) are left only for that.
  **Per group**: the base member's end bolted onto the gusset the others
  bolt onto (its bolts the gusset's holding bolts) — L10's question 5, a
  format decision.
- **Chords a few degrees out** (chord or body member running on, "· some
  a few degrees out"): the chord's flanges lie 2° to 5° off the planes its
  joints' ends lie in (Orient angles turns a chord to the faces through its
  neighbouring chords; its joints' ends say otherwise). **Per group**: the
  chord turned to what its joints say — L11's leg turn read for a chord
  (below, a setting, off).
- **Room** (E, 18 %, mostly "chord running on · with plan members · all in
  a flange's plane"): crowding — four or more ends on two flanges; the
  general steps (both faces of a flange, one plate per plane, shifts) are
  where it is won.

**Recommendation: general rules driven by the incoming ends, not methods
per place.** The same three causes (an end out of its host's flange plane,
its roll set at its other end, room) leave ends in every place; what
differs between groups is how often. Two causes are specific to a group
and worth a step of their own: the base member where nothing runs through
(a format decision) and the chords whose joints say they are turned (built
as a setting, off).

### A chord turned against its joints (a setting, off by default)

L11's leg turn ([above](#a-leg-turned-against-its-joints)) read for the
host's pieces running on through a joint that are not legs
(`joint_classes::chord_pieces`; `piece_rolls`): an arm's chord, a body's
horizontal or diagonal running on. With `LayoutSettings::chord_turns` the
run (`layout_run::chord_turned`) works in rounds: the chords flagged on its
result so far — the pieces in line turned alike taken together as one
chord, their flagged pieces' turns all one way within 4°, turned by their
middle — all turned (as Orient angles turns a chord, before the members'
own turns) and the run made again; each chord kept where its joints lay
out more ends, or as many on fewer plates, none fewer at any of them
(planned again with only those until each still helps); the next round
from that result (at most four), until no chord flagged there helps — so
that a run again on the result mostly finds nothing more to turn (not
always, below). The turns kept
are written as the chords' rolls (the file's rolls, not in the layout
record's `rolled`: a later run keeps them) and said in the notes. **Off by
default**: a chord's roll is Orient angles' (as a leg's), the owner
decides. The analysis takes the turned roll as it takes any roll.

Measured on the 78 private real towers (after Orient angles, with the
bundled plugin), L11 → with the setting on: joints laid out whole 35 656 →
35 884 (230 gained, 2 lost), ends 130 097 → 130 943, plates 67 532 →
67 798; joint by joint 858 better (more ends, or as many on fewer plates)
and 60 worse; 1 760 members' rolls differ on 71 towers (the chords turned
and the bracing turned after them). The gains lie in the groups "· some a
few degrees out": member running on (body) · with plan members 258 → 319
whole, chord · with plan members 93 → 151, chord · bracing 107 → 148. The
time: four to five runs of the layout per tower, median 41 → 164 s, the
largest 67 → 327 s (all 78 towers, five at a time). Turning every flagged
chord without weighing gained more joints whole (+633) but made 1 396
joints worse (216 no longer whole); keeping each chord's turn where its
joints were better in that comparison (worked out after the runs, not in
a run) gave +342 whole and 138 worse. A run again on its result (six towers, every 13th): the same text on five; on the sixth the run again turns some chords back (their bracing, turned after them, says another turn — up to 9°): not stable there, **a run again not always identical with the setting on** (with it off, as before: 78/78). The time on those six towers, back to back: median 39 → 161 s, the largest 40 → 245 s.

**L11's leg turns measured** (its question 6, the turns it proposes after
Orient angles, applied before the run without weighing): 114 leg pieces
on 22 towers (mostly leg extensions Orient angles finds no faces for),
joints whole +54 (58 gained, 4 lost), ends +145, plates −28, 101 joints
better and 14 worse — the group "leg running on · redundants · some a few
degrees out" 3 → 60 whole of 70.

## An end's host chosen by the plane its axis lies in (L13)

(The owner, 2026-11-21, on L12's finding: "the goal is to lay the joints so
that it works most of the time"; L12's recommendation, general rules driven
by the incoming ends. Built 2026-11-22, in active development, not approved
yet. The code: `joint_layout::hosts_to_name_by` (`named_hosts`,
`host_ending_in_plane`), `layout_run` (the turns and the as-drawn pass),
`LayoutSettings::plane_hosts`, **on by default**. **No format change**.)

### The rule

An end bolts flat onto a flange only where its axis lies in that flange's
plane (its leg bolted contains its axis). L12 found that an end's axis
lying out of its host's flanges' planes leaves more ends for a hand detail
than any other cause (43 %), and that many of them lie in the plane of a
flange of **another member at the joint** — most often one **ending**
there: a plan or hip member at a leg, whose face horizontal ends at the leg
with its outstanding leg level (Orient angles turns a face member's other
leg out of the tower); two members of one face meeting where nothing runs
through. Today's choice ([above](#which-member-an-end-bolts-onto)) looks
only at the members running through the joint, then the joint's leg.

Now, for an end whose today's choice lies **more than 10°** out of its
flanges' planes (or that has none), and that no member running through
holds in a flange's plane: a single angle **ending** at the joint in a
flange's plane of which the end's axis lies (within the owner's 2°) is
named what it bolts onto, before today's choice, where

- it is not one part with a member there, not running through, not in line
  with the end, and does not bolt onto the end;
- it is of a class at least the end's (leg, bracing, redundant) and as wide
  where of its class, the id deciding between equals — so two ends are never
  named onto each other, and a horizontal is never named onto the plan
  diagonal arriving on it;
- of several, the one the end's leg already lies flat on first (no turn:
  its member's other end keeps its own), then the class, the wider, the id;
- the end is not in line with a member at the joint (the plan leaves such
  an end as it is), and is not itself what another end is named onto by
  this rule (no chains of such names: measured, renaming both lost more
  joints than it gained);
- where nothing runs through the joint (an arm's tip, a K-brace's apex),
  only a plan or hip member (level within 5°) is so named: the base of
  such a joint waits for the owner's decision on its own bolts (L12's
  question 3), and the face members named there made more joints worse
  than better (measured, below).

The 10° (`PLANE_HOST_OFF`) is the program's value, measured (below): an end
within 10° of today's choice's flange keeps it — a bent plate by a few
degrees lays many of those out.

### Weighed, never forced

- **The turn.** The end's member is turned to lie flat on its new host as
  any member (one roll for the whole member, J3). Where no one roll lays
  every end of it flat with the new host, the name is taken back (today's
  choice there): it never costs the member's other ends their turn.
- **Joint by joint, after writing** (the run's as-drawn pass, L8): every
  joint with such a name is planned once more without it — the end where
  the file draws it, its member turned as it would be without the name —
  and written; the joint keeps today's choice where that lays out as many
  ends or more on no more plates (a tie goes to today's choice), else the
  name and its turn are put back.
- A plate on the new host is laid out as on any member's flange (its
  `on`); a level plate across two horizontals (or a bent one where their
  outstanding legs are not level) is **not built**: each end goes onto its
  own host's flange or a plate there.

What an end bolts onto is what its restraint is worked out from (the
code's stiffness rule, as before): the analysis takes the new host as it
takes any member an end is laid out onto.

An end laid out onto a member that **ends** at the joint (this rule's
names, and L1 d's before them) brings its force into that member near its
end; that member's own end bolts (a member not a leg) then carry it too.
Until L14 the joint checks checked them for that member's own force only
(NOT ON THE SAFE SIDE, said: the forces brought in were missing, so
those bolts' usage was too low). **Since 2026-11-22 (L14) the bolt groups
follow the force through** ([joint groups](joint-groups.md#the-force-through-a-members-own-end-bolts-l14)):
that member's end bolts carry its force and those of the ends laid onto it
(each along its line, with their moment about the bolts' centroid, shared
elastically; each bolt the larger of that and its own force alone), in the
group of what it bolts onto, a plate's holding bolts, a weld of a welded
plate and a group of its own end bolts (its bearing). Since the owner's
answer of 2026-11-22 to L14's question 2 also that member's own steel at
those bolts (its net section, its block shear) and, onto a plate, the
plate's own checks under them, under the larger of its own force and the
resultant (before: its own force only, NOT ON THE SAFE SIDE as the steel's
usage was too low; what remains is said there, with its reason).

### Measured (L13's first step, 2026-11-22)

On the 78 private tower files after Orient angles, with the bundled code
plugin found, L12's settings (the pieces in line and the leg splice places
confirmed). "L12" is the main build before:

| | L12 | with the plane's host |
|---|---|---|
| joints laid out whole | 35 656 (55.1 %) | 35 692 (55.1 %) |
| member ends laid out / candidates | 130 097 / 183 476 | 130 444 / 184 468 |
| plates written | 67 532 | 67 894 |
| members left after the clash rounds | 9 485 | 10 127 |
| joints that do not fit, as given / as assumed | 48 / 11 552 | 48 / 11 552 |
| a run again identical | 78 of 78 | 78 of 78 |

Joint by joint: **390 better** (more ends, or as many on fewer plates, or
newly whole) and **41 worse** (40 joints newly whole, 4 no longer). 5 830
ends were named so before the take-backs; of the ends so named, 596 are laid
out that were left before, 15 left that were laid out. The gains lie where
plan and hip members come in: "nothing runs through · with plan members"
(all three planes: 161 joints better, 3 worse, ends 3 962 → 4 138), "chord
running on · with plan members · all in a flange's plane" (whole 5 864 →
5 893), "leg running on · with plan members · some out of plane" (whole 4 →
12), "member running on (body) · with plan members · some out of plane" (37
better, 7 worse); the worse mostly "chord running on · with plan members ·
some out of plane" (8 better, 18 worse: a plan member's other end, turned
for its new host). More ends are candidates (a plan member named onto a
horizontal is planned, where before it was left as it is): 992 more.

**Tried and left out** (each measured on all 78 towers): the ends 5° to 10°
out of today's choice's flange named too (250 joints worse, 137 better: a
bent plate lays many of them out); chains of such names (an end named onto
one itself renamed: 90 joints worse, 16 better where the end was laid out
before); the face members where nothing runs through (123 joints worse, 34
better).

### An end on a built-up member seated by the plane its axis lies in

(L13's second step, the owner's answer to L12's question 2; built
2026-11-22, in active development, not approved yet. The code: the joint
details' leg bolted, `joints::leg_along_y`. **Every file**: the joint
details read it so; no format change.)

An end's **leg bolted** (where its design data does not say the long or
short leg of an unequal angle) is the one lying flat on the member it bolts
onto. Two things are new:

- **Onto a built-up member** whose angles' flanges' planes hold the end's
  axis in one of them (within the owner's 2°): the leg lying in that plane —
  the seat chosen by the plane the axis lies in, as the layout's turn
  already chooses its flange. Before, the leg most nearly parallel to
  either flange was taken.
- **Bolting onto nothing named** (no `onto`, none worked out): the member
  the layout plans it onto (the one running through the joint, as today's
  rule names it) is what its leg is chosen against. Before, its long leg was
  taken whatever it lay on, so wherever the run's name for such an end was
  taken back (the as-drawn pass, L8) the layout measured it against the
  flange its other leg happened to lie parallel to — L12's 1 547 ends on
  built-up members "out of the face" by 60° to 76°: their axis lay in the
  member's other flange's plane all along.

L12 also read the joint details giving a built-up member running on the
redundant arriving on it as what it bolts onto (worked out: the joint's
"leg" is its most nearly upright member where no leg meets). **Traced, not
built**: on the 78 private towers after Orient angles, with the pieces in
line confirmed one part (as the measurements lay out), no built-up member
running on has such an `onto` (of 31 109 such ends; on the traced joint its
two pieces are one part, `onto` its partner). Where the pieces are not yet
confirmed, the rule stands as before.

What this leaves: such an end now lies flat on the right flange — often
on the **inner face** of one of the member's back-to-back angles, where it
runs towards that angle's other leg ("turn it, or lay it out by hand"), or
in the gap between the two angles where the gusset belongs. **Built since
L15** ([below](#a-plate-between-a-built-up-members-angles-l15)): a plate
between a built-up member's angles, in the gap, onto which such ends bolt.

Measured on the same 78 towers, L13's first step → with this one: joints
laid out whole 35 692 → 35 940 (248 newly whole, none lost), member ends
laid out 130 444 → 130 779, plates 67 894 → 67 917; joint by joint **601
better and 301 worse**. The gains: "built-up member · redundants · all in a
flange's plane" whole 1 727 → 1 900 (235 better, 114 worse), "built-up
member · redundants · some a few degrees out" 28 better, none worse. The
worse: mostly where a plan member now lies flat on a built-up member's
flange and goes onto a plate there that runs into steel after writing
("built-up member · with plan members · some out of plane": 68 better, 96
worse) — the members left after the clash rounds 10 127 → 11 875, those
left at a built-up member 2 554 → 3 954: such ends now reach the built-up
member's own limits (J6's rest) instead of being measured out of the face.
The time per tower (six towers, back to back, twice, nothing else
running), L12 → L13 (both steps): median 38.4 → 37.8 s, the largest 40.3 →
40.1 s — the same.

### Leg and chord turns offered, never applied silently

(The owner's answers to L12's questions 4 and 5; built 2026-11-22, in
active development, not approved yet. The code: the application's
`layout_window` (the preview's ticks), `solve` (Solve the clashes); the
run's own chord turns, `LayoutSettings::chord_turns`, stay **off**. **No
format change**: a turn applied is the member's roll, as any.)

- **Lay out joints' preview** lists every leg piece (L11) and every chord
  piece running on (L12, `joint_classes::chord_pieces`) whose joints say it
  is turned ([above](#a-leg-turned-against-its-joints)), read from the
  joints asked as the file is, each with a **tick, off by default**. With
  turns ticked the preview is worked out on the file with them applied
  (each piece and the pieces in line with it turned alike, to a hundredth
  of a degree) and says the run **with and without** them — joints laid
  out whole, member ends laid out, plates (measured: the layout run both
  ways). Apply writes the turns with the layout, one undo step.
- **Solve the clashes** offers a chord piece at the joint as it offers a
  leg piece: the joint's solution worked out again with it turned, ticked
  off.

**Why Orient angles' chord turn and the joints disagree by a few degrees**
(L12's question 4, traced on the 78 private towers after Orient angles):
of the chord pieces flagged whose roll Orient angles set as a chord, **every
one** (839) is flagged by exactly half the difference of its two faces from
square — the turn that puts one flange in one face (Orient angles' rule
puts the angle's legs' bisector on the faces' bisector, each flange half the
difference off its face). The faces meet off square by a few degrees (half
the difference under 2.5° for 422 of them, 2.5° to 5° for 136, 5° to 10°
for 250, more for 31): the ends of **both** faces lie that far off their
flanges; the joints' estimate counts only the ends bolting onto the piece
(directly or onto a plate on it) — at a traced arm chord the ends of one
face, laid out, said −2.4° and −1.8°, while those of the other face, left
by hand, said nothing. Turning the chord lays that face's ends flat and
puts the other face's twice as far off. **Not a fault of Orient angles to
fix at no cost**: a tried rule (a flange in the face that alone has members
ending on the chord) changed 2 of the 839 pieces — the faces carry members
on both sides. The ends of such faces are for the bent plate (a few
degrees), or the chord's turn weighed per chord (the ticks above, or the
run's setting).

## The base member's end bolts as the gusset's holding bolts (L14)

(The owner's answer of 2026-11-22 to L12's question 3, which also settles
L10's question 5; built 2026-11-22, in active development, not approved
yet. `LayoutSettings::base_holds`, on; the plate's `held_by`, **structure
file version 24**.)

Where nothing runs through a joint (an arm tip, a K-brace's apex, the
pieces of a horizontal in line not one part) the layout lays the other
ends onto a plate on the **base member's** flange, and that member's own
end bolts (given at the end, or its design data's) were drawn **joining
nothing** — the joint not whole, the member by hand ([L10](#bolts-drawn-that-join-nothing-the-owners-picture);
565 joints on the private towers were left for this alone, L12). Now:

- **The rule.** A member left by hand only because its bolts join
  nothing, with a flat plate of the plan lying on its own flange (not one
  in its members' own plane, not bent from one, its outline not laid out),
  bolted by one leg: **its end is bolted onto that plate**, its bolts going
  onto the flange the plate lies on (the joint details' leg lying flat
  against what it bolts onto — unless its leg bolted is given as the other
  one), and the plate is written **held by it** (`held_by`). **Not where an
  end left as it is bolts onto that member directly**: its bolts would
  share the flange with that end's (measured on the 78 private towers
  without this condition: 19 more joints that do not fit as given — holes
  overlapping, bolts off the flange as the joint details read it — for as
  many joints whole; with it, 2: one tower's pair of joints). Where the plan
  gave the plate provisional bolts of its own onto that member (L5o, placed
  clear of what is there), **those** become the member's end bolts (one
  gauge line at an even pitch, the first at least one and a half diameters
  from where its steel stops; written as its bolts, gauge, pitch and end
  distance, provisional); else its own bolts as drawn. A plate whose own
  bolts cannot be so written keeps them, the member by hand as before.
- **Applied as a plan is written** — after it has been weighed against the
  others (L8's pass, L13's names, the tries), so the end it gains never
  decides between plans (measured: applied while planning, it chose plans
  with more plates for that end, 16 joints worse on six towers; applied at
  writing, none worse) — at every joint whose plan leaves such a member,
  also one planned again in the clash rounds.
- **The clash check** after writing tests that end like any other: its bolts'
  heads and nuts, its steel. Where they run into something it is put back
  as it was, by hand, said with what it runs into (its bolts as drawn, near
  the work point, often meet another member's steel there).
- **The joint checks**: the plate's holding bolts are that member's bolts on
  it, checked as a leg's ([joint groups](joint-groups.md#a-plates-holding-bolts)):
  the resultant of the other members' forces on the plate with its moment,
  each bolt's shear, its bearing on the plate, the member's bearing under
  them. The forces of ends laid onto that member are not followed through
  its bolts (it holds a plate; L14's first step leaves such a member out).
- **Drawn**: the member stays where it is (the plate lies on its flange, so
  it is not moved by the plate's thickness).

Measured on the 78 private tower files (L13 main → with it; the bundled
plugin, after Orient angles, L13's settings): bolts joining nothing 3 303 →
2 833 ends, 121 plates written held by their base member, joints laid out
whole 35 940 → 35 964 (24 newly whole, none lost), member ends laid out
130 779 → 130 902, plates 67 917 → 67 928, joint by joint 108 better and 11
worse, joints that do not fit as given 48 → 50, a run again identical 78 of
78; the members left after the clash rounds 11 875 → 12 260 (such ends put
back for a clash — their bolts as drawn, near the work point, run into
another member's steel — left by hand as before). Most such members lie at
joints with other ends left by hand, so few joints become whole for this
alone.

## A plate between a built-up member's angles (L15)

(L14's item 3, J6's rest; the owner busy: built on its own, reported. Built
2026-11-22, in active development, not approved yet.
`LayoutSettings::between_plates`, on; the plate's `between`, **structure
file version 25**.)

Since L13 an end on a built-up member lies with its leg in the plane of the
angles' flange its axis lies in. Where that plane is the backs of two angles
back to back (or heel to heel, in a star or a cross) and the end's
outstanding leg points out of one back, across the gap, the end belongs on
a plate **between the angles**, where the packing lies — the classical
detail. Until now the layout laid such an end on the back of an angle with
a plate as thick as its leg, which ran into the other angle across the gap
(on the private real towers 342 ends so, and 15 more for a plate in the
end's own plane).

- **Which ends** (`joints::between_corner`): onto a built-up member whose
  angles are known; its leg bolted lying flat (within the owner's 2°) on
  the back of one of its angles — the back towards another angle of the
  member across a gap, that angle's steel wholly beyond the gap — and its
  outstanding leg pointing out of that back, across the gap (it then lies
  on the plate's far face, the plane of the other angle's back). Of several
  such angles (four in a cross) the one the end does not leave away from,
  then the one moving it least.
- **The plate**: on the back of the one angle, **as thick as the gap** —
  the section's own (on imported sections fitted to its properties, and
  then often thinner than the legs on the plate); laid out only where the
  gap is at least **half the thickest leg on it** (`BETWEEN_LEAST`, *the
  program's value, not a code's*; else the ends by hand, said). The joint
  checks check the plate as thick as it is: the bearing under every bolt,
  its edges, its own sections. A plane of its own, never shared with ends
  lying on that back otherwise. Never bolted directly in the gap.
- **Its ends clear of the other angle**: the end lies in the plane of the
  other angle's back, so the plan now sees the member's own steel across
  the gap as in its way (before, the member an end bolts onto was never
  among what is in its way) and pulls the end back past it — beyond the
  backs' toes, or past the heels where the end leaves that way (the gap is
  open there between two angles back to back). Its bolts pass the angle
  the plate lies on by their nuts and the gap, as on any plate.
- **Its own bolts** (L5o) onto the member, through the plate and the angle
  on each of its faces where one lies there: two back to back — through
  both angles, **two shear planes** in the joint checks (unless the file
  gives them); in a star the other angle lies beside, not across: one
  plane. They lie on the angle's back within its flange by the rules of
  every holding bolt; the plate may **grow along the member** in the gap
  round them (nothing else lies there), its outline grown round them —
  each bolt with its distances in it — checked clear; else none, said (its
  holding *not checked*, as before).
- **Weighed, never forced**: every joint where an end is so planned is
  planned without too; the plan without is taken where it lays out more
  ends, or as many on fewer plates. **On a tie the plate between is
  kept** — it lies in the member's own plane (concentric with it), its
  bolts through both angles, where the other lies on an angle's outer face
  (an approved order changed, measured below). After writing, the run's
  guard plans a joint again without them where it was laid out with fewer
  ends than its plan without them (`PlanCtx::no_between`, as L11's guard),
  before the guard that plans it as before L1.
- **Drawn** (every file): a plate `between` on a built-up member is seated
  on the back of the angle its members look across (the same rule), in the
  plane of that back — its members within 2° of it lie flat on it, as on a
  single angle's flange (L10) —, its members on its far face. A plate said
  `between` on a member that is no built-up member of angles is drawn as any
  plate on it, said.
- **The clash check** tests a plate between the angles against the member
  it lies between (before, a plate and the member it lies on were never
  tested): it must lie in the gap, touching both angles only.
- **On the safe side, as for direct bolting onto a built-up member**: the
  ends' forces reach the member through the plate and its holding bolts
  (checked); the member's angles take them as its design assumes (the
  built-up section whole, as before). The member's bearing under the
  holding bolts is checked on one angle's thickness with the whole force
  (two angles back to back share it: on the safe side).

**Found along the way**: an end naming nothing it bolts onto, on a built-up
member, was planned on the *outer* face of the angle the joint details seat
it on by that angle's *inner* face (the plan's face came from what the file
says, the angle from the seat): its plate planned in the gap, drawn outside
it. Every such end on two angles back to back lies across their gap and is
now planned onto the plate between (or left, said, where the gap is too
thin); the plan without — weighed as above — kept the old face until L16
([below](#the-newest-step-never-leaving-a-joint-worse-the-plans-face-as-drawn-l16)),
which plans every end on the face the joint details seat it on.

**Measured** on the 78 private tower files (L14 main → with it; the bundled
plugin, after Orient angles, L14's settings): joints laid out whole 35 964 →
37 059 (+1 095), member ends laid out 130 902 → 133 015, plates 67 928 →
69 226, the members left after the clash rounds 12 260 → 11 670, joints that
do not fit as given 50 → 50 (as assumed 11 524 → 11 536), a run again
identical 78 of 78; joint by joint 1 490 better and **218 worse** (109 with
fewer ends — mostly where the plan with the plate between was taken back
after writing and the joint then planned as before L1, its plan as L14 made
it not reached again; 109 as many ends on more plates — a plate between and
one more for an end laid onto its member). 3 327 plates between the angles
written, 2 557 with their own bolts through the angles; the ends left with
a plate "into the member's other angle" 342 → 119. The joint checks on the
72 towers with loads (one vertical case): the rows of the plates between,
the members' bolts on them median usage 0.06, 412 of 8 173 over 1 (5 %;
the plates of the run as a whole 6.9 % — mostly the plate's buckling as a
strut where its members are pulled back far), their holding bolts median
0.07, 13 over 1.

## The newest step never leaving a joint worse; the plan's face as drawn (L16)

(L15's questions 3 and 4 and its item 4; the owner busy: built on its own,
reported. Built 2026-11-22, in active development, not approved yet. No
format change.)

- **A base member holds no plate where an end left with no connection
  bolts onto it** (L15's item 4). L14 makes the base member's own end bolts
  the holding bolts of the plate on its flange, but not where an end left
  as it is bolts onto that member directly. An end the run named onto
  another member only while planning (L8) is written without the name; the
  joint details then work out what it bolts onto as the joint's leg — and
  where that is the base member, its bolts were read on the base member's
  flange (on the private towers one tower's pair of joints: "does not fit
  as given"). The check now counts every end also as the file written
  gives it: what it bolts onto there, else the joint's leg worked out for
  it (`Arrival::joint_leg`).
- **The turns weighed as before L15.** Most joints L15 left worse (184 of
  218 on the private towers) had a member whose roll the run chose
  otherwise: the half turns (an end on the inner face, L10's other faces)
  are weighed on whole plans, and the plates between laid some ends out
  without them, so a turn another joint needed was not made. The turns are
  now weighed on plans without the plates between — the newest step
  never changes a member's roll —, the plans then made with them.
- **The run's fallback** (L15's question 3). A joint whose plan with the
  plates between lays out, after writing, fewer ends — **or as many on more
  plates** (the clash rounds' plates, before not counted) — than its plan
  without them is planned again without them; that plan is now planned as
  L14 planned it: the ends it leaves are searched for other members to
  bolt onto (L1 d) too — before, an end the plate between laid out was
  never searched —, the names written only when the joint falls back. Where
  the fallback then lays out less than the plan with the plates between
  did, that plan is taken again and kept: the best plan already known, no
  third try. The guard's measures (the ends of the plan without the plates
  between, without L11's steps, as before L1) are worked out again each
  time a joint is planned (an earlier pass's left out), and those of the
  plans weighed for L10's turns are kept (the plates between's were lost
  there).
- **The plan's face as drawn** (L15's question 4,
  `LayoutSettings::seat_as_drawn`, on): an end is planned on the face of
  the leg of what the layout lays it onto that the joint details then seat
  it on — one rule for both (`joints::seat_face`, the details' seat: the
  face given at the end, else the one turned towards its outer face; on a
  built-up member the angle facing it, chosen with the face given). Before,
  the plan took the face the end sat on what it bolted onto while planning
  — for an end naming nothing the joint's leg worked out, and where it did
  not lie flat on that leg no face at all (taken as the outer one): planned
  on the outer face, drawn on the inner (on two private towers about 120
  such ends each, every one where no face had been worked out).

**Measured** on the 78 private tower files (the bundled plugin, after
Orient angles, L14's settings; L15 main → items 1 and 2 → with the face as
drawn): joints laid out whole 37 059 → 37 031 → 37 100, member ends laid
out 133 015 → 133 060 → 133 075, plates 69 226 → 69 197 → 69 391, members
left after the clash rounds 11 670 → 11 744 → 11 366, joints that do not
fit as given 50 → 48 → 48, a run again identical 78 of 78 each. **Joint
by joint against L14** (before the plates between): L15 1 490 better and
218 worse; with items 1 and 2 1 517 better and **81 worse** (whole gained
1 070, lost 3) — of the 81, 41 still have a member turned otherwise (a
turn decided after writing, a member none of whose joints is laid out with
it), 42 lie where nothing runs through the joint. Against L15: 185 better,
43 worse (30 joints whole there no longer: the half turns L15 chose with
the plates between). **The face as drawn** alone (items 1 and 2 → with
it): 227 better and 221 worse (whole +69, ends +15, plates +194): the ends
an inner face cannot take (it runs towards the angle's other leg) are now
left, said, where before they were planned outside and drawn inside (left
so 1 002 → 1 645), and the plans' plates into steel 4 580 → 3 997; worse
mostly on built-up members (their angles' inner faces). The time per tower
(six towers, two at a time, back to back, twice): median 40.8 and 39.8 s →
41.7 and 41.1 s, the largest 41.9 and 40.7 s → 42.2 and 41.8 s.

## One plate in one gap; a turn tried as before the plates between (L17)

(L16's questions 2 and 3; the owner busy: built on its own, reported.
Built 2026-11-23, in active development, not approved yet. No format
change.)

- **One plate between in one gap.** L15 gave each end across a gap
  between a built-up member's angles a plane of its own, keyed by the back
  it lies on. Two ends across one gap from either side — mirror-image
  redundants at a built-up member's middle joint, say — or on the two
  pieces of a member one part through the joint then each had a plate in
  the one gap; the clash check after writing found the two plates in each
  other (as deep as the gap), the joint was taken back and fell back to
  plans without the plates between, often laying out less than before
  L15. The ends across one gap now share its plate: their plane is the
  gap's own (its normal turned one way, the gap's first back along it),
  each end on its own face of the plate — the joint details seat each on
  the face its outstanding leg looks out of, as before, and the plate in
  the gap whichever of them it is drawn from. The planner keeps the ends
  of the two faces apart in the plate's plane as it keeps those of one face
  apart (on the safe side: on different faces they could overlap there);
  the clash check after writing judges the plate and both ends as drawn.
- **A turn refused after writing, tried as before the plates between.**
  The turns are weighed on plans without the plates between (L16); a
  turned member that, once written, is laid out at none of its joints is
  not turned (its roll as before). Where one of its joints was planned
  onto a plate between, that joint is now first planned without them (the
  run's guard, as for a joint laid out worse with them; planned with them
  again where that lays out less there) and the turn kept if it then
  helps; refused only if it still helps none (each member tried once).
- **Tried, not kept**: an end on the inner face of a built-up member's
  angle running towards the angle's other leg (no bent plate round a
  built-up member's leg is laid out) turned half a turn onto a plate
  between, weighed with the plates between — measured on 70 of the private
  towers: 10 joints better, 17 worse, 4 no longer whole.

**Measured** on the 78 private tower files (the bundled plugin, after
Orient angles, L14's settings; L16 main → L17): joints laid out whole
37 100 → 37 315, member ends laid out 133 075 → 133 265, plates 69 391 →
69 295, members left after the clash rounds 11 366 → 11 179, joints that do
not fit as given 48 → 48 (as assumed 11 538 → 11 537), a run again
identical 78 of 78. Joint by joint 273 better and 47 worse (whole gained
217, lost 2): the gains on built-up members (a built-up member with
bracing on it, all in a flange's plane: whole 311 → 406 of 424; with
redundants 2 766 → 2 880 of 3 656); of the worse, 26 have a member whose
one roll now lays it out at a built-up joint on the shared plate (that
joint whole) and no longer onto a bent plate at its other joint, 21 lay
out as many ends on two plates where one was. Against L14 (before the
plates between): 1 761 better and 321 worse (L16: 1 744 and 301) — most of
those where L16 made plan and drawing agree. The time per tower (six
towers, two at a time, back to back, twice) unchanged within the scatter:
median 90.7 and 78.7 s → 77.9 and 83.2 s, the largest 104.2 and 84.8 s →
84.3 and 86.4 s (the computer busier than for L16's measurement).

## Lay out by planes first: rolls from the joints' planes (L18)

(The owner, 2026-11-22: "the axes of the incoming members into a joint
define planes together and we should have the parts rotated accordingly
and then shifted and either bolted to each other when possible and if not
with a new plate, and if there are multiple planes … then we should go
from high to low"; approved the same day with the decisions in the
[decision log](../development/decisions.md#2026-11-23-joint-layout-lay-out-by-planes-first-rolls-from-the-joints-planes-l18).
Built 2026-11-23, in active development, not approved yet. No format
change.)

**Why.** An imported tower carries no member orientation. Orient angles
turned each leg and chord from the tower's outline alone: its heel out,
the middle of its legs on the middle of the two faces meeting at it — so
where the faces meet off square each flange lay off its face by half the
difference, and no flange in the plane of what bolts onto it. The layout
then lays the bracing flat on those flanges (one roll per member, J3) but
never turns a leg or chord. Measured on the private towers (L12): the
joints whose incoming ends all lie within 2° of the host's flange plane
are laid out whole four times as often as those with ends a few degrees
out.

**The planes of a joint** (`ohl_structure_link::joint_planes`). The
joint's host is what the other ends bolt onto (the joint groups' choice:
legs first, then the largest steel area, with its piece in line). Each
other end's axis and the host's axis span a plane through the host's axis;
seen along the host it is a line at an angle ψ from the host's y axis
towards its z axis (0° to 180°). The ends whose ψ lie within 2° of each
other are one plane (the tool's value — the owner's flatness — not a
standard's). For each plane: the host's flange nearer to it (an angle's
flange along y lies in ψ = 0°, along z in ψ = 90°), the turn of the host
about its own axis laying that flange in it, and for each end the angle
its axis runs out of that flange's plane, asin(sin α · |sin(ψ − flange)|)
with α its angle to the host — what the owner's 2° measures. The planes
are listed **high to low**: the biggest member's first (its steel area),
then the larger force where sizes tie and forces are known (the structure
file stores no analysis forces, so the layout orders by size alone —
said), then more ends. An end within 10° of the host's axis (in line)
spans no plane.

**Rolls from the planes** (`ohl_structure_link::plane_rolls`, Orient
angles' default since L18; the outline alone stays selectable for
comparison). An angle has **one roll along its whole length**, so a turn
laying a flange in one joint's plane must suit every joint the member
reaches; the pieces of a leg or chord in line through its joints (within
10°, turned alike within 2°) are **one run** turned alike. The turn δ of a
run, added to every piece's roll, is solved over every end at every joint
of the run:

- where a piece of the run is the host, each other end against the host's
  piece its axis lies nearer to (the host and its piece in line may belong
  to two runs: at a taper change past the in-line tolerance, the ends of
  the panel above belong to the piece above); another leg's piece
  arriving (a taper change, an extension) is spliced, not bolted flat by a
  flange, and gives no plane;
- where a piece bolts onto another member (an arm's chord onto a leg), its
  own leg against that flange (the joint details' flat turn, and how far it
  stays off however turned).

The δ chosen lays **the most ends within 2°** of their flange's plane;
**centred** in the turns that keep those ends within (the middle of the
overlap of their 2° bands: each as far inside as the others allow — a
later small change of the roll, such as pieces confirmed one part with
their rolls averaged, keeps them in); of several such, the widest overlap,
then the least sum of squares of all the ends' angles (each counted at most
16°: a few degrees' turn does not bring a member lying far out in), then the
smallest δ. δ stays within 22.5° of the outline's turn (each flange then at
most 22.5° from its face, as Orient angles' corner: the heel stays out of
the tower); a δ below 0.05° is none, so a run again changes nothing. The
runs are solved in the owner's order — legs, then chords, then the face
members (bracing lying in a steep face) — each set on the joints as the
sets before left them. A leg the outline leaves (no faces found, a leg
extension) is turned from the roll it has, unless that was given by hand
and is not replaced. Orient angles lists each run's turn, its ends within
2° before and after and every end's angle before and after at each joint.

**Tried and not kept** (six private towers, every thirteenth, measured
against the outline's turn): the δ at the edge of the band rather than
centred — the ends at 2.0° fell out again when the one-part confirmation
averaged a chain's rolls (joints whole +24, plates +154, 306 joints worse
against 139 better); a member's own ends weighing a thousand hosted ends
(joints whole +42 instead of +60); each end weighed by its steel area
(+39 instead of +60); legs and chords alone, without the face members (+31
instead of +60).

**Automatic or proposed.** The owner's decision: on an imported tower the
rolls from the planes may be set automatically, on a tower designed in
OpenOHL they stay proposals. What the file records decides: a roll set by
Orient angles (the layout record's `oriented`) or by the layout (`rolled`),
or no roll at all, is the program's; a roll, reference or orientation
without such a mark is the designer's. Orient angles is run on an import
(it carries no orientation) and keeps rolls given by hand unless asked to
replace them; what it sets is marked automatic and undone like any edit.

**Offered side by side** (Lay out joints, Solve the clashes). The leg's
and chord's runs at the joints asked that their joints' planes would turn
(from the file as it is, on the whole tower: a run reaches past the joints
asked) are listed beside the leg-rotation check's proposals (L11, the
piece and the pieces in line with it), each with its own tick: ticked by
default where the run's roll is the program's, not where given by hand;
one turn per member. A plane turn of a run whose roll is the program's is
ticked by default only where every joint it reaches is laid out better
with it (the run's guard, joint by joint: at none fewer ends nor as many on
more plates; in all more ends, or as many on fewer plates — the measured
column *weighed* below). **A turn is never judged on one joint** (the owner:
"when turning a member both sides should be reverified"): every joint the
turned members reach (both ends of each piece; every joint of the run) is
laid out with the turn and its effect counted there — in Lay out joints the
whole, ends and plates at those joints without and with all the proposals
of its kind applied together; in Solve the clashes the joint's solution
worked out with every joint reached laid out, refused where those joints
are left with more clashes and problems.

**Plane by plane, high to low (step 3).** The plan already lays a joint
out plane by plane — the ends on one flange's face one plane, bolted
directly where every bolt lands on the flange, else onto one plate on that
flange shared by the plane's ends, moved sideways within the shift limit
only where the pull-back does not suffice (J1, L1) — with the planes taken
by the rank of what they bolt onto, then its id. Taking a rank's planes
from the biggest member down (class, then the wider leg) measured worse on
the six towers (joints whole −4 after the outline's rolls, −15 after the
planes', more plates): a setting (`planes_high_to_low`), off. Not built:
the special passes rewritten as one general procedure, and the forces in
the order or in the choice between bolting directly and a plate (the
structure file stores no analysis forces).

**Measured** on the 78 private tower files (the bundled plugin, L14's
settings, the layout without the bends' checks; each column after its
Orient angles): *L17* the outline's turn (reproduces L17 main exactly);
*planes* Orient angles v2 automatic; *L11* the outline's turn and every leg
and chord turn the leg-rotation check flags applied (its proposals); *both*
v2, then L11's proposals on the result; *weighed* v2 with each run's turn
kept only where every joint its members reach is laid out better (the
run's guard; 2 844 of 20 836 runs turned kept):

| | L17 | planes | L11 | both | weighed |
|---|---|---|---|---|---|
| member ends within 2° of their host's flange plane (before the layout) | 79.2 % | 81.0 % | 80.8 % | 81.5 % | 80.0 % |
| joints laid out whole | 37 315 | 38 042 | 38 046 | 38 317 | 37 882 |
| member ends laid out | 133 265 | 135 259 | 134 466 | 135 912 | 135 444 |
| plates written | 69 295 | 69 840 | 68 392 | 69 172 | 69 927 |
| members left after the clash rounds | 11 179 | 11 670 | 11 268 | 11 919 | 11 273 |
| not fitting as given | 48 | 46 | 48 | 46 | 48 |
| joints better / worse than L17 | — | 3 461 / 2 400 | 2 776 / 1 414 | 4 450 / 2 593 | 1 935 / 293 |
| whole gained / lost | — | 1 128 / 401 | 948 / 217 | 1 434 / 432 | 613 / 46 |

Of the planes' 2 400 worse joints 2 365 have a member turned otherwise
(the turn helps one joint of a run and costs another — what the weighed
column takes back). The gains are where the ends lie a few degrees out of
the host's flanges: a body's member running on with plan members, some a
few degrees out, whole 255 → 692 (planes), 713 (L11), 739 (both), 400
(weighed) of 1 281; an arm's chord with bracing a few degrees out 107 →
300, 245, 385, 234 of 747. Found: a built-up leg with bracing, all in a
flange's plane, loses whole joints with every unweighed turn (413 → 338,
333, 334 of 560; weighed 411) — the bracing's one roll turned for its other
end. The per-joint bound: turning each joint's host on its own as far as
22.5°, the ends within 2° at angle hosts of one tower rise from 82.5 % to
87 % only — most ends out of plane are so by the geometry (plan and hip
members between the flanges, members of another face), not by a roll.
The time per tower (six towers, two at a time, back to back, twice;
other work on the computer, load 4 to 10): Orient angles median 0.25 and
0.31 s → 2.0 and 2.0 s (the largest 0.37 and 0.40 → 2.3 and 2.4 s: the
joints in detail once per set of runs); the layout after it median 71.3
and 80.0 s → 79.2 and 81.8 s, the largest 80.2 and 81.8 → 84.8 and 88.5 s
(more ends laid out). A run again identical: 16 of 16 towers run again (every sixth and the first four; not all 78, for the time).

## Turns weighed over every joint they reach (L19)

(The owner, 2026-11-22: "when turning a member both sides should be
reverified"; roadmap O44 L19, the owner busy: built on its own, reported.
Built 2026-11-23, in active development, not approved yet. The layout
record gains a list, `plane_turns`; no version change.)

**Why.** Orient angles v2 (L18) turns each leg's, chord's and face
member's run from its joints' planes without laying anything out: the
turn lays the most ends within 2° of a flange's plane over all the run's
joints — but an end within 2° is not yet an end laid out, and an angle has
one roll along its whole length. On the private towers the turns applied
unweighed left 2 400 joints worse than the outline's turn, 2 365 of them
with a member turned otherwise: the turn helped one joint of its run and
cost another.

**Listed, not weighed yet.** Orient angles lists each run it turns from
the planes in the layout record (`plane_turns`: each member, its roll
before the turn and with it as written, the run's kind and its first
member). A row counts only while the member's roll is still the one
written with the turn: a roll changed since is the user's and never taken
back. Pieces confirmed one part (J9) get one roll, half-way between
theirs; where every piece of the chain is listed, the rolls before are
averaged alike, so that taking the turns back leaves them one roll (as
confirming them without the turns would); runs whose pieces are written
one part are weighed as one, and a run one part with a piece not listed
is left as written (taking it back would part what is one part). Orient
angles from the outline alone takes the rows of what it sets.

**Weighed by the layout run** (`ohl_structure_link::weighed_turns`,
`LayoutSettings::weigh_turns`, on by default) — a turn only by a run
that lays out every joint it reaches (a run on a few joints leaves the
others listed for a whole run):

1. every turn taken back (each member at its roll before) and the tower
   laid out: the base;
2. stage by stage in the owner's order — the legs' and chords' runs, then
   the face members' — the stage's turns applied on top of those kept so
   far and the tower laid out again. A turn is kept only where **every
   joint its members reach** (both ends of every piece; every joint of its
   run) is laid out **no worse** than before the stage — no fewer member
   ends laid out, nor as many on more plates, nor no longer whole — **and
   one better** (more ends, as many on fewer plates, or whole where it was
   not): the run's guard with a joint's wholeness added, the owner's rule;
3. **past the turned members**: a joint anywhere in the tower laid out
   worse than before the stage, where a member meets whose roll the layout
   wrote otherwise (a bracing member's one roll, chosen by the layout for
   all its ends — J3 —, turned for its end at the turned member), refuses
   the turns reaching that member's joints (and a turn whose own member it
   is) — the owner's "both sides" one member further;
4. the turns kept laid out again without those refused and weighed again
   there (a turn may have helped only beside one refused), until every turn
   kept still helps — at most four layouts more per stage; none of the
   stage kept where they do not agree by then;
5. a turn refused: its members back to the roll before, said in the run's
   notes ("… taken back (joint(s) … laid out worse with it …); offered in
   Lay out joints"), and offered by Lay out joints beside the leg-rotation
   check's proposals (unticked: it lays no joint out better).

Since every stage keeps no joint worse than before it, the result keeps
no joint worse than the base — except where a joint changes with neither
a turn reaching it nor a member turned otherwise meeting it (the layout's
order: joints planned in turn see what is laid out before them); those
are counted in the run's notes and below. The rows go with the run: the
file written lists none of the turns weighed, so a run again lays out the
same. Lay out joints weighs them once, on the whole tower, when its
preview is first worked out, and lays the rest (the proposals' effects)
out from the file so settled.

**Not on the safe side?** The weighing chooses between two rolls of the
same members; neither changes a member's size, work point or what is
checked: nothing here is less safe than the layout without it.

**Measured** on the 78 private tower files (the bundled plugin, L14's
settings, the layout without the bends' checks; each column after its
Orient angles): *L17* the outline's turn and *L18* the planes' turns
unweighed (reproducing L17 main and L18 main exactly), *weighed* the
planes' turns weighed by the run, *base* the weighing's base laid out
(every turn taken back):

| | L17 | L18 | base | weighed |
|---|---|---|---|---|
| joints laid out whole | 37 315 | 38 042 | 37 434 | 37 796 |
| member ends laid out | 133 265 | 135 259 | 133 610 | 135 194 |
| plates written | 69 295 | 69 840 | 69 211 | 69 763 |
| members left after the clash rounds | 11 179 | 11 670 | 11 177 | 11 179 |
| not fitting as given | 48 | 46 | 48 | 46 |
| joints better / worse than L17 | — | 3 461 / 2 400 | 225 / 80 | 1 679 / 122 |
| … than the base | | | — | 1 456 / 44 |
| … than L18 | | | | 2 391 / 1 946 |

Weighed keeps 66 % of the joints whole the unweighed turns gained (+481
of +727) and 97 % of their member ends (+1 929 of +1 994), with no more
members left after the clash rounds than L17 and 122 joints worse than
L17 instead of 2 400: 44 of them worse than the base (where no turn
reaches and no member turned otherwise meets them — the layout's order),
the rest where the base itself differs from L17 (pieces confirmed one part
after the turns, their rolls averaged then; runs one part with a piece not
listed, left as written). L18's offline weighing (each run's turn kept
where every joint it reaches lay out better, judged on the unweighed
layout and not laid out again) had +567 whole and 293 worse. Kept: 18 of
156 legs' runs turned, 462 of 1 938 chords', 684 of 18 519 face
members' — most turns lay no joint out better. By group: a body's member
running on with plan members, a few degrees out, whole 255 → 692 (L18) →
406 (weighed) of 1 281; an arm's chord with bracing a few degrees out
107 → 300 → 200 of 747; and the unweighed turns' losses gone — a built-up
leg with bracing all in a flange's plane 413 → 338 → 411 of 560, an arm's
chord with plan members all in plane 6 019 → 5 956 → 6 013 of 8 259.

*Both weighed* — the leg-rotation check's turns (L11) applied on top and
weighed as a stage between the legs' and chords' and the face members' —
was measured on every third tower (26; its time):

| every third tower (26) | L17 | L18 | weighed | both weighed |
|---|---|---|---|---|
| joints laid out whole | 12 161 | 12 382 | 12 304 | 12 342 |
| member ends laid out | 42 967 | 43 587 | 43 572 | 43 797 |
| plates written | 22 232 | 22 409 | 22 368 | 22 314 |
| members left after the clash rounds | 3 223 | 3 423 | 3 226 | 3 262 |
| joints better / worse than L17 | — | 1 083 / 768 | 539 / 43 | 828 / 90 |
| … than L18 | | | | 929 / 503 |
| … than weighed | | | | 317 / 48 |

Both weighed lays out the most member ends on fewer plates than L18
(+830 against +620 ends, +82 against +177 plates), 82 % of L18's joints
whole, 90 joints worse than L17 (L18: 768). A run again on the weighed
result: identical on 8 of 8 towers (every sixth; the rows go with the
first run, so a run again weighs nothing).

The time per tower (six towers, every thirteenth, two at a time, back to
back, twice; other work on the computer, load 2.6 to 7.6): the layout run
after Orient angles with the turns unweighed (L18) median 45.0 and 70.9 s
(the largest 53.8 and 77.5 s); weighed 265.7 and 335.3 s (320.7 and
379.0 s) — about five and a half layouts (the base, each stage's turns,
the stages' turns kept laid out again; 2 on the smallest tower, where no
turn helped); both weighed 426.7 and 433.9 s (599.9 and 605.2 s). Paid
once per imported tower: the file written lists nothing more to weigh.

**Plates with several bends, welded assemblies of several plates** (the
owner, 2026-11-23: asked about them, "go ahead to improve"; measured, the
planner not built yet). What the layout leaves that they could take, from
the reasons it gives (the weighed column, 78 private towers; an upper
bound — every such end taken as laid out): 6 780 member ends whose plate
welded in their own plane would have their outstanding leg stand on the
flat plate — it points into the flange, so a flap in the member's plane
does not free it (half a turn of the member, which its other end refuses,
or a stand-off holding the flap clear would); 5 674 whose axis that plate
would put farther out of the flange than one plate's thickness (the rule
keeping the line of action within what the analysis takes; a joggle, two
bends, could bring the axis back only where the leg lies at most 15° off
the flange — 1 806 of them); 7 266 on a
flange's inner face running towards the angle's other leg (a plate
wrapping the leg). Were all of these laid out, up to 5 130 joints more
would be whole. The format already carries both: a chain of bends (a flat
bent from a bent flat) is one part of its flats, each bend on the flat
naming it, drawn flat by flat, weighed from the developed area and
checked bend by bend (tested on an invented joint); a welded assembly is
every plate joined by `welded_to`, any number of them.

## A level plate across two horizontals (L20)

(L19's question 6 and the owner's "go ahead to improve" of 2026-11-23;
roadmap O44 L20, the owner busy: built on its own, reported. Built
2026-11-24, in active development, not approved yet. Structure file
version 26: a plate's `[plate.also_bolts]`.)

**Why.** A plan or hip member at a tower's corner, or where a plan member
meets two horizontals, lies level; the leg or the member running through
the joint stands steep, so its leg bolted lies on none of their flanges
(no roll lays it flat at both its ends), and L13 names a member ending at
the joint only where one horizontal of a class at least its own carries
it. L19 counted 6 793 plan and hip ends left so on the 78 private towers,
5 092 of them where two other horizontals (or one and a leg) meet. Usual
practice joins such ends on a horizontal plate lying on the level legs of
the horizontals and bolted to them.

**Which ends, which plate** (`joint_layout::level_pair`,
`LayoutSettings::level_plates`, on by default). For a level end (within
5°, a single angle): two other single angles at the joint, each level and
**ending** there (not one part with a member, not in line with another of
its members), not in line with the end nor with each other (20° to 160°),
neither bolting onto it, each with a flange in whose plane the end's axis
lies (2°), and those two flanges' outer faces **in one plane**: their
normals within 2°, their faces within the clash check's tolerance (1 mm)
of each other at the joint. Of such pairs the first by class, then the
wider, then the id; the plate lies on the first, its own bolts onto both.
The run names the first for the end (in its planning copy, as L13 names a
member) only where nothing else is named for it: its axis in no flange's
plane of today's choice nor of a member running through the joint, not
within 10° of today's choice's flange (a bent plate lays most of those
out), and no member ending there named by L13. Weighed as L13's names (the
run's as-drawn pass plans the joint again without them and keeps the plan
without where it is not better; a name whose turn no one roll gives with
the member's other ends is taken back). *Measured, not kept*: naming it
before a member running through wherever it already lay flat on the
level legs took an end from joints laid out whole (12 joints worse on four
private towers, a bracing member's name costing the plan member bolted
onto it).

**The plate** (`joint_layout::level_holds`). The plan lays the end onto
the first holder's level leg as any end onto a member: directly where its
bolts land, else onto a flat plate there held by its own provisional bolts
onto that member (L5o) — grown along it as a plate between a built-up
member's angles is (L15), so that those bolts find room past the end's
leg. Then, after every plane of the joint is laid out, the second
holder's bolts (`joint_layout::level_partner`: for any flat plate on a
level member carrying a level end — also one the run named otherwise,
and a first or second holder running on through the joint —, another
level single angle not in line with the end nor with the first holder,
its level leg's outer face in one plane with the first's, by class, the
wider, the id):

- both holders **as drawn**: each moved and lifted as the plan lays its
  own end out (its gauge line through the work point moves it within the
  face it bolts onto); their level legs still in one plane within 1 mm,
  else none (said: "… lies N mm out of the plate's plane");
- the plate's ends taken onto the second holder's flange (the same plane,
  its axes), as the plan places them; the bolts placed as the first
  holder's are (`plate_holds`): as many as onto the first, one gauge line
  along the second holder, within its flange (clear of its heel's leg and
  root by the nut, its edge distance from its toe), past where its steel
  starts by its end distance, inside the plate grown over that flange by
  its edge and end distances, the default minimum spacing from every hole
  through that flange and every bolt on the plate, each head clear of the
  legs on the plate and of what is fixed over the face, each nut of what
  is fixed behind it;
- the plate grown round them (each with its distances, the flange under
  them) clear of what it would run into (as any plate the plan grows),
  each bolt inside the plate's heel line and straight cuts by its
  distances;
- not for a plate in a member's own plane, a bent plate, a plate between
  a built-up member's angles, or one whose outline the plan lays out.

Where they do not fit, said ("H2: no level plate across two horizontals
— …"); the plate stays held by the first alone, as before. Written as the
plate's `[plate.also_bolts]` (the fields of `[plate.leg_bolts]`, the
second member), provisional as the first holder's. **Weighed per joint**:
the bolts onto the second holder only add to a plan; a joint laid out
with fewer ends after the clash rounds than its plan without them, or as
many on more plates, is planned again without them (the run's guard,
`PlanCtx::no_level`, as for L11's and L15's steps).

**Drawn, clash-checked, checked.** Joint details draw the second
holder's end as bolted onto the plate by those bolts (as the leg's for
`[plate.leg_bolts]`), the plate's outline worked out round every bolt on
it. The clash check tests the plate against every part of the joint
except the members bolted onto it (both holders: it lies on them); since
L21 its holding bolts are pieces of it too
([below](#a-plates-own-holding-bolts-in-the-clash-check-l21-item-2)). The
after-writing checks take both holders' bolts alike (inside the outline,
on the member's steel past its cut, no hole overlapped). The joint checks
take both as the plate's holding bolts (`joint_groups`): each holder's
group carries the forces of every other end on the plate (each along its
line through its bolts' centroid, the resultant's moment about these
bolts' centroid, shared elastically) **whole** — *on the safe side*: how
the two share them is not counted on —; each holder's flange in bearing
under its bolts; and the ends on the plate ride on each holder ending at
the joint (L14's path: its own end bolts carry their forces too, each
bolt the larger of with and without them).

**The force the plate passes between its holders** (L21, 2026-11-24;
before, said NOT ON THE SAFE SIDE on the plate's holding rows — the force
passed was missing, so those bolts' usage could be too low): each
holder's holding bolts, its own end bolts and its own steel there also
take the other holder's force passed through the plate, each share from
none to the whole and the other ends' forces whole or none, the larger
bolt by bolt — a statically safe bound
([joint checks](joint-groups.md#the-force-a-level-plate-passes-between-its-two-holders-l21)).

**Not on the safe side.** The end on the plate
lies a plate's thickness out of the holders' level legs, as on any plate
(its framing eccentricity, gauge to centroid, as for any end bolted by one
leg: the code's slenderness curve) — the plate's thickness beyond that is
not in the analysis: its moment is missing, so the member's usage there can be too low; planned (roadmap O45 D: the seat offsets as rigid end offsets), meanwhile check such members by hand or keep a margin. No member's size or work point
changes.


## The welded plate's axis offset carried into the analysis (L20 item 2)

(L19's question 7a and the owner's "go ahead to improve" of 2026-11-23;
roadmap O44 L20, built on its own, reported. Built 2026-11-24, in active
development, not approved yet. No format change: a mark in the layout's
record.)

**Why.** Step 4's plate in an end's own plane, welded to the flat plate
on the flange, lifts the member so that its leg bolted lies on it: its
axis then lies farther out of the flange than on a flat plate. Since
2026-11-19 such an end was laid out only where that is at most a plate's
thickness (about where a filler would put it): the analysis does not
take a line of action farther off the work point. L19 counted 5 674 member
ends refused so on the 78 private towers, 5 065 of them within 40 mm.

**What changed** (`LayoutSettings::own_offset_analysed`, on by default):

- the layout lays such an end out up to `joint_layout::OWN_OUT_MAX` =
  **40 mm** farther out (**the program's value, not a standard's**:
  measured, it takes about nine in ten of the ends refused before);
  farther still, a hand detail as before, said ("… more than the 40 mm
  the analysis takes for a plate in its own plane");
- the plate's offset is recorded with it (the record's mark
  `axis_out=<mm>`, `layout_run::OWN_OUT`; `LayoutRecord::own_out`);
- the **estimated eccentricities** (O45, an option at level 3, off by
  default; their rules unchanged) take it as that end's plate offset: an
  end laid out keeps its shift as written and, now, that offset — **whole
  and of either sign** in its member's own check (N·e, as O45's plate
  offsets), the cases solved without and with the estimates, each member
  and plate its larger usage;
- where the analysis does not take it — the option off, or the joint
  details taken (level 4: the connections' shifts only) — the run says
  **NOT ON THE SAFE SIDE**: how many such plates and the largest offset
  (their moment is missing, so those members' usage can be too low; at
  level 3 *Estimated eccentricities* takes them in);
- an end still left off its flange is given the plate case's upper bound
  at the larger of its leg's thickness and 40 mm (O45 A1, on the safe
  side: such a plate may now put it that far);
- **weighed per joint**: at a joint with an end off its flange the plan
  is made with and without these plates and the one laying out more ends,
  or as many on fewer plates, kept — on a tie the plan without (measured
  on two private towers before: an end left either way but tried on such a
  plate moved its plane's other ends onto plates, 10 joints as many ends
  on more plates); and after writing, a joint laid out with fewer ends
  than its plan without them, or as many on more plates, is planned again
  without them (the run's guard of the level plates, `PlanCtx::no_level`:
  an end laid out on such a plate whose neighbour the clash rounds then
  took back).

**Not on the safe side.** Without the estimated eccentricities (their
default) and at level 4 these offsets are not analysed: said on every run
of such a structure — their moments are missing, so those members' usage
can be too low; at level 3 turn *Estimated eccentricities* on; at level 4
not modelled yet — check those members by hand. The welded plate itself is drawn, clash-checked and
checked (its weld, its own sections) as before.


## A plate's own holding bolts in the clash check (L21 item 2)

(L20's question 3; roadmap O44 L21, built on its own, reported. Built
2026-11-24, in active development, not approved yet. No format change.)

**Before.** A plate's own bolts onto the member holding it
(`[plate.leg_bolts]` since 2026-11-19, the layout's L5o; `[plate.also_bolts]`
since L20) were drawn and checked but were **no pieces of the clash
check**: the plan places them clear (each head and nut by its footprint
against what is fixed over and behind the face, the default spacing from
the holes through that flange), and nothing tested them after writing.

**Now** each such bolt is a part of the clash check
(`ClashPart::PlateBolt`: the plate, the member, its place): its head,
shank and nut, drawn as the holder's end bolted onto the plate draws them
(the head on the holder's side, the nut on the plate's face). Not tested:
the bolt and its own plate; its shank and the member it holds the plate
onto, with a member running on in line (its head and nut lie on that
member's faces and are tested; a built-up member: the whole bolt, as for
an end's bolts); it and the bolts of an end bolted onto that member (they
may be the same bolts). Everything else is tested: the holder's own end
bolts, the other ends' steel and bolts, other plates. A clash is said as
any other (Joint details' problems of the member ends in it, the plate's
notes: "the nut of plate G1's bolt 2 onto L1 and D3 run 4 mm into each
other"); for the layout run's checks after writing it is the plate's (its
members laid out are pulled back, then left as they were, as for a clash
of the plate itself); the plan's fixed steel to place round is unchanged
(those bolts are placed by the plan itself).

**Measured first** (kept privately; L20 main's layouts of 32 of the 78
private towers, read again with the new pieces): **2 418 new clashes** at
978 of 29 312 joints (3.3 %; 10 605 plate holding ends) — the pieces in
them: 1 184 heads, 894 shanks, 340 nuts; what they run into: another
plate 1 151, a member 969, a bolt 298; depth under 2 mm 171, 2–5 mm 804,
5–10 mm 930, 10 mm or more 513. **Turned the other way** (the head on the
plate's face) the same plate and holder still clash at 514 of 531 (on 16
of those towers): the bolt's side does not help. Traced (a horizontal with
two gussets on the same face of its flange, one for the diagonals below,
one for those above): the second gusset, drawn a few millimetres off the
flange, lies over the first one's holding bolts — the plan places a
plate's own bolts clear of what is fixed but not of the other plates and
ends it lays out itself, nor of the bolts on the holder's other flange in
the angle's corner. **The run's reaction** (the guard after writing,
as for a clash of the plate), on eight private towers (the four L20 used
and the next four of its subset; L20 main as the base, the same
settings): 583 such clashes at 234 of 6 852 joints before; after, joints
whole 4 304 → 4 278, member ends laid out 15 337 → 15 233 (−0.7 %),
plates 8 527 → 8 382, 70 joints better and 139 worse (97 with fewer
ends, 42 as many ends on more plates; on the first four, 60 of the 64
worse had such a clash; better: the weighed turns kept otherwise); 170
such clashes left at 71 joints (with
members left as they were — their clashes are said, not acted on, as for
every part — or where the clash rounds ran out). A run again identical (one
tower).

## A plate's own holding bolts placed clear of what the plan lays out (L22)

(L21's question 4; roadmap O44 L22, built on its own, reported. Built
2026-11-24, in active development, not approved yet. No format change;
`LayoutSettings::holds_clear`, on by default; off: as before.)

**Before.** The plan placed a plate's own holding bolts (L5o,
`[plate.leg_bolts]`) clear of what is fixed at the joint and of what the
planes planned before that plate laid out — but not of what the plan lays
out **after** it: a second plate on the same face over the first one's
bolts, the ends and plates of later planes, their bolts' heads and nuts
on the holder's other flange in the angle's corner, the other plates' own
holding bolts. L21's clash check found those after writing, and the run
then took the plate's members back.

**Now**, at the end of each plan of a joint, each plate's own holding
bolts as placed are tested against everything the plan laid out there
and what it leaves as drawn (the members not laid out and their bolts,
plates and covers given by hand): each bolt as the joint details draw it
(L21) — its head behind the flange (0.65 bolt diameters high), its shank
through flange and plate, its nut and the shank beyond it over the plate
(0.8 + 0.3 diameters; the drawing's proportions, not a standard's) —, as
the square round the head's and nut's corner circle (*on the safe side*:
the square holds the hexagon whichever way it is turned) through that
slab, against each piece's footprint in it; an overlap of more than the
clash check's tolerance (1 mm) is a meeting. Excused as in the clash
check: its own plate and the ends on it (placed clear by the plan's own
rules), the member it holds the plate onto, the bolts of ends bolted onto
that member; not tested either, unlike the clash check, a member running
on in line with that member (another piece of the leg, perhaps thicker:
its head and nut against it are left to the check after writing — tried,
see below). Where one meets
something, the joint is **planned again** with that plate's bolts kept
clear of everything of the joint in their slab (by the nut's corner
radius and half the gap, as L5o keeps them clear of fixed steel) — along
the holder, across its flange, fewer where no more fit (at least two; the
joint checks check them, as before), inside the plate's outline by the
edge and end distances the plan already keeps —, at most twice
(`HOLD_TRIES`, the program's bound on the time; what was found in the way
gathered). The plan again is kept only where fewer of those bolts meet
something **and** it lays out as many ends, leaves no more members for a
hand detail and has as many plates held by their own bolts; else the
first plan, as before. Where no place inside the plate is clear the
bolts are placed as before and the plan's note says so ("not clear of the
plates, ends and bolts laid out after it at this joint …, placed as
before — the check after writing tests them"). Deterministic: the same
joint gives the same plan.

Not yet kept clear (the check after writing tests them, as since L21): a
level plate's bolts onto its second holder (`[plate.also_bolts]`, L20), a
bent plate's flat's own bolts (J2b), and the plates L11 puts on both
faces of one plate after the plan.

**Measured** (kept privately; the same eight private towers as L21, the
same settings):

| | L20 main | L21 main | L22 |
|---|---|---|---|
| joints laid out whole (of 6 852) | 4 304 | 4 278 | 4 283 |
| member ends laid out | 15 337 | 15 233 | 15 240 |
| plates | 8 527 | 8 382 | 8 406 |
| plates' own holding ends written | 2 658 | 2 552 | 2 599 |
| a plate's own bolts in a clash after writing (joints) | 583 (234) | 170 (71) | 164 (69) |
| joints better / worse than L21 main | | | 18 / 15 |

A small step: the plan finds such meetings and moves the bolts at some
joints (more plates keep their own bolts, a few more ends), but most of
what L21's guard took back stays taken back. Traced, the clashes left lie
where no place inside the plate is clear — heads into another piece of
the leg running on in line (often thicker), a plate of a member left as
it was over the whole flange — or the plan's joint is not the one the
run finally writes (the run plans a joint several times: with members
left, pulled back, without L11's steps). **Tried, measured, not kept**:
the plate grown along the holder round its bolts where no place inside
is clear (as L15's and L20's plates): 12 joints of one kind laid out
with an end fewer after writing (8 towers: whole 4 273, ends 15 234);
the octagon round the corner circle, the members in line with the holder
against head and nut, and the plate drawn round its own bolts for the
planes planned after it: on the first four towers the same counts as
without. The run's results vary by some tens of joints with any change
of the plans (the weighed turns follow every joint), so differences of
that size are within that spread.

## A plate's own holding bolts tested by the clash check after writing: tried, not kept (L23)

(L22's questions 2 and 3, approved by the owner on 2026-11-24; roadmap
O44 L23. Tried 2026-11-25, measured, not kept; no change to the program.)

**Why.** L22's plan tests a plate's own holding bolts against what it
lays out with its own geometry — close to, not the same as, what Joint
details draws: a plate seated a few millimetres off the flange, its
outline grown round every bolt on it, a member in line with the holder
drawn with its own section. The clash check after writing (L21) then
finds some of those bolts in a clash the plan had not seen, and the run
takes the plate's members back.

**Testing every candidate inside the plan** (the joint written, drawn and
clash-checked for each candidate bolt place) would cost a joint's whole
detail per candidate — the run's check after writing, which already draws
the joint as written, costs about that per joint and round — for
thousands of joints planned several times each. **Tried instead**: the
candidates the plan chooses tested by that check, and the plan told what
it found — where the check after writing finds a plate's own holding bolt
(of a flat plate held by its own bolts, L5o) in a clash, the joint planned
again first with that plate's bolts kept clear of the part they ran into
as the joint details draw it (the clash check's own convex pieces), each
part once per joint, before the plate's members are pulled back; taken
only where the plan again lays out as many ends with as many plates held
by their own bolts. Three forms, then the plate grown along its holder
where no place inside is clear (L22's question 3, tried again with it):

| four private towers (3 111 joints) | joints whole | member ends | plates | a plate's own bolts in a clash after writing (joints) | plates' own holding ends | joints better / worse than L22 | layout time |
|---|---|---|---|---|---|---|---|
| L22 main | 2 093 | 7 103 | 3 756 | 78 (28) | 1 345 | — | — |
| the plan again where it lays out as many ends | 2 093 | 7 103 | 3 756 | 78 (28) | 1 351 | 0 / 0 | about 1.8 times¹ |
| and the plate grown where no place inside is clear | 2 078 | 7 088 | 3 747 | 78 (28) | 1 321 | 12 / 15 | about 2.2 times¹ |
| not for a clash with a member left as it was (said, not acted on) | 2 090 | 7 100 | 3 759 | 87 (31) | 1 357 | 0 / 3 | +8 %² |
| and only where nothing but those bolts moves | 2 090 | 7 100 | 3 759 | 87 (31) | 1 357 | 0 / 3 | |

¹ under a varying load of other work, not comparable; ² against a run
beside it at the same time (one with the next section's change). Measured,
as L21's and L22's comparisons were, with the structure's default minimum
distances (the measuring setup's code plugin had lost its WebAssembly
file — found in L23); both sides alike.

No form lays out an end more; the first changes no joint (six more
holding ends) at a much longer run; the plate grown along its holder
again loses an end at the same kind of joint as in L22 (12 joints of one
kind on three towers); the last two leave one joint kind on three towers
an end short (the bolts moved, the next round's clash rounds then take
another member back) and nine more such clashes. Traced: the clashes
left are where no place inside the plate is clear — heads into another
piece of the leg in line, often thicker, or a plate of a member left as
it was over the whole flange; moving the bolts there only moves the
clash. **Not kept** (the owner's rule: no joint worse). Next (an owner
question): those plates need another holding — the member's own end
bolts (L14) where they reach, or a hand detail, said as now.

## The members' turns and the far-out welded plates: tried, not kept (L23)

(L21's item 3: L20 item 2's joints left worse — the owner kept the offset
step on, 2026-11-24. Tried 2026-11-25, measured, not kept; no change to
the program.)

**Why.** L20 item 2 lays an end out on a plate in its own plane welded to
the flat plate even where that puts its axis up to 40 mm farther out (the
offset recorded for the analysis), weighed joint by joint. But an angle
has one roll along its whole length, and the run chooses some turns — the
half and quarter turns (steps 2b, L3b) and L10's other faces — by counting
the ends its plans lay out at all the member's joints. With the welded
plates in those counts, a turn that laid one end of a member out could be
refused because its other end, at another joint, now went onto a welded
plate without it (L20 measured 22 joints worse on four private towers).

**Tried**: those turns chosen on plans **without** the welded plates (as
the plates between a built-up member's angles have been since L16), the
joints then planned with them where the rolls let them — so that such a
plate never changes a member's roll. **Measured worse** (four private
towers, against L22 main): 24 joints better, 28 worse, 30 member ends and
49 plates fewer. Traced: the welded plates gain mostly *through* turns —
a member turned so that its end goes onto a welded plate lets the other
ends of that joint onto plates too (one joint: three ends laid out with
the turn, none without). Taking the welded plates out of the turns' counts
loses those gains to save the few joints a turn costs. **Next** (an owner
question): weigh each turn the welded plates change as L19 weighs the
plane turns — the turn decided with and without them, and the one with
them kept only where every joint its member reaches is laid out no worse
and one better.

## The joggle (L19's question 7b): measured, not built (L23)

A joggle — a plate bent twice — was to bring back the axis of an end at
most 15° off its flange that a plate in its own plane puts farther out
than one plate's thickness. **It cannot, by geometry**: the member's leg
is straight, so over the flange its lowest point must lie at least a plate
thickness above the flange's face (the plate under it), and from there its
leg rises at its angle θ to the flange along the slope to its bolts: the
axis at the bolts lies at least (that distance) · sin θ farther out than
on a flat plate, whatever the plate under it does between the flange and
the leg. A plate in its own plane welded to the flat plate already comes
within the seam's room (the gap, 10 mm, along the slope: about 1 mm at
7°) of that bound; a single bend within the bend's curved part. On the
eight private towers L21–L23 measure, of the 60 ends at most 15° off
refused so, **5** would come within a plate's thickness with no room at
the seam at all (and 20 if also their bolts' edge distance to the bend
were nothing — which no plate allows). So nothing was built; an owner
question instead (the roadmap's L23 block).

## The clash check

Every solid at a joint — each member's steel as placed from its cut to
half its length, plates, covers, each bolt's head, shank and nut (since
L21 also a plate's own holding bolts) — is cut
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
| How far out of today's choice's flange an end must lie to be named onto a member ending at the joint by its plane (L13) | 10° | measured on the 78 private towers: the ends 5° to 10° out made 250 joints worse and 137 better (a bent plate lays many out); those farther out 1 to 4 times more better than worse |
| Passes planning a joint again with the members named but not laid out where the file draws them (L8) | 2 | a bound on the extra work: on six private real towers the second pass found 3 more joints (of about 1 190 in the first), one plate more |

## The detailing practice (L24)

(The owner, 2026-11-24: the layout's detailing rules pluggable as data,
the engine staying in the program, with six design questions answered by
their recommendations; roadmap O44 L24, built 2026-11-25 as recommended,
in active development, not approved yet;
[plugins](../development/plugins.md#detailing-practice).)

**Why.** Shops and utilities detail towers differently: some never weld,
some keep every gusset round the work point, some bolt three bolts where
others bolt two. Until L24 those preferences were constants in the code.
A **detailing practice** holds them as data and the layout follows it.

**Where it comes from.** The structure file's own `[layout_practice]`
table (structure file version 27) over the answer of the code plugin of
its member check (its optional `layout-practice` export), **value by
value**, over the default — the layout as it was before L24 exactly
(every value the constant the code had), so that an empty table lays out
as today.

**What it holds** (each with its default):

| Preference | Default | What the layout does with it |
|---|---|---|
| Fabrication order | direct → plate → bend → bends → welded, all allowed | a form not allowed is never made (an end needing it is left for a hand detail, said); the order decides only ties — the weighing keeps its rule, more ends or as many on fewer plates — today between a plate bent by a few degrees and one welded in the member's own plane (both fit an end: the earlier in the order is taken); "bends" (several bends) not built |
| Faces of a leg's flange | a diagonal onto the inner face first | L10's order of the ends tried on the other face |
| Members on either side of a flange | yes | L10's half turn onto the other face |
| Both faces of one plate | yes | L11 |
| Gap, pull-back, shift | 10, 300, 30 mm | the Lay out joints window's starting values (the settings above) |
| Flatness | 2° | where the planner decides an end lies flat on its flange, and the roll for a whole member |
| Small bend, at most | 20° | L3's plate bent by a few degrees |
| Welded plate's axis offset, at most | 40 mm | L20 item 2 |
| Bend's inside radius | 2 thicknesses | J2b's and L3's bent plates |
| Plate between angles, least gap | 0.5 × the thickest leg | L15 |
| Plates may stop short of the work point | yes | L2's follow-up (a) |
| The picked bolt's pattern | 2 in one line | the provisional bolts of an end without bolts or a class preset |
| Per member class | none | the picked bolt's pattern per class (as the connection presets are); per joint kind later |

**On the safe side.** A practice only chooses among what the engine does
already: it turns forms off, orders a tie and changes the program's
numbers, never adds a way of detailing. The core's checks are no values
of a practice: the clash check, the joint checks after writing (the
code's distances, holes overlapping, the bend check, the NOT ON THE SAFE
SIDE words) and the guard that no step leaves a joint worse run whatever
it says. Every value is checked and a practice with one out of its range
is refused — not used, the default used instead, said in Lay out joints
and in the run's notes — with what, why, what to give instead and whose
table:

- the flatness 1° to 2°: the joint details take an end as lying flat up
  to 2° (the owner's value, 2026-11-19); a looser practice would bolt
  flat ends they see off their flange — a forced fit whose locked-in
  bending no check takes. **Not changed with it**: the joint details, the
  clash check, the joints' planes and groups, Orient angles and the
  level plates' in-plane tests keep 2°, so a stricter practice puts more
  ends onto bent or welded plates but never lets one count as flat that
  the joint details see off;
- the shift up to 30 mm and the welded plate's axis offset up to 40 mm:
  what the estimated eccentricities take as their upper bounds where no
  plan exists (O45, decided 2026-11-19, until J5 checks the moment
  instead of refusing the end) — a larger value would let the layout make
  an offset beyond what the analysis is told;
- a bend's inside radius at least one plate thickness (a tighter bend
  may crack; the code plugins' bend checks check the radius against their
  code, and without such a check the bend is said NOT ON THE SAFE SIDE as
  before);
- the plate between a built-up member's angles at least half the
  thickest leg on it (the program's least, L15);
- 2 to 8 bolts in one gauge line, for the structure and per class: one
  bolt per end is the codes' question (J8), two lines for the picked bolt
  wait on the codes' bolt choice;
- "direct" and "plate" always allowed: the layout has no way without
  them; the five forms each once.

**Shown, and recorded.** Lay out joints shows, below its preview, the
practice in force, where it comes from, why one given is not used, and
each value with where it comes from (this file, the code plugin, the
default) and its effect. A run records the plugin's answer in the layout
record; an edit's follow and the estimated eccentricities lay the file's
table over it, as the run did. **Expected cost**: none measurable —
reading a practice is microseconds; a form not allowed makes a run a
little faster, never slower. The effect of each value on the private
real towers is not measured yet (each value's measured effect is the
owner's sixth recommendation: next, with the bundled `asce10` built
fresh, L23's finding).

**Tested** (`practice_tests.rs`, `ohl-structure-tests/tests/detailing_practice.rs`):
the default practice leaves the engine's settings exactly as they were
(also with switches off) and lays an invented K-brace out to the same
plans, the same file (but for its `[layout_practice]` table) and the
same estimated eccentricities, its diagonal flat, 10° and 30° off; the
file's values over the plugin's value by value, each value's source; a
shop without bends puts the 10° diagonal on a welded plate, one whose
order puts the welded plate first too, one without bends and welds
leaves it for a hand detail, said; an axis offset limit of 0 leaves the
30° diagonal by hand; three bolts per end, four for the bracing class;
every refusal, in the run's notes too; the record's plugin answer; the
example plugin's practice through the real host.

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

(Corrected with L7, 2026-11-20: the "L5" column ran with the bundled code
plugin's bolt minimums, the "before" column with the structure's default
minimums — a difference of the measuring, see
[L7](#the-measured-columns-traced-l6s-question-6). On the same footing the
moves laid out 218 more joints whole, not 494, and 1.0 % fewer plates, not
6.2 %.)

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
  O45 D: its moment would be missing, so the member's usage too low).
- **Time**: every thirteenth tower, six, laid out back to back twice at a quiet
  moment: the median per tower 19.6 s before, 24.8 s with L5 (both passes
  alike), the largest 21.3 s → 26.7 s — about a quarter more: each joint
  planned twice (the guard) and the moves searched a millimetre at a time.
  The full columns ran beside other work on a shared machine: their
  times are not compared.

## Measured (L6, 2026-11-20)

On the same 78 private tower files and settings as L5 (J9's proposals and
J7a's leg splice places confirmed, every switch before on). "Before" is
the main build of 2026-11-20 measured again (with L5); "L5o" adds the flat
plates' own bolts and J6's root radius of a built-up member's angles (J6's
plates off); "L6" all of it (the default now).

| | before | L5o | L6 |
|---|---|---|---|
| joints laid out whole | 32 376 (50.0 %) | 32 234 (49.8 %) | 33 776 (52.2 %) |
| member ends laid out | 116 245 (65.2 %) | 115 672 (64.9 %) | 122 441 (68.7 %) |
| ends left: built-up member | 8 197 | — | 1 857 |
| plates written | 65 240 | 64 898 | 71 499 |
| … their steel (the bill of materials, 73 towers) | 92.6 t | 88.7 t | 107.4 t |
| bolts (the bill of materials) | 183 149 | 213 018 | 224 254 |
| plates whose holding is not checked | 24 491 | 18 732 | 21 565 |
| plates' holding groups checked | 6 392 | 20 045 | 19 184 |
| shifts written | 1 789 | 1 752 | 1 704 |
| joints that do not fit, as given / as assumed | 26 / 13 007 | 26 / 13 082 | 26 / 13 204 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 |

- **L5o**: about 13 650 more plates are held by their own bolts (holding
  groups 6 392 → 20 045); the plates whose holding is not checked fall by
  a quarter. Those left: plates stopping short of the work point, plates on
  a built-up member, plates where two bolts do not fit. On their own the
  holding bolts cost about as many joints laid out whole as they gain
  (99 more, about 100 fewer, the run's guard taking most back); the 142
  fewer of the column are mostly J6's root radius: about 180 joints whose
  ends no longer land directly on a built-up member's angle (on the safe
  side). The bolts count about 30 000 more (two or more per plate).
- **J6**: 1 542 more joints laid out whole than L5o (+2.4 points), 6 769
  more ends; the ends left for a built-up member 8 197 → 1 857 (those left:
  a plate between the angles, or the plan with the plate laying out no
  more). About 6 600 more plates, their steel about a fifth more (the
  plates the hand detail would have made, now counted).
- **The clashes after writing** (L5m, counted on L6's run; about 14 300
  member ends left at the time of writing, by the pair of parts and what
  each part's member is): a plate laid out running into the steel of a
  member left as it is — a piece running on as one part with another, a
  section not a single angle, the partner of a member bolting onto it —
  about 3 900; a member left as it is whose bolts (as assumed) no longer
  fit the member laid out under it (off its flange or beyond its steel)
  about 1 700; a head or nut of a member end against a plate about 2 500;
  steel against steel of pieces in line or ends left for a hand detail
  about 1 900; a plate's own bolts outside its outline after writing about
  370 (most of them J2b's bent flats, before L6 too). One traced: a plate
  4 mm thick on a member's flange running 4 mm into a member drawn on its
  work line (no bolts, no seat) at the work point, which the plan's plate
  check did not see. Not changed; proposals in the roadmap (O44 L6).
- **Time**: every thirteenth tower (six), laid out back to back twice, the
  machine shared (load 7–11, another build beside it): the median per
  tower 26.7 and 26.9 s before, 33.7 and 36.2 s with L6 — about 30 % more
  —, the largest 28.5 s → 41.5 and 42.6 s: J6's guard plans each joint
  with a built-up member twice more, and the holding bolts' places are
  searched 2 mm at a time. The full columns ran beside other work: their
  times are not compared.

## Measured (L7, 2026-11-20)

On the same 78 private tower files and settings as L6, now **with the
bundled code plugin found** (its bolt minimums, as the program ships; L6
measured without it — see
[L7](#the-measured-columns-traced-l6s-question-6)). "Before" is the main
build with L6, measured again so; "L7" with the seat and the legs mapped.

| | before | L7 |
|---|---|---|
| joints laid out whole | 33 611 (51.9 %) | 34 024 (52.5 %) |
| member ends laid out | 121 850 (68.4 %) | 123 412 (69.4 %) |
| plates written | 67 432 | 69 154 |
| … their steel (the bill of materials, 73 towers) | 105.3 t | 111.8 t |
| bolts (the bill of materials) | 221 923 | 230 566 |
| plates' holding groups checked | 18 269 | 21 042 |
| plates whose holding is not checked | 20 937 | 21 270 |
| shifts written | 2 188 | 2 112 |
| joints that do not fit, as given / as assumed | 28 / 13 028 | 35 / 12 906 |
| a run again identical | 78 of 78 | 78 of 78 |

The member ends left at writing, by the record's reasons:

| | before | L7 |
|---|---|---|
| a plate into steel | 4 089 | 3 117 |
| a nut into a plate | 1 519 | 1 204 |
| a head into a plate | 1 079 | 1 128 |
| steel into steel | 1 588 | 1 562 |
| a member left as it is no longer fitting | 1 755 | 1 734 |
| a plate's own bolts outside its outline | 294 | 228 |
| holes overlapping, bolts beyond a member's end | 141 | 249 |

- **The seat**: per joint 762 more joints laid out whole, 349 fewer (mostly
  clashes the plan still does not foresee, now found where the steel
  lies). A plate into steel after writing falls by a quarter, a nut into a
  plate by a fifth. The plates and bolts rise with the ends laid out. The
  joints not fitting as given rise by seven on the one tower whose bolts
  are given: ends now drawn on what they bolt onto where it is.
- **The legs mapped**: about 2 800 more plates held by their own bolts
  (holding groups); the plates whose holding is not checked rise a little
  with the plates.
- **What is left** (with the members' parts as the plan sees them, on six
  of the towers): plates into the steel of pieces running on as one part
  and of sections not single angles, the partner of a member bolting onto
  it no longer fitting (off its flange mostly), heads and nuts of members
  left for a hand detail against plates on members left as they are. In
  the roadmap (O44 L7).

## Measured (L8, 2026-11-21)

On the same 78 private tower files and settings as L7, with the bundled
code plugin found. "Before" is the main build with L7; "as drawn" adds a
joint planned again with the members named but not laid out where the
file draws them (and the bolts already there found wherever the
structure lies); "L8" adds the partners' bolts kept.

| | before | as drawn | L8 |
|---|---|---|---|
| joints laid out whole | 34 024 (52.5 %) | 34 560 (53.4 %) | 34 515 (53.3 %) |
| member ends laid out | 123 412 (69.4 %) | 126 350 (70.7 %) | 126 362 (70.1 %) |
| plates written | 69 154 | 70 860 | 70 722 |
| … their steel (the bill of materials, 73 towers) | 111.8 t | 116.1 t | 115.8 t |
| bolts (the bill of materials) | 230 566 | 235 201 | 235 797 |
| plates' holding groups checked | 21 042 | 20 610 | 20 797 |
| plates whose holding is not checked | 21 270 | 21 759 | 20 973 |
| shifts written | 2 112 | 2 295 | 2 314 |
| joints that do not fit, as given / as assumed | 35 / 12 906 | 35 / 12 877 | 42 / 12 883 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 |
| time per tower, median (six towers, back to back, twice) | 30 s | 41 s | 45 s |

(The share of ends is of the ends the plan asks about; L8 asks about more
— an end left for a partner's bolts is now one the plan names, before it
was taken back after writing: 178 608 → 180 314.)

The member ends left at writing, by the record's reasons:

| | before | as drawn | L8 |
|---|---|---|---|
| the plan: its plate into steel | 6 380 | 4 681 | 4 446 |
| a plate into steel | 3 117 | 2 272 | 2 326 |
| a member left as it is no longer fitting | 1 734 | 1 379 | 152 |
| steel into steel | 1 562 | 962 | 942 |
| a nut into a plate | 1 204 | 1 051 | 1 073 |
| a head into a plate | 1 128 | 701 | 681 |
| a plate's own bolts outside its outline | 228 | 37 | 39 |
| holes overlapping, bolts beyond a member's end | 249 | 100 | 79 |

- **As drawn**: per joint 561 more joints laid out whole, 25 fewer; about
  2 900 more ends, 1 700 more plates with them. The bent flats' bolts off
  their flat (L6's question 7) fall from 228 to 37 ends; every kind of
  clash after writing falls, most the plates into steel (planned and after
  writing) and steel into steel. Slower: the joints with such members are
  worked out and checked once more.
- **The partners' bolts**: the members left as they are no longer fitting
  fall by nine tenths (1 379 → 152; the rest not traced yet — an end off
  its flange or leaving over the heel is not asked in the plan); about as many ends
  on 138 fewer plates; 45 fewer joints whole (32 won, 77 lost: the end now
  left in the plan, the joint planned around it); plates whose holding is
  not checked fall by 786. The seven joints more not fitting as given are
  one joint repeated on seven towers of one type (above).

## Measured (L9, 2026-11-21)

On the same 78 private tower files and settings as L8, with the bundled
code plugin found. "Before" is the main build with L8; "plane" adds a
plate's plane read from its whole outline; "as written" adds an end on a
plate seated where the plate's member is drawn and the welded plates' flat
plates planned as written; "L9" adds a flat plate's clashes after writing
owned by its welded plates' members and the holding bolts on plates
stopping short of the work point (and the speed, which changes no number:
the written files of all 78 towers the same byte for byte).

| | before | plane | as written | L9 |
|---|---|---|---|---|
| joints laid out whole | 34 515 (53.3 %) | 35 435 (54.7 %) | 36 406 (56.2 %) | 36 130 (55.8 %) |
| member ends laid out | 126 362 (70.1 %) | 128 561 (71.3 %) | 129 270 (71.6 %) | 128 092 (71.0 %) |
| plates written | 70 722 | 72 063 | 70 940 | 69 295 |
| … their steel (the bill of materials, 72 towers) | 115.8 t | 121.3 t | 122.4 t | 120.6 t |
| bolts (the bill of materials) | 235 797 | 242 698 | 245 161 | 244 497 |
| plates' holding groups checked | 20 797 | 21 520 | 21 908 | 22 443 |
| plates whose holding is not checked | 20 973 | 21 733 | 22 003 | 21 049 |
| shifts written | 2 314 | 2 349 | 2 379 | 2 412 |
| joints that do not fit, as given / as assumed | 42 / 12 883 | 50 / 12 567 | 51 / 11 576 | 53 / 11 633 |
| a run again identical | 78 of 78 | 78 of 78 | 78 of 78 | 78 of 78 |

The member ends left at writing, by the record's reasons:

| | before | plane | as written | L9 |
|---|---|---|---|---|
| the plan: its plate into steel | 4 446 | 4 206 | 4 049 | 4 051 |
| a plate into steel | 2 326 | 2 300 | 1 410 | 1 517 |
| a nut into a plate | 1 073 | 788 | 785 | 776 |
| a head into a plate | 681 | 19 | 19 | 19 |
| steel into steel | 942 | 777 | 518 | 479 |
| a member left as it is no longer fitting | 152 | 158 | 147 | 131 |
| a plate's own bolts outside its outline | 39 | 25 | 69 | 24 |
| holes overlapping, bolts beyond a member's end | 79 | 81 | 67 | 55 |

- **The plane**: 920 more joints laid out whole, 2 200 more ends; the
  heads against plates after writing all but gone (681 → 19), the nuts a
  quarter fewer; "does not fit, as assumed" 316 fewer (the joint checks
  read the plates in their planes too). Eight joints more not fitting as
  given: the mirrored twins of two joints known since L8 (now laid out on
  both sides of the tower), said by the joint checks.
- **As written**: 971 more joints whole, about 700 more ends on 1 100 fewer
  plates; the plates into steel after writing 2 300 → 1 410, steel into
  steel 777 → 518, "does not fit, as assumed" 991 fewer. The plates' own
  bolts outside their outline rise (25 → 69): holding bolts on flat plates
  now kept whose outline, kept to their seams, leaves them out — next.
- **L9**: owning a flat plate's clashes after writing takes back ends
  that were laid out with such a clash written unsaid (the most of the
  1 178 fewer ends; the plates into steel after writing count them now,
  1 410 → 1 517); the short plates' holding bolts, measured alone on six
  towers: holding groups checked 1 747 → 1 835, plates whose holding is not
  checked 1 624 → 1 558, for 3 fewer joints whole and 18 fewer ends (of
  9 537).

The time per tower (the layout alone, six towers — every 13th —, two at
a time, back to back, twice; nothing else running, load 3 to 8; seconds,
sorted):

| | first | second | median | largest |
|---|---|---|---|---|
| before (L8) | 4.4 39.8 45.8 46.0 46.9 49.9 | 4.4 43.6 44.4 44.5 46.8 49.9 | 45.9 / 44.5 | 49.9 |
| L9 without the speed | 4.5 36.8 40.6 44.3 45.2 45.2 | 4.4 36.8 40.7 44.4 45.1 45.1 | 42.5 / 42.5 | 45.2 |
| L9 | 3.8 29.1 32.3 33.5 34.3 34.5 | 3.9 29.1 32.3 33.7 34.2 34.6 | 32.9 / 33.0 | 34.6 |

## Measured (L11, 2026-11-21)

On the same 78 private tower files and settings as L10, with the bundled
code plugin found. "L10" is the main build before; "L11" adds one plate per
plane and both faces of one plate (on in a run) and the run's guard; the
end moved to pass the flange on a plate is off in a run (below).

| | L10 | L11 |
|---|---|---|
| joints laid out whole | 35 687 (55.1 %) | 35 656 (55.1 %) |
| member ends laid out / candidates | 130 329 / 183 541 (71.0 %) | 130 097 / 183 476 (70.9 %) |
| plates written | 70 140 | 67 532 |
| one plate per plane (pairs) / ends on a plate's back | – | 822 / 755 |
| shifts written | 2 675 | 2 744 |
| members left after the clash rounds | 9 438 | 9 485 |
| plates whose holding is not checked / holding groups | 21 394 / 23 340 | 21 195 / 22 238 |
| joints that do not fit, as given / as assumed | 48 / 11 461 | 48 / 11 552 |
| a run again identical | 78 of 78 | 78 of 78 |

Joint by joint (each joint's ends laid out, plates, whole): 1 675 joints
better (more ends, or as many on fewer plates, or newly whole), 240 worse
(fewer ends, or as many on more plates, or no longer whole; 80 no longer
whole, 49 newly whole). The run's guard plans a joint again without the new
steps where it lays out fewer ends after writing than its plan without them;
the joints still worse lose an end through a change at a neighbouring joint
(a member turned or a plate grown there), which the guard of one joint does
not see — to trace next.

**The end moved to pass the flange on a plate, measured alone** on every
thirteenth tower (six): fewer ends at 11 joints, more at 8, plates +2; with
both steps 109 joints better and 28 worse, with both faces alone 99 and 15.
It is therefore off in a run and tried per joint by Solve the clashes, where
the joint's outcome after writing decides.

The time per tower (six towers, back to back, twice, nothing else running)
median 35 → 38.5 s, the largest 38 → 41 s: the plans made again without the
new steps for the guard.

**The leg-rotation check**: after Orient angles (as the measurements lay
out), 114 of 9 429 leg pieces with estimates flagged, on 22 towers — 110 of
them leg extensions' pieces Orient angles finds no faces for (±3.8° to
±4.1°), four at ±13.5° where Orient's faces say 0°; on the towers as
imported (the import's rolls, no Orient) 4 212 of 8 695.
