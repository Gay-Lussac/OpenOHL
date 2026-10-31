# Check pages: design guidelines

How every check and calculation window of OHL-Design is laid out, so that
a user can **choose what to compute quickly** and **understand what will
happen before pressing Run** (owner, 2026-10-31, after the rework of the
Magnetic and Electric field windows: "it has to be easy and
understandable"). They extend the [desktop app conventions](ui.md) and the
one design of the check windows (2026-10-27, [UI review](ui-review-2026-10-27.md));
the shared code is `crate::check_ui` and `crate::check_help`. A window that
departs from them says why next to the code. Built, in active development,
not approved yet.

## 1. The shape of a check window

- **Two tabs: Setup and Results.** The setup scrolls; the results fill
  their tab. The **Run** footer stays at the bottom, always in reach.
- **The setup in numbered sections, always in this order**:
  1. **What it checks**: one short line, the longer text behind a (?), and
     the **drawings** of how it works. This part stays as it is in every
     window: it is what makes the check understood.
  2. **Scope**: what it is computed for (the weather case or cases, the
     circuits, and the objects: spans, sections, structures).
  3. **What to compute and where**: the outputs (profiles, cuts, volumes,
     points, rows) and their common sizes.
  4. **Model**: the modelling settings, **folded by default** (the
     defaults are sound; a user opens it to change them).
  5. Further sections (views, levels, export) after these.
- **One window, one job**: what belongs to another check links to it
  rather than being repeated.

## 2. Choosing quickly

- **Objects are chosen in a table, never in a small list.** One row per
  object (span, section, structure, circuit, case), its key data in
  columns (a span's length, a structure's type), the table as wide as the
  window and its height dragged by the user (`check_help::list`).
- **Ticks are fast to flip.** Unticked rows are greyed;
  **press on a tick and drag across rows**: every row passed takes the new
  state; shift-click ticks a range; **All**, **None** and **Invert** per
  group; a **filter** box keeps only the rows whose name contains the
  text.
- **Grouped as the line is**: an alignment's spans first, in station
  order, then the others. **Sensible defaults**: what a user usually wants
  is ticked (the alignment's spans), the rest is not; the choice is kept
  in the project.
- **What to compute where is in the same row** as the object it is
  computed on (a span's profile, its cut stations, its volume): no second
  list to keep in step with the first.
- **Places along an object are typed as the user says them**: *mid*,
  *lowest*, *230 m* (from the first structure), shown as small tags with
  × to remove and *+* to add.

## 3. Understanding what will happen

- **Every setting says, beside it, what it changes in the results and
  how long it takes** (measured on this computer where it can be): one
  short line (`check_help::explain`), the longer text behind a (?). The
  drawings follow the values typed.
- **Before Run, a summary**: what will be computed and about how long it
  takes ("4 spans · 3 profiles · 2 cuts · 1 volume — about 25 s"). When
  Run is greyed, the footer says why and where to fix it.
- **Assumptions are said**, conservative or not, in the setup where they
  are chosen and in the results' notes.
- **While it runs**: a progress bar with the stage and an honest time
  left; Stop always works.

## 4. Results that lead somewhere

- **One list of everything computed** (rows, profiles, cuts, volumes),
  the worst first or in the line's order, the value against the limit
  with its margin, the limit's colour when it fails.
- **Every row leads to what it is about**: a click selects and frames the
  object; **Open cut**, **Show in 3D**, **Plot** where it applies. What the
  screenshots of a report need is two clicks away from the results.
- **Notes** (what was assumed, what was left out) above the table; an
  **out-of-date banner** when the line changed since the run; **Export
  CSV** of what is shown.

## 5. Layout rules

- Label and value grids name their columns (`Grid::num_columns`), so a
  wrapped explanation takes the window's width; explanations are one
  wrapped label with their (?) beside it; nothing overlaps (checked on
  rendered screenshots of the Setup and Results).
- Windows about 860 px wide; lists dragged taller; long errors scroll in a
  fixed box; menus of options stay open while ticked.
- The same words as the [Glossary](../user-guide/glossary.md), the same
  control for the same thing in every window (a weather case picker, a
  span table, a level list).

## 6. Checking a window against these guidelines

The Setup and Results tabs are rendered by a test with `OHL_RENDER_PNG`
(made as `USER=tester`) and looked at: sections in order, nothing
overlapping, the drawings present, the summary before Run, each result row
leading somewhere.
