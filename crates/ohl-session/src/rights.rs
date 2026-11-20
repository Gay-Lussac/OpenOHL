// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Rights and roles on a network (ADR-0013, phase 7): **the one function**
//! deciding what a user may do, for the server's commands (the edit side)
//! and its outgoing filter (the see side: tiles, changes, rows, errors).
//!
//! - A **role** is a named set of [`Permission`]s; six are shipped
//!   ([`SHIPPED_ROLES`]), others an admin defines.
//! - An **assignment** gives a subject (a user or a group) a role on a
//!   [`Scope`] — or, set by a network admin only, **denies** a role's
//!   permissions (every permission when no role is named) on it.
//! - A **scope** is a conjunction of constraints: none is the whole
//!   network; then a line, a route (whole, or the part between two of its
//!   structures, snapped out to whole tension sections as a working set),
//!   a region (a polygon in the network's coordinates), an object kind, an
//!   object, a library kind, file or entry — "the costs of line B" is a
//!   line and a library kind.
//!
//! **Evaluation** ([`UserRights::allows`]): nothing without a grant
//! (default deny); a user's permissions on a [`Target`] are the union of
//! those of their own and their groups' assignments whose scope covers it;
//! `edit`, `create`, `delete`, `review` and `export` need `see` too; a deny
//! covering the target removes its permissions whatever grants it — the
//! order of assignments never matters (tested). A disabled user has no
//! rights at all.
//!
//! **On the safe side** where a scope meets a target only in part:
//! - a region *grants* what lies wholly inside it and *denies* what it
//!   touches (an object's plan bounds);
//! - a target without a place (a line's settings, a library file, a
//!   report) is not in any region or route; a grant on a route, a region or
//!   an object lets its user *see* the settings and library files of the
//!   lines it lies on (a section is solved with them), nothing more;
//! - the line as a whole ([`Target::Line`]: merges, branches, rollbacks,
//!   following a whole model) is granted only by a scope of the whole line
//!   or the network, and denied by any deny that may touch the line;
//! - the network itself ([`Target::Network`]: users, groups, roles,
//!   assignments, the audit) only by a scope of the network.
//!
//! What resolves scopes against a model (the members of a route's part,
//! the lines a region lies on) is the caller's: [`Resolve`].

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use ohl_core::grid::Bounds;
use serde::{Deserialize, Serialize};

use crate::{Kind, Role};

/// What a role may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// See (tiles, rows, changes, properties, results).
    See,
    /// Change what exists.
    Edit,
    /// Make new objects or files.
    Create,
    /// Remove objects or files.
    Delete,
    /// Approve or reject a review.
    Review,
    /// Export (a model as a project file).
    Export,
    /// Lock (protect a branch; library locks with phase 9).
    Lock,
    /// Request library additions (phase 9).
    Request,
    /// Give and take rights on the scope.
    ManageRights,
}

impl Permission {
    /// Every permission, in order.
    pub const ALL: [Permission; 9] = [
        Permission::See,
        Permission::Edit,
        Permission::Create,
        Permission::Delete,
        Permission::Review,
        Permission::Export,
        Permission::Lock,
        Permission::Request,
        Permission::ManageRights,
    ];

    /// As in the database and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::See => "see",
            Self::Edit => "edit",
            Self::Create => "create",
            Self::Delete => "delete",
            Self::Review => "review",
            Self::Export => "export",
            Self::Lock => "lock",
            Self::Request => "request",
            Self::ManageRights => "manage_rights",
        }
    }

    /// The permission named `s` (`None`: no such permission).
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// Needs `see` on the same target (ADR-0013).
    pub fn needs_see(self) -> bool {
        matches!(
            self,
            Self::Edit | Self::Create | Self::Delete | Self::Review | Self::Export
        )
    }

    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// A set of permissions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Perms(u16);

impl Perms {
    /// None.
    pub const NONE: Perms = Perms(0);
    /// Every permission.
    pub const ALL: Perms = Perms((1 << 9) - 1);

    /// Of a list.
    pub fn of(list: &[Permission]) -> Self {
        Perms(list.iter().fold(0, |a, p| a | p.bit()))
    }

    /// Holds `p`.
    pub fn contains(self, p: Permission) -> bool {
        self.0 & p.bit() != 0
    }

    /// Both together.
    pub fn union(self, o: Perms) -> Perms {
        Perms(self.0 | o.0)
    }

    /// Every permission of `o` is in this set.
    pub fn covers(self, o: Perms) -> bool {
        self.0 & o.0 == o.0
    }

