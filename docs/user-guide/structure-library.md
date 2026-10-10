# Structure and insulator libraries (`.olss`, `.olsi`)

Structure types and insulator assemblies are stored **one per file**, so each can be reused, shared and versioned individually:

- `*.olss` — structure types ("OLS structure"), like PLS-CADD's structure files: a project keeps its pylons in a **`Structures`** folder next to its line or project file (used automatically by `ols line`); shared types can live in `libraries/structures/`,
- `libraries/insulators/*.olsi` — insulator assemblies ("OLS insulator"), for detailed strings; simple pylons define their strings in the pylon file itself (below).

Both are TOML with units on every quantity (`"12.5 m"`, `"1.4 kN"`), like [wire libraries](wire-library.md). Unknown fields are errors.

## Structure types (`.olss`)

**Local axes**: origin at the centre of the structure, **x to the right** of the line looking ahead, **y ahead** along the line, **z up**. z = 0 is the ground, unless the type gives the level of its feet: a pylon drawn from a reference level under its lowest crossarm says `foot = "-32 m"`, and is placed with its feet on the ground (raised by the structure's `height_adjust` for foundations, see [Using types in a line](#using-types-in-a-line)).

```toml
format = "ols-structure"          # optional header
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
side = "ahead"                       # ahead (+y) | back (−y): which span it serves

[[extension]]                        # one type, several heights
name = "+3"
dz = "3 m"

[[body]]                             # optional outline for drawings / 3D
points = [["-4 m", "-4 m", "0 m"], ["0 m", "0 m", "52 m"], ["4 m", "4 m", "0 m"]]
```

| Field | Required | Meaning |
|---|---|---|
| `id`, `name`, `kind` | yes | Identity and family. |
| `point` | no | Points where strings hang: `label` and `point` (local), or `node` (at a joint), or `member` and `at` (along a member, 0–1 from its first joint) — then it moves with the pylon. Labels are unique among points and wire points. |
| `wire` | no | Wire points: `label`, optional `set` (a wire is clamped here; unique), optional `point` (local; default: placed from its strings, below), optional `weight` (a counterweight hanging at it). |
| `string` | no | Strings: `label`, `top` (a point or a wire point), `wire` (a wire point), an `insulator` from the library or a `length` (and `weight`), optional `compression` (what it may take; none: tension only), `strain`, `side` (a strain string's span: ahead or back). |
| `attachment` | no | Files of version 1: attachment points (`set`, `phase`, `point`, `insulator` or `string`, `side`), read as before (below). |
| `extension` | no | Named body extensions: `dz` lengthens the body below z = 0, so everything above the feet rises by it. |
| `foot` | no | Level of the feet in local z (default 0: z = 0 is the ground). |
| `body` | no | Stick drawing: polylines (local axes). Without it, a mast from the centre point to `height` (or the highest point) and a crossarm from the mast to every point. |
| `height` | no | Mast top (local z) for the generated stick drawing. |
| `capacity` | no | What the type can carry, for the structure checks (below). |
| `node`, `member` | no | The pylon's joints and members (levels 2 and 3, below). |
| `section`, `material`, `analysis`, `load` | no | A full design's sections, materials, analysis options and own loads — see [Structure analysis](structure-analysis.md). |
| `model` | no | A separate structure model (`.olsfe`, path relative to this file) for the finite-element check, instead of the members here. |
| `pole` | no | Pole id (a `.olsp` file in the same folder) for the pole check — see [Structure analysis](structure-analysis.md). |

### Points, strings and wire points

A string hangs from its top — a point of the structure, or another string's wire point — to a wire point, so any assembly is built from strings ([line file](line-file.md#points-strings-and-wire-points)): two strings to one wire point make a V; a third from that wire point down to a wire point with a set, a V with a string below; a string down to a wire point and two strain strings from it to two sets, a TAK.

A wire point without a `point` is placed from its strings (their tops placed first): below its one string by the string's length (a library assembly: its vertical drop), or along the line with a `side`; with several strings, below the middle of their tops where their lengths meet. A string's weight is its own, else its assembly's.

**Version 1 files** gave each string as an `[[attachment]]` with a set and phase; they are read as before: an attachment's point is labelled by its string's `top` label, else `A<set>-<phase>`, and its set is the set itself for phase 1, else `set + 1000 · (phase + 1)` ([line file](line-file.md#points-strings-and-wire-points)).

### Detail levels

A structure file describes its structure at one of three levels. The level is **worked out from what the file holds** (a `detail = …` in older files is read and ignored) and shown in the structure's properties; it rises as the pylon is filled in:

| Level | What the file holds | Used for |
|---|---|---|
| **1 — stick figure** | Only the points, strings and wire points: a mast from the centre to the top and crossarms to the points are drawn. | Placing the wires, drawings, the 3D view. |
| **2 — sticks of the pylon** | Joints (`[[node]]`) and members (`[[member]]`) without sections, or `[[body]]` polylines. | Drawings and the 3D view with the pylon's members; distances to the structure (planned). |
| **3 — full design** | Every member with a `section` (from `[[section]]` and `[[material]]`), and at least one supported joint — or a separate `model = "….olsfe"`. | Structure analysis (finite elements) and fabrication. |

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

The joints and members take the same fields as in a [structure model](structure-analysis.md#structure-models-olsfe); a point on a joint carries its loads in the finite-element check.

Checks that depend on the structure's shape — distances from the wires to the structure body, structure analysis, insulator swing to the structure — behave differently at each level; each such check says in its documentation what it does for levels 1, 2 and 3. The structure's level is shown in its properties.

### Capacity

The structure checks ([Structure usage](loads.md#structure-usage-capacity-checks)) read what a type can carry from its `[capacity]` — the values from its design (drawings, calculations, the manufacturer); OLS CAD ships none. Both methods are optional:

```toml
# Method 1: allowable spans by line angle (interpolated linearly; a
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

## Simple pylon files

How much string detail is worth modelling depends on the pylon. A **simple pylon file** gives each string's length and weight in the pylon itself — no insulator library needed:

```toml
format = "ols-structure"
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

[[point]]                           # a strain string on a dead-end: with a side
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
side = "ahead"
```

A string hangs down (suspension) or, with a `side`, is a strain string along the line — solved as a free member that lines up with its conductor, like a library strain assembly. A wire point no string reaches is clamped where it is. The stick drawing appears in the 3D viewer and in the plan and profile sheets.

## Insulator assemblies (`.olsi`)

```toml
format = "ols-insulator"
version = 1
id = "I-380-double"
name = "380 kV double I-string"
kind = "suspension"          # suspension | v_string | strain
length = "5.2 m"             # attachment point to clamp (leg length for v_string)
weight = "1.4 kN"            # total weight (default 0)
axial_stiffness = "50e6 N"   # per string / leg (default 5e7 N)
wind_area = "0.6 m²"         # projected area for wind (default 0)
# half_width = "3 m"         # v_string only: centre to each leg attachment
```

| Kind | Geometry | In the solver |
|---|---|---|
| `suspension` | clamp `length` below the attachment point | free clamp on a tension-only member (swings) |
| `v_string` | two legs from ±`half_width` (along the structure's local x, across the line) to a clamp below | two tension-only legs to a free clamp; the leeward leg goes slack in strong wind (a version 1 attachment; in version 2 each leg is a string naming the assembly, which gives each half its wind area) |
| `strain` | clamp `length` from the attachment point, along its conductor | free member of that length: direction and droop follow the conductor tension and the assembly weight |

Strings hanging from strings (a V with a string below, a TAK) are built from strings in the structure type (above). Yokes, jumper support strings and counterweights as strings are planned (DESIGN_GOALS §6, §11).

## Using types in a line

A structure in a [line file](line-file.md) references a type instead of listing its strings:

```json
{ "id": "T12", "type": "DL380-tangent", "position": [2600123.4, 1200456.7, 512.3],
  "orientation_deg": -31.5, "extension": "+3" }
```

`orientation_deg` is the direction of the type's local x axis (right of the line), degrees counter-clockwise from east. `position` is the centre on the ground: the type stands there with its feet on the ground, or `height_adjust` metres above it (e.g. `0.3` for foundations standing out of the ground; negative: sunk). When solving, OLS CAD places the type (`--structures`, `--insulators` library directories), keeps the structure's own points, strings and wire points as **overrides** (the same label; a wire point with a set replaces the type's with that set and what moves with it), leaves out those in `omit` / `omit_sets`, and aims every strain string at its neighbouring point along the section.

**Strings given as geometry vs library assemblies**: a string with a library `insulator` is modelled as a free member (it needs its weight); a strain string given only as geometry (e.g. imported from another program) gets the weight that keeps its clamp where the data puts it — imported clamp positions already are the source's equilibrium.

## Deriving libraries from an existing line

```bash
cargo run -p ols-cli -- structures derive my-line.ols.json --out project/ --inline-strings
```

writes simple pylon files, one per type, into `project/Structures/` (version 1 attachments with their strings, labels `A<set>-<phase>` and `W<set>-<phase>` to rename; a set hanging on several strings is derived as one string from their middle) and `project/my-line-typed.ols.json`. Without `--inline-strings`, `--out derived/` writes `derived/structures/*.olss`, `derived/insulators/*.olsi` and `derived/my-line-typed.ols.json` (the line referencing the types). Each structure's rotation is taken from its route (bisector of its back and ahead spans), attachment layouts that match within 1 cm become one type, and insulators are classified from the geometry (section ends are always strain assemblies; other clamps hanging below their attachment, swung less than 45°, are suspension strings) and grouped by kind, length and weight. The self-check reports how closely the derived libraries reproduce the attachment points and insulator lengths.

On a real 78-structure line (local check, data not in the repository) every tower turned out to be individual — each has its own layout — while about 1 500 attachments reduced to 33 insulator assemblies; the self-check reproduced all attachment points and lengths within 1 cm. Weights, stiffness and wind areas cannot be derived from geometry: fill them in.
