# csa-c22-3 — CSA C22.3 No. 1 loading zones and construction grades

Weather and load cases following the method of CSA C22.3 No. 1 (overhead
systems): the loading zone of the line gives the combined ice, wind and
temperature; the construction grade gives the overload factors and the
strength factor. **No values of the standard are included**: they come
from your parameter file. For the reliability-based CAN/CSA-C22.3
No. 60826 use the `iec60826` plugin.

## Parameter file

`example-parameters.toml` is the template (invented placeholders).

| Key | Meaning |
| --- | --- |
| `project.name`, `.condition` | set name; wire condition |
| `project.zone`, `.grade` | the `[[zone]]` and `[[grade]]` used, by name |
| `[[zone]]` | `name`, `ice_thickness` (mm), `ice_density` (kg/m³), `wind_pressure` (Pa), `temperature` (°C) |
| `[[grade]]` | `name`, `vertical`, `transverse`, `longitudinal` (overload factors), `strength` |
| `temperatures.minimum` | °C |
| `[[extra_weather]]`, `[[extra_load_case]]` | further cases as given (SI numbers) |

## What is generated

CSA zone loading (the grade's factors), CSA minimum temperature. Not
generated: broken-conductor and unbalanced loads.
