// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Country libraries (ADR-0013, phase 9; network schema 5): every library
//! kind kept **once for the whole network**, its maintainers, requests for
//! additions, releases, the lines' pins, withdrawn entries and the use
//! index.
//!
//! - **The network's library**: per kind, files (`lib_file`, each file's
//!   text as it is now — its *head*) holding entries
//!   ([`ohl_session::country::entries`]). A **locked** kind (or file)
//!   changes only by requests granted (`lib_request`) and is given to the
//!   lines by releases; an unlocked one may be changed directly by whoever
//!   may edit it. A network admin may change a locked file in an
//!   emergency (`emergency`), in the audit.
//! - **Maintainers** are assignments, no list of their own: who holds
//!   *review* and *lock* on the kind (the shipped *library maintainer*
//!   role scoped to a library kind, a network admin). Granting, refusing,
//!   releasing and withdrawing need both; locking needs *lock*, unlocking
//!   a network admin.
//! - **Requests** ([`Request`]): the entry asked for as a file of its own
//!   (its draft), by whoever may request library additions on the line it
//!   is for (or on the network's library); draft → requested → granted,
//!   refused or withdrawn. A grant lays the entry into the head
//!   ([`ohl_session::country::with_entry`]), by a maintainer who is not
//!   the requester. While pending, the draft may be used in the
//!   requester's own branch (as `request-N.ext`, [`LibraryState::check_file`]);
//!   it blocks the branch's review and its merge into main until it is
//!   granted or taken out of the branch.
//! - **Releases** (`lib_release`, `lib_release_file`): the head of a kind
//!   frozen under a tag; its texts kept as the version store keeps blobs
//!   (each once for the network); each entry's version noted with the
//!   release it first came in (`lib_entry`).
//! - **Pins** (`line.library_pins`: kind → release): a line holds the
//!   files of the release it is pinned to as its own library files, so
//!   the engine reads them as today and an exported model stands alone. A
//!   release changes no line; **Update library pins** ([`plan_pins`],
//!   [`apply_pins`]) moves a line's pins as one logged command, the routes
//!   using what changed marked out of date. Built lines (`line.built`)
//!   are pinned (moved only by whoever may review the whole line);
//!   designs in progress follow the latest (moved together by
//!   `--following`).
//! - **Withdrawn entries** (`lib_withdrawn`): kept in the head and later
//!   releases (a line using one keeps working and is shown it); no object
//!   may start using one (the server refuses it).
//! - **The use index** (`lib_use`): what each element of a model's main
//!   branch uses, and the entries of the files a line picks as a whole;
//!   read through the rights' one function ([`Network::uses`]): an entry
//!   one may not see is not found, a use on an object one may not see is
//!   left out without a word.
//!
//! Each change ([`LibraryCommand`]) is one transaction serialised by an
//! advisory lock, decided by the rights' one function, with its audit row
//! and the libraries' revision moved on (`net_meta` `library_rev`: the
//! server reads the locks, requests and withdrawn entries again when it
//! moved). Library changes need no writer's lock (they change no model);
//! Update library pins does (it changes a model: the server, or the
//! command line while no server runs).

use std::collections::{BTreeMap, BTreeSet};

use ohl_session::country::{self, REFERENCED_KINDS};
use ohl_session::rights::{NoModels, Permission, Placement, Target, UserRights};
use ohl_session::{Applied, Kind, Role, RoleSource, Session};
use postgres::Row;
use serde::{Deserialize, Serialize};

use crate::{Conn, NetError, Network, WRITER_LOCK, lock};

/// The advisory lock serialising library changes.
const LIBRARY_LOCK: i64 = WRITER_LOCK + 3;

/// The kinds of a line's elements whose objects use entries.
const USING_KINDS: [Kind; 4] = [Kind::Section, Kind::Jumper, Kind::Structure, Kind::Spacer];

fn refused(m: impl Into<String>) -> NetError {
    NetError::Refused(m.into())
}

/// A change of the network's library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum LibraryCommand {
    /// Lock (or unlock) a kind, or one file of it.
    Lock {
        /// The kind.
        kind: String,
        /// One file of it (none: the kind).
        #[serde(default)]
        file: Option<String>,
        /// Locked.
        locked: bool,
        /// Why.
        #[serde(default)]
        note: String,
    },
    /// Put a file into the head (an unlocked kind; a locked one in an
    /// emergency, network admins only).
    PutFile {
        /// The kind.
        kind: String,
        /// The file's name (with its extension).
        file: String,
        /// Its text.
        text: String,
        /// A network admin's change of a locked file.
        #[serde(default)]
        emergency: bool,
    },
    /// Remove a file from the head (as [`LibraryCommand::PutFile`]).
    DeleteFile {
        /// The kind.
        kind: String,
        /// The file.
        file: String,
        /// A network admin's change of a locked file.
        #[serde(default)]
        emergency: bool,
    },
    /// Ask for an entry to be added (a maintainer: or changed).
    Request {
        /// The kind.
        kind: String,
        /// The file of the network's library it goes into.
        file: String,
        /// A file holding the entry (its other entries left out).
        text: String,
        /// The entry (none: the file's only one).
        #[serde(default)]
        entry: Option<String>,
        /// Why.
        #[serde(default)]
        reason: String,
        /// The line it is for (its requester's own branch uses it).
        #[serde(default)]
        line: Option<String>,
        /// That branch (not main).
        #[serde(default)]
        branch: Option<String>,
        /// The routes it is for.
        #[serde(default)]
        routes: Vec<String>,
        /// Kept as a draft (not yet asked; only its requester sees it).
        #[serde(default)]
        draft: bool,
    },
    /// A draft asked for.
    Submit {
        /// The request.
        id: i64,
    },
    /// A request taken back by its requester.
    WithdrawRequest {
        /// The request.
        id: i64,
    },
    /// A request granted: its entry laid into the head.
    Grant {
        /// The request.
        id: i64,
        /// A note.
        #[serde(default)]
        note: String,
    },
    /// A request refused.
    Refuse {
        /// The request.
        id: i64,
        /// Why.
        #[serde(default)]
        note: String,
    },
    /// The head of a kind released under a tag.
    Release {
        /// The kind.
        kind: String,
        /// The tag.
        tag: String,
        /// Notes.
        #[serde(default)]
        notes: String,
    },
    /// An entry withdrawn (kept for who uses it; no new use).
    WithdrawEntry {
        /// The kind.
        kind: String,
        /// The file.
        file: String,
        /// The entry.
        entry: String,
        /// Why.
        #[serde(default)]
        note: String,
    },
    /// A withdrawn entry back.
    RestoreEntry {
        /// The kind.
        kind: String,
        /// The file.
        file: String,
        /// The entry.
        entry: String,
    },
    /// A line marked built (pinned) or back to a design in progress
    /// (following the latest).
    SetBuilt {
        /// The line.
        line: String,
        /// Built.
        built: bool,
    },
}

impl LibraryCommand {
    /// The audit's action (`library.<command>`).
    pub fn action(&self) -> String {
        let v = serde_json::to_value(self).expect("serialises");
        format!(
            "library.{}",
            v.get("do").and_then(|d| d.as_str()).unwrap_or("change")
        )
    }
}

/// What a library change did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Done {
    /// In words.
    pub text: String,
    /// The request made (a [`LibraryCommand::Request`]).
    pub request: Option<i64>,
}

/// A request for an addition (or a maintainer's change).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Request {
    /// Its number.
    pub id: i64,
    /// The kind.
    pub kind: String,
    /// The file of the network's library it goes into.
    pub file: String,
    /// The entry.
    pub entry: String,
    /// The draft: a file holding the entry.
    pub text: String,
    /// draft, requested, granted, refused, withdrawn.
    pub state: String,
    /// Who asked.
    pub user: String,
    /// The line it is for.
    pub line: Option<String>,
    /// The branch its requester uses it in.
    pub branch: Option<String>,
    /// Why.
    pub reason: String,
    /// The routes it is for.
    pub routes: Vec<String>,
    /// Who decided.
    pub maintainer: Option<String>,
    /// The decision's note.
    pub note: Option<String>,
    /// When asked.
    pub created: String,
    /// When decided.
    pub decided: Option<String>,
    /// The release it came in.
    pub released: Option<String>,
}

impl Request {
    /// Not decided yet (a draft or asked).
    pub fn pending(&self) -> bool {
        matches!(self.state.as_str(), "draft" | "requested")
    }

    /// The file name of its draft in a line (`request-12.ohlw`).
    pub fn draft_name(&self) -> String {
        country::draft_name(self.id, &self.file)
    }
}

const REQUEST_SELECT: &str =
    "SELECT id, kind, file, entry, text, state, user_name, line_id, branch,
     reason, routes, maintainer, note, ohl_time(created), ohl_time(decided), released
     FROM lib_request";

fn request_of(r: &Row) -> Request {
    Request {
        id: r.get(0),
        kind: r.get(1),
        file: r.get(2),
        entry: r.get(3),
        text: r.get(4),
        state: r.get(5),
        user: r.get(6),
        line: r.get(7),
        branch: r.get(8),
        reason: r.get(9),
        routes: r.get(10),
        maintainer: r.get(11),
        note: r.get(12),
        created: r.get(13),
        decided: r.get(14),
        released: r.get(15),
    }
}

/// What the server keeps of the libraries to decide commands with: the
/// locks, the requests (their drafts), the withdrawn entries.
#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    /// The libraries' revision when read.
    pub rev: i64,
    /// Kinds locked.
    pub locked_kinds: BTreeSet<String>,
    /// Files locked (kind, file).
    pub locked_files: BTreeSet<(String, String)>,
    /// Every request, by number.
    pub requests: BTreeMap<i64, Request>,
    /// Withdrawn entries (kind, entry id as objects name it).
    pub withdrawn: BTreeSet<(String, String)>,
}

impl LibraryState {
    /// Whether file `file` of `kind` is locked (its kind or itself).
    pub fn locked(&self, kind: &str, file: &str) -> bool {
        self.locked_kinds.contains(kind)
            || self
                .locked_files
                .contains(&(kind.to_owned(), file.to_owned()))
    }

