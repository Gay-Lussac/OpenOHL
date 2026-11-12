# Magnetic field

*Built, in active development (not approved yet).* **Checks › Electrical ›
Magnetic field…** in OHL-Design. This page explains the window for a line
designer: what it answers, how it works, every setting with its default,
its effect and its time, and what is and is not on the safe side. The
calculation in detail, with its sources and its validation:
[theory › Magnetic field](../theory/magnetic-field.md). The command line,
the plugins that place cuts at buildings and the plan templates:
[Analyses › Electrical: magnetic field](analyses.md#electrical-magnetic-field).

## What it answers

**How strong is the magnetic field of the line's currents around the
line — across a span, in a vertical cut at a house, in the space around
the wires — and where does it pass a level or a limit you give?** The
field is the magnetic flux density *B* in µT, the rms of the vector, from
the wires where they hang in a weather case.

![The 3D view of the example line's first span with the magnetic field's 1 µT surface around its wires, as the Magnetic field window draws it](../images/user-guide/magnetic-field-3d.png)

*The 1 µT surface around the wires of one span, drawn by the window in
the 3D view.*

**No limit is built in.** Which current, which places and which limit an
ordinance asks for are yours to give (or come from a plugin's parameter
file); the window computes the field and, when you give a limit, the
margin to it.

## How it works, step by step

### 1. The wires in a weather case

The line is solved in the weather case you choose, each section by its
own calculation unless you choose another (**Wires by**: e.g. *Max sag*,
the lowest wires and so the highest field at the ground). Each electrical
system, and the earth wires, may take a case of its own: the conductors
at their load's temperature, the earth wires at the air's.

### 2. The currents

Each circuit carries a current (A, rms, each phase), by default its
system's **operating current** from Line › Electrical systems. The three
phases are balanced at their system's phase angles (L1 0°, L2 −120°,
L3 +120°), unless you give each phase its own current and angle.

![Left: the three phase currents as phasors at 0, −120 and +120 degrees. Middle: at a point every wire adds its field per component as a phasor; B is the rms of the vector. Right: the field vector traces an ellipse over a cycle; its semi-major axis is B_max](../images/theory/m-phasors.svg)

*The phase currents as phasors; at a point every wire adds its field; B
is the rms of the vector, B max the largest over a cycle.*

### 3. The earth wires and the jumpers

If you tick it, the earth wires carry the currents the phases induce in
them, led into the ground at every structure through its earthing
resistance. The jumpers at the structures carry their phase's current.

![Four structures and three spans: the earth wire bonded at every structure, each structure leading current into the ground through its earthing resistance; in each span the earth wire current driven by the phases](../images/theory/m-earth-network.svg)

*The earth wires as a network over the line, each structure leading
current into the ground.*

### 4. The field at every point

Every wire is cut into straight pieces (as solved: 64 per span). At every
point asked, every piece adds its field, exactly (Biot–Savart in closed
form). For cuts and volumes, only the wires within a distance of the cut
or volume are taken (2 000 m by default); each run says how much the
wires left out change the field.

