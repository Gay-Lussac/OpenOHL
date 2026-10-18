# Wire library files (`.olsw`)

A **wire library** describes conductors, shield wires and other cables. Each file can hold any number of cables. OpenOHL loads every `*.olsw` file of a library directory (default `libraries/cables/`), or a single file given in a job.

- **Fill-in template:** [`libraries/cables/TEMPLATE.olsw.example`](https://github.com/Gay-Lussac/OpenOHL/blob/main/libraries/cables/TEMPLATE.olsw.example) — every field, commented.
- **Machine-readable schema:** [`docs/schema/ols-wire-1.schema.json`](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/schema/ols-wire-1.schema.json) (JSON Schema 2020-12) — for tools that create or check files.
- **Working example:** `libraries/cables/acsr-examples.olsw`.

## File format at a glance

A `.olsw` file is plain **TOML** text (any text editor works; editors highlight it if you associate `.olsw` with TOML). Rules:

1. One `[[cable]]` block per cable; sub-tables `[cable.outer]`, `[cable.core]` and their `stress_strain` follow it.
2. **Quantities are text with a unit:** `"28.14 mm"`, `"1.094 lbm/ft"`, `"31200 lbf"`. The space is optional (`"28.14mm"`). A bare number means SI (m, m², N, kg/m, Pa, K) — convenient for tools, risky by hand.
3. A bare number is never guessed: `"12.5"` (text without unit) is an error.
4. Unknown field names are errors (typos are caught, not ignored).
5. `#` starts a comment.

## Header (optional)

| Field | Value | Meaning |
|---|---|---|
| `format` | `"ols-wire"` | Identifies the file. Any other value is rejected. |
| `version` | `1` | Format version. Newer versions than the program supports are rejected. |

## `[[cable]]`

| Field | Required | Type / units | Notes |
|---|---|---|---|
| `id` | yes | text: letters, digits, `-`, `_` | Unique across all loaded libraries; used by jobs (`cable = "…"`). |
| `name` | yes | text | Display name. |
| `kind` | yes | one of `acsr`, `acss`, `aac`, `aaac`, `acar`, `accc`, `opgw`, `adss`, `steel`, `other` | Family. |
| `source` | recommended (required in the shipped libraries) | text | Data sheet, standard, revision. |
| `diameter` | yes | length: `m`, `mm`, `km`, `ft`, `in`, `mi` | Overall diameter. |
| `mass_per_length` | yes | mass per length: `kg/m`, `lbm/ft` | **Mass**, not weight. `lb/ft` is a force unit and is rejected here. Data sheets often give lb per 1000 ft: 1094 lb/kft = `"1.094 lbm/ft"`. |
| `rated_tensile_strength` | yes | force: `N`, `kN`, `lbf`, `lb`, `kip`, `kips` | RTS / RBS / UTS. |
| `outer` | yes | component table | The whole cable for homogeneous conductors (AAC, AAAC, steel…). |
| `core` | no | component table | Core of bimetallic / composite conductors (ACSR steel core…). |
| `model` | no (`"nonlinear"`) | `"nonlinear"`, `"linear_creep_tension"`, `"linear_creep_temperature"` | The cable model (below). |
| `creep_temperature_shift` | linear models | number, °C | The permanent stretch due to creep as a temperature increase. |
| `creep_shift_tension` | `linear_creep_tension` | force | The creep weather case tension at which the stretch equals `creep_temperature_shift`. |
| `bimetallic_behavior` | no (the line's choice) | `"compression"`, `"bird_cage"` | Cables with a core: what the outer strands do above the transition temperature (below). Not given: the line's bimetallic conductor model (Criteria › Bimetallic conductor model; compression unless chosen otherwise). |
| `max_virtual_compression` | `bimetallic_behavior = "compression"` (default `"68947.4 MPa"`, 10⁷ psi: in practice no limit) | stress | The outer strands' largest virtual compressive stress. |

### Cable models

The three models of line design programs, used for the conditions after creep and after load (a section's calculation, [line file](line-file.md#sections); the weather cases from Criteria › Creep-Stretch):

| `model` | Behaviour | After creep | After load |
|---|---|---|---|
| `nonlinear` | separate polynomials for the initial and creep behaviour of the outer and core materials (`stress_strain`; a component without them is linear) | the permanent set of the creep curve at the creep weather case | the largest permanent set of the initial curve among the load weather cases |
| `linear_creep_tension` | linear elastic | stretch of `creep_temperature_shift` (α·ΔT), scaled by the creep weather case tension over `creep_shift_tension` | none (elastic) |
| `linear_creep_temperature` | linear elastic | stretch of `creep_temperature_shift` (α·ΔT) | none |

**Bimetallic cables** (aluminium on steel…): give the outer layers and the core each their own `stress_strain` (initial and creep polynomials, final modulus, thermal expansion), on the total area (`basis = "total"`: the actual material values × component area / total area, as tables give them).

The outer strands expand more with temperature than the core. Above a transition temperature they are no longer in tension. What they do then is a property of the conductor, `bimetallic_behavior`. A cable that does not give it (as older cable files) follows the line's **bimetallic conductor model** (Criteria › Bimetallic conductor model…, kept with the line's settings: one undoable edit, every section solved again); a cable that gives it keeps its own:

| `bimetallic_behavior` | Above the transition temperature |
|---|---|
| `compression` (default) | the outer strands go into compression, up to `max_virtual_compression` |
| `bird_cage` | the outer strands take no compression: the core carries the whole tension |

Virtual stress = actual stress × Ao / At (Ao the outer strands' area, At the total area). Past the limit the core alone carries the tension, so a hot cable expands as the core does. Both methods use this: the ruling span through the cable's stress-strain model; the finite elements through the same curves when the cable has them (each span's length follows the curves, see [Finite-element wire systems](../theory/wirefe.md#non-linear-stress-strain-in-the-elements)), otherwise with one branch per section (the core alone when, at the section's mean tension, the outer strands are past their limit). A curve's own `max_compression` (below) still applies, whichever is smaller.

## Components: `[cable.outer]`, `[cable.core]`

| Field | Required | Type / units | Notes |
|---|---|---|---|
| `area` | yes | area: `mm²`/`mm2`, `cm²`/`cm2`, `m²`/`m2`, `in²`/`in2`, `kcmil`/`MCM` | Area of **this component only**. |
| `modulus` | yes | stress: `Pa`, `kPa`, `MPa`, `N/mm²`, `GPa`, `psi`, `ksi`, `psf` | Final (elastic) modulus; used by the linear model and when there are no curves. |
| `thermal_expansion` | yes | plain number, 1/K | e.g. `23.0e-6` (aluminium), `11.5e-6` (steel). |
| `stress_strain` | no | table, see below | Non-linear curves. Without it the component is linear. |
| `strands` | no | whole number ≥ 1 | Number of wires of this component, e.g. 26 and 7 for a 26/7 ACSR; their diameter follows from the area. Gives the cable's bending stiffness with its strands slipping, the least a jumper's estimated stiffness can be (see [Jumpers](../theory/jumpers.md)). Give it for every component or none. |

The cable's composite stiffness and expansion are computed from the components (\\( EA = \sum E_k A_k \\), \\( \alpha = \sum E_k A_k \alpha_k / EA \\)); do not enter composite values.

## Non-linear curves: `[cable.outer.stress_strain]`, `[cable.core.stress_strain]`

The curves follow the published polynomial method:

\\[ \sigma(\varepsilon) = a_0 + a_1\varepsilon + a_2\varepsilon^2 + a_3\varepsilon^3 + a_4\varepsilon^4 + \dots \\]

Any number of coefficients (at least two) is accepted; published curves are usually degree 4.

| Field | Required | Type | Default | Notes |
|---|---|---|---|---|
| `strain` | no | `"percent"` or `"unit"` | `"percent"` | Unit of \\( \varepsilon \\) in the polynomials (0.1 % strain is `0.1` in percent, `0.001` in unit). |
| `stress_unit` | yes | unit symbol (text): `Pa`, `kPa`, `MPa`, `N/mm²`, `GPa`, `psi`, `ksi`, `psf` | — | Unit of \\( \sigma \\) and of every coefficient's stress part. |
| `basis` | no | `"component"` or `"total"` | `"component"` | Area the stress refers to. Use `"total"` when the source gives the stress on the **whole cable** area for each component curve (common in tables for ACSR). |
| `initial` | yes | list of numbers `[a0, a1, a2, …]` | — | Initial (virgin loading) curve. Coefficient \\( a_k \\) has units stress / strainᵏ. |
| `creep` | no | list of numbers | none | Creep curve (typically 10-year). Omit for no creep (e.g. steel cores). |
| `final_modulus` | yes | number | — | Unloading / reloading slope, in `stress_unit` per strain unit (per % if `strain = "percent"`), on the same `basis`. |
| `max_compression` | no | number ≥ 0 | unlimited | Largest compressive stress carried, in `stress_unit`. Usually set with the cable's `bimetallic_behavior` instead. |
| `max_strain` | no | number > 0 | 1 % | Strain up to which the curves are valid, in strain units. The curves must rise up to it. |
| `reference_temperature` | no | temperature: `°C`, `degC`, `K`, `°F`, `degF` | `"20 °C"` | Temperature of the curve tests. Both components must use the same. |

### How the curves are used

- Both components stretch by the same amount; each one's thermal strain \\( \alpha_k (t - t_0) \\) is removed before its curve is applied; the cable tension is the sum of the component loads.
- **Initial** state: loading along `initial`. **After creep**: from `creep`. **Final**: elastic along `final_modulus` from the permanent set left by creep or by the heaviest load, whichever is larger — see the [sag-tension theory](../theory/sagtension.md).

### Converting a data sheet

| The source gives… | Write |
|---|---|
| strain in %, stress in psi on the total area, coefficients A0…A4 for the whole cable and B0…B4 for the core | Outer = whole − core is **not** automatic: if the source gives "complete cable" and "core" curves, derive the outer curve as (A − B) coefficient by coefficient, then `basis = "total"`, `stress_unit = "psi"`, `strain = "percent"` for both. |
| strain in %, stress in MPa on each component's own area | `basis = "component"`, `stress_unit = "MPa"`, `strain = "percent"`. |
| final modulus in psi (on the same basis as the curves) | `final_modulus` as a number in psi per unit strain: divide by 100 when `strain = "percent"` (e.g. 10.0×10⁶ psi → `final_modulus = 1.0e5`). |
| "10-year creep" coefficients | `creep = [...]`. |

## Validation when loading

OpenOHL refuses a library with any of these problems, naming the cable and the field:

- unknown fields, missing required fields, unknown units, duplicate `id`s, `id` with characters other than letters, digits, `-`, `_`;
- non-positive `diameter`, `mass_per_length`, `rated_tensile_strength`, `area`;
- `modulus` outside 1–500 GPa or `thermal_expansion` outside 0–50×10⁻⁶ /K (catches mm² vs m², MPa vs GPa, a missing `e-6`);
- total area larger than the area of a circle of the given diameter;
- a curve that stops rising before `max_strain`; a non-positive `final_modulus`; components with different `reference_temperature`s;
- a `format` other than `"ols-wire"`, or a `version` newer than supported.

## Minimal example (homogeneous, linear)

```toml
format = "ols-wire"
version = 1

[[cable]]
id = "aaac-example"
name = "Example AAAC"
kind = "aaac"
source = "Data sheet …"
diameter = "21.0 mm"
mass_per_length = "0.75 kg/m"
rated_tensile_strength = "75 kN"
outer = { area = "270 mm²", modulus = "60 GPa", thermal_expansion = 23.0e-6 }
```

## For tool developers

1. Parse with any TOML 1.0 library; validate the parsed object against `docs/schema/ols-wire-1.schema.json`.
2. Write quantities as `"<number> <unit>"` with the units listed above, or as SI numbers.
3. Run the semantic checks listed above, or let OpenOHL do it: `ohl sagtension` and the library loader report the first problem with its file, cable id and field.
4. Keep comments and field order stable when rewriting files, so manual edits survive (TOML libraries that preserve formatting, such as `toml_edit` in Rust or `tomlkit` in Python, help).

## Electrical data: `[cable.electrical]`

Optional electrical and thermal data of the cable, for losses, impedances and thermal rating. The voltage and the phases are not cable data: they belong to the line's [electrical systems](line-file.md#electrical-systems), and sections link the two through their `cable`.

```toml
[cable.electrical]
ac_resistance = [
    { temperature = "20 °C", resistance = "0.0618 Ω/km" },
    { temperature = "40 °C", resistance = "0.0661 Ω/km" },
]
dc_resistance = { temperature = "20 °C", resistance = "0.0605 Ω/km" }
geometric_mean_radius = "11.779 mm"
emissivity = 0.6
solar_absorptivity = 0.5
```

| Field | Type / units | Notes |
|---|---|---|
| `ac_resistance` | list of `{ temperature, resistance }` | Resistance per length: `Ω/m`, `Ω/km`, `Ω/mi`, `Ω/kft` (or `ohm/…`). Linear between the temperatures given, and beyond them along the same line. Give two temperatures (e.g. 25 °C and 75 °C) for exact data; a single value is scaled to other temperatures with the temperature coefficient of the cable's material (aluminium 0.00403 /K for the aluminium kinds and OPGW, steel 0.0045 /K); kinds `other` and `adss` need two. |
| `dc_resistance` | `{ temperature, resistance }` | |
| `geometric_mean_radius` | length | For inductance; at most the cable's radius. |
| `emissivity`, `solar_absorptivity` | number 0–1 | Surface coefficients for [thermal rating](thermal.md) (0.8 when not given). |

Cable files of other line design programs often hold the same data (AC resistances at two temperatures, DC resistance, geometric mean radius, emissivity); no importer for them is built.

