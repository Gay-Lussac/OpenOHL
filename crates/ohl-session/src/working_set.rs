// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The working set of a client of a country server (ADR-0013, phase 6):
//! what a user picks to work on, snapped out to whole tension sections,
//! and the **partial replica** that holds it on the user's computer, fed
//! by the server with the rows that change (never a copy of the whole
//! line). Nothing here changes what a project file holds or how the
//! desktop works alone: a replica is a project file holding a part of a
//! line ([`mod@crate::part`]) with a few meta keys more.
//!
//! **Picking** ([`Pick`], owner 2026-11-16): whole routes; a part of a
//! route by station range or from one structure to another; an area drawn
//! on the map (a polygon). [`Session::extent_of`] works out the
//! [`Extent`] on the session holding the whole model (the server):
//!
//! - a part (a station range, two structures, an area) takes **every
//!   section with a span in it, whole** — a section is solved whole — and
//!   the structures they hang on. Other sections on those structures with
//!   no span in it (circuits straining elsewhere) are not taken (they
//!   would grow a part to the whole route: measured on the synthetic
//!   country) and said in the notes; the context tiles draw them;
//! - with them the jumpers at its structures or tapped on its sections,
//!   the spacers on its sections and the alignment points tied to its
//!   structures; a whole route takes every object of the route;
//! - the **context** (in the replica, read-only): what the working set
//!   refers to, all the way (another route's section ending at a jumper,
//!   a structure's routes), the routes it is on with their alignments and
//!   points (the stations need them: a point tied to a structure brings
//!   that structure too), the line's settings and library files.
//!
//! **The replica** is written by [`Session::write_working_set`]: the rows
//! of the extent as the server holds them (their JSON and their places,
//! so the replica's order is the server's), the working set noted
//! (`part`), the objects outside it using its structures noted
//! (`part_users`, the rules of shared towers), and the network's keys.
//! **Fed by rows** ([`Rows`]): after a change of the model the server
//! works the client's extent out again and sends the rows of what changed
//! in it, what came into it and what left it ([`Session::rows_for`]); a
//! replica joining again sends what it holds, each object with a hash of
//! its JSON ([`object_hash`]), and gets only what differs
//! ([`Session::rows_against`]). The replica takes them as they are
//! ([`Session::take_rows`]: the server has validated them; no cascade, the
//! part's own refusals do not apply).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ohl_line::index::{LineIndex, Of};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::in_place::{kind_of, of_kind};
use crate::part::{PART_KEY, PartSummary};
use crate::shared::{PART_USERS_KEY, User};
use crate::{Applied, Command, Kind, Role, Session, SessionError, refused};

/// The `ols_meta` key of a replica holding its picks and its name (JSON,
/// [`WorkingSetSpec`]).
pub const WORKING_SET_KEY: &str = "network.working_set";

/// A pick of a working set (owner, 2026-11-16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "pick", rename_all = "snake_case")]
pub enum Pick {
    /// A whole route.
    Route {
        /// The route's id.
        route: String,
    },
    /// A part of a route by station range (m, along `alignment`, else the
    /// route's first alignment).
    Stations {
        /// The route's id.
        route: String,
        /// The alignment the stations are along (none: the route's first).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alignment: Option<String>,
        /// From station, m.
        from: f64,
        /// To station, m.
        to: f64,
    },
    /// A part of a route from one structure to another (both on it).
    Structures {
        /// The route's id.
        route: String,
        /// The first structure.
        from: String,
        /// The last structure.
        to: String,
    },
    /// An area drawn on the map: a polygon in plan (x, y), three corners
    /// or more.
    Area {
        /// Its corners, in order.
        polygon: Vec<[f64; 2]>,
    },
}

impl Pick {
    /// In words.
    pub fn describe(&self) -> String {
        match self {
            Pick::Route { route } => format!("route {route}"),
            Pick::Stations {
                route, from, to, ..
            } => format!("route {route} from station {from:.0} m to {to:.0} m"),
            Pick::Structures { route, from, to } => {
                format!("route {route} from structure {from} to {to}")
            }
            Pick::Area { polygon } => format!("an area of {} corners", polygon.len()),
        }
    }
}

/// A working set as it is saved and asked for: its picks, on one model.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WorkingSetSpec {
    /// Its name when saved (empty: not saved).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The model (line) it is on.
    pub model: String,
    /// What was picked.
    pub picks: Vec<Pick>,
}

