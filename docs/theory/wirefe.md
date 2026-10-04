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
- **Tolerance**: \\( 10^{-9} \\) of the loads, but not below the floating-point floor \\( 64\,\varepsilon\, k_{max} |x| \\) of stiff members.
- **Exit clauses**: 100 iterations (the time limit, 120 s, only guards against a solve that never ends); the error names the node with the largest unbalanced force. `OLS_FE_TRACE=1` prints the iteration history.

## Sections: sagging and clamping

A section (dead-end to dead-end, suspension I-strings or V-strings) is built as in the field: every span is sagged at the stringing tension with the strings **plumb** (at their stretched length under the clamp load), which fixes each span's unstressed length. Any other state — temperature (\\( L_0 e^{\alpha\Delta t} \\); the ruling-span method uses the additive form, see [Thermal strain: two forms in use](sagtension.md#thermal-strain-two-forms-in-use)), ice, wind on cable and insulators, point loads at predefined sites, broken spans — is then solved with insulator swing, unequal span tensions and 3D blowout.

In a line model (`ols-line`) this is a section **clipped in**. A section still **in the blocks** (the default while designing) rolls instead: in each weather case its total unstressed length, from the sagging condition (a tension, or a sag in the ruling span or in one span), is shared out between the spans so that all have the same horizontal tension — the length of each span for a tension \( H \) under the case's load is found exactly, and \( H \) is the root of \( \sum_i L_{0,i}(H) = L_0 \) (bracketed secant). The clamps settle where the strings put them (swung across the line at angles, plumb along it, where equal tensions cancel); the sharing is repeated until they stop moving (≤ 1 µm, 20 passes at most). Point loads are left out of the sharing.

What this shows that the equal-tension (ruling span) model cannot: in the example job, support 3 is a **valley structure** (a 25 m descent into it, a 40 m rise out of it). Its weight span is only ~55 m, so in high wind its string swings ~89° — a real design issue (V-string, hold-down weights or a taller structure).

## Not yet included

Strain insulator length at dead-ends, structure flexibility, conductor bundles, non-linear stress-strain and creep in the elements, dynamic effects.
