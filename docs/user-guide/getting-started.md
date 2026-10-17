# Getting started

OLS CAD is pre-alpha: the engines and the desktop application work on real lines, but every result must be checked by a qualified engineer. Build it with a Rust toolchain (`cargo`); the examples below run from the top of the source tree.

## Desktop application

```bash
cargo run --release -p ols-app -- examples/synthetic-line/synthetic-line.ols.json \
    --terrain examples/synthetic-line/synthetic-survey.csv \
    --weather examples/synthetic-line/weather-cases.olswc
```

See [Desktop application](desktop-app.md).

### A new line, step by step

A line is laid out in three steps, each needing the one before: a **route**, its **alignments** (each on one route), and the **alignment points** of each alignment, in order.

1. **The route** — File › New project, then name its route (the first window of a new project asks for it; Terrain › Routes later). A route is the path of the line; everything else is put on it.
2. **Its alignment and its points** — the New alignment tool is held: right-click its points in order (an existing alignment point is used as it is — shared, a junction; a survey or LiDAR point or a structure snaps; elsewhere you type E, N and height), Enter, choose its route (with no route yet, it offers to create one). Or place the points first with the **Add alignment point** tool (not connected), then pick them one after another with New alignment; **Edit alignment** later reroutes a stretch, extends an end, or branches off; or type them in Terrain › Alignments and Terrain › Alignment points.
3. **The ground and survey** — Terrain › Terrain surfaces, LiDAR point clouds (they can come before step 2, to snap the points to them).
4. **The structures** — Structures › **New pylon from a template…** (the face of the pylon: its crossarm levels and lengths; build the suspension type, switch to Strain, build the strain type), then **Place on alignment points…** (the suspension type on every point, the strain type on the angle points: one command; each turned along the line, then it stays as placed — Properties › Placement… › Put it on the line again). One at a time: Structures › Add structure… or the Add structure tool. They go on the route of the alignment they stand on.
5. **Their strings** — the Add string tool, or the structure's type.
6. **The wires** — the String wire tool, section by section, then the jumpers.
7. **The electrical systems**, then **the checks** (clearances, loads, …).

See [Routes and alignments](desktop-app.md#routes-and-alignments).

## Command-line tool

`ols` runs every calculation without a window. `ols --help` lists the commands, `ols <command> --help` their options.

```bash
cargo run -p ols-cli -- info               # version information
cargo run -p ols-cli -- validate           # run all validation cases in benchmarks/
cargo run -p ols-cli -- validate --list    # calculations available to cases
cargo run -p ols-cli -- plugin info plugins/examples/hello-wasm
```

`ols validate` exits with a non-zero code if any case fails or exceeds its time limit, and `--report report.md` writes a Markdown report.

| Command | What it does | See |
|---|---|---|
| `ols sagtension` | sag-tension of one tension section from a job file | below |
| `ols line` | solves a line file in 3D: clearances, structure loads, usage, exports; given a project, with the libraries it keeps | [Line files](line-file.md), [Terrain and ground clearance](terrain.md), [Load cases](loads.md), [Project files](project.md) |
| `ols zones` | vertical and radial clearances to the ground and LiDAR classes in the wind, by a rule file (the desktop clearance windows' check); given a project, with the libraries it keeps | [Terrain and ground clearance](terrain.md#vertical-and-radial-clearances-without-the-window-ols-zones) |
| `ols plugin info` | loads a plugin of any kind (design code, check, file format, terrain, project, minimal) and prints what it describes: name, version, world, formats or checks | [Plugins](../development/plugins.md) |
| `ols drawing` | plan and profile sheets and profile exports | [Drawings](drawings.md) |
| `ols structures` | derive structure types; frame/truss and pole analysis | [Structure libraries](structure-library.md), [Structure analysis](structure-analysis.md) |
| `ols code` | weather and load cases from a design-code plugin | [Design codes](design-codes.md) |
| `ols thermal` | ampacity and clearance-limited temperature | [Thermal rating](thermal.md) |
| `ols bom` | bill of materials and cost estimate | [Bill of materials](bom-and-cost.md) |
| `ols spot` | optimum structure spotting along a route | [Spotting](spotting.md) |
| `ols clearance` | galloping ellipses, blowout against the right of way | [Analyses](analyses.md) |
| `ols lidar` | LAS/LAZ information, classification, vegetation, as-built wires | [Analyses](analyses.md), [Terrain](terrain.md) |
| `ols project`, `ols pointcloud`, `ols surface` | the project file: commands, undo, versions, point clouds, terrain surfaces | [Project files](project.md), [Versions](versions.md) |
| `ols formats`, `ols import` | file formats for import and export | [Import and export](import-export.md) |
| `ols crs` | coordinate systems | [Coordinate systems](coordinates.md) |
| `ols serve` | hosts a project for several users | [Server mode](server.md) |

## Sag-tension of a tension section

```bash
cargo run -p ols-cli -- sagtension examples/sagtension/drake-section.toml
cargo run -p ols-cli -- sagtension examples/sagtension/drake-section.toml --units us --csv chart.csv
```

The job file names the cable (from the `.olsw` wire libraries in `libraries/cables/` — see [Wire library files](wire-library.md)), the spans (horizontal length and rise), the weather cases (temperature, radial ice, wind pressure), and either a fixed stringing tension or tension criteria. The report gives the ruling span, the governing stringing tension with the utilisation of every criterion, the tension and sag in each weather case, and a stringing chart; `--csv` exports the chart in SI units.

## Python

```bash
pip install ./crates/ols-py
python -c "import ols; print(ols.convert('length', '1000 ft', 'm'))"   # 304.8
```

Quantities given as text must always carry a unit (`"1000 ft"`, `"16 N/m"`, `"-20 °C"`); a bare number is rejected rather than guessed.
