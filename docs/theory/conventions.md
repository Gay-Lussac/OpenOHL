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
- **Library files** (TOML: cables, insulators, structure types, weather and load cases): every value carries its unit as text (`"12.5 m"`, `"20 °C"`, `"30 deg"`), any supported unit, converted to SI when read.

The conversion to radians and kelvin happens when the files are read; nothing is stored in radians or kelvin.

Conversion factors are the exact definitions (international foot = 0.3048 m, avoirdupois pound = 0.453 592 37 kg, standard gravity \\( g_n = 9.806\\,65 \\) m/s²). They are checked against NIST SP 811 in the validation suite.

The type system enforces dimensional consistency, e.g. a distributed load times a length is a force:

\\[ F = w \\, L \qquad [\mathrm{N}] = [\mathrm{N/m}] \cdot [\mathrm{m}] \\]

and a wind pressure on a conductor diameter is a load per unit length:

\\[ w_{wind} = q \\, d \qquad [\mathrm{N/m}] = [\mathrm{Pa}] \cdot [\mathrm{m}] \\]

## Numerical exit clauses

Every iterative method documented in this manual states its convergence criterion and its iteration and time limits. When a limit is reached the calculation fails with an explanation instead of returning an unconverged value (see [Performance and time budgets](../development/performance.md)).

## Coordinate conventions

- **Project coordinates**: a projected coordinate system in metres (the line's `crs`, e.g. `EPSG:2056`); x east, y north, z up. Heights are kept as given — usually orthometric — and no geoid model is applied ([Coordinate systems](../user-guide/coordinates.md)).
- **Stations and offsets**: stations are measured along a route's alignment (or its structures); offsets are positive to the right looking ahead ([Line files](../user-guide/line-file.md)).
- **Structure axes**: origin at the structure's ground centre; x to the right of the line (transverse), y ahead (along the line), z up. `orientation_deg` is the direction of the local x axis, degrees counter-clockwise from east.
- **Angles around a structure** as shown in the app (string directions, Structures › Angles around structures) turn **clockwise seen from above**, with *ahead* at the project's angle (the line's display setting `ahead_deg`, 0° by default; e.g. 270: ahead 270°, right 0°): right is ahead + 90°, back ahead + 180°, left ahead + 270°. This is a display convention only; stored positions do not depend on it.
- **Wind** comes from the left or the right of the line, looking ahead along it: along its routes' alignments (the nearest segment), whichever way a section or a jumper was entered. A section whose points run against the line is solved with the side swapped, so all spans blow to the same side. The wind acts horizontally, perpendicular to each span. A jumper takes the line's wind across itself: the part normal to it, by \\( \cos^2 \\) of the angle between the wind and its normal, so all of it for a jumper along the line, little for one across it.
- **Structure loads** are given in the line's axes (x, y, z) and in the structure's axes: transverse, longitudinal (ahead) and vertical (down) — see [Load cases](../user-guide/loads.md).
