// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The members' steel for the 3D view is the Detail view's (owner
//! 2026-11-05): the same corners at each end, the roll and the seat
//! applied, built-ups and channels by their outline, closed solids.

use ohl_structures::LibraryItem;

use crate::joints::{Arrival, JointDetails, joint_details};
use crate::joints_tests::{TOWER, library};
use crate::member_solids::{MemberSolid, member_solids, section_at, steel_start};

fn arrival<'a>(d: &'a JointDetails, joint: &str, member: &str) -> &'a Arrival {
    d.joint(joint)
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == member)
        .unwrap()
}

fn solid<'a>(s: &'a [MemberSolid], member: &str) -> &'a MemberSolid {
    s.iter().find(|x| x.member == member).unwrap()
}

/// Each end of every solid has exactly the corners the close-up draws
/// there (its joint plus [`section_at`] at the end's cut), within 1e-9 m.
fn ends_are_the_close_ups(d: &JointDetails, solids: &[MemberSolid]) {
    for s in solids {
        for k in 0..2 {
            let j = d.joint(&s.joints[k]).unwrap();
            let a = arrival(d, &s.joints[k], &s.member);
            assert_eq!(a.end, k);
            let p = a.profile.as_ref().unwrap();
            let want: Vec<[f64; 3]> = section_at(a, p, steel_start(a))
                .into_iter()
                .map(|q| [0, 1, 2].map(|i| q[i] + j.position[i]))
                .collect();
            assert_eq!(want.len(), s.ends[k].len());
            for (w, h) in want.iter().zip(&s.ends[k]) {
                for i in 0..3 {
                    assert!(
                        (w[i] - h[i]).abs() < 1e-9,
                        "{} end {k}: {w:?} {h:?}",
                        s.member
                    );
                }
            }
        }
    }
}

/// A solid is closed with its normals out: its faces' area vectors sum
/// to nothing and its volume (by the divergence theorem) is its outline's
/// area times its length between the cuts.
fn closed_and_outward(s: &MemberSolid, outline: &[[f64; 2]]) {
    let mut sum = [0.0; 3];
    let mut volume = 0.0;
    for f in &s.faces {
        for (t, _) in &f.triangles {
            let u = [0, 1, 2].map(|i| t[1][i] - t[0][i]);
            let v = [0, 1, 2].map(|i| t[2][i] - t[0][i]);
            let c = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let area = 0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            for (v, n) in sum.iter_mut().zip(f.normal) {
                *v += n * area;
            }
            let mid = [0, 1, 2].map(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.0);
            volume += area * (0..3).map(|i| mid[i] * f.normal[i]).sum::<f64>() / 3.0;
        }
    }
    assert!(sum.iter().all(|v| v.abs() < 1e-9), "{}: {sum:?}", s.member);
    let n = outline.len();
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0;
    let length: f64 = {
        let d = [0, 1, 2].map(|i| s.work[1][i] - s.work[0][i]);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - s.cut[0] - s.cut[1]
    };
    assert!(
        (volume - area * length).abs() < 1e-9 * (1.0 + area * length),
        "{}: volume {volume}, {}",
        s.member,
        area * length
    );
}