![A sagging wire and the chords between the solver's points, the sagitta between a chord and the wire; one piece cut into three, the new points on the curve](../images/theory/field-pieces.svg)

*The wire in straight pieces: the only error of a piece is how far the
wire curves away from it.*

![In plan, five spans and a cut across the middle one; the wires within d of the cut summed, those beyond left out](../images/theory/field-cutoff.svg)

*The wires modelled: those within the distance of a cut or volume.*

### 5. Where: profiles, cuts and volumes

![Three ways to see the field: a profile, a row of points at a height above the ground with the field plotted along it; a cut, a grid of points in a vertical plane with isolines; a volume, a lattice in a box with surfaces around the wires](../images/theory/field-views.svg)

- A **profile**: a row of points across a span at a height above the
  ground (1 m), plotted.
- A **2D cut**: a vertical plane across the line at a station of the
  alignment, its points every step, the isolines of the levels drawn.
- A **volume**: a lattice in a box over a run of spans, the surfaces of
  the levels drawn in the 3D view.
- **Points from CSV**: places you give (e.g. places of sensitive use).

![In plan: an alignment with an angle point; a cut at a station perpendicular to the alignment, a cut at the angle point along the bisector, a profile square to its span's axis](../images/theory/field-cut-plane.svg)

*A cut lies on the alignment, perpendicular to it (at an angle point
along the bisector), exactly where a cross-section panel opened at that
station lies.*

![A double-circuit line in a vertical cut across it with a house beside it: the grid's points, the isolines of 3, 10 and 30 microtesla cut at the ground](../images/theory/field-cut.svg)

*A cut at a house: the isolines between the grid's points.*

### 6. The results and the drawings

Every value at a point is computed in full; only the lines and surfaces
drawn between the points are interpolated. Each run measures how far its
drawings lie off their levels and says so.

![Left: a coarse grid around a wire, the isoline drawn straight between its crossings, cutting the circle's corners; right: a fine grid following the circle](../images/theory/field-grid-step.svg)

*A coarse step draws an isoline near a wire too small; a finer step
follows it.*

## The window, setting by setting

The Setup tab, top to bottom. The window's inputs are kept in the project
for the line worked on: it opens as it was left, for every user of the
project. A case or span no longer in the line falls back to the default.
Each heading's **user guide ↗** opens its section of this page.

### 1 What it checks

The question in one line; the method behind the **(?)**; two drawings:
a profile across the line (they follow the width, step and height you
type) and the sources of the field.

### 2 Scope

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| **Weather case** | the case shown in the views (then the one kept) | where the wires hang: lower wires, a higher field at the ground | each case not solved yet is solved for the run |
| **Wires by** | each section's own calculation | another (e.g. *Max sag*) solves the case for the run with it | a solve per case |
| **Per electrical system** (folded) | *as above* | a system's (or the earth wires') own case and calculation | a solve per extra case |
| **Ground from** | the surface of the checks | the ground the profiles' height and the cuts' and volumes' bottom stand on: a terrain surface (TIN layer) or the survey's ground points within 250 m of the spans | the survey's points: some seconds to read |
| **Currents**: per circuit, **given** | *the circuit* | *the circuit*: one current, balanced phases; *at its end points*: the current in (or out) at each end, every section's current follows; *per phase*: each phase's current and angle, an unbalanced circuit | none |
| **current [A]** | the system's operating current (*the line's*); ↺ brings it back | the field grows in proportion | none |
| **load flow** | along the line | *against the line*: every phase turned by 180°; which combination governs is the ordinance's question | none |
| **Earth wire currents typed** (folded) | none | an earth wire's own current and angle instead of a computed one | none |
| **Profiles by** | 3D | 2D: infinite straight conductors (tick *wires at the span's mean height* to take the line parameters' mean); not valid near a line angle over 5° or between differently arranged structures (warned) | 2D is quicker |
| **Earth wires** | off | *carry the currents the phases induce*: may raise or lower the field; *through the structures' earthing* (on): each structure's earthing resistance (Structures table, else the line's, else 10 Ω, said) | a network solve |
| **Jumpers** | on | off: left out (not on the safe side near the structures) | none |
| **Limit [µT]** | none | the margin at every point and the limit line's extent on every profile | none |
| **Points from CSV…** | none | the field at your places: a CSV with a header naming `x`, `y` and `z` (elevation) or `height` (above the ground), optionally `name` | a point each |
| **Buildings** (from the Buildings window) | none | the cuts and places a buildings preset handed over | as their cuts |

Run is greyed out, with the reason, while no circuit carries a current.

### 3 Spans and what to compute in each

**Make**: **2D: cuts and profiles** (the default; **profiles too**
ticked) or **3D: volumes**. Then the span table: every span a row, the
spans of each alignment first in station order and ticked, the others
unticked. Its columns: the tick, the span, its length, **2D cuts at**
(*+ station*: at mid-span, at its lowest point or a distance from its
first structure; a click on a chip removes it) and **Volume** (3D).
**Press on a tick and drag across the rows** to flip them; shift-click
ticks a range; a filter, *All*, *None* and *Invert* act on the rows
shown. A span on no alignment can have no cut (said under the table).

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| **Profiles** | at each ticked span's lowest point | *beside its structures* (10 m into the span from each), or *one span* at its lowest point or a place 0–1 along it | one profile each |
| **Across ± [m]**, **every [m]**, **at [m] above the ground** | ±100 m every 1 m at 1 m | wider: the low levels' reach inside the profile; a finer step: more points | in proportion to the points |
| **2D cuts ± [m]**, **every [m]**, **up to [m] over the highest wire** | ±100 m every 1 m, 10 m | the cut's size; a finer step draws the isolines closer to their levels (1 m: the 1 and 3 µT isolines within 0.03 %) | said beside it: points per cut and seconds |
| **Volumes ± [m]**, **every [m]** | ±100 m every 2 m | a finer step places the surfaces closer to their levels (2 m: 1 µT within 0.05 %, 100 µT within 15 %; 1 m: 0.01 % and 4 %) | said beside it; halving the step takes 4 to 8 times as long |

### 4 Model (folded)

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| **Wire pieces** | as solved (64 per span) | *at most [m]*: shorter pieces placed on the curve; the field changes by about the wire's offset over the distance to it (a few hundredths of a per cent at 10 m) | in proportion to the pieces; said beside it |
| **Modelled: wires within [m] of a cut or volume** | 2 000 m | larger: closer to the whole line's field (longer than the line: every wire); each run says what the wires left out change. Profiles and points always take the whole line | in proportion to the wires taken |

### 5 Views and levels

| Setting | Default | What changes | Time |
|---|---|---|---|
| **Levels drawn [µT]** | 1 and 10 µT drawn, 100 µT not | the isolines and surfaces drawn; display only, none is a limit. **+ level**, **+ the limit** | more levels, more surfaces |
| **colour the cuts by band** | off | a cut's cells in the colour of the highest level at or below their value | none |
| **Cross-sections: also in any cross-section panel I open, every [m]** | off (1 m when ticked) | the field in every cross-section panel you open (C), from the run's wires | some seconds per panel, said beside it |

The views are drawn on the layer **Magnetic field (isolines, surfaces)**
while the window is open.

### The footer

- Before **Run**, one line says what a run will compute and about how
  long: *2D · 4 span(s) · 2 cut(s) · 4 profile(s) — about 6 s*; the
  times come from the last run's speed, else from a short measurement on
  this computer. When Run is greyed out, the footer says why.
- While it runs: the step (*step 3 of 7: volume A–B: fine lattice*), a
  bar, the time gone and left; **Stop**.

### The results tab

The list of everything computed — every profile, cut, volume and grid with
its largest value — with **Show plot**, **Open cut** (a cross-section
panel on the very plane computed) and **Show in 3D**, and a **3D** tick to
draw each or not. Then the currents used, the earth wires' and
structures' currents, the notes (what was assumed or left out), the
chosen profile plotted (B against the distance from the axis, the cycle's
largest fainter, the limit dashed), a table per profile and a table of
the points with their margin (red above the limit), and per view how far
its drawings lie off their levels. **Export CSV…** writes every point;
**Save in Performed checks** keeps the run. The results are marked out
of date when the line or the ground changes.

## What is conservative and what is not

From the theory's [choices](../theory/magnetic-field.md#the-choices-and-why)
and [what is left out](../theory/magnetic-field.md#what-is-left-out):

- **Balanced currents** (the default) hold no net current: the result
  says **NOT ON THE SAFE SIDE far from the line**. Give the currents per
  phase to include one.
- **2D** at the wires' place under the lowest point is on the safe side
  against 3D on sagging spans; at the span's mean height it is not.
- **Jumpers off**: not on the safe side near the structures.
- **Earth wire currents** may raise or lower the field: run with and
  without them when it matters. Through the structures' earthing, the
  line's ends are not on the safe side (no substation earthing).
- **Wires within** a distance: the wires left out change the field; each
  run says by how much.
- Left out: the earth's currents, the steel's permeability, harmonics,
  other lines (said), DC systems (said) — small or said in the result.

## Without the window

`ohl electrical magnetic` runs the same calculation from the command line
(`ohl electrical fields` runs both fields at once):
[Analyses › Electrical: magnetic field](analyses.md#electrical-magnetic-field).
