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
   \\( \Delta h_{max} \\); slope \\( s \\) = 0.3, \\( \Delta h_0 \\) = 0.3 m,
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

Throughput (release build, one core): 4 million points over \\( 10^6 \\)
cells in 1.2 s, about 3 million points a second. Points are held in memory
(32 bytes each; `--max-points`, default 200 million); grids over
\\( 10^8 \\) cells are refused. Exit clauses: a time limit, checked per grid
row and every \\( 2^{20} \\) points.

The result is written as LAS/LAZ 1.4 (point format 6) or stored as a new
point cloud in the project file.

## Vegetation

For each wire (a span of a section) of a weather case — a hot case for sag,
a windy one for blowout — every point of the vegetation classes (by role):

- **Encroaching**: its radial distance to the wire (the nearest
  subconductor) is below the required clearance plus the growth until the
  next cut, \\( g = \text{growth per year} \times \text{years} \\) — the
  tree grown by \\( g \\) in every direction.
- **Fall-in**: with the ground below it, a point of height \\( h \\) above
  the ground (at least 2 m) is taken as a tree top on a trunk straight
  below; falling about its base \\( P_b \\), the top sweeps a sphere of
  radius \\( h + g \\). The tree can reach the wire when
  \\( \min_{q \in \text{wire}} |q - P_b| < h + g \\); the margin is the
  difference. Leaning trees and crown widths are not modelled.

Per wire: the counts, the nearest point, the tree with the smallest fall-in
margin, and the worst points by margin (the smaller of the clearance margin
and the fall-in margin). Wire segments are indexed on a 10 m plan grid with
the search reach (clearance + growth + 5 m, or the tallest tree looked
for), so each point is compared with the few segments near it — about 1 µs
a point.

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