/// What a working set holds of one route, snapped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteExtent {
    /// The route.
    pub route: String,
    /// Picked whole.
    pub whole: bool,
    /// The alignment its stations are along (the route's first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<String>,
    /// The first and last stations of the working set's structures on it,
    /// m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<f64>,
    /// See `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<f64>,
    /// The structures at those stations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
    /// See `first`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
    /// The working set's structures on it.
    pub structures: usize,
    /// Their places in plan, in station order (to draw the extent).
    pub outline: Vec<[f64; 2]>,
}

/// A working set in numbers, to show before it is confirmed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExtentSummary {
    /// By route.
    pub routes: Vec<RouteExtent>,
    /// Structures of the working set.
    pub structures: usize,
    /// Sections.
    pub sections: usize,
    /// Jumpers.
    pub jumpers: usize,
    /// Spacers.
    pub spacers: usize,
    /// Objects of the context (read-only in the replica).
    pub context: usize,
    /// The working set's structures' bounds in plan (x0, y0, x1, y1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[f64; 4]>,
    /// What the snapping did, in words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A working set worked out on the whole model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Extent {
    /// The line objects of the working set (editable).
    pub wanted: BTreeSet<(Kind, String)>,
    /// What the replica holds: the working set, its context, the line's
    /// settings and its library files.
    pub keep: BTreeSet<(Kind, String)>,
    /// In numbers.
    pub summary: ExtentSummary,
}

/// An object as the server holds it, sent to a replica.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// Its kind.
    pub kind: Kind,
    /// Its id.
    pub id: String,
    /// Its place among its kind (the server's).
    pub ord: i64,
    /// Its JSON.
    pub value: Value,
}

/// What a replica takes to follow the server ([`Session::take_rows`]).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Rows {
    /// Objects new or changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puts: Vec<Row>,
    /// Objects gone, or out of the extent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deletes: Vec<(Kind, String)>,
    /// The working set, when it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wanted: Option<Vec<(Kind, String)>>,
    /// The objects outside the replica using its structures, when they
    /// changed (`part_users`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub users: Option<BTreeMap<String, Vec<User>>>,
}

impl Rows {
    /// Nothing to take.
    pub fn is_empty(&self) -> bool {
        self.puts.is_empty()
            && self.deletes.is_empty()
            && self.wanted.is_none()
            && self.users.is_none()
    }
}

/// What a replica holds ([`Session::replica_of`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Replica {
    /// Its objects, as the server holds them.
    pub rows: Vec<Row>,
    /// Its working set.
    pub wanted: Vec<(Kind, String)>,
    /// The objects outside it using its structures (`part_users`).
    pub users: BTreeMap<String, Vec<User>>,
}

/// A route's alignments and structures, to pick a part of it
/// ([`Session::route_detail`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteDetail {
    /// The route.
    pub route: String,
    /// Its name.
    pub name: String,
    /// Its alignments, in the line's order (stations along the first).
    pub alignments: Vec<AlignmentLength>,
    /// Its structures, in station order (those without a station last).
    pub structures: Vec<StructureStation>,
}

/// An alignment and its length in plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlignmentLength {
    /// Its id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its length in plan, m.
    pub length: f64,
}

/// A structure of a route and its station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructureStation {
    /// Its id.
    pub id: String,
    /// Its station along the route's first alignment, m (none: the route
    /// has no alignment of two points).
    pub station: Option<f64>,
    /// Its place in plan.
    pub position: [f64; 2],
}

/// A hash of an object's JSON (64-bit FNV-1a of its text as serde writes
/// it): what a replica joining again says it holds.
pub fn object_hash(value: &Value) -> u64 {
    let text = serde_json::to_string(value).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Whether plan point `p` is inside polygon `poly` (even-odd).
fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            c = !c;
        }
        j = i;
    }
    c
}

/// Whether plan segments `a`–`b` and `c`–`d` cross or touch.
fn crosses(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    let on = |p: [f64; 2], q: [f64; 2], r: [f64; 2], v: f64| {
        v == 0.0
            && r[0] >= p[0].min(q[0])
            && r[0] <= p[0].max(q[0])
            && r[1] >= p[1].min(q[1])
            && r[1] <= p[1].max(q[1])
    };
    on(c, d, a, d1) || on(c, d, b, d2) || on(a, b, c, d3) || on(a, b, d, d4)
}

