// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Version control of the project (DESIGN_GOALS §7, ADR-0010).
//!
//! A **revision** is an immutable state of the project — the line's
//! objects (with their place among their kind) and the library files —
//! with its parents, author, time and message. States are stored by
//! content: every object, library text and **tree** (the list of a
//! state's objects and files with the hashes of their contents) is a
//! **blob** keyed by its SHA-256 and kept once (zstd), however many
//! revisions share it. A revision's id is the SHA-256 of its tree, parents,
//! author and message (not its time): the same in the server's file, in
//! every replica and in an offline copy.
//!
//! **Branches** are named heads (`main` first, empty until the first
//! commit); the **working state** — the session's line and libraries,
//! changed by logged commands — belongs to the current branch (the head).
//! A **tag** names a revision for good (e.g. `IFC rev B`).
//!
//! - [`Session::commit`] records the working state as a revision on the
//!   current branch.
//! - [`Session::checkout`] makes a branch (or a revision, detached) the
//!   working state: one logged, undoable command; refused while the
//!   working state has uncommitted changes.
//! - [`diff`] lists the objects and library files added, removed and
//!   changed between two states, field by field; [`crate::impact`] solves
//!   the touched sections on both sides.
//! - [`Session::merge`] merges another branch into the current one: by
//!   object and field against their common ancestor; changes that do not
//!   overlap merge by themselves, overlapping ones are listed as
//!   [`Conflict`]s and settled by choosing a side for each.
//! - [`Session::revert`] makes a new revision undoing another's changes.
//! - [`Session::export_bundle`] / [`Session::import_bundle`] carry a
//!   branch from an offline copy of the project back into it.
//!
//! Not under version control: point clouds, surfaces, project settings,
//! users and the operation log itself (each revision records the log
//! entry it was made after).
//!
//! Tables (schema 2):
//!
//! ```sql
//! vc_blob(hash PRIMARY KEY, data)                    -- zstd of the text
//! vc_revision(id PRIMARY KEY, parents, author, time, message, tree, seq)
//! vc_ref(name PRIMARY KEY, kind, revision, protected, user, time, note)
//! ols_meta('vc_head')                                -- branch:NAME or revision:ID
//! ```

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ols_line::Line;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Applied, Command, Kind, Object, Role, Session, SessionError, json, line_of, refused};

/// Tables of the version control.
pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS vc_blob (
        hash TEXT PRIMARY KEY, data BLOB NOT NULL);
    CREATE TABLE IF NOT EXISTS vc_revision (
        id TEXT PRIMARY KEY, parents TEXT NOT NULL, author TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        message TEXT NOT NULL, tree TEXT NOT NULL, seq INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS vc_ref (
        name TEXT PRIMARY KEY, kind TEXT NOT NULL, revision TEXT,
        protected INTEGER NOT NULL DEFAULT 0, user TEXT NOT NULL,
        time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        note TEXT NOT NULL DEFAULT '');";

/// The first branch of every project.
pub const MAIN: &str = "main";

/// zstd level of the blobs.
const LEVEL: i32 = 3;

/// Makes the branch `main` and the head when missing (new projects and
/// files of schema 1).
pub(crate) fn init(conn: &Connection) -> Result<(), SessionError> {
    conn.execute(
        "INSERT OR IGNORE INTO vc_ref (name, kind, revision, user) VALUES (?1, 'branch', NULL, '')",
        params![MAIN],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO ols_meta (key, value) VALUES ('vc_head', ?1)",
        params![format!("branch:{MAIN}")],
    )?;
    Ok(())
}

/// A state of the project under version control.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The line's objects by kind and id: place among their kind, JSON.
    pub objects: BTreeMap<(Kind, String), (i64, Value)>,
    /// Library files by kind and name: their text.
    pub libraries: BTreeMap<(String, String), String>,
}

impl State {
    /// The line of this state, validated.
    pub fn line(&self) -> Result<Line, SessionError> {
        let objects: BTreeMap<(Kind, String), Object> = self
            .objects
            .iter()
            .map(|(k, (ord, value))| {
                (
                    k.clone(),
                    Object {
                        ord: *ord,
                        version: 1,
                        value: value.clone(),
                    },
                )
            })
            .collect();
        line_of(&objects)
    }

    /// The texts of the library files of one kind, by name.
    pub fn library_files(&self, kind: &str) -> Vec<(&str, &str)> {
        self.libraries
            .iter()
            .filter(|((k, _), _)| k == kind)
            .map(|((_, n), t)| (n.as_str(), t.as_str()))
            .collect()
    }
}

/// A revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    /// Id: SHA-256 of tree, parents, author and message (hexadecimal).
    pub id: String,
    /// Parents: none for the first, two for a merge.
    pub parents: Vec<String>,
    /// Who made it.
    pub author: String,
    /// When (UTC).
    pub time: String,
    /// Why.
    pub message: String,
    /// Hash of its tree.
    pub tree: String,
    /// Last operation log entry when it was made.
    pub seq: i64,
}

impl Revision {
    /// The first ten characters of the id.
    pub fn short(&self) -> &str {
        short(&self.id)
    }
}

/// The first ten characters of a revision id.
pub fn short(id: &str) -> &str {
    id.get(..10).unwrap_or(id)
}

/// What the working state belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Head {
    /// A branch: commits move it.
    Branch(String),
    /// A revision checked out by id or tag: commits are refused.
    Detached(String),
}

/// Branch or tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    /// A branch (moves with commits).
    Branch,
    /// A tag (fixed).
    Tag,
}

/// A named branch or tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    /// Name.
    pub name: String,
    /// Branch or tag.
    pub kind: RefKind,
    /// The revision it names (`None`: a branch without commits yet).
    pub revision: Option<String>,
    /// Protected branch: changed only by admins or approved reviews.
    pub protected: bool,
    /// Who made it.
    pub user: String,
    /// When (UTC).
    pub time: String,
    /// Note (tags).
    pub note: String,
}

/// How an object or library file differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    /// Only in the second state.
    Added,
    /// Only in the first state.
    Removed,
    /// In both, different.
    Changed,
}

impl Change {
    /// `added`, `removed` or `changed`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
        }
    }
}

/// A field changed: its path in the object's JSON (`position`,
/// `stringing.tension`; `(order)`: its place among its kind).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    /// Path.
    pub path: String,
    /// Before (`None`: absent).
    pub before: Option<Value>,
    /// After (`None`: absent).
    pub after: Option<Value>,
}

/// An object that differs.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectChange {
    /// Kind.
    pub kind: Kind,
    /// Id.
    pub id: String,
    /// How.
    pub change: Change,
    /// Fields changed (changed objects).
    pub fields: Vec<FieldChange>,
}

/// A library file that differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryChange {
    /// Library kind (`cables`, `structures`…).
    pub kind: String,
    /// File name.
    pub name: String,
    /// How.
    pub change: Change,
    /// Lines only in the first / only in the second text.
    pub lines: (usize, usize),
}

/// Differences between two states.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Diff {
    /// Objects, by kind and id.
    pub objects: Vec<ObjectChange>,
    /// Library files, by kind and name.
    pub libraries: Vec<LibraryChange>,
}

