// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **A leg piece turned against its joints** (O44 L11, the owner,
//! 2026-11-21: "sometimes the main legs are rotated along their axis 5° out
//! of what would be needed for the joints … the program could find such
//! issues"; and: the plan square and the face planes only hint — "this
//! doesn't mean much when we can clearly see on the joint itself that
//! there's an issue with the main leg rotation").
//!
//! Read **from the joints themselves**: an angle bracing end can lie flat on
//! a flange of the leg only when its axis lies in that flange's plane (its
//! leg bolted contains its axis). For each member end bolting onto a leg
//! piece (directly, or onto a plate on it), the plane through the leg's axis
//! and the end's axis is the plane that flange should lie in; the turn of
//! the leg about its own axis that lays its nearer flange in that plane is
//! that end's estimate ([`crate::flat_roll`] of the leg against the plane).
//! A leg piece is **flagged** where at least two ends of two members give
//! an estimate (within [`IN_FACE_DEG`] of a flange's plane: a plan or hip
//! member arriving between the flanges says nothing), all the same way,
//! within [`SPREAD_DEG`] of each other, and their middle is more than
//! [`crate::FLAT_OFF`] (the owner's 2°): the turn proposed is that middle.
//! The tower's face planes through the neighbouring legs (Orient angles)
//! are only a second explanation, said beside it, never flagged alone.
//!
//! **Never applied silently** (the owner's rule): the layout's notes and
//! Joint details' Solve the clashes window propose "turn leg piece … by
//! x°", with the pieces in line with it (one part, or a splice of the same
//! roll) turned alike so that the whole leg keeps matching, and the effect
//! measured before it is applied by hand.

use std::collections::{BTreeMap, BTreeSet};

use crate::joints::{Arrival, FLAT_OFF, JointDetails};

/// An end gives an estimate only where the plane through the leg's axis
/// and its own lies within this of one of the leg's flanges, degrees (a
/// plan or hip member arriving between the flanges, about 45° off each,
/// says nothing about the leg's turn). An assumption of this check, not a
/// standard's.
pub const IN_FACE_DEG: f64 = 15.0;

/// The estimates of one leg piece agree when they lie within this of each
/// other, degrees (an assumption of this check: twice the owner's 2°
/// flatness, so that two ends each flat within 2° agree).
pub const SPREAD_DEG: f64 = 2.0 * FLAT_OFF;

/// Pieces in line through a joint: within this of each other, degrees (as
/// Orient angles takes legs running on, [`crate::IN_LINE_DEG`]).
const IN_LINE_DEG: f64 = 10.0;

/// One end's estimate of a leg piece's turn.
#[derive(Debug, Clone, PartialEq)]
pub struct RollEstimate {
    /// The joint.
    pub joint: String,
    /// The member end.
    pub member: String,
    /// The turn of the leg (added to its roll) that lays its nearer flange
    /// in the plane through both axes, degrees.
    pub turn: f64,
}

/// A leg piece whose joints say it is turned (or why not, for the pieces
/// with estimates that do not agree).
#[derive(Debug, Clone, PartialEq)]
pub struct LegRoll {
    /// The leg piece.
    pub member: String,
    /// Each end's estimate at its joints.
    pub estimates: Vec<RollEstimate>,
    /// The turn proposed (the estimates' middle), degrees; none: not
    /// flagged ([`LegRoll::why_not`]).
    pub turn: Option<f64>,
    /// Why not flagged (too few estimates, they disagree, within 2°).
    pub why_not: Option<String>,
    /// The pieces in line with it at its joints turned alike (their roll
    /// within 2° of its own): one leg, kept matching.
    pub partners: Vec<String>,
    /// The second explanation: the turn Orient angles gives it (the
    /// tower's faces through its neighbouring legs), degrees; none: not
    /// worked out (no faces found, or not asked).
    pub face_turn: Option<f64>,
}

impl LegRoll {
    /// Flagged: its joints say it is turned.
    pub fn flagged(&self) -> bool {
        self.turn.is_some()
    }

