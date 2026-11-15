# Galloping ellipses

*Built, in active development (not approved yet).* **Checks › Galloping
ellipses…** in OHL-Design. This page explains the window for a line
designer: what it answers, how it works, every setting and every field
of the parameter file. The calculation in detail, with its sources:
[theory › Galloping ellipses](../theory/galloping.md). The other checks
of the wires against each other are in [Wire clearances](wire-clearances.md)
and [Distance between conductors](distance-between-conductors.md); the
command line and the other analyses in [Analyses](analyses.md).

## What it answers

**When the iced wires of a span gallop, do the spaces they may sweep —
their galloping ellipses — keep apart, by the distance your method
requires?** Galloping is a slow, large oscillation of iced wires in
wind; design methods draw around each wire an ellipse sized from its
sag and ask that the ellipses of two wires of the same span (two phases,
two circuits, a phase and an earth wire) do not touch, or keep a small
distance. The check draws them for every span and every method of your
parameter file, in the weather case you choose, and gives the smallest
distance between every pair of ellipses.

![Two wires of one span side by side with their galloping ellipses, each drawn as a circumscribed polygon; the clearance is the distance between the polygons less each wire's outer radius](../images/theory/galloping-gap.svg)

*The clearance between the ellipses of two wires: the distance between
them less each wire's outer radius.*

**OpenOHL ships no galloping method.** Every coefficient comes from your
parameter file (`.ohlg`), copied from the method you design to (the
published ones are named in the [theory](../theory/galloping.md)); the
example shipped with OpenOHL (`examples/synthetic-line/galloping-example.ohlg`)
holds invented numbers that only show the form.

## How it works, step by step

![The check in five steps: the weather case solved and each span's sag; an ellipse around each wire by each method; the ellipses along the span at the stations; the pairs of wires between the same two structures and the distance between their ellipses; the smallest per pair and method against the requirement](../images/theory/galloping-steps.svg)

### 1. The weather case and the solve

The check runs in **one weather case**, the one chosen in *2 Scope*
(the case shown in the views when the window opens). The line's wires
are taken as solved in that case — each section by its own calculation,
or by the parameter file's `calculation` when it gives one (then the case
is solved again for the check). With **Use local conditions** ticked (in
the footer) the case is solved again on the line with the structures'
local wind for strength and the wire ice on their spans. A section that
does not solve in the case is named in the notes and left out.

The parameter file may name the weather cases its wires gallop in
(`cases`). The window says before Run when the case chosen is not one of
them, and runs anyway (the notes say it again); the command line runs
only the cases the file names.

### 2. The sag of each span

![A span from the side: the chord between the clamps, the horizontal span length L and the sag D; across the span at mid-span the wire at rest and its ellipse](../images/theory/galloping-sag.svg)

Each span's **sag D** is the largest vertical distance of the wire below
the chord between its clamps, as solved in the case; **L** is the span's
horizontal length; **d** the diameter of its wire type in the wire
library (a subconductor's for a bundle; the bare wire's, never the iced
one); **n** the method's number of loops. In a case with wind the wire
is swung sideways and D is its **vertical** sag, smaller than the sag in
the swung wire's plane: said in the results (see [below](#what-is-conservative-and-what-is-not)).

### 3. The ellipse of each wire

![Across the span: the wire at rest, the ellipse's major axis A tilted by theta from the vertical, its minor axis B and its centre c A above the wire](../images/theory/galloping-ellipse.svg)

Each **method** of the file draws one ellipse around every wire:

- the **major axis A** (its full length) is the sum of the method's
  terms: a factor times D, times L, times √D, times d, a logarithmic
  term and a constant ([every term](#the-axes-terms));
- the **minor axis B** the same, plus a factor times A (B = 0.4 A, say);
- the major axis is **tilted** θ from the vertical, its top to the
  right looking from the span's first structure to its last;
- the ellipse's **centre** lies on the major axis c A above the wire at
  rest (`centre_above_rest = c`; 0.5 puts the wire at the ellipse's
  lower end, 0 at its centre).

### 4. Along the span: loops and stations

![A span from the side with two loops: the ellipse's size scaled by |sin(n pi t)| along the span, nothing at the supports, and the inner stations](../images/theory/galloping-loops.svg)

A wire galloping in n loops moves most in the middle of each loop and
not at all at the supports. With `along = "sine"` (the default) the
ellipse at the fraction t of the span is scaled by |sin(nπt)| — axes and
centre offset together; with `along = "constant"` it keeps its full
size all along. The ellipses are compared at **stations − 1 inner
points** (the file's `stations`, default 20: 19 points, every 5 % of the
span). Where the middle of a loop is not a station (3 or 4 loops with
20 stations, say) the largest ellipse is not compared: the results say
so (*NOT ON THE SAFE SIDE*, with how much smaller) and the fix — a
number of stations that is a multiple of 2 n.

### 5. The pairs

Every two wires **hanging between the same two structures** are paired:
the phases of a circuit, the circuits on the same structures, a phase
and an earth wire, two earth wires. A bundle is one wire (its centre
line). Jumpers are not paired; spans of adjacent structures, or of
another line, are not compared ([why](#what-is-conservative-and-what-is-not)).
A span carrying a clamp a jumper hangs on (a tap) is paired as any other
(since 2026-11-15: before, it was left out without a word).

### 6. The clearance between two ellipses

At each station both ellipses are drawn in the plane square to the first
wire's span, each around its own wire at rest there, and their distance
is measured: **0 when they overlap**, else the shortest distance between
them, less each wire's **outer radius** (its bundle radius, its
conductor's radius — iced in a case with ice — and how far its computed
line may lie off the true curve). The ellipses are measured as polygons
of 72 sides drawn around them, never inside: the distance found is never
more than the true one (at most 0.1 % of A less).

### 7. The requirement

The file's `required` is the distance two ellipses must keep: one
distance (`"0.3 m"`), a table by voltage, or a base plus an amount per kV
— the forms of the [rule files](terrain.md#clearance). A requirement by
voltage is read at the pair's voltage (`pair_voltage`: the **higher** of
the two wires' nominal voltages by default, their **sum**, or the
**difference** — the voltage between the two conductors). Two earth
wires are read at 0 kV; a phase and an earth wire at the phase's voltage
(higher, sum) or its voltage to earth (difference). A pair above the
table's last row takes the last row's distance, said in the results
(*NOT ON THE SAFE SIDE*). Without `required` the check gives the
distances only.

### 8. The results

The smallest distance of each pair and method, along the span, is one
row; the rows **worst first** (the smallest margin to the requirement
first). A row clicked selects the first wire's span and draws it in the
weather case of the run.

## The window, setting by setting

The Setup tab, top to bottom. The weather case, the scope and **Use
local conditions** are kept in the project, per project line
(`app.galloping`); the parameter file is the project's first `.ohlg` (or
the one next to the line file) until another is chosen. A change of the
line, its weather cases or its library files marks the results *out of
date*. Each section and field explains itself in its **(?)** or on
hover; only warnings, *NOT ON THE SAFE SIDE* notes, what blocks **Run**
and the time stay written. Each section's **user guide ↗** opens its part
of this page.

### 1 What it checks

The question in one line with its (?), and **How it is computed**
folded below it (the method in a paragraph and the drawing: two wires,
their ellipses with the axes A and B from the sag, and the clearance
between them).

### 2 Scope

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| **Weather case** | the case shown in the views when the window opens | the sags the ellipses are sized from and the wires' positions at rest | none: the case is solved already (unless Use local conditions or the file's calculation solves it again) |
| **Circuits** | every circuit (also those added later) | only the wires of the circuits ticked are paired; none ticked: Run greyed | fewer wires: a little faster |
| **Routes**, **alignments**, **sections** | none ticked: the whole line | only the sections ticked, or with a structure on a route or alignment ticked | fewer spans: a little faster |

When the parameter file names its weather cases and the case chosen is
not one of them, a warning says so under the scope (Run is not greyed).

### 3 Rules: the parameter file

- **Parameter file** and **Choose…**: the `.ohlg` file the check reads.
  No file, or a file that cannot be read: Run is greyed and the footer
  says why; the reason the file was refused is written in the error
  colour.
- Under it, read from the file: its **name**, its **requirement** in
  words ("required 0.30 m", "by voltage: …", or *no requirement:
  distances only*), the **cases** it names, and a table of its
  **methods** — name, loops, the major and minor axes in words
  (`A = 1.2 D + 0.3 m`, `B = 0.4 A`).

The parameter file is edited outside the window (a text file: see
[every field](#every-field-of-the-parameter-file)); the window reads it
again when it changes on disk.

### 4 Model: from the parameter file (folded)

What the file sets about how the check is computed, read-only, with its
effect:

| Shown | From the file | What changes in the result | Time |
|---|---|---|---|
| **Stations** | `stations` (default 20) | the ellipses compared at stations − 1 inner points; more stations, the smallest gap found more closely; a number that is not a multiple of 2 n misses a loop's middle (said) | in proportion to the stations; measured 2026-11-15: the example line (10 spans, 2 methods, 20 stations) 0.1 s for the whole run in the window |
| **Calculation** | `calculation` (default: each section's own) | the wires solved by that condition and method (e.g. the final after load) for the sags and positions | one more solve of the line in the case |
| **Pair's voltage** | `pair_voltage` (default higher) | the voltage a requirement by voltage is read at | none |
| per method: **tilt**, **centre above the wire**, **along** | `tilt`, `centre_above_rest`, `along` | where the ellipse sits around the wire and how its size varies along the span | none |

### The footer

- **Use local conditions**: the case solved again on the line with the
  structures' local wind for strength and wire ice on their spans
  ([Local conditions](weather.md#local-conditions)); its time, measured,
  is said beside it.
- Before **Run**, one line says what a run computes ("3 sections (12
  spans), the ellipses of each span") and about how long it takes, from
  the window's last run on this computer; or **Before Run: …** with what
  blocks it: no weather case, no parameter file, a file that cannot be
  read, no circuit in scope.
- **Run** and **Stop**: the check runs in the background; Stop keeps the
  previous results. A run stops after five minutes at the latest.

### The results

The **verdict** first: red with the number of pairs × methods below the
requirement, green when every pair keeps it, or — without a requirement
in the file — the smallest distance found. Then the **notes** (folded
above the table: a case the file does not name, sections not solved,
Use local conditions, and every *NOT ON THE SAFE SIDE* note), then the
table, one row per pair and method:

| Column | Meaning |
|---|---|
| **Method** | the method's name in the file |
| **Span**, **Other span** | the two wires' spans (section and span, named from 1) |
| **Distance [m]** | the smallest distance between the two ellipses along the span, less both wires' outer radii; 0: they overlap; red below the requirement |
| **Required [m]** | the requirement at the pair's voltage (empty: none) |
| **At [%]** | where along the first wire's span (0 % at its first structure) |
| **Ellipse [m]**, **Other ellipse [m]** | each wire's ellipse at its largest, A × B |

**Only violations** (on by default), **Search**, **Group by**, **Export
CSV…** (the rows shown) and **Full report (CSV)…** (every pair, with the
weather case, the spans' 0-based indices and where along the span as a
fraction) as in every check window
([Check windows](desktop-app.md#check-windows-one-design)).

## Every field of the parameter file

A galloping parameter file (`.ohlg`) is a TOML text file. Its top level:

| Field | Unit | Default | Meaning |
|---|---|---|---|
| `format` | — | `"ohl-galloping"` | the file's kind; another is refused |
| `version` | — | 1 | the format's version; a newer one than this program reads is refused |
| `name` | — | empty | the file's name in the window and the output |
| `required` | m | none: distances only | the distance two ellipses must keep: `"0.3 m"`, `{ by_voltage = [[110, "0.3 m"], [245, "0.5 m"]] }` (up to kV, distance) or `{ base = "0.2 m", per_kv = "0.002 m", above_kv = 50 }` |
| `pair_voltage` | — | `"higher"` | the pair's voltage for a requirement by voltage: `higher`, `sum` or `difference` |
| `stations` | — | 20 | the ellipses compared at stations − 1 inner points; 2 to 1000 |
| `cases` | names (`*` `?`) | every case | the weather cases the wires gallop in |
| `calculation` | — | each section's own | how the wires are solved: `initial-fe`, `creep-fe`, `load-fe`, `max-sag-fe` or the same with `-rs` |
| `[[method]]` | — | at least one | a method: each checked on its own, a row per pair |

Each `[[method]]`:

| Field | Unit | Default | Meaning |
|---|---|---|---|
| `name` | — | (required) | the method's name in the results |
| `loops` | — | (required) | the number of loops n in the span, 1 to 10 |
| `major` | m | (required) | the major axis A, a table of [terms](#the-axes-terms) (`{}`: 0) |
| `minor` | m | (required) | the minor axis B, the same terms and `major` |
| `tilt` | ° | 0 | the major axis from the vertical, its top to the right; `10.0` or `"10°"`; within ±90° |
| `centre_above_rest` | × A | 0 | the ellipse's centre above the wire at rest, along the major axis; 0.5: the wire at the lower end |
| `along` | — | `"sine"` | the size along the span: `sine` (× \|sin(nπt)\|) or `constant` |

### The axes' terms

Each axis is the sum of the terms given (those left out are 0), never
below 0:

| Term | Unit of the factor | Adds |
|---|---|---|
| `sag` | — | factor × D |
| `span` | — | factor × L |
| `sqrt_sag` | √m | factor × √D |
| `diameter` | — | factor × d |
| `log = { factor, scale }` | — | factor × n × d × ln(scale × D / (n² d)); left out when D or d is 0 |
| `constant` | m | a length (`"0.3 m"`) |
| `major` | — | factor × A (the minor axis only) |

## What is conservative and what is not

On the safe side ([theory](../theory/galloping.md#clearance-between-ellipses)):

- the ellipses are measured as polygons drawn around them: the distance
  found is never more than between the true ellipses;
- a bundle's ellipse is that of its centre line, widened by the bundle's
  outer radius all round: every subconductor's own ellipse lies inside;
- every distance is to the wires' surfaces, iced in a case with ice.

Not on the safe side, and said in the results each time it applies
(*NOT ON THE SAFE SIDE: …*):

- **A loop's middle between two stations**: the largest ellipse is not
  compared (with 20 stations: 95 % of its size for 4 loops, 99 % for 3).
  Give `stations` a multiple of 2 n.
- **A wire without a diameter** (its wire type not in the library): the
  `diameter` and `log` terms are left out for it.
- **A pair beyond the requirement's table**: the table's last distance
  is taken.
- **A case with wind**: the ellipses are sized from the swung wires'
  vertical sag. If your method means the sag in the swung wire's plane,
  or the sag of the iced wire without wind, they are too small: choose
  the case your method means.

Simplifications, said here and in the theory:

- Both ellipses are drawn in the plane square to the first wire's span:
  wires that are not parallel are compared at slightly different places.
- Nothing is compared between the stations: where two wires are not
  alike (different sags, heights), the smallest gap may lie between two
  stations, a little smaller than found.
- Only wires between the same two structures are paired: adjacent spans
  near a structure, and another line's wires, are not compared.
- The method's coefficients are yours: the check is as safe as the
  method and the numbers you give it.

## Without the window

`ohl clearance galloping` runs the same check from the command line, in
every weather case the file names (`--case` / `--tag` to choose among
them), for the whole line ([Analyses](analyses.md#galloping-ellipses-ohlg)):

```sh
ohl clearance galloping synthetic-line.ohl.json \
    --weather weather-cases.ohlwc --params galloping-example.ohlg --csv galloping.csv
```

It prints the *NOT ON THE SAFE SIDE* notes and the 20 worst pairs per
case (spans counted from 0 there, from 1 in the window); `--csv` writes
every pair (with the overlap, both ellipses' axes and the first wire's
position). It does not apply the window's scope or Use local conditions.
