// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The checks of the joints themselves (stage 4.5 of the joint details,
//! 2026-11-19; built, in active development, not approved yet): the bolt
//! groups under the members' forces together and the bearing of the
//! member they bolt onto, gathered here per joint and per load case and
//! checked by the structure's design code's plugin (`joint-check-groups`,
//! with the bolts in tension `joint-check-groups-tension`; theory page
//! `docs/theory/joint-groups.md`).
//!
//! - **A plate's bolts onto the leg holding it** (`holding`): where the
//!   joint's leg bolts onto a gusset plate (or welded plate) that other
//!   members bolt onto too — a leg piece's own end bolted onto it, or the
//!   plate's own bolts onto the leg (`[plate.leg_bolts]`, 2026-11-19: a leg
//!   running on through the joint) — those bolts carry the resultant of
//!   the other members' forces on the plate — their end forces in the
//!   same case, each along its line through its bolts' centroid — with its
//!   moment about those bolts' centroid, shared out elastically
//!   ([`ohl_structures_fe::bolt_groups::elastic`]). The ply checked is the
//!   plate (bearing, its edges along each bolt's force).
//! - **The bolts onto a member at a joint** (`bearing`): every bolt
//!   through one member's flange at the joint — the members lapped onto it
//!   (each member's force shared by its own bolts, ASCE 10-97 4.1's equal
//!   share, a frame member's end moment elastically) and the holding bolts
//!   above — summed bolt by bolt where bolts of several parts are one
//!   physical bolt (within a quarter of a diameter). The ply checked is
//!   that member: its bearing under each bolt's resultant, its distances
//!   along and square to it.
//! - **The bolts in tension** (2026-11-19): the forces square to the plies
//!   and the moments bending them out of their plane, shared by each
//!   group's bolts elastically about their centroid
//!   ([`ohl_structures_fe::bolt_groups::tension`]), each bolt with the
//!   flanges it bends as it pulls (a T-stub's flange: the prying model of
//!   Struik and de Back, worked by the plugins).
//! - **A joint that does not fit** (owner 2026-11-19): a bolt outside the
//!   flange it bolts onto or the plate it holds, holes in one ply
//!   overlapping — the group reads "does not fit", without a usage,
//!   counted apart; never an infinite usage.
//! - **Pieces in line not said continuous** end at the joint (owner
//!   2026-11-19: on the safe side): each a member of its own there, its
//!   steel stopping at the joint.
//! - **What is not defined** is said, never silently skipped: a plate the
//!   joint's leg does not bolt onto (how it is held not given), a holding
//!   leg whose bolts are not placed, a member whose thickness is not known
//!   ([`GroupPlans::not_checked`]).
//!
//! A member's force is counted at every joint it bolts at, and only the
//! same case's forces are summed. The forces on a part are the members'
//! end forces from the analysis (a frame's end shear and moment with its
//! axial force); without them, the axial force along the member.

use std::borrow::Cow;
use std::collections::BTreeMap;

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::{BoltGrade, BoltLibrary};
use ohl_structures_fe::bolt_groups::{self as bg, Plane, PlaneLoad, V2, V3};
use ohl_structures_fe::model::Model;

use crate::joints::{Arrival, JointDetail, JointDetails};
use crate::plate_checks::{EndForces, PlateCase, PlatePart, PlateUsage};

/// Bolts of different parts nearer than this share one hole: a quarter of
/// the smaller diameter, at least 2 mm.
fn same_hole(d: f64) -> f64 {
    (0.25 * d).max(0.002)
}

/// Members in line through a joint within this angle, not said
/// continuous, are named in the notes (each ends at the joint), degrees.
const IN_LINE_DEG: f64 = 5.0;

/// Starts of the words of a group that does not fit (owner 2026-11-19).
pub const DOES_NOT_FIT: &str = "DOES NOT FIT as given";

/// Starts of the words of a group that does not fit only where the
/// placements were assumed — not given by the user, not laid out by the
/// joint layout (D14, the owner's decision of 2026-11-19): still failing,
/// never a usage or a pass, counted apart.
pub const DOES_NOT_FIT_ASSUMED: &str = "DOES NOT FIT as assumed — not detailed";

/// A flange a bolt bends as it pulls (a T-stub's flange, the prying model
/// of Struik and de Back): what the plugin's `tee` record takes.
#[derive(Debug, Clone, PartialEq)]
struct Tee {
    part: String,
    thickness: f64,
    fy: Option<f64>,
    /// From the bolt to the flange's free edge (`a`), m.
    edge: f64,
    /// From the bolt to the web's face (`b`), m.
    web: f64,
    /// Its length along the bolts' line the bolt takes (`w`), m.
    width: f64,
}

/// One physical bolt of a group.
#[derive(Debug, Clone, PartialEq)]
struct Bolt {
    /// Where it is (structure axes), m.
    at: V3,
    diameter: f64,
    hole: Option<f64>,
    ultimate: Option<f64>,
    shear: Option<f64>,
    shank: Option<f64>,
    threads_in: Option<bool>,
    /// Its library's tension capacity (its ultimate tensile load), N.
    tension: Option<f64>,
    grade: String,
    /// The planes it is checked on.
    planes: u32,
    /// The member ends and plates whose forces it carries.
    carries: Vec<String>,
    /// In a member's flange: its gauge from the heel, the flange's width
    /// and the direction across it from heel to toe.
    flange: Option<(f64, f64, V3)>,
    /// The flanges it bends as it pulls.
    tees: Vec<Tee>,
    /// The plates it bends as it pulls: flanges without a web (2026-11-19,
    /// [`plate_tee`]).
    plates: Vec<Tee>,
    /// The plies it pulls whose bending is not known, in words.
    unknown: Vec<String>,
    /// The bolt grades set on the members it carries, in words ("bolt
    /// grade 8.8 set by command: …"; owner 2026-11-19).
    grades: Vec<String>,
    /// Where it lies was given — by the user or the joint layout — not
    /// assumed by the joint details ([`placed_given`]; D14, 2026-11-19).
    given: bool,
}

/// A ply's edges.
#[derive(Debug, Clone, PartialEq)]
enum Edges {
    /// A plate's outline (empty: not known).
    Outline(Vec<V3>),
    /// A member's flange: its toe from each bolt's `flange`; its steel
    /// stopping at `end` from `origin` along `dir` (none: it runs on).
    Flange {
        dir: V3,
        origin: V3,
        end: Option<f64>,
    },
}

/// A ply checked.
#[derive(Debug, Clone, PartialEq)]
struct Ply {
    part: String,
    member: bool,
    /// A member whose toe is given flame-cut or sheared ([`toe_sheared`]).
    sheared: bool,
    thickness: f64,
    fy: Option<f64>,
    fu: Option<f64>,
    redundant: bool,
    known: bool,
    edges: Edges,
    /// Every hole in it at the joint (structure axes).
    holes: Vec<V3>,
    /// The bolts push it along their force (+1: the member they bolt
    /// onto) or against it (−1: the plate a holding bolt holds).
    sign: f64,
}

/// A member end whose force reaches the group's part.
#[derive(Debug, Clone, PartialEq)]
struct Source {
    member: String,
    end: usize,
    direction: V3,
    /// Its bolts' centroid on the part (the joint where not placed).
    centroid: V3,
    /// Bearing groups: its bolts here, (index in the group, where).
    bolts: Vec<(usize, V3)>,
    /// The member ends laid onto this member near its end whose forces
    /// reach these bolts too (L14, [`riders`]): each along its line
    /// through its own bolts' centroid, the same case.
    riders: Vec<Source>,
    /// The other holder of a level plate this member (or one of its
    /// riders) holds, and the ends laid onto that holder (L21, [`passed`]):
    /// the force the plate may pass between its two holders — each taken
    /// from none to the whole of its force, the larger bolt by bolt.
    passed: Vec<Source>,
}

/// One group, planned once per structure.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroupPlan {
    joint: String,
    holding: bool,
    part: String,
    /// Holding: the member(s) holding the plate, in words.
    holders: String,
    position: V3,
    plane: Plane,
    bolts: Vec<Bolt>,
    plies: Vec<Ply>,
    sources: Vec<Source>,
    /// Holding groups whose bolts are among these: (their index, (their
    /// bolt, ours)).
    from: Vec<(usize, Vec<(usize, usize)>)>,
    notes: Vec<String>,
    /// Why it does not fit as given (empty: it fits): the row reads "does
    /// not fit", without a usage.
    misfits: Vec<String>,
    /// A misfit lies on a placement given (by the user or the joint
    /// layout); false with misfits: they lie only on placements assumed —
    /// "does not fit as assumed — not detailed" (D14, 2026-11-19).
    misfit_given: bool,
    /// A member's own end bolts with the ends laid onto it (L14,
    /// [`end_bolt_groups`]): those ends in words; the group's `part` is
    /// the member.
    ends_of: Option<String>,
    /// A level plate's holding group (L21): the plate's other holder and
    /// the ends laid onto it — the force the plate may pass to these bolts,
    /// each from none to whole (with the other ends' forces whole or none),
    /// the larger bolt by bolt.
    passed: Vec<Source>,
}

/// A member's own steel at its end bolts with the ends laid onto it near
/// its end (the owner's answer of 2026-11-22 to L14's question 2): its
/// tension capacity as the code's plugin gives it — the least over its ends
/// of its net section, its block shear and its bolts' — against the larger
/// of its own force and the resultant through those bolts.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EndSteel {
    pub(crate) joint: String,
    pub(crate) member: String,
    /// From the joint along it (unit).
    pub(crate) direction: V3,
    /// The ends laid onto it (and onto those in turn): each and its
    /// direction from the joint.
    pub(crate) riders: Vec<(String, V3)>,
    /// The force a level plate may pass to it from the plate's other
    /// holder (L21, [`passed`]): each member and its direction, each from
    /// none to whole.
    pub(crate) passed: Vec<(String, V3)>,
    /// Its tension capacity (N) and what governs it, the plugin's words.
    pub(crate) capacity: f64,
    pub(crate) governs: String,
}

impl EndSteel {
    /// The force at its end bolts in a case (`axial`, tension positive): the
    /// larger of its own force alone and the resultant of its own and the
    /// riders' forces (each along its line), N (a size); (L21) each of
    /// these also with every choice of the forces a level plate may pass to
    /// it (`passed`, each none or whole), the largest.
    pub(crate) fn force(&self, axial: &BTreeMap<String, f64>) -> f64 {
        let n = |m: &str| axial.get(m).copied().unwrap_or(0.0);
        let own = scale(self.direction, n(&self.member));
        let mut r = own;
        for (m, d) in &self.riders {
            r = add(r, scale(*d, n(m)));
        }
        let k = self.passed.len().min(PASSED_EACH);
        let pass: Vec<(V3, V3)> = self.passed[..k]
            .iter()
            .map(|(m, d)| (scale(*d, n(m)), [0.0; 3]))
            .collect();
        // Beyond PASSED_EACH: each one's size added (a size of a sum is at
        // most the sum of the sizes: on the safe side).
        let rest: f64 = self.passed[k..].iter().map(|(m, _)| n(m).abs()).sum();
        with_passed(vec![(own, [0.0; 3]), (r, [0.0; 3])], &pass)
            .into_iter()
            .map(|(f, _)| norm(f))
            .fold(0.0, f64::max)
            + rest
    }
}

/// The groups of a structure and what is not checked.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct GroupPlans {
    pub(crate) groups: Vec<GroupPlan>,
    /// What at a joint is not checked: (joint, part, words).
    pub(crate) not_checked: Vec<(String, String, String)>,
    /// Members' own steel at their end bolts with ends laid onto them
    /// (L14's question 2).
    pub(crate) end_steel: Vec<EndSteel>,
}

impl GroupPlans {
    /// Nothing planned.
    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.not_checked.is_empty() && self.end_steel.is_empty()
    }

    /// Every row's part in words, as the rows of a case name them.
    fn part_of(&self, k: usize) -> PlatePart {
        let g = &self.groups[k];
        if g.holding {
            PlatePart::Holding(g.holders.clone())
        } else if let Some(w) = &g.ends_of {
            PlatePart::EndBolts(w.clone())
        } else {
            PlatePart::Bearing
        }
    }
}

/// The members in line with `id` through joint `j`, `id` among them,
/// sorted: a member running on as one part (`continuous`). And the
/// members in line with it within [`IN_LINE_DEG`] not said continuous
/// (owner 2026-11-19: each ends at the joint, on the safe side — named
/// for the notes).
fn run_at(j: &JointDetail, id: &str) -> (Vec<String>, Vec<String>) {
    let mut out = vec![id.to_owned()];
    let mut unsaid = Vec::new();
    let Some(a) = j.arrivals.iter().find(|x| x.member == id) else {
        return (out, unsaid);
    };
    for b in j.arrivals.iter().filter(|b| b.member != id) {
        let said = a.continuous.as_deref() == Some(b.member.as_str())
            || b.continuous.as_deref() == Some(id);
        let line = dot(a.direction, b.direction) <= -IN_LINE_DEG.to_radians().cos();
        if said {
            out.push(b.member.clone());
        } else if line && !b.modelling {
            unsaid.push(b.member.clone());
        }
    }
    out.sort();
    (out, unsaid)
}

/// The note of `run` in line with `unsaid` at joint `j`, not said
/// continuous: each ends there (none: nothing in line).
fn ends_note(run: &[String], unsaid: &[String], j: &str) -> Option<String> {
    (!unsaid.is_empty()).then(|| {
        format!(
            "{} in line with {} at {j}, not said continuous: each taken as ending at the joint \
             (on the safe side; say continuous where they are one piece)",
            run.join("/"),
            unsaid.join(", ")
        )
    })
}

/// Whether library bolt `l`'s strengths agree with the grade `g` set on
/// its member's bolts (none set: they do): its ultimate strength and grade
/// not given or the same.
fn grade_agrees(l: &ohl_structures::bolts::BoltType, g: Option<&BoltGrade>) -> bool {
    let Some(g) = g else {
        return true;
    };
    let fu = g.ultimate.si();
    l.ultimate.is_none_or(|u| (u.si() - fu).abs() <= 1e-6 * fu)
        && l.grade
            .as_deref()
            .is_none_or(|x| x.trim() == g.grade.trim())
}

/// The bolt data of member end `a` (its bolt set over the member's), with
/// its library's tension capacity (its ultimate tensile load; dropped
/// where a grade set on the member's bolts differs from the library's) and
/// the grade set on the member's bolts over the bolts' own (owner
/// 2026-11-19).
/// Whether where end `a`'s bolts lie was given — by the user, or written by
/// the joint layout (a `[[connection]]` at the end, a plate's own bolts, the
/// member's design data) — rather than assumed by the joint details (D14,
/// the owner's decision of 2026-11-19): what it bolts onto, its gauges, its
/// pitch, its end distance or where its steel stops. The count of its
/// gauge lines alone (an import's holes) is not where its bolts lie.
pub(crate) fn placed_given(a: &Arrival) -> bool {
    use crate::joints::Source as S;
    let given = |s: S| matches!(s, S::Given | S::Member);
    given(a.onto.source)
        || given(a.gauges.source)
        || given(a.pitch.source)
        || given(a.end_distance.source)
        || given(a.cut.source)
}

fn bolt_of(a: &Arrival, model: &Model, at: V3, bolts: &BoltLibrary) -> Option<Bolt> {
    let set = a.bolts.value.as_ref()?;
    let idx = model.members.iter().position(|m| m.id == a.member);
    let (merged, shank, threads) = idx
        .and_then(|k| {
            let m = &model.members[k];
            let e = m.design.ends.get(a.end).and_then(Option::as_ref);
            ohl_structures_fe::member_ends::end_bolts(&m.design, e, &mut Vec::new())
        })
        .map_or((None, None, None), |(b, s, t)| (Some(b), s, t));
    let set_grade = idx.and_then(|k| model.members[k].design.bolt_grade.as_ref());
    // The end's own bolts when they are this library bolt (their capacity
    // already dropped for another grade), else the library's.
    let own = idx
        .and_then(|k| model.members[k].design.ends.get(a.end))
        .and_then(Option::as_ref)
        .filter(|e| e.bolt.is_some() && e.bolt == set.bolt)
        .and_then(|e| e.bolts.as_ref());
    let tension = match own {
        Some(e) => e.tension_capacity,
        None => set
            .bolt
            .as_deref()
            .and_then(|id| bolts.get(id))
            .filter(|l| grade_agrees(l, set_grade))
            .and_then(|b| b.tension_capacity)
            .map(|f| f.si()),
    };
    Some(Bolt {
        at,
        diameter: set.diameter,
        hole: set.hole.or(merged.and_then(|b| b.hole)),
        // The grade set on the member's bolts, else the end's, else its
        // library bolt's (a plate's own bolts onto a leg running on: the
        // leg's end has none of its own there).
        ultimate: set_grade
            .map(|g| g.ultimate.si())
            .or(merged.and_then(|b| b.ultimate))
            .or_else(|| {
                set.bolt
                    .as_deref()
                    .and_then(|id| bolts.get(id))
                    .and_then(|l| l.ultimate)
                    .map(|u| u.si())
            }),
        shear: merged.and_then(|b| b.shear_capacity),
        shank,
        threads_in: threads,
        tension,
        grade: set_grade
            .map(|g| g.grade.trim().to_owned())
            .or_else(|| set.grade.clone())
            .or_else(|| merged.and_then(|b| b.grade).map(|g| g.name()))
            .unwrap_or_default(),
        planes: a.planes.0.value.unwrap_or(1).max(1),
        carries: vec![format!(
            "{} at {}",
            a.member,
            ["its first end", "its second end"][a.end.min(1)]
        )],
        flange: None,
        tees: Vec::new(),
        plates: Vec::new(),
        unknown: Vec::new(),
        grades: set_grade.map(BoltGrade::words).into_iter().collect(),
        given: placed_given(a),
    })
}

/// The bolt grades set on the members `bolts` carry, once each, for a
/// group's notes.
fn grade_notes(bolts: &[Bolt]) -> Vec<String> {
    let mut v = Vec::new();
    for w in bolts.iter().flat_map(|b| b.grades.iter()) {
        push_once(&mut v, format!("bolts: {w}"));
    }
    v
}

