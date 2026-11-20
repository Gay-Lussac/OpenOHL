// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! One model of the network as the session's store: every method of
//! [`Store`] on the model's rows (`line_id` = the model's code), with the
//! project file's meaning — the same tests pass on both
//! ([`ohl_session::store::suite`]).
//!
//! Where PostgreSQL differs from SQLite, the store keeps SQLite's
//! meaning:
//!
//! - **Steps**: a savepoint outside a transaction begins one (as SQLite
//!   does); releasing or rolling back the outermost ends it. Statements
//!   outside any step commit by themselves.
//! - **Numbers**: a new log entry takes the next seq of the network (the
//!   largest + 1, as SQLite's rowid does — the one writer makes them
//!   gapless); a grant or review the next id of its model.
//! - **Times**: `timestamptz`, given to the session as the project file
//!   writes them (`YYYY-MM-DDTHH:MM:SS.sssZ`, milliseconds).
//! - **Order**: "newest first" of revisions is the order they were kept
//!   in (`n`), as SQLite's rowid.

use std::collections::BTreeMap;
use std::path::Path;

use ohl_core::grid::{Bounds, Grid, Tile};
use ohl_session::points::PointChange;
use ohl_session::spatial::Entry;
use ohl_session::store::sqlite::SqliteStore;
use ohl_session::store::{
    EntryRow, GrantRow, LegacySnapshot, NewEntry, NewSurface, ObjectRow, RefRow, ReviewNoteRow,
    ReviewRow, RevisionRow, Store, SurfaceRow,
};
use ohl_session::{Kind, SCHEMA_VERSION, SessionError};
use postgres::Row;

use crate::{Conn, Shared, db_message, lock};

/// A model of the network as a [`Store`].
pub struct PgStore {
    conn: Shared,
    line: String,
    /// Blobs this store put, and the count at which the version store's
    /// statistics are read again next (phase 14: see
    /// [`PgStore::put_blob`]).
    blobs_put: std::sync::atomic::AtomicUsize,
    analyse_at: std::sync::atomic::AtomicUsize,
}

/// Blobs put before the version store's statistics are first read again;
/// then each time their number doubled.
const ANALYSE_FIRST: usize = 2_000;

impl std::fmt::Debug for PgStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgStore")
            .field("line", &self.line)
            .finish_non_exhaustive()
    }
}

/// A database error as the session's: an id another model has refused
/// in words, anything else a store error.
fn err(e: postgres::Error) -> SessionError {
    if let Some(d) = e.as_db_error()
        && d.constraint() == Some("object_network_id")
    {
        return SessionError::Refused(format!(
            "an object of another line of the network has this id ({}): ids are unique in \
             the network",
            d.detail().unwrap_or_default()
        ));
    }
    SessionError::Store(db_message(&e))
}

fn entry_row(r: &Row) -> EntryRow {
    (
        r.get(0),
        r.get(1),
        r.get(2),
        r.get(3),
        r.get(4),
        r.get(5),
        r.get(6),
        r.get(7),
        r.get(8),
        r.get(9),
        r.get(10),
        r.get(11),
    )
}

fn revision_row(r: &Row) -> RevisionRow {
    RevisionRow {
        id: r.get(0),
        parents: r.get(1),
        author: r.get(2),
        time: r.get(3),
        message: r.get(4),
        tree: r.get(5),
        seq: r.get(6),
    }
}

fn spatial_row(r: &Row) -> Result<(Kind, String, Entry), SessionError> {
    let level: i16 = r.get(2);
    Ok((
        Kind::parse(r.get::<_, &str>(0))?,
        r.get(1),
        Entry {
            tile: Tile {
                level: i8::try_from(level)
                    .map_err(|_| SessionError::Store(format!("tile level {level}")))?,
                x: r.get(3),
                y: r.get(4),
            },
            bounds: Bounds {
                min: [r.get(5), r.get(6)],
                max: [r.get(7), r.get(8)],
            },
        },
    ))
}

/// A savepoint's name: the session's own (letters, digits, `_`), never
/// anything else in a statement.
fn step_name(name: &str) -> Result<&str, SessionError> {
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(name)
    } else {
        Err(SessionError::Store(format!("step name \"{name}\"")))
    }
}

/// The model's log entries, oldest first.
const ENTRIES: &str = "SELECT seq, ohl_time(time), user_name, kind, command, inverse, objects, \
                       sections, undoes, undone_by, note, branch FROM oplog WHERE line_id = $1 \
                       ORDER BY seq";
/// The model's log entry `$2`.
const ENTRY: &str = "SELECT seq, ohl_time(time), user_name, kind, command, inverse, objects, \
                     sections, undoes, undone_by, note, branch FROM oplog \
                     WHERE line_id = $1 AND seq = $2";

impl PgStore {
    pub(crate) fn new(conn: Shared, line: &str) -> Self {
        Self {
            conn,
            line: line.to_owned(),
            blobs_put: std::sync::atomic::AtomicUsize::new(0),
            analyse_at: std::sync::atomic::AtomicUsize::new(ANALYSE_FIRST),
        }
    }

