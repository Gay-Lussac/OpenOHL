# 4. Strings on the structures

**Goal:** see the strings the structure types brought, and which **set**
each wire point has — the numbers you string the wires to in chapter 5.

## Steps

1. **Structures › Display options**: tick **Wire points (label, set)**
   for the Plan and the 3D view (the menu stays open while you tick;
   click elsewhere to close it).
2. Press **G** (Go to structure), choose *T2*, **Enter**: the 3D view
   goes to it. Then do the same for *T4*.
3. Hold **Edit** and click one of T2's strings (from a crossarm
   down to its wire point). The window **Strings of set
   1 of T2** (or 2, 3, 19) shows the string: the point it hangs from, its
   length and weight (the type's own values; a string can take them from
   an insulator assembly of the library instead), and its wire point with
   the set and the clamp (suspension). Click **Cancel**.
4. **Structures › Strings table** lists every string of the line, one
   row each. Close the table.

## What you should see

The starter types use these sets (looking ahead along the line, the
crossarms at 22 m and 28 m):

| Wire point | Suspension structure (T2, T3, T5) | Tension structure (T1, T4, T6): span behind / span ahead |
|---|---|---|
| left, lower crossarm | set 1 | set 21 / set 41 |
| right, upper crossarm | set 2 | set 22 / set 42 |
| right, lower crossarm | set 3 | set 23 / set 43 |
| earth wire, at the peak | set 19 | set 39 / set 59 |

A suspension structure has one string per wire point, hanging down. A
tension structure has two **tension strings** at each attachment point,
one towards the span behind, one towards the span ahead, each with its
own wire point.

**What just happened:** nothing changed — the strings came with the
types in chapter 3. A string runs from its top (an attachment point) to
its **wire point**; the **set** is the wire point's number on its
structure, the place a wire is connected to. A structure without a type
gets its strings with the **Add string** tool (a click on one of
its points). Changing a typed structure's string in its window overrides
the type's string for that structure only. See the
[Glossary](../user-guide/glossary.md#strings) (strings combine: two
strings to one wire point, a string below another…) and [Editing ›
Edit](../user-guide/desktop-app.md#editing).

Next: [5. Stringing the wires](05-stringing.md).
