// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Rights on a network's server (ADR-0013, phase 7): what the authority
//! keeps to decide with the session's one function
//! ([`ohl_session::rights`]) — for its commands (the edit side) and its
//! outgoing filter (the see side).
//!
//! - **Places**: where each object of a model is (plan bounds, routes) as
//!   of its last command — the state *before* the next one, so that a
//!   command is decided on what it changes as it was and as it is.
//! - **The edit side**: each [`Op`] has a rule ([`need`], an exhaustive
//!   match: a new operation cannot be added without one) checked before it
//!   runs; then, inside the same unit of work, every object it changed is
//!   decided — `create` for a new one (as it is), `delete` for one gone
//!   (as it was), `edit` for one changed (as it was and as it is); a
//!   refusal puts everything back (the session's unit, the database's
//!   savepoint).
//! - **The see side** ([`visible`]): a section is seen when it and every
//!   structure it hangs on are (its wires are drawn at their points); an
//!   alignment when it, its route and every point of it are. Tile
//!   packages are kept per rights class (the exact set of what a client
//!   may not see in the tile; none for most).
//! - **Error messages** ([`sanitise`]): a refusal naming an object the
//!   user may not see becomes [`HIDDEN_REFUSAL`] (no id, no place).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use ohl_core::grid::Bounds;
use ohl_session::rights::{HIDDEN_REFUSAL, Permission, Placement, Target, UserRights, words};
use ohl_session::{Kind, Role, RoleSource, Session};
use ohl_store_pg::library::LibraryState;

use crate::Op;

/// Where an object of a model is.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub(crate) bounds: Option<Bounds>,
    pub(crate) routes: Arc<[String]>,
}

/// Where every object of a model is, as of its last command.
#[derive(Debug, Default)]
pub(crate) struct Places {
    map: HashMap<(Kind, String), Place>,
}

/// The kinds with routes (the line's elements).
const ROUTED: [Kind; 7] = [
    Kind::Route,
    Kind::Alignment,
    Kind::AlignmentPoint,
    Kind::Structure,
    Kind::Section,
    Kind::Jumper,
    Kind::Spacer,
];

impl Places {
    /// Of every object of `s`'s line worked on.
    pub(crate) fn of_session(s: &mut Session) -> Self {
        let keys: Vec<(Kind, String)> = s.object_keys().cloned().collect();
        let mut p = Places::default();
        p.update(s, &keys);
        p
    }

    /// `objects` placed again as `s` holds them now (gone: removed).
    pub(crate) fn update(&mut self, s: &mut Session, objects: &[(Kind, String)]) {
        let routed: Vec<(Kind, String)> = objects
            .iter()
            .filter(|(k, id)| ROUTED.contains(k) && s.has_object(*k, id))
            .cloned()
            .collect();
        let routes = s.object_routes(&routed);
        let mut of: HashMap<(Kind, String), Arc<[String]>> = routed
            .into_iter()
            .zip(routes)
            .map(|(k, r)| (k, r.into_iter().collect::<Vec<_>>().into()))
            .collect();
        for key in objects {
            if !s.has_object(key.0, &key.1) {
                self.map.remove(key);
                continue;
            }
            let place = Place {
                bounds: s.spatial_entry(key.0, &key.1).map(|e| e.bounds),
                routes: of.remove(key).unwrap_or_else(|| Arc::from(Vec::new())),
            };
            self.map.insert(key.clone(), place);
        }
    }

    pub(crate) fn get(&self, kind: Kind, id: &str) -> Option<&Place> {
        self.map.get(&(kind, id.to_owned()))
    }

    /// How many objects are placed.
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Every object placed.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&(Kind, String), &Place)> {
        self.map.iter()
    }
}

/// The target of object `kind`/`id` of `line` placed at `place`.
pub(crate) fn target<'a>(
    line: &'a str,
    kind: Kind,
    id: &'a str,
    place: Option<&'a Place>,
) -> Target<'a> {
    Target::object(
        line,
        kind,
        id,
        match place {
            Some(p) => Placement {
                bounds: p.bounds,
                routes: &p.routes,
            },
            None => Placement::default(),
        },
    )
}

