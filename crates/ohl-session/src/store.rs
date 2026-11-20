// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The store under the session (ADR-0013, phase 2): what the session
//! needs to keep a project, in its own words. The session writes no SQL:
//! every read and write of the project goes through [`Store`].
//!
//! Two implementations: [`sqlite::SqliteStore`] — the project file
//! (`.ohl`, ADR-0004), with the very statements the session used before,
//! so a file written through it is the same, row for row — and the
//! PostgreSQL store of a country server (crate `ohl-store-pg`, phase 4:
//! one model of the network as a store). The interface names nothing of
//! SQLite but the file-level operations a project file has (a copy, a
//! checkpoint). [`copy`] copies a whole project from one store into
//! another through it (a network's import and per-model export); the
//! tests every store must pass are in `suite` (feature `store-suite`).
//!
//! Groups (the plan's §3.1):
//!
//! | Group | Methods |
//! |---|---|
//! | Opening | [`Store::prepare`], [`Store::mark_object_upgrades_done`] |
//! | Units of work | [`Store::savepoint`], [`Store::release`], [`Store::roll_back_to`] (nested steps) |
//! | Meta and settings | [`Store::meta`], [`Store::set_meta`], [`Store::delete_meta`] |
//! | Users | [`Store::user_count`], [`Store::put_user`], [`Store::user_role`], [`Store::users`] |
//! | Objects | [`Store::object_rows`], [`Store::write_object`], [`Store::write_place`], [`Store::delete_object`], … |
//! | Log | [`Store::append_entry`], [`Store::entries`], [`Store::entry`], [`Store::mark_undone`], … |
//! | Spatial | [`Store::spatial_load`], [`Store::spatial_write_diff`], [`Store::spatial_query`] |
//! | Versions | blobs, revisions, refs |
//! | Collaboration | grants, reviews and their notes |
//! | Terrain, points | surfaces; point changes through the point-cloud store |
//! | Whole copies | [`Store::meta_rows`], [`Store::blob_hashes`], `put_…_row` (rows as they are, ids and times kept) |
//!
//! Text columns holding JSON (a command, a grant's kinds, a revision's
//! parents) pass as text: the session (de)serialises them, the store keeps
//! them as given.

use std::collections::BTreeMap;
use std::path::Path;

use ohl_core::grid::{Bounds, Grid};

use crate::points::PointChange;
use crate::spatial::Entry;
use crate::{Kind, SessionError};

pub mod copy;
pub mod sqlite;
#[cfg(any(test, feature = "store-suite"))]
pub mod suite;

/// A stored object row of a project line (`gone`: 0 an object, 1 a
/// tombstone, 2 its base's object at a place of its own).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectRow {
    /// Project line (branch).
    pub line: String,
    /// Object kind as stored.
    pub kind: String,
    /// Object id.
    pub id: String,
    /// Place among its kind.
    pub ord: i64,
    /// Version.
    pub version: i64,
    /// Its value, JSON.
    pub json: String,
    /// 0, 1 or 2 (above).
    pub gone: i64,
}

/// A log entry to append.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEntry<'a> {
    /// User.
    pub user: &'a str,
    /// Entry kind (`do`, `undo`, `import`, …).
    pub kind: &'a str,
    /// The command applied, JSON.
    pub command: String,
    /// Its inverse, JSON.
    pub inverse: String,
    /// Objects and their versions after it, JSON.
    pub objects: String,
    /// Sections to solve again, JSON.
    pub sections: String,
    /// The entry it undoes.
    pub undoes: Option<i64>,
    /// A note.
    pub note: Option<String>,
    /// Project line.
    pub line: &'a str,
    /// Its number, when given (a replica of one model of a country
    /// server takes the server's numbers, [`crate::Session::set_next_seq`]);
    /// `None`: the next of the store, as always.
    pub seq: Option<i64>,
}

/// A stored log entry: seq, time, user, kind, command, inverse, objects,
/// sections, undoes, undone_by, note, line.
pub type EntryRow = (
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<i64>,
    Option<String>,
    String,
);

