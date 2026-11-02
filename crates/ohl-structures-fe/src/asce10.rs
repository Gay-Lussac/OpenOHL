// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Member capacities by **ASCE 10-97** (*Design of Latticed Steel
//! Transmission Structures*, American Society of Civil Engineers, 1998),
//! metric form — the default capacity method of the structure check
//! (owner, 2026-10-27). Built, in active development, not approved yet.
//!
//! Compression:
//!
//! - L/r = max(r_y·L/r_y, r_z·L/r_z, r_v·L/r_v) with the member's length
//!   ratios ([`crate::design::MemberDesign::length_ratios`], else its `k`
//!   on all three), L its `lk` when given, else its length; r_y, r_z the
//!   radii of gyration about the section's y and z, r_v the minor
//!   principal one.
//! - Effective slenderness KL/r: L/r ≤ 120 by the ends' eccentricity —
//!   none: L/r; one end: 30 + 0.75 L/r; both ends: 60 + 0.5 L/r; L/r > 120
//!   by the ends' rotational restraint — none: L/r; one end: 28.6 + 0.762
//!   L/r; both ends: 46.2 + 0.615 L/r.
//! - Local buckling of an angle's legs: (w/t)_lim = 209.6/√f_y (MPa);
//!   above it F_cr = (1.677 − 0.677 (w/t)/(w/t)_lim) f_y up to w/t =
//!   378/√f_y, beyond F_cr = 0.0332 π² E/(w/t)²; else F_cr = f_y.
//! - Column curve: C_c = π √(2E/F_cr); F_a = [1 − ½ (KL/r / C_c)²] F_cr
//!   for KL/r ≤ C_c, else π² E/(KL/r)²; P = F_a A.
//! - The capacity is the least of P and the connection's bolt shear and
//!   bearing.
//!
//! Tension: the least of the net section (A − holes·d_hole·t)·f_y, × 0.9
//! for an angle bolted by one leg; the bolts' shear (count × shear planes
//! × each one's shear capacity); bearing (count × 1.5 f_u d t × plies).
//!
//! **At each end** (2026-11-02, stage 4.1 and 4.2 of the joint details):
//! the ends' eccentricity and restraint are each end's
//! ([`crate::member_ends`]: given at the end's `[[connection]]`, the
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
//! without a root radius when it has none, one leg bolted, a hole 2 mm
//! larger than the bolt, one hole per section, the bolts' threads in the
//! shear plane (0.62 f_ub on 0.75 of the gross area) when only their
//! strength is given (with the threads given out of the plane but no
//! capacity for it: as with them in). Without bolts, the connection is not checked and
//! the tension is on the gross section (said so). Local buckling is
//! checked on angles (and on sections given a `width_thickness`) only.

use ohl_structures::connections::ConnectionKind;

use crate::design::{Bolts, ConnectedLeg, EndDesign, Ends, MemberClass, MemberDesign};
use crate::member_ends::{self, Source};
use crate::model::{Member, Model, Section};

/// The method's name in files and results.
pub const NAME: &str = "asce10-97";

/// A hole this much larger than its bolt when not given, m.
const HOLE_CLEARANCE: f64 = 0.002;

/// Bolt shear stress over the bolt's ultimate strength (threads in the
/// shear plane: on the root area, taken as this share of the gross).
const BOLT_SHEAR: f64 = 0.62;
const ROOT_AREA: f64 = 0.75;

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

