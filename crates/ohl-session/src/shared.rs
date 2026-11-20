// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Towers shared by several routes (country server, phase 8; ADR-0013,
//! plan §7): a user who works on some routes — their **working set** —
//! may change a tower that routes outside it use too, as long as the
//! other routes stay connected.
//!
//! **Where a working set is defined** (nowhere else: a project without
//! one behaves as before, every route editable, nothing refused or
//! written that was not before):
//!
//! - a user's own working set in a project file ([`Session::set_working_set`],
//!   `ols_meta` key [`WORKING_SETS_KEY`]): the routes they edit;
//! - a session over a part of a line ([`mod@crate::part`]): what is outside
//!   its working set is its context, and the objects of the whole line
//!   using its structures are recorded in it ([`PART_USERS_KEY`]).
//!
//! **The other routes' objects** are the sections, jumpers, spacers,
//! alignments, alignment points and routes of a route outside the working
//! set. A structure is on the routes it gives itself, else on those of
//! the alignments tied to it **and of every alignment passing within
//! [`ohl_line::route::NEAR_ALIGNMENT`] of it** — counted without a search
//! of the whole line, so never fewer than the line itself counts
//! ([`ohl_line::Line::structure_routes`]), sometimes more (a route
//! counted too many protects more). A section's routes are those all its
//! structures on a route share; a section with none shared runs from one
//! route into another and is on all of their routes. A jumper is on the
//! routes of the sections at its ends (else of its structures), a spacer
//! on its sections'. An object is outside when one of its routes is not
//! in the working set (a section on two routes, one outside, is the other
//! route's too).
//!
//! **Refused** (each refusal names the other route and its objects; the
//! number is the rule of plan §7.2):
//!
//! 0. changing an object of a route outside the working set (creating,
//!    changing or deleting it), or a structure all of whose routes are
//!    outside;
//! 1. deleting or renaming a structure the other routes' objects use;
//! 2. removing or omitting a wire point a section or jumper of another
//!    route ends on or passes through (a structure type or extension
//!    change leaving it out, a set renumbered);
//! 3. removing or changing what such a wire point hangs on (its strings,
//!    their tops, the points above);
//! 4. turning such a wire point from suspension to strain or back, or from
//!    hanging to fixed;
//! 5. removing or changing the strings of a jumper's support clamp of
//!    another route (or the assembly — a TAK — it is part of);
//! 6. taking an outside route off the structure (its `routes`), untying
//!    an outside route's alignment point from it, or moving a structure
//!    that is on an outside route by its alignment only away from it;
//! 7. deleting or renaming a structure an outside section's circuit start
//!    or a structure group naming an outside route's structures names;
//! 8. anything else leaving another route's objects not valid: the
//!    session validates every command (locally, [`ohl_line::Line::validate_local`]),
//!    and such a refusal says which outside routes use the structures
//!    changed.
//!
//! A **typed** structure takes its wire points and strings from its type
//! in the structure library, which the session does not read: with other
//! routes' sections or jumpers on it, only changes keeping the make-up of
//! its own hardware (which points, strings and wire points with which
//! sets, what hangs on what, strain, post, hinged; its omissions, type and
//! extension) can be shown to keep them connected — places may change,
//! any other change is refused (safe side).
//!
//! **Allowed, the other route marked stale** (§7.3): moving or turning a
//! shared structure, its height, its foundations and local factors,
//! moving a shared wire point or string (its topology kept), changing a
//! string's insulator. Each outside route touched gets a **stale mark**
//! with a notice ("T12 moved 0.80 m by ana"; `ols_meta` key
//! [`STALE_KEY`]), and the log entry says it. A change of the line's
//! settings or library files marks every outside route stale (its results
//! may change). On a **protected** project line ([`crate::versions`]) such
//! a change is refused instead: it "needs a review by its owners" — made
//! on a project line of one's own and submitted for review (the existing
//! review flow; an approved review's merge is not a user's edit).
//!
//! **Parts used only by the other route** (§7.4): a wire point used only
//! by outside routes, and the strings and points it alone hangs on, are
//! that route's hardware — changed only with that route in the working
//! set.
//!
//! **Merges** ([`connection_conflicts`]): a structure changed on one side
//! only while the other side made or changed something on it whose
//! connection the change breaks (rules 1–5) is a conflict to choose. In a
//! project without working sets this is said only where the merged line
//! would not be valid anyway (before: refused with nothing to choose);
//! with working sets, always.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use ohl_line::index::{LineIndex, Of};
use ohl_line::{JumperEnd, Line, Point, Structure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::in_place::of_kind;
use crate::store::Store;
use crate::{Command, Kind, Session, SessionError, refused};

/// `ols_meta` key: the working sets, `{user: [route, …]}` (absent: none).
pub const WORKING_SETS_KEY: &str = "working_sets";
/// `ols_meta` key: the routes whose results are out of date,
/// `{route: Stale}` (absent: none).
pub const STALE_KEY: &str = "stale_routes";
/// `ols_meta` key of a part: the objects of the whole line using its
/// structures that the part does not hold, `{structure: [User, …]}`.
pub const PART_USERS_KEY: &str = "part_users";
/// Notices kept per stale route (the latest; the others counted).
pub const NOTICES_KEPT: usize = 20;

/// A route's results out of date, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stale {
    /// Notices since the route was last marked up to date.
    pub count: usize,
    /// The log entry of the first.
    pub since: i64,
    /// The latest notices ([`NOTICES_KEPT`]), oldest first.
    pub notices: Vec<Notice>,
}

/// What made a route's results out of date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    /// The log entry.
    pub seq: i64,
    /// Who made the change.
    pub user: String,
    /// What changed, in words ("T12 moved 0.80 m").
    pub what: String,
}

/// An object using a structure, as a part records it (the part does not
/// hold it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct User {
    /// Its kind (section, jumper, alignment point).
    pub kind: Kind,
    /// Its id.
    pub id: String,
    /// Its routes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    /// The structure's sets it ends on or passes through.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<u32>,
    /// The structure's wire points (no set) holding its support clamps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clamps: Vec<String>,
    /// An alignment point tied to the structure.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tied: bool,
}

impl User {
    fn named(&self) -> String {
        let routes = match self.routes.len() {
            0 => String::new(),
            1 => format!(" of route {}", self.routes[0]),
            _ => format!(" of routes {}", self.routes.join(", ")),
        };
        format!(
            "{} {}{routes}",
            self.kind.as_str().replace('_', " "),
            self.id
        )
    }
}

/// A user of a structure, with whether it is outside the working set.
#[derive(Debug, Clone)]
struct Use {
    user: User,
    /// One of its routes is outside the working set: protected by the
    /// rules.
    outside: bool,
    /// None of its routes is in the working set: the other routes' only.
    foreign: bool,
}

/// What a command allowed does to the other routes: the notices by route,
/// and every outside route using what it changed (for a refusal's words).
#[derive(Debug, Clone, Default)]
pub(crate) struct Outcome {
    pub(crate) notices: BTreeMap<String, String>,
    pub(crate) involved: BTreeSet<String>,
}

impl Outcome {
    /// The log entry's words: "route Y's results are out of date (T12
    /// moved 0.80 m)".
    pub(crate) fn note(&self) -> Option<String> {
        if self.notices.is_empty() {
            return None;
        }
        let parts: Vec<String> = self
            .notices
            .iter()
            .map(|(r, what)| format!("route {r}'s results are out of date ({what})"))
            .collect();
        Some(format!("shared towers: {}", parts.join("; ")))
    }

