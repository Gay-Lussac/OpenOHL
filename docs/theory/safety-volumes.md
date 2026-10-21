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
  the **swing arc** (the surface swept by the span swinging rigidly about
  its attachment axis between the two blowouts, at the largest of the
  three radii) — the positions of the danger-tree check
  ([LiDAR analyses › Wire positions checked](lidar-analysis.md#wire-positions-checked)).
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
cylinder and two half balls) of radius \\( D_k \\) along its polyline.

The **swing arc** contributes every point within \\( D_k \\) of the arc
surface: at a place \\( s \\) along the span's axis the circle section of
centre \\( C(s) \\), radius \\( R(s) \\) between \\( \varphi_L(s) \\) and
\\( \varphi_R(s) \\) — the closed form of the danger-tree page, with its
**Conservative** labels (one radius, the largest; the angles from the
smallest to the largest) and its **Simplification** (a rigid swing).
Distances are measured in 3D, so beyond the arc's ends and beyond the
span's ends the volume is rounded (balls of radius \\( D \\)).

As built, the arc surface is cut into triangles: stations at every solved
point of the three wires and at most 2 m apart, rails at equal fractions
of the angle so that a chord's sag \\( R(1 - \cos\frac{\delta}{2}) \\) is
at most a quarter of the tolerance. In each cell between two stations
and two rails the closed form is evaluated at the cell's middle and at
the middles of its four sides, and its distance \\( e \\) to the cell's
two triangles found; that cell's triangles get the radius
\\( D + 2 e_{\max} \\). **Conservative:** the largest distance found in a
cell, doubled (an estimate from five points of each cell; the exact bound
would need the surface's curvature between stations). Where the closed
form jumps — at a dead-end clamp the wire meets the axis and the angle of
a point on the axis is undefined — a few cells are grown by up to about
0.6 m (the example line); the median is about 2 cm.

Structures and jumpers are not part of the volume (an option may add them
later).

The wires taken are those of the electrical systems, circuits or spans
ticked (2026-10-21); a wire's system is that of its section. By span,
only the positions of the spans ticked are taken (so the volume ends
within the distance of their ends), and a jumper's only when the spans
at both its ends are ticked. Grouping
changes only which positions one mesh holds: each wire keeps the
distance \( d(U) \) of its own system's voltage, so a volume per circuit
or per system is the union of its wires' volumes (meshed as one).
Earthed wires have no system: per system they make a volume per
circuit, and a choice by system leaves them out (tick their circuits to
include them); they keep the distance at 0 kV and are never given the
distance of a system they do not belong to. A choice made while the
owner has not ruled on it (an open question).

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
  `safetyvolume-002`, the same arc on a 40 m span at a tolerance of
  0.1 m): an arc of \\( R = 10 \\) m about a level axis from
  −40° to +40°, \\( d = 2 \\) m: at mid-span the volume reaches
  \\( R\sin 40° + d = 8.428 \\) m to each side, \\( R + d = 12 \\) m below
  the axis and up to \\( R\cos 40° - d = 5.660 \\) m below it. The mesh's
  section there: 8.483 m, 12.052 m and 5.604 m — never inside, outside by
  less than the measured allowance (0.061 m) and the arc's own growth
  (0.025 m).
- `each_wire_gets_the_distance_of_its_voltage`: two wires of 220 and
  380 kV, a table 7 m up to 220 kV and 9 m up to 380 kV (invented): their
  sections at mid-span lie 7.03–7.06 m and 9.03–9.06 m from them.
- `the_arc_holds_the_steps_up_to_the_rigid_swing` (the example line, high
  wind 600 Pa in steps of 100 Pa): of 110 stepped positions the farthest
  centre-line point lies 1.53 m off the arc's surface, **inside** it
  (1.53 m nearer the axis than the arc's radius: S1's second span, wind
  from the right at 400 Pa). The arc at the largest radius covers the
  outer side; a step whose wire hangs at a smaller radius (its strings
  and clamps swinging, its sag at that pressure) sticks out of the arc's
  volume towards the axis by that much. With "both", the union holds both.

## The files

All in the line's coordinate system:

- **DXF** (R12): a `3DFACE` per triangle (fourth corner = third), a layer
  per volume, coordinates to 0.1 mm; a `.prj` beside it with the system's
  WKT (EPSG systems only).
- **IFC4**: an `IfcBuildingElementProxy` per volume (object type
  `SafetyVolume`, translucent), shaped by a closed `IfcTriangulatedFaceSet`
  in local metres from a whole-metre origin; `IfcMapConversion` gives that
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
