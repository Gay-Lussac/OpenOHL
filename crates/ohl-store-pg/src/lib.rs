// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A country server's database (ADR-0013, phase 4): every model of the
//! network in one PostgreSQL database with PostGIS.
//!
//! - [`Network`]: a connection to the database — its schema made or
//!   brought forward ([`schema`], numbered migrations; a newer database
//!   refused), the models it holds (`line` table: a model is a design
//!   line, as a project is today), the **writer's lock** (one authority per
//!   database: [`Network::connect_writer`] takes a PostgreSQL advisory lock
//!   and a second writer is refused), the size, the audit.
//! - [`PgStore`]: one model as an [`ohl_session::store::Store`] — a
//!   session opened on it ([`ohl_session::Session::open_on`]) works as on
//!   a project file: commands, undo, redo, branches, versions. Every step
//!   of the session is a transaction (savepoints nested in it).
//! - [`import`]: a project file into the network (`ohl network import`):
//!   the current state of its main project line as one log entry, ids
//!   clashing with other models renamed with the model's code, twin towers
//!   (the same tower in two projects) listed for an admin, a report.
//! - [`export`]: one model as a standalone project file (`.ohl`, today's
//!   format) with a few `network.*` meta keys (`ohl network export`).
//!
//! Ids are unique in the network: a unique index refuses an element of a
//! model's main branch with the id of another model's
//! ([`schema`]). Rights (users, groups, roles, assignments) are kept and
//! changed by [`rights`] (phase 7), evaluated by the session's one
//! function ([`ohl_session::rights`]); point clouds and backgrounds stay in the project files
//! for now (said in the import report).
//!
//! Only the server (and the admin's command line) talks to the database;
//! the connection string comes from the command line or the environment
//! (`OHL_DATABASE_URL`) — never from a project file.

use std::sync::{Arc, Mutex, MutexGuard};

use postgres::types::ToSql;
use postgres::{Client, NoTls, Row, Statement};

pub mod export;
pub mod import;
pub mod library;
pub mod offline;
pub mod rights;
pub mod schema;
mod store;
pub mod stream;
#[cfg(feature = "testing")]
pub mod testing;
pub mod working_sets;

pub use store::PgStore;

/// The advisory lock key of the network's one writer ("ohl-auth").
pub const WRITER_LOCK: i64 = 0x6f68_6c2d_6175_7468;

/// An error of the network's database.
#[derive(Debug, Clone, PartialEq)]
pub enum NetError {
    /// Refused (another writer, a newer schema, a project that cannot come
    /// in, …): the message says why.
    Refused(String),
    /// The database failed.
    Database(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(m) => write!(f, "refused: {m}"),
            Self::Database(m) => write!(f, "network database: {m}"),
        }
    }
}

impl std::error::Error for NetError {}

impl From<postgres::Error> for NetError {
    fn from(e: postgres::Error) -> Self {
        Self::Database(db_message(&e))
    }
}

impl From<ohl_session::SessionError> for NetError {
    fn from(e: ohl_session::SessionError) -> Self {
        match e {
            ohl_session::SessionError::Store(m) => Self::Database(m),
            other => Self::Refused(other.to_string()),
        }
    }
}

/// A database error in words (PostgreSQL's message and detail).
pub(crate) fn db_message(e: &postgres::Error) -> String {
    match e.as_db_error() {
        Some(d) => match d.detail() {
            Some(detail) => format!("{} ({detail})", d.message()),
            None => d.message().to_owned(),
        },
        None => e.to_string(),
    }
}

/// The connection shared by a network and its models' stores, with the
/// steps (savepoints) open on it and the statements prepared.
pub(crate) struct Conn {
    pub(crate) client: Client,
    pub(crate) steps: Vec<String>,
    prepared: std::collections::HashMap<&'static str, Statement>,
    /// Holds the writer's advisory lock ([`WRITER_LOCK`]).
    writer: bool,
}

impl Drop for Conn {
    /// The writer's lock released explicitly when the connection's last
    /// user (the network or one of its models' stores) goes: PostgreSQL
    /// would let it go only once its process for the connection has
    /// ended, a moment after the client closed it, so a writer started
    /// again at once (a server restarted) could be refused. An open
    /// transaction is rolled back first (as closing would); errors are
    /// ignored (a connection already broken loses its lock anyway).
    fn drop(&mut self) {
        if !self.writer || self.client.is_closed() {
            return;
        }
        if !self.steps.is_empty() {
            let _ = self.client.batch_execute("ROLLBACK");
        }
        let _ = self
            .client
            .execute("SELECT pg_advisory_unlock($1)", &[&WRITER_LOCK]);
    }
}

impl Conn {
    /// The statement `sql`, prepared once per connection.
    pub(crate) fn statement(&mut self, sql: &'static str) -> Result<Statement, postgres::Error> {
        if let Some(s) = self.prepared.get(sql) {
            return Ok(s.clone());
        }
        let s = self.client.prepare(sql)?;
        self.prepared.insert(sql, s.clone());
        Ok(s)
    }

