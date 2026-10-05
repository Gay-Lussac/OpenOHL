# iec60826 — IEC 60826 reliability-based loading

Weather and load cases following the method of IEC 60826 (design
criteria of overhead transmission lines): climatic loads of the return
period of the reliability level, wind q0 = ½·τ·μ·(K_R·V_R)² on conductors
times C_xc·G_c·G_L, reference ice weight, the two ice–wind combinations
(high ice with low wind, low ice with high wind), use factors γ and the
global strength factor. **No values of the standard are included**: they
come from your parameter file. (CAN/CSA-C22.3 No. 60826 follows the same
method.)

## Parameter file

`example-parameters.toml` is the template (invented placeholders).

| Key | Meaning |
| --- | --- |
| `project.name`, `.condition`, `.reliability_level` | set name; wire condition; level (for the record) |
| `wind.reference_speed` | V_R, m/s |
| `wind.roughness_factor` | K_R |
| `wind.air_density_correction`, `wind.air_density` | τ, μ (kg/m³) |
| `wind.drag_factor`, `.gust_factor`, `.span_factor` | C_xc, G_c, G_L |
| `wind.temperature` | °C |
| `ice.weight`, `.density`, `.temperature` | g_R (N/m), kg/m³, °C |
| `combined.high_ice_factor`, `.low_wind_factor` | high ice with low wind (fractions of g_R, V_R) |
| `combined.low_ice_factor`, `.high_wind_factor` | low ice with high wind |
| `combined.drag_factor`, `.temperature` | C_xc of the iced conductor, °C |
| `temperatures.minimum` | °C |
| `factors.weight`, `.wind`, `.ice` | factor on weight, γ_W, γ_I |
| `factors.strength` | global strength factor φ_R |
| `[[extra_weather]]`, `[[extra_load_case]]` | further cases as given (SI numbers) |

## What is generated

IEC wind, IEC ice, IEC high ice with low wind, IEC low ice with high
wind, IEC minimum temperature. Not generated: security (failure
containment) and safety (construction and maintenance) loads.
