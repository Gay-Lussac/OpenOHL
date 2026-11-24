# Checks on a working set: what is pulled in

*Built 2026-11-20 (the country server's phase 10), in active development,
not approved yet.* Implemented in `crates/ohl-scope`; computed by a
network's server (`crates/ohl-server`, `network_scoped.rs`) when a client
asks for a check on a working set
([server mode](../user-guide/server.md#checks-with-their-neighbours-phase-10-in-active-development)).

On a network a user works on a **working set** (a part of one line,
[Working sets](../user-guide/working-set.md)). A check's result can depend
on objects outside it: another line's wires crossing or running beside,
this line's own wires beyond the working set, other lines' sections on a
shared tower, other lines' currents near. Each check declares its
**reach** — the distance within which other objects can change its result
— and the objects within it are **pulled in**: read (as solving each
section alone reads it, never a whole line), solved in the check's case
with their own line's settings and wires, and used. Every result lists
what was pulled in.

Labels as in [Conventions](conventions.md#conservative-assumptions-and-simplifications).

## The rule: never less than on the whole network

A scoped result must be **equal to the same check on the whole network**
(every line solved whole, every wire a target) **or more conservative**.
Where it cannot be, the result says **NOT ON THE SAFE SIDE** and this page
says why. A neighbour that could not be read or solved makes the check
**not all checked** — never a pass on the assumption that nothing is
there.

Proven on the synthetic country (each route a line of its own, towers
shared by two routes twins, as a network holds them; `crates/ohl-scope`,
`tests.rs`):

| Check | Scoped against the whole network |
|---|---|
| Clearances between wires (other lines, this line), two weather cases (still air, wind), three working sets of the lines with shared towers | the same results to the bit: 22 239 pairs and wires, 2 499 of them below the rules' distances, 1 022 other lines' objects used |
| Loads on shared towers, nine towers, every load case | the same loads to the bit; against the country as one project (a different model: a tower of two routes is solved there with one route's alignment), within 1.7 % of each point's load |
| Magnetic field, lines within 500 m | what the field distance leaves out: at most 0.00019 µT where the largest field is 4.2 µT (NOT ON THE SAFE SIDE, below) |

## The reach of each check

| Check | Reach |
|---|---|
| Clearances between wires (rules to other lines and between this line's wires; crossings among them) | every wire within the largest reach of the wire rules applying to the case |
| Clearances to the ground and to points | the working set's own wires; the ground and the points are not served by a network yet: not checked there, said |
| Distance between conductors (in the span, under unequal wind) | what the working set holds: its sections and every section on its structures |
| Magnetic field | every line with wires within the field distance (500 m, owner 2026-11-16; a setting of the check) |
| Structure loads | other lines' sections on the working set's structures: their twins |
| A plugin's check | as its manifest declares (`[reach]`, [Writing plugins](../development/plugins.md#a-checks-reach)); none declared: the working set's whole line and every line within 2 km |

## Clearances between wires

**The reach.** The largest plan reach of the wire rules applying to the
case (`Rule::reach_in_plan`: the rule's largest distance and its search
margin), worked out for every potential a section near the working set
can have and up to the highest wire there can be.

- **Conservative:** the potentials are each phase of each system of every
  line found near (a superset of its sections' own), and the highest wire
  the top of the highest structure near plus 2 m for a bundle — the reach
  is never smaller than the one the sections themselves would give. With
  the lines found in the larger area it is worked out again until it no
  longer grows.

**The candidates.** A span's wire, before it is solved, lies within a
bound of its structures' chord:

\\[ |P - M| \le \tfrac{\ell}{2} + \tfrac{R_A + R_B}{2} \\]

for every point *P* of the wire in plan, *M* the middle of the
structures' chord, ℓ the wire's length and *R* each structure's radius
(its points and wire points from its position, plus its longest string —
how far a wire point can swing). Proof: |P − E_A| + |P − E_B| ≤ ℓ along
the wire between its ends E, so |P − M_E| ≤ ℓ/2 for the ends' middle, and
a projection shortens every distance.

- **Conservative assumption:** no wire is longer than 1.5 times the
  straight line between its ends (a sag of about half the span: no
  overhead line comes near it), ℓ ≤ 1.5 · √((d + R_A + R_B)² + (Δz + H_A +
  H_B)²) with *d* the structures' plan distance, Δz their height
  difference and *H* their heights. Every candidate solved is checked
  against it; a wire outside it makes the result say NOT ON THE SAFE SIDE
  (a wire farther away could have been left out).
- **Conservative assumption:** a structure with a type, before its type is
  read, has its points within 50 m of its position and at most 150 m
  above it (larger than any overhead line structure).

A section is a candidate when one of its spans' bounds comes within the
reach of a checked wire. Each candidate is solved; its wires within the
reach are listed as used, the others as read.

**The same results.** The rules run by one function on the working set's
wires, this line's others and the other lines' (`ohl_scope::clearance::run_rules`),
the wires taken in one order (by line, section, span): a pair's distances,
requirements and counts depend on its two wires only, so every pair within
reach comes out as on the whole network.

- **Conservative:** pairs are looked for from three points of each piece
  of a checked wire; the scoped run looks as far as its reach plus the
  longest piece, so no pair within a rule's distance is missed whatever
  the pieces' length (the whole network's run may look less far).

## Magnetic field

The working set's line is computed as one: its sections and this line's
sections within the field distance, their currents' phases and
directions together, at profiles across the working set's spans. Every
other line with wires within the field distance is computed on its own
(its sections there, its own settings and currents) and its field added at
each point.

- **Conservative:** the other lines' fields are **added by size**. Two
  lines' phase angles to each other are not known to the program, and
  the size of a sum of fields is at most the sum of their sizes (the rms
  of the vector is a norm). The exact treatment: the lines' currents with
  their phase angles to each other, summed as phasors.
- **Not on the safe side (said in every result):** the wires farther than
  the field distance from the working set's wires — of this line and of
  others — are left out (owner, 2026-11-16: "fields pulling in lines
  within 500 m"). Their field at the points is not in the result. On the
  synthetic country (lines of 50 to 380 kV, 500 A in every circuit) it
  leaves out at most 0.00019 µT where the largest field is 4.2 µT; a line with
  far larger currents just beyond the distance leaves more out. The
  distance is a setting of the check: larger, less is left out.
- A circuit without a current (its system gives none and the check gives
  none) takes 0 A, said.

## Loads on shared towers

A tower carrying wires of several lines is, on a network, one structure in
each line's model: **twins** — of the same type, within 0.5 m in plan (the
network's import lists them so). Each line's sections hang on its own
twin's points.

- Under **each line's load cases** (its own library's, else the shipped
  defaults) every line's sections on the tower are solved in that case,
  each with its own line's settings and wires, and the loads at its twin's
  points taken. **The worst kept**: per point and component the largest
  factored load by size, with the line and case giving it.
- At a point holding one line's sections: that line's load there. At a
  point holding none: the case's line's own twin's (its strings only). The
  wind on the body: the case's line's, once.
- **Conservative:** at a point holding several lines' sections their loads
  are added, each line's strings there counted again (more load, said).
  The exact treatment: one model of the tower holding every line's
  strings.
- **Conservative:** the other lines' sections are solved in every case
  the case's line's twin takes, their own lines' structure groups set
  aside (the tower is the case's line's).
- **Not on the safe side (said with the tower):** the loads are solved
  rigid (calculation level 2). A line set to level 3 or 4 takes the larger
  of the rigid and the flexible solve's loads in its own checks; here the
  flexible solve is not made.
- **On an offline working set's file** (2026-11-24): OHL-Design's
  Structure loads window runs this same computation on the file
  (`ohl_server::offline_checks::shared_tower_loads`, the network's check
  without its report) for the shared towers of the working set among the
  structures it computes — the same loads, to the bit (tested). A shared
  tower outside the working set (read-only in the file) is **not
  checked** there with its other lines (NOT ALL CHECKED, said); the
  window's usage check keeps this line's loads alone on the shared towers
  (NOT ALL CHECKED for them, said).

## Neighbours a user may not see

The server computes every check with every neighbour, those its user may
not see too (owner, 2026-11-16), so the numbers are those of the whole
network. Its report says only what the rights allow: a neighbour the user
may not see is never named nor placed — "a hidden object of another line
is within 3.41 m", "check result limited by an object you may not see" on
the user's own span, without the point on it — and is counted in what was
pulled in. Its texts naming one are replaced. A change of a neighbour
makes the result stale; the notice names it only when the user may see
it.

## Results out of date

Each result is watched: the working set's objects, the neighbours it read,
the tiles its reach covers, the lines it read. The first change touching
any of them tells its user once that the result is stale; OHL-Design
marks the working set's routes out of date in its replica, as a shared
tower's move marks them ([Shared towers](../user-guide/shared-towers.md)).

## A check run on the replica alone

A check run in OHL-Design on a working set's replica (without asking the
network) reads what the replica holds. Where its reach needs more — the
wire rules, the structure loads, a plugin's check without a reach of 0 —
its result starts with **NOT ALL CHECKED** and says what was not pulled
in.
