# Deploying a network's server

*Built 2026-11-20 (ADR-0013, phase 13), in active development, not
approved yet.* How a country server — PostgreSQL with PostGIS, the
network's server (`ohl network serve`) and its backups — is run for real
use. **Nothing here is needed for the desktop applications**, nor for a
project file's own server (File › Host this project).

Two ways, the owner's IT decides:

- **Docker Compose** with the files in
  [`deploy/compose/`](https://github.com/Gay-Lussac/OpenOHL/tree/main/deploy/compose)
  (below);
- **plain programs**: PostgreSQL 17 with PostGIS 3 installed as the IT
  department runs databases, and `ohl` run as a service (systemd or the
  like) with the same commands and settings as the Compose file's.

## With Docker Compose

The files: `compose.yaml` (three services), `Dockerfile` (the `ohl`
program on PostgreSQL 17's image, for `pg_dump` and `pg_restore`),
`initdb/20-ohl.sh` (the network's database and its role), `backup-loop.sh`
(the backup job) and `secrets.example/` (**fake** secrets: copy and
replace them).

| Service | What it is | Health |
|---|---|---|
| `db` | PostgreSQL 17 with PostGIS 3.5. The network's database `network` belongs to the role `ohl` — no superuser, no new databases or roles; PostGIS is made once by the superuser. Its port is not published: only the server and the backup job reach it. Volume `db-data`. | `pg_isready` |
| `server` | `ohl network serve` on QUIC (UDP 4433, published). Its personal invitations' key in the volume `server-state` (`--keys`), its counters written every half second (`--stats`). | the counters' file newer than 2 minutes |
| `backup` | `ohl network backup` every 6 hours into the volume `backups` (`ohl-YYYYMMDD-HHMMSS/`), the newest 28 kept. Runs beside the server: nothing waits. | a backup newer than a day |

First start:

```sh
cd deploy/compose
cp -r secrets.example secrets          # then put real passwords in secrets/*.txt
docker compose build
docker compose up -d db
docker compose run --rm server network migrate --user admin       # makes the schema
docker compose run --rm server network rights bootstrap --user admin
docker compose run --rm -v /path/to/lines:/lines server \
    network import /lines/L001.ohl /lines/L002.ohl --user admin
docker compose up -d
docker compose logs server                                        # the invitations
```

Then edit the server's `command` in `compose.yaml`: `--advertise` with
the name users reach the server by, `--invite NAME` for the invitations
to print (`--invite-days N` to make them expire), and the OIDC settings
if the organisation signs in that way (below). `secrets/` is never
committed (`.gitignore`).

**Secrets.** Passwords are only in files given as Compose secrets
(`/run/secrets/…`): PostgreSQL's image reads `POSTGRES_PASSWORD_FILE`,
`ohl` reads `OHL_DATABASE_PASSWORD_FILE`. Never on a command line (every
user of the computer sees those) nor in `compose.yaml`. The invitations'
key (`server-key` in `server-state`) is a secret too: whoever holds it can
make any user's invitation.

**TLS to the database.** In the Compose file the database is on the
private network of the services (`OHL_DATABASE_SSLMODE: disable`, said
explicitly). A database on another host is reached with TLS only: give
`sslmode=verify-full sslrootcert=/run/secrets/db-ca.pem` (or
`OHL_DATABASE_SSLMODE=verify-full` and `OHL_DATABASE_CA`); without it
`ohl` refuses a host that is not its own computer.

## Sign-in with the organisation's accounts (OIDC)

An option; personal invitations stay the default and keep working.

1. Register the server as a client at the identity provider: its client
   id is the **audience** (`ohl-server` for example); the provider's
   **issuer** is the URL it names itself by
   (`https://id.example.org/realms/grid`).
2. Start the server with `--oidc-issuer ISSUER --oidc-audience ID`. The
   provider's keys come from its discovery document over HTTPS (its CA
   with `--oidc-ca FILE` if it is the organisation's own); without access
   to it, give them as a file (`--oidc-jwks-file`).
3. Map each person's subject (the ID token's `sub`) to a network user:
   `ohl network rights map-identity --issuer ISSUER --subject SUB --to NAME
   --user ADMIN`. An unmapped subject is refused.

The server checks each token's signature (asymmetric keys only), issuer,
audience and times (issued within the last hour, not expired). The
desktop and web sign-in windows are to come; the protocol takes the
token.

## Backups and restores

- **Backups**: `ohl network backup --out DIR --user ADMIN` (the backup
  job does it). A backup is a folder: `network.dump` (PostgreSQL's own
  format, restorable with `pg_restore` alone) and `manifest.json` (the
  schema, the time of the data, the dump's SHA-256, a hash of each line).
  **Copy the backups off the host** (another site): a volume on the same
  disk is not a backup.
- **Restore drill**: restore a backup into a new, empty database now and
  then — `createdb` it (PostGIS in it), then `ohl network restore DIR
  --database "…dbname=drill…" --user ADMIN`: it checks every line and the
  network's own tables against the manifest and fails, naming what
  differs, if anything does. The same command restores for real.
- **Upgrades**: an `ohl` of a newer network schema refuses to start on an
  older database unless a backup newer than `backup_max_age_hours` (24 by
  default; `ohl network setting backup_max_age_hours 48 --user ADMIN`)
  is recorded. Stop the server, make a backup (or check the job's last
  one), then `ohl network migrate --user ADMIN` and start the new server.
  `--i-have-a-backup` migrates without one recorded (a base backup made
  another way); it is in the audit.

PostgreSQL's own base backups and WAL archiving (point-in-time recovery)
work beside these; they are the IT department's.

## Running it without Docker

The same pieces: a PostgreSQL 17 cluster with PostGIS, a role without
superuser owning the network's database (as `initdb/20-ohl.sh`), `ohl
network serve` as a service with the environment of the Compose file
(`OHL_DATABASE_URL`, `OHL_DATABASE_PASSWORD_FILE`, TLS settings), and a
timer running `ohl network backup`.

[The commands](network-commands.md) ·
[the security review](../development/server-security-review.md) ·
[the database for developers](../development/postgresql.md)
