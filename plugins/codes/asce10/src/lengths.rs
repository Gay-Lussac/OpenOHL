// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **ASCE 10-97's buckling lengths from what braces a member**
//! (2026-11-19, stage 4.3 of the joint details): the rules this plugin
//! applies to the bracing OpenOHL sends ([`crate::bracing`]). A member
//! giving its own lengths (`lk`, a `k` other than 1, `length_ratios`)
//! keeps them ([`given`]).
//!
//! - **L between the points holding each axis** (3.7.4: L the unbraced
//!   length; C3.7.4: "from working point to working point"): about the
//!   section's y and z and its minor principal axis v, each over the
//!   longest stretch of its run between the points that hold it, that
//!   overlaps the member ([`crate::bracing::span`]).
//! - **A single angle's v axis** (r_z of the standard) is held at every
//!   point some member holds, whatever its direction — Figure 3.2 (a)
//!   takes a leg's v between the points of either face, Appendix B's
//!   Examples 5 and 7 a strut's and a crossing diagonal's between the
//!   points one member holds in the plane; its geometric axes (r_x, r_y)
//!   only where held along them (out of the plane). Every other section
//!   (3.9): each axis only where held along it.
//! - **Legs with staggered bracing** (3.7.4.1, Figure 3.2): an angle leg
//!   whose v stretch ends at a point held from one face only takes its
//!   geometric axes' L × 1.2 (Figure 3.2 (b), (c)); where the stretch
//!   between points held in both faces spans more than four panels
//!   (Figure 3.2: "supported in both faces at the same elevation every
//!   four panels"; a panel the figure's L, the longer geometric stretch —
//!   between the points of one face) its v between the points held in
//!   both faces (no rule: the longer length).
//!   Figure 3.2 is for equal legs without a change of load between panels:
//!   taken for every angle leg (an assumption, said).
//! - **Crossing diagonals** (Appendix B, Example 7, an example of the
//!   standard): a crossing (a point inside the run of a member that is
//!   not a leg, a member passing through) holds out of the plane only
//!   while the other diagonal (each of its pieces there) is in tension of
//!   at least 20 %
//!   of the member's compression, in the same load case; else only in the
//!   plane (as any member). An unequal angle bolted by its long leg is not
//!   held out of the plane at a crossing (Example 7's note: (L1 + 0.5 L2)
//!   / r_yy; the whole length taken, longer).
//! - **A bracing member strong enough** (C3.7.4: "The L/r limitations
//!   specified in Section 3.7.4.3 normally ensure that the redundant
//!   support is adequate"): a member holding a point inside the run counts
//!   only while its own KL/r is within its class's limit (leg 150, bracing
//!   200, redundant 250; without a class 250, as a redundant member —
//!   1.0's definition of the role); worked out again until no more drops
//!   out. A run's end at a joint other members frame into is a working
//!   point (C3.7.4: "from working point to working point"), held every
//!   way as the member's ends always were; one nothing frames into (and
//!   not supported) is a free end.
//! - **The curves' ends** (3.7.4.2: "the unsupported panel"): each end of
//!   a stretch takes its member end's eccentricity and restraint; a point
//!   inside the run (the member continuous through it) concentric and not
//!   restrained (Example 7: the crossover concentric; Examples 8, 9: "No
//!   end restraint is assumed in segment between crossover point and
//!   intermediate redundant support point"); a free end eccentric and not
//!   restrained.
//! - Not taken (lengths longer, on the safe side): 3.7.4.4's K' for a
//!   force varying along a panel; 3.7.4.6's tests.

use std::collections::{HashMap, HashSet};

use crate::bracing::{Axis, Bracing, End, Held, Span, span};
use crate::model::{ConnectedLeg, Ends, Member, MemberClass};

/// The other diagonal's tension holds a crossing out of its plane from
/// this share of the member's compression (ASCE 10-97 Appendix B,
/// Example 7: "not less than 20%").
pub const CROSSING_TENSION: f64 = 0.20;

/// The geometric axes' L of a leg with staggered bracing is multiplied by
/// this (ASCE 10-97 Figure 3.2 (b), (c): (1.2L)/r_xx).
pub const STAGGER: f64 = 1.2;