/// Adds bolt `b` to `bolts` (one hole with a bolt already there: what it
/// carries added, the weaker values kept, the flanges it bends added),
/// giving its index.
fn add_bolt(bolts: &mut Vec<Bolt>, b: Bolt, notes: &mut Vec<String>) -> usize {
    let near = same_hole(b.diameter);
    if let Some(k) = bolts
        .iter()
        .position(|x| norm(sub(x.at, b.at)) <= near.max(same_hole(x.diameter)))
    {
        let x = &mut bolts[k];
        if (x.diameter - b.diameter).abs() > 1e-6 {
            push_once(
                notes,
                "bolts of different sizes in one hole: the smaller taken (on the safe side)".into(),
            );
        }
        x.diameter = x.diameter.min(b.diameter);
        x.ultimate = min_opt(x.ultimate, b.ultimate);
        x.shear = min_opt(x.shear, b.shear);
        x.shank = min_opt(x.shank, b.shank);
        x.tension = match (x.tension, b.tension) {
            (Some(p), Some(q)) => Some(p.min(q)),
            // One of them not given: not known.
            _ => None,
        };
        x.planes = 1;
        for c in b.carries {
            if !x.carries.contains(&c) {
                x.carries.push(c);
            }
        }
        if x.flange.is_none() {
            x.flange = b.flange;
        }
        for t in b.tees {
            put_tee(&mut x.tees, t);
        }
        for t in b.plates {
            put_tee(&mut x.plates, t);
        }
        for w in b.unknown {
            push_once(&mut x.unknown, w);
        }
        for w in b.grades {
            push_once(&mut x.grades, w);
        }
        return k;
    }
    bolts.push(b);
    bolts.len() - 1
}

/// Puts `t` among `tees`: a flange of the same part replaced by the one
/// that takes the shorter length (the more conservative).
fn put_tee(tees: &mut Vec<Tee>, t: Tee) {
    match tees.iter_mut().find(|x| x.part == t.part) {
        Some(x) => {
            if t.width < x.width {
                *x = t;
            }
        }
        None => tees.push(t),
    }
}

fn min_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) | (None, x) => x,
    }
}

fn push_once(v: &mut Vec<String>, w: String) {
    if !v.contains(&w) {
        v.push(w);
    }
}

/// The centroid of points (the fallback where there are none).
fn centroid3(points: &[V3], fallback: V3) -> V3 {
    if points.is_empty() {
        return fallback;
    }
    let n = points.len() as f64;
    points
        .iter()
        .fold([0.0; 3], |s, q| add(s, scale(*q, 1.0 / n)))
}

/// A member's thickness and steel as a ply.
fn member_ply(a: &Arrival, model: &Model) -> Option<(f64, Option<f64>, Option<f64>, bool)> {
    let m = model.members.iter().find(|m| m.id == a.member)?;
    let s = &model.sections[m.section];
    let t = a
        .profile
        .as_ref()
        .and_then(|p| p.angle.map(|g| g.t))
        .or(m.design.thickness)?;
    let redundant = m.design.class == Some(ohl_structures_fe::design::MemberClass::Redundant);
    Some((t, s.yield_strength, s.ultimate, redundant))
}

/// A member's toe at its end `a` is given flame-cut or sheared
/// (`rolled_edge = false` at that end's connection; 2026-11-26, O44 L16
/// question 4): its ply is sent as `member-sheared` — the code's sheared
/// edge. Not given: a member's leg, its toe rolled (`member`), as before.
fn toe_sheared(a: &Arrival, model: &Model) -> bool {
    model
        .members
        .iter()
        .find(|m| m.id == a.member)
        .and_then(|m| m.design.ends.get(a.end))
        .and_then(Option::as_ref)
        .and_then(|e| e.rolled_edge)
        == Some(false)
}

/// A member's yield strength (its section's).
fn member_fy(id: &str, model: &Model) -> Option<f64> {
    let m = model.members.iter().find(|m| m.id == id)?;
    model.sections[m.section].yield_strength
}

/// The flange of `part` a bolt at `gauge` from its heel bends as it
/// pulls (the prying model's T-stub flange; theory page): `width` from
/// heel to toe, thickness `t`; the bolt at `along` on the bolts' line, the
/// flange's bolts at `others` (along, gauge), its steel stopping at `end`
/// (none: it runs on), diameter `d`. The length it takes along the line:
/// to the middle between it and the next bolt on its gauge line each way,
/// at most to the flange's end and to `b − d/2` each way (a lone bolt's
/// spread taken no further: on the safe side). None: the bolt off the
/// flange.
#[allow(clippy::too_many_arguments)]
fn flange_tee(
    part: &str,
    t: f64,
    fy: Option<f64>,
    gauge: f64,
    width: f64,
    along: f64,
    others: &[[f64; 2]],
    end: Option<f64>,
    d: f64,
) -> Option<Tee> {
    if outside(gauge, width) {
        return None;
    }
    let edge = (width - gauge).max(0.0);
    let web = gauge - t;
    let cap = (web - d / 2.0).max(0.0);
    let side = |sign: f64| -> f64 {
        let mut w = cap;
        for o in others.iter().filter(|o| (o[1] - gauge).abs() <= d / 2.0) {
            let x = (o[0] - along) * sign;
            if x > 1e-4 {
                w = w.min(x / 2.0);
            }
        }
        if let Some(e) = end {
            let x = (e - along) * sign;
            if x >= 0.0 {
                w = w.min(x);
            }
        }
        w
    };
    Some(Tee {
        part: part.to_owned(),
        thickness: t,
        fy,
        edge,
        web,
        width: side(1.0) + side(-1.0),
    })
}

/// The plate `part` a holding bolt at `at` bends as it pulls (2026-11-19,
/// the owner's decision of that day: the same T-stub model as an angle's
/// flange; theory page): a flange **without a web** — the pull reaches it
/// through the bolts of the members on it (`pulls`), which hold it against
/// no turning. Thickness `t`, yield strength `fy`; `n` the plate's normal,
/// `line` the holding bolts' line (the leg's), `others` the holding bolts,
/// `flange` the leg's flange under the bolt (its gauge from the heel, its
/// width, across it from heel to toe), `outline` the plate's, `d` the
/// bolt's diameter.
///
/// - `web` (b): in the plate's plane, square to the holding bolts' line,
///   from the bolt to the **farthest** bolt of a member on the plate (on the
///   safe side: the prying and the plate's bending grow with it);
/// - `edge` (a): from the bolt the other way to where the plate stops
///   bearing on the leg — its outline or the leg's flange's toe or heel,
///   the nearer;
/// - `width` (w): along the line, to the middle between it and the next
///   holding bolt on its line each way, at most to the outline and to b −
///   d/2 each way (as a flange's).
///
/// None: not known — no member's bolt on the plate placed, the bolt off the
/// outline, no outline.
#[allow(clippy::too_many_arguments)]
fn plate_tee(
    part: &str,
    t: f64,
    fy: Option<f64>,
    at: V3,
    n: V3,
    line: V3,
    pulls: &[V3],
    others: &[V3],
    flange: Option<(f64, f64, V3)>,
    outline: &[V3],
    d: f64,
) -> Option<Tee> {
    if outline.len() < 3 {
        return None;
    }
    let nn = scale(n, 1.0 / norm(n).max(1e-12));
    let flat = |v: V3| sub(v, scale(nn, dot(v, nn)));
    let u0 = flat(line);
    let u = scale(u0, 1.0 / norm(u0).max(1e-12));
    if norm(u0) < 1e-9 {
        return None;
    }
    let across = |p: V3| {
        let r = flat(sub(p, at));
        sub(r, scale(u, dot(r, u)))
    };
    let (b, dir) = pulls
        .iter()
        .map(|p| across(*p))
        .map(|c| (norm(c), c))
        .max_by(|x, y| x.0.total_cmp(&y.0))?;
    if b < 1e-4 {
        return None;
    }
    let e = scale(dir, 1.0 / b);
    let away = scale(e, -1.0);
    let mut a = crate::plate_checks::ray_to_outline(outline, at, away)?;
    if let Some((g, w, toe)) = flange {
        let s = dot(away, toe);
        if s > 1e-6 {
            a = a.min((w - g).max(0.0) / s);
        } else if s < -1e-6 {
            a = a.min(g.max(0.0) / -s);
        }
    }
    let cap = (b - d / 2.0).max(0.0);
    let side = |sign: f64| -> f64 {
        let dir = scale(u, sign);
        let mut w = cap;
        if let Some(x) = crate::plate_checks::ray_to_outline(outline, at, dir) {
            w = w.min(x);
        }
        for o in others {
            let r = flat(sub(*o, at));
            let x = dot(r, dir);
            if x > 1e-4 && norm(sub(r, scale(u, dot(r, u)))) <= d / 2.0 {
                w = w.min(x / 2.0);
            }
        }
        w
    };
    Some(Tee {
        part: part.to_owned(),
        thickness: t,
        fy,
        edge: a.max(0.0),
        web: b,
        width: side(1.0) + side(-1.0),
    })
}

/// Bolts on more than two gauge lines of one flange (`gauges`, m; lines
/// apart by more than half a diameter `d`): the outer ones are not
/// effective in tension without stiffening (Kulak, Fisher & Struik 17.6.1).
fn many_lines(gauges: &[f64], d: f64) -> bool {
    let mut lines: Vec<f64> = Vec::new();
    for g in gauges {
        if !lines.iter().any(|x| (x - g).abs() <= d / 2.0) {
            lines.push(*g);
        }
    }
    lines.len() > 2
}

/// The flange tees of member end `a`'s own bolts (in its leg bolted) and
/// whether it is an angle: each bolt's tee, in the order of its bolts.
fn own_tees(a: &Arrival, model: &Model, end: Option<f64>, d: f64) -> Option<Vec<Option<Tee>>> {
    let g = a.profile.as_ref()?.angle?;
    let width = if a.leg_y { g.b } else { g.h };
    let fy = member_fy(&a.member, model);
    let gauges: Vec<f64> = a.bolt_flat.iter().map(|f| f[1]).collect();
    if many_lines(&gauges, d) {
        return None;
    }
    Some(
        a.bolt_flat
            .iter()
            .map(|f| flange_tee(&a.member, g.t, fy, f[1], width, f[0], &a.bolt_flat, end, d))
            .collect(),
    )
}

/// Holes of one ply overlapping: each pair of `holes` (where, its
/// diameter) nearer than their radii together but not one hole, in words.
/// Holes in `part` that overlap, in words, each with whether one of the
/// two lies where it was given (`holes`: where, its diameter, what it is,
/// given; D14).
fn overlaps(part: &str, holes: &[(V3, f64, String, bool)]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    for (i, a) in holes.iter().enumerate() {
        for b in &holes[i + 1..] {
            let dist = norm(sub(a.0, b.0));
            let near = same_hole(a.1.min(b.1));
            if dist > near && dist < (a.1 + b.1) / 2.0 - 1e-4 {
                let w = format!(
                    "holes in {part} overlap ({} and {}: {:.0} mm apart, holes {:.1} and {:.1} \
                     mm)",
                    a.2,
                    b.2,
                    dist * 1000.0,
                    a.1 * 1000.0,
                    b.1 * 1000.0
                );
                match out.iter_mut().find(|x| x.0 == w) {
                    Some(x) => x.1 |= a.3 || b.3,
                    None => out.push((w, a.3 || b.3)),
                }
            }
        }
    }
    out
}

/// A bolt's hole: given, else its diameter (the least it can be).
fn hole_of(b: &Bolt) -> f64 {
    b.hole.unwrap_or(b.diameter)
}

/// Every group of the structure: per joint the plates' holding bolts and
/// the bolts onto each member, with what is not checked.
pub(crate) fn group_plans(
    details: &JointDetails,
    model: &Model,
    library: &SectionLibrary,
    bolts: &BoltLibrary,
) -> GroupPlans {
    let mut out = GroupPlans::default();
    // (L14's question 2) The members' tension capacities by the code's
    // plugin, once (only where some member ending at a joint has ends laid
    // onto it).
    let wanted = details
        .joints
        .iter()
        .any(|j| j.arrivals.iter().any(|a| !riders(j, &a.member).is_empty()));
    let caps: Result<BTreeMap<String, (f64, String)>, String> = if !wanted {
        Ok(BTreeMap::new())
    } else if model.capacity_plugin.is_none() {
        Err("the structure's member check is not a code plugin's".into())
    } else {
        ohl_member_check::capacities(model).map(|v| {
            v.into_iter()
                .filter_map(|g| {
                    g.capacities
                        .iter()
                        .find(|c| c.0 == "tension")
                        .map(|c| (g.id.clone(), (c.1, c.2.clone())))
                })
                .collect()
        })
    };
    for j in &details.joints {
        let first = out.groups.len();
        holding_groups(j, model, library, bolts, &mut out);
        bearing_groups(j, model, bolts, first, &mut out);
        end_bolt_groups(j, model, library, bolts, &caps, &mut out);
    }
    out
}

/// The member ends whose forces pass through `host`'s own end bolts at
/// joint `j` (L14, the owner's answer of 2026-11-22 to L13's question 3).
/// Since L1 (d) and L13 the joint layout lays an end onto a member that
/// **ends** at the joint; its force enters that member near its end and
/// leaves through that member's own end bolts — with the forces of the
/// ends laid onto those ends in turn (a chain, each member once). None
/// for a leg ([`crate::welds::joint_legs`], a member a plate's own bolts
/// onto the leg name, or one whose end bolts hold a plate, `held_by`: its
/// force goes on through its splice or the plate it holds) or a
/// member running on through the joint as one part (`continuous`). An end
/// said one part with another, a modelling member, an end not bolted, and
/// one `host` itself bolts onto (two members lapped onto each other: their
/// one connection) are left out.
pub(crate) fn riders<'a>(j: &'a JointDetail, host: &str) -> Vec<&'a Arrival> {
    let mut seen = vec![host.to_owned()];
    let mut out: Vec<&Arrival> = Vec::new();
    riders_into(j, host, &mut seen, &mut out);
    out
}

fn riders_into<'a>(
    j: &'a JointDetail,
    host: &str,
    seen: &mut Vec<String>,
    out: &mut Vec<&'a Arrival>,
) {
    let Some(h) = j.arrivals.iter().find(|a| a.member == host) else {
        return;
    };
    // (L20) A level plate held by this member and a second one (its
    // `also_bolts`): the plate's force goes on through this member's own
    // end bolts as a rider's does (below).
    let level = |p: &&crate::joints::PlateDetail| {
        p.also_bolts.is_some() && p.holding_ends().any(|l| l.member == host)
    };
    if h.modelling
        || crate::welds::joint_legs(j).iter().any(|l| l.member == host)
        || j.plates.iter().any(|p| {
            !level(&p)
                && (p.leg_bolts.as_deref().is_some_and(|l| l.member == host)
                    || p.held_by.as_deref() == Some(host))
        })
        || run_at(j, host).0.len() > 1
    {
        return;
    }
    for e in j.arrivals.iter().filter(|e| {
        e.onto.value.as_deref() == Some(host)
            && !e.modelling
            && e.continuous.is_none()
            && h.onto.value.as_deref() != Some(e.member.as_str())
            && e.kind
                .value
                .is_none_or(|k| k == ohl_structures::connections::ConnectionKind::Bolted)
    }) {
        if seen.contains(&e.member) {
            continue;
        }
        seen.push(e.member.clone());
        out.push(e);
        riders_into(j, &e.member, seen, out);
    }
    // (L20) The ends bolted onto a level plate it holds (not its other
    // holder): each holder's own end bolts taken carrying their whole force
    // (on the safe side: how the two share it is not counted on).
    for p in j.plates.iter().filter(level) {
        for e in j.arrivals.iter().filter(|e| {
            e.onto.value.as_deref() == Some(p.id.as_str())
                && !e.modelling
                && e.continuous.is_none()
                && !p.holding_ends().any(|l| l.member == e.member)
                && e.kind
                    .value
                    .is_none_or(|k| k == ohl_structures::connections::ConnectionKind::Bolted)
        }) {
            if seen.contains(&e.member) {
                continue;
            }
            seen.push(e.member.clone());
            out.push(e);
            riders_into(j, &e.member, seen, out);
        }
    }
}

/// [`riders`] of `host` as sources: each along its line through its bolts'
/// centroid (the joint where its bolts are not placed).
fn rider_sources(j: &JointDetail, host: &str) -> Vec<Source> {
    riders(j, host)
        .into_iter()
        .map(|e| Source {
            member: e.member.clone(),
            end: e.end,
            direction: e.direction,
            centroid: centroid3(&e.bolt_points, j.position),
            bolts: Vec::new(),
            riders: Vec::new(),
            passed: Vec::new(),
        })
        .collect()
}

/// The ends whose forces a level plate may pass to `host`'s bolts at joint
/// `j` (L21, L20's question 2): where `host` or an end laid onto it
/// ([`riders`]) holds a level plate (`[plate.also_bolts]`), the plate's
/// other holder — its holding end on the plate, its bolts there —, the
/// members running on with it through the joint as one part (each piece's
/// force reaching the plate through those bolts: their sum is what the run
/// gives off at the joint) and the ends laid onto that holder (not the
/// ends on the plate itself: they are `host`'s riders already, or the
/// holding group's sources). Each with the point its force reaches the
/// plate at. The plate joins its two holders: how a holder's force splits
/// between its own end bolts and the plate is not known, so each is taken
/// from none to the whole of it ([`with_passed`]; a statically safe bound,
/// no load path counted on). Empty for every other member.
pub(crate) fn passed<'a>(j: &'a JointDetail, host: &str) -> Vec<(&'a Arrival, V3)> {
    let rid = riders(j, host);
    let mut chain: Vec<&str> = vec![host];
    chain.extend(rid.iter().map(|r| r.member.as_str()));
    passed_beyond(j, &chain)
}

/// [`passed`] for the members `chain` (a group's members or its holders):
/// the other holders of the level plates any of them holds, and the ends
/// laid onto those holders, none of `chain` and none bolted onto the plate.
fn passed_beyond<'a>(j: &'a JointDetail, chain: &[&str]) -> Vec<(&'a Arrival, V3)> {
    let mut out: Vec<(&Arrival, V3)> = Vec::new();
    let has = |out: &Vec<(&Arrival, V3)>, m: &str| {
        chain.contains(&m) || out.iter().any(|a| a.0.member == m)
    };
    for p in j.plates.iter().filter(|p| p.also_bolts.is_some()) {
        if !p.holding_ends().any(|h| chain.contains(&h.member.as_str())) {
            continue;
        }
        let on_plate = |m: &str| {
            j.arrivals
                .iter()
                .any(|e| e.member == m && e.onto.value.as_deref() == Some(p.id.as_str()))
        };
        for h in p.holding_ends() {
            if has(&out, &h.member) {
                continue;
            }
            let at = centroid3(&h.bolt_points, j.position);
            out.push((h, at));
            // A holder running on through the joint as one part: each
            // piece's force through these bolts.
            for m in run_at(j, &h.member).0 {
                if let Some(a) = j.arrivals.iter().find(|a| a.member == m)
                    && !has(&out, &m)
                {
                    out.push((a, at));
                }
            }
            for r in riders(j, &h.member) {
                if !has(&out, &r.member) && !on_plate(&r.member) {
                    out.push((r, centroid3(&r.bolt_points, j.position)));
                }
            }
        }
    }
    out
}

/// [`passed`] of `host` as sources: each along its line through its bolts'
/// centroid (the joint where its bolts are not placed).
fn passed_sources(j: &JointDetail, host: &str) -> Vec<Source> {
    passed(j, host)
        .into_iter()
        .map(|(e, at)| plain_source(e, at))
        .collect()
}

