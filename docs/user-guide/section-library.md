# Cross-section library (`.ohlsec`)

A **cross-section library** holds standard profiles — angles, channels, I and
H cross-sections, tubes, round bars, pole shafts — and their materials. A
structure at [level 3](structure-library.md#detail-levels) refers to its
cross-sections there by id, so every structure of a project line shares one
set of profiles
(see [How structures refer to it](#how-structures-refer-to-it)).
OpenOHL computes each profile's properties from its dimensions; values
you give replace the computed ones.

All profiles are **your data** (supplier tables, drawings). OpenOHL ships
only an invented example, `libraries/sections/example-sections.ohlsec`,
with one profile of each shape — its dimensions are made up.

The file is TOML with units on every quantity, like the other libraries;
an unknown key is an error that names it.

```toml
format = "ohl-sections"          # optional header
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

Cross-section axes: **y** along the width `b`, **z** along the height `h`.
`iy` is about y — the strong axis of an I-section. Positions (the
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
  b·t³/3 over the plates for angles, channels and I-sections (fillets
  left out).
- **Shear areas**: the plates parallel to the shear for angles, channels,
  I-sections (web between the flanges; both flanges) and rectangular
  tubes (in proportion to the sides); half the area for round and
  polygonal tubes; 0.9 A for a round bar.
- A `general` cross-section has only what you give; its elastic and plastic
  moduli and torsion constant stay unknown unless given.

## Wind on the profile

The body wind of a level 3 structure is computed from the surface of its
profiles (the [member wind](structure-analysis.md#member-wind-body-wind-at-level-3)
of the finite-element check). `Section::wind_width(direction)` gives the profile's **width
seen by the wind**: the extent of its outline normal to a wind blowing
in the cross-section's plane at `direction` (from y towards z). Per metre of
member, the projected area is that width × 1 m
(`wind_area_per_length`).

Approximation: the silhouette of the whole outline. An angle's two legs
do not shield each other (at 45° the width is the diagonal extent of the
legs), a tube's width is its diameter in every direction, and no drag
factor is applied here — the member wind applies the drag factor: the
structure model's `drag`, or the library's wind factors below. A
`general` cross-section's width is that of its outline box.

## Wind factors for structures (`[wind]`)

A library file may hold one `[wind]` table: the **drag and shielding
factors** of a design code for the member wind of lattice structures. They
are **data** — from a design-code plugin (its `structure-wind`, written as
`<plugin>-wind.ohlsec` into the cross-section library, see [Design
codes](design-codes.md#wind-factors-for-structures)) or typed from your
standard. OpenOHL ships none.

```toml
[wind]
source = "my code, parameter file of 2026-10"   # shown with the results
drag = { equal_angle = 2.0, circular_hollow = 1.2 }   # per shape (invented)

[wind.face_drag]                  # a lattice face's drag by its solidity ratio
solidity = [0.1, 0.3, 0.5]        # rising, 0–1
factor = [3.0, 2.6, 2.2]          # invented

[wind.shielding]                  # factor on the members of a face behind another
solidity = [0.1, 0.3, 0.5]        # of the face in front, rising, 0–1
spacing = [0.5, 2.0]              # optional: distance between the faces / breadth
factor = [[0.9, 0.7, 0.5], [1.0, 0.8, 0.6]]   # a row per spacing (one flat row without)
```

Every part is optional. `drag` keys are the shapes of the table above.
Values between the table's points are interpolated linearly (bilinear with
`spacing`); **outside the table's range nothing is taken**: a face's
solidity outside `face_drag` falls back to the shape's drag, a member
behind a face outside `shielding` is not shielded, and the notes count
them. Shielding factors are 0–1 (0: a code that puts the whole force on
the near faces, with a face drag for the whole cross-section). A folder holds one
`[wind]` table at most: a second one in another file is an error naming
it.

What the member wind does with them — the faces of the lattice found from
the geometry, their solidity ratios, which face is in front of which
member — is in [Wind on lattice structures](../theory/structure-wind.md).
Without a `[wind]` table the member wind keeps one generic drag factor and
no shielding, and the results say so.

## Typing cross-sections in OHL-Structure

**Design › Sections & materials…**, tab **Cross-section library**, lists the
cross-sections of the library the structure reads (`--sections`, else
`Sections/` beside the structure file; started from the line application,
the project line's library, which OHL-Design keeps in the project when a
file there changes) with their shape, material, area, file and how many
members of the structure use them; one line says which wind factors the
library has.

- **+ New cross-section** opens the form: id, name, description, **shape** (the
  shapes above), **material** (the library's materials, or the file's
  `default_material`) and the shape's dimensions typed with their units
  (`61 mm`; `sides` a number); a `general` cross-section takes its area and
  second moments. The **properties** are computed as you type — area,
  second moments about y, z and the principal axes, torsion constant,
  elastic and plastic moduli, radii of gyration, centroid, and the weight
  per metre (area × the material's density) — by the cross-section library
  itself, so they are the values the checks use. What is wrong is said in
  the library's words (a missing dimension, plates that do not fit, a
  length without its unit, an id another file of the library has); a
  material the library lacks is a warning. **Save** writes the cross-section
  into its file (new cross-sections go into the file chosen, the first by
  default; with no file yet, `sections.ohlsec` is made).
- **✏** edits a cross-section in place (its other keys — properties given,
  comments — kept; dimensions the new shape does not take removed),
  **copy** duplicates it (a free id, `L-61x7-copy`), **🗑** deletes it after
  asking (saying how many members of the structure use it).
- Each change is written at once and is **one step of Edit › Undo**, in
  order with the structure file's edits (a change made outside since is
  not undone, and said).
- Members then pick the cross-section in **Design › Member cross-sections** and
  **Groups**: their cross-section lists hold the library's cross-sections first, then
  the file's own `[[section]]` tables.

Materials are not typed here: add a `[[material]]` to a library file.

## Several files

The applications and `ohl` read one cross-section library: the project
line's, or the file or folder given. Libraries can be merged in code
(`SectionLibrary::merge`: a cross-section or material whose id is already
there is left out, the earlier one wins); a user's own cross-section library
beside the project's is not read yet. In one file
an id given twice is an error; a folder of `.ohlsec` files is read in
name order, an id in two of them an error. A material a cross-section names
may be in another file read together; a material missing from all of
them is reported.

## How structures refer to it

A member of a structure (in its type's `.ohls`, or a separate `.ohlfe`
model) names its cross-section by id,

```toml
[[member]]
id = "leg1"
nodes = ["B1", "A1"]
section = "L-61x7"
```

and the id is looked up **in the project's cross-section library first, then in
the structure's own `[[section]]` tables** (read by the finite-element
check, [Structure analysis](structure-analysis.md#structure-models-ohlfe)):
the profile's properties and its material's E, G, density and yield. An
id in neither is refused, naming it. A structure file alone does not
check its ids against a library: the structure model does, when it is
built.

In a project the cross-section library is stored like the other library kinds
of the project file, per project line (kind `sections`), so every
structure of the line uses the same profiles: when the project opens
without one, the `.ohlsec` files of the `Sections/` folder next to it
(else those next to it) are copied in — never the shipped example — and
exported with the other libraries (see [Library
editors](desktop-app.md#library-editors)). For a line file the `Sections/`
folder (else the files next to it) is read directly. `ohl line` and `ohl
structures fe` take `--sections <file|folder>`; `ohl line` reads the
`Sections/` folder next to the line by default.

Member groups (one cross-section for many members) are set in the structure
application (**Design › Groups**): the group's cross-section is written on
each of its members, so it resolves the same way.
