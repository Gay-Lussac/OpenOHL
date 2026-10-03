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
| `delete` | `kind`, `id`, `confirm` | Delete an object; when it takes others with it, `confirm` is needed — see below. |
| `move_structure` | `id`, `position` | Move a structure; its attachments move with it. |
| `set_stringing` | `section`, `stringing` | Change a section's stringing. |
| `delete_structure` | `id`, `confirm` | Same as `delete` with kind `structure`. |
| `align_under_structures` | `route` | Replace a route's alignment by points tied under each of its structures. |
| `move_alignment_point` | `route`, `index`, `position` | Move an alignment point (`route: "line"` for the line's own alignment); a tied point is detached. |
| `insert_alignment_point` / `remove_alignment_point` | `route`, `index` (`position`) | Add a free point before `index`, or remove one. |
| `tie_alignment_point` / `detach_alignment_point` | `route`, `index` (`structure`) | Tie a point under a structure (it then follows it), or detach it (it stays in place). |
| `batch` | `commands` | Several commands together. |

### Deleting objects

One rule for every deletion: an object that takes others with it is not deleted until confirmed. Without `confirm` nothing changes and the answer lists everything that would go or change:

```text
$ ols project delete project.ols --user ana structure T7
Deleting structure T7 would also affect:
  attachments and insulators:
    - 1:1 dead-end
  sections:
    - S3
  jumpers:
    - J-T3-branch
  taken out of routes:
    - branch
  alignment points detached (they stay in place):
    - branch #2
Nothing was changed. Repeat with --confirm to apply all of this (it is logged and can be undone).
```

- **structure**: its attachments and insulators, the sections through it (a section cannot lose a point), jumpers at its attachments or at points of those sections; routes lose it (or are deleted when fewer than two structures are left); tied alignment points are detached.
- **section**: the jumpers at its points.
- **jumper**, **route**: nothing else — deleted directly (still logged).

With `--confirm` (`"confirm": true`) everything listed is applied as **one log entry** with a note of what went. Undo (or an admin rollback) restores all of it, in its original order.

### Alignments

The alignment of a route is the base of its plan and profile drawings, kept apart from the line design. Usually its points sit under the pylons, **tied** to them: they follow when a pylon moves. Any point can be moved (which detaches it), detached, re-tied, inserted or removed:

```bash
ols project apply project.ols --user ana '{"op": "align_under_structures", "route": "main"}'
ols project apply project.ols --user ana '{"op": "move_alignment_point", "route": "main", "index": 3, "position": [2600410.0, 1200125.0, 0.0]}'
ols project apply project.ols --user ana '{"op": "tie_alignment_point", "route": "main", "index": 3, "structure": "T4"}'
```

Each applied command is logged with the user, the time, its inverse, the
objects it changed and the **sections to recalculate** (changed sections,
sections through changed structures, sections at changed jumpers' ends) —
the basis for incremental recalculation.

## Incremental solving

The application keeps the line solved while it is edited: every command,
undo or rollback re-solves only the sections it touched (changed sections
and sections through changed structures); jumpers are refreshed each time.
The result is identical to a full solve (checked by tests after every kind
of edit). From the command line:

```bash
ols project apply project.ols --user ana --solve --temperature 40 \
  '{"op": "move_structure", "id": "T40", "position": [2598496.0, 1120627.6, 649.5]}'
```

On a real 554-section line, moving a pylon carrying 28 sections re-solved
those 28 in 4.6 ms (command, log and solve), against 11.7 ms for all 554.

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
