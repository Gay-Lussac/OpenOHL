// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Join routes from another project** (roadmap O17, *One model per
//! line*; owner, 2026-11-25): a line and its branches are one model, its
//! shared towers one object each. Where a line was split into several
//! project files, OHL-Design joins another file's routes into the open
//! project — one change (one undo step) and one revision "joined routes
//! from FILE". `ohl network import` stays one file = one line; twins
//! between separate lines stay as they are.
//!
//! What the network import does is reused, not written again: the
//! renaming of clashing ids ([`fresh_id`], with the session's own rename
//! so what refers to an object follows), the twin search's criterion
//! ([`twin_distance`]: the same type within [`TWIN_TOLERANCE`] in plan)
//! and the library mapping's matching of entries ([`match_entry`]). Both
//! `ohl_store_pg::import` and this module call them.
//!
//! 1. **The other file** is read into memory, never changed; its main
//!    project line comes in (its routes, alignments, alignment points,
//!    structures with their strings and wire points, sections, jumpers,
//!    spacers, and what they refer to: electrical systems, circuits and
//!    circuit starts, structure groups). A part of a line (a working set)
//!    or a file in another coordinate system is refused.
//! 2. **Ids clashing** with the open project's — of the network's kinds
//!    ([`NETWORK_KINDS`]), electrical systems, circuits (id or name) and
//!    structure groups — are renamed `PREFIX-id` (`PREFIX-id-2`, … when
//!    that is taken too) and listed; nothing is taken for "the same" by
//!    its id. A load case's own list (one per case) is joined into the
//!    open project's own list of the case: the picked and excluded
//!    structures added, the open project's rules kept (other rules
//!    listed).
//! 3. **Library entries** mapped as the network import maps them: an
//!    entry the open project has alike is reused (`same`); a new one is
//!    added to the open project's file of that name (`added`; a file that
//!    is one entry whose name is taken: under a free name); one the open
//!    project has with other values is a **conflict** — refused (the join
//!    waits) unless the user accepts the open project's entry for the
//!    joined routes, listed either way. A draft's file or a file that is
//!    not TOML is left and listed.
//! 4. **Twins**: the other file's structures of the same type within the
//!    tolerance of one of the open project's are listed with their height
//!    and orientation difference, **none merged unless ticked** (owner:
//!    never a silent change). A ticked twin: the open project's tower is
//!    kept as it is; the other line's sections, jumpers and circuit starts
//!    on it are re-attached to the kept tower's wire points **by label**
//!    (the other tower's wire point of the set → its label → the kept
//!    tower's wire point of that label → its set), its jumper clamps by
//!    their wire point's label, its alignment points tied to the kept
//!    tower, its structure groups naming the kept tower. A wire point
//!    whose label the kept tower has not (or has without a set) refuses
//!    that twin — it stays two towers, listed; nothing is guessed. The
//!    merged tower is then a tower shared by several routes (the rules of
//!    [`crate::shared`]); where its routes would not follow from the
//!    alignments, they are given on it (listed).
//! 5. Every **change of a joined structure's load cases** (by the open
//!    project's structure groups and rules) and every line setting that
//!    differs (the open project's apply to the joined routes) is listed.
//!
//! [`plan`] makes the plan (nothing is written); [`command`] gives the
//! one command (the library files changed and the merged line,
//! [`Command::ReplaceLine`]); [`join`] applies it and records the revision.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ohl_line::{Line, SectionPoint, Structure};
use ohl_structures::LibraryItem;
use ohl_structures::insulators::{InsulatorAssembly, InsulatorLibrary};
use ohl_structures::structures::{StructureLibrary, StructureType};
use serde::Serialize;

use crate::store::sqlite::SqliteStore;
use crate::versions::Revision;
use crate::{Applied, Command, Kind, Role, Session, SessionError, country, refused};

/// The kinds whose ids are unique in a network model (and renamed when
/// they clash, by the network import and by the join).
pub const NETWORK_KINDS: [&str; 7] = [
    "route",
    "alignment",
    "alignment_point",
    "structure",
    "section",
    "jumper",
    "spacer",
];

/// Twin towers: the largest distance in plan between two structures (of
/// the same type) taken for one tower, m (plan §3.5, an estimate to be
/// set by the owner).
pub const TWIN_TOLERANCE: f64 = 0.5;

/// Whether `code` may prefix renamed ids (a network line's code): 1 to 32
/// letters, digits, `_`, `.` or `-`, starting with a letter or digit.
pub fn check_prefix(code: &str) -> Result<(), String> {
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
        Err(format!(
            "\"{code}\": a code of 1 to 32 letters, digits, '_', '.' or '-'"
        ))
    }
}

/// The id an object whose `id` clashes takes: `CODE-id`, else
/// `CODE-id-2`, `CODE-id-3`, … — the first that `taken` says is free.
pub fn fresh_id<E>(
    code: &str,
    id: &str,
    mut taken: impl FnMut(&str) -> Result<bool, E>,
) -> Result<String, E> {
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            format!("{code}-{id}")
        } else {
            format!("{code}-{id}-{n}")
        };
        if !taken(&candidate)? {
            return Ok(candidate);
        }
        n += 1;
    }
}

/// The distance in plan between two structures at `a` and `b` of types
/// `a_type` and `b_type` (none: modelled point by point) when they are
/// twins — the same type, within `tolerance` — rounded to the mm; else
/// `None`.
pub fn twin_distance(
    a: [f64; 2],
    a_type: Option<&str>,
    b: [f64; 2],
    b_type: Option<&str>,
    tolerance: f64,
) -> Option<f64> {
    let d = (a[0] - b[0]).hypot(a[1] - b[1]);
    (d <= tolerance && a_type == b_type).then(|| (d * 1000.0).round() / 1000.0)
}

/// How an entry of a library file being brought in matches the library
/// it comes into ([`match_entry`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryMatch {
    /// That library has it alike, in this file.
    Same(String),
    /// That library has it with other values, in this file.
    Differs(String),
    /// That library has it not.
    New,
    /// That library has it not, and the file is one entry whose name a
    /// file of that library has already.
    NameTaken,
}