    /// A refusal of the command, with the outside routes using what it
    /// changed (rule 8).
    pub(crate) fn decorate(&self, e: SessionError) -> SessionError {
        match e {
            SessionError::Refused(m) if !self.involved.is_empty() && !m.contains("working set") => {
                let routes: Vec<&str> = self.involved.iter().map(String::as_str).collect();
                refused(format!(
                    "{m} (what it changes is used by route{} {}, not in your working set)",
                    if routes.len() == 1 { "" } else { "s" },
                    routes.join(", ")
                ))
            }
            e => e,
        }
    }
}

/// The routes of the alignments: for the routes of structures by their
/// place (alignments within [`ohl_line::route::NEAR_ALIGNMENT`]).
struct Geo {
    /// Per alignment: its route, its points in plan, its bounds.
    alignments: Vec<(String, Vec<[f64; 2]>, [f64; 4])>,
}

impl Geo {
    fn of(line: &Line, index: &LineIndex) -> Self {
        let alignments = line
            .alignments
            .iter()
            .map(|a| {
                let pts: Vec<[f64; 2]> = a
                    .points
                    .iter()
                    .filter_map(|p| index.alignment_point(line, p))
                    .map(|p| [p.position[0], p.position[1]])
                    .collect();
                let mut b = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for p in &pts {
                    b = [
                        b[0].min(p[0]),
                        b[1].min(p[1]),
                        b[2].max(p[0]),
                        b[3].max(p[1]),
                    ];
                }
                (a.route.clone(), pts, b)
            })
            .collect();
        Self { alignments }
    }

    /// The routes of the alignments passing within reach of `at` (plan).
    fn near(&self, at: Point) -> BTreeSet<String> {
        let reach = ohl_line::route::NEAR_ALIGNMENT + 1e-3;
        let p = [at[0], at[1]];
        let mut out = BTreeSet::new();
        for (route, pts, b) in &self.alignments {
            if out.contains(route)
                || p[0] < b[0] - reach
                || p[1] < b[1] - reach
                || p[0] > b[2] + reach
                || p[1] > b[3] + reach
            {
                continue;
            }
            let d = match pts.as_slice() {
                [] => f64::INFINITY,
                [a] => (a[0] - p[0]).hypot(a[1] - p[1]),
                _ => pts
                    .windows(2)
                    .map(|w| ohl_core::geometry::distance_to_segment(p, w[0], w[1]))
                    .fold(f64::INFINITY, f64::min),
            };
            if d <= reach {
                out.insert(route.clone());
            }
        }
        out
    }
}

/// One check of a command: the line before it, the working set, what is
/// worked out once.
struct Ctx<'a> {
    line: &'a Line,
    index: &'a LineIndex,
    /// The working set's routes (none: a part).
    routes: Option<&'a BTreeSet<String>>,
    /// The working set of a part.
    part: Option<&'a BTreeSet<(Kind, String)>>,
    /// Users recorded in a part.
    recorded: &'a BTreeMap<String, Vec<User>>,
    geo: RefCell<Option<Geo>>,
    of_structure: RefCell<HashMap<String, BTreeSet<String>>>,
}

