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
//!   rollback, manage users). Admins can limit a user's or a role's edits
//!   to some kinds, objects or a plan area ([`collab`]).
//! - **Project lines** ([`lines`]): several designs of the line (the main
//!   line and its phases), each with its own objects, log and index; a
//!   session works on one at a time.
//! - **Version control** ([`versions`]): revisions, branches (the project
//!   lines), tags, diff, merge and revert of the line and its libraries;
//!   reviews of branches before they are merged ([`collab`]).
//! - **Import / export**: [`Session::import`] loads a line (`.ols.json`)
//!   into a project file, [`Session::export`] gives it back.
//!
//! Objects: the line settings (`meta`), routes, structures (with their
//! attachments), sections (with stringing and loads) and jumpers, each a
//! JSON row with a version number, in their order in the line.
//!
//! Schema (version 3, tables next to the point clouds of ADR-0007; older
//! files are migrated when opened — version 2 added the tables of
//! [`versions`] and [`collab`], version 3 the project lines of [`lines`]):
//!
//! ```sql
//! ols_meta(key PRIMARY KEY, value)        -- 'session_schema' = '3'
//! session_user(name PRIMARY KEY, role)
//! model_object(line, kind, id, ord, version, json, PRIMARY KEY(line, kind, id))
//! oplog(seq PRIMARY KEY, time, user, kind, command, inverse, objects,
//!       sections, undoes, undone_by, note, line)
//! spatial_index(line, kind, id, level, tx, ty, xmin, ymin, xmax, ymax)  -- [`spatial`]
//! ```
//!
//! The **spatial index** ([`spatial`], ADR-0008) gives every object its
//! plan bounds and home tile of the project's chunk grid, updated with
//! every command: [`Session::objects_in`] answers "what is in this area"
//! from the file, the basis for partial loading and streaming.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use ols_core::grid::{Bounds, Grid, Tile};
use ols_line::{Jumper, Line, Point, Route, Section, Stringing, Structure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Schema version of the session tables.
pub const SCHEMA_VERSION: u32 = 3;

/// Errors of a session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionError {
    /// The command was refused (invalid result, conflict, permission).
    Refused(String),
    /// The command would delete more than its object: it lists everything
    /// that would go with it and must be repeated with `confirm: true`.
    NeedsConfirmation(Box<Impact>),
    /// The project file could not be used.
    Store(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(m) => write!(f, "refused: {m}"),
            Self::NeedsConfirmation(i) => write!(f, "confirmation needed: {i}"),
            Self::Store(m) => write!(f, "project file: {m}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// What deleting an object takes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impact {
    /// Kind of the object deleted.
    pub kind: Kind,
    /// Its id.
    pub id: String,
    /// The part of it deleted, if not all of it (a structure's string:
    /// `string 2:1`).
    pub part: Option<String>,
    /// Its attachments with their insulators (structures):
    /// `set:phase label (insulator)`.
    pub attachments: Vec<String>,
    /// Sections deleted with it (a section cannot lose a point).
    pub sections: Vec<String>,
    /// Jumpers deleted with it (at its attachments, or at the points of
    /// deleted sections).
    pub jumpers: Vec<String>,
    /// Routes it is taken out of.
    pub routes_changed: Vec<String>,
    /// Routes deleted (fewer than two structures left).
    pub routes_deleted: Vec<String>,
    /// Alignment points tied to it, detached (they stay where they are):
    /// `route #index`.
    pub alignment_detached: Vec<String>,
}

impl Impact {
    fn new(kind: Kind, id: &str) -> Self {
        Self {
            kind,
            id: id.to_owned(),
            part: None,
            attachments: vec![],
            sections: vec![],
            jumpers: vec![],
            routes_changed: vec![],
            routes_deleted: vec![],
            alignment_detached: vec![],
        }
    }

    /// Whether the deletion takes anything else with it (then it needs a
    /// confirmation).
    pub fn has_dependents(&self) -> bool {
        !(self.attachments.is_empty()
            && self.sections.is_empty()
            && self.jumpers.is_empty()
            && self.routes_changed.is_empty()
            && self.routes_deleted.is_empty()
            && self.alignment_detached.is_empty())
    }

    /// The dependents as (heading, items), for listing.
    pub fn groups(&self) -> Vec<(&'static str, &[String])> {
        vec![
            ("attachments and insulators", &self.attachments[..]),
            ("sections", &self.sections[..]),
            ("jumpers", &self.jumpers[..]),
            (
                "routes deleted (fewer than two structures left)",
                &self.routes_deleted[..],
            ),
            ("taken out of routes", &self.routes_changed[..]),
            (
                "alignment points detached (they stay in place)",
                &self.alignment_detached[..],
            ),
        ]
    }

    /// What is deleted: `structure T3`, `structure T3 string 2:1`.
    pub fn what(&self) -> String {
        match &self.part {
            Some(p) => format!("{} {} {p}", self.kind.as_str(), self.id),
            None => format!("{} {}", self.kind.as_str(), self.id),
        }
    }

    /// A one-line summary for the log.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("delete {}", self.what())];
        for (title, items) in self.groups() {
            if !items.is_empty() {
                parts.push(format!("{title}: {}", items.join(", ")));
            }
        }
        parts.join("; ")
    }
}

impl fmt::Display for Impact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} also affects — ", self.summary())?;
        write!(f, "repeat with confirmation to apply")
    }
}

impl From<rusqlite::Error> for SessionError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Store(e.to_string())
    }
}

/// Whether `name` is a plain file name, safe to write in a folder on every
/// system: letters, digits, spaces and `. _ - + ( )`, not starting with a
/// dot or a space, not a Windows device name, at most 255 bytes.
pub fn safe_file_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with(['.', ' '])
        && !name.ends_with(['.', ' '])
        && !device
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || " ._-+()".contains(c))
}

