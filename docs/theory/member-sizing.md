# The member sizer

(2026-11-26, roadmap O49 stage 1; **built, in active development, not
approved yet**.) The owner asked: *"Would it maybe help the joints to also
automate the level 3? (choice of members) where there would be a basic
choice and then plugins can adapt the values based on norms"*, then
*"Okay go ahead with the builder"*. This page says how OHL-Structure's
member sizer chooses the sections of a [level 3](../development/structure-detail.md)
structure, how its loop settles, and what it assumes. The window is
[Choose the members](../user-guide/member-sizing.md).

## What it answers

For each **member group** of the structure: the lightest section of the
project's section library, in the group's own family, that passes

1. **the design code's check**: every member of the group at most the
   target usage (1 by default) in every load case, by the code the
   structure names (`[analysis] capacity`: the bundled ASCE 10-97 plugin
   by default, SIA 263:2013, or another plugin's method; the core's
   `simple` check only where it is chosen — then NOT ON THE SAFE SIDE,
   below);
2. **the code's limits on the members' sizes** that a usage does not hold
   — a compression member's slenderness, a leg's flat width to thickness,
   a member's least thickness — given by the code's plugin (the
   `member-limits` export, below);
3. **the joint-fit limits**: the member can still be bolted — its leg
   takes its bolt, and it is long enough for the bolts its end force needs
   (below).

The core makes this basic choice; the plugins adapt the values to their
code (the check, the limits, the bolt distances and the bolt's
capacity). The choice can never land on a member that cannot be
connected by these rules.

## Groups and families

- **A group** is the members of one `group` with one section (the
  structure file's `group` of each member; a group with several sections
  gives one sizing group per section, named "group · section"). Members
  without a group stay with their **mirror images** (`[[symmetry]]`) of
  the same section; alone otherwise. The designer may **split** a group:
  each of its members (with its mirror images) is then sized alone; or
  **keep** it as it is. No file field is added: a group stays the members
  naming it (decision of 2026-11-26).
