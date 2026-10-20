# Textbook and published cases

*Built, in active development (not approved yet).* This page starts the
validation report (roadmap open point O11): the textbook cases of the
line calculations and the worked examples of CIGRE TB 324, each with its
source and tolerance. They run with every other case
(`ohl validate benchmarks`); the generated [validation report](report.md)
lists the results.

## How the references are made

Every textbook case has two independent checks:

- a **hand derivation** written out in the case's comments (`notes`):
  closed forms evaluated, or for the iterative calculations a classical
  approximation (the parabolic change-of-state cubic, the pendulum formula
  of a suspension string, the statics of a V) with its known error stated;
- an **independent numerical reference**,
  `benchmarks/reference/textbook_reference.py` (Python standard library
  only; run `python3 -I benchmarks/reference/textbook_reference.py`, about
  25 s). It uses none of the code's formulas: a span is the *integral* of
  its tangent \\( \mathbf F/|\mathbf F|\\,(1 + |\mathbf F|/EA) \\) along
  its length, with the tension vector \\( \mathbf F(s) = \mathbf F_0 -
  \mathbf q s \\) from equilibrium, evaluated by Gauss–Legendre quadrature
  (64 nodes per piece; the script checks against 96 nodes: \\( 3\cdot
  10^{-13} \\)); the start force (and the length of an inextensible span)
  by Newton with the quadrature's own Jacobian; point loads as jumps of
  \\( \mathbf F \\); sections and line models by Newton on the free nodes'
  force balance (largest unbalanced force below \\( 10^{-6} \\) N). The
  expected values are its output to 10 significant digits.

The TB 324 cases take the brochure's printed numbers as expected values;
the same script reproduces each of them from the printed inputs.

The line-model cases use `line.section` (one section of a line file
solved by the line's solver, optionally clipped in first as the app's
*Clip in* does) and the composite-properties cases `cables.properties`;
both are adapters of the validation runner (`crates/ohl-validation/src/textbook.rs`),
the engines are unchanged.

## Tolerances

| Kind | Tolerance | Why |
|---|---|---|
| Closed forms (catenary, loads, Irvine) | \\( 10^{-8} \\) relative (\\( 10^{-9} \\) for loads) | exact; reference to \\( 10^{-12} \\) |
| Change of state, sections, line models | \\( 10^{-7} \\) relative on forces and sags, \\( 10^{-6} \\) m on positions, \\( 10^{-6} \\)° on angles | iterative solvers converging far below it (FE: \\( 10^{-9} \\) of the loads) |
| TB 324 | half a unit of the printed last digit | the brochure's rounding; two cases add a stated model difference (below) |

No tolerance was widened to make a case pass.

## Cases

### Catenary (`benchmarks/sagtension/textbook-catenary.toml`)

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbcat-001 | level 400 m, C = 2000 m: sag, length, tensions; the parabola is 8.34 mm (0.083 %) short on sag, 0.33 mm on length | closed form + reference | \\( 10^{-8} \\) |
| tbcat-002 | level 1000 m, C = 1000 m: the parabola 2.626 m (2.06 %) short | closed form + reference | \\( 10^{-8} \\) |
| tbcat-003 | inclined 400 m, +20 m: low point, \\( T_B - T_A = wh \\), largest sag | closed form + reference | \\( 10^{-8} \\) |
| tbcat-004 | steep 300 m, +60 m: low point 47.96 m beyond the lower support, uplift 479.7 N | closed form + reference | \\( 10^{-8} \\) |
| tbcat-005 | tension for a given sag (inverse of 001) | closed form | \\( 10^{-7} \\) |

### Change of state (`benchmarks/sagtension/textbook-change-of-state.toml`)

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbcos-001 | 10 → 70 °C | parabolic cubic (−0.023 %) + reference | \\( 10^{-7} \\) |
| tbcos-002 | 12 mm glaze ice at −5 °C | parabolic cubic (−0.016 %) + reference | \\( 10^{-7} \\) |
| tbcos-003 | 600 Pa wind, resultant in the blown plane | parabolic cubic (−0.014 %) + reference | \\( 10^{-7} \\) |
| tbcos-004 | thermal sag at 150 °C | parabolic cubic (−0.044 %), slack balance + reference | \\( 10^{-7} \\) |
| tbcos-005 | inclined 300 m, +50 m, 80 °C | \\( T_B - T_A = wh \\) + reference | \\( 10^{-7} \\) |
| tbcreep-001 | creep as +25 °C (linear model), FE | equals the initial state at 35 °C to 0.003 % + reference | \\( 10^{-7} \\) |
| tbcreep-002 | the same, ruling-span method | additive strain + reference | \\( 10^{-7} \\) |

### Ruling span (`benchmarks/sagtension/textbook-ruling-span.toml`)

