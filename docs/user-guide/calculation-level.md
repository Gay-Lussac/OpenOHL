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
| **Full structures** | 4 | Level 3 first; then, from its solution, each such structure is solved in full in every pass — its own P-Delta, its tension-only members and guys going slack or taut with the load, its own wind and ice — the structures side by side, until the wires and the structures agree (on the example line at most one line solve more than level 3). A structure whose full solve fails in a case (unstable under the loads with the line) is taken linear there and reported **failing** in that case. The member and code checks are made once, on the converged forces. |

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
- **Full structures**: the last level-4 run here — the same, with the
  largest difference from level 3 (a point's move, a point's force) and
  the structures that failed at level 4 in a case. Not run here yet: an
  estimate from the last level-3 run (about 1.5 times its time on the
  example line, some two to three times with towers of thousands of
  members: level 3's line solves, then one to four more, each with a full
  solve of every flexible structure, side by side — some tenths of a
  second each for such a tower), else from the last rigid run.

The **(?)** beside the title lists the structures that stay rigid at
levels 3 and 4 and why (no structure type, a type not in the library, a type
with neither a structure model nor a pole), read from the files; after a
level-3 run, also the ones that run took rigid (a model that did not
solve at the stringing state, an attachment point in a direction no
member holds…).

## Which checks take it

| Check | At levels 3 and 4 |
|---|---|
| [Wire tensions](wire-tensions.md) | both solves of every case; each span's tension the larger of the two |
| String checks | both solves; each string's swing and load the row of the larger usage; the swing measured from the moved top |
| [Structure loads](structure-loads-window.md) | each load the larger of the two per component; the usage the worse of the two checks; at level 4 a structure that failed there in a case is failing in it |
| The loads sent to [OHL-Structure](structure-app.md) | each case twice, from the rigid and from the level-3 (or level-4) solve, each saying its level |
| Clearances (vertical, radial, wire clearances, vegetation, safety volumes, LiDAR) | not yet: rigid, said in their notes where it is cheap to say ("calculation level 2 (structures rigid): this check does not take the line's level yet"); with a [reference state](#the-reference-state-of-the-structures) in use: the structures as deflected in its case |
| Internal clearances | rigid until the structure body is drawn deflected (a moved conductor against an undeflected body would not be consistent), said |
| Galloping, ice shedding, lightning, distance between conductors | rigid, said |
| Views, tables, sagging tables, stringing charts, drawings | rigid (the sagging tables by definition: the structures as drawn are the structures as strung) |

**Never less safe than rigid** (owner, 2026-11-15): every check that
takes the level checks the rigid solve and the level-3 (or level-4) solve
of each case and keeps the worse. Choosing level 3 or 4 can only find
more, never less, than rigid; the change against rigid is said.

## What the results say

The notes of every result solved at level 3 or 4 begin with:

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
  real structures, so a clearance can be smaller than shown (level 4
  follows slack members; keep a margin for the rest, not modelled yet).

At level 4 they also say: the structures solved in full from level 3's
solution; the line solves at each level and the full solves; the largest
difference from level 3 (*level 4 against level 3: the points' moves
differ by up to … mm (T…), their forces by up to … %*); each structure
that failed at level 4 in a case, with why (*taken linear (level 3)
there; its usage in this case reported failing*); the structures without
a full model (taken linear in every case); a case whose Newton step had
to give way to staggered passes; and **NOT ON THE SAFE SIDE**: P-Delta on
the drawn geometry (no large displacements: a very flexible structure's
large rotations are not followed), rigid foundations, the stringing
state's deflection left out — the points may move less than the real
structures, so a clearance can be smaller than shown (not modelled yet:
keep a margin).

A rule that solves the wires by another method than the line (a rule's
ruling span on sections of finite elements, or the other way round) is
solved rigid, said: the structures' stiffness is set up for the line's
own methods.

## The Structure loads window's switch

The Structure loads window's **Structures flexible (level 3)** — **(level
4)** when the line's choice is full structures — follows the line's
choice; ticked or unticked there it sets that window's runs only (and the
loads sent to OHL-Structure while it is so), kept with the project's
setup, and its hover says the line's choice. Ticked back to the line's
choice, it follows the line again.

At level 4 the window's load cases run **one after the other**, each
case's structures side by side over the cores; the bar says the case,
the pass and the level of its answers, the structure being solved and
the last mismatch (*Solving the line: case 3 of 12 (pass 4 at level 4,
structure 17 of 71, mismatch 0.8 mm)*). **Stop** stops it between
structures.

## The reference state of the structures

*Built, in active development.* Owner (2026-11-16): *"Would it be
possible to define a weather case at which the structure is drawn in OHL
design where it would solve at level 4 and then keep it for later for
example level 2 solves. This could be useful to make the model match the
lidar scans."* The reference state is solved at **level 3**, or at
**level 4 when the line's choice is Full structures** (above): choose
*Full structures* there first, then **Solve and keep**. A state solved
at the other level is out of date (said) until solved again.

**Where:** Line › Criteria › Calculation options, under *Structures in the
wire solve*: **Reference case** (a weather case of the project, or none),
**Solve and keep**, **Remove**. Typical: the survey day's temperature and
wind.

