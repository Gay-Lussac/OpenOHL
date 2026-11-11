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
    closed_with_area(s, area_of(outline));
}

/// An outline's area.
fn area_of(outline: &[[f64; 2]]) -> f64 {
    let n = outline.len();
    (0..n)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

/// The same for a solid of steel area `area` in its section (a built-up
/// section's: its parts' together).
fn closed_with_area(s: &MemberSolid, area: f64) {
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

/// The built-ups (two angles back to back, a star, a cross) by their
/// parts (2026-11-10): a closed prism per angle, their steel the parts'
/// area (not the filled outline's), the member's size still its outer
/// outline's; a channel by its outline, the caps cut into triangles that
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
        assert_eq!(s.ends[0].len(), want.len(), "{m}");
        let parts = shape.built_up_parts().unwrap();
        assert_eq!(p.parts.len(), parts.len(), "{m}");
        assert_eq!(s.parts.len(), parts.len(), "{m}");
        assert_eq!(s.rings().count(), parts.len(), "{m}");
        let steel: f64 = p.pieces().map(area_of).sum();
        assert!(steel < area_of(&p.outline), "{m}: the gap is open");
        closed_with_area(s, steel);
        // Each part where the close-up draws it.
        for (part, ring) in p.parts.iter().zip(&s.parts) {
            let j = d.joint(&s.joints[0]).unwrap();
            let want = crate::outline_at(a, p, part, steel_start(a));
            for (w, h) in want.iter().zip(&ring[0]) {
                for i in 0..3 {
                    assert!((w[i] + j.position[i] - h[i]).abs() < 1e-9, "{m}");
                }
            }
        }
    }
    let c = solid(&solids, "C1");
    assert_eq!(c.ends[0].len(), 8);
    let p = arrival(&d, "N1", "C1").profile.clone().unwrap();
    closed_and_outward(c, &p.outline);
}

