# Joint details of towers (roadmap, proposal)

*Proposal, 2026-11-02 — stages 0 to 2 built the same day (in active
development, not approved yet: owner, "Go ahead with 0-2, yes and
yes"), stage 3 too ("Go ahead with the next stage"), stages 4.1 and
4.2 and stage 5's exports too ("Go with … B" and "… C"), stage 4.4's
eccentricities from the connections' shifts too; the rest of stage 4
and stage 5's validation not built; the bolt library built
(below). Asked by the owner:
"a roadmap for a tool to show and allow to define the joints in the OHL
structure in details for a pylon that isn't a pole … where the bars are
coming from, what's their condition on the joint and where are the
bolts, how many. And then how this is expected to impact the conditions
on the calculations later on." Open point O19 in the
[roadmap](../roadmap.md); comment there or under each question below.*

## What there is today

| Part | Today | Where |
|---|---|---|
| Tower or pole | `kind` in the structure file: `lattice`, `pole`, `portal`, `other`. Chosen once, when an empty structure is made; OHL-Structure opens the pole panel for a pole | `ohl_structures::StructureKind`, `apps/structure/ohl-structure-app/src/start.rs` |
| Joints | a position, supports and springs, a kind (*attach*, *support* or none). Nothing about what meets there or how | `ohl_structures::Joint`, `ohl_structures_fe::Node` |
| Members | truss, frame, cable or tension-only, between two joints; a section with a roll | `ohl_structures_fe::Member` |
| A member's connection | **one set for the whole member, the same at both ends**: bolts (count, diameter, hole, shear planes, strength, holes in the section), edge and end distance, spacing, the connected leg, end eccentricity and end restraint (none, one end, both ends), its class and its crossing partner | `ohl_structures_fe::design::MemberDesign` |
| Checks using it | ASCE 10-97 (the default): the slenderness curve from the end eccentricity and restraint, bolt shear, bearing, the net section; missing data take conservative assumptions, said per member | the bundled plugin `asce10` (`plugins/codes/asce10`; the core's `ohl_structures_fe::asce10` until 2026-11-18) |
| Bolts and plates elsewhere | only as a factor on the self-weight (`dead_load_factor`) | |

So the calculation already depends on the joints, but the joints are
typed in as flags per member ("both ends eccentric", "3 bolts"),
without anything to show where they come from.

## What the tool is for

At a tower's joint, see and define:

1. **Which members arrive and from where.** Their direction, the face
   they lie in, the angle to the leg, their class (leg, bracing,
   redundant, crossing).
2. **Each member end's condition.** Bolted to the leg directly or
   through a gusset plate, the leg it bolts by, pinned or held against
   rotation, eccentric or concentric.
3. **The bolts.** How many, which diameter and grade, in how many gauge
   lines, at which pitch, end and edge distance — and so where each bolt
   is.

From that, the conditions the checks take are **worked out instead of
typed**, and the tool says which were worked out, which given, and which
assumed.

## Stages

Each stage is usable on its own and ends with the full checks. Sizes:
S a few days, M one to two weeks, L more.

### Stage 0 — Pole or tower, a toggle (S) — built 2026-11-02

- The structure's kind becomes editable after the structure is made:
  **Geometry › Structure kind** in OHL-Structure (lattice tower, pole,
  portal, other), and shown in OHL-Design's structure picker as today.
- **Tower** (lattice, portal): the joint details below, ASCE 10-97 by
  default. **Pole**: the pole panel; no joint details (a pole's flanges
  and slip joints are a later, separate item). The Joints and Members
  tables stay for every kind: a pole may have members (its crossarms).
- Changing the kind never deletes data: what the new kind does not use
  is kept, hidden, and the switch says so.

> **Owner:**

### Stage 1 — The data: connections at the member ends (M) — built 2026-11-02

A **connection** is one member end at one joint. Each member has two;
a joint lists those that arrive there.

| Field | Meaning |
|---|---|
| `member`, `end` | the member, its end (`i` or `j`) |
| `onto` | what it bolts onto: a member (the leg), a gusset plate, a splice; or `welded`, `pinned`, `fixed` |
| `leg` | the leg of the angle it bolts by: long, short, both |
| `bolts` | count, diameter, grade (ultimate strength), shear planes, threads in the shear plane |
| `layout` | gauge lines (1 or 2), the gauge of each, pitch, end distance, edge distance; or explicit bolt points in the member's axes |
| `plate` | a gusset plate: id, thickness, material, outline; the members bolted on it |
| `eccentricity` | worked out from the gauge line and the section's centroid; may be given |

- **Where a member comes from** is not stored: it is the geometry (the
  member's direction, the face plane of the joints it lies in, its
  angle to the member it bolts onto), shown and reported.
- **Reading older files:** today's per-member data (`[member.design]`
  bolts, distances, connected leg, end eccentricity and restraint)
  become the default of both ends; a connection given overrides its
  end. A file without connections reads and computes exactly as today.
  Structure file version 9.
- Exports: the connections in the CSV export, the bolts and plates in
  IFC (Stage 5).

> **Owner:**

### Stage 2 — Showing a joint (M) — built 2026-11-02

How it works now: [OHL-Structure › Joint details](../user-guide/structure-app.md#joint-details-towers);
the file format: [connections and plates](../user-guide/structure-library.md).

In OHL-Structure, selecting a joint (in the 3D view or the Joints table)
opens the **Joint panel**:

- **A close-up** of the joint: the members as their real profiles
  (angles with their roll, so the connected leg shows), cut a little
  beyond the joint; an arrow per arriving member with its id, the face it
  lies in and its angle to the leg; the bolts as small cylinders on the
  leg or plate, the plates as thin outlines. Turnable as the main view.
- **The face view:** the joint seen square to a face, as a shop
  drawing — the members' gauge lines, the bolts with their pitch and end
  and edge distances dimensioned. Exportable as SVG or PDF.
- **The connections table:** member, end, class, from (face, angle),
  onto, leg, bolts (n × diameter), layout, plate, eccentric or not,
  restraint — each value marked **given**, **worked out** or
  **assumed**.
- In the 3D view, a colour per joint: complete, partly assumed, no
  connection data (a View tick, off by default).

> **Owner:**

### Stage 3 — Defining joints (M) — built 2026-11-02

Built as below, but: what the members bolt onto and the bolt points are
not carried by a copy to a group or class (they are the joint's own); a
preset is kept next to the structure (`Connections/*.ohlcp`); reading a
tower program's connection data is not built. How it works: [OHL-Structure
› Joint details](../user-guide/structure-app.md#joint-details-towers).

- Edit any connection in the table; the close-up and the face view follow.
- **Bolt layout helper:** n bolts in one to three gauge lines; pitch and
  distances from the minimums the code gives. The values come from the
  standard's text supplied by the owner, not typed from memory; until
  then the user types them.
- **Copy a joint's details** to other joints with the same members'
  sections and angles (within a tolerance), by symmetry
  (`[[symmetry]]`), or to every joint of a member group or class.
- **Connection presets** (a library, like the cross-sections): "bracing
  to leg, 2 × M16, one gauge line", applied to many ends at once.
- Later: read the connection data of tower programs' files (the local
  importer of another program's files), so a tower brought in keeps its
  bolts.

> **Owner:**

### Stage 4 — What the calculations take from it (M–L) — 4.1 and 4.2 built 2026-11-02

Built as 4.1 and 4.2 below, but: worked out only at an end with a
`[[connection]]` (an end without one keeps the member's flags and the
assumptions, so a tower without connection data computes as before);
bearing without the end and edge distances (the standard's rules for
them to come from its text); the gauge lines' holes in one section
(staggering not counted). The rules are in the core
(`ohl_structures_fe::member_ends`) and the Joint details table shows
them; how it works: [theory](../theory/asce10.md).

Each item is an option or a default to agree; every result says which
values were worked out from the joints and which assumed.

1. **ASCE 10-97 slenderness curves** (in the core today). End
   eccentricity and end restraint are worked out per end:
   - one leg bolted → eccentric at that end;
   - two or more bolts into a member stiff enough (its I/L at least the
     sum of the members it holds there, ASCE 10-97 C3.7.4 (b)) → partly
     restrained; onto a gusset plate only → not (3.7.4.5; 2026-11-03).

   Today they are flags per member, the same at both ends, and missing
   flags take the conservative curve. Expected effect: bracing that today
   takes the conservative assumption gains capacity where the joints
   show restraint, and loses it where an end turns out eccentric.
2. **Connection capacity per end.** Bolt shear, bearing (with that end's
   end and edge distances) and the net section (the holes actually in
   one section, from the layout), taken at each end; the weaker end
   governs. Expected effect: no change where both ends are alike; a
   member with a weaker end connection than the one typed today is found.
3. **Buckling lengths from what braces a member.** Where a redundant or
   a crossing diagonal is bolted along a member, and onto which leg,
   tells about which axis (v, or x/y) the member is braced. The length
   ratios are worked out instead of typed. Expected effect: legs and main
   bracing braced by redundants get shorter buckling lengths about the
   braced axes only — the right ones, not k = 1 everywhere.
   **Built 2026-11-19** (in active development, not approved yet): the
   core finds each member's run of members in line through its joints
   and the bracing points along it — each member holding a point and the
   direction it holds it in, from the model's geometry (the members the
   analysis joins there; a `crossing` partner where it crosses), with or
   without `[[connection]]`s (`ohl_structures_fe::bracing`) — and sends
   them to the code plugins (`member-check-bracing`), which apply their
   code's rules: ASCE 10-97 (an angle's v held at any point, its
   geometric axes only out of the plane; Figure 3.2's 1.2 for legs with
   staggered bracing; Appendix B Example 7's 20 % for crossings; C3.7.4's
   limits for a bracing member) and SIA 263 (5.4.1.3 read strictly;
   4.2.3.7's 1 %). Given `k`, `lk` or `length_ratios` still win; each
   result says "from the bracing", "given" or "assumed: no bracing
   found" ([ASCE 10-97](../theory/asce10.md#buckling-lengths-from-what-braces-the-member),
   [SIA 263](../theory/sia263.md#flexural-buckling)). Which leg of the
   angle a member bolts onto is not read from the connection: the
   direction of the member holding the point decides (the same thing for
   a member lying in that leg's plane).
4. **The FE model (options, measured before any default):**
   - the connections' eccentricity as offsets at the member ends (moments
     in frame legs) — **built 2026-11-02 for a connection's `shift`**
     (owner: "a sideways move is needed to fit the members on the gusset
     plate … Eccentricities need to be taken into account"; in active
     development, not approved yet): a rigid end offset to the joint,
     carried where a frame member meets it, left out and said where only
     bars do; always on, a tower without shifts unchanged
     ([theory](../theory/structures-fe.md#eccentric-member-ends-rigid-end-offsets));
   - pinned or held ends as end releases of frame members;
   - **bolt slip** (a hole larger than its bolt) as a joint's stiffness
     up to the slip, which adds to the tower's deflection.

   These matter for deflections and for the structure–line interaction
   at level 4 (O7), much less for the members' forces.
5. **Checks of the joints themselves** (O4: "members first, then
   joints"): the gusset plate (an effective width, block tearing), the
   bolt group under the members' forces together, the leg's bearing.
   ASCE 10-97's connection rules in the core; SIA 263 / EN 1993-1-8
   through the code-check interface of O4. Usage per joint, coloured in
   the 3D view.
   **Built 2026-11-19** (the bolt groups and the leg's bearing; in active
   development, not approved yet; the gusset plates since 2026-11-04):
   per joint of a tower at level 3 or more, **a plate's bolts onto the leg
   holding it** carry the resultant of the other members' forces on the
   plate with its moment about them, shared elastically (Kulak, Fisher &
   Struik), and **the bolts onto a member** — the lapped members' and the
   holding bolts' — each the vector sum of every part it holds, the same
   case only (`ohl_structure_link::joint_groups`,
   `ohl_structures_fe::bolt_groups`); each bolt checked by the code's
   plugin (`joint-check-groups`: bolt shear, bearing on the plate or the
   member, the distances along and square to its force — ASCE 10-97 4.3.2,
   4.4, 4.5; SIA 263 Table 16 (73)–(76), Table 17). **The joint's usage**
   is the largest of its plates', welds', bolt groups' and bearing checks:
   in Joint details, the 3D view's joint colours and the report's
   *Joints* table. Not defined → said, never skipped: a plate the joint's
   leg does not bolt onto ("how it is held is not given"), bolts not
   placed, a thickness not known
   ([theory](../theory/joint-groups.md)). The owner's follow-ups of
   2026-11-19 (built the same day, in active development, not approved
   yet): **a plate's own bolts onto the leg** (`[plate.leg_bolts]`,
   structure file version 20: a leg running on through the joint holds the
   plate by them); **a joint that does not fit** (a bolt's hole off its
   flange, a holding bolt outside the plate, holes overlapping, a bolt
   beyond a member's known end) reads "does not fit" — failing, counted
   apart, never an infinite usage; **pieces in line not said continuous
   end at the joint** (on the safe side; the layout proposing
   `continuous` for the user to confirm follows the layout's work, ROADMAP
   O19); **bolts in tension checked, with prying** (each bolt's pull from
   the forces square to the plies and the moments out of their plane,
   shared elastically about the bolts' centroid; the T-stub model of
   Struik and de Back with α = δ = 1; ASCE 10-97 4.3.3, 4.3.4, SIA 263
   (77), (78); `joint-check-groups-tension`); the elastic method for the
   moments stays.
6. **Weight and quantities.** The bolts and plates counted and weighed:
   - the weight replaces the `dead_load_factor` estimate (1.1–1.4 today)
     where every joint is defined;
   - the bill of materials lists bolts by diameter, length and grade, and
     the plates.

   **Built 2026-11-19** (in active development, not approved yet): the
   members, gusset plates, splice covers, bolts (library mass, else
   weighed from their sizes) and welds counted from the connection records
   only, the automatic layout's marked; a heavy and a light count (holes
   kept or deducted, a weld without a size at SIA 263 6.3.6.3's largest
   throat or left out); the bill in Design › Weight & quantities, as CSV,
   XLSX and in the report; where every joint is defined (no end without a
   `[[connection]]`, none left for a hand detail, every bolted end with
   bolts) a run's self-weight is, until the counting is validated, the
   larger of the counted mass and the members × the factor where the
   weight loads the tower and the lesser where it holds it;
   `self_weight_from` (version 19) `"factor"` keeps the factor,
   `"counted"` takes the count alone (not on the safe side). On 73 real towers after the
   layout none has every joint defined. Not counted, said: galvanising,
   fillers and packs (no record), fittings, washers of bolts without a
   library mass. [Theory](../theory/structure-weight.md).

> **Owner:**

### Stage 5 — Exports and validation (M) — exports built 2026-11-02

Built: the bolts and gusset plates in IFC4 and IFC2x3 — a joint
assembly per joint with its bolts (`IfcMechanicalFastener`) and plates
(`IfcPlate`), where the Joint view draws them; the members' solids
seated as drawn where their bolted ends agree, else on their axes with
their bolts; the analysis model on the axes. SAF has no fasteners (IFC and
CSV only). How: [Structure exports › Bolts and gusset plates](structure-exports.md#bolts-and-gusset-plates).
Not built: exports for tower or detailing programs' own formats, the
validation and the theory page below.

- IFC and Tekla exports with bolts and plates (structure exports).
- Validation: the per-end connection capacities against hand
  calculations of ASCE 10-97 cases, and against a tower program's
  connection results on the local towers (comparison kept in
  `local-data/`).
- Theory page: how each condition is worked out from a joint, with
  drawings (an angle bolted by one leg and its eccentricity; a redundant
  bracing a leg about one axis).

> **Owner:**

### Member details — built 2026-11-02

(Owner: "the same view of the members as for the joints … see live the
impacts on the bar's resistance".) A member's two ends edited from the
member (the joints' own data), its ASCE 10-97 capacities worked out live,
the usage under the last run's forces, its leg bolted laid flat with the
holes of both ends: [OHL-Structure](../user-guide/structure-app.md#joint-details-towers).

### The Detail view, plates and bolts — built 2026-11-03

(Owner, 2026-11-03.) The Detail view's tools are a toolbar over the
bottom of its drawing, as the 3D view's hotbar, with the tool held above
it; its details (Joint details, Member details) sit in a leaf of their
own below it. The 3D view's Edit tool asks before it opens a tower's joint
or member there. A member opened from a joint goes back to it (‹ Joint,
Escape). The Edit tool on a bolt shows its properties below with the bolt
libraries to change the end's bolts; on a plate, what it is welded to, its edges and the
bolts too near its edge. Several plates per joint (2026-11-03: bolted or welded onto
the leg, `[[plate]] kind`; since 2026-11-04 a welded plate is several
plates welded together, below); a plate drawn without an outline keeps the
code's end and edge distances in the plate (`bolt_rules::rules_in`:
the plate's thickness and its material's ultimate strength, else the
member's; the member end's force and bolts) round every bolt onto it. The close-up
is drawn in small pieces sorted by their own depth (whole faces sorted by
their middles hid the bolts at some angles). The plates' own checks and a weld's own data
(size, length): built 2026-11-04, below.

How it works: [OHL-Structure › Joint details](../user-guide/structure-app.md#joint-details-towers).

> **Owner:**

### Welded plates, a member's roll, a smooth close-up — built 2026-11-04

(Owner, 2026-11-04.) Built, in active development, not approved yet:

- **A welded plate is several plates welded together** so that every
  member bolting onto the joint reaches one ("the 'welded plate' should
  just mean that multiple plates are welded to be able to reach every
  member bolts that come on the joint"): `[[plate]] welded_to`, read both
  ways; the plates joined are one welded plate, the seams drawn where they
  meet. The 2026-11-03 `kind = "welded"` (welded onto the leg) is read as
  plates of one welded plate. **Make a welded plate for every member**: a
  plate per plane of the members arriving (with the joint's leg), each
  round its members' bolts with the code's end and edge distances in the
  plate, welded together.
- **A member's roll** ("a correction angle for the members to rotate
  them on their axis as one face need to come in contact with the
  plates"): Roll, ±90°, 180°, Lay flat in Member details, the Detail view
  and the Inspector; a member whose leg bolted lies more than 2° off what
  it bolts onto is named in Joint details, **Turn it flat**. The roll
  turns the member's axes for the analysis too.
- **Gauge lines from the holes** in one section of the member's design
  data where no connection gives them (a tower program's export).
- **The close-up** cut once per joint and file change, finely near the
  bolts only, sorted again only when turned: smooth on towers again.
- The Edit tool no longer asks "Open in the Detail view?" when the
  Detail view is shown.

Open: a roll that suits both ends of a member bolted onto differently
turned faces (Lay flat takes the end lying off; the other end is then
said).

> **Owner:**

### Weld size and length, the plate checks — built 2026-11-04

(Owner, 2026-11-04: "add weld size and length and the plate checks".)
Built, in active development, not approved yet:

- **A weld's data**: `[[plate.weld]] to = "G1", size = "6 mm", length =
  "220 mm", kind = "fillet"` (still structure file version 9). The size
  is the fillet's **throat a** (SIA 263 6.3.2.6); no length: the seam's
  from the geometry (one side); `welded_to` alone still read (a weld
  without a size: not checked, said). The plates table and a plate's box
  have a row per welded pair (kind, size, length; each change one undo
  step); the seam's label (`a6 × 220`) in the close-up and the face view;
  `plates.csv` weld columns, IFC `OHL_Plate.Welds`.
- **The plate checks**, per case with the analysis (Results › Run), by
  the structure's capacity method ([theory](../theory/joint-plates.md)):
  per member bolting onto a plate its bearing on the plate, the plate's
  block shear round the bolt group, its section across the group and its
  end and edge distances; per fillet weld (SIA 263 6.3.2, (87) and (88))
  the resultant of the member forces on the plates it holds (on the safe
  side). ASCE 10-97 gives no weld rule (5.2.5: AWS D1.1): welds not
  checked under it, said; the simple check checks neither. A plate in
  compression: a strut of Whitmore's width (2026-11-18, an assumption on
  the safe side; one bolt or no free length: not checked, said). Joint
  details lists each
  part's usage (the case shown and the largest), what governs with its
  clause, each check in a box; the 3D view marks the joints by their
  plates' worst usage (orange, red); the structure report has a table.

Done: the tearing path towards a plate's side edge (2026-11-08); the
moment of a weld's force about it, a gusset plate's buckling as a strut
and a frame member's end shear and moments in the plate checks
(2026-11-18, the lattice towers' safe-side items). Open: bolts in tension
(a force square to the plate, a moment bending it out of its plane: said
not on the safe side).

> **Owner:**

### A plate corrected by hand — built 2026-11-04

(Owner, 2026-11-04: "the user should be able to access the gusset plate
details if needed to correct some of the joint in case the automatic
sizing fails".) Built, in active development, not approved yet: a plate
picked in the Detail view shows **Plate details / outline** — its corners
as u, v (mm) in its own plane (`ohl_structure_link::PlateFrame`: n the
outline's normal, its largest component positive so that it stays the
same however the corners are edited; u along the joint's leg it lies on,
else its first member; v = n × u; the origin the work point laid onto the
plane), each typed, added (+ corner, midway) or taken away (three kept);
**Start from the automatic outline** / **Back to automatic** (the
`outline` written / taken out, `connection_edits::clear_plate_outline`);
the plate moved square to its plane (a plane offset); its edge distances,
the bolts too near its edge and its checks of the last run beside them.
In the face view its corners are dragged in the plate's plane (the view's
ray met with the plane), only where the plate is seen within about 45° of
square. Each change one undo step; the file format is unchanged (the 3D
`outline` of `[[plate]]`).

Open: snapping a corner to a bolt's minimum distance; corners dragged in
the close-up.

> **Owner:**

### What the values mean; a plate in a grip drawn — built 2026-11-04

(Owner, 2026-11-04: "with gauge, pitch, End, EDGE, e, stops, it's hard
to understand exactly what does what on a joint can you make it clearer ?
like make an explanatory illustration"; "also the plate thickness never
shows … the bolts get longer but without a plate".) Built, in active
development, not approved yet:

- **What the values mean**: a folding section above the connections
  table (`ohl-structure-app` `joint_sketch`): the leg bolted of an angle
  seen flat — heel, leg's edge (toe), the member's end where the steel
  stops, the axis (centroid line, dashed) running past it to beside the
  work point, the gauge line with three bolts, a second gauge line faint,
  what it bolts onto through the work point — and a dimension per value:
  Gauge (heel → gauge line), Pitch (bolt → bolt), End dist. (member's end
  → first bolt), Edge dist. (gauge line → leg's edge), Ecc. e (axis →
  gauge line), Steel stops (work point → member's end along the axis),
  Shift (the axis beside the work point, + towards what it bolts onto),
  Lines (the second line). One list of terms (`joint_sketch::TERMS`: the
  column's header, the drawing's name, the hover text) feeds the table's
  headers and the drawing, so they say the same. Theme colours from egui's
  visuals; scaled to the panel (labels keep their size, at least 480
  points: narrower scrolls); a test lays it out with egui's fonts at
  several widths and finds no label over another or over a bolt. The
  values of the member end whose bolt is picked are written beside the
  names. The user guide's image (`docs/images/joint-details/joint-values.svg`)
  is written from the same drawing (`joint_sketch::svg`, a test keeps it
  in step; `OHL_WRITE_DOCS_SVG=1` writes it again). The headers became
  **End dist. mm**, **Edge dist. mm**, **Ecc. e mm**, **Steel stops mm**
  (were End mm, Edge mm, e mm, Stops mm); the file format is unchanged.
- **A plate in a grip, drawn**: a member end onto a plate always had the
  plate's thickness in its bolts' grip (`joints::grip`) and was moved out
  by it from the joint's leg (`seat`), but the close-up drew the plate as
  a translucent film in the face of the bolts — the face the member's leg
  lies on — so it lay under that leg and behind the corner's flange, seen
  from almost nowhere: the bolts got longer, no plate showed. Now
  `PlateDetail::through` (the outline's normal turned against the bolts'
  axis of the members onto it, as the IFC export extrudes it) and the
  close-up draws the plate as a shaded solid from its outline through its
  thickness, filling the room the member was moved out by; its id and
  thickness are written on it, and the pointer over it (close-up and face
  view) says its thickness, material and the members whose bolts clamp
  it. A plate with nothing bolted onto it and no outline given used to
  get a 3 cm disc round the work point (inside the leg's steel); it now
  has no outline (`NO_OUTLINE`, said in its plates-table row, its box and
  its `notes`), and no grip takes it — a grip takes only the plate its
  own member end bolts onto. Tests: `a_plate_in_a_grip_has_an_outline_and_its_thickness`
  (link), `the_values_are_drawn_and_a_plate_in_a_grip_is_drawn` (app).

> **Owner:**

### The profiles in the 3D view as placed — built 2026-11-05

(Owner, 2026-11-05: "In the 3d view of the OHL structure, I need the
profiles to be shown with there actual right orientation and placement
as defined in the joints and members detail view".) Built, in active
development, not approved yet:

- **One builder** in the link crate (`member_solids.rs`):
  `section_at` (a member end's outline turned by its axes — the
  reference and the roll, the finite-element model's — and moved by the
  end's offset: an angle seated with its leg bolted on what it bolts
  onto, the end's shift), `steel_start` (the cut), `prism_faces` (the
  sides and the ends) and `member_solids` (each member from the cut at
  one end to the cut at the other). The close-up draws each member end
  with `section_at` and `prism_faces` from its cut to its reach; the 3D
  view the whole member with `member_solids`: at each end the same
  corners (tested within 1e-9 m). Where the two ends are seated
  differently the section moves linearly from one end's place to the
  other's (each end exactly as the close-up draws it). Built-ups by
  their built-up outline, channels, I sections, tubes (16 sides),
  general boxes; a section of values only as a square of its area (as
  the close-up).
- **The 3D view** draws each member with a solid as its steel, shaded,
  its edges and its work line (thin) drawn when a profile is at least
  4 pixels wide; further away the work lines as before over the steel
  (display only). Picking stays on the work line; picked members, the
  usage colours of the results and the extension shown colour the
  steel. A member without a profile, or whose joints have moved since
  the solids were made, is a stick; its hover says why. View › Display
  options › **Profiles as placed** (on by default) turns them off.
- **Once per revision**: a structure of up to 400 members has its
  solids at once (the joint details shared with the Joint details
  panel); a larger one on a thread of its own, the last solids drawn
  meanwhile where they still fit. A real lattice tower of 3030 members:
  joint details 0.31 s and solids 4 ms (release build), 71 248
  triangles.
- **The close-up was a mirror image** until now: its rotation had a
  determinant of −1 (seen from behind, +x still to the right). It is a
  rotation now, seen from the front as documented; turned and tilted it
  matches the 3D view's camera at yaw − 90°, pitch + 0.35 rad (tested).

Open: OHL-Design's line view still draws the structures' members about
their centroids (`ohl_structures_fe::drawn`), without the seat or the
cut. (Plates and covers are in the 3D view since stage 4 of the lap
joints, bolts since the section below.)

> **Owner:**

### Seated without bolts, bolts on both legs, bolts in 3D — built 2026-11-05

(Owner, 2026-11-05: "With the members please draw them at the right
place in 3d also give the possibility to define on which surface of the
member are the bolts we might need them on both side of a corner in
some cases for joints".) Built, in active development, not approved
yet:

- **Seated without bolts.** A member end that bolts onto a member (given
  or worked out) is seated on that member's leg whether or not it has
  bolts (`joints::place_bolts`); before, an end without bolts was seated
  only when its face was given, and was drawn on its work line through
  the other member's steel. Not an end in line with the member it meets
  (within 20°: a leg running on, one part or spliced, not lapped)
  without bolts or a face given: it stays on its work line. Display only,
  as before (the analysis takes the shift, not the seat).
- **Onto a built-up member (2026-11-11).** The seat and the cut need an
  angle at both ends; a built-up section of angles gives its angles
  (`Shape::built_up_angles`, `Profile::part_angles`) and the end is
  seated on and cut against the one facing it (`joints::facing`): the
  one whose leg, the one the end lies on, runs from its heel out on the
  end's side; not one where the end would lie in the gap against the
  next angle's back; not one its outstanding leg would pass through;
  then the nearest to its work line. On an inner face the cut also
  clears that angle's other leg (its thickness; a single angle's inner
  face as before). The cut is at most half the member, as before. The
  bolts' grip takes that angle's thickness. Display and detailing only:
  the member's design data, its end conditions, the finite-element
  model and every capacity are read from the file and do not use the
  seat. A plate on a built-up leg is not moved (where a plate lies
  enters its checks): it stays through the work point. Not yet: a
  built-up member as the arriving end (it sits astride what it bolts
  onto: another mechanism), two channels, an I or H, nesting in a
  built-up member.
- **Bolts on both legs.** `leg = "both"` lays the layout on each leg of
  the angle: the bolts shared (`count` the total; the leg lying on what
  it bolts onto takes the odd one), the same gauge lines, pitch and end
  distance, the gauges given measured from each leg's heel (assumed:
  each leg divided evenly). `other_leg_points` gives the other leg's
  bolts by hand, as `bolt_points` (then those of the leg on what it bolts
  onto). `Arrival::{other_points, other_flat, other_axis}` and
  `bolts_with_axes()`: the close-up, the joint sheet, the IFC export, the
  splice covers' outlines (each cover round the bolts of its own leg)
  and the 3D view take each bolt with its own axis. A bolt added, moved
  or taken away in the face view (the leg on what it bolts onto) writes
  the other leg's bolts as `other_leg_points`, so they stay.
- **The check:** the holes in one cross-section count both legs' lines
  (`member_ends::holes_of`; on the safe side: all in one section). The
  bolt shear takes all the bolts, the end is concentric (both legs), as
  before for `both`. Bearing onto a splice: the leg with fewer planes,
  as before.
- **Bolts in the 3D view** (`member_solids::bolt_solids`): every bolt as
  the close-up draws it, grey (orange: a layout partly assumed), in a
  scene of its own built once per solids (a pick does not build it
  again: a real tower of 2244 members has 10 772 bolts, 0.9 million
  triangles; a pick 7.1 ms per frame as without them), drawn near once
  a typical bolt is 3 pixels wide.

Open: the member view (the member laid flat) shows the leg on what it
bolts onto only; the other leg's bolts are edited in the file
(`other_leg_points`) or by the layout. A second leg bolting onto
something else than the first (another member, a plate) is not modelled
yet: both legs bolt onto `onto`.

> **Owner:**

### Lap joints, double covers, continuous members (proposal, 2026-11-05)

(Owner, 2026-11-05: "have a profile slide into the other (if smaller and
fits) and be directly bolted to the other … and also have the
possibility to have 2 plates on each side of the profile to bolt two
profiles together which would make the bolt have 2 shear planes", then
"also add the continuous … it's just one part", then "go ahead with the
development of stage 1 and further".) Three cases, at level 3:

- **A — one profile inside the other (lap, single shear).** A
  connection's `face`: `outside` (today, the default), `inside` (on the
  inside face of the leg it bolts onto) or `nested` (an angle heel into
  heel inside the other, by one leg or both). The fit is checked: the
  inner leg at most the outer leg less its thickness and root radius,
  the end and edge distances on both parts. The inner heel is taken as
  **ground to clear the outer root fillet** (the usual practice; not
  ground it would sit r (1 − 1/√2) off along the diagonal, said). One
  shear plane; bearing on the thinner of the two legs; net section and
  block shear on both; one leg bolted eccentric, both legs nested
  concentric. Angle in angle only for now (channels later, the same
  rule; a pole's slip joints stay their own item).
- **B — covers on both sides (double shear).** A `[[splice]]` at a
  joint: its id, the members it joins, its covers (per leg of an angle
  `outer` and `inner`, each a thickness, a material, an outline worked
  out from the bolts or given). Each member end bolts onto it (`onto =
  "S1"`) with its own bolts and layout. Two covers on a leg: two shear
  planes; one: one. Bearing on the lower of the member's leg and the
  covers' thicknesses together; each cover checked as a plate is
  (net section, block shear, end and edge distances) for its share of
  the force **by area** (an assumption, said in the theory); bolt slip
  not modelled. Flat covers only for now (an inner splice angle later).
- **C — continuous through the joint (one part).** `kind =
  "continuous"` with `onto` the member it runs on into; one end given
  sets the other's. Allowed when the two are in line within **1°** and
  have the same section, roll and reference; otherwise kept, said, and
  computed as a splice (today). The ends join rigidly in the analysis
  (as `fixed`); no bolt, bearing, net section or block shear at that
  end; the end concentric (its restraint not taken from the continuity:
  the panel beyond can buckle with it, ASCE 10-97 C3.7.4 — on the safe
  side); the buckling length is unchanged (from bracing point to bracing
  point). Drawn as one solid
  through the joint; the part lists count the chain as one piece of its
  whole length; the members keep their ids. A command proposes the
  pairs in line of the same section, for the user to confirm (never
  set by itself, nor by an importer).

**Built 2026-11-05** (in active development, not approved yet): stage
1 (the data, `f7cfd96d`) and stage 3's member checks — the shear planes
worked out, the bearing on the leg bolted onto (ASCE 10-97 4.4, SIA 263
(76); at ends with a connection), a continuous end without connection
checks ([theory](../theory/joint-laps-splices.md)); the covers' own
checks as plates for their share of the force by area; the exports
(`splices.csv`, `parts.csv`, a `face` column; IFC covers as `IfcPlate`
with `OHL_Splice`, `OHL_Member.Part`/`PartMembers`).

**Shear planes** are worked out from the plies the bolts pass through
(given still wins, said when it differs). Stages: 1 the data (`face`,
`[[splice]]`, `continuous`, the worked-out shear planes); 2 the
geometry (the nested seat and the fit, the covers' outlines, the bolt
grip, the continuous solid and part chains); 3 the checks (ASCE 10-97
and SIA 263, each by hand in a test); 4 the app (the Joint panel's
choices, the continuous command, the three views, the sketch); 5 the
exports (CSV, IFC `OHL_Splice`, one part per continuous chain) and the
docs.

**Stages 2 and 4 built 2026-11-05** (in active development, not approved
yet). Stage 2 (`ohl_structure_link`): `face = "inside"` lays the end on
the inner face of the other's leg (its outstanding leg through that leg:
a misfit, red); `nested` places the angle heel into heel (its heel
ground: on the other's inner faces), in line and turned alike within 5°
(an assumption), the fit said per leg (≤ leg − t − r) and the code's end
and edge distances in both parts, the larger running on past the work
point over the bolts (cut below 0); covers on the leg's faces round the
bolts (an inner cover from t + r), the grip the leg and the covers;
continuous ends without cut, shift or bolts, one solid through the joint
(`MemberSolid::runs_on`), `part_chains`; plates and covers as solids
(`plate_solids`). Stage 4 (OHL-Structure): Face and Planes columns,
continuous in the Held list (offered only when `continuous_problem` is
none), + Splice and the splice box, Design › Continuous members… (ticked,
one undo step), covers and misfits in the close-up, the face view and the
3D view, the sketch "How member ends meet" (`joint-plies.svg`). Open: the
checks (stage 3), the exports (stage 5), channels nested, a bent splice
angle.

> **Owner:**

### Bolt library — built 2026-11-02

"A bolt library could be nice to have too so that it can be easily
added and imported/exported."

- A **bolt library** file (`.ohlb`, format `ohl-bolts`), kept like the
  cross-section library (`Bolts/` next to the structure, the project's
  libraries): per bolt an `id`, `name`, diameter, hole, grade, ultimate
  and yield strength, shear capacity per plane (threads in or out of the
  plane), tension capacity, the head's and nut's sizes and a mass per
  grip length (for the drawing, the weight and the bill of materials).
  None of a standard's numbers built in: an example library of invented
  values ships, as for the cross-sections.
- A connection names a bolt: `bolts = { count = 2, bolt = "M16-8.8" }`;
  values typed in the connection still override the library's.
- OHL-Structure: File › Import and the Libraries panel list the bolt
  libraries, **Import** and **Export** as for cross-sections; the bolt
  layout window picks a bolt from them; a bolt the structure names but no
  library has is listed (as a cross-section).
- The checks (stage 4) then take the bolt's strengths from it; the bill
  of materials counts the bolts per kind (stage 4.6).

Built as above, but: the head's and nut's sizes and one mass and grip per
bolt (as sold); the bill of materials reads its mass and grip (stage 4.6,
2026-11-19); the checks do since stage
4.2 (2026-11-02: its shear capacities and ultimate strength).
[Format](../user-guide/structure-library.md). Questions left: masses per
grip length; one library per project or per folder (today: per folder,
as the cross-sections)?

> **Owner:**

### Automatic layout — built 2026-11-18, in active development

(Owner, 2026-11-18: "a plan for an automatic setting on the joints …
so that it's physically doable (no crossing members or beams)".) Built
in stages, each usable on its own.

**Stage 1 — the 3D clash check** (`ohl-structure-link` `clash.rs`).
Every solid at a joint — each member's steel as placed from its cut to
half its length (the other half is the other joint's), the plates and
covers, every bolt's head, shank and nut — is cut into convex pieces (a
profile's outline into convex polygons by its ears merged, run along
the member; a plate's outline the same through its thickness) and each
pair of pieces of parts that do not bolt to each other is tested by
separating axes (faces' normals and edges' cross products: exact for
convex solids). Run into each other by more than `CLASH_TOLERANCE`
(1 mm, the program's own: touching faces and the drawings' rounding)
along every axis: a `Clash` (the two parts, the depth, about where) in
`JointDetail::clashes`, a `Problem::Clash` of each member end in it
(drawn red: `misfit`), a note of a plate in it (`PlateSolid::misfit`),
`BoltSolid::clash` (red in the 3D view and the close-up). Not tested,
as bolting together: a member and what it bolts onto (and the member
running on from that in line, within 20°), a member and the member it
runs on into, a plate and its leg (and the leg running on), the plates
of a welded plate, a bolt and its member, a bolt's shank and what its
member bolts onto, the bolts of two members one bolting onto the other,
a bolt and a built-up member it bolts onto. The Problems list offers a
member's cut pulled back by the depth and 5 mm (tried on a copy). View ›
Display options › Joints with clashes colours the joints. Cost: worked
out with the joint details (a real tower of 3030 members, 1145 joints:
0.57 s for the details with the clashes, dev build).

On the private real towers (imported, never laid out) nearly every joint
has clashes: their members' ends are where the tower program's export
leaves them (the cut only clearing the leg's heel, the bolts at the
drawing's assumed gauge, pitch and end distance), so the bracing on one
flange overlaps — the case the layout's stages 2 to 4 are for.

**Stage 2 — the plane layout engine** (`joint_layout.rs`, `plan_joint`:
a pure function of a `JointDetail`, `LayoutSettings` and the code's edge
distance rule). Candidates: ends bolting by one leg of a single angle onto
a member's flange at the joint (`host_flange`: the angle, or the facing
angle of a built-up member), not in line with it (20° to 160°), not
running on, not nested; an end without `onto` takes the member running
through the joint (`through_of`: a crossing; also where a plate on no leg
lies). Each is a 2D strip in its flange's axes (along the host, across the
flange from the heel), grouped by plane (the outer or inner face). In
order (class, wider leg, larger angle, id): concentric (its gauge line
through the work point), then pulled back until clear of the heel (inner
face: the other leg and root by half the gap), the strips before it
(half-strips widened by half the gap, clipped as convex polygons) and the
bolts on that face not laid out; over the longest pull-back a sideways
move, a millimetre at a time up to the shift limit; then direct (every
bolt on the flange: from t + r + nut to width − edge) or one plate per
plane (bolts past the flange by nut and gap; not on an inner face or a
built-up leg). What does not fit: `hand` with why, left as it is; the
rest laid out. Tests (`joint_layout_tests.rs`): K-brace, X-brace (onto
the member running through), a shallow redundant (shifted or a hand
detail, never forced), two faces at a corner (two planes, a plate each),
a built-up leg (its facing angle's inner face, said), plates of the
thickest leg, the same plan twice.

**Stage 3 — plates and the run on a file** (`layout_run.rs`). `lay_out`:
the record's values taken out at the joints asked (and the members' rolls
back to what they were), members turned flat where their ends agree
within 2° (the middle of their turns), each joint planned, written into
the file as it was (`cut`, `shift` beyond concentric, `onto`, provisional
`bolts`/`gauge_lines` or a preset's fields), plates made (thickness the
thickest leg bolted on it, a hand thickness kept with its plate's id —
since the fifth round other ids the first free —; outline round the bolts written cut at the flange's heel by half
the gap; welded together as a choice), the record written; then the
joint details and the clash check again, a joint whose laid-out parts
would clash taken back. `follow`: an edit's touched joints (`touched`:
members, connections, plates, positions, settings) laid out again, an
automatic value changed by the edit made the user's; no fit: the edit
and the old layout kept, the joint in the record's `hand`, said. The
record: `[[generator]] kind = "joint_layout"` ([data model](data-model.md),
[format](../user-guide/structure-library.md#the-joint-layouts-record)),
read by the structure application only; `body::generators` leaves it out.
A concentric end is drawn with its gauge line through the work point (the
record's `gauge` mark, `joints::place_bolts`): display and clash check
only — the analysis keeps the member's axis there and the code's curve
takes that normal framing eccentricity (ASCE 10-97 3.7.4.2, C3.7.4, C2.4),
`shift` is only a move beyond it.

**Stage 4 — the app** (`layout_window.rs`): Design › Lay out joints… and
Joint details › Lay out joint… (the tool-window pattern), the preview on
a thread (`joint_details_at`: the joints asked only), Apply as one undo
step (`StructureFile::edit_alone`), Joint details' *laid out
automatically* / *provisional bolts* / *Needs a hand detail*, the follow
of edits (`StructureFile::set_follow`, the edit's message), the header's
level 3 while bolts are provisional and `analyse_structure` reading the
file without `joint_details` then (the core's level is unchanged: a core
change would be needed for OHL-Design and the core to say level 3 too —
not made, see the open points).

**Stage 5 — effect and docs**: the window's usage before and after for
the last run's cases (the analysis twice, on a thread) and the time the
layout took; [user guide](../user-guide/joint-layout.md),
[theory](../theory/joint-layout.md).

**Stage 6 — the real towers** (private, local-data only; numbers without
names): the 78 tower files of a real line, whole towers, values given by
hand replaced — 64 760 joints, 14 306 laid out (9 135 plates, 18 289 ends,
37 of them shifted, at most 20 mm), 59 319 with members left for a hand
detail, clashes at the joints 1 048 936 → 999 337, a run again identical
on all 78. A tower of 3 030 members: 4.3 s for the whole tower, a
following edit 1.6 s (release). Most of what is left: as imported, the
bracing lies on the legs' inner faces running towards the angle's other
leg, or off its flange with no common roll (plan and hip bracing), or
its axis out of the flange's plane — whether the imported angles are
turned as built is a question for the owner. The starting values of the
settings follow from these runs (theory page).

Third round (2026-11-18, owner-approved core change): the core reads the
record's `provisional` list (`StructureType::provisional_bolts`, only
that list; `JOINT_LAYOUT_RECORD`) and `StructureType::detail()` gives
level 3 while any bolt is provisional, `level_note()` says why — OHL-Design
(structure picker, properties, the type's workspace), `ohl structures fe`
(the model's joint-use note) and the line's level 4 alike; OHL-Structure's
own workaround (the run reading the file without `joint_details`) is
gone. Where no code gives minimum bolt distances the layout keeps the
structure's default minimums (`[default_bolt_minimums]`, `LayoutSettings::defaults`
from the file). Open: plates on an inner face and on
built-up legs, members on both sides of one plate, bent plates for
members no roll lays flat at both ends: by hand.

Fourth round (2026-11-18, owner: the legs' turn of an imported tower is
an import artefact — the export carries no member orientation —, "only
turn where it helps"):

- **Orient angles** (`orient.rs`, `orient_angles`; Design › Orient
  angles…, `orient_window.rs`): the legs (class `leg`) and the angles
  running on in line within 10° through a joint turned heel out with a
  flange in each of the two faces meeting at them — the faces from the
  neighbouring legs' lines (a leg's from the other legs, a chord's from
  the other chords, joined to its line by a member lying in one plane
  with both): seen along the member they make a convex polygon, the
  member a corner of it (`IN_LINE_DEG`, `PARALLEL_DEG`, `CORNER_DEG`);
  the angle's bisector on the faces' bisector; stars and crosses alike,
  back-to-back angles left; horizontal plan and hip members in no steep
  face laid flat, the other leg down (assumed). Rolls given by hand kept
  unless replaced; the record's `oriented` ([data model](data-model.md)).
  Tests: square and rectangular bodies, a taper change, a box crossarm, a
  portal of two columns, hand rolls, legs with fewer than two
  neighbours.
- **Turned only where it helps** (`layout_run.rs`): a member is turned
  only when one roll lays it flat at all its ends (those at joints not
  asked too), no end of it not asked is laid out with its roll now, and a
  joint of it is laid out with the turn (planned, then written and
  clash-checked); else its roll stays, said, and the joints are planned
  again without the turn. An automatic roll is taken back when none of
  its joints is laid out with it any more; a leg's or chord's roll from
  Orient angles is never turned.
- **Faster follow** (`reduce.rs`): the touched joints are worked out
  from the file cut down to what their details read (every joint, the
  members at them and at those members' other joints, the members they
  name, their connections, plates, splices and record marks), the
  planning done on that cut, the joint details building their model once
  (the exports' axes from it, section properties once per thread), and
  the structure file's after-edit hook giving the file as read
  (`ohl_structure_edit::Edited`, `follow_read`). A following edit on the
  private real tower of 3 030 members: 1.72 s at the start of the round,
  1.85 s with the turn checks from the whole file, **0.135 s** now
  (release optimisation, the machine shared with another build); 2 217
  members 0.09–0.10 s. The files written compared byte for byte with the
  whole-file follow on 35 edits of 7 real towers (a plate made thicker, a
  section, a roll, a joint moved, the gap setting): identical
  (`lay_out_whole`, `follow_whole` kept for that comparison).
- **On the 78 real tower files** (Orient angles, then Lay out joints,
  values given by hand replaced): joints laid out 14 306 → 15 122 (9 135 →
  8 384 plates), with a hand detail 59 319 → 59 159, clashes at the joints
  999 337 → 1 004 965 after (1 054 242 before, with the legs turned), a
  run again identical on all 78; Orient angles turned 58 646 members
  (36 335 legs and chords, 23 548 plan members) in 26 s for all 78, left
  1 083 legs (950 with fewer than two neighbours: the leg extensions at
  the foot). The whole tower of 3 030 members: 0.4 s to orient, 5.3 s to
  lay out (4.2 s before: the turns planned again where one does not
  help). Of the 59 159 joints left with a hand detail, by the kinds of
  their reasons: 12 691 only because a member's axis runs out of the
  plane of the flange it bolts onto (mostly redundants and crossings
  onto bracing that does not lie in its face: the bracing's own roll),
  11 054 only because a laid-out part would clash, 3 230 only an inner
  face (19 438 with it), 3 608 only a member no roll lays flat (17 467
  with it), 2 220 only a built-up leg (4 496 with it). Next: lay the face
  bracing in its face (Orient angles for bracing: one flange in the face
  its line lies in), then plates on an inner face; then bent plates and
  built-up legs; members on both sides of one plate by hand.

Fifth round (2026-11-18, owner: "for the automatic joint layout it
doesn't work too much on real structures … can you please tune it based
on 1 or 2 pylons"), tuned on two private real towers (a dead-end and a
suspension tower of 2 522 and 2 488 members) and checked on all 78:

- **Orient angles: face members** (`OrientKind::Face`, the record's
  `face`): every other single angle lying in a steep face between its
  legs or chords (`FaceOf::holds`: its ends within `IN_FACE` = 20 mm and
  5 mm per metre of its plane, its middle between the face's two lines)
  laid with one leg in the face, the other out of the tower, the leg in
  the face down from the heel where it can (assumed, as for plan
  members); and an angle running on in line that lies in a face of the
  legs is no chord (a face's diagonal running on through its crossing:
  on one real tower 260 of 670 "chords" were such diagonals, turned to a
  corner 18° off their face).
- **The member an end bolts onto named first** (`hosts_to_name`, in the
  run's planning copy): an end bolting onto nothing named gets the member
  running through the joint, so its leg bolted and its flat turn are
  those on that member (every "axis runs out of the face" end on the two
  towers was such an end, measured against the leg not bolted); an end
  whose worked-out `onto` (the joint's leg) has no flange in whose plane
  its axis lies gets a member running through the joint that has one
  (`host_in_plane`: a plan diagonal at a vertical post onto the
  horizontal). Choosing the in-line leg segment an end runs along was
  tried and left out: the ends of one face then fell on two hosts and
  clashed with each other.
- **Half a turn** (`layout_run`, before the turns are planned): a member
  with an end on an inner face that cannot be laid out there gets half a
  turn more, kept when more of its ends are planned laid out and its
  joints lose none; then the usual "turned only where it helps".
- **Plates on an inner face** (`plan_joint`): as on the outer one, the
  plate's heel line at the angle's thickness and root from the heel.
- **A run again identical** with the clash rounds: the plates the layout
  made at the joints written are taken out before any is written (their
  ids free: a plate's id is the first free one, kept only where something
  was given on it by hand), a connection emptied by a run goes at once,
  and the plates, connections and the record are written at the end of
  the file in that order (`Table::set_position(None)`): the same layout
  gives the same text, whatever an earlier run's text was.
- **The owner's step 1** (`joins_of`, `Join`, `JoinKind`; the run names
  them in its planning copy before anything else, writes them with the
  joint, records `kind`/`face` and `onto` as automatic): the biggest pair
  in line joined as one part (the core's `continuous_problem` none, no
  bolts at either end) or nested; one pair per joint, none where a member
  runs on through already. Those joined are not turned. On a built-up
  member the ends landing on its flange are bolted to it.
- **The clash check leaves members, not joints** (`write_rounds`,
  `plan_joint_except`): the laid-out members in a clash (a member before
  a plate's members, a redundant before bracing, then the larger id) are
  left as they were with the clash, the joint planned and checked again
  without them, up to `CLASH_ROUNDS` = 4 rounds, then left whole. Later
  rounds check only the joints planned again; a pass of the "turned only
  where it helps" loop takes the joints its refused turns do not touch as
  the last pass left them (`Checked`), and plate outlines are kept per
  joint while its plan stays.

On the two towers (Orient angles, then Lay out joints, values given by
hand replaced): joints laid out whole 71 → 225 (of 942) and 22 → 355 (of
1 006), joints written 306 → 579 and 243 → 708, member ends laid out
410 → 1 460 and 334 → 1 722; on all 78 tower files of the line (64 760
joints) laid out whole 1 414 → 17 829, written 15 122 → 43 608, a run
again identical. The settings are unchanged: none was too strict for
these towers once the members lay as built. Left by hand, with their
reasons (joints with each): a laid-out part that would clash (members
left, or the joint left whole), a member no roll lays flat at both ends
(a bent or welded plate, the owner's step 4, not built), an end running
towards the other leg on an inner face that half a turn does not help,
a built-up member's plate, members too short for their bolts.

Sixth round (2026-11-19, the owner's steps 4 and 1c and the leg
splices; "report only"):

- **Step 4, a welded plate** (`joint_layout::OwnPlane`, `own_geometry`):
  an end no roll lays flat goes onto a plate in its own plane welded to
  the flat plate of its face on the flange (the face's other ends bolted
  onto the flat plate), on the outer face or — coming from inside a
  leg's corner — the inner face; the weld written from both sides with
  its throat (0.4 × the plates' thickness, whole millimetres, at least
  3 mm: the layout's choice) and length so the existing weld check is
  made (SIA 263 6.3.3.5: a partly penetrated weld as a fillet weld). The
  record's `made` marks the plate `own_plane`: the joint details seat its
  member on its outline, lifted along the flat plate's normal
  (`PlateSeat::lift`; the plate's `leg` none, so the clash check tests it
  against the leg); a plate with no member welded to others takes their
  leg. **Limited on the safe side**: only where the member's axis then lies
  at most one plate thickness farther out of the flange than on a flat
  plate (its line of action off the work point is not in the analysis);
  measured on two private real towers the extra offset of the ends
  planned is 4–38 mm (median 15–17 mm), so most stay hand details with
  that reason — a question to the owner. A weld's size changed by hand is
  kept (`OldPlate::weld`, the follow dropping `weld` from the automatic
  fields). Members whose leg bolted faces away from the face get half a
  turn tried, as for an inner face.
- **Step 1c, splices** (`JoinKind::Spliced`): two angles in line of other
  sections that do not nest, both with bolts, nothing else bolting onto
  them at the joint (tried first with bracing on them: on the private
  towers every such cover then ran into the bracing and joints laid out
  whole before were lost — 17 848 → 17 755 on the 78 — so not there) — a
  `[[splice]]` the layout writes, covers outer and inner on each leg bolted (as thick as the
  thicker leg), both ends onto it; recorded in `made` (`splice`, `cover
  <t>`), made again by a run unless its covers were changed by hand (then
  kept). A cover counts as a part of the two members it joins in the
  clash rounds; the members onto a splice are kept clear of (their bolts)
  by the other ends.
- **Leg splices of one section** (`splice_unknown`, `SPLICE_UNKNOWN`):
  the bolts are the members' design data, at both ends of every leg
  member, so where the splice is is not known; drawn as before (lapped)
  and said, first among the joint's reasons when it is left whole.
- `EdgeRule` gives the code's edge **and end** distances (the welded
  plate's outline). The file cut down for a follow keeps the record's
  `made` rows of the plates it keeps (the seat reads `own_plane`): the
  follow compared byte for byte with the whole-file follow on five real
  towers, a welded plate's flat plate made thicker and its weld changed
  among the edits — identical (28 of 28). A flat plate with nothing
  bolted onto it keeps a thickness given by hand (it is known by its
  welded plates' members). A following edit on the real tower of 3 030
  members: 0.28–0.34 s, the same as before this round (load 12–18); at a
  welded joint 0.56–0.59 s.
- **On the 78 real tower files** (Orient angles, then Lay out joints,
  values given by hand replaced): laid out whole 17 848 → 17 880, written
  43 576 → 43 751, with a hand detail 42 890 → 42 858, clashes at the
  joints after 647 433 → 644 810, a run again identical on all 78; step 4
  on 371 joints (410 ends). Of the 26 967 ends still off the flange: 7 864
  their leg bolted facing away (half a turn did not help), 7 635 the
  welded plate would put them farther out than a plate's thickness, 7 256
  on an inner face running towards the angle's other leg, 3 779 running
  down towards the flange; step 1c none (every leg splice of other
  sections has bracing on it); 6 026 joints say where their leg splice
  is is not known.

Seventh round (2026-11-19, ROADMAP O44 J1 and J2, owner-approved in
that order; "plan ideas to get the automatic layout working"):

- **J1, a search per joint** (`joint_layout::plan_joint_with`,
  `search_plane`, `NODE_LIMIT` = 400 placings per joint, said with the
  ends left where reached): the planes on the biggest members first
  (`Strip::host_rank`); each plane's ends placed in every order that can
  matter, the owner's order first, backtracking (an end that fits nowhere
  now fits nowhere later; placings reached in another order not searched
  again); the most ends, then the least eccentricity, then the first
  found. Each end keeps clear of the joint's **fixed steel**
  (`clash::piece_corners`: the clash check's convex pieces of the parts
  not laid out, `fixed_in`, clipped to the end's layer and seen from above,
  `footprint`, `poly_need`) and of what the planes before laid out
  (`solids_of`: boxes of the legs, outstanding legs, bolt heads and plates,
  conservative). After writing, an end in a clash is first **pulled back
  past it** (`write_rounds`: its least cut from the clash's place, depth
  and the gap, `Strip::floor`; `PULLS` = 3 per end, `CLASH_ROUNDS` 4 → 6)
  before it is left as it was.
- **J2, the ends that fit kept and the place of a member left kept free**
  (`JointPlan::kept_free`): a member left for its own sake (it fits
  nowhere even alone) is fixed steel for the plan, and in the clash check a
  clash of an end laid out with it counts; one left for want of room or
  for a clash, a plate under a member left and the members joined first
  are said with it, as before. The window's hand list names the members
  left, the rest laid out, those kept free (`layout_window::hand_words`),
  and the summary the member ends laid out of all.
- **Fixed on the way**: the ends of one flange's face naming two members
  of a run through the joint (a leg in two members, one end onto each)
  were measured in two mirrored frames (the plane's x along each member
  named): their clearances were wrong. The plane's axes now run one way
  for every member of the run, so such ends share one plane and one
  plate.
- No format change: the record is the same (the reasons in `hand`).
- Tests: the owner's order filled one after the other leaves a short
  redundant, the search lays both out (D1 pulled back past it); a single
  placing says the limit; the same plan twice and a plan written kept; a
  cut given by hand kept; a member too short for its bolts keeps its
  place (plan and run); two invented versions of example joints the owner
  sent (diagonals bolted directly on a main angle's flange in the middle
  of a panel, two bolts each; a gusset plate on it there).
- **On the 78 private real tower files**: joints laid out whole 27.6 % →
  44.1 %, member ends laid out 48.9 % → 62.8 %, joints with a hand
  detail 66.2 % → 49.7 %, clashes at the joints after 61.6 % → 46.3 % of
  before, a run again identical on all 78; a whole tower of 2 190 to
  3 030 members 6–9 s → 10–15 s (one after the other). The order search
  itself changed nothing there (stops paying: kept for joints where it
  does); the gains are the fixed steel seen in the plan and the pull-back
  past a clash. What is left: [theory](../theory/joint-layout.md#measured-j1-and-j2-2026-11-19).
- **A bent plate** (the owner's example of 2026-11-19, a plate folded to
  follow two faces at a corner, members on both sides): of the joints
  still with a hand detail, 60 % have an end whose leg bolted lies off its
  flange or faces away, or lies on an inner face running towards the
  other leg — what a bent plate would take —, and for 29 % it is the only
  reason; onto a leg alone 9 % (the only reason for 2 %: most such ends
  bolt onto bracing — horizontals, plan and hip members). Approved
  2026-11-19 (before J3), built the same day as J2b (below).
- **J2b, the bent plate** (2026-11-19, in active development, not
  approved yet; [theory](../theory/joint-layout.md#a-bent-plate-round-the-other-leg-j2b)):
  an end on a flange's inner face leaving over the heel
  (`strip_of`: `Strip::over`, `Over`; before, "runs towards the angle's
  other leg") is placed by the search like any end, its cut keeping its
  steel and bolts past the other leg's outer face, the plate and its bend
  (`over_cut`, `over_steel`); after every plane is laid out, the ends of
  one plane share one bent plate (`bent_over`): the flat in their plane
  (an `OwnPlane` with `bend: Some(BendPlan { angle, inner_radius, line })`,
  no weld) and the flat on the other leg (`PlatePlan::outline` given,
  `PlatePlan::hold`, a `HoldPlan` — two provisional bolts onto the
  member, written as `[plate.leg_bolts]`), or that leg's face plate (2b)
  when its ends are all on one (`BentOver::into`: thicker, its `reach`
  over the flat); refused where the leg leaves no room for the holding
  bolts or an end lies directly on its face there. Inside radius
  `BEND_RADIUS_RATIO` × t (2, the program's starting value). The run checks
  its half turns with the bent plates off (`LayoutSettings::bent_plates`)
  and plans again with them (the owner's order: flat first). Written:
  `[plate.bend]` (structure file version 22), the record's `bend` and
  `leg_bolts` marks, the holding bolts provisional as (plate, joint).
  Joint details: `PlateDetail::bend`, the two flats one `assembly` (no
  seam or weld), members seated square to a flat standing square to the
  other (`PlateSeat::lift` falls back to the plate's normal), the bend's
  bending said NOT ON THE SAFE SIDE; bill of materials from the developed
  area (`quantities::bend_shortening`); IFC `BentWith`, `Bend`; close-up
  in green, the sheets "bent with". Tests: `bent_plate_tests` (the plan,
  the file, the details, no clash, a run again identical, the narrow leg
  refused, without bent plates as before, the estimates, the weight by
  hand), core `a_bent_plate_is_read_as_two_flats`, the app's window.
- **On the 78 private real tower files** (J2b): joints laid out whole
  44.1 % → 45.8 %, member ends laid out 62.8 % → 64.3 %, joints with a
  hand detail 49.7 % → 47.9 %, clashes after 46.3 % → 44.4 % of before, a
  run again identical on all 78, a whole tower about 5 % slower; bent
  plates at about 6 % of the joints; the ends over the heel left fell to
  about two fifths (most refused for want of room for the holding bolts
  on 45–55 mm legs). Joints that do not fit with the joint checks 32.3 %
  → 31.8 %; the estimated eccentricities' upper bounds 24.3 % → 22.0 % of
  the ends.
- **J3, one roll for the whole member** (2026-11-19, in active
  development, not approved yet; with the owner's order of the classes of
  the same day; [theory](../theory/joint-layout.md#one-roll-for-the-whole-member-j3)):
  `whole_roll::whole_roll(&[EndTurn { turn, either_leg }], FLAT_OFF) ->
  Option<(turn, RollWay)>` — the same legs within `FLAT_OFF` (2° since the owner's decision of
  2026-11-19, 5° before) first (as before),
  else an equal-leg angle's leg bolted changing between its ends (its
  turns a quarter turn apart, `layout_run::either_leg`: a single
  equal-leg angle not bolted by both legs), the smallest such turn; none:
  not turned, said. `lay_out_once` finds the turns **class by class**
  (`joint_layout::ClassRank`, `class_rank(class, direction)`: legs and
  chords never turned to suit bracing, then diagonals, secondary bracing —
  horizontal bracing, a `plan` member of Orient angles, no class (said) —,
  redundants), the joints in detail again with each class's turns before
  the next; the same rank orders a plane's ends and the planes in the
  search (`Strip::rank`, `host_rank`) and the members left first in a
  clash. Orient angles keeps a plan or face member's roll the layout chose
  (`OrientRun::by_layout`, its window's list). The relaxed planner
  (`plan_relaxed`, O45) takes the same. No format change. Tests:
  `whole_roll_tests`, `layout_run_tests::one_roll_for_the_whole_member_lets_both_its_ends_lie_flat`,
  `no_roll_for_both_ends_leaves_the_member_as_it_was`. Tried and left out (at the 5° of
  then): a turn leaving every end within the whole 5° of flat (measured: slightly
  fewer ends).
- **J4, laid out once per kind** (2026-11-19, in active development, not
  approved yet; [theory](../theory/joint-layout.md#laid-out-once-applied-to-the-joints-like-it-j4)):
  `joint_kinds::kinds(details, among, strict) -> BTreeMap<joint, Like>`
  (each joint like an earlier one with the match against the first of
  its kind: the rigid map of the eight keeping the vertical, `Map`, the
  member pairs; `like(a, b, strict)`, `like_joint(details, id)`;
  tolerances `DIR_TOL`, `LEN_TOL`), `map_plan(plan, like)` (the plan's
  members, points and directions mapped, lengths kept, words said with
  the joint's members: `Like::words`; `JointPlan::like` names the joint
  planned). `layout_run::plans_of` plans one joint per kind and maps it
  onto the others (`LayoutSettings::once_per_kind`, on by default; off:
  every joint on its own, for the comparison); each joint written and
  checked for clashes as before, the clash rounds planning a failing copy
  on its own. `apply_detail::apply_detail(text, sections, bolts, joint,
  only) -> ApplyRun { text, applied: Vec<Applied { joint, how, ends,
  plates }>, left, seconds }`: the command *Apply this joint's detail to
  similar joints* (OHL-Structure, the joint's bar in Joint details,
  `apply_window`): the source's connections and plates copied onto the
  joints of its kind (loose match: members only) — `onto`, `other_onto`,
  plates' `outline`, `[plate.bend]` `to`/`line`, `welded_to`, welds' `to`,
  `[plate.leg_bolts]` `member` mapped —, the target's own plates and
  connections there replaced (their plate ids reused: applied again, the
  same file), the record's `ends`, `made`, `provisional`, `joints` and
  `hand` following; a copy running into more than the source in its own
  place left (said), a joint with a splice left. No format change. Tests:
  `joint_kinds_tests` (a body's corners of one kind, another section not
  alike, a mapped plan as the joint's own, a run once per kind as every
  joint on its own, the words), `apply_detail_tests` (a laid-out joint
  applied turned, a hand detail, only where asked, applied again the
  same), the app's `apply_ui_tests` (the preview, one undo step, an
  unticked joint left, Escape).
- **J9, pieces in line proposed one part, and the mutual-host guard**
  (2026-11-19, the owner's decisions D15 and D16; in active development,
  not approved yet; [theory](../theory/joint-layout.md#pieces-in-line-one-part-confirmed-j9)):
  `in_line::in_line_pieces(t, joints) -> Vec<InLine { joint, member,
  onto, not, roll }>` (from the file alone: pieces of one section in line
  within `CONTINUOUS_ANGLE_DEG`, the same reference and orientation,
  rolls within `ROLL_TOLERANCE_DEG` = `FLAT_OFF`; `not`: why not proposed —
  legs with bolts (J7), a splice, `kind`, `onto` or bolts given by hand at
  either end, the layout record's automatic fields not counting;
  `roll`: the one roll confirming writes when theirs differ),
  `in_line::confirm(doc, t, list) -> (written, not)` (the proposed pairs:
  a chain's rolls set to the mean of its pieces' — `record.rolled` dropped
  for them —, the layout's automatic values at those ends taken out,
  `connection_edits::set_continuous`), `confirm_text`. A run says them
  (`LayoutRun::in_line`, the joints asked) and never writes them; the
  window lists them (`layout_window::layout_in_line`: *Confirm*, *Confirm
  all*, one `StructureFile::edit` each). A run replacing values given by
  hand keeps `onto` of a connection given `continuous`
  (`in_line::one_part`). The planner: `joint_layout::mutual_hosts` (of
  two ends whose hosts are each other, or one the other's piece in line,
  the one running through, else the larger, stays host; its end goes to
  `JointPlan::left` with why); `through_of` and `host_in_plane` rank the
  piece on the end's side first among equals (`same_side`); a strip's
  `host_ends`, `host_cut`, `host_along`, `host_end` and the planes ordered
  by the depth of their hosts' own ends (`laid_cut`): `lands` asks every
  bolt on the host's steel past its cut by the end distance where the
  host ends at the joint (`bolt_along`, `cut_of`). The run's clash rounds
  (`write_rounds`): an end laid directly onto a member is checked against
  that member as written (`misfit_on`: the joint checks' flange and
  beyond-the-end rules) and left as it was where it misfits, the joint
  planned again. No format change (`kind = "continuous"`, `onto` and a
  member's `roll` as before). Tests: `in_line_tests` (proposed and never
  written by a run, confirmed and kept by a run replacing values given by
  hand, a run again identical; legs with bolts, a splice, bolts given,
  turned apart not proposed; rolls a little apart one roll; the layout
  before and after confirming, the piece on the end's side named; the
  redundant on the pieces before and after),
  `joint_layout_tests::members_bolting_onto_each_other_are_never_both_laid_out`,
  the app's `layout_ui_tests::pieces_in_line_are_proposed_and_confirmed_in_one_undo_step`.
- **J7 part a, leg splices proposed, bracing through the outer cover**
  (2026-11-19, the owner's decisions of the same day; in active
  development, not approved yet;
  [theory](../theory/joint-layout.md#leg-splices-proposed-j7)):
  `leg_splices::leg_splices(details, t, joints) -> Vec<LegSplice { joint,
  member, onto, not, covers, thickness, provisional }>` (from the joints in
  detail as the run plans them: two pieces of a leg in line within
  `NESTED_ANGLE_DEG` of other sections — or of no leg, both bolted, nothing
  else on them: step 1c's rule —, their legs along each other; covers outer
  and inner on each leg bolted, the thicker piece's leg thick; the pieces
  without bolts given provisional ones; `not`: a splice given, `kind`,
  `onto`, `face` or bolts given by hand, not single angles, in line within
  more than 5°, legs not along each other, nesting; pieces spliced together
  already not listed), `LegSplice::assumed()` (the words of what is
  assumed), `leg_splices::confirm(doc, t, list, bolt)` (each a `[[splice]]`
  — `layout_run::write_splice`, shared with step 1c —, both ends `onto`
  it, provisional bolts `layout_run::write_bolts` listed in the record's
  `provisional`; the layout's own values at those ends taken out),
  `confirm_places(doc, t, one_part, splices, bolt)` (one edit: the same
  section's pieces one part by `in_line::confirm`, then the splices),
  `confirm_text`. `InLine::splice_bolts`: pieces of a leg with bolts are
  now proposed one part (the owner's rule), flagged, and listed with the
  splices; J9's list leaves them out. A run says them
  (`LayoutRun::splices`) and never writes them; `joins_of` makes no splice
  any more (step 1c's branch gone, `JoinKind::Spliced` kept for the writing
  and the clash rounds of a splice an older run made). `strip_joint` keeps
  an end's `onto` naming a splice the layout did not make (also when
  replacing values given by hand) and its provisional bolts listed;
  `LayoutRecord::is_empty` counts `provisional`. Through the cover:
  `joints::through_cover` (`Through { splice, leg, outer, inner, leg_t }`:
  an end lapped on the outer face of a spliced piece's leg with an outer
  cover there), `Arrival::through` (the splice and the leg): the end and
  its bolts moved out by the outer cover's thickness, the grip its own leg,
  then the outer cover, the leg and the inner covers; the outer cover's outline round its
  bolts too (`cover(.., through, ..)`); `clash`: such an end lies on the
  splice's covers, its bolts' shanks through them; `plate_checks::cover_plans`:
  the outer cover planned for the end's whole force (`share` 1, said). The
  planner: `Strip::through` (`outer_cover`), the strip's face and plane
  key moved out by the cover, the cover not fixed steel to it
  (`fixed_in`), such a plane bolted directly only — the ends whose bolts do
  not land left for a hand detail ("… a plate over the cover is not laid
  out"). No format change. The window: `layout_window::layout_leg_splices`
  (*Leg splices*, *Confirm*, *Confirm all*, *Assumed: …*, one
  `StructureFile::edit` "Leg splice places (n)"; provisional bolts: the
  preset for `leg`, else the bolt picked). Tests: `leg_splices_tests` (a
  change of section proposed and never written by a run, confirmed, kept
  by a run replacing values given by hand, two shear planes, a run again
  identical; the brace through the outer cover — seated, its grip, no
  clash, the cover checked for its whole force; provisional bolts listed
  and kept; the same section with bolts proposed one part; reasons said),
  `in_line_tests` (legs with bolts flagged), the app's
  `layout_ui_tests::leg_splices_are_proposed_and_confirmed_in_one_undo_step`
  (its picture `tool-lay-out-joints-leg-splices.png`).
- **L1, the owner's quick round** (2026-11-20, the ideas of 2026-11-19; in
  active development, not approved yet;
  [theory](../theory/joint-layout.md#the-owners-quick-round-l1)): three
  switches of `LayoutSettings`, on by default (`RecordSettings::layout` and
  the window take the defaults; not in the file — no format change).
  `bottom_up` (a): `joint_order::joint_order(t) -> JointOrder { zone,
  rank }` (`Zone::{Body, Arm, Top}`: the legs' lines of class `leg` within
  30° of the vertical, `LEG_STEEP`, give the body's outline at each height
  as a convex hull; a joint within `ON_OUTLINE` = 0.10 m of it is of the
  body; the order the body bottom up, then the arms and the top by how far
  out); `layout_run::plans_of` plans the joints in that order
  (`joint_kinds::kinds_in`: the first of a kind its representative) and
  lists them in the file's; the roll stage turns a member no one roll lays
  flat, none of its ends flat now, to lie flat at its end first in the
  order (said, kept only where it helps). `more_plates` (b):
  `joint_layout::plan_joint_ctx(.., ctx: &PlanCtx { plated, plain })` —
  `plan_once` with faces forced onto a plate (`PLATE_TRIES` = 4 faces
  bolted directly tried where the plan leaves members; strictly more ends
  wins); `write_rounds`: an end bolted directly that would be left (or is
  pulled back past its longest pull-back) is put in `plated` once and the
  joint planned again, kept where the plan lays it out (`Checked::plated`
  in the memo). `onto_search` (d): after the half turns, for each end the
  plans leave (its `onto` not given by hand), `joint_layout::onto_candidates`
  (single angles at the joint, 20°–160°, in a flange's plane, not bolting
  onto it; today's rule's order, those running through first) — each of
  `ONTO_TRIES` = 2 passes names the next one in a copy of the planning
  file, works out those joints in detail and plans them alone; a joint
  takes them where its plan lays out more ends and leaves no new end in
  `left`. The guard (`layout_run::Guard { changed, plain }`, for (d) and
  for (b) in the plan): after `write_rounds`, a joint L1 changed that lays
  out fewer ends than today's plan of it is planned again as today (its
  names taken back, `PlanCtx::plain`) and the clash rounds run again on it
  (the memo keeps the rest). Not in `plan_relaxed` (the estimated
  eccentricities). (c), redundants and the body's diagonals bolted directly
  on a face going onto a plate, was built, measured and taken out (no
  gain). Found measuring, for every run: `write_rounds` checks the holes of
  the ends laid directly onto a member's run overlapping
  (`holes_overlap`, the joint checks' rule) and a plate's own bolts
  (`[plate.leg_bolts]`) within its outline and on that member's steel past
  its cut as written
  (`plate_checks::ray_to_outline`), leaving such members as they were.
  Tests: `joint_order_tests` (the zones and the order of an invented tower
  with an arm and a peak, no legs), `layout_run_tests` (a member turned to
  lie flat at its end on the body; an end left on a built-up leg laid onto
  the other diagonal; holes overlapping as the checks say; the 8° strut
  with `bottom_up` off as before), `joint_layout_tests` (a face put onto a
  plate though its bolts land; the candidates' order).
- **The bent plate's own bolts (J2b fix) and L2, clash-aware planning**
  (2026-11-20; in active development, not approved yet;
  [theory](../theory/joint-layout.md#clash-aware-planning-l2)). J2b fix, for
  every run: `joint_layout::bent_over(.., laid_cut, ..)` places the two
  holding bolts (`HoldPlan::points`) on the steel of the member they bolt
  onto where it ends at the joint — past its cut as laid out in the plan
  (`laid_cut`), else as given or worked out, else the work point, by the
  end distance (`Strip::host_end`) —, the pair moved out along it, the flat
  with them; where that member's own end is laid out in the plan, the
  flats laid out where it will be drawn (with `exact_plan` the plane is
  shifted already, below; without, `moved3` in `bent_over`; a member on a
  welded plate in its own plane: the bent plate refused, said);
  `write_rounds` also leaves the members on a plate whose own bolts would
  overlap a hole of that member's own end or of an end bolted onto its run
  (`holes_overlap`), and (found measuring L2) the bolts of each member on
  a plate it made outside the plate's outline (`ray_to_outline`, the joint
  checks' rule) and a member not laid out (nor left for a hand detail)
  bolting onto one laid out that would then not fit onto it (`misfit_on`,
  the other way round): such members, or the member laid out, left as they
  were. L2, three switches of `LayoutSettings`, on by
  default (not in the window): `plate_on` (a) — `write_plan` writes each
  plate's `on` (structure file version 23, `PLATE_ON_VERSION`, the member
  whose flange it lies on; not for a plate in its member's own plane) and
  `joints::plate_seats` takes it as the plate's `leg` (its seat's plane,
  the flange under its worked-out outline, the clash check's leg and
  `PlateDetail::leg`); `exact_plan` (b) — per strip, besides `fixed`
  (its bolted leg's slab), `fixed_out` (the fixed steel up to its
  outstanding leg's height, `out_w` + the thicker of its leg and the
  flange, tested over `out_band`) and `fixed_far` (behind the face, to the
  thicker of the two plus `BOLT_DRAWN` nut and beyond, tested over each
  bolt's disc of the nut's corner radius) from `fixed_slab` /
  `footprint_in`; `place` keeps clear of both; `solids_of` adds each bolt's
  nut and shank beyond behind the flange or plate and draws a plate as
  `plate_outline2` (the outline `joints::plate` works out: the work point,
  each leg over its bolts from its first bolt less the end distance — not
  before the work point — to its last plus it, each bolt's ring of its end
  and edge distances in a plate of the end's leg's thickness, `keep_plate`,
  the flange under it across its width as far as those reach; cut at the
  heel as the run cuts it); `plate_hits` tests that outline in the plate's
  thickness against the fixed steel (not a member on it, the member it
  lies on or one in line with that, nor a member left for a hand detail),
  the plates and covers given by hand and what the planes before laid out
  (2D separating axes, `sat2`, above `CLASH_TOLERANCE`); a plane on a
  member laid out in the plan (its planes first) is shifted where that
  member will be drawn (`heel3`, `face3`, `heel`, `w`, `obstacles`: its
  move across, `v3 · moved`, and the plate under it). `clash_tries` (c) —
  `PlanCtx::direct`: on a face going onto a plate, those members (their
  bolts landing) bolted directly; `write_rounds` puts an end on a plate
  that would be left as it was there once (`directed`, `Checked::directed`
  in the memo; the replan closure takes the set), kept where the plan lays
  it out; in the plan, where the plate hits, the ends whose bolts land
  bolted directly, the others left, said ("on a plate on …, the plate
  would run into …"). L1 (c) rebuilt as `LayoutSettings::direct_first`
  (`PlanCtx::body` from `joint_order`), off by default (measured again).
  Tests: `bent_plate_tests` (the holding bolts past the work point on a
  member ending at the joint, within the flat's outline, the flat seated
  on it; the plates' `on` and version 23), `joint_layout_tests` (an end
  asked bolted directly leaves its plate, with clash-aware planning off and
  on), `layout_run_tests` (the end laid onto another member left when its
  plate, seated on that member, would run into the leg).
- **L2's follow-ups and L3, a plate bent by a few degrees** (2026-11-20; in
  active development, not approved yet;
  [theory](../theory/joint-layout.md#l2s-follow-ups-2026-11-20)). Three
  switches of `LayoutSettings`, on by default (the window lays out with them on):
  `short_plates` (a) — where `plate_hits` finds the plate's outline round the
  work point (`plate_outline2(.., Outline::Round)`) running into something,
  the short one (`Outline::Short`: no work point, each leg from where its
  nearer edge crosses the flange's toe, the flange only as far as the legs
  and rings reach) is tested, then cut straight (`separate`: the axis
  parting `Outline::Core` — the legs and the rings — from what it hits
  farthest, the plate stopping half the gap short, or half the parting; at
  most `CUT_TRIES` = 4) while it still lies on the flange over half a square
  of its width (`on_flange`); kept: `PlatePlan::short` (the record's
  `made` mark `layout_run::SHORT`: `joints::plate` draws it short, its
  `leg_strip` from the flange's toe) and `PlatePlan::cuts` (applied by
  `write_rounds` where it writes the outline, beside the heel's cut);
  `plate_hits` now gives what it hits as its footprint too; `solids_of`
  takes the plate's 2D outline. After writing, a bolt on a short plate
  nearer the outline than its distances (the joint details' `notes`) leaves
  its member, said. `slide_holds` (b) — `bent_over` slides the holding
  pair along the leg (millimetre steps, either way, up to the longest
  pull-back) until each keeps the default minimum spacing from the holes
  through that leg (`leg_holes`: the member's own end and the ends onto it,
  planned or as they are); none: refused, said. (c), for every run: the
  record's `ends` mark `onto=<member>` (`layout_run::ONTO_HAND`,
  `hand_onto`) where `write_plan` writes an `onto` given by hand that the
  plan kept (onto that member or a plate on it); `strip_joint` puts it back
  (unless replaced), the follow drops it when the edit changes `onto`, the
  window says "onto (yours: …, kept)". **L3**, `small_bends`: step 4's
  `own_geometry` takes a `Room` — `Weld` as before, or `Bend { down }`
  (the seam the bend; the steel the curved part (r + t) tan(θ/2) and half
  the gap from it, the bolts their edge distance from the curved part; the
  member lifted the least, or — towards the flange — lowered the least,
  `q` and the extreme taken the other way); `rooms_for` gives both bends
  then the weld for an end at most `BEND_SMALL_MAX` = 20° off; each
  `toe_clear`ed with its own cut, then `own_geometry`, `within` (bent:
  `|out_more|` at most the plate's thickness) and `own_fits`; of the bends
  that fit the least `|out_more|` (up on a tie), else the weld; the plate
  in the member's plane gets `OwnPlane::bend` (`BendPlan`: the angle, the
  inside radius `bend_radius`, the line where the middle planes meet,
  `OwnGeo::middle`), no weld — written by `write_plan`'s existing
  `[plate.bend]` branch (version 22, no format change). `strip_of`'s early
  test of a tilted end takes the union of the rooms that fit; the words
  of a hand detail say why each does not (`rooms_words`). Tests:
  `layout_run_tests` (an end 10° off on a bent plate: written, flat on it,
  no clash, its bend a row of the joint checks, a run again identical, the
  follow as the whole file's, off: welded as before; a plate marked short
  drawn without the work point, convex, its bolts' distances kept; an
  `onto` given by hand kept in the record and by a run again, replaced
  when asked), `joint_layout::cut_tests` (the straight cut), the welded
  plate's test now with `small_bends` off.
- **L3b, the bent end's turn and the end running back to its bend**
  (2026-11-20; in active development, not approved yet;
  [theory](../theory/joint-layout.md#the-bent-ends-turn-and-the-end-running-back-to-its-bend-l3b)).
  Two switches of `LayoutSettings`, on by default with `small_bends` (the
  window lays out with them on). `bend_facing`: step 2b (the half turns)
  then, for an equal-leg angle (`either_leg` at every end) it flipped
  without gain whose plan leaves an end at most `BEND_SMALL_MAX` off with
  "its outstanding leg points towards the flange", the quarter turn that
  `joint_layout::turned_end` finds facing that end's leg away (the leg
  lying flatter after the turn, its off on the flange `flat_of` would take,
  the facing as `strip_of` reads it on either face) — all such members in
  one more `planned_with` pass (flat plates and L3 bends, no J2b), each
  kept as the half turns are (more of its ends planned, its joints no
  fewer); the first plan reused where every one is kept. `bend_back`:
  `Tilted::back` (and `Tilted::band`, the flange across and its width)
  lets `own_geometry` take `Room::Bend { down: false }` for an end whose
  `q` runs against its direction (`dot(q, dir) < −sin FLAT_OFF`, refused
  before): each leg edge's steel kept up the flap only from the cut to
  where that edge leaves the flange's band by half the gap (past the toe,
  or the heel), not to half its length; the bolts and the limit as before.
  The holding bolts' slide is now `slide_holds` / `holds_clear` (pure, the
  same behaviour). Tests: `joint_layout::slide_tests` (the slide: least
  either way, the tie out, stays where clear, none past the reach),
  `joint_layout::back_tests` (an end running back refused, then laid out
  with its steel clear over the flange and past the bend's plane beyond),
  `layout_run_tests` (`turned_end`: a half turn keeps the off and turns the
  facing; the two quarter turns face opposite ways), and the app's
  `layout_ui_tests` (the window lays a few-degree bend out; its picture).

## Detail levels

Joint details exist at **level 3** only (full design). At levels 1 and 2
the Joint panel shows the members that meet and says that connections
belong to level 3. A pole has none.

Questions for each level, answered by the owner on 2026-11-02 ("yes and
yes"):

- A level 3 tower **without** connection data keeps today's per-member
  flags and conservative assumptions, said per member.
- The self-weight factor stays until every joint is defined (Stage 4.6;
  built 2026-11-19: then the envelope of the counted mass and the factor
  until the counting is validated, [theory](../theory/structure-weight.md)).

## Order and dependencies

```text
Stage 0 ─ Stage 1 ─┬─ Stage 2 ─ Stage 3
                   └─ Stage 4.1, 4.2, 4.6 (data only, no UI needed)
                       Stage 4.3 after Stage 3 (needs the bracing points)
                       Stage 4.4 with O7 (level 4 interaction; its
                         eccentricities from shifts built 2026-11-02)
                       Stage 4.5 with O4 (code-check interface)
Stage 5 alongside
```

The proposal is Stages 0–2 first, so the joints can be **seen** on the
towers already in the projects, then 3 and 4.1–4.2. Those change the
results most for the least work. (4.1 and 4.2 built 2026-11-02.)
