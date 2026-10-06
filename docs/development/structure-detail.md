# Structure detail levels in checks

A structure type says how much of the structure its file describes
(`detail`, see [Structure libraries](../user-guide/structure-library.md#detail-levels)):

1. **stick figure** — a mast and crossarms to the attachment points;
2. **outline** — the outside of the full pylon (`[[body]]` sticks, diagonals allowed);
3. **full design** — a finite-element model with every design element needed to build it.

**Rule for every new check or test whose result depends on the structure's
shape** (distances to the structure body, insulator swing against the
structure, structure analysis, clearances inside the window of a tower,
anything reading `body`, `members` or the `.olsfe` model):

- before implementing it, decide with the project owner what it does at each
  level — e.g. not available, an approximation (from the stick figure), or
  the exact check — and what it reports when a structure's level is too low;
- write that behaviour, per level, in the check's documentation and test each
  level;
- a check never silently treats a stick figure as an outline or a full design.

In code the level is `ols_structures::Detail` (`StructureType::detail`).
