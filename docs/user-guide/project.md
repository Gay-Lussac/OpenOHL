# Project files (`.ols`)

A project file is one SQLite database (ADR-0004) holding the line model, its
**operation log**, its users, and its **point clouds** (ADR-0007). Line files
(`.ols.json`) stay the open exchange format: import them into a project,
export the project back.

```bash
ols project import line.ols.json --into project.ols --user ana   # ana becomes admin
ols project user project.ols --by ana ben designer
ols project apply project.ols --user ben '{"op": "move_structure", "id": "T4", "position": [940.0, 52.1, 510.0]}'
ols project undo project.ols --user ben
ols project log project.ols
ols project export project.ols --out line.ols.json
ols line project.ols --temperature 80          # every command taking a line file takes a project too
```

## Commands

Every change is a command (JSON with an `op`), validated before it is
applied: the whole line must stay valid (references, geometry rules), or the
command is refused and nothing changes. A `batch` applies all its commands or
none.

| `op` | Fields | Effect |
|---|---|---|
| `put` | `kind`, `id`, `value` | Create or replace an object: `meta` (id `line`: `{name, crs, alignment, display}`), `route`, `structure` (with its attachments), `section` (with stringing and loads), `jumper` — `value` as in the line file. |
| `delete` | `kind`, `id` | Delete an object. |
| `move_structure` | `id`, `position` | Move a structure; its attachments move with it. |
| `set_stringing` | `section`, `stringing` | Change a section's stringing. |
| `delete_structure` | `id`, `confirm` | Delete a structure with everything that depends on it — see below. |
| `batch` | `commands` | Several commands together. |

### Deleting a structure

A structure cannot disappear alone: its attachments and insulators go with it, a section cannot lose one of its points, jumpers hang on its attachments, routes list it. `delete_structure` without `confirm` changes nothing and answers with the full list:

```text
$ ols project delete-structure project.ols --user ana T3
Deleting structure T3 would also delete:
  attachments and insulators: 1:1 suspension, 1:2 suspension, 2:1 branch dead-end
  sections through it: S1, S2, S3
  jumpers at it: J-T3-branch
  and take it out of routes: main, branch
Nothing was changed. Repeat with --confirm to delete all of this (it is logged and can be undone).
```

With `--confirm` (`"confirm": true`) everything listed is deleted as **one log entry** with a note of what went; routes left with fewer than two structures are deleted, others lose the structure. Undo (or an admin rollback) restores all of it, in its original order.

Each applied command is logged with the user, the time, its inverse, the
objects it changed and the **sections to recalculate** (changed sections,
sections through changed structures, sections at changed jumpers' ends) —
the basis for incremental recalculation.

## Undo, redo, rollback, users

- **Undo is per user**: `undo` reverses the user's own last command. It is
  refused if another user has since changed one of the same objects (judged
  by content: if they changed it back, undo is allowed) — nobody's work is
  overwritten silently. `redo` reverses the user's last undo.
- **Admin rollback**: `rollback SEQ` reverses any log entry (same conflict
  rule); `rollback SEQ --after` brings the whole model back to its state just
  after entry `SEQ`, as one logged entry.
- **Roles**: `viewer` (read only), `designer` (edit, own undo), `admin`
  (everything, rollback, users). The first user of a project is its admin.
  Finer permissions come with server mode (ADR-0005).

## Point clouds

```bash
ols pointcloud import TILES_DIR --into project.ols --name lidar-2025 \
  --classes classes.olsc --crs EPSG:2056
ols pointcloud info project.ols
ols line project.ols --pointcloud project.ols --rules rules.olsr
```

LAS/LAZ tiles are only the way in: points are stored in the project in
blocks of one 50 m chunk × one class (coordinates in mm relative to the
chunk, delta-encoded, byte-shuffled, zstd), with the class scheme. A check
then reads only the blocks of the classes it needs inside its corridor,
decompressed in parallel.

On a real 25 km line (99.6 million points, private data checked locally):
import 59 s; project file 647 MB against 938 MB of LAZ tiles; a full
clearance run (ground TIN from 11.3 million ground points, vegetation,
buildings, other lines' wires, phase-to-phase) 25 s from the project against
2 min 44 s from the tiles, with identical results.

`--pointcloud FILE#name` chooses a cloud when the project has several. Other
point sources (a point database, a web service) will be plugins implementing
the same point-source interface.
