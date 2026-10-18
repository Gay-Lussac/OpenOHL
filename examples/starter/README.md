# Starter template — example libraries for learning

**Every value here is invented or a typical textbook value: nothing in this
folder is a design code, manufacturer data or a real line. Do not use it for
design.**

OHL-Design offers this folder as **File › New project… › Starter template
(for learning)** (also on the start page: **New project from the starter
template…**). The new project gets the library files below and nothing
else: **no route, alignment, structure or wire** — the tutorial
(Help › Start from scratch) builds those, step by step. The tutorial uses
the same files.

The project's coordinate system is `local` (plane coordinates in metres,
no geographic reference); units are SI.

| Folder / file | Kept as | What it is |
|---|---|---|
| `cables/acsr-examples.olsw` | the project's wire library | ACSR conductors with typical published values (a copy of `libraries/cables`) |
| `structures/starter-suspension.olss` | structure type (level 1) | built from the pylon template `delta-1c`, suspension, its default (invented) dimensions |
| `structures/starter-strain.olss` | structure type (level 1) | the same template, strain |
| `insulators/*.olsi` | insulator library | example I, V and tension strings, typical values (a copy of `libraries/insulators`) |
| `weather/starter-weather-cases.olswc` | weather cases | everyday, maximum temperature, glaze ice, high wind, minimum temperature — invented |
| `loadcases/starter-load-cases.olslc` | load cases | three load cases with invented factors |
| `rules/starter-clearance-rules.olsr` | clearance rules | ground and phase-to-phase distances — invented |
| `pylon-templates/*.olspt` | copied beside the project, into `Pylon templates/` | `delta-1c` and `barrel-2c` (as shipped in `libraries/pylon-templates`) |
| `starter-survey.csv` | copied beside the project | survey points of invented ground (x about −100 to 1100 m, local coordinates; the ground of `examples/synthetic-line`), to import with Terrain › Terrain surfaces (TIN) |

The two structure types are what the pylon builder makes from
`pylon-templates/delta-1c.olspt` with its defaults; a test
(`starter_structure_types_are_built_from_its_templates` in
`crates/ols-app/src/starter.rs`) keeps them so, and
`OLS_WRITE_STARTER=1 cargo test -p ols-app starter_` writes them again
after a template changes.
