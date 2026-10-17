# Project files and point clouds (`.ols`)

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

**`ols line` and `ols zones` read the project's libraries.** Given a
project file, they take the libraries the project keeps with its current
line, as the desktop application does: the wire library, structure types
(with their poles and structure models), insulator assemblies and section
library — written to a temporary folder for the run and removed at its
end. A library given on the command line (`--library`, `--structures`,
`--insulators`, `--sections`) wins; a kind the project holds none of is
looked up as for a line file (the shipped wire and insulator libraries,
the `Structures` and `Sections` folders next to the file). `--weather`,
`--load-cases` and `--rules` also take the name of one of the project's
own files (`--weather cases.olswc`), and `ols zones` falls back on the
project's first weather case file. The summary says where each library
came from (`cables: the project's (2 files)`). The commands write nothing
to the project; opening a project file of an older build upgrades it (as
the desktop application does, e.g. the library files of session schema 3
become objects of each project line), and a project file that does not
exist is not created.

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
  Admins can limit a user's or a role's edits to some kinds, objects or a
  plan area: see [Versions, branches and reviews](versions.md#who-may-edit-what).
- **Revisions, branches, merges and reviews** of the project: see
  [Versions, branches and reviews](versions.md).

## Point clouds

Every point of every tile is imported, except points flagged *withheld*
(deleted by whoever made the survey). A tile more than 2 km from every
structure of the line is imported too — it may hold a substation at the
line's end or another structure worth having — but the import warns about
it, in case it belongs to another project.

```bash
ols pointcloud import TILES_DIR --into project.ols --name lidar-2025 \
  --classes classes.olsc --crs EPSG:2056
ols pointcloud info project.ols
ols pointcloud export project.ols --out survey.laz          # every point
ols pointcloud export project.ols --out wires.las --classes 14,15 \
  --area 2600000,1200000,2601000,1201000                     # classes, a rectangle
ols pointcloud export project.ols --out survey.copc.laz     # COPC
ols pointcloud info survey.copc.laz                         # its octree
ols pointcloud import survey.copc.laz --into other.ols      # COPC in
ols pointcloud octree old-project.ols                       # levels of detail
ols line project.ols --pointcloud project.ols --rules rules.olsr
ols line project.ols --pointcloud survey.copc.laz --rules rules.olsr
```

The desktop app does the same from **File › Import › LiDAR points
(LAS/LAZ)…** (files or a folder, into the project's point cloud or a new one,
in the background; the project is opened again to show them) and **File › Export ›
LiDAR points (LAS/LAZ)…** (every class or the classes shown, the whole survey or around the
line).

**Export** writes LAS 1.4, point format 6 (classes 0–255: schemes with codes
above 31 survive), LAZ when the file ends in `.laz`, with the survey's
coordinate scale, intensities and return numbers as imported, and the class
scheme next to it (`.olsc`) — to share a survey or compare it with other
data. Exported and imported again, a survey gives the same points, classes
and intensities. On the real line, the 1.09 million points within 150 m of a
structure are written in 0.3 s (6.9 MB of LAZ).

LAS/LAZ tiles are only the way in: points are stored in the project in
blocks of one 64 m chunk (a quarter of a grid tile, below; `--chunk M` on import) × one class (coordinates in mm relative to the
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

### Levels of detail

Every block also stores the coordinates of its points of **levels 1, 2 and
3** — about a 4th, a 16th and a 64th of them, picked by a hash of the
point's place in the block, so they are spread evenly and the same every
time; each level holds the coarser ones. They cost a third more coordinate
data (intensities and returns are stored once). A view far away reads a
level instead of the whole block — the 3D view streams by them under a
point budget (see the desktop app's [Level of detail](desktop-app.md#level-of-detail)),
and opening a cloud of more than 20 million points takes the points shown
from them (the ground surface still from every ground point in the corridor).

Imports build the levels. Projects from before (point-cloud schema 1) are
read as they are — the levels are then picked from the whole blocks while
reading, the same points, only slower — and `ols pointcloud octree FILE`
builds them once (the schema becomes 2; older versions of OLS CAD then refuse
the file's point clouds). `ols pointcloud info` says whether they are there.

### COPC

A **COPC** file (Cloud Optimized Point Cloud, [copc.io](https://copc.io)) is a
LAS 1.4 LAZ file whose points are grouped by the nodes of an octree, each
node a thinned sample of its cube that its children refine. OLS CAD:

- reads it **directly** as a point source — `--pointcloud survey.copc.laz` for
  `ols line` and the desktop app: a query reads only the nodes of its area
  (the corridor, a rectangle), down to a depth when it asks for less detail;
  the class scheme is the `.olsc` file next to it (`survey.copc.olsc` or
  `survey.olsc`), else ASPRS;
- **imports** it like any LAZ file (`ols pointcloud import`);
- **writes** it (`ols pointcloud export … --out FILE.copc.laz`): point format
  6, nodes split while they hold more than 100 000 points (each keeping one
  point per cell of a 128³ grid over its cube), one hierarchy page. The
  export holds the points in memory (32 bytes each: 10⁸ points need 3.2 GB).
  Files written are read by other COPC readers (checked with copc-rs), and
  files written by other tools (copc-rs, hierarchies in several pages) are
  read by OLS CAD.
- `ols pointcloud info FILE.copc.laz` lists the octree: nodes, points and
  spacing per level.

Points flagged withheld are left out, as for LAS/LAZ. Point formats 6–8 are
read (what COPC allows); colours, GPS times and extra bytes are not kept.

## Spatial index and tiles

The plan of a project is divided into a fixed grid of square tiles (256 m
at level 0; level 1 tiles are 512 m, level −1 128 m, and so on; ADR-0008).
Every structure, section, jumper and route is indexed in the project file by
its plan bounds and its *home tile* — the finest level whose tiles are as
wide as the object — and the index follows every command. This is what
partial loading, the viewer and the server stream by.

```bash
ols project tiles project.ols --level 2        # objects and points per 1 km tile
ols project query project.ols --area 2585600,1114880,2585856,1115136
ols project query project.ols --tile 0/10100/4355
```

`tiles` lists, for each tile of the level, how many objects touch it and
how many points each stored point cloud has there. `query` lists the objects
whose bounds touch an area or tile, with their home tile and bounds, read
from the index (no model loading needed for the search). Bounds are those of
the model (attachment points); wires swinging out under wind can reach a few
metres further.

On the real 25 km line (633 objects, 99.6 million points): an area query
takes 0.3 ms; listing the 53 level-2 tiles with object and point counts 80 ms.
