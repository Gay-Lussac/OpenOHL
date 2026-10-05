# Structure and insulator libraries (`.olss`, `.olsi`)

Structure types and insulator assemblies are stored **one per file**, so each can be reused, shared and versioned individually:

- `*.olss` — structure types ("OLS structure"), like PLS-CADD's structure files: a project keeps its pylons in a **`Structures`** folder next to its line or project file (used automatically by `ols line`); shared types can live in `libraries/structures/`,
- `libraries/insulators/*.olsi` — insulator assemblies ("OLS insulator"), for detailed strings; simple pylons define their strings in the pylon file itself (below).

Both are TOML with units on every quantity (`"12.5 m"`, `"1.4 kN"`), like [wire libraries](wire-library.md). Unknown fields are errors.

## Structure types (`.olss`)

**Local axes**: origin at the ground centre of the structure, **x to the right** of the line looking ahead, **y ahead** along the line, **z up**.

```toml
format = "ols-structure"          # optional header
version = 1
id = "DL380-tangent"              # letters, digits, - _ .
name = "380 kV double circuit, tangent"
kind = "lattice"                  # lattice | pole | portal | other
source = "Drawing 12-345 rev B"

[[attachment]]
set = 1
phase = 1
label = "L1 left"
point = ["-12.5 m", "0 m", "45 m"]   # insulator attachment point, local axes
insulator = "I-380-double"           # default assembly (from the insulator library)

[[attachment]]                       # a strain set on a dead-end type
set = 2
phase = 1
point = ["-12.5 m", "1.2 m", "38 m"]
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
| `attachment` | no | Attachment points: `set`, `phase`, `point` (local), optional `label`, `insulator`, `side` (strain sets). Set/phase pairs are unique; points must not be below ground. |
| `extension` | no | Named height offsets added to every attachment. |
| `body` | no | Stick drawing: polylines (local axes). Without it, a mast from the centre point to `height` (or the highest attachment) and a crossarm from the mast to every attachment point. |
| `height` | no | Mast top for the generated stick drawing. |
| `capacity` | no | What the type can carry, for the structure checks (below). |
| `model` | no | Structure model (`.olsfe`, path relative to this file) for the finite-element check — see [Structure analysis](structure-analysis.md). |
| `pole` | no | Pole id (a `.olsp` file in the same folder) for the pole check — see [Structure analysis](structure-analysis.md). |

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

# Method 2: allowable loads at an attachment (a load tree), structure axes,
# compared with the factored loads × the load case's strength factor.
[[capacity.attachment]]
set = 1
phase = 1
vertical = "40 kN"            # downwards
uplift = "10 kN"              # optional (none: an upward load fails)
transverse = "30 kN"          # either way
longitudinal = "20 kN"        # either way
interaction = "linear"        # optional: the ratios add up (default: each on its own)
```

Angles rise from 0; spans are above 0, the minimum weight span below the weight span. A direction without a value is not checked. Every `capacity.attachment` names an attachment of the type; attachments without one are reported as not in the load tree. The synthetic example's types (`examples/synthetic-line/Structures`) carry invented capacities.

## Simple pylon files

How much string detail is worth modelling depends on the pylon. A **simple pylon file** gives each string in the pylon itself: where it hangs, its length, and a label at its top (insulator attachment point) and at its bottom (wire attachment point) — no insulator library needed:

```toml
format = "ols-structure"
version = 1
id = "D380-tangent"
name = "380 kV double circuit, tangent: 3 crossarms each side + earth wire"
kind = "lattice"
height = "62 m"

[[attachment]]
set = 1
phase = 1
point = ["-12 m", "0 m", "40 m"]
string = { length = "5.2 m", weight = "1.4 kN", top = "A1-1", bottom = "L1" }

[[attachment]]                      # … one per crossarm end, both sides

[[attachment]]                      # the earth wire, clamped on the mast top
set = 9
phase = 1
label = "earth wire"
point = ["0 m", "0 m", "62 m"]

[[attachment]]                      # a strain string on a dead-end: with a side
set = 2
phase = 1
point = ["-12 m", "1.5 m", "36 m"]
side = "ahead"
string = { length = "5.0 m", top = "A2-1", bottom = "L2" }
```

| `string` field | Required | Meaning |
|---|---|---|
| `length` | yes | From the attachment point to the wire point. |
| `weight` | no (0) | Weight of the string. |
| `top` | no | Label of the insulator attachment point. |
| `bottom` | no | Label of the wire attachment point. |

A string hangs down (suspension) or, with a `side`, is a strain string along the line — solved as a free member that lines up with its conductor, like a library strain assembly. An attachment has an `insulator` (library) or a `string`, not both; with neither, the wire is clamped at the point. The labels are kept on the line's attachments; the stick drawing appears in the 3D viewer and in the plan and profile sheets.

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
| `v_string` | two legs from ±`half_width` (along the structure's local x, across the line) to a clamp below | two tension-only legs to a free clamp; the leeward leg goes slack in strong wind |
| `strain` | clamp `length` from the attachment point, along its conductor | free member of that length: direction and droop follow the conductor tension and the assembly weight |

General linked assemblies (yokes, jumper support strings, counterweights) are planned (DESIGN_GOALS §6, §11).

## Using types in a line

A structure in a [line file](line-file.md) references a type instead of listing attachments:

```json
{ "id": "T12", "type": "DL380-tangent", "position": [2600123.4, 1200456.7, 512.3],
  "orientation_deg": -31.5, "extension": "+3" }
```

`orientation_deg` is the direction of the type's local x axis (right of the line), degrees counter-clockwise from east. When solving, OLS CAD places the type (`--structures`, `--insulators` library directories), keeps any `attachments` listed on the structure as **overrides** (same set and phase), and aims every strain assembly at its neighbouring point along the section.

**Clamps given as geometry vs library assemblies**: an attachment with a library `insulator` is modelled as a free member (it needs its weight); an attachment given only as geometry (e.g. imported from another program) keeps its clamp where the data puts it — imported clamp positions already are the source's equilibrium.

## Deriving libraries from an existing line

```bash
cargo run -p ols-cli -- structures derive my-line.ols.json --out project/ --inline-strings
```

writes simple pylon files, one per type, into `project/Structures/` (strings in the files, labels `A<set>-<phase>` and `W<set>-<phase>` to rename) and `project/my-line-typed.ols.json`. Without `--inline-strings`, `--out derived/` writes `derived/structures/*.olss`, `derived/insulators/*.olsi` and `derived/my-line-typed.ols.json` (the line referencing the types). Each structure's rotation is taken from its route (bisector of its back and ahead spans), attachment layouts that match within 1 cm become one type, and insulators are classified from the geometry (section ends are always strain assemblies; other clamps hanging below their attachment, swung less than 45°, are suspension strings) and grouped by kind, length and weight. The self-check reports how closely the derived libraries reproduce the attachment points and insulator lengths.

On a real 78-structure line (local check, data not in the repository) every tower turned out to be individual — each has its own layout — while about 1 500 attachments reduced to 33 insulator assemblies; the self-check reproduced all attachment points and lengths within 1 cm. Weights, stiffness and wind areas cannot be derived from geometry: fill them in.
