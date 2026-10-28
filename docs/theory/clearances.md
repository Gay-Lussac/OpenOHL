# Vertical and radial clearances

*Built, in active development (not approved yet).* The Vertical and
Radial clearances windows of OHL-Design and `ohl zones`
(`crates/ohl-clearance`: `zone`, `zone_arc`, `zone_run`, `checks`). How to
use them: [OHL-Design › Vertical and radial
clearances](../user-guide/desktop-app.md#vertical-and-radial-clearances)
and [`ohl zones`](../user-guide/terrain.md#vertical-and-radial-clearances-without-the-window-ohl-zones).
Every clearance is to the conductor's surface ([Clearance](../user-guide/terrain.md#clearance)).

## The two measures

- **Vertical**: a target (a LiDAR point, the ground) within the zone's
  half width \\( w \\) of the wire in plan is compared with the wire's
  lowest point over the slice of depth \\( d \\) along the line around it
  (its highest for a target above): the height difference. A wire piece
  rising by \\( m \\) per metre is taken \\( m\,d/2 \\) lower over the
  slice (exact for the straight piece).
- **Radial**: the shortest 3D distance from the wire (each subconductor
  of a bundle).

![Across the line: a target within the zone's half width w measured straight up to the wire, one farther out left out of the vertical; along the line: the wire taken m d/2 lower over the slice](../images/theory/vertical-radial.svg)

*The two measures: the vertical within the zone's width, measured straight up; the radial, the shortest distance (schematic).*

### The vertical: straight below, drawn vertical (2026-10-22)

The owner's decision of 2026-10-22: a vertical rule checks only the
targets straight under (over) the wire, within the zone's width, and
measures and draws only vertically. As built (checked against the code,
`checks::PointCheck`, `checks::ground_check`, `zone_arc::ArcPointCheck`;
test `the_vertical_checks_only_the_band_and_is_drawn_vertical`):

- **Which targets.** A LiDAR point counts when its plan position lies
  within \\( w \\) of the wire's polyline in plan (the nearest point of a
  piece in plan, `geometry::over_in_plan`); the ground: the highest ground
  within \\( w \\) across each piece (its crossings under the piece and
  under its two edges, and the TIN's vertices between). A target farther
  than \\( w \\) in plan is **left out entirely**: no sloped or 3D
  distance is taken for it. Rule 10 m, \\( w = 1 \\) m: a point 0.9 m
  beside the wire is checked, one 1.1 m beside it is not, however near it
  is in 3D.
- **The distance.** The wire's height over the target at the target's
  plan position (the wire interpolated along the piece at the point
  nearest in plan): \\( z_{\text{wire}} - z_{\text{target}} \\) for a
  target below (to the lowest subconductor of a bundle, less the
  conductor's radius), above: the reverse. On a piece sloping at
  \\( \beta \\) both are measured **vertically** (2026-10-26, physics
  review): a vertical meets the round surface \\( r/\cos\beta \\) from the
  axis, and a subconductor \\( d \\) below the axis (square to it) lies
  \\( d/\cos\beta \\) below it at the same plan position — at most the
  offset plus the piece's own rise (a vertical through a steep piece
  leaves it through its end; `checks::vertical_of`). Before, \\( r \\) and
  \\( d \\) were taken off as on a level wire, \\( r(1/\cos\beta - 1) \\)
  too little: 4.6 mm for an iced radius of 30 mm on 30°, 39 mm for a
  bundle's 0.25 m. The ground, the points and the crossings of two wires
  all measure so; a level piece is unchanged, bit for bit. Test:
  `the_vertical_takes_the_radius_off_along_the_vertical`. With a depth \\( d \\) the
  wire is taken \\( m\,d/2 \\) lower (the wire's lowest point over the
  slice, above); a depth of 0 gives exactly the height over the point.
- **With the swing arc** the width is taken across the **swung** wire's
  plan position: the band's points within \\( w \\) across the line of
  the target (and within \\( d/2 \\) along it) count, each position of the
  band where it is in plan, not the still-air wire's; the distance is
  vertical as well — the height of the band's lowest such point over the
  target ([The vertical distance to the arc](#the-vertical-distance-to-the-arc)).
- **The drawing.** Every vertical violation, and the lowest reported, is
  drawn as a **vertical** segment from the target straight up (down) to
  the wire's height at that plan position — the same \\( x, y \\) at both
  ends, also for a target at the band's edge (`checks::Hit::drawn`,
  `zone::ZoneFinding::drawn`): in the 3D view, the cross-section panel,
  the plan (the mark at the target's plan position) and the profile. The
  value listed is the vertical distance. Radial rules keep their radial
  segment, from the wire point found to the target.

What differed before: the measure was already vertical and the band
already in plan (the numbers do not change); only the drawing ran from
the target to the wire point found — beside the target by up to \\( w \\)
— so a point at the band's edge was drawn slanting (1 m sideways on a
default zone), and the profile drew no clearance marks. The reports'
CSV keep the wire point found (its coordinates, the swing arc's angle).

Each rule gives the wires' voltage, its weather cases, the distance to
keep (one, by voltage, or base + per kV) and its targets; each span's
lowest clearance over every wind position of the rule's cases is
reported, with the position.

**Which ground** (since 2026-10-23). A ground rule checks the surface of
the checks (one for every user of the project), or the terrain surface
(TIN layer) of the project it names (`surface = "name"` in the rule
file, `Rule::surface`); the checks get the named surfaces by name
(`ZoneTargets::surfaces`, `CaseTargets::surfaces`) and measure to each
exactly as to the surface of the checks. A name the project does not
hold is noted and its rule is not checked: never passed silently. The
thermal limit refuses to run instead ([Thermal
rating](thermal.md#maximum-operating-temperature-from-clearance)).

## Wind positions

A weather case with wind is solved from each side chosen (left, right or
both, looking ahead along the line) at its full pressure. Two options,
alone or together:

- **Wind steps**: still air and every pressure \\( p_k = k\,\Delta p \\)
  up to the case's (at most 200 steps a side, the step made larger
  beyond), each solved by the line's solver: the wires' own positions,
  strings and clamps where they really swing, the sag of each pressure.
  Exact at the pressures solved, blind between two of them.
- **Swing arc** (2026-10-20, the danger-tree check's arc,
  [LiDAR analyses › Wire positions checked](lidar-analysis.md#wire-positions-checked),
  the same code `positions::SwingArc`): each span swung as one about
  its attachment axis (the line through the string tops, at a dead end
  the clamps) between still air and the full blowouts of the sides
  chosen — since 2026-10-21 a **band**: every radius from the smallest
  to the largest of theirs, \\( r = \min(R_L, R_0, R_R) \\) to
  \\( R = \max(R_L, R_0, R_R) \\), at each place along the span, from the
  smallest to the largest of their angles. Three solves per windy case
  (one side: two). The arc's own labels hold here too:

  **Conservative:** every radius between the smallest and the largest of
  the three along the whole arc, for targets on every side — below and
  beside the wire as well as between the axis and the arc (above the
  swinging wire). Until 2026-10-21 the largest radius only: a target
  between the axis and the arc could be nearer a wire at an intermediate
  pressure, which sags less, than to the arc — by 0.47 m on the example
  line (below).

  **Conservative:** the band widened by its margin \\( m \\) on every
  side, 0.15 m by default (the measured worst excess of the 10 Pa steps
  over the band, 0.089 m, plus an allowance of 0.061 m:
  [the swing arc](lidar-analysis.md#wire-positions-checked)). Since
  2026-10-21 the margin is a parameter (`[wind] band_margin`, the
  windows' **band margin**, `ohl zones --band-margin`): a smaller value
  is less safe and on the user's responsibility.

  **Jumpers** (2026-10-21): the jumpers a rule checks (`wires` jumpers or
  both) have a band of their own, about the line through their two end
  clamps (blown to the left, the first solve), from the smallest to the
  largest radius of their three positions (`positions::jumper_band`),
  widened by a margin of **their own** \\( m_j \\) (the owner's decision
  of 2026-10-21: `[wind] jumper_band_margin`, the windows' **jumper band
  margin**, `ohl zones --jumper-band-margin`), **0 m by default**. Before,
  they were checked at their solved positions only.

  **Simplification** (on the user's responsibility; exact: the wind
  steps): with \\( m_j = 0 \\) the jumper's band is **not conservative
  between its blowouts** — on the example line its 10 Pa steps lie up to
  0.293 m outside it ([the swing arc](lidar-analysis.md#wire-positions-checked)).
  The value that holds a jumper is the user's to enter for their jumpers
  (the wind steps show it).

  **Conservative:** the arc runs from the smallest to the largest of
  \\( \varphi_L, \varphi_0, \varphi_R \\).

  **Simplification:** a rigid swing about the axis through the
  attachments. Suspension strings swing with the wind and their clamps
  move along and across the line; the sag in the plane of the load
  changes with the pressure. The true positions lie near the arc, not on
  it. Exact: a solve at each pressure (the steps, at the pressures they
  solve).

### The radial distance to the arc

As for the danger trees: the point \\( q \\) projected on the axis at
\\( s_q \\), at radius \\( \rho_q \\) and angle \\( \varphi_q \\) in the
plane square to the axis there: within the band's angles the radial gap
— 0 for \\( r \le \rho_q \le R \\), \\( r - \rho_q \\) inside it,
\\( \rho_q - R \\) outside it — (with the distance along the axis beyond
the span's ends), else the distance to the nearer end edge (the radial
segment from \\( r \\) to \\( R \\) at \\( \varphi_L \\) or
\\( \varphi_R \\)). The clearance takes off the bundle radius, the largest
allowance of the three wires (**Conservative:** the bundle's outer
envelope) and the band's margin.

### The vertical distance to the arc

Definition: the vertical clearance of \\( q \\) to the band is the
smallest vertical clearance of \\( q \\) to any wire in it — **the
height of the band's lowest point over \\( q \\)'s plan position**, among
the band's points within the zone's half width \\( w \\) across the line
(and, with a depth, within \\( d/2 \\) along it); for a point above the
band, its height above the band's highest such point; 0 where \\( q \\)
lies between the two (a wire passes through it at some angle and
radius).

At a place \\( s \\) along the axis (unit vector \\( \mathbf u \\), its
horizontal part \\( u_h = |\mathbf u_{xy}| \\), its rise \\( u_z \\)), the
arc's point at angle \\( \varphi \\) is
\\( C(s) + R(s)(\cos\varphi\,\mathbf d + \sin\varphi\,\mathbf r) \\), with
\\( \mathbf d \\) the downward vertical made square to the axis
(\\( u_z\,\hat{\mathbf h} - u_h\,\hat{\mathbf z} \\)) and \\( \mathbf r \\)
horizontal, to the right. So it lies
\\( R\cos\varphi\,u_z \\) further along the line in plan than the
centre, \\( R\sin\varphi \\) to the right, and
\\( R\cos\varphi\,u_h \\) below the centre. Over the plan position
\\( (h, y) \\) of \\( q \\) (along the line from \\( A_1 \\), to the right
of the axis):

1. the band's points within the zone's width: the point at radius
   \\( \rho \\) and angle \\( \varphi \\) is \\( \rho\sin\varphi \\) to the
   right and \\( x = \rho\cos\varphi \\) "down" (in the plane square to
   the axis), so the admissible set is the annular sector
   \\( r \le \rho \le R \\), \\( \varphi_L \le \varphi \le \varphi_R \\) cut
   to the strip \\( |\rho\sin\varphi - y| \le w \\);
2. its lowest point is where \\( x \\) is largest (the highest: where it
   is smallest). \\( x \\) is linear in that plane, so its extremes lie on
   the set's boundary: on the circles of radius \\( R \\) and \\( r \\)
   (the ends of their admissible angles,
   \\( \sin\varphi \in [(y-w)/\rho,\ (y+w)/\rho] \\) within
   \\( [\varphi_L, \varphi_R] \\), and \\( \varphi = 0 \\)) or on the two end
   edges where the strip cuts them (\\( \rho = (y \pm w)/\sin\varphi_{L,R} \\)
   within \\( [r, R] \\)) — a handful of candidates, the extreme taken;
3. the place over \\( h \\): \\( s = (h - x(s)\,u_z)/u_h \\),
   solved by fixed-point steps from \\( s = h/u_h \\) (each a contraction
   by about \\( |dR/ds|\,u_z/u_h \\), far below 1 on a span), to
   \\( 10^{-6} \\) m. A point beyond the span's ends, or with no angle
   within the band, is not in the zone.

With a depth, the heights \\( f_-, f_0, f_+ \\) of the lowest point at
\\( h - d/2, h, h + d/2 \\) give the bound
\\( f_0 - \max(|f_0 - f_-|, |f_+ - f_0|) \\) (and the ends themselves) for
the lowest point over the slice. **Simplification:** this bound is exact
where the lowest point's height is convex along the line, as a hanging
wire's (and for a straight piece); a wire kinked by a point load between
two places 0.1 m apart could dip below it by millimetres.

**Simplification:** in the swing band the bundle radius and the
allowance are taken off as on a level wire (radially), not
\\( r/\cos\beta \\) as the still-air vertical does (above): on a span
sloping at \\( \beta \\) the band's vertical is \\( r(1/\cos\beta - 1) \\)
too large — unsafe, by millimetres for a conductor's radius (4.6 mm for
30 mm on 30°), more for a bundle. The radial distance, which never
exceeds the true vertical one, is not affected. Exact: the radius
measured vertically, as in still air.

**Check by hand** (`benchmarks/clearance/danger-trees.toml`,
`arcclear-001…006`): a level 100 m span 20 m up, sag 4 m in still air and
5 m blown ±40°; at mid-span the band runs from \\( r = 4 \\) to
\\( R = 5 \\) m. A point 4 m to the right at
\\( z = 10 \\) m, \\( w = 1 \\) m: \\( \sin\varphi \in [0.6, 1] \\), so
\\( \varphi \in [36.87°, 40°] \\); the lowest point at 36.87°
(\\( \cos = 0.8 \\)) is at \\( 20 - 4 = 16 \\) m: vertical 6 m (radially
\\( \sqrt{4^2 + 10^2} - 5 = 5.770 \\) m). The span inclined by 30 m: at
mid-axis the arc's lowest point is \\( 5 \times 0.2873 = 1.437 \\) m
further along in plan than the centre and \\( 5 \times 0.9578 = 4.789 \\)
m below it, at \\( z = 30.211 \\) m over \\( x = 51.437 \\) m: a point
there at 25 m is 5.211 m below — not the 4.794 m the point's own place
along the axis would give. A point 2 m below the axis at mid-span
(\\( z = 18 \\) m) lies above the band: its highest point within 1 m
across is on the inner circle at \\( \sin\varphi = 1/4 \\) (the 40° end
edges cross the strip only at \\( \rho = 1/\sin 40° = 1.556 \\) m, inside
\\( r \\)), at \\( 20 - 4\cos 14.48° = 16.127 \\) m: vertically 1.873 m,
radially \\( r - 2 = 2.0 \\) m (the arc alone at \\( R \\): 2.899 m and
3.0 m). A point inside the band (\\( z = 15.5 \\) m): 0 both ways.

![The check by hand at mid-span: the band from 4 m to 5 m, a point 4 m right at 10 m with the band's lowest point in its strip 6 m above it, and a point 2 m below the axis 1.873 m above the band](../images/theory/vertical-arc.svg)

*The vertical distance to the band in the check by hand, to scale.*

### The ground under the arc

The ground is a TIN: the vertical and radial clearances of the band to
it are those of the lines over its boundary — the wire swung to angles
every \\( \Delta\varphi = 2e/R_{\max} \\) (\\( e = \\) 2 cm,
`zone::ARC_TOLERANCE`) at the band's outer radius and again at its
inner radius, and the two end edges at fractions of the thickness every
\\( \Delta\rho \le 2e \\) — each a polyline through the band's points at
every station, checked as a solved wire (exact for its polyline). Every
point of the band's boundary lies within \\( e \\) of one of them at its
station, and a ground that reaches into the band crosses its boundary,
so the boundary's clearances are the band's.

**Conservative:** each such wire is taken \\( e \\) thicker (its
clearances 2 cm smaller), the zone \\( e \\) wider on each side (and
\\( 2e \\) deeper), and vertically each piece \\( e\,s \\) lower
(\\( s \\) the steeper of its rise per plan length and its
neighbours': a point of the arc \\( e \\) away from the wire's point
lies over ground the piece passes over up to \\( e\,s \\) higher or
lower), so that no point of the band between two of them can be nearer
the ground than reported. Exact: the distance between the swept surface
and the TIN (no closed form).

The outer surface's angles nearest the hanging position are checked
first, then the inner surface and the edges; a piece of
the next angles that cannot come below the lowest clearance found so
far, nor below the required clearance, is passed over (its bound from the
highest ground near it, as below): the counts and the lowest clearance
are those of every angle checked in full.

## Wire to wire on the swing arc (2026-10-28)

The Clearances window (wire clearances) checks the line's own wires
against each other on the swing arc too (owner, 2026-10-28;
`ohl_clearance::wire_arc`). Two wires do not swing independently: one
wind blows on both. So the spans are swung **together**: for a windy case
of pressure \\( P \\) and a side, the pressure \\( p = f P \\),
\\( 0 \le f \le 1 \\), and at every station of a span, about its
attachment axis,

\\[ \tan\varphi(f) = \tan\varphi_0 + f\,(\tan\varphi_1 - \tan\varphi_0),
\qquad r(f) = r_0 + f\,(r_1 - r_0), \\]

from the still-air \\( (\varphi_0, r_0) \\) and the full blowout's
\\( (\varphi_1, r_1) \\) (both solved). The tangent of a rigid swing is
the wind load over the weight per metre, and the wind load grows with the
pressure: a heavy span swings less than a light one at the same
pressure, as it does.

**Sampling.** \\( d\varphi/df = \Delta / (1 + \tan^2\varphi) \le \Delta \\)
(\\( \Delta = \tan\varphi_1 - \tan\varphi_0 \\)), so no point moves more
than \\( r_\max \Delta + |r_1 - r_0| \\) per unit of \\( f \\). The swing is
sampled in \\( n \\) steps of \\( f \\) such that this over \\( n \\) is at most
0.1 m (at most 2000 steps: a longer swing coarser), and at each sample
every rule between the line's wires is checked as in any case. To keep a
long line fast, the whole line is first swung in steps of 1 m, each wire
taken 0.5 m thicker: a pair meeting every rule there meets it everywhere
on the swing (the bound below); only the pairs that may not, and per rule
those within a coarse step of its lowest, are then swung alone in the
0.1 m steps. The result is the fine pass's.

**Conservative.** Between two samples a point is never farther than half
a step from one of them: each wire is taken that much thicker. The rigid
swing is not the wire's solved position at the pressures between; each
wire is also taken thicker by its **band margin**, as for the ground
(spans: the measured worst excess of the intermediate pressures plus an
allowance; jumpers: the user's, 0 by default — a Simplification on the
user's responsibility). Both are taken off every clearance.

**Not covered.** A rule's case pairs are not swung (each case at its full
wind). Wires of other lines are not on this arc.

## Arc or steps: speed and quality

Measured on the example line (`zone_arc` test) and on an invented line
of 600 spans (180 sections, 1 million invented points; `zone_speed`,
release build, 10 cores), one windy case of 650 Pa (600 Pa on the
example):

*The example line* (9 spans, the synthetic survey, 4000 invented points
within 14 m of the wires; rules of 7 m to the ground and to the points;
the solves of this small line take a millisecond each):

| | wind steps, 50 Pa | swing arc (band, 2026-10-21) | swing arc (before) |
|---|---|---|---|
| solves | 25 | 3 | 3 |
| vertical: solved, checked | 17 ms, 13 ms | 5 ms, 159 ms | 5 ms, 103 ms |
| radial: solved, checked | 17 ms, 24 ms | 5 ms, 270 ms | 5 ms, 193 ms |

*The invented long line* (600 spans of 350 m, three phases, 180 sections,
10⁶ points; rules of 7 m to the ground and 5 m to the points; a shared
computer, ± 20 %):

| | steps, 10 Pa | steps, 50 Pa | swing arc (band, 2026-10-21) | swing arc (before) |
|---|---|---|---|---|
| solves | 131: 56 s | 27: 12 s | 3: 1.5 s | 3: 1.3 s |
| vertical, ground | 7.1 s | 2.1 s | 9.4 s | 5.9 s |
| vertical, points | 10.0 s (10 µs a point) | 2.1 s | 1.3 s (1.3 µs) | 0.9 s (0.9 µs) |
| radial, ground | 38 s | 4.4 s | 38 s | 27 s |
| radial, points | 18 s | 2.0 s | 0.6 s | 0.9 s |
| lowest vertical clearance to the ground | 13.953 m | 13.960 m | 13.085 m | 13.235 m |

The band's ground check is the slower (about 1.4 to 1.6 times the arc's
before, measured 2026-10-21 on the same shared computer, ± 20 %): it
checks the band's inner surface and its end edges besides the outer
surface. Its lowest clearance to the ground is the arc's less the
band's margin (0.150 m): the ground lies outside, where the outer
surface was already the nearest.

The arc's solves and its point checks are a few per cent of the steps';
its ground check is not: the wire swung every 2 cm of the arc is many
wires (hundreds per span), each checked against the TIN — on the small
example line the steps are the faster, on a long line with fine steps
the arc (vertically 8 s against 73 s at 10 Pa and 16 s at 50 Pa;
radially, with the ground, 29 s against 113 s and 19 s).

- **Why the arc is faster.** Three solves per windy case (still air,
  left, right) instead of one per pressure and side —
  \\( 1 + 2\,P/\Delta p \\): 25 at 50 Pa, 131 at 10 Pa for 650 Pa — and
  one closed-form distance per point and span instead of one per
  position. The solves and the per-position search both grow with the
  number of positions; the arc keeps them at three.
- **What the arc gains.** It covers every angle between the blowouts
  continuously; the steps see the wire only at the pressures they solve
  and are blind between two of them. Since 2026-10-21 it takes every
  radius between the smallest and the largest of the three positions,
  widened by its margin (**Conservative** for targets on every side, see
  above).
- **What it simplifies.** The rigid swing about the attachment axis
  (**Simplification**, above): suspension clamps move along the line and
  the sag changes with the pressure, so the true positions are near the
  arc, not on it. The steps are exact at the pressures they solve: where
  a point lies close to the wire at an intermediate pressure, the steps
  give its distance to the solved wire, the arc its distance to the rigid
  swing. Measured on the example line (`zone_arc`, every span's lowest
clearance to the ground and to 4000 invented points): with the band
(2026-10-21) the arc is never above the steps (50 Pa), vertically nor
radially (radially at least 0.189 m below them at every span; with the
default margin), and below
them by up to 1.22 m elsewhere. Before, at the largest radius only, it
was radially above them by up to 2.1 cm (S2's first span, a point 2.2 cm
from the solved wire at some pressure, 4.2 cm from the arc), and a point
placed on the wire solved at 300 Pa (S1's first span, mid-span) was
0.47 m from the arc: at 300 Pa the wire sags less than at the full
pressure, so it passes 0.47 m nearer the axis than the arc. That point
now lies inside the band (clearance 0 less the conductor and the margin).
With steps of 10 Pa from both sides on the example lines no stepped
position lies outside the band widened by its margin (at most 0.089 m
outside the band itself; the arc alone missed them by up to 1.53 m:
`tests/arc_band.rs`). Never read "no loss" into the arc: it is a
  conservative envelope with a simplification, not the steps' exact
  positions.
- **Both together** (the steps and the arc) check the steps' positions
  and the arc: the smaller of the two at every span.

## How it is made fast (2026-10-20)

Speed work coarsens nothing computed: every change below gives the
results of before **bit for bit** (`checks::reference`, the tests
`the_point_check_gives_the_results_of_before` and
`the_ground_check_gives_the_results_of_before`, on the example line in
five wind positions, with bundles, 30 000 invented points and ties on
whole metres; the earlier code is kept for them).

- **One measure.** A window checks one measure; the other is no longer
  worked out for every point and piece.
- **Bounds before the exact distance.** A wire piece is passed over
  without its exact distance when the distance to its bounding box (in
  plan within the zone's width, for the vertical) shows that it can give
  neither a smaller clearance than the smallest found nor one below the
  required clearance. Equal clearances keep the first found, as before.
- **The highest ground per cell** (`checks::GroundMax`, 5 m cells in
  tiles of 64 × 64): a piece (radially a sample every 0.5 m) whose lowest
  point is higher above the highest ground near it than the smallest
  clearance so far (and the required clearance) is passed over — for the
  radial, the ground within \\( r = \sqrt{(t + a)^2 + (s/2)^2} \\) of the
  sample (\\( t \\) the threshold, \\( a \\) the allowance, \\( s \\) the
  sample spacing): a triangle nearer in plan is under that ground, one
  farther is more than \\( r \\) away.
- **Every core.** The ground check checks the wires in parallel; the
  points are read in blocks of 65 536, each block's stretches worked out
  on every core from the results so far and the results applied in the
  points' order (the same results as one point at a time). The solves
  were already spread over the cores, one wind position each.

Measured before and after on the invented long line (10 Pa steps, 131
solves, 10⁶ points; one run after the other on the same shared
computer):

| | before | after |
|---|---|---|
| solves | 68 s | 65 s (unchanged) |
| vertical, ground | 91 s | 12 s |
| vertical, points | 103 s (10 000 points a second) | 17 s (58 000 a second) |
| radial, ground | 264 s | 44 s |
| radial, points | 101 s | 15 s |
| the lowest clearances | 13.953, 0.000, 13.975, −0.015 m | the same |

A vertical check of the ground and of 10⁶ points with 10 Pa steps: 262 s
before, 94 s after, 8 s on the swing arc. With 10⁷ points the points'
time grows tenfold (about 20 minutes before, 4 after, 16 s on the arc,
extrapolated). With fine steps on a long line the **solves** are now
the larger part: one solve of every section per pressure and side,
spread over the cores — the swing arc is the remedy.

Ideas left out: *warm-starting each pressure's solve from the previous
one* — the solver stops at its tolerance, so a different start gives a
different (equally valid) position: not the same numbers. *Solving only
the sections near candidate points* — a ground rule has the ground under
every span, and which LiDAR points are near a wire is known only after
the solve; a bound from the still and full positions would rest on the
rigid-swing simplification. *Ignoring the depth and comparing each point
with the line's points only* (the owner's suggestion for the vertical) —
the vertical clearance is the wire's height over the point's plan
position: taking the nearest polyline point instead of the piece over it
would change the numbers by up to the rise of a piece over half its
length (centimetres on a slope), and the depth is a setting of the rule;
the speed comes instead from the plan box test, which looks at exactly
the pieces within the width.
