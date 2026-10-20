# Decision log

Every decision of the owner, and every change to how a calculation or a
format works, is recorded here with its date, newest first. The theory
pages, the user guide and the [data model](data-model.md) describe the
result; this page says *what was decided, when and why*. Nothing here is
approved as finished: everything is built and in active development
(not approved yet). Open questions are in the roadmap (Part 1).

## 2026-10-20

- **LiDAR points: what can be clicked, and changing a point** (owner's
  request): clicking LiDAR points was unreliable because a click looked at
  the points loaded when the project opened (a sample thinned to
  `--points`), not at those drawn — a streamed survey draws the nodes of
  its point budget, the chunks of points in memory draw a share at levels
  1 and 2, the plan draws none, the profile's dots were not clickable at
  all. Now **every point drawn can be clicked and snapped to, and no
  other** (3D: the nodes or chunks drawn in the last frame; profile: its
  dots; plan: none); of points within a pixel the one nearest the eye.
  Picking is display only: nothing computed changes. The Edit tool on a
  LiDAR point opens a window to change its **coordinates and class**: the
  new session command `change_points` changes the stored cloud point by
  point (blocks, levels of detail, bounds and counts made again from the
  blocks, the other points kept in order), logged and undone bit for bit
  as one step, through the server like every edit; the clearances are
  checked again and the ground built from the cloud when the project
  opened is built again. Conservative choices left to the owner: the
  clouds belong to the project, so a change is logged and undone on the
  project line it was made on and revisions of a line do not hold the
  points; several points at once only by the command (views select one
  point; box or lasso selection left for later); an empty block made again
  by an undo gets a new row id (its content is bit for bit the same).
  ([user guide](../user-guide/desktop-app.md#editing),
  [project files](../user-guide/project.md#changing-points),
  [data model](data-model.md#commands-the-only-way-to-change-a-project))
- **Rules by voltage and rule presets (confirmed by the owner):** a rule
  file is written only when every rule is complete (an unfinished rule
  shows "not written yet"); **Load preset…** reads the design-code plugins
  with the parameter file of Criteria › Design code, or none. A design-code
  plugin's presets for a national code (kept outside the repository) check
  the direct distances at the largest sag and under wind and the vertical
  distances at the largest sag; building distances are screened out to
  the largest horizontal distance the code asks; crossing presets that
  depend on the distance to the nearer tower or support are given only
  when that distance is entered (nothing assumed); its LiDAR class mapping
  confirmed.
- **Clearance rules by voltage in the app, rule presets from plugins**
  (owner's request): wherever a rule's distance is set (the Clearances
  tab's new rule editor, the Vertical / Radial clearances windows and
  their Add a rule) it is one distance, a table "up to kV: distance" or a
  base plus an amount per kV, with a preview at the project's voltages
  and 0 kV — as the rule files already allowed. The windows now keep a
  rule's requirement as written (`ZoneRule::distance` is a
  `Requirement`): a rule by voltage is one row, each wire checked with
  its voltage's distance, where an import gave one row per line voltage
  before (results unchanged: the same distance per wire); a table that
  does not reach a wire's voltage stops the window's check and `ohl zones`
  with an error, as `ohl line` already did. Design-code plugins may give
  presets of any clearance rule set through the new optional
  `rule-presets` export (interface 0.3 unchanged: the host looks for it,
  plugins built before load as they are); the app merges a preset into
  the chosen rule file or replaces its rules, one command; the CLI writes
  them (`ohl code rule-presets`, `ohl code generate`).
  ([user guide](../user-guide/desktop-app.md#distances-by-voltage),
  [plugins](plugins.md#rule-presets))
- **Danger trees built** (in active development, not approved): the
  method below as written, with these changes and readings
  ([theory](../theory/lidar-analysis.md#danger-trees-2026-10-20)):
  within the fall sector the distance is \(|q - H| - L\) (negative
  inside: by how much the tree reaches past the wire) instead of
  \(\max(0, \ldots)\) — same verdict, the margins as before; the
  resting angle at every TIN edge crossing along the fall direction (the
  TIN's resolution, exactly), rising ground seen from the base
  (**Conservative**); the smallest over a wire as the distance between
  the wire and the hinge circle (exact per piece; on a swing arc within
  1 mm below, **Conservative**); a tree whose nearest wire point is
  outside the sector's angles (sheltered) searched along the wire by the
  bound of a distance (**Simplification**), no shelter credited on an arc
  (**Conservative**); the arc's angles from the smallest to the largest
  of the three, the bundle's outer envelope on the arc (both
  **Conservative**), the three wires' points interpolated at the point's
  place (no radius interpolation error); the case's ice on one span as a
  weight spread along it, the wind on its bare diameter
  (**Simplification**); isoclearance slopes above 5:1 taken as 5:1 in
  the bounds (**Simplification**). Root-ball fraction: default 0 in the
  command line (results bit for bit as before), 0.1 proposed in the
  application (a user setting; open point for the owner). `ohl lidar
  vegetation` solves all the cases chosen together and reports per span
  the worst over them with case and position; its CSV changes to one row
  per kept point with case and position (`DANGER_CSV_HEADER`; the
  per-case blocks are gone). The vegetation, galloping, right-of-way and
  as-built commands now read a project file's own libraries, as `ohl
  line` does. New drawing layer `OHL_ISOCLEARANCE`. The cross-section
  view is left for later (no such view in the application yet).
- **Tree base needs a ground point near it** (owner, via the
  coordinator): `max_base_ground_distance` — a vegetation point's ground
  height (its tree base) is taken only when a ground point the surface is
  built from lies within that horizontal distance, else no base, no
  fall-in check, counted as "no ground point within the distance".
  Default no limit (results unchanged); a design-code plugin's preset
  may give 5 m (the private national-code plugin with root ball 0.05, a
  2 m clearance allowance and a 50 m search distance).
- **Danger trees (method written, to be built):** fall-in about a root
  ball (hinge at a fraction k of the height), the tree falling towards
  each wire point and resting on the ground along its direction; grow-in
  with the wire hot, blown left and right, and along its swing arc (a
  rigid swing about the attachment points' axis, the largest radius);
  an optional unbalanced-ice case per span; isoclearance lines over the
  corridor for one case or the worst. ([theory](../theory/lidar-analysis.md#danger-trees-2026-10-20))
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