- **A family** is the sections of the project's section library with the
  same shape (equal angle, unequal angle, double angle, tube, …) and the
  same material (steel grade) as the group's section: an angle stays an
  angle, an S355 section stays S355. Sections are tried from the lightest
  (mass per metre = area × density), then by area, then by id. A section
  missing from the library is never proposed; a group whose section is
  not in the library (the structure's own `[[section]]` table) is left as
  it is, said.
- Only members with a section, design-checked and not cables are sized.

## The loop

1. **Forces**: every load case solved on the structure as it is (its
   model, the counted self-weight as OHL-Structure's run takes it).
2. **Choice with these forces**: per group a pointer on its family,
   starting at the lightest section that can pass at all — a section whose
   gross area times its yield strength, times the largest strength factor
   and the target, is below the group's largest axial force is skipped
   (no code's capacity exceeds A·f_y). Each **trial** writes every open
   group's section at its pointer into a copy of the file, builds the
   model and asks, in one call each, the code's check for every member
   (with the forces of step 1), the code's limits and the joint-fit
   limits. A group whose every member passes all three is resolved at
   that section; the others move one section up. Trials go on until
   every group is resolved or its family is exhausted. The trials are as
   many as the longest family tried.
3. **Forces again** with the new sections — forces change with the
   sections' stiffness and the members' weight — and the choice again.
4. **Settled** when a choice equals the one before. A choice that
   **comes back** after a different one (a cycle): per group the heaviest
   section of the cycle's choices, said. **Not settled** within the most
   rounds (10 by default): per group the heavier of the last two choices,
   said.
5. **The proposal** is then a **full run** of the structure with the
   chosen sections — OHL-Structure's run, the file's own options in it —
   never the estimated forces of the loop: each group's largest usage
   before and after, its governing case, its mass, its joint-fit problems
   before and after. **Every member above the target in that run is
   listed** (the loop's forces and the full run can differ, e.g. by the
   estimated eccentricities the file asks for).

A group that no section of its family passes at the target gets the
section with the lowest usage among those meeting the limits (said:
"no section of its family passes at the target: the one using least");
where none meets the limits, it is left as it is (said). The designer
then chooses another family or another arrangement.

## The joint-fit limits

Per member end, with its bolt: the end's own bolts in its data (the
connection's, else the member's design data), else OpenOHL's **default
bolt** for the member (the largest bolt of the structure's bolt library
the leg's usual gauge takes, `ohl_member_check::default_bolts`). A member
end with neither is not checked (counted, said). The code's distances —
end distance e, spacing s, edge distance to the toe, and the force P one
bolt passes in this part (its design shear capacity or, by ASCE 10-97 4.4,
the smaller of it and its bearing in the part) — come from the plugin's
`bolt-distances-full` (else `bolt-distances`), without an analysis; where
no code gives any, the structure's default minimums
(`[default_bolt_minimums]`: e and the edge 1.5 d, s 3 d).

- **The leg takes its bolt** (an angle's connected leg w wide, t thick,
  root radius r; bolt d): at the usual gauge g = w/2 the hole is clear of
  the root fillet, g ≥ t + r + d/2, and the toe is the code's edge
  distance away, w − g ≥ edge.
- **The member holds its bolts**: n = ⌈N/P⌉ bolts at each end (N the
  member's largest axial force over the cases divided by the case's
  strength factor; n ≥ 1), in one line, 2e + (n − 1)s per end; both ends'
  sum at most the member's length between its joints.
- **The detailing practice's rules** (roadmap O44 L29, to come) plug in
  at one function (`sizing::practice_fit`), empty today (ROADMAP O49
  stage 4).

These limits are about building the joint, not its strength: the joint
checks (bolt groups, plates, welds) stay the code plugin's, in every run.

## The code's limits (`member-limits`)

An optional export of the plugin interface 0.3 (additive): per member,
what the code bounds, the member's value as the code works it out, the
bound and its clause; empty: no limit. A compression limit holds only for
a member compressed in a load case. The bundled plugins give:

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
resistance (class 4) in the check instead.

## Conservative assumptions

- **CONSERVATIVE**: a member without a class is taken as one of ASCE
  10-97's "other members" (KL/r 200, the smaller of the two bounds that
  could apply); by SIA 263 every member but a redundant one as main
  load-carrying (200, not 250).
- **CONSERVATIVE**: a crossing diagonal's slenderness for its limit is
  worked out with its partner not bracing it (the longer length): no load
  case is sent with the limits.
- **CONSERVATIVE**: the section skipped before a trial (A·f_y times the
  largest strength factor and the target below the force) cannot pass any
  code; a member whose data give a capacity above A·f_y may so get a
  heavier section than it needs, never a lighter one.
- **CONSERVATIVE**: a cycle takes per group the heaviest of the cycle's
  sections; a loop not settled the heavier of its last two choices.
- **CONSERVATIVE**: the joint fit's bolts in one line (the longest
  arrangement) and an end distance on both sides of each end's group.
- **Assumed, not a safety question**: the joint fit's gauge at mid-leg
  (the joint layout chooses the real gauges later); the bolt count from
  the bolt's capacity alone.

## NOT ON THE SAFE SIDE, and what makes it safe

Each is said in the window and on the command line where it applies:

- **The core's simple check**: (1) a design code's column curve,
  imperfections, net sections and connections are left out; (2) a chosen
  section can so be too small for a design code; (3) load the design
  code's plugin (`[analysis] capacity`, e.g. `asce10-97`, `sia263-2013`)
  and size again.
- **No code limits known** (the simple check, or a plugin without
  `member-limits`): (1) the slenderness, flat width to thickness and least
  thickness limits are not checked; (2) a section can pass its usage and
  still break such a limit; (3) a code plugin with the export (the
  bundled `asce10` and `sia263`).
- **Height extensions**: (1) the structure is sized as written, not on
  its extensions; (2) an extension changes the forces in the body (a
  taller one larger at the legs), so a member can be overloaded on one;
  (3) run Results on each extension after Apply, and size again where a
  member is above the target.
- **The joint-fit limits off** (a setting): not a safety question but
  said in the warning colour: a chosen member may not be connectable.

The session's end releases and bolt slip (Results options, off by
default) and a placed structure's foot movements are not in the loop nor
in the proposal's run; each can only raise a usage, and the Results run
with them says it per member.

## Settings: effect and time

| Setting | Default | Effect | Time |
|---|---|---|---|
| Target usage | 100 % | lower: heavier sections with a reserve (measured on the validation truss: 50 % → +62 % mass) | about the same: each section tried is one trial for every group at once |
| Keep to the joint-fit limits | on | off: the check and the code's limits alone; the proposal says how many groups the limits kept heavier and the mass (worked out in the same run) | none extra |
| Split / Keep a group | none | split: each member with its mirror images its own section (lighter, more sections); keep: left as it is | a little more per split group |
| Rounds at most | 10 | a structure usually settles in two to four rounds | one solve of every case and one trial per section tried, per round |

## Validation and tests

- The validation case `structsize-001`
  (`apps/structure/ohl-structure-tests/benchmarks/member-sizing.toml`, in
  the benchmark format, run by the integration test `member_sizing.rs`):
  a plane truss of three groups whose choice is proved the lightest
  passing one by **brute force** over every combination of the library's
  six angles (6³ = 216 full runs, the bundled ASCE 10-97 plugin, its
  limits and the joint fit).
- Unit tests (`ohl-structure-link` `sizing_tests`): the groups (by group
  and section, split, mirror images), the lightest passing section per
  group and the next lighter failing, the joint-fit limits by hand, the
  settings' effect, the loop with the members' weight, a cycle's heavier
  choice, Apply of the ticked groups only, a section outside the library.
- The plugins' native tests: the limits by hand (ASCE 10-97 on an L50×5,
  SIA 263 on an L60×6).
- OHL-Structure's window (`sizing_ui_tests`): the run, the proposal,
  Apply as one undo step, Escape.

## Later stages (ROADMAP O49)

Level 3 members created from a level 2 outline; the joint layout's
"bigger member" option calling the sizer; the detailing practice's rules
as joint-fit limits; sizing with the joint layout in the loop.