    /// Empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Its permissions, in order.
    pub fn list(self) -> Vec<Permission> {
        Permission::ALL
            .into_iter()
            .filter(|p| self.contains(*p))
            .collect()
    }

    /// Names, comma separated.
    pub fn describe(self) -> String {
        let v: Vec<&str> = self.list().into_iter().map(Permission::as_str).collect();
        if v.is_empty() {
            "nothing".into()
        } else {
            v.join(", ")
        }
    }
}

/// The shipped roles and their permissions (ADR-0013; the network schema
/// makes the same rows, tested).
pub const SHIPPED_ROLES: [(&str, &[Permission]); 6] = {
    use Permission::*;
    [
        ("viewer", &[See]),
        ("designer", &[See, Edit, Create, Delete, Export, Request]),
        ("checker", &[See, Review, Export]),
        (
            "library_maintainer",
            &[See, Edit, Create, Delete, Review, Lock],
        ),
        (
            "line_admin",
            &[
                See,
                Edit,
                Create,
                Delete,
                Review,
                Export,
                Lock,
                Request,
                ManageRights,
            ],
        ),
        (
            "network_admin",
            &[
                See,
                Edit,
                Create,
                Delete,
                Review,
                Export,
                Lock,
                Request,
                ManageRights,
            ],
        ),
    ]
};

/// The shipped role a project role maps to when a project comes into a
/// network (the import's rule).
pub fn network_role(role: Role) -> &'static str {
    match role {
        Role::Viewer => "viewer",
        Role::Designer => "designer",
        Role::Admin => "line_admin",
    }
}

/// Who an assignment is for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum Subject {
    /// A user of the network.
    User(String),
    /// A group of users.
    Group(String),
}

impl Subject {
    /// `user:NAME` or `group:NAME`.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.split_once(':') {
            Some(("user", n)) if !n.trim().is_empty() => Ok(Self::User(n.trim().into())),
            Some(("group", n)) if !n.trim().is_empty() => Ok(Self::Group(n.trim().into())),
            _ => Err(format!("\"{s}\": a subject is user:NAME or group:NAME")),
        }
    }

    /// `user` or `group`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Group(_) => "group",
        }
    }

    /// Its name.
    pub fn name(&self) -> &str {
        match self {
            Self::User(n) | Self::Group(n) => n,
        }
    }
}

impl std::fmt::Display for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind(), self.name())
    }
}

/// What an assignment covers: every constraint given must hold (none: the
/// whole network).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    /// A line (model) by code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// A route by id (unique in the network).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// With `to_structure`: the part of `route` between two of its
    /// structures, snapped out to whole tension sections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_structure: Option<String>,
    /// See `from_structure`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_structure: Option<String>,
    /// A region: a polygon (x, y) in the network's coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Vec<[f64; 2]>>,
    /// An object kind (`structure`, `section`, …, `meta` for the line's
    /// settings, `library` for every library file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// One object by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// A library kind (`cables`, `costs`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_kind: Option<String>,
    /// A library file (with its kind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_file: Option<String>,
    /// A library entry (with its file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_entry: Option<String>,
}

impl Scope {
    /// The whole network.
    pub fn network() -> Self {
        Self::default()
    }

    /// One line, whole.
    pub fn line(code: &str) -> Self {
        Self {
            line: Some(code.into()),
            ..Self::default()
        }
    }

    /// The whole network (no constraint).
    pub fn is_network(&self) -> bool {
        *self == Self::default()
    }

