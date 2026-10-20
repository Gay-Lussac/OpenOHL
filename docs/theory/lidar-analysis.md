# LiDAR analyses

Classification (`crates/ohl-lidar`, `classify`), vegetation and as-built wires
(`crates/ohl-clearance`, `vegetation` and `wirefit`). All three are a base to
build on: the methods are simple, published and documented with their limits.

## Classification

**Ground** — a progressive morphological filter (K. Zhang, S.-C. Chen, D.
Whitman, M.-L. Shyu, J. Yan, C. Zhang, "A progressive morphological filter
for removing nonground measurements from airborne LIDAR data", *IEEE
Transactions on Geoscience and Remote Sensing* 41(4), 2003):

1. A grid of cell size \\( c \\) (1 m) holds the lowest point of each cell.
2. The grid surface is *opened* — eroded (minimum filter) then dilated
   (maximum filter) — with square windows of \\( w_k = 2r_k + 1 \\) cells,
   \\( r_k = 1, 2, 4, 8, \dots \\) while \\( w_k c \\) is within the largest
   window (20 m). Opening removes objects smaller than the window.
3. A cell whose lowest point stands above the opened surface by more than
   \\( \Delta h_k = s\\,(w_k - w_{k-1})\\,c + \Delta h_0 \\) (at most
   \\( \Delta h_{max} \\); \\( \Delta h_0 \\) for the first window; slope \\( s \\) = 0.3, \\( \Delta h_0 \\) = 0.3 m,
   \\( \Delta h_{max} \\) = 3 m) holds no ground; the opened surface is the
   input of the next window.
4. The lowest points of the remaining cells make a TIN (no triangle sides
   above 50 m).

The minimum and maximum filters are separable (rows, then columns), so a
window costs \\( 2(2r+1) \\) comparisons per cell. Empty cells are skipped.

**Heights above the ground** — every point to classify (by default classes
0 and 1; points already classified are kept) is measured above the TIN:
within \\( \pm \\) 0.3 m it is ground (2); above, the first height band it
falls in — ASPRS LAS 1.4 classes 3 (low vegetation, up to 0.5 m), 4
(medium, up to 2 m) and 5 (high, up to 60 m), all parameters — and above the
last band high noise (18); below the ground, low noise (7). Buildings,
poles and wires above the ground get vegetation classes too: telling them
apart (planarity, linearity, returns) is not part of this base.

**Conservative:** for the vegetation analysis, a building, pole or wire
point classified as vegetation is checked as a tree: more points reported,
none missed. Not for other rules: a building rule finds no point the
classification gave to vegetation. Exact: a classification that tells
them apart (or classes edited by hand).

Throughput (release build, one core): 4 million points over \\( 10^6 \\)
cells in 1.2 s, about 3 million points a second. Points are held in memory
(32 bytes each; `--max-points`, default 200 million); grids over
\\( 10^8 \\) cells are refused. Exit clauses: a time limit (30 minutes in
`ohl lidar classify`), checked per grid row and every \\( 2^{20} \\) points.

The result is written as LAS/LAZ 1.4 (point format 6) or stored as a new
point cloud in the project file.

## Vegetation

