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