    /// The scope's kind, as the database keeps it: its narrowest
    /// constraint.
    pub fn scope_kind(&self) -> &'static str {
        if self.library_entry.is_some() {
            "library_entry"
        } else if self.library_file.is_some() {
            "library_file"
        } else if self.library_kind.is_some() {
            "library_kind"
        } else if self.object.is_some() {
            "object"
        } else if self.route.is_some() {
            "route"
        } else if self.region.is_some() {
            "region"
        } else if self.kind.is_some() {
            "kind"
        } else if self.line.is_some() {
            "line"
        } else {
            "network"
        }
    }

    /// Refuses a scope that cannot cover anything as meant.
    pub fn check(&self) -> Result<(), String> {
        let empty = |s: &Option<String>| s.as_ref().is_some_and(|v| v.trim().is_empty());
        for (name, v) in [
            ("line", &self.line),
            ("route", &self.route),
            ("from structure", &self.from_structure),
            ("to structure", &self.to_structure),
            ("kind", &self.kind),
            ("object", &self.object),
            ("library kind", &self.library_kind),
            ("library file", &self.library_file),
            ("library entry", &self.library_entry),
        ] {
            if empty(v) {
                return Err(format!("a scope's {name} cannot be empty"));
            }
        }
        if self.from_structure.is_some() != self.to_structure.is_some() {
            return Err("a part of a route needs both structures (from and to)".into());
        }
        if self.from_structure.is_some() && self.route.is_none() {
            return Err("a part of a route needs its route".into());
        }
        if self.library_entry.is_some() && self.library_file.is_none() {
            return Err("a library entry needs its file".into());
        }
        if self.library_file.is_some() && self.library_kind.is_none() {
            return Err("a library file needs its kind".into());
        }
        let library = self.library_kind.is_some();
        if library && (self.route.is_some() || self.region.is_some() || self.object.is_some()) {
            return Err(
                "a library scope may name a line, never a route, a region or an object".into(),
            );
        }
        if let Some(k) = &self.kind {
            if library && k != "library" {
                return Err(format!("a library scope's kind is library (\"{k}\" given)"));
            }
            if k != "library" && Kind::parse(k).is_err() {
                return Err(format!(
                    "\"{k}\": an object kind (route, alignment, alignment_point, structure, \
                     section, jumper, spacer, meta, report, annotation, point_cloud) or library"
                ));
            }
        }
        if let Some(r) = &self.region
            && (r.len() < 3 || r.iter().flatten().any(|v| !v.is_finite()))
        {
            return Err("a region needs at least 3 finite points".into());
        }
        Ok(())
    }

    /// In words.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(k) = &self.kind {
            parts.push(format!("kind {k}"));
        }
        if let Some(o) = &self.object {
            parts.push(format!("object {o}"));
        }
        if let Some(k) = &self.library_kind {
            let mut s = format!("library {k}");
            if let Some(f) = &self.library_file {
                s.push_str(&format!("/{f}"));
            }
            if let Some(e) = &self.library_entry {
                s.push_str(&format!(" entry {e}"));
            }
            parts.push(s);
        }
        if let Some(r) = &self.route {
            match (&self.from_structure, &self.to_structure) {
                (Some(a), Some(b)) => parts.push(format!("route {r} from {a} to {b}")),
                _ => parts.push(format!("route {r}")),
            }
        }
        if let Some(r) = &self.region {
            parts.push(format!("a region of {} points", r.len()));
        }
        if let Some(l) = &self.line {
            parts.push(format!("line {l}"));
        }
        if parts.is_empty() {
            "the network".into()
        } else {
            parts.join(", ")
        }
    }

    /// Whether this scope holds every target of `inner` (an admin may give
    /// rights only within what they manage). Structural and on the safe
    /// side: a region holds a region only when every point of the inner
    /// one lies inside it and no edges cross.
    pub fn contains(&self, inner: &Scope) -> bool {
        fn within(outer: &Option<String>, inner: &Option<String>) -> bool {
            outer.is_none() || outer == inner
        }
        if !within(&self.line, &inner.line)
            || !within(&self.kind, &inner.kind)
            || !within(&self.object, &inner.object)
            || !within(&self.library_kind, &inner.library_kind)
            || !within(&self.library_file, &inner.library_file)
            || !within(&self.library_entry, &inner.library_entry)
            || !within(&self.route, &inner.route)
        {
            return false;
        }
        if self.from_structure.is_some()
            && (self.from_structure != inner.from_structure
                || self.to_structure != inner.to_structure)
        {
            return false;
        }
        match (&self.region, &inner.region) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(o), Some(i)) => {
                o == i || (i.iter().all(|p| crate::collab::inside(o, *p)) && !edges_cross(o, i))
            }
        }
    }
}

/// An assignment as the network keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    /// Its number.
    pub id: i64,
    /// Who.
    pub subject: Subject,
    /// The role given (or, for a deny, whose permissions are denied; none:
    /// every permission).
    pub role: Option<String>,
    /// A deny (network admins only).
    pub deny: bool,
    /// Where.
    pub scope: Scope,
    /// Who set it.
    #[serde(default)]
    pub by: String,
    /// A note.
    #[serde(default)]
    pub note: String,
}

impl Assignment {
    /// In words.
    pub fn describe(&self) -> String {
        format!(
            "#{} {} {} {} on {}",
            self.id,
            self.subject,
            if self.deny { "denied" } else { "is" },
            self.role.as_deref().unwrap_or("everything"),
            self.scope.describe()
        )
    }
}

/// Where an object is, for the rights: its plan bounds and its routes.
#[derive(Debug, Clone, Copy, Default)]
pub struct Placement<'a> {
    /// Plan bounds (none: no place — settings, a report, …).
    pub bounds: Option<Bounds>,
    /// The routes it is on (a route is on itself).
    pub routes: &'a [String],
}