impl Ctx<'_> {
    /// The routes of structure `s` as it is (see the module's notes).
    fn structure_routes(&self, s: &Structure) -> BTreeSet<String> {
        if let Some(r) = self.of_structure.borrow().get(&s.id) {
            return r.clone();
        }
        let routes = if !s.routes.is_empty() {
            s.routes.iter().cloned().collect()
        } else {
            let mut r = self.tied_routes(&s.id);
            r.extend(self.near(s.position));
            r
        };
        self.of_structure
            .borrow_mut()
            .insert(s.id.clone(), routes.clone());
        routes
    }

    /// The routes of the alignments passing near `at`.
    fn near(&self, at: Point) -> BTreeSet<String> {
        let mut geo = self.geo.borrow_mut();
        geo.get_or_insert_with(|| Geo::of(self.line, self.index))
            .near(at)
    }

    /// The routes of the alignment points tied to structure `id`.
    fn tied_routes(&self, id: &str) -> BTreeSet<String> {
        self.index
            .referrers_of(Of::Structure, id, Of::AlignmentPoint)
            .flat_map(|p| self.point_routes(p))
            .collect()
    }

    /// The routes of alignment point `id`: those of its alignments.
    fn point_routes(&self, id: &str) -> BTreeSet<String> {
        self.index
            .referrers_of(Of::AlignmentPoint, id, Of::Alignment)
            .filter_map(|a| {
                self.line
                    .alignments
                    .get(self.index.position(Of::Alignment, a)?)
            })
            .map(|a| a.route.clone())
            .collect()
    }

    /// The routes of the structure `id` of the line (none: not there).
    fn routes_of_id(&self, id: &str) -> BTreeSet<String> {
        self.index
            .structure(self.line, id)
            .map(|s| self.structure_routes(s))
            .unwrap_or_default()
    }

    /// The routes of a section through `structures`: those all of them
    /// on a route are on, else those any is on (a structure on no route
    /// says nothing).
    fn section_routes<'s>(&self, structures: impl Iterator<Item = &'s str>) -> BTreeSet<String> {
        let mut all: Option<BTreeSet<String>> = None;
        let mut any = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for s in structures {
            if !seen.insert(s) {
                continue;
            }
            let r = self.routes_of_id(s);
            if r.is_empty() {
                continue;
            }
            any.extend(r.iter().cloned());
            all = Some(match all {
                None => r,
                Some(a) => a.intersection(&r).cloned().collect(),
            });
        }
        match all {
            Some(a) if !a.is_empty() => a,
            _ => any,
        }
    }

    /// The routes of section `id` of the line.
    fn routes_of_section(&self, id: &str) -> BTreeSet<String> {
        match self.index.section(self.line, id) {
            Some(s) => self.section_routes(s.points.iter().map(|p| p.structure.as_str())),
            None => BTreeSet::new(),
        }
    }

    /// The routes of jumper `j`: those of the sections at its ends, else
    /// of its structures.
    fn jumper_routes(&self, j: &ohl_line::Jumper) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for end in [&j.from, &j.to] {
            if let Some(s) = self.index.end_section(self.line, end, 0) {
                out.extend(self.routes_of_section(&s.id));
            }
        }
        if out.is_empty() {
            let mut at: Vec<String> = Vec::new();
            for end in [&j.from, &j.to] {
                at.extend(self.line.end_structures(end));
            }
            for c in j.supports() {
                at.push(c.structure_id(j).to_owned());
            }
            for s in at {
                out.extend(self.routes_of_id(&s));
            }
        }
        out
    }

    /// The routes of line object `kind`/`id` as it is in the line.
    fn routes_now(&self, kind: Kind, id: &str) -> BTreeSet<String> {
        match kind {
            Kind::Route => [id.to_owned()].into(),
            Kind::Alignment => self
                .index
                .position(Of::Alignment, id)
                .map(|k| [self.line.alignments[k].route.clone()].into())
                .unwrap_or_default(),
            Kind::AlignmentPoint => self.point_routes(id),
            Kind::Structure => self.routes_of_id(id),
            Kind::Section => self.routes_of_section(id),
            Kind::Jumper => self
                .index
                .jumper(self.line, id)
                .map(|j| self.jumper_routes(j))
                .unwrap_or_default(),
            Kind::Spacer => self
                .index
                .position(Of::Spacer, id)
                .map(|k| {
                    let s = &self.line.spacers[k];
                    let mut r = self.routes_of_section(&s.a.section);
                    r.extend(self.routes_of_section(&s.b.section));
                    r
                })
                .unwrap_or_default(),
            _ => BTreeSet::new(),
        }
    }

    /// The routes of line object `kind` as `value` would put it (its
    /// structures and alignments as the line has them).
    fn routes_of_value(&self, kind: Kind, id: &str, value: &Value) -> BTreeSet<String> {
        match kind {
            Kind::Route => [id.to_owned()].into(),
            Kind::Alignment => value
                .get("route")
                .and_then(Value::as_str)
                .map(|r| [r.to_owned()].into())
                .unwrap_or_else(|| self.routes_now(kind, id)),
            Kind::Section => match serde_json::from_value::<ohl_line::Section>(value.clone()) {
                Ok(s) => self.section_routes(s.points.iter().map(|p| p.structure.as_str())),
                Err(_) => self.routes_now(kind, id),
            },
            Kind::Jumper => match serde_json::from_value::<ohl_line::Jumper>(value.clone()) {
                Ok(j) => self.jumper_routes(&j),
                Err(_) => self.routes_now(kind, id),
            },
            Kind::Spacer => match serde_json::from_value::<ohl_line::Spacer>(value.clone()) {
                Ok(s) => {
                    let mut r = self.routes_of_section(&s.a.section);
                    r.extend(self.routes_of_section(&s.b.section));
                    r
                }
                Err(_) => self.routes_now(kind, id),
            },
            _ => self.routes_now(kind, id),
        }
    }

    /// Whether routes `r` reach outside the working set.
    fn outside_routes(&self, r: &BTreeSet<String>) -> bool {
        self.routes
            .is_some_and(|w| r.iter().any(|x| !w.contains(x)))
    }

    /// Those of `r` outside the working set.
    fn outside_of(&self, r: &BTreeSet<String>) -> Vec<String> {
        match self.routes {
            Some(w) => r.iter().filter(|x| !w.contains(*x)).cloned().collect(),
            None => r.iter().cloned().collect(),
        }
    }

    /// Whether line object `kind`/`id` with routes `r` is outside.
    fn is_outside(&self, kind: Kind, id: &str, r: &BTreeSet<String>) -> bool {
        match self.part {
            Some(wanted) => !wanted.contains(&(kind, id.to_owned())),
            None => self.outside_routes(r),
        }
    }

    /// Whether line object `kind`/`id` with routes `r` is the other
    /// routes' only (none of its routes in the working set).
    fn is_foreign(&self, kind: Kind, id: &str, r: &BTreeSet<String>) -> bool {
        match (self.part, self.routes) {
            (Some(wanted), _) => !wanted.contains(&(kind, id.to_owned())),
            (None, Some(w)) => !r.is_empty() && r.iter().all(|x| !w.contains(x)),
            (None, None) => false,
        }
    }

    /// The objects using structure `id` (sections, jumpers, alignment
    /// points tied to it; in a part, those it recorded too).
    fn users(&self, id: &str) -> Vec<Use> {
        let mut out = Vec::new();
        for sid in self.index.referrers_of(Of::Structure, id, Of::Section) {
            let Some(s) = self.index.section(self.line, sid) else {
                continue;
            };
            let routes = self.routes_of_section(sid);
            let mut sets: Vec<u32> = s
                .points
                .iter()
                .filter(|p| p.structure == id)
                .map(|p| p.set)
                .collect();
            sets.sort_unstable();
            sets.dedup();
            out.push(Use {
                outside: self.is_outside(Kind::Section, sid, &routes),
                foreign: self.is_foreign(Kind::Section, sid, &routes),
                user: User {
                    kind: Kind::Section,
                    id: sid.clone(),
                    routes: routes.into_iter().collect(),
                    sets,
                    clamps: Vec::new(),
                    tied: false,
                },
            });
        }
        for jid in self.index.referrers_of(Of::Structure, id, Of::Jumper) {
            let Some(j) = self.index.jumper(self.line, jid) else {
                continue;
            };
            let routes = self.jumper_routes(j);
            let mut sets: Vec<u32> = [&j.from, &j.to]
                .into_iter()
                .filter_map(|e| match e {
                    JumperEnd::Wire(p) if p.structure == id => Some(p.set),
                    _ => None,
                })
                .collect();
            sets.sort_unstable();
            sets.dedup();
            let mut clamps: Vec<String> = j
                .supports()
                .into_iter()
                .filter(|c| c.structure_id(j) == id)
                .map(|c| c.wire)
                .collect();
            clamps.sort();
            clamps.dedup();
            out.push(Use {
                outside: self.is_outside(Kind::Jumper, jid, &routes),
                foreign: self.is_foreign(Kind::Jumper, jid, &routes),
                user: User {
                    kind: Kind::Jumper,
                    id: jid.clone(),
                    routes: routes.into_iter().collect(),
                    sets,
                    clamps,
                    tied: false,
                },
            });
        }
        for pid in self
            .index
            .referrers_of(Of::Structure, id, Of::AlignmentPoint)
        {
            let routes = self.point_routes(pid);
            out.push(Use {
                outside: self.is_outside(Kind::AlignmentPoint, pid, &routes),
                foreign: self.is_foreign(Kind::AlignmentPoint, pid, &routes),
                user: User {
                    kind: Kind::AlignmentPoint,
                    id: pid.clone(),
                    routes: routes.into_iter().collect(),
                    sets: Vec::new(),
                    clamps: Vec::new(),
                    tied: true,
                },
            });
        }
        if let Some(rec) = self.recorded.get(id) {
            for u in rec {
                // (One the part holds after all is counted as it is there.)
                if out
                    .iter()
                    .any(|x| x.user.kind == u.kind && x.user.id == u.id)
                {
                    continue;
                }
                out.push(Use {
                    user: u.clone(),
                    outside: true,
                    foreign: true,
                });
            }
        }
        out
    }
}

/// How a wire point hangs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mode {
    /// No string reaches it: fixed to the structure.
    Fixed,
    /// Hanging on suspension strings.
    Suspension,
    /// On a strain string.
    Strain,
}

impl Mode {
    fn word(self) -> &'static str {
        match self {
            Mode::Fixed => "fixed to the structure",
            Mode::Suspension => "a suspension point",
            Mode::Strain => "a strain point",
        }
    }
}

/// A part of a structure's hardware, by what it is (not where).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    /// A structure point.
    Point(String),
    /// A wire point and its set.
    Wire(String, Option<u32>),
    /// A string: label, top, wire point, strain, post, hinged.
    String(String, String, String, bool, bool, bool),
    /// A label the hardware names that is not there.
    Missing(String),
}

impl Part {
    fn label(&self) -> &str {
        match self {
            Part::Point(l) | Part::Wire(l, _) | Part::String(l, ..) | Part::Missing(l) => l,
        }
    }
}

/// What a wire point is and hangs on, by what the parts are.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hang {
    label: String,
    omitted: bool,
    mode: Mode,
    parts: BTreeSet<Part>,
}

