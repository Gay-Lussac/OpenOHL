# The country server's database: running it and its tests locally

*In active development (ADR-0013, phases 4–6, 2026-11-19, phases 7, 9, 11 and 13 2026-11-20; the tests' scheme O46, 2026-11-19); not approved yet.* Every command: [A network on the command line](../user-guide/network-commands.md).

A country server keeps every model of its network in one PostgreSQL
database with PostGIS ([ADR-0013](../adr/0013-country-server.md); the
tables are in the [data model](data-model.md#server-database-postgresql)).
The crate `ohl-store-pg` is the session's store on it; the desktop's
project files are not touched by any of this.

## What you need

- **PostgreSQL 17** and **PostGIS 3** (on macOS with Homebrew:
  `brew install postgresql@17 postgis`). Nothing has to run as a service:
  the tests start a database of their own and stop it.
- The tests look for the binaries (`initdb`, `pg_ctl`) in `OHL_PG_BIN`,
  then in Homebrew's `/opt/homebrew/opt/postgresql@17/bin`, then where
  `pg_config --bindir` says. **Without them the tests say so on stderr and
  pass** (the continuous integration has no PostgreSQL); set
  `OHL_REQUIRE_PG_TESTS=1` to make a missing PostgreSQL a failure.

## The tests

```sh
cargo test -p ohl-store-pg -- --test-threads=4
```

**One throwaway cluster per test program, a database per test**
(`ohl_store_pg::testing::Cluster`, `crates/ohl-store-pg/src/testing.rs`;
O46, 2026-11-19; in active development, not approved yet). The first test
of a program makes the cluster in the temporary directory
(`ohl-pg-tests/KEY/`, the key from the program's path and PostgreSQL's
binaries) and starts it on a free port of 127.0.0.1 only (no Unix socket,
no fsync: it is thrown away); the program's other tests join it; each
test works in databases of its own (named with its process and test, e.g.
`t4711_0_lock_net`), dropped at its end, and a folder of files of its own.
The last test out stops the cluster and deletes it — also when a test
fails.

Why this scheme: nextest runs every test in a process of its own, so the
cluster is shared between processes, by locks on files that the operating
system lets go with the process (`flock`), never by a process that has to
stay alive:

- `KEY.lock`, the key's lock, held while a test joins, makes, leaves or
  stops the cluster (at most 180 s waited for, else a failure);
- `KEY/users.lock`, on which every test using the cluster holds a shared
  lock. A test leaving takes the key's lock, lets its own go and tries the
  exclusive one: got, no test uses the cluster — stopped and deleted. A
  test killed (a timeout) lets its lock go with its process: the next test
  of any program finds the cluster unused and stops and deletes it.
- **Its port**: asked of the system, then the cluster started on it; a
  start refused because another program took the port meanwhile is made
  again on another port (up to 20), and every cluster started or joined is
  checked to be this one — its `data_directory` is this folder — before
  any test uses it.

Isolation stays as before: a database per test (PostGIS, the schema, the
writer's lock and the advisory locks are a database's own), its own
files; nothing a test makes is seen by another. A connection's lock
left to go with it goes only once PostgreSQL's process for it has ended,
a moment after the client closed it; since 2026-11-19 (owner) a writer
releases its lock explicitly (`pg_advisory_unlock`) when its connection's
last user — the network or one of its models' stores — is dropped, so a
writer connecting at once (a server restarted) is not refused
(`schema_and_the_writers_lock` tries it without waiting). The tests that
hand a network from an import to a server still wait for the locks to go
(`Cluster::wait_for_locks_gone`, at most 10 s) as a safety net. This was the flake of 2026-11-19 (`schema_and_the_writers_lock`
refused its second writer while two check runs ran side by side): 1 run in
144 failed with six programs' tests started together eight times (the
three tests each in a process of its own, as nextest runs them); with the
scheme above, 0 in 144, and the same runs took 90 s instead of 126 s.

About 8 s for the seven tests (two programs, `network` and `library`):

- `store_suite_on_postgresql` — the **store suite**
  (`ohl_session::store::suite`, feature `store-suite`), the same tests the
  project file passes (`ohl-session`'s `store_suite_on_project_files`):
  meta and users, objects (tombstones, places, the JSON text kept as
  written), nested steps (a failed statement rolled back), the log (seq,
  times, undo marks), the spatial index, versions, project lines,
  grants and reviews, surfaces, whole copies; then **one session on both
  stores** (import, edits, undo, redo, commit, branch, tag, opened again):
  the same transcript.
- `schema_and_the_writers_lock` — migrations once, a newer schema refused,
  one writer (a second refused), readers, the append-only audit, line
  codes; saved working sets (schema 3: replaced by name, per owner,
  stations kept exactly, refusals) and a database of schema 2 brought
  forward (its working set of routes reads as those routes picked whole).
- `import_and_export_round_trip` — a project imported (one log entry,
  the history left in the file), exported back (the same object, index,
  meta, user and grant rows), edited through the network (the export
  follows), imported again under another code (every id renamed, every
  tower a twin; mapped back, the same line), ids unique in the network,
  refusals (a code taken, another coordinate system, a part of a line)
  leaving nothing behind, the audit.
- `rights_changes_through_the_one_function` (phase 7) — the shipped
  roles as the session knows them; the first network admin by bootstrap
  only; users, groups and roles by network admins; an assignment by who
  manages its scope and holds its role's permissions there; a deny by
  network admins only; the last network admin kept; a route scope given
  its line; a region read back exactly; the audit (what it was and is,
  the refusals) and the rights' revision.
- `schema_3_brought_to_4` (phase 7) — a database of phase 6 brought
  forward, its assignments read.
- `country_libraries_through_the_rights` (phase 9, `tests/library.rs`) —
  the import's mapping (entries added) and use index; a kind locked: a
  network-wide designer, a line's designer and maintainers without an
  emergency refused, a network admin's emergency change only; an
  unlocked kind changed by a network-wide designer, not by a line's;
  an entry named once per kind; releases by maintainers only; requests
  (never in main, only in one's own branch) seen by their requester and
  the maintainers (a draft by its writer only; the line hidden from a
  maintainer who may not see it); a draft only in its requester's branch,
  blocking main until granted by another maintainer; Update library pins
  (the release's files, one entry, the pins, the stale marks; a built
  line only by who reviews it; a release changing no line); a withdrawn
  entry kept and shown; the use index through the rights (a viewer of
  the kind without lines, of a route, of a line's structures; a denied
  entry not found in the words of one that is not there); the export's
  pinned files and meta; the audit.
- `schema_4_brought_to_5` (phase 9) — a database of phase 7 brought
  forward, the libraries' tables made.

- `tests/operations.rs` (phase 13, a program of its own): the **restore
  drill** (`backup_and_the_restore_drill`: a network of two models
  edited, committed and branched, backed up by `pg_dump` in a snapshot,
  restored into an empty database, each model's rows the same byte for
  byte, the manifest's hashes checked by the restore; refused: a database
  not empty, a dump changed, another schema, a manifest naming another
  file, rows differing from the manifest), **migrations after a backup**
  (`migrations_wait_for_a_recent_backup`: none, too old, the setting, a
  real backup, `--i-have-a-backup` audited, an empty database made
  without), `invitations_and_identities`, and **TLS**
  (`tls_to_the_database`: the test cluster given a certificate of a CA
  made by the test — `ALTER SYSTEM SET ssl = on`, reset at the end —;
  `verify-full`, `verify-ca` and `require` encrypted, another CA and a
  name the certificate is not for refused). They need `pg_dump` and
  `pg_restore` beside `initdb`.

## Backups, restores and TLS (phase 13)

Built 2026-11-20, in active development, not approved yet
([decisions](decisions.md#2026-11-20-country-server-phase-13),
[deployment](../user-guide/server-deployment.md)):

- `ohl network backup --out DIR` runs `pg_dump --format=custom
  --snapshot=…` in a snapshot its own connection exports, and reads in
  the same snapshot each model's rows (every table with a `line_id`,
  each row's JSON in the order of the table's key, times in UTC) and the
  network's own tables (the rest), hashed into `manifest.json` with the
  dump's SHA-256. Recorded in `net_meta` `last_backup` (every schema has
  it), `net_backup` and the audit.
- `ohl network restore DIR` into an empty database (PostGIS's own tables
  allowed): the manifest of this program's schema only, the dump's hash
  checked, `pg_restore --single-transaction --exit-on-error --no-owner
  --no-privileges --no-comments`, then the hashes compared (the restore
  drill).
- A migration (`Network::connect` on an older schema, the server's
  start, `ohl network migrate`) needs `last_backup` newer than
  `backup_max_age_hours` (24), else `--i-have-a-backup` (audited); a new
  database is made without.
- **TLS** (`ohl_store_pg::connect`): libpq's `sslmode` and `sslrootcert`
  read from the connection string (key/value or URL), rustls with ring;
  a host that is not this computer is refused without TLS unless
  `sslmode=disable` is said; the password from the string or from
  `OHL_DATABASE_PASSWORD_FILE`. `pg_dump` and `pg_restore` get the TLS
  settings and the password in their environment (`PGSSLMODE`,
  `PGSSLROOTCERT`, `PGPASSWORD`), never on their command line.

## The server's tests on a network

```sh
cargo test -p ohl-server --features network -- --test-threads=2
```

The feature `network` (the command line enables it; the desktop
application does not) builds the server on the database; its tests make
the server's test program's cluster (`ohl_store_pg::testing::Cluster`,
feature `testing`, the same scheme as above), each test a database of
its own with a small synthetic country imported line by line, 2 to 6 s
each:

- `a_network_serves_by_interest` — one writer per database; protocol 2
  and a desktop's hello refused with the reason; a desktop following a
  line (its copy equal to the server's export, row by row, log numbers
  included); a viewer of tiles; an editor on every line. What a command
  keeps (the routes and references of what it changed, its log entry's
  lines and tiles, the stream row); tiles read again at the change's
  revision, only those it touched; **the property**: 24 random edits and
  undos, then every client has received exactly the changes of the
  stream touching its interest, in order; a library change reaching a
  viewer as "read again"; **a lost change** shown by the next heartbeat
  and brought by a catch-up; **a desktop gone** while three moves and an
  undo were made: joining again from its position brings exactly those,
  in order, and its replica equals the server's line again.
- `a_network_keeps_roles_and_locks` — edits by each line's roles, a line
  the network has not refused, soft locks as on a file server.
- `a_working_set_is_a_partial_replica` (in `working_set_tests`, phase 6,
  about 3.5 s) — a route's structures; a part of it shown, snapped out to
  whole tension sections, a whole route, an unknown route refused; saved,
  listed, deleted; a desktop joining with the part gets its replica (the
  server's rows, its working set noted); another user's moves: rows for
  what it holds, none for the rest of the route or another line; its own
  edit's rows before its answer; 16 random moves and undos, then the
  replica equal to the model's extent object by object; a working set on
  another line refused, the whole route added (rows came, none went); the
  desktop gone while three moves were made, joining again with what it
  holds: only what differs comes.
- `the_idle_solve_makes_every_tile_once` (phase 6, about 2.6 s) — after a
  start every tile holding objects made once and kept, served from the
  cache to a viewer, nothing sent to a client that did not ask; a move
  makes the tiles it touched again. The other tests run with the idle
  solve off.
- `rights_decide_commands_and_what_is_sent` (in `rights_tests`, phase 7)
  — only the network's users join, with their personal invitations (in
  the audit when refused); a designer of one route: her edits pass on it,
  are refused elsewhere (nothing kept, nothing named); tiles, the
  network's lines and routes, changes ("read these tiles again"),
  catch-ups, route details and picks filtered; following a line whole
  refused.
- `grants_denies_and_groups_while_connected` — a deny set before the
  grant still wins; a line admin may set no deny nor give rights on
  another line; the deny taken away, the member taken out of her group,
  the user disabled — each while she is connected: let go, nothing more
  on the old connection, less seen after; the audit.
- `see_as_a_user` — an admin seeing as a viewer of one line: that line
  only, edits refused, in the audit; a viewer may not see as another.
- `country_libraries_on_the_server` (in `library_tests`, phase 9, about
  3 s) — Update library pins asked by a client, planned and carried out
  by the server (its carried-out form refused from a client; nothing to
  update said); a kind locked while the server runs: a line's file of it
  refused (put and batch); a pending draft in its requester's own branch
  (refused in main and for another user), the branch's review and merge
  refused until the request is granted, then merged; a withdrawn entry:
  a move of what uses it passes, a new use refused, then (restored) the
  use index follows the command; a built line's pins refused to a
  designer.
- `property_no_message_holds_a_hidden_id_or_place` — six rounds of random
  assignments, moves, tiles, catch-ups, route details and picks: no id
  and no place of what the user may not see in any message (about 7 s).
- `web_viewers_read_tiles_of_a_network` (in `web::tests`) — a browser:
  protocol 2 refused, the network's info, its tiles, an edit refused
  (a viewer), a QUIC user's move arriving and the tile read again.

## Running the server on the database locally

With a database made as below and lines imported:

```sh
ohl network rights bootstrap --user admin    # once: the first network admin
ohl network serve --bind 127.0.0.1:4433 --web 8443 --invite admin
```

It prints the invitations and the web link (open it, accept the
self-signed certificate after comparing its fingerprint, look around).
`ohl network bench --user admin --work DIR` measures it (phase 5's
targets, the idle solve off; `ohl network bench-working-set --user admin
--work DIR` phase 6's: working sets shown and loaded, rows, joining
again, a route added, context tiles, then the idle solve; `ohl network
bench-library --user admin` phase 9's: a library released, Update
library pins of every line in design, the use index, moves — as the
writer, with no server running; `ohl network bench-versions --user admin
--line CODE` phase 11's: a commit of one changed route, a branch over a
working set — its diff, ahead and behind, its merge — and a merge at a
shared tower, with split trees and with flat ones on the same model, as
the writer) (it starts
`ohl network serve` as a process of its own, then the server in its own
process; the machine's other work shows in the numbers; it moves
structures back and forth, so use a database made for it):
[performance](performance.md#synthetic-country-phase-5-the-server-on-the-database-2026-11-19).

## A database to try it by hand

The same steps as the tests, by hand (a folder of your own; never a
service):

```sh
BIN=/opt/homebrew/opt/postgresql@17/bin
DIR=$HOME/ohl-network-try
LC_ALL=en_US.UTF-8 $BIN/initdb -D $DIR -U ohl --auth=trust --no-locale -E UTF8
LC_ALL=en_US.UTF-8 $BIN/pg_ctl -D $DIR -l $DIR/log -w \
    -o "-p 54320 -c listen_addresses=127.0.0.1 -c unix_socket_directories=''" start
$BIN/createdb -h 127.0.0.1 -p 54320 -U ohl network
export OHL_DATABASE_URL="host=127.0.0.1 port=54320 user=ohl dbname=network"

ohl synth country --pylons 800 --per-line --lidar-density 0 --out /tmp/country
ohl network import /tmp/country/lines/*.ohl --user admin --report /tmp/reports
ohl network lines
ohl network export --line L001 --user admin /tmp/L001.ohl
ohl network measure --line L001 --user admin

LC_ALL=en_US.UTF-8 $BIN/pg_ctl -D $DIR -w -m fast stop
rm -rf $DIR
```

`LC_ALL` must be set for `pg_ctl` too on macOS (without it PostgreSQL
stops at once: "postmaster became multithreaded during startup").
`unix_socket_directories=''`: a long folder path does not fit a Unix
socket's name; the network is reached over 127.0.0.1.

The first connection makes the schema (`CREATE EXTENSION postgis` needs a
role allowed to make extensions — the cluster's own user here; on a
shared server an administrator makes it once). The connection string
comes from `--database` or `OHL_DATABASE_URL`, never from a project file;
a password belongs in the environment or PostgreSQL's password file, not
on the command line.

## What is not there yet

Rights are evaluated since phase 7, managed with `ohl network rights …`,
the country libraries with `ohl network library …` (phase 9) — the admin
app OHL-Server is a plan; Update library pins from OHL-Design is to come
(the server's operation is there); point clouds, backgrounds and tiled terrain stay in the project files (the
import report lists them) and the server does not serve them yet; a
model's surfaces are in the database whole, not served by tile yet; a
working set is on one model (routes of another line are their own working
set); checks on a working set pull no neighbours in yet (phase 10);
branches over a working set and split trees (phase 11) are made through
the server's protocol, no command line yet. Backups are `pg_dump`'s
(phase 13, above); WAL archiving and point-in-time recovery are
PostgreSQL's own, beside them, not driven by `ohl`.

**A big model's first revision in an empty network** (found 2026-11-20,
phase 11's measurements): while the version store's tables have no rows
the planner's statistics say so, and checking each blob of a first
revision of 100 000 objects reads the rows written so far in the same
transaction one after the other (minutes instead of seconds). **Since phase 14** the
store reads the version store's statistics again (`ANALYZE vc_blob,
vc_blob_use`, inside the transaction: its own rows counted) after 2 000
blobs put, then each time their number doubled: the synthetic country's
first revision as one model (an export recording it first) took 25 s
instead of more than 7 minutes (stopped there,
[measured](performance.md#synthetic-country-phase-14-scale-validation-and-the-exports-thread-2026-11-20)).
Only statistics change; a store reading many blob hashes at once is
still to come.
