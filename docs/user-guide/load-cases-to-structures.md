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

## The words

Each word means one thing, everywhere in OpenOHL.

| Word | What it is | Where it is kept |
|---|---|---|
| **Weather case** | Wind, ice and temperature. No factors. | The weather case file (`.ohlwc`) of the project ([Weather cases](weather.md)). |
| **Load case** | A weather case, the wires' condition (as strung, after creep…) and factors (vertical, transverse, longitudinal, strength φ); optionally the wind, ice and snow on the structures, broken or removed wires, *use local conditions*. Written in the Load cases library form or by a design-code plugin. | The load case library (`.ohllc`) of the project ([Load cases](loads.md)). |
| **Case set** | The load cases one structure takes: its own list, else its type's list, else every case of the library — or *none*. | The line file (per structure, per type). |
| **Line result** | A weather case sent to the structures *unfactored*: the wires' forces at each attachment point, every factor 1. A weather case with wind is sent twice: `<case> — wind from the left` and `— wind from the right`. | Ticked in Line › Criteria › Loads for structures. |
| **Library case** (in OHL-Structure) | A load case of the structure's set, rebuilt as a load case of OHL-Structure with the library's factors (the wires' forces, the self weight, the wind, ice and snow on the body). Read-only there. | The line-values file `<type>.<structure>.line-values.json`, written by OHL-Design. |
| **Extra load case** | A load case you make in OHL-Structure: *1. line forces* (a line result or a library case × your factors, at the points you choose, with shares of the back and ahead pull) and *2. loads on the structure itself*. | The structure type file (`.ohls`); `.ohlslc` only to copy cases. |
| **Typed values** | Numbers typed over the load table (a client's load tree, a value checked by hand). They replace the computed value of that row. | `<type>.<structure>.loads.csv`, per placed structure, kept in the project. |

## The flow

![Left, OHL-Design: weather cases feed the load case library; each structure takes its case set; the wires are solved in each case and the line checks the structures. Two arrows go to OHL-Structure: the structure's library cases (with the library's factors) and the weather cases ticked in Loads for structures (unfactored line results). Right, OHL-Structure: line results feed extra load cases; library cases, extra cases and typed values go into the load table, which is solved and checked](../images/theory/load-cases-flow.svg)

1. **Weather cases** give the wind, ice and temperature.
2. The **load case library** turns a weather case into a load case: it
   adds the wires' condition, the factors and the extras.
3. **① Case set.** Each structure takes the cases of its set: its own,
   else its type's, else every case. A structure set to *none* takes no
   case and is listed as *not checked*, never as passing.
4. **② Wires solved.** Every section is solved in each load case. The
   force at each attachment point is kept for the structures taking
   that case.
5. **③ The line's check** (Checks › Structure loads) uses these forces
   with the library's factors: allowable spans, allowable loads, the
   finite-element model, the pole. It gives the line's usage table.
6. **④ Open in OHL-Structure…** on a placed structure writes its
   line-values file. Its case set arrives as **library cases**, with the
   same factors as in ③.
7. **⑤ Loads for structures.** Weather cases ticked there arrive too,
   unfactored, as **line results**: ingredients for your own extra
   cases.
8. **⑥ OHL-Structure** solves the load table — library cases, extra
   cases, and line results, with typed values on top — and checks the
   members, the pole and the joints. This verdict is OHL-Structure's;
   the line's usage table does not show it.

## What each window does

| Window | What you do there |
|---|---|
| Line › Criteria › **Weather cases** | The weather. No factors here. |
| Line › Criteria › **Load cases library** | The load cases: weather case, wires, factors, broken and removed wires, wind, ice and snow on the structures, *use local conditions* ([the form](loads.md#the-load-cases-library-a-form)). |
| Line › Criteria › **Load cases per structure type…** | The set of each structure type. |
| A structure's **Properties › Load cases** | Its own set (*as its type*, *its own* ticks, or *none*). The Structures table's **load cases** column shows every structure's set. |
| Checks › **Structure loads** | The line's check of every structure, in the cases ticked there. **Use local conditions** applies the structures' local factors to every case. |
| Line › Criteria › **Loads for structures** | The weather cases sent unfactored (line results), the wires' condition for them, and the broken-wire cases sent. The library cases go whatever is ticked here. |
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
- **A line result:** no factor (every factor 1). An extra case that
  takes it gives the factors, in its *1. line forces* row.
- **Typed values:** taken as typed. They replace the computed value of
  that row; they are not multiplied again.

A line result run as a case of its own in OHL-Structure has every factor
1: its usage is not a design check. Read as one it is **NOT ON THE SAFE
SIDE**. Use the library cases, or an extra case with your factors.

## Which cases a structure takes

The order is **its own set ► its type's set ► every case**:

- A structure with its own ticks takes exactly those.
- Without its own, it takes its type's (Line › Criteria › Load cases per
  structure type).
- Without either, it takes every case of the library.
- Set to *none*, it takes no case: no loads, no check, listed as *not
  checked: takes no load case* in the usage table and said to
  OHL-Structure. The line results are still sent to it.

A name in a set that is not in the library (renamed or deleted there
after the set was made) is never dropped silently: the structure is
**not checked** in that case — said in the usage table (never a pass),
in the Structures table (⚠, in the error colour) and in OHL-Structure's
header. A structure whose set names only such cases is listed as not
checked in every case. **Rename** in the Load cases library renames the
case in every set at once, so a rename never drops a structure.

A library case that does not compute (it names a weather case, section
or span that is not there) fails on its own: the others still reach
OHL-Structure, and the header of each structure taking the faulty one
says *NOT SENT: library case "x" does not compute* with why. (The line's
own check stops with the error and says it.)

## Two checks, apart

The line's usage table (③) and OHL-Structure's results (⑥) are made
apart. OHL-Structure adds your extra cases, typed values and checks the
line does not make (joints and plates). Its results stay in
OHL-Structure: a line showing all structures below 100 % does not mean
every structure passes in OHL-Structure. Open each structure you design
in OHL-Structure and read its results there.

## Names you will see

- `storm — wind from the left`, `storm — wind from the right`: the two
  line results of weather case `storm`.
- `high wind — S3 span 2 broken — wind from the left`: a broken-wire
  case sent as a line result (Loads for structures › Broken-wire cases
  sent).
- `ice (span 2 broken)`: one break of a library case breaking every span
  in turn; each break is a case of its own in OHL-Structure.
- `wind (library)`: the library case `wind`, renamed because one of your
  extra cases is also called `wind`.

See also: [Load cases](loads.md), [Loads for
structures](desktop-app.md#loads-for-structures), [OHL-Structure ›
Started for a placed structure](structure-app.md#started-for-a-placed-structure),
[Structure load cases](structure-load-cases.md).
