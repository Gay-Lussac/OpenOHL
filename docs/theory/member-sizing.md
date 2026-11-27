# The member sizer

(2026-11-27, roadmap O49 stage 1; **built, in active development, not
approved yet**.) The owner asked on 2026-11-26: *"Would it maybe help the
joints to also automate the level 3? (choice of members) where there
would be a basic choice and then plugins can adapt the values based on
norms"*, then *"Okay go ahead with the builder"*. This page says how
OHL-Structure's member sizer chooses the sections of a
[level 3](../development/structure-detail.md) structure, how its loop
settles, and what it assumes. The window is
[Choose the members](../user-guide/member-sizing.md).

## What it answers

For each **member group** of the structure: the lightest section of the
project's section library, in the group's own family, that passes

1. **the design code's check**: every member of the group at most the
   target usage (100 % by default, never above) in every load case, by the
   code the structure names (`[analysis] capacity`: the bundled ASCE 10-97
   plugin by default, SIA 263:2013, or another plugin's method; the core's
   `simple` check only where it is chosen — then NOT ON THE SAFE SIDE,
   below);
2. **the code's limits on the members' sizes** that a usage does not hold
   — a compression member's slenderness, a leg's flat width to thickness,
   a member's least thickness — given by the code's plugin (the
   `member-limits` export, below);
3. **the joint-fit limits**: the member can still be bolted, by the rules
   Lay out joints uses (below).

