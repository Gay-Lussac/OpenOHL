// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! SIA 263 member capacities by hand: an angle's buckling about its
//! minor axis with 0.9 L (5.4.1.6), the bending of its eccentric
//! connection (5.4.1.2, formula 50), a class 4 angle by (62)/(63), the
//! tension by (38), (39) and 6.2.4.4 (84), the bolts by Table 16.

use crate::design::{Bolts, ConnectedLeg, Ends, Grade};
use crate::model::{Member, MemberKind, Model, Node, Section};
use crate::sia263::{self, GAMMA_M1, GAMMA_M2, chi};

const E: f64 = 210e9;

fn angle_model(h: f64, t: f64, fy: f64, length: f64) -> (Model, ohl_sections::Properties) {
    let p = ohl_sections::Section {
        id: "L".into(),
        name: String::new(),
        description: String::new(),
        shape: ohl_sections::Shape::EqualAngle { h, t, r: 0.0 },
        material: None,
        given: ohl_sections::Given::default(),
    };
    let q = p.properties();
    let mut s = Section::elastic("L", E, q.area, q.iy);
    s.iz = q.iz;
    s.i_min = q.iv;
    s.j = q.it.unwrap_or(0.0);
    s.g = E / 2.6;
    s.wy = q.wel_y;
    s.wz = q.wel_z;
    s.yield_strength = Some(fy);
    s.ultimate = Some(510e6);
    s.profile = Some(p);
    let mut m = Model {
        sections: vec![s],
        ..Model::default()
    };
    m.nodes = vec![
        Node::pinned("a", [0.0, 0.0, 0.0]),
        Node::pinned("b", [0.0, 0.0, length]),
    ];
    let mut b = Member::new("D", MemberKind::Truss, 0, 1, 0);
    b.design.length_ratios = Some([1.0; 3]);
    b.design.end_eccentricity = Some(Ends::BothEnds);
    b.design.connected_leg = Some(ConnectedLeg::Long);
    b.design.end_distance = Some(0.025);
    b.design.edge_distance = Some(0.030);
    b.design.bolts = Some(Bolts {
        diameter: 0.016,
        hole: None,
        count: 1,
        shear_planes: 1,
        shear_capacity: None,
        ultimate: Some(800e6),
        holes: Some(1),
        grade: Grade::parse("8.8"),
    });
    m.members = vec![b];
    (m, q)
}

/// L 60×6, S 355, 2 m, both ends eccentric: N_K,Rd about v with L_K = 0.9 L
/// on curve c (class 3: 60/6 = 10 within 14 ε); the eccentricity N·e of
/// the bolted leg's gauge (60 − e_2,min 20 = 40 mm from the heel, on its
/// back) in formula (50); the bolt by (73) and bearing (76); tension by
/// (38), (39), (84).
#[test]
fn eccentric_single_angle_by_hand() {
    let (fy, fu) = (355e6, 510e6);
    let (m, q) = angle_model(0.060, 0.006, fy, 2.0);
    let (c, st) = sia263::capacity(&m, 0, None);
    let st = st.expect("stability data");
    // Flexural buckling about v, 0.9 L.
    let lk_v = 0.9 * 2.0;
    let lb = lk_v / (q.iv / q.area).sqrt() / (std::f64::consts::PI * (E / fy).sqrt());
    let nk = chi(lb, 0.49) * fy * q.area / GAMMA_M1;
    assert!((st.n_k - nk).abs() < 1e-6 * nk, "{} {nk}", st.n_k);
    assert!(!st.class4);
    // Bolt M16 8.8 in shear: 0.6 f_ub · 0.6 A / γ_M2 (stress area as 0.6
    // of the gross, said); bearing 0.85 e_1/d_0 f_u d t / γ_M2, d_0 18.
    let a16 = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
    let shear = 0.6 * 800e6 * 0.6 * a16 / GAMMA_M2;
    let bearing = f64::min(
        0.85 * 0.025 / 0.018 * fu * 0.016 * 0.006,
        2.4 * fu * 0.016 * 0.006,
    ) / GAMMA_M2;
    let comp = c.compression.as_ref().unwrap();
    assert!(
        (comp.0 - nk.min(shear).min(bearing)).abs() < 1e-6,
        "{comp:?}"
    );
    // Tension: (84) one bolt, e_2 30: 0.9 f_u (2 e_2 − d_0) t / γ_M2.
    let one = 0.9 * fu * (2.0 * 0.030 - 0.018) * 0.006 / GAMMA_M2;
    let net = 0.9 * fu * (q.area - 0.018 * 0.006) / GAMMA_M2;
    let gross = fy * q.area / GAMMA_M1;
    let t = c.tension.as_ref().unwrap();
    let want = one.min(net).min(gross).min(shear).min(bearing);
    assert!((t.0 - want).abs() < 1e-6, "{t:?} {want}");
    // The eccentricity: the gauge the farthest Table 17 allows (40 mm),
    // on the back of the leg along y (the long one of an equal angle).
    let (dy, dz) = (0.040 - q.centroid[0], -q.centroid[1]);
    let th = q.principal_angle;
    let (eu, ev) = (
        dy * th.cos() + dz * th.sin(),
        -dy * th.sin() + dz * th.cos(),
    );
    assert!(st.eccentric);
    assert!((st.axes[0].e - ev).abs() < 1e-12 && (st.axes[1].e - eu).abs() < 1e-12);
    assert!(
        c.compression_assumed
            .iter()
            .any(|a| a.contains("gauge the farthest Table 17 allows")),
        "{:?}",
        c.compression_assumed
    );
    // Formula (50) at N = 20 kN by hand, ω = 1 (uniform N·e).
    let n = 20e3;
    let mut want = n / st.n_k;
    for ax in &st.axes {
        let m_ed = n * ax.e.abs();
        let mrd = match ax.lt {
            Some((s0, a)) => {
                let lbd = (fy / s0 * ax.w / ax.w_el).sqrt();
                sia263::chi_lt(lbd, a) * ax.m_rd
            }
            None => ax.m_rd,
        };
        want += m_ed / ((1.0 - n / ax.n_cr) * mrd);
    }
    let (u, words) = st.usage(-n, [[0.0; 2]; 2], 1.0);
    assert!((u - want).abs() < 1e-12, "{u} {want} {words}");
    assert!(u > n / st.n_k * 1.5, "the eccentricity matters: {u}");
    assert!(words.contains("formula 50"), "{words}");
    // Concentric (both legs bolted): no moment, buckling over L about v.
    let mut m2 = m.clone();
    m2.members[0].design.end_eccentricity = Some(Ends::None);
    m2.members[0].design.connected_leg = Some(ConnectedLeg::Both);
    let (_, st2) = sia263::capacity(&m2, 0, None);
    let st2 = st2.unwrap();
    assert!(!st2.eccentric && st2.n_k < st.n_k);
}

