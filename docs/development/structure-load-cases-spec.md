# Structure load cases: line forces and the structure's own loads (spec)

Status: agreed with the owner (2026-10-25); all four steps built, in
active development (not approved yet): decisions taken within it in the
[decision log](decisions.md) (2026-10-25). Replaces how the
line's loads reach a structure and how a structure load case is put
together; the load table, its typed values and the solve stay.

## Why

Today a structure load case is a flat list of "loads": each takes a line
value (`vertical`, `transverse_factored`, …) of a line load case with a
factor, or a force, or a pressure. Factors come from two places (the
line's `.ohllc` load cases and the recipe), a weather case's wind blows
from one side only, and what acts on the structure, from where, is hard
to follow (owner: "for now it's a bit hard for a human to understand how
things are applied and when").

## The flow (owner, 2026-10-25)

```
 OHL-Design (the line)                          OHL-Structure (a structure)
 ─────────────────────                          ───────────────────────────
 weather cases sent ──► line results ──────────► load case "ULS wind left"
  (Criteria › Loads      unfactored forces at      description: …
   for structures)       each attachment point     strength factor
                         ─ no wind: one result     ┌ 1. Line forces ───────────────┐
                         ─ wind: two results,      │ line result ▾  V   T   L  at  │
                           wind from the left      │ storm, left   1.35 1.5 1.0 all │
                           and from the right      └───────────────────────────────┘
                                                   ┌ 2. Loads on the structure ────┐
                                                   │ self weight × 1.35            │
                                                   │ wind on the body: from left,  │
                                                   │   650 Pa × 1.5                │
                                                   │ ice on members: 20 mm, 900    │
                                                   │   kg/m³ × 1.0                 │
                                                   │ point loads: 2 kN down at A1  │
                                                   └───────────────────────────────┘
                                                            │
                                                            ▼
                                                   load table → FE check
```

1. **The line gives forces, nothing else.** Every weather case sent gives
   the wires' and strings' forces at each attachment point, unfactored,
   the wires in one condition (chosen once: as strung by default). A
   weather case with wind gives **two line results**, the wind from the
   left and from the right ("storm — wind from the left"); without wind,
   one. The line's `.ohllc` load cases are no longer sent (they stay for
   the line's own checks: structure usage, etc.).
2. **All factors are in the structure load case** — one place.
3. **A structure load case has two parts**, each with its own factors:
   - **Line forces**: one row per line result taken, with a vertical, a
     transverse and a longitudinal factor, at all points or at some sets.
   - **Loads on the structure itself**: its self weight (factor); the
     wind on its body (from the left, right, ahead, back, or the four 45°
     directions; a pressure of its own or the pressure of a line result's
     weather case; a factor); additional weight on its members (ice as a
     radial thickness and density, or a load per metre; all or chosen
     members; a factor); point loads (a force, direction, points).
   - A **description** (free text, several lines) and the strength factor.
4. **The window says what the case does in words**, generated from it:
   "storm, wind from the left: vertical × 1.35, transverse × 1.5,
   longitudinal × 1.0 at every point · self weight × 1.35 · wind on the
   body from the left, 650 Pa × 1.5 · ice 20 mm (900 kg/m³) on every
   member × 1.0". Each part has one line saying what it does and where it
   ends up in the load table.

## Line side (OHL-Design)

- Criteria › **Loads for structures**: the weather cases sent (and their
  groups), the wire condition, and a line explaining that a wind case is
  sent twice. The load-case list goes (a project that ticked load cases:
  said once, the choice kept in the file but not used).
- Line results named `<weather case> — wind from the left` / `— wind from
  the right` (no wind: `<weather case>`); each solved as that weather case
  with the wind on that side. Cost: a wind case is solved twice (seconds
  for a long line; the same budget per section).
- The line-values file to the structure application carries the line
  results (name, weather case, side, the weather case's wind pressure, so
  a body wind can take it).

## Structure side (OHL-Structure)

File: `[[load_case]]` (structure file version 7, `.ohlslc` version 3):

```toml
[[load_case]]
name = "ULS wind left"
description = """Ultimate limit state, wind from the left.
Factors from the utility's design basis, table 3."""
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
from = "left"                         # left, right, ahead, back, ahead_left, …
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

- Older files (`[[load_case.load]]`, `line_case`, `self_weight` on the
  case) are read as they are and shown under **Loads in the older form**
  in the case (editable, evaluated as today); nothing is converted
  silently. A **Convert** button turns the line loads and self weight of
  an older case into the new parts where that is exact (same line case,
  one factor per direction, same points), and says what it could not.
- **Wind on the body** uses the same member wind as the line's own
  structure checks (`ohl_structures_fe` member wind with the section
  library's wind factors: faces, solidity, shielding), for the chosen
  direction, instead of today's pressure × projected area: the two
  applications then give the same body wind.
- **Ice on members**: thickness t on each member, a uniform layer on its
  **full perimeter** (owner, 2026-10-25: an angle's inside faces too, on
  the safe side): ice area per metre = perimeter × t + π t² (the corners'
  rounding; *conservative assumption*: a real layer bridges the inside
  corner). With wind in the same case the member's width for the wind
  grows by 2 t (owner: yes; *conservative*). A load per metre is taken as
  given, down.
- The load table gets its rows from the parts (source `line`, `self
  weight`, `wind`, `ice`, `point`), so a row says which part made it.

- **Each line force can be turned off** (owner, 2026-10-25: "to simulate
  the works on the line with a conductor removed"): a line-forces row has
  a tick per attachment point (its label, set and phase); unticked, that
  point's line forces are left out of the case (`off = ["A3"]`), said in
  the case's sentence ("without A3").
- **Kept in the structure**: the load cases are written in the structure
  file (`[[load_case]]`), as today, and travel with it; `.ohlslc` only to
  copy them between structures.

## UI (OHL-Structure, Load cases)

One window: the cases on the left (name, the first line of the
description), the case on the right:
1. **Name**, **Description** (multi-line), **Strength factor**, and the
   generated sentence of what it does.
2. **Line forces** (collapsible): a table — line result ▾ (the results
   the line sent, each with its weather case's values on hover),
   vertical, transverse, longitudinal factors, at (every point / sets ▾),
   remove; **Add line forces**. Placed structure without line results:
   says where they come from (Criteria › Loads for structures).
3. **Loads on the structure itself** (collapsible), four sub-parts, each
   with **Add**: self weight (factor), wind on the body (from ▾,
   pressure / of a line result ▾, factor), ice and additional weight
   (thickness + density / load per metre, members, factor), point loads.
4. **Loads in the older form** (only when the case has some).

Case buttons: new, duplicate, delete, **Mirror wind** (a case with wind
from the left copied with everything "left" turned "right": line results
and body wind) — the usual pair of cases made in one click.

## Order

1. Line side: two-sided line results, the Loads for structures menu, the
   line-values file.
2. Recipe model v7/v3 (parts, older form kept), evaluation into the load
   table (body wind through the member wind, ice), CSV.
3. The Load cases window.
4. Docs (structure-load-cases.md rewritten around the flow; desktop-app
   Loads for structures; data-model; decisions), examples rewritten.

## Decided (owner, 2026-10-25)

- The line sends weather cases, unfactored; a wind case twice (left,
  right). Factors only in the structure load case, per direction for the
  line forces.
- Ice on the structure: a thickness (and density) or a load per metre;
  the full perimeter; iced members widened by 2 t for the wind.
- Body wind: left, right, ahead, back and the four 45° directions.
- One wire condition for every line result, as strung by default.

## Later

- Design-code plugins write `.ohllc` load cases today; they could write
  structure load cases (`.ohlslc`) instead.
