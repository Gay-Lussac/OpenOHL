// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Rolls from the joints' planes** (the automatic joint layout's L18,
//! step 2 — Orient angles v2; the owner, 2026-11-22: lay out by planes
//! first). Built, in active development, not approved yet.
//!
//! An angle has **one roll along its whole length**, so the turn that lays
//! a flange in the planes of one joint ([`crate::joint_planes`]) must suit
//! every joint the member reaches; a leg or chord running on through
//! joints in line (one part, the same section, a splice) is **one run**
//! turned alike, so that its pieces keep matching. For each run the turn
//! about its axis (the same for every piece, added to its roll) is solved
//! over every end at every joint of the run:
//!
//! - where a piece of the run is the joint's host (what the others bolt
//!   onto), each other end: the angle its axis runs out of the host's
//!   flange's plane nearer to it, after the turn
//!   ([`crate::joint_planes::off_flange`]);
//! - where a piece arrives onto another member (an arm's chord onto a
//!   leg), its own leg against what it bolts onto (the joint details'
//!   `flat`: the turn laying it flat, and how far it stays off however
//!   turned), weighing as an end it hosts ([`OWN_WEIGHT`], measured).
//!
//! The turn chosen lays **the most ends within the owner's 2°**
//! ([`crate::FLAT_OFF`]) of their flange's plane (each end weighing 1,
//! [`Weighting::Count`], or its steel area, [`Weighting::Area`]), **centred**
//! in the turns that keep those ends within (the middle of their bands'
//! overlap: each as far inside its 2° as the others allow, so that a later
//! small change — pieces made one part with one roll — keeps them in); of
//! several such, the widest overlap, then the least sum of squares of all
//! (each end counted at most [`OUT_CAP_DEG`]: a few degrees' turn does not
//! bring a member lying far out in), then the smallest. Within
//! [`ROLL_WINDOW_DEG`] of the roll it starts from (a leg's heel stays out
//! of the tower: Orient angles' corner). A turn smaller than
//! [`MIN_TURN_DEG`] is none (a run again changes nothing).
//!
//! Legs first (their runs solved on the joints as they are), then the
//! chords (on the joints with the legs turned): the owner's order. The
//! bracing is laid flat by the joint layout's one roll per member
//! ([`crate::whole_roll`], J3: diagonals, then secondary bracing, then
//! redundants), against the flanges as turned here. Each run's residual
//! angle at each of its ends is reported ([`PlaneRoll::ends`]).
//!
//! Pure functions of the joints' details: deterministic. The tolerances are
//! the tool's, not a standard's.

use std::collections::{BTreeMap, BTreeSet};

use crate::joints::{Arrival, JointDetail, JointDetails};

/// A run is turned at most this from the roll it starts from, degrees (the
/// tool's value: Orient angles' corner lets each flange lie up to 22.5°
/// off its face, [`crate::CORNER_DEG`]; the heel stays out of the tower).
pub const ROLL_WINDOW_DEG: f64 = 22.5;

/// An end lying farther out of its flange's plane counts as this in the
/// sum of squares, degrees (the tool's value: the joint groups' "near a
/// flange", [`crate::joint_classes::NEAR_PLANE_DEG`]).
pub const OUT_CAP_DEG: f64 = crate::joint_classes::NEAR_PLANE_DEG;

/// A turn smaller than this is none, degrees (the tool's value: the files
/// write a roll to a hundredth of a degree; a run again changes nothing).
pub const MIN_TURN_DEG: f64 = 0.05;

/// A piece's own end weighs this many hosted ends (the tool's value,
/// measured: its own ends first — 1 000 — laid out fewer joints whole on six
/// private real towers than weighing them alike, +42 against +60 over the
/// outline's turn; the joint layout's one roll per member lays the bracing
/// flat on what it bolts onto afterwards).
pub const OWN_WEIGHT: f64 = 1.0;

/// Pieces in line through a joint: within this of each other, degrees (as
/// Orient angles' and the leg-rotation check's).
const IN_LINE_DEG: f64 = 10.0;