/// What a permission is asked on.
#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    /// The network's own rows: users, groups, roles, assignments, audit.
    Network,
    /// A line as a whole (its branches, merges, rollbacks; followed whole).
    Line(&'a str),
    /// An object of a line (its main branch's place and routes).
    Object {
        /// The line.
        line: &'a str,
        /// Its kind.
        kind: Kind,
        /// Its id.
        id: &'a str,
        /// Where it is.
        at: Placement<'a>,
    },
    /// A library file (or an entry of one) of a line.
    Library {
        /// The line.
        line: &'a str,
        /// The library kind.
        kind: &'a str,
        /// The file.
        file: &'a str,
        /// An entry of it (none: the file).
        entry: Option<&'a str>,
    },
    /// The network's own library of a kind (phase 9: country libraries),
    /// a file of it or an entry of that file — kept once for the whole
    /// network, of no line: covered by scopes naming no line, route,
    /// region or object.
    CountryLibrary {
        /// The library kind.
        kind: &'a str,
        /// A file of it (none: the kind as a whole).
        file: Option<&'a str>,
        /// An entry of the file (none: the file, or the kind).
        entry: Option<&'a str>,
    },
}

impl<'a> Target<'a> {
    /// The target of object `kind`/`id` of `line`: a library file for a
    /// library object (id `kind/name`).
    pub fn object(line: &'a str, kind: Kind, id: &'a str, at: Placement<'a>) -> Self {
        if kind == Kind::Library
            && let Some((k, f)) = id.split_once('/')
        {
            return Target::Library {
                line,
                kind: k,
                file: f,
                entry: None,
            };
        }
        Target::Object { line, kind, id, at }
    }
}

/// What resolves scopes against the models (the server's, the command
/// line's).
pub trait Resolve {
    /// The objects of a route's part (`route` of `line`, from structure
    /// `from` to `to`), snapped out to whole tension sections; `None`: it
    /// cannot be resolved (it then covers nothing).
    fn route_part(
        &self,
        line: &str,
        route: &str,
        from: &str,
        to: &str,
    ) -> Option<HashSet<(Kind, String)>>;

    /// The lines with an object whose place touches `region`.
    fn region_lines(&self, region: &[[f64; 2]]) -> BTreeSet<String>;
}

/// A resolver knowing no model: route parts cover nothing, regions lie on
/// no line (safe: grants cover less).
pub struct NoModels;

impl Resolve for NoModels {
    fn route_part(&self, _: &str, _: &str, _: &str, _: &str) -> Option<HashSet<(Kind, String)>> {
        None
    }

    fn region_lines(&self, _: &[[f64; 2]]) -> BTreeSet<String> {
        BTreeSet::new()
    }
}

/// The rights of a network as kept (roles, users, groups, assignments).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RightsData {
    /// Roles by name: permissions, shipped.
    pub roles: BTreeMap<String, (Perms, bool)>,
    /// Users by name: active.
    pub users: BTreeMap<String, bool>,
    /// Groups by name, with their members.
    pub groups: BTreeMap<String, BTreeSet<String>>,
    /// Every assignment.
    pub assignments: Vec<Assignment>,
    /// The rights' revision (each change of them adds one).
    pub rev: i64,
}

impl RightsData {
    /// The shipped roles only (a network made before anything was set).
    pub fn shipped() -> Self {
        Self {
            roles: SHIPPED_ROLES
                .iter()
                .map(|(n, p)| ((*n).to_owned(), (Perms::of(p), true)))
                .collect(),
            ..Self::default()
        }
    }

    /// The groups `user` is a member of.
    pub fn groups_of(&self, user: &str) -> BTreeSet<&str> {
        self.groups
            .iter()
            .filter(|(_, m)| m.contains(user))
            .map(|(g, _)| g.as_str())
            .collect()
    }

    /// The assignments of `user` (their own and their groups'), in order.
    pub fn assignments_of(&self, user: &str) -> Vec<&Assignment> {
        let groups = self.groups_of(user);
        self.assignments
            .iter()
            .filter(|a| match &a.subject {
                Subject::User(u) => u == user,
                Subject::Group(g) => groups.contains(g.as_str()),
            })
            .collect()
    }

    /// Whether `user` is a user of the network, not disabled.
    pub fn active(&self, user: &str) -> bool {
        self.users.get(user).copied().unwrap_or(false)
    }

    /// The network admins: users with every permission on the network by
    /// a grant and no deny (a network admin may be denied too).
    pub fn network_admins(&self) -> Vec<String> {
        self.users
            .keys()
            .filter(|u| {
                self.compile(u, &NoModels)
                    .allows(Permission::ManageRights, &Target::Network)
            })
            .cloned()
            .collect()
    }

