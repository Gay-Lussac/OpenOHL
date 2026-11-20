// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Backups, restores and the restore drill (ADR-0013, phase 13).
//!
//! **A backup** (`ohl network backup`, [`backup`]) is a folder of two
//! files: `network.dump`, PostgreSQL's own dump (`pg_dump`, custom
//! format: every table, PostGIS's extension named, restorable by
//! `pg_restore` alone), and `manifest.json` ([`Manifest`]): the network's
//! schema, the time of the data, the dump's SHA-256 and size, and **a
//! hash of each model and one of the network's own tables**, taken in the
//! very snapshot `pg_dump` reads (`pg_export_snapshot`, `--snapshot`): a
//! backup made while the server runs is consistent, and its hashes say
//! exactly what it holds. The backup is recorded in the database
//! (`net_meta` `last_backup`, the `net_backup` table, the audit): a
//! schema migration asks for one newer than the network's
//! `backup_max_age_hours` (default 24, [`recent_backup`]).
//!
//! **A restore** (`ohl network restore`, [`restore`]) goes into an
//! **empty** database only (PostGIS may be there), refuses a backup of
//! another schema than this program's and a dump whose hash is not the
//! manifest's, runs `pg_restore` in one transaction (nothing half
//! restored), then reads every model and the network's tables again and
//! compares their hashes with the manifest's — **the restore drill**: a
//! restore that does not give back exactly what was backed up says so.
//!
//! A model's hash ([`model_dump`]) is over its rows in every table that
//! has a `line_id` (each row as PostgreSQL's JSON of it, in the order of
//! the table's key, times in UTC); the network's ([`network_dump`]) over
//! every other table of the schema (the models' rows of `line`, users,
//! rights, libraries, blobs, the audit). Table names come from the
//! catalogue (quoted by PostgreSQL); nothing a user typed goes into SQL.
//!
//! The tools are PostgreSQL's (`pg_dump`, `pg_restore` of the server's
//! major version or newer): `OHL_PG_BIN`, else Homebrew's
//! `postgresql@17`, else the `PATH`. The connection's password and TLS
//! settings go to them in their environment, never on their command
//! line.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use postgres::fallible_iterator::FallibleIterator;
use postgres::types::ToSql;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::connect::Target;
use crate::{NetError, Network, lock, schema};

/// The manifest's file in a backup folder.
pub const MANIFEST: &str = "manifest.json";

/// The dump's file in a backup folder.
pub const DUMP: &str = "network.dump";

/// The manifest's `format`.
pub const FORMAT: &str = "ohl-network-backup";

/// The age of the last backup a migration accepts by default, hours
/// (`net_meta` `backup_max_age_hours`).
pub const DEFAULT_MAX_AGE_HOURS: f64 = 24.0;

/// The settings an admin may change (`ohl network setting`): key, what it
/// is, its check.
pub const SETTINGS: &[(&str, &str)] = &[(
    "backup_max_age_hours",
    "the age of the last backup a schema migration accepts, hours (more than 0, at most 8760)",
)];

/// A hash of rows (a model's, or the network's own tables').
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hashed {
    /// The model's code (`*` for the network's own tables).
    pub name: String,
    /// SHA-256 of its rows, hexadecimal.
    pub sha256: String,
    /// Its rows.
    pub rows: u64,
}

/// What a backup holds ([`MANIFEST`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// [`FORMAT`].
    pub format: String,
    /// This manifest's version (1).
    pub version: u32,
    /// The network's schema.
    pub schema: i32,
    /// When the data was read (UTC, the snapshot's time).
    pub time: String,
    /// The database's name.
    pub database: String,
    /// Who made it.
    pub user: String,
    /// The dump's file.
    pub file: String,
    /// SHA-256 of the dump, hexadecimal.
    pub sha256: String,
    /// The dump's size, bytes.
    pub bytes: u64,
    /// `pg_dump --version`.
    pub pg_dump: String,
    /// The program that made it (`ohl` and its version).
    pub program: String,
    /// Each model's hash, by code.
    pub models: Vec<Hashed>,
    /// The network's own tables' hash.
    pub network: Hashed,
}

/// What a backup did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupReport {
    /// The folder.
    pub dir: PathBuf,
    /// The manifest written.
    pub manifest: Manifest,
    /// Seconds in all, of them `pg_dump`'s and the hashes'.
    pub seconds: f64,
    /// See `seconds`.
    pub dump_seconds: f64,
    /// See `seconds`.
    pub hash_seconds: f64,
}

