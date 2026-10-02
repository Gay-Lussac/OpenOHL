# Structure and insulator libraries (`.olss`, `.olsi`)

Structure types and insulator assemblies are stored **one per file**, so each can be reused, shared and versioned individually:

- `libraries/structures/*.olss` — structure types ("OLS structure"),
- `libraries/insulators/*.olsi` — insulator assemblies ("OLS insulator").

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
| `body` | no | Outline polylines (local axes). |

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
| `v_string` | two legs from ±`half_width` to a clamp below | tension-only legs (one goes slack in wind) |
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
cargo run -p ols-cli -- structures derive my-line.ols.json --out derived/
```

writes `derived/structures/*.olss`, `derived/insulators/*.olsi` and `derived/my-line-typed.ols.json` (the line referencing the types). Each structure's rotation is taken from its route (bisector of its back and ahead spans), attachment layouts that match within 1 cm become one type, and insulators are classified from the geometry (section ends are always strain assemblies; other clamps hanging below their attachment, swung less than 45°, are suspension strings) and grouped by kind, length and weight. The self-check reports how closely the derived libraries reproduce the attachment points and insulator lengths.

On a real 78-structure line (local check, data not in the repository) every tower turned out to be individual — each has its own layout — while about 1 500 attachments reduced to 33 insulator assemblies; the self-check reproduced all attachment points and lengths within 1 cm. Weights, stiffness and wind areas cannot be derived from geometry: fill them in.