/// A leg is supported in both faces at least every this many stretches
/// (Figure 3.2: "every four panels").
pub const STAGGER_PANELS: usize = 4;

/// Whether member `m` gives its own buckling lengths: they win.
pub fn given(m: &Member) -> bool {
    m.lk.is_some() || m.k != 1.0 || m.design.length_ratios.is_some()
}

/// One axis's length and the conditions at its two ends.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisLength {
    /// The stretch.
    pub span: Span,
    /// Its L multiplied by this (Figure 3.2's 1.2, else 1).
    pub factor: f64,
    /// (eccentric, restrained) at each end; none: the member's own two
    /// ends (its conditions as they are).
    pub ends: Option<[(bool, bool); 2]>,
}

impl AxisLength {
    /// The L the slenderness takes, m.
    pub fn length(&self) -> f64 {
        self.factor * self.span.length
    }
}

/// A member's buckling lengths worked out from its bracing, one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct Worked {
    /// About y, z and v.
    pub axes: [AxisLength; 3],
    /// What was taken and why, in words.
    pub notes: Vec<String>,
    /// A key of the lengths (the capacities' store).
    pub key: u64,
}

impl Worked {
    /// The lengths in words: "y 2.40 m, N1–N3; z …; v …".
    pub fn words(&self) -> String {
        ["y", "z", "v"]
            .iter()
            .zip(&self.axes)
            .map(|(n, a)| {
                let f = if a.factor != 1.0 {
                    format!(" × {}", a.factor)
                } else {
                    String::new()
                };
                format!("{n} {}{f}", a.span.words())
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// What the rules read beyond the member and its bracing.
pub struct Context<'a> {
    /// Every member of the call, by id (other members' ends and forces).
    pub members: &'a HashMap<String, &'a Member>,
    /// The members that count at a point inside a run (none: all).
    pub adequate: Option<&'a HashSet<String>>,
    /// Each member's axial force in this load case, N, tension positive
    /// (none: no load case — a crossing then holds in its plane only).
    pub axial: Option<&'a HashMap<String, f64>>,
}

/// The single angles (3.7).
fn single_angle(m: &Member) -> bool {
    matches!(
        m.section.profile.as_ref().map(|p| &p.shape),
        Some(ohl_sections::Shape::EqualAngle { .. } | ohl_sections::Shape::UnequalAngle { .. })
    )
}

/// The displacement of buckling about the minor principal axis (along u),
/// local y and z (none: not known).
fn along_u(m: &Member) -> Option<[f64; 2]> {
    let q = m.section.properties.as_ref()?;
    let a = q.principal_angle;
    Some([a.cos(), a.sin()])
}

/// Member `m`'s lengths from bracing `b` by the rules above.
pub fn worked(m: &Member, b: &Bracing, cx: &Context) -> Worked {
    let mut notes: Vec<String> = Vec::new();
    let angle = single_angle(m);
    let long_leg_unequal = matches!(
        m.section.profile.as_ref().map(|p| &p.shape),
        Some(ohl_sections::Shape::UnequalAngle { .. })
    ) && m.design.connected_leg == Some(ConnectedLeg::Long);
    let own = cx.axial.and_then(|a| a.get(&m.id)).copied();
    let compression = own.map_or(0.0, |n| (-n).max(0.0));
    let mut dropped: Vec<String> = Vec::new();
    let held: Vec<Held> = b
        .points
        .iter()
        .map(|p| {
            let inside = !b.is_end(p.at);
            let mut dirs = Vec::new();
            for r in &p.restraints {
                if inside && cx.adequate.is_some_and(|a| !a.contains(&r.member)) {
                    let w = format!("{} at {}", r.member, point_name(p));
                    if !dropped.contains(&w) {
                        dropped.push(w);
                    }
                    continue;
                }
                dirs.push(r.direction);
            }
            // A crossing (inside the run, of a member that is not a leg:
            // Example 7's crossover, not a member passing a run's end):
            // out of the plane while the other diagonal's pieces there
            // pull with at least 20 % of the compression.
            let through: Vec<&str> = p
                .restraints
                .iter()
                .filter(|r| r.through)
                .map(|r| r.member.as_str())
                .collect();
            let crossing = inside
                && m.design.class != Some(MemberClass::Leg)
                && !through.is_empty()
                && compression > 0.0
                && !long_leg_unequal
                && cx.axial.is_some_and(|a| {
                    through.iter().all(|t| {
                        a.get(*t)
                            .is_some_and(|&n| n > 0.0 && n >= CROSSING_TENSION * compression)
                    })
                });
            // A run's end at a joint other members frame into: a working
            // point (C3.7.4: L "from working point to working point"),
            // held every way, as the member's own ends always were.
            let working_point = !inside && !p.restraints.is_empty();
            Held::new(dirs, p.support || crossing || working_point)
        })
        .collect();
    if !dropped.is_empty() {
        notes.push(format!(
            "not counted as bracing (KL/r above its limit, C3.7.4): {}",
            dropped.join(", ")
        ));
    }
    let u = along_u(m);
    let axes = [
        Axis {
            name: "y",
            displacement: Some([0.0, 1.0]),
            any: false,
        },
        Axis {
            name: "z",
            displacement: Some([1.0, 0.0]),
            any: false,
        },
        Axis {
            name: "v",
            displacement: u,
            any: angle,
        },
    ];
    let fallback = || Span {
        length: m.length,
        ends: [End::Own(0), End::Own(1)],
        between: [m.joints[0].clone(), m.joints[1].clone()],
        points: [None, None],
        free: false,
        unheld: false,
    };
    let mut spans: Vec<Span> = axes
        .iter()
        .map(|a| {
            span(b, &m.id, &held, a)
                .unwrap_or_else(fallback)
                .snapped(m.length)
        })
        .collect();
    let mut factor = [1.0, 1.0, 1.0];
    // Figure 3.2: a leg with staggered bracing.
    if angle && m.design.class == Some(MemberClass::Leg) {
        let every = Axis {
            name: "v",
            displacement: None,
            any: false,
        };
        if let Some(full) = span(b, &m.id, &held, &every)
            && full.length > spans[2].length + 1e-6
            && !full.free
        {
            // A panel: the stretch between the points of one face (the
            // geometric axes' stretches, Figure 3.2's L).
            let panel = spans[0].length.max(spans[1].length);
            if full.length > STAGGER_PANELS as f64 * panel + 1e-6 {
                notes.push(format!(
                    "Figure 3.2: not held in both faces within {STAGGER_PANELS} panels — v \
                     between the points held in both faces, {} (no rule: the longer length)",
                    full.words()
                ));
                spans[2] = full;
            } else {
                factor = [STAGGER, STAGGER, 1.0];
                notes
                    .push("Figure 3.2: staggered bracing — the geometric axes' L × 1.2".to_owned());
            }
            if matches!(
                m.section.profile.as_ref().map(|p| &p.shape),
                Some(ohl_sections::Shape::UnequalAngle { .. })
            ) {
                notes.push(
                    "Figure 3.2 is for equal legs: taken for an unequal angle (an assumption)"
                        .to_owned(),
                );
            }
        }
    }
    if long_leg_unequal
        && b.points
            .iter()
            .any(|p| p.restraints.iter().any(|r| r.through))
    {
        notes.push(
            "unequal angle bolted by its long leg: not held out of the plane at a crossing \
             (Example 7's note; the whole length taken)"
                .to_owned(),
        );
    }
    for (a, s) in axes.iter().zip(&spans) {
        if s.unheld {
            notes.push(format!(
                "nothing holds its run about {}: twice the run's length taken — a mechanism \
                 unless the joints' bending holds it, not checked",
                a.name
            ));
        } else if s.free {
            notes.push(format!(
                "about {} a free end ({}–{}): a cantilever, twice its length",
                a.name, s.between[0], s.between[1]
            ));
        }
    }
    let cond = |e: &End| -> (bool, bool) {
        let of = |c: &crate::model::EndConditions, end: usize| {
            (
                c.eccentric[end].value != Some(false),
                c.restrained[end].value == Some(true),
            )
        };
        match e {
            End::Own(k) => of(&m.conditions, *k),
            End::Piece(id, k) => cx
                .members
                .get(id)
                .map_or((true, false), |o| of(&o.conditions, *k)),
            End::Inside => (false, false),
            End::Free => (true, false),
        }
    };
    let mut key = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |x: u64| {
        key ^= x;
        key = key.wrapping_mul(0x0100_0000_01b3);
    };
    let axes: Vec<AxisLength> = spans
        .into_iter()
        .zip(factor)
        .map(|(s, f)| {
            let ends = match &s.ends {
                [End::Own(a), End::Own(b)] if a != b => None,
                [a, b] => Some([cond(a), cond(b)]),
            };
            mix(s.length.to_bits());
            mix(f64::to_bits(f));
            match ends {
                None => mix(1),
                Some(e) => {
                    for (x, y) in e {
                        mix(2 + u64::from(x) * 2 + u64::from(y));
                    }
                }
            }
            AxisLength {
                span: s,
                factor: f,
                ends,
            }
        })
        .collect();
    let axes: [AxisLength; 3] = axes.try_into().expect("three axes");
    Worked { axes, notes, key }
}

/// A point in words.
fn point_name(p: &crate::bracing::Point) -> String {
    if p.joint.is_empty() {
        "the crossing".to_owned()
    } else {
        p.joint.clone()
    }
}

/// Ends from two ends' values.
pub fn ends_of(a: bool, b: bool) -> Ends {
    match (a, b) {
        (true, true) => Ends::BothEnds,
        (false, false) => Ends::None,
        _ => Ends::OneEnd,
    }
}

/// The largest KL/r of a member holding a point inside a run (C3.7.4: the
/// limits of 3.7.4: leg 150, bracing 200, redundant 250; without a class
/// 250, a redundant member in that role).
fn bracing_limit(m: &Member) -> f64 {
    match m.design.class {
        Some(MemberClass::Leg) => 150.0,
        Some(MemberClass::Bracing | MemberClass::Crossing) => 200.0,
        Some(MemberClass::Redundant) | None => 250.0,
    }
}

/// The lengths of the members of one call: which members count as
/// bracing inside a run (worked out once, again until none more drops
/// out), then each member's lengths in a load case.
pub struct Lengths<'a> {
    members: HashMap<String, &'a Member>,
    bracing: &'a HashMap<String, Bracing>,
    adequate: HashSet<String>,
}

impl<'a> Lengths<'a> {
    /// For `members` with what braces them (`bracing`, by member id).
    pub fn new(members: &'a [Member], bracing: &'a HashMap<String, Bracing>) -> Self {
        let mut out = Self {
            members: members.iter().map(|m| (m.id.clone(), m)).collect(),
            bracing,
            adequate: members.iter().map(|m| m.id.clone()).collect(),
        };
        // Dropping a member only lengthens the others: at most as many
        // rounds as members, in practice two or three.
        for _ in 0..=members.len() {
            let next: HashSet<String> = members
                .iter()
                .filter(|m| {
                    // No load case: a crossing holds in its plane only (a
                    // crossing diagonal giving its own lengths: as without
                    // a partner) — the longer lengths.
                    let w = out.worked(m, None);
                    crate::asce10::capacity_worked(m, None, w.as_ref())
                        .kl_r
                        .is_none_or(|k| k <= bracing_limit(m))
                })
                .map(|m| m.id.clone())
                .collect();
            if next == out.adequate {
                break;
            }
            out.adequate = next;
        }
        out
    }

    /// Member `m`'s lengths from its bracing in a load case whose axial
    /// forces are `axial` (none: no load case); none: it gives its own
    /// ([`given`]) or has no bracing.
    pub fn worked(&self, m: &Member, axial: Option<&HashMap<String, f64>>) -> Option<Worked> {
        if given(m) {
            return None;
        }
        let b = self.bracing.get(&m.id)?;
        Some(worked(
            m,
            b,
            &Context {
                members: &self.members,
                adequate: Some(&self.adequate),
                axial,
            },
        ))
    }
}

#[cfg(test)]
#[path = "lengths_tests.rs"]
mod tests;
