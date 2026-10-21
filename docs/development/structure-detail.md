# Structure detail levels in checks

A structure type says how much of the structure its file describes
(`detail`, see [Structure libraries](../user-guide/structure-library.md#detail-levels)):

1. **stick figure** — a shaft and crossarms to the attachment points, or
   `[[body]]` polylines all in one plane (a face);
2. **sticks of the structure** — joints and members without cross-sections, or
   `[[body]]` polylines that make a 3D object (their vertices, all together,
   not in one plane within 1 mm: `StructureType::body_is_3d`);
3. **full design** — every member with its cross-section and a supported joint
   (or a separate `.ohlfe` model); extensions need the same. Only a
   joint's `support` counts (`StructureType::missing_for_design`): a
   structure standing only on springs (`spring`) stays at level 2 today.
   A pole is a full design when the pole check can read it (a `.ohlp`
   named, or a complete `[pole]`); the higher of the two levels counts.

The level follows from the file's content (`StructureType::detail()`), so it
rises as a structure is filled in.

**Rule for every new check or test whose result depends on the structure's
shape** (distances to the structure body, string swing against the
structure, structure analysis, clearances inside the window of a lattice tower,
anything reading `body`, the joints and members, or the `.ohlfe` model):

- before implementing it, decide with the project owner what it does at each
  level — e.g. not available, an approximation (from the stick figure), or
  the exact check — and what it reports when a structure's level is too low;
- write that behaviour, per level, in the check's documentation and test each
  level;
- a check never silently treats a stick figure as an outline or a full design.

In code the level is `ohl_structures::Detail` (`StructureType::detail()`).

Agreed so far: clearances to the structure body — none at level 1 (LiDAR
points are a separate topic), computed at levels 2 and 3, the tests to be
defined with the project owner.
