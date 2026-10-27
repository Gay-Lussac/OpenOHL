// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Member usage: the member forces of a [`Solution`] against the section's
//! capacity times the load case's strength factor.
//!
//! The capacities come from the model's capacity method
//! (`[analysis] capacity`, [`CapacityMethod`], 2026-10-27):
//!
//! - **ASCE 10-97** (the default, [`crate::asce10`]): compression by the
//!   effective slenderness, local buckling and the column curve, the
//!   least with the bolts' shear and bearing; tension the least of the net
//!   section, bolt shear and bearing — from the member's design data
//!   ([`crate::design`]), missing data assumed on the safe side and said
//!   per member (`[assumed: …]`). A member whose section has no yield
//!   strength falls back to the simple check.
//! - **Simple** ([`SIMPLE`]): tension against the section's allowable
//!   tension, else area × yield; compression against the allowable
//!   compression (else area × yield), but not above the Euler load
//!   π²EI/(kL)² (I: `i_min`, kL: the member's `lk`, else its effective
//!   length factor × its length) — no column curve, no imperfections, no
//!   connections: **not a design code, unsafe compared with design
//!   codes**.
//!
//! Either way the member's `capacity_tension` and `capacity_compression`
//! replace the computed ones (the compression one buckling included).
//! - **Frames**: linear interaction N/N_c + M_y/M_y,c + M_z/M_z,c with the
//!   largest end moments (the allowable moments, else W × yield) — except
//!   **single angles** (a profile `equal_angle` or `unequal_angle`),
//!   checked for their axial force only, as lattice tower practice checks
//!   them (2026-10-27; [`is_single_angle`]); `[analysis]
//!   single_angle_moments = true` adds their end moments as before.
//! - **Cables**: tension only.
//! - **Tension-only members**: tension as trusses; their compression —
//!   never above their own buckling load, which the solver caps
//!   ([`crate::solve::buckling_loads`]) — against the member's
//!   `capacity_compression`, else the allowable compression, else area ×
//!   yield, without the Euler load again (the solve has taken buckling
//!   into account: a bowed member carries its buckling load and no more,
//!   it does not fail by it). One bowed says so ([`BOWED`]: `bowed:
//!   carries its buckling load 0.83 kN`); one left out as slack (no
//!   buckling load) uses 0 ("slack"). One kept active because the active
//!   set did not settle ([`Solution::kept_active`], the last resort) is
//!   checked as a truss (compression too) and marked [`KEPT_ACTIVE`]:
//!   approximate. One that bends (`bending = true`) is checked as a frame
//!   while it is in the model (its moments too).
//! - **Modelling members** (`check = false`, a tower program's fictitious
//!   members): solved, not checked ([`NOT_CHECKED`], no usage).

use crate::model::{CapacityMethod, MemberKind, Model};
use crate::solve::Solution;

/// The simple check in words, as results and notes say it.
pub const SIMPLE: &str = "simple check (Euler buckling, gross yield): not a design code; \
                          unsafe compared with design codes";

/// The capacity method of `model` in words, for the results' notes: ASCE
/// 10-97 with how many members' capacities rest on assumptions (from
/// `usage`), or [`SIMPLE`].
pub fn capacity_note(model: &Model, usage: &[MemberUsage]) -> String {
    match model.analysis.capacity {
        CapacityMethod::Simple => format!("member capacities: {SIMPLE}"),
        CapacityMethod::Asce10 => {
            let assumed = usage.iter().filter(|u| u.governs.contains(ASSUMED)).count();
            let simple = usage
                .iter()
                .filter(|u| u.governs.contains(NO_YIELD))
                .count();
            let mut text = "member capacities: ASCE 10-97 (Design of Latticed Steel Transmission \
                            Structures)"
                .to_owned();
            if assumed > 0 {
                text.push_str(&format!(
                    "; {assumed} member(s) with data assumed on the safe side (\"{ASSUMED} …]\" in \
                     what governs)"
                ));
            }
            if simple > 0 {
                text.push_str(&format!(
                    "; {simple} member(s) without a yield strength: {SIMPLE}"
                ));
            }
            text
        }
    }
}

/// The mark before what a capacity method assumed for a member's missing
/// design data, in what governs: `[assumed: length ratios 1, …]`.
pub const ASSUMED: &str = "[assumed:";

