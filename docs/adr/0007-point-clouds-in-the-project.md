# ADR-0007: Point clouds stored in the project file

- **Status:** Accepted (amends the point-cloud part of ADR-0004)
- **Date:** 2026-10-02

## Context
ADR-0004 kept point clouds outside the project, in COPC/LAZ files. In practice survey tiles are delivered once and then read many times (ground surface, clearances to every class, every weather case). Reading LAZ means decompressing every point of every tile — about one minute per 10⁸ points — even when a check needs only the ground classes in a narrow corridor. The tiles should only be the way data gets *into* the model.

## Decision
- Point clouds are **imported into the project file** (`.ols`, SQLite, ADR-0004) as **blocks**: one block per square chunk of the plan (default 50 m; 64 m since [ADR-0008](0008-spatial-chunk-index.md), to nest in the grid tiles) and per class.
  - Coordinates as integers of the cloud's scale (default 1 mm; lossless for LAS data at 1 mm) relative to the chunk; columns sorted, delta-encoded, byte-shuffled and **zstd**-compressed. Optional attribute columns (intensity, returns) are stored separately and decoded only when asked for.
  - A query selects blocks by chunk (corridor) and by class before reading anything; blocks are decompressed in parallel.
  - The cloud keeps its **class scheme** (`.olsc` text) and the list of imported sources.
- LAS/LAZ (and later COPC, E57…) tiles are **import sources**; they can still be read directly.
- All readers implement one **point source** interface (area + classes → points). A link to an external point database is a **plugin** implementing that interface.

## Consequences
- Project files grow by roughly the compressed size of the cloud (same order as LAZ).
- Checks on a few classes in a corridor read a small part of the data instead of decompressing all of it.
- The block format is part of the versioned project schema; changing it needs a migration.

## Amendment (2026-10-05): levels of detail, COPC
ADR-0006 asks for point clouds streamed by octree under a per-frame point budget. A view far away should not decode whole blocks to show a sixty-fourth of their points.

- **Levels of detail per block** (point-cloud schema 2): a table `pointcloud_lod(block, level, count, z1, xyz)` holds, for levels 1–3, the coordinates of the block's points of that level — about 1/4, 1/16, 1/64 of them, chosen by a hash of the point's index in the block (spread evenly, nested, reproducible) — and the block's top `z1`. Coordinates only, encoded as the block's: a third more coordinate data. Deleting or merging a block drops its levels (foreign key, cascade).
- The **octree** of a stored cloud is these levels: per 256 m square of whole chunks (the 3D view's grid tile), a chain of four nodes (level 3, 2, 1, every point), each replacing the coarser one. Simpler than re-sorting the points into a spatial octree on import, it needs no change to the blocks every check reads, and the hierarchy is one query over the block index. Squares are coarse-grained: a view over a very large *area* (rather than a corridor) may need more nodes than the budget allows at the coarsest level; the nearest are then drawn and the farthest left out.
- **Migration**: version 1 files are read unchanged — a reader asking for a level picks the same points from the whole block (the same hash) — and `ols pointcloud octree` (`Store::build_lod`) builds the levels, setting the schema to 2. Imports build them for new blocks. A version 1 program refuses version 2 files (it would merge blocks without updating their levels).
- **COPC** is read directly as a point source (the hierarchy and LAZ chunks per node, on the `laz` crate; `copc-rs` was not used: it depends on older `las`/`laz` versions), imported like LAZ, and written on export (in memory).

Consequences: projects grow by about a third of their coordinate data; opening a large cloud and drawing it far away read a small part of it; the 3D view's frame cost is bounded by the point budget, not the size of the cloud.