/// How the ends weigh in the turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weighting {
    /// Each end 1 (the most ends within 2°).
    #[default]
    Count,
    /// Each end its steel area (the biggest members' planes first).
    Area,
}

/// One end a run's turn is solved over.
#[derive(Debug, Clone, PartialEq)]
pub struct RollEnd {
    /// The joint.
    pub joint: String,
    /// The member end (another member's where the run's piece hosts it;
    /// the piece itself where it arrives onto another member).
    pub member: String,
    /// The run's piece it is measured against.
    pub piece: String,
    /// The turn of the piece laying it in (its flange's plane, or flat),
    /// degrees.
    pub turn: f64,
    /// Its angle to the piece's axis, degrees (where it is hosted; 90
    /// where it is the piece's own end).
    pub to_host: f64,
    /// The piece's own end: how far it stays off however turned, degrees;
    /// none: an end the piece hosts.
    pub floor: Option<f64>,
    /// The turns laying it in repeat every this, degrees (90: either
    /// flange, or either leg of an equal-leg angle; 180: one leg).
    pub period: f64,
    /// Its weight.
    pub weight: f64,
}

impl RollEnd {
    /// How far it lies out after a turn of `delta` degrees, degrees.
    pub fn off(&self, delta: f64) -> f64 {
        let rel = fold(self.turn - delta, self.period);
        match self.floor {
            None => (self.to_host.to_radians().sin() * rel.to_radians().sin().abs())
                .min(1.0)
                .asin()
                .to_degrees(),
            Some(f) => f.hypot(rel),
        }
    }

    /// The turns within the owner's 2° of laying it in: the half-width of
    /// that band about [`RollEnd::turn`], degrees (none: never within).
    fn band(&self) -> Option<f64> {
        let lim = crate::FLAT_OFF;
        match self.floor {
            None => {
                let s = lim.to_radians().sin() / self.to_host.to_radians().sin().max(1e-9);
                Some(if s >= 1.0 {
                    self.period / 2.0
                } else {
                    s.asin().to_degrees()
                })
            }
            Some(f) if f < lim => Some((lim * lim - f * f).sqrt()),
            Some(_) => None,
        }
    }
}

/// One end's angle out of its flange's plane before and after the turn.
#[derive(Debug, Clone, PartialEq)]
pub struct EndResidual {
    /// The joint.
    pub joint: String,
    /// The member end.
    pub member: String,
    /// The run's piece it is measured against.
    pub piece: String,
    /// Before, degrees.
    pub before: f64,
    /// After, degrees.
    pub after: f64,
}

/// What a run is, in the owner's order (legs, then chords, then the
/// bracing hosting other ends).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RunKind {
    /// A leg's (class `leg`).
    Leg,
    /// A chord's (an angle running on in line at a corner of an arm's or a
    /// body's section, not a leg).
    Chord,
    /// A face's member (bracing lying in a steep face).
    Bracing,
}

impl RunKind {
    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Leg => "leg",
            Self::Chord => "chord",
            Self::Bracing => "bracing",
        }
    }
}

/// A run's turn from its joints' planes.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneRoll {
    /// The run's pieces (in the members' order).
    pub members: Vec<String>,
    /// What it is.
    pub kind: RunKind,
    /// The turn added to every piece's roll, degrees (0: none).
    pub turn: f64,
    /// Every end it was solved over, before and after.
    pub ends: Vec<EndResidual>,
    /// The ends within the owner's 2° before and after.
    pub within: (usize, usize),
}

impl PlaneRoll {
    /// It turns the run.
    pub fn turns(&self) -> bool {
        self.turn.abs() >= MIN_TURN_DEG
    }

    /// The joints the run reaches.
    pub fn joints(&self) -> BTreeSet<String> {
        self.ends.iter().map(|e| e.joint.clone()).collect()
    }

    /// The largest residual after, degrees.
    pub fn worst(&self) -> f64 {
        self.ends.iter().map(|e| e.after).fold(0.0, f64::max)
    }

