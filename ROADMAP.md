# Roadmap

*Step back, 2026-10-18; split by application 2026-10-21.* This page has
three parts: **Part A** for OHL-Design (the line), **Part B** for
OHL-Structure (the structures) and **Part C** for what both share (the
engine, the formats, plugins, the server and version control,
validation and docs, 1.0 and further ahead). Each part has where it
stands, the open points waiting on a decision, what is unfinished or
approximate, the proposed stages and the detailed record of what is
built. Item ids (O1…, U1…) are unchanged; an item that concerns both
applications sits where its work is, with a one-line pointer in the other
part.

**What the marks mean:** `[x]` = built and in active development, **not
yet approved by the owner** (nothing is approved yet); `[ ]` = not built.
An item the owner approves will be marked **✓ approved** with the date.
Everything is pre-alpha: results must be verified by a qualified engineer.

**How to comment:** write under the `> **Owner:**` line of any item (one
per item). Leave the rest as it is; it is rewritten from your comments.
Items that touch private data (the Swiss plugin, the existing lines) are in
`local-data/ROADMAP-private.md`, in the same form.

Guiding principles:
- **Quality first, then speed.** Speed work may only simplify what is
  drawn, never a computed number.
- **Headless engine first, GUI second.** Every calculation usable from the
  command line and covered by validation cases before it gets a user
  interface.
- **Plugins stay independent of the core**; limits like string ratings are
  data that plugins may check.
- **Multi-user from day one**: every edit is a command applied by a session.