/// How entry `entry` of library file `name` of `kind` matches a library
/// whose files of the kind hold the entry ids `files` (file name → ids);
/// `version_in(file)` gives the entry's version in a file holding it (the
/// network reads it from its database, the join from the files). An entry
/// of a referenced kind ([`country::REFERENCED_KINDS`]) is looked for in
/// every file of the kind, another only in the file of the same name.
pub fn match_entry<E>(
    kind: &str,
    name: &str,
    entry: &country::Entry,
    files: &[(String, BTreeSet<String>)],
    mut version_in: impl FnMut(&str) -> Result<Option<String>, E>,
) -> Result<EntryMatch, E> {
    let referenced = country::REFERENCED_KINDS.contains(&kind);
    let mut found: Option<(String, String)> = None;
    for (f, ids) in files {
        if !(referenced || f == name) || !ids.contains(&entry.id) {
            continue;
        }
        if let Some(v) = version_in(f)? {
            found = Some((f.clone(), v));
        }
    }
    Ok(match found {
        Some((f, v)) if v == entry.version => EntryMatch::Same(f),
        Some((f, _)) => EntryMatch::Differs(f),
        None if entry.list.is_none() && files.iter().any(|(f, _)| f == name) => {
            EntryMatch::NameTaken
        }
        None => EntryMatch::New,
    })
}

/// An object renamed by the join.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Renamed {
    /// Its kind (`structure`, `system`, `circuit`, `structure group`…).
    pub kind: String,
    /// Its id in the other file.
    pub from: String,
    /// Its id in the open project.
    pub to: String,
}

/// What became of a library entry of the other file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Outcome {
    /// The open project has it alike (in this file): reused.
    Same(String),
    /// Added to the open project's file of this name (new, or made).
    Added(String),
    /// The open project has it with other values (in this file): the
    /// join is refused unless `accepted` (the open project's entry then
    /// used by the joined routes).
    Conflict {
        /// The open project's file holding it.
        file: String,
        /// The user accepted the open project's entry.
        accepted: bool,
    },
    /// Left out, why (a draft's file, not TOML).
    Left(String),
}

/// One entry of the other file's library files and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mapping {
    /// The kind (`cables`, `structures`…).
    pub kind: String,
    /// The other file's library file.
    pub file: String,
    /// The entry (empty: the whole file).
    pub entry: String,
    /// What became of it.
    pub outcome: Outcome,
}

impl Mapping {
    /// The key a conflict is accepted by: `kind/file/entry`.
    pub fn key(&self) -> String {
        format!("{}/{}/{}", self.kind, self.file, self.entry)
    }

    /// In words.
    pub fn words(&self) -> String {
        let what = if self.entry.is_empty() {
            format!("{}/{}", self.kind, self.file)
        } else {
            format!("{} {} ({}/{})", self.kind, self.entry, self.kind, self.file)
        };
        match &self.outcome {
            Outcome::Same(f) => format!("{what}: the same as the open project's (in {f})"),
            Outcome::Added(f) if *f == self.file => format!("{what}: added"),
            Outcome::Added(f) => format!("{what}: added as {f} (the name was taken)"),
            Outcome::Conflict { file, accepted } => format!(
                "{what}: differs from the open project's (in {file}) — {}",
                if *accepted {
                    "the open project's entry used for the joined routes (accepted)"
                } else {
                    "refused: the join waits until it is settled"
                }
            ),
            Outcome::Left(why) => format!("{what}: left in the file ({why})"),
        }
    }
}

/// What became of a twin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TwinState {
    /// Not ticked: two towers.
    Apart,
    /// Ticked and merged: one shared tower; how the other line's objects
    /// were re-attached, in words.
    Merged(Vec<String>),
    /// Ticked and refused (two towers): why, in words.
    Refused(Vec<String>),
}

/// A structure of the other file of the same type within the tolerance of
/// one of the open project's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Twin {
    /// The open project's structure.
    pub open: String,
    /// The other file's structure (its id there).
    pub other: String,
    /// Its id when joined apart (renamed when it clashed).
    pub joined: String,
    /// Distance in plan, m.
    pub distance_m: f64,
    /// Their type (none: modelled point by point).
    pub structure_type: Option<String>,
    /// The other's height above its ground point less the open one's, m
    /// (the highest point, wire point or member; none: one not resolved).
    pub height_difference_m: Option<f64>,
    /// The other's ground point's level less the open one's, m.
    pub level_difference_m: f64,
    /// The other's orientation less the open one's, degrees, in (−180,
    /// 180].
    pub orientation_difference_deg: f64,
    /// Ticked: to be one shared tower.
    pub ticked: bool,
    /// What became of it.
    pub state: TwinState,
}

impl Twin {
    /// The key a twin is ticked by: `open|other`.
    pub fn key(&self) -> (String, String) {
        (self.open.clone(), self.other.clone())
    }
}

/// How to join.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JoinOptions {
    /// The prefix of renamed ids (a code: [`check_prefix`]).
    pub prefix: String,
    /// Twins within this distance in plan, m ([`TWIN_TOLERANCE`]).
    pub tolerance: f64,
    /// The twins ticked to become one shared tower: (the open project's
    /// structure, the other file's structure as it is there). None by
    /// default.
    pub merge: BTreeSet<(String, String)>,
    /// The library conflicts accepted (the open project's entry used for
    /// the joined routes), by [`Mapping::key`]. None by default.
    pub accept: BTreeSet<String>,
}

impl JoinOptions {
    /// With the prefix `prefix`, the twin tolerance, nothing ticked.
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_owned(),
            tolerance: TWIN_TOLERANCE,
            ..Self::default()
        }
    }
}