/// Wire point `label` of untyped structure `s`: its mode and the parts it
/// hangs on (the strings to it, their tops, all the way up) and the
/// strings hanging from it (none: no such wire point).
fn hang(s: &Structure, label: &str) -> Option<Hang> {
    let w = s.wires.iter().find(|w| w.label == label)?;
    let mut parts = BTreeSet::new();
    parts.insert(Part::Wire(w.label.clone(), w.set));
    let to: Vec<_> = s.strings.iter().filter(|x| x.wire == label).collect();
    let mode = if to.is_empty() {
        Mode::Fixed
    } else if to.iter().any(|x| x.strain) {
        Mode::Strain
    } else {
        Mode::Suspension
    };
    let mut queue: Vec<String> = vec![label.to_owned()];
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(l) = queue.pop() {
        if !seen.insert(l.clone()) || seen.len() > 64 {
            continue;
        }
        for x in s.strings.iter().filter(|x| x.wire == l) {
            parts.insert(Part::String(
                x.label.clone(),
                x.top.clone(),
                x.wire.clone(),
                x.strain,
                x.post,
                x.hinged,
            ));
            if let Some(p) = s.points.iter().find(|p| p.label == x.top) {
                parts.insert(Part::Point(p.label.clone()));
            } else if let Some(m) = s.wires.iter().find(|m| m.label == x.top) {
                parts.insert(Part::Wire(m.label.clone(), m.set));
                queue.push(m.label.clone());
            } else {
                parts.insert(Part::Missing(x.top.clone()));
            }
        }
    }
    // The strings hanging from it (a TAK's tension strings from the
    // middle point).
    for x in s.strings.iter().filter(|x| x.top == label) {
        parts.insert(Part::String(
            x.label.clone(),
            x.top.clone(),
            x.wire.clone(),
            x.strain,
            x.post,
            x.hinged,
        ));
    }
    let omitted = s.omit.iter().any(|o| o == label)
        || w.set.is_some_and(|set| s.omit_sets.contains(&set))
        || parts.iter().any(|p| s.omit.iter().any(|o| o == p.label()));
    Some(Hang {
        label: label.to_owned(),
        omitted,
        mode,
        parts,
    })
}

/// A structure's hardware by what it is (positions left out): type,
/// extension, omissions, points, strings, wire points.
fn topology(s: &Structure) -> Value {
    let points: Vec<&str> = s.points.iter().map(|p| p.label.as_str()).collect();
    let strings: Vec<Value> = s
        .strings
        .iter()
        .map(|x| serde_json::json!([x.label, x.top, x.wire, x.strain, x.post, x.hinged]))
        .collect();
    let wires: Vec<Value> = s
        .wires
        .iter()
        .map(|w| serde_json::json!([w.label, w.set]))
        .collect();
    serde_json::json!({
        "type": s.structure_type, "extension": s.extension, "omit": s.omit,
        "omit_sets": s.omit_sets, "points": points, "strings": strings, "wires": wires,
    })
}

/// A part of a structure's hardware: its JSON without its place, and its
/// place in the structure's own axes (none: a string, or a wire point
/// strings reach — placed by the strings' lengths and angles).
type Held = (Value, Option<[f64; 3]>);

/// A structure's hardware in its own axes (moved or turned whole: the
/// same): every point, string and wire point by label.
fn hardware_local(s: &Structure) -> BTreeMap<(u8, String), Held> {
    let a = -s.orientation_deg.to_radians();
    let (sin, cos) = a.sin_cos();
    let local = |p: Point| -> [f64; 3] {
        let d = [
            p[0] - s.position[0],
            p[1] - s.position[1],
            p[2] - s.position[2],
        ];
        [d[0] * cos - d[1] * sin, d[0] * sin + d[1] * cos, d[2]]
    };
    let without = |mut v: Value| {
        if let Some(o) = v.as_object_mut() {
            o.remove("position");
        }
        v
    };
    let mut out = BTreeMap::new();
    for p in &s.points {
        let v = serde_json::to_value(p).unwrap_or(Value::Null);
        out.insert((0, p.label.clone()), (without(v), Some(local(p.position))));
    }
    for x in &s.strings {
        let v = serde_json::to_value(x).unwrap_or(Value::Null);
        out.insert((1, x.label.clone()), (v, None));
    }
    for w in &s.wires {
        let v = serde_json::to_value(w).unwrap_or(Value::Null);
        // A wire point strings reach is placed by them: its place is
        // theirs, not its own.
        let held = s.strings.iter().any(|x| x.wire == w.label);
        let at = (!held).then(|| local(w.position));
        out.insert((2, w.label.clone()), (without(v), at));
    }
    out
}

/// Whether two parts of hardware are the same: the same JSON, places
/// within a micrometre (a move and its way back give the same).
fn same_held(a: Option<&Held>, b: Option<&Held>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.0 == b.0
                && match (a.1, b.1) {
                    (None, None) => true,
                    (Some(p), Some(q)) => (0..3).all(|k| (p[k] - q[k]).abs() <= 1e-6),
                    _ => false,
                }
        }
        _ => false,
    }
}

/// Whether two structures' hardware is the same in their own axes.
fn same_hardware(a: &Structure, b: &Structure) -> bool {
    let (x, y) = (hardware_local(a), hardware_local(b));
    x.len() == y.len() && x.iter().all(|(k, v)| same_held(Some(v), y.get(k)))
}

/// Why a change of structure `before` into `after` breaks user `u`'s
/// connection (rules 2–5; none: it holds).
fn breaks(before: &Structure, after: &Structure, u: &User) -> Option<String> {
    let id = &before.id;
    let typed = before.structure_type.is_some() || after.structure_type.is_some();
    if typed {
        if (u.sets.is_empty() && u.clamps.is_empty()) || topology(before) == topology(after) {
            return None;
        }
        return Some(format!(
            "{id} takes its wire points from its type in the structure library: whether this \
             change keeps {}'s wire points cannot be checked here (rules 2–5, safe side)",
            u.named()
        ));
    }
    for &set in &u.sets {
        let b = before.wire_at_set(set).and_then(|w| hang(before, &w.label));
        let a = after.wire_at_set(set).and_then(|w| hang(after, &w.label));
        let what = format!("wire point set {set} of {id}");
        match (b, a) {
            (None, _) => {}
            (Some(_), None) => {
                return Some(format!(
                    "{what} is used by {}: it would be removed (rule 2)",
                    u.named()
                ));
            }
            (Some(b), Some(a)) => {
                if a.omitted && !b.omitted {
                    return Some(format!(
                        "{what} is used by {}: it would be left out (rule 2)",
                        u.named()
                    ));
                }
                if a.mode != b.mode {
                    return Some(format!(
                        "{what} is used by {}: it would turn from {} to {} (rule 4)",
                        u.named(),
                        b.mode.word(),
                        a.mode.word()
                    ));
                }
                if a.parts != b.parts || a.label != b.label {
                    return Some(format!(
                        "{what} is used by {}: what it hangs on would change (its strings, \
                         their tops or the points above: rule 3)",
                        u.named()
                    ));
                }
            }
        }
    }
    for label in &u.clamps {
        let (b, a) = (hang(before, label), hang(after, label));
        if b.is_some()
            && (a.is_none() || a.as_ref().map(|a| &a.parts) != b.as_ref().map(|b| &b.parts))
        {
            return Some(format!(
                "wire point {label} of {id} holds a support clamp of {}: its strings or their \
                 assembly would change (rule 5)",
                u.named()
            ));
        }
    }
    None
}