    pub(crate) fn query(
        &mut self,
        sql: &'static str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, postgres::Error> {
        let s = self.statement(sql)?;
        self.client.query(&s, params)
    }

    pub(crate) fn query_opt(
        &mut self,
        sql: &'static str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, postgres::Error> {
        let s = self.statement(sql)?;
        self.client.query_opt(&s, params)
    }

    pub(crate) fn query_one(
        &mut self,
        sql: &'static str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, postgres::Error> {
        let s = self.statement(sql)?;
        self.client.query_one(&s, params)
    }

    pub(crate) fn execute(
        &mut self,
        sql: &'static str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<u64, postgres::Error> {
        let s = self.statement(sql)?;
        self.client.execute(&s, params)
    }
}

pub(crate) type Shared = Arc<Mutex<Conn>>;

pub(crate) fn lock(c: &Shared) -> MutexGuard<'_, Conn> {
    c.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A model of the network (a row of `line`).
#[derive(Debug, Clone, PartialEq)]
pub struct LineRow {
    /// Its code (`A`, `L012`): the prefix of renamed ids.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The project file it came from (file name), if imported.
    pub source: String,
    /// Built (pinned to library versions, phase 9).
    pub built: bool,
}

/// A connection to a network's database.
pub struct Network {
    conn: Shared,
    writer: bool,
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("writer", &self.writer)
            .finish_non_exhaustive()
    }
}

impl Network {
    /// Connects to the database at `url` (a PostgreSQL connection string,
    /// e.g. `host=127.0.0.1 port=5432 user=ohl dbname=network`) to read
    /// (and to write the audit): the schema made or brought forward
    /// first, a newer one refused.
    pub fn connect(url: &str) -> Result<Self, NetError> {
        let client = Client::connect(url, NoTls)?;
        let mut net = Self {
            conn: Arc::new(Mutex::new(Conn {
                client,
                steps: Vec::new(),
                prepared: std::collections::HashMap::new(),
                writer: false,
            })),
            writer: false,
        };
        net.migrate()?;
        Ok(net)
    }

    /// Connects as the network's one writer (the authority, an import):
    /// [`Network::connect`], then the writer's advisory lock — refused
    /// while another connection holds it. The lock is released when the
    /// connection's last user is dropped (explicitly, so that a writer
    /// connecting right after is not refused), or goes with the
    /// connection when it breaks.
    pub fn connect_writer(url: &str) -> Result<Self, NetError> {
        let mut net = Self::connect(url)?;
        let got: bool = lock(&net.conn)
            .query_one("SELECT pg_try_advisory_lock($1)", &[&WRITER_LOCK])?
            .get(0);
        if !got {
            return Err(NetError::Refused(
                "another program writes this network (its writer's lock is held): one \
                 authority per database"
                    .into(),
            ));
        }
        net.writer = true;
        lock(&net.conn).writer = true;
        Ok(net)
    }

    /// Whether this connection holds the writer's lock.
    pub fn is_writer(&self) -> bool {
        self.writer
    }

    pub(crate) fn shared(&self) -> Shared {
        self.conn.clone()
    }

    /// Makes or brings the schema forward ([`schema::MIGRATIONS`]), each
    /// migration in a transaction of its own, under a lock so that two
    /// programs starting together do not both migrate. A database of a
    /// newer schema is refused before anything changes.
    fn migrate(&mut self) -> Result<(), NetError> {
        let mut c = lock(&self.conn);
        let client = &mut c.client;
        let mut tx = client.transaction()?;
        tx.execute("SELECT pg_advisory_xact_lock($1)", &[&(WRITER_LOCK + 1)])?;
        tx.batch_execute(
            "CREATE TABLE IF NOT EXISTS net_schema (version integer PRIMARY KEY,
                 time timestamptz NOT NULL DEFAULT clock_timestamp(), note text NOT NULL)",
        )?;
        let have: i32 = tx
            .query_one("SELECT COALESCE(MAX(version), 0) FROM net_schema", &[])?
            .get(0);
        if have > schema::SCHEMA {
            return Err(NetError::Refused(format!(
                "network schema {have} newer than this program's ({})",
                schema::SCHEMA
            )));
        }
        for (version, note, sql) in schema::MIGRATIONS {
            if *version <= have {
                continue;
            }
            let mut step = tx.transaction()?;
            step.batch_execute(sql)?;
            step.execute(
                "INSERT INTO net_schema (version, note) VALUES ($1, $2)",
                &[version, note],
            )?;
            step.execute(
                "INSERT INTO net_meta (key, value) VALUES ('schema', $1)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                &[&version.to_string()],
            )?;
            step.commit()?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The schema of the database (migrations done).
    pub fn schema(&self) -> Result<i32, NetError> {
        Ok(lock(&self.conn)
            .query_one("SELECT COALESCE(MAX(version), 0) FROM net_schema", &[])?
            .get(0))
    }

    /// Network meta value `key` (`crs`, `grid_size`, `name`, …).
    pub fn meta(&self, key: &str) -> Result<Option<String>, NetError> {
        Ok(lock(&self.conn)
            .query_opt("SELECT value FROM net_meta WHERE key = $1", &[&key])?
            .map(|r| r.get(0)))
    }

    /// Sets network meta value `key`.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), NetError> {
        self.require_writer()?;
        lock(&self.conn).execute(
            "INSERT INTO net_meta (key, value) VALUES ($1, $2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            &[&key, &value],
        )?;
        Ok(())
    }