    /// In words: "turn leg piece L1 by 4.5° (with L2 in line, one leg): 5
    /// ends at N2, N3 say 4.1° to 5.2°".
    pub fn words(&self) -> String {
        let (lo, hi) = self
            .estimates
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), e| {
                (a.min(e.turn), b.max(e.turn))
            });
        let joints: BTreeSet<&str> = self.estimates.iter().map(|e| e.joint.as_str()).collect();
        let joints: Vec<&str> = joints.into_iter().collect();
        let said = format!(
            "{} end(s) at {} say {:+.1}° to {:+.1}°",
            self.estimates.len(),
            joints.join(", "),
            lo,
            hi
        );
        let face = match self.face_turn {
            Some(f) if (f - self.turn.unwrap_or(0.0)).abs() <= FLAT_OFF => {
                format!("; the tower's faces through its neighbouring legs say {f:+.1}° too")
            }
            Some(f) => format!(
                "; the tower's faces through its neighbouring legs say {f:+.1}° (a hint only: \
                 the joints decide)"
            ),
            None => String::new(),
        };
        match self.turn {
            Some(t) => {
                let with = if self.partners.is_empty() {
                    String::new()
                } else {
                    format!(" (with {} in line, one leg)", self.partners.join(", "))
                };
                format!(
                    "turn leg piece {} by {t:+.1}°{with}: {said}{face}",
                    self.member
                )
            }
            None => format!(
                "leg piece {}: not flagged — {}; {said}{face}",
                self.member,
                self.why_not.as_deref().unwrap_or("")
            ),
        }
    }
}

/// The leg pieces (class `leg`, single angles) of the joints in `d` and
/// what their joints say of their turn: those flagged, and those with at
/// least one estimate that are not (with why). Deterministic (in the
/// members' order).
pub fn leg_rolls(d: &JointDetails) -> Vec<LegRoll> {
    let is_leg = |a: &Arrival| {
        a.class.as_deref() == Some("leg") && a.profile.as_ref().is_some_and(|p| p.angle.is_some())
    };
    let mut est: BTreeMap<String, Vec<RollEstimate>> = BTreeMap::new();
    let mut partners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for j in &d.joints {
        let legs: Vec<&Arrival> = j.arrivals.iter().filter(|a| is_leg(a)).collect();
        if legs.is_empty() {
            continue;
        }
        // The pieces in line here, turned alike: one leg.
        for (i, a) in legs.iter().enumerate() {
            for b in legs.iter().skip(i + 1) {
                if angle_deg(a.direction, neg(b.direction)) > IN_LINE_DEG {
                    continue;
                }
                let (Some(pa), Some(pb)) = (&a.profile, &b.profile) else {
                    continue;
                };
                // Turned alike: their y axes within 2° (about their axes).
                if angle_deg(pa.axes[1], pb.axes[1]) <= FLAT_OFF {
                    partners
                        .entry(a.member.clone())
                        .or_default()
                        .insert(b.member.clone());
                    partners
                        .entry(b.member.clone())
                        .or_default()
                        .insert(a.member.clone());
                }
            }
        }
        for a in &j.arrivals {
            if is_leg(a)
                || a.continuous.is_some()
                || legs.iter().any(|l| {
                    angle_deg(a.direction, l.direction)
                        .min(angle_deg(a.direction, neg(l.direction)))
                        <= IN_LINE_DEG
                })
            {
                continue;
            }
            // The leg piece it bolts onto: named, or the plate's leg, else
            // every leg piece here (in line: one leg).
            let onto = a.onto.value.as_deref();
            let plate_leg = onto.and_then(|o| {
                j.plates
                    .iter()
                    .find(|p| p.id == o)
                    .and_then(|p| p.leg.clone())
            });
            let targets: Vec<&&Arrival> = match (onto, plate_leg.as_deref()) {
                (_, Some(pl)) => legs.iter().filter(|l| l.member == pl).collect(),
                (Some(o), None) if legs.iter().any(|l| l.member == o) => {
                    legs.iter().filter(|l| l.member == o).collect()
                }
                (Some(o), None) if j.arrivals.iter().any(|x| x.member == o) => Vec::new(),
                _ => legs.iter().collect(),
            };
            for l in targets {
                let Some(p) = &l.profile else { continue };
                let x = p.axes[0];
                let n = cross(x, a.direction);
                if norm(n) < 0.2 {
                    continue;
                }
                let n = scale(n, 1.0 / norm(n));
                let best = [true, false]
                    .into_iter()
                    .filter_map(|leg_y| crate::welds::flat_roll(p.axes, leg_y, n))
                    .min_by(|u, v| u.turn.abs().total_cmp(&v.turn.abs()));
                let Some(f) = best else { continue };
                if f.turn.to_degrees().abs() > IN_FACE_DEG {
                    continue;
                }
                est.entry(l.member.clone()).or_default().push(RollEstimate {
                    joint: j.id.clone(),
                    member: a.member.clone(),
                    turn: round2(f.turn.to_degrees()),
                });
            }
        }
    }
    let mut out = Vec::new();
    for (m, mut e) in est {
        e.sort_by(|a, b| (&a.joint, &a.member).cmp(&(&b.joint, &b.member)));
        e.dedup_by(|a, b| a.joint == b.joint && a.member == b.member);
        let members: BTreeSet<&str> = e.iter().map(|x| x.member.as_str()).collect();
        let mut turns: Vec<f64> = e.iter().map(|x| x.turn).collect();
        turns.sort_by(f64::total_cmp);
        let mid = if turns.len() % 2 == 1 {
            turns[turns.len() / 2]
        } else {
            0.5 * (turns[turns.len() / 2 - 1] + turns[turns.len() / 2])
        };
        let (lo, hi) = (turns[0], turns[turns.len() - 1]);
        let why_not = if members.len() < 2 {
            Some("fewer than two members' ends say how it is turned".to_owned())
        } else if mid.abs() <= FLAT_OFF {
            Some(format!("within {FLAT_OFF:.0}° of its joints"))
        } else if lo.signum() != hi.signum() {
            Some("its ends do not say one way".to_owned())
        } else if hi - lo > SPREAD_DEG {
            Some(format!(
                "its ends disagree by {:.1}° (more than {SPREAD_DEG:.0}°)",
                hi - lo
            ))
        } else {
            None
        };
        out.push(LegRoll {
            partners: partners
                .get(&m)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default(),
            turn: why_not.is_none().then_some(round2(mid)),
            why_not,
            estimates: e,
            member: m,
            face_turn: None,
        });
    }
    out
}

