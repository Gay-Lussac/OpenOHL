# Safety volumes (2026-10-20)

*Built, in active development (not approved yet). The owner's request of
2026-10-20: a volume around the lines, from weather cases and a distance
rule by voltage, exported as a georeferenced file, for the safety of work
near the line. Code: `ohl_clearance::safety_volume` (the envelope, its
mesh and its simplification), `ohl_io::safety_volume` (the files).*

## The envelope

For each wire (a circuit's phase through all its sections), each
circuit or each electrical system (its circuits together, 2026-10-21),
the safety volume is the set of points within a distance of the
wire in any of its positions:

\\[ E = \bigcup_{\text{positions } k} \{ x : \operatorname{dist}(x, W_k) \le D_k \}, \qquad D_k = d(U) + m + r_{\text{outer}} \\]

- \\( W_k \\): the centre line of position \\( k \\) — each weather case
  chosen as solved by the line's solver (still air as it is; a windy case
  blown to the left and to the right at its full pressure), and as asked
  for, every **wind step** (still air and every intermediate pressure) and
  the **swing arc** (since 2026-10-21 a band: the solid swept by the
  span swinging rigidly about its attachment axis between the two
  blowouts, at every radius from the smallest to the largest of the
  three) — the positions of the danger-tree check
  ([LiDAR analyses › Wire positions checked](lidar-analysis.md#wire-positions-checked)) —
  and, since 2026-10-21, the **jumpers** at their solved positions in
  each of those solves (blown left and right, every step, still air) and,
  with the swing arc, on a **swing band of their own** about the line
  through their two end clamps (still air then inside the band).
- \\( d(U) \\): the distance of the rule at the nominal voltage \\( U \\)
  of the wire's system (an earthed wire at 0 kV): one distance, a table
  "up to kV → distance" or \\( d_0 + d_1 (U - U_0) \\) above \\( U_0 \\),
  as the clearance rules write it. **No distance is built in**: the user
  enters it, or fills it from a rule of a design-code plugin's preset (its
  radial distance, else its vertical one).
- \\( m \\): an optional margin.
- \\( r_{\text{outer}} \\): what every clearance takes off a distance to
  the centre line (`Wire::outer_radius`): the bundle radius, the
  subconductor's radius (iced in a case with ice) and the bound of the
  polyline's chord sag. The distance is therefore measured from the
  conductor's surface. **Conservative:** for a bundle, the outer envelope
  of the subconductors is taken as a circle about the bundle's centre
  (exact: the union of the subconductors' own tubes), as everywhere in the
  clearance checks.

A solved position contributes a **tube**: the union of capsules (a
cylinder and two half balls) of radius \\( D_k \\) along its polyline. A
jumper's solved positions are tubes like the spans' (its own conductor's
radius and bundle in \\( r_{\text{outer}} \\)); it hangs from clamp to
clamp, so its tube joins the tubes of the sections it connects and the
volume is **continuous at the structures** (a dead end, a branch).

The **swing arc** contributes every point within \\( D_k + m_b \\) of the
band (\\( m_b \\) its margin, 0.15 m by default; since 2026-10-21 a
parameter — the window's **band margin**, `--band-margin` — recorded
with a saved run: a smaller value is less safe and on the user's
responsibility): at a place \\( s \\) along the
span's axis the annular sector of centre \\( C(s) \\) from radius
\\( r(s) \\) to \\( R(s) \\) between \\( \varphi_L(s) \\) and
\\( \varphi_R(s) \\) — the closed form of the danger-tree page, with its
**Conservative** labels (every radius from the smallest to the largest;
the band's margin; the angles from the smallest to the largest) and its
**Simplification** (a rigid swing). Distances are measured in 3D, so
beyond the band's ends and beyond the span's ends the volume is rounded
(balls of radius \\( D \\)).

A **jumper's band** (2026-10-21, the owner's decision) is the same
closed form about the axis through the jumper's two end clamps (blown to
the left, the first solve; `positions::jumper_band`), from the smallest
to the largest radius of its three positions, widened by a margin of its
own \\( m_j \\) (the window's **jumper band margin**,
`--jumper-band-margin`, recorded with a saved run; **0 m by default**),
meshed as a span's.

**Simplification** (the jumpers' margin; on the user's responsibility;
exact: the wind steps): with \\( m_j = 0 \\) a jumper's band is **not
conservative between its blowouts** — on the example line its jumper's
10 Pa steps lie up to 0.293 m outside it
([the swing arc](lidar-analysis.md#wire-positions-checked)). The user
enters a value for their jumpers. The jumper's solved positions (blown
left and right, every step) remain tubes of their own, so the volume
holds them exactly; only the positions between them depend on
\\( m_j \\).

As built, the band's boundary is cut into triangles: its outer and inner
surfaces (radius \\( R \\) and \\( r \\)), its two end edges swept along
the span (from \\( r \\) to \\( R \\) at \\( \varphi_L \\) and
\\( \varphi_R \\)), and its end faces across the span's ends where the
band has a thickness there. Every point outside the band within
\\( D \\) of it is within \\( D \\) of its boundary; inside, where the
band is thicker than \\( 2D \\) (rare: the thickness is a metre or two),
surfaces at intermediate radii at most \\( 2D \\) apart are added, so
that every point of the band lies within \\( D \\) of one of them (at
most 400). Stations at every solved point of the three wires and at most
2 m apart, rails at equal fractions of the angle so that a chord's sag
\\( R(1 - \cos\frac{\delta}{2}) \\) is at most a quarter of the
tolerance (the edges: one cell per metre of thickness, straight across).
In each cell the closed form is evaluated at the cell's middle and at
the middles of its four sides, and its distance \\( e \\) to the cell's
two triangles found; that cell's triangles get the radius
\\( D + 2 e_{\max} \\). **Conservative:** the largest distance found in a
cell, doubled (an estimate from five points of each cell; the exact bound
would need the surface's curvature between stations). Where the closed
form jumps — at a dead-end clamp the wire meets the axis and the angle of
a point on the axis is undefined — a few cells are grown by up to about
0.6 m (the example line); the median is about 2 cm.

Structures are not part of the volume (their steel, insulators and
fittings; an option may add them later): at a structure the volume is
the union of the sections' and the jumpers' tubes.

The wires taken are those of the electrical systems, circuits or spans
ticked (2026-10-21); a wire's system is that of its section. By span,
only the positions of the spans ticked are taken (so the volume ends
within the distance of their ends), and a jumper's only when the spans
at both its ends are ticked. Grouping
changes only which positions one mesh holds: each wire keeps the
distance \( d(U) \) of its own system's voltage, so a volume per circuit
or per system is the union of its wires' volumes (meshed as one).
The wires without a system — earthed wires and any section without
one — are one group since 2026-10-21 (the owner's decision,
`safety_volume::NO_SYSTEM`, "(no system)"): a row of the systems to
tick (`--system none`), and per system one volume of that name. They
keep the distance at 0 kV and are never given the distance of a system
they do not belong to. (Before, per system they made a volume per
circuit and a choice by system left them out.)

## The mesh

The volume is one closed triangle mesh per wire or per circuit: the
level \\( f = \varepsilon_0 \\) of

\\[ f(x) = \min_i \big(\operatorname{dist}(x, P_i) - r_i\big) \\]

(\\( P_i \\) the segments and arc triangles, \\( r_i \\) their radii) by
**marching tetrahedra** on a cubic grid of cell \\( h \\): each cube cut
into six tetrahedra about its diagonal, the same in every cube, so the
mesh is closed and every edge has exactly two triangles. A grid corner is
inside when \\( f \le \varepsilon_0 \\); each vertex is found on its edge
by bisection and **kept on the outer side** (\\( f \ge \varepsilon_0 \\)
there). The union is exact: no boolean of meshes is made, the distance to
the nearest primitive decides. \\( \varepsilon_0 \\) is 60 % of the
tolerance \\( \varepsilon \\) (the rest is the simplification's, below).

**The mesh holds the whole envelope.** With
\\[ h \le \sqrt{2 r_{\min} \varepsilon_0 + \varepsilon_0^2} \\]
(and \\( h\sqrt3 \le r_{\min} + \varepsilon_0 \\)): a triangle lies in one
cube, so in a circle of radius at most \\( h \\) (its sides are at most
the cube's diagonal); its corners are at least \\( r_i + \varepsilon_0 \\)
from every point \\( c \\) of every \\( P_i \\), so each of its points is
at least \\( \sqrt{(r_i + \varepsilon_0)^2 - h^2} \ge r_i \\) from
\\( c \\): no triangle enters the open ball \\( B(c, r_i) \\), and the
envelope is the union of these balls. The cube around any point of a
\\( P_i \\) has all its corners inside, so every part of the envelope is
on the mesh's inner side. Every radius carries 2 mm more (`ROUNDING`) so
that rounding the vertices in a file (to 0.1 mm in DXF and IFC, about
1 mm in KML) keeps this true.

The work is sparse: a block of cubes is skipped when \\( f \\) at its
centre is further than its half diagonal from the level (\\( f \\)
changes by at most the distance moved), so the cost follows the area of
the surface over \\( h^2 \\); blocks of 16³ cubes are meshed in parallel
and joined by their shared edges.

## Fewer triangles, only outward

The grid's mesh is fine everywhere (about a hundred thousand triangles for
100 m of a single tube). It is made coarser by **edge collapses**, one end
moved onto the other (no new vertex: every vertex stays one of the grid's,
outside the envelope), the flattest first (the area-weighted mean squared
distance to the planes merged, at most \\( (0.4\,\varepsilon)^2 \\)). A
collapse is kept only when

- the surface stays a closed manifold (the link condition; no triangle
  folded by more than 60° or doubled);
- \\( f \\) at each new triangle's centroid is at most \\( \varepsilon \\);
- **every new triangle stays out of the envelope**: its corners' balls
  prove it as above, else exact distances — triangle to segment and
  triangle to triangle — to every primitive within reach must be at least
  the primitive's radius.

It runs in parts of 24 m in parallel (their borders kept), twice, the
second time with the parts shifted by half. The result is then **checked
as a whole**: closed, no triangle within any primitive's radius, and one
point of every primitive inside the mesh (each connected part of the
envelope's inside holds whole primitives, so each part is inside). A
result failing the check is not used: the grid's mesh is kept.

**Tolerance.** The mesh is never inside the envelope. Outside, where the
envelope is smooth, by at most about \\( \varepsilon \\) (default
0.05 m) and 2 mm; **Conservative:** where tubes meet (a groove of the
union) the triangles bridge the groove, up to about one grid cell. The
window, the command line and the files give per volume the largest
\\( f \\) measured at every vertex, side middle and centroid (with the
2 mm): the "outside by at most" value, which includes the grooves. On top
of it, near arcs, each cell's own growth (above). Coarsening is only ever
outward.

## Checks by hand

- `a_straight_wire_gives_a_capsule` (validation `safetyvolume-001`): a
  straight level wire of radius 0.015 m, 100 m long, still air,
  \\( d = 3 \\) m: the envelope is a capsule of radius \\( D = 3.015 \\) m,
  volume \\( \pi D^2 L + \tfrac43 \pi D^3 = 2970.6 \\) m³. Cell 0.424 m,
  98 156 triangles from the grid, 3 094 after the simplification; closed;
  every sampled point of the exact surface inside; every vertex between
  \\( D \\) and \\( D + 0.032 \\) m from the wire; volume 3028.5 m³
  (+1.95 %, below the 3035.4 m³ of the capsule grown by the 0.032 m
  measured).
- `the_swing_arc_s_extremes_match_the_closed_form` (and validation
  `safetyvolume-003`, the same band on a 40 m span at a tolerance of
  0.1 m): a band about a level axis from −40° to +40°, blown at
  \\( R = 10 \\) m and hanging in still air at \\( r = 8 \\) m,
  \\( d = 2 \\) m and the band's margin: \\( D = 2.15 \\) m. At mid-span
  the volume reaches \\( R\sin 40° + D = 8.578 \\) m to each side,
  \\( R + D = 12.15 \\) m below the axis, and its top is
  \\( r\cos 40° - D = 3.978 \\) m below the axis (the band's inner end
  corners; the arc alone at \\( R \\): 5.660 m). The mesh's section there:
  8.632 m, 12.207 m and 3.930 m — never inside, outside by less than the
  measured allowance (0.064 m) and the band's own growth (0.025 m).
  `safetyvolume-002` keeps the arc of one radius (\\( r = R = 10 \\) m:
  8.578 m and 12.15 m with the margin).
- `each_wire_gets_the_distance_of_its_voltage`: two wires of 220 and
  380 kV, a table 7 m up to 220 kV and 9 m up to 380 kV (invented): their
  sections at mid-span lie 7.03–7.06 m and 9.03–9.06 m from them.
- `the_band_holds_the_steps_tubes` (the example line, high wind 600 Pa
  in steps of 100 Pa, both sides): every point \\( D = 2 \\) m from the
  centre line of each of the 110 stepped positions, in six directions,
  lies inside the band's envelope (with its margin): the worst 0.12 m
  inside. Until 2026-10-21 the arc at the largest radius alone missed
  them: a step's centre line lay up to 1.53 m inside the arc's radius
  (S1's second span, 400 Pa), its tube sticking out of the arc's volume
  towards the axis by that much. At 10 Pa on the example lines
  (`tests/arc_band.rs`): at most 0.089 m outside the band without its
  margin, none with it ([the swing arc](lidar-analysis.md#wire-positions-checked)).
- `the_jumper_closes_the_gap_at_a_dead_end` (and validation
  `safetyvolume-004`, `-005`): two straight sections end to end, their
  dead-end clamps 6 m apart either side of a structure, \\( d = 1 \\) m:
  alone their tubes leave a gap of \\( 6 - 2 = 4 \\) m (two pieces of
  mesh); with a jumper sagging 1.5 m between the clamps, one closed piece
  holding every point within 0.95 m of the jumper.
- `the_volume_s_positions_include_the_jumpers` (the example line, high
  wind, swing arc, a band margin of 0.4 m and a jumper band margin of
  0.25 m): the branch's jumper is taken blown left, blown right and on
  its own swing band (2026-10-21) with its own margin, every span's band
  with the spans'; the danger-tree check's positions leave jumpers out,
  as before.
- `chosen_and_grouped_by_system`: two earth wires of two circuits make
  one volume "(no system)" per system, and ticking "(no system)" takes
  them alone (2026-10-21).
- Validation `jumperband-001`, `-002`, `-003` (2026-10-21): a jumper's
  band about its end clamps blown to the left, by hand, with the
  jumpers' default margin 0 and a margin of 0.35 m entered
  (`benchmarks/clearance/danger-trees.toml`).

## The files

All in the line's coordinate system:

- **DXF** (R12): a `3DFACE` per triangle (fourth corner = third), a layer
  per volume, coordinates to 0.1 mm; a `.prj` beside it with the system's
  WKT (EPSG systems only).
- **IFC4**: a spatial element per volume (since 2026-10-21): an
  `IfcSpatialZone` (predefined type `USERDEFINED`, object type
  `SafetyVolume`; IFC4's spatial zone types are construction, fire
  safety, lighting, occupancy, security, thermal, transport and
  ventilation — none is a safety volume around a line, so user-defined),
  translucent, referenced in the site by an
  `IfcRelReferencedInSpatialStructure` — the relation IFC gives a spatial
  zone to the spatial structure (a zone is non-hierarchical and may
  overlap others, so it is referenced, not contained; the site is
  aggregated in the project) — shaped by a closed `IfcTriangulatedFaceSet`
  in local metres from a whole-metre origin, placed relative to the
  site; `IfcMapConversion` gives that
  origin's eastings, northings and height in an `IfcProjectedCRS` named by
  its EPSG code (scale 1, no rotation): local + origin = the line's
  coordinates. Property set `OHL_SafetyVolume`: line, circuit, phases,
  sections, voltage, distance, margin, rule, weather cases, wind, date,
  tolerance, volume, coordinate system.
- **KML/KMZ**: each volume a placemark of a `MultiGeometry` of triangles
  (polygons), `altitudeMode` absolute, longitude and latitude in WGS84
  through the line's system ([coordinate systems](../user-guide/coordinates.md)).
  **Heights are kept as the line has them** (above its height datum);
  Google Earth takes absolute altitudes as above its own geoid model. No
  geoid model is applied: the difference between the national height
  datum and that geoid (typically decimetres) is not corrected — an
  ellipsoidal height would instead be wrong by the whole geoid height
  (tens of metres). Positions round trip to the line's system within
  2 mm (tested). Polygons rather than a COLLADA model: Google Earth draws
  them at their absolute heights directly, without a model's local
  frame, and the coordinates stay readable.
