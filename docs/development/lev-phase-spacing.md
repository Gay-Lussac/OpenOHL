# LeV distance between conductors in the span (still air) — specification

Status: **steps 1 to 4 and 6 of the plan implemented**, and **moved to
the lev-ch plugin** (decided with the user: OLS CAD keeps what lets the
check run — the conductors, the pairs, the stations, the sags between
spacers, the window — the plugin the values and the rules, through the
`check` plugin interface; see [Writing plugins](plugins.md#check-plugins)
for the speed comparison). The values file
`plugins/codes/lev-ch/phase-distance.olspd`, the measuring
`ols_clearance::phase_distance`, the rules `lev-ch`'s
`phase_distance.rs`, with tests worked by hand; rule 2.2.4 (phase
spacers: z = k·√d_f + C₃ ≥ C₁, C₄ ≥ C₂ between systems, at least
0.90 m; d_f the largest sag between the spacers: each spacer taken as
an attachment point, the sag of each stretch on either side of it (span
clamp – spacer, spacer – spacer, spacer – span clamp) below that
stretch's chord, the biggest counting *(decided)*; f stays the sag
between the span's clamps, as without spacers *(decided)*); the length
below z, nearby pairs and the faster search after the first run on an
existing line; the Circuits table's insulation level, a column of the
LeV plugin; the window, Checks › LeV/OLEI (the plugin's menu) › Distance between
conductors; the library of performed checks, Checks › Performed
checks); the command line to come. Decisions marked
*(decided)* were taken with the user; *(open)* ones are still to settle.
Change this page when a decision changes.

## The law

Swiss *Leitungsverordnung* (LeV, SR 734.31), Annex 6 (referenced by
Art. 33), item 2.2.1 — the direct distance of two conductors in the span:

\\[ z = k \cdot \sqrt{f + l_k} + C \\]

- \\( C = C_1 \\) within one electrical system, and towards earth (earth
  wires, earthed parts); \\( C = C_2 \\) between different electrical
  systems — Table 2.4.1, by nominal voltage (the larger nominal voltage
  where systems differ, 2.3). The table also gives C₃ and C₄, the same
  with wind (for the checks with wind). Where a voltage has two lines,
  they are the two insulation levels: each value is given for a
  lightning impulse withstand voltage (e.g. 220 kV low: C₁ at 950 kV,
  C₂ at 1050 kV; high: C₁ at 1050 kV, C₂ at 1175 kV). The table holds up
  to 1000 m above sea level; above, the values grow linearly by 14 % per
  1000 m.

  Table 2.4.1 (consolidated text of 1 June 2019 on fedlex.admin.ch),
  as validated with the user; "at kV": the lightning impulse withstand
  voltage the value is for (C₃ and C₄ come with 50-Hz test voltages in
  the ordinance, not kept here):

  | Up to kV | Level | C₁ m | C₁ at kV | C₂ m | C₂ at kV | C₃ m | C₄ m |
  |---|---|---|---|---|---|---|---|
  | < 1 | — | 0.05 | — | 0.05 | — | 0.05 | 0.05 |
  | 3 | — | 0.06 | 40 | 0.06 | 46 | 0.06 | 0.06 |
  | 6 | — | 0.09 | 60 | 0.09 | 70 | 0.09 | 0.09 |
  | 10 | — | 0.12 | 75 | 0.12 | 85 | 0.12 | 0.12 |
  | 15 | — | 0.16 | 95 | 0.17 | 110 | 0.16 | 0.16 |
  | 20 | — | 0.22 | 125 | 0.26 | 145 | 0.18 | 0.18 |
  | 30 | — | 0.32 | 170 | 0.39 | 195 | 0.21 | 0.24 |
  | 45 | — | 0.52 | 250 | 0.63 | 290 | 0.30 | 0.36 |
  | 60 | — | 0.70 | 325 | 0.82 | 375 | 0.45 | 0.52 |
  | 110 | — | 0.92 | 450 | 1.38 | 650 | 0.61 | 0.89 |
  | 132 | — | 1.12 | 550 | 1.38 | 650 | 0.76 | 0.89 |
  | 150 | — | 1.32 | 650 | 1.64 | 750 | 0.92 | 1.05 |
  | 220 | low | 1.93 | 950 | 2.29 | 1050 | 1.35 | 1.60 |
  | 220 | high | 2.14 | 1050 | 2.56 | 1175 | 1.60 | 1.90 |
  | 380 | low | 2.64 | 1300 | 3.37 | 1550 | 2.09 | 2.30 |
  | 380 | high | 2.90 | 1425 | 3.64 | 1675 | 2.33 | 2.86 |

  The ordinance's text (2.1.1, 2.2.1) says "Stromkreis" (circuit) where
  the table says "System": one three-phase circuit, an electrical system
  in the line file.
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
- Within one electrical system (and to an earth wire): C1; between
  different systems: C2 *(decided)*. Sections without a system compare
  by circuit name.
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
differing by up to 40 % between conductors). Phase spacers (2.2.4): in
the check since the spacers came (see the status).

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
   the Circuits table). The values used shown as tables, locked; an
   admin may edit them, kept as the project's copy of the file. The results in the window (worst station per
   pair, all its terms; only those below z, or all), the clicked one's
   line in the 3D view with d and z; CSV export. The settings are kept
   for the session; with the library, a saved report keeps them.
5. **The command line.** The same check from `ols line` (parameter file,
   cases, circuits), its CSV.
6. **The library of performed checks** *(done, for this check — see
   below; the other checks to follow)*.
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