impl Diff {
    /// Whether the states are the same.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.libraries.is_empty()
    }

    /// The objects that differ.
    pub fn keys(&self) -> Vec<(Kind, String)> {
        self.objects
            .iter()
            .map(|o| (o.kind, o.id.clone()))
            .collect()
    }

    /// In words: `3 objects changed, 1 added; 1 library file changed`.
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "no differences".into();
        }
        let count = |changes: &mut dyn Iterator<Item = Change>| {
            let mut n = BTreeMap::new();
            for c in changes {
                *n.entry(c.as_str()).or_insert(0usize) += 1;
            }
            n.iter()
                .map(|(c, n)| format!("{n} {c}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut parts = Vec::new();
        if !self.objects.is_empty() {
            parts.push(format!(
                "objects: {}",
                count(&mut self.objects.iter().map(|o| o.change))
            ));
        }
        if !self.libraries.is_empty() {
            parts.push(format!(
                "library files: {}",
                count(&mut self.libraries.iter().map(|l| l.change))
            ));
        }
        parts.join("; ")
    }
}

/// The working state compared with the head.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// The head.
    pub head: Head,
    /// The head's revision (`None`: no commit yet).
    pub revision: Option<String>,
    /// Uncommitted changes (head revision → working state).
    pub changes: Diff,
}

/// Which side of a merge to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The current branch's.
    Ours,
    /// The branch merged in.
    Theirs,
}

impl Side {
    /// Parses `ours` or `theirs`.
    pub fn parse(s: &str) -> Result<Self, SessionError> {
        match s {
            "ours" => Ok(Self::Ours),
            "theirs" => Ok(Self::Theirs),
            other => Err(refused(format!("\"{other}\": ours or theirs"))),
        }
    }
}

/// Choices settling conflicts: [`Conflict::key`] (or an object's
/// `kind:id`, for all its fields) → side.
pub type Choices = BTreeMap<String, Side>;

/// Both sides changed the same object (or field, or library file)
/// differently.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// Key to choose by: `structure:T3` (the whole object),
    /// `structure:T3/position` (a field), `library:cables/x.olsw`.
    pub key: String,
    /// In words.
    pub what: String,
    /// In the common ancestor (`None`: absent).
    pub base: Option<Value>,
    /// On the current branch.
    pub ours: Option<Value>,
    /// On the branch merged in.
    pub theirs: Option<Value>,
    /// The side chosen, if any.
    pub chosen: Option<Side>,
}

/// What a merge does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeKind {
    /// The other side is already contained: nothing to do.
    UpToDate,
    /// The current branch is behind: it moves to the other revision.
    FastForward,
    /// A merge revision with two parents.
    ThreeWay,
}

/// A merge worked out without changing anything.
#[derive(Debug, Clone, PartialEq)]
pub struct MergePlan {
    /// The current branch's revision.
    pub ours: Option<String>,
    /// The revision merged in.
    pub theirs: String,
    /// Their common ancestor.
    pub base: Option<String>,
    /// What the merge does.
    pub kind: MergeKind,
    /// Conflicts (with the choices given).
    pub conflicts: Vec<Conflict>,
    /// What the merge changes in the working state.
    pub changes: Diff,
    /// Why the merged line is not valid (all conflicts chosen), if so.
    pub problem: Option<String>,
    /// The merged state (unresolved conflicts: ours).
    pub merged: State,
}

impl MergePlan {
    /// Conflicts without a choice.
    pub fn unresolved(&self) -> Vec<&Conflict> {
        self.conflicts
            .iter()
            .filter(|c| c.chosen.is_none())
            .collect()
    }
}

/// What a merge did.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOutcome {
    /// What it did.
    pub kind: MergeKind,
    /// The merge revision (three-way merges).
    pub revision: Option<Revision>,
    /// The working state's change (a logged command), if any.
    pub applied: Option<Applied>,
    /// Library files changed in the working state.
    pub libraries_changed: bool,
}

/// What deleting a branch or tag drops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefDeletion {
    /// The branch or tag.
    pub name: String,
    /// Branch or tag.
    pub kind: RefKind,
    /// Its revision.
    pub revision: Option<String>,
    /// Revisions reachable only through it: they stay in the file, by id.
    pub only_here: usize,
    /// Open reviews of it, closed with it.
    pub reviews: Vec<i64>,
}

impl std::fmt::Display for RefDeletion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            RefKind::Branch => "branch",
            RefKind::Tag => "tag",
        };
        write!(
            f,
            "delete {kind} \"{}\" (at {})",
            self.name,
            self.revision.as_deref().map(short).unwrap_or("no revision")
        )?;
        if self.only_here > 0 {
            write!(
                f,
                "; {} revision(s) only on it stay in the file, reachable by id (a branch made at {} brings them back)",
                self.only_here,
                self.revision.as_deref().map(short).unwrap_or("")
            )?;
        }
        if !self.reviews.is_empty() {
            let ids: Vec<String> = self.reviews.iter().map(|r| format!("#{r}")).collect();
            write!(f, "; open reviews closed: {}", ids.join(", "))?;
        }
        Ok(())
    }
}

/// A branch carried between copies of a project (offline work).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    /// `ols-bundle`.
    pub format: String,
    /// 1.
    pub version: u32,
    /// The branch's name.
    pub branch: String,
    /// Its revision.
    pub head: String,
    /// Its revisions, parents first.
    pub revisions: Vec<Revision>,
    /// The blobs they use: hash → text.
    pub blobs: BTreeMap<String, String>,
}

/// Format id of bundles.
pub const BUNDLE_FORMAT: &str = "ols-bundle";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Tree {
    objects: Vec<(Kind, String, i64, String)>,
    libraries: Vec<(String, String, String)>,
}

/// JSON with the keys of every object sorted: the text hashed.
pub fn canonical(v: &Value) -> String {
    fn write(v: &Value, out: &mut String) {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                out.push('{');
                for (k, key) in keys.into_iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    out.push_str(&Value::String(key.clone()).to_string());
                    out.push(':');
                    write(&m[key], out);
                }
                out.push('}');
            }
            Value::Array(a) => {
                out.push('[');
                for (k, x) in a.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    write(x, out);
                }
                out.push(']');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut out = String::new();
    write(v, &mut out);
    out
}

/// SHA-256 of a text, hexadecimal.
pub fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The tree of a state and the blobs it needs (hash → text).
fn tree_of(state: &State) -> (Tree, BTreeMap<String, String>) {
    let mut blobs = BTreeMap::new();
    let mut tree = Tree::default();
    for ((kind, id), (ord, value)) in &state.objects {
        let text = canonical(value);
        let h = hash(&text);
        tree.objects.push((*kind, id.clone(), *ord, h.clone()));
        blobs.insert(h, text);
    }
    for ((kind, name), text) in &state.libraries {
        let h = hash(text);
        tree.libraries.push((kind.clone(), name.clone(), h.clone()));
        blobs.insert(h, text.clone());
    }
    (tree, blobs)
}

fn tree_text(tree: &Tree) -> String {
    canonical(&json(tree))
}

fn revision_id(tree: &str, parents: &[String], author: &str, message: &str) -> String {
    hash(&canonical(&serde_json::json!({
        "tree": tree,
        "parents": parents,
        "author": author,
        "message": message,
    })))
}

