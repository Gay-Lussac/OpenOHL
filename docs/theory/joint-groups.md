# Joint bolt groups and the leg's bearing

Implemented in `apps/structure/ohl-structure-link/src/joint_groups.rs`
(the groups gathered per joint, their forces per case), with the
mechanics of `crates/ohl-structures-fe/src/bolt_groups.rs` (`elastic`,
`Plane`) and each bolt's checks in the design codes' bundled plugins —
ASCE 10-97 in `plugins/codes/asce10/src/groups.rs`, SIA 263:2013 in
`plugins/codes/sia263/src/groups.rs` — reached through the plugin
interface `joint-check-groups` (2026-11-19; one call with every group and
case), run with every case of OHL-Structure's analysis (Results › Run)
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
commentary cites as Kulak et al. 1988). Every assumption below is marked
**on the safe side** (conservative) or **not on the safe side**.

## What is checked

The [plate checks](joint-plates.md) look at each member's bolts in a
plate, and the [member checks](asce10.md) at each member end's own bolts,
each under that member's own force. Where **several members bolt onto
the same part** at a joint, its bolts carry their forces **together**:

- **A plate's bolts onto the leg holding it** (`holding`): a gusset
  plate (or a welded plate: plates welded together) that the joint's leg
  bolts onto and other members bolt onto too. The leg's bolts on it carry
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

Each bolt is checked; a group's usage in a case is its most loaded
bolt's largest check, named with the bolt (its place from the group's
centroid, what it carries, its force) and the check's clause.

**The joint's usage** is the largest of its plates', welds', bolt groups'
and bearing checks over the cases: shown in Joint details ("Usage of the
joint"), coloured in the 3D view (the joints' dots, as the members) and
listed per joint in the structure report. A part not checked is counted
beside it, never taken as a pass.

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
  else the steep ones) bolted onto the plate, with the members running on
  in line with it through the joint (`continuous`, or in line within 5°
  of the same section with no splice of the two there — **an
  assumption**, said: two pieces in line that are not one would have a
  splice in the file). Its own force does not pass through the plate when
  it runs on; where it ends at the plate its force is the others'
  resultant (equilibrium), the same. Where each piece of a leg spliced on
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
part out of its plane) pull on the bolts: **bolts in tension not
checked** (ASCE 10-97 4.3.3, 4.3.4; SIA 263 (77), (78)) — said **NOT ON
THE SAFE SIDE** when their force, the moment over the lever above added,
is 1 % or more of the force in the plane.

### One physical bolt

Bolts of different parts within a quarter of the smaller diameter (at
least 2 mm) of each other are **one bolt**: their forces added as
vectors, the same case. Such a bolt is checked on **one shear plane**
under the sum (**on the safe side**: the plane next to the member they
bolt onto carries the sum of what is on its far side) and with the
smaller diameter, the lower strengths (said). A bolt of one part keeps
its own planes.

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

- 4.5.1 measures the end distance "from the center of a hole to the end,
  whether this end is perpendicular or inclined to the line of force":
  taken **along the bolt's force** to the ply's edge.
- **Not known** where the member's steel stops: the end distance is not
  checked — said **NOT ON THE SAFE SIDE** (the standard's bearing takes no
  distance).
- No bolt strength given (neither a shear capacity nor F_u): the bolt's
  shear not checked, said NOT ON THE SAFE SIDE; the bearing then on the
  ply's F_u alone (said).

## SIA 263:2013

Design values with \\( \gamma_{M2} = 1.25 \\) (4.1.3; the structure's
parameter file for the plugin may change it).

| Check | Rule | Clause |
|---|---|---|
| bolt shear | per plane \\( 0.6 f_{ub} A_s/\gamma_{M2} \\) (4.6, 5.6, 8.8) or 0.5 (4.8, 5.8, 6.8, 10.9; a grade not known: 0.5, said), a given capacity over γ_M2 | Table 16, (73), (74), (75) |
| bearing on the ply | \\( F_{b,Rd} = \min(0.85\,e_1/d_0,\ 2.4)\, f_u\, d\, t / \gamma_{M2} \\), \\( e_1 \\) along the bolt's force (no edge: 2.4); a hole ahead nearer than \\( e_1 + d_0/2 \\): \\( e_1 = p_1 - d_0/2 \\) | Table 16, (76) |
| end, spacing and edge distances | \\( e_1 \\), \\( p_1 \\) along the force and \\( e_2 \\) square to it ≥ Table 17's minimums | Table 17, Figure 31 |

- Figure 31 names the distances by the force's direction and says
  \\( e_2 \\) and \\( p_2 \\) apply also oblique to it ("Les désignations
  e2 et p2 s'appliquent également aux entraxes obliques par rapport à la
  direction de l'effort"): taken along and square to each bolt's own
  force.
- (76) asks \\( e_2 \ge d_0 \\) and \\( p_2 \ge 2 d_0 \\): where a bolt's
  \\( e_2 \\) is smaller it is said.
- **Not known** where the member's steel stops: \\( e_1 \\) the smaller of
  the one found and Table 17's minimum (the ply may end as near the bolt
  as the code allows — **on the safe side**, said; as the member check
  does for the leg a member bolts onto, [laps](joint-laps-splices.md)); a
  bolt Table 17 has no row for: said NOT ON THE SAFE SIDE.

## What is not checked, said

Never silently skipped — each a row of the joint without a usage,
"NOT CHECKED, NOT ON THE SAFE SIDE: …":

- **a plate the joint's leg does not bolt onto**: how it is held is not
  given (a leg running on through the joint has no bolts at that end in
  the file, `kind = "continuous"`), so neither the bolts holding it nor
  the leg's bearing under them are checked. The file format has no
  plate's own bolts onto the leg (an open question to the owner);
- a holding leg whose bolts on the plate are not placed or not known;
- a member others bolt onto whose thickness is not known.

The simple check checks no joint group (not a design code, said); a
design code whose plugin was not run or has no `joint-check-groups`
export: every group NOT CHECKED, never another code's check.

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
  gives its holding bolts and the leg's bearing under them.