    /// **The rule of a line's library file** changed by `user` on branch
    /// `branch` of line `line` (`text`: as it is after; `None`: removed):
    ///
    /// - a file named as a request's draft (`request-N.ext`) holds that
    ///   request's text; it is put or changed only by its requester, in
    ///   the request's own branch (never main), while the request is a
    ///   draft, asked or granted; it reaches main (a merge) only once
    ///   granted — pending, it **blocks the merge**; taking it out (the
    ///   draft replaced) is always allowed;
    /// - any other file of a locked kind (or a locked file) is refused:
    ///   it reaches a line by Update library pins only.
    pub fn check_file(
        &self,
        user: &str,
        line: &str,
        branch: &str,
        kind: &str,
        file: &str,
        text: Option<&str>,
    ) -> Result<(), String> {
        let main = ohl_session::versions::MAIN;
        if let Some(id) = country::draft_of(file) {
            let Some(r) = self.requests.get(&id) else {
                return Err(format!(
                    "library file {kind}/{file} is named as a request's draft, and there is no \
                     library request #{id}"
                ));
            };
            let Some(text) = text else {
                return Ok(());
            };
            if r.kind != kind || r.line.as_deref() != Some(line) {
                return Err(format!(
                    "library request #{id} is for {} on {}",
                    r.kind,
                    r.line.as_deref().unwrap_or("no line")
                ));
            }
            if text != r.text {
                return Err(format!(
                    "{kind}/{file} is library request #{id}'s draft: it holds the request's text"
                ));
            }
            if branch == main {
                return if r.state == "granted" {
                    Ok(())
                } else {
                    Err(format!(
                        "library request #{id} ({kind} {}) is {}: its draft blocks the merge into \
                         {main} until it is granted, or taken out of the branch",
                        r.entry, r.state
                    ))
                };
            }
            if r.user != user || r.branch.as_deref() != Some(branch) {
                return Err(format!(
                    "{kind}/{file} is library request #{id}'s draft: only its requester uses it, \
                     in the branch it was asked for"
                ));
            }
            if !matches!(r.state.as_str(), "draft" | "requested" | "granted") {
                return Err(format!(
                    "library request #{id} is {}: its draft is no longer used",
                    r.state
                ));
            }
            return Ok(());
        }
        if self.locked(kind, file) {
            return Err(format!(
                "library {kind} is locked for the whole network: it changes by a request granted \
                 and a release (ohl network library …), and reaches a line by Update library pins"
            ));
        }
        Ok(())
    }

    /// The drafts among a branch's library files (kind, name, text) whose
    /// request is not granted: each blocks its review and merge.
    pub fn pending_drafts(&self, files: &[(String, String, String)]) -> Vec<String> {
        files
            .iter()
            .filter_map(|(kind, name, _)| {
                let id = country::draft_of(name)?;
                match self.requests.get(&id) {
                    Some(r) if r.state == "granted" => None,
                    Some(r) => Some(format!(
                        "{kind}/{name} (library request #{id}, {})",
                        r.state
                    )),
                    None => Some(format!("{kind}/{name} (no such library request)")),
                }
            })
            .collect()
    }

    /// The withdrawn entries among `uses` (library kind, entry).
    pub fn withdrawn_in(&self, uses: &[(&'static str, String)]) -> Vec<String> {
        uses.iter()
            .filter(|(k, e)| self.withdrawn.contains(&((*k).to_owned(), e.clone())))
            .map(|(k, e)| format!("{k} {e}"))
            .collect()
    }
}

/// A line's library change in one Update library pins: a file put
/// (`text`) or removed (`None`); `existed`: the line held it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileChange {
    /// The file.
    pub file: String,
    /// Its text after (`None`: removed).
    pub text: Option<String>,
    /// The line held it before.
    pub existed: bool,
}

/// What Update library pins does to one kind of a line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindPlan {
    /// The kind.
    pub kind: String,
    /// The release it is pinned to (none: the line's own files).
    pub from: Option<String>,
    /// The release it moves to.
    pub to: String,
    /// The files changed.
    pub changes: Vec<FileChange>,
    /// The entries whose version changed (added, changed or removed).
    pub changed: Vec<String>,
    /// The entries changed that the line uses.
    pub used_changed: Vec<String>,
    /// The routes whose results go out of date.
    pub stale_routes: Vec<String>,
    /// The line's own files kept (not of the release).
    pub kept: Vec<String>,
    /// Withdrawn entries the line uses (still there, shown).
    pub withdrawn_used: Vec<String>,
    /// Why this kind does not move (nothing of it is changed).
    pub refused: Option<String>,
}

/// What Update library pins does to a line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PinPlan {
    /// The line.
    pub line: String,
    /// Built (pinned): moved only by whoever may review the whole line.
    pub built: bool,
    /// Per kind.
    pub kinds: Vec<KindPlan>,
}

impl PinPlan {
    /// The kinds that move.
    fn moving(&self) -> impl Iterator<Item = &KindPlan> {
        self.kinds.iter().filter(|k| k.refused.is_none())
    }

    /// The library file changes of the kinds that move (kind, name, text).
    pub fn changes(&self) -> Vec<(String, String, Option<String>)> {
        self.moving()
            .flat_map(|k| {
                k.changes
                    .iter()
                    .map(|c| (k.kind.clone(), c.file.clone(), c.text.clone()))
            })
            .collect()
    }

    /// The pins after (kind → release) of the kinds that move.
    pub fn pins(&self) -> BTreeMap<String, String> {
        self.moving()
            .map(|k| (k.kind.clone(), k.to.clone()))
            .collect()
    }

    /// The routes out of date, each with its notice.
    pub fn stale(&self) -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for k in self.moving() {
            let what = format!(
                "library {} → {}: {} changed",
                k.kind,
                k.to,
                if k.used_changed.is_empty() {
                    "entries".to_owned()
                } else {
                    k.used_changed.join(", ")
                }
            );
            for r in &k.stale_routes {
                out.entry(r.clone()).or_default().push(what.clone());
            }
        }
        out.into_iter().map(|(r, w)| (r, w.join("; "))).collect()
    }

    /// The log entry's words.
    pub fn note(&self) -> String {
        let parts: Vec<String> = self
            .moving()
            .map(|k| {
                format!(
                    "{} {}→ {} ({} file{} changed)",
                    k.kind,
                    k.from.as_deref().map_or(String::new(), |f| format!("{f} ")),
                    k.to,
                    k.changes.len(),
                    if k.changes.len() == 1 { "" } else { "s" }
                )
            })
            .collect();
        format!("library pins: {}", parts.join(", "))
    }

    /// Whether anything moves (a pin, or a file).
    pub fn moves(&self) -> bool {
        self.moving()
            .any(|k| k.from.as_deref() != Some(k.to.as_str()) || !k.changes.is_empty())
    }

    /// In words, for the command line (a dry run or what was done).
    pub fn describe(&self) -> String {
        let mut out = format!(
            "line {}{}:\n",
            self.line,
            if self.built { " (built, pinned)" } else { "" }
        );
        for k in &self.kinds {
            out.push_str(&format!(
                "  {}: {} → {}",
                k.kind,
                k.from.as_deref().unwrap_or("own files"),
                k.to
            ));
            if let Some(why) = &k.refused {
                out.push_str(&format!(" — not moved: {why}\n"));
                continue;
            }
            out.push_str(&format!(
                ", {} file(s) changed, {} entr{} changed ({} used), {} route(s) out of date\n",
                k.changes.len(),
                k.changed.len(),
                if k.changed.len() == 1 { "y" } else { "ies" },
                k.used_changed.len(),
                k.stale_routes.len()
            ));
            if !k.kept.is_empty() {
                out.push_str(&format!("    own files kept: {}\n", k.kept.join(", ")));
            }
            if !k.withdrawn_used.is_empty() {
                out.push_str(&format!(
                    "    withdrawn entries still used: {}\n",
                    k.withdrawn_used.join(", ")
                ));
            }
        }
        out
    }
}

/// A row of the use index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UseRow {
    /// The object's kind (`library` for a file a line holds).
    pub object_kind: String,
    /// The object (`kind/name` for a file).
    pub object: String,
    /// The library kind.
    pub kind: String,
    /// The file (empty: an entry named by id).
    pub file: String,
    /// The entry.
    pub entry: String,
}

/// A use as a viewer may see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UseView {
    /// The line.
    pub line: String,
    /// The branch (`main`, or a branch using it in what it changed;
    /// phase 11: an object there is placed as on main — one made there
    /// alone is seen only by who sees the whole line).
    pub branch: String,
    /// The object's kind (`library`: the line holds the file).
    pub object_kind: String,
    /// The object.
    pub object: String,
    /// Its routes the viewer may see.
    pub routes: Vec<String>,
}

/// A line's state of its libraries.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineLibraries {
    /// The line.
    pub line: String,
    /// Built (pinned).
    pub built: bool,
    /// Per kind of the network's library.
    pub kinds: Vec<KindState>,
}

/// A line's state of one kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindState {
    /// The kind.
    pub kind: String,
    /// Its pin (none: its own files).
    pub pinned: Option<String>,
    /// The latest release.
    pub latest: Option<String>,
    /// Releases made since its pin.
    pub behind: usize,
    /// Withdrawn entries it uses.
    pub withdrawn_used: Vec<String>,
}

/// A kind of the network's library.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindInfo {
    /// The kind.
    pub kind: String,
    /// Locked.
    pub locked: bool,
    /// Files in its head.
    pub files: usize,
    /// Its releases, oldest first.
    pub releases: Vec<String>,
    /// Its maintainers (users and groups holding review and lock on it).
    pub maintainers: Vec<String>,
}

/// A file of the head with its entries (those the viewer may see).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileInfo {
    /// The file.
    pub file: String,
    /// Locked on its own.
    pub locked: bool,
    /// Its entries: id, version (short), first release, withdrawn.
    pub entries: Vec<(String, String, Option<String>, bool)>,
}

/// Roles of a model's session from a user's rights (the command line's
/// Update library pins, as the server's sessions take theirs).
pub struct RightsRoles {
    /// The model.
    pub line: String,
    /// The user's rights.
    pub rights: UserRights,
}

impl RoleSource for RightsRoles {
    fn role(&self, user: &str) -> Option<Role> {
        (user == self.rights.user())
            .then(|| self.rights.model_role(&self.line))
            .flatten()
    }
}

fn check_name(what: &str, name: &str) -> Result<(), NetError> {
    if ohl_session::safe_file_name(name) && !name.contains('/') && name.len() <= 128 {
        Ok(())
    } else {
        Err(refused(format!(
            "{what} \"{name}\": a plain name (letters, digits, . _ -)"
        )))
    }
}

fn check_tag(tag: &str) -> Result<(), NetError> {
    let ok = !tag.is_empty()
        && tag.len() <= 64
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(refused(format!(
            "release \"{tag}\": 1 to 64 letters, digits, . _ -"
        )))
    }
}

fn country_target<'a>(kind: &'a str, file: Option<&'a str>, entry: Option<&'a str>) -> Target<'a> {
    Target::CountryLibrary { kind, file, entry }
}

/// May see line `line` itself — its settings (a grant on the line, a
/// route, a region or an object of it): not only, through a library
/// scope, its library files.
fn sees_line(me: &UserRights, line: &str) -> bool {
    me.allows(
        Permission::See,
        &Target::object(line, Kind::Meta, "line", Placement::default()),
    )
}

/// Holds review and lock on the network's library target: a maintainer.
fn maintains(me: &UserRights, kind: &str, file: Option<&str>, entry: Option<&str>) -> bool {
    let t = country_target(kind, file, entry);
    me.allows(Permission::Review, &t) && me.allows(Permission::Lock, &t)
}

/// The head of a kind: file → text.
fn head(c: &mut Conn, kind: &str) -> Result<BTreeMap<String, String>, postgres::Error> {
    Ok(c.query(
        "SELECT name, text FROM lib_file WHERE kind = $1 ORDER BY name",
        &[&kind],
    )?
    .into_iter()
    .map(|r| (r.get(0), r.get(1)))
    .collect())
}

