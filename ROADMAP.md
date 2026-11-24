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
- [Next, in order](#next-in-order-owner-2026-11-19): the owner's order of 2026-11-19 across both applications
- [Part A — OHL-Design (the line)](#part-a--ohl-design-the-line): open O3, O13–O15, O18, O22–O43; U7, U8, U11, U13, U14 (done: O1, O16, U10, U12); Phases 1–5 for the line
- [Part B — OHL-Structure (the structures)](#part-b--ohl-structure-the-structures): open O4, O7, O8, O10, O12, O19–O21, O44, O45; U1, U2, U4–U6 (done: O5, O6); structure analysis, structures in their own axes, structure–line interaction
- [Part C — Shared](#part-c--shared): open O9, O11, O17 (the country server and OHL-Server), O46, O47 (done: O2); Phase 0, the engine, plugins, formats, server and version control, Phase 6, cross-cutting, further ahead

The phases below record what is built. Phases map to GitHub milestones;
each bullet becomes one or more issues labelled by module (`module:*`).

## Next, in order (owner, 2026-11-19)

Since 2026-11-19 every plan is on this page (owner: "put all different
plans we did in the roadmap … and work from there"): the plans that were
kept privately are carried into their items below, and the work runs
from here. The owner's order of 2026-11-19:

1. **Edits in OHL-Design as fast as on the server** ([O42](#o42-edits-in-ohl-design-as-fast-as-on-the-server))
   and **the tests' speed with the PostgreSQL port flake** ([O46](#o46-the-tests-speed-and-a-flaky-postgresql-test)).
2. **The automatic joint layout on real towers, J1–J2** ([O44](#o44-automatic-joint-layout)).
3. **The joint checks' follow-ups** decided 2026-11-19, then **bolts in
   tension with prying** ([O19](#o19-joint-details-of-towers-show-and-define-them), stage 4.5;
   built 2026-11-19, in active development, not approved yet — the
   layout's "continuous" proposal is [O44](#o44-automatic-joint-layout) J9).
4. **Joint eccentricities set automatically**, steps A1–A3 ([O45](#o45-joint-eccentricities-and-member-shifts-set-automatically);
   built 2026-11-19, in active development, not approved yet — off by
   default; the owner's answers of 2026-11-19 under O45, an offset of
   unknown sign in its member's own check built the same day).
5. **The automatic joint layout J3–J4, then J5–J8** (leg splices and a weld's sides among them) ([O44](#o44-automatic-joint-layout);
   J3–J4, J9 and J7a (leg splices) built 2026-11-19, in active development,
   not approved yet; J7b, a weld's sides, not started).
6. **The country server's phase 6, then 7, …** ([O17](#o17-a-database-as-the-models-backbone--the-country-server);
   phase 6 built 2026-11-19, phases 7 to 14 (rights and roles, country
   libraries, version control at scale, offline working sets, security,
   scale validation) and the rest of the exports' thread with OHL-Design's
   sign-in window 2026-11-20, the web viewer's sign-in, OHL-Design's
   Offline working set window and schema 8 2026-11-21, in active
   development, not approved yet — the owner's questions under O17).
7. **Lattice towers: end releases and bolt slip (queue C8), then the
   validation (C10)** ([O19](#o19-joint-details-of-towers-show-and-define-them), stages 4.4 and 5;
   end releases and bolt slip built 2026-11-23 as options, off by default,
   and the connection capacities' validation the same day, in active
   development, not approved yet — the owner's questions under O19).

The owner's answers of 2026-11-19 to the plans' open questions are in
their items ("decided 2026-11-19") and, briefly, in the
[decision log](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-owner-decisions-on-the-open-questions).
Questions still open are under each item's `> **Owner:**` line.

---

# Part A — OHL-Design (the line)

*The order across both applications since 2026-11-19: [Next, in
order](#next-in-order-owner-2026-11-19).*

**Next up** (2026-11-14): the owner's "to work on next" of 2026-11-10,
O22 to O26 — what is left of each is listed under it. Built since
2026-11-07, all in active development (not approved yet): most of O22
(cable mechanics), O23 (fields' inputs), O24 item 1 and O25 item 1
(2026-11-11); every check window with a user guide page of its own, the
grey lines in its help and the swing arc on by default (2026-11-12); the
field windows in three pages (2026-11-12); one window per plan with its
own sheet template (2026-11-08) and the route plan's paper chosen
automatically (O14, 2026-11-13). Still open beyond O22–O26: O13's
dynamic method, the jumpers' swing band margin (left to the owner,
Phase 4) and the items not built in the record below (`[ ]`). Then, in
the owner's order of 2026-11-14: the post insulators' remaining phases (O40),
the tables' copy, columns and paste, and the equivalence list O27–O39.

## Where it stands

OHL-Design (`ohl-design`) is built as a base through Phases 1 to 5
below, all in active development (not approved yet). The overall state,
the validation count and what was just finished in both applications
are in [Part C](#part-c--shared).

## Open points — waiting on a decision

### O3. Usability review — items not yet decided
From the review of 2026-10-18. Done since and taken out of this list:
tools acting on left-click (since 2026-10-26 always, a right-click opening
the object's menu) and the searchable coordinate-system picker (Phase 2).
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
> **Owner:** (2026-11-02) Let's go with the simplified for now.

*Built 2026-11-02, in active development (not approved yet): the
simplified method — `[[ice_shedding_rule]]` in the rule file (iced
cases, bare case, one span or every span shedding, a **required**
rebound factor, the distance by voltage, the wires), `ohl
ice-shedding`, Checks › Ice shedding… with the 3D layer of the jumped
spans and runs saved in Performed checks; see [Ice
shedding](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/ice-shedding.md).
One span shedding holds its iced tension (conservative); every span
shedding solves the bare case. No rebound factor was found in the
reference documents at hand: 2 by default (the undamped rebound, an
upper bound from mechanics; owner 2026-11-02: to be decided later), a
rule may give its own.
Still open: the dynamic time-domain solve (a second method beside the
simplified one).*

### O14. Situation plans, phase plans and plan sets exported at once
Two kinds of drawing beside the plan-and-profile sheets, and a way to
export a whole set of plans in one go, each kind with templates (built
in, and as plugin presets — a utility's own templates in its plugin).
*2026-11-07 (owner: the plans' templates "similar as for the profiles";
built, in active development (not approved yet)): the plans draw the
template's revision and stringing tables, key map, notes, legend, scale
bar and north arrow placement and pens as the sheets do; the title
block's revision follows the revision table.*
*Built 2026-10-24, in active development (not approved yet): the route
plan, the situation plans and the phase plan (Plans & sheets › Plans, `ohl
drawing --plans`), see [Drawings](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/drawings.md#plans-route-plan-situation-plans-phase-plan);
and plan sets from plugins (Plans & sheets › Plans › Plan set…, `.ohlset`,
the plugin manifest's `[[plan-set]]`); *built 2026-10-25, in active
development (not approved yet):* one PDF of a set (Plan set… › also as
one PDF, `--set-pdf`) and `ohl drawing --set FILE|PLUGIN/NAME`, a set's
second drawing of a kind named apart (`-2`); still to come: links to
public map services, the corridor's protection zones and land use as
their own layers (today: the project's drawings).*
*Status (2026-11-07), built, in active development (not approved yet):*
the [Sheet templates](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/sheet-templates.md)
guide rewritten with a section per part of a template (each key, its
values, default and effect) and drawings made by
`tools/sheet_template_figures.py` from real sheets of the invented
example line, saying which drawing uses which part; two defects found
on the way and fixed: the corridor on the route and situation plans
lost the last piece of the centreline, and the revision table drew the
oldest revision at the top. `{heading}` on the route, situation and
phase plans now says what the plan is (no longer "plan and profile"),
and the scale bars' box in the bottom row grows with its bars.
*Status (2026-11-14), built, in active development (not approved yet):*
one window per plan, each with its own sheet template (2026-11-08); a
plan's own name, name and folder last (2026-11-11); the route plan's
paper chosen automatically — the smallest of A4 to A0 that holds the
route on one sheet, else A0 with the fewest sheets — and a route of its
own, with user guide pages for the route plan and the profile sheets
(2026-11-13). Still to come: links to public map services (also for the
background maps, O16 done), the corridor's protection zones and land use
as their own layers, portrait for the papers chosen by hand.
Proposed (to discuss):

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
  *Built in part (2026-11-04; in active development, not approved yet):*
  the operating current per system (`current_a`, line file 13, Line ›
  Electrical systems column A), the default of the Thermal rating's
  current pages and of the Magnetic field. Not yet: the highest voltage,
  the current from the load flow, phase sequence and transposition.

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
   *Built 2026-10-29 (engine and command line; in active development,
   not approved yet):* `ohl_electrical::electric`, `ohl electrical
   electric`, [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/electric-field.md):
   charges per span \( q = P^{-1} V \) (earth wires and voltage-free
   circuits at zero), 3D from the solved wires and jumpers as pieces of
   uniform charge in closed form, the ground a plane fitted under each
   point (owner's question "what's easier and actually better", answered,
   to confirm), 2D line charges; profiles, points, cuts, volumes. The
   window in OHL-Design *built 2026-10-29 (in active development, not
   approved yet)*: Checks › Electrical › Electric field… (voltages per
   circuit, nominal by default, 0 kV voltage-free; 3D/2D, jumpers, ground
   plane radius, profiles, limit, points; the cross-section panels and
   volumes on the layer *Electric field (isolines, surfaces)*; inputs kept
   in the project). Not yet: isolines from above, a published profile in
   the validation, the structures and objects (not modelled, said).
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
     *Field presets built 2026-10-31 (in active development, not
     approved yet; owner: "implement in the core what's required for the
     building plugin"):* a plugin's optional `field-presets` export
     decides from the points the host gathers (the manifest's
     `[field-input]`: LiDAR class roles, the ground thinned, survey
     codes, a reach) and the solved model where cuts (with a label) and
     points are needed (cuts at alignment stations, 2026-10-31); the core
     runs the magnetic or electric field — or both in one run, `ohl
     electrical fields` — there and gives the results back as one
     `ohl-field-results-set` JSON for the plugin's report files (`ohl
     electrical magnetic|electric|fields --preset-plugin`, [plugins](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/plugins.md#field-presets)).
     *The building plugin built 2026-10-31 (in active development, not
     approved yet; owner: "go ahead"):* `plugins/examples/building-cuts`
     clusters the building points (LiDAR role, survey codes), gives per
     building the governing and nearest-point cuts and the places of
     sensitive use per floor, and reports the buildings' summary (both
     fields' columns), the cuts' CSV grids and 1:1 SVG drawings per
     field, the places and a GeoJSON ([user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/analyses.md#building-cuts)).
     *Field plan templates and the report one cut at a time built
     2026-11-02* (in active development, not approved yet; tried on a
     real line, local): `.ohlfp` (title, logos, point classes, TIN or
     drawing in the cut, limits and isolines, wire colours, the weather
     per system), `ohl electrical … --plan`, `--system-case`.
     The field presets in OHL-Design: Checks › Electrical › Buildings
     (fields)… (2026-11-02); the `cross-sections`
     export for noise methods: not yet.
7. [ ] **Corona and surface gradient** in the core or the plugin: the
   conductor surface gradient per subconductor (Markt–Mengele or a charge
   simulation), the onset gradient (Peek) and the margin, per section —
   to decide whether it belongs to the plugin above.
   *The surface gradient built 2026-11-04, 2D only, in active
   development, not approved yet* (owner: "yes go ahead with 1-4 but only
   for the 2D cuts for the noise do not apply it in 3D as these are
   different usage"): in the core (`ohl_electrical::surface_gradient`), a
   charge simulation of every subconductor and earth wire over level,
   perfectly conducting ground (8 line charges at 0.3 r by a convergence
   study, 360 points round each); per span at its lowest point and both
   ends; per bundle the average maximum, the maximum and Markt–Mengele's
   value, kV/cm rms. Checks › Electrical › Electric field… (2D, *Surface
   gradient*) and `ohl electrical gradient`. Validated on closed forms
   (0.03 %) and against hvlbuzz (within 1e-4; 4e-6 converged),
   `electrical-066…073` ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/surface-gradient.md)).
   Not yet: the onset gradient (Peek) and margin, the noise itself
   (item 6), a published worked example, Markt and Mengele's original
   publication.

*Status (2026-11-07), built, in active development (not approved yet):*
the Electric field and Magnetic field windows share what they had word
for word in one module (`field_common`: the setup's inputs and how they
are kept in the project, the span and profile setup, the profile plot,
the notes of a run); each window keeps its own physics. Meant to change
nothing the user sees; the project keys are unchanged.

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

### O22. Cable mechanics: further models
From a review of the cable mechanics on 2026-11-10 (the models offered,
against what line design practice uses). In the order proposed:

1. **Say the bimetallic model on the results.** A bimetallic conductor's
   aluminium takes compression without limit by default: above the knee
   the wire keeps the composite's expansion, which gives the **largest**
   hot sag (safe for clearances; the tension above the knee is then about
   1 % low). The other models — the aluminium going slack (bird cage) or
   a compression limit — give smaller hot sags: not on the safe side for
   clearances. No result says which model a conductor has: say it (the
   wire types and sections concerned; "not on the safe side" on the two
   other models, and on the tension of a case past the knee under the
   default), and put both in the safe-side list. *(Corrected 2026-11-10:
   this item first said the default gave the smaller sags; the
   bimetallic benchmark and the theory's source show the opposite.)*
   **Built 2026-11-11, in active development (not approved yet):** the
   notes per line (`ohl line`, `ohl tensions`, the zone check, `ohl
   internal`, thermal, `ohl sagtension`), the flags per weather case on
   the spans past the knee point, the theory section, both rows of the
   safe-side list, the benchmark `tbbim-003`. Still to do: the Sections
   table's cell and the hovers in the app.
2. **Strain strings as hanging cables.** A strain string is a straight
   bar with half its weight at each end. On a tight span that is a
   millimetre; on a slack span (a few kN, a substation span with heavy
   strings) the string itself sags by decimetres and the chord shortens by
   centimetres. A string as a heavy cable element (its own catenary).
3. **Sagging at a final condition and by catenary constant.** A section
   is strung at an initial tension. Wanted: the tension (or the catenary
   constant, or a sag) given at the state after creep or after load, the
   initial stringing worked back from it; and sagging found automatically
   against tension limits.
   **Built 2026-11-11, in active development (not approved yet):** the
   stringing given after creep or after load (`"condition"`) and by
   catenary constant (`"kind": "catenary_constant"`), line file 15, the
   initial tension worked back and said beside the value (`ohl line`),
   the section form and the Sections table, the benchmarks
   `finstr-001…004`; sagging to tension limits as a command (`ohl
   tensions --sag-to-limits`, `--write`), tension rules by catenary
   constant or tension (rule file 2), the benchmarks `saglim-001…002`.
   Still to do: the window's "Sag to these limits…", the initial tension
   in the Sections table.
4. **Wind on wires with a direction and with height.** The wind is one
   pressure, square to every span. Wanted: a wind direction (the part
   square to each span acting, with the part along it — a load case with
   the wind along the line loads the wires at angle and dead-end
   structures, which cannot be said today), the pressure growing with
   height above ground, gust and span factors as a design code gives
   them (the factors themselves from the code plugins), ice growing with
   height.
   **The direction built 2026-11-15 and 11-16, in active development
   (not approved yet)** (plan of 2026-11-15, its 15 questions answered as
   recommended; owner: "the current cases need to be identical", "add an
   option for the friction on the wire"): a weather case's wind direction
   (where it comes from, 0° from ahead, 90° from the right, relative to
   the alignment at each place; spans next to an angle point their own
   segment's direction, strings and bodies the bisector), a load case's
   own direction or a set of directions (the worst kept), the projected
   area by default with the cross-flow law and a friction along the wire
   as options, the vertical part on inclined spans, the structure bodies
   in their axes, the member wind's inclination factors (Part B, U5), the
   views by angles; every case without a direction bit for bit as before
   ([Wind on the line](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/wind.md)).
   Still open: the pressure growing with height, gust and span factors,
   ice growing with height; each element's own direction in the span
   load (planned, not built); a wind band's area along the line (one area
   today, said not on the safe side); compass bearings; a direction per
   span or structure (owner, 2026-11-15: not now).
5. **A section left in the blocks, the other model.** A section not
   clipped in rolls over frictionless sheaves in every weather case. The
   other common model fixes the wire lengths at equal tension at the
   sagging condition and lets the strings swing: offer it as a choice
   (tension to 0.6 %, sag to a few centimetres, either way per span).
6. **Creep at elevated temperature**, for conductors run hot.
   **Built 2026-11-11, in active development (not approved yet) — the
   mechanism only:** a law as data of the wire type (`[cable.hot_creep]`,
   wire library 3), a time at high temperature with the line
   (`creep_stretch.hot`, line file 15), strain hardening between stages,
   the lengthening after creep by ruling span and finite elements, the
   notes (the user's coefficients and their source; "not on the safe
   side" without a law), benchmarks `hotcreep-001…003` with invented
   coefficients. **Blocked on sources:** no law and no coefficient is
   shipped until the owner supplies Harvey & Larson (IEEE Trans. PAS-89,
   1970) and CIGRE Electra 75 (1981); the family of forms is to be
   confirmed against them. Still to do: `ohl sagtension`'s final states,
   the Creep-Stretch window's table, the wire form.
7. **Smaller:** a partial break of a bundle (some subconductors); point
   loads in a span with a part across or along the line; gap-type
   conductors; a share of a span's pull per set where one attachment
   point carries several sets (today per point).
   **Built 2026-11-11, in active development (not approved yet):** the
   share per set at a shared point (the line sends each set's part of
   the pulls, line values 7; `sets` with `back` / `ahead` scales those
   sets only); the partial break of a bundle (`subconductors = k` of a
   load case's broken wire, load case file 5: the broken ones gone from
   the span, the tensions per wire left; the form's field still to do).
   Point loads across or along (`force_across`, `force_along` on a
   span point's object, line file 15, finite elements; the ruling span
   says the point loads it leaves out; benchmarks `ptforce-001…002`; the
   object form's fields still to do). Gap-type conductors
   (`bimetallic_behavior = "gap"`, wire library 3: the outer strands
   unloaded in each section's stringing state, the knee point its
   stringing temperature; said "not on the safe side" on every result;
   benchmarks `tbgap-001…003`) — **the mechanism only: no primary source
   on gap-type conductors is in hand**, the theory cites none and the
   model is to be checked against one.

Three hand-sized comparison cases to keep with the benchmarks: three
inclined spans in the blocks, clipped, and in the blocks then clipped; a
slack span with heavy strain strings; ice given as a load with wind.
(Structures giving under the wires' pull: O7.)
> **Owner:** (2026-11-10) To work on next.

### O23. Fields and line parameters: inputs and outputs
From a review on 2026-11-10.

1. **Line parameter totals by the wire's length.** The totals of a span
   are the values per kilometre times the span's horizontal length: low
   by the sag (0.1–0.3 %, said in the theory) and by the slope, which the
   theory does not say (1.5 % at 10°, 6.4 % at 20°). Take the wire's
   length, or say it.
   *Status (2026-11-11), built, in active development (not approved
   yet):* the totals over the wires' length as solved (sag and slope
   in), said in the results, both lengths in the table and the CSV;
   the values per kilometre are per kilometre of wire. Open: the
   jumpers' own length at strain structures; the wires' spacing square
   to a sloping span.
2. **A current per wire.** One balanced current per circuit is given.
   Wanted: magnitude and angle per phase (unbalance — far from the line a
   net current governs the magnetic field, so balanced-only input is not
   conservative there) and an earth-wire current typed instead of
   computed.
   *Status (2026-11-11), built, in active development (not approved
   yet):* a circuit given per phase (magnitude and angle, its net
   current said) and earth wire currents typed, in the window and on
   the command line; a result with a balanced circuit says "NOT ON THE
   SAFE SIDE far from the line". Open: per phase together with the
   currents at the end points; a typed earth wire current per span.
3. **The electric field's major axis and vertical component**, beside the
   rms of the vector (the magnetic field has both already).
4. **The voltage of the electric field**: the nominal voltage is taken;
   offer the highest operating voltage, and say which was used.
   *Status (2026-11-11), built, in active development (not approved
   yet):* a system's optional highest operating voltage (the user's
   value) and insulation level (high unless given; read by no check
   yet) in the line file and the systems table; the electric field and
   the surface gradient take the highest operating voltage where
   given, else the nominal with "NOT ON THE SAFE SIDE", and always say
   which voltage was used. Open: the two values in the plugin
   interface.
5. **Other lines in the same field run** (a parallel or crossing line of
   the project).
6. **Smaller:** milligauss beside microtesla; line parameters per unit on
   a base power.

Five comparison cases to keep with the validation (a single circuit on a
level span, magnetic and electric; with and without an earth wire and a
bundle; an inclined span; a double circuit with two phasings; line
parameters on a level and a steep span).
> **Owner:** (2026-11-10) To work on next.

### O24. Clearances: the zone down to the ground, and reports
From a review on 2026-11-10.

1. **A vertical rule "down to the ground".** A vertical clearance rule
   looks above (or below) each point within its band in plan. A wire
   passing *beside* a tall object, below its top, is then not found by a
   vertical rule alone — on the unsafe side unless the class also has a
   radial rule or a screen. Wanted: an option of the rule making the
   point stand for the whole height from the ground up to it (a building,
   a mast: the zone is the band in plan, from the ground to the point
   plus the vertical distance), the default for the classes it suits to
   be decided.
   *Status (2026-11-11), built, in active development (not approved
   yet):* the rule option `to_ground` (the Vertical clearances window's
   choice, `ohl zones`, `ohl line`); a vertical point rule without it, a
   radial rule or a screen for its classes says "NOT ON THE SAFE SIDE"
   in its results. Its default (owner, 2026-11-11: "use the same as for
   the TIN; if none then mention it"): a rule that does not give it
   takes it when all its classes are classes the terrain surface is
   built from (the class scheme's ground role), said in the results;
   mixed classes or no terrain class known: not taken, said.
2. **A horizontal-only measure between wires** (the distance in plan,
   whatever the height difference), beside the radial and the vertical.
3. **A printable clearance report**: per finding a cross-section at its
   place (the wire positions checked, the target, the required zone),
   beside the tables, the CSV and the drawings.
> **Owner:** (2026-11-10) To work on next.

### O25. Vegetation and danger trees: questionable trees, work sites
From a review on 2026-11-10.

1. **Trees without ground nearby.** A vegetation point with no ground
   within the search distance is left out of the fall-in check and only
   counted. List them as *questionable* (where they are, why), so that
   none is silently unchecked.
   *Status (2026-11-11), built, in active development (not approved
   yet):* listed in the result, the panel, the CSV and a saved run
   (those within the tallest tree's reach of a span; at most 10 000,
   the nearest first; the counts stay). Open: drawing them in the
   views.
2. **Grow-in with a horizontal and a vertical distance.** Grow-in is one
   radial distance. Wanted: separate distances across and up (a rectangle
   round the wire), as clearance rules have, beside the radial one.
3. **Work sites.** Findings grouped into sites (neighbouring points as
   one area with its size and its worst finding), for the people who cut.
4. **Export for maps**: the findings and sites as KML and shapefiles.
5. **Decide the default of the ground stopping a falling tree**: the
   trunk rests on the ground today (the physical choice, said in the
   report); the full circle of the tree top flags more below steep
   slopes and is the conservative choice.
> **Owner:** (2026-11-10) To work on next.

### O26. Sheets: free annotation, tables and pages
From a review on 2026-11-10 of what a sheet can hold.

1. **Free annotation on a sheet**: lines, arrows, clouds, dimensions and
   texts placed by hand on a given sheet (a remark tied to an object
   needs a CAD program afterwards today), kept with the project.
2. **Report tables on a sheet**: any table of results (clearances, loads,
   a bill of materials of the sheet's structures) placed as an inset,
   beside the revision and stringing tables there are.
3. **Title, notes and appendix pages** in a set of sheets, and several
   station ranges per route.
4. **A material list per structure** in its text block.
5. **The true plan** (not straightened along the alignment) above or
   below the profile as a choice, and a plan detail inset.
6. **DXF beyond R12**: line weights, true colours, embedded logos.
> **Owner:** (2026-11-10) To work on next.

### Equivalence with the established line-design program (2026-11-14)
From a chapter-by-chapter review on 2026-11-14 of the user manual of the
line-design program most utilities use today, against OpenOHL (the
review itself is kept privately; the earlier reviews of 2026-11-10 gave
O22–O26). Owner, 2026-11-14: "close all the gaps … to ensure
equivalence" — after the post insulators and the tables' copy, columns
and paste. Ranked by what would keep a utility engineer from switching;
sizes: S about a session, M two or three, L four or more. Known items
already on this roadmap are named, not repeated: wind with height and
direction (O22.4), strain strings as hanging cables (O22.2), the
clearance report (O24.3), sheets (O26), structure flexibility (O7),
equipment loads (O21), member code checks (O4), the validation report
(O11), translations (Phase 6).

### O27. Spotting by station
1. **A structure placed by station and offset** on an alignment, moved by
   station; **copies** of a structure, and copies every so many metres
   from one station to another. (M)
2. **A structure put into a strung span joins its wires**: a suspension
   set clamps the wire in, a dead-end set splits the section in two —
   today the strings are picked again by hand. (M)
> **Owner:** to decide (2026-11-14): spotting where? The plan and the
> profile take no tools (2026-11-01): a station window with the 3D view
> snapping to the alignment, or one spotting tool in the profile with the
> clearance line, or both.

### O28. Survey points with feature codes
A survey point's code says what it is — ground, an obstacle (a road, a
building, a crossing line) with its height and the clearances it needs,
an aerial obstacle — its symbol and label in the profile and plan. Today
the codes are kept but **every survey point is ground**: a coded crossing
is a bump in the terrain, not a target with its own clearance. Codes as
targets of the clearance rules, a survey-point clearance report,
obstacles in the clearance line and in spotting. (M)
> **Owner:** to decide (2026-11-14): the codes as classes with roles, as
> the LiDAR points have, so one rule set serves both?

### O29. Existing lines from LiDAR
1. **The as-built fit written into the model**: per span the wire length
   that gives the surveyed sag at the survey's temperature, kept with the
   survey's date and temperature (a section "calibrated to the survey");
   today the fit is reported only. (M)
2. **A line built from the scan**: wires, structure tops and attachment
   points found in the cloud, catenaries fitted, structures and sections
   proposed. (L)
> **Owner:** to decide (2026-11-14): 1 as proposed? 2 wanted, or are
> existing lines brought in by import?

### O30. Construction outputs
1. **Stringing charts** for chosen sections, structures or circuits over a
   range of temperatures — initial or final, sags, tensions, catenary
   constants, wave times — as a report (today: one temperature per
   section in its window, a range through `ohl sagtension` only). (M)
2. **Clipping offsets for one pull** through several sections or a range
   of structures. (M with 1)
3. **Staking, further**: reference stakes (left and right, ahead and
   back), hubs, guy anchors with lead lengths, offsets at angle
   structures, a sheet per structure. (M)
4. **Leg extensions and guy lengths** from the terrain: each leg's length
   to the ground along its slope, the leg extension to choose. (M with 3)
> **Owner:** to decide (2026-11-14): which setting-out conventions (the
> stakes' layout) to follow.

### O31. Material management
Parts and assemblies on structure types and placed structures; a bill of
materials per structure; each item's status (new, existing, moved,
removed) with a bill per status and the difference between two designs;
on the sheets with O26.4. (L)
> **Owner:** to decide (2026-11-14): a parts library of OpenOHL's own, or
> left to the utility's ERP through the exports? Status per object, or
> two project lines (as built, as planned) and the difference?

### O32. Units other than SI in the application
Every view, table, report and export in US customary units on request
(today SI everywhere; US output in `ohl sagtension` only). (L)
> **Owner:** to decide (2026-11-14): is a North-American audience a goal?

### O33. Automatic spotting, second version
Several weather cases (clearance, uplift); clearance to obstacles (O28,
LiDAR classes), not only under the centreline; insulator swing limits;
required structures and positions (every angle point); extra-cost zones;
constraints from drawings and terrain slope; tension sections and ruling
spans iterated until stable; a report of what governed. Then a **pole
selector** (the lightest pole class for each place) and the **choice of
body and leg extensions**. (L + M + M)

### O34. Reports as documents
A line summary (criteria, statistics, structures, attachment points with
mid-span and low points and the ground under them, sections with sagging
data, wire lengths, circuits), a criteria report, structure heights with
a flag above a limit (aviation marking), printed or PDF, and any report
to KML or shapefile. With O24.3. (M)

### O35. Stations: start station, equations, reversed alignments
An alignment starting at a given station, station equations (breaks in
the stationing) shown in the profile, sheets, staking and reports, and an
alignment reversed. Existing lines are known by their stations. (S)

### O36. Distribution and joint use
Messenger wires with cables lashed to them (the ice on each cable),
stringing a bare messenger first; span guys and down guys per pole;
standard pole-top assemblies combined on a pole; an arm turned square to
a wire. (M–L)
> **Owner:** to decide (2026-11-14): distribution in scope, or
> transmission only?

### O37. Survey and terrain housekeeping
Survey points edited, turned off and on, filtered on import, codes
changed inside an area; survey points from DXF, shapefiles and KML;
shapefile backgrounds and map services (WMS); contour lines of the
terrain; a terrain from a profile only (old paper profiles); an arbitrary
profile between two points. (S each; the stringing direction is drawn
already.)
> **Owner:** to decide (2026-11-14): map services and web maps send
> requests to outside servers — allowed (the user's own server, off by
> default) or kept out?

### O38. Load cases by structure groups
Built 2026-11-15 (in active development, not approved yet): Line ›
Structures › Load cases per group — groups picked or by rules (types,
line angle, station on an alignment, dead end), a structure taking the
union of its groups' cases (in none: every case); the per-structure and
per-type sets read into groups; see the user guide's *Load cases per
group*; a ground elevation rule and a load case's own list (picked per
position where the case is defined) built 2026-11-15. From the plan of
2026-11-14 for load cases between the two applications (approved with
its recommendations; the owner's model: the line transfers what the
wires do, the structure side decides what acts — construction stages are
OHL-Structure load cases): phases 0 to 3 built 2026-11-14 and 11-15 (one
user guide page, never silently absent, one name one meaning, the local
conditions reaching OHL-Structure, one way in — Loads for structures
gone —, the groups). Still open:
- a rule on the height; the design-code plugins tagging their cases so a
  group takes cases by tag (S);
- phase 4: an extra (own) OHL-Structure load case applying to every
  structure of a type or to one placed structure only, labelled so, the
  latter kept with that structure in the project (owner: both) (M);
- phase 5: the line's usage table showing OHL-Structure's last result
  per structure, dated, marked out of date when the line changed (S–M).

### O39. Also found, smaller
A background "design OK" flag per structure and section while editing;
a temperature typed for a section's display without a weather case;
reports to KML; draped orthophotos in 3D. (S each)
Further, from the same review, not ranked: photos per structure; the
loss of strength by annealing (needs a primary source); a departure-angle
report; a clearance-to-ground contour map; the thermal rating's wind
turned per span from one direction, and its results written back as
display temperatures; statistics of a table's selection and filling cells
as a spreadsheet does (with the tables' copy and paste); a cost factor
per site, and an automatic spotting taken into the project as a project
line; whole-structure interaction diagrams and capacities per component
(arms, braces) in the line's structure check; report layouts the user can
change; a display by catenary constant or swing angle; two attachments of
a strung section swapped. (S each)

### O40. Post insulators and rigid connections
From the plan of 2026-11-14 (owner: "put post insulators on structures
or on the ground so that we can make connections to a rigid piece").
Built, in active development, not approved yet
([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/post-insulators.md)):
- [x] 1 — a rigid jumper's wind in its end forces (2026-11-14; its shape
  stays straight).
- [x] 2 — posts in the insulator library (ratings as optional limits),
  fixed and braced (hinged) posts on structures, their load to the base
  with its moment, a jumper leaving a post's top at a fixed exit angle,
  the ratings in the string check (2026-11-14); a pivoting post held by
  what is attached to it and free when nothing is (owner, 2026-11-15),
  an optional hinge friction.
- [x] 2c — posts on structure types and in OHL-Structure: a fixed post's
  moment sent with its force, a hinged post's forces only (2026-11-14).
- [x] 3 — the Add post insulator tool, its shapes, tables and guide
  pages (2026-11-14).
- [x] 4 — a post on the ground as a post support, a preset (2026-11-14).
  [ ] The pedestal as a structure type of its own (its weight and wind on
  the foundation: today said NOT ON THE SAFE SIDE).
- [ ] The post's deflection (a spring at its top from the data sheet's
  deflection) and its own ice.
- [ ] 5 — a rigid conductor (tube or bar) on posts: its own section, wind
  and ice on it, held at its ends and along it by posts (a continuous
  beam, its reactions to the posts), its deflection against an optional
  limit, thermal expansion (one support fixed or all, the safe side said),
  jumpers tapping onto it, drawn as a tube; posts along risers. (L)
- [ ] 6 — later: short-circuit forces on rigid conductors and posts (the
  standard's document needed first; no number from memory).
> **Owner:** (to answer, 2026-11-14) 1. The case in mind: a line entry
> or terminal structure (conductor, post on the structure, tube to a
> sealing end), or a busbar on posts on the ground? It decides the order
> of 4 and 5. 3. On the ground: is the pedestal part of the post support,
> or a structure type of its own? 4. The rigid piece: a tube library, or
> tube dimensions given on the jumper? 5. Short-circuit forces needed?
> Then the standard's document first. 6. In OHL-Design now, or ground
> posts and busbars left for OHL-Substations? (Question 2, a rigid post
> by default: built so; the deflection not yet.)

### O41. LiDAR editing tools
From the plan of 2026-11-14 (owner: "make a plan of tools to edit the
lidar … mass selection based on a surface … delete what's in or out of the
selection … also a duplicate detection then I'll validate it before
moving forward with it"). **Nothing built: it waits on the answers
below.** Every tool is a selection, then an action. The selection is a
description (areas, a height band, classes, a corridor, combined) that
reaches every point of the cloud inside it, not only those drawn; the
action is previewed (counts per class, the time) and applied as one undo
step in the background with progress and Stop.
- Selecting: A1 an area clicked in the 3D view from above (owner,
  2026-11-14: "we could do in 3d from above" — the camera turned straight
  down when the tool is taken; the plan and profile keep no tool), the
  points inside or outside; A2 an area from a DXF polyline, a GeoJSON or
  shapefile polygon; A3 a corridor from the alignments or the conductors;
  A4 a height band above or below a surface; A5 classes, intensity,
  return number; A6 box and lasso reaching every point, not only those
  drawn; A7 add, subtract, intersect, invert; A8 saved selections.
- Acting: B1 change the class (the 10⁶-point limit lifted by batches);
  B2 delete; B3 crop; B4 export to LAS/LAZ; B5 shift heights (optional);
  B6 count and report.
- Cleaning: C1 duplicates — exact (same stored coordinates and class),
  same place with another class, near within a small tolerance (reported
  only: overlapping flight strips are real data); removal keeps one point
  by a rule (the first imported by default); C2 isolated points to a
  noise class (the criterion from its primary source); C3 points below
  the ground; C4 coverage gaps along the corridor; C5 overlapping tiles
  found at import (add all, skip duplicates, replace the area).
- Underneath: a deletion command in the session, a large action split
  into commands grouped as one undo step and sent through the server;
  `ohl pointcloud select | delete | crop | dedup | report` with the
  selection as a small file; one LiDAR editing window (Select, Action,
  Preview, Apply); the tools in the Terrain theme; a theory page.
- Phases: 1 deletion and undo, the limit lifted (M); 2 A1, A4, A5, A7, the
  window, B1–B3, the preview, the theory (L); 3 C1 and C5 (M); 4 A2, A3,
  A6, A8, B4–B6 (M); 5 C2–C4 and the command line (M).
> **Owner:** (to answer, 2026-11-14) 2. Delete: *withheld* (the LAS
> flag: the points stay stored, drawn and checked as gone, undo is a
> flag; a later "Purge withheld points" removes them for good —
> recommended), or removed at once (each point kept in the log for undo:
> a large log)? 3. Duplicates: the tolerance for "near" (5 mm?), the
> point kept (the first imported?), same-place points of another class
> deleted or only reported? 4. Isolated points (C2) now or later?
> 5. Saved selections for everyone in the project, or per user?
> 6. Order: deletion and the area first, or duplicates first?

### O42. Edits in OHL-Design as fast as on the server
Owner, 2026-11-19: "And the check for the server per member can it also
be applied on the app itself? So that the process is as speedy" — "per
element modified instead of validating the whole thing". **Decided
2026-11-19, first in the [order](#next-in-order-owner-2026-11-19); items
1, 2 and 4 built 2026-11-19, in active development, not approved yet**
([performance](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#ohl-design-an-edit-taken-in-place-o42-2026-11-19),
[decision log](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-o42-and-o46-edits-in-ohl-design-in-place-the-tests-postgresql)):
the app's line is taken in place after a command the session applied in
place (on the whole synthetic country 0.85 ms instead of 587 ms an edit),
made whole only for a settings or library change, the session's whole
way or a line replaced meanwhile (said in the edit's report), proved on
random command sequences. **The solve after an edit built 2026-11-19, in
active development, not approved yet** (owner, 2026-11-19: the solver's
internals may change, the results bit for bit the same;
[performance](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#the-solve-after-an-edit-o42-2026-11-19),
[decision log](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-o42-the-solve-after-an-edit-the-writers-lock-released)):
a solve's lookups worked out once instead of searched through the whole
line for every section and jumper (`ohl_line::lookups`), the route views
and the nearest route segment found by place, the sections of an update
solved over the cores — on the 2 000-pylon synthetic country the case's
solve after an edit in OHL-Design 0.35 s instead of 3.0 s, opening it
33 s instead of 21.5 minutes; the whole country (12 000 pylons) opened
in 3.6 minutes (before: not finished), an edit's solve 1.4 s; proved
against the searches on the examples and a synthetic country. Left:
still once per update over the whole line — the route views, the
jumpers' estimated loads, the lookups, each jumper's inputs compared,
the solved shapes copied — about 0.1 s on 2 000 pylons, more than half the
1.4 s on 12 000: to follow the edit as well (the route views kept and
changed only where a structure or an alignment changed).
Item 3 (views) stays as it is (owner, 2026-11-19): the first frame after
an edit 57 ms on a 500-pylon country. The session already applies a command in place and validates
only what it touches and what refers to it (the country server's phase 3:
a move on the 12 000-pylon synthetic country in about 2 ms). After every
command the app then rebuilds its own line whole — a clone of the line,
every structure type resolved, the reference state checked over the whole
line — before it re-solves the touched sections.
1. Measure the app's edit, split (session, clone, resolve, reference
   check, solve, views), on the examples and the synthetic country.
2. Update the app's line in place from the command's applied changes:
   the changed objects replaced, structure types resolved only for the
   structures changed (and those whose type or library changed), the
   reference check only for what changed; the whole rebuild kept where it
   is needed (a library change, the creep-stretch, bimetallic or
   reference-state switches, a library's undo) and said. Proof: random
   command sequences — the line kept in place equals the line rebuilt,
   and the solved cases are equal.
3. Views: only the changed objects' chunks rebuilt, if measured costly.
4. Kept: only the cases shown are solved at once, the others after the
   settle (owner, 2026-11-18).
Quality before speed: no computed result coarser, no file changed.

### O43. The swing band, jumpers and spans the wind lifts — what is left
From the reviews on a real line of 2026-11-16 and 11-17 (kept
privately). Built, in active development, not approved yet: the swing
band with levels, at least one every 200 Pa (within 0.063 m of the 10 Pa
steps on that line, inside the 0.15 m margin; the old band was up to
0.69 m outside); spans the wind lifts to or through their chord found
and checked at their 10 Pa steps; each distinct solve made once in a run
(cases differing only by their pressure share their solves: 69 to 84 %
fewer solves there); stiff jumpers solved along the rising and the
falling wind, a second still-air shape said, swaps between the band's
pressures solved every 10 Pa
([clearances](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/clearances.md),
[jumpers](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/jumpers.md)).
Still open:
- Jumpers: the margin 0 stays NOT ON THE SAFE SIDE (a jumper about a
  decimetre outside its band at a low pressure from one side); a
  jumper's other still-air shape (a loop stable to either side) is said,
  not checked — adding it as a position is the owner's choice; a jumper
  with two shapes 0.2 m apart: a 10 Pa path, or a margin; the jumper
  paths' cost (6 to 10 times a windy solve's jumper part).
- A run's bands depend on which case is solved first (a span's axis
  through a dead-end clamp as first solved); the wire positions are exact.
- The clearance windows and `ohl zones` only say NOT ON THE SAFE SIDE for
  lifted spans (their states are fixed before the solve): their steps
  would need a second solve pass.
- The 0.3 mm per pascal of spacing behind the 200 Pa rule is measured on
  one real line and two example lines: a measured bound, not a proven one;
  winds at an angle above 1000 Pa and the ice series not measured again.
- A 500 Pa band takes minutes on a long real line (its slowest sections);
  a few sections fail some steps at high pressure along the line (one
  node held by slack strings converges slowly): each worth a look.

### Done and taken out of this list
Built, in active development (not approved yet); the details are in the
user guide, the theory and the [decision log](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md).
- **O1. Tutorial "Start from scratch"** (2026-10-25): eleven chapters in
  `docs/tutorial/`, Help › Start from scratch in both applications, each
  chapter's steps run by a test (`crates/ohl-app/src/tests/tutorial.rs`).
- **O16. Background maps imported into the project** (2026-10-25): an
  image layer is an orthophoto or a map (Terrain › Backgrounds › Import
  orthophotos or maps…), shown on the plan, the sheets and the plans,
  the maps first on the route and situation plans and the key map; links
  to public map services stay with O14.

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
  own). *2026-11-18 (owner), built, in active development, not approved
  yet:* in a placed structure's Edit window a wire point dragged turns its
  string about its attachment point (the length kept; held by several
  strings it moves only where every length holds, else it is refused and
  said), an attachment point dragged moves everything hanging from it;
  the strings come from the string library, their lengths read-only
  there, set by two angles; a click in the view selects the row; an edit
  solves the cases shown at once and the others later; Apply reaches the
  main views; *Put the project's strings in a library*, a one-off command
  for existing projects.
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
- **U13. Galloping check, left from its review** (2026-11-15; the faults
  fixed, the open points made safe by the owner the same day, see the
  decision log). Still open: the window runs in a weather case the
  parameter file does not name (said before Run) while the command line
  refuses it — align them?; pairs across adjacent spans or to another
  line's span are not compared (a simplification, said) — wanted?; the
  command line numbers spans from 0, the window from 1.
- **U14. Smaller items left from the work plan of 2026-11-10**: the
  profile sheet's side lines under the lowest arm, to confirm by the
  owner — the 0.5 m level tolerance, the wire's end height (not the arm's
  attachment) deciding the lowest level, the span's first structure
  deciding, the outermost by the mean offset of both ends; the field
  volumes not reused between runs (needs volumes per span); `ohl drawing
  --name` (the windows have a name since 2026-11-11); the thermal limit,
  ice shedding and lightning in the app read a vertical rule without
  `to_ground` as off, not by the terrain classes' default the clearance
  checks take (to look at); a plugin's own column for the insulation
  level stays until the plugin interface passes the core's value (O23.4).
  The version-management points of that plan: Part C, O47.
- *Done and taken out:* U10, named terrain surfaces in the thermal limit
  (2026-10-23); U12, the old `ols` names no longer read (2026-10-26; the
  on-disk and protocol names `ols_meta`, ALPN `ols/2` and the storage ids
  stay on purpose).

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
- [x] Creep models: creep-curve method (10-year curve); the linear wire models take creep as a given equivalent temperature shift (`linear_creep_temperature`, and `linear_creep_tension` scaled by the creep case's tension; see [Sag-tension](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/sagtension.md)) — *still open (corrected 2026-11-07): a time-based creep law, and working out the equivalent temperature instead of typing it (GitHub issue #12, "Creep: a time-based law")*
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
- [x] Multi-span sections solved in 3D (FE with suspension strings, wind per span, point loads); `ohl line` with 3D HTML viewer and CSV exports — compared with an existing line exported from another program (930 spans, median 1.1 cm) *(2026-11-06, built, in active development (not approved yet): the dynamic relaxation fallback no longer stops after 5 s of wall-clock time, only by its step count and the budget's time limit and cancel token, so its result no longer depends on how busy the computer is)*
- [x] Insulator behaviour in the 3D solve: a string's swing, the forces of two strings to one wire point & slack detection, strain assembly geometry *(from the FE solve; closed-form checks as validation cases since 2026-10-20: `tbins-001…005`, `tbrs-002…003`, `tb324-011`)*
- [x] `ohl-clearance` v1: vertical ground clearance of every solved span per weather case, violations in `ohl line --terrain --clearance` and in the viewer
- [x] `ohl-clearance` v2 (DESIGN_GOALS §13): vertical **and** radial clearances; rule files (`.ohlr`) per target — ground, LiDAR classes/roles, other lines, other wires of the same line (same span and between spans) *(2026-11-04, built, in active development (not approved yet): every step of a run counted with its own time left, Stop answered at once, the corridor test of the points faster with the same findings; 2026-11-06: the radial clearance to the ground kept within the reach — the requirement plus `search_margin`, a wire below the ground always counting — as the points already were, and `ohl zones` writes its progress every 30 s)*
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
- [x] Tools act on left-click (an option from 2026-10-19, always so since 2026-10-26): a left-click uses the tool held (Select held: it selects), a right-click opens the object's menu, in the 3D (every scheme), plan and profile views; drags unchanged
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

- [x] `ohl-loads`: load case framework (weather × load factors × wire condition), `.ohllc` files kept in the project, edited in the app *(2026-11-03, owner's local-conditions and load-case design; built, in active development (not approved yet); added to this page 2026-11-07: `.ohllc` version 4 — removed conductors apart from broken ones, several broken wires solved together, the structure wind's direction, ice and snow on the structures; the Load cases library as a form; a load case set per structure (its own, else its type's, else every case); one **Use local conditions** switch per check window in place of the weather groups' override ticks; OHL-Structure takes a placed structure's set from the library; see the [decision log](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md), 2026-11-03)*
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
- [x] Danger trees (2026-10-20, built, in active development (not approved yet)): fall-in about the root ball (hinge at k·h', resting on the ground along the fall direction, sheltered by ridges; tree base only with a ground point within a given distance), grow-in with buffer against every case solved left and right, the optional swing arc per span and unbalanced ice per span near vegetation; per span the worst with case and position; isoclearance lines (GeoJSON/DXF/CSV, drawings layer `OHL_ISOCLEARANCE`, app layers in 3D and plan); parallel and bounded (`ohl lidar vegetation --root-ball --fall-allowance --buffer --swing-arc --band-margin --unbalanced-ice --isoclearance --max-base-ground-distance`; Checks › Vegetation; validation `dangertree-001…005`, `swingarc-001…004`). Since 2026-10-20 the unbalanced ice is a weather case of its own on the span (`--unbalanced-ice case|thickness=m|extra=N/m|load=N`; the wind on its iced diameter) and the defaults are the utility practice (root ball 0.05, allowance 2 m, base ground point within 5 m, search distance 50 m; the command line's results with the defaults change). Since 2026-10-21 a cross-section view of the line (built, in active development (not approved yet)); the check's grow-in and fall-in points in it still to come. *2026-11-05 (owner), built, in active development (not approved yet): a falling trunk stops at the ground (said in the theory as not conservative against a rule that ignores the ground); a switch lets it turn on through the ground — the vegetation window's "the trunk turns on through the ground", `[vegetation] fall_through_ground`, `--fall-through-ground` — off by default*
- [x] Thermal rating: IEEE 738, CIGRE TB 601 / TB 207, max operating temperature from clearance — `ohl-thermal`: ampacity, temperature, transient, solar position; `.ohlta` ambient cases; per-section clearance-limited temperature (ground and point rules); `ohl thermal`, Checks › Thermal rating *(base: uniform conductor temperature, no radial gradient; no published TB 601 example in the validation yet)*
- [x] Lightning shielding (2026-10-31, built, in active development (not approved yet); added to this page 2026-11-07): `[[lightning_rule]]` in the rule file (shielding angle, or rolling sphere with a typed striking distance or a formula of the current), cross-sections square to each span, the margin; `ohl lightning`, Checks › Electrical › Lightning shielding with the 3D layer of the exposed parts; validation against the classical construction ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/lightning-shielding.md))
- [x] Thermal rating at a current (owner, 2026-11-04 — *built 2026-11-04, in active development, not approved yet*): a system's operating current in the line file (version 13); Checks › Thermal rating in four pages with Setup and Results — Ampacity, Temperature at a current (steady, per ambient case, heat terms on hover), Step change (the transient as a table and a plot), Clearance (the clearance-limited temperature, or the clearance at the current's temperature: pass or fail per section, drawn in the views); `ohl thermal clearance-check` *(the bundle shares the current equally; the steady temperature, not the transient's, in the clearance check)*
- [x] Optimum (automatic) structure spotting — cost-based optimisation: dynamic programming over candidate stations × types/extensions (ground clearance of a design catenary, allowable wind/weight spans, alignment points, forbidden zones); `ohl spot`, Tools › Spot structures (proposal as a new line file) *(clearance under the centreline only, one tension for the route)*
- [x] Galloping ellipses, blowout, phase-to-phase / circuit-to-circuit clearances — base: ellipses by user coefficients (`.ohlg`; Davison / RUS / CIGRE TB 322 forms, no coefficients shipped), swept along the span, per span pair (`ohl clearance galloping`, app window); blown-out wires against the right of way (`ohl clearance row`); `circuits = same|different` on wire rules with case pairs
- [x] Drafting: plan & profile sheets, title blocks, DXF/DWG-compatible output, PDF (`ohl drawing`, Plans & sheets › Plan and profile sheets): one format-free drawing model written as SVG, R12 DXF (sheets in mm, plan in project coordinates, profile by station × exaggerated height), vector PDF (a page per sheet) and PNG *(base: no DWG written — the R12 DXF imports into DWG-based programs; DXF has ACI colours, no line weights, SVG logos as outlines, no raster logos; PDF uses the standard Helvetica fonts)*; since 2026-10-20 the title block, page and styles from sheet templates (`.ohlsheet`, plugin presets), built, in active development (not approved yet)
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

**Next up** (2026-11-19; the order across both applications: [Next, in
order](#next-in-order-owner-2026-11-19)): O4's design codes are out of
the core (SIA 263:2013 and ASCE 10-97 bundled plugins, 2026-11-18); the
pole codes follow when a pole export exists. O19's buckling lengths, the
joints' own checks and the weight and quantities are built (2026-11-19);
next, in the owner's order: the joint checks' follow-ups and bolts in
tension with prying (O19), joint eccentricities set automatically (O45),
leg splices and a weld's sides (O44), then end releases and bolt slip and
the validation (O19). Open besides: O20's B3 to B5; O21 (equipment); the
hotbar's drawing and the check footer into the shared UI crate (Part C).

## Where it stands

### Level 4 and the joints in the calculation (2026-11-11)
Built, in active development (not approved yet):
- **Level 4, "full design with joint details"**: a level 3 tower whose
  every checked member end has its joint detail. The structural
  calculation stays in its **simple form** by default (connections not
  taken); the option *Joint details in the calculation*
  (`joint_details = true`, structure file version 11) takes them, at
  level 4 only — set below it, not applied and said. Results of existing
  files with connections change to the simple form (intended); the
  results say which form was used. The joints' own checks (plates,
  covers, welds), drawings and exports do not depend on it.
- **Usability pass on OHL-Structure**: the header counts only what
  blocks a calculation (*ready to calculate*, *optional: level 4, N
  advice*); fewer default tabs so every title shows; notices above the
  tool hotbar; the Inspector's fields beside their labels; the joint
  workflow walked step by step in a window test (undo and redo exact).

### Just finished (2026-11-07)
All built, in active development (not approved yet):
- **Joint details** (O19): plates several per joint, welded plates, a
  member's roll, weld size and length and the plate checks, a plate's
  outline corrected by hand, the values of a joint drawn and explained
  (2026-11-03 and 2026-11-04); the members' profiles in the 3D view as
  the Detail view places them, lap joints, splices with covers (two
  shear planes) and continuous members, ends seated without bolts, bolts
  on both legs, bolts in the 3D view (2026-11-05).
- **Member capacities**: SIA 263:2013 beside ASCE 10-97, torsional and
  torsional-flexural buckling in the ASCE 10-97 check, bolt tools that
  keep the design code's minimum distances; steel pole shafts by
  ASCE/SEI 48-19 or SIA 263, wood poles by SIA 265 (2026-11-03).
- **Load cases**: a structure's set from the Load cases library
  (2026-11-03, Part A Phase 4); OHL-Structure's menus aligned with
  OHL-Design's (2026-11-03); a row double-clicked zooms the 3D view on
  it (2026-11-04); `double_channel` sections (2026-11-04).
- **One UI crate** shared with OHL-Design, `crates/ohl-ui` (2026-11-07,
  Part C).

## Open points — waiting on a decision

### O4. Code checks of members and joints (e.g. SIA 263)
*Status 2026-11-11: the plugin interface for the members' check
(`member-check`, every member and load case in one call, run by
OHL-Structure, `ohl structures fe` and the line's structure check), the
joints' (`joint-check`: plates, covers, welds, bolt groups) and the
bolts' minimum distances (`bolt-distances`), with an example plugin, are
built, in active development (not approved yet). 2026-11-18 (owner
2026-11-17: "yes go ahead with your recommendations"): **SIA 263:2013
moved out of the core** into the bundled plugin `sia263`
(`plugins/codes/sia263`; members, plates and welds, bolt distances),
behind a further export `member-check-full` (everything the core's check
read, the plugin's parameter file `Plugins/sia263.toml`); its numbers
bit for bit the core's (303 557 member results compared before the core's
copy went); `capacity = "sia263-2013"` an alias, no file changed; the
plugin checks take the worse of with and without foot movements; Member
details shows a plugin's capacities; the bundled plugin ships with the
applications, an error at start when it does not load. Second round, the
same day (owner 2026-11-18): **ASCE 10-97, the default, moved out of the
core** into the bundled plugin `asce10` (`plugins/codes/asce10`; members,
plates, bolt distances), its numbers bit for bit the core's (4 487 212
member results and 9 452 plate results compared before the core's copy
went); no `capacity`, or `capacity = "asce10-97"`, its method, no file
changed; a plugin's parameter file reaches its joint checks
(`joint-check-full`) and Member details too; the bundled plugins have 120 s
and 4 GiB a call (user plugins 5 s, 256 MiB). Third round, the same day
(owner): the bolt tools' force basis each code plugin's own
(`bolt-distances-full`; SIA 263's bolt capacity smaller than ASCE 10-97's,
said); default bolt minimums where no code gives any (1.5 d end and
edge, 3 d spacing, editable, said "assumed, not a code's"); Member details
following the plugin's parameter file; provisional bolts keeping a tower
at level 3 everywhere. Next: the pole codes when a pole export exists.*

A plugin interface for code checks: what the core hands over (member
forces, buckling lengths, sections, joint details), what comes back
(usage per member and joint, formulas). To be agreed before it is built.
> **Owner:** (2026-10-19) Can be done in parallel with other work. First
> code: SIA 263 for steel. Members first, then joints. The theory of the
> checks goes in a theory page of the book.

ASCE 10-97 (the default, 2026-10-27,
[theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/asce10.md))
and SIA 263:2013 (`[analysis] capacity = "sia263-2013"`, 2026-11-03,
[theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/sia263.md))
are the bundled plugins `asce10` and `sia263` since 2026-11-18; the
joints' plates and fillet welds are checked by the structure's capacity
method's plugin (O19).

### O19. Joint details of towers: show and define them
A tool in OHL-Structure to see and define, at each joint of a tower:
- the members that arrive and from where;
- each member end's condition (onto the leg or a gusset plate, the leg
  bolted, eccentric, restrained);
- the bolts (how many, where).

Also: the structure's kind (pole or tower) editable as a toggle.

The checks would then take their conditions from the joints instead of
flags typed per member: ASCE 10-97's slenderness curves and connection
capacity per end, buckling lengths from what braces a member, optional
eccentricities, releases and bolt slip in the FE model, the joints'
own checks (with O4), the bolts' and plates' weight.

Six stages, the questions per stage and per detail level:
[Joint details of towers](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md).
- [x] Stage 0 — Geometry › Structure kind (2026-11-02)
- [x] Stage 1 — `[[connection]]` and `[[plate]]`, structure file version
  9; the CSV export (2026-11-02)
- [x] Stage 2 — Design › Joint details: close-up, face view (SVG, PDF),
  the connections table with where each value comes from; View › Joints
  by their details (2026-11-02)
- [x] Stage 3 — defining joints in the application: every value edited
  in the table, the bolt layout window, copies to the joints like it and
  to a group or class, presets (`Connections/*.ohlcp`), plates (2026-11-02)
- [x] Where a member lands (its steel's cut, a sideways shift) and the
  Detail view's tools (edit, add and remove bolts, member end, plate
  around the bolts) (2026-11-02)
- [x] Member details: a member's two ends edited from the member, its
  capacities live, its leg laid flat in the Detail view (2026-11-02)
- [x] A bolt library (`.ohlb`, `Bolts/` beside the structure), imported
  and exported; bolts drawn with head and nut, the members seated leg on
  leg in the drawings (2026-11-02)
- [x] Stage 4.1 and 4.2 — the ASCE 10-97 check takes each end's
  eccentricity and restraint (given, the member's, worked out at an end
  with a connection, assumed) and its bolts (a library bolt's strengths)
  for the bolt shear, bearing and net section, the weaker end governing;
  a tower without connections unchanged; `ohl structures fe --bolts`
  (2026-11-02)
- [x] Stage 4.4, eccentricities — a connection's `shift` as a rigid end
  offset in the FE model: the moment carried where a frame member meets
  the joint, left out and said where only bars do; reported per member
  and case; validation `structfe-015` (2026-11-02)
- [x] The ASCE 10-97 connection rules on the safe side (no end restraint
  from bolts onto a gusset plate only, punched holes, block shear, the
  short-leg rule, up to three gauge lines) and bolt tools that keep the
  design code's minimum distances (snapping onto gauge lines, breaking
  bolts ringed with the clause) (2026-11-03)
- [x] Plates: several per joint, bolted or welded, drawn round their
  bolts with the code's end and edge distances in the plate
  (2026-11-03); a welded plate as several plates welded together, a
  member's roll (Lay flat, Turn it flat), gauge lines from the holes
  (2026-11-04)
- [x] Weld size and length (`[[plate.weld]]`) and the plate checks per
  case — bearing, block shear, the section across the bolt group, end
  and edge distances, fillet welds by SIA 263 (welds not checked under
  ASCE 10-97, said); the joints marked by their plates' worst usage
  (2026-11-04)
- [x] A plate's outline corrected by hand in its own plane, back to
  automatic, corners dragged in the face view (2026-11-04); what the
  values of a joint mean, drawn and dimensioned above the connections
  table and in the user guide; a plate in a bolt's grip drawn as a
  solid; joints like it copying the end's stops, shift and rolled edge
  (2026-11-04)
- [x] The members' profiles in OHL-Structure's 3D view as the Detail
  view places them (seat, cut, roll, shift), View › Display options ›
  Profiles as placed (2026-11-05)
- [x] Lap joints, double covers and continuous members (owner,
  2026-11-05): a connection's `face` (outside, inside, nested with its
  fit), `[[splice]]` covers giving two shear planes and checked as
  plates for their share by area, continuous members as one part
  (Design › Continuous members…, never set by itself); bearing on the
  leg bolted onto; `splices.csv`, `parts.csv`, IFC covers
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-laps-splices.md))
  (2026-11-05)
- [x] Ends seated without bolts, bolts on both legs (`leg = "both"`,
  `other_leg_points`; the net section counting both legs' holes), bolts
  in the 3D view (2026-11-05)
- *The open-items queue of 2026-11-18* (owner: "Go ahead"), in its
  order: **A, the safe side first** — 1 a weld's moments about its seam,
  2 a shift where only bars meet, 3 a gusset plate in compression, 4
  tearing towards a plate's side edge and a frame member's end shear and
  moments in the plate checks (all built 2026-11-18, below); **B**
  buckling lengths (stage 4.3, built 2026-11-19); **C** — 6 the joints'
  own checks (stage 4.5, built 2026-11-19), 7 weight and quantities
  (stage 4.6, built 2026-11-19), 8 end releases and bolt slip (stage 4.4,
  open), 10 the validation (stage 5, open).
- [x] Stage 4.3, buckling lengths from what braces a member (2026-11-19,
  built, in active development, not approved yet): the core's runs and
  bracing points, each code plugin's rules; given ratios still win, each
  result says worked out, given or assumed
  ([ASCE 10-97](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/asce10.md#buckling-lengths-from-what-braces-the-member)).
  Decided 2026-11-19: SIA 263's strict reading stays the default (a joint
  held only across the direction of buckling does not hold it); ASCE
  10-97's practice only as an option marked "not in SIA 263's text"; the
  ASCE "panel" taken as the stretch between one face's bracing points
  (accepted); at a run's end where every other member there lies in one
  line or plane, the end is held only in that plane (the safer variant);
  the 20 % crossing rule kept also for given lengths. To build: the
  option for SIA 263 and the run ends held in one plane.
- [x] Stage 4.4, end releases and bolt slip as options of the FE model,
  measured before any default (queue C8) — *built 2026-11-23, in active
  development, not approved yet*
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/structures-fe.md#end-releases-and-bolt-slip-from-the-joints-bolts-options),
  [decision](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-23-structure-analysis-end-releases-and-bolt-slip-as-options-o19-stage-44)):
  both **off by default**, stored in no file (OHL-Structure's Results ticks
  for the session, `ohl structures fe --end-releases --bolt-slip`). **End
  releases**: a frame end with one bolt pinned, one with a single row of
  bolts in one leg free out of that leg's plane (condensed out of the
  element), read from the connections, else from the members' bolts where
  the end does not run on in line; **bolt slip**: each bolted end slips by
  its hole clearance before its bolts bear (friction not counted), secant
  passes. Where a joint would be held by nothing or the solve finds a
  mechanism or buckling mode, the options are taken back there, said. Every
  run with an option solves each case without it too and says the effect.
  Measured on 72 real towers (one case, kept privately) and two invented
  ones: the releases change little (the top's displacement and a tower's
  largest usage by about 0.1 % in the median, single members up to 2.4
  times), the slip much (the top's displacement about 2.9 times, a tower's
  largest usage +15 %, most bracing relieved and the legs loaded more, the
  solve 16 times slower); runs fail with them on 3 (releases), 6 (slip)
  and 10 (both) of 72 towers. Alone neither is on the safe side for every
  member; the tower program's results match the model without them.
  Validation `structfe-017` to `structfe-020`. **Follow-up built 2026-11-24**
  (the owner's answer to (2) below; built, in active development, not
  approved yet;
  [decision](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-24-structure-analysis-the-joint-options-never-lower-a-usage-o19-stage-44-follow-up)):
  with either option on, each member's and joint check's usage is the
  larger of with and without it (the solve without it, already made for the
  effect, now used; said "without the joint options" where it governs; the
  effect line counts the members each solve governs); each joint's larger
  displacement; each support's reaction component by component the larger
  in magnitude (a sign flip said not on the safe side); OHL-Structure and
  `ohl structures fe`. Neither option can lower a usage now. Measured on
  the 72 private towers: with the slip 68 % of the members are governed by
  the solve without it (up to +47 points on relieved bracing), a tower's
  largest usage raised on 13 of 66 towers (at most ×1.29); with the
  releases 2 092 members raised by more than a point, the largest usage on
  14 of 69 towers (at most ×1.08). Still as before: a case failing with the
  options fails; the stiffness for the line at levels 3–4 without the slip.
  The towers where the take-back fails: (5) below.
  > **Owner:** (questions, 2026-11-23) (1) Keep both off by default? →
  > yes: the releases change little for the time they cost and fail on a
  > few towers; the slip is a different model of the tower, not a
  > correction. (2) With the slip on, each member's usage the larger of
  > with and without it (as the foot movements), so the slip never relieves
  > a member? → yes, build it before the slip is used in design. (3) A
  > friction branch (no slip below the bolts' slip resistance) from a
  > measured source? → yes, later, with the source's numbers read from the
  > paper (to be checked; not in the repository's documents). (4) Store the
  > options per structure (a file field, version bump)? → only once (1)–(2)
  > are decided. **Built since:** (2), 2026-11-24 (above).
- [x] Stage 4.5, the joints' own checks: the plates', covers' and fillet
  welds' (2026-11-04, above); the safe-side items of 2026-11-18 — a weld's
  moments about its seam (SIA 263; no weld rule in ASCE 10-97, said), a
  shift where only bars meet carried into the member's check, a gusset
  plate in compression as a strut of Whitmore's width (an assumption on
  the safe side, said), tearing towards a plate's side edge and a frame
  member's end shear and moments in the plate checks; the bolt groups
  under the members' forces together, the leg's bearing and a usage per
  joint in the tables, the report and the 3D view's joints (2026-11-19,
  [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-groups.md)),
  all built, in active development, not approved yet.
  Decided 2026-11-19 and built the same day (step 3 of the order;
  built, in active development, not approved yet;
  [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-groups.md)):
  a gusset plate's own bolts onto the leg in the file format
  (`[plate.leg_bolts]`, structure file version 20) and checked; a joint
  that does not fit (a bolt's hole off its flange, a holding bolt outside
  the plate, holes overlapping, a bolt beyond a member's known end) reads
  "does not fit", coloured as failing and counted apart, never an
  infinite usage; in-line pieces not marked continuous end at the joint
  (on the safe side); the bolts in tension checked, with prying (the
  T-stub model of Struik and de Back, α = δ = 1; ASCE 10-97 4.3.3, 4.3.4;
  SIA 263 (77), (78)); the elastic method for the moments stays. Still
  assumed or not checked, said: a member's own gauge eccentricity in its
  plane as a moment on its bolts, the plies' offsets in the levers, the
  joint's own loads through its plates, the holding bolts of a welded
  plate's several plates (a plate's own prying and bending under bolts in
  tension: built since). Decided 2026-11-19 and built the same day (built, in active
  development, not approved yet): a **bolt grade set by command**
  (OHL-Structure's Design › Bolt grade…: the members picked, a group's or
  the whole tower; SIA 263:2013 Table 2's strengths, recorded in the
  structure file as set by command with their source, structure file
  version 21; no default grade — a bolt without strengths stays said not
  on the safe side; [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/bolt-grade.md));
  a bolt library's `tension_capacity` defined as the bolt's ultimate
  tensile load F_u × A_s; **a gusset plate's own prying and bending** by
  the same T-stub model, the plate a flange without a web (the additive
  plugin interface `joint-check-groups-prying`), no longer said not
  worked out. **The joint layout proposing "continuous"** for in-line
  pieces, for the user to confirm: built 2026-11-19 (O44 J9), in active
  development, not approved yet. To build: a window for a plate's own bolts onto the leg
  (typed in the file until then), and those bolts drawn in the 3D view,
  the close-up and the IFC export (counted in the bill of materials
  already).
  Decided 2026-11-19 (owner: "Okay go with recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-owner-22-recommendations-approved)):
  a gusset plate's prying lever stays measured to the farthest bolt (B6);
  no "web" credit for members bolted across a plate unless a source is
  cited (B7); bolt grade 8.8 in the current line's bolt library (B8, done
  in the private data, with its rebuild step); the NOT ON THE SAFE SIDE flag
  for a moment about the line of a plate's holding bolts stays (B9); **a
  "does not fit" only on placements assumed** (not given by the user, not
  laid out by the layout) reads "does not fit as assumed — not detailed",
  still failing, counted apart in the run's summary, the report and the
  3D view (orange) — built the same day, in active development, not
  approved yet (D14); J9 proposes "continuous" for same-section pieces in
  line, the user confirms (a "confirm all" button), never a silent
  default (D15); the mutual-host guard with J9, not before (D16) — both
  built 2026-11-19 (O44 J9). On the
  78 real towers (kept privately), one case: as imported 23 128 of 33 544
  joints do not fit, 22 993 only as assumed; with the layout 17 821 of
  55 970, 11 057 as assumed — not detailed, 6 764 as given.
- [x] Stage 4.6, weight and quantities (2026-11-19, built, in active
  development (not approved yet)): a tower's members, gusset plates,
  splice covers, bolts and welds counted from the connection records and
  weighed, a heavy and a light count; the bill of materials in
  OHL-Structure (Design › Weight & quantities), as CSV
  (`bill_of_materials.csv`), XLSX and in the structure report; where
  every joint is defined a run's self-weight is, until the counting is
  validated, the larger of the counted mass and the members × the
  dead-load factor where the weight loads the tower and the lesser where it
  holds it (`self_weight_from`, structure file version 19: `"factor"`, or
  `"counted"` alone, not on the safe side)
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/structure-weight.md)).
  Decided 2026-11-19: the typed factor stays the floor until the counting
  is validated (built); a typed allowance per structure for galvanising
  and fittings (no default number: the note stays until one is given);
  provisional bolts block the switch to the counted mass; a record for
  fillers and packs, together with the layout's filler-plate question
  (O44); OHL-Design keeps the typed factor until the counting is
  validated; a per-case "weight favourable" mark in the load-case recipe
  (the light count for it). To build: the allowance, the provisional
  bolts' block, the fillers' record, the favourable mark.
- [x] Stage 5, exports — the bolts and gusset plates in IFC4 and IFC2x3
  (a joint assembly per joint), the members seated as the Joint view
  draws them (2026-11-02)
- [x] Stage 5, validation — connection capacities against hand
  calculations and a tower program's results (on the real towers, kept
  locally); the theory page (queue C10) — *built 2026-11-23, in active
  development, not approved yet*: `benchmarks/structures-fe/joint-capacities.toml`
  (`joints-001` to `joints-014`, `ohl validate`): invented bolt groups and
  plate parts sent to the bundled plugins as a run sends them (calculations
  `structures.joint_groups`, `structures.joint_parts`) — bolt shear in one
  and two planes, bearing, end and edge distances, prying (the flange's
  bending and the bolt's tension governing), block shear between two planes
  and towards a plate's side edge, the Whitmore strut, fillet welds with
  their moments about the middle and about the seam (SIA 263), SIA 263's bolt
  shear (73) and (74) — against hand calculations written out in
  `benchmarks/structures-fe/reference_joint_capacities.py`: all agree to
  10⁻⁹, no discrepancy between the theory pages, the plugins and the hand
  calculations. Against a tower program's capacities of the real line's
  towers (163 690 members, kept privately): bolt shear and bearing agree
  within 1 % on 99.3 % of the members it governs, the net section is 1.3 %
  lower (the punched hole, on the safe side); 100 members are higher than the
  program's (validation pages:
  [textbook](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/validation/textbook.md#connection-capacities-benchmarksstructures-fejoint-capacitiestoml)).
  > **Owner:** (questions, 2026-11-23) (1) 64 members whose design data
  > gives no bolt or hole are checked on their gross section, where the
  > tower program takes a net section 20 % lower: take a hole per leg until
  > the bolts are known? → yes, on the safe side (said). (2) A built-up
  > member's bearing at member level counts both its angles (2 × t), the
  > program one (36 double angles, up to 1.8 times ours against it); the
  > plate between them is checked only where the joint's plate is defined:
  > take one ply's thickness at member level until the plate is known? →
  > yes, on the safe side (a plugin change: both codes' plugins).
> **Owner:** (2026-11-02) "a roadmap for a tool to show and allow to
> define the joints in the OHL structure in details for a pylon that
> isn't a pole … where the bars are coming from, what's their condition
> on the joint and where are the bolts, how many. And then how this is
> expected to impact the conditions on the calculations later on."
> (2026-11-02) "Go ahead with 0-2, yes and yes" (both questions of the
> detail levels: yes). "Go ahead with the next stage" (3). "A bolt
> library could be nice to have too so that it can be easily added and
> imported/exported." (2026-11-02) "Go with … C" (stage 5, exports).

### O20. Built-up members: two angles back to back, in a star, four in a cross
Members made of several profiles: shown as their parts and calculated
with the rules for built-up members. Today they are `general`
cross-sections of typed values, drawn as boxes. Five stages and the
questions: [Built-up members](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/built-up-members.md).
- [x] B1 — built-up cross-sections in the library, calculated from their
  angles: `double_angle`, `star_angle`, `cross_angle`, the gap required;
  each angle's own properties kept; validation cases `sections-001…003`
  (2026-11-02, built, in active development, not approved yet)
- [x] B2 — importing them: the local importer of a tower program's
  export maps them, the root radius from the area, the gap fitted to
  the export's second moments (within 5 %, else kept `general`, said);
  its bolts as the project's bolt library (library kind `bolts`)
  (2026-11-02, built, in active development, not approved yet)
- [x] `double_channel`: two channels back to back, its properties from
  the outline (2026-11-04, built, in active development, not approved
  yet)
- [ ] B3 — shown: each angle drawn, the joints see the plies
  *(status 2026-11-07: since 2026-11-05 OHL-Structure's 3D view and the
  joint close-up draw a built-up member by its built-up outline (O19,
  the profiles as placed); since 2026-11-10 each part drawn and the
  member view for every section, since 2026-11-11 braces seated and cut
  against built-up legs of angles; the joint details taking a built-up
  member's plies and filler plates: not built)*
- [ ] B4 — the rules for built-up members
- [ ] B5 — joints of built-up members *(owner to decide, from the work
  plan of 2026-11-10: plates on built-up legs (it moves the plate checks),
  built-up members as the arriving end, ends onto channels and I
  sections)*
> **Owner:** (2026-11-02) "plan the double star and 4 corners to be
> shown or at least calculable". "Go with A" (B1 first).

### O7. Structure–line interaction, levels 3 and 4
Level 3: a stiffness matrix at each structure's suspension points from
level 3 structures. Level 4: structures and line iterated until both
converge. Order and timing to decide.
> **Owner:** (2026-10-19) Straight to level 4: level 3 is a simplified
> version of 4 and can easily be derived from it afterwards.
> Also needed (possibly with O7): **anchoring (guy) cables** in the
> structures.

**Where it stands (2026-11-16):** plan approved 2026-11-15 (level 3 first,
level 4 on the same loop). Built, in active development: phases 0–5 —
the structures condensed at their attachment points, the interface loop,
calculation level 3 in the structure loads (never below rigid), and the
line's setting (Line › Criteria › Calculation options, line file version
21, `--structures-as`) taken by the structure loads, the wire tensions,
the string checks and the loads sent to OHL-Structure; phase 5b (owner
2026-11-16) — a **reference state of the structures**: a weather case
solved once at level 3 and kept with the line (version 22), the
structures drawn deflected in it and every level-2 solve taking their
points as moved there while it is current (the LiDAR comparison by
default); then (owner 2026-11-16: towers built from their plans, moved by
the measured movements of their feet and the reference case's lean) a
placed structure's **foot movements** (version 23): prescribed support
displacements in the structure engine, taken by the reference state,
level 3, the level-2 solves without a reference state (the feet alone)
and the structure checks (each member's larger usage of with and
without), sent to OHL-Structure; phase 6 (2026-11-16) — **calculation level
4**: from level 3's solution each flexible structure solved in full in every pass (P-Delta,
tension-only members and guys; the structures side by side, the checks
once at the end), a structure failing there taken linear and reported
failing, the reference state solved at level 4 when the line's setting
is full structures. Next: the clearances taking the level (after the
swing-arc band work); levels 3 and 4 linearised at / started from a
current reference state (not built).

The plan of 2026-11-15 ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/structure-flexibility.md);
owner: "keep the larger of rigid and flexible, otherwise your
recommendations"; the structures solved side by side with a progress bar,
the code checks once on the converged state), its phases:
- [x] 0 the theory page; 1 the structures condensed at their attachment
  points; 2 the wires' stiffness and moved attachments; 3 the interface
  loop and level 3; 4 level 3 in the structure loads (the larger of rigid
  and flexible kept per structure, point and case); 5 the line's setting
  and the checks taking it; 5b the reference state and the foot
  movements; 6 level 4 (2026-11-15 to 11-16), twice as fast on a real
  line with the same digits (2026-11-17); 8 measured on a real line
  (kept privately, 2026-11-16 to 11-18).
- [ ] 7 completing: the internal clearances with the deflected body; the
  clearances taking the level (rigid today, said); OHL-Design updated
  near moved structures only; a level per structure group and per check;
  springs per attachment point at level 2 (only if wanted); a fixed
  post's tilt; levels 3 and 4 linearised at or started from a current
  reference state.
- [ ] Smaller, from the notes: foot movements are not carried by OHL XML;
  a twist about the vertical is not in the format; the condensation's
  note "offset taken as the joint's translation only" fires for the
  millimetre offsets of imported points rounded to centimetres (could be
  quiet below a tolerance); a tower model unstable at the reference state
  is taken rigid (said).

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

### O21. Equipment on a structure: permanent loads
A structure type has no place for a piece of equipment fixed to it (a
switch, an antenna, a platform, a cable end): its weight, its wind area
with a drag factor and its ice area, at a joint, acting in **every** load
case. A type's own `[[load]]` only acts in the model's own case, not in
the cases built from the line's forces. Wanted: a permanent load of a
structure type (weight; wind area and drag, taking each case's wind on
the structure; ice area, taking each case's ice), shown and edited in
OHL-Structure, counted in the loads table and the reports. Until then a
structure carrying equipment is checked without it — on the unsafe side,
to be said by whoever models it.
> **Owner:** (2026-11-10) Agreed in principle; kept on the roadmap for
> now, not built yet.

### O44. Automatic joint layout
From the plan of 2026-11-18 (owner: "a plan for an automatic setting on
the joints … so that you can click on a button and it makes the
necessary eccentricities and creates the plates on the joint so that it's
physically doable … the optimal bolts and bolts placement we'll do this
later with the codes"; approved the same day with every recommendation).
Built, in active development, not approved yet
([user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/joint-layout.md),
[theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md),
[joint details](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md)):
- [x] Stages 1 to 6 (2026-11-18): the 3D clash check at joints; the plane
  layout engine (concentric first with the ends pulled back, a sideways
  shift up to a limit only where that fails, else *needs a hand detail*
  with the reason — never forced); the plates; the app (Design › Lay out
  joints…, a preview, one undo step, the *automatic* marks, copies to
  similar and symmetric joints, a laid-out joint laid out again by a later
  edit in the same undo step); the effect and the docs; measured on 78
  real tower files (kept privately). Bolts provisional until the codes
  choose them; a tower with provisional bolts stays level 3.
- [x] Orient angles from the tower's shape, a member turned only where it
  helps, a faster follow (2026-11-18); tuned on real towers to the
  owner's order of steps (2026-11-18 and 11-19): everything pulled back;
  the biggest profiles first — one part when the same and aligned (1a),
  nested (1b), else a plate; the next size directly on a flange of the
  bigger (2a), on a plate where needed (2b); an end arriving out of plane
  on a welded plate (4).
- [x] Step 4, the welded plate (in the member's own plane, welded to a
  flat plate on the flange) where its extra offset is at most one plate
  thickness; step 1c, splice covers between in-line angles of other
  sections where nothing else bolts onto them; leg splices said first
  among a joint's reasons (2026-11-19).
Decided 2026-11-19:
- leg splices: a change of section is a splice; the same section is one
  part unless marked; a command proposes the splice places and the user
  confirms;
- the welded plate's extra offset goes into the analysis through the
  automatic eccentricities (O45, the out-of-plane offsets), not
  separately;
- a weld gets a `sides` (fillet count) field and its check (today a weld
  bent about its seam is said NOT ON THE SAFE SIDE);
- bracing bolted through the outer splice cover: yes, after the splice
  places;
- ends whose bolted leg faces away: first the other leg of an equal-leg
  angle bolted where it lies in the face, a bent plate later, else a hand
  detail.
Next — getting the automatic layout to work on real towers (owner,
2026-11-19: "plan ideas to get the automatic layout working", approved
"with recommendations", in this order). On the 78 real towers about 27 %
of the joints are laid out whole; most of the rest fail on a chain: the
members are placed one after the other, and one that does not fit blocks
the joint. Each step is measured on the same towers (share laid out per
step, reasons left), and the work stops where it stops paying:
- [x] J1. **A search per joint** instead of one member after the other:
  per end its choices (flange, outer or inner side, leg bolted, direct or
  on a plate, a few shift steps), searched with backtracking for no clash
  and the least eccentricity; deterministic, no new clash, values given
  by hand kept. Built 2026-11-19, in active development, not approved
  yet: the planes on the biggest members first, each plane's ends in
  every order that can matter (the owner's order first, at most 400
  placings per joint), clear of the joint's fixed steel and of what the
  planes before laid out; after writing, an end in a clash pulled back
  past it (up to three times) before it is left. The flange, the side and
  the leg bolted follow the member's roll (a choice for the whole member:
  J3); direct or on a plate stays per plane (mixed was tried 2026-11-18
  and lost joints). On the 78 real towers: joints laid out whole 27.6 % →
  44.1 %, member ends laid out 48.9 % → 62.8 %, a whole tower 6–9 s →
  10–15 s. The order search itself changed nothing there (stops paying;
  kept for the joints where it does): the gains are the fixed steel seen
  in the plan and the pull-back past a clash.
- [x] J2. **The ends that fit are kept** in a joint that is not whole: the
  space of a member left for a hand detail reserved so nothing laid out
  runs into it; the joint then needs only that member by hand. Built
  2026-11-19, in active development, not approved yet: a member left for
  its own sake keeps its place as it is (about 1 % of the ends laid out on
  two real towers); one left for want of room beside the others, or for a
  clash, is said with what it runs into (keeping every member's place
  cost a quarter to a third of the ends); the window names the members
  left, those whose place is kept, and the ends laid out of all.
- [x] J2b. **The bent plate** (the owner's example joint, approved
  2026-11-19, before J3): an end on a flange's inner face leaving over
  the heel (through the angle's other leg) onto a plate folded along the
  member's line round the heel — one flat where the end lies, one on the
  other leg's outer face bolted to it (two provisional bolts,
  `[plate.leg_bolts]`), the ends of one plane sharing the flat; after the
  half turn onto the outer face, before a hand detail; never forced, no
  new clash, values given by hand kept. Built 2026-11-19, in active
  development, not approved yet: `[plate.bend]` (structure file version
  22: the two flats, the bend line, its angle, the inside radius — two
  thicknesses to start with, the program's value), each flat checked as a
  flat plate, the bend's own bending said NOT ON THE SAFE SIDE, weighed
  from the developed area, in the 3D view, close-up, sheets and IFC; the
  end's offset for the estimated eccentricities 0 (it lies where it lay).
  On the 78 real towers (kept privately): joints laid out whole
  44.1 % → 45.8 %, member ends laid out 62.8 % → 64.3 %, joints with a
  hand detail 49.7 % → 47.9 %, the ends over the heel left 13 370 → 5 705
  (most left now: the other leg too narrow for the holding bolts), a run
  again identical, about 5 % more time; the estimated eccentricities'
  upper bounds 24.3 % → 22.0 % of the ends. An inside radius of one
  thickness changed almost nothing. Decided 2026-11-19 (owner: "Okay go
  with recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-owner-22-recommendations-approved)): no bent plate for tilted ends,
  J5 takes them (C10); the inside radius stays two thicknesses, no shop
  value given (C11); **a bending check at the bend in both code plugins**
  (C12) — built the same day, in active development, not approved yet:
  the additive plugin interface `joint-check-bends`, ASCE 10-97 3.13-1 with
  the gross elastic section and von Mises' criterion for the shear (no rule
  in the standard: on the safe side), SIA 263 4.3.5.4 (6) with elastic
  stresses and Table 14's radius; a plugin without it: not checked, NOT ON
  THE SAFE SIDE as before; on 45–55 mm legs without room for the holding
  bolts the hand detail stays until J8 (C13).
- [x] J3. **Each angle's turn chosen for the whole member**: a roll that
  lets both its ends lie flat, the leg bolted allowed to change between
  its ends where that is how towers are built. Owner 2026-11-19: "You can
  do 90 degrees turn or more if necessary also start with aligning the
  main legs then diagonals and finally secondary braces with redundant
  members being last". Built 2026-11-19, in active development, not
  approved yet (`whole_roll`): the turn from both ends' needs together,
  any turn, the smallest; an equal-leg angle's leg bolted changing between
  its ends where a quarter turn more serves both (never an unequal
  angle's); never forced, said; class by class — legs and chords never
  turned to suit bracing, then diagonals, secondary bracing (no class:
  with them, said), redundants, each with the classes before turned; the
  same order in J1's search (the owner's order used; the biggest member
  first within the bracing measured too: 0.06 % more ends, the same
  joints whole within 7); Orient angles keeps a roll the layout chose for
  both ends; O45's planner the same. On the 78 real towers (kept
  privately): joints laid out whole 45.8 % → 45.8 %, member ends 64.3 % →
  64.2 %, a run again identical 78/78, a whole tower about 5 % slower —
  **stops paying**: the members left
  off their flange need turns at their ends 5°–20° apart (seven in ten),
  their faces turned against each other about the member; no roll removes
  that (J5, a bent welded plate). A turn leaving every end within the
  whole 5° of flat was tried: fewer ends.
- [x] J4. **Lay out once, apply to the joints like it**: the joints
  grouped by kind (the tower's symmetry and repeated panels), one
  solution per kind applied, mirrored, to the others; a command *Apply
  this joint's detail to similar joints* for hand details too. Built
  2026-11-19, in active development, not approved yet (`joint_kinds`,
  `apply_detail`): the eight rigid maps keeping the vertical, matched
  geometrically (a tenth of a degree, 2 mm — the tool's tolerance) on
  members, sections, rolls, classes and what the planner reads; each copy
  checked for clashes in its own place, planned on its own where it fails;
  the command a tool window from the joint's bar (a preview of the joints
  it changes, one undo step), its copies checked in place too. On the 78
  real towers: 30 % of the joints laid out with the plan of a joint like
  them, joints laid out whole 45.8 % → 45.9 %, ends 64.3 %, a run again
  identical 78/78; **no time saved** (planning is 1–1.5 % of a run: the
  joint details and clash checks per round take the rest).
  > **Owner:** (1) J1's search in your order of the classes costs 0.06 %
  > of the ends against the biggest member first within the bracing:
  > keep? (2) Legs are never turned to suit bracing (816 leg ends on the
  > real towers now kept off what they bolt onto, said): keep, or allow it
  > where Orient angles left a leg? (3) J4 saves no time but makes the
  > joints of a kind identical: keep it on by default? (4) ~~The 5° a leg
  > may lie off and still bolt flat is the tool's: your shop's value?~~
  > **Answered 2026-11-19: 2°** (owner: "2° for both", with J9's roll
  > tolerance; `FLAT_OFF`, built the same day; which members count as
  > horizontal keeps its 5°, `HORIZONTAL_OFF`).
- [ ] J5. **Larger shifts once the analysis takes them** (after
  [O45](#o45-joint-eccentricities-and-member-shifts-set-automatically)):
  the shift limit and the welded plate's offset limit growing with the
  member's size, the moment checked instead of the end refused.
- [ ] J6. **Built-up members**: each angle of a back-to-back or star
  member a surface to bolt onto. The safe part built with L6 below
  (2026-11-20, in active development): a plate on the angle facing the
  ends; the rest (between the angles, built-up members arriving, holding
  bolts onto them) not yet.
- [x] J7a. **Leg splices** as decided above, then the bracing bolted
  through the splice cover. **Built 2026-11-19, in active development,
  not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#leg-splices-proposed-j7),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-joint-layout-leg-splices-proposed-bracing-through-the-outer-cover)):
  Lay out joints' preview proposes the leg splice places — two leg
  pieces of other sections spliced (covers outer and inner on each leg
  bolted, the thicker piece's leg thick, the import's bolts, else
  provisional ones), two of one section with bolts one part (the owner's
  rule; their bolts then not checked there: NOT ON THE SAFE SIDE where a
  splice of one section is really there) —, *Confirm* per joint and
  *Confirm all*, one undo step each, never written silently (step 1c's
  silent splices gone); confirmed, bracing lapped on a spliced leg's outer
  face where an outer cover lies bolts through it (seated on the cover,
  its grip the covers and the leg, the cover checked for its whole force;
  directly only, else a hand detail). On the 78 real towers (kept
  privately; J9's proposals confirmed, 2°): every leg place confirmed,
  joints laid out whole 39.1 % → 41.5 %, ends 53.1 % → 55.3 %, misfits as
  given 525 → 475; 503 splices, 10 572 pairs one part; 1 336 bracing ends
  through a cover, 1 242 left (the splice's own bolts in the way — the
  main limit now); a run again identical 78/78, no time change. The 2°
  flatness alone (main at 5° against 2°): joints whole 41.9 % → 39.1 %,
  ends 56.1 % → 53.2 %, misfits as given 605 → 525.
  > **Owner:** (1) The same section one part by your rule: 10 572 pairs,
  > their import bolts not checked there — a splice of one section in a
  > long run would be missed (NOT ON THE SAFE SIDE). Recommended: keep,
  > and give the shop's longest piece so that a run of one section longer
  > than it is said. (2) 1 242 bracing ends at a confirmed splice are left
  > because the splice's own bolts are in their way: recommended next,
  > the bracing's bolts taken as some of the splice's (one bolt through
  > the brace, the cover and the leg), then a plate over the cover. (3)
  > The covers as thick as the thicker leg (the program's value): keep
  > until the codes size them? Recommended: yes. (4) 554 pairs where the
  > smaller leg angle nests in the bigger are not proposed (the layout
  > nests only the biggest pair of a joint): propose them nested for you
  > to confirm? Recommended: yes. (5) Inner covers stop plates on a leg's
  > inner face (about 450 clashes): keep both covers (two shear planes)?
  > Recommended: keep until the shop drawings say otherwise.
- [ ] J7b. **A weld's sides** (decided above: a `sides` field and its
  check). Not started.
- [ ] J8. **Short members**: the class's smaller bolt, one bolt where the
  code's text allows it for a redundant, else a plate shared with the
  neighbouring member.
- [x] J9. **Pieces in line proposed "continuous"** (owner 2026-11-19, a
  follow-up of the joint checks, O19 stage 4.5): the layout proposes
  `continuous` where two pieces run on in line through a joint without a
  splice, for the user to confirm; until then such pieces each end at the
  joint in the checks (on the safe side). Decided 2026-11-19 (D15, D16):
  proposed for same-section pieces in line, the user confirms (with a
  "confirm all" button) — never a silent default; the mutual-host guard
  (two ends bolting onto each other) built together with J9. **Built
  2026-11-19, in active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-joint-layout-pieces-in-line-proposed-one-part-the-mutual-host-guard)):
  the proposals in Lay out joints' preview (*Confirm*, *Confirm all*, one
  undo step; rolls up to 5° apart given one roll, their mean; legs with
  splice bolts not proposed — J7); the guard; ends on a member ending at
  the joint kept on its steel past its cut (else a plate, else by hand);
  ends laid onto a member left as it was checked as written. On the 78
  real towers (kept privately): every proposal confirmed, joints that do
  not fit 31.8 % → 28.8 % (as given 6 764 → 605), joints laid out whole
  46.1 % → 41.9 %; nothing confirmed 36.2 % whole, 839 as given; the guard
  alone 42.1 %; a run again identical 78/78; no time change. The leg
  splices (J7) are now the main limit.
  > **Owner:** (1) ~~the 5° roll tolerance for proposing pieces one part
  > (the mean roll written; seen 0.1°–1.1°): keep?~~ **Answered 2026-11-19:
  > 2°** ("2° for both", with the flatness of J3's question 4; built the
  > same day); (2) ~~Not confirmed, the layout treats the pieces as ending
  > at the joint (fewer joints whole, almost no misfit of its own):
  > keep?~~ **Answered 2026-11-19: stays as it is**; (3) ~~J7 next?~~
  > J7a built 2026-11-19 (above).
- Asked (open): shop drawings of real towers, to tune to how the steel
  is really detailed; the owner is looking for example joints
  (2026-11-19) and sent three that day: a bent gusset plate at a corner,
  folded to follow the two faces, members bolted on both sides of it;
  diagonals bolted directly on a main angle's flanges; gusset plates on a
  leg where diagonals meet it in the middle of a panel. The last two are
  steps 2a and 2b (invented versions in the tests); the bent plate is a
  new element — proposed next: of the joints left for a hand detail after
  J1 and J2, 60 % have an end it would take (an end off its flange, its
  leg bolted facing away, or on an inner face running towards the other
  leg), the only reason for 29 % — built for the ends over the heel (J2b
  above); the ends off their flange stay with step 4's welded plate.

The owner's new ideas for the automatic layout (2026-11-19, approved the
same day in this order; each measured on the same real towers, kept
only where it helps):
- [x] L1. **The quick round.** (a) Bottom up, then out (owner: "try to
  go bottom up and out. More things align on the body from the bottom up
  and then less at the mast ends (arms and top)"): the joints of the body
  first, from the bottom up, then the arms and the top, working outwards
  from the body — the body and the arms worked out from the geometry (the
  legs' lines give the body's outline at each height), the order
  deterministic. (b) Slightly more plates (owner: "maybe increase
  slightly the possibility of having plates"): the plate option also
  tried where it lays out more ends or clears a clash, with a small
  penalty so that bolting directly wins a tie. (c) Redundants and the
  body's diagonals bolted directly (owner: "Redundant members are usually
  bolted into another and diagonals of the shaft too but it can depend"):
  bolted directly onto another member first, plates the fallback — not a
  hard rule. (d) What an end bolts onto (`onto`) chosen in the search:
  the other members it could bolt onto tried too, today's rule first and
  the tie-break, the extra work bounded. **Built 2026-11-20, in active
  development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#the-owners-quick-round-l1),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-joint-layout-the-owners-quick-round-l1)):
  (a), (b) and (d) kept, each a switch, on by default; (c) left out (no
  gain: the members bolted directly ran into the plate of the others on
  the same flange). (a) the body, arms and top from the legs' lines; the
  joint a kind is planned at and a member no one roll lays flat (none of
  its ends flat now) taken bottom up, then out — the order matters
  nowhere else (each joint is planned on its own). (b) a plate also tried
  where it lays out more, and for an end bolted directly before it is
  given up after writing. (d) at most two other members per end left,
  kept at a joint only where it lays out more after the clash rounds than
  today's plan (the guard). Found measuring: three more of the joint
  checks' rules checked after writing (holes of the ends in one member
  overlapping; a plate's own bolts outside its outline or beyond the
  steel of the member it lies on) — the misfits as given 475 → 41. On
  the 78 real towers (kept privately; J9's and J7a's proposals
  confirmed): joints laid out whole 41.5 % → 42.7 % (body 44.0 % →
  45.2 %, arms and top 38.3 % → 39.5 %; (a) +0.3, (b) +0.3, (d) +0.8
  points alone), ends 55.3 % → 56.5 %, plates +4.5 %, a run again
  identical 78/78, the time per tower 12.3 → 20.7 s (median; largest
  18.8 → 33.2 s).
  > **Owner:** (1) The three together take about 1.7 times the time of a
  > run for 1.2 points more joints whole: keep all on by default?
  > Recommended: yes (a follow lays out only the joints an edit touches).
  > (2) (c) is left out; redundants bolted onto another member come from
  > (d) where that lays out more: agree? Recommended: yes, and look at it
  > again with L2 (a direct end kept clear of the plate while planning).
  > (3) The guard keeps L1's plan at a joint only where it lays out at
  > least as many ends as today's *plan* (today's result after the clash
  > rounds is not known without a second run, about twice the time):
  > keep? Recommended: yes. (4) ~~A bent plate's own bolts are placed
  > without the cut of the member they bolt onto, and on some members
  > drawn just outside the flat's outline: the layout now leaves such ends
  > (said). Fix in the bent plate next? Recommended: yes, before L2
  > (small).~~ **Built 2026-11-20 as recommended** (the J2b fix, with L2
  > below).
  > (5) The body's tolerances — legs within 30° of the vertical, 0.10 m
  > from the outline — keep? Recommended: yes. (6) More plates put more
  > ends a plate's thickness out of the flange, not in the analysis (NOT
  > ON THE SAFE SIDE, as for every plate): leave to O45 D? Recommended:
  > yes.
- [x] J2b fix and L2. **The bent plate's own bolts; clash-aware
  planning**: the real 3D shapes while planning, not boxes; before an end
  is given up for a clash, the other face or the plate's other side
  tried. **Built 2026-11-20, in active development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#clash-aware-planning-l2),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-joint-layout-the-bent-plates-own-bolts-clash-aware-planning-l2)).
  The clash-kind reasons left on the real towers before it (36 837), by
  the two parts: a plate with a member's steel 51 %, a bolt's head, nut or
  shank with a plate 19 %, member with member 13 %, plate with plate 9 %,
  the bent plate's own bolts 4 %. The bent plate's own bolts now lie on
  the steel of the member they bolt onto (past its cut by the end
  distance, where that member will be drawn), not through a hole there.
  L2, three parts, each a switch, on by default: (a) **each plate names
  the member whose flange it lies on** (`on`, structure file version 23 —
  found looking into the clashes: the joint details took every plate to
  lie on the joint's leg, so a plate on a bracing member's flange was drawn
  elsewhere than planned and its clash with the leg hidden; now seen, on
  the safe side); (b) **the planner sees what the clash check sees** — the
  outstanding leg to its full height, the nuts and shanks behind the
  flange or plate, each plate's outline as it will be drawn against the
  fixed steel and the planes before, the laid-out bolts whole, and a member
  laid out at the same joint taken where it will be drawn for what bolts
  onto it (deterministic, no new search); (c) **tries**: an end on a plate
  bolted directly where its bolts land (in the plan where its plate hits;
  after writing once per end), the half turn for an end the plan leaves
  for a clash; an end on its own plate not built (it would reach the work
  point and the flange as the shared plate does). Found measuring, for
  every run: two more of the joint checks' rules after writing (a member's
  bolts within the outline of a plate laid out in the plan; a member not
  laid out, nor left by hand, still fitting onto one laid out). L1 (c)
  measured again on top: no gain, left out. On the 78 real towers (kept
  privately; J9's and J7a's proposals confirmed): joints laid out whole
  42.7 % → 48.6 % (body 45.2 % → 48.2 %, arms and top 39.5 % → 49.0 %),
  ends 56.5 % → 61.8 %, joints not fitting 29.7 % → 24.7 % (as given
  41 → 42), clash-kind reasons 36 837 → 22 960 (8 945 of them said in the
  plan, not written), plates +15 %, a run again identical 78/78; the time per tower about the same (median 19.3 → 17–18 s on six towers laid out back to back, the largest about a tenth more).
  The bent plate's own bolts alone: +949 joints whole, the ends left for
  them 1 456 → 872.
  > **Owner:** (1) Each plate the layout makes names the member it lies
  > on (`on`): a laid-out tower with plates is written at structure file
  > version 23 (an older program refuses it). Keep? Recommended: yes — the
  > plates were drawn on the wrong member before; a plate given by hand can
  > name it too (by hand in the file for now; in the Joint panel later).
  > (2) The two new after-writing checks take 2 384 ends back out (joints
  > that would be written not fitting): keep? Recommended: yes (nothing
  > that does not fit is written). (3) A member left for a hand detail is
  > not asked by the second: its bolts may no longer fit the member laid
  > out under it (as given 41 → 42; leaving that member too cost about four
  > times the ends on a sample). Keep? Recommended: yes (its hand detail
  > places its bolts; said). (4) About 9 000 ends are now left in the plan
  > because their plate would run into steel already there, mostly near the
  > work point (a plate's outline always reaches the work point and the
  > flange under it — the rule for plates of 2026-11-02, "keep hidden in
  > the shape of the corner"). Let the layout give such a plate an outline
  > that stops short of what it would run into (round its bolts and the
  > flange it is held by)? Recommended: measure it next, it is the biggest
  > group of L2's. (5) The bent plate's own bolts through a hole of another
  > member there (about 670 ends): slide the pair along clear of the holes?
  > Recommended: yes (small). (6) An `onto` given by hand is replaced by
  > the plate when the layout puts the end onto a plate on that member, and
  > the record then takes it as the layout's (a run again from the file
  > takes it out): keep the hand value in the record? Recommended: yes.
  > **(4)–(6) built 2026-11-20 as recommended** (L2's follow-ups, below).
  > **Answered 2026-11-20: as recommended** (owner; (1)–(3) kept as
  > they are; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-owner-joint-layout-l2-l3-l3b-recommendations-approved)).
- [x] L2's follow-ups. **Built 2026-11-20, in active development, not
  approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#l2s-follow-ups-2026-11-20),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-joint-layout-l2s-follow-ups-a-plate-bent-by-a-few-degrees-l3)):
  (a) a plate whose outline round the work point would run into
  something stops short of the work point — round its members' legs from
  the flange's toe over their bolts (each bolt's end and edge distances)
  and the flange under them —, cut straight short of what it would still
  run into (at most four cuts), where it still lies on the flange over half
  a square of its width; convex, its bolts inside with their distances
  (checked after writing); the plate checks take the outline as cut (the
  strut's free length still to the flange's toe: less conservative than
  round the work point, said). (b) a bent plate's holding bolts slid along
  the leg clear of the holes there (the default minimum spacing). (c) an
  `onto` given by hand kept in the record (`onto=<member>`). Each of (a)
  and (b) a switch, on by default. Measured: below with L3.
- [x] L3. **A plate bent by a few degrees** for ends whose members need
  rolls 5°–20° apart at their two ends (about 24 900 ends on the real
  towers, the second group; the owner's example joint shows such plates).
  **Built 2026-11-20, in active development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#a-plate-bent-by-a-few-degrees-l3)):
  an end at most 20° off its flange goes first onto a plate bent by as
  much from step 4's flat plate — away from the flange, or towards it past
  the flange's toe, whichever moves the member's axis least (within one
  plate thickness either way, step 4's limit) —, then welded as before;
  `[plate.bend]` as J2b's, **no new format field** (no version 24); the
  bend's curved part (r + t) tan(θ/2) from its line, the member's steel
  half the gap beyond it, its bolts their edge distance beyond it. The end
  that takes the bend: unchanged rules — an end flat now stays flat, else
  the end first bottom up, then out lies flat (the arms' and the top's end,
  or the higher one, takes the bend). The bend check C12 applies as it is
  (plugins unchanged). On the 78 real towers (kept privately; J9's and J7a's proposals confirmed;
  L2's result 48.6 % whole, 61.8 % of the ends): (a) alone 49.0 % / 64.0 %
  (5 533 plates stop short, the plates said to run into steel 8 290 →
  5 396), (b) alone the ends left for a hole under a holding bolt 668 →
  129, Part 1 together 49.0 % / 64.0 % (as given 42 → 28), **L3 on top
  49.7 % whole (body 49.5 %, arms and top 50.0 %), 64.7 % of the ends**,
  joints not fitting 24.7 % → 24.1 %, a run again identical 78/78; 4 085
  bent plates written and the welded plates 2 758 → 11 (a bent plate fits
  wherever the welded one did); the ends at most 20° off left 17 840 →
  15 907 (most bends are of 2°–5°; of the 5°–20° ends left, 4 100 have
  their outstanding leg towards the flange, 2 639 lie on an inner face
  running towards the other leg, 2 598 would lie more than a plate's
  thickness out). The bend check C12 as it is, under the comparison's one
  vertical case: 83 % of the L3 bends pass (median usage 0.20, 90 % below
  1.49, largest 6.97 — short bends of thin plates); J2b's 90° bends 50 %
  (median 1.00). Time per tower (six back to back, twice): median
  17.5–18.2 s → 19.3–20.1 s, the largest 24 s → 22 s.
  > **Owner:** (1) Plates stopping short of the work point (a): keep on by default?
  > The strut check of such a plate is less conservative than of the plate
  > round the work point (its free length still reaches the flange's toe).
  > Recommended: yes. (2) Bent before welded up to 20° off (the welded
  > plates almost gone): keep? Recommended: yes (galvanised towers are
  > rarely welded; no weld to check). (3) Bent towards the flange past its
  > toe (the member then nearer the flange than on a flat plate): keep?
  > Recommended: yes. (4) The bend check stays strict for now (no contact
  > relief): 83 % of the L3 bends pass under one vertical case, J2b's only
  > 50 %. Recommended: decide after a run with the design cases; then, if
  > still many fail, a least bend length in the layout rather than relief.
  > (5) Which end takes the bend: the existing rules (the end flat now, else
  > bottom up then out — the arms' and top's end takes it); the end with
  > fewer members not preferred. Recommended: keep; next, choose the flat
  > end's half turn so that the bent end's leg faces its flange (4 100
  > ends 5°–20° off are left for that alone). (6) 20° as the most a plate
  > is bent (`BEND_SMALL_MAX`) and a bolt's edge distance from a bend's
  > curved part as from a free edge: keep? Recommended: yes, until a shop
  > gives its rules.
  > **Answered 2026-11-20: as recommended** (owner; (4): the run with the
  > design cases done with L5 below; (5)'s next step built with L3b).
- [x] L3b. **The bent end's turn, the end running back to its bend, L3's
  small things.** **Built 2026-11-20, in active development, not approved
  yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#the-bent-ends-turn-and-the-end-running-back-to-its-bend-l3b),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-joint-layout-the-bent-ends-turn-the-end-running-back-to-its-bend-l3b);
  no format change). (1) **The flat end's turn chosen for the bent end**:
  looked into on the real towers — the half turn was already tried (step
  2b) and mostly leaves the bent end's axis more than a plate's thickness
  out, or the flat end on the flange's other face; built: an equal-leg
  angle the half turn did not help tried with its other leg bolted (the
  quarter turn facing its bent end's leg away from the flange), kept as
  the half turns are (`bend_facing`, on); a turn picked from the geometry
  alone measured and left out (fewer ends). (2) **An end running back
  towards its bend** bent away from the flange, its steel kept clear of the
  bend only over the flange (to half the gap past its toe) — within L3's
  limits otherwise (`bend_back`, on); bent the other way was tried already
  (fails the axis limit), the flap on the joint's side of the bend line not
  built (the flat plate would leave the work point). (3) The holding bolts'
  slide unit-tested; the 131 ends left for a holding bolt over a hole
  looked into, not fixed (below); the user guide's picture of a
  few-degree bend. On the 78 real towers (kept privately; L3's result
  49.7 % whole, 64.7 % of the ends): joints laid out whole 49.71 % → 49.78 %
  (body 49.5 % → 49.6 %, arms and top 50.0 %), ends 64.66 % → 64.85 % ((1)
  alone +39 joints whole, +199 ends; (2) alone +8 and +132), joints not
  fitting as given 28 → 28, the ends 5°–20° off left 11 260 → 11 037 (their
  outstanding leg towards the flange 4 100 → 3 955, running back to their
  bend 1 600 → 1 529), L3 bends written 4 085 → 4 398 (the strict bend check
  under the one vertical case: 83 % → 84 % pass), a run again identical
  78/78; the time per tower (six back to back, twice) median 19.3–19.7 s →
  20.0–21.0 s, the largest 21.0–21.4 s → 21.9–22.8 s.
  > **Owner:** (1) Keep the other leg tried for the bent end (one more
  > planning pass)? → Recommended: yes (+39 joints whole, +199 ends; the
  > pass costs a few per cent of the time). (2) Keep an end running back to
  > its bend bent away from the flange, its steel past the bend's plane
  > beyond the flange's toe (the clash check checks it)? → Recommended:
  > yes (+8 joints whole, +132 ends; nothing of the joint lies there, and
  > what is written is checked for clashes). (3) The ends left for a holding
  > bolt over a hole (131): the
  > hole is the end of the member the plate is held to, not laid out at that
  > joint and drawn after writing where the plan did not see it (its `onto`
  > named only while planning, or its end off its flange). Write that end's
  > `onto` when a plate is held to it, or leave such ends? → Recommended:
  > leave them for now (a hand detail, said); look again with L4.
  > (4) Most of the bent ends with their outstanding leg towards the flange
  > (3 955 left) cannot be laid out within one plate thickness either way
  > round: allow a filler (a packing plate) under the bent flap? →
  > Recommended: not before the owner's answers to L3's questions 4–6.
  > **Answered 2026-11-20: as recommended** (owner; (4) measured with L5
  > below).
- [x] L5. **Ends moved to bolt directly** (the owner's idea, 2026-11-20:
  "move the 2 diagonal a bit forward maybe to have one bolt on the main leg
  without them touching each other. The second bolt needs the plate anyway
  but maybe shorter"). **Built 2026-11-20, in active development, not
  approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#ends-moved-to-bolt-directly-one-bolt-through-the-leg-looked-into-l5),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-joint-layout-ends-moved-to-bolt-directly-l5);
  no format change): on a face that goes onto a plate, each end whose bolts
  do not land pulled back further, else moved sideways by at most the
  plate's thickness, until its bolts land on the flange; where every end of
  the face then lands, no plate (`direct_moved`, on). The move is the end's
  `shift` (the analysis takes it at level 4, and at level 3 with the
  estimated eccentricities; without them NOT ON THE SAFE SIDE, as every
  shift); the limit on the safe side relative to the plate it replaces
  (whose own offset out of the flange is as large and not in the analysis).
  Kept at a joint only where it lays out more ends, or as many on fewer
  plates. The owner's one bolt through the leg with the rest on a shorter
  plate: **looked into, not built** — the bolt through the leg must lie on
  the flange and the next one past its toe by its nut, so the two lie apart
  across the flange by about three bolt diameters (edge distance, nut, gap):
  only for members nearly square to the flange; 3.7 % of the plate ends on
  a sample (63 % of them have one bolt), and a bolt through three plies
  needs a format field. On the 78 real towers (kept privately;
  J9's and J7a's proposals confirmed): joints laid out whole 49.7 % → 50.4 %
  (body 49.3 % → 50.7 %, arms and top 50.1 %), ends 65.0 % → 65.3 %, plates
  written 65 878 → 61 800 (−6.2 %; their steel by the bill of
  materials 94.2 → 90.8 t, −3.7 %), shifts written 346 → 2 333 (the
  moves median 3 mm, at most 8), joints not fitting as given 26 → 28 (L2's
  question 3: a member left for a hand detail onto one laid out), a run
  again identical 78/78; the time per tower about a quarter more (median
  19.6 → 24.8 s on six towers back to back: the plan made twice for the
  guard, the moves searched).
  > **Owner:** (1) Moves up to the plate's thickness to bolt directly (no
  > plate), on by default, for about a quarter more time: keep? →
  > Recommended: yes (fewer plates, more joints whole, the eccentricity no
  > larger than the plate's own and in the analysis; the time to win back
  > later by planning the moves only where a face goes onto a plate).
  > (2) Up to the shift limit (30 mm) instead (L5a below: on a sample 12 %
  > fewer plates and 0.6 points more joints whole on top of L5, but moves of
  > 18 mm median, e·A/W median 2.3 — the member's usage rises much where the
  > analysis takes it) → Recommended: not by default; after O45 A5 (the
  > estimated eccentricities on by default), as a proposal shown with its
  > usage.
  > (3) One bolt through the leg, the rest on a shorter plate (a format field,
  > version 24, for 3–4 % of the plate ends) → Recommended: not now; again
  > when the codes choose the bolts (O45 B: a pitch that takes the second
  > bolt past the toe). (4) Found: a flat plate the layout lays on a leg
  > running through the joint has no bolt of its own into that leg (the
  > joint checks say its holding not checked) → Recommended: give such
  > plates two provisional holding bolts as J2b's flats have (next, small;
  > measured first). (5) The bend check under the design cases (L3's
  > question 4): the line's checking cases (ice ×1.3, wind 300 Pa
  > ×1.5 from four sides; not a design code's): L3's bends 80 % (the one vertical
  > case) → 57 % pass, J2b's 49 % → 24 % → Recommended: keep the check strict; the
  > bent plate made thick enough for its bend at the forces (with O45 B),
  > else a flat or welded plate, before any relief. (6) A packing plate under
  > the bent flap (L3b's question 4): +1.0 point of the joints whole on a sample (its
  > own target barely), the member a further plate's thickness out of the
  > flange, not in the analysis → Recommended: after
  > O45 D (the out-of-plane offsets in the analysis), not before.
  > **Answered 2026-11-20: as recommended** (owner: "Go ahead with it";
  > (4) built with L6 below).
- Proposed (2026-11-20, L5's part 3; the owner: "imagine more ways the
  number of automatically laid joints can increase. The goal is to have a
  level 3 pylon where the steel is defined and to help the designer to go to
  level 4"). Not built; each measured on every thirteenth of the 78 private
  real towers, or counted on all 78. **What is left** after L5 (the joints
  not laid out whole, by the reasons the layout and its after-writing
  checks give; of 64 760 joints, 61 915 member ends left — main of
  2026-11-20, before L5): ends off their flange (no one roll lays both ends
  flat) 26 262 — at most 20° off 15 674 (their outstanding leg towards the
  flange 5 738, on an inner face running towards the other leg 4 604, the
  axis more than a plate's thickness out 4 389), 20°–45° 6 941, beyond 45°
  3 647; clashes after writing 12 914 and plates planned into steel 6 015;
  built-up members 8 630; flat on an inner face running towards the other
  leg 4 804; a splice's cover in the way 1 119; the longest pull-back 1 432;
  too short for their bolts 725. The only cause at a joint: the clashes
  after writing 4 632 joints, ends ≤ 20° off 4 517, built-up 2 542, inner
  face 1 647, 20°–45° off 1 379, beyond 45° 953, plates into steel 875, too
  short 446, the pull-back 230, a splice 184.
  - L5a. **Moves up to the shift limit** (30 mm) to bolt directly: on the
    sample, on top of L5, +0.6 points of the joints whole, +0.6 of the ends,
    12 % fewer plates; but moves of 18 mm median, the member's extra stress
    from N·e (e·A/W) 2.3 times the axial one at the median, 4.8 at most. No
    steel; the work point moved — shown with its usage, never silently.
  - L5b. **A packing plate under a bent flap** (the axis allowed two plate
    thicknesses out instead of one): +1.0 point of the joints whole, +0.9 of
    the ends on the sample (its own target, the outstanding leg towards the
    flange, barely: 257 → 246 such ends); one small part more per end; the
    member's axis a further plate's thickness out of the flange, not in the
    analysis (NOT ON THE SAFE SIDE until O45 D).
  - L5c. **A longer pull-back** (400 mm instead of 300): +0.2 points of the
    joints whole, +0.25 of the ends, plates +0.5 %: stops paying. L5d. **A
    larger shift limit** (60 mm) and L5e. **bends up to 30° or 45°**: no
    change on the sample (the axis limit fails first).
  - L5f. **A larger member** (a wider flange of what the ends bolt onto):
    of the ends on plates on the sample, 9 % (+10 mm of flange), 17 % (+20)
    and 23 % (+30) would bolt directly at the same cut, the plates of 6, 12
    and 18 % of the joints with plates gone. It lays out few more joints (the
    plates lay them out already): a trade of plates against the steel of a
    whole member (about +10 % of its weight per 10 mm of leg), and it
    changes the level 3 design (stiffness, forces, every joint of that
    member) — proposed to the designer with its usage, never applied.
  - L5g. **Other bolts**: one diameter less (16 → 12 mm) lands 9 % of the
    plate ends directly on the sample — more bolts for the same force, so
    only with the codes choosing them (O45 B); two gauge lines or staggered
    bolts for long ends, and the other leg of an unequal angle (J3: few):
    not measured. L5h. **The owner's one bolt through the leg** (L5 above):
    3.7 % of the plate ends, a format field.
  - L5i. **Built-up members** (J6): 8 630 ends, the only cause at 2 542
    joints (up to +3.9 points). L5j. **The bracing's bolts as some of a
    splice's** (J7a's follow-up): 1 119 ends, 184 joints (+0.3). L5k.
    **Short members** (J8): 725 ends, 446 joints (+0.7). L5l. **Ends off
    their flange on an inner face** (J5, a bent or welded plate for tilted
    ends; C10): 6 446 ends tilted and 4 804 flat, the only cause at 1 647
    joints for the flat ones (+2.5).
  - L5m. **The clashes after writing**, the largest group (the only cause at
    4 632 joints, 7.2 points): a plate into a member's steel 3 962, a nut
    into a plate 1 439, steel into steel 1 397, a head into a plate 859 —
    next to look into one by one (L2's way), the moves of L5 and L5a helping
    some.
  - L5n. **A hand detail copied across towers of a type** (L4): the 78
    towers are 22 types, 63 of them of 7 types with three or more towers —
    each hand detail made once per type would save about seven in ten of
    them on such a line (counted, not measured).
  - L5o. **Holding bolts for the flat plates on a leg running through**
    (found with L5: such a plate has no bolt into that leg; the joint checks
    say its holding not checked): two provisional bolts as J2b's flats.
  **One way to weigh them per joint** (proposed): an ordered rule, each step
  tried only for the ends the steps before leave, within a step the least
  eccentricity, then the fewest parts — (1) as given or concentric, pulled
  back, bolted directly; (2) pulled back further to bolt directly; (3) moved
  by at most the plate's thickness to bolt directly (L5); (4) a flat plate,
  stopping short where it runs into steel; (5) a plate bent by a few degrees
  (L3); (6) a plate bent round the other leg (J2b); (7) a welded plate;
  (8) a sideways shift up to the shift limit — all of these automatic, then,
  **never applied silently, proposed to the designer with their effect on
  the usage**: (9) another bolt layout within the design's capacity (with
  the codes, O45 B); (10) a larger offset of the work point, in the analysis
  (L5a); (11) a larger member (L5f); (12) a hand detail, copied from a joint
  like it (J4, L4). Each step's cost in that order: no part; a cut only; an
  eccentricity no larger than a plate's; a plate; a bend; a bend and two
  bolts; a weld; an eccentricity up to 30 mm; then a change of the design.
  > **Owner:** (7) The weighing rule above, the automatic steps (1)–(8) in
  > that order, (9)–(12) only proposed → Recommended: yes. (8) Next of these
  > → Recommended: L5o (small, the plates' holding), then J6 (the largest
  > single gain, +3.9 points at most), then a look into the clashes after
  > writing (L5m); L5a after O45 A5; L5b after O45 D; L5c–L5e not.
  > **Answered 2026-11-20: as recommended** (owner: "Go ahead with it";
  > L5o, J6 and a first look into L5m built with L6 below).
- [x] L6. **The flat plates' own bolts (L5o), plates on built-up members
  (J6's safe part), the clashes after writing counted (L5m)** (the owner's
  order of 2026-11-20). **Built 2026-11-20, in active development, not
  approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#holding-bolts-plates-on-built-up-members-and-the-clashes-after-writing-l6)):
  a flat plate on a member running on through the joint, which nothing of
  the joint's legs bolts onto, gets at least two provisional bolts of its
  own onto that member (`[plate.leg_bolts]`; more where one member on it
  has more — the program's starting value), within the flange, inside the
  plate, clear of the holes and steel there — the joint checks then check
  its holding (before: not checked); not on a plate stopping short of the
  work point nor on a built-up member (said). A face onto a built-up member
  whose angles are known goes onto a plate on the angle facing it, kept
  clear of the other angles (never between them: the packing not known),
  kept only where the joint lays out more ends, or as many on fewer
  plates; a built-up member's angles keep their root radius for the bolts
  landing on them (before none: on the safe side now). No format change.
  On the 78 real towers (kept privately): joints laid out whole 50.0 % →
  52.2 %, ends 65.2 % → 68.7 %, the ends left for a built-up member
  8 197 → 1 857, plates 65 240 → 71 499 (their steel 92.6 → 107.4 t),
  bolts 183 149 → 224 254, plates whose holding is not checked 24 491 →
  21 565 (L5o alone: 18 732), joints not fitting as given 26 → 26, a run
  again identical 78/78; the time per tower about 30 % more (median 26.8 → 35 s on six towers back to back: J6's guard plans joints with a built-up member twice more, the holding bolts' places searched). L5m: the clashes after writing counted
  by their parts (a plate into the steel of a member left as it is about
  3 900; a member left as it is no longer fitting the member laid out under
  it about 1 700; a head or nut against a plate about 2 500; steel against
  steel about 1 900) — not changed.
  > **Owner:** (1) The flat plates' holding bolts: two, or the most of one
  > member's on the plate — the program's starting value until the codes
  > choose the bolts → Recommended: keep until O45 B, the joint checks
  > checking them meanwhile. (2) No holding bolts on plates stopping short
  > of the work point (their outline would change) and on built-up members
  > (the format's `[plate.leg_bolts]` names a single angle's leg) →
  > Recommended: the short plates next (their outline planned round the
  > bolts), the built-up members with a reading of which angle (no new
  > field). (3) J6's plates: 6 600 more plates, a fifth more plate steel,
  > for 1 542 more joints whole → Recommended: keep (they are the plates a
  > hand detail would make; the bill of materials counts them). (4) The
  > root radius of a built-up member's angles kept by the nuts (fewer ends
  > bolted directly: about 180 joints) → Recommended: keep (on the safe
  > side). (5) Next for the clashes after writing → Recommended: first the
  > plates running into steel left as it is (about 3 900; the plan's check
  > misses a member drawn on its work line without bolts or a seat), then
  > the members left as they are no longer fitting (about 1 700: their
  > host's cut and move kept within their bolts, or the member laid onto
  > the host's plate), then the heads and nuts against plates. (6) Found:
  > main measured again lays out 32 376 joints whole, not the 32 652 of
  > L5's measured column (plates 65 240, not 61 800) → Recommended: trace
  > it before L5a. (7) Found: a J2b bent plate's own bolts drawn outside
  > its flat after writing, its members then left (about 400 ends before
  > L6, 486 with it) → Recommended: trace and fix next (small).
  > **Answered 2026-11-20** (the owner): (1) as built — two, or the most
  > of one member's on the plate; follow-up: the count from SIA 263 (with
  > O45 B; "you can try with SIA 263 later"). (2) as recommended: the short
  > plates next, the built-up members later. (3) keep ("we need them for
  > complex cases"). (4) keep. (5) the order as a starting point, not a
  > fixed rule ("if it needs change to make it better it should not be a
  > fixed decision"): changed where the measurements show a better order
  > or approach, with why. (6) trace it. (7) fix it. (6) and the first of
  > (5) built with L7 below, (7) traced there.
- [x] L7. **An end seated where its member is drawn, the measured columns
  traced, the bent flats' bolts traced, the clashes after writing**
  (the owner's answers to L6 of 2026-11-20). **Built 2026-11-20, in active
  development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#an-end-seated-where-its-member-is-drawn-the-measuring-traced-l7),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-joint-layout-an-end-seated-where-its-member-is-drawn-l7);
  no format change). **L6's question 6 traced: a difference of the
  measuring**, not a regression nor a merge effect — the bundled code
  plugin (the default member check's bolt minimums) is found beside the
  program or above the current folder; L5's later runs started where it had
  been built, its "before" column and L6's did not. On the same footing
  L5's moves gave +218 joints whole (not +494) and 1.0 % fewer plates (not
  6.2 %); L5's quick "guard" effect was the plugin too. From L7 on the
  measurements run with the bundled plugin (as the program ships). **The
  clashes after writing, first cause found and fixed** (the owner's order a
  starting point: the measurements showed one cause under all three
  groups): an end lapped onto a member that is itself drawn off its work
  line (seated on a third, shifted, its gauge line moved through the work
  point) was drawn as if that member lay on its work line — off it, in the
  drawings and every check; now seated on it where it is drawn (the joint
  details; every file, not only laid-out ones; on the safe side). And a
  flat plate's own bolts at a joint laid out like another, mirrored, name
  the leg the map carries the other's onto (before: the other leg, off the
  plate). On the 78 real towers (kept privately; with the bundled plugin):
  joints laid out whole 51.9 % → 52.5 % (+413), ends 68.4 % → 69.4 %,
  plates 67 432 → 69 154, plates' holding groups 18 269 → 21 042, clashes
  after writing: a plate into steel 4 089 → 3 117 ends, a nut into a
  plate 1 519 → 1 204, a member left as it is no longer fitting 1 755 →
  1 734, a plate's own bolts outside its outline 294 → 228; joints not
  fitting as given 28 → 35 (one tower: ends now drawn on what they bolt
  onto where it is), as assumed 13 028 → 12 906; a run again identical
  78/78. **L6's question 7
  traced, not fixed**: the bent flat's member is mostly one the run named
  what it bolts onto in its planning copy and does not lay out — written
  without the name, it is drawn elsewhere; writing the name was tried (on
  six towers 17 → 12 such ends) but turns those members' assumed bolts into
  given ones (not fitting as given 34 → 102) for no more ends: not kept.
  L5o on short plates: not reached.
  > **Owner:** (1) An end seated on its member where that member is drawn
  > (the joint details of every file change where such ends are) →
  > Recommended: keep (the drawings and checks then see the steel where it
  > lies; on the safe side). (2) The measurements from now on with the
  > bundled code plugin, L5's figures corrected as above → Recommended:
  > yes. (3) The bent flats (and plates, ends) on a member the run names but
  > does not lay out: plan such a member where the file will draw it (the
  > joints with such members in detail once more without the run's names)
  > → Recommended: next, measured first. (4) The members left as they are
  > no longer fitting (about 1 700; mostly the partner of a member bolting
  > onto it, off its flange): when the plan lays out the member such a
  > partner bolts onto, keep the partner's bolts on it — its cut no further
  > than the partner's first bolt less the end distance, its move within
  > them — else leave it → Recommended: try next, measured. (5) Heads and
  > nuts against plates (about 2 300 after L7; mostly of members left for
  > a hand detail against plates on members left as they are): keep the
  > heads and nuts of members left for a hand detail clear in the plan, as
  > their steel already is (J2) → Recommended: try, measured (it may lay
  > out fewer ends where a hand detail would have moved them). (6) A plate
  > into the steel of a piece running on as one part with another (the
  > largest plate clash left) → Recommended: trace next. (7) L5o on short
  > plates (L6's question 2) → Recommended: after (3).
- [x] L8. **A joint planned where its members are drawn, the partners'
  bolts kept, the bolts already there found** (L7's proposals 3 to 6 in
  L7's recommended order; the owner busy: built on its own, reported).
  **Built 2026-11-21, in active development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#a-joint-planned-where-its-members-are-drawn-the-partners-bolts-kept-l8),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-21-joint-layout-a-joint-planned-where-its-members-are-drawn-the-partners-bolts-kept-l8);
  no format change). **L7's proposal 3 built (fixes L6's question 7)**: a
  joint with an end the run named what it bolts onto in its planning copy
  but does not lay out (the name not written, the member drawn elsewhere)
  is planned once more with that member where the file draws it, kept
  where it lays out more ends, or as many on no more plates, leaving no
  member it laid out or left for a hand detail unsaid. **L7's proposal 4
  built**: an end laid out onto a member left as it is that bolts onto it
  keeps that member's bolts — its cut and move only where each hole stays
  within its flange, past its cut by the end distance, its own bolts the
  minimum spacing away (the code's, else the default minimums: assumed);
  else left for a hand detail in the plan, said. **Found measuring,
  fixed**: the bolts already there on a face were missed by the plan away
  from the structure's origin. **L7's proposal 5 tried, not kept** (no
  change on six towers); traced: nearly all heads and nuts against plates
  are the end's own bolts against another plate of the joint. **L7's proposal
  6 traced, not fixed**: a plate whose outline after writing reaches over
  a run lapped on the same face. L5o on short plates: not reached.
  On the 78 real towers (kept privately; with the bundled plugin), before
  → as drawn → L8: joints laid out whole 52.5 % → 53.4 % → 53.3 %, ends
  69.4 % → 70.7 % → 70.1 % (of more ends asked: an end left for a
  partner's bolts is now named by the plan), plates 69 154 → 70 860 →
  70 722 (their steel 111.8 → 116.1 → 115.8 t), the bent flats' bolts off
  their flat 228 → 37 → 39 ends, members left as they are no longer
  fitting 1 734 → 1 379 → 152, plates whose holding is not checked
  21 270 → 21 759 → 20 973, joints not fitting as given 35 → 35 → 42 (one
  joint repeated on seven towers of one type: a member nested in the
  partner carries its bolts onto the end now left; said by the joint
  checks), as assumed 12 906 → 12 877 → 12 883, a run again identical
  78/78; the time per tower (six towers, back to back,
  twice, load 5–6) median 30 → 41 → 45 s, the largest 36 → 47 → 50 s.
  > **Owner:** (1) Plan a joint again with the members named but not laid
  > out where the file draws them (the run's time about a third more) →
  > Recommended: keep (more ends, the bent flats' bolts off their flat
  > gone; the plan sees what the checks see). (2) The partners' bolts kept
  > on the end they bolt onto: about as many ends on 138 fewer plates, 45 fewer
  > joints whole, the members no longer fitting after writing 1 379 → 152,
  > one joint more not fitting as given (on seven towers) → Recommended: keep (the
  > decision made in the plan where the joint is planned round it, no end
  > taken back after writing for it, fewer plates; on the safe side); the
  > alternative: off (more ends laid out, each taken back after writing
  > where its partner's bolts no longer fit). (3) The plan keeping clear of
  > the bolts already there wherever the structure lies (a fix) →
  > Recommended: keep. (4) Heads and nuts against plates: nearly all
  > (1 751 of 1 754 ends) are the end's own bolts against another plate
  > of the joint, not a hand member's (L7's proposal 5 tried: no change)
  > → Recommended: next, trace where the plan misses them, then the ends'
  > heads and nuts and the plates checked against each other across the
  > joint's planes in the plan. (5) Plates into a run
  > lapped on the same face (L7's proposal 6) → Recommended: next with
  > (4): the plate's planned outline as after writing, checked against the
  > run, else cut short of it as a short plate is. (6) L5o on short plates
  > → Recommended: after (4) and (5).
- [x] L9. **A plate read in its own plane, plates planned as written,
  short plates held, the run faster** (L8's questions 4 to 6 in L8's
  recommended order, then the speed; the owner busy: built on its own,
  reported). **Built 2026-11-21, in active development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#a-plate-read-in-its-own-plane-plates-planned-as-written-short-plates-held-l9),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-21-joint-layout-a-plate-read-in-its-own-plane-plates-planned-as-written-l9);
  no format change). **L8's question 4 traced and fixed**: the heads and
  nuts against plates were nearly all an end's own bolts against its
  **own** plate, drawn in a wrong plane — a plate's outline as written
  read back from its first three corners, in a line where it runs
  straight (on the private towers' written files 22 % of the outlines
  more than 2° off); now from all its corners. **Found with question 5,
  fixed**: an end on a plate lying on a member drawn off its work line
  was seated as if that member lay on it, where nothing else bolted onto
  the member directly. **L8's question 5 built**: a welded plate's flat
  plate (step 4) planned with its outline as written (round the work point,
  the seams and the flange under them), else stopping short of the work
  point and cut straight, else the ends in their own planes left for a
  hand detail; and after writing, a flat plate with nothing bolted on it is
  its welded plates' members' (before, its clashes were written unsaid:
  **not on the safe side before L9**). **L8's question 6 (L5o on short
  plates) built**: holding bolts on a plate stopping short of the work
  point where its outline grown round them still fits. **Faster, every
  number the same** (all 78 written towers byte for byte): the least-cut
  clips only where they cut, the joint details' model without the plastic
  moduli, a connection's empty test. On the 78 real towers (kept
  privately; with the bundled plugin), L8 → the plate's plane → the plates
  on members drawn off their line and the flat plates as written → a flat
  plate's clashes after writing owned and short plates held: joints laid out whole 53.3 % → 54.7 % → 56.2 % → 55.8 %, ends 70.1 % →
  71.3 % → 71.6 % → 71.0 %, plates 70 722 → 72 063 → 70 940 → 69 295 (their steel 115.8 →
  120.6 t), heads and nuts against plates after writing 1 754 → 807 →
  804 → 795 ends, plates into steel after writing 2 326 → 2 300 → 1 410
  → 1 517, plates whose holding is not checked 20 973 → 21 049, joints not
  fitting as given 42 → 53 (mirrored twins of joints known since L8, and
  members left as given beside ends now drawn where they are; said by the
  joint checks), a run again identical 78/78; the last step's fewer ends
  (−1 178) are mostly ends laid out before with a clash of their flat plate
  written unsaid (measured alone on six towers, the short plates' holding
  bolts cost 18 ends of 9 537); the time per tower (six towers, back to
  back, twice, load 3 to 8) median 45 → 42.5 (L9 without the speed) → 33 s,
  the largest 50 → 45 → 34.5 s.
  > **Owner:** (1) A plate's plane read from all its corners (every file
  > with plate outlines given reads so; files laid out before too) →
  > Recommended: keep (a fix: the plate and its ends drawn where they are).
  > (2) An end on a plate seated on the plate's member where that member is
  > drawn (every file where such plates lie) → Recommended: keep (L7's rule,
  > for plates too). (3) A welded plate's flat plate planned as written,
  > else short, else the ends in their own planes left; its clashes after
  > writing its welded members' → Recommended: keep (the plan sees what is
  > written; the second was not on the safe side). (4) Holding bolts on
  > short plates: on the 78 towers the plates' holding groups checked 21 908 → 22 443 and the plates whose holding is not checked 22 003 → 21 049; measured alone on six towers: 3 fewer joints whole and 18 fewer ends (of 9 537), 12 fewer plates → Recommended: keep (more plates' holding checked:
  > on the safe side); the alternative: off (`short_holds`). (5) The speed:
  > a method added to the section crate (`elastic_properties`), the
  > finite-element model's sections built with it (the same values) →
  > Recommended: keep. (6) What is left of the heads and nuts against
  > plates (795 ends): mostly an end's own nut 1 to 4 mm into its own
  > plate where two ends on one plate lie a few degrees off each other (each
  > within the owner's 2°) → Recommended: next — the plate drawn in the
  > plane of the flange it lies on and an end's own head and nut against its
  > own plate tested as lying on it where its leg lies within 2° of it (it
  > bolts flat by the owner's rule), measured first. (7) The plates into
  > steel left (1 517 ends; bent flats, short plates, plates in their
  > members' own planes) → Recommended: trace next, kind by kind. (8) Not
  > written: the tests L8 could not write (the bolts already there away
  > from the origin; an end keeping its partner's bolts — on the invented
  > split joint no cut and move keeps them: the pieces' bolts lie nearer
  > the redundant's end than their end distance) → Recommended: with the
  > next step, from an invented joint built for them.
  > **Answered 2026-11-21: keep what's built and move forward** (the
  > owner: "Keep what's built and move forward": (1)–(5) kept as built,
  > (6)–(8) go ahead — with L10 below).
- [x] L10. **Members on either side of a leg's flange, bolts that join
  nothing, a plate in its flange's plane** (the owner's idea from
  photographs of real towers, the owner's picture of two bolts joining
  nothing, and L9's questions 6 to 8; the owner busy: built on its own,
  reported). **Built 2026-11-21, in active development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#members-on-either-side-of-a-legs-flange-bolts-that-join-nothing-a-plate-in-its-flanges-plane-l10),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-21-joint-layout-members-on-either-side-of-a-legs-flange-l10);
  no format change). **The owner's idea built**: where two or more ends
  on one face of a leg's flange go onto a plate (or one is left for want
  of room beside another), some of them half a turn onto the flange's other
  face — the diagonals onto the inner face first, as in the photographs —,
  kept where the member's joints then lay out more ends, or as many on
  fewer plates (joint by joint, none worse), and applied only where every
  end so moved lies no farther from the work point across the flange than
  now (onto the inner face: nearer); the other way only proposed, with the
  bending it would add. The move across the flange is a seat, not a
  `shift` (the analysis leaves every face's seat out, as before; NOT ON THE
  SAFE SIDE in that sense for every end, as before). **The owner's picture
  traced and fixed**: an end left as it is that bolts onto nothing (pieces
  in line not one part) is drawn with its design data's bolts, which join
  nothing; such a joint is no longer counted whole, the member by hand,
  said; the bolts are not moved onto the plate (a question). **L9's
  question 6 built**: a plate on a flange, its members within 2°, drawn in
  the flange's plane, and an end's own head and nut on its own plate
  touching it within 2° (the owner's 2° rule). **L9's question 7 traced,
  not fixed**: two in three of the plates into steel that take ends back
  run into a piece running on as one part through the joint (mostly at
  crossings left for a hand detail anyway). **L9's question 8**: the bolts
  already there far from the origin and a short plate's own bolts tested on
  invented joints; an end keeping its partner's bolts still not (no
  invented joint reaches it). **The owner's second picture (two diagonals
  on both faces of one plate) traced, not built**: on the same tower laid
  out as measured that joint lays out both diagonals directly, no plate;
  ends on both faces of one plate are left for L11. On the 78 real
  towers (kept privately; with the bundled plugin), L9 → the face step alone → L10: joints laid out whole 55.8 % →
  55.7 % → 55.1 % (the joints with bolts joining nothing no longer counted
  whole), ends 128 092 → 128 237 → 130 329 (71.0 % of more asked), plates
  69 295 → 69 320 → 70 140 (their steel 120.6 → 123.1 t), nuts into plates
  after writing 776 → 20 ends, members left after the clash rounds 10 619 →
  9 438, joints not fitting as given 53 → 48, as assumed 11 633 → 11 461,
  bolts joining nothing in joints counted whole 1 289 ends (3 217 bolts)
  → 0, a run again identical 78/78; the time per tower (six towers, back to back, twice)
  median 33 → 35 s, the largest 34.5 → 38 s (load 2 to 3).
  > **Owner:** (1) The members on either side of a leg's flange
  > (the half turn tried, kept where the member's joints lay out more ends,
  > or as many on fewer plates, and only where every end so moved lies no
  > farther from the work point across the flange): on the 78 towers,
  > alone, ends laid out 128 092 → 128 237, joints whole 36 130 → 36 102, plates 69 295
  > → 69 320 (1 586 ends turned, 1 385 onto the inner face; 166 proposed
  > the other way); small, as the clash check after writing takes back
  > some of what the plan saved → Recommended:
  > keep on. (2) The move across the flange written as the end's seat, not
  > as a `shift` (the brief asked for the shift; the format's `shift` lies in
  > the face, and the analysis leaves every face's seat out, the inner
  > face's nearer the work point than the outer's) → Recommended: keep;
  > every end's offset across its flange into the analysis later, with
  > O45 D (a new field, structure file version 24). (3) The photographs'
  > pattern first (the diagonals onto the inner face): no gain measured on
  > six towers (the same ends, 8 more plates than the later class first) →
  > Recommended: keep (the owner's pattern, no end lost). (4) The half turns
  > the other way (onto the outer face, the flange's and the end's thickness
  > farther out) only proposed in the notes with e·A/W (166 notes on the 78
  > towers) → Recommended: keep as proposals; a list in the window to take
  > them by hand later. (5) A joint with bolts joining nothing not counted
  > whole (the joints whole 55.7 → 55.1 % on the 78 towers with (6)'s gain
  > in: those said honestly); the bolts
  > not moved onto the layout's plate as its holding bolts → Recommended:
  > keep; confirm the pieces in line one part where proposed (J9), which
  > takes them away; using an end's own bolts as a plate's holding bolts
  > only with a format decision. (6) A plate drawn in its flange's plane
  > and an end's own head and nut on its own plate touching it within 2°
  > (every file) → Recommended: keep. (7) L9's question 7: plates into a
  > piece running on as one part (two in three of the plate clashes that
  > take ends back) → Recommended: next, a bent plate's flat and flap
  > planned against such a piece as written (as L9 did for the welded
  > plates' flat plates). (8) Both faces of one plate (the owner's second
  > picture) → Recommended: L11, counted first (how often a flat and a bent
  > plate on one face could be one plate). (9) An end keeping its
  > partner's bolts: no invented joint reaches it → Recommended: write the
  > test from a joint kind found on the real towers, reproduced invented.
- [x] L11. **Solve the clashes, an end moved to pass the flange on a plate,
  both faces of one plate, a leg turned against its joints** (the owner's
  four pictures of a real tower and words of 2026-11-21; the owner busy:
  built on its own, reported). **Built 2026-11-21, in active development,
  not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#solve-the-clashes-an-end-moved-to-pass-the-flange-on-a-plate-both-faces-of-one-plate-a-leg-turned-against-its-joints-l11),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/joint-layout.md#solve-the-clashes),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-21-joint-layout-solve-the-clashes-face-lining-steps-a-leg-turned-against-its-joints-l11);
  no format change). **A. Joint details**: the problems grouped by kind with
  counts, closed until opened, no button per problem; one **Solve the
  clashes…** working out the joint's best whole solution on copies (the
  layout of the joint, hand values kept then replaced; the layout's
  proposals at the joint confirmed, marked; then the fixes of what is left),
  weighed by clashes, problems, members left by hand, refused where it pulls
  back a leg or a member running on, turns a leg, changes a section or makes
  the joints around worse; what it changes listed (turns, moves,
  confirmations marked), the close-up before and after, usage on request,
  Apply one undo step; when nothing is better it says why. **B. Face-lining
  steps added to the plan** (not one rewritten procedure): an end on a plate
  whose bolts would need more than the longest pull-back to pass the flange
  moved sideways within the shift limit (its `shift`); two plates on
  flanges in one plane of members one part made one; an end on a bent or
  welded plate lying flat in a flange's plane on its inner side, leaving
  over the heel, onto the back of that flange's plate (both faces of one
  plate), drawn there by the joint details. Each kept at a joint only where
  it lays out more ends or as many on fewer plates, and planned again
  without them where the joint lays out fewer ends after writing (the run's
  guard). **C. A leg turned against its joints** found from the bracing
  ends' planes through the leg's axis (two members agreeing within 4°, more
  than 2°; the faces through the neighbouring legs a hint only), listed by
  Lay out joints and offered in Solve the clashes with its effect, never
  applied silently.
  On the 78 real towers (kept privately; with the bundled plugin), L10 →
  L11 (the end moved to pass the flange off in a run): joints laid out
  whole 35 687 → 35 656 (55.1 %), ends 130 329 → 130 097 (70.9 %), plates
  70 140 → 67 532 (−3.7 %; one plate per plane on 822 pairs, both faces of
  one plate 755 ends), joint by joint 1 675 better (more ends, or as many on
  fewer plates) and 240 worse (80 no longer whole, 49 newly whole) — the
  run's guard plans a joint again without the steps where it lays out fewer
  ends after writing, but a joint can still lose an end through a
  neighbour's change; a run again identical 78/78; the time per tower (six
  towers, back to back, twice, nothing else running) median 35 → 38.5 s, the
  largest 38 → 41 s. The end moved to pass the flange,
  measured alone on six towers: fewer ends at 11 joints, more at 8, plates
  +2 — not better in a run, so off there and tried per joint by Solve the
  clashes. The leg-rotation check: after Orient angles 114 of 9 429 leg
  pieces flagged on 22 towers (110 at leg extensions where Orient finds no
  faces, ±3.8° to ±4.1°; four at ±13.5° where Orient's faces say 0°); as
  imported (no Orient) 4 212 of 8 695 (the import's rolls). The owner's
  joints: the splice joint, as imported, Solve the clashes 35 clashes → 4,
  with the leg turn it proposes (+3.8°, the tower's faces agree) → 0, all
  six ends bolted directly; after Orient angles → 0 with the end moved
  sideways, one plate per leg plane (the owner's picture); the joint of
  bolts joining nothing → 0, nothing left, the pieces confirmed one part;
  the joint of two diagonals → 0 clashes with three plates (the second
  diagonal 3.6° off its flange by its roll as imported: not flat enough for
  the plate's back; with hand values replaced one plate, the diagonal
  half-turned, its other end one problem more → refused).
  > **Owner:** (1) Problems grouped and closed, one Solve the clashes, no
  > button per problem; the per-problem fixes now only a last step of the
  > solution, never pulling back a leg or a member running on → Recommended:
  > keep. (2) The solution may confirm the layout's own proposals at the
  > joint (pieces in line one part, a leg splice place), marked, applied
  > with Apply (one undo step) → Recommended: keep (the owner's rules for
  > them; marked, one click to undo); the alternative: a tick, off by
  > default. (3) The end moved sideways to pass the flange on a plate: off
  > in a run (measured not better), tried per joint by Solve the clashes →
  > Recommended: keep so. (4) Both faces of one plate and one plate per
  > plane on in a run: 3.7 % fewer plates, 1 675 joints better, 240 worse
  > (232 ends fewer in all) → Recommended: keep on, and next trace the
  > worse joints (a neighbour's merged plate or back-face end taking room
  > after writing); the alternative: off in a run, tried per joint like (3).
  > (5) A member on a plate's back drawn there by the joint details (every
  > file; before: drawn off the plate) → Recommended: keep (a fix). (6) The
  > leg-rotation check: proposals only; after Orient angles 114 pieces,
  > mostly leg extensions Orient finds no faces for → Recommended: next,
  > Orient angles turning such pieces by the joints' estimate (proposed with
  > its effect), and a look at the one tower where the joints and the faces
  > disagree by 13.5°. (7) Not built: the plates at a leg splice as the
  > splice's outer covers ("everything bolted to the plates", the legs'
  > splice bolts through the gusset) → Recommended: next with J7a's
  > follow-up (a cover grown into a gusset; a format field). (8) Not built:
  > one general per-joint procedure replacing the special passes (the brief's
  > generalisation) — the new steps were added to the plan in the owner's
  > order and measured → Recommended: continue step by step, each measured
  > joint by joint, rather than one rewrite.
  > **Answered 2026-11-21 (evening): (1) and (2) approved** (the problems
  > kept grouped and closed, one Solve the clashes; one-click Apply,
  > including the layout's own proposals at the joint, marked); **(3)–(8)
  > not answered yet** (the owner asked for the joints grouped first: L12
  > below).
- [x] L12. **The joints grouped by what comes in and how it lies; a chord
  turned against its joints (a setting, off)** (the owner, 2026-11-21:
  "grouping the joints based on how they are laid out and what kind of
  members come in … see if we would need to apply per group methods or
  have it working differently based on the incoming parts … The goal is to
  lay the joints so that it works most of the time"; the owner busy: built
  on its own, reported). **Built 2026-11-21, in active development, not
  approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#the-joints-grouped-by-what-comes-in-and-how-it-lies-l12),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/joint-layout.md#a-joints-group),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-21-joint-layout-the-joints-grouped-by-what-comes-in-and-how-it-lies-l12);
  no format change). **The grouping**: every joint described from its
  details alone — its host (what the other ends bolt onto: class first,
  then the steel area), what runs through along it (one part, the same
  section, a splice, the section changing, nothing), its place (body, arm,
  top) and per incoming end its class, a plan member or not, its size, its
  angle to the host, how far its axis runs out of the host's flanges'
  planes (in within 2°, a few degrees out within 16°, out beyond) and how
  far its leg lies off (its roll). Measured on the 78 private real towers,
  as imported and after Orient angles: the place and the members' counts
  explained less than **how the incoming ends lie against the host's
  flanges** — all in a flange's plane 80 % whole after Orient angles (65 %
  as imported), some a few degrees out 13 % (16 %), some out of plane 5 %
  (3 %), in every place. So a group is *what the ends bolt onto · what
  comes in · how they lie* (89 groups; the 30 largest hold 88 % of the
  joints); shown in Joint details as "Group: …". The largest groups (after
  Orient angles; whole as imported → after):

  | group | joints | whole | of the ends left by hand |
  |---|---|---|---|
  | chord running on (arm, top) · with plan members · all in a flange's plane | 12.9 % | 34 → 70 % | 8.3 % |
  | member running on (body) · redundants · all in plane | 10.9 % | 62 → 98 % | 0.4 % |
  | chord running on · bracing · all in plane | 6.8 % | 75 → 87 % | 1.4 % |
  | built-up member · redundants · all in plane | 5.6 % | 46 → 47 % | 6.3 % |
  | leg running on · redundants · all in plane | 5.0 % | 14 → 98 % | 0.3 % |
  | chord running on · redundants · all in plane | 4.5 % | 43 → 97 % | 0.2 % |
  | built-up leg · redundants · all in plane | 4.2 % | 4 → 97 % | 0.4 % |
  | leg running on · bracing · all in plane | 4.1 % | 52 → 96 % | 0.5 % |
  | member running on (body) · with plan members · all in plane | 4.0 % | 24 → 83 % | 1.1 % |
  | built-up member · redundants · a few degrees out | 3.4 % | 15 → 11 % | 2.5 % |
  | member running on (body) · with plan members · a few degrees out | 2.0 % | 16 → 20 % | 3.3 % |
  | nothing runs through · bracing · out of plane | 2.0 % | 0 → 0 % | 9.4 % |
  | member running on (body) · with plan members · out of plane | 1.9 % | 0 → 0 % | 5.8 % |
  | crossing · nothing else · none to measure | 1.9 % | 23 → 51 % | 0.2 % |
  | chord running on · with plan members · out of plane | 1.8 % | 4 → 1 % | 5.5 % |
  | built-up member · with plan members · out of plane | 1.8 % | 3 → 7 % | 6.0 % |
  | leg running on · with plan members · out of plane | 1.5 % | 0 → 0 % | 3.6 % |

  **Why ends are left** (43 909 after Orient angles): an end's axis more
  than 2° out of the host's flanges' planes 43 %; in a flange's plane but
  its leg not flat there (its roll set at its other end, or its leg on the
  host's other flange) 19 %; room (a plate into steel, clashes, the
  pull-back, too short) 18 %; a built-up host 10 %; bolts joining nothing
  7 %; a leg splice 3 % — the same causes in every place. Pareto: ten
  groups hold 57 % of the ends left; seven of them have a plan member
  coming in (plan and hip members: 57 % of the ends left). **Per group
  or general**: what is missing is general and driven by the incoming
  ends (an end's host chosen by the plane its axis lies in, a member ending
  at the joint too — the plan members onto a horizontal's level leg; a
  built-up member's seat by that plane, then J6's plates), except two
  causes of their own group: where nothing runs through, the base member's
  own bolts join nothing (565 joints left for that alone — L10's question
  5, a format decision), and chords whose joints say they are turned
  (built as a setting, off). **A chord turned against its joints**
  (`LayoutSettings::chord_turns`, off): L11's leg turn read for a chord
  (a host piece running on that is not a leg), in rounds — turned, the run
  made again, kept per chord where its joints lay out more ends or as many
  on fewer plates, none fewer at any — until nothing flagged helps.
  Measured (78 towers, after Orient angles; L11 → the setting on): joints
  whole 35 656 → 35 884 (230 gained, 2 lost), ends 130 097 → 130 943,
  plates 67 532 → 67 798, joint by joint 858 better and 60 worse, the time
  per tower median 41 → 164 s (four to five runs of the layout);
  a run again identical on 5 of 6 towers tried (on the sixth some chords are turned back: not stable there). Unweighed, every flagged chord turned: +633 whole but
  1 396 joints worse. L11's leg turns applied (its question 6): +54 whole,
  101 better, 14 worse.
  > **Owner:** (1) The grouping (what the ends bolt onto · what comes in ·
  > how they lie), shown in Joint details, nothing depending on it →
  > Recommended: keep. (2) Per group or general: general rules driven by
  > the incoming ends, two group causes apart → Recommended: next, an
  > end's host chosen by the plane its axis lies in, including a member
  > ending at the joint (the plan and hip members onto a horizontal's level
  > leg or a level plate across two), measured joint by joint; then the
  > built-up member's seat by that plane and J6's plates. (3) Where nothing
  > runs through: the base member's end bolted onto the gusset the others
  > bolt onto, its own bolts the gusset's holding bolts (a format field;
  > 565 joints wait for it alone) → Recommended: yes, with the format
  > decision. (4) The chords turned against their joints
  > (`chord_turns`, off): +228 joints whole, 858 better and 60 worse, but
  > four times the time, and a chord's roll is Orient angles' → Recommended:
  > keep off in a run; next, propose the chord turns as L11 proposes the
  > leg turns (Lay out joints' preview and Solve the clashes, with their
  > effect, a tick to apply), and trace why Orient angles' turn of a chord
  > (to the faces through its neighbouring chords) and its joints disagree
  > by a few degrees (a fix there costs no time). (5) The leg turns L11
  > proposes (its question
  > 6), measured applied after Orient angles: 114 pieces, joints whole
  > +54 (58 gained, 4 lost), ends +145, plates −28, 101 joints better and
  > 14 worse → Recommended: offer them in Lay out joints' preview with a
  > tick (applied with the run, one undo step), as Solve the clashes does
  > per joint. (6) The ends 2° to 5° out of their host's flange plane
  > (12 039 ends, 41 % left; from 5° to 20° 82 % left): the owner's 2° rule
  > decides them → Recommended: keep the 2°; the chord turns bring about a
  > tenth of them within it (measured), the rest are for the bent plate —
  > next, trace why the bent plate fails from 5° to 20° (its outstanding leg
  > towards the flange, the inner face over the heel, the welded plate's
  > offset limit).
  > **Answered 2026-11-22: (1)–(6) approved with the recommendations**
  > (the owner: "Ok with recommendations go ahead"): the grouping kept;
  > general rules driven by the incoming ends (L13 below); (3) the base
  > member's own end bolts as the gusset's holding bolts where nothing runs
  > through, with its format field (a structure file version) — allowed,
  > also for L10's question 5; (4) the chord turns off in a run, offered
  > with a tick, the disagreement with Orient angles traced; (5) the leg
  > turns offered with a tick; (6) the 2° kept.
- [x] L13. **An end's host chosen by the plane its axis lies in** (L12's
  recommendation, the owner's answers of 2026-11-22; the owner busy: built
  on its own, reported). **Built 2026-11-22, in active development, not
  approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#an-ends-host-chosen-by-the-plane-its-axis-lies-in-l13),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-22-joint-layout-an-ends-host-chosen-by-the-plane-its-axis-lies-in-l13);
  no format change). **A. The host by the plane** (`plane_hosts`, on): an
  end more than 10° out of today's choice's flanges' planes, held by no
  member running through, named onto a single angle **ending** at the joint
  whose flange's plane holds its axis (a plan diagonal onto a face
  horizontal's level leg) — of a class at least its own and as wide, the one
  it already lies flat on first, no chains, where nothing runs through only
  a plan or hip member; the name taken back where its member's turn would
  spoil its other ends; every such joint planned once more without it (its
  member turned as without it) and written, today's kept where not worse.
  An end laid onto a member that ends at the joint (also L1 d's): that
  member's own end bolts carry its force too, checked for its own force only
  — **NOT ON THE SAFE SIDE**, said in the run's notes. On the 78 real towers
  (kept privately; with the bundled plugin), L12 → A: joints whole 35 656 →
  35 692, ends 130 097 → 130 444, plates 67 532 → 67 894, 390 joints better
  and 41 worse, a run again identical 78/78. Not built: a level (or bent)
  plate across two horizontals.
  > **Owner:** (1) The host by the plane, an end more than 10° out of
  > today's choice (the ends 5° to 10° out renamed too: 250 joints worse,
  > 137 better) → Recommended: keep (10°, measured). (2) Where nothing runs
  > through, only plan and hip members so named until the base member's
  > bolts are decided (the face members there: 123 worse, 34 better) →
  > Recommended: keep; look again with item 5 (the base's bolts). (3) An
  > end laid onto a member ending at the joint: that member's end bolts
  > checked for its own force only — NOT ON THE SAFE SIDE, said →
  > Recommended: next, the joint bolt groups follow the force through (the
  > host's end bolts with the vector sum of what bolts onto it near its
  > end).

  **B. A built-up member's seat by the plane** (every file's joint
  details): an end on a built-up member whose angles' flanges' planes hold
  its axis lies with its leg in that plane; an end naming nothing it bolts
  onto has its leg chosen against the member the layout plans it onto (before
  its long leg: L12's ends "out of the face by 60° to 76°"). L13 A → B:
  joints whole 35 692 → 35 940 (none lost), ends 130 444 → 130 779, plates
  67 894 → 67 917, 601 joints better and 301 worse (mostly plan members now
  flat on a built-up member's flange whose plate runs into steel: J6's
  rest); the time per tower (six towers, back to back, twice) median
  38.4 → 37.8 s, the largest 40.3 → 40.1 s (L12 → A and B). Traced, not
  built: a built-up member running on given the
  redundant as what it bolts onto (none on the 78 towers with the pieces in
  line confirmed); a plate between a built-up member's angles (J6's rest).
  > **Owner:** (4) The built-up seat by the plane, on every file (301
  > joints worse, 601 better, +248 whole) → Recommended: keep; next, J6's
  > plate between a built-up member's angles (where such ends belong).

  **C. Leg and chord turns offered with a tick** (the owner's answers to
  L12's questions 4 and 5; [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/joint-layout.md#leg-or-chord-turned-against-its-joints)):
  Lay out joints' preview lists the leg and chord pieces the joints say are
  turned, a tick each (off by default); ticked, the preview is worked out
  with the turns and says the run with and without them (measured); Apply
  writes them with the layout, one undo step. Solve the clashes offers the
  chord pieces as the leg pieces. The run's `chord_turns` stays off.
  **Orient angles against the joints traced**: every flagged chord piece
  Orient angles set (839 on the 78 towers) is flagged by exactly half its
  faces' difference from square (Orient bisects; the joints' estimate counts
  only the ends bolting onto the piece and asks for one flange in their
  face, the other then twice as far off); a rule tried (a flange in the face
  that alone has members ending on the chord) changed 2 of 839 — not a fix
  at no cost, left as built.
  **D. The few-degree bent plate, 5° to 20° (traced, not fixed)**: of the
  ends 5° to 20° out of their host's flange plane after L12 (10 101), 7 704
  left: the welded plate's tests after the bent plate — its outstanding leg
  towards the flange (1 405), its axis farther out than a plate's thickness
  (1 174) —, the inner face towards the angle's other leg (about 1 230),
  measured against a flange its axis does not lie in (about 1 800, partly
  what B now seats right).
  > **Owner:** (5) The turns offered with a tick, the preview's run with and
  > without them → Recommended: keep. (6) Orient angles' chord turn left as
  > the bisector (faces off square: neither face flat within 2° where the
  > difference is over 4°) → Recommended: keep; the ticks weigh per chord.
  > (7) Not built: L12's question 3 (the base member's own bolts as the
  > gusset's holding bolts, a format field — allowed 2026-11-22) →
  > Recommended: next, as its own step (a structure file version, the reader
  > keeping older files, the joint checks' holding group from those bolts).
  > **Answered 2026-11-22** (owner: "for L13 go with recommendations"):
  > questions 1 to 7 approved as recommended.
- [x] L14. **The force through a host's own end bolts; the base member's
  end bolts as the gusset's holding bolts** (the owner's answers of
  2026-11-22: L13's question 3, L12's question 3 / L10's question 5; the
  owner busy: built on its own, reported). **Built 2026-11-22, in active
  development, not approved yet.**
  **1. The force followed through**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-groups.md#the-force-through-a-members-own-end-bolts-l14),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-22-joint-checks-the-force-through-a-members-own-end-bolts-l14);
  no format change, the plugins' interface unchanged): an end bolted onto a
  member that ends at the joint (L1 d's and L13's names) — that member's
  own end bolts carry its force and the end's (each along its line, their
  moment about the bolts' centroid, shared elastically; each bolt the
  larger of that and the member's own force alone, on the safe side), in
  the group of what it bolts onto, a plate's holding bolts, a welded
  plate's welds and a group of its own end bolts ("its end bolts with … laid
  onto it": shear, bearing on the member and the plate). Still NOT ON THE
  SAFE SIDE, said on a row and in the run's notes: the member's own steel at
  those bolts (net section, tearing out) and a plate's own checks under them
  (its own force only). On the 72 real towers with loads (kept privately,
  one vertical case): 414 such groups (usage median 15 %, at most 78 %); 5
  usages of the rows before change, all up (largest 54 % → 137 %, a plate's
  holding bolts); 867 ends onto a member bolting onto nothing or whose
  bolts are not placed now said NOT CHECKED (before silent); the layout
  unchanged.
  > **Owner:** (1) Each bolt the larger of the force followed through and
  > the member's own alone (a relieving end not counted on) → Recommended:
  > keep (on the safe side; a usage can only grow). (2) The member's own
  > net section and tearing out at its end bolts still under its own force
  > (said NOT ON THE SAFE SIDE) → Recommended: next, the member check
  > given the force followed through at that end (a core change the code
  > plugins' member check reads: asked first).
  **2. The base member's end bolts as the gusset's holding bolts**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#the-base-members-end-bolts-as-the-gussets-holding-bolts-l14),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-22-joint-layout-the-base-members-end-bolts-as-the-gussets-holding-bolts-l14);
  **structure file version 24**, a plate's `held_by`): where nothing runs
  through the joint, a member left by hand only because its bolts join
  nothing, with a flat plate of the plan on its own flange, is bolted onto
  that plate and holds it (the plate's own provisional bolts onto it, where
  planned, become its end bolts; else its own as drawn), applied as a plan
  is written; the joint checks take it as the plate's holder. On the 78
  real towers (kept privately): bolts joining nothing 3 303 → 2 833 ends,
  121 plates held so, joints whole 35 940 → 35 964, ends 130 779 → 130 902, plates
  67 917 → 67 928, 108 joints better and 11 worse, a run again
  identical 78 of 78; most such joints keep other hand details, and where the member's bolts as drawn run into steel near the
  work point it is put back (members left after the clash rounds 11 875 →
  12 260).
  > **Owner:** (3) The base member's own bolts as drawn used where the plate
  > has no provisional bolts of its own (they often run into steel near
  > the work point and are put back) → Recommended: keep; next, lay the
  > base member's end bolts out as the plate's (placed clear, pulled back
  > along it) where its own run into something. (4) Applied as a plan is
  > written, not while planning (measured: while planning 16 joints worse
  > on six towers, at writing none) → Recommended: keep. (5) Not where an
  > end left as it is bolts onto the base member directly (without that
  > condition: the same joints whole, 364 ends more instead of 123, 355
  > joints better and 11 worse, but 19 more joints that do not fit as given
  > — the base's bolts against that end's holes) → Recommended: keep (the
  > layout writes nothing that does not fit); next, the base's bolts placed
  > clear of such an end's.
  > **Answered 2026-11-22**: question 2 **yes** (owner: "you should check
  > the impact on the bolt on the other member to make it safe") — built
  > the same day (in active development, not approved yet;
  > [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-22-joint-checks-a-members-own-steel-and-its-plate-under-the-force-followed-through-l14s-question-2)):
  > the member's own steel at those end bolts (its tension capacity by the
  > code's plugin — the least over its ends of net section, block shear and
  > bolts — against the larger of its own force and the resultant) and a
  > plate's own checks under them (the larger of the two, along the
  > member's line) now under the force followed through; no plugin
  > interface changed; the NOT ON THE SAFE SIDE row gone where covered;
  > still said: no tension capacity from the plugin (NOT CHECKED), the
  > plate's sections under a resultant turned more than 15° off the
  > member's line (NOT ON THE SAFE SIDE on the row). On the 72 private
  > towers with loads (one vertical case): 414 such rows now checked
  > (usage median 0.12, at most 0.47), 28 plate rows up (the largest
  > +0.10), none newly failing. Questions 1, 3 to 6 not answered
  > explicitly: kept as built and recommended.
- [x] L15. **A plate between a built-up member's angles** (L14's item 3 —
  J6's rest; L13's question 4; the owner busy: built on its own,
  reported). **Built 2026-11-22, in active development, not approved
  yet.**
  **1. The plate between the angles**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#a-plate-between-a-built-up-members-angles-l15),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-22-joint-layout-a-plate-between-a-built-up-members-angles-l15);
  **structure file version 25**, a plate's `between`): an end whose leg
  lies flat on the back of a built-up member's angle facing another across
  a gap, its outstanding leg across it (L13's seat puts plan members
  there), goes onto a plate in the gap, as thick as the gap (the
  section's), where the gap is at least half the thickest leg on it; the
  ends on its far face pulled back clear of the other angle (the member's
  own steel across the gap now in their way); its own provisional bolts
  through the plate and both angles back to back (two shear planes in the
  joint checks), the plate grown along the member in the gap round them,
  checked clear; the joint details seat it on that back, in its plane; the
  clash check tests it against the member it lies between. Weighed: the
  plan without where it lays out more ends or as many on fewer plates —
  on a tie the plate between. On
  the 78 real towers (kept privately; with the bundled plugin), L14 main
  → with it: joints whole 35 964 → 37 059, ends 130 902 → 133 015, plates
  67 928 → 69 226, members left after the clash rounds 12 260 → 11 670,
  not fitting as given 50 → 50, a run again identical 78/78; joint by
  joint 1 490 better and 218 worse; 3 327 plates between the angles; their
  members' bolts' joint checks failing as often as the run's other
  plates' (5 % against 6.9 % of rows, one vertical case).
  > **Owner:** (1) The plate between as thick as the gap (the section's,
  > fitted to its properties on imported sections: 3 to 15 mm, often
  > thinner than the legs on it), laid out where the gap is at least half
  > the thickest leg on it (the program's value) → Recommended: keep (the
  > joint checks check it as thick as it is); set the real gusset thickness
  > as the sections' gap where known. (2) On a tie the plate between kept
  > (concentric, its bolts through both angles; an approved order changed)
  > → Recommended: keep. (3) 218 joints worse (109 fewer ends: the plan with
  > the plate between taken back after writing, the joint then planned as
  > before L1 rather than as L14 made it; 109 more plates) → Recommended:
  > next, the run's guards tried in turn (without the plates between, then
  > as before L1) so that no joint ends worse than L14's. (4) Found: an end
  > naming nothing it bolts onto, on a built-up member, planned on the outer
  > face of the angle the joint details seat it on by its inner face →
  > Recommended: next, plan it on the face the joint details seat it on
  > where it lies across no gap.
- [x] L16. **The newest step never leaving a joint worse; the plan's face
  as drawn** (L15's questions 3 and 4 and its item 4; the owner busy:
  built on its own, reported). **Built 2026-11-22, in active development,
  not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#the-newest-step-never-leaving-a-joint-worse-the-plans-face-as-drawn-l16),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-22-joint-layout-the-newest-step-never-leaving-a-joint-worse-the-plans-face-as-drawn-l16);
  no format change). **1.** A base member holds no plate where an end
  left with no connection bolts onto it as the file written gives it (its
  `onto` there, else the joint's leg worked out: `Arrival::joint_leg`) —
  L15's item 4: not fitting as given 50 → 48. **2.** The turns (the half
  turns, L10's other faces) weighed on plans without the plates between
  (184 of L15's 218 worse joints had a member turned otherwise); the
  fallback without the plates between also where a joint lays out as many
  ends on more plates, planned as L14 planned it (its ends searched for
  other members to bolt onto, those names written only on the fallback),
  and the plan with the plates between taken again and kept where the
  fallback lays out less; the guard's measures worked out again at every
  planning. **3.** An end planned on the face the joint details seat it
  on (`joints::seat_face`, `LayoutSettings::seat_as_drawn`, on). On the 78
  real towers (kept privately; with the bundled plugin), L15 main → 1 and
  2 → 3: joints whole 37 059 → 37 031 → 37 100, ends 133 015 → 133 060 →
  133 075, plates 69 226 → 69 197 → 69 391, members left after the clash
  rounds 11 670 → 11 744 → 11 366, a run again identical 78/78; joint by
  joint against L14 main: 218 worse → 81 (better 1 490 → 1 517); 1 and 2
  against L15 185 better, 43 worse; 3 alone 227 better, 221 worse; the time
  per tower +2 % to +3 % (median 40.8/39.8 → 41.7/41.1 s). **4. Traced,
  not changed** (L15's question 6): a 60 mm leg's "22 mm from its heel and
  41 mm from its toe" — the heel distance its thickness, root and the nut
  (the program's clearance); the toe distance the bundled code plugin's
  ASCE 10-97 edge distance (4.5.1 Eq. 4.5-1, 4.5.3 Eq. 4.5-7, a sheared edge
  assumed) with the force per bolt, without an analysis, the bolt's own
  design shear capacity (4.3.2; an M20 bolt on a 5 mm flange of steel with
  f_u 490 MPa: 94 kN, end 46 mm, edge 41 mm). Item 5 (a level or bent
  plate across two horizontals) not built.
  > **Owner:** (1) The turns weighed without the plates between (the
  > newest step never changes a roll; 30 joints whole under L15 no longer,
  > 218 → 81 worse against L14) → Recommended: keep. (2) The remaining 81
  > worse against L14 (41 a member turned otherwise after writing, 42 where
  > nothing runs through) → Recommended: next, a turn refused after writing
  > (no joint of it laid out with it) tried as before the plates between
  > first; then look at the joints where nothing runs through with item 5.
  > (3) The face as drawn: plan and drawing now agree (227 better, 221
  > worse, +69 whole, members left after the clash rounds −378, ends an
  > inner face cannot take now left and said) → Recommended: keep (a plan
  > that the drawing does not follow is no sound layout); next, the ends on
  > a built-up member's inner face (most of the worse) — their half turn or
  > the plate between. (4) The force per bolt without an analysis for the
  > code's end and edge distances: ASCE 10-97 4.4 limits the force a bolt
  > passes into a part to 1.5 f_u d t (bearing), so 1.2 P/(f_u t) needs not
  > exceed 1.8 d — on that 60 mm leg the edge 41 → 32 mm (31 mm for its
  > rolled toe, Eq. 4.5-6), a band of 5.5 mm where there is none now →
  > Recommended: yes, the plugin's force basis without an analysis the
  > smaller of the bolt's shear capacity and its bearing in the part (a
  > code plugin change, the source cited; the joint checks still check the
  > bearing), and the toe of a rolled angle taken as a rolled edge.
- [x] L17. **One plate in one gap; a turn refused after writing tried as
  before the plates between** (L16's questions 2 and 3; the owner busy:
  built on its own, reported). **Built 2026-11-23, in active development,
  not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#one-plate-in-one-gap-a-turn-tried-as-before-the-plates-between-l17),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-23-joint-layout-one-plate-in-one-gap-a-turn-tried-as-before-the-plates-between-l17);
  no format change). **1. Traced**: most joints worse than before the
  plates between that are not where L16 made plan and drawing agree came
  from **two plates between in one gap** — two ends across a built-up
  member's gap from either side (mirror-image redundants), or on the two
  pieces of a member one part through the joint, each given a plate in the
  one gap (L15 keyed a plate by the back it lies on): the clash check after
  writing found the plates in each other, the joint was taken back and fell
  back. Now the ends across one gap **share its plate**, each on its own
  face (the plan's plane the gap's own; the joint details seat each end on
  the face its outstanding leg looks out of, as before). And a turn refused
  after writing (its member laid out at none of its joints) is first tried
  with its joints that were planned onto a plate between planned without
  them (the turns are weighed so since L16), each member once. On the 78
  real towers (kept privately; with the bundled plugin), L16 main → L17:
  joints whole 37 100 → 37 315, ends 133 075 → 133 265, plates 69 391 →
  69 295, members left after the clash rounds 11 366 → 11 179, not fitting
  48 / 11 538 → 48 / 11 537, a run again identical 78/78; joint by joint
  273 better and 47 worse (whole gained 217, lost 2); against L14 main
  1 761 better and 321 worse (L16: 1 744 and 301). Of the 47 worse, 26
  have a member turned otherwise: its one roll now lays it out at a built-up
  joint on the shared plate (that joint whole) and no longer onto a bent
  plate at its other joint (as many of its ends, one joint each way); 21 lay
  out as many ends on two plates where L16 had one. The time per tower
  (six towers, two at a time, back to back, twice) unchanged within the
  scatter: median 90.7 and 78.7 s → 77.9 and 83.2 s (the computer busier
  than for L16's 41 s). **2. Tried, not kept**: the ends on the inner face of a built-up
  member's angle that run towards its other leg (574 left; 981 more on
  single angles where the bent plate round the other leg finds no room for
  its own bolts — the code plugin's edge distance, L16's question 4) — the
  half turn onto a plate between weighed with the plates between: 10 joints
  better, 17 worse, 4 whole lost on 70 towers; not built. **3. Not built**
  (time): a level or bent plate across two horizontals (the plan and hip
  ends left: about 6 700 lying off their host's flange with no roll laying
  them flat, about 3 200 with their axis out of its face).
  > **Owner:** (1) One plate between in one gap, its ends on its two faces
  > (whole +215, 273 better, 47 worse — 26 of them a member's one roll
  > now serving its other joint) → Recommended: keep. (2) The joints still
  > worse than before the plates between (321 against L14 main): mostly
  > where L16 made plan and drawing agree (built-up members' angles whose
  > inner face a bolt or a plate cannot take as drawn) and members' one
  > roll serving one joint or the other → Recommended: accept as the sound
  > layout (the L14 result there was planned on another face than drawn);
  > next, the joint checks' view of those joints by hand. (3) The ends on a
  > built-up member's inner face: the half turn onto the plate between made
  > more joints worse than better → Recommended: next, a bent plate round
  > the angle's other leg on a built-up member (the leg's outer face free in
  > two angles back to back) — the joint details to draw it first. (4) The
  > 981 single-angle ends whose bent plate finds no room for its own bolts
  > wait for L16's question 4 (the edge distance) → Recommended: answer
  > L16's question 4 first. (5) A level plate across two horizontals for
  > the plan and hip members → Recommended: next, as its own step (a plate
  > on two members, a format field).
- [x] L18. **Lay out by planes first: rolls from the joints' planes**
  (the owner, 2026-11-22: "the axes of the incoming members into a joint
  define planes together and we should have the parts rotated accordingly
  and then shifted and either bolted to each other when possible and if not
  with a new plate, and if there are multiple planes … then we should go
  from high to low"; approved the same day with four decisions — automatic
  on imported towers, proposals on designed ones; side by side with L11's
  turns; every joint a turn reaches checked). **Built 2026-11-23, in active
  development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#lay-out-by-planes-first-rolls-from-the-joints-planes-l18),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/joint-layout.md#leg-or-chord-turned-against-its-joints),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-23-joint-layout-lay-out-by-planes-first-rolls-from-the-joints-planes-l18);
  no format change). **1. The planes of a joint** (`joint_planes`): each
  other end's axis and the host's span a plane through the host's axis;
  within 2° one plane; high to low (the biggest member, then force where
  given — none in the structure file —, then the number of ends); per plane
  the host's nearer flange, the turn laying it there, each end's angle out
  of it. **2. Orient angles v2** (`plane_rolls`, the default; the outline
  alone kept selectable): each leg's, then chord's, then face member's run
  in line turned as one (one roll per member) so that the most ends at all
  its joints lie within 2° of its flanges' planes, centred in the turns
  keeping them so, at most 22.5° from the outline's turn; residuals per end
  and joint reported. On the 78 real towers (kept privately; with the
  bundled plugin; L17 main reproduced exactly) the columns the owner asked
  for, and v2 weighed (each run's turn kept only where every joint it
  reaches is laid out better):

  | | L17 main | v2 automatic | L11 proposals applied | both | v2 weighed |
  |---|---|---|---|---|---|
  | ends within 2° of the host's flange plane | 79.2 % | 81.0 % | 80.8 % | 81.5 % | 80.0 % |
  | joints whole | 37 315 | 38 042 | 38 046 | 38 317 | 37 882 |
  | member ends laid out | 133 265 | 135 259 | 134 466 | 135 912 | 135 444 |
  | plates | 69 295 | 69 840 | 68 392 | 69 172 | 69 927 |
  | members left after the clash rounds | 11 179 | 11 670 | 11 268 | 11 919 | 11 273 |
  | not fitting as given | 48 | 46 | 48 | 46 | 48 |
  | joints better / worse than L17 | — | 3 461 / 2 400 | 2 776 / 1 414 | 4 450 / 2 593 | 1 935 / 293 |

  Of v2's 2 400 worse joints 2 365 have a member turned otherwise (a turn
  helping one joint of a run, costing another). Gains where ends lie a few
  degrees out (a body's member running on with plan members: whole 255 →
  692 of 1 281; an arm's chord with bracing: 107 → 300 of 747); found: a
  built-up leg with bracing in plane loses whole joints with every unweighed
  turn (413 → 338 of 560; weighed 411). The bound: each joint's host turned
  on its own lays 82.5 → 87 % of one tower's ends within 2° — most ends out
  of plane are so by the geometry, not by a roll. The time per tower (six towers, back to back, twice):
  Orient angles 0.3 → 2.0 s, the layout after it median 71.3 / 80.0 → 79.2 /
  81.8 s; a run again identical on 16 of 16 towers run again (every sixth and the first four).
  **3. Plane by plane, high to low**: the plan already works plane by plane
  (directly where every bolt lands, else one plate shared by the plane);
  a rank's planes from the biggest member down measured worse on six
  towers (whole −4 / −15, more plates): `planes_high_to_low`, off. Not
  built: the special passes as one general procedure, forces (none in the
  file). **4. Side by side**: Lay out joints lists the plane turns of the
  leg's and chord's runs beside L11's, each its own tick (ticked by default
  where the roll is the program's), each with its effect at every joint its
  members reach; Solve the clashes offers both, each solution laid out at
  every joint the turned members reach and refused where those are left
  worse; Lay out joints ticks a plane turn of a program roll by default only
  where every joint it reaches is laid out better.
  > **Owner:** (1) Orient angles from the joints' planes as the default on
  > imports (unweighed: +727 whole, +1 994 ends, but 2 400 joints worse)
  > → Recommended: next, weigh it in the run — each automatic run's turn
  > kept only where every joint it reaches is laid out better (measured
  > +567 whole, +2 179 ends, 1 935 better and 293 worse; a layout run more);
  > until then keep v2 as built. (2) L11's proposals applied lay out as many
  > joints whole with 903 fewer plates; both together the most (+1 002
  > whole) → Recommended: keep both offered side by side; next, measure
  > both weighed (v2, then L11's, each kept where it helps). (3) Step 3: a
  > joint's planes high to low by size measured worse (−4 / −15 whole on six
  > towers) → Recommended: keep off; the plan already works plane by plane,
  > and what is left out of plane is mostly plan and hip members between a
  > host's flanges — next, L17's question 5 (a level plate across two
  > horizontals). (4) Forces: the structure file stores no analysis forces,
  > so planes are ordered by size → Recommended: no change until analysis
  > results are kept with a structure (a format decision). (5) "Imported"
  > read from the file's record (a roll set by Orient angles or the layout,
  > or none, is the program's; else the designer's) — no new flag →
  > Recommended: keep (no format change). (6) Face members turned from their
  > planes too, after legs and chords (+60 against +31 whole on six towers)
  > → Recommended: keep, with (1)'s weighing.
- [x] L19. **Turns weighed over every joint they reach** (L18's questions
  1 and 2; the owner's rule of 2026-11-22, "when turning a member both
  sides should be reverified"; the owner busy: built on its own,
  reported). **Built 2026-11-23, in active development, not approved yet**
  ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/joint-layout.md#turns-weighed-over-every-joint-they-reach-l19),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/joint-layout.md#orient-angles),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-23-joint-layout-turns-weighed-over-every-joint-they-reach-l19);
  the layout record's `plane_turns`, no version change). **1. Weighed
  automatic turns**: Orient angles lists each run it turns from the
  joints' planes (roll before and with); the next layout run weighs them —
  every turn taken back (the base), then the legs' and chords' runs, then
  the face members', each kept only where every joint its members reach is
  laid out no worse and one better, and where no joint elsewhere laid out
  worse meets a member the layout turned otherwise beside it (a bracing's
  one roll turned for its end at the turned member); laid out again with
  only the turns kept until they all still help. A turn refused: its roll
  back, said, offered in Lay out joints; a turn weighed only by a run
  laying out every joint it reaches; Lay out joints weighs once per
  preview on the whole tower. On the 78 private towers (the bundled
  plugin; L17 main and L18 main reproduced exactly):

  | | L17 main | L18 main (unweighed) | weighed | its base |
  |---|---|---|---|---|
  | joints whole | 37 315 | 38 042 | 37 796 | 37 434 |
  | member ends laid out | 133 265 | 135 259 | 135 194 | 133 610 |
  | plates | 69 295 | 69 840 | 69 763 | 69 211 |
  | members left after the clash rounds | 11 179 | 11 670 | 11 179 | 11 177 |
  | not fitting as given | 48 | 46 | 46 | 48 |
  | joints better / worse than L17 | — | 3 461 / 2 400 | 1 679 / 122 | 225 / 80 |
  | joints better / worse than L18 | | — | 2 391 / 1 946 | |

  | every third tower (26) | L17 main | L18 main | weighed | both weighed (L11's turns too) |
  |---|---|---|---|---|
  | joints whole | 12 161 | 12 382 | 12 304 | 12 342 |
  | member ends laid out | 42 967 | 43 587 | 43 572 | 43 797 |
  | plates | 22 232 | 22 409 | 22 368 | 22 314 |
  | joints better / worse than L17 | — | 1 083 / 768 | 539 / 43 | 828 / 90 |

  Weighed keeps 66 % of the joints whole the turns gained and 97 % of the
  ends; of its 122 joints worse than L17, 44 are worse than its base (no
  turn reaches them, no member turned otherwise meets them), the rest where
  the base differs from L17 (pieces confirmed one part after the turns;
  runs one part with a piece not listed, left as written). Kept: 18 of 156
  legs' runs, 462 of 1 938 chords', 684 of 18 519 face members'. Groups: a
  body's member running on with plan members a few degrees out whole 255
  → 692 (L18) → 406 of 1 281; the unweighed turns' losses gone (a built-up
  leg with bracing in plane 413 → 338 → 411 of 560). A run again identical
  8 of 8. The time per tower (six towers, twice, load 2.6–7.6): the layout
  after Orient angles median 45.0 / 70.9 s unweighed, 265.7 / 335.3 s
  weighed (about five and a half layouts), 426.7 / 433.9 s both weighed —
  once per imported tower. **2. Not built** (time): a level or bent plate
  across two horizontals (L17's question 5) — counted: 6 793 plan and hip
  ends off their host's flange with no roll laying them flat, 5 092 of them
  where two other horizontals (or one and a leg) meet; 3 282 with their
  axis out of the host's face.
  **3. Plates with several bends and welded assemblies of several plates**
  (the owner, 2026-11-23, asked about them: "go ahead to improve").
  **3a. Measured** (the weighed column, all 78 private towers; the reason
  the layout gives for each member end it leaves — an upper bound: every
  such end taken as laid out, nothing checked yet): 6 780 ends refused
  because a plate welded in their own plane would have their outstanding
  leg stand on the flat plate (it points into the flange: a flap in their
  plane does not free it — the member would need half a turn, which its
  other end refuses, or a stand-off holding the flap clear; up to 1 062
  joints whole); 5 674 because that plate would put their axis farther out
  of the flange than one plate's thickness (the rule keeping the line of
  action within what the analysis takes; 5 065 of them within 40 mm, 1 806
  with their leg at most 15° off the flange, where a joggle — two bends —
  could bring the axis back; up to 656 joints whole); 7 266 on
  a flange's inner face running towards the other leg (a plate wrapping
  the leg; up to 1 525 joints whole); 9 961 whose axis runs out of the
  host's face (up to 2 305 joints whole); the first three kinds together:
  up to 5 130 joints whole. Plan and hip ends among them: 2 889, 2 270,
  1 490 and 2 656. The format: a chain of bends (a flat bent from a bent
  flat) reads as one part of its flats, each bend on the flat naming it,
  drawn flat by flat, weighed from the developed area and checked bend by
  bend (tested on an invented joint); a welded assembly is every plate
  joined by `welded_to`, any number. **3b. Not built**: what was measured
  does not support the two forms as asked — a flap in the member's plane
  does not free an outstanding leg pointing into the flange, and a joggle
  does not take out a tilt (most of the second kind are more than 15° off)
  — so nothing was built on a guess (the owner's question below).
  > **Owner:** (1) The automatic turns weighed in the run (the owner's
  > rule): 66 % of the joints whole and 97 % of the ends the unweighed
  > turns gained, 122 joints worse than L17 instead of 2 400 — at the cost of
  > the layout run laid out about five and a half times (once, after
  > Orient angles) → Recommended: keep as the default; the time is paid
  > once per imported tower. (2) Both weighed (L11's turns as a stage
  > between legs and chords and face members): the most member ends on
  > fewer plates, 90 worse than L17 → Recommended: keep L11's turns
  > proposals (never automatic, as decided); next, offer "weigh the
  > leg-rotation check's turns too" as a tick in Lay out joints with this
  > measured effect. (3) The guard past the turned members (a joint
  > elsewhere worse where a member the layout turned otherwise beside it
  > meets) and a joint's wholeness in the guard → Recommended: keep (44
  > joints worse than the base remain, where nothing turned meets them).
  > (4) The base differs from L17 (80 worse, 225 better): pieces in line
  > confirmed one part after Orient angles average their rolls with the
  > turns in → Recommended: next, Orient angles' runs joined as one-part
  > proposals join pieces (in line within 1°, rolls within 2°), so that a
  > chain and its run share one turn. (5) The time (a layout run about
  > five and a half times) → Recommended: accept (quality before speed);
  > next, if it matters, weigh a stage's turns on the joints they reach and
  > one ring around (measured equal to the whole first). (6) A level plate
  > across two horizontals (L17's question 5) not built: up to 5 092 plan
  > and hip ends where two horizontals or one and a leg meet → Recommended:
  > next as its own step: a plate held by two members (a format field,
  > `[plate.also_bolts]`, version 26), drawn, clash-checked, its holding
  > bolts on both members checked as holding bolts (L14's force path),
  > weighed per joint. (7) Plates with several bends and welded assemblies
  > (item 3): the format already carries them (a chain of bends tested);
  > the ends they could take are many (up to 5 130 joints whole) but the
  > largest kind, an outstanding leg pointing into the flange (6 780 ends),
  > is not freed by a flap in the member's plane, and a joggle helps only
  > the ends at most 15° off (1 806) → Recommended: next, (a) the axis
  > offset of a plate in the member's own plane taken into the analysis
  > (the estimated eccentricities, O45) instead of refusing it beyond a
  > plate's thickness — most of the 5 674, the plate already drawn and
  > checked; (b) the joggle for the ends at most 15° off; (c) the
  > outstanding leg into the flange: a stand-off (a welded assembly of
  > three parts) or a half turn — the owner to choose; each weighed per
  > joint with the fabrication order direct → one plate → one bend →
  > several bends → welded (fewer parts, no weld to inspect, first).
- Not built yet, J6's rest: built-up members arriving (as the end, not
  what it bolts onto), holding bolts onto a built-up member's angle (a
  plate on its outer face).
- [ ] L4. **A joint's detail copied to the other towers of the line**
  (laid out or done by hand) onto their matching joints, not only within
  one tower.
- Not scheduled yet: J7a's follow-up — the bracing's bolts used as some
  of the splice's bolts, for the bracing ends a confirmed splice's nuts
  block (1 242 on the real towers).

Later: the bolts chosen by the code plugins and the layout run again with
them.

### O45. Joint eccentricities and member shifts set automatically
From an investigation of 2026-11-19 (owner: "investigate a bit more how
to automatically set the joints eccentricities and member shifts").
**Decided 2026-11-19; A1–A3 built 2026-11-19, in active development, not
approved yet (step 4 of the [order](#next-in-order-owner-2026-11-19);
[theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/estimated-eccentricities.md)).**
Three offsets meet at a tower
joint: an angle bolted by one leg sits out of the plane (the codes'
rules for such angles cover it; it is not in the FE model); the sideways
shift (a rigid end offset in the analysis, read only at level 4 with the
joint details on — about three quarters of the real towers' joints have
no shift data); the welded plate's extra offset (not analysed).
- [x] A1 (M): the layout's planner run relaxed — the planned shift kept
  even where the joint is not written; where no plan exists, an upper
  bound; kept as an analysis value apart from the fabrication record,
  never shown as a laid-out joint; deterministic, followed after edits.
  Built 2026-11-19, in active development, not approved yet
  (`layout_run::plan_relaxed`, `eccentricities::estimate`): laid out /
  given / estimated / upper bound per end; the upper bound the shift limit
  and, where a plate in the member's own plane would be needed, that
  plate's offset at step 4's limit; a planned welded plate's offset too;
  made again from the file at every run (nothing written).
- [x] A2 (S): the analysis reads the estimates when the option is on;
  results say "estimated" and "upper bound at N ends"; theory and
  safe-side pages. Built 2026-11-19, in active development, not approved
  yet: OHL-Structure's Results › *Estimated eccentricities*
  (`estimated_eccentricities = true` in the layout's record); without,
  with, and with the unknown signs the other way — each member and plate
  its largest usage; NOT ON THE SAFE SIDE said for two or more ends of
  unknown sign at one frame joint and for an end left out.
- [x] A3 (S): measured on the 78 real towers (ends estimated against
  upper bounds, the usage change, the largest moments; against the real
  shifts of level-4 towers). Measured 2026-11-19 (kept privately): the
  planned shifts almost all 0, the upper bounds about a quarter of the
  ends and the whole effect; usages rise a great deal; on some towers the
  solve with the upper bounds does not settle (the case fails, said); the
  level 4 offsets of laid-out joints equal the estimates' for them.
  **Proposed: stays off by default** until the questions below.
  Decided 2026-11-19 (owner: "Okay go with recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-owner-22-recommendations-approved)):
  (1) **an offset of unknown sign taken only in its member's own check**
  — built the same day, in active development, not approved yet: the core's
  `Member::free_offsets`, never in the solve, the moment N·e whole and
  sign-free at frame joints too (on the safe side for the member,
  statically admissible for the others); two solves instead of three; the
  mixed-sign note gone. On the 78 real towers (kept privately), one
  vertical case: towers solving with the option 64 → 72 of 73 (the one
  left fails without it too), the members whose usage rises 86 % → 27 %,
  the towers' largest usage with it median 0.84 → 0.58; (2) the upper
  bound stays the 30 mm shift limit plus the plate's offset, until J5;
  (3) towers that failed to solve were accepted until (1) — reported as
  failed (none left but one that fails without the option); (4) the
  estimates stay in OHL-Structure only, the core's line calculation does
  not read them; (5) on by default only after (1), J3 and the bent plate
  (all three built now), measured again with wind cases — **to do**: that
  measure (the comparison set has one vertical case).
- [ ] A5: measured again with wind cases, then on by default (the owner's
  decision of 2026-11-19).
- [ ] B (M): the bolts worked out from the forces through the code
  plugins, iterated (two or three rounds), if the presets prove too rough.
- [ ] D (M): the out-of-plane seat offsets as rigid end offsets in the FE
  model, measured once as an option (needed for the welded plate's
  offset, O44).
Decided 2026-11-19: "estimated eccentricities" an option at level 3, off
until measured on the 78 towers, then on by default; where no layout
fits, an upper bound — the 30 mm shift limit, or the plate case's offset
where a plate would be needed — said "upper bound"; the bolts from the
class presets first, from the forces (B) next if too rough; the seat
offsets measured once as an option (D).

### Done and taken out of this list
Built, in active development (not approved yet).
- **O5. Tension-only parts that stay nearly free** (decided 2026-10-19):
  a tension-only member carries compression up to its own buckling load,
  so it always solves (U1); the energy-based active-set update stays for
  later (U1, Structure analysis — known limitations).
- **O6. Clearances to the structure body** (2026-10-25, owner's details
  of 2026-10-19): Checks › Internal clearances… and `ohl internal` — the
  conductors, jumpers and strings' live ends against every member, to the
  member's surface; the volumes on the strings (sphere, cylinder, box,
  cone; grounded, electrified, insulated; freedoms; several strings in
  one assembly), exact on the convex solids; rules `[[internal_rule]]`,
  by voltage or by the system's insulation level (2026-11-11), another
  distance with wind; a user guide page of its own (2026-11-12);
  [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/internal-clearances.md).

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
  *2026-11-15 (review of the member wind on inclined members; owner: "go
  ahead with your recommendations"), built, in active development, not
  approved yet:* SIA 261's inclination factors (Tab. 74) built in for every
  non-round shape and sin θ for round ones, tapered below 30° with the
  projected area, curves per shape in the library's `[wind]` so a code
  plugin can give its own, sin² kept as an option; without a given drag,
  SIA 261's force coefficient for the shape (Tab. 71) instead of 1 — the
  member wind up by about 16 to 24 % on the reviewed towers.
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

3. **Structures checked** — the codes moved into plugins behind the
   code-check interface (O4); OHL-Structure's open points (U4–U6).
4. **Line and structures together** — interaction levels 3 and 4 (O7).

## Detailed record

### Phase 5 — Advanced engineering modules
*Goal: feature parity with professional workflows.*

*Status: built as a base, in active development (not approved yet). Each item says what is simplified or left for later.*

- [x] `ohl-structures-fe`: 3D frame/truss FE analysis of lattice towers and poles (geometric non-linearity, guys) — standalone module. *Base:* `.ohlfe` models (truss, 3D Euler–Bernoulli frame, tension-only cables with pretension, supports, springs), first order or P-Delta (geometric stiffness, Newton iterations with exit clauses), skyline LDLᵀ with RCM ordering (1000 members in milliseconds), member usage (tension, compression with Euler buckling, linear N + M interaction; a member's own capacities and effective length, angle legs by structure directions, a dead-load factor on the self-weight), attachment nodes fed with each load case's factored loads (`fe` check in the structure usage, `ohl structures fe`, the Results tab of OHL-Structure). *Simplified / later:* no corotational large rotations, wind on the structure body only on profiles from the cross-section library (drag per shape, face drag by solidity and shielding of the faces behind only when the cross-section library's `[wind]` gives them — from a design-code plugin or typed; else one drag factor, no shielding), no code column curves or angle-section eccentricities, no member-load cases beyond self-weight and the load-case recipes, no 3D view of the model in OHL-Design. *Since then, built, in active development (not approved yet); added to this page 2026-11-07:* member capacities by ASCE 10-97 in the core, the default (2026-10-27; column curves, end eccentricity and restraint, angle legs' local buckling, bolts and net section; [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/asce10.md)), with its torsional and torsional-flexural buckling by the profile's symmetry (2026-11-03); SIA 263:2013 as a second capacity method beside it (`capacity = "sia263-2013"`, 2026-11-03, owner; [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/sia263.md)); the end conditions and bolts taken from the joints where they are defined (O19)
- [x] **Joint stabilisation**: lattice towers drawn as trusses solve — joint directions no member stiffens (coplanar or collinear trusses, a frame chain free to spin) are held when the model would otherwise be a mechanism, and the force they take is reported; a load no member holds is refused (a hold to the ground; a hold relative to its line is flagged above the limit, not refused — owner, 2026-10-19); true mechanisms are named with the joints that move *(since 2026-10-19, U6, not approved yet: a joint on a straight line of members is held relative to the line's ends, following its face with P-Delta; the others to the ground)*
- [x] **Tension-only members** (`kind = "tension"`, the "tension-only" members of tower programs): a truss in tension and in compression up to its own elastic buckling load π²EI_min/L_k² (L_k: `lk`, else `k` × length, k = 1 pinned), **bowed** beyond it carrying that load (slack without a second moment); solved by active-set iterations on that cap (members beyond it bowed, those loaded less put back, first order then P-Delta); members on one line through a joint switch as one tie, split into their members when the set goes round or a half hides compression; bowed members named in the notes, usage, `ohl structures fe` and OHL-Structure's results (benchmark `structfe-011`); tension-only members that bend (`bending = true`, a tower program's tension-only beams: frames while in, trusses bowed or slack; benchmark `structfe-012`) and members not checked (`check = false`, its fictitious members), `.ohls` 6, `.ohlfe` 3. *See Known limitations below:* the last resort
- [x] Wood/steel/concrete pole libraries and checks. *Base:* `.ohlp` poles (taper by diameter or circumference, fibre stress for wood, moment capacity at the ground line or by height, hollow wall), moment along the pole from the factored attachment loads (both directions, offsets) plus wind on the pole, P-Delta amplification from a tapered frame model (`pole` check, `ohl structures pole`). *Simplified / later:* one amplification factor for every height, no foundation/embedment check, no shear or deflection limits, no pole class tables (user data). *2026-11-03 (owner), built, in active development (not approved yet); added to this page 2026-11-07:* hollow steel pole shafts by ASCE/SEI 48-19 (the default for them, [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/asce48.md)) or by SIA 263:2013 (`pole code`), wood poles by SIA 265:2012 ([theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/sia265.md)); member buckling of a steel shaft by SIA 263 said not covered

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
- [ ] Code checks of members and joints as plugins (e.g. SIA 263) *(2026-11-11, built, in active development (not approved yet): the interface — `member-check`, `joint-check`, `bolt-distances`, 2026-11-18 `member-check-full` — with an example plugin; SIA 263:2013 and ASCE 10-97 moved into the bundled plugins `sia263` and `asce10` on 2026-11-18, with `joint-check-full`, and their bolt capacity for the bolt tools (`bolt-distances-full`, third round); not yet: the pole codes, O4)*
- [ ] **Fully integrated structure editors with a 3D preview** for every detail level (1 stick figure, 2 sticks of the structure, 3 full design), all in one window: geometry, members, sections, materials, supports, strings and wire points, the type's use in the line, checks — lattice towers and **poles of wood, steel (and concrete)** alike *(base: OHL-Structure, with poles in Body › Pole and Design › Pole, sections and materials typed; still to come: checks per level, the use in the line)*

### Structure–line interaction — levels of computation
*Agreed with the project owner (2026-10-10), levels defined 2026-10-17 as is common in line design software: how far the line and its structures are computed together, chosen per run.*

- [x] **Level 1** — ruling span: sections sagged by the ruling span method on rigid structures
- [x] **Level 2** — FE of the wires on rigid structures (`ohl-wirefe`, what OpenOHL has today); the conductors' forces put on the structures, every structure checked once *(today's structure loads and usage)*
- [ ] **Level 3** — FE of the wires with a linear stiffness matrix at each structure's suspension points, derived from the structure (level 3 structures only) *(built, in active development, 2026-11-16: the structure loads, wire tensions, string checks and the loads sent to OHL-Structure; the clearances still rigid, said)*
- [ ] **Level 4** — the full solve: structures and line iterated until both converge (displacements and forces between passes below tolerances; exit clauses, damping, reports of what did not converge) *(built, in active development, 2026-11-16: the same checks as level 3; a structure failing at level 4 taken linear and reported failing; the clearances still rigid, said)*
- [ ] The level chosen per line (Line › Criteria › Calculation options) and per run (`--structures-as`), shown with every result that takes it; a structure below level 3 is rigid at levels 3 and 4, and the result says so *(2026-11-16; per check and per structure group later)*

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
- **277 validation cases pass** (count of 2026-11-18, `ohl validate
  benchmarks`; the last added 2026-11-18: `sia263-001…003`, run through
  the bundled plugin; with O22, 2026-11-11: `tbbim-003`,
  `finstr-001…004`, `saglim-001…002`, `hotcreep-001…003`,
  `ptforce-001…002`, `tbgap-001…003`) and there are 27 theory pages; the
  private Swiss plugin and the importers of another program's files live
  outside the public code.

### Just finished (2026-11-14)
All built, in active development (not approved yet):
- **Check windows** (2026-11-12): each with a user guide page of its own
  opened by its user guide buttons, the help beside each field (the grey
  lines behind a (?)), units in the labels, what is not on the safe side
  said; the swing arc on by default in the clearance, vegetation and
  safety volume checks; the field windows in three pages (3D volumes, 2D
  profiles, 2D cuts); plugins' help pages of their own.
- **The built manual inside the applications** (2026-11-12): Help and
  the user guide buttons open it offline.
- **A CAD navigation scheme** and a red snap dot where a click would
  pick, in both applications (2026-11-12).
- **Version management** (2026-11-10): one window for every merge,
  library files merged entry by entry, keep both offered.
- **Packaging** (2026-11-11): a draft release workflow with the Windows
  installer, the macOS disk image built locally.
- **Safe side** (2026-11-08 to 11-09): checks not done are said, never
  passed; clauses not covered named; clearance distances growing with the
  altitude.
- **Post insulators** (2026-11-14): on structures, fixed or braced, their
  load to their base with its moment, their ratings checked, a jumper at
  their top at a fixed exit angle; a jumper's load on a clamp fixed to the
  structure no longer dropped.
- **Staking table** (2026-11-14): Line › Structures › Staking table… and
  `ohl staking` — stations, offsets, line angles, spans, coordinates,
  ground, bearings, and the feet of the typed structures.
- In Part A: O22–O26 in part, the plans (O14). In Part B: level 4 and the
  joints in the calculation, the code-check plugin interface (O4).

### Just finished (2026-11-07)
All built, in active development (not approved yet):
- **One shared UI crate**, `crates/ohl-ui` (2026-11-07, owner): the
  look, messages, notices, long tables, the artwork's drawing and the
  glyph test's helpers, used by both applications; still to move: the
  hotbar's drawing and the check footer
  ([A shared UI crate](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/ui-shared.md)).
- **Duplicate code merged** (2026-11-06): helpers that had been copied
  around (CSV fields, the 3D cross product, the tick menu, the
  out-of-date test, the weather case match and others) have one
  definition each; no change of method or result. The copies between
  the two applications that stay are decided in the same page.
- **The docs checked as a whole** (2026-11-04): every page in the book,
  every `ohl` command in the overview; measuring with two angles for two
  points and an Angle tool for three.
- In Part A: the thermal rating at a current, the conductor surface
  gradient in 2D cuts, the clearance runs counted and faster, the radial
  ground clearance within the reach, the wire solver's relaxation
  without its own clock, the sheet templates explained with drawings,
  the plans' headings and scale bars, the field windows' shared setup.
  In Part B: the joint details, SIA 263 and the pole codes, the load
  cases per structure.
- **GitHub issues tidied** (2026-11-07): the epics #28 (tower and pole
  FE), #44 (the renderer), #48 (version control) and #50 (roles and
  permissions) closed as built bases, their follow-ups on this page (O4,
  O7, O19, O20; Phase 3's renderer items; Server and version control —
  more capabilities); #12 retitled "Creep: a time-based law" (Phase 1);
  #34 (stable plugin SDK and registry) and #35 (theory manual and
  validation report) still open (Phase 6).

> **Owner:**

## Open points — waiting on a decision

*Done and taken out:* O2, the project folder renamed on disk (2026-10-18;
folder, repository and website renamed, formats, crates, plugins and
local data ols → ohl).

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
--load-cases`, the Load cases library; such a case reaches
OHL-Structure as a library case of the structure's groups (since
2026-11-15; before, also as an unfactored line result from Criteria ›
Loads for structures, now gone). Still to come: the codes' security
and construction cases written by their plugins (the format is
documented for them), a single subconductor of a bundle broken, the
dynamic impact beyond a factor.
*Status (2026-11-03; added here 2026-11-07), built, in active
development (not approved yet):* `.ohllc` version 4 — several broken
wires in one case, solved together per section, and **removed**
conductors (the wire absent from the solve) apart from broken ones
(owner: broken only for conductors that actually break); a broken-wire
case sent to OHL-Structure breaks all its wires and leaves its removed
conductors out.
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

### O17. A database as the model's backbone — the country server
In a standalone server mode, the project's model kept in a database
(instead of the project file) through a connection the server holds, so
that other software can read and write the line and OpenOHL fits into a
larger ecosystem (asset management, GIS, planning tools). Stronger
version validation goes with it: every change checked against the
revision it was made from, the model's schema version checked at each
connection, and writes from outside validated as the session's commands
are. Since 2026-11-18 this is the country server below (PostgreSQL, the
server the only writer, the session over a store).
> **Owner:** (2026-10-25) Not now, but in the roadmap: a connection to a
> database as the backbone of the model in a standalone server mode, to
> connect it to existing software and integrate it into a bigger
> ecosystem; it will also need stronger version validation.

**The country server** (plan of 2026-11-16; owner: a whole country in a
database, not all in memory, the user picking the routes he works on and
the rest loaded near them, the computations on those routes, an admin
deciding what each user may see; [ADR-0013](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/adr/0013-country-server.md)).
The plan's 15 questions answered as recommended on 2026-11-16 ("Just
reduce the synthetic test otherwise ok for the plan": PostgreSQL; a line
of the network is what a project is today; ids unique, clashes renamed
with the line's code; default deny with see-grants; a hidden neighbour
computed on the server and reported without its details; shared towers
checked under each line's cases, the worst kept; fields pulling in lines
within 500 m; plugin checks declaring an optional reach; PostGIS;
offline working sets without reservations at first; library requests as
pending drafts; the import keeping the current state; the server's
project lines called branches; Compose and OIDC, the owner's IT
deciding). Started 2026-11-18 (owner: rights and roles "applied to almost
every element and library"; the single-user model saved in today's
format; every model and element of a server in PostgreSQL, each model
exported on its own as a project file). Sizes: S about a session, M two
or three, L four or more. The phases, in order:
- [x] 0 Decisions: ADR-0013, its amendments, the database's tables drafted
  (2026-11-18).
- [x] 1 A synthetic country of the owner's size (12 000 pylons, 2.5
  systems a pylon), deterministic, and today's baseline: open 25 s, an
  edit about 32 s (2026-11-18).
- [x] 2 The store interface under the session (SQLite), files unchanged
  (2026-11-18). Left: the point-cloud and background stores still open
  the file themselves.
- [x] 3 Commands applied in place and validated locally (what a command
  touches and what refers to it): an edit 30–60 s → 2–4 ms on the
  synthetic country, open 25 → 1.8 s; a part of a line in a file of its
  own (2026-11-18).
- [x] 4 The PostgreSQL store (`ohl-store-pg`), the same store tests on
  both stores, `ohl network import` (ids renamed with the line's code,
  twin towers listed) and `ohl network export` of one model as a project
  file (2026-11-19). Decided 2026-11-19: one model is one store, branches
  per model until phase 11; the objects' JSON kept as text, not jsonb;
  twin towers listed within 0.5 m with the same type, their height and
  orientation differences shown, never used to hide a pair, the admin
  deciding; the import makes the "imported from FILE" revision by
  default, `--no-revision` to leave it out (built); renamed ids nested
  accepted, never the same code twice; `tools/check.sh` requires the
  PostgreSQL tests on this machine (`OHL_REQUIRE_PG_TESTS=1`; the CI stays
  manual and skips them); `ohl network init --name`. To build of these:
  the twins' differences shown, the check script's requirement, `ohl
  network init`.
- [x] 5 The server on the database (`ohl network serve`): every model in
  memory, each command one transaction, the stream by interest (models,
  routes, tiles) with heartbeats and catch-up, tile packages built by
  packager threads (each section solved alone, the same to the bit), the
  web client by tile, protocol 3 (alpha clients refused); every memory
  and latency target met on the synthetic country (2026-11-19). Decided
  2026-11-19: the context case solved ahead when the server is idle (in
  phase 6, low priority, its memory bounded); catch-up positions for
  changes without a log entry: fine; tiles of level 2 (1 024 m) and a
  context case of 15 °C still air as a network setting, the context wires
  labelled a drawing — never a result, never used by a check; one model
  per line stays the recommendation, and the packager's copy of a whole
  line goes with phase 6 and O42's in-place work; default deny: only
  users an admin added may join a network, even as viewers (today a
  newcomer is a viewer of everything until phase 7).
- [x] 6 The client's working set and context (L): the working set picked
  as whole routes (a list or the map), a part of a route by station range
  or from one structure to another, or an area drawn on the map — always
  snapped out to whole tension sections, shown before it is confirmed,
  saved by name (owner, 2026-11-16); context tiles around the camera,
  read-only (LRU); a partial replica fed by the stream (a route newly
  added caught up); "add this route to my working set"; the desktop's
  single file unchanged. With it: the idle solve of the context case, no
  whole-line copy in the packager. **Built 2026-11-19, in active
  development, not approved yet** ([user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/working-set.md),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-country-server-phase-6),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-6-working-sets-and-context-2026-11-19)):
  File › Join a network (picks shown as the server snaps them, saved by
  name, network schema 3); the replica a project file of today's format
  holding the working set and, read-only, what its sections' solves read
  (they solve as on the whole line, tested), fed by rows (what changed,
  came, went; joining again: only what differs, by a hash of each
  object); File › Working set (a route of the same line added, caught
  up); context tiles 5 × 5 around the camera, 64 MB, drawn grey, a
  drawing; the idle solve (the synthetic country's 26 543 tiles in 60 s,
  78 MB of shapes); each tile's part of a line for the packager instead of
  a copy of the whole line. On the synthetic country: a third of a route
  loaded in 0.08 s (1.8 MB), a move's rows in the replica p95 24 ms,
  joining again after 20 moves 0.35 s. Decided as conservative (the
  owner's questions below): other sections on its structures come as
  context, not as members; a working set is on one line; a 64-bit hash
  per object; protocol 3 with messages added. Left: checks on a working
  set pull no neighbours in (phase 10); the extent worked out per change
  costs the model's size (an index by route next; matters for a country
  as one model).
  Decided 2026-11-19 (owner: "Okay go with recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-19-owner-22-recommendations-approved)):
  other sections on a working set's structures are read-only context
  (E17); one line per working set until phase 11 (E18); the 64-bit FNV
  hash for joining again, for now (E19); protocol 3 (E20); 5 × 5 context
  tiles and a 64 MB cache, a setting later (E21); **a replica's import
  entry written with an empty command**, as the network's import (E22) —
  built the same day, in active development, not approved yet: the whole
  route's replica on the synthetic country 3.48 → 1.98 MB (the line's own
  copy 2.6 MB).
- [x] 7 Rights and roles (M): roles as named permission sets (shipped:
  viewer, designer, checker, library maintainer, line admin, network
  admin; permissions see, edit, create, delete, review, export, lock,
  request, manage rights), assignments of a user or group, a role and a
  scope (the network, a region, a line, a route or part of one, an object
  kind, an object, a library, a library file, an entry), default deny, an
  explicit deny set by network admins only, one function for the commands
  and the server's outgoing filter (the tile packages' rights class),
  "see as user", the audit; property tests that no message holds a hidden
  id or coordinate. **Built 2026-11-20, in active development, not
  approved yet** ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-phase-7),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/server.md#a-networks-server-in-active-development),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-7-rights-and-roles-2026-11-20)):
  the one function `ohl_session::rights` (a scope a conjunction of
  constraints; the union of grants, see needed, a deny wins whatever the
  order); every server operation with a rule (an exhaustive match) and
  every object a command changed decided as it was and as it is, in the
  same unit of work (refused: nothing kept); everything the server sends
  filtered by the same function — lines and routes listed, tile packages
  per rights class (the exact set hidden in a tile), changes ("read these
  tiles again" for who may not see all of one), catch-ups, working sets
  and their rows, route details, error messages, the users' presence and
  locks; only the network's users join, with their personal invitations;
  rights changed by `ohl network rights …` (network schema 4), audited,
  taken by the running server before its next job, a client whose
  rights changed let go; "see as user" for who manages rights (both
  users' rights). Decided as conservative (the owner's questions below):
  a region grants what lies inside it and denies what it touches; a
  grant on a route, region or object lets see its lines' settings and
  library files; the line as a whole needs a scope of the whole line;
  branches, commits and tags need edit on the whole line; a part of a
  route's grant does not make new objects; an export needs export on the
  whole line; users, groups and roles by network admins only. Measured
  on the synthetic country with 204 users and 1 297 assignments: a
  command's rights decided p95 23 µs, a tile's rights class p50 28–39 µs
  and its package filtered p50 0.18 ms (once, then kept), a move applied
  and broadcast p95 9.8 ms (phase 5: 10.3 ms), the start 2.5 → 2.8 s.
  > **Owner:** (answered 2026-11-20: as recommended — "Go ahead with
  > recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-owner-country-server-phases-7-10-11-and-12--recommendations-approved); questions 1 and 2 built with phase 11
  > as recommended; the recommendation after the
  > arrow) 1. A designer of a part of a route making new objects in it?
  > → yes, with phase 11; a whole route until then. 2. Branches, commits
  > and tags need edit on the whole line until phase 11? → yes. 3. Users,
  > groups and memberships by network admins only? → yes, as built.
  > 4. Export only of the whole line? → yes; parts as offline working
  > sets (phase 12). 5. A client let go when its rights change? → yes, as
  > built. 6. Presence and locks hidden from who does not see everything?
  > → now; filtered per object with OHL-Server stage B. 7. A project's
  > edit grants turned into assignments at import? → yes (later), both
  > checked until then. 8. "See as user" for whoever manages rights,
  > within what both see? → yes.
- [x] 8 Towers shared by several routes: a change that breaks another
  line's connection refused, naming it; a change that only moves it
  allowed, its results marked stale and its owners told; protected lines
  refused pending their owners' review; a merge breaking a connection is
  a conflict; OHL-Design's editable routes, notices and stale marks
  (2026-11-18).
- [x] 9 Country libraries (L): kinds locked for the whole server, their
  maintainers, requests for additions (may be refused; a pending draft
  usable in the requester's own branch, blocking review and merge),
  releases, pins (built lines pinned, designs in progress following the
  latest), *Update library pins*, withdrawn entries, a use index, the
  import's mapping of today's per-project libraries. **Built 2026-11-20,
  in active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-phase-9),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/server.md#country-libraries-phase-9-in-active-development),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-9-country-libraries-2026-11-20)):
  each kind kept once for the network (its files as they are now,
  releases kept as blobs once), a line holding the files of the release
  it is pinned to as its own library files (the engine and the `.ohl`
  format unchanged; an export stands alone, `network.library_pins` and
  `network.library_withdrawn` in its meta); kinds or files locked (a
  line's own file of a locked kind refused by the server; unlocking by
  network admins; an emergency change by a network admin, audited);
  maintainers as assignments (*review* and *lock* on the kind); requests
  granted or refused by a maintainer other than the requester, seen only
  by the requester and the maintainers (a draft only by its writer, the
  line hidden from a maintainer who may not see it); a pending draft a
  file `request-N.ext` in its requester's own branch only, blocking its
  review and merge into main until granted or taken out; releases that
  change no line; *Update library pins* (`ohl network library
  update-pins`, the server's `UpdateLibraryPins`) as one logged command,
  the routes using a changed entry marked out of date, a built line moved
  only by whoever reviews it; withdrawn entries kept by the lines using
  them, never newly used; a use index kept with every command and read
  through the rights (no hidden entry or line); the import mapping each
  entry (the same, added to a kind not locked, or asked for — never
  replaced); network schema 5; `ohl network library …`. Measured on the
  synthetic country: Update library pins of its 147 lines 3.3 s (per line
  p50 21 ms), the use index of the wire every span uses 0.31 s (a
  route's viewer 39 ms), moves and working sets as fast as with phase 7's
  code (a move p50 9.1–9.7 ms against 9.1–9.8 ms). Left: Update library pins in
  OHL-Design, notices when a release makes a line behind, the
  engineering impact before a pin moves, a line's file filtered per
  entry, branches' use index (phase 11), OHL-Server's Libraries page
  (stage D).
  > **Owner:** (answered 2026-11-20: as recommended — "go ahead with
  > recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-owner-country-server-phase-9s-10-recommendations-approved);
  > question 9 and the branches' use index built with phase 11; the recommendation after the
  > arrow) 1. A release moves no line; designs in progress follow by
  > `update-pins --following`, shown behind? → yes, as built. 2. Unlocking
  > by network admins only? → yes. 3. A grant by a maintainer other than
  > the requester? → yes. 4. A draft "replaced" = taken out of the branch;
  > a withdrawn request's draft still blocks? → yes. 5. A built line's
  > pins moved by whoever reviews the whole line (no review flow of its
  > own yet)? → yes for now; a library review with OHL-Server stage D.
  > 6. Update library pins not undone by Undo (updated back to the
  > release before)? → yes. 7. A withdrawn entry stays in releases, never
  > newly used; results using it marked "not on the safe side" later? →
  > yes, the marks with the app's library notices. 8. The import adds new
  > entries of unlocked kinds, asks for the rest, keeps the model's own
  > files until Update library pins? → yes. 9. A draft only in the
  > requester's own branch? → yes; a team's branch with phase 11. 10. A
  > line's library file sent whole to whoever sees it (an entry's deny
  > holds on the network's library)? → filter per entry with OHL-Server
  > stage D; deny the file until then.
- [x] 10 Scoped computations (L; an engine change and an optional plugin
  interface step, approved 2026-11-20: "You can go ahead with
  recommendations and the changes to pull nearby lines"): each check's
  own reach, neighbours pulled in, other lines' wires as targets,
  structure loads with other lines' sections, hidden neighbours computed
  on the server, results saying what was pulled in and marked stale when
  it changes. **Built 2026-11-20, in active development, not approved
  yet** ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-phase-10),
  [theory](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/theory/scoped-checks.md),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/server.md#checks-with-their-neighbours-phase-10-in-active-development),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-10-scoped-computations-2026-11-20)):
  `ohl_scope` (each check's reach; the candidates by a bound of each
  span's wire before it is solved; clearances between wires with this
  line's and other lines' wires as targets, crossings among them; the
  field with the lines within 500 m, added by size, what lies beyond
  said NOT ON THE SAFE SIDE; the loads on shared towers — twins — under
  each line's cases, the worst kept); the network's server computes them
  on what it holds (each part as solving it alone reads it, never a whole
  line), a hidden neighbour computed too and only counted (never named
  nor placed), every result listing what it pulled in and watched — told
  stale once, OHL-Design marking the working set's routes out of date
  (phase 8's marks); a check run on the replica alone says NOT ALL
  CHECKED where its reach needs more; plugins' optional `[reach]` (none:
  the whole line and every line within 2 km). Proven on the synthetic
  country: scoped clearances equal to the whole network's to the bit (the
  same pairs, distances, violations), shared towers' loads to the bit;
  the property that no message holds a hidden id or place extended to
  checks. Measured on
  the synthetic country: a whole route's clearances 9.6 s, its field
  8.4 s, a shared tower's loads 0.39 s; moves, joining again and replica
  loads as fast as with main's code.
  Left: OHL-Design's windows asking the network for these checks; a
  worker of their own (today on the authority between commands); the
  ground and points on the network; rules across weather cases; the
  field's left-out part bounded; shared towers at level 3/4.
  > **Owner:** (answered 2026-11-20: as recommended — "Go ahead with
  > recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-owner-country-server-phases-7-10-11-and-12--recommendations-approved); question 1 built with phase 13: the
  > checks' worker thread; the recommendation after the
  > arrow) 1. Checks computed on the authority between commands (a long
  > check holds the edits)? → a worker thread of their own next. 2. The
  > field's wires beyond 500 m left out, said NOT ON THE SAFE SIDE? →
  > keep, add a bound of what is left out later. 3. A wire at most 1.5
  > times its ends' distance for the candidates (checked on what is
  > solved)? → yes. 4. Several lines' sections at one point added, their
  > strings counted again; other lines' sections in every case the twin
  > takes? → yes. 5. A plugin without `[reach]`: the whole line and every
  > line within 2 km? → yes. 6. A result stale when anything changes in
  > its reach's tiles (more often than needed)? → yes. 7. A hidden
  > neighbour's distance said ("within X m")? → yes, as the plan's answer.
- [x] 11 Version control at scale (M–L): trees split, branches over
  working sets, merges at shared towers. **Built 2026-11-20, in active
  development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-phase-11),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/server.md#branches-over-a-working-set-phase-11-in-active-development),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-11-version-control-at-scale-2026-11-20)):
  **split trees** on a network's models (a trie of the objects by the
  hash of their kind and id, 64 objects a leaf, the library files in the
  root; the desktop's project file keeps flat trees, both read
  everywhere): a commit hashes and writes what changed since the line's
  last tree, a diff or a merge reads the nodes and objects that differ,
  ahead/behind and merge bases exact as before; **merges from what
  differs**, validated as a command is (locally), library files' merges
  the whole way; **branches over a working set** (`CreateWorkingSetBranch`:
  its routes, its maker, a team), only changes stored, the rules of
  shared towers on every command with its routes, its stale marks written
  when it reaches main; **merges at shared towers**: a broken connection
  a conflict, named; a move marking the other routes stale and their
  connected owners told; a protected main waiting for a review. With the
  owner's questions left for phase 11: a commit needs edit only on what
  it records, a branch over a working set its routes, its work its maker
  and team, its merge into main what it changes there (tags and
  whole-line branches the line whole, as before); a part's designer makes
  new objects inside the part; a draft usable in a team's branch; the
  branches' use index (only their changes). Network schema unchanged (5).
  Measured on the synthetic country as one model (108 785 objects): a
  commit of one changed route 6.7 s → 0.10 s, its status 1.4 → 0.11 s, a
  branch's diff 134 → 10 ms, its merge 33.8 → 0.12 s, a merge at a shared
  tower 10.7 → 0.08 s (flat trees, as phase 9's code writes them, against
  split ones); moves, replicas, joining again and Update library pins as
  with phase 9's code.
  Left: a branch's line in memory is a whole line (its JSON shared:
  a branch the size of its working set with phase 12); reverts and
  rollbacks read whole states; history, diffs and reviews served to a
  network's clients and OHL-Design's window for these branches. Its two
  open items **done with phase 12** (2026-11-20): the rules of shared
  towers on edits made directly on a network's main line, and a
  branch's base read along what differs (opening the country as one
  model with branches 20.8 → 4.7 s).
  > **Owner:** (answered 2026-11-20: as recommended — "Go ahead with
  > recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-owner-country-server-phases-7-10-11-and-12--recommendations-approved); the recommendation after the
  > arrow) 1. Split trees only on a network, the project file keeping flat
  > ones? → yes, as built. 2. A model exported keeps its split trees
  > (readable by this build on), or is flattened (new revision ids, the
  > bundle round trip lost)? → keep them, as built. 3. Tags, whole-line
  > branches, rollbacks and reviews' merges still need the whole line? →
  > yes for now. 4. A commit decided object by object (it records others'
  > uncommitted work only if its user may edit it all)? → yes, as built.
  > 5. A branch's stale marks written when it reaches main? → yes, as
  > built. 6. A merge into a protected main moving another route's tower
  > waits for a review, even by a line admin? → yes, as built (phase 8's
  > rule). 7. A team named by users and groups, a draft's requester named
  > in it? → yes, as built. 8. Merges changing library files the whole
  > way? → yes, until library files merge entry by entry on the trees.
- [x] 12 The offline working set (M): exported with read-only
  neighbours, back as a bundle. **Built 2026-11-20, in active
  development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-phase-12),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/server.md#offline-working-sets-phase-12-in-active-development),
  [commands](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/network-commands.md#offline-working-sets-phase-12),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-12-the-offline-working-set-2026-11-20)):
  `ohl network export-working-set` (the server's `export_working_set`)
  writes a working set as a project file of today's format — the working
  set editable, its context and the line's own neighbours within the
  checks' reach read-only, other lines' neighbours (what solving their
  sections there reads, with their wire and case files) in
  `network.neighbours` — filtered by the rights (the working set seen
  whole, export on each of its objects; what its user may not see left
  out and counted, never written), its base the main revision (the main
  line's changes recorded first by phase 11's rule), the export recorded
  (`offline_export`, network schema 6: its tree's hashes, no object
  copied) and audited, no reservation; `ohl network check-offline` runs
  the scoped checks on the file with the server's own code (the same
  report, tested to the bit, or NOT ALL CHECKED where the file cannot
  give it); `ohl network offline-bundle` makes a bundle of only the
  changes; `ohl network import-bundle` (the server's `import_bundle`)
  verifies it against the recorded export and applies it on a new branch
  over the working set made from the export's base — never on main — as
  one command decided by the user's current rights and the rules of
  shared towers, then committed; conflicts with main listed, never
  settled; read-only neighbours, library files, a bundle changed by hand,
  of an unknown base, another user's or brought back twice refused,
  nothing kept. With it, **phase 11's two open items done**: the rules of
  shared towers on commands made directly on a network's main line (the
  routes its user may edit something on, narrowed to the working set its
  connection follows; the same code path as a branch), and a branch's
  base read along what differs from the main line (opening a model with
  branches, a line made from a revision). Measured on the synthetic
  country: a whole route exported in 0.25 s (7.7 MB, 15 other lines near), bundles of 20 / 500 changes brought back in 0.29 / 0.66 s, the country as one model opened with its branches 20.8 → 4.7 s; moves, replicas, joining again and the start as with main's code.
  Left: ~~OHL-Design's Offline copy menu for a network~~ (built
  2026-11-21, phase 16); the export on a thread of its own (built with
  phase 14); ~~OHL-Design's
  check windows using a file's neighbours~~ (built 2026-11-23, below);
  ~~a revision of main's changes made by the server so any designer can
  export~~ (built 2026-11-23, below).
  > **Owner:** (answered 2026-11-20: as recommended — "Go ahead with
  > recommendations"; [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-owner-country-server-phases-7-10-11-and-12--recommendations-approved); question 6, the export on a thread of
  > its own, still to build: see phase 13; the recommendation after the
  > arrow) 1. Edits directly on main: a user's routes those they may edit
  > something on, narrowed to their connection's working set; who may
  > edit every route and follows none as before? → yes, as built.
  > 2. An export records main's unrecorded changes first, only by who
  > may edit them all? → yes for now; a server-made revision next.
  > 3. Export on each object of the working set, see on its context and
  > neighbours? → yes. 4. The line's own neighbours all left out when one
  > is hidden? → yes for now; section by section later. 5. The area the
  > working set's bounds grown by the reach (500 m) and three span
  > margins? → yes; a corridor if files grow too big. 6. The export on the
  > authority between commands? → a thread of its own next. 7. A bundle
  > one command on a new branch, the offline revisions not kept? → yes.
  > 8. A bundle twice refused, another of the same export allowed? → yes.
  > 9. Library files and settings not changed offline? → yes.
- [x] 13 Security, deployment, backups (M): a security review of the
  network server (the interest pace, tile limits, queues), Compose files,
  OIDC sign-in, a restore drill, migrations refused without a recent
  backup, TLS to a database on another host. **Built 2026-11-20, in
  active development, not approved yet**
  ([the review](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/server-security-review.md),
  [decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-phase-13),
  [deployment](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/server-deployment.md),
  [commands](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/network-commands.md#operations-phase-13),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-13-security-backups-and-the-checks-worker-2026-11-20)):
  **the review**, 15 findings each fixed and tested or accepted with its
  reason — checks, exports and bundles queued behind the commands (one
  per user, four in all), a check's reach bounded (10 km, a field 2 km),
  new tiles per client and second bounded (64, 1 024 being made), lists
  bounded, large messages within one 256 MB budget (a browser's 8 MB),
  IPv6 limits by /64, no client words in paths, no secret in logs or
  command lines; **invitations** reissued per user (older ones refused,
  the user let go) and with an optional end (`--invite-days`);
  **OIDC sign-in** as an option (`jsonwebtoken`, `ureq`; signature,
  issuer, audience, times; keys fetched and kept; subjects mapped to
  users by network admins); **backups** (`ohl network backup`: `pg_dump`
  in an exported snapshot, a manifest with each model's hash) and
  **restores** into an empty database with **the restore drill** (every
  model compared); **migrations refused** without a backup newer than
  `backup_max_age_hours` (24) unless `--i-have-a-backup` (audited);
  **TLS** to a database on another host (`sslmode`, `sslrootcert`; refused
  without unless `sslmode=disable`); **Compose** files (PostgreSQL with
  PostGIS and a role without superuser, the server, a backup job,
  healthchecks, fake example secrets); network schema 7. With it, the
  owner's approval of 2026-11-20: **the checks' worker thread** (phase
  10's question 1) — a check computed from a snapshot on a thread of its
  own, its report the same as on the authority, a change landing
  meanwhile marking it stale.
  Measured on the synthetic country: main's code against phase 13's on fresh databases of the 147 lines (a shared machine, the quietest rounds): a move applied and broadcast p50 9.2 → 8.4 ms (a route's designer 9.5 → 8.9 ms), a replica loaded 0.076 / 0.130 → 0.074 / 0.120 s, joining again after 20 moves 358 → 355 ms, the start 3.0 → 2.8 s; a backup of the 147 lines 7.0–10.6 s (13.6 MB), a restore with its drill 13.8–14.1 s; **a move while a whole route's clearances run (12 s): on the authority the first waited 12.08 s, on the worker p50 9.6 ms, p95 27.8 ms (836 moves meanwhile)**.
  Left: the offline export on a thread of its own (phase 12's question
  6, approved; built with phase 14); ~~OHL-Design's sign-in window~~
  (built 2026-11-20, below); ~~the web client's sign-in window~~ (built
  2026-11-21, phase 16); a role per
  task in the database; WAL archiving driven by `ohl`; OHL-Server's
  Operations page (stage F).
  > **Owner:** (to answer, 2026-11-20; the recommendation after the
  > arrow) 1. Long work queued behind the commands, one per user, four
  > in all? → yes, as built. 2. A check reads at most 10 km around its
  > working set, a field 2 km? → yes. 3. 64 new tiles a second per client,
  > 1 024 being made at once? → yes, settings of the server. 4. Invitations
  > reissued per user and with an optional end, the old ones valid until
  > reissued? → yes; an end of 90 days by default once OHL-Design shows an
  > expired invitation. 5. OIDC subjects mapped by network admins, no
  > automatic accounts? → yes; from a group claim later if wanted.
  > 6. Backups by `pg_dump` with a restore drill, WAL archiving the IT
  > department's? → yes. 7. A migration refused without a backup newer
  > than 24 h, the override audited? → yes. 8. A remote database refused
  > without TLS unless `sslmode=disable`? → yes. 9. Compose with a role
  > without superuser, the image on PostgreSQL's? → yes; a slimmer image
  > later. 10. The checks' worker from a snapshot of the lines within
  > 10 km? → yes; the export next.
- [x] 14 Scale validation (S): the synthetic country measured end to end;
  a real line's import measured locally only. **Built 2026-11-20, in
  active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-phase-14),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-phase-14-scale-validation-and-the-exports-thread-2026-11-20),
  [commands](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/network-commands.md#measurements-each-but-bench-scoped-edits-the-network-use-a-database-made-for-it)):
  `ohl network bench-scale` (80 clients joining at once, 40 editors
  moving at once while checks and exports are asked) and `bench-offline
  --during-reach`; with it **the offline export on a thread of its own**
  (phase 12's question 6, approved): decided on the authority (rights,
  base, neighbours, replica in one moment), written on the exports'
  thread, recorded and answered on the authority; a change landing while
  it is written marks it stale (`changed_meanwhile`, `stale`) instead of
  being replayed; the same file either way (tested). Found and fixed
  while measuring: **long work starved** behind forty editors (it now
  starts after 2 s, at most once every 2 s) and **a big model's first
  revision** (more than 7 minutes → 25 s: the version store's statistics
  read again during it), and **a line's library import** reading every
  file of a kind again for each entry (about a sixth of the time on a
  real line measured locally). Measured on the synthetic country: a move while
  a whole route is exported with 10 km on the country as one model 1.40 s
  (on the authority) → p50 6.7 ms, at most 714 ms (on the thread);
  checks and exports asked while 40 users edit started after 60 s → p50
  6.0 s; 80 joins at once p50 3.8 s to the whole first view; about 200
  moves a second through the authority; 1.31 GB idle, 2.29 GB with 85
  clients working; a backup 6.2–7.2 s, a restore with its drill
  8.4–10.0 s; moves, replicas and the start as with main's code. The
  real line's import measured locally (results private).
  Left: ~~the export's decision on a big model (0.7 s on the country as
  one model: its extents over the whole model) and a bundle brought back
  there (10 s) still on the authority~~ (built 2026-11-20, below); the
  real country's lines on a server of their own.
  > **Owner:** (to answer, 2026-11-20; the recommendation after the arrow)
  > 1. An export decided on the authority and written on a thread of its
  > own; a change landing meanwhile marks it stale instead of being
  > replayed into the file? → yes, as built (a replay would make the file
  > differ from the base its record names).
  > 2. A user whose rights change while their export is written gets
  > nothing (asks again)? → yes, as built.
  > 3. Long work that has waited 2 s behind commands that keep coming
  > starts anyway, at most once every 2 s (a pause of up to its snapshot or
  > decision for the editors)? → yes, as built; a setting of the server if
  > 2 s proves wrong in use.
  > 4. The version store's statistics read again during a large first
  > revision (ANALYZE after 2 000 blobs, then each doubling)? → yes, as
  > built; a store reading many blob hashes at once later.
  > 5. The export's decision on the country as one model (0.7 s) and a
  > bundle brought back there (10 s) still on the authority? → for now;
  > next an index by place in the session (the extents) and the bundle on
  > the exports' thread.
  > 6. Forty editors moving without a pause is the saturation measured
  > (about 200 moves a second); is that enough for the country? → yes for
  > now (a working day's edits are far fewer); measure again with the real
  > country's lines once they are imported on a server of their own.
- [x] 15 The rest of the exports' thread and the sign-in window (S):
  phase 14's "left" and phase 13's OHL-Design sign-in window. **Built
  2026-11-20, in active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-20-country-server-the-rest-of-the-exports-thread-and-the-sign-in-window),
  [measured](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#synthetic-country-the-rest-of-the-exports-thread-2026-11-20),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/sign-in.md)):
  **an index by place in the session** (each structure's routes, the
  structures by cell, the longest span: an extent looks at its
  candidates only, each tested as before — the same extent, tested
  against the whole scan); **a bundle's branch made ready on the
  exports' thread** (decided and applied on the authority; changes of
  main meanwhile its conflicts, rights changed meanwhile nothing
  applied; the same report and branch as on the authority, tested);
  **a network's database keeps the main lines' spatial index only**
  (writing a whole model's for each new branch was most of a bundle);
  **OHL-Design's Sign in to a network window** (the invitation's end
  said, reissued and expired invitations explained, "Sign in with your
  organisation" when the server offers it: the code flow with PKCE and a
  loopback redirect, no password in OHL-Design, tokens in memory only;
  tested against a fake provider on the loopback); **personal
  invitations 90 days by default** (phase 13's question 4). Measured on
  the country as one model: an export's decision 0.79 → 0.31 s (a move
  during the export at most 804 → 314 ms); a bundle of 20 changes 22.8 s
  on the authority (10.7 s on a quieter round) → 2.4 s asked → answered,
  the authority held about 0.1 s for its decision and 0.1–0.3 s for its
  final apply (a move meanwhile p50 6.8 ms, at most 243 ms); moves,
  joins, memory and the start as with main's code.
  Left: ~~the web client's sign-in~~, ~~a clean-up of the branches'
  spatial rows written before~~, ~~OHL-Design's Offline copy menu for a
  network~~ (all built 2026-11-21, phase 16).
  > **Owner:** (to answer, 2026-11-20; the recommendation after the arrow)
  > 1. An extent narrowed by an index by place, every candidate tested as
  > before (the same extent, tested)? → yes, as built.
  > 2. A network's database keeps only the main lines' spatial index (a
  > branch's in memory; rows of branches written before ignored)? → yes,
  > as built; a clean-up of the old rows with the next migration.
  > 3. A bundle decided and applied on the authority, its branch's line
  > made ready on the exports' thread; the final apply on the authority
  > (it changes the model, decided by the rights and lines as they are)?
  > → yes, as built.
  > 4. A change of main landing while a bundle is made ready: one of its
  > conflicts; rights changed meanwhile: nothing applied? → yes, as built.
  > 5. Personal invitations 90 days by default (`--invite-days 0`: until
  > reissued)? → yes, as built.
  > 6. The organisation's sign-in in the user's browser (code flow with
  > PKCE, a loopback redirect), tokens in memory only, renewed with the
  > refresh token; signing in again after OHL-Design restarts? → yes; an
  > OS keychain only if users ask for it.
  > 7. The sign-in question answered without a new protocol number (a
  > server of before answers nothing: invitations only)? → yes.
  > 8. The web client's sign-in → build it next, with the provider's
  > origin added to the page's `connect-src`.
- [x] 16 The web viewer's sign-in, the Offline working set window, schema
  8 (S): phase 15's "left" and its questions 2 and 8, phase 12's "left".
  **Built 2026-11-21, in active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-21-country-server-the-web-viewers-sign-in-the-offline-working-set-window-schema-8),
  [the window](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/offline-working-set.md),
  [the web viewer's sign-in](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/sign-in.md#in-the-web-viewer),
  [the security review](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/server-security-review.md#the-web-viewers-sign-in-2026-11-21)):
  **the web viewer's "Sign in with your organisation"** (the code flow
  with PKCE in the tab, `web/signin.js`; the page's own address the
  redirect; tokens in the page's memory only, the one-time values of a
  sign-in under way in the tab's session storage until it is back; the
  page's `connect-src` names the provider's origin, from the configured
  issuer only; the provider must allow the page's redirect and its
  origin on its discovery document and token endpoint — documented;
  tested against the fake provider on the loopback, the page's own script
  run in Node.js); **OHL-Design's Offline working set window**
  (Versions › Offline copy: export, check offline, make the bundle,
  bring back — the tool windows' rules, a stale export said in red with
  what changed meanwhile, what changed on the server said on bringing
  back; `ohl-server`'s new `offline-checks` feature, no PostgreSQL client
  in the desktop; tested against a local network's server, its pictures
  made from it as `USER=tester`); **network schema 8** (the branches' old
  rows of the spatial index deleted, a migration after a backup; tested on
  a database holding such rows). With it, O44 L9's loosened layout test
  made exact (each joint's value; the plate's 0.8° to the flange).
  Measured: a whole route's clearances checked offline in OHL-Design
  17.6 s on a synthetic country of 300 pylons (on a thread of its own:
  the window stays usable).
  Left: the web viewer's glue driven in a real browser against a real
  provider (its script tested in Node.js; the page checked to parse);
  ~~OHL-Design's check windows using an offline file's neighbours~~
  (built 2026-11-23, below); a server setting naming a token endpoint on
  another origin, if a real provider needs it.
  > **Owner:** (to answer, 2026-11-21; the recommendation after the arrow)
  > 1. The web viewer's tokens in the page's memory only, a reload signing
  > in again? → yes, as built (a session storage of tokens only if users
  > ask).
  > 2. The page's `connect-src` names the issuer's origin only; a provider
  > splitting its token endpoint onto another origin not reached from the
  > page? → yes, as built; a server setting naming that origin if a real
  > provider needs it.
  > 3. The desktop and the web viewer the same OIDC client (the audience),
  > both redirects registered with it? → yes, as built (one audience the
  > server checks).
  > 4. The Offline working set window under Versions › Offline copy, its
  > four steps in one window? → yes, as built.
  > 5. An export answered after its window stopped waiting writes no file
  > (the export stays recorded on the server)? → yes, as built; export
  > again.
  > 6. An existing file never replaced by an export or a bundle? → yes, as
  > built.
  > 7. `ohl-server`'s `offline-checks` feature, the desktop without a
  > PostgreSQL client? → yes, as built.
  > 8. Schema 8 deletes the old branch rows in the migration's own
  > transaction (no `VACUUM`: the database's administrators or autovacuum
  > give the space back)? → yes, as built.
- [x] Follow-up of 2026-11-21 (S): two server tests that failed under the
  full check's load (and passed alone) made certain; phase 14's second
  real line refused at import ("the project has no line") found and
  fixed. **Built 2026-11-21, in active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-21-an-older-projects-lines-kept-as-differences-upgraded-on-their-whole-state)).
  **The exports' thread held
  at a gate in the tests** (`ExportGate`, tests only) instead of a fixed
  wait of 1.5 s: a move or a rights change made while an export is
  written or a bundle made ready came after it when the test's own steps
  (reading the model, a request) took longer than the wait — the export
  not stale, the bundle applied without its conflict (shown by taking
  the wait away: the same failure); now the work is decided, the change
  made, then the work let through. **The server tests' waits generous**
  (one constant, 120 s instead of 5–20 s; a wait ends as soon as its
  condition holds): the property of rights waited 10 s for a merge's
  answer, which took up to 7.3 s with the machine loaded (measured), more
  in the full check; a fixed 300 ms sleep in the sign-in test is a wait
  for the connection gone. What the tests check is unchanged.
  **An older project with project lines kept as differences opens**
  (and imports): the objects' upgrades run once when a project of before
  2026-11-15 opens (here: the load case sets read into structure groups)
  took each project line's rows for a whole line; a line kept as
  differences holds only what it changed, no settings row of its own, so
  the project was refused — in the network's import and in OHL-Design
  alike, a session bug, not the file's or the importer plugin's. Now such
  a line is upgraded on its whole state (its base revision with its rows
  laid over it) and what changes is written as its own rows; the
  upgrades are marked done once every line is (tested on an invented
  project of the same shape: refused before, open after). The real line
  refused in phase 14 now imports (measured locally only).
- [x] Follow-up of 2026-11-23 (S): phase 16's and phase 12's "left" —
  **OHL-Design's check windows with an offline file's neighbours**
  (owner, 2026-11-23: "go ahead with the server too"). **Built
  2026-11-23, in active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-23-country-server-the-check-windows-with-an-offline-files-neighbours),
  [user guide](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/user-guide/offline-working-set.md#checks-with-the-exports-neighbours)):
  on an offline working set's file, **the Clearances window's rules to
  other lines take the export's other lines** — those within the rules'
  reach solved in every position checked with their own lines' settings
  and wire libraries (the library built by the network's code) — and
  find the same pairs at the same distances as the network's own check
  on the file (tested against a local network's server, a synthetic
  country of 150 pylons with a tower shared by two lines); **never
  silently**: a block before Run and the report's lines say the export's
  date (and number), the lines read, those within reach, and that they
  **may be stale** — with what changed while the file was written when
  OHL-Design wrote it (the server's answer kept in the file as the
  application setting `app.offline_export`; the format unchanged);
  **NOT ALL CHECKED** in red for what the file cannot give (neighbours
  left out, a reach beyond the export, a neighbour not solved, case pairs
  to other lines), and **the verdict never *Every rule met* then** (also
  fixed for a joined working set's replica, where it was a note under a
  ✔). **Structure loads, the fields and plugins name the export's
  neighbours** (the shared towers, the lines within 500 m, those within a
  plugin's reach), NOT ALL CHECKED, pointing to the Offline working set
  window's checks; **found**: the field windows on a joined replica said
  nothing of the neighbours not pulled in — they now do. Measured on the
  synthetic country of 300 pylons (two runs, a shared machine): a
  route's clearances (one weather case, its rules to other lines and
  between its wires) 3.5–3.9 s on the file alone → 5.8–7.3 s with its 2
  neighbour lines (OHL-Design's window); the network's own check on the
  same file 5.2–5.6 s.
  Left: the loads on shared towers and the magnetic field computed with
  the export's neighbours in the windows themselves (now in the Offline
  working set window only); rules to other lines across weather cases.
  > **Owner:** (to answer, 2026-11-23; the recommendation after the arrow)
  > 1. Structure loads, the fields and plugins name the export's
  > neighbours (NOT ALL CHECKED) instead of adding them; the Offline
  > working set window adds them for the loads on shared towers and the
  > magnetic field? → yes for now; next, those two computed with them in
  > the windows themselves by the network's code.
  > 2. The server's answer on the export kept in the exported file as an
  > application setting (not in the line, its revisions or a bundle)? →
  > yes.
  > 3. The neighbours solved in the weather case and wind position of this
  > line's check, each with its own settings and wires? → yes, as the
  > network's checks.
  > 4. Rules to other lines across weather cases still not in the window,
  > NOT ALL CHECKED when the export's neighbours are there? → yes, until
  > the network's checks take case pairs.
  > 5. The verdict NOT ALL CHECKED whenever a reason holds, also on a
  > network's working set joined? → yes.
- [x] Follow-up of 2026-11-23, second (S): phase 12's last "left" — **a
  revision of main's changes made by the server so any designer can
  export** (phase 12's question 2, approved 2026-11-20: "a revision of
  main made by the server itself (audited) when someone exports, next").
  **Built 2026-11-23, in active development, not approved yet**
  ([decisions](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/decisions.md#2026-11-23-country-server-a-revision-of-main-made-by-the-server-so-any-designer-can-export)):
  main's unrecorded changes are recorded before an export as a revision
  by the network's server (`network:server`, a name no user can have),
  after the export's own rights checks, audited under the exporter's
  name (*server revision*), broadcast as any commit, on a protected main
  too (it moves nothing); before, the exporter recorded them and a
  route's designer was refused while others' changes were unrecorded.
  Tested on PostgreSQL: such a designer's export, refused before, goes
  out on the server's revision.
  > **Owner:** (to answer, 2026-11-23; the recommendation after the arrow)
  > 1. The server's revision also on a protected main (it records what
  > main holds; no review)? → yes: it changes nothing a review would
  > look at.
  > 2. Always the server's revision, even when the exporter could record
  > every change themselves? → yes: one author for main's bookkeeping,
  > the exporter named in the message and the audit.
  > 3. Its author `network:server` (not a user)? → yes.

Later: point clouds, backgrounds and terrain tiles in PostgreSQL, served
by tile; the merge of twin towers (an admin's command). A first usable
milestone (a country in the database, users moving about and editing
their routes, checks on the working set said "neighbours not pulled in")
comes after phase 6 with 7 and 8; phase 10 brings the full checks.

**OHL-Server, an app for admins** (plan of 2026-11-18; owner: "Can we
have the server running in its own app which would allow for the gestion
of the roles and an overview map of the model inside but very simplified
mostly oriented for admin? This is a plan to define and not already start
on"). **Nothing built until the owner answers.** A third application, for
admins only (no engineering editing, no 3D, no checks run from it), in
two modes: *host* (the server inside the app, for a small office or a
test; closing asks first) and *connect* (to a headless server, the
production way; the app is never needed for the server to run). Its
pages: Overview; a simplified 2D map (routes by voltage, owner or state,
structures as dots past a zoom, shared towers; overlays of assignments,
working sets in use, locks, stale results; no background by default);
Users and groups; Roles (the permission matrix); Assignments (the scope
picked on the map, in a tree or in the library list; "what changes"
before saving); See as user (the server's own filter); Lines (export one
model, import with its report); Libraries (locks, maintainers, requests,
releases); Audit (append-only, to CSV); Operations (backups, the restore
drill, migrations, the log, sessions, settings). It talks to the server's
protocol only, never to the database; every admin action audited. Stages
with the server's phases: A after 5 (the app, both modes, Overview,
Operations, the map), B with 7 (the rights pages, replacing the admin
window planned inside OHL-Design), C with 8, D with 9, E the Lines page,
F with 13.
> **Owner:** (to answer, 2026-11-18; the recommendation after the arrow)
> 1. A desktop app or a page in the browser? → a desktop app. 2. Host and
> connect modes, or connect only? → both. 3. Who may open it? → network
> admins, line admins (their lines), library maintainers (their
> libraries). 4. Anything editable on the map? → no, read-only. 5. The
> map's background? → none by default; region outlines the admin
> imports; optionally the owner's own tile server. 6. The admin rights in
> OHL-Design today? → kept for a project hosted from a file; for the
> database server only in OHL-Server. 7. When? → stages A–F with the
> server's phases, A after phase 5.

### O46. The tests' speed and a flaky PostgreSQL test
Owner, 2026-11-19: "the server seems to have very long tests can they be
optimised?" **Decided 2026-11-19, first in the
[order](#next-in-order-owner-2026-11-19) with O42; built 2026-11-19, in
active development, not approved yet**
([the test scheme](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/postgresql.md#the-tests),
[timings](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/performance.md#the-tests-of-the-server-the-store-the-session-and-the-command-line-o46-2026-11-19)):
the flake was the writer's lock still held a moment after its connection
closed (1 run in 144 side by side), now waited for; ports made safe; one
cluster per test program, a database per test; the safety-volume test
11.5 s → 5.6 s (its 0.5 m mesh accepted by the owner, 2026-11-19);
nothing removed or weakened. Owner, 2026-11-19: the writer's lock is
released explicitly when its connection's last user goes (a server
started again at once is not refused; built, the wait kept as a safety
net); the server's sub-line test 5.9 s → 0.7 s with O42's lookups.
1. First, the flake: `schema_and_the_writers_lock` failed when other runs
   of the check ran at the same time — the PostgreSQL tests
   (`crates/ohl-store-pg/tests/common/mod.rs`) pick a free port by
   binding and releasing it, so two runs can take the same port or
   connect to the other run's cluster: start again on another port and
   check that the server's data directory is the test's own; proven with
   runs side by side.
2. One PostgreSQL cluster per test binary (started once, a database per
   test, stopped and deleted at the end, also after a failure), the tests
   as isolated and as strict.
3. The slow command-line safety-volume test (about 20 s): cheaper
   without testing less.
4. Every test of the server, the store, the session and the command line
   timed before and after (performance.md). No test removed or weakened.
5. (2026-11-19, with O44 J3–J4) OHL-Structure's quantities panel test
   (`quantities_ui_tests::the_panel_counts_says_and_keeps_the_factor`)
   failed now and then under load with "max_steps exceeded": the panel's
   and the 3D view's background threads each ask for a frame when done,
   which arrived in the middle of a run of frames. It now waits for them
   (their answers taken, two frames in a row) before running to rest,
   with a few more frames allowed for the menu's own run; what it checks
   unchanged. Built, in active development, not approved yet.

### O47. Version management, left from the work plan of 2026-11-10
- Merge plans made in the engine: the app rebuilds the plans for a
  revert, a review into another line and Bring in, and those copies skip
  the libraries check; made in the engine, that is fixed and Bring in
  gets its list of changes; with it "main has K newer" in the menu (the
  count exists). (S–M)
- A tower and its alignment point settled as one choice (a convenience;
  today's behaviour is safe). (M)
- The docs' screenshots of the Versions menu taken again.
> **Owner:** (to answer, 2026-11-10) The "keep both" choice and servers:
> a shared server updated together with the app, or the app hides the
> choice when the server does not know it?

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

### Shared user interface
*Added 2026-11-07.*

- [x] `ohl-ui`, one UI crate for OHL-Design and OHL-Structure (2026-11-07, owner; built, in active development (not approved yet)): text sizes and scroll bars, value fields, the UI scale, messages and long errors in a scroll box of one height, notices, long tables drawing only the rows in view, the SVG artwork, the glyph test's helpers; it depends on no model, engine or format crate ([A shared UI crate](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/ui-shared.md))
- [ ] The hotbar's drawing (slots, inventory, drag and drop) with the tools as a trait each application implements, and the check footer (Run/Stop, progress) once OHL-Structure has windows that run

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
- [ ] **Server**: more editing through the web client (structures, strings, sections), terrain and point clouds in the web client, several projects per server, accounts and roles managed in the app, presence and locks per object, a server log and backups *(the country server and OHL-Server, O17, take most of these)*
- [ ] **Version control**: cherry-pick, visual diffs in the views, history export for audits, revision comparison of results (sag, clearances, loads), automatic sync of offline copies, custom roles, area grants drawn in the plan

### Phase 6 — Ecosystem & 1.0
- [ ] Stable plugin SDK (semantic versioning, compatibility guarantees) — GitHub issue #34
  *(2026-11-07, built, in active development (not approved yet)): the
  versioning rules written down
  ([ADR-0012](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/adr/0012-plugin-interface-versions.md),
  proposed); a compatibility guard (the 0.3 interface frozen and compared
  on every check, one plugin of each world kept as built and loaded by the
  tests); `ohl plugin new` (a plugin of any kind from a template, built
  outside the repository), `ohl plugin check`, `ohl plugin wit`, `ohl
  plugin points`. Still open: 1.0 itself, and how long the host loads the
  previous major version from then.*
- [ ] Plugin registry / index — GitHub issue #34
- [ ] Complete theory manual and validation report — GitHub issue #35 *(2026-11-13: 27 theory pages and 255 validation cases; 2026-11-17: 274 validation cases, a report from `ohl validate --report`; 2026-11-18: 277; not complete)*
- [ ] Translations (i18n) *(to answer, from the equivalence review of
  2026-11-14: which languages first — French, German and Italian for
  Switzerland? — and the theory pages too, or only the user interface?)*
- [ ] Public release, governance model, maintainers team
- [ ] The Windows installer (its files and the draft-release workflow,
  2026-11-11, never built or run on Windows;
  [releases](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/releases.md#the-windows-installer)) —
  to decide: the applications without their console window, the icon in
  the executables, *Add the ohl command line to PATH* ticked by default.

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
