# en50341 — EN 50341-1 general approach + national annex

Weather and load cases following the method of EN 50341-1 (Section 4,
actions on lines, general approach) with the combinations of EN 1990
§6.4.3.2 (dominant action with γ, accompanying action with γ·ψ), and a
national normative annex (NNA) overlay in the same file. **No values of
the standard are included**: they come from your parameter file.

## Parameter file

`example-parameters.toml` is the template (invented placeholders).

| Key | Meaning |
| --- | --- |
| `project.name` | name of the generated sets |
| `project.condition` | wire condition of every load case: `initial`, `creep`, `load`, `max_sag` |
| `wind.basic_velocity` | v_b, m/s |
| `wind.air_density` | ρ, kg/m³ |
| `wind.exposure_factor` | c_e(h) |
| `wind.structural_factor` | G_c (conductors) |
| `wind.drag_factor` | C_c, bare conductor |
| `wind.temperature` | °C with the extreme wind |
| `wind.min_temperature_velocity_factor` | wind at minimum temperature, fraction of v_b |
| `ice.load` or `ice.thickness` | I_k in N/m, or radial thickness in mm |
| `ice.density`, `ice.temperature` | kg/m³, °C |
| `ice_wind.ice_factor` | ice with wind, fraction of I_k (or of the thickness) |
| `ice_wind.velocity_factor` | wind with ice, fraction of v_b |
| `ice_wind.drag_factor` | C_c of the iced conductor |
| `temperatures.minimum` | °C |
| `partial_factors.permanent`, `.wind`, `.ice` | γ_G, γ_W, γ_I |
| `partial_factors.psi_wind`, `.psi_ice` | ψ of the accompanying wind / ice |
| `partial_factors.material` | γ_M; strength factor 1/γ_M |
| `nna.country`, `nna.notes` | shown in the notes |
| `nna.<table>` | any table above given again: its keys replace the general ones |
| `[[nna.extra_weather]]`, `[[nna.extra_load_case]]` | national cases, as in `.ohlwc` / `.ohllc` but numbers in SI (°C, Pa, mm, N/m, kg/m³) |

## What is generated

Wind pressure on conductors q = c_e · ½ρ(f·v_b)² · G_c · C_c.

| Load case | Weather | Vertical | Transverse | Longitudinal |
| --- | --- | --- | --- | --- |
| EN 1a extreme wind | extreme wind | γ_G | γ_W | γ_W |
| EN 1b wind at minimum temperature | reduced wind, t_min | γ_G | γ_W | γ_W |
| EN 2 ice | ice | γ_I | γ_I | γ_I |
| EN 3a ice dominant with wind | ice with wind | γ_I | γ_W·ψ_W | γ_I |
| EN 3b wind dominant with ice | ice with wind | γ_I·ψ_I | γ_W | γ_W |
| EN 4 minimum temperature | t_min | γ_G | γ_G | γ_G |

Strength factor 1/γ_M in all. Not generated: security loads (broken
wire, unbalanced ice), construction and maintenance loads.
