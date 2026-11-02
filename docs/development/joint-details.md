# Joint details of towers (roadmap, proposal)

*Proposal, 2026-11-02 — not built, not approved. Asked by the owner:
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
| Checks using it | ASCE 10-97 (the default): the slenderness curve from the end eccentricity and restraint, bolt shear, bearing, the net section; missing data take conservative assumptions, said per member | `ohl_structures_fe::asce10` |
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

### Stage 0 — Pole or tower, a toggle (S)

- The structure's kind becomes editable after the structure is made:
  **Geometry › Structure kind** in OHL-Structure (lattice tower, pole,
  portal, other), and shown in OHL-Design's structure picker as today.
- **Tower** (lattice, portal): the Joints and Members tables, the joint
  details below, ASCE 10-97 by default. **Pole**: the pole panel; no
  joint details (a pole's flanges and slip joints are a later, separate
  item).
- Changing the kind never deletes data: what the new kind does not use
  is kept, hidden, and the switch says so.

> **Owner:**

### Stage 1 — The data: connections at the member ends (M)

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

### Stage 2 — Showing a joint (M)

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

### Stage 3 — Defining joints (M)

- Edit any connection in the table; the close-up and the face view follow.
- **Bolt layout helper:** n bolts in one or two gauge lines; pitch and
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

### Stage 4 — What the calculations take from it (M–L)

Each item is an option or a default to agree; every result says which
values were worked out from the joints and which assumed.

1. **ASCE 10-97 slenderness curves** (in the core today). End
   eccentricity and end restraint are worked out per end:
   - one leg bolted → eccentric at that end;
   - two or more bolts into a stiffer member (or a plate held by one) →
     partly restrained.

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
4. **The FE model (options, measured before any default):**
   - the connections' eccentricity as offsets at the member ends (moments
     in frame legs);
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
6. **Weight and quantities.** The bolts and plates counted and weighed:
   - the weight replaces the `dead_load_factor` estimate (1.1–1.4 today)
     where every joint is defined;
   - the bill of materials lists bolts by diameter, length and grade, and
     the plates.

> **Owner:**

### Stage 5 — Exports and validation (M)

- IFC and Tekla exports with bolts and plates (structure exports).
- Validation: the per-end connection capacities against hand
  calculations of ASCE 10-97 cases, and against a tower program's
  connection results on the local towers (comparison kept in
  `local-data/`).
- Theory page: how each condition is worked out from a joint, with
  drawings (an angle bolted by one leg and its eccentricity; a redundant
  bracing a leg about one axis).

> **Owner:**

## Detail levels

Joint details exist at **level 3** only (full design). At levels 1 and 2
the Joint panel shows the members that meet and says that connections
belong to level 3. A pole has none.

Questions for each level, to agree before Stage 4:

- Should a level 3 tower **without** connection data keep today's
  per-member flags and conservative assumptions? The proposal: yes, said
  per member.
- Should a joint partly defined count as defined for the weight (Stage
  4.6)? The proposal: the factor stays until every joint is defined.

## Order and dependencies

```text
Stage 0 ─ Stage 1 ─┬─ Stage 2 ─ Stage 3
                   └─ Stage 4.1, 4.2, 4.6 (data only, no UI needed)
                       Stage 4.3 after Stage 3 (needs the bracing points)
                       Stage 4.4 with O7 (level 4 interaction)
                       Stage 4.5 with O4 (code-check interface)
Stage 5 alongside
```

The proposal is Stages 0–2 first, so the joints can be **seen** on the
towers already in the projects, then 3 and 4.1–4.2. Those change the
results most for the least work.
