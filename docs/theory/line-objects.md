# Objects on the wires

*Built, in active development (not approved yet).* Implemented in
`crates/ohl-line` (`objects.rs`; taken by `solve.rs`, `jumper.rs`). A
**point** is a labelled place on a section's span or on a jumper; it holds
**one object**, or none (a bare point: a spread's end, a mark). The object
types come from the library of point and spread loads (`.ohlload`,
[data model](../development/data-model.md#point-and-spread-load-library-ohlload)),
and every object keeps its own values. How a body's weight, ice and wind
are worked out is in
[Point loads](wirefe.md#point-loads-marker-balls-and-other-bodies). This
page says what each object is for, how the **tap clamp** and the
**spacer clamp** differ and where each may be mounted, and how objects
count on a **bundle** (owner, 2026-11-14: "it's important to have the
theory explain the use of a tap clamp vs a spacer clamp and where it can
be mounted").

## The objects

| Object | What it is | What the wire carries |
|---|---|---|
| **Body** | Something hanging on the wire: a marker ball, an obstruction light, a bird diverter, a weight. | Its weight, the ice on it, the wind on its iced area. |
| **Tap clamp** | Where a jumper starts from the wire: a T-off, a loop, the jumpers at a riser's top and bottom. | Its own weight, ice and wind, **and the jumper's force** at the clamp. |
| **Spacer clamp** | Where an interphase spacer ends: the insulating bar that keeps two conductors of one span apart. | Its own weight, ice and wind, **and the spacer's force**. |
| **Clamp** (either) | A clamp a jumper or a spacer may use: own values, files from before 2026-11-14. | The same, for whichever uses it. |
| **Spread** | A load along a stretch between two points of one span: a heated stretch, a lashed cable. | N per metre between its points. |

Every clamp is a **hinge**: it passes forces, no moment and no torsion
(owner decision, 2026-10-16). **Simplification:** a real clamp passes
some moment and twists the conductor a little. The moments are small
next to the forces (a clamp's lever arm is a few centimetres) and are
left out, either way ([Jumpers](jumpers.md)). Exact: the clamp's
moments and the conductor's torsional stiffness.

## Tap clamp or spacer clamp

| | Tap clamp | Spacer clamp |
|---|---|---|
| **Holds** | The end of a **jumper** that starts from the wire. | One end of an **interphase spacer**. |
| **Mounted on** | A **span point** of a section's span, or a **point of another jumper** (a jumper on a jumper). | A **span point** of a section's span **only**: a spacer joins span points of two conductors of the same span. |
| **Not on** | A jumper's support clamp: that clamp is held up by strings (it holds the jumper up, it does not start one). | A **jumper point**: refused, a spacer has nothing there to end on. |
| **Force it passes** | The jumper's force at its end: its own pull solved in passes (*the full loop*, the default), or its weight, ice and wind up to its first support (*one calculation*), Line › Criteria › Calculation options ([Jumpers on taps](jumpers.md#jumpers-on-taps)). | The spacer's axial force: tension, or compression up to its compression limit (flagged beyond it); half the spacer's weight and wind at each end. |
| **Solved** | The section by finite elements (a span with a tap is not solved by the ruling span); the tap jumper given by its length, or rigid. The jumpers hanging on a jumper are solved after it. | The two sections by finite elements, solved together with their spacers. |
| **Made with** | Add on span (object: a tap clamp), then Add jumper from it. | Add string between two span points; a bare end point gets the project library's spacer clamp. |

What is refused, saying why:

- a jumper starting on a **spacer clamp** ("a jumper starts from a tap
  clamp");
- a spacer ending on a **tap clamp** ("a spacer ends on a spacer
  clamp");
- a **spacer clamp on a jumper point**.

A clamp of either use (`clamp`) takes both, as before. On a span point
that holds a tap and a spacer end together, that is the only kind that
can be used.

**Not checked:** the clamp's own strength (a tap clamp's slip load, its
rated pull) and the conductor diameters it fits. They are the maker's
data and are not compared with the forces; the forces at the clamp are
in the results (the jumper's end forces, the spacer's force).

## On a bundle

A bundle is solved as **one subconductor** carrying its share: the
subconductors are taken as hanging alike
([Line sections](wirefe.md#line-sections-strain-assemblies-bundles-jumpers)).
An object's forces are the whole bundle's, and the model takes them ÷
the number of subconductors \\( n \\).

**One on each subconductor** (the tick *per subconductor*, owner
2026-11-14). A clamp is fitted on every subconductor of a bundle: a
twin bundle's tap has two tap clamps, a quad bundle's spacer end four
clamps. Ticked, the object's values are **one object's**, and the
bundle carries

\\[ \mathbf F_{\text{bundle}} = n \left( W + W_{\text{ice}},\; F_{\text{wind}} \right) \\]

— each subconductor its own clamp's weight, ice and wind; the model of
one subconductor carries one of them. Not ticked, there is one object for
the whole bundle and its values are the whole bundle's (as before
2026-11-14). On a single wire both are the same.

- The tap clamp and spacer clamp templates are ticked; the bodies (a
  marker ball sits on one subconductor) are not. Each type's tick is
  changed in the library (Line › Sections › Point and spread load types,
  *load on*), each object's in its window or the Span points and loads
  table.
- **Conservative:** each subconductor's clamp takes the **full wind
  pressure**, without the shielding of the clamp in front of it. The
  clamps behind are partly in the wake of those in front; the real wind
  on the bundle's clamps is a little less. Exact: shielding factors for
  the bundle's geometry.
- Where it counts: the sections (finite elements), the jumpers (each
  model: catenary, rod, riser, rigid) and the flag of a section solved by
  the ruling span, which names the point loads it leaves out with the
  whole bundle's weight.
- **Drawn**: on a bundle a clamp is drawn on **every subconductor**, as
  it is fitted (a clamp's cylinder across the line joins them instead);
  a body is drawn on each when ticked. The drawing does not change the
  forces.

## In the files

The line file (version 16): `"clamp_for": "tap" | "spacer"` on a clamp
and `"per_subconductor": true` on a body or a clamp, each written only
when used ([Line files](../user-guide/line-file.md)). The library
(`.ohlload` version 2): `kind = "tap_clamp"`, `"spacer_clamp"`, and
`per_subconductor` on any type. Files without them are written in the
older versions, for older programs.

*Validation:* the library's round trip and counts (`objects` tests), the
refusals (`ohl-line` validation tests), the drawing on each subconductor
(`ohl-app` tests).
