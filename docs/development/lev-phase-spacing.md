# LeV distance between conductors in the span (still air) — specification

Status: **steps 1 to 4 of the plan implemented** (the values file
`libraries/phase-distance/lev-ch.olspd`, the check
`ols_clearance::phase_distance`, with tests worked by hand; the length
below z, nearby pairs and the faster search after the first run on an
existing line; the Circuits table's insulation level, a column of the
LeV plugin; the window, Checks › LeV / OLEI › Distance between
conductors); the library of performed checks and the command line to
come. Decisions marked
*(decided)* were taken with the user; *(open)* ones are still to settle.
Change this page when a decision changes.

## The law

Swiss *Leitungsverordnung* (LeV, SR 734.31), Annex 6 (referenced by
Art. 33), item 2.2.1 — the direct distance of two conductors in the span:

\\[ z = k \cdot \sqrt{f + l_k} + C \\]

- \\( C = C_1 \\) within the same circuit, and towards an earth wire
  (2.1.1); \\( C = C_2 \\) between different circuits — Table 2.4.1, by
  nominal voltage (the larger nominal voltage where circuits differ,
  2.3). The table holds up to 1000 m above sea level; above, the values
  grow linearly by 14 % per 1000 m.
- \\( f \\): the conductor's sag at 10 °C conductor temperature (2.4).
- \\( l_k \\): the length (Baulänge) of suspension strings (2.4).
- \\( k \\): Diagrams 2.4.2 (wind pressure 500 N/m²) and 2.4.3
  (650 N/m²), linear in the system angle β (the angle of the line
  between the conductors from the vertical), one line per swing class:

  | Swing angle φ | class w = d/q at 500 N/m² | at 650 N/m² | k at β = 0° | k at β = 90° |
  |---|---|---|---|---|
  | φ ≤ 40° | w ≤ 16.8 | w ≤ 12.9 | 0.70 | 0.60 |
  | 40° < φ ≤ 55° | 16.8 < w ≤ 28.5 | 12.9 < w ≤ 22.0 | 0.75 | 0.65 |
  | 55° < φ ≤ 65° | 28.5 < w ≤ 42.8 | 22.0 < w ≤ 33.0 | 0.85 | 0.70 |
  | φ > 65° | w > 42.8 | w > 33.0 | 0.95 | 0.75 |

  d: conductor diameter in mm, q: conductor weight in kg/m,
  φ = arctan(p · d / q · 10⁻⁴).
- Conductors of different cross-section, material or sag: the distance
  is determined for each conductor and the larger one applies (2.2.3).

(Swiss legislation is not protected by copyright; the values above are
taken from the official text, as of 1 January 2008.)

## What is measured *(decided)*

For each **pair of conductors** in the **weather case(s) chosen**
(default: still air at 10 °C; each case checked on its own — both
conductors always from the same case):

1. **Stations** along the first conductor, every *Δs* of its length
   (parameter, default **1 m**), its ends included.
2. At each station P on the first conductor, the **closest point Q on
   the second conductor** (3D). The measurement is made in the
   **vertical plane through P and Q** (not square to the span: with
   large line angles at the structures, a square section would compare
   points far apart along the other conductor).
3. **Distance** d = |PQ|.
4. **Local sag** of each conductor at its point: the vertical distance
   from its chord (the straight line between its clamps) down to the
   conductor there (the usual sag definition) — 0 at the clamps.
5. **l_k** of each conductor at its point: linear along its span between
   its two ends, by the length of its fraction of the span. At an end:
   - a pure suspension I-string: its length;
   - a V-string: 0;
   - a tension (strain) string: 0;
   - a V with a string below: the length of the string below;
   - a TAK: the length of its suspension string (the one from the
     structure to the node);
   - a wire clamped straight to the structure: 0.
   Example: tension at one end (0), a 3 m suspension string at the
   other: 1.5 m at mid-span.
