# Decision log

Every decision of the owner, and every change to how a calculation or a
format works, is recorded here with its date, newest first. The theory
pages, the user guide and the [data model](data-model.md) describe the
result; this page says *what was decided, when and why*. Nothing here is
approved as finished: everything is built and in active development
(not approved yet). Open questions are in the roadmap (Part 1).

## 2026-10-20

- **Sheet templates: second round of fixes and the owner's additions**
  (an A1 export of the synthetic line with a title block of 35/55/45/45
  mm):
  - The **title block, revision table and legend lie inside the frame's
    padding** (they sat on the frame's outer edge and crossed its lines;
    right-aligned values were cut). The built-in full-width title block
    moves in by the padding too (5 mm; the profile panel 5 mm lower). A
    text too wide or tall for its cell is made smaller (to 40 %), a
    revision cell's (to 60 %) then shortened with "…"; a note is broken
    at the padding, and one lying over the title block, revisions,
    legend, key map or scale bars is warned.
  - **No empty field leaves a gap**: `{height_datum}` comes from the
    coordinate system (the vertical part of `EPSG:h+v`, a proj4 geoid);
    otherwise every field that comes out empty is written as the
    template's `[if_empty]` says, built in "unknown datum"
    (`height_datum`), "unknown coordinate system" (`crs`), "–" (any
    other); the station range's `ends` and the title lines stay empty
    unless named. The project has no vertical datum of its own (a core
    change, not made).
  - **Templates carry the words of every generated text**: the legend's
    entries (`[legend.labels]` by entry kind, with placeholders) and the
    panel titles (`[layout] plan_title`, `profile_title`); English stays
    the default.
  - **Structures band row** `format` (`{name}`, `{type}`, `{type_id}`,
    `{extension}`, `{station}`, `{height}`, two lines at a line break).
  - **The label of the line below the lowest conductor** is placed clear
    of the structures, the wires and the ground (`[conductor_clearance]
    label_at = "auto"`, default; `"middle"` the old place; `"start"`,
    `"end"`).
  - **LiDAR points on the sheets** (owner): the classes chosen (template
    `[lidar] classes`, the export window, `--sheet-lidar-classes`) within
    the corridor (else the line width), as dots in the profile (by
    station and height) and optionally the plan, styled per class or
    group, named in the legend by the class scheme; one dot per class
    and 0.2 mm paper cell (drawing only; no number changes). The app
    draws the points it has loaded (its point budget), the command line
    reads `--lidar` / `--pointcloud`.
  - **The corridor's sides in the profile** (owner): the ground along
    both edges, the **near side the right** looking along increasing
    stations by default (the plan's lower edge, nearest a reader of the
    profile under the plan; `[corridor] near_side`), each edge in its own
    pen (`near`, `far`) in the plan, the key map and the profile alike,
    both in the legend. `--side-offset` stays as it was.
  - **The centreline** (owner): the ground on the axis always solid
    (`[centreline] ground = false` only leaves it out); the plan's axis in
    `[centreline] pen` (the built-in look kept: grey dash-dot; the
    example: solid black).
- **Lines on the ground beside the line** (owner request: "an option in
  View to draw a line on each side of the lines, on the ground, at the
  displayed weather case and a given distance away from the line"): a
  display option, View › Lines on the ground beside the line, this
  user's view setting (`ViewPrefs.ground_lines`, never in the project,
  not logged, never in a result). The distance is measured by default
  **from the outermost conductor** of the weather case shown, in plan,
  with its wind blowout (both wind sides when both are shown); each side
  from its own outermost conductor; the alternative is **from the
  axis** (the alignment). Geometry: per span, across its chord every 2 m,
  the farthest point within the distance of that span's wires (and the
  jumpers at its ends); round outer corners about the structure in 5°
  steps; the inner loop cut off where the two spans' lines cross (the
  outer envelope, no self-intersection). Draped on the active terrain
  surface (or the LiDAR ground), a point at every triangle edge crossed;
  without a ground surface on the alignment's heights every 2 m. 3D and
  plan; the profile as two side profiles (right near, left far) when
  ticked. Kept for the last eight weather cases shown.
- **Decided here** for the lines on the ground (the most conservative
  choice, open to the owner): default distance **10 m** (a display
  value, no rule behind it); with several weather cases shown, **the
  case chosen as shown** only (not the envelope of all of them); spans
  to structures **away from the alignment** and **other project lines**
  are not taken into account; the lines **end at the first and last
  structure** (no round end caps); where the line leaves the ground
  surface it is **broken**, not continued on the alignment's heights.

