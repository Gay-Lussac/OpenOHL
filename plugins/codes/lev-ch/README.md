# lev-ch — Swiss LeV / OLEI load assumptions

Weather and load cases following the method of the Swiss
Leitungsverordnung (LeV / OLEI, SR 734.31) for overhead lines: an
additional (ice / snow) load per metre that depends on the conductor
diameter and the altitude zone, wind on the conductors, their reduced
combination; loads unfactored, strengths divided by the safety factor.
For the load cases **no values of the ordinance are included**: they come
from your parameter file. It also runs the **distance between conductors
in the span** (Annex 6, 2.2), with the ordinance's values (below).

## Parameter file

`example-parameters.toml` is the template (invented placeholders).

| Key | Meaning |
| --- | --- |
| `project.name`, `project.condition` | set name; wire condition (`initial`, `creep`, `load`, `max_sag`) |
| `site.altitude` | m above sea level: chooses the altitude zone |
| `site.reference_diameter` | mm: the conductor diameter the additional load is computed for |
| `[[altitude_zone]]` | lowest first; the first with `up_to` ≥ the site altitude applies |
| `altitude_zone.up_to` | m |
| `altitude_zone.additional_load_base`, `.additional_load_per_mm` | additional load = base + per_mm × d, N/m |
| `altitude_zone.additional_load_density` | kg/m³ (iced diameter for the wind) |
| `altitude_zone.wind_pressure` | Pa on the projected conductor |
| `temperatures.additional_load`, `.wind`, `.minimum` | °C |
| `load_with_wind.load_factor`, `.wind_factor` | the combination: fractions of the additional load and of the wind pressure |
| `safety.normal` | safety factor S; strength factor 1/S |
| `[[extra_weather]]`, `[[extra_load_case]]` | further cases as given (SI numbers) |

## What is generated

LeV normal load (additional load, no wind), LeV wind, LeV additional load
with wind, LeV minimum temperature; all factors 1, strength 1/S. Wires of
another diameter need their own file (the load is per metre for the
reference diameter). Not generated: exceptional load cases (one-sided
additional load, broken conductor).

## Table column

The manifest adds an **insulation level** column (low / high, not set:
high) to the app's Electrical circuits table: the row of LeV Table 2.4.1
for the circuit's lightning impulse withstand level. It is kept on the
circuit's sections as `lev-ch.insulation_level`, and the distance between
conductors in the span reads it there.

## Distance between conductors in the span (check)

The plugin's `check` interface (`phase-distance`): OLS CAD measures the
conductors (pairs, stations, distance d, angle β, local sag f, l_k, the
largest sags between phase spacers), the plugin says what is required at
each station:

- 2.2.1: z = k·√(f + l_k) + C₁ within one system, + C₂ between systems
  (2.2.3: the larger z of the two conductors);
- 2.2.4, conductors joined by phase spacers: z = k·√d_f + C₃ ≥ C₁
  (+ C₄ ≥ C₂ between systems), at least 0.90 m;
- k from Diagram 2.4.2 or 2.4.3 (a choice) by β and the swing class
  w = d/q; C by the higher nominal voltage and the circuit's insulation
  level; +14 % per 1000 m above 1000 m.

The values — Table 2.4.1 (C₁–C₄, both insulation levels with their
impulse withstand voltages) and the k diagrams — are the ordinance's
(Swiss legislation is not protected by copyright), in
`phase-distance.olspd`; an admin may edit a project's copy in the
check's window. Tests: `cargo test` here (native).