fn kind_locked(c: &mut Conn, kind: &str) -> Result<bool, postgres::Error> {
    Ok(
        c.query_opt("SELECT locked FROM lib_kind WHERE kind = $1", &[&kind])?
            .is_some_and(|r| r.get(0)),
    )
}

fn file_locked(c: &mut Conn, kind: &str, file: &str) -> Result<bool, postgres::Error> {
    Ok(kind_locked(c, kind)?
        || c.query_opt(
            "SELECT locked FROM lib_file WHERE kind = $1 AND name = $2",
            &[&kind, &file],
        )?
        .is_some_and(|r| r.get(0)))
}

/// The withdrawn entries of a kind: (file, entry).
fn withdrawn_of(c: &mut Conn, kind: &str) -> Result<BTreeSet<(String, String)>, postgres::Error> {
    Ok(c.query(
        "SELECT file, entry FROM lib_withdrawn WHERE kind = $1",
        &[&kind],
    )?
    .into_iter()
    .map(|r| (r.get(0), r.get(1)))
    .collect())
}

/// Checks a head file's new text: TOML with entries; for the kinds named
/// by id, no id another file of the kind holds; its withdrawn entries
/// kept as they are.
/// Phase 14: the entry ids of each file of `kind` in the head — each
/// file's text read and parsed only when it is not known by its SHA-256
/// already (a line's import lays in hundreds of files one after the
/// other: each used to read and parse every other file of its kind
/// again, minutes for a line with its towers' structure files).
fn head_entry_ids(
    c: &mut Conn,
    kind: &str,
) -> Result<Vec<(String, crate::EntryIds)>, postgres::Error> {
    let rows = c.query(
        "SELECT name, sha256(convert_to(text, 'UTF8')) FROM lib_file WHERE kind = $1 \
         ORDER BY name",
        &[&kind],
    )?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let (name, hash): (String, Vec<u8>) = (r.get(0), r.get(1));
        let key = (kind.to_owned(), name.clone());
        let known = c
            .entry_ids
            .get(&key)
            .filter(|(h, _)| *h == hash)
            .map(|(_, ids)| ids.clone());
        let ids = match known {
            Some(ids) => ids,
            None => {
                let text: String = c
                    .query_one(
                        "SELECT text FROM lib_file WHERE kind = $1 AND name = $2",
                        &[&kind, &name],
                    )?
                    .get(0);
                let ids: BTreeSet<String> = country::entries(&name, &text)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|e| e.id)
                    .collect();
                let ids = std::sync::Arc::new(ids);
                c.entry_ids.insert(key, (hash, ids.clone()));
                ids
            }
        };
        out.push((name, ids));
    }
    Ok(out)
}

fn check_head_text(
    c: &mut Conn,
    kind: &str,
    file: &str,
    text: &str,
) -> Result<Vec<country::Entry>, NetError> {
    let entries = country::entries(file, text).map_err(refused)?;
    if REFERENCED_KINDS.contains(&kind) {
        for (other, theirs) in head_entry_ids(c, kind)? {
            if other == file {
                continue;
            }
            if let Some(e) = entries.iter().find(|e| theirs.contains(&e.id)) {
                return Err(refused(format!(
                    "entry {} is in {kind}/{other} already: an entry of {kind} is named once \
                     in the network",
                    e.id
                )));
            }
        }
    }
    let old: Option<String> = c
        .query_opt(
            "SELECT text FROM lib_file WHERE kind = $1 AND name = $2",
            &[&kind, &file],
        )?
        .map(|r| r.get(0));
    if let Some(old) = old {
        let old = country::entries(file, &old).unwrap_or_default();
        for (f, e) in withdrawn_of(c, kind)? {
            if f != file {
                continue;
            }
            let before = old.iter().find(|x| x.id == e);
            let after = entries.iter().find(|x| x.id == e);
            if before.is_some() && before.map(|x| &x.version) != after.map(|x| &x.version) {
                return Err(refused(format!(
                    "entry {e} is withdrawn: it stays as it is (the lines using it keep it)"
                )));
            }
        }
    }
    Ok(entries)
}

fn put_head(
    c: &mut Conn,
    kind: &str,
    file: &str,
    text: &str,
    actor: &str,
) -> Result<(), postgres::Error> {
    let hash = country::text_hash(text);
    c.execute(
        "INSERT INTO lib_file (kind, name, text, hash, by_user, time)
         VALUES ($1, $2, $3, $4, $5, clock_timestamp())
         ON CONFLICT (kind, name) DO UPDATE SET text = excluded.text, hash = excluded.hash,
             by_user = excluded.by_user, time = excluded.time",
        &[&kind, &file, &text, &hash, &actor],
    )?;
    Ok(())
}

/// The latest release of each kind.
fn latest_releases(c: &mut Conn) -> Result<BTreeMap<String, String>, postgres::Error> {
    Ok(c.query(
        "SELECT DISTINCT ON (kind) kind, tag FROM lib_release ORDER BY kind, n DESC",
        &[],
    )?
    .into_iter()
    .map(|r| (r.get(0), r.get(1)))
    .collect())
}

/// The files of release `tag` of `kind`: file → text.
fn release_files(
    c: &mut Conn,
    kind: &str,
    tag: &str,
) -> Result<BTreeMap<String, String>, NetError> {
    let rows = c.query(
        "SELECT f.file, b.data FROM lib_release_file f JOIN vc_blob b ON b.hash = f.hash
         WHERE f.kind = $1 AND f.tag = $2 ORDER BY f.file",
        &[&kind, &tag],
    )?;
    let mut out = BTreeMap::new();
    for r in rows {
        let data: Vec<u8> = r.get(1);
        out.insert(
            r.get(0),
            country::blob_text(&data).map_err(NetError::Database)?,
        );
    }
    Ok(out)
}

/// The releases of a kind, oldest first.
fn releases_of(c: &mut Conn, kind: &str) -> Result<Vec<String>, postgres::Error> {
    Ok(c.query(
        "SELECT tag FROM lib_release WHERE kind = $1 ORDER BY n",
        &[&kind],
    )?
    .into_iter()
    .map(|r| r.get(0))
    .collect())
}

fn bump(c: &mut Conn) -> Result<(), postgres::Error> {
    c.execute(
        "INSERT INTO net_meta (key, value) VALUES ('library_rev', '1')
         ON CONFLICT (key) DO UPDATE SET value = (net_meta.value::bigint + 1)::text",
        &[],
    )?;
    Ok(())
}

/// Lays request `r`'s entry into the head (a grant, an import's
/// addition): checked as a head change.
fn lay_in(c: &mut Conn, r: &Request, actor: &str) -> Result<(), NetError> {
    let files = head(c, &r.kind)?;
    let text = country::with_entry(files.get(&r.file).map(String::as_str), &r.text, &r.entry)
        .map_err(refused)?;
    check_head_text(c, &r.kind, &r.file, &text)?;
    put_head(c, &r.kind, &r.file, &text, actor)?;
    Ok(())
}

/// Where entry `entry` of `kind` is in the head (its file), else in the
/// newest release holding it.
fn file_of_entry(c: &mut Conn, kind: &str, entry: &str) -> Result<Option<String>, NetError> {
    for (f, t) in head(c, kind)? {
        if country::entries(&f, &t)
            .unwrap_or_default()
            .iter()
            .any(|e| e.id == entry)
        {
            return Ok(Some(f));
        }
    }
    Ok(c.query_opt(
        "SELECT e.file FROM lib_entry e JOIN lib_release r ON r.kind = e.kind AND r.tag = e.release
             WHERE e.kind = $1 AND e.entry = $2 ORDER BY r.n DESC LIMIT 1",
        &[&kind, &entry],
    )?
    .map(|r| r.get(0)))
}

impl Network {
    /// The libraries' revision (each change adds one).
    pub fn library_rev(&self) -> Result<i64, NetError> {
        Ok(lock(&self.conn)
            .query_opt("SELECT value FROM net_meta WHERE key = 'library_rev'", &[])?
            .and_then(|r| r.get::<_, String>(0).parse().ok())
            .unwrap_or(0))
    }

    /// The rights' and the libraries' revisions, in one round trip.
    pub fn revisions(&self) -> Result<(i64, i64), NetError> {
        let mut out = (0, 0);
        for r in lock(&self.conn).query(
            "SELECT key, value FROM net_meta WHERE key IN ('rights_rev', 'library_rev')",
            &[],
        )? {
            let (k, v): (String, String) = (r.get(0), r.get(1));
            let v = v.parse().unwrap_or(0);
            if k == "rights_rev" {
                out.0 = v;
            } else {
                out.1 = v;
            }
        }
        Ok(out)
    }

    /// The locks, requests and withdrawn entries, read at one moment.
    pub fn library_state(&self) -> Result<LibraryState, NetError> {
        let mut c = lock(&self.conn);
        let mut s = LibraryState::default();
        for r in c.query("SELECT kind FROM lib_kind WHERE locked", &[])? {
            s.locked_kinds.insert(r.get(0));
        }
        for r in c.query("SELECT kind, name FROM lib_file WHERE locked", &[])? {
            s.locked_files.insert((r.get(0), r.get(1)));
        }
        for r in c.client.query(REQUEST_SELECT, &[])? {
            let q = request_of(&r);
            s.requests.insert(q.id, q);
        }
        for r in c.query("SELECT kind, entry FROM lib_withdrawn", &[])? {
            s.withdrawn.insert((r.get(0), r.get(1)));
        }
        s.rev = c
            .query_opt("SELECT value FROM net_meta WHERE key = 'library_rev'", &[])?
            .and_then(|r| r.get::<_, String>(0).parse().ok())
            .unwrap_or(0);
        Ok(s)
    }

    /// Applies library change `command` of `actor`: decided by the rights'
    /// one function against the rights and libraries as they are (read in
    /// the same transaction), written, audited, the revision moved on;
    /// refused (and audited) otherwise.
    pub fn library_command(&self, actor: &str, command: &LibraryCommand) -> Result<Done, NetError> {
        let done = self.library_change(actor, command);
        if let Err(e) = &done {
            // The command in full, but a file's text only by its size.
            let mut v = serde_json::to_value(command).expect("serialises");
            if let Some(t) = v.get_mut("text") {
                let n = t.as_str().map_or(0, str::len);
                *t = serde_json::Value::String(format!("({n} bytes)"));
            }
            let _ = self.audit(actor, "refused", &command.action(), &format!("{e}; {v}"));
        }
        done
    }