/// Paths of the leaves of a JSON value (objects recursed, arrays and
/// empty objects are leaves).
fn flatten(v: &Value) -> BTreeMap<Vec<String>, Value> {
    fn walk(v: &Value, path: &mut Vec<String>, out: &mut BTreeMap<Vec<String>, Value>) {
        match v {
            Value::Object(m) if !m.is_empty() => {
                for (k, x) in m {
                    path.push(k.clone());
                    walk(x, path, out);
                    path.pop();
                }
            }
            other => {
                out.insert(path.clone(), other.clone());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(v, &mut Vec::new(), &mut out);
    out
}

/// The value of leaves by path (`None`: the paths do not fit together).
fn unflatten(leaves: &BTreeMap<Vec<String>, Value>) -> Option<Value> {
    fn build(leaves: &[(&[String], &Value)]) -> Option<Value> {
        if let [(path, v)] = leaves
            && path.is_empty()
        {
            return Some((*v).clone());
        }
        let mut map = serde_json::Map::new();
        let mut k = 0;
        while k < leaves.len() {
            let (path, _) = leaves[k];
            let key = path.first()?;
            let group: Vec<(&[String], &Value)> = leaves[k..]
                .iter()
                .take_while(|(p, _)| p.first() == Some(key))
                .map(|(p, v)| (&p[1..], *v))
                .collect();
            k += group.len();
            // A leaf and deeper paths under the same key do not fit.
            if group.len() > 1 && group.iter().any(|(p, _)| p.is_empty()) {
                return None;
            }
            map.insert(key.clone(), build(&group)?);
        }
        Some(Value::Object(map))
    }
    let list: Vec<(&[String], &Value)> = leaves.iter().map(|(p, v)| (&p[..], v)).collect();
    if list.is_empty() {
        return Some(Value::Object(Default::default()));
    }
    build(&list)
}

fn path_text(path: &[String]) -> String {
    path.join(".")
}

/// Lines only in `a` and only in `b` (as multisets).
fn line_changes(a: &str, b: &str) -> (usize, usize) {
    let mut count: BTreeMap<&str, i64> = BTreeMap::new();
    for l in a.lines() {
        *count.entry(l).or_default() += 1;
    }
    for l in b.lines() {
        *count.entry(l).or_default() -= 1;
    }
    let removed = count
        .values()
        .filter(|n| **n > 0)
        .map(|n| *n as usize)
        .sum();
    let added = count
        .values()
        .filter(|n| **n < 0)
        .map(|n| (-*n) as usize)
        .sum();
    (removed, added)
}

/// The differences from state `a` to state `b`.
pub fn diff(a: &State, b: &State) -> Diff {
    let mut out = Diff::default();
    let keys: BTreeSet<&(Kind, String)> = a.objects.keys().chain(b.objects.keys()).collect();
    for key in keys {
        let (kind, id) = (key.0, key.1.clone());
        match (a.objects.get(key), b.objects.get(key)) {
            (None, Some(_)) => out.objects.push(ObjectChange {
                kind,
                id,
                change: Change::Added,
                fields: vec![],
            }),
            (Some(_), None) => out.objects.push(ObjectChange {
                kind,
                id,
                change: Change::Removed,
                fields: vec![],
            }),
            (Some((oa, va)), Some((ob, vb))) if oa != ob || va != vb => {
                let mut fields = Vec::new();
                if va != vb {
                    let (fa, fb) = (flatten(va), flatten(vb));
                    let paths: BTreeSet<&Vec<String>> = fa.keys().chain(fb.keys()).collect();
                    for p in paths {
                        let (x, y) = (fa.get(p), fb.get(p));
                        if x != y {
                            fields.push(FieldChange {
                                path: path_text(p),
                                before: x.cloned(),
                                after: y.cloned(),
                            });
                        }
                    }
                }
                if oa != ob {
                    fields.push(FieldChange {
                        path: "(order)".into(),
                        before: Some(Value::from(*oa)),
                        after: Some(Value::from(*ob)),
                    });
                }
                out.objects.push(ObjectChange {
                    kind,
                    id,
                    change: Change::Changed,
                    fields,
                });
            }
            _ => {}
        }
    }
    let names: BTreeSet<&(String, String)> = a.libraries.keys().chain(b.libraries.keys()).collect();
    for key in names {
        let (ta, tb) = (a.libraries.get(key), b.libraries.get(key));
        let change = match (ta, tb) {
            (None, Some(_)) => Change::Added,
            (Some(_), None) => Change::Removed,
            (Some(x), Some(y)) if x != y => Change::Changed,
            _ => continue,
        };
        out.libraries.push(LibraryChange {
            kind: key.0.clone(),
            name: key.1.clone(),
            change,
            lines: line_changes(ta.map_or("", |t| t), tb.map_or("", |t| t)),
        });
    }
    out
}

fn object_key(kind: Kind, id: &str) -> String {
    format!("{}:{id}", kind.as_str())
}

/// Three-way merge of `ours` and `theirs` from `base`, by object and field;
/// conflicts settled by `choices` (unsettled: ours kept).
pub fn merge_states(
    base: &State,
    ours: &State,
    theirs: &State,
    choices: &Choices,
) -> (State, Vec<Conflict>) {
    let mut merged = State::default();
    let mut conflicts = Vec::new();
    let keys: BTreeSet<&(Kind, String)> = base
        .objects
        .keys()
        .chain(ours.objects.keys())
        .chain(theirs.objects.keys())
        .collect();
    for key in keys {
        let (b, o, t) = (
            base.objects.get(key),
            ours.objects.get(key),
            theirs.objects.get(key),
        );
        let (bv, ov, tv) = (b.map(|x| &x.1), o.map(|x| &x.1), t.map(|x| &x.1));
        let okey = object_key(key.0, &key.1);
        let what = format!("{} {}", key.0.as_str(), key.1);
        // The place: ours, unless only theirs moved it.
        let ord = |o: Option<&(i64, Value)>, t: Option<&(i64, Value)>| match (o, t) {
            (Some(o), Some(t)) if b.map(|b| b.0) == Some(o.0) => t.0,
            (Some(o), _) => o.0,
            (None, Some(t)) => t.0,
            (None, None) => 0,
        };
        let taken: Option<(i64, Value)> = if ov == tv {
            o.or(t)
                .map(|_| (ord(o, t), ov.cloned().expect("same, present")))
        } else if bv == ov {
            t.cloned()
        } else if bv == tv {
            o.cloned()
        } else if let (Some(bv), Some(ov), Some(tv)) = (bv, ov, tv) {
            match merge_fields(&okey, &what, bv, ov, tv, choices, &mut conflicts) {
                Some(v) => Some((ord(o, t), v)),
                None => whole_conflict(&okey, &what, b, o, t, choices, &mut conflicts),
            }
        } else {
            whole_conflict(&okey, &what, b, o, t, choices, &mut conflicts)
        };
        if let Some(x) = taken {
            merged.objects.insert(key.clone(), x);
        }
    }
    let names: BTreeSet<&(String, String)> = base
        .libraries
        .keys()
        .chain(ours.libraries.keys())
        .chain(theirs.libraries.keys())
        .collect();
    for key in names {
        let (b, o, t) = (
            base.libraries.get(key),
            ours.libraries.get(key),
            theirs.libraries.get(key),
        );
        let taken = if o == t || b == t {
            o
        } else if b == o {
            t
        } else {
            let ckey = format!("library:{}/{}", key.0, key.1);
            let lines = |x: Option<&String>| {
                x.map(|x| Value::String(format!("{} lines", x.lines().count())))
            };
            let chosen = choices.get(&ckey).copied();
            conflicts.push(Conflict {
                key: ckey,
                what: format!("library file {}/{}", key.0, key.1),
                base: lines(b),
                ours: lines(o),
                theirs: lines(t),
                chosen,
            });
            match chosen {
                Some(Side::Theirs) => t,
                _ => o,
            }
        };
        if let Some(text) = taken {
            merged.libraries.insert(key.clone(), text.clone());
        }
    }
    (merged, conflicts)
}

fn whole_conflict(
    okey: &str,
    what: &str,
    b: Option<&(i64, Value)>,
    o: Option<&(i64, Value)>,
    t: Option<&(i64, Value)>,
    choices: &Choices,
    conflicts: &mut Vec<Conflict>,
) -> Option<(i64, Value)> {
    let chosen = choices.get(okey).copied();
    conflicts.push(Conflict {
        key: okey.to_owned(),
        what: what.to_owned(),
        base: b.map(|x| x.1.clone()),
        ours: o.map(|x| x.1.clone()),
        theirs: t.map(|x| x.1.clone()),
        chosen,
    });
    match chosen {
        Some(Side::Theirs) => t.cloned(),
        _ => o.cloned(),
    }
}

/// Field-level merge of one object changed on both sides (`None`: the
/// fields do not fit together; the object is then one conflict).
fn merge_fields(
    okey: &str,
    what: &str,
    b: &Value,
    o: &Value,
    t: &Value,
    choices: &Choices,
    conflicts: &mut Vec<Conflict>,
) -> Option<Value> {
    let (fb, fo, ft) = (flatten(b), flatten(o), flatten(t));
    let paths: BTreeSet<&Vec<String>> = fb.keys().chain(fo.keys()).chain(ft.keys()).collect();
    let mut leaves = BTreeMap::new();
    let mut found = Vec::new();
    for p in paths {
        let (bv, ov, tv) = (fb.get(p), fo.get(p), ft.get(p));
        let pick = if ov == tv || bv == tv {
            ov
        } else if bv == ov {
            tv
        } else {
            let key = format!("{okey}/{}", path_text(p));
            let chosen = choices.get(&key).or_else(|| choices.get(okey)).copied();
            found.push(Conflict {
                key,
                what: format!("{what} {}", path_text(p)),
                base: bv.cloned(),
                ours: ov.cloned(),
                theirs: tv.cloned(),
                chosen,
            });
            match chosen {
                Some(Side::Theirs) => tv,
                _ => ov,
            }
        };
        if let Some(v) = pick {
            leaves.insert(p.clone(), v.clone());
        }
    }
    let merged = unflatten(&leaves)?;
    conflicts.extend(found);
    Some(merged)
}

fn store(e: impl std::fmt::Display) -> SessionError {
    SessionError::Store(e.to_string())
}

impl Session {
    fn write_blob(conn: &Connection, h: &str, text: &str) -> Result<(), SessionError> {
        let exists = conn
            .prepare_cached("SELECT 1 FROM vc_blob WHERE hash = ?1")?
            .exists(params![h])?;
        if !exists {
            let data = zstd::encode_all(text.as_bytes(), LEVEL).map_err(store)?;
            conn.execute(
                "INSERT INTO vc_blob (hash, data) VALUES (?1, ?2)",
                params![h, data],
            )?;
        }
        Ok(())
    }

    fn read_blob(&self, h: &str) -> Result<String, SessionError> {
        let data: Option<Vec<u8>> = self
            .conn
            .prepare_cached("SELECT data FROM vc_blob WHERE hash = ?1")?
            .query_row(params![h], |r| r.get(0))
            .optional()?;
        let data = data.ok_or_else(|| store(format!("version store: blob {h} missing")))?;
        let raw = zstd::decode_all(data.as_slice()).map_err(store)?;
        String::from_utf8(raw).map_err(store)
    }

    /// The working state: the session's line and the library files.
    pub fn working_state(&self) -> Result<State, SessionError> {
        let mut state = State {
            objects: self
                .objects
                .iter()
                .map(|(k, o)| (k.clone(), (o.ord, o.value.clone())))
                .collect(),
            libraries: BTreeMap::new(),
        };
        let mut stmt = self
            .conn
            .prepare("SELECT kind, name, text FROM library_file")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (kind, name, text) = row?;
            state.libraries.insert((kind, name), text);
        }
        Ok(state)
    }

    /// The state of revision `id`. Reads one blob per object (about
    /// 10 µs each).
    pub fn revision_state(&self, id: &str) -> Result<State, SessionError> {
        let rev = self.revision(id)?;
        let tree: Tree = serde_json::from_str(&self.read_blob(&rev.tree)?).map_err(store)?;
        let mut state = State::default();
        for (kind, id, ord, h) in tree.objects {
            let value: Value = serde_json::from_str(&self.read_blob(&h)?).map_err(store)?;
            state.objects.insert((kind, id), (ord, value));
        }
        for (kind, name, h) in tree.libraries {
            state.libraries.insert((kind, name), self.read_blob(&h)?);
        }
        Ok(state)
    }

    fn revision_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(Revision, String)> {
        Ok((
            Revision {
                id: r.get(0)?,
                parents: vec![],
                author: r.get(2)?,
                time: r.get(3)?,
                message: r.get(4)?,
                tree: r.get(5)?,
                seq: r.get(6)?,
            },
            r.get(1)?,
        ))
    }

    /// Revision `id` (the full id).
    pub fn revision(&self, id: &str) -> Result<Revision, SessionError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, parents, author, time, message, tree, seq FROM vc_revision WHERE id = ?1",
                params![id],
                Self::revision_row,
            )
            .optional()?
            .ok_or_else(|| refused(format!("no revision {id}")))?;
        let (mut rev, parents) = row;
        rev.parents = serde_json::from_str(&parents).map_err(store)?;
        Ok(rev)
    }

    /// Every revision, newest first.
    pub fn revisions(&self) -> Result<Vec<Revision>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parents, author, time, message, tree, seq FROM vc_revision ORDER BY rowid DESC",
        )?;
        let rows = stmt.query_map([], Self::revision_row)?;
        rows.map(|r| {
            let (mut rev, parents) = r?;
            rev.parents = serde_json::from_str(&parents).map_err(store)?;
            Ok(rev)
        })
        .collect()
    }

    /// The branches and tags, by name.
    pub fn refs(&self) -> Result<Vec<Ref>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kind, revision, protected, user, time, note FROM vc_ref ORDER BY kind, name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        rows.map(|r| {
            let (name, kind, revision, protected, user, time, note) = r?;
            Ok(Ref {
                name,
                kind: if kind == "tag" {
                    RefKind::Tag
                } else {
                    RefKind::Branch
                },
                revision,
                protected,
                user,
                time,
                note,
            })
        })
        .collect()
    }

    fn find_ref(&self, name: &str) -> Result<Option<Ref>, SessionError> {
        Ok(self.refs()?.into_iter().find(|r| r.name == name))
    }

    /// Branch `name`'s revision (`None`: no commit yet); refused if there
    /// is no such branch.
    pub fn branch_revision(&self, name: &str) -> Result<Option<String>, SessionError> {
        match self.find_ref(name)? {
            Some(r) if r.kind == RefKind::Branch => Ok(r.revision),
            _ => Err(refused(format!("no branch \"{name}\""))),
        }
    }

    /// What the working state belongs to.
    pub fn head(&self) -> Result<Head, SessionError> {
        let v: String = self
            .conn
            .query_row(
                "SELECT value FROM ols_meta WHERE key = 'vc_head'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| format!("branch:{MAIN}"));
        Ok(match v.split_once(':') {
            Some(("revision", id)) => Head::Detached(id.to_owned()),
            Some((_, b)) => Head::Branch(b.to_owned()),
            None => Head::Branch(v),
        })
    }

    fn set_head(&self, head: &Head) -> Result<(), SessionError> {
        let v = match head {
            Head::Branch(b) => format!("branch:{b}"),
            Head::Detached(id) => format!("revision:{id}"),
        };
        self.conn.execute(
            "INSERT INTO ols_meta (key, value) VALUES ('vc_head', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![v],
        )?;
        Ok(())
    }

    /// The head's revision (`None`: no commit yet).
    pub fn head_revision(&self) -> Result<Option<String>, SessionError> {
        match self.head()? {
            Head::Branch(b) => self.branch_revision(&b),
            Head::Detached(id) => Ok(Some(id)),
        }
    }

    /// The revision named `name`: a branch, a tag, `HEAD`, or a revision
    /// id or its start (at least 4 characters, unique).
    pub fn resolve_revision(&self, name: &str) -> Result<String, SessionError> {
        let name = name.trim();
        if name.eq_ignore_ascii_case("head") {
            return self
                .head_revision()?
                .ok_or_else(|| refused("no revision yet: commit first"));
        }
        if let Some(r) = self.find_ref(name)? {
            return r
                .revision
                .ok_or_else(|| refused(format!("branch \"{name}\" has no revision yet")));
        }
        if name.len() >= 4 && name.chars().all(|c| c.is_ascii_hexdigit()) {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM vc_revision WHERE id LIKE ?1 || '%' LIMIT 2")?;
            let found: Vec<String> = stmt
                .query_map(params![name.to_ascii_lowercase()], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            match found.len() {
                1 => return Ok(found[0].clone()),
                0 => {}
                _ => return Err(refused(format!("\"{name}\" names several revisions"))),
            }
        }
        Err(refused(format!("no branch, tag or revision \"{name}\"")))
    }

    /// The working state compared with the head.
    pub fn status(&self) -> Result<Status, SessionError> {
        let head = self.head()?;
        let revision = self.head_revision()?;
        let working = self.working_state()?;
        let changes = match &revision {
            Some(id) => self.diff_to(id, &working)?,
            None => diff(&State::default(), &working),
        };
        Ok(Status {
            head,
            revision,
            changes,
        })
    }

    /// The differences from revision `id` to `state`, reading only the
    /// objects and files whose hashes differ (hashing the state: about
    /// 2 µs per object).
    pub fn diff_to(&self, id: &str, state: &State) -> Result<Diff, SessionError> {
        let rev = self.revision(id)?;
        let (now, _) = tree_of(state);
        if hash(&tree_text(&now)) == rev.tree {
            return Ok(Diff::default());
        }
        let then: Tree = serde_json::from_str(&self.read_blob(&rev.tree)?).map_err(store)?;
        let index = |t: &Tree| -> BTreeMap<(Kind, String), (i64, String)> {
            t.objects
                .iter()
                .map(|(k, id, ord, h)| ((*k, id.clone()), (*ord, h.clone())))
                .collect()
        };
        let files = |t: &Tree| -> BTreeMap<(String, String), String> {
            t.libraries
                .iter()
                .map(|(k, n, h)| ((k.clone(), n.clone()), h.clone()))
                .collect()
        };
        // Only what differs, on both sides: their diff is the whole diff.
        let (mut a, mut b) = (State::default(), State::default());
        let (ia, ib) = (index(&then), index(&now));
        for key in ia.keys().chain(ib.keys()) {
            if ia.get(key) == ib.get(key) || a.objects.contains_key(key) {
                continue;
            }
            if let Some((ord, h)) = ia.get(key) {
                let value = serde_json::from_str(&self.read_blob(h)?).map_err(store)?;
                a.objects.insert(key.clone(), (*ord, value));
            }
            if let Some(x) = state.objects.get(key) {
                b.objects.insert(key.clone(), x.clone());
            }
        }
        let (fa, fb) = (files(&then), files(&now));
        for key in fa.keys().chain(fb.keys()) {
            if fa.get(key) == fb.get(key) {
                continue;
            }
            if let Some(h) = fa.get(key) {
                a.libraries.insert(key.clone(), self.read_blob(h)?);
            }
            if let Some(t) = state.libraries.get(key) {
                b.libraries.insert(key.clone(), t.clone());
            }
        }
        Ok(diff(&a, &b))
    }

    fn require_clean(&self) -> Result<(), SessionError> {
        let status = self.status()?;
        if !status.changes.is_empty() {
            return Err(refused(format!(
                "the working state has uncommitted changes ({}): commit them first",
                status.changes.summary()
            )));
        }
        Ok(())
    }

    fn current_branch(&self) -> Result<String, SessionError> {
        match self.head()? {
            Head::Branch(b) => Ok(b),
            Head::Detached(id) => Err(refused(format!(
                "revision {} is checked out, not a branch: make a branch there first",
                short(&id)
            ))),
        }
    }

    /// Refuses changes of a protected branch by anyone but an admin
    /// (unless through an approved review).
    pub(crate) fn check_branch_write(
        &self,
        user: &str,
        branch: &str,
        approved: bool,
    ) -> Result<(), SessionError> {
        let protected = self.find_ref(branch)?.is_some_and(|r| r.protected);
        if protected && !approved && self.role(user)? != Some(Role::Admin) {
            return Err(refused(format!(
                "branch \"{branch}\" is protected: its changes come through an approved review, or from an admin"
            )));
        }
        Ok(())
    }

    /// Writes a revision of `state` and moves `branch` to it.
    fn write_revision(
        &self,
        user: &str,
        state: &State,
        parents: Vec<String>,
        message: &str,
        branch: Option<&str>,
    ) -> Result<Revision, SessionError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(refused("a revision needs a message"));
        }
        let (tree, blobs) = tree_of(state);
        let text = tree_text(&tree);
        let tree_hash = hash(&text);
        if let [parent] = &parents[..]
            && self.revision(parent)?.tree == tree_hash
        {
            return Err(refused(format!(
                "nothing to commit: the working state is that of revision {}",
                short(parent)
            )));
        }
        let id = revision_id(&tree_hash, &parents, user, message);
        let exists = self
            .conn
            .prepare("SELECT 1 FROM vc_revision WHERE id = ?1")?
            .exists(params![id])?;
        if exists {
            return Err(refused(format!("revision {} exists already", short(&id))));
        }
        let seq = self.last_seq()?;
        let tx = self.conn.unchecked_transaction()?;
        for (h, t) in &blobs {
            Self::write_blob(&tx, h, t)?;
        }
        Self::write_blob(&tx, &tree_hash, &text)?;
        tx.execute(
            "INSERT INTO vc_revision (id, parents, author, message, tree, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                serde_json::to_string(&parents).expect("serialises"),
                user,
                message,
                tree_hash,
                seq
            ],
        )?;
        if let Some(b) = branch {
            tx.execute(
                "UPDATE vc_ref SET revision = ?1 WHERE name = ?2 AND kind = 'branch'",
                params![id, b],
            )?;
        }
        tx.commit()?;
        self.revision(&id)
    }

    /// Records the working state as a revision on the current branch.
    pub fn commit(&mut self, user: &str, message: &str) -> Result<Revision, SessionError> {
        self.require(user, Role::Designer)?;
        let branch = self.current_branch()?;
        self.check_branch_write(user, &branch, false)?;
        if !self.has_line() {
            return Err(refused("the project holds no line yet"));
        }
        let parent = self.branch_revision(&branch)?;
        let state = self.working_state()?;
        self.write_revision(
            user,
            &state,
            parent.into_iter().collect(),
            message,
            Some(&branch),
        )
    }

    fn check_ref_name(&self, name: &str) -> Result<String, SessionError> {
        let name = name.trim();
        let ok = !name.is_empty()
            && name.len() <= 100
            && !name.eq_ignore_ascii_case("head")
            && !name.contains(':')
            && !name.chars().any(char::is_control);
        if !ok {
            return Err(refused(format!(
                "\"{name}\": a branch or tag name of 1 to 100 characters, without ':' (and not HEAD)"
            )));
        }
        if self.find_ref(name)?.is_some() {
            return Err(refused(format!(
                "a branch or tag \"{name}\" exists already"
            )));
        }
        Ok(name.to_owned())
    }

    /// Makes branch `name` at revision `at` (default: the head's). The
    /// working state stays where it is: check the branch out to work on it.
    pub fn create_branch(
        &self,
        user: &str,
        name: &str,
        at: Option<&str>,
    ) -> Result<String, SessionError> {
        self.require(user, Role::Designer)?;
        let name = self.check_ref_name(name)?;
        let rev = match at {
            Some(a) => self.resolve_revision(a)?,
            None => self
                .head_revision()?
                .ok_or_else(|| refused("no revision yet: commit first"))?,
        };
        self.conn.execute(
            "INSERT INTO vc_ref (name, kind, revision, user) VALUES (?1, 'branch', ?2, ?3)",
            params![name, rev, user],
        )?;
        Ok(rev)
    }

    /// Names revision `at` (default: the head's) for good, e.g. an issued
    /// revision.
    pub fn tag(
        &self,
        user: &str,
        name: &str,
        at: Option<&str>,
        note: &str,
    ) -> Result<String, SessionError> {
        self.require(user, Role::Designer)?;
        let name = self.check_ref_name(name)?;
        let rev = match at {
            Some(a) => self.resolve_revision(a)?,
            None => self
                .head_revision()?
                .ok_or_else(|| refused("no revision yet: commit first"))?,
        };
        self.conn.execute(
            "INSERT INTO vc_ref (name, kind, revision, user, note) VALUES (?1, 'tag', ?2, ?3, ?4)",
            params![name, rev, user, note.trim()],
        )?;
        Ok(rev)
    }

    /// Admin: protects branch `name` (or lifts the protection): a
    /// protected branch changes only through admins or approved reviews.
    pub fn protect_branch(
        &self,
        admin: &str,
        name: &str,
        protected: bool,
    ) -> Result<(), SessionError> {
        self.require(admin, Role::Admin)?;
        self.branch_revision(name)?;
        self.conn.execute(
            "UPDATE vc_ref SET protected = ?1 WHERE name = ?2 AND kind = 'branch'",
            params![protected, name],
        )?;
        Ok(())
    }

    /// Ancestors of `id` (with itself) by distance.
    pub(crate) fn ancestors(&self, id: &str) -> Result<BTreeMap<String, usize>, SessionError> {
        let parents: BTreeMap<String, Vec<String>> = self
            .revisions()?
            .into_iter()
            .map(|r| (r.id, r.parents))
            .collect();
        let mut out = BTreeMap::new();
        let mut queue = VecDeque::from([(id.to_owned(), 0usize)]);
        // Each revision is visited once: at most the number of revisions.
        while let Some((r, d)) = queue.pop_front() {
            if out.contains_key(&r) {
                continue;
            }
            for p in parents.get(&r).into_iter().flatten() {
                queue.push_back((p.clone(), d + 1));
            }
            out.insert(r, d);
        }
        Ok(out)
    }

    /// The nearest common ancestor of two revisions.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>, SessionError> {
        let of_a = self.ancestors(a)?;
        let of_b = self.ancestors(b)?;
        Ok(of_b
            .iter()
            .filter(|(r, _)| of_a.contains_key(*r))
            .min_by_key(|(r, d)| (**d + of_a[*r], (*r).clone()))
            .map(|(r, _)| r.clone()))
    }

    /// What deleting branch or tag `name` drops (nothing is changed).
    pub fn ref_deletion(&self, name: &str) -> Result<RefDeletion, SessionError> {
        let r = self
            .find_ref(name)?
            .ok_or_else(|| refused(format!("no branch or tag \"{name}\"")))?;
        let mut only_here = 0;
        if let Some(rev) = &r.revision {
            let mut elsewhere = BTreeSet::new();
            for other in self.refs()? {
                if other.name != r.name
                    && let Some(o) = &other.revision
                {
                    elsewhere.extend(self.ancestors(o)?.into_keys());
                }
            }
            only_here = self
                .ancestors(rev)?
                .keys()
                .filter(|k| !elsewhere.contains(*k))
                .count();
        }
        Ok(RefDeletion {
            name: r.name.clone(),
            kind: r.kind,
            revision: r.revision.clone(),
            only_here,
            reviews: self
                .reviews()?
                .into_iter()
                .filter(|v| {
                    v.state == crate::collab::ReviewState::Open
                        && (v.branch == r.name || v.target == r.name)
                })
                .map(|v| v.id)
                .collect(),
        })
    }

    /// Deletes branch or tag `name`: refused without `confirm` (the
    /// refusal lists what goes with it), and for the current branch.
    /// Revisions stay in the file: a branch or tag made at the listed
    /// revision brings it back. Tags and protected branches: admins only.
    pub fn delete_ref(
        &self,
        user: &str,
        name: &str,
        confirm: bool,
    ) -> Result<RefDeletion, SessionError> {
        self.require(user, Role::Designer)?;
        let d = self.ref_deletion(name)?;
        let protected = self.find_ref(name)?.is_some_and(|r| r.protected);
        if d.kind == RefKind::Tag || protected {
            self.require(user, Role::Admin)?;
        }
        if self.head()? == Head::Branch(name.to_owned()) {
            return Err(refused(format!(
                "\"{name}\" is the current branch: check out another first"
            )));
        }
        if !confirm {
            return Err(refused(format!("{d} — repeat with confirmation to apply")));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM vc_ref WHERE name = ?1", params![name])?;
        for id in &d.reviews {
            tx.execute(
                "UPDATE vc_review SET state = 'closed' WHERE id = ?1",
                params![id],
            )?;
        }
        tx.commit()?;
        Ok(d)
    }

    /// Makes `target` the working state: one logged, undoable command for
    /// the line; library files replaced (not logged, as all library
    /// changes). Returns the command applied (`None`: the line did not
    /// change) and whether library files changed.
    pub(crate) fn set_working(
        &mut self,
        user: &str,
        target: &State,
        note: &str,
    ) -> Result<(Option<Applied>, bool), SessionError> {
        target.line()?;
        let mut prims = Vec::new();
        for (key, (ord, value)) in &target.objects {
            match self.objects.get(key) {
                Some(o) if o.value == *value && o.ord == *ord => {}
                _ => prims.push(Command::Put {
                    kind: key.0,
                    id: key.1.clone(),
                    value: value.clone(),
                    ord: Some(*ord),
                }),
            }
        }
        for key in self.objects.keys() {
            if !target.objects.contains_key(key) {
                prims.push(Command::Delete {
                    kind: key.0,
                    id: key.1.clone(),
                    confirm: false,
                });
            }
        }
        let applied = if prims.is_empty() {
            None
        } else {
            let a = self.apply_as(
                user,
                Command::Batch { commands: prims },
                "do",
                None,
                false,
                false,
            )?;
            self.conn.execute(
                "UPDATE oplog SET note = ?1 WHERE seq = ?2",
                params![note, a.seq],
            )?;
            Some(a)
        };
        let current = self.working_state()?.libraries;
        let changed = current != target.libraries;
        if changed {
            let tx = self.conn.unchecked_transaction()?;
            for (kind, name) in current.keys() {
                if !target.libraries.contains_key(&(kind.clone(), name.clone())) {
                    tx.execute(
                        "DELETE FROM library_file WHERE kind = ?1 AND name = ?2",
                        params![kind, name],
                    )?;
                }
            }
            for ((kind, name), text) in &target.libraries {
                if current.get(&(kind.clone(), name.clone())) != Some(text) {
                    tx.execute(
                        "INSERT INTO library_file (kind, name, user, text) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(kind, name) DO UPDATE SET text = excluded.text, user = excluded.user,
                             time = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                        params![kind, name, user, text],
                    )?;
                }
            }
            tx.commit()?;
        }
        Ok((applied, changed))
    }

    /// Makes branch `target` (or a tag or revision: detached, no commits
    /// there) the working state, as one logged, undoable command. Refused
    /// while the working state has uncommitted changes, and for users whose
    /// edits are limited ([`crate::collab`]). Returns the line's change
    /// (`None`: the line is the same) and whether library files changed.
    ///
    /// In server mode every user shares the working state: a checkout
    /// switches it for all.
    pub fn checkout(
        &mut self,
        user: &str,
        target: &str,
    ) -> Result<(Option<Applied>, bool), SessionError> {
        self.require(user, Role::Designer)?;
        self.require_unrestricted(user)?;
        self.require_clean()?;
        let target = target.trim();
        let (head, rev) = match self.find_ref(target)? {
            Some(r) if r.kind == RefKind::Branch => (Head::Branch(r.name), r.revision),
            _ => {
                let id = self.resolve_revision(target)?;
                (Head::Detached(id.clone()), Some(id))
            }
        };
        let out = match &rev {
            Some(id) => {
                let state = self.revision_state(id)?;
                self.set_working(user, &state, &format!("check out {target} ({})", short(id)))?
            }
            None => (None, false),
        };
        self.set_head(&head)?;
        Ok(out)
    }

    /// Works out merging `other` (branch, tag or revision) into the current
    /// branch with `choices`, changing nothing.
    pub fn merge_plan(&self, other: &str, choices: &Choices) -> Result<MergePlan, SessionError> {
        let theirs = self.resolve_revision(other)?;
        let ours = self.head_revision()?;
        let working = self.working_state()?;
        let mut plan = MergePlan {
            ours: ours.clone(),
            theirs: theirs.clone(),
            base: None,
            kind: MergeKind::UpToDate,
            conflicts: vec![],
            changes: Diff::default(),
            problem: None,
            merged: working.clone(),
        };
        let Some(ours) = ours else {
            plan.kind = MergeKind::FastForward;
            plan.merged = self.revision_state(&theirs)?;
            plan.changes = diff(&working, &plan.merged);
            plan.problem = plan.merged.line().err().map(|e| e.to_string());
            return Ok(plan);
        };
        let ours_ancestors = self.ancestors(&ours)?;
        if ours_ancestors.contains_key(&theirs) {
            return Ok(plan);
        }
        let base = self.merge_base(&ours, &theirs)?;
        plan.base = base.clone();
        if base.as_deref() == Some(ours.as_str()) {
            plan.kind = MergeKind::FastForward;
            plan.merged = self.revision_state(&theirs)?;
        } else {
            plan.kind = MergeKind::ThreeWay;
            let base_state = match &base {
                Some(b) => self.revision_state(b)?,
                None => State::default(),
            };
            let ours_state = self.revision_state(&ours)?;
            let (merged, conflicts) = merge_states(
                &base_state,
                &ours_state,
                &self.revision_state(&theirs)?,
                choices,
            );
            plan.merged = merged;
            plan.conflicts = conflicts;
        }
        plan.changes = diff(&working, &plan.merged);
        if plan.unresolved().is_empty() {
            plan.problem = plan.merged.line().err().map(|e| e.to_string());
        }
        Ok(plan)
    }

    /// Merges `other` into the current branch: fast-forward when the
    /// branch is behind, else a merge revision with two parents (message:
    /// `message`, or `Merge OTHER into BRANCH`). Refused with uncommitted
    /// changes, unsettled conflicts (listed) or an invalid merged line.
    pub fn merge(
        &mut self,
        user: &str,
        other: &str,
        message: Option<&str>,
        choices: &Choices,
    ) -> Result<MergeOutcome, SessionError> {
        self.merge_as(user, other, message, choices, false)
    }

    pub(crate) fn merge_as(
        &mut self,
        user: &str,
        other: &str,
        message: Option<&str>,
        choices: &Choices,
        approved: bool,
    ) -> Result<MergeOutcome, SessionError> {
        self.require(user, Role::Designer)?;
        self.require_unrestricted(user)?;
        let branch = self.current_branch()?;
        self.check_branch_write(user, &branch, approved)?;
        self.require_clean()?;
        let plan = self.merge_plan(other, choices)?;
        if plan.kind == MergeKind::UpToDate {
            return Err(refused(format!(
                "branch \"{branch}\" already contains {other}"
            )));
        }
        let unresolved = plan.unresolved();
        if !unresolved.is_empty() {
            let list: Vec<String> = unresolved.iter().map(|c| c.key.clone()).collect();
            return Err(refused(format!(
                "{} conflict(s) to settle (choose ours or theirs for each): {}",
                list.len(),
                list.join(", ")
            )));
        }
        if let Some(p) = &plan.problem {
            return Err(refused(format!("the merged line is not valid: {p}")));
        }
        let note = format!("merge {other} ({}) into {branch}", short(&plan.theirs));
        let (applied, libraries_changed) = self.set_working(user, &plan.merged, &note)?;
        let revision = match plan.kind {
            MergeKind::FastForward => {
                self.conn.execute(
                    "UPDATE vc_ref SET revision = ?1 WHERE name = ?2 AND kind = 'branch'",
                    params![plan.theirs, branch],
                )?;
                None
            }
            _ => {
                let message = message
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Merge {other} into {branch}"));
                let parents = vec![plan.ours.clone().expect("three-way"), plan.theirs.clone()];
                let state = self.working_state()?;
                Some(self.write_revision(user, &state, parents, &message, Some(&branch))?)
            }
        };
        Ok(MergeOutcome {
            kind: plan.kind,
            revision,
            applied,
            libraries_changed,
        })
    }

    /// Makes a new revision on the current branch undoing the changes of
    /// `revision` (against its first parent), merged three-way with what
    /// came after it; conflicts settled by `choices`.
    pub fn revert(
        &mut self,
        user: &str,
        revision: &str,
        message: Option<&str>,
        choices: &Choices,
    ) -> Result<MergeOutcome, SessionError> {
        self.require(user, Role::Designer)?;
        self.require_unrestricted(user)?;
        let branch = self.current_branch()?;
        self.check_branch_write(user, &branch, false)?;
        self.require_clean()?;
        let id = self.resolve_revision(revision)?;
        let rev = self.revision(&id)?;
        let parent = rev.parents.first().ok_or_else(|| {
            refused(format!(
                "revision {} is the first: there is nothing before it to go back to",
                rev.short()
            ))
        })?;
        let head = self
            .head_revision()?
            .ok_or_else(|| refused("no revision yet"))?;
        let working = self.working_state()?;
        let (merged, conflicts) = merge_states(
            &self.revision_state(&id)?,
            &working,
            &self.revision_state(parent)?,
            choices,
        );
        let unresolved: Vec<String> = conflicts
            .iter()
            .filter(|c| c.chosen.is_none())
            .map(|c| c.key.clone())
            .collect();
        if !unresolved.is_empty() {
            return Err(refused(format!(
                "changed again since {}: {} conflict(s) to settle: {}",
                rev.short(),
                unresolved.len(),
                unresolved.join(", ")
            )));
        }
        if merged == working {
            return Err(refused(format!(
                "nothing to revert: the changes of {} are not in the working state",
                rev.short()
            )));
        }
        merged
            .line()
            .map_err(|e| refused(format!("the line without {}: {e}", rev.short())))?;
        let note = format!("revert {}", rev.short());
        let (applied, libraries_changed) = self.set_working(user, &merged, &note)?;
        let message = message
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Revert {}: {}", rev.short(), rev.message));
        let state = self.working_state()?;
        let revision = self.write_revision(user, &state, vec![head], &message, Some(&branch))?;
        Ok(MergeOutcome {
            kind: MergeKind::ThreeWay,
            revision: Some(revision),
            applied,
            libraries_changed,
        })
    }

    /// Branch `branch` with all its revisions and their blobs, to carry to
    /// another copy of the project ([`Session::import_bundle`]).
    pub fn export_bundle(&self, branch: &str) -> Result<Bundle, SessionError> {
        let head = self
            .branch_revision(branch)?
            .ok_or_else(|| refused(format!("branch \"{branch}\" has no revision")))?;
        let ancestors = self.ancestors(&head)?;
        let mut revisions: Vec<Revision> = self
            .revisions()?
            .into_iter()
            .filter(|r| ancestors.contains_key(&r.id))
            .collect();
        revisions.reverse();
        let mut blobs = BTreeMap::new();
        for r in &revisions {
            let text = self.read_blob(&r.tree)?;
            let tree: Tree = serde_json::from_str(&text).map_err(store)?;
            blobs.insert(r.tree.clone(), text);
            for (_, _, _, h) in tree.objects {
                if !blobs.contains_key(&h) {
                    blobs.insert(h.clone(), self.read_blob(&h)?);
                }
            }
            for (_, _, h) in tree.libraries {
                if !blobs.contains_key(&h) {
                    blobs.insert(h.clone(), self.read_blob(&h)?);
                }
            }
        }
        Ok(Bundle {
            format: BUNDLE_FORMAT.into(),
            version: 1,
            branch: branch.to_owned(),
            head,
            revisions,
            blobs,
        })
    }

    /// Adds a bundle's revisions (checked against their ids) and makes its
    /// branch `as_branch` (default: the bundle's name): a new branch, or an
    /// existing one moved forward. Then merge it as any branch. Returns the
    /// number of new revisions.
    pub fn import_bundle(
        &self,
        user: &str,
        bundle: &Bundle,
        as_branch: Option<&str>,
    ) -> Result<usize, SessionError> {
        self.require(user, Role::Designer)?;
        if bundle.format != BUNDLE_FORMAT || bundle.version != 1 {
            return Err(refused(format!(
                "not a bundle of this version ({} {})",
                bundle.format, bundle.version
            )));
        }
        for (h, text) in &bundle.blobs {
            if hash(text) != *h {
                return Err(refused(format!("bundle: blob {h} does not match its hash")));
            }
        }
        let known: BTreeSet<String> = self.revisions()?.into_iter().map(|r| r.id).collect();
        let mut seen = known.clone();
        let mut new = Vec::new();
        for r in &bundle.revisions {
            if revision_id(&r.tree, &r.parents, &r.author, &r.message) != r.id {
                return Err(refused(format!(
                    "bundle: revision {} is damaged",
                    r.short()
                )));
            }
            if let Some(p) = r.parents.iter().find(|p| !seen.contains(*p)) {
                return Err(refused(format!(
                    "bundle: parent {} of {} missing",
                    short(p),
                    r.short()
                )));
            }
            let has = |h: &str| -> Result<bool, SessionError> {
                Ok(bundle.blobs.contains_key(h)
                    || self
                        .conn
                        .prepare_cached("SELECT 1 FROM vc_blob WHERE hash = ?1")?
                        .exists(params![h])?)
            };
            if !has(&r.tree)? {
                return Err(refused(format!("bundle: tree of {} missing", r.short())));
            }
            seen.insert(r.id.clone());
            if !known.contains(&r.id) {
                new.push(r);
            }
        }
        if !seen.contains(&bundle.head) {
            return Err(refused("bundle: its head is not among its revisions"));
        }
        let name = as_branch.unwrap_or(&bundle.branch).trim().to_owned();
        let existing = self.find_ref(&name)?;
        match &existing {
            Some(r) if r.kind == RefKind::Tag => {
                return Err(refused(format!("\"{name}\" is a tag")));
            }
            Some(r) => {
                if self.head()? == Head::Branch(name.clone()) {
                    return Err(refused(format!(
                        "\"{name}\" is the current branch: import under another name and merge it"
                    )));
                }
                self.check_branch_write(user, &name, false)?;
                if let Some(at) = &r.revision {
                    // Moved forward only: the branch's revision must be in
                    // the bundle's history.
                    if !bundle.revisions.iter().any(|x| x.id == *at) {
                        return Err(refused(format!(
                            "branch \"{name}\" has revisions the bundle lacks: import under another name and merge"
                        )));
                    }
                }
            }
            None => {
                self.check_ref_name(&name)?;
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        for (h, text) in &bundle.blobs {
            Self::write_blob(&tx, h, text)?;
        }
        for r in &new {
            tx.execute(
                "INSERT INTO vc_revision (id, parents, author, time, message, tree, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    r.id,
                    serde_json::to_string(&r.parents).expect("serialises"),
                    r.author,
                    r.time,
                    r.message,
                    r.tree,
                    r.seq
                ],
            )?;
        }
        if existing.is_some() {
            tx.execute(
                "UPDATE vc_ref SET revision = ?1 WHERE name = ?2",
                params![bundle.head, name],
            )?;
        } else {
            tx.execute(
                "INSERT INTO vc_ref (name, kind, revision, user) VALUES (?1, 'branch', ?2, ?3)",
                params![name, bundle.head, user],
            )?;
        }
        tx.commit()?;
        Ok(new.len())
    }
}

/// The line of `state` with its structure types placed from its own
/// libraries (`structures`, `insulators`): the line as solved.
pub fn resolved_line(state: &State) -> Result<Line, String> {
    use ols_structures::LibraryItem;
    let line = state.line().map_err(|e| e.to_string())?;
    if !line.structures.iter().any(|s| s.structure_type.is_some()) {
        return Ok(line);
    }
    let mut structures = ols_structures::StructureLibrary::default();
    for (name, text) in state.library_files("structures") {
        let path = std::path::Path::new(name);
        let item = ols_structures::StructureType::parse(path, text).map_err(|e| e.to_string())?;
        structures.insert(path, item).map_err(|e| e.to_string())?;
    }
    let mut insulators = ols_structures::InsulatorLibrary::default();
    for (name, text) in state.library_files("insulators") {
        let path = std::path::Path::new(name);
        let item =
            ols_structures::InsulatorAssembly::parse(path, text).map_err(|e| e.to_string())?;
        insulators.insert(path, item).map_err(|e| e.to_string())?;
    }
    line.resolve(&structures, &insulators)
        .map_err(|e| e.to_string())
}
