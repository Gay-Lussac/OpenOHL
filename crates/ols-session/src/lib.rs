// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The session (ADR-0005): the only way to change a model.
//!
//! A [`Session`] owns a line model stored in the project file (`.ols`,
//! SQLite, ADR-0004). Every edit is a [`Command`] from a user: it is
//! validated (the whole line must stay valid), applied atomically, and
//! appended to the **operation log** together with its **inverse** and the
//! sections it affects (dirty tracking for incremental recalculation).
//!
//! - **Undo is per user** ([`Session::undo`]): the user's last command
//!   that is not undone yet is reversed by applying its inverse — refused
//!   when another user has changed one of the same objects since, so
//!   nobody's work is overwritten silently. [`Session::redo`] reverses the
//!   undo.
//! - **Admin rollback** ([`Session::rollback`]): an admin reverses any
//!   command, or everything after a point of the log
//!   ([`Session::rollback_after`]).
//! - **Roles**: `viewer` (read only), `designer` (edit), `admin` (edit,
//!   rollback, manage users). Finer permissions come with server mode.
//! - **Import / export**: [`Session::import`] loads a line (`.ols.json`)
//!   into a project file, [`Session::export`] gives it back.
//!
//! Objects: the line settings (`meta`), routes, structures (with their
//! attachments), sections (with stringing and loads) and jumpers, each a
//! JSON row with a version number, in their order in the line.
//!
//! Schema (version 1, tables next to the point clouds of ADR-0007):
//!
//! ```sql
//! ols_meta(key PRIMARY KEY, value)        -- 'session_schema' = '1'
//! session_user(name PRIMARY KEY, role)
//! model_object(kind, id, ord, version, json, PRIMARY KEY(kind, id))
//! oplog(seq PRIMARY KEY, time, user, kind, command, inverse, objects,
//!       sections, undoes, undone_by)
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use ols_line::{Jumper, Line, Point, Route, Section, Stringing, Structure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Schema version of the session tables.
pub const SCHEMA_VERSION: u32 = 1;

/// Errors of a session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionError {
    /// The command was refused (invalid result, conflict, permission).
    Refused(String),
    /// The project file could not be used.
    Store(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(m) => write!(f, "refused: {m}"),
            Self::Store(m) => write!(f, "project file: {m}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<rusqlite::Error> for SessionError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Store(e.to_string())
    }
}

fn refused(m: impl Into<String>) -> SessionError {
    SessionError::Refused(m.into())
}

/// Kind of model object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Line settings (name, CRS, alignment, display); id `"line"`.
    Meta,
    /// A route.
    Route,
    /// A structure with its attachments.
    Structure,
    /// A wire section.
    Section,
    /// A jumper.
    Jumper,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::Route => "route",
            Self::Structure => "structure",
            Self::Section => "section",
            Self::Jumper => "jumper",
        }
    }

    fn parse(s: &str) -> Result<Self, SessionError> {
        Ok(match s {
            "meta" => Self::Meta,
            "route" => Self::Route,
            "structure" => Self::Structure,
            "section" => Self::Section,
            "jumper" => Self::Jumper,
            other => return Err(SessionError::Store(format!("unknown object kind {other}"))),
        })
    }
}

/// A user's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Read only.
    Viewer,
    /// May edit.
    Designer,
    /// May edit, roll back anyone's commands and manage users.
    Admin,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Designer => "designer",
            Self::Admin => "admin",
        }
    }

    /// Parses `viewer`, `designer` or `admin`.
    pub fn parse(s: &str) -> Result<Self, SessionError> {
        Ok(match s {
            "viewer" => Self::Viewer,
            "designer" => Self::Designer,
            "admin" => Self::Admin,
            other => return Err(refused(format!("unknown role \"{other}\""))),
        })
    }
}