/// L 100×6, S 355: b/t 16.7 above 14 ε = 11.4, class 4. A_eff from each
/// leg's ρ (outstand, k 0.426, b from the theoretical line h − t/2);
/// compression by (62)/(63) with the bow of curve c (L/200).
#[test]
fn class_4_angle_by_hand() {
    let fy = 355e6;
    let (h, t) = (0.100, 0.006);
    let (m, q) = angle_model(h, t, fy, 2.0);
    let (c, st) = sia263::capacity(&m, 0, None);
    let st = st.unwrap();
    assert!(st.class4, "{c:?}");
    let b = h - t / 2.0;
    let rho = sia263::rho(b / t, 0.426, fy);
    assert!(rho < 1.0);
    let a_eff = q.area - 2.0 * (1.0 - rho) * b * t;
    assert!((st.a_eff - a_eff).abs() < 1e-12, "{} {a_eff}", st.a_eff);
    // The weaker of the member and its one bolt governs the axial check.
    assert!(c.compression.unwrap().0 <= st.n_k);
    let n = 30e3;
    let (u, words) = st.usage(-n, [[0.0; 2]; 2], 1.0);
    // By hand: both formulas, the larger.
    let mut worst: f64 = 0.0;
    for k in 0..2 {
        let mut sigma = n / st.a_eff;
        for (a, ax) in st.axes.iter().enumerate() {
            let amp = if a == k {
                1.0 / (1.0 - n / ax.n_cr)
            } else {
                1.0
            };
            sigma += amp * n * (ax.e.abs() + ax.w0) / ax.w;
        }
        worst = worst.max(sigma / (fy / GAMMA_M1));
    }
    assert!((u - worst).abs() < 1e-12, "{u} {worst} {words}");
    // The bow about v: L/200 over the bar (2 m).
    assert!((st.axes[1].w0 - 2.0 / 200.0).abs() < 1e-12);
}

/// The method by name, its notes and the bolt rules of Table 17.
#[test]
fn method_by_name_and_bolt_rules() {
    use crate::model::CapacityMethod;
    assert_eq!(
        CapacityMethod::parse("sia263-2013"),
        Ok(CapacityMethod::Sia263)
    );
    assert_eq!(CapacityMethod::Sia263.as_str(), "sia263-2013");
    let (mut m, _) = angle_model(0.060, 0.006, 355e6, 2.0);
    m.analysis.capacity = CapacityMethod::Sia263;
    let r = crate::bolt_rules::rules(&m, 0, 0, Some(10e3)).expect("rules");
    assert_eq!((r.end, r.spacing, r.edge), (0.025, 0.040, 0.020));
    assert_eq!(r.code, "SIA 263:2013");
}

/// The angle bolted at its top end onto a leg of 4 mm (thinner than its
/// 6 mm): the bearing (76) on that leg, with this member's e_1, governs:
/// min(0.85 e_1/d_0, 2.4) f_u d t / γ_M2 on t = 4 mm (2026-11-05).
#[test]
fn bearing_on_the_thinner_leg_bolted_onto_by_hand() {
    use crate::design::{EndDesign, Onto};
    let (mut m, _) = angle_model(0.060, 0.006, 355e6, 2.0);
    m.nodes.push(Node::pinned("c", [0.0, 0.0, 4.0]));
    let mut leg = Member::new("G", MemberKind::Truss, 1, 2, 0);
    leg.design.thickness = Some(0.004);
    m.members.push(leg);
    m.members[0].design.ends[1] = Some(EndDesign {
        onto: Some(Onto::Member(1)),
        ..EndDesign::default()
    });
    let (c, _) = sia263::capacity(&m, 0, None);
    let d0 = sia263::standard_hole(0.016).unwrap();
    let fu = 510e6;
    let one = |t: f64| (0.85 * 0.025 / d0 * fu * 0.016 * t).min(2.4 * fu * 0.016 * t) / GAMMA_M2;
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - one(0.004)).abs() < 1e-6, "{t} {what} {}", one(0.004));
    assert_eq!(what, "bearing at b");
    assert!(one(0.004) < one(0.006));
}
