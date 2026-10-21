# Galloping, blowout, vegetation and as-built wires

Analyses of a solved line beyond the clearance rules: galloping ellipses,
blown-out wires against the right of way, LiDAR classification, vegetation
near the wires, safety volumes around the wires for work near the line
and catenaries fitted to the surveyed wires. Each is a
command of `ohl` and (but for classification and the right of way) a window
of the desktop app's **Checks** menu. The methods are in the theory manual:
[Galloping ellipses and blowout](../theory/galloping.md),
[LiDAR analyses](../theory/lidar-analysis.md),
[Safety volumes](../theory/safety-volumes.md).

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
distance between their ellipses along the span — to the wires' surfaces
(the ellipses are of the wires' axes; each wire's bundle radius and
conductor radius, iced in a case with ice, are taken off) — where, and the
ellipses' sizes; worst first. The parameter file holds **all** coefficients — copy
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
to its outer surface (bundle radius and conductor radius, iced in a case
with ice, included), and how far it goes beyond the right of way (`--left`,
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
the same value as the Conductor paths table's column. **Check** runs
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

## Vegetation and danger trees

```bash
ohl lidar vegetation line.ohl.json --pointcloud project.ohl#scan \
    --weather cases.ohlwc --case "max temperature" --case "high wind" \
    --clearance 5 --growth 0.5 --years 3 --fall-in --csv vegetation.csv
```

All the cases chosen are solved first — a case with wind **from the left
and from the right** — and every vegetation point (classes of `--role`,
default `vegetation`) is checked once against every position of every
span (in parallel on every core):

- **Grow-in:** closer to a position's surface than the clearance plus the
  growth until the next cut plus `--buffer` (default 0; e.g. the survey's
  accuracy).
- **Fall-in** (`--fall-in`): a point of height h above the ground (from
  the cloud's ground classes, `--ground-role`) is a tree top; the tree,
  grown, falls towards each wire point turning about the edge of its
  **root ball** — a hinge `--root-ball` × its height from the trunk
  (default 0.05; 0: about its base) — and rests on the ground along that
  direction (a ridge between the tree and a low wire shelters it). A
  danger tree when the swept trunk comes nearer the wire's surface than
  `--fall-allowance` (default 2 m, e.g. the flashover clearance).
  `--max-tree-height` (40 m) is the tallest tree looked for.
  `--max-base-ground-distance` (default 5 m): a tree's base is taken
  from the ground only when a ground point lies within that distance in
  plan (`inf`: no limit); the points left without a base are counted,
  with the reason.
- **Swing arc** (`--swing-arc`): each span of a windy case also anywhere
  along its swing between the left and the right blowout (the span
  swinging as one about the line through its attachment points; since
  2026-10-21 at every radius from the smallest to the largest of the
  three — a band — widened by the **band margin** on every side, so that
  it holds the positions at every intermediate pressure, also for a tree
  between the axis and the arc).
- **Band margin** (`--band-margin M`, default 0.15 m; since 2026-10-21):
  every distance to the band taken this much smaller. The default is the
  worst excess measured on the example lines (0.089 m) plus an allowance:
  conservative. A smaller value is less safe and on your responsibility.
  The jumpers' own margin (below, in the safety volumes and the
  clearance windows) does not apply here: the danger-tree check leaves
  the jumpers out.
- **Unbalanced ice** (`--unbalanced-ice`): each span with vegetation
  within reach solved with its section on its own, the span in a weather
  case of its own (its ice weight, the wind on its iced diameter) and the
  other spans in the section's: `case` (the case's ice on that span
  only, the other spans in the case without ice; cases with ice),
  `thickness=0.01` (the case with 10 mm more radial ice on the span; in
  metres), `extra=5` (the case with 5 N/m more ice load on each wire of
  the span — per subconductor, as a weather case's ice load; before
  2026-10-20 the whole bundle's) or `load=2000` (2000 N at mid-span, the
  whole bundle's).

**Defaults** (since 2026-10-20, the same in the desktop application):
root ball 0.05, allowance 2 m, a ground point within 5 m of a tree base,
a 50 m search distance (`--corridor`) — the utility practice; a
design-code plugin's preset may give others. Before, the command line's
defaults were 0, 0, no limit and 60 m: **runs that relied on the
defaults now give other results.** With `--root-ball 0
--fall-allowance 0 --max-base-ground-distance inf` and no buffer the
results are those of the check before the danger trees. The table lists the spans
with findings: grow-in and fall-in counts, the nearest point and the worst
fall-in margin, each with its case and position (`case`, `left`,
`right`, `arc`, `ice`, `ice left`, `ice right`). The CSV (`--csv`) holds
per span its counts, its nearest point, its worst falling tree and its
worst points (10), with the case and position of each:
`section,span,circuit,phase,kind,grow_in,fall_in,class,x,y,z,distance_m,grow_case,grow_position,height_m,fall_margin_m,fall_case,fall_position,margin_m`.
(Before 2026-10-20 the CSV had one block per case.) `--lidar DIR` reads
LAS/LAZ tiles instead of a project's cloud; `--lidar-classes` gives the
class scheme. A project file's own libraries are used, as `ohl line`
does. `--time-limit` (3600 s) stops a run that takes longer, with an
error.

**Isoclearance lines** (`--isoclearance FILE`, repeatable: `.geojson`,
`.dxf` or `.csv` by the extension): the ground's clearance to the nearest
wire's surface over all the positions checked, contoured over the
corridor (`--corridor`, the search distance, ±50 m around the spans'
chords) every
`--iso-interval` metres (5) up to `--iso-max` (40; lines above are not
drawn), on a `--iso-spacing` grid (1 m) refined from a `--iso-coarse` one
(8 m) where lines can be. A tree of height h standing inside the h line
can reach a wire when it falls (no root ball, level ground). GeoJSON:
LineStrings with `level_m`, project coordinates; DXF: layer
OHL_ISOCLEARANCE, the clearance written on each line; CSV:
`level_m,line,vertex,x,y,z`. The drawings draw them too:
`ohl drawing … --isoclearance-interval 5` (`--isoclearance-max`,
`--isoclearance-spacing`, `--isoclearance-swing-arc`), over the line
width, from the cases drawn and the ground.

**Wind steps** (`--wind-step Pa`, `--wind-sides both|left|right`): each
windy case is also solved in still air and at every pressure from 0 up to
the case's in steps of the value given, from the sides given (the full
pressure is checked from both sides in any case) — as the vertical and
radial clearance windows step the wind; alongside the swing arc or
instead of it. The positions are named `still`, `left (step) at 300 Pa`,
`right (step) at 300 Pa`.