/// The diagonal bolted onto the leg at N2 with its steel cut 50 mm short
/// and shifted 20 mm: its solid's end there is the close-up's (seated on
/// the leg, shifted, from the cut), its other end (N4, nothing given) on
/// its axis from the work point; the two ends seated differently, the
/// solid runs straight from one to the other.
#[test]
fn a_seated_cut_end_is_the_close_ups() {
    let text = format!(
        "{TOWER}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nleg = \"long\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\", grade = \"8.8\" }}\n\
         gauges = [\"30 mm\"]\ngauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n\
         cut = \"50 mm\"\nshift = \"20 mm\"\n"
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let solids = member_solids(&d);
    assert_eq!(solids.len(), 3, "L1, L2, D1");
    ends_are_the_close_ups(&d, &solids);
    let s = solid(&solids, "D1");
    let a = arrival(&d, "N2", "D1");
    assert!(
        a.offset.iter().any(|v| v.abs() > 1e-3),
        "seated: {:?}",
        a.offset
    );
    assert_eq!(s.cut[1], 0.05);
    // The far end's section is the near end's moved: offsets differ (N4
    // has none), the solid is straight between them.
    let b = arrival(&d, "N4", "D1");
    assert_ne!(a.offset, b.offset);
    let profile = a.profile.as_ref().unwrap();
    assert_eq!(profile.outline.len(), 6, "an angle's L");
    for i in 1..6 {
        let d0 = [0, 1, 2].map(|k| s.ends[0][i][k] - s.ends[0][0][k]);
        let d1 = [0, 1, 2].map(|k| s.ends[1][i][k] - s.ends[1][0][k]);
        assert!((0..3).all(|k| (d0[k] - d1[k]).abs() < 1e-12));
    }
    closed_and_outward(s, &profile.outline);
    for m in ["L1", "L2"] {
        let s = solid(&solids, m);
        let p = arrival(&d, &s.joints[0], m).profile.clone().unwrap();
        closed_and_outward(s, &p.outline);
    }
}

/// Every profile of the example library (examples/exports/every-profile.ohls):
/// each member's solid by its own outline (the angle's L, the channel's
/// C, the I, the tubes' polygons, the general section's box), turned by
/// its axes — the reference and the roll (m-UA turned 30°) as the
/// exports and the finite-element model take them — and closed.
#[test]
fn every_profile_by_its_outline_turned_by_its_roll() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let text = std::fs::read_to_string(root.join("examples/exports/every-profile.ohls")).unwrap();
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let solids = member_solids(&d);
    ends_are_the_close_ups(&d, &solids);
    let t = ohl_structures::StructureType::parse(std::path::Path::new("t.ohls"), &text).unwrap();
    assert_eq!(solids.len(), t.members.len());
    let model =
        ohl_structures_fe::model::Model::from_structure_type_with(&text, &library()).unwrap();
    for s in &solids {
        let a = arrival(&d, &s.joints[0], &s.member);
        let p = a.profile.as_ref().unwrap();
        closed_and_outward(s, &p.outline);
        // The ring is the outline in the member's axes as the model turns
        // it, the end's offset and the cut along it.
        let m = model.members.iter().find(|m| m.id == s.member).unwrap();
        let axes = crate::member_axes(s.work[0], s.work[1], m.reference, m.roll);
        for (q, c) in p.outline.iter().zip(&s.ends[0]) {
            let rel = [0, 1, 2].map(|k| c[k] - s.work[0][k] - a.offset[k]);
            let y: f64 = (0..3).map(|k| rel[k] * axes[1][k]).sum();
            let z: f64 = (0..3).map(|k| rel[k] * axes[2][k]).sum();
            let x: f64 = (0..3).map(|k| rel[k] * axes[0][k]).sum();
            assert!(
                (y - q[0]).abs() < 1e-9 && (z - q[1]).abs() < 1e-9,
                "{}",
                s.member
            );
            assert!((x - s.cut[0]).abs() < 1e-9);
        }
    }
    let ua = solid(&solids, "m-UA");
    let m = model.members.iter().find(|m| m.id == "m-UA").unwrap();
    assert!((m.roll - 30f64.to_radians()).abs() < 1e-12);
    // Not the unturned axes: the roll shows.
    let flat = crate::member_axes(ua.work[0], ua.work[1], None, 0.0);
    let p = arrival(&d, "N1", "m-UA").profile.clone().unwrap();
    assert!((0..3).any(|k| (p.axes[1][k] - flat[1][k]).abs() > 0.1));
    assert_eq!(solid(&solids, "m-CH").ends[0].len(), 8, "a channel's C");
}

/// The built-ups (two angles back to back, a star, a cross) and a
/// channel: by their built-up outline, the caps cut into triangles that
/// cover it.
#[test]
fn built_ups_and_channels_by_their_outline() {
    let mut text = TOWER.replace("section = \"UA-83x52x6\"", "section = \"2L-83x52x6\"");
    text.push_str(
        "\n[[node]]\nid = \"N5\"\npoint = [\"4 m\", \"1 m\", \"5 m\"]\n\
         \n[[member]]\nid = \"S1\"\nnodes = [\"N4\", \"N5\"]\nsection = \"2S-61x7\"\n\
         \n[[member]]\nid = \"X1\"\nnodes = [\"N2\", \"N5\"]\nsection = \"4X-61x7\"\n\
         \n[[member]]\nid = \"C1\"\nnodes = [\"N1\", \"N5\"]\nsection = \"CH-157\"\nroll = \"90 deg\"\n",
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let solids = member_solids(&d);
    ends_are_the_close_ups(&d, &solids);
    let lib = library();
    for (m, id) in [("D1", "2L-83x52x6"), ("S1", "2S-61x7"), ("X1", "4X-61x7")] {
        let s = solid(&solids, m);
        let a = arrival(&d, &s.joints[0], m);
        let p = a.profile.as_ref().unwrap();
        let shape = lib.section(id).unwrap().shape.clone();
        let want = shape.built_up_outline().unwrap();
        assert_eq!(p.outline.len(), want.len(), "{m}");
        assert!(p.outline.len() > 6, "{m}: more than one angle's L");
        closed_and_outward(s, &p.outline);
    }
    let c = solid(&solids, "C1");
    assert_eq!(c.ends[0].len(), 8);
    let p = arrival(&d, "N1", "C1").profile.clone().unwrap();
    closed_and_outward(c, &p.outline);
}

/// The time to work out the joint details and the solids of a large
/// tower, run by hand: `OHL_SOLIDS_FILE` (a structure file) and
/// `OHL_SOLIDS_SECTIONS` (its section library).
#[test]
#[ignore = "timing, run by hand"]
fn solids_timing() {
    let (Some(file), Some(sections)) = (
        ohl_core::env::var_os("OHL_SOLIDS_FILE"),
        ohl_core::env::var_os("OHL_SOLIDS_SECTIONS"),
    ) else {
        return;
    };
    let text = std::fs::read_to_string(file).unwrap();
    let lib = crate::SectionLibrary::load(std::path::Path::new(&sections)).unwrap();
    let t0 = std::time::Instant::now();
    let d = joint_details(&text, &lib, &Default::default()).unwrap();
    let t1 = std::time::Instant::now();
    let s = member_solids(&d);
    let t2 = std::time::Instant::now();
    let tris: usize = s.iter().map(MemberSolid::triangle_count).sum();
    eprintln!(
        "{} joints, {} solids, {tris} triangles: details {:.0} ms, solids {:.1} ms",
        d.joints.len(),
        s.len(),
        (t1 - t0).as_secs_f64() * 1e3,
        (t2 - t1).as_secs_f64() * 1e3
    );
}
