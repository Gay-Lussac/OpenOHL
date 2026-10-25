# Structure load cases (`[[load_case]]`, `.ohlslc`)

A structure carries **load cases of its own**. Each says how the forces
the line gives at a placed structure, and the loads on the structure
itself, act on it; the structure application evaluates them for each
placed structure into a **load table** — per case and point (or member),
the transverse, longitudinal and vertical load — and solves it.
Specification: [Structure load cases: line forces and the structure's
own loads](../development/structure-load-cases-spec.md) (owner,
2026-10-25), and [Building structures](../development/structure-building.md)
§2 (decisions 5 to 7).

Status: built, in active development (not approved yet)
(`apps/structure/ohl-structure-loads`). The structure application edits
the cases in the **Load case editor** and shows the **Load table**
(Load cases menu), and solves the table's cases in its **FE results**
panel (Results menu) ([The structure application](structure-app.md#load-cases)).
**Open in OHL-Structure…** on a placed structure in the line application
opens the structure with that place's line values and keeps the values
typed over its table with it ([Started for a placed
structure](structure-app.md#started-for-a-placed-structure)). XLSX
import and export come later; CSV works today.

## The flow

```
 OHL-Design (the line)                      OHL-Structure (a structure)
 weather cases sent ──► line results ──────► load case
  (Criteria › Loads      unfactored forces    1. line forces: a line result,
   for structures)       at each point           V / T / L factors, points off
                         a wind case twice:   2. loads on the structure itself:
                         from the left and       self weight, wind on the body,
                         from the right          ice and weight, point loads
                                                        │
                                                        ▼
                                              load table ──► finite-element check
```

1. **The line gives forces, nothing else.** Each weather case ticked in
   Criteria › Loads for structures ([Loads for
   structures](desktop-app.md#loads-for-structures)) is a **line
   result**: the wires' and strings' forces at each attachment point,
   unfactored. A weather case with wind is two line results, `<case> —
   wind from the left` and `<case> — wind from the right`; without wind,
   one, named as the weather case.
2. **All the factors are in the structure load case**, in one place.
3. **A load case has two parts**, each with its own factors, with a
   description (several lines) and a strength factor.

```toml
[[load_case]]
name = "ULS wind left"
description = """Ultimate limit state, wind from the left.
Invented factors."""
strength = 1.0

[[load_case.line]]                    # 1. line forces
result = "storm — wind from the left"
vertical = 1.35
transverse = 1.5
longitudinal = 1.0
sets = [1, 2]                         # optional (default: every point)
off = ["A3"]                          # points whose line forces are left out

[load_case.structure]                 # 2. loads on the structure itself
self_weight = 1.35

[[load_case.structure.wind]]
from = "left"
pressure = "650 Pa"                   # or: pressure_of = "storm — wind from the left"
factor = 1.5

[[load_case.structure.ice]]
thickness = "20 mm"                   # or: load = "15 N/m"
density = "900 kg/m³"
members = "all"
factor = 1.0

[[load_case.structure.point]]
value = "2 kN"
direction = "down"
points = ["A1"]
```

(Invented values: they show the format, they are not from a design
code.)

| Case field | Required | Default | Notes |
|---|---|---|---|
| `name` | yes | | Unique. `model` is the name of the `[[load]]` tables' case. |
| `description` | no | | Several lines (`"""…"""`). |
| `strength` | no | 1 | Strength factor of the checks; above 0. |

Every key is checked: a mistyped one is refused, naming it.

## 1. Line forces (`[[load_case.line]]`)

One table per line result taken:

| Field | Default | Notes |
|---|---|---|
| `result` | | The line result, by its name (as the line sent it). |
| `vertical`, `transverse`, `longitudinal` | 1 | The factor on each axis (vertical down, transverse right of the line, longitudinal ahead). |
| `sets` | every point | Only the points carrying these sets. |
| `off` | none | Points (labels) whose line forces are left out — works on the line with a conductor removed. |

In the load table: per point carrying a wire (a set) or given a value by
the line in that result, one row (source `line`, row **1001** for the
first table, 1002 for the second, …): the line's transverse,
longitudinal and vertical force × its factors. A point turned off keeps
its row with no force, noted "off" (a value typed over it stays on the
same row whether it is off or on). A value the line does not give makes
the row **no line value** (force 0; so does every line value of a
structure opened on its own); it can be typed into the table.

## 2. Loads on the structure itself (`[load_case.structure]`)

| Sub-part | Fields | In the load table |
|---|---|---|
| `self_weight` | a factor (0 or more) | No rows: the solve puts each member's own weight (× its dead-load factor) × this factor on the members. |
| `[[…wind]]` | `from`: `left`, `right`, `ahead`, `back`, or 45° between them `ahead_left`, `ahead_right`, `back_left`, `back_right`; `pressure` (Pa) **or** `pressure_of` (a line result: its weather case's wind pressure, unfactored); `factor` (default 1) | One row per member (source `wind`, rows 2001, …). |
| `[[…ice]]` | `thickness` and `density`, **or** `load` (N/m); `members` (`"all"`, the default, or ids); `factor` (default 1) | One row per member (source `ice`, rows 3001, …), down. |
| `[[…point]]` | `value` (a force), `direction`, `points` / `sets` / `phases` (as in the older form) | One row per point (source `point`, rows 4001, …). |

**The self weight** given here is the case's. The case's older
`self_weight` must then be left at 1 (a file giving both differently is
refused: "the self weight is given twice"); without one here, the older
`self_weight` (default 1) is the factor.

**Wind on the body**: the same member wind as the line's own structure
checks ([Wind on lattice structures](../theory/structure-wind.md)): per
member q × c × η × d × L × sin²θ, the drag c of its face (by solidity) or
its shape and the shielding η from the cross-section library's `[wind]`
factors (none given: the model's generic drag, no shielding — the row
notes say which), for the chosen direction, the pressure × the factor;
the row is the member's whole force (normal to it, as the line's checks
apply it). A member whose cross-section is not in the library has no
area (its row says why). The two applications so give the same body
wind.

**Ice and additional weight**: a thickness t of a density ρ all round
each member's outline: its weight per metre ρ g (P t + π t²), P the
outline's **full perimeter** — an angle's inside faces too (owner,
2026-10-25). *Conservative assumption*: a real layer bridges an angle's
inside corner; taking every face is on the safe side. A load per metre is
taken as given. Either × the member's length × the factor, down. Several
ice loads each weigh on the bare outline. **With a wind on the body in
the same case**, each iced member is 2 t wider for the wind (owner: yes;
the sum of 2 t of each ice load on it). *Conservative assumption*: the
iced outline is taken as the bare one grown by t on both sides, its
corners not rounded; the faces' solidity and shielding stay those of the
bare members.

**On an extension and on a pole** (the FE results run): the wind on the
body and the ice act on the extension's own members as on the body (a
note per load); the wind on the body (pressure × factor, from its
direction) is also the wind on a pole (× the pole's drag); the ice is not
on the pole (the pole check takes no ice of its own; said in the case's
notes).

## What the case does, in words

The editor says what each case does, made from it, its parts separated
by "·": *storm — wind from the left: vertical × 1.35, transverse × 1.5,
longitudinal × 1.0 at every point (without A3) · self weight × 1.35 ·
wind on the body from the left, 650 Pa × 1.5 · ice 20 mm (900 kg/m³) on
every member × 1.0 · 2 kN down at A1* (`ohl_structure_loads::describe`).

## Convert and Mirror wind

- **Convert** (a case with loads in the older form) turns them into the
  parts where that is exact: the line's **unfactored** transverse,
  longitudinal and vertical values of one line case at the same points
  (every point, or the same sets), with no share, side, direction or
  phase — one line forces table per line case and points (an axis given
  twice: its factors added; one not given: factor 0); a `value` at points
  — a point load; the case's `self_weight` — the structure's self
  weight. A line case `weather: X` becomes the line result `X`; a weather
  case the line now sends twice (wind from the left and from the right)
  stays, said ("choose one"). The rest stays in the older form, said: the
  factored values (the line sends its forces unfactored now), shares and
  sides, the pressures (the wind on the body is the member wind now, not
  the same rule). One undo step.
- **Mirror wind** copies a case with the wind from the other side:
  "left" and "right" swapped in its name, description, line results and
  wind on the body (`ahead_left` ↔ `ahead_right`, …; ahead and back
  stay); the points turned off and any loads in the older form stay as
  they are. A case of the mirrored name there already is not replaced.

## The load table

One row per case, load and point (or member, or joint):

| Column | Meaning |
|---|---|
| `case`, `load` | The case and the row's number in it: an older load 1 to 999, the line forces from 1001, the wind on the body from 2001, the ice from 3001, the point loads from 4001 (each part's first item, second, …: a value typed over a row keeps matching when another part changes); shown as load 2, line 1, wind 1, …; empty: a typed row of its own |
| `at`, `label` | `point`, `member` or `joint`, and its label or id (empty: the load picks nothing) |
| `transverse`, `longitudinal`, `vertical` | N; vertical down |
| `source` | The part the row comes from: `line` (line forces, or an older load taking a line value), `wind`, `ice`, `point`, `recipe` (an older load's own value or pressure), `typed` |
| `evaluated_…` | A value typed over an evaluated one: the evaluated value, kept |
| `missing` | Why there is no value: "no line value: …", "no area: …", "no outline: …", "no member wind: …", "no point …", "selects no point", "off: …" |
| `note` | |
| `area` | A surface load: the member's area it acts on, m² (may be left out when read) |

**Typed values** stay as given and are marked `typed`; the evaluated
value is kept beside them. Evaluated again (new line values, a recipe
changed), the typed values are put back over the new table. A typed row
with no load number is a load of its own (e.g. from a client's load
tree).

The structure application keeps the typed values beside the structure
file as `<name>.loads.csv` (`type-02.ohls` → `type-02.loads.csv`): the
table's CSV with only the typed rows, written when the structure is
saved and read when it is opened. Started for a placed structure (say
T2), as `<name>.T2.loads.csv` instead: each place has its own loads
(decision 7), and in a project the line application keeps the file in
the project with that placed structure (project library kind `loads`).
A table read back from CSV types what
it has beyond the evaluation (other values, rows of its own), so a table
exported and imported again is the same table.

The table is written and read as CSV with these columns (numbers in N);
a spreadsheet with the same columns reads back.


## The `[[load]]` tables: case `model`

A structure's `[[load]]` tables (nodal forces at joints, structure axes,
z up; [Structure analysis](structure-analysis.md)) are read as one case
named `model`, its rows at the joints (vertical turned to down). A
moment given there is not in the table (the finite-element check reads it
from `[[load]]`).

## Loads in the older form

Cases written before 2026-10-25 (and any case that still has them)
carry a list of loads (`[[load_case.load]]`), read and evaluated as
before; the Load case editor shows them under **Loads in the older
form**, and **Convert** turns what is exact into the two parts
(above). In the older form a load case is a list of loads. Each load says **what**, **how** and
**where**:

```toml
[[load_case]]
name = "ice, unbalanced"
description = "the ice case's vertical loads × 1.3, and 2 kN along the line at set 1"
line_case = "ice"          # the line's load case giving the values
strength = 0.9             # strength factor of the checks (default 1)
self_weight = 1.0          # factor on the structure's own weight (default 1)

[[load_case.load]]
what = "vertical"          # a value the line gives
factor = 1.3
points = "all"

[[load_case.load]]
value = "2 kN"             # a value of the case itself
direction = "y"
sets = [1]

[[load_case.load]]
what = "tension_max"
one_sided = "40 %"         # 40 % on one side only…
side = "ahead"             # …this one
sets = [1]

[[load_case]]
name = "body wind"

[[load_case.load]]
pressure = "0.6 kPa"       # a surface load on the members' area
direction = "x"
members = "all"
```

| Case field | Required | Default | Notes |
|---|---|---|---|
| `name` | yes | | Unique. `model` is the name of the `[[load]]` tables' case. |
| `description` | no | | |
| `line_case` | no | | The line's load case (`.ohllc`, by name) whose values the loads take. A load may name another. |
| `strength` | no | 1 | Strength factor of the checks; above 0. |
| `self_weight` | no | 1 | Factor on the structure's own weight; 0 or more. |

| Load field | Notes |
|---|---|
| `what` | A value the line gives at the point (table below), or the line case's wind on the body (`structure_wind`, `structure_wind_factored`: on `members`). |
| `value` | Or a force of the case itself (`"2 kN"`); needs a `direction`. |
| `pressure` | Or a pressure on the members' area (`"0.6 kPa"`); needs `members` and a `direction`. |
| `line_case` | The line's load case for this load (default: the case's). |
| `factor` | Default 1. |
| `one_sided` | A share (`0.4` or `"40 %"`) on one side only; needs `side`. |
| `side` | `ahead` or `back`: points whose strain strings point to the other side are left out; a tension acts towards this side. |
| `direction` | In the structure's axes: `x` (right of the line), `y` (ahead), `z` (up), each also with `-`; `right`, `left`, `ahead`, `back`, `up`, `down` read too. |
| `points` | `"all"` or a list of labels (points, or wire points a wire is fixed to). |
| `sets` | Or the points carrying these sets (through their strings). |
| `phases` | Or the points carrying these phases (the line's phase labels; known only with the line). |
| `members` | `"all"` or member ids: a surface load. |
| `note` | Free text. |

A load gives one of `what`, `value`, `pressure`, and says where. `points`,
`sets` and `phases` together pick the points that match all of them.
Every key is checked: a mistyped one is refused, naming it.

#### What the line gives

Forces in the structure's axes: transverse to the right of the line,
longitudinal ahead, vertical down.

| `what` | Meaning | From the line today |
|---|---|---|
| `transverse`, `longitudinal`, `vertical` | The wires' and strings' force at the point, unfactored | yes |
| `transverse_factored`, `longitudinal_factored`, `vertical_factored` | The same, factored by the line's load case | yes |
| `tension_back`, `tension_ahead` | The wire tension in the back / ahead span | not yet: typed |
| `tension_max` | The larger of the two | not yet: typed |
| `wire_wind` | The wind on the wires, across the line | not yet: typed |
| `ice` | The ice on the wires, down | not yet: typed |
| `structure_wind` | A pressure (Pa) on `members`: the line case's wind on the structure body, unfactored | yes |
| `structure_wind_factored` | The same × the case's structure wind factor: the pressure the line's own structure checks put on the body | yes |

The two structure winds are the load case's own values for the
structures when its `.ohllc` gives them (`[case.structure]`,
[Load cases](loads.md#wind-on-the-structures-apart-from-the-wires-casestructure)),
else its weather case's pressure and its transverse factor: one rule with
the line's checks. They act on `members` like a `pressure` (× area, the
total per member), blowing as the line case's wind (`-x` with the wind
from the right) unless the load gives a `direction`. A structure file
using one is version 6, a `.ohlslc` version 2 (OHL-Structure raises the
file when a load takes one; older programs say "newer version").

The line's [structure loads](loads.md#structure-loads) give the first six
at every point, in each **line result** the line sends (Criteria › Loads
for structures, see [Loads for structures](desktop-app.md#loads-for-structures)):
each weather case ticked there, unfactored (factored = unfactored), named
as the weather case; one with wind twice, `<case> — wind from the left`
and `<case> — wind from the right` (since 2026-10-25; before, the line's
load cases and `weather: <name>`). A recipe names the line result by that
name. A value the line does not give makes a row marked **no
line value** (its force 0); so does every line value of a structure
opened on its own. Such values can be typed into the table.

#### How a load acts

- **A line value without a direction** acts along its own axis, signed
  as the line gives it; a tension acts along the line towards its span
  (`tension_back` back, `tension_ahead` ahead), or towards `side`.
- **A line value with a direction**: its size along the direction.
- **A value of the case** (`value`): along its direction, signed.
- **A pressure**: pressure × the member's area along the direction, the
  total per member, along the direction. The area comes from the
  [cross-section library](section-library.md): the profile's width seen along
  the direction (in the member's cross-section plane) × the member's
  length × sin θ (θ between the direction and the member; across the
  member: width × length). No drag factor. A member whose cross-section is not
  in the library has **no area** (the row says why).

The load is the value × `factor` × `one_sided`.

## Structure load-case files (`.ohlslc`)

The cases on their own, to copy them from one structure to another (the
structure file keeps its own: they travel with it):

```toml
format = "ohl-structure-load-cases"
version = 3
name = "Synthetic example structure load cases"

[[load_case]]
name = "construction"

[load_case.structure]
self_weight = 1.1

[[load_case.structure.point]]
value = "1.5 kN"
direction = "down"
points = "all"
```

The same `[[load_case]]` tables as in the structure file; `format` and
`version` as in every OpenOHL file. **Versions**: a `.ohlslc` is written
at 3 when a case has the parts (line forces, loads on the structure
itself), else at 2 when an older load takes the line's `structure_wind`,
else at 1; a structure file with the parts is version 7 (OHL-Structure
raises it after an edit leaving one). An older program says "newer
version" and does not read it.

The cases in the older form are also written and read as CSV: one row
per load (`case`, `description`, `line_case`, `strength`, `self_weight`,
`load`, `what`, `value` in N, `pressure` in Pa, `load_line_case`,
`factor`, `one_sided`, `side`, `direction`, `points`, `sets`, `phases`,
`members`, `note`; lists separated by `;`). A case with the parts is not
written as CSV (said): export it as a `.ohlslc`.

Examples (invented, not from a design code):
`examples/synthetic-line/Structures/type-02.ohls` (cases of both parts
and of the older form, and its `[[load]]`s) and
`examples/synthetic-line/structure-load-cases.ohlslc`.