/// What changed of a structure, in words ("T12 moved 0.80 m").
fn describe(before: &Structure, after: &Structure) -> String {
    let mut what = Vec::new();
    let d = [
        after.position[0] - before.position[0],
        after.position[1] - before.position[1],
        after.position[2] - before.position[2],
    ];
    let plan = d[0].hypot(d[1]);
    if plan > 1e-6 {
        what.push(format!("moved {plan:.2} m"));
    }
    if d[2].abs() > 1e-6 || (after.height_adjust - before.height_adjust).abs() > 1e-6 {
        what.push(format!(
            "height changed {:+.2} m",
            d[2] + after.height_adjust - before.height_adjust
        ));
    }
    if (after.orientation_deg - before.orientation_deg).abs() > 1e-9 {
        what.push(format!(
            "turned {:+.1}°",
            after.orientation_deg - before.orientation_deg
        ));
    }
    if !same_hardware(before, after) {
        what.push("its strings or wire points changed".into());
    }
    if what.is_empty() {
        what.push("changed".into());
    }
    format!("{} {}", before.id, what.join(", "))
}

/// Parses a structure as a put gives it, its strings placed as the line
/// places them when it is read ([`ohl_line::Line::place_strings`]; one
/// that cannot be placed is refused by the validation after).
fn structure_of(value: &Value) -> Result<Structure, SessionError> {
    let mut s: Structure =
        serde_json::from_value(value.clone()).map_err(|e| refused(format!("structure: {e}")))?;
    s.take_string_geometry();
    let _ = s.place_wires();
    Ok(s)
}

/// The objects a command changes, as it leaves them (none: deleted).
fn after_values(prims: &[Command]) -> BTreeMap<(Kind, String), Option<&Value>> {
    let mut out = BTreeMap::new();
    for p in prims {
        match p {
            Command::Put {
                kind, id, value, ..
            } => {
                out.insert((*kind, id.clone()), Some(value));
            }
            Command::Delete { kind, id, .. } => {
                out.insert((*kind, id.clone()), None);
            }
            _ => {}
        }
    }
    out
}

/// "route Y" / "routes Y, Z".
fn routes_word(r: &[String]) -> String {
    match r.len() {
        1 => format!("route {}", r[0]),
        _ => format!("routes {}", r.join(", ")),
    }
}

/// The refusal's words for objects outside the working set.
fn not_in(part: bool) -> &'static str {
    if part {
        "which this part does not hold (it is changed on the whole line)"
    } else {
        "not in your working set"
    }
}

impl Session {
    /// The working sets: user → the routes they edit.
    pub fn working_sets(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.working_sets
    }

    /// `user`'s working set (none: every route).
    pub fn working_set(&self, user: &str) -> Option<&BTreeSet<String>> {
        self.working_sets.get(user)
    }

    /// Gives `user` the working set `routes` (none: every route again, as
    /// a project without one). Refused: a route not in the line, a user
    /// with no role.
    pub fn set_working_set(
        &mut self,
        user: &str,
        routes: Option<BTreeSet<String>>,
    ) -> Result<(), SessionError> {
        if self.role(user)?.is_none() {
            return Err(refused(format!("{user} has no role in this project")));
        }
        let mut sets = self.working_sets.clone();
        match routes {
            Some(r) => {
                if let Some(x) = r
                    .iter()
                    .find(|x| !self.line.routes.iter().any(|y| y.id == **x))
                {
                    return Err(refused(format!("no route {x}")));
                }
                sets.insert(user.to_owned(), r);
            }
            None => {
                if sets.remove(user).is_none() {
                    return Ok(());
                }
            }
        }
        if sets.is_empty() {
            self.conn.delete_meta(WORKING_SETS_KEY)?;
        } else {
            let text = serde_json::to_string(&sets).expect("serialises");
            self.conn.set_meta(WORKING_SETS_KEY, &text)?;
        }
        self.working_sets = sets;
        Ok(())
    }

    /// Whether the rules of shared towers apply anywhere in this project:
    /// a working set given, or a part.
    pub fn shared_rules_in_use(&self) -> bool {
        self.part.is_some() || !self.working_sets.is_empty()
    }

    /// The routes whose results are out of date, and why.
    pub fn stale_routes(&self) -> Result<BTreeMap<String, Stale>, SessionError> {
        Ok(match self.conn.meta(STALE_KEY)? {
            Some(t) => serde_json::from_str(&t)
                .map_err(|e| SessionError::Store(format!("{STALE_KEY}: {e}")))?,
            None => BTreeMap::new(),
        })
    }

    /// Marks `routes` out of date with notice `what` by `user` (country
    /// server, phase 10: a check on a network's working set whose
    /// pulled-in neighbours changed, written into its replica as phase 8
    /// writes a shared tower's move — the same marks, the same key; the
    /// file's format unchanged). Its entry: the log's last (0 when empty).
    pub fn mark_stale(
        &self,
        user: &str,
        routes: &[String],
        what: &str,
    ) -> Result<(), SessionError> {
        if routes.is_empty() {
            return Ok(());
        }
        let seq = self.last_seq()?;
        let notices: BTreeMap<String, String> = routes
            .iter()
            .map(|r| (r.clone(), what.to_owned()))
            .collect();
        Self::write_stale(&*self.conn, seq, user, &notices)
    }

    /// `user` marks route `route`'s results up to date (after checking
    /// it again). Refused: a user whose working set does not hold it.
    pub fn clear_stale(&self, user: &str, route: &str) -> Result<(), SessionError> {
        self.require(user, crate::Role::Designer)?;
        if let Some(w) = self.working_sets.get(user)
            && !w.contains(route)
        {
            return Err(refused(format!(
                "route {route} is not in your working set: its users mark it up to date"
            )));
        }
        let mut stale = self.stale_routes()?;
        if stale.remove(route).is_none() {
            return Ok(());
        }
        if stale.is_empty() {
            self.conn.delete_meta(STALE_KEY)?;
        } else {
            let text = serde_json::to_string(&stale).expect("serialises");
            self.conn.set_meta(STALE_KEY, &text)?;
        }
        Ok(())
    }

    /// Reads the working sets and a part's recorded users.
    pub(crate) fn read_shared(&mut self) -> Result<(), SessionError> {
        self.working_sets = match self.conn.meta(WORKING_SETS_KEY)? {
            Some(t) => serde_json::from_str(&t)
                .map_err(|e| SessionError::Store(format!("{WORKING_SETS_KEY}: {e}")))?,
            None => BTreeMap::new(),
        };
        self.part_users = match self.conn.meta(PART_USERS_KEY)? {
            Some(t) => serde_json::from_str(&t)
                .map_err(|e| SessionError::Store(format!("{PART_USERS_KEY}: {e}")))?,
            None => BTreeMap::new(),
        };
        Ok(())
    }

    /// The routes of line objects of the line worked on, as the rules
    /// above count them (see the module's notes): for a country server
    /// keeping each object's routes with every write (ADR-0013, phase 5).
    /// Objects that are not line objects, and objects not in the line:
    /// none.
    pub(crate) fn routes_of_objects(
        &mut self,
        objects: &[(Kind, String)],
    ) -> Vec<BTreeSet<String>> {
        self.line_index();
        let index = self.index.as_ref().expect("made above");
        let ctx = Ctx {
            line: &self.line,
            index,
            routes: None,
            part: None,
            recorded: &BTreeMap::new(),
            geo: RefCell::new(None),
            of_structure: RefCell::new(HashMap::new()),
        };
        objects
            .iter()
            .map(|(kind, id)| ctx.routes_now(*kind, id))
            .collect()
    }