    /// In words: "leg run L1, L2 turned by −4.80° (from the joints'
    /// planes): 6 of 8 ends within 2° (before 2), the largest left 5.1°".
    pub fn words(&self) -> String {
        let what = self.kind.as_str();
        let n = self.ends.len();
        let (b, a) = self.within;
        if self.turns() {
            format!(
                "{what} run {} turned by {:+.2}° (from its joints' planes): {a} of {n} end(s) \
                 within {:.0}° of their flange's plane (before {b}), the largest left {:.1}°",
                self.members.join(", "),
                self.turn,
                crate::FLAT_OFF,
                self.worst()
            )
        } else {
            format!(
                "{what} run {}: as its joints' planes want it ({a} of {n} end(s) within {:.0}°)",
                self.members.join(", "),
                crate::FLAT_OFF
            )
        }
    }
}

/// `x` folded into (−p/2, p/2] degrees.
fn fold(x: f64, p: f64) -> f64 {
    let mut d = x % p;
    if d > p / 2.0 {
        d -= p;
    } else if d <= -p / 2.0 {
        d += p;
    }
    d
}

/// The turn of a run over `ends` within `window` degrees (see the module),
/// degrees, to a hundredth; 0 where nothing is better.
pub fn best_turn(ends: &[RollEnd], window: f64) -> f64 {
    if ends.is_empty() {
        return 0.0;
    }
    let lim = crate::FLAT_OFF + 1e-9;
    let count = |delta: f64| -> f64 {
        ends.iter()
            .filter(|e| e.off(delta) <= lim)
            .map(|e| e.weight)
            .sum()
    };
    let squares = |delta: f64| -> f64 {
        ends.iter()
            .map(|e| {
                let c = e.off(delta).min(OUT_CAP_DEG);
                e.weight * c * c
            })
            .sum()
    };
    let round = |x: f64| (x * 100.0).round() / 100.0;
    // The candidates: none, each end laid in, the edges of its band.
    let mut cands: Vec<f64> = vec![0.0];
    for e in ends {
        let t = fold(e.turn, e.period);
        for k in [-1.0, 0.0, 1.0] {
            let c = t + k * e.period;
            cands.push(c);
            if let Some(b) = e.band() {
                cands.push(c - b + 1e-6);
                cands.push(c + b - 1e-6);
            }
        }
    }
    cands.retain(|c| c.abs() <= window + 1e-9);
    let most = cands
        .iter()
        .map(|&c| count(c))
        .fold(f64::NEG_INFINITY, f64::max);
    // Each candidate laying the most within, centred in the turns that keep
    // those ends within (the middle of their bands' overlap: each end as far
    // inside its 2° as the others allow — a later small change of the roll,
    // a piece made one part with its neighbour, keeps them in); of those
    // the widest overlap, then the least squares of all, then the smallest.
    // (count, width, squares, |turn|, turn)
    let mut best: Option<(f64, f64, f64, f64, f64)> = None;
    for &c in &cands {
        if count(c) < most - 1e-9 {
            continue;
        }
        let within: Vec<&RollEnd> = ends.iter().filter(|e| e.off(c) <= lim).collect();
        let (mut lo, mut hi) = (-window, window);
        for e in &within {
            let t = c + fold(e.turn - c, e.period);
            let b = e.band().unwrap_or(0.0);
            lo = lo.max(t - b);
            hi = hi.min(t + b);
        }
        let mid = if lo <= hi {
            round(0.5 * (lo + hi))
        } else {
            round(c)
        };
        let m = if count(mid) >= most - 1e-9 {
            mid
        } else {
            round(c)
        };
        if count(m) < most - 1e-9 {
            continue;
        }
        let key = (count(m), (hi - lo).max(0.0), squares(m), m.abs(), m);
        let better = match best {
            None => true,
            Some(b) => {
                if (key.0 - b.0).abs() > 1e-9 {
                    key.0 > b.0
                } else if (key.1 - b.1).abs() > 1e-6 {
                    key.1 > b.1
                } else if (key.2 - b.2).abs() > 1e-9 {
                    key.2 < b.2
                } else {
                    key.3 < b.3 - 1e-12
                }
            }
        };
        if better {
            best = Some(key);
        }
    }
    let Some(b) = best else { return 0.0 };
    let t = b.4;
    if t.abs() < MIN_TURN_DEG { 0.0 } else { t }
}