For each wire (a span of a section) of a weather case — a hot case for sag,
a windy one for blowout — every point of the vegetation classes (by role).
Distances are to the conductor's surface, as every clearance (its radius,
iced in a case with ice, and the polyline's bound off the distance to its
axis: [Clearance](../user-guide/terrain.md#clearance)):

- **Encroaching**: its radial distance to the wire (the nearest
  subconductor's surface) is below the required clearance plus the growth until the
  next cut, \\( g = \text{growth per year} \times \text{years} \\) — the
  tree grown by \\( g \\) in every direction.

  **Conservative:** every point grows by \\( g \\) in every direction,
  also sideways and down, where trees mostly grow up and out at their
  tips. Exact: growth by species and direction (not modelled).
- **Fall-in**: with the ground below it, a point of height \\( h \\) above
  the ground (at least 2 m) is taken as a tree top on a trunk straight
  below; falling about its base \\( P_b \\), the top sweeps a sphere of
  radius \\( h + g \\). The tree can reach the wire when
  \\( \min_{q \in \text{wire}} |q - P_b| - r < h + g \\) (\\( r \\): the
  conductor's radius and the polyline's bound); the margin is the
  difference. Leaning trees and crown widths are not modelled.

  **Simplification:** a straight trunk below each point. A leaning tree
  falls further one way, and a crown's width adds to its reach; both
  reach further than computed (unsafe). Exact: the tree's stem and crown
  segmented from the points.

Per wire: the counts, the nearest point, the tree with the smallest fall-in
margin, and the worst points by margin (the smaller of the clearance margin
and the fall-in margin). Wire segments are indexed on a 10 m plan grid with
the search reach (clearance + growth + 5 m, or the tallest tree looked
for), so each point is compared with the few segments near it — about 1 µs
a point.


## Danger trees (planned, 2026-10-20)

*This section describes the method to be built; the section above is
what exists today. The two checks above are extended: trees falling about
their root ball, the wire anywhere along its blowout swing, an
unbalanced-ice case, and isoclearance lines. Every distance is to the
conductor's surface, as above.*

### Wire positions checked

For each weather case chosen (up to a few hundred), each span of each
section is solved by the line's own solver (finite elements). A case with
wind is solved with the wind **from the left and from the right**.

- **Hot / still air:** the case's own position.
- **Blowout:** the left and right positions.
- **Swing arc (optional):** every position the span passes through
  between its left and its right blowout, through the still-air position.
  The span is taken to swing as one about its **axis** \\( a \\), the line
  through its two attachment points \\( A_1, A_2 \\) (string tops; at a
  dead-end, the clamp), unit vector \\( \mathbf u = (A_2 - A_1)/|A_2 - A_1| \\).
  At a station \\( s \\) along the span (a fraction of its length), the
  wire's point lies in the plane square to \\( \mathbf u \\) at
  \\( C(s) = A_1 + s\,(A_2 - A_1) \\): at angle \\( \varphi_L(s) \\) and
  radius \\( R_L(s) \\) blown to the left, \\( \varphi_0(s), R_0(s) \\) in
  still air, \\( \varphi_R(s), R_R(s) \\) blown to the right (angles about
  \\( \mathbf u \\) from the downward vertical). The arc at \\( s \\) is the
  circular arc of centre \\( C(s) \\), radius
  \\( R(s) = \max(R_L, R_0, R_R) \\), from \\( \varphi_L \\) to \\( \varphi_R \\).

  **Conservative:** one radius, the largest of the three, along the whole
  arc (a wind between none and the full one sags the wire between the two
  radii: never further than the largest).

  **Simplification:** a rigid swing about the axis through the
  attachment points. Suspension strings swing with the wire, and the
  clamps move on their own arcs, so the true locus is close to this one
  but not on it. Exact: a solve at each intermediate wind pressure.

  The distance from a point \\( q \\) to the arc of a span: project
  \\( q \\) on the axis, \\( s_q = (q - A_1)\cdot\mathbf u / |A_2 - A_1| \\)
  (clamped to [0, 1]); in the plane at \\( s_q \\), \\( q \\) is at radius
  \\( \rho_q \\) and angle \\( \varphi_q \\). With \\( R, \varphi_L, \varphi_R \\)
  interpolated at \\( s_q \\) from the stations either side:
  \\[ d_{\text{arc}} = \begin{cases} |\rho_q - R| & \varphi_L \le \varphi_q \le \varphi_R \\ \min\big(|q - P_L|,\ |q - P_R|\big) & \text{otherwise} \end{cases} \\]
  (\\( P_L, P_R \\): the arc's ends), and the clearance is
  \\( d_{\text{arc}} - r \\), \\( r \\) the conductor's radius and the
  polyline's bound as everywhere.

- **Unbalanced ice (optional):** the span checked carries more ice than
  its neighbours (by default: the case's ice on that span only, or a
  given extra ice load or concentrated load on it), so its suspension
  strings swing into it and it sags further. Each span checked is solved
  with its section on its own (the section's other spans as in the case).

### Grow-in

A vegetation point \\( q \\) encroaches on a wire when its distance to the
wire's surface, for any position checked above, is below the required
clearance plus the allowances:
\\[ d(q, \text{wire}) - r < c_{\text{req}} + g + b \\]
with \\( c_{\text{req}} \\) the required clearance (flashover or minimum
approach distance, by voltage, from the rule), \\( g \\) the growth until
the next cut, \\( b \\) a buffer (e.g. for the survey's accuracy). The margin
is the difference; the worst position (hot, left, right or on the arc) is
reported.

### Fall-in: a tree pivoting about its root ball

A vegetation point \\( q_t \\) at height \\( h \\) above the ground below it
(at least the smallest tree height) is taken as the top of a tree whose
trunk stands straight below it, base \\( B \\) on the ground. Grown by
\\( g \\), its height is \\( h' = h + g \\). The tree falls by turning about
the edge of its root ball: a hinge \\( H \\) on the ground at the
**root-ball radius** \\( b_r = k\,h' \\) from the trunk, \\( k \\) a given
fraction of the height (e.g. 0.1), on the side it falls to.

It falls towards the wire point \\( q \\) (the worst direction). In the
vertical plane through \\( B \\) and \\( q \\), with coordinates \\( (x, z) \\)
from \\( B \\) (\\( x \\) towards \\( q \\), \\( z \\) up):
\\( H = (b_r, 0) \\), the top \\( T = (0, h') \\), the trunk's length from the
hinge \\( L = \sqrt{h'^2 + b_r^2} \\) and its starting angle about the hinge
\\( \theta_0 = \operatorname{atan2}(h', -b_r) \\) (just over 90°). Falling, the
trunk sweeps the **sector** of radius \\( L \\) about \\( H \\) from
\\( \theta_0 \\) down to \\( \theta_{\min} \\), where it comes to rest on the
ground: the largest angle, seen from \\( H \\), of the ground along the fall
direction within reach,
\\[ \theta_{\min} = \max_{0 < u \le L} \operatorname{atan2}\big(z_g(b_r + u) - z_B,\ u\big) \\]
(\\( z_g \\): the ground's height along the direction; on level ground
0°, falling down a slope below 0°).

With \\( q = (\rho, \zeta) \\) in that plane (\\( \rho \\) its horizontal
distance from \\( B \\), \\( \zeta \\) its height above \\( B \\)) and
\\( \theta_q = \operatorname{atan2}(\zeta, \rho - b_r) \\) its angle about
\\( H \\), the distance from the wire point to the swept trunk is
\\[ d_{\text{fall}} = \begin{cases} \max\big(0,\ |q - H| - L\big) & \theta_{\min} \le \theta_q \le \theta_0 \\ \min\big(d(q, [H, H + L e^{i\theta_{\min}}]),\ d(q, [H, T])\big) & \text{otherwise} \end{cases} \\]
The tree is a **danger tree** when, for some wire point and position,
\\[ d_{\text{fall}} - r < A \\]
with \\( A \\) the clearance allowance (flashover clearance, buffers; the
growth is already in \\( h' \\)). The fall-in margin is the difference. With
\\( k = 0 \\) and level ground this is the check above (a sphere of radius
\\( h' \\) about the base), so the existing results are the special case.

**Conservative:** the tree falls straight towards each wire point, the
worst direction for every point.
**Simplification:** a straight trunk below the top point and no crown;
the root ball as a hinge at a fixed fraction of the height. A leaning tree
falls further one way and a crown reaches further than its trunk (unsafe).
Exact: the stem and crown segmented from the points, lean measured.

### Isoclearance lines

The **clearance** of a ground point \\( G = (x, y, z_g(x, y)) \\) is its
distance to the nearest wire's surface over the positions checked:
\\[ c(x, y) = \min_{\text{cases, positions, wires}} \big(d(G, \text{wire}) - r\big) \\]
For one case, or the worst (smallest) over many. **Isoclearance lines** are
its contours at a chosen interval (e.g. 1 m or 5 m), drawn over the
corridor; lines above a chosen value can be hidden, so what remains shows
where the ground or the vegetation comes closest. A tree of height
\\( h' \\) standing inside the \\( h' \\) line can reach a wire when it falls
(\\( k = 0 \\), level ground).

\\( c \\) is computed on a grid of the corridor (spacing \\( \delta \\), e.g.
1 m) and contoured by marching squares with linear interpolation between
nodes.

**Simplification:** a contour is placed by linear interpolation between
grid nodes; the values at the nodes are exact. As \\( c \\) changes by at
most the distance moved along the ground (it is a distance:
\\( |c(G_1) - c(G_2)| \le |G_1 - G_2| \\)), a contour is placed within
\\( \delta\sqrt{1 + m^2} \\) of the true line (\\( m \\): the ground's slope).
The contours are a drawing; the checks above use the exact distances.

### Results

Per span: the number of grow-in and fall-in points, the worst of each
(point, margin, the position and case, the wire), and the list of
violating points; per line the totals. Shown in the 3D view, the plan and
the profile (grow-in and fall-in as their own symbols and layers), in a
cross-section across the span at a chosen point (the swing arc, the
point, the fall sector), on the drawings, and as CSV.

### How it is made fast

LiDAR surveys hold \\( 10^7 \\) to \\( 10^8 \\) points per line, and every
vegetation point meets every position of every case. The work is kept to
the points and wires that can matter; the numbers computed are never
coarsened.

- **Only what can reach.** A point is compared only with the spans within
  its *reach*: \\( c_{\text{req}} + g + b \\) for grow-in, and the tallest
  tree looked for plus its root ball plus \\( A \\) for fall-in. Spans are
  indexed once per position on a plan grid (cells of about the reach), so
  a point meets the few spans near it, not the line.
- **One bound per span before the exact distance.** Each span's positions
  (hot, left, right and the swing arc) fit in a capsule about its axis of
  radius \\( \max R + \) reach; a point outside it is rejected with one
  distance to a segment. Only points inside get the exact distances
  above, each a closed form (no iteration).
- **Cases share the work.** Positions of all cases are gathered per span;
  the point is looked up once and compared with all its span's positions.
  The smallest margin found so far bounds the rest: a case cannot make a
  point worse once its distance exceeds the current worst by the reach.
- **Ground once per point.** The height above the ground (for fall-in)
  comes from the ground surface once per point; the fall sector's resting
  angle \\( \theta_{\min} \\) is computed only for points that are near
  enough, by stepping along the fall direction at the TIN's resolution.
- **Unbalanced ice only where trees are.** A span is solved with its own
  ice only if vegetation points lie within its reach (most spans over
  fields have none). Spans are independent: they are solved in parallel.
- **Isoclearance by refinement.** \\( c \\) is first computed on a coarse
  grid (e.g. 8 m). Because \\( c \\) cannot change faster than the distance
  moved, a coarse cell whose corner values all exceed the highest line
  drawn by more than its diagonal (times \\( \sqrt{1 + m^2} \\)) cannot hold
  a drawn line and is not refined. Only cells near the wires or near
  low clearance are refined to \\( \delta \\).
- **Parallel by tiles.** Points are read tile by tile from the project's
  point cloud (its spatial chunks) and the tiles are processed in
  parallel; results are merged per span.
- **Expected cost:** about 1 µs per vegetation point and position set
  today; with the swing arc and a few dozen cases, a line of \\( 10^8 \\)
  points (a third of them vegetation) is expected in minutes on a
  10-core computer, and an isoclearance drawing of a 50 km corridor in
  seconds. Exit clauses: a time limit per run, reported, never a partial
  result shown as complete.

## As-built wires

**Points of a span**: points of the wire classes (roles `conductor`,
`shield_wire`) within a plan distance (1 m) of the vertical plane through
the modelled span's chord, within a height window (3 m) of the modelled wire
(still air, a chosen temperature), away from the ends by 5 % of the span.
Each becomes \\( (s, z) \\): horizontal distance along the chord from the
first clamp, and elevation. Bundle subconductors are fitted together.

**Fit**: the catenary in the plane,

\\[ z(s) = z_0 + C\left[\cosh\frac{s - s_0}{C} - 1\right], \\]

with low point \\( (s_0, z_0) \\) and catenary constant \\( C = H/w \\), by
least squares on the vertical residuals. The start is a least-squares
parabola \\( z = a u^2 + b u + e \\) (\\( u \\) centred on the mean
\\( s \\)): \\( C = 1/2a \\), \\( s_0 = \bar s - b/2a \\),
\\( z_0 = e - b^2/4a \\). Gauss–Newton steps with Levenberg–Marquardt
damping (D. W. Marquardt, *J. SIAM* 11(2), 1963) on the Jacobian

\\[ \frac{\partial z}{\partial C} = \cosh u - 1 - u\sinh u, \quad
    \frac{\partial z}{\partial s_0} = -\sinh u, \quad
    \frac{\partial z}{\partial z_0} = 1, \qquad u = \frac{s - s_0}{C} \\]

until the step is below \\( 10^{-10} C \\) (about ten iterations; exit
clause 500). Points further than three RMS (at least 5 cm) from the first
fit are dropped (birds, other wires inside the window) and the fit repeated
once.

**Simplification:** the span's points are taken in the vertical plane
through its chord (still air at the survey). A wire blown sideways at the
survey is fitted by its sag's vertical part, smaller than its true sag,
so the temperature found is too low. Exact: a 3D catenary fit with the
plane's swing as an unknown.

**Sag** of the fitted wire: the largest vertical distance below the chord
from \\( s = 0 \\) to \\( s = L \\) — the model's definition. Where the
curve is parallel to the chord, \\( \sinh((s^* - s_0)/C) = m \\) with
\\( m \\) the chord's slope, so \\( s^* = s_0 + C \operatorname{asinh} m \\).
Compared with the model: the sag difference, the height difference at
mid-span and at the low point.

**Temperature at the survey**: the section is solved in still air at trial
temperatures — with its stringing, creep condition and clamping as modelled
— until the span's sag equals the fitted sag: regula falsi (Illinois
variant) on \\( \text{sag}(T) - D_{fit} \\), increasing in \\( T \\),
between −30 and 120 °C, to 0.01 °C or 0.1 mm of sag (about six solves; exit
clause 60). Outside that range no temperature is given. The temperature is
that of the model's sag–temperature relation: errors of the model (stringing
tension, creep) show up as temperature.

*Tests:* a synthetic survey of the example line at 35 °C — points every
0.5 m along each span with ±3 cm noise, birds above the wires — gives every
span's sag within 2 cm and its temperature within 2 °C (in practice
±0.5 °C). *Validation:* `wirefit-001…002` (exact points of known catenaries;
sag against a brute-force evaluation).