pub(crate) fn refused(m: impl Into<String>) -> SessionError {
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
    /// Name as in commands (`structure`, `section`…).
    pub fn as_str(self) -> &'static str {
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
    /// The name: `viewer`, `designer` or `admin`.
    pub fn as_str(self) -> &'static str {
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
        /// Position among the objects of its kind (when re-creating a
        /// deleted object in its place; new objects go last).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ord: Option<i64>,
    },
    /// Delete an object. When it takes others with it (a structure's
    /// attachments, sections and jumpers; a section's jumpers; routes and
    /// alignment points), the command is refused with the list
    /// ([`SessionError::NeedsConfirmation`]) unless `confirm` is set; then
    /// everything listed is deleted (or detached) as one log entry.
    Delete {
        /// Kind.
        kind: Kind,
        /// Id.
        id: String,
        /// Confirms the listed dependent deletions.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        confirm: bool,
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
    /// Move an alignment point (of route `route`, or `"line"` for the
    /// line's own alignment). A point tied to a structure is detached.
    MoveAlignmentPoint {
        /// Route id (`"line"`: the line alignment).
        route: String,
        /// Point index.
        index: usize,
        /// New position.
        position: Point,
    },
    /// Insert a free alignment point before `index` (= length: at the end).
    InsertAlignmentPoint {
        /// Route id.
        route: String,
        /// Index.
        index: usize,
        /// Position.
        position: Point,
    },
    /// Remove an alignment point.
    RemoveAlignmentPoint {
        /// Route id.
        route: String,
        /// Index.
        index: usize,
    },
    /// Tie an alignment point to a structure: it moves under it and then
    /// follows it.
    TieAlignmentPoint {
        /// Route id.
        route: String,
        /// Index.
        index: usize,
        /// Structure id.
        structure: String,
    },
    /// Detach an alignment point from its structure (it stays in place).
    DetachAlignmentPoint {
        /// Route id.
        route: String,
        /// Index.
        index: usize,
    },
    /// Replace a route's alignment by points tied under each of its
    /// structures.
    AlignUnderStructures {
        /// Route id.
        route: String,
    },
    /// Same as `delete` with kind `structure`.
    DeleteStructure {
        /// Structure id.
        id: String,
        /// Confirms the listed deletions.
        #[serde(default)]
        confirm: bool,
    },
    /// Delete a structure's wire point with set `set` and what hangs only
    /// for it (its strings, and the wire points without a set between
    /// them): the structure's own are removed, the structure type's left
    /// out ([`ols_line::Structure::omit_sets`]). The sections through it and
    /// the jumpers at it (or at those sections) go with it, after a
    /// confirmation as for [`Command::Delete`].
    DeleteAttachment {
        /// Structure id.
        structure: String,
        /// Set.
        set: u32,
        /// Projects written before: the phase (with `set`, the set number
        /// [`ols_line::assembly::set_from_v1`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<u32>,
        /// Confirms the listed deletions.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        confirm: bool,
    },
    /// Rename a structure, section or jumper (it keeps its place); what
    /// refers to a structure follows: section points, jumper ends, routes
    /// and alignment points tied to it.
    Rename {
        /// Kind (structure, section or jumper).
        kind: Kind,
        /// Id.
        id: String,
        /// New id.
        new_id: String,
    },
    /// Renumber a structure's wire point (one of its own) from set `set`
    /// to `new_set`; the sections and jumpers there follow.
    RenameAttachment {
        /// Structure id.
        structure: String,
        /// Set.
        set: u32,
        /// Projects written before: the phase.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<u32>,
        /// New set.
        new_set: u32,
        /// Projects written before: the new phase.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_phase: Option<u32>,
    },
    /// Rename an electrical system; the sections given it follow.
    RenameSystem {
        /// System id.
        id: String,
        /// New id.
        new_id: String,
    },
    /// Replace the whole line by `line` (e.g. a saved project line made
    /// the working line again): objects not in it are deleted, the others
    /// put — one log entry, undone at once.
    ReplaceLine {
        /// The new line.
        line: Box<Line>,
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
    /// What the command was, in words (e.g. a structure deletion and what
    /// went with it).
    pub note: Option<String>,
    /// The project line it changed.
    pub line: String,
}

/// Result of an applied command.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// Log sequence number.
    pub seq: i64,
    /// Objects changed.
    pub objects: Vec<(Kind, String)>,
    /// Sections to recalculate: changed sections and sections through
    /// changed structures (jumpers are recalculated on their own).
    pub sections: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    name: String,
    #[serde(default)]
    crs: String,
    #[serde(default)]
    alignment: Vec<ols_line::AlignmentPoint>,
    #[serde(default)]
    display: ols_line::Display,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    systems: Vec<ols_line::electrical::System>,
    #[serde(
        default,
        skip_serializing_if = "ols_line::condition::CreepStretch::is_empty"
    )]
    creep_stretch: ols_line::condition::CreepStretch,
    #[serde(
        default,
        skip_serializing_if = "ols_cables::BimetallicModel::is_default"
    )]
    bimetallic: ols_cables::BimetallicModel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    circuit_starts: Vec<ols_line::SectionPoint>,
}

impl Meta {
    /// The settings of `line`.
    fn of(line: &Line) -> Self {
        Self {
            name: line.name.clone(),
            crs: line.crs.clone(),
            alignment: line.alignment.clone(),
            display: line.display.clone(),
            systems: line.systems.clone(),
            creep_stretch: line.creep_stretch.clone(),
            bimetallic: line.bimetallic,
            circuit_starts: line.circuit_starts.clone(),
        }
    }
}

/// The put of the line's settings (name, CRS, alignment, display, systems,
/// creep stretch) as in `line`, with a new alignment and/or new systems.
/// Every field is written: the put replaces the whole object.
pub fn line_settings_put(
    line: &Line,
    alignment: Option<Vec<ols_line::AlignmentPoint>>,
    systems: Option<Vec<ols_line::electrical::System>>,
) -> Command {
    let mut meta = Meta::of(line);
    if let Some(a) = alignment {
        meta.alignment = a;
    }
    if let Some(s) = systems {
        meta.systems = s;
    }
    Command::Put {
        kind: Kind::Meta,
        id: "line".into(),
        value: json(&meta),
        ord: None,
    }
}

/// A raw log row: seq, time, user, kind, command, inverse, objects,
/// sections, undoes, undone_by, note, line.
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
    Option<String>,
    String,
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
    grid: Grid,
    spatial: BTreeMap<(Kind, String), spatial::Entry>,
    /// The project line the commands act on ([`lines`]).
    current: String,
    /// The other project lines' states.
    others: BTreeMap<String, lines::LineState>,
    /// The last line renamed (from, to): a session working on it follows.
    renamed: Option<(String, String)>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("objects", &self.objects.len())
            .finish_non_exhaustive()
    }
}

