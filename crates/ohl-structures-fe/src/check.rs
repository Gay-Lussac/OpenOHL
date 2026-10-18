// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Member usage: the member forces of a [`Solution`] against the section's
//! capacity times the load case's strength factor.
//!
//! - **Tension**: the member's `capacity_tension` when it gives one, else
//!   the section's allowable tension, else area × yield.
//! - **Compression**: the member's `capacity_compression` when it gives
//!   one (it replaces the whole computation, buckling included), else the
//!   allowable compression (else area × yield), but not above the Euler
//!   load π²EI/(kL)² (I: `i_min`, kL: the member's `lk`, else its
//!   effective length factor × its length). A simple check — no column
//!   curve, no imperfections; a design code's buckling curve gives the
//!   allowable compression to write in the section or the member instead.
//! - **Frames**: linear interaction N/N_c + M_y/M_y,c + M_z/M_z,c with the
//!   largest end moments (the allowable moments, else W × yield).
//! - **Cables**: tension only.
//! - **Tension-only members**: as trusses while they take force; one left
//!   out as slack uses 0 ("slack"). One kept active because the active
//!   set did not settle ([`Solution::kept_active`]) is checked as a truss
//!   (compression too) and marked [`KEPT_ACTIVE`]: approximate.

use crate::model::{MemberKind, Model};
use crate::solve::Solution;

/// Usage of one member.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberUsage {
    /// Member (index).
    pub member: usize,
    /// Share of the capacity used (1 = 100 %); none: the section gives no
    /// capacity.
    pub usage: Option<f64>,
    /// What governs, in words.
    pub governs: String,
}

/// The mark on what governs a tension-only member kept active because the
/// active set did not settle ([`Solution::kept_active`]): its usage is
/// that of a truss, approximate.
pub const KEPT_ACTIVE: &str = "kept active (did not settle)";

/// Euler buckling load of a member, N (none without a second moment).
pub fn euler_load(model: &Model, member: usize) -> Option<f64> {
    let m = &model.members[member];
    let s = &model.sections[m.section];
    let kl = match m.lk {
        Some(lk) => lk,
        None => m.k * model.length(m),
    };
    (s.i_min > 0.0).then(|| std::f64::consts::PI.powi(2) * s.e * s.i_min / (kl * kl))
}

/// Usage of every member; `strength`: the load case's strength factor.
pub fn member_usage(model: &Model, solution: &Solution, strength: f64) -> Vec<MemberUsage> {
    model
        .members
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let s = &model.sections[m.section];
            let f = &solution.members[idx];
            if m.kind == MemberKind::Tension && f.slack {
                return MemberUsage {
                    member: idx,
                    usage: Some(0.0),
                    governs: "slack".into(),
                };
            }
            let n = f.axial;
            let kn = |x: f64| x / 1000.0;
            // Axial: (ratio, words).
            let axial: Option<(f64, String)> = if n >= 0.0 {
                match m.capacity_tension {
                    Some(cap) => Some((
                        n / (strength * cap),
                        format!("tension {:.1} kN of {:.1} kN (given)", kn(n), kn(cap)),
                    )),
                    None => s
                        .tension
                        .or(s.yield_strength.map(|y| y * s.area))
                        .map(|cap| {
                            (
                                n / (strength * cap),
                                format!("tension {:.1} kN of {:.1} kN", kn(n), kn(cap)),
                            )
                        }),
                }
            } else if m.kind == MemberKind::Cable {
                Some((0.0, "slack".into()))
            } else if let Some(cap) = m.capacity_compression {
                Some((
                    -n / (strength * cap),
                    format!("compression {:.1} kN of {:.1} kN (given)", kn(-n), kn(cap)),
                ))
            } else {
                let squash = s.compression.or(s.yield_strength.map(|y| y * s.area));
                let euler = euler_load(model, idx);
                let cap = match (squash, euler) {
                    (Some(a), Some(b)) => Some(if b < a { (b, true) } else { (a, false) }),
                    (Some(a), None) => Some((a, false)),
                    (None, Some(b)) => Some((b, true)),
                    (None, None) => None,
                };
                cap.map(|(cap, buckling)| {
                    (
                        -n / (strength * cap),
                        format!(
                            "compression {:.1} kN of {:.1} kN{}",
                            kn(-n),
                            kn(cap),
                            if buckling { " (buckling)" } else { "" }
                        ),
                    )
                })
            };
            if m.kind != MemberKind::Frame {
                return MemberUsage {
                    member: idx,
                    usage: axial.as_ref().map(|a| a.0),
                    governs: axial.map(|a| a.1).unwrap_or_default(),
                };
            }
            let e = &f.end_forces;
            let my = e[4].abs().max(e[10].abs());
            let mz = e[5].abs().max(e[11].abs());
            let fy = s.yield_strength;
            let cap_y = s.moment_y.or(s.wy.zip(fy).map(|(w, y)| w * y));
            let cap_z = s.moment_z.or(s.wz.zip(fy).map(|(w, y)| w * y));
            let mut parts: Vec<(f64, String)> = Vec::new();
            if let Some(a) = axial {
                parts.push(a);
            }
            if let Some(c) = cap_y {
                parts.push((
                    my / (strength * c),
                    format!("My {:.1} kN m of {:.1}", kn(my), kn(c)),
                ));
            }
            if let Some(c) = cap_z {
                parts.push((
                    mz / (strength * c),
                    format!("Mz {:.1} kN m of {:.1}", kn(mz), kn(c)),
                ));
            }
            if parts.is_empty() {
                return MemberUsage {
                    member: idx,
                    usage: None,
                    governs: String::new(),
                };
            }
            let usage = parts.iter().map(|p| p.0).sum();
            let governs = if parts.len() == 1 {
                parts[0].1.clone()
            } else {
                parts
                    .iter()
                    .map(|p| format!("{} ({:.0} %)", p.1, p.0 * 100.0))
                    .collect::<Vec<_>>()
                    .join(" + ")
            };
            MemberUsage {
                member: idx,
                usage: Some(usage),
                governs,
            }
        })
        .map(|mut u| {
            if solution.kept_active.contains(&u.member) {
                u.governs = if u.governs.is_empty() {
                    KEPT_ACTIVE.to_owned()
                } else {
                    format!("{} — {KEPT_ACTIVE}", u.governs)
                };
            }
            u
        })
        .collect()
}

/// The member using most of its capacity.
pub fn worst(usage: &[MemberUsage]) -> Option<&MemberUsage> {
    usage
        .iter()
        .filter(|u| u.usage.is_some())
        .max_by(|a, b| a.usage.unwrap_or(0.0).total_cmp(&b.usage.unwrap_or(0.0)))
}
