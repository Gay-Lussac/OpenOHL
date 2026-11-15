# Structure flexibility in the line solve (calculation levels 3 and 4)

*Partly built, in active development (not approved yet; plan approved by
the owner 2026-11-15).* Built (no setting, no check uses them yet): the
condensation of a structure to its attachment points
(`ohl_structures_fe::condense`), of a pole (`condense_pole`), of a placed
structure in the line's axes (`ohl-loads`, `flexibility`); on the wire
side a section solved with its structure points moved and its stiffness
at those points (`ohl_wirefe::Model::support_stiffness`,
`ohl_line::solve::Coupling::moved`, `SupportStiffness`); and **the
interface loop with calculation level 3** (`ohl_line::solve::flexible`,
`ohl_loads::solve_line_flexible`), run for testing by the command line's
`ohl line … --calculation-level 3`, which prints the attachment points'
displacements and the change against rigid after the rigid results.
**In active development, not built yet:** the structures' own wind and
ice in their deflection, the structure checks with the larger of rigid
and flexible, the other checks taking the level, the line setting and
every window, level 4. Every result the program gives (every check,
every file written) is solved with the structures rigid (calculation
level 2), as [Finite-element wire systems](wirefe.md) says.

## Two kinds of level

Two different things are called a *level*; this page always says which.

- **Calculation level** — how a line and its structures are computed
  together: **1** ruling span on rigid structures; **2** wire finite
  elements on rigid structures (today); **3** wire finite elements with a
  linear stiffness of each structure at its attachment points; **4** wire
  finite elements with the structures' full, non-linear models solved
  together with them.
- **Detail level** of a structure — how much its file describes: 1 stick
  figure, 2 outline, 3 full design, 4 full design with joint details.

Only a structure whose file gives a computable model can be flexible:
detail level 3 or 4 (its members), a separate model file (`.ohlfe`), or a
pole with an elasticity. **Every other structure stays rigid** at
calculation levels 3 and 4, and the results say which and why (below).

## Rigid, level 3, level 4

**Rigid (levels 1 and 2).** Every structure point is a fixed node of the
wire model: strings swing, clamps move, spans take unequal tensions — the
points the strings hang from never move, whatever the wires pull. The
structures are analysed afterwards under the forces of that solve; their
deflection never goes back to the wires. Sections hanging on one tower do
not feel each other through it.

**Level 3 — linear structure stiffness.** Each flexible structure is
represented at its attachment points by a matrix: how far each point moves
in each of three directions under a unit force at any point in any
direction — including how a pull at one phase moves another phase's point
(a tower is one body). The matrix is condensed once from the structure's
model at a reference state (below) and does not change with the load:
members going slack and P-Delta growing with the load are fixed at that
state.

**Level 4 — full structures.** The structures' own non-linear solves
(P-Delta at the actual loads, tension-only members and slack guys as each
case has them, the structure's own wind and ice) take part in every pass
of the iteration below; the matrix of level 3 is replaced by each
structure's tangent at its current state. *(Not built.)*

## The interface loop

Wires and structures are **not** merged into one finite-element model:
each section and each structure keeps its own solver, its exit clauses and
its error reports; sections stay parallel and memoised; one failing
structure does not fail the line. They meet only at the attachment points.
*(Built: `ohl_line::solve::flexible::solve_line_coupled`, level 3 by
`solve_line_flexible`; the whole line coupled at once, owner 2026-11-15.)*

The unknowns are the displacements **u** of the attachment points of the
flexible structures (three each, line axes, measured from the structures
as strung); rigid is u = 0.

1. **Pass 0 is the rigid solve**: every section solved with u = 0 — the
   whole line as always, shared assemblies, spacers and the jumpers' pull
   included, bit for bit the rigid solve (tested) — gives the forces
   **F** the wires put on each attachment point (the whole bundle's,
   unfactored) and each section's stiffness there (below), summed per
   point into \\( \mathbf K_w \\). Every result therefore shows its change
   against rigid at no extra cost.
2. **The structures answer**: level 3,
   \\( \mathbf u_s = \mathbf C\,(\mathbf F - \mathbf F_{ref}) \\) with C a
   structure's flexibility at its points (one matrix per structure, its
   points fully coupled) and \\( \mathbf F_{ref} \\) the wires' forces at
   the stringing state (below); level 4 (not built), the structure's
   non-linear solve under F less its state at the reference.
