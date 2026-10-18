# Finite-element wire systems

Implemented in `crates/ols-wirefe` — a "semi finite element" approach in the spirit of SAPS: each cable span is **one exact elastic catenary element** (no meshing, exact for any sag), insulator strings are truss elements, and the free nodes (clamps, load points, yoke plates) are found by Newton–Raphson in 3D.

## Elastic catenary element

Unstressed length \\( L_0 \\), axial stiffness \\( EA \\), uniform load \\( \mathbf q \\) per unit **unstressed** length (mass is conserved when the cable stretches). The cable lies in the plane of the chord and \\( \mathbf q \\). In that plane (\\( z \\) opposite to the load), with \\( w = |\mathbf q| \\), \\( W = wL_0 \\), and \\( H, V_0 \\) the tension components at node *i*, \\( V_1 = V_0 + W \\):

\\[ x = \frac{HL_0}{EA} + \frac{H}{w}\left[\operatorname{asinh}\frac{V_1}{H} - \operatorname{asinh}\frac{V_0}{H}\right], \qquad
   z = \frac{V_0L_0 + wL_0^2/2}{EA} + \frac{1}{w}\left[\sqrt{H^2+V_1^2} - \sqrt{H^2+V_0^2}\right] \\]

For given end positions, \\( (H, V_0) \\) come from a damped Newton iteration (exit clauses: 100 iterations, 20 ms). The element returns the end forces and the exact 3×3 tangent stiffness (in-plane: inverse of the flexibility matrix; out-of-plane: \\( H/x \\)), verified against finite differences.

*Validation:* `wirefe-001…003` against RK4 integration of the elastic cable along its unstressed length (with a tension jump for a point load).

## Truss elements

Insulator strings, links and V-string legs: axial force \\( N = \frac{EA}{L_0}(L - L_0) \\), tangent stiffness \\( \frac{EA}{L_0}\mathbf n\mathbf n^T + \frac{N}{L}(\mathbf I - \mathbf n\mathbf n^T) \\). Tension-only members (V legs) go slack when clearly shorter than \\( L_0 \\). Weight is lumped half at each end.

## Newton–Raphson

Unknowns: positions of the free nodes. Each iteration assembles the stiffness of every element and solves \\( (\mathbf K + \mu\mathbf I)\Delta\mathbf x = \mathbf R \\):

- **Levenberg–Marquardt damping** \\( \mu \\): negligible next to stiff string directions (a string picks up its load in one step) but keeps steps moderate where the structure is still soft — a string at zero force has no pendulum stiffness yet. \\( \mu \\) falls after good steps, so the end is plain Newton (quadratic convergence).
- **Swing-arc projection**: a straight Newton step moves a swinging clamp along the tangent of its arc; with a nearly rigid string, the fake stretch \\( \delta^2/2L \\) creates huge forces. The step keeps Newton's intended string length but puts the clamp back on its arc. A bar between two free nodes (a string hanging from a string) is projected the same way about its end nearer the supports (counted in bars; equally far: its top), with the length change of the ends' relative move. Nodes are placed from the supports outwards, each once the nodes it hangs from are: on its one bar's sphere, or — held by two bars (a V's tip, also the joint closing two chains of strings) — where the two spheres meet nearest to Newton's point; with more bars, a few alternating passes. A node all of whose bars went slack in a step drops back until its bar nearest the supports is taut (slack bars give it no stiffness: Newton cannot bring it back).
- **Line search** on the largest unbalanced nodal force; a step lowering the unbalanced force of all nodes together (root of the sum of squares) by 10 % is also taken — a chain of strings swinging as a whole lowers it while one joint's force may still grow.
- **Step limit for chained strings**: where strings hang from strings, a joint moves at most a third of its shortest string per step (a longer step can fold a chain the wrong way). Models without them step as before.
- **Tolerance**: \\( 10^{-9} \\) of the loads, but not below the floating-point floor \\( 64\\,\varepsilon\\, k_{max} |x| \\) of stiff members.
- **Exit clauses**: 100 iterations (the time limit, 120 s, only guards against a solve that never ends); the error names the node with the largest unbalanced force. `OLS_FE_TRACE=1` prints the iteration history.
- **Fallbacks** when Newton stops: (1) the cables under their vertical load alone, then the point loads and the sideways (wind) part of the cables' load ramped up together, each step from the last solution; (2) for models with strings hanging from strings only, **dynamic relaxation** — each free node a mass moving under its unbalanced force (masses from the stiffness's row sums, never decreasing, so every step is stable), the motion stopped at each peak of kinetic energy — to near equilibrium (10⁻³ of the loads), then Newton from there. Exit clauses of the relaxation: 50 000 steps or 5 s. A long chain in high wind (four strings on one side, five on the other, ending in a V) takes about 2 s this way.

