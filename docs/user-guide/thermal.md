# Thermal rating (`.olsta`)

OLS CAD computes the current a conductor can carry (its **ampacity**) for a
maximum conductor temperature, the conductor temperature for a current, the
temperature after a step of current, and — for a whole line — the highest
conductor temperature its clearances allow, by **IEEE Std 738-2012**,
**CIGRE TB 601** or the older **CIGRE TB 207**. The formulas are described
in the [theory manual](../theory/thermal.md).

## What the conductor needs

The cable's `[cable.electrical]` data in the wire library
([Wire library files](wire-library.md)): at least one **AC resistance**
(two or more at different temperatures — linear between and beyond them),
and preferably the **emissivity** and **solar absorptivity** (0.8 each when
neither the cable nor the ambient case gives them). The diameter comes from
the cable; the heat capacity for transients is estimated from its
components (aluminium outer wires, steel core) and its mass.

```toml
[cable.electrical]
ac_resistance = [
    { temperature = "25 °C", resistance = "0.07283 Ω/km" },
    { temperature = "75 °C", resistance = "0.08688 Ω/km" },
]
emissivity = 0.8
solar_absorptivity = 0.8
```

The shipped example `acsr-drake-26-7` has such data (the IEEE 738 example's
values; typical, not a manufacturer's).

## Ambient cases (.olsta)

The weather around the conductor, one `[[case]]` each, with units in the
values (`examples/synthetic-line/thermal-ambient.olsta`):

```toml
format = "ols-thermal-ambient"
version = 1
name = "Summer"

[[case]]
name = "summer noon"
description = "hot, sunny, light wind across the line"
air_temperature = "35 °C"
wind_speed = "0.6 m/s"
wind_angle = "90 °"       # between wind and conductor: 90 ° across (default);
                          # any angle, folded to 0–90° (180 ° = along the line)
elevation = "500 m"       # of the conductor above sea level (default 0)
latitude = "47 °"         # north positive
date = "2023-07-15"       # or day_of_year = 196
time = "13:00"            # local solar time
line_azimuth = "90 °"     # from north; leave out for the sun across the line
atmosphere = "clear"      # or "industrial"

[[case]]
name = "winter night"
air_temperature = "5 °C"
wind_speed = "1 m/s"
solar_flux = "0 W/m²"     # instead of the sun's position: irradiance on the conductor
```

| key | meaning |
|---|---|
| `air_temperature`, `wind_speed` | required |
| `wind_angle` | angle between wind and conductor axis, default 90 ° |
| `elevation` | default 0 m |
| `latitude`, `date` or `day_of_year`, `time` | the sun's position (local **solar** time) |
| `line_azimuth` | the conductor's direction from north; without it the sun shines across it |
| `atmosphere` | `clear` (default) or `industrial` |
| `solar_flux` | instead of the position: the irradiance on the conductor's projected area, W/m² — `0 W/m²` for night |
| `emissivity`, `absorptivity` | instead of the cable's |
| `albedo` | CIGRE: ground reflectance, default 0.1 |
| `clearness_ratio` | CIGRE: \\( N_s \\), default 1.0 clear, 0.8 industrial |
| `roughness` | CIGRE: surface roughness \\( R_s \\), default 0.1 (round outer wires) |

A case gives either `solar_flux` or the position, never both; unknown keys
are errors (a typo never goes unnoticed).

## Command line

Ampacity at a maximum temperature, per ambient case:

```bash
ols thermal ampacity --cable acsr-drake-26-7 \
    --ambient examples/synthetic-line/thermal-ambient.olsta --max-temp 80
```

```
Drake 795 kcmil 26/7 ACSR — IEEE 738-2012, 3 ambient cases
  summer noon                   840 A at 80 °C
    Joule 62.28, solar 23.18 (sun altitude 61.8°, azimuth 210.6°, 76.1° to the line), convection 59.29 (forced), radiation 26.16 W/m
  …
```

- `--current 900` instead of `--max-temp`: the steady-state temperature;
- `--current 1000 --from-current 500`: the temperature after switching from
  500 A (steady) to 1000 A, every `--print-every` s (300) for `--duration` s
  (3600), in `--time-step` s steps (10, or the default values'); `--defaults FILE.olsdef` (Criteria › Default values exported): the emissivity, absorptivity, roughness, albedo and time step where the cable or the case gives none; `--heat-capacity` J/(m·K) instead
  of the estimate;
- `--method cigre601` or `cigre207` (default `ieee738`); `--case NAME`
  (wildcards, repeatable) for some cases; `--csv FILE` for the results (the
  transient: every step); `--library` for another wire library.

### Highest temperature the clearances allow

```bash
cd examples/synthetic-line
ols thermal clearance-limit synthetic-line.ols.json --terrain synthetic-survey.csv \
    --clearance 5 --weather weather-cases.olswc --case "max temperature" \
    --ambient thermal-ambient.olsta --method cigre601 --csv limits.csv
```

```
clearance-limited temperature of 3 sections (0–150 °C, ±0.5 °C; base case "max temperature"; 1.8s):
  S1           acsr-drake-26-7      above 150 °C
      summer noon              at least 1424 A (CIGRE TB 601)
  …
  S2           acsr-drake-26-7      69.1 °C            ground span 2 vertical 4.99 m < 5.00 m at 69.4 °C
      summer noon                  655 A (CIGRE TB 601)
```

Each section is solved at other temperatures in the base weather case
(`--weather` and `--case`; still air without them) and checked by the
**ground and point rules** of `--rules` (and `--clearance` as a vertical
ground rule) that apply to that case, against `--terrain`, `--lidar` or
`--pointcloud` — the same options as `ols line`. The result per section:

- the limiting temperature (to `--tolerance`, 0.5 °C) with the governing
  span, rule and clearance just above it;
- *above* the range (`--max-temp`, 150 °C): nothing fails — the ampacity
  shown is at the top of the range, a lower bound;
- *below* the range (`--min-temp`, 0 °C): the section misses a clearance
  even then;

and the ampacity at the limit for every ambient case (`--ambient`,
`--ambient-case`). `--step` (10 °C) sets the scan before the bisection. The
CSV has one row per section: limit, status, governing rule, span, measure,
clearance, requirement, then one ampacity column per ambient case.

Wire-to-wire rules are not part of this check. A long line with LiDAR takes
up to about a second per section and limit.

## Desktop application

**Checks › Thermal rating…** opens a window:

- **Ambient cases** — the project's `.olsta` file (next to a line file) is
  taken at once;
  *Choose…* for another; each case in a few words below.
- **Method** and **maximum temperature** — the table shows at once the
  ampacity of every section in every ambient case (hover a value for the
  heat terms).
- **Weather case heated** and the temperature range — then
  **Clearance-limited temperature** runs the search in the background with a
  progress bar (*Cancel* stops it), using the rule file chosen under
  Checks › Wire clearances and the ground and LiDAR points of the checks. The
  table gains the limit per section, the governing span and the ampacity at
  the limit per ambient case.

Nothing in the model changes.
