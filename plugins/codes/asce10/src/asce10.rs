// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Member capacities by **ASCE 10-97** (*Design of Latticed Steel
//! Transmission Structures*, American Society of Civil Engineers, 1998),
//! metric form — OpenOHL's default capacity method of the structure check
//! (owner, 2026-10-27), the core's `ohl_structures_fe::asce10` moved into
//! this plugin unchanged (2026-11-18). Built, in active development, not
//! approved yet.
//!
//! Compression:
//!
//! - L/r = max(r_y·L/r_y, r_z·L/r_z, r_v·L/r_v) with the member's length
//!   ratios ([`crate::model::MemberDesign::length_ratios`], else its `k`
//!   on all three), L its `lk` when given, else its length; r_y, r_z the
//!   radii of gyration about the section's y and z, r_v the minor
//!   principal one.
//! - Torsional and torsional-flexural buckling by the profile's symmetry
//!   ([`torsional_slenderness`]): an I or H section (3.9.4) and a star or
//!   cross of angles (3.9.6) by L/r_t, a channel or two angles back to back
//!   (3.9.5) by L/r_tf, taken into L/r when larger; single angles by L/r
//!   and their legs' w/t only (3.7, as C3.7 explains), closed sections by
//!   L/r only (3.9.7); sections without a library profile, or a `general`
//!   one, not checked for it (3.9.8: tests or analysis; said).
//! - Effective slenderness KL/r: L/r ≤ 120 by the ends' eccentricity —
//!   none: L/r; one end: 30 + 0.75 L/r; both ends: 60 + 0.5 L/r; L/r > 120
//!   by the ends' rotational restraint — none: L/r; one end: 28.6 + 0.762
//!   L/r; both ends: 46.2 + 0.615 L/r.
//! - Local buckling of an angle's legs (3.7.3): (w/t)_lim = 80 Ψ/√f_y
//!   (Eq. 3.7-1; Ψ = 2.62 with f_y in MPa: 209.6/√f_y); above it F_cr =
//!   (1.677 − 0.677 (w/t)/(w/t)_lim) f_y up to w/t = 144 Ψ/√f_y (Eq. 3.7-2:
//!   377.3/√f_y), beyond F_cr = 0.0332 π² E/(w/t)² (Eq. 3.7-3); else F_cr =
//!   f_y.
//! - Column curve: C_c = π √(2E/F_cr); F_a = [1 − ½ (KL/r / C_c)²] F_cr
//!   for KL/r ≤ C_c, else π² E/(KL/r)²; P = F_a A.
//! - The capacity is the least of P and the connection's bolt shear and
//!   bearing.
//!
//! Tension: the least of the net section (A − holes·d_hole·t)·f_y, × 0.9
//! for an angle bolted by one leg (3.10.1, 3.10.2; the holes taken as
//! punched, + 1.6 mm; an unequal angle bolted by its short leg with its
//! long leg counted as the short one); block shear of an angle bolted by
//! one leg (Eq. 3.10-1); the bolts' shear (count × shear planes × each
//! one's shear capacity, 4.3.2); bearing (count × 1.5 f_u d t × plies, f_u
//! the lower of the member's and the bolt's, 4.4). An angle's w/t above
//! 25 is said (3.7.1).
//!
//! **At each end** (2026-11-02, stage 4.1 and 4.2 of the joint details):
//! the ends' eccentricity and restraint are each end's
//! (OpenOHL's `member_ends`, sent with the member: given at the end's `[[connection]]`, the
//! member's flags, worked out at an end with a connection, else assumed),
//! the two together giving the curves' none / one end / both ends. The
//! bolt shear, bearing and net section are taken at each end with what
//! its connection gives over the member's data (its bolts, a library
//! bolt's shear capacity with the threads in or out of the shear plane
//! and its ultimate strength, the holes in one section from its gauge
//! lines or bolt points, the leg bolted); the weaker end governs, named
//! (`bolt shear at N2`) when the ends differ. A member without connections
//! is checked as before. Per end, what came from the joints is said in
//! [`Capacity::joints`].
//!
//! A crossing diagonal (class `crossing` with a partner) in compression
//! is braced at the crossing only when its partner is in tension (as
//! given); otherwise its L/r is at least L over its smaller geometric
//! radius of gyration (the whole length out of the plane).
//!
//! **Missing data are assumed on the safe side and said per member**
//! (`[assumed: …]` in what governs): length ratios 1 (or `k`), both ends
//! eccentric, no end restraint, the leg's flat width from the profile
//! without a root radius when it has none, one leg bolted (an unequal
//! angle's short leg), a standard hole (the bolt + 1.6 mm), the holes
//! punched, one hole per section, the bolts' threads in the shear plane
//! (0.62 f_ub on 0.6 of the gross area) when only their
//! strength is given (with the threads given out of the plane but no
//! capacity for it: as with them in). Without bolts, the connection is not checked and
//! the tension is on the gross section (said so). Local buckling is
//! checked on angles (and on sections given a `width_thickness`) only.

use crate::lengths::{Worked, ends_of};
use crate::model::{
    Bolts, Condition, ConnectedLeg, EndDesign, Ends, Member, MemberClass, MemberDesign, Onto,
    OntoLeg, Section, Source,
};

