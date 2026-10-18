# Weather cases (`.ohlwc`)

A weather case is a cable temperature, ice and wind. Cases are kept in a
file and each run chooses the ones it needs.

```toml
format = "ohl-weather-cases"
version = 1
name = "Project X"

[[case]]
name = "max temperature"
tags = ["clearance"]
temperature = "80 °C"

[[case]]
name = "glaze ice"
tags = ["clearance", "loads"]
temperature = "-5 °C"
ice_thickness = "12.7 mm"
ice_density = "913 kg/m³"

[[case]]
name = "rime ice + wind"
tags = ["loads"]
temperature = "-5 °C"
ice_load = "20 N/m"
ice_density = "500 kg/m³"
wind_pressure = "200 Pa"
```

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | Unique name. |
| `description` | no | Free text. |
| `tags` | no | Words for choosing cases (`clearance`, `loads`, …). |
| `temperature` | yes | Cable temperature. |
| `ice_thickness` | no | Radial ice thickness. |
| `ice_load` | no | Or: ice weight per length of wire (not both). |
| `ice_density` | no | With a thickness: the ice weight (default glaze ice, 913 kg/m³). With a load: the iced diameter for the wind; without it, wind acts on the bare diameter. |
| `wind_pressure` | no | Wind pressure on the projected (iced) diameter, perpendicular to each span. |
| `air_density_factor` | no | Q, Pa/(m/s)²: the wind pressure of a wind velocity V is Q·V² (default 0.613, ½ρ of air at 15 °C at sea level). Only relates pressure and velocity (the Weather cases table shows and takes both); the loads use `wind_pressure`. |
| `ambient_temperature` | no | Air temperature, e.g. `"35 °C"`, for ratings; the wires use `temperature`. |
| `wind_from` | no | Files of earlier versions only: the side is now chosen where the wires are shown (desktop app: View › Display weather cases — right, left or both; the command line still reads it, right by default, and `ohl line --wind-from` sets it for a single case). A project's weather case file loses it when the project is opened; a file read directly is not changed (its value is not used by the app). |

Values take units (`"0.5 in"`, `"4 psf"`, `"1.2 lbf/ft"`, …).

## How ice loads a wire

On a bare wire of diameter *d* and weight *w*:

- **thickness *t***: ice area A = π/4 [(d + 2t)² − d²], ice weight ρ g A, iced diameter D = d + 2t;
- **load *w*ᵢ**: ice weight as given; with a density ρ the ice area is A = wᵢ / (ρ g) and the iced diameter D = √(d² + 4A/π) — light rime of the same weight as dense glaze presents more area to the wind; without a density, D = d.

The vertical load is w + ice weight, the wind load q·D, and the wire hangs in the plane of their resultant.

## Choosing cases

```bash
ohl line project.ohl --weather cases.ohlwc --tag clearance --pointcloud project.ohl --rules rules.ohlr
ohl line project.ohl --weather cases.ohlwc --case "glaze*" --case "max temperature"
```

`--case` takes names with `*` and `?` wildcards, `--tag` tags; both can be repeated and combined (a case is run if it matches any). Without them every case of the file runs. A name or tag that matches nothing is an error, so a typo cannot silently skip a case.

Each case is solved and checked against the clearance rules in turn; the ground and LiDAR points are loaded once for all cases. The clearance CSV has a `case` column and ends with the envelope over all cases (lowest clearance and governing case per wire and rule; see [clearance rules](terrain.md#envelopes-over-weather-cases)). The plan and profile sheets draw every case's wires; the 3D viewer and the wire CSVs show the first case (with violation markers of all cases).

Without `--weather`, one case comes from `--temperature`, `--wind-pressure` (with `--wind-from left|right`) and `--ice-thickness` or `--ice-load` (with `--ice-density`).

`ohl sagtension` jobs use the same case fields in their `[[case]]` lists.