/// The plan of a join: what comes, renamed, mapped, merged; nothing is
/// written.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinPlan {
    /// The other file (its name only).
    pub file: String,
    /// Its line's name.
    pub other_name: String,
    /// The other file's main revision it came from (`None`: not
    /// committed).
    pub source_revision: Option<String>,
    /// Its routes, as they are named in the open project after the join
    /// (from, to).
    pub routes: Vec<(String, String)>,
    /// Objects joined, by kind (the merged towers left out).
    pub objects: BTreeMap<String, usize>,
    /// Ids renamed.
    pub renamed: Vec<Renamed>,
    /// Twins, ticked or not.
    pub twins: Vec<Twin>,
    /// The other file's library entries and what became of them.
    pub libraries: Vec<Mapping>,
    /// What else was done, in words.
    pub notes: Vec<String>,
    /// What changes for the joined routes or the open project's towers
    /// (load cases, settings), in words — to look at before applying.
    pub warnings: Vec<String>,
    /// What stays in the other file, in words.
    pub left_in_file: Vec<String>,
    /// The merged line.
    pub line: Line,
    /// The library files to write (kind, name, text).
    pub library_changes: Vec<(String, String, String)>,
    /// Why it cannot be applied (empty: it can).
    pub blocked: Vec<String>,
}

impl JoinPlan {
    /// Whether it can be applied.
    pub fn can_apply(&self) -> bool {
        self.blocked.is_empty()
    }

    /// The revision's message (and the change's name).
    pub fn message(&self) -> String {
        format!("joined routes from {}", self.file)
    }

