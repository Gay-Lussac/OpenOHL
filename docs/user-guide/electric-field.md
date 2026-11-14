# Electric field

*Built, in active development (not approved yet).* **Checks › Electrical ›
Electric field…** in OHL-Design. This page explains the window for a line
designer: what it answers, how it works, and its three pages — **3D
volumes**, **2D profiles**, **2D cuts** — each with its methods and every
setting it shows (default, effect, time), then what is and is not on the
safe side. The calculation in detail, with its sources and its validation:
[theory › Electric field](../theory/electric-field.md) and [Conductor
surface gradient](../theory/surface-gradient.md). The command line:
[Analyses › Electrical: electric field](analyses.md#electrical-electric-field).
The window is built as the [Magnetic field](magnetic-field.md) window.

## What it answers

**How strong is the electric field of the line's voltages near the
ground — across a span, in a vertical cut at a house, in the space around
the wires — and where does it pass a level or a limit you give?** The
field is the electric field strength *E* in kV/m, the rms of the vector,
from the wires where they hang in a weather case. On the 2D pages it also
gives the **conductor surface gradient** (kV/cm), the input of the corona
noise methods.

![The Electric field window on the example line: the profiles across each span with their largest field, and the plot of one against a limit](../images/user-guide/electric-field-window.png)

*The window's results: a profile per span and the plot of one.*

**No limit and no voltage table are built in.** Which voltage, which sag,
which places and which limit an ordinance asks for are yours to give; the
window computes the field and, when you give a limit, the margin to it.

## How it works, step by step

### 1. The wires in a weather case

As in the magnetic field: the line solved in the case you choose (**Wires
by** another calculation if you choose, e.g. *Max sag*: the lowest wires,
the highest field at the ground), each electrical system and the earth
wires optionally in a case of their own.

### 2. The voltages and the charges

Each circuit is at a voltage between phases (kV, rms): by default its
system's **highest operating voltage** (Line › Electrical systems, column
**max kV**, your value), else its nominal voltage — lower, so **not on the
safe side**, and said. From the voltages, the charge per metre of every
wire follows at each cross-section along the span, where the wire really
hangs (each span in 24 stretches).

![Two wires above a perfectly conducting ground and their images below it; the potential coefficients between the wires and to the images give the charges from the voltages](../images/theory/e-images.svg)

*The charges from the voltages, with their images in the ground.*

![A span between two structures in stretches; each stretch at the charge per metre of the cross-section at its middle, larger in the middle of the span where the wire hangs lower](../images/theory/e-stretches.svg)

*Each span in stretches, each at its own charge.*

### 3. The ground (3D model)

The ground is a conductor at earth potential. By default it is a **tiled
floor** laid on the terrain around each profile, cut and volume, its
tiles' charges solved so that it stays at zero potential: it follows
hollows and ridges. Quicker, a **plane under each point**, fitted over a
short distance, exact on level or evenly sloping ground. Optionally the
structures' steel stands on the floor at earth potential.

![Left: in a hollow, the plane fitted under the point and the charge's image in it; the hollow's sides are not seen. Right: the tiled floor follows the terrain and carries induced charges](../images/theory/e-ground-models.svg)

*The plane under each point against the tiled floor.*

