# Server mode (pre-alpha)

Several people work on one project at the same time, like players on a game server: one computer **hosts** the project, the others **join** it. Every edit, from anyone, is checked by the server, applied in one order, written to the project file and sent to everyone (ADR-0005).

## Hosting

- **In the application:** File › **Host this project…** (a project file `.ohl` open): the port (UDP, default 4433) and what happens to people the project does not know (they join as viewers, as designers, or are refused). The server runs inside the application, and the application joins it like everyone else. File › **Stop hosting** ends it and opens the project file again.
- **Without the application:** `ohl serve PROJECT.ohl` (options `--bind 0.0.0.0:4433`, `--advertise HOST`, `--newcomers viewer|designer|none`, `--invite USER` repeatable, `--code SECRET`, `--keys FILE`, `--web PORT` for the [web client](#web-client), `--seconds N` to stop after a time). It prints the invitations and who joins and leaves, and runs until stopped (Ctrl+C). Every edit is already in the project file.

The project file stays on the hosting computer. The port must be reachable over UDP from the others' computers (firewall, port forwarding over the internet).

## Invitations

An invitation looks like `ohl://192.168.1.20:4433/3f2a9c1b07de/80854a…` (the address, an access code, and the fingerprint of the server's certificate). The connection is encrypted (QUIC, TLS 1.3), and the application only talks to the server whose certificate matches the fingerprint.

| Invitation | Who joins with it |
|---|---|
| The **general** invitation (People › Invitation; `ohl serve` prints it) | Only names the project does not know. They get the newcomers' role. |
| A **personal** invitation (People › Personal invitation for NAME, while hosting; `ohl serve --invite NAME`) | That user only, with their role in the project. |

A user the project knows can only join with their own personal invitation, so nobody joins under someone else's name.

Where the codes come from (since 2026-10-26):

- The **general** access code comes from a secret made when the server starts, or from the one you give (`--code SECRET`, to keep the general invitation across restarts; links made with a code before 2026-10-26 stay valid). Everybody holding the general invitation knows that code, and a short `--code` can be guessed from it: nothing else is made from it.
- The **personal** access codes (128 bits each) come from a random key that has nothing to do with `--code`. It is made when the server starts, so the personal invitations change at every start, unless it is kept in a key file: `--keys FILE` (made with a fresh key when missing, readable by you only). With `--code` and no `--keys`, `ohl serve` keeps it in `PROJECT.ohl.server-key` beside the project and says so. Keep that file private (never in a shared folder, a repository or a mail with the project): whoever has it can make anybody's personal invitation. Personal invitations made before 2026-10-26 no longer work: hand out new ones.

## Joining

File › **Join a server…** (also on the start page): the invitation and your user name. The project is copied to this computer (with its point clouds; a progress bar shows the transfer), then opened like any project. From then on it follows everyone's edits.

## Working together

- **Edits** work as usual: each is sent to the server, which applies it or refuses it (the reason shows in the status bar). Others' edits appear between frames, and the sections they touch are solved again.
- **Undo** is per user: Undo takes back your last edit, never someone else's.
- **Roles:** viewers read; designers edit; admins also change roles (People) and reverse anyone's edits (Versions › **Operation log…**: **Reverse** one entry, or **Back to here**, the line as it was just after an entry; both are undoable).
- **People** (View › People) lists who is connected: their role, their selection, what they are editing, and **Go to** to look where they look. Others are also shown in the plan view, as a diamond with their name.
- **Soft locks:** a structure in someone's Move window is theirs until they close it. Others cannot open it, and the server refuses their edits of it — and their **undo** or **redo** when the edit it takes back touches it. **Admins** pass the locks (their edits, merges and roll-backs go through).
- **Project settings** (the surface of the checks, the drawing settings of a project line): changed by a designer whose edits no edit right limits (on a protected project line, by an admin), sent to everyone, and written in the server's log (who changed which setting). A setting never holds a path of one computer: the server refuses such a value, and the copy of the project a joining user receives keeps only the file name (e.g. of the weather case file used last).
- **Versions** (Versions › Revisions, tags and reviews…): commits, branches, merges, reviews and edit rights go through the server too, and every replica keeps the same history. Every user works on their own project line (Versions › Project lines): edits go to it, and others see them when they work on or show that line. See [Project lines and stages](project-lines.md) and [Versions, branches and reviews](versions.md).
- Libraries and project lines also go through the server. What each person **displays** is their own: the surface used as the ground and the others shown, the LiDAR classes hidden, the share of points, the layers — remembered on their computer, never sent to the server. Importing terrain surfaces and point clouds is done on the hosting computer.

## Web client

People without OpenOHL join in a browser. Host with **Web client** ticked in File › Host this project (TCP port, default 8443), or `ohl serve PROJECT.ohl --web 8443`. The link is shown in People (**Web client**, and **Copy web link** next to a personal invitation) and printed by `ohl serve`:

    https://192.168.1.20:8443/#user=bob&code=57b8cd31f0e1

Opening it joins at once; without the part after `#`, the page asks for the name and the access code. The same codes and rules apply as for the application: a user of the project needs their personal code, others get the newcomers' role.

The web client is a **viewer**: it shows the project's **main line** and does not edit,
whatever the user's role (the server refuses edits and locks from browsers). What the page
shows:

- **3D** (WebGPU when the browser has it, else Canvas2D), **Plan** and **Profile** (along the
  main route; no terrain yet): structures, alignments, and the wires solved on the server at
  **15 °C, no wind** (one weather case for now). The line is drawn again after each change.
- **Moving around:** drag (3D: orbit; Shift or right drag: pan; plan and profile: pan), the
  wheel or **+ / −** to zoom, **W A S D** or the arrow keys to move, **F** to fit the line.
- **G: go to a structure.** Type its name (completed from the list; case does not matter, the
  start of a name is enough when only one fits) and press Enter. The view centres on it and
  selects it.
- **Click** a structure or a wire for its **properties**:
  - a structure: type, extension, position, orientation, height, the sections through it, and
    its attachments (wire point height, string length);
  - a wire span: section, span and its structures, length and chord, height difference, sag,
    largest tension, cable, bundle, circuit, phase, weather case.

  **Escape** clears the selection.
- **Display** (remembered in the browser): structures, wires, alignments, structure names,
  sags, where the others look; and the **detail**:
  - **Low:** large lines or slow computers;
  - **Medium:** the default;
  - **High:** every point.

  A wire span small on the screen is drawn with a few points or as its chord, a structure far
  away as one stroke, and what is off the screen not at all. Names and sags are written
  nearest the middle of the view first, never over each other, up to a number per detail
  level. The panel says what was drawn and how long it took. Without WebGPU, a line of
  2000 structures draws in about 10 ms at Medium; spans are sent with at most 24 points each.
- **People:** who is connected, what they selected, **Look** to look where they look; others
  are diamonds in the views.
- **Operations:** every edit as it arrives (on which project line), with its log entry.

**Security.** The page and the WebSocket are served over **HTTPS with the server's self-signed certificate**, so the browser warns the first time ("not private", "not trusted"). Before accepting it, compare the certificate's SHA-256 fingerprint (the browser shows it in the certificate details) with the one `ohl serve` prints, or with the last part of the `ohl://` invitation: if they differ, do not continue. The access code travels in the link's `#` part, which browsers do not send to the server or other sites, but it is in the browser history: hand links out like invitations. The listener serves only the client's own four files and the WebSocket; messages have the same size limits as the application's, and a browser silent for 30 s is disconnected. On a network you do not trust, prefer the application (its certificate is checked against the invitation automatically).

## A network's server (in active development)

What is built — every row **built, in active development, not approved
yet** ([ADR-0013](../adr/0013-country-server.md),
[decisions](../development/decisions.md)) — and what is not yet:

| Part | State |
|---|---|
| The database (PostgreSQL with PostGIS), import and export of lines (phases 2–4) | built, in active development |
| The server on the database: tiles, changes by interest, catch-up, heartbeats, the web client by tile (phase 5) | built, in active development |
| Working sets and context in OHL-Design (phase 6) | built, in active development |
| Rights and roles on (almost) every element and library, the audit, see as user (phase 7) | built, in active development; managed on the command line |
| Towers shared by several routes (phase 8) | built, in active development: in the session, on branches over a working set and (since 2026-11-20) on edits made directly on a network's main line |
| Country libraries: locks, requests, releases, pins, withdrawn entries, the use index (phase 9) | built, in active development; Update library pins on the command line and the server's protocol |
| Scoped computations: checks pulling in their neighbours — clearances between wires, the magnetic field, shared towers' loads — hidden neighbours counted, results marked stale (phase 10) | built, in active development; through the server's protocol (OHL-Design's check windows run on the working set's file and say what they did not pull in) |
| Version control at scale: split trees, branches over a working set, merges at shared towers (phase 11) | built, in active development; through the server's protocol |
| Offline working sets: exported with read-only neighbours, checked offline, back as a bundle onto a branch (phase 12) | built, in active development; on the command line, the server's protocol and, since 2026-11-21, OHL-Design's [Offline working set window](offline-working-set.md) (Versions › Offline copy) |
| Security review and its fixes, deployment files (Compose), **backups, restores and the restore drill**, migrations only after a backup, OIDC sign-in, invitations reissued and expiring, TLS to a database on another host (phase 13) | built, in active development: on the command line ([below](#security-sign-in-and-backups-phase-13-in-active-development), [deployment](server-deployment.md), [the review](../development/server-security-review.md)); OHL-Design's sign-in window built 2026-11-20 ([Signing in to a network](sign-in.md)); the web client's not yet |
| The admin app OHL-Server | a plan waiting for the owner |
| Point clouds, terrain tiles and backgrounds served by the network | not yet (they stay in project files) |

A country server keeps every line of a network in a PostgreSQL database
([ADR-0013](../adr/0013-country-server.md); setting one up:
[the country server's database](../development/postgresql.md)). Its
server is started on the database instead of a project file:

    ohl network serve --database "host=… dbname=network" --web 8443 --invite alice

(the same options as `ohl serve`, and `--readers`, `--packagers`,
`--packages-mb`, `--shapes-mb`, `--heartbeat-ms`). One server per
database: a second one is refused. It reads every line once (the
12 000-pylon synthetic country's 147 lines: about 3 s) and writes each
edit to the database, in one transaction, before it tells anyone. Every
`ohl network …` command, its rights and an example:
[A network on the command line](network-commands.md).

- **Lines in and out**: `ohl network import FILES…` makes each project
  file a line of the network (ids another line has renamed `CODE-id`,
  twin towers listed for an admin, never merged; library files mapped
  onto the network's; network admins); `ohl network export --line CODE
  FILE` writes one line as a standalone project file of today's format
  (*export* on the whole line). Both are in the audit.

- **Browsers** joining it get no copy of a line: they see the network
  **by tiles** around their view — the structures (a stick each), the
  wires solved at 15 °C without wind (each section alone: a drawing, not
  a result), the alignments and the routes' names — and read a tile again
  when someone's edit touched it. Moving around asks for the tiles
  around the new view (at most 96 at once). The header says "network of
  N lines (tiles)". Still a viewer.
- **OHL-Design** joins a network with a **working set** (File › Join a
  network…, phase 6, 2026-11-19): whole routes, a part of a route or an
  area, snapped out to whole tension sections, kept on your computer as a
  project file and fed by the server; the rest around the camera drawn as
  read-only context — see [Working sets on a network](working-set.md).
  File › Join a server… still joins servers of a project file; a
  network's server refuses it, saying to join with a working set.
- The server solves the context's wires ahead while it is idle (every
  tile once after a start, then the tiles edits touched, one at a time;
  `--no-idle-solve` turns it off).
- Clients of **protocol 2** (OpenOHL before 2026-11-19) are refused by
  every server, with a message to update.
- **Only the network's users join**, each with their **personal
  invitation** (`--invite NAME`): the general invitation admits nobody,
  and `--newcomers` is refused on a network. What each user may see and
  do comes from the network's **rights** (below).

### Rights on a network (phase 7, in active development)

*Built 2026-11-20, in active development, not approved yet
([decisions](../development/decisions.md#2026-11-20-country-server-phase-7)).*

A **role** is a named set of permissions — *see*, *edit*, *create*,
*delete*, *review*, *export*, *lock*, *request*, *manage rights*. Six are
shipped: **viewer** (see), **designer** (see, edit, create, delete,
export, request), **checker** (see, review, export), **library
maintainer** (see, edit, create, delete, review, lock), **line admin** and
**network admin** (everything, on their scope). An admin may define
others.

An **assignment** gives a user or a group a role on a **scope**: the whole
network; a line; a route, or the part of a route between two of its
structures (whole tension sections, as a working set); a region (a
polygon); an object kind (on a line or everywhere); one object; a library
kind, file or entry (on a line or everywhere) — every part given must
hold ("the costs of line B" is a line and a library kind). Nothing is
allowed without an assignment. A user's permissions are those of all
their own and their groups' assignments together; editing, making,
removing, reviewing and exporting need seeing too. A network admin may
also set a **deny** (a role's permissions, or everything) on a scope: it
wins over every grant there.

On the safe side: a region gives what lies wholly inside it and denies
what it touches; someone who may see part of a line also sees its
settings and library files (needed to solve its sections), never more;
merging, rolling back, branching, committing, following a line whole and
exporting it need a right on the **whole line**. What a user may not see
never reaches them — not in the tiles, the changes, the catch-ups, a
working set, a route's structures, the users' list or an error message
("refused: it touches objects you have no access to"); a working set
reaching something they may not see is refused.

Rights are managed on the command line (the admin app OHL-Server is a
plan waiting for the owner). `--user` is who acts: their own rights
decide whether they may.

    ohl network rights bootstrap --user ana          # the first network admin (only while none)
    ohl network rights add-user ben --user ana       # users, groups, roles: network admins
    ohl network rights add-group crew --user ana
    ohl network rights add-member crew ben --user ana
    ohl network rights put-role surveyor --permissions see,export --user ana
    ohl network rights assign --to group:crew --role designer --line L001 --user ana
    ohl network rights assign --to user:ben --role viewer --route R7 --user ana
    ohl network rights assign --to user:ben --role designer --route R7 --from-structure T3 --to-structure T9 --user ana
    ohl network rights assign --to user:ben --role viewer --region "0,0;900,0;900,700" --user ana
    ohl network rights assign --to group:crew --deny --line L002 --library-kind costs --user ana
    ohl network rights list                          # roles, users, groups, assignments (numbered)
    ohl network rights unassign 12 --user ana
    ohl network rights disable ben --user ana        # no rights at all; connections let go
    ohl network rights explain --as ben --permission edit --line L001 --kind structure --id T5 --user ana
    ohl network rights see-as --as ben --user ana    # what ben may see of each line
    ohl network rights audit --user ana              # network admins: changes and refusals

- A **line admin** gives rights within their line (never a deny, never
  more than they hold); users, groups, memberships and roles are the
  network admins'. The last network admin cannot be taken away.
- A running server takes a change before its next request (at the latest
  with its next heartbeat); a connected user whose rights changed is
  told and let go, and joins again with their new rights.
- **See as user**: an admin (whoever manages rights) may join seeing as
  another user (what both may see; nothing is changed so; in the audit).
- Every change, with what it was and is, and every refusal is in the
  network's **audit** (append only).
- Importing lines needs a network admin (anyone while the network has
  none); exporting a line needs *export* on the whole line.

### Country libraries (phase 9, in active development)

*Built 2026-11-20, in active development, not approved yet
([decisions](../development/decisions.md#2026-11-20-country-server-phase-9)).*

A network keeps each library kind — wires, structure types, insulator
assemblies, weather cases, rules, costs, … — **once for the whole
network**. Each line still holds its own library files, as a project
does: the files of the **release** it is **pinned** to. So a line solves
exactly as before, and a line exported as a project file carries the
library entries it uses.

- **Locked kinds.** A network admin or a library maintainer locks a kind
  (or one file): from then on it changes only by **requests granted**
  and **releases**, and no line may change its own files of that kind.
  Unlocking is for network admins only. A network admin may change a
  locked file directly in an emergency (`--emergency`, in the audit).
- **Maintainers** are not a list of their own: whoever holds *review* and
  *lock* on the kind — the shipped *library maintainer* role assigned on
  the kind (`ohl network rights assign --to user:mia --role
  library_maintainer --library-kind cables`), and network admins.
  `ohl network library kinds` names them.
- **Requests.** A designer asks for an entry the network does not have
  (`ohl network library request`), for a line and a branch of their own.
  The request may be granted or refused by a maintainer — never by the
  person who asked. While it is pending, the entry may be used **in the
  requester's own branch only**, as a library file named after the
  request (`request-12.ohlw`); that branch is **not reviewed or merged
  into main** until the request is granted, or until the draft is taken
  out of the branch (another entry used instead). Requests are seen by
  whoever asked and by the kind's maintainers (a draft not yet asked:
  only by whoever wrote it); a maintainer who may not see the line it is
  for sees the request without its line.
- **Releases.** A maintainer releases the kind as it is now under a tag.
  **A release changes no line.** Lines in design **follow the latest**:
  `ohl network library status` shows them behind, and *Update library
  pins* moves them. **Built** lines (`ohl network library built LINE`)
  stay pinned; their pins move only by whoever may review the whole
  line.
- **Update library pins** moves a line's files to the latest releases (or
  one kind to a release named) as **one** logged command; the routes
  whose results change are marked out of date, as when a shared tower
  moves. It is refused for a kind when the line uses an entry the
  release lacks. It is not undone by Undo: update the pins again, to the
  release before.
- **Withdrawn entries.** A maintainer withdraws an entry: lines using it
  keep it (it stays in later releases) and are shown it in their status;
  no object may start using it.
- **Where is it used?** `ohl network library uses KIND ENTRY` lists the
  lines, routes and objects using an entry — only those you may see; an
  entry you may not see is "not found", as one that is not there.
- **Importing a project** maps its library files onto the network's: each
  entry the same, added (a kind not locked), or asked for (a locked kind,
  or an entry that differs — for a maintainer; never replaced). The line
  keeps its own files until *Update library pins*.

On the command line (`--user` is who acts; their rights decide):

    ohl network library lock cables --user mia
    ohl network library put weather north.ohlwc --user dan          # a kind not locked
    ohl network library request cables new-wire.ohlw --into acsr.ohlw \
        --line L001 --branch my-branch --reason "new conductor" --user ben
    ohl network library requests --user mia
    ohl network library grant 12 --user mia
    ohl network library release cables 2026-11 --user mia
    ohl network library status --user ben
    ohl network library update-pins --line L001 --dry-run --user ben
    ohl network library update-pins --following --user ana          # every line in design
    ohl network library withdraw cables acsr.ohlw old-wire --note "out of production" --user mia
    ohl network library uses cables acsr-drake-26-7 --user mia
    ohl network library built L001 --user ana

*Update library pins* on the command line needs the network's writer (no
server running); a running server carries it out when a client asks it
through its protocol (a command for it in OHL-Design is to come).

### Checks with their neighbours (phase 10, in active development)

*Built 2026-11-20, in active development, not approved yet
([decisions](../development/decisions.md#2026-11-20-country-server-phase-10),
[theory](../theory/scoped-checks.md)).*

A check on a working set can depend on what lies beside it: another
line's wires crossing or running alongside, this line's wires beyond the
working set, other lines' sections on a shared tower, other lines'
currents near. Asked of the network's server, a check **pulls in what
lies within its own reach** and computes there:

- **Clearances between wires**: every wire within the rules' distance —
  other lines' too (crossings among them) — is a target.
- **Magnetic field**: the lines within 500 m (a setting) are added. The
  wires farther away are left out: the result says **NOT ON THE SAFE
  SIDE**.
- **Structure loads**: a tower shared with another line carries every
  line's sections, checked under each line's load cases, the worst kept.

Every result says **what it pulled in** (the lines, routes and objects) and
is **marked stale** when any of them changes: OHL-Design marks your
working set's routes out of date, as when a shared tower moves. A
neighbour you may not see is computed on the server too, so the numbers
are complete; it is never named or placed — "a hidden object of another
line is within 3.4 m", "check result limited by an object you may not
see" — only counted. A neighbour that could not be read or solved makes
the result **NOT ALL CHECKED**, never a pass. The bench `ohl network
bench-scoped --user admin` measures the three checks
([performance](../development/performance.md#synthetic-country-phase-10-scoped-computations-2026-11-20)).

### Branches over a working set (phase 11, in active development)

*Built 2026-11-20, in active development, not approved yet
([decisions](../development/decisions.md#2026-11-20-country-server-phase-11)).*

On a network, a designer of a route — or of a part of one — can work on
a **branch of their own over their working set**, without needing the
whole line:

- **Making one**: a branch over a set of routes is made from a line (main,
  usually) by whoever sees each route and may edit something on it; its
  maker may name a **team** (users or groups) who work on it too. It
  holds **only what it changes**, never a copy of the line. A branch of
  the whole line still needs edit on the whole line.
- **Working on it**: its maker and its team only. The rules of towers
  shared by several routes apply to every edit on it, its routes being
  the working set: what would break another route's connection is
  refused, naming it; moving a shared tower is allowed. Rights decide
  every object as on main.
- **Committing** — on a branch or on main — needs edit only on **what the
  commit records** (the changes since the last revision, whoever made
  them); a commit recording something you may not change is refused
  (without naming it when you may not see it). A tag still needs the
  whole line.
- **Merging it into main** needs only what the merge changes there. It is
  worked out from what differs and checked where it changes, as an edit
  is. A merge touching a shared tower: a broken connection is a
  **conflict**, named, to choose; a tower moved marks the other routes'
  results **out of date** and the users connected who may edit those
  routes get a notice; **a protected main waits for a review** — submit
  the branch, a reviewer of the whole line approves it, then it is
  merged. Bringing main's changes into the branch is not checked again
  (they were on main).
- **Library drafts**: a pending library request's draft may be used in a
  branch over a working set whose maker named the requester in its team.
- **Deleting or renaming** it: its maker.

A network's revisions are kept as **split trees**: a commit, a diff or a
merge reads and writes what changed, not the whole line (on the
12 000-pylon synthetic country as one model, a commit of one changed
route takes a tenth of a second instead of seven seconds, a branch's
merge a tenth of a second instead of half a minute). A project
file on the desktop keeps its revisions as before; a model exported from
a network keeps its split revisions, which this version and later read.

**Edits made directly on main** (since 2026-11-20) follow the same rules
of shared towers: the routes that are yours — those your rights let you
edit something on, and of them only your connection's working set if it
follows one — are your working set. Deleting or changing what another
route's sections hang on is refused, naming the route; moving a shared
tower is allowed, the other routes are marked out of date and their
connected owners told; on a protected main such a move waits for a
review. Someone who may edit every route of the line and follows no
working set edits as before.

### Offline working sets (phase 12, in active development)

*Built 2026-11-20, in active development, not approved yet
([decisions](../development/decisions.md#2026-11-20-country-server-phase-12)).*

Take your working set with you where there is no connection, and bring
the work back:

1. **Export it** — `ohl network export-working-set --line L001 --route R7
   --user ben ws.ohl` (or a part: `--from T3 --to T9`, `--stations
   1200:4800`; the server's message `export_working_set`). You get a
   project file of today's format: your working set editable; its
   context and your line's neighbours within the checks' reach (500 m
   by default, `--reach`) read-only; other lines' wires within that reach
   kept for the checks. You need to see everything it holds, *export* on
   each object of your working set, and to be able to edit something of
   each of its routes. Anything you may not see is **left out and only
   counted**. If the main line has changes not recorded in a revision,
   they are recorded first, as a revision **by the network's server**
   (author `network:server`, audited under your name as *server
   revision*; since 2026-11-23): any designer who may export can, not
   only someone who may edit every change.
   The export is recorded on the server and in the audit; nothing is
   reserved for you. *Since phase 14* the server writes the file on a
   thread of its own, so other users keep editing meanwhile: the file is
   your working set as it was when the server took it (its base
   revision). If something in it, or within its reach, changes on the
   server while the file is being written, the report says how many
   objects changed and marks the export **stale** (`"stale": true`,
   `changed_meanwhile`): those changes are not in your file — export
   again for the newest state, or bring your work back as usual (they
   are listed as conflicts with main, or as neighbours changed since).
   If your rights change while it is written, you get nothing: ask
   again.
2. **Work offline** in OHL-Design or on the command line, and **commit**
   your work. The read-only neighbours cannot be changed. `ohl network
   check-offline ws.ohl --check clearances --case "10 °C" --rules
   rules.ohlr` runs the network's checks on the file with the neighbours
   it holds — the same report as the server's, or **NOT ALL CHECKED**
   where the file cannot give it (neighbours you may not see were left
   out; the check reaches beyond what was exported, for example after
   you moved the working set far).
3. **Make the bundle** — `ohl network offline-bundle ws.ohl --out
   work.bundle` holds only your revisions since the export and what they
   changed (uncommitted changes refuse it: commit first).
4. **Bring it back** — `ohl network import-bundle work.bundle --user ben`
   (the server's message `import_bundle`). The server checks the bundle
   against the export it recorded and applies your changes on **a new
   branch over your working set** made from the export's base — never on
   main — checked by your rights **as they are now** and the rules of
   shared towers, then committed. Objects you changed that also changed
   on main since your export are **listed as conflicts**: merging the
   branch into main asks you to settle them (nothing is settled for
   you). Refused, with nothing kept: a bundle changing a read-only
   neighbour or a library file, changed by hand, of an export the network
   does not know or of another user's, or brought back already.
   *Since 2026-11-20* the others' edits no longer wait for a big bundle:
   the server checks it at once, makes its branch's line ready on a
   thread of its own (seconds on a model of a whole country), then
   applies it between two edits (a fraction of a second). An edit of main
   landing meanwhile is one of its conflicts, as if it had come just
   before; if your rights changed meanwhile nothing is applied (bring it
   back again).

Then merge the branch into main as any branch over a working set
(above). *Since 2026-11-21* OHL-Design does the four steps without the
command line: Versions › Offline copy › [Offline working
set](offline-working-set.md) (export, check offline, make the bundle,
bring it back; a stale export said in red with what changed
meanwhile).

### Security, sign-in and backups (phase 13, in active development)

*Built 2026-11-20, in active development, not approved yet*
([decisions](../development/decisions.md#2026-11-20-country-server-phase-13),
[the security review](../development/server-security-review.md),
[deploying a server](server-deployment.md),
[commands](network-commands.md#operations-phase-13)).

- **Checks, exports and bundles wait behind the edits.** They are long
  work: the server does them one at a time, only when no edit, view or
  join is waiting, at most one per user waiting and four in all ("the
  server is busy … try again in a minute" beyond). A check reads at most
  10 km around its working set; a field's distance is at most 2 km.
- **New tiles**: each user may have 64 new tiles made a second (256 at
  once); the server makes at most 1 024 at once; tiles beyond are not
  made then, and a notice says to ask again (moving the view does).
- **Invitations reissued and expiring.** A network admin makes a user's
  invitation again (`ohl network rights reissue-invitation NAME`): every
  older one of that user is refused, and they are let go if connected.
  `ohl network serve --invite NAME --invite-days 30` prints invitations
  valid to the end of that day (UTC) — 90 days by default since
  2026-11-20 (`--invite-days 0`: until reissued). Invitations given
  before stay valid until reissued.
- **Signing in with the organisation's account (OIDC)**, if the owner's
  IT wants it: the server takes an ID token of the identity provider
  instead of an access code, checks its signature, issuer, audience and
  times, and lets the token's subject in as the network user a network
  admin mapped it to (`ohl network rights map-identity`). Personal
  invitations stay the default and keep working. Since 2026-11-20
  OHL-Design's **Sign in to a network** window (File › Join a network)
  offers it when the server does — the browser's sign-in, no password in
  OHL-Design, the token in memory only — and says when an invitation
  expired or was reissued ([Signing in to a network](sign-in.md)); the
  web client does not offer it yet.
- **Backups** (`ohl network backup`) beside the running server, **a
  restore** (`ohl network restore`) into an empty database that checks
  every line against the backup (the restore drill), and **schema
  migrations refused** unless a backup newer than a day (the network's
  `backup_max_age_hours`) is recorded — or an admin says they have one
  (`ohl network migrate --i-have-a-backup`, in the audit).
- **A database on another host** is reached with TLS only
  (`sslmode=verify-full` and the owner's CA), unless `sslmode=disable` is
  said.

## Roles

*A project file's server.* On a network's server the roles are the
network's (viewer, designer, checker, library maintainer, line admin,
network admin, and roles of an admin's own), given by assignments on a
scope — [above](#rights-on-a-network-phase-7-in-active-development); the
edit grants a line's project file brought with it are still checked beside
them.

Each user of a project has one role; admins give them (People). The
**Roles** page (View) shows this table, your own role marked. Edit grants
may narrow a designer further (by kind, object or area).

| May | Viewer | Designer | Admin |
|---|---|---|---|
| Open the project, see the line, its results and checks, switch branches | ✔ | ✔ | ✔ |
| Edit the line (structures, sections, wires, terrain, library files, settings) and undo or redo | — | ✔ | ✔ |
| Commit, make branches and tags, revert, delete unprotected branches | — | ✔ | ✔ |
| Create project lines and bring another project in | — | ✔ | ✔ |
| Submit reviews; approve or reject others' reviews; merge an approved review | — | ✔ | ✔ |
| Change a protected branch directly; merge into it alone, without another's approval | — | — | ✔ |
| Protect or unprotect branches; delete tags and protected branches | — | — | ✔ |
| Roll back anyone's edits; roll a project line back to a revision; reproject | — | — | ✔ |
| Add users, change roles, grant or revoke edit rights by kind, object or area | — | — | ✔ |
| Pass other users' locks | — | — | ✔ |

The table is the program's (`ohl_session::ROLE_ABILITIES`); a test keeps
it in step with the checks.

## Limits against misuse

Somebody holding an invitation must not be able to stop the others (security review 2026-10-26):

- A request that fails inside the server (a bug) is refused to its sender ("an internal error of the server"); the server reads the project from its file again and keeps running for everyone.
- **Joining:** at most 12 joins a minute from one address; after a **wrong access code** the address waits 1 s, then 2 s, 4 s … up to 5 minutes (a right code resets it); at most 8 connections of one address that have not joined yet (application and browser each), at most 256 application connections in all — both are the host's parameters (`ohl serve --max-per-address N --max-connections N`, or **Connections at most** and **Per address at most** in File › Host this project) and hold for this computer's own connections too: behind a reverse proxy every user comes from `127.0.0.1`, so raise the per-address limit there. A copy of the project is shared by everyone joining while nothing changed, at most 8 copies exist at once (more joins are asked to try again in a minute), and a copy not taken within a minute (plus 1 s per 256 KiB) is dropped. The general invitation adds at most 200 new users in one run of the server. The join rate and the waits after wrong codes leave the computer itself (`127.0.0.1`) out.
- **Presence and locks:** a connection's presence updates beyond 10 a second (20 at once) are dropped, its lock requests beyond 20 a second refused; the list of users is sent to everyone at most 10 times a second.
- **Messages** are read as their bytes arrive (an announced size alone allocates nothing), at most 64 MiB each; on a network's server (phase 13) messages over 1 MiB share one budget of 256 MiB at once, and a browser's messages are at most 8 MiB after it joined. The limits per address count an IPv6 address by its /64.
- **Every edit is written once** for everyone (2026-10-27): the server encodes each edit, lock change and users' list once and sends the same bytes to every application and browser, so many users cost the server little more than one.
- **Names:** a user name has no invisible characters (zero-width spaces, direction marks), and a newcomer whose name looks like a user's (`Аdmin` with a Cyrillic A beside `admin`; letter case, `0`/`o`, `1`/`l`/`i`, spaces, `-`, `_` and `.` ignored; Cyrillic and Greek look-alikes, full-width letters) is refused.
- **Reviews:** approved or rejected only by designers or admins, never by the review's author ([Versions, branches and reviews](versions.md#reviews)).
- The copies of the project for joining users are written in a folder of the temporary folder that only the server's user may open.

## Limits for now

- Edits wait for the server's answer (milliseconds on a network; at most 15 s) before they show. Edits are not shown before the server confirms them.
- Joining a project file's server copies the whole project; joining a network's server sends a working set and the tiles around the view (above).
- The application speaks QUIC only (UDP). Browsers use the web client's WebSocket (TCP); the application cannot use it yet, for networks that block UDP.
- The web client: a viewer of the main line (of the tiles around the view, on a network's server); one weather case, no terrain, point clouds or backgrounds.
- On a project file's server a user's name and the access code are the only identity. A network's server may also take sign-ins of the organisation's identity provider (OIDC, phase 13: [above](#security-sign-in-and-backups-phase-13-in-active-development)), offered by OHL-Design's sign-in window ([Signing in to a network](sign-in.md)); the web client's is to come.
- If the connection drops, the project turns read-only. Join again to continue.
