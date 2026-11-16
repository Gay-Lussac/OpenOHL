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
| Clearances (vertical, radial, wire clearances, vegetation, safety volumes, LiDAR) | not yet: rigid, said in their notes where it is cheap to say ("calculation level 2 (structures rigid): this check does not take the line's level yet"); with a [reference state](#the-reference-state-of-the-structures) in use: the structures as deflected in its case |
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

## The reference state of the structures

*Built, in active development.* Owner (2026-11-16): *"Would it be
possible to define a weather case at which the structure is drawn in OHL
design where it would solve at level 4 and then keep it for later for
example level 2 solves. This could be useful to make the model match the
lidar scans."* Level 4 is not built yet, so the reference state is solved
at **level 3** for now; once level 4 is built it is solved there.

**Where:** Line › Criteria › Calculation options, under *Structures in the
wire solve*: **Reference case** (a weather case of the project, or none),
**Solve and keep**, **Remove**. Typical: the survey day's temperature and
wind.

1. Choose the case, then **Solve and keep**. The structures are solved
   in that case at level 3 (each structure with a model a linear
   stiffness at its attachment points, condensed at the stringing state,
   its own wind and ice of the case in its deflection), on its own
   thread — a spinner and **Stop** while it runs. Then the state is kept
   with the line: one step, undone as one (Edit › Undo).
2. While the state is **current**, every rigid (level 2) solve — the
   views, the tables, the clearances, the LiDAR
   comparison — takes each attachment point of a flexible structure
   moved by its displacement in the reference case. As fast as before:
   the same solve, the points moved.
3. The 3D view draws those structures **deflected** as in the reference
   case (at true scale: millimetres to centimetres for lattice towers,
   decimetres for poles), and says so at its bottom left: *Structures
   drawn as deflected in case "…" (calculation level 3)*. The sheets
   draw the same bodies.
4. **Remove** sets none again (one step): the structures as drawn, as
   before, bit for bit.

The window shows the state in use (its case, how many points moved, the
largest move and where, the structures kept as drawn) and its time: once
per solve — on the example line about 0.6 s (setup 0.1 s, the case's
coupled solve 0.5 s in three passes, the deflected bodies 15 ms, an
unoptimised build); every later rigid solve as fast as before.

**Out of date.** The state keeps a fingerprint of what it was solved
from: the line (its structures, sections, jumpers, settings), the wires'
library entries, the structure types with their models and poles and the
cross-sections, and the case's weather. Any edit of one of them — a
structure moved or turned, a wire changed, a model edited, the case's
wind changed, another reference case chosen — makes it **out of date**:
it is then **not used** (the structures as drawn, bit for bit), the 3D
view and every result say *Reference case "…" out of date (… changed
since it was solved): structures as drawn*, and **Solve and keep** solves
it again. While a structure is dragged it is out of date too. A project
opened, or a line read on the command line, checks it before anything is
solved; one that cannot be checked (the weather cases not given) is not
used, said.

**What the results say:** *structures as deflected in case "…"
(calculation level 3): each flexible structure's attachment points moved
as solved there; every other case keeps that lean, not its own — NOT ON
THE SAFE SIDE where a case leans the structures further*, with the
structures kept as drawn (no computable model) listed. A case other than
the reference one keeps **the reference case's lean**, not its own: a
case with more ice or more wind than the reference would lean the
structures further than drawn (theory: [the reference
state](../theory/structure-flexibility.md#the-reference-state-of-the-structures)).
Check those at level 3, where each case finds its own deflection.

**Not taken by:**

| Where | Why |
|---|---|
| Level 3 (and 4) | each case finds its own deflection from the structures as strung; the reference state is the level-3 solve of its own case (said: "the line's reference case not used at calculation level 3") |
| [Structure loads](structure-loads-window.md) and the loads sent to OHL-Structure | kept as drawn, rigid: on the safe side (loads never below rigid, owner 2026-11-15), said |
| Internal clearances | the structure body is not deflected in the check: the structures and their strings as drawn, said |
| The As-built wires window unticked | its **Structures** switch (below) |
| Sections solved by the ruling span | their points are not moved |

**Draw the structures as designed (or as strung).** The reference state
adds the reference case's deflection to the structures as drawn. A model
whose attachment points were placed from a scan taken in the reference
case already leans as there: its state would count the lean twice.

## On the command line

`--structure-reference use|ignore` (every command reading a line, and
`ohl line`): `use` (default) checks the line's reference state against
the libraries and the run's weather cases and, current, takes it;
`ignore` the structures as drawn. Either way the run says which.
`ohl line … --weather W --solve-reference "CASE"` solves one for the run
(each structure's largest move, the times printed) and `--save-reference
FILE` writes the line with it to a new line file (the input is not
changed).

`--structures-as rigid|linear` overrides the line's choice for one run on
`ohl line` (the weather cases solved again at level 3 and printed after
the rigid tables; with `--load-cases` the structure loads and usage),
`ohl tensions`, `ohl strings` and the other commands reading a line (those
that do not take the level say so and stay rigid). `ohl line
--calculation-level 2|3` is the same by the level's number. `full` is
refused: not built yet. `ohl structures …` solves no line (its loads come
from `ohl line --loads-csv`, which takes the level).