    fn library_change(&self, actor: &str, command: &LibraryCommand) -> Result<Done, NetError> {
        let mut c = lock(&self.conn);
        if !c.steps.is_empty() {
            return Err(refused(
                "a library change during a unit of work of the same connection",
            ));
        }
        c.client.batch_execute("BEGIN")?;
        let out = (|| -> Result<(String, String, Option<i64>), NetError> {
            c.execute("SELECT pg_advisory_xact_lock($1)", &[&LIBRARY_LOCK])?;
            let data = crate::rights::read(&mut c)?;
            let me = data.compile(actor, &NoModels);
            let network_admin = me.allows(Permission::ManageRights, &Target::Network);
            let request = |c: &mut Conn, id: i64| -> Result<Request, NetError> {
                c.client
                    .query_opt(&format!("{REQUEST_SELECT} WHERE id = $1"), &[&id])?
                    .map(|r| request_of(&r))
                    .ok_or_else(|| refused(format!("no library request #{id}")))
            };
            let not_maintainer = |kind: &str| {
                refused(format!(
                    "{actor} is not a maintainer of library {kind} (review and lock on it)"
                ))
            };
            Ok(match command {
                LibraryCommand::Lock {
                    kind,
                    file,
                    locked,
                    note,
                } => {
                    check_name("library kind", kind)?;
                    let t = country_target(kind, file.as_deref(), None);
                    if *locked && !me.allows(Permission::Lock, &t) {
                        return Err(refused(format!("{actor} may not lock library {kind}")));
                    }
                    if !*locked && !network_admin {
                        return Err(refused(format!(
                            "{actor} may not unlock library {kind}: network admins only"
                        )));
                    }
                    match file {
                        None => {
                            c.execute(
                                "INSERT INTO lib_kind (kind, locked, by_user, note)
                                 VALUES ($1, $2, $3, $4)
                                 ON CONFLICT (kind) DO UPDATE SET locked = excluded.locked,
                                     by_user = excluded.by_user, time = clock_timestamp(),
                                     note = excluded.note",
                                &[kind, locked, &actor, note],
                            )?;
                        }
                        Some(f) => {
                            let n = c.execute(
                                "UPDATE lib_file SET locked = $3 WHERE kind = $1 AND name = $2",
                                &[kind, f, locked],
                            )?;
                            if n == 0 {
                                return Err(refused(format!("no library file {kind}/{f}")));
                            }
                        }
                    }
                    (
                        format!(
                            "library {kind}{} {}",
                            file.as_ref().map_or(String::new(), |f| format!("/{f}")),
                            if *locked { "locked" } else { "unlocked" }
                        ),
                        format!("library:{kind}"),
                        None,
                    )
                }
                LibraryCommand::PutFile {
                    kind,
                    file,
                    text,
                    emergency,
                } => {
                    check_name("library kind", kind)?;
                    check_name("library file", file)?;
                    if country::draft_of(file).is_some() {
                        return Err(refused(format!(
                            "{file}: the names request-N are the drafts' names"
                        )));
                    }
                    let exists = head(&mut c, kind)?.contains_key(file);
                    let t = country_target(kind, Some(file), None);
                    if file_locked(&mut c, kind, file)? {
                        if !(*emergency && network_admin) {
                            return Err(refused(format!(
                                "library {kind} is locked: it changes by requests granted (a \
                                 network admin may change it in an emergency)"
                            )));
                        }
                    } else {
                        let p = if exists {
                            Permission::Edit
                        } else {
                            Permission::Create
                        };
                        if !me.allows(p, &t) {
                            return Err(refused(format!(
                                "{actor} may not {} library {kind}/{file}",
                                p.as_str()
                            )));
                        }
                    }
                    let entries = check_head_text(&mut c, kind, file, text)?;
                    put_head(&mut c, kind, file, text, actor)?;
                    (
                        format!(
                            "library {kind}/{file} {} ({} entr{}){}",
                            if exists { "changed" } else { "added" },
                            entries.len(),
                            if entries.len() == 1 { "y" } else { "ies" },
                            if *emergency {
                                ", an emergency change"
                            } else {
                                ""
                            }
                        ),
                        format!("library:{kind}/{file}"),
                        None,
                    )
                }
                LibraryCommand::DeleteFile {
                    kind,
                    file,
                    emergency,
                } => {
                    if !head(&mut c, kind)?.contains_key(file) {
                        return Err(refused(format!("no library file {kind}/{file}")));
                    }
                    if file_locked(&mut c, kind, file)? {
                        if !(*emergency && network_admin) {
                            return Err(refused(format!(
                                "library {kind} is locked: its files stay (withdraw an entry \
                                 instead)"
                            )));
                        }
                    } else if !me
                        .allows(Permission::Delete, &country_target(kind, Some(file), None))
                    {
                        return Err(refused(format!(
                            "{actor} may not delete library {kind}/{file}"
                        )));
                    }
                    if !withdrawn_of(&mut c, kind)?.iter().all(|(f, _)| f != file) {
                        return Err(refused(format!(
                            "{kind}/{file} holds withdrawn entries: they stay"
                        )));
                    }
                    c.execute(
                        "DELETE FROM lib_file WHERE kind = $1 AND name = $2",
                        &[kind, file],
                    )?;
                    (
                        format!("library {kind}/{file} removed (its releases keep it)"),
                        format!("library:{kind}/{file}"),
                        None,
                    )
                }
                LibraryCommand::Request {
                    kind,
                    file,
                    text,
                    entry,
                    reason,
                    line,
                    branch,
                    routes,
                    draft,
                } => {
                    check_name("library kind", kind)?;
                    check_name("library file", file)?;
                    if country::draft_of(file).is_some() {
                        return Err(refused(format!(
                            "{file}: name the file of the network's library it goes into"
                        )));
                    }
                    let found = country::entries(file, text).map_err(refused)?;
                    let e = match entry {
                        Some(id) => found
                            .iter()
                            .find(|e| &e.id == id)
                            .ok_or_else(|| refused(format!("the text holds no entry {id}")))?,
                        None if found.len() == 1 => &found[0],
                        None => {
                            return Err(refused(
                                "the text holds several entries: name the one asked for",
                            ));
                        }
                    };
                    // The draft holds that entry alone.
                    let draft_text = country::only_entry(file, text, &e.id).map_err(refused)?;
                    let t = country_target(kind, Some(file), Some(&e.id));
                    match line {
                        Some(l) => {
                            let lt = Target::Library {
                                line: l,
                                kind,
                                file,
                                entry: Some(&e.id),
                            };
                            if !(sees_line(&me, l) && me.allows(Permission::Request, &lt)) {
                                return Err(refused(format!(
                                    "{actor} may not request library additions for line {l}"
                                )));
                            }
                            if c.query_opt("SELECT 1 FROM line WHERE id = $1", &[l])?
                                .is_none()
                            {
                                return Err(refused(format!("the network has no line {l}")));
                            }
                        }
                        None => {
                            if !me.allows(Permission::Request, &t) {
                                return Err(refused(format!(
                                    "{actor} may not request additions to library {kind}"
                                )));
                            }
                        }
                    }
                    if let Some(b) = branch {
                        let Some(l) = line else {
                            return Err(refused("a branch names its line"));
                        };
                        if b == ohl_session::versions::MAIN {
                            return Err(refused(format!(
                                "a pending draft is used in a branch of its requester's own, \
                                 never in {b}"
                            )));
                        }
                        let made: Option<String> = c
                            .query_opt(
                                "SELECT user_name FROM vc_ref
                                 WHERE line_id = $1 AND name = $2 AND kind = 'branch'",
                                &[l, b],
                            )?
                            .map(|r| r.get(0));
                        // Phase 11 (the owner's question 9 of phase 9): or
                        // a branch over a working set its maker shares
                        // with a team the requester is named in.
                        let team: bool = match &made {
                            Some(u) if u != actor => {
                                c.query_opt(
                                    "SELECT value FROM line_meta WHERE line_id = $1 AND key = $2",
                                    &[l, &ohl_session::shared::BRANCH_SCOPES_KEY],
                                )?
                                .and_then(|r| {
                                    serde_json::from_str::<
                                        BTreeMap<String, ohl_session::shared::BranchScope>,
                                    >(&r.get::<_, String>(0))
                                    .ok()
                                })
                                .and_then(|m| m.get(b).cloned())
                                .is_some_and(|sc| sc.by == *u && sc.team.iter().any(|t| t == actor))
                            }
                            _ => false,
                        };
                        match made {
                            None => {
                                return Err(refused(format!("line {l} has no branch {b}")));
                            }
                            Some(u) if u != actor && !team => {
                                return Err(refused(format!(
                                    "branch {b} is {u}'s: a draft is used in its requester's own \
                                     branch, or a team's branch they are named in"
                                )));
                            }
                            _ => {}
                        }
                    }
                    // An addition, or (a maintainer) a change.
                    let files = head(&mut c, kind)?;
                    let mut held_in: Option<String> = None;
                    for (f, ft) in &files {
                        let referenced = REFERENCED_KINDS.contains(&kind.as_str());
                        if (referenced || f == file)
                            && country::entries(f, ft)
                                .unwrap_or_default()
                                .iter()
                                .any(|x| x.id == e.id)
                        {
                            held_in = Some(f.clone());
                        }
                    }
                    if let Some(f) = &held_in {
                        if f != file {
                            return Err(refused(format!(
                                "entry {} is in {kind}/{f}: ask for its change there",
                                e.id
                            )));
                        }
                        if !me.allows(Permission::Edit, &t) {
                            return Err(refused(format!(
                                "entry {} is in library {kind} already: only its maintainers \
                                 ask for a change",
                                e.id
                            )));
                        }
                        if withdrawn_of(&mut c, kind)?.contains(&(f.clone(), e.id.clone())) {
                            return Err(refused(format!("entry {} is withdrawn", e.id)));
                        }
                    } else if e.list.is_none()
                        && let Some(ft) = files.get(file)
                    {
                        let theirs = country::entries(file, ft).unwrap_or_default();
                        return Err(refused(format!(
                            "{kind}/{file} holds {} already: a file of one entry is asked for \
                             under a name of its own",
                            theirs.first().map_or("an entry", |x| x.id.as_str())
                        )));
                    }
                    let state = if *draft { "draft" } else { "requested" };
                    let id: i64 = c
                        .query_one(
                            "INSERT INTO lib_request (kind, file, entry, reason, routes, state,
                                 user_name, text, line_id, branch)
                             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
                            &[
                                kind,
                                file,
                                &e.id,
                                reason,
                                routes,
                                &state,
                                &actor,
                                &draft_text,
                                line,
                                branch,
                            ],
                        )?
                        .get(0);
                    let name = country::draft_name(id, file);
                    (
                        format!(
                            "library request #{id}: {kind} {} into {file} ({state}){}",
                            e.id,
                            match (line, branch) {
                                (Some(l), Some(b)) => format!(
                                    "; its draft {kind}/{name} may be used in branch {b} of line \
                                     {l} until it is decided"
                                ),
                                _ => String::new(),
                            }
                        ),
                        format!("library_request:{id}"),
                        Some(id),
                    )
                }
                LibraryCommand::Submit { id } => {
                    let r = request(&mut c, *id)?;
                    if r.user != actor {
                        return Err(refused(format!(
                            "library request #{id} is {}'s: only they ask for it",
                            r.user
                        )));
                    }
                    if r.state != "draft" {
                        return Err(refused(format!("library request #{id} is {}", r.state)));
                    }
                    c.execute(
                        "UPDATE lib_request SET state = 'requested' WHERE id = $1",
                        &[id],
                    )?;
                    (
                        format!("library request #{id} asked for"),
                        format!("library_request:{id}"),
                        Some(*id),
                    )
                }
                LibraryCommand::WithdrawRequest { id } => {
                    let r = request(&mut c, *id)?;
                    if r.user != actor {
                        return Err(refused(format!(
                            "library request #{id} is {}'s: only they take it back",
                            r.user
                        )));
                    }
                    if !r.pending() {
                        return Err(refused(format!("library request #{id} is {}", r.state)));
                    }
                    c.execute(
                        "UPDATE lib_request SET state = 'withdrawn', decided = clock_timestamp()
                         WHERE id = $1",
                        &[id],
                    )?;
                    (
                        format!(
                            "library request #{id} taken back (its draft, where used, blocks \
                             the branch until taken out)"
                        ),
                        format!("library_request:{id}"),
                        Some(*id),
                    )
                }
                LibraryCommand::Grant { id, note } | LibraryCommand::Refuse { id, note } => {
                    let grant = matches!(command, LibraryCommand::Grant { .. });
                    let r = request(&mut c, *id)?;
                    if !maintains(&me, &r.kind, Some(&r.file), Some(&r.entry)) {
                        return Err(not_maintainer(&r.kind));
                    }
                    if r.state != "requested" {
                        return Err(refused(format!(
                            "library request #{id} is {}: only a request asked for is decided",
                            r.state
                        )));
                    }
                    if r.user == actor {
                        return Err(refused(format!(
                            "library request #{id} is {actor}'s own: another maintainer decides it"
                        )));
                    }
                    if grant {
                        if withdrawn_of(&mut c, &r.kind)?
                            .iter()
                            .any(|(_, e)| *e == r.entry)
                        {
                            return Err(refused(format!("entry {} is withdrawn", r.entry)));
                        }
                        lay_in(&mut c, &r, actor)?;
                    }
                    c.execute(
                        "UPDATE lib_request SET state = $2, maintainer = $3, note = $4,
                             decided = clock_timestamp()
                         WHERE id = $1",
                        &[id, &if grant { "granted" } else { "refused" }, &actor, note],
                    )?;
                    (
                        format!(
                            "library request #{id} {} ({} {}{})",
                            if grant { "granted" } else { "refused" },
                            r.kind,
                            r.entry,
                            if grant {
                                format!(" laid into {}; in the next release", r.file)
                            } else {
                                String::new()
                            }
                        ),
                        format!("library_request:{id}"),
                        Some(*id),
                    )
                }
                LibraryCommand::Release { kind, tag, notes } => {
                    check_tag(tag)?;
                    if !maintains(&me, kind, None, None) {
                        return Err(not_maintainer(kind));
                    }
                    let files = head(&mut c, kind)?;
                    if files.is_empty() {
                        return Err(refused(format!("library {kind} has no files to release")));
                    }
                    if c.query_opt(
                        "SELECT 1 FROM lib_release WHERE kind = $1 AND tag = $2",
                        &[kind, tag],
                    )?
                    .is_some()
                    {
                        return Err(refused(format!("library {kind} has a release {tag}")));
                    }
                    let tree: String = files
                        .iter()
                        .map(|(f, t)| format!("{f}\t{}\n", country::text_hash(t)))
                        .collect();
                    let revision = country::text_hash(&tree);
                    let last: Option<(String, String)> = c
                        .query_opt(
                            "SELECT tag, revision FROM lib_release WHERE kind = $1
                             ORDER BY n DESC LIMIT 1",
                            &[kind],
                        )?
                        .map(|r| (r.get(0), r.get(1)));
                    if let Some((t, rev)) = &last
                        && *rev == revision
                    {
                        return Err(refused(format!(
                            "library {kind} has not changed since release {t}"
                        )));
                    }
                    c.execute(
                        "INSERT INTO lib_release (kind, tag, revision, notes, by_user)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[kind, tag, &revision, notes, &actor],
                    )?;
                    let mut new_entries = 0;
                    let mut all_entries = BTreeSet::new();
                    for (f, t) in &files {
                        let hash = country::text_hash(t);
                        let data = country::blob_data(t).map_err(NetError::Database)?;
                        c.execute(
                            "INSERT INTO vc_blob (hash, data) VALUES ($1, $2)
                             ON CONFLICT DO NOTHING",
                            &[&hash, &data],
                        )?;
                        c.execute(
                            "INSERT INTO lib_release_file (kind, tag, file, hash)
                             VALUES ($1, $2, $3, $4)",
                            &[kind, tag, f, &hash],
                        )?;
                        for e in country::entries(f, t).map_err(refused)? {
                            new_entries += c.execute(
                                "INSERT INTO lib_entry (kind, file, entry, version, release)
                                 VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
                                &[kind, f, &e.id, &e.version, tag],
                            )?;
                            all_entries.insert(e.id);
                        }
                    }
                    let entries: Vec<String> = all_entries.iter().cloned().collect();
                    c.execute(
                        "UPDATE lib_request SET released = $2
                         WHERE kind = $1 AND state = 'granted' AND released IS NULL
                           AND entry = ANY($3)",
                        &[kind, tag, &entries],
                    )?;
                    let (following, built): (i64, i64) = {
                        let r = c.query_one(
                            "SELECT count(*) FILTER (WHERE NOT built), count(*) FILTER (WHERE built)
                             FROM line",
                            &[],
                        )?;
                        (r.get(0), r.get(1))
                    };
                    (
                        format!(
                            "library {kind} release {tag}: {} file(s), {} entries ({new_entries} \
                             new or changed); no line changed — {following} line(s) in design \
                             follow the latest (Update library pins moves them), {built} built \
                             line(s) stay pinned",
                            files.len(),
                            all_entries.len()
                        ),
                        format!("library:{kind}@{tag}"),
                        None,
                    )
                }
                LibraryCommand::WithdrawEntry {
                    kind,
                    file,
                    entry,
                    note,
                } => {
                    if !maintains(&me, kind, Some(file), Some(entry)) {
                        return Err(not_maintainer(kind));
                    }
                    let in_head = head(&mut c, kind)?.get(file).is_some_and(|t| {
                        country::entries(file, t)
                            .unwrap_or_default()
                            .iter()
                            .any(|e| e.id == *entry)
                    });
                    let released = c
                        .query_opt(
                            "SELECT 1 FROM lib_entry WHERE kind = $1 AND file = $2 AND entry = $3 LIMIT 1",
                            &[kind, file, entry],
                        )?
                        .is_some();
                    if !in_head && !released {
                        return Err(refused(format!("no entry {entry} in {kind}/{file}")));
                    }
                    let n = c.execute(
                        "INSERT INTO lib_withdrawn (kind, file, entry, note, by_user)
                         VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
                        &[kind, file, entry, note, &actor],
                    )?;
                    if n == 0 {
                        return Err(refused(format!("entry {entry} is withdrawn already")));
                    }
                    let uses: i64 = c
                        .query_one(
                            "SELECT count(*) FROM lib_use WHERE kind = $1 AND entry = $2
                               AND branch = 'main'",
                            &[kind, entry],
                        )?
                        .get(0);
                    (
                        format!(
                            "entry {entry} of {kind}/{file} withdrawn: the lines using it ({uses} \
                             use{}) keep it and are shown it; no object may start using it",
                            if uses == 1 { "" } else { "s" }
                        ),
                        format!("library:{kind}/{file}#{entry}"),
                        None,
                    )
                }
                LibraryCommand::RestoreEntry { kind, file, entry } => {
                    if !maintains(&me, kind, Some(file), Some(entry)) {
                        return Err(not_maintainer(kind));
                    }
                    let n = c.execute(
                        "DELETE FROM lib_withdrawn WHERE kind = $1 AND file = $2 AND entry = $3",
                        &[kind, file, entry],
                    )?;
                    if n == 0 {
                        return Err(refused(format!("entry {entry} is not withdrawn")));
                    }
                    (
                        format!("entry {entry} of {kind}/{file} back"),
                        format!("library:{kind}/{file}#{entry}"),
                        None,
                    )
                }
                LibraryCommand::SetBuilt { line, built } => {
                    if !me.allows(Permission::Lock, &Target::Line(line)) {
                        return Err(refused(format!(
                            "{actor} may not mark line {line} built (lock on the whole line)"
                        )));
                    }
                    let n =
                        c.execute("UPDATE line SET built = $2 WHERE id = $1", &[line, built])?;
                    if n == 0 {
                        return Err(refused(format!("the network has no line {line}")));
                    }
                    (
                        if *built {
                            format!(
                                "line {line} built: pinned to its library releases (moved only \
                                 by whoever reviews the whole line)"
                            )
                        } else {
                            format!("line {line} in design again: it follows the latest releases")
                        },
                        format!("line:{line}"),
                        None,
                    )
                }
            })
        })();
        let out = out.and_then(|(text, target, id)| {
            let mut v = serde_json::to_value(command).expect("serialises");
            if let Some(t) = v.get_mut("text") {
                *t = serde_json::Value::String(country::text_hash(t.as_str().unwrap_or("")));
            }
            let after = Some(v.to_string());
            c.execute(
                "INSERT INTO audit (user_name, action, target, after, detail)
                 VALUES ($1, $2, $3, ($4::text)::jsonb, $5)",
                &[&actor, &command.action(), &target, &after, &text],
            )?;
            bump(&mut c)?;
            Ok(Done { text, request: id })
        });
        match out {
            Ok(done) => {
                c.client.batch_execute("COMMIT")?;
                Ok(done)
            }
            Err(e) => {
                let _ = c.client.batch_execute("ROLLBACK");
                Err(e)
            }
        }
    }

    /// The network's library kinds `viewer` may see something of: locked,
    /// files, releases, maintainers.
    pub fn library_kinds(&self, viewer: &UserRights) -> Result<Vec<KindInfo>, NetError> {
        let data = self.rights()?;
        let mut c = lock(&self.conn);
        let mut kinds: BTreeSet<String> = BTreeSet::new();
        for q in [
            "SELECT DISTINCT kind FROM lib_file",
            "SELECT DISTINCT kind FROM lib_release",
            "SELECT kind FROM lib_kind",
        ] {
            for r in c.client.query(q, &[])? {
                kinds.insert(r.get(0));
            }
        }
        let mut out = Vec::new();
        for kind in kinds {
            if !viewer.allows(Permission::See, &country_target(&kind, None, None)) {
                continue;
            }
            let files = head(&mut c, &kind)?.len();
            let locked = kind_locked(&mut c, &kind)?;
            let releases = releases_of(&mut c, &kind)?;
            let mut maintainers: BTreeSet<String> = BTreeSet::new();
            for a in &data.assignments {
                if a.deny {
                    continue;
                }
                let Some((p, _)) = a.role.as_ref().and_then(|r| data.roles.get(r)) else {
                    continue;
                };
                let s = &a.scope;
                let of_network = s.line.is_none()
                    && s.route.is_none()
                    && s.region.is_none()
                    && s.object.is_none()
                    && s.kind.as_deref().is_none_or(|k| k == "library")
                    && s.library_kind.as_deref().is_none_or(|k| k == kind)
                    && s.library_file.is_none();
                if of_network && p.contains(Permission::Review) && p.contains(Permission::Lock) {
                    maintainers.insert(format!("{}:{}", a.subject.kind(), a.subject.name()));
                }
            }
            out.push(KindInfo {
                kind,
                locked,
                files,
                releases,
                maintainers: maintainers.into_iter().collect(),
            });
        }
        Ok(out)
    }

    /// The head files of `kind` with the entries `viewer` may see.
    pub fn library_files(
        &self,
        viewer: &UserRights,
        kind: &str,
    ) -> Result<Vec<FileInfo>, NetError> {
        let mut c = lock(&self.conn);
        let withdrawn = withdrawn_of(&mut c, kind)?;
        let mut out = Vec::new();
        let rows = c.query(
            "SELECT name, text, locked FROM lib_file WHERE kind = $1 ORDER BY name",
            &[&kind],
        )?;
        for r in rows {
            let (file, text, locked): (String, String, bool) = (r.get(0), r.get(1), r.get(2));
            if !viewer.allows(Permission::See, &country_target(kind, Some(&file), None)) {
                continue;
            }
            let mut entries = Vec::new();
            for e in country::entries(&file, &text).unwrap_or_default() {
                if !viewer.allows(
                    Permission::See,
                    &country_target(kind, Some(&file), Some(&e.id)),
                ) {
                    continue;
                }
                let first: Option<String> = c
                    .query_opt(
                        "SELECT release FROM lib_entry
                         WHERE kind = $1 AND file = $2 AND entry = $3 AND version = $4",
                        &[&kind, &file, &e.id, &e.version],
                    )?
                    .and_then(|r| r.get(0));
                let w = withdrawn.contains(&(file.clone(), e.id.clone()));
                entries.push((e.id, e.version[..12].to_owned(), first, w));
            }
            out.push(FileInfo {
                file,
                locked,
                entries,
            });
        }
        Ok(out)
    }

    /// The requests `viewer` may see: their own (drafts too), and those
    /// asked for of the entries they maintain; the line and routes of a
    /// request shown only to who may see something of the line.
    pub fn library_requests(&self, viewer: &UserRights) -> Result<Vec<Request>, NetError> {
        let rows = lock(&self.conn)
            .client
            .query(&format!("{REQUEST_SELECT} ORDER BY id"), &[])?;
        let mut out = Vec::new();
        for r in rows {
            let mut q = request_of(&r);
            let own = q.user == viewer.user() && viewer.active();
            let maintainer =
                q.state != "draft" && maintains(viewer, &q.kind, Some(&q.file), Some(&q.entry));
            if !(own || maintainer) {
                continue;
            }
            if !own && q.line.as_deref().is_some_and(|l| !sees_line(viewer, l)) {
                q.line = None;
                q.branch = None;
                q.routes.clear();
                q.reason = String::from("(for a line you have no access to)");
            }
            out.push(q);
        }
        Ok(out)
    }

    /// **The use index** for `viewer`: where entry `entry` of `kind` is
    /// used. An entry the viewer may not see is not found (the same words
    /// as an entry that is not there); a use on an object (or a file) the
    /// viewer may not see is left out, without a word; of an object's
    /// routes only those seen.
    pub fn uses(
        &self,
        viewer: &UserRights,
        kind: &str,
        entry: &str,
    ) -> Result<Vec<UseView>, NetError> {
        let not_found = || refused(format!("no entry {entry} of library {kind} you may see"));
        let file = {
            let mut c = lock(&self.conn);
            file_of_entry(&mut c, kind, entry)?
        };
        let Some(file) = file else {
            return Err(not_found());
        };
        if !viewer.allows(
            Permission::See,
            &country_target(kind, Some(&file), Some(entry)),
        ) {
            return Err(not_found());
        }
        // Objects read only on the lines the viewer may see objects of (a
        // prefilter: every row is decided all the same; files on every line).
        let lines: Vec<String> = if viewer.sees_everything() {
            Vec::new()
        } else {
            self.lines()?
                .into_iter()
                .map(|l| l.id)
                .filter(|l| viewer.sees_objects_of_line(l))
                .collect()
        };
        // Main's rows, then (phase 11) the branches' — placed as on main;
        // two statements: the branches' few rows do not widen main's sort.
        let mut rows = Vec::new();
        for on_main in [true, false] {
            rows.extend(lock(&self.conn).query(
                "SELECT u.line_id, u.object_kind, u.object, u.file, o.routes, s.x0, s.y0, s.x1,
                     s.y1, u.branch
                 FROM lib_use u
                 LEFT JOIN object o ON o.line_id = u.line_id AND o.branch = 'main'
                     AND o.kind = u.object_kind AND o.id = u.object
                 LEFT JOIN spatial s ON s.line_id = u.line_id AND s.branch = 'main'
                     AND s.kind = u.object_kind AND s.id = u.object
                 WHERE u.kind = $1 AND u.entry = $2 AND (u.branch = 'main') = $5
                   AND ($3 OR u.object_kind = 'library' OR u.line_id = ANY($4))
                 ORDER BY u.line_id, u.object_kind, u.object, u.branch",
                &[&kind, &entry, &viewer.sees_everything(), &lines, &on_main],
            )?);
        }
        let mut route_seen: BTreeMap<(String, String), bool> = BTreeMap::new();
        let mut out = Vec::new();
        for r in rows {
            let line: String = r.get(0);
            let object_kind: String = r.get(1);
            let object: String = r.get(2);
            let ufile: String = r.get(3);
            let branch: String = r.get(9);
            let routes: Vec<String> = r.get::<_, Option<Vec<String>>>(4).unwrap_or_default();
            let x0: Option<f64> = r.get(5);
            let bounds =
                x0.map(|x0| ohl_core::grid::Bounds::new([x0, r.get(6)], [r.get(7), r.get(8)]));
            let seen = if object_kind == "library" {
                viewer.allows(
                    Permission::See,
                    &Target::Library {
                        line: &line,
                        kind,
                        file: &ufile,
                        entry: Some(entry),
                    },
                )
            } else {
                let Ok(k) = Kind::parse(&object_kind) else {
                    continue;
                };
                viewer.allows(
                    Permission::See,
                    &Target::object(
                        &line,
                        k,
                        &object,
                        Placement {
                            bounds,
                            routes: &routes,
                        },
                    ),
                )
            };
            if !seen {
                continue;
            }
            let mut shown = Vec::new();
            for route in routes {
                let key = (line.clone(), route.clone());
                let ok = match route_seen.get(&key) {
                    Some(v) => *v,
                    None => {
                        let (b, _) = self.placement_of(&line, "route", &route)?;
                        let one = [route.clone()];
                        let v = viewer.allows(
                            Permission::See,
                            &Target::object(
                                &line,
                                Kind::Route,
                                &route,
                                Placement {
                                    bounds: b,
                                    routes: &one,
                                },
                            ),
                        );
                        route_seen.insert(key, v);
                        v
                    }
                };
                if ok {
                    shown.push(route);
                }
            }
            out.push(UseView {
                line,
                branch,
                object_kind,
                object,
                routes: shown,
            });
        }
        Ok(out)
    }

    /// The lines `viewer` may see something of (all with `line` none), each
    /// with its pins against the latest releases and the withdrawn entries
    /// it uses (those the viewer may see).
    pub fn library_status(
        &self,
        viewer: &UserRights,
        line: Option<&str>,
    ) -> Result<Vec<LineLibraries>, NetError> {
        let mut c = lock(&self.conn);
        let latest = latest_releases(&mut c)?;
        let mut order: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for k in latest.keys() {
            order.insert(k.clone(), releases_of(&mut c, k)?);
        }
        let withdrawn: Vec<(String, String, String)> = c
            .query("SELECT kind, file, entry FROM lib_withdrawn", &[])?
            .into_iter()
            .map(|r| (r.get(0), r.get(1), r.get(2)))
            .collect();
        let rows = c.query(
            "SELECT id, built, library_pins::text FROM line ORDER BY id",
            &[],
        )?;
        let mut out = Vec::new();
        for r in rows {
            let id: String = r.get(0);
            if line.is_some_and(|l| l != id) || !viewer.sees_some_of_line(&id) {
                continue;
            }
            let pins: BTreeMap<String, String> =
                serde_json::from_str(&r.get::<_, String>(2)).unwrap_or_default();
            let mut kinds = Vec::new();
            let mut all: BTreeSet<String> = latest.keys().cloned().collect();
            all.extend(pins.keys().cloned());
            for kind in all {
                let pinned = pins.get(&kind).cloned();
                let rel = order.get(&kind).cloned().unwrap_or_default();
                let behind = match &pinned {
                    Some(p) => rel
                        .iter()
                        .position(|t| t == p)
                        .map_or(0, |k| rel.len() - 1 - k),
                    None => rel.len(),
                };
                let mut withdrawn_used = Vec::new();
                for (wk, wf, we) in withdrawn.iter().filter(|w| w.0 == kind) {
                    // Seen in the network's library, or in the line's own
                    // copy of the file.
                    let seen = viewer
                        .allows(Permission::See, &country_target(wk, Some(wf), Some(we)))
                        || viewer.allows(
                            Permission::See,
                            &Target::Library {
                                line: &id,
                                kind: wk,
                                file: wf,
                                entry: Some(we),
                            },
                        );
                    if !seen {
                        continue;
                    }
                    let used = c
                        .query_opt(
                            "SELECT 1 FROM lib_use WHERE line_id = $1 AND branch = 'main'
                               AND kind = $2 AND entry = $3 LIMIT 1",
                            &[&id, wk, we],
                        )?
                        .is_some();
                    if used {
                        withdrawn_used.push(we.clone());
                    }
                }
                kinds.push(KindState {
                    latest: latest.get(&kind).cloned(),
                    kind,
                    pinned,
                    behind,
                    withdrawn_used,
                });
            }
            out.push(LineLibraries {
                line: id,
                built: r.get(1),
                kinds,
            });
        }
        Ok(out)
    }

    /// The lines in design (not built): they follow the latest releases.
    pub fn following_lines(&self) -> Result<Vec<String>, NetError> {
        Ok(lock(&self.conn)
            .query("SELECT id FROM line WHERE NOT built ORDER BY id", &[])?
            .into_iter()
            .map(|r| r.get(0))
            .collect())
    }

    /// Moves model `code`'s pins to `pins` (kind → release), the others
    /// kept (inside the caller's unit of work).
    pub fn set_pins(&self, code: &str, pins: &BTreeMap<String, String>) -> Result<(), NetError> {
        self.require_writer()?;
        let json = serde_json::to_string(pins).expect("serialises");
        lock(&self.conn).execute(
            "UPDATE line SET library_pins = library_pins || ($2::text)::jsonb WHERE id = $1",
            &[&code, &json],
        )?;
        Ok(())
    }

    /// Writes the use index of model `code`'s main branch: the rows of
    /// the objects `from` (object kind, object) go, `rows` come.
    pub fn write_uses(
        &self,
        code: &str,
        from: &[(String, String)],
        rows: &[UseRow],
    ) -> Result<(), NetError> {
        self.write_branch_uses(code, ohl_session::versions::MAIN, from, rows)
    }

    /// **A branch's use index** (phase 11): what the objects branch
    /// `branch` of model `code` changed use, as it holds them — only its
    /// changes, never a copy of main's rows. The rows of the objects
    /// `from` go, `rows` come.
    pub fn write_branch_uses(
        &self,
        code: &str,
        branch: &str,
        from: &[(String, String)],
        rows: &[UseRow],
    ) -> Result<(), NetError> {
        self.require_writer()?;
        let shared = self.shared();
        for chunk in from.chunks(BATCH) {
            let kinds: Vec<&str> = chunk.iter().map(|(k, _)| k.as_str()).collect();
            let ids: Vec<&str> = chunk.iter().map(|(_, i)| i.as_str()).collect();
            lock(&shared).execute(
                "DELETE FROM lib_use u USING unnest($2::text[], $3::text[]) AS v(k, i)
                 WHERE u.line_id = $1 AND u.branch = $4 AND u.object_kind = v.k
                   AND u.object = v.i",
                &[&code, &kinds, &ids, &branch],
            )?;
        }
        for chunk in rows.chunks(BATCH) {
            let ok: Vec<&str> = chunk.iter().map(|r| r.object_kind.as_str()).collect();
            let o: Vec<&str> = chunk.iter().map(|r| r.object.as_str()).collect();
            let k: Vec<&str> = chunk.iter().map(|r| r.kind.as_str()).collect();
            let f: Vec<&str> = chunk.iter().map(|r| r.file.as_str()).collect();
            let e: Vec<&str> = chunk.iter().map(|r| r.entry.as_str()).collect();
            lock(&shared).execute(
                "INSERT INTO lib_use (line_id, branch, object_kind, object, kind, file, entry)
                 SELECT $1, $7, a, b, c, d, e
                 FROM unnest($2::text[], $3::text[], $4::text[], $5::text[], $6::text[])
                     AS v(a, b, c, d, e)
                 ON CONFLICT DO NOTHING",
                &[&code, &ok, &o, &k, &f, &e, &branch],
            )?;
        }
        Ok(())
    }

    /// A branch's use index gone with it (phase 11).
    pub fn forget_branch_uses(&self, code: &str, branch: &str) -> Result<(), NetError> {
        self.require_writer()?;
        if branch == ohl_session::versions::MAIN {
            return Ok(());
        }
        lock(&self.conn).execute(
            "DELETE FROM lib_use WHERE line_id = $1 AND branch = $2",
            &[&code, &branch],
        )?;
        Ok(())
    }

    /// A branch's use index renamed with it (phase 11).
    pub fn rename_branch_uses(&self, code: &str, from: &str, to: &str) -> Result<(), NetError> {
        self.require_writer()?;
        lock(&self.conn).execute(
            "UPDATE lib_use SET branch = $3 WHERE line_id = $1 AND branch = $2",
            &[&code, &from, &to],
        )?;
        Ok(())
    }

    /// The use index of the files model `code`'s main branch holds, made
    /// again from them (kind, name, text): its rows of files go, `files`'
    /// come.
    pub fn write_file_uses(
        &self,
        code: &str,
        files: &[(String, String, String)],
    ) -> Result<(), NetError> {
        self.require_writer()?;
        lock(&self.conn).execute(
            "DELETE FROM lib_use WHERE line_id = $1 AND branch = 'main' AND object_kind = 'library'",
            &[&code],
        )?;
        self.write_uses(code, &[], &uses_of_files(files))
    }

    /// Whether model `code` has no row in the use index (imported before
    /// schema 5, or using nothing).
    pub fn uses_missing(&self, code: &str) -> Result<bool, NetError> {
        Ok(lock(&self.conn)
            .query_opt("SELECT 1 FROM lib_use WHERE line_id = $1 LIMIT 1", &[&code])?
            .is_none())
    }

    /// The use index of model `code` made again from its session (on its
    /// main branch): every element's uses and the entries of the files it
    /// picks as a whole. Returns the rows written.
    pub fn reindex_uses(&self, code: &str, session: &Session) -> Result<usize, NetError> {
        self.require_writer()?;
        lock(&self.conn).execute(
            "DELETE FROM lib_use WHERE line_id = $1 AND branch = 'main'",
            &[&code],
        )?;
        let rows = uses_of_session(session, None);
        self.write_uses(code, &[], &rows)?;
        Ok(rows.len())
    }
}

