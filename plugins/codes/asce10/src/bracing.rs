// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Buckling lengths from **what braces a member** (2026-11-19, stage 4.3
//! of the joint details): OpenOHL sends each member's run (the members in
//! line through their joints) and the bracing points along it, each with
//! the members holding it and the direction each holds it in
//! (`member-check-bracing`). This module measures, for a buckling
//! direction, the length between the points that hold it; which points
//! hold which direction is the code's ([`Held`], [`Axis`]: the plugin
//! picks the members that count first).
//!
//! The same file in the asce10 and sia263 plugins (each code passes its
//! own rules).
//!
//! Assumptions (no code gives them; in the theory pages):
//! - two holding directions at least [`APART_DEG`] apart hold the point
//!   in every direction;
//! - one holding direction holds a buckling direction (the displacement
//!   of buckling about an axis) when within [`ALONG_DEG`] of it;
//! - a stretch of the run beyond its last holding point (a free end in
//!   that direction) buckles as a cantilever: twice its length;
//! - a run that nothing holds in a direction: twice the run's length,
//!   said not on the safe side (a mechanism unless the joints' bending
//!   holds it).

/// Two holding directions this far apart (degrees) hold a point every way.
pub const APART_DEG: f64 = 20.0;

/// One holding direction holds a buckling direction within this (degrees).
pub const ALONG_DEG: f64 = 20.0;

/// Positions closer than this are one, m.
const EPS: f64 = 1e-6;

/// A member holding a bracing point.
#[derive(Debug, Clone, PartialEq)]
pub struct Restraint {
    /// Its id.
    pub member: String,
    /// Its direction square to the run, in the braced member's local y, z.
    pub direction: [f64; 2],
    /// It passes through the point (a crossing).
    pub through: bool,
}

/// A bracing point along a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    /// From the run's start, m.
    pub at: f64,
    /// The joint there ("" a crossing away from a joint).
    pub joint: String,
    /// A support fixed in all three translations.
    pub support: bool,
    /// The members holding it.
    pub restraints: Vec<Restraint>,
}

/// A member of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// Its id.
    pub member: String,
    /// Along the run, m.
    pub start: f64,
    /// See `start`.
    pub end: f64,
    /// Its first joint is at `start`.
    pub forward: bool,
}

/// What braces a member: its run and the run's bracing points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bracing {
    /// The run's members in order.
    pub run: Vec<Piece>,
    /// The run's length, m.
    pub length: f64,
    /// Its bracing points by `at` (its two ends among them).
    pub points: Vec<Point>,
}

impl Bracing {
    /// Whether `at` is one of the run's two ends.
    pub fn is_end(&self, at: f64) -> bool {
        at <= EPS || at >= self.length - EPS
    }

    /// The piece of member `id`.
    pub fn piece(&self, id: &str) -> Option<&Piece> {
        self.run.iter().find(|p| p.member == id)
    }
}

/// How a point holds once the code's rules have picked the members that
/// count there.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Held {
    /// The directions of the members that count.
    pub directions: Vec<[f64; 2]>,
    /// Held every way (a support, two directions apart, or a code's rule).
    pub every_way: bool,
}

impl Held {
    /// From the directions that count: every way when two are at least
    /// [`APART_DEG`] apart, or `forced`.
    pub fn new(directions: Vec<[f64; 2]>, forced: bool) -> Self {
        let s = APART_DEG.to_radians().sin();
        let apart = directions.iter().enumerate().any(|(i, a)| {
            directions[i + 1..]
                .iter()
                .any(|b| (a[0] * b[1] - a[1] * b[0]).abs() >= s)
        });
        Self {
            every_way: forced || apart,
            directions,
        }
    }
}

/// A buckling direction to measure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    /// The axis's name ("y", "z", "v", "u").
    pub name: &'static str,
    /// The displacement of buckling about it, local y and z (unit; none:
    /// not known — held only every way).
    pub displacement: Option<[f64; 2]>,
    /// Any holding direction holds it (a code's rule, e.g. ASCE 10-97 for
    /// an angle's v).
    pub any: bool,
}

impl Axis {
    /// Whether a point held as `h` holds this direction.
    pub fn held(&self, h: &Held) -> bool {
        if h.every_way || (self.any && !h.directions.is_empty()) {
            return true;
        }
        let c = ALONG_DEG.to_radians().cos();
        self.displacement.is_some_and(|w| {
            h.directions
                .iter()
                .any(|p| (p[0] * w[0] + p[1] * w[1]).abs() >= c)
        })
    }
}