/// What a restore did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreReport {
    /// The backup's manifest.
    pub manifest: Manifest,
    /// Models found identical to the backup's (all of them, else the
    /// restore failed).
    pub models: usize,
    /// Seconds in all, of them `pg_restore`'s and the drill's.
    pub seconds: f64,
    /// See `seconds`.
    pub restore_seconds: f64,
    /// See `seconds`.
    pub verify_seconds: f64,
}

/// Whether a backup recent enough for a migration is recorded.
pub(crate) enum Recent {
    /// Yes: its time in words.
    Yes(String),
    /// No: why.
    No(String),
}

/// Reads the last backup recorded and the network's setting (in the
/// migration's transaction: `net_meta`, which every schema has).
pub(crate) fn recent_backup(tx: &mut postgres::Transaction<'_>) -> Result<Recent, NetError> {
    let max = tx
        .query_opt(
            "SELECT value FROM net_meta WHERE key = 'backup_max_age_hours'",
            &[],
        )?
        .and_then(|r| r.get::<_, String>(0).parse::<f64>().ok())
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or(DEFAULT_MAX_AGE_HOURS);
    let Some(last) = tx.query_opt("SELECT value FROM net_meta WHERE key = 'last_backup'", &[])?
    else {
        return Ok(Recent::No("no backup is recorded".into()));
    };
    let last: String = last.get(0);
    let row = tx.query_one(
        "SELECT (extract(epoch FROM now())::float8 - (($1::text)::json->>'epoch')::float8) / 3600.0,
                ($1::text)::json->>'time'",
        &[&last],
    )?;
    let (age, time): (f64, Option<String>) = (row.get(0), row.get(1));
    let time = time.unwrap_or_default();
    Ok(if age <= max {
        Recent::Yes(format!("{time} ({age:.1} h before)"))
    } else {
        Recent::No(format!(
            "the last backup ({time}) is {age:.1} h old, more than the network's \
             backup_max_age_hours ({max})"
        ))
    })
}

/// PostgreSQL's tool `name` (`pg_dump`, `pg_restore`): in `OHL_PG_BIN`,
/// Homebrew's `postgresql@17`, or the `PATH`.
pub fn pg_tool(name: &str) -> Result<PathBuf, NetError> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("OHL_PG_BIN") {
        dirs.push(PathBuf::from(d));
    }
    dirs.push(PathBuf::from("/opt/homebrew/opt/postgresql@17/bin"));
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter()
        .map(|d| d.join(name))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            NetError::Refused(format!(
                "{name} not found (PostgreSQL's tools): set OHL_PG_BIN to their folder"
            ))
        })
}