Three level spans 200/350/250 m, 3 m suspension strings, strung at 25 kN
at 10 °C. The approximation, stated: at 80 °C the exact equal-tension
section carries 18312.83 N; the classical ruling span (288.314 m) gives
18312.01 N (−0.0045 %), its parabolic cubic 18316.8 N (+0.022 %); with
the real strings (FE, clipped in) the spans carry −0.74 %, +0.25 % and
−0.38 % of it — the short spans sag more than the ruling span says. At
−20 °C: +0.17 %, −0.05 %, +0.09 %.

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbrs-001 | exact equal-H section and the ruling span length | closed form \\( \sqrt{\sum S^3/\sum S} \\) + reference | \\( 10^{-7} \\), \\( 10^{-9} \\) |
| tbrs-002 | FE, 80 °C: span tensions, clamp moves, string swing | \\( \Delta H = N \sin\varphi \\) at each clamp + reference | \\( 10^{-7} \\), \\( 10^{-6} \\) m/° |
| tbrs-003 | FE, −20 °C | the same | the same |
| tbrs-004 | the line's ruling-span method (one H, additive strain) | reference | \\( 10^{-7} \\) |

### Point load (`benchmarks/sagtension/textbook-point-load.toml`)

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbpl-001 | level 300 m, 2 kN at mid-span: slope jump P/H | closed form \\( C[\cosh(a+b) - \cosh b] \\) + reference | \\( 10^{-8} \\) |
| tbpl-002 | inclined, 1.5 kN at 100 m | vertical balance, \\( T_B - T_A = wh \\) + jump + reference | \\( 10^{-8} \\) |

### Bimetallic conductor (`benchmarks/sagtension/textbook-bimetallic.toml`)

Linear aluminium on a linear steel core (TB 324's typical component
values), the outer strands taking no compression; knee point at 140.85 °C
by hand and by the reference.

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbbim-001 | 60 °C, both components in tension | composite modulus and expansion + reference | \\( 10^{-7} \\) |
| tbbim-002 | 150 °C, the core alone | core-only strain + reference | \\( 10^{-7} \\) |

### Ice and wind (`benchmarks/weather/textbook-ice-wind.toml`)

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbwx-001 | 12 mm glaze ice per metre | \\( \rho g \pi t (d + t) \\) | \\( 10^{-9} \\) |
| tbwx-002 | 25 mm rime (500 kg/m³) + 300 Pa on the iced diameter, blowout | closed form | \\( 10^{-9} \\) |
| tbwx-003 | bare wire in 600 Pa: blowout angle | closed form | \\( 10^{-9} \\) |

### Elastic catenary (`benchmarks/wirefe/textbook-elastic-catenary.toml`)

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbirv-001 | level 400 m, \\( L_0 \\) = 400.5 m | Irvine's equations (residual < \\( 10^{-12} \\) m) + reference | \\( 10^{-8} \\) |
| tbirv-002 | inclined, vertex outside the span | Irvine + \\( T_j - T_i \approx wh(1 - \bar T/EA) \\) + reference | \\( 10^{-8} \\) |
| tbirv-003 | taut 50 m with \\( L_0 \\) < chord | Irvine + parabolic estimate + reference | \\( 10^{-8} \\) |

### Insulators and blowout (`benchmarks/wirefe/textbook-insulators.toml`)

| Case | What | Source | Tolerance |
|---|---|---|---|
| tbins-001 | dead-end span blown out by \\( \arctan(qd/w) \\) = 41.741° | exact plane of the load + reference | \\( 10^{-7} \\), \\( 10^{-6} \\) m |
| tbins-002 | suspension string 3 m, 800 N, 0.4 m² in 500 Pa: 36.86° | pendulum formula (40.10°) corrected for the chords' skew + reference | \\( 10^{-7} \\), \\( 10^{-6} \\) m/° |
| tbins-003 | V-string in still air | \\( V / (2\cos\beta) \\) + reference | \\( 10^{-7} \\) |
| tbins-004 | V in 300 Pa, both legs taut | two-bar statics + reference | \\( 10^{-7} \\) |
| tbins-005 | V in 1000 Pa: the lee leg slack, the tip on the other leg | pendulum on one leg + reference | \\( 10^{-7} \\), \\( 10^{-6} \\) m |

### CIGRE TB 324 (`benchmarks/sagtension/cigre-tb324.toml`)

| Case | Brochure | Printed | Tolerance |
|---|---|---|---|
| tb324-001 | sec. 2.1–2.4 p. 11–15 | sag 6.420 m, length 300.366 m, \\( T = H + wD \\) | 0.0005 m, 0.5 N |
| tb324-002 | sec. 2.3 p. 13 | 7.241 m ↔ 24.8 kN | 50 N |
| tb324-003…010 | sec. 2.7, Table 2 p. 20 (and the example p. 19) | low point, both support tensions for h = 5…40 m, uplift from 30 m | 0.005 m, 0.5 N |
| tb324-011 | sec. 2.5 p. 16–17 | 2 m, 100 kg dead-end strings: sag 6.45 m | 0.005 m |
| tb324-012 | sec. 5.2.3–5.2.4 p. 37–39 | Drake: \\( E_{AS} \\) = 74 GPa, \\( \alpha_{AS} \\) | \\( 10^{-4} \\) (eq. (19)–(20) evaluated) |
| tb324-013 | sec. 5.2.1 p. 35 | A1-37 at 12 kN: 506 µstrain | 0.5 µstrain |
| tb324-014 | sec. 6.1 p. 57 | 430 Pa on 26.1 mm: 11.2 N/m, resultant 15.6 N/m | 0.05 N/m |
| tb324-015 | sec. 6.1 p. 57 | initial sag 8.0 m | 0.05 m |
| tb324-016 | sec. 6.1 p. 57–58 | linear elastic under wind: H = 20745 N, sag 8.5 m | 2.5 N (see below), 0.05 m |

