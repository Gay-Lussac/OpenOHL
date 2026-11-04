# Weather cases (`.ohlwc`)

A weather case is a wire temperature, ice and wind. Cases are kept in a
file and each run chooses the ones it needs.

```toml
format = "ohl-weather-cases"
version = 1
name = "Project X"

[[case]]
name = "max temperature"
tags = ["clearance"]
temperature = "80 °C"

[[case]]
name = "glaze ice"
tags = ["clearance", "loads"]
temperature = "-5 °C"
ice_thickness = "12.7 mm"
ice_density = "913 kg/m³"

[[case]]
name = "rime ice + wind"
tags = ["loads"]
temperature = "-5 °C"
ice_load = "20 N/m"
ice_density = "500 kg/m³"
wind_pressure = "200 Pa"
```

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | Unique name. |
| `description` | no | Free text. |
| `tags` | no | Words for choosing cases (`clearance`, `loads`, …). |
| `temperature` | yes | Wire temperature. |
| `ice_thickness` | no | Radial ice thickness. |
| `ice_load` | no | Or: ice weight per length of wire (not both). |
| `ice_density` | no | Default glaze ice, 913 kg/m³. With a thickness: the ice weight. With a load: the iced diameter, for the wind and the clearances. |
| `wind_pressure` | no | Wind pressure on the projected (iced) diameter, perpendicular to each span. |
| `air_density_factor` | no | Q, Pa/(m/s)²: the wind pressure of a wind velocity V is Q·V² (default 0.613, ½ρ of air at 15 °C at sea level). Only relates pressure and velocity (the Weather cases table shows and takes both); the loads use `wind_pressure`. |
| `ambient_temperature` | no | Air temperature, e.g. `"35 °C"`, for ratings; the wires use `temperature`. |
| `wind_from` | no | Files of earlier versions only: the side is now chosen where the wires are shown (desktop app: View › Display weather cases — right, left or both; the command line still reads it, right by default, and `ohl line --wind-from` sets it for a single case). A project's weather case file loses it when the project is opened; a file read directly is not changed (its value is not used by the app). |

Values take units (`"0.5 in"`, `"4 psf"`, `"1.2 lbf/ft"`, …).

## How ice loads a wire

On a bare wire of diameter *d* and weight *w*:

- **thickness *t***: ice area A = π/4 [(d + 2t)² − d²], ice weight ρ g A, iced diameter D = d + 2t;
- **load *w*ᵢ**: ice weight as given; with the density ρ the ice area is A = wᵢ / (ρ g) and the iced diameter D = √(d² + 4A/π) — light rime of the same weight as dense glaze presents more area to the wind. Without a density the ice is glaze, ρ = 913 kg/m³ (the same default as for a thickness).

The vertical load is w + ice weight, the wind load q·D, and the wire hangs in the plane of their resultant. Clearances are measured to the iced surface, D/2 from the wire's axis ([Clearance](terrain.md#clearance)).

