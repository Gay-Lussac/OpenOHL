# Load cases and structure loads (`.ohllc`)

A **load case** combines a weather case, the wires' condition and load factors. The **structure loads** are the forces the wires and strings put on the structures at every point where strings hang, in each load case: the starting point of the structure checks.

## Load case files (`.ohllc`)

```toml
format = "ohl-load-cases"
version = 1
name = "Synthetic example load cases"

[[case]]
name = "wind"
description = "high wind, factored"
weather = "high wind"      # a weather case of the model, by name
condition = "initial"      # initial, creep, load or max_sag
vertical = 1.0             # factor on the vertical loads
transverse = 1.4           # factor on the transverse loads
longitudinal = 1.0         # factor on the longitudinal loads
strength = 1.0             # strength (resistance) factor, for the capacity checks
tags = ["ULS"]
```

| Field | Required | Default | Notes |
|---|---|---|---|
| `name` | yes | | Unique in the file. |
| `description` | no | | E.g. the code clause it stands for. |
| `weather` | yes | | A weather case of the model's weather case file. |
| `condition` | no | `initial` | The wires' condition ([Calculations](line-file.md#calculations-condition-and-method)). After creep or after load needs the line's creep-stretch criteria. |
| `vertical`, `transverse`, `longitudinal` | no | 1 | Load factors, 0 to 100, on the load **components** in the structure's axes (below). |
| `strength` | no | 1 | Strength factor of the capacity checks; more than 0. |
| `tags` | no | | Code, limit state… |
| `[case.structure]` | no | | Version 2: the wind on the structures' own bodies, when it differs from the wind on the wires (below). |
| `[case.broken]` | no | | Version 3: a broken wire, the section solved with the wire of one span gone (below). |

**The factors act on components, not on actions** (**Simplification**): the wires are solved with the unfactored weather, and the factors then multiply each structure load's transverse, longitudinal and vertical components. So the transverse factor also scales the angle component of the wire tension, the longitudinal factor the tension unbalance, the vertical factor the vertical part of the tension on inclined spans. A design code that factors actions (wind, ice, tension) gives other loads — higher or lower depending on its factors (worked example: [Structure analysis](../theory/structures-fe.md#load-factors-on-components-not-on-actions)). Choose the factors with this in mind.

### Wind on the structures apart from the wires (`[case.structure]`)

Design codes often treat the wind on structures differently from the wind on the conductors (another gust treatment, another partial factor). A load case may therefore give the structures their own values; each one given replaces, **for the structures only**, the line's value:

```toml
format = "ohl-load-cases"
version = 2

[[case]]
name = "wind"
weather = "high wind"      # 600 Pa: the wires and strings take it, × transverse
transverse = 1.4

[case.structure]
wind_pressure = "900 Pa"   # the pressure on the structure bodies (default: the weather case's)
wind_factor = 1.2          # the factor on it (default: the transverse factor)
```

| Field | Default | Notes |
|---|---|---|
| `wind_pressure` | the weather case's `wind_pressure` | Pa (or with a unit, `"0.9 kPa"`); 0 or more. |
| `wind_factor` | the case's `transverse` | 0 to 100. |

The wind side stays the weather case's (`wind_from`). The wires, strings and jumpers never take these values. Every check of the structures takes them — one rule (`ohl_loads::LoadCase::structure_wind`): the wind bands below, the member wind of the **fe** check, the wind on a pole, and the line cases in OHL-Structure ([Structure application](structure-app.md#the-lines-wind-on-the-structure-body)). A gust or structure factor of a code goes into the pressure or the factor; drag (force) coefficients stay with the structure (its bands' `drag`, its members' sections, its pole's `drag`). Without the table a case behaves as before. A file is written at version 2 only when a case has such a table (a design-code plugin does the same), so older programs still read the others; they refuse a version 2 file as "newer". In the Load cases library page the field `structure` is offered as `{ wind_pressure = …, wind_factor = … }`.

### A broken wire (`[case.broken]`)

The security cases of the design codes break a wire and check the structures under the unbalanced pull of the spans left (roadmap O9; owner 2026-10-25: "per span and set, in a load case"). A load case names the wire:

```toml
format = "ohl-load-cases"
version = 3

[[case]]
name = "S3 span 2 broken"
weather = "everyday"
longitudinal = 1.0         # a code's impact factor goes here

[case.broken]
section = "S3"             # the section, by id
span = 2                   # its span, from 1 (the section's first span is 1); or "every"
set = 1                    # optional: the set of the wire at the span's ends
subconductors = 1          # optional, version 5: of a bundle, how many break (default: all)
```

| Field | Required | Notes |
|---|---|---|
| `section` | yes | The section, by id. A section the line does not have fails the case (it is not solved as if whole). |
| `span` | yes | The span broken, counted from 1 along the section's points; or `"every"`: each span of the section in turn (below). A span the section does not have fails the case. |
| `subconductors` | no | Version 5 (2026-11-11, in active development): of a **bundle**, how many subconductors break in the span — 1 to the bundle's number. Not given, or all of them: the **whole phase** (the default, and the larger unbalance on the structures). Fewer: the wires left keep pulling, so the unbalance on the structures is much smaller than with the whole phase broken, and the wires left carry nearly the pull of all the wires behind them (their tension rises by about n/(n − k): check it against their strength). The broken ones are taken as gone from the span; the tension of the wires left with the broken ones hanging in their spacers is higher and **not checked** — said on the span and in the case's notes ("NOT ON THE SAFE SIDE"). Same solve time. 0, or more than the bundle has, fails the case. A file is written at version 5 only when a case gives it. See [the theory](../theory/wirefe.md#some-subconductors-of-a-bundle-broken). |
| `set` | no | The set of the wire at the span's ends, as the structures number their wire points (a section point's set): the wire of that set in that span. A check that the span is the wire meant — the case fails when neither end of the span has that set (a section renumbered or strung again). |

