# Load cases per group

Status: built, in active development (not approved yet; owner,
2026-11-15). This page is the user guide of the window **Line ›
Structures › Load cases per group…**: which structures take which load
cases. The window's *user guide ↗* buttons open its sections.

**The idea in one sentence:** make the load cases once (Line ›
Structures › **Load cases…**, the [load case library](loads.md)), then
put the structures in **groups** and tick, per group, the cases its
structures take.

**Per position, not per type** (owner, 2026-11-15: "Okay for automatic
grouping but manual should always be available where load cases rules
are defined and on which pylon they apply with a list of them. Not per
type but per position"): picking structures by hand is the first way —
two structures of the same type can take different cases; the rules
(type, line angle, station, dead end) are an optional automatic helper.
The manual way is also where each case is defined: the Load cases form's
section **8 Applies to** lists every structure, says how it gets the case
and ticks it on or off — the case's **own list**
([below](#a-load-cases-own-list)), shown here as *wind (own list)*.

```
 Load case library (.ohllc)          Groups (in the line)               Each structure takes
 ┌──────────────────────┐   ┌─────────────────────────────────┐
 │ everyday             │   │ suspension   type 1A            │──► T2, T4: everyday, wind
 │ wind                 │──►│              everyday, wind     │
 │ ice                  │   │ angle        line angle 10–30°  │──► T3: everyday, wind, ice
 │ S1 broken            │   │              everyday, ice      │    (in "suspension" and "angle":
 └──────────────────────┘   │ dead ends    dead end           │     both groups' cases)
                            │              S1 broken, ice     │──► T1, T5: S1 broken, ice
                            └─────────────────────────────────┘
                              a structure in no group ─────────► every case of the library
```

## What a structure takes

| The structure is… | It takes |
|---|---|
| in one group | the cases ticked in that group |
| in two or more groups | the cases of all its groups together (the **union**: more cases, the safe side) |
| in no group | **every case** of the library |
| only in groups that tick no case | **no load case**: no loads computed, no structure check; listed as *not checked: takes no load case* in the usage table, never as passing, and said to OHL-Structure |
| picked in a case's own list | that case **on top of** the above (in no group it has every case already: the own list does not count as a group) |
| excluded in a case's own list | the above **without that case** — exactly that one (in no group: every case but it; excluded from all its cases: none, said as above) |

The line's own check (Checks › Structure loads) and OHL-Structure
(**Open in OHL-Structure…** on a placed structure) take exactly these
cases: OHL-Structure gets the structure's library cases, nothing else
([How load cases reach a structure](load-cases-to-structures.md)).

Nothing is solved again when the groups change: each load case is solved
once for the whole line; a structure not taking a case simply has no
loads in it.

## The window

The window has one list of groups and, for the group picked, its
settings in numbered sections. Every change is **one step of Edit ›
Undo**. A group is kept in the line (line file version 18), so it
travels with the project and the line file.

### 1 How it works

One line, the longer text behind the (?), and the line's summary:
*3 group(s) · 6 of 7 structure(s) in a group · 1 in no group: every case*.
Structures taking no load case are named in the warning colour.

A line written before 2026-11-15 that still holds the settings of the old
**Loads for structures** menu (weather cases sent unfactored, the wires'
condition for them, broken-wire cases sent) shows one note here: they are
kept as read and **no longer used**.

### 2 Groups

The table of the line's groups — **group** (its name; its description on
hover), **structures** (how many it holds now), **load cases** (how many
it ticks; *none* in the warning colour), **rules** (in words: *type 1A;
line angle 10–30°; dead end*; *— (picked only)* when it has none). Click a
name to pick the group. A load case's own list is a row too, *wind (own
list)*: **load cases** shows its case (in the error colour when it is no
longer in the library), **rules** *— the load case's own list (Load
cases… › Applies to)* with how many it excludes. Picked, it shows its
description, its picked and excluded structures (× takes one away; **Add
/ Remove the N selected** work as the form's ticks) and who takes the
case and how; it has no rules and no other case.

- **New group** — an empty group (*group 1*, *group 1 (2)*…): it holds
  nothing and ticks no case until you give it some.
- **Delete group** — the group picked is taken away. Its structures keep
  their other groups; a structure left in none takes every case.

### 3 Name and description

| Field | Meaning |
|---|---|
| name | Unique in the line. Shown wherever a structure's groups are listed: the Structures table, its Properties, the notes of the checks, OHL-Structure's status block. |
| description | Free text: what the group is for. A group made from the sets of before 2026-11-15 says here where it came from. |

A text is taken when you leave the field (or press Enter).

### 4 Structures picked

The first way: structures in the group by hand, per position, whatever
their type and whatever the rules say. Select structures in the
**Structures table** or in a **view** (Shift or Cmd/Ctrl + click for
several), then **Add the N selected**; **Remove the N selected** takes
the selected ones out again, and **×** next to a structure takes that one
out. A picked id that is no longer in the line is shown *(not in the
line)* and left out.

### 5 Rules (optional)

An optional, automatic helper to fill the group without picking. A
structure for which **every rule given holds** is in the group (bounds
included). A rule left empty does not count; a group without rules holds
only its picked structures. The rules are read from the line **as it is**:
move a structure past the angle given and it leaves (or joins) the group
at once; the usage notes and OHL-Structure's status block say which groups
a structure is in.

| Rule in the window | In the line file (`rules`) | Unit | Default | Meaning |
|---|---|---|---|---|
| types | `types` | — | any | Its structure type is one of those ticked. |
| line angle from / to | `angle_from_deg`, `angle_to_deg` | ° | no bound | The size of the angle the line turns at the structure, left or right (the largest over the alignments it stands on). A structure on no alignment has no angle and is not taken by an angle rule. |
| on alignment | `alignment` (its id) | — | any alignment | The alignment of the station rule; alone: the structure stands on it. |
| station from / to | `station_from`, `station_to` | m | no bound | Its station on that alignment (any alignment: on any it stands on). |
| dead end | `dead_end` | — | either | *a dead end*: a section of the line ends at the structure; *not a dead end*: none does. Read from the line's sections, not from the type's name. |

Structures picked by hand join the structures of the rules: a group with
rule *type 1A* and T7 picked holds every 1A structure and T7.

### 6 Load cases

A tick per case of the [load case library](loads.md), with **All** and
**None**. The ticked cases are taken by every structure in the group,
together with those of its other groups.

- **None ticked**: the group gives its structures no case — and a
  structure in no other group then takes **none**: no loads, no
  structure check, listed as not checked (never as passing).
- A ticked case **no longer in the library** (renamed or deleted there)
  is shown in the error colour: its structures are **not checked** in it
  — said in the usage table, the Structures table (⚠) and OHL-Structure's
  status block, never dropped silently. **Rename** in the Load cases
  library renames the case in every group at once.

### 7 Who is in

The structures in the group now: **structure**, **type**, **why**
(*picked*, *rules*, or both), **its groups** and **its load cases** — what
it takes from all its groups (the union), the cases the line's check and
OHL-Structure use. Below: the structures **in no group**, which take
every case.

## A load case's own list

*Built, in active development, not approved yet* (owner, 2026-11-15).
Each load case can have one **own list**, made and changed where the case
is defined — Line › Structures › Load cases… › the case › **8 Applies
to** ([Load cases](loads.md#applies-to-which-structures-take-the-case)) —
and shown in this window as the row *wind (own list)*. It is a group of
the line with two lists and nothing else:

| List | Meaning |
|---|---|
| **picked** | these structures take the case **on top of** what their groups give |
| **excluded** | these structures do **not** take the case, whatever their groups give |

It has no rules and gives no other case, and it does **not** count as a
group for *in no group: every case*: picking a structure in no group
changes nothing for its other cases (it keeps every case). So an own list
only ever adds its case, or takes away exactly that case — never more (the
safe side).

Ticking in **Applies to** writes this list: ticking a structure that does
not take the case **picks** it (and takes an exclusion away); unticking
one picked here takes it out; unticking one that gets the case through a
group — by a **rule** or picked in that group — or by being in no group
**excludes** it, and says how it took the case (*T5 excluded here (it took
it: group angle by rule (line angle 10–30°)) — change the rule in Load
cases per group, or keep the exclusion*). The groups and their rules are
never changed from the form.

An exclusion is never hidden: it is shown in the case's Applies to list
(*excluded here (else: …)*), in this window (the own list's row and its
*excluded* list; *its load cases* in **7 Who is in**, in the warning
colour), in the Structures table (*— wind excluded in the own list*), in
the structure's Properties (in the warning colour) and in OHL-Structure's
status block. A structure excluded from every case it had takes none and
is listed as *takes no load case*, never as passing.

An own list left empty (nothing picked, nothing excluded, no description)
is taken away. **Delete group** on it gives its excluded structures the
case back and takes it from its picked ones (unless a group gives it).
**Rename** of the case in the library renames its own list in the same
command; a deleted case's own list stays, in the error colour: its picked
structures are **not checked** in it (said in the checks' notes).

## Where else you see the groups

- **Structures table › load cases**: the structure's groups and cases —
  `suspension, angle: 3: everyday, wind, ice`; `no group: every case`;
  `none: takes no load case (dismantled)`; with own lists
  `suspension, ice (own list): 2: everyday, ice` (picked for ice),
  `suspension: 1: everyday — wind excluded in the own list`,
  `no group: every case but wind (excluded in the own list)`. A case not
  in the library begins the cell with ⚠ in the error colour.
- **A structure's Properties › Load cases**: its groups and the cases
  they give it (shown only), the own lists picking or excluding it, with
  **Change its groups…** opening this window at its first group.
- **Checks › Structure loads**: per case, *taken by N of M structures
  (their groups); not by: …*; the structures taking no case are listed
  as not checked.
- **OHL-Structure**: its status block says *Groups: T7 is in suspension,
  angle — it takes their load cases: …*.

## Lines and projects of before 2026-11-15

Before 2026-11-15 a structure could give its **own set** of load cases
(its Properties › Load cases), a structure type a **default set**
(Line › Criteria › Load cases per structure type), and a structure could
take **none**. Such a line is read into groups when it is opened, and
written back in the new form (line file version 18):

| Before | Becomes |
|---|---|
| structure type X's set | group **type X**, rule *type X*, the same cases |
| a structure's own set | a group picking it, **named after it** (`T7`; several structures with the same set: one group, `T7, T8` or `T7, T8 and 3 more`) |
| structures taking none | group **no load case**, picking them, ticking no case |

Each group's description says where it came from. Because a structure
takes the **union** of its groups, a structure that had its own set and
whose type had one now takes **both** sets — more cases, on the safe
side; a structure that took none and whose type had a set now takes its
type's cases. Look at the window's *7 Who is in* after opening an older
project, and untick what does not apply.

The settings of the old **Line › Criteria › Loads for structures** menu
(weather cases sent unfactored, the wires' condition for them,
broken-wire cases sent) are **kept as read and no longer used**; one note
says so (here and in Checks › Structure loads). OHL-Structure gets the
structure's library cases only; an extra load case there takes the
unfactored forces of any of them, broken-wire cases included.

## In the line file

```json
"structure_groups": [
  { "name": "angle 10–30°", "description": "angle towers",
    "rules": { "angle_from_deg": 10.0, "angle_to_deg": 30.0 },
    "load_cases": ["everyday", "wind", "ice"] },
  { "name": "crossing", "structures": ["T6", "T7"], "load_cases": ["wind"] },
  { "name": "dismantled", "structures": ["T9"] },
  { "load_case": "ice", "structures": ["T4"], "excluded": ["T5"] }
]
```

| Key | Meaning |
|---|---|
| `name` | unique (absent on a load case's own list) |
| `description` | free text (optional) |
| `structures` | ids picked by hand (optional) |
| `rules` | `types`, `angle_from_deg`, `angle_to_deg`, `alignment`, `station_from`, `station_to`, `dead_end` — each optional, all given must hold |
| `load_cases` | names of cases of the load case library; absent or empty: its structures take none from it |
| `load_case` | set: the group is that case's **own list** (one per case; no `name`, `rules` or `load_cases`) |
| `excluded` | own list only: ids that do not take its case (an id both picked and excluded is refused) |

A line with a group is written at version 18 (an older program refuses it
rather than giving every structure every case); the own lists came in the
same, unreleased raise. In OHL XML:
`<structure_group name description><structure id/><rules …><type id/></rules><case name/></structure_group>`,
an own list `<structure_group load_case><structure id/><excluded id/></structure_group>`
([ohl-line.xsd](../schema/ohl-line.md)). Details: [data model](../development/data-model.md).

See also: [How load cases reach a structure](load-cases-to-structures.md),
[Load cases](loads.md), [OHL-Structure](structure-app.md).
