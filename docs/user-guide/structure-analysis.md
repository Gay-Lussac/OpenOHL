# Structure analysis (`.ohlfe`, `.ohlp`)

Besides the allowable spans and load trees of a structure type ([Structure libraries](structure-library.md#capacity)), OpenOHL can check a structure by **analysis**:

- a **structure model**: a 3D finite-element model of a lattice tower, frame or guyed mast — member forces and how much of its capacity each member uses. It is the members of a type that is a full design ([detail level 3](structure-library.md#detail-levels): every member with a section, a joint with a `support` — springs alone do not count), in the type's own `.ohls`; or a separate `.ohlfe` file the type names;
- a **pole** (`.ohlp`, or the structure's own `[pole]` table): wood, steel or concrete pole — bending moment along the pole against its moment capacity.

Every structure of that type is then checked in every load case with the factored attachment loads ([Load cases](loads.md)). The theory is in [Structure analysis](../theory/structures-fe.md).

```toml
# In the structure type (.ohls), before the [[point]] tables:
model = "DL380-tangent.ohlfe"   # optional: a separate model, relative to the .ohls file
pole = "wood-14m"               # id of a .ohlp file in the same folder (or its file name),
                                # or a [pole] table in the file (Structure libraries › Poles)
```

In a project, the `.ohlfe` and `.ohlp` files of the structure folder are copied into the project file when it opens, and read from there after (see [Library editors](desktop-app.md#library-editors)); a model in a subfolder is not, so keep them next to the `.ohls`.

All values in these files are **your data** (drawings, supplier tables, the pole manufacturer or the design code). OpenOHL ships only invented examples: `examples/synthetic-line/Structures/type-02.ohls` (a small lattice tower, its members in the type) and `steel-pole-30m.ohlp`.

## Structure models (.ohlfe)

The tables below go into the structure type itself (a full design), or into a separate `.ohlfe` file with this header:

```toml
format = "ohl-structure-model"
version = 1
name = "Example tower"

[analysis]
nonlinear = true          # P-Delta (default); false: first order
self_weight = true        # members' weight × the load case's vertical factor (default)
drag = 1.0                # member wind: drag factor on the profiles (default 1)
stabilise = true          # hold joint directions no member stiffens (default; below)
dead_load_factor = 1.0    # members' self-weight × this: bolts, plates (default 1; below)

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
attachment = "A1"         # carries the type's point A1 (version 1: [set, phase])

[[member]]
id = "leg1"
nodes = ["B1", "L1A"]
section = "leg"           # the section library's first, then a [[section]] table
kind = "truss"            # truss (default) | frame | cable | tension (version 2)
k = 1.0                   # effective length factor for buckling
# lk = "2.4 m"            # or the effective length itself (instead of k × length)
# capacity_tension = "180 kN"      # the member's capacities, instead of the computed
# capacity_compression = "95 kN"   #   ones (compression: buckling included)
# dead_load_factor = 1.3  # its self-weight × this, instead of [analysis]'s
# frame: reference = [0, 0, 1] (a direction in the local x–y plane), roll = "0 deg"
# or: orientation = "x+y-"  # an angle's legs (local y, z) towards +x and −y
# cable: pretension = "15 kN" (tension only)

[[load]]                  # optional: the model's own load case
node = "A1"
force = ["4 kN", "0 kN", "-9 kN"]
```

Units: lengths, areas (`mm2`, `cm2`, `m2`), stresses, forces as elsewhere; second moments in `cm4`, `mm4`, `m4`, `in4`; section moduli in `cm3`, `mm3`, `m3`, `in3`; moments in `kN m`, `N m`, `kip ft`. A plain number is SI.

**Attachments**: a node with `attachment = "label"` (in a structure type, also a `[[point]]` with `node = "…"`) receives the factored loads at that point of the structure type (where strings hang): transverse along x, longitudinal along y, vertical downwards. A `[[point]]` along a member (`member`, `at`) shares its loads to the member's two joints: (1 − `at`) to the first, `at` to the second — the same forces on the rest of the structure as a joint there, without the bending of that member between its joints. Version 1 models' `attachment = [set, phase]` names the point `A<set>-<phase>`. A loaded point without a node or member in the model is reported in the usage notes and its load is left out.

**Sections from the section library**: a member's `section = "<id>"` is
looked up first in the project's [section library](section-library.md)
(`.ohlsec`), then in the file's own `[[section]]` tables; a library
section of the same id as a table replaces it. Its properties are the
profile's (area, iy, iz, the torsion constant — iy + iz for a `general`
section without one —, the minor principal second moment for buckling,
the elastic moduli) and its material's (E, G, density, yield; no
allowables, so the capacity comes from the yield). A library section's
material missing, or an id in neither place, is refused, naming it. A
member whose section comes from the library has a **profile**: it takes
the member wind (below). Without a section library (a line file with no
`Sections/` folder, `ohl structures fe` without `--sections`) the tables
alone are read, as before.

**Members**: trusses take axial force only; frames are 3D beams (six degrees of freedom at their nodes); cables take tension only and may be pretensioned — a cable that would be compressed goes slack (guys); `tension` members are trusses in tension and in compression up to their own buckling load, bowed beyond it (slack without a second moment), without pretension (below). Truss-only nodes have no rotations. A model file with a `tension` member is version 2 (an older program says it is newer).

**Supports and springs**: `support` fixes directions of a joint, `spring` holds them elastically (a foundation's stiffness: N/m for x, y, z, N·m/rad for rx, ry, rz). The **reactions** are reported at every joint with either: a fixed direction gives the support's force, a sprung one the spring's force (−k × its travel), translations and rotations alike, first order and with P-Delta — so Σ reactions balance Σ loads, self-weight included, whether the structure stands on supports, springs or both (the out-of-balance of OHL-Structure's FE results, the sum `ohl structures fe` prints). A direction given both a support and a spring does not move: the support takes everything, the spring nothing.

### Tension-only members

Tower programs model light bracing that buckles under a small compression as **tension-only**. `kind = "tension"` carries tension as a truss and compression **up to its own buckling load** and no more:

- its buckling load is the elastic (Euler) load \\( N_{cr} = \pi^2 E I_{min} / L_k^2 \\) from its own section and length — \\( L_k \\) its `lk` when it gives one, else its `k` × its length, `k` = 1 by default (pinned ends);
- compressed less than that, it is a truss (a stocky tension-only member solves exactly as a truss);
- shortened further, it is **bowed**: it carries \\( N_{cr} \\) and no more (no stiffness of its own; its force stays);
- a section without a second moment (`i_min`, `iy`, `iz` all 0) has \\( N_{cr} = 0 \\): the member goes **slack** in compression (force 0), as before.

So the light compression real bracing takes is in the results, and a part held only by such members is not left free. They are not sagging cables (use `cable` for guys). The solver, the way tower programs do it:

1. solves the model with every member;
2. **bows** (or slackens) each tension-only member compressed beyond its buckling load, and **puts back** each bowed or slack one that the structure now loads less;
3. solves again, until the set does not change.

Tension-only members that meet at a joint on one line — a diagonal drawn in two halves through its crossing — count as **one tie**: they bow (or go slack) or come back together, by the tie's stretch from end to end, at the smallest buckling load of its halves (a half alone would only hang from the crossing). Where a joint on such a line is held by something else — a load, a spring, another member — the halves can carry different forces: when the tie would hide a compressed half, or when the set goes round (a cycle: some members going taut and slack in turn), the tie is **split** and its members switched one by one, and the iterations run again.

With P-Delta the set is found first order, then the model is solved with P-Delta from that set (each solve converged before the set is updated; the set is checked again after the P-Delta solve). Members left out together can leave a panel free to sway that one of them alone would hold — both diagonals of an X panel shortened by its legs under a heavy vertical load: one of them is stretched as soon as the panel sways. A solve that becomes a mechanism after members were bowed or slackened is therefore repeated with only the more shortened half of them out (down to one; when even one member's leaving makes a mechanism, the structure needs it beyond its buckling load, and the mechanism is reported).

A joint held only by members gone slack or bowed — the crossing of an X bracing whose compressed diagonal is out, a mid-joint of two such diagonals — has no stiffness of its own in some directions; the joint stabilisation (below) holds them **exactly as for a truss-only joint**, on the model of each solve's active members. Such a joint carries only what is loaded on it in that direction (its self-weight share, as for any stabilised joint; reported in the notes, refused above the same limits). Whether a member out is loaded again is judged with its joints there placed where the members out at them are in balance, so the two halves of a diagonal through a held crossing shorten alike.

Results: a **slack** member has force 0 and uses 0 % (`slack`); a **bowed** member's force is its buckling load (compression) and what governs says `bowed: carries its buckling load 0.83 kN — compression 0.8 kN of 355.0 kN`. A tension-only member's compression is checked against its given compression capacity, else the allowable compression, else area × yield — not against the Euler load again: the solve has taken buckling into account. The notes say `model: tension-only members: 3 of 140 slack, 12 bowed (4 solve(s)): D3, …; bowed (carry their buckling load): D12 0.83 kN, …` (the structure usage notes, OHL-Structure's FE results, where members are marked *slack* or *bowed*, and `ohl structures fe`). A model without tension-only members is solved exactly as before; so is one whose sections give no second moment (all its tension-only members slack in compression, as before).

**Example** (`benchmarks/structures-fe`, `structfe-011`): an X-braced panel 4 m × 3 m, 10 kN across its top, diagonals of 1000 mm² and 1 cm⁴: the compressed diagonal (5 m) bows at π² × 210 GPa × 1 cm⁴ / (5 m)² = 0.83 kN, the stretched one carries 12.50 − 0.83 = 11.67 kN (12.50 kN without a second moment, the other then slack).

#### When the set does not settle (last resort, approximate)

When single members still go taut and slack in turn after the ties were split — or after 1000 solves in all — the program does **not** stop; as a last resort it:

1. keeps the members going round **active, as ordinary truss members** (they may then carry compression beyond their buckling load, which a tension-only member cannot);
2. solves again, the other tension-only members still switched as above (repeated if another set goes round; at worst every tension-only member is kept active);
3. **flags the result**: the notes say `tension-only members kept active because the set did not settle: D12, D14 — approximate (last resort)` (the structure usage notes, OHL-Structure's FE results, `ohl structures fe`); each of those members' usage says *kept active (did not settle)*; and the structure's usage is marked **approximate** — never a plain pass (the usage CSV's `incomplete` column says `approximate: fe (tension-only members kept active, set did not settle)`, `ohl line` lists the structures and cases, OHL-Design colours them as approximate).

No model tested so far needs it — the case that used to (a pair of diagonals in halves through a joint 1 mm off the line, held otherwise only by a weak spring) now settles with its ties split. If one does, check the members named and the part around them: a part the model holds only through them. A better update of the set, which would always settle, is on the roadmap for later.

**Joint stabilisation** (`stabilise = true`, the default). Towers drawn as trusses — as most tower programs draw them — have joints that no member stiffens in some direction: a joint whose members all lie in one plane (the joints of a K or diamond bracing in a tower face, a bracing crossing joined at its crossing) is free across that plane; a joint of members on one line is free across the line; a chain of frames on one line, with no other frame and no rotational support at its joints, is free to spin about it. Solved as they are, such models are mechanisms. With stabilisation, when the solve finds a mechanism (or buckling), these directions are found from the geometry and held — exactly when the direction is along x, y or z (that degree of freedom removed, as a support would), else by a spring along it as stiff as the joint's stiffest member — and the model is solved again. A model that solves without it is solved exactly as before, bit for bit: the stabilisation only starts when the first solve fails. The usage notes (and the FE results of the structure application, and `ohl structures fe`) then say what was held: `model: stabilised 9 joint direction(s) without stiffness at 9 joint(s): L1 y, R1 y, …; largest force held 0.130 kN at M2 (0.45 % of the reactions)`. A held direction carries only what is loaded along it — the out-of-plane part of the self-weight at a joint of a battered face, say, which the structure then does not carry. When one carries more than 1 % of the loads *and* more than a quarter of its own joint's load (a load across a plane no member holds), the model is refused naming the joints (`mechanism: joint(s) M2 loaded in a direction no member holds …`). What is not one joint's own freedom stays a **mechanism**: a part that can move as a whole (a missing support, a frame of four bars without a diagonal) is reported with the joints that move (`mechanism: node C is free to move in x, with 2 joints: C, D …`). With `stabilise = false` such models fail as mechanisms, as before.

**Dead-load factor** (`dead_load_factor`, default 1): the members' self-weight is multiplied by it — the allowance for what the members do not model, bolts, gusset plates, galvanising (tower programs typically use 1.1–1.4 for a lattice tower, up to about 2.4 on light bracing). A member's own `dead_load_factor` replaces the structure's for it (set per member, or for a whole group in **Design › Groups**). It scales the self-weight loads exactly (the load case's vertical factor still applies on top) and the weight the structure application adds up for its load sum; `ohl structures fe` shows the members' weight with and without the factors. 1, or none, changes nothing.

**Usage** per member, against the section's capacity times the load case's strength factor: tension against the allowable tension (else area × yield); compression against the allowable compression (else area × yield), but not above the Euler load π²EI/(kL)² — kL the member's `lk` when it gives one, else its `k` × its length; frames add N, My and Mz ratios. Without a design-code column curve, the Euler check is optimistic for stocky members — write the code's allowable compression in the section for a design check, or the member's own capacities: a member's `capacity_tension` and `capacity_compression` (forces, e.g. from a tower program's design check of that member and its connection) replace the computed ones in its usage — the compression one the buckling check too — and what governs says so (`tension 112.0 kN of 180.0 kN (given)`). Without them the usage is computed as before.

**Orientation of a section** (`reference`, `roll`, or `orientation`): the member's local y and z are the section's y and z (an angle's legs run along +y and +z from its heel; a section's `iy` is its bending in the local x–z plane). `reference` gives a direction in the local x–y plane (default global z, global x for a vertical member) and `roll` turns y and z about the member. `orientation = "x+y-"` gives them instead from the structure's axes: the first leg (the section's y, its width b) towards +x, the second (z, the height h) towards −y, each taken normal to the member (the local y is the first direction's part normal to the member, z = x × y). The axes are right-handed: when the two directions are the mirror image of that for the member's direction (from its first joint to its second), an angle with equal legs (iy = iz and wy = wz) has its legs swapped, and one with unequal legs is refused with the order to give (or turn the member's nodes). A direction along the member, two directions on one line, or `orientation` with `reference` or `roll` are refused. It turns the bending stiffness (frames) and the profile's width in the member wind; the sections' product of inertia is not used (bending about the section's y and z).

## Member wind (body wind at level 3)

When members have profiles from the section library, the wind of the load
case blows on them: per member, the weather case's wind pressure × the
load case's transverse factor × `drag` × the profile's **width** seen by
the wind × the member's length × sin²θ (θ between the wind and the
member: only the wind's component normal to the member counts), normal to
the member in the plane of the member and the wind; half goes to each of
its joints. The width is the profile's outline across the wind's
direction in the member's local y–z plane (the section's y and z, the
same axes as `iy`, `iz`: set by `reference` and `roll`). In the line run
the wind blows across the line from the weather case's wind side, as on
the wires and the poles.

Approximations: no member shields another (a lattice's far face takes the
full wind), no solidity or shape factors from a code — `drag` (one factor
for the whole structure, default 1) is the only one; an angle's legs do
not shield each other ([Wind on the profile](section-library.md#wind-on-the-profile));
the load is lumped at the joints, so a member's own bending under its
wind is left out (the joints carry the same forces).

The type's wind bands (`[[wind.band]]`) are loads at no point of the
structure: they are in the structure loads (and the CSV), never in the FE
model, so the body wind is not counted twice. The usage notes say when
the member wind was used: `model: member wind from the section profiles,
12.3 kN across the line (the type's wind bands not in the model)`.
Without profiles (sections from the tables only) there is no member wind
and nothing changes.

## Poles (.ohlp)

```toml
format = "ohl-pole"
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
# wall = "6 mm"                # hollow section, or "solid" (none: solid)
elasticity = "11 GPa"          # optional: P-Delta amplification
density = "650 kg/m3"          # optional: the pole's weight in P-Delta
drag = 1.0                     # shape factor on the wind pressure
```

The check takes the factored loads at the **type's points** (horizontal loads × height, vertical loads × their offset from the axis, both directions combined) and the **wind on the pole** (the weather case's wind pressure × the transverse factor × drag × diameter, across the line), at 41 heights from the ground line to the top. With `elasticity`, a frame model of the tapered pole gives the P-Delta amplification of the ground-line moment, applied at every height. Usage: the largest moment over (strength factor × capacity) — for a tapered wood pole it may be above the ground line.

### Poles in the structure file (`[pole]`)

A structure may describe its pole itself ([Poles](structure-library.md#poles)): its outline by height (round or polygonal) and the same level 3 fields as a `.ohlp`. The pole check reads it through the **same conversion** as a `.ohlp` file and checks it alike:

- the heights of the outline above the feet are the heights above the ground line; the diameter between them is linear (a `.ohlp`: linear from the ground line to the top). An outline of two heights starting at the feet is exactly a `.ohlp` taper: the example pole written in the structure file gives the `.ohlp`'s numbers to the last digit (a test);
- a polygonal pole: its section at each height is the regular polygon of that width across flats (hollow by the wall), its elastic modulus taken at a corner; its width across flats takes the wind;
- `wall` is a thickness or the word `"solid"` (no hollow; any other word is refused, naming `wall`); solid checks exactly as a `.ohlp` without a wall (a test);
- a `section` of the section library gives the wall (unless `wall` is typed: a thickness, or solid), and its material the elasticity and density not given in the table;
- the pole check runs at level 3 only: a `[pole]` with an outline but no material, strength or wall (level 2) is not checked (the type then needs a capacity, as before).

`ohl structures pole --type structure.ohls` without a `.ohlp` checks the type's own pole (its `[pole]` table, or the `.ohlp` it names), with `--sections` for a section it names.

## Extensions in the model

A placed structure standing on an extension that has a **piece** (its own joints and members below the feet) or **leg extensions** is checked on the model **with that extension** ([Extension bodies](structure-library.md#extension-bodies-and-leg-extensions)): the piece's joints and members added (all their fields: section, kind, support…), the supports of the body's joints the piece reaches dropped, a leg's support joint lowered along its leg member. The loads come in at the same points (the body above is not moved in the model: the extension's `dz` raises the whole, which changes nothing in the statics). Each extension's model is built once per usage run; structures of the same type on another extension, or on none, use theirs. An extension without a piece or legs uses the model as written (as before). A separate `.ohlfe` model has no extensions.

`ohl structures fe structure.ohls --extension "+3"` solves the model standing on extension `+3`.

OHL-Structure's FE results solve the same model on the extension chosen in the panel, and the structure report (`ohl-structure-report --extension "+3"`) too ([Structure application](structure-app.md#results)). On an extension the type's wind bands rise by its `dz` (the same force, acting higher; [Structure loads](loads.md)); the pole check is the same on every extension (the pole and its points rise together).

## In the application and the line run

The **Structure loads › Usage** page and `ohl line … --usage-csv` show the two checks as **fe** and **pole** next to the spans and loads methods; the structure's usage is the largest. What governs names the member (`member m174 compression 7.9 kN of 23.5 kN (buckling)`) or the pole section (`moment 267.3 kN m of 900.0 kN m × 1 at 0.0 m (P-Delta × 1.022)`). A model that does not solve (mechanism, buckling, exit clause) counts as not allowed, with the reason.

## From the command line

One model with the loads of one structure, from the CSV of `ohl line --loads-csv`:

```bash
ohl line line.ohl.json --weather cases.ohlwc --load-cases loads.ohllc --loads-csv loads.csv
ohl structures fe Structures/type-02.ohls --loads loads.csv --structure T2 [--case wind] [--csv members.csv]
ohl structures fe Structures/type-02.ohls           # the model's own [[load]]s (an .ohlfe works too)
ohl structures fe Structures/type-02.ohls --sections Sections --wind 840 --wind-dir 180
ohl structures pole Structures/steel-pole-30m.ohlp --type Structures/type-05.ohls \
    --loads loads.csv --structure T6 --wind-pressure 840
```

`ohl structure …` works too. Options: `--strength` (factor on the capacities), `--weight-factor` (self-weight), `--linear` (first order), `--top N` (members printed per case), `--sections` (a `.ohlsec` file or folder: the section library, looked up first), `--wind` (member wind: factored pressure, Pa, added to every case; needs members with profiles) and `--wind-dir` (where it blows to, degrees from x — right of the line — towards y, ahead; default 0); `ohl line` takes `--sections` too (default: the `Sections` folder next to the line or project file); for poles `--wind-pressure` (factored, Pa), `--wind-from left|right`, `--no-p-delta`. Each case prints the iterations and time, the largest displacement, the sum of the reactions and the members using most of their capacity.
