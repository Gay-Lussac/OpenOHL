// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A member's usage by ASCE 10-97 under its forces in one load case:
//! OpenOHL's core check (`ohl_structures_fe::check`, before 2026-11-18)
//! for the ASCE 10-97 method, moved with the code unchanged — what governs
//! in the same words, the same marks (`[assumed: …]`, `[not on the safe
//! side: …]`, `[joints: …]`, `[redundant force: …]`), so that a
//! structure's results are the same as before the move. What the core
//! says about a member whatever method checks it (kept active, an
//! unbraced crossing, eccentric ends) is still added by OpenOHL.
//!
//! - **Frames**: linear interaction N/N_c + M_y/M_y,c + M_z/M_z,c with the
//!   largest moments along the member, except single angles and lattice
//!   members (a class given) checked for their axial force only unless
//!   `[analysis] single_angle_moments`.
//! - **No yield strength**: the simple check (Euler buckling, gross
//!   yield), said (`no yield strength: simple check`).
//! - **Cables**: tension only. **Tension-only members**: their compression
//!   (never above their buckling load: the solve caps it) against the
//!   member's or section's given compression, else area × yield.
//! - **Redundant members** (`[analysis] redundant_force`): also for the
//!   share of the compression in the member they brace.

use crate::asce10;
use crate::lengths::Worked;
use crate::model::{Analysis, Member, MemberForces, MemberKind, Supported};

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
/// it braces, else why not found). `worked`: its buckling lengths worked
/// out from its bracing in this case (none: its own, or assumed).
#[allow(clippy::too_many_arguments)]
pub fn member_usage(
    analysis: &Analysis,
    m: &Member,
    f: &MemberForces,
    partner_axial: Option<f64>,
    kept_active: bool,
    strength: f64,
    supported: Option<Result<Supported, String>>,
    worked: Option<&Worked>,
) -> MemberUsage {
    // The crossing partner holds it out of its plane while in tension of
    // at least 20 % of its compression in this case (Appendix B, Example
    // 7; 2026-11-19, before: any tension).
    let partner = partner_axial.map(|a| partner_holds(a, f.axial));
    let own = usage_of(analysis, m, f, partner, kept_active, strength, worked);
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
            worked,
        ),
        None => own,
    }
}

/// Whether a crossing partner of axial force `partner` holds a member of
/// axial force `own` (tension positive) out of its plane: in tension of
/// at least [`crate::lengths::CROSSING_TENSION`] of the member's
/// compression (ASCE 10-97 Appendix B, Example 7); a member not in
/// compression: any tension.
pub fn partner_holds(partner: f64, own: f64) -> bool {
    partner > 0.0 && partner >= crate::lengths::CROSSING_TENSION * (-own).max(0.0)
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
    worked: Option<&Worked>,
) -> MemberUsage {
    let Some(supported) = supported else {
        return own;
    };
    let pct = share * 100.0;
    let s = match supported {
        Err(why) => {
            own.governs.push_str(&format!(
                " {NOT_SAFE} redundant force ({pct} %) not checked: {why} — its usage can be \
                 higher]"
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
        worked,
    )];
    if !matches!(m.kind, MemberKind::Tension | MemberKind::Cable) {
        checks.push(usage_of(
            analysis,
            m,
            &pieces(-nominal),
            partner,
            kept_active,
            strength,
            worked,
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
    worked: Option<&Worked>,
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
    let asce = s
        .yield_strength
        .map(|_| asce10::capacity_worked(m, partner, worked));
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
    // A single angle's end moments, left out of its check (ASCE 10-97's
    // curves take the connection's eccentricity), said when they would
    // add to its usage. Likewise any member checked for its axial force
    // alone: its bending between its joints from the loads along it (its
    // own weight, member wind, ice), said when it would add.
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
                    ", single_angle_moments off — turn it on to take them"
                } else {
                    ": a truss member — make it a frame to take them"
                },
                add * 100.0,
                kn(my),
                kn(mz)
            ));
        }
    }
    not_safe_notes.extend(uncovered_clauses(analysis, m, f, strength));
    if !not_safe_notes.is_empty() {
        suffix.push_str(&format!(
            " {NOT_SAFE} {} — its usage can be higher than shown; give what is missing, or \
             check it by hand]",
            not_safe_notes.join(", ")
        ));
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

/// The clauses of ASCE 10-97 that apply to member `m` under its forces
/// `f` but are not covered by the program (2026-11-08): each named, for
/// the member's `[not on the safe side: …]`; a resistance given on the
/// member (`capacity_compression`) covers the compression clauses.
///
/// - a compressed tube, channel or I section: its elements' effective
///   widths (3.9.3) are not applied;
/// - a compressed built-up member (two angles, a star or cross of angles,
///   two channels): its stitch bolts (3.11), each part buckling between
///   them;
/// - a frame checked with its moments: compression with bending by the
///   linear sum, without the amplification and C_m (3.12, 3.13); an I
///   section, channel or two channels bent about its major axis:
///   lateral-torsional buckling (3.14);
/// - a frame: its shear and torsion (3.15), when the shear reaches 1 % of
///   A·f_y/√3 (the torsion given with it).
///
/// Each only above 1 % of the member's capacity where a share can be
/// worked out (the moments, the shear), as the moments' notes.
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
                Shape::Channel { .. }
                    | Shape::ISection { .. }
                    | Shape::RectangularHollow { .. }
                    | Shape::CircularHollow { .. }
                    | Shape::PolygonalHollow { .. }
                    | Shape::DoubleChannel { .. }
            )
        )
    {
        out.push("effective widths of its elements not applied (ASCE 10-97 3.9.3)".to_owned());
    }
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
        out.push("stitch bolts of the built-up member not checked (ASCE 10-97 3.11)".to_owned());
    }
    let frame =
        shifted(m, f) || (m.is_frame() && !(axial_member(m) && !analysis.single_angle_moments));
    if !frame {
        return out;
    }
    let fy = s.yield_strength;
    let [my, mz] = moments_along(f);
    let cap_y = s.moment_y.or(s.wy.zip(fy).map(|(w, y)| w * y));
    let cap_z = s.moment_z.or(s.wz.zip(fy).map(|(w, y)| w * y));
    let share = |mo: f64, cap: Option<f64>| cap.map_or(0.0, |c| mo / (strength * c));
    if f.axial < 0.0 && share(my, cap_y) + share(mz, cap_z) >= ANGLE_MOMENTS_SAID {
        out.push(
            "compression with bending by the linear sum, without the amplification and C_m \
             (ASCE 10-97 3.12, 3.13)"
                .to_owned(),
        );
    }
    if matches!(
        shape,
        Some(Shape::ISection { .. } | Shape::Channel { .. } | Shape::DoubleChannel { .. })
    ) && share(my, cap_y) >= ANGLE_MOMENTS_SAID
    {
        out.push("lateral-torsional buckling not checked (ASCE 10-97 3.14)".to_owned());
    }
    let e = &f.end_forces;
    let shear = e[1].hypot(e[2]).max(e[7].hypot(e[8]));
    let torsion = e[3].abs().max(e[9].abs());
    if let Some(fy) = fy {
        let cap = s.area * fy / 3f64.sqrt();
        if cap > 0.0 && shear / (strength * cap) >= ANGLE_MOMENTS_SAID {
            out.push(format!(
                "shear {:.1} kN and torsion {:.2} kN m not checked (ASCE 10-97 3.15)",
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