3. **Mismatch** \\( \mathbf r = \mathbf u - \mathbf u_s \\).
4. **Converged** when, at every point, the mismatch is at most
   **0.1 mm** and the force changed by at most **1 N** (or 10⁻⁶ of the
   force, the larger) since the last accepted pass. In pass 0, which has
   no last pass, the force test takes \\( \mathbf K_w \mathbf r \\), the
   change a move by the mismatch would make: a case whose structures would
   move less than 0.1 mm is answered by the rigid solve itself.
5. **Newton step** on all attachment displacements of the line at once,
   from \\( \mathbf r(\mathbf u) = \mathbf u - \mathbf C\,(\mathbf F(\mathbf u) - \mathbf F_{ref}) \\)
   and \\( \partial\mathbf F/\partial\mathbf u = -\mathbf K_w \\):
   \\[ (\mathbf I + \mathbf C\,\mathbf K_w)\,\Delta\mathbf u = -\mathbf r \\]
   — the flexibility form, which needs no inverse of C (equivalently
   \\( (\mathbf K_s + \mathbf K_w)\,\Delta\mathbf u = -\mathbf K_s\,\mathbf r \\)
   with \\( \mathbf K_s = \mathbf C^{-1} \\) where C has an inverse; it does
   not when two points cannot move apart, below). 3 × (flexible points)
   unknowns, C K_w formed structure by structure (C is block diagonal),
   solved by Gaussian elimination with partial pivoting within the
   system's band (the points ordered by structure along the line; a
   section couples only the structures it passes), a dense LU when the
   band is wide.
6. **Line search**: a pass whose largest mismatch grew is solved again with
   half the step, at most three times; then the step is taken anyway.
7. **Move the attachment points** to u + Δu, solve the line again (the
   sections in parallel), back to 2.

Only the forces must be exact; \\( \mathbf K_w \\) may be approximate — it
then costs passes, never accuracy (the hand cases below converge
quadratically: the mismatch 30 mm, 0.1 mm, 10⁻⁹ m).

**Exit clauses** (each said when it stops a case): 20 passes at most (line
solves, halved steps included; level 3 converges in 1–6 in the cases
below); a time limit of 10 minutes per case, checked before each pass with
the last pass's time (a guard far above any real case); the budget's
cancel token; each section's own budget. **A case that does not converge
fails** (owner 2026-11-15) — never the last pass returned as if converged
— naming the worst point, its direction in the structure's axes and the
trend: "not converged in 8 passes; mismatch still … mm at T2 W1
(vertical), decreasing …× a pass". A section that solved rigid but not
with its points moved fails the case too, naming it. The rigid solve is
kept with the failure and said rigid. After pass 0 the time is
estimated as pass 0's time × 4 (level 3 expects 2–5 passes); the result
gives the estimate, the measured time and each pass's time.

**A local origin for the moved passes.** A line in projected coordinates
of millions of metres is solved in its passes after pass 0 about a local
origin (the first structure's position rounded to whole kilometres; pass
0 stays the rigid solve as given) and its results are moved back: with
the points moved, some sections' Newton iterations otherwise stall at a
force noise of about 10⁻³ N — the rounding of such coordinates
(about 5·10⁻¹⁰ m) times the wires' stiffness — just above their
tolerance (found on a real line; about the local origin the same
sections converge). Tested: the same line near and far from the origin
gives the same results to 10⁻⁶ m and 10⁻³ N.