What is solved: in the case's weather and condition, the section with the wire of that span **gone** — each span keeping its unstressed length of the whole section in the case, the broken span carrying nothing, its two clamps each a free end held by its strings alone. A suspension string next to the break swings towards the spans left on its side until it, its weight and the remaining span's pull are in equilibrium; the remaining spans re-solve with the clamp where it went. The other wires stay as they are. The structure next to the break takes the remaining span's pull along the line, unbalanced; a section end next to the break (a dead-end) holds only its strings' weight. A **bundle**'s broken wire is the whole phase (every subconductor in that span). See [Finite-element wire systems](../theory/wirefe.md#a-broken-wire) for the model and what it leaves out (the dynamic impact, the pieces on the ground).

**Every span, one at a time** (`span = "every"`): the section is solved once per span, that span broken; at every attachment point the loads keep, for each component (transverse, longitudinal, vertical), the largest of those breaks (by size, its sign kept, compared factored) with the span that gave it. Taken together the components are an envelope (they may come from different spans): on the safe side for the allowable-loads check (Method 2), which takes each component on its own and in both directions, and what the Structure loads page and the CSV show. A member force combines the components, and the envelope is no bound of that (nor does it keep both pulls along the line): the **finite-element** and **pole** checks therefore take each break as its own load and keep the worst, naming the span ("… (span 2 broken)"), and a placed structure opened in OHL-Structure gets one case per break, `<case> (span k broken)` ([theory](../theory/wirefe.md#a-broken-wire)).

The case's factors apply as to any case. A file is written at version 3 only when a case has a broken wire (the Load cases library raises it, a design-code plugin writes it so); older programs refuse it as "newer". In the Load cases library page the field `broken` is offered as `{ section = …, span = … }`.

### Version 4: removed conductors, several broken wires, the wind's direction, ice and snow on the structures

Built, in active development, not approved yet (owner 2026-11-03; the computation and the file, from the command line and the Structure loads page, and the form of the Load cases library below). A file is written at version 4 only when a case uses one of these values; older programs refuse it as "newer", and versions 1 to 3 read as before.

```toml
format = "ohl-load-cases"
version = 4

[[case]]
name = "stringing at T7"
weather = "strong wind"
wind_from = "left"          # the wires' wind side in this case (default: the weather case's)
use_local = true            # take the structures' local conditions

[case.structure]
direction = 90              # the wind on the structure bodies: "right", "left", "ahead", "back" or degrees
                            # (up to version 6: 0 from the right, 90 from ahead — here from ahead)
ice = true                  # the structures' local ice; or a thickness, "20 mm"; false: none
ice_density = "900 kg/m³"   # default: the weather case's
snow = true                 # snow on the structures from their snow on the ground
snow_shape = 0.8            # μ (default 0.8)
snow_exposure = 1.0         # C_e (default 1.0)
snow_thermal = 1.0          # C_T (default 1.0)

[[case.removed]]            # a conductor not there in this case (several allowed)
section = "S3"
set = 2                     # optional check

[[case.broken]]             # several broken wires, broken together
section = "S1"
span = 2
[[case.broken]]
section = "S1"
span = 3
```

| Field | Default | Notes |
|---|---|---|
| `wind_from` | the weather case's | `"left"` or `"right"`: the side the wind on the wires comes from in this case. Only meaningful when the weather case has wind: in a case without wind it has no effect, and the case's notes say so. |
| `use_local` | false | The case takes the structures' local conditions ([Line file](line-file.md)): the wires solved with each span's local wind and ice for strength (from its two structures' `wind_static` and `wire_ice`, the worst or the mean as the line says), the wind on each structure body times its `wind_static`. |
| `[[case.removed]]` | none | **Removed conductors** (owner: "a removed conductor option and broken is only used for actually broke cables"): a construction state, e.g. a tension mast with some phases not yet strung. The section's wire is absent in that case: all its spans are left out of the solve, its loads on every structure are 0; the other sections solve as usual. Jumpers with an end on its wire points only (or on its span points) and spacers on it are left out too. `set`: a check that one of the section's points is of that set. A section not in the line, or removed twice, or both removed and broken, is refused. |
| `[[case.broken]]` | none | **Broken wires** (actual breaks): several per case; all the spans listed are broken **together**, in one solve per section — two spans of one section broken: both gone in that solve, a point between two broken spans then holds no wire (its strings hang under their own weight). `span = "every"` keeps its meaning (each span of that section in turn, the envelope and each break on its own as above), the other broken wires broken in each of those solves; it may not be combined with another broken span of the same section, nor given twice in a case (refused: give them in separate cases). The version-3 single `[case.broken]` table still reads as one broken wire. |
| `direction` | the wires' direction | Where the wind on the structure bodies comes **from**, in the structure's axes (x to the right of the line, y ahead): `"ahead"`, `"right"`, `"back"`, `"left"`, or an angle in degrees. **Version 7 and later:** the program's one convention, 0° from ahead, 90° from the right, 180° from behind, 270° from the left, clockwise ([Wind on the line](../theory/wind.md)). **Files up to version 6:** 0° from the right counted towards ahead (90° from ahead, 180° from the left, 270° from behind — OHL-Structure's convention): read as such, θ = 90° − θ_old, the same forces bit for bit, and written back at its version as read (converted only when the file is raised to 7). Not given: the wires' direction at an angle (version 7), else across from the wires' side. |
| `ice` | by the weather case | Ice on the structure bodies: `true`, each structure's local ice (`structure_ice` of its local conditions; none where it gives none), `"weather"` (version 6, below): the weather case's ice on the wires, a radial thickness for every structure, or `false` (also `"none"`, 2026-11-04): no ice on them, not even the local ice. Not given: none — except in a case taking the local conditions whose weather case has ice, where each structure's local ice ([below](#the-structures-local-ice)). |
| `ice_density` | the weather case's | kg/m³. Without it and without the weather case's `ice_density` the case is refused when computed — no density is made up (with `ice = "weather"`: glaze ice, as on the wires). |
| `ice_factor` | 1 | Version 6: a factor on the structures' ice **thickness**, whichever it is (the wires', the local, the one given) — below. |
| `snow` | false | Snow on the structures' horizontal surfaces: q = μ · C_e · C_T · s_k (SIA 261:2014 (9)), s_k each structure's snow on the ground (its local `snow_ground`, or SIA 261 (10) from its `snow_altitude`); none where it gives neither (the notes list them). |
| `snow_shape`, `snow_exposure`, `snow_thermal` | 0.8, 1.0, 1.0 | μ, C_e and C_T: μ = 0.8 is SIA 261 5.3.4's coefficient where the snow cannot slide off, taken for a pylon's horizontal surfaces as an assumption; C_e = 1.0 normal exposure (5.2.4), C_T = 1.0 (5.2.5). Give them only with `snow = true`. |

### Version 7: the wind's direction for the wires, and sets of directions

*Built, in active development, not approved yet* (owner 2026-11-15). A
case may give where the wind comes from, for the wires **and** the
structure bodies, or a set of directions solved one by one:

```toml
format = "ohl-load-cases"
version = 7

[[case]]
name = "wind along the line, forward"
weather = "high wind"
wind_direction = 180            # from behind: blowing ahead (replaces the weather case's)
longitudinal = 1.4
[case.structure]
wind_factor = 1.4               # the bodies' own factor (else the transverse one)

[[case]]
name = "wind, every direction"
weather = "high wind"
wind_directions = [0, 45, 90, 135, 180, 225, 270, 315]
```

| Field | Default | Notes |
|---|---|---|
| `wind_direction` | the weather case's | Where the wind comes from, relative to the line at each place: 0 from ahead, 90 from the right, 180 from behind, 270 from the left, clockwise (a name, or degrees). 90 and 270 are the across rule from that side (as `wind_from = "right"` / `"left"`, still read); another angle is one wind, each span its part normal to it ([Wind on the line](../theory/wind.md)). The bodies take it too unless `[case.structure] direction` is given (in a structure turned off the line's direction, converted to its axes). Not with `wind_from`. |
| `wind_directions` | none | A **set**: the case is solved once per direction, each as its own case `<case> (wind from θ°)`. The **fe** and **pole** checks and the allowable loads take each as its own case and keep the worst, naming it; the Structure loads page lists each; the structures' groups (Applies to) know them by the set's name; OHL-Structure gets one case per direction. Cost: one line solve per direction. Not with `wind_direction`. |

