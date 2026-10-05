# asce-mop74 — ASCE MOP 74 structural loading

Weather and load cases following the method of ASCE Manual of Practice 74
(guidelines for electrical transmission line structural loading): extreme
wind on wires with the pressure Q·k_z·K_zt·V²·G_w·C_f at the wind speed of
the chosen return period, ice with a concurrent wind, load factors γ to
adjust to the return period, a strength factor. **No values of the manual
(or of ASCE 7 maps) are included**: they come from your parameter file.

## Parameter file

`example-parameters.toml` is the template (invented placeholders).

| Key | Meaning |
| --- | --- |
| `project.name`, `.condition`, `.return_period` | set name; wire condition; years (for the record) |
| `wind.speed` | V, m/s (3-s gust) |
| `wind.air_density_factor` | Q, Pa/(m/s)² |
| `wind.exposure_factor`, `.topographic_factor`, `.gust_response_factor`, `.force_coefficient` | k_z, K_zt, G_w, C_f |
| `wind.temperature` | °C |
| `ice.thickness`, `.density` | mm, kg/m³ |
| `ice.concurrent_wind_speed`, `.force_coefficient`, `.temperature` | m/s, C_f of the iced wire, °C |
| `temperatures.minimum` | °C |
| `load_factors.weight`, `.wind`, `.ice`, `.concurrent_wind`, `.strength` | γ factors and strength factor |
| `[[extra_weather]]`, `[[extra_load_case]]` | further cases as given (SI numbers) |

## What is generated

MOP 74 extreme wind, MOP 74 ice with concurrent wind, MOP 74 minimum
temperature. Not generated: wind on structures, failure containment
(longitudinal) and construction and maintenance loads.
