# Wind on lattice structures: faces, solidity and shielding

Implemented in `crates/ohl-structures-fe/src/wind.rs` (`faces`,
`MemberWindSetup`, `Model::member_wind_with`), with the factors read by
`crates/ohl-sections/src/wind.rs` (`StructureWind`, the `[wind]` table of
the cross-section library). Built, in active development (not approved yet);
pre-alpha: results must be verified by a qualified engineer.

**What is OpenOHL's and what is data.** The geometry is computed here:
which members form a face of the lattice, each face's outline and
solidity ratio, and, for a wind direction, which face stands in front of
which member. The **factors** of a design code — the drag factor of a
profile shape, the drag factor of a lattice face as a function of its
solidity ratio, the shielding factor of a face behind another as a
function of the front face's solidity ratio and the spacing ratio — are
**data**: a design-code plugin returns them from the user's parameter
file (the optional `structure-wind` export, see [Writing
plugins](../development/plugins.md#wind-factors-for-structures)), or the
user types them into a `[wind]` table of the cross-section library ([Cross-section
library](../user-guide/section-library.md#wind-factors-for-structures-wind)).
OpenOHL ships no such values. Without them the member wind is the one of
[Structure analysis](structures-fe.md#member-wind): one generic drag
factor, nothing shielded — and the results say so.

**Conservative:** without shielding factors nothing is shielded: every
member takes its full wind, as if it stood alone. Exact: the code's
shielding factors as data. (The generic drag factor 1 is a
**Simplification**, see [Member wind](structures-fe.md#member-wind).)

## Member force

For a member with a profile from the cross-section library, the wind of
pressure \\( q \\) blowing along the unit vector \\( \hat{\mathbf w} \\)
gives, with the member's axis \\( \hat{\mathbf x} \\) and the wind's
component normal to it
\\( \mathbf w_n = \hat{\mathbf w} - (\hat{\mathbf w}\cdot\hat{\mathbf x})\hat{\mathbf x} \\)
(length \\( \sin\theta \\)),

\\[ \mathbf F = q\, c\, \eta\, d\, L \sin\theta \; \mathbf w_n \qquad (|\mathbf F| = q\, c\, \eta\, d\, L \sin^2\theta), \\]

\\( d \\) the profile's outline width across \\( \mathbf w_n \\) in the
member's local y–z plane, \\( L \\) its length, half of \\( \mathbf F \\)
at each joint. Two factors:

- **drag** \\( c \\): the face drag \\( c_f(\varphi) \\) of the member's
  face (when the data give a face drag and the member lies in a face
  facing the wind: the one facing it most, \\( \max |\hat{\mathbf n}\cdot\hat{\mathbf w}| \\)),
  else the drag of the profile's shape (`drag.<shape>`), else the
  model's generic `drag` (default 1). A face whose solidity is outside the
  face drag table falls back the same way, and the notes count it.

  **Simplification:** that fallback. The shape's drag or the generic 1 can
  be lower than the face drag the table would give at that solidity
  (unsafe); the notes say how many members it concerns. Exact: a face
  drag table covering the solidity ratios of the structure.
- **shielding** \\( \eta \\): 1, unless the member stands behind another
  face (below), then the data's shielding factor
  \\( \eta(\varphi_{front}, e/b) \\).

![A member at an angle theta to the wind: the wind's unit vector, the member's axis, the wind's part normal to the member and the force along it, half at each joint](../images/theory/member-wind.svg)

*The cross-flow force on a member: along the wind's part normal to it, proportional to \\( \sin^2\theta \\).*

## Faces

A **face** is a set of at least three members lying in one plane,
joined at their joints, with the rest of the structure on one side of
that plane:

1. Every pair of members meeting at a joint and not parallel (the sine of
   their angle above 0.0175, about 1°) defines a plane; planes found
   twice (normal and offset rounded) are taken once.
2. Per plane, the members with both ends within the tolerance (1 mm plus
   a millionth of the model's size) of it, split into sets joined at
   joints; sets of fewer than three members are dropped.
3. **Outer faces only**: every member that leaves a joint of the set out
   of the plane must go to the same side of it. This keeps the faces of a
   body or a crossarm and drops planes through the middle of a body (a
   plan bracing's diagonal with two legs, the horizontal plane of a
   panel's joints, which has legs above and below).
4. A set inside a larger one (the same plane found from a short pair at a
   slightly different angle) is dropped.

The **outline** of a face is the convex hull of its joints in its plane;
its area \\( A_o \\). The **solidity ratio**

\\[ \varphi = \frac{\sum_{m \in face} d_m L_m \sin\theta_m}{A_o} \\]

sums the members' silhouettes seen along the face's normal (the profile's
width for that direction × length; members without a profile count 0).
The face's **breadth** \\( b = A_o / h \\), \\( h \\) the outline's
extent along the global z axis taken into the plane (for a horizontal
face, along an in-plane axis).

![A lattice face with its convex-hull outline of area A_o, its extent h and a member's silhouette; beside it a waisted face whose hull is larger than the face](../images/theory/face-solidity.svg)

*A face's outline, breadth and solidity ratio; for a face that is not convex the hull is larger, which lowers \\( \varphi \\) (schematic).*

Approximations: the convex hull overestimates the outline of a face that
is not convex (a waisted body, the window of a cat-head tower), which
lowers \\( \varphi \\); members overlapping at the joints are counted
twice and gusset plates not at all; a crossarm whose face is not flush
with the body's is a face of its own.

**Conservative:** the convex-hull outline, which lowers the solidity ratio
of a face that is not convex. A lower \\( \varphi_{front} \\) shelters
less (a shielding factor nearer 1), on the safe side; for the face drag it
is on the safe side where the data's face drag falls as the solidity grows,
and not where it rises. Exact: the face's own outline (a polygon round its
members).

**Simplification:** members overlapping at the joints counted twice
(raises \\( \varphi \\): more shelter behind the face, unsafe by a
little) and gusset plates left out (their own wind area missing, unsafe;
\\( \varphi \\) a little low, safe for the shielding). Exact: the
face's true silhouette, overlaps once and plates included.

## Which members are shielded

For a wind direction \\( \hat{\mathbf w} \\), a face **faces the wind**
when \\( |\hat{\mathbf n}\cdot\hat{\mathbf w}| > 10^{-3} \\) (an edge-on
face — a side of a square lattice tower under a wind along its other sides —
neither shelters nor is sheltered). A member that lies in a face facing
the wind is **shielded** when the line from its middle back against the
wind meets another face facing the wind (one the member is not in),
inside that face's outline (within the tolerance), at a distance above
the tolerance; the nearest such face is the one **in front**. Its
solidity ratio \\( \varphi_{front} \\) and the **spacing ratio**
\\( e/b \\) — \\( e \\) the member's distance from the front face's plane
along its normal, \\( b \\) the front face's breadth — give
\\( \eta = \eta(\varphi_{front}, e/b) \\), interpolated linearly in the
data's table (bilinear with a spacing axis). Outside the table's range
nothing is shielded (\\( \eta = 1 \\)) and the notes say how many members
that left out.

**Conservative:** no factor outside the table's range: such a member takes
its full wind (\\( \eta = 1 \\)) instead of an extrapolated shielding
factor. Exact: a table covering the structure's solidity and spacing
ratios.

**Conservative:** a member behind several faces takes the factor of the
nearest one only, not the shelter of all of them. Exact: the code's rule
for several faces in a row, as data.

**Simplification:** a member is shielded, or not, as a whole, by the ray
from its middle. A member partly behind the front face's outline takes
the factor all along or not at all (either way). Exact: the shielded share
of its length.

So, for a square lattice body under a wind normal to two of its faces,
the far face is shielded by the near one and the two side faces (edge-on)
are not; at 45°, the two far faces are shielded, the far corner leg too
(it lies only in far faces), the other legs not (each lies in a near
face). Members in no face (plan bracing, hip bracing) are never shielded.

![Plan of a square lattice body: with the wind normal to two faces the far face is shielded and the side faces are edge-on; at 45 degrees the two far faces and the far corner leg are shielded](../images/theory/shielding-plan.svg)

*Shielding in plan: the far faces behind a face facing the wind, the side faces edge-on (schematic).*

**Conservative:** members inside the body (in no face: plan and hip
bracing, inner diagonals) are never shielded and take their full wind,
where a code would also shelter them behind the windward face. Exact: the
code's rule for inner members, as data.

A code that gives one force coefficient for a whole lattice cross-section
(windward and leeward faces together) is expressed as a face drag by
solidity with a shielding factor of 0: the near faces carry the whole
force, the far ones none.

## Where it applies

- The finite-element check of a level 3 structure in the line
  application (Checks › Compute structure loads, its Usage page; `ohl line`): the wind of the
  load case across the line, the factors of the project line's section
  library; the faces are found once per model. The usage notes say what
  was applied: *member wind: drag factors from …: 12 member(s) by their
  face's solidity, 30 by their shape, 0 the generic 1* and *shielding
  from …: 18 member(s) behind another face of 9 face(s) found, factors
  0.82–0.88*, or *drag factor 1 on every profile (the model's generic
  drag: no code drag factors given); no shielding (no shielding factors
  given)*.
- `ohl structures fe --wind … --sections …`: the same, the notes printed.
- Not (yet) the surface loads of OHL-Structure's load recipes (a
  `pressure` on members takes the members' projected areas without
  factors, as before).

  **Conservative:** that `pressure` acts along its direction with the
  area width × length × \\( \sin\theta \\) — more than the cross-flow
  force above (\\( \sin^2\theta \\)) for a member at an angle to the
  wind — and nothing is shielded. **Simplification:** no drag factor (1),
  which can be lower than a code's (unsafe); a member whose cross-section is not
  in the library has no area (said in its row). Exact: the member wind
  above, with the code's factors.
- OHL-Structure's **wind on the body** of a load case
  (`[[load_case.structure.wind]]`, 2026-10-25): this member wind, the
  same factors and faces, for the case's direction (left, right, ahead,
  back or 45° between them) and pressure × factor; a row per member.
  With ice given as a radial thickness \\( t \\) in the same case, each
  iced member's width \\( d \\) becomes \\( d + 2t \\).
  **Conservative:** that widening — the iced outline is taken as the
  bare outline grown by \\( t \\) on both sides, its corners not rounded,
  an angle's open side as if filled. **Simplification:** the faces'
  solidity ratios and the shielding stay those of the bare members.

## Direction, ice and snow on the structure (2026-11-03)

Implemented in `crates/ohl-loads/src/options.rs` (`WindDirection`,
`StructureOptions`, `IceOn`, `SnowOn`, `snow_load`),
`crates/ohl-loads/src/analysis.rs` (`body_weights`, `pole_ice`, the
**fe** and **pole** checks) and `crates/ohl-loads/src/body.rs`
(`add_structure_loads`), from a load case's `[case.structure]` table
(file version 4, [Load cases](../user-guide/loads.md#version-4-removed-conductors-several-broken-wires-the-winds-direction-ice-and-snow-on-the-structures)).
Built, in active development (not approved yet).

### The wind's direction

The structure's axes: \\( x \\) to the right of the line looking ahead,
\\( y \\) ahead, \\( z \\) up. A load case gives where the wind on the
structure bodies comes **from** as an angle \\( \theta \\): 0° from the
right, counted towards ahead (90° from ahead, 180° from the left, 270°
from behind); the names `right`, `ahead`, `left`, `back` are 0°, 90°,
180°, 270°, as OHL-Structure's wind on the body. It blows towards

\\[ \hat{\mathbf w} = (-\cos\theta,\; -\sin\theta,\; 0). \\]

Without a direction the wind blows across the line from the side of the
wind on the wires, as before. The wires keep that side: the direction is
the structure bodies' only.

- **Member wind (fe check):** the force of [Member force](#member-force)
  for \\( \hat{\mathbf w} \\), faces and shielding found for it — exact
  within that model.
- **Pole:** the pressure as a vector \\( q\,\hat{\mathbf w} \\) on the
  shaft (the pole is round or polygonal: its area is the same from every
  side; **Simplification:** a polygon's drag taken the same from every
  side).
- **Wind bands:** a band holds one area \\( A \\), the one a wind across
  the line meets, and a drag \\( c \\). A wind from another direction
  takes the same area: \\( \mathbf F = q\,c\,A\,\hat{\mathbf w} \\), so a
  wind from ahead gives a longitudinal load of \\( q c A \\).
  **Simplification, not on the safe side:** a structure usually shows
  more area along the line (its crossarms seen in full, the body's other
  face) — the longitudinal load is then too low; and at an angle the
  force is taken along the wind, not by the components of each face.
  Exact: a band area per direction (not in the type format yet), or the
  member wind of a level 3 structure.

### Ice on the structure

A radial thickness \\( t \\) (the case's, or each structure's local
`structure_ice`) of density \\( \rho \\) (the case's `ice_density`, else
the weather case's; without either the case is refused — no density is
assumed).

- **Members (fe check):** on every member with a profile from the
  cross-section library, the ice all round its outline of perimeter
  \\( P \\) (the outline's outer edge: an angle's inside faces too):
  \\[ w = \rho\, g\, (P\,t + \pi t^2) \quad \text{per metre}, \\]
  half of each member's at each end, down, times the load case's
  vertical factor; the member \\( 2t \\) wider for the member wind, as
  OHL-Structure's ice in a load recipe.
  **Conservative:** the ice all round the outline, inside an angle's legs
  too, and the iced outline not rounded (owner 2026-10-25, the rule of
  OHL-Structure).
  **Simplification, not on the safe side:** members without a profile
  (sections given by their values only) take no ice, and gusset plates
  and bolts none (the notes count the members left out). Exact: every
  member's outline, the plates.
- **Pole:** the shaft above the ground, cut in 8 pieces of equal height,
  each \\( \rho\, g\, (\pi D t + \pi t^2) \\) per metre with \\( D \\) its
  diameter at the piece's middle, as vertical loads on the axis; the wind
  on the iced pole is the pressure times \\( (D_{top} + 2t)/D_{top} \\).
  **Conservative:** that factor, the largest ratio along the pole (at the
  top, the smallest diameter), for the whole shaft.
  **Simplification, not on the safe side:** a polygonal shaft's perimeter
  taken as \\( \pi D \\) (a polygon's is larger by its corners).
- **Wind bands only (level 1 and 2 types):** no surface to put the ice
  on: none counted, the notes name the structures. **Not on the safe
  side.** Exact: a type with members or a pole.

### Snow on the structure

The snow load on a horizontal surface by SIA 261:2014 (9):

\\[ q_k = \mu\, C_e\, C_T\, s_k, \\]

\\( s_k \\) the snow on the ground at the structure (its local
`snow_ground`, or by SIA 261 (10) from its reference altitude
`snow_altitude`, see [Finite-element wire systems](wirefe.md)); none where
neither is given (said). Defaults: \\( \mu = 0.8 \\) (SIA 261 5.3.4, the
coefficient where the snow cannot slide off), \\( C_e = 1.0 \\) (5.2.4,
normal exposure), \\( C_T = 1.0 \\) (5.2.5); each may be given.

- **Members (fe check):** on every member with a profile, \\( q_k \\)
  times its plan — the profile's width seen from above times the member's
  length projected on the ground (\\( d\,L\,\sin\varphi \\), \\( \varphi \\)
  its angle to the vertical: a leg none, a horizontal its full length) —
  half at each end, down, times the vertical factor.
  **Assumption:** μ = 0.8 is the standard's value for roofs where the snow
  cannot slide; a pylon is not a roof — taken for its horizontal
  surfaces, the owner's choice, to be confirmed by the engineer.
  **Conservative:** the snow on every member's plan, the inclined ones too
  (snow slides off a steep member), overlapping members counted twice.
  **Simplification, not on the safe side:** snow bridging between members
  (a crossarm's plan bracing filled, a platform) not counted, nor on
  plates; members without a profile none.
- **Pole:** none (no horizontal surface but its top; said).
- **Wind bands only:** none (no plan area; said, not on the safe side).

### Where these loads go

The **fe** and **pole** checks take them on their members and shaft. The
loads listed per structure ("ice on the structure", "snow on the
structure": the same weights, down, at their centroid on the structure's
axis) are data for the reports; the allowable loads at the attachments
(Method 2, load trees) take no load without a point, as the wind bands:
**Simplification:** the type's allowable loads are taken to allow for its
own body's ice and snow (unsafe where they do not). Exact: the
fe or pole check.

## Validation

`benchmarks/structures-fe/member_wind.toml`: an invented square lattice
panel with invented factors (drag 1.2 for round bars, η = 1 − φ):
the faces found (four sides and the top), the solidity ratio
\\( \varphi = 0.05 \cdot (10 + 2\sqrt{20}) / 8 = 0.1184 \\) by hand, the
far face shielded, the force along the wind by hand; and the same panel
without factors (today's generic drag, nothing shielded). Unit tests
(`wind_tests.rs`) add a two-panel lattice tower with a plan diagonal (the plane
through the body dropped), the face drag by solidity, a spacing axis,
the table's range, and the members shielded at 45°.