/// A stored revision (parents as JSON text).
#[derive(Debug, Clone, PartialEq)]
pub struct RevisionRow {
    /// Id.
    pub id: String,
    /// Parents, JSON.
    pub parents: String,
    /// Author.
    pub author: String,
    /// Time (UTC).
    pub time: String,
    /// Message.
    pub message: String,
    /// Tree hash.
    pub tree: String,
    /// Last log entry when it was made.
    pub seq: i64,
}

/// A stored branch or tag.
#[derive(Debug, Clone, PartialEq)]
pub struct RefRow {
    /// Name.
    pub name: String,
    /// `branch` or `tag`.
    pub kind: String,
    /// Revision.
    pub revision: Option<String>,
    /// Protected.
    pub protected: bool,
    /// Who made it.
    pub user: String,
    /// When.
    pub time: String,
    /// Note.
    pub note: String,
}

/// A stored edit grant (kinds, objects and area as JSON text).
#[derive(Debug, Clone, PartialEq)]
pub struct GrantRow {
    /// Id.
    pub id: i64,
    /// User or role.
    pub subject: String,
    /// Kinds, JSON.
    pub kinds: String,
    /// Objects, JSON.
    pub objects: String,
    /// Area, JSON.
    pub area: Option<String>,
    /// Who granted it.
    pub user: String,
    /// When.
    pub time: String,
    /// Note.
    pub note: String,
}

/// A stored review (reviewers as JSON text).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewRow {
    /// Id.
    pub id: i64,
    /// Branch reviewed.
    pub branch: String,
    /// Target branch.
    pub target: String,
    /// Revision submitted.
    pub revision: String,
    /// Author.
    pub author: String,
    /// When.
    pub time: String,
    /// Note.
    pub note: String,
    /// Reviewers, JSON.
    pub reviewers: String,
    /// State.
    pub state: String,
}

/// A stored note of a review.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewNoteRow {
    /// Review id.
    pub review: i64,
    /// User.
    pub user: String,
    /// When.
    pub time: String,
    /// Verdict.
    pub verdict: String,
    /// Comment.
    pub comment: String,
}

/// A stored surface, without its data (bounds as JSON text).
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceRow {
    /// Name.
    pub name: String,
    /// When.
    pub time: String,
    /// Who.
    pub user: String,
    /// Where from.
    pub source: String,
    /// Points.
    pub points: i64,
    /// Triangles.
    pub triangles: i64,
    /// Bounds, JSON.
    pub bounds: String,
}

/// A surface to keep.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSurface<'a> {
    /// Name.
    pub name: &'a str,
    /// Who.
    pub user: &'a str,
    /// Where from.
    pub source: &'a str,
    /// Points.
    pub points: i64,
    /// Triangles.
    pub triangles: i64,
    /// Bounds, JSON.
    pub bounds: String,
    /// The mesh, encoded.
    pub data: Vec<u8>,
}

/// A project line kept by builds before schema 4 (`line_snapshot`):
/// name, user, note, the line's JSON.
pub type LegacySnapshot = (String, String, String, String);

/// What the session keeps a project in. Every method is one step of
/// today's file format; none commits by itself what a surrounding
/// savepoint holds.
pub trait Store: Send {
    // --- Opening ----------------------------------------------------------

    /// Makes and upgrades what the store needs to keep a project and
    /// returns its chunk grid: a newer store refused before anything in
    /// it changes; the tables made; older stores upgraded; the grid fixed
    /// when the project is made; the schema number; the version store's
    /// main branch and head. `label` names the store in messages (the
    /// file's path); `writable`: false for a store opened to read only
    /// (anything that would write fails with its "readonly database"
    /// error).
    fn prepare(&self, label: &Path, writable: bool) -> Result<Grid, SessionError>;
    /// Marks the upgrades into routes, alignment points and structure
    /// groups done (meta rows replaced).
    fn mark_object_upgrades_done(&self) -> Result<(), SessionError>;