/// Two angles in a star (owner 2026-11-10: "the double in X corners are
/// not well drafted as they go into another"): the member's solid is two
/// separate angles heel to heel, no point of its section in both, the
/// gusset's gap between their heels open; its face view two outlines;
/// seen from the side two bands with the gap between them.
#[test]
fn a_star_is_two_angles_that_do_not_overlap() {
    let mut text = TOWER.to_owned();
    text.push_str(
        "\n[[node]]\nid = \"N5\"\npoint = [\"4 m\", \"1 m\", \"5 m\"]\n\
         \n[[member]]\nid = \"S1\"\nnodes = [\"N4\", \"N5\"]\nsection = \"2S-61x7\"\n",
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let solids = member_solids(&d);
    let s = solid(&solids, "S1");
    let a = arrival(&d, "N4", "S1");
    let p = a.profile.as_ref().unwrap();
    assert_eq!((p.parts.len(), s.parts.len()), (2, 2));
    assert!(p.parts.iter().all(|o| o.len() == 6), "each an angle's L");
    let inside = |o: &[[f64; 2]], q: [f64; 2]| -> bool {
        let mut odd = false;
        for k in 0..o.len() {
            let (a, c) = (o[k], o[(k + 1) % o.len()]);
            if (a[1] > q[1]) != (c[1] > q[1])
                && q[0] < a[0] + (q[1] - a[1]) / (c[1] - a[1]) * (c[0] - a[0])
            {
                odd = !odd;
            }
        }
        odd
    };
    // Over the section's box (about the member's axis): no point in both
    // angles, each angle holding some, the axis (between the heels) in
    // neither.
    let half = p
        .outline
        .iter()
        .map(|q| q[0].abs().max(q[1].abs()))
        .fold(0.0, f64::max);
    let n = 121;
    let mut held = [0usize; 2];
    for i in 0..n {
        for k in 0..n {
            let q = [
                half * (2.0 * (i as f64 + 0.5) / n as f64 - 1.0),
                half * (2.0 * (k as f64 + 0.5) / n as f64 - 1.0),
            ];
            let hit = [inside(&p.parts[0], q), inside(&p.parts[1], q)];
            assert!(!(hit[0] && hit[1]), "{q:?} in both angles");
            held[0] += usize::from(hit[0]);
            held[1] += usize::from(hit[1]);
        }
    }
    assert!(held[0] > 0 && held[0] == held[1], "{held:?}");
    assert!(!p.parts.iter().any(|o| inside(o, [0.0, 0.0])));
    // The heels 11 mm apart (the gusset's gap) in both directions.
    let heel = |o: &[[f64; 2]]| {
        *o.iter()
            .min_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])))
            .unwrap()
    };
    let (h0, h1) = (heel(&p.parts[0]), heel(&p.parts[1]));
    assert!((h0[0] - h1[0] - 0.011).abs() < 1e-9 && (h0[1] - h1[1] - 0.011).abs() < 1e-9);
    // The face view: an outline per angle.
    let j = d.joint("N4").unwrap();
    let members = |marks: &[crate::Mark]| {
        marks
            .iter()
            .filter(|m| {
                matches!(
                    m,
                    crate::Mark::Line {
                        kind: crate::LineKind::Member,
                        closed: true,
                        ..
                    }
                )
            })
            .count()
    };
    let with = members(&crate::face_view(j, "y+").marks);
    let singles = j.arrivals.iter().filter(|a| a.member != "S1").count();
    assert_eq!(with, singles + 2);
    // From the side: two bands, the gap between them, its cuts the
    // solid's.
    let e = crate::elevation(&crate::member_ends(&d, "S1").unwrap()).unwrap();
    assert_eq!(e.bands.len(), 2);
    assert_eq!(e.section, p.parts);
    assert_eq!(e.cuts, s.cut);
    let (lo, hi) = if e.bands[0][0] < e.bands[1][0] {
        (e.bands[0], e.bands[1])
    } else {
        (e.bands[1], e.bands[0])
    };
    assert!((hi[0] - lo[1] - 0.011).abs() < 1e-9, "{lo:?} {hi:?}");
    // A single angle: one piece, its outline.
    let l = crate::elevation(&crate::member_ends(&d, "L1").unwrap()).unwrap();
    assert_eq!((l.bands.len(), l.section.len()), (1, 1));
}

/// A point (y, z) inside an outline.
fn inside_outline(o: &[[f64; 2]], q: [f64; 2]) -> bool {
    let mut odd = false;
    for k in 0..o.len() {
        let (a, c) = (o[k], o[(k + 1) % o.len()]);
        if (a[1] > q[1]) != (c[1] > q[1])
            && q[0] < a[0] + (q[1] - a[1]) / (c[1] - a[1]) * (c[0] - a[0])
        {
            odd = !odd;
        }
    }
    odd
}

