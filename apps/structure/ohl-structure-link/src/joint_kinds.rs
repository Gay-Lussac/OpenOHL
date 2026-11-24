// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Joints of one kind** (the automatic joint layout's J4, owner
//! 2026-11-19: "lay out once, apply to the joints like it"): a lattice
//! tower repeats its joints — its symmetry (mirror planes, quarter turns
//! about its axis) and its repeated panels. Two joints are of one kind
//! when one is the other moved, mirrored or turned: a **rigid map** of the
//! eight that keep the vertical (the identity, the quarter, half and
//! three-quarter turns about the vertical, the mirrors in the two vertical
//! planes of the axes and in the two diagonal ones), its translation from
//! one joint to the other, carrying every member end of the one onto a
//! member end of the other.
//!
//! The match is **geometric, with a tolerance** ([`DIR_TOL`],
//! [`LEN_TOL`]): per member end, its direction, its length, its
//! cross-section and how it is turned (the section's outline where it
//! is), its class; in the **strict** form (the layout's run) also what
//! the planner reads of it — its bolts, its leg bolted, what it bolts onto
//! (the matched member), where it sits, running on as one part, the cut
//! and shift given, the bolts where they are — and no plate or splice at
//! either joint. The tolerances are the tool's, not a standard's: a
//! millimetre or two on a member's place, a tenth of a degree on its
//! direction. Deterministic: the joints in the file's order, the first of
//! a kind its representative, the maps tried in a fixed order.
//!
//! What uses it: the layout run plans one joint of each kind and applies
//! that plan, mapped, to the others (`layout_run`; each copy then
//! checked for clashes in its own place, a copy that fails planned on its
//! own); **Apply this joint's detail to similar joints**
//! ([`crate::apply_detail()`]) copies a joint's connections and plates, mapped, to
//! the joints of its kind — a hand detail too.

use std::collections::{BTreeMap, BTreeSet};

use crate::joints::{Arrival, JointDetail, JointDetails};

/// Two directions match within this, as the length of their difference
/// (unit vectors: about a tenth of a degree; the tool's tolerance).
pub const DIR_TOL: f64 = 0.002;

/// Two lengths or places match within this, m (the tool's tolerance: the
/// drawings' rounding).
pub const LEN_TOL: f64 = 0.002;

/// A rigid map keeping the vertical: a 3 × 3 matrix (rows) and its name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Map {
    /// The matrix (applied as `m · v`).
    pub m: [[f64; 3]; 3],
    /// Mirrors (its determinant −1).
    pub mirrors: bool,
    /// In words.
    pub name: &'static str,
}

impl Map {
    /// `v` mapped.
    pub fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        let m = &self.m;
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    }
}

