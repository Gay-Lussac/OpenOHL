# Finite-element wire systems

Implemented in `crates/ohl-wirefe` — a "semi finite element" approach: each wire span is **one exact elastic catenary element** (no meshing, exact for any sag), strings are truss elements, and the free nodes (clamps, load points, yoke plates) are found by Newton–Raphson in 3D.

## Elastic catenary element

Unstressed length \\( L_0 \\), axial stiffness \\( EA \\), uniform load \\( \mathbf q \\) per unit **unstressed** length (mass is conserved when the wire stretches; the ice and wind, per metre of the stretched wire, are brought to it below). The wire lies in the plane of the chord and \\( \mathbf q \\). In that plane (\\( z \\) opposite to the load), with \\( w = |\mathbf q| \\), \\( W = wL_0 \\), and \\( H, V_0 \\) the tension components at node *i*, \\( V_1 = V_0 + W \\):

\\[ x = \frac{HL_0}{EA} + \frac{H}{w}\left[\operatorname{asinh}\frac{V_1}{H} - \operatorname{asinh}\frac{V_0}{H}\right], \qquad
   z = \frac{V_0L_0 + wL_0^2/2}{EA} + \frac{1}{w}\left[\sqrt{H^2+V_1^2} - \sqrt{H^2+V_0^2}\right] \\]

For given end positions, \\( (H, V_0) \\) come from a damped Newton iteration (exit clauses: 100 iterations; the time limit, 10 s, only guards against a loop). The element returns the end forces and the exact 3×3 tangent stiffness (in-plane: inverse of the flexibility matrix; out-of-plane: \\( H/x \\)), verified against finite differences.

*Validation:* `wirefe-001…003` against RK4 integration of the elastic wire along its unstressed length (with a tension jump for a point load); `tbirv-001…003` against Irvine's equations and a quadrature of the wire's tangent ([Textbook and published cases](../validation/textbook.md)).