Two cases need a word:

- **tb324-012**: the brochure prints \\( E_{AS} \\) = 74 GPa and
  \\( \alpha_{AS} \\) = 18.84·10⁻⁶/K, the latter evaluated with the
  rounded 74 GPa. Its equations with its printed inputs give 73.956 GPa
  and 18.851·10⁻⁶/K; the case checks these.
- **tb324-016**: the brochure's linear model stretches the cable by its
  horizontal tension, additively; OpenOHL by its mean tension,
  multiplicatively ([Sag-tension](../theory/sagtension.md#thermal-strain-two-forms-in-use)).
  The reference reproduces the brochure's 20745 N with its model
  (20744.73 N) and gives 20742.77 N with OpenOHL's: 1.97 N (0.0095 %).
  The tolerance is the printed rounding (0.5 N) plus that stated model
  difference (2.0 N).

**Not used** (the brochure's text is not copied; the reasons):

- *Table 3 and Fig. 13* (sec. 3.1, p. 23–25: 250 m and 350 m dead-end
  spans of Drake strung at 35 kN at 15 °C, at 100 °C 21177 N and
  24670 N): the brochure's own linear Drake data (sec. 5.2.3–5.2.4) give
  19735 N and 23598 N (7 % and 4 % lower); the elongation model behind the
  table is not stated, so its inputs are incomplete. The tilt of the
  string that follows depends on these tensions.
- *Table 4* (sec. 3.2, p. 27): the stringing tension of the two spans is
  not given (the dead-end tensions at 100 °C, 17.1 and 19.9 kN, are not
  those of Table 3).
- *Fig. 15* (p. 24–25: 20 cm of tilt balance 580 N): read from a graph;
  OpenOHL solves the equilibrium rather than taking a tilt; the moment
  balance is the one tbins-002 and tbrs-002 check.
- *sec. 5.3* (component tensions with 300 µstrain of aluminium set): no
  calculation of OpenOHL takes a plastic strain of the outer strands
  alone; the linear models take creep as a stretch of the whole cable.
- *sec. 6.2* (SPE model, Fig. 28): a graphical solution, and its
  installation stress (48 MPa, which gives the printed 8.0 m sag) and its
  "28.0 kN" disagree. Read as 48 MPa, the reference and OpenOHL agree
  with each other (19.90 kN, 9.04 m at 15 °C; 15.66 kN, 11.49 m at 100 °C)
  but not with the figure's readings (20.1 kN, 8.95 m; 15.9 kN, 11.3 m:
  1.5 % and 0.19 m), whose reading accuracy is unknown.
- *sec. 6.3, Tables 5, 6, 12, 14, 15* (EPE model, knee points, sensitivity
  studies): they need the conductors' stress-strain and creep curves,
  which the brochure does not give (graphs and a program's output).
- *sec. 2.5* clamps with bending stiffness, armour rods and a deflecting
  structure: not modelled (the structure's flexibility is roadmap point O7).
- *sec. 5.4.3* (creep formula, 649 µstrain): OpenOHL takes creep from
  curves or as a temperature shift, not from that formula.

## What the cases showed

- The code agreed with every independent reference to the tolerances
  above. One first disagreement (tbpl-002, 0.09 mm of sag) was a sign
  error in the reference script's search for the largest sag of a loaded
  span, found by the derivation now in the case's notes; the code was
  right.
- **Weight of a heated span in the finite elements** (documented in
  [Finite-element wire systems](../theory/wirefe.md#elastic-catenary-element)):
  a span's unstressed length grows by \\( e^{\alpha\Delta t} \\) and its
  load stays per unit of that length, so a hot span weighs more (and a
  cold one less) by \\( e^{\alpha\Delta t} - 1 \\). For the textbook
  cable strung at 10 °C: +0.04 % on H at 70 °C, +0.17 % at 150 °C
  (sag +7 mm and +13 mm), −0.04 % at −20 °C. The cases follow the code's
  model; the size is from the reference.
- The textbook model differences are as documented: the ruling-span
  method's additive thermal strain (+0.028 % on H at 80 °C in tbrs-004),
  the elastic wire's weight per unstressed metre (tb324-011: 5 mm less
  sag than the inextensible catenary at 28 kN).

## Not covered yet

Time-based creep (not modelled), non-linear stress-strain against
published curves (none provided), thermal ratings against a published
TB 601 example (none provided yet), and bundles and jumpers by textbook
cases.
