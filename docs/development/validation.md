# Validation cases

Every calculation must be validated against independent reference results: standards, published examples (CIGRE, IEEE, IEC), textbooks or documented hand calculations.

Cases are TOML files under `benchmarks/<module>/`, one or more `[[case]]` per file:

```toml
[[case]]
id = "units-003"                      # unique across the repository
title = "Pound-force to newton"
calculation = "units.convert"         # see `ohl validate --list`
reference = "NIST SP 811 (2008), Appendix B.8 — pound-force"
notes = "Optional assumptions"
max_seconds = 1.0                     # optional time limit (default 10 s)
inputs = { quantity = "force", value = "1 lbf", to = "N" }
expected.value = { value = 4.448222, rel_tol = 1e-6 }   # abs_tol and/or rel_tol
```

Run them with `ohl validate` (today 135 cases in 29 files). Run them locally before each push; the CI workflow (started by hand only) runs every case too and publishes the report. A case fails when an output is outside its tolerance, missing, when the calculation errors, or when it exceeds its time limit.

Engines expose calculations to the runner by implementing `ohl_validation::Calculation`.

Independent references are scripts next to the cases (`benchmarks/<module>/reference_*.py`, and `benchmarks/reference/textbook_reference.py` for the textbook and CIGRE TB 324 cases), run with `python3 -I`: they solve each case by a different method than the code (numerical integration, quadrature, shooting) and print the expected values. The textbook cases also say in their comments how to check them by hand. The textbook and published cases, their sources and tolerances are listed in [Textbook and published cases](../validation/textbook.md).

Two calculations exist for the validation of whole line models and cables: `line.section` (a section of a line file solved by the line's solver, in one weather case, optionally clipped in first as the app does) and `cables.properties` (a cable's composite area, stiffness, modulus and expansion, and the strain at a tension).