6. **β**: the angle of PQ from the vertical.
7. **k** at that station, for each conductor, from β and its swing
   class on the chosen diagram.
8. **Required**: \\( z = \max_i \big(k_i \sqrt{f_i + l_{k,i}}\big) + C \\)
   with C by voltage, pair kind and altitude.
9. **Margin** d − z; below 0: a violation.

The stations are taken on each conductor of the pair in turn (P on the
first and Q on the second, then the other way), so a short and a long
conductor are both covered. For a bundle, each subconductor's stations
are measured to the nearest subconductor of the other conductor.

## Which pairs *(decided — may change)*

- Conductors (sections' spans; bundles: the nearest subconductors)
  hanging **between the same two structures**. A parallel line on its
  own structures is not checked here.
- Only the **circuits ticked** (a drop-down list of tick boxes, with
  select all / select none); a pair is checked when both its
  conductors' circuits are ticked.
- **Nearby pairs** *(decided)*: a tick box adds pairs of conductors not
  between the same two structures (another line's spans, the next
  span's) whose boxes come within a **maximum distance** (field,
  default 20 m — keep it above the largest z, about 12 m at 380 kV);
  they are measured only where closer than that distance.
- Same circuit (and phase to earth wire): C1; different circuits: C2.
- Not paired: jumpers (they only go from one span to the next), two
  earth wires, two conductors of the same circuit and phase (one
  potential), two sharing a clamp (the next span of the same wire).

## Report

Per pair: the worst station (its two points, where along each
conductor, d, z, the margin, f and l_k of each conductor, β, k, C, the
voltage and the case), and the **length of conductor below z** (m, on
the conductor and subconductor where it is longest — each stretch
counted once, not per subconductor pair or measuring direction).
Shown in the check's window, worst first, and exported as CSV.
**Clicking a result** draws in the model the line between its
two points, labelled with its length d and the required z.

A note on the report: a worst station **at a clamp** (f about 0) is
legitimate — the formula holds there too, with z then C plus the
string term — but is marked as such, so a design that is fine in the
span and tight only at the structure is easy to recognise.

## Settings

- Weather case(s): any of the model's (default: a still-air case at
  10 °C), each checked on its own.
- Circuits: ticked in a list (select all / none).
- Station spacing Δs: default 1 m.
- Diagram for k: 2.4.2 (500 N/m²) or 2.4.3 (650 N/m²), a choice
  *(decided)*.