- A case with a direction says in its notes that it is **one direction,
  not the across envelope**: a wind across the line is not checked by
  that case. The across cases stay the default.
- **An along-line wind loads the structure longitudinally** through the
  wires at angle structures and its body: give the factors accordingly
  (`longitudinal` for the wires' part, `[case.structure] wind_factor`
  for the body, which otherwise takes `transverse`).
- **Old files.** Files up to version 6 read and compute as before, bit for
  bit; their numeric `direction` stays in the old convention. A file is
  written at version 7 only when a case uses one of these (or gives a
  body angle in the new convention); older programs refuse it as "newer".
  In the Load cases library's text page, a file raised to version 7
  has its body angles converted (θ = 90° − θ_old).

### Version 6: the wires' ice on the structures, with a factor

*Built, in active development, not approved yet* (owner 2026-11-15: "a factor as for the wind to change the value compared to the conductor weather cases"). As the wind on the bodies has its own factor (`wind_factor`), the ice on them can now be taken from the wires' and changed by a factor:

```toml
format = "ohl-load-cases"
version = 6

[[case]]
name = "ice, half on the towers"
weather = "heavy ice"        # 40 mm on the wires

[case.structure]
ice = "weather"              # the weather case's ice thickness on the wires
ice_factor = 0.5             # times this: 20 mm on the structure bodies
```

- `ice = "weather"` puts the weather case's radial ice thickness on every structure taking the case, of the case's `ice_density`, else the weather case's, else glaze ice (as on the wires). A weather case that gives its ice as a **load per metre of wire** (`ice_load`) has no thickness for a structure: the case's ice on the structures is then refused, and the case's notes say why (none is made up). A weather case without ice puts none.
- `ice_factor` multiplies the **thickness**, whichever source: the wires' ice, the structures' local ice, or a thickness given (0 to 100; refused with `ice = false`). The ice's weight follows from the factored thickness (ρ g (P t + π t²) per metre of member) and is then times the case's **vertical** factor, as before — so `ice_factor` changes the amount of ice, the vertical factor stays the load factor on it.
- The wires keep the weather case's ice: the factor acts on the structure bodies only.
- **The engineer's choice:** a factor below 1 puts less ice on the bodies than on the wires — not checked by the program; give it as your design code says.
- A file is written at version 6 only when a case uses one of these; older programs refuse it as "newer".

In the Load cases library form, section 5: ice **the wires' ice** (the hover gives the rule), and **factor on the thickness** (shown with any ice; with the wires' ice the form shows the thickness it gives, e.g. *= 20.0 mm on the structures*). The first choice, formerly *by the weather case*, is now called **default** (the same rule: the local ice when the case takes the local conditions and its weather case has ice, else none).