**The reference forces** \\( \mathbf F_{ref} \\): each section in still air
at its stringing temperature, the whole line solved as in any case (one
solve per distinct stringing temperature, each section's forces from the
solve at its own) — the state the structures are drawn in (geometry as
strung). A case equal to it gives u = 0 exactly (tested: one pass, the
sections the rigid solve's bit for bit). A section in a final condition
(after creep or after load) is solved in that condition here too, as in
every case.

**Results** per case: per point of a flexible structure its displacement
(line axes and the structure's: right, ahead, up) and the wires' force on
it rigid and flexible; per span the horizontal tension, the sag and the
lowest point's height, rigid and flexible; each pass's mismatch, force
change, worst point and time; the structures taken rigid with why; the
notes below.

## A structure condensed to its attachment points

*(Built: `ohl_structures_fe::condense`.)*

**What.** For a placed structure: the flexibility **C** between its
attachment points that carry wires, three translations per point
(transverse, longitudinal, vertical), every coupling between points — a
3m × 3m symmetric matrix for m points.

**How: static condensation.** The model's tangent stiffness **K** (below)
is factorised once with the structure solver's own skyline \\( \mathbf L\mathbf D\mathbf L^T \\);
for each attachment point and direction a unit force is solved by forward
and back substitution; the attachment points' displacements under it give
one column:
\\[ \mathbf C = \mathbf P^T \mathbf K^{-1} \mathbf P , \\]
**P** the unit loads (one column per point and direction, at the point's
joint). This is the reduction of the stiffness to the attachment degrees
of freedom described by Guyan (1965), exact for a static linear
structure: \\( \mathbf K_c = \mathbf C^{-1} = \mathbf K_{aa} - \mathbf K_{ao}\mathbf K_{oo}^{-1}\mathbf K_{oa} \\),
the joints that carry no attachment eliminated. C is symmetric
(Maxwell–Betti); round-off is removed by taking its mean with its
transpose.

**Points, exactly as the structure check maps their loads.** A point's
unit loads come from the same function that maps its loads for the
structure check (`Model::attachment_load`): a point on a joint; a placed
structure's own link (a relink, an offset) over the type's; an unlinked
point refuses the structure. An **offset** r from the joint is a rigid
link: where a member that is always a frame meets the joint, the unit
force carries its moment r × F and the point moves by
\\( \mathbf u + \boldsymbol\theta \times \mathbf r \\); where only bars meet it, the
moment is left out (as the check leaves it out) and the point moves with
the joint's translation only — said in the notes. Points whose unit loads
are the same (several points on one joint without a turned offset) share
one block of the matrices.

**When there is no stiffness matrix.** \\( \mathbf K_c = \mathbf C^{-1} \\) exists only
when C is positive definite. Two points on one rigid link (one frame
joint, two offsets) cannot move apart along their line, and a point on a
fixed joint does not move at all: C is then singular, the condensation
says so and gives the flexibility only — hence the flexibility form of the
Newton step above.

**Placement.** Condensed in the structure's axes (x right of the line,
y ahead, z up), turned to the line's by the placed structure's
orientation, each 3 × 3 block B becoming \\( \mathbf R\,\mathbf B\,\mathbf R^T \\). The
height adjustment is a translation and changes nothing. A structure on an
extension that changes its model (a body or leg extension) is condensed on
that model, as the structure check analyses it.

**Foundations** as the model gives them: fixed or pinned supports, or
springs. Rigid supports are stiffer than real foundations: deflections
too small — not on the safe side for clearances; springs give the exact
treatment.

## The reference state and the linearisation

**Geometry as strung** (owner 2026-11-15). The attachment points as drawn
are where they are at the stringing state; displacements are measured from
there: u = 0 at stringing, \\( \mathbf F_{ref} \\) the stringing forces. The
stringing, its memo and the sagging tables stay exactly as they are. Left
out: the deflection already present at stringing (a crossarm's droop under
the wires' weight, a dead end's lean) — **not on the safe side** where it
lowers conductors. Solving the stringing itself on flexible structures is
a later option.

**Level 3's matrix is the tangent at the stringing state** (owner
2026-11-15). The structure is solved under its self-weight and the wires'
stringing forces with its own settings; the matrix is condensed from the
tangent stiffness the solver assembles at that solution:
\\( \mathbf K = \mathbf K_e + \mathbf K_g(N) \\) —

