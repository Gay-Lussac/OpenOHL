# Points on the wires and the objects on them (proposal)

Status: **draft, decisions taken (below); the tap's details being
discussed** — nothing built. Every point below is an engine change
(data, solver, files).

Decided with the user (2026-10-16): **one object per point**; objects
of a given geometry, with **templates** for the most used ones and more
geometry in advanced settings (in the adding tool and the table); the
**spacers' ends become clamp objects** (one clamp object for jumpers and
spacers); a tap clamp is a **hinge (ball joint) without strength** — no
moment, no torsion passed between the jumper and the wire; jumper points
placed by fraction, length from `from` **and** distance along the line;
**one library file** for point and spread types.

Asked by the user: point loads on jumpers too; a jumper starting from a
point — on a span or on another jumper; not every point load a sphere,
its type defined in a library of point and spread loads; and two
separate things: **the point of application** (on the wire, with a
label) and **the object put on it** (its type and properties), because a
clamp starting a jumper is not defined like a marker ball.

## Today

- **Span points** (`SpanPoint`: section, span, place, label) exist only
  for spacers.
- **Point loads** (`SpanLoad`) carry their own place and are always a
  body taken as a sphere (diameter, or given wind and ice areas, drag).
- **Spread loads** (`SpreadLoad`) carry a start and an end place.
- **Jumpers** end only at section ends (`from`/`to`: structure + set);
  along them only support clamps on suspension strings
  (`JumperClamp`); no loads.

## 1. Points of application

One idea, **a labelled point on a wire**, on two hosts:

| | Host | Place |
| --- | --- | --- |
| span point | a section's span (as today) | fraction, distance from the back structure, station |
| jumper point | a jumper | fraction of its length from `from`, or a length along it from `from` |

- Label unique on its host (section or jumper); referenced as
  `{section, point}` or `{jumper, point}` (`PointRef`).
- A point is only a place: no mass, no load. It is drawn (a dot), listed,
  selectable in every weather case drawn, and placed with the span tool
  (a right-click on a span or a jumper, where clicked).
- Solved: a node of the wire there (as span points now; on a jumper a
  node of its rod model), its position per weather case reported.
- Deleting a point lists and deletes what is on it (objects, spacers,
  jumpers starting there).

## 2. Objects on the points

An **object** is put on a point — **one object per point**. It refers to a **type** of the library,
or gives its own values; per object only what the type allows to
override.

### The library of point and spread loads (`.olsload`, new)