/// May `rights` see object `kind`/`id` of model `line` (placed at
/// `place`)?
pub(crate) fn sees(
    rights: &UserRights,
    line: &str,
    kind: Kind,
    id: &str,
    place: Option<&Place>,
) -> bool {
    rights.allows(Permission::See, &target(line, kind, id, place))
}

/// May `rights` see object `kind`/`id` of model `line` as drawn: the
/// object and what it refers to — a section with every structure it hangs
/// on, an alignment with its route and every point of it, an alignment
/// point with the structure it is tied to (its place is the structure's),
/// a jumper or a spacer with its sections and structures.
pub(crate) fn visible(
    rights: &UserRights,
    line: &str,
    s: &mut Session,
    places: &Places,
    kind: Kind,
    id: &str,
) -> bool {
    visible_to(rights, line, s, places, kind, id, 2)
}

fn visible_to(
    rights: &UserRights,
    line: &str,
    s: &mut Session,
    places: &Places,
    kind: Kind,
    id: &str,
    depth: u8,
) -> bool {
    if !sees(rights, line, kind, id, places.get(kind, id)) {
        return false;
    }
    if depth == 0 || !ROUTED.contains(&kind) {
        return true;
    }
    let refs = s.object_refs(kind, id);
    refs.iter().all(|(k, rid, _)| match k {
        Kind::AlignmentPoint => visible_to(rights, line, s, places, *k, rid, depth - 1),
        k if ROUTED.contains(k) => sees(rights, line, *k, rid, places.get(*k, rid)),
        _ => true,
    })
}

/// The users' compiled rights, shared with the models' sessions (their
/// roles): filled by the authority before it runs a user's command.
pub(crate) type Compiled = Arc<RwLock<HashMap<String, Arc<UserRights>>>>;

/// A model's users' roles on a network: from their rights
/// ([`UserRights::model_role`]).
pub(crate) struct ModelRoles {
    pub(crate) line: String,
    pub(crate) rights: Compiled,
}

impl RoleSource for ModelRoles {
    fn role(&self, user: &str) -> Option<Role> {
        self.rights
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(user)
            .and_then(|r| r.model_role(&self.line))
    }
}

/// What an operation needs before it runs (its objects are decided after,
/// in the same unit of work).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Need {
    /// Only its objects (decided after it ran).
    Objects,
    /// These permissions on the line whole.
    Line(&'static [Permission]),
    /// Edit of the line's settings.
    Settings,
    /// A library file: `create` (new) or `edit`, or `delete`.
    Library {
        kind: String,
        file: String,
        delete: bool,
    },
    /// Not on a network.
    Refused(&'static str),
    /// Phase 11: a commit — the objects it records (the line's changes
    /// since its revision) decided as edits of their own.
    Recorded,
    /// Phase 11: a branch over a working set made — its routes seen, each
    /// with something its maker may edit.
    WorkingSet,
    /// Phase 11: changes brought into a branch over a working set — its
    /// objects decided after, but those now as on main (main's changes,
    /// decided there).
    IntoBranch,
}

/// Phase 11: who may act on a branch over a working set.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Member {
    /// Its maker only (deleting, renaming, describing it).
    Maker(String),
    /// Its maker or its team (its commands, commits, reviews, merges).
    Team(String),
}

const AT_LARGE: &[Permission] = &[Permission::Edit, Permission::Create, Permission::Delete];
const EDIT: &[Permission] = &[Permission::Edit];

