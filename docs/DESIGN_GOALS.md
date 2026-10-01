# Design Goals & Core Requirements

OLS CAD aims to be **faster and more user-friendly** than existing overhead line design tools. This document records the requirements that every module must respect. Changes go through an ADR.

## 1. Performance first

- The computation core is written in a **compiled, memory-safe language (Rust)** — no interpreted code in hot paths.
- Python is a **scripting and plugin layer only**, never required for the core to run.
- The GUI renders on the GPU and never blocks on computation: calculations run on background threads and results stream in.
- Every performance-sensitive feature ships with a benchmark tracked in CI; regressions fail the build.

### Performance targets (initial, to be refined)

| Operation | Target |
|---|---|
| Application cold start | < 2 s |
| Open a 500-structure / 100 km project (without point cloud) | < 2 s to first interactive frame |
| Re-solve one tension section after an edit (sag-tension, all weather cases) | < 10 ms (interactive while dragging) |
| Clearance check, full 100 km line, all weather cases | < 1 s |
| Finite-element wire system, one 20-span section, one load case | < 100 ms |
| 3D view with LiDAR (≥ 1 billion points, streamed) | ≥ 60 fps, first frame < 1 s |
| Change propagated to other users in a shared session (LAN) | < 100 ms |

## 2. User-friendliness

- Direct manipulation: drag structures on the profile, wires and clearances update live.
- No modal dialogs for core workflows; property panels with immediate feedback.
- Every number shows its units; switch SI/US customary at any time.
- Every result is explainable: click a value → see the inputs, model and standard clause that produced it.
- Unlimited undo/redo, full project history.
- Sensible defaults and templates; a new user can produce a sag-tension table in minutes.
- Keyboard-driven workflows and a command palette for power users.

## 3. Loads on cables

In addition to self-weight, wind and ice, the core must support **additional loads applied on wires**, in every analysis (sag-tension, clearances, FE wire system, structure loads):

| Type | Unit | Examples |
|---|---|---|
| **Point (concentrated) load** at a position along the span | N (vector: vertical, transverse, longitudinal) | Aircraft warning markers, spacers, dampers, bird diverters, line hardware, line cart / worker, sleeves, signage |
| **Distributed load** over the full span or a sub-interval | N/m (vector) | Partial ice / ice shedding, partial wind, cable-mounted fibre (ADSS lashed), heating strips, de-icing equipment |

Requirements:
- Loads can be defined **per weather/load case** (e.g. worker only in maintenance case), and as **permanent** (present in every case).
- Positions defined by distance from either support, fraction of span, or station.
- Analytic treatment where possible (segmented catenary between point loads); general case through the FE wire solver.
- Loads affect creep/final conditions where physically relevant (permanent loads).
- Exposed in the plugin API so tools can generate loads (e.g. a marker-ball placement tool).

## 4. Collaborative multi-user editing ("server mode")

A model can be opened as a **shared session**, like a game server: one machine hosts the project, several users connect and work on it at the same time, seeing each other's changes live.

This is a **core** requirement, not a GUI feature, because it dictates how the data model works:

- **Authoritative server**: one process owns the model. Clients send *commands* (move structure, change tension, add point load…); the server validates, applies, and broadcasts the resulting *changes*.
- **Single-user mode is the same thing**: the desktop app runs an embedded in-process server. There is only one code path.
- **Command / operation log**: every edit is a serialisable command → gives undo/redo, full history, audit trail ("who changed this tension, when"), and replay for free.
- **Conflict handling**: fine-grained object versioning; optional soft locks on objects/sections being edited; presence (see where others are working).
- **Permissions**: roles (viewer, designer, approver), per-object or per-area editing rights.
- **Compute placement**: calculations can run on the server (shared results, thin clients) or locally on clients.
- **Transport**: binary protocol over WebSocket/QUIC, encrypted; works on LAN and Internet. A future web client can join the same session.
- Offline work with later sync is a stretch goal.

## 5. Render distance, LOD and streaming

Large projects (hundreds of km, billions of LiDAR points) must open fast and stay smooth:

- **Spatial chunking**: the project world is divided into tiles/chunks along the line (like Minecraft chunks). Only chunks near the viewpoint are loaded in memory and on the GPU.
- **Render distance** setting (user-adjustable), separately for terrain, point cloud, structures, wires and labels.
- **Level of Detail (LOD)**:
  - Terrain: quadtree tiles with progressively simplified meshes.
  - Point clouds: octree streaming (COPC/LAZ-based), point budget per frame.
  - Structures: several mesh LODs (full lattice → simplified → billboard/icon).
  - Wires: adaptive segment count by distance and curvature.
  - Labels/annotations: decluttering and distance fading.
- **Lazy loading**: opening a project loads metadata and the visible area first; the rest streams in the background.
- **Frustum and occlusion culling**, GPU instancing for repeated structures and hardware.
- The same chunking is used by the server to send clients only what they need.

The **engineering model is never simplified** by LOD: calculations always use full-fidelity data. LOD affects display and loading only.
