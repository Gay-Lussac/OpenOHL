// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **What braces a member** (stage 4.3 of the joint details, 2026-11-19;
//! built, in active development, not approved yet): the geometry a design
//! code's buckling lengths are worked out from, without any code's rule —
//! the code plugins apply their own ([`crate::check`] sends it with the
//! members, the `member-check-bracing` interface).
//!
//! - **The run.** A member and the members continuing it in line through
//!   its joints (two design-checked members whose directions from a joint
//!   are opposite within [`IN_LINE_DEG`], each the other's most nearly
//!   opposite): a leg split at every panel joint, a diagonal split at its
//!   crossing. Its buckling lengths are measured along the run, from
//!   bracing point to bracing point.
//! - **The bracing points.** Every joint of the run (its two ends
//!   included) with what holds it there: each other member ending or
//!   passing there — design-checked, not a cable, more than
//!   [`IN_LINE_DEG`] off the run's line — holds it along that member's
//!   direction square to the run (its axial stiffness; a tension-only
//!   member only where it passes through, a crossing); a support fixed in
//!   all three translations holds it in every direction, one fixed in some
//!   along each of those axes (springs not counted). A member's
//!   **crossing partner** (`crossing`) that crosses the run away from a
//!   joint gives a point where it crosses (the two lines within
//!   [`CROSSING_GAP`] of the shorter member's length), held along the
//!   partner. A member counts only where it is connected in the model:
//!   at a joint the analysis joins it there; a partner only through its
//!   given `crossing`.
//! - Only geometry: which point holds which buckling direction, whether a
//!   crossing's other diagonal holds it out of its plane, and whether a
//!   bracing member is strong enough are each code's rules (its plugin).

use crate::model::{Model, dot, norm, sub};

/// Two members continue one line through a joint when their directions
/// from it are opposite within this angle, degrees (as the redundant
/// members' braced member, [`crate::check::STRAIGHT_THROUGH_DEG`]: a
/// leg's slope changing by a few degrees at a section joint is still the
/// leg); a member at a joint holds the run only when more than this off
/// its line.
pub const IN_LINE_DEG: f64 = crate::check::STRAIGHT_THROUGH_DEG;

/// A crossing partner crosses the run away from a joint when the two
/// lines come within this share of the shorter member's length.
pub const CROSSING_GAP: f64 = 0.05;

/// One member (or a support's fixed direction) holding a bracing point.
#[derive(Debug, Clone, PartialEq)]
pub struct Restraint {
    /// The member (index; none: a support fixed along `direction`).
    pub member: Option<usize>,
    /// Along it from the point, square to the run, structure axes (unit).
    pub direction: [f64; 3],
    /// It passes through the point (a crossing: its pieces on both sides
    /// in line, or a crossing partner away from a joint).
    pub through: bool,
}

/// A bracing point along a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    /// From the run's start, m.
    pub at: f64,
    /// The joint there (none: a crossing away from a joint).
    pub joint: Option<usize>,
    /// A support fixed in all three translations.
    pub support: bool,
    /// What holds it.
    pub restraints: Vec<Restraint>,
}

/// A run of members in line and its bracing points.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// Its members, in order (indices).
    pub members: Vec<usize>,
    /// Where each lies along the run (from, to), m.
    pub spans: Vec<(f64, f64)>,
    /// Each one's first joint is at its `from` (else its second).
    pub forward: Vec<bool>,
    /// Its length, m.
    pub length: f64,
    /// Its joints from start to end (each a point, the ends included) and
    /// its crossings away from joints, by `at`.
    pub points: Vec<Point>,
}

/// The runs of a model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bracing {
    /// The runs.
    pub runs: Vec<Run>,
    /// Per member, its run (index; none: not design-checked, a cable, or
    /// of no length).
    pub run_of: Vec<Option<usize>>,
}

impl Bracing {
    /// The run of member `k` and where `k` lies along it.
    pub fn of(&self, k: usize) -> Option<(&Run, (f64, f64))> {
        let r = &self.runs[(*self.run_of.get(k)?)?];
        let i = r.members.iter().position(|&m| m == k)?;
        Some((r, r.spans[i]))
    }
}

/// Whether member `m` gives its own buckling lengths (`lk`, a `k` other
/// than 1, or `length_ratios`): they win over the bracing (`k = 1` is the
/// default and cannot be told from one not written).
pub fn lengths_given(m: &crate::Member) -> bool {
    m.lk.is_some() || m.k != 1.0 || m.design.length_ratios.is_some()
}

/// Whether member `k` takes part (a run of its own, a restraint).
fn counted(model: &Model, k: usize) -> bool {
    let m = &model.members[k];
    m.check && m.kind != crate::MemberKind::Cable && model.length(m) > 0.0
}