/// A tool run with the connection's environment; its error output kept.
fn run_tool(mut cmd: Command, target: &Target, what: &str) -> Result<(), NetError> {
    for (k, v) in target.libpq_env()? {
        cmd.env(k, v);
    }
    let out = cmd
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| NetError::Refused(format!("{what}: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = err.lines().take(12).collect();
        Err(NetError::Database(format!(
            "{what} failed: {}",
            lines.join(" | ")
        )))
    }
}

/// A sink hashing what is written, counting rows.
struct Hasher {
    hash: sha2::Sha256,
    rows: u64,
}

impl Hasher {
    fn new() -> Self {
        Self {
            hash: sha2::Sha256::new(),
            rows: 0,
        }
    }

    fn done(self, name: &str) -> Hashed {
        Hashed {
            name: name.to_owned(),
            sha256: hex(&self.hash.finalize()),
            rows: self.rows,
        }
    }
}

impl Write for Hasher {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hash.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file.
fn file_hash(path: &Path) -> Result<(String, u64), NetError> {
    let mut f = std::fs::File::open(path)
        .map_err(|e| NetError::Refused(format!("{}: {e}", path.display())))?;
    let mut h = sha2::Sha256::new();
    let n = std::io::copy(&mut f, &mut h)
        .map_err(|e| NetError::Refused(format!("{}: {e}", path.display())))?;
    Ok((hex(&h.finalize()), n))
}

/// A table of the schema: its name quoted (by PostgreSQL), whether it
/// has a `line_id`, its key's columns quoted (empty: none).
struct Table {
    name: String,
    quoted: String,
    by_line: bool,
    key: String,
}

/// The schema's tables (not PostGIS's), by name.
fn tables(c: &mut crate::Conn) -> Result<Vec<Table>, NetError> {
    Ok(c.query(
        "SELECT c.relname::text, quote_ident(c.relname),
             EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = c.oid
                 AND a.attname = 'line_id' AND NOT a.attisdropped),
             COALESCE((SELECT string_agg(quote_ident(a.attname), ', ' ORDER BY k.n)
                 FROM pg_index i
                 CROSS JOIN LATERAL unnest(i.indkey::int2[]) WITH ORDINALITY AS k(attnum, n)
                 JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
                 WHERE i.indrelid = c.oid AND i.indisprimary), '')
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public' AND c.relkind = 'r'
           AND NOT EXISTS (SELECT 1 FROM pg_depend d
               WHERE d.classid = 'pg_class'::regclass AND d.objid = c.oid AND d.deptype = 'e')
         ORDER BY 1",
        &[],
    )?
    .into_iter()
    .map(|r| Table {
        name: r.get(0),
        quoted: r.get(1),
        by_line: r.get(2),
        key: r.get(3),
    })
    .collect())
}

/// Writes the rows of `sql` (one text column) to `out`, one per line.
fn rows_to(
    c: &mut crate::Conn,
    sql: &str,
    params: &[&(dyn ToSql + Sync)],
    out: &mut dyn Write,
) -> Result<u64, NetError> {
    let mut it = c.client.query_raw(sql, params.iter().copied())?;
    let mut n = 0;
    while let Some(row) = it.next()? {
        let text: String = row.get(0);
        out.write_all(text.as_bytes())
            .and_then(|()| out.write_all(b"\n"))
            .map_err(|e| NetError::Database(e.to_string()))?;
        n += 1;
    }
    Ok(n)
}

/// A table's rows' statement: each row's JSON, in its key's order.
fn select(t: &Table, by_line: bool) -> String {
    let order = if t.key.is_empty() {
        "1".to_owned()
    } else {
        t.key.clone()
    };
    let filter = if by_line { " WHERE line_id = $1" } else { "" };
    format!(
        "SELECT row_to_json(t)::text FROM public.{} t{filter} ORDER BY {order}",
        t.quoted
    )
}

/// Every row of model `code` (each table with a `line_id`, by name), as
/// the hashes read them: the table's name, then its rows' JSON one per
/// line. Read in the connection's transaction, if any (times in UTC).
pub fn model_dump(net: &Network, code: &str, out: &mut dyn Write) -> Result<u64, NetError> {
    let mut c = lock(&net.conn);
    let tables = tables(&mut c)?;
    dump_model(&mut c, &tables, code, out)
}

fn dump_model(
    c: &mut crate::Conn,
    tables: &[Table],
    code: &str,
    out: &mut dyn Write,
) -> Result<u64, NetError> {
    let mut n = 0;
    for t in tables.iter().filter(|t| t.by_line) {
        writeln!(out, "table {}", t.name).map_err(|e| NetError::Database(e.to_string()))?;
        n += rows_to(c, &select(t, true), &[&code], out)?;
    }
    Ok(n)
}

/// Every row of the network's own tables (those without a `line_id`, by
/// name), as [`model_dump`] writes a model's.
pub fn network_dump(net: &Network, out: &mut dyn Write) -> Result<u64, NetError> {
    let mut c = lock(&net.conn);
    let tables = tables(&mut c)?;
    dump_network(&mut c, &tables, out)
}

fn dump_network(
    c: &mut crate::Conn,
    tables: &[Table],
    out: &mut dyn Write,
) -> Result<u64, NetError> {
    let mut n = 0;
    for t in tables.iter().filter(|t| !t.by_line) {
        writeln!(out, "table {}", t.name).map_err(|e| NetError::Database(e.to_string()))?;
        n += rows_to(c, &select(t, false), &[], out)?;
    }
    Ok(n)
}

/// Each model's hash and the network's, read on `c` (in its snapshot).
fn hashes(c: &mut crate::Conn) -> Result<(Vec<Hashed>, Hashed), NetError> {
    let tables = tables(c)?;
    let codes: Vec<String> = c
        .query("SELECT id FROM line ORDER BY id", &[])?
        .into_iter()
        .map(|r| r.get(0))
        .collect();
    let mut models = Vec::with_capacity(codes.len());
    for code in codes {
        let mut h = Hasher::new();
        h.rows = dump_model(c, &tables, &code, &mut h)?;
        models.push(h.done(&code));
    }
    let mut h = Hasher::new();
    h.rows = dump_network(c, &tables, &mut h)?;
    Ok((models, h.done("*")))
}

/// Network admins only, when the network has any (who holds the
/// database's credentials can do this anyway; the audit names them).
fn admins_only(net: &Network, user: &str, what: &str) -> Result<(), NetError> {
    if net.schema_as_is()? < 4 {
        return Ok(());
    }
    let admins = net.rights()?.network_admins();
    if admins.is_empty() || admins.contains(&user.to_owned()) {
        Ok(())
    } else {
        let why = format!("{user} may not {what}: network admins only");
        let _ = net.audit(user, "refused", what, &why);
        Err(NetError::Refused(why))
    }
}

/// A new folder only its owner may enter (or an empty one).
fn new_dir(dir: &Path) -> Result<(), NetError> {
    let bad = |e: std::io::Error| NetError::Refused(format!("{}: {e}", dir.display()));
    if dir.exists() {
        if std::fs::read_dir(dir).map_err(bad)?.next().is_some() {
            return Err(NetError::Refused(format!(
                "{}: not empty (a backup goes into a new folder)",
                dir.display()
            )));
        }
        return Ok(());
    }
    let mut b = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir).map_err(bad)
}

