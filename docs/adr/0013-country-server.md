# ADR-0013: Country server — a store interface, PostgreSQL, working sets, rights and country libraries

- **Status:** Proposed (in active development since 2026-11-18; the owner
  answered the plan's questions on 2026-11-16 and started the work on
  2026-11-18; phases 0–9 and 11 built — this record is not approved yet)
- **Date:** 2026-11-18
- **Amends:** [ADR-0004](0004-project-file-format.md) (the project file
  stays the desktop's format; the server stores models in PostgreSQL),
  [ADR-0005](0005-collaboration-session-model.md) (authority over a store,
  interest management by working set and tile, roles in detail),
  [ADR-0008](0008-spatial-chunk-index.md) (one grid for the whole network,
  geometry beside the tile key), [ADR-0010](0010-version-control.md)
  (branches over working sets, trees split, library releases)

## Context

A utility wants every line of a country in one place: in the owner's words
a network of **12 000 pylons carrying on average 2.5 electrical systems**,
with users who work on a few routes at a time, see only what they may see,
and compute without choosing among the whole network.

Today one project is one `ohl_line::Line` in one SQLite file
([ADR-0004](0004-project-file-format.md)). The session reads every object of
every project line into memory when it opens, and every command copies the
object map, rebuilds the whole line from every object's JSON and validates
it whole. The server ([ADR-0005](0005-collaboration-session-model.md)) owns
that session and hands every joining desktop a full copy of the file. About
160 SQL statements are spread over the session's modules; the point-cloud,
background, import and app crates open the file themselves.

Scaled from a real line of about 80 structures (*estimates*, to be measured
on the synthetic country): the line model of the country is ≈150 MB of
JSON (≈0.3–0.6 GB as objects in memory); what hangs around it is far
larger — LiDAR ≈110–120 GB, terrain ≈8 GB, structure types' finite-element
models 0.5–13 GB, solved results. The line model fits in a server's memory;
the rest must stream.

## Options considered

1. **One huge project file on a file share**: no change of code, but every
   user loads everything, and SQLite is not a multi-user server.
2. **A database behind an ORM**, the session's SQL generated: the queries
   are few and specific (range scans, upserts, savepoints); an ORM hides
   them and adds a layer both stores must follow.
3. **A store interface in the session's own words, with two
   implementations — SQLite (today's code moved behind it) and PostgreSQL**
   (chosen; owner, 2026-11-16: "PostgreSQL is good").
4. **Several databases, one per region**: splits the shared towers and
   corridors that cross regions; the engineering order of one writer is
   lost.

## Decision

### Two modes, one session

- **The single-user desktop keeps today's format** (owner, 2026-11-18): the
  SQLite `.ohl` project file, session schema 6, unchanged. The same session
  code runs over the SQLite store with the whole project in scope. Moving
  the SQL behind the interface changes no byte of what a project file holds
  (tested: `sqlite3 .dump` of files written before and after compared).
- **The server keeps every model and every model element in PostgreSQL**
  (with PostGIS for regions, rights polygons and reach queries): every line
  (design unit), its routes, structures, sections, jumpers, spacers,
  libraries, terrain, point clouds, versions, users, roles and the audit.
- **Each model can be exported on its own as an `.ohl` file** (§ Export
  below), openable by the desktop alone.

### Words

- **Network**: everything a server holds.
- **Line**: a design unit, as a project is today — its settings (criteria,
  calculation options, electrical systems, defaults, structure groups), its
  routes and their wires. A tower may carry wires of several lines.
- **Route**: as today (`ohl_line::Route`), a path made of alignments; what a
  user picks to work on.
- **Branch**: what the desktop calls a *project line* (ADR-0010 amendment of
  2026-10-06). On the server it is called a branch so that *line* means the
  design unit; the desktop keeps "project lines" until both meet.
- **Working set**: what a user picked — full, editable, solved, in the
  computation tabs.
- **Context**: everything else the user may see, streamed by tile around
  the camera, read-only and light.

### The store interface

`ohl_session::store` defines a `Store` interface in the session's words; the
session no longer writes SQL itself. Its groups:

| Group | What it offers |
|---|---|
| Meta and settings | key–value rows (`ols_meta` / `net_meta`), the schema number |
| Users | users and their project role (desktop: `session_user`) |
| Objects | rows of a branch by key, by kind, by tile range; put and delete rows (`ord`, `version`, `gone`) |
| Log | append an entry (returns its `seq`), read entries by seq range, user, branch, objects; mark undone |
| Spatial | write the index's differences, query an area (tile ranges, then bounds) |
| Versions | blobs by hash, revisions, refs, trees |
| Collaboration | grants, reviews and their notes (later: roles, assignments, groups — § Rights) |
| Libraries | (later) files, entries, releases, locks, uses, requests — § Country libraries |
| Terrain, point clouds, backgrounds | blocks and tiles by key range; the point-cloud and background stores take their connection from the store |
| Units of work | begin, commit, roll back a step; steps nest (savepoints) |

Two implementations: **SQLite** (`store/sqlite.rs`, phase 2: the file format
unchanged, every existing test unchanged) and **PostgreSQL** (a crate of its
own, `ohl-store-pg`, phase 4: the synchronous `postgres` client — the
authority is one thread — and a connection pool for the readers). The same
store tests run on both. Nothing PostgreSQL-specific enters the session
before phase 4.

Built 2026-11-19 (phase 4, in active development): **one model is one
store** — the session opens a model of the network as it opens a project
file (`Session::open_on`), every table it uses keyed by the model's code
(`line_id`); the store tests (`ohl_session::store::suite`) and one session
run on both stores give the same results. The session holds its store as
`Box<dyn Store>`; the interface gained what a second store needs (making
its tables, `prepare`; reading and writing rows as they are, for whole
copies — `store::copy`); the project file's statements and files are
unchanged (`sqlite3 .dump` compared). Decided beside the draft: objects
keep their **JSON as text** (as the session wrote it: an export is the
rows imported; `jsonb` would reorder keys and rewrite numbers); the log's
`seq` is numbered across the network but unique per model (a copied model
keeps its numbers); blobs are kept once with each model's use noted; the
draft's `object.routes` and `oplog.lines`/`tiles` come with phase 5, when
the session can keep them current. The tables and their columns: the
[data model](../development/data-model.md#server-database-postgresql).

### One writer, transactions, order

- **One authority applies every command, in one order**, as today: each
  command is one PostgreSQL transaction (savepoints for the steps of a unit
  of work), committed before it is broadcast; `seq` comes from it and has
  no gaps. Validation, per-user undo and the replicas' replay rely on this
  order ([ADR-0005](0005-collaboration-session-model.md)).
- Readers (the tile packager, the compute workers) read at a snapshot
  (`REPEATABLE READ`) and label what they make with the `seq` they read at.
- Only one authority per database: it takes a PostgreSQL advisory lock at
  start; a second server process refuses to start as writer.

Built 2026-11-19 (phase 5, in active development): `ohl network serve`
(`ohl_server::network`, feature `network`) opens a session on every model
of the database as its one writer; each command is one transaction — the
session's rows, then each changed element's routes and references, the
models and tiles its log entries touched, and the change itself in the
`stream` table at its **position** (`seq`, `step`: changes making no log
entry count steps after the last entry) — committed before it is
broadcast; a command that cannot be kept after the session applied it is
rolled back and its model read again. A joining desktop's copy of its
model is written by a reader connection at a snapshot taken between two
commands (the authority goes on meanwhile).

### Server memory

The line model of the network (with its spatial and reference indexes) is
kept **whole in the server's memory** and changed in place by each command
(phase 3); what is heavy streams from the database and is kept in bounded
caches: structure types' models (per type, LRU), resolved structures,
solved section shapes (per section revision and case), check results, tile
packages (per tile, tile revision and rights class). LiDAR blocks, terrain
tiles and backgrounds are never held whole. Targets (to measure, phase 1
onwards): server start < 60 s, idle memory < 2 GB for the 12 000-pylon
country, a command applied and broadcast < 50 ms at p95.

### Local validation

A command is validated on what it touches and on what refers to it (a
**reference index**: who refers to structure S, by kind and set, kept with
every write), not on the whole line (phase 3). Property tests check that
local validation answers as the whole validation does, and that the line
changed in place equals the line rebuilt from every object. Built
2026-11-18 (in active development): `ohl_line::index`,
`Line::validate_local` (every check and its reach in the
[decision log](../development/decisions.md#2026-11-18-country-server-phase-3)),
`ohl_session::in_place`; what goes the whole way is listed there too.

### Working set and context

- **Picking** (owner, 2026-11-16): whole routes (a list, or clicked on the
  map); a part of a route by station range or from structure A to
  structure B; or an area drawn on the map (a polygon, or a corridor along
  the alignment). A part **always snaps out to whole tension sections**
  (strain structure to strain structure), because a section is solved
  whole; the snapped extent is shown before it is confirmed, and what lies
  beyond is context. Working sets can be saved under a name. The same
  picking gives the scope of a rights assignment (§ Rights).
- The working set is an `ohl_line::Line` per design line, as today: every
  table, check and drawing keeps working on a whole `Line`. Structures
  shared with routes outside it come in whole; the other routes' wires on
  them come as context.
- **Context** tiles (ADR-0006 render distances, an LRU with a memory bound):
  per structure its id, position, orientation, type, height, a stick
  drawing; per section its wires solved in one *context case* (a network
  setting); routes as polylines. Never in the tables or checks except as
  pulled-in neighbours.
- The desktop on a server keeps a **partial replica**: the session over a
  local SQLite store holding the working set and the context it has seen,
  fed by the operations of its interest. The desktop alone: the whole
  project is the working set, no context — no change.
- The operation stream is filtered by interest (each operation names its
  lines and tiles; a heartbeat carries the current `seq`), protocol
  version 3; alpha clients are refused with a message.

Built 2026-11-19 (phase 5, in active development): a client follows
whole models (a desktop's replica of one), routes and tiles; every change
is encoded once and broadcast, and each connection passes on only what
touches its client's interest; what is for one client alone (a tile
package, a catch-up, a change of its interest) goes on that client's own
queue marked with its place among the broadcasts, so a tile taken at its
revision is followed by exactly the changes after it. A heartbeat carries
the stream's position and the last change sent to that client (a gap
shows); a client catches up from its position (`stream`, in the
database, so also after a restart); a change touching more tiles than a
setting reaches the clients that only view them as "read again". Tile
packages (level 2, 1 024 m, by default) are made by packager threads from
a copy of each model's line taken at the model's revision: per structure
its id, position, orientation, type, height and a stick; per section its
wires in the context case (each section solved alone, on the part of its
line it reads — the same shape as on the whole line, bit for bit, tested:
a drawing); the alignments' segments in the tile; the routes' names. Kept, each bounded
(settings): packages per (tile, revision, rights class — 0 until phase
7), solved section shapes (keyed like the solve memo's, by everything
the solve reads), resolved structures (per model and library revision).
The web client reads tile packages on a network's server and stays a
viewer. The desktop application joins file servers as before (protocol
3, its messages unchanged); joining a network with a working set is
phase 6. Measured on the 12 000-pylon synthetic country:
[performance](../development/performance.md#synthetic-country-phase-5-the-server-on-the-database-2026-11-19).

Built 2026-11-19 (phase 6, in active development; the
[decision log](../development/decisions.md#2026-11-19-country-server-phase-6)):
OHL-Design joins a network with a working set (File › Join a network):
picked as whole routes (a list or the map), a station range, two
structures or an area drawn on the map, shown as the server snaps it
(every section with a span in a pick whole, with its structures), saved
by name (only the picks, schema 3). Its replica is a project file of
today's format holding the working set and, read-only, what solving each
of its sections alone reads (the other sections and jumpers on its
structures, its spacers' sections, the alignments within 500 m), what it
refers to and its routes' alignments — its sections solve as on the whole
line (tested, bit for bit); it is written from the server's rows (their
JSON and places) and fed by **rows**, not operations (an operation may
touch objects a part does not hold): after each change that may touch it
the authority works the extent out again and sends what changed, came
and went; a replica joining again sends a hash of each object it holds
and gets what differs. A working set is on one model. Context tiles
around the camera (5 × 5 at level 2, a cache of 64 MB) are drawn grey,
a drawing. The packager reads each tile's part of the model instead of a
copy of the whole line, and the context case is solved ahead while the
server is idle. Measured:
[performance](../development/performance.md#synthetic-country-phase-6-working-sets-and-context-2026-11-19).

### Computations on the working set and what touches it

A check runs on the working set's routes the user ticks in its Setup, and
**pulls in, for each part, what can change its answer, by that check's own
distance** (owner, 2026-11-16): the wires on a checked structure (other
lines' too, each solved with its own line's settings), wires within a
clearance rule's reach, terrain and LiDAR in the corridor, other lines
within the field distance (500 m by default, a setting, said in the
result). Every result lists what was pulled in and why. **Safe side**: a
neighbour that could not be pulled in makes the check *not all checked*,
never a pass; a check with no known reach (a plugin that declares none)
runs on the working set only and says so. A neighbour the user may not see
is computed on the server and reported on the user's own span without its
details. A structure carrying wires of several lines is checked under each
line's load cases, the worst kept. (Engine changes in phase 10; the plugin
interface step is asked first.)

Built 2026-11-20 (phase 10, in active development; the owner: "You can go
ahead with recommendations and the changes to pull nearby lines"; the
[decision log](../development/decisions.md#2026-11-20-country-server-phase-10),
the [theory](../theory/scoped-checks.md)): `ohl_scope` — each check's
reach, the candidates by a bound of each span's wire before it is solved,
clearances between wires with this line's and other lines' wires as
targets, the field with the lines within 500 m (NOT ON THE SAFE SIDE for
what lies beyond, said), the loads on shared towers (twins) under each
line's cases, the worst kept; computed by the network's server on what it
holds (each part as `Session::drawing_part` takes it), the report filtered
by the rights (a hidden neighbour counted, never named nor placed), each
result watched and marked stale once (protocol 3, messages `check`,
`check_result`, `check_stale`; no schema change). Scoped results equal the
whole network's (tested on the synthetic country, to the bit for the
clearances and the loads). The plugin step: an optional manifest key
`[reach]` (interface 0.3 unchanged).

### Towers shared by several routes

A user may edit a tower shared with routes outside the working set (owner,
2026-11-16); a change that breaks another line's connection is **refused**,
naming the other route and objects: the tower deleted or renamed; a wire
point another line's section or jumper ends on removed or omitted; the
string it hangs on removed; a suspension point changed to strain or back
where another section passes or ends; a clamp or assembly removed; the
other route taken off the tower; anything that makes the other line fail
its local validation. Changes that only move the other line's wires are
allowed; its results go stale and its owners are told (a review instead on
a protected or built line). Parts used only by the other line need rights
on it. The same rule makes a merge that would break a connection a
conflict. Built 2026-11-18 for the session (phase 8, in active
development): `ohl_session::shared` — the desktop's working set (the
routes a user edits, View › Editable routes) and a part's recorded users
stand for rights until phase 7; stale marks with notices in the project
file; a protected project line stands for a built line, its review the
existing one; a project without working sets unchanged (the
[decision log](../development/decisions.md#2026-11-18-country-server-phase-8)).
On a network's server (since phases 7, 11 and 12): the rights decide every
object a command changes; the rules of this section apply to the
commands on a branch over a working set (its routes the working set), to
its merge where it lands (stale marks, the other routes' connected
owners told, a protected line waiting for a review) and, since 2026-11-20
(phase 12's first item), to commands made directly on a network's main
line — checked against the routes that are their user's (those the rights
let them edit something on, narrowed to the working set their connection
follows; a user who may edit every route and follows no working set:
no rule, as a project without working sets), through the same code path
([decision log](../development/decisions.md#2026-11-20-country-server-phase-12)).

### Rights and roles on (almost) every element and library

Owner, 2026-11-18: "the rights and roles should be applied to almost every
element and library".

- **Permissions**: `see`, `edit`, `create`, `delete`, `review` (approve or
  reject), `export`, `lock`, `request` (library additions), `manage_rights`.
- **Roles** are named sets of permissions that an admin defines. Shipped
  defaults: *viewer* (see), *designer* (see, edit, create, delete, export,
  request), *checker* (see, review, export), *library maintainer* (see,
  edit, create, delete, review on library scopes; lock), *line admin*
  (everything on its lines, including manage_rights for them), *network
  admin* (everything everywhere).
- **Assignments** are (user or group, role, scope). A **scope** is one of:
  the network; a region (a polygon); a line; a route or part of a route
  (whole tension sections, picked as a working set); an object kind; a
  single object; a library kind, a library file or a single library entry.
  Every model element kind (meta/settings, routes, alignments, alignment
  points, structures, sections, jumpers, spacers, annotations, reports,
  point clouds, surfaces, backgrounds) and every library kind is a scope;
  a scope can combine a kind with a line, route or region ("the costs of
  line B").
- **The exceptions** — not scopes of their own: derived data (tiles and
  tile packages, solved shapes, caches, spatial index rows, reference and
  use index rows, stale marks) follows the rights of the objects it comes
  from; a log entry or a revision is visible as far as its objects are
  (an entry touching a hidden object shows "an object you have no access
  to"); the schema and meta rows of the database itself, and the audit,
  belong to network admins only.
- **Evaluation**: nothing without a grant (default deny). A user's
  permissions on an object are the union of the permissions of their
  assignments (their own and their groups') whose scope covers it.
  *edit*, *create*, *delete*, *review* and *export* need *see*. An explicit
  **deny** assignment, set by a network admin (rare: a contractor's group
  barred from one line's costs), overrides every grant on its scope. One
  function in the session resolves it; every command (edit side) and the
  server's outgoing filter (see side: tiles, operations, search, properties,
  check results, diffs, impacts, merge plans, reviews, error messages,
  exports) use it. Rights classes are cached per tile and per working set.
- **Tests**: property tests over random roles and assignments — no command
  succeeds without the right; no outgoing message holds a hidden id or
  coordinate; every command kind and every library kind is listed in a
  test, so that a new one cannot be added without a rights rule.
- **The desktop project file is unchanged**: its users keep today's three
  roles (viewer, designer, admin) and edit grants; they map onto the
  shipped roles when a project is imported into a network. Built in phase 7
  (rights) and phase 9 (libraries); the store's collaboration group keeps
  room for them.

Built 2026-11-20 (phase 7, in active development; the
[decision log](../development/decisions.md#2026-11-20-country-server-phase-7)):
the one function is `ohl_session::rights::UserRights::allows` (a scope a
conjunction of constraints; regions grant what lies inside them and deny
what they touch; the line as a whole needs a scope of the whole line);
the server decides each operation by its rule before it runs and every
object it changed after, as it was and as it is, in the same unit of work
(refused: nothing kept); everything it sends is filtered by the same
function, objects seen as drawn (a section with its structures, an
alignment point with its tied structure), tile packages kept per rights
class (the exact set hidden in the tile). Only the network's users join,
each with their personal invitation. Rights are changed by
`ohl network rights …` (network schema 4) without the writer's lock,
audited, taken by the running server before its next job; a client
whose rights changed is let go. "See as user" is the intersection of
both users' rights. A project's edit grants imported with it are still
checked by the session beside the assignments.

### Country libraries

- Every library kind a project keeps today (wires, structure types,
  insulator assemblies, weather and load cases, clearance rules, galloping,
  ambient, costs, poles, finite-element models, cross-sections and steel
  materials, bolts, field plans, typed structure loads), and the kinds read
  only from folders today when wanted, can be kept **once for the whole
  network**.
- An admin **locks** a library kind or file. A locked library changes only
  through a **library review** (a maintainer proposes, another maintainer
  approves, then it is released); an admin may change it directly in an
  emergency, logged in the audit. Library maintainers are assignments of
  the *library maintainer* role scoped to a library kind (no list of their
  own).
- Users **request** additions to a locked library (owner: "it might not be
  granted"): draft, requested, granted, refused, withdrawn. While
  requested the entry may be used in the requester's own branch as a
  marked pending draft; such a branch cannot be submitted, merged into
  `main` or marked built.
- References stay by name; each **release** of a library is a tag, each
  entry has a version (content hash, first release). **Built lines are
  pinned** to the entry versions they used; **designs in progress follow
  the latest** (owner, 2026-11-16: the recommendation). A pinned line moves
  its pins as one logged, undoable command after an impact run (through a
  review when protected). A withdrawn entry flags every result using it
  *not on the safe side* until the pin moves or an admin acknowledges it.
- A **use index** answers "where is this entry used"; only changes are
  stored (entries as content-addressed blobs, releases as revisions).

Built 2026-11-20 (phase 9, in active development; the
[decision log](../development/decisions.md#2026-11-20-country-server-phase-9)):
`ohl_store_pg::library` (network schema 5) and `ohl_session::country`.
**A line holds the files of the release it is pinned to as its own
library files**, so the engine and the project file are unchanged and an
exported model stands alone (`network.library_pins`,
`network.library_withdrawn`). An entry is a file with a top-level `id`,
or a table of a list named by `id`, `name` or `label`; its version the
SHA-256 of its values written canonically. Locks by kind or file
(unlocking by network admins; an emergency change by a network admin,
audited); maintainers hold *review* and *lock* on the kind; the
network's library a target of the rights' one function of its own
(`Target::CountryLibrary`, of no line). A request's draft is a file
`request-N.ext` used only in its requester's own branch, blocking its
review and merge into main until granted (by a maintainer other than the
requester) or taken out. A release changes no line; *Update library
pins* moves a line's pins as one logged command (kind `library_pins`,
not undone by Undo), the routes using a changed entry marked out of date
as phase 8 marks them; built lines only by whoever reviews the whole
line. Withdrawn entries stay in later releases, never newly used. The use
index (`lib_use`) is kept by every command of the server and read
through the rights. The import maps each entry (the same, added to a kind
not locked, or asked for — never replaced); the model keeps its own files
until Update library pins.

### Export of one model as a project file

Owner, 2026-11-18: "each model could be exported individually in the OHL
format from the server".

- Any one model — a line with its routes, its structures and their types,
  the library entries it uses (pinned versions copied in), optionally its
  terrain and LiDAR in a corridor — is written as a **standalone `.ohl`
  project file in today's format** (schema 6), openable by the desktop
  alone. Its `ols_meta` gains a few keys naming the network, the revision
  it was exported at and the library pins; nothing else differs from a
  project made on the desktop.
- Rights-filtered (only what the exporter may see) and only with the
  *export* permission; every export is in the audit.
- Distinct from an **offline working set** (exported with read-only
  neighbours, brought back as a bundle and merged like a branch; below).
  An exported model can be imported again as a new model.
- Built in phase 4 (PostgreSQL → SQLite writer through the store
  interface), with a round-trip test: a project imported into the network
  and exported back has the same objects (dump compared, ids mapped).
  Built 2026-11-19 (`ohl network export`, in active development; rights
  and the export permission with phase 7): every row of the model read at
  one snapshot; the same object, index, meta, user and grant rows as the
  file imported, on the example line and on the 12 000-pylon synthetic
  country.

### Offline working sets

A user takes their working set away with no connection and brings the
work back (plan of 2026-11-16: no reservations at first). Built
2026-11-20 (phase 12, in active development, not approved yet;
[decision log](../development/decisions.md#2026-11-20-country-server-phase-12)):

- **Out**: the working set (picked as a join picks it) written by the
  server as a project file of today's format — a part of the line, the
  working set editable, its context and the line's own neighbours within
  the checks' reach read-only, other lines' neighbours (what solving their
  sections there reads, types resolved, with their wire and case files)
  in a meta key; everything filtered by the rights' one function (the
  working set seen whole; a neighbour its user may not see left out and
  counted, never written). Its base is the model's main revision (the
  main line's changes recorded first, by phase 11's rule for a branch);
  the file's first revision — the export's — is recorded on the server
  (`offline_export`, network schema 6: its tree's list of hashes, no
  object again) and the export audited.
- **Offline**: the scoped checks of phase 10 run on the file with the
  server's own code and the neighbours it holds — the same report, or NOT
  ALL CHECKED where the file cannot give it (a hidden neighbour left out,
  a check reaching beyond what was exported).
- **Back**: a bundle of only the changes (the existing bundle format and
  verification), checked against the export the network recorded; its
  changes applied on a **branch over the working set made from the
  export's base** — never on main — as one command decided by the user's
  current rights and the rules of shared towers, then committed. Changes
  to read-only neighbours and library files are refused; conflicts with
  what changed on main meanwhile are listed (its merge asks to settle
  them), never settled; a bundle of an unknown base, of another user's
  export, changed by hand or brought back twice is refused.

### Import of existing projects

`ohl network import PROJECT.ohl --line CODE`: one CRS and one grid for the
network (a project in another is refused, to be reprojected first); ids
stay the names users see, a clash renamed with the line's short code
(`A-T12`) with everything that refers to it; a tower modelled in two
projects (found by position and type) is listed for an admin to merge or
keep — never merged silently; the current state (and tags if asked) comes
in as **one** log entry per line, the file kept as an archive; libraries
mapped to the country libraries (same content mapped, conflicts to a
maintainer, never silently replaced); a report lists every decision.
Built 2026-11-19 (`ohl network import`, in active development): as one
transaction with one log entry and a revision "imported from FILE";
the twin towers within 0.5 m in plan and of the same type (an estimate
for the owner to set); the file's history, other project lines, tags,
point clouds and backgrounds left in the file and listed; library mapping
waits for the country libraries (phase 9: a model keeps its own library
files meanwhile).

### Versions at the scale of a country

One protected `main` for the network; designers work on branches over
their working set, kept as differences (schema 6 already). A revision's tree
becomes a tree of trees (by line, kind, tile bucket), so a commit costs what
changed; old flat trees still read. No whole-network snapshots or
whole-branch copies (owner rule: only changes logged).

Built 2026-11-20 (phase 11, in active development; the
[decision log](../development/decisions.md#2026-11-20-country-server-phase-11)),
**amending the split**: a revision's tree on a network's model is a
**trie of its objects keyed by the SHA-256 of each object's kind and
id** (`ohl_session::trees`, tree version 2: at most 64 objects a leaf,
up to 16 children by the next hexadecimal digit, the library files in
the root), not a tree by line, kind and tile bucket — a model is one
line already, and a bucket by tile or route is worked out from the line
(objects move between buckets with edits, a tower stands on several
routes), while a trie by key depends on the objects alone, so every copy
makes the same revision id. A commit hashes and writes what changed
since the line's last tree (the session keeps each object's hash with
its shared JSON), a diff or a merge reads the nodes and objects that
differ; ahead/behind and merge bases stay exact. **The desktop's project
file keeps flat trees** (a store says which it writes; both are read).
Merges are worked out from what differs and validated as a command is
(locally); a merge changing library files goes the whole way.
**Branches over a working set** (`CreateWorkingSetBranch`; the session's
`BranchScope`, a model's meta row): a project line over a set of routes,
its maker's and its team's, only its changes stored, the rules of
towers shared by several routes applied to its commands with its routes
as the working set and to its merge where it lands — a broken
connection a conflict, a move marking the other routes stale (their
connected owners told), a protected line waiting for a review. Rights:
a commit needs edit only on what it records; a branch over a working
set needs its routes; tags, whole-line branches, rollbacks and reviews'
merges still the whole line. The network schema stays 5.

### Security and operations

Sign-in by OIDC/SSO with the owner's identity provider (personal
invitations stay for the desktop server); only the server talks to
PostgreSQL, with its own least-privilege role; the rights filter is the
security boundary and gets a dedicated review before real data goes in.
Shipped deployment: a Docker Compose file (server, PostgreSQL with PostGIS,
backup job), plain binaries as the other option — the owner's IT decides.
Backups by base backup and WAL archiving, with a restore drill; a schema
migration is refused when the last backup is older than a setting.
**None of this section is built yet** (phase 13): sign-in is the personal
invitation, the server connects with the connection string it is given,
there are no deployment files, and backups are PostgreSQL's own tools
until then.

### Measured first

A **synthetic country** of the owner's size (12 000 pylons, 2.5 systems
each, routes, alignments, structures, sections and wires; synthetic LiDAR
on a handful of sample tiles only, its scaling measured per tile and
extrapolated) is generated deterministically (`ohl synth country`, a hash
per seed tested) and every phase is measured on it. No stress setting
beyond the owner's size (owner: "Just reduce the synthetic test"). Real
utility data is never in the repository; it is measured locally only.

### Phases

0 decisions · 1 synthetic country and baseline · 2 store interface (SQLite)
· 3 incremental, locally validated session · 4 PostgreSQL store, import and
per-model export · 5 server on the database · 6 client working set and
context · 7 rights and roles · 8 shared towers rule · 9 country libraries ·
10 scoped computations · 11 versions at scale · 12 offline working set ·
13 security, deployment, backups · 14 scale validation. Phases 2 and 3 come
first: they also make big desktop projects faster.

## Consequences

- The session talks to a store, not to SQLite; new SQL goes into a store
  implementation. Every store test runs on both stores (phase 4 on).
- The project file stays the exchange and desktop format; nothing in it
  changes for the server, except the meta keys of a model exported from
  a network.
- Checks gain a reach and a pulled-in list (phase 10, an engine change);
  plugin checks gain an optional reach (an additive plugin interface step,
  [ADR-0012](0012-plugin-interface-versions.md), asked before it is made).
- Two modes could drift apart: one session code, one test suite on both
  stores.
- The owner's side runs a database server, backups and upgrades.
- Not decided yet (later phases): splitting the network into writers per
  region (not planned); object storage for point clouds (only if backups or
  size demand it).
