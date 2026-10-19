# Galloping ellipses and blowout

Implemented in `crates/ohl-clearance` (`galloping`, `blowout`, and the
`same_line` rules of `rules`/`checks`).

## Galloping ellipses

Galloping is a low-frequency, large-amplitude oscillation of iced wires in
wind; the wire moves on a roughly elliptical path in the plane square to the
span, with one or more loops (standing half-waves) between the supports.
Design practice draws the **galloping ellipse** — the envelope of that
motion — around each wire and requires the ellipses of different phases,
circuits and earth wires not to touch, or to keep a small clearance.

The published methods are empirical. A. E. Davison ("Dancing conductors",
*AIEE Transactions* 58, 1939) proposed a single-loop ellipse sized from the
sag; the US Rural Utilities Service design manual (RUS Bulletin 1724E-200)
draws single- and two-loop ellipses from the sag following that line of
work; CIGRE Technical Brochure 322 (*State of the art of conductor
galloping*, 2007) collects amplitude formulas from field data, among them
expressions in the conductor diameter, the number of loops and the sag.
They differ in coefficients, in how the ellipse sits on the wire and in how
it is tilted. **OpenOHL ships none of their coefficients**: every number
comes from a parameter file written by the user from the method they design
to (`*.ohlg`, [user guide](../user-guide/analyses.md)); the shipped example's
numbers are invented.

### The ellipse

For a span of horizontal length \\( L \\), sag \\( D \\) (in the weather case
checked: the largest vertical distance below the chord, as solved), conductor
diameter \\( d \\) and \\( n \\) loops, each axis is a sum of terms with
user coefficients:

\\[ A = k_D D + k_L L + k_{\sqrt D} \sqrt D + k_d d + k_{\ln}\\, n d \ln\frac{s D}{n^2 d} + k_0, \qquad
    B = (\text{the same terms}) + k_A A \\]

\\( A \\) is the major axis, \\( B \\) the minor axis (full lengths). The
major axis is tilted by \\( \theta \\) from the vertical (positive: its top
to the right, looking from the span's first structure to its last), and the
ellipse's centre lies on it a distance \\( c\\,A \\) above the wire at rest
(`centre_above_rest = c`; 0.5 puts the wire at the lower end of the major
axis).

Along the span the ellipse is swept with the loop shape: at the fraction
\\( t \\) of the span its size (axes and centre offset) is scaled by
\\( |\sin(n\pi t)| \\) — each loop largest in its middle, nothing at the
supports — or kept the same (`along = "constant"`, as some methods draw one
ellipse at mid-span for the whole span).

### Clearance between ellipses

Wires hanging between the same two structures are paired (phases of one
circuit, circuits on one tower, phases and earth wires). At `stations − 1`
inner points of the span (default 19) both ellipses are drawn in the plane
square to the first wire's chord, each around its own wire at rest there,
and their distance is found: zero when they overlap (a vertex of one inside
the other, or crossing edges), else the shortest vertex-to-edge distance.
The smallest over the stations is reported per pair and method, with the
requirement (a distance, or by voltage as in rule files, with the pair's
voltage taken as the higher, sum or difference of the two systems').

Accuracy: the ellipses are 72-sided inscribed polygons, so a distance is
high by at most \\( A(1-\cos 2.5°)/2 \\), about 0.05 % of the major
axis; the wires of a span are taken as parallel (one cross-section plane).

**Simplification:** the inscribed polygons lie inside the ellipses, so the
gap found is high by at most that amount (unsafe, by about 0.05 % of the
major axis). Exact: the distance between the true ellipses (or polygons
circumscribed about them, which would be on the safe side).

**Simplification:** both ellipses in the plane square to the first wire's
chord, the wires taken as parallel there. Wires that are not parallel (a
line angle between their spans, different attachment heights) are compared
at slightly different places along the span (either way). Exact: the
ellipses swept along each wire and their distance in 3D.

**Simplification:** only wires between the same two structures are paired.
Galloping of a span next to another line's span, or of adjacent spans
near a structure, is not compared. Exact: pairs of every wire within
reach.

Cost: stations × 72² distances per pair and method, about 0.1 ms; one step
of the exit clause (`Budget`) per pair and method.

*Validation:* `galloping-001…004` (axes from the sag, the ellipse along the
span, gaps between ellipses side by side, tilted and one above the other)
against hand calculations and an independent evaluation through the
difference body (`benchmarks/clearance/reference_clearance.py`).

## Blowout

The wind swings each span's wire plane, and the suspension strings with it;
the solved shapes of a windy weather case are the blown-out wires. Two
checks use them:

- **Objects and other wires**: the radial clearances of the clearance rules
  in that case — LiDAR points of the chosen classes (buildings, trees),
  other wires, the ground.
- **Right of way**: each wire point is projected onto the centreline of its
  route (the route holding both structures of its span; the nearest route
  otherwise): the signed offset, right positive, plus the bundle radius.
  The largest offsets to the right and to the left are compared with the
  right-of-way widths each side; **encroachment** is how far the wire goes
  beyond the edge (negative: the margin inside).

  **Simplification:** the offset is to the wire's axis plus the bundle
  radius: the conductor's own radius is not added (unsafe by half its
  diameter). Exact: the offset of the conductor's outer surface.

## Phase to phase and circuit to circuit

`same_line` rules check every pair of wires of the line (wires sharing an
end point within 1 cm, such as adjacent spans and a jumper and its
sections, excepted; wires of the same circuit and phase too, unless
`include_same_phase = true`). `circuits = "same"` keeps pairs of
the same circuit (phase to phase), `"different"` pairs of different circuits
(circuit to circuit). With `case_pairs` the first wire is taken in one
weather case and the second in another — one circuit at maximum sag and the
other with ice dropped off, an iced upper phase over a bare lower one.
