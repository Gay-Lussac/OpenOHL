# The country server's database: running it and its tests locally

*In active development (ADR-0013, phase 4, 2026-11-19); not approved yet.*

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

Each test makes a throwaway cluster in the temporary directory, starts it
on a free port of 127.0.0.1 only (no Unix socket, no fsync: it is thrown
away), and stops and deletes it at the end — also when the test fails.
About 5 s for the three tests:

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
  codes.
- `import_and_export_round_trip` — a project imported (one log entry,
  the history left in the file), exported back (the same object, index,
  meta, user and grant rows), edited through the network (the export
  follows), imported again under another code (every id renamed, every
  tower a twin; mapped back, the same line), ids unique in the network,
  refusals (a code taken, another coordinate system, a part of a line)
  leaving nothing behind, the audit.

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

Rights are kept (roles, assignments) but not evaluated (phase 7); point
clouds, backgrounds and tiled terrain stay in the project files (the
import report lists them); the server does not run on the database yet
(phase 5); refusing a migration without a recent backup, TLS to a
database on another host and the deployment files are phase 13.