    /// The plan in words (the command line's report).
    pub fn text(&self) -> String {
        let mut out = String::new();
        let mut line = |s: String| {
            out.push_str(&s);
            out.push('\n');
        };
        line(format!(
            "join routes from {} (line \"{}\", {}):",
            self.file,
            self.other_name,
            self.source_revision.as_deref().map_or(
                "its uncommitted state".to_owned(),
                |r| format!("revision {}", crate::versions::short(r))
            )
        ));
        line(format!(
            "  routes: {}",
            self.routes
                .iter()
                .map(|(a, b)| if a == b {
                    a.clone()
                } else {
                    format!("{a} → {b}")
                })
                .collect::<Vec<_>>()
                .join(", ")
        ));
        line(format!(
            "  objects: {}",
            self.objects
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        line(format!("  ids renamed: {}", self.renamed.len()));
        for r in &self.renamed {
            line(format!("    {} {} → {}", r.kind, r.from, r.to));
        }
        line(format!("  twins: {}", self.twins.len()));
        for t in &self.twins {
            line(format!("    {}", twin_words(t)));
        }
        line(format!("  library entries: {}", self.libraries.len()));
        for m in &self.libraries {
            line(format!("    {}", m.words()));
        }
        for (title, list) in [
            ("notes", &self.notes),
            ("to look at", &self.warnings),
            ("left in the file", &self.left_in_file),
            ("refused", &self.blocked),
        ] {
            if !list.is_empty() {
                line(format!("  {title}:"));
                for n in list {
                    line(format!("    {n}"));
                }
            }
        }
        out
    }
}

/// A twin in words.
pub fn twin_words(t: &Twin) -> String {
    let height = t
        .height_difference_m
        .map_or("height not compared".to_owned(), |h| {
            format!("height {h:+.2} m")
        });
    let state = match &t.state {
        TwinState::Apart => "two towers (not ticked)".to_owned(),
        TwinState::Merged(how) => format!("one shared tower: {}", how.join("; ")),
        TwinState::Refused(why) => format!("refused, two towers: {}", why.join("; ")),
    };
    format!(
        "{} (this project) and {} (joined{}): {:.3} m apart, type {}, {height}, level {:+.2} m, \
         turned {:+.1}° — {state}",
        t.open,
        t.other,
        if t.joined == t.other {
            String::new()
        } else {
            format!(" as {}", t.joined)
        },
        t.distance_m,
        t.structure_type.as_deref().unwrap_or("none"),
        t.level_difference_m,
        t.orientation_difference_deg
    )
}

/// The other file read: its session on an in-memory copy, on its main
/// project line.
fn read_other(path: &Path) -> Result<Session, SessionError> {
    let file = file_name(path);
    if !path.is_file() {
        return Err(refused(format!("{}: no such project file", path.display())));
    }
    let copy = SqliteStore::copy_in_memory(path)?;
    let mut session = Session::open_on(Box::new(copy), &path.to_string_lossy())?;
    if session.store().meta(crate::part::PART_KEY)?.is_some() {
        return Err(refused(format!(
            "{file} holds a part of a line (a working set): join the whole line"
        )));
    }
    let main = crate::versions::MAIN;
    if session.current_line() != main {
        session.select_line(main)?;
    }
    if !session.has_line() {
        return Err(refused(format!("{file} holds no line")));
    }
    Ok(session)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The ids of `line`'s objects of network kind `kind`.
fn ids_of(line: &Line, kind: &str) -> BTreeSet<String> {
    match kind {
        "route" => line.routes.iter().map(|x| x.id.clone()).collect(),
        "alignment" => line.alignments.iter().map(|x| x.id.clone()).collect(),
        "alignment_point" => line.alignment_points.iter().map(|x| x.id.clone()).collect(),
        "structure" => line.structures.iter().map(|x| x.id.clone()).collect(),
        "section" => line.sections.iter().map(|x| x.id.clone()).collect(),
        "jumper" => line.jumpers.iter().map(|x| x.id.clone()).collect(),
        "spacer" => line.spacers.iter().map(|x| x.id.clone()).collect(),
        _ => BTreeSet::new(),
    }
}

/// Structure types and insulator assemblies read from library files
/// (kind, name, text); the files that do not read, in words.
fn type_libraries(
    files: &[(String, String, String)],
) -> (StructureLibrary, InsulatorLibrary, Vec<String>) {
    let mut structures = StructureLibrary::default();
    let mut insulators = InsulatorLibrary::default();
    let mut errors = Vec::new();
    for (kind, name, text) in files {
        let path = Path::new(name);
        let done = match kind.as_str() {
            "structures" => StructureType::parse(path, text)
                .and_then(|t| structures.insert(path, t))
                .map_err(|e| e.to_string()),
            "insulators" => InsulatorAssembly::parse(path, text)
                .and_then(|t| insulators.insert(path, t))
                .map_err(|e| e.to_string()),
            _ => Ok(()),
        };
        if let Err(e) = done {
            errors.push(format!("{kind}/{name}: {e}"));
        }
    }
    (structures, insulators, errors)
}

/// Structure `s` with its type's points, strings and wire points (as the
/// line resolves it).
fn resolved(
    s: &Structure,
    libs: &(StructureLibrary, InsulatorLibrary, Vec<String>),
) -> Result<Structure, String> {
    ohl_line::library::resolve_structure(s, &libs.0, &libs.1, None).map_err(|e| e.to_string())
}

/// The highest point of a resolved structure above its ground point, m.
fn height(s: &Structure) -> f64 {
    let mut top = f64::NEG_INFINITY;
    for p in &s.points {
        top = top.max(p.position[2]);
    }
    for w in &s.wires {
        top = top.max(w.position[2]);
    }
    for m in &s.members {
        for p in m {
            top = top.max(p[2]);
        }
    }
    if top.is_finite() {
        top - s.position[2]
    } else {
        0.0
    }
}

/// `deg` in (−180, 180].
fn wrap_deg(deg: f64) -> f64 {
    let mut d = deg % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// A load case set in words.
fn case_words(c: &ohl_line::CaseSet) -> String {
    match c {
        ohl_line::CaseSet::Every => "every case".into(),
        ohl_line::CaseSet::EveryBut(but) => format!("every case but {}", but.join(", ")),
        ohl_line::CaseSet::These(these) => these.join(", "),
        ohl_line::CaseSet::NoCase => "no load case".into(),
    }
}

/// Makes the plan of joining the main project line of the project file
/// `other` into the open project, whose line (as stored: typed structures
/// not merged with their types) is `open` and whose library files (kind,
/// name, text) are `open_libs`. Nothing is written; the other file is
/// read, never changed.
pub fn plan(
    open: &Line,
    open_libs: &[(String, String, String)],
    other: &Path,
    options: &JoinOptions,
) -> Result<JoinPlan, SessionError> {
    check_prefix(&options.prefix).map_err(refused)?;
    if !(options.tolerance.is_finite() && options.tolerance >= 0.0) {
        return Err(refused(format!(
            "twin tolerance {} m: zero or more",
            options.tolerance
        )));
    }
    let file = file_name(other);
    let mut scratch = read_other(other)?;
    let main = crate::versions::MAIN;
    // One coordinate system.
    let crs = scratch.line().crs.clone();
    if crs != open.crs {
        return Err(refused(format!(
            "{file} is in \"{crs}\", this project in \"{}\": reproject one of them first \
             (Terrain › Coordinate system)",
            open.crs
        )));
    }
    let source_revision = scratch
        .store()
        .refs()?
        .into_iter()
        .find(|r| r.kind == "branch" && r.name == main)
        .and_then(|r| r.revision);
    let mut left_in_file = Vec::new();
    {
        let refs = scratch.store().refs()?;
        let others = refs
            .iter()
            .filter(|r| r.kind == "branch" && r.name != main)
            .count();
        if others > 0 {
            left_in_file.push(format!(
                "{others} other project line(s): only its main line is joined"
            ));
        }
        let revisions = scratch.store().revisions()?.len();
        if revisions > 0 {
            left_in_file.push(format!(
                "{revisions} revision(s) (its history; the joined state is its current one)"
            ));
        }
        left_in_file
            .push("its point clouds, backgrounds, terrain surfaces, users and notes".to_owned());
    }
    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut blocked = Vec::new();

    // 2. Ids clashing: renamed in the scratch session with the session's
    // own rename (what refers to an object follows), as the network
    // import renames them.
    let theirs = scratch.export();
    let mut renamed: Vec<Renamed> = Vec::new();
    let mut plan_renames: Vec<(Kind, String, String)> = Vec::new();
    for kind in NETWORK_KINDS {
        let ours = ids_of(open, kind);
        let mine = ids_of(&theirs, kind);
        for id in mine.intersection(&ours) {
            let to = fresh_id(&options.prefix, id, |c| -> Result<bool, SessionError> {
                Ok(ours.contains(c)
                    || mine.contains(c)
                    || renamed.iter().any(|r| r.kind == kind && r.to == c))
            })?;
            renamed.push(Renamed {
                kind: kind.to_owned(),
                from: id.clone(),
                to: to.clone(),
            });
            plan_renames.push((Kind::parse(kind)?, id.clone(), to));
        }
    }
    // Electrical systems.
    let mut system_renames = Vec::new();
    {
        let ours: BTreeSet<&str> = open.systems.iter().map(|s| s.id.as_str()).collect();
        let mine: BTreeSet<&str> = theirs.systems.iter().map(|s| s.id.as_str()).collect();
        for id in mine.intersection(&ours) {
            let to = fresh_id(&options.prefix, id, |c| -> Result<bool, SessionError> {
                Ok(ours.contains(c) || mine.contains(c))
            })?;
            renamed.push(Renamed {
                kind: "system".into(),
                from: (*id).to_owned(),
                to: to.clone(),
            });
            system_renames.push(((*id).to_owned(), to));
        }
    }
    if !plan_renames.is_empty() || !system_renames.is_empty() {
        let admin = scratch
            .users()?
            .into_iter()
            .find(|(_, r)| *r == Role::Admin)
            .map(|(n, _)| n)
            .ok_or_else(|| refused(format!("{file} has no admin to rename its objects as")))?;
        for (kind, from, to) in &plan_renames {
            scratch
                .apply(
                    &admin,
                    Command::Rename {
                        kind: *kind,
                        id: from.clone(),
                        new_id: to.clone(),
                    },
                )
                .map_err(|e| refused(format!("renaming {} {from}: {e}", kind.as_str())))?;
        }
        for (from, to) in &system_renames {
            scratch
                .apply(
                    &admin,
                    Command::RenameSystem {
                        id: from.clone(),
                        new_id: to.clone(),
                    },
                )
                .map_err(|e| refused(format!("renaming system {from}: {e}")))?;
        }
    }
    let mut theirs = scratch.export();
    let other_name = theirs.name.clone();
    // Circuits: by id or name (sections name them by name).
    {
        let ids: BTreeSet<String> = open.circuits.iter().map(|c| c.id.clone()).collect();
        let names: BTreeSet<String> = open.circuits.iter().map(|c| c.name.clone()).collect();
        let my_ids: BTreeSet<String> = theirs.circuits.iter().map(|c| c.id.clone()).collect();
        let my_names: BTreeSet<String> = theirs.circuits.iter().map(|c| c.name.clone()).collect();
        let mut name_map: BTreeMap<String, String> = BTreeMap::new();
        for c in &mut theirs.circuits {
            // A circuit's system follows the system renamed.
            if let Some((_, to)) = system_renames
                .iter()
                .find(|(from, _)| c.system.as_deref() == Some(from.as_str()))
            {
                c.system = Some(to.clone());
            }
            if !(ids.contains(&c.id) || names.contains(&c.name)) {
                continue;
            }
            let id = fresh_id(&options.prefix, &c.id, |x| -> Result<bool, SessionError> {
                Ok(ids.contains(x) || my_ids.contains(x))
            })?;
            let name = fresh_id(
                &options.prefix,
                &c.name,
                |x| -> Result<bool, SessionError> { Ok(names.contains(x) || my_names.contains(x)) },
            )?;
            renamed.push(Renamed {
                kind: "circuit".into(),
                from: c.id.clone(),
                to: id.clone(),
            });
            name_map.insert(c.name.clone(), name.clone());
            c.id = id;
            c.name = name;
        }
        for s in &mut theirs.sections {
            if let Some(n) = name_map.get(&s.circuit_name) {
                s.circuit_name = n.clone();
            }
        }
    }
    // Structure groups: by name; a load case's own list joined into the
    // open project's own list of its case.
    let own_lists: Vec<ohl_line::StructureGroup>;
    {
        let names: BTreeSet<String> = open
            .structure_groups
            .iter()
            .filter(|g| !g.is_own_list())
            .map(|g| g.name.clone())
            .collect();
        let mine: BTreeSet<String> = theirs
            .structure_groups
            .iter()
            .filter(|g| !g.is_own_list())
            .map(|g| g.name.clone())
            .collect();
        for g in &mut theirs.structure_groups {
            if g.is_own_list() {
                continue;
            }
            if names.contains(&g.name) {
                let to = fresh_id(
                    &options.prefix,
                    &g.name,
                    |x| -> Result<bool, SessionError> { Ok(names.contains(x) || mine.contains(x)) },
                )?;
                renamed.push(Renamed {
                    kind: "structure group".into(),
                    from: g.name.clone(),
                    to: to.clone(),
                });
                g.name = to;
            }
        }
        let (own, groups): (Vec<_>, Vec<_>) = std::mem::take(&mut theirs.structure_groups)
            .into_iter()
            .partition(|g| g.is_own_list());
        theirs.structure_groups = groups;
        own_lists = own;
    }

    // What the other line's structures took before (their ids as joined).
    let cases_before = theirs.case_sets();
    let open_cases_before = open.case_sets();
    let routes_before = theirs.structure_routes();
    let open_routes = open.structure_routes();

    // 4. Twins: the network import's criterion.
    let open_libs_read = type_libraries(open_libs);
    let other_lib_files = scratch.line_library_all(main)?;
    let other_libs_read = type_libraries(&other_lib_files);
    let from_id: BTreeMap<&str, &str> = renamed
        .iter()
        .filter(|r| r.kind == "structure")
        .map(|r| (r.to.as_str(), r.from.as_str()))
        .collect();
    let cell = options.tolerance.max(1.0);
    let key = |p: [f64; 3]| ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64);
    let mut grid: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (i, s) in open.structures.iter().enumerate() {
        grid.entry(key(s.position)).or_default().push(i);
    }
    let mut twins: Vec<Twin> = Vec::new();
    for s in &theirs.structures {
        let (cx, cy) = key(s.position);
        let mut near: Vec<usize> = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = grid.get(&(cx + dx, cy + dy)) {
                    near.extend(v);
                }
            }
        }
        near.sort_unstable();
        for i in near {
            let o = &open.structures[i];
            let Some(d) = twin_distance(
                [o.position[0], o.position[1]],
                o.structure_type.as_deref(),
                [s.position[0], s.position[1]],
                s.structure_type.as_deref(),
                options.tolerance,
            ) else {
                continue;
            };
            let other_id = from_id
                .get(s.id.as_str())
                .map_or_else(|| s.id.clone(), |x| (*x).to_owned());
            let heights = resolved(o, &open_libs_read)
                .and_then(|ro| resolved(s, &other_libs_read).map(|rs| height(&rs) - height(&ro)));
            let ticked = options.merge.contains(&(o.id.clone(), other_id.clone()));
            twins.push(Twin {
                open: o.id.clone(),
                other: other_id,
                joined: s.id.clone(),
                distance_m: d,
                structure_type: o.structure_type.clone(),
                height_difference_m: heights.ok().map(|h| (h * 1000.0).round() / 1000.0),
                level_difference_m: ((s.position[2] - o.position[2]) * 1000.0).round() / 1000.0,
                orientation_difference_deg: (wrap_deg(s.orientation_deg - o.orientation_deg)
                    * 1000.0)
                    .round()
                    / 1000.0,
                ticked,
                state: TwinState::Apart,
            });
        }
    }
    twins.sort_by(|a, b| (&a.open, &a.other).cmp(&(&b.open, &b.other)));

    // Ticked twins merged, each on its own (a refusal leaves that twin
    // two towers).
    let mut merged_into: BTreeMap<String, String> = BTreeMap::new(); // joined id → kept
    let mut kept_used: BTreeSet<String> = BTreeSet::new();
    for t in twins.iter_mut().filter(|t| t.ticked) {
        if merged_into.contains_key(&t.joined) {
            t.state = TwinState::Refused(vec![format!(
                "{} is one tower with {} already",
                t.other, merged_into[&t.joined]
            )]);
            continue;
        }
        if kept_used.contains(&t.open) {
            t.state = TwinState::Refused(vec![format!(
                "{} is one tower with another of the joined structures already",
                t.open
            )]);
            continue;
        }
        let kept = open
            .structure(&t.open)
            .cloned()
            .expect("a twin of the line");
        let gone = theirs.structure(&t.joined).cloned().expect("a twin joined");
        match reattach(&mut theirs, &kept, &gone, &open_libs_read, &other_libs_read) {
            Ok(how) => {
                merged_into.insert(t.joined.clone(), t.open.clone());
                kept_used.insert(t.open.clone());
                t.state = TwinState::Merged(how);
            }
            Err(why) => t.state = TwinState::Refused(why),
        }
    }

    // The merged line: the open project's, the other line's objects
    // after it; the open project's settings kept.
    let mut line = open.clone();
    let route_pairs: Vec<(String, String)> = {
        let from_route: BTreeMap<&str, &str> = renamed
            .iter()
            .filter(|r| r.kind == "route")
            .map(|r| (r.to.as_str(), r.from.as_str()))
            .collect();
        theirs
            .routes
            .iter()
            .map(|r| {
                (
                    from_route
                        .get(r.id.as_str())
                        .map_or_else(|| r.id.clone(), |x| (*x).to_owned()),
                    r.id.clone(),
                )
            })
            .collect()
    };
    let mut objects: BTreeMap<String, usize> = BTreeMap::new();
    let mut count = |k: &str, n: usize| {
        if n > 0 {
            objects.insert(k.to_owned(), n);
        }
    };
    count("routes", theirs.routes.len());
    count("alignments", theirs.alignments.len());
    count("alignment points", theirs.alignment_points.len());
    count(
        "structures",
        theirs
            .structures
            .iter()
            .filter(|s| !merged_into.contains_key(&s.id))
            .count(),
    );
    count("sections", theirs.sections.len());
    count("jumpers", theirs.jumpers.len());
    count("spacers", theirs.spacers.len());
    count("electrical systems", theirs.systems.len());
    count("circuits", theirs.circuits.len());
    count("structure groups", theirs.structure_groups.len());
    line.routes.extend(theirs.routes.iter().cloned());
    line.alignments.extend(theirs.alignments.iter().cloned());
    line.alignment_points
        .extend(theirs.alignment_points.iter().cloned());
    line.structures.extend(
        theirs
            .structures
            .iter()
            .filter(|s| !merged_into.contains_key(&s.id))
            .cloned(),
    );
    line.sections.extend(theirs.sections.iter().cloned());
    line.jumpers.extend(theirs.jumpers.iter().cloned());
    line.spacers.extend(theirs.spacers.iter().cloned());
    line.systems.extend(theirs.systems.iter().cloned());
    line.circuits.extend(theirs.circuits.iter().cloned());
    for p in &theirs.circuit_starts {
        if !line.circuit_starts.contains(p) {
            line.circuit_starts.push(p.clone());
        }
    }
    line.structure_groups
        .extend(theirs.structure_groups.iter().cloned());
    for g in own_lists {
        let case = g.load_case.clone().unwrap_or_default();
        match line
            .structure_groups
            .iter_mut()
            .find(|x| x.load_case == g.load_case)
        {
            Some(x) => {
                for s in &g.structures {
                    if !x.structures.contains(s) {
                        x.structures.push(s.clone());
                    }
                }
                for s in &g.excluded {
                    if !x.excluded.contains(s) {
                        x.excluded.push(s.clone());
                    }
                }
                if x.rules != g.rules {
                    warnings.push(format!(
                        "load case {case}: its own list's rules in {file} differ from this \
                         project's; this project's are kept (the joined list's picked and \
                         excluded structures added)"
                    ));
                }
                notes.push(format!(
                    "load case {case}: the own lists joined (its picked and excluded structures)"
                ));
            }
            None => line.structure_groups.push(g),
        }
    }
    // A merged tower's routes: those of both lines; given on it where
    // the alignments would not give them.
    let after_routes = line.structure_routes();
    for (joined, kept) in &merged_into {
        let mut want: Vec<String> = open_routes.get(kept).cloned().unwrap_or_default();
        for r in routes_before.get(joined).into_iter().flatten() {
            if !want.contains(r) {
                want.push(r.clone());
            }
        }
        let has = after_routes.get(kept).cloned().unwrap_or_default();
        if want.iter().any(|r| !has.contains(r))
            && let Some(s) = line.structures.iter_mut().find(|s| &s.id == kept)
        {
            let mut given = s.routes.clone();
            for r in &want {
                if !given.contains(r) {
                    given.push(r.clone());
                }
            }
            notes.push(format!(
                "{kept}: its routes given on it as {} (the alignments alone would not give \
                 them all)",
                given.join(", ")
            ));
            s.routes = given;
        }
    }

    // 5. What changes for the structures' load cases.
    let cases_after = line.case_sets();
    for s in &theirs.structures {
        let (id, before) = match merged_into.get(&s.id) {
            Some(_) => continue,
            None => (s.id.as_str(), cases_before.of(&s.id)),
        };
        let after = cases_after.of(id);
        if before != after {
            warnings.push(format!(
                "structure {id} (joined) took {}; after the join it takes {} (this project's \
                 structure groups)",
                case_words(before),
                case_words(after)
            ));
        }
    }
    for (joined, kept) in &merged_into {
        let before = open_cases_before.of(kept);
        let after = cases_after.of(kept);
        if before != after {
            warnings.push(format!(
                "structure {kept} (one tower with the joined {joined}) took {}; after the join \
                 it takes {}",
                case_words(before),
                case_words(after)
            ));
        }
    }
    // The line's settings: the open project's apply to the joined routes.
    let settings: [(&str, bool); 7] = [
        (
            "creep-stretch criteria",
            differs(&open.creep_stretch, &theirs.creep_stretch),
        ),
        (
            "bimetallic conductor model",
            differs(&open.bimetallic, &theirs.bimetallic),
        ),
        ("calculation options", differs(&open.solver, &theirs.solver)),
        (
            "electrical settings",
            differs(&open.electrical, &theirs.electrical),
        ),
        (
            "reference case of the structures",
            differs(&open.reference, &theirs.reference),
        ),
        (
            "height datum",
            differs(&open.height_datum, &theirs.height_datum),
        ),
        ("default values", differs(&open.defaults, &theirs.defaults)),
    ];
    for (what, d) in settings {
        if d {
            warnings.push(format!(
                "the {what} of {file} differ from this project's: this project's apply to the \
                 joined routes"
            ));
        }
    }

    // 3. The library files, as the network import maps them.
    let (libraries, library_changes) = map_libraries(open_libs, &other_lib_files, &options.accept)?;
    for m in &libraries {
        match &m.outcome {
            Outcome::Conflict {
                accepted: false, ..
            } => blocked.push(format!(
                "{}: accept this project's entry for the joined routes, or make the entries \
                 the same, to join",
                m.words()
            )),
            Outcome::Conflict { accepted: true, .. } => warnings.push(m.words()),
            Outcome::Left(_) => warnings.push(m.words()),
            _ => {}
        }
    }
    for e in &other_libs_read.2 {
        notes.push(format!(
            "{file}'s library file {e} (twins not resolved with it)"
        ));
    }

    // The merged line must be valid as the session would take it.
    if let Err(e) = line.validate() {
        blocked.push(format!("the joined line is not valid: {e}"));
    }
    Ok(JoinPlan {
        file,
        other_name,
        source_revision,
        routes: route_pairs,
        objects,
        renamed,
        twins,
        libraries,
        notes,
        warnings,
        left_in_file,
        line,
        library_changes,
        blocked,
    })
}

/// Whether two settings differ (compared as they are written).
fn differs<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() != serde_json::to_value(b).ok()
}

