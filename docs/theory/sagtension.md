# Sag-tension

Implemented in `crates/ols-sagtension`. Every formula below is checked by validation cases in `benchmarks/sagtension/` against independent Python references (different formulas or numerical methods, see each `reference_*.py`).

## Catenary of one span

Left support at \\( (0,0) \\), right support at \\( (S,h) \\); uniform load \\( w \\) per unit cable length; horizontal tension \\( H \\); catenary parameter \\( C = H/w \\). With \\( a = S/2C \\):

\\[ x_m = \frac{S}{2} - C \operatorname{asinh}\frac{h}{2C\sinh a}, \qquad
   y(x) = 2C \sinh\frac{x - 2x_m}{2C} \sinh\frac{x}{2C} \\]

\\[ L = \sqrt{h^2 + (2C\sinh a)^2}, \qquad T(x) = H\cosh\frac{x - x_m}{C}, \qquad T_B - T_A = w\,h \\]

The product forms avoid cancellation for taut spans. Maximum sag is where the cable is parallel to the chord: \\( x^\* = x_m + C\operatorname{asinh}(h/S) \\). For taut spans the sag tends to \\( wS^2 / (8H\cos\theta) \\) (\\( \theta \\): chord angle — the weight acts per unit of cable length).

*Validation:* `catenary-001…004`.

## Change of state

A cable has a fixed unstressed length \\( L_0 \\) at a reference temperature \\( t_0 \\). Under temperature \\( t \\) and load \\( w \\):

\\[ L = L_0\, e^{\alpha (t - t_0)} \left(1 + \frac{\bar T}{EA}\right), \qquad
   \int_0^L T\,ds = H\left[\frac{S}{2} + \frac{C}{2}\sinh\frac{S}{C}\cosh\frac{S - 2x_m}{C}\right] \\]

The exponential thermal factor is the exact integral of \\( dL/(L\,dT) = \alpha \\). The common additive form \\( 1 + \alpha\Delta t + \bar T/EA \\) agrees to second order but is **not path independent**: defining a span at 15 °C, solving at 75 °C, re-defining from that result and solving back gave 28.009 kN instead of 28 kN (span slack magnifies tiny length errors). The multiplicative form makes every state give the same unstressed length.

The length mismatch decreases strictly with \\( H \\): unique solution, found by bracketed bisection (limits: 200 iterations, 50 ms; typical: ~60 iterations, a few µs).

*Validation:* `cos-001…003`.

## Tension sections

Suspension insulators equalise the horizontal tension along a section. OLS CAD solves the section exactly: one \\( H \\) for all spans, total unstressed length conserved:

\\[ \sum_i \frac{L_i(H)}{1 + \bar T_i(H)/EA} = L_0\, e^{\alpha (t - t_0)} \\]

The classic ruling span \\( S_r = \sqrt{\sum S_i^3 / \sum S_i} \\) is also reported. For the inclined 4-span validation section it is 0.116 % off in tension; the exact solution costs about 60 µs for 20 spans.

*Validation:* `section-001`. *Not yet modelled:* insulator string length changing the effective span lengths.

## Tension criteria

Limits (% RTS support or horizontal tension, maximum \\( H \\), maximum \\( H/w \\)) apply in given load states. Raising the stringing tension raises the tension in every state, so each limit gives one maximum stringing tension; the governing one is the smallest. Nested bisection, exit clauses at both levels.

*Validation:* `criteria-001`.

## Weather loads

Radial ice of thickness \\( t \\) and density \\( \rho_i \\) on diameter \\( d \\): \\( w_i = \rho_i g \frac{\pi}{4}[(d+2t)^2 - d^2] \\). Wind pressure \\( q \\) on the iced diameter: \\( w_h = q(d + 2t) \\). Resultant \\( \sqrt{(w_c + w_i)^2 + w_h^2} \\), in a plane swung by \\( \arctan(w_h / w_v) \\). Sag-tension uses the resultant in the span plane (exact for level spans). No design-code factors yet (Phase 4 plugins).

*Validation:* `weather-001`.

## Point and distributed loads

Between loads the cable is a catenary with the common \\( H \\); with \\( y' = \sinh\psi \\), \\( \psi \\) grows linearly in each segment, and a point load \\( P \\) makes the slope jump: \\( \sinh\psi^+ = \sinh\psi^- + P/H \\). The start angle is found by bisection so the cable reaches the far support; the change of state iterates on \\( H \\) around it. Vertical loads only for now; transverse components need the 3D finite-element wire model (Phase 4) and are rejected rather than ignored.

*Validation:* `loaded-001…002` against an RK4 integration of the cable ODE.

## Not yet included

Non-linear stress-strain (polynomial initial/final curves), creep, and final-after-load conditions — next Phase 1 steps. Results are linear-elastic and must be verified by a qualified engineer.