/// Removes the structure's own wire point with set `set` and what hangs
/// only for it: its strings and the wire points without a set between them
/// (strings and points other wire points still need stay).
fn remove_set(s: &mut ols_line::Structure, set: u32) {
    let Some(label) = s.wire_at_set(set).map(|w| w.label.clone()) else {
        return;
    };
    let c = s.component(&label);
    let others: Vec<u32> = c
        .wires
        .iter()
        .filter_map(|&i| s.wires[i].set)
        .filter(|&x| x != set)
        .collect();
    if others.is_empty() {
        let wires: Vec<String> = c.wires.iter().map(|&i| s.wires[i].label.clone()).collect();
        let strings: Vec<String> = c
            .strings
            .iter()
            .map(|&i| s.strings[i].label.clone())
            .collect();
        s.wires.retain(|w| !wires.contains(&w.label));
        s.strings.retain(|x| !strings.contains(&x.label));
    } else {
        // Shared (a TAK): only its wire point and the strings to it.
        s.wires.retain(|w| w.label != label);
        s.strings.retain(|x| x.wire != label);
    }
}

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("model objects serialise")
}

/// Objects of a line, in line order.
fn objects_of(line: &Line) -> Vec<(Kind, String, Value)> {
    let mut out = vec![(Kind::Meta, "line".to_owned(), json(&Meta::of(line)))];
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
        systems: meta.systems,
        creep_stretch: meta.creep_stretch,
        bimetallic: meta.bimetallic,
        circuit_starts: meta.circuit_starts,
        condition_cases: Default::default(),
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
/// change, and before it for deleted ones): changed sections and sections
/// through changed structures.
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
                // Jumpers hang on the solved clamps but do not change the
                // sections: they are recalculated on their own.
                Kind::Jumper | Kind::Meta | Kind::Route => {}
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
                 line TEXT NOT NULL DEFAULT 'main',
                 kind TEXT NOT NULL, id TEXT NOT NULL, ord INTEGER NOT NULL,
                 version INTEGER NOT NULL, json TEXT NOT NULL, PRIMARY KEY (line, kind, id));
             CREATE TABLE IF NOT EXISTS oplog (
                 seq INTEGER PRIMARY KEY, time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 user TEXT NOT NULL, kind TEXT NOT NULL, command TEXT NOT NULL, inverse TEXT NOT NULL,
                 objects TEXT NOT NULL, sections TEXT NOT NULL,
                 undoes INTEGER, undone_by INTEGER, note TEXT,
                 line TEXT NOT NULL DEFAULT 'main');
             CREATE TABLE IF NOT EXISTS line_snapshot (
                 name TEXT PRIMARY KEY, time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 user TEXT NOT NULL, note TEXT NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS library_file (
                 kind TEXT NOT NULL, name TEXT NOT NULL,
                 time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 user TEXT NOT NULL, text TEXT NOT NULL, PRIMARY KEY (kind, name));",
        )?;
        // Added within schema 1 before its first release: add the column to
        // project files made by earlier builds.
        let has_note: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('oplog') WHERE name = 'note'")?
            .exists([])?;
        if !has_note {
            conn.execute("ALTER TABLE oplog ADD COLUMN note TEXT", [])?;
        }
        // Schema 3: every project line has its own objects, log and index.
        lines::migrate(&conn)?;
        conn.execute_batch(spatial::SCHEMA)?;
        conn.execute_batch(surfaces::SCHEMA)?;
        conn.execute_batch(versions::SCHEMA)?;
        conn.execute_batch(collab::SCHEMA)?;
        lines::migrate_descriptions(&conn)?;
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
        versions::init(&conn)?;
        let mut by_line: BTreeMap<String, BTreeMap<(Kind, String), Object>> = BTreeMap::new();
        {
            let mut stmt =
                conn.prepare("SELECT line, kind, id, ord, version, json FROM model_object")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?;
            for row in rows {
                let (line, kind, id, ord, version, text) = row?;
                let value =
                    serde_json::from_str(&text).map_err(|e| SessionError::Store(e.to_string()))?;
                by_line.entry(line).or_default().insert(
                    (Kind::parse(&kind)?, id),
                    Object {
                        ord,
                        version,
                        value,
                    },
                );
            }
        }
        let mut session = Self {
            conn,
            objects: BTreeMap::new(),
            line: lines::empty_line(),
            grid,
            spatial: BTreeMap::new(),
            current: versions::MAIN.to_owned(),
            others: BTreeMap::new(),
            renamed: None,
        };
        session.load_lines(by_line)?;
        Ok(session)
    }

    /// The current line.
    pub fn line(&self) -> &Line {
        &self.line
    }

    /// The project's chunk grid.
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// The spatial index entry of an object (bounds and home tile).
    pub fn spatial_entry(&self, kind: Kind, id: &str) -> Option<spatial::Entry> {
        self.spatial.get(&(kind, id.to_owned())).copied()
    }

    /// Objects whose plan bounds touch `area`, from the index in the
    /// project file (by tile ranges, without reading the objects).
    pub fn objects_in(
        &self,
        area: &Bounds,
    ) -> Result<Vec<(Kind, String, spatial::Entry)>, SessionError> {
        spatial::query(&self.conn, &self.current, &self.grid, area)
    }

    /// Number of objects of each kind by home tile.
    pub fn home_tiles(&self) -> BTreeMap<Tile, BTreeMap<Kind, usize>> {
        let mut out: BTreeMap<Tile, BTreeMap<Kind, usize>> = BTreeMap::new();
        for ((kind, _), e) in &self.spatial {
            *out.entry(e.tile).or_default().entry(*kind).or_default() += 1;
        }
        out
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

    /// The library files kept in the project of one kind (e.g. `cables`,
    /// `structures`, `insulators`, `weather`): file name and text, by name.
    pub fn library_files(&self, kind: &str) -> Result<Vec<(String, String)>, SessionError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, text FROM library_file WHERE kind = ?1 ORDER BY name")?;
        let rows = stmt.query_map(params![kind], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Keeps library file `name` of `kind` in the project (replacing one
    /// of that name). Libraries are part of the project, not of the line:
    /// not logged, not undone.
    pub fn put_library_file(
        &self,
        user: &str,
        kind: &str,
        name: &str,
        text: &str,
    ) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        self.check_library_edit(user)?;
        if kind.is_empty() || !safe_file_name(name) {
            return Err(refused(format!(
                "library file \"{name}\" of \"{kind}\": bad name"
            )));
        }
        self.conn.execute(
            "INSERT INTO library_file (kind, name, user, text) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(kind, name) DO UPDATE SET text = excluded.text, user = excluded.user,
                 time = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![kind, name, user, text],
        )?;
        Ok(())
    }

    /// Removes library file `name` of `kind` from the project.
    pub fn delete_library_file(
        &self,
        user: &str,
        kind: &str,
        name: &str,
    ) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        self.check_library_edit(user)?;
        self.conn.execute(
            "DELETE FROM library_file WHERE kind = ?1 AND name = ?2",
            params![kind, name],
        )?;
        Ok(())
    }

    /// Writes a consistent copy of the project file to `path` (which must
    /// not exist): what a client joining a server receives. Takes about as
    /// long as reading the file.
    pub fn copy_to(&self, path: &Path) -> Result<(), SessionError> {
        self.conn
            .execute("VACUUM INTO ?1", params![path.to_string_lossy()])?;
        Ok(())
    }

    /// The last log entry's sequence number (0: none).
    pub fn last_seq(&self) -> Result<i64, SessionError> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(MAX(seq), 0) FROM oplog", [], |r| r.get(0))?)
    }

    /// Writes everything into the project file itself (no change is ever
    /// waiting: every command is stored as it is applied).
    pub fn checkpoint(&self) -> Result<(), SessionError> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// A setting of the project kept for the applications (not part of the
    /// line, not logged), e.g. `app.weather` — the weather case file used.
    pub fn setting(&self, key: &str) -> Result<Option<String>, SessionError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM ols_meta WHERE key = ?1",
                params![format!("setting.{key}")],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Sets (or, with `None`, removes) a setting ([`Session::setting`]).
    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<(), SessionError> {
        let key = format!("setting.{key}");
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO ols_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?,
            None => self
                .conn
                .execute("DELETE FROM ols_meta WHERE key = ?1", params![key])?,
        };
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

    /// Refuses unless `user` has at least the role `at_least`.
    pub fn require(&self, user: &str, at_least: Role) -> Result<(), SessionError> {
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
            .map(|(kind, id, value)| Command::Put {
                kind,
                id,
                value,
                ord: None,
            })
            .collect();
        self.apply_as(
            user,
            Command::Batch { commands },
            "import",
            None,
            true,
            false,
        )
    }

    /// The line as a line file.
    pub fn export(&self) -> Line {
        self.line.clone()
    }

    /// Applies a command of `user`.
    pub fn apply(&mut self, user: &str, command: Command) -> Result<Applied, SessionError> {
        self.require(user, Role::Designer)?;
        self.check_line_write(user)?;
        self.apply_as(user, command, "do", None, false, true)
    }

    /// The alignment of route `route` (`"line"`: the line's own).
    fn alignment_of(&self, route: &str) -> Result<Vec<ols_line::AlignmentPoint>, SessionError> {
        if route == "line" {
            return Ok(self.line.alignment.clone());
        }
        self.line
            .routes
            .iter()
            .find(|r| r.id == route)
            .map(|r| r.alignment.clone())
            .ok_or_else(|| refused(format!("no route {route}")))
    }

    /// The command setting the line's creep-stretch criteria (Criteria ›
    /// Creep-Stretch): one logged, undoable put of the line's settings.
    pub fn creep_stretch_command(
        &self,
        creep_stretch: ols_line::condition::CreepStretch,
    ) -> Command {
        let meta = Meta {
            creep_stretch,
            ..Meta::of(&self.line)
        };
        Command::Put {
            kind: Kind::Meta,
            id: "line".into(),
            value: json(&meta),
            ord: None,
        }
    }

    /// The command setting the line's bimetallic conductor model
    /// (Criteria › Bimetallic conductor model): one logged, undoable put of
    /// the line's settings.
    pub fn bimetallic_command(&self, bimetallic: ols_cables::BimetallicModel) -> Command {
        let mut line = self.line.clone();
        line.bimetallic = bimetallic;
        line_settings_put(&line, None, None)
    }

    /// A put replacing the alignment of `route`.
    fn put_alignment(
        &self,
        route: &str,
        alignment: Vec<ols_line::AlignmentPoint>,
    ) -> Result<Command, SessionError> {
        if route == "line" {
            let meta = Meta {
                alignment,
                ..Meta::of(&self.line)
            };
            return Ok(Command::Put {
                kind: Kind::Meta,
                id: "line".into(),
                value: json(&meta),
                ord: None,
            });
        }
        let mut r = self
            .line
            .routes
            .iter()
            .find(|r| r.id == route)
            .ok_or_else(|| refused(format!("no route {route}")))?
            .clone();
        r.alignment = alignment;
        Ok(Command::Put {
            kind: Kind::Route,
            id: route.to_owned(),
            value: json(&r),
            ord: None,
        })
    }

    /// Expands commands into puts and deletes. `user`: a command given by
    /// a user (deletions cascade, with confirmation); otherwise a replayed
    /// inverse (applied as recorded).
    fn primitives(
        &self,
        command: Command,
        out: &mut Vec<Command>,
        notes: &mut Vec<String>,
        user: bool,
    ) -> Result<(), SessionError> {
        let point = |alignment: &[ols_line::AlignmentPoint], index: usize, route: &str| {
            if index < alignment.len() {
                Ok(())
            } else {
                Err(refused(format!(
                    "route {route} has no alignment point {index} ({} points)",
                    alignment.len()
                )))
            }
        };
        match command {
            Command::Put { .. } => out.push(command),
            Command::Delete { kind, id, confirm } => {
                if !user {
                    out.push(Command::Delete {
                        kind,
                        id,
                        confirm: false,
                    });
                    return Ok(());
                }
                let impact = self.deletion_impact(kind, &id)?;
                if impact.has_dependents() && !confirm {
                    return Err(SessionError::NeedsConfirmation(Box::new(impact)));
                }
                let del = |kind, id: &String| Command::Delete {
                    kind,
                    id: id.clone(),
                    confirm: false,
                };
                for j in &impact.jumpers {
                    out.push(del(Kind::Jumper, j));
                }
                for sec in &impact.sections {
                    out.push(del(Kind::Section, sec));
                }
                for r in &impact.routes_deleted {
                    out.push(del(Kind::Route, r));
                }
                // Routes kept: without the structure, tied points detached.
                let mut touched: Vec<String> = impact.routes_changed.clone();
                if kind == Kind::Structure {
                    for r in &self.line.routes {
                        if !touched.contains(&r.id)
                            && !impact.routes_deleted.contains(&r.id)
                            && r.alignment
                                .iter()
                                .any(|a| a.structure.as_deref() == Some(&id))
                        {
                            touched.push(r.id.clone());
                        }
                    }
                    if self
                        .line
                        .alignment
                        .iter()
                        .any(|a| a.structure.as_deref() == Some(&id))
                    {
                        touched.push("line".into());
                    }
                }
                for r in &touched {
                    let mut alignment = self.alignment_of(r)?;
                    for a in &mut alignment {
                        if a.structure.as_deref() == Some(&id) {
                            a.structure = None;
                        }
                    }
                    let mut put = self.put_alignment(r, alignment)?;
                    if let Command::Put { value, .. } = &mut put
                        && r != "line"
                    {
                        let mut route: Route =
                            serde_json::from_value(value.clone()).expect("route serialises");
                        route.structures.retain(|s| *s != id);
                        *value = json(&route);
                    }
                    out.push(put);
                }
                out.push(del(kind, &id));
                if impact.has_dependents() {
                    notes.push(impact.summary());
                } else {
                    notes.push(format!("delete {} {id}", kind.as_str()));
                }
            }
            Command::DeleteStructure { id, confirm } => self.primitives(
                Command::Delete {
                    kind: Kind::Structure,
                    id,
                    confirm,
                },
                out,
                notes,
                user,
            )?,
            Command::DeleteAttachment {
                structure,
                set,
                phase,
                confirm,
            } => {
                let set = phase.map_or(set, |p| ols_line::assembly::set_from_v1(set, p));
                let impact = self.attachment_deletion(&structure, set)?;
                if user && impact.has_dependents() && !confirm {
                    return Err(SessionError::NeedsConfirmation(Box::new(impact)));
                }
                let del = |kind, id: &String| Command::Delete {
                    kind,
                    id: id.clone(),
                    confirm: false,
                };
                for j in &impact.jumpers {
                    out.push(del(Kind::Jumper, j));
                }
                for sec in &impact.sections {
                    out.push(del(Kind::Section, sec));
                }
                let mut s = self
                    .line
                    .structure(&structure)
                    .expect("checked by the impact")
                    .clone();
                remove_set(&mut s, set);
                if s.structure_type.is_some() && !s.omit_sets.contains(&set) {
                    s.omit_sets.push(set);
                }
                out.push(Command::Put {
                    kind: Kind::Structure,
                    id: structure,
                    value: json(&s),
                    ord: None,
                });
                notes.push(impact.summary());
            }
            Command::Rename { kind, id, new_id } => {
                let new_id = new_id.trim().to_owned();
                if new_id.is_empty() || new_id == id {
                    return Err(refused(format!("{} {id}: give a new id", kind.as_str())));
                }
                let old = self
                    .objects
                    .get(&(kind, id.clone()))
                    .ok_or_else(|| refused(format!("no {} {id}", kind.as_str())))?;
                if self.objects.contains_key(&(kind, new_id.clone())) {
                    return Err(refused(format!(
                        "{} {new_id} exists already",
                        kind.as_str()
                    )));
                }
                let mut value = old.value.clone();
                value["id"] = Value::String(new_id.clone());
                out.push(Command::Put {
                    kind,
                    id: new_id.clone(),
                    value,
                    ord: Some(old.ord),
                });
                match kind {
                    Kind::Structure => {
                        for sec in &self.line.sections {
                            if sec.points.iter().any(|p| p.structure == id) {
                                let mut sec = sec.clone();
                                for p in &mut sec.points {
                                    if p.structure == id {
                                        p.structure = new_id.clone();
                                    }
                                }
                                out.push(Command::Put {
                                    kind: Kind::Section,
                                    id: sec.id.clone(),
                                    value: json(&sec),
                                    ord: None,
                                });
                            }
                        }
                        for j in &self.line.jumpers {
                            if j.from.structure == id || j.to.structure == id {
                                let mut j = j.clone();
                                for p in [&mut j.from, &mut j.to] {
                                    if p.structure == id {
                                        p.structure = new_id.clone();
                                    }
                                }
                                out.push(Command::Put {
                                    kind: Kind::Jumper,
                                    id: j.id.clone(),
                                    value: json(&j),
                                    ord: None,
                                });
                            }
                        }
                        let tie = |a: &mut Vec<ols_line::AlignmentPoint>| {
                            let mut changed = false;
                            for p in a {
                                if p.structure.as_deref() == Some(&id) {
                                    p.structure = Some(new_id.clone());
                                    changed = true;
                                }
                            }
                            changed
                        };
                        for r in &self.line.routes {
                            let mut r = r.clone();
                            let mut changed = tie(&mut r.alignment);
                            for s in &mut r.structures {
                                if *s == id {
                                    *s = new_id.clone();
                                    changed = true;
                                }
                            }
                            if changed {
                                out.push(Command::Put {
                                    kind: Kind::Route,
                                    id: r.id.clone(),
                                    value: json(&r),
                                    ord: None,
                                });
                            }
                        }
                        let mut alignment = self.line.alignment.clone();
                        if tie(&mut alignment) {
                            out.push(self.put_alignment("line", alignment)?);
                        }
                    }
                    Kind::Section | Kind::Jumper => {}
                    other => {
                        return Err(refused(format!(
                            "a {} is not renamed this way",
                            other.as_str()
                        )));
                    }
                }
                out.push(Command::Delete {
                    kind,
                    id: id.clone(),
                    confirm: false,
                });
                notes.push(format!("rename {} {id} to {new_id}", kind.as_str()));
            }
            Command::RenameAttachment {
                structure,
                set,
                phase,
                new_set,
                new_phase,
            } => {
                let set = phase.map_or(set, |p| ols_line::assembly::set_from_v1(set, p));
                let new_set =
                    new_phase.map_or(new_set, |p| ols_line::assembly::set_from_v1(new_set, p));
                let mut s = self
                    .line
                    .structure(&structure)
                    .ok_or_else(|| refused(format!("no structure {structure}")))?
                    .clone();
                if set == new_set {
                    return Err(refused("give another set"));
                }
                if s.wire_at_set(new_set).is_some() {
                    return Err(refused(format!(
                        "{structure} has a wire point with set {new_set} already"
                    )));
                }
                let w = s
                    .wires
                    .iter_mut()
                    .find(|w| w.set == Some(set))
                    .ok_or_else(|| {
                        refused(format!(
                            "{structure} set {set} is not a wire point of its own (one of its type: change it first, it then becomes its own)"
                        ))
                    })?;
                w.set = Some(new_set);
                if s.structure_type.is_some() && !s.omit_sets.contains(&set) {
                    // The type's wire point of the old number stays out.
                    s.omit_sets.push(set);
                }
                out.push(Command::Put {
                    kind: Kind::Structure,
                    id: structure.clone(),
                    value: json(&s),
                    ord: None,
                });
                let old = ols_line::SectionPoint::new(structure.clone(), set);
                let new = ols_line::SectionPoint::new(structure.clone(), new_set);
                for sec in &self.line.sections {
                    if sec.points.contains(&old) {
                        let mut sec = sec.clone();
                        for p in &mut sec.points {
                            if *p == old {
                                *p = new.clone();
                            }
                        }
                        out.push(Command::Put {
                            kind: Kind::Section,
                            id: sec.id.clone(),
                            value: json(&sec),
                            ord: None,
                        });
                    }
                }
                for j in &self.line.jumpers {
                    if j.from == old || j.to == old {
                        let mut j = j.clone();
                        for p in [&mut j.from, &mut j.to] {
                            if *p == old {
                                *p = new.clone();
                            }
                        }
                        out.push(Command::Put {
                            kind: Kind::Jumper,
                            id: j.id.clone(),
                            value: json(&j),
                            ord: None,
                        });
                    }
                }
                notes.push(format!(
                    "renumber wire point {structure} set {set} to {new_set}"
                ));
            }
            Command::RenameSystem { id, new_id } => {
                let new_id = new_id.trim().to_owned();
                let mut systems = self.line.systems.clone();
                if new_id.is_empty() || systems.iter().any(|s| s.id == new_id) {
                    return Err(refused(format!("system {new_id}: empty or taken")));
                }
                systems
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or_else(|| refused(format!("no system {id}")))?
                    .id = new_id.clone();
                let meta = Meta {
                    systems,
                    ..Meta::of(&self.line)
                };
                out.push(Command::Put {
                    kind: Kind::Meta,
                    id: "line".into(),
                    value: json(&meta),
                    ord: None,
                });
                for sec in &self.line.sections {
                    if sec.system.as_deref() == Some(&id) {
                        let mut sec = sec.clone();
                        sec.system = Some(new_id.clone());
                        out.push(Command::Put {
                            kind: Kind::Section,
                            id: sec.id.clone(),
                            value: json(&sec),
                            ord: None,
                        });
                    }
                }
                notes.push(format!("rename system {id} to {new_id}"));
            }
            Command::ReplaceLine { line } => {
                let new = objects_of(&line);
                let keep: std::collections::BTreeSet<(Kind, String)> =
                    new.iter().map(|(k, id, _)| (*k, id.clone())).collect();
                for (k, id) in self.objects.keys() {
                    if !keep.contains(&(*k, id.clone())) {
                        out.push(Command::Delete {
                            kind: *k,
                            id: id.clone(),
                            confirm: false,
                        });
                    }
                }
                for (kind, id, value) in new {
                    if self
                        .objects
                        .get(&(kind, id.clone()))
                        .is_some_and(|o| o.value == value)
                    {
                        continue;
                    }
                    out.push(Command::Put {
                        kind,
                        id,
                        value,
                        ord: None,
                    });
                }
                notes.push(format!("replace the line by \"{}\"", line.name));
            }
            Command::Batch { commands } => {
                for c in commands {
                    self.primitives(c, out, notes, user)?;
                }
            }
            Command::MoveStructure { id, position } => {
                let s = self
                    .line
                    .structure(&id)
                    .ok_or_else(|| refused(format!("no structure {id}")))?;
                let moved = s.moved_to(position);
                out.push(Command::Put {
                    kind: Kind::Structure,
                    id: id.clone(),
                    value: json(&moved),
                    ord: None,
                });
                // Alignment points tied to it follow.
                let mut routes: Vec<String> = self
                    .line
                    .routes
                    .iter()
                    .filter(|r| {
                        r.alignment
                            .iter()
                            .any(|a| a.structure.as_deref() == Some(&id))
                    })
                    .map(|r| r.id.clone())
                    .collect();
                if self
                    .line
                    .alignment
                    .iter()
                    .any(|a| a.structure.as_deref() == Some(&id))
                {
                    routes.push("line".into());
                }
                for r in routes {
                    let mut alignment = self.alignment_of(&r)?;
                    for a in &mut alignment {
                        if a.structure.as_deref() == Some(&id) {
                            a.position = position;
                        }
                    }
                    out.push(self.put_alignment(&r, alignment)?);
                }
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
                    ord: None,
                });
            }
            Command::MoveAlignmentPoint {
                route,
                index,
                position,
            } => {
                let mut alignment = self.alignment_of(&route)?;
                point(&alignment, index, &route)?;
                if let Some(s) = alignment[index].structure.take() {
                    notes.push(format!(
                        "route {route}: alignment point {index} detached from {s} and moved"
                    ));
                }
                alignment[index].position = position;
                out.push(self.put_alignment(&route, alignment)?);
            }
            Command::InsertAlignmentPoint {
                route,
                index,
                position,
            } => {
                let mut alignment = self.alignment_of(&route)?;
                if index > alignment.len() {
                    return Err(refused(format!(
                        "route {route}: index {index} beyond the end"
                    )));
                }
                alignment.insert(index, ols_line::AlignmentPoint::free(position));
                out.push(self.put_alignment(&route, alignment)?);
            }
            Command::RemoveAlignmentPoint { route, index } => {
                let mut alignment = self.alignment_of(&route)?;
                point(&alignment, index, &route)?;
                alignment.remove(index);
                out.push(self.put_alignment(&route, alignment)?);
            }
            Command::TieAlignmentPoint {
                route,
                index,
                structure,
            } => {
                let mut alignment = self.alignment_of(&route)?;
                point(&alignment, index, &route)?;
                let s = self
                    .line
                    .structure(&structure)
                    .ok_or_else(|| refused(format!("no structure {structure}")))?;
                alignment[index] = ols_line::AlignmentPoint {
                    position: s.position,
                    structure: Some(structure),
                };
                out.push(self.put_alignment(&route, alignment)?);
            }
            Command::DetachAlignmentPoint { route, index } => {
                let mut alignment = self.alignment_of(&route)?;
                point(&alignment, index, &route)?;
                alignment[index].structure = None;
                out.push(self.put_alignment(&route, alignment)?);
            }
            Command::AlignUnderStructures { route } => {
                let r = self
                    .line
                    .routes
                    .iter()
                    .find(|r| r.id == route)
                    .ok_or_else(|| refused(format!("no route {route}")))?;
                let alignment = self.line.alignment_under_structures(r);
                out.push(self.put_alignment(&route, alignment)?);
            }
        }
        Ok(())
    }

    /// What deleting object `kind`/`id` takes with it (nothing is
    /// changed).
    pub fn deletion_impact(&self, kind: Kind, id: &str) -> Result<Impact, SessionError> {
        let mut impact = Impact::new(kind, id);
        match kind {
            Kind::Meta => return Err(refused("the line settings cannot be deleted")),
            Kind::Structure => return self.structure_deletion(id),
            Kind::Section => {
                let sec = self
                    .line
                    .sections
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| refused(format!("no section {id}")))?;
                impact.jumpers = self
                    .line
                    .jumpers
                    .iter()
                    .filter(|j| sec.points.contains(&j.from) || sec.points.contains(&j.to))
                    .map(|j| j.id.clone())
                    .collect();
            }
            Kind::Route => {
                if !self.line.routes.iter().any(|r| r.id == id) {
                    return Err(refused(format!("no route {id}")));
                }
            }
            Kind::Jumper => {
                if !self.line.jumpers.iter().any(|j| j.id == id) {
                    return Err(refused(format!("no jumper {id}")));
                }
            }
        }
        Ok(impact)
    }

    /// What deleting string `set`:`phase` of structure `id` takes with it
    /// (nothing is changed): the sections through it, the jumpers at it
    /// and at those sections' points.
    pub fn attachment_deletion(&self, id: &str, set: u32) -> Result<Impact, SessionError> {
        let s = self
            .line
            .structure(id)
            .ok_or_else(|| refused(format!("no structure {id}")))?;
        if s.wire_at_set(set).is_none() && s.structure_type.is_none() {
            return Err(refused(format!(
                "structure {id} has no wire point with set {set}"
            )));
        }
        let mut impact = Impact::new(Kind::Structure, id);
        impact.part = Some(format!("set {set}"));
        let point = ols_line::SectionPoint::new(id, set);
        let sections: Vec<&ols_line::Section> = self
            .line
            .sections
            .iter()
            .filter(|sec| sec.points.contains(&point))
            .collect();
        impact.sections = sections.iter().map(|sec| sec.id.clone()).collect();
        impact.jumpers = self
            .line
            .jumpers
            .iter()
            .filter(|j| {
                j.from == point
                    || j.to == point
                    || sections
                        .iter()
                        .any(|sec| sec.points.contains(&j.from) || sec.points.contains(&j.to))
            })
            .map(|j| j.id.clone())
            .collect();
        Ok(impact)
    }

    /// What deleting structure `id` takes with it (nothing is changed).
    pub fn structure_deletion(&self, id: &str) -> Result<Impact, SessionError> {
        let s = self
            .line
            .structure(id)
            .ok_or_else(|| refused(format!("no structure {id}")))?;
        let mut impact = Impact::new(Kind::Structure, id);
        impact.attachments = s
            .wires
            .iter()
            .map(|w| {
                let mut t = match w.set {
                    Some(set) => format!("set {set} {}", w.label),
                    None => w.label.clone(),
                };
                let strings: Vec<String> = s
                    .strings_to(&w.label)
                    .map(|x| match &x.insulator {
                        Some(i) => format!("{} from {} ({i})", x.label, x.top),
                        None => format!("{} from {}", x.label, x.top),
                    })
                    .collect();
                if !strings.is_empty() {
                    t.push_str(&format!(" [{}]", strings.join(", ")));
                }
                t
            })
            .collect();
        let sections: Vec<&ols_line::Section> = self
            .line
            .sections
            .iter()
            .filter(|sec| sec.points.iter().any(|p| p.structure == id))
            .collect();
        impact.sections = sections.iter().map(|sec| sec.id.clone()).collect();
        // Jumpers at its attachments, and at points of the deleted sections.
        impact.jumpers = self
            .line
            .jumpers
            .iter()
            .filter(|j| {
                j.from.structure == id
                    || j.to.structure == id
                    || sections
                        .iter()
                        .any(|sec| sec.points.contains(&j.from) || sec.points.contains(&j.to))
            })
            .map(|j| j.id.clone())
            .collect();
        for r in &self.line.routes {
            if r.structures.iter().any(|x| x == id) {
                if r.structures.iter().filter(|x| *x != id).count() < 2 && r.alignment.len() < 2 {
                    impact.routes_deleted.push(r.id.clone());
                    continue;
                }
                impact.routes_changed.push(r.id.clone());
            }
            for (k, a) in r.alignment.iter().enumerate() {
                if a.structure.as_deref() == Some(id) {
                    impact.alignment_detached.push(format!("{} #{k}", r.id));
                }
            }
        }
        for (k, a) in self.line.alignment.iter().enumerate() {
            if a.structure.as_deref() == Some(id) {
                impact.alignment_detached.push(format!("line #{k}"));
            }
        }
        Ok(impact)
    }

    /// Applies `command` (validated, atomic), logs it as `entry_kind`.
    /// `cascade`: a user's command (deletions take their dependents, after
    /// a confirmation); otherwise puts and deletes are applied as given.
    fn apply_as(
        &mut self,
        user: &str,
        command: Command,
        entry_kind: &str,
        undoes: Option<i64>,
        import: bool,
        cascade: bool,
    ) -> Result<Applied, SessionError> {
        let mut prims = Vec::new();
        let mut notes = Vec::new();
        self.primitives(command, &mut prims, &mut notes, cascade)?;
        let note = (!notes.is_empty()).then(|| notes.join("; "));
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
                Command::Put {
                    kind,
                    id,
                    value,
                    ord: wanted,
                } => {
                    let key = (*kind, id.clone());
                    match objects.get_mut(&key) {
                        Some(o) => {
                            // A place given moves it among its kind (a
                            // revision checked out); the inverse moves it
                            // back.
                            let moved = wanted.filter(|w| *w != o.ord);
                            inverse.push(Command::Put {
                                kind: *kind,
                                id: id.clone(),
                                value: o.value.clone(),
                                ord: moved.map(|_| o.ord),
                            });
                            o.value = value.clone();
                            o.version += 1;
                            if let Some(w) = moved {
                                o.ord = w;
                            }
                        }
                        None => {
                            inverse.push(Command::Delete {
                                kind: *kind,
                                id: id.clone(),
                                confirm: false,
                            });
                            let ord = wanted.unwrap_or_else(|| next_ord(&objects, *kind));
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
                Command::Delete { kind, id, .. } => {
                    let key = (*kind, id.clone());
                    let old = objects
                        .remove(&key)
                        .ok_or_else(|| refused(format!("no {} {id}", kind.as_str())))?;
                    // Re-created in its place by the inverse.
                    inverse.push(Command::Put {
                        kind: *kind,
                        id: id.clone(),
                        value: old.value,
                        ord: Some(old.ord),
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
        // Moved structures move their sections, jumpers and routes too:
        // the index is recomputed from the line, only differences written.
        let spatial = spatial::entries(&line, &self.grid);
        // Edit rights limited to kinds, objects or an area (admins: all).
        if !import {
            self.check_edit(user, &changed, &self.spatial, &spatial)?;
        }

        // Persist atomically: objects and log entry.
        let tx = self.conn.transaction()?;
        for (kind, id) in &changed {
            match objects.get(&(*kind, id.clone())) {
                Some(o) => {
                    tx.execute(
                        "INSERT INTO model_object (kind, id, ord, version, json, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT(line, kind, id) DO UPDATE SET ord = excluded.ord, version = excluded.version, json = excluded.json",
                        params![kind.as_str(), id, o.ord, o.version, o.value.to_string(), self.current],
                    )?;
                }
                None => {
                    tx.execute(
                        "DELETE FROM model_object WHERE kind = ?1 AND id = ?2 AND line = ?3",
                        params![kind.as_str(), id, self.current],
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
            "INSERT INTO oplog (user, kind, command, inverse, objects, sections, undoes, note, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                user,
                entry_kind,
                serde_json::to_string(&applied).expect("serialises"),
                serde_json::to_string(&inverse).expect("serialises"),
                serde_json::to_string(&after).expect("serialises"),
                serde_json::to_string(&sections).expect("serialises"),
                undoes,
                note,
                self.current
            ],
        )?;
        let seq = tx.last_insert_rowid();
        if let Some(u) = undoes {
            tx.execute(
                "UPDATE oplog SET undone_by = ?1 WHERE seq = ?2",
                params![seq, u],
            )?;
        }
        spatial::write_diff(&tx, &self.current, &self.spatial, &spatial)?;
        tx.commit()?;
        self.objects = objects;
        self.line = line;
        self.spatial = spatial;
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
            r.get(10)?,
            r.get(11)?,
        ))
    }

    /// The operation log, oldest first.
    pub fn log(&self) -> Result<Vec<LogEntry>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by, note, line FROM oplog ORDER BY seq",
        )?;
        let rows = stmt.query_map([], Self::entry_row)?;
        let store = |e: serde_json::Error| SessionError::Store(e.to_string());
        rows.map(|r| {
            let (
                seq,
                time,
                user,
                kind,
                command,
                _,
                objects,
                sections,
                undoes,
                undone_by,
                note,
                line,
            ) = r?;
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
                note,
                line,
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
                "SELECT seq, time, user, kind, command, inverse, objects, sections, undoes, undone_by, note, line FROM oplog WHERE seq = ?1",
                params![seq],
                Self::entry_row,
            )
            .optional()?
            .ok_or_else(|| refused(format!("no log entry {seq}")))?;
        let (_, _, _, entry_kind, command, inverse, _, _, _, undone_by, _, line) = row;
        if line != self.current {
            return Err(refused(format!(
                "entry {seq} is of project line {line}: work on that line to reverse it"
            )));
        }
        if entry_kind == "import" {
            return Err(refused("the import cannot be undone"));
        }
        if entry_kind == "reproject" {
            return Err(refused(
                "the move into another coordinate system cannot be undone (the terrain would not \
                 follow): reproject the project back instead",
            ));
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
                    Command::Put {
                        kind, id, value, ..
                    } => {
                        left.insert((kind, id), Some(value));
                    }
                    Command::Delete { kind, id, .. } => {
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
        let applied = self.apply_as(user, inverse, kind, Some(seq), false, false)?;
        // Say what was reversed.
        self.conn.execute(
            "UPDATE oplog SET note = 'reverses ' || ?2 || COALESCE(': ' || (SELECT note FROM oplog WHERE seq = ?2), '') WHERE seq = ?1",
            params![applied.seq, seq],
        )?;
        Ok(applied)
    }

    /// Undoes `user`'s last command that is not undone yet.
    pub fn undo(&mut self, user: &str) -> Result<Applied, SessionError> {
        self.require(user, Role::Designer)?;
        self.check_line_write(user)?;
        let seq: Option<i64> = self
            .conn
            .query_row(
                "SELECT seq FROM oplog WHERE user = ?1 AND line = ?2 AND kind IN ('do', 'redo') AND undone_by IS NULL ORDER BY seq DESC LIMIT 1",
                params![user, self.current],
                |r| r.get(0),
            )
            .optional()?;
        let seq = seq.ok_or_else(|| refused(format!("{user} has nothing to undo")))?;
        self.reverse(user, seq, "undo")
    }

    /// Redoes `user`'s last undo (if nothing of theirs happened since).
    pub fn redo(&mut self, user: &str) -> Result<Applied, SessionError> {
        self.require(user, Role::Designer)?;
        self.check_line_write(user)?;
        let last: Option<(i64, String, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT seq, kind, undone_by FROM oplog WHERE user = ?1 AND line = ?2 AND kind IN ('do', 'undo', 'redo') ORDER BY seq DESC LIMIT 1",
                params![user, self.current],
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
            .prepare("SELECT inverse FROM oplog WHERE seq > ?1 AND line = ?2 ORDER BY seq DESC")?;
        let inverses: Vec<String> = stmt
            .query_map(params![seq, self.current], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        let store = |e: serde_json::Error| SessionError::Store(e.to_string());
        let mut target: BTreeMap<(Kind, String), (Value, Option<i64>)> = self
            .objects
            .iter()
            .map(|(k, o)| (k.clone(), (o.value.clone(), Some(o.ord))))
            .collect();
        for text in inverses {
            if let Command::Batch { commands } = serde_json::from_str(&text).map_err(store)? {
                for c in commands {
                    match c {
                        Command::Put {
                            kind,
                            id,
                            value,
                            ord,
                        } => {
                            let key = (kind, id);
                            let ord = ord.or_else(|| target.get(&key).and_then(|t| t.1));
                            target.insert(key, (value, ord));
                        }
                        Command::Delete { kind, id, .. } => {
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
                Some((v, _)) if *v == o.value => {}
                Some((v, _)) => diff.push(Command::Put {
                    kind: key.0,
                    id: key.1.clone(),
                    value: v.clone(),
                    ord: None,
                }),
                None => diff.push(Command::Delete {
                    kind: key.0,
                    id: key.1.clone(),
                    confirm: false,
                }),
            }
        }
        for (key, (v, ord)) in &target {
            if !self.objects.contains_key(key) {
                diff.push(Command::Put {
                    kind: key.0,
                    id: key.1.clone(),
                    value: v.clone(),
                    ord: *ord,
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
            false,
        )?;
        self.conn.execute(
            "UPDATE oplog SET undone_by = ?1 WHERE seq > ?2 AND seq < ?1 AND undone_by IS NULL AND line = ?3",
            params![applied.seq, seq, self.current],
        )?;
        Ok(Some(applied))
    }
}

pub mod collab;
pub mod impact;
pub mod lines;
pub mod live;
pub mod reproject;
pub mod spatial;
pub mod surfaces;
pub mod versions;

pub use live::Live;

#[cfg(test)]
mod lines_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod versions_tests;
