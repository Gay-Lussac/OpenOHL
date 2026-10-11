// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Project lines: several designs of the line in one project (e.g. the
//! main line and the phases of the project), each **editable on its own**.
//!
//! A project line is a branch of the version control ([`crate::versions`])
//! with its own working state: its objects, its operation log entries
//! (undo is per user and per line) and its spatial index entries. The
//! first one is `main`, the main line.
//!
//! - Each user works on one line at a time ([`Session::select_line`]): the
//!   commands, undo, redo, commits and merges act on it. Choosing a line
//!   changes nothing for the others (on a server every user has their
//!   own).
//! - [`Session::create_line`] starts a line from another (its working
//!   state as it is, uncommitted changes included) or from a revision or
//!   tag. Designers may; the line is theirs.
//! - A line is an independent copy: changes of the line it came from reach
//!   it only through [`Session::bring_in`] (a merge: changes to different
//!   objects combine by themselves, the same object changed on both sides
//!   is a conflict to settle). A phase goes back into the main line
//!   through a review ([`crate::collab`]).
//! - [`Session::delete_line`], [`Session::rename_line`]: the line's creator
//!   or an admin; the main line only an admin.
//!
//! Tables (schema 3): `model_object`, `oplog` and `spatial_index` carry the
//! line's name (`line`); the lines are the branches of `vc_ref`. Files of
//! schema 2 are migrated when opened: their objects and log become the
//! main line's; other branches get the state of their last revision; the
//! project lines saved before (frozen copies, `line_snapshot`) become
//! lines.

use std::collections::BTreeMap;

use ols_line::Line;
use rusqlite::{Connection, OptionalExtension, params};

use crate::versions::{
    Choices, Conflict, MAIN, MergeOutcome, RefKind, State, merge_states, short, state_hash,
};
use crate::{
    Applied, Kind, Object, Role, Session, SessionError, line_of, objects_of, refused, spatial,
};

/// A project line's state while another is the session's current one.
#[derive(Debug, Clone)]
pub(crate) struct LineState {
    pub(crate) objects: BTreeMap<(Kind, String), Object>,
    pub(crate) line: Line,
    pub(crate) spatial: BTreeMap<(Kind, String), spatial::Entry>,
}

/// What is known of a project line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineInfo {
    /// Name (its branch's).
    pub name: String,
    /// Who made it (empty: the project's first line).
    pub user: String,
    /// When it was made (UTC).
    pub time: String,
    /// Note.
    pub note: String,
    /// Protected: commits and merges by admins or approved reviews only.
    pub protected: bool,
    /// What the line is (free text, may be long).
    pub description: String,
    /// Its last revision (`None`: nothing committed yet).
    pub revision: Option<String>,
    /// Number of objects (structures, sections…) of its working state.
    pub objects: usize,
}

impl LineInfo {
    /// Whether it is the main line.
    pub fn is_main(&self) -> bool {
        self.name == MAIN
    }
}