/// A segment touching a polygon: an end inside, or crossing an edge.
fn segment_in(poly: &[[f64; 2]], a: [f64; 2], b: [f64; 2]) -> bool {
    if inside(poly, a) || inside(poly, b) {
        return true;
    }
    let n = poly.len();
    (0..n).any(|i| crosses(a, b, poly[i], poly[(i + 1) % n]))
}

fn xy(p: &[f64; 3]) -> [f64; 2] {
    [p[0], p[1]]
}

/// The kinds a replica keeps whole (not part of the line).
fn kept_whole(kind: Kind) -> bool {
    matches!(kind, Kind::Meta | Kind::Library)
}

impl Session {
    /// The working set a user picks (`picks`) on the line worked on (the
    /// whole model), snapped out to whole tension sections (see the
    /// module's notes). Refused: no pick, an unknown route or structure, a
    /// structure not on its route, a route without an alignment of two
    /// points for a station range, a polygon of fewer than three corners,
    /// picks holding nothing.
    ///
    /// Expected cost: the routes of every line object once (the reference
    /// index and the alignments' grid), then what the picks touch; a few
    /// milliseconds for a line of a few hundred structures.
    pub fn pick_extent(&mut self, picks: &[Pick]) -> Result<Extent, SessionError> {
        self.extent(picks, true)
    }

    /// The working set of `picks` worked out again after the model
    /// changed: as [`Self::pick_extent`], but a pick that no longer holds
    /// (its route or structure gone, moved off its route) is left out and
    /// said in the notes, and a working set left empty is empty (the
    /// replica then holds its context only) — never refused.
    pub fn extent_of(&mut self, picks: &[Pick]) -> Extent {
        self.extent(picks, false).unwrap_or_default()
    }

