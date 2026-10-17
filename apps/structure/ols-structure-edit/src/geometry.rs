// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Where the joints, members, points, wire points and strings of a
//! structure are, in its own axes, and what is picked at a place on the
//! screen.

use ols_structures::{InsulatorLibrary, StructureType};

/// Something picked: in the 3D view, a table row, an item of the header.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Handle {
    /// A joint (`[[node]]`), by id.
    Joint(String),
    /// A member, by id.
    Member(String),
    /// A point where strings hang, by label.
    Point(String),
    /// A wire point, by label.
    Wire(String),
}

impl Handle {
    /// In words: "joint N1".
    pub fn describe(&self) -> String {
        match self {
            Handle::Joint(id) => format!("joint {id}"),
            Handle::Member(id) => format!("member {id}"),
            Handle::Point(l) => format!("point {l}"),
            Handle::Wire(l) => format!("wire point {l}"),
        }
    }
}

/// The positions (local axes) of the joints, points and wire points of a
/// structure, and its strings (top, wire point).
#[derive(Debug, Clone, Default)]
pub struct Geometry {
    /// Joints: id, position, supported.
    pub joints: Vec<(String, [f64; 3], bool)>,
    /// Members: id, ends, with a section.
    pub members: Vec<(String, [f64; 3], [f64; 3], bool)>,
    /// Points: label, position.
    pub points: Vec<(String, [f64; 3])>,
    /// Wire points: label, position, set.
    pub wires: Vec<(String, [f64; 3], Option<u32>)>,
    /// Strings: label, top, wire point.
    pub strings: Vec<(String, [f64; 3], [f64; 3])>,
    /// The stick figure or the `[[body]]` polylines.
    pub sticks: Vec<Vec<[f64; 3]>>,
}

/// The geometry of `t` in its axes; strings placed with `insulators`.
pub fn geometry(t: &StructureType, insulators: &InsulatorLibrary) -> Geometry {
    let si = |p: &[ols_core::units::Length; 3]| p.map(|v| v.si());
    let mut g = Geometry {
        joints: t
            .nodes
            .iter()
            .map(|n| (n.id.clone(), si(&n.point), n.support.is_some()))
            .collect(),
        ..Default::default()
    };
    let joint = |id: &str| g.joints.iter().find(|j| j.0 == id).map(|j| j.1);
    g.members = t
        .members
        .iter()
        .filter_map(|m| {
            Some((
                m.id.clone(),
                joint(&m.nodes[0])?,
                joint(&m.nodes[1])?,
                m.section.is_some(),
            ))
        })
        .collect();
    // Placed with the feet on the ground at the feet's level: local axes.
    let foot = t.foot_level();
    if let Ok(placed) = t.place_strings([0.0, 0.0, foot], 0.0, None, insulators) {
        g.points = placed
            .points
            .iter()
            .map(|p| (p.label.clone(), p.position))
            .collect();
        g.wires = placed
            .wires
            .iter()
            .map(|w| (w.label.clone(), w.position, w.set))
            .collect();
        let at = |label: &str| {
            g.points
                .iter()
                .map(|p| (&p.0, p.1))
                .chain(g.wires.iter().map(|w| (&w.0, w.1)))
                .find(|(l, _)| *l == label)
                .map(|(_, p)| p)
        };
        g.strings = placed
            .strings
            .iter()
            .filter_map(|s| Some((s.label.clone(), at(&s.top)?, at(&s.wire)?)))
            .collect();
    } else {
        g.points = t
            .points
            .iter()
            .map(|p| (p.label.clone(), si(&p.position())))
            .collect();
    }
    if t.members.is_empty() {
        // The stick figure (or the body lines), back in local axes.
        if let Ok(lines) = t.stick(None) {
            g.sticks = lines
                .into_iter()
                .map(|l| l.into_iter().map(|p| [p[0], p[1], p[2] + foot]).collect())
                .collect();
        }
    } else {
        for b in &t.body {
            g.sticks.push(b.points.iter().map(si).collect());
        }
    }
    g
}