/// A brace bolted onto a built-up leg (owner 2026-11-11: "Go ahead with
/// the braces"; before, it ran uncut to the work point through the leg):
/// onto two angles in a star and onto four in a cross it is seated on the
/// angle facing it (shifted off its work line, lying on a face of that
/// angle's leg), cut short of the work point, and its steel — the middle
/// of both its legs' thickness, from its cut end 0.6 m along it — lies in
/// none of the leg's angles; its solid's end is the close-up's. The same
/// brace whatever its roll (four quarter turns) and from either side.
#[test]
fn a_brace_is_seated_and_cut_against_a_built_up_leg() {
    for leg in ["2S-61x7", "4X-61x7"] {
        for roll in [0, 90, 180, 270] {
            for x in ["4 m", "-4 m"] {
                let text = TOWER
                    .replace("section = \"EA-61x7\"", &format!("section = \"{leg}\""))
                    .replace(
                        "point = [\"4 m\", \"1 m\", \"0 m\"]",
                        &format!("point = [\"{x}\", \"1 m\", \"0 m\"]"),
                    )
                    .replace(
                        "section = \"UA-83x52x6\"",
                        &format!("section = \"UA-83x52x6\"\nroll = \"{roll} deg\""),
                    );
                let what = format!("{leg}, roll {roll}°, from x = {x}");
                let d = joint_details(&text, &library(), &Default::default()).unwrap();
                assert!(d.level3, "{what}: {:?}", d.notes);
                let a = arrival(&d, "N2", "D1");
                let o = arrival(&d, "N2", a.onto.value.as_deref().unwrap());
                let q = o.profile.as_ref().unwrap();
                assert!(q.angle.is_none() && q.part_angles.len() >= 2, "{what}");
                let p = a.profile.as_ref().unwrap();
                let g = p.angle.unwrap();
                // Seated: moved off its work line, on a face of the leg.
                assert!(a.sits.value.is_some(), "{what}: not seated");
                let off = a.offset.iter().map(|v| v * v).sum::<f64>().sqrt();
                assert!(off > 1e-4 && off < 0.2, "{what}: offset {off}");
                // Cut short, within half its length.
                let cut = a.cut.value.unwrap();
                assert!(cut > 0.0 && cut <= a.length / 2.0, "{what}: cut {cut}");
                assert_eq!(a.cut.source, crate::Source::WorkedOut, "{what}");
                assert!(!a.misfit, "{what}: {:?}", a.notes);
                // Its steel in none of the leg's angles.
                let mut samples: Vec<[f64; 2]> = Vec::new();
                let n = 12;
                for k in 0..=n {
                    let f = k as f64 / n as f64;
                    samples.push([
                        g.heel[0] + g.t / 2.0 + f * (g.b - g.t / 2.0),
                        g.heel[1] + g.t / 2.0,
                    ]);
                    samples.push([
                        g.heel[0] + g.t / 2.0,
                        g.heel[1] + g.t / 2.0 + f * (g.h - g.t / 2.0),
                    ]);
                }
                for step in 0..=60 {
                    // From 1 mm past its cut end (the end itself touches).
                    let along = steel_start(a) + 0.001 + step as f64 * 0.01;
                    for (k, at) in crate::outline_at(a, p, &samples, along).iter().enumerate() {
                        let yz =
                            [0, 1].map(|i| (0..3).map(|c| at[c] * q.axes[i + 1][c]).sum::<f64>());
                        for part in &q.parts {
                            assert!(
                                !inside_outline(part, yz),
                                "{what}: its steel (sample {k}, {along:.2} m along) in the leg"
                            );
                        }
                    }
                }
                let solids = member_solids(&d);
                ends_are_the_close_ups(&d, &solids);
                let s = solid(&solids, "D1");
                assert!((s.cut[a.end] - cut).abs() < 1e-12, "{what}");
            }
        }
    }
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
    // The ends bolting onto a built-up member (seated on its angle
    // facing them since 2026-11-11): how many, seated, cut, the largest
    // cut, not fitting, off its face; the members with no steel left
    // between their cuts.
    let (mut onto, mut seated, mut cut, mut largest, mut misfit, mut off) = (0, 0, 0, 0.0, 0, 0);
    for j in &d.joints {
        for a in &j.arrivals {
            let built_up = a
                .onto
                .value
                .as_ref()
                .and_then(|o| j.arrivals.iter().find(|b| &b.member == o))
                .and_then(|b| b.profile.as_ref())
                .is_some_and(|q| !q.part_angles.is_empty());
            if !built_up || a.profile.as_ref().is_none_or(|p| p.angle.is_none()) {
                continue;
            }
            onto += 1;
            seated += usize::from(a.offset != [0.0; 3]);
            let c = a.cut.value.unwrap_or(0.0);
            cut += usize::from(c > 0.0);
            largest = f64::max(largest, c);
            misfit += usize::from(a.misfit);
            off += usize::from(a.flat.is_some_and(|f| f.off > crate::FLAT_OFF));
            assert!(c <= a.length / 2.0 + 1e-12, "{}: cut {c}", a.member);
        }
    }
    let gone = d
        .joints
        .iter()
        .flat_map(|j| &j.arrivals)
        .filter(|a| a.end == 0 && a.profile.is_some())
        .count()
        - s.len();
    eprintln!(
        "STATS onto built-up {onto} seated {seated} cut {cut} largest {largest:.4} misfit \
         {misfit} off {off} no-solid {gone}"
    );
}

