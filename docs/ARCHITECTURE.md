# Architecture

> Draft — subject to the ADRs in [adr/](adr/).

## Principles

1. **Layered**: data model → engines → application. Lower layers never depend on upper ones.
2. **Headless engines**: every calculation runs without a GUI (library + CLI), so it can be scripted, tested and run in CI or on servers.
3. **Interfaces, not implementations**: modules communicate through documented interfaces (abstract base classes / protocols). Any implementation can be swapped by a plugin.
4. **Open data**: the project file is an open, versioned, human-readable schema (JSON + binary sidecars for large data like point clouds).
5. **Traceability**: every result can report which models, standards and parameters produced it.

## Layers

```
┌───────────────────────────────────────────────────────────────┐
│ Applications        ols-gui (desktop) · ols-cli · notebooks   │
├───────────────────────────────────────────────────────────────┤
│ Engines             ols-sagtension · ols-loads · ols-clearance│
│                     ols-wirefe · ols-structures-fe · ols-rating│
│                     ols-spotting · ols-drafting · ols-reports  │
├───────────────────────────────────────────────────────────────┤
│ Domain libraries    ols-cables · ols-terrain · ols-structures │
│                     ols-weather · ols-io                       │
├───────────────────────────────────────────────────────────────┤
│ Core                ols-core (data model, units, geometry,     │
│                     coordinate systems, plugin registry, events)│
└───────────────────────────────────────────────────────────────┘
```

## Modules

| Package | Responsibility |
|---|---|
| `ols-core` | Project data model, units, geometry primitives, CRS, plugin registry, event bus, schema & migrations |
| `ols-cables` | Cable/conductor library, stress-strain & creep models |
| `ols-weather` | Weather cases, load case definitions (code-agnostic) |
| `ols-sagtension` | Catenary, change of state, ruling span, stringing charts |
| `ols-terrain` | Survey points, feature codes, TIN, profiles, LiDAR |
| `ols-structures` | Structure/framing/insulator libraries, attachment geometry |
| `ols-wirefe` | Non-linear finite-element model of wire systems |
| `ols-structures-fe` | 3D frame/truss FE of towers and poles |
| `ols-loads` | Load-case engine; design codes are **plugins** (`ols-code-nesc`, `ols-code-iec60826`, …) |
| `ols-clearance` | Clearance checks, envelopes, violation reporting |
| `ols-rating` | Thermal rating (IEEE 738, CIGRE) |
| `ols-spotting` | Automatic/optimum structure spotting |
| `ols-drafting` | Plan & profile sheets, DXF/PDF |
| `ols-io` | Import/export formats (each format is a plugin) |
| `ols-gui` | Desktop application |
| `ols-sdk` | Public plugin API, templates, test helpers |

## Plugin system (proposal — ADR-0003)

Plugins are ordinary Python packages that declare **entry points** in named groups. The core discovers them at start-up.

| Entry-point group | Example plugin |
|---|---|
| `ols.cable_models` | a new stress-strain or creep model |
| `ols.design_codes` | NESC, IEC 60826, EN 50341-2-x |
| `ols.importers` / `ols.exporters` | LAS, LandXML, DXF, Shapefile |
| `ols.checks` | custom clearance or utility-specific rule |
| `ols.reports` | custom report templates |
| `ols.solvers` | alternative sag-tension or FE solver |
| `ols.gui_tools` | new panels, views, commands in the desktop app |

```toml
# pyproject.toml of a third-party plugin
[project.entry-points."ols.design_codes"]
my_utility_standard = "my_plugin.codes:MyUtilityStandard"
```

Each plugin declares the SDK version it targets; the registry refuses incompatible plugins with a clear message.

## Tech stack (proposal — ADR-0002)

- **Language**: Python 3.12+ for the whole platform (low barrier for power engineers to contribute); performance-critical kernels (FE solvers, LiDAR processing) in Rust via PyO3 or NumPy/Numba where needed.
- **Numerics**: NumPy, SciPy; `pint` for units.
- **Geospatial**: PROJ (`pyproj`), Shapely, `laspy`/PDAL for point clouds.
- **GUI**: Qt 6 (PySide6) + VTK (via PyVista) for 3D.
- **File format**: JSON Schema-validated project file; Parquet/LAZ sidecars for bulk data.
- **Quality**: pytest, hypothesis, ruff, mypy, GitHub Actions.
- **Docs**: MkDocs Material (theory manual + API).