- the elastic stiffness of the members active at that state: tension-only
  members bowed or slack there and slack guys are out of the stiffness, as
  the solver leaves them out (a bowed member's constant force still turns
  with its chord: its geometric part stays);
- with P-Delta (when the model asks for it) the geometric stiffness of the
  axial forces of that state;
- the joints the reference solve stabilised, held the same way.

The alternative, the unloaded elastic structure (first order, every
tension-only member active), is the stiffest and is kept for checks.

What a linear matrix cannot follow — members switching between slack and
taut with the load, guys slackening, P-Delta growing — is level 4's. Level
3 says it: "linear stiffness at the stringing state: tension-only members
and guys as at that state".

**Refused as flexible** (the structure stays rigid, with the reason): a
point unlinked or on no joint; a point on a joint only bars meet, all in a
line or a plane (no stiffness across them — the structure check refuses a
load across such a joint too); a unit force at a point that a joint held
to the ground would carry by more than 1 % (a direction no member holds:
its flexibility would mean nothing; the solver's own limit for a load); a
reference solve that fails; a tangent that is a mechanism or not positive
definite (compression above a buckling load at the reference). A reference
solution accepted as approximate (tension-only members kept active as a
last resort, or iterations stopped just above their tolerance) gives a
matrix flagged approximate.

## Poles

*(Built: `condense_pole`.)* The pole check's own frame model of the pole
above the ground line — tapered frame elements, fixed at the ground line —
with a node at each attachment point's height and the point's offset from
the axis as a rigid link (the crossarm rigid): bending both ways and
torsion (J = 2I, G = E/2.6 of the round section; to be checked for
polygonal hollow shafts). At the reference, its own weight and the wires'
stringing forces with P-Delta. **Not on the safe side for clearances,
said with every pole:** fixed at the ground line (the embedment's rotation
left out), crossarms rigid, wood's creep under sustained load left out —
all stiffer than the real pole. A pole without an elasticity stays rigid.
A pole drawn as members in a model takes the model's condensation.

## Structures that stay rigid

Listed with the structure and the reason, e.g.: "no structure model
(detail level 1 or 2)", "its model could not be read: …", "pole without
elasticity", "attachment point A2-L not linked to a joint", "attachment
point A1-R: only bars meet its joint J7, in a line or a plane — no
stiffness across them", "its model did not solve at the reference: …",
"no structure type". A structure that carries no wire solved by finite
elements has nothing to move and is left out of the loop. Ruling-span
sections are solved on rigid structures (owner 2026-11-15); their forces
load the structures in the structure checks, but at level 3 the change of
their forces is not in the structures' deflection (they report no forces
at points) — said with the result.

## The wire side

*(Built: `ohl_wirefe::Model::support_stiffness`, `Coupling::moved`,
`Coupling::stiffness`.)*

**A section with its points moved.** Each structure point holding a
section (a string's top, a wire point fixed to the structure, a post's
top) can be placed at its position plus a displacement, by structure and
label — the labels the section reports its loads with. The unstressed
lengths stay those of the stringing (reference as strung). A post moves
with its base's translation; its tilt is left out (said, until the
rotations of fixed posts are condensed). **No displacement, or zero
displacements, solve exactly as before, bit for bit** (tested on every
section of the example line, still air and wind).

**The section's stiffness at its points.** At the solved state, the wire
model's tangent is assembled with the structure points as unknowns too and
condensed onto them:
\\[ \mathbf K_w = \mathbf K_{ss} - \mathbf K_{sf}\,\mathbf K_{ff}^{-1}\,\mathbf K_{fs} , \\]
s the structure points, f the free nodes (clamps, string joints, load
points). Moving the points by δ changes the force the section puts on its
structures by \\( -\mathbf K_w \delta \\). The model is one subconductor's;
forces and \\( \mathbf K_w \\) are the whole bundle's (times the bundle).
\\( \mathbf K_w \\) is exact for the elements as solved; it leaves out what a
case re-solves around them — the sharing of a section in the blocks over
its sheaves, the secant on stress-strain curves, the dependence of the ice
and wind per stretched metre on the stretch (about 10⁻⁴ of the stiffness
in the test below), the jumpers' pull. That is allowed: the Newton step
needs only an approximate stiffness.

