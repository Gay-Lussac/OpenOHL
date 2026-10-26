# 6. Jumpers

**Goal:** the two sections of each phase joined at T4 by a **jumper**.

## Steps

1. Hold **Add jumper**. It is not in the hotbar at first: click **Hold
   Add jumper** on the Next steps page, or choose it in **Tools**, or
   press **E** and drag it from the inventory onto a slot.
2. Click the structure **T4** (its body, not a string).
3. The status bar says *added 3 jumpers at T4*. Click T1 or T6:
   *none missing* — a section ends there with nothing to join.
4. Left-click one of the new jumpers (the loop of wire under T4's
   crossarm) and look at the **Properties** page.

## What you should see

At T4 each phase now runs on: the wire of the span behind, the tension
string, a jumper hanging in a loop below the crossarm, the other
tension string, the wire of the span ahead. The jumper's properties give
its wire type (its section's), its length and its results in every
weather case. **Line › Sections › Jumpers table** lists the three.

**What just happened:** at a structure the tool joins the section ends
of the same **circuit** and **phase** (that is why each section got C1
and L1, L2 or L3 in chapter 5) — two ends of the same phase, one jumper.
Two clicks on strings instead join exactly those two. A jumper is
solved as a wire of its own, from clamp to clamp, with its bending
stiffness estimated from its wire type; the sections carry an estimate
of its load at their clamps (Line › Criteria › Calculation options… chooses).
Joined by the jumpers, S1 and S2 are now one **conductor path**. See
[Jumpers](../theory/jumpers.md), [Jumpers in the line
file](../user-guide/line-file.md#jumpers) and the
[Glossary](../user-guide/glossary.md#electrical).

Next: [7. Results](07-results.md).
