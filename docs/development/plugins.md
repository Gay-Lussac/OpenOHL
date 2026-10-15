# Writing plugins

OLS CAD has three plugin tiers (ADR-0003): **WebAssembly** components (stable interface, sandboxed, any language), **Python** (scripts, reports, import/export) and native Rust inside this repository.

Every plugin has an `ols-plugin.toml` manifest:

```toml
[plugin]
name = "hello-wasm"
version = "0.1.0"
description = "Example plugin"
sdk = "0.1"                 # plugin interface version targeted
tier = "wasm"               # "wasm" or "python"
entry = "target/wasm32-wasip2/release/hello_wasm.wasm"
extension-points = []

[permissions]               # everything denied by default
network = false
filesystem = []
```

### Table columns

A plugin may add columns to the app's tables, declared in its manifest
(any tier; nothing to implement):

```toml
[[column]]
table = "circuits"         # structures, sections, circuits, systems,
                           # jumpers, alignments or routes
name = "insulation_level"  # unique in the plugin: letters, digits, _ -
title = "insulation level" # header (default: name)
help = "…"                 # shown on the header
values = ["low", "high"]   # a dropdown; none: typed
default = "high"           # shown, dimmed, where not set
```

The values are kept on the objects, in their `fields` under
`plugin.name` (here `lev-ch.insulation_level`; a circuit's on all its
sections), saved with the project and in OLS XML (`<field key value>`).
The plugin, or the check that belongs to it, reads them there. Without
the plugin, a project keeps the values and the tables still show them
in a column marked "(unused)", typed like text. The Swiss LeV plugin
declares the circuits' insulation level this way (the distance between
conductors reads it).

## WebAssembly plugins

The interface is defined in WIT: `crates/ols-plugins/wit/plugin.wit`. A Rust example lives in `plugins/examples/hello-wasm`:

```bash
rustup target add wasm32-wasip2
cd plugins/examples/hello-wasm && cargo build --release --target wasm32-wasip2
cargo run -p ols-cli -- plugin info plugins/examples/hello-wasm
```

Plugins run sandboxed — no file system, network, environment variables or standard I/O — with **exit clauses** on every call: a time limit (default 5 s; a runaway plugin is interrupted and reported as a timeout) and a memory cap (default 256 MiB). `plugins/tests/runaway` is a deliberately endless plugin used to test this.

## File-format plugins

A plugin that adds import or export formats implements the `io-plugin` world of the same WIT file and lists the extension point `io` in its manifest:

```toml
extension-points = ["io"]
```

- `formats()` lists its formats: id, description, extension and direction (exporter or importer).
- `export-model(format, model, options)` receives the solved model as JSON and returns the file's bytes. The model is `{"format": "ols-export-model", "version": 1, "crs": …, "line": <line file>, "cases": [{"name", "summary", "wires": [{"section", "span", "circuit", "phase", "color", "system", "voltage_kv", "horizontal_tension_n", "max_tension_n", "sag_m", "points": [[x, y, z], …]}]}]}` (coordinates in the line's coordinate system; jumpers have section `jumper:<id>` and no horizontal tension; tensions per wire, N; sag in m).
- `import-file(format, data, options)` returns a line file (JSON), which the host checks like any line file. When the user gives a line to update, it arrives as option `into` (its JSON).
- Errors are returned as strings and shown to the user as the format's error.

`plugins/examples/geojson` is a complete example (GeoJSON export of structures and wires; structures imported from GeoJSON points). Its formats appear next to the built-in ones in `ols formats --plugin plugins/examples/geojson`; see [Import and export](../user-guide/import-export.md).

## Terrain plugins

A plugin that reads terrain surfaces implements the `terrain-plugin` world and lists the extension point `terrain`:

```toml
extension-points = ["terrain"]

# Large files: more than the default 5 s and 256 MiB per call (granted up
# to 600 s and 4 GiB).
[limits]
seconds = 60
memory-mib = 1024
```

- `formats()` lists the formats it reads (direction `importer`).
- `import-surfaces(format, data, options)` returns surfaces: a name, the points as `x0, y0, z0, x1, …` (metres, x east, y north, z up) and the triangles as three point indices each. The host checks them like any mesh and keeps them in the project.

`plugins/examples/esri-grid` reads ESRI ASCII grids (`.asc`): a point per cell centre, two triangles per square of cells with data. Its format appears in `ols surface formats --plugin plugins/examples/esri-grid` and in Terrain › Terrain surfaces once the plugin is installed or its folder added (Plugins › Plugin manager).

A plugin also suits formats that should not be in the open-source code — e.g. one read from a private specification — kept and installed only where it may be used.

## Project plugins

A plugin that reads another program's project files implements the `project-plugin` world and lists the extension point `project` (with `[limits]` for large files, as above):

