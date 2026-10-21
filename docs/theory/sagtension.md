# Sag-tension

Implemented in `crates/ohl-sagtension`. Every formula below is checked by validation cases in `benchmarks/sagtension/` against independent Python references (different formulas or numerical methods, see each `reference_*.py`).

## Catenary of one span

Left support at \\( (0,0) \\), right support at \\( (S,h) \\); uniform load \\( w \\) per unit cable length; horizontal tension \\( H \\); catenary parameter \\( C = H/w \\). With \\( a = S/2C \\):

\\[ x_m = \frac{S}{2} - C \operatorname{asinh}\frac{h}{2C\sinh a}, \qquad
   y(x) = 2C \sinh\frac{x - 2x_m}{2C} \sinh\frac{x}{2C} \\]

\\[ L = \sqrt{h^2 + (2C\sinh a)^2}, \qquad T(x) = H\cosh\frac{x - x_m}{C}, \qquad T_B - T_A = w\\,h \\]

The product forms avoid cancellation for taut spans. Maximum sag is where the cable is parallel to the chord: \\( x^\* = x_m + C\operatorname{asinh}(h/S) \\). For taut spans the sag tends to \\( wS^2 / (8H\cos\theta) \\) (\\( \theta \\): chord angle — the weight acts per unit of cable length).

