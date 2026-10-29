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
- [Part A — OHL-Design (the line)](#part-a--ohl-design-the-line): O1, O3, O13–O16, O18; U7, U8, U10, U11; Phases 1–5 for the line
- [Part B — OHL-Structure (the structures)](#part-b--ohl-structure-the-structures): O4–O8, O10, O12; U1, U2, U4–U6; structure analysis, structures in their own axes, structure–line interaction
- [Part C — Shared](#part-c--shared): O2, O9, O11, O17; Phase 0, the engine, plugins, formats, server and version control, Phase 6, cross-cutting, further ahead

The phases below record what is built. Phases map to GitHub milestones;
each bullet becomes one or more issues labelled by module (`module:*`).

---

# Part A — OHL-Design (the line)

**Next up** (2026-10-25): the broken-wire case (O9), local winds per
span (O15), the sheave model for sections in the blocks and the lyre
pylons' risers are built (in active development, not approved yet);
next background maps in the project (O16). Still open: O13 (the conductor jump, to be evaluated), the
jumpers' swing band margin (left to the owner, Phase 4) and the items
not built in the record below (`[ ]`).

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

**Status (2026-10-25):** built at the owner's request, in active
development (not approved yet): eleven chapters in `docs/tutorial/`
(Help › Start from scratch), each chapter's steps run by a test
(`crates/ohl-app/src/tests/tutorial.rs`); chapter 9 on the loads for the
structures.

*Built (in active development, not approved yet), 2026-10-25, at the
owner's request: [Start from scratch — your first
line](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/tutorial/index.md),
eleven short chapters (0–10) on the starter template — the ground, the
route and alignment, six structures, their strings, three phases strung
in two sections, the jumpers, the results, the checks, saving; Help ›
Start from scratch opens it in both applications. Chapter 9
(OHL-Structure) is a placeholder until the new structure load cases
land. An end-to-end test does its steps through the UI
(`crates/ohl-app/src/tests/tutorial.rs`).*

### O3. Usability review — items not yet decided
From the review of 2026-10-18 (quick wins are being done now):
- **One name per object** across both apps and the docs (string /
  insulator / attachment point / structure point …), with a glossary.
  *Built (in active development, not approved yet): the terms agreed on
  2026-10-21 in the [Glossary](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/glossary.md) (Help ›
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
  *Open (owner, 2026-10-27: stays on the roadmap; needs a robust way to
  know which set goes where, see 3. below).*
- **Tools act on left-click** — *done: since the UI review of 2026-10-26
  a left-click acts with the tool held (Select held: it selects) and a
  right-click opens the object's menu, in every view (the option of
  2026-10-19 is no longer needed).* Before: *Built (in active development,
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
> **Owner:** (2026-10-27) 3D and Plan stay as they are (point removed).
> One name per object: yes — to be finished across both apps and the
> docs. "String all sets…" stays open. Tools on left-click: done.

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
*Built 2026-10-24, in active development (not approved yet): the route
plan, the situation plans and the phase plan (Export › Plans, `ohl
drawing --plans`), see [Drawings](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/drawings.md#plans-route-plan-situation-plans-phase-plan);
and plan sets from plugins (Export › Plans › Plan set…, `.ohlset`,
the plugin manifest's `[[plan-set]]`); *built 2026-10-25, in active
development (not approved yet):* one PDF of a set (Plan set… › also as
one PDF, `--set-pdf`) and `ohl drawing --set FILE|PLUGIN/NAME`, a set's
second drawing of a kind named apart (`-2`); still to come: links to
public map services, the corridor's protection zones and land use as
their own layers (today: the project's drawings).* Proposed (to discuss):

1. **Route plan and situation plans** — the plan alone, the line drawn
   on a background, at two scales (owner, 2026-10-23):
   - the **route plan** (overview of the whole route) at 1:5000 to
     1:25 000, as needed to see it, with the municipal boundaries and,
     where the template asks, protection zones, waters, protected areas
     and infrastructure (from layers the user loads);
   - **situation plans** (the same, local) at 1:500 to 1:2000, with the
     parcel boundaries within 50 m each side of the line and the parcel
     numbers (from the cadastral survey loaded as a drawing);
   - both with a complete legend, the scale written and drawn, a north
     arrow;
   - the line's axis, its structures (numbered), the corridor, the other
     lines and project lines in their roles, crossings marked; legend,
     north arrow, scale bar, title block from the sheet template;
   - background: a map the user loads into the project (an image layer,
     like the orthophotos) and the project's orthophotos and DXF drawings
     (as under the sheets' plan, 2026-10-23), chosen in the template;
     **later**: links to public map services (an interface to fetch
     their maps), a separate step;
   - laid out on sheets along the route (overlapping, a key of the
     sheets), or one sheet of the whole line fitted to a paper size (the
     paper optimisation of 2026-10-22).
2. **Phase plan** — where the phase arrangement changes along the line,
   and nothing else (owner, 2026-10-23):
   - only the **stick figures** of the structures (the schematic
     elevations of 2026-10-22), each wire position labelled with its
     circuit and phase (phase colours; earth wires);
   - drawn only where the arrangement on the structure **changes**
     (a transposition, a circuit leaving or joining, a change of arm);
   - where everything runs straight along the line unchanged, no
     drawing: a text "from structure A to structure B: arrangement
     unchanged" (the template's words) between the changes;
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
> **Owner:** (2026-10-23) The phase plan shows only the stick figures of
> the pylons and the phase changes in the layout on the pylon; if
> everything goes straight along the line unchanged, not drawn but a
> text "from mast to mast", the plan showing where the line changes.
> Background map of the situation plans: for now a map the user loads
> into the project; later links to public services (API).
> **Owner:** (2026-10-23) Route plan (full route) between 1:5000 and
> 1:25 000 depending on what needs to be seen; situation plans the same
> but smaller, 1:500 to 1:2000, to show what is needed locally (as
> described in the authority's directive for plan submissions).
> **Owner:** (to answer) The plans of a set and their naming.

*In Part B:* O6 (clearances from the strings and conductors to the
structure body) and O7 (the line and its structures solved together).
*In Part C:* O9 (design-code loads, the broken-wire case of a section).

### O15. Local winds given per span
A weather case has one wind pressure for the whole line. Some spans see
another wind (an exposed ridge, a sheltered valley, a wind study for a
stretch). Built 2026-10-25 (in active development, not approved yet;
[Line file](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/line-file.md#local-winds-per-span)):
per span a **factor on the case's wind**, in every weather case with
wind (`wind_factors` of a section, line file 9), used wherever the case's
wind acts on the wires — the wire solve (blowout, tensions), the
strings and jumpers (the larger factor next to them), the structure
loads, the clearances' blowout and swing arc, the distance under unequal
wind (each span steps up to its own wind), the wind span — and edited in
the Sections table (`2–4: 1.2`), a span's Properties (a run of spans)
and `ohl project apply` (`set_wind_factor`), each one undo step;
validation `localwind-001…004`. Simplified (said in the theory): the
same factor on every case with wind, the ice-with-wind cases too; the
structures' own wind not scaled; the ruling span takes the section's
largest factor. Still open: a factor per weather case, a direction of
its own, editing on the profile.
> **Owner:** (2026-10-24) Be able to have local winds, given per span.
> **Owner:** (2026-10-25) A factor on the case's wind, for every weather
> case with wind.

### O16. Background maps imported into the project
The drawings and orthophotos already show on the plan, the sheets'
plans and the key map. Still to come: background maps imported into
the project (one or many), each shown on the plan view only or switched
on the other views and the sheets (the key map, the plan); later, links
to public map services.
> **Owner:** (2026-10-23) Asked, with the key map's backgrounds.
> **Owner:** (2026-10-25) A map loaded into the project is fine; shown on
> the plan, the sheets and the plans (not draped in 3D).

**Built 2026-10-25, in active development (not approved yet):** an image
layer is an orthophoto or a map (Terrain › Backgrounds › Import
orthophotos or maps…, the kind turned in the menu); the route and
situation plans and the key map take the maps first. Later: links to
public map services.

### O18. Electrical module: line parameters, load flow, fields and noise
Electrical calculations from the line as it is modelled, so the designer
gets the line's electrical data from the same geometry as its mechanical
design: the conductors where they hang (in a weather case or a chosen
sag), their bundles, the wire data, the earth wires, the other lines
nearby. **Draft (owner, 2026-10-27; to discuss).** Steps 0 and 1 (the
cross-sections and the line parameters) **built, in active development,
not approved yet** (2026-10-27, owner's choice of the first part); step 4
(the magnetic field) built in part, in active development, not approved
yet (2026-10-27).

**Where it lives.** A headless engine crate `ohl-electrical` (no GUI, no
plugin calls) for what is textbook and general — line parameters, the
load flow and the electric and magnetic fields (2026-10-27: the fields
moved into the core, "magnetic field first") —, a window in OHL-Design
(Checks › Electrical) and `ohl electrical …` commands; **the limits and
rules of an ordinance and the noise methods through plugins**, so that a
utility or a research group brings its own limits (an ordinance's limits
in its country's plugin, e.g. the LeV plugin) and its own methods, the
core staying independent of them.

**What the core gives every electrical calculation (one data model):**
- **Cross-sections of the line** at any station (or a span's lowest
  point, or where a clearance is smallest): every conductor and earth
  wire there in the plane across the line — position (horizontal, height
  above the ground under it), bundle (subconductors, spacing,
  arrangement), wire (diameter, resistance at a temperature, geometric
  mean radius), its circuit, system and phase (voltage, frequency, phase
  angle), the ground (height, slope across, soil resistivity) — and
  other lines' wires in the corridor. The same `CrossSection` record for
  the parameters, the fields and the noise, exported and given to
  plugins.
  *Built (2026-10-27; in active development, not approved yet):*
  `crates/ohl-electrical`, `CrossSection` at a station or as each span's
  mean (the solved wire's average, or attachment height minus 2/3 of the
  sag), the line's own wires; the soil resistivity as a line setting (one
  value, 100 Ω·m assumed when not given, said). Not yet: other lines'
  wires (an empty extension point), soil resistivity along the route, the
  terrain under the spans (straight between the feet), the plugin
  extension point (with step 6).
- **Operating data per system**: voltage (rated, highest), current per
  circuit (given, from the load flow, or the thermal rating's ampacity),
  phase sequence and transposition along the line.

1. [x] **Line parameters** (core) — *built 2026-10-27, in active
   development, not approved yet*: `ohl electrical parameters`, Checks ›
   Electrical › Line parameters…, [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/line-parameters.md),
   13 validation cases (Kersting's Ex. 4.1 and Overbye's lecture, Carson's
   integral by quadrature). Built: Carson's series / complex depth / first
   terms, Kron, potential coefficients, per circuit and stretch, sequence
   values ideally transposed and the coupling as built, zero sequence
   coupling between circuits on the same spans, transpositions along the
   line in the totals. Not yet: parallel lines elsewhere, a transposition
   plan, soil along the route. The draft: per circuit and section, from the
   cross-sections (mean height in the span, or several stations
   averaged): series impedance with earth return (Carson, or its complex
   depth approximation) and the earth wires reduced out, as phase
   matrices and as positive, negative and zero sequence values; shunt
   capacitance and susceptance (potential coefficients with images);
   mutual coupling between circuits on the same structures and with
   parallel lines; transposition (a plan along the line, or ideal).
   Data to add: each wire's resistance and geometric mean radius (the
   wire library has `[cable.electrical]` already), soil resistivity along
   the route.
2. [ ] **Basic load flow** (core) — buses, the project's lines with their
   parameters, a slack bus, generators, loads and transformers as simple
   models; voltages, currents, power flows and losses in steady state,
   balanced first (Newton–Raphson), unbalanced from the phase matrices
   later. The current each line carries set beside its thermal rating
   (ampacity, existing) and usable as the current of the field
   calculation.
3. [ ] **Exchange** — the parameters exported for network calculation
   programs in an open format (to choose: CIM/CGMES line parameters, or a
   simple table).
4. [ ] **Magnetic field (core; first of the fields — owner, 2026-10-27:
   "magnetic field first").** Draft,
   to discuss. The magnetic flux density is textbook (Biot–Savart), so
   the calculation is in the core like the line parameters; the limits
   and the rules of an ordinance (which current, which places, which
   limit) come from a plugin's parameter file (for Switzerland the NIS
   ordinance, NISV: in the LeV plugin or one of its own), never typed
   into the core. Reference for what an ordinance asks: the BAFU's NISV
   enforcement guide for high-voltage lines (2007 draft for trial, still
   current; sections 2.4 and 8.1.4), kept in the owner's documents.
   - **Method (3D).** Each conductor and earth wire as solved in a
     weather case (the polylines the solver gives: sag, line angles and
     finite lengths as they are), a bundle subconductor by
     subconductor, carrying a complex current (rms magnitude and phase
     angle); the field of each straight segment in closed form
     (Biot–Savart for a finite segment), summed as phasors per
     component; reported as the rms of the vector,
     \( B = \sqrt{|B_x|^2 + |B_y|^2 + |B_z|^2} \) (as the NISV asks;
     the largest value over the cycle — the ellipse's major axis — given
     beside it). The ground's return currents left out at first (their
     image lies at the complex depth, hundreds of metres down at 50 Hz:
     said as an assumption and its size estimated in the theory page);
     the line's own wires only, then other lines in the corridor (the
     cross-sections' extension point). A **2D variant** on the
     cross-sections (infinite straight parallel conductors at the span's
     mean height) for quick profiles, refused or warned where the NISV
     guide says 2D is not valid: within reach of a line angle over 5°,
     near crossings, on spans between differently arranged structures.
   - **Currents.** Per circuit: given (A), the wire's thermal rating
     (the ampacity OpenOHL already computes; the NISV's governing current
     is the thermal limit current at 40 °C and 0.5 m/s wind), or later
     the load flow (step 2). The direction of the load flow per circuit
     as a sign (the NISV's governing combination of directions; BAFU's
     LFS spreadsheet finds it from operating data); balanced phases, no
     harmonics; phase angles from the systems (L1 0°, L2 −120°, L3 +120°
     by default, a transformer's fixed shift added, e.g. Yd11 +30°).
     Systems of two frequencies (16.7 Hz rail, 50 Hz) computed apart and
     added as \( \sqrt{B_{16.7}^2 + B_{50}^2} \).
   - **Where.** Profiles across the line at stations (every span's
     lowest point by default, ±50 m, 1 m above the ground — heights as
     the parameter file says), grids on a horizontal or vertical plane
     (isolines: the 1 µT line of the NISV), and given points: **places
     of sensitive use** (OMEN) as point objects of the project, typed or
     taken from the survey's buildings class or a building file, each
     with its use and height(s). The modelled length: the whole line, or
     at least five times the 1 µT reach before and after (the NISV
     guide's rule), checked and said.
   - **Earth wire currents** (the NISV guide: optional, included when
     the program computes them): first from the impedance matrix the
     line parameters already give (an infinitely long earth wire,
     earthed everywhere: \( I_e = -Z_{ee}^{-1} Z_{ep} I_p \)), then
     per span with the towers' earthing (a finite earth wire, the
     footing resistances: Nguyen, Liebermann, Schegner, TU Dresden 2024
     — needs a measured earthing resistance per structure, not the soil
     resistivity column).
   - **Results.** A window (Checks › Electrical › Magnetic field…) and
     `ohl electrical magnetic`: the profiles plotted (B against the
     distance from the axis, per station), the isolines drawn on the
     cross-section panel and in plan on the 3D view, the values at the
     OMEN in a table with the limit and the margin, CSV; out of date like
     the other checks. The NISV form's numbers (the 1 µT line's width per
     span) for the site data sheet.
   - **Phase arrangement.** Later: the arrangement of phases on a
     double-circuit structure that gives the smallest field (the NISV's
     optimisation duty for old lines), tried over the permutations.
   - *Built in part (2026-10-27; built, in active development, not
     approved yet):* `ohl electrical magnetic` and Checks › Electrical ›
     Magnetic field…, [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/magnetic-field.md),
     8 validation cases (electrical-030…037). Built: 3D on the solved
     wires (finite segments, subconductors, phasors; B and the cycle's
     largest value), 2D at the station or the span's mean with warnings
     (line angle over 5°, differently arranged structures), currents
     given or the thermal rating (command line) with a direction per
     circuit, frequencies combined, induced earth wire currents (infinite
     earth wire), profiles (every span's lowest point by default), grids,
     points from a CSV, an optional limit with margins and the limit
     line's extent, the modelled length rule said. *Built 2026-10-29
     (owner: cuts across "to show what happens near buildings", from
     the top, 3D with the performance checked; every modelling setting
     with its effect and time):* levels as a table (display only), the
     isolines from above, the field in every cross-section panel open,
     volumes (isosurfaces in the 3D view, OBJ), shorter wire pieces, the
     wires modelled within a distance (default: the whole line), each
     drawing's distance off its level and each setting's effect measured
     at every run, time estimates from this computer's speed. Not yet:
     places of sensitive use as project objects, the thermal rating in
     the window, crossings and other lines, the earth wires per span with
     the towers' earthing, the phase arrangement search, the site data
     sheet's numbers, a limit from a plugin's parameter file, a published
     profile in the validation.
   - [x] **Option: the jumpers** as current paths (owner, 2026-10-29) —
     *built 2026-10-29, in active development, not approved yet*: a
     jumper between two section ends of one phase carries its phase's
     current as solved (on by default); taps and mixed phases left out,
     said.
   - [x] **Option: the structures' steel** (owner, 2026-10-29) — *built
     2026-10-29, in active development, not approved yet*: the earth
     wires as a network through every structure's earthing resistance
     (a new structure and line value, 10 Ω assumed when not given), the
     structures' currents as paths down their axes; the steel's
     permeability not modelled (its effect estimated in the theory: under
     0.25 % at 1 m from a 5 cm member). Not yet: the currents shared over
     the legs, a substation's earthing grid at the line's ends.
   - [x] **Continuity between the views' tiles** (owner, 2026-10-29) —
     *built*: grids from above and volumes laid on one path along each
     run of spans, mitred at the line angles.
   - **Validation.** Closed forms (one infinite straight conductor; a
     balanced three-phase flat line, whose field falls as 1/d²), a finite
     segment against numerical integration, published profiles (to find:
     IEC 62110 annex examples, CIGRE TB 375 and the papers validated
     against measurements, e.g. Bürgi et al. 2017 for Swiss lines); the
     validation rests on public sources and closed forms.
5. [ ] **Electric field (core, after the magnetic field).** The charges
   from the potential coefficients the line parameters already have
   (\( q = P^{-1} V \), images in a ground plane, the earth wires at
   zero potential), the field of the line charges and their images at
   1 m above the ground; the NISV's governing state for it (the highest
   operating voltage, the largest sag, a circuit that compensates
   another taken as switched off and earthed), a slope across as a
   tilted plane; 2D enough (the guide), 3D with the solved wires later.
   Validation as above (closed forms, published profiles).
6. [ ] **Audible noise and a cross-check of the fields — a plugin for
   hvlbuzz.** [hvlbuzz](https://fkh-ch.gitlab.io/emc/hvlbuzz/index.html)
   (FKH, Switzerland; a Python library on a Rust core, `ebl`) computes the
   electric field (V/m), the magnetic flux density (µT, RMS and complex)
   and corona audible noise (dB(A), BPA method) of AC and DC lines from
   the phase systems' geometry, bundles, voltages and currents, guard
   wires, terrain slope and soil resistivity, at any points, with contour
   lines. A plugin would:
   - take the core's cross-sections (above) at the stations asked
     (every span's lowest point, a list, or a profile along the line), the
     voltages (rated or highest) and the currents (given, load flow, or
     ampacity) — every input from the project, nothing typed twice;
   - call hvlbuzz for profiles across the line (e.g. ±50 m at 1 m above
     ground) and at given points (sensitive places: houses from the
     cadastral or 3D-building data, the survey's buildings class), in
     the weather cases asked (rain/fair weather for the noise);
   - give back tables and profiles (plotted in a window, exported), the
     field and noise contours drawn on the cross-section, and values
     against limits **given in the plugin's parameter file** (an
     ordinance's installation or exposure limits, never typed from
     memory; for Switzerland the NIS ordinance, in the LeV plugin or a
     plugin of its own).
   - Built as a **Python plugin** first (hvlbuzz is a Python library; the
     Python console and plugins need the build with Python, not the
     current release builds), or as a WebAssembly plugin on the Rust core
     `ebl` if its licence and API allow. *To check before building: the
     licence of hvlbuzz and ebl (not stated on the documentation page),
     the method's validity range, and their published validation.*
   - A small core addition for it: a plugin extension point
     `cross-sections` (the cross-section record sent, tables and drawings
     given back), so any field or noise method can be added the same way
     (also, for instance, radio interference or surface gradient).
7. [ ] **Corona and surface gradient** in the core or the plugin: the
   conductor surface gradient per subconductor (Markt–Mengele or a charge
   simulation), the onset gradient (Peek) and the margin, per section —
   to decide whether it belongs to the plugin above.

Methods from textbooks and papers (cited in a new theory page), each
validated against worked examples with their published values; the
plugins' methods validated by their authors, the comparison values in
the plugin's own tests.
> **Owner:** (2026-10-26) Before the first release, put on the roadmap an
> electrical module for the necessary electrical calculations, like the
> impedance, or simply a basic load flow.
> **Owner:** (2026-10-27) Draft the electrical module ideas, including a
> plugin for hvlbuzz.
> **Owner:** (2026-10-27) Draft the fields, magnetic field first, from
> the public theory; go ahead.

## Unfinished or approximate (known limitations)

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
- **U11. Jumpers' pull: a case brought up to date edit by edit against a
  full solve** (found 2026-10-24 with the edit benchmark): on a real line
  two sections with taut jumpers and their pull have clamps up to 0.17 m
  apart at −20 °C between the two (the same before the speed work of
  2026-10-24). To find out whether the pull's iteration stops before it
  settles or settles where it starts, and to make both give the same
  answer (the full solve's, or the better converged one).
  > **Owner:** (2026-10-24) Unless it can be fixed fast, keep it as it is,
  > but say it in the docs (done: the jumpers' theory, After an edit).
- **U10. Named TIN layers in the thermal limit** (2026-10-23; built
  2026-10-23, in active development, not approved yet): clearance rules
  may name a terrain surface of their own (the clearance windows, the
  Clearances page, `ohl zones`, `ohl line --rules`), and the thermal
  limit (Analysis › Thermal limit, `ohl thermal clearance-limit
  FILE.ohl`) now reads them too; a name the project does not hold stops
  the search with an error.
- **U12. Old names still read** (code review 2026-10-26) — **done:
  removed 2026-10-26 (owner)**, before the first release: the `ols`,
  `ols-cad` and `ols-structure` programs, `import ols` (Python),
  `ols-plugin.toml` manifests, the `ols` extensions and format ids (and
  OLS XML's), `ols://` invitations, `OLS_BUILD_ID`, `OLS_STRUCTURE_APP`
  and the other `OLS_…` variables are no longer built or read. The
  on-disk and protocol names kept on purpose (`ols_meta`, ALPN `ols/2`,
  the storage ids `OLS CAD` / `OLS Structure` and the window storage keys
  `ols-structure-…`) were not part of it and stay.

## Proposed stages (to discuss)

1. **Finish and settle** — the pieces in progress (Next up, above); docs
   checked against the apps (the tutorial, O1, and the folder rename, O2,
   are done).
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
- [x] Sagging outputs (2026-10-22, built, in active development (not approved yet)): stopwatch, dynamometer, transit angle of sight, level sag and inclined-span correction, sag per kelvin, clipping offsets (clipped in at another temperature than sagged) — `ohl sagtension` `[sagging]`, the section window's Sagging panel; see [Sagging outputs](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/sagging.md)
- [x] Sections in the blocks over frictionless sheaves (owner, 2026-10-25 — reversing "not wanted" of 2026-10-22; built, in active development (not approved yet)): the default — the wire's tension equal on both sides of each suspension clamp's sheave in every weather case, each span its own horizontal tension, the strings swinging along the line to balance; the section's length as before (every span at the given tension, strings plumb); the equal-H model the option *simplified (equal H)* (Criteria › Calculation options, `solver.blocks`, line file version 10); the sagging outputs, clipping in and `ohl sagtension` `[sagging]` follow; the ruling-span method keeps one H; every in-the-blocks result changes slightly (the example: H within 0.6 %, sags within 4 cm); validation `sag-004` *(the sheave's size left out)*
- [ ] Sheave friction in the blocks (later): needs a tension ratio per sheave and the pulling end
- [x] Performance: single section re-solve < 10 ms *(~60 µs for 20 spans; loaded span change of state ~1 ms)*
- [x] Validation: CIGRE TB 324 examples, textbook cases, hand calcs (2026-10-20, built, in active development (not approved yet)): 47 cases — catenary and the parabola's error, inclined spans and uplift, Irvine's elastic catenary, change of state (temperature, ice, wind, 150 °C, creep as a temperature), ruling span against the exact section and the FE strings, ice and wind loads, a point load's slope jump, a bimetallic knee point, blowout, the swing of one string and the forces of two strings to one wire point — each with a hand derivation and `benchmarks/reference/textbook_reference.py`; TB 324 sec. 2 (Table 2, dead-end strings), 5.2 and 6.1 *(TB 324's non-linear examples wait for published curves; see [Textbook and published cases](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/validation/textbook.md))*; 2026-10-21: the wire's own weight conserved in every calculation (a heated, crept or stretched span weighs what it weighs when strung; ice and wind per metre of the real conductor), 3 cases (`tbheat-001…003`: closed form, FE and ruling span at 150 °C), tb324-016 within the printed rounding; the weight from the data sheet's reference temperature (a wire type's optional `reference_temperature`, else 20 °C) taken at the stringing temperature, and a stringing temperature of 20 °C for new sections and jumpers (`tbwref-001…002`); the FE's ice and wind per metre of the stretched conductor, iterated with the solution (`tbice-001`; built, in active development (not approved yet)); strain strings with a resolved weight of 0 N keep the estimated weight, and the string catalogue no longer makes 0 N entries

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
- [x] Insulator assembly model (graph of nodes and elements): structure points (fixed), wire points (free), strings as bars between them — one string, two strings to one wire point, a string below two, strain (single and multiple), strings hanging from strings, compression allowed per string (flagged beyond its limit); sections sharing an assembly solved together, Dirichlet–Neumann by the stiffest section (see [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/wirefe.md#strings-hanging-from-strings)) *(post insulators and yokes as presets still to add)*
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
- [x] Weather cases › Merge duplicates (proposed 2026-10-21; built 2026-10-24, in active development (not approved yet)): Criteria › Merge duplicate weather cases… — the cases of the same physics (`WeatherCase::same_physics`) in groups, each merged into the case kept, every name of the others renamed in one undoable command: the project's rule files (`cases`, `case_pairs`, tension rules), load cases (`weather`), galloping files, sheet templates' `[cases]`, the line's creep-stretch cases and the weather cases sent to the structures; the cases shown and the sections' display cases follow *(saved performed checks are not rewritten: records of a run)*. Design-code cases matched to the project's the same way (no case created twice)
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
- [x] Lyre pylons' risers (owner, 2026-10-25; built, in active development (not approved yet)): a jumper given **by its tension** (at its higher end, in still air at its temperature), dead-ended between two wire points, any direction (vertical too): cut from the tension and kept, solved as a taut wire of bars in every weather case (bowing in the wind, its tension following the weather; slack refused); the jumpers at its top and bottom tap onto its points; its pull in the structure loads; clearances and drawings as a jumper; the jumper window's *tension* choice; line file version 10, OHL XML; validation `riser-001` *(post insulators along the riser — its helping strings — later)*
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
- [x] String limits: optional maximum load and swing angles on strings
- [x] String checks (2026-10-24, built, in active development (not approved yet)): Checks › String checks… and `ohl strings` — every string as solved, per rule (`[[string_rule]]` in a rule file; a design-code plugin's preset with `checks = ["strings"]` gives its template): its load against its own maximum load, its swing against its own swing limits and the rule's, the suspension strings' uplift (going slack, or pushed beyond their compression limit); by weather case, kind (suspension, strain) and voltage *(a jumper's helping strings not checked yet)*
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

### Just finished (2026-10-25)
- **Load cases of two parts** in the Load case editor (a window of its
  own): the line's forces with their factors and a tick per attachment
  point, the loads on the structure itself (self weight, wind on the body
  in eight directions, ice, point loads), a sentence saying what each
  case does, Mirror wind.
- **Materials typed** in the cross-section library (U5); the **volumes on
  the strings** drawn in the 3D view.

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

**Status (2026-10-25):** first part in active development — Checks ›
Internal clearances… and `ohl internal`: the conductors near each
structure (within a reach along the span), its jumpers and its strings'
live ends (their wire points, as points) as solved, against every member
of its body (levels 2 and 3), to the member's surface where its solid is
known, else its centre line (said: on the unsafe side); rules
`[[internal_rule]]` (distance fixed or by voltage, plugin presets), the
cases' structure and section overrides.

**Second part built (2026-10-25), in active development, not approved
yet: the volumes on the strings.** Shapes on the insulator assembly
(`.ohli` 4: sphere, cylinder, box, cone; grounded, electrified,
insulated; position, size, offset), a line's string overriding one by
name (line file 8); freedoms (swing across, swing along, twist: free or
blocked — the shapes' orientation; their positions follow the solved
string); several strings in one assembly (number, arrangement, spacing, a
shared yoke). The check: electrified shapes and conductors (jumpers,
string ends) against the grounded shapes and every member, exact on the
convex solids (GJK, a lower bound: conservative); a rule's `shapes` and
`distance_wind` (another distance with wind). Structures › String shapes
and the String library's columns edit them; Structures › Display
options › Volumes on the strings (3D) draws them. Theory:
[Internal clearances](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/internal-clearances.md). OHL-Structure draws
them too (View › Volumes on the strings: in still air, placed by
`ohl-structure-link` as the line places them; owner, 2026-10-25). A
structure type's own strings carry shape overrides too (`.ohls` 7,
`[[string.shape]]`).

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
  in the report with it. Since 2026-10-25 the load cases' wind on the
  body takes the line's member wind (U5's drag and shielding factors);
  only the older form's pressure loads still take the projected area
  without drag.
- **U5. Cross-section library**: sections are now typed in OHL-Structure
  (Sections & materials › Cross-section library: new, edit, duplicate, delete,
  properties computed as typed, undoable), and the member wind takes drag
  and shielding factors as **data** (a `[wind]` table of the section
  library, from a design-code plugin's new optional `structure-wind` or
  typed): OpenOHL finds the lattice faces, their solidity ratios and which
  face shields which member. Still open: no code's factors yet (the code
  plugins return none, so the generic drag stays and nothing is shielded);
  the factors on OHL-Structure's surface loads and materials typed in the
  editor since 2026-10-25; built, in active development (not approved yet).
  *Owner, 2026-10-19:* the choices of U5 confirmed (factors on the FE
  member wind only; only members in a face turned to the wind shielded;
  the face outline as the convex hull of its joints; no factor outside a
  table's range). **Materials typed in the section editor**: built 2026-10-25 (in active
  development, not approved yet): new, edit, duplicate, delete, a rename
  carried to the sections and the default material.
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
- [x] Lattice tower geometry from another program's XML export (joints, members, sections) into levels 2 and 3 *(the private importer, outside the public code; see U2)*
- [ ] Level 2 from LiDAR: a plugin (later); the classes holding structures chosen by the user from all classes, as in the display options
- [x] Clearances to the structure body: none for level 1 (LiDAR points only, separate topic); levels 2 and 3 computed *(O6: the conductors, jumpers and strings' live ends against the members, and the volumes on the strings, 2026-10-25)*
- [x] **Structure application OHL-Structure** (`ohl-structure`, decided 2026-10-17, [spec](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/structure-building.md)): a separate program on the same workspace crates, started from **Edit** on a structure in OHL-Design or by opening a `.ohls`, or on its own (start page, launcher, its own libraries and plugins); its tabs (geometry, body, design, load cases, results, use in the line), undo, what is missing for each level *(the structure report built, its format still to define with the owner (U4); not yet: the mechanism found by a trial solve among the advice)*
- [x] Levels as decided: 1 attachment points only (stick figure, not verifiable); 2 the real geometry (no code check); 3 every part and joint detail, computed (FE) and code-checked (e.g. SIA 263). **Poles** the same: 1 points without body, 2 outside lines, 3 detailed engineering (a wall thickness, or solid). `[[body]]` polylines give level 2 only in 3D. At levels 1 and 2 maximum loads only on the strings, never for the structure itself (its capacity only at level 3) *(the level derived from the content; the code check of level 3 waits for the code-check plugins below)*
- [x] A structure file as a structure family: body and leg extensions chosen per position adapt it to the ground; each placed structure unique to its position (its own name, loads, model, results) — OHL-Structure started for a placed structure works on its own loads and extension
- [x] **Load-case recipes** per structure (`[[load_case]]`, `.ohlslc` or CSV): which line value, factor, one-sided share, on which points or members, surface loads (wind, snow); evaluated by the core with the line's values at each placed structure; the loads in a table, editable (typed values kept over the evaluation), exported and imported *(since 2026-10-25 a load case of two parts — the line's forces, unfactored and a windy case from both sides, with their factors and a tick per point; the loads on the structure itself: self weight, wind on the body in eight directions, ice, point loads — in the Load case editor; the older form still read; [spec](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/structure-load-cases-spec.md))*
- [x] **Cross-section library**: a project library kind the structures refer to (not copied), exported and imported *(base: `.ohlsec` profiles and materials with computed properties; a member's section from the library first, then the structure's tables; the FE check's member wind on the profiles, the wind bands kept out of the model; member areas for surface loads; sections typed in OHL-Structure (Sections & materials › Cross-section library: the library's shapes, properties computed as typed, what is wrong said, each change an undo step in order with the structure's edits; members pick them); wind factors for structures as data (`[wind]`: drag per shape, a lattice face's drag and shielding by solidity ratio and spacing) from a design-code plugin's optional `structure-wind` (WIT `code-wind-plugin`, interface 0.3 unchanged for older plugins; the code plugins' stubs give none) or typed, with the faces, solidity ratios and shielding computed from the geometry (`docs/theory/structure-wind.md`, validation cases structwind-001/002). Since 2026-10-25: materials typed in the editor, and the load cases' wind on the body takes the member wind with these factors. Later: the codes' factors in their plugins (O9))*
- [x] Structure exports: IFC4 (member solids and the analysis model with load cases), IFC2x3 for steel-detailing programs, SAF 2.2 (xlsx) for analysis programs, CSV tables; checked with free tools (IfcOpenShell validation with the schema's rules, geometry and content; SAF sheets, columns and references; `tools/check_structure_exports.py`, test run when `OHL_IFC_PYTHON` is set) and sample files in `examples/exports/`; a tension-only member that bends is exported as a frame with its tension-only behaviour and `check = false` as information (IFC property set, the analysis member's description, SAF layer, CSV columns) *(not yet: the imports tried in another program; other tower programs)*
- [ ] Code checks of members and joints as plugins (e.g. SIA 263), interface agreed with the project owner first
- [ ] **Fully integrated structure editors with a 3D preview** for every detail level (1 stick figure, 2 sticks of the structure, 3 full design), all in one window: geometry, members, sections, materials, supports, strings and wire points, the type's use in the line, checks — lattice towers and **poles of wood, steel (and concrete)** alike *(base: OHL-Structure, with poles in Body › Pole and Design › Pole, sections and materials typed; still to come: checks per level, the use in the line)*

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

### Just finished (2026-10-25)
- **The tutorial** (O1), **structure load cases** of two parts (line
  forces with their factors, the loads on the structure itself; the line
  sends weather cases unfactored, a windy case from both sides),
  **clearances to the structure body** with the **volumes on the
  strings** (O6), **measuring tools and notes**, **plan sets** in one go
  and as one PDF (O14), **another program's project imported into an
  existing one**, **weather case groups** per structure and section,
  plugin presets edited in the project, materials typed (U5), the theory
  pages illustrated, background maps (O16), the broken-wire case (O9) and
  local winds per span (O15).

**Just built** (in active development, not approved yet): the sheave
model for sections in the blocks (the default; equal H an option) and
the lyre pylons' risers (a jumper given by its tension).

> **Owner:**

## Open points — waiting on a decision

### O2. Rename the project folder on disk (done)
`…/Claude/OLS CAD` → `…/Claude/OpenOHL`, as the last step when nothing is
running and the apps are closed (the agents' worktrees and the saved
session notes follow the path).
> **Owner:** Done 2026-10-18 (folder, repository and website renamed;
> formats, crates, plugins and local data renamed ols → ohl).

### O9. Design-code loads still to come
Security / broken-wire and construction loads, and code-specific wind on
structures, in the design-code plugins. The core's **broken-wire case**
built 2026-10-25 (in active development, not approved yet;
[Load cases](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/loads.md#a-broken-wire-casebroken)):
a load case names the section, the span (or every span, one at a time,
the worst of each force kept) and optionally the set (`[case.broken]`,
`.ohllc` 3); the section is solved with that wire gone, its clamps held
by their strings alone (the strings next to the break swing to the spans
left), giving the unbalanced longitudinal loads at the structures
(validation `brokenwire-001…003`, hand-checked); `ohl line
--load-cases`, the Load cases library, and Criteria › Loads for
structures sends such a case to OHL-Structure as a line result
`<weather case> — S3 span 2 broken`. Still to come: the codes' security
and construction cases written by their plugins (the format is
documented for them), a single subconductor of a bundle broken, the
dynamic impact beyond a factor.
> **Owner:** (2026-10-19) All three, each in its respective design-code
> plugin. The core gets a **broken-wire case** type (the section solved
> with the wire gone) for the plugins to use: approved.
> **Owner:** (2026-10-25) The broken wire given per span and set in a load
> case (or every span in turn, the worst kept).

### O11. Validation report
Phase 1 validation against published examples (CIGRE TB 324, textbooks)
is started (2026-10-20, built, in active development (not approved yet)):
31 textbook cases with hand derivations and an independent reference
script, and the 16 TB 324 examples whose inputs are complete
([Textbook and published cases](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/validation/textbook.md), which also
says which examples were left out and why). The cases found that a
heated span weighed more than its wire (the finite elements by
\( e^{\alpha\Delta t} - 1 \), the closed forms with every lengthening);
corrected 2026-10-21 by owner decision: every calculation keeps the
wire's weight (`tbheat-001…003`; [Decision log](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md)).
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

### O17. A database as the model's backbone (standalone server)
In a standalone server mode, the project's model kept in a database
(instead of the project file) through a connection the server holds, so
that other software can read and write the line and OpenOHL fits into a
larger ecosystem (asset management, GIS, planning tools). Stronger
version validation goes with it: every change checked against the
revision it was made from, the model's schema version checked at each
connection, and writes from outside validated as the session's commands
are. Proposed (to discuss; nothing built): the session's objects and
log as tables, the server the only writer, the database chosen with the
owner.
> **Owner:** (2026-10-25) Not now, but in the roadmap: a connection to a
> database as the backbone of the model in a standalone server mode, to
> connect it to existing software and integrate it into a bigger
> ecosystem; it will also need stronger version validation.

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
