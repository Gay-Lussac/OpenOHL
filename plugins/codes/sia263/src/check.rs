// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A member's usage by SIA 263:2013 under its forces in one load case:
//! OpenOHL's core check (`ohl_structures_fe::check`, before 2026-11-18)
//! for the SIA 263 method, moved with the code unchanged — what governs
//! in the same words, the same marks (`[assumed: …]`, `[not on the safe
//! side: …]`, `[joints: …]`, `[redundant force: …]`), so that a
//! structure's results are the same as before the move. What the core
//! says about a member whatever method checks it (kept active, an
//! unbraced crossing, eccentric ends) is still added by OpenOHL.
//!
//! - **Frames**: linear interaction N/N_c + M_y/M_y,c + M_z/M_z,c with the
//!   largest moments along the member, except single angles and lattice
//!   members (a class given) checked for their axial force only unless
//!   `[analysis] single_angle_moments`; SIA 263's stability check
//!   (formulas 50, 62, 63) for frames, eccentric angles and class 4.
//! - **Cables**: tension only. **Tension-only members**: their compression
//!   (never above their buckling load: the solve caps it) against the
//!   member's or section's given compression, else area × yield.
//! - **Redundant members** (`[analysis] redundant_force`): also for the
//!   share of the compression in the member they brace.

use crate::model::{Analysis, Member, MemberForces, MemberKind, Supported};
use crate::sia263;

/// A member's usage in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberUsage {
    /// Share of the capacity used (1 = 100 %); none: no capacity.
    pub usage: Option<f64>,
    /// What governs, in words.
    pub governs: String,
}

/// The mark before what a capacity method assumed for a member's missing
/// design data, in what governs: `[assumed: length ratios 1, …]`.
pub const ASSUMED: &str = "[assumed:";

/// The mark before what a member's check could not do, so that its usage
/// may be too low: `[not on the safe side: …]` (OpenOHL counts it).
pub const NOT_SAFE: &str = "[not on the safe side:";

/// Whether a capacity method's note on a member is a part it could not
/// check (its usage then possibly too low), not an assumption on the safe
/// side.
fn unchecked_note(note: &str) -> bool {
    note.contains("not checked")
}

/// A single angle's (or lattice member's) end moments are left out of
/// its check unless they would add at least this share of its capacity
/// (1 %): above it the member says what they would add.
pub const ANGLE_MOMENTS_SAID: f64 = 0.01;

/// The mark before the conditions a member's ends take from their
/// connections, per end, in what governs: `[joints: …]`.
pub const JOINTS: &str = "[joints:";

/// The mark of a member checked by the simple check under a design code
/// because its section has no yield strength.
pub const NO_YIELD: &str = "no yield strength: simple check";

/// The mark on what governs a bowed tension-only member, followed by its
/// buckling load: `bowed: carries its buckling load 0.83 kN`.
pub const BOWED: &str = "bowed: carries its buckling load";

/// What governs a slack tension-only member of a model whose tension-only
/// members carry no compression (`[analysis] tension_compression =
/// "none"`).
pub const SLACK_NO_COMPRESSION: &str = "slack (no compression)";

/// The mark before a redundant member's nominal force check in what
/// governs (`[analysis] redundant_force`): `[redundant force: 2.5 % of
/// 37.0 kN, the compression of L12 at N4 (the member it braces): 0.93 kN,
/// 6 % — own force governs]`.
pub const REDUNDANT_FORCE: &str = "[redundant force:";

/// The largest bending moments along a member, about its local y and z
/// (N·m), between its joints: from its end forces and the load along it
/// they include (a frame's self-weight), the moment at every point x —
/// M_y(x) = M_y,i + x F_z,i + q_z x² / 2, M_z(x) = M_z,i − x F_y,i − q_y x²
/// / 2 — its largest at an end or where the shear is zero; plus, for the
/// load put on its joints as halves (member wind, ice, a truss's
/// self-weight), q L² / 8 (a simply supported member's: on the safe side,
/// whatever its ends do).
pub fn moments_along(f: &MemberForces) -> [f64; 2] {
    let e = &f.end_forces;
    let l = f.length;
    let largest = |m0: f64, v: f64, q: f64| -> f64 {
        let at = |x: f64| (m0 + v * x + q * x * x / 2.0).abs();
        let mut best = at(0.0).max(at(l));
        if q.abs() > 0.0 {
            let x = -v / q;
            if x > 0.0 && x < l {
                best = best.max(at(x));
            }
        }
        best
    };
    let my = largest(e[4], e[2], f.span_load[2]) + f.lumped_load[2].abs() * l * l / 8.0;
    let mz = largest(e[5], -e[1], -f.span_load[1]) + f.lumped_load[1].abs() * l * l / 8.0;
    [my, mz]
}

