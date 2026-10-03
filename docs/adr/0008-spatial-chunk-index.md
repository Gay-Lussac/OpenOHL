# ADR-0008: Spatial chunk index

- **Status:** Accepted (refines the chunk grid of ADR-0006 and the spatial index of ADR-0004)
- **Date:** 2026-10-03

## Context
Rendering (ADR-0006), partial loading of the project file (ADR-0004) and the server's interest management (ADR-0005) all need to answer the same question fast: *what is in this part of the plan?* — for structures, sections, jumpers, routes and point-cloud blocks. They should share one grid so that a tile means the same thing to the renderer, the file and the server. Lines are long and thin: a section can be several kilometres long, crossing many tile borders.

## Decision
- **One fixed quadtree grid per project** in its coordinate system (`ols_core::grid`): level-0 tiles 256 m wide (stored in the project, `grid_size`), level `L` tiles `256 · 2^L` m wide, all sharing the origin so that they nest exactly. Tiles are named `level/x/y`.
- **Loose placement:** every object has one *home tile* — the finest level (≥ 0) whose tiles are at least as wide as the object, containing the centre of its plan bounds. Its bounds then lie inside the home tile grown by half a tile, so an area query reads a known range of tiles on each level in use. Long objects sit at a coarse level instead of being split or pushed to the root by a tile border.
- **In the project file:** table `spatial_index(kind, id, level, tx, ty, xmin, ymin, xmax, ymax)` with an index on `(level, tx, ty)`, written in the same transaction as every command (moving a structure re-indexes its sections, jumpers and routes; only changed rows are written). Files without the table get it when opened. A plain B-tree on tile keys is used instead of SQLite's R*Tree module: the tile key is what streaming and the server work with, and queries stay simple range scans.
- **Point clouds:** blocks are selected by chunk range in SQL (no scan of the block list); the default chunk is 64 m, a quarter of a level-0 tile, so blocks nest in grid tiles and point counts per tile are exact. Clouds imported with other chunk sizes still work; per-tile counts then go by chunk centre.

## Consequences
- The renderer and the server stream by tile from the start; level-of-detail data (simplified terrain, point subsets, structure LODs) will be stored per tile of the same grid.
- Bounds are those of the model (attachment points), not of a solved weather case: callers looking for swung wires grow the query area.
- Changing the grid size of an existing project needs a re-index (cheap: computed from the model).