/// Batches of rows written in one statement.
const BATCH: usize = 2000;

/// The use rows of the files a line holds of the kinds it picks as a
/// whole (not named by id): each entry of each file.
pub fn uses_of_files(files: &[(String, String, String)]) -> Vec<UseRow> {
    let mut out = Vec::new();
    for (kind, name, text) in files {
        if REFERENCED_KINDS.contains(&kind.as_str()) {
            continue;
        }
        for e in country::entries(name, text).unwrap_or_default() {
            out.push(UseRow {
                object_kind: "library".into(),
                object: format!("{kind}/{name}"),
                kind: kind.clone(),
                file: name.clone(),
                entry: e.id,
            });
        }
    }
    out
}

/// The use rows of an element (kind, id, JSON).
pub fn uses_of_object(kind: Kind, id: &str, v: &serde_json::Value) -> Vec<UseRow> {
    country::uses_of(kind, v)
        .into_iter()
        .map(|(lib, entry)| UseRow {
            object_kind: kind.as_str().to_owned(),
            object: id.to_owned(),
            kind: lib.to_owned(),
            file: String::new(),
            entry,
        })
        .collect()
}

/// The use rows of `objects` of `session`'s line worked on (all of them
/// and its library files when `None`).
pub fn uses_of_session(session: &Session, objects: Option<&[(Kind, String)]>) -> Vec<UseRow> {
    let mut out = Vec::new();
    let keys: Vec<(Kind, String)> = match objects {
        Some(o) => o.to_vec(),
        None => session
            .object_keys()
            .filter(|(k, _)| USING_KINDS.contains(k))
            .cloned()
            .collect(),
    };
    for (kind, id) in keys {
        if !USING_KINDS.contains(&kind) {
            continue;
        }
        if let Some(v) = session.object_value(kind, &id) {
            out.extend(uses_of_object(kind, &id, &v));
        }
    }
    if objects.is_none()
        && let Ok(files) = session.line_library_all(session.current_line())
    {
        out.extend(uses_of_files(&files));
    }
    out
}