    // --- Units of work -------------------------------------------------

    /// Begins a step named `name` (nested in any step begun before).
    fn savepoint(&self, name: &str) -> Result<(), SessionError>;
    /// Keeps step `name`.
    fn release(&self, name: &str) -> Result<(), SessionError>;
    /// Undoes step `name` and ends it.
    fn roll_back_to(&self, name: &str) -> Result<(), SessionError>;

    // --- The store as a whole -----------------------------------------

    /// Writes a consistent copy of the whole store to `path` (must not
    /// exist).
    fn copy_to(&self, path: &Path) -> Result<(), SessionError>;
    /// [`Store::copy_to`] with the settings holding a path of this
    /// computer cut to the file name.
    fn copy_for_others(&self, path: &Path) -> Result<(), SessionError>;
    /// Writes everything waiting into the store itself.
    fn checkpoint(&self) -> Result<(), SessionError>;
    /// The time now, UTC (`YYYY-MM-DDTHH:MM:SSZ`), as the store tells it.
    fn now(&self) -> Result<String, SessionError>;

    // --- Meta and settings --------------------------------------------

    /// Meta value `key`.
    fn meta(&self, key: &str) -> Result<Option<String>, SessionError>;
    /// Sets meta value `key` (kept in its row).
    fn set_meta(&self, key: &str, value: &str) -> Result<(), SessionError>;
    /// Removes meta value `key`.
    fn delete_meta(&self, key: &str) -> Result<(), SessionError>;

    // --- Users ----------------------------------------------------------

    /// Number of users.
    fn user_count(&self) -> Result<i64, SessionError>;
    /// Adds user `name` or changes their role.
    fn put_user(&self, name: &str, role: &str) -> Result<(), SessionError>;
    /// User `name`'s role.
    fn user_role(&self, name: &str) -> Result<Option<String>, SessionError>;
    /// Users and roles, by name.
    fn users(&self) -> Result<Vec<(String, String)>, SessionError>;

    // --- Objects --------------------------------------------------------

    /// Every object row of every project line.
    fn object_rows(&self) -> Result<Vec<ObjectRow>, SessionError>;
    /// Object `kind`/`id` of line `line`: `Some((ord, version, json))`,
    /// or `None` for a tombstone.
    fn write_object(
        &self,
        line: &str,
        kind: Kind,
        id: &str,
        object: Option<(i64, i64, &str)>,
    ) -> Result<(), SessionError>;
    /// Object `kind`/`id` of line `line` replaced whole by an upgrade of
    /// an older file (a row of its own).
    fn replace_object(
        &self,
        line: &str,
        kind: Kind,
        id: &str,
        ord: i64,
        version: i64,
        json: &str,
    ) -> Result<(), SessionError>;
    /// Object `kind`/`id` of line `line` kept as its base's, at place
    /// `ord`.
    fn write_place(
        &self,
        line: &str,
        kind: Kind,
        id: &str,
        ord: i64,
        version: i64,
    ) -> Result<(), SessionError>;
    /// Removes the row of object `kind`/`id` of line `line`.
    fn delete_object(&self, line: &str, kind: Kind, id: &str) -> Result<(), SessionError>;

    // --- Project lines (branches with a working state) ------------------

    /// The revision line `name` keeps its differences from.
    fn line_base(&self, name: &str) -> Result<Option<String>, SessionError>;
    /// Line `name`'s description.
    fn line_description(&self, name: &str) -> Result<Option<String>, SessionError>;
    /// Sets line `name`'s description.
    fn set_line_description(&self, name: &str, description: &str) -> Result<(), SessionError>;
    /// Sets the base of branch `name`.
    fn set_line_base(&self, name: &str, base: &str) -> Result<(), SessionError>;
    /// Removes line `name`: its objects, its index, its branch.
    fn delete_line(&self, name: &str) -> Result<(), SessionError>;
    /// Renames line `from` to `to`: its objects, log, index, branch and
    /// reviews follow.
    fn rename_line(&self, from: &str, to: &str) -> Result<(), SessionError>;
    /// Project lines kept by builds before schema 4 (`None`: none kept).
    fn legacy_snapshots(&self) -> Result<Option<Vec<LegacySnapshot>>, SessionError>;
    /// Removes legacy project line `name`.
    fn delete_legacy_snapshot(&self, name: &str) -> Result<(), SessionError>;
    /// Removes what kept the legacy project lines.
    fn drop_legacy_snapshots(&self) -> Result<(), SessionError>;

