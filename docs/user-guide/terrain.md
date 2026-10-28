# Terrain and ground clearance

OpenOHL builds a terrain surface from survey points and checks how far every
solved wire is above it.

```bash
cargo run -p ohl-cli -- line examples/synthetic-line/synthetic-line.ohl.json --temperature 80 \
  --terrain examples/synthetic-line/synthetic-survey.csv --clearance 7 --html line.html
```

```text
terrain: 2960 points, 5694 triangles — loaded and 10 spans checked in 7.7ms
  lowest vertical ground clearance 4.64 m: section S2 span 1 at (464.0, 6.7)
  spans below the required 7.00 m: 1
    section S2         span   1:   4.64 m at (464.0, 6.7, wire 512.92 m)
```

| Option | Meaning |
|---|---|
| `--terrain FILE` | Survey points (below). Enables the clearance check; the viewer draws the ground. |
| `--clearance M` | Required vertical ground clearance, m: spans below it are listed and marked in the viewer. |
| `--rules FILE` | Clearance rules (`.ohlr`, below): vertical and radial clearances to the ground, LiDAR classes, other lines and other wires of this line. `--clearance M` adds a vertical ground rule. |
| `--other-line FILE` | Another line (`.ohl.json`) for wire-to-wire clearances; repeatable. Solved in the same weather case with the same libraries. |
| `--clearance-csv FILE` | Every clearance found: `rule,measure,section,span,other_section,other_span,clearance_m,required_m,violations,wire_x,…,target_x,…,class`. |

Run it once per weather case that governs clearance (e.g. maximum operating
temperature, ice), with `--temperature`, `--ice-thickness` and
`--wind-pressure`. Blown-out wires are checked over the ground they are blown
over.

## Plan and profile drawings

`--drawings DIR` writes, for every route, a plan and profile sheet `ROUTE.svg` and the profiles as `ROUTE-profile.csv`:

```bash
ohl line project.ohl --pointcloud project.ohl --temperature 80 --rules rules.ohlr \
  --clearance 7 --drawings sheets --side-offset -15 --side-offset 15
```

