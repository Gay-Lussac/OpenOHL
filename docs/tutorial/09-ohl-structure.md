# 9. Loads for the structures

**Goal:** say which structures take which load cases, and see how a
structure's own load case is made from the line's forces in
OHL-Structure.

How it works, in one line: **the load cases are made once** (Line ›
Structures › Load cases…, the starter has *everyday*, *wind* and *ice*);
**groups of structures** say which structure takes which; a placed
structure gets the cases of its groups in OHL-Structure, and **your own
load cases** there take their unfactored forces with your factors
([How load cases reach a structure](../user-guide/load-cases-to-structures.md)).

```
load case library ──► groups of structures ──► each structure: its groups' cases
(everyday, wind, ice)  (picked, or by rules)    (in no group: every case)
                                            ──► OHL-Structure: a load case =
                                                  1. line forces (a library case's unfactored forces × your factors)
                                                  2. loads on the structure itself (self weight, wind on its body, ice, point loads)
```

## Steps

1. Click **T2** (a suspension structure) to select it.
2. **Line › Structures › Load cases per group…** opens the window.
   - **New group**; in *3 Name and description* type the name
     **suspension** (Enter).
   - *5 Rules*: tick **type starter-suspension** — every suspension
     structure is in the group, now and when you add more.
   - *6 Load cases*: tick **everyday** and **wind**.
   - *7 Who is in* lists the suspension structures and the cases each
     takes; below it, the tension structures, in no group, take every
     case.

   ![The Load cases per group window: the group suspension, rule type
   starter-suspension, everyday and wind ticked, and the structures in
   it](../images/tutorial/tutorial-load-cases-per-group.png)

3. Close the window. The Structures table's **load cases** column now
   reads *suspension: 2: everyday, wind* for the suspension structures
   and *no group: every case* for the others.
4. Hold **Edit** and click **T2**; in its Properties, **Open in
   OHL-Structure…** opens it with the loads the line puts on it
   (OHL-Structure must be built next to OHL-Design).
5. In OHL-Structure: **Loads › Load case editor** (a window of its
   own).
   - **+ Case**; name it *wind, my factors* and type a description, e.g.
     *Invented factors for the tutorial*.
   - **1. Line forces › Add line forces**: choose the library case
     **wind** (its unfactored forces) and type the factors, e.g.
     vertical **1.35**, transverse **1.5**, longitudinal **1.0**
     (invented). The sentence under the description now says what the
     case does.
   - **Points (0 off)**: untick one attachment point — its line forces
     are left out (a conductor taken down for works); tick it again.
6. **Loads › Load table**: the rows each case makes, one per
   point and axis, marked *line*.

## What you should see

In OHL-Design, the group *suspension* holds the suspension structures,
which take *everyday* and *wind*; the tension structures take every case.
In OHL-Structure, T2 has the library cases *everyday* and *wind* first
(read-only), then your case, whose sentence reads like *wind: vertical ×
1.35, transverse × 1.5, longitudinal × 1.0 at every point*, and its rows
in the load table.

**What just happened:** the load cases live in one place, the library;
the groups say who takes them, and follow the line as it changes; the
factors of your own cases live in the structure's load cases, and each
case says in words what it does. The starter's structures are of level 1
(points and strings, no members): the **loads on the structure itself**
— the wind on its body, ice on its members — need a structure with
members (levels 2 and 3), whose own weight and wind the finite-element
check then takes. See [Load cases per
group](../user-guide/load-cases-per-group.md) and [Structure load
cases](../user-guide/structure-load-cases.md).

Next: [10. Saving and what next](10-saving.md).
