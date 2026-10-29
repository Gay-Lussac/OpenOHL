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

### The ground

**By default a tiled conducting floor** (owner, 2026-10-29, after the
comparison below: "keep the tiled implementation"): around each profile,
each group of given points near one another along a span (they share a
floor around their middle, its fine tiles reaching the farthest across)
and each cut (the cross-section panels' too), the floor below is solved;
some seconds each (its dense system grows with the tiles' cube: about
7 s for 3 700 tiles here). **Cuts across the line and volumes on coarser
tiles by default, each a parameter** (owner, 2026-10-29): a cut's floor
of 8 m tiles (`cut_tile`); a volume's floor under its whole run of spans
and 100 m beyond every way, square tiles of 8 m (`volume_tile`). Measured
against the exact field on even ground, from 1 m above it up: 16 m tiles
8.3 % of the largest value, **8 m 2.8 %**, 4 m 0.16 % (a 300 m span: 0.5 s,
1 s and 39 s). A tile near the point is summed finer the nearer the point
(points closer together than its distance to the tile, up to 96 per
side): next to coarse tiles the field stays right. The plane under each
point is kept as the quick choice.

#### The plane under each point

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

### The plane against a tiled floor (2026-10-29)

The owner asked for "a comparison with a tiled floor … in a valley area
as well as flatland" on a few extracts of a real line. The reference
(`GroundModel::Tiled`, `ohl electrical electric --tiled`): around a
profile's station, the terrain as a floor of square tiles — 2 m within
30 m along the line and 10 m beyond the profile across, 8 m beyond, to
200 m every way — each a quadrilateral on the terrain with a uniform
charge per square metre, solved so that the floor is at zero potential
at every tile's centre under the line's charges (a dense system, some
4 000 tiles, about 10 s per profile); the field the line's charges and
the tiles' (each tile near the point as 6 × 6 point charges).

- **The reference's own error**, on the synthetic example's evenly
  sloping ground where the plane is exact: within 0.2 % of the profile's
  largest value (tested). On real terrain, from 4 m to 2 m tiles the
  profiles moved by 1–3 % of their largest value (15 % on one rough
  extract), the largest values by under 3 %.
- **A real alpine line** (the ground from its LiDAR ground classes, the
  lowest point per metre; 1 m above the ground, ±40 m, the systems'
  nominal voltages), six extracts:

| Extract | Terrain off the plane within ±30 m | Largest E, plane | Tiled | Plane against tiled at the largest | Along the profile, at most |
|---|---|---|---|---|---|
| valley floor A | 4.6 m | 0.162 kV/m | 0.175 kV/m | −7.8 % | 14 % of the largest |
| valley floor B | 5.0 m | 0.213 | 0.223 | −4.5 % | 31 % |
| valley side A (200 m rise per 500 m) | 18 m | 1.020 | 1.087 | −6.2 % | 9 % |
| valley side B | 5.4 m | 0.770 | 0.800 | −3.8 % | 8 % |
| slope C | 4.5 m | 0.908 | 0.911 | −0.4 % | 9 % |
| slope D | 2.2 m | 0.736 | 0.749 | −1.7 % | 5 % |

**Exactly, in a V-valley** (owner: "can you test the per point plane
too?"; a unit test): a long wire 20 m above the bottom of a 90° V
(sides at 45°, z = |y|) has the exact field of its charge and three
images (a 90° wedge). 1 m above the ground across the valley:

| Across the valley | Exact | Plane under each point | Tiled floor |
|---|---|---|---|
| 0 m (the bottom) | 180 V/m | 1 802 | 82 |
| 5 m | 1 374 | 2 274 | 1 365 |
| 10 m (the largest) | 2 054 | 2 542 (+24 %) | 2 065 |
| 20 m | 962 | 1 209 | 982 |

The tiled floor is within 4.8 % of the largest value everywhere; the
plane under each point **overestimates everywhere in the hollow** — the
sides shield it, which a plane cannot see — by 24 % at the largest and
tenfold at the bottom (**conservative** there).

So the plane under each point **underestimates the largest field by up
to some 8 % on the real line's bumps and slopes — not conservative** —
and overestimates in hollows (conservative) — and differs locally
by 5–30 % of the largest value (more where the field is weak: a bump of
the ground raises the field on it, a hollow lowers it, which a plane
cannot follow). Each profile says how far the terrain departs from the
plane under its largest value (`ground_departure`); where it is metres,
take the tiled floor for the places that matter (`--tiled`, profiles
only for now) or add a margin.

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
