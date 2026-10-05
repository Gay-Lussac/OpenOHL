# Getting started

OLS CAD is pre-alpha: the engines and the desktop application work on real lines, but every result must be checked by a qualified engineer. Build it with a Rust toolchain (`cargo`); the examples below run from the top of the source tree.

## Desktop application

```bash
cargo run --release -p ols-app -- examples/synthetic-line/synthetic-line.ols.json \
    --terrain examples/synthetic-line/synthetic-survey.csv \
    --weather examples/synthetic-line/weather-cases.olswc
```

See [Desktop application](desktop-app.md).

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
| `ols line` | solves a line file in 3D: clearances, structure loads, usage, exports | [Line files](line-file.md), [Terrain and ground clearance](terrain.md), [Load cases](loads.md) |
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
