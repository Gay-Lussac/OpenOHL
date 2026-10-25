# Measuring tools and annotations (proposal)

Status: **proposed** (draft for the owner, 2026-10-25); **phase 1 built**
(2026-10-25: Distance, Height above ground, Stations, the status bar's
station and offset, the Measurements panel — in active development, not
approved); **phase 2 built** (2026-10-25: kept measurements and notes as
annotations — leaders as notes with a fixed short leader, placement by
hand not yet); **T11 follow and snap built** (2026-10-25: wires, jumpers
and structures; a kept one keeps its LiDAR point — open question 2 not
decided yet). Nothing else here is built or approved. The owner decided four of the first draft's questions
(2026-10-25: stored apart from the line; wire distances to the surface;
an annotation deleted with its object; shared by everyone, others' hidden
in the display options) and asked for one more tool (T11, follow an
object and snap to LiDAR classes); the remaining open questions are at
the end, each with a recommended default.

## Purpose and scope

A designer checks a line by asking small geometric questions all day:
how far is this wire from that roof, how high is the conductor above
the road at this station, how far apart are these two circuits where
they cross, what is the line angle here. Today OpenOHL answers such
questions only through the checks (rules, runs, reports). This proposal
adds:

1. **Measuring tools**: interactive, on-the-spot measurements in the
   plan, profile and 3D views, nothing stored unless asked.
2. **Annotations**: measurements, dimension lines, leaders and notes
   kept with the project line, recomputed when the line changes, and
   drawn on the sheets and exports.

