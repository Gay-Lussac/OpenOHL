# Structure exports: IFC, SAF, CSV

Status: **built, in active development (not approved yet)**; pre-alpha (first written 2026-10-17; checked with free tools on 2026-10-19 — IfcOpenShell for IFC, a standard-library reader for SAF, [below](#checks-with-free-tools); no export has been imported into another program yet), phase 7 of [Building structures](structure-building.md)
(decision 12: export to IFC and to analysis and steel-detailing programs; other tower programs later). Crate
`apps/structure/ohl-structure-export` ([crates](structure-app.md)); user
guide: [The structure application › Export](../user-guide/structure-app.md#export).

A **level 3** structure (joints, members with cross-sections, supports) is
written with its cross-sections (from the [cross-section library](../user-guide/section-library.md)
or the file's own `[[section]]` tables), materials, supports, member kinds
and load cases (a [load table](../user-guide/structure-load-cases.md)).
Only open, documented formats are written; no proprietary binary format.

## What the target programs import (research, 2026-10-17)

Public documentation only (the programs' own help pages, not named
here); the format pages read are listed under [Sources](#sources).

| Program | Format it imports for an analysis model | What it makes of it |
|---|---|---|
| **An analysis program** (finite elements) | **SAF** (import and export) | the analysis model: nodes, members, cross-sections, materials, supports, load cases and loads |
| | **IFC** 2x3, 4.0, 4.3 | with a **Structural Analysis View** the analytical elements (nodes, members, surfaces) are created directly; materials and cross-sections mapped through conversion tables. Other views (Reference, Coordination) are shown in the IFC viewer and native members can be derived from them |
| **A steel-detailing program** | **IFC** as a reference model, converted into native parts by its IFC object conversion | IfcBeam, IfcColumn, **IfcMember**… with parametric I, L, U, C, T, Z, rectangle and circular profiles, or an arbitrary polygon profile. The conversion needs **IFC2x3 Coordination View 2.0**: its help says IFC4 and newer are not supported in the conversion. An IFC4 file is shown as a reference model only |
| **A structural design program** of the same vendor | none of the open formats: its help lists imports from other vendors' programs only, IFC as export only | — (no import path; go through the detailing or the analysis program) |
| Other analysis and BIM programs | SAF [S1] | (not targets of this work; listed by saf.guide as SAF supporters) |

SAF is not listed for the steel-detailing program in saf.guide's "Who
supports SAF?" (last updated August 2022, which lists the analysis
program with SAF **1.0.5**) [S1].

## Formats chosen

| Format | For | Holds |
|---|---|---|
| **IFC4** (ADD2 TC1), header `ViewDefinition [StructuralAnalysisView]` | analysis programs (analysis model with load cases), BIM viewers | members as `IfcMember` with profile solids **and** the structural analysis model with load cases, in one file |
| **IFC4**, members only, `ViewDefinition [ReferenceView_V1.2]` | BIM viewers, coordination | the members with profile solids, materials |
| **IFC2x3** (TC1), `ViewDefinition [CoordinationView_V2.0]` | **steel-detailing programs** that convert IFC2x3 only (IFC object conversion into native parts) | the members with parametric profiles and materials, in a site, building and storey (the analysis model of IFC2x3 is not written) |
| **SAF** 2.2.0 (xlsx) | analysis programs that read SAF | materials, cross-sections, nodes, members, supports, load group, load cases, point and line loads |
| **CSV** tables | anything else | `materials.csv`, `sections.csv`, `nodes.csv`, `supports.csv`, `members.csv`, `load_cases.csv`, `loads.csv` |

All are written from plain data (`ohl_structure_link::ExportStructure`,
made by `ohl_structure_link::export_structure` from the structure file,
the cross-section library and a `LoadTable`): SI units, the structure's axes
(x right, y ahead, z up), no map coordinates. GlobalIds (IFC) and `Id`s
(SAF) are hashes of the structure's id and each object's id, so an
export of the same structure gives the same ids, and an object keeps its
id when others change. With the export time left at 0 (the command
line's default) the same structure gives the same bytes in every format.

## What maps

### Common (`ExportStructure`)

| OpenOHL | Export data |
|---|---|
| `[[node]]` `point`, `support` (`fixed`, `pinned`, a list of x, y, z, rx, ry, rz), `spring` | joint, held directions, springs (N/m, N·m/rad) |
| `[[member]]` `nodes`, `section`, `kind` (truss, frame, cable, tension), `bending`, `check`, `reference`, `roll`, `k`, `pretension` | member with its **local axes** worked out exactly as the FE model does (`member_axes`: y in the plane of x and the reference — global z, or global x for a vertical member — then the roll about x); k and pretension carried along |
| cross-section of the library (`.ohlsec`) | profile `Shape` and dimensions, A, Iy, Iz, It, Wpl, the centroid relative to the outline box |
| `[[section]]` table | A, Iy, Iz, It only (no shape) |
| material (library or `[[material]]`) | E, G, density, yield |
| load table rows (`ohl_structure_loads::LoadTable`) | per case: a point's load at the joint carrying it (a point is always on a joint since 2026-10-21; a file that placed one along a member is read with a joint inserted there); a joint's load; a member's surface load as a **uniform load** (total ÷ length); right, ahead, down → x, y, −z |
| recipe `self_weight` | the case's self-weight factor (case `model`: `[analysis] self_weight`) |

In the structure application the load table is the Load table panel's:
for a placed structure the line's forces at that place (its cases first,
then the recipes evaluated with the line's values) and the values typed
over the table. The command-line tool (and the application for a
structure opened on its own) takes the file's recipes evaluated without
line values — rows needing a line value are left out and counted in a
note; surface loads on the members' areas from the cross-section library
(`ohl_structure_link::SectionAreas`) — and its `[[load]]` tables as case
`model`; a load table written by the application (CSV) can be given
instead.

### IFC4

| Export data | IFC4 |
|---|---|
| structure | `IfcProject` (units: m, rad, N, Pa, kg, m², m³ and the derived units of the values written), `IfcSite` at the origin |
| member (physical) | `IfcMember` (`.MEMBER.`), `ObjectType` = kind; placement at the first joint with Z along the member and X along its local y; representations `Axis` (`Curve3D` line) and `Body` (`SweptSolid`: `IfcExtrudedAreaSolid` of the profile over the length); property set `OHL_Member` (Cross-section, Kind, EffectiveLengthFactor, Pretension, From, To; where set: Bending `IFCBOOLEAN(.T.)` for a tension-only member that bends, Check `IFCBOOLEAN(.F.)` for a member not design-checked) |
| profile: equal / unequal angle | `IfcLShapeProfileDef` (Depth h, Width b, Thickness t, FilletRadius r) |
| channel | `IfcUShapeProfileDef` (Depth h, FlangeWidth b, WebThickness tw, FlangeThickness tf, FilletRadius r) |
| I-section | `IfcIShapeProfileDef` (OverallWidth b, OverallDepth h, tw, tf, r) |
| rectangular hollow | `IfcRectangleHollowProfileDef` (XDim b, YDim h, WallThickness t) |
| circular hollow | `IfcCircleHollowProfileDef` (Radius d/2, WallThickness t) |
| round bar | `IfcCircleProfileDef` (Radius d/2) |
| polygonal hollow (pole) | `IfcArbitraryProfileDefWithVoids` (outer and inner `IfcPolyline`, a side at the bottom) |
| `general`, `[[section]]` table | `IfcProfileDef` by name only (IFC4 allows it); no solid, the axis only |
| profile position | the parameterised profile's `Position` moves the outline box so that the **centroid** lies on the member's axis (IFC4 places a parameterised profile at the centre of its box); profile x = member local y, profile y = local z |
| cross-section and material | `IfcMaterialProfile` / `IfcMaterialProfileSet` / `IfcMaterialProfileSetUsage` (cardinal point 10, the centroid), associated with the physical and the analysis member; `IfcProfileProperties` `Pset_ProfileMechanical` (CrossSectionArea, TorsionalConstantX) and `OHL_Section` (SecondMomentLocalY, SecondMomentLocalZ: about the member's local y and z) |
| material | `IfcMaterial` (category from E and density, below) with `IfcMaterialProperties` `Pset_MaterialMechanical` (YoungModulus, ShearModulus, PoissonRatio), `Pset_MaterialCommon` (MassDensity), `Pset_MaterialSteel` (YieldStress) |
| analysis model | `IfcStructuralAnalysisModel` (`.LOADING_3D.`, a shared placement at the origin), items assigned by `IfcRelAssignsToGroup`, serving the site (`IfcRelServicesBuildings`) |
| joint | `IfcStructuralPointConnection` with an `IfcVertexPoint` topology; a support as `IfcBoundaryNodeCondition`: held `IFCBOOLEAN(.T.)`, free `.F.`, a spring as `IfcLinearStiffnessMeasure` / `IfcRotationalStiffnessMeasure` |
| member (analysis) | `IfcStructuralCurveMember` with an `IfcEdge` between the joints' vertices; `Axis` = the member's local z; truss → `.PIN_JOINED_MEMBER.`, frame → `.RIGID_JOINED_MEMBER.`, cable → `.CABLE.`, tension → `.TENSION_MEMBER.`, a tension member with `bending` → `.RIGID_JOINED_MEMBER.` ([below](#tension-only-members-that-bend-and-members-not-checked)); `Description` "tension only, bending" and/or "not checked" where it applies, else none; `IfcRelConnectsStructuralMember` at both ends (no applied condition: rigid, as for every member); `IfcRelAssignsToProduct` to its `IfcMember` |
| load case | `IfcStructuralLoadCase` (`.LOAD_CASE.`, action type and source `.NOTDEFINED.`), `SelfWeightCoefficients` (0, 0, −factor) |
| load at a joint | `IfcStructuralPointAction` (no placement of its own, as the standard requires) with `IfcStructuralLoadSingleForce`, `.GLOBAL_COORDS.`, connected to the point connection |
| load along a member (a point) | `IfcStructuralPointAction` with its own placement and vertex, connected to the curve member |
| uniform load on a member | `IfcStructuralCurveAction` (`.CONST.`, `.TRUE_LENGTH.`) with `IfcStructuralLoadLinearForce` |

### IFC2x3 (steel detailing)

The members only, as above, with the differences of the older schema:
an owner history on every object (its person: family name `Unknown`, since IFC2x3's `IfcPerson` needs a name, rule WR1, and nobody's name is written), the members in a building storey
(site, building, storey), profile `Position` always given, fillet radii
left out when 0 (they must be positive there), `IfcMaterial` by name
associated with the members, no `IfcProfileDef` by name (a cross-section
without geometry gives an axis only). A detailing program's conversion
maps profile and material **names** to its catalogues (missing ones are
asked for): the profile name written is the
cross-section's id.

### SAF (2.2.0)

| Sheet | Rows | Notes |
|---|---|---|
| `Project`, `Model` | property name in column A, value in B | `SAF Version` 2.2.0, `Global coordinate system` Z vertical, `LCS of cross-section` ZYX, `System of units` Metric, `National code` EC-Standard-EN (required by SAF; OpenOHL does not tie the export to a code) |
| `StructuralMaterial` | one per material | `Type` from E and density (below), `Quality` = the material's name, E and G in MPa, unit mass kg/m³, Poisson's ratio E/2G − 1 |
| `StructuralCrossSection` | one per cross-section | **Parametric**: angle → `Angle` (H; B; t; R; R1 = h; b; t; r; 0), channel → `Channel` (H; B; t; s; R = h; b; tf; tw; r), I → `I rolled` (H; B; t; s; R = h; b; tf; tw; r), rectangular hollow → `Tube` (H; B; s; R; r1 = h; b; t; 0; 0), circular hollow → `Pipe` (D; t), round bar → `Circle` (D), parameters in mm. **General** (polygonal hollow, `general`, `[[section]]` tables): `Profile` names a `CompositeShapeDef` row. A, Iy, Iz, It, Wply, Wplz given for every cross-section |
| `CompositeShapeDef` | cross-sections without a parametric shape | polygonal hollow: the outline (counter-clockwise, with the material) and the opening (clockwise); `general`: its outline box; a `[[section]]` table: **a square of the same area** (no shape is known; its A, Iy, Iz, It are in the cross-section row) |
| `StructuralPointConnection` | one per joint | m |
| `StructuralCurveMember` | one per member | `Nodes` "first; second", `Segments` Line, `LCS` **z by vector** with the member's local z, rotation 0 (the roll is in the vector), `System line` Centre, analysis eccentricities 0; `Behaviour in analysis`: truss → Axial force only, frame → Standard, cable and tension → Tension only, a tension member with `bending` → Standard; `Layer` "OHL: tension only, bending" and/or "OHL: not checked" where it applies, else empty |
| `StructuralPointSupport` | one per supported joint | In node, Global; each direction Rigid, Free or Flexible (stiffness MN/m, MN·m/rad); `Type` Fixed, Hinged or Custom |
| `StructuralLoadGroup` | one, `LG1` | Permanent, Standard |
| `StructuralLoadCase` | one per case | Permanent, load group LG1, load type Standard; the description says the self-weight factor |
| `StructuralPointAction` | one row per non-zero component (X, Y, Z) of a load, kN | at a joint: In node; along a member: On beam, Relative position from the start |
| `StructuralCurveAction` | one row per non-zero component of a uniform load, kN/m | On beam, Uniform, Global, Length, Relative, Full, 0 → 1 |

The workbook is written by the crate itself (a zip of SpreadsheetML
parts with the workspace's `zip` crate, as `ohl-io`'s XLSX export; no
new dependency), inline strings, a fixed zip time.

### CSV

One table per file, a header row with the unit in the column name (`_m`,
`_pa`, `_n`…), SI units, structure axes. `members.csv` has `bending` and
`check` (1 or 0) after `kind`, and carries the local
y and z axes (`y_x … z_z`), `loads.csv` the place (`node`,
`member_point` with its position 0–1, `member_uniform` per metre).

### Tension-only members that bend and members not checked

A `tension` member with `bending = true` (decided by the owner on
2026-10-19: the exports **keep the bending**) bends and twists as a frame
while it is in the model, and drops out in compression beyond its
buckling load. No format written has a member that is both: IFC4's
`.TENSION_MEMBER.` and SAF's "Tension only" are axial members. So it is
written as a **frame** — what the analysis takes while it carries load —
and its tension-only behaviour as information an engineer (or an
importer) can read and act on:

| Format | Analysis behaviour | Tension-only information | Not checked (`check = false`) |
|---|---|---|---|
| IFC4 analysis view | `IfcStructuralCurveMember` `.RIGID_JOINED_MEMBER.`, both ends connected without an applied condition (rigid), as a frame member | `IfcMember` `ObjectType` and `OHL_Member` `Kind` = `tension`, `Bending` = true; the analysis member's `Description` "tension only, bending" | `OHL_Member` `Check` = false; `Description` "not checked" |
| IFC4 members only, IFC2x3 | (no analysis model) | `ObjectType` and `Kind` = `tension`, `Bending` = true | `Check` = false |
| SAF | `Behaviour in analysis` Standard (a beam) | `Layer` "OHL: tension only, bending" | `Layer` "OHL: not checked" |
| CSV | `kind` = `tension` | `bending` = 1 | `check` = 0 |

`Bending` and `Check` are written only where set (a member that bends;
a member not checked), so other members' files are unchanged. A member
not checked is exported as any other member of its kind.

**SAF:** the `StructuralCurveMember` sheet has no comment column, and a
member's `Name` is what the actions refer to, so the information goes in
`Layer`, a free-text column that also groups such members in the
importing program (prefix `OHL: `; both notes joined by `; `). No SAF
rule is assumed beyond the column taking text; whether an importer
keeps the layer is for the import test (O8). "Tension only" is not
written for such a member: an importer would make it an axial member and
lose the bending.

**What is lost:** the switch itself — no format here says that the frame
drops out (or carries only its buckling load) in compression. An
analysis in the target program treats the member as a frame in every
case unless the engineer sets it up as tension-only there.

### Material category

The cross-section library does not say what a material is; IFC's category and
SAF's `Type` are guessed from E and the density: steel 180–230 GPa,
aluminium 60–80 GPa, concrete 20–50 GPa above 2000 kg/m³, timber below
20 GPa and 1000 kg/m³, else Other.

## What is lost

- **Line values**: a structure opened on its own (or exported with the
  command-line tool) has no line values; its recipes needing one give
  rows without a value, left out (a note says how many). Open it with
  **Edit…** on the placed structure in the line application, or give a
  load table, for the full loads.
- **Moments** of `[[load]]` tables (the load table carries forces only).
- **Load combinations**, partial factors and the strength factor of a
  case: the cases are written as they are (already factored as their
  recipes say); SAF's action type is Permanent / Standard for every case
  and IFC's `.NOTDEFINED.`, because OpenOHL does not classify them.
- **Self-weight** in SAF: only as text in the case's description (SAF
  has no factor per case); in IFC4 as `SelfWeightCoefficients`.
- **Effective length factor k** and **pretension**: IFC4 property set
  `OHL_Member` and CSV only; SAF has no column for them.
- **Member wind areas**, wind bands, the structure's points, strings,
  wire points, insulators, capacities: not part of an analysis model.
- **Shapes**: SAF has no parametric polygonal tube; it goes as a general
  polygon. The root radius of angles, channels and I-sections is kept;
  toe radii and flange slopes do not exist in the library. `[[section]]`
  tables have no shape: IFC writes the profile by name only (no solid),
  SAF an equal-area square.
- **Unequal angles** keep h along the cross-section's z and b along y. IFC's
  convention puts the longer leg along the profile's y; an angle whose b
  is longer than h is written as it is (the attributes allow it).
- **IFC2x3** carries no analysis model and no material values (the
  detailing program maps names).
- **Tension-only members that bend** are frames in IFC4's analysis
  model and in SAF; their tension-only behaviour is information only
  ([above](#tension-only-members-that-bend-and-members-not-checked)).
- Coordinates are the structure's own axes; a placed structure's
  position and orientation in the line are not written.

## Open points

- **Checked with free tools only** ([below](#checks-with-free-tools)).
  The unit tests also check the STEP structure (header, numbering
  without gaps, every reference resolving, balanced quotes), the number
  of attributes of **every** entity against the IFC4 ADD2 TC1 and IFC2x3
  TC1 EXPRESS schemas, unique GlobalIds, the expected counts for
  `examples/synthetic-line/Structures/type-02.ohls`, the profile mapping
  per shape, the SAF sheets and columns read back, and that two exports
  are identical. An import into an analysis program and a
  steel-detailing program is still to be tried by someone with licences,
  with the sample files of `examples/exports/` (its README says what to
  look at in each program).
- **SAF version**: written as 2.2.0 (the sheets used are unchanged since
  1.0.5 except two optional columns of `StructuralCurveAction` added in
  2.0.0 [S5]); the analysis program was listed with 1.0.5 in 2022 [S1].
- **SAF parametric shapes**: the parameter letters are from the annex
  [S3]; their drawings were not read (images). `t` = flange and `s` =
  web thickness for `I rolled` and `Channel`, `s` = wall for `Tube`, are
  read from the letters (one SAF program's naming); an import should confirm them.
- **Project / Model sheets**: SAF says their properties are "oriented in
  columns not in rows as usual" [S4]; they are written as name–value
  pairs, column A and B.
- Exports for other tower programs: later (decision 12).
- A structure whose model is a separate `.ohlfe` (`model = "…"`) is not
  exported yet (the export builds the model from the `.ohls` itself).

## Checks with free tools

`tools/check_structure_exports.py` checks exported files; the test
`exports_pass_the_free_tools` (`apps/structure/ohl-structure-tests/tests/exports_checked.rs`)
writes the IFC4 (with and without the analysis model), IFC2x3 and SAF
exports of three structures — the example lattice
`examples/synthetic-line/Structures/type-02.ohls` (`[[section]]` tables:
no shapes), the invented `examples/exports/every-profile.ohls` (every
profile of the example cross-section library, a spring support, a load along a
member, a tension-only member that bends and a member not checked) and a body built by the generators (X panels with horizontals,
triangular crossarms, a peak; library angles) — and runs the script on
them. The test runs only when `OHL_IFC_PYTHON` names a Python with
IfcOpenShell; otherwise it says so and passes:

```bash
OHL_IFC_PYTHON=/path/to/venv/bin/python cargo test -p ohl-structure-tests \
    --test exports_checked -- --nocapture
# or on any exported files or folders:
/path/to/venv/bin/python -I tools/check_structure_exports.py out/tower.ifc out/tower.xlsx
```

Nothing is downloaded: IfcOpenShell (0.9.0 tried) is installed by the
person running the check. The example pole (`type-05.ohls`) is a pole
check structure without joints and members, so it has no level 3 model to
export; the polygonal pole profile is checked through `every-profile`.

**IFC, per file:**

- `ifcopenshell.validate.validate(f, logger, express_rules=True)`: the
  schema (attribute types, counts, enumerations, inverse cardinalities)
  and the schema's where-rules and global rules. IfcOpenShell's rule runner
  imports pytest only to rewrite `assert` statements into readable
  messages; the script gives it a stand-in that uses each rule's own
  expression as the message, so pytest is not needed (the rules are
  IfcOpenShell's, unchanged).
- Units (metre, radian, newton, pascal, kilogram), the counts of members,
  profiles by type, materials and structural analysis entities, GlobalIds
  unique and 22 characters.
- Every `IfcMember`: a local placement with orthonormal, right-handed
  axes; its `Axis` line's ends, placed, at its analysis member's joints
  (IFC4 with the analysis model); `OHL_Member` (IFC4); its `Body` built by
  IfcOpenShell's geometry kernel with a positive volume, the solid's
  centroid on the member's axis (a warning beyond 1 mm), and in IFC4 the
  volume against A × L of the cross-section (a warning beyond 3 %).
- Every profile with geometry has `OHL_Section` (IFC4).

**SAF, per file** (the standard library only; the published SAF
specification and its SDK are not used, so only what the exporter states
is checked, no rule of the specification is invented): the workbook
opens (zip, SpreadsheetML parts), every sheet's column headers equal the
lists in `saf.rs` (`columns`), the `Model` sheet's SAF Version is
`SAF_VERSION` and its units Metric, names unique per sheet, `Id`s are
UUIDs; members → cross-sections → materials, cross-sections →
`CompositeShapeDef`, members' `Nodes`, supports' `Node`, actions →
joints, members and load cases, load cases → load group all resolve;
numbers are numbers, A, E, G and the unit mass positive, a member's LCS
vector of length 1, parametric parameters numbers, contour 1 counter-
clockwise and contour 2 clockwise, relative positions within 0–1; a
warning for an E outside 100 MPa–1000 GPa or an A outside 10⁻⁷–1 m²
(units), and for Iy, Iz or It = 0.

**Results (2026-10-19, IfcOpenShell 0.9.0.post1).** Before: every IFC2x3
file failed one rule, `IfcPerson` WR1 (a person needs a family or a given
name; the owner history's person had none) — fixed (family name
`Unknown`). IFC4 had no finding. After: 12 files, 0 errors. Body solids
of all 84 shaped members built; centroids at most 0.008 mm off the axis;
volume / (A × L) 0.990–1.001 (the round bar and the circular tube are
below 1 because the geometry kernel draws circles as polygons; the others
within 0.1 % of the library's area). Warnings left: the SAF export of
`type-02` has Iy, Iz and It = 0 for its three `[[section]]` tables, which
give the area only (its members are truss members, which need only A); an
analysis program may refuse zero bending stiffness for a frame member —
give such cross-sections in the cross-section library.
Rerun on 2026-10-19 after `every-profile` gained a tension-only member
that bends and a member not checked (`Bending`, `Check` in `OHL_Member`,
the analysis members' `Description`, SAF `Layer`): 12 files, 0 errors.

**What the free tools cannot confirm** (only the target programs can):

- IFC: the **model view definitions** — Structural Analysis View, Reference
  View and IFC2x3 Coordination View 2.0 — are not checked by IfcOpenShell
  (no MVD checker); whether an analysis program creates its analytical
  members, supports and load cases from the IFC4 file; whether a
  steel-detailing program converts the IFC2x3 members into native parts,
  and how it maps the profile names (the cross-section ids) and the material
  names to its catalogues; the profile orientation (roll) a program
  shows.
- SAF: every rule of the specification itself (no schema or SDK here):
  required columns and allowed values per sheet (enumerations such as
  `Type`, `Shape`, `Behaviour in analysis`, `Force action`), the
  parameter order of the parametric shapes (`t` and `s` of `I rolled`,
  `Channel`, `Tube`; see the open points), the `Project` and `Model`
  sheets' layout (rows or columns), `National code` values, the units an
  importer assumes, and whether an importer accepts version 2.2.0 or
  needs an older one.

## Sources

- The programs' own help pages on IFC import and conversion and on SAF
  (read 2026-10-17; not listed here, so that no program is named).
- [S1] SAF, "Who supports SAF?",
  <https://www.saf.guide/en/stable/getting-started/who-supports-saf.html>.
- [S2] SAF 2.2.0 object pages: StructuralMaterial, StructuralCrossSection,
  CompositeShapeDef, StructuralPointConnection, StructuralCurveMember
  (structural-analysis-elements/…), StructuralPointSupport
  (supports-and-hinges/…), StructuralLoadGroup, StructuralLoadCase,
  StructuralPointAction, StructuralCurveAction (loads/…), under
  <https://www.saf.guide/en/stable/>.
- [S3] SAF, "Supported shapes of parametric cross-section",
  <https://www.saf.guide/en/stable/annexes/supported-shapes-of-parametric-cross-section.html>.
- [S4] SAF, "Project and model specifications",
  <https://www.saf.guide/en/stable/getting-started/project-and-model-specifications/README.html>.
- [S5] SAF, "Changelog", <https://www.saf.guide/en/stable/annexes/changelog.html>.
- [I1] buildingSMART, IFC4 ADD2 TC1 and IFC2x3 TC1 EXPRESS schemas,
  `reference_schemas/IFC4_ADD2_TC1.exp` and `IFC2X3_TC1.exp` in
  <https://github.com/buildingSMART/IFC4.x-development> (entity
  attributes and enumerations).
- [I2] buildingSMART IFC documentation (IFC 4.3 development repository,
  <https://github.com/buildingSMART/IFC4.3.x-development>, `docs/schemas`):
  IfcStructuralCurveMember (local axes from `Axis`; profile x, y along
  the member's local y, z; cardinal point 10 at the centroid),
  IfcLShapeProfileDef (position at the centre of the bounding box),
  IfcProfileDef (instantiable since IFC4), IfcStructuralPointAction (no
  placement when connected to a point item), IfcStructuralCurveAction,
  IfcStructuralLoadCase (`SelfWeightCoefficients`).