/// An end's steel area as drawn, m² (its outlines').
fn area_of(a: &Arrival) -> f64 {
    let Some(p) = a.profile.as_ref() else {
        return 0.0;
    };
    p.pieces()
        .map(|q| {
            let mut s = 0.0;
            for i in 0..q.len() {
                let (u, v) = (q[i], q[(i + 1) % q.len()]);
                s += u[0] * v[1] - v[0] * u[1];
            }
            (s / 2.0).abs()
        })
        .sum()
}

/// A single angle (its flanges along its y and z).
fn single_angle(a: &Arrival) -> bool {
    a.profile
        .as_ref()
        .is_some_and(|p| p.angle.is_some() && p.parts.is_empty())
}

/// The runs of `pieces` (members) in `d`: the pieces in line through a
/// joint (within 10°) and turned alike (their y axes within the owner's 2°)
/// joined, each run in the members' order, the runs by their first piece.
pub fn runs_of(d: &JointDetails, pieces: &BTreeSet<String>) -> Vec<Vec<String>> {
    let mut parent: BTreeMap<String, String> =
        pieces.iter().map(|m| (m.clone(), m.clone())).collect();
    fn root(p: &mut BTreeMap<String, String>, m: &str) -> String {
        let mut k = m.to_owned();
        loop {
            let up = p[&k].clone();
            if up == k {
                return k;
            }
            let upup = p[&up].clone();
            p.insert(k.clone(), upup.clone());
            k = upup;
        }
    }
    let cos = IN_LINE_DEG.to_radians().cos();
    let alike = crate::FLAT_OFF.to_radians().cos();
    for j in &d.joints {
        let here: Vec<&Arrival> = j
            .arrivals
            .iter()
            .filter(|a| pieces.contains(&a.member) && single_angle(a))
            .collect();
        for (i, a) in here.iter().enumerate() {
            for b in here.iter().skip(i + 1) {
                if dot(a.direction, b.direction) > -cos {
                    continue;
                }
                let (Some(pa), Some(pb)) = (&a.profile, &b.profile) else {
                    continue;
                };
                if dot(pa.axes[1], pb.axes[1]) < alike {
                    continue;
                }
                let (x, y) = (root(&mut parent, &a.member), root(&mut parent, &b.member));
                if x != y {
                    let (lo, hi) = if x < y { (x, y) } else { (y, x) };
                    parent.insert(hi, lo);
                }
            }
        }
    }
    let mut runs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let keys: Vec<String> = parent.keys().cloned().collect();
    for m in keys {
        let r = root(&mut parent, &m);
        runs.entry(r).or_default().push(m);
    }
    runs.into_values().collect()
}

/// The ends run `run`'s turn is solved over in `d` (see the module).
pub fn run_ends(d: &JointDetails, run: &[String], weighting: Weighting) -> Vec<RollEnd> {
    let in_run = |m: &str| run.iter().any(|x| x == m);
    let mut out = Vec::new();
    for j in &d.joints {
        if !j.arrivals.iter().any(|a| in_run(&a.member)) {
            continue;
        }
        let f = crate::joint_classes::joint_features(j, crate::joint_order::Zone::Body);
        // Where a piece of the run is the host (or its run): every other
        // end against the host's piece it lies nearer to (the host and its
        // run may be pieces of two runs: a taper change), once per joint.
        // (A leg host: the other leg pieces here too — a taper change past
        // the in-line tolerance.)
        let leg_host = f
            .ends
            .iter()
            .any(|e| e.host && e.class == crate::joint_classes::MemberClass::Leg);
        let hosts: Vec<String> = f
            .ends
            .iter()
            .filter(|e| {
                e.host || (leg_host && e.angle && e.class == crate::joint_classes::MemberClass::Leg)
            })
            .map(|e| e.member.clone())
            .collect();
        if hosts.iter().any(|h| in_run(h)) {
            push_hosted(&mut out, j, &f, &hosts, &in_run, weighting);
            continue;
        }
        // Else each piece arriving onto another member: its own leg (not
        // a leg's piece onto a leg: a splice, not a flange bolted flat).
        let host_leg = f
            .ends
            .iter()
            .any(|e| e.host && e.class == crate::joint_classes::MemberClass::Leg);
        for a in j.arrivals.iter().filter(|a| in_run(&a.member)) {
            if host_leg && a.class.as_deref() == Some("leg") {
                continue;
            }
            let Some(fl) = a.flat else { continue };
            let t = fl.turn.to_degrees();
            let floor = (fl.off * fl.off - t * t).max(0.0).sqrt();
            out.push(RollEnd {
                joint: j.id.clone(),
                member: a.member.clone(),
                piece: a.member.clone(),
                turn: t,
                to_host: 90.0,
                floor: Some(floor),
                period: if either_leg(a) { 90.0 } else { 180.0 },
                // (Its own ends weigh as the ends it hosts: measured.)
                weight: OWN_WEIGHT
                    * match weighting {
                        Weighting::Count => 1.0,
                        Weighting::Area => area_of(a) * 1e4,
                    },
            });
        }
    }
    out
}