/// The rule of each operation (every operation listed: a new one cannot be
/// added without a rule).
pub(crate) fn need(op: &Op) -> Need {
    match op {
        Op::Do { .. } | Op::Undo | Op::Redo => Need::Objects,
        // The working state at large.
        Op::Rollback { .. }
        | Op::RollbackAfter { .. }
        | Op::RollBackLine { .. }
        | Op::BringIn { .. }
        | Op::Merge { .. }
        | Op::Revert { .. }
        | Op::MergeReview { .. } => Need::Line(AT_LARGE),
        // Project lines (branches) and revisions: copies of the line whole.
        Op::CreateLine { .. }
        | Op::DeleteLine { .. }
        | Op::RenameLine { .. }
        | Op::SetLineDescription { .. }
        | Op::Commit { .. }
        | Op::CreateBranch { .. }
        | Op::Tag { .. }
        | Op::DeleteRef { .. }
        | Op::SubmitReview { .. } => Need::Line(EDIT),
        // Decided by [`need_on`] (its routes).
        Op::CreateWorkingSetBranch { .. } => Need::WorkingSet,
        Op::ProtectBranch { .. } => Need::Line(&[Permission::Lock]),
        Op::DecideReview { .. } => Need::Line(&[Permission::Review]),
        Op::SetSetting { .. } => Need::Settings,
        Op::PutLibrary { kind, name, .. } => Need::Library {
            kind: kind.clone(),
            file: name.clone(),
            delete: false,
        },
        Op::DeleteLibrary { kind, name } => Need::Library {
            kind: kind.clone(),
            file: name.clone(),
            delete: true,
        },
        Op::SetUser { .. } | Op::Grant { .. } | Op::Revoke { .. } => Need::Refused(
            "on a network, users and their rights are the network's (ohl network rights …, by \
             an admin)",
        ),
        // Phase 9: the server plans it (and checks its plan: each file's
        // edit, a built line's review) and carries it out as LibraryPins,
        // whose files are decided after as any object.
        Op::UpdateLibraryPins { .. } => Need::Refused(
            "Update library pins is carried out by the network's server as it plans it",
        ),
        Op::LibraryPins { .. } => Need::Objects,
    }
}

/// Phase 9: what the libraries need of an operation on project line
/// `branch` of model `line` before it runs — a library file of a locked
/// kind or a draft by [`LibraryState::check_file`]; a branch holding a
/// pending draft not submitted for review nor merged into main.
pub(crate) fn check_library_need(
    lib: &LibraryState,
    user: &str,
    line: &str,
    branch: &str,
    op: &Op,
    s: &Session,
) -> Result<(), String> {
    let main = ohl_session::versions::MAIN;
    let blocked = |from: &str| -> Result<(), String> {
        // A tag or a revision: its library files are decided as they come
        // in (a draft reaching main pending is refused then).
        let Ok(files) = s.line_library_all(from) else {
            return Ok(());
        };
        let pending = lib.pending_drafts(&files);
        if pending.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "project line {from} holds pending library drafts ({}): it is not reviewed or \
                 merged into {main} until they are granted, or taken out of it",
                pending.join(", ")
            ))
        }
    };
    match op {
        Op::PutLibrary { kind, name, text } => {
            lib.check_file(user, line, branch, kind, name, Some(text))
        }
        Op::DeleteLibrary { kind, name } => lib.check_file(user, line, branch, kind, name, None),
        Op::SubmitReview { branch: from, .. } => blocked(from),
        Op::Merge { other, .. } if branch == main => blocked(other),
        Op::BringIn { from, .. } if branch == main => blocked(from),
        _ => Ok(()),
    }
}

/// Phase 9: how the objects a command changed are decided for the
/// libraries — its library files by [`LibraryState::check_file`] (but
/// Update library pins', the server's own), and no element starting to
/// use a withdrawn entry.
pub(crate) struct LibCheck<'a> {
    pub(crate) state: &'a LibraryState,
    pub(crate) branch: &'a str,
    pub(crate) pins: bool,
    /// What each element of main used before the command.
    pub(crate) before: &'a UsesCache,
}

