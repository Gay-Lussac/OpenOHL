# Galloping, blowout, vegetation and as-built wires

Analyses of a solved line beyond the clearance rules: galloping ellipses,
blown-out wires against the right of way, LiDAR classification, vegetation
near the wires and catenaries fitted to the surveyed wires. Each is a
command of `ohl` and (but for classification and the right of way) a window
of the desktop app's **Checks** menu. The methods are in the theory manual:
[Galloping ellipses and blowout](../theory/galloping.md),
[LiDAR analyses](../theory/lidar-analysis.md).

The commands share the options of `ohl line` for the model: the line or
project file, `--library`, `--structures`, `--insulators`; and for weather
cases `--weather FILE.ohlwc` with `--case NAME` / `--tag TAG` (repeatable;
all cases without them).

**Sections that do not solve.** A section or jumper that does not solve in
a weather or load case is left out of what is computed from it there, and
said so: the clearance checks list it as not solved; the structure loads
and usage of every structure it ends at or hangs on are marked
**incomplete** in that case — "not checked", never a pass (see
[Incomplete structures](loads.md#structure-usage-capacity-checks)), also
in the values handed to the structure application. In the desktop app the
section is drawn in that case where it last was — its last solved shape in
that case, else its shape in another weather case, else the straight
chords between its wire points — as a **dashed red** line; hovering it in
the plan says why, and a click on any of its copies selects the span, whose
properties say why it did not solve and where the drawn shape comes from.

## Galloping ellipses (`.ohlg`)

```bash
ohl clearance galloping synthetic-line.ohl.json \
    --weather weather-cases.ohlwc --params galloping-example.ohlg --csv galloping.csv
```

Every pair of wires between the same two structures, by every method of the
parameter file, in each weather case the file applies to: the smallest
distance between their ellipses along the span, where, and the ellipses'
sizes; worst first. The parameter file holds **all** coefficients — copy
them from the method you design to; the example's are invented:

```toml
format = "ohl-galloping"
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
file (the project's `.ohlg`, or one next to a line file, is chosen first) and the weather case,
then **Check**.

## Blowout and the right of way

```bash
ohl clearance row synthetic-line.ohl.json --weather weather-cases.ohlwc \
    --case "high wind" --half-width 10 --csv row.csv
```

How far each wire swings to the right and to the left of its route's
centreline (alignment, or the line through the structures) in each case,
bundles included, and how far it goes beyond the right of way (`--left`,
`--right` for different widths each side). Clearances of the blown-out wires
to buildings, trees and other wires are the radial clearances of a rule file
run in the windy case (`ohl line --rules`, the Clearances tab).

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

## Distance between conductors in the span (check plugins)

Some codes require a distance between two conductors of a span that
depends on where along it one looks — on the local sag, the suspension
strings, the angle between the conductors, the voltages (e.g. the Swiss
ordinance's still-air distance, LeV Annex 6, 2.2). OpenOHL **measures**
the conductors; the values and the rules are a **check plugin**'s (it
must be in use: Plugins › Plugin manager). A plugin for the Swiss LeV
exists separately; it is not part of the public code. What is measured
and how: [the core's part](../development/phase-distance-check.md).

In the desktop app, with such a plugin in use (the plugin defines the
menu: without it, it is not there), its entry in the Checks menu (e.g.
**Checks › LeV/OLEI › Distance between conductors…**) opens the check's
window: a values file (else the project's copy, else the plugin's), the
plugin's choices, the weather cases (each checked on its own), the
circuits (ticked in a list, select all / none), the station spacing
(1 m), and nearby spans (conductors not between the same two
structures, within a distance). The page opens floating; drag its tab
onto a panel (next to Properties, for example) to dock it. When the
plugin needs a value per circuit (a plugin column, e.g. an insulation
level), **Circuits checked** lists the ticked circuits with their
voltage and that value, chosen there for all the circuit's sections —
the same value as the Electrical circuits table's column. **Check** runs
in the background; the pairs come worst first: the two spans and
circuits, kV, d, the required distance, the margin, the length of
conductor below it, and at the worst station the measured values, the
plugin's own and the rule it applied, with a note when it is at a clamp
or on a nearby span. Clicking a span, or a distance, selects the span,
shows the result's weather case and draws the worst station's line in
the 3D view, labelled with d and the required distance; the line goes
when the window is closed (or a new run starts). **Export CSV…** writes
every pair with the points. Below the settings, the window explains the
computation (the plugin's "How it is computed") and shows the **values
used** as tables, for transparency, and where they come from (the
plugin's, a file chosen, a saved report, or the project's copy). They
are locked; an **admin** of the project may **Edit values** (a cell at a
time, checked by the plugin) and **Save in the project**: the project's
copy of the file (headed with who edited it and when), used by every
run on the project and kept in each report; **Back to the plugin's**
removes the copy. **Choose a values file…** uses another one instead.

**Save** keeps the run in the [library of performed
checks](#the-library-of-performed-checks) under a name: its parameters
(the values file's text included), the pairs below the required
distance and within 1 m of it, the counts. Opened again it shows its
results and whether the line changed since (*stale*); **Check** runs it
again with its parameters, shows each pair against the saved run (new,
fixed, worse, better) and asks whether to replace the saved report.

## The library of performed checks

**Checks › Performed checks…** (a page like the check's, floating
first, dockable) lists the checks saved on the project line you work on: name, check, when, by whom, **status** (draft,
checked, approved — chosen in the list), the **line** (unchanged, or
what changed since: e.g. "1 structure changed"), the result and a
**note** (double-click to write it; double-click the name to rename).
Filter by text, check and status. **Open** shows the report in its
check's window; **Export…** writes it to a file (`.ohlcr`), **Import…**
reads one onto this line (a report of the same name is replaced after
you confirm); **Delete…** asks first.

Reports are kept with the project line (its branch in
[Versions](versions.md)): a line made from another takes its reports,
commits, merges and bundles carry them, and each change to a report is
an edit like any other (undo brings it back). Saving a report needs a
project and a designer's rights.

## LiDAR classification

```bash
ohl lidar classify tiles/ --out project.ohl#classified --line line.ohl.json --corridor 100
ohl lidar classify scan.laz --out scan-classified.laz
```

Classifies unclassified points (classes 0 and 1; `--reclassify` for others)
into ground (2), low, medium and high vegetation (3, 4, 5) by height above
the ground, and noise (7, 18). Options: `--cell` (1 m), `--max-window`
(20 m: objects up to this size are removed from the ground), `--slope`
(0.3), `--ground-tolerance` (0.3 m), `--max-points` (200 million). The input
is LAS/LAZ (a file or a folder) or a cloud of a project (`FILE.ohl#name`);
the output a `.las`/`.laz` file or a new cloud of a project (`--replace` to
replace one of that name). Points are held in memory: classify a corridor
(`--line`) on large surveys. About 3 million points a second. Buildings,
poles and wires get vegetation classes: check the result.

## Vegetation

```bash
ohl lidar vegetation line.ohl.json --pointcloud project.ohl#scan \
    --weather cases.ohlwc --case "max temperature" --case "high wind" \
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
ohl lidar fit-wires line.ohl.json --pointcloud project.ohl#scan --csv as-built.csv
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
