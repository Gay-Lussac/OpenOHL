# 9. Loads for the structures

**Goal:** send the line's forces to its structures, and see how a
structure's load case is made from them in OHL-Structure.

How it works, in one line: **the line gives forces** at each attachment
point, unfactored; **the structure's load cases** take them with their
factors and add the loads on the structure itself.

```
weather cases ticked ──► line results (forces at each attachment point, unfactored)
                         a case without wind: one; with wind: from the left and from the right
                     ──► OHL-Structure: a load case =
                           1. line forces (a line result × vertical, transverse, longitudinal factors)
                           2. loads on the structure itself (self weight, wind on its body, ice, point loads)
```

## Steps

1. **Criteria › Loads for structures** (a menu of ticks; it stays open
   while you tick).
   - **Wires**: leave *as strung*.
   - **Weather cases sent**: tick **everyday** and **high wind**.
2. Close the menu (a click beside it, or Escape).
3. Hold **Edit** and click **T2** (a suspension structure);
   in its Properties, **Open in OHL-Structure…** opens it with the loads
   the line puts on it (OHL-Structure must be built next to OHL-Design).
4. In OHL-Structure: **Load cases › Load case editor…** (a window of its
   own).
   - **+ Case**; name it *wind from the left* and type a description,
     e.g. *Invented factors for the tutorial*.
   - **1. Line forces › Add line forces**: choose the line result
     **high wind — wind from the left** and type the factors, e.g.
     vertical **1.35**, transverse **1.5**, longitudinal **1.0**
     (invented). The sentence under the description now says what the
     case does.
   - **Points (0 off)**: untick one attachment point — its line forces
     are left out (a conductor taken down for works); tick it again.
   - **Mirror wind**: the same case with the wind from the right.
5. **Load cases › Load table…**: the rows each case makes, one per
   point and axis, marked *line*.

## What you should see

In OHL-Design, the line sends three **line results**: *everyday*, and
*high wind — wind from the left* and *high wind — wind from the right*.
In OHL-Structure, two cases whose sentences read like *high wind — wind
from the left: vertical × 1.35, transverse × 1.5, longitudinal × 1.0 at
every point*, and their rows in the load table.

**What just happened:** the factors live in one place, the structure's
load cases, and each case says in words what it does. The starter's
structures are of level 1 (points and strings, no members): the
**loads on the structure itself** — the wind on its body, ice on its
members — need a structure with members (levels 2 and 3), whose own
weight and wind the finite-element check then takes. See [Loads for
structures](../user-guide/desktop-app.md#loads-for-structures) and
[Structure load cases](../user-guide/structure-load-cases.md).

Next: [10. Saving and what next](10-saving.md).