/// The method's name in files and results.
pub const NAME: &str = "asce10-97";

/// The method in words, for the run's notes (as the core's note named it).
pub const TITLE: &str = "ASCE 10-97 (Design of Latticed Steel Transmission Structures)";

/// A standard hole is this much larger than its bolt (ASCE 10-97 4.1:
/// 1/16 in.), m — the hole when not given.
pub const STANDARD_HOLE: f64 = 0.0016;

/// A punched hole is taken this much larger than its nominal diameter in
/// the net section and block shear (3.10.1, 3.10.2: 1/16 in.), m; holes
/// are taken as punched (drilled holes are not told apart).
const PUNCHED: f64 = 0.0016;

/// Bolt shear stress over the bolt's ultimate strength (4.3.2), on the
/// root area with the threads in the shear plane.
const BOLT_SHEAR: f64 = 0.62;

/// The root area over the gross area when the bolt gives no shear
/// capacity: 0.6, the root of a 60° thread (d₃ = d − 1.2269 P) of pitch up
/// to d/5.4 — on the safe side for usual bolts; a bolt's own shear
/// capacity gives the real value.
const ROOT_AREA: f64 = 0.6;

/// The largest w/t of an angle's leg (3.7.1).
const MAX_W_T: f64 = 25.0;

/// The capacities of one member and what they rest on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Capacity {
    /// Compression capacity, N, and what governs it in words.
    pub compression: Option<(f64, String)>,
    /// Tension capacity, N, and what governs it in words.
    pub tension: Option<(f64, String)>,
    /// The effective slenderness KL/r (compression).
    pub kl_r: Option<f64>,
    /// What the compression capacity assumed for missing data, in words.
    pub compression_assumed: Vec<String>,
    /// What the tension capacity assumed for missing data, in words.
    pub tension_assumed: Vec<String>,
    /// Slenderness above the class's limit, in words.
    pub limit: Option<String>,
    /// For a member with connections (`[[connection]]`): per end, its
    /// conditions and where each comes from, in words ("N2 eccentric
    /// (worked out: one leg bolted), restrained (given)"); empty without.
    pub joints: Vec<String>,
}

/// The effective slenderness KL/r from L/r and the ends (ASCE 10-97,
/// curves 1–6).
pub fn effective_slenderness(l_r: f64, eccentricity: Ends, restraint: Ends) -> f64 {
    if l_r <= 120.0 {
        match eccentricity {
            Ends::None => l_r,
            Ends::OneEnd => 30.0 + 0.75 * l_r,
            Ends::BothEnds => 60.0 + 0.5 * l_r,
        }
    } else {
        match restraint {
            Ends::None => l_r,
            Ends::OneEnd => 28.6 + 0.762 * l_r,
            Ends::BothEnds => 46.2 + 0.615 * l_r,
        }
    }
}

/// Ψ of ASCE 10-97 Eqs. 3.7-1 to 3.7-3 with f_y in MPa.
const PSI_MPA: f64 = 2.62;

/// The stress replacing f_y for a leg of width/thickness `w_t` (local
/// buckling, ASCE 10-97 3.7.3, Eqs. 3.7-1 to 3.7-3), Pa; `fy`, `e` in Pa.
pub fn local_buckling_stress(fy: f64, e: f64, w_t: f64) -> f64 {
    let fy_mpa = fy / 1e6;
    let lim = 80.0 * PSI_MPA / fy_mpa.sqrt();
    if w_t <= lim {
        fy
    } else if w_t <= 144.0 * PSI_MPA / fy_mpa.sqrt() {
        (1.677 - 0.677 * w_t / lim) * fy
    } else {
        0.0332 * std::f64::consts::PI.powi(2) * e / (w_t * w_t)
    }
}

/// The design compressive stress F_a, Pa, at effective slenderness `kl_r`
/// with the (local-buckling) stress `fcr`.
pub fn column_stress(fcr: f64, e: f64, kl_r: f64) -> f64 {
    let cc = std::f64::consts::PI * (2.0 * e / fcr).sqrt();
    if kl_r <= cc {
        (1.0 - 0.5 * (kl_r / cc).powi(2)) * fcr
    } else {
        std::f64::consts::PI.powi(2) * e / (kl_r * kl_r)
    }
}

/// The equivalent radius of gyration for torsional buckling r_t (ASCE
/// 10-97 Eq. 3.8-2): warping constant `cw` (m⁶), torsion constant `j`
/// (m⁴), polar second moment about the shear centre `ips` (m⁴), the length
/// free to warp `kt_l` (m).
pub fn torsional_radius(cw: f64, j: f64, ips: f64, kt_l: f64) -> f64 {
    ((cw + 0.04 * j * kt_l * kt_l) / ips).sqrt()
}

/// The equivalent radius of gyration for torsional-flexural buckling r_tf
/// (ASCE 10-97 Eq. 3.8-1) from r_t, the radius about the axis of symmetry
/// r_u, the shear centre's distance from the centroid `u0` and the polar
/// radius about the shear centre `rps`. Ends that differ for twisting and
/// for bending about u (C3.8): r_t / K_t and r_u / K_u passed, the result
/// then r_tf / K'.
pub fn torsional_flexural_radius(rt: f64, ru: f64, u0: f64, rps: f64) -> f64 {
    let (a, b) = (1.0 / (rt * rt), 1.0 / (ru * ru));
    let c = u0 / (rt * ru * rps);
    let sum = a + b + ((a - b).powi(2) + 4.0 * c * c).sqrt();
    (2.0 / sum).sqrt()
}

