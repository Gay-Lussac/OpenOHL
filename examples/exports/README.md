# Structure export samples

Pre-alpha, in active development (not approved yet). Small files for
someone with an analysis program, a steel-detailing program or a BIM
viewer to open and report back on. They are made from the invented frame
`every-profile.ohls` (ten members on five joints, one of each profile of
`libraries/sections/example-sections.ohlsec`; not a design) with
`ohl-structure-export` and the example section library, export time 0.
A test (`apps/structure/ohl-structure-tests/tests/exports_checked.rs`)
keeps them equal to what the exporter writes; rewrite them with
`OHL_UPDATE_EXPORT_SAMPLES=1 cargo test -p ohl-structure-tests --test exports_checked`.

They pass IfcOpenShell's validation (schema and rules) and the checks of
`tools/check_structure_exports.py`; what those checks cannot confirm is
listed in `docs/development/structure-exports.md` (Checks with free tools).

The frame, in the structure's axes (x right, y ahead, z up), metres:
joints N0 (0, 0, 0) fixed, N1 (0, 0, 5), N2 (4, 0, 5) held in x, y, z with
a rotational spring of 2000 N·m/rad about z, N3 (4, 3, 5), P1 (0, 0,
1.25: an attach joint a quarter up `m-EA`, where attachment point P1
hangs). Members: `m-EA` and `m-EA-2` equal angle EA-61x7 (N0–P1 and
P1–N1: one member split at P1, continuous, each buckling over 5 m), `m-UA` unequal angle UA-83x52x6
(N1–N2, rolled 30°), `m-CH` channel CH-157 (N2–N3), `m-IS` I section
IS-213 (N0–N2, reference along y), `m-RHS` rectangular hollow
RHS-121x79x5 (N1–N3, a tension-only member that bends: exported as a
frame with its tension-only behaviour as information), `m-CHS` circular hollow CHS-167x7 (N0–N3), `m-RB`
round bar RB-23 (N3–N1, a cable with 1 kN pretension), `m-POLE`
12-sided polygonal tube POLE-12-430 (N2–N1), `m-GEN` a general section
GEN-1 (N0–N3, no shape; `check = false`, a member not design-checked). Load cases: `model` with (1, 2, −3) kN at N3;
`along` with self-weight × 1.35 and 4 kN in x at P1.

## `every-profile-ifc4.ifc` — IFC4, Structural Analysis View

For an analysis program reading IFC (and any BIM viewer). Check:

- the analytical model is created: 5 nodes, 10 members (9 with a profile;
  `m-GEN` has none), the supports at N0 (fixed) and N2 (x, y, z held,
  spring about z), the two load cases with their loads (the 4 kN at
  joint P1, the self-weight factor of `along`);
- `m-RHS` comes in as a frame (rigid-jointed), its tension-only
  behaviour shown as information (`OHL_Member`: Kind `tension`, Bending
  true; the analysis member's description), and `m-GEN`'s `Check` false
  and description "not checked" (structure-exports.md, Tension-only
  members that bend);
- profiles and materials are recognised or offered for mapping (names
  are the section ids; materials `steel-example`, `alloy-example`), and
  each member's profile orientation matches the 30° roll of `m-UA` and
  the I section's web along y;
- units: metres, newtons, pascals.

## `every-profile-ifc2x3.ifc` — IFC2x3, Coordination View 2.0

For a steel-detailing program's IFC object conversion. Check:

- the reference model shows 10 members, 9 as solids (`m-GEN` as an axis
  only), in one site, building and storey;
- the conversion into native parts accepts the parametric profiles
  (L, U, I, rectangle and circle hollow, circle) and the polygonal tube
  (arbitrary profile with a void), and what it asks for the profile names
  (section ids) and material names it does not know;
- the parts' positions and rotations match the IFC4 file in a viewer.

## `every-profile-saf.xlsx` — SAF 2.2.0

For an analysis program reading SAF. Check:

- the import accepts version 2.2.0 (or says which it needs);
- materials, cross-sections (parametric: Angle, Channel, I rolled, Tube,
  Pipe, Circle; general: the pole and GEN-1 as polygons), nodes, members,
  supports, the load group, the two load cases and the point actions are
  all created;
- the parametric shapes' dimensions come in right (flange and web
  thickness of the channel and the I section, the tube's wall), and the
  members' local axes (LCS "z by vector") match the IFC4 file;
- `m-RHS` is a beam (Behaviour in analysis Standard) and the layers
  "OHL: tension only, bending" (`m-RHS`) and "OHL: not checked"
  (`m-GEN`) are kept;
- the units: m, mm for the shapes' parameters, MPa, kN.