1. Choose the case, then **Solve and keep**. The structures are solved
   in that case at level 3 (each structure with a model a linear
   stiffness at its attachment points, condensed at the stringing state,
   its own wind and ice of the case in its deflection; at level 4 from
   there each structure solved in full), on its own
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

**How towers are built** (owner, 2026-11-16: *"the tower should not be
initially built from a scan but from plans then adjusted based on the
movement of the feet and the lean of the weather case"*):

1. **From the design plans**: place each structure where the plans place
   it, with its type (its model) as designed — not at positions taken
   from a scan.
2. **Moved by the measured movements of its feet**: a settled foundation,
   a leg pushed out, a pole's tilted base ([Foot movements](#foot-movements)
   below).
3. **Leaning as in the reference case**: its deflection in the survey
   day's weather (above).

The window says so under *Reference state of the structures*. Points
placed from a scan already contain the lean: with a reference case it
would be counted twice.

## Foot movements

*Built, in active development.* The measured movements of a placed
structure's feet from the design, with their source and date.

**Where:** the structure's window (double-click it, or Edit on it) ›
**Foot movements (measured)**: the source (who measured, how) and the
date, then a row per foot — the foot (a support joint of its type's
model, chosen from the list; **all** for every foot alike: the structure
moved as one body; **base** for a pole), its movement across (to the
right of the line), ahead and up (mm), and its tilt to the right and
ahead (degrees). **Apply** keeps them with the structure: one step,
undone as one (Edit › Undo). On the command line: `ohl structures feet
LINE.ohl.json --structure T12 --foot "L0B:0,0,-12" --foot
"L0C:0,0,-4:0.05,0" --source "survey" --date 2026-05-12 --out
OUT.ohl.json` (mm, degrees; without `--foot` it lists them, `--clear`
takes them away). Kept in the line file (version 23).

**What they do:**

| Where | What |
|---|---|
| Every rigid (level 2) solve — the views, the tables, the clearances, the LiDAR comparison | without a reference state in use (none set, not solved yet, out of date or not checked): each structure's attachment points moved by its foot movements alone — the structure solved on its moved feet **without loads**, automatically, said in every result (*structures moved by their measured foot movements alone*); with a current reference state: by that state, which includes them |
| The reference state | solved on the moved feet: its points moved by the feet and by the case's deflection; the feet are part of what it was solved from — changed, it is out of date (*the foot movements changed since it was solved*) |
| Calculation level 3 | each structure's points moved by its feet in every case, on top of its own deflection |
| The 3D view | the structures drawn on their moved feet (at true scale: millimetres), said at its bottom left |
| The structure checks (Structure loads window's usage, OHL-Structure) | each case solved with and without the foot movements, **each member's larger usage** kept (below) |
| OHL-Structure | the line-values file carries them (version 11): the Loads tab lists them, the results solve the placed structure on its moved feet |

**Strain.** A structure standing on more feet than it needs — a lattice
tower on four, a portal — is strained when one foot moves apart from the
others (a differential settlement): its members take forces without any
load. A pole, on one base, only leans (and its weight and loads lean
with it: P-Delta). The structure checks keep per member the larger usage
of with and without the movements: the settlement's forces are counted
where they load a member and never taken to relieve one (on the safe
side, [theory](../theory/structure-flexibility.md#foot-movements)).

**Said, NOT ON THE SAFE SIDE where it applies:** a foot that is not a
support joint of the model (its movements are not taken, said with the
structure), single feet on a structure without a model (only **all**
can be taken there), a movement in a direction the support does not hold
(left out: the structure follows freely there), a tilt at a pinned foot
(it turns freely) — each leaves out a lean or a settlement's forces, so a
clearance or a usage can be worse than shown; give the movement at a
support joint to take it. The structure loads (the wires' forces) stay as drawn,
rigid (on the safe side, owner 2026-11-16).

## On the command line

`--structure-reference use|ignore` (every command reading a line, and
`ohl line`): `use` (default) checks the line's reference state against
the libraries and the run's weather cases and, current, takes it;
`ignore` the structures as drawn. Either way the run says which.
The reference case's wind is taken as OHL-Design shows it — a case
without its own direction **from the right** — so a state saved by one
program is current in the other; the command line's other checks keep
the weather file's side. A line with foot movements and no reference
state in use takes them alone (said), as in OHL-Design; `ignore` sets
them aside too. `ohl structures feet` lists, sets or clears a structure's
foot movements ([above](#foot-movements)).
`ohl line … --weather W --solve-reference "CASE"` solves one for the run
(each structure's largest move, the times printed) and `--save-reference
FILE` writes the line with it to a new line file (the input is not
changed).

`--structures-as rigid|linear|full` overrides the line's choice for one
run on `ohl line` (the weather cases solved again at level 3 or 4 and
printed after the rigid tables — at level 4 also each case's line solves
at each level, the full solves, the difference from level 3 and any
structure that failed there; with `--load-cases` the structure loads and
usage), `ohl tensions`, `ohl strings` and the other commands reading a
line (those that do not take the level say so and stay rigid). `ohl line
--calculation-level 2|3|4` is the same by the level's number; with
`--solve-reference` the reference state is solved at the level chosen
(3, or 4 for full). `ohl structures …` solves no line (its loads come
from `ohl line --loads-csv`, which takes the level).
