# lev-ch — Swiss LeV / OLEI load assumptions

Weather and load cases following the method of the Swiss
Leitungsverordnung (LeV / OLEI, SR 734.31) for overhead lines: an
additional (ice / snow) load per metre that depends on the conductor
diameter and the altitude zone, wind on the conductors, their reduced
combination; loads unfactored, strengths divided by the safety factor.
**No values of the ordinance are included**: they come from your
parameter file.

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