/// [`leg_rolls`] with the second explanation: the turn Orient angles would
/// give each leg piece of `rolls` (the tower's faces through its
/// neighbouring legs) in `text`.
pub fn with_faces(
    mut rolls: Vec<LegRoll>,
    text: &str,
    library: &ohl_sections::SectionLibrary,
) -> Vec<LegRoll> {
    let members: BTreeSet<String> = rolls.iter().map(|r| r.member.clone()).collect();
    if members.is_empty() {
        return rolls;
    }
    let Ok(run) = crate::orient::orient_angles(
        text,
        library,
        &crate::orient::OrientAsk {
            members: Some(members),
            replace_hand: true,
        },
    ) else {
        return rolls;
    };
    let turn: BTreeMap<&str, f64> = run
        .turned
        .iter()
        .chain(&run.same)
        .chain(&run.kept)
        .map(|o| {
            (
                o.member.as_str(),
                if run.same.iter().any(|s| s.member == o.member) {
                    0.0
                } else {
                    o.turn
                },
            )
        })
        .collect();
    for r in &mut rolls {
        r.face_turn = turn.get(r.member.as_str()).copied().map(fold90);
    }
    rolls
}

/// A turn folded into (−90°, 90°] (an angle's flanges at 90°: a quarter
/// turn more lays the other flange where this one was).
fn fold90(mut d: f64) -> f64 {
    while d > 45.0 {
        d -= 90.0;
    }
    while d <= -45.0 {
        d += 90.0;
    }
    round2(d)
}

fn round2(d: f64) -> f64 {
    (d * 100.0).round() / 100.0
}

fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    (dot(a, b) / (norm(a) * norm(b)).max(1e-18))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

fn neg(a: [f64; 3]) -> [f64; 3] {
    [-a[0], -a[1], -a[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