/// **Update library pins**, planned: what moving model `code`'s pins
/// (`kinds`, all kinds released when none) to `release` (one kind; the
/// latest when none) changes — the files put and removed (the release's;
/// a file of the line not in it removed when the release holds every
/// entry of it the line uses, kept when it is wholly the line's own; the
/// files a line picks by name kept), the entries changed, the routes
/// whose results go out of date. A kind is not moved (said why) when the
/// line uses an entry the release lacks. Nothing is changed.
pub fn plan_pins(
    net: &Network,
    session: &mut Session,
    code: &str,
    kinds: Option<&[String]>,
    release: Option<&str>,
) -> Result<PinPlan, NetError> {
    let main = ohl_session::versions::MAIN;
    if session.current_line() != main {
        return Err(refused("library pins are of the main branch"));
    }
    let shared = net.shared();
    let mut c = lock(&shared);
    let row = c
        .query_opt(
            "SELECT built, library_pins::text FROM line WHERE id = $1",
            &[&code],
        )?
        .ok_or_else(|| refused(format!("the network has no line {code}")))?;
    let built: bool = row.get(0);
    let pins: BTreeMap<String, String> =
        serde_json::from_str(&row.get::<_, String>(1)).unwrap_or_default();
    let latest = latest_releases(&mut c)?;
    let kinds: Vec<String> = match kinds {
        Some(k) => k.to_vec(),
        None => latest.keys().cloned().collect(),
    };
    if release.is_some() && kinds.len() != 1 {
        return Err(refused("a release is of one kind: name it"));
    }
    let withdrawn: BTreeSet<(String, String)> = c
        .query("SELECT kind, entry FROM lib_withdrawn", &[])?
        .into_iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    // What the line uses: kind → entry → objects.
    let mut used: BTreeMap<String, BTreeMap<String, Vec<(Kind, String)>>> = BTreeMap::new();
    let keys: Vec<(Kind, String)> = session
        .object_keys()
        .filter(|(k, _)| USING_KINDS.contains(k))
        .cloned()
        .collect();
    for (k, id) in &keys {
        if let Some(v) = session.object_value(*k, id) {
            for (lib, entry) in country::uses_of(*k, &v) {
                used.entry(lib.to_owned())
                    .or_default()
                    .entry(entry)
                    .or_default()
                    .push((*k, id.clone()));
            }
        }
    }
    let all_routes: Vec<String> = session.line().routes.iter().map(|r| r.id.clone()).collect();
    let mut out = PinPlan {
        line: code.to_owned(),
        built,
        kinds: Vec::new(),
    };
    for kind in kinds {
        let from = pins.get(&kind).cloned();
        let mut plan = KindPlan {
            kind: kind.clone(),
            from: from.clone(),
            to: String::new(),
            changes: Vec::new(),
            changed: Vec::new(),
            used_changed: Vec::new(),
            stale_routes: Vec::new(),
            kept: Vec::new(),
            withdrawn_used: Vec::new(),
            refused: None,
        };
        let to = match release
            .map(str::to_owned)
            .or_else(|| latest.get(&kind).cloned())
        {
            Some(t) => t,
            None => {
                plan.refused = Some(format!("library {kind} has no release"));
                out.kinds.push(plan);
                continue;
            }
        };
        plan.to = to.clone();
        if c.query_opt(
            "SELECT 1 FROM lib_release WHERE kind = $1 AND tag = $2",
            &[&kind, &to],
        )?
        .is_none()
        {
            plan.refused = Some(format!("library {kind} has no release {to}"));
            out.kinds.push(plan);
            continue;
        }
        let rf = release_files(&mut c, &kind, &to)?;
        let lf: BTreeMap<String, String> = session.library_files(&kind)?.into_iter().collect();
        let referenced = REFERENCED_KINDS.contains(&kind.as_str());
        let key = |file: &str, id: &str| {
            if referenced {
                id.to_owned()
            } else {
                format!("{file}:{id}")
            }
        };
        let entries_of = |files: &BTreeMap<String, String>| -> BTreeMap<String, String> {
            let mut m = BTreeMap::new();
            for (f, t) in files {
                for e in country::entries(f, t).unwrap_or_default() {
                    m.insert(key(f, &e.id), e.version);
                }
            }
            m
        };
        let r_entries = entries_of(&rf);
        for (f, t) in &rf {
            if lf.get(f) != Some(t) {
                plan.changes.push(FileChange {
                    file: f.clone(),
                    text: Some(t.clone()),
                    existed: lf.contains_key(f),
                });
            }
        }
        let empty = BTreeMap::new();
        let uses = used.get(&kind).unwrap_or(&empty);
        let mut after = rf.clone();
        for (f, t) in &lf {
            if rf.contains_key(f) {
                continue;
            }
            if !referenced {
                plan.kept.push(f.clone());
                after.insert(f.clone(), t.clone());
                continue;
            }
            let es = country::entries(f, t).unwrap_or_default();
            let missing: Vec<&country::Entry> = es
                .iter()
                .filter(|e| !r_entries.contains_key(&e.id))
                .collect();
            if missing.is_empty() {
                plan.changes.push(FileChange {
                    file: f.clone(),
                    text: None,
                    existed: true,
                });
            } else if missing.len() == es.len() {
                plan.kept.push(f.clone());
                after.insert(f.clone(), t.clone());
            } else {
                let used_missing: Vec<String> = missing
                    .iter()
                    .filter(|e| uses.contains_key(&e.id))
                    .map(|e| e.id.clone())
                    .collect();
                if used_missing.is_empty() {
                    plan.changes.push(FileChange {
                        file: f.clone(),
                        text: None,
                        existed: true,
                    });
                } else {
                    plan.refused = Some(format!(
                        "{kind}/{f} holds {} (used, not in release {to}) beside entries the \
                         release has: ask for {} first",
                        used_missing.join(", "),
                        if used_missing.len() == 1 {
                            "it"
                        } else {
                            "them"
                        }
                    ));
                }
            }
        }
        if plan.refused.is_none() && referenced {
            let have: BTreeSet<String> = entries_of(&after).into_keys().collect();
            let lacking: Vec<&String> = uses.keys().filter(|e| !have.contains(*e)).collect();
            // Only what the line had before and the release lacks: an
            // entry missing already is the line's own problem, not the
            // update's.
            let before: BTreeSet<String> = entries_of(&lf).into_keys().collect();
            let lost: Vec<String> = lacking
                .into_iter()
                .filter(|e| before.contains(*e))
                .cloned()
                .collect();
            if !lost.is_empty() {
                plan.refused = Some(format!(
                    "the line uses {}, which release {to} lacks",
                    lost.join(", ")
                ));
            }
        }
        if plan.refused.is_some() {
            plan.changes.clear();
            out.kinds.push(plan);
            continue;
        }
        let before = entries_of(&lf);
        let after_e = entries_of(&after);
        let names: BTreeSet<&String> = before.keys().chain(after_e.keys()).collect();
        for n in names {
            if before.get(n) != after_e.get(n) {
                plan.changed.push(n.clone());
            }
        }
        if referenced {
            let mut objects: Vec<(Kind, String)> = Vec::new();
            for n in &plan.changed {
                if let Some(objs) = uses.get(n) {
                    plan.used_changed.push(n.clone());
                    objects.extend(objs.iter().cloned());
                }
            }
            let mut routes: BTreeSet<String> = BTreeSet::new();
            for r in session.object_routes(&objects) {
                routes.extend(r);
            }
            plan.stale_routes = routes.into_iter().collect();
            plan.withdrawn_used = uses
                .keys()
                .filter(|e| withdrawn.contains(&(kind.clone(), (*e).clone())))
                .cloned()
                .collect();
        } else if !plan.changed.is_empty() {
            plan.used_changed = plan.changed.clone();
            plan.stale_routes = all_routes.clone();
        }
        out.kinds.push(plan);
    }
    Ok(out)
}

