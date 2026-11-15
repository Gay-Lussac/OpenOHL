# Structure flexibility in the line solve (calculation levels 3 and 4)

*Partly built, in active development (not approved yet; plan approved by
the owner 2026-11-15).* Built (internal, no setting, no result uses them
yet): the condensation of a structure to its attachment points
(`ohl_structures_fe::condense`), of a pole (`condense_pole`), of a placed
structure in the line's axes (`ohl-loads`, `flexibility`), and on the wire
side a section solved with its structure points moved and its stiffness
at those points (`ohl_wirefe::Model::support_stiffness`,
`ohl_line::solve::Coupling::moved`, `SupportStiffness`). **In active
development, not built yet:** the iteration that couples structures and
wires, calculation levels 3 and 4 as results, the line setting, the
command-line flag and every window. Today every line result is solved
with the structures rigid (calculation level 2), as
[Finite-element wire systems](wirefe.md) says.

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
*(The loop itself is not built yet; the parts it calls are.)*

The unknowns are the displacements **u** of the attachment points of the
flexible structures (three each, line axes); rigid is u = 0.

1. **Pass 0 is today's rigid solve**: every section solved with u = 0
   gives the forces **F** the wires put on each attachment point (the
   whole bundle's). Every result can therefore show its change against
   rigid at no extra cost.
2. **The structures answer**: level 3, \\( \mathbf u_s = \mathbf C\,(\mathbf F - \mathbf F_{ref}) \\)
   with C a structure's flexibility; level 4, the structure's non-linear
   solve under F less its state at the reference.
3. **Mismatch** \\( \mathbf r = \mathbf u - \mathbf u_s \\).
4. **Newton step** on all attachment displacements of the line at once.
   With \\( \mathbf K_w \\) the wires' stiffness at the attachment points
   (how their forces change when the points move, below) the step solves
   \\( (\mathbf I + \mathbf C\,\mathbf K_w)\,\Delta\mathbf u = -\mathbf r \\)
   — the flexibility form, which needs no inverse of C (equivalently
   \\( (\mathbf K_s + \mathbf K_w)\,\Delta\mathbf u = -\mathbf K_s\,\mathbf r \\)
   with \\( \mathbf K_s = \mathbf C^{-1} \\) where C has an inverse; it does
   not when two points cannot move apart, below).
5. **Move the attachment points** by Δu, solve the sections again (in
   parallel), back to 2, until every point's mismatch is below the
   tolerance and the forces no longer change.

Only the forces must be exact; \\( \mathbf K_w \\) may be approximate — it
then costs passes, never accuracy. Planned exit clauses: a number of
passes, a time limit as a guard, the cancel token, each section's and each
structure's own budget; a case that does not converge fails, naming the
worst point and the trend (owner 2026-11-15), and the rigid results stay
available, said rigid.

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
stiffness across them", "its model did not solve at the reference: …".
Ruling-span sections are solved on rigid structures; their forces still
load the structures (owner 2026-11-15).

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
so far change no result.)*

What rigid gives compared with the real, flexible structure, reasoned from
the mechanics ("to verify": a test of the level-3 phase will measure it):

| Quantity | Rigid gives | So rigid is | Sure? |
|---|---|---|---|
| Tension of dead-end and angle spans in cold or iced cases | higher | conservative for tension limits and structure loads | yes |
| Longitudinal load at the suspension tower next to an unbalanced load or a break | higher | conservative | yes |
| Longitudinal load at towers farther from a break or unbalance | possibly lower (the load spreads further) | **possibly not conservative** | to verify |
| Loads on the other phases' points of a tower with a broken wire | none (no coupling) | **not conservative** for those points and the torsion they cause | yes (qualitatively) |
| Vertical (weight span) loads | change with the neighbours' relative vertical movement | either way, small | to verify |
| Transverse (wind) loads | nearly unchanged | neutral | yes |
| Sag at a tension above the stringing tension (cold, ice, wind) | smaller | **not conservative** for clearances below the wires | yes |
| Sag at a tension below the stringing tension (hot) | larger (the structure springs back and tightens the span) | conservative (slightly) | to verify |
| Sag of the loaded span in unbalanced ice | smaller | **not conservative** | yes |
| Clearances of the remaining spans after a break | smaller sags | **not conservative** | yes |
| Ground clearance under arms that deflect down | conductors higher | **not conservative** | yes |
| Horizontal clearances under wind | conductors closer to the tower line | **not conservative** leeward, conservative windward | yes |
| Longitudinal string swing in unbalanced cases | larger | conservative for swing limits | yes |
| Internal clearances (to the body) | body and point both undeflected | about neutral; the tower's twist under a one-sided load to verify | to verify |
| Member usage | rigid forces | conservative for most members, not for those loaded by the redistribution | to verify |

The page [Finite-element wire systems](wirefe.md) says flexibility "raises
the sags (on the unsafe side for clearances)": true above the stringing
tension and in unbalanced cases; in hot cases slightly the other way (to
be measured).

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
(microseconds). A line with ten towers of such size: some 5–10 s for all
its matrices — more than the plan's first estimate (under a second),
because a real tower with frames factorises in about 0.1 s, not
milliseconds. The wire side's stiffness costs one evaluation of the
elements and one solve of the free nodes' block per section (well under a
millisecond).

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
