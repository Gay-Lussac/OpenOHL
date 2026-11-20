# Towers shared by several routes

*In active development (country server, phase 8): built, not approved yet.*

*On a network's server* (phase 11) the same rules apply to a **branch
over a working set** — its routes are the working set — and to its merge
into main: see
[Branches over a working set](server.md#branches-over-a-working-set-phase-11-in-active-development).
Edits made directly on a network's main line are decided by the rights,
without these rules yet.

A tower often carries the wires of more than one route: a branch leaves
a line at a Y pylon, two routes run on the same towers for a while, a
jumper joins their wires. When several people work on one project, each
on their own routes, a change to such a tower must not cut the other
routes' wires off.

## Your working set

**View › Editable routes (working set)** lists the project's routes (shown
when the project has more than one and you can edit it). Untick the
routes you do not work on: the ticked ones are your **working set**.
Ticking every route again, or **Every route (no working set)**, goes back
to working without one — the project then behaves exactly as before:
nothing is refused or written that was not before.

The working set is yours (another user of the project has their own) and
is kept in the project file. A [part of a line](../development/data-model.md#a-part-of-a-line-ols_meta-key-part-2026-11-18)
written into a file of its own has its working set built in.

## What belongs to which route

- A **tower** is on the routes it names itself, else on those of the
  alignment points tied to it and of every alignment passing within 5 m
  of it (counted generously: a route counted too many protects more).
- A **section** is on the routes all its towers share; one running from
  one route into another is on both.
- A **jumper** is on the routes of the sections it joins; a jumper
  between two routes is on both.

An object is **another route's** when one of its routes is not in your
working set.

## What is refused

Each refusal says which route and which of its objects stop you, and
the rule:

| Rule | Refused |
|---|---|
| — | changing, making or deleting another route's sections, jumpers, spacers, alignments, alignment points or the route itself; changing a tower that only the other routes use |
| 1 | deleting or renaming a tower another route uses |
| 2 | removing or leaving out a wire point another route's section or jumper ends on or passes through (also by a set renumbered or the structure replaced) |
| 3 | changing what such a wire point hangs on: its strings, their tops, the points above |
| 4 | turning such a wire point from suspension to strain or back, or from hanging to fixed |
| 5 | changing the strings holding another route's jumper support clamp |
| 6 | taking another route off the tower, untying its alignment point from it, or moving the tower away from that route's alignment |
| 7 | deleting or renaming a tower named in another route's circuit start or in a structure group with another route's towers |
| 8 | anything else that would leave another route's objects not valid (the message names the routes) |
| §7.4 | changing a string or wire point only the other routes use (their hardware) |

A **typed** tower takes its wire points and strings from the structure
library, which these checks do not read: with another route on it, its
hardware may move (the tower moved or turned, a point or string moved,
an insulator changed) but not change its make-up — which points,
strings, wire points and sets it has, what hangs on what, strain or
suspension, its omissions, type and extension: such a change is refused
(on the safe side).

To make such a change, tick the other route too — then the change and
the other route's adjustment go together, as one edit.

## What is allowed: results out of date

Moving or turning a shared tower, its height, foundations and local
factors, moving a shared string or wire point (its strings and their
tops kept), changing a string's insulator: allowed, because the other
route stays connected. Its results are then **out of date**:

- the status line says it after the edit ("Shared towers: route B's
  results are out of date (T12 moved 0.80 m).") and the operation log's
  entry keeps it;
- the **Routes** table and View › Editable routes list every route out of
  date with its last change, who made it and the log entry;
- a user with that route in their working set (or with none) checks it
  again and clicks **Mark route … up to date**.

A change of the line's settings or library files marks every route
outside your working set out of date.

## Protected project lines

On a [protected project line](project-lines.md) (changed by admins or
through reviews), a change that would move another route's wires is
refused even for an admin with a working set: it needs a review by the
route's owners. Make the change on a project line of your own and submit
it for review into the protected one ([Versions, branches and
reviews](versions.md)).

## Merges

A merge where one side changed a tower and the other strung or changed a
wire on it that the change breaks (rules 1–5) shows a **conflict** on
the tower: keep the tower's change (the other side's wire on it is then
not taken) or the tower as it was. Without working sets in the project
this conflict appears only where the merged line would not be valid —
before, such a merge was refused with nothing to choose; with working
sets, always.