/// The mark of a member checked by the simple check under ASCE 10-97
/// because its section has no yield strength.
pub const NO_YIELD: &str = "no yield strength: simple check";

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

/// The mark on what governs a bowed tension-only member
/// ([`crate::MemberForces::bowed`]), followed by its buckling load:
/// `bowed: carries its buckling load 0.83 kN`.
pub const BOWED: &str = "bowed: carries its buckling load";

/// What governs a member not design-checked (`check = false`: a
/// modelling member): no usage.
pub const NOT_CHECKED: &str = "not checked (modelling member)";

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
    (0..model.members.len())
        .map(|idx| usage_of(model, solution, idx, &solution.members[idx], strength))
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

/// Usage of member `idx` under forces `f` (its own, or a piece's).
fn usage_of(
    model: &Model,
    solution: &Solution,
    idx: usize,
    f: &crate::solve::MemberForces,
    strength: f64,
) -> MemberUsage {
    {
        {
            let m = &model.members[idx];
            let s = &model.sections[m.section];
            if !m.check {
                // A modelling member: solved, not checked.
                return MemberUsage {
                    member: idx,
                    usage: None,
                    governs: NOT_CHECKED.into(),
                };
            }
            if m.kind == MemberKind::Tension && f.slack {
                return MemberUsage {
                    member: idx,
                    usage: Some(0.0),
                    governs: "slack".into(),
                };
            }
            let n = f.axial;
            let kn = |x: f64| x / 1000.0;
            // The design method's capacities (none: the simple check).
            let asce = (model.analysis.capacity == CapacityMethod::Asce10
                && s.yield_strength.is_some())
            .then(|| {
                let partner = m
                    .design
                    .crossing
                    .map(|p| solution.members.get(p).is_some_and(|f| f.axial > 0.0));
                crate::asce10::capacity(model, idx, partner)
            });
            let mut assumed: &[String] = &[];
            let mut extra = String::new();
            if model.analysis.capacity == CapacityMethod::Asce10 && s.yield_strength.is_none() {
                extra = format!(" ({NO_YIELD})");
            }
            // Axial: (ratio, words).
            let axial: Option<(f64, String)> = if n >= 0.0 {
                match (
                    m.capacity_tension,
                    asce.as_ref().and_then(|c| c.tension.as_ref()),
                ) {
                    (Some(cap), _) => Some((
                        n / (strength * cap),
                        format!("tension {:.1} kN of {:.1} kN (given)", kn(n), kn(cap)),
                    )),
                    (None, Some((cap, what))) if m.kind != MemberKind::Cable => {
                        assumed = asce.as_ref().map_or(&[][..], |c| &c.tension_assumed[..]);
                        Some((
                            n / (strength * cap),
                            format!("tension {:.1} kN of {:.1} kN ({what})", kn(n), kn(*cap)),
                        ))
                    }
                    _ => s
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
            } else if m.kind == MemberKind::Tension && !solution.kept_active.contains(&idx) {
                // Buckling is in the solve (the cap): no Euler term.
                let bowed = if f.bowed {
                    format!("{BOWED} {:.2} kN — ", kn(-n))
                } else {
                    String::new()
                };
                let cap = m.capacity_compression.map(|c| (c, " (given)")).or(s
                    .compression
                    .or(s.yield_strength.map(|y| y * s.area))
                    .map(|c| (c, "")));
                let Some((cap, given)) = cap else {
                    // No capacity: no usage, but a bowed member says so.
                    return MemberUsage {
                        member: idx,
                        usage: None,
                        governs: bowed.trim_end_matches(" — ").to_owned(),
                    };
                };
                Some((
                    -n / (strength * cap),
                    format!(
                        "{bowed}compression {:.1} kN of {:.1} kN{given}",
                        kn(-n),
                        kn(cap)
                    ),
                ))
            } else if let Some(cap) = m.capacity_compression {
                Some((
                    -n / (strength * cap),
                    format!("compression {:.1} kN of {:.1} kN (given)", kn(-n), kn(cap)),
                ))
            } else if let Some(c) = &asce
                && let Some((cap, what)) = &c.compression
            {
                assumed = &c.compression_assumed;
                if let Some(l) = &c.limit {
                    extra = format!("; {l}");
                }
                Some((
                    -n / (strength * cap),
                    format!(
                        "compression {:.1} kN of {:.1} kN ({what})",
                        kn(-n),
                        kn(*cap)
                    ),
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
            let axial = axial.map(|(u, mut words)| {
                words.push_str(&extra);
                if !assumed.is_empty() {
                    words.push_str(&format!(" {ASSUMED} {}]", assumed.join(", ")));
                }
                (u, words)
            });
            if !m.is_frame() || (is_single_angle(s) && !model.analysis.single_angle_moments) {
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
        }
    }
}

/// Whether a section is a single angle: its profile an `equal_angle` or
/// `unequal_angle` of the section library. A frame of such a section is
/// checked for its axial force only unless `[analysis]
/// single_angle_moments` (the end moments of a lattice tower's angles
/// modelled as frames are secondary: tower practice designs them axially,
/// the connections' eccentricity in the effective slenderness).
pub fn is_single_angle(s: &crate::model::Section) -> bool {
    matches!(
        s.profile.as_ref().map(|p| &p.shape),
        Some(ohl_sections::Shape::EqualAngle { .. } | ohl_sections::Shape::UnequalAngle { .. })
    )
}

/// The member using most of its capacity (a usage that is not a number
/// ranks above every other, [`usage_order`]).
pub fn worst(usage: &[MemberUsage]) -> Option<&MemberUsage> {
    usage
        .iter()
        .filter(|u| u.usage.is_some())
        .max_by(|a, b| usage_order(a.usage.unwrap_or(0.0), b.usage.unwrap_or(0.0)))
}

/// Usage at or above this share (and up to 100 %) is shown as near full
/// (a warning, orange) by both applications and the reports. A display
/// threshold, not a design limit.
pub const NEAR_FULL: f64 = 0.9;

/// What a usage comes to, one rule for every check, view and report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageClass {
    /// Below [`NEAR_FULL`].
    Pass,
    /// From [`NEAR_FULL`] to 100 %.
    Near,
    /// Over 100 %, not allowed (infinite), or not a number (code review
    /// 2026-10-26: a usage that is not a number never passes).
    Over,
}

/// The class of usage `u` (1 = 100 %): over above 1 and when not a
/// number, near from [`NEAR_FULL`], else pass.
pub fn classify_usage(u: f64) -> UsageClass {
    if u.is_nan() || u > 1.0 {
        UsageClass::Over
    } else if u >= NEAR_FULL {
        UsageClass::Near
    } else {
        UsageClass::Pass
    }
}

/// Orders two usages, a usage that is not a number above every other
/// (whatever its sign bit), so that the worst of a list is never a pass
/// hiding a failed computation.
pub fn usage_order(a: f64, b: f64) -> std::cmp::Ordering {
    let key = |u: f64| if u.is_nan() { f64::INFINITY } else { u };
    key(a)
        .total_cmp(&key(b))
        .then_with(|| a.is_nan().cmp(&b.is_nan()))
}

#[cfg(test)]
mod usage_class_tests {
    use super::*;

    #[test]
    fn usages_are_classed_and_ordered_nan_safe() {
        assert_eq!(classify_usage(0.5), UsageClass::Pass);
        assert_eq!(classify_usage(NEAR_FULL), UsageClass::Near);
        assert_eq!(classify_usage(1.0), UsageClass::Near);
        assert_eq!(classify_usage(1.0001), UsageClass::Over);
        assert_eq!(classify_usage(f64::INFINITY), UsageClass::Over);
        assert_eq!(classify_usage(f64::NAN), UsageClass::Over);
        assert_eq!(classify_usage(-f64::NAN), UsageClass::Over);
        // NaN (either sign) above everything, infinity included.
        let mut v = [0.3, -f64::NAN, 2.0, f64::INFINITY, 0.9];
        v.sort_by(|a, b| usage_order(*a, *b));
        assert!(v[4].is_nan() && v[3] == f64::INFINITY && v[0] == 0.3);
        let members = [
            MemberUsage {
                member: 0,
                usage: Some(0.4),
                governs: String::new(),
            },
            MemberUsage {
                member: 1,
                usage: Some(-f64::NAN),
                governs: String::new(),
            },
            MemberUsage {
                member: 2,
                usage: Some(3.0),
                governs: String::new(),
            },
        ];
        assert_eq!(worst(&members).map(|m| m.member), Some(1));
    }
}