![In plan: fine tiles near the profile, coarse tiles beyond. In elevation: the tiles on the terrain with induced charges, a structure's steel cut into pieces, all at zero potential](../images/theory/e-floor.svg)

*The tiled floor: fine tiles near the profile, coarse beyond.*

### 4. The field at every point: two methods

**From the 3D model** (volumes, cuts, profiles): every wire in straight
pieces of uniform charge (as solved: 64 per span), the jumpers too; at
every point every piece, every tile (or image) and every piece of steel
adds its field, exactly. The whole line is always modelled.

**2D only** (profiles): every wire an infinitely long, straight line
charge where it crosses the profile, over level ground at the
cross-section (the charges' images in it) — the classic cross-section
method. No sag along the span, no line angle, no jumpers, no structures,
no terrain.

### 5. The results and the drawings

Every value at a point is computed in full; only the lines and surfaces
between the points are interpolated, and each run says how far they lie
off their levels.

![Three ways to see the field: a profile, a cut with isolines, a volume with surfaces around the wires](../images/theory/field-views.svg)

## The window

As in the [magnetic field](magnetic-field.md#the-window): three tabs —
**3D volumes**, **2D profiles**, **2D cuts** (owner, 2026-11-12) — each a
page with only the settings its output needs, in numbered sections (1
What it computes, 2 Scope, 3 What and where, 4 Model folded, 5 Views and
levels on the volumes and cuts). **Run computes the page shown.** The
inputs shared by the pages are one set of values. The inputs are kept in
the project for the line worked on, with the page shown, written when a
run starts or the window closes; a project kept before the pages opens on
the page that made what it asked. Each heading's **user guide ↗** opens
its page's part below. A setting keeps its label (units in brackets), its
value and at most one short line; what it does is said on hover and
behind its **(?)**; warnings, what is not on the safe side and why Run is
greyed stay in view.

### Scope (every page)

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| **Weather case**, **Wires by**, **Per electrical system**, **Ground from** | as in the [magnetic field](magnetic-field.md#scope-every-page) | where the wires hang and the ground they stand over | a solve per case |
| **Voltage between phases [kV]**, per circuit | the system's highest operating voltage (*highest*), else its nominal (*the system's nominal — not on the safe side*, in the warning colour) | the field grows in proportion; **highest** / **nominal** beside a typed value sets it back; **0 kV**: voltage-free (at earth potential, like the earth wires); DC systems are left out (said) | none |
| **Limit [kV/m]** | none | the margin at every point | none |

Run is greyed out, with the reason, while every circuit is voltage-free,
while the page has nothing to compute (no span ticked, no cut asked, no
volume ticked), and **when the page's floor would need more memory than
one calculation may take**: the memory is a setting of the application,
Line › Calculation options… › *Memory one calculation may take* (2 GB by
default, offered up to three quarters of this computer's memory).

### The footer and the results

Beside **Run**, one line says what Run will compute on the page shown and
about how long (the floors counted): *Run: 2D profiles, 4 profile(s), from
the 3D model — about 20 s*; when Run is greyed out, why. While it runs:
the step, which floor is being solved, a bar, the time left; **Stop**.

**Results**: the list of everything computed with **Show plot**, **Open
cut**, **Show in 3D** and a **3D** tick each; the case and method (a
2D-only run says its assumptions); the **charges per wire** (per section
and span: the voltage to earth, nC/m, the frequency); the notes; the
chosen profile plotted (E against the distance from the axis, the limit
dashed); a table per profile and of the points with their margin; per
view how far its drawings lie off their levels; the **Conductor surface
gradient, 2D** per cross-section and bundle (the average maximum, the
maximum and Markt–Mengele's value, kV/cm rms; per subconductor in its
CSV). **Export CSV…**, **Save in Performed checks**. The results are
marked out of date when the line or the ground changes. Drawn on the layer
**Electric field (isolines, surfaces)** while the window is open.

**A cut or a step that fails** (2026-11-14) does not lose the rest. Each
cut is computed on its own: one that cannot be computed (its span not
found or off the alignment, a wire not connected) is listed at the top of
the notes in red — *NOT COMPUTED: 1 of 12 cut(s) failed … — cut House 7:
why* — and every other cut keeps its results. The same for a whole step
(the profiles, the cuts, the volumes run one after another): a step that
fails is said in red and the steps that worked are shown. Only **Stop**
ends the run. A run where nothing could be computed gives its errors.
On the command line (`ohl electrical`), the cuts that worked are written
and the command then ends with the failed cuts as its error (a non-zero
exit), so that a script notices.

## 3D volumes

**What it gives:** the surfaces of the levels (1 and 5 kV/m by default)
around the wires over each run of spans with **Volume** ticked, drawn in
the 3D view (**Show in 3D**); the largest value in each volume.

**How it works:** from the 3D model only, over a floor of its own tiles
along the volume's whole run and beyond it. A volume too large for one
floor (over the memory one calculation may take) is floored in parts,
each over the ground within its reach.

![A long volume along its run in four parts, two parts sharing the plane where they meet; each part's floor over the ground within the margin of it](../images/theory/field-parts.svg)

*A long volume floored in parts.*

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| Scope | see [Scope](#scope-every-page) | | |
| **Volume** column of the span table | none ticked | the spans a volume covers | in proportion to the spans |
| **Volume size ± [m]**, **every [m]** | ±50 m every 2 m | wider: a low level's surface inside the box; a finer step places the surfaces closer to their levels; beside it the floor: its tiles, its memory, its time, how many parts, in red when it would not fit | said beside it |
| **Jumpers** (4 Model) | on | off: left out (not on the safe side near the structures) | none |
| **Stretches per span** (4 Model) | 24 | fewer: the charge follows the sag less closely (measured beside it and in the [theory](../theory/electric-field.md#the-charges)) | hardly changes |
| **Ground** (4 Model) | tiled floor | *plane under each point*, fitted over **± [m]** (10 m): quick, up to 8 % low on bumps (not on the safe side) and 24 % high in a hollow on a real line | the tiled floor: some seconds per floor, said beside it |
| **tiles: volumes [m]**, **reaching [m]** (4 Model) | 8 m, 100 m | finer tiles: a closer floor (8 m some 3 % off on even ground, 4 m 0.5 %); the reach of the floor beyond the volume | 4 m some 20 times as long as 8 m; the tile cannot be set below what fits the memory |
| **Structures' steel** (4 Model) | off | *earthed (on the tiled floor)*: the members at earth potential, the field lower near the structures, higher at the steel; structures of detail level 1 left out (counted) | longer floors, said beside it |
| **Wire pieces** (4 Model) | as solved | *at most [m]*: shorter pieces placed on the curve; the field changes by about the wire's offset over the distance to it | in proportion to the pieces |
| **Levels drawn [kV/m]** (5) | 1 and 5 kV/m drawn, 10 kV/m not | the surfaces drawn; display only, none is a limit. **+ level**, **+ the limit** | more levels, more surfaces |

## 2D profiles

**What it gives:** E across each span ticked, at a height above the
ground, plotted against the distance from the axis and listed; E at the
points of a CSV file; and, ticked, the conductor surface gradient.

**How it works:** a row of points square to the span's axis, at the
span's lowest point by default, computed by the method chosen.

### The methods compared

| | From the 3D model (default) | 2D only |
|---|---|---|
| Assumes | the wires and jumpers as solved (sag, line angles, the line's length), each span in stretches at its own charge, over the ground (a tiled floor on the terrain, or a plane under each point), the steel optional | infinitely long, straight, parallel line charges where each wire crosses the profile, over level ground at the cross-section; no sag along the span, no angle, no jumpers, no structures, no terrain |
| Enough when | always (the reference); the terrain counts | long straight spans over level ground away from structures and angles; under the lowest point close to 3D on a long level span |
| Not valid | — | near a line angle, a structure or on sloping ground (warned) |
| Time | the floors and the points: said beside them | a moment |
| Its settings | jumpers, stretches, ground and its tiles, steel, wire pieces | none (4 Model says so) |

The 2D-only method's assumptions are said under its choice and above its
results.

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| Scope | see [Scope](#scope-every-page) | | |
| **Points from CSV…** | none | the field at your places (as in the magnetic field) | a point each |
| **Method** | from the 3D model | see the comparison above | see above |
| **Profile** column of the span table, **Profiles** | each ticked span's lowest point | *beside its structures* or *one span* at a place along it | one profile each |
| **Across ± [m]**, **every [m]**, **at [m] above the ground** | ±50 m every 1 m at 1 m | wider: the low levels' reach; a finer step: more points; higher: nearer the wires | in proportion to the points |
| **Surface gradient** | on | the gradient at the conductors' surfaces in each profiled span, at its lowest point and both ends (2D, whatever the method); changes no field value | some milliseconds per cross-section |
| **Jumpers**, **Stretches**, **Ground**, **Structures' steel**, **Wire pieces** (4 Model, from the 3D model) | as on the volumes | as on the volumes | as on the volumes |
| **tiles: profiles and points [m]** (4 Model) | 2 m | finer: a floor closer to the terrain (4 m to 2 m moved the profiles 1–3 %) | 1 m about 8 times as long |

## 2D cuts

**What it gives:** the isolines of the levels in vertical planes across
the line at the stations asked in the span table, with **Open cut**
showing each on the wires; the cuts and places handed over by the
**Buildings** window; and, ticked, the conductor surface gradient.

**How it works:** a lattice of points in the vertical plane at the
alignment station, perpendicular to the alignment (at an angle point
along the bisector), as in the magnetic field, over a floor of the cuts'
own tiles.

### The methods

| | From the 3D model | 2D only |
|---|---|---|
| Assumes | the wires and jumpers as solved over the ground, at every point of the cut | — |
| Enough when | always | **not available yet**: the engine computes cuts from the 3D model only (shown greyed in the window) |
| Time | said beside the cuts' sizes | — |

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| Scope | see [Scope](#scope-every-page) | | |
| **Buildings** | none | the cuts and places a buildings preset handed over (its places computed with the cuts, from the 3D model) | as their cuts |
| **2D cuts at** column of the span table | none | the cuts asked | one cut each |
| **Cut size ± [m]**, **every [m]**, **up to [m] over the highest wire** | ±60 m every 1 m, 10 m | the cut's size; a finer step draws the isolines closer to their levels | said beside it |
| **Surface gradient** | on | as on the profiles (the spans ticked) | some milliseconds per cross-section |
| **Jumpers**, **Stretches**, **Ground**, **Structures' steel**, **Wire pieces** (4 Model) | as on the volumes | as on the volumes | as on the volumes |
| **tiles: cuts [m]** (4 Model) | 8 m | finer: a closer floor | far longer |
| **Levels drawn [kV/m]** (5) | 1 and 5 kV/m drawn, 10 kV/m not | the isolines drawn; display only | none |
| **colour the cuts by band** (5) | off | a cut's cells in the colour of the highest level at or below their value | none |
| **Cross-sections: also in any cross-section panel I open, every [m]** (5) | off (1 m when ticked) | the field in every cross-section panel you open, from the run's charges | said beside it |

## What is conservative and what is not

From the theory's [choices](../theory/electric-field.md#the-choices-and-why)
and [what is left out](../theory/electric-field.md#what-is-left-out):

- **The nominal voltage** (when neither a voltage for the run nor the
  system's highest operating voltage is given): **NOT ON THE SAFE SIDE**,
  said in the window and in the result with the circuits concerned. The
  surface gradient the same.
- **2D only**: infinite straight conductors over level ground; near a
  line angle, a structure or on sloping ground not valid (warned).
- **The plane under each point**: up to 8 % low on bumps (not on the safe
  side); the tiled floor follows the terrain.
- **The structures' steel off** (the default): on the safe side beside a
  structure, not at the steel.
- **Jumpers off**: not on the safe side near the structures.
- **Trees, buildings and people** are not modelled: on the safe side away
  from them, not at their tops.
- **DC systems** are left out (said).

## Without the window

`ohl electrical electric` runs the same calculation from the command line,
`ohl electrical gradient` the surface gradient, `ohl electrical fields`
both fields at once: [Analyses › Electrical: electric
field](analyses.md#electrical-electric-field).
