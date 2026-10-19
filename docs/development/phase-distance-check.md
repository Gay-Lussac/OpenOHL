# Distance between conductors: the core's part

A design code may require a distance between two conductors of a span
that depends on where along the span one looks — on the local sag, the
suspension strings, the angle of the line between the conductors.
OpenOHL **measures** (the conductors, their pairs, stations along them: the
geometry); a **check plugin** holds the code's values and its rules and
says what is required at each station, through the `check` interface
([Writing plugins](plugins.md#check-plugins)). A plugin for the Swiss
LeV runs such a check; it is kept apart from the public code. The
measuring is `ohl_clearance::phase_distance`, the host's side
`ohl_io::checks`, the window `crates/ohl-app/src/phase_distance.rs`.

## What is measured

For each **pair of conductors** in each **weather case chosen** (each
case checked on its own, both conductors always from the same case):

1. **Stations** along the first conductor, every *Δs* of its length
   (default **1 m**), its ends included.
2. At each station P, the **closest point Q on the second conductor**
   (3D). The measurement is made in the vertical plane through P and Q
   (not square to the span: with large line angles a square section
   would compare points far apart along the other conductor).
3. **Distance** d = |PQ| between the conductors' **axes**, as the codes
   define the phase distance (LeV: between axes, owner 2026-10-19), less
   both polylines' error bounds (a few millimetres); P and Q are on the
   axes. The other clearances are measured to the surfaces
   ([Clearance](../user-guide/terrain.md#clearance)).
4. **Local sag** of each conductor at its point: the vertical distance
   from its chord (the straight line between its clamps) down to the
   conductor — 0 at the clamps.
5. **l_k** of each conductor at its point: the length of the suspension
   string, linear along its span between its two ends. At an end: a
   suspension I-string, its length; a V-string, a tension string or a
   wire clamped straight to the structure, 0; a V with a string below,
   that string's length; a TAK, its suspension string's length.
6. **β**: the angle of PQ from the vertical.
7. With **phase spacers** joining the two conductors in the span, each
   one's largest sag between its spacer clamps (each spacer taken as an
   attachment point: the sag of each stretch on either side of it below
   that stretch's chord, the biggest counting); the local sag stays the
   one between the span's clamps.

The stations are taken on each conductor of the pair in turn, so a short
and a long conductor are both covered. For a bundle, each
subconductor's stations are measured to the nearest subconductor of the
other conductor.

How exact the measuring is (labels as in the [theory manual](../theory/conventions.md#conservative-assumptions-and-simplifications)):

- **Conservative:** d is between the conductors' axes less both
  polylines' bounds (a few millimetres), so never above the true distance
  between the catenaries' axes. A code that defines the required distance
  between the surfaces would need the two radii added to its required
  distance by its plugin.
- **Simplification:** stations every Δs: the closest approach between two
  stations is not measured, but the second conductor's closest point is
  searched continuously at each station, so the error is that of the first
  conductor's spacing (small for Δs = 1 m, unsafe). Exact: the minimum of d
  along the first conductor.

The plugin receives, per pair, what its rules need besides the stations
(`conductor-pair`): whether both are in one electrical system, whether
they hang between the same two structures, each one's nominal voltage
(none: an earth wire), the plugin columns of their sections, the mean
conductor height of the span, the diameters and masses, the sags between
spacers; and per station the six numbers d, β, each local sag, each l_k.
It returns per station the required distance first, then its own values
(kept with the worst station), and the rule applied.

## Which pairs

- Conductors (sections' spans; bundles: the nearest subconductors)
  hanging **between the same two structures**.
- Only the **circuits ticked**; a pair is checked when both its
  conductors' circuits are.
- **Nearby pairs** (a tick box): pairs of conductors not between the
  same two structures (another line's spans, the next span's) whose
  boxes come within a **maximum distance** (default 20 m), measured only
  where closer than that.
- Not paired: jumpers, two earth wires, two conductors of the same
  circuit and phase (one potential), two sharing a clamp.

## Report

Per pair: the worst station (its two points, d, the required distance,
the margin, the measured values and the plugin's own, the case) and the
**length of conductor below the required distance** (on the conductor
where it is longest, each stretch counted once). Shown in the check's
window, worst first, exported as CSV; a worst station at a clamp is
marked as such. Clicking a result draws the line between its two
points in the 3D view, labelled with d and the required distance. Runs
are kept in the library of performed checks
([Analyses](../user-guide/analyses.md#the-library-of-performed-checks)).

## The values

The plugin ships its values file (TOML, its own extension) and shows it
as tables (`value-tables`), locked in the window; a project admin may
edit a cell (`edit-value`, checked by the plugin), kept as the
project's copy and used by every run on the project and kept in each
report.

## Speed

On an existing line (930 conductors, 5593 pairs between the same
structures; about 10 500 with nearby pairs within 20 m): about 0.8 s on all
cores, the closest point searched in boxes of 8 pieces, nearest first;
the plugin's evaluation costs about as much as the same computation
built into OpenOHL, plus about 0.3 s to load the plugin once.