/// The slenderness of torsional or torsional-flexural buckling of the
/// section's profile over `length` (ASCE 10-97 3.9.4–3.9.6), with the
/// mode in words and what it assumed; none where the code asks only for
/// flexural buckling (single angles, 3.7 and C3.7; closed sections, 3.9.7)
/// and where the profile is not known (a `[[section]]` table; a `general`
/// profile: `Err`, 3.9.8). The member's ends free to warp (K_t 1: the
/// length free to warp is its whole length, on the safe side); bending
/// about the axis of symmetry by the member's length ratio for it.
pub fn torsional_slenderness(
    s: &Section,
    length: f64,
    ratios: [f64; 3],
) -> Option<Result<(f64, &'static str), ()>> {
    use ohl_sections::Symmetry;
    let p = s.profile.as_ref()?;
    let sym = p.shape.symmetry();
    if matches!(sym, Symmetry::Angle | Symmetry::Closed) {
        return None;
    }
    // The properties as OpenOHL worked them out (sent with every profile).
    let q = s.properties.as_ref()?;
    let (Some(sc), Some(cw)) = (q.shear_centre, q.warping) else {
        return Some(Err(()));
    };
    if sym == Symmetry::Unknown || q.area <= 0.0 || length <= 0.0 {
        return Some(Err(()));
    }
    let j = q.it.unwrap_or(s.j);
    let u0 = match sym {
        Symmetry::AboutY => (sc[0] - q.centroid[0]).abs(),
        Symmetry::AboutZ => (sc[1] - q.centroid[1]).abs(),
        _ => 0.0,
    };
    let ips = q.iy + q.iz + q.area * u0 * u0;
    let rt = torsional_radius(cw, j, ips, length);
    Some(Ok(match sym {
        Symmetry::AboutY | Symmetry::AboutZ => {
            let (i, k) = if sym == Symmetry::AboutY {
                (q.iy, ratios[0])
            } else {
                (q.iz, ratios[1])
            };
            let ru = (i / q.area).sqrt() / k.max(f64::MIN_POSITIVE);
            let rps = (ips / q.area).sqrt();
            let rtf = torsional_flexural_radius(rt, ru, u0, rps);
            (length / rtf, "torsional-flexural, 3.9.5")
        }
        _ => (
            length / rt,
            if sym == Symmetry::Double {
                "torsional, 3.9.4"
            } else {
                "torsional, 3.9.6"
            },
        ),
    }))
}

/// The slenderness limit of a member class (compression): leg 150,
/// bracing 200, redundant 250; none for a crossing diagonal without
/// another class (bracing: 200).
fn class_limit(class: Option<MemberClass>) -> Option<(f64, &'static str)> {
    match class? {
        MemberClass::Leg => Some((150.0, "leg")),
        MemberClass::Bracing | MemberClass::Crossing => Some((200.0, "bracing")),
        MemberClass::Redundant => Some((250.0, "redundant")),
    }
}

/// The leg geometry of a section's profile: (largest leg, thickness, root
/// radius), m — angles only.
fn angle_legs(s: &Section) -> Option<(f64, f64, f64)> {
    match s.profile.as_ref()?.shape {
        ohl_sections::Shape::EqualAngle { h, t, r } => Some((h, t, r)),
        ohl_sections::Shape::UnequalAngle { h, b, t, r } => Some((h.max(b), t, r)),
        _ => None,
    }
}

/// The ASCE 10-97 capacities of member `m`. `partner_in_tension`: for a
/// crossing diagonal with a partner, whether that partner is in tension
/// in this load case (none: no partner, or not a crossing diagonal).
/// Worked out once per member and partner in a call ([`crate::memo`]).
pub fn capacity(m: &Member, partner_in_tension: Option<bool>) -> Capacity {
    capacity_worked(m, partner_in_tension, None)
}

/// The same with buckling lengths worked out from the member's bracing
/// in this load case (`worked`, [`crate::lengths`]; none: its own or
/// assumed, as [`capacity`]).
pub fn capacity_worked(
    m: &Member,
    partner_in_tension: Option<bool>,
    worked: Option<&Worked>,
) -> Capacity {
    crate::memo::capacity(
        &m.id,
        partner_in_tension,
        worked.map_or(0, |w| w.key),
        || capacity_of(m, partner_in_tension, worked),
    )
}