- Table 2.4.1 row where a voltage has two (110/132, 220, 380 kV: the
  lightning impulse withstand level): low or high, chosen **per
  circuit** in the Circuits table (the LeV plugin's column); a circuit
  not set takes the high row (the stricter) *(decided; the default may
  change)*.
- Altitude for the +14 %/1000 m: the **mean conductor height of the
  span** above sea level (the mean height of its conductor points)
  *(decided)*.

## Not in this check

On the structure (C1/C2, at least 0.5 m), with wind (C3/C4, wind
differing by up to 40 % between conductors), phase spacers (2.2.4).

## Implementation plan

1. **Values as data.** A parameter file holding the LeV's C1/C2 rows by
   nominal voltage (low and high where there are two), the four k
   lines of each diagram with their swing-class limits, the altitude
   rule (1000 m, +14 %/1000 m), and the l_k rules by string kind. The
   LeV values ship as one such file; the LeV design-code plugin can
   produce it later (design codes as plugins producing rule sets).
2. **The check** in `ols-clearance` (a module of its own, next to the
   galloping check that already pairs wires between the same two
   structures): from the line (resolved: strings, structures), the
   solved shapes of one weather case, the cables (diameter, weight)
   and the settings, per pair of conductors between the same two
   structures of the ticked circuits: stations, closest points, local
   sags, l_k, β, k, z, margins; the worst station per pair and the
   length below z. Pairs measured on all cores; the closest point
   searched in boxes of pieces, nearest first.
3. **The Circuits table column** *(done)*. The LeV plugin's manifest
   adds a column "insulation level" (low / high, not set: high) to the
   Circuits table; its value is kept on the circuit's sections
   (`lev-ch.insulation_level`) and the check reads it there. Tables
   accept columns added by plugins (`[[column]]` in any plugin's
   manifest, on structures, sections, circuits, systems, jumpers,
   alignments, routes): the values are kept with the objects, saved and
   in OLS XML, and shown (unused) where the plugin is not installed.
4. **The app** *(done; saving to the library comes with it)*. A window
   (Checks › LeV / OLEI, a submenu for the ordinance's checks): the weather cases ticked
   (default: a still-air case at 10 °C), the circuits ticked (a
   drop-down of tick boxes, select all / select none), the diagram
   (500 or 650 N/m²), Δs, nearby pairs and their distance (the levels:
   the Circuits table). The results in the window (worst station per
   pair, all its terms; only those below z, or all), the clicked one's
   line in the 3D view with d and z; CSV export. The settings are kept
   for the session; with the library, a saved report keeps them.
5. **The command line.** The same check from `ols line` (parameter file,
   cases, circuits), its CSV.
6. **The library of performed checks** *(decided — see below)*.
7. **Tests and benchmarks** (below), docs (user guide, theory page).

### The library of performed checks *(decided)*

For every check (this one first, then wire clearances, wind
clearances, galloping, loads…), with one format:

1. **Saving**: each check's window offers to save its result in the
   project's library of performed checks (a choice, per run).
2. **Where**: in the project, **attached to the branch of the line** it
   was made on (version control): it moves with the branch — commits,
   checkout, merge, the bundles carrying a branch to an offline copy —
   and is imported and exported with it.
3. **The state of the line**: the report records the project log entry
   it was made after and a fingerprint of each object it used. Reloaded,
   it says "line unchanged", or "changed since" with the objects changed
   (e.g. 4 sections, 2 structures).
4. **Contents**: the time and user; the app's version, and the plugin's
   name and version when a plugin made it; every parameter (the values
   file and its fingerprint, the weather cases with their values, the
   settings, the insulation levels used); the results; a note.
5. **Reloading**: its results in the Clearances tab and the 3D view
   (clicking one draws the line with d and z); marked *stale* when the
   line changed; **run again** with the same parameters shows the
   comparison (new, fixed, worse, better), then **asks whether to
   replace the old report with the new one** — one report per check is
   kept, no history (the line's history is kept by version control).
6. **Managing**: a list filtered by check, date, user and status;
   rename, note, delete (asking first); a **status** to follow where it
   is (e.g. draft, checked, approved). Sharing (a file format for other
   people) later.

### First run on an existing line (local)

The still-air 10 °C case of a 25 km line with 380, 220, 132 and 65 kV
circuits: 930 conductors, 5593 pairs between the same structures,
60 below z (about 12 km of conductor in all); with nearby pairs
(20 m), 10 468 pairs and 4 more below. 0.8 s on all cores (first
version: 10.9 s, one core, every piece of the other conductor per
station; now boxes of 8 pieces searched nearest first). The worst: a
380 kV conductor above a 132 kV one in long spans (f 25–29 m, l_k
7.6 m), and a phase near its clamp against an earth wire. Two phases
of a short drop span that cross each other in the data (d ≈ 0) were
found too.

## Tests (planned)

- Benchmarks worked by hand (`ols validate benchmarks`): a single
  span, 380 kV, f = 12 m at 10 °C, l_k = 4.5 m, β = 0°, swing class
  φ ≤ 40°: k = 0.70, z = 0.70 · √16.5 + 2.64 = 5.48 m; then β = 90°,
  tension at one end (l_k linear), two circuits (C2), a span above
  1000 m, two different conductors, a line angle (the vertical plane
  through the closest points).
- The synthetic line in the repository; locally the real lines and the
  test line.