    /// The rights of `user`, compiled (scopes resolved with `resolve`).
    pub fn compile(&self, user: &str, resolve: &dyn Resolve) -> UserRights {
        let mut out = UserRights {
            user: user.to_owned(),
            active: self.active(user),
            grants: Vec::new(),
            denies: Vec::new(),
            also: None,
        };
        if !out.active {
            return out;
        }
        for a in self.assignments_of(user) {
            let perms = match &a.role {
                Some(r) => match self.roles.get(r) {
                    Some((p, _)) => *p,
                    // A role removed meanwhile: a grant gives nothing, a
                    // deny keeps denying everything (safe side).
                    None if a.deny => Perms::ALL,
                    None => continue,
                },
                None if a.deny => Perms::ALL,
                None => continue,
            };
            if perms.is_empty() {
                continue;
            }
            let s = &a.scope;
            let members = match (&s.line, &s.route, &s.from_structure, &s.to_structure) {
                (Some(l), Some(r), Some(f), Some(t)) => {
                    Some(Arc::new(resolve.route_part(l, r, f, t).unwrap_or_default()))
                }
                // A part whose line is not known covers nothing.
                (None, Some(_), Some(_), Some(_)) => Some(Arc::new(HashSet::new())),
                _ => None,
            };
            // A route or object scope's line is set when it is assigned
            // (ids are unique in the network); without one it may be on
            // any line.
            let lines: Option<BTreeSet<String>> = if s.route.is_some() || s.object.is_some() {
                s.line.as_ref().map(|l| [l.clone()].into())
            } else {
                s.region.as_ref().map(|r| match &s.line {
                    Some(l) => [l.clone()].into(),
                    None => resolve.region_lines(r),
                })
            };
            let c = Compiled {
                id: a.id,
                perms,
                scope: s.clone(),
                region_box: s.region.as_ref().and_then(|r| {
                    let pts: Vec<[f64; 3]> = r.iter().map(|p| [p[0], p[1], 0.0]).collect();
                    Bounds::of_points(pts.iter())
                }),
                members,
                lines,
            };
            if a.deny {
                out.denies.push(c);
            } else {
                out.grants.push(c);
            }
        }
        out
    }
}

/// An assignment ready to decide with.
#[derive(Debug, Clone)]
struct Compiled {
    id: i64,
    perms: Perms,
    scope: Scope,
    region_box: Option<Bounds>,
    /// A route's part: its objects.
    members: Option<Arc<HashSet<(Kind, String)>>>,
    /// The lines a route, region or object scope lies on (for the
    /// settings and library files its grant lets see).
    lines: Option<BTreeSet<String>>,
}

impl Compiled {
    fn line_ok(&self, line: Option<&str>) -> bool {
        match (&self.scope.line, line) {
            (None, _) => true,
            (Some(l), Some(t)) => l == t,
            (Some(_), None) => false,
        }
    }

    /// No constraint but (maybe) the line.
    fn whole_line(&self, line: &str) -> bool {
        let s = &self.scope;
        self.line_ok(Some(line))
            && s.route.is_none()
            && s.region.is_none()
            && s.kind.is_none()
            && s.object.is_none()
            && s.library_kind.is_none()
    }

    /// Whether this scope may hold anything of `line`. A deny's region
    /// may touch every line (the lines it was resolved to may change with
    /// an edit); a grant's, the lines resolved.
    fn may_touch_line(&self, line: &str, deny: bool) -> bool {
        self.line_ok(Some(line))
            && ((deny && self.scope.region.is_some())
                || self.lines.as_ref().is_none_or(|l| l.contains(line)))
    }

    /// A grant on a route, a region or an object: its user may see the
    /// settings and library files of its lines.
    fn sees_line_shared(&self, line: &str, p: Permission) -> bool {
        p == Permission::See
            && self.scope.kind.is_none()
            && self.scope.library_kind.is_none()
            && self.lines.as_ref().is_some_and(|l| l.contains(line))
    }