*Validation:* `catenary-001…004`; textbook cases `tbcat-001…005` (the parabola's error: 0.08 % of sag at C = 2000 m on 400 m, 2 % at C = 1000 m on 1000 m; a low point beyond the support) and CIGRE TB 324's examples `tb324-001…010`, `tb324-015` ([Textbook and published cases](../validation/textbook.md)).

## Change of state

A cable has a fixed unstressed length \\( L_0 \\) at a reference temperature \\( t_0 \\). Under temperature \\( t \\) and load \\( w \\):

\\[ L = L_0\\, e^{\alpha (t - t_0)} \left(1 + \frac{\bar T}{EA}\right), \qquad
   \int_0^L T\\,ds = H\left[\frac{S}{2} + \frac{C}{2}\sinh\frac{S}{C}\cosh\frac{S - 2x_m}{C}\right] \\]

The exponential thermal factor is the exact integral of \\( dL/(L\\,dT) = \alpha \\). The common additive form \\( 1 + \alpha\Delta t + \bar T/EA \\) agrees to first order but is **not path independent**: defining a span at 15 °C, solving at 75 °C, re-defining from that result and solving back gave 28.009 kN instead of 28 kN (span slack magnifies tiny length errors). The multiplicative form makes every state give the same unstressed length.

The length mismatch decreases strictly with \\( H \\): unique solution, found by bracketed bisection (limit: 200 iterations; typical: ~60 iterations, a few tens of µs; the time limit, 10 s, only guards against a loop).

### The cable's own weight is conserved

*2026-10-21, owner decision.* A cable keeps its mass when it lengthens: when heated, stretched by its tension, after creep or a permanent set. Its weight per metre \\( w \\) (the data's mass per length times g) is per metre of **unstressed** cable at the reference temperature — the known (stringing) state's — so a span weighs \\( wL_0 \\) in every state. The catenary above carries its load per metre of stretched cable; the own weight is therefore spread over the span's length:

\\[ w_L = \frac{w L_0}{L} = \frac{w}{k}, \qquad k = e^{\alpha (t - t_0)} \left(1 + \frac{\bar T}{EA}\right) \\]

(the additive form below: \\( k = (1 + \varepsilon)/(1 + \varepsilon_w) \\), with \\( \varepsilon_w \\) the strain at zero tension at the known state's temperature, the length that carries \\( w \\) per metre). As \\( \bar T \\) depends on the load, \\( k \\) is a fixed point solved for each trial tension: it changes the load by about \\( 10^{-3} \\) and the mean tension by about \\( 10^{-5} \\) of that, so it settles in two or three passes (exit clause: 50). Ice and wind are loads per metre of the **real** conductor and stay as given; point loads are forces. A load state therefore says which part of its load is the cable's own weight and which part is wind (`LoadState::weight`, `wind`; the rest of the vertical load is ice).

Before this, the same load per metre of stretched cable was taken in every state, so a span's weight grew with its length: from the stringing state by \\( L/L_{strung} - 1 \\) (+0.25 % at 150 °C for 300 m strung at 25 kN at 10 °C), and the stringing state itself carried \\( w \\) per stretched metre, \\( \bar T/EA \\) (about 0.08 %) more than the cable weighs. The effect on that span: H −0.12 % at 70 °C, −0.28 % at 150 °C, +0.007 % with 12 mm of ice at −5 °C; the sag 5–6 mm less in every case (most of it the stringing state's weight). The finite elements carried their weight per unstressed metre already, but not its thermal lengthening ([Finite-element wire systems](wirefe.md#elastic-catenary-element)); with both corrected, the closed form and the finite elements agree to \\( 10^{-6} \\) on a single span (`tbheat-001…002`), and creep as a temperature shift gives the same tension by both methods to 0.0004 % (`tbcreep-001…002`, 0.008 % before).

**Simplification:** the own weight spread evenly over the stretched span. The cable stretches more where its tension is higher (towards the supports), so its mass per stretched metre varies along the span by \\( \Delta T/EA \\) (about \\( 10^{-5} \\)); the catenary takes the mean. Either way, below \\( 10^{-6} \\) of the tension (−0.9·10⁻⁷ on a level 300 m span under 12 mm of ice, against an integration of the loads following the local stretch, `tbice-001`). Exact: the elastic catenary (the finite elements).

**Ice and wind per metre of the stretched cable** (checked 2026-10-21 with the finite elements' correction, [Finite-element wire systems](wirefe.md#elastic-catenary-element)): every closed form carries the ice and wind as given per metre of its catenary, which is the stretched cable — the single span and the section (`LoadState::load_at`), the loaded span (`LoadedSpan`, its base load per cable length) and the non-linear section — so they were already exact there; only the own weight is spread (above). The finite elements now carry them per stretched metre too: on a level 300 m span under 12 mm of ice the two agree to 4.2·10⁻⁷ (0.018 N of 42571 N; 2.8·10⁻⁸ on 100 m), each method's even spread of one load being the rest.

### The weight from the data sheet

*2026-10-21, owner decision.* A cable's mass per metre is its data sheet's, at the data sheet's reference temperature \\( t_{ref} \\): the wire library's optional `reference_temperature` of the cable, else 20 °C ([Wire library](../user-guide/wire-library.md)). A metre of unstressed cable at another temperature holds another share of the same mass, so at the stringing temperature \\( t_s \\) — the known state, from which the weight is conserved (above) — the own weight per metre is

\\[ w_s = w_{ref}\\, e^{-\alpha (t_s - t_{ref})} \\]

in the multiplicative thermal form (the linear change of state, the finite elements of a cable without curves; composite \\( \alpha \\)), and its equivalent in the additive form of the stress-strain curves (the ruling-span method, the non-linear model, the finite elements of a cable with curves):

\\[ w_s = w_{ref}\\, \frac{1 + \varepsilon(0, t_{ref})}{1 + \varepsilon(0, t_s)} \\]

with \\( \varepsilon(0, t) \\) the curves' strain at zero tension (initial state; \\( \alpha (t - 20\\,°C) \\) for a linear cable). The two differ by \\( (\alpha \Delta t)^2 \\), about 10⁻⁸. The line (`ohl_line::solve::strung_cable`, per section at its stringing temperature; jumpers at their own temperature, `jumper::cut_cable`), the CLI's sag-tension job (at its reference weather case) and the spotting (at its condition's temperature) take the weight this way; the engines take the weight as given at their known state (`LoadState::weight`). The textbook cable strung at 10 °C weighs 0.020 % more than its data sheet's 20 °C value; on the example line (strung at 15 °C) the tensions grow by up to 0.009 % in still air and the sags by up to 0.9 mm. Hand-checked by `tbwref-001` (the closed form, the weight given at 20 °C) and `tbwref-002` (the finite elements, the library's mass at 40 °C) ([Textbook and published cases](../validation/textbook.md)).

*Validation:* `cos-001…003`; `tbcos-001…005` (temperature, ice, wind, 150 °C, an inclined span; the parabolic change-of-state cubic agrees within 0.05 % on level spans), `tbheat-001` (the weight kept at 150 °C), `tb324-016` (CIGRE TB 324 sec. 6.1, within the printed rounding) ([Textbook and published cases](../validation/textbook.md)).

### Thermal strain: two forms in use

OpenOHL combines thermal and elastic strain in two ways, depending on the calculation. Both are standard, they agree to first order, and the difference is small — but it is there, and it is stated here so results can be traced.

| Form | Formula | Used by |
|---|---|---|
| Multiplicative | \\( L = L_0\\, e^{\alpha\Delta t}\\,(1 + \bar T/EA) \\) | the linear-elastic change of state and tension sections above; the finite-element wire model for cables without stress-strain curves (every *FE* calculation of such a section: the unstressed lengths are scaled by \\( e^{\alpha\Delta t} \\), then stretched elastically); jumpers |
| Additive | \\( \varepsilon = \varepsilon_{mech}(\bar T) + \alpha\Delta t \\), \\( L = L_0 (1 + \varepsilon) \\) | the non-linear stress-strain model (below) and so every *RS* (ruling-span) calculation of a section, and the *FE* calculation of a cable with curves (each span's length follows the curves, see [Finite-element wire systems](wirefe.md#non-linear-stress-strain-in-the-elements)); each component's mechanical strain is \\( \varepsilon - \alpha_k \Delta t \\) |

**Why two.** The multiplicative form is exact and path independent (see above). The additive form is how published stress-strain curves are defined and used — total strain is mechanical strain plus \\( \alpha\Delta t \\) — and it is what the Aluminum Association method and common line design software use, so the non-linear model keeps it to stay comparable with them.

**How large the difference is.** Expanding both: \\( e^{\alpha\Delta t}(1+\varepsilon_e) - (1 + \alpha\Delta t + \varepsilon_e) \approx (\alpha\Delta t)^2/2 + \alpha\Delta t\\,\varepsilon_e \\). For an ACSR at 80 °C strung at 20 °C (\\( \alpha\Delta t \approx 1.2\times10^{-3} \\)) and an elastic strain of \\( 10^{-3} \\), that is about \\( 2\times10^{-6} \\): 0.6 mm on a 300 m span, a few millimetres of sag. In hot cases the FE and RS results of a section whose cable has no curves therefore differ by up to about 0.1–0.2 % in tension for this reason alone (the validation compares them within 0.2 %). In cold cases \\( \Delta t < 0 \\) and the difference changes sign.

**Simplification:** two thermal strain forms, as above. The additive form gives a slightly shorter cable when hot (less sag, more tension) and a slightly longer one when cold; about 10⁻⁶ of strain, a few millimetres of sag. Exact: the multiplicative form everywhere (planned once comparisons say which to keep).

**Choosing.** The FE and RS methods of a section are both available in the Sections table (*Initial/Creep/Load/Max Sag × FE/RS*); the difference above is part of the difference between them, alongside what really differs (strings swinging and unequal span tensions in FE). Unifying the two forms is planned once comparisons with other programs on real lines say which to keep.

## Tension sections

Suspension insulators equalise the horizontal tension along a section. OpenOHL solves the section exactly: one \\( H \\) for all spans, total unstressed length conserved:

\\[ \sum_i \frac{L_i(H)}{1 + \bar T_i(H)/EA} = L_0\\, e^{\alpha (t - t_0)} \\]

each span carrying its own weight spread over its length ([above](#the-cables-own-weight-is-conserved): one \\( H \\), a load per metre of its own per span). The classic ruling span \\( S_r = \sqrt{\sum S_i^3 / \sum S_i} \\) is also reported. For the inclined 4-span validation section it is 0.116 % off in tension; the exact solution costs about 60 µs for 20 spans.

*Validation:* `section-001`; `tbrs-001` (the ruling span 0.0045 % off on three level spans), `tbrs-004` (the line's ruling-span method) ([Textbook and published cases](../validation/textbook.md)). *Not yet modelled:* insulator string length changing the effective span lengths.

**Simplification:** one horizontal tension for the whole section, the strings plumb and of no length (the ruling-span model). Unequal spans, inclined spans and swinging strings make the span tensions and sags differ; the error goes either way per span. Exact: the finite-element section ([Finite-element wire systems](wirefe.md)), available as the *FE* calculations.

One case for all spans: a section whose spans carry different weather
cases (one span iced, unequal ice) is solved by the finite elements only
([A weather case per span](wirefe.md#a-weather-case-per-span-2026-10-20));
the ruling span refuses it.

## Tension criteria

Limits (% RTS support or horizontal tension, maximum \\( H \\), maximum \\( H/w \\)) apply in given load states. Raising the stringing tension raises the tension in every state, so each limit gives one maximum stringing tension; the governing one is the smallest. Nested bisection, exit clauses at both levels.

**Simplification:** the criteria in `ohl sagtension` use the linear-elastic cable (no creep or permanent set from the stress-strain curves). For a cable whose final state is longer than its elastic one, the final tensions are lower and the sags larger than computed here. Exact: the criteria on the non-linear model in each state.

*Validation:* `criteria-001`.

## Weather loads

Radial ice of thickness \\( t \\) and density \\( \rho_i \\) on diameter \\( d \\): \\( w_i = \rho_i g \frac{\pi}{4}[(d+2t)^2 - d^2] \\). Wind pressure \\( q \\) on the iced diameter: \\( w_h = q(d + 2t) \\). Resultant \\( \sqrt{(w_c + w_i)^2 + w_h^2} \\), in a plane swung by \\( \arctan(w_h / w_v) \\). Sag-tension uses the resultant in the span plane (exact for level spans).

**Simplification:** in an inclined span the swung plane of the resultant is not the plane through the chord, so the plane-curve solution is approximate there; the error grows with the incline and the wind, either way. Exact: the 3D finite-element span (`ohl-wirefe`), which solves any load direction.

**Simplification:** ice as a concentric cylinder of uniform thickness along the whole span. Real ice is often eccentric (on the windward or top side) and uneven along the span; the weight is what the case gives, but the wind area and the twist of the wire differ. Exact: an ice shape per case (not modelled); uneven ice along a span can be given as distributed loads (below). No design-code factors are applied here: load factors belong to the load cases ([Load cases](../user-guide/loads.md)), and design-code plugins generate weather and load cases ([Design codes](../user-guide/design-codes.md)).

*Validation:* `weather-001`; `tbwx-001…003` (ice per metre, wind on the iced diameter, blowout), `tb324-014` ([Textbook and published cases](../validation/textbook.md)).

## Point and distributed loads

Between loads the cable is a catenary with the common \\( H \\); with \\( y' = \sinh\psi \\), \\( \psi \\) grows linearly in each segment, and a point load \\( P \\) makes the slope jump: \\( \sinh\psi^+ = \sinh\psi^- + P/H \\). The start angle is found by bisection so the cable reaches the far support; the change of state iterates on \\( H \\) around it. Vertical loads only: transverse and longitudinal components are rejected rather than ignored. The 3D finite-element model (`ohl-wirefe`) takes loads in any direction at its nodes; point loads in a line file are weights, and bodies with a size (marker balls) also take wind and ice (see [Point loads: marker balls and other bodies](wirefe.md#point-loads-marker-balls-and-other-bodies)).

*Validation:* `loaded-001…002` against an RK4 integration of the cable ODE; `tbpl-001…002` (the slope jump P/H; closed form for a load at mid-span) ([Textbook and published cases](../validation/textbook.md)).

## Non-linear stress-strain and creep

Implemented in `ohl_cables::mechanics` and `ohl_sagtension::nonlinear`, following the published polynomial method (Aluminum Association practice; the approach of IEEE and CIGRE sag-tension methods). Each component (outer / core) may carry:

- an **initial** curve: stress as a polynomial of strain (typically 4th order, % strain, psi or MPa, on the component or total area — converted to SI load on loading);
- a **creep** curve (typically 10-year);
- a **final modulus** (unloading and reloading).

Components share the total strain; component *k* has mechanical strain \\( \varepsilon - \alpha_k (t - t_0) \\) and the cable tension is the sum of the component loads. The outer layers can be limited in compression (aluminium unloads onto the core at high temperature: the "knee point").

A section keeps one unstressed length (zero strain, curves' reference temperature) and each span stretches with the strain at its mean tension:

\\[ \sum_i \frac{L_i(H)}{1 + \varepsilon(\bar T_i(H), t, s)} = L_0 \\]

and span \\( i \\) carries \\( w (1 + \varepsilon_w)/(1 + \varepsilon_i) \\) per metre of its length ([the cable's own weight is conserved](#the-cables-own-weight-is-conserved)): thermal strain, creep and the permanent set add length, not weight. A linear model's creep stretch lengthens \\( L_0 \\) and lowers \\( 1 + \varepsilon_w \\) by the same factor.

**Final condition.** The cable is strung on its initial curve. The permanent set *after load* is computed from the heaviest load case on the initial curve, the permanent set *after creep* from the creep case on the creep curve; the final state is the one leaving the cable longer, elastic along the final modulus (and back onto the initial curve if loaded beyond). "Longer" is compared at a working tension (the creep case's), and the permanent stretch passed to the FE model is measured at the stringing tension — not at zero tension, where outer strands that take no compression are slack and only the core's set would count (about half the stretch of the synthetic test cable). At the governing heaviest case, final and initial tensions are therefore equal — a consistency check used in the validation.

**Conservative:** the final state takes the larger of the two permanent sets (after creep, after the heaviest load), not their combination over time. The longer cable gives the larger sag: on the safe side for clearances (and a section's *Max Sag* calculation takes, per weather case, whichever of the two states sags more); for tensions the shorter cable of the initial state gives the higher values, and that state is solved on its own. Exact: a time-based history of loads and creep (not modelled).

**Simplification:** creep is the creep curve's (typically 10-year) set at one creep weather case. Creep goes on beyond the curve's period and depends on the tension history; the curve's period must match the line's life. Exact: a time-based creep law (not modelled).

**Simplification:** the linear cable models of the [wire library](../user-guide/wire-library.md#cable-models) (`linear_creep_tension`, `linear_creep_temperature`) take creep as an equivalent temperature rise and have no permanent set after load. After a heavy ice or wind load the real cable is longer: sags are underestimated and tensions overestimated. Exact: the non-linear model with the cable's curves.

Curves are cable **data**: OpenOHL ships no manufacturer curves. The repository's non-linear validation cases use a **synthetic** cable (invented curves) against an independent re-implementation; comparison with published examples (CIGRE TB 324 etc.) is planned.

*Validation:* `nonlinear-001…002`; `tbbim-001…002` (a linear bimetallic cable whose outer strands take no compression, below and above its knee point, 140.85 °C) ([Textbook and published cases](../validation/textbook.md)).

The finite-element model follows the same curves: each span's unstressed length is corrected until its length at the span's mean tension is the curves' length in the case's material state, see [Finite-element wire systems](wirefe.md#non-linear-stress-strain-in-the-elements).

## Not yet included

- Tension criteria in `ohl sagtension` still use the linear-elastic cable.
- Time-based creep models.
- Validation of the non-linear method against published cases: waits for published reference data (no tables typed from memory); see the roadmap (open point O11).

Results must be verified by a qualified engineer.
