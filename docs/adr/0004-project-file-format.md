# ADR-0004: Project file format

- **Status:** Accepted
- **Date:** 2026-10-01

## Context
Projects can be very large (hundreds of km, many revisions), must open fast with partial loading (ADR-0006), store the command log and history (DESIGN_GOALS §4, §7), survive crashes, and remain open and readable.

## Options considered
1. Single JSON/XML file — readable and diffable, but slow to save/load when large, no partial loading, no transactions.
2. Directory of text files — git-friendly, but fragile to move/share and slow with many objects.
3. Zip container of JSON — single file, but whole-file rewrite on save.
4. **SQLite database** — transactional, incremental, spatially indexed, single file, universally readable.

## Decision
- **`.ohl` = SQLite database** with a documented, versioned schema.
  - Objects as rows with stable **UUIDv7** IDs (needed for collaboration and diffs).
  - Spatial index on chunks for partial loading/streaming (a tile index, see [ADR-0008](0008-spatial-chunk-index.md)).
  - Command log, revisions, branches and tags stored in the same file.
  - Values always in SI units.
- **Point clouds** stay in external **COPC** (LAZ 1.4) files, referenced by the project. *Amended by [ADR-0007](0007-point-clouds-in-the-project.md): point clouds are imported into the project file.*
- **Libraries** (cables, insulators, structures, weather cases) as **text files** (TOML/JSON) — shareable and git-friendly.
- **`.ohl.json` export/import** — full human-readable dump for exchange, archiving and plain-text diffs.
- **Schema versioning** with automatic forward migrations on open.

Version control in the file: [ADR-0010](0010-version-control.md).

## Consequences
- Project files are binary (not line-diffable in git); version tracking is provided by OpenOHL itself (DESIGN_GOALS §7) and by the JSON export.

## Amendment (2026-11-18): the desktop's format; models of a server in PostgreSQL

[ADR-0013](0013-country-server.md), in active development (not approved
yet):

- **The `.ohl` stays the single-user desktop's format**, session schema 6,
  unchanged (owner, 2026-11-18). The session reaches it through a store
  interface (`ohl_session::store`, SQLite implementation); moving the SQL
  behind it changes no byte of what a file holds (tested by comparing
  `sqlite3 .dump` of files written before and after).
- **A country server keeps every model and model element in PostgreSQL**
  (the PostgreSQL store, a crate of its own), not in project files.
- **Each model on a server can be exported on its own as an `.ohl`** in
  this format, openable by the desktop alone; its `ols_meta` then names
  the network, the revision and the library pins it was exported with.
- The "stable UUIDv7 ids" above were never built: objects are keyed by the
  names users see (`T12`), unique per project; in a network a clash is
  renamed at import with the line's short code.