/// An end as a source with nothing laid onto it, its force through `at`.
fn plain_source(e: &Arrival, at: V3) -> Source {
    Source {
        member: e.member.clone(),
        end: e.end,
        direction: e.direction,
        centroid: at,
        bolts: Vec::new(),
        riders: Vec::new(),
        passed: Vec::new(),
    }
}

/// Up to this many forces a level plate may pass, each is taken from none
/// to whole by trying every corner ([`with_passed`], 2ⁿ loadings); beyond,
/// a bolt's forces are found exact by [`widen_exact`] (2026-11-25; before,
/// the rest were taken together, none or all — NOT ON THE SAFE SIDE) and
/// a member's own steel adds each further one's size (a bound). The
/// program's value: a level plate has two holders, the ends laid onto the
/// other one are few.
const PASSED_EACH: usize = 6;

/// The loadings whose larger, bolt by bolt, a group takes (L21): each of
/// `base` alone and with every choice of the forces `passed` (each added
/// or not — from none to the whole of it: a bolt's force is linear in each
/// share, so its largest over every share lies at one of these corners).
/// At most [`PASSED_EACH`] of them (the callers take any more exact, or
/// bounded); a longer list is cut there (never met: the callers do not
/// pass one).
fn with_passed(base: Vec<(V3, V3)>, passed: &[(V3, V3)]) -> Vec<(V3, V3)> {
    debug_assert!(passed.len() <= PASSED_EACH);
    let each: Vec<(V3, V3)> = passed.iter().take(PASSED_EACH).copied().collect();
    let mut v = base;
    for p in each {
        let more: Vec<(V3, V3)> = v.iter().map(|x| (add(x.0, p.0), add(x.1, p.1))).collect();
        v.extend(more);
    }
    v
}

/// The vector of largest size among `base + Σ sᵢ gᵢ` over every share
/// `sᵢ` from 0 to 1 (2026-11-25: beyond [`PASSED_EACH`] forces a level
/// plate may pass, exact instead of the rest taken together). The set of
/// such sums is a zonotope; the size, a convex function, is largest at one
/// of its corners, and each corner is the sum taking every `gᵢ` pointing
/// into one direction `u` (`gᵢ · u > 0`) — the same for all `u` between
/// two of the directions square to some `gᵢ`. So the corners are found by
/// one `u` in each arc between those directions: exact, at most 2 n
/// corners for n shares (no 2ⁿ corners tried).
fn box_max_2d(base: V2, gens: &[V2]) -> V2 {
    let gens: Vec<V2> = gens
        .iter()
        .copied()
        .filter(|g| g[0] != 0.0 || g[1] != 0.0)
        .collect();
    // The directions square to each gᵢ, as angles in [0, 2π).
    let tau = std::f64::consts::TAU;
    let mut cuts: Vec<f64> = gens
        .iter()
        .flat_map(|g| {
            let a = g[1].atan2(g[0]);
            [
                a + std::f64::consts::FRAC_PI_2,
                a - std::f64::consts::FRAC_PI_2,
            ]
        })
        .map(|a| a.rem_euclid(tau))
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let corner = |u: V2| -> V2 {
        let mut v = base;
        for g in &gens {
            if g[0] * u[0] + g[1] * u[1] > 0.0 {
                v = [v[0] + g[0], v[1] + g[1]];
            }
        }
        v
    };
    let mut best = base;
    let size = |v: V2| v[0].hypot(v[1]);
    let n = cuts.len();
    for k in 0..n.max(1) {
        // The middle of the arc from cut k to the next one (round the
        // circle); no cuts: any direction (no gᵢ, the base itself).
        let mid = if n == 0 {
            0.0
        } else {
            let (a, b) = (
                cuts[k],
                if k + 1 < n {
                    cuts[k + 1]
                } else {
                    cuts[0] + tau
                },
            );
            0.5 * (a + b)
        };
        let v = corner([mid.cos(), mid.sin()]);
        if size(v) > size(best) {
            best = v;
        }
    }
    best
}

/// The largest size of `base + Σ sᵢ cᵢ` over every share `sᵢ` from 0 to 1
/// (a number: each share where it adds to that sign), exact.
fn box_max_1d(base: f64, gens: &[f64]) -> f64 {
    let up = base + gens.iter().map(|c| c.max(0.0)).sum::<f64>();
    let down = base + gens.iter().map(|c| c.min(0.0)).sum::<f64>();
    up.abs().max(down.abs())
}

/// Each bolt of `pts` (in `plane`) under the larger, bolt by bolt, over
/// every loading of `base` with every share of every force of `passed`
/// from none to whole (2026-11-25, beyond [`PASSED_EACH`]): the shear
/// force in the plane at its exact largest ([`box_max_2d`]) widening
/// `forces`, its pull out of the plane at its exact largest
/// ([`box_max_1d`]) widening `tension` — a bolt's forces are linear in
/// each share under the elastic sharing.
fn widen_exact(
    plane: &Plane,
    pts: &[V2],
    base: &[(V3, V3)],
    passed: &[(V3, V3)],
    forces: &mut [V2],
    tension: &mut [f64],
) {
    let shear = |(f, m): &(V3, V3)| {
        bg::elastic(
            pts,
            &PlaneLoad {
                force: plane.vector(*f),
                at: bg::centroid(pts),
                moment: dot(*m, plane.n),
            },
        )
        .forces
    };
    let pulls = |(f, m): &(V3, V3)| {
        let out = sub(*m, scale(plane.n, dot(*m, plane.n)));
        bg::tension(pts, dot(*f, plane.n), plane.vector(out)).forces
    };
    let gens: Vec<(Vec<V2>, Vec<f64>)> = passed.iter().map(|p| (shear(p), pulls(p))).collect();
    for b in base {
        let (s0, t0) = (shear(b), pulls(b));
        let mut widest: Vec<V2> = Vec::with_capacity(pts.len());
        for (k, a) in s0.iter().enumerate() {
            let g: Vec<V2> = gens.iter().map(|x| x.0[k]).collect();
            widest.push(box_max_2d(*a, &g));
            let c: Vec<f64> = gens.iter().map(|x| x.1[k]).collect();
            tension[k] = tension[k].max(box_max_1d(t0[k], &c));
        }
        larger(forces, &widest);
    }
}

