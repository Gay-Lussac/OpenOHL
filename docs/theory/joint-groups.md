# Joint bolt groups and the leg's bearing

Implemented in `apps/structure/ohl-structure-link/src/joint_groups.rs`
(the groups gathered per joint, their forces per case), with the
mechanics of `crates/ohl-structures-fe/src/bolt_groups.rs` (`elastic`,
`Plane`) and each bolt's checks in the design codes' bundled plugins —
ASCE 10-97 in `plugins/codes/asce10/src/groups.rs`, SIA 263:2013 in
`plugins/codes/sia263/src/groups.rs` — reached through the plugin
interface `joint-check-groups` (2026-11-19; one call with every group and
case) and, with the bolts in tension, `joint-check-groups-tension`
(2026-11-19) or, with the gusset plates' own prying too,
`joint-check-groups-prying` (2026-11-19), run with every case of OHL-Structure's analysis (Results › Run)
and shown with the plate checks in Joint details, the 3D view's joint
colours and the structure report. Stage 4.5 of the
[joint details](../development/joint-details.md) ("checks of the joints
themselves"). Built, in active development (not approved yet); pre-alpha:
results must be verified by a qualified engineer.

**Sources.** ASCE 10-97, *Design of Latticed Steel Transmission
Structures* (American Society of Civil Engineers), and SIA 263:2013,
*Construction en acier* (Swiss Society of Engineers and Architects);
clause, table, figure and equation numbers below are the standards',
each read in their texts. The elastic sharing of a bolt group's moment:
Kulak, G. L., Fisher, J. W. and Struik, J. H. A., *Guide to Design
Criteria for Bolted and Riveted Joints*, 2nd ed., Wiley, New York, 1987,
its chapter on eccentrically loaded joints (the guide ASCE 10-97's
commentary cites as Kulak et al. 1988). The prying of bolts in tension:
the T-stub model of Struik, J. H. A. and de Back, J., *Tests on Bolted
T-Stubs with Respect to a Bolted Beam-to-Column Connection*, Report
6-69-13, Stevin Laboratory, Delft University of Technology, 1969, as the
same guide gives it (its chapter 17, *Tension-type connections*, Eqs.
17.8–17.12 and the design recommendations of 17.6.1 — checked there; the
Stevin report itself was not at hand). Every assumption below is marked
**on the safe side** (conservative) or **not on the safe side**.

## What is checked

The [plate checks](joint-plates.md) look at each member's bolts in a
plate, and the [member checks](asce10.md) at each member end's own bolts,
each under that member's own force. Where **several members bolt onto
the same part** at a joint, its bolts carry their forces **together**:

- **A plate's bolts onto the leg holding it** (`holding`): a gusset
  plate (or a welded plate: plates welded together) that the joint's leg
  bolts onto and other members bolt onto too — a leg piece's own end
  bolted onto the plate, or (2026-11-19) **the plate's own bolts onto the
  leg** (`[plate.leg_bolts]` in the structure file: a leg running on
  through the joint has no bolts of its own there). The leg's bolts on it carry
  the **resultant of the other members' forces on the plate**, with the
  moment of their lines about those bolts' centroid. Checked: each bolt's
  shear, its bearing on the plate and the plate's edges along and square
  to its force.
- **The bolts onto a member at a joint** (`bearing`, the leg's bearing):
  every bolt through one member's flange at the joint — the members
  lapped onto it and a plate's holding bolts — each with the **vector
  sum** of the forces of every part it holds. Checked: each bolt's shear
  under that sum, the member's bearing under it and its edges along and
  square to it.
- **A member's own end bolts with the ends laid onto it** (2026-11-22,
  L14; *Part* "its end bolts with … laid onto it"): a member ending at
  the joint that other ends are bolted onto near its end — its own end
  bolts carry their forces with its own
  ([below](#the-force-through-a-members-own-end-bolts-l14)). Checked: each
  bolt's shear, its bearing on the member and, onto a plate, on the plate,
  and the distances along and square to its force.

Each bolt is checked — in shear and bearing, and (2026-11-19) in tension
with prying where the forces pull it ([below](#bolts-in-tension-with-prying));
a group's usage in a case is its most loaded bolt's largest check, named
with the bolt (its place from the group's centroid, what it carries, its
force) and the check's clause.

**The joint's usage** is the largest of its plates', welds', bolt groups'
and bearing checks over the cases: shown in Joint details ("Usage of the
joint"), coloured in the 3D view (the joints' dots, as the members) and
listed per joint in the structure report. A part not checked is counted
beside it, never taken as a pass. **A joint that does not fit** as given
reads so ([below](#a-joint-that-does-not-fit)): failing, counted apart,
without a usage.

## The forces

**Same case only; each member at every joint it bolts at.** A member's
force on the part is its **end force** at that joint from the analysis
(the force on the member there, reversed: a frame member's end shear with
its axial force), its end moment with it; without end forces, its axial
force along it. The forces of different cases are never summed or mixed.

**A member's own bolts share its force equally** (ASCE 10-97 4.1: "bolts
connecting one member to another carry the load in the connection
equally"): its force acts along its line through its bolts' centroid.
Its own gauge's offset from its centroidal axis (the "normal framing
eccentricity") is **not taken as a moment on its bolts** — the standard
takes it into the member's slenderness (3.7.4) and its bolts by 4.1.
SIA 263 6.2.4.6 asks for "une attention particulière" to that
eccentricity in a single angle bolted by several rows, without a rule:
**not on the safe side** where the joint does not carry it by bending of
the member (said here, as in the member checks). A frame member's end
moment, and its end shear's lever to its bolts' centroid, are shared
**elastically** by its bolts (below).

**The plies' offsets through their thickness** (a lapped member's bolts
on the far face of the leg) are left out of the levers — the moment they
make bends the bolts, which a bearing-type connection's checks of both
standards do not take (the bolts' shear and bearing). The same as every
lapped connection in the member and plate checks.

### A plate's holding bolts

The resultant on the plate, at the holding bolts' centroid \\( c \\):

\\[ \mathbf F = \sum_i \mathbf F_i,\qquad
M_c = \sum_i \left( M_i + (\mathbf c_i - \mathbf c) \times \mathbf F_i \right)\cdot \mathbf n \\]

over the members bolting onto the plate other than the holding leg —
\\( \mathbf F_i \\) member \\( i \\)'s force on the plate at its bolts'
centroid \\( \mathbf c_i \\), \\( M_i \\) its end moment, \\( \mathbf n \\)
the holding bolts' axis. By the plate's equilibrium the holding bolts carry
\\( -\mathbf F \\) on the plate, \\( +\mathbf F \\) on the leg.

- **The holding leg** is the joint's leg (the members of class `leg`,
  else the steep ones, the member a plate's own bolts onto the leg name,
  and since 2026-11-22 the member the plate is **held by** — `held_by`,
  structure file version 24: where nothing runs through the joint, the
  base member whose own end bolts onto the plate hold it, L14) bolted onto
  the plate, with the members running on in line with
  it through the joint as one part (`continuous`). **Pieces in line not
  said continuous each end at the joint** (owner 2026-11-19; until then
  taken as one angle running on, an assumption): each is a member of its
  own there, its steel stopping at the joint — **on the safe side** (its
  bolts are its own; an end where its steel stops; the other piece's
  bolts not counted with it), said in the group's notes ("… in line with
  … not said continuous: each taken as ending at the joint"). Where the
  piece's steel stops is not given, the code's conservative case
  ([below](#the-distances-along-a-bolts-force)). Its own force does not
  pass through the plate when it runs on; where it ends at the plate its
  force is the others' resultant (equilibrium), the same. The joint
  layout proposes `continuous` for the user to confirm (O44 J9, built
  2026-11-19 — [joint layout](joint-layout.md#pieces-in-line-one-part-confirmed-j9));
  until confirmed, it lays out what bolts onto such a piece on that
  piece's own steel too. Where each piece of a leg spliced on
  the plate carries its own force through its own bolts, that is the
  plate check of each piece (its own force, ASCE 10-97 4.1) — not added
  here.
- **A welded plate**: the members on any of its plates. The holding legs'
  bolts on several of its plates: each plate's bolts taken carrying the
  whole (**on the safe side**, said).
- A member whose bolts on the plate are not placed: its force through
  the joint (said).
- The joint's own loads (applied at its node) are taken by the leg, not
  through the plate: the analysis puts them on the node, and a load on
  the plate itself (a string's attachment) would be a plate's member of
  its own.

### The force through a member's own end bolts (L14)

Since the joint layout's L1 (d) and L13 an end may be laid onto a member
that **ends** at the joint (a plan diagonal onto a face horizontal's level
leg, say). Its force enters that member near its end and leaves through
**that member's own end bolts** — together with the member's own force.
Before 2026-11-22 those bolts were checked for the member's own force
only (**NOT ON THE SAFE SIDE**, said in the layout's notes). Now (the
owner's answer of 2026-11-22 to L13's question 3) the bolt groups follow
the force through:

- **Which ends**: every end bolted onto a member (`onto`) that ends at
  the joint — not a leg (the joint's leg, or a member a plate's own bolts
  onto the leg name: its force goes on through its splice or the plate
  it holds), not running on as one part (`continuous`); pieces in line
  not said continuous each end at the joint (as everywhere here, on the
  safe side). A chain is followed (an end laid onto an end laid onto the
  member), each member once. Left out: an end said one part with
  another, a modelling member, an end not bolted, and two members lapped
  onto each other (their one connection).
- **The force**: the member's own end force at its bolts' centroid
  \\( \mathbf c_0 \\) plus each such end's force \\( \mathbf F_i \\) along
  its line through its own bolts' centroid \\( \mathbf c_i \\), the same
  case:

  \\[ \mathbf F = \mathbf F_0 + \sum_i \mathbf F_i,\qquad
  M = M_0 + \sum_i \left( M_i + (\mathbf c_i - \mathbf c_0) \times \mathbf F_i \right)\cdot \mathbf n \\]

  shared by the member's end bolts **elastically** (below), as a plate's
  holding bolts share theirs. Each bolt is then taken under **the larger
  of that and the member's own force alone** (bolt by bolt): where another
  end's force relieves the bolts (pulling against the member's own), the
  relief is not counted on — **on the safe side**; so a bolt's force can
  only grow. Its pull out of the plies' plane likewise the larger.
- **Where it is checked**:
  - the group of the member it bolts onto (`bearing`): its bearing under
    those bolts, each bolt's shear, the distances along and square to the
    force — the bolts now carrying the forces followed through;
  - a plate's holding bolts (`holding`): the ends laid onto a member on
    the plate added to the plate's resultant, each along its line (the
    larger of with and without them, bolt by bolt);
  - **its own end bolts' group** (new, *Part* "its end bolts with … laid
    onto it"): each bolt's shear, its bearing on the member itself and,
    onto a plate, on the plate — with the distances to the member's end
    (where its steel stops, or the code's conservative case where not
    given) and edges;
  - a weld of a welded plate: the ends laid onto a member on the plate
    on that member's side of the weld (the larger of its loads with and
    without them).
- **Not covered, said NOT ON THE SAFE SIDE** (a row of the joint without
  a usage, "NOT ON THE SAFE SIDE: H1's own steel at its end bolts …"):
  the member's own steel at those bolts — its net section and its tearing
  out, checked by its member check under its own force only — and, onto a
  plate, the plate's own checks under them (its tearing out, its
  effective width) under the member's own force only. A member ending at
  the joint with ends laid onto it that bolts onto nothing there, or whose
  bolts are not placed: "NOT CHECKED, NOT ON THE SAFE SIDE: … where their
  forces go is not given".

### Elastic sharing (Kulak, Fisher & Struik)

The part the bolts hold is rigid and the bolts equally stiff, so the
force through the centroid is shared equally and the moment about it in
proportion to each bolt's distance \\( r_k \\) from the centroid, square to
its radius:

\\[ \mathbf f_k = \frac{\mathbf F}{n} + \frac{M_c\,(\hat{\mathbf n} \times \mathbf r_k)}{\sum_j r_j^2} \\]

The **elastic** analysis is the simpler and the more conservative of the
guide's two (its ultimate-strength analysis lets ductile bolts share
more): **on the safe side** for bolts that bear. One bolt (or bolts all
at their centroid) cannot carry a moment: said **NOT ON THE SAFE SIDE**
("a pin, the moment not carried") when the moment over the group's
largest radius (three bolt diameters for one bolt) is 1 % or more of the
force.

**Forces square to the plies** (a member's force out of the plate's
plane, a welded plate's other plates, a frame member's moment bending the
part out of its plane) pull on the bolts: checked since 2026-11-19
([bolts in tension, with prying](#bolts-in-tension-with-prying)). A code
plugin without the `joint-check-groups-tension` export: **bolts in tension
not checked**, said **NOT ON THE SAFE SIDE** when their force, the moment
over the lever above added, is 1 % or more of the force in the plane.

### One physical bolt

Bolts of different parts within a quarter of the smaller diameter (at
least 2 mm) of each other are **one bolt**: their forces added as
vectors, the same case. Such a bolt is checked on **one shear plane**
under the sum (**on the safe side**: the plane next to the member they
bolt onto carries the sum of what is on its far side) and with the
smaller diameter, the lower strengths (said). A bolt of one part keeps
its own planes.

## Bolts in tension, with prying

Since 2026-11-19 (the owner's decision of that day; before, said NOT ON
THE SAFE SIDE).

**Each bolt's pull.** The force of a member end (a plate's holding bolts:
the resultant on the plate) square to the plies, \\( N \\), and its moment
bending them out of their plane, \\( \mathbf M \\) (about the bolts'
centroid, the in-plane levers as above), are shared by the bolts
elastically about their centroid — the plies rigid, the bolts equally
stiff in tension:

\\[ t_k = \frac{N}{n} + \mathbf s\cdot\mathbf r_k,\qquad
\Big(\sum_j \mathbf r_j\,\mathbf r_j^{\mathsf T}\Big)\,\mathbf s = \hat{\mathbf n}\times\mathbf M \\]

(`ohl_structures_fe::bolt_groups::tension`). **No contact of the plies is
taken** — **on the safe side**: where the plies bear on each other beyond
the bolts, the levers are longer and the bolts' pulls lower. **Which side
of the plies each ply lies on is not used**: each bolt's share is taken as
pulling, its size whatever its sign — **on the safe side** (a share that
presses the plies together would be carried by their contact). Several
parts' pulls on one physical bolt are added. A moment about a line every
bolt lies on (one bolt, one row about the row) cannot be carried by them in
tension: said **NOT ON THE SAFE SIDE** ("… is not carried by them in
tension") when it is 1 % or more of the force over the group's lever.

**Prying.** Both codes ask for it — ASCE 10-97 4.3.3 ("any tensile stress
resulting from prying action"), SIA 263 6.2.2.4 with its Figure 26 — and
neither gives a method. OpenOHL uses the T-stub model of Struik and de Back
(Kulak et al., Eqs. 17.8–17.12). A bolt pulling a flange (an angle's leg:
its web the angle's other leg) with the force \\( T \\) carries

\\[ B = T\Big(1 + \frac{\delta\alpha}{1+\delta\alpha}\,\frac{b'}{a'}\Big),
\qquad a' = a + \tfrac d2,\quad b' = b - \tfrac d2,\quad a \le 1.25\,b \\]

with \\( a \\) from the bolt to the flange's free edge (its toe), \\( b \\) to
the web's face (the gauge less the angle's thickness), \\( \delta \\) the net
over the gross area of the flange at the bolts and \\( \alpha \le 1 \\) the
ratio of the flange's moments at the bolts and at the web. OpenOHL takes
**α = δ = 1** — the largest the model gives (B rises with both): **on the
safe side** — so \\( Q = T\,b'/(2a') \\). The guide finds the model already
overestimates the prying of its tests (its Figs. 17.16 and 17.18). Each
flange the bolt pulls is taken (the lapped member's own leg and the leg it
bolts onto; a plate's holding bolts: the leg's flange) and **the largest
prying governs** (the guide, 17.6.1: the least stiff part governs).

**The flange's bending** (the model's mechanism, Eq. 17.12 with α = 1):
\\( T\,b' \le (1+\delta)\,w\,t^2 f_y/4 \\), \\( \delta = 1 - d_0/w \\), with
\\( w \\) the flange's length along the bolts the bolt takes: to the middle
between it and the next bolt each way, at most to the flange's end and at
most \\( b' \\) each way (a lone bolt's spread taken no further — **on the
safe side**: the yield lines round a lone bolt reach further).

Then each code's rules (below): the bolt's tension under \\( T + Q \\) and its
shear and tension together.

### A gusset plate's own prying and bending

Since 2026-11-19 (the owner's decision of that day: "modelled with the
same T-stub method already used for angle flanges"; before, said NOT ON
THE SAFE SIDE). A plate's holding bolts pulled out of its plane bend the
plate too. The plate is taken as the T-stub's flange **without a web**:
the pull reaches it through the bolts of the members bolted onto it,
which, in a line, hold it against no turning — so the model's moment at
the web is taken as **none** and the whole moment of the pull is carried
at the bolt line (Kulak et al. Eq. 17.8 with no moment at the web; their
Eq. 17.11 with \\( \delta\alpha/(1+\delta\alpha) \\) at its largest, 1):

\\[ Q = T\,\frac{b'}{a'},\qquad B = T\Big(1 + \frac{b'}{a'}\Big),\qquad
T\,b' \le \delta\,\frac{w\,t^2 f_y}{4},\quad \delta = 1 - \frac{d_0}{w} \\]

with \\( a' = a + d/2 \\), \\( b' = b - d/2 \\), \\( a \le 1.25\,b \\) as for a
flange — twice the prying of a flange with α = δ = 1, and the bending
checked at the bolt line on the net section alone: **on the safe side**
(a member's bolts that do restrain the plate would share the moment). Its
dimensions, per holding bolt (`ohl_structure_link::joint_groups::plate_tee`):

- **b**, in the plate's plane square to the holding bolts' line (the
  leg's): from the bolt to the **farthest** bolt of a member on the plate —
  **on the safe side**: the prying and the plate's bending grow with it;
- **a**, from the bolt the other way to where the plate stops bearing on
  the leg under it: the plate's outline or the leg's flange's toe or heel
  (past them nothing is under the plate to pry against), the nearer —
  **on the safe side**;
- **w**, along the line: to the middle between it and the next holding
  bolt on its line each way, at most to the outline and to \\( b' \\) each
  way (a lone bolt's spread taken no further, **on the safe side**, as for
  a flange).

The bolt's prying is the largest of its flanges' and plates' (the least
stiff part governs, the guide 17.6.1); each plate's bending is a check of
its own ("bending of plate G1 under the bolt's pull …"). Where the
plate's geometry is not known — no outline, no member's bolt on it placed,
the bolt off the outline — its prying is said not worked out, NOT ON THE
SAFE SIDE, as before; so it is with a code plugin without the
`joint-check-groups-prying` export.

### Bolt strengths: a grade set by command

A bolt's tension needs its ultimate strength F_u (or its library's
`tension_capacity`, **its ultimate tensile load F_u A_s** — owner
2026-11-19; not a design value). Bolts imported from a tower program's
export often have neither: their tension is then not checked, said NOT ON
THE SAFE SIDE. Since 2026-11-19 (the owner's decision: "add a command to
set it") OHL-Structure's Design › Bolt grade… gives the bolts of chosen
members a grade, its strengths from **SIA 263:2013, 3.4.2, Table 2** (the
strength classes of SN EN 20898-2; 8.8: f_yb 640, f_ub 800 N/mm²), written
into the structure file with its source and "set by command"
([user guide](../user-guide/bolt-grade.md)). The grade's f_ub and name
then go over the bolt's library or design data; a capacity given for
another grade is dropped (the grade's worked out by the code, said); each
group's notes say the grade, who set it and where its values come from.
**No grade is taken by itself.**

**Not covered, said NOT ON THE SAFE SIDE** in the group's result:

- a ply whose bending is not known — a member that is not an angle, a
  flange with bolts on more than two gauge lines (the outer lines are not
  effective in tension without stiffening, the guide 17.6.1), a gusset
  plate whose outline or members' bolts are not known (since 2026-11-19 a
  plate is otherwise worked out, [above](#a-gusset-plates-own-prying-and-bending))
  — "the prying of … not worked out", where the pull is 1 % or more of the
  bolt's shear;
- a bolt within half a diameter of the web (\\( b' \le 0 \\)): the local
  prying of its head (the guide's Fig. 17.19) is not covered;
- a flange without a yield strength: its bending not checked;
- a bolt without a tension capacity or an ultimate strength: its tension
  not checked.

**Not checked, said in the notes**: fatigue (the guide 17.6.2; SIA 263
6.2.2.3 asks for preloaded high-strength bolts where the loads are not
essentially static — the loads' character is the engineer's); SIA 263
6.2.1.4 (countersunk bolts, thin nuts: 25 % less) and 6.2.2.7 (machined
threads: 0.85) — the bolts' kind is not in the data.

## A joint that does not fit

Owner 2026-11-19: a joint whose details do not fit as given reads **"does
not fit"** — coloured as failing, counted apart in the tables, the report
and the 3D view, **never an infinite usage** (before, a distance of 0 gave
one). A group does not fit when:

- a bolt's hole is not within the flange it bolts onto (from the heel,
  its centre less half the hole below 0, or its centre and half the hole
  beyond the toe; 1 mm allowed either way);
- a holding bolt lies outside the plate's outline;
- two holes in one ply overlap (centres nearer than the holes' radii
  together, and not one hole);
- a bolt lies beyond the end of the member's steel at the joint (where
  its `cut` is given or worked out).

Such a group is not sent to the code's plugin; its row says why
("DOES NOT FIT as given: …"). The joint's other parts keep their usages
(Joint details: "Usage of its parts that fit"); the joint is failing
whatever they give.

**Only where the placements were assumed** (the owner's decision of
2026-11-19, D14): where every misfit of a group lies on member ends whose
placement was **assumed** — not given by the user, not laid out by the
joint layout (no `onto`, gauges, pitch, end distance or `cut` given at the
end or in the member's design data: the joint details worked them out or
assumed them, e.g. an import's bolts set from the work point at half the
leg) — the row reads **"DOES NOT FIT as assumed — not detailed: …"**. It
is still failing, never a usage or a pass, and is **counted apart** from a
joint that does not fit as given: in the run's summary ("joints that do
not fit: N as given, M as assumed — not detailed"), on its own row of the
report's joints table with its own count, and in the 3D view in a colour
of its own (orange; red: as given). A misfit on a placement given — or a
plate's bolts, a plate being in the file — reads "as given". On the 78
real towers of the investigation, about two thirds of the joints that do
not fit with the layout, and nearly all without it, are only assumed
([measured](#measured-the-joints-that-do-not-fit-as-assumed-2026-11-19)).

### Measured: the joints that do not fit as assumed (2026-11-19)

On the 78 tower files of a private real line (kept privately), one case:
as imported (no joint given — the bolts placed as the joint details assume
them) 23 128 of 33 544 joints do not fit, 22 993 of them only as assumed,
135 as given; after Orient angles and Lay out joints, 17 821 of 55 970,
11 057 as assumed — not detailed and 6 764 as given (most of these the
layout's own: pieces in line it cuts back with a member bolted onto them,
the mutual hosts — the owner's decisions D15 and D16 of 2026-11-19 take
them with J9).

## The distances along a bolt's force

Each bolt pushes the ply it bears on: the member it bolts onto **along**
the force on that member, the plate a holding bolt holds **against** it.
From the bolt in that direction:

- **to the edge** (\\( e_1 \\), the end distance along the force): in a
  plate, to its outline (a bolt outside it: 0); in an angle's flange, to
  its **toe** (from the bolt's gauge, the flange's width and the
  direction's part across it) and, where the member's steel stops at the
  joint, to its **end** (its `cut` from the work point); the heel is no
  edge (the other leg runs on). None: no edge that way (the member runs
  on);
- **square to it** (\\( e_2 \\), the edge distance), to the nearer edge
  either way;
- **to the next hole** (\\( p_1 \\)) in the same ply along it, a hole
  in front within one bolt diameter of the bolt's line, centre to centre.

**Not known**: where the member's steel stops is not given (a member
ending at the joint, its `cut` assumed) or the flange not found (not an
angle) — each code then takes its conservative case (below), said.

## ASCE 10-97

| Check | Rule | Clause |
|---|---|---|
| bolt shear | design shear of the bolt on its planes (a given capacity, else 0.62 F_u on the root area taken as 0.6 of the gross) | 4.3.2 |
| bearing on the ply | force on the bolt ≤ 1.5 F_u d t, F_u the lower of the ply's and the bolt's | 4.4 |
| end distance along the force | e ≥ max(1.2 P/(F_u t), 1.3 d, t + d/2) (redundant member: max(t + d/2, 1.2 d)), P the bolt's force | 4.5.1, Eqs. 4.5-1 to 4.5-4 |
| spacing to the next hole along the force | s ≥ 1.2 P/(F_u t) + 0.6 d | 4.5.2, Eq. 4.5-5 |
| edge distance square to it | f ≥ 0.85 e_min (an angle's rolled toe), + 1/16 in. (a plate's sheared edge) | 4.5.3, Eqs. 4.5-6, 4.5-7 |
| bolt tension (2026-11-19) | the pull with its prying ≤ F_t A_s, F_t = 0.6 F_u (case (b): the ASTM proof-load stress of case (a) is not in the data); A_s taken as 0.6 of the gross area; a library's tension capacity (its F_u A_s): 0.6 of it | 4.3.3 |
| shear and tension together | (f_t/F_t)² + (f_v/F_v)² ≤ 1 (Eq. 4.3-2, F_t(v) = F_t [1 − (f_v/F_v)²]^½, squared), its square root as the usage | 4.3.4, Eq. 4.3-2 |
| the flange's bending under the pull | T b′ ≤ (1 + δ) w t² F_y / 4 (the prying model's mechanism; no rule in the standard) | — |
| a gusset plate's bending under the pull (2026-11-19) | T b′ ≤ δ w t² F_y / 4 (a flange without a web: the moment at the bolt line; no rule in the standard) | — |

- 4.5.1 measures the end distance "from the center of a hole to the end,
  whether this end is perpendicular or inclined to the line of force":
  taken **along the bolt's force** to the ply's edge.
- **Not known** where the member's steel stops: the end distance is not
  checked — said **NOT ON THE SAFE SIDE** (the standard's bearing takes no
  distance).
- No bolt strength given (neither a shear capacity nor F_u): the bolt's
  shear not checked, said NOT ON THE SAFE SIDE; the bearing then on the
  ply's F_u alone (said).
- **Bolts in tension**: 4.3.3's F_t = 0.6 F_u (case (b)) for every bolt —
  C4.3.3 calls it "a conservative estimate" for bolts without a proof
  load; for bolts with one (case (a)) **on the safe side**. The stress area
  of Eq. 4.3-1, \\( A_s = \frac\pi4 (d - 0.974/n)^2 \\), needs the threads
  per length \\( n \\), not in the data: \\( A_s \\) taken as 0.6 of the gross
  area — **on the safe side** for any thread with \\( n\,d \ge 4.32 \\) (a
  pitch up to 0.23 d, every coarse thread). 4.3.4 asks for the combined
  stresses "at the same cross-section": the shear's ratio as the shear
  check takes it (on its planes) with the tension's — **on the safe side**
  (the two sections may differ). The flange's bending has no rule in
  the standard: F_y without a factor, as its other members' capacities.

## SIA 263:2013

Design values with \\( \gamma_{M2} = 1.25 \\) (4.1.3; the structure's
parameter file for the plugin may change it).

| Check | Rule | Clause |
|---|---|---|
| bolt shear | per plane \\( 0.6 f_{ub} A_s/\gamma_{M2} \\) (4.6, 5.6, 8.8) or 0.5 (4.8, 5.8, 6.8, 10.9; a grade not known: 0.5, said), a given capacity over γ_M2 | Table 16, (73), (74), (75) |
| bearing on the ply | \\( F_{b,Rd} = \min(0.85\,e_1/d_0,\ 2.4)\, f_u\, d\, t / \gamma_{M2} \\), \\( e_1 \\) along the bolt's force (no edge: 2.4); a hole ahead nearer than \\( e_1 + d_0/2 \\): \\( e_1 = p_1 - d_0/2 \\) | Table 16, (76) |
| end, spacing and edge distances | \\( e_1 \\), \\( p_1 \\) along the force and \\( e_2 \\) square to it ≥ Table 17's minimums | Table 17, Figure 31 |
| bolt tension (2026-11-19) | the pull with its prying ≤ \\( F_{t,Rd} = 0.9 f_{ub} A_s/\gamma_{M2} \\), \\( A_s \\) taken as 0.6 of the gross area; a library's tension capacity (its \\( f_{ub} A_s \\)): 0.9 of it over \\( \gamma_{M2} \\) | Table 16, (77); 6.2.2.4 (prying) |
| shear and tension together | \\( (F_{v,Ed}/F_{v,Rd})^2 + (F_{t,Ed}/F_{t,Rd})^2 \le 1 \\), its square root as the usage | 6.2.2.6, (78) |
| the flange's bending under the pull | \\( T b' \le (1+\delta) w t^2 f_y/(4\gamma_{M1}) \\) (the prying model's mechanism; SIA 263 gives no rule) | — |
| a gusset plate's bending under the pull (2026-11-19) | \\( T b' \le \delta w t^2 f_y/(4\gamma_{M1}) \\) (a flange without a web: the moment at the bolt line; SIA 263 gives no rule) | — |

- Figure 31 names the distances by the force's direction and says
  \\( e_2 \\) and \\( p_2 \\) apply also oblique to it ("Les désignations
  e2 et p2 s'appliquent également aux entraxes obliques par rapport à la
  direction de l'effort"): taken along and square to each bolt's own
  force.
- (76) asks \\( e_2 \ge d_0 \\) and \\( p_2 \ge 2 d_0 \\): where a bolt's
  \\( e_2 \\) is smaller it is said.
- **Bolts in tension**: \\( A_s = \pi d_s^2/4 \\), \\( d_s = 0.5(d_f + d_k) \\)
  (Table 16) needs the threads' diameters, not in the data: \\( A_s \\) taken
  as 0.6 of the gross area — **on the safe side** for every coarse thread
  (as the shear of (73)–(74) takes it). The flange's bending with
  \\( \gamma_{M1} \\) (4.1.3; the plugin's parameter file may change it).
- **Not known** where the member's steel stops: \\( e_1 \\) the smaller of
  the one found and Table 17's minimum (the ply may end as near the bolt
  as the code allows — **on the safe side**, said; as the member check
  does for the leg a member bolts onto, [laps](joint-laps-splices.md)); a
  bolt Table 17 has no row for: said NOT ON THE SAFE SIDE.

## What is not checked, said

Never silently skipped — each a row of the joint without a usage,
"NOT CHECKED, NOT ON THE SAFE SIDE: …":

- **a plate the joint's leg does not bolt onto**: how it is held is not
  given, so neither the bolts holding it nor the leg's bearing under them
  are checked. A leg running on through the joint (`kind =
  "continuous"`) has no bolts at that end: give the plate's own bolts onto
  it (`[plate.leg_bolts]`, structure file version 20, 2026-11-19);
- a holding leg whose bolts on the plate are not placed or not known;
- a member others bolt onto whose thickness is not known;
- (2026-11-22, L14) ends laid onto a member that ends at the joint and
  bolts onto nothing there (or is not bolted), or whose own end bolts are
  not placed: where their forces go is not given.

And, without a usage, "NOT ON THE SAFE SIDE: …" (2026-11-22, L14): a
member's own steel at its end bolts (its net section, its tearing out) and,
onto a plate, the plate's own checks under them, where ends are laid onto
that member near its end — checked under its own force only (its end
bolts carry the others' forces, above).

The simple check checks no joint group (not a design code, said); a
design code whose plugin was not run or has no `joint-check-groups`
export: every group NOT CHECKED, never another code's check.

**Still assumed or not checked** (2026-11-19), each said in the results
and above: a member's own gauge eccentricity in its plane not taken as a
moment on its bolts (ASCE 10-97 4.1; SIA 263 6.2.4.6 gives no rule — not
on the safe side); the plies' offsets through their thickness left out of
the levers; the joint's own loads not routed through its plates; the
holding bolts of a welded plate's several plates each taken carrying the
whole (on the safe side); a plate's prying and bending under bolts in
tension worked out since 2026-11-19 as a flange without a web (on the safe
side; said not worked out only where its geometry is not known); the
moments shared by
the elastic method (on the safe side; owner 2026-11-19: the elastic method
stays, the instantaneous centre not used).

## Validation

Unit tests on invented data, worked by hand:

- `crates/ohl-structures-fe/src/bolt_groups.rs`
  `an_eccentric_group_shares_its_moment`: four bolts on an 80 × 60 mm
  rectangle, 100 kN acting 150 mm beside their centroid: M = 15 kN m,
  Σr² = 0.01 m², the most loaded bolts (45, −85) kN, 96.18 kN; the others
  57.01 kN; the bolts' forces sum to the load and their moments to M.
- `apps/structure/ohl-structure-link/src/joint_groups.rs`
  `a_plates_holding_bolts_carry_the_resultant_and_its_moment`: two
  diagonals on one plate onto a leg, 60 kN each at 45°: (0, 84.85) kN on
  the plate; holding bolts whose centroid the lines pass through 42.43 kN
  each; moved 40 mm aside, M = 3.394 kN m, each bolt 54.33 kN.
  `a_shared_bolt_on_the_leg_carries_both_members`: a redundant and a
  diagonal through one hole of the leg: (12, 16) + (−10, 0) = (2, 16) kN;
  the distances along and square to each bolt's force to the leg's toe
  (51.7 and 38.75 mm) and to the next hole (50 mm centre to centre).
- `crates/ohl-structures-fe/src/bolt_groups.rs`
  `bolts_share_a_pull_and_a_moment_out_of_their_plane`: four bolts on an
  80 × 60 mm rectangle, 40 kN square to the plane (10 kN each) and 1.2 kN
  m about the 60 mm side's axis: Σx² = 0.0064 m², ∓7.5 kN, together 2.5
  and 17.5 kN; a row of bolts about the row: all of the moment not
  carried.
- `plugins/codes/asce10/src/groups.rs` `a_bolt_in_tension_with_prying`: an
  M16 (F_u 800 MPa) pulled by 20 kN through a 60 × 6 mm leg, a = 30, b =
  24 mm: a′ = 38, b′ = 16 mm, Q = 4.21 kN; F_t A_s = 0.6 × 800 × 0.6 ×
  201.06 mm² = 57.91 kN, 41.8 %; the flange (w 32 mm, hole 17.5 mm, δ
  0.453) 148.6 N m against T b′ = 320 N m, 215 %; with 30 kN of shear
  (59.84 kN) Eq. 4.3-2's root 0.653. `plugins/codes/sia263/src/groups.rs`
  `a_bolt_in_tension_with_prying`: the same by (77), 69.49 kN, and (78).
- `plugins/codes/asce10/src/groups.rs` `a_plates_prying_without_a_web`
  (2026-11-19): the M16 pulled by 20 kN through a 10 mm plate (S355), a =
  40, b = 60 mm, w 50 mm: a′ = 48, b′ = 52 mm, Q = 20 × 52 / 48 = 21.67 kN
  (twice a flange's), 41.67 kN against 57.91 kN, 72.0 %; δ = 1 − 17.5/50 =
  0.65, δ w t² F_y / 4 = 288.4 N m against T b′ = 1040 N m, 360.6 %.
  `plugins/codes/sia263/src/groups.rs` `a_plates_prying_without_a_web`:
  the same by (77) and with γ_M1, d₀ 18 mm.
- `apps/structure/ohl-structure-link/src/joint_groups.rs`
  `a_plate_bent_by_a_holding_bolt` (2026-11-19): a plate in z = 0 from
  (−100, −50) to (100, 150) mm, holding bolts at x = ∓40 mm on the leg's
  line, the leg's flange 60 mm wide (the bolts 30 mm from its heel, its toe
  towards −y), the members' bolts at (0, 60) and (20, 90) mm: b = 90 mm
  (the farthest), a = 30 mm (the toe nearer than the outline's 50 mm), w =
  40 + 60 = 100 mm.
- `plugins/codes/asce10/src/groups.rs` `a_shared_bolt_on_the_leg`: 60 kN
  on an M16 8.8 in an 8 mm leg: shear 59.84 kN (one plane), bearing
  97.92 kN, end distance 20.8 mm needed; `plugins/codes/sia263/src/groups.rs`
  `bearing_along_the_bolts_force`: (76) with e₁ 30 mm along the force
  73.98 kN, no edge 125.34 kN, the end not known 61.65 kN (Table 17's 25
  mm).
- End to end with the bundled plugins
  (`apps/structure/ohl-structure-link/src/joint_groups_tests.rs`): a
  diagonal lapped onto the leg gives the leg's bearing 20 kN / (1.5 F_u ×
  16 × 7 mm²) per bolt (ASCE 10-97 4.4), by SIA 263 its (76); a plate the
  leg does not bolt onto is said not checked; a plate the leg bolts onto
  gives its holding bolts and the leg's bearing under them; a plate on a
  leg running on held by its own bolts onto it (`[plate.leg_bolts]`) the
  same; overlapping holes "does not fit", without a usage; a diagonal
  pulled out of the plate's plane checked in tension by both codes, the
  plate's own bending governing (b′ 42 mm), its prying no longer said not
  worked out; bolts of a library bolt without strengths not checked in
  tension (said), checked once SIA 263 Table 2's 8.8 is set on the
  members' bolts by command, the group's notes saying so
  (`a_grade_set_by_command_checks_the_bolts_in_tension`).