**Clearance by voltage** (`--clearance`, `--fall-allowance`): one
distance (`5`, `"5 m"`), or as a rule file writes a distance by voltage —
a table `'{ by_voltage = [[220, "7 m"], [380, "9 m"]] }'` (up to kV, m:
the first row whose voltage is not exceeded; above the last row its
distance) or `'{ base = "3 m", per_kv = "0.01 m", above_kv = 110 }'`.
Each span is checked with the distance for its wire's system's nominal
voltage (an earthed wire: 0 kV). A single value works as before.
**Classes** (`--classes 42,43`): the vegetation classes by number instead
of by role.

### The vegetation panel

**Checks › Vegetation…** opens the danger-tree panel (a window that
**docks as a panel** like the clearance windows: 📌 Dock as a panel, ⬈
Float). At its top: **Check** (runs in the background), the name to
**save** the run as, and — when what the run was computed from changed —
the banner *Out of date: … changed since this run* with **Run again**.
Below, the **Preset** dropdown, then the settings in four groups. Each
value has a short explanation beside it (more on hover), each group a
link to its section here (**… user guide ↗**), and small drawings follow
the values typed.

#### Vegetation: wire positions

The **weather cases** ticked (the list stays open while you tick): a case
without wind as it is, a windy case blown to the left and to the right.
**Swing arc**: every position between the two blowouts (a band of radii,
widened by the **band margin**: see above). With the arc ticked, a field
**band margin** (m, default 0.15 m) follows, with a line saying what it
is and a longer text on hover (what it covers, that it is conservative,
that a smaller value is less safe — said in warning colour below the
default — and a button back to 0.15 m); the run is saved with it and
marked *out of date* when it changes. Under it the **jumper band
margin** is shown greyed: this check leaves the jumpers out. **Wind stepped**
(a step in Pa, the sides): still air and every pressure up to the case's.
Arc and steps work alone or together — the same wind choice as the
[vertical and radial clearance windows](desktop-app.md#clearances-wind),
with the solves a windy case needs said below it and the arc suggested
when fine steps need many (here the full pressure is solved from both
sides in any case; the sides apply to the steps). **Unbalanced ice**: off, the case's
ice on the span alone, more radial ice (mm), more ice load per wire
(N/m), a load at mid-span (N) — on the spans with vegetation within
reach. Drawings: the span seen along it with its positions, arc and
steps; three spans with the middle one iced.

#### Vegetation: grow-in

The **required clearance c**: one distance, a table *up to kV → distance*
or a base plus an amount per kV, with the same editor and preview (at 0
kV and the project's voltages) as the clearance rules — each wire checked
with its own system's voltage. A utility's practice may be higher than
the law, e.g. 7 m at 220 kV and 9 m at 380 kV. **Growth per year g** and
**years** until the next cut, the **buffer b** (e.g. the survey's
accuracy). A point closer than c + g·years + b grows in. Drawing: the
wire with the three rings.

#### Vegetation: fall-in

**Trees falling in**; the **root-ball fraction k** (the hinge k·h from the
trunk; default 0.05), the **allowance A** (default 2 m; one distance or by
voltage like c), the **smallest and tallest tree** (2 and 40 m), the
**tree base's ground point** within a distance (ticked, 5 m: else no
fall-in check for that point). Drawings: the falling tree (trunk, hinge
at k·h, the sector it sweeps, A to the wire), the tree base and the
nearest ground point.

#### Vegetation: search

The **classes** checked as vegetation — a tick menu of the project's class
scheme (number, name, colour; it stays open while you tick), by default
the classes whose role is vegetation; the **search distance** (half width
of the corridor read, ± 50 m), the **points kept per span** in the table,
**isoclearance lines** (interval, hide above, grid).

**Results.** The table lists per span the required clearance applied, the
grow-in and fall-in counts, the nearest point and the worst fall-in margin
with their case and position; **Worst points** below lists the worst of
each span. **Every** grow-in point and **every** danger tree found is
drawn, on the layers *Vegetation: grow-in* (a dot) and *Vegetation:
danger trees* (the trunk from base to top) in the 3D view, the plan and
the profile (within the line width), in two **colours** chosen in the
panel (the colour tool; kept with this project's view settings), one
batch each in 3D and one mark a pixel in the plan and the profile (10⁶
points stay quick; no number is coarsened). No line is drawn to the worst
point any more: the table names it. *Isoclearance lines* are drawn in the
colours of their clearance. **The layers show the run of the open panel
only: closing the panel takes them off the views** (also when ticked in
View › Layers); reopening it, or reopening a saved run, draws them again.
Docking or moving the panel keeps them. **Export CSV…** writes the worst
points of every span and every point found (with case and position),
**Export lines (CSV)…** the isoclearance lines.

**Presets.** The **Preset** dropdown lists the vegetation presets of
every design-code plugin turned on (a preset with vegetation settings;
the panel takes only those, its wind steps and its own weather cases,
never its clearance rules or their cases — see [Presets per
check](design-codes.md#presets-per-check)), grouped by plugin (the description on
hover; one that has none for this check says so). A vegetation preset
sets the classes, the clearance and allowance (by voltage), the growth,
the danger-tree values, the search distance and the wind steps, and names
its weather cases. Chosen, it is previewed — every value, and for each of
its weather cases what applying it does: **uses your case X** (a case of
the project with the same weather — temperature, ice, wind and the rest
a solve reads — whatever its name; with several, the first in your list,
and it says so) or **creates Y** (none is the same; when your project
already has a case of that name with other values, it is created as
"Y (preset)" and yours is kept). See [A preset's weather
cases](design-codes.md#a-presets-weather-cases). **Apply preset** puts
the values in (edit them freely after), creates the cases to create in
the project's weather cases (one change of the project, undone by Undo)
and ticks the preset's cases — your own where they are used.
The same dropdown lists the presets in the rule editor's **Load preset…**
and in the vertical and radial clearance windows (**Plugin preset**,
then **Load**).

**Saving.** **Save in Performed checks** keeps the run — its settings,
weather cases, per-span results, worst points and every grow-in and
fall-in point with its case and position — in Checks › Performed checks
(one edit, undone by Undo; a name in use is replaced on a second click).
Opened from there, the panel shows it with its layers, and says *out of
date* when the line (its objects and library files), the LiDAR points or
the ground changed since it was saved.

See the theory: [Danger trees](../theory/lidar-analysis.md#danger-trees-2026-10-20).

## Safety volumes for work near the line (2026-10-20)

*Built, in active development (not approved yet).* The space within a
distance of the wires in every position of the weather cases chosen —
for planning work near the line (cranes, scaffolds, trees felled): one
closed volume per wire (a circuit's phase through all its sections), per
circuit or per electrical system, shown in the 3D view and the plan, saved with the line and
exported as georeferenced files. Since 2026-10-21 the **jumpers** are
part of it (at their solved positions in each case: blown left and
right, still air, every step; with the swing arc, on a swing band of
their own about the line through their two end clamps), so the volume is
continuous at the structures; the structures themselves (steel,
insulators, fittings) are not.

**In the desktop app:** Checks › **Safety volumes…** The window reads
as the [vegetation panel](#the-vegetation-panel) and the clearance
windows do: the settings in five groups, each with a heading and a link
**user guide ↗** to its section below, a line under each value saying
what it does (a longer text on hover), and small drawings that follow
the values typed (the distance around a conductor, the wind positions
and the swing arc, the volumes grouped, a mesh where two tubes meet).

#### Safety volumes: wire positions

- **Weather cases**: tick any number. Each is solved by the line's
  solver: still air as it is, a windy case blown to the left and to the
  right at its full pressure.
- **Wind**: the two ticks shared with the danger-tree check — **swing
  arc** (also every position between the two blowouts: the span swinging
  about its attachment axis over the band from the smallest to the largest
  radius of its positions, widened by the **band margin**, closed form;
  each jumper on a band of its own) and **wind stepped** by **step
  Pa** (also still air and every pressure up to the case's, from the
  sides chosen); both, either or none (none: the blowouts at full wind).
  The window says how many solves a windy case needs, and suggests the
  arc when the steps need many. The drawing shows the positions seen
  along a span.
- **Band margin** (with the swing arc; since 2026-10-21): the shared
  field of the danger-tree check (default 0.15 m, conservative; smaller
  is less safe and on your responsibility). Saved with the run; a change
  marks the run *out of date*.
- **Jumper band margin** (with the swing arc; since 2026-10-21, the
  owner's decision): the jumpers' bands' own margin, default 0 m — no
  widening, so a jumper's band does not hold it between its blowouts (on
  the example line up to 0.293 m outside): a simplification on your
  responsibility; enter a value for your jumpers (the wind steps show
  it). Saved with the run (a run saved before reads the band margin, as
  its jumpers then took); a change marks the run *out of date*.

#### Safety volumes: distance

- **Distance from the conductors' surface**: the shared distance editor
  ([Distances by voltage](desktop-app.md#distances-by-voltage)) — **One
  distance**, a **Table by voltage** or **Base + per kV**, each value an
  input field with its unit, the distance at the line's voltages on the
  line under it. Each wire takes the distance of its own system's nominal
  voltage (earthed wires: 0 kV). **No safety distance is built in**:
  enter the one of the rules you work to, or choose a design-code
  plugin's preset and one of its rules — shown greyed, *(preview — Apply
  to edit)* — then **Fill the distance** (its radial distance, else its
  vertical one).
- **Margin**: added to the distance (e.g. the survey's accuracy, tools).

#### Safety volumes: what is included

- **Scope by** (2026-10-21): **Systems | Circuits | Spans**, then a list
  of the line's objects of that kind, each with a tick — systems: name
  and voltage; circuits: name, system and voltage; spans: section, span
  number (from 1), the two structures and the length in plan. Above the
  list a **search** field filters the rows as you type (by name or id, a
  part of it, upper and lower case alike); **All**, **None** and
  **Invert** act on the rows shown only, and "n of m ticked" counts the
  ticks. The list scrolls in a box of fixed height (hundreds of spans
  stay quick). All with nothing searched: every one, also those added
  later; none ticked: Run says to tick one. Systems end with a row
  **(no system)** (since 2026-10-21): the earthed wires and any section
  without a system, ticked like a system (their distance at 0 kV). A span ticked brings every
  wire on it; a jumper comes along when the spans at both its ends are
  ticked (continuity).
- **One volume per**: **wire** (a circuit's phase through all its
  sections), **circuit** (its phases together) or **system** (every
  circuit of one electrical system together; since 2026-10-21 the wires
  without a system — earthed wires — together in one volume named
  **(no system)**). The distance by voltage still uses each wire's own
  system voltage.
- The jumpers are part of the volumes (at their solved positions, and on
  a swing band of their own with the swing arc); the structures are not.

#### Safety volumes: mesh

- **Tolerance** (default 0.05 m): how far the volume may lie outside the
  exact one where it is smooth; it is never inside it. Smaller: more
  triangles, longer. The results give each volume's **outside by at
  most**: measured; where two tubes meet the triangles bridge the groove
  between them, so it can be larger than the tolerance.

**Run** computes it in the background. The table lists each volume: its
colour (its circuit's wire colour), voltage, distance, triangles, volume
and how far it lies outside the exact volume at most (measured; where
tubes meet the triangles bridge the groove, so this can be larger than
the tolerance). The volumes are drawn translucent in the 3D view (layer
*Safety volumes*) and their outlines in the plan while the window is
open. When the line or its weather cases change the window says *out of
date* (the layers dimmed) with **Run again** (also when the band margin
changes with the swing arc on).

**Save in Performed checks** keeps the run (its settings — with the
scope, the ticks and the grouping — and the volumes' meshes) with the project line; Checks › Performed checks opens it again.

#### Safety volumes: export

**Export to a folder…** (after a run) writes the formats ticked, in the
line's coordinate system (Properties of the line, *CRS*; the window shows
it, and says when none is set):

| file | contents |
|------|----------|
| `.dxf` + `.prj` | DXF R12: 3D faces (`3DFACE`), a layer `SAFETY_<volume>` per volume, in the line's coordinates; the `.prj` holds the coordinate system as WKT (EPSG systems) |
| `.ifc` | IFC4: one spatial element per volume (since 2026-10-21 an `IfcSpatialZone`, predefined type user-defined, object type `SafetyVolume`, referenced in the site), a closed triangulated face set, translucent; georeferenced by `IfcMapConversion` to an `IfcProjectedCRS` named by its EPSG code; property set `OHL_SafetyVolume` (line, circuit, phases, sections, voltage, distance, margin, rule, weather cases, wind, date, tolerance, volume) |
| `.kml`, `.kmz` | WGS84 for Google Earth: each volume a placemark of triangles at absolute heights, with the same properties; needs the line's real coordinate system (EPSG) |

Heights in KML are the line's (above its height datum), not corrected to
a geoid: see the theory.

**From the command line:**

```bash
ohl safety-volume line.ohl.json --weather cases.ohlwc --case "high wind" --case "max temperature" \
    --wind both --wind-step 100 --distance '{ by_voltage = [[0, 2], [110, 3], [380, 5]] }' \
    --margin 0.5 --per circuit --crs EPSG:2056 --out volumes --format dxf,ifc,kmz
```

`--wind none|steps|arc|both`, `--wind-step` (Pa), `--wind-sides
both|left|right`, `--band-margin` (m, default 0.15; with the arc),
`--jumper-band-margin` (m, default 0: the jumpers' bands' own margin;
with the arc),
`--distance` (required; the rule file's forms),
`--margin`, `--tolerance`, `--per wire|circuit|system`, `--circuit`
(repeatable), `--system` (repeatable: a system's id or name; `none` —
or `"(no system)"` — for the wires without a system, unless the line
has a system called so), `--span
SECTION:N` (repeatable, N from 1: every wire on the spans; given
together, a wire must match each filter),
`--crs` (when the line file has none), `--format` (dxf, ifc, kml, kmz),
`--name` (file name stem, default `safety-volume`), `--time-limit`.

On the example line (two cases, wind stepped and swing arc, 4 m): three
volumes of 23 000–48 000 triangles (the branch's with its jumper), about
a minute and a half (2026-10-21, a shared computer; with the band and the
jumpers).

See the theory: [Safety volumes](../theory/safety-volumes.md).

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
