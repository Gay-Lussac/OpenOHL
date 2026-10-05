# go95 — CPUC General Order 95 loading

Weather and load cases following the method of California Public
Utilities Commission General Order 95: the Rule 43 loading district of
the line by elevation (ice, wind on the projected area, temperature);
loads unfactored, the Rule 44 safety factors dividing the strengths once
at installation and once at replacement. US customary input, converted
to SI in the files. **No values of GO 95 are included**: they come from
your parameter file.

## Parameter file

`example-parameters.toml` is the template (invented placeholders).

| Key | Meaning |
| --- | --- |
| `project.name`, `.condition` | set name; wire condition |
| `project.site_elevation` | ft: the district with the highest `from_elevation` at or below it applies |
| `project.district` | or the district by name (wins over the elevation) |
| `[[district]]` | `name`, `from_elevation` (ft), `ice_thickness` (in), `ice_density` (lb/ft³), `wind_pressure` (psf), `temperature` (°F) |
| `safety_factors.installation`, `.replacement` | strength factors 1 / factor |
| `[[extra_weather]]`, `[[extra_load_case]]` | further cases as given (SI numbers) |

## What is generated

One weather case (the district's loading) and two load cases: GO 95 at
installation, GO 95 at replacement. Not generated: longitudinal
(dead-end, broken conductor) loads.