fn capacity_of(m: &Member, partner_in_tension: Option<bool>, worked: Option<&Worked>) -> Capacity {
    let s = &m.section;
    let d = &m.design;
    let mut out = Capacity::default();
    let Some(fy) = s.yield_strength else {
        return out;
    };
    let e = s.e;
    let a = s.area;
    let mut assumed: Vec<String> = Vec::new();
    // Slenderness.
    let length = m.lk.unwrap_or(m.length);
    let ratios = match (worked, d.length_ratios) {
        (Some(w), _) => w.axes.each_ref().map(|x| x.span.length / m.length),
        (None, Some(r)) => r,
        (None, None) => {
            if m.lk.is_none() && m.k == 1.0 {
                assumed.push("length ratios 1".into());
            }
            let k = if m.lk.is_some() { 1.0 } else { m.k };
            [k; 3]
        }
    };
    let r = |i: f64| (i / a).sqrt();
    let radii = [r(s.iy), r(s.iz), r(s.i_min)];
    // Worked out: each axis's L/r (its L the stretch's, × Figure 3.2's
    // factor) and the curves' ends of its stretch.
    let per_axis: Vec<(f64, usize)> = match worked {
        Some(w) => (0..3)
            .filter(|&k| radii[k] > 0.0)
            .map(|k| (w.axes[k].length() / radii[k], k))
            .collect(),
        None => Vec::new(),
    };
    let mut l_r = match worked {
        Some(_) => per_axis.iter().map(|x| x.0).fold(0.0, f64::max),
        None => (0..3)
            .filter(|&k| radii[k] > 0.0)
            .map(|k| ratios[k] * length / radii[k])
            .fold(0.0, f64::max),
    };
    let crossing = d.class == Some(MemberClass::Crossing) && worked.is_none();
    if crossing {
        let geometric = radii[0].min(radii[1]);
        let unsupported = if geometric > 0.0 {
            m.length / geometric
        } else {
            0.0
        };
        match partner_in_tension {
            Some(true) => {}
            Some(false) => {
                if unsupported > l_r {
                    l_r = unsupported;
                    assumed.push(
                        "crossing partner not in tension of 20 % of the compression (Appendix B, \
                         Example 7): unbraced out of plane"
                            .into(),
                    );
                }
            }
            None => {
                if unsupported > l_r {
                    l_r = unsupported;
                    assumed
                        .push("crossing diagonal without a partner: unbraced out of plane".into());
                }
            }
        }
    }
    // Torsional and torsional-flexural buckling (3.9.4-3.9.6).
    let mut mode = None;
    match torsional_slenderness(s, length, ratios) {
        Some(Ok((t, what))) if t > l_r => {
            l_r = t;
            mode = Some(what);
            assumed.push("free to warp (K_t 1)".into());
            if s.profile.as_ref().is_some_and(|p| p.shape.is_built_up()) {
                assumed.push("warping of the angles left out".into());
            }
        }
        Some(Err(())) => assumed.push("torsional buckling not checked (general profile)".into()),
        _ => {}
    }
    // What the lengths from the bracing took (Figure 3.2, a free end,
    // bracing not counted).
    if let Some(w) = worked {
        assumed.extend(w.notes.iter().cloned());
    }
    // The ends' eccentricity and restraint, end by end (OpenOHL's
    // member_ends, sent with the member).
    let ends = &m.conditions;
    let at = |end: usize| m.joints[end].as_str();
    let assumed_at =
        |c: &[Condition; 2]| -> [bool; 2] { [0, 1].map(|e| c[e].source == Source::Assumed) };
    match assumed_at(&ends.eccentric) {
        [true, true] => assumed.push("both ends eccentric".into()),
        [true, false] => assumed.push(format!("eccentric at {}", at(0))),
        [false, true] => assumed.push(format!("eccentric at {}", at(1))),
        [false, false] => {}
    }
    if l_r > 120.0 {
        match assumed_at(&ends.restrained) {
            [true, true] => assumed.push("no end restraint".into()),
            [true, false] => assumed.push(format!("not restrained at {}", at(0))),
            [false, true] => assumed.push(format!("not restrained at {}", at(1))),
            [false, false] => {}
        }
        // A restraint worked out by the I/L rule lowers KL/r: what the
        // rule cannot know is said, not on the safe side (2026-11-08).
        for end in 0..2 {
            let c = &ends.restrained[end];
            if c.source == Source::WorkedOut && c.value == Some(true) {
                assumed.push(format!(
                    "restraint at {} by the I/L rule (C3.7.4): the restraining members buckling \
                     in the same case and the plane of the stress not checked",
                    at(end)
                ));
            }
        }
    }
    // Concentric worked out from the bolting: C3.7.4's condition on the
    // bolts' centroid (between the heel and the leg's centre line) is not
    // checked (2026-11-08).
    for end in 0..2 {
        let c = &ends.eccentric[end];
        if c.source == Source::WorkedOut && c.value == Some(false) {
            assumed.push(format!(
                "concentric at {} worked out: the bolts' centroid condition (C3.7.4) not checked",
                at(end)
            ));
        }
    }
    // Worked out: each axis by the ends of its stretch (a torsional mode
    // by the member's own), the largest governing.
    let (kl_r, axis) = match worked {
        None => (
            effective_slenderness(l_r, ends.eccentricity(), ends.restraint()),
            None,
        ),
        Some(w) => {
            let mut best: (f64, Option<usize>) = (0.0, None);
            for &(lr, k) in &per_axis {
                let (ecc, res) = match w.axes[k].ends {
                    None => (ends.eccentricity(), ends.restraint()),
                    Some([(e0, r0), (e1, r1)]) => (ends_of(e0, e1), ends_of(r0, r1)),
                };
                let x = effective_slenderness(lr, ecc, res);
                if best.1.is_none() || x > best.0 {
                    best = (x, Some(k));
                }
            }
            if mode.is_some() {
                let x = effective_slenderness(l_r, ends.eccentricity(), ends.restraint());
                if x >= best.0 {
                    best = (x, None);
                }
            }
            best
        }
    };
    out.kl_r = Some(kl_r);
    if let Some((limit, class)) = class_limit(d.class)
        && kl_r > limit
    {
        out.limit = Some(format!("KL/r {kl_r:.0} above {limit:.0} ({class})"));
    }
    // Local buckling.
    let legs = angle_legs(s);
    let w_t = match (d.width_thickness, legs) {
        (Some(w), _) => Some(w),
        (None, Some((b, t, r))) if t > 0.0 => {
            if r == 0.0 {
                assumed.push("w/t without a root radius".into());
            }
            Some((b - t - r).max(0.0) / t)
        }
        _ => None,
    };
    if let (Some(w), Some(_)) = (w_t, legs)
        && w > MAX_W_T
    {
        let over = format!("w/t {w:.1} above {MAX_W_T:.0} (3.7.1)");
        out.limit = Some(match out.limit.take() {
            Some(l) => format!("{l}; {over}"),
            None => over,
        });
    }
    let fcr = match s.compression {
        // A given allowable compression before buckling replaces A·F_cr.
        Some(c) => c / a,
        None => match w_t {
            Some(w) => local_buckling_stress(fy, e, w),
            None => fy,
        },
    };
    let column = column_stress(fcr, e, kl_r) * a;
    let mut comp = (
        column,
        match (axis, worked) {
            (Some(k), Some(w)) => {
                let x = &w.axes[k];
                let f = if x.factor != 1.0 {
                    format!(" × {}", x.factor)
                } else {
                    String::new()
                };
                format!(
                    "KL/r {kl_r:.0} ({} over {}{f}, from the bracing)",
                    ["y", "z", "v"][k],
                    x.span.words()
                )
            }
            _ => match mode {
                Some(m) => format!("KL/r {kl_r:.0} ({m})"),
                None => format!("KL/r {kl_r:.0}"),
            },
        },
    );
    // The connection, at each end: the end's own data over the member's.
    let thickness = d.thickness.or(legs.map(|l| l.1));
    let checks = [0, 1].map(|end| {
        end_check(
            d,
            d.ends[end].as_ref(),
            s,
            thickness,
            fy,
            a,
            m.onto[end].as_ref(),
        )
    });
    // Ends alike (no connection data, or the same capacities at both): as
    // one, as before; else what governs named by its end (a note of one
    // end only is named by it in any case).
    let alike = (checks[0].shear, checks[0].bearing, &checks[0].tension)
        == (checks[1].shear, checks[1].bearing, &checks[1].tension);
    let name = |what: &str, end: usize| {
        if alike {
            what.to_owned()
        } else {
            format!("{what} at {}", at(end))
        }
    };
    for (end, c) in checks.iter().enumerate() {
        for (v, what) in [(c.shear, "bolt shear"), (c.bearing, "bearing")] {
            if let Some(v) = v
                && v < comp.0
            {
                comp = (v, name(what, end));
            }
        }
    }
    let notes = |pick: fn(&EndCheck) -> &Vec<String>| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (end, c) in checks.iter().enumerate() {
            for n in pick(c) {
                let n = if pick(&checks[1 - end]).contains(n) {
                    n.clone()
                } else {
                    format!("{n} at {}", at(end))
                };
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
    };
    let conn = notes(|c| &c.conn);
    out.compression = Some(comp);
    out.compression_assumed = assumed;
    out.compression_assumed.extend(conn.iter().cloned());
    // Tension: the weaker end.
    let mut tension = (f64::INFINITY, String::new());
    for (end, c) in checks.iter().enumerate() {
        if c.tension.0 < tension.0 {
            tension = (c.tension.0, name(&c.tension.1, end));
        }
    }
    if let Some(t) = s.tension {
        tension = (t, "allowable".to_owned());
    }
    out.tension = Some(tension);
    out.tension_assumed = conn;
    for n in notes(|c| &c.tension_notes) {
        if !out.tension_assumed.contains(&n) {
            out.tension_assumed.push(n);
        }
    }
    // Where the conditions come from, for a member with connections.
    if ends.connected.contains(&true) {
        for end in 0..2 {
            let say = |c: &Condition, yes: &str, no: &str| {
                let word = match c.value {
                    Some(true) => yes.to_owned(),
                    Some(false) => no.to_owned(),
                    None => format!("{yes} at one end"),
                };
                match &c.why {
                    Some(w) => format!("{word} ({}: {w})", c.source.words()),
                    None => format!("{word} ({})", c.source.words()),
                }
            };
            let mut line = format!(
                "{} {}, {}",
                at(end),
                say(&ends.eccentric[end], "eccentric", "concentric"),
                say(&ends.restrained[end], "restrained", "not restrained")
            );
            if let Some(e) = &d.ends[end] {
                if let Some(b) = &e.bolt {
                    line.push_str(&format!(", bolt {b}"));
                }
                for n in &e.notes {
                    line.push_str(&format!(", {n}"));
                }
            } else {
                line.push_str(", no connection given");
            }
            out.joints.push(line);
        }
    }
    out
}

/// One end's connection as the check takes it.
#[derive(Debug, Clone, PartialEq)]
struct EndCheck {
    /// The bolts' shear, N.
    shear: Option<f64>,
    /// Bearing, N.
    bearing: Option<f64>,
    /// The tension capacity at this end and what governs it.
    tension: (f64, String),
    /// What the connection assumed or could not check.
    conn: Vec<String>,
    /// What the tension (net section) assumed besides.
    tension_notes: Vec<String>,
}

/// The bolts at one end: what the end's connection gives over the
/// member's (a diameter, hole, shear capacity of the member's only for
/// bolts of the same diameter); with the library's shear capacity with
/// the threads out of the shear plane and whether the threads are in it.
pub fn end_bolts(
    d: &MemberDesign,
    end: Option<&EndDesign>,
    notes: &mut Vec<String>,
) -> Option<(Bolts, Option<f64>, Option<bool>)> {
    let layout = end.and_then(|e| e.holes);
    let member = d.bolts.map(|mut b| {
        if layout.is_some() {
            b.holes = layout;
        }
        (b, None, None)
    });
    let Some(e) = end else {
        return member;
    };
    if e.continuous {
        notes.push("continuous through the joint: no connection to check".into());
        return None;
    }
    let Some(own) = e.bolts else {
        if let Some(k) = &e.kind
            && k != "bolted"
        {
            notes.push(format!("{k}: no bolts, the connection not checked"));
            return None;
        }
        return member;
    };
    let Some(diameter) = own.diameter else {
        notes.push("bolt diameter unknown: the member's bolts".into());
        return member;
    };
    let same = d.bolts.filter(|b| (b.diameter - diameter).abs() < 1e-9);
    let b = Bolts {
        diameter,
        hole: own.hole.or(same.and_then(|b| b.hole)),
        count: own.count,
        shear_planes: {
            let worked = e.planes.as_ref();
            if let (Some(g), Some((w, why))) = (own.shear_planes, worked)
                && g != *w
            {
                notes.push(format!("shear planes: {g} given ({w} worked out: {why})"));
            }
            // A splice's covers count over the member's design data (they
            // are the end's own); a lap's one plane only where nothing
            // else says (as before: the member's, else 1).
            let splice = worked.filter(|_| matches!(e.onto, Some(Onto::Splice)));
            own.shear_planes
                .or(splice.map(|w| w.0))
                .or(d.bolts.map(|b| b.shear_planes))
                .or(worked.map(|w| w.0))
                .unwrap_or(1)
        },
        shear_capacity: own.shear_capacity.or(same.and_then(|b| b.shear_capacity)),
        ultimate: own.ultimate.or(d.bolts.and_then(|b| b.ultimate)),
        holes: layout.or(d.bolts.and_then(|b| b.holes)),
        grade: own.grade.or(same.and_then(|b| b.grade)),
    };
    Some((b, own.shear_capacity_shank, own.threads_in_shear))
}

/// The connection's capacities at one end (`end`: its connection, none:
/// the member's data only), as before for the member's data.
fn end_check(
    d: &MemberDesign,
    end: Option<&EndDesign>,
    s: &Section,
    thickness: Option<f64>,
    fy: f64,
    a: f64,
    onto: Option<&OntoLeg>,
) -> EndCheck {
    let plies = f64::from(d.plies.unwrap_or(1));
    let mut shear = None;
    let mut bearing = None;
    let mut conn: Vec<String> = Vec::new();
    let bolts = end_bolts(d, end, &mut conn);
    match bolts {
        None => {
            if conn.is_empty() {
                conn.push("no bolts given: connection not checked".into());
            }
        }
        // Holes only (the bolts elsewhere): the net section, no bolt check.
        Some((b, _, _)) if b.count == 0 => {
            conn.push("no end bolts: connection not checked".into());
        }
        Some((b, shank, threads_in)) => {
            let gross = std::f64::consts::PI * b.diameter.powi(2) / 4.0;
            // The threads out of the shear plane: the library's capacity
            // for it; without one, as with the threads in (on the safe
            // side, said).
            let out_given = threads_in == Some(false);
            let per_plane = match (shank.filter(|_| out_given), b.shear_capacity, b.ultimate) {
                (Some(v), _, _) => Some(v),
                (None, Some(v), _) => {
                    if out_given {
                        conn.push("bolt shear as with the threads in the shear plane".into());
                    }
                    Some(v)
                }
                (None, None, Some(fub)) => {
                    if out_given {
                        conn.push("bolt shear as with the threads in the shear plane".into());
                    } else if threads_in.is_none() {
                        conn.push("bolt threads in the shear plane".into());
                    }
                    Some(BOLT_SHEAR * fub * ROOT_AREA * gross)
                }
                _ => {
                    conn.push("bolt shear not checked (no capacity)".into());
                    None
                }
            };
            shear = per_plane.map(|v| f64::from(b.count) * f64::from(b.shear_planes) * v);
            // Bearing (4.4): 1.5 f_u of the connected part or of the bolt,
            // the lower.
            let fu = match (s.ultimate, b.ultimate) {
                (Some(m), Some(bolt)) => Some(m.min(bolt)),
                (Some(m), None) => {
                    conn.push("bearing on the member's f_u (the bolt's not given)".into());
                    Some(m)
                }
                (None, _) => None,
            };
            match (fu, thickness) {
                (Some(fu), Some(t)) => {
                    bearing = Some(f64::from(b.count) * 1.5 * fu * b.diameter * t * plies);
                }
                (None, _) => conn.push("bearing not checked (no ultimate strength)".into()),
                (_, None) => conn.push("bearing not checked (no thickness)".into()),
            }
            // And on the leg it bolts onto (4.4), when thinner or weaker.
            if let Some(o) = onto {
                let fu = match (o.fu, b.ultimate) {
                    (Some(m), Some(bolt)) => Some(m.min(bolt)),
                    (m, _) => m,
                };
                if let Some(fu) = fu {
                    let v = f64::from(b.count) * 1.5 * fu * b.diameter * o.thickness;
                    if bearing.is_none_or(|own| v < own) {
                        bearing = Some(v);
                        conn.push(format!(
                            "bearing on {}'s leg ({:.0} mm), the weaker",
                            o.id,
                            o.thickness * 1e3
                        ));
                    }
                }
            }
        }
    }
    let connected_leg = end.and_then(|e| e.leg).or(d.connected_leg);
    let mut notes = Vec::new();
    // The angle's legs (long, short), m.
    let angle = s.profile.as_ref().and_then(|p| match p.shape {
        ohl_sections::Shape::EqualAngle { h, .. } => Some((h, h)),
        ohl_sections::Shape::UnequalAngle { h, b, .. } => Some((h.max(b), h.min(b))),
        _ => None,
    });
    let unequal = angle.is_some_and(|(l, s)| l > s);
    let mut tension = match (bolts, thickness) {
        (None, _) => (a * fy, "gross section, no holes".to_owned()),
        (Some((b, _, _)), Some(t)) => {
            // The hole: given, else a standard hole (4.1); taken as punched
            // (3.10.1).
            let hole = b.hole.unwrap_or_else(|| {
                notes.push("standard hole (bolt + 1.6 mm)".into());
                b.diameter + STANDARD_HOLE
            }) + PUNCHED;
            notes.push("holes punched (+ 1.6 mm)".into());
            let holes = f64::from(b.holes.unwrap_or_else(|| {
                notes.push("one hole per section".into());
                1
            }));
            let leg = match connected_leg {
                Some(c) => c,
                None => {
                    notes.push(if unequal {
                        "one leg bolted, the short one".into()
                    } else {
                        "one leg bolted".into()
                    });
                    if unequal {
                        ConnectedLeg::Short
                    } else {
                        ConnectedLeg::Long
                    }
                }
            };
            // An unequal angle bolted by its short leg: its other leg no
            // larger than the connected one (3.10.2).
            let gross = match (leg, angle) {
                (ConnectedLeg::Short, Some((long, short))) if long > short => {
                    a - (long - short) * t
                }
                _ => a,
            };
            let one_leg = leg.one_leg();
            let net = (gross - holes * hole * t).max(0.0) * fy * if one_leg { 0.9 } else { 1.0 };
            let mut net = (net, "net section".to_owned());
            // Block shear of an angle bolted by one leg (3.10.2, Eq.
            // 3.10-1, Figure 3.5): checked whatever the bolt pattern's
            // centroid, on the safe side.
            if let (true, Some((long, short))) = (one_leg && b.count > 0, angle) {
                let width = if leg == ConnectedLeg::Short {
                    short
                } else {
                    long
                };
                match block_shear(d, end, &b, hole, t, width, fy, s.ultimate) {
                    Ok((p, said)) => {
                        if p < net.0 {
                            net = (p, "block shear".to_owned());
                            if let Some(w) = said {
                                notes.push(w.into());
                            }
                        }
                    }
                    Err(why) => notes.push(format!("block shear not checked ({why})")),
                }
            }
            net
        }
        (Some(_), None) => {
            notes.push("net section not checked (no thickness)".into());
            let one_leg = connected_leg.is_none_or(ConnectedLeg::one_leg);
            (
                a * fy * if one_leg { 0.9 } else { 1.0 },
                "gross section".to_owned(),
            )
        }
    };
    for (v, what) in [(shear, "bolt shear"), (bearing, "bearing")] {
        if let Some(v) = v
            && v < tension.0
        {
            tension = (v, what.to_owned());
        }
    }
    EndCheck {
        shear,
        bearing,
        tension,
        conn,
        tension_notes: notes,
    }
}

/// The minimum end distance, spacing and edge distance of bolts (ASCE
/// 10-97 4.5), m: bolt diameter `d`, connected thickness `t`, its tensile
/// strength `fu`, the force one bolt transmits `p` (none: not known — the
/// force terms left out, said by the caller), a redundant member, a
/// sheared (or flame-cut) edge rather than a rolled one. Holes are taken
/// as punched (Eq. 4.5-3 applies; it does not for drilled holes).
///
/// - end distance (4.5.1): the largest of 1.2 P/(f_u t), 1.3 d and t + d/2
///   (Eqs. 4.5-1 to 4.5-3); a redundant member the larger of t + d/2 and
///   1.2 d (4.5-3, 4.5-4);
/// - spacing along a line of force (4.5.2): 1.2 P/(f_u t) + 0.6 d (4.5-5);
/// - edge distance (4.5.3): 0.85 e_min for a rolled edge (4.5-6), 0.85
///   e_min + 1/16 in. (1.5875 mm) for a sheared one (4.5-7).
pub fn bolt_minimums(
    d: f64,
    t: f64,
    fu: f64,
    p: Option<f64>,
    redundant: bool,
    sheared: bool,
) -> [f64; 3] {
    let force = |p: f64| {
        if fu > 0.0 && t > 0.0 {
            1.2 * p / (fu * t)
        } else {
            0.0
        }
    };
    let end = if redundant {
        (t + d / 2.0).max(1.2 * d)
    } else {
        p.map_or(0.0, force).max(1.3 * d).max(t + d / 2.0)
    };
    let spacing = p.map_or(0.0, force) + 0.6 * d;
    let edge = 0.85 * end + if sheared { 0.0625 * 0.0254 } else { 0.0 };
    [end, spacing, edge]
}

/// The force basis of the bolt tools' minimum distances without an
/// analysis (third round, 2026-11-18; before, the core's
/// `member_ends::bolt_shear_per_bolt`, moved here unchanged): the design
/// shear capacity of one bolt, all its shear planes, N — a given capacity
/// through the shank when the threads are said to be out of the shear
/// plane, else the given capacity, else 4.3.2's design shear stress 0.62
/// f_u of the bolt on the root area (taken as 0.6 of the gross area: the
/// threads in the shear plane, on the safe side); none without either.
/// `shear` and `shank` per plane, N; `fub` Pa; `d` m.
pub fn bolt_shear(
    d: f64,
    planes: u32,
    shear: Option<f64>,
    shank: Option<f64>,
    threads_in: Option<bool>,
    fub: Option<f64>,
) -> Option<f64> {
    let gross = std::f64::consts::PI * d.powi(2) / 4.0;
    let out_given = threads_in == Some(false);
    let per_plane = match (shank.filter(|_| out_given), shear, fub) {
        (Some(v), _, _) => v,
        (None, Some(v), _) => v,
        (None, None, Some(fub)) => BOLT_SHEAR * fub * ROOT_AREA * gross,
        _ => return None,
    };
    Some(per_plane * f64::from(planes))
}

/// Block shear (ASCE 10-97 Eq. 3.10-1): P = 0.60 A_v f_u + A_t f_y, A_v =
/// t (e + (n − 1) s − (n − ½) d_h) along a bolt line of n bolts (the
/// bolts shared out over the gauge lines), A_t = t (c − ½ d_h) from the
/// line nearest the toe to the toe. The end distance and pitch from the
/// end's connection, else the member's; the distance to the toe from the
/// connection's gauges (the leg's width less the largest), else the edge
/// distance (taken to the toe). Several gauge lines: with their gauges,
/// the least of the outer line's block times the lines (the bolts sharing
/// the force equally, 4.1) and the block of all lines (A_t from the inner
/// line to the toe, less the other lines' holes); without them, the outer
/// line's block alone carrying the whole force (on the safe side; said in
/// the second value). Err: what is missing.
#[allow(clippy::too_many_arguments)]
fn block_shear(
    d: &MemberDesign,
    end: Option<&EndDesign>,
    b: &Bolts,
    hole: f64,
    t: f64,
    width: f64,
    fy: f64,
    fu: Option<f64>,
) -> Result<(f64, Option<&'static str>), &'static str> {
    let fu = fu.ok_or("no ultimate strength")?;
    let e = end
        .and_then(|e| e.end_distance)
        .or(d.end_distance)
        .ok_or("no end distance")?;
    let lines = b.holes.unwrap_or(1).max(1);
    let n = b.count.div_ceil(lines).max(1);
    let pitch = if n > 1 {
        end.and_then(|e| e.pitch).or(d.spacing).ok_or("no pitch")?
    } else {
        0.0
    };
    let gauges = end.map(|e| e.gauges.as_slice()).filter(|g| !g.is_empty());
    let toe = match gauges {
        Some(g) => width - g.iter().copied().fold(f64::MIN, f64::max),
        None => end
            .and_then(|e| e.edge_distance)
            .or(d.edge_distance)
            .ok_or("no edge distance")?,
    };
    let n = f64::from(n);
    let av = (t * (e + (n - 1.0) * pitch - (n - 0.5) * hole)).max(0.0);
    let block = |c: f64, holes: f64| 0.60 * av * fu + (t * (c - holes * hole)).max(0.0) * fy;
    let outer = block(toe, 0.5);
    if lines == 1 {
        return Ok((outer, None));
    }
    match gauges.filter(|g| g.len() == lines as usize) {
        Some(g) => {
            let inner = g.iter().copied().fold(f64::MAX, f64::min);
            let all = block(width - inner, f64::from(lines) - 0.5);
            Ok((all.min(outer * f64::from(lines)), None))
        }
        None => Ok((outer, Some("block shear: the outer gauge line alone"))),
    }
}