    /// The model's code.
    pub fn line(&self) -> &str {
        &self.line
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Conn> {
        lock(&self.conn)
    }

    fn exec(
        &self,
        sql: &'static str,
        params: &[&(dyn postgres::types::ToSql + Sync)],
    ) -> Result<u64, SessionError> {
        self.c().execute(sql, params).map_err(err)
    }

    fn rows(
        &self,
        sql: &'static str,
        params: &[&(dyn postgres::types::ToSql + Sync)],
    ) -> Result<Vec<Row>, SessionError> {
        self.c().query(sql, params).map_err(err)
    }

    fn row(
        &self,
        sql: &'static str,
        params: &[&(dyn postgres::types::ToSql + Sync)],
    ) -> Result<Option<Row>, SessionError> {
        self.c().query_opt(sql, params).map_err(err)
    }

    fn one(
        &self,
        sql: &'static str,
        params: &[&(dyn postgres::types::ToSql + Sync)],
    ) -> Result<Row, SessionError> {
        self.c().query_one(sql, params).map_err(err)
    }

    /// Writes the model as a project file at `path` (must not exist):
    /// every row through the store interface
    /// ([`ohl_session::store::copy::copy_all`]).
    fn write_file(&self, path: &Path) -> Result<(), SessionError> {
        if path.exists() {
            return Err(SessionError::Store(format!(
                "{}: exists already",
                path.display()
            )));
        }
        let file = SqliteStore::open(path)?;
        ohl_session::store::copy::copy_all(self, &file, path)?;
        file.checkpoint()?;
        Ok(())
    }
}

impl Store for PgStore {
    fn prepare(&self, label: &Path, writable: bool) -> Result<Grid, SessionError> {
        let line = &self.line;
        if let Some(v) = self.meta("session_schema")?
            && v.parse::<u32>().ok().is_none_or(|v| v > SCHEMA_VERSION)
        {
            return Err(SessionError::Store(format!(
                "{}: session schema {v} newer than supported ({SCHEMA_VERSION})",
                label.display()
            )));
        }
        // One grid for the network: the model's is the network's.
        let net: Option<String> = self
            .row("SELECT value FROM net_meta WHERE key = 'grid_size'", &[])?
            .map(|r| r.get(0));
        let grid = match (self.meta("grid_size")?, net) {
            (Some(own), Some(net)) if own != net => {
                return Err(SessionError::Store(format!(
                    "{}: grid of {own} m, the network's is {net} m",
                    label.display()
                )));
            }
            (Some(text), _) | (None, Some(text)) => text
                .parse::<f64>()
                .map_err(|e| e.to_string())
                .and_then(Grid::new)
                .map_err(|e| SessionError::Store(format!("grid_size \"{text}\": {e}")))?,
            (None, None) => Grid::default(),
        };
        if writable {
            let size = grid.size.to_string();
            self.exec(
                "INSERT INTO net_meta (key, value) VALUES ('grid_size', $1) ON CONFLICT DO NOTHING",
                &[&size],
            )?;
            self.exec(
                "INSERT INTO line_meta (line_id, key, value) VALUES ($1, 'grid_size', $2)
                 ON CONFLICT DO NOTHING",
                &[line, &size],
            )?;
            self.exec(
                "INSERT INTO line_meta (line_id, key, value) VALUES ($1, 'session_schema', $2)
                 ON CONFLICT (line_id, key) DO UPDATE SET value = excluded.value",
                &[line, &SCHEMA_VERSION.to_string()],
            )?;
        }
        let main = ohl_session::versions::MAIN;
        if writable || !self.versions_initialised(main)? {
            self.init_versions(main)?;
        }
        Ok(grid)
    }

    fn mark_object_upgrades_done(&self) -> Result<(), SessionError> {
        for key in ["routes_alignments", "alignment_points", "structure_groups"] {
            self.set_meta(key, "1")?;
        }
        Ok(())
    }

    fn savepoint(&self, name: &str) -> Result<(), SessionError> {
        let name = step_name(name)?;
        let mut c = self.c();
        if c.steps.is_empty() {
            c.client.batch_execute("BEGIN").map_err(err)?;
        }
        c.client
            .batch_execute(&format!("SAVEPOINT {name}"))
            .map_err(err)?;
        c.steps.push(name.to_owned());
        Ok(())
    }

    fn release(&self, name: &str) -> Result<(), SessionError> {
        let name = step_name(name)?;
        let mut c = self.c();
        let at = c
            .steps
            .iter()
            .rposition(|s| s == name)
            .ok_or_else(|| SessionError::Store(format!("no step {name}")))?;
        c.steps.truncate(at);
        let sql = if c.steps.is_empty() {
            "COMMIT".to_owned()
        } else {
            format!("RELEASE SAVEPOINT {name}")
        };
        if let Err(e) = c.client.batch_execute(&sql) {
            if c.steps.is_empty() {
                let _ = c.client.batch_execute("ROLLBACK");
            }
            return Err(err(e));
        }
        Ok(())
    }

    fn roll_back_to(&self, name: &str) -> Result<(), SessionError> {
        let name = step_name(name)?;
        let mut c = self.c();
        let at = c
            .steps
            .iter()
            .rposition(|s| s == name)
            .ok_or_else(|| SessionError::Store(format!("no step {name}")))?;
        c.steps.truncate(at);
        let sql = if c.steps.is_empty() {
            "ROLLBACK".to_owned()
        } else {
            format!("ROLLBACK TO SAVEPOINT {name}; RELEASE SAVEPOINT {name}")
        };
        c.client.batch_execute(&sql).map_err(err)
    }

    fn copy_to(&self, path: &Path) -> Result<(), SessionError> {
        self.write_file(path)
    }

    fn copy_for_others(&self, path: &Path) -> Result<(), SessionError> {
        self.write_file(path)?;
        let file = SqliteStore::open(path)?;
        for (key, value) in file.meta_rows()? {
            if key.starts_with("setting.") && ohl_session::is_local_path(&value) {
                let name = value.rsplit(['/', '\\']).next().unwrap_or_default();
                file.set_meta(&key, name)?;
            }
        }
        file.checkpoint()
    }

    fn checkpoint(&self) -> Result<(), SessionError> {
        // Every commit is on disk already.
        Ok(())
    }

    fn now(&self) -> Result<String, SessionError> {
        Ok(self
            .one(
                "SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')",
                &[],
            )?
            .get(0))
    }