- **Textbook and published validation cases** (the owner: "start with
  the textbook cases to improve the test suite"): 31 textbook cases
  (`benchmarks/*/textbook-*.toml`) and 16 worked examples of CIGRE TB 324
  (`benchmarks/sagtension/cigre-tb324.toml`), 135 cases in all. Each
  textbook case has a hand derivation in its comments and expected values
  from a new independent reference, `benchmarks/reference/textbook_reference.py`
  (standard library; the cable as the quadrature of its tangent, Newton
  with the quadrature's Jacobian — none of the code's formulas). The TB 324
  cases take the brochure's printed numbers with their printed rounding as
  tolerance; examples with incomplete inputs (Table 3, Table 4), graphical
  or curve-based ones (sec. 6.2, 6.3, Tables 5, 6, 12, 14, 15) are left out
  with the reason ([Textbook and published cases](../validation/textbook.md)).
  Two validation calculations added (runner adapters, no engine change):
  `line.section` (a line's section by its solver, optionally clipped in as
  the app does) and `cables.properties`. No computed number changes. Found
  and now documented as a **Simplification** in
  [Finite-element wire systems](../theory/wirefe.md#elastic-catenary-element):
  the FE's thermal lengthening of a span's unstressed length also scales
  its weight (+0.17 % on H at 150 °C, −0.04 % at −20 °C for the textbook
  span); left for the owner to decide.
- **Sheet export: the defects fixed** (the review of the sheet export,
  D1–D17): the profile panel takes the whole height; its height range
  fits the ground, the clearance line and the wires' lowest points; tops
  that do not fit are cut (a break mark), and when not even those fit the
  **datum is stepped** at a structure — the vertical scale is never
  changed, the ground always seen, each warned; height labels inside the
  panel; panels and content end at the match lines; a structure at a
  sheet's end drawn whole on both sheets; elevations drawn after the match
  lines, the plan panel grown to hold them; heading "Route id — name";
  the app draws each other weather case from its own solution and takes
  the markers, side profiles, plan scale, logos and the ground rule's
  clearance; PDF logos cropped (no hairlines) and only what is seen
  written; DXF texts in R12's own codes and logos left out warned. The
  sheets' look changes accordingly (height ranges, panel heights); no
  computed number changes.
- **Sheet templates — the owner's answers** (all 14 as recommended):
  extension **`.ohlsheet`** (format `ohl-sheet-template`, v1); templates
  **in the project and in a library folder** (the project's with their
  images embedded); plugin presets **through the WIT and as manifest
  files**; an **own revision list** per project line with a prefill from
  the last version commit; **DXF R12** with SVG logos as outlines, raster
  logos left out with a warning (DXF 2000 later if needed); a **north
  arrow per straight stretch**; a font **embedded only on request**
  (Helvetica stays the default — embedding itself not built yet);
  templates **carry their own texts** (no translation tables); a profile
  taller than the sheet **steps the datum** and warns; **nothing beyond
  the match line** by default, faded as an option; **portrait, long
  formats and a binding edge** yes (the built-in keeps 5 mm margins, the
  example 20 mm); **templates define the band rows** (the export may
  switch them off); **scale bars** in the example template, not in the
  built-in; an **empty required field refuses the export**, naming the
  fields.
- **Sheet templates — the owner's additions** (all template or export
  options, default off or neutral; generic, no third party named):
  (1) **several versions of the line on one sheet**, each in a **role**
  (existing, project, to be removed, decommissioned, other …) styled by
  the template, in plan, profile and elevations, with legend entries;
  chosen in the export window (project lines) and `--version LINE=ROLE`;
  (2) the **corridor** at ± W from the axis in the plan and the key map,
  labelled, the plan at least that wide; (3) **styles by line kind**
  (high / low voltage, telecom, earth wire) combined with the role,
  overhead solid and cable dashed, other lines grey with their label; a
  **legend** made from what each sheet draws, a graphic **scale bar**,
  **north arrows**; every item settable from a template (the keys
  documented), the built-in default neutral; (4) a **key map** of the
  route with the sheet's extent marked; (5) the **structures' elevations
  above the line, turned 90°**, a first-class template option with their
  own scale, offset, rotation and labels (number, type, height, the
  suspension strings' lengths); (6) **lines below the lowest conductor**
  per circuit or voltage, lowered by the ground clearance of the
  project's rules (by voltage; the rule's weather cases, else the largest
  sag drawn, said in the legend; a design code's ground preset fills them
  in through the existing rule machinery; no new engine calculation), on
  the sheets and in the sheet preview; options on/off, circuits, style,
  label, as well as or instead of the line above the ground.
- **Decided here** (not covered by the requests; the most conservative
  choice, open to the owner): where several ground rules check a circuit
  the line below its lowest conductor uses the **largest** distance (a
  **Conservative:** label in the theory page); a rule naming weather
  cases that are not drawn gives **no line** and a warning (rather than
  another case); low voltage has **no default limit** (without one every
  system with a voltage counts as high voltage); the plugin interface
  stays at **SDK 0.3** (the export is additive, as `rule-presets`: plugins
  built before load unchanged — the plan's 0.4 would have refused them
  under the 0.x rule); the listing is the command **`ohl
  sheet-templates`** (not `ohl drawing sheet-templates`, which clashes
  with its file argument); the corridor option is **`--corridor-width`**
  (`--corridor` is the LiDAR corridor); the band row `ground_clearance`
  stays empty until check results are given to it; overlap warnings of
  elevations count the line's own only (another version's elevation at
  the same structure overlaps by design); a version's wires and
  structures are those **within the line width** of the route.
