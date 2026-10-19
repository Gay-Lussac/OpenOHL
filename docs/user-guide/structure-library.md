# Structure and insulator libraries (`.ohls`, `.ohli`)

Structure types and insulator assemblies are stored **one per file**, so each can be reused, shared and versioned individually. In the desktop app the structure types are the **Structure library** (Structures menu): every structure the model uses, each imported or exported as a file:

- `*.ohls` — structure types (the `ohl-structure` format): a project keeps its pylons in a **`Structures`** folder next to its line or project file (used automatically by `ohl line`); shared types can live in `libraries/structures/`,
- `libraries/insulators/*.ohli` — insulator assemblies (the `ohl-insulator` format), for detailed strings; simple pylons define their strings in the pylon file itself (below). OpenOHL ships three **examples** with typical values (a 220 kV I-string, tension string and V-string, ids `example-220-…`) to start from — check them against your assemblies' data sheets. Structure types belong to a project (its `Structures` folder); none are shipped.

Both are TOML with units on every quantity (`"12.5 m"`, `"1.4 kN"`), like [wire libraries](wire-library.md). Unknown fields are errors.

## Structure types (`.ohls`)

**Local axes**: origin at the centre of the structure, **x to the right** of the line looking ahead, **y ahead** along the line, **z up**. z = 0 is the ground, unless the type gives the level of its feet: a pylon drawn from a reference level under its lowest crossarm says `foot = "-32 m"`, and is placed with its feet on the ground (raised by the structure's `height_adjust` for foundations, see [Using types in a line](#using-types-in-a-line)).

```toml
format = "ohl-structure"          # optional header
version = 2
id = "DL380-tangent"              # letters, digits, - _ .
name = "380 kV double circuit, tangent"
kind = "lattice"                  # lattice | pole | portal | other
source = "Drawing 12-345 rev B"

[[point]]                            # where strings hang, local axes
label = "A1"
point = ["-12.5 m", "0 m", "45 m"]

[[wire]]                             # where the wire is clamped: set 1
label = "W1"
set = 1

[[string]]                           # an I-string from A1 to W1
label = "S1"
top = "A1"
wire = "W1"
insulator = "I-380-double"           # from the insulator library (or length, weight)

[[point]]                            # a strain set on a dead-end type
label = "A2"
point = ["-12.5 m", "1.2 m", "38 m"]

[[wire]]
label = "W2"
set = 2

[[string]]
label = "S2"
top = "A2"
wire = "W2"
insulator = "S-380-double"
angle = "90 deg"                     # from straight down: 0 hanging, 90 level
direction = "0 deg"                  # in plan: 0 ahead (+y), 90 right, 180 back

[[extension]]                        # one type, several heights
name = "+3"
dz = "3 m"

[[body]]                             # optional outline for drawings / 3D
points = [["-4 m", "-4 m", "0 m"], ["0 m", "0 m", "52 m"], ["4 m", "4 m", "0 m"]]
```

| Field | Required | Meaning |
|---|---|---|
| `id`, `name`, `kind` | yes | Identity and family. |
| `description` | no | What it is, in words (free text; shown in the structure picker). |
| `point` | no | Points where strings hang (each, like wire points and strings, may have a `description`, free text, carried into the line): `label` and `point` (local), or `node` (at a joint), or `member` and `at` (along a member, 0–1 from its first joint) — then it moves with the pylon. Labels are unique among points and wire points. |
| `wire` | no | Wire points: `label`, optional `set` (a wire is clamped here; unique), optional `point` (local; default: placed from its strings, below), optional `weight` (a counterweight hanging at it). |
| `string` | no | Strings: `label`, `top` (a point or a wire point), `wire` (a wire point), an `insulator` from the library or a `length` (and `weight`), optional `wind_area` (default: its insulator's, else none), optional `compression` (what it may take; none: tension only), `strain` (a strain string, else suspension), `angle` (from straight down, default 0: hanging) and `direction` (in plan, default 0: ahead); version 4: optional **limits** `max_load`, `max_swing`, `max_swing_across`, `max_swing_along` (default: its insulator's; not checked, [String limits](#string-limits-not-checked)). Files written before gave a strain string a `side` (`ahead`/`back`): read as level, direction 0° or 180°. |
| `attachment` | no | Files of version 1: attachment points (`set`, `phase`, `point`, `insulator` or `string`, `side`), read as before (below). |
| `extension` | no | Named body extensions: `dz` lengthens the body below z = 0, so everything above the feet rises by it. Version 3: an extension may carry its own joints, members and leg extensions ([Extension bodies](#extension-bodies-and-leg-extensions)). |
| `foot` | no | Level of the feet in local z (default 0: z = 0 is the ground). |
| `body` | no | Stick drawing: polylines (local axes). Without it, a mast from the centre point to `height` (or the highest point) and a crossarm from the mast to every point. |
| `height` | no | Mast top (local z) for the generated stick drawing. |
| `capacity` | no | What the type can carry, for the structure checks (below). |
| `source` | no | Where the data comes from (free text). |
| `wind` | no | Wind on the structure, in height bands ([below](#wind-on-the-structure)). |
| `load_case` | no | The structure's own load cases ([below](#load-cases-of-the-structure)). |
| `template` | no | The pylon template it was built from, its role and values ([Pylon templates](#pylon-templates-ohlpt)). |
| `node`, `member` | no | The pylon's joints and members (levels 2 and 3, below); a member may have a `group`, and both a `made_by` (the generator that wrote it). |
| `generator`, `symmetry` | no | Version 3: the body generators with their inputs, and the mirror symmetry kept while editing ([Body generators](#body-generators-groups-and-symmetry)). |
| `section`, `material`, `analysis`, `load` | no | A full design's sections, materials, analysis options and own loads — see [Structure analysis](structure-analysis.md). |
| `model` | no | A separate structure model (`.ohlfe`, path relative to this file) for the finite-element check, instead of the members here. |
| `pole` | no | The pole: `pole = "id"` (a `.ohlp` file in the same folder, by its id or its file name) or, version 3, a `[pole]` table in the file with its outline and design ([Poles](#poles)) — checked by the pole check, see [Structure analysis](structure-analysis.md). |

### Points, strings and wire points

A string hangs from its top — a point of the structure, or another string's wire point — to a wire point, so any assembly is built from strings ([line file](line-file.md#points-strings-and-wire-points)): two strings to one wire point make a V; a third from that wire point down to a wire point with a set, a V with a string below; a string down to a wire point and two strain strings from it to two sets, a TAK.

A wire point is placed from its strings (their tops placed first): at its one string's length (a library assembly: its length) in the string's direction (`angle`, `direction`); with two or more strings from other tops (a V), where the first two's lengths meet: of the circle both reach, the point nearest where the first string's direction points (hanging: the lowest). A wire point no string reaches is clamped where its `point` says. A string's weight, stiffness and wind area are its own, else its assembly's (a V-string assembly's weight and wind area shared by its legs).

**Version 1 files** gave each string as an `[[attachment]]` with a set and phase; they are read as before: an attachment's point is labelled by its string's `top` label, else `A<set>-<phase>`, and its set is the set itself for phase 1, else `set + 1000 · (phase + 1)` ([line file](line-file.md#points-strings-and-wire-points)).

### Detail levels

A structure file describes its structure at one of three levels. The level is **worked out from what the file holds** (a `detail = …` in older files is read and ignored) and shown in the structure's properties; it rises as the pylon is filled in:

| Level | What the file holds | Used for |
|---|---|---|
| **1 — stick figure** | Only the points, strings and wire points: a mast from the centre to the top and crossarms to the points are drawn — or `[[body]]` polylines that all lie in one plane (a face of the pylon). | Placing the wires, drawings, the 3D view. |
| **2 — sticks of the pylon** | Joints (`[[node]]`) and members (`[[member]]`) without sections, or `[[body]]` polylines that make a 3D object (their points, all together, not in one plane — more than 1 mm off it). | Drawings and the 3D view with the pylon's members; distances to the structure (planned). |
| **3 — full design** | Every member with a `section` (from `[[section]]` and `[[material]]`), and at least one supported joint (a joint with a `support`; a joint held only by a `spring` does not count, so a structure standing only on springs stays at level 2) — or a separate `model = "….ohlfe"`. An extension with a body: its members with sections and a supported joint too. | Structure analysis (finite elements) and fabrication. |

A **pole** has the same three levels ([Poles](#poles)): without an outline it adds nothing (level 1); its outline — diameters at two heights at least — makes level 2; with what the pole check needs (material, strength, its wall thickness, `solid`, or a section) level 3. A pole named by a `.ohlp` file is level 3. The structure's level is the higher of its members' and its pole's.

```toml
[[node]]                             # a joint, local axes
id = "B1"
point = ["-2.5 m", "-2.5 m", "-30 m"]
support = "pinned"                   # level 3: where the pylon is held

[[node]]
id = "T1"
point = ["-6 m", "0 m", "0 m"]

[[member]]
id = "arm1"
nodes = ["B1", "T1"]
section = "L90x9"                    # level 3; without it: a stick

[[point]]
label = "A1"
node = "T1"                          # at a joint (or member = "arm1", at = 0.5)
```

The joints and members take the same fields as in a [structure model](structure-analysis.md#structure-models-ohlfe); a point on a joint carries its loads in the finite-element check. A member's `kind` is `truss` (default), `frame`, `cable` or — version 5 — `tension`: a truss in tension only, left out of the analysis while it would be compressed (a tower program's "T-only" bracing; [Tension-only members](structure-analysis.md#tension-only-members)). The structure application raises the file to version 5 when a member is made `tension`; an older program then says the file is newer instead of failing on the kind. Version 6 also adds two member keys: `bending = true` on a `tension` member (it also bends, as a frame, while it is in the analysis: a tower program's tension-only beam; [Tension-only members](structure-analysis.md#tension-only-members)) and `check = false` (a member that is only part of the model — a tower program's fictitious member — solved but not checked).

Checks that depend on the structure's shape — distances from the wires to the structure body, structure analysis, insulator swing to the structure — behave differently at each level; each such check says in its documentation what it does for levels 1, 2 and 3. The structure's level is shown in its properties.

### Body generators, groups and symmetry

Version 3 of the format (versions 1 and 2 are read as before; an older OpenOHL refuses a version 3 file as "newer") keeps what the [structure application](structure-app.md#the-body-panels) needs to build the body again:

```toml
version = 3

[[generator]]                        # legs and panels: the body as a frustum
id = "body"
kind = "panels"
feet = "6 m"                         # width at the feet; or ["8 m", "6 m"]: across (x), along (y)
waist = "2 m"                        # width at the top of the last panel
# bottom = "0 m"                     # level of the feet (default: the file's foot)
panel = [
  { top = "8 m", bracing = "x", horizontal = true },
  { top = "15 m", bracing = "k" },   # horizontal: true by default
  # options, each written only when on:
  # { top = "20 m", bracing = "x", width = "3 m", redundants = "one", plan = "x" },
]
# plan_auto = "diamond"              # plan bracing where the slope changes and at the crossarms' levels

[[generator]]                        # crossarms from the body face to points
id = "arm1"
kind = "crossarm"
points = ["A1", "A11"]
shape = "triangular"                 # or "rectangular"
# one_arm = true                     # the points on one side sharing their roots make one arm

[[generator]]                        # a peak on the top of the body
id = "peak"
kind = "peak"
points = ["E1"]                      # earth-wire points; none: one tip at the centre…
# height = "4 m"                     # …this high above the top

[[generator]]                        # a face of a panels body to the opposite face
id = "front"
kind = "copy_face"
body = "body"
face = "y-"                          # x+, x-, y+ or y-

[[symmetry]]
mirror = "xy"                        # "x": about x = 0 (left ↔ right), "y": about y = 0 (ahead ↔ back), "xy": both
# groups = ["body-leg"]              # only the members of these groups and their joints (default: all)

[[member]]
id = "body-M1"
nodes = ["body-L0-1", "body-L1-1"]
group = "body-leg"                   # sections and results per group
made_by = "body"                     # written by generator body: a rebuild replaces it
```

- **What a generator writes** are ordinary `[[node]]` and `[[member]]` entries with its `made_by`; the file never needs the generator to be read. Ids follow from the inputs: joints `body-L<level>-<corner>` (corners 1 to 4 counter-clockwise from +x +y, level 0 the feet), `body-L<level>-F<face>` in the middle of a horizontal edge (faces 1 to 4: x+, y+, x−, y−), `body-P<panel>-<corner>` in the middle of a leg; members `body-M<n>`; groups `body-leg`, `body-bracing-<panel>`, `body-horizontal-<level>`; a crossarm's tip `arm1-A1` (the point is put on it), groups `arm1-bottom`, `arm1-top`, `arm1-bracing`; a peak's tip `peak-tip` (at a point: `peak-E1`), group `peak-peak`; a copied face's joints and members `front-<the original's id>`, in the original's group.
- **Bracing** per panel: `single` (one diagonal, bottom left to top right of each face), `x` (both, crossing without a joint), `k` (from the bottom corners to the middle of the top edge, which then has its horizontal), `diamond` (between the middles of the four edges, legs and both horizontals split there), `warren` (one diagonal, alternating from panel to panel). `horizontal` puts horizontals at the panel's top.
- **Crossarms** start on the body face on the point's side (its outermost joints at a level): the lower root at the highest level at or below the point, the upper root at the next level up (a point at the top of the body: the top and the level below). *Triangular*: four chords to one tip at the point. *Rectangular*: a bottom tip at the point, a top tip above it, a vertical and two diagonals (a point at the top of the body: the point is the top tip, the bottom tip one level down). A peak rises from the body's top level, not from the arms' tips.
- **Options** (all off by default; a generator without them builds exactly what it built before, and writes none of them). They are **version 6** of the format: the structure application raises the file to 6 when it writes a generator using one (and only then; a file without a `version` stays without), so an older OpenOHL says the file is newer instead of failing on an unknown field. Versions 1 to 5 are read as before:
  - `width` on a panel (not the last: its top is the waist): the body's width at the panel's top, a length or [across, along]; between the widths given (the feet, the panels', the waist) the faces are straight, so the slope can change at a panel's top (legs, then a straight or less battered body).
  - `redundants` on a panel, `"one"` or `"two"`: secondary members in its four faces, in group `body-redundant-<panel>`, so they get their own section and results. They cut the main diagonals into shorter lengths; every joint they add lies on a member (cut there) and is braced to joints held already, so each face stays triangulated (no mechanism in its plane). The legs are cut at their middle (`body-P<panel>-<corner>`). Per bracing, one level: *single* and *Warren* — the diagonal's middle on a member between the legs' middles, and a secondary diagonal in each quadrilateral either side (from each end of the diagonal to the middle of the other leg); *X* — a joint at the crossing, braced to both legs' middles; *K* — each diagonal's middle, braced to its leg's middle and to the top corner on its side; *diamond* — each side's middle, braced to the corner it faces. Two levels add the halves' middles, braced to the same leg middle or corner. Joints `body-R<panel>-F<face>-C`, `-C1`…, `-D1`…; members `body-R<panel>-<n>`; a member cut keeps its id with `-1`, `-2`… (`body-M5-1`, `body-M5-2`), so the other members keep theirs.
  - `plan` on a panel, `"x"` or `"diamond"`: plan bracing at the panel's top, in group `body-plan-<level>`, members `body-H<level>-<n>`. *X*: both diagonals of the plan, corner to corner (crossing without a joint). *Diamond*: between the middles of the four horizontals (joints `body-L<level>-F<face>`, the horizontals cut there) and a cross from them to a joint at the centre (`body-L<level>-C`; without it the diamond turns in plan, its corners sliding along the horizontals). A level without horizontals gets them too (`body-H<level>-E<face>`).
  - `plan_auto` on the generator: that plan bracing at the levels where the slope changes (a panel's `width` off the line of its neighbours) and at the two levels each crossarm generator of the file starts from (its points' lower and upper roots on this body). A panel's own `plan` goes first. The crossarms are read from the file when the body is built: after adding a crossarm, build the body again (**Rebuild all**).
  - `one_arm` on a crossarm: the points on one side whose arms start from the same root joints (one side, one level) make one arm. The outermost point is the tip, as a single arm has it; at each other point the arm has a section: joints where its chords pass (`arm1-A2-bottom-1`, `-2`, `arm1-A2-top-1`, `-2`), the point braced to the four, a member up each side and one across the other chord. Between sections: the chords, a diagonal in each side face (from the lower chord outwards to the upper one) and two members from the section inside to the next point. Points are put on their joints. With one point per side it is the same arm as without the option; two points as far out on one arm are refused.
- **Pieces keep what was set**: when a rebuild cuts a member it made into pieces (`body-M5` → `body-M5-1`, `body-M5-2`, a redundant turned on), the pieces take what was set on it (section, kind, group…); turned off again, the whole member takes the first piece's.
- **Rebuild** replaces only what the generator made: a joint or member it makes again (same id) keeps what was set on it (section, kind, group, support…); one it no longer makes goes, unless a member made by hand, another generator's member or a point uses it — then it stays, as made by hand, and is reported. Hand-made joints and members are never touched.
- **Groups**: there is no table of groups; a group is the members naming it. A section set for a group is written on each of its members, so the finite-element model and the level read the members only (the FE model leaves `group` and `made_by` out).
- **Symmetry**: joints are matched by position (within 1 mm); see [the Body panels](structure-app.md#the-body-panels).

### Poles

A pole structure names its pole — `pole = "steel-pole-30m"` (the `.ohlp` file with that id next to the structure; or `pole = "steel-pole-30m.ohlp"`, by file name), level 3 — or, version 3, describes it in the structure file:

```toml
version = 3

[pole]
shape = "polygon"                    # or "round" (default)
sides = 12                           # a polygon's number of sides (3 or more)
embedment = "3 m"                    # depth in the ground (optional): length = height + this

# Level 3: the fields of a .ohlp file, same names and meaning
material = "steel"                   # wood | steel | concrete | other
wall = "8 mm"                        # or "solid", or section = "P-12x500" (below); none: not given
elasticity = "210 GPa"               # P-Delta
density = "7850 kg/m3"               # the pole's weight
drag = 0.8                           # shape factor on the wind (default 1)
# fibre_stress = "55 MPa"            # wood: the moment capacity from the taper
# moment_capacity = "900 kN m"       # at the ground line

[[pole.outline]]                     # level 2: the outside lines, from the feet up
z = "0 m"                            # local z of the structure (the feet: the ground line)
diameter = "700 mm"                  # or circumference; a polygon: across flats

[[pole.outline]]
z = "27 m"
diameter = "300 mm"

[[pole.capacity]]                    # or: the moment capacity by height above the ground line
height = "0 m"
moment = "900 kN m"
```

- **Outline**: heights rise from row to row (by more than 1 mm); a row gives its `diameter` or its `circumference` (a polygon's perimeter: across flats = perimeter / (n · tan(π/n))). Between the heights the diameter is linear; below the lowest and above the top it is held. The pole's height above ground is the top's z less the feet's.
- **Level 3** needs the `material`, a strength (`fibre_stress`, `moment_capacity` or `[[pole.capacity]]`) and its wall: its wall thickness, `wall = "solid"` (a solid pole, no hollow), or a `section`, whatever the material. `wall` takes a length or the word `"solid"`; any other word is refused, naming `wall`. A pole named by a `.ohlp` file counts as level 3 as given; brought into the structure file, its missing wall is listed (a `.ohlp` without a wall brings none: you decide between a thickness, solid and a section).
- **`section`**: a hollow section of the project's section library ([Section library](section-library.md)) — `polygonal_hollow` with the pole's sides, or `circular_hollow` for a round pole. Its wall `t` is the pole's wall (unless `wall` is given: a thickness, or solid), and its material's elasticity and density are taken when not given here. The section's own diameter is not used: the outline gives the diameters.
- **A polygon's section** at each height: a regular polygon of the outline's width across flats, hollow by the wall; its second moment is the same about every axis through the centre (A·(12 r² + a²)/48 for inradius r and side a, as the section library computes it), its elastic modulus taken at a corner (the smaller).

The structure application edits the pole (**Body › Pole…**, **Design › Pole design…**) and brings a named `.ohlp` into the file in one step: its diameters at the ground line and the top become a two-height outline from the feet, its other fields are copied, and the pole check gives the same numbers.

### Extension bodies and leg extensions

At levels 1 and 2 an extension is a `name` and a `dz`. At level 3 (version 3) it may carry its own **piece** — the joints and members between the ground and the feet — and **leg extensions**, one leg longer than the others (decision 20: one file describes a family; each placed structure stands on the extension chosen at its place):

```toml
[[extension]]
name = "+3"
dz = "3 m"

[[extension.node]]                   # the piece: z from the feet less dz to the feet
id = "X3-J1"
point = ["3.3 m", "3.3 m", "-3 m"]
support = "pinned"                   # the piece carries the supports

[[extension.member]]                 # between the piece's joints and the body's, by id
id = "X3-M1"
nodes = ["X3-J1", "body-L0-1"]
section = "L80"
group = "X3-leg"

[[extension.leg]]                    # one leg longer: A, B, C, D or a joint id
leg = "A"
dz = "1 m"
```

**The rule**, in the structure's own axes (the body is not moved; placed, the whole rises by the extension's `dz` as before):

1. **A body extension** adds its joints and members to the body's. A body joint a member of the extension reaches stands on the extension: its support (and spring) is dropped; the extension's own joints carry the supports written on them. Its joints lie between the feet less `dz` and the feet; its ids are unique among the body's; its `dz` is above 0.
2. **A leg extension** lowers one support joint by its `dz`. Leg **A** is the joint in the quarter x > 0, y > 0, **B** x < 0, y > 0, **C** x < 0, y < 0, **D** x > 0, y < 0 — among the supported joints (after rule 1) when there are any, else among all joints — the lowest one there (two equally low: name the joint by its id). The joint slides along its **leg member** — of the members ending at it, the one rising most steeply to a joint above — until it is `dz` lower, so the leg member is stretched along its own line; with no member rising from it, it moves straight down. Every member ending at it follows.
3. A placed structure standing on an extension with a piece or legs is analysed on that model ([Structure analysis](structure-analysis.md#extensions-in-the-model)); an extension without them uses the model as written (as before).

At level 3 every extension piece gets all its parameters: an extension's member without a section, or a piece without a supported joint, keeps the structure at level 2. An extension without members stays allowed at level 3 — the structure application lists it as advice ("the FE model leaves it out").

The structure application's **Body › Extensions** adds a piece by hand or builds it in one go: under every foot of the body (its supported joints, else its lowest joints) a joint `dz` lower along the foot's leg (the same leg rule) with the foot's support, a leg member to the foot (group `X<name>-leg`), the faces between neighbouring feet braced (one diagonal or X, horizontals at the bottom on request, group `X<name>-bracing`), sections as the body's last member; ids `X<name>-J<n>` and `X<name>-M<n>` (`+3` → `X3-J1`).

### Capacity

The structure checks ([Structure usage](loads.md#structure-usage-capacity-checks)) read what a type can carry from its `[capacity]` — the values from its design (drawings, calculations, the manufacturer); OpenOHL ships none. Both methods are optional:

```toml
# Method 1: allowable spans by line angle (interpolated linearly in the
# sine of half the angle, on the safe side; a
# structure turning more than the last angle fails).
[[capacity.spans]]
angle = "0 deg"
wind_span = "400 m"
weight_span = "600 m"
min_weight_span = "-50 m"     # optional: uplift limit (may be negative)

[[capacity.spans]]
angle = "10 deg"
wind_span = "300 m"
weight_span = "550 m"

# Method 2: allowable loads at a point of the structure (a load tree),
# structure axes, compared with the factored loads × the load case's
# strength factor.
[[capacity.attachment]]
point = "A1"                  # version 1 files: set, phase of an attachment
vertical = "40 kN"            # downwards
uplift = "10 kN"              # optional (none: an upward load fails)
transverse = "30 kN"          # either way
longitudinal = "20 kN"        # either way
interaction = "linear"        # optional: the ratios add up (default: each on its own)
```

Angles rise from 0; spans are above 0, the minimum weight span below the weight span. A direction without a value is not checked. Every `capacity.attachment` names a point of the type; points loaded without one are reported as not in the load tree. The synthetic example's types (`examples/synthetic-line/Structures`) carry invented capacities.

### Wind on the structure

```toml
[[wind.band]]
from = "0 m"
to = "20 m"
area = "14 m²"     # projected area facing the wind (across the line)
drag = 3.0         # drag (force) coefficient
```

Each band: bottom and top above the ground, projected area, drag coefficient (from the structure's design or the code used; e.g. about 2.8–3.2 for lattice faces, 0.7–1.2 for round poles). The structure loads add pressure × drag × area per band in every load case with wind ([Structure loads](loads.md#structure-loads)).

### Load cases of the structure

`[[load_case]]` tables hold the structure's own load cases: recipes saying how the line's loads act on it, for the structure application ([Structure load cases](structure-load-cases.md)). The line run does not read them.

## Simple pylon files

How much string detail is worth modelling depends on the pylon. A **simple pylon file** gives each string's length and weight in the pylon itself — no insulator library needed:

```toml
format = "ohl-structure"
version = 2
id = "D380-tangent"
name = "380 kV double circuit, tangent: 3 crossarms each side + earth wire"
kind = "lattice"
height = "62 m"

[[point]]
label = "A1"
point = ["-12 m", "0 m", "40 m"]

[[wire]]
label = "L1"
set = 1

[[string]]
label = "S1"
top = "A1"
wire = "L1"
length = "5.2 m"
weight = "1.4 kN"

# … one per crossarm end, both sides

[[wire]]                            # the earth wire, clamped on the mast top
label = "E"
set = 9
point = ["0 m", "0 m", "62 m"]

[[point]]                           # a strain string on a dead-end: level, ahead
label = "A2"
point = ["-12 m", "1.5 m", "36 m"]

[[wire]]
label = "L2"
set = 2

[[string]]
label = "S2"
top = "A2"
wire = "L2"
length = "5.0 m"
strain = true
angle = "90 deg"
direction = "0 deg"
```

A string hangs down by default; its `angle` and `direction` point it anywhere — a strain string level along the line, at 0° or 180° — and it is solved as a free member that lines up with its conductor, like a library strain assembly. A wire point no string reaches is clamped where it is. The stick drawing appears in the 3D viewer and in the plan and profile sheets.

## Pylon templates (`.ohlpt`)

A **pylon template** is the face of a pylon — across the line and up, in
its own axes, no outside sizes — given by a few named dimensions. From it
**Structures › New pylon from a template…** builds a structure type of
level 1 (its points, strings and wire points with their sets), for
**suspension** or for **strain**: two separate types, built one after the
other with the toggle. The type records the template, the role and the
values (`[template]`), so a type built before is built again from them
(the structures of that type follow). Built again, only what the
template makes is replaced — the header, `[template]`, and the points,
strings and wire points it makes (by label, for either role); everything
else in the file stays: joints, members, sections, materials, supports,
capacity, bodies, extensions, and points, strings and wire points added
by hand. What refers to a point or wire point the template no longer
makes (a string, a joint's `attachment`, a capacity entry) is kept and
listed in the window to look at; if the file would not read any more
(a string to a wire point no longer there), nothing is written and the
window says why.

The library grows with files: the templates shipped with OpenOHL in
`libraries/pylon-templates`, and a project's own in its
`Pylon templates` folder next to the line (one of the same id is used
instead of the shipped one). The structure application keeps its own
apart: it reads and imports templates in a `PylonTemplates` folder beside
the structure file, and its **New structure…** offers the shipped ones
([OHL-Structure](structure-app.md#libraries)).

```toml
format = "ohl-pylon-template"
version = 1
id = "delta-1c"
name = "Delta, one circuit, one earth wire"
description = "Two phases on the lower crossarm, one on the upper."
kind = "lattice"                # the family of the types built
height = "peak"                 # mast top for the stick drawing (expression)
# first_offset = 20             # strain: the string at 180°: slot + 20 (default)
# second_offset = 40            # strain: the string at 0°: slot + 40 (default)

[strings]                       # offered; changed in the builder
suspension_length = "3 m"
suspension_weight = "1 kN"
strain_length = "4 m"
strain_weight = "1.5 kN"
earth_suspension_length = "0.3 m"   # the earth wires' short strings (clamps)
earth_suspension_weight = "0.1 kN"
earth_strain_length = "0.5 m"
earth_strain_weight = "0.2 kN"

[[parameter]]
name = "lower_level"
default = "22 m"
description = "height of the lower crossarm"

[[parameter]]
name = "lower_half"
default = "5 m"
description = "lower crossarm: mast to each phase"

[[parameter]]
name = "peak"
default = "34 m"

[[attachment]]                  # where a phase hangs
slot = 1
label = "L1"
across = "-lower_half"          # expression of the parameters, m (right +)
up = "lower_level"

[[earth]]                       # an earth wire: its point and a short string
slot = 19
across = "0"
up = "peak"
```

- **Expressions**: numbers (metres), the parameters' names, `+ - * /` and
  parentheses (`"-(inner + 2 * step)"`).
- **Sets** follow the slots. A suspension type: a string hangs at each
  attachment, its wire point has the slot as set. A strain type: two level
  strain strings at each attachment along the line, given by their angles
  only — the first at 180° with the set slot + 20, the second at 0° with
  slot + 40. Earth wires the same, with their own short strings (lengths
  and weights of their own): a point on the pylon, a string to the wire
  point. So slots 1–20 give suspension sets 1–20, and strain sets 21–40
  and 41–60 — a numbering that keeps every phase's set the same through
  suspension and strain structures, also for crowded multi-circuit
  pylons: e.g. circuits in slots 1–3, 4–6, 7–9, a two-phase circuit of
  four conductors in 10–13, another circuit in 16–18, earth wires in 19
  and 20 (the shipped `multi-5c`).
- Labels: the points `A-<label>`, the wire points `W-<label>` (strain:
  `W-<label>-1`, `-2`), the strings `S-<label>` (`-1`, `-2`).
- Checked when read: unique names, slots and labels; every expression
  reads; built with its defaults it makes a valid type both ways; no
  string reaches the ground; the mast top is not below an attachment.

Shipped: `delta-1c`, `flat-1c` (two earth wires), `danube-2c`,
`barrel-2c`, `multi-5c`, `pole-1c` — synthetic dimensions, to be changed
in the builder.

## Insulator assemblies (`.ohli`)

```toml
format = "ohl-insulator"
version = 1
id = "I-380-double"
name = "380 kV double I-string"
kind = "suspension"          # suspension | v_string | strain
length = "5.2 m"             # attachment point to clamp (leg length for v_string)
weight = "1.4 kN"            # total weight (default 0)
axial_stiffness = "50e6 N"   # per string / leg (default 5e7 N)
wind_area = "0.6 m²"         # projected area for wind (default 0)
# half_width = "3 m"         # v_string only: centre to each leg attachment
# version 2: limits, not checked by OpenOHL (see String limits)
# max_load = "120 kN"        # the tension it is rated for (v_string: each leg's)
# max_swing = "60 deg"       # from straight down, any direction
# max_swing_across = "55 deg"
# max_swing_along = "30 deg"
```

| Kind | Geometry | In the solver |
|---|---|---|
| `suspension` | clamp `length` below the attachment point | free clamp on a tension-only member (swings) |
| `v_string` | two legs from ±`half_width` (along the structure's local x, across the line) to a clamp below | two tension-only legs to a free clamp; the leeward leg goes slack in strong wind (a version 1 attachment; in version 2 each leg is a string naming the assembly, which gives each half its wind area) |
| `strain` | clamp `length` from the attachment point, along its conductor | free member of that length: direction and droop follow the conductor tension and the assembly weight |

### String limits (not checked)

A string may carry **limits that OpenOHL does not check**: kept with the string, shown and edited, but nothing is coloured or flagged by them — a plugin or a report that needs them compares them with the solved forces and swings. Each is optional; absent means *not given* (not zero, not unlimited):

| Key | Unit | Meaning |
|---|---|---|
| `max_load` | force (`"120 kN"`) | the tension it is rated for (e.g. its specified mechanical load; a `v_string` assembly: each leg's) |
| `max_swing` | angle (`"60 deg"`) | the swing from straight down it may take, in any direction |
| `max_swing_across` | angle | the swing across the line it may take (in the structure's x–z plane), either side |
| `max_swing_along` | angle | the swing along the line it may take (in the structure's y–z plane), either way |

Angles from 0 to 180°, a load above 0; other values and unknown keys are refused. They go on an **insulator assembly** (`.ohli`, version 2) as defaults and on a **string** (`[[string]]` of a structure type, version 4; a line's own string, line file version 6): a string naming an assembly takes each limit the assembly gives where the string gives none of its own — the string's own value wins. The compression a string may take is not one of them: that is `compression`, which the solve uses (a string with it acts both ways and is flagged beyond it).

The swings they compare with are those of the string as solved, from its top to its wire point: from straight down, and the angles of its projections on the planes across and along the line, positive to the right and ahead (`ohl_line::swing`). OpenOHL writes a file's newer version only when it holds limits (an `.ohli` without any stays version 1), so an older program still reads the rest.

### Library labels: every string in the library

Each string carries a **library label**: the `id` of its assembly (`insulator` in the files). A labelled string takes its weight, wind area and stiffness from the assembly; its length is its own (or, in a structure type, the assembly's). In a project, the strings of the structures' own (not those inside a structure type's file) are **all taken into the library**: on opening, and whenever an edit puts a string without a label — strings of the same values (suspension or strain, length to the mm, weight, wind area, stiffness) share an entry, an existing one with those values or a new one (`I-3.500m-900N`, `Strain-…`, source "taken from the model's strings"); the labels go in with the edit (one undo). Structures › **String library** is the library as a table: label, name, kind, length, weight, wind area, stiffness, half width (a V), the four limits (max load, max swing, max across, max along; empty: not given), source, and how many strings **use** it. Edited there, every string with the label follows (a new length moves their wire points; a V's tip goes where both legs meet; a length its strings cannot take is refused with the reason); rows can be added; a used row deleted (asked first, listing its strings) leaves its strings its values (and its limits, where they give none) under a matching or new label (one used by a structure type's strings cannot be deleted: change the type first).

Strings hanging from strings (a V with a string below, a TAK) are built from strings in the structure type (above). Yokes, jumper support strings and counterweights as strings are planned (DESIGN_GOALS §6, §11).

## Using types in a line

A structure in a [line file](line-file.md) references a type instead of listing its strings:

```json
{ "id": "T12", "type": "DL380-tangent", "position": [2600123.4, 1200456.7, 512.3],
  "orientation_deg": -31.5, "extension": "+3" }
```

`orientation_deg` is the direction of the type's local x axis (right of the line), degrees counter-clockwise from east. `position` is the centre on the ground: the type stands there with its feet on the ground, or `height_adjust` metres above it (e.g. `0.3` for foundations standing out of the ground; negative: sunk). When solving, OpenOHL places the type (`--structures`, `--insulators` library directories), keeps the structure's own points, strings and wire points as **overrides** (the same label; a wire point with a set replaces the type's with that set and what moves with it), leaves out those in `omit` / `omit_sets`, and aims every strain string at its neighbouring point along the section.

**Strings given as geometry vs library assemblies**: a string with a library `insulator` is modelled as a free member (it needs its weight); a strain string given only as geometry (e.g. imported from another program) gets the weight that keeps its clamp where the data puts it — imported clamp positions already are the source's equilibrium.

## Deriving libraries from an existing line

```bash
cargo run -p ohl-cli -- structures derive my-line.ohl.json --out project/ --inline-strings
```

writes simple pylon files, one per type, into `project/Structures/` (version 1 attachments with their strings, labels `A<set>-<phase>` and `W<set>-<phase>` to rename; a set hanging on several strings is derived as one string from their middle) and `project/my-line-typed.ohl.json`. Without `--inline-strings`, `--out derived/` writes `derived/structures/*.ohls`, `derived/insulators/*.ohli` and `derived/my-line-typed.ohl.json` (the line referencing the types). Each structure's rotation is taken from its route (bisector of its back and ahead spans), attachment layouts that match within 1 cm become one type, and insulators are classified from the geometry (section ends are always strain assemblies; other clamps hanging below their attachment, swung less than 45°, are suspension strings) and grouped by kind, length and weight. The self-check reports how closely the derived libraries reproduce the attachment points and insulator lengths.

On a real 78-structure line (local check, data not in the repository) every tower turned out to be individual — each has its own layout — while about 1 500 attachments reduced to 33 insulator assemblies; the self-check reproduced all attachment points and lengths within 1 cm. Weights, stiffness and wind areas cannot be derived from geometry: fill them in.
