# ADR-0006: Rendering, LOD & streaming

- **Status:** Accepted
- **Date:** 2026-10-01

## Context
Large projects (hundreds of km, billions of LiDAR points) must open fast and render smoothly (DESIGN_GOALS §1, §5). LOD must never alter the engineering model.

## Decision
- **Renderer**: `wgpu` (Vulkan / Metal / DX12, WebGPU for a future web client); one renderer for plan, profile (with vertical exaggeration) and 3D views.
- **Precision**: camera-relative rendering — `f64` on the CPU, positions relative to the camera sent as `f32` to the GPU (avoids jitter with UTM-scale coordinates).
- **Chunks**: fixed quadtree grid of map tiles in the project CRS (e.g. 256 m base tiles), independent of the alignment.
- **Render distance** per layer (terrain, point cloud, structures, wires, labels) + automatic mode targeting a frame rate.
- **LOD**:
  - Terrain: quadtree LOD with simplified meshes per level.
  - Point clouds: COPC octree streaming with a per-frame point budget and screen-space error.
  - Structures: GPU instancing; LOD0 full lattice → LOD1 simplified → LOD2 line/icon, chosen by screen-space size.
  - Wires: GPU receives catenary parameters and tessellates adaptively (distance, curvature) — instant update while dragging.
  - Labels: decluttering and distance fading.
- **Culling**: frustum + occlusion.
- **Picking**: GPU ID buffer.
- **Threads**: loading and meshing on background threads; the UI thread never blocks.

## Consequences
- Structure library items need several mesh LODs (auto-generated where possible).
- The chunk grid is shared by rendering, project file indexing (ADR-0004) and server interest management (ADR-0005).