    fn meta(&self, key: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .row(
                "SELECT value FROM line_meta WHERE line_id = $1 AND key = $2",
                &[&self.line, &key],
            )?
            .map(|r| r.get(0)))
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO line_meta (line_id, key, value) VALUES ($1, $2, $3)
             ON CONFLICT (line_id, key) DO UPDATE SET value = excluded.value",
            &[&self.line, &key, &value],
        )?;
        Ok(())
    }

    fn delete_meta(&self, key: &str) -> Result<(), SessionError> {
        self.exec(
            "DELETE FROM line_meta WHERE line_id = $1 AND key = $2",
            &[&self.line, &key],
        )?;
        Ok(())
    }

    fn user_count(&self) -> Result<i64, SessionError> {
        Ok(self
            .one(
                "SELECT COUNT(*) FROM line_user WHERE line_id = $1",
                &[&self.line],
            )?
            .get(0))
    }

    fn put_user(&self, name: &str, role: &str) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO line_user (line_id, name, role) VALUES ($1, $2, $3)
             ON CONFLICT (line_id, name) DO UPDATE SET role = excluded.role",
            &[&self.line, &name, &role],
        )?;
        Ok(())
    }

    fn user_role(&self, name: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .row(
                "SELECT role FROM line_user WHERE line_id = $1 AND name = $2",
                &[&self.line, &name],
            )?
            .map(|r| r.get(0)))
    }

    fn users(&self) -> Result<Vec<(String, String)>, SessionError> {
        Ok(self
            .rows(
                "SELECT name, role FROM line_user WHERE line_id = $1 ORDER BY name",
                &[&self.line],
            )?
            .iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect())
    }

    fn object_rows(&self) -> Result<Vec<ObjectRow>, SessionError> {
        Ok(self
            .rows(
                "SELECT branch, kind, id, ord, version, json, gone FROM object WHERE line_id = $1",
                &[&self.line],
            )?
            .iter()
            .map(|r| ObjectRow {
                line: r.get(0),
                kind: r.get(1),
                id: r.get(2),
                ord: r.get(3),
                version: r.get(4),
                json: r.get(5),
                gone: i64::from(r.get::<_, i16>(6)),
            })
            .collect())
    }

    fn write_object(
        &self,
        line: &str,
        kind: Kind,
        id: &str,
        object: Option<(i64, i64, &str)>,
    ) -> Result<(), SessionError> {
        let (ord, version, json, gone) = match object {
            Some((o, v, j)) => (o, v, j, 0i16),
            None => (0, 0, "null", 1),
        };
        self.exec(
            "INSERT INTO object (line_id, branch, kind, id, ord, version, json, gone)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (line_id, branch, kind, id) DO UPDATE SET ord = excluded.ord,
                 version = excluded.version, json = excluded.json, gone = excluded.gone",
            &[
                &self.line,
                &line,
                &kind.as_str(),
                &id,
                &ord,
                &version,
                &json,
                &gone,
            ],
        )?;
        Ok(())
    }

    fn replace_object(
        &self,
        line: &str,
        kind: Kind,
        id: &str,
        ord: i64,
        version: i64,
        json: &str,
    ) -> Result<(), SessionError> {
        // As SQLite's INSERT OR REPLACE: the row made again whole (gone
        // back to its default).
        self.exec(
            "INSERT INTO object (line_id, branch, kind, id, ord, version, json, gone)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 0)
             ON CONFLICT (line_id, branch, kind, id) DO UPDATE SET ord = excluded.ord,
                 version = excluded.version, json = excluded.json, gone = 0",
            &[
                &self.line,
                &line,
                &kind.as_str(),
                &id,
                &ord,
                &version,
                &json,
            ],
        )?;
        Ok(())
    }

    fn write_place(
        &self,
        line: &str,
        kind: Kind,
        id: &str,
        ord: i64,
        version: i64,
    ) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO object (line_id, branch, kind, id, ord, version, json, gone)
             VALUES ($1, $2, $3, $4, $5, $6, 'null', 2)
             ON CONFLICT (line_id, branch, kind, id) DO UPDATE SET ord = excluded.ord,
                 version = excluded.version, json = excluded.json, gone = excluded.gone",
            &[&self.line, &line, &kind.as_str(), &id, &ord, &version],
        )?;
        Ok(())
    }

    fn delete_object(&self, line: &str, kind: Kind, id: &str) -> Result<(), SessionError> {
        self.exec(
            "DELETE FROM object WHERE line_id = $1 AND branch = $2 AND kind = $3 AND id = $4",
            &[&self.line, &line, &kind.as_str(), &id],
        )?;
        Ok(())
    }

    fn line_base(&self, name: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .row(
                "SELECT base FROM vc_ref WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
                &[&self.line, &name],
            )?
            .and_then(|r| r.get(0)))
    }

    fn line_description(&self, name: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .row(
                "SELECT description FROM vc_ref WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
                &[&self.line, &name],
            )?
            .map(|r| r.get(0)))
    }

    fn set_line_description(&self, name: &str, description: &str) -> Result<(), SessionError> {
        self.exec(
            "UPDATE vc_ref SET description = $3 WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
            &[&self.line, &name, &description],
        )?;
        Ok(())
    }

    fn set_line_base(&self, name: &str, base: &str) -> Result<(), SessionError> {
        self.exec(
            "UPDATE vc_ref SET base = $3 WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
            &[&self.line, &name, &base],
        )?;
        Ok(())
    }

    fn delete_line(&self, name: &str) -> Result<(), SessionError> {
        let l = &self.line;
        self.exec(
            "DELETE FROM object WHERE line_id = $1 AND branch = $2",
            &[l, &name],
        )?;
        self.exec(
            "DELETE FROM spatial WHERE line_id = $1 AND branch = $2",
            &[l, &name],
        )?;
        self.exec(
            "DELETE FROM vc_ref WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
            &[l, &name],
        )?;
        Ok(())
    }

    fn rename_line(&self, from: &str, to: &str) -> Result<(), SessionError> {
        let l = &self.line;
        self.exec(
            "UPDATE object SET branch = $3 WHERE line_id = $1 AND branch = $2",
            &[l, &from, &to],
        )?;
        self.exec(
            "UPDATE oplog SET branch = $3 WHERE line_id = $1 AND branch = $2",
            &[l, &from, &to],
        )?;
        self.exec(
            "UPDATE spatial SET branch = $3 WHERE line_id = $1 AND branch = $2",
            &[l, &from, &to],
        )?;
        self.exec(
            "UPDATE vc_ref SET name = $3 WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
            &[l, &from, &to],
        )?;
        self.exec(
            "UPDATE vc_review SET branch = $3 WHERE line_id = $1 AND branch = $2",
            &[l, &from, &to],
        )?;
        self.exec(
            "UPDATE vc_review SET target = $3 WHERE line_id = $1 AND target = $2",
            &[l, &from, &to],
        )?;
        Ok(())
    }

    fn legacy_snapshots(&self) -> Result<Option<Vec<LegacySnapshot>>, SessionError> {
        // Only project files of schema 3 and before have them.
        Ok(None)
    }

    fn delete_legacy_snapshot(&self, _name: &str) -> Result<(), SessionError> {
        Ok(())
    }

    fn drop_legacy_snapshots(&self) -> Result<(), SessionError> {
        Ok(())
    }

    fn append_entry(&self, e: &NewEntry<'_>) -> Result<i64, SessionError> {
        // The next seq of the network (the one writer: no gaps, as SQLite's
        // rowid).
        Ok(self
            .one(
                "INSERT INTO oplog (line_id, seq, user_name, kind, command, inverse, objects,
                     sections, undoes, note, branch)
                 SELECT $1, COALESCE($11, COALESCE(MAX(seq), 0) + 1), $2, $3, $4, $5, $6, $7,
                     $8, $9, $10
                 FROM oplog
                 RETURNING seq",
                &[
                    &self.line,
                    &e.user,
                    &e.kind,
                    &e.command,
                    &e.inverse,
                    &e.objects,
                    &e.sections,
                    &e.undoes,
                    &e.note,
                    &e.line,
                    &e.seq,
                ],
            )?
            .get(0))
    }

    fn mark_undone(&self, seq: i64, by: i64) -> Result<(), SessionError> {
        self.exec(
            "UPDATE oplog SET undone_by = $2 WHERE line_id = $1 AND seq = $3",
            &[&self.line, &by, &seq],
        )?;
        Ok(())
    }

    fn mark_undone_after(&self, by: i64, seq: i64, line: &str) -> Result<(), SessionError> {
        self.exec(
            "UPDATE oplog SET undone_by = $2 WHERE line_id = $1 AND seq > $3 AND seq < $2
                 AND undone_by IS NULL AND branch = $4",
            &[&self.line, &by, &seq, &line],
        )?;
        Ok(())
    }

    fn entries(&self) -> Result<Vec<EntryRow>, SessionError> {
        Ok(self
            .rows(ENTRIES, &[&self.line])?
            .iter()
            .map(entry_row)
            .collect())
    }

    fn entry(&self, seq: i64) -> Result<Option<EntryRow>, SessionError> {
        Ok(self
            .row(ENTRY, &[&self.line, &seq])?
            .as_ref()
            .map(entry_row))
    }

    fn entry_objects(&self, seq: i64) -> Result<Option<String>, SessionError> {
        Ok(self
            .row(
                "SELECT objects FROM oplog WHERE line_id = $1 AND seq = $2",
                &[&self.line, &seq],
            )?
            .map(|r| r.get(0)))
    }

    fn note_reversal(&self, seq: i64, reversed: i64) -> Result<(), SessionError> {
        self.exec(
            "UPDATE oplog SET note = 'reverses ' || $3::bigint || COALESCE(': ' ||
                 (SELECT o.note FROM oplog o WHERE o.line_id = $1 AND o.seq = $3), '')
             WHERE line_id = $1 AND seq = $2",
            &[&self.line, &seq, &reversed],
        )?;
        Ok(())
    }

    fn set_entry_note(&self, seq: i64, note: &str) -> Result<(), SessionError> {
        self.exec(
            "UPDATE oplog SET note = $3 WHERE line_id = $1 AND seq = $2",
            &[&self.line, &seq, &note],
        )?;
        Ok(())
    }

    fn last_undoable(&self, user: &str, line: &str) -> Result<Option<i64>, SessionError> {
        Ok(self
            .row(
                "SELECT seq FROM oplog WHERE line_id = $1 AND user_name = $2 AND branch = $3
                     AND kind IN ('do', 'redo') AND undone_by IS NULL ORDER BY seq DESC LIMIT 1",
                &[&self.line, &user, &line],
            )?
            .map(|r| r.get(0)))
    }

    fn last_own_entry(
        &self,
        user: &str,
        line: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SessionError> {
        Ok(self
            .row(
                "SELECT seq, kind, undone_by FROM oplog WHERE line_id = $1 AND user_name = $2
                     AND branch = $3 AND kind IN ('do', 'undo', 'redo') ORDER BY seq DESC LIMIT 1",
                &[&self.line, &user, &line],
            )?
            .map(|r| (r.get(0), r.get(1), r.get(2))))
    }

    fn inverses_after(&self, seq: i64, line: &str) -> Result<Vec<String>, SessionError> {
        Ok(self
            .rows(
                "SELECT inverse FROM oplog WHERE line_id = $1 AND seq > $2 AND branch = $3
                 ORDER BY seq DESC",
                &[&self.line, &seq, &line],
            )?
            .iter()
            .map(|r| r.get(0))
            .collect())
    }

    fn last_seq(&self) -> Result<i64, SessionError> {
        Ok(self
            .one(
                "SELECT COALESCE(MAX(seq), 0) FROM oplog WHERE line_id = $1",
                &[&self.line],
            )?
            .get(0))
    }

    fn last_seq_of_line(&self, line: &str) -> Result<i64, SessionError> {
        Ok(self
            .one(
                "SELECT COALESCE(MAX(seq), 0) FROM oplog WHERE line_id = $1 AND branch = $2",
                &[&self.line, &line],
            )?
            .get(0))
    }

    fn spatial_write_diff(
        &self,
        line: &str,
        old: &BTreeMap<(Kind, String), Entry>,
        new: &BTreeMap<(Kind, String), Entry>,
    ) -> Result<usize, SessionError> {
        if !self.spatial_kept(line) {
            return Ok(0);
        }
        let mut changed = 0;
        let mut c = self.c();
        let delete = c
            .statement(
                "DELETE FROM spatial WHERE line_id = $1 AND branch = $2 AND kind = $3 AND id = $4",
            )
            .map_err(err)?;
        for ((kind, id), _) in old.iter().filter(|(k, _)| !new.contains_key(*k)) {
            c.client
                .execute(&delete, &[&self.line, &line, &kind.as_str(), id])
                .map_err(err)?;
            changed += 1;
        }
        let upsert = c
            .statement(
                "INSERT INTO spatial (line_id, branch, kind, id, level, tx, ty, x0, y0, x1,
                     y1, geom)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                     ST_MakeEnvelope($8, $9, $10, $11))
                 ON CONFLICT (line_id, branch, kind, id) DO UPDATE SET level = excluded.level,
                     tx = excluded.tx, ty = excluded.ty, x0 = excluded.x0,
                     y0 = excluded.y0, x1 = excluded.x1, y1 = excluded.y1,
                     geom = excluded.geom",
            )
            .map_err(err)?;
        for ((kind, id), e) in new {
            if old.get(&(*kind, id.clone())) == Some(e) {
                continue;
            }
            let b = e.bounds;
            c.client
                .execute(
                    &upsert,
                    &[
                        &self.line,
                        &line,
                        &kind.as_str(),
                        id,
                        &i16::from(e.tile.level),
                        &e.tile.x,
                        &e.tile.y,
                        &b.min[0],
                        &b.min[1],
                        &b.max[0],
                        &b.max[1],
                    ],
                )
                .map_err(err)?;
            changed += 1;
        }
        Ok(changed)
    }

    fn spatial_kept(&self, line: &str) -> bool {
        line == ohl_session::versions::MAIN
    }

    fn spatial_load(&self, line: &str) -> Result<BTreeMap<(Kind, String), Entry>, SessionError> {
        let mut out = BTreeMap::new();
        if !self.spatial_kept(line) {
            // Rows of a branch written before 2026-11-20: not read.
            return Ok(out);
        }
        for r in self.rows(
            "SELECT kind, id, level, tx, ty, x0, y0, x1, y1 FROM spatial
             WHERE line_id = $1 AND branch = $2",
            &[&self.line, &line],
        )? {
            let (kind, id, e) = spatial_row(&r)?;
            out.insert((kind, id), e);
        }
        Ok(out)
    }

    fn spatial_query(
        &self,
        line: &str,
        grid: &Grid,
        area: &Bounds,
    ) -> Result<Vec<(Kind, String, Entry)>, SessionError> {
        let levels: Vec<i16> = self
            .rows(
                "SELECT DISTINCT level FROM spatial WHERE line_id = $1 AND branch = $2
                 ORDER BY level",
                &[&self.line, &line],
            )?
            .iter()
            .map(|r| r.get(0))
            .collect();
        let mut out = Vec::new();
        for level in levels {
            let r = grid.query(area, level as i8);
            for row in self.rows(
                "SELECT kind, id, level, tx, ty, x0, y0, x1, y1 FROM spatial
                 WHERE line_id = $1 AND branch = $2 AND level = $3
                   AND tx BETWEEN $4 AND $5 AND ty BETWEEN $6 AND $7
                   AND x1 >= $8 AND x0 <= $9 AND y1 >= $10 AND y0 <= $11",
                &[
                    &self.line,
                    &line,
                    &level,
                    &r.x[0],
                    &r.x[1],
                    &r.y[0],
                    &r.y[1],
                    &area.min[0],
                    &area.max[0],
                    &area.min[1],
                    &area.max[1],
                ],
            )? {
                out.push(spatial_row(&row)?);
            }
        }
        out.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        Ok(out)
    }

    fn split_trees(&self) -> bool {
        true
    }

    fn has_blob(&self, hash: &str) -> Result<bool, SessionError> {
        Ok(self
            .row(
                "SELECT 1 FROM vc_blob_use WHERE line_id = $1 AND hash = $2",
                &[&self.line, &hash],
            )?
            .is_some())
    }

    fn put_blob(&self, hash: &str, data: &[u8]) -> Result<(), SessionError> {
        // Kept once for the network; the model's use of it noted (a blob
        // put twice by one model refused, as in a project file).
        self.exec(
            "INSERT INTO vc_blob (hash, data) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&hash, &data],
        )?;
        self.exec(
            "INSERT INTO vc_blob_use (line_id, hash) VALUES ($1, $2)",
            &[&self.line, &hash],
        )?;
        // Phase 14: a model's first revision writes every object's blob in
        // one transaction; the planner, with the statistics of tables
        // nearly empty, checked each new row's references by reading the
        // rows written so far one after the other (minutes on the
        // synthetic country as one model). The statistics are read again
        // (ANALYZE: within the transaction, its own rows counted) after
        // 2 000 blobs, then each time their number doubled — a few times
        // per model, nothing else changed.
        use std::sync::atomic::Ordering::Relaxed;
        let n = self.blobs_put.fetch_add(1, Relaxed) + 1;
        if n >= self.analyse_at.load(Relaxed) {
            self.analyse_at.store(n.saturating_mul(2), Relaxed);
            self.c()
                .client
                .batch_execute("ANALYZE vc_blob, vc_blob_use")
                .map_err(err)?;
        }
        Ok(())
    }

    fn blob(&self, hash: &str) -> Result<Option<Vec<u8>>, SessionError> {
        Ok(self
            .row(
                "SELECT b.data FROM vc_blob b JOIN vc_blob_use u ON u.hash = b.hash
                 WHERE u.line_id = $1 AND b.hash = $2",
                &[&self.line, &hash],
            )?
            .map(|r| r.get(0)))
    }

    fn revision(&self, id: &str) -> Result<Option<RevisionRow>, SessionError> {
        Ok(self
            .row(
                "SELECT id, parents, author, ohl_time(time), message, tree, seq FROM vc_revision
                 WHERE line_id = $1 AND id = $2",
                &[&self.line, &id],
            )?
            .as_ref()
            .map(revision_row))
    }

    fn revisions(&self) -> Result<Vec<RevisionRow>, SessionError> {
        Ok(self
            .rows(
                "SELECT id, parents, author, ohl_time(time), message, tree, seq FROM vc_revision
                 WHERE line_id = $1 ORDER BY n DESC",
                &[&self.line],
            )?
            .iter()
            .map(revision_row)
            .collect())
    }

    fn revisions_starting(&self, prefix: &str) -> Result<Vec<String>, SessionError> {
        Ok(self
            .rows(
                "SELECT id FROM vc_revision WHERE line_id = $1 AND starts_with(id, $2) LIMIT 2",
                &[&self.line, &prefix],
            )?
            .iter()
            .map(|r| r.get(0))
            .collect())
    }

    fn has_revision(&self, id: &str) -> Result<bool, SessionError> {
        Ok(self
            .row(
                "SELECT 1 FROM vc_revision WHERE line_id = $1 AND id = $2",
                &[&self.line, &id],
            )?
            .is_some())
    }

    fn put_revision(&self, r: &RevisionRow) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_revision (line_id, id, parents, author, message, tree, seq)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &self.line, &r.id, &r.parents, &r.author, &r.message, &r.tree, &r.seq,
            ],
        )?;
        Ok(())
    }

    fn put_revision_with_time(&self, r: &RevisionRow) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_revision (line_id, id, parents, author, time, message, tree, seq)
             VALUES ($1, $2, $3, $4, CAST($5::text AS timestamptz), $6, $7, $8)",
            &[
                &self.line, &r.id, &r.parents, &r.author, &r.time, &r.message, &r.tree, &r.seq,
            ],
        )?;
        Ok(())
    }

    fn refs(&self) -> Result<Vec<RefRow>, SessionError> {
        Ok(self
            .rows(
                "SELECT name, kind, revision, protected, user_name, ohl_time(time), note FROM vc_ref
                 WHERE line_id = $1 ORDER BY kind, name",
                &[&self.line],
            )?
            .iter()
            .map(|r| RefRow {
                name: r.get(0),
                kind: r.get(1),
                revision: r.get(2),
                protected: r.get(3),
                user: r.get(4),
                time: r.get(5),
                note: r.get(6),
            })
            .collect())
    }

    fn versions_initialised(&self, main: &str) -> Result<bool, SessionError> {
        let branch = self
            .row(
                "SELECT 1 FROM vc_ref WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
                &[&self.line, &main],
            )?
            .is_some();
        Ok(branch && self.meta("vc_head")?.is_some())
    }

    fn init_versions(&self, main: &str) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_ref (line_id, name, kind, revision, user_name)
             VALUES ($1, $2, 'branch', NULL, '') ON CONFLICT DO NOTHING",
            &[&self.line, &main],
        )?;
        self.exec(
            "INSERT INTO line_meta (line_id, key, value) VALUES ($1, 'vc_head', $2)
             ON CONFLICT DO NOTHING",
            &[&self.line, &format!("branch:{main}")],
        )?;
        Ok(())
    }

    fn add_branch(&self, name: &str, user: &str) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_ref (line_id, name, kind, revision, user_name)
             VALUES ($1, $2, 'branch', NULL, $3)",
            &[&self.line, &name, &user],
        )?;
        Ok(())
    }

    fn add_branch_noted(&self, name: &str, user: &str, note: &str) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_ref (line_id, name, kind, revision, user_name, note)
             VALUES ($1, $2, 'branch', NULL, $3, $4)",
            &[&self.line, &name, &user, &note],
        )?;
        Ok(())
    }

    fn add_line_branch(
        &self,
        name: &str,
        revision: Option<&str>,
        user: &str,
        note: &str,
    ) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_ref (line_id, name, kind, revision, user_name, note, base)
             VALUES ($1, $2, 'branch', $3, $4, $5, $3)",
            &[&self.line, &name, &revision, &user, &note],
        )?;
        Ok(())
    }

    fn add_tag(
        &self,
        name: &str,
        revision: &str,
        user: &str,
        note: &str,
    ) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_ref (line_id, name, kind, revision, user_name, note)
             VALUES ($1, $2, 'tag', $3, $4, $5)",
            &[&self.line, &name, &revision, &user, &note],
        )?;
        Ok(())
    }

    fn set_branch_revision(&self, name: &str, revision: &str) -> Result<(), SessionError> {
        self.exec(
            "UPDATE vc_ref SET revision = $3 WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
            &[&self.line, &name, &revision],
        )?;
        Ok(())
    }

    fn set_ref_revision(&self, name: &str, revision: &str) -> Result<(), SessionError> {
        self.exec(
            "UPDATE vc_ref SET revision = $3 WHERE line_id = $1 AND name = $2",
            &[&self.line, &name, &revision],
        )?;
        Ok(())
    }

    fn set_protected(&self, name: &str, protected: bool) -> Result<(), SessionError> {
        self.exec(
            "UPDATE vc_ref SET protected = $3 WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
            &[&self.line, &name, &protected],
        )?;
        Ok(())
    }

    fn delete_ref(&self, name: &str) -> Result<(), SessionError> {
        self.exec(
            "DELETE FROM vc_ref WHERE line_id = $1 AND name = $2",
            &[&self.line, &name],
        )?;
        Ok(())
    }

    fn grants(&self) -> Result<Vec<GrantRow>, SessionError> {
        Ok(self
            .rows(
                "SELECT id, subject, kinds, objects, area, user_name, ohl_time(time), note
                 FROM edit_grant WHERE line_id = $1 ORDER BY id",
                &[&self.line],
            )?
            .iter()
            .map(|r| GrantRow {
                id: r.get(0),
                subject: r.get(1),
                kinds: r.get(2),
                objects: r.get(3),
                area: r.get(4),
                user: r.get(5),
                time: r.get(6),
                note: r.get(7),
            })
            .collect())
    }

    fn add_grant(&self, g: &GrantRow) -> Result<i64, SessionError> {
        Ok(self
            .one(
                "INSERT INTO edit_grant (line_id, id, subject, kinds, objects, area, user_name, note)
                 SELECT $1, COALESCE(MAX(id), 0) + 1, $2, $3, $4, $5, $6, $7
                 FROM edit_grant WHERE line_id = $1
                 RETURNING id",
                &[
                    &self.line,
                    &g.subject,
                    &g.kinds,
                    &g.objects,
                    &g.area,
                    &g.user,
                    &g.note,
                ],
            )?
            .get(0))
    }

    fn delete_grant(&self, id: i64) -> Result<(), SessionError> {
        self.exec(
            "DELETE FROM edit_grant WHERE line_id = $1 AND id = $2",
            &[&self.line, &id],
        )?;
        Ok(())
    }

    fn reviews(&self) -> Result<Vec<ReviewRow>, SessionError> {
        Ok(self
            .rows(
                "SELECT id, branch, target, revision, author, ohl_time(time), note, reviewers, state
                 FROM vc_review WHERE line_id = $1 ORDER BY id DESC",
                &[&self.line],
            )?
            .iter()
            .map(|r| ReviewRow {
                id: r.get(0),
                branch: r.get(1),
                target: r.get(2),
                revision: r.get(3),
                author: r.get(4),
                time: r.get(5),
                note: r.get(6),
                reviewers: r.get(7),
                state: r.get(8),
            })
            .collect())
    }

    fn review_notes(&self) -> Result<Vec<ReviewNoteRow>, SessionError> {
        Ok(self
            .rows(
                "SELECT review, user_name, ohl_time(time), verdict, comment FROM vc_review_note
                 WHERE line_id = $1 ORDER BY seq",
                &[&self.line],
            )?
            .iter()
            .map(|r| ReviewNoteRow {
                review: r.get(0),
                user: r.get(1),
                time: r.get(2),
                verdict: r.get(3),
                comment: r.get(4),
            })
            .collect())
    }

    fn open_review(&self, branch: &str, target: &str) -> Result<Option<i64>, SessionError> {
        Ok(self
            .row(
                "SELECT id FROM vc_review WHERE line_id = $1 AND branch = $2 AND target = $3
                     AND state IN ('open', 'approved')",
                &[&self.line, &branch, &target],
            )?
            .map(|r| r.get(0)))
    }

    fn add_review(&self, r: &ReviewRow) -> Result<i64, SessionError> {
        Ok(self
            .one(
                "INSERT INTO vc_review (line_id, id, branch, target, revision, author, note,
                     reviewers, state)
                 SELECT $1, COALESCE(MAX(id), 0) + 1, $2, $3, $4, $5, $6, $7, 'open'
                 FROM vc_review WHERE line_id = $1
                 RETURNING id",
                &[
                    &self.line,
                    &r.branch,
                    &r.target,
                    &r.revision,
                    &r.author,
                    &r.note,
                    &r.reviewers,
                ],
            )?
            .get(0))
    }

    fn set_review_state(&self, id: i64, state: &str) -> Result<(), SessionError> {
        self.exec(
            "UPDATE vc_review SET state = $3 WHERE line_id = $1 AND id = $2",
            &[&self.line, &id, &state],
        )?;
        Ok(())
    }

    fn add_review_note(
        &self,
        review: i64,
        user: &str,
        verdict: &str,
        comment: &str,
    ) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_review_note (line_id, review, user_name, verdict, comment)
             VALUES ($1, $2, $3, $4, $5)",
            &[&self.line, &review, &user, &verdict, &comment],
        )?;
        Ok(())
    }

    fn surfaces(&self) -> Result<Vec<SurfaceRow>, SessionError> {
        Ok(self
            .rows(
                "SELECT name, ohl_time(time), user_name, source, points, triangles, bounds
                 FROM surface WHERE line_id = $1 ORDER BY name",
                &[&self.line],
            )?
            .iter()
            .map(|r| SurfaceRow {
                name: r.get(0),
                time: r.get(1),
                user: r.get(2),
                source: r.get(3),
                points: r.get(4),
                triangles: r.get(5),
                bounds: r.get(6),
            })
            .collect())
    }

    fn surface_data(&self, name: &str) -> Result<Option<Vec<u8>>, SessionError> {
        Ok(self
            .row(
                "SELECT data FROM surface WHERE line_id = $1 AND name = $2",
                &[&self.line, &name],
            )?
            .map(|r| r.get(0)))
    }

    fn put_surface(&self, s: &NewSurface<'_>) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO surface (line_id, name, user_name, source, points, triangles, bounds, data)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (line_id, name) DO UPDATE SET user_name = excluded.user_name,
                 source = excluded.source, points = excluded.points,
                 triangles = excluded.triangles, bounds = excluded.bounds, data = excluded.data,
                 time = clock_timestamp()",
            &[
                &self.line,
                &s.name,
                &s.user,
                &s.source,
                &s.points,
                &s.triangles,
                &s.bounds,
                &s.data,
            ],
        )?;
        Ok(())
    }

    fn rename_surface(&self, from: &str, to: &str) -> Result<usize, SessionError> {
        Ok(self.exec(
            "UPDATE surface SET name = $3 WHERE line_id = $1 AND name = $2",
            &[&self.line, &from, &to],
        )? as usize)
    }

    fn delete_surface(&self, name: &str) -> Result<usize, SessionError> {
        Ok(self.exec(
            "DELETE FROM surface WHERE line_id = $1 AND name = $2",
            &[&self.line, &name],
        )? as usize)
    }

    fn change_points(
        &self,
        cloud: &str,
        _points: &[PointChange],
    ) -> Result<(Vec<PointChange>, Vec<PointChange>), SessionError> {
        Err(SessionError::Refused(format!(
            "point cloud {cloud}: point clouds are not kept in the network's database yet \
             (they stay in the project files)"
        )))
    }

    fn points_still_there(
        &self,
        _cloud: &str,
        _points: &[PointChange],
    ) -> Result<bool, SessionError> {
        Ok(false)
    }

    fn points_entry(&self, _grid: &Grid, _cloud: &str, _points: &[PointChange]) -> Option<Entry> {
        None
    }

    fn points_state(&self) -> String {
        // No point clouds: the state is the last entry naming one (none
        // can be applied) and an empty list.
        let seq: i64 = self
            .row(
                "SELECT COALESCE(MAX(seq), 0) FROM oplog WHERE line_id = $1
                     AND objects LIKE '%\"point_cloud\"%'",
                &[&self.line],
            )
            .ok()
            .flatten()
            .map_or(0, |r| r.get(0));
        format!("{seq}|")
    }

    fn meta_rows(&self) -> Result<Vec<(String, String)>, SessionError> {
        Ok(self
            .rows(
                "SELECT key, value FROM line_meta WHERE line_id = $1 ORDER BY key",
                &[&self.line],
            )?
            .iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect())
    }

    fn blob_hashes(&self) -> Result<Vec<String>, SessionError> {
        Ok(self
            .rows(
                "SELECT hash FROM vc_blob_use WHERE line_id = $1 ORDER BY hash",
                &[&self.line],
            )?
            .iter()
            .map(|r| r.get(0))
            .collect())
    }

    fn put_entry_row(&self, e: &EntryRow) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO oplog (line_id, seq, time, user_name, kind, command, inverse, objects,
                 sections, undoes, undone_by, note, branch)
             VALUES ($1, $2, CAST($3::text AS timestamptz), $4, $5, $6, $7, $8, $9, $10, $11,
                 $12, $13)",
            &[
                &self.line, &e.0, &e.1, &e.2, &e.3, &e.4, &e.5, &e.6, &e.7, &e.8, &e.9, &e.10,
                &e.11,
            ],
        )?;
        Ok(())
    }

    fn put_ref_row(
        &self,
        r: &RefRow,
        description: &str,
        base: Option<&str>,
    ) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_ref (line_id, name, kind, revision, protected, user_name, time, note,
                 description, base)
             VALUES ($1, $2, $3, $4, $5, $6, CAST($7::text AS timestamptz), $8, $9, $10)
             ON CONFLICT (line_id, name) DO UPDATE SET kind = excluded.kind,
                 revision = excluded.revision, protected = excluded.protected,
                 user_name = excluded.user_name, time = excluded.time, note = excluded.note,
                 description = excluded.description, base = excluded.base",
            &[
                &self.line,
                &r.name,
                &r.kind,
                &r.revision,
                &r.protected,
                &r.user,
                &r.time,
                &r.note,
                &description,
                &base,
            ],
        )?;
        Ok(())
    }

    fn put_grant_row(&self, g: &GrantRow) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO edit_grant (line_id, id, subject, kinds, objects, area, user_name, time,
                 note)
             VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8::text AS timestamptz), $9)",
            &[
                &self.line, &g.id, &g.subject, &g.kinds, &g.objects, &g.area, &g.user, &g.time,
                &g.note,
            ],
        )?;
        Ok(())
    }

    fn put_review_row(&self, r: &ReviewRow) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_review (line_id, id, branch, target, revision, author, time, note,
                 reviewers, state)
             VALUES ($1, $2, $3, $4, $5, $6, CAST($7::text AS timestamptz), $8, $9, $10)",
            &[
                &self.line,
                &r.id,
                &r.branch,
                &r.target,
                &r.revision,
                &r.author,
                &r.time,
                &r.note,
                &r.reviewers,
                &r.state,
            ],
        )?;
        Ok(())
    }

    fn put_review_note_row(&self, n: &ReviewNoteRow) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO vc_review_note (line_id, review, user_name, time, verdict, comment)
             VALUES ($1, $2, $3, CAST($4::text AS timestamptz), $5, $6)",
            &[
                &self.line, &n.review, &n.user, &n.time, &n.verdict, &n.comment,
            ],
        )?;
        Ok(())
    }

    fn put_surface_row(&self, s: &SurfaceRow, data: &[u8]) -> Result<(), SessionError> {
        self.exec(
            "INSERT INTO surface (line_id, name, time, user_name, source, points, triangles,
                 bounds, data)
             VALUES ($1, $2, CAST($3::text AS timestamptz), $4, $5, $6, $7, $8, $9)
             ON CONFLICT (line_id, name) DO UPDATE SET time = excluded.time,
                 user_name = excluded.user_name, source = excluded.source,
                 points = excluded.points, triangles = excluded.triangles,
                 bounds = excluded.bounds, data = excluded.data",
            &[
                &self.line,
                &s.name,
                &s.time,
                &s.user,
                &s.source,
                &s.points,
                &s.triangles,
                &s.bounds,
                &data,
            ],
        )?;
        Ok(())
    }
}