/// Whether `rights` may carry out `plan` on its line: edit (create,
/// delete) of each library file changed; on a built line, review of the
/// whole line too.
pub fn check_pin_rights(rights: &UserRights, plan: &PinPlan) -> Result<(), String> {
    let user = rights.user();
    let line = plan.line.as_str();
    if plan.built && !rights.allows(Permission::Review, &Target::Line(line)) {
        return Err(format!(
            "line {line} is built: its pins move only by whoever may review the whole line"
        ));
    }
    for k in plan.moving() {
        for ch in &k.changes {
            let p = match (&ch.text, ch.existed) {
                (None, _) => Permission::Delete,
                (Some(_), true) => Permission::Edit,
                (Some(_), false) => Permission::Create,
            };
            let t = Target::Library {
                line,
                kind: &k.kind,
                file: &ch.file,
                entry: None,
            };
            if !rights.allows(p, &t) {
                return Err(format!(
                    "{user} may not {} library {}/{} of line {line}",
                    p.as_str(),
                    k.kind,
                    ch.file
                ));
            }
        }
    }
    Ok(())
}

/// **Update library pins**, carried out by the command line (the
/// network's writer, no server running): `plan` checked by the rights,
/// then in one unit of work the session's library files changed (one log
/// entry, the routes marked out of date), the pins moved, the use index
/// of the files rewritten, the audit row. Returns what changed (`None`:
/// nothing moved).
pub fn apply_pins(
    net: &Network,
    session: &mut Session,
    rights: &UserRights,
    plan: &PinPlan,
) -> Result<Option<Applied>, NetError> {
    net.require_writer()?;
    if !plan.moves() {
        return Ok(None);
    }
    check_pin_rights(rights, plan).map_err(refused)?;
    let user = rights.user().to_owned();
    let code = plan.line.clone();
    net.begin_unit()?;
    let done = session.atomically(|s| -> Result<Option<Applied>, ohl_session::SessionError> {
        let applied = s.library_pins_update(&user, &plan.changes(), &plan.stale(), &plan.note())?;
        let fail = |e: NetError| ohl_session::SessionError::Store(e.to_string());
        net.set_pins(&code, &plan.pins()).map_err(fail)?;
        let files = s.line_library_all(ohl_session::versions::MAIN)?;
        let from: Vec<(String, String)> = plan
            .changes()
            .iter()
            .map(|(k, n, _)| ("library".to_owned(), format!("{k}/{n}")))
            .collect();
        let rows: Vec<UseRow> = uses_of_files(&files)
            .into_iter()
            .filter(|r| from.iter().any(|(_, o)| *o == r.object))
            .collect();
        net.write_uses(&code, &from, &rows).map_err(fail)?;
        net.audit(
            &user,
            "library.pins",
            &code,
            &serde_json::json!({
                "pins": plan.pins(),
                "files": plan.changes().len(),
                "stale_routes": plan.stale().len(),
            })
            .to_string(),
        )
        .map_err(fail)?;
        Ok(applied)
    });
    match done {
        Ok(a) => {
            net.commit_unit()?;
            Ok(a)
        }
        Err(e) => {
            let _ = net.roll_back_unit();
            Err(e.into())
        }
    }
}