impl Geometry {
    /// Every position drawn.
    pub fn all(&self) -> impl Iterator<Item = [f64; 3]> + '_ {
        self.joints
            .iter()
            .map(|j| j.1)
            .chain(self.points.iter().map(|p| p.1))
            .chain(self.wires.iter().map(|w| w.1))
            .chain(self.sticks.iter().flatten().copied())
    }

    /// Where the point or wire point `label` is.
    pub fn at(&self, label: &str) -> Option<[f64; 3]> {
        self.points
            .iter()
            .map(|p| (&p.0, p.1))
            .chain(self.wires.iter().map(|w| (&w.0, w.1)))
            .find(|(l, _)| *l == label)
            .map(|(_, p)| p)
    }
}

/// Where `h` is drawn (local axes; a member at its middle).
pub fn position(g: &Geometry, h: &Handle) -> Option<[f64; 3]> {
    match h {
        Handle::Joint(id) => g.joints.iter().find(|j| &j.0 == id).map(|j| j.1),
        Handle::Point(l) => g.points.iter().find(|p| &p.0 == l).map(|p| p.1),
        Handle::Wire(l) => g.wires.iter().find(|w| &w.0 == l).map(|w| w.1),
        Handle::Member(id) => g
            .members
            .iter()
            .find(|m| &m.0 == id)
            .map(|m| [0, 1, 2].map(|k| 0.5 * (m.1[k] + m.2[k]))),
    }
}

/// The handle nearest to `at` (screen pixels, `project` from local axes)
/// within `radius`: joints, points and wire points first, then members.
pub fn pick(
    g: &Geometry,
    project: &impl Fn([f64; 3]) -> Option<[f64; 2]>,
    at: [f64; 2],
    radius: f64,
) -> Option<Handle> {
    let d =
        |p: [f64; 3]| project(p).map(|q| ((q[0] - at[0]).powi(2) + (q[1] - at[1]).powi(2)).sqrt());
    let nearest = |found: Vec<(Option<f64>, Handle)>| {
        found
            .into_iter()
            .filter_map(|(dist, h)| Some((dist.filter(|x| *x <= radius)?, h)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|b| b.1)
    };
    let handles = g
        .points
        .iter()
        .map(|(l, p)| (d(*p), Handle::Point(l.clone())))
        .chain(
            g.wires
                .iter()
                .map(|(l, p, _)| (d(*p), Handle::Wire(l.clone()))),
        )
        .chain(
            g.joints
                .iter()
                .map(|(id, p, _)| (d(*p), Handle::Joint(id.clone()))),
        )
        .collect();
    if let Some(h) = nearest(handles) {
        return Some(h);
    }
    let mut members = Vec::new();
    for (id, a, b, _) in &g.members {
        let (Some(qa), Some(qb)) = (project(*a), project(*b)) else {
            continue;
        };
        let (dx, dy) = (qb[0] - qa[0], qb[1] - qa[1]);
        let len2 = dx * dx + dy * dy;
        let s = if len2 > 0.0 {
            (((at[0] - qa[0]) * dx + (at[1] - qa[1]) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (x, y) = (qa[0] + s * dx, qa[1] + s * dy);
        members.push((
            Some(((x - at[0]).powi(2) + (y - at[1]).powi(2)).sqrt()),
            Handle::Member(id.clone()),
        ));
    }
    nearest(members)
}

/// The compass's radius: beyond the structure's reach in plan, a third of
/// its height at least.
pub fn compass_radius(g: &Geometry) -> f64 {
    let reach = g.all().map(|p| p[0].hypot(p[1])).fold(3.0, f64::max);
    let (lo, hi) = g
        .all()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p[2]), b.max(p[2]))
        });
    (reach * 1.25).max(0.35 * (hi - lo).max(0.0))
}

/// From `top` to `at`: the angle from straight down (0 hanging, 90 level),
/// the direction in plan (clockwise from ahead, +y) in degrees, and the
/// distance.
pub fn polar(top: [f64; 3], at: [f64; 3]) -> (f64, f64, f64) {
    let d = [0, 1, 2].map(|k| at[k] - top[k]);
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-9 {
        return (0.0, 0.0, 0.0);
    }
    let down = (-d[2] / len).clamp(-1.0, 1.0).acos().to_degrees();
    let dir = if d[0].hypot(d[1]) < 1e-9 {
        0.0
    } else {
        d[0].atan2(d[1]).to_degrees().rem_euclid(360.0)
    };
    (down, dir, len)
}