/// An edit. Serialised as JSON with an `op` tag, e.g.
/// `{"op": "move_structure", "id": "T12", "position": [x, y, z]}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Create or replace an object (its JSON as in the line file; `meta` is
    /// `{name, crs, alignment, display}`).
    Put {
        /// Kind.
        kind: Kind,
        /// Id.
        id: String,
        /// The object.
        value: Value,
    },
    /// Delete an object.
    Delete {
        /// Kind.
        kind: Kind,
        /// Id.
        id: String,
    },
    /// Move a structure (its attachments move with it).
    MoveStructure {
        /// Structure id.
        id: String,
        /// New position.
        position: Point,
    },
    /// Change a section's stringing.
    SetStringing {
        /// Section id.
        section: String,
        /// New stringing.
        stringing: Stringing,
    },
    /// Several commands applied together (all or none).
    Batch {
        /// The commands.
        commands: Vec<Command>,
    },
}

/// One entry of the operation log.
#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    /// Sequence number.
    pub seq: i64,
    /// UTC time.
    pub time: String,
    /// User.
    pub user: String,
    /// `do`, `undo`, `redo`, `rollback` or `import`.
    pub kind: String,
    /// The command as applied (puts and deletes).
    pub command: Command,
    /// Objects changed.
    pub objects: Vec<(Kind, String)>,
    /// Sections to recalculate.
    pub sections: Vec<String>,
    /// Entry this one reverses.
    pub undoes: Option<i64>,
    /// Entry that reversed this one.
    pub undone_by: Option<i64>,
}

/// Result of an applied command.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// Log sequence number.
    pub seq: i64,
    /// Objects changed.
    pub objects: Vec<(Kind, String)>,
    /// Sections to recalculate: changed sections, sections through changed
    /// structures, sections of changed jumpers' ends.
    pub sections: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    name: String,
    #[serde(default)]
    crs: String,
    #[serde(default)]
    alignment: Vec<Point>,
    #[serde(default)]
    display: ols_line::Display,
}

/// A raw log row: seq, time, user, kind, command, inverse, objects,
/// sections, undoes, undone_by.
type EntryRow = (
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
);

/// One object of the model.
#[derive(Debug, Clone, PartialEq)]
struct Object {
    ord: i64,
    version: i64,
    value: Value,
}

/// An open project.
pub struct Session {
    conn: Connection,
    objects: BTreeMap<(Kind, String), Object>,
    line: Line,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("objects", &self.objects.len())
            .finish_non_exhaustive()
    }
}

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("model objects serialise")
}

/// Objects of a line, in line order.
fn objects_of(line: &Line) -> Vec<(Kind, String, Value)> {
    let mut out = vec![(
        Kind::Meta,
        "line".to_owned(),
        json(&Meta {
            name: line.name.clone(),
            crs: line.crs.clone(),
            alignment: line.alignment.clone(),
            display: line.display.clone(),
        }),
    )];
    out.extend(
        line.routes
            .iter()
            .map(|r| (Kind::Route, r.id.clone(), json(r))),
    );
    out.extend(
        line.structures
            .iter()
            .map(|s| (Kind::Structure, s.id.clone(), json(s))),
    );
    out.extend(
        line.sections
            .iter()
            .map(|s| (Kind::Section, s.id.clone(), json(s))),
    );
    out.extend(
        line.jumpers
            .iter()
            .map(|j| (Kind::Jumper, j.id.clone(), json(j))),
    );
    out
}

