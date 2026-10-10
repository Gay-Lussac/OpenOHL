# Summary

[Introduction](introduction.md)

# User guide

- [Getting started](user-guide/getting-started.md)
- [Desktop application](user-guide/desktop-app.md)
- [Wire library files (.olsw)](user-guide/wire-library.md)
- [Line files (.ols.json)](user-guide/line-file.md)
- [Weather cases (.olswc)](user-guide/weather.md)
- [Load cases and structure loads (.olslc)](user-guide/loads.md)
- [Design codes](user-guide/design-codes.md)
- [Structure and insulator libraries (.olss, .olsi)](user-guide/structure-library.md)
- [Structure analysis (.olsfe, .olsp)](user-guide/structure-analysis.md)
- [Terrain and ground clearance](user-guide/terrain.md)
- [Thermal rating (.olsta)](user-guide/thermal.md)
- [Bill of materials and cost estimate (.olscost)](user-guide/bom-and-cost.md)
- [Optimum structure spotting](user-guide/spotting.md)
- [Galloping, blowout, vegetation and as-built wires](user-guide/analyses.md)
- [Drawings and profile exports](user-guide/drawings.md)
- [Coordinate systems](user-guide/coordinates.md)
- [Import and export](user-guide/import-export.md)
  - [OLS XML schema](schema/ols-line.md)
- [Project files and point clouds (.ols)](user-guide/project.md)
- [Server mode (alpha)](user-guide/server.md)
- [Project lines and phases](user-guide/project-lines.md)
- [Versions, branches and reviews](user-guide/versions.md)

# Theory manual

- [Conventions and units](theory/conventions.md)
- [Sag-tension](theory/sagtension.md)
- [Finite-element wire systems](theory/wirefe.md)
- [Jumpers](theory/jumpers.md)
- [Thermal rating](theory/thermal.md)
- [Optimum structure spotting](theory/spotting.md)
- [Structure analysis: frames, trusses, guys and poles](theory/structures-fe.md)
- [Galloping ellipses and blowout](theory/galloping.md)
- [LiDAR analyses](theory/lidar-analysis.md)

# Design

- [Design goals and core requirements](DESIGN_GOALS.md)
- [Architecture](ARCHITECTURE.md)
- [Roadmap](roadmap.md)
- [Architecture decision records](adr/README.md)
  - [ADR-0001: License](adr/0001-license.md)
  - [ADR-0002: Tech stack](adr/0002-tech-stack.md)
  - [ADR-0003: Plugin mechanism](adr/0003-plugin-mechanism.md)
  - [ADR-0004: Project file format](adr/0004-project-file-format.md)
  - [ADR-0005: Collaboration / session model](adr/0005-collaboration-session-model.md)
  - [ADR-0006: Rendering, LOD & streaming](adr/0006-rendering-lod-streaming.md)
  - [ADR-0007: Point clouds stored in the project file](adr/0007-point-clouds-in-the-project.md)
  - [ADR-0008: Spatial chunk index](adr/0008-spatial-chunk-index.md)
  - [ADR-0009: GUI toolkit — egui with wgpu](adr/0009-gui-toolkit.md)
  - [ADR-0010: Version control, edit rights and reviews](adr/0010-version-control.md)
  - [ADR template](adr/0000-template.md)

# Development

- [Validation cases](development/validation.md)
- [Writing plugins](development/plugins.md)
- [Data model map](development/data-model.md)
- [Performance and time budgets](development/performance.md)
- [Desktop app conventions](development/ui.md)
- [Structure detail levels in checks](development/structure-detail.md)

# Validation

- [Validation report](validation/report.md)
