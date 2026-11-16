# Structures in the wire solve (calculation level)

*Built, in active development (not approved yet).* **Line › Criteria ›
Calculation options… › Structures in the wire solve** in OHL-Design. How
the structures hold the wires in the checks that take the line's
**calculation level**. The method, its sources and what it leaves out:
[theory › Structure flexibility in the line solve](../theory/structure-flexibility.md).

*Calculation level* is how the line and its structures are computed
together. It is not the *detail level* of a structure (how much its file
describes: stick figure, outline, full design, joint details).

## The three choices

| Choice | Level | What it does |
|---|---|---|
| **Rigid** (default) | 2 | Every structure point holds its wires where it is drawn: the structures do not move, whatever the wires pull. |
| **Linear stiffness** | 3 | Each structure that has a computable model (a full design, a separate `.ohlfe` model, or a pole with an elasticity) answers the wires' forces with a deflection: a linear stiffness at its attachment points, condensed once from its model at the stringing state, with its own wind and ice of the case in its deflection. Each case is solved again until the wires and the structures agree (two to five line solves per case). |
| **Full structures** | 4 | The structures' full non-linear models in every pass. **Not built yet**: shown greyed, not offered; a line file that asks for it is read, but a check run on it is refused with why. |

The choice is one change of the line's settings (one command, undone as
one); every result made before it is *out of date* (its banner names the
line). It is written in the line file only when not rigid (line file
version 21, `solver.structures`).

## Its time and its effect, beside each choice

Beside each choice the window says what it costs and changes **on this
computer** (kept with the application, for every project):

- **Rigid**: the last rigid run here — what ran, how long, on how many
  structures and sections.
- **Linear stiffness**: the last level-3 run here — what ran, its time,
  the structures' setup time, how many times its rigid solves it took,
  and the effect it measured: the largest change against rigid (a span's
  tension, a string's swing or load, a structure's load) and the largest
  move of a point. Not run here yet: an estimate from the last rigid run
  (about four line solves per case instead of one, plus one condensation
  per structure first).

The **(?)** beside the title lists the structures that stay rigid at
level 3 and why (no structure type, a type not in the library, a type
with neither a structure model nor a pole), read from the files; after a
level-3 run, also the ones that run took rigid (a model that did not
solve at the stringing state, an attachment point in a direction no
member holds…).

## Which checks take it

| Check | At level 3 |
|---|---|
| [Wire tensions](wire-tensions.md) | both solves of every case; each span's tension the larger of the two |
| String checks | both solves; each string's swing and load the row of the larger usage; the swing measured from the moved top |
| [Structure loads](structure-loads-window.md) | each load the larger of the two per component; the usage the worse of the two checks |
| The loads sent to [OHL-Structure](structure-app.md) | each case twice, from the rigid and from the level-3 solve, each saying its level |
| Clearances (vertical, radial, wire clearances, vegetation, safety volumes, LiDAR) | not yet: rigid, said in their notes where it is cheap to say ("calculation level 2 (structures rigid): this check does not take the line's level yet") |
| Internal clearances | rigid until the structure body is drawn deflected (a moved conductor against an undeflected body would not be consistent), said |
| Galloping, ice shedding, lightning, distance between conductors | rigid, said |
| Views, tables, sagging tables, stringing charts, drawings | rigid (the sagging tables by definition: the structures as drawn are the structures as strung) |

**Never less safe than rigid** (owner, 2026-11-15): every check that
takes the level checks the rigid solve and the level-3 solve of each
case and keeps the worse. Choosing level 3 can only find more, never
less, than rigid; the change against rigid is said.

## What the results say

The notes of every result solved at level 3 begin with:

- the level and how the structures are taken (linear stiffness at their
  attachment points, linearised at the stringing state, their own wind
  and ice in their deflection);
- the reference: the structures as drawn are the structures as strung
  (displacements from the stringing state);
- the rule of the worse of rigid and level 3;
- how many structures are flexible and which are taken rigid, with why;
  the sections solved by the ruling span (rigid, their forces still
  loading the structures);
- the setup's time, the cases solved, their passes and time, the largest
  move of a point;
- the largest change against rigid of the check's value;
- a case whose loop did not converge, checked on the rigid solve only;
- what is **NOT ON THE SAFE SIDE** where the level-3 solve governs: the
  deflection already present at stringing is left out, the models'
  foundations are rigid, the linearisation is stiffer than the case's
  state where members would go slack — the points move less than the
  real structures.

A rule that solves the wires by another method than the line (a rule's
ruling span on sections of finite elements, or the other way round) is
solved rigid, said: the structures' stiffness is set up for the line's
own methods.

## The Structure loads window's switch

The Structure loads window's **Structures flexible (level 3)** follows
the line's choice; ticked or unticked there it sets that window's runs
only (and the loads sent to OHL-Structure while it is so), kept with the
project's setup, and its hover says the line's choice. Ticked back to
the line's choice, it follows the line again.

## On the command line

`--structures-as rigid|linear` overrides the line's choice for one run on
`ohl line` (the weather cases solved again at level 3 and printed after
the rigid tables; with `--load-cases` the structure loads and usage),
`ohl tensions`, `ohl strings` and the other commands reading a line (those
that do not take the level say so and stay rigid). `ohl line
--calculation-level 2|3` is the same by the level's number. `full` is
refused: not built yet. `ohl structures …` solves no line (its loads come
from `ohl line --loads-csv`, which takes the level).