- **Profile**: ground along the route centreline (exact TIN profile), side ground profiles at `--side-offset` (right positive), the required-clearance line (ground + `--clearance`), structures with their stations, the route's wires and jumpers coloured by phase, and violation markers.
- **Plan**: straightened along the route (station × offset at true scale), aligned with the profile.
- Scales `--hscale 2000 --vscale 200` (1:2000 / 1:200 by default); the plan across the line `--plan-scale 1000` (default: the horizontal scale — along the line the plan always keeps the horizontal scale to stay aligned with the profile); plan half width `--plan-width 40` (also `--line-width`): the **line width**, what is drawn along the line — a structure beside the line (a connection away from the alignment) only within it, wires cut at it (see [Drawings](drawings.md#line-width)); in the desktop app the same setting as the Profile page's line width. It is not the LiDAR `--corridor` (the points read for the ground and the checks).
- **Sheets**: `--page A1` (A0–A4 landscape, or `WIDTHxHEIGHT` in mm) splits each route **at structures** — as many whole spans as fit per sheet, the place ending a sheet starting the next (match line): a structure on the line (never one beside it, such as a connection, however near) or an alignment point without a structure, at least the break clearance on paper (`--break-clearance`, default 2 mm, 0 to 20) from what is drawn of every other structure (see [Drawings](drawings.md#sheets)) — into `ROUTE-sheet-NN.svg`, each with its own height range and a title block (line, route, sheet n/N, station range, structures, scales, format, weather case). A span longer than the sheet or a profile taller than it is drawn anyway and reported as a warning: change the scales or the format.
- **Logos**: `--logo company.svg:30` (SVG, PNG or JPEG, width in mm; repeatable) in the title block.

The sheet layout (title block, margins) is built in for now; company templates will come as a layout plugin. For DXF, PDF, PNG and XML, station ranges and the profile alone, see [Drawings and profile exports](drawings.md) (`ohl drawing`).

## Clearance rules (`.ohlr`)

Every rule names a **target** and the required **vertical** and/or **radial** clearance:

- **vertical**: to the ground, the wire's height above it directly below (negative: the wire is below the ground); to points, the vertical separation from points within `vertical_band` (default 0.5 m) of the wire in plan, above or below it; to other wires, the vertical separation where the wires cross in plan.
- **radial**: the shortest 3D distance between the wire and the target.
- **To the surface**: every clearance is measured to the conductor's surface — the distance from its axis less its radius (half its diameter, iced in a weather case with ice) and less the bound of its polyline (see [Clearance](#clearance)); between two wires, both wires' radii and bounds.
- **Bundles** (sections with `bundle_spacing`): to the nearest subconductor's surface — the lowest one for vertical clearances below the wire, the highest above it, each subconductor for radial clearances.

```toml
format = "ohl-clearance-rules"
version = 1
name = "Example rules"

[[rule]]
name = "Ground"
target = "ground"
vertical = "7 m"
radial = "6.5 m"

[[rule]]
name = "Vegetation"
target = "points"            # LiDAR points, by role and/or class number
roles = ["vegetation"]
classes = [99]
vertical = "5 m"
radial = "4 m"

[[rule]]
name = "Phase to phase"
target = "same_line"         # other wires of this line: same span and other spans
radial = "3 m"

[[rule]]
name = "Crossed lines"
target = "other_lines"       # wires of the lines given with --other-line
vertical = "4.5 m"
radial = "4.5 m"
```

| Field | Meaning |
|---|---|
| `target` | `ground`, `points`, `same_line` or `other_lines`. |
| `roles`, `classes` | Points rules: classes by role (resolved with `--lidar-classes`) and/or by number. |
| `surface` | Ground rules: the terrain surface (TIN layer) of the project checked, by name, e.g. `"TIN 2024"`; default: the surface the checks use (the project's surface of the checks, or `--terrain`). A TIN of a few triangles per square metre is checked faster than the LiDAR points it was made from. Read from the project file the line comes from (`ohl line FILE.ohl --rules …`, `ohl zones`, `ohl thermal clearance-limit FILE.ohl`); a name the project does not hold is noted and its rule not checked. The thermal limit (`ohl thermal clearance-limit`, Checks › Thermal rating… › Run) refuses to run instead, as a limit found without that rule could be too high. |
| `vertical`, `radial` | Required clearances (at least one): a distance, or depending on the voltage (below). |
| `pair_voltage` | Wire rules: the voltage of a pair of wires is the `higher` of the two systems' nominal voltages (default), their `sum`, or the `difference` — the actual voltage between the two conductors from phase angles and DC polarities. |
| `vertical_band` | Points count for vertical clearance within this horizontal distance of the wire (default 0.5 m); points farther away in plan are left out entirely (no slanting or 3D distance). For the ground: the highest ground within this distance (default 0: straight below the wire). The distance is the wire's height over the point at the point's place along the wire, and it is drawn vertical, from the point straight up to that height (2026-10-22). |
| `screen` | `true`: a **screen** (2026-10-27) — a horizontal distance of a code the rule has no measure for (e.g. a lamp post's 5 m + 0.01 m/kV in plan): the `vertical` requirement is then the half width b of a band in plan (fixed or by the wire's voltage), and every point of the rule's classes within b of the wire in plan is reported, at any height. The results say it plainly — "6.10 m in plan, within b = 8.80 m" — and give the distance in plan and b (the windows, the Clearances window, the plan's labels, `ohl zones` and its CSV), not a vertical clearance. Points targets only. One rule for every voltage (its b by voltage). |
| `vertical_depth` | Vertical clearances to points and the ground: the depth along the line of the area under the wire (`vertical_band` its half width across), e.g. `"0.2 m"`. The clearance is the shortest vertical distance between the wire and a point of the area: a point is compared with the wire's lowest point within that depth around it (its highest for a point above the wire). Default 0: the wire straight above the point. |
| `voltages` | Optional: only the wires of these nominal voltages are checked, e.g. `[220]` (kV; `0` for earthed wires); default: all. Wire rules: the first wire of a pair. A distance by voltage (a table or base + per kV) needs none: one rule covers every voltage (2026-10-21; [A rule's voltages](desktop-app.md#a-rules-voltages)). |
| `wires` | Which wires are checked: `"sections"` (the spans), `"jumpers"`, or `"both"` (default). |
| `search_margin` | Targets are reported within the required distance plus this (default 5 m); crossings of other wires are always reported. |
| `include_same_phase` | `same_line`: also check wires of the same circuit and phase (default: no). |
| `circuits` | `same_line`: `"different"` checks only pairs of wires of different circuits (circuit to circuit, e.g. the two circuits of a double-circuit structure; a phase and an earth wire), `"same"` only pairs of the same circuit (phase to phase); default `"any"`. Combine with `case_pairs` for one circuit iced and the other not. |
| `cases` | Weather cases the rule applies to, by name (`*` `?` wildcards); default: all cases run. |
| `exclude_near_ends` | Exclusion zone: the wire within this horizontal distance of its ends (clamps) is not checked, e.g. `"10 m"` — where wires converge at supports by design. |
| `exclude_spans_at` | Exclusion zone: spans ending at these structures are not checked, e.g. `["con_*"]` for down-leads into cable terminations (wildcards). |
| `case_pairs` | Wire rules: `[["glaze ice", "max temperature"]]` — the first wire in the first case, the second wire in the second (e.g. an iced upper wire sagging towards a hot lower one); checked after all cases are solved. |

A rule file may hold a `[wind]` table. The desktop [vertical and radial clearance windows](desktop-app.md#vertical-and-radial-clearances) and [`ohl zones`](#vertical-and-radial-clearances-without-the-window-ohl-zones) read it (the windows write it); `ohl line` ignores it and checks each weather case as it is:

```toml
[wind]
steps = true        # every pressure from 0 up to the case's, in steps
step = "50 Pa"
sides = "both"      # both, left or right
arc = true          # each span also on its swing arc (2026-10-20; default false)
band_margin = "0.15 m"  # the swing band's margin (2026-10-21; default 0.15 m)
jumper_band_margin = "0.3 m"  # the jumpers' bands' own margin (2026-10-21; default 0 m)
```

`arc = true` adds each span's swing arc between still air and the full
blowouts of the sides chosen (three solves per windy case; since
2026-10-21 a band of every radius from the smallest to the largest of the
three positions, widened by the band margin; see [Arc or
steps](../theory/clearances.md#arc-or-steps-speed-and-quality)); alone or
with the steps; since 2026-10-21 the jumpers a rule checks have bands of
their own. A file without it reads as before, and is written without it.
`band_margin` (2026-10-21, a length ≥ 0): every distance to a swing band
is taken this much smaller; default 0.15 m (conservative: the measured
worst excess plus an allowance); a smaller value is on your
responsibility. Written only when not the default (older programs refuse
a file with it: an unknown key); a design code's preset may set it.
`jumper_band_margin` (2026-10-21, the owner's decision; a length ≥ 0):
the jumpers' bands' own margin, default 0 m — no widening: a jumper's
band then does not hold it between its blowouts (a simplification on
your responsibility; exact: the steps), so enter a value for your
jumpers. Written only when not 0.

and a `[check]` table — what those windows and `ohl zones` solve and
read; `ohl line` ignores it too:

```toml
[check]
circuits = ["North", "South"]   # circuit names; none: every circuit
method = "fe"                   # every section by finite elements ("rs": ruling span); none: each section's own
search_distance = "30 m"        # LiDAR points farther from the wires are ignored
routes = ["north"]              # route ids; with alignments none: the whole line (2026-10-27)
alignments = ["A 2"]            # alignment ids, besides whole routes
```

`routes` and `alignments` (2026-10-27; built, in active development, not
approved yet): only the sections and jumpers with a structure on a route
or an alignment named are solved and checked (a section reaching into
another route is taken whole). The desktop windows and the Clearances
tab keep their choice of circuits, routes and alignments here.

Design-code plugins may give such rule files as **clearance presets**
for those windows ([Design codes](design-codes.md#clearance-presets)),
and **rule presets** for any rule set — ground, objects, crossings, wire
to wire ([Design codes](design-codes.md#rule-presets)). In the desktop
application the rule tables of the Clearances window and the clearance windows
set every distance in the three forms below
([Distances by voltage](desktop-app.md#distances-by-voltage)).

### Clearances by voltage

Each section belongs to an electrical system ([systems](line-file.md#electrical-systems): AC or DC, voltage, phase angles) through its conductor path, independent of the wire type; its spans and jumpers are checked with the required clearance for the system's nominal voltage. `vertical` and `radial` can be a distance, a table by voltage, or a formula:

```toml
[[rule]]
name = "Buildings"
target = "points"
roles = ["building"]
# Rows [up to kV, distance]: the first row whose voltage is not exceeded.
vertical = { by_voltage = [[0, "5 m"], [150, "6 m"], [245, "6.5 m"], [420, "7.5 m"]] }
# base + per_kv · (U − above_kv) above above_kv, else base.
radial = { base = "5 m", per_kv = "0.01 m", above_kv = 50 }
```

With one rule per group of LiDAR classes (vegetation, buildings, roads, railways…), each with its own voltage table, the required clearance follows both the class and the voltage. Earthed sections (no system: earth wires, OPGW) are checked as 0 kV — give a `0` row for them. Between wires, `pair_voltage = "difference"` uses the voltage actually between the two conductors: two phases of a 380 kV system meet the 380 kV row, the same phase of two parallel systems the 0 kV row, a phase and an earth wire 219 kV. A voltage above a table's last row stops the run with an error naming the rule, before any check (`ohl line`, `ohl zones` and the windows alike).

How a required clearance is evaluated for a nominal voltage U (kV; earthed wires U = 0; wire rules: the pair's voltage):

- a distance d: d at every voltage;
- a table of rows (U₁, d₁), (U₂, d₂), … with U₁ < U₂ < …: the d of the first row with U ≤ Uᵢ (a row "up to" its voltage, that voltage included); no row when U is above the last one — an error, not the last row's distance;
- a formula: base + per_kv · max(0, U − above_kv). Example: base 7 m, per_kv 0.01 m, above_kv 0 gives 7 + 0.01 · 220 = 9.2 m at 220 kV, 10.8 m at 380 kV, 7 m for earth wires. `per_kv` is written as a length (`"0.01 m"`, meaning metres per kV).

Nothing is interpolated between the rows of a table: a voltage between two rows takes the upper row's distance (the larger one when distances grow with the voltage). A required clearance that cannot be written in these forms (a larger of two values, a term depending on something other than the voltage) needs two rules, each checked on its own. The report prints the voltages found; the CSV's `required_m` and the envelope rows give the required clearance of each wire or pair.

### Envelopes over weather cases

With several weather cases (`--weather`), each case is reported in turn, then the **envelope**: for every rule and every wire (or wire pair), the lowest vertical and radial clearance over all cases with the **case that governs** it, and the cases in which a required clearance is not met:

```text
envelope over 3 weather cases (glaze ice, max temperature, rime ice + wind) and 1 case pairs (glaze ice × max temperature):
  rule "Ground": lowest vertical 4.64 m (S2 span 1, max temperature); lowest radial 4.63 m (S2 span 1, max temperature)
    below the required clearance in some case: 1
      S2 span 1: vertical 4.64 m (max temperature), radial 4.63 m (max temperature) — in: glaze ice, max temperature
```

The clearance CSV holds each case's rows and then the envelope rows (`case = envelope`, last column `governing_case`). The plan and profile sheets draw every case's wires — the first solid, the others dashed, dotted… (named in the title block) — the envelope of the wire positions.

Wires sharing an end point (adjacent spans, a jumper and the sections it joins) are connected and never checked against each other. Point rules are evaluated during the same LiDAR pass that collects the ground. [`examples/synthetic-line/clearance-rules.ohlr`](https://github.com/Gay-Lussac/OpenOHL/blob/main/examples/synthetic-line/clearance-rules.ohlr) and the crossing line `crossing-line.ohl.json` next to it show all wire rules on the synthetic model. Rule values come from the design code or the utility; the desktop clearance windows edit rules per LiDAR class, and design-code plugins may give rule files as clearance presets.

### Wire tension rules (`[[tension_rule]]`)

Since 2026-10-22 a rule file may hold **tension rules** for the wire
tension check (Checks › [Wire tensions…](desktop-app.md#wire-tensions)
and `ohl tensions`): optional `[[tension_rule]]` tables, beside or
instead of the clearance rules. A file without them reads as before;
`ohl line` and the clearance windows ignore them. A preset declares the
check with `checks = ["tension"]` (or holds tension rules); string
rules: [below](#string-rules-string_rule).

```toml
[[tension_rule]]
name = "Iced (invented 0.45 RTS)"
cases = ["glaze ice", "rime ice + wind"]   # optional; none: every case
factor = 0.45                              # allowed = factor × RTS (> 0)
measure = "max"                            # "max" (default) or "horizontal"
targets = { wire_types = ["acsr-*"], systems = ["S1", "none"], circuits = ["North"] }  # optional; none: every wire
```

| Key | Meaning |
|---|---|
| `name` | for the reports |
| `cases` | weather case names (`*` `?` wildcards); none: every case |
| `factor` | the allowed tension as a factor of the wire type's rated tensile strength; above 0. **No factor is built in**: the example's are invented |
| `measure` | `max`: the tension at the span's most tensioned support (default); `horizontal`: the horizontal tension |
| `targets` | `wire_types` (ids, wildcards), `systems` (ids; `none`: the sections without a system), `circuits` (names); a wire counts when it is in every list given; none: every wire |

**Without the window:**

```sh
ohl tensions examples/synthetic-line/synthetic-line.ohl.json \
  --weather examples/synthetic-line/weather-cases.ohlwc \
  --rules examples/synthetic-line/tension-rules.ohlr --csv tensions.csv
```

prints every span of every wire in each weather case of a rule — rule,
case, section, span, tension, allowed, usage %, pass or FAIL — the
largest usage first, and writes them as CSV with `--csv` (`rule, case,
section, span, wire_type, measure, tension_n, rts_n, factor, allowed_n,
usage_pct, status, x, y, z`, the point the most tensioned support). Exit
status 0: every span within its allowed tension, 1: some above, 2: an
error (a rule file without tension rules too). The example's rule file
(`examples/synthetic-line/tension-rules.ohlr`) uses **invented** factors.

### String rules (`[[string_rule]]`)

Since 2026-10-24 a rule file may hold **string rules** for the string
check (Checks › [String checks…](desktop-app.md#string-checks) and `ohl
strings`): optional `[[string_rule]]` tables. A file without them reads
as before; the other checks ignore them. A preset declares the check with
`checks = ["strings"]` (or holds string rules) — a design-code plugin's
template for the values to respect.

```toml
[[string_rule]]
name = "Suspension swing (invented 55°)"
cases = ["high wind"]          # optional; none: every case
strings = "suspension"         # "all" (default), "suspension" or "strain"
voltages = [220.0]             # optional; none: every voltage (0: earth wires)
load = true                    # the strings' own maximum load (default true)
swing = true                   # the strings' own swing limits (default true)
uplift = true                  # suspension strings going slack (default true)
max_swing_across_deg = 55.0    # optional: the rule's own swing limits, degrees
# max_swing_deg, max_swing_along_deg likewise
```

| Key | Meaning |
|---|---|
| `name` | for the reports |
| `cases` | weather case names (`*` `?` wildcards); none: every case |
| `strings` | which strings: `suspension` (not dead ends), `strain` (dead ends, as the solve treats them: flagged `strain`, or the string holding the clamp at a section's end), `all` |
| `voltages` | the nominal voltages of their sections, kV; none: every voltage |
| `load` | the string's axial force against **its own** `max_load` (the string's, else its library entry's) — a plain comparison; a string without one: not checked, counted in a note |
| `swing` | its swing as solved against **its own** swing limits (`max_swing`, `max_swing_across`, `max_swing_along`), where it has them |
| `max_swing_deg`, `max_swing_across_deg`, `max_swing_along_deg` | the rule's own limits (above 0°, at most 180°): from straight down, across and along the line (either side), checked for every string the rule takes; each limit given (the string's and the rule's) is its own row |
| `uplift` | a suspension string that **goes slack** (pushed while it takes tension only) or, one that may push (`compression` given), pushed beyond that limit — not the geometry: some strings hang upside down by design |

No limit is built in: the strings' are yours, the rules' a design
code's or yours (the examples' are invented).

**Without the window:**

```sh
ohl strings examples/synthetic-line/synthetic-line.ohl.json \
  --weather examples/synthetic-line/weather-cases.ohlwc \
  --rules my-string-rules.ohlr --csv strings.csv
```

prints every limit of every string in each weather case of a rule —
rule, case, string, check, value, limit, usage %, pass or FAIL —
failures first, then the largest usage, and writes them as CSV with
`--csv` (`rule, case, structure, string, kind, section, point, measure,
limit_from, value, limit, unit, usage_pct, status, x, y, z`, the point
the string's wire point). Exit status 0: every string within its
limits, 1: some beyond, 2: an error (a rule file without string rules
too).

### Internal clearance rules (`[[internal_rule]]`)

Since 2026-10-25 a rule file may hold **internal clearance rules**: the
electrified parts at each structure against the structure's own body
(`ohl internal`; the desktop window is in active development). Optional
`[[internal_rule]]` tables; a file without them reads as before, the
other checks ignore them. A preset declares the check with `checks =
["internal"]` (or holds internal rules).

```toml
[[internal_rule]]
name = "Body (invented 1.5 m)"
cases = ["high wind"]          # optional; none: every case
voltages = [220.0]             # optional; none: every electrified part
distance = "1.5 m"             # or { by_voltage = [[up to kV, "m"], …] } or { base, per_kv, above_kv }
conductors = true              # the conductors near the structure (default true)
jumpers = true                 # the jumpers at it (default true)
string_ends = true             # the strings' wire points (default true)
shapes = true                  # the volumes on the strings (default true)
reach = "20 m"                 # how far along each span from its clamp (default 20 m)
distance_wind = "1.2 m"        # optional: in the cases with wind (invented)
```

| Key | Meaning |
|---|---|
| `name` | for the reports |
| `cases` | weather case names (`*` `?` wildcards); none: every case |
| `voltages` | the nominal voltages of the parts' sections, kV; none: every electrified part (earth wires are never checked) |
| `distance` | the distance to keep, required: a distance, a table by voltage or a formula, as the clearance rules' `vertical` / `radial`; a part whose voltage the table does not reach is not checked (a note says so) |
| `conductors` | each span's conductor from its clamp at the structure out to `reach` (in plan along the solved wire), as solved in the case (the strings' swing included); a bundle subconductor by subconductor |
| `jumpers` | each jumper with an end on the structure, as solved |
| `string_ends` | each string's wire point (its bottom) as solved, not in still air — only on a string with an electrified volume (2026-10-27): without one the wire point is the conductor's clamp, already measured with the conductor (to its surface), so only the conductor's distance to the structure is given |
| `shapes` | the volumes on the strings (2026-10-25): each string's electrified shapes as parts, and every part measured against the grounded shapes too; `false`: neither (the first part's check) |
| `reach` | above 0 m; default 20 m |
| `distance_wind` | optional (2026-10-25, the owner's rule): the distance to keep in the cases **with wind** (a wind pressure above 0), in the forms of `distance`; none: `distance` in every case |

What is measured: only **structures at detail level 2 or 3** (a body:
members, a pole); a level-1 structure is listed as *not checked*. Each
part against **every member** of the body, to the member's **surface**
where its section's profile is known (the member as drawn), else to its
**centre line** — the member's size then left out, **on the unsafe
side**: a note names each structure with such members and every row
says which. The distances are exact on the solved polylines, to the
wire's surface: its radius in the case (iced with ice) and the bound of
how far the wire lies off its polyline are taken off (conservative). A
string's wire point counts as a point (its fittings' size is not
counted: on the unsafe side, unless they are given as volumes).

**Volumes on the strings** (2026-10-25, the next part): the shapes of
each string's insulator assembly ([Insulator assemblies](structure-library.md#volumes-on-the-strings)),
its own overriding them by name, placed on the string as solved by the
assembly's freedoms and strings. Each **electrified** shape is a part
(`string S1 clamp`; several strings: `string S1 horn (string 2)`), and
every part is measured against the body and the **grounded** shapes of
every string at the structure (`shape string S1 top fitting` in the
member column); insulated shapes are not clearance targets. The shapes'
distances are exact on the convex solids (a lower bound within 1 nm:
conservative); how they are placed and what is approximate is in the
theory manual, [Internal clearances](../theory/internal-clearances.md).
The insulator library is `--insulators` (default: the project's, else
the shipped one); a line without any string carrying volumes says so.

```sh
ohl internal examples/synthetic-line/synthetic-line-typed.ohl.json \
  --structures examples/synthetic-line/Structures \
  --weather examples/synthetic-line/weather-cases.ohlwc \
  --rules my-internal-rules.ohlr --csv internal.csv
```

prints the structures not checked, the notes, and one row per rule,
structure, case and part — its smallest distance, the required one, the
nearest member, whether to its surface or its centre line, pass or
FAIL — the smallest margin first, and writes them as CSV with `--csv`
(the member column names a grounded shape as `shape string S1 …`;
`rule, case, structure, part, member, distance_m, required_m,
margin_m, to, status, x, y, z, member_x, member_y, member_z`: the point
of the part and of the member nearest each other). `--sections` gives
the cross-section library (the members' profiles); `--time-limit`
(seconds, default 600) stops a run that takes too long. Exit status 0:
every part keeps its distance, 1: some too near, 2: an error (a rule
file without internal rules too). No distance is built in.

## Vertical and radial clearances without the window (`ohl zones`)

`ohl zones` runs the check of the desktop
[vertical and radial clearance windows](desktop-app.md#vertical-and-radial-clearances)
without the program: a rule file gives the rules, the zone, the wind and
what is solved; the project gives the point cloud and the ground.

```bash
ohl zones project.ohl --rules vertical-clearances.ohlr
ohl zones examples/synthetic-line/synthetic-line.ohl.json --rules rules.ohlr \
  --weather examples/synthetic-line/weather-cases.ohlwc \
  --terrain examples/synthetic-line/synthetic-survey.csv --csv zones.csv
```

```text
vertical clearances: Demo (rules.ohlr) — 1 rule row(s); zone ±0 m in slices of 0 m, wind stepped by 100 Pa from the left and the right, method fe
libraries:
  cables: libraries/cables (shipped)
  structures: examples/synthetic-line/Structures (next to the file)
  insulators: libraries/insulators (shipped)
weather: examples/synthetic-line/weather-cases.ohlwc — 5 case(s), 2 wind position(s)
ground: survey file examples/synthetic-line/synthetic-survey.csv
points: not needed
solved 3 section(s) in 2 wind position(s) in 283.8ms; checked in 10.0ms; 0 LiDAR points read
1 span below a rule
  status    rule      span        case & wind       target surface m required m    count
  violation Ground    S2 span 2   max temperature   ground      4.70       7.50 16 pieces
```

The rule file is read as the windows import it: its ground and point
rules with a clearance of one measure — **vertical** or **radial**, the
only one the file has (a file with both needs `--measure vertical` or
`--measure radial`); a required clearance by voltage stays one row, each wire
checked with the distance of its voltage (a rule that lists `voltages`
gives one row with those voltages, 2026-10-21); roles become the classes of the class scheme; the first rule's
`vertical_band` and `vertical_depth` are the zone's width and depth. The
`[wind]` table sets the wind (default: the cases' own pressure from both
sides), the `[check]` table the circuits, the calculation method and the
search distance. A window's own rule file (**Export…**, or **File ›
Export › Libraries…**) and a design code's clearance preset work as they
are.

| Option | Meaning |
|---|---|
| `--rules FILE` | The rule file (`.ohlr`); for a project, also the name of one of its own rule files. |
| `--measure vertical\|radial` | The measure, for a rule file with both. |
| `--weather FILE` | Weather cases (`.ohlwc`; for a project, also the name of one of its own). Default: the file of the same name as the rule file, next to it or in a `weather` folder beside it (a design code's preset), else the project's weather case file used last, else the project's first weather case file. |
| `--case NAME` | Only these weather cases (wildcards; repeatable). Default: every case the rules name. |
| `--cloud SPEC` | Point cloud: `FILE.ohl`, `FILE.ohl#name` or `FILE.copc.laz`. Default: the project's own point cloud, if it holds one. `--no-cloud` reads none. |
| `--terrain FILE`, `--surface NAME` | The ground: survey points, or a surface stored in the project. Default: the project's surface of the checks, else its active surface (as set by an earlier build), else the ground classes of the point cloud along the spans (lowest point per square metre, triangles of up to 50 m) — as the program builds it. |
| `--lidar-classes FILE` | Class scheme (`.ohlc`); default the point cloud's own, else ASPRS. |
| `--library`, `--structures`, `--insulators` | As for `ohl line`: for a project, the libraries it keeps with its current line; else (and for a kind the project holds none of) the shipped wire and insulator libraries, the `Structures` folder next to the file. |
| `--swing-arc`, `--wind-step PA`, `--wind-sides both\|left\|right`, `--band-margin M`, `--jumper-band-margin M` | Override the rule file's `[wind]`: add the swing arc, step the wind by this step, the sides, the swing band's margin (m, 2026-10-21; default the file's, else 0.15 m), the jumpers' bands' own margin (m, 2026-10-21; default the file's, else 0 m). |
| `--search-distance M`, `--circuits NAME`, `--method fe\|rs` | Override the rule file's `[check]` (`--circuits` repeatable; `""`: the sections without a circuit). |
| `--route ID`, `--alignment ID` | Check only the sections and jumpers on these routes or alignments (repeatable, 2026-10-27; override the rule file's `routes` and `alignments`; an unknown id is refused). `ohl line --rules` takes `--clearance-route` and `--clearance-alignment` for the same (`--route` names the route drawn in `ohl drawing`). |
| `--csv FILE` | Every finding: `status` (`violation`, then `ok` for each rule's lowest clearance that keeps it), rule, section, span, case, wind pressure and side (`arc` on the swing arc), target, class, clearance and required distance, count, the wire and target points, and `arc_deg` (the angle on the swing arc, degrees from the vertical, positive to the right; empty off it). |
| `--time-limit S` | Stop after this many seconds: solving every wind position and reading the points. Default (2026-10-27): 900 s and 20 s more per million LiDAR points of the rules' classes in the cloud, as the desktop windows; printed before the run, and said with what makes a run faster when reached. |

The report says where each library came from (`cables: the project's
(2 files)`, `--library`, shipped, next to the file), lists the sections
solved, the wind positions, the LiDAR points read and the time taken —
since 2026-10-27 with where the time went (solving, indexing the wires,
reading the points, checking them, the slowest rules with their reach in
plan, and hints) — then
the findings as the window shows them: the violations, worst first, then
each rule's lowest clearance. A rule of several voltages makes one row
with them (2026-10-21; a note `no wire of 110 or 150 kV` when the line has
none of them); rules of the same name stay together in the summary: one
line `NAME: nothing within reach (220, 380 kV)` per rule name, and one note
`no wire of 150, 125, 110 kV: 12 rule rows not used` for the rows of
voltages the line does not have (a note repeated is printed once with its count). The CSV keeps one
row per finding. The exit
status is **0** with no violation, **1** with violations, **2** on an
error (a rule file with both measures, no weather case, the time limit) —
for scripts and batch runs.

## Survey files

Plain text, one point per line, separated by commas, semicolons, tabs or
spaces. Empty lines and lines starting with `#` are ignored. Coordinates are
metres in the line's coordinate system.

Without a header the columns are `x y z [code] [line]`:

```text
# x, y, z, feature code, breakline id
2600010.00,1200020.00,512.31,GND,
2600030.00,1200020.00,512.80,ROAD,road-left
```

With a header, columns are found by name, so other layouts work too:

| Column | Names |
|---|---|
| x (easting) | `x`, `e`, `east`, `easting` |
| y (northing) | `y`, `n`, `north`, `northing` |
| z (elevation) | `z`, `h`, `elev`, `elevation`, `height` |
| feature code | `code`, `d`, `desc`, `description`, `feature` |
| breakline id | `line`, `breakline`, `string` |

Other columns (e.g. a point number) are ignored, so a PNEZD export only needs
the header `p,n,e,z,d`.

- **Feature codes** are kept with each point (e.g. `GND`, `ROAD`, `TREE`).
  They will select what each clearance rule applies to; v1 uses all points
  as ground.
- **Breaklines**: consecutive points (in file order) with the same non-empty
  breakline id are joined by edges the triangulation must keep — ridges,
  ditches, road edges, embankment toes. Without them a triangle may cut
  across a ditch. A breakline edge that would cross an earlier breakline is
  skipped and counted in the output.
- Points with the same x and y are merged (the first one is kept).

## Ground from LiDAR

```bash
cargo run --release -p ohl-cli -- line LINE.ohl.json --lidar TILES_DIR \
  --lidar-classes SCHEME.ohlc --clearance 8
```

| Option | Meaning |
|---|---|
| `--lidar PATH` | LAS / LAZ file or directory of tiles. (`--pointcloud` instead reads a project's point cloud or a COPC file, only the blocks or octree nodes of the corridor: [Point clouds](project.md#point-clouds).) |
| `--lidar-classes FILE` | Class scheme (below); default: ASPRS LAS 1.4 standard classes. |
| `--ground-role ROLE` | Classes with this role make the terrain surface (default `ground`). |
| `--corridor M` | Only points within M of the spans (default 60 m). Every tile is read (the bounds in tile headers are not trusted); points flagged *withheld* are left out. |
| `--max-edge M` | Longest triangle side (default 50 m): no ground is invented across larger gaps. |
| `--thin M` | Optional: keep the lowest ground point per M × M cell. |

`ohl lidar info PATH --classes [--scheme FILE]` lists the tiles and counts the points of every class, with the scheme's names and roles.

### Class schemes (`.ohlc`)

What a class number means depends on who classified the data — the ASPRS LAS standard, or a utility's or surveyor's own specification. A class scheme names every class and gives it **roles**; tools select classes by role, so any classification works. A class can have several roles, and roles are free words:

```toml
format = "ohl-lidar-classes"
version = 1
id = "my-survey"
name = "Survey company X, project Y"
source = "Survey specification v2"   # optional: where the scheme comes from

[[class]]
code = 1
name = "Terrain"
roles = ["ground"]

[[class]]
code = 5
name = "Asphalt road"
roles = ["ground", "road"]
```

The ASPRS LAS 1.4 scheme is in [`libraries/lidar-classes/asprs-las-1.4.ohlc`](https://github.com/Gay-Lussac/OpenOHL/blob/main/libraries/lidar-classes/asprs-las-1.4.ohlc) (ground: 2, water 9, road surface 11). Used by OpenOHL: `ground` (the terrain surface), `vegetation` (vegetation analysis), `conductor` and `shield_wire` (as-built wire fitting), and any role a clearance rule names; the application also colours points by role. Suggested: `vegetation`, `building`, `water`, `road`, `railway`, `bridge`, `structure`, `insulator`, `conductor`, `shield_wire`, `load_object`, `other_line_structure`, `other_line_conductor`, `object`, `noise`, `temporary`, `unclassified`.

On a real 25 km line (private data, checked locally), ground built this way from 99.6 million LiDAR points (11.3 million in the utility's ground classes, 22.6 million triangles, about one minute) gave span clearances matching those on the TIN of a commercial program to a median of 2 mm (94 % of 930 spans within 10 cm).

A class may have a display **colour**, `color = "#rrggbb"`; without one, its colour comes from its roles (ground brown, vegetation green, buildings red, water blue, wires yellow…). In the application, Line › Terrain › Survey data › **Class colours…** edits them per class and saves the scheme to a file; the scheme used is each user's choice (for a project or every project), so the same point cloud can be read with another scheme (e.g. a utility's own class numbers stored as ASPRS).

## The terrain surface

The points are triangulated (constrained Delaunay, the
[spade](https://crates.io/crates/spade) library) into a TIN. A finished
mesh (an imported TIN, or the output of a terrain-building plugin) can be
used as it is. The ground
height anywhere inside the surveyed area (the convex hull of the points) is
the linear interpolation on the triangle below. Outside it there is no
ground: spans that are not fully over the survey are reported.

## Backgrounds: orthophotos, maps and drawings

**Line › Terrain › Backgrounds** keeps orthophotos, maps and DXF drawings (e.g.
the cadastre) in the project file, drawn under the line. Like point clouds and
surfaces they belong to the project, not to the line: they are not logged
and not undone.

- **Import orthophotos or maps…** — *They are orthophotos* or *a map*
  (2026-10-25): a map is a georeferenced image of a map loaded into the
  project; under the plan view it is drawn like an orthophoto, and the
  **route and situation plans** and the **sheets' key map** take the maps
  first (every layer when the project has none; the sheets' plan takes the
  layers its template or the export names). The menu lists each layer with
  its kind; a click on the kind turns it (kept in the project). North-up images in the project's coordinate
  system: **GeoTIFF** (its tags give where it lies; a GeoTIFF naming
  another EPSG system than the project's is refused), or **TIFF, JPEG,
  PNG with a world file** next to them (`.tfw`, `.jgw`, `.pgw`, `.wld`, or
  the extension with `w`). Add files or a whole folder; they go into a
  **layer** (e.g. `Orthophoto 2024`) shown or hidden as one. Each image is
  cut into JPEG tiles of 256 pixels with a zoom pyramid (each level half
  the last, down to one tile; about a third more): the plan view reads only
  the tiles in sight at the level of the zoom (a coarse level first). Plain
  black or white tiles (no data) are not kept. Importing an image of the
  same name in the same layer replaces it. It runs in the background;
  the project opens again to show them. An image is decoded whole (3 bytes
  per pixel: 10 000 × 10 000 pixels need 300 MB).
- **TIFF variants read**: uncompressed, LZW, deflate, PackBits and
  **JPEG-compressed** TIFFs, in strips or tiled, grey, RGB or RGBA of 8 or
  16 bits — and **JPEG-compressed YCbCr** TIFFs, as GDAL writes them with
  `COMPRESS=JPEG PHOTOMETRIC=YCBCR` and many mapping agencies publish
  their orthophotos (the JPEG tables in the file's `JPEGTables` tag, the
  chroma subsampled): their strips or tiles are decoded and turned into
  RGB as JPEG defines it (built, in active development (not approved
  yet)). A TIFF of any other kind — a palette, CMYK, CIE Lab, floating
  point, more bands — is refused with a message naming it (for example
  *palette, 8 bits, uncompressed, in strips*) and the workaround: convert
  it to a JPEG with a world file (`.jgw`), or to a plain RGB TIFF, e.g.
  `gdal_translate -co COMPRESS=DEFLATE -co PHOTOMETRIC=RGB in.tif out.tif`
  (`-expand rgb` for a palette). The GeoTIFF tags or the world file place
  it as for any other image.
- **ECW** files are converted to GeoTIFF first by an external converter you
  install yourself: GDAL's `gdal_translate` **with the ECW driver** (ECW is
  a proprietary format whose decoder needs a licensed SDK; it is not part
  of OpenOHL, and GDAL's usual builds leave it out). Give it in the window
  (**Find** looks in the usual places: PATH, Homebrew, QGIS, OSGeo4W); it
  is remembered on this computer. Each `.ecw` is converted into a temporary
  GeoTIFF (its own georeferencing, else the `.eww` world file next to the
  ECW), imported, and the temporary file removed. Without such a GDAL,
  convert them once yourself (QGIS with ECW support, or the ECW vendor's free
  tools) to GeoTIFF — some mapping agencies publish their orthophotos as
  GeoTIFF directly.
- **Import DXF drawing…** — an ASCII DXF file (any version; binary DXF is
  refused): lines, polylines (with their arcs), circles, arcs, ellipses,
  points and texts, by layer, in their colours (the DXF colour index or
  true colour; BYLAYER, BYBLOCK), at their own heights; blocks are
  inserted (scaled, turned, nested up to 8 deep) with their attributes.
  Hatches, dimensions, splines, faces and meshes are counted and not
  drawn (the import says which). Layers switched off in the file are not
  drawn. Coordinates are taken as they are (the project's system and
  units).
- **Remove backgrounds…** — an orthophoto layer or a drawing, after a
  second click.

What is shown is yours (remembered on this computer, like the other view
choices): **Orthophotos** and **Drawings** on or off, and each orthophoto
layer and each drawing on its own — the menu stays open while you tick.
Both are shown when a project opens.

- **Orthophotos** are drawn in the **plan view** only.
- **Drawings** are drawn in the **plan view** (texts from 6 pixels high,
  turned as in the file) and in the **3D view** at their own heights —
  nothing is draped on the ground: a 2D drawing (heights 0) lies at height
  0. Texts are not drawn in 3D.

**Reproject project** (Line › Terrain › Coordinate system) moves the drawings
with the project; orthophotos would need resampling, so they are left out
of the new project: import them again in the new system.

## Terrain surfaces in the project

A project keeps any number of terrain surfaces (TINs, *triangulated irregular networks*), by name, compressed in the project file. You choose one as **active**: it is the ground — drawn, and used for clearances, profiles and placing structures. When a project with an active surface opens, the ground is read from it (a 12.7-million-triangle surface: about 1 s) instead of being built from the LiDAR every time. Other surfaces can be **shown** too, each in its colour (the *Other surfaces* layer, drawn in chunks with levels of detail like the ground).

Line › Terrain › **Terrain surfaces (TIN)…** manages them:

- **Import…** a file. Its format comes from the extension, or you choose it:

  | Format | Extension | |
  |---|---|---|
  | LandXML | `.xml` | every TIN surface of the file (`<Surface>` with `<Faces>`; points are *northing easting elevation*) |
  | OBJ | `.obj` | a mesh (`v x y z`, `f a b c …`) |
  | Survey points | `.csv`, `.xyz` | triangulated, breaklines kept; option `max_edge=50` leaves out longer triangles |
  | Terrain plugins | as they say | e.g. `plugins/examples/esri-grid` (ESRI ASCII grids, `.asc`) |

  The first surface kept becomes the ground when there is none.
- **Build from LiDAR:** tick the classes the ground is built from in the list of every LiDAR class of the model (code, name, points; it scrolls). At first the classes with the `ground` role are ticked; **Ground classes**, **All** and **None** set the ticks at once. Without LiDAR points the section is greyed out. Then the lowest point of each cell (default 1 m) of those classes, near the line or the whole cloud, is triangulated with a longest side (default 50 m) and kept under the **surface name** given.
- **Keep the ground shown as a surface:** the ground built at opening (from the LiDAR or a survey file), kept so the next opening reads it.
- **Active** (radio) and **shown** (tick, colour) are your own choice: remembered on this computer for this project, never written to the project nor sent to a server — everyone working on a hosted project decides what they show.
- **Checks** (radio): the surface the checks (clearances) use, chosen for the project — one for everyone (a project setting; needs edit rights). None chosen: each user's ground. (The clearance checks will be reworked around the LiDAR points.)
- **Rename**, **Export…** (LandXML `.xml` or OBJ `.obj`), **Delete** (confirmed; it cannot be undone — export first to keep a copy).

Reading, building and storing run in the background. With a project joined on a server, surfaces are imported and built on the hosting computer.

From the command line:

```bash
ohl surface list project.ohl
ohl surface import ground.xml --into project.ohl --user ann [--name N] [--format F] [--option max_edge=50] [--plugin DIR] [--replace]
ohl surface export project.ohl "ground" ground.xml
ohl surface rename project.ohl "ground" "survey 2026" --user ann
ohl surface delete project.ohl "ground" --user ann
ohl surface formats [--plugin DIR]
```

Ground profiles follow the TIN exactly: a profile point at every triangle
edge crossed, with straight ground between them — no sampling step to choose
and no ridge missed between samples.

## Clearance

For every span (and every jumper, reported as `jumper:<id>`) the **vertical** distance from the wire's surface down to the ground is
evaluated under every piece of the solved wire, at every TIN edge crossing,
and the smallest value is reported with its location (the point on the
wire's axis it is measured from).

**Every clearance is to the conductor's surface** — to the ground, to
LiDAR points, to other wires, in zones, for vegetation, galloping, the
right of way and the distance between conductors: the distance measured
from the conductor's axis (each subconductor's) less

- the conductor's **radius**: half its diameter, and in a weather case
  with ice half its iced diameter — \\( d/2 + t \\) for a radial thickness
  \\( t \\), \\( \sqrt{d^2 + 4A/\pi}/2 \\) for an ice load (its area
  \\( A \\) from the case's ice density, else glaze ice, see
  [Weather cases](weather.md#how-ice-loads-a-wire));
- the **polyline's bound**: the wire is a polyline of closely spaced points
  on the solved catenary, and between two points the catenary lies off
  their chord by at most the sagitta \\( R - \sqrt{R^2 - L^2/4} \\) of the
  piece's length \\( L \\) on a circle of radius \\( R = H/w \\) — the
  catenary curves at most \\( w/H \\), at its low point (a few
  millimetres: about 1.8 mm for the 4.8 m pieces of a 305 m span of the example line
  at \\( H/w \\) ≈ 1570 m);

and between two wires, both wires' radii and bounds. A clearance reported
is therefore never more than the exact one to the surface.

How exact the clearances are (labels as in the [theory manual](../theory/conventions.md#conservative-assumptions-and-simplifications)):

- **Conservative:** the polyline's bound is taken off every clearance, below and above the wire, although the catenary lies below its chords (a clearance to a target above the wire is low by up to twice the bound, a few millimetres). Exact: the distance to the catenary itself.
- **Conservative:** the radial distance to the ground is taken from points every 0.5 m (at most) along the wire, and the shortest one \\( D \\) is lowered to \\( \sqrt{D^2 - (s/2)^2} \\) for points \\( s \\) apart: no point of the wire between them can be nearer the ground than that (where the wire comes nearest, it runs square to the line to the ground, so the nearest point taken is at most \\( \sqrt{d^2 + (s/2)^2} \\) away). The clearance is low by at most \\( s^2/8d \\) (3 mm at 10 m). Exact: the distance from the continuous wire.
- **Conservative:** in a weather case with ice, the iced radius all along every wire (the case's ice on every span, as for its loads). Exact: the ice where it is.
- **Simplification:** a wire swung by the wind lies off its chords in the swung plane, so its plan position too differs from the polyline's by up to the bound times the sine of the swing; over ground sloping across the line the ground under it is then higher or lower by the slope times that (micrometres to a millimetre, either way). Exact: the distance to the catenary itself.
- Bundles: each subconductor where the bundle's spacing puts it, to its own surface (exact for the bundle's geometry); galloping and the right of way take the bundle's outer envelope ([Galloping](../theory/galloping.md)).

Several weather cases are checked together with [envelopes](#envelopes-over-weather-cases); exclusion zones near the wire ends are rule fields (`exclude_near_ends`, `exclude_spans_at`); the plan and profile sheets draw the required-clearance line and the violations.

## Example data

[`examples/synthetic-line/synthetic-survey.csv`](https://github.com/Gay-Lussac/OpenOHL/blob/main/examples/synthetic-line/synthetic-survey.csv)
is invented ground for the synthetic example line, generated by
[`make_synthetic_survey.py`](https://github.com/Gay-Lussac/OpenOHL/blob/main/examples/synthetic-line/make_synthetic_survey.py):
smooth ground through the structure bases, an 8 m hill under span T2–T3 and a
road embankment surveyed as two breaklines across span T4–T5.
