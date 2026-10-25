# 3. The first structures

**Goal:** six structures on the alignment in one command — tension
structures at both ends and at the angle, suspension structures between.

## Steps

1. Hold **Add structure** (its slot in the hotbar, or **Next: Hold Add
   structure** on the Next steps page).
2. In the plan, right-click alignment point **P1** (E 0, N 0). The window
   **Add structure** opens: id *T1*, standing **on an alignment point**
   (P1 chosen in the list, the base on the ground), its
   orientation **along the line** ticked.
3. **type**: click the type button. The **structure picker** lists the
   two starter types with a drawing of each. Click `starter-strain` and
   **Choose**.
4. Open **Also on other points of this alignment** (folded, under the
   point). Click **All**: points P2 to P6 are ticked.
5. In the **type** column choose `starter-suspension` for P2, P3 and
   P5. Leave P4 and P6 at *(the type above)*: they take `starter-strain`
   too.
6. Click **Apply** (or press **Enter**).

![The Add structure window on P1: the type starter-strain, the other
points of the alignment ticked, starter-suspension at P2, P3 and
P5](../images/tutorial/tutorial-add-structure.png)

## What you should see

Six structures, *T1* to *T6*, one on each alignment point: T1, T4 and
T6 are tension structures, T2, T3 and T5 suspension structures. The plan
and the 3D view fit to them. Each is turned along the line: T4, at the
angle, has its crossarms on the bisector of the line angle. Next steps has *Structures
on the line* and *Strings on the structures* done.

**What just happened:** each structure is a **structure type** placed
at one spot: the type (`.ohls`, in the project's structure library)
gives its shape, its attachment points and its strings; the structure
gives its place, height and turn. Each is tied to its alignment point:
move the structure and the alignment follows. The window put all six
in one command, so one **Undo** (⌘Z / Ctrl+Z) takes them all back. A
tension structure ends sections (chapter 5); a suspension structure
carries the wire through. See [Editing › Add a
structure](../user-guide/desktop-app.md#editing), [Structure
picker](../user-guide/desktop-app.md#structure-picker) and [Structure
templates](../user-guide/structure-library.md#structure-templates-ohlpt)
(the starter types were built from the template `delta-1c`; Tools ›
**New structure from a template…** builds your own).

Next: [4. Strings on the structures](04-strings.md).