**Where the wind on the bodies blows from another direction:** the wind bands give one area, the one a wind across the line meets; a wind from another direction takes that same area, its force along the direction it blows (|F| = q · c · A), so a wind from ahead gives a longitudinal load. This is an approximation and **not on the safe side** where the structure shows more area along the line (its crossarms seen in full). The member wind of the **fe** check and the wind on a pole follow the direction exactly as their own models do ([theory](../theory/structure-wind.md#direction-ice-and-snow-on-the-structure)).

**Where the ice and snow go:** on the members of the structure's model (members with a profile from the cross-section library; the **fe** check, ice all round each member's outline and snow on each member's plan), on a pole's shaft (ice; no snow on a pole), each times the vertical factor; and listed per structure as loads without a point, "ice on the structure" and "snow on the structure", down at the weights' centroid. A type with only wind bands or attachment points gives no surface: the ice and snow are then not counted, and the case's notes name those structures (not on the safe side). The allowable loads at the attachments (Method 2) take no load without a point — as the wind bands. Every assumption is in the [theory](../theory/structure-wind.md#direction-ice-and-snow-on-the-structure).

**Which structures take which case: structure groups** (owner, 2026-11-15: "simply defining load cases and then have them attributed in groups of pylons"; built, in active development, not approved yet). A structure takes the load cases of the **groups** it is in — picked by hand, or by rules (types, line angle, station on an alignment, dead end), all made in Line › Structures › **Load cases per group…** ([its page](load-cases-per-group.md)); in two groups, the cases of both (the union: more cases, the safe side); in **no group, every case**; in groups ticking no case only, **none**. A structure's loads are computed only in its cases: in another case it has none (the notes list it: *not taken by the groups of N structure(s)*). Groups are kept in the line (`structure_groups`, line file version 18). Before 2026-11-15 a structure gave its own set (`load_cases`) or *none* (`no_load_cases`), a structure type a default (`structure_loads.type_load_cases`); such lines are read into groups ([how](load-cases-per-group.md#lines-and-projects-of-before-2026-11-15)).

**A case ticked in a group but not in the library** (renamed or deleted there after the group was made) is never dropped silently (2026-11-14): the case's notes say it; the usage lists the structure as **not checked** — *not checked: load case(s) X of its groups not in the library* — never as a pass (its usage in the cases it does take is still shown); a structure whose groups tick *only* such cases is listed in every case as not checked, with no loads; the Structures table's **load cases** cell begins with ⚠ in the error colour; OHL-Structure's header says it too. **Rename** in the Load cases library renames the case in every group at once (below).

**A structure that takes no load case** (its groups tick none): no loads are computed for it in any case and no structure check is made. It is never silently absent: every case's notes name it ("takes no load case"), the usage table lists it as *not checked* with that note — never as passing — and OHL-Structure is told so in its line-values note (no case of the library for it). It costs nothing: the wires are solved as before.

In the application: Line › Structures › **Load cases per group…** makes the groups (each change one command, undone as one); the **Structures** table's **load cases** column shows each structure's groups and cases (`suspension, angle: 3: everyday, wind, ice`, `no group: every case`, `none: takes no load case (…)`; its hover the whole text); a structure's **Properties › Load cases** shows them (changed in the window). No time is added: each case is solved once for the whole line, a structure not taking it only has no loads in it. OHL-Structure takes a placed structure's cases as its load cases ([The structure application](structure-app.md#the-lines-load-cases-the-library)).

**Notes.** Each case's notes say what was assumed: the wires' wind side, the local conditions, the conductors removed (and the jumpers and spacers left out with them), the wires broken, the structures its groups do not give it, the direction of the wind on the bodies, the ice and snow and where they were not counted. `ohl line … --load-cases` prints them under each case.

### The Load cases library, a form

Line › Structures › **Load cases…** (until 2026-11-15 Line › Criteria › Load cases library) shows the file as a **form** (owner: "design it so that it's easy to understand and use for a human"; built, in active development, not approved yet): the cases on the left — **Add** (the first weather case, every factor 1), **Duplicate**, **Delete** (its hover names the structure groups ticking the case and its own list when it picks structures: deleted, their structures are not checked in it, said in the usage and the Structures table), **Rename** (2026-11-14: in a project, one change of the project — the case renamed in the file, comments kept, and in every structure group ticking it and in its own list, undone as one; with unsaved changes on the page, save or discard them first; a file outside a project: the file only), **Add the default cases…** (2026-11-15: OpenOHL's default load cases the file lacks, [below](#the-default-load-cases)) (a case that does not check is marked ⚠) — and the case picked on the right, in plain sections:

1. **Weather** — the weather case, picked from the project's weather cases only (a case naming another says so in red), and the wires' condition (as strung, after creep, after the heaviest load, the one that sags more).
2. **Wind on the wires** — shown only when the weather case has wind: *as the weather case*, *from the left* or *from the right* (`wind_from`).
3. **Wires removed and wires broken** — per section of the line (open when the line has four or fewer, else click its name): **removed (not strung)** — a construction state, every span of that section left out, it puts no load anywhere — and, for a real break, a tick per span (*S1 span 2 (T2–T3) broken*; several break together) or **every span broken, one at a time** (its cost in the hover: one solve per span), with the **set checked** when the section has several sets. A section with a broken span cannot be removed too (its tick greyed).
4. **Wind on the structures** — *off*, *the weather case's* (its pressure shown) or *own pressure* (Pa); the direction: *as the wires*, *from the left*, *from the right*, *from ahead*, *from behind* or *at an angle* with its degrees (since 2026-11-15 in the program's convention: 0° from ahead, 90° from the right, clockwise; an angle read from an older file is shown converted and kept as read until changed); *own factor* (else the transverse one).
5. **Ice and snow on the structures** — ice *default* (the structures' local ice when the case takes the local conditions and its weather case has ice, else none), *none* (not even the local ice), *the structures' local ice*, *the wires' ice* (the weather case's thickness, version 6) or *a thickness* (mm); *factor on the thickness* (version 6, any of them; unticked 1); with *own density* (else the weather case's; when it gives none the form says a density is needed); **Snow on the structures** with μ, C_e and C_T (0.8, 1.0, 1.0 by default; the hover cites SIA 261:2014 5.3.4, 5.2.4 and 5.2.5, and says that μ = 0.8 for a pylon's horizontal surfaces is an assumption the engineer is to confirm).
6. **Factors** — vertical, transverse, longitudinal and the strength factor φ; **Use local conditions** (what it takes and its cost in the hover).
7. **Tags and description**.
8. **Applies to** — which structures of the line take the case, how each gets it, and ticks to change it right there ([below](#applies-to-which-structures-take-the-case)).

#### Applies to: which structures take the case

*Built, in active development, not approved yet* (owner, 2026-11-15: "manual should always be available where load cases rules are defined and on which pylon they apply with a list of them. Not per type but per position: if 2 pylons are the same type they can still get different load cases based on the location").

- **The summary**: *Taken by 5 of 8 structure(s): T1, T2, …* — or, in the warning colour, *No structure takes this case: it gives no loads and no check anywhere* (a case naming nothing is said, never silent).
- **The list**: every structure of the line, in the line's order, one row each — its id, its station on its alignment, its type, and **how it gets the case**:

| The row says | Meaning |
|---|---|
| `in no group: every case` | in no [structure group](load-cases-per-group.md): it takes every case of the library |
| `group angle by rule (line angle 10–30°)` | a group ticking the case holds it by a rule of the group (type, line angle, station, dead end) |
| `group crossing (picked)` | a group ticking the case holds it picked by hand |
| `picked here` | picked in the case's **own list** (below): it takes the case on top of its groups' cases |
| `excluded here (else: …)` | excluded in the case's own list: it does **not** take the case, whatever its groups give; *else* says how it would get it |
| `not taken: its groups suspension do not tick it` | it is in groups, none of which gives the case |

The row is ticked when the structure takes the case. The list is a tick list as elsewhere in OpenOHL: a **search** field filtering the rows (by id, station, type or the *how* text: type `rule` or a group's name), **All**, **None** and **Invert** acting on the rows shown, a press dragged across rows giving them all one state, shift-click for a range.

- **Tick a structure** that does not take the case: it is **picked** in the case's own list and takes the case on top of its groups' cases. Its groups and their other cases are not touched.
- **Untick a structure** that takes the case:
  - picked here only: it is **taken out** of the own list;
  - taken through a group (by a **rule**, or picked in that group) or by being in no group: it is **excluded** in the case's own list — exactly this case leaves it, nothing else (its groups and their rules unchanged). The status line says how it took the case: *T5 excluded here (it took it: group angle by rule (line angle 10–30°)) — change the rule in Load cases per group, or keep the exclusion*. To take it out for good, change the rule in Line › Structures › [Load cases per group…](load-cases-per-group.md#5-rules-optional) instead.
- **Add the N selected** / **Remove the N selected** do the same for the structures selected in the Structures table or a view (Shift or Cmd/Ctrl + click for several).
- Below the list: *Its own list: 2 picked here (T2, T4), 1 excluded (T5) — also in Line › Structures › Load cases per group as "wind (own list)"*.

Each change — a click, a drag across rows, All, None, Invert, Add, Remove — is **one step of Edit › Undo**, a change of the line (not of the `.ohllc` file: no Save needed). Nothing is solved again: the case is solved once for the whole line. A case added on the page but not saved yet cannot be given to structures (*Save the file first*: structures take a case by its saved name). A library file that is not the one the line uses shows no list (its cases are not the line's). Read-only, the list is shown greyed.

**Two structures of the same type** take different cases simply by ticking one and not the other: the per-type rule of a group is only an optional automatic helper.

The form writes the file with OpenOHL's own writer, at the lowest version its cases need (a file using nothing of version 4 stays at its version); **Save** keeps it — in a project, one change of the project, undone as one. A file written from the form keeps no comments: **Fields (expert)** edits the file's fields as before (comments kept). Why the file does not check is said above the case, in a box of its own height.

### Use local conditions: one rule

A check's **Use local conditions** switch (Checks › Structure loads) and a case's `use_local` mean the same (2026-11-03): *this case takes the structures' local conditions*. Ticked, every case is computed as if it said `use_local = true`: the wires on the line with the spans' local wind and ice for strength, and the wind on each structure's body — its wind bands, the members of the usage check's **fe** model and a pole — times its `wind_static`, **once** (a structure of 1.5 gets 1.5 × its body wind, never 2.25 ×; tested). Unticked, each case as written (a case's own `use_local` kept). The snow on the structures comes from the case's section 5 (each structure's s_k), with or without the switch; the ice too, and with the switch also the structures' local ice ([below](#the-structures-local-ice)).

**OHL-Structure follows the switch** (2026-11-14, built, in active development, not approved yet): the switch is kept with the project (per project line), and what a placed structure gets in OHL-Structure (**Open in OHL-Structure…**) takes it too — every library case as if it said `use_local = true`, so both checks take the same loads; OHL-Structure's status block says *With the local conditions…*. Before, OHL-Structure got the cases as written: with the switch ticked, its loads were lower than the line check's where a structure's `wind_static` is above 1 — **not on the safe side**, and not said. Changing the switch writes the open structures' line values again.

### The structures' local ice

*Built, in active development, not approved yet* (owner, 2026-11-04: "yes apply local ice too"). A case that **takes the local conditions** (its `use_local`, or the check's switch) and whose **weather case has ice** (a thickness or a load above 0) puts each structure's local ice (`structure_ice`) on its members — unless the case gives its own: a thickness, `ice = true` (the same local ice, in every case), or `ice = false` (none). The density: the case's `ice_density`, else the weather case's; neither: the case is refused, the message says so (no density is made up — give one, or `ice = false`). Taken by the structure loads (each structure's "ice on the structure" row), the **fe** check's members and a pole's shaft (their notes say "the structure's local ice") and OHL-Structure's library cases (the case's description says so). A note of each case names the structures that got it, with their thickness. Not taken without the local conditions, nor in a case whose weather case has no ice (tested by hand).

Design codes (EN 50341 with a national annex, the Swiss LeV…) supply their combinations and partial factors as such files: a design-code plugin writes them from your parameter file ([Design codes](design-codes.md)). OpenOHL ships no code tables: the factors come from the standard you hold. `examples/synthetic-line/load-cases.ohllc` only shows the format.

In a project, the load case files are kept in the project like the other libraries (Line › Structures › Load cases… edits them).

### The default load cases

*Built, in active development, not approved yet* (owner, 2026-11-15: "then also have some default load cases for pylons like -20°, 10°, 40°, 10° + 300 Pa wind left, and right and 0° + 20N/m ice with a factor 1.5 on wind for the pylons and 1.3 on the ice. these values are arbitrary but needed to check the codes. also i would like to have the option to have wind along the line forward and backward to test the pylons"). OpenOHL ships eight load cases for the structures, `libraries/loadcases/default-load-cases.ohllc` (file version 4). **Their values and factors are the owner's arbitrary choice for checking the program and the design codes — not taken from a design code or a standard.** They name the [default weather cases](weather.md#the-default-weather-cases). Every case: the wires as strung (`condition = "initial"`: it needs no creep-stretch criteria of the line; the owner gave no condition), strength factor 1, tag `default`, a description beginning "default case, arbitrary values for checking (not from a design code)".

| Name | Weather case | V | T | L | Wind on the wires | Wind on the structure bodies |
|---|---|---|---|---|---|---|
| `-20 °C` | `-20 °C` | 1 | 1 | 1 | — | — |
| `10 °C` | `10 °C` | 1 | 1 | 1 | — | — |
| `40 °C` | `40 °C` | 1 | 1 | 1 | — | — |
| `10 °C + 300 Pa wind left` | `10 °C + 300 Pa wind` | 1 | 1.5 | 1 | 300 Pa from the left (`wind_from = "left"`) | 300 Pa from the left × 1.5 (`wind_factor = 1.5`) |
| `10 °C + 300 Pa wind right` | `10 °C + 300 Pa wind` | 1 | 1.5 | 1 | 300 Pa from the right (`wind_from = "right"`) | 300 Pa from the right × 1.5 |
| `0 °C + 20 N/m ice` | `0 °C + 20 N/m ice` | 1.3 | 1 | 1 | — | — (no ice on the bodies) |
| `10 °C + 300 Pa wind ahead` | `10 °C` | 1 | 1 | 1 | none | 300 Pa from ahead × 1.5 (`wind_pressure = "300 Pa"`, `direction = "ahead"`) |
| `10 °C + 300 Pa wind back` | `10 °C` | 1 | 1 | 1 | none | 300 Pa from behind × 1.5 (`direction = "back"`) |

**What the factors mean** (they act on the load components, [above](#load-case-files-ohllc)):

- *"A factor 1.5 on wind for the pylons"*: `transverse = 1.5` multiplies the wires' and strings' loads across the line, and `[case.structure] wind_factor = 1.5` the wind on the structure bodies (the wind bands, the **fe** check's member wind, a pole). The transverse factor acts on the whole transverse component — at a line angle also the wires' pull across the structure — so it is **above** a factor on the wind alone: on the safe side. Vertical and longitudinal stay 1.
- *"1.3 on the ice"*: `vertical = 1.3`. It acts on the whole vertical component — the wires' and strings' own weight with the ice — so it is above a factor on the ice alone: on the safe side for the downward loads (an uplift is made larger too). The structure bodies take no ice in this case: the weather case gives its ice per metre of wire, which is no thickness for a structure (`ice = "weather"` refuses such a case, [version 6](#version-6-the-wires-ice-on-the-structures-with-a-factor)). **Not on the safe side** where a code puts ice on the structure too: give it in the case's section 5.

**The wind along the line, forward and backward** (`… wind ahead`, `… wind back`): the wind blows along the line on the **structure bodies** only, from ahead (towards the back) or from behind (towards ahead), 300 Pa × 1.5 — a **longitudinal** load on each structure with a body that takes wind (wind bands, members with profiles, a pole). **The wires take no wind** in these cases: they name the still weather case `10 °C` (the same temperature as the wind case), and the body's 300 Pa is the case's own pressure (`[case.structure] wind_pressure`), equal to that of `10 °C + 300 Pa wind` — change both together. Why: the program models the wind on the wires across the line only (from the left or the right); a wind along the line meets the spans end-on and puts (nearly) nothing on them, while giving the wires the weather case's cross wind as well would load the structures with a wind they do not get at the same time. What this leaves out — **NOT ON THE SAFE SIDE**, said in each such case's notes:

- at a **line angle** the spans are not quite along the wind, and their share of it (small for small angles) is not taken;
- the **strings', jumpers' and posts'** own wind along the line is not taken (they take no wind in these cases);
- the wind bands give the area a wind **across** the line meets; a wind along the line takes that same area ([above](#version-6-the-wires-ice-on-the-structures-with-a-factor), "Where the wind on the bodies blows from another direction") — not on the safe side where a structure shows more area along the line.

The load case format needed nothing new for this. A weather case with wind at any angle (wires and bodies alike) is being planned separately.

**Where they appear:**

- **Every new project** starts with them, with the default weather cases (File › New project…, the tick **Start with the default weather and load cases (arbitrary values for checking)**, on by default): an empty project gets this file as its load case library; the starter template's load case file gets them after its own.
- **An existing project**: Line › Structures › **Load cases…** › **Add the default cases…** (beside Add, Duplicate, Delete, Rename) lists the default cases the file lacks — by name: a case of the same name is kept as it is, never replaced — and the weather cases they name that the project lacks (add those with Line › Criteria › Weather cases › **Add the default cases…**); **Add N case(s)** puts them into the form, **Save** keeps them (one change, one undo).
- **The command line**: `ohl line … --weather libraries/weather/default-weather-cases.ohlwc --load-cases libraries/loadcases/default-load-cases.ohllc`; without `--load-cases` nothing changes.

## Structure loads

Every section is solved in each load case with the finite elements (strings swing, each span its own tension), in the load case's condition and weather. At every point of the structure its strings hang on, the load is the force the section puts on the structure: the reaction there reversed (a V: one per point), or at the wire point of a wire fixed to the structure. Points used by several sections add up; sections sharing strings (a TAK) are solved together and their loads counted once. It is the whole bundle's load, and includes the strings' weight.

Each load is given:

- **in the line's axes:** x east, y north, z up, in N;
- **in the structure's axes:** transverse (along the structure's local x, to the right of the line looking ahead — `orientation_deg`), longitudinal (ahead), and vertical (down);
- **unfactored, and factored** by the load case's vertical, transverse and longitudinal factors.

A section that does not solve in a load case is listed, with why. Its loads are then missing, and every structure it ends at or hangs on (and, for a jumper held by strings, every structure holding it) is **incomplete** in that case: its loads are those computed — nothing is made up for the missing wire — but they lack that wire's forces. The incomplete structures are listed per case with the sections or jumpers they miss ("T3: incomplete — not checked: S3 not solved in ice"), and their rows on the Structure loads page are grey with the missing wires named.

**Ice, snow and notes in the window** (2026-11-03): the Results tab lists the rows *ice on the structure* and *snow on the structure* of each structure taking a case with ice or snow on the structures, says which structures take the case shown (with per-pylon sets), and the case's notes — what it assumed — in a box of its own height under its factors.

**In the application:** Checks › **Compute structure loads** runs in the background (the model is not changed) and opens the **Structure loads** window (floating first, docked as a panel with its 📌 like every check window); **Run** in its footer (bottom left) computes them again. Since 2026-10-31 it has the check windows' **Setup** and **Results** tabs: *1 What it checks* (with *How it is computed* and a drawing of the loads at a structure point: T across, L along, V down) and *2 Scope* — the load case file (with **Load cases library…**), the **load cases** and the **structures** computed, each a tick list (none ticked: Run greyed, with the reason). Only the load cases ticked are solved (fewer cases: a shorter run); every section is still solved in them, and the loads are kept at the structures ticked. Before Run the footer says what it computes and about how long, from the last run ("3 load cases × 7 structures — about 2 s (from the last run)"). The Results tab shows one load case at a time (factored, or unfactored), a filter by structure, a structure's link to go to it, the table's **Export CSV…** for the case
shown and **Export every case (CSV)…** for all of them (named apart,
2026-10-31).

**From the command line:**

```bash
ohl line line.ohl.json --weather cases.ohlwc --load-cases loads.ohllc --loads-csv loads.csv
```

The CSV columns: `case,structure,point,label,sets,x,y,z,transverse,longitudinal,vertical,factored_transverse,factored_longitudinal,factored_vertical,broken_span,factored_moment_transverse,factored_moment_longitudinal,factored_moment_vertical` (`point`: the point's label, empty for the wind on the structure; `sets`: the wire ends hanging there, separated by spaces; `broken_span`: in a broken-wire case the span broken that gives each component — transverse, longitudinal, vertical, separated by spaces, the same three for one span, the governing ones for `"every"` — empty in the other cases; the three factored moments, N·m, about the point in the structure's axes — of a load acting away from its point, a post insulator's (below), 0 for every other load; added at the end on 2026-11-14). The run names each broken-wire case (`broken wire: S3 span 2 broken`).

**Local winds:** the spans' local wind factors of the line ([Line file](line-file.md#local-winds-per-span)) are in every load case with wind: the wires take each span's wind times its factor, and so do the loads they put on the structures. The structures' own wind (the bands below, the member wind, the pole) is the load case's, not the spans'.

**Wind on the structures themselves:** a structure type may give the area the wind blows on, by height band (`[[wind.band]]`, see [Structure libraries](structure-library.md#wind-on-the-structure)). In every load case with wind, each band adds a load across the line — wind pressure × drag coefficient × area, on the wind side of the weather case — at the band's mid-height on the structure's centre line, listed without a point ("wind on the structure"), factored by the transverse factor — or, when the load case gives the structures their own wind (`[case.structure]`), the pressure there × drag × area, factored by its `wind_factor`. The capacity checks' load trees are per point and leave these out, and so does the **fe** check: it takes the member wind on the profiles instead, when the members' sections come from the [cross-section library](section-library.md) (see [Member wind](structure-analysis.md#member-wind-body-wind-at-level-3)). A structure standing on a extension stands its `dz` higher, and its bands with it: the band heights are above the ground of the structure as written, so on a +3 m extension a band 0–20 m acts at 3–23 m (its mid-height 13 m). The force does not change — the pressure is the same at every height — only where it acts (the `z` of the CSV, the label). The extension's own piece below the feet has no band: its members take the member wind of the **fe** check, and OHL-Structure's results put a case's pressure on all members on them too (the same rule, [Structure application](structure-app.md#results)).

**Jumpers:** the sections carry them at their clamps as Line › Criteria › Calculation options says (an estimate by default — weight, ice and wind — or the jumper's own pull; line file `solver.jumper_loads`), and a jumper held by helping strings pulls on those strings, so on the structure holding them. A clamp **fixed to the structure** (a wire point without strings, a post insulator's top) passes the jumper's load straight to the structure, and a jumper end where **no section ends** (a jumper from a dead end to a post's top, or to a wire point of its own) loads that wire point's strings' tops, the wire point, or the post's base, in the same way (since 2026-11-14; before, only a riser's end was taken there and a fixed clamp dropped the jumper's load — not on the safe side). See [Jumpers](../theory/jumpers.md).

**Post insulators** (2026-11-14): the load on a fixed post's top is reported at the post's **base** point, as a load of its own labelled with its lever ("A1 (post, 1.60 m)", `x,y,z` where it acts), so the structure check takes the **moment of the post's height** at the base's joint (where only trusses meet there, or in the pole check, the moment is left out and the result says NOT ON THE SAFE SIDE). The post's own weight and wind are another such load, at its middle. A stiff jumper held in the post's clamp adds its **moment** on the clamp (factored by the largest of the case's factors); with the estimated jumper loads the case's notes say that this moment and the estimated forces are not one state (NOT ON THE SAFE SIDE; exact with the jumpers' own pull). A braced (hinged) post is a string of its assembly: its force goes to its base like any string's. See [Post insulators](../theory/post-insulators.md).

## Structure usage (capacity checks)

With the loads, OpenOHL checks how much of its capacity every structure uses, from its type's `[capacity]` (see [Structure libraries](structure-library.md#capacity)). Two methods, each used when the type gives its data:

- **Method 1, allowable spans.** The structure's **wind span** (half the sum of its adjacent spans, each times its local wind factor when it has one, the largest over its conductor positions) and **weight span** (the wires' vertical load at the points they hang on, without the strings, over their vertical load per metre in the load case — for a catenary, the distance between the low points of the adjacent spans, negative under uplift; the largest and the smallest over its conductor positions) against the type's allowable spans at the structure's **line angle** (the largest over the routes through it). The allowable spans are interpolated between the angles given — linearly in \\( \sin(\theta/2) \\), not in the angle θ (below); a structure turning more than the last angle fails ("not allowed"). Usage: the larger of wind span / allowable and weight span / allowable; a weight span below the minimum (uplift) is 100 % plus the shortfall over the allowable weight span. The strength factor does not apply: allowable spans are the type's design spans. A **conductor position** takes both sides of the structure whichever way they hang: one wire point carrying the back and ahead spans (a suspension point; a line end has only one side), or a dead-end's back and ahead strain strings — two wire points of the structure joined by a jumper whose two ends both end a section there, or hanging on the same strings (a TAK). A jumper from a branch's end to a point in another section's middle (a tee-off) joins nothing: the branch is a position of its own. Without the jumper, a dead-end's two strings are checked as two line ends.
- **Method 2, allowable loads (load trees).** At every point the type's load tree gives, the **factored** loads in the structure's axes against the allowable loads times the load case's **strength factor**: transverse and longitudinal either way, vertical downwards, uplift upwards (an upward load with no uplift given fails). Usage: the largest ratio of the point, or their sum with `interaction = "linear"`; the structure's is its worst point's.

How exact Method 1 is (labels as in the [theory manual](../theory/conventions.md#conservative-assumptions-and-simplifications)):

- **Conservative:** the largest wind span and the largest and smallest weight span over the structure's conductor positions, and the largest line angle over its routes, are taken together, as if they all occurred at one position. Exact: each position against its own limits.
- **Simplification:** the wind span is half the sum of the horizontal span lengths. The wind acts on the wire's length, longer in an inclined span by \\( 1/\cos \\) of its incline, which this check does not see (unsafe in steep terrain, unless the type's allowable wind span allows for it). The structure loads (Method 2, fe) carry the solved wind, not this. Exact: the wind on the wires' length.
- **Conservative:** the allowable spans between two angles the type gives are interpolated linearly in \\( s = \sin(\theta/2) \\). Why this is on the safe side: the wires pull across the structure with \\( 2T s \\) at a line angle θ. With the structure's response linear in its loads, every member's force is linear in the wind span, the weight span and \\( s \\), so the largest wind (or weight) span keeping every member within its capacity is the least of functions linear in \\( s \\): concave in \\( s \\), it lies on or above the straight line between two of its values, and the interpolation never allows more than the type does. The smallest weight span (uplift) is the largest of such functions, convex, on or below its chord: the interpolated minimum is never less. Linear in θ (as before 2026-10) it could allow too much: \\( s \\) is concave in θ, so a span falling linearly in \\( s \\) lies below the straight line in θ — e.g. 600 − 1000 \\( s \\) m given at 30° and 60° is 217.3 m at 45°, the straight line in θ 220.6 m. The interpolation equals the type's spans when one member governs between the two angles, and is below them when another member takes over. Exact: allowable spans at enough angles, or Method 2. A structure whose response is not linear in its loads (second-order effects, buckling interaction) is outside this argument — its allowable spans should be given at close angles.

Two more checks come from an analysis, when the type names a structure model or a pole (see [Structure analysis](structure-analysis.md)):

- **fe** — the type's finite-element model (its members when it is a full design, or `model = "….ohlfe"`) solved with the factored loads at its points (and the member wind on profiles from the cross-section library); usage: the member using most of its capacity.
- **pole** — the type's pole (`pole = "id"`): bending moment along the pole from the factored loads and the wind on the pole, P-Delta amplified, against its moment capacity.

The wind on the structure in the **fe** and **pole** checks (and on the bands) is the weather case's pressure × the transverse factor, or the load case's own values for the structures (`[case.structure]`, above); a note on the structure names them when they are the case's own.

A structure's usage is the largest of its checks. A structure without a type, or whose type gives nothing to check against, is listed as not checked.

**Incomplete structures.** A structure whose loads in a load case lack a section or jumper that did not solve there is **incomplete** in that case: each of its checks (spans, loads, fe, pole) is computed from the loads there are and marked incomplete — the usage may be too low, so it is never shown as a pass, whatever its number. A structure every wire of which failed has no load at all and is listed all the same.

**In the application:** on the Structure loads page, **Usage** shows every structure worst first — for the governing load case of each (or one load case): usage (red over 100 %, orange from 90 %), each method's usage (spans, loads, fe, pole), what governs (e.g. "weight span 640 m of 600 m", "1:2 transverse 31.2 kN of 30.0 kN × 0.9"), the load case, line angle, wind and weight spans — and **Export CSV…**. An incomplete structure's row is grey and italic: its usage reads **not checked** (the usage of the loads there are on hover) and "governs" names what is missing, e.g. "not checked: S12 not solved in ice". In the governing view a structure incomplete in any load case is shown so, with those cases. The summary counts the structures not checked because of a wire not solved.

**Colour structures by usage** (a tick on the Usage view) colours the structures in the plan and 3D views by their usage in the load case chosen there (the governing one: their worst; incomplete in any case is incomplete): green below 90 %, orange from 90 % to 100 %, red over 100 %, a neutral grey-blue for **not checked: a wire not solved**, dark grey for **not checked (no capacity, a case)** — no capacity to check, or (2026-11-14) a load case of its groups not in the library. A legend sits at the bottom right of both views.

**From the command line:** the loads run prints the ten structures using most of their capacity (an incomplete one marked "(incomplete)") and lists the incomplete structures per case; `--usage-csv usage.csv` writes every load case and structure:

```bash
ohl line line.ohl.json --weather cases.ohlwc --load-cases loads.ohllc --usage-csv usage.csv
```

The CSV columns: `case,structure,type,line_angle_deg,wind_span,weight_span_min,weight_span_max,spans_usage,loads_usage,fe_usage,pole_usage,usage,governs,notes,incomplete` (usages in %). `incomplete` is empty when the structure's loads in that case are complete, else it names what is missing: `incomplete: section S12 not solved (why); jumper J3 not solved (why)` — the usages of that row are then those of the loads there are.

