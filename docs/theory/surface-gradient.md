# Conductor surface gradient

*Built 2026-11-04, 2D only, in active development, not approved yet*
(roadmap O18 item 7; `crates/ohl-electrical`, `surface_gradient`; the
window Checks › Electrical › Electric field… with its 2D cuts in
OHL-Design and `ohl electrical gradient` on the command line). The owner,
2026-11-04: "only for the 2D cuts for the noise do not apply it in 3D as
these are different usage". The electric field's 3D model and its 2D
field values are unchanged by it.

The **surface gradient** is the electric field strength at the surface
of a conductor. Corona — and with it the audible noise and the radio
interference of a line — starts where it is largest, so the corona noise
formulas take it as their main input. This page says how the program
computes it, how far it can be trusted, and what it leaves out. Every
choice is marked **conservative** (the gradient computed is at least the
real one), **not conservative**, or neither, as far as that can be told.

## What is computed

At each cross-section asked, for every subconductor of every phase and
for every earth wire:

- its **largest surface gradient** round it and where (the angle from
  the horizontal to the right looking ahead, counter-clockwise: 90° the
  top, −90° the bottom);
- its **mean** surface gradient, \\( |Q|/(2\pi\varepsilon_0 r) \\) for
  its charge \\( Q \\) per metre and radius \\( r \\) (Gauss's law: exact
  for any charge distribution round it).

Per bundle (a single wire: a bundle of one):

- the **average maximum** — the mean over its subconductors of each
  one's largest gradient. This is the value the corona noise formulas
  take, and the one hvlbuzz reports for a bundle;