/// How one entry of an imported project's library files maps onto the
/// network's library.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mapping {
    /// The kind.
    pub kind: String,
    /// The project's file.
    pub file: String,
    /// The entry.
    pub entry: String,
    /// `same` (the network's has it alike), `added` (into the network's
    /// file of that name: the kind not locked), `requested #N` (a locked
    /// kind: asked for), `differs, requested #N` (the network's differs:
    /// a change asked for, for a maintainer), or why it was left (a file
    /// that is not TOML, a draft).
    pub outcome: String,
}

/// The import's mapping of a project's library files onto the network's
/// library (inside the import's transaction): each entry the same,
/// added (an unlocked kind), or asked for (a locked kind, or an entry
/// that differs — never replaced silently). The model keeps its own
/// files; Update library pins later gives it the network's.
pub(crate) fn map_import(
    net: &Network,
    code: &str,
    user: &str,
    source: &str,
    files: &[(String, String, String)],
) -> Result<Vec<Mapping>, NetError> {
    let shared = net.shared();
    let mut c = lock(&shared);
    let mut out = Vec::new();
    let mut changed = false;
    for (kind, name, text) in files {
        let mut push = |entry: &str, outcome: String| {
            out.push(Mapping {
                kind: kind.clone(),
                file: name.clone(),
                entry: entry.to_owned(),
                outcome,
            });
        };
        if country::draft_of(name).is_some() {
            push("", "left: a draft's file".into());
            continue;
        }
        let entries = match country::entries(name, text) {
            Ok(e) => e,
            Err(e) => {
                push("", format!("left: {e}"));
                continue;
            }
        };
        for e in entries {
            // Phase 14: each file's entries known by its text's SHA-256;
            // only a file holding the entry read again for its version.
            let ids_now = head_entry_ids(&mut c, kind)?;
            let files: Vec<(String, BTreeSet<String>)> = ids_now
                .iter()
                .map(|(f, ids)| (f.clone(), ids.iter().cloned().collect()))
                .collect();
            // The matching shared with the join (`ohl_session::join`).
            let found = ohl_session::join::match_entry(kind, name, &e, &files, |f| {
                let t: String = c
                    .query_one(
                        "SELECT text FROM lib_file WHERE kind = $1 AND name = $2",
                        &[kind, &f],
                    )?
                    .get(0);
                Ok::<_, NetError>(
                    country::entries(f, &t)
                        .unwrap_or_default()
                        .into_iter()
                        .find(|x| x.id == e.id)
                        .map(|x| x.version),
                )
            })?;
            let draft = country::only_entry(name, text, &e.id).map_err(refused)?;
            let ask = |c: &mut Conn, file: &str, why: &str| -> Result<i64, NetError> {
                Ok(c.query_one(
                    "INSERT INTO lib_request (kind, file, entry, reason, state, user_name, text,
                         line_id)
                     VALUES ($1, $2, $3, $4, 'requested', $5, $6, $7) RETURNING id",
                    &[kind, &file, &e.id, &why, &user, &draft, &code],
                )?
                .get(0))
            };
            use ohl_session::join::EntryMatch;
            match &found {
                EntryMatch::Same(_) => push(&e.id, "same".into()),
                EntryMatch::Differs(f) => {
                    let id = ask(
                        &mut c,
                        f,
                        &format!("import of {source}: differs from the network's"),
                    )?;
                    changed = true;
                    push(&e.id, format!("differs, requested #{id}"));
                }
                EntryMatch::New | EntryMatch::NameTaken => {
                    let single_taken = found == EntryMatch::NameTaken;
                    if file_locked(&mut c, kind, name)? || single_taken {
                        let id = ask(&mut c, name, &format!("import of {source}"))?;
                        changed = true;
                        push(&e.id, format!("requested #{id}"));
                    } else {
                        let r = Request {
                            id: 0,
                            kind: kind.clone(),
                            file: name.clone(),
                            entry: e.id.clone(),
                            text: draft,
                            state: String::new(),
                            user: user.to_owned(),
                            line: None,
                            branch: None,
                            reason: String::new(),
                            routes: Vec::new(),
                            maintainer: None,
                            note: None,
                            created: String::new(),
                            decided: None,
                            released: None,
                        };
                        match lay_in(&mut c, &r, user) {
                            Ok(()) => {
                                changed = true;
                                push(&e.id, "added".into());
                            }
                            Err(NetError::Refused(why)) => push(&e.id, format!("left: {why}")),
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
        }
    }
    if changed {
        bump(&mut c)?;
    }
    Ok(out)
}
