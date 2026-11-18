// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The project file (`.ohl`, SQLite, ADR-0004) as a [`Store`]: the
//! statements the session ran itself before phase 2 of ADR-0013, moved
//! here unchanged — the same rows, in the same order, so a file written
//! through the store is the file written before (tested by comparing
//! `sqlite3 .dump`s). Its tables, schema 6, and the upgrades of older
//! files (`SqliteStore::prepare`) are this store's own.

use std::collections::BTreeMap;
use std::path::Path;

use ohl_core::grid::{Bounds, Grid, Tile};
use rusqlite::{Connection, OptionalExtension, params};

use super::{
    EntryRow, GrantRow, LegacySnapshot, NewEntry, NewSurface, ObjectRow, RefRow, ReviewNoteRow,
    ReviewRow, RevisionRow, Store, SurfaceRow,
};
use crate::points::PointChange;
use crate::spatial::Entry;
use crate::{
    Kind, SCHEMA_VERSION, SessionError, harden, library_id, library_value, safe_file_name,
};

/// The spatial index's table (ADR-0008).
pub(crate) const SPATIAL_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS spatial_index (
        line TEXT NOT NULL DEFAULT 'main',
        kind TEXT NOT NULL, id TEXT NOT NULL, level INTEGER NOT NULL, tx INTEGER NOT NULL,
        ty INTEGER NOT NULL, xmin REAL NOT NULL, ymin REAL NOT NULL, xmax REAL NOT NULL,
        ymax REAL NOT NULL, PRIMARY KEY (line, kind, id));
     CREATE INDEX IF NOT EXISTS spatial_tile ON spatial_index (line, level, tx, ty);";

/// The terrain surfaces' table.
pub(crate) const SURFACES_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS surface (
    name TEXT PRIMARY KEY,
    time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    user TEXT NOT NULL, source TEXT NOT NULL,
    points INTEGER NOT NULL, triangles INTEGER NOT NULL,
    bounds TEXT NOT NULL, data BLOB NOT NULL);";

/// The version store's tables (ADR-0010).
pub(crate) const VERSIONS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS vc_blob (
        hash TEXT PRIMARY KEY, data BLOB NOT NULL);
    CREATE TABLE IF NOT EXISTS vc_revision (
        id TEXT PRIMARY KEY, parents TEXT NOT NULL, author TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        message TEXT NOT NULL, tree TEXT NOT NULL, seq INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS vc_ref (
        name TEXT PRIMARY KEY, kind TEXT NOT NULL, revision TEXT,
        protected INTEGER NOT NULL DEFAULT 0, user TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        note TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '', base TEXT);";

/// The edit grants' and reviews' tables.
pub(crate) const COLLAB_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS edit_grant (
        id INTEGER PRIMARY KEY, subject TEXT NOT NULL, kinds TEXT NOT NULL,
        objects TEXT NOT NULL, area TEXT, user TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        note TEXT NOT NULL DEFAULT '');
    CREATE TABLE IF NOT EXISTS vc_review (
        id INTEGER PRIMARY KEY, branch TEXT NOT NULL, target TEXT NOT NULL,
        revision TEXT NOT NULL, author TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        note TEXT NOT NULL, reviewers TEXT NOT NULL, state TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS vc_review_note (
        seq INTEGER PRIMARY KEY, review INTEGER NOT NULL, user TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        verdict TEXT NOT NULL, comment TEXT NOT NULL);";

/// A project file opened for the session.
///
/// In this crate's tests it reads as its SQLite connection (`Deref`), so
/// that tests can look into the file; the session itself reaches it only
/// through [`Store`].
pub struct SqliteStore {
    conn: Connection,
}

#[cfg(test)]
impl std::ops::Deref for SqliteStore {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.conn
    }
}

/// The SQLite URI reading `path` read-only (`immutable`: as a file no
/// program changes meanwhile — no locks, no `-wal` or `-shm`): the
/// characters a URI path gives a meaning (`%`, `?`, `#`) and spaces
/// escaped, a Windows path's backslashes as slashes.
fn read_only_uri(path: &Path, immutable: bool) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut escaped = String::new();
    for c in text.chars() {
        match c {
            '%' => escaped.push_str("%25"),
            '?' => escaped.push_str("%3f"),
            '#' => escaped.push_str("%23"),
            ' ' => escaped.push_str("%20"),
            c => escaped.push(c),
        }
    }
    // A drive letter (C:/…) after a slash, as SQLite wants it.
    if escaped.as_bytes().get(1) == Some(&b':') {
        escaped.insert(0, '/');
    }
    format!(
        "file:{escaped}?mode=ro{}",
        if immutable { "&immutable=1" } else { "" }
    )
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, SessionError> {
    Ok(conn
        .prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?
        .exists(params![table, column])?)
}

fn entry_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
        r.get(11)?,
    ))
}