Checks: a level span between two supports — \\( \mathbf K_w \\) against central
finite differences of the solved reactions to 10⁻⁶, and along the span
within 2 % of the parabola's \\( 1/(L/EA + w^2L^3/12H^3) \\); a two-span
section with a suspension string to 10⁻⁵ (the finite-difference error
falls with the step squared: 1.4·10⁻³ at 10 cm, 1.3·10⁻⁴ at 3 cm, below
10⁻⁵ at 3 mm); a quad bundle in wind through the line solve to 10⁻⁵ with
the ice and wind per unstretched metre and 10⁻³ per stretched metre, four
times one wire's; a support moved 5 cm along a 300 m span at 20 kN: the
tension falls by about 0.6 kN, as the elastic catenary
\\( L = \tfrac{2H}{w}\operatorname{asinh}\tfrac{wL_0}{2H} + \tfrac{HL_0}{EA} \\)
with the same \\( L_0 \\) and w says, to 10⁻⁷.

## What level 3 leaves out or approximates

- **The matrix is fixed** at the stringing state: members going slack or
  taut with the case's load, guys slackening and P-Delta growing are not
  followed (level 4's) — stiffer than the case's state where members
  would go slack.
- **The structures' own wind and ice** are not in their deflection yet
  (they come with the structure loads, plan phase 4): under wind the
  points move less than they would — **not on the safe side** for
  leeward clearances.
- **The deflection already present at stringing** is left out (geometry
  as strung) — not on the safe side where it lowers conductors.
- **Translations only**: a hinge's friction moment on a pivoting post and
  a fixed post's tilt are left out (the post moves with its base).
- **Ruling-span sections** are rigid: their points do not move and the
  change of their forces is not in the structures' deflection (they
  report no forces at points); said with the result.
- **A section that did not solve rigid** stays missing as in the rigid
  solve; its points' forces are taken as at the stringing state in the
  structures' deflection, said.
- **The wires' stiffness** \\( \mathbf K_w \\) is approximate (above): it
  costs passes, never accuracy — the converged answer is exact for the
  forces as the sections solve them.
- **Unfactored**: the interaction at service loads; the load case's
  factors go on the resulting forces (below).

## Hand cases (level 3)

Each checked by a test against a reference independent of the loop
(`ohl-line`, `solve::flexible_tests`; `ohl-loads`, `flexibility`):

1. **A span on a spring** — a level 300 m span strung at 20 kN at 15 °C,
   one end rigid, the other on a spring of 100 kN/m along the span. The
   tension solves the elastic catenary with the unstressed length and the
   weight of the case (from the rigid solve) on the chord shortened by
   the spring, \\( L(H) = 300 - (nH + F_{ref,x})/k \\), found by bisection.
   At −20 °C: rigid 22 965.2 N, level 3 22 524.0 N, the closed form the
   same to 2·10⁻¹⁴; the end moves 25.2 mm towards the span; sag 7.831 →
   7.984 m; 4 passes (mismatch 30 mm, 0.11 mm, 1.4·10⁻⁹ m, 2·10⁻¹⁶ m). At
   50 °C the structure springs back and tightens the span: 17 871.7 →
   18 054.0 N, sag 10.057 → 9.957 m (rigid sags more: the "hot" row
   below, measured). A bundle of two: the same per wire, the spring
   taking twice the force. Agreement 10⁻⁶ with a 10⁻⁷ m tolerance, 10⁻⁴
   with the default 0.1 mm.
2. **A span on a pole** — the same span, its end on a pole top of
   7 kN/m both ways (3EI/h³ of a 12 m pole, softer than the span along
   it), 20 mm of ice at −5 °C: rigid 49 231.7 N, level 3 30 537.2 N
   (−38 %), the closed form the same to 5·10⁻¹³; the top moves 1.505 m
   towards the span; sag 9.83 → 15.74 m; 6 passes.