/// The other line's objects on structure `gone` (in `theirs`) re-attached
/// to `kept` (the open project's) by wire point label, and `gone`'s
/// other references turned to `kept` (`gone` is then left out of the
/// merged line by the caller). How, in words; or why not (nothing is
/// changed then).
fn reattach(
    theirs: &mut Line,
    kept: &Structure,
    gone: &Structure,
    open_libs: &(StructureLibrary, InsulatorLibrary, Vec<String>),
    other_libs: &(StructureLibrary, InsulatorLibrary, Vec<String>),
) -> Result<Vec<String>, Vec<String>> {
    let rk = resolved(kept, open_libs).map_err(|e| {
        vec![format!(
            "{} not resolved with this project's types: {e}",
            kept.id
        )]
    })?;
    let rg = resolved(gone, other_libs).map_err(|e| {
        vec![format!(
            "{} not resolved with its file's types: {e}",
            gone.id
        )]
    })?;
    let mut why: Vec<String> = Vec::new();
    let mut how: Vec<String> = Vec::new();
    // Set on the gone tower → set on the kept one, by label.
    let mut sets: BTreeMap<u32, u32> = BTreeMap::new();
    let mut map_set = |set: u32, what: &str, why: &mut Vec<String>| -> Option<u32> {
        if let Some(s) = sets.get(&set) {
            return Some(*s);
        }
        let Some(w) = rg.wire_at_set(set) else {
            why.push(format!(
                "{what}: {} has no wire point of set {set}",
                gone.id
            ));
            return None;
        };
        match rk.wire(&w.label) {
            Some(k) => match k.set {
                Some(s) => {
                    sets.insert(set, s);
                    Some(s)
                }
                None => {
                    why.push(format!(
                        "{what}: wire point {} of {} has no set (no section can end there)",
                        w.label, kept.id
                    ));
                    None
                }
            },
            None => {
                why.push(format!(
                    "{what}: {} has no wire point labelled {} (set {set} of {})",
                    kept.id, w.label, gone.id
                ));
                None
            }
        }
    };
    let on = |p: &SectionPoint| p.structure == gone.id;
    for sec in &theirs.sections {
        for p in sec.points.iter().filter(|p| on(p)) {
            map_set(p.set, &format!("section {}", sec.id), &mut why);
        }
    }
    for j in &theirs.jumpers {
        for p in j.points().filter(|p| on(p)) {
            map_set(p.set, &format!("jumper {}", j.id), &mut why);
        }
        let from_here = j.from.point().is_some_and(on);
        for c in j.supports() {
            let here = c.structure.as_deref() == Some(gone.id.as_str())
                || (c.structure.is_none() && from_here);
            if here && rk.wire(&c.wire).is_none() {
                why.push(format!(
                    "jumper {}: its clamp {} hangs on wire point {}, which {} has not",
                    j.id, c.label, c.wire, kept.id
                ));
            }
        }
    }
    for p in theirs.circuit_starts.iter().filter(|p| on(p)) {
        map_set(p.set, "a circuit start", &mut why);
    }
    if !why.is_empty() {
        why.sort();
        why.dedup();
        return Err(why);
    }
    // Applied.
    let to = |p: &mut SectionPoint, sets: &BTreeMap<u32, u32>| {
        if p.structure == gone.id {
            p.structure = kept.id.clone();
            p.set = sets[&p.set];
        }
    };
    for sec in &mut theirs.sections {
        if sec.points.iter().any(on) {
            for p in &mut sec.points {
                to(p, &sets);
            }
            how.push(format!("section {} re-attached", sec.id));
        }
    }
    for j in &mut theirs.jumpers {
        let touched = j.points().any(on)
            || j.supports()
                .iter()
                .any(|c| c.structure.as_deref() == Some(gone.id.as_str()));
        if !touched {
            continue;
        }
        for p in [&mut j.from, &mut j.to]
            .into_iter()
            .filter_map(ohl_line::JumperEnd::point_mut)
        {
            to(p, &sets);
        }
        let (g, k) = (gone.id.clone(), kept.id.clone());
        j.edit_supports(|cs| {
            for c in cs.iter_mut() {
                if c.structure.as_deref() == Some(g.as_str()) {
                    c.structure = Some(k.clone());
                }
            }
        });
        how.push(format!("jumper {} re-attached", j.id));
    }
    for p in &mut theirs.circuit_starts {
        to(p, &sets);
    }
    for p in &mut theirs.alignment_points {
        if p.structure.as_deref() == Some(gone.id.as_str()) {
            p.structure = Some(kept.id.clone());
            p.position = kept.position;
            how.push(format!("alignment point {} tied to {}", p.id, kept.id));
        }
    }
    for g in &mut theirs.structure_groups {
        for s in g.structures.iter_mut().chain(&mut g.excluded) {
            if *s == gone.id {
                *s = kept.id.clone();
            }
        }
    }
    let changed: Vec<String> = sets
        .iter()
        .filter(|(a, b)| a != b)
        .map(|(a, b)| format!("set {a} to set {b}"))
        .collect();
    if !changed.is_empty() {
        how.push(format!("by label: {}", changed.join(", ")));
    }
    if how.is_empty() {
        how.push("nothing hung on it".into());
    }
    Ok(how)
}