    fn covers(&self, t: &Target, p: Permission, deny: bool) -> bool {
        let s = &self.scope;
        match t {
            Target::Network => s.is_network(),
            // The network's own library (phase 9): scopes of no line, no
            // place and no object kind but `library`; a grant naming a
            // file or an entry covers only it, a deny naming one covers
            // the kind or file holding it too (it may touch it).
            Target::CountryLibrary { kind, file, entry } => {
                if s.line.is_some() || s.route.is_some() || s.region.is_some() || s.object.is_some()
                {
                    return false;
                }
                if s.kind.as_deref().is_some_and(|k| k != "library") {
                    return false;
                }
                let within = |scope: &Option<String>, t: Option<&str>| match (scope, t) {
                    (None, _) => true,
                    (Some(a), Some(b)) => a == b,
                    (Some(_), None) => deny,
                };
                s.library_kind.as_deref().is_none_or(|k| k == *kind)
                    && within(&s.library_file, *file)
                    && within(&s.library_entry, *entry)
            }
            Target::Line(l) => {
                if deny {
                    self.may_touch_line(l, true)
                } else {
                    self.whole_line(l)
                }
            }
            Target::Library {
                line,
                kind,
                file,
                entry,
            } => {
                if !self.line_ok(Some(line)) {
                    return false;
                }
                if s.object.is_some() || s.route.is_some() || s.region.is_some() {
                    return !deny && self.sees_line_shared(line, p);
                }
                if s.kind.as_deref().is_some_and(|k| k != "library") {
                    return false;
                }
                s.library_kind.as_deref().is_none_or(|k| k == *kind)
                    && s.library_file.as_deref().is_none_or(|f| f == *file)
                    && match (&s.library_entry, entry) {
                        (None, _) => true,
                        (Some(e), Some(t)) => e == t,
                        (Some(_), None) => false,
                    }
            }
            Target::Object { line, kind, id, at } => {
                if !self.line_ok(Some(line)) || s.library_kind.is_some() {
                    return false;
                }
                if s.kind.as_deref().is_some_and(|k| k != kind.as_str()) {
                    return false;
                }
                // Objects without a place or routes: the line's settings,
                // reports, annotations, point clouds.
                let placeless = matches!(
                    kind,
                    Kind::Meta | Kind::Report | Kind::Annotation | Kind::PointCloud
                );
                if placeless {
                    let direct = s.route.is_none()
                        && s.region.is_none()
                        && s.object.as_deref().is_none_or(|o| o == *id);
                    return if direct {
                        true
                    } else {
                        *kind == Kind::Meta && !deny && self.sees_line_shared(line, p)
                    };
                }
                if s.object.as_deref().is_some_and(|o| o != *id) {
                    return false;
                }
                if let Some(r) = &s.route {
                    let on = match &self.members {
                        Some(m) => m.contains(&(*kind, (*id).to_owned())),
                        None => {
                            at.routes.iter().any(|x| x == r) || (*kind == Kind::Route && id == r)
                        }
                    };
                    if !on {
                        return false;
                    }
                }
                if let Some(poly) = &s.region {
                    let Some(b) = at.bounds else {
                        return false;
                    };
                    let in_box = self.region_box.is_some_and(|rb| {
                        b.min[0] <= rb.max[0]
                            && b.max[0] >= rb.min[0]
                            && b.min[1] <= rb.max[1]
                            && b.max[1] >= rb.min[1]
                    });
                    if !in_box {
                        return false;
                    }
                    return if deny {
                        region_touches(poly, &b)
                    } else {
                        region_holds(poly, &b)
                    };
                }
                true
            }
        }
    }
}

/// A user's rights, compiled: the one function deciding.
#[derive(Debug, Clone)]
pub struct UserRights {
    user: String,
    active: bool,
    grants: Vec<Compiled>,
    denies: Vec<Compiled>,
    /// "See as user": another user's rights, both needed.
    also: Option<Box<UserRights>>,
}

impl UserRights {
    /// No rights at all.
    pub fn none(user: &str) -> Self {
        Self {
            user: user.to_owned(),
            active: false,
            grants: Vec::new(),
            denies: Vec::new(),
            also: None,
        }
    }

    /// Whose.
    pub fn user(&self) -> &str {
        &self.user
    }

    /// A user of the network, not disabled.
    pub fn active(&self) -> bool {
        self.active
    }

    /// **The decision**: may the user do `p` on `t`?
    pub fn allows(&self, p: Permission, t: &Target) -> bool {
        if p.needs_see() && !self.raw(Permission::See, t) {
            return false;
        }
        self.raw(p, t) && self.also.as_ref().is_none_or(|o| o.allows(p, t))
    }

    fn raw(&self, p: Permission, t: &Target) -> bool {
        self.active
            && self
                .grants
                .iter()
                .any(|g| g.perms.contains(p) && g.covers(t, p, false))
            && !self
                .denies
                .iter()
                .any(|d| d.perms.contains(p) && d.covers(t, p, true))
    }