3. **Two spans on a flexible suspension string, one span iced** — two
   300 m spans clipped in, a 3 m suspension string between them whose top
   sits on springs (50 kN/m along and across, 5 MN/m vertically), 15 mm of
   ice on the first span only. Reference: the same wire model solved at
   once with the string's top free and held by three bars of 100 km along
   the axes (EA = k L₀, unstressed so that they carry the stringing force
   at the drawn point; their sideways stiffness, force/length, below 10⁻⁵
   of k) — one Newton solve, no interface. The loop agrees to 2·10⁻⁸ m at
   the clamp, 0.001 N in the force on the string's top, 0.002 N in the
   tensions; 3 passes. **The unbalance relieved:** the longitudinal load on
   the structure 653.7 N rigid, 621.4 N flexible (−4.9 %, the string's
   swing already relieves most of it); the top moves 12.4 mm towards the
   iced span; the iced span sags 1.8 mm more, the bare one 2.1 mm less.
4. **The limits** — in the stringing state nothing moves (one pass, the
   sections the rigid solve's bit for bit); springs a million times stiffer
   give the rigid tensions to 10⁻⁶; no flexible structure: the rigid
   solve.
5. **Exit clauses** — the pole of case 2 stopped after 2 passes or by a
   zero time limit fails with the worst point, its direction and the
   trend; a deliberately soft top (1 N/m vertically) fails cleanly after
   8 passes, and given its 20 finds the real equilibrium — the top sunk
   tens of metres, until its vertical load is back at the stringing
   state's — after many halved steps.
6. **The example line** — its lattice towers (a model) and its steel pole
   (an elasticity) flexible, the other structures rigid ("no structure
   model"); every case converged with u = C (F − F_ref) at every point
   to 0.1 mm, the rigid solve kept bit for bit. In still air, cold, the
   suspension structures move less than 0.1 mm (pass 0 is the answer); in
   the high-wind case the lattice towers' points move about 19 mm leeward,
   the pole's top about 0.2 m, the tensions change by at most 0.1 %
   (3 passes).

## Load factors

The interaction is solved at **unfactored** loads (the service state);
the load case's factors go on the redistributed forces for the structure
checks, as today (owner 2026-11-15). At level 4 the structure check still
solves the factored loads with its own P-Delta. Every level 3 or 4 load
case will say: "structure flexibility at unfactored loads; factors on the
resulting forces".

## Safe side

**The owner's rule (2026-11-15): structure loads and usage are never below
rigid.** The structure check keeps, per structure, point and load case,
the **larger of the rigid and the flexible result**, and shows the change
against rigid. Every other result (sags, clearances, swings) takes the
flexible solve where it is the less favourable, or checks both — per
check, with the same rule: never less safe than rigid. *(The rule applies
from the phase that brings levels 3 and 4 to the checks; the parts built
so far change no result. Until then the level-3 run says with every
result: "NOT ON THE SAFE SIDE for the structures if taken from here: the
flexible loads may be lower than rigid — every structure check still uses
the rigid solve".)*

What rigid gives compared with the real, flexible structure, reasoned from
the mechanics ("to verify": still to be measured on a line with the
structure loads, plan phase 4; "measured": a hand case above):

| Quantity | Rigid gives | So rigid is | Sure? |
|---|---|---|---|
| Tension of dead-end and angle spans in cold or iced cases | higher | conservative for tension limits and structure loads | yes (measured: hand cases 1, 2) |
| Longitudinal load at the suspension tower next to an unbalanced load or a break | higher | conservative | yes (measured: hand case 3, −4.9 %) |
| Longitudinal load at towers farther from a break or unbalance | possibly lower (the load spreads further) | **possibly not conservative** | to verify |
| Loads on the other phases' points of a tower with a broken wire | none (no coupling) | **not conservative** for those points and the torsion they cause | yes (qualitatively) |
| Vertical (weight span) loads | change with the neighbours' relative vertical movement | either way, small | to verify |
| Transverse (wind) loads | nearly unchanged | neutral | yes |
| Sag at a tension above the stringing tension (cold, ice, wind) | smaller | **not conservative** for clearances below the wires | yes |
| Sag at a tension below the stringing tension (hot) | larger (the structure springs back and tightens the span) | conservative (slightly) | yes (measured: hand case 1, 0.10 m at 50 °C) |
| Sag of the loaded span in unbalanced ice | smaller | **not conservative** | yes (measured: hand case 3) |
| Clearances of the remaining spans after a break | smaller sags | **not conservative** | yes |
| Ground clearance under arms that deflect down | conductors higher | **not conservative** | yes |
| Horizontal clearances under wind | conductors closer to the tower line | **not conservative** leeward, conservative windward | yes |
| Longitudinal string swing in unbalanced cases | larger | conservative for swing limits | yes |
| Internal clearances (to the body) | body and point both undeflected | about neutral; the tower's twist under a one-sided load to verify | to verify |
| Member usage | rigid forces | conservative for most members, not for those loaded by the redistribution | to verify |

The page [Finite-element wire systems](wirefe.md) says flexibility "raises
the sags (on the unsafe side for clearances)": true above the stringing
tension and in unbalanced cases; in hot cases slightly the other way
(measured, hand case 1).

What the flexible levels themselves leave out, all stiffer than reality
(so loads on the safe side, deflections not): rigid foundations in the
model; a pole fixed at the ground line with rigid crossarms; the
deflection already present at stringing; an offset at a joint only bars
meet taken as the joint's translation; a fixed post's tilt; level 3's
linearisation at the stringing state, stiffer than the case's state where
members would go slack under its load. Each is said where it applies.

## Cost (measured, 2026-11-15)

Condensation, tangent at a reference with P-Delta, unoptimised test build
unless said:

- the example pylon (178 members, 2 points): 7 ms;
- a generated lattice tower of 1008 truss members, 24 points (72 unit
  solves): 0.12 s;
- imported real towers of 2190–2410 members (frames, about 900 joints,
  17–35 points), optimised build: 0.47–1.2 s each, of which the reference
  solve itself takes 0.27–0.66 s; the rest is one more factorisation of the
  tangent (about 5000 equations) and the unit solves.

Done once per structure type, extension and reference state and kept by
the key `CondenseKey` (the type, the extension, a fingerprint of the
model, the points and their links, and the linearisation — an edited
model is another key); per placed structure only the turn of the matrices
(microseconds). At the stringing state the linearisation holds the
structure's own wires' forces, so in practice each placed structure is
condensed once (identical ones shared), over the cores. A line with ten towers of such size: some 5–10 s for all
its matrices — more than the plan's first estimate (under a second),
because a real tower with frames factorises in about 0.1 s, not
milliseconds. The wire side's stiffness costs one evaluation of the
elements and one solve of the free nodes' block per section (well under a
millisecond).

**The loop** (level 3, optimised build without link-time optimisation,
the example line with its five weather cases, 2026-11-15): the setup —
the stringing state's solve and the three condensations — 7 ms; per case
1 to 3 passes, 8–36 ms against 7–12 ms for the rigid solve: about two to
three rigid solves per case, as the plan estimated (3–5×). Each pass
solves the whole line again; a real line's pass costs more than its rigid
solve where that comes from the memo (the moved sections are solved
afresh, with their stiffness), so the estimate printed after pass 0
(pass 0 × 4) can be low there — measured times are printed with it. The
interface system is held dense (3 × flexible points square: about
90 MB for a thousand points), formed and eliminated within its band.

## Sources

- R. J. Guyan, "Reduction of stiffness and mass matrices", *AIAA
  Journal*, vol. 3, no. 2, p. 380, 1965 — the reduction of a structure's
  stiffness to chosen degrees of freedom (static condensation).
- A. H. Peyrot, A. M. Goulois, "Analysis of flexible transmission lines",
  *Journal of the Structural Division*, ASCE, vol. 104, no. ST5, 1978 —
  an early analysis of a line's cables together with flexible supports
  (cited as the literature names it; not read here).
- The skyline \\( \mathbf L\mathbf D\mathbf L^T \\) solver and the element stiffnesses: see
  [Structure analysis](structures-fe.md#solver).