**Simplification** (not on the safe side for the wind, the swing and the clearances): an ice load given without a density is glaze ice of that weight, 913 kg/m³ (the program's default ice density, `GLAZE_ICE_DENSITY`). Glaze is the densest ice, so of all ice of that weight it has the **smallest** iced diameter: the least wind on the iced wire, the least blow-out and string swing, and the least clearance lost to the iced surface. Lighter ice of the same weight (rime, wet snow) gives more of each. Example (the physics review, 2026-10-26): 20 N/m on a 28.1 mm wire is 60.3 mm iced as glaze, 77.4 mm at 500 kg/m³ — 28 % more wind, and the surface 8.5 mm further out. Only against the bare diameter is it more (the ice is counted at all). Exact: give the ice's density, or the thickness. The weather case table says so under the table for every case with wind and an ice load but no density, asking for one; the program picks no other density. Ice is a concentric shell of one thickness along the whole span (see [Sag-tension](../theory/sagtension.md#weather-loads)).

## Groups of cases

A case file may name **groups** of its cases (2026-10-25), each with a
description:

```toml
[[group]]
name = "Clearances"
description = "The cases the clearance checks take"
cases = ["max temperature", "glaze ice"]
```

In the app they are made in **Line › Criteria › Weather case groups…** (a name,
a description, the cases ticked; **Save** writes them into the file, one
change) and **picked as one** — a group's button adds its cases — in
**Line › Criteria › Loads for structures** and in the checks' case lists
(vertical and radial clearances, vegetation, safety volumes, wire
tensions, string checks, distance between conductors). **Groups are only
that** (2026-11-03): named lists of cases for picking several at once.
Until then structures and sections took groups of their own (a
**weather groups** column in the Structures and Sections tables) and
every check had a **structure override** and a **section override**
tick taking at those objects only the check's cases in their groups;
that way of giving local conditions is replaced by the structures'
[local conditions](#local-conditions) and one **Use local conditions**
switch per check. The groups still in a line file are read and kept, no
longer used: a check run says so once ("weather groups on structures
are no longer used: give local conditions"); the old ticks in rule
files (`[check]` `structure_override`, `section_override`) and in the
windows' kept setups (`overrides`) are read and ignored. Renaming a case in
the Weather cases table renames it in the groups too; a name no case has
(deleted, or renamed elsewhere) stays in its group, shown as such, not
refused. Group names are unique in the file.

### A wind study per structure

*Built, in active development, not approved yet* (owner, 2026-11-04:
"make the wind study local"). A wind study is a structure's **local
condition**: the Structures table's column **wind study Pa** (in the line
file its `local.wind_study`, Pa) — the wind pressure the study gives at
that structure for the conductors' distances to it. With **Use local
conditions** ticked, **Internal clearances** checks each structure with a
body and a wind study, in each weather case of its rules **with wind** (a
pressure above 0), a second time at the study's pressure in place of the
case's: those cases are solved again at it (for the whole line, the
results taken at those structures only) and its rows in them are that
run's, named `<case> (wind study 450 Pa)`. Structures without one keep the
case's wind (times their **wind clearance** factor). Unticked, no wind
study is taken (the notes say at how many structures one is given).

Two settings of the check, in its Setup (4 Wind study) and its rule
file's `[check]`:

| Key | Default | What |
|---|---|---|
| `wind_study_min` | none | The **least pressure** a wind study may give: a value below it is taken at it, said in the notes (a design code's bound; a design-code plugin's preset may bring it). |
| `wind_study_cases` | none: every case of the rules with wind | The weather cases that take the wind study, by name (`*` `?` wildcards); only those with wind take it. |

(The LeV's explanations, ESTI 240 Art. 33: a wind study may lower the
wind for the distances of the conductors to the structures down to
300 N/m², per EN 50341-1 — the check's least.) Only the internal
clearances take it. The study's runs are on the line as written (its
pressure in place of the case's), not times the structures' wind for
clearances; the spans' own local winds (Sections table) apply on it.

**Before (2026-10-25 to 2026-11-04)** a weather case group gave the wind
study: `structure_wind` named a Structures table column (a plugin's
`plugin.column`, e.g. `my-code.wind_study`), `structure_wind_min` its
least, and the group's cases took it. Those keys are still read and kept
in the weather case file, **no longer used**: a run says so in its notes.
Internal clearances › Setup › 4 Wind study offers **Copy into the
structures** while a structure's value in such a column differs from its
wind study: one change (Undo takes it back) that sets each structure's
`local.wind_study` to its value in the column (Pa, with or without the
unit; a value that is not a pressure is said and left out; two groups
giving one structure different values: the larger, said), keeps the
plugin's column (the plugin's data), and gives the check the groups'
least (the largest of theirs, on the safe side: more wind, more swing)
and their cases where it gives none.

### Local conditions

*Built, in active development, not approved yet* (owner, 2026-11-03).
A structure may give **local conditions** — the Structures table's
columns **wind static** (a factor on the weather cases' wind for
strength), **wind clearance** (a factor on their wind for clearances),
**wire ice** (a factor on the ice and wet snow on the wires of its
spans), **structure ice mm** (ice on its members), **snow s_k kN/m²**
and **snow h0 m** (the snow on the ground, given or from the reference
altitude), **wind study Pa** (a wind study's pressure for the internal
clearances, above); in the line file a structure's `local`
([Line file](line-file.md)). A span takes the **worst** of its two
structures' factors, or their **mean** (Line › Criteria › Calculation
options; a structure without one counts 1); a span's own local winds and
local ice (Sections table) are kept.

Every check has one switch beside **Run**: **Use local conditions**.
Unticked, the check takes the weather cases as written. Ticked, it
computes on the line with the local conditions on its spans, each of its
weather cases solved again on it (the solves shared by the checks of the
same wind, from the memo of solves). Which wind a check takes:

| Check | Wind | Why |
|---|---|---|
| Structure loads (and the usage) | for strength | the wires' tensions, ice and wind on the structures; also the wind on the structures' own wind bands, times their **wind static** |
| String checks | for strength | the strings' loads against their resistance |
| Wire tensions | for strength | the wires' tensions |
| Ice shedding | for strength | the tensions and sags before and after the ice drops |
| Galloping | for strength | the sag and tension sizing the ellipses |
| Internal clearances | for clearances | the swing of the strings and jumpers at the structures |
| Vertical and radial clearances, Clearances | for clearances | the wires' swing and sag |
| Distance between conductors (and under unequal wind) | for clearances | the conductors' swing and sag |
| Safety volumes | for clearances | the wires' swing building the volumes |
| Vegetation | for clearances | the wires' swing and sag over the vegetation |
| Lightning | for clearances | the wires' positions against the shield wires |

The **wire ice** factor is taken by every check with the switch ticked.
The **structure ice** is taken by every load case that takes the local
conditions (the switch, or the case's own **Use local conditions**) and
whose weather case has ice, unless the case gives ice of its own (owner,
2026-11-04: "yes apply local ice too"), and by the cases giving *the
structures' local ice*; the **snow** by the cases with *Snow on the
structures* (Load cases library › Ice and snow on the structures; [Load
cases: the structures' local ice](loads.md#the-structures-local-ice)).
In Checks › Structure loads the switch means the same as a load case's
**Use local conditions** (one rule, 2026-11-03): the wind on each
structure's body — its wind bands, the members of the usage check and a
pole — is times its **wind static** once ([Use local conditions: one
rule](loads.md#use-local-conditions-one-rule)).

When no structure gives local conditions, the switch changes nothing
and nothing is solved again (its hover says so, the notes too). The
check's notes say what was taken: "local conditions (wind for strength,
wire ice): N structure(s), spans worst; solved again". The switch's
hover says what the check takes the wind for, the structures giving
local conditions on this line, and the time of its last runs with and
without them (measured). Measured on the synthetic example line (an
optimised build, 2026-11-03): its five weather cases solved again on the
line with local conditions in about 50 ms (the line as written: 40 to
50 ms); a second check of the same wind takes them from the memo in
under 1 ms. The switch is kept with the check: in its rule
file's `[check]` (`local = true`) where it has one, else in its setup
kept in the project (`"local": true`); the structure loads' for the
session. The command line does not take it yet.

A rule preset's groups come with its weather cases into the project's
file (its cases under the project's names), a group of a name the file
has staying the file's.

### Local winds per span

A weather case's wind is the same along the whole line. Spans that see
another (an exposed ridge, a sheltered valley, a wind study for a
stretch) are given a **local wind factor** with the line, not in the
weather case file: a section's spans, each a factor on the wind of every
case with wind (roadmap O15, owner 2026-10-25) — see
[Line file: local winds per span](line-file.md#local-winds-per-span). The
factor applies to the pressure of the case as solved: in a check that
steps the wind (the swing arc, the distance under unequal wind) each
step is times the span's factor; in a wind study per structure (above)
the pressure of the study is taken as the case's, and the spans' factors
apply on it too.

**Simplification:** one factor for every case with wind — the ice with
wind as the wind alone — and the case's side; a factor per case or a
direction of its own is not given (to decide).

## Choosing cases

```bash
ohl line project.ohl --weather cases.ohlwc --tag clearance --pointcloud project.ohl --rules rules.ohlr
ohl line project.ohl --weather cases.ohlwc --case "glaze*" --case "max temperature"
```

`--case` takes names with `*` and `?` wildcards, `--tag` tags; both can be repeated and combined (a case is run if it matches any). Without them every case of the file runs. A name or tag that matches nothing is an error, so a typo cannot silently skip a case.

Each case is solved and checked against the clearance rules in turn; the ground and LiDAR points are loaded once for all cases. The clearance CSV has a `case` column and ends with the envelope over all cases (lowest clearance and governing case per wire and rule; see [clearance rules](terrain.md#envelopes-over-weather-cases)). The plan and profile sheets draw every case's wires; the 3D viewer and the wire CSVs show the first case (with violation markers of all cases).

Without `--weather`, one case comes from `--temperature`, `--wind-pressure` (with `--wind-from left|right`) and `--ice-thickness` or `--ice-load` (with `--ice-density`).

The finite elements carry the ice and wind per metre of the **stretched** wire (exact, the default) or, with the line's calculation option (Line › Criteria › Calculation options, the line file's `solver.loads_per`, `ohl line --loads-per unstretched`), per metre of the **unstretched** wire, as other programs appear to do it: about 0.1 % less ice and wind, slightly less sag and swing — a simplification on the unsafe side, for comparison ([Finite-element wire systems](../theory/wirefe.md#elastic-catenary-element)).

`ohl sagtension` jobs use the same case fields in their `[[case]]` lists.
