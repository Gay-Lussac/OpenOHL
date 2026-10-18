# Section library (`.olssec`)

A **section library** holds standard profiles — angles, channels, I and
H sections, tubes, round bars, pole shafts — and their materials. A
structure at [level 3](structure-library.md#detail-levels) refers to its
sections there by id, so a whole project shares one set of profiles
(see [How structures refer to it](#how-structures-refer-to-it)).
OpenOHL computes each profile's properties from its dimensions; values
you give replace the computed ones.

All profiles are **your data** (supplier tables, drawings). OpenOHL ships
only an invented example, `libraries/sections/example-sections.olssec`,
with one profile of each shape — its dimensions are made up.

The file is TOML with units on every quantity, like the other libraries;
an unknown key is an error that names it.

```toml
format = "ols-sections"          # optional header
version = 1
name = "Project sections"
description = "Angles and tubes of the tower family"
default_material = "steel-a"     # for sections naming no material

[[material]]
id = "steel-a"                   # letters, digits, - _ .
name = "Steel A"
elasticity = "210 GPa"           # E
shear_modulus = "81 GPa"         # G (default E / 2.6)
density = "7850 kg/m3"           # default 0
yield = "355 MPa"                # fy, optional
ultimate = "490 MPa"             # fu, optional
partial_factors = { gamma_m0 = 1.0, gamma_m1 = 1.1 }   # optional, by name

[[section]]
id = "L-61x7"
name = "Angle 61 × 7"
description = ""
shape = "equal_angle"
material = "steel-a"             # optional: default_material
h = "61 mm"
t = "7 mm"
r = "8 mm"                       # root radius (default 0)
```

Partial factors are kept by name for the code checks (plugins) that will
use them; OpenOHL itself does not read them.

## Shapes

Section axes: **y** along the width `b`, **z** along the height `h`.
`iy` is about y — the strong axis of an I section. Positions (the
centroid) are measured from the lower left of the profile's outline box:
the heel of an angle, the back of a channel's web at its bottom flange.

| `shape` | Dimensions | Drawn as |
|---|---|---|
| `equal_angle` | `h`, `t`, `r` | legs `h` along y and z from the heel |
| `unequal_angle` | `h` (along z), `b` (along y), `t`, `r` | the same |
| `channel` | `h`, `b`, `tw`, `tf`, `r` | web along z, flanges towards +y, parallel flanges |
| `i_section` | `h`, `b`, `tw`, `tf`, `r` | I and H sections, parallel flanges |
| `rectangular_hollow` | `h`, `b`, `t` | sharp corners |
| `circular_hollow` | `d` (outside), `t` | tube |
| `round_bar` | `d` | solid |
| `polygonal_hollow` | `sides`, `d` (across flats), `t` | a pole's shaft, a side at the bottom |
| `general` | `h`, `b` (outline box) | nothing computed: give `area`, `iy`, `iz` (and what else is needed) |

`r` (root radius, the fillets between the plates) may be left out (0). A
dimension a shape does not take is an error ("b is not a dimension of
equal_angle").

## Properties

Computed for every shape (`Section::properties`):

| Property | Key to give it | Unit | |
|---|---|---|---|
| area A | `area` | `mm2`, `cm2` | |
| second moments about y, z | `iy`, `iz` | `cm4`, `mm4`, `in4` | about the centroid |
| product ∫yz dA, principal values and angle | `iu`, `iv` | `cm4` | angles: u major, v minor |
| torsion constant | `it` | `cm4` | see below |
| elastic moduli about y, z (and u, v) | `wel_y`, `wel_z` | `cm3`, `mm3`, `in3` | second moment over the farthest fibre |
| plastic moduli about y, z | `wpl_y`, `wpl_z` | `cm3` | about the axis halving the area |
| radii of gyration about y, z, u, v | `gyration_y`, `gyration_z`, `gyration_u`, `gyration_v` | `mm` | √(I / A) |
| shear areas along y, z | `shear_area_y`, `shear_area_z` | `mm2` | see below |

A given value replaces the computed one; radii of gyration you do not
give follow from the final area and second moments, principal values you
do not give from the final `iy`, `iz`.

What is exact and what is approximate:

- **Area, centroid, second moments, principal axes, elastic and plastic
  moduli** are exact for the drawn shape, root radii included. Not drawn:
  the toe radii of angles and flanges, sloped flanges, the corner radii
  of rectangular tubes (sharp corners). For a profile whose table values
  include those, give the table's values.
- **Torsion constant**: exact for tubes and round bars; Bredt's formula
  on the mid-wall line for rectangular and polygonal tubes; the sum of
  b·t³/3 over the plates for angles, channels and I sections (fillets
  left out).
- **Shear areas**: the plates parallel to the shear for angles, channels,
  I sections (web between the flanges; both flanges) and rectangular
  tubes (in proportion to the sides); half the area for round and
  polygonal tubes; 0.9 A for a round bar.
- A `general` section has only what you give; its elastic and plastic
  moduli and torsion constant stay unknown unless given.

## Wind on the profile

The body wind of a level 3 structure is computed from the surface of its
profiles (the [member wind](structure-analysis.md#member-wind-body-wind-at-level-3)
of the finite-element check). `Section::wind_width(direction)` gives the profile's **width
seen by the wind**: the extent of its outline normal to a wind blowing
in the section's plane at `direction` (from y towards z). Per metre of
member, the projected area is that width × 1 m
(`wind_area_per_length`).

Approximation: the silhouette of the whole outline. An angle's two legs
do not shield each other (at 45° the width is the diagonal extent of the
legs), a tube's width is its diameter in every direction, and no drag
factor is applied here — the structure model's `drag` is the one factor
the member wind applies; shielding and the solidity rules of a lattice
face from a code are a later step. A `general` section's width is that of
its outline box.

## Several files

A project and a user may each keep a section library. They are read in
order — the project's first, then the user's — and a section or material
whose id is already there is left out (the earlier one wins). In one file
an id given twice is an error; a folder of `.olssec` files is read in
name order, an id in two of them an error. A material a section names
may be in another file read together; a material missing from all of
them is reported.

## How structures refer to it

A member of a structure (in its type's `.olss`, or a separate `.olsfe`
model) names its section by id,

```toml
[[member]]
id = "leg1"
nodes = ["B1", "A1"]
section = "L-61x7"
```

and the id is looked up **in the project's section library first, then in
the structure's own `[[section]]` tables** (read by the finite-element
check, [Structure analysis](structure-analysis.md#structure-models-olsfe)):
the profile's properties and its material's E, G, density and yield. An
id in neither is refused, naming it. A structure file alone does not
check its ids against a library: the structure model does, when it is
built.

In a project the section library is stored like the other library kinds
of the project file, per project line (kind `sections`), so every
structure of the line uses the same profiles: when the project opens
without one, the `.olssec` files of the `Sections/` folder next to it
(else those next to it) are copied in — never the shipped example — and
exported with the other libraries (see [Library
editors](desktop-app.md#library-editors)). For a line file the `Sections/`
folder (else the files next to it) is read directly. `ohl line` and `ohl
structures fe` take `--sections <file|folder>`; `ohl line` reads the
`Sections/` folder next to the line by default.

Member groups (one section for many members) come with the structure
application; a group's `section` will resolve the same way.
