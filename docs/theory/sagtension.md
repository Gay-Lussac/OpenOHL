# Sag-tension

Implemented in `crates/ohl-sagtension`. Every formula below is checked by validation cases in `benchmarks/sagtension/` against independent Python references (different formulas or numerical methods, see each `reference_*.py`).

## Catenary of one span

Left support at \\( (0,0) \\), right support at \\( (S,h) \\); uniform load \\( w \\) per unit wire length; horizontal tension \\( H \\); catenary parameter \\( C = H/w \\). With \\( a = S/2C \\):

\\[ x_m = \frac{S}{2} - C \operatorname{asinh}\frac{h}{2C\sinh a}, \qquad
   y(x) = 2C \sinh\frac{x - 2x_m}{2C} \sinh\frac{x}{2C} \\]

\\[ L = \sqrt{h^2 + (2C\sinh a)^2}, \qquad T(x) = H\cosh\frac{x - x_m}{C}, \qquad T_B - T_A = w\\,h \\]

The product forms avoid cancellation for taut spans. Maximum sag is where the wire is parallel to the chord: \\( x^\* = x_m + C\operatorname{asinh}(h/S) \\). For taut spans the sag tends to \\( wS^2 / (8H\cos\theta) \\) (\\( \theta \\): chord angle — the weight acts per unit of wire length).

