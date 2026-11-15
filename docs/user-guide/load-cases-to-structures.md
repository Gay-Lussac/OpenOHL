# How load cases reach a structure

Status: built, in active development (not approved yet). This page
explains, for a line engineer, how the load cases of the line become the
loads a structure is checked with: in OHL-Design (the line) and in
OHL-Structure (one placed structure). Each step has its own page; the
links say where.

**The model** (owner, 2026-11-14): *the line transfers what the wires
do* — the forces at the attachment points, per load case, per wire set,
of the back and the ahead spans. *The structure side decides what acts
on the structure* — factors, the cases it takes, shares of the back and
ahead pull, points left out — *and checks it*. A construction stage (a
phase not yet strung, a reduced pull) is a load case made in
OHL-Structure, not a setting of the line.

**Made simple** (owner, 2026-11-15: "simply defining load cases and then
have them attributed in groups of pylons"): the load cases are made once,
in the library; **groups of structures** say which structure takes which
([Load cases per group](load-cases-per-group.md)); OHL-Structure gets a
structure's **library cases only**. The old Line › Criteria › *Loads for
structures* menu (weather cases sent unfactored, broken-wire cases sent),
the per-structure sets and the per-type sets are gone; older lines are
read into groups.

## The words

Each word means one thing, everywhere in OpenOHL.

| Word | What it is | Where it is kept |
|---|---|---|
| **Weather case** | Wind, ice and temperature. No factors. | The weather case file (`.ohlwc`) of the project ([Weather cases](weather.md)). |
| **Load case** | A weather case, the wires' condition (as strung, after creep…) and factors (vertical, transverse, longitudinal, strength φ); optionally the wind, ice and snow on the structures, broken or removed wires, *use local conditions*. Written in the Load cases library form or by a design-code plugin. | The load case library (`.ohllc`) of the project ([Load cases](loads.md)). |
| **Structure group** | Structures — picked by hand (per position, the first way), or by rules (types, line angle, station, dead end: an optional helper) — and the load cases of the library they take. A structure takes the cases of all its groups; in no group, every case; in groups ticking none, none. | The line file (`structure_groups`, version 18). |
| **Own list** (of a load case) | The structures picked for, or excluded from, one load case, ticked where the case is defined (Load cases… › 8 Applies to). Picked: the case on top of their groups'; excluded: not that case, whatever their groups give. | The line file, as a structure group with `load_case` (version 18). |
| **Library case** (in OHL-Structure) | A load case of the structure's groups, rebuilt as a load case of OHL-Structure with the library's factors (the wires' forces, the self weight, the wind, ice and snow on the body). Read-only there. | The line-values file `<type>.<structure>.line-values.json`, written by OHL-Design. |
| **Extra load case** | A load case you make in OHL-Structure: *1. line forces* (a library case's **unfactored** forces × your factors, at the points you choose, with shares of the back and ahead pull — broken-wire cases too) and *2. loads on the structure itself*. | The structure type file (`.ohls`); `.ohlslc` only to copy cases. |
| **Typed values** | Numbers typed over the load table (a client's load tree, a value checked by hand). They replace the computed value of that row. | `<type>.<structure>.loads.csv`, per placed structure, kept in the project. |

## The flow

![Left, OHL-Design: weather cases feed the load case library; each structure takes the cases of its groups; the wires are solved in each case and the line checks the structures. One arrow goes to OHL-Structure: the structure's library cases (their unfactored forces and the library's factors). Right, OHL-Structure: the library cases, the extra load cases taking their unfactored forces with your factors, and typed values go into the load table, which is solved and checked](../images/theory/load-cases-flow.svg)

1. **Weather cases** give the wind, ice and temperature.
2. The **load case library** turns a weather case into a load case: it
   adds the wires' condition, the factors and the extras.
3. **① Groups.** Each structure takes the cases of the groups it is in
   (two groups: both groups' cases); in no group, every case. A structure
   whose groups tick no case takes none and is listed as *not checked*,
   never as passing ([Load cases per group](load-cases-per-group.md)).
4. **② Wires solved.** Every section is solved in each load case. The
   force at each attachment point is kept for the structures taking
   that case.
5. **③ The line's check** (Checks › Structure loads) uses these forces
   with the library's factors: allowable spans, allowable loads, the
   finite-element model, the pole. It gives the line's usage table.
6. **④ Open in OHL-Structure…** on a placed structure writes its
   line-values file. The cases of its groups arrive as **library
   cases**, with the same factors as in ③, and with their unfactored
   forces per point: an **extra case** of yours takes the forces of any
   of them (broken-wire cases too) with your own factors. Nothing else
   arrives (since 2026-11-15).
7. **⑥ OHL-Structure** solves the load table — library cases and extra
   cases, with typed values on top — and checks the members, the pole
   and the joints. This verdict is OHL-Structure's; the line's usage
   table does not show it.

## What each window does

| Window | What you do there |
|---|---|
| Line › Criteria › **Weather cases** | The weather. No factors here. |
| Line › Structures › **Load cases…** | The load cases: weather case, wires, factors, broken and removed wires, wind, ice and snow on the structures, *use local conditions* ([the form](loads.md#the-load-cases-library-a-form)); and in its **8 Applies to**, which structures take each case — a list of every structure with how it gets the case, ticked on or off per structure ([Applies to](loads.md#applies-to-which-structures-take-the-case)). |
| Line › Structures › **Load cases per group…** | The groups of structures and the cases each takes ([its page](load-cases-per-group.md)). The Structures table's **load cases** column and a structure's **Properties › Load cases** show its groups and cases. |
| Checks › **Structure loads** | The line's check of every structure, in the cases ticked there. **Use local conditions** applies the structures' local factors to every case. |
| **Open in OHL-Structure…** (a placed structure) | Starts OHL-Structure with this place's loads. |
| OHL-Structure: **Load cases** panel | The library cases first (*From the line's load case library*, read-only), then your extra cases. |
| OHL-Structure: **Load case editor** | Your extra cases: line forces × factors, loads on the structure itself. |
| OHL-Structure: **Load table**, **Line loads** | Every row the cases give, and what the line sent, per point and case. Type values over the table here. |
| OHL-Structure: **FE results** | The solve and the checks of every case of the table. |

## Where the factors are applied

- **A library case:** its vertical, transverse and longitudinal factors
  multiply the components of the wires' forces at each point; its wind
  factor the wind on the body; its vertical factor the self weight, the
  ice and the snow; its strength factor φ the capacities. The line's
  check (③) and OHL-Structure (④) apply the same numbers. The factors
  act on components, not on actions
  ([Load cases](loads.md#load-case-files-ohllc)).
- **An extra case:** takes a library case's **unfactored** forces and
  gives its own factors, in its *1. line forces* row.
- **Typed values:** taken as typed. They replace the computed value of
  that row; they are not multiplied again.

**Local conditions.** Checks › Structure loads › **Use local
conditions** (kept with the project) applies the structures' local wind
and ice to every case. OHL-Structure follows it: with it ticked, the
cases a placed structure gets take the local conditions too, as in the
line's check, and its status block says so.

An **older line-values file** (written before 2026-11-15) may still hold
unfactored weather cases sent on their own: OHL-Structure runs them as
before (every factor 1, their body wind as pressure × area without drag
or shielding) and its status block says they are **NOT ON THE SAFE
SIDE** and that the line no longer sends them: open the structure again
from OHL-Design to get its library cases.

## Which cases a structure takes

The **union of the cases of its groups** ([Load cases per
group](load-cases-per-group.md)):

- In one group: that group's cases. In two or more: all their cases
  together (more cases: the safe side).
- In no group: every case of the library.
- In groups ticking no case only: none — no loads, no check, listed as
  *not checked: takes no load case* in the usage table and said to
  OHL-Structure.
- Membership follows the line: a group by rules (types, line angle,
  station, dead end) gains or loses structures as the line is edited;
  the usage notes and OHL-Structure's status block say which groups a
  structure is in.
- Then each case's **own list** (Load cases… › 8 Applies to): picked
  there, a structure takes that case too (in no group it has every case
  already); excluded there, it does not take that case — exactly that
  one, said in the Structures table, its Properties, the groups window
  and OHL-Structure's status block
  ([A load case's own list](load-cases-per-group.md#a-load-cases-own-list)).
- Per position, not per type: two structures of the same type take
  different cases when ticked differently.

A case ticked in a group but no longer in the library (renamed or
deleted there after the group was made) is never dropped silently: the
structure is **not checked** in that case — said in the usage table
(never a pass), in the Structures table (⚠, in the error colour) and in
OHL-Structure's header. A structure whose groups tick only such cases is
listed as not checked in every case. **Rename** in the Load cases library
renames the case in every group at once, so a rename never drops a
structure.

A library case that does not compute (it names a weather case, section
or span that is not there) fails on its own: the others still reach
OHL-Structure, and the header of each structure taking the faulty one
says *NOT SENT: library case "x" does not compute* with why. (The line's
own check stops with the error and says it.)

**An extra case naming a case that is not sent** (a weather case sent on
its own before 2026-11-15, or a library case the structure's groups do
not take): its line-force rows have no value and are left out of the run
— **NOT ON THE SAFE SIDE** — and OHL-Structure's status block says so in
the warning colour, naming the case and the cases that are sent. Pick
one of the library cases in its *1. line forces*.

## Two checks, apart

The line's usage table (③) and OHL-Structure's results (⑥) are made
apart. OHL-Structure adds your extra cases, typed values and checks the
line does not make (joints and plates). Its results stay in
OHL-Structure: a line showing all structures below 100 % does not mean
every structure passes in OHL-Structure. Open each structure you design
in OHL-Structure and read its results there.

## Names you will see

- `ice (span 2 broken)`: one break of a library case breaking every span
  in turn; each break is a case of its own in OHL-Structure.
- `wind (library)`: the library case `wind`, renamed because one of your
  extra cases is also called `wind`.
- In older line-values files only: `storm — wind from the left` (a
  weather case sent on its own, unfactored) and `everyday (weather
  case)` (one so renamed because the library had a case of its name).

A value typed over a row whose case was later renamed lands on no row:
it is **not run**, and the Load table lists it with **Move** to the case
it belongs to ([OHL-Structure › Load cases](structure-app.md#load-cases)).

See also: [Load cases](loads.md), [Load cases per
group](load-cases-per-group.md), [OHL-Structure ›
Started for a placed structure](structure-app.md#started-for-a-placed-structure),
[Structure load cases](structure-load-cases.md).