    /// The objects of the line using structure `id` that `keep` does not
    /// hold (for a part: [`PART_USERS_KEY`]).
    pub(crate) fn users_outside(
        &self,
        index: &LineIndex,
        id: &str,
        keep: &BTreeSet<(Of, String)>,
    ) -> Vec<User> {
        let ctx = Ctx {
            line: &self.line,
            index,
            routes: None,
            part: None,
            recorded: &BTreeMap::new(),
            geo: RefCell::new(None),
            of_structure: RefCell::new(HashMap::new()),
        };
        ctx.users(id)
            .into_iter()
            .filter(|u| {
                of_kind(u.user.kind).is_some_and(|of| !keep.contains(&(of, u.user.id.clone())))
            })
            .map(|u| u.user)
            .collect()
    }

    /// Checks a command of `user` (expanded into puts and deletes,
    /// `prims`) against the rules of shared towers (see the module's
    /// notes). None: no working set applies.
    pub(crate) fn check_shared(
        &mut self,
        user: &str,
        prims: &[Command],
    ) -> Result<Option<Outcome>, SessionError> {
        let in_part = self.part.is_some();
        if !in_part && !self.working_sets.contains_key(user) {
            return Ok(None);
        }
        self.line_index();
        let index = self.index.as_ref().expect("made above");
        let ctx = Ctx {
            line: &self.line,
            index,
            routes: if in_part {
                None
            } else {
                self.working_sets.get(user)
            },
            part: self.part.as_ref(),
            recorded: &self.part_users,
            geo: RefCell::new(None),
            of_structure: RefCell::new(HashMap::new()),
        };
        let after = after_values(prims);
        let mut out = Outcome::default();
        let stale = |out: &mut Outcome, routes: &[String], what: &str| {
            for r in routes {
                out.notices
                    .entry(r.clone())
                    .and_modify(|w: &mut String| {
                        if !w.split("; ").any(|x| x == what) {
                            w.push_str("; ");
                            w.push_str(what);
                        }
                    })
                    .or_insert_with(|| what.to_owned());
            }
        };
        // Structures first: their refusals say the most.
        for ((kind, id), value) in &after {
            if *kind != Kind::Structure {
                continue;
            }
            let before = index.structure(&self.line, id);
            let Some(before) = before else {
                // A new structure: not one of an outside route's own.
                if let Some(v) = value {
                    let s = structure_of(v)?;
                    let r: BTreeSet<String> = s.routes.iter().cloned().collect();
                    if !in_part && !r.is_empty() && ctx.outside_of(&r).len() == r.len() {
                        return Err(refused(format!(
                            "structure {id} would be on {} only, {}",
                            routes_word(&ctx.outside_of(&r)),
                            not_in(false)
                        )));
                    }
                }
                continue;
            };
            let after_s = match value {
                Some(v) => Some(structure_of(v)?),
                None => None,
            };
            // Put as it is: nothing to check.
            if after_s.as_ref() == Some(before) {
                continue;
            }
            let routes = ctx.structure_routes(before);
            let outside_routes = ctx.outside_of(&routes);
            let users = ctx.users(id);
            let outside: Vec<&Use> = users.iter().filter(|u| u.outside).collect();
            // The other routes' own structure: by the routes it gives
            // itself, else by what uses it (only they), else by its place
            // (an alignment passing near it is not enough to take it over).
            let theirs = if !before.routes.is_empty() {
                outside_routes.len() == routes.len()
            } else if !users.is_empty() {
                users.iter().all(|u| u.foreign)
            } else {
                !routes.is_empty() && outside_routes.len() == routes.len()
            };
            if !in_part && theirs {
                let of: BTreeSet<String> = if before.routes.is_empty() && !outside.is_empty() {
                    outside
                        .iter()
                        .flat_map(|u| ctx.outside_of(&u.user.routes.iter().cloned().collect()))
                        .collect()
                } else {
                    outside_routes.iter().cloned().collect()
                };
                return Err(refused(format!(
                    "structure {id} is {}'s only, {}: it is changed with that route in the \
                     working set",
                    routes_word(&of.into_iter().collect::<Vec<_>>()),
                    not_in(false)
                )));
            }
            for u in &outside {
                out.involved
                    .extend(ctx.outside_of(&u.user.routes.iter().cloned().collect()));
            }
            // Rule 1 (and 7): deleted or renamed.
            if after_s.is_none() {
                if let Some(u) = outside.first() {
                    return Err(refused(format!(
                        "structure {id} is used by {}{}, {}: it cannot be deleted or renamed \
                         (rule 1)",
                        u.user.named(),
                        match outside.len() {
                            1 => String::new(),
                            n => format!(" and {} other object(s)", n - 1),
                        },
                        not_in(in_part)
                    )));
                }
                if !in_part {
                    self.check_named(&ctx, id)?;
                }
                continue;
            }
            let after_s = after_s.expect("checked");
            // Rules 2–5.
            for u in &outside {
                if let Some(why) = breaks(before, &after_s, &u.user) {
                    return Err(refused(format!("{why}, {}", not_in(in_part))));
                }
            }
            // Rule 6: an outside route taken off it.
            if !in_part {
                let gone: Vec<String> = if before.routes.is_empty() {
                    if after_s.routes.is_empty() {
                        // By its place: still within reach of the outside
                        // routes it stood by (not tied: a tied point
                        // follows it).
                        let tied = ctx.tied_routes(id);
                        let near = if before.position == after_s.position {
                            routes.clone()
                        } else {
                            let mut n = ctx.near(after_s.position);
                            n.extend(tied.iter().cloned());
                            n
                        };
                        outside_routes
                            .iter()
                            .filter(|r| !near.contains(*r))
                            .cloned()
                            .collect()
                    } else {
                        outside_routes
                            .iter()
                            .filter(|r| !after_s.routes.contains(r))
                            .cloned()
                            .collect()
                    }
                } else {
                    before
                        .routes
                        .iter()
                        .filter(|r| outside_routes.contains(r) && !after_s.routes.contains(r))
                        .cloned()
                        .collect()
                };
                if !gone.is_empty() {
                    return Err(refused(format!(
                        "structure {id} is on {}, {}: it cannot be taken off it (rule 6)",
                        routes_word(&gone),
                        not_in(false)
                    )));
                }
            }
            // §7.4: hardware only the other routes use.
            if before.structure_type.is_none() && after_s.structure_type.is_none() {
                self.check_their_hardware(before, &after_s, &users, in_part)?;
            }
            // Allowed: the outside routes using it (or on it) are stale.
            let mut touched: BTreeSet<String> = outside
                .iter()
                .flat_map(|u| ctx.outside_of(&u.user.routes.iter().cloned().collect()))
                .collect();
            if !in_part {
                touched.extend(outside_routes.iter().cloned());
            }
            if !touched.is_empty() {
                let what = describe(before, &after_s);
                stale(&mut out, &touched.into_iter().collect::<Vec<_>>(), &what);
            }
        }
        // The other line objects changed (rules 0 and 6).
        if !in_part {
            for ((kind, id), value) in &after {
                if of_kind(*kind).is_none() || *kind == Kind::Structure {
                    continue;
                }
                let before_r = ctx.routes_now(*kind, id);
                let after_r = match value {
                    Some(v) => ctx.routes_of_value(*kind, id, v),
                    None => BTreeSet::new(),
                };
                let exists = match of_kind(*kind) {
                    Some(of) => index.position(of, id).is_some(),
                    None => false,
                };
                // A point tied to a structure of the outside routes only:
                // it would make that structure the user's route's too.
                if *kind == Kind::AlignmentPoint
                    && let Some(v) = value
                    && let Some(s) = v.get("structure").and_then(Value::as_str)
                    && index
                        .alignment_point(&self.line, id)
                        .is_none_or(|p| p.structure.as_deref() != Some(s))
                {
                    let r = ctx.routes_of_id(s);
                    let outside_r = ctx.outside_of(&r);
                    if !r.is_empty() && outside_r.len() == r.len() {
                        return Err(refused(format!(
                            "structure {s} is on {} only, {}: alignment point {id} cannot be \
                             tied to it here (add the route to your working set)",
                            routes_word(&outside_r),
                            not_in(false)
                        )));
                    }
                }
                if *kind == Kind::AlignmentPoint && exists && value.is_some() {
                    // A point of an outside route: moved only (with its
                    // structure, or a junction moved).
                    let p = index.alignment_point(&self.line, id).expect("there");
                    let q: ohl_line::AlignmentPoint =
                        serde_json::from_value((*value.expect("put")).clone())
                            .map_err(|e| refused(format!("alignment point {id}: {e}")))?;
                    let outside_r = ctx.outside_of(&before_r);
                    if outside_r.is_empty() {
                        continue;
                    }
                    if p.structure != q.structure {
                        return Err(refused(format!(
                            "alignment point {id} is on {}, {}: {} (rule 6)",
                            routes_word(&outside_r),
                            not_in(false),
                            match &p.structure {
                                Some(s) => format!("it cannot be untied from {s}"),
                                None => "it cannot be tied to a structure here".into(),
                            }
                        )));
                    }
                    let follows = p
                        .structure
                        .as_ref()
                        .is_some_and(|s| after.contains_key(&(Kind::Structure, s.clone())));
                    if outside_r.len() == before_r.len() && !follows {
                        return Err(refused(format!(
                            "alignment point {id} is on {} only, {}",
                            routes_word(&outside_r),
                            not_in(false)
                        )));
                    }
                    if q.description != p.description {
                        return Err(refused(format!(
                            "alignment point {id} is on {}, {}",
                            routes_word(&outside_r),
                            not_in(false)
                        )));
                    }
                    if !follows && p.position != q.position {
                        let d =
                            (q.position[0] - p.position[0]).hypot(q.position[1] - p.position[1]);
                        stale(
                            &mut out,
                            &outside_r,
                            &format!("alignment point {id} moved {d:.2} m"),
                        );
                    }
                    continue;
                }
                let mut r = before_r.clone();
                r.extend(after_r);
                if ctx.outside_routes(&r) {
                    let outside_r = ctx.outside_of(&r);
                    let doing = match (exists, value.is_some()) {
                        (false, _) => "it cannot be made here",
                        (true, true) => "it cannot be changed here",
                        (true, false) => "it cannot be deleted here",
                    };
                    return Err(refused(format!(
                        "{} {id} is of {}, {}: {doing} (add the route to your working set)",
                        kind.as_str().replace('_', " "),
                        routes_word(&outside_r),
                        not_in(false)
                    )));
                }
            }
            // Settings and library files: every outside route may change.
            let settings = after
                .keys()
                .any(|(k, id)| (*k == Kind::Meta && id != "circuit_starts") || *k == Kind::Library);
            if settings {
                let all: BTreeSet<String> = self.line.routes.iter().map(|r| r.id.clone()).collect();
                let outside_r = ctx.outside_of(&all);
                stale(
                    &mut out,
                    &outside_r,
                    "the line's settings or library files changed",
                );
            }
        }
        // A protected project line: a review by the other routes' owners.
        if !out.notices.is_empty() && self.find_ref(&self.current)?.is_some_and(|r| r.protected) {
            let routes: Vec<String> = out.notices.keys().cloned().collect();
            return Err(refused(format!(
                "this changes {} ({}) on project line \"{}\", which is protected: it needs a \
                 review by its owners — make the change on a project line of your own and \
                 submit it for review into \"{}\"",
                routes_word(&routes),
                out.notices.values().cloned().collect::<Vec<_>>().join("; "),
                self.current,
                self.current
            )));
        }
        Ok(Some(out))
    }