/// The other file's library files mapped onto the open project's, as the
/// network import maps them ([`match_entry`]): the mappings, and the
/// files to write (kind, name, text).
#[allow(clippy::type_complexity)]
fn map_libraries(
    open: &[(String, String, String)],
    other: &[(String, String, String)],
    accept: &BTreeSet<String>,
) -> Result<(Vec<Mapping>, Vec<(String, String, String)>), SessionError> {
    // The open project's files as they will be, by (kind, name).
    let mut texts: BTreeMap<(String, String), String> = open
        .iter()
        .map(|(k, n, t)| ((k.clone(), n.clone()), t.clone()))
        .collect();
    let mut changed: BTreeSet<(String, String)> = BTreeSet::new();
    let mut out = Vec::new();
    for (kind, name, text) in other {
        let mut push = |entry: &str, outcome: Outcome| {
            out.push(Mapping {
                kind: kind.clone(),
                file: name.clone(),
                entry: entry.to_owned(),
                outcome,
            });
        };
        if country::draft_of(name).is_some() {
            push("", Outcome::Left("a draft's file".into()));
            continue;
        }
        let entries = match country::entries(name, text) {
            Ok(e) => e,
            Err(e) => {
                push("", Outcome::Left(e));
                continue;
            }
        };
        for e in entries {
            // The open project's files of the kind as they are now (an
            // entry added before counts).
            let files: Vec<(String, Vec<country::Entry>)> = texts
                .iter()
                .filter(|((k, _), _)| k == kind)
                .map(|((_, n), t)| (n.clone(), country::entries(n, t).unwrap_or_default()))
                .collect();
            let ids: Vec<(String, BTreeSet<String>)> = files
                .iter()
                .map(|(n, es)| (n.clone(), es.iter().map(|x| x.id.clone()).collect()))
                .collect();
            let found = match_entry(kind, name, &e, &ids, |f| -> Result<_, SessionError> {
                Ok(files
                    .iter()
                    .find(|(n, _)| n == f)
                    .and_then(|(_, es)| es.iter().find(|x| x.id == e.id))
                    .map(|x| x.version.clone()))
            })?;
            match found {
                EntryMatch::Same(f) => push(&e.id, Outcome::Same(f)),
                EntryMatch::Differs(f) => {
                    let key = format!("{kind}/{name}/{}", e.id);
                    push(
                        &e.id,
                        Outcome::Conflict {
                            file: f,
                            accepted: accept.contains(&key),
                        },
                    );
                }
                EntryMatch::New | EntryMatch::NameTaken => {
                    let target = if found == EntryMatch::NameTaken {
                        free_file_name(name, |n| texts.contains_key(&(kind.clone(), n.to_owned())))
                    } else {
                        name.clone()
                    };
                    let draft = match country::only_entry(name, text, &e.id) {
                        Ok(d) => d,
                        Err(why) => {
                            push(&e.id, Outcome::Left(why));
                            continue;
                        }
                    };
                    let head = texts.get(&(kind.clone(), target.clone())).cloned();
                    match country::with_entry(head.as_deref(), &draft, &e.id) {
                        Ok(t) => {
                            texts.insert((kind.clone(), target.clone()), t);
                            changed.insert((kind.clone(), target.clone()));
                            push(&e.id, Outcome::Added(target));
                        }
                        Err(why) => push(&e.id, Outcome::Left(why)),
                    }
                }
            }
        }
    }
    let files = changed
        .into_iter()
        .map(|(k, n)| {
            let t = texts[&(k.clone(), n.clone())].clone();
            (k, n, t)
        })
        .collect();
    Ok((out, files))
}

/// `name` with `-2`, `-3`, … before its extension, the first `taken`
/// says is free.
fn free_file_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    let mut n = 2;
    loop {
        let candidate = format!("{stem}-{n}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// The one command of the join: the library files changed and the merged
/// line (one log entry, undone as one). Refused when the plan cannot be
/// applied.
pub fn command(session: &Session, user: &str, plan: &JoinPlan) -> Result<Command, SessionError> {
    if !plan.can_apply() {
        return Err(refused(format!(
            "the join is refused: {}",
            plan.blocked.join("; ")
        )));
    }
    let mut commands = session.library_puts(user, &plan.library_changes)?;
    commands.push(Command::ReplaceLine {
        line: Box::new(plan.line.clone()),
    });
    Ok(Command::Batch { commands })
}

/// Applies the join (one command, [`command`]) and records the revision
/// "joined routes from FILE" on the current project line.
pub fn join(
    session: &mut Session,
    user: &str,
    plan: &JoinPlan,
) -> Result<(Applied, Revision), SessionError> {
    let c = command(session, user, plan)?;
    let applied = session.apply(user, c)?;
    let revision = session.commit(user, &plan.message())?;
    Ok((applied, revision))
}
