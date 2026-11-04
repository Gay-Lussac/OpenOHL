# Galloping, blowout, vegetation and as-built wires

Analyses of a solved line beyond the clearance rules: galloping ellipses,
the conductor jump when the ice drops, blown-out wires against the right of way, LiDAR classification, vegetation
near the wires, safety volumes around the wires for work near the line
and catenaries fitted to the surveyed wires. Each is a
command of `ohl` and (but for classification and the right of way) a window
of the desktop app's **Checks** menu. The methods are in the theory manual:
[Galloping ellipses and blowout](../theory/galloping.md),
[Ice shedding](../theory/ice-shedding.md),
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
calculation = "load-fe"             # optional (2026-11-01): how the wires are solved; none: each section's own

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

In the desktop app: **Checks › Galloping ellipses…** — choose the weather
case (*2 Scope*) and the parameter file (*3 Rules*; the project's `.ohlg`,
or one next to a line file, is chosen first), then **Run** (the window's
footer, bottom left). Under the parameter file's name a line says what it
holds: its methods (with their loops), its requirement and the cases it
names (2026-10-31). The Results tab lists every pair and method (spans
named from 1), *Only violations* on by default, the notes folded above
the table; a row clicked selects its span and draws it in the weather
case of the run. **Export CSV…** writes the rows shown, **Full report
(CSV)…** every pair with the case.

## Ice shedding: the conductor jump when the ice drops

*Built 2026-11-02, in active development (not approved yet); roadmap
O13, the simplified method.* When the ice falls off a span at once, the
wire jumps upwards and may come close to the wire above it. The check
raises each span of the wires a rule names from its iced position by the
rule's **rebound factor** k times its **static rise** (the bare position
at rest less the iced one), and measures the clearance from the jumped
wire to every other wire of the same span (phase to phase, phase to
earth wire), the other wires keeping their ice. The method, its
assumptions and the sources: [Ice shedding](../theory/ice-shedding.md).

```bash
ohl ice-shedding synthetic-line.ohl.json --weather weather-cases.ohlwc \
    --rules ice-shedding-rules.ohlr --csv ice-shedding.csv
```

The rules are tables of a rule file (`.ohlr`), beside or instead of its
clearance rules:

```toml
[[ice_shedding_rule]]
name = "One span sheds"
cases = ["glaze ice"]       # the iced cases (none: every case with ice)
# bare = "-5 °C bare"       # the bare case (default: the iced case without its ice)
# shedding = "all"          # one (default) or all
# calculation = "creep-fe"  # how the wires are solved (default: each section's own)
factor = 2.0                # the rebound factor k — optional, 2 by default (an upper
                            # bound from mechanics, to be decided; give a published one)
distance = "2 m"            # or { by_voltage = [[110, "1 m"], [245, "2 m"]] }
# pair_voltage = "higher"   # higher (default), sum or difference
# targets = { circuits = ["North"] }   # the wires that shed (default: every wire)
```

- `factor` is required: no value is built in. Take it from the published
  method you design to and check how it defines the factor — here it
  multiplies the static rise from the iced to the bare position at rest
  (k = 1: the bare wire at rest).
- `shedding = "one"` (default): each span sheds alone, the others keep
  their ice; the span's iced horizontal tension is held, which gives the
  largest rise (conservative). `"all"`: every span of the wire sheds; the
  line is solved in the bare case too and the tension drops, so the rise
  is smaller. The time: one solve per iced case, one more per bare case
  with `"all"`.
- A weather case without ice is skipped (a rule naming none with ice
  says so).

The output lists per rule, iced case, span and other wire of the span:
the largest rise, the jump (k × rise), the clearance before (both iced)
and after the jump, the required distance and pass or fail, the smallest
margin first; exit status 0 when every clearance is kept, 1 when one is
not, 2 on an error. CSV columns: rule, case, bare case, section, span,
other section, other span, factor, rise, jump, before, after, required,
margin, status, where (fraction of the span), the closest points.