- **Format and interface changes**: new file format `.ohlsheet`; new
  project library kind `sheet-templates`; the drawing settings
  (`app.drawings.<line>`) gain `sheet`, `side_offsets`, `plan_scale`,
  `logos`, `clearance_rule`, `markers` and `page = "template"` (older
  settings read with the defaults); new optional WIT export
  `sheet-templates` and manifest `[[sheet-template]]`; new DXF layers
  (`OHL_TITLE_FIELDS`, `OHL_TITLE_IMAGE`, `OHL_REVISIONS`, `OHL_BANDS`,
  `OHL_LEGEND`, `OHL_KEY_MAP`, `OHL_CORRIDOR`, `OHL_VERSIONS`,
  `OHL_CONDUCTOR_CLEARANCE`); DXF text codes `%%nnn` instead of `\U+`.

- **The vegetation and danger-tree panel** (owner's requests): **wind
  steps** for the danger trees — each windy case also solved in still air
  and at every pressure up to its own, by a step, from the sides chosen
  (the clearance windows' `WindSteps`), alongside or instead of the swing
  arc (new position kinds, added beside the others); the **required
  clearance and the allowance A as requirements by voltage** (one
  distance, a table, base + per kV; each wire with its system's voltage;
  single values work as before; the command line takes the rule file's
  forms); **every** grow-in and fall-in point kept and drawn, in two
  colours of the user's (kept with the view settings), one batch in 3D and
  one mark a pixel in plan and profile; **no line to the worst point**;
  the window a **dockable panel**; its **layers shown only while the panel
  is open** (closing it takes them off, reopening it or a saved run draws
  them again; docking or moving keeps them); runs **saved in the
  performed checks** (`ohl.vegetation`: settings, cases, per-span
  results, worst points, every point packed) and shown out of date when
  the line, the points or the ground changed since; every setting
  **explained** beside it with small drawings, grouped (wire positions,
  grow-in, fall-in, search) with links to the user guide; the **classes**
  chosen in a tick menu (default: by role); **design-code presets** of
  the check — a rule preset carrying a `[vegetation]` table (no change to
  the plugin interface; plugins built before are unaffected) — with its
  weather cases, those the project lacks created as one undoable change;
  **preset dropdowns** listing every enabled design-code plugin's presets
  grouped by plugin, in the rule editor, the clearance windows and the
  vegetation panel. Decided here (the requests left it open): a preset's
  weather case is matched **by name** (one of the same name with other
  values is kept as the project's, and the preview says so); a saved run
  tells the points changed when a point edit was logged after it (an undo
  counts: **conservative**); isoclearance lines are not saved with a run;
  plan and profile draw one mark a pixel (display only). ([user
  guide](../user-guide/analyses.md#the-vegetation-panel),
  [theory](../theory/lidar-analysis.md#wire-positions-checked),
  [plugins](plugins.md#vegetation-presets),
  [data model](data-model.md#performed-checks-ohl_sessionreports))
- **No tool in two menus** (owner): OHL-Design's tools live in the
  Tools menu and the inventory/hotbar, by theme (General, Alignments,
  Structures, Wires); the Structures menu no longer has Add structure…,
  New pylon from a template… nor Place on alignment points…. It keeps
  what is not a tool: the libraries, the tables, the display options,
  Spot structures (a calculation proposing a new line) and Angles around
  structures (a project setting). **Place on alignment points is Add
  structure on an alignment point**: the structure window, new on an
  alignment point, has **Also on other points of this alignment** (the
  former command's table: ticks, All/None, a row's own type), and the
  base on the ground or at the point's own height there; the structures
  ticked are added with it in one command (one undo), each tied to its
  point and turned along the line. Choices confirmed by the owner
  (2026-10-20): the other points take the window's extension
  and height adjustment with its type, a row's own type its base (the old
  command gave every one the base); nothing is ticked at first (the old
  command ticked every free point); the other points are always turned
  along the line; their routes and description are not copied from the
  window. **New pylon from a template…** is a
  command in the Tools menu's Structures group (and on the quick bar,
  key `new-pylon`), not a tool: it opens a window that builds a library
  type, with no click in a view. No saved quick bar or shortcut named
  the removed entries (they were never quick-bar commands); the tool's
  saved key `tool-add_structure` is unchanged. New pylon gets no
  keyboard shortcut (owner).
- **Danger-tree defaults = the utility practice** (owner): root-ball
  fraction k = 0.05, clearance allowance A = 2 m, a ground point within
  5 m of a tree base, a 50 m search distance (the half width of the
  corridor read) — in `VegetationParams::default`, `ohl lidar
  vegetation` and Checks › Vegetation alike; a design-code plugin's
  preset may give others, and each can be set (k = 0, A = 0,
  `--max-base-ground-distance inf` for no limit). **This changes the
  results of `ohl lidar vegetation` for runs that relied on the
  defaults** (before: 0, 0, no limit, 60 m; the application proposed
  k = 0.1 with no limit). The validation cases state their own
  parameters (results unchanged); tests of the check as before set them
  explicitly. Reading taken (not stated by the owner): the "search
  distance" is the corridor half width (`--corridor`, the window's
  "search ± m"). ([theory](../theory/lidar-analysis.md#danger-trees-2026-10-20),
  [user guide](../user-guide/analyses.md))
- **Unbalanced ice as a weather case; a weather case per span in the
  section solver** (owner: "the extra ice should just be like another
  weather case"; core change approved): the section solver takes a
  weather case per span (`ohl_line::solve::SpanCases`,
  `solve_section_span_cases`; `ohl_wirefe::State::span_states`): a
  span's own case sets its load per metre (weight, ice, the wind on its
  iced diameter), its temperature, the forces on the bodies on it and its
  surface radius; the section's case the strings' wind
  (**Simplification**, labelled), the jumpers, the core branch and the
  rolling of a section in the blocks (it rolls in the section's case; the
  spans' own cases act on it clipped in, as point loads do). One case for
  all spans is solved bit for bit as before (tested); the ruling span
  refuses spans in different cases. Validation `spancases-001…003`
  against an independent closed-form evaluation. The danger trees'
  unbalanced ice is rebuilt on it: the case's ice on the span (the other
  spans in the case without ice), more ice as a thickness (new) or as a
  load — **per wire (subconductor), as a weather case's ice load; before,
  the whole bundle's** (the heavier reading for bundles) — each a derived
  weather case (`WeatherCase::without_ice`, `with_more_ice_thickness`,
  `with_more_ice_load`); a concentrated load stays a point load. The
  wind now acts on the iced span's iced diameter (the old
  **Simplification**, wind on the bare diameter, is gone), and bodies on
  the span ice too. Example line, S1's second span: rime ice with wind,
  the case's ice on the span, blown 7.3 m across its axis (3.7 m before),
  its lowest point 0.67 m higher; glaze ice without wind 3 cm lower (the
  marker ball's ice). ([theory](../theory/wirefe.md#a-weather-case-per-span-2026-10-20))
- **Results out of date** (owner's decision for point edits, extended to
  line edits so the rule is the same): a result computed on demand keeps
  the revision it was computed from — a content hash of the line (with
  its weather cases, wire library and library files), of the point cloud
  (a sum of point hashes, changed by each point changed) and of the ground
  surface — and its window shows "Out of date: … changed since this run"
  with Run again when it differs; layers drawn from it are dimmed.
  Results are never dropped and never run again by themselves. Decided
  here (the task left it open): **an undo back to the state a result was
  computed from clears the mark**, since the revision is a hash of content
  and a new run would give the same result. Conservative choices: a
  change of any library file marks every result using the line (not only
  those reading that file); the wind side shown is a view setting and is
  not tracked (a galloping result follows the case shown as solved).
  ([user guide](../user-guide/desktop-app.md#results-out-of-date))
- **Box and lasso selection of LiDAR points** (owner's request): Alt +
  drag (box) and Alt + Shift + drag (lasso) with the selecting button in
  the 3D view and the profile select the points drawn inside (the rule of
  a click); their class is changed together as one `change_points`
  command, with a class filter; coordinates only for a single point. A
  command takes at most 10⁶ points (more: refused with the reason); from
  100 points it is written packed (zstd + base64 of its columns, some 10
  bytes a point) — a change of the log's format, older lists still read.
  The point store keeps a block's points in runs while changing them
  (stored bytes unchanged). ([user guide](../user-guide/desktop-app.md#editing),
  [data model](data-model.md#commands-the-only-way-to-change-a-project))
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
