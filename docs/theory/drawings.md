# Plan and profile drawings

*Built, in active development (not approved yet).* The drawings compute
nothing an analysis does not: they place what is solved and checked on
paper. This page says how the sheets choose their height ranges and how
the lines below the lowest conductor are made.

## The profile's height range on a sheet

The profile panel takes the whole height left between the plan and the
band rows. Its **vertical scale is never changed**. On each sheet:

1. **Everything fits** — the ground (on the axis and beside it), the
   required-clearance line above the ground, the wires, the structures and
   room for their names: one height range, the ground near the bottom of
   the panel.
2. **Only the essentials fit** — the ground, the clearance line and each
   wire's **lowest point** in the sheet (what the ground clearance is
   about): one height range from the lowest of them; the structures' tops
   (and the wires near their attachments) are cut at the panel's top, a
   break mark on each mast cut, a warning naming the structures.
3. **Not even the essentials fit** — the **datum is stepped**: the sheet
   is split at structures on the line into parts, each as long as its
   essentials fit, each with its own height range, a double line across
   the panel at the step and its own height labels; the warning says where
   and by how much (`datum stepped at T4 (+5 m)`). A single span whose
   essentials alone do not fit is drawn from its lowest point, its top
   cut, warned.

The profile is never empty and the ground is always on the sheet. The
continuous drawing (no sheets) takes the height it needs.

## Lines below the lowest conductor

For each circuit (its sections' circuit name) the line is

z(s) = min over its spans in the chosen weather case(s) of z_wire(s) − d

at every station s where one of its wires is drawn: the **lower
envelope** of its conductors (each span interpolated linearly between its
solved points), lowered by d, the vertical ground clearance its rule
requires. Jumpers are left out (they hang at the structures).

- d: the clearance rules for the ground (`target = "ground"`, a `vertical`
  requirement) that check the circuit's voltage (`voltages`, none: all),
  at the circuit's nominal voltage (the highest of its sections' systems;
  earthed: 0 kV) by the requirement's form — a distance, a table by
  voltage (`by_voltage`), or `base + per_kv · (U − above_kv)` — as the
  clearance checks read it ([Terrain and ground
  clearance](../user-guide/terrain.md)). A voltage above a table's last
  row gives no line and a note.
- The weather cases: those the rule names (`cases`, wildcards); none named:
  the drawn case whose wires of the circuit reach lowest (the largest
  sag), said in the legend.

**Conservative:** where several ground rules check a circuit, the line
uses the **largest** of their distances (the strictest line on the
sheet), with that rule's weather cases; the checks themselves report each
rule on its own. The exact treatment would draw one line per rule.

The line is a drawing aid: whether the ground stays below it is read on
the sheet, not computed; the ground clearance check gives the numbers.

Its label (`label_at = "auto"`) is placed by trying, every 2 mm along
the sheet, the place just above and just below the line: the one over
the fewest structures (each structure's width from its base to the room
for its name), then over the fewest points of the wires, the ground
lines and the other such lines (every 1 mm along them), then nearest the
sheet's middle; above before below. Only the label moves.

## The corridor's sides

The ground along the corridor's edges is the ground profile at the
offsets ± w (w the corridor's half width; right positive) along the
centreline, made as the side profiles are. The **near side** — nearest
the reader of the profile — is by default the right one looking along
increasing stations: with the stations increasing to the right on paper,
a reader of the profile looks across the line from its right side (and
that side is the plan's lower edge). The template may say the left.

## LiDAR points

A LiDAR point (x, y, z) of a class drawn is projected on the route's
centreline (station s, offset o) and kept when |o| ≤ w (the corridor's
half width, else the line width) and it lies beside the centreline, not
beyond its ends; the profile shows it at (s, z), the plan at (s, o).
**Drawing only:** one dot per class is drawn in each square paper cell of
the template's `cell` (0.2 mm); the points themselves are not changed
and no number is computed from them here.
