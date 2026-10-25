# 9. OHL-Structure

*This chapter is to be written when the new structure load cases land:
they are being redesigned now. Until then it only shows what the line
sends to its structures.*

**Goal:** see which loads the line hands to its structures.

## Steps

1. **Criteria › Loads for structures** (a menu of ticks; it stays open
   while you tick). It has two lists:
   - **Load cases**: the starter's three load cases, from the line's
     load case file — all sent (the default, also those added later;
     **all** / **none**). Hover a case for its weather case, the wires'
     condition and its factors.
   - **Weather cases on their own (all factors 1)**: none ticked by
     default. A weather case ticked is sent as a case of its own, the
     wires solved in it as strung, every factor 1 (no safety factor).
2. Close the menu without changing anything.

## What you should see

The three load cases ticked, no weather case ticked. Each tick you make
there is one edit of the project line (Undo takes it back).

**What just happened:** nothing changed. When a placed structure is
opened in the structure application — **Open in OHL-Structure…** in its
Properties — it gets these cases with the loads the line puts on it at
its place. How OHL-Structure then combines them is what the chapter to
come will show. See [Loads for
structures](../user-guide/desktop-app.md#loads-for-structures), [Load
cases](../user-guide/loads.md) and
[OHL-Structure](../user-guide/structure-app.md).

Next: [10. Saving and what next](10-saving.md).
