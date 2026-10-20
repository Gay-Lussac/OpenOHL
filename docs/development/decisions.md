# Decision log

Every decision of the owner, and every change to how a calculation or a
format works, is recorded here with its date, newest first. The theory
pages, the user guide and the [data model](data-model.md) describe the
result; this page says *what was decided, when and why*. Nothing here is
approved as finished: everything is built and in active development
(not approved yet). Open questions are in the roadmap (Part 1).

## 2026-10-20

- **Licence:** OpenOHL is under the **AGPL-3.0-or-later** (forks and
  modified versions, also those offered as a network service, stay open
  under the same licence). The plugin interface (`crates/ohl-plugins/wit/`)
  and everything under `plugins/` stay under **MPL-2.0**, so plugins may
  keep any licence. ([ADR-0011](../adr/0011-license-agpl.md))
- **Comparison tool for the existing line** (local): the "ice as weight
  only" variant no longer shows the wind on the bare diameter since an ice
  load without a density is taken as glaze ice (2026-10-19).

## 2026-10-19

**Calculations**

- **Tension-only members** carry compression up to their own elastic
  buckling load π²EI/L_k² (L_k = k·L, k = 1 unless given), then stay at
  it ("bowed"); always solves. Not modelled as sagging cables.
  ([theory](../theory/structures-fe.md)) Usage of bowed members without
  the Euler term, and k depending on the joint type: open point O12.
- **Tension-only members that bend** (`bending = true`): act as a frame
  while in the model, as a truss when bowed or slack; members can be
  solved without a check (`check = false`). Read from another program's
  "tension-only beam" for the imported towers; the reading confirmed by
  the owner. The structure exports keep the bending.
- **Relaxed refusal accepted:** a truss rod may carry load across itself
  through a relative hold (flagged "LARGE", not refused). Rebuilding an
  imported tower's rounded primary joints: later.
- **Joint stabilisation:** a free joint on a straight line of members is
  held relative to the line's ends (its load goes through the structure;
  with P-Delta it follows its face); others are held to the ground. A
  relative hold above the old 1 % limit is flagged ("LARGE"), no longer
  refused; a hold to the ground above it is still refused.
- **Clearances to the conductor's surface**, not its axis (the iced
  radius in a case with ice), less the polyline's error bound; sampling
  errors bounded on the safe side; galloping ellipses as circumscribed
  polygons. Results move by the radius (about 17 mm bare, 30 mm iced).
- **Phase distance (check plugins): between the conductors' axes**, as
  the codes define it (LeV: between axes), less the polylines' bounds.
  ([check](phase-distance-check.md))
- **Ice given as a load without a density:** taken as glaze ice
  (913 kg/m³), never the bare diameter.
- **Allowable spans between two line angles:** interpolated linearly in
  sin(θ/2), on the safe side for a structure responding linearly.
- **Wind on structures:** a load case may give the structures their own
  wind pressure and factor, apart from the wires' (`[case.structure]`,
  `.ohllc` 2); one rule for the line and OHL-Structure.
- **Drag and shielding of lattice towers:** factors are data (a `[wind]`
  table, from a design-code plugin or typed), never built in; without
  them the generic drag stays and nothing is shielded. Only members in a
  face turned to the wind are shielded; the face outline is the convex
  hull of its joints; no factor outside a table's range.
- **Wind bands on an extension** rise with the structure (heights above
  the ground of the structure as written); the force is unchanged.
- **Transparency:** every conservative assumption is marked
  **Conservative:**, every other simplification **Simplification:**, in
  the theory pages ([conventions](../theory/conventions.md)).

**Formats**

- **Structure exports keep a tension member's bending** (owner): a
  `tension` member with `bending = true` is exported as a frame
  (IFC4 analysis view: `.RIGID_JOINED_MEMBER.` with rigid ends, as frame
  members are; SAF: `Behaviour in analysis` Standard), its tension-only
  behaviour kept as information (IFC `OHL_Member` Kind `tension`,
  `Bending` true; the analysis member's `Description`; SAF `Layer`
  "OHL: tension only, bending"; CSV `bending`). Members with
  `check = false` are exported as any other, flagged (`OHL_Member`
  `Check` false, `Description` / `Layer` "not checked", CSV `check`).
  SAF's "Tension only" is not used for them: it carries axial force only.
  ([exports](structure-exports.md))
- `.ohls` version 6: body-generator options, recipe values of the
  structures' wind, bending/unchecked members (one unreleased raise).
- `.ohllc` version 2 (structures' wind), `.ohlfe` version 3, the section
  library keeps version 1 with an optional `[wind]` table.
- Drawing settings are stored in the project, per project line.

**Applications**

- Drawings: one **line width** (±40 m) for the profile and the plan; it
  decides what is drawn (wires, structures, survey and LiDAR points).
  Sheets end only at a structure on the line or at an alignment point
  without a structure, never beside the line; the break clearance is a
  setting (2 mm on paper, 0 to 20 mm); a sheet preview in the app; the
  structures' elevations may be drawn above or below the line on the
  plan (90°, offset from the centreline).
- Level 2 and 3 members and poles drawn with their real outline; solids
  within a distance of their own (400 m by default).
- Drag handles in the line's 3D view; joints stay editable in
  OHL-Structure only; a dragged point takes what hangs below it along.
- Tools grouped by theme in the inventory and the Tools menu; an option
  for tools to act on left-click; a searchable coordinate-system picker.
- OHL-Structure: results on the chosen extension, the pole check, the
  structure report (format to define with the owner); `ohl structure`
  asks OHL-Structure's headless tool, the programs exchanging JSON.

**Roadmap answers:** see ROADMAP.md Part 1 (O1–O13) for the owner's
answers in full.