/// The note of a group whose sources carry ends laid onto them (L14).
fn riders_note(sources: &[Source], part: &str) -> Option<String> {
    let with: Vec<String> = sources
        .iter()
        .filter(|s| !s.riders.is_empty())
        .map(|s| {
            format!(
                "{} with {}",
                s.member,
                s.riders
                    .iter()
                    .map(|r| r.member.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    (!with.is_empty()).then(|| {
        format!(
            "on {part}, the forces of the ends laid onto a member ending at the joint go through \
             that member's own bolts ({}): each along its line through its bolts' centroid, the \
             same case, with its moment about those bolts' centroid, shared elastically — each \
             bolt taken under the larger of that and the member's own force alone (another's \
             force relieving it not counted on: on the safe side; L14)",
            with.join("; ")
        )
    })
}

/// The note of a group whose sources may take a force a level plate passes
/// between its two holders (L21, [`passed`]).
fn passed_note(sources: &[Source], part: &str) -> Option<String> {
    let with: Vec<String> = sources
        .iter()
        .filter(|s| !s.passed.is_empty())
        .map(|s| {
            format!(
                "{} from {}",
                s.member,
                s.passed
                    .iter()
                    .map(|r| r.member.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    (!with.is_empty()).then(|| {
        format!(
            "on {part}, a level plate joins a member's end to another member ({}): the force the \
             plate may pass to that member's bolts taken from none to the whole of each such \
             force (along its line through its bolts' centroid, the same case) — each bolt under \
             the larger, bolt by bolt, of every choice (how the plate's forces split between its \
             holders is not known: a statically safe bound, no load path counted on; L21){}",
            with.join("; "),
            if sources.iter().any(|s| s.passed.len() > PASSED_EACH) {
                format!(
                    "; more than {PASSED_EACH} such forces: each bolt's largest over every share \
                     found exact (the corners of the sums), and the member's own steel takes \
                     each further force's size added (on the safe side)"
                )
            } else {
                String::new()
            }
        )
    })
}

/// The groups of a member's own end bolts with the ends laid onto it
/// (L14): per member ending at joint `j` with [`riders`] and its own bolts
/// placed onto a member or a plate, its bolts under its force and theirs
/// together — each bolt's shear, its bearing on the member itself and
/// (onto a plate) on the plate, their distances to the member's end and
/// edges. The member it bolts onto has its bearing under these bolts in
/// its own group ([`bearing_groups`], the forces followed the same way).
/// What is not covered is said: the member's own steel at those bolts (its
/// net section, its tearing out) and the plate's own checks under them
/// take its own force only — NOT ON THE SAFE SIDE there; a member ending at
/// the joint with ends laid onto it but bolting onto nothing, or its bolts
/// not placed: the path of their forces not checked.
fn end_bolt_groups(
    j: &JointDetail,
    model: &Model,
    library: &SectionLibrary,
    bolt_lib: &BoltLibrary,
    caps: &Result<BTreeMap<String, (f64, String)>, String>,
    out: &mut GroupPlans,
) {
    for h in j.arrivals.iter().filter(|a| !a.modelling) {
        let rid = riders(j, &h.member);
        if rid.is_empty() {
            continue;
        }
        let names: Vec<&str> = rid.iter().map(|r| r.member.as_str()).collect();
        let words = names.join(", ");
        let bolted = h
            .kind
            .value
            .is_none_or(|k| k == ohl_structures::connections::ConnectionKind::Bolted);
        let Some(onto) = h.onto.value.as_deref().filter(|_| bolted) else {
            out.not_checked.push((
                j.id.clone(),
                h.member.clone(),
                format!(
                    "NOT CHECKED, NOT ON THE SAFE SIDE: {words} laid onto {}, which ends at the \
                     joint and {} here: where their forces go is not given, so a failure there \
                     would not be found (give its end bolted onto a part, or say the pieces in \
                     line one part)",
                    h.member,
                    if bolted {
                        "bolts onto nothing"
                    } else {
                        "is not bolted"
                    }
                ),
            ));
            continue;
        };
        let unchecked = |why: &str| {
            (
                j.id.clone(),
                h.member.clone(),
                format!(
                    "NOT CHECKED, NOT ON THE SAFE SIDE: {}'s end bolts with {words} laid onto it \
                     — {why}, so a failure there would not be found until that is given",
                    h.member
                ),
            )
        };
        if h.bolt_points.is_empty() {
            out.not_checked.push(unchecked("its bolts are not placed"));
            continue;
        }
        let Some((t, fy, fu, redundant)) = member_ply(h, model) else {
            out.not_checked
                .push(unchecked("its thickness is not known"));
            continue;
        };
        let plate = j.plates.iter().find(|p| p.id == onto);
        let mut notes = Vec::new();
        let d = h.bolts.value.as_ref().map_or(0.016, |b| b.diameter);
        let end = h.cut.known().then(|| h.cut.value.unwrap_or(0.0));
        let tees = own_tees(h, model, end, d);
        let width = h
            .profile
            .as_ref()
            .and_then(|p| p.angle)
            .map(|g| if h.leg_y { g.b } else { g.h });
        let across = h
            .profile
            .as_ref()
            .map(|p| if h.leg_y { p.axes[1] } else { p.axes[2] });
        let mut bolts: Vec<Bolt> = Vec::new();
        let mut mine: Vec<(usize, V3)> = Vec::new();
        let mut misfits: Vec<String> = Vec::new();
        let mut misfit_given = false;
        let outline = plate.map(|pd| pd.outline.clone()).unwrap_or_default();
        for (i, q) in h.bolt_points.iter().enumerate() {
            let Some(mut b) = bolt_of(h, model, *q, bolt_lib) else {
                continue;
            };
            for r in &names {
                b.carries.push(format!("{r} laid onto it"));
            }
            b.flange = h
                .bolt_flat
                .get(i)
                .zip(width)
                .zip(across)
                .map(|((f, w), c)| (f[1], w, toward_toe(c, h)));
            // Does not fit (as the other groups say it): a hole off its own
            // flange, a bolt beyond its steel's known end, a bolt outside
            // the plate it bolts onto.
            if let Some((g, w, _)) = b.flange
                && off_flange(g, w, hole_of(&b))
            {
                misfit_given |= b.given;
                push_once(
                    &mut misfits,
                    format!(
                        "a bolt of {} {:.0} mm from its heel: its hole ({:.1} mm) is not within \
                         its flange (0 to {:.0} mm)",
                        h.member,
                        g * 1000.0,
                        hole_of(&b) * 1000.0,
                        w * 1000.0
                    ),
                );
            }
            if let Some(cut) = end {
                let s = dot(sub(*q, j.position), h.direction);
                if s < cut - 0.001 {
                    misfit_given |= b.given;
                    push_once(
                        &mut misfits,
                        format!(
                            "a bolt of {} lies beyond the end of its steel at the joint ({:.0} mm \
                             from the work point, the steel from {:.0} mm)",
                            h.member,
                            s * 1000.0,
                            cut * 1000.0
                        ),
                    );
                }
            }
            if outline.len() >= 3
                && crate::plate_checks::ray_to_outline(&outline, *q, h.direction).is_none()
            {
                misfit_given |= b.given;
                push_once(
                    &mut misfits,
                    format!(
                        "a bolt of {} onto plate {onto} lies outside the plate's outline",
                        h.member
                    ),
                );
            }
            match tees.as_ref().map(|t| t.get(i).cloned().flatten()) {
                Some(Some(t)) => b.tees.push(t),
                Some(None) => {}
                None => b.unknown.push(format!(
                    "{}'s flange (not an angle, or bolts on more than two gauge lines)",
                    h.member
                )),
            }
            let k = add_bolt(&mut bolts, b, &mut notes);
            mine.push((k, *q));
        }
        if bolts.is_empty() {
            out.not_checked.push(unchecked("its bolts not known"));
            continue;
        }
        let c = centroid3(&bolts.iter().map(|b| b.at).collect::<Vec<_>>(), j.position);
        let Some(plane) = Plane::new(c, h.bolt_axis, Some(h.direction)) else {
            continue;
        };
        // Every hole in its flange here: its own and those of the ends laid
        // onto it through the same flange.
        let mut holes: Vec<V3> = h.bolt_points.clone();
        for r in &rid {
            if r.onto.value.as_deref() == Some(h.member.as_str())
                && dot(r.bolt_axis, h.bolt_axis).abs() >= 10f64.to_radians().cos()
            {
                holes.extend(r.bolt_points.iter().copied());
            }
        }
        // Holes overlapping in its flange (its own and those of the ends laid
        // onto it there).
        {
            let d0 = bolts.iter().map(hole_of).fold(f64::INFINITY, f64::min);
            let mut list: Vec<(V3, f64, String, bool)> = bolts
                .iter()
                .map(|b| (b.at, hole_of(b), format!("a bolt of {}", h.member), b.given))
                .collect();
            for x in holes.iter().skip(h.bolt_points.len()) {
                if !bolts
                    .iter()
                    .any(|b| norm(sub(b.at, *x)) <= same_hole(b.diameter))
                {
                    list.push((*x, d0, "a bolt laid onto it".into(), true));
                }
            }
            for (w, g) in overlaps(&h.member, &list) {
                misfit_given |= g;
                push_once(&mut misfits, w);
            }
        }
        let mut plies = vec![Ply {
            part: h.member.clone(),
            member: true,
            sheared: toe_sheared(h, model),
            thickness: t,
            fy,
            fu,
            redundant,
            known: h.cut.known() && bolts.iter().all(|b| b.flange.is_some()),
            edges: Edges::Flange {
                dir: h.direction,
                origin: j.position,
                end,
            },
            holes,
            // Its bolts push it against their force on what it bolts onto.
            sign: -1.0,
        }];
        if let Some(pd) = plate {
            let (steel, steel_note) = crate::plate_checks::plate_steel_of(pd, j, model, library);
            notes.extend(steel_note);
            if outline.len() < 3 {
                notes.push(format!(
                    "plate {onto} has no outline: its edges' distances not known"
                ));
            }
            plies.push(Ply {
                part: onto.to_owned(),
                member: false,
                sheared: false,
                thickness: steel.thickness,
                fy: steel.fy,
                fu: steel.fu,
                redundant: false,
                known: outline.len() >= 3,
                edges: Edges::Outline(outline.clone()),
                holes: onto_plate(j, onto)
                    .iter()
                    .flat_map(|a| a.bolt_points.clone())
                    .collect(),
                sign: 1.0,
            });
        }
        if !h.cut.known() {
            notes.push(format!(
                "{} ends at {}: where its steel stops is not given (taken at the work point; the \
                 code's conservative case)",
                h.member, j.id
            ));
        }
        notes.push(format!(
            "{m}'s own end bolts onto {onto} carry its force and those of {words} laid onto it \
             near its end (each along its line through its bolts' centroid, the same case) with \
             their moment about these bolts' centroid, shared elastically — each bolt under the \
             larger of that and {m}'s own force alone (on the safe side; L14). Checked here: \
             each bolt's shear, its bearing on {m}{p} and the distances along and square to its \
             force; {onto}'s bearing under them {w}",
            m = h.member,
            p = if plate.is_some() {
                format!(" and on plate {onto}")
            } else {
                String::new()
            },
            w = if plate.is_some() {
                "here"
            } else {
                "in its own group"
            }
        ));
        let own = Source {
            member: h.member.clone(),
            end: h.end,
            direction: h.direction,
            centroid: centroid3(&h.bolt_points, j.position),
            bolts: mine,
            riders: rider_sources(j, &h.member),
            passed: passed_sources(j, &h.member),
        };
        if let Some(w) = passed_note(std::slice::from_ref(&own), &h.member) {
            notes.push(w);
        }
        notes.extend(grade_notes(&bolts));
        out.groups.push(GroupPlan {
            joint: j.id.clone(),
            holding: false,
            part: h.member.clone(),
            holders: String::new(),
            position: j.position,
            plane,
            bolts,
            plies,
            sources: vec![own],
            from: Vec::new(),
            notes,
            misfits,
            misfit_given,
            ends_of: Some(words.clone()),
            passed: Vec::new(),
        });
        // (L14's question 2, the owner 2026-11-22) The member's own steel
        // at those bolts under the force followed through: its tension
        // capacity (the code's plugin's, the least over its ends of its net
        // section, block shear and bolts) against the larger of its own
        // force and the resultant; the plate's own checks under those bolts
        // take that force too (`plate_checks`).
        match caps.as_ref().map(|c| c.get(&h.member)) {
            Ok(Some((cap, governs))) if *cap > 0.0 => out.end_steel.push(EndSteel {
                joint: j.id.clone(),
                member: h.member.clone(),
                direction: h.direction,
                riders: rid
                    .iter()
                    .map(|r| (r.member.clone(), r.direction))
                    .collect(),
                passed: passed(j, &h.member)
                    .iter()
                    .map(|(r, _)| (r.member.clone(), r.direction))
                    .collect(),
                capacity: *cap,
                governs: governs.clone(),
            }),
            other => out.not_checked.push((
                j.id.clone(),
                h.member.clone(),
                format!(
                    "NOT CHECKED, NOT ON THE SAFE SIDE: {}'s own steel at its end bolts (its net \
                     section, its block shear) under the forces of {words} laid onto it near \
                     its end — {}, so a failure there would not be found",
                    h.member,
                    match other {
                        Err(e) => format!("no tension capacity: {e}"),
                        _ => "the code's plugin gives it no tension capacity".to_owned(),
                    }
                ),
            )),
        }
    }
}

/// The member ends bolted onto plate `q` at joint `j`: those of
/// [`crate::joints::bolted_onto`], then the plate's own bolts onto the
/// leg (`[plate.leg_bolts]`, 2026-11-19) as that leg's end bolted onto it.
fn onto_plate<'a>(j: &'a JointDetail, q: &str) -> Vec<Cow<'a, Arrival>> {
    let mut v = crate::joints::bolted_onto(&j.arrivals, q);
    if let Some(p) = j.plates.iter().find(|p| p.id == q) {
        v.extend(p.holding_ends().map(Cow::Borrowed));
    }
    v
}

/// The holding groups of joint `j` (module docs).
fn holding_groups(
    j: &JointDetail,
    model: &Model,
    library: &SectionLibrary,
    bolt_lib: &BoltLibrary,
    out: &mut GroupPlans,
) {
    let mut legs: Vec<String> = crate::welds::joint_legs(j)
        .iter()
        .map(|a| a.member.clone())
        .collect();
    // The members a plate's own bolts onto the leg name hold it too; and
    // (L14) the member whose own end bolts onto a plate are said to hold it
    // (`held_by`: where nothing runs through the joint, the base member).
    for p in &j.plates {
        for a in p.holding_ends() {
            if !legs.contains(&a.member) {
                legs.push(a.member.clone());
            }
        }
        if let Some(m) = p.held_by.as_deref()
            && !legs.iter().any(|l| l == m)
            && j.arrivals
                .iter()
                .any(|a| a.member == m && a.onto.value.as_deref() == Some(p.id.as_str()))
        {
            legs.push(m.to_owned());
        }
    }
    let mut done: Vec<Vec<String>> = Vec::new();
    for p in &j.plates {
        let mut asm: Vec<String> = if p.assembly.is_empty() {
            vec![p.id.clone()]
        } else {
            p.assembly.clone()
        };
        asm.sort();
        if done.contains(&asm) {
            continue;
        }
        done.push(asm.clone());
        let words = asm.join("+");
        let on: Vec<(String, Cow<Arrival>)> = asm
            .iter()
            .flat_map(|q| onto_plate(j, q).into_iter().map(move |a| (q.clone(), a)))
            .filter(|(_, a)| !a.modelling)
            .collect();
        let held = |a: &Arrival| legs.contains(&a.member);
        // (L20) A level plate held by two members (`also_bolts`): neither
        // holder's force taken as a force on it — each holder's bolts carry
        // the other ends' forces whole (on the safe side for the bolts; the
        // force the plate may pass between its two holders is not counted:
        // said).
        let level = asm.iter().any(|q| {
            j.plates
                .iter()
                .any(|p| &p.id == q && p.also_bolts.is_some())
        });
        let other_end = |run: &[String], a: &Arrival| {
            if level {
                !held(a)
            } else {
                !run.contains(&a.member)
            }
        };
        if on.iter().all(|(_, a)| held(a)) {
            continue;
        }
        if !on.iter().any(|(_, a)| held(a)) {
            if !legs.is_empty() {
                out.not_checked.push((
                    j.id.clone(),
                    words.clone(),
                    format!(
                        "NOT CHECKED, NOT ON THE SAFE SIDE: how plate {words} is held to the \
                         joint's leg ({}) is not given — no bolts of the leg onto it (a leg \
                         running on through the joint: give the plate's own bolts onto it, \
                         [plate.leg_bolts]): the bolts holding it and the leg's bearing under \
                         them, so a failure there would not be found",
                        legs.join(", ")
                    ),
                ));
            }
            continue;
        }
        // The holding legs, by the run each belongs to, by the plate
        // their bolts are on.
        let mut runs: Vec<(Vec<String>, Vec<String>)> = Vec::new();
        for (_, a) in on.iter().filter(|(_, a)| held(a)) {
            let r = run_at(j, &a.member);
            if !runs.iter().any(|x| x.0 == r.0) {
                runs.push(r);
            }
        }
        for (run, unsaid) in runs {
            let mut plates: Vec<String> = Vec::new();
            for (q, a) in &on {
                if run.contains(&a.member) && !plates.contains(q) {
                    plates.push(q.clone());
                }
            }
            for q in &plates {
                let holders: Vec<&Cow<Arrival>> = on
                    .iter()
                    .filter(|(x, a)| x == q && run.contains(&a.member))
                    .map(|(_, a)| a)
                    .collect();
                let mut names: Vec<String> = Vec::new();
                for a in &holders {
                    if !names.contains(&a.member) {
                        names.push(a.member.clone());
                    }
                }
                let hw = names.join("/");
                let points: Vec<(usize, V3)> = holders
                    .iter()
                    .enumerate()
                    .flat_map(|(k, a)| a.bolt_points.iter().map(move |x| (k, *x)))
                    .collect();
                if points.is_empty() {
                    out.not_checked.push((
                        j.id.clone(),
                        q.clone(),
                        format!(
                            "NOT CHECKED, NOT ON THE SAFE SIDE: plate {q}'s bolts onto {hw} — \
                             their bolts are not placed, so a failure there would not be found; \
                             place them"
                        ),
                    ));
                    continue;
                }
                let pd = j.plates.iter().find(|x| &x.id == q);
                let Some(pd) = pd else {
                    continue;
                };
                let mut notes = Vec::new();
                let mut misfits = Vec::new();
                let mut misfit_given = false;
                let outline = pd.outline.clone();
                let mut bolts: Vec<Bolt> = Vec::new();
                let (steel, steel_note) =
                    crate::plate_checks::plate_steel_of(pd, j, model, library);
                // The bolts of the members pulling the plate (2026-11-19:
                // the plate's own prying).
                let pulls: Vec<V3> = on
                    .iter()
                    .filter(|(_, a)| other_end(&run, a))
                    .flat_map(|(_, a)| a.bolt_points.iter().copied())
                    .collect();
                let holding: Vec<V3> = holders
                    .iter()
                    .flat_map(|a| a.bolt_points.iter().copied())
                    .collect();
                for a in &holders {
                    // The holder's own flange: its tee for each bolt (the
                    // leg ending at the plate: its steel stops at its cut).
                    let d = a.bolts.value.as_ref().map_or(0.016, |b| b.diameter);
                    let end = (run.len() == 1 && a.cut.known()).then(|| a.cut.value.unwrap_or(0.0));
                    let tees = own_tees(a, model, end, d);
                    for (i, at) in a.bolt_points.iter().enumerate() {
                        let Some(mut b) = bolt_of(a, model, *at, bolt_lib) else {
                            continue;
                        };
                        b.carries = vec![format!("plate {words}")];
                        match tees.as_ref().map(|t| t.get(i).cloned().flatten()) {
                            Some(Some(t)) => b.tees.push(t),
                            Some(None) => {}
                            None => b.unknown.push(format!(
                                "{}'s flange (not an angle, or bolts on more than two gauge \
                                 lines)",
                                a.member
                            )),
                        }
                        // The plate's own prying (2026-11-19): a flange
                        // without a web, where its geometry is known.
                        let flange = a.profile.as_ref().and_then(|p| {
                            let g = p.angle?;
                            let w = if a.leg_y { g.b } else { g.h };
                            let c = if a.leg_y { p.axes[1] } else { p.axes[2] };
                            Some((a.bolt_flat.get(i)?[1], w, toward_toe(c, a)))
                        });
                        match plate_tee(
                            q,
                            steel.thickness,
                            steel.fy,
                            *at,
                            a.bolt_axis,
                            a.direction,
                            &pulls,
                            &holding,
                            flange,
                            &outline,
                            b.diameter,
                        ) {
                            Some(t) => b.plates.push(t),
                            None => b.unknown.push(format!("plate {q}")),
                        }
                        if outline.len() >= 3
                            && crate::plate_checks::ray_to_outline(&outline, *at, a.direction)
                                .is_none()
                        {
                            misfit_given |= b.given;
                            push_once(
                                &mut misfits,
                                format!(
                                    "a bolt of {} onto plate {q} lies outside the plate's outline",
                                    a.member
                                ),
                            );
                        }
                        add_bolt(&mut bolts, b, &mut notes);
                    }
                }
                if bolts.is_empty() {
                    out.not_checked.push((
                        j.id.clone(),
                        q.clone(),
                        format!(
                            "NOT CHECKED, NOT ON THE SAFE SIDE: plate {q}'s bolts onto {hw} — \
                             their bolts not known, so a failure there would not be found; give \
                             them"
                        ),
                    ));
                    continue;
                }
                let c = centroid3(&bolts.iter().map(|b| b.at).collect::<Vec<_>>(), j.position);
                let Some(plane) = Plane::new(c, holders[0].bolt_axis, Some(holders[0].direction))
                else {
                    continue;
                };
                let sources: Vec<Source> = on
                    .iter()
                    .filter(|(_, a)| other_end(&run, a))
                    .map(|(_, a)| {
                        if a.bolt_points.is_empty() {
                            push_once(
                                &mut notes,
                                format!(
                                    "{}: its bolts on the plate not placed — its force taken \
                                     through the joint",
                                    a.member
                                ),
                            );
                        }
                        Source {
                            member: a.member.clone(),
                            end: a.end,
                            direction: a.direction,
                            centroid: centroid3(&a.bolt_points, j.position),
                            bolts: Vec::new(),
                            riders: rider_sources(j, &a.member),
                            passed: passed_sources(j, &a.member),
                        }
                    })
                    .collect();
                if let Some(w) = riders_note(&sources, &format!("plate {words}")) {
                    notes.push(w);
                }
                if let Some(w) = passed_note(&sources, &format!("plate {words}")) {
                    notes.push(w);
                }
                notes.extend(steel_note);
                let holes: Vec<V3> = onto_plate(j, q)
                    .iter()
                    .flat_map(|a| a.bolt_points.clone())
                    .collect();
                // Holes overlapping in the plate: its holding bolts against
                // every hole in it.
                let d0 = bolts.iter().map(hole_of).fold(f64::INFINITY, f64::min);
                let mut list: Vec<(V3, f64, String, bool)> = bolts
                    .iter()
                    .map(|b| (b.at, hole_of(b), format!("a bolt of {hw}"), b.given))
                    .collect();
                // The other members' holes in the plate: given with the
                // plate's members (their ends onto it in the file).
                let given_here: Vec<(V3, bool)> = onto_plate(j, q)
                    .iter()
                    .flat_map(|a| {
                        let g = placed_given(a);
                        a.bolt_points.iter().map(move |x| (*x, g))
                    })
                    .collect();
                for h in &holes {
                    if !bolts
                        .iter()
                        .any(|b| norm(sub(b.at, *h)) <= same_hole(b.diameter))
                    {
                        let g = given_here
                            .iter()
                            .find(|x| norm(sub(x.0, *h)) < 1e-9)
                            .is_none_or(|x| x.1);
                        list.push((*h, d0, "another member's bolt".into(), g));
                    }
                }
                for (w, g) in overlaps(&format!("plate {q}"), &list) {
                    misfit_given |= g;
                    push_once(&mut misfits, w);
                }
                notes.push(format!(
                    "plate {words}'s bolts onto {hw}: the resultant of the other members' forces \
                     on the plate (each along its line through its bolts' centroid, the same \
                     case) with its moment about these bolts' centroid, shared elastically \
                     (Kulak, Fisher & Struik; on the safe side)"
                ));
                if let Some(l) = pd.leg_bolts.as_deref()
                    && run.contains(&l.member)
                {
                    notes.push(format!(
                        "plate {q}'s own bolts onto {} ([plate.leg_bolts]) among its holding bolts",
                        l.member
                    ));
                }
                // (L21) The force a level plate may pass to these bolts
                // from its other holder: that holder and the ends laid onto
                // it, none of this run's, the sources' or their riders'.
                let passed_here: Vec<Source> = if level {
                    let mut chain: Vec<&str> = run.iter().map(String::as_str).collect();
                    for s in &sources {
                        chain.push(&s.member);
                        chain.extend(s.riders.iter().map(|r| r.member.as_str()));
                    }
                    passed_beyond(j, &chain)
                        .into_iter()
                        .map(|(e, at)| plain_source(e, at))
                        .collect()
                } else {
                    Vec::new()
                };
                if let Some(l) = pd.also_bolts.as_deref() {
                    let other: Vec<&str> = pd
                        .holding_ends()
                        .map(|x| x.member.as_str())
                        .filter(|m| !run.iter().any(|r| r == m))
                        .collect();
                    if run.contains(&l.member) {
                        notes.push(format!(
                            "plate {q}'s own bolts onto {} ([plate.also_bolts]) among its holding \
                             bolts",
                            l.member
                        ));
                    }
                    let words_passed: Vec<&str> =
                        passed_here.iter().map(|x| x.member.as_str()).collect();
                    notes.push(format!(
                        "plate {q} is a level plate held by two members ({}): these bolts taken \
                         under the larger, bolt by bolt, of every other end's force on it whole \
                         and none, each with and without the force the plate may pass to them \
                         from {} — each from none to its whole force (how the plate's forces \
                         split between its holders is not known: a statically safe bound, no \
                         load path counted on; L21)",
                        pd.holding_ends()
                            .map(|x| x.member.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        if words_passed.is_empty() {
                            if other.is_empty() {
                                "none".to_owned()
                            } else {
                                other.join(", ")
                            }
                        } else {
                            words_passed.join(", ")
                        }
                    ));
                }
                if passed_here.len() > PASSED_EACH {
                    notes.push(format!(
                        "more than {PASSED_EACH} forces the plate may pass to these bolts: each \
                         bolt's largest over every share of them found exact (the corners of the \
                         sums, not 2ⁿ loadings tried)"
                    ));
                }
                if plates.len() > 1 {
                    notes.push(format!(
                        "{hw} bolts onto several plates of {words}: each plate's bolts taken \
                         carrying the whole (on the safe side)"
                    ));
                }
                if let Some(w) = ends_note(&run, &unsaid, &j.id) {
                    notes.push(w);
                }
                if outline.len() < 3 {
                    notes.push(format!(
                        "plate {q} has no outline: its edges' distances not known"
                    ));
                }
                notes.extend(grade_notes(&bolts));
                out.groups.push(GroupPlan {
                    joint: j.id.clone(),
                    holding: true,
                    part: q.clone(),
                    holders: hw,
                    position: j.position,
                    plane,
                    bolts,
                    plies: vec![Ply {
                        part: q.clone(),
                        member: false,
                        sheared: false,
                        thickness: steel.thickness,
                        fy: steel.fy,
                        fu: steel.fu,
                        redundant: false,
                        known: outline.len() >= 3,
                        edges: Edges::Outline(outline),
                        holes,
                        sign: -1.0,
                    }],
                    sources,
                    from: Vec::new(),
                    notes,
                    misfits,
                    misfit_given,
                    ends_of: None,
                    passed: passed_here,
                });
            }
        }
    }
}

/// The bearing groups of joint `j`: per member others bolt onto (with the
/// members running on in line with it) and per flange, every bolt
/// through it — the lapped members' and the holding groups' from `first`
/// on (this joint's).
fn bearing_groups(
    j: &JointDetail,
    model: &Model,
    bolt_lib: &BoltLibrary,
    first: usize,
    out: &mut GroupPlans,
) {
    // (run, the arrival whose bolt it is, the host arrival, holding group).
    struct Entry<'a> {
        run: Vec<String>,
        unsaid: Vec<String>,
        a: &'a Arrival,
        host: &'a Arrival,
        holding: Option<usize>,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for a in j
        .arrivals
        .iter()
        .filter(|a| !a.modelling && !a.bolt_points.is_empty())
    {
        let Some(h) = a.onto.value.as_deref() else {
            continue;
        };
        let Some(host) = j.arrivals.iter().find(|x| x.member == h) else {
            continue;
        };
        if a.kind
            .value
            .is_some_and(|k| k != ohl_structures::connections::ConnectionKind::Bolted)
        {
            continue;
        }
        let (run, unsaid) = run_at(j, h);
        entries.push(Entry {
            run,
            unsaid,
            a,
            host,
            holding: None,
        });
    }
    // The holding bolts: in the holding leg's flange (a leg piece's end
    // bolted onto the plate, or the plate's own bolts onto the leg).
    let leg_ends: Vec<&Arrival> = j
        .arrivals
        .iter()
        .chain(j.plates.iter().flat_map(|p| p.holding_ends()))
        .collect();
    for (k, g) in out.groups.iter().enumerate().skip(first) {
        if !g.holding {
            continue;
        }
        for a in leg_ends.iter().copied().filter(|a| {
            g.holders.split('/').any(|h| h == a.member)
                && (a.onto.value.as_deref() == Some(g.part.as_str())
                    || a.other_onto.as_deref() == Some(g.part.as_str()))
        }) {
            let (run, unsaid) = run_at(j, &a.member);
            entries.push(Entry {
                run,
                unsaid,
                a,
                host: a,
                holding: Some(k),
            });
        }
    }
    // By run, then by flange (the bolts' axes within 10°).
    let mut keys: Vec<Vec<String>> = Vec::new();
    for e in &entries {
        if !keys.contains(&e.run) {
            keys.push(e.run.clone());
        }
    }
    for run in keys {
        let here: Vec<&Entry> = entries.iter().filter(|e| e.run == run).collect();
        let mut flanges: Vec<(V3, Vec<&Entry>)> = Vec::new();
        for e in here {
            let axis = if e.holding.is_some() && e.a.other_onto.is_some() {
                e.a.other_axis
            } else {
                e.a.bolt_axis
            };
            match flanges
                .iter_mut()
                .find(|f| dot(f.0, axis).abs() >= 10f64.to_radians().cos())
            {
                Some(f) => f.1.push(e),
                None => flanges.push((axis, vec![e])),
            }
        }
        for (axis, list) in flanges {
            let host = list[0].host;
            let part = run.join("/");
            let mut notes = Vec::new();
            let mut misfits = Vec::new();
            let mut misfit_given = false;
            let Some((t, fy, fu, redundant)) = member_ply(host, model) else {
                out.not_checked.push((
                    j.id.clone(),
                    part.clone(),
                    format!(
                        "NOT CHECKED, NOT ON THE SAFE SIDE: the bolts onto {part} — its thickness \
                         is not known, so a failure there would not be found; give its thickness"
                    ),
                ));
                continue;
            };
            // Where the host's steel stops at the joint (none: it runs on);
            // only where given or worked out (not known: the plugin takes
            // its code's conservative case, the ply's distances said not
            // known).
            let ends_here = run.len() == 1;
            let end = (ends_here && host.cut.known()).then(|| host.cut.value.unwrap_or(0.0));
            let mut bolts: Vec<Bolt> = Vec::new();
            let mut sources: Vec<Source> = Vec::new();
            let mut from: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
            for e in &list {
                match e.holding {
                    Some(k) => {
                        let g = out.groups[k].clone();
                        let mut map = Vec::new();
                        let width =
                            e.a.profile
                                .as_ref()
                                .and_then(|p| p.angle)
                                .map(|g| if e.a.leg_y { g.b } else { g.h });
                        let across =
                            e.a.profile
                                .as_ref()
                                .map(|p| if e.a.leg_y { p.axes[1] } else { p.axes[2] });
                        for (n, b) in g.bolts.iter().enumerate() {
                            let mut b = b.clone();
                            b.carries = vec![format!("plate {}", g.part)];
                            b.planes = 1;
                            // Its gauge: the holding leg's own layout.
                            b.flange =
                                e.a.bolt_points
                                    .iter()
                                    .position(|q| norm(sub(*q, b.at)) < 1e-6)
                                    .and_then(|i| e.a.bolt_flat.get(i))
                                    .zip(width)
                                    .zip(across)
                                    .map(|((f, w), c)| (f[1], w, toward_toe(c, e.a)));
                            if let Some((gg, w, _)) = b.flange
                                && off_flange(gg, w, hole_of(&b))
                            {
                                misfit_given |= b.given;
                                push_once(
                                    &mut misfits,
                                    format!(
                                        "a bolt of plate {} {:.0} mm from {part}'s heel: its \
                                         hole ({:.1} mm) is not within the flange (0 to {:.0} \
                                         mm)",
                                        g.part,
                                        gg * 1000.0,
                                        hole_of(&b) * 1000.0,
                                        w * 1000.0
                                    ),
                                );
                            }
                            let ours = add_bolt(&mut bolts, b, &mut notes);
                            map.push((n, ours));
                        }
                        from.push((k, map));
                    }
                    None => {
                        let flange = crate::joints::host_flange(e.a, e.host);
                        let d = e.a.bolts.value.as_ref().map_or(0.016, |b| b.diameter);
                        // The lapped member's own flange, its steel
                        // stopping at its cut (where known).
                        let tees = own_tees(
                            e.a,
                            model,
                            e.a.cut.known().then(|| e.a.cut.value.unwrap_or(0.0)),
                            d,
                        );
                        let mut mine = Vec::new();
                        for (i, q) in e.a.bolt_points.iter().enumerate() {
                            let Some(mut b) = bolt_of(e.a, model, *q, bolt_lib) else {
                                continue;
                            };
                            b.flange = flange.map(|f| {
                                let rel = sub(sub(*q, j.position), f.heel);
                                (dot(rel, f.across), f.width, f.across)
                            });
                            if let Some((g, w, _)) = b.flange
                                && off_flange(g, w, hole_of(&b))
                            {
                                misfit_given |= b.given;
                                push_once(
                                    &mut misfits,
                                    format!(
                                        "{}'s bolt {:.0} mm from {part}'s heel: its hole ({:.1} \
                                         mm) is not within the flange (0 to {:.0} mm)",
                                        e.a.member,
                                        g * 1000.0,
                                        hole_of(&b) * 1000.0,
                                        w * 1000.0
                                    ),
                                );
                            }
                            match tees.as_ref().map(|t| t.get(i).cloned().flatten()) {
                                Some(Some(t)) => b.tees.push(t),
                                Some(None) => {}
                                None => b.unknown.push(format!(
                                    "{}'s flange (not an angle, or bolts on more than two gauge \
                                     lines)",
                                    e.a.member
                                )),
                            }
                            let k = add_bolt(&mut bolts, b, &mut notes);
                            mine.push((k, *q));
                        }
                        if flange.is_none() {
                            push_once(
                                &mut notes,
                                format!(
                                    "{}'s flange under {} not found (not an angle): its edges' \
                                     distances not known",
                                    part, e.a.member
                                ),
                            );
                        }
                        sources.push(Source {
                            member: e.a.member.clone(),
                            end: e.a.end,
                            direction: e.a.direction,
                            centroid: centroid3(&e.a.bolt_points, j.position),
                            bolts: mine,
                            riders: rider_sources(j, &e.a.member),
                            passed: passed_sources(j, &e.a.member),
                        });
                    }
                }
            }
            if bolts.is_empty() {
                continue;
            }
            // The host's flange each bolt bends as it pulls: along the
            // host's line, every bolt of the group in it.
            let along: Vec<f64> = bolts
                .iter()
                .map(|b| dot(sub(b.at, j.position), host.direction))
                .collect();
            let gauges: Vec<f64> = bolts.iter().filter_map(|b| b.flange.map(|f| f.0)).collect();
            let d_min = bolts
                .iter()
                .map(|b| b.diameter)
                .fold(f64::INFINITY, f64::min);
            let lines = many_lines(&gauges, d_min);
            let placed: Vec<[f64; 2]> = bolts
                .iter()
                .zip(&along)
                .map(|(b, s)| [*s, b.flange.map_or(f64::NAN, |f| f.0)])
                .collect();
            for (k, b) in bolts.iter_mut().enumerate() {
                let tee = b.flange.and_then(|(g, w, _)| {
                    flange_tee(&part, t, fy, g, w, along[k], &placed, end, b.diameter)
                });
                match tee {
                    Some(x) if !lines => put_tee(&mut b.tees, x),
                    _ if b.flange.is_some_and(|(g, w, _)| outside(g, w)) => {}
                    _ => push_once(
                        &mut b.unknown,
                        format!(
                            "{part}'s flange (not found, or bolts on more than two gauge lines)"
                        ),
                    ),
                }
            }
            // Holes overlapping in the host's flange.
            let in_flange: Vec<(V3, f64, String, bool)> = bolts
                .iter()
                .map(|b| (b.at, hole_of(b), b.carries.join("+"), b.given))
                .collect();
            for (w, g) in overlaps(&part, &in_flange) {
                misfit_given |= g;
                push_once(&mut misfits, w);
            }
            let shared = bolts.iter().filter(|b| b.carries.len() > 1).count();
            if shared > 0 {
                notes.push(format!(
                    "{shared} bolt(s) carry the forces of several parts: summed as vectors, \
                     the same case, on one shear plane (on the safe side)"
                ));
            }
            notes.push(format!(
                "the bolts onto {part}: each member's force shared equally by its own bolts \
                 (ASCE 10-97 4.1; a frame member's end moment elastically), summed bolt by bolt"
            ));
            if let Some(w) = riders_note(&sources, &part) {
                notes.push(w);
            }
            if let Some(w) = passed_note(&sources, &part) {
                notes.push(w);
            }
            let unsaid: Vec<String> =
                list.iter()
                    .flat_map(|e| e.unsaid.iter().cloned())
                    .fold(Vec::new(), |mut v, x| {
                        if !v.contains(&x) {
                            v.push(x);
                        }
                        v
                    });
            if let Some(w) = ends_note(&run, &unsaid, &j.id) {
                notes.push(w);
            }
            // A bolt beyond the host's steel where it is known to stop:
            // it does not fit.
            if let Some(cut) = end {
                for b in &bolts {
                    let s = dot(sub(b.at, j.position), host.direction);
                    if s < cut - 0.001 {
                        misfit_given |= b.given;
                        push_once(
                            &mut misfits,
                            format!(
                                "a bolt of {} lies beyond the end of {part}'s steel at the joint \
                                 ({:.0} mm from the work point, the steel from {:.0} mm)",
                                b.carries.join("+"),
                                s * 1000.0,
                                cut * 1000.0
                            ),
                        );
                    }
                }
            }
            let known =
                (!ends_here || host.cut.known()) && bolts.iter().all(|b| b.flange.is_some());
            if ends_here && !host.cut.known() {
                notes.push(format!(
                    "{part} ends at {}: where its steel stops is not given (taken at the work \
                     point; the code's conservative case)",
                    j.id
                ));
            }
            let c = centroid3(&bolts.iter().map(|b| b.at).collect::<Vec<_>>(), j.position);
            let Some(plane) = Plane::new(c, axis, Some(host.direction)) else {
                continue;
            };
            let holes = bolts.iter().map(|b| b.at).collect();
            notes.extend(grade_notes(&bolts));
            out.groups.push(GroupPlan {
                joint: j.id.clone(),
                holding: false,
                part: part.clone(),
                holders: String::new(),
                position: j.position,
                plane,
                bolts,
                plies: vec![Ply {
                    part,
                    member: true,
                    sheared: toe_sheared(host, model),
                    thickness: t,
                    fy,
                    fu,
                    redundant,
                    known,
                    edges: Edges::Flange {
                        dir: host.direction,
                        origin: j.position,
                        end,
                    },
                    holes,
                    sign: 1.0,
                }],
                sources,
                from,
                notes,
                misfits,
                misfit_given,
                ends_of: None,
                passed: Vec::new(),
            });
        }
    }
}

/// `across` (an axis of the angle's leg bolted) pointing from its heel to
/// its toe: the angle's heel lies at the legs' far side from the toe.
fn toward_toe(across: V3, a: &Arrival) -> V3 {
    let Some(p) = a.profile.as_ref() else {
        return across;
    };
    let Some(g) = p.angle else {
        return across;
    };
    // The heel's coordinate along this axis; the toe lies the other way
    // from the centroid.
    let heel = if a.leg_y { g.heel[0] } else { g.heel[1] };
    if heel > 0.0 {
        scale(across, -1.0)
    } else {
        across
    }
}

/// A member end's force and moment on the part in one case: the end
/// force from the analysis (on the member, reversed), its moment taken to
/// its bolts' centroid without the axial force's arm (each member's bolts
/// share its axial force equally) and with the levers laid into the
/// group's `plane` (the plies' offsets through their thickness left out,
/// as every lapped connection's checks take them); without end forces, the
/// axial force along it.
fn source_load(
    s: &Source,
    plane: &Plane,
    position: V3,
    axial: &BTreeMap<String, f64>,
    ends: Option<&EndForces>,
) -> (V3, V3) {
    let n = axial.get(&s.member).copied().unwrap_or(0.0);
    match ends.and_then(|e| e.get(&s.member)) {
        Some(ef) => {
            let e = ef[s.end.min(1)];
            let f = [-e[0], -e[1], -e[2]];
            let m = [-e[3], -e[4], -e[5]];
            let v = sub(f, scale(s.direction, dot(f, s.direction)));
            (f, add(m, cross(in_plane(plane, position, s.centroid), v)))
        }
        None => (scale(s.direction, n), [0.0; 3]),
    }
}

/// The forces of the ends laid onto source `s`'s member (L14, its
/// `riders`) on the group, and their moment about `about`: each its end
/// force ([`source_load`]) along its line through its own bolts'
/// centroid, the levers laid into `plane`.
fn riders_load(
    s: &Source,
    plane: &Plane,
    position: V3,
    about: V3,
    axial: &BTreeMap<String, f64>,
    ends: Option<&EndForces>,
) -> (V3, V3) {
    let mut f3 = [0.0; 3];
    let mut m3 = [0.0; 3];
    for r in &s.riders {
        let (f, m) = source_load(r, plane, position, axial, ends);
        f3 = add(f3, f);
        m3 = add(m3, add(m, cross(in_plane(plane, r.centroid, about), f)));
    }
    (f3, m3)
}

/// Each bolt's force in `forces` replaced by the one in `other` where that
/// is larger (the larger of two loadings, bolt by bolt: L14).
fn larger(forces: &mut [V2], other: &[V2]) {
    for (x, y) in forces.iter_mut().zip(other) {
        if y[0].hypot(y[1]) > x[0].hypot(x[1]) {
            *x = *y;
        }
    }
}

/// From `b` to `a` laid into `plane` (structure axes).
fn in_plane(plane: &Plane, a: V3, b: V3) -> V3 {
    let (pa, pb) = (plane.point(a), plane.point(b));
    plane.to_3d([pa[0] - pb[0], pa[1] - pb[1]])
}

/// A bolt `g` from a flange's heel lies outside its width `w` (by more
/// than a millimetre either way).
fn outside(g: f64, w: f64) -> bool {
    !(-0.001..=w + 0.001).contains(&g)
}

/// A bolt's hole of diameter `h` at `g` from a flange's heel is not
/// within its width `w` (by more than a millimetre either way): the bolt
/// does not fit as given (owner 2026-11-19).
fn off_flange(g: f64, w: f64, h: f64) -> bool {
    g - h / 2.0 < -0.001 || g + h / 2.0 > w + 0.001
}

/// The lever a moment on bolts is compared with: the group's largest
/// radius, or three bolt diameters for one bolt (a pin's), m.
fn lever(points: &[V2], d: f64) -> f64 {
    let c = bg::centroid(points);
    points
        .iter()
        .map(|p| (p[0] - c[0]).hypot(p[1] - c[1]))
        .fold(0.0, f64::max)
        .max(3.0 * d)
}

/// One group's forces in one case: each bolt's force in the plies' plane
/// on the part it bolts onto (N), its tension (N, ≥ 0), what is not on the
/// safe side, and whether a bolt's tension is 1 % or more of the force in
/// the plane (the words said when the plugin does not check tension).
#[derive(Debug, Clone, PartialEq, Default)]
struct CaseForces {
    forces: Vec<V2>,
    tension: Vec<f64>,
    said: Vec<String>,
    pulled: Option<String>,
}

/// The tension of bolts at `pts` (the group's plane) from a force `f` and
/// a moment `m` (structure axes, about the bolts' centroid): the part
/// square to the plane and the moment out of it, shared elastically
/// ([`bg::tension`]); each bolt's share taken as pulling whichever side
/// its plies lie on (their sides not used: on the safe side). With the
/// moment the bolts cannot carry in tension (N·m).
fn pull(plane: &Plane, pts: &[V2], f: V3, m: V3) -> (Vec<f64>, f64) {
    let n = dot(f, plane.n);
    let out = sub(m, scale(plane.n, dot(m, plane.n)));
    let t = bg::tension(pts, n, plane.vector(out));
    (t.forces.iter().map(|x| x.abs()).collect(), t.left)
}

/// Each group's bolt forces on the part it bolts onto in one case (the
/// plane's vectors, N), their tensions and what is not on the safe side.
fn case_forces(
    plans: &GroupPlans,
    axial: &BTreeMap<String, f64>,
    ends: Option<&EndForces>,
) -> Vec<CaseForces> {
    let mut out: Vec<CaseForces> = Vec::with_capacity(plans.groups.len());
    for g in &plans.groups {
        let mut c = CaseForces {
            forces: vec![[0.0, 0.0]; g.bolts.len()],
            tension: vec![0.0; g.bolts.len()],
            ..CaseForces::default()
        };
        let d = g
            .bolts
            .iter()
            .map(|b| b.diameter)
            .fold(f64::INFINITY, f64::min);
        if g.holding {
            let pts: Vec<V2> = g.bolts.iter().map(|b| g.plane.point(b.at)).collect();
            let c3 = g.plane.origin;
            let mut f3 = [0.0; 3];
            let mut m3 = [0.0; 3];
            // Without the ends laid onto the plate's members (L14): the
            // larger, bolt by bolt, is taken.
            let mut f3_own = [0.0; 3];
            let mut m3_own = [0.0; 3];
            for s in &g.sources {
                let (f, m) = source_load(s, &g.plane, g.position, axial, ends);
                let mo = add(m, cross(in_plane(&g.plane, s.centroid, c3), f));
                f3 = add(f3, f);
                m3 = add(m3, mo);
                f3_own = add(f3_own, f);
                m3_own = add(m3_own, mo);
                let (fr, mr) = riders_load(s, &g.plane, g.position, c3, axial, ends);
                f3 = add(f3, fr);
                m3 = add(m3, mr);
            }
            let ridden = g.sources.iter().any(|s| !s.riders.is_empty());
            // (L21) A level plate's holding bolts: the other ends' forces
            // whole or none, each with every choice of the forces the
            // plate may pass from its other holder.
            let mut base = vec![(f3, m3)];
            if ridden {
                base.push((f3_own, m3_own));
            }
            if !g.passed.is_empty() {
                base.push(([0.0; 3], [0.0; 3]));
            }
            let pass: Vec<(V3, V3)> = g
                .passed
                .iter()
                .map(|s| {
                    let (f, m) = source_load(s, &g.plane, g.position, axial, ends);
                    (f, add(m, cross(in_plane(&g.plane, s.centroid, c3), f)))
                })
                .collect();
            // Beyond PASSED_EACH such forces, every share exact
            // (2026-11-25): the bolts widened below; the corners of the
            // bases alone here.
            let exact = pass.len() > PASSED_EACH;
            let loads = if exact {
                base.clone()
            } else {
                with_passed(base.clone(), &pass)
            };
            let f2 = g.plane.vector(f3);
            let mn = dot(m3, g.plane.n);
            let e = bg::elastic(
                &pts,
                &PlaneLoad {
                    force: f2,
                    at: bg::centroid(&pts),
                    moment: mn,
                },
            );
            c.forces = e.forces.clone();
            for (f, m) in loads.iter().skip(1) {
                let other = bg::elastic(
                    &pts,
                    &PlaneLoad {
                        force: g.plane.vector(*f),
                        at: bg::centroid(&pts),
                        moment: dot(*m, g.plane.n),
                    },
                );
                larger(&mut c.forces, &other.forces);
            }
            let l = lever(&pts, d);
            let size = f2[0].hypot(f2[1]);
            if !e.shared && mn.abs() / l >= 0.01 * size.max(1e-9) && mn.abs() > 1e-6 {
                c.said.push(format!(
                    "one bolt cannot carry the moment of the members' forces about it ({:.2} kN \
                     m): a pin, the moment not carried",
                    mn.abs() / 1000.0
                ));
            }
            let (t, left) = pull(&g.plane, &pts, f3, m3);
            c.tension = t;
            for (f, m) in loads.iter().skip(1) {
                let (t_other, _) = pull(&g.plane, &pts, *f, *m);
                for (x, y) in c.tension.iter_mut().zip(t_other) {
                    *x = x.max(y);
                }
            }
            if exact {
                widen_exact(&g.plane, &pts, &base, &pass, &mut c.forces, &mut c.tension);
            }
            // The words: the loading with every force (and, with the
            // forces a level plate may pass, the most of any loading).
            let worded = if g.passed.is_empty() { 1 } else { loads.len() };
            let (mut normal, mut out_m) = loads[..worded]
                .iter()
                .map(|(f, m)| {
                    (
                        dot(*f, g.plane.n).abs(),
                        norm(sub(*m, scale(g.plane.n, dot(*m, g.plane.n)))),
                    )
                })
                .fold((0.0, 0.0), |a: (f64, f64), b| (a.0.max(b.0), a.1.max(b.1)));
            if exact {
                // Each passed force's size added (a bound, for the words).
                for (f, m) in &pass {
                    normal += dot(*f, g.plane.n).abs();
                    out_m += norm(sub(*m, scale(g.plane.n, dot(*m, g.plane.n))));
                }
            }
            if normal + out_m / l >= 0.01 * size.max(1e-9) && normal + out_m > 1e-6 {
                c.pulled = Some(format!(
                    "the members' forces square to the plate ({:.1} kN) and their moment bending \
                     it out of its plane ({:.2} kN m) pull on its bolts: bolts in tension not \
                     checked",
                    normal / 1000.0,
                    out_m / 1000.0
                ));
            }
            if left / l >= 0.01 * size.max(normal).max(1e-9) && left > 1e-6 {
                c.said.push(format!(
                    "the moment about the line of the plate's holding bolts ({:.2} kN m) is not \
                     carried by them in tension (where the plies bear on each other not known)",
                    left / 1000.0
                ));
            }
        } else {
            for s in &g.sources {
                let (f_own, m_own) = source_load(s, &g.plane, g.position, axial, ends);
                let pts: Vec<V2> = s.bolts.iter().map(|x| g.plane.point(x.1)).collect();
                if pts.is_empty() {
                    continue;
                }
                // With the ends laid onto it (L14): their forces about its
                // bolts' centroid, the larger bolt by bolt.
                let (fr, mr) = riders_load(s, &g.plane, g.position, s.centroid, axial, ends);
                let (f, m) = (add(f_own, fr), add(m_own, mr));
                let f2 = g.plane.vector(f);
                let mn = dot(m, g.plane.n);
                let mut e = bg::elastic(
                    &pts,
                    &PlaneLoad {
                        force: f2,
                        at: bg::centroid(&pts),
                        moment: mn,
                    },
                );
                let (mut t, left) = pull(&g.plane, &pts, f, m);
                // Without the riders, and (L21) with every choice of the
                // forces a level plate may pass to it: the larger, bolt by
                // bolt.
                let mut base = vec![(f, m)];
                if !s.riders.is_empty() {
                    base.push((f_own, m_own));
                }
                let pass: Vec<(V3, V3)> = s
                    .passed
                    .iter()
                    .map(|p| {
                        let (fp, mp) = source_load(p, &g.plane, g.position, axial, ends);
                        (
                            fp,
                            add(mp, cross(in_plane(&g.plane, p.centroid, s.centroid), fp)),
                        )
                    })
                    .collect();
                // Beyond PASSED_EACH such forces, every share exact
                // (2026-11-25).
                let exact = pass.len() > PASSED_EACH;
                if exact {
                    widen_exact(&g.plane, &pts, &base, &pass, &mut e.forces, &mut t);
                }
                let corners = if exact {
                    base.clone()
                } else {
                    with_passed(base, &pass)
                };
                for (fo, mo) in corners.iter().skip(1) {
                    let other = bg::elastic(
                        &pts,
                        &PlaneLoad {
                            force: g.plane.vector(*fo),
                            at: bg::centroid(&pts),
                            moment: dot(*mo, g.plane.n),
                        },
                    );
                    larger(&mut e.forces, &other.forces);
                    let (t_other, _) = pull(&g.plane, &pts, *fo, *mo);
                    for (x, y) in t.iter_mut().zip(t_other) {
                        *x = x.max(y);
                    }
                }
                for ((k, _), v) in s.bolts.iter().zip(&e.forces) {
                    c.forces[*k][0] += v[0];
                    c.forces[*k][1] += v[1];
                }
                for ((k, _), v) in s.bolts.iter().zip(&t) {
                    // Several parts' pulls on one bolt: added (on the
                    // safe side).
                    c.tension[*k] += v;
                }
                let l = lever(&pts, d);
                let size = f2[0].hypot(f2[1]);
                if !e.shared && mn.abs() / l >= 0.01 * size.max(1e-9) && mn.abs() > 1e-6 {
                    push_once(
                        &mut c.said,
                        format!(
                            "one bolt of {} cannot carry its end moment ({:.2} kN m): a pin, the \
                             moment not carried",
                            s.member,
                            mn.abs() / 1000.0
                        ),
                    );
                }
                let normal = dot(f, g.plane.n).abs();
                let out_m = norm(sub(m, scale(g.plane.n, mn)));
                if normal + out_m / l >= 0.01 * size.max(1e-9)
                    && normal + out_m > 1e-6
                    && c.pulled.is_none()
                {
                    c.pulled = Some(format!(
                        "{}'s force square to {} ({:.1} kN) and its moment out of the plane \
                         ({:.2} kN m) pull on its bolts: bolts in tension not checked",
                        s.member,
                        g.part,
                        normal / 1000.0,
                        out_m / 1000.0
                    ));
                }
                if left / l >= 0.01 * size.max(normal).max(1e-9) && left > 1e-6 {
                    push_once(
                        &mut c.said,
                        format!(
                            "the moment of {} about the line of its bolts ({:.2} kN m) is not \
                             carried by them in tension (where the plies bear on each other not \
                             known)",
                            s.member,
                            left / 1000.0
                        ),
                    );
                }
            }
            for (k, map) in &g.from {
                let Some(theirs) = out.get(*k) else {
                    continue;
                };
                let hp = plans.groups[*k].plane;
                for (n, ours) in map {
                    let v3 = hp.to_3d(theirs.forces[*n]);
                    let v = g.plane.vector(v3);
                    c.forces[*ours][0] += v[0];
                    c.forces[*ours][1] += v[1];
                    c.tension[*ours] += theirs.tension[*n];
                }
                if c.pulled.is_none() {
                    c.pulled = theirs.pulled.clone();
                }
            }
        }
        out.push(c);
    }
    out
}

/// From `q` along `u` to ply `p`'s edge (none: no edge that way).
fn ray(p: &Ply, b: &Bolt, q: V3, u: V3) -> Option<f64> {
    match &p.edges {
        Edges::Outline(o) if o.len() >= 3 => {
            // A bolt outside the outline: nothing there (0, on the safe
            // side; the group then does not fit and is not checked).
            Some(crate::plate_checks::ray_to_outline(o, q, u).unwrap_or(0.0))
        }
        Edges::Outline(_) => None,
        Edges::Flange { dir, origin, end } => {
            let mut best: Option<f64> = None;
            // A bolt outside the flange: no steel round it (0; the group
            // then does not fit and is not checked).
            if b.flange.is_some_and(|(g, w, _)| outside(g, w)) {
                return Some(0.0);
            }
            if let Some((g, w, across)) = b.flange {
                let c = dot(u, across);
                if c > 1e-6 {
                    best = Some((w - g).max(0.0) / c);
                }
            }
            if let Some(cut) = end {
                let a = dot(u, *dir);
                if a < -1e-6 {
                    let s = dot(sub(q, *origin), *dir);
                    let e = (s - cut).max(0.0) / -a;
                    best = Some(best.map_or(e, |x| x.min(e)));
                }
            }
            best
        }
    }
}

/// The groups' rows of every case (`ends`: each case's end forces, or
/// empty), checked by the structure's plugin `p` — one call with every
/// group and case, the bolts in tension with them — or, without a plugin,
/// `none` said on each. A group that does not fit reads so, without a
/// usage.
pub(crate) fn group_rows(
    plans: &GroupPlans,
    structure: &str,
    plugin: Option<&ohl_structures_fe::model::PluginMethod>,
    none: Option<&str>,
    cases: &[PlateCase],
    ends: &[&EndForces],
) -> Vec<Vec<PlateUsage>> {
    use ohl_member_check::joints as by_plugin;
    use ohl_member_check::joints::groups as w;
    use ohl_member_check::joints::prying as wp;
    use ohl_member_check::joints::tension as wt;
    let per_case: Vec<Vec<CaseForces>> = cases
        .iter()
        .enumerate()
        .map(|(n, c)| {
            case_forces(
                plans,
                c.2,
                ends.get(n).copied().filter(|_| ends.len() == cases.len()),
            )
        })
        .collect();
    // The rows without a usage first.
    let said_rows = |n: usize| -> Vec<PlateUsage> {
        let mut rows: Vec<PlateUsage> = plans
            .not_checked
            .iter()
            .map(|(joint, part, words)| PlateUsage {
                joint: joint.clone(),
                plate: part.clone(),
                part: PlatePart::NotChecked(words.clone()),
                force: 0.0,
                usage: None,
                governs: words.clone(),
                checks: Vec::new(),
                notes: Vec::new(),
                does_not_fit: None,
                misfit_assumed: false,
            })
            .collect();
        // (L14's question 2) A member's own steel at its end bolts with the
        // ends laid onto it: worked here from its tension capacity.
        if let Some(c) = cases.get(n) {
            rows.extend(plans.end_steel.iter().map(|e| end_steel_row(e, c)));
        }
        rows
    };
    let largest = |n: usize, k: usize| -> f64 {
        per_case[n][k]
            .forces
            .iter()
            .map(|f| f[0].hypot(f[1]))
            .fold(0.0, f64::max)
    };
    let row = |n: usize, k: usize| -> PlateUsage {
        let g = &plans.groups[k];
        let misfit = (!g.misfits.is_empty()).then(|| g.misfits.join("; "));
        let assumed = misfit.is_some() && !g.misfit_given;
        PlateUsage {
            joint: g.joint.clone(),
            plate: g.part.clone(),
            part: plans.part_of(k),
            force: largest(n, k),
            usage: None,
            governs: misfit
                .as_ref()
                .map(|m| {
                    if assumed {
                        format!("{DOES_NOT_FIT_ASSUMED}: {m}")
                    } else {
                        format!("{DOES_NOT_FIT}: {m}")
                    }
                })
                .unwrap_or_default(),
            checks: Vec::new(),
            notes: g.notes.clone(),
            does_not_fit: misfit,
            misfit_assumed: assumed,
        }
    };
    let Some(p) = plugin else {
        let words = none.unwrap_or(ohl_structures_fe::plates::NOT_RUN);
        return (0..cases.len())
            .map(|n| {
                let mut rows = said_rows(n);
                rows.extend((0..plans.groups.len()).map(|k| {
                    let mut r = row(n, k);
                    if r.does_not_fit.is_none() {
                        r.governs = words.to_owned();
                    }
                    r
                }));
                rows
            })
            .collect();
    };
    // The groups that fit go to the plugin.
    let fits: Vec<usize> = (0..plans.groups.len())
        .filter(|k| plans.groups[*k].misfits.is_empty())
        .collect();
    let input = w::GroupInput {
        structure: structure.to_owned(),
        parameters: String::new(),
        cases: cases
            .iter()
            .map(|c| w::LoadCase {
                name: c.0.to_owned(),
                strength: c.1,
            })
            .collect(),
        groups: fits
            .iter()
            .map(|&k| {
                let g = &plans.groups[k];
                let c = bg::centroid(
                    &g.bolts
                        .iter()
                        .map(|b| g.plane.point(b.at))
                        .collect::<Vec<_>>(),
                );
                w::Group {
                    id: k as u32,
                    joint: g.joint.clone(),
                    kind: if g.holding { "holding" } else { "bearing" }.to_owned(),
                    part: g.part.clone(),
                    plies: g
                        .plies
                        .iter()
                        .map(|p| w::Ply {
                            part: p.part.clone(),
                            kind: match (p.member, p.sheared) {
                                (true, false) => "member",
                                (true, true) => "member-sheared",
                                (false, _) => "plate",
                            }
                            .to_owned(),
                            thickness: p.thickness,
                            fy: p.fy,
                            fu: p.fu,
                            redundant: p.redundant,
                            known: p.known,
                        })
                        .collect(),
                    bolts: g
                        .bolts
                        .iter()
                        .map(|b| {
                            let q = g.plane.point(b.at);
                            w::GroupBolt {
                                diameter: b.diameter,
                                hole: b.hole,
                                bolt_ultimate: b.ultimate,
                                shear_capacity: b.shear,
                                shear_capacity_shank: b.shank,
                                threads_in_shear: b.threads_in,
                                grade: b.grade.clone(),
                                planes: b.planes,
                                carries: b.carries.clone(),
                                at: (q[0] - c[0], q[1] - c[1]),
                            }
                        })
                        .collect(),
                    loads: (0..cases.len())
                        .map(|n| {
                            let forces = &per_case[n][k].forces;
                            g.bolts
                                .iter()
                                .zip(forces)
                                .map(|(b, f)| bolt_load(g, b, *f))
                                .collect()
                        })
                        .collect(),
                    notes: g.notes.clone(),
                }
            })
            .collect(),
    };
    // The bolts in tension: the groups with a pull in a case.
    let tee = |t: &Tee| wt::Tee {
        part: t.part.clone(),
        thickness: t.thickness,
        fy: t.fy,
        edge: t.edge,
        web: t.web,
        width: t.width,
    };
    let tension: Vec<wp::GroupPrying> = fits
        .iter()
        .filter(|&&k| (0..cases.len()).any(|n| per_case[n][k].tension.iter().any(|t| *t > 1e-6)))
        .map(|&k| {
            let g = &plans.groups[k];
            wp::GroupPrying {
                id: k as u32,
                bolts: g
                    .bolts
                    .iter()
                    .map(|b| wp::BoltPrying {
                        tension_capacity: b.tension,
                        tees: b.tees.iter().map(tee).collect(),
                        plates: b.plates.iter().map(tee).collect(),
                        unknown: b.unknown.clone(),
                    })
                    .collect(),
                loads: (0..cases.len())
                    .map(|n| per_case[n][k].tension.clone())
                    .collect(),
            }
        })
        .collect();
    let (checked, tension_checked) = if fits.is_empty() {
        (by_plugin::GroupsChecked::Results(Vec::new()), true)
    } else {
        // With the plates' prying when the plugin has it (2026-11-19),
        // else said not worked out by the plugin (each plate named among
        // the plies whose bending is not known).
        let (c, level) = by_plugin::check_joint_groups_prying(p, &input, &tension);
        (c, level != by_plugin::TensionChecked::No)
    };
    let not_checked = by_plugin::not_checked_by(p);
    (0..cases.len())
        .map(|n| {
            let mut rows = said_rows(n);
            for (k, forces) in per_case[n].iter().enumerate() {
                let mut r = row(n, k);
                if r.does_not_fit.is_some() {
                    rows.push(r);
                    continue;
                }
                let results = match &checked {
                    by_plugin::GroupsChecked::NotChecked(why) => {
                        r.governs = format!("{not_checked}: {why}");
                        rows.push(r);
                        continue;
                    }
                    by_plugin::GroupsChecked::Results(x) => x,
                };
                let Some(x) = results.iter().find(|x| x.id as usize == k) else {
                    r.governs =
                        format!("{not_checked}: the plugin returned nothing for this group");
                    rows.push(r);
                    continue;
                };
                r.notes.extend(x.notes.iter().cloned());
                let Some(u) = x.usages.iter().find(|u| u.load_case as usize == n) else {
                    r.governs =
                        format!("{not_checked}: the plugin returned no usage for this load case");
                    rows.push(r);
                    continue;
                };
                r.usage = Some(u.usage);
                r.governs = if u.check.is_empty() {
                    x.governs.clone()
                } else {
                    u.check.clone()
                };
                r.checks = vec![(r.governs.clone(), u.usage)];
                let mut unsafe_words: Vec<String> = Vec::new();
                if let Some(ws) = &x.not_safe {
                    unsafe_words.push(ws.clone());
                }
                unsafe_words.extend(forces.said.iter().cloned());
                if !tension_checked && let Some(w) = &forces.pulled {
                    unsafe_words.push(w.clone());
                }
                if !unsafe_words.is_empty() {
                    r.governs.push_str(&format!(
                        " {} {}]",
                        ohl_structures_fe::check::NOT_SAFE,
                        unsafe_words.join("; ")
                    ));
                }
                rows.push(r);
            }
            rows
        })
        .collect()
}

/// One bolt's load as the plugin takes it: the size of its force `f` and,
/// per ply, the distances along the direction it pushes the ply and
/// square to it.
fn bolt_load(g: &GroupPlan, b: &Bolt, f: V2) -> ohl_member_check::joints::groups::BoltLoad {
    let size = f[0].hypot(f[1]);
    let mut ends = Vec::new();
    let mut sides = Vec::new();
    let mut ahead = Vec::new();
    for p in &g.plies {
        if size < 1e-9 {
            ends.push(None);
            sides.push(None);
            ahead.push(None);
            continue;
        }
        let u = scale(g.plane.to_3d([f[0] / size, f[1] / size]), p.sign);
        let wq = cross(g.plane.n, u);
        ends.push(ray(p, b, b.at, u));
        let s = match (ray(p, b, b.at, wq), ray(p, b, b.at, scale(wq, -1.0))) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, None) | (None, x) => x,
        };
        sides.push(s);
        // The next hole ahead: in front, within a bolt diameter of the
        // force's line; its distance centre to centre.
        let mut next: Option<f64> = None;
        for h in &p.holes {
            let d = sub(*h, b.at);
            let along = dot(d, u);
            let lat = dot(d, wq).abs();
            if along > 1e-4 && lat < b.diameter {
                let c = norm(d);
                next = Some(next.map_or(c, |x: f64| x.min(c)));
            }
        }
        ahead.push(next);
    }
    ohl_member_check::joints::groups::BoltLoad {
        force: size,
        ends,
        sides,
        ahead,
    }
}

/// The row of a member's own steel at its end bolts with the ends laid onto
/// it in case `c` (L14's question 2): its tension capacity (times the case's
/// strength factor) against the larger of its own force and the resultant
/// through those bolts — *on the safe side*: the capacity the least over its
/// ends of its net section, block shear and bolts as the code's plugin gives
/// it, the resultant's whole size taken along the member however it turns
/// (its distances along each bolt's own force are in its end bolts' group).
fn end_steel_row(e: &EndSteel, c: &PlateCase) -> PlateUsage {
    let f = e.force(c.2);
    let r = e.capacity * c.1;
    let usage = if r > 0.0 {
        f / r
    } else if f > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    let words: Vec<&str> = e.riders.iter().map(|r| r.0.as_str()).collect();
    let words = words.join(", ");
    let governs = format!(
        "{}'s own steel at its end bolts with {words} laid onto it: {:.1} kN (the larger of its \
         own force and the resultant with theirs) against its tension capacity {:.1} kN ({})",
        e.member,
        f / 1000.0,
        r / 1000.0,
        e.governs
    );
    PlateUsage {
        joint: e.joint.clone(),
        plate: e.member.clone(),
        part: PlatePart::EndSteel(words.clone()),
        force: f,
        usage: Some(usage),
        governs: governs.clone(),
        checks: vec![(governs, usage)],
        notes: vec![format!(
            "{m}'s own steel at its end bolts (its net section, its block shear) under the \
             forces of {words} laid onto it near its end (L14's question 2, the owner \
             2026-11-22): its tension capacity as the code's plugin gives it — the least over its \
             ends of its net section, block shear and bolts (on the safe side) — against the \
             larger of its own force alone and the resultant of its own and theirs (each along \
             its line, the same case), the resultant's whole size taken along {m} (on the safe \
             side: across it the leg is the stronger; each bolt's distances along its own force \
             are checked in its end bolts' group)",
            m = e.member
        )],
        does_not_fit: None,
        misfit_assumed: false,
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m16(at: V3, carries: &[&str]) -> Bolt {
        Bolt {
            at,
            diameter: 0.016,
            hole: None,
            ultimate: Some(800e6),
            shear: None,
            shank: None,
            threads_in: None,
            tension: None,
            grade: "8.8".into(),
            planes: 1,
            carries: carries.iter().map(|s| (*s).to_owned()).collect(),
            flange: None,
            tees: Vec::new(),
            plates: Vec::new(),
            unknown: Vec::new(),
            grades: Vec::new(),
            given: true,
        }
    }

    fn source(member: &str, direction: V3, centroid: V3, bolts: Vec<(usize, V3)>) -> Source {
        Source {
            member: member.into(),
            end: 1,
            direction,
            centroid,
            bolts,
            riders: Vec::new(),
            passed: Vec::new(),
        }
    }

    fn group(holding: bool, bolts: Vec<Bolt>, sources: Vec<Source>, edges: Edges) -> GroupPlan {
        let c = centroid3(&bolts.iter().map(|b| b.at).collect::<Vec<_>>(), [0.0; 3]);
        GroupPlan {
            joint: "N2".into(),
            holding,
            part: if holding { "G1" } else { "L1" }.into(),
            holders: if holding { "L1" } else { "" }.into(),
            position: [0.0; 3],
            plane: Plane::new(c, [0.0, 0.0, 1.0], Some([1.0, 0.0, 0.0])).unwrap(),
            plies: vec![Ply {
                part: "X".into(),
                member: !holding,
                sheared: false,
                thickness: 0.008,
                fy: Some(355e6),
                fu: Some(510e6),
                redundant: false,
                known: true,
                edges,
                holes: bolts.iter().map(|b| b.at).collect(),
                sign: if holding { -1.0 } else { 1.0 },
            }],
            bolts,
            sources,
            from: Vec::new(),
            notes: Vec::new(),
            misfits: Vec::new(),
            misfit_given: false,
            ends_of: None,
            passed: Vec::new(),
        }
    }

    fn forces(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    /// Two diagonals on one plate onto a leg (invented, by hand): D1
    /// pulling 60 kN along (1, 1)/√2 through its bolts at (120, 120) mm,
    /// D2 pushing 60 kN along (1, −1)/√2 through (120, −120) mm. On the
    /// plate: (42.43, 42.43) + (−42.43, 42.43) = (0, 84.85) kN along the
    /// leg. Holding bolts at (0, ±50) mm: both lines pass through their
    /// centroid — no moment, 42.43 kN each. Moved to (−40, ±50) mm: M =
    /// 84.85 × 0.04 = 3.394 kN m, Σr² = 2 × 0.05² = 0.005 m², the moment's
    /// share 3.394 × 0.05 / 0.005 = 33.94 kN across: each bolt
    /// √(33.94² + 42.43²) = 54.33 kN.
    #[test]
    fn a_plates_holding_bolts_carry_the_resultant_and_its_moment() {
        let s = 0.5f64.sqrt();
        let sources = || {
            vec![
                source("D1", [s, s, 0.0], [0.12, 0.12, 0.0], Vec::new()),
                source("D2", [s, -s, 0.0], [0.12, -0.12, 0.0], Vec::new()),
            ]
        };
        let axial = forces(&[("D1", 60e3), ("D2", -60e3)]);
        for (x, want) in [(0.0, 60e3 * s), (-0.04, 54.33e3)] {
            let g = group(
                true,
                vec![
                    m16([x, -0.05, 0.0], &["plate G1"]),
                    m16([x, 0.05, 0.0], &["plate G1"]),
                ],
                sources(),
                Edges::Outline(Vec::new()),
            );
            let plans = GroupPlans {
                groups: vec![g],
                not_checked: Vec::new(),
                end_steel: Vec::new(),
            };
            let f = case_forces(&plans, &axial, None);
            for v in &f[0].forces {
                assert!((v[0].hypot(v[1]) - want).abs() < 10.0, "{x}: {v:?}");
            }
            // The sum is the resultant, along the leg.
            let sum = [
                f[0].forces[0][0] + f[0].forces[1][0],
                f[0].forces[0][1] + f[0].forces[1][1],
            ];
            assert!(
                sum[0].abs() < 1e-6 && (sum[1] - 120e3 * s).abs() < 1e-6,
                "{sum:?}"
            );
            assert!(f[0].said.is_empty(), "{:?}", f[0].said);
            // In the plate's plane: no bolt pulled.
            assert!(f[0].tension.iter().all(|t| t.abs() < 1e-6) && f[0].pulled.is_none());
        }
    }

    /// A redundant and a diagonal on the same leg bolt line (invented, by
    /// hand): D1's two bolts at (0, 0) and (0, 50) mm push the leg 40 kN
    /// along (0.6, 0.8), 20 kN each; R1's one bolt is the same hole as
    /// D1's first and pushes 10 kN along −x. That bolt: (12, 16) + (−10,
    /// 0) = (2, 16) kN, 16.12 kN; the other 20 kN. In the leg's flange
    /// (gauge 30 mm of 61 mm, the toe along +x, the leg running on): the
    /// second bolt's force reaches the toe 31/0.6 = 51.7 mm away, the edge
    /// square to it 31/0.8 = 38.75 mm; the first has D1's second bolt
    /// ahead, 50 mm centre to centre (6 mm off its force's line).
    #[test]
    fn a_shared_bolt_on_the_leg_carries_both_members() {
        let flange = Some((0.03, 0.061, [1.0, 0.0, 0.0]));
        let mut b0 = m16([0.0, 0.0, 0.0], &["D1", "R1"]);
        b0.flange = flange;
        let mut b1 = m16([0.0, 0.05, 0.0], &["D1"]);
        b1.flange = flange;
        let g = group(
            false,
            vec![b0, b1],
            vec![
                source(
                    "D1",
                    [0.6, 0.8, 0.0],
                    [0.0, 0.025, 0.0],
                    vec![(0, [0.0, 0.0, 0.0]), (1, [0.0, 0.05, 0.0])],
                ),
                source("R1", [-1.0, 0.0, 0.0], [0.0; 3], vec![(0, [0.0; 3])]),
            ],
            Edges::Flange {
                dir: [0.0, 1.0, 0.0],
                origin: [0.0; 3],
                end: None,
            },
        );
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
            end_steel: Vec::new(),
        };
        let axial = forces(&[("D1", 40e3), ("R1", 10e3)]);
        let f = case_forces(&plans, &axial, None);
        let v = &f[0].forces;
        assert!(
            (v[0][0] - 2e3).abs() < 1e-6 && (v[0][1] - 16e3).abs() < 1e-6,
            "{v:?}"
        );
        assert!((v[1][0].hypot(v[1][1]) - 20e3).abs() < 1e-6);
        let g = &plans.groups[0];
        let l1 = bolt_load(g, &g.bolts[1], v[1]);
        assert!((l1.ends[0].unwrap() - 0.031 / 0.6).abs() < 1e-9, "{l1:?}");
        assert!((l1.sides[0].unwrap() - 0.031 / 0.8).abs() < 1e-9, "{l1:?}");
        assert!(l1.ahead[0].is_none());
        let l0 = bolt_load(g, &g.bolts[0], v[0]);
        assert!((l0.ahead[0].unwrap() - 0.05).abs() < 1e-9, "{l0:?}");
    }

    /// L14: a member's end bolts carry the forces of an end laid onto it
    /// (invented, by hand). H1 pulls 30 kN along +x through its two bolts
    /// at x = 50 and 100 mm (centroid 75 mm); P1, laid onto H1 near its end,
    /// pulls 20 kN along +y through its bolts' centroid at x = 150 mm. On
    /// H1's bolts: (30, 20) kN with M = 0.075 × 20 = 1.5 kN m about their
    /// centroid; Σr² = 2 × 0.025² = 0.00125 m², the moment's share
    /// 1.5 × 0.025 / 0.00125 = 30 kN square to each radius. The bolt at
    /// 100 mm (nearer P1's line): (15, 10 + 30) → 42.72 kN; at 50 mm:
    /// (15, 10 − 30) → 25 kN (H1 alone: 15 kN each). Their sum is the
    /// resultant (30, 20) kN.
    #[test]
    fn an_end_laid_onto_a_member_loads_its_end_bolts() {
        let b0 = m16([0.05, 0.0, 0.0], &["H1"]);
        let b1 = m16([0.10, 0.0, 0.0], &["H1"]);
        let mut h1 = source(
            "H1",
            [1.0, 0.0, 0.0],
            [0.075, 0.0, 0.0],
            vec![(0, [0.05, 0.0, 0.0]), (1, [0.10, 0.0, 0.0])],
        );
        h1.riders = vec![source("P1", [0.0, 1.0, 0.0], [0.15, 0.0, 0.0], Vec::new())];
        let g = group(
            false,
            vec![b0, b1],
            vec![h1],
            Edges::Flange {
                dir: [1.0, 0.0, 0.0],
                origin: [0.0; 3],
                end: None,
            },
        );
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
            end_steel: Vec::new(),
        };
        let f = case_forces(&plans, &forces(&[("H1", 30e3), ("P1", 20e3)]), None);
        let v = &f[0].forces;
        assert!(
            (v[0][0] - 15e3).abs() < 1e-6 && (v[0][1] + 20e3).abs() < 1e-6,
            "{v:?}"
        );
        assert!(
            (v[1][0] - 15e3).abs() < 1e-6 && (v[1][1] - 40e3).abs() < 1e-6,
            "{v:?}"
        );
        assert!((v[1][0].hypot(v[1][1]) - 42.72e3).abs() < 10.0);
        // P1 pushing 20 kN: (15, −10 + 30) at 50 mm and (15, −10 − 30) at
        // 100 mm — mirrored, the same sizes.
        let f = case_forces(&plans, &forces(&[("H1", 30e3), ("P1", -20e3)]), None);
        let v = &f[0].forces;
        assert!((v[0][0].hypot(v[0][1]) - 25e3).abs() < 1e-6, "{v:?}");
        assert!((v[1][0].hypot(v[1][1]) - 42.72e3).abs() < 10.0, "{v:?}");
    }

    /// L14: another's force relieving a member's end bolts is not counted
    /// on (invented, by hand): P1 pulling 20 kN along −x on H1's line
    /// leaves (10, 0) kN on H1's two bolts, 5 kN each — each bolt keeps H1's
    /// own 15 kN (the larger, bolt by bolt).
    #[test]
    fn a_relieving_end_is_not_counted_on() {
        let mut h1 = source(
            "H1",
            [1.0, 0.0, 0.0],
            [0.075, 0.0, 0.0],
            vec![(0, [0.05, 0.0, 0.0]), (1, [0.10, 0.0, 0.0])],
        );
        h1.riders = vec![source("P1", [-1.0, 0.0, 0.0], [0.15, 0.0, 0.0], Vec::new())];
        let g = group(
            false,
            vec![
                m16([0.05, 0.0, 0.0], &["H1"]),
                m16([0.10, 0.0, 0.0], &["H1"]),
            ],
            vec![h1],
            Edges::Flange {
                dir: [1.0, 0.0, 0.0],
                origin: [0.0; 3],
                end: None,
            },
        );
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
            end_steel: Vec::new(),
        };
        let f = case_forces(&plans, &forces(&[("H1", 30e3), ("P1", 20e3)]), None);
        for v in &f[0].forces {
            assert!((v[0] - 15e3).abs() < 1e-6 && v[1].abs() < 1e-6, "{v:?}");
        }
    }

    /// L14: a plate's holding bolts carry the ends laid onto a member on
    /// the plate too (the first test's plate, invented): D1 as there, D2
    /// with R1 laid onto it near its end — R1 pushing 30 kN along −y through
    /// (120, 0) mm. The resultant on the plate (0, 84.85 − 30) kN through
    /// the holding bolts' centroid at (0, 0) (R1's line passes through it
    /// along y): 27.43 kN each, against 42.43 kN without R1 — each bolt
    /// keeps the larger, 42.43 kN.
    #[test]
    fn a_holding_group_follows_the_force_through() {
        let s = 0.5f64.sqrt();
        let mut d2 = source("D2", [s, -s, 0.0], [0.12, -0.12, 0.0], Vec::new());
        d2.riders = vec![source("R1", [0.0, 1.0, 0.0], [0.0, 0.12, 0.0], Vec::new())];
        let g = group(
            true,
            vec![
                m16([0.0, -0.05, 0.0], &["plate G1"]),
                m16([0.0, 0.05, 0.0], &["plate G1"]),
            ],
            vec![source("D1", [s, s, 0.0], [0.12, 0.12, 0.0], Vec::new()), d2],
            Edges::Outline(Vec::new()),
        );
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
            end_steel: Vec::new(),
        };
        let f = case_forces(
            &plans,
            &forces(&[("D1", 60e3), ("D2", -60e3), ("R1", -30e3)]),
            None,
        );
        for v in &f[0].forces {
            assert!((v[0].hypot(v[1]) - 60e3 * s).abs() < 1e-6, "{v:?}");
        }
        // R1 pulling 30 kN (along +y): (0, 114.85) kN, 57.43 kN each.
        let f = case_forces(
            &plans,
            &forces(&[("D1", 60e3), ("D2", -60e3), ("R1", 30e3)]),
            None,
        );
        for v in &f[0].forces {
            assert!((v[0].hypot(v[1]) - (60e3 * s + 15e3)).abs() < 1e-6, "{v:?}");
        }
    }

    /// A holding bolt pushes the plate against its force: the end distance
    /// in the plate is taken the other way (a 200 × 100 mm plate, the bolt
    /// 50 mm from its left edge, the force on the leg along +x: 50 mm).
    #[test]
    fn the_plate_is_pushed_the_other_way() {
        let outline = vec![
            [-0.05, -0.05, 0.0],
            [0.15, -0.05, 0.0],
            [0.15, 0.05, 0.0],
            [-0.05, 0.05, 0.0],
        ];
        let g = group(
            true,
            vec![m16([0.0; 3], &["plate G1"])],
            Vec::new(),
            Edges::Outline(outline),
        );
        let l = bolt_load(&g, &g.bolts[0], [10e3, 0.0]);
        assert!((l.ends[0].unwrap() - 0.05).abs() < 1e-12, "{l:?}");
        assert!((l.sides[0].unwrap() - 0.05).abs() < 1e-12, "{l:?}");
    }

    /// A diagonal pulling out of the leg's flange (invented, by hand): D1's
    /// two bolts at (0, 0) and (0, 50) mm on the leg, its force 30 kN along
    /// (0, 0.8, 0.6) — 18 kN square to the plies, shared equally, 9 kN on
    /// each bolt (the side its plies lie on not used: pulling); in the
    /// plane 24 kN, 12 kN each. Said when the plugin does not check
    /// tension.
    #[test]
    fn a_force_out_of_the_plane_pulls_the_bolts() {
        let g = group(
            false,
            vec![
                m16([0.0, 0.0, 0.0], &["D1"]),
                m16([0.0, 0.05, 0.0], &["D1"]),
            ],
            vec![source(
                "D1",
                [0.0, 0.8, 0.6],
                [0.0, 0.025, 0.0],
                vec![(0, [0.0; 3]), (1, [0.0, 0.05, 0.0])],
            )],
            Edges::Flange {
                dir: [0.0, 1.0, 0.0],
                origin: [0.0; 3],
                end: None,
            },
        );
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
            end_steel: Vec::new(),
        };
        let f = case_forces(&plans, &forces(&[("D1", -30e3)]), None);
        for (v, t) in f[0].forces.iter().zip(&f[0].tension) {
            assert!((v[0].hypot(v[1]) - 12e3).abs() < 1e-6, "{v:?}");
            assert!((t - 9e3).abs() < 1e-6, "{t}");
        }
        assert!(
            f[0].pulled
                .as_deref()
                .is_some_and(|w| w.contains("bolts in tension"))
        );
    }

    /// The plate a holding bolt bends, a flange without a web (2026-11-19;
    /// invented, by hand): a plate in z = 0 from (−100, −50) to (100, 150)
    /// mm, the holding bolts (16 mm) at x = ∓40 mm on the leg's line (along
    /// x), the leg's flange 60 mm wide under them, the bolts 30 mm from its
    /// heel, its toe towards −y; the members' bolts on the plate at (0, 60)
    /// and (20, 90) mm. For the bolt at −40: b = 90 mm (the farthest, square
    /// to the line); a the other way (−y): the outline 50 mm, the flange's
    /// toe 30 mm — 30 mm; w: towards +x the next holding bolt 80 mm away
    /// (40 mm), towards −x the outline 60 mm (b − d/2 = 82 mm more) — 100
    /// mm. No member's bolt placed, or no outline: not known.
    #[test]
    fn a_plate_bent_by_a_holding_bolt() {
        let outline = [
            [-0.1, -0.05, 0.0],
            [0.1, -0.05, 0.0],
            [0.1, 0.15, 0.0],
            [-0.1, 0.15, 0.0],
        ];
        let holding = [[-0.04, 0.0, 0.0], [0.04, 0.0, 0.0]];
        let pulls = [[0.0, 0.06, 0.0], [0.02, 0.09, 0.0]];
        let flange = Some((0.03, 0.06, [0.0, -1.0, 0.0]));
        let t = super::plate_tee(
            "G1",
            0.01,
            Some(355e6),
            holding[0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &pulls,
            &holding,
            flange,
            &outline,
            0.016,
        )
        .unwrap();
        assert!((t.web - 0.09).abs() < 1e-12, "{t:?}");
        assert!((t.edge - 0.03).abs() < 1e-12, "{t:?}");
        assert!((t.width - 0.10).abs() < 1e-12, "{t:?}");
        let call = |pulls: &[V3], outline: &[V3]| {
            super::plate_tee(
                "G1",
                0.01,
                None,
                holding[0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                pulls,
                &holding,
                flange,
                outline,
                0.016,
            )
        };
        assert!(call(&[], &outline).is_none());
        assert!(call(&pulls, &[]).is_none());
    }

    /// The flange a bolt bends (invented, by hand): a 60 mm leg 6 mm
    /// thick, the bolt (16 mm) 30 mm from the heel: a = 30, b = 24 mm; b −
    /// d/2 = 16 mm each way at most; its neighbour on its gauge line 50 mm
    /// away: 25 mm that way, so 16 + 16 = 32 mm (a bolt beside it on
    /// another gauge line does not count); with the flange's end 10 mm
    /// behind it: 16 + 10 = 26 mm. Off the flange: none.
    #[test]
    fn a_flange_and_the_length_a_bolt_takes() {
        let t = flange_tee(
            "L1",
            0.006,
            Some(355e6),
            0.03,
            0.06,
            0.1,
            &[[0.1, 0.03], [0.15, 0.03], [0.1, 0.05]],
            None,
            0.016,
        )
        .unwrap();
        assert!((t.edge - 0.03).abs() < 1e-12 && (t.web - 0.024).abs() < 1e-12);
        assert!((t.width - 0.032).abs() < 1e-12, "{t:?}");
        let t = flange_tee(
            "L1",
            0.006,
            None,
            0.03,
            0.06,
            0.1,
            &[[0.1, 0.03], [0.15, 0.03], [0.1, 0.05]],
            Some(0.09),
            0.016,
        )
        .unwrap();
        assert!((t.width - 0.026).abs() < 1e-12, "{t:?}");
        assert!(flange_tee("L1", 0.006, None, 0.07, 0.06, 0.1, &[], None, 0.016).is_none());
        assert!(many_lines(&[0.02, 0.04, 0.06], 0.016));
        assert!(!many_lines(&[0.02, 0.025, 0.06], 0.016));
    }

    /// Holes in one ply overlapping: 10 mm apart with 17.5 mm holes; one
    /// hole (1 mm apart) and holes clear of each other are not.
    #[test]
    fn overlapping_holes_do_not_fit() {
        let h = |x: f64, g: bool| ([x, 0.0, 0.0], 0.0175, format!("at {x}"), g);
        assert_eq!(overlaps("L1", &[h(0.0, true), h(0.01, false)]).len(), 1);
        assert!(overlaps("L1", &[h(0.0, true), h(0.001, true)]).is_empty());
        assert!(overlaps("L1", &[h(0.0, true), h(0.05, true)]).is_empty());
        // Given where one of the two is given (D14); assumed where both are
        // assumed.
        assert!(overlaps("L1", &[h(0.0, true), h(0.01, false)])[0].1);
        assert!(!overlaps("L1", &[h(0.0, false), h(0.01, false)])[0].1);
    }

    /// L21: a level plate's holding bolts take the force the plate may pass
    /// from its other holder (invented, by hand). G pulls 10 kN along +x
    /// through the holding bolts' centroid (0, 0); the other holder H2
    /// pulls 10 kN along +y through it too; two holding bolts at (0, ±50)
    /// mm, no moment. G alone: (5, 0) kN each; with H2's whole force:
    /// (5, 5) kN, 7.071 kN each (H2's alone: (0, 5)). The larger bolt by
    /// bolt: 7.071 kN (5 kN before L21, H2's force not counted).
    #[test]
    fn a_level_plates_holding_bolts_take_the_other_holders_force() {
        let mut g = group(
            true,
            vec![
                m16([0.0, -0.05, 0.0], &["plate G1"]),
                m16([0.0, 0.05, 0.0], &["plate G1"]),
            ],
            vec![source("G", [1.0, 0.0, 0.0], [0.0; 3], Vec::new())],
            Edges::Outline(Vec::new()),
        );
        let axial = forces(&[("G", 10e3), ("H2", 10e3)]);
        let run = |g: &GroupPlan| {
            let plans = GroupPlans {
                groups: vec![g.clone()],
                not_checked: Vec::new(),
                end_steel: Vec::new(),
            };
            case_forces(&plans, &axial, None)
        };
        for v in &run(&g)[0].forces {
            assert!((v[0].hypot(v[1]) - 5e3).abs() < 1e-6, "{v:?}");
        }
        g.passed = vec![source("H2", [0.0, 1.0, 0.0], [0.0; 3], Vec::new())];
        for v in &run(&g)[0].forces {
            assert!(
                (v[0] - 5e3).abs() < 1e-6 && (v[1] - 5e3).abs() < 1e-6,
                "{v:?}"
            );
            assert!((v[0].hypot(v[1]) - 7071.07).abs() < 0.1);
        }
    }

    /// L21: the other holder's force alone governs where the plate's other
    /// ends oppose it (invented, by hand): G pulls 10 kN along +x, H2
    /// pushes 20 kN along +x (−20 kN), both through the centroid. G alone:
    /// 5 kN each; G with H2: −10 kN, 5 kN each; H2 alone (G's force taken
    /// by H2's own bolts, the plate passing H2's whole force): 10 kN each —
    /// the larger, 10 kN.
    #[test]
    fn the_other_holders_force_alone_where_it_governs() {
        let mut g = group(
            true,
            vec![
                m16([0.0, -0.05, 0.0], &["plate G1"]),
                m16([0.0, 0.05, 0.0], &["plate G1"]),
            ],
            vec![source("G", [1.0, 0.0, 0.0], [0.0; 3], Vec::new())],
            Edges::Outline(Vec::new()),
        );
        g.passed = vec![source("H2", [1.0, 0.0, 0.0], [0.0; 3], Vec::new())];
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
            end_steel: Vec::new(),
        };
        let f = case_forces(&plans, &forces(&[("G", 10e3), ("H2", -20e3)]), None);
        for v in &f[0].forces {
            assert!((v[0].hypot(v[1]) - 10e3).abs() < 1e-6, "{v:?}");
        }
    }

    /// L21: the other holder running on through the joint as one part (two
    /// pieces in line, invented, by hand): what the run gives off at the
    /// joint is the sum of its pieces' forces — B1 pulling 10 kN along +x,
    /// B2 (along −x from the joint) pushing 10 kN (−10 kN): (10, 0) + (10,
    /// 0) = 20 kN through the plate's bolts onto it at the centroid, 10 kN
    /// on each holding bolt; one piece alone: 5 kN.
    #[test]
    fn a_running_holder_passes_what_its_pieces_give_off() {
        let mut g = group(
            true,
            vec![
                m16([0.0, -0.05, 0.0], &["plate G1"]),
                m16([0.0, 0.05, 0.0], &["plate G1"]),
            ],
            vec![source("G", [1.0, 0.0, 0.0], [0.0; 3], Vec::new())],
            Edges::Outline(Vec::new()),
        );
        g.passed = vec![
            source("B1", [1.0, 0.0, 0.0], [0.0; 3], Vec::new()),
            source("B2", [-1.0, 0.0, 0.0], [0.0; 3], Vec::new()),
        ];
        let plans = GroupPlans {
            groups: vec![g],
            ..GroupPlans::default()
        };
        let f = case_forces(&plans, &forces(&[("B1", 10e3), ("B2", -10e3)]), None);
        for v in &f[0].forces {
            assert!((v[0].hypot(v[1]) - 10e3).abs() < 1e-6, "{v:?}");
        }
    }

    /// L21: a holder's own end bolts take the force a level plate may pass
    /// to it from the other holder (invented, by hand): H1 pulls 30 kN
    /// along +x through its two bolts at x = 50 and 100 mm (centroid 75
    /// mm); H2's 20 kN along +y passes through that centroid. H1 alone: (15,
    /// 0) kN each; with H2's: (15, 10) kN, 18.03 kN each.
    #[test]
    fn a_holders_end_bolts_take_the_force_the_plate_passes() {
        let mut h1 = source(
            "H1",
            [1.0, 0.0, 0.0],
            [0.075, 0.0, 0.0],
            vec![(0, [0.05, 0.0, 0.0]), (1, [0.10, 0.0, 0.0])],
        );
        h1.passed = vec![source("H2", [0.0, 1.0, 0.0], [0.075, 0.0, 0.0], Vec::new())];
        let g = group(
            false,
            vec![
                m16([0.05, 0.0, 0.0], &["H1"]),
                m16([0.10, 0.0, 0.0], &["H1"]),
            ],
            vec![h1],
            Edges::Flange {
                dir: [1.0, 0.0, 0.0],
                origin: [0.0; 3],
                end: None,
            },
        );
        let plans = GroupPlans {
            groups: vec![g],
            not_checked: Vec::new(),
            end_steel: Vec::new(),
        };
        let f = case_forces(&plans, &forces(&[("H1", 30e3), ("H2", 20e3)]), None);
        for v in &f[0].forces {
            assert!(
                (v[0] - 15e3).abs() < 1e-6 && (v[1] - 10e3).abs() < 1e-6,
                "{v:?}"
            );
        }
        // H2 pushing: (15, −10), the same size; H2 nil: H1's own 15 kN.
        let f = case_forces(&plans, &forces(&[("H1", 30e3), ("H2", -20e3)]), None);
        for v in &f[0].forces {
            assert!((v[0].hypot(v[1]) - 18027.76).abs() < 0.1, "{v:?}");
        }
        let f = case_forces(&plans, &forces(&[("H1", 30e3)]), None);
        for v in &f[0].forces {
            assert!((v[0].hypot(v[1]) - 15e3).abs() < 1e-6, "{v:?}");
        }
    }

    /// L21: a holder's own steel at its end bolts (invented, by hand): H1
    /// 10 kN along +x, G laid onto it (through the level plate) 10 kN along
    /// +y, H2's 10 kN along +y that the plate may pass. Without H2: the
    /// larger of 10 and |(10, 10)| = 14.14 kN; with: |(10, 20)| = 22.36 kN
    /// (10√5) the largest; H2 pushing 10 kN: |(10, 10)|, |(10, −10)|, |(10,
    /// 0)|: 14.14 kN.
    #[test]
    fn a_holders_own_steel_takes_the_force_the_plate_passes() {
        let mut e = EndSteel {
            joint: "N2".into(),
            member: "H1".into(),
            direction: [1.0, 0.0, 0.0],
            riders: vec![("G".into(), [0.0, 1.0, 0.0])],
            passed: Vec::new(),
            capacity: 100e3,
            governs: "net section".into(),
        };
        let axial = forces(&[("H1", 10e3), ("G", 10e3), ("H2", 10e3)]);
        assert!((e.force(&axial) - 14142.14).abs() < 0.1);
        e.passed = vec![("H2".into(), [0.0, 1.0, 0.0])];
        assert!((e.force(&axial) - 22360.68).abs() < 0.1);
        let axial = forces(&[("H1", 10e3), ("G", 10e3), ("H2", -10e3)]);
        assert!((e.force(&axial) - 14142.14).abs() < 0.1);
    }

    /// L21: the corners of the shares — each passed force added or not.
    #[test]
    fn every_choice_of_the_passed_forces_is_taken() {
        let f = |x: f64| ([x, 0.0, 0.0], [0.0; 3]);
        let v = with_passed(vec![f(1.0)], &[f(10.0), f(100.0)]);
        let mut xs: Vec<f64> = v.iter().map(|x| x.0[0]).collect();
        xs.sort_by(f64::total_cmp);
        assert_eq!(xs, [1.0, 11.0, 101.0, 111.0]);
        // Up to PASSED_EACH: 2ⁿ corners.
        let six: Vec<(V3, V3)> = (0..PASSED_EACH).map(|_| f(1.0)).collect();
        assert_eq!(with_passed(vec![f(0.0)], &six).len(), 1 << PASSED_EACH);
    }

    /// 2026-11-25: beyond PASSED_EACH, the largest over every share exact
    /// — against every one of the 2ⁿ corners tried (n = 10, invented
    /// vectors in all directions; before, the rest taken together, none or
    /// all, missed a mix of them). By hand: base (1, 0) with (1, 1),
    /// (1, −1) and (−3, 0) — the largest (3, 0), not (0, 0) of all.
    #[test]
    fn beyond_six_passed_forces_every_share_is_exact() {
        let v = box_max_2d([1.0, 0.0], &[[1.0, 1.0], [1.0, -1.0], [-3.0, 0.0]]);
        assert!((v[0] - 3.0).abs() < 1e-12 && v[1].abs() < 1e-12, "{v:?}");
        // Every corner of ten invented generators.
        let gens: Vec<V2> = (0..10)
            .map(|k| {
                let a = 0.7 * k as f64 + 0.3;
                let r = 1.0 + (k % 3) as f64;
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        let base = [0.4, -1.3];
        let mut best = 0.0f64;
        for mask in 0..(1u32 << gens.len()) {
            let mut s = base;
            for (k, g) in gens.iter().enumerate() {
                if mask & (1 << k) != 0 {
                    s = [s[0] + g[0], s[1] + g[1]];
                }
            }
            best = best.max(s[0].hypot(s[1]));
        }
        let v = box_max_2d(base, &gens);
        assert!((v[0].hypot(v[1]) - best).abs() < 1e-9, "{v:?} {best}");
        // The rest taken together (the first six each, then all or none
        // of the other four) can find less: six small forces, then three
        // of 1 kN one way and one of 3 kN the other — together none or
        // nothing net, the exact search 3 kN (three of them).
        let mut gens: Vec<V2> = (0..6).map(|k| [0.0, 10.0 * (k + 1) as f64]).collect();
        gens.extend([[1e3, 0.0], [1e3, 0.0], [1e3, 0.0], [-3e3, 0.0]]);
        let mut together = 0.0f64;
        for mask in 0..(1u32 << 7) {
            let mut s = [0.0, 0.0];
            for (k, g) in gens.iter().enumerate() {
                let bit = if k < 6 { k } else { 6 };
                if mask & (1 << bit) != 0 {
                    s = [s[0] + g[0], s[1] + g[1]];
                }
            }
            together = together.max(s[0].hypot(s[1]));
        }
        let v = box_max_2d([0.0, 0.0], &gens);
        let exact = v[0].hypot(v[1]);
        assert!((exact - 3e3f64.hypot(210.0)).abs() < 1e-9, "{v:?}");
        assert!((together - 210.0).abs() < 1e-9, "{together}");
        // A number: each share where it adds to that sign.
        assert_eq!(box_max_1d(1.0, &[2.0, -5.0, 3.0]), 6.0);
        assert_eq!(box_max_1d(-1.0, &[2.0, -5.0]), 6.0);
        assert_eq!(box_max_1d(0.5, &[]), 0.5);
    }

    /// 2026-11-25: a group's bolts under eight forces a level plate may
    /// pass (invented: three bolts in a plane, forces in and out of it with
    /// moments): each bolt's shear and pull by the exact search equal the
    /// largest over all 2⁸ corners, bolt by bolt.
    #[test]
    fn a_groups_bolts_beyond_six_passed_forces_match_every_corner() {
        let plane = Plane::new([0.0; 3], [0.0, 0.0, 1.0], Some([1.0, 0.0, 0.0])).unwrap();
        let pts: Vec<V2> = vec![[0.0, 0.0], [0.06, 0.0], [0.03, 0.05]];
        let load = |k: usize| -> (V3, V3) {
            let a = 1.1 * k as f64;
            (
                [3e3 * a.cos(), 2e3 * a.sin(), 500.0 * (k as f64 - 3.5)],
                [
                    40.0 * a.sin(),
                    -30.0 * a.cos(),
                    90.0 * (k % 3) as f64 - 60.0,
                ],
            )
        };
        let base = vec![
            ([1e3, -2e3, 300.0], [10.0, 0.0, 25.0]),
            ([0.0; 3], [0.0; 3]),
        ];
        let pass: Vec<(V3, V3)> = (0..8).map(load).collect();
        let n = pts.len();
        let mut forces = vec![[0.0, 0.0]; n];
        let mut tension = vec![0.0; n];
        widen_exact(&plane, &pts, &base, &pass, &mut forces, &mut tension);
        // Every corner, bolt by bolt.
        let mut best_f = vec![0.0f64; n];
        let mut best_t = vec![0.0f64; n];
        for b in &base {
            for mask in 0..(1u32 << pass.len()) {
                let mut l = *b;
                for (k, p) in pass.iter().enumerate() {
                    if mask & (1 << k) != 0 {
                        l = (add(l.0, p.0), add(l.1, p.1));
                    }
                }
                let e = bg::elastic(
                    &pts,
                    &PlaneLoad {
                        force: plane.vector(l.0),
                        at: bg::centroid(&pts),
                        moment: dot(l.1, plane.n),
                    },
                );
                let (t, _) = pull(&plane, &pts, l.0, l.1);
                for k in 0..n {
                    best_f[k] = best_f[k].max(e.forces[k][0].hypot(e.forces[k][1]));
                    best_t[k] = best_t[k].max(t[k]);
                }
            }
        }
        for k in 0..n {
            let got = forces[k][0].hypot(forces[k][1]);
            assert!(
                (got - best_f[k]).abs() < 1e-6,
                "bolt {k}: {got} {}",
                best_f[k]
            );
            assert!(
                (tension[k] - best_t[k]).abs() < 1e-6,
                "bolt {k}: {} {}",
                tension[k],
                best_t[k]
            );
        }
    }

    /// L21 on L20's invented corner, laid out: the level plate's two
    /// holding groups each take the other holder's force the plate may
    /// pass (not the diagonal's twice), said without NOT ON THE SAFE SIDE;
    /// no bolt's force less than without it, some more.
    #[test]
    fn the_level_plates_holders_take_each_others_force() {
        use crate::level_plate_tests::{CORNER, ask, settings};
        let lib = crate::joints_tests::library();
        let bolts = Default::default();
        let s = settings(true);
        let r = crate::layout_run::lay_out(CORNER, &ask(&lib, &bolts, &s)).unwrap();
        let d = crate::joints::joint_details(&r.text, &lib, &bolts).unwrap();
        let p = d.joint("P").unwrap();
        let pd = p
            .plates
            .iter()
            .find(|q| q.also_bolts.is_some())
            .expect("a level plate");
        let holders: Vec<&str> = pd.holding_ends().map(|a| a.member.as_str()).collect();
        assert_eq!(holders.len(), 2, "{holders:?}");
        for (k, h) in holders.iter().enumerate() {
            let pass = passed(p, h);
            assert!(
                pass.iter().any(|a| a.0.member == holders[1 - k]),
                "{h}: {:?}",
                pass.iter().map(|a| &a.0.member).collect::<Vec<_>>()
            );
            assert!(!pass.iter().any(|a| a.0.member == "G"));
        }
        assert!(passed(p, "G").is_empty());
        let model = ohl_structures_fe::model::Model::from_structure_type_with_joint_details(
            &r.text, &lib, &bolts, None,
        )
        .unwrap();
        let plans = group_plans(&d, &model, &lib, &bolts);
        let held: Vec<&GroupPlan> = plans
            .groups
            .iter()
            .filter(|g| g.holding && g.part == pd.id)
            .collect();
        assert_eq!(held.len(), 2, "{:?}", plans.not_checked);
        let axial = forces(&[("G", 10e3), ("H1", 10e3), ("H2", 10e3)]);
        let mut more = false;
        for g in &held {
            assert_eq!(g.passed.len(), 1, "{:?}", g.passed);
            assert!(holders.contains(&g.passed[0].member.as_str()));
            assert!(!g.holders.contains(&g.passed[0].member));
            assert!(
                g.notes.iter().any(|n| n.contains("may pass")),
                "{:?}",
                g.notes
            );
            assert!(
                !g.notes.iter().any(|n| n.contains("NOT ON THE SAFE SIDE")),
                "{:?}",
                g.notes
            );
            let run = |g: GroupPlan| {
                case_forces(
                    &GroupPlans {
                        groups: vec![g],
                        ..GroupPlans::default()
                    },
                    &axial,
                    None,
                )
                .remove(0)
                .forces
            };
            let with = run((*g).clone());
            let mut before = (*g).clone();
            before.passed.clear();
            let without = run(before);
            for (a, b) in with.iter().zip(&without) {
                let (a, b) = (a[0].hypot(a[1]), b[0].hypot(b[1]));
                assert!(a >= b - 1e-6, "{a} < {b}");
                more |= a > b + 1.0;
            }
        }
        assert!(more);
    }
}