/// The eight maps keeping the vertical, the identity first.
pub fn maps() -> [Map; 8] {
    let map = |m: [[f64; 3]; 3], mirrors: bool, name: &'static str| Map { m, mirrors, name };
    [
        map(
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            false,
            "the same, moved",
        ),
        map(
            [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            true,
            "mirrored across x",
        ),
        map(
            [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
            true,
            "mirrored across y",
        ),
        map(
            [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
            false,
            "turned a half turn",
        ),
        map(
            [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            false,
            "turned a quarter turn",
        ),
        map(
            [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            false,
            "turned three quarters",
        ),
        map(
            [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            true,
            "mirrored across a diagonal",
        ),
        map(
            [[0.0, -1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            true,
            "mirrored across the other diagonal",
        ),
    ]
}

/// A joint of the kind of another: the map from that one to it and its
/// members matched.
#[derive(Debug, Clone, PartialEq)]
pub struct Like {
    /// The joint.
    pub joint: String,
    /// The joint it is like (the representative).
    pub of: String,
    /// The map carrying `of` onto it (about `of`'s position, then moved to
    /// the joint's).
    pub map: Map,
    /// From `of`'s position to the joint's.
    pub from: [f64; 3],
    /// To.
    pub to: [f64; 3],
    /// Each member end of `of` and its match here: (`of`'s member, this
    /// joint's member).
    pub pairs: Vec<(String, String)>,
}

impl Like {
    /// A point of `of` mapped here.
    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        let d = self.map.apply(sub(p, self.from));
        add(self.to, d)
    }

    /// A direction of `of` mapped here.
    pub fn dir(&self, v: [f64; 3]) -> [f64; 3] {
        self.map.apply(v)
    }

    /// `of`'s member `m` here (none: not one of its ends).
    pub fn member(&self, m: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(a, _)| a == m)
            .map(|(_, b)| b.as_str())
    }

    /// A text of `of` with its members' and its own id said as here's:
    /// each word that is one of them replaced (words: runs of letters,
    /// digits and `_-.`, a full stop at the end left).
    pub fn words(&self, text: &str) -> String {
        let is_word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.');
        let mut out = String::with_capacity(text.len());
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut String| {
            if word.is_empty() {
                return;
            }
            let (core, tail) = match word.strip_suffix('.') {
                Some(c) if self.swap(c).is_some() => (c.to_owned(), "."),
                _ => (word.clone(), ""),
            };
            match self.swap(&core) {
                Some(x) => {
                    out.push_str(x);
                    out.push_str(tail);
                }
                None => out.push_str(word),
            }
            word.clear();
        };
        for c in text.chars() {
            if is_word(c) {
                word.push(c);
            } else {
                flush(&mut word, &mut out);
                out.push(c);
            }
        }
        flush(&mut word, &mut out);
        out
    }

    fn swap(&self, w: &str) -> Option<&str> {
        if w == self.of {
            return Some(&self.joint);
        }
        self.member(w)
    }
}

/// The ends of a joint as matched: the member, and the key parts that
/// must be equal.
fn exact_key(a: &Arrival, strict: bool) -> String {
    let p = a.profile.as_ref();
    let mut k = format!(
        "{:?}|{:?}|{}",
        p.map(|p| p.section_id.as_str()),
        a.class,
        a.modelling
    );
    if strict {
        let b = a.bolts.value.as_ref();
        k.push_str(&format!(
            "|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}",
            a.kind,
            a.leg,
            b.map(|b| (b.count, (b.diameter * 1e4).round() as i64)),
            b.map(|b| (b.grade.clone(), b.bolt.clone())),
            a.bolts.source,
            a.sits,
            a.gauge_lines,
            a.gauges,
            a.pitch,
            a.end_distance,
            a.cut.source,
            a.continuous.is_some(),
            a.other_onto.is_some()
        ));
    }
    k
}

/// `a` of one joint and `b` of another matched under `map` (the joints'
/// positions apart): the same section, turned alike, the same direction
/// and length; strict: the planner's inputs too (all but what they bolt
/// onto, matched after).
fn ends_match(a: &Arrival, b: &Arrival, map: &Map, strict: bool) -> bool {
    if exact_key(a, strict) != exact_key(b, strict) {
        return false;
    }
    if norm(sub(map.apply(a.direction), b.direction)) > DIR_TOL
        || (a.length - b.length).abs() > LEN_TOL
    {
        return false;
    }
    // Turned alike: the outline where it is (a set of points: a mirror
    // reverses its order).
    match (a.profile.as_ref(), b.profile.as_ref()) {
        (Some(p), Some(q)) => {
            let at = |p: &crate::joints::Profile, yz: [f64; 2]| {
                add(scale(p.axes[1], yz[0]), scale(p.axes[2], yz[1]))
            };
            let mine: Vec<[f64; 3]> = p.outline.iter().map(|x| map.apply(at(p, *x))).collect();
            let theirs: Vec<[f64; 3]> = q.outline.iter().map(|x| at(q, *x)).collect();
            if mine.len() != theirs.len()
                || !mine
                    .iter()
                    .all(|x| theirs.iter().any(|y| norm(sub(*x, *y)) <= LEN_TOL))
            {
                return false;
            }
        }
        (None, None) => {}
        _ => return false,
    }
    if !strict {
        return true;
    }
    let close = |x: f64, y: f64| (x - y).abs() <= LEN_TOL;
    if norm(sub(map.apply(a.offset), b.offset)) > LEN_TOL
        || !close(a.shift.0, b.shift.0)
        || a.cut.value.is_some() != b.cut.value.is_some()
        || !close(a.cut.value.unwrap_or(0.0), b.cut.value.unwrap_or(0.0))
        || a.bolt_flat.len() != b.bolt_flat.len()
    {
        return false;
    }
    let mut fa: Vec<[f64; 2]> = a.bolt_flat.clone();
    let mut fb: Vec<[f64; 2]> = b.bolt_flat.clone();
    let order = |x: &[f64; 2], y: &[f64; 2]| x[0].total_cmp(&y[0]).then(x[1].total_cmp(&y[1]));
    fa.sort_by(order);
    fb.sort_by(order);
    fa.iter()
        .zip(&fb)
        .all(|(x, y)| close(x[0], y[0]) && close(x[1], y[1]))
}

/// Joint `b` is like joint `a` under one of the maps (the first that
/// carries every member end of `a` onto one of `b`): the match; none: not
/// of one kind. Strict: as the layout plans (see the module).
pub fn like(a: &JointDetail, b: &JointDetail, strict: bool) -> Option<Like> {
    if a.arrivals.len() != b.arrivals.len() || a.arrivals.is_empty() {
        return None;
    }
    if strict
        && (!a.plates.is_empty()
            || !b.plates.is_empty()
            || !a.splices.is_empty()
            || !b.splices.is_empty())
    {
        return None;
    }
    'maps: for map in maps() {
        let mut used = vec![false; b.arrivals.len()];
        let mut pairs = Vec::with_capacity(a.arrivals.len());
        for x in &a.arrivals {
            let Some(k) = (0..b.arrivals.len())
                .find(|&k| !used[k] && ends_match(x, &b.arrivals[k], &map, strict))
            else {
                continue 'maps;
            };
            used[k] = true;
            pairs.push((x.member.clone(), b.arrivals[k].member.clone()));
        }
        // What they bolt onto and run on into, matched alike.
        if strict {
            let pair = |m: &str| pairs.iter().find(|(x, _)| x == m).map(|(_, y)| y.clone());
            for x in &a.arrivals {
                let y = b
                    .arrivals
                    .iter()
                    .find(|y| Some(&y.member) == pair(&x.member).as_ref())
                    .expect("paired");
                let onto = x.onto.value.as_deref().map(|o| pair(o).ok_or(o));
                let same_onto = match (onto, y.onto.value.as_deref()) {
                    (None, None) => true,
                    (Some(Ok(o)), Some(p)) => o == p && x.onto.source == y.onto.source,
                    _ => false,
                };
                let run = x.continuous.as_deref().map(pair);
                let same_run = match (run, y.continuous.as_deref()) {
                    (None, None) => true,
                    (Some(Some(o)), Some(p)) => o == p,
                    _ => false,
                };
                if !same_onto || !same_run {
                    continue 'maps;
                }
            }
        }
        return Some(Like {
            joint: b.id.clone(),
            of: a.id.clone(),
            map,
            from: a.position,
            to: b.position,
            pairs,
        });
    }
    None
}

/// A rough key: joints whose keys differ are of no one kind (the number
/// of ends and their sections and classes).
fn bucket(j: &JointDetail, strict: bool) -> String {
    let mut k: Vec<String> = j.arrivals.iter().map(|a| exact_key(a, strict)).collect();
    k.sort();
    format!(
        "{}|{}|{}",
        k.join(";"),
        j.plates.is_empty(),
        j.splices.is_empty()
    )
}

/// The joints `among` (in the file's order) grouped by kind: for each
/// joint like an earlier one, the match against the first of its kind
/// (the representative); the representatives are not listed. Strict: as
/// the layout plans.
pub fn kinds(d: &JointDetails, among: &BTreeSet<String>, strict: bool) -> BTreeMap<String, Like> {
    kinds_in(d.joints.iter().filter(|j| among.contains(&j.id)), strict)
}

/// [`kinds`] of `joints` in the order given (the first of a kind its
/// representative; L1 a: the layout's order, bottom up, then out).
pub fn kinds_in<'a>(
    joints: impl IntoIterator<Item = &'a JointDetail>,
    strict: bool,
) -> BTreeMap<String, Like> {
    let mut reps: BTreeMap<String, Vec<&JointDetail>> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for j in joints {
        if j.arrivals.is_empty() {
            continue;
        }
        let key = bucket(j, strict);
        let list = reps.entry(key).or_default();
        match list.iter().find_map(|r| like(r, j, strict)) {
            Some(l) => {
                out.insert(j.id.clone(), l);
            }
            None => list.push(j),
        }
    }
    out
}

/// The joints like joint `id` (not strict: its members alike, whatever
/// their connections and plates), each with its match; in the file's
/// order, `id` left out.
pub fn like_joint(d: &JointDetails, id: &str) -> Vec<Like> {
    let Some(a) = d.joint(id) else {
        return Vec::new();
    };
    d.joints
        .iter()
        .filter(|j| j.id != id)
        .filter_map(|j| like(a, j, false))
        .collect()
}

/// The plan of joint `l.of` as the plan of joint `l.joint` (J4: laid out
/// once, applied to the joints like it): its members, points and
/// directions mapped, its lengths as they are (a cut, a shift towards what
/// the end bolts onto, a gauge's distance: the same on a mirrored or
/// turned joint), its words said with this joint's members. The joins are
/// the caller's (this joint's own).
pub fn map_plan(p: &crate::joint_layout::JointPlan, l: &Like) -> crate::joint_layout::JointPlan {
    use crate::joint_layout::{EndPlan, JointPlan, Onto, PlatePlan};
    let m = |x: &str| l.member(x).unwrap_or(x).to_owned();
    let pt = |x: &[f64; 3]| l.point(*x);
    let pts = |v: &[[f64; 3]]| v.iter().map(pt).collect::<Vec<_>>();
    JointPlan {
        joint: l.joint.clone(),
        joins: Vec::new(),
        ends: p
            .ends
            .iter()
            .map(|e| EndPlan {
                member: m(&e.member),
                onto: match &e.onto {
                    Onto::Member(h) => Onto::Member(m(h)),
                    Onto::Plate(k) => Onto::Plate(*k),
                },
                ..e.clone()
            })
            .collect(),
        plates: p
            .plates
            .iter()
            .map(|x| PlatePlan {
                members: x.members.iter().map(|y| m(y)).collect(),
                thickness: x.thickness,
                on: m(&x.on),
                heel: x.heel.map(|(a, d)| (pt(&a), l.dir(d))),
                own: x.own.as_ref().map(|o| {
                    let mut o = o.clone();
                    o.outline = pts(&o.outline);
                    if let Some(b) = o.bend.as_mut() {
                        b.line = [pt(&b.line[0]), pt(&b.line[1])];
                    }
                    o
                }),
                reach: x.reach.as_ref().map(|r| crate::joint_layout::Reach {
                    points: pts(&r.points),
                    normal: l.dir(r.normal),
                    keep_to: r.keep_to.iter().map(|(a, d)| (pt(a), l.dir(*d))).collect(),
                }),
                outline: x.outline.as_ref().map(|o| pts(o)),
                short: x.short,
                cuts: x.cuts.iter().map(|(a, d)| (pt(a), l.dir(*d))).collect(),
                between: x.between,
                hold: x.hold.as_ref().map(|h| {
                    let mut h = h.clone();
                    h.member = m(&h.member);
                    h
                }),
                also: x.also.as_ref().map(|h| {
                    let mut h = h.clone();
                    h.member = m(&h.member);
                    h
                }),
            })
            .collect(),
        hand: p.hand.iter().map(|h| l.words(h)).collect(),
        hand_members: p.hand_members.iter().map(|y| m(y)).collect(),
        kept_free: p.kept_free.iter().map(|y| m(y)).collect(),
        left: p.left.iter().map(|h| l.words(h)).collect(),
        notes: p.notes.iter().map(|h| l.words(h)).collect(),
        off_flange: p.off_flange.iter().map(|(y, v)| (m(y), *v)).collect(),
        hand_hosts: p.hand_hosts.iter().map(|(y, z)| (m(y), m(z))).collect(),
        like: Some(l.of.clone()),
    }
}

/// (L7, 2026-11-20) The leg each plate's own bolts name (`long` or
/// `short`) in plan `p` mapped from joint `of` here (`here`, the joint
/// [`map_plan`] mapped it to): the leg of this joint's member that the map
/// carries the representative's leg onto — under a mirror an angle's leg
/// along its y can be the matched member's leg along its z (before, the
/// name was copied, and the bolts were drawn on the other leg, off the
/// plate).
pub fn map_hold_legs(
    p: &mut crate::joint_layout::JointPlan,
    l: &Like,
    of: &JointDetail,
    here: &JointDetail,
) {
    let leg_of = |a: &Arrival, name: &str| -> Option<[f64; 3]> {
        let q = a.profile.as_ref()?;
        let g = q.angle?;
        let along_y = (name == "long") == (g.b >= g.h);
        Some(if along_y { q.axes[1] } else { q.axes[2] })
    };
    for h in p
        .plates
        .iter_mut()
        .flat_map(|x| x.hold.iter_mut().chain(x.also.iter_mut()))
    {
        let Some(name) = h.leg else {
            continue;
        };
        let src = l
            .pairs
            .iter()
            .find(|(_, mine)| *mine == h.member)
            .map(|(theirs, _)| theirs.as_str());
        let (Some(a), Some(b)) = (
            src.and_then(|m| of.arrivals.iter().find(|a| a.member == m)),
            here.arrivals.iter().find(|a| a.member == h.member),
        ) else {
            continue;
        };
        let (Some(v), Some(q)) = (leg_of(a, name), b.profile.as_ref()) else {
            continue;
        };
        let Some(g) = q.angle else {
            continue;
        };
        let v = l.dir(v);
        let dot = |u: [f64; 3], w: [f64; 3]| u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
        let along_y = dot(v, q.axes[1]).abs() >= dot(v, q.axes[2]).abs();
        h.leg = Some(if along_y == (g.b >= g.h) {
            "long"
        } else {
            "short"
        });
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
