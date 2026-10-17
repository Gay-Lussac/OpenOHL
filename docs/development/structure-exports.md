# Structure exports: IFC, SAF, CSV

Status: **built** (2026-10-17), phase 7 of [Building structures](structure-building.md)
(decision 12: export to IFC, Tekla and RFEM; PLS-TOWER later). Crate
`apps/structure/ols-structure-export` ([crates](structure-app.md)); user
guide: [The structure application › Export](../user-guide/structure-app.md#export).

A **level 3** structure (joints, members with sections, supports) is
written with its sections (from the [section library](../user-guide/section-library.md)
or the file's own `[[section]]` tables), materials, supports, member kinds
and load cases (a [load table](../user-guide/structure-load-cases.md)).
Only open, documented formats are written; no proprietary binary format.

## What each target program imports (research, 2026-10-17)

Public documentation only; the pages read are listed under
[Sources](#sources).

| Program | Format it imports for an analysis model | What it makes of it |
|---|---|---|
| **Dlubal RFEM 6** (and RSTAB 9) | **SAF** (import and export) [D2] | the analysis model: nodes, members, cross-sections, materials, supports, load cases and loads |
| | **IFC** 2x3, 4.0, 4.3 [D1] | with a **Structural Analysis View** the analytical elements (nodes, members, surfaces) are created directly; materials and cross-sections mapped through conversion tables. Other views (Reference, Coordination) are shown in the IFC viewer and native members can be derived from them |
| **Tekla Structures** | **IFC** as a reference model, converted into native parts by *IFC object conversion* [T1] | IfcBeam, IfcColumn, **IfcMember**… with parametric I, L, U, C, T, Z, rectangle and circular profiles, or an arbitrary polygon profile. The conversion needs **IFC2x3 Coordination View 2.0**: *"The IFC4.0, IFC4.1, and any newer IFC4 formats are not supported in the IFC object conversion."* An IFC4 file is shown as a reference model only |
| **Tekla Structural Designer** | none of the open formats: its help lists imports from Revit, Westok Cellbeam and FBEAM only, IFC as export only [T2] | — (no import path; go through Tekla Structures or RFEM) |
| SCIA, Graphisoft, Allplan, AxisVM, SOFiSTiK, RISA, FRILO… | SAF [S1] | (not targets of this work; listed by saf.guide as SAF supporters) |

SAF is not listed for Tekla in saf.guide's "Who supports SAF?" (last
updated August 2022, which lists Dlubal with SAF **1.0.5**) [S1].

## Formats chosen

| Format | For | Holds |
|---|---|---|
| **IFC4** (ADD2 TC1), header `ViewDefinition [StructuralAnalysisView]` | RFEM 6 (analysis model with load cases), BIM viewers | members as `IfcMember` with profile solids **and** the structural analysis model with load cases, in one file |
| **IFC4**, members only, `ViewDefinition [ReferenceView_V1.2]` | BIM viewers, coordination | the members with profile solids, materials |
| **IFC2x3** (TC1), `ViewDefinition [CoordinationView_V2.0]` | **Tekla Structures** (IFC object conversion into native parts) | the members with parametric profiles and materials, in a site, building and storey (the analysis model of IFC2x3 is not written) |
| **SAF** 2.2.0 (xlsx) | RFEM 6, SCIA and other SAF programs | materials, cross-sections, nodes, members, supports, load group, load cases, point and line loads |
| **CSV** tables | anything else | `materials.csv`, `sections.csv`, `nodes.csv`, `supports.csv`, `members.csv`, `load_cases.csv`, `loads.csv` |

All are written from plain data (`ols_structure_link::ExportStructure`,
made by `ols_structure_link::export_structure` from the structure file,
the section library and a `LoadTable`): SI units, the structure's axes
(x right, y ahead, z up), no map coordinates. GlobalIds (IFC) and `Id`s
(SAF) are hashes of the structure's id and each object's id, so an
export of the same structure gives the same ids, and an object keeps its
id when others change. With the export time left at 0 (the command
line's default) the same structure gives the same bytes in every format.

## What maps

### Common (`ExportStructure`)

| OLS CAD | Export data |
|---|---|
| `[[node]]` `point`, `support` (`fixed`, `pinned`, a list of x, y, z, rx, ry, rz), `spring` | joint, held directions, springs (N/m, N·m/rad) |
| `[[member]]` `nodes`, `section`, `kind` (truss, frame, cable), `reference`, `roll`, `k`, `pretension` | member with its **local axes** worked out exactly as the FE model does (`member_axes`: y in the plane of x and the reference — global z, or global x for a vertical member — then the roll about x); k and pretension carried along |
| section of the library (`.olssec`) | profile `Shape` and dimensions, A, Iy, Iz, It, Wpl, the centroid relative to the outline box |
| `[[section]]` table | A, Iy, Iz, It only (no shape) |
| material (library or `[[material]]`) | E, G, density, yield |
| load table rows (`ols_structure_loads::LoadTable`) | per case: a point's load at the joint carrying it, or **at its place along the member** (a `[[point]]` with `member`, `at`); a joint's load; a member's surface load as a **uniform load** (total ÷ length); right, ahead, down → x, y, −z |
| recipe `self_weight` | the case's self-weight factor (case `model`: `[analysis] self_weight`) |

In the structure application the load table is the Load cases tab's:
for a placed structure the line's values at that place and the values
typed over the table. The command-line tool (and the application for a
structure opened on its own) takes the file's recipes evaluated without
line values — rows needing a line value are left out and counted in a
note; surface loads on the members' areas from the section library
(`ols_structure_link::SectionAreas`) — and its `[[load]]` tables as case
`model`; a load table written by the application (CSV) can be given
instead.

### IFC4

| Export data | IFC4 |
|---|---|
| structure | `IfcProject` (units: m, rad, N, Pa, kg, m², m³ and the derived units of the values written), `IfcSite` at the origin |
| member (physical) | `IfcMember` (`.MEMBER.`), `ObjectType` = kind; placement at the first joint with Z along the member and X along its local y; representations `Axis` (`Curve3D` line) and `Body` (`SweptSolid`: `IfcExtrudedAreaSolid` of the profile over the length); property set `OLS_Member` (Section, Kind, EffectiveLengthFactor, Pretension, From, To) |
| profile: equal / unequal angle | `IfcLShapeProfileDef` (Depth h, Width b, Thickness t, FilletRadius r) |
| channel | `IfcUShapeProfileDef` (Depth h, FlangeWidth b, WebThickness tw, FlangeThickness tf, FilletRadius r) |
| I section | `IfcIShapeProfileDef` (OverallWidth b, OverallDepth h, tw, tf, r) |
| rectangular hollow | `IfcRectangleHollowProfileDef` (XDim b, YDim h, WallThickness t) |
| circular hollow | `IfcCircleHollowProfileDef` (Radius d/2, WallThickness t) |
| round bar | `IfcCircleProfileDef` (Radius d/2) |
| polygonal hollow (pole) | `IfcArbitraryProfileDefWithVoids` (outer and inner `IfcPolyline`, a side at the bottom) |
| `general`, `[[section]]` table | `IfcProfileDef` by name only (IFC4 allows it); no solid, the axis only |
| profile position | the parameterised profile's `Position` moves the outline box so that the **centroid** lies on the member's axis (IFC4 places a parameterised profile at the centre of its box); profile x = member local y, profile y = local z |
| section and material | `IfcMaterialProfile` / `IfcMaterialProfileSet` / `IfcMaterialProfileSetUsage` (cardinal point 10, the centroid), associated with the physical and the analysis member; `IfcProfileProperties` `Pset_ProfileMechanical` (CrossSectionArea, TorsionalConstantX) and `OLS_Section` (SecondMomentLocalY, SecondMomentLocalZ: about the member's local y and z) |
| material | `IfcMaterial` (category from E and density, below) with `IfcMaterialProperties` `Pset_MaterialMechanical` (YoungModulus, ShearModulus, PoissonRatio), `Pset_MaterialCommon` (MassDensity), `Pset_MaterialSteel` (YieldStress) |
| analysis model | `IfcStructuralAnalysisModel` (`.LOADING_3D.`, a shared placement at the origin), items assigned by `IfcRelAssignsToGroup`, serving the site (`IfcRelServicesBuildings`) |
| joint | `IfcStructuralPointConnection` with an `IfcVertexPoint` topology; a support as `IfcBoundaryNodeCondition`: held `IFCBOOLEAN(.T.)`, free `.F.`, a spring as `IfcLinearStiffnessMeasure` / `IfcRotationalStiffnessMeasure` |
| member (analysis) | `IfcStructuralCurveMember` with an `IfcEdge` between the joints' vertices; `Axis` = the member's local z; truss → `.PIN_JOINED_MEMBER.`, frame → `.RIGID_JOINED_MEMBER.`, cable → `.CABLE.`; `IfcRelConnectsStructuralMember` at both ends; `IfcRelAssignsToProduct` to its `IfcMember` |
| load case | `IfcStructuralLoadCase` (`.LOAD_CASE.`, action type and source `.NOTDEFINED.`), `SelfWeightCoefficients` (0, 0, −factor) |
| load at a joint | `IfcStructuralPointAction` (no placement of its own, as the standard requires) with `IfcStructuralLoadSingleForce`, `.GLOBAL_COORDS.`, connected to the point connection |
| load along a member (a point) | `IfcStructuralPointAction` with its own placement and vertex, connected to the curve member |
| uniform load on a member | `IfcStructuralCurveAction` (`.CONST.`, `.TRUE_LENGTH.`) with `IfcStructuralLoadLinearForce` |

### IFC2x3 (Tekla Structures)

The members only, as above, with the differences of the older schema:
an owner history on every object, the members in a building storey
(site, building, storey), profile `Position` always given, fillet radii
left out when 0 (they must be positive there), `IfcMaterial` by name
associated with the members, no `IfcProfileDef` by name (a section
without geometry gives an axis only). Tekla's conversion maps profile
and material **names** to its catalogues (missing ones are asked for in
its *Missing Mapping* dialog [T1]): the profile name written is the
section's id.

### SAF (2.2.0)

| Sheet | Rows | Notes |
|---|---|---|
| `Project`, `Model` | property name in column A, value in B | `SAF Version` 2.2.0, `Global coordinate system` Z vertical, `LCS of cross-section` ZYX, `System of units` Metric, `National code` EC-Standard-EN (required by SAF; OLS CAD does not tie the export to a code) |
| `StructuralMaterial` | one per material | `Type` from E and density (below), `Quality` = the material's name, E and G in MPa, unit mass kg/m³, Poisson's ratio E/2G − 1 |
| `StructuralCrossSection` | one per section | **Parametric**: angle → `Angle` (H; B; t; R; R1 = h; b; t; r; 0), channel → `Channel` (H; B; t; s; R = h; b; tf; tw; r), I → `I rolled` (H; B; t; s; R = h; b; tf; tw; r), rectangular hollow → `Tube` (H; B; s; R; r1 = h; b; t; 0; 0), circular hollow → `Pipe` (D; t), round bar → `Circle` (D), parameters in mm. **General** (polygonal hollow, `general`, `[[section]]` tables): `Profile` names a `CompositeShapeDef` row. A, Iy, Iz, It, Wply, Wplz given for every section |
| `CompositeShapeDef` | sections without a parametric shape | polygonal hollow: the outline (counter-clockwise, with the material) and the opening (clockwise); `general`: its outline box; a `[[section]]` table: **a square of the same area** (no shape is known; its A, Iy, Iz, It are in the cross-section row) |
| `StructuralPointConnection` | one per joint | m |
| `StructuralCurveMember` | one per member | `Nodes` "first; second", `Segments` Line, `LCS` **z by vector** with the member's local z, rotation 0 (the roll is in the vector), `System line` Centre, analysis eccentricities 0; `Behaviour in analysis`: truss → Axial force only, frame → Standard, cable → Tension only |
| `StructuralPointSupport` | one per supported joint | In node, Global; each direction Rigid, Free or Flexible (stiffness MN/m, MN·m/rad); `Type` Fixed, Hinged or Custom |
| `StructuralLoadGroup` | one, `LG1` | Permanent, Standard |
| `StructuralLoadCase` | one per case | Permanent, load group LG1, load type Standard; the description says the self-weight factor |
| `StructuralPointAction` | one row per non-zero component (X, Y, Z) of a load, kN | at a joint: In node; along a member: On beam, Relative position from the start |
| `StructuralCurveAction` | one row per non-zero component of a uniform load, kN/m | On beam, Uniform, Global, Length, Relative, Full, 0 → 1 |

The workbook is written by the crate itself (a zip of SpreadsheetML
parts with the workspace's `zip` crate, as `ols-io`'s XLSX export; no
new dependency), inline strings, a fixed zip time.

### CSV

One table per file, a header row with the unit in the column name (`_m`,
`_pa`, `_n`…), SI units, structure axes. `members.csv` carries the local
y and z axes (`y_x … z_z`), `loads.csv` the place (`node`,
`member_point` with its position 0–1, `member_uniform` per metre).

### Material category

The section library does not say what a material is; IFC's category and
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
  and IFC's `.NOTDEFINED.`, because OLS CAD does not classify them.
- **Self-weight** in SAF: only as text in the case's description (SAF
  has no factor per case); in IFC4 as `SelfWeightCoefficients`.
- **Effective length factor k** and **pretension**: IFC4 property set
  `OLS_Member` and CSV only; SAF has no column for them.
- **Member wind areas**, wind bands, the structure's points, strings,
  wire points, insulators, capacities: not part of an analysis model.
- **Shapes**: SAF has no parametric polygonal tube; it goes as a general
  polygon. The root radius of angles, channels and I sections is kept;
  toe radii and flange slopes do not exist in the library. `[[section]]`
  tables have no shape: IFC writes the profile by name only (no solid),
  SAF an equal-area square.
- **Unequal angles** keep h along the section's z and b along y. IFC's
  convention puts the longer leg along the profile's y; an angle whose b
  is longer than h is written as it is (the attributes allow it).
- **IFC2x3** carries no analysis model and no material values (Tekla
  maps names).
- Coordinates are the structure's own axes; a placed structure's
  position and orientation in the line are not written.

## Open points

- **No external validator ran**: IfcOpenShell and the SAF SDK are not
  installed here. The tests check the STEP structure (header, numbering
  without gaps, every reference resolving, balanced quotes), the number
  of attributes of **every** entity against the IFC4 ADD2 TC1 and IFC2x3
  TC1 EXPRESS schemas, unique GlobalIds, the expected counts for
  `examples/synthetic-line/Structures/type-02.olss`, the profile mapping
  per shape, the SAF sheets and columns read back, and that two exports
  are identical. An import into RFEM 6 and Tekla Structures is still to
  be tried by someone with licences.
- **SAF version**: written as 2.2.0 (the sheets used are unchanged since
  1.0.5 except two optional columns of `StructuralCurveAction` added in
  2.0.0 [S5]); Dlubal was listed with 1.0.5 in 2022 [S1].
- **SAF parametric shapes**: the parameter letters are from the annex
  [S3]; their drawings were not read (images). `t` = flange and `s` =
  web thickness for `I rolled` and `Channel`, `s` = wall for `Tube`, are
  read from the letters (SCIA's naming); an import should confirm them.
- **Project / Model sheets**: SAF says their properties are "oriented in
  columns not in rows as usual" [S4]; they are written as name–value
  pairs, column A and B.
- **PLS-TOWER** export: later (decision 12).
- A structure whose model is a separate `.olsfe` (`model = "…"`) is not
  exported yet (the export builds the model from the `.olss` itself).

## Sources

- [D1] Dlubal, "Guideline for Using IFC Format in RFEM 6: Import,
  Conversion, and Export", knowledge base 001797,
  <https://www.dlubal.com/en/support-and-learning/support/knowledge-base/001797>
  (read 2026-10-17).
- [D2] Dlubal, "Implementation of SAF Format", product feature 002269,
  <https://www.dlubal.com/en/support-and-learning/support/product-features/002269>.
- [T1] Trimble, Tekla Structures 2025, "Convert IFC objects into native
  Tekla Structures objects",
  <https://support.tekla.com/doc/tekla-structures/2025/int_conversion_management>.
- [T2] Trimble, Tekla Structural Designer 2026, "Export to and import
  from other applications",
  <https://support.tekla.com/doc/tekla-structural-designer/2026/bim_exportimportotherapplications>.
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