A library like the wire and insulator libraries (shipped examples, the
project's copy, a table, import/export). Each entry has an id, a name, a
description and a **kind**:

| Kind | What it is | Properties |
| --- | --- | --- |
| `body` | marker ball, obstruction light, bird diverter, weight, sensor | mass (N weight); its **shape** for wind and ice: `sphere` (diameter), `cylinder` (diameter, length, along or across the wire), `box` (three sizes), or `areas` (wind area, ice surface given); drag coefficient (default by shape) |
| `clamp` | where a jumper or a spacer is held on a span or a jumper (a tap clamp, a spacer's end clamp) | mass; its size for wind and ice (a template); length along the host wire; which cables it takes (diameters, optional); a **hinge**: it passes forces, no moment |
| `spread` | a heated stretch, a cable lashed on, covers, a bird-diverter run as N/m | load N/m; for the wind an added diameter (or none); for the ice whether it takes ice and on what diameter |

A **spread object** lies between **two points** of the same span (or
the same jumper): start label, end label.

### Loads in a weather case

Templates (shipped in the library, the adding tool's first choices):
marker ball (sphere), obstruction light (cylinder), bird diverter (given
areas), weight (no wind), tap clamp, spacer clamp, heated stretch,
lashed cable (spread). Advanced settings (adding tool, table): the
shape and its sizes, given areas, drag, ice surface.

- `body`: weight; ice on its surface by the case's ice thickness (shape
  formula per shape, as the sphere's today); wind on its iced area, its
  drag, from the case's side, across the line (as on the wires).
- `clamp`: weight (and ice/wind on a small body, optional); the pull of
  the jumper starting there (§3).
- `spread`: N/m between its points, its wind and ice as given.

### Old files

Read without change for the user: a `SpanLoad` becomes a point (labelled
from its label, else `L1`, `L2`…) and a body object with its own values
(a sphere); a `SpreadLoad` two points and a spread object. Written in the
new form (line file version +1). OLS XML, the XSD, the data model page
follow.

## 3. Jumpers from a point (taps)

A jumper end (`from` or `to`) becomes either, as now, **a section end**
(structure + set), or **a point with a clamp object** (`PointRef`):

- on a **span** (a T-off / tap from a conductor in the span),
- on **another jumper** (a jumper branching from a jumper).

### Solving

- The end is where the point is solved in the weather case (it moves with
  the wire). The clamp is a **hinge**: the jumper leaves it in the
  direction its own equilibrium gives (no exit angle, no moment); the
  rod model already takes a free end (`rod::Rod` `start`/`end` none),
  the jumper's own bending stiffness acting along it as now. The host
  wire takes the force only (the wires carry no bending nor torsion).
- **On the host:** the clamp's weight, and the jumper's load:
  - *estimate* (the default today): the tap end carries the weight, ice
    and wind of its part of the jumper, as section clamps do;
  - *pull* (the option of Calculation options): the jumper's own force
    at its end, and the host solved again, in passes, until the clamp no
    longer moves (as the jumpers' pull on the clamps today, and the
    spacers).
- A section with a tap point is solved by **finite elements only** (the
  ruling span refused, as with spacers).
- Order: hosts before the jumpers they carry; a jumper on a jumper after
  it; **cycles refused** (a jumper on itself, two jumpers on each
  other).
- Jumper points and loads on a jumper (§1, §2): nodes of its rod model
  with their loads; its end forces include them, so the clamps (section
  or tap) carry them with either option.

### Everything else a tap touches

- **Electrical circuits:** a tap connects the jumper's circuit to the
  host's conductor (the circuit tracing follows taps; a tap on a jumper
  joins that jumper's path).
- **Checks:** a jumper and its host are connected at the tap: not paired
  in the distance between conductors (as two wires sharing a clamp), the
  stretch near the tap left out of the clearances between the two;
  clearances of the jumper itself as for any jumper.
- **Loads on structures:** the host's tensions change (through the solve);
  nothing else.
- **Import:** PLS-CADD files that hold taps or point loads on jumpers, if
  they do, mapped.

## 4. App

- **Span tool** (Add on span): on a span or on a jumper, where clicked —
  a point only, or a point with an object chosen from the library
  (regular spacing makes several).
- **Add jumper**: right-click a section end (as now) or a point holding a
  clamp object (on a span or a jumper), then the other end; a point
  without a clamp offers to put one there.
- **Tables:** *Points* (host, label, place, objects on it), *Objects*
  (point, type, overrides, the load in the case shown), the *Point and
  spread load library*.
- **Drawing** by kind and shape: spheres, cylinders, boxes to size;
  clamps as a short bar along the wire; spread objects as a band along
  their stretch — in every weather case drawn, selectable there.
- **Properties, Edit, Delete** (listing what goes with a point).

## 5. Order of work (when decided)

1. Points on jumpers; objects separate from points; the library; old
   files read (span loads, spread loads → points + objects). Tests:
   round trips, the old synthetic example unchanged in results.
2. Loads of each kind in the solve (bodies by shape, spread), on spans
   and jumpers.
3. Jumper ends at points (taps): solve order, estimate and pull,
   validation (FE only, no cycles); circuits and checks.
4. App: tool, tables, library page, drawing.
5. Docs: data model, line file, the library, theory (taps, shapes).

## Decisions (questions answered)

1. One object per point.
2. Given geometry, templates for the most used, more in advanced
   settings (adding tool and table).
3. Spacer ends become clamp objects (one clamp object for jumpers and
   spacers).
4. A tap clamp is a hinge without strength (forces only) — details and
   costs being discussed.
5. Jumper points also by distance along the line.
6. One library file for point and spread types (`.olsload`).