    /// Why (or why not): the assignments granting and denying `p` on `t`.
    pub fn explain(&self, p: Permission, t: &Target) -> (bool, Vec<i64>, Vec<i64>) {
        let grants: Vec<i64> = self
            .grants
            .iter()
            .filter(|g| g.perms.contains(p) && g.covers(t, p, false))
            .map(|g| g.id)
            .collect();
        let denies: Vec<i64> = self
            .denies
            .iter()
            .filter(|d| d.perms.contains(p) && d.covers(t, p, true))
            .map(|d| d.id)
            .collect();
        (self.allows(p, t), grants, denies)
    }

    /// Sees everything of the network (a grant of `see` on the network,
    /// no deny of it anywhere): the outgoing filter has nothing to take
    /// out.
    pub fn sees_everything(&self) -> bool {
        self.active
            && self
                .grants
                .iter()
                .any(|g| g.perms.contains(Permission::See) && g.scope.is_network())
            && !self
                .denies
                .iter()
                .any(|d| d.perms.contains(Permission::See))
            && self.also.as_ref().is_none_or(|o| o.sees_everything())
    }

    /// Sees everything of line `line` (followed whole, exported): a grant
    /// of `see` on the whole line, no deny that may touch it.
    pub fn sees_whole_line(&self, line: &str) -> bool {
        self.allows(Permission::See, &Target::Line(line))
    }

    /// May see something of line `line` (a grant of `see` that may touch
    /// it, unless denied the whole line).
    pub fn sees_some_of_line(&self, line: &str) -> bool {
        self.active
            && self
                .grants
                .iter()
                .any(|g| g.perms.contains(Permission::See) && g.may_touch_line(line, false))
            && !self
                .denies
                .iter()
                .any(|d| d.perms.contains(Permission::See) && d.whole_line(line))
            && self.also.as_ref().is_none_or(|o| o.sees_some_of_line(line))
    }

    /// May see some object of line `line` (a grant of `see` that may touch
    /// it, not of a library scope; unless denied the whole line): a
    /// prefilter of what to decide object by object (phase 9's use index).
    pub fn sees_objects_of_line(&self, line: &str) -> bool {
        self.active
            && self.grants.iter().any(|g| {
                g.perms.contains(Permission::See)
                    && g.scope.library_kind.is_none()
                    && g.may_touch_line(line, false)
            })
            && !self
                .denies
                .iter()
                .any(|d| d.perms.contains(Permission::See) && d.whole_line(line))
            && self
                .also
                .as_ref()
                .is_none_or(|o| o.sees_objects_of_line(line))
    }

    /// The role the session's own rules give `user` on line `line`: admin
    /// with every permission on the whole line, designer with a grant to
    /// edit, make or remove something of it, viewer with one to see
    /// something of it; none otherwise. The objects themselves are
    /// decided by [`UserRights::allows`] (the server's check after each
    /// command); this only lets the session's role rules pass.
    pub fn model_role(&self, line: &str) -> Option<Role> {
        if !self.sees_some_of_line(line) {
            return None;
        }
        if self.also.is_some() {
            return Some(Role::Viewer);
        }
        let whole = |p| self.allows(p, &Target::Line(line));
        if [
            Permission::Edit,
            Permission::Create,
            Permission::Delete,
            Permission::ManageRights,
        ]
        .into_iter()
        .all(whole)
        {
            return Some(Role::Admin);
        }
        let edits = self.grants.iter().any(|g| {
            (g.perms.contains(Permission::Edit)
                || g.perms.contains(Permission::Create)
                || g.perms.contains(Permission::Delete))
                && g.may_touch_line(line, false)
        });
        Some(if edits { Role::Designer } else { Role::Viewer })
    }

    /// Manages rights somewhere (may see as another user, within what
    /// both may see).
    pub fn manages_anything(&self) -> bool {
        self.active
            && self
                .grants
                .iter()
                .any(|g| g.perms.contains(Permission::ManageRights))
    }

    /// Whether the user's grants of `p` hold every target of `scope`
    /// (and no deny of `p` may touch it): may give rights there.
    pub fn manages(&self, scope: &Scope) -> bool {
        self.active
            && self
                .grants
                .iter()
                .any(|g| g.perms.contains(Permission::ManageRights) && g.scope.contains(scope))
            && !self.denies.iter().any(|d| {
                d.perms.contains(Permission::ManageRights) && scopes_may_meet(&d.scope, scope)
            })
    }

    /// The route parts' assignments of line `line` (recompiled after its
    /// objects change).
    pub fn has_route_parts(&self, line: &str) -> bool {
        self.grants
            .iter()
            .chain(&self.denies)
            .any(|c| c.members.is_some() && c.scope.line.as_deref() == Some(line))
    }