/// The sum of a set of faces' area vectors (nothing for a closed solid)
/// and their volume by the divergence theorem.
fn area_sum_and_volume<'a>(
    faces: impl Iterator<Item = &'a crate::member_solids::SolidFace>,
) -> ([f64; 3], f64) {
    let mut sum = [0.0; 3];
    let mut volume = 0.0;
    for f in faces {
        for (t, _) in &f.triangles {
            let u = [0, 1, 2].map(|i| t[1][i] - t[0][i]);
            let v = [0, 1, 2].map(|i| t[2][i] - t[0][i]);
            let c = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let area = 0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            for (x, n) in sum.iter_mut().zip(f.normal) {
                *x += n * area;
            }
            let mid = [0, 1, 2].map(|i| (t[0][i] + t[1][i] + t[2][i]) / 3.0);
            volume += area * (0..3).map(|i| mid[i] * f.normal[i]).sum::<f64>() / 3.0;
        }
    }
    (sum, volume)
}

/// Two members running on through a joint as one part (2026-11-05): their
/// sections meet in the same corners there and neither has an end face
/// at it; together closed (one solid through the joint).
#[test]
fn a_continuous_pair_is_one_solid_through_the_joint() {
    let text = format!(
        "{TOWER}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\n\
         onto = \"L2\"\n"
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let s = member_solids(&d);
    let (l1, l2) = (solid(&s, "L1"), solid(&s, "L2"));
    assert_eq!(l1.runs_on, [false, true]);
    assert_eq!(l2.runs_on, [true, false]);
    assert_eq!(l1.ends[1], l2.ends[0]);
    // Its sides and one end face each.
    let p = arrival(&d, "N2", "L1").profile.as_ref().unwrap();
    assert_eq!(l1.faces.len(), p.outline.len() + 1);
    assert_eq!(l2.faces.len(), p.outline.len() + 1);
    let (sum, _) = area_sum_and_volume(l1.faces.iter().chain(&l2.faces));
    assert!(sum.iter().all(|v| v.abs() < 1e-9), "{sum:?}");
}

/// A plate's solid: its outline run through its thickness, closed, its
/// volume the outline's area times the thickness (10 mm).
#[test]
fn a_plate_is_a_solid_of_its_thickness() {
    let text = format!(
        "{TOWER}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\nleg = \"long\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\" }}\ngauges = [\"30 mm\"]\npitch = \"50 mm\"\n\
         end_distance = \"30 mm\"\n"
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let plates = crate::plate_solids(&d);
    assert_eq!(plates.len(), 1);
    let s = &plates[0];
    assert_eq!(s.id, "G1");
    assert!(!s.cover);
    let (sum, volume) = area_sum_and_volume(s.faces.iter());
    assert!(sum.iter().all(|v| v.abs() < 1e-9), "{sum:?}");
    // The outline's area (a fan about its first corner).
    let o = &d.joint("N2").unwrap().plates[0].outline;
    let mut area = [0.0; 3];
    for k in 1..o.len() - 1 {
        let u = [0, 1, 2].map(|i| o[k][i] - o[0][i]);
        let v = [0, 1, 2].map(|i| o[k + 1][i] - o[0][i]);
        area[0] += 0.5 * (u[1] * v[2] - u[2] * v[1]);
        area[1] += 0.5 * (u[2] * v[0] - u[0] * v[2]);
        area[2] += 0.5 * (u[0] * v[1] - u[1] * v[0]);
    }
    let area = (area[0] * area[0] + area[1] * area[1] + area[2] * area[2]).sqrt();
    assert!(
        (volume.abs() - area * 0.01).abs() < 1e-12,
        "{volume} {area}"
    );
}
