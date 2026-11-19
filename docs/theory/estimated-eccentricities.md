# Estimated eccentricities

*Built 2026-11-19, in active development, not approved yet* (roadmap
O45, steps A1–A3; owner's decisions of 2026-11-19). How the option
**Estimated eccentricities** (OHL-Structure: Results; [user
guide](../user-guide/structure-analysis.md)) gives a level 3 lattice tower
the sideways shifts of its member ends that the [joint
layout](joint-layout.md) would make, and how the analysis takes them. The
code: `ohl_structure_link::eccentricities` (the estimates and the
offsets), `layout_run::plan_relaxed` (the planner run relaxed), `run`
(the solves and their envelope).

## Why

Three offsets meet at a tower joint. An angle bolted by one leg lies out
of the plane of the member it bolts onto: the codes' rules for single
angles cover that (the slenderness curves of ASCE 10-97 include the
normal framing eccentricity), and it is not in the finite-element model.
A member end moved **sideways** so that it fits (the connection's
`shift`) passes its force beside the joint's work point: the analysis
takes it as a rigid end offset ([Eccentric member
ends](structures-fe.md#eccentric-member-ends-rigid-end-offsets)) — but
only at level 4 with the joint details taken, and on a real tower most
joints are not detailed. A welded plate in the member's own plane (the
layout's step 4) moves the member's axis farther out of the flange.

At level 3 the members run to the work points: none of this is in the
analysis. The option puts the sideways shifts in — **estimated**, from
what the layout would do — without detailing the joints.

## The estimates (A1)

The layout's planner is run **relaxed** on the file as it is: the same
planning as *Lay out joints* — the automatic values taken out, the
members turned flat where that helps (each turn kept only where a joint
is planned with it), each joint searched ([J1](joint-layout.md#the-search-per-joint-j1)),
the place of a member left kept ([J2](joint-layout.md#members-left-for-a-hand-detail-keep-their-place-j2)) —
and **nothing written**: no clash rounds after, the file unchanged. Every
member end the planner takes (a single angle bolting by one leg onto a
member at the joint, not in line with it) gets one value:

| Source | When | Its shift |
|---|---|---|
| **laid out** | the joint layout laid it out (its record lists the end) | the `shift` as written (none: 0) |
| **given** | a `shift` typed at the end | as given |
| **estimated** | the planner places it | the shift it plans (none: 0), kept even where a run would not write the joint (a clash after, the joint left whole); on a plate in its own plane (step 4) also the plan's offset out of the flange; on a bent plate (J2b, 2026-11-19: an end over the heel, its flat folded round the other leg) the plan's offset, 0 — it lies where it lay on the flange's inner face ([bent plate](joint-layout.md#a-bent-plate-round-the-other-leg-j2b)) — so such an end, before an upper bound (the shift limit), is now estimated |
| **upper bound** | no layout fits the end | the **shift limit** (the record's, 30 mm to start with) and, where a plate in its own plane would be needed (its bolted leg lies off the flange: the plate case), that plate's offset out of the flange at step 4's limit — the end's leg thickness |

A shift typed anywhere else (an end the planner does not take) is taken
as given. The ends the planner does not take — in line with another
member (a splice, one part), not a single angle, onto a plate or splice
given by hand — get none: as at level 4, their offset is not a sideways
shift.

The bolts of an end without its own are those of its class's connection
preset (the presets beside the structure, named in the layout's record),
else the bolt picked (the record's; without a record the layout window's
starting bolt, 16 mm). The bolts worked out from the forces are a later
step (O45 B).

**Kept apart from the fabrication record**: the estimates are never
written into the file, never shown as a laid-out joint, never counted as
a joint detail; they are made again from the file at every run, so they
follow every edit. The same file, libraries and presets give the same
estimates (the planner is deterministic).

## In the analysis (A2)

Each end with a shift or a plate's offset becomes the member's rigid end
offset, exactly as a connection's `shift` at level 4:

- **r = s · u + p · n**: s the shift, u its direction — across the member
  towards what it bolts onto (`ohl_structures::connections::shift_direction`):
  for an estimate the member the planner bolts it onto (on a plate, the
  member the plate lies on), for a value in the file as the level 4
  analysis measures it (the member it names, else the joint's leg); an
  upper bound towards what it would bolt onto (the member named, else the
  one running through the joint), else the joint's leg — p the plate's
  offset, n out of the face (u × the member's direction). An end with no
  direction to measure in (nothing at the joint, or in line) is left out:
  **NOT ON THE SAFE SIDE**, said with the ends.
- In the solve the end is tied rigidly to the joint; where a frame meets
  the joint the frames carry the moment, where only bars meet it the
  member's own check takes the whole moment N·e, uniform along it (on the
  safe side for it, [as at level 4](structures-fe.md#eccentric-member-ends-rigid-end-offsets)).

**The signs not known.** An upper bound's sign is not known (a hand
detail may move the end either way), nor is the side of a plate's offset
(the estimates do not place the plate). The cases are therefore solved
**three times** — without the estimates, with them (the upper bounds and
plates' offsets one way), with the upper bounds and plates' offsets the
other way — and each member, plate and weld takes its **largest usage**
of the solves (its forces and what governs from that solve, said in what
governs). With every sign known (no upper bound, no plate's offset) two
solves.

*Conservative assumptions:*

- *the solve without the estimates is in the envelope* — an estimate may
  not be built as planned (the joint detailed otherwise, concentric): a
  member never gets less than without the option (on the safe side);
- *the upper bound is the shift limit* (and the plate case's offset at
  step 4's limit): the most the layout itself allows; the owner's bound
  of 2026-11-19. **A hand detail that shifts an end farther is not
  covered** — its own value is taken once it is written at the end
  (given, or the joint details at level 4);
- *a plate's offset out of the face* taken with both signs, square to the
  shift (its own direction is known only once the plate is placed).

**NOT ON THE SAFE SIDE (said where it applies):**

- *two or more ends whose sign is not known at one joint where a frame
  meets it*: their moments are shared by the frames in the solve, and the
  envelope takes their signs all one way and all the other, not mixed —
  a mix may be worse for the frames. The results name such joints
  ("NOT ON THE SAFE SIDE: at N joint(s) where a frame meets two or more
  ends whose offset's sign is not known …"). Exact: every mix of signs (2ⁿ
  solves), or the moments of those ends taken by the members alone as at
  a joint of bars (a change of the core's solve: asked, not built).
- *an extension's own members* (a leg extension's piece): the layout plans
  the body's joints only; their ends stay at the work points, as without
  the option — said.

Not in this step, and so not in the analysis (as without the option):
the out-of-plane seat offset of every end on a gusset plate (one plate's
thickness farther out than on the flange; O45 D, the seat offsets as
rigid end offsets, measured once as an option); the bolts worked out from
the forces (O45 B).

The option is ignored where the joint details are taken (level 4: the
connections' own shifts) and on a model file; said.

## The results

The run's notes say how many ends have an offset and from where — *laid
out*, *given*, *estimated from its plan*, **upper bound at N end(s)** —,
the shift limit, how many upper bounds carry a plate's offset, the solves
and the time the estimates took. Each case's note on eccentric member
ends reads *(the estimated eccentricities)*. A member with an end of them
has its eccentric words followed by the sources, e.g. `[estimated at N2;
upper bound at N7]`; where a solve other than the one with the estimates
governs, what governs ends with *(without the estimated eccentricities)*
or *(with the upper bounds the other way)*.

## An example

The invented K-brace of the tests (`ohl-structure-tests`,
`estimated_eccentricities.rs`; the example sections, the simple check,
one case pushing the joint N2): two diagonals D1, D2 and a shallow
redundant R1 meet the leg at N2. The planner lays D1 and D2 out
concentric (*estimated*, shift 0); R1 runs within 1° of D1 and finds no
room on their plate: *upper bound*, 30 mm.

| | legs as trusses | legs as frames |
|---|---|---|
| R1's offset | 30 mm, only bars at N2: in R1's own check (0.08 kN m, uniform) | 30 mm, carried by the legs' frames (0.07 kN m) |
| R1's usage, off → on | 16.3 % → 20.0 % | 16.3 % → 16.3 % (the solve without the estimates governs) |
| D1's usage, off → on | 16.5 % → 16.5 % | 16.5 % → 18.1 % (the upper bound the other way governs) |
| the legs | unchanged | their moments rise; a single angle checked for its axial force only says what the bending would add, **not on the safe side** (as for any frame moment: [single angles' end moments](structures-fe.md)) |

The notes: *1 member end with an offset of 3 the layout's planner takes —
0 laid out, 0 given, 2 estimated from its plan, upper bound at 1 end (the
30 mm shift limit); … the largest of 3 solves*.

## Time

The estimates cost the layout's planning without its clash rounds — on
a lattice tower of a couple of thousand members a few seconds, about a
third of a whole *Lay out joints* — and the cases are solved two or three
times (a case of such a tower takes a fraction of a second). OHL-Structure
shows both, measured, after each run beside the option.

## Measured on real towers (A3, 2026-11-19)

Measured on the 78 tower files of a private real line, kept privately
(the numbers are the owner's; they decide whether the option is turned on
by default). What they showed, without numbers: nearly every end the
planner places is concentric, so the estimates' own shifts change little;
the effect comes from the **upper bounds** (about a quarter of the ends
on those towers) — usages rise a great deal, and on some towers the solve
with them does not settle (an end held eccentrically at a joint where a
single brace holds an angle running through: the offset lets the joint
turn, and an angle's small torsional stiffness does not stop it; the case
then fails, said). The level 4 offsets of the joints laid out there are
those the estimates take for them (every one compared). **Off by default**
until the owner decides (open questions in the roadmap, O45).
