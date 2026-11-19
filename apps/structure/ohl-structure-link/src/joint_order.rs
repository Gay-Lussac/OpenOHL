// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Bottom up, then out** (the automatic joint layout's L1 (a), owner
//! 2026-11-19: "try to go bottom up and out. More things align on the body
//! from the bottom up and then less at the mast ends (arms and top)").
//!
//! A tower's joints sorted into its **body**, its **arms** and its **top**
//! from the geometry alone — never from names:
//!
//! - **The body's outline at each height** is the convex hull of the points
//!   where the legs' lines pass that height: the members of class `leg`
//!   that run steeply (within 30° of the vertical, [`LEG_STEEP`]: an arm's
//!   chord of class `leg` lies flatter). Where no such line passes a
//!   height, no outline there (one line: the outline is that point).
//! - A joint **in the outline** at its height (within [`ON_OUTLINE`]) is of
//!   the body; **outside** it, of an arm — so is a joint at a height the
//!   legs do not reach.
//! - The body above the highest arm joint is the **top** (the peak above
//!   the top arm), and so are the joints above the legs' highest point.
//!
//! The order the layout takes the joints in: the body's from the bottom
//! up, then the arms' and the top's working outwards from the body — an
//! arm joint by how far it lies outside the body's outline, a top joint by
//! how far above the highest arm joint (or above the legs) — each tie by
//! height, then the id. No legs found (no member of class `leg` running
//! steeply): every joint of the body, from the bottom up.
//!
//! Deterministic: a pure function of the joints' points and the legs'
//! lines (lengths rounded to the millimetre for the order).
//!
//! The tolerances are the tool's, not a standard's.

use std::collections::BTreeMap;

use ohl_structures::StructureType;

/// Where a joint lies on the tower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Zone {
    /// In the body's outline (the legs' lines), up to the highest arm.
    Body,
    /// Outside the body's outline: an arm (a crossarm, a bridge).
    Arm,
    /// The body above the highest arm joint, or above the legs.
    Top,
}

impl Zone {
    /// In words.
    pub fn as_str(self) -> &'static str {
        match self {
            Zone::Body => "body",
            Zone::Arm => "arm",
            Zone::Top => "top",
        }
    }
}

/// How steeply a leg's line must run to give the body's outline: its
/// direction's vertical part at least cos 30° (within 30° of the vertical).
/// The tool's value, not a standard's.
pub const LEG_STEEP: f64 = 0.866;

/// How far outside the legs' outline a joint may lie and still be of the
/// body, m (a drawing's rounding and a leg's own width). The tool's value,
/// not a standard's.
pub const ON_OUTLINE: f64 = 0.10;

/// The tower's joints sorted: each joint's zone and its place in the
/// layout's order (0 first).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JointOrder {
    /// Each joint's zone.
    pub zone: BTreeMap<String, Zone>,
    /// Each joint's place in the order (0 first).
    pub rank: BTreeMap<String, usize>,
}

impl JointOrder {
    /// The zone of joint `j` (an unknown joint: the body).
    pub fn zone_of(&self, j: &str) -> Zone {
        self.zone.get(j).copied().unwrap_or(Zone::Body)
    }

    /// The place of joint `j` in the order (an unknown joint: last).
    pub fn rank_of(&self, j: &str) -> usize {
        self.rank.get(j).copied().unwrap_or(usize::MAX)
    }
}

