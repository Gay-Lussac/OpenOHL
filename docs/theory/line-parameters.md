# Line parameters

*Built, in active development, not approved yet* (roadmap O18, steps 0
and 1, 2026-10-27). Implemented in `crates/ohl-electrical`: the line's
cross-sections, then per span the phase matrices per unit length, and
along the line per circuit. Used by `ohl electrical parameters` and Checks
› Electrical › Line parameters… ([Electrical: line
parameters](../user-guide/analyses.md#electrical-line-parameters)).

Every assumption is marked **Simplification** (it changes the result,
either way) or **Assumption** (a value taken where the model has none);
none of them is conservative or unconservative in a design sense — line
parameters feed network calculations, which take them as they are.

## Cross-sections

A **span of the line** is two structures next to each other along the
line with every wire spanning between them. Its axis runs in plan from the
first structure to the second; each wire's place across the line is its
**offset** from the axis (positive to the right looking ahead), its
height is measured from the ground under it. Both come from the sections
solved in a weather case:

- **At a station** (a fraction of the span): where the solved wire
  crosses the plane square to the axis there (linear between the solved
  points).
- **As the span's mean** (what the line parameters use), by the line's
  setting *Height in a span*:
  - `solved` (the default): the exact average of the solved wire's offset
    and height along the axis, by trapezoids over its solved points;
  - `two_thirds_sag`: the textbook approximation — the mean of the
    attachment heights minus two thirds of the sag (the largest vertical
    distance below the chord), the offset the attachments' mean. For a
    parabola \\( h(t) = H - 4ft(L-t)/L^2 \\) over level ground the mean is
    exactly \\( H - \tfrac23 f \\) (its integral); the catenary and a
    sloping ground make the two differ by a few centimetres (checked in
    the crate's tests). **Simplification** of the `solved` mean.

The ground: a terrain when the caller gives one, else **straight between
the two structures' feet** (their positions) and **level across the
line** — **Simplification** where no terrain is given (the command
line takes `--terrain` or a project surface; the window none yet). Over a
valley the straight line between the feet can lie above the wires: those
wires are then left out of the span, said, and the circuit's totals are
short of the span (said).

The record (`CrossSection`, see the [data model](../development/data-model.md#cross-sections-and-line-parameters-ohl-electrical-derived))
holds per wire its section and span, offset, height, elevation, bundle
(subconductors, spacing, rotation), wire data (diameter, geometric mean
radius, AC resistance at the case's temperature) and system and phase
(circuit, voltage, frequency, phase angle; none for an earth wire), and
the ground (elevation, slope across, soil resistivity). Other lines' wires
in the corridor have a place in it and are not filled yet.

## Series impedance

Per unit length between wires \\( i \\) and \\( j \\) above a homogeneous
earth (J. R. Carson, "Wave propagation in overhead wires with ground
return", *Bell System Technical Journal* 5 (4), 1926, pp. 539–554):

\\[ Z_{ii} = R_i + j\frac{\omega\mu_0}{2\pi}\ln\frac{2h_i}{GMR_i} + \Delta Z_{ii},
\qquad Z_{ij} = j\frac{\omega\mu_0}{2\pi}\ln\frac{D'_{ij}}{d_{ij}} + \Delta Z_{ij} \\]

with \\( d_{ij} \\) the distance between the wires, \\( D'_{ij} \\) the
distance from wire \\( i \\) to the image of wire \\( j \\) below the earth
surface, \\( R_i \\) the AC resistance at the weather case's temperature
(the wire library's values, linear between them, as in [thermal
rating](thermal.md)) and \\( \Delta Z \\) the correction for the earth's
finite conductivity, by one of three methods (the line's setting *Earth
return*):

- **Carson's series** (`carson_series`, the default):
  \\( \Delta Z = \frac{\omega\mu_0}{\pi}(P + jQ) \\), \\( P, Q \\) the
  series in \\( a = D'\sqrt{\omega\mu_0/\rho} \\)
  (\\( = 4\pi\sqrt5\cdot10^{-4}D'\sqrt{f/\rho} \\)) and the angle
  \\( \theta \\) from the vertical to the image; the first terms
  \\( P = \pi/8 - a\cos\theta/(3\sqrt2) + \dots \\),
  \\( Q = (\tfrac12 - \gamma)/2 + \tfrac12\ln(2/a) + a\cos\theta/(3\sqrt2)
  - \dots \\), with the coefficients' recurrence written out in the code
  (`ohl_electrical::earth::carson_pq`). Summed until the terms fall below
  \\( 10^{-17} \\) up to \\( a = 15 \\), Carson's asymptotic expansion
  beyond. **Validity:** a homogeneous earth, the wires long and parallel
  to it, the earth's displacement current neglected (Carson's integral
  itself: power and harmonic frequencies; it assumes a TEM wave and fails
  above about 10 MHz — T. Theodoulidis, "On the closed-form expression of
  Carson's integral", *Periodica Polytechnica EECS* 59 (1), 2015). In double
  precision the series stays within \\( 10^{-10} \\) of the integral up to
  \\( a = 15 \\) and the asymptotic expansion within \\( 10^{-6} \\) above
  it — checked against a numerical integration of Carson's integral (below).
  At 50 Hz and 100 Ω·m, \\( a \approx 0.002 \\) per metre of
  \\( D' \\): line wires are far inside the series' range.
- **Complex depth** (`complex_depth`; C. Dubanton 1969, A. Deri, G.
  Tevan, A. Semlyen, A. Castanheira, "The complex ground return plane",
  *IEEE Trans. PAS-100* (8), 1981, pp. 3686–3693): the earth replaced by a
  perfect conductor at depth \\( p = \sqrt{\rho/(j\omega\mu_0)} \\),
  \\( Z_{ij} = j\frac{\omega\mu_0}{2\pi}\ln\frac{\sqrt{(h_i+h_j+2p)^2 +
  x_{ij}^2}}{d_{ij}} \\). **Simplification** of Carson: within 1–3 % of his
  correction term in the cases checked (a few per mille of the impedance).
- **Carson's first terms** (`carson_first_terms`, the "modified Carson"
  equations of distribution textbooks): \\( P = \pi/8 \\),
  \\( Q = (\tfrac12-\gamma)/2 + \tfrac12\ln(2/a) \\); the heights then drop
  out (the familiar \\( 0.0953 \\) Ω/mile and \\( D_e = 658.5\sqrt{\rho/f} \\)
  m at 60 Hz). **Simplification:** for comparison with programs that use
  it; Wagner and Evans (as cited by Vismor, *Carson's Equations*) note it
  overstates the resistance and understates the reactance at low
  resistivities.

**Assumption — soil resistivity:** homogeneous under each span. A span
takes the **geometric mean** of the values measured at its two structures
(one measured: that one), else the line's setting; the geometric mean
because the impedance goes with \( \ln\sqrt\rho \), so it averages what
the calculation feels. When neither a structure nor the line gives one, **100 Ω·m** is taken, the value the
IEEE distribution test feeders assume (IEEE PES Distribution System
Analysis Subcommittee, "Radial Distribution Test Feeders", 2000,
section III), and every result says so. Soils range from a few Ω·m to
thousands; the zero sequence impedance depends on it (roughly with
\\( \ln\sqrt\rho \\)), the positive sequence hardly.

**Assumption — frequency:** the system's; 50 Hz for a system that gives
none (said in the notes). DC systems have no AC parameters and are left
out (said).

### Bundles

A bundle of \\( n \\) subconductors on a circle (spacing \\( s \\), radius
\\( R = s/(2\sin(\pi/n)) \\)) is replaced by **one equivalent wire at its
centre**: geometric mean radius \\( (n\,GMR\,R^{n-1})^{1/n} \\) (the
geometric mean of a subconductor's distances to itself and to the others,
whose product on a regular polygon is \\( nR^{n-1} \\)), resistance
\\( R_{ac}/n \\); for the capacitance the same with the radius.
**Simplification:** the subconductors carry equal currents and charges —
exact for the far field, a small error when the bundle is large against
the distances between phases.

A wire type without a geometric mean radius takes \\( e^{-1/4} \\) times
its radius (a solid round conductor) — **assumption**, said; a stranded
conductor's differs (give it in the wire library). Without any resistance the
section is left out (said); without an AC resistance its DC value is
taken (said).

### Earth wires: Kron's reduction

The earth wires (sections of no system: earth wires, OPGW) are reduced
out of the primitive matrix (G. Kron's reduction):
\\( Z_{abc} = Z_{pp} - Z_{pe}Z_{ee}^{-1}Z_{ep} \\).
**Assumption:** the earth wires are **continuously earthed** — at zero
voltage all along the span (earthed at every structure, the span short
against the wavelength); sectionalised or insulated earth wires are not
modelled. The phases of other systems and circuits stay in the matrix
(they carry their own currents): their coupling is kept.

## Shunt capacitance

Potential coefficients with images in a **perfectly conducting earth**
and air of permittivity \\( \varepsilon_0 \\) (**Simplification**: the
earth's finite conductivity changes the capacitance negligibly at power
frequency):

\\[ P_{ii} = \frac{1}{2\pi\varepsilon_0}\ln\frac{2h_i}{r_i}, \qquad
P_{ij} = \frac{1}{2\pi\varepsilon_0}\ln\frac{D'_{ij}}{d_{ij}} \\]

the earth wires reduced out (zero voltage) the same way, then
\\( C = P^{-1} \\) (Maxwell's coefficients) and \\( B = \omega C \\).

## Along the line

Each span's matrices are reduced to its phases, keyed by **circuit,
system and phase** (not by the wires' places), and summed times the span's
**horizontal length**: series impedances add (the spans in series) and so
do the capacitances (the spans in parallel at one voltage). A transposition
along the line is so in the totals as built. **Simplification:** the
wires' length is taken as the span's horizontal length (the sag adds about
\\( 8f^2/(3L) \\), 0.1–0.3 %); each span's wires parallel at their mean
height.

A **circuit** is the phases of one system with one circuit name (or none);
its **stretches** are consecutive spans with the same sections (between
strain structures, or where a section changes). A span missing one of its
phases is left out of that circuit (said).

## Sequence values

For a three-phase circuit, with its phases as a, b, c (its first phase
as a, then the phase lagging it by about 120° as b), the symmetrical
components (C. L. Fortescue, 1918) of its total matrix,
\\( Z_{012} = A^{-1}Z_{abc}A \\). The values reported are the **ideally
transposed line's**:
\\( Z_1 = Z_s - Z_m \\), \\( Z_0 = Z_s + 2Z_m \\) (\\( Z_s \\) the mean of
the diagonal, \\( Z_m \\) of the rest) — which are also the diagonal of
\\( Z_{012} \\) — and likewise \\( C_1, C_0 \\), \\( B_1, B_0 \\) from the
capacitance matrix. As built, a line not transposed couples its
sequences: the largest off-diagonal term of \\( Z_{012} \\) relative to
\\( Z_1 \\) is reported (*unbalance*); the phase matrices (`--matrices`)
give the line as it is.

**Transposition of the capacitance:** the program averages the
capacitances over the line (the stretches in parallel at the same phase
voltages). Textbooks average the potential coefficients (equal charges
along the line) and invert; on Overbye's flat triple-bundle line the two
differ by 1.4 % (C₁ 1.187 against 1.17 nF/km), on a vertical circuit
0.7 %. The validation case checks the textbook value computed from the
program's matrices.

**Coupling between circuits** sharing spans (the same frequency): the
mutual block of their matrices summed over the shared length, and its zero
sequence mutual impedance \\( Z_{0M} = \tfrac13\sum_{ij} Z_{ij} \\) (the
zero sequence voltage on one per zero sequence current in the other).
Parallel lines elsewhere in the corridor come with other lines' wires in
the cross-section (later).

## Validation

`benchmarks/electrical/` (see [Textbook and published
cases](../validation/textbook.md#line-parameters-benchmarkselectricalpublishedtoml-earth-returntoml)):

- **W. H. Kersting**, *Distribution System Modeling and Analysis*,
  Example 4.1, as printed with its inputs and results in **T. Overbye**,
  "ECEN 667 Power System Stability, Lecture 4", Texas A&M University, 2019,
  slides 13–21,
  <https://overbye.engr.tamu.edu/wp-content/uploads/sites/146/2019/09/ECEN667_2019_Lect4.pdf>:
  the 3×3 phase impedance matrix with the neutral reduced out (Carson's
  first terms, 100 Ω·m, 60 Hz) — all twelve values within 0.0002 Ω/mile
  of the printed four decimals; the reduced potential coefficients
  (within 0.15 %) and four of the six capacitances (within
  0.00008 µF/mile; two printed values are not the inverse of the slides'
  own coefficients and are left out). The phase c position comes from the
  IEEE test feeders' spacing ID-500 (IEEE PES DSAS, "Radial Distribution
  Test Feeders", 2000, Figure 1,
  <https://ewh.ieee.org/soc/pes/dsacom/testfeeders/testfeeders.pdf>).
- **T. Overbye**, the same lecture, slides 7–8: a transposed line of
  triple bundles — R₁ 0.02 Ω/km, L₁ 9.63·10⁻⁷ H/m, C₁ 1.17·10⁻¹¹ F/m
  within their three printed figures.
- **Carson's integral** evaluated by adaptive numerical quadrature
  (`reference_electrical.py`, a different method from the program's):
  the earth term at \\( a \\) = 0.02 to 28 (16.7 Hz to 50 kHz, 0.5 to
  1000 Ω·m) within \\( 10^{-6} \\) (\\( 10^{-5} \\) for the asymptotic
  expansion).
- The complex depth against its closed form evaluated independently.
- Hand calculations in the crate's tests: Kron's reduction, the sequence
  transform of a flat line written out, the bundle's equivalent radius,
  the positive sequence reactance \\( \omega\frac{\mu_0}{2\pi}\ln(GMD/GMR) \\)
  and capacitance of a circuit on the synthetic example line.

**Not validated against a published example** (none with inputs and
results found in an open source): Carson's full series on a real line
geometry (only the first terms are in the distribution examples), the
sequence impedances (checked as the transform of the published matrix),
the coupling between circuits, and the cross-sections' mean heights (hand
calculation of a parabola).
