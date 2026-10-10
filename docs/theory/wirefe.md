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
- **Swing-arc projection**: a straight Newton step moves a swinging clamp along the tangent of its arc; with a nearly rigid string, the fake stretch \\( \delta^2/2L \\) creates huge forces. The step keeps Newton's intended string length but puts the clamp back on its arc.
- **Line search** on the largest unbalanced nodal force.
- **Tolerance**: \\( 10^{-9} \\) of the loads, but not below the floating-point floor \\( 64\\,\varepsilon\\, k_{max} |x| \\) of stiff members.
- **Exit clauses**: 100 iterations (the time limit, 120 s, only guards against a solve that never ends); the error names the node with the largest unbalanced force. `OLS_FE_TRACE=1` prints the iteration history.

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

In a line model the section's ends, and middle points whose wire point is not below the insulator point, are **strain assemblies**: a free clamp on a member from the fixed insulator point, lined up with the conductor by equilibrium, so it moves with every weather case. A strain string given without a weight (geometry only, e.g. imported) gets the weight that holds it where the data puts it in the stringing state. A point whose wire and insulator points coincide is fixed. A **bundle** is solved as one subconductor carrying its share of the strings' weight and wind area; the string forces and reactions reported are the whole bundle's. **Jumpers** are solved separately, between the solved clamps, as inextensible elastic rods with bending stiffness (see [Jumpers](jumpers.md)).

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