    /// Rule 7: structure `id` deleted or renamed while a circuit start of
    /// an outside section or a structure group naming an outside route's
    /// structures names it.
    fn check_named(&self, ctx: &Ctx<'_>, id: &str) -> Result<(), SessionError> {
        for p in self
            .line
            .circuit_starts
            .iter()
            .filter(|p| p.structure == id)
        {
            for s in ctx.index.at_point(id, p.set, Of::Section) {
                let r = ctx.routes_of_section(s);
                if ctx.outside_routes(&r) {
                    return Err(refused(format!(
                        "structure {id} holds the circuit start of section {s} of {}, {}: it \
                         cannot be deleted or renamed (rule 7)",
                        routes_word(&ctx.outside_of(&r)),
                        not_in(false)
                    )));
                }
            }
        }
        for g in &self.line.structure_groups {
            if !g.structures.iter().chain(&g.excluded).any(|s| s == id) {
                continue;
            }
            for s in g.structures.iter().chain(&g.excluded) {
                let r = ctx.routes_of_id(s);
                if ctx.outside_routes(&r) {
                    return Err(refused(format!(
                        "structure {id} is named in structure group \"{}\", which names {s} of \
                         {}, {}: it cannot be deleted or renamed (rule 7)",
                        g.label(),
                        routes_word(&ctx.outside_of(&r)),
                        not_in(false)
                    )));
                }
            }
        }
        Ok(())
    }

    /// §7.4: refuses changing the hardware of untyped structure `before`
    /// that only outside users use (their wire points and what only they
    /// hang on).
    fn check_their_hardware(
        &self,
        before: &Structure,
        after: &Structure,
        users: &[Use],
        in_part: bool,
    ) -> Result<(), SessionError> {
        // The wire points used, by label: used by the user's routes too,
        // used by the other routes only, by whom.
        let mut used: BTreeMap<String, (bool, Vec<&Use>)> = BTreeMap::new();
        for u in users {
            let labels = u
                .user
                .sets
                .iter()
                .filter_map(|s| before.wire_at_set(*s).map(|w| w.label.clone()))
                .chain(u.user.clamps.iter().cloned());
            for l in labels {
                let e = used.entry(l).or_insert((false, Vec::new()));
                if u.foreign {
                    e.1.push(u);
                } else {
                    e.0 = true;
                }
            }
        }
        // Each part: the wire points whose hanging it belongs to.
        let mut belongs: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for label in used.keys() {
            if let Some(h) = hang(before, label) {
                for p in &h.parts {
                    belongs
                        .entry(p.label().to_owned())
                        .or_default()
                        .push(label.as_str());
                }
            }
        }
        let (b, a) = (hardware_local(before), hardware_local(after));
        let changed = b
            .iter()
            .filter(|(k, v)| !same_held(a.get(*k), Some(*v)))
            .map(|(k, _)| k)
            .chain(a.keys().filter(|k| !b.contains_key(*k)));
        for (_, label) in changed {
            let Some(of) = belongs.get(label) else {
                continue;
            };
            let theirs = of.iter().all(|l| {
                used.get(*l)
                    .is_some_and(|(inside, out)| !inside && !out.is_empty())
            });
            if theirs {
                let u = of
                    .iter()
                    .find_map(|l| used.get(*l).and_then(|(_, out)| out.first()))
                    .expect("used outside");
                return Err(refused(format!(
                    "{label} of {} is the hardware of {} only, {}: it is changed with that \
                     route in the working set (plan §7.4)",
                    before.id,
                    u.user.named(),
                    not_in(in_part)
                )));
            }
        }
        Ok(())
    }

