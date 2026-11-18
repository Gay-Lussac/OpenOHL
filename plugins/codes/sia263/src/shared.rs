// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the SIA 263 check took from OpenOHL's ASCE 10-97 code in the core
//! (`ohl_structures_fe::asce10` before 2026-11-18), copied unchanged: the
//! capacities' record, the bolts at a member's end, and torsional and
//! torsional-flexural buckling by elastic theory.

use crate::model::{Bolts, EndDesign, MemberDesign, Onto, Section};

/// The capacities of one member and what they rest on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Capacity {
    /// Compression capacity, N, and what governs it in words.
    pub compression: Option<(f64, String)>,
    /// Tension capacity, N, and what governs it in words.
    pub tension: Option<(f64, String)>,
    /// The slenderness (the largest λ_K).
    pub kl_r: Option<f64>,
    /// What the compression capacity assumed for missing data, in words.
    pub compression_assumed: Vec<String>,
    /// What the tension capacity assumed for missing data, in words.
    pub tension_assumed: Vec<String>,
    /// Slenderness above the class's limit, in words.
    pub limit: Option<String>,
    /// For a member with connections: per end, its conditions and where
    /// each comes from, in words; empty without.
    pub joints: Vec<String>,
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

/// The torsional radius of gyration r_t (ASCE 10-97 Eq. 3.9-2): √((C_w +
/// 0.04 J (K_t L)²)/I_ps).
pub fn torsional_radius(cw: f64, j: f64, ips: f64, kt_l: f64) -> f64 {
    ((cw + 0.04 * j * kt_l * kt_l) / ips).sqrt()
}

/// The equivalent radius of gyration for torsional-flexural buckling r_tf
/// (ASCE 10-97 Eq. 3.8-1).
pub fn torsional_flexural_radius(rt: f64, ru: f64, u0: f64, rps: f64) -> f64 {
    let (a, b) = (1.0 / (rt * rt), 1.0 / (ru * ru));
    let c = u0 / (rt * ru * rps);
    let sum = a + b + ((a - b).powi(2) + 4.0 * c * c).sqrt();
    (2.0 / sum).sqrt()
}

/// The slenderness of torsional or torsional-flexural buckling of the
/// section's profile over `length`, with the mode in words; none where
/// only flexural buckling applies (single angles, closed sections) and
/// where the profile is not known (`Err` for a `general` profile). The
/// member's ends free to warp; bending about the axis of symmetry by the
/// member's length ratio for it.
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
