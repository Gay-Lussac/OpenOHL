# Galloping, blowout, vegetation and as-built wires

Analyses of a solved line beyond the clearance rules: galloping ellipses,
blown-out wires against the right of way, LiDAR classification, vegetation
near the wires and catenaries fitted to the surveyed wires. Each is a
command of `ols` and (but for classification and the right of way) a window
of the desktop app's **Checks** menu. The methods are in the theory manual:
[Galloping ellipses and blowout](../theory/galloping.md),
[LiDAR analyses](../theory/lidar-analysis.md).

The commands share the options of `ols line` for the model: the line or
project file, `--library`, `--structures`, `--insulators`; and for weather
cases `--weather FILE.olswc` with `--case NAME` / `--tag TAG` (repeatable;
all cases without them).

## Galloping ellipses (`.olsg`)

```bash
ols clearance galloping synthetic-line.ols.json \
    --weather weather-cases.olswc --params galloping-example.olsg --csv galloping.csv
```

Every pair of wires between the same two structures, by every method of the
parameter file, in each weather case the file applies to: the smallest
distance between their ellipses along the span, where, and the ellipses'
sizes; worst first. The parameter file holds **all** coefficients — copy
them from the method you design to; the example's are invented:

```toml
format = "ols-galloping"
version = 1
name = "My galloping method"
required = "0.3 m"                  # or { by_voltage = [[0, "0.2 m"], [245, "0.5 m"]] }
pair_voltage = "higher"             # higher, sum or difference (as in rule files)
stations = 20                       # ellipses compared at 19 inner stations
cases = ["galloping*"]              # weather cases (default: all chosen)

[[method]]
name = "single loop"
loops = 1
major = { sag = 1.2, constant = "0.3 m" }
minor = { major = 0.4 }
tilt = 10.0                         # major axis from vertical, top to the right, °
centre_above_rest = 0.25            # centre above the wire at rest, × major axis
along = "sine"                      # or "constant"
```

| Axis term | Meaning |
|---|---|
| `sag` | × the span's sag in the weather case, m |
| `span` | × the horizontal span length |
| `sqrt_sag` | × √sag (√m) |
| `diameter` | × the conductor diameter |
| `log = { factor, scale }` | factor · n · d · ln(scale · sag / (n² d)) |
| `constant` | a length |
| `major` | × the major axis (minor axis only) |

CSV columns: case, method, both spans, distance, required, overlap, where
(fraction of the span), both ellipses' axes, the first wire's position.

In the desktop app: **Checks › Galloping ellipses…** — choose the parameter
file (a `.olsg` next to the model is chosen first) and the weather case,
then **Check**.

## Blowout and the right of way

```bash
ols clearance row synthetic-line.ols.json --weather weather-cases.olswc \
    --case "high wind" --half-width 10 --csv row.csv
```

How far each wire swings to the right and to the left of its route's
centreline (alignment, or the line through the structures) in each case,
bundles included, and how far it goes beyond the right of way (`--left`,
`--right` for different widths each side). Clearances of the blown-out wires
to buildings, trees and other wires are the radial clearances of a rule file
run in the windy case (`ols line --rules`, the Clearances tab).

## Phase to phase and circuit to circuit

Rule files' `same_line` rules take `circuits = "same"` (phase to phase) or
`"different"` (circuit to circuit, phase to earth wire), with `case_pairs`
for one wire in one weather case and the other in another — e.g. one circuit
at maximum sag and the other with its ice dropped:

```toml
[[rule]]
name = "Circuit to circuit, ice drop"
target = "same_line"
circuits = "different"
radial = { by_voltage = [[0, "2 m"], [245, "3.5 m"]] }
case_pairs = [["glaze ice", "everyday"]]
```

## LiDAR classification

```bash
ols lidar classify tiles/ --out project.ols#classified --line line.ols.json --corridor 100
ols lidar classify scan.laz --out scan-classified.laz
```

Classifies unclassified points (classes 0 and 1; `--reclassify` for others)
into ground (2), low, medium and high vegetation (3, 4, 5) by height above
the ground, and noise (7, 18). Options: `--cell` (1 m), `--max-window`
(20 m: objects up to this size are removed from the ground), `--slope`
(0.3), `--ground-tolerance` (0.3 m), `--max-points` (200 million). The input
is LAS/LAZ (a file or a folder) or a cloud of a project (`FILE.ols#name`);
the output a `.las`/`.laz` file or a new cloud of a project (`--replace` to
replace one of that name). Points are held in memory: classify a corridor
(`--line`) on large surveys. About 3 million points a second. Buildings,
poles and wires get vegetation classes: check the result.

## Vegetation

```bash
ols lidar vegetation line.ols.json --pointcloud project.ols#scan \
    --weather cases.olswc --case "max temperature" --case "high wind" \
    --clearance 5 --growth 0.5 --years 3 --fall-in --csv vegetation.csv
```

For each span of each case: points of the vegetation classes (`--role`,
default `vegetation`) closer than the clearance plus the growth until the
next cut, and with `--fall-in` trees that could reach a wire when falling
(heights above the ground from the cloud's ground classes; `--max-tree-height`,
default 40 m). The table lists the spans with findings: counts, the nearest
point, the worst fall-in margin. The CSV holds, per span, the nearest point,
the worst falling tree and the worst points by margin. `--lidar DIR` reads
LAS/LAZ tiles instead of a project's cloud; `--lidar-classes` gives the
class scheme.

Desktop app: **Checks › Vegetation…** — clearance, growth per year, years,
trees falling in, the weather case; the ground is the surface of the
checks.

## As-built wires

```bash
ols lidar fit-wires line.ols.json --pointcloud project.ols#scan --csv as-built.csv
```

For each span: a catenary fitted to the points of the wire classes
(`--role`, default `conductor` and `shield_wire`) near the span's vertical
plane (`--lateral`, 1 m) and its modelled wire (`--vertical`, 3 m) — the
model solved in still air at `--temperature` (15 °C) — then the fitted sag
against the model's, the height difference at mid-span and at the low
point, the fit's RMS, and the **conductor temperature at the survey**: the
temperature at which the model's span sags as surveyed
(`--min-temperature`, `--max-temperature`; `--no-temperature` to skip).
Sections with creep or load conditions need their weather cases:
`--weather`.

Desktop app: **Checks › As-built wires…**.