/// Euler buckling load of a member, N (none without a second moment).
pub fn euler_load(m: &Member) -> Option<f64> {
    let s = &m.section;
    let kl = match m.lk {
        Some(lk) => lk,
        None => m.k * m.length,
    };
    (s.i_min > 0.0).then(|| std::f64::consts::PI.powi(2) * s.e * s.i_min / (kl * kl))
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

/// The member's usage in one case: its own forces `f`, and — a redundant
/// member with the check on — its nominal force (`supported`: the member
/// it braces, else why not found).
#[allow(clippy::too_many_arguments)]
pub fn member_usage(
    analysis: &Analysis,
    m: &Member,
    f: &MemberForces,
    partner_axial: Option<f64>,
    kept_active: bool,
    strength: f64,
    supported: Option<Result<Supported, String>>,
) -> MemberUsage {
    // The crossing partner in tension in this case (as the core took it).
    let partner = partner_axial.map(|a| a > 0.0);
    let own = usage_of(analysis, m, f, partner, kept_active, strength);
    match analysis.redundant_force {
        Some(share) => with_redundant_force(
            analysis,
            m,
            f,
            partner,
            kept_active,
            strength,
            share,
            &supported,
            own,
        ),
        None => own,
    }
}

/// A redundant member's usage `own` with its nominal force check
/// (`[analysis] redundant_force`): `share` of the compression in the
/// member it braces (`supported`), checked in compression and in tension
/// (the direction of the bracing force is not known; a tension-only
/// member or cable in tension only) by the same capacities as its own
/// force; its usage the larger of its own and the nominal one, what
/// governs saying which ([`REDUNDANT_FORCE`]). Where the braced member
/// could not be determined: said not on the safe side (the nominal check
/// not done). Every other member: `own`.
#[allow(clippy::too_many_arguments)]
fn with_redundant_force(
    analysis: &Analysis,
    m: &Member,
    f: &MemberForces,
    partner: Option<bool>,
    kept_active: bool,
    strength: f64,
    share: f64,
    supported: &Option<Result<Supported, String>>,
    mut own: MemberUsage,
) -> MemberUsage {
    let Some(supported) = supported else {
        return own;
    };
    let pct = share * 100.0;
    let s = match supported {
        Err(why) => {
            own.governs.push_str(&format!(
                " {NOT_SAFE} redundant force ({pct} %) not checked: {why}]"
            ));
            return own;
        }
        Ok(s) => s,
    };
    let braced = format!(
        "the compression of {} at {} (the member it braces{})",
        s.member,
        s.joint,
        if s.via_redundant {
            ", itself redundant: its design force"
        } else {
            ""
        }
    );
    let nominal = share * s.force;
    if nominal <= 0.0 {
        own.governs.push_str(&format!(
            " {REDUNDANT_FORCE} {braced}: none in this case — none]"
        ));
        return own;
    }
    let pieces = |axial: f64| MemberForces {
        axial,
        end_forces: [0.0; 12],
        slack: false,
        bowed: false,
        length: f.length,
        span_load: [0.0; 3],
        lumped_load: [0.0; 3],
    };
    let mut checks = vec![usage_of(
        analysis,
        m,
        &pieces(nominal),
        partner,
        kept_active,
        strength,
    )];
    if !matches!(m.kind, MemberKind::Tension | MemberKind::Cable) {
        checks.push(usage_of(
            analysis,
            m,
            &pieces(-nominal),
            partner,
            kept_active,
            strength,
        ));
    }
    let Some(worst) = checks
        .into_iter()
        .filter(|u| u.usage.is_some())
        .max_by(|a, b| usage_order(a.usage.unwrap_or(0.0), b.usage.unwrap_or(0.0)))
    else {
        own.governs.push_str(&format!(
            " {REDUNDANT_FORCE} {pct} % of {:.2} kN, {braced}: {:.2} kN — no capacity]",
            s.force / 1000.0,
            nominal / 1000.0
        ));
        return own;
    };
    let u_nominal = worst.usage.unwrap_or(0.0);
    let governs_nominal = match own.usage {
        Some(u) => usage_order(u_nominal, u) == std::cmp::Ordering::Greater,
        None => true,
    };
    if governs_nominal {
        MemberUsage {
            usage: worst.usage,
            governs: format!(
                "{} {REDUNDANT_FORCE} {pct} % of {:.2} kN, {braced}: {:.2} kN — governs; own \
                 force {}]",
                worst.governs,
                s.force / 1000.0,
                nominal / 1000.0,
                own.usage
                    .map_or("—".to_owned(), |u| format!("{:.0} %", u * 100.0))
            ),
        }
    } else {
        own.governs.push_str(&format!(
            " {REDUNDANT_FORCE} {pct} % of {:.2} kN, {braced}: {:.2} kN, {:.0} % — own force \
             governs]",
            s.force / 1000.0,
            nominal / 1000.0,
            u_nominal * 100.0
        ));
        own
    }
}

/// Usage of member `m` under forces `f` (its own, or a piece's).
fn usage_of(
    analysis: &Analysis,
    m: &Member,
    f: &MemberForces,
    partner: Option<bool>,
    kept_active: bool,
    strength: f64,
) -> MemberUsage {
    let s = &m.section;
    if m.kind == MemberKind::Tension && f.slack {
        return MemberUsage {
            usage: Some(0.0),
            governs: if analysis.no_compression {
                SLACK_NO_COMPRESSION.into()
            } else {
                "slack".into()
            },
        };
    }
    let n = f.axial;
    let kn = |x: f64| x / 1000.0;
    // The design method's capacities (none: the simple check).
    let mut stability = None;
    let asce = match s.yield_strength {
        None => None,
        Some(_) => {
            let (c, st) = sia263::capacity(m, partner);
            stability = st;
            Some(c)
        }
    };
    let mut assumed: &[String] = &[];
    let mut joints: &[String] = &[];
    let mut extra = String::new();
    if s.yield_strength.is_none() {
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
                joints = asce.as_ref().map_or(&[][..], |c| &c.joints[..]);
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
    } else if m.kind == MemberKind::Tension && !kept_active {
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
        joints = &c.joints;
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
        let euler = euler_load(m);
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
    let mut suffix = extra.clone();
    if !joints.is_empty() {
        suffix.push_str(&format!(" {JOINTS} {}]", joints.join("; ")));
    }
    // What the method assumed on the safe side, and what it could not
    // check (its usage then possibly too low).
    let (unchecked, safe): (Vec<&String>, Vec<&String>) =
        assumed.iter().partition(|n| unchecked_note(n));
    if !safe.is_empty() {
        let safe: Vec<&str> = safe.iter().map(|n| n.as_str()).collect();
        suffix.push_str(&format!(" {ASSUMED} {}]", safe.join(", ")));
    }
    let mut not_safe_notes: Vec<String> = unchecked.into_iter().cloned().collect();
    // Any member checked for its axial force alone: its bending between
    // its joints from the loads along it (its own weight, member wind,
    // ice), said when it would add.
    let axial_only =
        !shifted(m, f) && (!m.is_frame() || (axial_member(m) && !analysis.single_angle_moments));
    if axial_only && m.kind != MemberKind::Cable {
        let [my, mz] = moments_along(f);
        let fy = s.yield_strength;
        let cap_y = s.moment_y.or(s.wy.zip(fy).map(|(w, y)| w * y));
        let cap_z = s.moment_z.or(s.wz.zip(fy).map(|(w, y)| w * y));
        let add =
            cap_y.map_or(0.0, |c| my / (strength * c)) + cap_z.map_or(0.0, |c| mz / (strength * c));
        if add >= ANGLE_MOMENTS_SAID {
            not_safe_notes.push(format!(
                "bending not checked (axial check{}): it would add {:.0} % (My {:.2}, Mz {:.2} kN \
                 m along it)",
                if m.is_frame() {
                    ", single_angle_moments off"
                } else {
                    ": a truss member"
                },
                add * 100.0,
                kn(my),
                kn(mz)
            ));
        }
    }
    not_safe_notes.extend(uncovered_clauses(analysis, m, f, strength));
    if !not_safe_notes.is_empty() {
        suffix.push_str(&format!(" {NOT_SAFE} {}]", not_safe_notes.join(", ")));
    }
    // SIA 263: compression with the bending of an eccentric connection,
    // class 4 and frames by the stability check (formulas 50, 62, 63), the
    // larger of it and the axial one (the connection's).
    if let Some(st) = &stability {
        let frame =
            shifted(m, f) || (m.is_frame() && !(axial_member(m) && !analysis.single_angle_moments));
        let given_cap = if n < 0.0 {
            m.capacity_compression.is_some()
        } else {
            m.capacity_tension.is_some()
        };
        if !given_cap && (frame || (n < 0.0 && (st.eccentric || st.class4))) {
            let moments = if frame {
                // A moment larger between the joints: as a uniform moment
                // that large (on the safe side).
                let mut mm = sia263::frame_moments(st, &f.end_forces);
                let [my, mz] = moments_along(f);
                for (k, a) in st.axes.iter().enumerate() {
                    let along = match a.name {
                        "y" => my,
                        "z" => mz,
                        _ => 0.0,
                    };
                    if along > mm[k][0].abs().max(mm[k][1].abs()) + 1e-9 {
                        mm[k] = [along, along];
                    }
                }
                mm
            } else {
                [[0.0; 2]; 2]
            };
            let (u, words) = st.usage(n, moments, strength);
            let (u, words) = match &axial {
                Some((ua, wa)) if *ua > u || u.is_nan() && !ua.is_nan() => (*ua, wa.clone()),
                _ => (u, words),
            };
            let mut words = words;
            words.push_str(&suffix);
            return MemberUsage {
                usage: Some(u),
                governs: words,
            };
        }
    }
    let axial = axial.map(|(u, mut words)| {
        words.push_str(&suffix);
        (u, words)
    });
    if axial_only {
        return MemberUsage {
            usage: axial.as_ref().map(|a| a.0),
            governs: axial.map(|a| a.1).unwrap_or_default(),
        };
    }
    // The largest moments along it, not only at its ends.
    let [my, mz] = moments_along(f);
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
        usage: Some(usage),
        governs,
    }
}

/// The clauses that apply to member `m` under its forces `f` but are not
/// covered (2026-11-08): each named, for the member's `[not on the safe
/// side: …]`; a resistance given on the member (`capacity_compression`)
/// covers the compression clauses.
///
/// - a compressed built-up member (two angles, a star or cross of angles,
///   two channels): its battens and connectors (SIA 263 5.5), each part
///   buckling between them;
/// - a frame: its shear and torsion (SIA 263 5.1.4, 5.1.7), when the
///   shear reaches 1 % of A·f_y/√3 (the torsion given with it).
fn uncovered_clauses(
    analysis: &Analysis,
    m: &Member,
    f: &MemberForces,
    strength: f64,
) -> Vec<String> {
    use ohl_sections::Shape;
    let s = &m.section;
    if m.kind == MemberKind::Cable {
        return Vec::new();
    }
    let shape = s.profile.as_ref().map(|p| &p.shape);
    let compressed = f.axial < 0.0 && m.capacity_compression.is_none();
    let mut out = Vec::new();
    if compressed
        && matches!(
            shape,
            Some(
                Shape::DoubleAngle { .. }
                    | Shape::StarAngle { .. }
                    | Shape::CrossAngle { .. }
                    | Shape::DoubleChannel { .. }
            )
        )
    {
        out.push(
            "battens and connectors of the built-up member not checked (SIA 263 5.5)".to_owned(),
        );
    }
    let frame =
        shifted(m, f) || (m.is_frame() && !(axial_member(m) && !analysis.single_angle_moments));
    if !frame {
        return out;
    }
    let fy = s.yield_strength;
    let e = &f.end_forces;
    let shear = e[1].hypot(e[2]).max(e[7].hypot(e[8]));
    let torsion = e[3].abs().max(e[9].abs());
    if let Some(fy) = fy {
        let cap = s.area * fy / 3f64.sqrt();
        if cap > 0.0 && shear / (strength * cap) >= ANGLE_MOMENTS_SAID {
            out.push(format!(
                "shear {:.1} kN and torsion {:.2} kN m not checked (SIA 263 5.1.4, 5.1.7)",
                shear / 1000.0,
                torsion / 1000.0,
            ));
        }
    }
    out
}

/// Whether a truss (or tension-only) member's forces carry end moments:
/// the moments of its eccentric ends its solve could not carry, only bars
/// meeting the joint — OpenOHL puts them in its end forces as a uniform
/// moment (2026-11-18; a truss's solve gives none). Such a member is
/// checked with its bending, by the code's interaction of axial force and
/// bending, as a frame is.
pub fn shifted(m: &Member, f: &MemberForces) -> bool {
    !m.is_frame()
        && m.kind != MemberKind::Cable
        && [4, 5, 10, 11].iter().any(|&k| f.end_forces[k] != 0.0)
}

/// Whether a section is a single angle (a profile `equal_angle` or
/// `unequal_angle`).
pub fn is_single_angle(s: &crate::model::Section) -> bool {
    matches!(
        s.profile.as_ref().map(|p| &p.shape),
        Some(ohl_sections::Shape::EqualAngle { .. } | ohl_sections::Shape::UnequalAngle { .. })
    )
}

/// Whether a frame member is checked for its axial force only (unless
/// `[analysis] single_angle_moments`): a single angle, or a lattice
/// member — one whose design data give it a class.
pub fn axial_member(m: &Member) -> bool {
    is_single_angle(&m.section) || m.design.class.is_some()
}
