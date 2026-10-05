# Structure analysis (.olsfe, .olsp)

Besides the allowable spans and load trees of a structure type ([Structure libraries](structure-library.md#capacity)), OLS CAD can check a structure by **analysis**:

- a **structure model** (`.olsfe`): a 3D finite-element model of a lattice tower, frame or guyed mast — member forces and how much of its capacity each member uses;
- a **pole** (`.olsp`): wood, steel or concrete pole — bending moment along the pole against its moment capacity.

A structure type names them; every structure of that type is then checked in every load case with the factored attachment loads ([Load cases](loads.md)). The theory is in [Structure analysis](../theory/structures-fe.md).

```toml
# In the structure type (.olss), before the [[attachment]] tables:
model = "DL380-tangent.olsfe"   # relative to the .olss file
pole = "wood-14m"               # id of a .olsp file in the same folder
```

All values in these files are **your data** (drawings, supplier tables, the pole manufacturer or the design code). OLS CAD ships only invented examples: `examples/synthetic-line/Structures/type-02.olsfe` (a small lattice tower) and `steel-pole-30m.olsp`.

## Structure models (.olsfe)

```toml
format = "ols-structure-model"
version = 1
name = "Example tower"

[analysis]
nonlinear = true          # P-Delta (default); false: first order
self_weight = true        # members' weight × the load case's vertical factor (default)

[[material]]
id = "steel"
elasticity = "210 GPa"
shear_modulus = "81 GPa"  # default E / 2.6
density = "7850 kg/m3"
yield = "355 MPa"         # optional: capacities from A·fy and W·fy

[[section]]
id = "leg"
material = "steel"
area = "1500 mm2"
i_min = "80 cm4"          # buckling (default: the smaller of iy, iz)
# frames also: iy, iz, torsion (J), wy, wz; allowables instead of the yield:
# tension, compression (N), moment_y, moment_z ("kN m")

[[node]]
id = "B1"
point = ["-2.5 m", "-2.5 m", "0 m"]   # structure axes: x right, y ahead, z up
support = "pinned"        # fixed | pinned | a list of "x", "y", "z", "rx", "ry", "rz"
# spring = [kx, ky, kz, krx, kry, krz]  (N/m, N·m/rad)

[[node]]
id = "A1"
point = ["6 m", "0 m", "30 m"]
attachment = [1, 1]       # carries the type's attachment set 1, phase 1

[[member]]
id = "leg1"
nodes = ["B1", "L1A"]
section = "leg"
kind = "truss"            # truss (default) | frame | cable
k = 1.0                   # effective length factor for buckling
# frame: reference = [0, 0, 1] (a direction in the local x–y plane), roll = "0 deg"
# cable: pretension = "15 kN" (tension only)

[[load]]                  # optional: the model's own load case
node = "A1"
force = ["4 kN", "0 kN", "-9 kN"]
```

Units: lengths, areas (`mm2`, `m2`), stresses, forces as elsewhere; second moments in `cm4`, `mm4`, `m4`, `in4`; section moduli in `cm3`, `mm3`, `m3`, `in3`; moments in `kN m`, `N m`, `kip ft`. A plain number is SI.

**Attachments**: a node with `attachment = [set, phase]` receives the factored loads of that attachment (summed over a V-string's legs): transverse along x, longitudinal along y, vertical downwards. An attachment without a node is reported in the usage notes and its load is left out.

**Members**: trusses take axial force only; frames are 3D beams (six degrees of freedom at their nodes); cables take tension only and may be pretensioned — a cable that would be compressed goes slack. Truss-only nodes have no rotations; a node held by nothing in one direction is a mechanism and is named in the error.

**Usage** per member, against the section's capacity times the load case's strength factor: tension against the allowable tension (else area × yield); compression against the allowable compression (else area × yield), but not above the Euler load π²EI/(kL)²; frames add N, My and Mz ratios. Without a design-code column curve, the Euler check is optimistic for stocky members — write the code's allowable compression in the section for a design check. Wind on the tower body is not included yet (only the attachment loads and the members' weight).

## Poles (.olsp)

```toml
format = "ols-pole"
version = 1
id = "wood-14m"
name = "Wood pole 14 m"
material = "wood"              # wood | steel | concrete | other
length = "14 m"
embedment = "2 m"
top_diameter = "190 mm"        # or top_circumference
ground_diameter = "290 mm"     # or ground_circumference (at the ground line)
fibre_stress = "55 MPa"        # wood: capacity f·πD³/32 at each height
# moment_capacity = "180 kN m" # or: at the ground line (steel, concrete)
# [[capacity]] height = "0 m"; moment = "180 kN m"   # or: by height
# wall = "6 mm"                # hollow section (for the stiffness)
elasticity = "11 GPa"          # optional: P-Delta amplification
density = "650 kg/m3"          # optional: the pole's weight in P-Delta
drag = 1.0                     # shape factor on the wind pressure
```

The check takes the factored loads at the **type's attachment points** (horizontal loads × height, vertical loads × their offset from the axis, both directions combined) and the **wind on the pole** (the weather case's wind pressure × the transverse factor × drag × diameter, across the line), at 41 heights from the ground line to the top. With `elasticity`, a frame model of the tapered pole gives the P-Delta amplification of the ground-line moment, applied at every height. Usage: the largest moment over (strength factor × capacity) — for a tapered wood pole it may be above the ground line.

## In the application and the line run

The **Structure loads › Usage** page and `ols line … --usage-csv` show the two checks as **fe** and **pole** next to the spans and loads methods; the structure's usage is the largest. What governs names the member (`member m174 compression 7.9 kN of 23.5 kN (buckling)`) or the pole section (`moment 267.3 kN m of 900.0 kN m × 1 at 0.0 m (P-Delta × 1.022)`). A model that does not solve (mechanism, buckling, exit clause) counts as not allowed, with the reason.

## From the command line

One model with the loads of one structure, from the CSV of `ols line --loads-csv`:

```bash
ols line line.ols.json --weather cases.olswc --load-cases loads.olslc --loads-csv loads.csv
ols structures fe Structures/type-02.olsfe --loads loads.csv --structure T2 [--case wind] [--csv members.csv]
ols structures fe Structures/type-02.olsfe          # the model's own [[load]]s
ols structures pole Structures/steel-pole-30m.olsp --type Structures/type-05.olss \
    --loads loads.csv --structure T6 --wind-pressure 840
```

`ols structure …` works too. Options: `--strength` (factor on the capacities), `--weight-factor` (self-weight), `--linear` (first order), `--top N` (members printed per case); for poles `--wind-pressure` (factored, Pa), `--wind-from left|right`, `--no-p-delta`. Each case prints the iterations and time, the largest displacement, the sum of the reactions and the members using most of their capacity.