**Contents**
- [Part A — OHL-Design (the line)](#part-a--ohl-design-the-line): O1, O3, O13; U3, U7, U8; Phases 1–5 for the line
- [Part B — OHL-Structure (the structures)](#part-b--ohl-structure-the-structures): O4–O8, O10, O12; U1, U2, U4–U6; structure analysis, structures in their own axes, structure–line interaction
- [Part C — Shared](#part-c--shared): O2, O9, O11; Phase 0, the engine, plugins, formats, server and version control, Phase 6, cross-cutting, further ahead

The phases below record what is built. Phases map to GitHub milestones;
each bullet becomes one or more issues labelled by module (`module:*`).

---

# Part A — OHL-Design (the line)

**Next up:** towards the beta release (Phase 5's deliverable): the
items still open in the record below (`[ ]`) and the open questions
of this part: O13 (the conductor jump, to be evaluated) and the
jumpers' swing band margin, left to the owner (Phase 4, swing arc);
O1, the tutorial, comes later by owner decision.

## Where it stands

OHL-Design (`ohl-design`) is built as a base through Phases 1 to 5
below, all in active development (not approved yet). The overall state,
the validation count and what was just finished in both applications
are in [Part C](#part-c--shared).

## Open points — waiting on a decision

### O1. Tutorial "Start from scratch"
A tutorial of about 10 short chapters on the starter template: route,
alignment, first basic structures, strings on the structures, the wire type
file and stringing, jumpers; then results, checks, OHL-Structure, saving.
Proposed to start once the starter template has landed.
> **Owner:** Later: the tutorial comes once most features are done, just
> before a release. Other work goes first.

### O3. Usability review — items not yet decided
From the review of 2026-10-18 (quick wins are being done now):
- **3D and Plan side by side** by default (today Plan is a tab behind 3D).
- **One name per object** across both apps and the docs (string /
  insulator / attachment point / structure point …), with a glossary.
  *Built (in active development, not approved yet): the terms agreed on
  2026-10-21 in the [Glossary](docs/user-guide/glossary.md) (Help ›
  Glossary in both apps, with drawings); the texts renamed step by step
  (decision log 2026-10-21). Attachment points linked to typed joints
  (attach, support), offsets as rigid links whose moments the analysis
  applies, exact matching with the
  nearest attach joint offered on a structure change, link / relink /
  unlink commands (`ohl structures points`, `ohl structures link`,
  session `link_point`, `unlink_point`, `relink_points`): built in the
  engine, in active development (not approved yet). In both
  applications: the Link tool, the rigid links drawn (their own layer,
  clickable), the Attachment points tables, joint kinds in
  OHL-Structure's Joints table, the offset moments in its results, a
  drag changing the offset or relinking on a joint, Replace structure's
  Attachment points list: built, in active development (not approved
  yet). **Circuits** as objects of the line (2026-10-21): id, name,
  optional system, description, their conductor paths as members, in a
  Circuits table (Sections menu) and from Properties (*Assign to
  circuit…* on a wire); they follow the physical connections — a
  conductor path in one circuit, one path per phase, no merge (two
  circuits become one only when connected, the jumper's window asking
  which name stays); files before read into one circuit per name, results
  unchanged; `--circuits` / `--circuit` by id or name: built, in active
  development (not approved yet).
  Owner decisions of 2026-10-21, built, in active development (not
  approved yet): the link follows edits (a linked point moved keeps its
  joint, its offset recomputed: drags, the Attachment points tables, the
  Edit tool's windows, `ohl project apply`; a joint moved takes its
  points along); only attach joints take a link (the Link tool rings and
  clicks only them in both applications and on the structure screen);
  `ohl structures derive` writes version 6 types, a joint of kind attach
  per point; a point cannot be put on a member — files that place one
  along a member are read with a joint inserted and the member split
  (continuous), OHL-Structure inserts a joint on a member, the FE share
  to the member ends is gone and a joint only bars meet, loaded across
  them, is refused with the fix.*
- **"String all sets…"**: one command to string every set along an
  alignment or from one structure to another (preview, undo).
- **Option "tools act on left-click"** (today a right-click uses a tool;
  left-click selects). Default unchanged. *Built (in active development,
  not approved yet): a tick in the Tools menu and Help › Controls; on, a
  left-click uses the tool held (Select held: it selects) and a
  right-click selects, in the 3D, plan and profile views; drags unchanged
  (mapping table in the user guide's Controls).*
- **A searchable coordinate-system picker** (EPSG list). *Built (in
  active development, not approved yet): search by code, name, area or
  unit; rows show code, name, unit and the area of use where the
  registry's definition gives it; only systems the projection library can
  compute (geocentric ones left out); in Define, Calculator, Reproject,
  new project and Import LAS/LAZ.*
> **Owner:** (2026-10-19)
> 1. 3D and Plan: leave as is, the layout is fine.
> 2. One name per object: yes, same object same name everywhere; unify.
> 3. "String all sets…": only if it is robust (how does it know which set
>    goes where?); otherwise not. Decided: not done (matching sets by
>    number does not work between suspension and tension structures).
> 4. Tools on left-click option: go ahead.
> 5. Searchable coordinate-system picker: yes.

### O13. Conductor jump when ice drops suddenly (ice shedding)
When ice falls off a span at once, the conductor jumps upwards and can
swing towards the wire above (phase to phase, phase to earth wire). To be
evaluated: the jump height and the clearances during the jump, per section
and weather case, shown with the other clearances. Options when it is
treated: a simplified method (from the sag difference between the iced and
the bare conductor, with a rebound factor from published data), or a
dynamic time-domain solve of the section with the ice removed (in one span
or several). Reference data to come from published sources, not from
memory.
> **Owner:** (2026-10-19) Add the evaluation of the conductor jump when the
> ice drops suddenly.

### O14. Situation plans, phase plans and plan sets exported at once
Two kinds of drawing beside the plan-and-profile sheets, and a way to
export a whole set of plans in one go, each kind with templates (built
in, and as plugin presets — a utility's own templates in its plugin).
Proposed (to discuss; nothing built yet):

1. **Situation plan** — the plan alone, at an overview scale (e.g.
   1:5000 to 1:25 000; the template's), the line drawn on a background:
   - the line's axis, its structures (numbered), the corridor, the other
     lines and project lines in their roles, crossings marked; legend,
     north arrow, scale bar, title block from the sheet template;
   - background: the project's orthophotos and DXF drawings (as under the
     sheets' plan, 2026-10-23) and, if wanted, a background map from a
     map service chosen in the template (which services, and whether the
     program may fetch from one, to decide);
   - laid out on sheets along the route (overlapping, a key of the
     sheets), or one sheet of the whole line fitted to a paper size (the
     paper optimisation of 2026-10-22).
2. **Phase plan** — which conductor (circuit and phase) hangs where on
   every structure along the line:
   - a diagram along the line: one column per structure (number, type),
     one row per wire position (arms, left and right; earth wires on
     top), each wire marked with its circuit and phase in the phase
     colours, transpositions shown where a phase changes position;
   - the structures' heads (the elevations of 2026-10-22, schematic) with
     their positions labelled, once per type or per structure;
   - taken from the line as it is (sections, circuits, phases, wire
     points): nothing typed twice.
3. **Plan sets, exported at once** — a set names the drawings to make
   (plan and profile, situation plan, phase plan, …), each with its
   template, scales, routes and options; one command in the app (File ›
   Export › Plan set…) and on the command line (`ohl drawing --set
   FILE`) writes them all, named by a pattern (e.g.
   `{project}-{kind}-{route}-{sheet}`), optionally as one PDF; a set can
   be kept in the project and given by a plugin with its templates.
4. **Plugin presets** — a plugin gives its situation-plan and phase-plan
   templates and a plan set (sheet templates already come from plugins,
   2026-10-20).

> **Owner:** (2026-10-23) Set up a roadmap for situation plans and phase
> plans; export them automatically too; templates in the plugin.
> **Owner:** (to answer) Scales of the situation plan; a background map
> from a service, and which; what the phase plan must show (the diagram,
> the heads, both); the plans of a set and their naming.

*In Part B:* O6 (clearances from the strings and conductors to the
structure body) and O7 (the line and its structures solved together).
*In Part C:* O9 (design-code loads, the broken-wire case of a section).

## Unfinished or approximate (known limitations)

- **U3. Profile sheets**: a sheet break can still fall at a structure that
  stands off the alignment. *Built 2026-10-19, in active development (not
  approved yet)*: sheets break only at structures of the run along the
  alignment, at least 2 mm on paper from what is drawn of every other
  structure (away ones included); the break moves back to the last free
  structure, or the sheet runs on to the next free one with a warning.
  Profile first, sheets on top (owner, 2026-10-19): the app's profile view
  stays continuous; sheet format, scales and breaks belong to the drawing
  exports. *Follow-up built 2026-10-19, in active development (not
  approved yet)*: the break clearance is an export setting (default 2 mm
  on paper, 0 to 20 mm: `--break-clearance`, the export window); a
  structure counts as on the line, and may end a sheet, when it stands
  within the line width (the drawings' plan half width, `--plan-width`,
  default ±40 m), breaks stay at structures only; a sheet preview (off by
  default) shows the export's sheets, match lines, numbers and warnings
  over the continuous profile and the plan, laid out by the export's own
  code with the export window's settings. *Follow-up 2, built
  2026-10-19, in active development (not approved yet)*: one line width
  (default ±40 m) for the drawings and the Profile page (which shows the
  ground edges, survey and LiDAR points and 3D drawings within it): a
  structure beside the line is drawn only within it, wires are cut at it;
  a sheet ends only at a structure on the line (of the run along the
  alignment; never at a connection beside it, however near) or at an
  alignment point without a structure, still clear of every other
  structure drawn and never in a span; the drawing export settings
  (formats, scales, sheet format, line width, break clearance, options,
  the preview on or off) are kept in the project per project line.
  *Follow-up 3, built 2026-10-20, in active development (not approved
  yet)*: the sheet export's defects fixed (the ground always on the
  sheet: the profile panel the whole height, tops cut or the **datum
  stepped** at a structure, warned, never another vertical scale; panels
  ending at the match lines; structures at a sheet's end whole; the app's
  other cases, markers, side profiles, plan scale, logos, ground rule);
  **sheet templates** (`.ohlsheet`: page and orientation, frame, panels,
  pens, band rows, title block grid with fields and logos, revisions,
  notes, legend, key map, scale bars, north arrows, corridor, elevations
  with labels, wires by kind and the line's **versions in roles**, **lines
  below the lowest conductor** from the ground rules), built in, in the
  project, in a library folder and as **plugin presets** (WIT export
  `sheet-templates` or manifest files); `ohl drawing --sheet-template
  --field --revision --version --role --corridor-width
  --conductor-clearance`, `ohl sheet-templates`; the export window with
  the template, its values, revisions, versions and a sheet preview.
  *Follow-up 4, built 2026-10-20, in active development (not approved
  yet)*: title block, revisions and legend inside the frame's padding,
  texts fitted to their cells; no empty field (`{height_datum}` from the
  CRS, `[if_empty]`); the legend's and panels' words from the template
  (`[legend.labels]`, `plan_title`, `profile_title`); a structures band
  row `format`; the conductor-clearance label clear of the structures
  (`label_at`); **LiDAR points** of chosen classes within the corridor
  as dots in the profile and the plan (`[lidar]`,
  `--sheet-lidar-classes`, the export window); the **ground along the
  corridor's near and far edges** in the profile, each edge in its pen
  in plan, key map and profile; the centreline's ground always solid, the
  plan's axis in its own pen (`[centreline]`).
  *Follow-up 5, built 2026-10-21, in active development (not approved
  yet)*: the project's **height datum** (a name and an optional EPSG
  vertical code, Terrain › Coordinate system › Define… or `ohl project
  apply`; line file 7, project schema 5) written by `{height_datum}`
  before the CRS's vertical part and `[if_empty]`.
  *Follow-up 6, built 2026-10-22, in active development (not approved
  yet)*: **paper size and orientation optimised** per route from the
  ticked standard sizes (ISO A and B, long A sizes, ANSI, ARCH): fewest
  warnings, then least paper, then fewest sheets; different sizes of one
  standard on a route and the same paper on every route as ticks;
  `--page optimised`, template `format = "optimised"`. The sheet preview
  draws one sheet in the background, after the settings settle, with
  route and sheet chosen in its window.
  Still open: font embedding, a title block editor in the app, an
  orthophoto under the key map, DXF 2000 images. Mixed sizes stay free
  (owner, 2026-10-22: the goal is that it fits). *Follow-up 7, built
  2026-10-22, in active development (not approved yet)*: the elevations
  as schematic **stick figures** on request, with a chain of heights
  (base → arms → top), each arm's height above the base and its vertical
  strings' lengths, placed clear of the figure and of each other. *Follow-up 8, built 2026-10-23, in active development (not approved
  yet)*: each circuit drawn in a weather case of its own (template
  `[cases]`, the export window per circuit), a `stringing` band row (each
  tension section's horizontal tension in kN in the stringing case),
  the strings drawn on the structures. Still to come (owner, 2026-10-23,
  later): a background map under the key map, chosen in the template.
  *Follow-up 9, built 2026-10-23, in active development (not approved
  yet)*: the project's **DXF drawings and orthophotos under the sheets'
  plan**, straightened along the line like the wires (drawings on
  `OHL_BACKGROUND_DRAWINGS`, the orthophoto resampled per sheet under a
  white veil on `OHL_BACKGROUND_PHOTO`; SVG, PDF, PNG; not in DXF R12),
  chosen by the sheet template (`[plan_background]`), the export
  window's ticks and `ohl drawing --plan-drawings --plan-orthophoto`.
- **U7. Structure screen in OHL-Design**: drag handles for points in the
  line's 3D view built (in active development, not approved yet): points
  and wire points of the structure being edited, held to an axis or plane
  of its axes, snapped, one command each; a point of the type becomes the
  structure's own. A dragged point takes along the wire points whose
  strings all hang from it (given by length and direction or by their own
  position: the strings keep their length and direction); a wire point
  also held from another point (two strings to one wire point) stays and its strings change length,
  the status bar says so. Joints are shown but not dragged: they are
  edited in OHL-Structure only (a placed structure has no joints of its
  own).
- **U8. Level 2 and 3 members**: now drawn (built, in active development,
  not approved yet) in the line's 3D view — as lines, and as solids of
  their library profile within the **solids distance**, its own setting
  in the Level of detail table (default 400 m, measured); a member
  clicked selects its structure and, in the open structure screen, the
  member — and on the plan and profile sheets on their own layer
  `OHL_MEMBERS` instead of the symbol. **Poles at levels 2 and 3** are
  drawn with their outline: in 3D a solid of their real outline by
  height (round or polygonal, tapered, solid or hollow) within the solids
  distance, a click inside it selecting the structure; on the sheets the
  silhouette in the profile and the outline at the ground line and the
  top in the plan, on `OHL_MEMBERS`. On request the plan sheets carry
  each structure's **elevation** beside the line (off, above or below;
  offset on paper; rotation, 90° by default) on the layer
  `OHL_ELEVATIONS`, overlaps warned about, stored with the drawing
  settings and shown by the sheet preview. Left: members with a
  `[[section]]` table of their own (no profile) stay lines (decided); no
  GPU instancing (the geometry is shared per type and moved on the CPU).
- **U10. Named TIN layers in the thermal limit** (2026-10-23, not
  started): clearance rules may name a terrain surface of their own
  (built, in active development, not approved yet: the clearance
  windows, the Clearances page, `ohl zones`, `ohl line --rules`); the
  thermal limit does not read them yet and skips such a rule (no
  ground).
- **U9. New project lines are full copies** (asked by the owner,
  2026-10-22; *step 1 built 2026-10-23, in active development (not
  approved yet)*: a line shown is solved per case displayed in the
  background from the worked-on line's solutions, only what differs
  solved again and drawn; step 2, the file, under way): starting a project line (Versions) writes
  every object of the line it starts from again under the new line's
  name (`Session::create_line`), saved check reports included, so a new
  branch takes time and room in proportion to the whole line, and is
  loaded and shown as a line of its own. To do: a project line keeps
  only what differs from the revision it starts from (objects changed,
  added or deleted), reading the rest from that revision; creating one
  then costs next to nothing, and opening and drawing it can share what
  is unchanged with the line it came from (solved sections, drawing
  data). Needs a project schema change (with reading of the old form)
  and care with merging, deleting the source line and the per-line
  settings.

## Proposed stages (to discuss)

1. **Finish and settle** — land the three running pieces; the tutorial
   (O1); the folder rename (O2); docs checked against the apps.
2. **Usability, second round** — the items of O3 you choose; screenshots
   for the docs generated by a test so they stay current.

## Detailed record

### Phase 1 — Calculation core (headless)
*Goal: trustworthy single-section sag-tension engine.*

- [x] `ohl-cables`: conductor/wire library — area, diameter, weight, RTS, modulus, thermal coefficients, bimetallic (core/outer) properties
- [x] Stress-strain models: linear elastic, polynomial (initial / final / creep curves, per CIGRE/Aluminum Association practice), bimetallic split, aluminium compression limit
- [x] Creep models: creep-curve method (10-year curve) — *time-based / equivalent-temperature variants still open*
- [x] `ohl-sagtension`: exact catenary, parabola approximation, inclined spans, sag/tension/length/slack
- [x] Change-of-state equation; initial / final-after-creep / final-after-load conditions
- [x] Ruling-span (equivalent span) method for tension sections, plus exact equal-H section solution
- [x] Weather cases (temperature, wind, ice, combined) and tension limit criteria (% RTS, catenary constant, H/w); ice as radial thickness + density or as a load per length (density → wind diameter; without a density glaze ice, 913 kg/m³ — never the bare diameter, 2026-10-19); case files (`.ohlwc`) with tags, cases chosen per run
- [x] `ohl-wireloads`: additional loads on wires — point loads (N, 3D vector) and distributed loads (N/m, full span or sub-interval), permanent or per load case
- [x] Segmented-catenary solution for spans with point loads / partial distributed loads *(vertical plane; transverse components → FE wire model, Phase 4)*
- [x] Stringing charts output (CSV; PDF later with drafting)
- [x] CLI: `ohl sagtension job.toml` (report in metric or US units, CSV chart)
- [x] Non-linear model after the evaluation of a public-domain sag-tension library (2026-10-22, built, in active development (not approved yet)): the permanent set signed (a core steeper than its final modulus: after load, final equals initial at the load-stretch case); both final states reported, the governing one per weather case (the longer wire); tension criteria on the non-linear model, each in a condition (initial, after creep, after load), the governing one by iteration; warnings for states beyond a curve's `max_strain` (only where the cable gives one, owner 2026-10-22); validated on synthetic and invented cables against independent re-implementations (`nlstate-001…005`, `nlcrit-001…005`) — the Drake-like cross-check on curves of unstated source removed (owner, 2026-10-22)
- [x] Sagging outputs (2026-10-22, built, in active development (not approved yet)): stopwatch, dynamometer, transit angle of sight, level sag and inclined-span correction, sag per kelvin, clipping offsets (clipped in at another temperature than sagged) — `ohl sagtension` `[sagging]`, the section window's Sagging panel; see [Sagging outputs](docs/user-guide/sagging.md)
- [x] ~~A sheave model in the blocks~~: **not wanted (owner, 2026-10-22)** — the in-the-blocks model keeps one horizontal tension through the section (the simplification stays, marked in the theory page)
- [x] Performance: single section re-solve < 10 ms *(~60 µs for 20 spans; loaded span change of state ~1 ms)*
- [x] Validation: CIGRE TB 324 examples, textbook cases, hand calcs (2026-10-20, built, in active development (not approved yet)): 47 cases — catenary and the parabola's error, inclined spans and uplift, Irvine's elastic catenary, change of state (temperature, ice, wind, 150 °C, creep as a temperature), ruling span against the exact section and the FE strings, ice and wind loads, a point load's slope jump, a bimetallic knee point, blowout, the swing of one string and the forces of two strings to one wire point — each with a hand derivation and `benchmarks/reference/textbook_reference.py`; TB 324 sec. 2 (Table 2, dead-end strings), 5.2 and 6.1 *(TB 324's non-linear examples wait for published curves; see [Textbook and published cases](docs/validation/textbook.md))*; 2026-10-21: the wire's own weight conserved in every calculation (a heated, crept or stretched span weighs what it weighs when strung; ice and wind per metre of the real conductor), 3 cases (`tbheat-001…003`: closed form, FE and ruling span at 150 °C), tb324-016 within the printed rounding; the weight from the data sheet's reference temperature (a wire type's optional `reference_temperature`, else 20 °C) taken at the stringing temperature, and a stringing temperature of 20 °C for new sections and jumpers (`tbwref-001…002`); the FE's ice and wind per metre of the stretched conductor, iterated with the solution (`tbice-001`; built, in active development (not approved yet)); strain strings with a resolved weight of 0 N keep the estimated weight, and the string catalogue no longer makes 0 N entries

**Deliverable:** `ohl` CLI + Rust library + Python bindings, reproducible sag-tension tables validated against published references.

### Phase 2 — Line model & terrain
*Goal: a real line described in data, with a ground profile.*

- [x] Line data model (`ohl-line`, `.ohl.json`): alignment, structures, attachment sets (insulator + wire points), wire sections, stringing, point loads (stations and offsets per route: see Routes below)
- [x] Point clouds in the project file (`ohl-pointcloud`, ADR-0007): import from LAS/LAZ, chunk × class blocks, point-source interface (database link as a plugin later)
- [x] Spatial chunk index (ADR-0008): fixed quadtree tile grid (`ohl_core::grid`), loose home tiles, index in the project file kept up to date by every command, area/tile queries (`ohl project tiles`, `ohl project query`), point blocks selected by area and counted per tile (basis for streaming and LOD)
- [x] Routes (DESIGN_GOALS §9): several alignments per line, structures shared between routes (Y structures, parallel routes rejoining), stationing, offsets and line angles per route
- [x] Coordinate systems (`ohl-crs`, pure Rust proj4rs + EPSG registry): points and whole lines (`ohl crs`), LV95 ↔ WGS84 matching swisstopo REFRAME to < 1 mm *(grid-based datum shifts via a PROJ plugin later)*
- [x] Searchable coordinate-system picker in the app (`ohl_crs::search`): by EPSG code, name, area of use or unit; code, name, unit and area per row; only the registry's systems the projection library computes (geocentric left out)
- [x] `ohl-terrain`: survey point import (CSV/XYZ, named columns incl. PNEZD), feature codes, breaklines, constrained-Delaunay TIN, ground heights, exact profiles along segments (route centreline and side profiles in the drawings)
- [x] Structure library v1 (`.ohls`, DESIGN_GOALS §10): one file per type, local attachment points, default insulator per set, extensions, placement (position, rotation, extension), derive types from an existing line
- [x] Insulator library basics (`.ohli`, DESIGN_GOALS §11): suspension, two-string and strain assemblies used by the solver (two strings to one wire point solved in lines as tension-only, slack strings reported)
- [x] Insulator assembly model (graph of nodes and elements): structure points (fixed), wire points (free), strings as bars between them — one string, two strings to one wire point, a string below two, strain (single and multiple), strings hanging from strings, compression allowed per string (flagged beyond its limit); sections sharing an assembly solved together, Dirichlet–Neumann by the stiffest section (see [theory](docs/theory/wirefe.md#strings-hanging-from-strings)) *(post insulators and yokes as presets still to add)*
- [x] Multi-span sections solved in 3D (FE with suspension strings, wind per span, point loads); `ohl line` with 3D HTML viewer and CSV exports — compared with an existing line exported from another program (930 spans, median 1.1 cm)
- [x] Insulator behaviour in the 3D solve: a string's swing, the forces of two strings to one wire point & slack detection, strain assembly geometry *(from the FE solve; closed-form checks as validation cases since 2026-10-20: `tbins-001…005`, `tbrs-002…003`, `tb324-011`)*
- [x] `ohl-clearance` v1: vertical ground clearance of every solved span per weather case, violations in `ohl line --terrain --clearance` and in the viewer
- [x] `ohl-clearance` v2 (DESIGN_GOALS §13): vertical **and** radial clearances; rule files (`.ohlr`) per target — ground, LiDAR classes/roles, other lines, other wires of the same line (same span and between spans)
- [x] Clearances to the conductor's **surface** (2026-10-19): every check (ground, points, wires, zones, vegetation, galloping, right of way, distance between conductors, the thermal clearance limit) takes the conductor's radius — iced in a case with ice — and the polyline's bound (the solver's `polyline_error`, sagitta on the catenary's smallest radius of curvature H/w) off the distance to the axis; the radial ground distance from 0.5 m samples lowered by its sampling bound; galloping ellipses as circumscribed polygons — each on the safe side, labelled **Conservative** (validation `clearance-surface-001`)
- [x] Clearance envelopes over several weather cases (lowest clearance and governing case per wire and rule), different cases per wire in wire-to-wire checks (`case_pairs`), rules limited to cases; sheets draw every case
- [x] Exclusion zones in clearance rules: near the wire ends (`exclude_near_ends`) and spans at given structures (`exclude_spans_at`, e.g. cable terminations)
- [x] Electrical systems (`systems`: AC/DC, voltage, frequency, phase angles or pole polarities) assigned once per conductor path (sections joined by jumpers); clearances by voltage — tables or formulas per rule (so per LiDAR class group), pair voltage higher / sum / actual difference between conductors; unknown voltages refused before checking
- [x] Bundled conductors: `bundle` subconductors on a circle with `bundle_spacing` (e.g. 4 × 400 mm) and optional rotation; insulator weight shared by the subconductors in the solve; clearances to the nearest subconductor
- [x] Electrical wire type data in the wire library (`[cable.electrical]`: AC resistance by temperature, DC resistance, geometric mean radius, emissivity, solar absorptivity; resistance units) — filled by a local importer from another program's files until an importer is decided
- [ ] Rule editor tool (per LiDAR class)
- [x] Plan & profile views per route (`ohl-drawing`, `ohl line --drawings DIR`): straightened plan + profile with ground, side profiles, clearance line, structures, wires by phase, violation markers — SVG and profile CSV — sheets on A0–A4/custom formats split at structures (only at a structure on the line or a free alignment point, the break clearance, `--break-clearance`, default 2 mm on paper, from every other structure drawn; with no free break the sheet runs on, warned; a sheet preview over the app's profile and plan; the line width `--plan-width` decides what is drawn along the line, one setting with the app's profile; the export settings kept in the project per project line), title block with logos, separate plan scale across the line *(DXF, PDF, PNG and XML since Phase 5, `ohl drawing`)*; since 2026-10-20 (built, in active development, not approved yet) sheet templates (`.ohlsheet`; built in, project, library folder, plugin presets) with band rows, legend, key map, scale bars, north arrows, corridor, revisions, fields, the line's versions in roles, wires by kind, lines below the lowest conductor, datum steps; LiDAR dots in the profile and plan, the ground along the corridor's near and far edges, the template's own legend words, `[if_empty]`
- [x] Display: phase colours per circuit/phase, configurable per project (by phase label) and per section, legend in the viewer (DESIGN_GOALS §12)
- [x] Jumpers connecting sections at dead-ends or taps (slack catenary between solved clamps, length or sag input, optional support string, weather loads, ground clearance; DESIGN_GOALS §12) *(clearance to the structure body still to come)*

**Deliverable:** load a survey + structure positions, compute wire positions and ground clearance violations from CLI.

### Phase 3 — Desktop application MVP
*Goal: engineers can build and inspect a line interactively.*

- [x] Application shell (`ohl-design`, egui + wgpu, ADR-0009): background loading and solving, dockable views sharing selection and weather case; edits go through `ohl-session` commands
- [x] `ohl-render` basics: wgpu lines (pixel widths), points, shaded surfaces with depth and anti-aliasing; origin-relative precision; orbit camera; headless images
- [x] `ohl-render`: chunks on the project grid streamed in and out by distance, render distance per layer, three levels of detail (terrain grids, LiDAR shares, structure simplification, thinned wires and bundles), frustum culling, a level-of-detail table per layer in the app
- [ ] `ohl-render`: GPU instancing of structures and hardware, occlusion culling, wires tessellated on the GPU from catenary parameters, structure mesh LODs from the library, labels
- [x] Plan view, profile view, 3D view with synchronised selection; properties of structures and spans in every weather case; layers; LiDAR points by class role and ground from LiDAR
- [x] Live update while editing (Move window: the shown case re-solves the touched sections as values change; one command on Apply)
- [x] Tools as a Minecraft-style hotbar (1–9, wheel when captured) with an inventory (E, drag and drop), remembered
- [x] No tool in two menus (2026-10-20, built, in active development): tools live in the Tools menu and the inventory, by theme; the Structures menu keeps libraries, tables, display options, Spot structures and Angles around structures. Structures › Add structure… and Place on alignment points… removed: the Add structure tool on an alignment point does both (its window's **Also on other points of this alignment**: the other points ticked, a row's own type, base on the ground or at the point's height, one command); New structure from a template… moved to the Tools menu's Structures group (a command, also on the quick bar)
- [x] 3D navigation: fly with W A S D, Space/Shift up/down, Ctrl faster; control schemes as data (Orbit; Minecraft creative with captured mouse, crosshair selection, Escape to free) — more schemes and editing later
- [x] Option "Tools act on left-click" (Tools menu, Help › Controls; remembered): a left-click uses the tool held, a right-click selects, in the 3D (both schemes), plan and profile views; drags unchanged; off by default
- [ ] Command palette, keyboard workflows, explainable results (click value → inputs & references)
- [x] Editing through the session in the app: move (drag or typed position) and delete structures (confirmation listing what goes with them), undo/redo, incremental re-solve of every case
- [x] Structures and strings in the app: Add structure (on the ground, type from the library), Add string, Edit (structure, string in the structure's axes, override of a type's string) and Delete (structures, strings, sections, jumpers) tools, one session command each *(spotting by hand on the profile still open; automatic spotting: Phase 5)*
- [x] Wire stringing: String wire tool (strings in order → section window: wire type, circuit, system, bundle), sections edited and strung again through new points; sagged to a tension or a sag (ruling span or chosen span); status in the blocks (rolling: equal tension in all spans in every case, default) or clipped in (unstressed lengths per span) *(tension criteria by weather case still open)*
- [x] Clearance violation display: Clearances tab (rule file, violations of the shown case re-checked after edits in the background, click to go there), red marks in 3D and plan *(reports: `ohl line --clearance-csv`)*
- [x] Clearance rules by voltage in the app (2026-10-20, built, in active development): every rule distance — the Clearances tab's rule editor (rules of the chosen file edited in place, comments kept, each change one undoable command), the Vertical / Radial clearances windows and their Add a rule — as one distance, a table "up to kV: distance" or a base plus an amount per kV, with a preview at the project's voltages; 2026-10-21 (built, in active development (not approved yet)): one shared editor everywhere (rule editor, clearance windows, vegetation panel, safety volumes) with a form selector "One distance | Table by voltage | Base + per kV", every value a framed field with its unit in place, table rows with × (remove) / + Add row / Sort by voltage, a live line of the distances at the line's voltages, read-only previews greyed "(preview — Apply to edit)"; Load preset… with the rule presets of the enabled design-code plugins (optional `rule-presets` export, interface 0.3 unchanged for older plugins), merged or replacing, one command; `ohl code rule-presets` *(the example codes give none; presets that need geometry the rules cannot express — e.g. a distance growing with the distance to the nearest structure — wait on the engine)*
- [ ] Weather cases › Merge duplicates… (proposed 2026-10-21): find the project's cases of the same physics (`WeatherCase::same_physics`) and merge them, every reference renamed in one undoable command — rule files (`cases`, `case_pairs`), load cases (`weather`), the line's creep-stretch condition cases, saved performed checks and the drawings' case choices *(not built: those references live in several stores — library files, the line, the performed checks — that no single command changes yet)*
- [x] Library editors (wires, insulators, structure types, weather cases): fields with units, known fields to add, checked as typed, comments kept; files browsed or created anywhere (system file dialog) and used for the model; the model reads its libraries again on save
- [x] Menu bar (File, Edit with Snap, View pages and layers, Lines, Tools, Checks on demand, Terrain, Structures and Sections with their libraries and tables, Criteria, Parameters, Plugins, Help) and a customizable quick bar; pages closable and reopened from View
- [x] Circuits table (2026-10-21, built, in active development (not approved yet)): circuits as objects of the line (`circuits`, line file version 7, the same unreleased raise; OHL XML `<circuit>`; a project's settings row `circuits`) — id, name, optional system, description, members (conductor paths, by their first section), phases, sections, notes; added, renamed (the sections follow), given a system, deleted (paths in none after, confirmed), paths taken out and put in, one command per Apply; *Assign to circuit…* in Properties for a wire's whole conductor path. Following the connections: a conductor path in two circuits, two paths of one phase not connected, a path of another system, a repeated name — refused (files written so are read, noted); no merge. Files before: one circuit per name on the sections (id the name), the same results
- [x] Table editing: structures, sections, strings, jumpers, electrical systems, alignment points, weather cases (written to the case file) — every value typed in, rows added and deleted (dependents confirmed), renames followed by what refers to them, section targets (tension or sag in its display case), per-section display weather case; one command per Apply, sorting, filter *(paste from spreadsheets still open)*
- [x] Alignment points drawn in 3D and plan, selected, edited, deleted and added (tool and table); the profile follows
- [x] LiDAR points (2026-10-20, built, in active development): every point drawn can be clicked and snapped to, and no other — the 3D view's nodes or chunks drawn in the last frame, the profile's dots, none in the plan (a click looked at the sample loaded at opening before); the Edit tool's **LiDAR point** window changes a point's coordinates and class (classes of the project's scheme by name), read-only for viewers; the `change_points` session command changes the stored cloud point by point (blocks, levels, bounds, counts), logged, undone bit for bit, through the server; clearances checked again, the ground from LiDAR built again; **box and lasso selection** (Alt + drag, Alt + Shift + drag in the 3D view and the profile) of the points drawn, their class changed together as one command (class filter, at most 10⁶ points a command, packed in the log; 10⁶ points selected, changed and undone in seconds)
- [x] Danger-tree panel (2026-10-20, built, in active development): wind steps beside or instead of the swing arc (CLI `--wind-step`, `--wind-sides`); required clearance and allowance A by voltage (CLI in the rule file's forms); the vegetation classes chosen (tick menu; CLI `--classes`); every grow-in and fall-in point drawn in two colours of the user's, no line to the worst point; a dockable panel whose layers show only while it is open; every setting explained with live drawings, grouped with links to the user guide; runs saved in Performed checks and reopened (out of date when the line, points or ground changed); design-code presets of the check (`[vegetation]` in a rule preset) with their weather cases (since 2026-10-21 matched to the project's by physics first, then by name: an equivalent case of the project used, only the others created); preset dropdowns grouped by plugin in the rule editor, the clearance windows and the panel *(since 2026-10-21 the cross-section view shows a cut across a span, see below; the check's points in it still to come)*
- [x] Results out of date (2026-10-20, built, in active development): every result computed on demand (vegetation, as-built wires, vertical and radial clearances, galloping, distance between conductors, structure loads, thermal clearance limits, costs, spotting, the Clearances tab) keeps the revision of the line, the LiDAR points and the ground it was computed from (content hashes: an undo back clears it); a banner with Run again when it changed, layers drawn from it dimmed; nothing dropped, nothing run again by itself *(a change of the wind side shown is a view setting: not tracked)*
- [x] Cross-section view (2026-10-21, owner request; since 2026-10-22 a 3D panel, owner request; built, in active development (not approved yet)): C (View › Cross-section…, a structure's menu, the profile's right-click) opens a window — a station on an alignment or a structure (searchable list or clicked in a view), depth ±10 m by default, width the line width —, then the cut as a 3D panel of its own (several at once): perpendicular to the alignment (the bisector at an angle point) or in a structure's cross-arm plane; the 3D view of the line clipped to the slab (renderer clip box, only the chunks and point nodes meeting it built, indexes shared with the 3D view): terrain, LiDAR points, wires and jumpers of the cases shown, structures, ground lines and every other layer; opens in the front view (orthographic, looking ahead, 1:1, axes, scale bar, station label), then orbit, pan, zoom and fly as the 3D view, ⌂ / Home back to the front view, orthographic / perspective; picking, tools, handles, layers and cases as in 3D; hover readouts in the plane; ◀ ▶ , . steps and ⏮ ⏭ structures carry the camera; the main 3D view outlines each slab, a click on it brings the panel forward *(not yet: the swing band, the clearance circle, a gizmo to drag the plane, export of the front view to SVG/PNG/DXF)*
- [x] Check windows (2026-10-21, owner requests; built, in active development (not approved yet)): **Run at the bottom left** of every check window and page (Vertical and Radial clearances, vegetation, safety volumes, galloping, as-built wires, distance between conductors, thermal, structure loads, bill of materials, spotting, the Clearances tab), in a footer that stays visible (the body scrolls above it), with Stop and the progress while it runs (`check_help::body` / `footer` / `run_button`); a check's marks in the views taken off when its window or page is closed, back when it is opened again (the vertical / radial windows and the Clearances tab, as the vegetation panel and the safety volumes); **a rule's voltage filter is optional** — with a table or base + per kV hidden behind "Only for some voltages…" (off), old files' filters kept and shown; a window row keeps all of a rule's voltages (`ZoneRule::voltages`, no longer one row per voltage); the rule editor shows the filter, offers **Merge rules by voltage…** (previewed, undoable) and **Remove all rules** (confirmed, undoable)
- [x] Lines on the ground beside the line (2026-10-20, built, in active development (not approved yet)): View › Lines on the ground beside the line — a line each side of each alignment, a distance (m, default 10) from the outermost conductor or from the axis; since 2026-10-21 (built, in active development (not approved yet)) the plan envelope over a set of weather cases ticked in the menu (All, None, the case shown — the default —, each case), both wind sides of a windy case, the jumpers counting as conductors so the lines go on round the structures, kept per set of cases; the outer envelope of the band within the distance, round outer corners, no self-intersection; draped on the active terrain surface, else the LiDAR ground, else the alignment's heights (since 2026-10-21, built, in active development (not approved yet): without a TIN the LiDAR ground classes of the whole corridor, not the points shown, built again when the active surface is deleted; the gaps without ground counted; the menu says the ground used and, with no ground under the lines and a point cloud, offers Build a terrain surface from the LiDAR…; since 2026-10-21, built, in active development (not approved yet): a LiDAR corridor of the lines' own, the model's 60 m by default, up to 200 m — a wider LiDAR ground built in the background for the lines only and kept per width, the checks' ground unchanged; the menu says when the lines go beyond the corridor and to what width to widen it); 3D, plan and optionally the profile (right near, left far); routes, colour, width, dashes chosen; this user's view setting, remembered per project on this computer; since 2026-10-22 (built, in active development (not approved yet)) round every span of the line — every section of every route and branch and the spans to structures away from the alignments (gantries, portals), each in its own direction — joined at the structures with no gap at a tap and closed round the ends (a round end beyond the wires' ends), the route ticks listing every route and branch, one envelope over the routes ticked or each route its own lines *(other project lines still open)*
- [x] Orthophotos: TIFF variants (2026-10-22, built, in active development (not approved yet)): Terrain › Backgrounds › Import orthophotos… reads JPEG-compressed YCbCr GeoTIFFs (GDAL's `COMPRESS=JPEG PHOTOMETRIC=YCBCR`, tiled or in strips) with the `tiff` crate, no new dependency; any TIFF variant neither reader takes is refused with a message naming it and the workaround
- [x] Project lines: frozen states of the line (as built, as planned) saved in the project, shown next to the working line in 3D and plan, made the working line (one undoable command); since 2026-10-20 drawn on the plan and profile sheets in their roles (built, in active development, not approved yet) *(on the app's Profile page still open)*
- [x] Add jumper tool (missing jumpers at a structure, or between two strings); ⌘S / Ctrl+S saves
- [x] Jumpers held by their clamps: bending stiffness (estimated from the wire type or set) and the angle each end leaves its clamp at; solved as an inextensible elastic rod; project lines drawn like the working line (bundles, yokes, strings, jumpers)
- [x] Jumpers held by support clamps between their ends: each clamp on one or two of a structure's strings, free to swing and turn, the jumper straight through it (a rigid piece of the rod, also with EI = 0); tension-only strings (a leg goes slack in the wind); edited in the jumper's window, shown in the tables, properties and 3D view, written to OHL XML; their strings' pull in the structure loads; a clamp may also hang on a string below another string (e.g. a V with a string below, `chained_top`)
- [x] Strain assemblies always free members (move with the weather, in the calculation); weights of geometry-only strings estimated from their data position in the stringing state
- [x] Libraries (wires, structure types, insulators, weather cases) kept in the project file; export to files to share or compare; Terrain / Structures / Sections menus; LiDAR classes shown; table defaults shown
- [x] Section calculations: Initial / Creep / Load / Max Sag × RS / FE (Sections table, section editor); Criteria › Creep-Stretch (weather cases for final after creep and after load); wire models nonlinear, linear with creep ∝ creep-case tension, linear with a temperature increase; bimetallic curves per material in the wire library editor
- [x] Projects from other programs: `project-plugin` WIT world (line + library files + notes), File › New project from another program, `ohl project import-from` with clip-in; example plugin `project-bundle`
- [x] Terrain surfaces (TIN) in the project: several, the active one is the ground, others shown in colour; import LandXML / OBJ / survey points and terrain plugins (`terrain-plugin` world, e.g. ESRI ASCII grids), build from LiDAR ground classes, export LandXML / OBJ; `ohl surface`
- [x] Plugin manager UI: plugins installed and found in folders, what they provide, why one cannot be used, on/off, install, remove; their formats in File › Import into the line / Export (run on a worker)
- [x] Python scripting console inside the application: embedded interpreter on its own thread, the `ohl` module reads the line and results and edits through session commands (undoable), Stop and a time limit, plugin scripts
- [x] Points on the wires and the objects on them (one object per point from an object library: spacers, spreads, …), jumpers from a point or a tap; spacers in the solve
- [x] View performance: a frame benchmark; what made a long line lag found and fixed

**Deliverable:** alpha release of the desktop app, including shared multi-user sessions.

### Phase 4 — Loading & structural checks
*Goal: design-code compliant loading.*

*Status: built as a base, in active development (not approved yet). Each item says what is simplified or left for later.*

- [x] `ohl-loads`: load case framework (weather × load factors × wire condition), `.ohllc` files kept in the project, edited in the app
- [x] Structure capacity Method 1: allowable spans (wind/weight span, line angle) — `[capacity.spans]` in the structure types, interpolated by line angle (linearly in sin(θ/2), on the safe side for a structure responding linearly — 2026-10-19), minimum weight span (uplift)
- [x] Method 2: allowable load trees per attachment (vertical, uplift, transverse, longitudinal; linear interaction optional) against factored loads × strength factor *(interaction diagrams of the whole structure later)*
- [x] Finite-element wire system (SAPS-style, `ohl-wirefe`): exact elastic catenary elements, single strings and two strings to one wire point (tension-only), 3D wind blowout, insulator swing, broken span, point loads, unequal spans *(delivered early, in Phase 1)*
- [x] FE wire system extensions: strain insulators (free strain members), jumpers (solved between the section ends), bundles, non-linear stress-strain and creep in elements (secant iteration on the curves, permanent set from the material state; FE and RS agree within 0.02 % in tension on a fixed-end span) *(linked assemblies: the insulator assembly model above)*
- [x] A weather case per span (2026-10-20, built, in active development (not approved yet)): a section solved with its spans in cases of their own (`ohl_line::solve::SpanCases`, `solve_section_span_cases`; `ohl_wirefe::State::span_states`) — load, temperature, bodies and radius per span; one case for all bit for bit as before; finite elements only (the ruling span refuses it); validation `spancases-001…003`. Used by the danger trees' unbalanced ice; to be used for unequal-ice checks and galloping's unequal ice
- [x] Structure loads output (attachment point loads per load case): line and structure axes, unfactored and factored; Structure loads page, CSV (`ohl line --load-cases --loads-csv`) — wind on the structures by height band (`[[wind.band]]`), the strings holding jumpers' support clamps *(the pull of jumpers on their end clamps still open)*
- [x] Usage / utilisation reports: Structure loads › Usage (worst first, governing case, colours), `ohl line --usage-csv`, structures coloured by usage in the plan and 3D views with a legend; a structure whose wires did not solve is **incomplete** (never a pass), one whose FE check kept tension-only members active (the last resort) is **approximate**
- [x] String limits: optional maximum load and swing angles on strings, kept for plugins (no core check)
- [x] Wire tensions (2026-10-22, built, in active development (not approved yet)): a rule-based check of every span's tension against a factor of its wire type's RTS per weather case (maximum at the most tensioned support, or horizontal), rules as `[[tension_rule]]` tables of a rule file (presets with `checks = ["tension"]`), Checks › Wire tensions… and `ohl tensions` (CSV) — no code's factor in the public code; and a warning whenever a span's tension exceeds the RTS, in every solver
- [x] Clearance presets from design-code plugins for the Vertical and Radial clearances windows
- [x] Swing arc in the Vertical and Radial clearances (2026-10-20, built, in active development (not approved yet)): none, the wind steps, the swing arc or both, with the sides (rule file `[wind] arc = true`, `ohl zones --swing-arc`), the danger-tree check's arc shared (closed-form radial and vertical distance to points, the ground against the wire swung every 2 cm, conservative; since 2026-10-21, built, in active development (not approved yet), a band from the smallest to the largest radius of the three positions, widened by 0.15 m — the measured worst excess of the 10 Pa steps, 0.089 m, plus an allowance — so it holds every intermediate position on the example lines, for the danger trees, these checks and the safety volumes together; validation `swingarc-004`, `arcclear-006`; since 2026-10-21, built, in active development (not approved yet): the band margin editable wherever the band is used — the windows, the vegetation panel, the safety volumes, `[wind] band_margin`, `--band-margin`, saved runs — validation `swingarc-005`; the jumpers a rule checks on a swing band of their own about their end clamps, validation `jumperband-001…002` — on the example line's jumper its 10 Pa steps lie up to 0.293 m outside it, more than the default margin, the owner to decide; a rule's voltages and wires now applied to the arcs); the windows explained as the vegetation panel (grouped settings, a line each with hover text, drawings, user-guide links, the solves a choice needs, the arc suggested for fine steps), a progress bar with the time left, Stop keeping the previous results; the point and ground checks made faster with the same numbers bit for bit (one measure, bounds before exact distances, the highest ground per cell, every core)

**Deliverable:** code-checked line design with structure loading reports.

### Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

*Status: built as a base, in active development (not approved yet). Each item says what is simplified or left for later.*

- [x] LiDAR basics (`ohl-lidar`, delivered early in Phase 2): LAS/LAZ tiles, class schemes with roles (`.ohlc`, any classification), ground in a corridor, TIN from LiDAR — matched a commercial TIN's span clearances to a median of 2 mm on a real line
- [x] LiDAR: COPC, octree streaming and point budget (billions of points at 60 fps) — *base*: COPC read directly as a point source (area/corridor/depth queries by octree node; also on `--pointcloud` for `ohl line` and the app), imported like LAZ, written on export (in memory), `ohl pointcloud info` shows its octree; stored clouds keep levels of detail per block (¼, 1/16, 1/64 of the points; schema 2, `ohl pointcloud octree` for older files, ADR-0007 amendment); the 3D view streams nodes on background threads by screen-space error under a per-user point budget (default 5 M) with an LRU GPU cache. Measured (M1 Max, synthetic 25 M points): 5 M points drawn at 16.5 ms a frame, 10 M at 32 ms; a view read in 0.1–0.2 s. Not measured with 10⁹ points (synthetic data only, extrapolated in the user guide); stored-cloud nodes are 256 m squares × 4 levels, not a spatial octree, so very large *areas* hit the budget at the coarsest level; COPC export holds the points in memory; COPC colours/GPS time not kept
- [x] LiDAR: classification, vegetation & encroachment analysis, as-built wire extraction/fitting — base: ground by progressive morphological filter + height classes (`ohl lidar classify`), vegetation encroachment with growth and fall-in per span, catenary fits per span with the survey temperature from the model (synthetic tests: sag within 2 cm, temperature within 2 °C); app windows. Still to come: buildings/wires/poles told apart from vegetation, leaning trees
- [x] Safety volumes for work near the line (2026-10-20, built, in active development (not approved yet)): the union over weather cases and wind positions (blowouts, steps, swing arc, both) of the space within a distance by voltage (no distance built in; a preset rule may fill it) plus a margin of the conductors, one closed mesh per wire or circuit that holds the exact volume whole (sparse marching tetrahedra, then collapses kept only outward, checked as a whole); Checks › Safety volumes (3D translucent, plan outline, Performed checks, out of date), `ohl safety-volume`; DXF R12 + `.prj`, IFC4 with `IfcMapConversion`, KML/KMZ (WGS84, heights kept); validation `safetyvolume-001…005`. Since 2026-10-21 (built, in active development (not approved yet)): the swing arc a band from the smallest to the largest radius (widened by its measured margin), each volume an `IfcSpatialZone` in IFC, and the jumpers included at their solved positions (continuous at the structures). Not included yet: structures Also since 2026-10-21: wires chosen by system, circuit or span (searchable tick lists with All / None / Invert on the rows found, a shared widget; CLI `--span`), one volume per wire, circuit or system (CLI `--system`, `--per system`), the window's settings grouped and explained with drawings and user-guide links; the wires without a system (earthed wires) one group "(no system)" to tick and one volume per system (CLI `--system none`); the band margin editable (`--band-margin`); with the swing arc each jumper on a band of its own about its end clamps (built, in active development (not approved yet))
- [x] Danger trees (2026-10-20, built, in active development (not approved yet)): fall-in about the root ball (hinge at k·h', resting on the ground along the fall direction, sheltered by ridges; tree base only with a ground point within a given distance), grow-in with buffer against every case solved left and right, the optional swing arc per span and unbalanced ice per span near vegetation; per span the worst with case and position; isoclearance lines (GeoJSON/DXF/CSV, drawings layer `OHL_ISOCLEARANCE`, app layers in 3D and plan); parallel and bounded (`ohl lidar vegetation --root-ball --fall-allowance --buffer --swing-arc --band-margin --unbalanced-ice --isoclearance --max-base-ground-distance`; Checks › Vegetation; validation `dangertree-001…005`, `swingarc-001…004`). Since 2026-10-20 the unbalanced ice is a weather case of its own on the span (`--unbalanced-ice case|thickness=m|extra=N/m|load=N`; the wind on its iced diameter) and the defaults are the utility practice (root ball 0.05, allowance 2 m, base ground point within 5 m, search distance 50 m; the command line's results with the defaults change). Since 2026-10-21 a cross-section view of the line (built, in active development (not approved yet)); the check's grow-in and fall-in points in it still to come
- [x] Thermal rating: IEEE 738, CIGRE TB 601 / TB 207, max operating temperature from clearance — `ohl-thermal`: ampacity, temperature, transient, solar position; `.ohlta` ambient cases; per-section clearance-limited temperature (ground and point rules); `ohl thermal`, Checks › Thermal rating *(base: uniform conductor temperature, no radial gradient; no published TB 601 example in the validation yet)*
- [x] Optimum (automatic) structure spotting — cost-based optimisation: dynamic programming over candidate stations × types/extensions (ground clearance of a design catenary, allowable wind/weight spans, alignment points, forbidden zones); `ohl spot`, Tools › Spot structures (proposal as a new line file) *(clearance under the centreline only, one tension for the route)*
- [x] Galloping ellipses, blowout, phase-to-phase / circuit-to-circuit clearances — base: ellipses by user coefficients (`.ohlg`; Davison / RUS / CIGRE TB 322 forms, no coefficients shipped), swept along the span, per span pair (`ohl clearance galloping`, app window); blown-out wires against the right of way (`ohl clearance row`); `circuits = same|different` on wire rules with case pairs
- [x] Drafting: plan & profile sheets, title blocks, DXF/DWG-compatible output, PDF (`ohl drawing`, File › Export drawings): one format-free drawing model written as SVG, R12 DXF (sheets in mm, plan in project coordinates, profile by station × exaggerated height), vector PDF (a page per sheet) and PNG *(base: no DWG written — the R12 DXF imports into DWG-based programs; DXF has ACI colours, no line weights, SVG logos as outlines, no raster logos; PDF uses the standard Helvetica fonts)*; since 2026-10-20 the title block, page and styles from sheet templates (`.ohlsheet`, plugin presets), built, in active development (not approved yet)
- [x] Profile exports: ground/wire/clearance profiles with configurable scales and station ranges — DXF, PDF, SVG, CSV/XML, images (DESIGN_GOALS §8) — `--from`/`--to`, `--hscale 1:2000 --vscale 1:200`, `--profile-only`; profile XML documented in the user guide *(base: one weather case solid and the others dashed, as in the drawings)*
- [x] Bill of materials & cost estimation: structures by type/extension, insulator assemblies, cut lengths per wire type (bundles, jumpers), spacers/dampers by rule, priced with a `.ohlcost` file (missing prices listed); `ohl bom` (text, CSV, JSON), Checks › Bill of materials

**Deliverable:** beta release usable on real projects (with independent verification).

### Structures in the line
*From [Structures in their own axes, three detail levels](#structures-in-their-own-axes-three-detail-levels) (Part B): what OHL-Design does with the structures.*

- [x] Structure type workspace in the app: its own 3D view in local axes, tables (joints, members, points, strings, wire points), select / edit / drag with snaps, add members, string presets; used-by count; the line follows on save *(base: presets for one string and two strings to one wire point — two tension strings from a string and a string below two by hand; ids renamed in Fields; drag handles in the line's 3D view built since 2026-10-19 (U7): points and wire points; joints shown only. Since 2026-10-17 OHL-Design's structure screen edits only the basics — points, strings, wire points — and the body and design are edited in OHL-Structure)*
- [x] **Replace structure** tool: a new structure put in place of another, with its strings; wire points (sets) that match are rewired automatically, for the others the user says which goes where; with no match the replacement still runs after the model shows what will be removed and the user confirms
- [x] Structure picker window: table of structures (name, description, level, extensions, uses) with a filter and a preview, instead of the drop-downs
- [x] **Members of levels 2 and 3 drawn with the line** (U8): in the line's 3D view as placed by the line (position, orientation, extension, height adjustment), as lines and within the solids distance (its own level-of-detail setting, default 400 m) as solids of their library profile turned as the FE model turns them (`ohl_structures_fe::drawn`, the geometry made once per type and extension); poles at levels 2 and 3 as solids of their outline by height; a click on a member selects its structure and the member in the structure screen, a click on a pole its structure; on the plan and profile sheets (SVG, DXF, PDF, PNG) on the layer `OHL_MEMBERS` instead of the symbol (poles: the silhouette and the outlines at the ground and the top), level 1 unchanged; structures' elevations beside the line on the plan on request (`OHL_ELEVATIONS`) *(members of `[[section]]` tables stay lines; not yet: GPU instancing)*
- [x] Weather and load cases at the suspension points from level 1, exported for a structure designed in another program — the load cases and weather cases the line sends to placed structures, as the load table (CSV)

---

# Part B — OHL-Structure (the structures)

**Earlier stage — not for use yet.** OHL-Structure is at an earlier
stage than OHL-Design: built as a base, in active development (not
approved yet), with the open points and limitations below.

**Next up:** O4, code checks of members (SIA 263 for steel first,
members before joints): waits for the owner's agreement on the plugin
interface and for the standard's values from the owner (none typed from
memory).

## Where it stands

### Just finished (2026-10-18)
- **Spring reactions**: a sprung joint now reports the spring's force
  (models without springs unchanged; two new validation cases, 69 in all).

## Open points — waiting on a decision

### O4. Code checks of members and joints (e.g. SIA 263)
A plugin interface for code checks: what the core hands over (member
forces, buckling lengths, sections, joint details), what comes back
(usage per member and joint, formulas). To be agreed before it is built.
> **Owner:** (2026-10-19) Can be done in parallel with other work. First
> code: SIA 263 for steel. Members first, then joints. The theory of the
> checks goes in a theory page of the book.

### O5. Tension-only parts that stay nearly free
Today: kept active and flagged *approximate* (see U1). Options when this is
treated: a very low compression stiffness instead of tension-only (your
idea), a damped or energy-based active-set update, and telling the user
which part is nearly free.
> **Owner:** (2026-10-19) What matters: it always solves, close to
> reality. In reality such a member takes a very light compression. Also
> consider treating tension-only members as cables. The complex
> active-set update can go on the list for later.
> Decided: a tension-only member carries compression up to its own buckling
> load (always solves; members at that load shown in the results). Not as
> sagging cables. The complex active-set update: later.

### O6. Clearances to the structure body (levels 2 and 3)
None at level 1. The tests for levels 2 and 3 are still to be defined with
you (what is checked, against which part of the body, in which cases).
> **Owner:** (2026-10-19)
> 1. What is checked: the conductors, and **basic volumes on the strings**:
>    spheres, cylinders, boxes and cones, each **grounded**, **insulated**
>    or **electrified**; distances are computed from these as well as from
>    the conductors. Each shape is placed along the string's length, with
>    its rotations (degrees of freedom) **free or blocked** to mimic real
>    strings; the freedoms are along the string's own axis, so they do not
>    depend on how the string is defined in the pylon. For double or triple
>    strings: the attachment point of each, the configuration and the
>    spacing.
> 2. Against every member of the body.
> 3. Cases as for the other clearances: weather cases chosen, wind stepped.
> 4. Distances rule based; a rule may give a different distance with wind
>    than in still air.

**Proposed details for O6 (to confirm):**
- **The shapes belong to the string type** (the insulator library), so
  every string of that type has them; a structure may override one.
- **Position along the string** from its top (fraction or metres), size,
  and an offset from the string's axis (an arcing horn beside it).
- **Electrical state:** *grounded* = at the structure's potential (top
  fittings, tower-side horn); *electrified* = at the line's potential
  (clamp, conductor-side horn, corona ring, yoke at the bottom);
  *insulated* = the insulator itself (its length counts as insulation, not
  as a clearance). Checked: electrified shapes and conductors against
  grounded shapes and every body member.
- **Freedoms** in the string's own axes: swing across the line, swing
  along it, twist about the axis; each free or blocked.
- **Several strings in one assembly** (double, triple): number,
  arrangement (side by side across the line, one behind the other along
  it, triangle), spacing, and the yoke plate as a shape.
- **The members' real size:** distance to the member's surface (from its
  section's outer dimensions), not to its centre line.
- **Jumpers** at tension structures checked against the body too.
- **Results:** the smallest distance per rule and case, which shape and
  which member, shown in the 3D view; levels 2 and 3 only (level 1 has no
  body).
> Details confirmed by the owner (2026-10-19).

### O7. Structure–line interaction, levels 3 and 4
Level 3: a stiffness matrix at each structure's suspension points from
level 3 structures. Level 4: structures and line iterated until both
converge. Order and timing to decide.
> **Owner:** (2026-10-19) Straight to level 4: level 3 is a simplified
> version of 4 and can easily be derived from it afterwards.
> Also needed (possibly with O7): **anchoring (guy) cables** in the
> structures.

### O8. Exports tried in other programs
IFC4, IFC2x3 and SAF are written from the public specifications and
checked with free tools (2026-10-19: IfcOpenShell's schema and rule
validation, geometry and content checks; the SAF workbook read back with
its references; `tools/check_structure_exports.py`, one rule fixed in
IFC2x3). They have not been imported into the target programs yet: the
sample files in `examples/exports/` are for someone with those programs
(what to look at: its README).
> **Owner:** (2026-10-19) Both: check the files now with free open-source
> tools (IfcOpenShell for IFC, the SAF schema), and have a test file opened
> in the real programs when someone can.

### O10. A pole with both `solid` and a library section
Today *solid* wins (the section still gives elasticity and density).
Alternative: refuse the combination.
> **Owner:** (2026-10-19) Poles need their own user-defined volumes: some
> shapes fall outside the standard library.

**Proposed for O10 (to confirm):** a pole's shape is either a library
section or its **own shape**, never both (the combination is refused with a
clear message). Its own shape is given along the height in segments, each
with a cross-section (circle, polygon with n sides, rectangle, or a drawn
outline), solid or hollow (wall thickness), tapering from bottom to top;
area, inertia, section modulus, weight and wind area are computed from that
geometry, and the material (steel, concrete, wood) is chosen per pole.
> Confirmed by the owner (2026-10-19).

### O12. Bowed tension-only members: usage and buckling length
Since U1 a tension-only member carries compression up to its own buckling
load. Its usage is checked against its compression capacity *without* the
buckling reduction (the solve already caps its force at N_cr; otherwise
every bowed member would show 100 % or more). Its buckling length is its
own length between joints (k = 1) unless `lk` or `k` is given, so a
diagonal drawn in halves buckles at half length.
> **Owner:** (2026-10-19) Usage: OK for now, to be discussed again later.
> Buckling length: depends on the joint type; OK for now.

*In Part A:* O3 item 2 (one name per object; attachment points linked
to joints, the Link tool and rigid links, in both applications). *In
Part C:* O9 (code-specific wind on structures, in the design-code
plugins).

## Unfinished or approximate (known limitations)

- **U1. Tension-only members** now carry compression up to their own
  elastic buckling load (O5, decided 2026-10-19; built, in active
  development, not approved yet): beyond it they are **bowed** and carry
  that load, shown in the results; without a second moment they go slack
  as before. A set that goes round, or a tie hiding compression in one
  half (a slightly kinked tie), has its ties split into their members:
  the case that used to go round now settles. *Still open:* keeping
  members active (flagged *approximate*) remains as a last resort when
  single members go round — no model tested reaches it; the
  energy-based active-set update that would always settle is for later
  (O5); no post-buckling rise or bending of a bowed member.
- **U2. Imported lattice towers**: looked at (2026-10-19; built, in
  active development, not approved yet). Most failures were the import:
  the source program's tension-only *beams* (insulators hung from the
  middle of such rods, X-bracing crossings) brought in as tension-only
  trusses, and joints placed at coordinates rounded to four digits
  (joints meant to be coplanar a little off: not held, nearly free,
  "unstable"). The core now has tension-only members that bend
  (`bending = true`) and members not checked (`check = false`, the
  source's fictitious members); the private importer uses them and places
  joints by their definitions. With U1 (compression up to the buckling
  load) and the relative holds, all but two of the first line's imported
  towers solve. A hold relative to its line is no longer refused above
  the 1 % limit (owner, 2026-10-19), only flagged; holds to the ground
  still are. *Still open:* two towers stay unstable — joints a little
  off a plane by the export's rounding of their primary coordinates, and
  a small frame group that turns about the line of its frame chain.
  *Owner, 2026-10-19:* the reading of the "tension-only beam" confirmed;
  the relaxed refusal for relative holds accepted; rebuilding rounded
  primary joints (one tower left) later; the exports keep the bending.
- **U4. OHL-Structure**: *built, in active development (not approved
  yet)*: the Results tab solves on the chosen extension, with the line's
  pole check (as the line does); wind bands rise with an extension (line
  and app, one rule); the structure report (HTML and members CSV, menu
  and `ohl-structure-report`). Follow-up of 2026-10-19, built (not
  approved yet): **the wind on the structures set apart from the wind on
  the wires** per load case (`[case.structure]` of the `.ohllc`, version
  2: the structures' own wind pressure and factor, defaulting to the
  weather case's pressure and the transverse factor; design-code plugins
  can supply them in the load cases they generate, the example plugins
  give none) — one rule for the line's structure checks (bands, member
  wind, pole) and OHL-Structure (the line values carry it; line cases put
  it on the members and the pole; recipes take `structure_wind` /
  `structure_wind_factored`); **`ohl structure analyse|report`**: `ohl`
  asks OHL-Structure's headless tool `ohl-structure-report` for a
  structure's results in JSON (a program of its own, found next to `ohl`,
  on the `PATH` or through `OHL_STRUCTURE_REPORT`; no crate dependency).
  Still open: the **report format, to define with the owner** (reports
  will be needed as for the line); a PDF of its own and the input hashes
  in the report with it; the same pressure still becomes member
  forces differently: the line's member wind takes U5's drag and
  shielding factors, OHL-Structure's surface loads the projected area
  without drag.
- **U5. Cross-section library**: sections are now typed in OHL-Structure
  (Sections & materials › Cross-section library: new, edit, duplicate, delete,
  properties computed as typed, undoable), and the member wind takes drag
  and shielding factors as **data** (a `[wind]` table of the section
  library, from a design-code plugin's new optional `structure-wind` or
  typed): OpenOHL finds the lattice faces, their solidity ratios and which
  face shields which member. Still open: no code's factors yet (the code
  plugins return none, so the generic drag stays and nothing is shielded),
  the factors not on OHL-Structure's surface loads, materials not typed in
  the editor; built, in active development (not approved yet).
  *Owner, 2026-10-19:* the choices of U5 confirmed (factors on the FE
  member wind only; only members in a face turned to the wind shielded;
  the face outline as the convex hull of its joints; no factor outside a
  table's range). **Materials typed in the section editor: planned.**
- **U6. Body generators**: redundants, plan bracing, widths per panel and
  several points on one crossarm built (2026-10-19, options off by
  default, in active development, not approved yet). A file using one is
  written at `.ohls` version 6 (one unreleased raise, also for anything
  else added before the next release; an older program says "newer").
  The joints the stabilisation holds across their face (the redundants'
  and plan bracing's) are held relative to the ends of the members they
  cut, so with P-Delta they follow the face: what they take is below
  5·10⁻⁴ of the loads in the tests, as in first order (held to the
  ground: 0.2–1.1 %), and a load across such a joint goes to the line's
  ends instead of the ground.

> **Owner:**

*In Part A:* U7 (the structure screen in OHL-Design) and U8 (level 2
and 3 members drawn with the line).

## Proposed stages (to discuss)

3. **Structures checked** — the code-check interface (O4) and a first code;
   clearances to the body (O6); OHL-Structure's open points (U4–U6).
4. **Line and structures together** — interaction levels 3 and 4 (O7).

## Detailed record

### Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

*Status: built as a base, in active development (not approved yet). Each item says what is simplified or left for later.*

- [x] `ohl-structures-fe`: 3D frame/truss FE analysis of lattice towers and poles (geometric non-linearity, guys) — standalone module. *Base:* `.ohlfe` models (truss, 3D Euler–Bernoulli frame, tension-only cables with pretension, supports, springs), first order or P-Delta (geometric stiffness, Newton iterations with exit clauses), skyline LDLᵀ with RCM ordering (1000 members in milliseconds), member usage (tension, compression with Euler buckling, linear N + M interaction; a member's own capacities and effective length, angle legs by structure directions, a dead-load factor on the self-weight), attachment nodes fed with each load case's factored loads (`fe` check in the structure usage, `ohl structures fe`, the Results tab of OHL-Structure). *Simplified / later:* no corotational large rotations, wind on the structure body only on profiles from the cross-section library (drag per shape, face drag by solidity and shielding of the faces behind only when the cross-section library's `[wind]` gives them — from a design-code plugin or typed; else one drag factor, no shielding), no code column curves or angle-section eccentricities, no member-load cases beyond self-weight and the load-case recipes, no 3D view of the model in OHL-Design
- [x] **Joint stabilisation**: lattice towers drawn as trusses solve — joint directions no member stiffens (coplanar or collinear trusses, a frame chain free to spin) are held when the model would otherwise be a mechanism, and the force they take is reported; a load no member holds is refused (a hold to the ground; a hold relative to its line is flagged above the limit, not refused — owner, 2026-10-19); true mechanisms are named with the joints that move *(since 2026-10-19, U6, not approved yet: a joint on a straight line of members is held relative to the line's ends, following its face with P-Delta; the others to the ground)*
- [x] **Tension-only members** (`kind = "tension"`, the "tension-only" members of tower programs): a truss in tension and in compression up to its own elastic buckling load π²EI_min/L_k² (L_k: `lk`, else `k` × length, k = 1 pinned), **bowed** beyond it carrying that load (slack without a second moment); solved by active-set iterations on that cap (members beyond it bowed, those loaded less put back, first order then P-Delta); members on one line through a joint switch as one tie, split into their members when the set goes round or a half hides compression; bowed members named in the notes, usage, `ohl structures fe` and OHL-Structure's results (benchmark `structfe-011`); tension-only members that bend (`bending = true`, a tower program's tension-only beams: frames while in, trusses bowed or slack; benchmark `structfe-012`) and members not checked (`check = false`, its fictitious members), `.ohls` 6, `.ohlfe` 3. *See Known limitations below:* the last resort
- [x] Wood/steel/concrete pole libraries and checks. *Base:* `.ohlp` poles (taper by diameter or circumference, fibre stress for wood, moment capacity at the ground line or by height, hollow wall), moment along the pole from the factored attachment loads (both directions, offsets) plus wind on the pole, P-Delta amplification from a tapered frame model (`pole` check, `ohl structures pole`). *Simplified / later:* one amplification factor for every height, no foundation/embedment check, no shear or deflection limits, no pole class tables (user data)

#### Structure analysis — known limitations (unfinished)
- [ ] **Tension-only sets that do not settle — last resort** (recorded 2026-10-18; O5 decided 2026-10-19). Since the buckling-load cap and the splitting of ties (above), the case that used to go round (a pair of diagonals in halves through a joint slightly off a tie's line, the joint held otherwise only by a weak spring) settles. Kept as a flagged **last resort**: when single members still go taut and slack in turn (or after 1000 solves), they are **kept active as ordinary truss members**, the others iterated on, and the result is flagged — a note *"tension-only members kept active because the set did not settle: … — approximate (last resort)"*, those members marked *kept active (did not settle)*, the structure's usage **approximate** (never a plain pass). No tested model reaches it. Later (owner, O5): the energy-based (or damped) active-set update, which would always settle

### Structures in their own axes, three detail levels
*Before 1.0, agreed with the project owner (2026-10-10). A structure is defined
in its own axes (x right of the line, y ahead, z up; 0;0 at the centre)
and placed in the line by a position, an angle and a height adjustment.*

- [x] Placement: the type's z = 0 may be any reference level (usually under the lowest crossarm); `foot` gives the level of its feet, so the placed structure stands with its feet on the ground, raised by the structure's `height_adjust` (foundations); extensions lengthen the body below the reference level
- [x] One file per structure (`.ohls`): joints and members in the type itself (sticks), level 3 adds sections, materials, supports and member kinds (truss / frame / cable / tension) to the same members; `.ohlfe` models still read. The level is **derived from the content** (1 stick figure → 2 lattice outline as soon as there are members → 3 as soon as every member has a section and a joint has a `support`; a structure standing only on springs stays at level 2 today); the marker follows
- [x] String points as joints, so strings follow the members *(since 2026-10-21 a point needs a joint: built, in active development (not approved yet) — a point is no longer put along a member; a joint is inserted on the member, the member split and continuous; files that placed one along a member are read so)*
- [x] Symmetry (optional, decided 2026-10-17): members and joints copied about x, y or both, to ease building — `[[symmetry]]`, edits mirrored, **Make symmetric** in OHL-Structure
- [x] **Body generators** (format version 3; the options version 6): legs and panels (single, X, K, diamond, Warren; horizontals), crossarms (triangular, rectangular), peak, copy a face — each writing ordinary joints and members; rebuild replacing only what a generator made; member groups with a section and kind per group; options off by default: redundants per panel (one or two levels), plan bracing (X, diamond) per level and where the slope changes or at the crossarms' levels, widths per panel, several points on one crossarm (2026-10-19)
- [x] **Poles with levels and extensions**: a `[pole]` table in the structure file (outline by height, round or polygonal; a solid pole counts), its levels, the pole check reading it as a `.ohlp`; extension pieces and leg extensions in the FE model of each placed structure *(U4, built, not approved yet: the pole check and extensions in OHL-Structure's FE results, wind bands raised by an extension, the structure report — HTML and members CSV from Results › Structure report… and `ohl-structure-report`, its format still to define with the owner; the structures' own wind per load case, `[case.structure]` of the `.ohllc`, one rule for the line and OHL-Structure; `ohl structure analyse|report` asking `ohl-structure-report` in JSON)*
- [ ] Lattice tower geometry from another program's XML export (joints, members, sections) into levels 2 and 3 (local importer)
- [ ] Level 2 from LiDAR: a plugin (later); the classes holding structures chosen by the user from all classes, as in the display options
- [ ] Clearances to the structure body: none for level 1 (LiDAR points only, separate topic); levels 2 and 3 computed — **the tests are to be defined with the project owner**
- [x] **Structure application OHL-Structure** (`ohl-structure`, decided 2026-10-17, [spec](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/structure-building.md)): a separate program on the same workspace crates, started from **Edit** on a structure in OHL-Design or by opening a `.ohls`, or on its own (start page, launcher, its own libraries and plugins); its tabs (geometry, body, design, load cases, results, use in the line), undo, what is missing for each level *(not yet: a report of a structure; the mechanism found by a trial solve among the advice)*
- [x] Levels as decided: 1 attachment points only (stick figure, not verifiable); 2 the real geometry (no code check); 3 every part and joint detail, computed (FE) and code-checked (e.g. SIA 263). **Poles** the same: 1 points without body, 2 outside lines, 3 detailed engineering (a wall thickness, or solid). `[[body]]` polylines give level 2 only in 3D. At levels 1 and 2 maximum loads only on the strings, never for the structure itself (its capacity only at level 3) *(the level derived from the content; the code check of level 3 waits for the code-check plugins below)*
- [x] A structure file as a structure family: body and leg extensions chosen per position adapt it to the ground; each placed structure unique to its position (its own name, loads, model, results) — OHL-Structure started for a placed structure works on its own loads and extension
- [x] **Load-case recipes** per structure (`[[load_case]]`, `.ohlslc` or CSV): which line value, factor, one-sided share, on which points or members, surface loads (wind, snow); evaluated by the core with the line's values at each placed structure; the loads in a table, editable (typed values kept over the evaluation), exported and imported
- [x] **Cross-section library**: a project library kind the structures refer to (not copied), exported and imported *(base: `.ohlsec` profiles and materials with computed properties; a member's section from the library first, then the structure's tables; the FE check's member wind on the profiles, the wind bands kept out of the model; member areas for surface loads; sections typed in OHL-Structure (Sections & materials › Cross-section library: the library's shapes, properties computed as typed, what is wrong said, each change an undo step in order with the structure's edits; members pick them); wind factors for structures as data (`[wind]`: drag per shape, a lattice face's drag and shielding by solidity ratio and spacing) from a design-code plugin's optional `structure-wind` (WIT `code-wind-plugin`, interface 0.3 unchanged for older plugins; the code plugins' stubs give none) or typed, with the faces, solidity ratios and shielding computed from the geometry (`docs/theory/structure-wind.md`, validation cases structwind-001/002). Later: the codes' factors in their plugins (O9), the factors on OHL-Structure's surface loads, materials typed in the editor)*
- [x] Structure exports: IFC4 (member solids and the analysis model with load cases), IFC2x3 for steel-detailing programs, SAF 2.2 (xlsx) for analysis programs, CSV tables; checked with free tools (IfcOpenShell validation with the schema's rules, geometry and content; SAF sheets, columns and references; `tools/check_structure_exports.py`, test run when `OHL_IFC_PYTHON` is set) and sample files in `examples/exports/`; a tension-only member that bends is exported as a frame with its tension-only behaviour and `check = false` as information (IFC property set, the analysis member's description, SAF layer, CSV columns) *(not yet: the imports tried in another program; other tower programs)*
- [ ] Code checks of members and joints as plugins (e.g. SIA 263), interface agreed with the project owner first
- [ ] **Fully integrated structure editors with a 3D preview** for every detail level (1 stick figure, 2 sticks of the structure, 3 full design), all in one window: geometry, members, sections, materials, supports, strings and wire points, the type's use in the line, checks — lattice towers and **poles of wood, steel (and concrete)** alike *(base: OHL-Structure, with poles in Body › Pole and Design › Pole; still to come: typed sections and materials, checks per level, the use in the line)*

### Structure–line interaction — levels of computation
*Agreed with the project owner (2026-10-10), levels defined 2026-10-17 as is common in line design software: how far the line and its structures are computed together, chosen per run.*

- [x] **Level 1** — ruling span: sections sagged by the ruling span method on rigid structures
- [x] **Level 2** — FE of the wires on rigid structures (`ohl-wirefe`, what OpenOHL has today); the conductors' forces put on the structures, every structure checked once *(today's structure loads and usage)*
- [ ] **Level 3** — FE of the wires with a linear stiffness matrix at each structure's suspension points, derived from the structure (level 3 structures only)
- [ ] **Level 4** — the full solve: structures and line iterated until both converge (displacements and forces between passes below tolerances; exit clauses, damping, reports of what did not converge)
- [ ] The level chosen per run (app, CLI, load cases), shown with every result; a structure below level 3 is rigid at levels 3 and 4, and the result says so

---

# Part C — Shared

The engine both applications run on, the formats, plugins and SDK, the
server and version control, validation and docs, 1.0 and further ahead.

## Where it stands

- **Two applications on one engine**: **OHL-Design** (`ohl-design`: the
  line, sag-tension, clearances, layout, drawings) and **OHL-Structure**
  (`ohl-structure`: structures and their loading), with the `ohl` command line.
  Project renamed OpenOHL (formerly OLS CAD).
- Phases 0 to 5 of the detailed records are built as a base, all in active development (none approved yet); the structure work
  (levels 1–3, poles, extensions, generators, loads per placed structure,
  FE with joint stabilisation and tension-only members (compression up to
  their own buckling load), cross-section library,
  exports) is in place with the limits listed below.
- 177 validation cases pass (47 textbook and CIGRE TB 324 cases added
  2026-10-20; 3 on the weight of a heated span, 5 on the swing band and safety volumes, 3 on the band margin and the jumpers' bands, and 3 on the data sheet's weight temperature and the ice per stretched metre 2026-10-21; 2 on attachment points: an offset's moment and a member split at a point, 2026-10-21; 1 on the jumpers' own band margin and 1 on the ice and wind per unstretched metre, 2026-10-21; 2026-10-22: 2 on the curves' range, 5 on tension criteria on the non-linear model, 3 on the sagging outputs, then the 9 Drake-like cross-check cases removed and 5 on both final states and the signed set on an invented cable, 2 more on the curves' range added); the private Swiss plugin and the importers of
  another program's files live outside the public code.

### Just finished (2026-10-18)
- **Start-user improvements** in both apps: the review's quick wins, a
  **Next steps** page, the **starter template project**
  (`examples/starter/`: invented libraries, no line), **Open the example
  (copy)**, clearance windows that always have a rule (its distance to
  set) with an **Add a rule** helper, tool windows that can be docked,
  Help › User guide / Start from scratch / About in both apps.
- **Icons in the apps**: window icons, tool icons in the hotbar,
  inventory and menus, the OpenOHL mark on the start pages and in About.

Nothing is running now.

> **Owner:**

## Open points — waiting on a decision

### O2. Rename the project folder on disk
`…/Claude/OLS CAD` → `…/Claude/OpenOHL`, as the last step when nothing is
running and the apps are closed (the agents' worktrees and the saved
session notes follow the path).
> **Owner:** Done 2026-10-18 (folder, repository and website renamed;
> formats, crates, plugins and local data renamed ols → ohl).

### O9. Design-code loads still to come
Security / broken-wire and construction loads, and code-specific wind on
structures, in the design-code plugins.
> **Owner:** (2026-10-19) All three, each in its respective design-code
> plugin. The core gets a **broken-wire case** type (the section solved
> with the wire gone) for the plugins to use: approved.

### O11. Validation report
Phase 1 validation against published examples (CIGRE TB 324, textbooks)
is started (2026-10-20, built, in active development (not approved yet)):
31 textbook cases with hand derivations and an independent reference
script, and the 16 TB 324 examples whose inputs are complete
([Textbook and published cases](docs/validation/textbook.md), which also
says which examples were left out and why). The cases found that a
heated span weighed more than its wire (the finite elements by
\( e^{\alpha\Delta t} - 1 \), the closed forms with every lengthening);
corrected 2026-10-21 by owner decision: every calculation keeps the
wire's weight (`tbheat-001…003`; [Decision log](docs/development/decisions.md)).
Also 2026-10-21 (owner decisions; built, in active development (not
approved yet)): the weight is the data sheet's at its reference
temperature (20 °C unless the wire type gives one), taken at the stringing
temperature (`tbwref-001…002`), and the finite elements carry the ice and
wind per metre of the stretched conductor (`tbice-001`), with an option
per unstretched metre as other programs (`tbice-002`, a Simplification).
A full validation report is
a goal for 1.0; the non-linear (stress-strain and creep) examples need
published curves (no tables typed from memory).
> **Owner:** (2026-10-19) Later: the owner will say when reference data is
> available.

## Proposed stages (to discuss)

5. **Towards 1.0** — validation report (O11), stable plugin SDK, plugin
   registry, translations (Phase 6 below).
6. **Further ahead** — OHL-Foundations, then OHL-Substations (Further ahead, below).

> **Owner:**

## Detailed record

### Phase 0 — Foundations
*Goal: a project people can contribute to.*

- [x] Choose license (see ADR-0001)
- [x] Choose tech stack: language, 3D engine (ADR-0002) — GUI toolkit deferred to Phase 3
- [x] Define the plugin / extension mechanism (ADR-0003)
- [x] Define the open project file format (ADR-0004)
- [x] Define the collaboration / session model (ADR-0005)
- [x] Define the rendering, LOD & streaming strategy (ADR-0006)
- [x] Cargo workspace layout, CI (clippy, tests, validation cases, docs; macOS, Windows and benchmarks in a full run), started by hand only
- [x] Python bindings skeleton (`ohl-py`, PyO3) and WASM plugin host skeleton
- [x] Performance benchmark harness (criterion) with a regression gate in a full CI run (more than 1.5× slower than the parent commit fails)
- [x] Units system (SI internally, US customary + SI at the I/O boundary)
- [x] Documentation site skeleton (theory manual + API reference)
- [x] Validation/benchmark framework (case format + reference results + tolerance reporting)
- [x] Contribution guide, code of conduct, governance, clean-room policy
- [x] Exit clauses: `ohl_core::budget` (iteration/time limits, cancellation), plugin time & memory limits, validation case time limits, CI job timeouts

**Deliverable:** empty-but-wired repo, CI green, ADRs accepted.

### The engine
*From Phase 2.*

- [x] `ohl-session`: project file (`.ohl`), validated commands, operation log, per-user undo/redo (content-based conflicts), admin rollback, roles, dirty tracking with **incremental re-solving** (`ohl-line::incremental`, `ohl_session::Live`: only the touched sections, identical to a full solve), `.ohl.json` import/export

### Plugins and SDK
*From Phases 2 and 4.*

- [x] Exporter/importer plugin interface (first formats: CSV, KML/KMZ; GeoJSON example plugin)
- [x] Design code plugins: NESC (ANSI C2), CSA C22.3, IEC 60826, EN 50341 (+ national NNAs), ASCE MOP 74, GO 95 — each as a separate plugin (a Swiss LeV plugin exists separately, not part of the public code) *(base: `code-plugin` world, `plugins/codes/*`, `ohl code`, Criteria › Design code; each code's method with every value from the user's parameter file — no code tables shipped; security / broken-wire and construction loads, NESC constant k and code-specific wind on structures still to come)*

### Formats and interop
*From Phase 5.*

- [x] Interop plugins: XML (LandXML + open OHL XML schema), KML/KMZ, IFC, Shapefile/GeoJSON/GeoPackage, CSV, XLSX; import of common neutral formats from other tools (DESIGN_GOALS §8) — base, built into `ohl-io` (written from the public specifications, no native GIS library): OHL XML (whole line, lossless, `docs/schema/ohl-line.xsd`), LandXML alignments + CgPoints (export; import of points/alignments), GeoJSON / zipped Shapefile / GeoPackage layers (structures, routes, wires per case with tensions and sag), IFC4 (structures as proxies with stick geometry, spans as cable segments, property sets), XLSX report tables; structures imported from CSV (mapped columns), GeoJSON, Shapefile and LandXML points along a route, into a line or a project as one command. Left for later: shapefile `.prj` / GeoPackage WKT for projected systems, GeoPackage spatial index, IFC georeferencing (`IfcMapConversion`) and solid geometry, curves of LandXML alignments (read as chords); DXF and PDF are written by `ohl drawing`, not as `ohl-io` formats

### Server and version control
*From Phases 3 and 5.*

- [x] **Server mode (pre-alpha)** (`ohl-server`, `ohl serve`, File › Host / Join): host a project, multiple users connect over LAN/Internet (QUIC, TLS pinned by the invitation), live change broadcast to replicas, presence (People page, plan markers), soft locks (Move window), per-user undo, roles & permissions (personal invitations; admins set roles), admin rollback (Edit › History) *(still open: optimistic prediction, chunk-based interest management, a WebSocket fallback for the application, OIDC)*
- [x] Collaboration (base, ADR-0010): edit grants per user or role limited to kinds, objects or a plan polygon, checked by the session for every command (so by the server too); review & approval of branches with protected branches; offline work as a branch carried back in a bundle and merged. Not yet: automatic sync of offline copies, custom roles, area grants drawn in the plan
- [x] **Git-like version control** of projects (base, ADR-0010): content-addressed revisions of the line and its libraries in the project file, history browser (Edit › Versions, `ohl project history`), diff field by field with engineering impact (touched sections solved on both sides: sag, tension, ground clearance), branches, three-way merge by object and field with conflicts settled per key, tags, revert; through the server's operations. Not yet: cherry-pick, visual diff in the views, history export for audit (DESIGN_GOALS §7)
- [x] Web client (WebGPU) joining server sessions — base: `ohl serve --web PORT` / File › Host this project (Web client), HTTPS + WebSocket with the same codes and roles; 3D (WebGPU, Canvas2D fallback), plan and profile of the line with wires solved on the server (one weather case), selection, presence, live operation log; designers move structures. Not yet: terrain/point clouds, weather choice, other edits

### Server and version control — more capabilities
- [ ] **Server**: more editing through the web client (structures, strings, sections), terrain and point clouds in the web client, several projects per server, accounts and roles managed in the app, presence and locks per object, a server log and backups
- [ ] **Version control**: cherry-pick, visual diffs in the views, history export for audits, revision comparison of results (sag, clearances, loads), automatic sync of offline copies, custom roles, area grants drawn in the plan

### Phase 6 — Ecosystem & 1.0
- [ ] Stable plugin SDK (semantic versioning, compatibility guarantees)
- [ ] Plugin registry / index
- [ ] Complete theory manual and validation report
- [ ] Translations (i18n)
- [ ] Public release, governance model, maintainers team

### Cross-cutting (every phase)
- Validation cases for every calculation, with references
- Theory documentation written alongside the code
- Performance benchmarks against the targets in DESIGN_GOALS.md (large lines: 500+ structures, LiDAR with 10⁹ points); regressions fail a full CI run
- UX reviews with practising line engineers
- Backwards compatibility of the project file format (versioned schema + migrations)

### Further ahead
*Two further applications on the same engine, added by the project owner (2026-10-18), in this order. Neither is started.*

1. [ ] **OHL-Foundations** — the foundations of the structures: foundation types per structure (pad and chimney, piles, rock anchors, pole embedment…), their data in their own library files, the loads the structure checks give (level 1 and up) on them, checks by the user's code parameters (uplift, compression, overturning, sliding), bill of materials and costs; the structure's `height_adjust` linked to the foundation's reveal
2. [ ] **OHL-Substations** — substations, where lines end: gantries, busbars and equipment

> **Owner:**