/// An end of a buckling span.
#[derive(Debug, Clone, PartialEq)]
pub enum End {
    /// The member's own end (0 its first joint, 1 its second).
    Own(usize),
    /// Another member's end, the run's end (its id, its end).
    Piece(String, usize),
    /// Inside the run (the member continuous through the point).
    Inside,
    /// A free end (a cantilever).
    Free,
}

/// The length a member buckles over in one direction.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The buckling length, m (a cantilever's doubled).
    pub length: f64,
    /// Its two ends.
    pub ends: [End; 2],
    /// The joints at its ends in words ("N3", "the crossing", "a free end").
    pub between: [String; 2],
    /// The points at its ends (their places in `Bracing::points`; none:
    /// a free end).
    pub points: [Option<usize>; 2],
    /// A cantilever (one end free in this direction).
    pub free: bool,
    /// Nothing holds the run in this direction.
    pub unheld: bool,
}

impl Span {
    /// The member's own `length` where the stretch is the member itself
    /// (its run's positions added up differ from it by a rounding).
    pub fn snapped(mut self, length: f64) -> Self {
        if !self.free && (self.length - length).abs() <= 1e-9 * length {
            self.length = length;
        }
        self
    }

    /// In words: "1.20 m, N3–N5" (and why when not plain).
    pub fn words(&self) -> String {
        let mut w = format!(
            "{:.2} m, {}–{}",
            self.length, self.between[0], self.between[1]
        );
        if self.unheld {
            w.push_str(" (nothing holds it: twice the run)");
        } else if self.free {
            w.push_str(" (a free end: a cantilever, twice)");
        }
        w
    }
}

/// The span member `id` buckles over in direction `axis`: of the
/// stretches between the points holding it (`held`, one per point of
/// `b`) that overlap the member, the longest — a stretch to a free end of
/// the run doubled, the whole run doubled when nothing holds it. None:
/// the member is not in the run.
pub fn span(b: &Bracing, id: &str, held: &[Held], axis: &Axis) -> Option<Span> {
    let piece = b.piece(id)?;
    let (s, e) = (piece.start, piece.end);
    let hold: Vec<usize> = (0..b.points.len().min(held.len()))
        .filter(|&i| axis.held(&held[i]))
        .collect();
    let name = |i: usize| {
        let p = &b.points[i];
        if p.joint.is_empty() {
            "the crossing".to_owned()
        } else {
            p.joint.clone()
        }
    };
    let end_at = |at: f64| -> End {
        let own = |first: bool| End::Own(if first == piece.forward { 0 } else { 1 });
        if (at - s).abs() <= EPS {
            own(true)
        } else if (at - e).abs() <= EPS {
            own(false)
        } else if at <= EPS {
            let p = &b.run[0];
            End::Piece(p.member.clone(), if p.forward { 0 } else { 1 })
        } else if at >= b.length - EPS {
            let p = &b.run[b.run.len() - 1];
            End::Piece(p.member.clone(), if p.forward { 1 } else { 0 })
        } else {
            End::Inside
        }
    };
    if hold.is_empty() {
        return Some(Span {
            length: 2.0 * b.length,
            ends: [End::Free, End::Free],
            between: ["a free end".into(), "a free end".into()],
            points: [None, None],
            free: true,
            unheld: true,
        });
    }
    // The stretches: (from, to, from point, to point); none: a free end.
    let mut stretches: Vec<(f64, f64, Option<usize>, Option<usize>)> = Vec::new();
    let first = hold[0];
    if b.points[first].at > EPS {
        stretches.push((0.0, b.points[first].at, None, Some(first)));
    }
    for w in hold.windows(2) {
        stretches.push((b.points[w[0]].at, b.points[w[1]].at, Some(w[0]), Some(w[1])));
    }
    let last = hold[hold.len() - 1];
    if b.points[last].at < b.length - EPS {
        stretches.push((b.points[last].at, b.length, Some(last), None));
    }
    let mut best: Option<Span> = None;
    for (a, z, pa, pz) in stretches {
        if !(a < e - EPS && z > s + EPS) {
            continue;
        }
        let free = pa.is_none() || pz.is_none();
        let length = if free { 2.0 * (z - a) } else { z - a };
        let side = |p: Option<usize>, at: f64| match p {
            Some(i) => (end_at(at), name(i)),
            None => (End::Free, "a free end".to_owned()),
        };
        let (ea, na) = side(pa, a);
        let (ez, nz) = side(pz, z);
        if best.as_ref().is_none_or(|x| length > x.length) {
            best = Some(Span {
                length,
                ends: [ea, ez],
                between: [na, nz],
                points: [pa, pz],
                free,
                unheld: false,
            });
        }
    }
    best
}
