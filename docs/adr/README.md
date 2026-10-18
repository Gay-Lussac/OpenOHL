# Architecture decision records

> The project was renamed from "OLS CAD" to **OpenOHL** (the line application **OHL-Design**, `ohl-design`; the structure application **OHL-Structure**, `ohl-structure`; the command line `ohl`). The records below keep the names of the time they were written.

| # | Title | Status |
|---|---|---|
| [0001](0001-license.md) | License (MPL-2.0) | Accepted |
| [0002](0002-tech-stack.md) | Tech stack (Rust) | Accepted |
| [0003](0003-plugin-mechanism.md) | Plugin mechanism (WASM + Python + in-tree Rust) | Accepted |
| [0004](0004-project-file-format.md) | Project file format (SQLite + JSON export) | Accepted |
| [0005](0005-collaboration-session-model.md) | Collaboration / session model | Accepted |
| [0006](0006-rendering-lod-streaming.md) | Rendering, LOD & streaming | Accepted |
| [0007](0007-point-clouds-in-the-project.md) | Point clouds stored in the project file (amended: levels of detail, COPC) | Accepted |
| [0008](0008-spatial-chunk-index.md) | Spatial chunk index | Accepted |
| [0009](0009-gui-toolkit.md) | GUI toolkit: egui with wgpu | Accepted |
| [0010](0010-version-control.md) | Version control, edit rights and reviews in the project file | Accepted |

Template: copy `0000-template.md`, give it the next number, open a PR.
