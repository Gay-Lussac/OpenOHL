# The Structure loads window

Checks › **Compute structure loads** opens the **Structure loads**
window: the loads the wires and strings put on each structure in each
load case it takes, and how much of its capacity each structure uses
([Structure loads](loads.md), [Structure usage](loads.md#structure-usage-capacity-checks)).
The window follows the [check pages' layout](../development/check-pages.md):
a **Setup** tab in numbered sections, a **Results** tab, and **Run** in
the footer. Built, in active development, not approved yet.

**Nothing is computed when the window opens** (2026-11-15): a run starts
only when you press **Run**. The results of an earlier run in the session
stay shown, with an *Out of date* banner when the line, a weather case or
a library (the load case library among them) changed since that run.

The window's setup is kept with the project, per project line (the
project setting `app.structure_loads`): Use local conditions, the groups
not run, the load cases run for the structures in no group and the
structures ticked or unticked.

## 1 What it checks

The loads at every point a wire or string hangs from, transverse,
longitudinal and vertical in the structure's axes, factored by the load
case; then the structure's usage: its type's allowable spans and loads,
its structure model (finite elements) and its pole. *How it is computed*
folds out the method and a drawing of the loads at a structure point.

## 2 Load cases

The load case file of the project (Line › Structures › **Load cases…**,
with the **Load cases library…** button) and every case in it, with:

- its weather case;
- **taken by**: how many structures take it (their groups, its own list)
  and how many this setup computes;
- its **state**: *computed on Run*; *not computable: no weather case "…"
  in the project* (in the error colour — add the weather case in Line ›
  Criteria › Weather cases, or with the library's **Add the default
  cases…**, which adds the default weather cases the default load cases
  name, see below); *taken by no structure* (e.g. the 60 N/m ice of the
  default cases on a line below 1000 m); *not computed* by the setup
  (3, 4).

The list is read again whenever the library is saved in its page (or the
file changes): a case added, edited, renamed or deleted there is listed
at once, without opening the project again (owner report 2026-11-15).

## 3 Structure groups

The groups of Line › Structures › [Load cases per group](load-cases-per-group.md),
each with:

- a **run** tick (ticked by default): unticked, the structures it holds
  are not computed in this run;
- how many structures it holds (and how many are computed) and the load
  cases it gives;
- its structures (folded), as tick lists: those on an alignment first,
  then, under **Not on an alignment (not computed unless ticked)**, those
  on none.

A structure takes the cases of every group it is in. A structure in a
group run and in a group not run is computed for the cases of the groups
run only (said in 5). Below the groups, the **load cases' own lists**
(Load cases… › Applies to): the structures each picks, its rule (e.g.
ground elevation ≥ 1000 m) and the structures it excludes — they add
their case to, or take it from, those structures whatever their groups
give. The line's groups are never changed in this window.

## 4 Structures in no group

A structure in no group takes every case of the library (less the cases
whose own list excludes it). Here:

- **Load cases run for them**: a tick list of the library's cases (all
  ticked by default, also cases added later). It is a choice of this
  window, kept with its setup in the project — the line's groups are not
  changed;
- **Structures**: the structures in no group, on an alignment first, then
  those on none (left out unless ticked);
- **Make these a group…** (optional): selects the structures in no group
  that are ticked and opens Load cases per group, where **New group** and
  **Add the selected** make them a group with cases of its own.

**Structures on no alignment** (owner 2026-11-15: "put the structures
outside of the alignment by default out of the calculations and all at
the bottom of the list"): a structure with no station on any alignment is
not computed unless ticked, wherever it is listed; the results list it as
*not computed (not on an alignment, unticked)*, never as passing.

## 5 Before Run

What Run computes: the structures computed, the load cases solved and the
structure checks (structure × case); then what is left out, with why —
the cases not computable, the structures in a group run and one not run,
the structures not computed (their groups not run, unticked, not on an
alignment, on another route than the one worked on, no case ticked for
them) and those taking no load case. The footer repeats the counts and
the time estimated from the last run.

**A run** has two stages, shown by the bar in the footer with the time
gone and the time left in the stage:

1. **Solving the line**: every section solved in each load case needed
   ("Solving the line: case 3 of 9"), the load cases in parallel;
2. **Structures**: each structure checked in every case it is computed
   for ("Structures: 41 of 73"), the structures in parallel; the member
   checks of a structure model once per case, on the solved forces —
   never inside the solver's iterations.

The answer is the same, to the last digit, as one case and one structure
after the other. **Stop** ends the run between cases or structures; the
previous results stay.

## Results

**Loads** (one load case at a time, factored or unfactored, a filter by
structure, CSV export of the case or of every case) and **Usage** (per
structure, the governing case or one case; colour the structures by
usage in the views). The loads table's **Given by** column says which
groups gave each structure the case (*no group (every case)*, a group's
name, or the case's own list).

Above them, in a box of its own height, what the run did not compute:
each load case not computed with why and the structures not checked in
it, and the structures left out with why. A structure that was to be
computed in a case that could not be computed is **not checked** in the
governing usage (grey), never a pass.

## Add the default cases… and the weather cases

The Load cases library's **Add the default cases…** adds OpenOHL's
default load cases the file lacks. They name the default weather cases;
when the project lacks some, the tick **Also add the default weather
cases they name that the project lacks** (on by default) adds those
weather cases to the project's weather case file at once — its own
change, one undo step (a weather case of the same name is never
replaced). Unticked, only the load cases are added, and the window lists
each of them as *not computable* until its weather case exists.
