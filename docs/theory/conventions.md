# Conventions and units

## Units

All values are computed in **SI base units**, as `f64`:

| Quantity | Computed in | Typical display (metric / US) |
|---|---|---|
| Length | m | m / ft |
| Area | m² | mm² / in² (kcmil for conductors) |
| Force (tension, point load) | N | kN / lbf |
| Force per length (weight, wind, ice, distributed load) | N/m | N/m / lbf/ft |
| Stress, pressure, modulus | Pa | MPa / psi |
| Mass per length | kg/m | kg/m / lbm/ft |
| Angle | rad | ° |
| Speed | m/s | m/s / mph |
| Temperature | K | °C / °F |
| Second moment of area, torsion constant | m⁴ | cm⁴ / in⁴ |
| Section modulus | m³ | cm³ / in³ |

What the files store differs for two quantities, so that they read naturally:

- **Line model** (line file, OHL XML, project tables): plain SI numbers, except **angles in degrees** (fields ending in `_deg`) and **temperatures in °C**. Voltages are in kV (`_kv`) and frequencies in Hz (`_hz`).
- **Library files** (TOML: wire types, insulators, structure types, weather and load cases): every value carries its unit as text (`"12.5 m"`, `"20 °C"`, `"30 deg"`), any supported unit, converted to SI when read.

The conversion to radians and kelvin happens when the files are read; nothing is stored in radians or kelvin.

Conversion factors are the exact definitions (international foot = 0.3048 m, avoirdupois pound = 0.453 592 37 kg, standard gravity \\( g_n = 9.806\\,65 \\) m/s²). They are checked against NIST SP 811 in the validation suite.

The type system enforces dimensional consistency, e.g. a distributed load times a length is a force:

\\[ F = w \\, L \qquad [\mathrm{N}] = [\mathrm{N/m}] \cdot [\mathrm{m}] \\]

and a wind pressure on a conductor diameter is a load per unit length:

\\[ w_{wind} = q \\, d \qquad [\mathrm{N/m}] = [\mathrm{Pa}] \cdot [\mathrm{m}] \\]

## Conservative assumptions and simplifications

Where a calculation does not follow the physics exactly, the theory pages say so with one of two bold labels:

- **Conservative:** the calculation deliberately errs on the safe side. The label says what is assumed, why it is on the safe side, and what the exact treatment would be.
- **Simplification:** the calculation simplifies in a way that is not known to be on the safe side, or is on the unsafe side. The label says what is simplified, which way the error can go, and what the exact treatment would be.

Some pages use these other words for the same:

- **Conservative assumption**, **on the safe side**: as **Conservative**.
- **Not on the safe side**, **Limitation (not on the safe side)**: as **Simplification**, the error known to be on the unsafe side.
- **Assumption** (the electrical pages): a modelling choice, said with which way it errs where that is known; the electrical results are not design checks in themselves.

"Safe side" always refers to a named result (a clearance, a usage, a tension); an assumption can be conservative for one result and not for another, and the label then says which. Results must be verified by a qualified engineer either way.

## Numerical exit clauses

Every iterative method documented in this manual states its convergence criterion and its iteration and time limits. When a limit is reached the calculation fails with an explanation instead of returning an unconverged value (see [Performance and time budgets](../development/performance.md)).

## Coordinate conventions

![In plan: the project axes x east and y north; a route through T1, T2 and T3 with the station measured along it and a point at a positive offset to the right looking ahead; at T2 the structure's x axis to the right and y axis ahead, and orientation_deg from east to x, counter-clockwise; the wind from the left or the right. Beside it, the angles around a structure as shown in the application, clockwise from ahead.](../images/theory/conventions-axes.svg)

- **Project coordinates**: a projected coordinate system in metres (the line's `crs`, e.g. `EPSG:2056`); x east, y north, z up. Heights are kept as given — usually orthometric — and no geoid model is applied ([Coordinate systems](../user-guide/coordinates.md)).
- **Stations and offsets**: stations are measured along a route's alignment (or its structures); offsets are positive to the right looking ahead ([Line files](../user-guide/line-file.md)).
- **Structure axes**: origin at the structure's ground centre; x to the right of the line (transverse), y ahead (along the line), z up. `orientation_deg` is the direction of the local x axis, degrees counter-clockwise from east.
- **Angles around a structure** as shown in the app (string directions, Line › Structures › Angles around structures) turn **clockwise seen from above**, with *ahead* at the project's angle (the line's display setting `ahead_deg`, 0° by default; e.g. 270: ahead 270°, right 0°): right is ahead + 90°, back ahead + 180°, left ahead + 270°. This is a display convention only; stored positions do not depend on it.
- **Wind** comes from the left or the right of the line, looking ahead along it: along its routes' alignments (the nearest segment), whichever way a section or a jumper was entered. A section whose points run against the line is solved with the side swapped, so all spans blow to the same side. The wind acts horizontally, perpendicular to each span, at the full pressure even where a span runs at an angle to the line.

  **Conservative for the transverse loads and the side clearances only:** every span takes the case's full pressure square to itself, whatever its direction. A real wind blows from one direction, so a span at an angle to it takes less (the part normal to it): the transverse load on every span and its swing to the side are the largest any wind of that pressure gives. Exact: one wind direction per case, each span loaded by the part of it normal to the span.

  **Limitation (not on the safe side): the wires take no wind along the line or at an angle to it.** Every case's wind on the wires and jumpers blows across the line, from the case's side. A wind at an angle to the line loads a structure **along** the line through the wires at an angle structure too; those longitudinal wire loads are not produced. The structure **bodies** do take a direction: a load case may give the wind's direction on them (`[case.structure]`, [Wind on structures](structure-wind.md#the-winds-direction)), so a square lattice body loaded on two faces at once, or along the line, can be checked; without a direction their wind blows across the line as the wires'. Exact: a wind direction per load case for the wires too, each span loaded by the part normal to it.

  **Simplification:** one wind pressure per weather case, at every height and along every span. Real wind grows with the height above the ground and varies along a span. The error goes either way: the case's pressure must be chosen for the heights it stands for (a design code's height and span factors belong in the weather cases). Exact: a pressure by height (and a span factor) per wire point.

  A jumper takes the case's full pressure square to each of its pieces, as the spans do, pushed to the case's side of the line. **Conservative**, for the same reason as the spans: a wind along the line would blow a jumper across it out fully. Exact: a wind direction per case.
- **Structure loads** are given in the line's axes (x, y, z) and in the structure's axes: transverse, longitudinal (ahead) and vertical (down) — see [Load cases](../user-guide/loads.md).
