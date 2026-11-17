# 4. The set numbers

<img src="../images/tutorial/tutorial-tension-sets.png" alt="You will end with this: T4 up close, each wire point labelled with its set" width="400">

**Goal:** know which string is which. In chapter 5 you string the wires
by clicking strings, and each string's wire point has a **set** number.
This drawing is all you need:

![The set numbers. Suspension structure, looking ahead along the line:
set 1 for L1 (left, lower crossarm), set 2 for L2 (right, upper), set 3
for L3 (right, lower), set 19 for the earth wire. Tension structure: two
strings at each place, 21, 22, 23 and 39 towards the span behind, 41,
42, 43 and 59 towards the span ahead. Below, the clicks of section S1:
T1 set 41, T2 set 1, T3 set 1, T4 set 21, then
Enter](../images/tutorial/tutorial-set-numbers.svg)

## Steps

1. Choose **Line › Structures › Display options** and tick **Wire
   points (label, set)** in the **3D** column.
2. Click anywhere outside the menu to close it.

   ![Line › Structures › Display options: Wire points (label, set)
   ticked for 3D](../images/tutorial/tutorial-structure-display-options.png)

3. Press **G** (Go to structure), choose **T2** and press **Enter**.
4. Scroll to zoom in until T2 fills the view. Drag to turn the view.

   ![T2 up close: W-L1 (set 1), W-L2 (set 2), W-L3 (set 3) and the
   earth wire's W-E1 (set 19)](../images/tutorial/tutorial-suspension-sets.png)

5. Hold **Edit** (press **5**) and click one of T2's strings. The window
   **Strings of set 1 of T2** (or 2, 3, 19) shows the string and its
   set.

   ![The window Strings of set 1 of T2](../images/tutorial/tutorial-string-window.png)

6. Click **Cancel**.
7. Look at **T4** the same way: **G**, choose **T4**, **Enter**, zoom
   in.

   ![T4 up close: two strings at each place, sets 21, 22, 23 and 39
   behind, 41, 42 and 43 ahead](../images/tutorial/tutorial-tension-sets.png)

## What you should see

Each wire point is labelled with its name and set, e.g. *W-L1 (set 1)*
on T2 and *W-L1-2 (set 41)* on T4. Labels that would overlap are left
out: turn the view and they come back.

<details>
<summary>Why it works this way</summary>

A string runs from an attachment point on the structure to its **wire
point**, where the wire is clamped. The **set** is that wire point's
number on its structure; the structure type gives it. The starter's
types number the phases 1, 2, 3 (L1, L2, L3) and the earth wire 19; a
tension structure has two strings at each place, one for each span, so
it adds 20 for the span behind and 40 for the span ahead. Other
structure types may number their sets differently: their drawings (in
the structure picker, and in the Add structure window) show the sets.
Nothing changed in this chapter — the strings came with the types.

</details>

Read more: [Glossary: strings](../user-guide/glossary.md#strings),
[Editing › Edit](../user-guide/desktop-app.md#editing).

Next: [5. Stringing the wires](05-stringing.md).