In the desktop app: **Checks › Ice shedding…** — the rules in *3 Rules*
(kept in the project as `<line>.ice-shedding-rules.ohlr`; a design-code
plugin's preset brings its ice shedding rules), the wires that shed in
*2 Scope*, then **Run**. The Results tab lists every pair; a row clicked
selects its span, drawn in its iced case. The 3D view's layer *Ice
shedding (jumped wires)* draws each jumped span (red where a clearance
after the jump is short, else orange) and the shortest distance of each
failing pair while the window is open. **Save in Performed checks** keeps
the run with the project line ([The library of performed
checks](#the-library-of-performed-checks)).

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
run in the windy case (`ohl line --rules`, the Clearances window).

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
must be in use: File › Plugins › Plugin manager). A plugin for the Swiss LeV
exists separately; it is not part of the public code. What is measured
and how: [the core's part](../development/phase-distance-check.md).

In the desktop app, with such a plugin in use (the plugin defines the
menu: without it, it is not there), its entry in the Checks menu (e.g.
**Checks › LeV/OLEI › Distance between conductors…**) opens the check's
window: a values file (else the project's copy, else the plugin's), the
plugin's choices, the weather cases (each checked on its own), the
circuits (ticked in a list, select all / none), the station spacing
(1 m), and nearby spans (conductors not between the same two
structures, within a distance). On the check windows' layout
(2026-10-27; 2026-10-31): *1 What it checks* with *How it is computed*
folded, *2 Scope* (the weather cases — **Weather cases: n of m**, a tick
menu; under unequal wind one case, the weather case list the other
single-case windows use —, circuits, routes), *3 Rules* (**Choose a
values file…** and the plugin's choices, the circuits' levels, the
values used), *4 Model*, folded (the station spacing — under unequal
wind the wind step and the other conductor's share instead —, nearby
spans; each with a line saying what it changes); the Results tab holds the saved
report and *Save*, the verdict and the table (*Only violations* — the
pairs below z — on by default, sortable, grouped by case, span or rule).
The setup is kept in the project per project line (`app.phase_distance`,
`app.unequal_wind`). Without its plugin the window says so once and Run
is greyed ("Before Run: the check plugin: …"). The window opens floating; its **📌**
docks it as a panel (next to Properties), like every check window
([one design](desktop-app.md#check-windows-one-design)). When the
plugin needs a value per circuit (a plugin column, e.g. an insulation
level), **Circuits checked** lists the ticked circuits with their
voltage and that value, chosen there for all the circuit's sections —
the same value as the Conductor paths table's column. **Run** (the
window's footer, bottom left) runs in the background; the pairs come worst first: the two spans and
circuits, kV, d, the required distance, the margin, the length of
conductor below it, and at the worst station the measured values, the
plugin's own and the rule it applied, with a note when it is at a clamp
or on a nearby span. Clicking a span, or a distance, selects the span,
draws the two conductors' sections in the result's weather case (first,
the cases shown after it; the case shown for the rest of the line stays
as it is), says the case beside it, and draws the worst station's line
in the 3D view, labelled with d and the required distance; the line goes
when the window is closed (or a new run starts), the result's case when
the window is closed or you select something else. **Export CSV…** writes
the rows shown, **Full report (CSV)…** every pair with the points. Below the settings, the window explains the
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
results and whether the line changed since (*stale*); **Run** runs it
again with its parameters, shows each pair against the saved run (new,
fixed, worse, better) and asks whether to replace the saved report.


### Under unequal wind

With a plugin that runs it (e.g. **Checks › LeV/OLEI › Distance between
conductors under unequal wind…**), the same window checks the distance
two conductors keep while the wind blows them unequally: one at a wind
p, the other at a share of p up to p (LeV: 60 … 100 %), p from none up
to the wind of the **weather case** chosen (its temperature and ice
kept), in **steps** you set (default 50 Pa; the case's wind always
included), from each side. The line is solved at each step and at the
share of each (a few seconds per level on a long line, side by side on
the cores). The pairs are chosen as above (circuits, nearby spans).
Per pair: the closest approach — the exact shortest distance between
the two wires (a bundle by its subconductors), not at stations —, d,
the required distance (the plugin's: with the LeV, C₃ within a system,
C₄ between systems), the margin, the wind on each conductor and the
side.
Clicking it draws the line between the two points and the pair's two
conductors as they were measured, each at its wind, labelled with it
and the side — only that span's pair, on top of the view; the pair's
sections are drawn in the result's weather case (as above), the case
shown for the line stays as it is and nothing is solved (a saved run keeps the
pair's conductors to draw them too). Values, saved runs
and CSV as above: **Save** keeps the run in the library of performed
checks, and opened again from there it shows its pairs on its Results
tab with its weather case, wind step and share set again; **Run** runs
it again with them and compares it with the saved run, as for the
distance in the span. (Runs saved before 2026-11-04 could not be opened
again: they open now.) How it is measured: [the core's
part](../development/phase-distance-check.md#under-unequal-wind).
## The library of performed checks

**Checks › Performed checks…** (a window like the check's, floating
first, docked with its 📌) lists the checks saved on the project line you work on: name, check, when, by whom, **status** (draft,
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
Float). At its bottom left, in a footer that stays in place while the
panel scrolls: **Run** (runs in the background) and its status. At its
top — when what the run was computed from changed — the banner *Out of
date: … changed since this run* with **Run again**; under the results,
the name to **save** the run as.
Below, in the check windows' sections: *2 Scope* (circuits, routes,
sections, weather cases), *3 Rules* — the **Preset** dropdown, then the
**Grow-in** and **Fall-in** values (the clearance, growth, buffer and
allowance; under *Rules* since 2026-10-31, the owner's choice) — and
*4 Model*, folded: the **wire positions** and the **search**. Each value
has a short explanation beside it (more on hover), each group a link to
its section here (**… user guide ↗**), and small drawings follow the
values typed.

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

**Scope** (2026-10-27, the owner: some circuits "we will model but we
don't need them checked"; built, in active development, not approved
yet). In the panel's *2 Scope*: the **circuits**, the **routes and
alignments** and the **sections** checked, and the **weather cases** —
each a tick menu that stays open (search, All / None / Invert). The line is
still solved whole (sections solved together stay together); the spans
of the wires out of scope are left out of the check: no grow-in point or
danger tree is looked for near them, and the scope is said in the run's
notes. The panel's setup — settings, cases (by name), scope, **Use local
conditions** (`local`, 2026-11-03: the wind for clearances) —
is **kept in the project** for each project line (project setting
`app.vegetation.<line>`), read again when the project opens.

**Results** (the panel's Results tab, opened by itself after a run; [the
check windows' layout](desktop-app.md#check-windows-setup-and-results)).
A verdict first (spans with vegetation, grow-in points and fall-in trees,
the nearest to the clearance), then the table: per span (named from 1,
"S1 span 1" the first) the required clearance applied *c [m]*, the
grow-in and fall-in counts, the nearest point *[m]* and the worst fall-in
margin *[m]* with their case and position — *Only violations* on by
default (the spans with grow-in or fall-in), sortable by any column, a
row clicked goes to its span; **Worst points** folded below lists the
worst of each span. **Every** grow-in point and **every** danger tree found is
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
Docking or moving the panel keeps them. **Export points CSV…** writes the worst
points of every span and every point found (with case and position;
**Export CSV…** under it, the rows of the table as shown),
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
The rule tables of the other check windows list them in their one
**Preset** (a dropdown) (previewed, then **Merge** or **Replace**).

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
and the swing arc, the volumes grouped, a mesh where two tubes meet). On
the check windows' layout (2026-10-27): *2 Scope* holds the weather cases
and *What is included*, *3 Rules* the distance, *4 Model* (folded) the
wire positions and the mesh (in *What is included* until 2026-10-31); the volumes, *Save* and *Export* are in the
Results tab. The setup is kept in the project per project line
(`app.safety_volumes`).

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

**Run** (the window's footer, bottom left) computes it in the
background. The table lists each volume: its
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

Desktop app: **Checks › As-built wires…** (spans named from 1 in its
table since 2026-10-27; Run greyed without a point cloud, saying where
to import one). Its *3 Model*, folded (2026-10-31), has a labelled row
each: **Model temperature** (the still-air case compared with), **Points
across** and **Points up or down** (how far from the modelled wire a
LiDAR point may lie to be fitted) and **Survey temperature** (estimated
from the model's sag at each temperature: longer runs), each with a line
saying what it changes.

## Electrical: line parameters

*Built, in active development, not approved yet* (roadmap O18, 2026-10-27).
The series impedance with earth return and the capacitance of every
circuit, from the wires where they hang in a weather case — the same
geometry as the mechanical design. Methods, assumptions and validation:
[Line parameters](../theory/line-parameters.md).

**What it needs:** the sections' systems and phases (Line › Electrical
systems; sections of no system are taken as earthed earth wires), the
wire types' electrical data (`[cable.electrical]`: AC resistance, geometric
mean radius — see [Wire library](wire-library.md#electrical-data-cableelectrical)),
and the line's **electrical settings**:

| Setting | Default | Meaning |
|---|---|---|
| Soil resistivity | none: **100 Ω·m assumed**, said with every result | homogeneous earth along the line, Ω·m |
| Earth return | Carson's series | or the complex depth, or Carson's first terms ("modified Carson") |
| Height in a span | the solved wire's average | or the attachment height minus 2/3 of the sag |

**In OHL-Design:** Checks › Electrical › **Line parameters…**. The
settings in *3 Model* (folded) are the line's (each change one undoable
command, kept in the project). Choose the weather case in *2 Scope* (the wires as
solved in it; its temperature is the conductors' for the resistances; it
was in the footer until 2026-10-27) and **Run**; the results open in the
Results tab. The table gives per circuit the positive and zero sequence
R, X (Ω/km) and B (µS/km) of the ideally transposed line and the totals;
the + button shows the circuit's stretches (consecutive spans with the same
sections). Hover a circuit to see how much its sequences couple as built
(a line not transposed). The coupling rows give the zero sequence mutual
impedance between circuits sharing spans. A missing value is "-" (a
circuit without three phases, the coupling's own sequences); the notes
are folded above the table; a row clicked selects the spans of its
circuit's sections (a stretch: its own; a coupling: both circuits').
**Export CSV…** writes the rows shown, **Full report (CSV)…** the
table with the notes, as `ohl electrical` does. The results are marked out of date when the line
changes, like the other checks.

**On the command line:**

```text
ohl electrical parameters LINE.ohl.json|PROJECT.ohl
    [--weather CASES.ohlwc --case NAME | --temperature 20]
    [--height solved|mean] [--soil OHM_M]
    [--earth carson_series|complex_depth|carson_first_terms]
    [--terrain FILE | --surface NAME]
    [--section ID]… [--matrices] [--cross-sections] [--csv OUT.csv]
    [--library WIRES.ohlw]
```

Without a weather case the wires hang in still air at `--temperature`
(20 °C by default). `--height`, `--soil` and `--earth` override the
line's settings for this run (`mean` is the two-thirds rule).
`--terrain` (a terrain file) or `--surface` (a terrain surface of the
project, `ohl surface list`) gives the ground under the spans; without
one the ground is straight between the structures' feet, which over a
valley lies above the wires: such a span's wires are left out, said in the
notes, and the circuit's totals are short of it (the note says so too).
`--section` keeps only the stretches holding those sections;
`--matrices` prints each circuit's phase matrices per km (R + jX, and C in
nF/km); `--cross-sections` every span's wires (offset to the right of the
line, height above the ground). Try it on the synthetic example:

```text
ohl electrical parameters examples/electrical/double-circuit.ohl.json \
    --library examples/electrical/wires.ohlw --cross-sections
```

Not yet: other lines in the corridor, soil resistivity along the route,
a transposition plan, the terrain in the window (the ground is taken
straight between the structures' feet there) and the point cloud's ground,
audible noise (roadmap O18). The magnetic and electric fields: below.

## Electrical: magnetic field

*Built, in active development, not approved yet* (roadmap O18 step 4,
first slice, 2026-10-27). The magnetic flux density (µT, the rms of the
vector) of the line's currents, from the wires where they hang in a
weather case. Methods, assumptions (each marked conservative or not) and
validation: [Magnetic field](../theory/magnetic-field.md).

**What it needs:** the sections' systems and phases (Line › Electrical
systems; the circuits are the phases of a system in one named circuit;
sections of no system are earth wires), and **a current per circuit**.
For the earth wire currents also the wires' electrical data and the
line's soil resistivity (as for the [line
parameters](#electrical-line-parameters)). **No limit is built in**: an
ordinance's values (which current, which places, which limit) are yours
to give, or come from its plugin's parameter file later.

**In OHL-Design:** Checks › Electrical › **Magnetic field…** Its inputs
— the weather case, the currents and directions, the model, the earth
wires, the profiles, the limit and the points (the CSV's text) — are kept
in the project for the project line worked on (2026-10-27): the window
opens as it was left, for every user of the project. A case or span no
longer in the line falls back to the default.

- **Currents**: per circuit its current (A, rms, each phase) and its load
  flow, *along the line* (from the line's first structure to its last) or
  *against it* (every phase turned by 180°). The phases are balanced at
  their systems' phase angles (L1 0°, L2 −120°, L3 +120° unless the
  system gives others). By default a circuit's current is its system's
  **operating current** (Line › Electrical systems, column A; 2026-11-04),
  said *the line's* beside it; a current typed here overrides it (a
  button brings the line's back).
  **Given** *the circuit* (one current) or *at its end points*
  (2026-10-29): the current in at each end (negative: out), one end
  ticked *the rest*; every section's current follows (branches
  included; a circuit closing a loop is refused: it needs the load
  flow). On the command line `--end CIRCUIT@STRUCTURE=A` (or `=rest`),
  `--ends` lists the end points.
- **Model**: *3D* — the wires as solved (sag, line angles, the line's
  length), bundles subconductor by subconductor; *2D* — the wires as
  infinite straight conductors where they cross the profile (tick *wires
  at the span's mean height* to take the line parameters' mean instead).
  2D results warn where the model is not valid: near a line angle over
  5°, on a span between differently arranged structures.
- **Earth wires**: tick to let them carry the currents the phases induce
  (it may raise the field as well as lower it). *Through the structures'
  earthing* (on by default, 2026-10-29): the earth wires as a network
  over the line, each structure leading current into the ground through
  its **earthing resistance** (Structures table, **earthing Ω**, or the
  structure's window; else the line's, in Line parameters; else 10 Ω
  assumed, said) — the structures' currents in the field and listed in
  the results. Unticked: an infinitely long earth wire earthed everywhere
  (far larger currents on a short line).
- **Jumpers** (3D, on by default, 2026-10-29): a jumper between two
  section ends of one phase carries that phase's current; jumpers on taps
  or between different phases are left out and listed in the notes.
**Per electrical system** (2026-11-01, in *2 Scope* under *Wires by*;
the owner: "a weather case per system (e.g. earth wire at 10 °C and
conductors at 40 °C due to being in use) … best would be per system"):
each system of the line, and the earthed wires without one, may take
its own weather case and calculation (*as above*: the window's); each
case and calculation is solved once for the run and every section taken
from its system's, the jumpers from the window's case; the notes say
which ("per system: system ex220: in max temperature").

**Ground from** (2026-11-01, in *2 Scope*; the owner: "choose which
TIN layer it uses and if none exist which survey data to use to make
it"): the surface of the checks (default), a terrain surface (TIN layer)
of the project, or the survey's ground points (the point cloud's ground
classes within a distance of the spans, 250 m by default, the lowest per
metre) made into a surface for the run — the electric field's tiles, the
profiles' height and the cuts' and volumes' bottom stand on it.

**Stretches per span** (Electric field, 4 Model, 2026-11-01): each span's
charge in this many stretches along it, 24 by default (16 before); its
effect measured beside it and in [the
theory](../theory/electric-field.md#the-charges). **The settings'
effects, measured** — the cut-off distance, the cuts' and volumes'
steps, the tiles — are tabled in the theory pages ([magnetic
field](../theory/magnetic-field.md#cut-offs-and-grids-measured), [electric
field](../theory/electric-field.md#the-tiled-floor-default)).

**Long volumes in parts** (2026-11-01, the owner: "modelled in parts to
keep the amount of memory used low in long lines"): a volume's lattice
is computed along its run in parts of at most some 20 million points
(160 MB) each, the plane where two meet shared — the same surfaces as
in one piece (tested), far less memory on a long run.

**3 Spans and what to compute in each** (2026-10-31, the owner's
validated rework; the same in both field windows). **Make** (2026-11-01,
the owner: "the 3D method to only make the volumes … 2D where you can
setup cuts … and also ask if we want the profiles"): **2D: cuts and
profiles** — the cuts at the stations asked in the table and, with
**profiles too** ticked, the profiles (and the points from CSV) — or
**3D: volumes** — only the volumes of the spans with *Volume* ticked. A
run computes each kind as a step of its own, one after another (its
progress says which: "2 cut(s)", "the profiles", "1 volume(s)"): the
cuts and the volumes from the wires in 3D, the profiles by their model
(*Profiles by* 3D or 2D, 4 Model). The isolines *from above* are no
longer offered (2026-11-01). **The span table**:
every span a row — per alignment its spans in station order, ticked
(the spans of the line's run along it, and those out of its first or
last structure); then the other spans, unticked: on no alignment, of
another line crossing or running beside it, connections from a structure
in the middle of the line (owner, 2026-10-31: the main line's spans not
interrupted by crossing spans) — with its length, its profile, its **2D cuts at** (the
stations asked: *+ station* at mid-span, at its lowest point or a
distance from its first structure; a click on a station's chip removes
it; 2D) and its **Volume** (3D). **A cut is made
on the alignment** (owner, 2026-10-31: "cuts are made on the alignment
not the circuits or span"): each chip is placed at the alignment station
of that place in the span (between its structures' stations; the lowest
point found by the run, then placed the same way), the cut's plane
perpendicular to the alignment — at an angle point along the bisector —
exactly as a cross-section panel opened there. A span on no alignment
can have no cut: the window says why under the table and the run leaves
it out. An unticked span
is greyed and nothing is computed in it. **Press on a tick and drag
across the rows**: every row passed takes the state the press gave;
shift-click ticks a range; a filter (part of a span's
name), *All*, *None* and *Invert* act on the rows shown. The table is as
tall as you drag its handle. The ticks, the stations and the volumes are
kept in the project; with no span ticked, Run says so. Under the table:

- **Profiles**: at each ticked span's lowest point, *beside its structures*
  (2026-10-31: a profile this far into each ticked span from both its
  structures, 10 m by default), or one span at its
  lowest point or at a place 0–1 along it; **Across** ± the width every step at a
  height above the ground (default ±100 m every 1 m at 1 m; ±50 m until
  2026-11-02: on a real line the 1 µT isoline lay beyond it in most
  cuts, see [Cut-offs and grids](../theory/magnetic-field.md#cut-offs-and-grids-measured)). The ground is
  the project's surface for the checks when there is one, else straight
  between the structures' feet.
- **2D cuts** (2D) or **Volumes** (3D): their sizes — the cuts' half width, step and reach over the highest wire
  (default ±100 m — ±60 m until 2026-11-02 — every 1 m — 0.5 m until 2026-11-01: the low levels' isolines as close, at a quarter of the points; see [Cut-offs and grids](../theory/magnetic-field.md#cut-offs-and-grids-measured) — to 10 m over it); the volumes' width each
  side of the axis (2026-11-01: their own, so a low level's surface
  reaching farther than the profiles is inside the box; 100 m by default
  since 2026-11-02) and step — each with how many the table asks and how
  long they take (5 Views below).

Beside **Run**, a summary: the spans, profiles, cuts and volumes the run
will compute and about how long. In **2 Scope**:

- **Limit**: optional, µT: the margin at every point and the limit line's
  extent on every profile.
- **Points from CSV…**: places to evaluate (e.g. places of sensitive use),
  a CSV with a header naming `x`, `y` and `z` (elevation) or `height`
  (above the ground), optionally `name`.

**Results** (2026-10-31): first a list of everything computed — every
profile, cut, volume and grid with its largest value and a note — and a
button on each: **Show plot** picks the profile in the plot below and
scrolls to it, **Open cut** opens a
cross-section panel at the cut's alignment station on the wires — on the
very plane the run computed, the cut drawn in it —, **Show in 3D** turns
the 3D view to the volume or grid and shows the field's layer. A cut's
row names its alignment and station (and the span it crosses). The
**3D** tick of a cut, volume or grid (2026-11-01) draws it in the 3D view
and the cross-section panels or not; a new run draws all. The volumes'
surfaces are translucent: the structures, wires and points behind them
stay visible.

**Save in Performed checks** (2026-11-01; under the list) keeps the run —
its inputs, the profiles, cuts, grids, points and the volumes' surfaces
(up to two million triangles; a larger run is kept without them) — in
Checks › Performed checks; opened from there, the window shows it with
what changed on the line since.

**4 Model** (3D; 2026-10-29; folded since 2026-10-31: open it to change
the model). Every setting says beside it what it
changes in the results and about how long it takes on this computer —
from the last run's speed, else a short measurement (a laptop slows to
about half speed after a few seconds of full load; the estimate allows
for it).

- **Wire pieces**: *as solved* (the solver's 64 pieces per span or part
  of a span; it says the longest and how far a wire lies off them, a few
  millimetres) or *at most* a length: the pieces are cut, the new points
  placed on the curve through the solver's points (on the chord at a
  kink: a spacer, a clamp). The field changes by about the wire's offset
  over the distance to it: a few hundredths of a per cent at 10 m.
  Shorter pieces take longer in proportion.
- **Modelled**: the *wires within* a distance of each cut and volume —
  **2 km by default since 2026-11-01** (the defaults reviewed for accuracy
  and speed) — or *the whole line* (the most exact; a project kept before
  keeps its choice). Profiles and points always take the whole line. Each run then computes the whole
  line's field at some 200 points of the cut or volume and says how much
  the wires left out change it (on a real line of 28 km, 2 km changed it
  by at most 0.03 % and made a volume ten times quicker).

**5 Views and levels** (2026-10-29). The **levels** are a table of
values (µT) with a colour each, ticked to draw them, **+ level**, **+ the
limit** (the limit given above) — display only, none is a limit. *Colour
the cuts by band* fills a cut's cells with the colour of the highest
level at or below their value. The views, drawn on the layer **Magnetic
field (isolines, surfaces)** while the window is open:

- **2D cuts**: at the stations asked in the table (*+ station*), computed
  by the run on the alignment; their size under the table. **Results ›
  Open cut** opens a cross-section panel at the cut's alignment station:
  the same plane as the run's cut (one placement for both), on the wires.
- **Cross-sections**: *also in any cross-section panel I open*
  (**Cross-section…**, C), every step (default 0.5 m) — open a cut at a
  building to see the field around it with the ground, the LiDAR points
  and the line. Computed after a run whenever a panel opens or moves
  (some seconds each), from the run's wires.
- **Volume**: isosurfaces of the levels over the spans with *Volume*
  ticked in the table (one volume along each run of them following each
  other, bent at the line angles: no seam at the structures; its step
  under the table; **Results › Show in 3D** turns the 3D view to it),
  ± the width, from the ground to 10 m over the highest wire, the lattice every
  step (default 2 m), drawn translucent in the 3D view. A coarse lattice
  first, then the fine one only where a level passes, near the wires and
  wherever a surface goes on. On a real line (7 circuits, 1000 A each), one
  span at 2 m took about a minute with the whole line modelled and 6 s
  with the wires within 2 km; 1 m takes about four times as long. Thin,
  short pieces of a surface (at most 2 steps across, 20 long: a tube
  thinner than the lattice, e.g. round an earth wire at a high level, met
  only here and there) are left out and counted with the results
  (2026-10-31: drawn, they were beads along the wire); a finer step
  resolves them.

Every value at a grid or lattice point is computed in full; only the
lines and surfaces between the points are interpolated. **Each run says
how far its drawings lie off their levels**: the field computed again at
up to 200 points of every isoline and surface (half within …, nine in ten
within …, at most …). A level drawn nowhere is said: *not reached*, or
*the field is above it everywhere shown* (widen the view). Thin surfaces
around the conductors (100 µT at 1000 A: some 2 m from a wire) need a
fine step: at 2 m they lay up to 50 % off their level on a real line, at
1 m within 4 % for nine points in ten.

Choose the weather case in *2 Scope* (in the footer until 2026-10-27),
the currents under it, and **Run** (a progress bar and **Stop** while it
runs; the time left from the points computed so far and, in the electric
field, the tiled floors still to solve at the speed of those solved —
every floor of the run counted from the start, 2026-10-30); the results open in the Results tab. The results: the currents used, the earth wires'
currents, the notes (what was assumed); the chosen profile plotted —
B against the distance from the axis (right positive), the cycle's
largest value fainter, the limit dashed, the value under the pointer; a
table per profile (station, the distance along the line, the largest B
and where, the limit line's extent, warnings such as a line end nearer
than five times the limit line's reach); a table of the points with B and
the margin (red when above the limit). **Export CSV…** writes every point.
The results are marked out of date when the line or the ground changes;
the views then draw faded. Under **Views**: per view what was computed
and how far its drawings lie off their levels. Not yet: the thermal
rating as the currents (on the command line only), places of sensitive
use as objects of the project, the structures' magnetic permeability
(estimated small: theory page).

**On the command line:**

```text
ohl electrical magnetic LINE.ohl.json|PROJECT.ohl
    [--weather CASES.ohlwc --case NAME | --temperature 40]
    (--current CIRCUIT=A[:reverse])… (--end CIRCUIT@STRUCTURE=A|rest)… [--ends]
    [--thermal AMBIENT.ohlta --ambient-case NAME --max-temp °C
        [--method ieee738|cigre601|cigre207] [--reverse CIRCUIT]…]
    [--earth-wires [--towers]] [--no-jumpers] [--two-d [--mean-heights]]
    [--profile FROM:TO[@FRACTION]]… [--width 50] [--height 1] [--step 1]
    [--grid FROM:TO]… [--grid-step M] [--points FILE.csv] [--limit µT]
    [--piece-length M] [--modelled-within M]
    [--cut ALIGNMENT@STATION[#LABEL]]… [--cut-step 1]
    [--volume FROM:TO]… [--volume-step 1] [--above 10] [--level µT]…
    [--obj SURFACES.obj]
    [--terrain FILE | --surface NAME] [--all-points] [--csv OUT.csv]
    [--json OUT.json]
    [--preset-plugin DIR [--params FILE.toml] [--preset NAME] [--out DIR]
        [--cloud FILE.copc.laz|PROJECT.ohl[#NAME]]]
    [--library WIRES.ohlw]
```

**The ground** (both fields, 2026-10-31): `--terrain` or `--surface`;
without either, a project's **surface of the checks**, else its **point
cloud's ground** along the whole line (some seconds, said), as in
OHL-Design. A line file without terrain takes the ground straight between
the structures' feet and says so as a **warning**: across a valley that
line passes close under the wires (a 220 kV span read 14.5 kV/m at 1 m
there, 0.007 kV/m over the real valley floor).

`CIRCUIT` is a circuit's name or its system's id, `*` every circuit;
`:reverse` sets the load flow against the line. `--thermal` takes each
circuit's thermal rating (the least over its sections of the bundle's
ampacity at `--max-temp` in the ambient case; see [Thermal
rating](../theory/thermal.md)) — the ambient case and the temperature are
yours to give; `--current` then overrides single circuits. A circuit
given none takes its system's operating current from the line file
(`current_a`, 2026-11-04; said in the notes), else 0 A. `--profile`
without `@` takes the span's lowest point; none: every span's.
`--grid` adds a grid on a horizontal plane over a span (to draw isolines
elsewhere). `--cut` computes a vertical cut across the line **at a
station of an alignment** (2026-10-31; e.g. `--cut main@150`, `m` after
the station allowed): its plane perpendicular to the alignment, at an
angle point along the bisector — where OHL-Design's cross-section panel
at that station lies —, ± `--width`, from the ground to `--above` over
the highest wire; its points in the CSV as `cut` rows. The span it
crosses is found by the run and printed. The older `FROM:TO[@FRACTION]`
(a span's place; without `@` its lowest point) is still taken and
**converted** to the alignment station of that place — said in the
output (`given as T2:T3@0.25: converted to main@375.00`); a span on no
alignment, an alignment not in the line or a station beyond it is an
error saying so. `--volume` the isosurfaces of the `--level` values over
a span (written by `--obj` as a Wavefront OBJ file, one object per span
and level); both say how far their drawings lie off the levels, and with
`--modelled-within` what the wires left out change. A cut may carry a
**label** after `#` (e.g. `--cut main@150#house 12`): it is printed with
the cut and written in the CSV's last column (`label`) and the JSON.

`--json` writes the results as JSON (format `ohl-field-results-set`: one
`ohl-field-results` document per field run — the cuts with their
alignment station, values, ground and isolines of the `--level` values,
the given points, the settings and the case; [data
model](../development/data-model.md#field-results-json)).

<a id="both-fields"></a>**Both fields in one run** (2026-10-31; built,
in active development, not approved yet; the owner: "Ok for 1 but keep
it clean"): `ohl electrical fields` takes the options both fields share
once — the line, the case, the ground, the profiles, `--cut`,
`--volume`, `--points`, `--width`, `--above` … —, the magnetic field's
own (`--current`, `--end`, `--thermal`, `--earth-wires`, `--grid`,
`--modelled-within` …) and the electric field's own (`--voltage`,
`--plane`, `--tile`, `--steel` …), with the levels and limits per field
(`--level-ut`, `--level-kv-m`, `--limit-ut`, `--limit-kv-m`) and a CSV
per field (`--csv-magnetic`, `--csv-electric`). It runs the magnetic
field, then the electric field, on the same places and the same ground
(the electric field's reach), prints both, writes one JSON set (both
documents) and, with `--preset-plugin`, calls the plugin's report once
with both:

```text
ohl electrical fields LINE.ohl.json|PROJECT.ohl (--current CIRCUIT=A)…
    [--voltage CIRCUIT=KV]… [--plane] [--cut ALIGNMENT@STATION[#LABEL]]…
    [--level-ut µT]… [--level-kv-m KV/M]… [--limit-ut µT] [--limit-kv-m KV/M]
    [--json OUT.json] [--preset-plugin DIR …] [the other options of both]
```

**Field presets from a plugin** (2026-10-31; built, in active
development, not approved yet): `--preset-plugin DIR` names a plugin
with the `field-presets` export (e.g. a future building plugin). The
points its manifest asks for are gathered — the point cloud of
`--cloud` (default: the project's) and the survey points of `--terrain`
—, the plugin gives the cuts and points to compute (with its parameters
from `--params`, else its template; its preset `--preset`, else its
first), they are added to those you give, the field — or both with
`ohl electrical fields` — is run as without it, and the plugin's report
files of the run are written into `--out` (default: the current folder).
Every value the plugin gives is checked first (each cut on an alignment
of the line at a station within it, sizes above 0, names unique, file
names without a folder): [Writing
plugins](../development/plugins.md#field-presets).

<a id="building-cuts"></a>**The building-cuts plugin** (example plugin
`plugins/examples/building-cuts`, 2026-10-31; built, in active
development, not approved yet) finds the **buildings** near the line and
gives the cuts and points the fields need at each of them:

- **Buildings** from the point cloud's class role `building` and from
  survey points whose code matches `building_codes` (default none: the
  point cloud's classes alone; e.g. `BLD*`).
  The points are put on a plan grid (`cell`, 0.5 m); cells closer than
  `gap` (1.5 m) belong to one building; fewer than `min_points` (20)
  points or a footprint under `min_area` (10 m²) is no building (the
  count left out is said). Survey points with a breakline id form one
  outline each, in the surveyed order. The result depends on the points,
  not on their order.
- **Footprint**: the convex hull of the points (default), or with
  `outline = "cells"` the cells' boundary traced (an L-shape keeps its
  notch), corners within `simplify` (0.25 m) of the simplified line
  dropped. The cells' outline runs on the cells' edges: up to one cell
  wider than the points on each side.
- **Heights**: the ground is the 10th percentile (`ground_percentile`)
  of the ground points 2–5 m around the footprint (`ground_ring`; the
  point cloud's ground role and the survey's `ground_codes`; farther
  ones, then the building's lowest point, when none — said); the eaves
  the 50th percentile of the building's elevations, the roof the 98th;
  **floors** = ceil((eaves − ground) / `floor_height` (3 m)), at least 1.
  A building whose eaves are less than 2 m over its ground (a surveyed
  footprint) is taken `default_height` (6 m) high, said.
- **Cuts** across the line **on the alignment** (2026-10-31): each at a
  station of the alignment nearest the building (within
  `max_distance`), its plane perpendicular to the alignment — at an
  angle point along the bisector —, as the engine and OHL-Design's
  cross-section panels place it. The building's stations along the
  alignment drive them: the **governing cut** at the station of the
  point of a phase conductor (as solved in the case run) that comes
  closest in 3D to its box (footprint × ground to roof), and the cut at
  the station of its footprint's **point nearest the axis** (a side
  parallel to the axis: its middle), optionally its centroid's
  (`centroid_cut`), each held within the building's stations; cuts of
  one alignment within `merge` (2 m) are one, labelled with all of them
  (`B007-gov+B007-near`). A building across a structure needs nothing
  more: its stations run through the structure's and the engine finds
  the span each cut crosses. **A line without an alignment gets no
  cut** (said; the places are given). Each cut reaches `cut_margin`
  (10 m) beyond the buildings it crosses (at least `min_width`, 20 m),
  `above` (5 m) over the highest wire, points `step` (1 m) apart; when
  the host's limit of 4 000 000 cut points a run would be passed, the
  step is raised and said.
- **Places of sensitive use** (the given points): the corners and the
  midpoints of the footprint's sides facing the line, and its point
  nearest the axis, at `first_height` + k × `floor_height` (1.5 m, 4.5 m,
  …) above the ground, one per floor, named `B007-F2-c1-h4.5` (building,
  floor, corner `c` / midpoint `m` / nearest point `n`, height).
  **Conservative**: the top floor's place may lie above the eaves (a
  7 m building has places at 1.5, 4.5 and 7.5 m).

By default the buildings and their ground come from the point cloud's
classes alone (the roles `building` and `ground`; owner 2026-11-02):
`building_codes` and `ground_codes` are empty. The host still hands the
plugin the survey points near the line (`survey-codes = ["*"]` in its
manifest); give codes to use them. The ground around
each building is taken from 2 m cells of the ground class (2026-11-02;
1 m cells over a real line passed the plugin's memory).

Its two presets: `buildings` (the cuts and the places) and `places` (the
places alone, quicker). Its parameters: a TOML file
(`plugins/examples/building-cuts/example-parameters.toml`; every key
optional, the defaults above). Isolines at `levels_ut` and `levels_kv_m`
(0.1, 1, 10, 100 µT; 0.1, 1, 5 kV/m); a limit (`limit_ut`, `limit_kv_m`)
gives each building's margin — **no ordinance's value is built in**.

**One run, one report** (2026-10-31): run both fields at once with
`ohl electrical fields` (above); the plugin's report then holds both
fields side by side. A single-field command (`magnetic` or `electric`)
gives the same files with that field alone.

| File | What |
|---|---|
| `buildings.csv` | a row per building: id, points, area, centroid, ground / eaves / roof elevation, floors, span, alignment, station range along it, smallest plan distance to the axis, smallest 3D distance to a phase conductor, its cuts; then **per field** (B in µT, E in kV/m) the largest value in its sections of the cuts, the largest at its places (B: also the cycle's largest), and with a limit the limit and the margin — the margin from the largest of all of them (**conservative**) |
| `points.csv` | every place: building, floor, place, x, y, z, height, then per field the value (B: and the cycle's largest) and the margin |
| `cut-LABEL-B.csv`, `cut-LABEL-E.csv` | a cut's grid per field: offset, elevation, ground, value, below the ground (1/0) |
| `cut-LABEL-B.svg`, `cut-LABEL-E.svg` | the cut drawn 1:1, one drawing per field: the ground profile along the cut (filled below it), each building's section where the cut crosses its footprint (ground to eaves, the roof above dashed) labelled with its id and its largest value, every phase conductor and earth wire crossing labelled (circuit and phase), the axis, the field's bands between the levels (light) and its isolines with a legend, elevation and offset scales, a scale bar; the title gives the alignment, the station (an angle point said) and the span |
| `buildings.geojson` | the footprints (with their summary), the cuts' lines (with alignment and station) and the places, in the line's coordinate system |
| `buildings.dxf` | with `dxf = true`: the same in plan (R12: layers BUILDINGS, CUTS, PLACES) |
| `notes.txt` | what the plugin assumed and found, each field's notes, warnings |

<a id="field-plan-templates"></a>**Field plan templates** (`--plan
FILE.ohlfp`, 2026-11-02; built, in active development, not approved
yet): how the cuts are drawn and in which weather the wires hang, kept
as a project library (kind `field-plans`) or a file:

- **Title**: lines over each drawing (`{line}`, `{cut}`, `{field}`,
  `{case}` replaced) and logos beside them (SVG, PNG or JPEG files, a
  width each).
- **What the cut shows**: the point cloud's points of chosen classes
  within a band of the cut's plane (each class its colour), and a TIN
  (a project surface, or the field's ground) or a background drawing
  of the project (its 3D lines where they cross the cut, by layer).
- **Limits and lines**: the magnetic and electric limits (the margins in
  the tables; the template has the Swiss NISV's 1 µT and 5 kV/m — give
  your ordinance's) and the isolines drawn, each its value, colour,
  width and label (added to the run's levels).
- **Wires**: the colours of the phase conductors and earth wires.
- **Weather**: the case every wire hangs in, the earth wires' own and
  each electrical system's own (by its id) — names of the `--weather`
  file. `--system-case SYSTEM=CASE` (`earth` for the wires of no system)
  overrides it per run.

A new template's text: `ohl_electrical::field_plan::TEMPLATE` (every key
with its default, commented). The parameters' `limit_ut` / `limit_kv_m`,
when given, win over the plan's.

The report is made **one cut at a time** (2026-11-02): on a real line
the whole set of results was 110 MB, more than a plugin gets by default;
now each cut is handed over alone and the tables are made from the cuts'
summaries.

A building's section in a cut takes the grid's points up to half a step
outside it (**conservative**); it is drawn from the building's ground
(one elevation, the 10th percentile around it) even where the cut's
ground under it slopes. The wires drawn are those the plugin found
crossing its own cuts (a cut given with `--cut` is drawn without them).

```text
ohl electrical fields examples/electrical/double-circuit.ohl.json \
    --library examples/electrical/wires.ohlw --current '*=1000' \
    --terrain survey.csv --cloud project.ohl \
    --preset-plugin plugins/examples/building-cuts \
    --params plugins/examples/building-cuts/example-parameters.toml --out report
```

With a plan template and a weather case per system:

```text
ohl electrical fields project.ohl --current '*=900' \
    --weather cases.ohlwc --plan plans.ohlfp --system-case earth=cold \
    --preset-plugin plugins/examples/building-cuts --out report
```

(`survey.csv`, `project.ohl`: your survey and a project with its point
cloud; the plugin is built with `cargo build --release --target
wasm32-wasip2` in its folder.)

Try it on the synthetic example:

```text
ohl electrical magnetic examples/electrical/double-circuit.ohl.json \
    --library examples/electrical/wires.ohlw \
    --current North=1000 --current South=1000:reverse --earth-wires --limit 1
```

## Electrical: electric field

*Built, in active development, not approved yet* (roadmap O18 step 5,
2026-10-29). The electric field strength (kV/m, the rms of the vector)
of the line's voltages, from the wires where they hang in a weather case.
Methods, assumptions (each marked conservative or not) and validation:
[Electric field](../theory/electric-field.md).

**What it needs:** the sections' systems and phases (Line › Electrical
systems; the circuits are the phases of a system in one named circuit;
sections of no system are earth wires, at earth potential) and the
wires' diameters. **No limit is built in**: an ordinance's values (which
voltage, which sag, which places, which limit) are yours to give.

**In OHL-Design:** Checks › Electrical › **Electric field…** Its inputs
— the weather case, the voltages, the model, the jumpers, the ground
plane, the profiles, the limit, the points (the CSV's text), the pieces,
the levels and the views — are kept in the project for the project line
worked on, written when a run starts or the window closes, as for the
magnetic field. A case or span no longer in the line falls back to the
default.

- **Voltages**: per circuit its voltage between phases (kV, rms); each
  phase at it over √3 to earth at its system's phase angle (L1 0°, L2
  −120°, L3 +120° unless the system gives others). Unless given, the
  system's **nominal** voltage (said beside it; **nominal** sets it back).
  **0 kV: voltage-free** — the circuit at earth potential, like the earth
  wires. The NISV guide (BAFU 2007, 8.7.3) asks the highest operating
  voltage, the largest sag (the weather case) and, on a double circuit
  built to compensate, one circuit voltage-free; which governs is yours to
  choose. DC systems are left out (a static field with space charge). Run
  stays greyed while every circuit is voltage-free.
- **Model**: *3D* — the wires as solved (sag, line angles, the line's
  length) and the jumpers as straight pieces of uniform charge, their
  images in the ground under each point; *2D* — infinite line charges
  where the wires cross the profile over level ground at the
  cross-section (quick; not valid near a line angle, a structure or on
  sloping ground). The charges per wire from the cross-section where they are: in 2D
  the profile's own, in 3D each span in 16 stretches (2026-10-31; until
  then from each span's mean cross-section, 3–4 % low where the wires
  hang lowest).
- **Jumpers** (3D, on by default): a jumper at a section's end carries
  that section's charge per metre; jumpers not at a section's end are
  left out and listed in the notes.
- **Structures' steel** (4 Model; 2026-10-31, **off by default**):
  *earthed (on the tiled floor)* — the structures' members at earth
  potential, solved with each floor they stand on: the field lower near
  the structures, higher at the steel. Beside it: how many structures
  have members (detail levels 2 and 3, else their own stick drawing),
  how many are left out (level 1) and how much longer a profile's floor
  beside two structures takes. Use the profiles *beside its structures*
  to see it. On the command line `--steel`.
- **Ground** (3D): *tiled floor* (the default since 2026-10-29: the
  owner, "keep the tiled implementation") — the terrain as a conducting
  floor of tiles (fine tiles of 2 m near the profile by default, 8 m
  beyond, to 200 m) solved around each profile, group of points and cut,
  some seconds each (the window says how many tiles and how long); it
  follows hollows and ridges. Or *plane under each point*, fitted over ±
  a distance (default ±10 m): quick, exact for level or evenly sloping
  ground, on a real line up to 8 % low on bumps and up to 24 % high in a
  hollow. Cuts across the line (the cross-section panels too) and volumes
  on coarser 8 m tiles by default, each a parameter (*tiles: profiles and
  points / cuts / volumes*; a volume's floor of 8 m tiles is some 3 % off
  on even ground, of 4 m 0.2 % and some 40 times as long), and how far a
  volume's floor **reaches** beyond it (100 m; 2026-11-01). A volume too
  large for one floor (over 2 GB) is floored in parts as long as that
  reach, each over the ground within it (as close to the exact field as
  one floor on level ground; see [the theory](../theory/electric-field.md));
  the window says so before Run, the results in their notes. On the command
  line `--plane`; the tiles `--tile`, `--coarse-tile`, `--floor-reach`,
  `--cut-tile`, `--volume-tile`, `--volume-margin`. Progress bars: the run's in the footer
  (saying which floor is being solved), the cross-section panels' under
  *Cross-sections* and in the results.
- **Points from CSV…**: as for the [magnetic
  field](#electrical-magnetic-field).
- **Limit**: optional, kV/m: the margin at every point and per profile.

**3 Spans and what to compute in each** (2026-10-31): the span table, the
**Profiles** and **Across** (default ±50 m every 1 m at 1 m above the
ground) and the sizes of the **2D cuts** and **Volumes**, as for the
[magnetic field](#electrical-magnetic-field) (no isolines from above
yet); the volumes' floor (tiles, memory, time) beside them. The summary
beside **Run** and the **Results** list (Plot, Open cut, Show in 3D) as
there.

**4 Model** (3D; folded): the structures' steel, **Wire pieces** *as solved* or *at most* a length, as
for the magnetic field (each piece's field exact, in closed form; the
only error is how far the wire curves away from it). Beside it the
profiles' time: every point sums every piece twice (the charges and
their images), estimated from the last run's speed, else from a short
measurement on this computer. The whole line is always modelled.

**5 Views and levels**: the **levels** (kV/m; default 1 and 5 kV/m
drawn, 10 kV/m not; display only, none is a limit; **+ the limit**),
*colour the cuts by band*, the field *also in any cross-section panel I
open* (every step, default 0.5 m; computed from the run's charges
whenever a panel opens or moves); the cuts and volumes asked in the
table (isosurfaces of the levels, one volume along each run of spans
with *Volume* ticked, ± the width, from the ground to 10 m over the
highest wire) — each with its time beside it, drawn on the
layer **Electric field (isolines, surfaces)** while the window is open,
as for the magnetic field. Not yet: isolines from above.

The results: the case and model, the **charges per wire** (per section
and span: the voltage to earth, the charge per metre in nC/m, the
frequency), the notes (what was assumed or left out); the chosen profile
plotted — E against the distance from the axis (right positive), the
limit dashed, the value under the pointer; a table per profile (station,
the largest E and where, the margin); a table of the points with E and
the margin (red when above the limit); under **Views**, per view what was
computed and how far its drawings lie off their levels (the cuts on the
alignment, as in the magnetic field's window). **Export CSV…**
writes every profile point, point and cut point. The results are marked
out of date when the line or the ground changes; the views then draw
faded. Not modelled: the structures, trees, buildings and people (earthed
objects lower the field beside them and raise it at their tops).

**On the command line:** `ohl electrical electric LINE.ohl.json|PROJECT.ohl`
with the voltages, profiles, points, cuts and volumes as options
(`ohl electrical electric --help`); as for the magnetic field, cuts are
placed at an alignment station and may carry a label (`--cut
ALIGNMENT@STATION#LABEL`), `--csv` writes every point, `--json` the
results (`ohl-field-results-set`), both fields in one run with `ohl
electrical fields` ([above](#both-fields)), and `--preset-plugin DIR
[--params FILE] [--preset NAME] [--out DIR] [--cloud SPEC]` takes a
plugin's field presets (above).

## Electrical: buildings near the line (fields)

<a id="buildings-fields"></a>Checks › Electrical › **Buildings
(fields)…** (2026-11-02, reworked 2026-11-03; built, in active
development, not approved yet): finds the buildings near the line and
**sets up the field windows** with them; the fields are run there, not
in this window (owner 2026-11-03). A plugin with field presets ([the
building-cuts plugin](#building-cuts), or your own) finds the buildings
and gives the cuts across the line at each of them and the places of
sensitive use.

- **Scope**: the whole line; the weather case the wires hang in when the
  field plan names none.
- **Survey data**:
  - the **point cloud's classes** (those in the cloud, else the class
    scheme's), each with two ticks: **buildings** and **ground**. Until
    you tick, the classes of the plugin's roles (its manifest, e.g. the
    scheme's building and ground classes) are ticked; **The plugin's
    classes** goes back to them;
  - the **survey file's codes** (its `code` column), each with the same
    two ticks; the codes ticked are handed to the plugin with its survey
    points and set its `building_codes` and `ground_codes`.
- **Presets**:
  - **Plugin** (those in use with field presets, File › Plugins ›
    Plugin manager…) and its **preset** (empty: its first).
  - **Parameters**: the plugin's own, as a form made from its template —
    each key with its value, the comment above it as its help, a key
    left out by default with a box to set it; **Reset to the plugin's**
    goes back to its template.
  - **Field plan**: a [field plan template](#field-plan-templates) of the
    project (library kind `field-plans`): its limits and weather case go
    to the field windows. **New** opens the template's text (every key
    with its default) in the window, checked as you type; **Save into
    the project** keeps it; **Edit** opens the one chosen.

**Run** (bottom left) only finds the buildings. **Results**: the
buildings found — id, area, ground, height, its cuts and places — and the
notes (what the plugin read and found). **Set up the field tools** hands
the preset's cuts and places to the **Magnetic field** and **Electric
field** windows and opens them, with the plan's limits and weather case;
each window shows them on its *Buildings* row (**Clear** removes them)
and runs them with its own cuts and points (2D runs; the preset's
isolines added). Choose the currents, voltages and everything else
there, and run. The window's setup is kept in the project for the
project line worked on, the preset in each field window's. Without a
point cloud the survey codes alone give the buildings (said).

The plugin's own report (tables, drawings, plan files) is written by
`ohl electrical fields --preset-plugin` ([above](#both-fields)); the
window no longer writes it.