/// The stress replacing f_y for a leg of width/thickness `w_t` (local
/// buckling), Pa; `fy`, `e` in Pa.
pub fn local_buckling_stress(fy: f64, e: f64, w_t: f64) -> f64 {
    let fy_mpa = fy / 1e6;
    let lim = 209.6 / fy_mpa.sqrt();
    if w_t <= lim {
        fy
    } else if w_t <= 378.0 / fy_mpa.sqrt() {
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

/// The ASCE 10-97 capacities of member `idx`. `partner_in_tension`: for a
/// crossing diagonal with a partner, whether that partner is in tension
/// in this load case (none: no partner, or not a crossing diagonal).
pub fn capacity(model: &Model, idx: usize, partner_in_tension: Option<bool>) -> Capacity {
    let m: &Member = &model.members[idx];
    let s = &model.sections[m.section];
    let d = &m.design;
    let mut out = Capacity::default();
    let Some(fy) = s.yield_strength else {
        return out;
    };
    let e = s.e;
    let a = s.area;
    let mut assumed: Vec<String> = Vec::new();
    // Slenderness.
    let length = m.lk.unwrap_or_else(|| model.length(m));
    let ratios = match d.length_ratios {
        Some(r) => r,
        None => {
            if m.lk.is_none() && m.k == 1.0 {
                assumed.push("length ratios 1".into());
            }
            let k = if m.lk.is_some() { 1.0 } else { m.k };
            [k; 3]
        }
    };
    let r = |i: f64| (i / a).sqrt();
    let radii = [r(s.iy), r(s.iz), r(s.i_min)];
    let mut l_r = (0..3)
        .filter(|&k| radii[k] > 0.0)
        .map(|k| ratios[k] * length / radii[k])
        .fold(0.0, f64::max);
    let crossing = d.class == Some(MemberClass::Crossing);
    if crossing {
        let geometric = radii[0].min(radii[1]);
        let unsupported = if geometric > 0.0 {
            model.length(m) / geometric
        } else {
            0.0
        };
        match partner_in_tension {
            Some(true) => {}
            Some(false) => {
                if unsupported > l_r {
                    l_r = unsupported;
                    assumed.push("crossing partner not in tension: unbraced out of plane".into());
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
    // The ends' eccentricity and restraint, end by end (crate::member_ends).
    let ends = member_ends::end_conditions(model, idx);
    let at = |end: usize| model.nodes[if end == 0 { m.i } else { m.j }].id.as_str();
    let assumed_at = |c: &[member_ends::Condition; 2]| -> [bool; 2] {
        [0, 1].map(|e| c[e].source == Source::Assumed)
    };
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
    }
    let kl_r = effective_slenderness(l_r, ends.eccentricity, ends.restraint);
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
    let fcr = match s.compression {
        // A given allowable compression before buckling replaces A·F_cr.
        Some(c) => c / a,
        None => match w_t {
            Some(w) => local_buckling_stress(fy, e, w),
            None => fy,
        },
    };
    let column = column_stress(fcr, e, kl_r) * a;
    let mut comp = (column, format!("KL/r {kl_r:.0}"));
    // The connection, at each end: the end's own data over the member's.
    let thickness = d.thickness.or(legs.map(|l| l.1));
    let checks = [0, 1].map(|end| end_check(d, d.ends[end].as_ref(), s, thickness, fy, a));
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
            let say = |c: &member_ends::Condition, yes: &str, no: &str| {
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
fn end_bolts(
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
    let Some(own) = e.bolts else {
        if let Some(k) = e.kind
            && k != ConnectionKind::Bolted
        {
            notes.push(format!("{}: no bolts", k.as_str()));
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
        shear_planes: own
            .shear_planes
            .or(d.bolts.map(|b| b.shear_planes))
            .unwrap_or(1),
        shear_capacity: own.shear_capacity.or(same.and_then(|b| b.shear_capacity)),
        ultimate: own.ultimate.or(d.bolts.and_then(|b| b.ultimate)),
        holes: layout.or(d.bolts.and_then(|b| b.holes)),
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
            match (s.ultimate, thickness) {
                (Some(fu), Some(t)) => {
                    bearing = Some(f64::from(b.count) * 1.5 * fu * b.diameter * t * plies);
                }
                (None, _) => conn.push("bearing not checked (no ultimate strength)".into()),
                (_, None) => conn.push("bearing not checked (no thickness)".into()),
            }
        }
    }
    let connected_leg = end.and_then(|e| e.leg).or(d.connected_leg);
    let mut notes = Vec::new();
    let mut tension = match (bolts, thickness) {
        (None, _) => (a * fy, "gross section, no holes".to_owned()),
        (Some((b, _, _)), Some(t)) => {
            let hole = b.hole.unwrap_or_else(|| {
                notes.push("hole 2 mm larger than the bolt".into());
                b.diameter + HOLE_CLEARANCE
            });
            let holes = f64::from(b.holes.unwrap_or_else(|| {
                notes.push("one hole per section".into());
                1
            }));
            let one_leg = match connected_leg {
                Some(c) => c.one_leg(),
                None => {
                    notes.push("one leg bolted".into());
                    true
                }
            };
            let net = (a - holes * hole * t).max(0.0) * fy * if one_leg { 0.9 } else { 1.0 };
            (net, "net section".to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 210e9;

    /// A crossing diagonal braced at mid-length (length ratios 0.5): with
    /// its partner in tension its L/r is half its length over r_v; with
    /// the partner compressed (or no partner) at least its whole length
    /// over its smaller geometric radius, said in its assumptions. Each
    /// missing datum is named; given data are not.
    #[test]
    fn crossing_diagonal_and_assumptions() {
        use crate::model::{Member, MemberKind, Node, Section};
        let mut s = Section::elastic("s", E, 10e-4, 50e-8);
        s.i_min = 20e-8;
        s.yield_strength = Some(355e6);
        let mut m = Model {
            sections: vec![s],
            ..Model::default()
        };
        m.nodes = vec![
            Node::pinned("a", [0.0, 0.0, 0.0]),
            Node::pinned("b", [3.0, 0.0, 3.0]),
            Node::pinned("c", [3.0, 0.0, 0.0]),
            Node::pinned("d", [0.0, 0.0, 3.0]),
        ];
        let mut d1 = Member::new("d1", MemberKind::Truss, 0, 1, 0);
        d1.design.class = Some(MemberClass::Crossing);
        d1.design.crossing = Some(1);
        d1.design.length_ratios = Some([0.5; 3]);
        d1.design.end_eccentricity = Some(Ends::BothEnds);
        m.members = vec![d1, Member::new("d2", MemberKind::Truss, 2, 3, 0)];
        let l = 18f64.sqrt();
        let (ry, rv) = ((50e-8f64 / 10e-4).sqrt(), (20e-8f64 / 10e-4).sqrt());
        let braced = capacity(&m, 0, Some(true));
        let lr = 0.5 * l / rv;
        assert!(
            (braced.kl_r.unwrap() - effective_slenderness(lr, Ends::BothEnds, Ends::None)).abs()
                < 1e-9
        );
        assert!(
            !braced
                .compression_assumed
                .iter()
                .any(|a| a.contains("crossing"))
        );
        let free = capacity(&m, 0, Some(false));
        let lr = l / ry;
        assert!(
            (free.kl_r.unwrap() - effective_slenderness(lr, Ends::BothEnds, Ends::None)).abs()
                < 1e-9
        );
        assert!(
            free.compression_assumed
                .iter()
                .any(|a| a.contains("not in tension")),
            "{free:?}"
        );
        assert!(free.compression.unwrap().0 < braced.compression.unwrap().0);
        // The partner without data: every assumption named.
        let bare = capacity(&m, 1, None);
        for a in ["length ratios 1", "both ends eccentric", "no bolts given"] {
            assert!(
                bare.compression_assumed.iter().any(|x| x.contains(a)),
                "{a}: {bare:?}"
            );
        }
        assert_eq!(bare.tension.as_ref().unwrap().1, "gross section, no holes");
        // Bolts counted 0: the holes only — the net section, no bolt check.
        m.members[1].design.bolts = Some(crate::design::Bolts {
            diameter: 0.016,
            hole: Some(0.0175),
            count: 0,
            shear_planes: 1,
            shear_capacity: Some(60e3),
            ultimate: None,
            holes: Some(1),
        });
        m.members[1].design.thickness = Some(0.006);
        m.members[1].design.connected_leg = Some(crate::design::ConnectedLeg::Both);
        let holes = capacity(&m, 1, None);
        let net = (10e-4 - 0.0175 * 0.006) * 355e6;
        assert!(
            (holes.tension.as_ref().unwrap().0 - net).abs() < 1e-6,
            "{holes:?}"
        );
        assert!(
            holes
                .tension_assumed
                .iter()
                .any(|a| a.contains("no end bolts"))
        );
        // No yield strength: no capacity (the check falls back).
        m.sections[0].yield_strength = None;
        assert!(capacity(&m, 0, None).compression.is_none());
    }

    /// The six curves at L/r 100 and 150 (hand values).
    #[test]
    fn effective_slenderness_curves() {
        assert_eq!(effective_slenderness(100.0, Ends::None, Ends::None), 100.0);
        assert_eq!(
            effective_slenderness(100.0, Ends::OneEnd, Ends::None),
            105.0
        );
        assert_eq!(
            effective_slenderness(100.0, Ends::BothEnds, Ends::None),
            110.0
        );
        assert_eq!(
            effective_slenderness(150.0, Ends::BothEnds, Ends::None),
            150.0
        );
        assert!((effective_slenderness(150.0, Ends::None, Ends::OneEnd) - 142.9).abs() < 1e-9);
        assert!((effective_slenderness(150.0, Ends::None, Ends::BothEnds) - 138.45).abs() < 1e-9);
    }

    /// Local buckling for f_y 355 MPa: (w/t)_lim = 11.12, the linear
    /// range to 20.06, then elastic.
    #[test]
    fn local_buckling() {
        let fy = 355e6;
        assert_eq!(local_buckling_stress(fy, E, 10.0), fy);
        let lim = 209.6 / 355f64.sqrt();
        let f = local_buckling_stress(fy, E, 14.0);
        assert!((f - (1.677 - 0.677 * 14.0 / lim) * fy).abs() < 1.0);
        let f = local_buckling_stress(fy, E, 25.0);
        assert!((f - 0.0332 * std::f64::consts::PI.powi(2) * E / 625.0).abs() < 1.0);
    }

    /// The column curve: C_c for f_y 355 MPa is 108.06; at KL/r 80
    /// F_a = (1 − ½(80/107.4)²) f_y; at 150 the Euler stress.
    #[test]
    fn column_curve() {
        let fy = 355e6;
        let cc = std::f64::consts::PI * (2.0 * E / fy).sqrt();
        assert!((cc - 108.06).abs() < 0.01, "{cc}");
        let f = column_stress(fy, E, 80.0);
        assert!((f - (1.0 - 0.5 * (80.0 / cc).powi(2)) * fy).abs() < 1.0);
        let f = column_stress(fy, E, 150.0);
        assert!((f - std::f64::consts::PI.powi(2) * E / 22500.0).abs() < 1.0);
    }
}
