# nesc — NESC (ANSI C2) Rule 250 loadings

Weather and load cases following the method of the National Electrical
Safety Code, Section 25: Rule 250B combined ice and wind of the line's
loading district, Rule 250C extreme wind and Rule 250D extreme ice with
concurrent wind (pressure = constant · k_z · G · C_f · V²), each with the
construction grade's load factors and a strength factor. US customary
input, converted to SI in the files. **No values of the code are
included**: they come from your parameter file.

## Parameter file

`example-parameters.toml` is the template (invented placeholders).

| Key | Meaning |
| --- | --- |
| `project.name`, `.condition`, `.grade` | set name; wire condition; construction grade (for the record) |
| `project.district` | the loading district: one of the `[[district]]` names |
| `[[district]]` | `name`, `ice_thickness` (in), `wind_pressure` (psf), `temperature` (°F), `constant_k` (lb/ft) |
| `ice.density` | lb/ft³ |
| `extreme_wind.enabled`, `.speed` (mph), `.temperature` (°F) | Rule 250C |
| `extreme_wind.velocity_pressure_constant`, `.exposure_factor`, `.gust_response_factor`, `.force_coefficient` | constant, k_z, G, C_f (also used for 250D) |
| `extreme_ice.enabled`, `.ice_thickness` (in), `.wind_speed` (mph), `.temperature` (°F) | Rule 250D |
| `load_factors.district/.extreme_wind/.extreme_ice` | `vertical`, `transverse`, `longitudinal` |
| `strength_factors.district`, `.extreme` | strength factors |
| `[[extra_weather]]`, `[[extra_load_case]]` | further cases as given (SI numbers) |

## What is generated

NESC 250B district loading, NESC 250C extreme wind, NESC 250D extreme ice
with wind. The district's constant k (added to the resultant) cannot be
put in a weather case: the notes give it in N/m to add by hand. Not
generated: longitudinal (broken wire, dead-end) and construction loads.