The core makes this basic choice; the plugins adapt the values to their
code (the check, the limits, the bolt distances and the bolt's capacity).
The choice never lands on a member that cannot be connected where a
connectable one passes; where none does, the lightest passing is taken
and its joint-fit problems are said (a joint to detail by hand).

## Groups and families

- **A group** is the members of one `group` with one section (the
  structure file's `group` of each member; a group with several sections
  gives one sizing group per section, named "group · section"). Members
  without a group stay with their **mirror images** (`[[symmetry]]`) of
  the same section; alone otherwise. The designer may **split** a group:
  each of its members (with its mirror images) is then sized alone; or
  **keep** it as it is. No file field is added: a group stays the members
  naming it (decision of 2026-11-27).
- **A family** is the sections of the project's section library with the
  same shape (equal angle, unequal angle, double angle, tube, …) and the
  same material (steel grade) as the group's section: an angle stays an
  angle, an S355 section stays S355. Sections are tried from the lightest
  (mass per metre = area × density), then by area, then by id. A section
  missing from the library is never proposed; a group whose section is
  not in the library (the structure's own `[[section]]` table) is left as
  it is, said.
- Only members with a section, design-checked and not cables are sized.
- **Left as they are, said**: a group whose members carry capacities
  written by hand (`capacity_tension`, `capacity_compression`: they
  describe the section they were written for). **Refused**: a structure
  analysed from a finite-element model file of its own (`model = …`: its
  sections are in that file).

## The loop

1. **Forces**: every load case solved on the structure as it is (its
   model, the counted self-weight as OHL-Structure's run takes it).
2. **Choice with these forces**: per group a pointer on its family,
   starting at the lightest section that can pass at all — a section whose
   gross area times its yield strength, times the target, is below the
   group's largest axial force over the cases (each divided by its case's
   strength factor) is skipped (no code's capacity exceeds A·f_y). Each
   **trial** writes every open group's section at its pointer into a copy
   of the file, builds the model and asks, in one call each, the code's
   check for every member (with the forces of step 1), the code's limits
   and the joint-fit limits. A group whose every member passes all three
   is resolved at that section; the others move one section up. Trials go
   on until every group is resolved or its family is exhausted: as many as
   the longest family tried.
3. **Forces again** with the new sections — forces change with the
   sections' stiffness and the members' weight — and the choice again.
   **A choice the structure does not solve with** (a buckling mode the
   lighter members let happen, a near-mechanism): the groups made lighter
   go back to the sections that solved, **never lighter again**, said on
   each — first those of the members the solver names (in compression at
   the joint moving most, straining most in the mode), else those meeting
   at the joints it moves most, else the nearer half of the groups made
   lighter (by their members' distance to those joints); solved again
   after each, until it solves; after 20 tries every group made lighter
   goes back at once (the choice that solved).
4. **Settled** when a choice equals the one before. A choice that
   **comes back** after a different one (a cycle): per group the heaviest
   section of the cycle's choices, said. **Not settled** within the most
   rounds (10 by default, at least 2): per group the heavier of the last
   two choices, said.
5. **The proposal** is then a **full run** of the structure with the
   chosen sections — OHL-Structure's run, the file's own options in it —
   never the estimated forces of the loop: each group's largest usage
   before and after, its governing case, its mass, its joint-fit problems
   before and after. **Every member above the target in that run is
   listed** (the loop's forces and the full run can differ, e.g. by the
   estimated eccentricities the file asks for).

**How a group's choice is made**, in this order:

1. the lightest of its family passing the check, the code's limits and
   the joint-fit limits;
2. none does: the lightest passing the check and the code's limits — its
   joint-fit problems said, its joints to detail by hand;
3. none passes the check at the target: the section with the lowest
   usage among those meeting the code's limits (said; the full run lists
   its members above the target);
4. none meets the code's limits with a usage: left as it is (said).

## The joint-fit limits

Worked out with the rules the joint layout and the bolt tools already use
(`ohl_structure_link::joint_fit`), so that the sizer and
[Lay out joints](joint-layout.md) agree:

- **The code's distances** — end distance e, spacing s, edge distance to
  the toe, and the force P one bolt passes in this part (its design shear
  capacity or, by ASCE 10-97 4.4, the smaller of it and its bearing in the
  part) — of an end **with bolts of its own** (the connection's, else the
  member's design data): the function Lay out joints' edge rule and the
  bolt tools take (the code plugin's `bolt-distances-full`, else
  `bolt-distances`; where no code gives any, the structure's default
  minimums `[default_bolt_minimums]`: e and the edge 1.5 d, s 3 d, said).
  An end **without bolts**: OpenOHL's **default bolt** for the member (the
  largest bolt of the structure's bolt library its leg takes,
  `ohl_member_check::default_bolts`) with the code's distances for it,
  never below the default minimums the layout draws a picked bolt with
  (the larger of the two). A member end with neither is not checked
  (counted, said).
- **The detailing practice in force** (O44 L29, Structures › Detailing
  practice): the structure file's own `[layout_practice]` over its code
  plugin's, value by value, over the default — exactly as Lay out joints
  takes it (a practice refused, or a plugin failing, leaves the default,
  said). From it: the bolts per end the layout gives an end without bolts
  (per member class, else its count) and the plate thicknesses the shop
  makes.

Per member:

1. **The leg takes its bolt** (an angle's connected leg w wide, t thick,
   root radius r; bolt d): at the gauge the layout draws one line at,
   g = w/2, the hole is clear of the root fillet, g ≥ t + r + d/2, and
   the toe is the code's edge distance away, w − g ≥ edge.
2. **The member holds its bolts**: per end n = the larger of the bolts it
   has (its own; without: the practice's count for its class) and ⌈N/P⌉
   (N the member's largest axial force over the cases divided by the
   case's strength factor), in one line, 2e + (n − 1)s per end; both
   ends' sum at most the member's length between its joints.
3. **A plate the shop makes takes its leg**: where the practice lists
   plate thicknesses, one at least as thick as the member's leg (a plate
   is never thinner than the thickest leg on it); none: an end onto a
   plate cannot be laid out, said.
4. **A bolt**: a member without bolts of its own whose leg takes no bolt
   of the library cannot be connected.

**An end where the member goes on in line through its joint as a piece of
the same section** (within the core's 1° for one part) holds no bolts
there: one part, by the owner's rule of 2026-11-19 (*"a change of section
is a splice; the same section is one part unless marked"*); rules 1 and 2
skip it. Pieces of one group get one section, so they stay one part.

These limits are about building the joint, not its strength: the joint
checks (bolt groups, plates, welds) stay the code plugin's, in every run.

## The code's limits (`member-limits`)

An optional export of the plugin interface 0.3 (additive, as ADR-0012
allows): per member, what the code bounds, the member's value as the code
works it out, the bound and its clause; an empty list: no limit (nothing
checked). A compression limit holds only for a member compressed in a
load case. The bundled plugins give, each value read in the standard:

| Code | Limit | Bound | Clause |
|---|---|---|---|
| ASCE 10-97 | L/r of a leg (members carrying calculated compressive stress) | < 150 | 3.4 |
| ASCE 10-97 | KL/r of other members | < 200 | 3.4 |
| ASCE 10-97 | KL/r of a redundant member | ≤ 250 | 3.4 |
| ASCE 10-97 | w/t of an angle's legs (compression members) | ≤ 25 | 3.7.1 |
| ASCE 10-97 | thickness of members ("suggested") | ≥ 3 mm (1/8 in.) | 3.3 |
| SIA 263:2013 | λ_K = L_K / i of a compression member, main load-carrying | ≤ 200 | 4.5.1.7 (5.4.1.7 for lattice members) |
| SIA 263:2013 | λ_K of bracing and secondary members (a redundant member) | ≤ 250 | 4.5.1.7 |

The values are the check's own: ASCE 10-97's L/r and KL/r with its
end-condition curves (3.7.4) and the buckling lengths worked out from the
bracing; SIA 263's λ_K as its check takes it.

**Left out** (the data do not say what they need): ASCE 10-97 3.4's
tension-only members "detailed with draw" (L/r between 300 and 500); 3.8.1
(lipped angles: none in OpenOHL's sections); SIA 263 4.5.1.7's 160 for
members of structures loaded in fatigue; SIA 263 5.7.3.2's least
thicknesses (cold-formed sheets and profiles only). SIA 263 bounds no flat
width to thickness of hot-rolled angles: a slender leg lowers the
resistance (class 4) in the check instead. ASCE 10-97 3.3's 3/16 in. for
connection plates is a plate's, not a member's: the detailing practice's
plate thicknesses hold it.

## Conservative assumptions

- **Conservative**: a member without a class is taken as one of ASCE
  10-97's "other members" (KL/r 200, the smaller of the two bounds that
  could apply); by SIA 263 every member but a redundant one as main
  load-carrying (200, not 250).
- **Conservative**: a crossing diagonal's slenderness for its limit is
  worked out with its partner not bracing it (the longer length): no load
  case is sent with the limits.
- **Conservative**: the section skipped before a trial (A·f_y times the
  target below the force) cannot pass any code; a member whose data give
  a capacity above A·f_y is not sized at all (its capacities written by
  hand).
- **Conservative**: a cycle takes per group the heaviest of the cycle's
  sections; a loop not settled the heavier of its last two choices; a
  choice the structure does not solve with goes back to the sections that
  solved and is never made lighter again.
- **Conservative**: the joint fit's bolts in one line (the longest
  arrangement) and an end distance on both sides of each end's group; the
  code's distances of a default bolt never below the default minimums.
- **Assumed, not a safety question** (they decide whether a joint can be
  built, not its strength; Lay out joints checks the real joint): the
  gauge at mid-leg; the length between the joints' work points (the steel
  is shorter by its ends' cuts, so a member passing may still be short
  where the layout pulls its ends back — Lay out joints then says so); the
  bolt count from the bolt's capacity alone; pieces in line of one section
  taken as one part (as the owner's rule).

## NOT ON THE SAFE SIDE, and what makes it safe

Each is said in the window and on the command line where it applies, with
what is left out, why that can be unsafe and what makes it safe:

- **The core's simple check**: (1) a design code's column curve,
  imperfections, net sections and connections are left out; (2) a chosen
  section can so be too small for a design code's check; (3) load the
  design code's plugin (`[analysis] capacity`, e.g. `asce10-97`,
  `sia263-2013`) and choose again.
- **No code limits known** (the simple check, or a plugin without
  `member-limits`): (1) the slenderness, flat width to thickness and least
  thickness limits are not checked; (2) a section can pass its usage and
  still break such a limit (a slender member the code does not allow);
  (3) a code plugin with the export (the bundled `asce10` and `sia263`).
- **Height extensions**: (1) the structure is sized as written, not on
  its extensions; (2) an extension changes the forces in the body (a
  taller one larger at the legs), so a member can be overloaded on one;
  (3) run Results on each extension after Apply, and size again where a
  member is above the target.

Said, not a safety question: the joint-fit limits off (a chosen member may
not be connectable); groups whose joints fit no section (to detail by
hand); legs thicker than every plate the practice lists; level 4 — the
joints' details were laid out for the old sections (lay them out again
after Apply).

The session's end releases and bolt slip (Results options, off by
default) and a placed structure's foot movements are not in the loop nor
in the proposal's run; each can only raise a usage, and the Results run
with them says it per member.

## Settings: effect and time

Measured on the validation truss below (the bundled ASCE 10-97 plugin; the
whole run about 0.6 s of CPU time, most of it the program's start and the
plugin's loading) and, for the time, on eight real towers of 600 to
2 700 members (kept privately): about 5 s to 2 min of CPU time for 40 to
360 groups, two to four rounds. The window shows each setting's effect
and the rounds' times measured in the last run.

| Setting | Default | Effect (validation truss) | Time |
|---|---|---|---|
| Target usage | 100 % (the most) | 90 %: no change (each group held by a limit, not its usage); 50 %: two groups heavier, 79 → 103 kg (the window says how many groups the target made heavier and by how much) | about the same: each section tried is one trial for every group at once |
| Keep to the joint-fit limits | on | off: the top chord an L40×4 instead of an L50×5 (79 → 76 kg) with 2 joint-fit problems (its 16 mm bolts do not fit its 40 mm legs); the window says how many groups the limits kept heavier and the mass, worked out in the same run | none extra |
| Split a group | none | the web split: each diagonal and the post its own section, 79 → 70 kg, more different sections | a little more per split group |
| Keep a group | none | left as it is | less |
| Rounds at most | 10 (at least 2) | the truss settles in 2, the towers in 2 to 4; one round could not show a choice holds with its own forces | one solve of every case and one trial per section tried, per round |

## Validation and tests

- The validation case `structsize-001`
  (`apps/structure/ohl-structure-tests/benchmarks/member-sizing.toml`, in
  the benchmark format, run by the integration test `member_sizing.rs`):
  a plane truss of three groups whose choice is proved the lightest
  passing one by **brute force** over every combination of the library's
  six angles (6³ = 216 full runs, the bundled ASCE 10-97 plugin, its
  limits and the joint fit). Each group is held by another rule, none by
  its usage: the bottom chord by 3.4's L/r < 150 of a leg, the top chord
  by the joint fit, the web by 3.4's KL/r < 200.
- Unit tests (`ohl-structure-link` `sizing_tests`): the groups (by group
  and section, split, mirror images), the lightest passing section per
  group and the next lighter failing, the joint-fit limits by hand (pieces
  in line of one section and a splice), the detailing practice's plate
  thicknesses and bolt counts, the settings' effect, the loop with the
  members' weight, a cycle's heavier choice, Apply of the ticked groups
  only, a section outside the library, hand capacities left, a model file
  and a target above 100 % refused.
- The plugins' native tests: the limits by hand (ASCE 10-97 on an L50×5,
  SIA 263 on an L60×6).
- OHL-Structure's window (`sizing_ui_tests`): the run, the proposal,
  Apply as one undo step, Escape.
- The taking back of a choice the structure does not solve with was met
  and checked on two of the real towers (a joint's buckling mode once its
  bracing was made lighter); no small invented structure was found that
  shows it (the solver's joint stabilisation holds the simple ones).

## Later stages (ROADMAP O49)

Level 3 members created from a level 2 outline; the joint layout's
"bigger member" option calling the sizer; sizing with the joint layout in
the loop.