    fn extent(&mut self, picks: &[Pick], strict: bool) -> Result<Extent, SessionError> {
        if picks.is_empty() && strict {
            return Err(refused(
                "pick at least one route, a part of a route or an area",
            ));
        }
        // Every line object with its routes.
        let all: Vec<(Kind, String)> = {
            let l = &self.line;
            let mut v = Vec::new();
            v.extend(l.routes.iter().map(|x| (Kind::Route, x.id.clone())));
            v.extend(l.alignments.iter().map(|x| (Kind::Alignment, x.id.clone())));
            v.extend(
                l.alignment_points
                    .iter()
                    .map(|x| (Kind::AlignmentPoint, x.id.clone())),
            );
            v.extend(l.structures.iter().map(|x| (Kind::Structure, x.id.clone())));
            v.extend(l.sections.iter().map(|x| (Kind::Section, x.id.clone())));
            v.extend(l.jumpers.iter().map(|x| (Kind::Jumper, x.id.clone())));
            v.extend(l.spacers.iter().map(|x| (Kind::Spacer, x.id.clone())));
            v
        };
        let routes_of: BTreeMap<(Kind, String), BTreeSet<String>> = all
            .iter()
            .cloned()
            .zip(self.routes_of_objects(&all))
            .collect();
        self.line_index();
        let index = self.index.as_ref().expect("made above");
        let line = &self.line;
        let on = |kind: Kind, id: &str, route: &str| {
            routes_of
                .get(&(kind, id.to_owned()))
                .is_some_and(|r| r.contains(route))
        };
        let position = |id: &str| index.structure(line, id).map(|s| xy(&s.position));
        // The polyline of a route's alignment (given, else its first).
        let polyline = |route: &str,
                        alignment: Option<&str>|
         -> Result<(String, Vec<[f64; 3]>), SessionError> {
            let a = line
                .alignments
                .iter()
                .find(|a| a.route == route && alignment.is_none_or(|x| x == a.id))
                .ok_or_else(|| {
                    refused(match alignment {
                        Some(a) => format!("route {route} has no alignment {a}"),
                        None => format!("route {route} has no alignment"),
                    })
                })?;
            let points: Vec<[f64; 3]> = a
                .points
                .iter()
                .filter_map(|p| index.alignment_point(line, p).map(|p| p.position))
                .collect();
            if points.len() < 2 {
                return Err(refused(format!(
                    "alignment {} has fewer than two points: no stations",
                    a.id
                )));
            }
            Ok((a.id.clone(), points))
        };
        let station = |poly: &[[f64; 3]], id: &str| {
            index
                .structure(line, id)
                .map(|s| ohl_line::route::project(poly, s.position).0)
        };
        let mut whole: BTreeSet<String> = BTreeSet::new();
        let mut routes: BTreeSet<String> = BTreeSet::new();
        let mut seeds: BTreeSet<String> = BTreeSet::new();
        let mut lone: BTreeSet<String> = BTreeSet::new();
        let mut wanted: BTreeSet<(Kind, String)> = BTreeSet::new();
        let mut notes: Vec<String> = Vec::new();
        for pick in picks {
            // A pick that does not hold: refused (`strict`), else left out.
            macro_rules! check {
                ($e:expr) => {
                    match $e {
                        Ok(v) => v,
                        Err(e) => {
                            if strict {
                                return Err(e);
                            }
                            notes.push(format!("{} left out: {e}", pick.describe()));
                            continue;
                        }
                    }
                };
            }
            let route_known = |r: &str| -> Result<(), SessionError> {
                index
                    .position(Of::Route, r)
                    .map(|_| ())
                    .ok_or_else(|| refused(format!("no route {r}")))
            };
            // A station range of a route: the sections with a span in it,
            // its structures without one.
            let mut range = |route: &str, poly: &[[f64; 3]], from: f64, to: f64| {
                let (lo, hi) = (from.min(to), from.max(to));
                for s in &line.sections {
                    if !on(Kind::Section, &s.id, route) {
                        continue;
                    }
                    let st: Vec<Option<f64>> = s
                        .points
                        .iter()
                        .map(|p| station(poly, &p.structure))
                        .collect();
                    let hit = st.windows(2).any(|w| match (w[0], w[1]) {
                        (Some(a), Some(b)) => a.min(b) <= hi && a.max(b) >= lo,
                        _ => false,
                    }) || st.iter().flatten().any(|x| (lo..=hi).contains(x));
                    if hit {
                        seeds.insert(s.id.clone());
                    }
                }
                for s in &line.structures {
                    if on(Kind::Structure, &s.id, route)
                        && station(poly, &s.id).is_some_and(|x| (lo..=hi).contains(&x))
                    {
                        lone.insert(s.id.clone());
                    }
                }
            };
            match pick {
                Pick::Route { route } => {
                    check!(route_known(route));
                    whole.insert(route.clone());
                    routes.insert(route.clone());
                    for (kind, id) in &all {
                        if on(*kind, id, route) {
                            wanted.insert((*kind, id.clone()));
                        }
                    }
                }
                Pick::Stations {
                    route,
                    alignment,
                    from,
                    to,
                } => {
                    check!(route_known(route));
                    if !from.is_finite() || !to.is_finite() {
                        check!(Err(refused("give the stations as numbers")));
                    }
                    let (_, poly) = check!(polyline(route, alignment.as_deref()));
                    routes.insert(route.clone());
                    range(route, &poly, *from, *to);
                }
                Pick::Structures { route, from, to } => {
                    check!(route_known(route));
                    for s in [from, to] {
                        if index.position(Of::Structure, s).is_none() {
                            check!(Err(refused(format!("no structure {s}"))));
                        }
                        if !on(Kind::Structure, s, route) {
                            check!(Err(refused(format!(
                                "structure {s} is not on route {route}"
                            ))));
                        }
                    }
                    let (_, poly) = check!(polyline(route, None));
                    let (a, b) = (
                        station(&poly, from).unwrap_or(0.0),
                        station(&poly, to).unwrap_or(0.0),
                    );
                    routes.insert(route.clone());
                    range(route, &poly, a, b);
                    lone.insert(from.clone());
                    lone.insert(to.clone());
                }
                Pick::Area { polygon } => {
                    if polygon.len() < 3 || polygon.iter().flatten().any(|v| !v.is_finite()) {
                        check!(Err(refused("an area needs three corners or more")));
                    }
                    for s in &line.sections {
                        let pts: Vec<Option<[f64; 2]>> =
                            s.points.iter().map(|p| position(&p.structure)).collect();
                        let hit = pts.windows(2).any(|w| match (w[0], w[1]) {
                            (Some(a), Some(b)) => segment_in(polygon, a, b),
                            _ => false,
                        }) || pts.iter().flatten().any(|p| inside(polygon, *p));
                        if hit {
                            seeds.insert(s.id.clone());
                            if let Some(r) = routes_of.get(&(Kind::Section, s.id.clone())) {
                                routes.extend(r.iter().cloned());
                            }
                        }
                    }
                    for s in &line.structures {
                        if inside(polygon, xy(&s.position)) {
                            lone.insert(s.id.clone());
                            if let Some(r) = routes_of.get(&(Kind::Structure, s.id.clone())) {
                                routes.extend(r.iter().cloned());
                            }
                        }
                    }
                }
            }
        }
        // Snapped out: each section with a span in a pick whole, with the
        // structures it hangs on. Other sections on those structures with
        // no span in a pick (other circuits straining elsewhere) are not
        // taken: they would grow a part of a route to the whole route.
        let sections: BTreeSet<String> = seeds;
        let mut structures: BTreeSet<String> = lone;
        for id in &sections {
            if let Some(s) = index.section(line, id) {
                for p in &s.points {
                    structures.insert(p.structure.clone());
                }
            }
        }
        let beside: BTreeSet<&String> = structures
            .iter()
            .flat_map(|t| index.referrers_of(Of::Structure, t, Of::Section))
            .filter(|s| !sections.contains(*s))
            .collect();
        if !beside.is_empty() && whole.len() < routes.len() {
            notes.push(format!(
                "{} other section(s) hang on the working set's structures with no span in it \
                 (circuits straining elsewhere): not in it, drawn as context",
                beside.len()
            ));
        }
        for s in &sections {
            wanted.insert((Kind::Section, s.clone()));
            for j in index.referrers_of(Of::Section, s, Of::Jumper) {
                wanted.insert((Kind::Jumper, j.clone()));
            }
            for x in index.referrers_of(Of::Section, s, Of::Spacer) {
                wanted.insert((Kind::Spacer, x.clone()));
            }
        }
        for t in &structures {
            wanted.insert((Kind::Structure, t.clone()));
            for j in index.referrers_of(Of::Structure, t, Of::Jumper) {
                wanted.insert((Kind::Jumper, j.clone()));
            }
            for p in index.referrers_of(Of::Structure, t, Of::AlignmentPoint) {
                wanted.insert((Kind::AlignmentPoint, p.clone()));
            }
        }
        if strict && !wanted.iter().any(|(k, _)| *k != Kind::Route) {
            return Err(refused(format!(
                "{} holds no structure or section",
                picks
                    .iter()
                    .map(Pick::describe)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        // The context: what it refers to, all the way; the routes with
        // their alignments and points; then what those refer to.
        let mut queue: Vec<(Of, String)> = wanted
            .iter()
            .filter_map(|(k, id)| of_kind(*k).map(|of| (of, id.clone())))
            .collect();
        for r in &routes {
            queue.push((Of::Route, r.clone()));
            for a in line.alignments.iter().filter(|a| a.route == *r) {
                queue.push((Of::Alignment, a.id.clone()));
            }
        }
        let mut keep_of: BTreeSet<(Of, String)> = BTreeSet::new();
        while let Some((of, id)) = queue.pop() {
            if index.position(of, &id).is_none() || !keep_of.insert((of, id.clone())) {
                continue;
            }
            let Some(item) = index.get(line, of, &id) else {
                continue;
            };
            for t in item.targets() {
                let (of, id) = match t {
                    ohl_line::index::Target::Object(of, id) => (of, id),
                    ohl_line::index::Target::Point(structure, _) => (Of::Structure, structure),
                };
                if !keep_of.contains(&(of, id.clone())) {
                    queue.push((of, id));
                }
            }
        }
        let mut keep: BTreeSet<(Kind, String)> = keep_of
            .iter()
            .map(|(of, id)| (kind_of(*of), id.clone()))
            .collect();
        wanted.retain(|k| keep.contains(k));
        keep.extend(self.objects.keys().filter(|(k, _)| kept_whole(*k)).cloned());
        // In numbers, by route.
        let mut summary = ExtentSummary::default();
        let mut bounds: Option<[f64; 4]> = None;
        for (k, id) in &wanted {
            match k {
                Kind::Structure => {
                    summary.structures += 1;
                    if let Some(p) = position(id) {
                        let b = bounds.get_or_insert([p[0], p[1], p[0], p[1]]);
                        *b = [
                            b[0].min(p[0]),
                            b[1].min(p[1]),
                            b[2].max(p[0]),
                            b[3].max(p[1]),
                        ];
                    }
                }
                Kind::Section => summary.sections += 1,
                Kind::Jumper => summary.jumpers += 1,
                Kind::Spacer => summary.spacers += 1,
                _ => {}
            }
        }
        summary.bounds = bounds;
        summary.context = keep.len() - wanted.len();
        for r in &routes {
            let ours: Vec<&String> = wanted
                .iter()
                .filter(|(k, id)| *k == Kind::Structure && on(*k, id, r))
                .map(|(_, id)| id)
                .collect();
            let mut e = RouteExtent {
                route: r.clone(),
                whole: whole.contains(r),
                alignment: None,
                from: None,
                to: None,
                first: None,
                last: None,
                structures: ours.len(),
                outline: Vec::new(),
            };
            if let Ok((a, poly)) = polyline(r, None) {
                let mut at: Vec<(f64, &String)> = ours
                    .iter()
                    .filter_map(|id| station(&poly, id).map(|s| (s, *id)))
                    .collect();
                at.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(y.1)));
                e.alignment = Some(a);
                e.from = at.first().map(|x| x.0);
                e.to = at.last().map(|x| x.0);
                e.first = at.first().map(|x| x.1.clone());
                e.last = at.last().map(|x| x.1.clone());
                e.outline = at.iter().filter_map(|(_, id)| position(id)).collect();
            }
            if !e.whole
                && let (Some(a), Some(b)) = (e.from, e.to)
            {
                notes.push(format!(
                    "route {r}: snapped out to whole tension sections, stations {a:.0}–{b:.0} m"
                ));
            }
            summary.routes.push(e);
        }
        summary.notes = notes;
        Ok(Extent {
            wanted,
            keep,
            summary,
        })
    }

    /// The structures' users outside `keep` (a replica's `part_users`).
    fn users_of(&self, index: &LineIndex, extent: &Extent) -> BTreeMap<String, Vec<User>> {
        let keep_of: BTreeSet<(Of, String)> = extent
            .keep
            .iter()
            .filter_map(|(k, id)| of_kind(*k).map(|of| (of, id.clone())))
            .collect();
        extent
            .wanted
            .iter()
            .filter(|(k, _)| *k == Kind::Structure)
            .map(|(_, id)| (id.clone(), self.users_outside(index, id, &keep_of)))
            .filter(|(_, u)| !u.is_empty())
            .collect()
    }

    /// The `part_users` of `extent` on the line worked on.
    pub fn extent_users(&mut self, extent: &Extent) -> BTreeMap<String, Vec<User>> {
        self.line_index();
        let index = self.index.as_ref().expect("made above");
        self.users_of(index, extent)
    }

    /// The row of object `key`, if the session holds it.
    fn row(&self, key: &(Kind, String)) -> Option<Row> {
        self.objects.get(key).map(|o| Row {
            kind: key.0,
            id: key.1.clone(),
            ord: o.ord,
            value: (*o.value).clone(),
        })
    }

    /// What a replica of `extent` holds, taken from this session (the whole
    /// model): its rows as this session holds them (their JSON and places),
    /// its working set and its structures' users — a copy of the extent's
    /// objects only, cheap enough for the authority to take between two
    /// commands; [`Session::write_replica`] writes it (in another thread).
    pub fn replica_of(&mut self, extent: &Extent) -> Replica {
        Replica {
            users: self.extent_users(extent),
            rows: extent.keep.iter().filter_map(|k| self.row(k)).collect(),
            wanted: extent.wanted.iter().cloned().collect(),
        }
    }

    /// Writes the replica of `extent` (worked out on this session, the
    /// whole model) into a new project file at `path`
    /// ([`Session::replica_of`], then [`Session::write_replica`]).
    pub fn write_working_set(
        &mut self,
        extent: &Extent,
        path: &Path,
        user: &str,
        meta: &[(String, String)],
    ) -> Result<PartSummary, SessionError> {
        if path.exists() {
            return Err(refused(format!("{}: exists already", path.display())));
        }
        let replica = self.replica_of(extent);
        Session::write_replica(replica, path, user, meta)
    }

    /// Writes `replica` into a new project file at `path` (refused when it
    /// exists): its rows as one import by `user` (the replica's admin), the
    /// working set and its structures' users noted, and `meta` (the
    /// network's keys) beside them.
    pub fn write_replica(
        replica: Replica,
        path: &Path,
        user: &str,
        meta: &[(String, String)],
    ) -> Result<PartSummary, SessionError> {
        if path.exists() {
            return Err(refused(format!("{}: exists already", path.display())));
        }
        let Replica {
            rows,
            wanted,
            users,
        } = replica;
        let total = rows.len();
        let commands: Vec<Command> = rows
            .into_iter()
            .map(|r| Command::Put {
                kind: r.kind,
                id: r.id,
                value: r.value,
                ord: Some(r.ord),
            })
            .collect();
        let mut part = Session::open(path)?;
        if part.users()?.is_empty() {
            part.set_user(user, user, Role::Admin)?;
        }
        part.apply_as(
            user,
            Command::Batch { commands },
            "import",
            None,
            true,
            false,
        )?;
        part.conn.set_meta(
            PART_KEY,
            &serde_json::to_string(&wanted).expect("serialises"),
        )?;
        let n = wanted.len();
        part.part = Some(wanted.into_iter().collect());
        if !users.is_empty() {
            part.conn.set_meta(
                PART_USERS_KEY,
                &serde_json::to_string(&users).expect("serialises"),
            )?;
        }
        part.part_users = users;
        for (k, v) in meta {
            part.conn.set_meta(k, v)?;
        }
        part.checkpoint()?;
        Ok(PartSummary {
            wanted: n,
            context: total.saturating_sub(n),
        })
    }

    /// The structures of route `route` with their stations along its first
    /// alignment, in station order, and its alignments with their lengths
    /// (to pick a part of it).
    pub fn route_detail(&mut self, route: &str) -> Result<RouteDetail, SessionError> {
        let r = self
            .line
            .routes
            .iter()
            .find(|r| r.id == route)
            .ok_or_else(|| refused(format!("no route {route}")))?;
        let name = r.name.clone();
        let ids: Vec<(Kind, String)> = self
            .line
            .structures
            .iter()
            .map(|s| (Kind::Structure, s.id.clone()))
            .collect();
        let routes = self.routes_of_objects(&ids);
        let line = &self.line;
        let point = |p: &String| line.alignment_point(p).map(|p| p.position);
        let mut alignments = Vec::new();
        let mut first: Option<Vec<[f64; 3]>> = None;
        for a in line.alignments.iter().filter(|a| a.route == route) {
            let poly: Vec<[f64; 3]> = a.points.iter().filter_map(point).collect();
            let length = poly
                .windows(2)
                .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                .sum();
            alignments.push(AlignmentLength {
                id: a.id.clone(),
                name: a.name.clone(),
                length,
            });
            if first.is_none() && poly.len() >= 2 {
                first = Some(poly);
            }
        }
        let mut structures: Vec<StructureStation> = ids
            .iter()
            .zip(&routes)
            .filter(|(_, r)| r.contains(route))
            .filter_map(|((_, id), _)| {
                let s = line.structure(id)?;
                Some(StructureStation {
                    id: id.clone(),
                    station: first
                        .as_ref()
                        .map(|poly| ohl_line::route::project(poly, s.position).0),
                    position: xy(&s.position),
                })
            })
            .collect();
        structures.sort_by(|a, b| {
            a.station
                .unwrap_or(f64::INFINITY)
                .total_cmp(&b.station.unwrap_or(f64::INFINITY))
                .then(a.id.cmp(&b.id))
        });
        Ok(RouteDetail {
            route: route.to_owned(),
            name,
            alignments,
            structures,
        })
    }

    /// The rows a replica of extent `old` needs to become the replica of
    /// `new` after the objects `changed` (puts and deletes of this
    /// session's last command, or of a library) changed: what changed in
    /// it, what came into it, what left it; the working set and its
    /// structures' users when they differ (`old_users`).
    pub fn rows_for(
        &mut self,
        old: &Extent,
        old_users: &BTreeMap<String, Vec<User>>,
        new: &Extent,
        changed: &BTreeSet<(Kind, String)>,
    ) -> Rows {
        let mut rows = Rows::default();
        for k in &new.keep {
            if (!old.keep.contains(k) || changed.contains(k))
                && let Some(r) = self.row(k)
            {
                rows.puts.push(r);
            }
        }
        for k in &old.keep {
            if !new.keep.contains(k) || (changed.contains(k) && !self.objects.contains_key(k)) {
                rows.deletes.push(k.clone());
            }
        }
        if new.wanted != old.wanted {
            rows.wanted = Some(new.wanted.iter().cloned().collect());
        }
        let users = self.extent_users(new);
        if users != *old_users {
            rows.users = Some(users);
        }
        rows
    }

    /// The rows a replica holding `held` (each object with its
    /// [`object_hash`]) needs to become the replica of `new`: the objects
    /// it lacks or holds otherwise, those it holds and should not; the
    /// working set and its structures' users always.
    pub fn rows_against(&mut self, held: &BTreeMap<(Kind, String), u64>, new: &Extent) -> Rows {
        let mut rows = Rows::default();
        for k in &new.keep {
            if let Some(r) = self.row(k)
                && held.get(k) != Some(&object_hash(&r.value))
            {
                rows.puts.push(r);
            }
        }
        for k in held.keys() {
            if !new.keep.contains(k) {
                rows.deletes.push(k.clone());
            }
        }
        rows.wanted = Some(new.wanted.iter().cloned().collect());
        rows.users = Some(self.extent_users(new));
        rows
    }

    /// What this replica holds, each object with its [`object_hash`] (the
    /// line objects, the settings and the library files).
    pub fn held(&self) -> BTreeMap<(Kind, String), u64> {
        self.objects
            .iter()
            .filter(|((k, _), _)| kept_whole(*k) || of_kind(*k).is_some())
            .map(|(k, o)| (k.clone(), object_hash(&o.value)))
            .collect()
    }

    /// Takes the server's rows into this replica, as `user` (one log entry
    /// of kind `network`, applied in place where it can: the line kept
    /// beside the session follows [`Applied`]); the working set and its
    /// structures' users noted when given. The server validated them on
    /// the whole model: they are taken as they are (no cascade; the part's
    /// own refusals of context changes and deletions are the user's
    /// commands', not these). `None`: no object changed.
    pub fn take_rows(&mut self, user: &str, rows: Rows) -> Result<Option<Applied>, SessionError> {
        let mut commands = Vec::new();
        for r in rows.puts {
            let key = (r.kind, r.id.clone());
            if self
                .objects
                .get(&key)
                .is_some_and(|o| *o.value == r.value && o.ord == r.ord)
            {
                continue;
            }
            commands.push(Command::Put {
                kind: r.kind,
                id: r.id,
                value: r.value,
                ord: Some(r.ord),
            });
        }
        for (kind, id) in rows.deletes {
            if self.objects.contains_key(&(kind, id.clone())) {
                commands.push(Command::Delete {
                    kind,
                    id,
                    confirm: false,
                });
            }
        }
        let applied = if commands.is_empty() {
            None
        } else {
            Some(self.apply_as(
                user,
                Command::Batch { commands },
                "network",
                None,
                false,
                false,
            )?)
        };
        if let Some(wanted) = rows.wanted {
            self.conn.set_meta(
                PART_KEY,
                &serde_json::to_string(&wanted).expect("serialises"),
            )?;
            self.part = Some(wanted.into_iter().collect());
        }
        if let Some(users) = rows.users {
            self.conn.set_meta(
                PART_USERS_KEY,
                &serde_json::to_string(&users).expect("serialises"),
            )?;
            self.part_users = users;
        }
        Ok(applied)
    }

    /// The working set a replica was made for ([`WORKING_SET_KEY`]), if it
    /// is one.
    pub fn working_set_spec(&self) -> Result<Option<WorkingSetSpec>, SessionError> {
        match self.conn.meta(WORKING_SET_KEY)? {
            Some(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| SessionError::Store(format!("{WORKING_SET_KEY}: {e}"))),
            None => Ok(None),
        }
    }

    /// Notes the working set a replica follows from now on.
    pub fn set_working_set_spec(&self, spec: &WorkingSetSpec) -> Result<(), SessionError> {
        self.conn.set_meta(
            WORKING_SET_KEY,
            &serde_json::to_string(spec).expect("serialises"),
        )
    }

    /// A meta key of the project file (`ols_meta`), as the store keeps it.
    pub fn file_meta(&self, key: &str) -> Result<Option<String>, SessionError> {
        self.conn.meta(key)
    }

    /// Sets a meta key of the project file (the network's keys of a
    /// replica; never the session's own).
    pub fn set_file_meta(&self, key: &str, value: &str) -> Result<(), SessionError> {
        if !key.starts_with("network.") {
            return Err(refused(format!(
                "{key}: only the network's keys (network.…) are set this way"
            )));
        }
        self.conn.set_meta(key, value)
    }
}
