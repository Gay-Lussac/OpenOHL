# Wind on lattice structures: faces, solidity and shielding

Implemented in `crates/ohl-structures-fe/src/wind.rs` (`faces`,
`MemberWindSetup`, `Model::member_wind_with`), with the factors read by
`crates/ohl-sections/src/wind.rs` (`StructureWind`, the `[wind]` table of
the section library). Built, in active development (not approved yet);
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
user types them into a `[wind]` table of the section library ([Section
library](../user-guide/section-library.md#wind-factors-for-structures-wind)).
OpenOHL ships no such values. Without them the member wind is the one of
[Structure analysis](structures-fe.md#member-wind): one generic drag
factor, nothing shielded — and the results say so.

## Member force

For a member with a profile from the section library, the wind of
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
- **shielding** \\( \eta \\): 1, unless the member stands behind another
  face (below), then the data's shielding factor
  \\( \eta(\varphi_{front}, e/b) \\).

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

Approximations: the convex hull overestimates the outline of a face that
is not convex (a waisted body, the window of a cat-head tower), which
lowers \\( \varphi \\); members overlapping at the joints are counted
twice and gusset plates not at all; a crossarm whose face is not flush
with the body's is a face of its own.

## Which members are shielded

For a wind direction \\( \hat{\mathbf w} \\), a face **faces the wind**
when \\( |\hat{\mathbf n}\cdot\hat{\mathbf w}| > 10^{-3} \\) (an edge-on
face — a side of a square tower under a wind along its other sides —
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

So, for a square lattice body under a wind normal to two of its faces,
the far face is shielded by the near one and the two side faces (edge-on)
are not; at 45°, the two far faces are shielded, the far corner leg too
(it lies only in far faces), the other legs not (each lies in a near
face). Members in no face (plan bracing, hip bracing) are never shielded.
This is conservative where a code would also shelter members inside the
body.

A code that gives one force coefficient for a whole lattice section
(windward and leeward faces together) is expressed as a face drag by
solidity with a shielding factor of 0: the near faces carry the whole
force, the far ones none.

## Where it applies

- The finite-element check of a level 3 structure in the line
  application (Checks › Structure usage, `ohl line`): the wind of the
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

## Validation

`benchmarks/structures-fe/member_wind.toml`: an invented square lattice
panel with invented factors (drag 1.2 for round bars, η = 1 − φ):
the faces found (four sides and the top), the solidity ratio
\\( \varphi = 0.05 \cdot (10 + 2\sqrt{20}) / 8 = 0.1184 \\) by hand, the
far face shielded, the force along the wind by hand; and the same panel
without factors (today's generic drag, nothing shielded). Unit tests
(`wind_tests.rs`) add a two-panel tower with a plan diagonal (the plane
through the body dropped), the face drag by solidity, a spacing axis,
the table's range, and the members shielded at 45°.