/// What each element of a model's main branch uses of the libraries
/// (library kind, entry), those using something only.
pub(crate) type UsesCache = HashMap<(Kind, String), Vec<(&'static str, String)>>;

const USING: [Kind; 4] = [Kind::Section, Kind::Jumper, Kind::Structure, Kind::Spacer];

/// The uses of every element of `s`'s line worked on.
pub(crate) fn uses_cache(s: &Session) -> UsesCache {
    let keys: Vec<(Kind, String)> = s
        .object_keys()
        .filter(|(k, _)| USING.contains(k))
        .cloned()
        .collect();
    let mut out = UsesCache::new();
    for (k, id) in keys {
        if let Some(v) = s.object_value(k, &id) {
            let u = ohl_session::country::uses_of(k, &v);
            if !u.is_empty() {
                out.insert((k, id), u);
            }
        }
    }
    out
}

/// The elements among `objects` whose uses changed, `cache` brought up
/// to date.
pub(crate) fn uses_changed(
    s: &Session,
    cache: &mut UsesCache,
    objects: &[(Kind, String)],
) -> Vec<(Kind, String)> {
    let mut out = Vec::new();
    for key in objects {
        if !USING.contains(&key.0) {
            continue;
        }
        let now = s
            .object_value(key.0, &key.1)
            .map(|v| ohl_session::country::uses_of(key.0, &v))
            .unwrap_or_default();
        let was = cache.get(key).cloned().unwrap_or_default();
        if now != was {
            if now.is_empty() {
                cache.remove(key);
            } else {
                cache.insert(key.clone(), now);
            }
            out.push(key.clone());
        }
    }
    out
}

/// Phase 11 (the owner's questions of phase 7, decided conservatively):
/// the rule of `op` on project line `branch` of a model, branches over a
/// working set considered — **a commit needs edit only on what it
/// records**; a branch over a working set is made by whoever may edit on
/// each of its routes, its commands, commits, reviews and merges are its
/// maker's and its team's, deleting, renaming or describing it its
/// maker's; merging it into main needs only what the merge changes there;
/// changes brought into it from main are main's. Tags, branches without a
/// working set, rollbacks, reverts and reviews' merges need the line
/// whole, as before.
pub(crate) fn need_on(op: &Op, branch: &str, s: &Session) -> (Need, Option<Member>) {
    let main = ohl_session::versions::MAIN;
    let scoped = |b: &str| s.branch_scope(b).is_some();
    let team = |b: &str| scoped(b).then(|| Member::Team(b.to_owned()));
    match op {
        Op::Do { .. } | Op::Undo | Op::Redo => (Need::Objects, team(branch)),
        Op::Commit { .. } => (Need::Recorded, team(branch)),
        Op::DeleteLine { name } | Op::DeleteRef { name, .. } if scoped(name) => {
            (Need::Objects, Some(Member::Maker(name.clone())))
        }
        Op::RenameLine { from: name, .. } | Op::SetLineDescription { line: name, .. }
            if scoped(name) =>
        {
            (Need::Objects, Some(Member::Maker(name.clone())))
        }
        Op::SubmitReview { branch: b, .. } if scoped(b) => (Need::Objects, team(b)),
        Op::Merge { other, .. } if scoped(branch) => {
            let _ = other;
            (Need::IntoBranch, team(branch))
        }
        Op::BringIn { .. } if scoped(branch) => (Need::IntoBranch, team(branch)),
        Op::Merge { other: b, .. } | Op::BringIn { from: b, .. } if branch == main && scoped(b) => {
            (Need::Objects, None)
        }
        _ => (need(op), None),
    }
}

/// Whether `user` may act on branch-over-a-working-set `m` of `s`: its
/// maker, or (for its work) a user named in its team or in one of its
/// groups (`data`).
pub(crate) fn check_member(
    data: &ohl_session::rights::RightsData,
    s: &Session,
    m: &Member,
    user: &str,
) -> Result<(), String> {
    let (name, team_too) = match m {
        Member::Maker(n) => (n, false),
        Member::Team(n) => (n, true),
    };
    let Some(scope) = s.branch_scope(name) else {
        return Ok(());
    };
    if scope.by == user {
        return Ok(());
    }
    if team_too {
        let groups = data.groups_of(user);
        if scope
            .team
            .iter()
            .any(|t| t == user || groups.contains(t.as_str()))
        {
            return Ok(());
        }
    }
    Err(format!(
        "branch {name} is {}'s{}: {user} may not work on it",
        scope.by,
        if team_too { " and its team's" } else { "" }
    ))
}

/// Phase 11: the objects a commit on project line `branch` records — its
/// changes since its revision — decided as edits (`create` for one the
/// revision lacks, `delete` for one gone, `edit` for one changed; places
/// as the server keeps them, main's, else the object's own on the
/// branch).
pub(crate) fn check_recorded(
    rights: &UserRights,
    line: &str,
    s: &mut Session,
    branch: &str,
    places: &Places,
) -> Result<(), String> {
    let changed = s.uncommitted(branch).map_err(|e| e.to_string())?;
    if changed.is_empty() {
        return Ok(());
    }
    let user = rights.user();
    s.with_line(branch, |s| {
        let keys: Vec<(Kind, String)> = changed.iter().map(|(k, _, _)| k.clone()).collect();
        let mut here = Places::default();
        here.update(s, &keys);
        for ((kind, id), was, is) in &changed {
            let place = places.get(*kind, id).or_else(|| here.get(*kind, id));
            let p = match (was, is) {
                (false, true) => Permission::Create,
                (true, false) => Permission::Delete,
                _ => Permission::Edit,
            };
            let t = target(line, *kind, id, place);
            if !rights.allows(p, &t) {
                return Err(ohl_session::SessionError::Refused(
                    if rights.allows(Permission::See, &t) {
                        format!(
                            "{user} may not commit: it records {} of {} {id} (no assignment \
                             gives it there)",
                            p.as_str(),
                            kind.as_str()
                        )
                    } else {
                        HIDDEN_REFUSAL.to_owned()
                    },
                ));
            }
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// Phase 11: may `rights` make a branch over `routes` of model `line`?
/// Each route seen, with at least one object on it they may edit (the
/// places as the server keeps them).
pub(crate) fn check_working_set(
    rights: &UserRights,
    line: &str,
    routes: &[String],
    places: &Places,
) -> Result<(), String> {
    let user = rights.user();
    for r in routes {
        let seen = sees(rights, line, Kind::Route, r, places.get(Kind::Route, r));
        let editable = seen
            && places.iter().any(|((k, id), p)| {
                p.routes.iter().any(|x| x == r)
                    && rights.allows(Permission::Edit, &target(line, *k, id, Some(p)))
            });
        if !seen {
            return Err(HIDDEN_REFUSAL.to_owned());
        }
        if !editable {
            return Err(format!(
                "{user} may edit nothing of route {r}: a branch over it is not theirs to make"
            ));
        }
    }
    Ok(())
}

/// Decides operation `op` of `rights` on model `line` before it runs, by
/// its rule `need` ([`need_on`]).
pub(crate) fn check_need_with(
    rights: &UserRights,
    line: &str,
    op: &Op,
    s: &Session,
    need: Need,
) -> Result<(), String> {
    let user = rights.user();
    match need {
        Need::Recorded | Need::WorkingSet | Need::IntoBranch => {
            if rights.model_role(line).is_none() {
                return Err(format!("{user} has no rights on line {line}"));
            }
            Ok(())
        }
        Need::Objects => {
            if rights.model_role(line).is_none() {
                return Err(format!("{user} has no rights on line {line}"));
            }
            Ok(())
        }
        Need::Line(perms) => {
            for p in perms {
                if !rights.allows(*p, &Target::Line(line)) {
                    return Err(format!(
                        "{user} may not {}: it needs {} on line {line} whole",
                        op.describe(),
                        p.as_str()
                    ));
                }
            }
            Ok(())
        }
        Need::Settings => {
            let t = target(line, Kind::Meta, "line", None);
            if rights.allows(Permission::Edit, &t) {
                Ok(())
            } else {
                Err(format!("{user} may not change line {line}'s settings"))
            }
        }
        Need::Library { kind, file, delete } => {
            let id = format!("{kind}/{file}");
            let exists = s.has_object(Kind::Library, &id);
            let p = match (delete, exists) {
                (true, _) => Permission::Delete,
                (false, true) => Permission::Edit,
                (false, false) => Permission::Create,
            };
            let t = Target::Library {
                line,
                kind: &kind,
                file: &file,
                entry: None,
            };
            if rights.allows(p, &t) {
                Ok(())
            } else if rights.allows(Permission::See, &t) {
                Err(format!(
                    "{user} may not {} library {kind}/{file} of line {line}",
                    p.as_str()
                ))
            } else {
                Err(HIDDEN_REFUSAL.to_owned())
            }
        }
        Need::Refused(why) => Err(why.to_owned()),
    }
}

/// Decides every object a command changed (`changed`), on `s` as it is
/// now (after) and `before` as it was: `create` for a new one, `delete`
/// for one gone, `edit` for one changed (both places).
///
/// Phase 11 (the owner's question 1 of phase 7): a new object refused by
/// the rights as they were is decided again by `again` — the user's
/// rights with their parts of routes worked out on the line as it is
/// after the command — so a designer of a part of a route may make new
/// objects inside it.
pub(crate) fn check_objects(
    rights: &UserRights,
    line: &str,
    s: &mut Session,
    before: &Places,
    changed: &[(Kind, String)],
    lib: Option<&LibCheck>,
    again: Option<&dyn Fn(&mut Session) -> UserRights>,
) -> Result<(), String> {
    let mut after = Places::default();
    after.update(s, changed);
    let user = rights.user();
    if let Some(lib) = lib {
        check_library_objects(lib, user, line, s, changed)?;
    }
    let mut after_rights: Option<UserRights> = None;
    for (kind, id) in changed {
        let b = before.get(*kind, id);
        let existed = b.is_some() || (*kind == Kind::Meta);
        let a = after.get(*kind, id);
        if !existed && a.is_some() {
            let t = target(line, *kind, id, a);
            if rights.allows(Permission::Create, &t) {
                continue;
            }
            if let Some(again) = again {
                let r = after_rights.get_or_insert_with(|| again(s));
                if r.allows(Permission::Create, &t) {
                    continue;
                }
            }
        }
    }
    for (kind, id) in changed {
        let b = before.get(*kind, id);
        let existed = b.is_some() || (*kind == Kind::Meta);
        let a = after.get(*kind, id);
        let exists = a.is_some();
        let decided = |p: Permission, place: Option<&Place>| -> Result<(), String> {
            let t = target(line, *kind, id, place);
            if rights.allows(p, &t) {
                Ok(())
            } else if rights.allows(Permission::See, &t) {
                Err(format!(
                    "{user} may not {} {} {id} of line {line} (no assignment gives it there)",
                    p.as_str(),
                    kind.as_str()
                ))
            } else {
                Err(HIDDEN_REFUSAL.to_owned())
            }
        };
        match (existed, exists) {
            (false, true) => {
                let t = target(line, *kind, id, a);
                let made = rights.allows(Permission::Create, &t)
                    || after_rights
                        .as_ref()
                        .is_some_and(|r| r.allows(Permission::Create, &t));
                if !made {
                    decided(Permission::Create, a)?;
                }
            }
            (true, false) => decided(Permission::Delete, b)?,
            (true, true) => {
                decided(Permission::Edit, b.or(a))?;
                decided(Permission::Edit, a)?;
            }
            (false, false) => {}
        }
    }
    Ok(())
}

/// The libraries' side of the objects a command changed ([`LibCheck`]).
fn check_library_objects(
    lib: &LibCheck,
    user: &str,
    line: &str,
    s: &Session,
    changed: &[(Kind, String)],
) -> Result<(), String> {
    for (kind, id) in changed {
        let value = s.object_value(*kind, id);
        match kind {
            Kind::Library if !lib.pins => {
                let Some((k, name)) = id.split_once('/') else {
                    continue;
                };
                let text = value
                    .as_ref()
                    .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_owned));
                lib.state
                    .check_file(user, line, lib.branch, k, name, text.as_deref())?;
            }
            Kind::Section | Kind::Jumper | Kind::Structure | Kind::Spacer => {
                let Some(v) = value else {
                    continue;
                };
                let uses = ohl_session::country::uses_of(*kind, &v);
                for (k, entry) in uses {
                    if !lib.state.withdrawn.contains(&(k.to_owned(), entry.clone())) {
                        continue;
                    }
                    let before = lib
                        .before
                        .get(&(*kind, id.clone()))
                        .is_some_and(|u| u.iter().any(|(a, b)| *a == k && *b == entry));
                    if !before {
                        return Err(format!(
                            "entry {entry} of library {k} is withdrawn: {} {id} may not start \
                             using it (pick another)",
                            kind.as_str()
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// A message for `rights`: refused as [`HIDDEN_REFUSAL`] when it names an
/// object (of any model in `models`) the user may not see.
pub(crate) fn sanitise<'a>(
    rights: &UserRights,
    text: &str,
    models: impl Iterator<Item = (&'a str, &'a Places)> + Clone,
) -> String {
    if rights.sees_everything() {
        return text.to_owned();
    }
    for w in words(text) {
        for (line, places) in models.clone() {
            for kind in ROUTED {
                if let Some(p) = places.get(kind, w)
                    && !sees(rights, line, kind, w, Some(p))
                {
                    return format!("refused: {HIDDEN_REFUSAL}");
                }
            }
        }
    }
    text.to_owned()
}

/// The objects a change touched, as kept in the stream (`kind/id`).
pub(crate) fn object_keys(objects: &[(Kind, String)]) -> Vec<String> {
    objects
        .iter()
        .map(|(k, id)| format!("{}/{id}", k.as_str()))
        .collect()
}

/// A route's part: line, route, from structure, to structure.
pub(crate) type PartKey = (String, String, String, String);

/// What a route's part holds.
pub(crate) type Members = Arc<std::collections::HashSet<(Kind, String)>>;

/// The route parts of a set of assignments, resolved (the server's
/// [`ohl_session::rights::Resolve`]).
pub(crate) struct Resolved {
    pub(crate) parts: BTreeMap<PartKey, Members>,
    pub(crate) regions: BTreeMap<String, std::collections::BTreeSet<String>>,
}

impl ohl_session::rights::Resolve for Resolved {
    fn route_part(
        &self,
        line: &str,
        route: &str,
        from: &str,
        to: &str,
    ) -> Option<std::collections::HashSet<(Kind, String)>> {
        self.parts
            .get(&(line.into(), route.into(), from.into(), to.into()))
            .map(|m| (**m).clone())
    }

    fn region_lines(&self, region: &[[f64; 2]]) -> std::collections::BTreeSet<String> {
        self.regions
            .get(&serde_json::to_string(region).unwrap_or_default())
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every operation has a rule; the rules of the operations at large
    /// need the line whole, those of users and grants are refused.
    #[test]
    fn every_operation_has_a_rule() {
        let ops = [
            Op::Undo,
            Op::Redo,
            Op::Rollback { seq: 1 },
            Op::Merge {
                other: "b".into(),
                message: None,
                choices: Default::default(),
            },
            Op::SetSetting {
                key: "k".into(),
                value: None,
            },
            Op::SetUser {
                name: "x".into(),
                role: Role::Admin,
            },
            Op::ProtectBranch {
                name: "main".into(),
                protected: true,
            },
            Op::DecideReview {
                id: 1,
                approve: true,
                comment: String::new(),
            },
        ];
        let rules: Vec<Need> = ops.iter().map(need).collect();
        assert_eq!(rules[0], Need::Objects);
        assert_eq!(rules[2], Need::Line(AT_LARGE));
        assert_eq!(rules[3], Need::Line(AT_LARGE));
        assert_eq!(rules[4], Need::Settings);
        assert!(matches!(rules[5], Need::Refused(_)));
        assert_eq!(rules[6], Need::Line(&[Permission::Lock]));
        assert_eq!(rules[7], Need::Line(&[Permission::Review]));
        // Phase 9: Update library pins planned by the server; what it
        // carries out decided by its objects.
        assert!(matches!(
            need(&Op::UpdateLibraryPins {
                kinds: Vec::new(),
                release: None
            }),
            Need::Refused(_)
        ));
        assert_eq!(
            need(&Op::LibraryPins {
                pins: Default::default(),
                changes: Vec::new(),
                stale: Default::default(),
                note: String::new(),
            }),
            Need::Objects
        );
    }
}