    /// Writes the stale marks of a command's notices, as entry `seq` of
    /// `user` (in the command's step).
    pub(crate) fn write_stale(
        store: &dyn Store,
        seq: i64,
        user: &str,
        notices: &BTreeMap<String, String>,
    ) -> Result<(), SessionError> {
        let mut stale: BTreeMap<String, Stale> = match store.meta(STALE_KEY)? {
            Some(t) => serde_json::from_str(&t)
                .map_err(|e| SessionError::Store(format!("{STALE_KEY}: {e}")))?,
            None => BTreeMap::new(),
        };
        for (route, what) in notices {
            let s = stale.entry(route.clone()).or_insert(Stale {
                count: 0,
                since: seq,
                notices: Vec::new(),
            });
            s.count += 1;
            s.notices.push(Notice {
                seq,
                user: user.to_owned(),
                what: what.clone(),
            });
            if s.notices.len() > NOTICES_KEPT {
                let extra = s.notices.len() - NOTICES_KEPT;
                s.notices.drain(..extra);
            }
        }
        let text = serde_json::to_string(&stale).expect("serialises");
        store.set_meta(STALE_KEY, &text)
    }
}

/// The connection conflicts of a merge (see the module's notes): a
/// structure changed on one side only, objects on it made or changed on
/// the other whose connection the change breaks. `always`: said even
/// where the merged line would be valid (working sets in use); else only
/// where it would not be (before: refused with nothing to choose).
/// Settled by `choices` like the deletions' conflicts: the side changing
/// the structure (what the other put on it then not taken) or the other.
pub fn connection_conflicts(
    base: &crate::versions::State,
    ours: &crate::versions::State,
    theirs: &crate::versions::State,
    merged: &mut crate::versions::State,
    conflicts: &mut Vec<crate::versions::Conflict>,
    choices: &crate::versions::Choices,
    always: bool,
) {
    use crate::versions::Side;
    let same = |a: Option<&std::sync::Arc<Value>>, b: Option<&std::sync::Arc<Value>>| match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b) || a == b,
        _ => false,
    };
    let side = |s: Side| match s {
        Side::Ours => ours,
        _ => theirs,
    };
    let mut found = Vec::new();
    for (key, b) in &base.objects {
        if key.0 != Kind::Structure {
            continue;
        }
        let (o, t) = (ours.objects.get(key), theirs.objects.get(key));
        let changer = match (
            same(Some(&b.1), o.map(|x| &x.1)),
            same(Some(&b.1), t.map(|x| &x.1)),
        ) {
            (true, false) => Side::Theirs,
            (false, true) => Side::Ours,
            _ => continue,
        };
        let keeper = if changer == Side::Ours {
            Side::Theirs
        } else {
            Side::Ours
        };
        // Deleted: the deletions' conflicts say it already.
        let Some(changed) = side(changer).objects.get(key) else {
            continue;
        };
        let (Ok(before), Ok(after)) = (
            serde_json::from_value::<Structure>((*b.1).clone()),
            serde_json::from_value::<Structure>((*changed.1).clone()),
        ) else {
            continue;
        };
        // What the keeping side made or changed on it.
        let mut on_it = Vec::new();
        for (m, v) in &side(keeper).objects {
            if !matches!(m.0, Kind::Section | Kind::Jumper)
                || same(base.objects.get(m).map(|x| &x.1), Some(&v.1))
                || !merged
                    .objects
                    .get(m)
                    .is_some_and(|x| same(Some(&x.1), Some(&v.1)))
            {
                continue;
            }
            let Some(u) = user_in(m.0, &m.1, &v.1, &key.1) else {
                continue;
            };
            if let Some(why) = breaks(&before, &after, &u) {
                on_it.push((m.clone(), why));
            }
        }
        if !on_it.is_empty() {
            found.push((key.clone(), b.clone(), changer, on_it));
        }
    }
    if found.is_empty() {
        return;
    }
    if !always && merged.line().is_ok() {
        return;
    }
    for (key, b, changer, on_it) in found {
        let okey = format!("{}:{}", key.0.as_str(), key.1);
        // Already a conflict of its own (both sides): left to it.
        if conflicts.iter().any(|c| c.key == okey) {
            continue;
        }
        let chosen = choices.get(&okey).copied().filter(|s| *s != Side::Both);
        let names: Vec<String> = on_it
            .iter()
            .map(|(m, _)| format!("{} {}", m.0.as_str(), m.1))
            .collect();
        conflicts.push(crate::versions::Conflict {
            key: okey,
            what: format!(
                "{} {} (changed on one line; on the other, on it: {} — {})",
                key.0.as_str(),
                key.1,
                names.join(", "),
                on_it[0].1
            ),
            base: Some((*b.1).clone()),
            ours: ours.objects.get(&key).map(|x| (*x.1).clone()),
            theirs: theirs.objects.get(&key).map(|x| (*x.1).clone()),
            chosen,
            keep_both: None,
        });
        if chosen.unwrap_or(Side::Ours) == changer {
            // The change taken: what the other side put on it is not.
            for (m, _) in on_it {
                match side(changer).objects.get(&m) {
                    Some(x) => merged.objects.insert(m, x.clone()),
                    None => merged.objects.remove(&m),
                };
            }
        } else {
            merged.objects.insert(key, b);
        }
    }
}

/// How object `kind`/`id` (its JSON `value`) uses structure `structure`.
fn user_in(kind: Kind, id: &str, value: &Value, structure: &str) -> Option<User> {
    let (sets, clamps) = match kind {
        Kind::Section => {
            let s: ohl_line::Section = serde_json::from_value(value.clone()).ok()?;
            let sets: Vec<u32> = s
                .points
                .iter()
                .filter(|p| p.structure == structure)
                .map(|p| p.set)
                .collect();
            (sets, Vec::new())
        }
        Kind::Jumper => {
            let j: ohl_line::Jumper = serde_json::from_value(value.clone()).ok()?;
            let sets: Vec<u32> = [&j.from, &j.to]
                .into_iter()
                .filter_map(|e| match e {
                    JumperEnd::Wire(p) if p.structure == structure => Some(p.set),
                    _ => None,
                })
                .collect();
            let clamps: Vec<String> = j
                .supports()
                .into_iter()
                .filter(|c| c.structure_id(&j) == structure)
                .map(|c| c.wire)
                .collect();
            (sets, clamps)
        }
        _ => return None,
    };
    (!sets.is_empty() || !clamps.is_empty()).then(|| User {
        kind,
        id: id.to_owned(),
        routes: Vec::new(),
        sets,
        clamps,
        tied: false,
    })
}