/// The ends hosted at joint `j` (features `f`) by the host's pieces
/// `hosts`: each end against the piece whose flange's plane it lies nearer
/// to as they lie, kept where that piece is `in_run`.
fn push_hosted(
    out: &mut Vec<RollEnd>,
    j: &JointDetail,
    f: &crate::joint_classes::JointFeatures,
    hosts: &[String],
    in_run: &dyn Fn(&str) -> bool,
    weighting: Weighting,
) {
    let frames: Vec<(&str, [[f64; 3]; 3])> = hosts
        .iter()
        .filter_map(|hp| {
            let h = j.arrivals.iter().find(|a| &a.member == hp)?;
            let p = h.profile.as_ref()?;
            Some((hp.as_str(), [h.direction, p.axes[1], p.axes[2]]))
        })
        .collect();
    // (Another leg's piece arriving — a leg's taper change, an extension —
    // is spliced, not bolted flat by a flange: no plane.)
    for e in f
        .ends
        .iter()
        .filter(|e| !e.host && e.class != crate::joint_classes::MemberClass::Leg)
    {
        let best = frames
            .iter()
            .filter_map(|(hp, axes)| {
                let (psi, to) = crate::joint_planes::plane_angle(*axes, e.direction)?;
                let (flange, turn) = crate::joint_planes::nearer_flange(psi);
                let off = crate::joint_planes::off_flange(psi, to, flange, 0.0);
                Some((*hp, turn, to, off))
            })
            .min_by(|a, b| a.3.total_cmp(&b.3));
        let Some((hp, turn, to, _)) = best else {
            continue;
        };
        if !in_run(hp) {
            continue;
        }
        out.push(RollEnd {
            joint: j.id.clone(),
            member: e.member.clone(),
            piece: hp.to_owned(),
            turn,
            to_host: to,
            floor: None,
            period: 90.0,
            weight: match weighting {
                Weighting::Count => 1.0,
                Weighting::Area => e.area * 1e4,
            },
        });
    }
}

fn either_leg(a: &Arrival) -> bool {
    a.other_onto.is_none()
        && a.leg.value != Some(ohl_structures::connections::BoltedLeg::Both)
        && a.profile
            .as_ref()
            .and_then(|p| p.angle)
            .is_some_and(|g| (g.b - g.h).abs() <= 1e-9)
}

/// Each run of `runs` (of kind `kind`) turned from its joints' planes in
/// `d` (see the module).
pub fn plane_rolls(
    d: &JointDetails,
    runs: &[Vec<String>],
    kind: RunKind,
    weighting: Weighting,
) -> Vec<PlaneRoll> {
    runs.iter()
        .map(|run| {
            let ends = run_ends(d, run, weighting);
            let turn = best_turn(&ends, ROLL_WINDOW_DEG);
            let lim = crate::FLAT_OFF + 1e-9;
            let res: Vec<EndResidual> = ends
                .iter()
                .map(|e| EndResidual {
                    joint: e.joint.clone(),
                    member: e.member.clone(),
                    piece: e.piece.clone(),
                    before: round3(e.off(0.0)),
                    after: round3(e.off(turn)),
                })
                .collect();
            let within = (
                ends.iter().filter(|e| e.off(0.0) <= lim).count(),
                ends.iter().filter(|e| e.off(turn) <= lim).count(),
            );
            PlaneRoll {
                members: run.clone(),
                kind,
                turn,
                ends: res,
                within,
            }
        })
        .collect()
}

