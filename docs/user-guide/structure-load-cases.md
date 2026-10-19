# Structure load cases (`[[load_case]]`, `.ohlslc`)

A structure carries **load cases of its own**: recipes that say how the
loads the line gives at a placed structure act on it. The structure
application evaluates them for each placed structure into a **load
table**: per case and point (or member), the transverse, longitudinal and
vertical load. Specification: [Building structures](../development/structure-building.md)
§2 (decisions 5 to 7).

Status: the recipes, their files, the evaluation and the table's CSV
are built, in active development (not approved yet)
(`apps/structure/ohl-structure-loads`). The structure application edits
them in the **Load recipes** and **Load table** panels (Load cases menu)
and solves the table's cases in its **FE results** panel (Results menu)
([The structure application](structure-app.md#load-cases)).
**Open in OHL-Structure…** on a placed structure in the line application opens the
structure with that place's line values and keeps the values typed over
its table with it ([Started for a placed
structure](structure-app.md#started-for-a-placed-structure)). XLSX
import and export of the table and of the recipes come later; CSV works
today.

## Recipes

A load case is a list of loads. Each load says **what**, **how** and
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

### What the line gives

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
at every point, in each case the line sends (Criteria › Loads for
structures, see [Loads for structures](desktop-app.md#loads-for-structures)):
its load cases (all by default) and the weather cases ticked to be sent
on their own, named `weather: <name>`, every factor 1 (factored =
unfactored). A recipe names the line's case by that name. A value the line does not give makes a row marked **no
line value** (its force 0); so does every line value of a structure
opened on its own. Such values can be typed into the table.

### How a load acts

- **A line value without a direction** acts along its own axis, signed
  as the line gives it; a tension acts along the line towards its span
  (`tension_back` back, `tension_ahead` ahead), or towards `side`.
- **A line value with a direction**: its size along the direction.
- **A value of the case** (`value`): along its direction, signed.
- **A pressure**: pressure × the member's area along the direction, the
  total per member, along the direction. The area comes from the
  [section library](section-library.md): the profile's width seen along
  the direction (in the member's cross-section plane) × the member's
  length × sin θ (θ between the direction and the member; across the
  member: width × length). No drag factor. A member whose section is not
  in the library has **no area** (the row says why).

The load is the value × `factor` × `one_sided`.

## The `[[load]]` tables: case `model`

A structure's `[[load]]` tables (nodal forces at joints, structure axes,
z up; [Structure analysis](structure-analysis.md)) are read as one case
named `model`, its rows at the joints (vertical turned to down). A
moment given there is not in the table (the finite-element check reads it
from `[[load]]`).

## The load table

One row per case, load and point (or member, or joint):

| Column | Meaning |
|---|---|
| `case`, `load` | The case and the load's number in it (empty: a typed row of its own) |
| `at`, `label` | `point`, `member` or `joint`, and its label or id (empty: the load picks nothing) |
| `transverse`, `longitudinal`, `vertical` | N; vertical down |
| `source` | `line` (a line value through the recipe), `recipe` (a value or pressure of the case), `typed` |
| `evaluated_…` | A value typed over an evaluated one: the evaluated value, kept |
| `missing` | Why there is no value: "no line value: …", "no area: …", "no point …", "selects no point" |
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

## Structure load-case files (`.ohlslc`)

The recipes on their own, to export them from one structure and import
them into another:

```toml
format = "ohl-structure-load-cases"
version = 1
name = "Synthetic example structure load cases"

[[load_case]]
name = "construction"
self_weight = 1.1

[[load_case.load]]
value = "1.5 kN"
direction = "-z"
points = "all"
```

The same `[[load_case]]` tables as in the structure file; `format` and
`version` as in every OpenOHL file. The recipes are also written and
read as CSV: one row per load (`case`, `description`, `line_case`,
`strength`, `self_weight`, `load`, `what`, `value` in N, `pressure` in
Pa, `load_line_case`, `factor`, `one_sided`, `side`, `direction`,
`points`, `sets`, `phases`, `members`, `note`; lists separated by `;`).

Examples (invented, not from a design code):
`examples/synthetic-line/Structures/type-02.ohls` (three cases and its
`[[load]]`s) and `examples/synthetic-line/structure-load-cases.ohlslc`.
