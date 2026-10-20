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

Each rule gives the wires' voltage, its weather cases, the distance to
keep (one, by voltage, or base + per kV) and its targets; each span's
lowest clearance over every wind position of the rule's cases is
reported, with the position.

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
  chosen, at the largest of their radii \\( R = \max(R_L, R_0, R_R) \\) at
  each place along the span, from the smallest to the largest of their
  angles. Three solves per windy case (one side: two). The arc's own
  labels hold here too:

  **Conservative:** one radius, the largest of the three, along the whole
  arc (a wind between none and the full one never sags the wire further)
  — for targets outside the arc (below and beside the wire). A target
  between the axis and the arc (above the swinging wire) can be nearer a
  wire at an intermediate pressure, which sags less, than to the arc: by
  0.47 m on the example line (below).

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
plane square to the axis there: \\( |\rho_q - R| \\) within the arc's
angles (with the distance along the axis beyond the span's ends), else
the distance to the nearer end. The clearance takes off the bundle
radius and the largest allowance of the three wires (**Conservative:**
the bundle's outer envelope).

### The vertical distance to the arc

Definition: the vertical clearance of \\( q \\) to the arc is the
smallest vertical clearance of \\( q \\) to any wire on it — **the
height of the arc's lowest point over \\( q \\)'s plan position**, among
the arc's points within the zone's half width \\( w \\) across the line
(and, with a depth, within \\( d/2 \\) along it); for a point above the
arc, its height above the arc's highest such point; 0 where \\( q \\)
lies between the two (a wire passes through it at some angle).

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

1. the angles within the band: \\( |R\sin\varphi - y| \le w \\), i.e.
   \\( \sin\varphi \in [(y-w)/R,\ (y+w)/R] \\), intersected with the arc's
   \\( [\varphi_L, \varphi_R] \\);
2. among them, the lowest point is where \\( R\cos\varphi \\) is largest
   — the angle nearest the vertical (the highest: the farthest);
3. the place over \\( h \\): \\( s = (h - R(s)\cos\varphi\,u_z)/u_h \\),
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
two places 0.1 m apart could dip below it by millimetres. The bundle
radius and the allowance are taken off as radially (**Conservative**).

**Check by hand** (`benchmarks/clearance/danger-trees.toml`,
`arcclear-001…005`): a level 100 m span 20 m up, sag 4 m in still air and
5 m blown ±40°; at mid-span \\( R = 5 \\) m. A point 4 m to the right at
\\( z = 10 \\) m, \\( w = 1 \\) m: \\( \sin\varphi \in [0.6, 1] \\), so
\\( \varphi \in [36.87°, 40°] \\); the lowest point at 36.87°
(\\( \cos = 0.8 \\)) is at \\( 20 - 4 = 16 \\) m: vertical 6 m (radially
\\( \sqrt{4^2 + 10^2} - 5 = 5.770 \\) m). The span inclined by 30 m: at
mid-axis the arc's lowest point is \\( 5 \times 0.2873 = 1.437 \\) m
further along in plan than the centre and \\( 5 \times 0.9578 = 4.789 \\)
m below it, at \\( z = 30.211 \\) m over \\( x = 51.437 \\) m: a point
there at 25 m is 5.211 m below — not the 4.794 m the point's own place
along the axis would give.

### The ground under the arc

The ground is a TIN: the vertical and radial clearances of the arc to it
are those of the wire swung to angles every
\\( \Delta\varphi = 2e/R_{\max} \\) (\\( e = \\) 2 cm,
`zone::ARC_TOLERANCE`), each a polyline through the arc's points at
every station, checked as a solved wire (exact for its polyline).
Every point of the arc lies within \\( e \\) of one of them at its
station.

**Conservative:** each such wire is taken \\( e \\) thicker (its
clearances 2 cm smaller), the zone \\( e \\) wider on each side (and
\\( 2e \\) deeper), and vertically each piece \\( e\,s \\) lower
(\\( s \\) the steeper of its rise per plan length and its
neighbours': a point of the arc \\( e \\) away from the wire's point
lies over ground the piece passes over up to \\( e\,s \\) higher or
lower), so that no point of the arc between two of them can be nearer
the ground than reported. Exact: the distance between the swept surface
and the TIN (no closed form).

The angles nearest the hanging position are checked first; a piece of
the next angles that cannot come below the lowest clearance found so
far, nor below the requirement, is passed over (its bound from the
highest ground near it, as below): the counts and the lowest clearance
are those of every angle checked in full.

## Arc or steps: speed and quality

Measured on the example line (`zone_arc` test) and on an invented line
of 600 spans (180 sections, 1 million invented points; `zone_speed`,
release build, 10 cores), one windy case of 650 Pa (600 Pa on the
example):

*The example line* (9 spans, the synthetic survey, 4000 invented points
within 14 m of the wires; rules of 7 m to the ground and to the points;
the solves of this small line take a millisecond each):

| | wind steps, 50 Pa | swing arc |
|---|---|---|
| solves | 25 | 3 |
| vertical: solved, checked | 18 ms, 15 ms | 5 ms, 103 ms |
| radial: solved, checked | 17 ms, 25 ms | 5 ms, 193 ms |

*The invented long line* (600 spans of 350 m, three phases, 180 sections,
10⁶ points; rules of 7 m to the ground and 5 m to the points; a shared
computer, ± 20 %):

| | steps, 10 Pa | steps, 50 Pa | swing arc |
|---|---|---|---|
| solves | 131: 56 s | 27: 12 s | 3: 1.3 s |
| vertical, ground | 7.1 s | 2.1 s | 5.9 s |
| vertical, points | 10.0 s (10 µs a point) | 2.1 s | 0.9 s (0.9 µs) |
| radial, ground | 38 s | 4.4 s | 27 s |
| radial, points | 18 s | 2.0 s | 0.9 s |
| lowest vertical clearance to the ground | 13.953 m | 13.960 m | 13.235 m |

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
  and are blind between two of them. It takes the largest radius of the
  three positions (**Conservative** for targets outside the arc, see
  above).
- **What it simplifies.** The rigid swing about the attachment axis
  (**Simplification**, above): suspension clamps move along the line and
  the sag changes with the pressure, so the true positions are near the
  arc, not on it. The steps are exact at the pressures they solve: where
  a point lies close to the wire at an intermediate pressure, the steps
  give its distance to the solved wire, the arc its distance to the rigid
  swing. Measured on the example line (`zone_arc`, every span's lowest
clearance to the ground and to 4000 invented points): vertically the
arc is never above the steps (50 Pa); radially it is above them by at
most 2.1 cm (S2's first span, a point 2.2 cm from the solved wire at
some pressure, 4.2 cm from the arc), and below them by up to 1.07 m
elsewhere. A point placed on the wire solved at 300 Pa (S1's first span,
mid-span) is 0.47 m from the arc: at 300 Pa the wire sags less than at
the full pressure, so it passes 0.47 m nearer the axis than the arc.
**The largest radius is conservative for targets outside the arc — below
and beside the wire, the ground, trees — not for a target between the
axis and the arc** (above the swinging wire, rare for these checks). Never read "no loss" into the arc: it is a
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
  requirement. Equal clearances keep the first found, as before.
- **The highest ground per cell** (`checks::GroundMax`, 5 m cells in
  tiles of 64 × 64): a piece (radially a sample every 0.5 m) whose lowest
  point is higher above the highest ground near it than the smallest
  clearance so far (and the requirement) is passed over — for the
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