    // --- Log ------------------------------------------------------------

    /// Appends `entry`; returns its sequence number.
    fn append_entry(&self, entry: &NewEntry<'_>) -> Result<i64, SessionError>;
    /// Marks entry `seq` reversed by entry `by`.
    fn mark_undone(&self, seq: i64, by: i64) -> Result<(), SessionError>;
    /// Marks the entries of line `line` after `seq` and before `by` not
    /// reversed yet as reversed by `by`.
    fn mark_undone_after(&self, by: i64, seq: i64, line: &str) -> Result<(), SessionError>;
    /// Every entry, oldest first.
    fn entries(&self) -> Result<Vec<EntryRow>, SessionError>;
    /// Entry `seq`.
    fn entry(&self, seq: i64) -> Result<Option<EntryRow>, SessionError>;
    /// The objects of entry `seq` (JSON).
    fn entry_objects(&self, seq: i64) -> Result<Option<String>, SessionError>;
    /// Notes on entry `seq` that it reverses entry `reversed` (with that
    /// entry's note).
    fn note_reversal(&self, seq: i64, reversed: i64) -> Result<(), SessionError>;
    /// Sets entry `seq`'s note.
    fn set_entry_note(&self, seq: i64, note: &str) -> Result<(), SessionError>;
    /// `user`'s last entry on `line` an undo reverses.
    fn last_undoable(&self, user: &str, line: &str) -> Result<Option<i64>, SessionError>;
    /// `user`'s last do, undo or redo on `line`: seq, kind, undone_by.
    fn last_own_entry(
        &self,
        user: &str,
        line: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SessionError>;
    /// The inverses of line `line`'s entries after `seq`, newest first.
    fn inverses_after(&self, seq: i64, line: &str) -> Result<Vec<String>, SessionError>;
    /// The last entry's sequence number (0: none).
    fn last_seq(&self) -> Result<i64, SessionError>;
    /// Line `line`'s last entry's sequence number (0: none).
    fn last_seq_of_line(&self, line: &str) -> Result<i64, SessionError>;

    // --- Spatial index ----------------------------------------------------

    /// Writes the differences between `old` and `new` entries of line
    /// `line`; returns how many rows changed.
    fn spatial_write_diff(
        &self,
        line: &str,
        old: &BTreeMap<(Kind, String), Entry>,
        new: &BTreeMap<(Kind, String), Entry>,
    ) -> Result<usize, SessionError>;
    /// The stored index of line `line`.
    fn spatial_load(&self, line: &str) -> Result<BTreeMap<(Kind, String), Entry>, SessionError>;
    /// Objects of line `line` whose bounds touch `area`, by kind and id.
    fn spatial_query(
        &self,
        line: &str,
        grid: &Grid,
        area: &Bounds,
    ) -> Result<Vec<(Kind, String, Entry)>, SessionError>;

    // --- Versions ---------------------------------------------------------

    /// Whether the session writes **split trees** for revisions
    /// ([`crate::trees`], phase 11): a network's models do; a project file
    /// keeps flat trees (the desktop's format unchanged). Both are read.
    fn split_trees(&self) -> bool {
        false
    }
    /// Whether blob `hash` is stored.
    fn has_blob(&self, hash: &str) -> Result<bool, SessionError>;
    /// Stores blob `hash` (its data compressed by the caller).
    fn put_blob(&self, hash: &str, data: &[u8]) -> Result<(), SessionError>;
    /// Blob `hash`'s data.
    fn blob(&self, hash: &str) -> Result<Option<Vec<u8>>, SessionError>;
    /// Revision `id`.
    fn revision(&self, id: &str) -> Result<Option<RevisionRow>, SessionError>;
    /// Every revision, newest first.
    fn revisions(&self) -> Result<Vec<RevisionRow>, SessionError>;
    /// At most two revision ids starting with `prefix`.
    fn revisions_starting(&self, prefix: &str) -> Result<Vec<String>, SessionError>;
    /// Whether revision `id` is stored.
    fn has_revision(&self, id: &str) -> Result<bool, SessionError>;
    /// Stores a revision made now (`time` left to the store).
    fn put_revision(&self, revision: &RevisionRow) -> Result<(), SessionError>;
    /// Stores a revision with its own time (from a bundle).
    fn put_revision_with_time(&self, revision: &RevisionRow) -> Result<(), SessionError>;
    /// Branches and tags, by kind and name.
    fn refs(&self) -> Result<Vec<RefRow>, SessionError>;
    /// Whether the main branch and the head exist.
    fn versions_initialised(&self, main: &str) -> Result<bool, SessionError>;
    /// Makes the main branch and the head when missing.
    fn init_versions(&self, main: &str) -> Result<(), SessionError>;
    /// Adds branch `name` (no revision) of `user`.
    fn add_branch(&self, name: &str, user: &str) -> Result<(), SessionError>;
    /// Adds branch `name` with a note (no revision): a legacy project line.
    fn add_branch_noted(&self, name: &str, user: &str, note: &str) -> Result<(), SessionError>;
    /// Adds branch `name` at `revision` (also its base).
    fn add_line_branch(
        &self,
        name: &str,
        revision: Option<&str>,
        user: &str,
        note: &str,
    ) -> Result<(), SessionError>;
    /// Adds tag `name` at `revision`.
    fn add_tag(
        &self,
        name: &str,
        revision: &str,
        user: &str,
        note: &str,
    ) -> Result<(), SessionError>;
    /// Moves branch `name` to `revision`.
    fn set_branch_revision(&self, name: &str, revision: &str) -> Result<(), SessionError>;
    /// Moves ref `name` (branch or tag) to `revision`.
    fn set_ref_revision(&self, name: &str, revision: &str) -> Result<(), SessionError>;
    /// Protects branch `name` or not.
    fn set_protected(&self, name: &str, protected: bool) -> Result<(), SessionError>;
    /// Removes ref `name`.
    fn delete_ref(&self, name: &str) -> Result<(), SessionError>;

    // --- Collaboration ----------------------------------------------------

    /// Every grant, by id.
    fn grants(&self) -> Result<Vec<GrantRow>, SessionError>;
    /// Adds a grant (its id and time left to the store); returns its id.
    fn add_grant(&self, grant: &GrantRow) -> Result<i64, SessionError>;
    /// Removes grant `id`.
    fn delete_grant(&self, id: i64) -> Result<(), SessionError>;
    /// Every review, newest first.
    fn reviews(&self) -> Result<Vec<ReviewRow>, SessionError>;
    /// Every note of every review, oldest first.
    fn review_notes(&self) -> Result<Vec<ReviewNoteRow>, SessionError>;
    /// The open (or approved) review of `branch` into `target`.
    fn open_review(&self, branch: &str, target: &str) -> Result<Option<i64>, SessionError>;
    /// Adds an open review (id and time left to the store); returns its id.
    fn add_review(&self, review: &ReviewRow) -> Result<i64, SessionError>;
    /// Sets review `id`'s state.
    fn set_review_state(&self, id: i64, state: &str) -> Result<(), SessionError>;
    /// Adds a note to review `review`.
    fn add_review_note(
        &self,
        review: i64,
        user: &str,
        verdict: &str,
        comment: &str,
    ) -> Result<(), SessionError>;

    // --- Terrain ------------------------------------------------------------

    /// The surfaces, by name.
    fn surfaces(&self) -> Result<Vec<SurfaceRow>, SessionError>;
    /// Surface `name`'s data.
    fn surface_data(&self, name: &str) -> Result<Option<Vec<u8>>, SessionError>;
    /// Keeps a surface (replacing one of its name).
    fn put_surface(&self, surface: &NewSurface<'_>) -> Result<(), SessionError>;
    /// Renames surface `from`; returns the surfaces renamed (0 or 1).
    fn rename_surface(&self, from: &str, to: &str) -> Result<usize, SessionError>;
    /// Removes surface `name`; returns the surfaces removed (0 or 1).
    fn delete_surface(&self, name: &str) -> Result<usize, SessionError>;

    // --- Point clouds -------------------------------------------------------

    /// Changes points of cloud `cloud`: as applied, and their inverse.
    fn change_points(
        &self,
        cloud: &str,
        points: &[PointChange],
    ) -> Result<(Vec<PointChange>, Vec<PointChange>), SessionError>;
    /// Whether the points an entry left are still in cloud `cloud`.
    fn points_still_there(&self, cloud: &str, points: &[PointChange])
    -> Result<bool, SessionError>;
    /// The plan area of points changed in cloud `cloud` (`None`: no such
    /// cloud).
    fn points_entry(&self, grid: &Grid, cloud: &str, points: &[PointChange]) -> Option<Entry>;
    /// The state of the point clouds: the last entry changing points and
    /// each cloud's name, count and creation time.
    fn points_state(&self) -> String;

    // --- Whole copies ([`copy`]) --------------------------------------------

    /// Every meta row, by key.
    fn meta_rows(&self) -> Result<Vec<(String, String)>, SessionError>;
    /// Every blob's hash, by hash.
    fn blob_hashes(&self) -> Result<Vec<String>, SessionError>;
    /// Keeps log entry `row` as it is (its seq, time and marks).
    fn put_entry_row(&self, row: &EntryRow) -> Result<(), SessionError>;
    /// Keeps branch or tag `row` as it is (replacing one of its name),
    /// with a branch's description and base.
    fn put_ref_row(
        &self,
        row: &RefRow,
        description: &str,
        base: Option<&str>,
    ) -> Result<(), SessionError>;
    /// Keeps grant `row` as it is (its id and time).
    fn put_grant_row(&self, row: &GrantRow) -> Result<(), SessionError>;
    /// Keeps review `row` as it is (its id, time and state).
    fn put_review_row(&self, row: &ReviewRow) -> Result<(), SessionError>;
    /// Keeps note `row` of a review as it is (after the notes kept
    /// before).
    fn put_review_note_row(&self, row: &ReviewNoteRow) -> Result<(), SessionError>;
    /// Keeps surface `row` with its data as it is (its time).
    fn put_surface_row(&self, row: &SurfaceRow, data: &[u8]) -> Result<(), SessionError>;

    /// The project file's SQLite connection, for this crate's tests that
    /// look into the file (`None` for any other store).
    #[cfg(test)]
    fn sqlite(&self) -> Option<&rusqlite::Connection> {
        None
    }
}

#[cfg(test)]
mod tests {
    /// The session writes no SQL: outside `store/` (and the tests, which
    /// look into the file), no source holds a statement.
    #[test]
    fn no_sql_outside_the_store() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let words = [
            "SELECT ",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
            "CREATE ",
            "ALTER ",
            "DROP ",
            "PRAGMA ",
            "SAVEPOINT",
            "VACUUM",
        ];
        let mut read = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".rs") || name.ends_with("tests.rs") || name == "store.rs" {
                continue;
            }
            read += 1;
            let text = std::fs::read_to_string(&path).unwrap();
            for w in words {
                assert!(
                    !text.contains(&format!("\"{w}")),
                    "{name}: an SQL statement ({w}) outside the store"
                );
            }
        }
        assert!(read > 10, "the session's sources were not found");
    }
}