/// The runs of the structure file `text` turned from their joints' planes
/// (see the module), set by set in the order of `sets` (the owner's: legs,
/// then chords, then bracing), each set's runs solved on the joints as they
/// are then (the sets before turned). The file after (each piece's roll
/// plus its run's turn, to a hundredth) and the runs in that order.
pub fn turn_runs(
    text: &str,
    library: &ohl_sections::SectionLibrary,
    sets: &[(RunKind, BTreeSet<String>)],
    weighting: Weighting,
) -> Result<(String, Vec<PlaneRoll>), String> {
    let bolts = ohl_structures::bolts::BoltLibrary::default();
    let mut text = text.to_owned();
    let mut out = Vec::new();
    for (kind, set) in sets {
        if set.is_empty() {
            continue;
        }
        let d = crate::joint_details(&text, library, &bolts)?;
        let runs = runs_of(&d, set);
        let rolls = plane_rolls(&d, &runs, *kind, weighting);
        if rolls.iter().any(PlaneRoll::turns) {
            text = with_turns(&text, rolls.iter().filter(|r| r.turns()))?;
        }
        out.extend(rolls);
    }
    Ok((text, out))
}

/// The leg's and chord's runs of `d` (the whole tower in detail) with a
/// piece at a joint of `at` (none: every run), each turned from its
/// joints' planes on the joints as they are (legs and chords on the same
/// details: proposals, each measured on its own) — only those that turn,
/// legs first. The legs: single angles of class `leg`; the chords: the
/// pieces running on through a joint as its host that are not legs
/// ([`crate::joint_classes::chord_pieces`]).
pub fn proposed_runs(d: &JointDetails, at: Option<&BTreeSet<String>>) -> Vec<PlaneRoll> {
    let mut legs: BTreeSet<String> = BTreeSet::new();
    let mut chords: BTreeSet<String> = BTreeSet::new();
    for j in &d.joints {
        for a in &j.arrivals {
            if a.class.as_deref() == Some("leg") && single_angle(a) {
                legs.insert(a.member.clone());
            }
        }
        for i in crate::joint_classes::chord_pieces(j) {
            if let Some(a) = j.arrivals.get(i) {
                chords.insert(a.member.clone());
            }
        }
    }
    chords.retain(|m| !legs.contains(m));
    let here = |run: &Vec<String>| {
        at.is_none_or(|at| {
            d.joints
                .iter()
                .filter(|j| at.contains(&j.id))
                .any(|j| j.arrivals.iter().any(|a| run.contains(&a.member)))
        })
    };
    let mut out = Vec::new();
    for (kind, set) in [(RunKind::Leg, legs), (RunKind::Chord, chords)] {
        let runs: Vec<Vec<String>> = runs_of(d, &set).into_iter().filter(here).collect();
        out.extend(
            plane_rolls(d, &runs, kind, Weighting::Count)
                .into_iter()
                .filter(PlaneRoll::turns),
        );
    }
    out
}

/// `text` with each run of `rolls` turned (every piece's roll plus the
/// run's turn, to a hundredth of a degree).
pub fn with_turns<'a>(
    text: &str,
    rolls: impl Iterator<Item = &'a PlaneRoll>,
) -> Result<String, String> {
    let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("structure.ohls"),
        text,
    )
    .map_err(|e| e.message)?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    for r in rolls {
        for m in &r.members {
            let now = t
                .members
                .iter()
                .find(|x| &x.id == m)
                .map_or(0.0, ohl_structure_edit::edits::member_roll);
            ohl_structure_edit::edits::set_member_roll(&mut doc, m, now + r.turn.to_radians());
        }
    }
    Ok(doc.to_string())
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