    /// The same rights (two compilations agree on everything).
    pub fn same_as(&self, o: &UserRights) -> bool {
        type Key = (
            i64,
            Perms,
            Option<Vec<(Kind, String)>>,
            Option<BTreeSet<String>>,
        );
        fn key(c: &Compiled) -> Key {
            (
                c.id,
                c.perms,
                c.members.as_ref().map(|m| {
                    let mut v: Vec<(Kind, String)> = m.iter().cloned().collect();
                    v.sort();
                    v
                }),
                c.lines.clone(),
            )
        }
        self.active == o.active
            && self.grants.len() == o.grants.len()
            && self.denies.len() == o.denies.len()
            && self
                .grants
                .iter()
                .zip(&o.grants)
                .all(|(a, b)| key(a) == key(b) && a.scope == b.scope)
            && self
                .denies
                .iter()
                .zip(&o.denies)
                .all(|(a, b)| key(a) == key(b) && a.scope == b.scope)
    }

    /// The intersection with another user's rights ("see as user": an
    /// admin sees what both may see; nothing is edited so).
    pub fn and(mut self, other: UserRights) -> UserRights {
        self.also = Some(Box::new(other));
        self
    }

    /// Seeing as another user ([`UserRights::and`]).
    pub fn seeing_as(&self) -> Option<&str> {
        self.also.as_deref().map(UserRights::user)
    }
}

/// Whether two scopes may hold a common target (safe side: yes unless a
/// constraint plainly differs).
fn scopes_may_meet(a: &Scope, b: &Scope) -> bool {
    fn apart(x: &Option<String>, y: &Option<String>) -> bool {
        matches!((x, y), (Some(p), Some(q)) if p != q)
    }
    !(apart(&a.line, &b.line)
        || apart(&a.kind, &b.kind)
        || apart(&a.object, &b.object)
        || apart(&a.route, &b.route)
        || apart(&a.library_kind, &b.library_kind)
        || apart(&a.library_file, &b.library_file))
}

/// The rectangle `b` lies wholly inside polygon `poly`: its corners
/// inside and no edge of the polygon reaching it.
pub fn region_holds(poly: &[[f64; 2]], b: &Bounds) -> bool {
    let corners = [b.min, [b.max[0], b.min[1]], b.max, [b.min[0], b.max[1]]];
    corners.iter().all(|c| crate::collab::inside(poly, *c))
        && !(0..poly.len()).any(|i| segment_hits_box(poly[i], poly[(i + 1) % poly.len()], b))
}

/// Polygon `poly` and the rectangle `b` meet (a corner of one inside the
/// other, or an edge reaching the rectangle).
pub fn region_touches(poly: &[[f64; 2]], b: &Bounds) -> bool {
    let corners = [b.min, [b.max[0], b.min[1]], b.max, [b.min[0], b.max[1]]];
    corners.iter().any(|c| crate::collab::inside(poly, *c))
        || poly
            .iter()
            .any(|p| p[0] >= b.min[0] && p[0] <= b.max[0] && p[1] >= b.min[1] && p[1] <= b.max[1])
        || (0..poly.len()).any(|i| segment_hits_box(poly[i], poly[(i + 1) % poly.len()], b))
}

/// Segment a–b reaches the closed rectangle `b` (Liang–Barsky).
fn segment_hits_box(a: [f64; 2], c: [f64; 2], b: &Bounds) -> bool {
    let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-dx, a[0] - b.min[0]),
        (dx, b.max[0] - a[0]),
        (-dy, a[1] - b.min[1]),
        (dy, b.max[1] - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Some edge of polygon `a` crosses an edge of polygon `b`.
fn edges_cross(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    fn orient(p: [f64; 2], q: [f64; 2], r: [f64; 2]) -> f64 {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    }
    for i in 0..a.len() {
        let (p1, p2) = (a[i], a[(i + 1) % a.len()]);
        for j in 0..b.len() {
            let (q1, q2) = (b[j], b[(j + 1) % b.len()]);
            let d1 = orient(q1, q2, p1);
            let d2 = orient(q1, q2, p2);
            let d3 = orient(p1, p2, q1);
            let d4 = orient(p1, p2, q2);
            if d1 * d2 <= 0.0 && d3 * d4 <= 0.0 {
                return true;
            }
        }
    }
    false
}

/// Words of `text` that may be ids (runs of letters, digits, `_`, `-`,
/// `.`, `/`), for the outgoing filter of error messages.
pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')))
        .map(|w| w.trim_matches(|c| c == '.' || c == '/'))
        .filter(|w| !w.is_empty())
}

/// What the server tells a user refused on an object they may not see (no
/// id, no place).
pub const HIDDEN_REFUSAL: &str = "it touches objects you have no access to (ask a network admin)";