## Sections: sagging and clamping

A section (dead-end to dead-end, suspension I-strings or V-strings) is built as in the field: every span is sagged at the stringing tension with the strings **plumb** (at their stretched length under the clamp load), which fixes each span's unstressed length. Any other state — temperature (\\( L_0 e^{\alpha\Delta t} \\); the ruling-span method uses the additive form, see [Thermal strain: two forms in use](sagtension.md#thermal-strain-two-forms-in-use)), ice, wind on cable and insulators, point loads at predefined sites, broken spans — is then solved with insulator swing, unequal span tensions and 3D blowout.

In a line model (`ols-line`) this is a section **clipped in**. A section still **in the blocks** (the default while designing) rolls instead: in each weather case its total unstressed length, from the sagging condition (a tension, or a sag in the ruling span or in one span), is shared out between the spans so that all have the same horizontal tension — the length of each span for a tension \\( H \\) under the case's load is found exactly, and \\( H \\) is the root of \\( \sum_i L_{0,i}(H) = L_0 \\) (bracketed secant). The clamps settle where the strings put them (swung across the line at angles, plumb along it, where equal tensions cancel); the sharing is repeated until they stop moving (≤ 1 µm, 20 passes at most). Point loads are left out of the sharing.

What this shows that the equal-tension (ruling span) model cannot: in the example job, support 3 is a **valley structure** (a 25 m descent into it, a 40 m rise out of it). Its weight span is only ~55 m, so in high wind its string swings ~89° — a real design issue (V-string, hold-down weights or a taller structure).

## Point loads: marker balls and other bodies

A point load in a span is a weight at a fraction of the span, and may also be a body of some size: a marker ball, an obstruction light. It's given a diameter \\( D \\) (taken as a sphere), a drag coefficient \\( C_d \\) (default 1), and optionally its wind area \\( A \\) or the surface \\( S \\) that takes the ice instead of the sphere's. In a weather case it carries:

- **Ice:** the wire's radial ice thickness \\( t \\) (the case's thickness, or with an ice load and an ice density the thickness that load makes on the wire's bare diameter), as a shell of \\( \frac{\pi}{6}\left((D+2t)^3 - D^3\right) \\), or a layer \\( S\,t \\) on a given surface, at the case's ice density. A case with an ice load but no density gives no thickness, so the body takes no ice.
- **Wind:** \\( F = p\,C_d\,\frac{\pi}{4}(D+2t)^2 \\) across the span, from the case's side (a given area \\( A \\) is grown by the ice as a sphere of that area).

On a bundle the forces are shared by its subconductors. These loads can matter: a 0.6 m ball takes about 180 N of wind at 650 Pa and, with 16 mm of ice, about 170 N of ice, several times its own weight. A dozen of them on a long earth-wire span add a quarter or more to the wind on the wire. Without their wind area a long marked span swings metres less than it should.

## Line sections: strain assemblies, bundles, jumpers

