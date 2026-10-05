# Sag-tension

Implemented in `crates/ols-sagtension`. Every formula below is checked by validation cases in `benchmarks/sagtension/` against independent Python references (different formulas or numerical methods, see each `reference_*.py`).

## Catenary of one span

Left support at $\( (0,0) \)$, right support at \\( (S,h) \\); uniform load \\( w \\) per unit cable length; horizontal tension \\( H \\); catenary parameter \\( C = H/w \\). With \\( a = S/2C \\):

$\[ x_m = \frac{S}{2} - C \operatorname{asinh}\frac{h}{2C\sinh a}, \qquad
   y(x) = 2C \sinh\frac{x - 2x_m}{2C} \sinh\frac{x}{2C} \]$

\\[ L = \sqrt{h^2 + (2C\sinh a)^2}, \qquad T(x) = H\cosh\frac{x - x_m}{C}, \qquad T_B - T_A = w\,h \\]

The product forms avoid cancellation for taut spans. Maximum sag is where the cable is parallel to the chord: \\( x^\* = x_m + C\operatorname{asinh}(h/S) \\). For taut spans the sag tends to \\( wS^2 / (8H\cos\theta) \\) (\\( \theta \\): chord angle — the weight acts per unit of cable length).

*Validation:* `catenary-001…004`.

## Change of state

A cable has a fixed unstressed length \\( L_0 \\) at a reference temperature \\( t_0 \\). Under temperature \\( t \\) and load \\( w \\):

\\[ L = L_0\, e^{\alpha (t - t_0)} \left(1 + \frac{\bar T}{EA}\right), \qquad
   \int_0^L T\,ds = H\left[\frac{S}{2} + \frac{C}{2}\sinh\frac{S}{C}\cosh\frac{S - 2x_m}{C}\right] \\]

The exponential thermal factor is the exact integral of \\( dL/(L\,dT) = \alpha \\). The common additive form \\( 1 + \alpha\Delta t + \bar T/EA \\) agrees to second order but is **not path independent**: defining a span at 15 °C, solving at 75 °C, re-defining from that result and solving back gave 28.009 kN instead of 28 kN (span slack magnifies tiny length errors). The multiplicative form makes every state give the same unstressed length.

The length mismatch decreases strictly with \\( H \\): unique solution, found by bracketed bisection (limit: 200 iterations; typical: ~60 iterations, a few µs; the time limit only guards against a loop).

*Validation:* `cos-001…003`.

### Thermal strain: two forms in use

OLS CAD combines thermal and elastic strain in two ways, depending on the calculation. Both are standard, they agree to first order, and the difference is small — but it is there, and it is stated here so results can be traced.

| Form | Formula | Used by |
|---|---|---|
| Multiplicative | \\( L = L_0\, e^{\alpha\Delta t}\,(1 + \bar T/EA) \\) | the linear-elastic change of state and tension sections above; the finite-element wire model (every *FE* calculation of a section: the unstressed lengths are scaled by \\( e^{\alpha\Delta t} \\), then stretched elastically); jumpers |
| Additive | \\( \varepsilon = \varepsilon_{mech}(\bar T) + \alpha\Delta t \\), \\( L = L_0 (1 + \varepsilon) \\) | the non-linear stress-strain model (below) and so every *RS* (ruling-span) calculation of a section; each component's mechanical strain is \\( \varepsilon - \alpha_k \Delta t \\) |

**Why two.** The multiplicative form is exact and path independent (see above). The additive form is how published stress-strain curves are defined and used — total strain is mechanical strain plus \\( \alpha\Delta t \\) — and it is what PLS-CADD and the Aluminum Association method use, so the non-linear model keeps it to stay comparable with them.

**How large the difference is.** Expanding both: \\( e^{\alpha\Delta t}(1+\varepsilon_e) - (1 + \alpha\Delta t + \varepsilon_e) \approx (\alpha\Delta t)^2/2 + \alpha\Delta t\,\varepsilon_e \\). For an ACSR at 80 °C strung at 20 °C (\\( \alpha\Delta t \approx 1.2\times10^{-3} \\)) and an elastic strain of \\( 10^{-3} \\), that is about \\( 2\times10^{-6} \\): 0.6 mm on a 300 m span, a few millimetres of sag. In hot cases a section's FE and RS results therefore differ by up to about 0.1–0.2 % in tension for this reason alone (the validation compares them within 0.2 %). In cold cases \\( \Delta t < 0 \\) and the difference changes sign.

**Choosing.** The FE and RS methods of a section are both available in the Sections table (*Initial/Creep/Load/Max Sag × FE/RS*); the difference above is part of the difference between them, alongside what really differs (strings swinging and unequal span tensions in FE). Unifying the two forms is planned once comparisons with PLS-CADD on real lines say which to keep.

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

## Non-linear stress-strain and creep

Implemented in `ols_cables::mechanics` and `ols_sagtension::nonlinear`, following the published polynomial method (Aluminum Association practice; the approach of IEEE and CIGRE sag-tension methods). Each component (outer / core) may carry:

- an **initial** curve: stress as a polynomial of strain (typically 4th order, % strain, psi or MPa, on the component or total area — converted to SI load on loading);
- a **creep** curve (typically 10-year);
- a **final modulus** (unloading and reloading).

Components share the total strain; component *k* has mechanical strain \\( \varepsilon - \alpha_k (t - t_0) \\) and the cable tension is the sum of the component loads. The outer layers can be limited in compression (aluminium unloads onto the core at high temperature: the "knee point").

A section keeps one unstressed length (zero strain, curves' reference temperature) and each span stretches with the strain at its mean tension:

\\[ \sum_i \frac{L_i(H)}{1 + \varepsilon(\bar T_i(H), t, s)} = L_0 \\]

**Final condition.** The cable is strung on its initial curve. The permanent set *after load* is computed from the heaviest load case on the initial curve, the permanent set *after creep* from the creep case on the creep curve; the final state is the one leaving the cable longer, elastic along the final modulus (and back onto the initial curve if loaded beyond). "Longer" is compared at a working tension (the creep case's), and the permanent stretch passed to the FE model is measured at the stringing tension — not at zero tension, where outer strands that take no compression are slack and only the core's set would count (about half the stretch of the synthetic test cable). At the governing heaviest case, final and initial tensions are therefore equal — a consistency check used in the validation.

Curves are cable **data**: OLS CAD ships no manufacturer curves. The repository's non-linear validation cases use a **synthetic** cable (invented curves) against an independent re-implementation; comparison with published examples (CIGRE TB 324 etc.) is planned.

*Validation:* `nonlinear-001…002`.

## Not yet included

Tension criteria and the finite-element elements still use the linear-elastic cable; time-based creep models; published-case validation of the non-linear method. Results must be verified by a qualified engineer.