/// The line made of `objects`, validated.
fn line_of(objects: &BTreeMap<(Kind, String), Object>) -> Result<Line, SessionError> {
    let bad = |m: String| refused(m);
    let mut ordered: Vec<(&(Kind, String), &Object)> = objects.iter().collect();
    ordered.sort_by_key(|(k, o)| (k.0, o.ord));
    let meta = objects
        .get(&(Kind::Meta, "line".to_owned()))
        .ok_or_else(|| bad("the project has no line".into()))?;
    let meta: Meta = serde_json::from_value(meta.value.clone())
        .map_err(|e| bad(format!("line settings: {e}")))?;
    let mut line = Line {
        format: ols_line::LINE_FILE_FORMAT.to_owned(),
        version: ols_line::LINE_FILE_VERSION,
        name: meta.name,
        crs: meta.crs,
        alignment: meta.alignment,
        routes: vec![],
        structures: vec![],
        sections: vec![],
        jumpers: vec![],
        display: meta.display,
    };
    for ((kind, id), o) in ordered {
        let v = o.value.clone();
        let parse_err = |e: serde_json::Error| bad(format!("{} {id}: {e}", kind.as_str()));
        match kind {
            Kind::Meta => {}
            Kind::Route => line
                .routes
                .push(serde_json::from_value::<Route>(v).map_err(parse_err)?),
            Kind::Structure => line
                .structures
                .push(serde_json::from_value::<Structure>(v).map_err(parse_err)?),
            Kind::Section => line
                .sections
                .push(serde_json::from_value::<Section>(v).map_err(parse_err)?),
            Kind::Jumper => line
                .jumpers
                .push(serde_json::from_value::<Jumper>(v).map_err(parse_err)?),
        }
    }
    // Ids inside the JSON must match the object ids.
    for ((kind, id), o) in objects {
        if *kind != Kind::Meta && o.value.get("id").and_then(Value::as_str) != Some(id) {
            return Err(bad(format!(
                "{} {id}: its \"id\" field differs",
                kind.as_str()
            )));
        }
    }
    line.validate().map_err(|e| bad(e.to_string()))?;
    Ok(line)
}

/// Sections to recalculate after `changed` objects (on the line after the
/// change, and before it for deleted ones).
fn affected_sections(lines: [&Line; 2], changed: &[(Kind, String)]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        for (kind, id) in changed {
            match kind {
                Kind::Section => {
                    out.insert(id.clone());
                }
                Kind::Structure => {
                    for s in &line.sections {
                        if s.points.iter().any(|p| &p.structure == id) {
                            out.insert(s.id.clone());
                        }
                    }
                }
                Kind::Jumper => {
                    if let Some(j) = line.jumpers.iter().find(|j| &j.id == id) {
                        for s in &line.sections {
                            if s.points.contains(&j.from) || s.points.contains(&j.to) {
                                out.insert(s.id.clone());
                            }
                        }
                    }
                }
                Kind::Meta | Kind::Route => {}
            }
        }
    }
    out.into_iter().collect()
}