**The weight of a heated span (2026-10-21, owner decision).** A wire keeps its mass when it lengthens without being loaded more: when heated, after creep or a permanent set, past a bimetallic wire's transition. In a line's sections the temperature lengthens each span's unstressed length \\( L_0 \\) (as cut at stringing) to \\( L_0 e^{\alpha\Delta t} \\) (\\( \Delta t \\) from the stringing temperature); the element's own weight per unit of that longer length is therefore \\( w\,e^{-\alpha\Delta t} \\), so the span weighs \\( wL_0 \\) hot or cold. The same for every other lengthening without mass: a span whose reference length grew by a factor \\( k \\) (the linear models' creep stretch \\( 1 + \varepsilon_c \\), the stress-strain curves' correction of step 3 below, the core branch's offset) carries \\( w/(k\,e^{\alpha\Delta t}) \\) per unit of its element length; spread loads on the wire (N per metre of wire, a weight fixed to it) are scaled alike; jumpers too (\\( w \\) over their free thermal strain's factor). Ice and wind are loads per metre of the **real** conductor: they act on the longer or shorter wire as it is, not conserved like the weight (a cold span carries its ice on its shorter length, as it should), and on its elastic stretch too (below). Point loads, bodies and strings are forces, not per metre: unchanged. Before this, the load stayed per unit of the longer length, so a hot span weighed more than its wire by \\( e^{\alpha\Delta t} - 1 \\) (for a 300 m span strung at 25 kN at 10 °C, EA 30 MN, \\( \alpha \\) = 20·10⁻⁶/K: +0.04 % on H at 70 °C, +0.17 % at 150 °C, sag 7 and 13 mm larger; −0.04 % at −20 °C). The closed-form and ruling-span calculations conserve the weight the same way ([Sag-tension](sagtension.md#the-wires-own-weight-is-conserved)), and the three agree: a level 400 m span strung at 25 kN at 10 °C weighs 6288.73 N at 150 °C in each (its data sheet's 1.6 kg/m at 20 °C, per metre at 10 °C), FE 17734.05 N, the closed form 0.013 N less, the ruling-span method 8.28 N more (its additive thermal form) — validation `tbheat-001…003` ([Textbook and published cases](../validation/textbook.md)).

**The weight from the data sheet (2026-10-21, owner decision).** A wire library gives the mass per metre at the data sheet's reference temperature (the wire type's `reference_temperature`, else 20 °C). The element's own weight is per metre of the wire as cut at the **stringing** temperature \\( t_s \\): \\( w_s = w_{ref}\\, e^{-\alpha (t_s - t_{ref})} \\) (a metre of wire at a higher temperature holds less of it), and from there it is conserved as above. With stress-strain curves (whose strain form is additive) it is \\( w_{ref} (1 + \varepsilon(0, t_{ref})) / (1 + \varepsilon(0, t_s)) \\), the curves' zero-tension lengths ([Sag-tension](sagtension.md#the-weight-from-the-data-sheet)). Jumpers take theirs at their own temperature. For the textbook wire strung at 10 °C (α = 20·10⁻⁶/K) the weight is 0.020 % more than the data sheet's; validation `tbwref-001…002`.

**Ice and wind per metre of the stretched conductor (2026-10-21, owner decision).** Ice and wind act on the real conductor, which is longer than its unstressed length by its elastic strain (about 10⁻³: 1.4·10⁻³ at 42 kN on 30 MN). An element carries its own weight per unit unstressed length (above) and its ice and wind \\( \mathbf q_s \\) per metre of its stretched length: per unit unstressed length \\( \mathbf q_s\\, k \\), with the element's stretch

\\[ k = \frac{L}{L_0} = 1 + \frac{1}{EA\\,L_0}\int_0^{L_0} T\\,ds_0, \qquad \int_0^{L_0} T\\,ds_0 = \frac{G(V_1) - G(V_0)}{w}, \quad G(V) = \tfrac12\left(V\sqrt{H^2+V^2} + H^2\operatorname{asinh}\frac{V}{H}\right) \\]

(exact for the solved element, `CableProperties::stretched_length`). As \\( k \\) depends on the solution, the solve is a fixed point (`Model::solve`): solved with the elements' last stretches (1 at first), the stretches recomputed, solved again from the last positions, until no element's stretch changes by more than 10⁻¹¹ (its ice and wind then within 10⁻¹¹ of themselves, far below the solver's tolerance of 10⁻⁹ of the loads: no reported digit changes). It contracts by about \\( T/EA \\) a pass: three solves in practice, the later ones a few Newton iterations each. Exit clause: 20 solves, then an error naming the element whose stretch still changes and by how much. A section in the blocks shares out its length with the same loads (each span's length for a tension found with its own stretch, a fixed point started from the last tension's). Jumper pieces solved as catenaries (taut, or on a support string) do the same; the jumper rod is inextensible ([Jumpers](jumpers.md)). Before this, the ice and wind were per metre of unstressed length: about 0.1 % of them too little, on the unsafe side (−0.047 % on the tension of a 300 m span under 12 mm of ice, 1.5 mm less sag).

**Option: ice and wind per unstretched metre ("as other programs", 2026-10-21, owner decision).** The project's **Criteria › Calculation options** choose per what metre the finite elements carry the ice and wind (the line file's `solver.loads_per`, OHL XML `<solver loads_per>`, `ohl line --loads-per stretched|unstretched`):

- **per stretched metre** — the default, exact, as above;
- **per unstretched metre** — the former treatment, which other programs appear to use (a comparison of a real line with another program's results suggested it): each element carries its ice and wind \\( \mathbf q_s \\) per unit of its unstressed length, \\( k = 1 \\), no fixed point (one solve). It reproduces the results before 2026-10-21 (validation `tbice-002`: 42551.16 N, tbice-001's earlier value).

**Simplification** (the option per unstretched metre; for comparison with other programs, on the user's choice): the conductor carries about \\( T/EA \approx 0.1\\,\\% \\) less ice and wind than it does, so the tensions, sags and swings in iced and windy cases come out slightly smaller than with the exact treatment — on the unsafe side for clearances (on the example line −0.03 to −0.07 % on H in the iced and windy cases, sags within a few mm, still air identical; `tbice-002`: −0.047 % on H, 1.5 mm less sag). Exact: per stretched metre (the default). The results say which was used: `ohl line` prints it in its header (`calculation: ice and wind per … metre`), saved check reports keep it (`loads_per`), and a change marks every result out of date, naming the option. The own weight is per unstressed metre either way (it is conserved), and the closed forms (ruling span, the sag-tension job) always carry the ice and wind per metre of the stretched catenary ([Sag-tension](sagtension.md#the-wires-own-weight-is-conserved)): the option is the finite elements' only.

**Simplification:** the ice and wind are spread evenly over an element's unstressed length at its mean stretch. The conductor stretches more where its tension is higher (towards the supports), so the real ice per unstressed metre varies along the span by \\( \Delta T/EA \\) (about 10⁻⁵); the element's total ice and wind is exact, its distribution is not. On a level 300 m span under 12 mm of ice (`tbice-001`) the finite elements carry 3.3·10⁻⁷ more tension than an integration of the ice following the local stretch (0.014 N of 42571 N; the sag 2 µm less: on the unsafe side for clearances, the safe side for tensions), the closed form 0.9·10⁻⁷ less (it spreads the own weight evenly instead), so the two differ by 4.2·10⁻⁷ there (2.8·10⁻⁸ on a 100 m span). Exact: the load following the stretch along each element (more elements per span, or an element with a varying load). Wind per metre of conductor (its whole arc, as all of OpenOHL's calculations take it) rather than per metre of span, as codes often give it, is a separate question: on the real arc it is \\( 8f^2/3S^2 \\) more (0.15 % for a sag of 7 m on 300 m), on the safe side.

## Truss elements

Strings and links: axial force \\( N = \frac{EA}{L_0}(L - L_0) \\), tangent stiffness \\( \frac{EA}{L_0}\mathbf n\mathbf n^T + \frac{N}{L}(\mathbf I - \mathbf n\mathbf n^T) \\). Tension-only members (strings) go slack when clearly shorter than \\( L_0 \\). Weight is lumped half at each end.

## Newton–Raphson

Unknowns: positions of the free nodes. Each iteration assembles the stiffness of every element and solves \\( (\mathbf K + \mu\mathbf I)\Delta\mathbf x = \mathbf R \\):

- **Levenberg–Marquardt damping** \\( \mu \\): negligible next to stiff string directions (a string picks up its load in one step) but keeps steps moderate where the structure is still soft — a string at zero force has no pendulum stiffness yet. \\( \mu \\) falls after good steps, so the end is plain Newton (quadratic convergence).
- **Swing-arc projection**: a straight Newton step moves a swinging clamp along the tangent of its arc; with a nearly rigid string, the fake stretch \\( \delta^2/2L \\) creates huge forces. The step keeps Newton's intended string length but puts the clamp back on its arc. A bar between two free nodes (a string hanging from a string) is projected the same way about its end nearer the supports (counted in bars; equally far: its top), with the length change of the ends' relative move. Nodes are placed from the supports outwards, each once the nodes it hangs from are: on its one bar's sphere, or — held by two bars (a V's tip, also the joint closing two chains of strings) — where the two spheres meet nearest to Newton's point; with more bars, a few alternating passes. A node all of whose bars went slack in a step drops back until its bar nearest the supports is taut (slack bars give it no stiffness: Newton cannot bring it back).
- **Line search** on the largest unbalanced nodal force; a step lowering the unbalanced force of all nodes together (root of the sum of squares) by 10 % is also taken — a chain of strings swinging as a whole lowers it while one joint's force may still grow.
- **Step limit for chained strings**: where strings hang from strings, a joint moves at most a third of its shortest string per step (a longer step can fold a chain the wrong way). Models without them step as before.
- **Tolerance**: \\( 10^{-9} \\) of the loads, but not below the floating-point floor \\( 64\\,\varepsilon\\, k_{max} |x| \\) of stiff members, nor below \\( 10^{-11} \\) of the largest wire \\( EA \\) (the noise of the elements' own iterations).
- **Exit clauses**: 100 iterations (the time limit, 120 s, only guards against a solve that never ends); the error names the node with the largest unbalanced force. `OHL_FE_TRACE=1` prints the iteration history.
- **Fallbacks** when Newton stops: (1) the wires under their vertical load alone, then the point loads and the sideways (wind) part of the wires' load ramped up together, each step from the last solution (at most 40 steps, no step smaller than 1/256 of the loads); (2) for models with strings hanging from strings only, **dynamic relaxation** — each free node a mass moving under its unbalanced force (masses from the stiffness's row sums, never decreasing, so every step is stable), the motion stopped at each peak of kinetic energy — to near equilibrium (10⁻³ of the loads), then Newton from there. Exit clauses of the relaxation: 50 000 steps or 5 s. A long chain in high wind (four strings on one side, five on the other, ending in a V) takes about 2 s this way.

## Sections: sagging and clamping

A section (dead-end to dead-end, suspension strings, alone or two to a wire point) is built as in the field: every span is sagged at the stringing tension with the strings **plumb** (at their stretched length under the clamp load), which fixes each span's unstressed length. Any other state — temperature (\\( L_0 e^{\alpha\Delta t} \\), the span's own weight kept, see [above](#elastic-catenary-element); the ruling-span method uses the additive form, see [Thermal strain: two forms in use](sagtension.md#thermal-strain-two-forms-in-use)), ice, wind on wires and strings, point loads at predefined sites, broken spans — is then solved with string swing, unequal span tensions and 3D blowout.

In a line model (`ohl-line`) this is a section **clipped in**. A section still **in the blocks** (the default while designing) rolls instead: in each weather case its total unstressed length, from the sagging condition (a tension, or a sag in the ruling span or in one span), is shared out between the spans so that all have the same horizontal tension — the length of each span for a tension \\( H \\) under the case's load is found exactly, and \\( H \\) is the root of \\( \sum_i L_{0,i}(H) = L_0 \\) (bracketed secant). The clamps settle where the strings put them (swung across the line at angles, plumb along it, where equal tensions cancel); the sharing is repeated until they stop moving (≤ 1 µm, 20 passes at most). Point loads are left out of the sharing.

**Simplification:** in the blocks, point loads do not take part in sharing the length between the spans. A loaded span would pull length from its neighbours through the blocks; leaving the loads out keeps its length as in still air, so its sag under the load can be a little larger or smaller than with the loads shared. Exact: point loads in the sharing.

*Validation* (clipped-in sections against an independent solver, [Textbook and published cases](../validation/textbook.md)): `tbrs-002…003` (three spans with suspension strings against the ruling span: −0.73 % to +0.26 % at 80 °C), `tbins-001…005` (a span's blowout, a suspension string's swing in wind, a V's leg forces and its slack leg), `tbcreep-001` (creep as a temperature shift), `tb324-011` (dead-end strings, CIGRE TB 324).

## A weather case per span (2026-10-20)

*Built, in active development (not approved yet).* A section can be
solved with a **weather case per span** (`ohl_line::solve::SpanCases`,
`solve_section_span_cases`; in the plain section model
`ohl_wirefe::State::span_states`): the section has its case, and some
spans carry a case of their own — one span iced while its neighbours are
not (danger trees' unbalanced ice), unequal ice along a section, later
galloping's unequal ice. A span's own case sets everything that acts on
that span: its load per metre (weight, ice, and the wind on its **iced**
diameter from its side), its temperature (the thermal strain of its
unstressed length; with stress-strain curves, the curves' strain at its
tension and temperature), the forces on the bodies on it (their ice and
wind) and its surface radius. The section's case sets the rest: the wind
on the strings, the jumpers on the clamps, the core branch of a
bimetallic wire (one per section) — and the rolling of a section **in
the blocks**: it rolls in the section's case, and the spans' own cases
then act on the wire clipped in, as point loads do (ice arriving on one
span of a strung line swings the strings into it; it cannot draw wire
from its neighbours through the blocks).

With every span in the section's case the section is solved exactly as
with that one case, bit for bit (tested on every section of the example
line, still, hot, iced and windy). The ruling-span method has one tension
for all spans: spans in different cases are refused there (an error that
says so), only the finite elements solve them.

**Simplification:** the strings take the wind of the section's case;
when a span's own case has another wind, the strings at its ends do not
see it. Exact: each string the wind of the spans next to it. (Unbalanced
ice keeps the wind of the section's case on its span, so it is exact
there.)

Checked against an independent evaluation (validation
`spancases-001`…`003`, `benchmarks/wirefe/span_cases.toml`): two level
300 m spans, a 3 m suspension string between them, span 0 iced by
10 N/m more — the string swings 2.89° into span 0, whose horizontal
tension is 338 N above the other's (\\( H_0 - H_1 = T \sin\varphi \\) at
the clamp) — and span 0 at 60 °C with span 1 at 15 °C (the string swings
2.34° towards span 1; each span keeps its weight): the closed-form elastic catenary of each span and
Newton on the clamp's position, its own implementation; the solver agrees
to 10⁻⁷.

What this shows that the equal-tension (ruling span) model cannot: in the example job, support 3 is a **valley structure** (a 25 m descent into it, a 40 m rise out of it). Its weight span is only ~55 m, so in high wind its string swings ~89° — a real design issue (V-string, hold-down weights or a taller structure).

## Point loads: marker balls and other bodies

A point load in a span is a weight at a fraction of the span, and may also be a body of some size: a marker ball, an obstruction light. It's given a shape — a sphere of diameter \\( D \\), a cylinder, a box, or given areas (a wind area \\( A \\) and the surface \\( S \\) that takes the ice) — and a drag coefficient \\( C_d \\) (default 1). In a weather case it carries:

- **Ice:** the wire's radial ice thickness \\( t \\) (the case's thickness, or with an ice load the thickness that load makes on the wire's bare diameter, at the case's ice density or else glaze ice, 913 kg/m³), as a shell of \\( \frac{\pi}{6}\left((D+2t)^3 - D^3\right) \\) on a sphere, or a layer \\( t \\) on the surface of a cylinder or a box, or \\( S\,t \\) on given areas, at the case's ice density. A case with an ice load but no density gives no thickness, so the body takes no ice.
- **Wind:** \\( F = p\,C_d\,\frac{\pi}{4}(D+2t)^2 \\) on a sphere, across the span, from the case's side (a cylinder or a box: its projected sizes grown by \\( 2t \\); a given area \\( A \\) is grown by the ice as a sphere of that area).

On a bundle the forces are shared by its subconductors. These loads can matter: a 0.6 m ball takes about 180 N of wind at 650 Pa and, with 16 mm of ice, about 170 N of ice, several times its own weight. A dozen of them on a long earth-wire span add a quarter or more to the wind on the wire. Without their wind area a long marked span swings metres less than it should.

**Simplification:** a body's drag coefficient is 1 unless given, and its ice is the wire's radial thickness. Both are stand-ins: a code or the maker may give other values, either way. Exact: the body's own drag and ice from the data.

## Line sections: strain assemblies, bundles, jumpers

In a line model the strings holding the wire at each point are built as the structure gives them ([line file](../user-guide/line-file.md#points-strings-and-wire-points)): its points are fixed nodes, its wire points free nodes, its strings bars. One string from the structure in the middle of the section, hanging below its top, is a **suspension string**; the section's ends, and middle points held by one string not below its top, are **strain assemblies**: a free clamp on a member from the fixed attachment point, lined up with the conductor by equilibrium, so it moves with every weather case. A strain string given without a weight (geometry only, e.g. imported) gets the weight that holds it where the data puts it in the stringing state. Without a weight means a resolved weight of 0 N — from its own data or from an insulator assembly that gives 0 N — whether or not it names an assembly (2026-10-21; before, a string naming one never got the estimate, and linked to "0 N" library entries a real line's strain strings lost it: median sag differences to another program's results grew from 0.7–2.4 cm to 2–9 cm). An assembly with a real weight gives that weight. A point whose wire point and attachment point coincide is fixed. A **bundle** is solved as one subconductor carrying its share of the strings' weight and wind area; the string forces and reactions reported are the whole bundle's.

**Simplification:** the weight given to a strain string without one (0 N resolved, its own or its assembly's) is fitted to the drawn geometry, not taken from the hardware; it is as good as the drawing (either way). Exact: the string's real weight in the insulator library.

**Simplification:** the subconductors of a bundle are taken as hanging alike (one subconductor solved, times their number). Unequal ice or wind on the subconductors, and the bundle twisting, are not modelled; either way. Exact: each subconductor and the spacers in the model. **Jumpers** are solved separately, between the solved clamps, as inextensible elastic rods with bending stiffness (see [Jumpers](jumpers.md)).

## Strings hanging from strings

Any assembly is strings between fixed points (on the structure) and free wire points: a string below two strings has a free wire point between them and a free clamp; two tension strings from a string's wire point, a free node with two tension strings to two sections' clamps. Each string is a bar of its given length (top to wire point as given), tension-only unless it **may take compression**: it then acts both ways — compressed, its stiffness is the axial one without a geometric part, so it holds as a strut — and its force beyond the given compression is **flagged** with the solution (the solve does not change it: a buckled member is the designer's to change). Its weight and wind area go half to each end (wind across the line at the section point).

**Settling while sagging.** In the stringing reference and while a section in the blocks rolls, the clamps are moved back across the line after each solve: along the line equal tensions cancel at any position (a neutral direction the iteration would creep along), and the crew clamps in plumb along it. A suspension string swings back into the plane across the line keeping its radius; a V turns about the axis through its tops. Any other assembly is solved again on its own, from its solved shape, under its weights and the wires' pull on the clamp (what balances the strings and loads there) less its part along the line — exact for a V with a string below and every other shape.

**Sections sharing an assembly** (the end of one section and the start of the next hang on the same strings) are solved together, Dirichlet–Neumann:

1. each section alone first, the other sections' clamps in its assemblies held where given;
2. the **stiffest** section through a shared assembly solves it with the other sections' wires pulling on their clamps (their last pull, as a load on those free nodes); the others hold their clamp where that solution puts it, which gives their pull for the next pass. Stiffest along the line at its end there: the least compliance \\( \sum_i L_i/EA + w^2 L_i^3 / 12 H_i^3 \\) over its spans as solved alone (the ruling span's: span lengths \\( L_i \\), horizontal tensions \\( H_i \\), the case's load per metre \\( w \\)); of equals, the first in the line's order;
3. the pulls on the held clamps are relaxed with Aitken's factor from the last two passes, half way at first, between 0.02 and 1;
4. until the pulls on the held clamps change less than 0.1 N (or 10⁻⁶ of themselves); not in 30 passes: an error for the sections of the group, naming the pull still changing and by how much.

Why the stiffest: a held section's pull changes by its stiffness times the move of its clamp, and that move is the pull's change over what holds the clamp back (the solving section and the strings). The pass multiplies a pull's error by minus the ratio of the two. Held, a short single span against a long section on a swinging string (on the second existing line, freshly imported) pulls some 20 times harder for a move of its clamp than the long section and the string hold it back: the pulls overshot in turn by hundreds of kN, the string swinging metres either way, and never settled, not with half steps nor with Aitken's (28 passes in still air, failing in other cases) nor with a secant (Anderson) step. Solving the TAK, the stiff span takes the long section's pull, and the pass multiplies the error by about −1/20: 5–15 passes in every case, both wind sides. (Taken whole, two similar sections' ends pull back and forth: a TAK damped by only 0.89 a pass, half way by about 0.06.)

The stringing reference of each section holds the other sections' clamps where given (the line as strung). The loads on the attachment points of a shared assembly are reported once, by the section that solves it. Expected cost: a few solves of each section of the group (milliseconds); sections that share nothing are solved alone, as before.

## Non-linear stress-strain in the elements

A wire with stress-strain curves (the non-linear model of the [wire library](../user-guide/wire-library.md#wire-models)) keeps elastic catenary elements with its \\( EA \\), but the unstressed length of each span is corrected until the span follows the curves (`ohl-line`, `solve_fe`):

1. The section is first solved in the **stringing state** (still air at the stringing temperature). Each span's mean tension there, through the initial curve, fixes the span's length at zero strain.
2. In the weather case, the section is solved; each span's mean tension \\( \bar T_s \\) (weighted by the elements' unstressed lengths) gives the curves' strain \\( \varepsilon(\bar T_s, t, \text{state}) \\) in the case's **material state** — initial, after creep or after load — so the permanent set comes with the state.
3. Each span's unstressed length is set so that its length at \\( \bar T_s \\) is the curves' length: the zero-strain length times \\( (1+\varepsilon) \\), divided by the element's own stretch \\( e^{\alpha\Delta t}(1 + \bar T_s/EA) \\). Its own weight per unit of that length falls by the same factor (the span keeps its weight: the set and the curves' thermal strain add no mass).
4. The section is solved again with those lengths, until they change by less than \\( 10^{-9} \\) (relative). This secant iteration settles in 3–6 solves; it stops with an error after 30.

The curves' compression limit of the outer strands is part of the strain, so it replaces the bimetallic core branch used for linear wires (past the transition temperature, a linear bimetallic wire is solved as its core alone, with the outer strands' constant compressive load). Linear models are unchanged: they are elastic with \\( EA \\), longer by the final state's permanent stretch.

On a dead-end span with the wires fixed at the structures, FE and RS then agree within about 0.02 % in tension (the test allows 0.1 %) and 1 cm in sag, from −20 °C to 60 °C (the two methods measure sag on slightly different chords: 5 mm apart already at stringing). Since the curves define strain in the additive thermal form, the FE of a wire with curves follows that form; the FE of a wire without curves keeps the multiplicative one, see [Thermal strain: two forms in use](sagtension.md#thermal-strain-two-forms-in-use).

## Not yet included

- **Structure flexibility** (**Simplification:** structures taken as rigid): the structures' points are fixed in the wire solve. A real structure gives under the wires' pull, which lowers the tensions (on the safe side for loads computed here) and raises the sags (on the unsafe side for clearances); the structures are analysed separately ([Structure analysis](structures-fe.md)). Decided as the structure–line interaction levels: level 3 adds a stiffness at each structure's suspension points (from level 3 structures), level 4 iterates structures and line together — both planned, see the roadmap (open point O7).
- **Dynamic effects** (galloping is treated by ellipses, see [Galloping](galloping.md)).

Linked insulator assemblies are built: strings hang from attachment points or from other strings (a string below two, two tension strings from a string coupling two sections), and jumper support clamps hang on a structure's strings.