- `formats()` lists the formats it reads.
- `import-project(format, data, options)` returns a **project**: the line as a line file (JSON), its **library files** (`kind` — `cables`, `weather`, `loadcases`, `structures`, `insulators` — `name` and TOML `text`, as OLS CAD writes them) and **notes**: what was read, what was left out, what was assumed. The host checks the line and creates a new project file with it and its libraries (File › New project from another program, `ols project import-from`), and shows the notes.

`plugins/examples/project-bundle` reads a JSON bundle `{"line": …, "libraries": [{"kind", "name", "text"}]}` — the shape such a reader returns. Readers of formats that should stay out of the open-source code are installed only where they may be used.

## Design-code plugins

A plugin for a design code implements the `code-plugin` world
(`crates/ols-plugins/wit/code.wit`) and lists the extension point
`design-code`:

```toml
extension-points = ["design-code"]
```

- `describe()` returns its name, version, a one-line description and a
  **parameters template**: a TOML parameter file with its schema in
  comments and placeholder values.
- `generate(parameters)` reads the user's parameter file (TOML text) and
  returns **generated**: `weather-cases` (an `.olswc` file's text),
  `load-cases` (an `.olslc` file's text, each case naming one of those
  weather cases) and `notes` (what was computed, assumed or left out). A
  wrong file is an error string, shown to the user.

The host (`ols_io::codes`, `WasmHost::load_code`) checks both files as
OLS CAD reads them before anything is written; `ols code generate` and
Criteria › Design code write them. A plugin ships **no values of the
standard**: it implements the code's method, every number comes from the
parameter file, and the template's numbers are invented placeholders,
marked as such.

`plugins/codes/*` are such plugins (EN 50341 with a national annex overlay,
Swiss LeV, IEC 60826, NESC, CSA C22.3, ASCE MOP 74, GO 95); they share
`plugins/codes/common` (reading parameter files, writing the case files,
the `code_plugin!` macro implementing the world). See
[Design codes](../user-guide/design-codes.md).

## Check plugins

A plugin that runs a check implements the `check` interface
(`crates/ols-plugins/wit/code.wit`: the `check-plugin` world, or
`code-check-plugin` for a design-code plugin that also runs checks) and
lists the extension point `check`. **OLS CAD measures, the plugin holds
the values and the rules**: the measuring stays in OLS CAD (it is
geometry, and fast), the code's numbers and its formulas stay out of it.

- `checks()` lists the checks it runs: the **id** of what OLS CAD
  measures for it (today `phase-distance`, the distance between
  conductors in the span), its title and Checks submenu, an
  **explanation** of the computation (shown in its window), its
  **values** file as shipped (TOML) with its extension, the **choices**
  made before a run (e.g. the k diagram), and the plugin columns of the
  circuits its window shows to be set there.
- `value-tables(check, values)` shows a values file as tables (shown
  locked in the window, for transparency); `edit-value(check, values,
  table, row, column, text)` returns the file with that cell changed (an
  admin's edit, kept as the project's copy), or what is wrong.
- `evaluate(check, values, choices, pair, stations)`: for one pair of
  conductors (`conductor-pair`: same system, same span, each one's kV,
  plugin columns, diameter and mass, the mean height, the largest sags
  between spacers when spacers join them) and its stations (6 numbers
  each: distance, β, each local sag, each l_k), the numbers required at
  each station — the distance first, then the check's own (kept with the
  worst station) — and the rule applied.

The host (`ols_io::checks`, `WasmHost::load_check`) measures
(`ols_clearance::phase_distance`), calls the plugin once per pair, one
instance per core (`WasmHost::another_check`, compiled once), and keeps
each pair's worst station and the length below. The `lev-ch` plugin runs
the LeV distance between conductors (Annex 6, 2.2.1 and, with phase
spacers, 2.2.4); its values are the law's (Swiss legislation is not
protected by copyright), in `plugins/codes/lev-ch/phase-distance.olspd`.
On an existing line (930 conductors, 5593 pairs) it takes about as long
as the same computation built into OLS CAD (0.8 s against 0.65–0.9 s;
with nearby pairs, 10 475, 0.9 s against 0.65–0.7 s), plus about 0.3 s
to load the plugin once.

## Python plugins

Python plugins are scripts (`tier = "python"`, `entry` the script). Outside the application they use the `ols` package built from `crates/ols-py`; inside it, the Python console runs them (Plugins › Python console › Plugin scripts) with the `ols` module of the application, which also reads and edits the open project ([Python console](../user-guide/desktop-app.md#python-console)). See `plugins/examples/hello-python`.

The desktop application embeds Python through its `python` feature (on by default): building it needs a shared Python 3.10+ (`PYO3_PYTHON` chooses which; a static one, such as Anaconda's, cannot be embedded). Without the feature (`--no-default-features`), the console says so.