/// Backs the network at `url` up into the new (or empty) folder `dir`, as
/// `user` (a network admin when the network has one): `pg_dump` in a
/// snapshot this connection exported, the hashes read in it, the manifest
/// written, the backup recorded. Runs beside the server (nothing waits).
pub fn backup(url: &str, user: &str, dir: &Path) -> Result<BackupReport, NetError> {
    let started = Instant::now();
    let target = Target::parse(url)?;
    let pg_dump = pg_tool("pg_dump")?;
    let net = Network::connect_as_is(url)?;
    let schema = net.schema_as_is()?;
    if schema == 0 {
        return Err(NetError::Refused("no network in this database".into()));
    }
    admins_only(&net, user, "back the network up")?;
    new_dir(dir)?;
    let dump = dir.join(DUMP);
    let version = Command::new(&pg_dump)
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    // One snapshot: pg_dump's and the hashes'.
    let (epoch, time, database, models, network, dump_s, hash_s) = {
        let mut c = lock(&net.conn);
        c.client.batch_execute(
            "BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY;
             SET LOCAL TimeZone = 'UTC'; SET LOCAL extra_float_digits = 1",
        )?;
        let out = (|| -> Result<_, NetError> {
            let row = c.query_one(
                "SELECT pg_export_snapshot(), extract(epoch FROM now())::float8, ohl_time(now()),
                     current_database()::text",
                &[],
            )?;
            let (snapshot, epoch, time, database): (String, f64, String, String) =
                (row.get(0), row.get(1), row.get(2), row.get(3));
            let t = Instant::now();
            let mut cmd = Command::new(&pg_dump);
            cmd.args(["--format=custom", "--no-password"])
                .arg(format!("--snapshot={snapshot}"))
                .arg(format!("--file={}", dump.display()))
                .arg(format!("--dbname={}", target.libpq_string()));
            run_tool(cmd, &target, "pg_dump")?;
            let dump_s = t.elapsed().as_secs_f64();
            let t = Instant::now();
            let (models, network) = hashes(&mut c)?;
            Ok((
                epoch,
                time,
                database,
                models,
                network,
                dump_s,
                t.elapsed().as_secs_f64(),
            ))
        })();
        let _ = c.client.batch_execute("COMMIT");
        match out {
            Ok(x) => x,
            Err(e) => {
                let _ = std::fs::remove_file(&dump);
                return Err(e);
            }
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dump, std::fs::Permissions::from_mode(0o600));
    }
    let (sha256, bytes) = file_hash(&dump)?;
    let manifest = Manifest {
        format: FORMAT.into(),
        version: 1,
        schema,
        time: time.clone(),
        database,
        user: user.to_owned(),
        file: DUMP.into(),
        sha256,
        bytes,
        pg_dump: version,
        program: format!("ohl {}", env!("CARGO_PKG_VERSION")),
        models,
        network,
    };
    let text = serde_json::to_string_pretty(&manifest).expect("serialises");
    std::fs::write(dir.join(MANIFEST), text + "\n")
        .map_err(|e| NetError::Refused(format!("{}: {e}", dir.display())))?;
    let seconds = started.elapsed().as_secs_f64();
    record(&net, "backup", user, &manifest, epoch, seconds)?;
    Ok(BackupReport {
        dir: dir.to_path_buf(),
        manifest,
        seconds,
        dump_seconds: dump_s,
        hash_seconds: hash_s,
    })
}

