# Electric field

*Built, in active development, not approved yet* (roadmap O18 step 5,
2026-10-29; the window Checks › Electrical › Electric field… in
OHL-Design and `ohl electrical electric` on the command line, both on
the same engine, `crates/ohl-electrical`, `electric`). The electric field
strength (kV/m) of the line's voltages, from the wires where they hang
in a weather case.

Every choice below is marked **conservative** (the field computed is at
least the real one), **not conservative**, or neither, as far as that
can be told. The program computes the field; **which voltage, which
places and which limit** an ordinance asks for are yours to give (or a
plugin's parameter file), never built in.

## What is computed

The rms of the field's vector,

\\[ E = \sqrt{|\underline E_x|^2 + |\underline E_y|^2 + |\underline E_z|^2} \\]

in kV/m, with \\( \underline E_x, \underline E_y, \underline E_z \\) the
complex rms phasors of the three components. It is computed:

- on **profiles** across the line (1 m above the ground by default);
- at **given points** (e.g. places of sensitive use);
- in vertical **cuts** across the line, with the levels' isolines;
- in **volumes** over runs of spans, as the levels' isosurfaces.

![Three ways to see the field: a profile, a row of points at a height above the ground with the field plotted along it; a cut, a grid of points in a vertical plane with isolines; a volume, a lattice in a box with surfaces around the wires](../images/theory/field-views.svg)

These are the same places and views as for the [magnetic
field](magnetic-field.md#where), drawn and checked the same way ([cuts
and volumes](magnetic-field.md#cuts-volumes-and-what-they-are-drawn-with)).
Systems of different frequencies are computed apart and added as
\\( E = \sqrt{\sum_f E_f^2} \\). DC systems are left out (a static field
with space charge needs another calculation; said).

**What it is compared to.** A limit (kV/m) is optional and yours to give:
each point then shows its margin (limit − E). No limit is built in. The
Swiss enforcement guide (BAFU, NISV guide, 8.7) asks for the field at the
highest operating voltage, the largest sag and, on a double circuit built
to compensate, one circuit voltage-free, 1 m above the ground (see
[Standards and sources](#standards-and-sources)). The program takes each
of these as an input; it chooses none of them for you.

## Inputs and parameters

The window's names, in the order of its Setup tab. Defaults are those of
a new window; the command line has the same defaults (its option in
brackets). "Time" is the effect on how long a run takes.

| In the window | Default | Effect on the results | Effect on the time |
|---|---|---|---|
| **2 Scope** › weather case | the first case | the wires' places (sag, swing) as solved in it; for the NISV guide the case of the largest sag | a case not solved yet is solved first |
| voltage between phases, per circuit (`--voltage C=KV`) | the system's nominal voltage, said | the field in proportion; each phase at it over \\( \sqrt3 \\) to earth, at its system's phase angle; **0 kV**: the circuit voltage-free (its wires at earth potential, like the earth wires) | none |
| Model: **3D** / 2D (`--two-d`) | 3D | 3D: the wires as solved (sag, line angles, the line's length), the jumpers, the ground under the line; 2D: infinite straight wires over level ground at the profile | 2D: a moment; 3D: seconds to minutes |
| Jumpers: carry their phase's charge | on | near the structures; off: left out (**not conservative** there) | a few more pieces |
| Ground: **tiled floor** / plane under each point (`--plane`) | tiled floor | the floor follows hollows and ridges; the plane read the largest field up to 8 % low on bumps (**not conservative**) and up to 24 % high in a hollow | the floor: about a second per profile; the plane: none |
| tiles: profiles and points (`--tile`) | 2 m | the floor's fine tiles; from 4 m to 2 m the profiles moved 1–3 % | grows with the tiles' number squared to cubed (1 m: about 8 times 2 m) |
| tiles: cuts (`--cut-tile`) | 8 m | 8 m: within 2.8 % of the largest value on even ground (16 m: 8.3 %, 4 m: 0.16 %) | as above |
| tiles: volumes (`--volume-tile`) | 8 m | as for cuts | as above; the floor of a long run of spans may exceed 2 GB (refused, said before Run) |
| fitted over ± (plane only, `--ground-radius`) | 10 m | the plane's slope: short follows the ground right under the point (a bank, a ditch), long the slope at large | none |
| Limit: given | none | the margin at every point | none |
| Points from CSV… | none | the field at each point: x, y and z (elevation) or height (above the ground), optionally a name | one floor per group of points near one another |
| **3 Spans** › the span table | spans on an alignment ticked, others not | where profiles, cuts and volumes are computed | in proportion to the spans |
| Profiles | at each ticked span's lowest point | the place: the lowest point of the span's lowest phase conductor; *beside its structures* (10 m into the span from each); *one span* (its lowest point or a place 0–1 along it) | one floor and one row of points per profile |
| Across: ± / every / at | ± 50 m, every 1 m, at 1 m above the ground | the profile's width, its points' spacing and height | in proportion to the points |
| 2D cuts at (+ station, in the table) | none | a cut at mid-span, at the span's lowest point or at a distance from its first structure | one floor and one grid per cut |
| 2D cuts: ± / every / up to | ± 60 m, every 0.5 m, up to 10 m over the highest wire | the cut's size and grid | in proportion to the points |
| Volumes: every (Volume ticked in the table) | 2 m | how closely the surfaces follow their levels (measured after each run) | halving the step takes 4 to 8 times as long |
| **4 Model** › Structures' steel: earthed (`--steel`) | off | lowers the field near the structures, raises it at the steel; on the tiled floor only | a profile beside two structures adds their pieces to the floor's system |
| Wire pieces: as solved / at most | as solved | how far a wire lies off its straight pieces (millimetres); the field moves by about that over the distance | in proportion to the pieces |
| **5 Views and levels** › levels | 1 and 5 kV/m on, 10 kV/m off | drawing only: the isolines and isosurfaces; none is a limit | each level a little |
| colour the cuts by band | on | drawing only | none |
| Cross-sections: also in any cross-section panel I open | off (0.5 m when on) | the field in the plane of each cross-section view opened, after a run | per view opened |

The window says the time of each choice before Run, at the speed of the
last run on this computer (before a first run: a short measurement).
The command line has three more floor settings: `--coarse-tile` (8 m),
`--floor-reach` (200 m) and `--lidar-ground` (the ground from the point
cloud even when a surface of the checks exists).

## How it works, step by step

1. **The wires.** The line is solved in the weather case: each wire's
   polyline per span (a bundle subconductor by subconductor), each
   jumper's shape.
2. **The charges.** At many cross-sections along each span the charges
   per metre of all wires follow from the voltages,
   \\( \underline q = P^{-1}\underline V \\) ([The
   charges](#the-charges)).
3. **The 3D model.** Every wire becomes straight pieces of uniform
   charge, each span in stretches (24 by default) at their own charges
   ([The 3D model](#the-3d-model)).
4. **The ground.** For each profile, group of points, cut and volume, a
   tiled floor on the terrain is solved so that it is at zero potential
   under the line's charges; with the structures' steel if asked ([The
   ground](#the-ground)). Or, quickly, a plane under each point with the
   charges' images.
5. **The field.** At every point, the field of every piece of charge, of
   every tile (or every image) and of every piece of steel is added per
   component, as phasors, then \\( E \\) as above.
6. **The views.** Profiles are plotted; cuts and volumes are drawn
   between their points (isolines, isosurfaces), and how far the drawing
   lies off its level is measured ([magnetic field: cuts, volumes and
   what they are drawn with](magnetic-field.md#cuts-volumes-and-what-they-are-drawn-with)).

## The charges

![Two wires above a perfectly conducting ground and their images below it; d_ij between the wires and D'_ij from one wire to the other's image give the potential coefficients; q = P^-1 V; the field at a point from the charges and the images](../images/theory/e-images.svg)

The charges come from the cross-section where they are (owner,
2026-10-31; the [line parameters](line-parameters.md)' cross-section,
each bundle as its equivalent wire):

\\[ \underline q = P^{-1}\,\underline V \\]

- \\( P \\): the potential coefficients with images in a perfectly
  conducting earth, \\( p_{ij} = \ln(D'_{ij}/d_{ij}) / (2\pi\varepsilon_0) \\);
  a wire's own \\( p_{ii} = \ln(2h_i/r_i) / (2\pi\varepsilon_0) \\) with
  its height \\( h_i \\) and its (bundle's) equivalent radius \\( r_i \\).
- \\( \underline V \\): the voltages to earth. Each phase is its
  circuit's voltage between phases over \\( \sqrt 3 \\), at its phase
  angle (L1 0°, L2 −120°, L3 +120° unless the system gives others). Every
  earth wire is at zero: it takes the charge that keeps it there.
- Each frequency is solved apart, the phases of the others at zero.
- **The voltage** per circuit is yours to give. A circuit without one
  takes its **system's nominal voltage, said**: an ordinance may ask
  the highest operating voltage (the NISV guide does: give it). A circuit
  given 0 kV is voltage-free: its wires at earth potential, carrying the
  charges that keep them there.

**Where along the span.** In 2D the profile's or point's own
cross-section. In 3D each span is cut into **stretches** along its
length — **24 by default**, a setting (*Stretches per span*, 4 Model;
2026-11-01, 16 until then) — each at the charges of the cross-section at
its middle: a wire hanging lower in the middle of a span carries more
charge than near its ends.

![A span between two structures in 16 stretches; each stretch at the charge per metre of the cross-section at its middle; the bars below larger in the middle of the span, where the wire hangs lower](../images/theory/e-stretches.svg)

- **Simplification**: each cross-section's charges from parallel wires at
  their heights over level ground (as the line parameters take them).
  Neither conservative nor not.
- **Simplification**: in 3D the charge is constant within each of a
  span's stretches. The charge changes by a few per cent over a span,
  well under 1 % within a stretch. Neither conservative nor not.

**How many stretches** (measured 2026-11-01 on the example double
circuit, `crates/ohl-electrical/tests/sensitivity.rs`: the largest value
of a 3D profile against 64 stretches; the time of a run unchanged to the
millisecond):

| Stretches | At mid-span | 5 % into the span (near a structure) | A quarter into it |
|---|---|---|---|
| 1 (the span's mean) | +0.31 % | +7.0 % | +3.0 % |
| 4 | −0.56 % | +1.2 % | −0.22 % |
| 8 | −0.10 % | +0.16 % | −0.03 % |
| 16 (until 2026-11-01) | −0.013 % | +0.034 % | −0.003 % |
| **24 (default)** | −0.002 % | +0.004 % | −0.002 % |
| 32 | 0.000 % | +0.009 % | +0.001 % |

Near a structure, where the wire rises steeply, the charge changes
fastest: that is where too few stretches show. From 16 on the result no
longer moves by more than a few hundredths of a per cent; 24 leaves room
for longer spans and steeper wires than the example's.
- **Corrected 2026-10-31**: until then the charges came from the span's
  mean cross-section (the wires at their mean height). Against hvlbuzz
  that read the field 3.6 % low under the line at mid-span — **not
  conservative**, replaced. The table of charges per wire and the
  jumpers still take the span's mean.
- **Assumption**: a bundle's charge shared equally by its subconductors
  (exact for a regular bundle by symmetry, close for others).

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
evaluated numerically (Simpson, \\( 10^{-8} \\)).

- **The pieces** are the solver's (some 64 per span), or shorter on
  request (*Wire pieces: at most*). Each piece's field is exact; the
  only error is how far the wire curves away from its pieces (the
  sagitta, millimetres; it falls with the square of the piece's length).
  The field moves by about that share of the distance (0.04 % at 10 m).
  Neither conservative nor not. The same pieces and the same sum as for
  the [magnetic field](magnetic-field.md#the-3d-model) (each vertex's
  distance once, blocks of 64 points, all cores).
- **The jumpers** (on by default): a jumper at a section's end carries
  the charge per metre of that section's end span (its mean
  cross-section). A jumper not at a section's end is left out, said. Off:
  all are left out — **not conservative** near the structures.

## The ground

The ground is a perfect conductor at zero potential: at power frequency
the earth's conductivity is high enough for the electric field. Two
models; the **tiled floor is the default** (owner, 2026-10-29: "keep the
tiled implementation").

![Left: in a hollow, the plane fitted under the point and the charge's image in it; the hollow's sides are not seen. Right: the tiled floor follows the terrain and carries induced charges](../images/theory/e-ground-models.svg)

### The tiled floor (default)

![Left, in plan: fine tiles near the profile, coarse tiles beyond, to 200 m every way. Right, in elevation: the tiles on the terrain with induced charges, a structure's steel cut into pieces of at most 3 m, all at zero potential](../images/theory/e-floor.svg)

Around each profile, each group of given points near one another along a
span, each cut and each volume, the terrain is laid out as a floor of
tiles:

- **The tiles.** Each tile is a quadrilateral on the terrain, carrying a
  uniform charge per square metre. Around a profile: square tiles of the
  fine size (2 m) within 30 m along the line and 10 m beyond the
  profile's width across; coarse tiles (8 m) beyond; to 200 m every way.
  A group of points shares one floor around its middle, its fine tiles
  reaching the farthest point. A cut's floor lies in the cut's own frame
  (centred on its plane, its tiles along and across it) with the cut
  tiles (8 m). A volume's floor runs under its whole run of spans and
  its margin beyond every way (100 m by default, a parameter since
  2026-11-01), square tiles of the volume tile size (8 m).
- **The solution.** The tiles' charges are such that the floor is at
  zero potential at every tile's centre under the line's charges:
  \\( A\,\sigma = -\varphi_{line} \\), a dense system solved by LU on all
  cores (faer). There are no images: the floor's charges are the
  ground's answer to the line.
- **The field.** The line's charges plus every tile's. A tile far from
  the point counts as one point charge; nearer, as 6 × 6; very near, as
  points closer together than its distance to the tile (up to 96 per
  side), so the field stays right next to coarse tiles.
- **Time.** About a second per floor (2026-10-31: 0.4 s for the LU at
  4 660 tiles, against 8.5 s on one core before, the same values to
  \\( 10^{-14} \\); filling the table and the line's potential at the
  tiles some 0.8 s). The LU grows with the tiles' cube, the rest with
  their square. Two floors are solved side by side; on a real line 4
  spans' profiles took 4.5 s.
- **Memory** (owner, 2026-10-31: laptops of 16 to 32 GB). A floor takes
  16 bytes per pair of tiles while it is solved (its table and its
  factors): 0.35 GB at 4 660 tiles. As many floors are solved side by
  side as 1 GB holds (at most 2). A floor over **2 GB** (some 11 000
  tiles) is refused, said, rather than slowing the computer by swapping.
  A volume's floor grows with its run: 2 850 tiles for one 300 m span,
  5 360 for three (0.45 GB); the window says the longest run's before
  Run.
- **A volume in parts** (owner, 2026-11-01: "split only when it doesn't
  fit, margin by distance"). A volume whose one floor would take more
  than 2 GB is no longer refused: its run is cut into equal parts about
  as long as the margin, and each part gets a floor of its own — square
  tiles on a plan grid along the part, every tile whose centre lies
  within the volume's width plus the margin of the part's line in plan
  (so a line angle's other leg and a line close by are under it, and no
  tiles fold over each other on the inner side of an angle). Each point
  of the volume takes the floor of the part it lies in (its nearest
  place on the run's line). Parts as long as the margin take the least
  time (the floors' time grows with the cube of their tiles): the
  owner's 3.5 km run of 17 936 tiles (5.1 GB, refused before) becomes
  some 36 floors of under 2 500 tiles (under 0.1 GB each).
  **An approximation, neither conservative nor not**: a part's floor
  leaves out the ground's charges beyond its margin, as one floor
  leaves out those beyond the run's ends; where two parts meet, the
  surface may step slightly. Measured on level ground against the exact
  field (the example's two spans, 600 m, from 1 m above the ground up,
  the largest difference over the largest value): one floor 3.1 %, six
  parts with 100 m 2.8 %, three with 200 m 3.4 % (8 m tiles) — the
  tiles' size makes the difference, not the parts. A run that fits in
  one floor keeps it, as before.
- **Why coarser tiles for cuts and volumes** (owner, 2026-10-29, each a
  parameter). A volume's tiles, measured again 2026-11-01 (the example's
  first span, level ground where the plane is exact, from 1 m above the
  ground up, the largest difference over the largest value;
  `crates/ohl-electrical/tests/sensitivity.rs`):

  ![A long volume along its run in four parts, two parts sharing the plane where they meet; part 2's floor over the ground within the margin m of it](../images/theory/field-parts.svg)

  | Tiles | The span, 100 m beyond | One part of 60 m, 50 m beyond |
  |---|---|---|
  | 16 m | 9.8 % (460 tiles, 0.01 s) | 9.7 % (94 tiles) |
  | **8 m (default)** | 2.9 % (1 832 tiles, 0.06 s) | 2.7 % (374 tiles) |
  | 4 m | 0.47 % (7 328 tiles, 1.2 s) | 1.2 % (1 498 tiles, 0.04 s) |
  | 2 m | — (14 352 tiles: 3.3 GB, over the 2 GB a floor may take) | 1.1 % (5 952 tiles, 0.7 s) |

  Read: halving the tiles takes the error down by three or four while
  the margin is wide; with a narrow margin the floor's end becomes the
  larger error (the 50 m column stops near 1 % from 4 m on). Finer tiles
  are worth their time only with the margin (*reaching*, 4 Model) wide
  enough: 4 m tiles and 100 m for some 0.5 %; 2 m tiles are possible
  only in parts and gain little over 4 m. The default 8 m is about
  3 % off, quick on any line.
- **Its own errors**: the tiles' size and the floor's end at 200 m. On
  evenly sloping ground, where the plane is exact, within 0.2 % of the
  profile's largest value. Neither conservative nor not. Measured again
  2026-11-01 (the example, level ground, every point of a profile and of
  a cut against the exact field, over the largest value): a **profile**'s
  fine tiles 4 m 1.1 % (0.16 s), **2 m (default) 0.19 %** (0.40 s) — 1 m
  would take 11 572 tiles, over the 2 GB a floor may take; a **cut**'s
  tiles 8 m (default) 0.78 % (0.13 s), 4 m 0.79 %, 2 m 0.79 % (0.47 s): a
  cut's floor has converged at 8 m (its largest values are near the
  wires, where the floor counts least), finer tiles only take longer.
  The defaults kept on that ground (2026-11-01).
- **The ground's elevation** — *Ground from* (2 Scope; owner, 2026-11-01:
  "choose which TIN layer it uses and if none exist which survey data to
  use to make it"): the **surface of the checks** (Line › Terrain; the
  default), any **terrain surface** (TIN layer) of the project, or the
  **survey's ground points** — the point cloud's classes with the role
  *ground* within a distance of the spans (250 m by default), the lowest
  per metre, made into a surface for the run (said in the notes). The
  tiles take their corners' elevations from it; the profiles their
  height above the ground; the cuts and volumes their bottom. Without
  any, straight between the structures' feet and level across
  (**Simplification**).

### The structures' steel (optional)

Off by default (owner, 2026-10-31); only on the tiled floor in 3D.

- Each structure's members — its type's at detail levels 2 and 3, else
  its own stick drawing; a pole's centreline — are cut into **pieces of
  at most 3 m**, each with its own unknown charge per metre. A structure
  with neither (detail level 1) is left out, said.
- The pieces are solved **together with the floor's tiles**, so that the
  steel too is at zero potential: held on its surface at each piece's
  middle. Its radius is the farthest its section reaches from the axis,
  else 5 cm (a thin wire). Only the steel standing on a floor takes part.
- The field adds each piece's closed form, as for the wires.
- **Simplification**: the line's charges are taken as without the steel
  (they follow from the voltages and the wires; the steel changes them
  little, except near the clamps).
- **Tested**: a long earthed bar beside a charged wire over flat ground
  lowers the field 1 m above the ground as the exact 2D result (the
  bar's charge \\( q_2 = -\frac{p_{21}}{p_{22}} q_1 \\)) within 0.4 % of
  the field.
- **Left out** (the default), the field near the ground beside a
  structure reads high — **conservative**; at the steel itself, low —
  **not conservative**. On a real line with stick drawings (about 6
  members per structure) it changed a profile 12 m from a mast by
  0.1 %; with lattice members, more.
- Not with the plane under each point (the steel would be solved again
  at every point) nor in 2D (said).

### The plane under each point (quick)

Under each point of evaluation the program fits a **plane to the
terrain**: its elevation there and its slopes from the elevations
`ground_radius` (10 m) either side in x and in y. It takes the images of
all charges in that plane, with the opposite sign. The images' field at
\\( p \\) is the charges' field at \\( p \\)'s mirror point, mirrored
back (a reflection keeps every distance): one more sum, no system to
solve.

Why it was built (owner, 2026-10-29, asked "what's easier and actually
better"): it is continuous along the line (no seam at a structure, as a
plane per span would leave), exact for level or evenly sloping ground,
and as costly as one more sum. **Simplification on uneven ground**: in a
hollow or on a ridge the real ground is not the plane. The comparison
below made the tiled floor the default.

### The plane against the tiled floor (2026-10-29)

The owner asked for "a comparison with a tiled floor … in a valley area
as well as flatland" on a few extracts of a real line. The reference: the
tiled floor above (2 m tiles), about 10 s per profile at the time.

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
and overestimates in hollows (conservative), and differs locally by
5–30 % of the largest value (more where the field is weak: a bump of
the ground raises the field on it, a hollow lowers it, which a plane
cannot follow). Each profile says how far the terrain departs from the
plane under its largest value (`ground_departure`). With the plane, where
it is metres, add a margin or use the tiled floor (the default).

## The 2D model

A cross-section's wires as infinite line charges with their images in
level ground at the cross-section:

\\[ \vec E = \frac{q}{2\pi\varepsilon_0}\left(\frac{\vec r}{r^2} - \frac{\vec r'}{r'^2}\right) \\]

with \\( \vec r \\) from the wire to the point and \\( \vec r' \\) from its
image. The charges are the cross-section's own (above). Quick, and
exactly the textbook case — but it assumes every span infinitely long,
straight and over level ground: the sag along the span, line angles,
jumpers, the terrain and the steel are not seen. Under the lowest point
of a long level span it is close to 3D (see Validation). Cuts and volumes
are 3D only.

Tested: one wire over level ground, the field at the ground below it
\\( q/(\pi\varepsilon_0 h) \\), in 2D and from a long 3D wire with its
image; a wire over a tilted plane gives the field of the level case
turned with it.

## Where: profiles, cuts and volumes

As for the [magnetic field](magnetic-field.md#where) (2026-11-01): a run
makes **2D** — the cuts asked and, asked, the profiles and points — or
**3D** — the volumes only, each kind a step of its own (the cuts and
volumes from the charges in 3D, the profiles by their model). The wires
come from the window's weather case and calculation, or a system's own
(*Per electrical system*: e.g. the conductors at their load's
temperature, the earth wires at the air's); each case and calculation
solved once.

- **Profiles** are square to the span's axis, at the span's lowest point
  by default.
- **Cuts** are vertical planes at a **station of an alignment**,
  perpendicular to it; at an angle point along the bisector, as a
  structure stands (owner, 2026-10-31: "cuts are made on the alignment
  not the circuits or span"). The same plane as the cross-section view
  opened at that station (one implementation). A span on no alignment
  can have no cut (said).

![In plan: an alignment with an angle point and the wires along it; a cut at a station perpendicular to the alignment, a cut at the angle point along the bisector, a profile square to its span's axis](../images/theory/field-cut-plane.svg)

- **Volumes** are lattices in a box over a run of ticked spans that
  follow each other (one volume, no seam at the structures).

**The conductor surface gradient** (the field at a subconductor's
surface, for the corona noise; 2026-11-04, 2D only) is a calculation of
its own, a charge simulation round every subconductor: [Conductor surface
gradient](surface-gradient.md). The charges above (one per bundle) give
the field away from the wires, not at their surface.

## The choices and why

| Choice | Why | Effect on the result |
|---|---|---|
| Charges from the cross-section where they are (24 stretches per span in 3D, a setting) | the charge follows the sag; the span's mean read 3.6 % low at mid-span | neither; replaced a **not conservative** choice |
| The tiled floor by default | it follows hollows and ridges; the plane read up to 8 % low on bumps | neither (its error: the tiles, under 3 % at 8 m) |
| Fine tiles 2 m at profiles, 8 m under cuts and volumes | profiles are where limits are checked; cuts and volumes cover far more ground | cuts and volumes within some 3 % of the largest value on even ground |
| A floor over 2 GB refused | laptops of 16 to 32 GB; swapping would stall the computer | none (a run is refused, said) |
| The structures' steel off by default | it needs members (detail level 2 or 3) and time; it lowers the field beside the structures | **conservative** beside a structure, **not conservative** at the steel |
| The nominal voltage when none is given | a value is needed; said in the notes | **not conservative** where the highest operating voltage is asked: give it |
| The jumpers on by default | they carry the phase's charge near the structures | off: **not conservative** near the structures |
| Trees, buildings, people not modelled | not in the model | **conservative** away from them, **not conservative** at their tops |
| DC systems left out | a static field with space charge needs another method | **not conservative** near a DC system (said) |

## Validation

Tests in the crate: the closed forms above; on the synthetic double
circuit the symmetry of a profile, the field proportional to the voltage,
3D against 2D at mid-span (within 10 %), a cut and a volume with their
isolines' and surfaces' distance off their levels; the earthed bar
(0.4 %); the V-valley (above).

**Against hvlbuzz** (hvlbuzz — *HVLBuzz 2.0* by Michael Kefeder, Gerhard Bräunlich and Joel Winiger (the authors its `Cargo.toml` names), © FKH, Switzerland ([fkh.ch](https://fkh.ch)); source [gitlab.com/fkh-ch/emc](https://gitlab.com/fkh-ch/emc); documentation [fkh-ch.gitlab.io/emc/hvlbuzz/index.html](https://fkh-ch.gitlab.io/emc/hvlbuzz/index.html); GNU General Public License version 3; an independent, open-source
program, built from its repository on 2026-10-29 — thanks to its authors
and FKH, see [Conductor surface gradient](surface-gradient.md#validation)): the 2D
charges and field on three cross-sections, 1 m above the ground — within
0.12 % on 380 kV bundles (one charge per bundle at its centre against
hvlbuzz's twelve round every subconductor), 0.003 % on single wires, with
and without an earth wire
([validation](../validation/textbook.md#the-2d-fields-against-hvlbuzz-benchmarkselectricalhvlbuzztoml)).

And the line's own profile (2026-10-31): the example double circuit at
the middle of T1–T2, its 2D profile against hvlbuzz on the same
cross-section within 0.2 % at every point (tested; from the span's mean
cross-section it was 3.6 % low at the axis); the 3D profile's largest
value 0.6 % above hvlbuzz's on the plane under each point, 0.8 % on the
tiled floor. The 2D and 3D methods differ (the wires curve up away from
the middle, the next span adds its field), by up to 2–3 % within ±15 m of
the axis and more where the field is smallest.

**Not validated yet** against a published profile (to find: IEC 62110's
examples, CIGRE's brochures on fields of overhead lines, the NISV guide's
examples).

## What is left out

- **Trees, buildings, fences and people.** Earthed objects shield the
  field: the field computed is **conservative** away from them and **not
  conservative** at their tops, where it concentrates. A building drawn
  in a cut or a cross-section view is shown, not modelled.
- **The structures' steel** unless asked (above).
- **The earth's resistivity**: the ground is a perfect conductor.
- **Space charge and corona**; DC systems.
- **The charges' change by the steel** (the line's charges as without
  it).
- **Other lines** (crossings, parallel lines): not in the model yet.

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
