// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The session (ADR-0005): the only way to change a model.
//!
//! A [`Session`] owns a line model stored in the project file (`.ohl`,
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
//! - **Import / export**: [`Session::import`] loads a line (`.ohl.json`)
//!   into a project file, [`Session::export`] gives it back.
//!
//! Objects: the line settings (`meta`), routes, structures (with their
//! attachments), sections (with stringing and loads) and jumpers, each a
//! JSON row with a version number, in their order in the line.
//!
//! Schema (version 6, tables next to the point clouds of ADR-0007; older
//! files are migrated when opened — version 2 added the tables of
//! [`versions`] and [`collab`], version 3 the project lines of [`lines`],
//! version 4 the library files as objects of each project line, version
//! 5 the heights' vertical datum in the line settings, nothing to
//! convert; version 6 lines kept as differences from a revision, older
//! lines read as they are):
//!
//! ```sql
//! ols_meta(key PRIMARY KEY, value)        -- 'session_schema' = '6'
//! session_user(name PRIMARY KEY, role)
//! model_object(line, kind, id, ord, version, json, gone, PRIMARY KEY(line, kind, id))
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
use std::sync::Arc;

use ohl_core::grid::{Bounds, Grid, Tile};
use ohl_line::{Jumper, Line, Point, Route, Section, Stringing, Structure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Schema version of the session tables: 6 keeps a project line started
/// since as the revision it starts from (`vc_ref.base`) and only what it
/// changes (`model_object` rows, tombstones with `gone` for objects
/// deleted) — an older program would read such a line as nearly empty, so
/// it refuses the project; lines of older projects keep all their objects
/// and are read as before ([`lines`]). 5 lets the line settings row
/// (`meta` `line`) hold the heights' vertical datum (`height_datum`, line
/// file version 7) — an older program refuses such a project instead of
/// dropping the datum when it writes the row; nothing in an older project
/// changes when it opens (it has none). 4 keeps library files as objects
/// of each project line (before, a `library_file` table shared by all
/// lines).
pub const SCHEMA_VERSION: u32 = 6;

/// The object id of library file `name` of `kind`.
pub(crate) fn library_id(kind: &str, name: &str) -> String {
    format!("{kind}/{name}")
}

/// The object of library file `name` of `kind`.
pub(crate) fn library_value(kind: &str, name: &str, text: &str) -> Value {
    serde_json::json!({
        "id": library_id(kind, name),
        "kind": kind,
        "name": name,
        "text": text,
    })
}

/// A library object's kind, name and text.
pub(crate) fn library_entry(v: &Value) -> Option<(String, String, String)> {
    Some((
        v.get("kind")?.as_str()?.to_owned(),
        v.get("name")?.as_str()?.to_owned(),
        v.get("text")?.as_str()?.to_owned(),
    ))
}

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
    /// Spacers deleted with it (on deleted sections).
    pub spacers: Vec<String>,
    /// Alignments deleted with it (a route's alignments).
    pub alignments_deleted: Vec<String>,
    /// Alignments an alignment point deleted is taken off (when it is on
    /// more than one; the others keep their order).
    pub alignments_changed: Vec<String>,
    /// Structures that gave this route themselves: it is taken out.
    pub structures_changed: Vec<String>,
    /// Alignment points tied to it, detached (they stay where they are).
    pub alignment_detached: Vec<String>,
    /// A structure replaced: its own points, strings and wire points over
    /// its type (and the type's left out), dropped.
    pub overrides: Vec<String>,
    /// A structure replaced: jumper support clamps on wire points the new
    /// structure does not have, dropped (the jumpers stay).
    pub supports: Vec<String>,
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
            spacers: vec![],
            alignments_deleted: vec![],
            alignments_changed: vec![],
            structures_changed: vec![],
            alignment_detached: vec![],
            overrides: vec![],
            supports: vec![],
        }
    }

    /// Whether the deletion takes anything else with it (then it needs a
    /// confirmation).
    pub fn has_dependents(&self) -> bool {
        !(self.attachments.is_empty()
            && self.sections.is_empty()
            && self.jumpers.is_empty()
            && self.spacers.is_empty()
            && self.alignments_deleted.is_empty()
            && self.alignments_changed.is_empty()
            && self.structures_changed.is_empty()
            && self.alignment_detached.is_empty()
            && self.overrides.is_empty()
            && self.supports.is_empty())
    }

    /// The dependents as (heading, items), for listing.
    pub fn groups(&self) -> Vec<(&'static str, &[String])> {
        vec![
            ("attachments and insulators", &self.attachments[..]),
            ("sections", &self.sections[..]),
            ("jumpers", &self.jumpers[..]),
            ("spacers", &self.spacers[..]),
            ("alignments deleted", &self.alignments_deleted[..]),
            (
                "alignments it is taken off (their other points keep their order)",
                &self.alignments_changed[..],
            ),
            (
                "structures taken out of the route",
                &self.structures_changed[..],
            ),
            (
                "alignment points detached (they stay in place)",
                &self.alignment_detached[..],
            ),
            (
                "own points, strings and wire points dropped (overrides of the old type)",
                &self.overrides[..],
            ),
            (
                "jumper support clamps dropped (no such wire point on the new structure)",
                &self.supports[..],
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
    /// A route (a path of the line, made of alignments).
    Route,
    /// An alignment (its route and its points, by id, in order).
    Alignment,
    /// An alignment point (on one or more alignments).
    AlignmentPoint,
    /// A structure with its attachments.
    Structure,
    /// A wire section.
    Section,
    /// A jumper.
    Jumper,
    /// A performed check's report ([`reports`]): kept with the line's
    /// branch, not part of the line itself.
    Report,
    /// An interphase spacer (between span points of two sections).
    Spacer,
    /// A library file kept with the project line (cables, structure types,
    /// insulators, weather cases, …): id `kind/name`, its text; not part of
    /// the line itself ([`Session::library_files`]).
    Library,
    /// A point cloud of the project file, by name: its points changed by
    /// [`Command::ChangePoints`] (no object row; not part of the line, kept
    /// by the project for every line, [`points`]).
    PointCloud,
    /// An annotation ([`annotations`]): a kept measurement, a note or a
    /// leader; kept with the line's branch, not part of the line itself.
    Annotation,
}

impl Kind {
    /// Name as in commands (`structure`, `section`…).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::Route => "route",
            Self::Alignment => "alignment",
            Self::AlignmentPoint => "alignment_point",
            Self::Structure => "structure",
            Self::Section => "section",
            Self::Jumper => "jumper",
            Self::Report => "report",
            Self::Spacer => "spacer",
            Self::Library => "library",
            Self::PointCloud => "point_cloud",
            Self::Annotation => "annotation",
        }
    }

    fn parse(s: &str) -> Result<Self, SessionError> {
        Ok(match s {
            "meta" => Self::Meta,
            "route" => Self::Route,
            "alignment" => Self::Alignment,
            "alignment_point" => Self::AlignmentPoint,
            "structure" => Self::Structure,
            "section" => Self::Section,
            "jumper" => Self::Jumper,
            "report" => Self::Report,
            "spacer" => Self::Spacer,
            "library" => Self::Library,
            "point_cloud" => Self::PointCloud,
            "annotation" => Self::Annotation,
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
    /// the line settings: `{name, crs, display, systems, …}`).
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
    /// Give spans `spans` (first and last, 0 = the section's first span,
    /// as the line file counts them) of section `section` a local wind
    /// factor (roadmap O15): the weather case's wind there times `factor`
    /// in every case with wind; 1 takes them back to the case's wind.
    /// Refused: no such section or spans, a factor out of 0 to 10.
    SetWindFactor {
        /// Section id.
        section: String,
        /// The first and last span, 0 = first.
        spans: [usize; 2],
        /// The factor (1: the case's wind).
        factor: f64,
    },
    /// Move point `index` of alignment `route` (`"line"`: the alignment
    /// `main` of older projects); a point shared by other alignments moves
    /// on all of them. A point tied to a structure is detached.
    MoveAlignmentPoint {
        /// Alignment id.
        route: String,
        /// Point index.
        index: usize,
        /// New position.
        position: Point,
    },
    /// Insert a point into alignment `route` before `index` (= length: at
    /// the end): a new free point at `position`, or the existing point
    /// `point` (shared with its other alignments: a junction).
    InsertAlignmentPoint {
        /// Alignment id.
        route: String,
        /// Index.
        index: usize,
        /// Position (of a new point).
        position: Point,
        /// An existing point to put there instead of a new one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        point: Option<String>,
    },
    /// Delete point `index` of alignment `route`: it is taken off every
    /// alignment it is on (their other points keep their order).
    RemoveAlignmentPoint {
        /// Alignment id.
        route: String,
        /// Index.
        index: usize,
    },
    /// Tie an alignment point to a structure: it moves under it and then
    /// follows it.
    TieAlignmentPoint {
        /// Alignment id.
        route: String,
        /// Index.
        index: usize,
        /// Structure id.
        structure: String,
    },
    /// Detach an alignment point from its structure (it stays in place).
    DetachAlignmentPoint {
        /// Alignment id.
        route: String,
        /// Index.
        index: usize,
    },
    /// Replace an alignment's points by points tied under each of its
    /// structures (a point already tied to one is reused); its old points
    /// left on no alignment are deleted (replaced).
    AlignUnderStructures {
        /// Alignment id.
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
    /// out ([`ohl_line::Structure::omit_sets`]). The sections through it and
    /// the jumpers at it (or at those sections) go with it, after a
    /// confirmation as for [`Command::Delete`].
    DeleteAttachment {
        /// Structure id.
        structure: String,
        /// Set.
        set: u32,
        /// Projects written before: the phase (with `set`, the set number
        /// [`ohl_line::assembly::set_from_v1`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<u32>,
        /// Confirms the listed deletions.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        confirm: bool,
    },
    /// Replace structure `id` by another (structure building decision 8,
    /// [`replace`]): it keeps its id, position, orientation, height
    /// adjustment, description and routes. What ends at an old set goes to
    /// the new set given in `sets`, else to the same set when the new
    /// structure has it, else is removed — with the jumpers and spacers
    /// that go with the sections removed — after a confirmation as for
    /// [`Command::Delete`] (also asked when the old structure's own points,
    /// strings or wire points, or jumper support clamps, are dropped).
    ReplaceStructure {
        /// Structure id.
        id: String,
        /// The new structure: its type and extension, or no type and its
        /// own points, strings and wire points (its id, place and
        /// orientation are the old one's).
        structure: Box<Structure>,
        /// A typed new structure: its wire points as placed from the
        /// structure library (the session does not read it).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        wires: Vec<replace::NewWire>,
        /// Old set → new set (none: removed), for the sets not kept as
        /// they are.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sets: Vec<replace::SetMove>,
        /// The old structure's attachment points matched to the new one's
        /// joints ([`ohl_line::links::match_points`]): each linked (the
        /// new structure carries it as its own point, linked so) or
        /// removed (listed).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        points: Vec<replace::PointMove>,
        /// Confirms the listed removals.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        confirm: bool,
    },
    /// Link attachment point `label` of typed structure `structure` to
    /// joint `joint` of its type at `offset` (structure axes, m): the
    /// Link tool; relinking a linked point is the same command. The point
    /// becomes the structure's own, at `position` — the joint as placed
    /// plus the offset, given by the caller
    /// ([`ohl_line::links::linked`]: the session does not read the
    /// structure library).
    LinkPoint {
        /// Structure id.
        structure: String,
        /// The attachment point's label.
        label: String,
        /// The joint's id.
        joint: String,
        /// From the joint to the point, structure axes, m.
        #[serde(default, skip_serializing_if = "is_zero_point")]
        offset: Point,
        /// Where the point is then, m.
        position: Point,
    },
    /// Unlink attachment point `label` of typed structure `structure`: it
    /// stays at `position`, the structure's loads refused until it is
    /// linked again.
    UnlinkPoint {
        /// Structure id.
        structure: String,
        /// The attachment point's label.
        label: String,
        /// Where it stays, m.
        position: Point,
    },
    /// Give several attachment points of typed structure `structure` their
    /// links at once (after its type changed: [`ohl_line::links`]), each
    /// replacing the structure's own point of its label.
    RelinkPoints {
        /// Structure id.
        structure: String,
        /// The points, linked (or unlinked) as they are to be.
        points: Vec<ohl_line::StructurePoint>,
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
    /// Relabel a point of a section's spans or of a jumper (`kind`:
    /// section or jumper, `id` its id); the spreads, spacers and taps
    /// naming it follow.
    RenamePoint {
        /// Section or jumper.
        kind: Kind,
        /// Its id.
        id: String,
        /// The point's label.
        label: String,
        /// Its new label.
        new_label: String,
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
    /// Change points of the project's point cloud `cloud` ([`points`]):
    /// each point `from`, exactly as stored, becomes `to` (other
    /// coordinates, another class), in its block of the cloud. Refused when
    /// a point is not there (changed since). The clouds belong to the
    /// project: every project line sees the change; it is logged and
    /// undone on the line it was made on.
    ChangePoints {
        /// The point cloud's name.
        cloud: String,
        /// The points changed, in order (at most [`points::MAX_POINTS`];
        /// written packed from [`points::packed::PACK_FROM`] points on).
        #[serde(with = "points::packed")]
        points: Vec<points::PointChange>,
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
    /// Points of the project's point clouds changed (by cloud, as
    /// applied: an undo gives the changes back), [`Command::ChangePoints`].
    pub points: Vec<(String, Vec<points::PointChange>)>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    name: String,
    #[serde(default)]
    crs: String,
    /// Schema 5: the heights' vertical datum (left out when none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    height_datum: Option<ohl_line::HeightDatum>,
    /// Projects before routes and alignments: the line's own alignment
    /// (made the alignment `main` when the project opens).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    alignment: Vec<ohl_line::OldPoint>,
    #[serde(default)]
    display: ohl_line::Display,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    systems: Vec<ohl_line::electrical::System>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    circuits: Vec<ohl_line::circuits::Circuit>,
    #[serde(
        default,
        skip_serializing_if = "ohl_line::condition::CreepStretch::is_empty"
    )]
    creep_stretch: ohl_line::condition::CreepStretch,
    #[serde(
        default,
        skip_serializing_if = "ohl_cables::BimetallicModel::is_default"
    )]
    bimetallic: ohl_cables::BimetallicModel,
    #[serde(default, skip_serializing_if = "ohl_line::SolverOptions::is_default")]
    solver: ohl_line::SolverOptions,
    #[serde(default, skip_serializing_if = "ohl_line::Defaults::is_default")]
    defaults: ohl_line::Defaults,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    circuit_starts: Vec<ohl_line::SectionPoint>,
    #[serde(default, skip_serializing_if = "ohl_line::StructureLoads::is_default")]
    structure_loads: ohl_line::StructureLoads,
}

impl Meta {
    /// The settings of `line`.
    fn of(line: &Line) -> Self {
        Self {
            name: line.name.clone(),
            crs: line.crs.clone(),
            height_datum: line.height_datum.clone(),
            alignment: line.alignment.clone(),
            display: line.display.clone(),
            systems: line.systems.clone(),
            circuits: line.circuits.clone(),
            creep_stretch: line.creep_stretch.clone(),
            bimetallic: line.bimetallic,
            solver: line.solver,
            defaults: line.defaults,
            circuit_starts: line.circuit_starts.clone(),
            structure_loads: line.structure_loads.clone(),
        }
    }
}

/// The line's settings in rows of their own (`meta` objects), so that
/// two users changing different settings do not overwrite each other and
/// undo one each: `line` (name, CRS, display) and one row per setting.
/// Projects written before kept them all in `line`: read as defaults
/// under the rows ([`meta_of`]), until the first change writes the rows.
const META_ROWS: [&str; 8] = [
    "systems",
    "circuits",
    "creep_stretch",
    "bimetallic",
    "solver",
    "defaults",
    "circuit_starts",
    "structure_loads",
];

/// The settings `meta` as rows: (id, value).
fn meta_rows(meta: &Meta) -> Vec<(String, Value)> {
    let Value::Object(mut all) = json(meta) else {
        unreachable!("settings are an object")
    };
    let mut out = Vec::new();
    for key in META_ROWS {
        all.remove(key);
        // Every row written, defaults too (a row overrides the old one).
        let v = match key {
            "systems" => json(&meta.systems),
            "circuits" => json(&meta.circuits),
            "creep_stretch" => json(&meta.creep_stretch),
            "bimetallic" => json(&meta.bimetallic),
            "solver" => json(&meta.solver),
            "defaults" => json(&meta.defaults),
            "circuit_starts" => json(&meta.circuit_starts),
            _ => json(&meta.structure_loads),
        };
        out.push((key.to_owned(), serde_json::json!({ key: v })));
    }
    out.insert(0, ("line".to_owned(), Value::Object(all)));
    out
}

/// The puts of the settings `meta`, every row (those unchanged are left
/// out when applied).
fn meta_puts(meta: &Meta) -> Command {
    Command::Batch {
        commands: meta_rows(meta)
            .into_iter()
            .map(|(id, value)| Command::Put {
                kind: Kind::Meta,
                id,
                value,
                ord: None,
            })
            .collect(),
    }
}

/// The settings kept in the `meta` rows of `objects`: `line` under the
/// others.
fn meta_of(objects: &BTreeMap<(Kind, String), Object>) -> Result<Option<Meta>, String> {
    let Some(base) = objects.get(&(Kind::Meta, "line".to_owned())) else {
        return Ok(None);
    };
    let Value::Object(mut all) = (*base.value).clone() else {
        return Err("line settings: not an object".into());
    };
    for key in META_ROWS {
        if let Some(Value::Object(row)) = objects
            .get(&(Kind::Meta, key.to_owned()))
            .map(|o| (*o.value).clone())
        {
            all.extend(row);
        }
    }
    serde_json::from_value(Value::Object(all))
        .map(Some)
        .map_err(|e| format!("line settings: {e}"))
}

/// The put of the settings row of the circuit starts.
fn starts_put(starts: &[ohl_line::SectionPoint]) -> Command {
    Command::Put {
        kind: Kind::Meta,
        id: "circuit_starts".into(),
        value: serde_json::json!({ "circuit_starts": starts }),
        ord: None,
    }
}

/// The puts of the settings rows that differ between `before` (the line
/// the change was made on) and `after`: a user's change of one setting
/// puts that row only, never another user's settings as they were.
pub fn line_settings_change(before: &Line, after: &Line) -> Command {
    let old = meta_rows(&Meta::of(before));
    Command::Batch {
        commands: meta_rows(&Meta::of(after))
            .into_iter()
            .zip(old)
            .filter(|((_, a), (_, b))| a != b)
            .map(|((id, value), _)| Command::Put {
                kind: Kind::Meta,
                id,
                value,
                ord: None,
            })
            .collect(),
    }
}

/// A change of circuits ([`ohl_line::circuits`]) as one command: the
/// settings rows that differ between `before` and `after` (the circuits
/// list) and the puts of the sections whose circuit differs (their
/// `circuit_name`). Refused when `after` has a circuit contradiction
/// `before` had not ([`Line::check_circuit_change`]).
pub fn line_circuits_change(before: &Line, after: &Line) -> Result<Command, String> {
    after.check_circuit_change(before)?;
    let mut commands = vec![line_settings_change(before, after)];
    for s in &after.sections {
        let old = before.sections.iter().find(|x| x.id == s.id);
        if old.is_some_and(|o| o.circuit_name != s.circuit_name) {
            commands.push(Command::Put {
                kind: Kind::Section,
                id: s.id.clone(),
                value: json(s),
                ord: None,
            });
        }
    }
    Ok(Command::Batch { commands })
}

/// The puts of the line's settings (name, CRS, display, systems, creep
/// stretch…) as in `line`, with new systems when given: one row each
/// (`META_ROWS`), those unchanged left out when applied.
pub fn line_settings_put(
    line: &Line,
    systems: Option<Vec<ohl_line::electrical::System>>,
) -> Command {
    let mut meta = Meta::of(line);
    if let Some(s) = systems {
        meta.systems = s;
    }
    meta_puts(&meta)
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

/// One object of the model. Its JSON is shared (not copied) by the
/// lines and states holding it unchanged.
#[derive(Debug, Clone, PartialEq)]
struct Object {
    ord: i64,
    version: i64,
    value: Arc<Value>,
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
fn remove_set(s: &mut ohl_line::Structure, set: u32) {
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

/// A point at the origin (an offset of none).
pub(crate) fn is_zero_point(p: &Point) -> bool {
    *p == [0.0; 3]
}

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("model objects serialise")
}

/// The put of object `kind`/`id`.
fn put<T: Serialize>(kind: Kind, id: &str, v: &T) -> Command {
    Command::Put {
        kind,
        id: id.to_owned(),
        value: json(v),
        ord: None,
    }
}

/// Objects of a line, in line order.
fn objects_of(line: &Line) -> Vec<(Kind, String, Value)> {
    let mut out: Vec<(Kind, String, Value)> = meta_rows(&Meta::of(line))
        .into_iter()
        .map(|(id, v)| (Kind::Meta, id, v))
        .collect();
    out.extend(
        line.routes
            .iter()
            .map(|r| (Kind::Route, r.id.clone(), json(r))),
    );
    out.extend(
        line.alignments
            .iter()
            .map(|a| (Kind::Alignment, a.id.clone(), json(a))),
    );
    out.extend(
        line.alignment_points
            .iter()
            .map(|p| (Kind::AlignmentPoint, p.id.clone(), json(p))),
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
    out.extend(
        line.spacers
            .iter()
            .map(|s| (Kind::Spacer, s.id.clone(), json(s))),
    );
    out
}

/// The line made of `objects`, validated. Routes in the form before
/// alignments (a value put back by undoing an older command) are read
/// into routes and alignments.
fn line_of(objects: &BTreeMap<(Kind, String), Object>) -> Result<Line, SessionError> {
    let mut line = raw_line_of(objects)?;
    line.upgrade_routes();
    line.place_strings().map_err(|e| refused(e.to_string()))?;
    line.validate().map_err(|e| refused(e.to_string()))?;
    Ok(line)
}

/// Objects as written: kind, id, value.
type ObjectList = Vec<(Kind, String, Value)>;

/// Older projects: `objects` read into routes, alignments and alignment
/// points ([`Line::upgrade_routes`]); `None` when nothing changes.
/// `before_routes`: a project from before routes and alignments (its
/// routes were each their own alignment, or followed its structures, and
/// the line had its own alignment).
fn upgraded_objects(
    objects: &BTreeMap<(Kind, String), Object>,
    before_routes: bool,
) -> Result<Option<ObjectList>, SessionError> {
    if objects.is_empty() {
        return Ok(None);
    }
    let mut line = raw_line_of(objects)?;
    line.version = if before_routes {
        2
    } else {
        ohl_line::LINE_FILE_VERSION
    };
    let before = line.clone();
    line.upgrade_routes();
    if line == before {
        return Ok(None);
    }
    line.version = ohl_line::LINE_FILE_VERSION;
    Ok(Some(objects_of(&line)))
}

/// The line made of `objects`, not validated.
fn raw_line_of(objects: &BTreeMap<(Kind, String), Object>) -> Result<Line, SessionError> {
    let bad = |m: String| refused(m);
    let mut ordered: Vec<(&(Kind, String), &Object)> = objects.iter().collect();
    ordered.sort_by_key(|(k, o)| (k.0, o.ord));
    let meta = meta_of(objects)
        .map_err(bad)?
        .ok_or_else(|| bad("the project has no line".into()))?;
    let mut line = Line {
        alignments: Default::default(),
        alignment_points: Default::default(),
        format: ohl_line::LINE_FILE_FORMAT.to_owned(),
        version: ohl_line::LINE_FILE_VERSION,
        name: meta.name,
        crs: meta.crs,
        height_datum: meta.height_datum,
        alignment: meta.alignment,
        routes: vec![],
        structures: vec![],
        sections: vec![],
        jumpers: vec![],
        display: meta.display,
        systems: meta.systems,
        circuits: meta.circuits,
        creep_stretch: meta.creep_stretch,
        bimetallic: meta.bimetallic,
        solver: meta.solver,
        circuit_starts: meta.circuit_starts,
        structure_loads: meta.structure_loads,
        condition_cases: Default::default(),
        spacers: vec![],
        defaults: meta.defaults,
    };
    for ((kind, id), o) in ordered {
        let v = (*o.value).clone();
        let parse_err = |e: serde_json::Error| bad(format!("{} {id}: {e}", kind.as_str()));
        match kind {
            Kind::Meta => {}
            Kind::Route => line
                .routes
                .push(serde_json::from_value::<Route>(v).map_err(parse_err)?),
            Kind::Alignment => line
                .alignments
                .push(serde_json::from_value::<ohl_line::Alignment>(v).map_err(parse_err)?),
            Kind::AlignmentPoint => line
                .alignment_points
                .push(serde_json::from_value::<ohl_line::AlignmentPoint>(v).map_err(parse_err)?),
            Kind::Structure => line
                .structures
                .push(serde_json::from_value::<Structure>(v).map_err(parse_err)?),
            Kind::Section => line
                .sections
                .push(serde_json::from_value::<Section>(v).map_err(parse_err)?),
            Kind::Jumper => line
                .jumpers
                .push(serde_json::from_value::<Jumper>(v).map_err(parse_err)?),
            // Kept with the line, not part of it.
            Kind::Report | Kind::Library | Kind::PointCloud | Kind::Annotation => {}
            Kind::Spacer => line
                .spacers
                .push(serde_json::from_value::<ohl_line::Spacer>(v).map_err(parse_err)?),
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
    // Starts left by deletions before they followed them, spacers' bare
    // ends.
    line.read_older();
    Ok(line)
}

/// Sections to recalculate after `changed` objects, the line `lines[0]`
/// before and `lines[1]` after: the one rule of [`ohl_line::diff`] (the
/// sections that differ, through a structure that does, at a jumper's end,
/// joined by a spacer, read a line setting that does, or given the wind
/// from the other side); none when nothing of the line changed (a report,
/// a library file, points).
fn affected_sections(lines: [&Line; 2], changed: &[(Kind, String)]) -> Vec<String> {
    if changed.iter().all(|(k, _)| {
        matches!(
            k,
            Kind::Report | Kind::Library | Kind::PointCloud | Kind::Annotation
        )
    }) {
        return Vec::new();
    }
    let [a, b] = lines;
    ohl_line::diff::sections_to_solve_again(a, b)
        .into_iter()
        .collect()
}

/// Upgrade steps of older project files that come before the current
/// tables are made, in schema order. A new file has none of them. Each
/// step looks for what it changes, so a file of an in-between build (its
/// schema number already raised) is upgraded too, and a done step does
/// nothing; the steps after the tables are [`lines::migrate_descriptions`]
/// (schema 3) and [`migrate_library_files`] (schema 4).
fn upgrade_before_tables(conn: &Connection) -> Result<(), SessionError> {
    // Schema 1, before its first release: the log's note column.
    let has_note: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('oplog') WHERE name = 'note'")?
        .exists([])?;
    if !has_note {
        conn.execute("ALTER TABLE oplog ADD COLUMN note TEXT", [])?;
    }
    // Schema 3: every project line has its own objects, log and index.
    lines::migrate(conn)?;
    // Schema 6: lines kept as differences have tombstones.
    lines::migrate_tombstones(conn)
}

/// Schema 4: library files of the `library_file` table (schema 3 and
/// before: one set for all project lines) made `library` objects of every
/// project line, then the table dropped. Nothing to do once done.
fn migrate_library_files(conn: &Connection) -> Result<(), SessionError> {
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
    if !lines.iter().any(|l| l == versions::MAIN) {
        lines.push(versions::MAIN.to_owned());
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

impl Session {
    /// Opens a project file (created if missing; an empty project has no
    /// line until [`Session::import`]).
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        let conn = Connection::open(path)?;
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
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS ols_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
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
            upgrade_before_tables(&conn)?;
        }
        conn.execute_batch(spatial::SCHEMA)?;
        conn.execute_batch(surfaces::SCHEMA)?;
        conn.execute_batch(versions::SCHEMA)?;
        conn.execute_batch(collab::SCHEMA)?;
        if has_meta {
            lines::migrate_descriptions(&conn)?;
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
        versions::init(&conn)?;
        if has_meta {
            migrate_library_files(&conn)?;
        }
        let mut by_line: BTreeMap<String, BTreeMap<(Kind, String), Object>> = BTreeMap::new();
        // Tombstones of lines kept as differences ([`lines`]).
        let mut gone: BTreeMap<String, BTreeSet<(Kind, String)>> = BTreeMap::new();
        // Their base's objects at a place of their own (`gone` = 2).
        let mut placed: lines::Places = BTreeMap::new();
        {
            let mut stmt =
                conn.prepare("SELECT line, kind, id, ord, version, json, gone FROM model_object")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })?;
            for row in rows {
                let (line, kind, id, ord, version, text, gone_as) = row?;
                if gone_as == 2 {
                    placed
                        .entry(line)
                        .or_default()
                        .insert((Kind::parse(&kind)?, id), (ord, version));
                    continue;
                }
                if gone_as == 1 {
                    gone.entry(line)
                        .or_default()
                        .insert((Kind::parse(&kind)?, id));
                    continue;
                }
                let value: Value =
                    serde_json::from_str(&text).map_err(|e| SessionError::Store(e.to_string()))?;
                by_line.entry(line).or_default().insert(
                    (Kind::parse(&kind)?, id),
                    Object {
                        ord,
                        version,
                        value: Arc::new(value),
                    },
                );
            }
        }
        // Older projects: read into routes, alignments and alignment
        // points once (the objects rewritten in place; not a logged edit).
        let done = |key: &str| -> Result<bool, SessionError> {
            Ok(conn
                .query_row(
                    "SELECT value FROM ols_meta WHERE key = ?1",
                    params![key],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .is_some())
        };
        let before_routes = !done("routes_alignments")?;
        if before_routes || !done("alignment_points")? {
            let tx = conn.unchecked_transaction()?;
            for (name, objects) in &mut by_line {
                let Some(new) = upgraded_objects(objects, before_routes)? else {
                    continue;
                };
                // Alignments and their points: in their order.
                let mut nth: BTreeMap<Kind, i64> = BTreeMap::new();
                let keys: BTreeSet<(Kind, String)> =
                    new.iter().map(|(k, id, _)| (*k, id.clone())).collect();
                for (k, (kind, id, value)) in new.into_iter().enumerate() {
                    let key = (kind, id.clone());
                    let n = nth.entry(kind).or_default();
                    let place = *n;
                    *n += 1;
                    let reorder = matches!(kind, Kind::Alignment | Kind::AlignmentPoint);
                    let (ord, version) = match objects.get(&key) {
                        Some(o) if *o.value == value && (!reorder || o.ord == place) => continue,
                        Some(o) => (if reorder { place } else { o.ord }, o.version + 1),
                        None => (if reorder { place } else { k as i64 }, 1),
                    };
                    tx.execute(
                        "INSERT OR REPLACE INTO model_object (line, kind, id, ord, version, json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![name, kind.as_str(), id, ord, version, value.to_string()],
                    )?;
                    objects.insert(
                        key,
                        Object {
                            ord,
                            version,
                            value: Arc::new(value),
                        },
                    );
                }
                // Objects the upgrade left out (not the line's reports and
                // library files: kept with it, not part of the line).
                let gone: Vec<(Kind, String)> = objects
                    .keys()
                    .filter(|k| {
                        !keys.contains(*k)
                            && !matches!(k.0, Kind::Report | Kind::Library | Kind::Annotation)
                    })
                    .cloned()
                    .collect();
                for (kind, id) in gone {
                    tx.execute(
                        "DELETE FROM model_object WHERE line = ?1 AND kind = ?2 AND id = ?3",
                        params![name, kind.as_str(), id],
                    )?;
                    objects.remove(&(kind, id));
                }
            }
            tx.execute_batch(
                "INSERT OR REPLACE INTO ols_meta (key, value) VALUES ('routes_alignments', '1');
                 INSERT OR REPLACE INTO ols_meta (key, value) VALUES ('alignment_points', '1');",
            )?;
            tx.commit()?;
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
        session.load_lines(by_line, gone, placed)?;
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

    /// The library files kept with the current project line of one kind
    /// (e.g. `cables`, `structures`, `insulators`, `weather`): file name and
    /// text, by name.
    pub fn library_files(&self, kind: &str) -> Result<Vec<(String, String)>, SessionError> {
        let mut out: Vec<(String, String)> = self
            .objects
            .iter()
            .filter(|((k, _), _)| *k == Kind::Library)
            .filter_map(|(_, o)| library_entry(&o.value))
            .filter(|(k, _, _)| k == kind)
            .map(|(_, name, text)| (name, text))
            .collect();
        out.sort();
        Ok(out)
    }

    /// Keeps library file `name` of `kind` with the current project line
    /// (replacing one of that name): a logged command, undone as any.
    pub fn put_library_file(
        &mut self,
        user: &str,
        kind: &str,
        name: &str,
        text: &str,
    ) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        self.check_library_edit(user)?;
        if kind.is_empty() || kind.contains('/') || !safe_file_name(name) {
            return Err(refused(format!(
                "library file \"{name}\" of \"{kind}\": bad name"
            )));
        }
        let id = library_id(kind, name);
        let value = library_value(kind, name, text);
        if self
            .objects
            .get(&(Kind::Library, id.clone()))
            .is_some_and(|o| *o.value == value)
        {
            return Ok(());
        }
        self.apply(
            user,
            Command::Put {
                kind: Kind::Library,
                id,
                value,
                ord: None,
            },
        )?;
        Ok(())
    }

    /// The puts of library files changed by `user` (kind, name, text),
    /// for a command of several changes (one log entry, undone as one):
    /// checked as [`Session::put_library_file`] checks one (the role, the
    /// "library" edit grant, the names); files unchanged left out.
    pub fn library_puts(
        &self,
        user: &str,
        changes: &[(String, String, String)],
    ) -> Result<Vec<Command>, SessionError> {
        self.require(user, Role::Designer)?;
        self.check_library_edit(user)?;
        let mut out = Vec::new();
        for (kind, name, text) in changes {
            if kind.is_empty() || kind.contains('/') || !safe_file_name(name) {
                return Err(refused(format!(
                    "library file \"{name}\" of \"{kind}\": bad name"
                )));
            }
            let id = library_id(kind, name);
            let value = library_value(kind, name, text);
            if self
                .objects
                .get(&(Kind::Library, id.clone()))
                .is_none_or(|o| *o.value != value)
            {
                out.push(Command::Put {
                    kind: Kind::Library,
                    id,
                    value,
                    ord: None,
                });
            }
        }
        Ok(out)
    }

    /// Keeps library files with the current project line as one log entry
    /// of kind `library` — what a program does on its own (files next to
    /// the project copied in when it opens, an older form converted):
    /// in the history, not undone by the user's Undo. Each change: kind,
    /// name and text (`None`: removed).
    pub fn keep_library_files(
        &mut self,
        user: &str,
        changes: &[(String, String, Option<String>)],
    ) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        self.check_library_edit(user)?;
        let mut commands = Vec::new();
        for (kind, name, text) in changes {
            if kind.is_empty() || kind.contains('/') || !safe_file_name(name) {
                return Err(refused(format!(
                    "library file \"{name}\" of \"{kind}\": bad name"
                )));
            }
            let id = library_id(kind, name);
            let current = self.objects.get(&(Kind::Library, id.clone()));
            match text {
                Some(t) => {
                    let value = library_value(kind, name, t);
                    if current.is_none_or(|o| *o.value != value) {
                        commands.push(Command::Put {
                            kind: Kind::Library,
                            id,
                            value,
                            ord: None,
                        });
                    }
                }
                None if current.is_some() => commands.push(Command::Delete {
                    kind: Kind::Library,
                    id,
                    confirm: false,
                }),
                None => {}
            }
        }
        if commands.is_empty() {
            return Ok(());
        }
        self.apply_as(
            user,
            Command::Batch { commands },
            "library",
            None,
            false,
            true,
        )?;
        Ok(())
    }

    /// Removes library file `name` of `kind` from the current project
    /// line (a logged command).
    pub fn delete_library_file(
        &mut self,
        user: &str,
        kind: &str,
        name: &str,
    ) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        self.check_library_edit(user)?;
        let id = library_id(kind, name);
        if !self.objects.contains_key(&(Kind::Library, id.clone())) {
            return Ok(());
        }
        self.apply(
            user,
            Command::Delete {
                kind: Kind::Library,
                id,
                confirm: false,
            },
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

    /// Alignment `id` (`"line"`, the line's own alignment of older
    /// commands: `main`).
    fn alignment_of(&self, id: &str) -> Result<&ohl_line::Alignment, SessionError> {
        let id = if id == "line" { "main" } else { id };
        self.line
            .alignment_by_id(id)
            .ok_or_else(|| refused(format!("no alignment {id}")))
    }

    /// Point `index` of alignment `id`.
    fn point_of(&self, id: &str, index: usize) -> Result<ohl_line::AlignmentPoint, SessionError> {
        let a = self.alignment_of(id)?;
        let p = a.points.get(index).ok_or_else(|| {
            refused(format!(
                "alignment {} has no point {index} ({} points)",
                a.id,
                a.points.len()
            ))
        })?;
        self.line
            .alignment_point(p)
            .cloned()
            .ok_or_else(|| refused(format!("no alignment point {p}")))
    }

    /// The command setting the line's creep-stretch criteria (Criteria ›
    /// Creep-Stretch): one logged, undoable put of the line's settings.
    pub fn creep_stretch_command(
        &self,
        creep_stretch: ohl_line::condition::CreepStretch,
    ) -> Command {
        let mut line = self.line.clone();
        line.creep_stretch = creep_stretch;
        line_settings_change(&self.line, &line)
    }

    /// The command setting the line's bimetallic conductor model
    /// (Criteria › Bimetallic conductor model): one logged, undoable put of
    /// the line's settings.
    pub fn bimetallic_command(&self, bimetallic: ohl_cables::BimetallicModel) -> Command {
        let mut line = self.line.clone();
        line.bimetallic = bimetallic;
        line_settings_change(&self.line, &line)
    }

    /// The command setting the line's calculation options (Criteria ›
    /// Calculation options): one logged, undoable put of the line's
    /// settings.
    pub fn solver_command(&self, solver: ohl_line::SolverOptions) -> Command {
        let mut line = self.line.clone();
        line.solver = solver;
        line_settings_change(&self.line, &line)
    }

    /// The command setting what the line sends to the placed structures
    /// (Criteria › Loads for structures: load cases, weather cases on
    /// their own): one logged, undoable put of its settings row
    /// (`structure_loads`); no section is solved again.
    pub fn structure_loads_command(&self, choice: ohl_line::StructureLoads) -> Command {
        let mut line = self.line.clone();
        line.structure_loads = choice;
        line_settings_change(&self.line, &line)
    }

    /// The alignment points tied to structure `id`.
    fn points_tied_to(&self, id: &str) -> Vec<ohl_line::AlignmentPoint> {
        self.line
            .alignment_points
            .iter()
            .filter(|p| p.structure.as_deref() == Some(id))
            .cloned()
            .collect()
    }

    /// Deleting alignment point `id`: the alignments it is on without it
    /// (their other points in order), then the point.
    fn delete_point(&self, id: &str, out: &mut Vec<Command>) {
        for a in &self.line.alignments {
            if a.points.iter().any(|p| p == id) {
                let mut a = a.clone();
                a.points.retain(|p| p != id);
                out.push(put(Kind::Alignment, &a.id, &a));
            }
        }
        out.push(Command::Delete {
            kind: Kind::AlignmentPoint,
            id: id.to_owned(),
            confirm: false,
        });
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
        match command {
            Command::Put {
                kind: Kind::PointCloud,
                ..
            } => {
                return Err(refused(
                    "a point cloud is not an object: its points change by change_points",
                ));
            }
            // A structure put by a user: its linked attachment points moved
            // stay on their joints, their offsets following the move (owner
            // decision 2026-10-21; [`ohl_line::links::follow_moves`]).
            Command::Put {
                kind: Kind::Structure,
                ref id,
                ref value,
                ord,
            } if user => {
                let moved = self.line.structure(id).and_then(|old| {
                    let mut new: ohl_line::Structure =
                        serde_json::from_value(value.clone()).ok()?;
                    ohl_line::links::follow_moves(old, &mut new).then_some(new)
                });
                match moved {
                    Some(new) => out.push(Command::Put {
                        kind: Kind::Structure,
                        id: id.clone(),
                        value: json(&new),
                        ord,
                    }),
                    None => out.push(command),
                }
            }
            Command::Put { .. } => out.push(command),
            Command::ChangePoints {
                ref cloud,
                ref points,
            } => {
                points::check(cloud, points)?;
                out.push(command);
            }
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
                for s in &impact.spacers {
                    out.push(del(Kind::Spacer, s));
                }
                for sec in &impact.sections {
                    out.push(del(Kind::Section, sec));
                }
                // An alignment point: taken off its alignments first.
                if kind == Kind::AlignmentPoint {
                    for a in &self.line.alignments {
                        if a.points.contains(&id) {
                            let mut a = a.clone();
                            a.points.retain(|p| *p != id);
                            out.push(put(Kind::Alignment, &a.id, &a));
                        }
                    }
                }
                for a in &impact.alignments_deleted {
                    out.push(del(Kind::Alignment, a));
                }
                // A route deleted: taken out of the structures that gave it.
                if kind == Kind::Route {
                    for s in &self.line.structures {
                        if impact.structures_changed.contains(&s.id) {
                            let mut s = s.clone();
                            s.routes.retain(|r| *r != id);
                            out.push(put(Kind::Structure, &s.id, &s));
                        }
                    }
                }
                // A structure deleted: the points tied to it detached.
                if kind == Kind::Structure {
                    for mut p in self.points_tied_to(&id) {
                        p.structure = None;
                        out.push(put(Kind::AlignmentPoint, &p.id.clone(), &p));
                    }
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
                let set = phase.map_or(set, |p| ohl_line::assembly::set_from_v1(set, p));
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
                for s in &impact.spacers {
                    out.push(del(Kind::Spacer, s));
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
            Command::LinkPoint {
                structure,
                label,
                joint,
                offset,
                position,
            } => {
                let old = self.own_point(&structure, &label)?;
                let point = ohl_line::StructurePoint {
                    description: old.map(|p| p.description).unwrap_or_default(),
                    label: label.clone(),
                    position,
                    joint: Some(joint),
                    offset,
                    unlinked: false,
                };
                out.push(self.with_points(&structure, vec![point])?);
                notes.push(format!("link attachment point {label} of {structure}"));
            }
            Command::UnlinkPoint {
                structure,
                label,
                position,
            } => {
                let old = self.own_point(&structure, &label)?;
                let point = ohl_line::StructurePoint {
                    description: old.map(|p| p.description).unwrap_or_default(),
                    label: label.clone(),
                    position,
                    joint: None,
                    offset: [0.0; 3],
                    unlinked: true,
                };
                out.push(self.with_points(&structure, vec![point])?);
                notes.push(format!("unlink attachment point {label} of {structure}"));
            }
            Command::RelinkPoints { structure, points } => {
                let labels: Vec<String> = points.iter().map(|p| p.label.clone()).collect();
                out.push(self.with_points(&structure, points)?);
                notes.push(format!(
                    "relink attachment points of {structure}: {}",
                    labels.join(", ")
                ));
            }
            Command::ReplaceStructure {
                id,
                structure,
                wires,
                sets,
                points,
                confirm,
            } => {
                let plan = self.replacement(&id, &structure, &wires, &sets, &points)?;
                if user && plan.impact.has_dependents() && !confirm {
                    return Err(SessionError::NeedsConfirmation(Box::new(plan.impact)));
                }
                self.replace_primitives(plan, out, notes);
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
                let mut value = (*old.value).clone();
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
                            let clamp_on = |c: &ohl_line::JumperClamp| {
                                c.structure.as_deref() == Some(id.as_str())
                            };
                            if j.points().any(|p| p.structure == id)
                                || j.supports().iter().any(clamp_on)
                            {
                                let mut j = j.clone();
                                for p in [&mut j.from, &mut j.to]
                                    .into_iter()
                                    .filter_map(ohl_line::JumperEnd::point_mut)
                                {
                                    if p.structure == id {
                                        p.structure = new_id.clone();
                                    }
                                }
                                j.edit_supports(|cs| {
                                    for c in cs.iter_mut().filter(|c| clamp_on(c)) {
                                        c.structure = Some(new_id.clone());
                                    }
                                });
                                out.push(Command::Put {
                                    kind: Kind::Jumper,
                                    id: j.id.clone(),
                                    value: json(&j),
                                    ord: None,
                                });
                            }
                        }
                        for mut p in self.points_tied_to(&id) {
                            p.structure = Some(new_id.clone());
                            out.push(put(Kind::AlignmentPoint, &p.id.clone(), &p));
                        }
                        // The circuit starts on it.
                        out.extend(self.starts_renamed(|p| {
                            (p.structure == id)
                                .then(|| ohl_line::SectionPoint::new(new_id.clone(), p.set))
                        }));
                    }
                    // A section renamed: its spacers follow.
                    Kind::Section => {
                        for s in &self.line.spacers {
                            if s.a.section == id || s.b.section == id {
                                let mut s = s.clone();
                                for end in [&mut s.a, &mut s.b] {
                                    if end.section == id {
                                        end.section = new_id.clone();
                                    }
                                }
                                out.push(put(Kind::Spacer, &s.id.clone(), &s));
                            }
                        }
                        // And the jumpers hanging on taps on it.
                        for j in &self.line.jumpers {
                            let mut j = j.clone();
                            let mut changed = false;
                            for t in [&mut j.from, &mut j.to]
                                .into_iter()
                                .filter_map(ohl_line::JumperEnd::tap_mut)
                            {
                                if let ohl_line::Tap::Span(r) = t
                                    && r.section == id
                                {
                                    r.section = new_id.clone();
                                    changed = true;
                                }
                            }
                            if changed {
                                out.push(put(Kind::Jumper, &j.id.clone(), &j));
                            }
                        }
                    }
                    // A route renamed: its alignments and the structures
                    // giving it follow.
                    Kind::Route => {
                        for a in &self.line.alignments {
                            if a.route == id {
                                let mut a = a.clone();
                                a.route = new_id.clone();
                                out.push(put(Kind::Alignment, &a.id, &a));
                            }
                        }
                        for st in &self.line.structures {
                            if st.routes.contains(&id) {
                                let mut st = st.clone();
                                for r in st.routes.iter_mut().filter(|r| **r == id) {
                                    *r = new_id.clone();
                                }
                                out.push(put(Kind::Structure, &st.id, &st));
                            }
                        }
                    }
                    Kind::Alignment => {}
                    // A point renamed: its alignments follow.
                    Kind::AlignmentPoint => {
                        for a in &self.line.alignments {
                            if a.points.contains(&id) {
                                let mut a = a.clone();
                                for p in a.points.iter_mut().filter(|p| **p == id) {
                                    *p = new_id.clone();
                                }
                                out.push(put(Kind::Alignment, &a.id, &a));
                            }
                        }
                    }
                    // A jumper renamed: the jumpers hanging on taps on its
                    // points follow.
                    Kind::Jumper => {
                        for j in &self.line.jumpers {
                            let mut j = j.clone();
                            let mut changed = false;
                            for t in [&mut j.from, &mut j.to]
                                .into_iter()
                                .filter_map(ohl_line::JumperEnd::tap_mut)
                            {
                                if let ohl_line::Tap::Jumper { jumper, .. } = t
                                    && *jumper == id
                                {
                                    *jumper = new_id.clone();
                                    changed = true;
                                }
                            }
                            if changed {
                                out.push(put(Kind::Jumper, &j.id.clone(), &j));
                            }
                        }
                    }
                    Kind::Spacer => {}
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
                let set = phase.map_or(set, |p| ohl_line::assembly::set_from_v1(set, p));
                let new_set =
                    new_phase.map_or(new_set, |p| ohl_line::assembly::set_from_v1(new_set, p));
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
                let old = ohl_line::SectionPoint::new(structure.clone(), set);
                let new = ohl_line::SectionPoint::new(structure.clone(), new_set);
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
                    if j.is_at(&old) {
                        let mut j = j.clone();
                        for p in [&mut j.from, &mut j.to]
                            .into_iter()
                            .filter_map(ohl_line::JumperEnd::point_mut)
                        {
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
                out.extend(self.starts_renamed(|p| (*p == old).then(|| new.clone())));
                notes.push(format!(
                    "renumber wire point {structure} set {set} to {new_set}"
                ));
            }
            Command::RenamePoint {
                kind,
                id,
                label,
                new_label,
            } => {
                if !matches!(kind, Kind::Section | Kind::Jumper) {
                    return Err(refused(format!(
                        "a {} has no points to relabel",
                        kind.as_str()
                    )));
                }
                let mut line = self.line.clone();
                line.relabel_point(kind == Kind::Jumper, &id, &label, &new_label)
                    .map_err(refused)?;
                // The objects that changed.
                for (a, b) in self.line.sections.iter().zip(&line.sections) {
                    if a != b {
                        out.push(put(Kind::Section, &b.id, b));
                    }
                }
                for (a, b) in self.line.jumpers.iter().zip(&line.jumpers) {
                    if a != b {
                        out.push(put(Kind::Jumper, &b.id, b));
                    }
                }
                for (a, b) in self.line.spacers.iter().zip(&line.spacers) {
                    if a != b {
                        out.push(put(Kind::Spacer, &b.id, b));
                    }
                }
                notes.push(format!(
                    "relabel point {label} of {} {id} to {}",
                    kind.as_str(),
                    new_label.trim()
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
                let mut line = self.line.clone();
                line.systems = systems;
                // The circuits of the system follow.
                for c in &mut line.circuits {
                    if c.system.as_deref() == Some(&id) {
                        c.system = Some(new_id.clone());
                    }
                }
                self.primitives(line_settings_change(&self.line, &line), out, notes, user)?;
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
                    // The reports stay: they are not part of the line.
                    if !matches!(k, Kind::Report | Kind::Library | Kind::Annotation)
                        && !keep.contains(&(*k, id.clone()))
                    {
                        out.push(Command::Delete {
                            kind: *k,
                            id: id.clone(),
                            confirm: false,
                        });
                    }
                }
                // In the replacing line's order: each object's place among
                // its kind.
                let mut nth: BTreeMap<Kind, i64> = BTreeMap::new();
                for (kind, id, value) in new {
                    let n = nth.entry(kind).or_default();
                    let place = *n;
                    *n += 1;
                    if self
                        .objects
                        .get(&(kind, id.clone()))
                        .is_some_and(|o| *o.value == value && o.ord == place)
                    {
                        continue;
                    }
                    out.push(Command::Put {
                        kind,
                        id,
                        value,
                        ord: Some(place),
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
                for mut p in self.points_tied_to(&id) {
                    p.position = position;
                    out.push(put(Kind::AlignmentPoint, &p.id.clone(), &p));
                }
            }
            Command::SetWindFactor {
                section,
                spans: [first, last],
                factor,
            } => {
                let mut s = self
                    .line
                    .sections
                    .iter()
                    .find(|s| s.id == section)
                    .ok_or_else(|| refused(format!("no section {section}")))?
                    .clone();
                let n = s.points.len().saturating_sub(1);
                if first > last || last >= n {
                    return Err(refused(format!(
                        "section {section} has spans 1 to {n}: not {} to {}",
                        first + 1,
                        last + 1
                    )));
                }
                if !(factor.is_finite() && (0.0..=ohl_line::MAX_WIND_FACTOR).contains(&factor)) {
                    return Err(refused(format!(
                        "local wind factor {factor}: 0 to {}",
                        ohl_line::MAX_WIND_FACTOR
                    )));
                }
                s.set_wind_factor(first, last, factor);
                out.push(Command::Put {
                    kind: Kind::Section,
                    id: section,
                    value: json(&s),
                    ord: None,
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
                    ord: None,
                });
            }
            Command::MoveAlignmentPoint {
                route,
                index,
                position,
            } => {
                let mut p = self.point_of(&route, index)?;
                if let Some(s) = p.structure.take() {
                    notes.push(format!(
                        "alignment point {} detached from {s} and moved",
                        p.id
                    ));
                }
                p.position = position;
                out.push(put(Kind::AlignmentPoint, &p.id.clone(), &p));
            }
            Command::InsertAlignmentPoint {
                route,
                index,
                position,
                point,
            } => {
                let mut a = self.alignment_of(&route)?.clone();
                if index > a.points.len() {
                    return Err(refused(format!(
                        "alignment {} has {} points: cannot insert before {index}",
                        a.id,
                        a.points.len()
                    )));
                }
                let id = match point {
                    Some(id) => {
                        if self.line.alignment_point(&id).is_none() {
                            return Err(refused(format!("no alignment point {id}")));
                        }
                        if a.points.contains(&id) {
                            return Err(refused(format!(
                                "alignment {}: point {id} is on it already",
                                a.id
                            )));
                        }
                        id
                    }
                    None => {
                        let id = self.line.new_point_id();
                        let p = ohl_line::AlignmentPoint::free(&id, position);
                        out.push(put(Kind::AlignmentPoint, &id, &p));
                        id
                    }
                };
                a.points.insert(index, id);
                out.push(put(Kind::Alignment, &a.id.clone(), &a));
            }
            Command::RemoveAlignmentPoint { route, index } => {
                let p = self.point_of(&route, index)?;
                self.delete_point(&p.id, out);
            }
            Command::TieAlignmentPoint {
                route,
                index,
                structure,
            } => {
                let mut p = self.point_of(&route, index)?;
                let s = self
                    .line
                    .structure(&structure)
                    .ok_or_else(|| refused(format!("no structure {structure}")))?;
                // Its description stays.
                p.position = s.position;
                p.structure = Some(structure);
                out.push(put(Kind::AlignmentPoint, &p.id.clone(), &p));
            }
            Command::DetachAlignmentPoint { route, index } => {
                let mut p = self.point_of(&route, index)?;
                p.structure = None;
                out.push(put(Kind::AlignmentPoint, &p.id.clone(), &p));
            }
            Command::AlignUnderStructures { route } => {
                let mut a = self.alignment_of(&route)?.clone();
                let r = self
                    .line
                    .alignment_views()
                    .into_iter()
                    .find(|r| r.id == a.id)
                    .ok_or_else(|| refused(format!("no alignment {route}")))?;
                let points = self.line.alignment_under_structures(&r);
                if points.is_empty() {
                    return Err(refused(format!("alignment {}: no structure on it", a.id)));
                }
                for p in &points {
                    if self.line.alignment_point(&p.id) != Some(p) {
                        out.push(put(Kind::AlignmentPoint, &p.id, p));
                    }
                }
                let old =
                    std::mem::replace(&mut a.points, points.iter().map(|p| p.id.clone()).collect());
                out.push(put(Kind::Alignment, &a.id.clone(), &a));
                // Its old points on no other alignment.
                for p in old {
                    let elsewhere = self
                        .line
                        .alignments
                        .iter()
                        .any(|b| b.id != a.id && b.points.contains(&p));
                    if !elsewhere && !a.points.contains(&p) {
                        out.push(Command::Delete {
                            kind: Kind::AlignmentPoint,
                            id: p,
                            confirm: false,
                        });
                    }
                }
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
            Kind::PointCloud => {
                return Err(refused(
                    "a point cloud is not deleted by a command (Terrain › point clouds)",
                ));
            }
            Kind::Structure => return self.structure_deletion(id),
            Kind::Section => {
                let sec = self
                    .line
                    .sections
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| refused(format!("no section {id}")))?;
                let at_ends: Vec<String> = self
                    .line
                    .jumpers
                    .iter()
                    .filter(|j| j.points().any(|p| sec.points.contains(p)))
                    .map(|j| j.id.clone())
                    .collect();
                // With the jumpers hanging on taps on it (and on those).
                impact.jumpers = self.jumpers_hanging(&[id.to_owned()], &at_ends);
            }
            Kind::Route => {
                if !self.line.routes.iter().any(|r| r.id == id) {
                    return Err(refused(format!("no route {id}")));
                }
                let gone: Vec<&ohl_line::Alignment> = self
                    .line
                    .alignments
                    .iter()
                    .filter(|a| a.route == id)
                    .collect();
                impact.alignments_deleted = gone.iter().map(|a| a.id.clone()).collect();
                impact.structures_changed = self
                    .line
                    .structures
                    .iter()
                    .filter(|s| s.routes.iter().any(|r| r == id))
                    .map(|s| s.id.clone())
                    .collect();
            }
            Kind::Alignment => {
                // Its points stay (on no alignment, they wait for another).
                if self.line.alignment_by_id(id).is_none() {
                    return Err(refused(format!("no alignment {id}")));
                }
            }
            Kind::AlignmentPoint => {
                if self.line.alignment_point(id).is_none() {
                    return Err(refused(format!("no alignment point {id}")));
                }
                let on = self.line.point_alignments(id);
                if on.len() > 1 {
                    impact.alignments_changed = on;
                }
            }
            Kind::Jumper => {
                if !self.line.jumpers.iter().any(|j| j.id == id) {
                    return Err(refused(format!("no jumper {id}")));
                }
                // The jumpers hanging on taps on its points (and on those).
                impact.jumpers = self
                    .jumpers_hanging(&[], &[id.to_owned()])
                    .into_iter()
                    .filter(|j| j != id)
                    .collect();
            }
            Kind::Report | Kind::Spacer | Kind::Library | Kind::Annotation => {
                if !self.objects.contains_key(&(kind, id.to_owned())) {
                    return Err(refused(format!("no {} {id}", kind.as_str())));
                }
            }
        }
        if kind == Kind::Section {
            impact.spacers = self.spacers_on(&[id.to_owned()]);
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
        let point = ohl_line::SectionPoint::new(id, set);
        let sections: Vec<&ohl_line::Section> = self
            .line
            .sections
            .iter()
            .filter(|sec| sec.points.contains(&point))
            .collect();
        impact.sections = sections.iter().map(|sec| sec.id.clone()).collect();
        let at_ends: Vec<String> = self
            .line
            .jumpers
            .iter()
            .filter(|j| {
                j.points()
                    .any(|end| *end == point || sections.iter().any(|sec| sec.points.contains(end)))
            })
            .map(|j| j.id.clone())
            .collect();
        // With the jumpers hanging on taps on those sections (and on those).
        impact.jumpers = self.jumpers_hanging(&impact.sections, &at_ends);
        impact.spacers = self.spacers_on(&impact.sections);
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
        let sections: Vec<&ohl_line::Section> = self
            .line
            .sections
            .iter()
            .filter(|sec| sec.points.iter().any(|p| p.structure == id))
            .collect();
        impact.sections = sections.iter().map(|sec| sec.id.clone()).collect();
        // Jumpers at its attachments, and at points of the deleted sections.
        let at_ends: Vec<String> = self
            .line
            .jumpers
            .iter()
            .filter(|j| {
                j.points().any(|end| {
                    end.structure == id || sections.iter().any(|sec| sec.points.contains(end))
                }) || j.supports().iter().any(|c| c.structure_id(j) == id)
            })
            .map(|j| j.id.clone())
            .collect();
        // With the jumpers hanging on taps on those sections (and on those).
        impact.jumpers = self.jumpers_hanging(&impact.sections, &at_ends);
        impact.alignment_detached = self.points_tied_to(id).into_iter().map(|p| p.id).collect();
        impact.spacers = self.spacers_on(&impact.sections);
        Ok(impact)
    }

    /// `jumpers` and the jumpers hanging on taps on span points of
    /// `sections` or on their points, and so on — those hanging first
    /// (deleted before what they hang on).
    fn jumpers_hanging(&self, sections: &[String], jumpers: &[String]) -> Vec<String> {
        let mut out: Vec<String> = jumpers.to_vec();
        let mut k = 0;
        let hangs_on = |j: &ohl_line::Jumper, ids: &[String]| {
            j.taps().any(|t| match t {
                ohl_line::Tap::Span(r) => sections.contains(&r.section),
                ohl_line::Tap::Jumper { jumper, .. } => ids.contains(jumper),
            })
        };
        loop {
            let more: Vec<String> = self
                .line
                .jumpers
                .iter()
                .filter(|j| !out.contains(&j.id) && hangs_on(j, &out))
                .map(|j| j.id.clone())
                .collect();
            if more.is_empty() || k > self.line.jumpers.len() {
                break;
            }
            out.extend(more);
            k += 1;
        }
        // Those hanging first.
        out.reverse();
        out
    }

    /// The spacers on any of `sections`.
    fn spacers_on(&self, sections: &[String]) -> Vec<String> {
        self.line
            .spacers
            .iter()
            .filter(|s| sections.contains(&s.a.section) || sections.contains(&s.b.section))
            .map(|s| s.id.clone())
            .collect()
    }

    /// The circuit starts no longer a section's point after `prims` (a
    /// structure, a set or a section deleted, a section's points
    /// changed) taken out of the line's settings: out of the settings
    /// `prims` put, else by a put added.
    fn starts_follow(&self, prims: &mut Vec<Command>) {
        let mut sections: BTreeMap<String, Vec<ohl_line::SectionPoint>> = self
            .line
            .sections
            .iter()
            .map(|s| (s.id.clone(), s.points.clone()))
            .collect();
        let mut meta_at = None;
        for (k, p) in prims.iter().enumerate() {
            match p {
                Command::Put {
                    kind: Kind::Section,
                    id,
                    value,
                    ..
                } => {
                    if let Ok(s) = serde_json::from_value::<Section>(value.clone()) {
                        sections.insert(id.clone(), s.points);
                    }
                }
                Command::Delete {
                    kind: Kind::Section,
                    id,
                    ..
                } => {
                    sections.remove(id);
                }
                Command::Put {
                    kind: Kind::Meta,
                    id,
                    ..
                } if id == "circuit_starts" => meta_at = Some(k),
                _ => {}
            }
        }
        let on = |p: &ohl_line::SectionPoint| sections.values().any(|pts| pts.contains(p));
        let starts_of = |v: &Value| -> Vec<ohl_line::SectionPoint> {
            v.get("circuit_starts")
                .and_then(|x| serde_json::from_value(x.clone()).ok())
                .unwrap_or_default()
        };
        let mut starts = match meta_at {
            Some(k) => match &prims[k] {
                Command::Put { value, .. } => starts_of(value),
                _ => unreachable!("a put"),
            },
            None => self.line.circuit_starts.clone(),
        };
        let n = starts.len();
        starts.retain(on);
        if starts.len() == n {
            return;
        }
        let put = starts_put(&starts);
        match meta_at {
            Some(k) => prims[k] = put,
            None => prims.push(put),
        }
    }

    /// The put of the line's settings with the circuit starts renamed by
    /// `rename` (none when no start changes).
    fn starts_renamed(
        &self,
        rename: impl Fn(&ohl_line::SectionPoint) -> Option<ohl_line::SectionPoint>,
    ) -> Option<Command> {
        let mut meta = Meta::of(&self.line);
        let mut changed = false;
        for p in &mut meta.circuit_starts {
            if let Some(new) = rename(p) {
                *p = new;
                changed = true;
            }
        }
        changed.then(|| starts_put(&meta.circuit_starts))
    }

    /// The command setting the project's default values (Criteria ›
    /// Default values): one logged, undoable put of the line's settings.
    pub fn defaults_command(&self, defaults: ohl_line::Defaults) -> Command {
        let mut line = self.line.clone();
        line.defaults = defaults;
        line_settings_change(&self.line, &line)
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
        if cascade {
            self.starts_follow(&mut prims);
        }
        // Settings rows unchanged are left out: a setting changed touches
        // its own row only ([`META_ROWS`]).
        let same = |c: &Command| {
            matches!(c, Command::Put { kind: Kind::Meta, id, value, .. }
                if self.objects.get(&(Kind::Meta, id.clone())).is_some_and(|o| *o.value == *value))
        };
        if prims.iter().any(|c| !same(c)) {
            prims.retain(|c| !same(c));
        }
        let note = (!notes.is_empty()).then(|| notes.join("; "));
        if prims.is_empty() {
            return Err(refused("empty command"));
        }
        // Points of the project's clouds: applied in the transaction below.
        let (point_prims, mut prims): (Vec<Command>, Vec<Command>) = prims
            .into_iter()
            .partition(|c| matches!(c, Command::ChangePoints { .. }));
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
                                value: (*o.value).clone(),
                                ord: moved.map(|_| o.ord),
                            });
                            o.value = Arc::new(value.clone());
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
                                    value: Arc::new(value.clone()),
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
                        value: Arc::unwrap_or_clone(old.value),
                        ord: Some(old.ord),
                    });
                    changed.push((*kind, id.clone()));
                }
                _ => unreachable!("expanded above"),
            }
        }
        inverse.reverse();
        // The line is checked when its objects change (a change of points
        // only leaves it as it is; a project may have none yet).
        let objects_changed = !changed.is_empty();
        for p in &point_prims {
            if let Command::ChangePoints { cloud, .. } = p {
                changed.push((Kind::PointCloud, cloud.clone()));
            }
        }
        for (kind, id) in &changed {
            if *kind == Kind::Report
                && let Some(o) = objects.get(&(*kind, id.clone()))
            {
                reports::check_value(id, &o.value)?;
            }
            if *kind == Kind::Annotation
                && let Some(o) = objects.get(&(*kind, id.clone()))
            {
                annotations::check_value(id, &o.value)?;
            }
        }
        let line = if objects_changed {
            line_of(&objects)?
        } else {
            self.line.clone()
        };
        // Annotations attached to what the command deleted go with it, in
        // the same entry (owner, 2026-10-25): one undo brings both back.
        if cascade && objects_changed {
            let gone = annotations::orphaned(
                objects
                    .iter()
                    .filter(|((k, _), _)| *k == Kind::Annotation)
                    .map(|((_, id), o)| (id, &*o.value)),
                &line,
            );
            for id in gone {
                let key = (Kind::Annotation, id.clone());
                if let Some(old) = objects.remove(&key) {
                    inverse.insert(
                        0,
                        Command::Put {
                            kind: Kind::Annotation,
                            id: id.clone(),
                            value: Arc::unwrap_or_clone(old.value),
                            ord: Some(old.ord),
                        },
                    );
                    prims.push(Command::Delete {
                        kind: Kind::Annotation,
                        id: id.clone(),
                        confirm: false,
                    });
                    changed.push(key);
                }
            }
        }
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
        // Edit rights limited to kinds, objects or an area (admins: all);
        // changed points: where they were and where they go.
        if !import {
            let objects_only: Vec<(Kind, String)> = changed
                .iter()
                .filter(|(k, _)| *k != Kind::PointCloud)
                .cloned()
                .collect();
            self.check_edit(
                user,
                &objects_only,
                &self.spatial,
                &spatial,
                Some((&self.line, &line)),
            )?;
            for p in &point_prims {
                if let Command::ChangePoints { cloud, points } = p {
                    let key = (Kind::PointCloud, cloud.clone());
                    let area: BTreeMap<(Kind, String), spatial::Entry> =
                        points::entry(&self.conn, &self.grid, cloud, points)
                            .map(|e| (key.clone(), e))
                            .into_iter()
                            .collect();
                    self.check_edit(user, &[key], &area, &area, None)?;
                }
            }
        }

        // Persist atomically: points, objects and log entry.
        let tx = self.conn.transaction()?;
        let mut point_inverses = Vec::new();
        let mut points_changed = Vec::new();
        for p in point_prims {
            if let Command::ChangePoints { cloud, points } = p {
                let (applied, back) = points::apply(&tx, &cloud, &points)?;
                points_changed.push((cloud.clone(), applied.clone()));
                prims.push(Command::ChangePoints {
                    cloud: cloud.clone(),
                    points: applied,
                });
                point_inverses.push(Command::ChangePoints {
                    cloud,
                    points: back,
                });
            }
        }
        // Undone newest first, before the objects (they do not depend on
        // each other).
        point_inverses.reverse();
        point_inverses.extend(inverse);
        let inverse = point_inverses;
        // A line kept as differences from a revision keeps a tombstone
        // for an object deleted ([`lines`]).
        let based = lines::line_base(&tx, &self.current)?.is_some();
        for (kind, id) in &changed {
            if *kind == Kind::PointCloud {
                continue;
            }
            match objects.get(&(*kind, id.clone())) {
                Some(o) => lines::write_row(&tx, &self.current, *kind, id, Some(o))?,
                None if based => lines::write_row(&tx, &self.current, *kind, id, None)?,
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
            points: points_changed,
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
                    // The points it changed must be as it left them.
                    Command::ChangePoints { cloud, points } => {
                        let there = points::still_there(&self.conn, &cloud, &points)?;
                        if !there {
                            return Err(refused(format!(
                                "points of point cloud {cloud} were changed after entry {seq}: \
                                 undo those changes first or ask an admin to roll back"
                            )));
                        }
                    }
                    _ => {}
                }
            }
        }
        for ((k, id), value) in &left {
            let now = self.objects.get(&(*k, id.clone())).map(|o| &*o.value);
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
            .map(|(k, o)| (k.clone(), ((*o.value).clone(), Some(o.ord))))
            .collect();
        // Changed points: put back by the inverses, newest first.
        let mut points_back = Vec::new();
        for text in inverses {
            if let Command::Batch { commands } = serde_json::from_str(&text).map_err(store)? {
                for c in commands {
                    match c {
                        c @ Command::ChangePoints { .. } => points_back.push(c),
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
        let mut diff = points_back;
        for (key, o) in &self.objects {
            match target.get(key) {
                Some((v, _)) if *v == *o.value => {}
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

pub mod annotations;
pub mod collab;
pub mod impact;
pub mod lines;
pub mod live;
pub mod packing;
pub mod points;
pub mod replace;
pub mod reports;
pub mod reproject;
pub mod spatial;
pub mod surfaces;
pub mod versions;

pub use live::Live;

#[cfg(test)]
mod lines_tests;
#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod points_tests;
#[cfg(test)]
mod replace_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod versions_tests;