fn revision_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RevisionRow> {
    Ok(RevisionRow {
        id: r.get(0)?,
        parents: r.get(1)?,
        author: r.get(2)?,
        time: r.get(3)?,
        message: r.get(4)?,
        tree: r.get(5)?,
        seq: r.get(6)?,
    })
}

type SpatialRow = (String, String, Entry);

fn spatial_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SpatialRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        Entry {
            tile: Tile {
                level: r.get(2)?,
                x: r.get(3)?,
                y: r.get(4)?,
            },
            bounds: Bounds {
                min: [r.get(5)?, r.get(6)?],
                max: [r.get(7)?, r.get(8)?],
            },
        },
    ))
}

impl SqliteStore {
    /// Opens the project file at `path` to write it (created if missing).
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        let conn = Connection::open(path)?;
        harden(&conn)?;
        Ok(Self { conn })
    }

    /// Opens the existing project file at `path` read-only (as immutable
    /// when no program has it open: no `-wal` beside it).
    pub fn open_read_only(path: &Path) -> Result<Self, SessionError> {
        use rusqlite::OpenFlags;
        let mut wal = path.as_os_str().to_owned();
        wal.push("-wal");
        let immutable = !Path::new(&wal).exists();
        let uri = read_only_uri(path, immutable);
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(&uri, flags)?;
        harden(&conn)?;
        Ok(Self { conn })
    }

    /// A copy in memory of the project file at `path` (read as
    /// [`SqliteStore::open_read_only`] reads it), to upgrade without
    /// touching the file.
    pub fn copy_in_memory(path: &Path) -> Result<Self, SessionError> {
        let from = Self::open_read_only(path)?;
        let mut copy = Connection::open_in_memory()?;
        rusqlite::backup::Backup::new(&from.conn, &mut copy)?.run_to_completion(
            1024,
            std::time::Duration::ZERO,
            None,
        )?;
        drop(from);
        harden(&copy)?;
        Ok(Self { conn: copy })
    }

    /// Makes and upgrades the tables of a project file and returns its
    /// chunk grid: a newer file refused before anything in it changes;
    /// the journal mode set (`writable`); the tables made; the upgrade
    /// steps of older files; the grid fixed when the project is made; the
    /// schema number; the version store's main branch and head; the
    /// library files of schema 3 and before made objects. On a read-only
    /// connection anything that writes fails with SQLite's "readonly
    /// database" error.
    pub(crate) fn prepare(&self, path: &Path, writable: bool) -> Result<Grid, SessionError> {
        let conn = &self.conn;
        // A newer project file is refused before anything in it changes.
        let has_meta = conn
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'ols_meta'")?
            .exists([])?;
        if has_meta {
            let version: Option<String> = conn
                .query_row(
                    "SELECT value FROM ols_meta WHERE key = 'session_schema'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(v) = version
                && v.parse::<u32>().ok().is_none_or(|v| v > SCHEMA_VERSION)
            {
                return Err(SessionError::Store(format!(
                    "{}: session schema {v} newer than supported ({SCHEMA_VERSION})",
                    path.display()
                )));
            }
        }
        if writable {
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ols_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS session_user (name TEXT PRIMARY KEY, role TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS model_object (
                 line TEXT NOT NULL DEFAULT 'main',
                 kind TEXT NOT NULL, id TEXT NOT NULL, ord INTEGER NOT NULL,
                 version INTEGER NOT NULL, json TEXT NOT NULL, gone INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (line, kind, id));
             CREATE TABLE IF NOT EXISTS oplog (
                 seq INTEGER PRIMARY KEY, time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 user TEXT NOT NULL, kind TEXT NOT NULL, command TEXT NOT NULL, inverse TEXT NOT NULL,
                 objects TEXT NOT NULL, sections TEXT NOT NULL,
                 undoes INTEGER, undone_by INTEGER, note TEXT,
                 line TEXT NOT NULL DEFAULT 'main');",
        )?;
        // Steps of older files, before the tables they need are made.
        if has_meta {
            self.upgrade_before_tables()?;
        }
        conn.execute_batch(SPATIAL_SCHEMA)?;
        conn.execute_batch(SURFACES_SCHEMA)?;
        conn.execute_batch(VERSIONS_SCHEMA)?;
        conn.execute_batch(COLLAB_SCHEMA)?;
        if has_meta {
            self.migrate_descriptions()?;
        }
        // The chunk grid is fixed when the project is created.
        let grid_size: Option<String> = conn
            .query_row(
                "SELECT value FROM ols_meta WHERE key = 'grid_size'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let grid = match grid_size {
            Some(text) => text
                .parse::<f64>()
                .map_err(|e| e.to_string())
                .and_then(Grid::new)
                .map_err(|e| SessionError::Store(format!("grid_size \"{text}\": {e}")))?,
            None => {
                let g = Grid::default();
                conn.execute(
                    "INSERT INTO ols_meta (key, value) VALUES ('grid_size', ?1)",
                    params![g.size.to_string()],
                )?;
                g
            }
        };
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM ols_meta WHERE key = 'session_schema'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match version.as_deref().map(str::parse::<u32>) {
            None => {
                conn.execute(
                    "INSERT INTO ols_meta (key, value) VALUES ('session_schema', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )?;
            }
            Some(Ok(v)) if v == SCHEMA_VERSION => {}
            // Older files: the new tables were just made; nothing else
            // changes.
            Some(Ok(v)) if v < SCHEMA_VERSION => {
                conn.execute(
                    "UPDATE ols_meta SET value = ?1 WHERE key = 'session_schema'",
                    params![SCHEMA_VERSION.to_string()],
                )?;
            }
            Some(_) => {
                return Err(SessionError::Store(format!(
                    "{}: session schema newer than supported ({SCHEMA_VERSION})",
                    path.display()
                )));
            }
        }
        // Read-only: written only when missing (else a readonly error and
        // the copy in memory, [`crate::Session::open_read_only`]).
        let main = crate::versions::MAIN;
        if writable || !self.versions_initialised(main)? {
            self.init_versions(main)?;
        }
        if has_meta {
            self.migrate_library_files()?;
        }
        Ok(grid)
    }

    /// Upgrade steps of older project files that come before the current
    /// tables are made, in schema order. A new file has none of them. Each
    /// step looks for what it changes, so a file of an in-between build
    /// (its schema number already raised) is upgraded too, and a done step
    /// does nothing; the steps after the tables are
    /// [`SqliteStore::migrate_descriptions`] (schema 3) and
    /// [`SqliteStore::migrate_library_files`] (schema 4).
    fn upgrade_before_tables(&self) -> Result<(), SessionError> {
        let conn = &self.conn;
        // Schema 1, before its first release: the log's note column.
        let has_note: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('oplog') WHERE name = 'note'")?
            .exists([])?;
        if !has_note {
            conn.execute("ALTER TABLE oplog ADD COLUMN note TEXT", [])?;
        }
        // Schema 3: every project line has its own objects, log and index.
        self.migrate_lines()?;
        // Schema 6: lines kept as differences have tombstones.
        self.migrate_tombstones()
    }

    /// Schema 2 → 3: the objects, log and index of each project line
    /// (those of earlier files become the main line's). The index is
    /// rebuilt from the objects when the session opens.
    fn migrate_lines(&self) -> Result<(), SessionError> {
        let conn = &self.conn;
        if !has_column(conn, "model_object", "line")? {
            let tx = crate::Step::begin(self)?;
            conn.execute_batch(
                "ALTER TABLE model_object RENAME TO model_object_v2;
                 CREATE TABLE model_object (
                     line TEXT NOT NULL DEFAULT 'main',
                     kind TEXT NOT NULL, id TEXT NOT NULL, ord INTEGER NOT NULL,
                     version INTEGER NOT NULL, json TEXT NOT NULL, PRIMARY KEY (line, kind, id));
                 INSERT INTO model_object (line, kind, id, ord, version, json)
                     SELECT 'main', kind, id, ord, version, json FROM model_object_v2;
                 DROP TABLE model_object_v2;",
            )?;
            tx.commit()?;
        }
        if !has_column(conn, "oplog", "line")? {
            conn.execute(
                "ALTER TABLE oplog ADD COLUMN line TEXT NOT NULL DEFAULT 'main'",
                [],
            )?;
        }
        let has_index: bool = conn
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'spatial_index'")?
            .exists([])?;
        if has_index && !has_column(conn, "spatial_index", "line")? {
            conn.execute_batch("DROP INDEX IF EXISTS spatial_tile; DROP TABLE spatial_index;")?;
        }
        Ok(())
    }

    /// Lines get a description (added within schema 3), and (schema 6)
    /// the revision they keep their differences from (`base`; none: the
    /// line keeps all its objects, as every line of older files).
    fn migrate_descriptions(&self) -> Result<(), SessionError> {
        let conn = &self.conn;
        if !has_column(conn, "vc_ref", "description")? {
            conn.execute(
                "ALTER TABLE vc_ref ADD COLUMN description TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !has_column(conn, "vc_ref", "base")? {
            conn.execute("ALTER TABLE vc_ref ADD COLUMN base TEXT", [])?;
        }
        Ok(())
    }

    /// Schema 6: rows of `model_object` may be tombstones (`gone`: an
    /// object of the line's base deleted on the line). Nothing else
    /// changes: the lines of older files keep all their objects.
    fn migrate_tombstones(&self) -> Result<(), SessionError> {
        if !has_column(&self.conn, "model_object", "gone")? {
            self.conn.execute(
                "ALTER TABLE model_object ADD COLUMN gone INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    /// Schema 4: library files of the `library_file` table (schema 3 and
    /// before: one set for all project lines) made `library` objects of
    /// every project line, then the table dropped. Nothing to do once
    /// done.
    fn migrate_library_files(&self) -> Result<(), SessionError> {
        let conn = &self.conn;
        let exists = conn
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'library_file'")?
            .exists([])?;
        if !exists {
            return Ok(());
        }
        let files: Vec<(String, String, String)> = conn
            .prepare("SELECT kind, name, text FROM library_file ORDER BY kind, name")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        let mut lines: Vec<String> = conn
            .prepare("SELECT DISTINCT line FROM model_object")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if !lines.iter().any(|l| l == crate::versions::MAIN) {
            lines.push(crate::versions::MAIN.to_owned());
        }
        let tx = conn.unchecked_transaction()?;
        for line in &lines {
            for (k, (kind, name, text)) in files.iter().enumerate() {
                // Names that were refused when put are left out.
                if kind.is_empty() || kind.contains('/') || !safe_file_name(name) {
                    continue;
                }
                tx.execute(
                    "INSERT OR IGNORE INTO model_object (line, kind, id, ord, version, json)
                     VALUES (?1, 'library', ?2, ?3, 1, ?4)",
                    params![
                        line,
                        library_id(kind, name),
                        k as i64,
                        library_value(kind, name, text).to_string()
                    ],
                )?;
            }
        }
        tx.execute("DROP TABLE library_file", [])?;
        tx.commit()?;
        Ok(())
    }

    /// Marks the upgrades into routes, alignment points and structure
    /// groups done (meta rows replaced).
    pub(crate) fn mark_object_upgrades_done(&self) -> Result<(), SessionError> {
        self.conn.execute_batch(
            "INSERT OR REPLACE INTO ols_meta (key, value) VALUES ('routes_alignments', '1');
             INSERT OR REPLACE INTO ols_meta (key, value) VALUES ('alignment_points', '1');
             INSERT OR REPLACE INTO ols_meta (key, value) VALUES ('structure_groups', '1');",
        )?;
        Ok(())
    }
}

impl Store for SqliteStore {
    fn savepoint(&self, name: &str) -> Result<(), SessionError> {
        self.conn.execute_batch(&format!("SAVEPOINT {name}"))?;
        Ok(())
    }

    fn release(&self, name: &str) -> Result<(), SessionError> {
        self.conn.execute_batch(&format!("RELEASE {name}"))?;
        Ok(())
    }

    fn roll_back_to(&self, name: &str) -> Result<(), SessionError> {
        self.conn
            .execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"))?;
        Ok(())
    }

    fn copy_to(&self, path: &Path) -> Result<(), SessionError> {
        self.conn
            .execute("VACUUM INTO ?1", params![path.to_string_lossy()])?;
        Ok(())
    }

    fn copy_for_others(&self, path: &Path) -> Result<(), SessionError> {
        self.copy_to(path)?;
        let conn = Connection::open(path)?;
        harden(&conn)?;
        let mut stmt =
            conn.prepare("SELECT key, value FROM ols_meta WHERE key LIKE 'setting.%'")?;
        let local: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<(String, String)>, _>>()?
            .into_iter()
            .filter(|(_, v)| crate::is_local_path(v))
            .collect();
        drop(stmt);
        for (key, value) in local {
            let name = value
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .to_owned();
            conn.execute(
                "UPDATE ols_meta SET value = ?2 WHERE key = ?1",
                params![key, name],
            )?;
        }
        Ok(())
    }

    fn checkpoint(&self) -> Result<(), SessionError> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    fn now(&self) -> Result<String, SessionError> {
        Ok(self
            .conn
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |r| {
                r.get(0)
            })?)
    }

    fn meta(&self, key: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM ols_meta WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO ols_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    fn delete_meta(&self, key: &str) -> Result<(), SessionError> {
        self.conn
            .execute("DELETE FROM ols_meta WHERE key = ?1", params![key])?;
        Ok(())
    }

    fn user_count(&self) -> Result<i64, SessionError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM session_user", [], |r| r.get(0))?)
    }

    fn put_user(&self, name: &str, role: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO session_user (name, role) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET role = excluded.role",
            params![name, role],
        )?;
        Ok(())
    }

    fn user_role(&self, name: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT role FROM session_user WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn users(&self) -> Result<Vec<(String, String)>, SessionError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, role FROM session_user ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn object_rows(&self) -> Result<Vec<ObjectRow>, SessionError> {
        let mut stmt = self
            .conn
            .prepare("SELECT line, kind, id, ord, version, json, gone FROM model_object")?;
        let rows = stmt.query_map([], |r| {
            Ok(ObjectRow {
                line: r.get(0)?,
                kind: r.get(1)?,
                id: r.get(2)?,
                ord: r.get(3)?,
                version: r.get(4)?,
                json: r.get(5)?,
                gone: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn write_object(
        &self,
        line: &str,
        kind: Kind,
        id: &str,
        object: Option<(i64, i64, &str)>,
    ) -> Result<(), SessionError> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO model_object (line, kind, id, ord, version, json, gone) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(line, kind, id) DO UPDATE SET ord = excluded.ord, version = excluded.version,
                 json = excluded.json, gone = excluded.gone",
        )?;
        match object {
            Some((ord, version, json)) => {
                stmt.execute(params![line, kind.as_str(), id, ord, version, json, false])?
            }
            None => stmt.execute(params![line, kind.as_str(), id, 0, 0, "null", true])?,
        };
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
        self.conn.execute(
            "INSERT OR REPLACE INTO model_object (line, kind, id, ord, version, json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![line, kind.as_str(), id, ord, version, json],
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
        self.conn
            .prepare_cached(
                "INSERT INTO model_object (line, kind, id, ord, version, json, gone) VALUES (?1, ?2, ?3, ?4, ?5, 'null', 2)
                 ON CONFLICT(line, kind, id) DO UPDATE SET ord = excluded.ord, version = excluded.version,
                     json = excluded.json, gone = excluded.gone",
            )?
            .execute(params![line, kind.as_str(), id, ord, version])?;
        Ok(())
    }

    fn delete_object(&self, line: &str, kind: Kind, id: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "DELETE FROM model_object WHERE line = ?1 AND kind = ?2 AND id = ?3",
            params![line, kind.as_str(), id],
        )?;
        Ok(())
    }

    fn line_base(&self, name: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .conn
            .prepare_cached("SELECT base FROM vc_ref WHERE name = ?1 AND kind = 'branch'")?
            .query_row(params![name], |r| r.get::<_, Option<String>>(0))
            .optional()?
            .flatten())
    }

    fn line_description(&self, name: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT description FROM vc_ref WHERE name = ?1 AND kind = 'branch'",
                params![name],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn set_line_description(&self, name: &str, description: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE vc_ref SET description = ?2 WHERE name = ?1 AND kind = 'branch'",
            params![name, description],
        )?;
        Ok(())
    }

    fn set_line_base(&self, name: &str, base: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE vc_ref SET base = ?1 WHERE name = ?2 AND kind = 'branch'",
            params![base, name],
        )?;
        Ok(())
    }

    fn delete_line(&self, name: &str) -> Result<(), SessionError> {
        self.conn
            .execute("DELETE FROM model_object WHERE line = ?1", params![name])?;
        self.conn
            .execute("DELETE FROM spatial_index WHERE line = ?1", params![name])?;
        self.conn.execute(
            "DELETE FROM vc_ref WHERE name = ?1 AND kind = 'branch'",
            params![name],
        )?;
        Ok(())
    }

    fn rename_line(&self, from: &str, to: &str) -> Result<(), SessionError> {
        for table in ["model_object", "oplog", "spatial_index"] {
            self.conn.execute(
                &format!("UPDATE {table} SET line = ?2 WHERE line = ?1"),
                params![from, to],
            )?;
        }
        self.conn.execute(
            "UPDATE vc_ref SET name = ?2 WHERE name = ?1 AND kind = 'branch'",
            params![from, to],
        )?;
        self.conn.execute(
            "UPDATE vc_review SET branch = ?2 WHERE branch = ?1",
            params![from, to],
        )?;
        self.conn.execute(
            "UPDATE vc_review SET target = ?2 WHERE target = ?1",
            params![from, to],
        )?;
        Ok(())
    }

    fn legacy_snapshots(&self) -> Result<Option<Vec<LegacySnapshot>>, SessionError> {
        // Files made since schema 4 have no such table.
        let has_saved = self
            .conn
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'line_snapshot'")?
            .exists([])?;
        if !has_saved {
            return Ok(None);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT name, user, note, json FROM line_snapshot ORDER BY time, name")?;
        let saved = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?;
        Ok(Some(saved))
    }

    fn delete_legacy_snapshot(&self, name: &str) -> Result<(), SessionError> {
        self.conn
            .execute("DELETE FROM line_snapshot WHERE name = ?1", params![name])?;
        Ok(())
    }

    fn drop_legacy_snapshots(&self) -> Result<(), SessionError> {
        self.conn.execute("DROP TABLE line_snapshot", [])?;
        Ok(())
    }

    fn append_entry(&self, e: &NewEntry<'_>) -> Result<i64, SessionError> {
        self.conn.execute(
            "INSERT INTO oplog (user, kind, command, inverse, objects, sections, undoes, note, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                e.user,
                e.kind,
                e.command,
                e.inverse,
                e.objects,
                e.sections,
                e.undoes,
                e.note,
                e.line
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn mark_undone(&self, seq: i64, by: i64) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE oplog SET undone_by = ?1 WHERE seq = ?2",
            params![by, seq],
        )?;
        Ok(())
    }

    fn mark_undone_after(&self, by: i64, seq: i64, line: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE oplog SET undone_by = ?1 WHERE seq > ?2 AND seq < ?1 AND undone_by IS NULL AND line = ?3",
            params![by, seq, line],
        )?;
        Ok(())
    }

    fn entries(&self) -> Result<Vec<EntryRow>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by, note, line FROM oplog ORDER BY seq",
        )?;
        let rows = stmt.query_map([], entry_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn entry(&self, seq: i64) -> Result<Option<EntryRow>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by, note, line FROM oplog WHERE seq = ?1",
                params![seq],
                entry_row,
            )
            .optional()?)
    }

    fn entry_objects(&self, seq: i64) -> Result<Option<String>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT objects FROM oplog WHERE seq = ?1",
                params![seq],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn note_reversal(&self, seq: i64, reversed: i64) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE oplog SET note = 'reverses ' || ?2 || COALESCE(': ' || (SELECT note FROM oplog WHERE seq = ?2), '') WHERE seq = ?1",
            params![seq, reversed],
        )?;
        Ok(())
    }

    fn set_entry_note(&self, seq: i64, note: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE oplog SET note = ?1 WHERE seq = ?2",
            params![note, seq],
        )?;
        Ok(())
    }

    fn last_undoable(&self, user: &str, line: &str) -> Result<Option<i64>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq FROM oplog WHERE user = ?1 AND line = ?2 AND kind IN ('do', 'redo') AND undone_by IS NULL ORDER BY seq DESC LIMIT 1",
                params![user, line],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn last_own_entry(
        &self,
        user: &str,
        line: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, kind, undone_by FROM oplog WHERE user = ?1 AND line = ?2 AND kind IN ('do', 'undo', 'redo') ORDER BY seq DESC LIMIT 1",
                params![user, line],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    fn inverses_after(&self, seq: i64, line: &str) -> Result<Vec<String>, SessionError> {
        let mut stmt = self
            .conn
            .prepare("SELECT inverse FROM oplog WHERE seq > ?1 AND line = ?2 ORDER BY seq DESC")?;
        let out = stmt
            .query_map(params![seq, line], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }

    fn last_seq(&self) -> Result<i64, SessionError> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(MAX(seq), 0) FROM oplog", [], |r| r.get(0))?)
    }

    fn last_seq_of_line(&self, line: &str) -> Result<i64, SessionError> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM oplog WHERE line = ?1",
            [line],
            |r| r.get(0),
        )?)
    }

    fn spatial_write_diff(
        &self,
        line: &str,
        old: &BTreeMap<(Kind, String), Entry>,
        new: &BTreeMap<(Kind, String), Entry>,
    ) -> Result<usize, SessionError> {
        let conn = &self.conn;
        let mut changed = 0;
        for ((kind, id), _) in old.iter().filter(|(k, _)| !new.contains_key(*k)) {
            conn.execute(
                "DELETE FROM spatial_index WHERE line = ?1 AND kind = ?2 AND id = ?3",
                params![line, kind.as_str(), id],
            )?;
            changed += 1;
        }
        let mut stmt = conn.prepare_cached(
            "INSERT INTO spatial_index (kind, id, level, tx, ty, xmin, ymin, xmax, ymax, line)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(line, kind, id) DO UPDATE SET level = excluded.level, tx = excluded.tx,
                 ty = excluded.ty, xmin = excluded.xmin, ymin = excluded.ymin,
                 xmax = excluded.xmax, ymax = excluded.ymax",
        )?;
        for ((kind, id), e) in new {
            if old.get(&(*kind, id.clone())) == Some(e) {
                continue;
            }
            let b = e.bounds;
            stmt.execute(params![
                kind.as_str(),
                id,
                e.tile.level,
                e.tile.x,
                e.tile.y,
                b.min[0],
                b.min[1],
                b.max[0],
                b.max[1],
                line
            ])?;
            changed += 1;
        }
        Ok(changed)
    }

    fn spatial_load(&self, line: &str) -> Result<BTreeMap<(Kind, String), Entry>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT kind, id, level, tx, ty, xmin, ymin, xmax, ymax FROM spatial_index WHERE line = ?1",
        )?;
        let rows = stmt.query_map(params![line], spatial_row)?;
        let mut out = BTreeMap::new();
        for r in rows {
            let (kind, id, e) = r?;
            out.insert((Kind::parse(&kind)?, id), e);
        }
        Ok(out)
    }

    fn spatial_query(
        &self,
        line: &str,
        grid: &Grid,
        area: &Bounds,
    ) -> Result<Vec<(Kind, String, Entry)>, SessionError> {
        let conn = &self.conn;
        let levels: Vec<i8> = conn
            .prepare("SELECT DISTINCT level FROM spatial_index WHERE line = ?1 ORDER BY level")?
            .query_map(params![line], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut stmt = conn.prepare_cached(
            "SELECT kind, id, level, tx, ty, xmin, ymin, xmax, ymax FROM spatial_index
             WHERE level = ?1 AND tx BETWEEN ?2 AND ?3 AND ty BETWEEN ?4 AND ?5
               AND xmax >= ?6 AND xmin <= ?7 AND ymax >= ?8 AND ymin <= ?9 AND line = ?10",
        )?;
        let mut out = Vec::new();
        for level in levels {
            let r = grid.query(area, level);
            let rows = stmt.query_map(
                params![
                    level,
                    r.x[0],
                    r.x[1],
                    r.y[0],
                    r.y[1],
                    area.min[0],
                    area.max[0],
                    area.min[1],
                    area.max[1],
                    line
                ],
                spatial_row,
            )?;
            for x in rows {
                let (kind, id, e) = x?;
                out.push((Kind::parse(&kind)?, id, e));
            }
        }
        out.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        Ok(out)
    }

    fn has_blob(&self, hash: &str) -> Result<bool, SessionError> {
        Ok(self
            .conn
            .prepare_cached("SELECT 1 FROM vc_blob WHERE hash = ?1")?
            .exists(params![hash])?)
    }

    fn put_blob(&self, hash: &str, data: &[u8]) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO vc_blob (hash, data) VALUES (?1, ?2)",
            params![hash, data],
        )?;
        Ok(())
    }

    fn blob(&self, hash: &str) -> Result<Option<Vec<u8>>, SessionError> {
        Ok(self
            .conn
            .prepare_cached("SELECT data FROM vc_blob WHERE hash = ?1")?
            .query_row(params![hash], |r| r.get(0))
            .optional()?)
    }

    fn revision(&self, id: &str) -> Result<Option<RevisionRow>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, parents, author, time, message, tree, seq FROM vc_revision WHERE id = ?1",
                params![id],
                revision_row,
            )
            .optional()?)
    }

    fn revisions(&self) -> Result<Vec<RevisionRow>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parents, author, time, message, tree, seq FROM vc_revision ORDER BY rowid DESC",
        )?;
        let rows = stmt.query_map([], revision_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn revisions_starting(&self, prefix: &str) -> Result<Vec<String>, SessionError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM vc_revision WHERE id LIKE ?1 || '%' LIMIT 2")?;
        let found = stmt
            .query_map(params![prefix], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(found)
    }

    fn has_revision(&self, id: &str) -> Result<bool, SessionError> {
        Ok(self
            .conn
            .prepare("SELECT 1 FROM vc_revision WHERE id = ?1")?
            .exists(params![id])?)
    }

    fn put_revision(&self, r: &RevisionRow) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO vc_revision (id, parents, author, message, tree, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![r.id, r.parents, r.author, r.message, r.tree, r.seq],
        )?;
        Ok(())
    }

    fn put_revision_with_time(&self, r: &RevisionRow) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO vc_revision (id, parents, author, time, message, tree, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![r.id, r.parents, r.author, r.time, r.message, r.tree, r.seq],
        )?;
        Ok(())
    }

    fn refs(&self) -> Result<Vec<RefRow>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kind, revision, protected, user, time, note FROM vc_ref ORDER BY kind, name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(RefRow {
                name: r.get(0)?,
                kind: r.get(1)?,
                revision: r.get(2)?,
                protected: r.get(3)?,
                user: r.get(4)?,
                time: r.get(5)?,
                note: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn versions_initialised(&self, main: &str) -> Result<bool, SessionError> {
        let branch = self
            .conn
            .prepare("SELECT 1 FROM vc_ref WHERE name = ?1 AND kind = 'branch'")?
            .exists(params![main])?;
        let head = self
            .conn
            .prepare("SELECT 1 FROM ols_meta WHERE key = 'vc_head'")?
            .exists([])?;
        Ok(branch && head)
    }

    fn init_versions(&self, main: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO vc_ref (name, kind, revision, user) VALUES (?1, 'branch', NULL, '')",
            params![main],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO ols_meta (key, value) VALUES ('vc_head', ?1)",
            params![format!("branch:{main}")],
        )?;
        Ok(())
    }

    fn add_branch(&self, name: &str, user: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO vc_ref (name, kind, revision, user) VALUES (?1, 'branch', NULL, ?2)",
            params![name, user],
        )?;
        Ok(())
    }

    fn add_branch_noted(&self, name: &str, user: &str, note: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO vc_ref (name, kind, revision, user, note) VALUES (?1, 'branch', NULL, ?2, ?3)",
            params![name, user, note],
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
        self.conn.execute(
            "INSERT INTO vc_ref (name, kind, revision, user, note, base) VALUES (?1, 'branch', ?2, ?3, ?4, ?2)",
            params![name, revision, user, note],
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
        self.conn.execute(
            "INSERT INTO vc_ref (name, kind, revision, user, note) VALUES (?1, 'tag', ?2, ?3, ?4)",
            params![name, revision, user, note],
        )?;
        Ok(())
    }

    fn set_branch_revision(&self, name: &str, revision: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE vc_ref SET revision = ?1 WHERE name = ?2 AND kind = 'branch'",
            params![revision, name],
        )?;
        Ok(())
    }

    fn set_ref_revision(&self, name: &str, revision: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE vc_ref SET revision = ?1 WHERE name = ?2",
            params![revision, name],
        )?;
        Ok(())
    }

    fn set_protected(&self, name: &str, protected: bool) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE vc_ref SET protected = ?1 WHERE name = ?2 AND kind = 'branch'",
            params![protected, name],
        )?;
        Ok(())
    }

    fn delete_ref(&self, name: &str) -> Result<(), SessionError> {
        self.conn
            .execute("DELETE FROM vc_ref WHERE name = ?1", params![name])?;
        Ok(())
    }

    fn grants(&self) -> Result<Vec<GrantRow>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, kinds, objects, area, user, time, note FROM edit_grant ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(GrantRow {
                id: r.get(0)?,
                subject: r.get(1)?,
                kinds: r.get(2)?,
                objects: r.get(3)?,
                area: r.get(4)?,
                user: r.get(5)?,
                time: r.get(6)?,
                note: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn add_grant(&self, g: &GrantRow) -> Result<i64, SessionError> {
        self.conn.execute(
            "INSERT INTO edit_grant (subject, kinds, objects, area, user, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![g.subject, g.kinds, g.objects, g.area, g.user, g.note],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn delete_grant(&self, id: i64) -> Result<(), SessionError> {
        self.conn
            .execute("DELETE FROM edit_grant WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn reviews(&self) -> Result<Vec<ReviewRow>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, branch, target, revision, author, time, note, reviewers, state FROM vc_review ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ReviewRow {
                id: r.get(0)?,
                branch: r.get(1)?,
                target: r.get(2)?,
                revision: r.get(3)?,
                author: r.get(4)?,
                time: r.get(5)?,
                note: r.get(6)?,
                reviewers: r.get(7)?,
                state: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn review_notes(&self) -> Result<Vec<ReviewNoteRow>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT review, user, time, verdict, comment FROM vc_review_note ORDER BY seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ReviewNoteRow {
                review: r.get(0)?,
                user: r.get(1)?,
                time: r.get(2)?,
                verdict: r.get(3)?,
                comment: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn open_review(&self, branch: &str, target: &str) -> Result<Option<i64>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM vc_review WHERE branch = ?1 AND target = ?2 AND state IN ('open', 'approved')",
                params![branch, target],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn add_review(&self, r: &ReviewRow) -> Result<i64, SessionError> {
        self.conn.execute(
            "INSERT INTO vc_review (branch, target, revision, author, note, reviewers, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open')",
            params![r.branch, r.target, r.revision, r.author, r.note, r.reviewers],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn set_review_state(&self, id: i64, state: &str) -> Result<(), SessionError> {
        self.conn.execute(
            "UPDATE vc_review SET state = ?1 WHERE id = ?2",
            params![state, id],
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
        self.conn.execute(
            "INSERT INTO vc_review_note (review, user, verdict, comment) VALUES (?1, ?2, ?3, ?4)",
            params![review, user, verdict, comment],
        )?;
        Ok(())
    }

    fn surfaces(&self) -> Result<Vec<SurfaceRow>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, time, user, source, points, triangles, bounds FROM surface ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SurfaceRow {
                name: r.get(0)?,
                time: r.get(1)?,
                user: r.get(2)?,
                source: r.get(3)?,
                points: r.get(4)?,
                triangles: r.get(5)?,
                bounds: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn surface_data(&self, name: &str) -> Result<Option<Vec<u8>>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT data FROM surface WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn put_surface(&self, s: &NewSurface<'_>) -> Result<(), SessionError> {
        self.conn.execute(
            "INSERT INTO surface (name, user, source, points, triangles, bounds, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(name) DO UPDATE SET user = excluded.user, source = excluded.source,
                 points = excluded.points, triangles = excluded.triangles,
                 bounds = excluded.bounds, data = excluded.data,
                 time = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![
                s.name,
                s.user,
                s.source,
                s.points,
                s.triangles,
                s.bounds,
                s.data
            ],
        )?;
        Ok(())
    }

    fn rename_surface(&self, from: &str, to: &str) -> Result<usize, SessionError> {
        Ok(self.conn.execute(
            "UPDATE surface SET name = ?2 WHERE name = ?1",
            params![from, to],
        )?)
    }

    fn delete_surface(&self, name: &str) -> Result<usize, SessionError> {
        Ok(self
            .conn
            .execute("DELETE FROM surface WHERE name = ?1", params![name])?)
    }

    fn change_points(
        &self,
        cloud: &str,
        points: &[PointChange],
    ) -> Result<(Vec<PointChange>, Vec<PointChange>), SessionError> {
        crate::points::apply(&self.conn, cloud, points)
    }

    fn points_still_there(
        &self,
        cloud: &str,
        points: &[PointChange],
    ) -> Result<bool, SessionError> {
        crate::points::still_there(&self.conn, cloud, points)
    }

    fn points_entry(&self, grid: &Grid, cloud: &str, points: &[PointChange]) -> Option<Entry> {
        crate::points::entry(&self.conn, grid, cloud, points)
    }

    fn points_state(&self) -> String {
        let seq: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM oplog WHERE objects LIKE '%\"point_cloud\"%'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let clouds: String = self
            .conn
            .query_row(
                "SELECT COALESCE(group_concat(name || ':' || points || ':' || created, ';'), '')
                 FROM (SELECT name, points, created FROM pointcloud ORDER BY name)",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();
        format!("{seq}|{clouds}")
    }
}