impl Session {
    /// Opens a project file (created if missing; an empty project has no
    /// line until [`Session::import`]).
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS ols_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS session_user (name TEXT PRIMARY KEY, role TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS model_object (
                 kind TEXT NOT NULL, id TEXT NOT NULL, ord INTEGER NOT NULL,
                 version INTEGER NOT NULL, json TEXT NOT NULL, PRIMARY KEY (kind, id));
             CREATE TABLE IF NOT EXISTS oplog (
                 seq INTEGER PRIMARY KEY, time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 user TEXT NOT NULL, kind TEXT NOT NULL, command TEXT NOT NULL, inverse TEXT NOT NULL,
                 objects TEXT NOT NULL, sections TEXT NOT NULL,
                 undoes INTEGER, undone_by INTEGER);",
        )?;
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
            Some(Ok(v)) if v <= SCHEMA_VERSION => {}
            Some(_) => {
                return Err(SessionError::Store(format!(
                    "{}: session schema newer than supported ({SCHEMA_VERSION})",
                    path.display()
                )));
            }
        }
        let mut objects = BTreeMap::new();
        {
            let mut stmt = conn.prepare("SELECT kind, id, ord, version, json FROM model_object")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (kind, id, ord, version, text) = row?;
                let value =
                    serde_json::from_str(&text).map_err(|e| SessionError::Store(e.to_string()))?;
                objects.insert(
                    (Kind::parse(&kind)?, id),
                    Object {
                        ord,
                        version,
                        value,
                    },
                );
            }
        }
        let line = if objects.is_empty() {
            Line {
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
            }
        } else {
            line_of(&objects)?
        };
        Ok(Self {
            conn,
            objects,
            line,
        })
    }

    /// The current line.
    pub fn line(&self) -> &Line {
        &self.line
    }

    /// Whether the project holds a line.
    pub fn has_line(&self) -> bool {
        !self.objects.is_empty()
    }

    /// Version of an object (0 if it does not exist).
    pub fn version(&self, kind: Kind, id: &str) -> i64 {
        self.objects
            .get(&(kind, id.to_owned()))
            .map_or(0, |o| o.version)
    }

    /// Adds or changes a user. The first user of a project may be added
    /// by anyone and becomes the reference admin; afterwards only admins
    /// manage users.
    pub fn set_user(&self, by: &str, name: &str, role: Role) -> Result<(), SessionError> {
        let users: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM session_user", [], |r| r.get(0))?;
        if users > 0 && self.role(by)? != Some(Role::Admin) {
            return Err(refused(format!("{by} is not an admin")));
        }
        let role = if users == 0 { Role::Admin } else { role };
        self.conn.execute(
            "INSERT INTO session_user (name, role) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET role = excluded.role",
            params![name, role.as_str()],
        )?;
        Ok(())
    }

    /// A user's role (`None`: unknown user).
    pub fn role(&self, user: &str) -> Result<Option<Role>, SessionError> {
        let r: Option<String> = self
            .conn
            .query_row(
                "SELECT role FROM session_user WHERE name = ?1",
                params![user],
                |r| r.get(0),
            )
            .optional()?;
        r.map(|r| Role::parse(&r)).transpose()
    }

    /// Users and roles.
    pub fn users(&self) -> Result<Vec<(String, Role)>, SessionError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, role FROM session_user ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.map(|r| {
            let (n, role) = r?;
            Ok((n, Role::parse(&role)?))
        })
        .collect()
    }

    fn require(&self, user: &str, at_least: Role) -> Result<(), SessionError> {
        match self.role(user)? {
            Some(r) if r >= at_least => Ok(()),
            Some(r) => Err(refused(format!(
                "{user} is a {} and may not do this ({} needed)",
                r.as_str(),
                at_least.as_str()
            ))),
            None => Err(refused(format!("unknown user {user}"))),
        }
    }

    /// Loads a line into an empty project, as user `user` (who becomes its
    /// admin if the project has no user yet).
    pub fn import(&mut self, line: &Line, user: &str) -> Result<Applied, SessionError> {
        if self.has_line() {
            return Err(refused("the project already holds a line"));
        }
        if self.users()?.is_empty() {
            self.set_user(user, user, Role::Admin)?;
        }
        self.require(user, Role::Designer)?;
        let commands = objects_of(line)
            .into_iter()
            .map(|(kind, id, value)| Command::Put { kind, id, value })
            .collect();
        self.apply_as(user, Command::Batch { commands }, "import", None, true)
    }

    /// The line as a line file.
    pub fn export(&self) -> Line {
        self.line.clone()
    }

    /// Applies a command of `user`.
    pub fn apply(&mut self, user: &str, command: Command) -> Result<Applied, SessionError> {
        self.require(user, Role::Designer)?;
        self.apply_as(user, command, "do", None, false)
    }

    /// Expands convenience commands into puts and deletes.
    fn primitives(&self, command: Command, out: &mut Vec<Command>) -> Result<(), SessionError> {
        match command {
            Command::Put { .. } | Command::Delete { .. } => out.push(command),
            Command::Batch { commands } => {
                for c in commands {
                    self.primitives(c, out)?;
                }
            }
            Command::MoveStructure { id, position } => {
                let s = self
                    .line
                    .structure(&id)
                    .ok_or_else(|| refused(format!("no structure {id}")))?;
                let d = [
                    position[0] - s.position[0],
                    position[1] - s.position[1],
                    position[2] - s.position[2],
                ];
                let mut moved = s.clone();
                moved.position = position;
                let shift = |p: &mut Point| {
                    for k in 0..3 {
                        p[k] += d[k];
                    }
                };
                for a in &mut moved.attachments {
                    shift(&mut a.insulator_point);
                    shift(&mut a.wire_point);
                    if let Some(legs) = &mut a.legs {
                        legs.iter_mut().for_each(shift);
                    }
                }
                out.push(Command::Put {
                    kind: Kind::Structure,
                    id,
                    value: json(&moved),
                });
            }
            Command::SetStringing { section, stringing } => {
                let mut s = self
                    .line
                    .sections
                    .iter()
                    .find(|s| s.id == section)
                    .ok_or_else(|| refused(format!("no section {section}")))?
                    .clone();
                s.stringing = stringing;
                out.push(Command::Put {
                    kind: Kind::Section,
                    id: section,
                    value: json(&s),
                });
            }
        }
        Ok(())
    }

    /// Applies `command` (validated, atomic), logs it as `entry_kind`.
    fn apply_as(
        &mut self,
        user: &str,
        command: Command,
        entry_kind: &str,
        undoes: Option<i64>,
        import: bool,
    ) -> Result<Applied, SessionError> {
        let mut prims = Vec::new();
        self.primitives(command, &mut prims)?;
        if prims.is_empty() {
            return Err(refused("empty command"));
        }
        // Apply to a copy, collect the inverse (in reverse order).
        let mut objects = self.objects.clone();
        let mut inverse = Vec::new();
        let mut changed: Vec<(Kind, String)> = Vec::new();
        let next_ord = |objects: &BTreeMap<(Kind, String), Object>, kind: Kind| {
            objects
                .iter()
                .filter(|((k, _), _)| *k == kind)
                .map(|(_, o)| o.ord)
                .max()
                .map_or(0, |m| m + 1)
        };
        for p in &prims {
            match p {
                Command::Put { kind, id, value } => {
                    let key = (*kind, id.clone());
                    match objects.get_mut(&key) {
                        Some(o) => {
                            inverse.push(Command::Put {
                                kind: *kind,
                                id: id.clone(),
                                value: o.value.clone(),
                            });
                            o.value = value.clone();
                            o.version += 1;
                        }
                        None => {
                            inverse.push(Command::Delete {
                                kind: *kind,
                                id: id.clone(),
                            });
                            let ord = next_ord(&objects, *kind);
                            objects.insert(
                                key,
                                Object {
                                    ord,
                                    version: 1,
                                    value: value.clone(),
                                },
                            );
                        }
                    }
                    changed.push((*kind, id.clone()));
                }
                Command::Delete { kind, id } => {
                    let key = (*kind, id.clone());
                    let old = objects
                        .remove(&key)
                        .ok_or_else(|| refused(format!("no {} {id}", kind.as_str())))?;
                    inverse.push(Command::Put {
                        kind: *kind,
                        id: id.clone(),
                        value: old.value,
                    });
                    changed.push((*kind, id.clone()));
                }
                _ => unreachable!("expanded above"),
            }
        }
        inverse.reverse();
        let line = line_of(&objects)?;
        changed.sort();
        changed.dedup();
        let sections = if import {
            line.sections.iter().map(|s| s.id.clone()).collect()
        } else {
            affected_sections([&self.line, &line], &changed)
        };

        // Persist atomically: objects and log entry.
        let tx = self.conn.transaction()?;
        for (kind, id) in &changed {
            match objects.get(&(*kind, id.clone())) {
                Some(o) => {
                    tx.execute(
                        "INSERT INTO model_object (kind, id, ord, version, json) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(kind, id) DO UPDATE SET ord = excluded.ord, version = excluded.version, json = excluded.json",
                        params![kind.as_str(), id, o.ord, o.version, o.value.to_string()],
                    )?;
                }
                None => {
                    tx.execute(
                        "DELETE FROM model_object WHERE kind = ?1 AND id = ?2",
                        params![kind.as_str(), id],
                    )?;
                }
            }
        }
        let applied = Command::Batch { commands: prims };
        let inverse = Command::Batch { commands: inverse };
        // Versions after the change, to detect later edits by others.
        let after: Vec<(Kind, String, i64)> = changed
            .iter()
            .map(|(k, id)| {
                (
                    *k,
                    id.clone(),
                    objects.get(&(*k, id.clone())).map_or(0, |o| o.version),
                )
            })
            .collect();
        tx.execute(
            "INSERT INTO oplog (user, kind, command, inverse, objects, sections, undoes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                user,
                entry_kind,
                serde_json::to_string(&applied).expect("serialises"),
                serde_json::to_string(&inverse).expect("serialises"),
                serde_json::to_string(&after).expect("serialises"),
                serde_json::to_string(&sections).expect("serialises"),
                undoes
            ],
        )?;
        let seq = tx.last_insert_rowid();
        if let Some(u) = undoes {
            tx.execute(
                "UPDATE oplog SET undone_by = ?1 WHERE seq = ?2",
                params![seq, u],
            )?;
        }
        tx.commit()?;
        self.objects = objects;
        self.line = line;
        Ok(Applied {
            seq,
            objects: changed,
            sections,
        })
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
        ))
    }

    /// The operation log, oldest first.
    pub fn log(&self) -> Result<Vec<LogEntry>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by FROM oplog ORDER BY seq",
        )?;
        let rows = stmt.query_map([], Self::entry_row)?;
        let store = |e: serde_json::Error| SessionError::Store(e.to_string());
        rows.map(|r| {
            let (seq, time, user, kind, command, _, objects, sections, undoes, undone_by) = r?;
            let objects: Vec<(Kind, String, i64)> =
                serde_json::from_str(&objects).map_err(store)?;
            Ok(LogEntry {
                seq,
                time,
                user,
                kind,
                command: serde_json::from_str(&command).map_err(store)?,
                objects: objects.into_iter().map(|(k, id, _)| (k, id)).collect(),
                sections: serde_json::from_str(&sections).map_err(store)?,
                undoes,
                undone_by,
            })
        })
        .collect()
    }

    /// Reverses log entry `seq` as `user`, logged as `kind`. Refused if one
    /// of its objects was changed after it (unless `force`, used when
    /// rolling back everything after a point in reverse order).
    fn reverse(&mut self, user: &str, seq: i64, kind: &str) -> Result<Applied, SessionError> {
        let row = self
            .conn
            .query_row(
                "SELECT seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by FROM oplog WHERE seq = ?1",
                params![seq],
                Self::entry_row,
            )
            .optional()?
            .ok_or_else(|| refused(format!("no log entry {seq}")))?;
        let (_, _, _, entry_kind, command, inverse, _, _, _, undone_by) = row;
        if entry_kind == "import" {
            return Err(refused("the import cannot be undone"));
        }
        if let Some(by) = undone_by {
            return Err(refused(format!("entry {seq} was already reversed by {by}")));
        }
        let store = |e: serde_json::Error| SessionError::Store(e.to_string());
        // Conflicts are judged by content: every object must still be as
        // the entry left it (versions alone would also refuse after another
        // user changed an object and changed it back).
        let command: Command = serde_json::from_str(&command).map_err(store)?;
        let mut left: BTreeMap<(Kind, String), Option<Value>> = BTreeMap::new();
        if let Command::Batch { commands } = command {
            for c in commands {
                match c {
                    Command::Put { kind, id, value } => {
                        left.insert((kind, id), Some(value));
                    }
                    Command::Delete { kind, id } => {
                        left.insert((kind, id), None);
                    }
                    _ => {}
                }
            }
        }
        for ((k, id), value) in &left {
            let now = self.objects.get(&(*k, id.clone())).map(|o| &o.value);
            if now != value.as_ref() {
                return Err(refused(format!(
                    "{} {id} was changed after entry {seq}: undo those changes first or ask an admin to roll back",
                    k.as_str()
                )));
            }
        }
        let inverse: Command = serde_json::from_str(&inverse).map_err(store)?;
        self.apply_as(user, inverse, kind, Some(seq), false)
    }

    /// Undoes `user`'s last command that is not undone yet.
    pub fn undo(&mut self, user: &str) -> Result<Applied, SessionError> {
        self.require(user, Role::Designer)?;
        let seq: Option<i64> = self
            .conn
            .query_row(
                "SELECT seq FROM oplog WHERE user = ?1 AND kind IN ('do', 'redo') AND undone_by IS NULL ORDER BY seq DESC LIMIT 1",
                params![user],
                |r| r.get(0),
            )
            .optional()?;
        let seq = seq.ok_or_else(|| refused(format!("{user} has nothing to undo")))?;
        self.reverse(user, seq, "undo")
    }

    /// Redoes `user`'s last undo (if nothing of theirs happened since).
    pub fn redo(&mut self, user: &str) -> Result<Applied, SessionError> {
        self.require(user, Role::Designer)?;
        let last: Option<(i64, String, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT seq, kind, undone_by FROM oplog WHERE user = ?1 AND kind IN ('do', 'undo', 'redo') ORDER BY seq DESC LIMIT 1",
                params![user],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match last {
            Some((seq, kind, None)) if kind == "undo" => self.reverse(user, seq, "redo"),
            _ => Err(refused(format!("{user} has nothing to redo"))),
        }
    }

    /// Admin: reverses any log entry.
    pub fn rollback(&mut self, admin: &str, seq: i64) -> Result<Applied, SessionError> {
        self.require(admin, Role::Admin)?;
        self.reverse(admin, seq, "rollback")
    }

    /// Admin: brings the model back to its state just after log entry
    /// `seq` with one rollback entry; the entries after `seq` still in
    /// effect are marked as reversed by it.
    ///
    /// The earlier state is found by applying the inverses of all later
    /// entries, newest first (each inverse exactly reverses its entry, also
    /// when the entry was itself an undo); the difference to the current
    /// state is applied as one command, validated like any other.
    pub fn rollback_after(
        &mut self,
        admin: &str,
        seq: i64,
    ) -> Result<Option<Applied>, SessionError> {
        self.require(admin, Role::Admin)?;
        let mut stmt = self
            .conn
            .prepare("SELECT inverse FROM oplog WHERE seq > ?1 ORDER BY seq DESC")?;
        let inverses: Vec<String> = stmt
            .query_map(params![seq], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        let store = |e: serde_json::Error| SessionError::Store(e.to_string());
        let mut target: BTreeMap<(Kind, String), Value> = self
            .objects
            .iter()
            .map(|(k, o)| (k.clone(), o.value.clone()))
            .collect();
        for text in inverses {
            if let Command::Batch { commands } = serde_json::from_str(&text).map_err(store)? {
                for c in commands {
                    match c {
                        Command::Put { kind, id, value } => {
                            target.insert((kind, id), value);
                        }
                        Command::Delete { kind, id } => {
                            target.remove(&(kind, id));
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut diff = Vec::new();
        for (key, o) in &self.objects {
            match target.get(key) {
                Some(v) if *v == o.value => {}
                Some(v) => diff.push(Command::Put {
                    kind: key.0,
                    id: key.1.clone(),
                    value: v.clone(),
                }),
                None => diff.push(Command::Delete {
                    kind: key.0,
                    id: key.1.clone(),
                }),
            }
        }
        for (key, v) in &target {
            if !self.objects.contains_key(key) {
                diff.push(Command::Put {
                    kind: key.0,
                    id: key.1.clone(),
                    value: v.clone(),
                });
            }
        }
        if diff.is_empty() {
            return Ok(None);
        }
        let applied = self.apply_as(
            admin,
            Command::Batch { commands: diff },
            "rollback",
            None,
            false,
        )?;
        self.conn.execute(
            "UPDATE oplog SET undone_by = ?1 WHERE seq > ?2 AND seq < ?1 AND undone_by IS NULL",
            params![applied.seq, seq],
        )?;
        Ok(Some(applied))
    }
}

#[cfg(test)]
mod tests;