In a line model the strings holding the wire at each point are built as the structure gives them ([line file](../user-guide/line-file.md#points-strings-and-wire-points)): its points are fixed nodes, its wire points free nodes, its strings bars. One string from the structure in the middle of the section, hanging below its top, is a **suspension string**; the section's ends, and middle points held by one string not below its top, are **strain assemblies**: a free clamp on a member from the fixed insulator point, lined up with the conductor by equilibrium, so it moves with every weather case. A strain string given without a weight (geometry only, e.g. imported) gets the weight that holds it where the data puts it in the stringing state. A point whose wire and insulator points coincide is fixed. A **bundle** is solved as one subconductor carrying its share of the strings' weight and wind area; the string forces and reactions reported are the whole bundle's. **Jumpers** are solved separately, between the solved clamps, as inextensible elastic rods with bending stiffness (see [Jumpers](jumpers.md)).

## Strings hanging from strings

Any assembly is strings between fixed points (on the structure) and free wire points: a V with a string below has a free tip and a free clamp, a TAK a free node with two strain strings to two sections' clamps. Each string is a bar of its given length (top to wire point as given), tension-only unless it **may take compression**: it then acts both ways — compressed, its stiffness is the axial one without a geometric part, so it holds as a strut — and its force beyond the given compression is **flagged** with the solution (the solve does not change it: a buckled member is the designer's to change). Its weight and wind area go half to each end (wind across the line at the section point).

**Settling while sagging.** In the stringing reference and while a section in the blocks rolls, the clamps are moved back across the line after each solve: along the line equal tensions cancel at any position (a neutral direction the iteration would creep along), and the crew clamps in plumb along it. A suspension string swings back into the plane across the line keeping its radius; a V turns about the axis through its tops. Any other assembly is solved again on its own, from its solved shape, under its weights and the wires' pull on the clamp (what balances the strings and loads there) less its part along the line — exact for a V with a string below and every other shape.

**Sections sharing an assembly** (a TAK: the end of one section and the start of the next hang on the same strings) are solved together, Dirichlet–Neumann:

1. each section alone first, the other sections' clamps in its assemblies held where given;
2. the **stiffest** section through a shared assembly solves it with the other sections' wires pulling on their clamps (their last pull, as a load on those free nodes); the others hold their clamp where that solution puts it, which gives their pull for the next pass. Stiffest along the line at its end there: the least compliance \\( \sum_i L_i/EA + w^2 L_i^3 / 12 H_i^3 \\) over its spans as solved alone (the ruling span's: span lengths \\( L_i \\), horizontal tensions \\( H_i \\), the case's load per metre \\( w \\)); of equals, the first in the line's order;
3. the pulls on the held clamps are relaxed with Aitken's factor from the last two passes, half way at first, between 0.02 and 1;
4. until the pulls on the held clamps change less than 0.1 N (or 10⁻⁶ of themselves); not in 30 passes: an error for the sections of the group, naming the pull still changing and by how much.

Why the stiffest: a held section's pull changes by its stiffness times the move of its clamp, and that move is the pull's change over what holds the clamp back (the solving section and the strings). The pass multiplies a pull's error by minus the ratio of the two. Held, a short single span against a long section on a swinging string (on the second existing line, freshly imported) pulls some 20 times harder for a move of its clamp than the long section and the string hold it back: the pulls overshot in turn by hundreds of kN, the string swinging metres either way, and never settled, not with half steps nor with Aitken's (28 passes in still air, failing in other cases) nor with a secant (Anderson) step. Solving the TAK, the stiff span takes the long section's pull, and the pass multiplies the error by about −1/20: 5–15 passes in every case, both wind sides. (Taken whole, two similar sections' ends pull back and forth: a TAK damped by only 0.89 a pass, half way by about 0.06.)

The stringing reference of each section holds the other sections' clamps where given (the line as strung). The loads on the structure points of a shared assembly are reported once, by the section that solves it. Expected cost: a few solves of each section of the group (milliseconds); sections that share nothing are solved alone, as before.

## Non-linear stress-strain in the elements

A cable with stress-strain curves (the non-linear model of the [wire library](../user-guide/wire-library.md#cable-models)) keeps elastic catenary elements with its \\( EA \\), but the unstressed length of each span is corrected until the span follows the curves (`ols-line`, `solve_fe`):

1. The section is first solved in the **stringing state** (still air at the stringing temperature). Each span's mean tension there, through the initial curve, fixes the span's length at zero strain.
2. In the weather case, the section is solved; each span's mean tension \\( \bar T_s \\) (weighted by the elements' unstressed lengths) gives the curves' strain \\( \varepsilon(\bar T_s, t, \text{state}) \\) in the case's **material state** — initial, after creep or after load — so the permanent set comes with the state.
3. Each span's unstressed length is set so that its length at \\( \bar T_s \\) is the curves' length: the zero-strain length times \\( (1+\varepsilon) \\), divided by the element's own stretch \\( e^{\alpha\Delta t}(1 + \bar T_s/EA) \\).
4. The section is solved again with those lengths, until they change by less than \\( 10^{-9} \\) (relative). This secant iteration settles in 3–6 solves; it stops with an error after 30.

The curves' compression limit of the outer strands is part of the strain, so it replaces the bimetallic core branch used for linear cables (past the transition temperature, a linear bimetallic cable is solved as its core alone, with the outer strands' constant compressive load). Linear models are unchanged: they are elastic with \\( EA \\), longer by the final state's permanent stretch.

On a dead-end span with the wires fixed at the structures, FE and RS then agree within about 0.02 % in tension (the test allows 0.1 %) and 1 cm in sag, from −20 °C to 60 °C (the two methods measure sag on slightly different chords: 5 mm apart already at stringing). Since the curves define strain in the additive thermal form, the FE of a cable with curves follows that form; the FE of a cable without curves keeps the multiplicative one, see [Thermal strain: two forms in use](sagtension.md#thermal-strain-two-forms-in-use).

## Not yet included

Structure flexibility (the insulator points are fixed; the structures are analysed separately, see [Structure analysis](structures-fe.md)), general linked insulator assemblies (yokes and jumper support strings as one graph), dynamic effects.