*Validation:* `catenary-001…004`; textbook cases `tbcat-001…005` (the parabola's error: 0.08 % of sag at C = 2000 m on 400 m, 2 % at C = 1000 m on 1000 m; a low point beyond the support) and CIGRE TB 324's examples `tb324-001…010`, `tb324-015` ([Textbook and published cases](../validation/textbook.md)).

## Change of state

A wire has a fixed unstressed length \\( L_0 \\) at a reference temperature \\( t_0 \\). Under temperature \\( t \\) and load \\( w \\):

\\[ L = L_0\\, e^{\alpha (t - t_0)} \left(1 + \frac{\bar T}{EA}\right), \qquad
   \int_0^L T\\,ds = H\left[\frac{S}{2} + \frac{C}{2}\sinh\frac{S}{C}\cosh\frac{S - 2x_m}{C}\right] \\]

The exponential thermal factor is the exact integral of \\( dL/(L\\,dT) = \alpha \\). The common additive form \\( 1 + \alpha\Delta t + \bar T/EA \\) agrees to first order but is **not path independent**: defining a span at 15 °C, solving at 75 °C, re-defining from that result and solving back gave 28.009 kN instead of 28 kN (span slack magnifies tiny length errors). The multiplicative form makes every state give the same unstressed length.

The length mismatch decreases strictly with \\( H \\): unique solution, found by bracketed bisection (limit: 200 iterations; typical: ~60 iterations, a few tens of µs; the time limit, 10 s, only guards against a loop).

### The wire's own weight is conserved

*2026-10-21, owner decision.* A wire keeps its mass when it lengthens: when heated, stretched by its tension, after creep or a permanent set. Its weight per metre \\( w \\) (the data's mass per length times g) is per metre of **unstressed** wire at the reference temperature — the known (stringing) state's — so a span weighs \\( wL_0 \\) in every state. The catenary above carries its load per metre of stretched wire; the own weight is therefore spread over the span's length:

\\[ w_L = \frac{w L_0}{L} = \frac{w}{k}, \qquad k = e^{\alpha (t - t_0)} \left(1 + \frac{\bar T}{EA}\right) \\]

(the additive form below: \\( k = (1 + \varepsilon)/(1 + \varepsilon_w) \\), with \\( \varepsilon_w \\) the strain at zero tension at the known state's temperature, the length that carries \\( w \\) per metre). As \\( \bar T \\) depends on the load, \\( k \\) is a fixed point solved for each trial tension: it changes the load by about \\( 10^{-3} \\) and the mean tension by about \\( 10^{-5} \\) of that, so it settles in two or three passes (exit clause: 50). Ice and wind are loads per metre of the **real** conductor and stay as given; point loads are forces. A load state therefore says which part of its load is the wire's own weight and which part is wind (`LoadState::weight`, `wind`; the rest of the vertical load is ice).

Before this, the same load per metre of stretched wire was taken in every state, so a span's weight grew with its length: from the stringing state by \\( L/L_{strung} - 1 \\) (+0.25 % at 150 °C for 300 m strung at 25 kN at 10 °C), and the stringing state itself carried \\( w \\) per stretched metre, \\( \bar T/EA \\) (about 0.08 %) more than the wire weighs. The effect on that span: H −0.12 % at 70 °C, −0.28 % at 150 °C, +0.007 % with 12 mm of ice at −5 °C; the sag 5–6 mm less in every case (most of it the stringing state's weight). The finite elements carried their weight per unstressed metre already, but not its thermal lengthening ([Finite-element wire systems](wirefe.md#elastic-catenary-element)); with both corrected, the closed form and the finite elements agree to \\( 10^{-6} \\) on a single span (`tbheat-001…002`), and creep as a temperature shift gives the same tension by both methods to 0.0004 % (`tbcreep-001…002`, 0.008 % before).

**Simplification:** the own weight spread evenly over the stretched span. The wire stretches more where its tension is higher (towards the supports), so its mass per stretched metre varies along the span by \\( \Delta T/EA \\) (about \\( 10^{-5} \\)); the catenary takes the mean. Either way, below \\( 10^{-6} \\) of the tension (−0.9·10⁻⁷ on a level 300 m span under 12 mm of ice, against an integration of the loads following the local stretch, `tbice-001`). Exact: the elastic catenary (the finite elements).

**Ice and wind per metre of the stretched wire** (checked 2026-10-21 with the finite elements' correction, [Finite-element wire systems](wirefe.md#elastic-catenary-element)): every closed form carries the ice and wind as given per metre of its catenary, which is the stretched wire — the single span and the section (`LoadState::load_at`), the loaded span (`LoadedSpan`, its base load per wire length) and the non-linear section — so they were already exact there; only the own weight is spread (above). The finite elements now carry them per stretched metre too: on a level 300 m span under 12 mm of ice the two agree to 4.2·10⁻⁷ (0.018 N of 42571 N; 2.8·10⁻⁸ on 100 m), each method's even spread of one load being the rest. The finite elements' option per *unstretched* metre (Criteria › Calculation options, 2026-10-21: as other programs; a Simplification, about 0.1 % less ice and wind) does not apply to the closed forms, which keep the stretched metre ([Finite-element wire systems](wirefe.md#elastic-catenary-element)).

### The weight from the data sheet

*2026-10-21, owner decision.* A wire's mass per metre is its data sheet's, at the data sheet's reference temperature \\( t_{ref} \\): the wire library's optional `reference_temperature` of the wire type, else 20 °C ([Wire library](../user-guide/wire-library.md)). A metre of unstressed wire at another temperature holds another share of the same mass, so at the stringing temperature \\( t_s \\) — the known state, from which the weight is conserved (above) — the own weight per metre is

\\[ w_s = w_{ref}\\, e^{-\alpha (t_s - t_{ref})} \\]

in the multiplicative thermal form (the linear change of state, the finite elements of a wire without curves; composite \\( \alpha \\)), and its equivalent in the additive form of the stress-strain curves (the ruling-span method, the non-linear model, the finite elements of a wire with curves):

\\[ w_s = w_{ref}\\, \frac{1 + \varepsilon(0, t_{ref})}{1 + \varepsilon(0, t_s)} \\]

with \\( \varepsilon(0, t) \\) the curves' strain at zero tension (initial state; \\( \alpha (t - 20\\,°C) \\) for a linear wire). The two differ by \\( (\alpha \Delta t)^2 \\), about 10⁻⁸. The line (`ohl_line::solve::strung_cable`, per section at its stringing temperature; jumpers at their own temperature, `jumper::cut_cable`), the CLI's sag-tension job (at its reference weather case) and the spotting (at its condition's temperature) take the weight this way; the engines take the weight as given at their known state (`LoadState::weight`). The textbook wire strung at 10 °C weighs 0.020 % more than its data sheet's 20 °C value; on the example line (strung at 15 °C) the tensions grow by up to 0.009 % in still air and the sags by up to 0.9 mm. Hand-checked by `tbwref-001` (the closed form, the weight given at 20 °C) and `tbwref-002` (the finite elements, the library's mass at 40 °C) ([Textbook and published cases](../validation/textbook.md)).

*Validation:* `cos-001…003`; `tbcos-001…005` (temperature, ice, wind, 150 °C, an inclined span; the parabolic change-of-state cubic agrees within 0.05 % on level spans), `tbheat-001` (the weight kept at 150 °C), `tb324-016` (CIGRE TB 324 sec. 6.1, within the printed rounding) ([Textbook and published cases](../validation/textbook.md)).

### Thermal strain: two forms in use

OpenOHL combines thermal and elastic strain in two ways, depending on the calculation. Both are standard, they agree to first order, and the difference is small — but it is there, and it is stated here so results can be traced.

| Form | Formula | Used by |
|---|---|---|
| Multiplicative | \\( L = L_0\\, e^{\alpha\Delta t}\\,(1 + \bar T/EA) \\) | the linear-elastic change of state and tension sections above; the finite-element wire model for wires without stress-strain curves (every *FE* calculation of such a section: the unstressed lengths are scaled by \\( e^{\alpha\Delta t} \\), then stretched elastically); jumpers |
| Additive | \\( \varepsilon = \varepsilon_{mech}(\bar T) + \alpha\Delta t \\), \\( L = L_0 (1 + \varepsilon) \\) | the non-linear stress-strain model (below) and so every *RS* (ruling-span) calculation of a section, and the *FE* calculation of a wire with curves (each span's length follows the curves, see [Finite-element wire systems](wirefe.md#non-linear-stress-strain-in-the-elements)); each component's mechanical strain is \\( \varepsilon - \alpha_k \Delta t \\) |

**Why two.** The multiplicative form is exact and path independent (see above). The additive form is how published stress-strain curves are defined and used — total strain is mechanical strain plus \\( \alpha\Delta t \\) — and it is what the Aluminum Association method and common line design software use, so the non-linear model keeps it to stay comparable with them.

**How large the difference is.** Expanding both: \\( e^{\alpha\Delta t}(1+\varepsilon_e) - (1 + \alpha\Delta t + \varepsilon_e) \approx (\alpha\Delta t)^2/2 + \alpha\Delta t\\,\varepsilon_e \\). For an ACSR at 80 °C strung at 20 °C (\\( \alpha\Delta t \approx 1.2\times10^{-3} \\)) and an elastic strain of \\( 10^{-3} \\), that is about \\( 2\times10^{-6} \\): 0.6 mm on a 300 m span, a few millimetres of sag. In hot cases the FE and RS results of a section whose wire has no curves therefore differ by up to about 0.1–0.2 % in tension for this reason alone (the validation compares them within 0.2 %). In cold cases \\( \Delta t < 0 \\) and the difference changes sign.

**Simplification:** two thermal strain forms, as above. The additive form gives a slightly shorter wire when hot (less sag, more tension) and a slightly longer one when cold; about 10⁻⁶ of strain, a few millimetres of sag. Exact: the multiplicative form everywhere (planned once comparisons say which to keep).

**Choosing.** The FE and RS methods of a section are both available in the Sections table (*Initial/Creep/Load/Max Sag × FE/RS*); the difference above is part of the difference between them, alongside what really differs (strings swinging and unequal span tensions in FE). Unifying the two forms is planned once comparisons with other programs on real lines say which to keep.

## Tension sections

Suspension strings equalise the horizontal tension along a section. OpenOHL solves the section exactly: one \\( H \\) for all spans, total unstressed length conserved:

\\[ \sum_i \frac{L_i(H)}{1 + \bar T_i(H)/EA} = L_0\\, e^{\alpha (t - t_0)} \\]

each span carrying its own weight spread over its length ([above](#the-wires-own-weight-is-conserved): one \\( H \\), a load per metre of its own per span). The classic ruling span \\( S_r = \sqrt{\sum S_i^3 / \sum S_i} \\) is also reported. For the inclined 4-span validation section it is 0.116 % off in tension; the exact solution costs about 60 µs for 20 spans.

*Validation:* `section-001`; `tbrs-001` (the ruling span 0.0045 % off on three level spans), `tbrs-004` (the line's ruling-span method) ([Textbook and published cases](../validation/textbook.md)). *Not yet modelled:* string length changing the effective span lengths.

**Simplification:** one horizontal tension for the whole section, the strings plumb and of no length (the ruling-span model). Unequal spans, inclined spans and swinging strings make the span tensions and sags differ; the error goes either way per span. Exact: the finite-element section ([Finite-element wire systems](wirefe.md)), available as the *FE* calculations.

One case for all spans: a section whose spans carry different weather
cases (one span iced, unequal ice) is solved by the finite elements only
([A weather case per span](wirefe.md#a-weather-case-per-span-2026-10-20));
the ruling span refuses it.

## Tension criteria

Limits (% RTS support or horizontal tension, maximum \\( H \\), maximum \\( H/w \\) with \\( w \\) the case's load per metre) apply in given load states. Raising the stringing tension raises the tension in every state, so each limit gives one maximum stringing tension; the governing one is the smallest. Nested bisection, exit clauses at both levels.

**On the non-linear model** (2026-10-22). A wire with stress-strain curves takes its criteria on the non-linear model, each in a **condition**: *initial* (as strung), *after creep* or *after load*. The unknown is the stringing tension (horizontal, at the reference case, on the initial curve). Each trial stringing tension is a full calculation: the section strung, both permanent sets taken from it (the creep case on the creep curve, the heaviest load case on the initial curve), the criterion's case solved in its condition. The sets therefore follow the stringing tension, and the governing criterion is found **by iteration, not in one pass**: a bisection per criterion gives its largest stringing tension; the smallest is checked against every criterion; when one is above its limit there (a set that grows faster than the tension), its bisection runs again below it; at most 10 passes (an error names the criterion beyond that). Hand checks: a limit on the initial state at the reference case gives the limit itself (`nlcrit-001`); after load at the load-stretch case equals the initial state there (`nlcrit-003` = `nlcrit-004`).

**Simplification:** a wire without curves (the linear models) takes its criteria on the linear-elastic wire, without creep or permanent set; a condition after creep or after load is refused for it. For a wire whose final state is longer than its elastic one, the final tensions are lower and the sags larger than computed here. Exact: the wire's curves, with the conditions above.

*Validation:* `criteria-001`; `nlcrit-001…005` (an invented cable, against an independent re-implementation).

## Weather loads

Radial ice of thickness \\( t \\) and density \\( \rho_i \\) on diameter \\( d \\): \\( w_i = \rho_i g \frac{\pi}{4}[(d+2t)^2 - d^2] \\). Wind pressure \\( q \\) on the iced diameter: \\( w_h = q(d + 2t) \\). Resultant \\( \sqrt{(w_c + w_i)^2 + w_h^2} \\), in a plane swung by \\( \arctan(w_h / w_v) \\). Sag-tension uses the resultant in the span plane (exact for level spans).

**Simplification:** in an inclined span the swung plane of the resultant is not the plane through the chord, so the plane-curve solution is approximate there; the error grows with the incline and the wind, either way. Exact: the 3D finite-element span (`ohl-wirefe`), which solves any load direction.

**Simplification:** ice as a concentric cylinder of uniform thickness along the whole span. Real ice is often eccentric (on the windward or top side) and uneven along the span; the weight is what the case gives, but the wind area and the twist of the wire differ. Exact: an ice shape per case (not modelled); uneven ice along a span can be given as distributed loads (below). No design-code factors are applied here: load factors belong to the load cases ([Load cases](../user-guide/loads.md)), and design-code plugins generate weather and load cases ([Design codes](../user-guide/design-codes.md)).

*Validation:* `weather-001`; `tbwx-001…003` (ice per metre, wind on the iced diameter, blowout), `tb324-014` ([Textbook and published cases](../validation/textbook.md)).

## Point and distributed loads

Between loads the wire is a catenary with the common \\( H \\); with \\( y' = \sinh\psi \\), \\( \psi \\) grows linearly in each segment, and a point load \\( P \\) makes the slope jump: \\( \sinh\psi^+ = \sinh\psi^- + P/H \\). The start angle is found by bisection so the wire reaches the far support; the change of state iterates on \\( H \\) around it. Vertical loads only: transverse and longitudinal components are rejected rather than ignored. The 3D finite-element model (`ohl-wirefe`) takes loads in any direction at its nodes; point loads in a line file are weights, and bodies with a size (marker balls) also take wind and ice (see [Point loads: marker balls and other bodies](wirefe.md#point-loads-marker-balls-and-other-bodies)).

*Validation:* `loaded-001…002` against an RK4 integration of the wire ODE; `tbpl-001…002` (the slope jump P/H; closed form for a load at mid-span) ([Textbook and published cases](../validation/textbook.md)).

## Non-linear stress-strain and creep

Implemented in `ohl_cables::mechanics` and `ohl_sagtension::nonlinear`, following the published polynomial method (Aluminum Association practice; the approach of IEEE and CIGRE sag-tension methods). Each component (outer / core) may carry:

- an **initial** curve: stress as a polynomial of strain (typically 4th order, % strain, psi or MPa, on the component or total area — converted to SI load on loading);
- a **creep** curve (typically 10-year);
- a **final modulus** (unloading and reloading).

Components share the total strain; component *k* has mechanical strain \\( \varepsilon - \alpha_k (t - t_0) \\) and the wire tension is the sum of the component loads. The outer layers can be limited in compression (aluminium unloads onto the core at high temperature: the "knee point").

A section keeps one unstressed length (zero strain, curves' reference temperature) and each span stretches with the strain at its mean tension:

\\[ \sum_i \frac{L_i(H)}{1 + \varepsilon(\bar T_i(H), t, s)} = L_0 \\]

and span \\( i \\) carries \\( w (1 + \varepsilon_w)/(1 + \varepsilon_i) \\) per metre of its length ([the wire's own weight is conserved](#the-wires-own-weight-is-conserved)): thermal strain, creep and the permanent set add length, not weight. A linear model's creep stretch lengthens \\( L_0 \\) and lowers \\( 1 + \varepsilon_w \\) by the same factor.

**Final condition.** The wire is strung on its initial curve. The permanent set *after load* is computed from the heaviest load case on the initial curve, the permanent set *after creep* from the creep case on the creep curve; each final state is elastic along the final modulus from its set (and back onto the initial curve if loaded beyond). The permanent stretch passed to the FE model is measured at the stringing tension — not at zero tension, where outer strands that take no compression are slack and only the core's set would count (about half the stretch of the synthetic test wire).

**The permanent set is signed** (2026-10-22). Component \\( k \\), brought to mechanical strain \\( e_k \\) and load \\( P_k \\) on its curve, keeps the set

\\[ \varepsilon_{p,k} = e_k - \frac{P_k}{EA_{f,k}} \\]

and in the final state carries \\( \min\big(EA_{f,k}(\varepsilon_k - \varepsilon_{p,k}),\\; P_{init,k}(\varepsilon_k)\big) \\), with \\( P_{init,k} \\) the initial state (the initial curve, and the final modulus from zero below zero strain). The line passes through the point reached, so reloaded to the tension of the set's own case the wire is back on the initial curve: **after load, final and initial tensions are equal at the load-stretch case** (a check used in the validation, `nlstate-004`, to \\( 10^{-6} \\)). A component whose initial curve rises more steeply than its final modulus near zero strain (some steel cores: \\( P_k(e) > EA_{f,k}\\, e \\)) gets a slightly **negative** set. Before 2026-10-22 the set was clamped at zero: the final line then lay under the initial curve at the very point it starts from, and the final tension came out below the initial at the load-stretch case (by 0.14 % on a test cable since removed; `nlstate-004` checks it on an invented core of 1950 MPa/% initially against 1900 MPa/% final, a set of −0.002 %). Storing the stretch as a load (the line through the curve's point at that load) is the same line; the signed strain was kept because the final state, the FE model and the reports already carry a strain per component.

**Both final states; the governing one per case** (2026-10-22). Both final states are solved in every weather case, and **the one leaving the wire longer in that case governs**: the lower horizontal tension, so the larger sags. Which one that is can change with the case: on the invented test cable, after load governs at 15 °C and 50 °C and at 25 N/m, after creep at 80 °C — and at the load-stretch case itself, where after load equals the initial state and the creep set is the larger (`nlstate-001…005`). A tie (within \\( 10^{-9} \\) of the tension, both states on the initial curve) goes to the load. One state for the whole section, where one is needed (the FE model's permanent stretch, the old single output), is the longer at the creep case's tension. The line's *Max Sag* calculation takes, per weather case, whichever of the two sags more — the same rule.

**Conservative:** the final state takes, per weather case, the larger of the two permanent sets (after creep, after the heaviest load), not their combination over time. The longer wire gives the larger sag: on the safe side for clearances (a section's *Max Sag* calculation takes, per weather case, whichever of the two states sags more); for tensions the shorter wire of the initial state gives the higher values, and that state is solved on its own. Exact: a time-based history of loads and creep (not modelled).

**The curves' range.** A stress-strain polynomial is valid up to its data's `max_strain` (optional; old files read unchanged). **Only a range the cable gives is warned about** (owner, 2026-10-22): without `max_strain` there is no warning (until then a 1 % default was taken); the curves must still rise up to 1 % for the strain to be found from the tension. Beyond it OpenOHL still evaluates the polynomial — it does not invent an extrapolation — but the results **warn**: the sag-tension validation output `beyond_range`, `ohl sagtension` ("Warning: … beyond its curves' range"), the app's warnings per weather case (the ruling-span and finite-element solutions) and its Sagging panel. The check takes each span's most tensioned support, whose strain is a little higher than the mean tension's the length is computed from: it warns slightly early, on the safe side (`nonlinear-003…004`).

**Above the wire's strength** (owner, 2026-10-22). Whatever the range warnings do, a solved state in which a span's tension at its **most tensioned support** (the larger end tension of the span, per wire) exceeds the wire type's rated tensile strength (RTS) is **always warned about**: "span 2: tension 141.20 kN at the most tensioned support exceeds the wire's rated tensile strength (RTS 138.78 kN, 101.7 %)" — in every solver of the line (ruling span and finite elements: `solve::rts_flags`, the app's warnings by weather case and the Sections table's "above RTS", `ohl line`) and in `ohl sagtension` (the closed forms: each weather case, and on the non-linear model each of the initial and both final states). A warning, not a check against a design limit: the limits a code sets are the tension rules' ([Wire tensions](wire-tensions.md)) or the criteria's. *Tests:* `a_tension_above_the_rts_is_warned_in_every_solver`, `a_tension_above_the_rts_is_warned`.

**A slack cable's strain at zero tension** (2026-10-22). When every component is limited to no compression, a slack cable carries no tension over a range of strains; its strain at zero tension — the length that carries the own weight per metre — is the upper end of that range, where it starts to carry tension. It was the lower end of the search bracket before (−0.1 %, by accident), which made such a cable's own weight per metre 0.1 % too low (on a test cable with both components without compression, since removed, −0.03 % to +0.04 % in tension).

**Simplification:** creep is the creep curve's (typically 10-year) set at one creep weather case. Creep goes on beyond the curve's period and depends on the tension history; the curve's period must match the line's life. Exact: a time-based creep law (not modelled).

**Simplification:** the linear wire models of the [wire library](../user-guide/wire-library.md#wire-models) (`linear_creep_tension`, `linear_creep_temperature`) take creep as an equivalent temperature rise and have no permanent set after load. After a heavy ice or wind load the real wire is longer: sags are underestimated and tensions overestimated. Exact: the non-linear model with the wire's curves.

Curves are wire type **data**: OpenOHL ships no manufacturer curves. The repository's non-linear validation cases use a **synthetic** wire and an **invented** one (invented curves, not real conductors), each against an independent re-implementation. A cross-check on curves of unstated source was removed on 2026-10-22 (the owner's decision: no data in the examples and checks that is not ours to use). Comparison with published examples (CIGRE TB 324 etc.) waits for curves from an admissible source.

*Validation:* `nonlinear-001…006`; `nlstate-001…005` (the invented cable: both final states, the governing one per case, after load equal to initial at the load-stretch case with a negative set); `tbbim-001…002` (a linear bimetallic wire whose outer strands take no compression, below and above its knee point, 140.85 °C) ([Textbook and published cases](../validation/textbook.md)).

The finite-element model follows the same curves: each span's unstressed length is corrected until its length at the span's mean tension is the curves' length in the case's material state, see [Finite-element wire systems](wirefe.md#non-linear-stress-strain-in-the-elements).

## Sagging outputs

*2026-10-22.* What the stringing crew measures, from the states already solved — post-processing only, in `ohl_sagtension::sagging`; `ohl sagtension`'s `[sagging]` and the section window's **Sagging** panel ([Sagging outputs](../user-guide/sagging.md)). The section is in the blocks, initial state, bare, still air, at the conductor temperature of sagging. Each formula is derived here; the textbook formulas and a public-domain library are used only as cross-checks.

**Stopwatch (return wave).** A transverse wave runs along a cable at \\( c = \sqrt{T/\mu} \\) (tension \\( T \\), mass per metre \\( \mu \\); from the wave equation of a string, \\( \mu\\,\partial^2 y/\partial t^2 = T\\,\partial^2 y/\partial s^2 \\)). Along the catenary \\( T = H\cosh u \\), \\( ds = \cosh u\\,dx \\), \\( u = (x - x_m)/C \\), and \\( \mu = q/g \\) with \\( q \\) the span's load per metre (the own weight spread over the stretched wire). A pulse struck at one support and seen back after \\( n \\) round trips takes

\\[ t_n = 2n \int_0^L \frac{ds}{c} = 2n \sqrt{\frac{q}{gH}} \int_0^S \sqrt{\cosh\frac{x - x_m}{C}}\\,dx \\]

(composite Simpson, 1024 intervals). For a taut level span (\\( T \approx H \\), \\( L \approx S \\), \\( D = qS^2/8H \\)) this is the field formula \\( t_n = 2n\sqrt{8D/g} \\), i.e. \\( D = g t_n^2/(32 n^2) \\): +0.03 % on the time at 300 m, 28 kN (`sag-001`). **Simplification:** the wave speed of a taut string, \\( \sqrt{T/\mu} \\): the cable's bending stiffness and the sag's coupling of transverse and longitudinal waves are left out, as in the field method. A public-domain library takes \\( H \\) for \\( T \\) along the whole arc (+0.06 % here, a slower wave).

**Dynamometer.** It reads the tension along the wire at the clamp where it is inserted: \\( T_A = H\cosh(x_m/C) \\), \\( T_B = H\cosh((S - x_m)/C) \\); \\( T_B - T_A = qh \\) exactly (`sag-002`).

**Transit (angle of sight).** An instrument at \\( (x_i, z_i) \\) in the span's plane (below the wire), the line of sight tangent to the wire at \\( x_t \\): the tangent \\( z = y(x_t) + y'(x_t)(x - x_t) \\) passes through the instrument when

\\[ f(x_t) = y(x_t) + y'(x_t)(x_i - x_t) - z_i = 0, \qquad f'(x_t) = y''(x_t)(x_i - x_t) < 0 \quad (x_t > x_i) \\]

so beyond the instrument there is one tangent point (bisection). The angle is \\( \arctan y'(x_t) \\); the target goes where the line meets the far support's vertical, \\( z_i + y'(x_t)(S - x_i) \\). Hand check, the parabola of a level span with the instrument \\( a \\) below one support and the target \\( b \\) below the other: \\( \sqrt a + \sqrt b = 2\sqrt D \\), tangent at \\( x_t = S\sqrt{a/4D} \\) (`sag-001`: the parabola 0.01 % off the exact catenary). An instrument above the wire, or a tangent beyond the far support, is refused with the reason.

**Inclined-span correction.** A level-span sag chart gives \\( D_{level} = C(\cosh(S/2C) - 1) \\); the inclined span's sag (vertical, where the wire is parallel to the chord) is larger, about \\( D_{level}/\cos\theta \\) since the load is per metre of wire. The correction is the exact difference, \\( D - D_{level} \\) (`sag-002`: 0.0883 m on 300 m +50 m; \\( D_{level}(1/\cos\theta - 1) \\) gives 0.0885 m).

**Stringing temperature.** The conductor is rarely at the chart's temperature; the sag per kelvin is the central difference of the section solved at \\( t \pm 0.5 \\) K (each solve exact). Hand check, the parabola of a linear wire: from \\( L = S + 8D^2/3S \\), \\( dL = S(\alpha\\,dt + dT/EA) \\) and \\( T \approx qS^2/8D \\),

\\[ \frac{dD}{dt} = \frac{S\alpha}{16D/3S + qS^3/(8D^2 EA)} \\]

(`sag-001`: 0.037345 m/K by the parabola, 0.037429 m/K exact).

**Clipping offsets.** A section sagged in the blocks at one condition keeps, per span, the unstressed lengths \\( L_{0,j}^{sag} \\) of that state; clipped in at another (the conductor at another temperature), the wire has rolled through the blocks to \\( L_{0,j}^{clip} \\) — in the engine's in-the-blocks model, one horizontal tension through the section, a long span taking more slack as \\( H \\) falls (about \\( q^2 S^3\\,dH/12H^3 \\) per span). At structure \\( i \\) (after span \\( i \\)) the clamp point lies

\\[ \delta_i = \sum_{j \le i} \left(L_{0,j}^{sag} - L_{0,j}^{clip}\right) \\]

along the unstressed wire from the point at the block, positive into the span ahead; both sets add up to the section's unstressed length, so \\( \delta \\) is zero at the far dead-end (`sag-003`: +30.7 mm and −18.0 mm at the two suspension structures of 200/350/250 m, sagged at 15 °C, clipped at 35 °C). Clipped at the sagging temperature the offsets are zero in this model.

**Simplification:** in the blocks the wire keeps one horizontal tension through the section (the engine's model, unchanged). Real sheaves pass the tension *along the wire* (equal support tension on both sides of a frictionless sheave, differing from equal \\( H \\) by about \\( q\\,\Delta h \\) per inclined span) and have friction; on inclined spans that gives sag corrections and clipping offsets of a few millimetres even at the sagging temperature, which this model does not give. Exact: a sheave model of the in-the-blocks state — **not wanted (owner, 2026-10-22)**: the simplification stays.

**Simplification:** the clipping offsets are given along the unstressed wire; along the stretched wire they are \\( 1 + \varepsilon \\) (about \\( 10^{-3} \\)) longer — micrometres.

*Validation:* `sag-001…003` (hand derivations above and an independent reference with other numerics: Gauss–Legendre in the arc parameter, Newton for the tangent).

## Not yet included

- Time-based creep models.
- A sheave model in the blocks (equal support tension across frictionless sheaves, sheave friction): **not wanted (owner, 2026-10-22)**; the in-the-blocks simplification stays.
- Validation of the non-linear method against published cases: waits for published reference data (no tables typed from memory); see the roadmap (open point O11).

Results must be verified by a qualified engineer.
