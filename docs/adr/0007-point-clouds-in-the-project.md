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
