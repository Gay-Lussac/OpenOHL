# Conventions and units

## Units

All values are stored and computed in **SI base units**, as `f64`:

| Quantity | Stored in | Typical display (metric / US) |
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

Conversion factors are the exact definitions (international foot = 0.3048 m, avoirdupois pound = 0.453 592 37 kg, standard gravity \\( g_n = 9.806\,65 \\) m/s²). They are checked against NIST SP 811 in the validation suite.

The type system enforces dimensional consistency, e.g. a distributed load times a length is a force:

\\[ F = w \, L \qquad [\mathrm{N}] = [\mathrm{N/m}] \cdot [\mathrm{m}] \\]

and a wind pressure on a conductor diameter is a load per unit length:

\\[ w_{wind} = q \, d \qquad [\mathrm{N/m}] = [\mathrm{Pa}] \cdot [\mathrm{m}] \\]

## Numerical exit clauses

Every iterative method documented in this manual states its convergence criterion and its iteration and time limits. When a limit is reached the calculation fails with an explanation instead of returning an unconverged value (see [Performance and time budgets](../development/performance.md)).

## Coordinate conventions

To be specified with the line data model (Phase 2): project coordinate reference system, stationing, offsets (positive to the right looking ahead), elevations and the local structure axes.
