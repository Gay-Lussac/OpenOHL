# Electric field

*Built, in active development, not approved yet* (roadmap O18 step 5,
2026-10-29: the engine and the command line; the window in OHL-Design
next). The electric field strength (kV/m) of the line's voltages, from
the wires where they hang in a weather case.

## What is computed

The rms of the field's vector,
\\( E = \sqrt{|E_x|^2 + |E_y|^2 + |E_z|^2} \\), kV/m, at the profiles'
points (1 m above the ground by default), at given points, in vertical
cuts and as isosurfaces in volumes — the same places and views as the
[magnetic field](magnetic-field.md#where), drawn and checked the same
way (the [cuts and volumes](magnetic-field.md#cuts-volumes-and-what-they-are-drawn-with)).
Systems of different frequencies are computed apart and added as
\\( \sqrt{\sum_f E_f^2} \\). DC systems are left out (a static field
with space charge: another calculation).

## The charges

Per span, from its mean cross-section (the [line
parameters](line-parameters.md)' cross-section: the wires at their mean
height, each bundle as its equivalent wire):

\\[ \underline q = P^{-1}\,\underline V \\]

with \\( P \\) the potential coefficients with images in a perfectly
conducting earth (\\( p_{ij} = \ln(D'_{ij}/d_{ij}) / (2\pi\varepsilon_0) \\),
a wire's own with its equivalent radius) and \\( \underline V \\) the
voltages to earth: each phase its circuit's voltage between phases over
\\( \sqrt 3 \\) at its phase angle, every earth wire at zero (it takes
the charge that keeps it there). Each frequency is solved apart, the
phases of the others at zero. The voltage per circuit is yours to give;
a circuit without one takes its **system's nominal voltage, said** — an
ordinance may ask the highest operating voltage (the NISV does: give it).
A circuit given 0 kV is voltage-free: its wires at earth potential,
carrying the charges that keep them there (as the earth wires).

- **Simplification**: the charges per span from its mean cross-section
  (parallel wires at their mean height over level ground), as the line
  parameters take them; along a span they are taken constant.
- **Simplification**: a bundle's charge shared equally by its
  subconductors.

## The 3D model

Every wire as solved — a bundle subconductor by subconductor, the
[jumpers](magnetic-field.md#the-3d-model) with their phase's charge per
metre (an option, on by default) — as straight pieces of uniform charge
\\( \lambda \\) (C/m). The field of a piece from \\( a \\) to \\( b \\)
at \\( p \\) in closed form (Coulomb's law integrated along it):

\\[ \vec E = \frac{\lambda}{4\pi\varepsilon_0} \left(
    \frac{\vec d}{\rho^2}\left(\frac{L - t}{|\vec r_b|} + \frac{t}{|\vec r_a|}\right)
    + \hat u \left(\frac{1}{|\vec r_b|} - \frac{1}{|\vec r_a|}\right) \right) \\]

with \\( \hat u \\) the piece's direction, \\( L \\) its length,
\\( t = (p - a)\cdot\hat u \\), \\( \vec d \\) the perpendicular from the
piece's line to \\( p \\) (length \\( \rho \\)), \\( \vec r_a = p - a \\),
\\( \vec r_b = p - b \\). Tested against the infinite line charge
\\( \lambda/(2\pi\varepsilon_0\rho) \\) and against the integral
evaluated numerically (Simpson, \\( 10^{-8} \\)). The pieces are summed
per polyline as for the magnetic field (each vertex's distance once,
blocks of points), with the same choices of piece length.

### The ground: a plane under each point

The ground is a perfect conductor (at power frequency the earth's
conductivity is high enough for the electric field). Under each point of
evaluation the program fits a **plane to the terrain** — its elevation
there and its slopes from the elevations a radius either side in x and
in y (`ground_radius`, 10 m by default; without a terrain, straight
between the structures' feet and level across) — and takes the images of
all charges in that plane, with the opposite sign. The images' field at
\\( p \\) is the charges' field at \\( p \\)'s mirror point, mirrored
back (a reflection keeps every distance), so it costs one more sum.

Why this ground (owner, 2026-10-29, asked "what's easier and actually
better"): it is continuous along the line (the plane changes with the
point, no seam at a structure as a plane per span would leave), exact for
level or evenly sloping ground, and as costly as one more sum; where
ordinances ask the field — 1 m above the ground — objects (trees,
buildings, fences) shield it far more than the terrain's curvature
changes it. **Simplification on uneven ground**: in a hollow or on a
ridge the real ground is not the plane; a conducting terrain surface
(charges on its triangles) would follow it, at the cost of a large dense
system — later, if a comparison shows it matters.

- **Not modelled**: the structures (earthed steel: they lower the field
  near them), trees, buildings and people (earthed objects shield the
  field: the field computed is **conservative** away from them and **not
  conservative** at their tops, where it concentrates).

## The 2D model

A cross-section's wires as infinite line charges with their images in
level ground at the cross-section:
\\( \vec E = \frac{q}{2\pi\varepsilon_0}\left(\frac{\vec r}{r^2} - \frac{\vec r'}{r'^2}\right) \\),
the same charges as above. Tested: one wire over level ground, the field
at the ground below it \\( q/(\pi\varepsilon_0 h) \\), in 2D and from a
long 3D wire with its image; a wire over a tilted plane gives the field of
the level case turned with it.

## Validation

Tests in the crate: the closed forms above; on the synthetic double
circuit the symmetry of a profile, the field proportional to the voltage,
3D against 2D at mid-span (within 10 %), a cut and a volume with their
isolines' and surfaces' distance off their levels. **Not validated yet**
against a published profile (to find: IEC 62110's examples, CIGRE's
brochures on fields of overhead lines, the NISV guide's examples).

## Standards and sources

- BAFU, *Hochspannungsleitungen. Vollzugshilfe zur NISV* (2007, draft
  for trial), 8.7: the electric field is computed (measuring is not
  recommended) for the operating state of the largest field — the
  **highest operating voltage** (380 kV lines: 420 kV), the **largest
  sag** (the approved profiles' wire positions), and on a double circuit
  whose phasing compensates **one circuit voltage-free** — at the
  accessible place of the largest field, **1 m above the ground**
  (usually under the line where it is lowest; on a building's balcony or
  roof terrace where one stands near). In the program: the voltages per
  circuit (`--voltage CIRCUIT=KV`; a circuit voltage-free: `=0`, its
  wires then at earth potential), the weather case of the largest sag,
  the height (1 m by default); none of these is chosen for you.
- IEC 62110:2009, *Electric and magnetic field levels generated by AC
  power systems — Measurement procedures with regard to public exposure*.
- EPRI, *Transmission Line Reference Book, 345 kV and Above* (the
  charge simulation of line conductors over a conducting earth).