    pub(crate) fn require_writer(&self) -> Result<(), NetError> {
        if self.writer {
            Ok(())
        } else {
            Err(NetError::Refused(
                "connected to read only: connect as the network's writer".into(),
            ))
        }
    }

    /// The models, by code.
    pub fn lines(&self) -> Result<Vec<LineRow>, NetError> {
        Ok(lock(&self.conn)
            .query("SELECT id, name, source, built FROM line ORDER BY id", &[])?
            .into_iter()
            .map(|r| LineRow {
                id: r.get(0),
                name: r.get(1),
                source: r.get(2),
                built: r.get(3),
            })
            .collect())
    }

    /// Adds an empty model `code` named `name` (refused when it exists or
    /// the code is not a short code of letters, digits, `_`, `.`, `-`).
    pub fn create_line(&self, code: &str, name: &str, source: &str) -> Result<(), NetError> {
        self.require_writer()?;
        check_code(code)?;
        let mut c = lock(&self.conn);
        if c.query_opt("SELECT 1 FROM line WHERE id = $1", &[&code])?
            .is_some()
        {
            return Err(NetError::Refused(format!(
                "the network has a line {code} already"
            )));
        }
        c.execute(
            "INSERT INTO line (id, name, source) VALUES ($1, $2, $3)",
            &[&code, &name, &source],
        )?;
        Ok(())
    }

    /// Model `code` as a store, to open a session on
    /// ([`ohl_session::Session::open_on`]); refused when the network has
    /// no such model. Writes through it need the writer's connection.
    pub fn store(&self, code: &str) -> Result<PgStore, NetError> {
        if lock(&self.conn)
            .query_opt("SELECT 1 FROM line WHERE id = $1", &[&code])?
            .is_none()
        {
            return Err(NetError::Refused(format!("the network has no line {code}")));
        }
        Ok(PgStore::new(self.shared(), code))
    }

    /// Appends a row to the audit (who, what, on what, details).
    pub fn audit(
        &self,
        user: &str,
        action: &str,
        target: &str,
        detail: &str,
    ) -> Result<(), NetError> {
        lock(&self.conn).execute(
            "INSERT INTO audit (user_name, action, target, detail) VALUES ($1, $2, $3, $4)",
            &[&user, &action, &target, &detail],
        )?;
        Ok(())
    }

    /// The audit, oldest first: time, user, action, target, detail.
    pub fn audit_rows(&self) -> Result<Vec<[String; 5]>, NetError> {
        Ok(lock(&self.conn)
            .query(
                "SELECT ohl_time(time), user_name, action, target, detail FROM audit ORDER BY id",
                &[],
            )?
            .into_iter()
            .map(|r| [r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)])
            .collect())
    }

    /// The database's size on disk, bytes.
    pub fn size(&self) -> Result<i64, NetError> {
        Ok(lock(&self.conn)
            .query_one("SELECT pg_database_size(current_database())", &[])?
            .get(0))
    }

    /// Rows and bytes (with indexes and TOAST) of each of the network's
    /// tables, the largest first.
    pub fn table_sizes(&self) -> Result<Vec<(String, i64, i64)>, NetError> {
        Ok(lock(&self.conn)
            .query(
                "SELECT c.relname::text, c.reltuples::bigint, pg_total_relation_size(c.oid)
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
                 WHERE n.nspname = 'public' AND c.relkind = 'r'
                 ORDER BY pg_total_relation_size(c.oid) DESC",
                &[],
            )?
            .into_iter()
            .map(|r| (r.get(0), r.get(1), r.get(2)))
            .collect())
    }

    /// Brings the planner's statistics up to date (after a large import).
    pub fn analyse(&self) -> Result<(), NetError> {
        lock(&self.conn).client.batch_execute("ANALYZE")?;
        Ok(())
    }
}

/// Refuses a model code that is not 1 to 32 letters, digits, `_`, `.` or
/// `-` (starting with a letter or digit): it becomes the prefix of
/// renamed ids.
pub fn check_code(code: &str) -> Result<(), NetError> {
    let ok = !code.is_empty()
        && code.len() <= 32
        && code
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if ok {
        Ok(())
    } else {
        Err(NetError::Refused(format!(
            "\"{code}\": a line code of 1 to 32 letters, digits, '_', '.' or '-'"
        )))
    }
}