/// A backup or a restore recorded: `net_backup` (schema 7 on), the audit,
/// and for a backup `net_meta` `last_backup` (read by a migration, in
/// every schema).
fn record(
    net: &Network,
    kind: &str,
    user: &str,
    m: &Manifest,
    epoch: f64,
    seconds: f64,
) -> Result<(), NetError> {
    let models = m.models.len() as i32;
    let bytes = m.bytes as i64;
    let mut c = lock(&net.conn);
    if kind == "backup" {
        c.execute(
            "INSERT INTO net_meta (key, value) VALUES ('last_backup',
                 json_build_object('epoch', $1::float8, 'time', $2::text, 'schema', $3::int,
                     'sha256', $4::text, 'user', $5::text)::text)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            &[&epoch, &m.time, &m.schema, &m.sha256, &user],
        )?;
    }
    let has_table: bool = c
        .query_one("SELECT to_regclass('public.net_backup') IS NOT NULL", &[])?
        .get(0);
    if has_table {
        c.execute(
            "INSERT INTO net_backup (kind, user_name, schema, sha256, bytes, models, seconds, note)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &kind,
                &user,
                &m.schema,
                &m.sha256,
                &bytes,
                &models,
                &seconds,
                &format!("data of {}", m.time),
            ],
        )?;
    }
    drop(c);
    net.audit(
        user,
        kind,
        "network",
        &serde_json::json!({
            "time": m.time, "schema": m.schema, "sha256": m.sha256, "bytes": m.bytes,
            "models": m.models.len(), "seconds": seconds,
        })
        .to_string(),
    )
}

/// A backup's manifest, checked to be one.
pub fn read_manifest(dir: &Path) -> Result<Manifest, NetError> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| NetError::Refused(format!("{}: {e}", path.display())))?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| {
        NetError::Refused(format!("{}: not a backup's manifest: {e}", path.display()))
    })?;
    if m.format != FORMAT || m.version != 1 {
        return Err(NetError::Refused(format!(
            "{}: not a network backup's manifest ({} {})",
            path.display(),
            m.format,
            m.version
        )));
    }
    // The dump's file is the folder's own (never a path elsewhere).
    if m.file != DUMP {
        return Err(NetError::Refused(format!(
            "{}: the dump must be {DUMP}, not \"{}\"",
            path.display(),
            m.file
        )));
    }
    Ok(m)
}

/// Whether the database is empty: no table of its own (PostGIS's
/// allowed) and no network.
fn is_empty(net: &Network) -> Result<bool, NetError> {
    let n: i64 = lock(&net.conn)
        .query_one(
            "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
               AND n.nspname NOT LIKE 'pg_temp%' AND n.nspname NOT LIKE 'pg_toast_temp%'
               AND c.relkind IN ('r', 'p', 'v', 'm', 'S', 'f')
               AND NOT EXISTS (SELECT 1 FROM pg_depend d
                   WHERE d.classid = 'pg_class'::regclass AND d.objid = c.oid
                     AND d.deptype = 'e')",
            &[],
        )?
        .get(0);
    Ok(n == 0)
}