/// The joints of `t` sorted bottom up, then out (see the module).
pub fn joint_order(t: &StructureType) -> JointOrder {
    let nodes: BTreeMap<&str, [f64; 3]> = t
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.point.map(|v| v.si())))
        .collect();
    // The legs' lines: class `leg`, running steeply.
    let mut legs: Vec<([f64; 3], [f64; 3])> = Vec::new();
    for m in &t.members {
        let class = m
            .design
            .as_ref()
            .and_then(|d| d.get("class"))
            .and_then(toml::Value::as_str);
        if class != Some("leg") {
            continue;
        }
        let (Some(a), Some(b)) = (
            nodes.get(m.nodes[0].as_str()).copied(),
            nodes.get(m.nodes[1].as_str()).copied(),
        ) else {
            continue;
        };
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if l > 1e-9 && d[2].abs() / l >= LEG_STEEP {
            legs.push(if a[2] <= b[2] { (a, b) } else { (b, a) });
        }
    }
    let top_of_legs = legs
        .iter()
        .map(|(_, b)| b[2])
        .fold(f64::NEG_INFINITY, f64::max);
    // The outline at height z: the hull of the legs' points there.
    let outline = |z: f64| -> Vec<[f64; 2]> {
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for (a, b) in &legs {
            if z < a[2] - 1e-6 || z > b[2] + 1e-6 {
                continue;
            }
            let dz = b[2] - a[2];
            let s = if dz.abs() < 1e-12 {
                0.0
            } else {
                ((z - a[2]) / dz).clamp(0.0, 1.0)
            };
            pts.push([a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1])]);
        }
        hull(pts)
    };
    // Each joint: inside the outline (how far outside, m) or no outline.
    let mut outside: BTreeMap<&str, Option<f64>> = BTreeMap::new();
    if !legs.is_empty() {
        for (id, p) in &nodes {
            let h = outline(p[2]);
            outside.insert(
                id,
                (!h.is_empty()).then(|| distance_outside(&h, [p[0], p[1]])),
            );
        }
    }
    let arm = |id: &str| -> bool {
        outside
            .get(id)
            .is_some_and(|d| d.is_none_or(|d| d > ON_OUTLINE))
    };
    // The highest arm joint (none: no arm).
    let top_arm = nodes
        .iter()
        .filter(|(id, p)| arm(id) && p[2] <= top_of_legs + 1e-6)
        .map(|(_, p)| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let mut zone: BTreeMap<String, Zone> = BTreeMap::new();
    // (Body: height; out: how far out, then height.)
    let mut body: Vec<(i64, i64, i64, &str)> = Vec::new();
    let mut out: Vec<(i64, i64, &str)> = Vec::new();
    let mm = |v: f64| (v * 1000.0).round() as i64;
    for (id, p) in &nodes {
        let z = if legs.is_empty() {
            Zone::Body
        } else if p[2] > top_of_legs + 1e-6 {
            Zone::Top
        } else if arm(id) {
            Zone::Arm
        } else if top_arm.is_finite() && p[2] > top_arm + 1e-6 {
            Zone::Top
        } else {
            Zone::Body
        };
        zone.insert((*id).to_owned(), z);
        match z {
            Zone::Body => body.push((mm(p[2]), mm(p[0]), mm(p[1]), id)),
            Zone::Arm => {
                let d = outside.get(id).copied().flatten().unwrap_or(0.0);
                out.push((mm(d), mm(p[2]), id));
            }
            Zone::Top => {
                let base = if top_arm.is_finite() && p[2] <= top_of_legs + 1e-6 {
                    top_arm
                } else {
                    top_of_legs
                };
                out.push((mm(p[2] - base), mm(p[2]), id));
            }
        }
    }
    body.sort();
    out.sort();
    let rank = body
        .iter()
        .map(|x| x.3)
        .chain(out.iter().map(|x| x.2))
        .enumerate()
        .map(|(k, id)| (id.to_owned(), k))
        .collect();
    JointOrder { zone, rank }
}

/// The convex hull of `pts` (counter-clockwise, no point twice; Andrew's
/// monotone chain).
fn hull(mut pts: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower: Vec<[f64; 2]> = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// How far point `q` lies outside the convex polygon `h` (counter-clockwise;
/// two points: a segment), m; 0 inside.
fn distance_outside(h: &[[f64; 2]], q: [f64; 2]) -> f64 {
    let seg = |a: [f64; 2], b: [f64; 2]| -> f64 {
        let d = [b[0] - a[0], b[1] - a[1]];
        let l2 = d[0] * d[0] + d[1] * d[1];
        let s = if l2 < 1e-18 {
            0.0
        } else {
            (((q[0] - a[0]) * d[0] + (q[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0)
        };
        let p = [a[0] + s * d[0], a[1] + s * d[1]];
        ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt()
    };
    if h.len() < 3 {
        return match h {
            [a] => seg(*a, *a),
            [a, b] => seg(*a, *b),
            _ => f64::INFINITY,
        };
    }
    let inside = (0..h.len()).all(|k| {
        let (a, b) = (h[k], h[(k + 1) % h.len()]);
        (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0]) >= -1e-9
    });
    if inside {
        return 0.0;
    }
    (0..h.len())
        .map(|k| seg(h[k], h[(k + 1) % h.len()]))
        .fold(f64::INFINITY, f64::min)
}