/// The unit direction of member `k` from its joint `from`.
fn direction(model: &Model, k: usize, from: usize) -> [f64; 3] {
    let m = &model.members[k];
    let other = if m.i == from { m.j } else { m.i };
    let d = sub(model.nodes[other].point, model.nodes[from].point);
    let l = norm(d).max(1e-12);
    d.map(|x| x / l)
}

/// `d` without its part along unit `axis`, unit (none: along it).
fn square_to(d: [f64; 3], axis: [f64; 3]) -> Option<[f64; 3]> {
    let a = dot(d, axis);
    let p = [d[0] - a * axis[0], d[1] - a * axis[1], d[2] - a * axis[2]];
    let l = norm(p);
    (l > 1e-9).then(|| p.map(|x| x / l))
}

/// The runs and bracing points of `model`.
pub fn bracing(model: &Model) -> Bracing {
    let n = model.members.len();
    let mut at: Vec<Vec<usize>> = vec![Vec::new(); model.nodes.len()];
    for (k, m) in model.members.iter().enumerate() {
        if m.i != m.j {
            at[m.i].push(k);
            at[m.j].push(k);
        }
    }
    let in_line = -(IN_LINE_DEG.to_radians().cos());
    // The member continuing `k` through joint `j` (each the other's most
    // nearly opposite, within IN_LINE_DEG).
    let best = |k: usize, j: usize| -> Option<usize> {
        let dk = direction(model, k, j);
        at[j]
            .iter()
            .copied()
            .filter(|&o| o != k && counted(model, o))
            .map(|o| (o, dot(dk, direction(model, o, j))))
            .filter(|&(_, c)| c < in_line)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(o, _)| o)
    };
    let next = |k: usize, j: usize| -> Option<usize> {
        let o = best(k, j)?;
        (best(o, j) == Some(k)).then_some(o)
    };
    let other_end = |k: usize, j: usize| {
        let m = &model.members[k];
        if m.i == j { m.j } else { m.i }
    };
    let mut out = Bracing {
        runs: Vec::new(),
        run_of: vec![None; n],
    };
    for start in 0..n {
        if out.run_of[start].is_some() || !counted(model, start) {
            continue;
        }
        // Back to the run's first member, then forwards.
        let (mut k, mut j) = (start, model.members[start].i);
        let mut seen = vec![start];
        while let Some(o) = next(k, j) {
            if seen.contains(&o) {
                break;
            }
            seen.push(o);
            j = other_end(o, j);
            k = o;
        }
        // `k` is the first member, `j` its free end (the run's start).
        let mut members = vec![k];
        let mut joints = vec![j];
        let (mut cur, mut from) = (k, j);
        loop {
            let to = other_end(cur, from);
            joints.push(to);
            match next(cur, to) {
                Some(o) if !members.contains(&o) => {
                    members.push(o);
                    cur = o;
                    from = to;
                }
                _ => break,
            }
        }
        let mut spans = Vec::with_capacity(members.len());
        let mut s = 0.0;
        for &m in &members {
            let l = model.length(&model.members[m]);
            spans.push((s, s + l));
            s += l;
        }
        let r = out.runs.len();
        for &m in &members {
            out.run_of[m] = Some(r);
        }
        let forward = members
            .iter()
            .zip(&joints)
            .map(|(&m, &j)| model.members[m].i == j)
            .collect();
        out.runs.push(Run {
            members,
            spans,
            forward,
            length: s,
            points: Vec::new(),
        });
        let run = out.runs.last_mut().unwrap();
        // The joints' points.
        for (p, &node) in joints.iter().enumerate() {
            let at_ = if p == 0 { 0.0 } else { run.spans[p - 1].1 };
            // The run's direction there (the members' on either side).
            let mut axis = [0.0; 3];
            if p > 0 {
                let d = direction(model, run.members[p - 1], node);
                axis = [axis[0] - d[0], axis[1] - d[1], axis[2] - d[2]];
            }
            if p < run.members.len() {
                let d = direction(model, run.members[p], node);
                axis = [axis[0] + d[0], axis[1] + d[1], axis[2] + d[2]];
            }
            let l = norm(axis).max(1e-12);
            let axis = axis.map(|x| x / l);
            let own: Vec<usize> = [p.checked_sub(1), Some(p)]
                .into_iter()
                .flatten()
                .filter_map(|i| run.members.get(i).copied())
                .collect();
            let off_line = IN_LINE_DEG.to_radians().cos();
            let mut restraints = Vec::new();
            for &o in &at[node] {
                if own.contains(&o) || !model.members[o].check {
                    continue;
                }
                let mo = &model.members[o];
                if mo.kind == crate::MemberKind::Cable || model.length(mo) <= 0.0 {
                    continue;
                }
                let d = direction(model, o, node);
                if dot(d, axis).abs() >= off_line {
                    continue;
                }
                // It passes through: a member at the joint in line with it.
                let through = at[node].iter().any(|&q| {
                    q != o
                        && !own.contains(&q)
                        && counted(model, q)
                        && dot(d, direction(model, q, node)) < in_line
                });
                if mo.kind == crate::MemberKind::Tension && !through {
                    continue;
                }
                if let Some(dir) = square_to(d, axis) {
                    restraints.push(Restraint {
                        member: Some(o),
                        direction: dir,
                        through,
                    });
                }
            }
            // A support: every way when fixed in all three translations,
            // else along each fixed axis more than IN_LINE_DEG off the run.
            let f = model.nodes[node].fixed;
            let support = f[0] && f[1] && f[2];
            if !support {
                for (i, _) in f.iter().take(3).enumerate().filter(|(_, x)| **x) {
                    let mut e = [0.0; 3];
                    e[i] = 1.0;
                    if dot(e, axis).abs() < off_line
                        && let Some(dir) = square_to(e, axis)
                    {
                        restraints.push(Restraint {
                            member: None,
                            direction: dir,
                            through: false,
                        });
                    }
                }
            }
            run.points.push(Point {
                at: at_,
                joint: Some(node),
                support,
                restraints,
            });
        }
    }
    // Crossing partners crossing a run away from its joints.
    for k in 0..n {
        let Some(p) = model.members[k].design.crossing else {
            continue;
        };
        for (a, b) in [(k, p), (p, k)] {
            add_crossing(model, &mut out, a, b);
        }
    }
    for r in &mut out.runs {
        r.points.sort_by(|a, b| a.at.total_cmp(&b.at));
    }
    out
}