/// Restores the backup in `dir` into the empty database at `url`, as
/// `user` (in the restored network's audit): refused for a backup of
/// another schema than this program's, a dump changed since, a database
/// that is not empty; `pg_restore` in one transaction; then every model
/// and the network's tables compared with the manifest's hashes (the
/// restore drill) — a difference fails the restore, naming what differs.
pub fn restore(url: &str, user: &str, dir: &Path) -> Result<RestoreReport, NetError> {
    let started = Instant::now();
    let manifest = read_manifest(dir)?;
    if manifest.schema != schema::SCHEMA {
        return Err(NetError::Refused(format!(
            "the backup is of network schema {}, this program's is {}: restore it with the \
             program that made it ({}), then migrate",
            manifest.schema,
            schema::SCHEMA,
            manifest.program
        )));
    }
    let dump = dir.join(DUMP);
    let (sha256, bytes) = file_hash(&dump)?;
    if sha256 != manifest.sha256 || bytes != manifest.bytes {
        return Err(NetError::Refused(format!(
            "{}: not the dump the manifest names (changed or damaged: SHA-256 {sha256}, the \
             manifest's {})",
            dump.display(),
            manifest.sha256
        )));
    }
    let target = Target::parse(url)?;
    let pg_restore = pg_tool("pg_restore")?;
    {
        let net = Network::connect_as_is(url)?;
        if !is_empty(&net)? {
            return Err(NetError::Refused(
                "the database is not empty: a backup is restored into a new, empty database \
                 (PostGIS may be in it)"
                    .into(),
            ));
        }
    }
    let t = Instant::now();
    let mut cmd = Command::new(&pg_restore);
    cmd.args([
        "--exit-on-error",
        "--single-transaction",
        "--no-owner",
        "--no-privileges",
        "--no-password",
    ])
    .arg(format!("--dbname={}", target.libpq_string()))
    .arg(&dump);
    run_tool(cmd, &target, "pg_restore")?;
    let restore_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let net = Network::connect_as_is(url)?;
    let schema = net.schema_as_is()?;
    let (models, network) = {
        let mut c = lock(&net.conn);
        c.client.batch_execute(
            "BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY;
             SET LOCAL TimeZone = 'UTC'; SET LOCAL extra_float_digits = 1",
        )?;
        let out = hashes(&mut c);
        let _ = c.client.batch_execute("COMMIT");
        out?
    };
    let verify_s = t.elapsed().as_secs_f64();
    let mut differ = Vec::new();
    if schema != manifest.schema {
        differ.push(format!("schema {schema}, the backup's {}", manifest.schema));
    }
    if network != manifest.network {
        differ.push("the network's own tables".to_owned());
    }
    let want: std::collections::BTreeMap<&str, &Hashed> = manifest
        .models
        .iter()
        .map(|h| (h.name.as_str(), h))
        .collect();
    let got: std::collections::BTreeMap<&str, &Hashed> =
        models.iter().map(|h| (h.name.as_str(), h)).collect();
    for (name, h) in &want {
        match got.get(name) {
            Some(g) if g == h => {}
            Some(_) => differ.push(format!("line {name}")),
            None => differ.push(format!("line {name} missing")),
        }
    }
    for name in got.keys().filter(|n| !want.contains_key(*n)) {
        differ.push(format!("line {name} not in the backup"));
    }
    if !differ.is_empty() {
        return Err(NetError::Database(format!(
            "the restore drill failed — the database restored differs from the backup in: {} \
             (the database is left as restored, for a look)",
            differ.join(", ")
        )));
    }
    let seconds = started.elapsed().as_secs_f64();
    let epoch = 0.0;
    record(&net, "restore", user, &manifest, epoch, seconds)?;
    Ok(RestoreReport {
        models: models.len(),
        manifest,
        seconds,
        restore_seconds: restore_s,
        verify_seconds: verify_s,
    })
}

impl Network {
    /// Network setting `key` ([`SETTINGS`]; none: its default).
    pub fn setting(&self, key: &str) -> Result<Option<String>, NetError> {
        if !SETTINGS.iter().any(|(k, _)| *k == key) {
            return Err(NetError::Refused(format!(
                "no setting {key} (settings: {})",
                SETTINGS
                    .iter()
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        self.meta(key)
    }

    /// Sets network setting `key` to `value`, as `user` (network admins;
    /// in the audit). Needs no writer: it is read when it is used.
    pub fn set_setting(&self, user: &str, key: &str, value: &str) -> Result<(), NetError> {
        let before = self.setting(key)?;
        let ok = match key {
            "backup_max_age_hours" => value
                .parse::<f64>()
                .is_ok_and(|h| h.is_finite() && h > 0.0 && h <= 8760.0),
            _ => false,
        };
        if !ok {
            let what = SETTINGS
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, w)| *w)
                .unwrap_or_default();
            return Err(NetError::Refused(format!("{key} = {value}: {what}")));
        }
        admins_only(self, user, &format!("change setting {key}"))?;
        lock(&self.conn).execute(
            "INSERT INTO net_meta (key, value) VALUES ($1, $2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            &[&key, &value],
        )?;
        self.audit(
            user,
            "setting",
            key,
            &format!("{} → {value}", before.as_deref().unwrap_or("(default)")),
        )
    }

    /// The last backup recorded (`net_meta` `last_backup`: time, schema,
    /// hash, user, as JSON), if any.
    pub fn last_backup(&self) -> Result<Option<serde_json::Value>, NetError> {
        Ok(self
            .meta("last_backup")?
            .and_then(|t| serde_json::from_str(&t).ok()))
    }
}
