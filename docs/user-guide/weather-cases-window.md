# The Weather cases window

*Built, in active development, not approved yet* (owner, 2026-11-15: "make
it as for the checks with an explanation page of what's done where").

**Line › Criteria › Weather cases** opens every weather case of the
project in one table: a wire temperature, ice and wind per row. This page
says what each column holds and **where each value is used** in the
program. The file itself (`.ohlwc`, its fields and units) is described in
[Weather cases (.ohlwc)](weather.md). The **user guide ↗** button above
the table opens this page, **theory ↗** the
[theory of the weather loads](../theory/sagtension.md#weather-cases-in-the-calculations).

## What it answers

- Which weather the line is solved in: each row is one state of the wires
  (temperature, ice, wind) that the checks pick from.
- What a value changes: the table below says which calculation takes it.
- What a weather case is **not**: it carries no load factors and nothing
  about the structures. Factors, broken or removed wires, the wind and ice
  on the structure bodies belong to the **load cases**, which name a
  weather case ([Load cases](loads.md)).

## The columns

Every column, as the table shows it (its hover says the same):

| Column | Unit | What it holds | Empty |
|---|---|---|---|
| **#** | — | The order in the file; type 2.5 to move a row between 2 and 3. | — |
| **Name** | — | Unique name; everything naming the case (groups, load cases, rule files, check setups) follows a rename. | refused |
| **Description** | — | Free text. | none |
| **Tags** | — | Words for choosing cases, comma separated (`clearance, loads`); the command line's `--tag`. | none |
| **Air density factor** | Pa/(m/s)² | Q, relating pressure and velocity: P = Q·V². | 0.613 (air at 15 °C at sea level) |
| **Wind velocity** | m/s | V = √(P/Q), shown from the pressure; typed in, the pressure becomes Q·V². | no wind |
| **Wind pressure** | Pa | On the projected (iced) diameter of the wires, perpendicular to each span. **What the calculations use.** | no wind |
| **Ice thickness** | mm | Radial ice all round the wire. | no ice |
| **Ice density** | kg/m³ | With a thickness: the ice's weight. With a load: the iced diameter for the wind and the clearances. | glaze ice, 913 |
| **Ice load** | N/m | Or: the ice's weight per metre of wire, instead of a thickness (not both). | none |
| **Wire temp** | °C | The wires' temperature in the case. | refused |
| **Wind direction** | ° from | Where the wind comes from, relative to the line: 0 from ahead, 90 from the right, 180 from behind, 270 from the left, clockwise (or a name: ahead, right, back, left). 90 and 270 are across ([Wind on the line](../theory/wind.md)); another angle is one wind, each span its part normal to it. The file is written as version 2. | across: every span the full pressure, the side as View › Display weather cases shows it |
| **Ambient temp** | °C | The air temperature, kept with the case for ratings. | not given |

Under the table: where the cases are kept (the project's file), and each
case with wind and an ice **load** but no density — that ice is then taken
as glaze, the smallest iced diameter for its weight (**not on the safe
side** for the wind, the swing and the clearances; give the density). See
[How ice loads a wire](weather.md#how-ice-loads-a-wire).

Each row's **Show this weather case** button shows the line in that case
in the views. Changes are pending until **Apply** (one command, one
undo); **Revert** drops them.

## Where each value is used

| Value | Used by | How |
|---|---|---|
| Wire temperature | every solve of the wires: sag-tension, the finite-element wire systems, every check | the wire's thermal strain in the change of state ([Sag-tension](../theory/sagtension.md#change-of-state)) |
| Ice (thickness or load, density) | every solve of the wires | the ice's weight on the wire and its iced diameter, which takes the wind and to whose surface clearances are measured ([Weather loads](../theory/sagtension.md#weather-loads)) |
| Ice | the jumpers and the bodies on the wires (marker balls) | the same ice on them, as each object's own rule says; the strings take none ([Jumpers](../theory/jumpers.md), [wire objects](../theory/wirefe.md#point-loads-marker-balls-and-other-bodies)) |
| Ice | ice shedding, galloping | the iced state the check starts from ([Ice shedding](../theory/ice-shedding.md), [Galloping](../theory/galloping.md)) |
| Wind pressure | every solve of the wires | the wind load q·D on the iced diameter; the wire hangs in the plane of the resultant ([Weather loads](../theory/sagtension.md#weather-loads)) |
| Wind pressure | the strings' and jumpers' swing | the internal clearances, the radial clearances, the distance between conductors ([Internal clearances](../theory/internal-clearances.md), [Clearances](../theory/clearances.md)) |
| Wind pressure | the structure bodies | only through a **load case** naming this weather case: its pressure (or the load case's own) on the wind bands, members and poles, times the load case's factor ([Wind on the structure](../theory/structure-wind.md)) |
| Wind direction | every solve of the wires; the structure bodies through a load case | empty, 90 or 270: the across rule (as before); another angle: each span, string, object, jumper and post the one wind's part normal to it, the bodies the same direction unless the load case gives one ([Wind on the line](../theory/wind.md)) |
| Ice | the structure bodies | only through a **load case**: none by default, the structures' local ice, a thickness, or — `ice = "weather"` — this case's ice thickness times a factor ([Load cases, version 6](loads.md#version-6-the-wires-ice-on-the-structures-with-a-factor)) |
| Air density factor | the table only | turns a velocity into a pressure and back; no calculation takes it |
| Ambient temperature | kept with the case | no calculation takes it yet: Checks › Thermal rating takes its own ambient cases (`.ohlta`) |
| Tags | choosing cases | `ohl line --tag`; the checks' case lists show them |
| Name | everything that names the case | weather case groups, load cases, rule files, the checks' kept setups |

**Where the values are changed on the way** (never in the file):

- **Local conditions** of the structures, with a check's **Use local
  conditions** ticked: the wind times each span's **wind static** (for
  strength) or **wind clearance** factor (for clearances), the ice on the
  wires times its **wire ice** factor ([Local conditions](weather.md#local-conditions)).
- **Local winds per span**: a section's spans may take a factor on every
  case's wind ([Local winds per span](weather.md#local-winds-per-span)).
- **Load factors** are never applied here: they belong to the load cases,
  on the force components ([Load cases](loads.md)).

## The menus around it

| Menu | What |
|---|---|
| Line › Criteria › Weather cases | this table |
| Line › Criteria › Merge duplicate weather cases… | cases with the same weather under other names merged into one, everything naming them following |
| Line › Criteria › Weather case groups… | named groups of cases, picked as one in the checks' case lists ([Groups of cases](weather.md#groups-of-cases)) |
| View › Display weather cases | the cases shown, each named with its wind (e.g. *high wind — from the right (90°)*), and the wind: each case as it is (across: from the right, 90°), mirrored about the line (across: from the left, 270°; at an angle θ: 360° − θ), or both |

## On the safe side

- An ice load without a density is glaze ice: **not on the safe side**
  for the wind, the swing and the clearances (said under the table).
- Ice is a concentric shell of one thickness along the whole span — a
  simplification either way ([Weather loads](../theory/sagtension.md#weather-loads)).
- The wind is one pressure along the whole line except where local
  conditions or local winds say otherwise.
- A case with a direction is **that one wind only**, not the across
  envelope: its results say so; the across cases stay the usual worst for
  transverse loads and side clearances.

## The command line

```bash
ohl line project.ohl --weather cases.ohlwc --tag clearance
ohl line project.ohl --weather cases.ohlwc --case "glaze*" --case "max temperature"
```

See [Choosing cases](weather.md#choosing-cases).