/// A point where member `b` (a crossing partner) crosses member `a`'s run
/// away from a joint, held along `b` (passing through).
fn add_crossing(model: &Model, out: &mut Bracing, a: usize, b: usize) {
    if b >= model.members.len() || !counted(model, b) {
        return;
    }
    let Some(r) = out.run_of.get(a).copied().flatten() else {
        return;
    };
    let ma = &model.members[a];
    let mb = &model.members[b];
    let (pa, qa) = (model.nodes[ma.i].point, model.nodes[ma.j].point);
    let (pb, qb) = (model.nodes[mb.i].point, model.nodes[mb.j].point);
    let (u, v) = (sub(qa, pa), sub(qb, pb));
    let (la, lb) = (norm(u), norm(v));
    if la <= 0.0 || lb <= 0.0 {
        return;
    }
    // Closest points of the two segments' lines.
    let w = sub(pa, pb);
    let (uu, uv, vv, uw, vw) = (dot(u, u), dot(u, v), dot(v, v), dot(u, w), dot(v, w));
    let det = uu * vv - uv * uv;
    if det <= 1e-12 * uu * vv {
        return; // parallel
    }
    let s = (uv * vw - vv * uw) / det;
    let t = (uu * vw - uv * uw) / det;
    // Strictly inside both (a joint at an end is a joint's point).
    let inside = |x: f64, l: f64| x * l > 1e-3 && (1.0 - x) * l > 1e-3;
    if !inside(s, la) || !inside(t, lb) {
        return;
    }
    let ca = [pa[0] + s * u[0], pa[1] + s * u[1], pa[2] + s * u[2]];
    let cb = [pb[0] + t * v[0], pb[1] + t * v[1], pb[2] + t * v[2]];
    if norm(sub(ca, cb)) > CROSSING_GAP * la.min(lb) {
        return;
    }
    let axis = u.map(|x| x / la);
    let Some(dir) = square_to(v.map(|x| x / lb), axis) else {
        return;
    };
    let run = &mut out.runs[r];
    let Some(i) = run.members.iter().position(|&m| m == a) else {
        return;
    };
    // Along the run: from the member's first joint in the run's direction.
    let (from, to) = run.spans[i];
    let along = if run.forward[i] {
        from + s * la
    } else {
        to - s * la
    };
    if run
        .points
        .iter()
        .any(|p| (p.at - along).abs() < 1e-3 && p.restraints.iter().any(|x| x.member == Some(b)))
    {
        return;
    }
    run.points.push(Point {
        at: along,
        joint: None,
        support: false,
        restraints: vec![Restraint {
            member: Some(b),
            direction: dir,
            through: true,
        }],
    });
}

/// `d` (structure axes) in member `k`'s local y and z.
pub fn local(model: &Model, k: usize, d: [f64; 3]) -> [f64; 2] {
    let r = crate::element::axes(model, &model.members[k]);
    let y = [r[(1, 0)], r[(1, 1)], r[(1, 2)]];
    let z = [r[(2, 0)], r[(2, 1)], r[(2, 2)]];
    let (a, b) = (dot(d, y), dot(d, z));
    let l = (a * a + b * b).sqrt();
    if l > 0.0 { [a / l, b / l] } else { [0.0, 0.0] }
}

#[cfg(test)]
#[path = "bracing_tests.rs"]
mod tests;
