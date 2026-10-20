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
and the fall-in margin). Since 2026-10-20 this is the special case of the
danger-tree check below (one case as it is, no root ball, no allowance, no
buffer), with the same results; how the points meet the wires is described
there (about 0.5 µs a point on one core).


## Danger trees (2026-10-20)

*Built, in active development (not approved yet). The two checks above
are extended: trees falling about their root ball, the wire anywhere along
its blowout swing, an unbalanced-ice case, and isoclearance lines. With
the root-ball fraction, the clearance allowance and the buffer at 0 (the
command line's defaults) the results are those of the section above, bit
for bit. Every distance is to the conductor's surface, as above. Code:
`ohl_clearance::fall`, `ohl_clearance::positions`,
`ohl_clearance::vegetation`, `ohl_clearance::isoclearance`.*

### Wire positions checked

For each weather case chosen (up to a few hundred), each span of each
section is solved by the line's own solver (finite elements). A case with
wind is solved with the wind **from the left and from the right**.

- **Hot / still air:** the case's own position (a case without wind).
- **Blowout:** the left and right positions (a case with wind).
- **Swing arc (optional):** every position the span passes through
  between its left and its right blowout, through the still-air position
  (the case solved once more without wind, for the arc only).
  The span is taken to swing as one about its **axis** \\( a \\), the line
  through its two attachment points \\( A_1, A_2 \\) (string tops; at a
  dead-end, the clamp), unit vector \\( \mathbf u = (A_2 - A_1)/|A_2 - A_1| \\).
  As built, an attachment is the top of the one string hanging below it
  at a point inside the section; at the section's ends, a V-string or a
  wire fixed to the structure, the clamp (as solved in the case's first
  position; the string tops do not move).
  At a station \\( s \\) along the span (a fraction of its length), the
  wire's point lies in the plane square to \\( \mathbf u \\) at
  \\( C(s) = A_1 + s\,(A_2 - A_1) \\): at angle \\( \varphi_L(s) \\) and
  radius \\( R_L(s) \\) blown to the left, \\( \varphi_0(s), R_0(s) \\) in
  still air, \\( \varphi_R(s), R_R(s) \\) blown to the right (angles about
  \\( \mathbf u \\) from the downward vertical, positive to the right). The
  arc at \\( s \\) is the circular arc of centre \\( C(s) \\), radius
  \\( R(s) = \max(R_L, R_0, R_R) \\), from \\( \varphi_L \\) to \\( \varphi_R \\).

  **Conservative:** one radius, the largest of the three, along the whole
  arc (a wind between none and the full one sags the wire between the two
  radii: never further than the largest).

  **Conservative:** the arc runs from the smallest to the largest of
  \\( \varphi_L, \varphi_0, \varphi_R \\) (the still-air position need not
  lie between the two blowouts, e.g. at a line angle).

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

  As built: the stations are every point of the three solved wires
  (their places along the axis); between two stations each wire is one
  straight piece, so the three wires' points at \\( s_q \\) are
  interpolated (exact for the polylines) and \\( R \\) and the angles are
  worked out from them — the same as interpolating \\( R \\) and the
  angles, without the error of interpolating a radius. A point beyond the
  span's ends (\\( s_q \\) clamped) has its distance along the axis
  counted too: \\( \sqrt{(\rho_q - R)^2 + a^2} \\), \\( a \\) its distance
  from the end's plane. The arc's ends \\( P_L, P_R \\) are on the arc, at
  radius \\( R \\). **Conservative:** for a bundle, \\( r \\) on the arc is
  the bundle radius plus the subconductor's radius and the polyline's
  bound (the bundle's outer envelope), where the polylines take each
  subconductor.

- **Unbalanced ice (optional):** the span checked carries more ice than
  its neighbours (by default: the case's ice on that span only, or a
  given extra ice load or concentrated load on it), so its suspension
  strings swing into it and it sags further. Each span checked is solved
  with its section on its own (the section's other spans as in the case).

  As built: only the spans with vegetation points within reach (in plan,
  of the span's positions) are solved; each span and case on its own, in
  parallel. *The case's ice on that span only*: the section solved in the
  case without ice, the span carrying the ice's weight per metre (the
  case's iced load less the bare wire's, times the bundle) as a load
  spread along it; the span's surface radius is the iced one. *An extra
  load*: the case as it is and the given load per metre (N/m, the whole
  bundle's) spread along the span. *A concentrated load*: the case as it
  is and the given load (N, the whole bundle's) at mid-span. A windy case
  is solved from the left and from the right. Cases without ice are left
  out of *the case's ice*.

  **Simplification:** the ice on the span alone is a weight spread along
  it: the wind on that span acts on the bare diameter (the wind on the
  extra ice is left out). Exact: the span's own iced diameter in the
  solver.

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
fraction of the height (a user setting: the command line's default 0, the
desktop application proposes 0.1), on the side it falls to.

**The tree base needs a ground point near it** (optional,
`max_base_ground_distance`, e.g. 5 m): the ground's height below the
point — its base — is taken only when a point the ground surface is built
from (a TIN vertex: a LiDAR ground point or a survey point) lies within
that horizontal distance; else the point has no base and no fall-in check,
and is counted as "no ground point within the distance" (with the points
that have no ground below them at all). It guards against a base
interpolated across a large gap in the ground points — a steep slope, a
hole under dense canopy — where the TIN's height may be metres off and the
tree's height with it. **Simplification:** such a point is left out of
the fall-in check rather than given a guessed height (it is reported, not
assumed safe: the count says how many). Default: no limit.

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

As built, \\( \theta_{\min} \\) is taken at every edge crossing of the
ground's TIN along the direction (the profile from \\( H \\) to \\( L \\)
beyond it): between crossings the ground is linear and its angle seen from
\\( H \\) turns one way, so the largest is at a crossing — the TIN's
resolution, exactly. Parts of the direction without ground (a hole) do
not stop the trunk; with no ground at all, −90°.
**Conservative:** where the ground is higher than the base (falling
uphill), its angle is seen from \\( B \\) instead (\\( u + b_r \\) for
\\( u \\)): seen from \\( H \\), at the base's height, rising ground just
beyond it would stand at nearly 90° and stop the trunk at once; from
\\( B \\) it rises at its slope. A smaller resting angle is a larger
sector. With \\( k = 0 \\) the two are the same.

With \\( q = (\rho, \zeta) \\) in that plane (\\( \rho \\) its horizontal
distance from \\( B \\), \\( \zeta \\) its height above \\( B \\)) and
\\( \theta_q = \operatorname{atan2}(\zeta, \rho - b_r) \\) its angle about
\\( H \\), the distance from the wire point to the swept trunk is
\\[ d_{\text{fall}} = \begin{cases} |q - H| - L & \theta_{\min} \le \theta_q \le \theta_0 \\ \min\big(d(q, [H, H + L e^{i\theta_{\min}}]),\ d(q, [H, T])\big) & \text{otherwise} \end{cases} \\]
The tree is a **danger tree** when, for some wire point and position,
\\[ d_{\text{fall}} - r < A \\]
with \\( A \\) the clearance allowance (flashover clearance, buffers; the
growth is already in \\( h' \\)). The fall-in margin is the difference. With
\\( k = 0 \\) and level ground this is the check above (a sphere of radius
\\( h' \\) about the base), so the existing results are the special case.

*Changed from the method as first written:* within the sector the
distance is \\( |q - H| - L \\), negative when the trunk passes beyond the
wire point, where the method had \\( \max(0, |q - H| - L) \\). The verdict
is the same; the margin also says by how much the tree reaches past the
wire, as the margins before did (with \\( k = 0 \\) they are unchanged).

**The smallest over the wire, as built.** Within the sector,
\\( |q - H(q)| \\) is the distance from \\( q \\) to the horizontal
**hinge circle** of radius \\( b_r \\) about \\( B \\) (\\( H(q) \\) is its
point towards \\( q \\)), so the smallest \\( d_{\text{fall}} \\) over a
position is the distance between the position and that circle, less
\\( L \\). To a solved wire it is found piece by piece: along a piece
\\( a + t\,d \\) the squared distance to the circle
\\( \varphi(t) = |q - B|^2 - 2 b_r \rho + b_r^2 \\) has
\\( \varphi'' \ge 2|d|^2 (1 - b_r/\rho) \\), convex wherever the piece stays
further than \\( b_r \\) from the base in plan, and its one minimum is
found by Newton steps inside a shrinking bracket to \\( 10^{-12} \\) of the
piece (exact); a piece over the root ball (rare) by the bound of a
distance to \\( 10^{-6} \\) m, the bound taken. To a swing arc it is the
smallest of \\( d_{\text{arc}} \\) over the circle's points, by the bound of
a distance along the circle (Piyavskii–Shubert: it changes by at most
\\( b_r \\) per radian), to 1 mm. **Conservative:** on an arc the bound,
at most 1 mm below the smallest distance, is taken. With \\( k = 0 \\) the
circle is the base: the nearest wire point to the base, as before. Then
the resting angle along the direction of that wire point, and the
sector's test: within the angles, the margin is the circle's. Outside
them (the nearest point below the trunk at rest — sheltered by a ridge —
or above the root ball), the smallest of the "otherwise" distance along
the wire, by the bound of a distance along the wire to 1 mm (the bound
taken), and the other positions in the order of their circle margins.
**Simplification:** in that search the distance is taken to change no
faster than the wire point moves; the resting angle changes with the
direction, so a sharp change of the ground across directions could be
missed. Exact: the resting angle as a function of the direction.
**Conservative:** on a swing arc the shelter is not credited (the
circle's margin is kept).

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
nodes (a saddle cell split by its centre's value).

**Simplification:** a contour is placed by linear interpolation between
grid nodes; the values at the nodes are exact. As \\( c \\) changes by at
most the distance moved along the ground (it is a distance:
\\( |c(G_1) - c(G_2)| \le |G_1 - G_2| \\)), a contour is placed within
\\( \delta\sqrt{1 + m^2} \\) of the true line (\\( m \\): the ground's slope).
The contours are a drawing; the checks above use the exact distances.

As built: the corridor is the half width around the spans' chords (the
command's `--corridor`, the drawings' line width, the application's
corridor); the lines run from 0 at the interval up to the top line
("hide above", default 40 m). Clearances are worked out up to the top
line plus a coarse cell's diagonal times \\( \sqrt{1 + m^2} \\) with
\\( m \\) the ground's steepest triangle (beyond it a node is above every
line drawn, and so is every point of its cells). **Simplification:**
slopes above 5:1 (79°, TIN artefacts and cliffs) are taken as 5:1 in
these bounds, else a single near-vertical triangle would make the reach
huge; on such ground a coarse cell just above the top line could hold a
line and not be refined. **Simplification:** a
coarse cell with no corner inside the corridor (or on the ground) is left
out — a corridor narrower than a coarse cell could lose a line.

### Results

Per span: the number of grow-in and fall-in points, the worst of each
(point, margin, the position and case, the wire), and the list of
violating points; per line the totals. Shown in the 3D view, the plan and
the profile (grow-in and fall-in as their own symbols and layers), in a
cross-section across the span at a chosen point (the swing arc, the
point, the fall sector), on the drawings, and as CSV.

As built: per span the counts (a point counted once per span, whatever
its positions), the nearest point (the worst grow-in) and the worst
falling tree with the case and position of each, and the worst points
(default 10 per span) — in the command's table and CSV, and in the
application's window. The application draws grow-in points (cyan dots
joined to the wire) and danger trees (an orange trunk from base to top, a
line to the wire point it falls to) on their own layers in the 3D view
and the plan, and in the profile within the line width; the isoclearance
lines on their layer in the 3D view and the plan, and on the drawings'
plan (layer OHL_ISOCLEARANCE). *Left for later:* the cross-section view
(the application has no cross-section facility yet).

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
  As built: each span is indexed once with all its positions (cells of a
  quarter of the reach, at least 10 m).
- **One bound per span before the exact distance.** Each span's positions
  (hot, left, right and the swing arc) fit in a capsule about its axis of
  radius \\( \max R + \) reach; a point outside it is rejected with one
  distance to a segment. Only points inside get the exact distances
  above, each a closed form (no iteration).
  As built: then a bound from **bands** — for every 20 m along the axis,
  how far from the axis and in which directions about it each position
  (and all of the span's together) lies; a position is worked out only if
  its bound can beat the best found. The nearest piece of a solved wire is
  looked for outward from the point's place along the axis, stopping when
  the places alone are further than the best. (The fall-in distance to a
  wire over the hinge circle is found by the Newton steps above, on an arc
  by the bound along the circle — not closed forms.)
- **Cases share the work.** Positions of all cases are gathered per span;
  the point is looked up once and compared with all its span's positions.
  The smallest margin found so far bounds the rest: a case cannot make a
  point worse once its distance exceeds the current worst by the reach.
  As built for fall-in: a tree is worked out fully (circle, resting angle,
  shelter) only when its bound is below 0 (it may count) or below the
  span's worst fall so far (it may be reported); a tree that becomes the
  span's nearest point or enters its worst list is worked out fully
  then. Counts never depend on it (a bound from 0 up never counts).
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
  low clearance are refined to \\( \delta \\). As built: \\( m \\) is the
  steepest triangle near the cell; rows of cells in parallel.
- **Parallel by tiles.** Points are read tile by tile from the project's
  point cloud (its spatial chunks) and the tiles are processed in
  parallel; results are merged per span. As built: the points are read
  chunk by chunk into memory (13 bytes a point, whole millimetres), then
  checked in blocks of 65 536 points (about a stored chunk) on every
  core (the standard library's scoped threads, as the rest of the code);
  the results merged per span in a fixed order (the same answer whatever
  the threads).
- **Expected cost:** about 1 µs per vegetation point and position set
  today; with the swing arc and a few dozen cases, a line of \\( 10^8 \\)
  points (a third of them vegetation) is expected in minutes on a
  10-core computer, and an isoclearance drawing of a 50 km corridor in
  seconds. Exit clauses: a time limit per run, reported, never a partial
  result shown as complete.

  **Measured** (an invented 10 km line of 25 spans, three phases and an
  earth wire, a million invented vegetation points over ±60 m — a cleared
  strip of ±12 m and trees up to 28 m beyond, an unusually dense share of
  danger trees: 2–12 % of the points; a 10-core computer; the
  ignored test `danger_speed`): one case as before (k = 0) 0.4–0.7 µs a
  point on one thread, 0.08–0.18 µs on ten; one case with k = 0.1
  0.6–1.0 µs (0.1–0.2); one windy case with left, right and the swing arc,
  k = 0.1: 6–12 µs (0.9–2.1); twenty windy cases with their arcs (6000
  positions): 31–90 µs (4.6–14) — most of it the fall-in of danger trees
  against the arcs (the search along the hinge circle). Isoclearance of
  the 10 km corridor (±60 m, lines every 5 m to 40 m, 1 m grid refined
  from 8 m) for the twenty cases: 6–9 s (1.6 million clearances, most
  cells refined: lines run all along a corridor of that width) — a 50 km
  corridor about half a minute, not seconds; one case is about ten times
  faster. On a real line (930 spans of its sections, about 37 km of
corridor ±60 m, 18.7 million vegetation points of 100 million, 10
cores): one case as before, the check 1 s (0.05 µs a point); three cases
— hot, windy with left, right and the swing arcs, iced with unbalanced
ice on every span (930 solves) — with k = 0.1 and A = 2 m: the solves
13–23 s, the check 21–34 s (1.1–1.8 µs a point), the isoclearance
lines 13–14 s (9 million clearances). Exit clauses: `--time-limit` (default an hour) for a run in the
  command line, an hour in the application, checked between blocks and
  rows: an error, never a partial result.

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
