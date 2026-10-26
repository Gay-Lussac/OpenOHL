# Sagging outputs

*Built, in active development (not approved yet).* What the stringing
crew measures in the field, per span of a tension section: the sag, the
correction of an inclined span, the sag per kelvin, the dynamometer
reading, the stopwatch time, a transit's angle of sight and the clipping
offsets. They are computed from the section as solved — post-processing,
the engine unchanged; the formulas are derived in the theory manual
([Sag-tension › Sagging outputs](../theory/sagtension.md#sagging-outputs)).

The state is the stringing state: in the blocks, **initial** (as strung,
on the wire's initial curve when it has stress-strain curves), bare,
still air, at the conductor temperature of sagging. In the blocks the
wire runs **over frictionless sheaves** (the default since 2026-10-25):
its tension along it is the same on both sides of each sheave, so each
span has its own horizontal tension — on unequal or inclined spans they
differ by a few tenths of a percent, even at the stringing temperature.
The section's length is that of every span at the stringing tension
(the horizontal tension in the ruling span) with the strings plumb; the
sheaves share it out. The former model, one horizontal tension through
the section, is the option *simplified (equal H)* (Line › Criteria ›
Calculation options; `blocks = "equal_h"` on the command line).

## What is given

| Output | Meaning |
|---|---|
| H | the span's horizontal tension (over sheaves each span's own) |
| sag | vertical distance from the chord, where the wire is parallel to it |
| level sag | the sag of a level span of the same length, tension and load: what a level-span sag chart gives |
| incl. corr. | sag − level sag: the correction of an inclined span to a level-span chart |
| sag per K | how much the sag changes per kelvin of conductor temperature: the correction when the conductor is not at the chart's temperature |
| dyno back / ahead | the tension a dynamometer reads at the back or ahead clamp of the span (they differ by the load per metre × the rise) |
| stopwatch | the return time of *n* waves: a pulse struck at one support, seen back *n* times (the field formula *D* = *g t*²/(32 *n*²) is its taut-span approximation) |
| transit | from an instrument below the back support: the angle of sight tangent to the wire (degrees above the horizontal) and where to set the target, as a distance below the ahead clamp |
| clipping offsets | at each structure, where the clamp point lies along the wire from the point at the block, positive into the span ahead, when the section is clipped in at another temperature than it was sagged at |

**Clipping offsets.** Clipped in at the sagging temperature, the offsets
are zero: the wire is clipped in as it lies in the blocks. Clipped in at
another temperature, the wire has rolled through the blocks and the
offsets give the marks that put each span back to its sagged length.
The sheaves are frictionless and of no size (the wire runs over them at
the clamp point): sheave friction, which holds back a little tension
from the pulling end, is not modelled yet (on the roadmap). On the
synthetic example the sheaves change the clipping offsets by under a
millimetre against the simplified model, and the sags by up to about
4 cm.

**Warnings.** A wire with stress-strain curves warns when a state lies
beyond its curves' range (`max_strain` in the [wire library](wire-library.md)).

## In the app

Open a section's window (the **Edit** tool and a click on one of its spans): the section
window has a **Sagging** panel (folded). Give the conductor temperature
(off: the stringing temperature), the temperature at clipping in (off: the
sagging temperature), the number of waves and, optionally, a transit this
far below each span's back support; **Calculate** gives the table of the
section as stored (its spans between the clamps at stringing, in the
blocks as the calculation options say). The table scrolls; a transit
that cannot see the wire (above it, or a tangent beyond the far support)
says why on hover.

## From the command line

`ohl sagtension` takes a `[sagging]` table in its job file:

```toml
[sagging]
temperature = "20 °C"          # conductor temperature (default: the reference case's)
waves = [1, 3]                 # stopwatch (default 1 and 3)
clip_temperature = "10 °C"     # clipping offsets (default: the sagging temperature)
transit = [{ span = 3, below_back = "3 m" }]   # optional; from_back = "0 m" by default
blocks = "sheaves"             # or "equal_h" (simplified: one H); default sheaves
```

With `examples/sagtension/drake-section.toml` (Drake, four spans, the
stringing tension from the criteria):

```text
Sagging (in the blocks over frictionless sheaves, initial state, bare, still air, at 20.0 °C; H 24.31 kN to 24.77 kN)
  span            H        sag  level sag  incl. corr.      sag per K    dyno back   dyno ahead     1 wave    3 waves
     1     24.48 kN    3.955 m    3.945 m      0.009 m     0.0180 m/K     24.48 kN     24.71 kN      3.59s     10.78s
     2     24.31 kN    7.916 m    7.890 m      0.026 m     0.0364 m/K     24.71 kN     24.32 kN      5.08s     15.25s
     3     24.31 kN   11.926 m   11.861 m      0.065 m     0.0549 m/K     24.32 kN     24.95 kN      6.24s     18.73s
     4     24.77 kN    5.450 m    5.446 m      0.004 m     0.0246 m/K     24.95 kN     24.79 kN      4.22s     12.66s
  Transit, span 3: 3.00 m below the back support, 0.00 m from it: angle of sight +2.4543°, tangent 95.60 m from the back support, target 26.713 m below the ahead support
  Clipping offsets, clipped in at 10.0 °C (…): 2: -0.017 m, 3: -0.017 m, 4: 0.013 m
```

Over the sheaves the dynamometer reads the same on both sides of each
suspension structure (24.71 kN ahead of span 1 and back of span 2); the
simplified model (`blocks = "equal_h"`) gives one H, 24.40 kN, instead.

A wire with stress-strain curves is sagged on its initial curve; its
report also gives, per weather case, the horizontal tension initially,
after creep and after load, and which final state governs (the longer
wire, the larger sag) — see
[Sag-tension › Non-linear stress-strain and creep](../theory/sagtension.md#non-linear-stress-strain-and-creep).

Results must be verified by a qualified engineer.
