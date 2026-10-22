# Sagging outputs

*Built, in active development (not approved yet).* What the stringing
crew measures in the field, per span of a tension section: the sag, the
correction of an inclined span, the sag per kelvin, the dynamometer
reading, the stopwatch time, a transit's angle of sight and the clipping
offsets. They are computed from the section as solved — post-processing,
the engine unchanged; the formulas are derived in the theory manual
([Sag-tension › Sagging outputs](../theory/sagtension.md#sagging-outputs)).

The state is the stringing state: in the blocks (one horizontal tension
through the section), **initial** (as strung, on the wire's initial
curve when it has stress-strain curves), bare, still air, at the
conductor temperature of sagging.

## What is given

| Output | Meaning |
|---|---|
| sag | vertical distance from the chord, where the wire is parallel to it |
| level sag | the sag of a level span of the same length, tension and load: what a level-span sag chart gives |
| incl. corr. | sag − level sag: the correction of an inclined span to a level-span chart |
| sag per K | how much the sag changes per kelvin of conductor temperature: the correction when the conductor is not at the chart's temperature |
| dyno back / ahead | the tension a dynamometer reads at the back or ahead clamp of the span (they differ by the load per metre × the rise) |
| stopwatch | the return time of *n* waves: a pulse struck at one support, seen back *n* times (the field formula *D* = *g t*²/(32 *n*²) is its taut-span approximation) |
| transit | from an instrument below the back support: the angle of sight tangent to the wire (degrees above the horizontal) and where to set the target, as a distance below the ahead clamp |
| clipping offsets | at each structure, where the clamp point lies along the wire from the point at the block, positive into the span ahead, when the section is clipped in at another temperature than it was sagged at |

**Clipping offsets in this model.** In the blocks, OpenOHL keeps one
horizontal tension through the section. Clipped in at the sagging
temperature, the offsets are therefore zero; clipped in at another
temperature, the wire has rolled through the blocks and the offsets give
the marks that put each span back to its sagged length. Sheaves that pass
the tension along the wire (and their friction) are not modelled: on
inclined spans they give offsets and sag corrections of a few millimetres
even at the sagging temperature (a sheave model is not wanted: the owner's decision of 2026-10-22).

**Warnings.** A wire with stress-strain curves warns when a state lies
beyond its curves' range (`max_strain` in the [wire library](wire-library.md)).

## In the app

Open a section's window (the **Edit** tool and a right-click on one of its spans): the section
window has a **Sagging** panel (folded). Give the conductor temperature
(off: the stringing temperature), the temperature at clipping in (off: the
sagging temperature), the number of waves and, optionally, a transit this
far below each span's back support; **Calculate** gives the table of the
section as stored (the ruling-span model). The table scrolls; a transit
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
```

With `examples/sagtension/drake-section.toml` (Drake, four spans, the
stringing tension from the criteria):

```text
Sagging (in the blocks, initial state, bare, still air, at 20.0 °C; H 24.40 kN)
  span        sag  level sag  incl. corr.      sag per K    dyno back   dyno ahead     1 wave    3 waves
     1    3.966 m    3.957 m      0.009 m     0.0182 m/K     24.40 kN     24.64 kN      3.60s     10.79s
     2    7.885 m    7.860 m      0.025 m     0.0362 m/K     24.81 kN     24.41 kN      5.07s     15.22s
     3   11.881 m   11.816 m      0.065 m     0.0545 m/K     24.41 kN     25.05 kN      6.23s     18.69s
     4    5.532 m    5.528 m      0.004 m     0.0254 m/K     24.59 kN     24.43 kN      4.25s     12.75s
  Transit, span 3: 3.00 m below the back support, 0.00 m from it: angle of sight +2.4748°, tangent 95.78 m from the back support, target 26.577 m below the ahead support
  Clipping offsets, clipped in at 10.0 °C (…): 2: -0.017 m, 3: -0.017 m, 4: 0.012 m
```

A wire with stress-strain curves is sagged on its initial curve; its
report also gives, per weather case, the horizontal tension initially,
after creep and after load, and which final state governs (the longer
wire, the larger sag) — see
[Sag-tension › Non-linear stress-strain and creep](../theory/sagtension.md#non-linear-stress-strain-and-creep).

Results must be verified by a qualified engineer.