/// A line with nothing in it.
pub fn empty_line() -> Line {
    Line {
        alignments: Default::default(),
        format: ols_line::LINE_FILE_FORMAT.to_owned(),
        version: ols_line::LINE_FILE_VERSION,
        name: String::new(),
        crs: String::new(),
        alignment: vec![],
        routes: vec![],
        structures: vec![],
        sections: vec![],
        jumpers: vec![],
        display: ols_line::Display::default(),
        systems: vec![],
        creep_stretch: Default::default(),
        bimetallic: Default::default(),
        circuit_starts: Vec::new(),
        condition_cases: Default::default(),
    }
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, SessionError> {
    Ok(conn
        .prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?
        .exists(params![table, column])?)
}

/// Schema 2 → 3: the objects, log and index of each project line (those
/// of earlier files become the main line's). The index is rebuilt from
/// the objects when the session opens.
pub(crate) fn migrate(conn: &Connection) -> Result<(), SessionError> {
    if !has_column(conn, "model_object", "line")? {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(
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

/// Longest description of a line, characters.
pub const MAX_DESCRIPTION: usize = 20_000;

/// Lines get a description (added within schema 3).
pub(crate) fn migrate_descriptions(conn: &Connection) -> Result<(), SessionError> {
    if !has_column(conn, "vc_ref", "description")? {
        conn.execute(
            "ALTER TABLE vc_ref ADD COLUMN description TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    Ok(())
}

fn write_objects(
    conn: &Connection,
    line: &str,
    objects: &BTreeMap<(Kind, String), Object>,
) -> Result<(), SessionError> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO model_object (line, kind, id, ord, version, json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(line, kind, id) DO UPDATE SET ord = excluded.ord, version = excluded.version, json = excluded.json",
    )?;
    for ((kind, id), o) in objects {
        stmt.execute(params![
            line,
            kind.as_str(),
            id,
            o.ord,
            o.version,
            o.value.to_string()
        ])?;
    }
    Ok(())
}

/// The objects of a line file, placed in their order.
fn objects_of_line(line: &Line) -> BTreeMap<(Kind, String), Object> {
    let mut ords: BTreeMap<Kind, i64> = BTreeMap::new();
    objects_of(line)
        .into_iter()
        .map(|(kind, id, value)| {
            let ord = ords.entry(kind).or_default();
            *ord += 1;
            (
                (kind, id),
                Object {
                    ord: *ord - 1,
                    version: 1,
                    value,
                },
            )
        })
        .collect()
}

impl Session {
    /// Builds the state of line `name` from its objects, the index in the
    /// file brought up to date.
    fn line_state(
        &self,
        name: &str,
        objects: BTreeMap<(Kind, String), Object>,
    ) -> Result<LineState, SessionError> {
        let line = if objects.is_empty() {
            empty_line()
        } else {
            line_of(&objects)?
        };
        let stored = spatial::load(&self.conn, name)?;
        let current = spatial::entries(&line, &self.grid);
        if stored != current {
            let tx = self.conn.unchecked_transaction()?;
            spatial::write_diff(&tx, name, &stored, &current)?;
            tx.commit()?;
        }
        Ok(LineState {
            objects,
            line,
            spatial: current,
        })
    }

    /// Sets the session's lines from the objects stored by line: the
    /// project lines saved before schema 3 and branches without a working
    /// state of their own become lines; the current one is the head of
    /// earlier files, else the main line.
    pub(crate) fn load_lines(
        &mut self,
        mut by_line: BTreeMap<String, BTreeMap<(Kind, String), Object>>,
    ) -> Result<(), SessionError> {
        // Frozen project lines of earlier files: editable lines now.
        let saved: Vec<(String, String, String, String)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT name, user, note, json FROM line_snapshot ORDER BY time, name")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<Result<_, _>>()?
        };
        let refs = self.refs()?;
        for (name, user, note, json) in saved {
            let line: Line = serde_json::from_str(&json)
                .map_err(|e| SessionError::Store(format!("project line {name}: {e}")))?;
            let mut new = name.trim().to_owned();
            let taken = |n: &str| refs.iter().any(|r| r.name == n) || by_line.contains_key(n);
            if taken(&new) || self.check_ref_name(&new).is_err() {
                new = (1..)
                    .map(|k| format!("{} ({k})", name.trim().replace(':', "-")))
                    .find(|n| !taken(n))
                    .expect("a free name");
            }
            let objects = objects_of_line(&line);
            let tx = self.conn.unchecked_transaction()?;
            write_objects(&tx, &new, &objects)?;
            tx.execute(
                "INSERT INTO vc_ref (name, kind, revision, user, note) VALUES (?1, 'branch', NULL, ?2, ?3)",
                params![new, user, note],
            )?;
            tx.execute("DELETE FROM line_snapshot WHERE name = ?1", params![name])?;
            tx.commit()?;
            by_line.insert(new, objects);
        }
        // Branches without a working state: their last revision's.
        for r in self.refs()? {
            if r.kind != RefKind::Branch || by_line.contains_key(&r.name) {
                continue;
            }
            let mut objects = BTreeMap::new();
            if let Some(rev) = &r.revision {
                for ((kind, id), (ord, value)) in self.revision_state(rev)?.objects {
                    objects.insert(
                        (kind, id),
                        Object {
                            ord,
                            version: 1,
                            value,
                        },
                    );
                }
                let tx = self.conn.unchecked_transaction()?;
                write_objects(&tx, &r.name, &objects)?;
                tx.commit()?;
            }
            by_line.insert(r.name.clone(), objects);
        }
        // Objects of a line without a branch (written outside a session).
        for name in by_line.keys() {
            if self.find_ref(name)?.is_none() {
                self.conn.execute(
                    "INSERT INTO vc_ref (name, kind, revision, user) VALUES (?1, 'branch', NULL, '')",
                    params![name],
                )?;
            }
        }
        let head: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM ols_meta WHERE key = 'vc_head'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .and_then(|v: String| v.strip_prefix("branch:").map(str::to_owned));
        let current = head
            .filter(|h| by_line.contains_key(h))
            .or_else(|| by_line.contains_key(MAIN).then(|| MAIN.to_owned()))
            .or_else(|| by_line.keys().next().cloned())
            .unwrap_or_else(|| MAIN.to_owned());
        by_line.entry(current.clone()).or_default();
        self.others.clear();
        for (name, objects) in by_line {
            let state = self.line_state(&name, objects)?;
            if name == current {
                self.objects = state.objects;
                self.line = state.line;
                self.spatial = state.spatial;
            } else {
                self.others.insert(name, state);
            }
        }
        self.current = current;
        Ok(())
    }

    /// The project line the commands act on.
    pub fn current_line(&self) -> &str {
        &self.current
    }

    /// Refuses `user` changing the line worked on when it is protected
    /// (an admin only; its changes otherwise come through approved reviews).
    pub(crate) fn check_line_write(&self, user: &str) -> Result<(), SessionError> {
        let line = self.current.clone();
        self.check_branch_write(user, &line, false)
    }

    /// Whether `name` is a project line.
    pub fn is_line(&self, name: &str) -> bool {
        self.current == name || self.others.contains_key(name)
    }

    /// The project lines: the main line first, then by name.
    pub fn lines(&self) -> Result<Vec<LineInfo>, SessionError> {
        let mut out: Vec<LineInfo> = self
            .refs()?
            .into_iter()
            .filter(|r| r.kind == RefKind::Branch && self.is_line(&r.name))
            .map(|r| LineInfo {
                objects: if r.name == self.current {
                    self.objects.len()
                } else {
                    self.others.get(&r.name).map_or(0, |s| s.objects.len())
                },
                name: r.name,
                user: r.user,
                time: r.time,
                note: r.note,
                protected: r.protected,
                revision: r.revision,
                description: String::new(),
            })
            .collect();
        for l in &mut out {
            l.description = self
                .conn
                .query_row(
                    "SELECT description FROM vc_ref WHERE name = ?1 AND kind = 'branch'",
                    params![l.name],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_default();
        }
        out.sort_by_key(|l| (!l.is_main(), l.name.clone()));
        Ok(out)
    }

    /// Project line `name`'s line as it is now.
    pub fn line_named(&self, name: &str) -> Result<&Line, SessionError> {
        if name == self.current {
            return Ok(&self.line);
        }
        self.others
            .get(name)
            .map(|s| &s.line)
            .ok_or_else(|| refused(format!("no project line \"{name}\"")))
    }

    /// Makes `name` the line the commands act on (nothing is changed:
    /// any user may look at any line).
    pub fn select_line(&mut self, name: &str) -> Result<(), SessionError> {
        if name == self.current {
            return Ok(());
        }
        let next = self
            .others
            .remove(name)
            .ok_or_else(|| refused(format!("no project line \"{name}\"")))?;
        let previous = LineState {
            objects: std::mem::replace(&mut self.objects, next.objects),
            line: std::mem::replace(&mut self.line, next.line),
            spatial: std::mem::replace(&mut self.spatial, next.spatial),
        };
        let old = std::mem::replace(&mut self.current, name.to_owned());
        self.others.insert(old, previous);
        Ok(())
    }

    /// Runs `f` on line `name`, then comes back to the line selected
    /// before: under its new name when `f` renamed it; when `f` deleted
    /// it, the session stays on `name`.
    pub fn with_line<T>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut Self) -> Result<T, SessionError>,
    ) -> Result<T, SessionError> {
        let mut previous = self.current.clone();
        self.select_line(name)?;
        self.renamed = None;
        let out = f(self);
        if let Some((from, to)) = self.renamed.take()
            && from == previous
        {
            previous = to;
        }
        if self.is_line(&previous) {
            self.select_line(&previous)?;
        }
        out
    }

    /// Who may delete or rename line `name`: its creator or an admin; the
    /// main line only an admin.
    fn may_manage_line(&self, user: &str, name: &str) -> Result<(), SessionError> {
        let r = self
            .find_ref(name)?
            .filter(|r| r.kind == RefKind::Branch && self.is_line(name))
            .ok_or_else(|| refused(format!("no project line \"{name}\"")))?;
        if self.role(user)? == Some(Role::Admin) {
            return Ok(());
        }
        self.require(user, Role::Designer)?;
        if r.protected {
            return Err(refused(format!(
                "project line \"{name}\" is protected: only an admin deletes, renames or describes it"
            )));
        }
        if name == MAIN {
            return Err(refused(
                "only an admin deletes, renames or describes the main line",
            ));
        }
        if r.user != user {
            return Err(refused(format!(
                "project line \"{name}\" is {}'s: only they or an admin delete, rename or describe it",
                if r.user.is_empty() {
                    "the project"
                } else {
                    &r.user
                }
            )));
        }
        Ok(())
    }

    /// Starts project line `name` from `from`: a line (its working state as
    /// it is now, uncommitted changes included — recorded as a revision of
    /// `from` first, the two lines' common ancestor) or a revision or tag.
    /// The line is `user`'s; it is not selected.
    pub fn create_line(&mut self, user: &str, name: &str, from: &str) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        let name = self.check_ref_name(name)?;
        let from = from.trim();
        let (objects, revision) = if self.is_line(from) {
            // The state it starts from, recorded on its source: their
            // common ancestor when changes are brought in later.
            self.with_line(from, |s| {
                s.record(user, &format!("Start of project line {name}"))
            })?;
            let objects = if from == self.current {
                self.objects.clone()
            } else {
                self.others[from].objects.clone()
            };
            (objects, self.branch_revision(from)?)
        } else {
            let rev = self.resolve_revision(from)?;
            let objects = self
                .revision_state(&rev)?
                .objects
                .into_iter()
                .map(|(k, (ord, value))| {
                    (
                        k,
                        Object {
                            ord,
                            version: 1,
                            value,
                        },
                    )
                })
                .collect();
            (objects, Some(rev))
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO vc_ref (name, kind, revision, user, note) VALUES (?1, 'branch', ?2, ?3, ?4)",
            params![name, revision, user, format!("from {from}")],
        )?;
        write_objects(&tx, &name, &objects)?;
        tx.commit()?;
        let state = self.line_state(&name, objects)?;
        self.others.insert(name, state);
        Ok(())
    }

    /// Sets the description of project line `name` (its creator or an
    /// admin; the main line an admin): free text, at most
    /// [`MAX_DESCRIPTION`] characters.
    pub fn set_line_description(
        &mut self,
        user: &str,
        name: &str,
        description: &str,
    ) -> Result<(), SessionError> {
        self.may_manage_line(user, name)?;
        if description.chars().count() > MAX_DESCRIPTION {
            return Err(refused(format!(
                "a description of at most {MAX_DESCRIPTION} characters"
            )));
        }
        self.conn.execute(
            "UPDATE vc_ref SET description = ?2 WHERE name = ?1 AND kind = 'branch'",
            params![name, description.trim_end()],
        )?;
        Ok(())
    }

    /// Deletes project line `name` with its working state (its revisions
    /// stay in the file: a line made from its last revision brings it
    /// back; its open reviews are closed). Refused for the line being
    /// worked on and for the project's only line.
    pub fn delete_line(&mut self, user: &str, name: &str) -> Result<(), SessionError> {
        self.may_manage_line(user, name)?;
        if name == self.current {
            return Err(refused(format!(
                "\"{name}\" is the line you work on: choose another one first"
            )));
        }
        let reviews = self.ref_deletion(name)?.reviews;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM model_object WHERE line = ?1", params![name])?;
        tx.execute("DELETE FROM spatial_index WHERE line = ?1", params![name])?;
        tx.execute(
            "DELETE FROM vc_ref WHERE name = ?1 AND kind = 'branch'",
            params![name],
        )?;
        for id in reviews {
            tx.execute(
                "UPDATE vc_review SET state = 'closed' WHERE id = ?1",
                params![id],
            )?;
        }
        tx.commit()?;
        self.others.remove(name);
        Ok(())
    }

    /// Renames project line `from` to `to` (its log, index, branch and
    /// reviews follow).
    pub fn rename_line(&mut self, user: &str, from: &str, to: &str) -> Result<(), SessionError> {
        self.may_manage_line(user, from)?;
        let to = self.check_ref_name(to)?;
        let tx = self.conn.unchecked_transaction()?;
        for table in ["model_object", "oplog", "spatial_index"] {
            tx.execute(
                &format!("UPDATE {table} SET line = ?2 WHERE line = ?1"),
                params![from, to],
            )?;
        }
        tx.execute(
            "UPDATE vc_ref SET name = ?2 WHERE name = ?1 AND kind = 'branch'",
            params![from, to],
        )?;
        tx.execute(
            "UPDATE vc_review SET branch = ?2 WHERE branch = ?1",
            params![from, to],
        )?;
        tx.execute(
            "UPDATE vc_review SET target = ?2 WHERE target = ?1",
            params![from, to],
        )?;
        tx.commit()?;
        if self.current == from {
            self.current = to.clone();
        } else if let Some(s) = self.others.remove(from) {
            self.others.insert(to.clone(), s);
        }
        self.renamed = Some((from.to_owned(), to));
        Ok(())
    }

    /// Records the working state of the current line as a revision when it
    /// has changes (its content does not change: also on a protected
    /// line).
    fn record(&mut self, user: &str, message: &str) -> Result<(), SessionError> {
        if !self.has_line() || self.status()?.changes.is_empty() {
            return Ok(());
        }
        // A protected line's changes become a revision by an admin (or
        // through an approved review), not as a side effect for anyone.
        self.check_line_write(user).map_err(|_| {
            refused(format!(
                "project line \"{}\" is protected and has changes not committed: an admin commits them first",
                self.current
            ))
        })?;
        let parent = self.branch_revision(&self.current.clone())?;
        let state = self.working_state()?;
        let branch = self.current.clone();
        self.write_revision(
            user,
            &state,
            parent.into_iter().collect(),
            message,
            Some(&branch),
        )?;
        Ok(())
    }

    /// What bringing the changes of project line `from` into the current
    /// line would give, both as they are now (uncommitted work included):
    /// the conflicts (with `choices` applied) and, when they are all
    /// settled, whether the merged line is valid (`Some(why)` if not).
    /// Nothing is changed.
    pub fn bring_in_plan(
        &self,
        from: &str,
        choices: &Choices,
    ) -> Result<(Vec<Conflict>, Option<String>), SessionError> {
        if from == self.current {
            return Err(refused("a line cannot bring in its own changes"));
        }
        let theirs_objects = &self
            .others
            .get(from)
            .ok_or_else(|| refused(format!("no project line \"{from}\"")))?
            .objects;
        let ours = self.working_state()?;
        let theirs = State {
            objects: theirs_objects
                .iter()
                .map(|(k, o)| (k.clone(), (o.ord, o.value.clone())))
                .collect(),
            libraries: ours.libraries.clone(),
        };
        let base = match (
            self.branch_revision(&self.current)?,
            self.branch_revision(from)?,
        ) {
            (Some(a), Some(b)) => self.merge_base(&a, &b)?,
            _ => None,
        };
        let base = match base {
            Some(b) => self.revision_state(&b)?,
            None => State::default(),
        };
        let (merged, conflicts) = merge_states(&base, &ours, &theirs, choices);
        let settled = conflicts.iter().all(|c| c.chosen.is_some());
        let problem = if settled {
            merged.line().err().map(|e| e.to_string())
        } else {
            None
        };
        Ok((conflicts, problem))
    }

    /// Brings the changes of project line `from` into the current line (a
    /// merge against their common ancestor, [`Session::merge`]): changes
    /// to different objects combine by themselves; the same object changed
    /// on both sides is a conflict, settled by `choices`
    /// ([`Session::bring_in_plan`] lists them). Refused, changing nothing,
    /// while a conflict is not settled. Both lines' uncommitted work is
    /// recorded as revisions of their own first.
    pub fn bring_in(
        &mut self,
        user: &str,
        from: &str,
        choices: &Choices,
    ) -> Result<MergeOutcome, SessionError> {
        self.require(user, Role::Designer)?;
        self.require_unrestricted(user)?;
        let into = self.current.clone();
        self.check_branch_write(user, &into, false)?;
        let (conflicts, problem) = self.bring_in_plan(from, choices)?;
        let open: Vec<&str> = conflicts
            .iter()
            .filter(|c| c.chosen.is_none())
            .map(|c| c.key.as_str())
            .collect();
        if !open.is_empty() {
            return Err(refused(format!(
                "{} conflict(s) to settle (choose ours or theirs for each): {}",
                open.len(),
                open.join(", ")
            )));
        }
        if let Some(p) = problem {
            return Err(refused(format!(
                "the line brought up to date is not valid: {p}"
            )));
        }
        self.with_line(from, |s| {
            s.record(user, &format!("State of {from} brought into {into}"))
        })?;
        self.record(user, &format!("Before bringing in {from}"))?;
        self.merge(
            user,
            from,
            Some(&format!("Bring {from} into {into}")),
            choices,
        )
    }

    /// Whether project line `name` has changes not committed (its working
    /// state differs from its last revision; a line with objects and no
    /// revision has). Hashes its state: about 2 µs per object.
    pub fn has_uncommitted(&self, name: &str) -> Result<bool, SessionError> {
        let objects = if name == self.current {
            &self.objects
        } else {
            &self
                .others
                .get(name)
                .ok_or_else(|| refused(format!("no project line \"{name}\"")))?
                .objects
        };
        let Some(rev) = self.branch_revision(name)? else {
            return Ok(!objects.is_empty());
        };
        let mut state = self.working_state()?;
        state.objects = objects
            .iter()
            .map(|(k, o)| (k.clone(), (o.ord, o.value.clone())))
            .collect();
        Ok(state_hash(&state) != self.revision(&rev)?.tree)
    }

    /// Admin: rolls project line `line` back to revision `revision` (any
    /// revision, a branch or a tag): the line's state becomes that one, as
    /// one logged command, recorded as a new revision of the line — nothing
    /// of the history is lost (its uncommitted work is recorded first), and
    /// the rollback can itself be rolled back. Returns the change of the
    /// line.
    pub fn roll_back_line(
        &mut self,
        admin: &str,
        line: &str,
        revision: &str,
    ) -> Result<Option<Applied>, SessionError> {
        self.require(admin, Role::Admin)?;
        let rev = self.resolve_revision(revision)?;
        if !self.is_line(line) {
            return Err(refused(format!("no project line \"{line}\"")));
        }
        self.with_line(line, |s| {
            s.record(admin, &format!("Before rolling back to {}", short(&rev)))?;
            let state = s.revision_state(&rev)?;
            let (applied, _) = s.set_working(
                admin,
                &state,
                &format!("roll back {line} to {}", short(&rev)),
            )?;
            let parent = s.branch_revision(line)?;
            if parent.as_deref() != Some(rev.as_str()) {
                s.record(admin, &format!("Rolled back to {}", short(&rev)))?;
            }
            Ok(applied)
        })
    }
}
