# 5. Stringing the wires

**Goal:** the three phases strung, each in two **sections** — T1 to T4
and T4 to T6 — and solved in every weather case.

## Steps

1. **The wire type.** **Sections › Wires library** shows the project's
   wire library: the starter's ACSR conductor `acsr-drake-26-7` with its
   diameter, mass, strength and moduli (typical published values). Close
   it; nothing to change.
2. Hold **String wire**.
3. Right-click the strings of the first section, in line order (the set
   labels of chapter 4 help, in the 3D view):
   T1's string of **set 41**, then T2's **set 1**, T3's **set 1**, and
   T4's **set 21**.
   The status bar counts the points; a line is drawn through them.
   **Backspace** takes the last one back, **Escape** gives up.
4. Press **Enter**. The window **String wire: new section** opens: id
   *S1*, wire type `acsr-drake-26-7`, its points (`T1 set 41 ➡ T2 set 1 ➡
   T3 set 1 ➡ T4 set 21`). Type **circuit** `C1` and **phase** `L1`. Keep the
   status **in the blocks** and **sagged to a tension**: 20 kN at
   20 °C (an invented value). Click **Apply**.

   ![The plan with the six structures, and the window String wire: new
   section for S1](../images/tutorial/tutorial-string-wire.png)
5. The second section of the same phase: right-click T4's **set 41**,
   T5's **set 1**, T6's **set 21**, **Enter**. The window starts from the
   first section's values (circuit C1, phase L1): **Apply**.
6. The other phases the same way, changing the **phase** in the window
   each time:

   | Section | Strings, in order | Phase |
   |---|---|---|
   | S3 | T1 set 42, T2 set 2, T3 set 2, T4 set 22 | L2 |
   | S4 | T4 set 42, T5 set 2, T6 set 22 | L2 |
   | S5 | T1 set 43, T2 set 3, T3 set 3, T4 set 23 | L3 |
   | S6 | T4 set 43, T5 set 3, T6 set 23 | L3 |

7. **View › Display weather cases…**: tick **max temperature** beside
   **everyday**. The wires are drawn in both cases.

## What you should see

Six sections, each solved at once: the wires hang between the
structures, the suspension strings plumb in still air, the tension
strings lined up with their wires. With *max temperature* shown the
wires hang lower; with *high wind* they swing to one side (the wind is
from the right, or both sides, as chosen in the same window).

**What just happened:** a **section** is a wire strung between two
tension structures over one or more spans, with its wire type, its
circuit and phase, and its **stringing**: here a horizontal tension at a
temperature, the same in every span while it is *in the blocks*. From
that condition every **weather case** — temperature, wind, ice — is
solved: the change of state gives each case's tension and sag. You
may string the earth wire the same way (T1 set 59, T2 and T3 set 19,
T4 set 39, then T4 set 59, T5 set 19, T6 set 39; circuit and phase cleared:
it is earthed). See
[Sag-tension](../theory/sagtension.md), [Stringing and
status](../user-guide/line-file.md#stringing-and-status), [Weather cases
shown](../user-guide/desktop-app.md#weather-cases-shown) and the
[Glossary](../user-guide/glossary.md#sections-spans-and-jumpers).

Next: [6. Jumpers](06-jumpers.md).
