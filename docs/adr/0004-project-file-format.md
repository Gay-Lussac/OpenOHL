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
- **`.ols` = SQLite database** with a documented, versioned schema.
  - Objects as rows with stable **UUIDv7** IDs (needed for collaboration and diffs).
  - R*Tree index on chunks for partial loading/streaming.
  - Command log, revisions, branches and tags stored in the same file.
  - Values always in SI units.
- **Point clouds** stay in external **COPC** (LAZ 1.4) files, referenced by the project.
- **Libraries** (cables, insulators, structures, weather cases) as **text files** (TOML/JSON) — shareable and git-friendly.
- **`.ols.json` export/import** — full human-readable dump for exchange, archiving and plain-text diffs.
- **Schema versioning** with automatic forward migrations on open.

## Consequences
- Project files are binary (not line-diffable in git); version tracking is provided by OLS CAD itself (DESIGN_GOALS §7) and by the JSON export.
