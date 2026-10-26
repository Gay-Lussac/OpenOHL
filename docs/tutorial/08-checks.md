# 8. Checks

**Goal:** check the line — clearances to the ground and between the
wires, the wire tensions and the strings — and see what a failing check
looks like.

The checks run **on demand**: they never change the line. Each window
has **Run** at its bottom left. All distances and factors below are
the starter's invented values, not those of any regulation.

## Steps

1. **Wire clearances.** **Checks › Wire clearances › Check wire
   clearances** (also on the quick bar). The **Clearances** page shows
   the result for the weather case shown first (*everyday*), with the
   starter's rule file *starter-clearance-rules* chosen: per rule its
   lowest clearance — *Ground* (7 m) and *Phase to phase* (2 m) — and no
   violation.
2. **Vertical clearances.** **Checks › Ground clearances › Vertical
   clearances…**. The window starts with one rule, *Ground*, its
   distance still to set: in the **Rules** table type **7** m in its
   **distance** (one distance, every voltage, every weather case). Click
   **Run**. The result: no span below the rule; the lowest clearance,
   between 9 and 10 m, at the maximum temperature.
3. **See a failure.** Hold **Edit**, click a span of S1, set its
   **horizontal tension** to **10 kN** and **Apply**. The vertical
   clearances window says its result is out of date: **Run again**. Now
   *S1 span 3* is listed below the rule (about 6.3 m at the maximum
   temperature), red in the window and marked red in the views. Click
   the span in the list to go to it. Then **Undo** (⌘Z / Ctrl+Z) and
   **Run** again: clear. Close the window.
4. **Wire tensions.** **Checks › Wire tensions…**. Click **Add rule** and
   type its **factor of RTS**: **0.5** (each wire may carry half its rated
   tensile strength — invented). Click **Run**: every span in every
   weather case, with its tension, the allowed tension and the usage;
   all pass.
5. **String checks.** **Checks › String checks…**. The window starts
   with a rule checking each string against its own limits and the
   suspension strings' uplift. Click **Run**: the uplift passes; the notes
   say the starter's strings have no maximum load or swing limits of
   their own, so those are not checked. Limits come with insulator
   assemblies, or are typed in the Strings table.

![The Vertical clearances window with S1 at 10 kN: one span below the
7 m rule at the maximum temperature, marked in the profile](../images/tutorial/tutorial-vertical-clearances.png)

**What just happened:** the wire clearances check one weather case
against a **rule file** (`.ohlr`) — the starter's, kept in the project.
The vertical clearances window keeps its own rules (in the project, as
a rule file of its own) and checks every weather case, with the wind
from both sides. The wire tensions and string checks keep their rules
the same way. A rule's distance or factor is never filled in for you:
take it from the regulation you work to. See
[Clearances](../user-guide/desktop-app.md#clearances), [Vertical and
radial clearances](../theory/clearances.md), [Wire
tensions](../theory/wire-tensions.md) and [String
checks](../user-guide/desktop-app.md#string-checks).

Next: [9. Loads for the structures](09-ohl-structure.md).