Out of scope: new checks (rules, limits, pass/fail), changes to the
engine's computed geometry, and any measuring a check already does
(those stay the checks' own; a tool may reuse their geometry code).

## The main goal: responsive, fast, intuitive (owner, 2026-10-25)

The tools are judged first by how they feel in use. Every design choice
below gives way to these, except precision (computed values are never
coarsened: *quality before speed* — speed comes from searching less, not
from measuring worse).

- **Responsive**: the value under the cursor follows it every frame.
  Target: a hover update (snap search, the measurement, its overlay) well
  within one frame, 16 ms, on the largest real line and its LiDAR; a
  search that cannot finish in that time shows the last value with a
  "…" and finishes in the background, never freezing the view.
- **Fast**: nothing solved again to measure (the solved shapes as
  drawn); searches local (LiDAR through the cloud's blocks by box, wires
  through the clearance crate's chunk boxes, structures through the
  spatial index); the exact closest-approach code with its pruning. Each
  tool gets a benchmark beside the existing ones, with its time in
  performance.md.
- **Intuitive**: one tool, then pick — no dialog before the first
  measurement; the snap target named and marked under the cursor before
  the click; the value always visible next to the cursor, not only in a
  panel; sensible defaults (case shown, surface distance, classes shown),
  options in the tool's small panel, remembered per user; Esc and
  Backspace always step back; everything kept is one undo. A first-time
  user should measure a wire's clearance to a tree in three clicks
  (T11: the wire, the classes, the spot).

Each phase is tested for these as for its values: the hover time
measured in the benchmarks, and the clicks of each tool's main use
written in its user-guide section.

## What exists today

- **Distance between conductors / unequal wind results** draw one
  measured line with a label and the two conductors of the pair in the
  3D view: `View3d::measure`, `measure_wires`, `measure_from`
  (`crates/ohl-app/src/view3d.rs:76-83`), painted on top at
  `view3d.rs:1590-1640`; the types `Measure = ([f64; 3], [f64; 3],
  String)` and `MeasureWire` (`crates/ohl-app/src/phase_distance.rs:362-380`).
  Only a check result sets them; no user tool does.
- **Exact closest approach of two wires**: segment against segment,
  pieces' boxes nearest first, farther pieces left out —
  `segments` and `approach` in `crates/ohl-clearance/src/unequal_wind.rs:247`
  and `:289`; the general `closest_between_segments`,
  `closest_on_segment`, `closest_on_triangle` in
  `crates/ohl-clearance/src/geometry.rs:41-94`. The phase distance
  measure (stations every Δs, closest point searched continuously) is
  described in [phase-distance-check.md](phase-distance-check.md).
- **Selection and picking**: one `Selection` shared by all views
  (`crates/ohl-app/src/selection.rs:14`: structure, jumper, span,
  attachment, string, point, alignment point, span point, spread,
  spacer, jumper point, LiDAR point, survey point); the snap kinds of
  Edit › Snap (`Snap`, `selection.rs:146`); `pick_in_model`
  (`selection.rs:1089`) and, for case-dependent objects, every drawn copy
  of every weather case shown, with the case of the copy clicked kept in
  `Model::selection_case` (`selection.rs:1152-1156`,
  `crates/ohl-app/src/model.rs:578`). LiDAR points can be clicked only
  where drawn (`pick_lidar`, `selection.rs:881`).
- **Tools** are data (`Tool::ALL`, `crates/ohl-app/src/tools.rs:14`): a
  held tool acts on a right-click, a left-click always selects
  (`controls::click_buttons` swaps them).
- **Box / lasso** areas on a view's screen (`crates/ohl-app/src/area.rs`).
- **Labels next to objects** per view (`SectionDisplay`,
  `crates/ohl-app/src/annotations.rs`: section label, sag at mid-span,
  ids, point and string labels), one user's view preference
  ([ui.md](ui.md#display-preferences): text in the object's colour, no
  box, overlapping notes left out).
- **Stations and offsets** along a route: `point_at`
  (`crates/ohl-line/src/route.rs:360`), `span_stations` (`route.rs:608`),
  the profile's station/offset of points (`crates/ohl-app/src/profile.rs:829`).
  Ground height: `crates/ohl-terrain/src/lib.rs:582`.
- **Sheets** already draw leaders (elevations with a thin dashed leader,
  layer `OHL_ELEVATIONS`, [drawings.md](../user-guide/drawings.md)); the
  export layers are the `OHL_*` names of `crates/ohl-drawing/src`.
- **Storage**: every change is an `ohl_session::Command`
  (`crates/ohl-session/src/lib.rs:401`), objects are `model_object`
  rows by `Kind` (`lib.rs:298`); a performed check is a `report` row,
  kept with the project line but not part of it.

Nothing today lets a user pick two arbitrary things and read a
distance, and nothing stores a user's dimension or note.

## Common rules for every tool

### Picking and snapping

A measuring tool takes **picks**. Each pick snaps to the nearest target
of the kinds switched on in Edit › Snap, plus the measuring-only kinds
below (a new group "Measuring" in the Snap menu, ticks kept open as
every tick menu):

| Target | Resolves to | Case-dependent |
|---|---|---|
| Structure | its base point (plan position, ground height) or, with Shift, its top | no |
| Attachment point (insulator top, wire clamp) | the point as placed | clamp: yes |
| Wire point at the cursor | the closest point on the drawn wire of that copy | yes |
| Wire point at a station | a typed station (or span fraction) on the picked span | yes |
| Ground | the ground surface under the cursor (the surface the checks use) | no |
| LiDAR point | the point as stored behind the one drawn (as the Edit tool does) | no |
| Survey point | the stored point | no |
| Background drawing vertex / segment | the vertex, or the closest point on the segment; height from the drawing when it has one, else from the ground (stated) | no |
| Alignment point, route axis | the point; the axis at the cursor's station | no |
| Free point | the cursor in the view's plane (plan: ground height; profile: the station and height under the cursor; 3D: on the ground, or on a plane through the previous pick, chosen with a key) | no |

A pick on a case-dependent target remembers **which weather case's copy**
was clicked (the rule of *pick in every case*: every drawn copy can be
clicked, all copies of the picked object highlighted).

Snap feedback: a marker shaped by target kind under the cursor, the
target's name in the status bar ("clamp T12 set 3, case −5 °C + ice"),
and Alt held to pick free (no snapping), as for the drag handles.

### Weather cases

Wire positions depend on the solved case. Proposed:

- Each pick on a wire keeps its case. A measurement between two wire
  picks uses each wire's own case (so "phase A at max temperature to the
  ground wire at ice" is possible) and says both cases in the result.
- **Per case** (a tick in the tool's options): the measurement is
  repeated in every case shown, one row per case, the worst first. A
  pick on a case not shown any more keeps its value but is marked
  ("case not shown").
- A case not solved yet: the row says "solving…" and fills in when the
  case is ready; never a value from another case.

### What is shown

Every result gives the **value** and its **components**, each where it
makes sense:

- slant (3D) distance; horizontal (plan) distance; vertical difference
  (signed, from the first pick to the second);
- along the route and across it (station difference and offset
  difference, on the alignment chosen: the one of the picked span, else
  the nearest);
- the angle of the line between the picks from the horizontal, and its
  bearing in plan (in the project's angle convention, data-model.md
  "Coordinates and angles");
- the picks' own coordinates (easting, northing, height), stations and
  offsets.

The measured line is drawn on top in every view where both ends are
visible (as `View3d::measure` now; the plan and profile get the same
overlay), its label at its middle, in the overlay colour.

### Units and precision

- Values in the project's units (metres, degrees); shown to **0.01 m**
  and **0.01°** by default, the precision a user preference (display
  only). Copy and CSV give the full computed value (at least 1 mm,
  stored as `f64`).
- Computed values are **never coarsened** for speed: a view showing
  lighter geometry (level of detail, fewer points drawn) does not change
  what is measured; measurements use the solved wire shape and the
  stored points, not the drawn ones. Where only the drawn set can be
  clicked (LiDAR), the point measured is the stored point behind it.
- Every approximation is stated in the result's hover text and in the
  theory pages, marked **conservative** or **simplification** as the
  theory manual does. Known ones: wires as polylines with their error
  bound (the distance given less the bound, as the checks do —
  conservative); background drawings without heights take the ground
  height (simplification, stated).
- Wire distances are to the **surface** by default (owner,
  2026-10-25): the radii subtracted (bundles: to the subconductors, as
  the falling-tree decision of 2026-10-23); a tick "to the axis" gives
  the axis distance. The result says which.

## The tools

All proposed as entries of a new **Measure** group in the tool
inventory (`Tool::ALL`), so they can go on the hotbar and the quick bar.

### T1 Distance (point to point)

Two picks of any target kinds; Enter or a third pick starts a new one.
Shows all components above. Views: plan, profile, 3D. In the profile,
picks are in the profile's plane (station, height) unless they snap to
an object, which gives its true 3D point (the along/across split then
says the offset).

### T2 Polyline length and area

Picks in order, Enter to finish, Backspace removes the last. Shows the
total length (plan and 3D), each leg's length, and for a closed polyline
(Shift+Enter or a pick on the first point) the plan area. Views: plan,
3D (polyline on the ground or free), profile (length along the profile).

### T3 Height above ground

One pick (wire, attachment, structure top, LiDAR point, free point).
Shows the vertical distance to the ground surface the checks use under
the point, and, when LiDAR is loaded, to the highest ground-class point
within a small radius (default 0.5 m, an option). Views: all three.
Per case on a wire pick.

### T4 Wire to ground clearance at a point

One pick on a wire (or a station typed on a span). Shows:

- the **vertical** clearance to the ground straight under the point;
- the **shortest** (radial) distance from the wire to the ground
  surface and to the LiDAR points shown, searched within a radius
  (default the clearance search margin of the project defaults), with
  the closest ground point marked;
- optional sweep: along the whole span, the station of the smallest
  vertical and the smallest radial clearance.

It reuses the clearance crate's point and triangle distances (`geometry.rs`),
not a new method, and states that it is a measurement, not a check: no
rule, no limit, no pass/fail.

### T5 Wire to wire (shortest distance)

Two picks on wires (spans or jumpers, any sections, any circuits).
Shows the exact closest approach between the two picked spans —
segment against segment as in `ohl_clearance::unequal_wind::approach`
(`unequal_wind.rs:289`), proposed to be made public in
`ohl_clearance::geometry` so the tool and the check share one code
path — with the two closest points, the distance, its horizontal and
vertical parts and the angle from the vertical (β as in the phase
distance check). With **whole wire** ticked, the search covers every
span of both sections. Each wire in its own picked case (or per case,
both from the same case, as the phase distance check does).

### T6 Wire to object

One pick on a wire, one on an object: a structure (its members as
drawn in the solids, or its stick lines when no solids; said which), a
background drawing line or polygon (2D: in plan; 3D when the drawing
has heights), a LiDAR point or the LiDAR points inside a box/lasso
(the closest of them reported), a survey point. Shows the shortest
distance and the closest points, horizontal and vertical parts.

### T7 Along the route (station difference)

Two picks; shows the station of each and the difference along the
alignment, and the plan straight-line distance beside it. With picks on
two structures: also the span length(s) between them and the sum.

### T8 Offset from the axis

One pick; shows its station and signed offset (left/right of the
alignment in the direction of stationing), and the plan distance to the
nearest structure. Proposed also as a permanent readout in the status
bar while hovering any view (station, offset, height under the cursor).

### T9 Angles

- **Line angle at a structure**: pick a structure; shows the deflection
  angle of the alignment there and the bisector direction, and the
  angle between the incoming and outgoing spans of each section held
  (they may differ from the alignment's).
- **Swing angle**: pick a string; shows its swing from the vertical in
  each case (across and along the line), the values the solve already
  gives, not recomputed.
- **Free angle**: three picks (vertex in the middle); the angle in 3D
  and in plan.

### T11 Follow an object, snap to LiDAR classes (owner, 2026-10-25)

For clearance to vegetation, buildings or anything else the LiDAR shows,
along a whole object rather than at one picked point:

1. **Select the object first**: a conductor (a span, or the whole
   section), a jumper, a string, a structure, or any other object of the
   line that can be selected. The selection is the tool's subject; the
   case of the copy clicked is kept (wires), as for every pick.
2. **Choose the LiDAR classes** it snaps to: a tick list of the classes
   loaded (codes and the project's group names, as the LiDAR display
   options), staying open on a tick as every tick menu; default the
   classes shown. The choice is remembered per user.
3. **Move the cursor along the object**: the point of the object under
   (nearest to) the cursor is followed — on a wire the point at the
   cursor's position along the span, on a structure the nearest point of
   its members (solids when built, else its stick lines, said which).
   For that point the tool finds the **nearest stored LiDAR point of the
   ticked classes** (3D, to the wire's surface by default) and draws the
   measured line live with its value; as the cursor moves, the result
   jumps from point to point automatically (no click). A small panel
   shows the value, the point's class, its station and offset, and the
   horizontal and vertical parts.
4. **Click again to lock**: the measurement is fixed and created as a
   dimension annotation (anchors: the object point — wire by section,
   span and fraction; structure by its id and the member point — and the
   LiDAR point by its stored coordinates and class). The tool stays on
   the same object for the next one; Esc ends it.

Searched on the **stored** points, not the thinned ones drawn (quality
before speed), within a search radius (default the project's clearance
search margin, an option); no point of the classes within it: "none
within r m", never a farther guess. The LiDAR points are looked up
through the point cloud's blocks by box, so the search stays local.

Also offered: a key (**N**) jumps the cursor to the object's point with
the smallest distance over the whole object (the sweep of T4, for these
classes), so the closest spot is found without scanning by hand.

Views: plan, profile, 3D. Per case: the live value in the case of the
copy selected; once locked, per case if the tool's "per case" tick is
on.

### T10 Live measurement

Any measurement above can be **pinned**: it stays in a Measurements
panel and in the views, follows edits (a moved structure, a re-strung
section) and case changes, and shows its previous value beside the new
one until acknowledged. Pinned measurements are shared like
annotations (others see them, unless hidden in their display options;
below) and become annotations when kept.

## Measurements panel

A dockable panel (as the check windows) listing the current and pinned
measurements: one row per measurement (per case when per-case), columns
tool, picks (by object name), case(s), value, components. Rows can be
selected (the views show that measurement), copied (Ctrl+C: tab-separated
text with full precision), saved as CSV (as the check windows' Save CSV),
made into annotations, or deleted. Long errors scroll in a fixed box
(ui.md).

## Annotations

### Kinds

1. **Dimension**: a stored measurement of any tool, drawn as a
   dimension line (with extension lines in plan and profile) and its
   value. Its ends are **anchors** (below), so it recomputes.
2. **Leader with text**: an arrow from an anchor to a text placed in the
   view (the text's place stored as an offset from the anchor, per view
   kind).
3. **Note**: text attached to an anchor without an arrow (a tag on a
   structure, a span, a station range), or free at a coordinate.

### Anchors

An anchor names what it is attached to, by id, so renames follow it as
the other references do (data-model.md "How objects point at each
other"):

| Anchor | Fields |
|---|---|
| structure | `structure`, `at` (`base`, `top`, or a point label) |
| attachment | `structure`, `set` |
| wire at station | `section`, `span`, `fraction` or `station`; `case` (name) or "case shown" |
| span point | `section`, `label` |
| alignment station | `alignment`, `station`, `offset` |
| LiDAR / survey point | cloud name and the point's stored coordinates (a point moved by an edit is looked up again; removed: the annotation is deleted with it) |
| background vertex | drawing name and vertex coordinates |
| coordinate | easting, northing, height (height `null`: on the ground) |

### When the line changes

- Anchored values **recompute** when what they use changes (the same
  "what a result uses" rule as `crate::revision::Uses`).
- An annotation whose object is **deleted is deleted with it** (owner,
  2026-10-25), in the same command, so one undo brings both back; the
  delete confirmation says "and n annotations". The same when a case an
  anchor names is deleted, or a LiDAR or survey point it uses is removed.
- An anchor that can still be found but no longer resolves (a wire at a
  station beyond a shortened span; a case not solved yet) shows
  "solving…" or the reason, never a value from elsewhere.

### Where they are shown

- Views: plan, profile, 3D, each ticked on its own (a "Annotations" layer
  per view, as the Display options of `SectionDisplay`), and per
  annotation a set of views (default all three).
- **Sheets and exports**: dimensions, leaders and notes on the plan and
  profile sheets, in the case the sheet draws; a dimension whose ends
  are not both on a sheet is drawn on the sheet with its first end, its
  value written, the other end shown as "→ sheet n". Exported to their
  own layers: `OHL_DIMENSIONS`, `OHL_LEADERS`, `OHL_NOTES` (DXF layers;
  SVG groups; PDF optional content groups where the PDF writer supports
  them).
- Styling from the sheet template: a new `[annotations]` table in
  `.ohlsheet` (text height, arrow style, line weights, colour per kind,
  decimals); without it, the template's text style. The template format
  version is raised only if a key is added that older readers must
  refuse.
- Texts placed so they do not overlap (the sheets' existing rule:
  pushed apart with a leader), and never over the structure.

### Shared

Annotations and pinned measurements are **project objects** (owner,
2026-10-25): everyone sees them, each carries its author, they are
logged and undone per user and follow the edit grants. Each user can
hide the others': two ticks in the Display options, **Others'
annotations** and **Others' measurements** (default shown), a view
preference of that user like the other display ticks. One's own are
always listed in the Measurements panel; the others' there too when
shown, with their author.

## Data model proposal

- New `ohl_session::Kind::Annotation` (`model_object` kind
  `annotation`), one row per annotation or pinned measurement, kept with
  the project line and its revisions and branches but **stored apart
  from the line** (owner, 2026-10-25; like `report`): it does not change
  the line's hash, so adding a note does not mark any check result out of
  date, and the line file does not carry them (below).
- Row JSON (proposed):

  ```json
  {
    "id": "a-0007",
    "kind": "dimension",          // dimension | leader | note
    "tool": "wire-to-wire",       // for dimensions: which measurement
    "anchors": [ {...}, {...} ],
    "options": { "surface": false, "per_case": true, "whole_wire": false },
    "text": "",                   // leader / note text; for a dimension, a prefix
    "placement": { "plan": [dx, dy], "profile": [ds, dz], "3d": null },
    "views": ["plan", "profile", "3d"],
    "sheets": true,
    "pinned": false,              // a pinned measurement not kept yet
    "author": "user name",
    "last": { "value": 7.412, "cases": ["..."], "line_hash": "..." }
  }
  ```

- `last` keeps the value computed when saved, so a reader without the
  solve (CLI, review of a branch, diff) can show it and say it may be
  out of date.
- Commands: the existing `put` / `delete` with `kind: annotation` — no
  new command needed; Undo per user as for every object; the edit grant
  kind `annotation` (default: every designer).
- Spatial index: an annotation's plan bounds from its anchors, so
  partial loading finds it.
- Line file (`.ohl.json`) and OHL XML: **not** in them (stored apart);
  the project file holds them. Whether an annotations file of its own is
  wanted for exchange is open (below).
- [data-model.md](data-model.md) to be updated with the kind, the JSON,
  the commands and the layers when built.

## CLI (proposed)

- `ohl measure <project|line> --from <anchor> --to <anchor> [--case
  NAME ...] [--surface] [--csv]`: one measurement, anchors written as
  `structure:T12`, `clamp:T12/3`, `wire:S4/2@0.5`, `station:main/1234.5`,
  `xyz:E,N,H`.
- `ohl measure ... --clearance wire:S4/2` (T4), `--angle structure:T12`.
- `ohl annotations list|recompute <project>`: every annotation with its
  value now and its last value, those that no longer resolve flagged;
  exit code non-zero with `--fail-on-unresolved` (for scripted reviews).

## UI summary

- Tool inventory group **Measure** (T1–T9, T11) with icons; hotbar slots as
  any tool. Menu **View › Measure** with the same entries and
  **Measurements panel**, **Clear measurements** (Esc while a tool is
  held clears the current picks).
- Keys (proposed, to check against the existing map in desktop-app.md):
  **M** Distance, Shift+M the last measuring tool used, **Enter** finish,
  **Backspace** undo last pick, **P** pin, **A** make annotation,
  **Alt** pick free, **Tab** cycle snap candidates under the cursor.
- Right-click picks as for every tool (left with *Tools act on
  left-click*).
- Status bar: snap target, then the running value while the second pick
  is hovered (rubber band in the view).

## Delivery (proposed phases)

1. **MVP**: T1 Distance, T3 Height above ground, T7 along the route, T8
   offset readout; picks on structures, attachments, wires at the cursor,
   ground, LiDAR, survey points, free points; case shown only; plan,
   profile and 3D overlays; Measurements panel with copy and CSV.
2. Annotations in the project: the `annotation` kind (apart from the
   line), dimensions, leaders, notes, anchors, deleted with their
   object, undo, shared with the Display options' ticks for others'.
3. **T11** follow an object and snap to LiDAR classes (it creates
   dimensions, so after 2), then the wire tools: T4 clearance at a
   point, T5 wire to wire (shared closest-approach code made public in
   `ohl_clearance::geometry`), T6 wire to object, per-case results,
   wire at a typed station.
4. Sheets and exports: annotation layers, `[annotations]` in sheet
   templates, placement on sheets.
5. T2 polyline/area, T9 angles, T10 live pinning, the CLI.

Each phase with its own tests (values against hand-worked geometry and
against the checks' own results on the example line) and a theory page
section listing its assumptions.

## Decided (owner, 2026-10-25)

- **Stored apart** from the line (a `Kind::Annotation` like reports; no
  effect on the line's hash; not in the line file).
- Wire distances to the **surface** by default; the axis on a tick.
- An annotation is **deleted with its object** (one command, one undo).
- **Shared by everyone**; others' annotations and measurements can be
  hidden in the Display options.
- New tool **T11**: select an object, choose the LiDAR classes, the
  measurement follows the cursor along the object snapping to the
  nearest point of those classes, a click locks and creates it.

## Open questions for the owner

1. **Two wire picks in different cases**: allowed, or always one case?
   Recommended: allowed for T1/T5 picks, each case named; per-case mode
   uses one case for both.
2. **T11 after a change**: when the object moves (a re-strung section,
   another case), does a locked T11 dimension keep its LiDAR point, or
   search the nearest point of its classes again? Recommended: search
   again from the same object point (the dimension means "clearance to
   these classes here"), the previous point and value shown until
   acknowledged.
3. **T11 search radius**: default the project's clearance search margin?
   Recommended: yes, an option of the tool.
4. **Clearance tool (T4) and the checks**: should T4 show the required
   distance from the active clearance rules beside the measured one?
   Recommended: no in phase 3 (a measurement, not a check); later a
   link "open in Vertical/Radial clearances at this span".
5. **LiDAR in T3/T4**: ground surface only, or LiDAR points too, and
   which classes? Recommended: the ground surface the checks use, plus
   the LiDAR points of the classes shown, both reported separately.
6. **Display precision default**: 0.01 m and 0.01°? Recommended: yes,
   a user preference; full precision in copy/CSV.
7. **Dimensions spanning sheets**: drawn on the first sheet with a
   reference, or on both? Recommended: first sheet with "→ sheet n".
8. **Keyboard shortcuts**: M / Shift+M / P / A / N free in the current
   map? To be checked; recommended as above if free.
9. **Status-bar station/offset readout (T8)** always on, or only
   while a measuring tool is held? Recommended: always on, a tick to
   hide it.
10. **CLI scope**: is `ohl measure` wanted in the first phases, or only
    once annotations exist? Recommended: after phase 3.
11. **Exchange**: since the line file does not carry annotations, is an
    annotations file of its own wanted (to send them with a line)?
    Recommended: not now; the project file carries them.
