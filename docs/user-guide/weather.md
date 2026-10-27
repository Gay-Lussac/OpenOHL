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
tensions, string checks, distance between conductors). **Structures and sections** take groups of their own — the **weather
groups** column of the Structures and Sections tables, ticked in its dropdown (the case file's groups only, nothing typed; a group name has no semicolon)
(a wind study's lower winds at some pylons; the snow load varying along
the line by section). Every check has two ticks — beside **Run**, in the vertical and radial
clearance windows under *2 Scope*:
**structure override** and **section override**. Ticked, a result at a
structure (a string, a structure's loads) or on a span takes only the
check's cases that are in that structure's or section's groups — a span
is at its section and at both its ends — and is left out in the others;
objects without groups take every case of the check. A case is still
computed for the line as a whole: the overrides choose which results
count where. Checks with them: vertical and radial clearances, vegetation,
safety volumes, wire tensions, string checks, distance between conductors
(also under unequal wind: a pair only in a case both its spans take),
galloping, and the structure loads (the structure override only: their
loads are summed per structure from every section). The ticks of the
clearance windows, wire tensions and string checks are kept in their
rule files (`[check]` `structure_override`, `section_override`); the
others' for the session. Renaming a case in
the Weather cases table renames it in the groups too; a name no case has
(deleted, or renamed elsewhere) stays in its group, shown as such, not
refused. Group names are unique in the file.

### A wind study per structure

A group may take its wind at each structure from a **Structures table
column** (2026-10-25): `structure_wind` names the column (a plugin's
`plugin.column`, e.g. `lev-ch.wind_study`), `structure_wind_min` the
least pressure it may give.

```toml
[[group]]
name = "LeV conductors–structure, short approach"
cases = ["LeV 10 °C wind (Annex 1 no. 37)"]
structure_wind = "lev-ch.wind_study"
structure_wind_min = "300 Pa"
```

With the **structure override** ticked, the internal clearances check a
structure with a value in that column in the group's cases at that
pressure: those cases are solved again at it (for the whole line, the
results taken at those structures only) and its rows named
`<case> (wind study 450 Pa)`. A value under the least is taken at the
least, said in the notes; a value that is not a pressure is said and the
case's own wind taken. Structures without a value keep the case's wind.
(The LeV's explanations, ESTI 240 Art. 33: a wind study may lower the
wind for the distances of the conductors to the structures down to
300 N/m², per EN 50341-1.) Only the internal clearances take it so far.

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