- the **maximum** — the largest of them;
- **Markt–Mengele's** average maximum from the bundle's charge, beside
  them as a cross-check ([below](#markt-and-mengeles-formula)).

All in **kV/cm, rms**: the voltages are rms phasors, the normal field at
a point of the surface is a phasor, and its magnitude is its rms value
(frequencies added as \\( \sqrt{\sum_f E_f^2} \\), as for the electric
field). The **peak** is \\( \sqrt 2 \\) times the rms. The noise methods
take the rms: hvlbuzz's corona noise takes its bundle gradients
computed from rms voltages; which value each published noise formula
takes is checked against its primary source when the noise method is
built (roadmap O18 item 6).

## Why the electric field's charges are not enough

The electric field ([Electric field](electric-field.md)) takes one line
charge per bundle at its centre (2D) or equal shares on the
subconductors' axes (3D). Both give the field away from the wires
exactly enough (0.12 % against hvlbuzz at 1 m above the ground), but not
the field *at* a subconductor's surface. A cylinder in an outside field
\\( E \\) carries induced charges that add \\( 2E\cos\theta \\) at its
surface; a charge on its axis gives only its own uniform
\\( Q/(2\pi\varepsilon_0 r) \\), and its neighbours' field there is only
\\( E\cos\theta \\). On a two-conductor 380 kV bundle (case A below) the
mean gradient \\( Q/(2\pi\varepsilon_0 r) \\) is 7.1 % below the average
maximum. In a noise formula with a term \\( 120\log_{10} g \\), 4 % in
\\( g \\) is 2 dB: hence a calculation of its own.

## The method: a charge simulation

The charge simulation method (Singer, Steinbigler and Weiss, 1974)
replaces the surface charge of each conductor by line charges inside it,
chosen so that the potential at points on its surface equals its
voltage. In the cross-section:

1. Every subconductor and earth wire is a smooth **cylinder** of its
   wire's radius \\( r \\) at its place in the cross-section (a bundle's
   subconductors by its count, spacing and rotation, as drawn), parallel
   to level, **perfectly conducting ground** at the cross-section.
2. Inside each, \\( n \\) **line charges** \\( q_j \\) on a circle of
   radius \\( a = 0.3\,r \\) at the angles \\( 2\pi j/n \\)
   ([below](#how-many-charges-and-where)); on its surface \\( n \\)
   **collocation points** at the same angles.
3. The potential at collocation point \\( i \\) of all charges and their
   images in the ground,
   \\[ \varphi_i = \sum_j \frac{q_j}{2\pi\varepsilon_0}
      \ln\frac{|\vec x_i - \vec c_j^{\,\prime}|}{|\vec x_i - \vec c_j|}, \\]
   equals the wire's voltage to earth: a phase its system's voltage
   between phases over \\( \sqrt 3 \\) at its phase angle (the
   circuit's voltage given, else the system's nominal, said), an earth
   wire zero. One real system \\( P\,q = V \\) of \\( n \\) unknowns per
   cylinder, solved once (LU) per cross-section and frequency for the
   real and the imaginary part of the phasors \\( V \\).
4. The field of the charges and their images,
   \\( \vec E = \sum_j \frac{q_j}{2\pi\varepsilon_0}\left(
   \frac{\vec x - \vec c_j}{|\vec x - \vec c_j|^2} -
   \frac{\vec x - \vec c_j^{\,\prime}}{|\vec x - \vec c_j^{\,\prime}|^2}\right) \\),
   is evaluated at **360 points** round every subconductor (one a
   degree), its component normal to the surface taken (the tangential
   one is the method's error, nearly zero).

A **self-check** per cross-section: the largest potential error at the
points midway between the collocation points, as a share of the largest
voltage (some \\( 3\times10^{-6} \\) on the cases below; reported with
each cross-section).

## How many charges and where

Measured (`cargo test -p ohl-electrical --release convergence_table --
--ignored --nocapture`, 2026-11-04) on the closed forms and on the
cross-sections of the validation: A, B, C (hvlbuzz's three, below) and
D, a 420 kV flat circuit of four-conductor bundles (0.45 m, 31.8 mm) at
18 m with two earth wires. For the closed forms the relative error of
the largest gradient against the exact value; for A–D the largest
relative difference of a bundle's average maximum from that with 64
charges.

**At \\( a = 0.3\,r \\)** (the default):

| n | wire 15.9 mm at 20 m | 0.5 m at 2 m | two 0.1 m, 0.6 m apart | 1 m at 1.2 m | A | B | C | D |
|---|---|---|---|---|---|---|---|---|
| 4 | 7.9e-3 | 1.0e-2 | 9.2e-3 | 1.8e-2 | 0.97 % | 0.79 % | 0.79 % | 1.3 % |
| 6 | 6.1e-4 | 5.2e-3 | 9.3e-3 | 0.20 | 0.087 % | 0.066 % | 0.071 % | 0.0074 % |
| **8** | **6.5e-5** | **2.2e-4** | **3.0e-4** | 7.0e-3 | **0.0079 %** | **0.0052 %** | **0.0064 %** | **0.0097 %** |
| 12 | 5.3e-7 | 3.7e-6 | 7.7e-6 | 2.2e-3 | 0.0001 % | 0.0001 % | 0.0001 % | 0.0001 % |
| 16 | 4.3e-9 | 5.6e-8 | 1.9e-7 | 6.2e-4 | < 1e-6 | < 1e-6 | < 1e-6 | < 1e-6 |
| 32 | 2e-15 | 2e-15 | 9e-14 | 3.2e-6 | — | — | — | — |
| 64 | 8e-12 | 5e-15 | 9e-16 | 1.4e-8 | (reference) | | | |

**At \\( a = r/k \\), \\( k = 1 + 2\sin(\pi/n) \\)** (the placement
hvlbuzz's source uses): the field between the collocation
points ripples with the factor \\( (1/k)^n \\), which tends to
\\( e^{-2\pi} \approx 0.002 \\) instead of falling — on the wire at
20 m the largest gradient was 1.1 % high with 8 charges, 0.51 % with 16,
0.33 % with 32 and still **0.25 % with 64**; on case A 0.25 % high with
64 (14.909 against the converged 14.872 kV/cm). At \\( a = 0.5\,r \\) it
converges too, more slowly than at 0.3 r on these geometries (case A:
0.026 % with 12 charges, 0.0016 % with 16).

**The default: 0.3 r and 8 charges** — the smallest count within 0.05 %
of 64 on every benchmark (the cross-sections A–D within 0.01 %, the
closed forms within 0.03 %; the owner's rule, 2026-11-04). More charges
are a setting (`--charges`; 12 brings the cross-sections to 1e-6). The
time (release build): under 1 ms per cross-section for A–C and 1 ms for D
with 8 charges, 9–90 ms with 64. Only a cylinder nearly touching its
neighbour or the ground (the 1 m cylinder 0.2 m above the plane) needs
more (16: 0.06 %); a line's wires are tens of radii apart.

## Markt and Mengele's formula

The average maximum gradient of a bundle of \\( n \\) subconductors of
radius \\( r \\) on a circle of radius \\( R \\) carrying \\( Q \\) per
metre in all:

\\[ g_{\text{av max}} = \frac{Q}{n\,2\pi\varepsilon_0 r}
    \left(1 + (n-1)\frac{r}{R}\right). \\]

Derived: each subconductor carries \\( Q/n \\); the other \\( n-1 \\)
give at its centre a field pointing away from the bundle's centre of
\\( \frac{Q/n}{2\pi\varepsilon_0}\sum_{j\ne i}\frac{\cos\alpha_j}{d_j} =
\frac{Q/n}{2\pi\varepsilon_0}\frac{n-1}{2R} \\) (on the regular polygon
each term is \\( 1/(2R) \\)); a conducting cylinder in a uniform outside
field \\( E \\) has \\( 2E\cos\theta \\) added at its surface; so its
largest gradient is the mean \\( \frac{Q/n}{2\pi\varepsilon_0 r} \\) plus
\\( \frac{Q/n}{2\pi\varepsilon_0}\frac{n-1}{R} \\). The other phases'
and the ground's field at the bundle are left out (they are far), so
the formula is a cross-check, not the result. \\( Q \\) is the bundle's
charge from the potential coefficients with the bundle as its equivalent
wire (the electric field's). A single wire: its mean
\\( Q/(2\pi\varepsilon_0 r) \\).

Measured against the charge simulation: on case A 0.25 % (outer phases)
and 0.20 % (middle) **above** it — on the conservative side there; on
single wires up to 0.6 % **below** (B, C: the other phases' field, left
out of the formula, raises one side) — not conservative there.

## Where

**2D cross-sections only** (the owner, 2026-11-04): per span at its
**lowest point** (the lowest phase conductor's) and at **both ends** —
at the structures, the first cross-section every wire of the span reaches
(with strain strings: where the conductor leaves the string); or at the
places asked (a span and a fraction of it). Never in 3D: the 3D electric
field keeps its own charges, and the gradient is not computed from them.

- **OHL-Design**: Checks › Electrical › Electric field…, *Surface
  gradient* (on by default when the run is 2D — cuts and profiles): at
  the profiles' spans; the results under **Conductor surface gradient,
  2D**, one row per bundle and cross-section, **Export CSV…** per
  subconductor.
- **Command line**: `ohl electrical gradient LINE.ohl.json|PROJECT.ohl
  [--voltage CIRCUIT=KV] [--profile FROM:TO[@FRACTION]] [--no-ends]
  [--charges N] [--subconductors] [--csv FILE]`.

## Validation

**Exact closed forms** (`benchmarks/electrical/gradient.toml`,
`reference_gradient.py`). A cylinder of radius \\( r \\) whose axis is
\\( h \\) above a conducting plane is an equipotential of one line
charge at \\( d = \sqrt{h^2 - r^2} \\) and its image (bipolar
coordinates): \\( q = 2\pi\varepsilon_0 V/\ln((h+d)/r) \\), the surface
field \\( \frac{q}{2\pi\varepsilon_0}\frac{2d}{ab} \\) at distances
\\( a, b \\) from the charge and the image, largest at the bottom,
\\( E_{\max} = \frac{V d}{r(h-r)\ln((h+d)/r)} \\). Two cylinders at
\\( \pm V \\) in free space are the same (the plane midway at zero). With
the default 8 charges:

| Case | Largest | Smallest | Angle |
|---|---|---|---|
| electrical-069: 31.8 mm wire at 20 m, 231 kV | +6.5e-5 | −1.6e-5 | −90° exactly |
| electrical-070: 0.5 m cylinder at 2 m, 100 kV | +2.2e-4 | +4.2e-6 | −90° exactly |
| electrical-071: two 0.1 m cylinders 0.6 m apart, ±10 kV, free space | +3.0e-4 | −3.6e-5 | −90° exactly |

Tolerance 0.1 %. Crate tests besides: the surface field at 72 angles
within 0.1 % of the closed form (64 charges, three geometries,
tangential field below 0.2 %), the convergence with the charges, an
earth wire's surface at zero potential, a symmetric bundle's mirror
gradients, the gradient proportional to the voltage, and on the example
double circuit every span's three cross-sections, the two circuits'
mirror bundles alike and Markt–Mengele within 3 %.

**Against hvlbuzz** (`benchmarks/electrical/hvlbuzz-gradient.toml`).
hvlbuzz — *HVLBuzz 2.0*, by Michael Kefeder, Gerhard Bräunlich and Joel
Winiger (the authors its `Cargo.toml` names), © FKH, Switzerland
([fkh.ch](https://fkh.ch)); source
[gitlab.com/fkh-ch/emc](https://gitlab.com/fkh-ch/emc) (default branch
`release`, version 2.0.0-rc2 there on 2026-11-04; the build used here
was made from that repository on 2026-10-29); documentation
[fkh-ch.gitlab.io/emc/hvlbuzz/index.html](https://fkh-ch.gitlab.io/emc/hvlbuzz/index.html);
licence GNU General Public License version 3 (its `LICENSE` file and
README badge) — computes the electric and magnetic fields and the corona
noise of a line's cross-section with a charge simulation of its own. It
is open source and was used here as the **independent reference**; it
is not part of OpenOHL and none of its code is in it (its method was
implemented independently from the publications above). The OpenOHL
owner thanks its authors and FKH: hvlbuzz has been of great help to him
before for understanding these calculations.

Its bundle table (`E_max (bundle, AC)`, kV/cm) was taken with the
project's `n_contour` set to 360 — its charge simulation, the largest
gradient over its contour points averaged over the bundle — and to 2 —
below 3 it gives the bundle's charge shared by its subconductors,
\\( |q|/(2\pi\varepsilon_0 r) \\) times \\( 1 + (n-1)r/R \\), Markt and
Mengele's formula. Its default (12) is kept for the record. The cases:
A one 380 kV circuit, flat (−9, 0, 9 m at 20 m), two-conductor bundles
0.4 m, 31.8 mm; B a 110 kV double circuit (±5 m, 22/27/32 m, 28.1 mm);
C the same with an earth wire (0, 38 m, 16 mm) at earth potential.

| Case | Bundle | hvlbuzz, 360 | OpenOHL, 8 | OpenOHL, 64 | hvlbuzz, Markt–Mengele | OpenOHL, Markt–Mengele |
|---|---|---|---|---|---|---|
| A (electrical-066, -072) | L1, L3 | 14.87165 | 14.87289 (+8.3e-5) | 14.87171 (+3.9e-6) | 14.91223 | 14.90859 (−2.4e-4) |
| | L2 | 16.18764 | 16.18899 (+8.3e-5) | 16.18770 (+3.9e-6) | 16.22855 | 16.22084 (−4.8e-4) |
| B (electrical-067) | L1 | 7.19287 | 7.19312 (+3.4e-5) | | 7.15478 | 7.15478 (+4e-7) |
| | L2 | 7.83642 | 7.83686 (+5.6e-5) | | 7.83272 | 7.83273 (+8e-7) |
| | L3 | 6.97068 | 6.97100 (+4.6e-5) | | 6.92954 | 6.92954 (+4e-7) |
| C (electrical-068, -073) | L1 | 7.15553 | 7.15577 (+3.4e-5) | 7.15556 (+3.9e-6) | 7.11772 | 7.11772 (+4e-7) |
| | L2 | 7.81182 | 7.81235 (+6.8e-5) | 7.81185 (+3.9e-6) | 7.80844 | 7.80844 (+8e-7) |
| | L3 | 7.14807 | 7.14846 (+5.5e-5) | 7.14810 (+3.9e-6) | 7.11126 | 7.11126 (+4e-7) |
| | earth wire | 2.08858 | 2.08873 (+6.9e-5) | 2.08859 (+3.9e-6) | 2.07382 | 2.07382 (+1.9e-6) |

(kV/cm rms; the second system of B and C alike, by symmetry.) Converged
(64 charges against hvlbuzz's 360, whose own 1000 differ from them by
3e-7), the two programs agree within **3.9e-6**, the same ratio on every
wire. The default 8 charges are within 1e-4 (tolerance 2e-4; 64 charges
2e-5). Markt–Mengele agrees within 2e-6 on single wires and within 5e-4
on the bundles (each program's bundle charge; tolerance 1e-3). hvlbuzz's
own default of 12 charges reads 3e-5 (A) to 1.3e-4 (C) below its 360.

**Not validated yet** against a published worked example: none was found
in a source that could be read (to add).

## Assumptions and what is left out

- **A smooth cylinder.** The stranding raises the gradient locally on
  the strands' crowns; the corona noise formulas are written for the
  smooth conductor's gradient and stand for the stranding and the
  surface's state with their own factors (as Peek's onset gradient does
  with its surface factor). Neither conservative nor not: the value the
  methods expect.
- **Level, perfectly conducting ground at the cross-section.** The
  gradient depends on the height only through \\( \ln(2h/r) \\): 10 %
  lower wires raise it some 1.4 % (a wire of 31.8 mm at 20 m). The
  cut's own wire heights are taken, so the sag is seen; the terrain
  across the line is not (a bank closer to the wires raises it a
  little): not conservative there, negligible elsewhere.
- **2D: infinite, straight, parallel wires** at the cross-section's
  heights. At mid-span exact for the purpose. At the spans' ends the
  wires are not parallel and meet insulators, jumpers and the structure:
  an approximation there.
- **The structures' steel, corona rings, spacers, dampers and fittings
  are left out.** Earthed steel close to a conductor raises its gradient
  on the side facing it (not conservative near the structures); the
  fittings are designed to stay below corona onset and are not part of
  a conductor's gradient.
- **The earth wires at zero potential** (continuously earthed); their
  gradient from their induced charge is reported too.
- **The voltage**: the circuit's given, else the system's nominal (said).
  Noise is usually assessed at the highest operating voltage: giving
  only the nominal is **not conservative**.
- **No space charge** (AC), no rain or water drops on the surface (the
  noise methods' weather terms), the air of permittivity
  \\( \varepsilon_0 \\). DC systems left out (a static field with space
  charge).

## Standards and sources

- H. Singer, H. Steinbigler, P. Weiss, "A Charge Simulation Method for
  the Calculation of High Voltage Fields", *IEEE Transactions on Power
  Apparatus and Systems*, vol. PAS-93, no. 5, pp. 1660–1668, September
  1974, doi:[10.1109/TPAS.1974.293898](https://doi.org/10.1109/TPAS.1974.293898)
  — the charge simulation method (bibliographic data verified in
  Crossref, 2026-11-04).
- G. Markt and B. Mengele — the bundle formula that bears their names.
  Their original publication has not been read yet: the formula is
  derived on this page instead, and its primary source is still to be
  added.
- hvlbuzz (above): *HVLBuzz 2.0*, M. Kefeder, G. Bräunlich, J. Winiger,
  © FKH; [gitlab.com/fkh-ch/emc](https://gitlab.com/fkh-ch/emc);
  [fkh-ch.gitlab.io/emc/hvlbuzz/index.html](https://fkh-ch.gitlab.io/emc/hvlbuzz/index.html);
  GNU GPL version 3 — the independent reference of the validation.
