# A network on the command line

Every `ohl network …` and `ohl synth …` command, with what it does, who
may run it and an example. Built, in active development, not approved
yet ([ADR-0013](../adr/0013-country-server.md); setting up a database:
[the country server's database](../development/postgresql.md)). `ohl
COMMAND --help` lists the same flags with their defaults.

**Common to all.** `--database` is a PostgreSQL connection string (default:
the environment variable `OHL_DATABASE_URL`; a password belongs in the
environment or PostgreSQL's password file, never on the command line).
`--user` is **who acts**: their rights on the network decide whether they
may (the rights' one function, [Rights on a network](server.md#rights-on-a-network-phase-7-in-active-development)),
and refusals and changes go into the audit. Commands marked *writer* need
the network's writer: no `ohl network serve` running on the database
(one writer per database; a second is refused).

**The database's connection (phase 13).** A database on another host
needs TLS: `sslmode=verify-full sslrootcert=/etc/ohl/ca.pem` in the
connection string (or `OHL_DATABASE_SSLMODE` and `OHL_DATABASE_CA` in the
environment); `sslmode=require` encrypts without checking the
certificate, `verify-ca` checks its chain but not its name; without any,
a host that is not this computer is refused, and `sslmode=disable` is the
explicit choice of none (a network you trust). The password may come
from a file: `OHL_DATABASE_PASSWORD_FILE` (a Compose secret). A schema
migration needs a recent backup ([operations](#operations-phase-13)).

## Lines: import, export, list

| Command | What it does | Who may | Example |
|---|---|---|---|
| `import FILES… [--line CODE] [--name NAME] [--tolerance M] [--no-revision] [--report DIR]` | Imports project files, each as a new line (model): the current state of its main project line as one log entry, ids another line has renamed `CODE-id`, twin towers within `--tolerance` (0.5 m, same type) listed for an admin, library files mapped onto the network's (the same, added to a kind not locked, or asked for), the revision "imported from FILE" unless `--no-revision`; the report printed (HTML and JSON with `--report`). The files are not changed. *Writer.* | a network admin (anyone while the network has none) | `ohl network import lines/*.ohl --user ana --report reports` |
| `export --line CODE OUT` | One line as a standalone project file of today's format (`network.*` meta keys added; its library files the pinned releases'; its revisions kept as they are — split trees since phase 11). | *export* on the whole line | `ohl network export --line L001 --user ana L001.ohl` |
| `lines` | The network's lines, its schema and size. | anyone reaching the database | `ohl network lines` |
| `measure --line CODE [--edits N] [--seconds S] [--export FILE]` | Times opening a line, edits (structures moved 1 m), undo and redo through the store; JSON. Edits the line (moved back). *Writer.* | a user who may edit the line | `ohl network measure --line L001 --user ana` |

## Offline working sets (phase 12)

The same four steps in OHL-Design, without the command line (2026-11-21):
Versions › Offline copy, [Offline working sets in
OHL-Design](offline-working-set.md).

| Command | What it does | Who may | Example |
|---|---|---|---|
| `export-working-set --line CODE --route ID… [--from A --to B \| --stations X:Y] [--reach M] OUT` | Your working set (whole routes, or a part of one route) as a project file of today's format: the working set editable, its context and the line's neighbours within `--reach` (500 m by default, at most 10 000) read-only, other lines' neighbours kept for the checks; what you may not see left out and counted. The main line's unrecorded changes recorded first, as a revision by the network's server itself (`network:server`, since 2026-11-23; audited under your name as *server revision*): any designer who may export can. Recorded on the server and in the audit; no reservation. Prints its report (JSON); phase 14: written on the server's exports' thread, the report's `stale` and `changed_meanwhile` say what changed on the server while it was written (not in the file). *Writer* (hosts the network in this process: refused while its server runs — its clients send `export_working_set`). | see on everything it holds, *export* on each object of the working set, edit on something of each of its routes | `ohl network export-working-set --line L001 --route R7 --from T3 --to T9 --user ben ws.ohl` |
| `offline-bundle FILE --out BUNDLE` | The bundle of an offline working set's work: its revisions since the export and only what they changed (the bundle format). Uncommitted changes refuse it. No database. | anyone holding the file | `ohl network offline-bundle ws.ohl --out work.bundle` |
| `import-bundle BUNDLE [--branch NAME]` | Brings the work back: the bundle checked against the export the network recorded, its changes applied on a new branch over the working set made from the export's base (default name `offline-USER-N-K`), checked by your rights now and the rules of shared towers, committed; conflicts with main listed, never settled. Refused, nothing kept: read-only neighbours or library files changed, a bundle changed by hand, an unknown base, another user's export, a bundle brought back already. In the audit. Prints its report (JSON). *Writer* (as `export-working-set`; clients send `import_bundle`). | the export's user, with their rights as they are now | `ohl network import-bundle work.bundle --user ben` |
| `check-offline FILE --check clearances\|fields\|electric\|loads [--case NAME] [--rules FILE] [--structure ID]… [--distance M] [--amps A] [--limit-ut B] [--limit-kv-m E]` | A check of the network's (clearances between wires, the magnetic field, the electric field with the lines near solved together (2026-11-24), the loads on towers shared with other lines) run on an offline working set with the neighbours it holds: the server's report, or NOT ALL CHECKED where the file cannot give it. No database; prints JSON. | anyone holding the file | `ohl network check-offline ws.ohl --check clearances --case "10 °C" --rules rules.ohlr` |

## The server

| Command | What it does | Who may | Example |
|---|---|---|---|
| `serve [--bind ADDR] [--advertise HOST] [--code SECRET] [--keys FILE] [--invite NAME]… [--invite-days N] [--web PORT] [--seconds N] [--readers N] [--packagers N] [--solve-threads N] [--packages-mb N] [--shapes-mb N] [--heartbeat-ms N] [--no-idle-solve] [--stats FILE] [--max-connections N] [--max-per-address N] [--max-heavy-queue N] [--tiles-per-second X] [--oidc-issuer URL --oidc-audience ID [--oidc-jwks-url URL \| --oidc-jwks-file FILE] [--oidc-ca FILE] [--oidc-max-age-s S]]` | Hosts the network (protocol 3): every line read once, each edit one transaction written before it is sent, clients served by interest (a desktop's working set, a browser's tiles), the context solved ahead while idle. Prints the personal invitations asked for — each of the user's current generation, valid `--invite-days` days (90 by default since 2026-11-20, when OHL-Design's sign-in window began showing an expired invitation; `0`: until reissued). Phase 13: checks, exports and bundles wait behind the commands (`--max-heavy-queue`, 4, one per user); each client may have `--tiles-per-second` (64) new tiles made, 4 times as many at once; OIDC sign-in with the owner's identity provider when `--oidc-issuer` and `--oidc-audience` are given (its keys from its discovery document, a URL or a file; its CA; tokens issued within `--oidc-max-age-s`, 3 600) — personal invitations still work; the server tells clients that ask (OHL-Design's Sign in to a network window; the web viewer's join page since 2026-11-21, whose security policy then names the issuer's origin) its provider's issuer and the client id, so they offer "Sign in with your organisation" — the provider must allow both redirects and the web page's origin ([deployment](server-deployment.md#sign-in-with-the-organisations-accounts-oidc)). `--newcomers` exists for the file server's options: on a network only `none` is accepted (only the network's users join). *Writer.* | whoever runs it on the database; joining needs a network user's personal invitation or a sign-in mapped to one | `ohl network serve --bind 0.0.0.0:4433 --web 8443 --invite ana --invite-days 30 --keys /var/lib/ohl/server-key` |

## Operations (phase 13)

| Command | What it does | Who may | Example |
|---|---|---|---|
| `backup --out DIR` | Backs the network up into a new (or empty) folder: `network.dump` (PostgreSQL's dump, custom format) and `manifest.json` (the schema, the time of the data, the dump's SHA-256, a hash of each line and of the network's own tables — read in the dump's own snapshot). Runs beside the server (nothing waits). Recorded in the network and the audit. Needs `pg_dump` (`OHL_PG_BIN`, Homebrew's `postgresql@17` or the `PATH`). | a network admin (anyone while the network has none) | `ohl network backup --out /backups/ohl-20261120 --user ana` |
| `restore DIR` | Restores a backup into an **empty** database (PostGIS may be in it), of this program's schema only, in one transaction; then the restore drill: every line and the network's tables compared with the manifest's hashes (a difference fails it, naming what differs). A dump changed since the backup is refused. In the restored network's audit. Needs `pg_restore`. | whoever reaches the empty database | `ohl network restore /backups/ohl-20261120 --database "host=db2 dbname=network sslmode=verify-full sslrootcert=ca.pem" --user ana` |
| `migrate [--i-have-a-backup]` | Brings the network's schema forward to this program's (8 since 2026-11-21: the branches' old rows of the spatial index deleted): only with a backup newer than the network's `backup_max_age_hours` (24) recorded; `--i-have-a-backup` migrates without one (you made one another way) and says so in the audit. An empty database is made without a backup. The server refuses to start on an older schema without a backup the same way. *Writer.* | whoever holds the database's credentials (in the audit under `--user`) | `ohl network migrate --user ana` |
| `setting KEY [VALUE]` | Shows or sets a network setting: `backup_max_age_hours` (more than 0, at most 8 760). In the audit. | network admins (to set) | `ohl network setting backup_max_age_hours 48 --user ana` |

## Rights (`ohl network rights …`)

| Command | What it does | Who may | Example |
|---|---|---|---|
| `list` | Roles, users, groups, assignments (numbered). | anyone reaching the database | `ohl network rights list` |
| `bootstrap` | Makes the first network admin. | only while the network has none | `ohl network rights bootstrap --user ana` |
| `add-user NAME [--display TEXT]`, `disable NAME`, `enable NAME` | Users of the network (a disabled user has no rights; their connections are let go; the last network admin cannot be disabled). | network admins | `ohl network rights add-user ben --user ana` |
| `add-group NAME [--note TEXT]`, `remove-group NAME`, `add-member GROUP USER`, `remove-member GROUP USER` | Groups and their members (a group removed takes its assignments). | network admins | `ohl network rights add-member crew ben --user ana` |
| `put-role NAME --permissions LIST [--note TEXT]`, `remove-role NAME` | A role of one's own (the six shipped ones are fixed; a role in use cannot be removed). Permissions: `see`, `edit`, `create`, `delete`, `review`, `export`, `lock`, `request`, `manage_rights`. | network admins | `ohl network rights put-role surveyor --permissions see,export --user ana` |
| `assign --to user:NAME\|group:NAME [--role ROLE] [--deny] [scope] [--note TEXT]` | Gives a role on a scope; `--deny` denies it (every permission without `--role`). Scope flags, all that are given must hold: `--line CODE`, `--route ID` (`--from-structure A --to-structure B`: the part between, snapped out to whole tension sections), `--region "x,y;x,y;…"`, `--kind KIND` (`meta`: the settings, `library`: every library file), `--object ID`, `--library-kind`, `--library-file`, `--library-entry`. | whoever manages rights on a scope holding it and holds every permission of the role there; a deny: network admins | `ohl network rights assign --to user:ben --role designer --route R7 --from-structure T3 --to-structure T9 --user ana` |
| `unassign ID` | Takes an assignment away. | as `assign` | `ohl network rights unassign 12 --user ana` |
| `explain --as USER --permission P [--line CODE [--kind K --id ID]]` | Whether a user may do something on a target, and the assignments granting and denying it (no `--line`: the network; `--line` alone: the line whole; `--kind library --id KIND/FILE`: a library file). In the audit. | whoever manages rights somewhere | `ohl network rights explain --as ben --permission edit --line L001 --kind structure --id T5 --user ana` |
| `see-as --as USER [--line CODE]` | What a user may see of each line, as the server decides it (within what the asker may see). In the audit. | whoever manages rights somewhere | `ohl network rights see-as --as ben --user ana` |
| `audit [--after N] [--limit N]` | The audit: rights changes (what was and is), refusals, imports, exports, library changes, backups, restores, migrations, settings. | network admins | `ohl network rights audit --user ana` |
| `reissue-invitation NAME` | Phase 13: makes a user's personal invitation again: every older one of theirs is refused from now on (they are let go if connected); the server prints the new one (`serve --invite NAME`). | network admins | `ohl network rights reissue-invitation ben --user ana` |
| `map-identity --issuer URL --subject SUB --to NAME`, `unmap-identity --issuer URL --subject SUB`, `identities` | Phase 13: an identity provider's subject (the ID token's `sub`; the issuer exactly as `serve --oidc-issuer`) signs in as a network user, or no longer; `identities` lists them. | network admins | `ohl network rights map-identity --issuer https://id.example.org/realms/grid --subject 00u1ab --to ben --user ana` |

Rights changes need no writer: a running server reads them again before
its next job (at the latest with its next heartbeat) and lets go every
client whose rights changed.

## Country libraries (`ohl network library …`)

| Command | What it does | Who may | Example |
|---|---|---|---|
| `kinds`, `files KIND` | The kinds you may see (locked, files, releases, maintainers); a kind's files as they are now, with their entries (version, first release, withdrawn). | what you may see | `ohl network library files cables --user ben` |
| `lock KIND [--file F] [--unlock] [--note TEXT]` | Locks a kind or one file for the whole network: then it changes only by requests granted and releases. | *lock* on the kind; unlocking: network admins | `ohl network library lock cables --user mia` |
| `put KIND PATH [--name N] [--emergency]`, `delete KIND NAME [--emergency]` | A file of the network's library (its releases keep a removed one). | *create*/*edit*/*delete* on it; a locked kind only with `--emergency`, network admins (in the audit) | `ohl network library put weather north.ohlwc --user dan` |
| `request KIND PATH --into FILE [--entry E] [--reason TEXT] [--line CODE --branch B] [--route R]… [--draft]`, `submit ID`, `withdraw-request ID`, `requests` | Asks for an entry (an addition; a maintainer may ask for a change); its draft may be used, as `request-N.ext`, in the requester's own branch of the line — or, since phase 11, a branch over a working set whose maker named them in its team — and blocks that branch's review and merge into main until granted or taken out. `requests`: yours and those you maintain. | *request* on the line's library (a designer of the line) | `ohl network library request cables new-wire.ohlw --into acsr.ohlw --line L001 --branch my-branch --user ben` |
| `grant ID [--note]`, `refuse ID [--note]` | Decides a request. | a maintainer (*review* and *lock* on the kind) other than the requester | `ohl network library grant 12 --user mia` |
| `release KIND TAG [--notes]` | Freezes the kind as it is now under a tag. No line changes. | a maintainer | `ohl network library release cables 2026-11 --user mia` |
| `withdraw KIND FILE ENTRY [--note]`, `restore KIND FILE ENTRY` | An entry withdrawn (kept by who uses it, never newly used) or brought back. | a maintainer | `ohl network library withdraw cables acsr.ohlw old-wire --note "out of production" --user mia` |
| `built LINE [--design]` | A line built (pinned) or in design again (following the latest). | *lock* on the whole line | `ohl network library built L001 --user ana` |
| `status [--line CODE] [--json]` | The lines you may see with their pins, the latest releases, the withdrawn entries they use. | what you may see | `ohl network library status --user ben` |
| `uses KIND ENTRY` | The use index: where an entry is used — main and, since phase 11, the branches that changed what uses it — only what you may see (a hidden entry is "not found"). | what you may see | `ohl network library uses cables acsr-drake-26-7 --user mia` |
| `update-pins [--line CODE \| --following] [--kind K]… [--release R] [--dry-run]` | Moves a line's library files to the releases as one logged command; the routes whose results change marked out of date; not undone by Undo. *Writer* (a running server does it when a client asks). | *create*/*edit*/*delete* of each file changed; a built line: *review* on the whole line | `ohl network library update-pins --following --user ana` |
| `reindex` | Makes the use index again from every line. *Writer.* | anyone reaching the database as the writer | `ohl network library reindex` |

## Measurements (each but `bench-scoped` edits the network: use a database made for it)

| Command | What it measures | Example |
|---|---|---|
| `bench --work DIR [--line] [--viewers N] [--view-w N --view-h N] [--moves N] [--areas N]` | Phase 5's targets: start, memory, joins, tile packages, a move applied and broadcast, area queries. | `ohl network bench --user ana --work /tmp/b` |
| `bench-working-set --work DIR [--line] [--moves N] [--idle-wait-s S]` | Phase 6: working sets shown and loaded, rows, joining again, a route added, context tiles, the idle solve. | `ohl network bench-working-set --user ana --work /tmp/w` |
| `bench-rights [--users N] [--assignments N] [--moves N]` | Phase 7: the rights' cost with many users and assignments. | `ohl network bench-rights --user ana` |
| `bench-library [--queries N]` | Phase 9: releases, Update library pins of every line, the use index. *Writer.* | `ohl network bench-library --user ana` |
| `bench-versions [--line CODE]… [--rounds N] [--trees split,flat]` | Phase 11: a commit of one changed route, a branch over a working set (diff, ahead and behind, merge), a merge at a shared tower — split and flat trees on the same model. *Writer.* | `ohl network bench-versions --user ana --line L001` |
| `bench-offline --work DIR [--line CODE] [--rounds N] [--during-reach M]` | Phase 12: a whole route's offline working set exported (size and time), bundles of 20 and 500 changes made and brought back (each onto a branch). Phase 14, with `--during-reach`: then a move's latency (another user, `bench-mover`) while the route is exported with that reach, written on the authority and on the exports' thread; since 2026-11-20 also while a bundle of 20 changes of that export is brought back (on the authority, and its branch's line made ready on the exports' thread). Hosts the network in this process. *Writer.* | `ohl network bench-offline --user ana --work /tmp/o --during-reach 10000` |
| `bench-scale --work DIR [--editors N] [--viewers N] [--view-w N --view-h N] [--moves N]` | Phase 14, end to end: `ohl network serve` as a process of its own; editors (each a designer of one route) and viewers join at once with views of their own; every editor moves structures of its route at once while two whole routes' clearances and two offline working sets are asked; the joins', moves' and long work's times, the memory. Adds users `scale-…` (made once). `OHL_BENCH_SERVE_EXE` measures another program's server with these clients. | `ohl network bench-scale --user ana --work /tmp/s` |
| `bench-scoped [--runs N]` | Phase 10: checks on a working set with their neighbours pulled in, computed by the server in the bench's process — a whole route's clearances between wires, its magnetic field with the lines within 500 m, the loads on a tower shared with another line; each `--runs` times. Changes nothing (a user who sees the whole network). | `ohl network bench-scoped --user ana` |

The numbers: [performance](../development/performance.md).

## The synthetic country (`ohl synth …`)

| Command | What it does | Example |
|---|---|---|
| `country --out DIR [--pylons N] [--systems X] [--seed N] [--section-spans X] [--branching X] [--lidar-density X] [--corridor M] [--json] [--user U] [--seconds S] [--per-line]` | A deterministic invented country (12 000 pylons, 2.5 systems each by default): `country.ohl` (every route in one project), `summary.json`, with `--per-line` each route as `lines/CODE.ohl` for `ohl network import` (towers shared by two routes in both files). | `ohl synth country --per-line --lidar-density 0 --out /tmp/country` |
| `measure PROJECT [--edits N] [--user U] [--seconds S] [--whole] [--working-set] [--shared-towers]` | Times the session on a project file: open, memory, whole validation, edits, undo, redo (with `--working-set`, towers shared with routes outside it). | `ohl synth measure /tmp/country/country.ohl` |

## Not there yet

No `ohl network init` (the first connection, or `migrate`, makes the
schema), no WAL archiving or point-in-time recovery (the backups are
`pg_dump`'s; PostgreSQL's own base backups and WAL archiving work beside
them), no sign-in flow in OHL-Design or the web client yet (the protocol
takes an ID token: `id_token` in the join), no command for branches over a working set
(the server's operation `create_working_set_branch`; OHL-Design's window
to come; `import-bundle` makes one), no OHL-Design menu for offline
working sets (the commands above and the server's messages
`export_working_set`, `import_bundle`), no command for the checks with their
neighbours (phase 10: the server's messages `check`, answered with
`check_result` and later, once, `check_stale` —
[the server's messages](../development/data-model.md#the-servers-messages-protocol-3);
OHL-Design's check windows asking for them to come).
