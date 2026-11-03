// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints in detail: each value with where it comes from, the bolts
//! where the layout puts them, a plate around its bolts.

use std::path::Path;

use crate::joints::{Completeness, Source, joint_details};
use crate::{BoltedLeg, ConnectionKind, SectionLibrary};

fn library() -> SectionLibrary {
    SectionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap()
}

/// A leg in two members in the face y = 1 m and a diagonal to its middle.
const TOWER: &str = r#"id = "T"
name = "t"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "1 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "1 m", "5 m"]

[[node]]
id = "N3"
point = ["0 m", "1 m", "10 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["4 m", "1 m", "0 m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "UA-83x52x6"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 1 } }
"#;

const AT_N2: &str = r#"
[[connection]]
member = "D1"
joint = "N2"
leg = "long"
bolts = { count = 2, diameter = "16 mm", grade = "8.8" }
gauges = ["30 mm"]
gauge_lines = 1
pitch = "50 mm"
end_distance = "30 mm"
"#;

#[test]
fn each_value_says_where_it_comes_from() {
    let d = joint_details(&format!("{TOWER}{AT_N2}"), &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let n2 = d.joint("N2").unwrap();
    assert_eq!(n2.arrivals.len(), 3);
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    assert_eq!(a.end, 1);
    assert_eq!(a.face, "y+");
    // Onto the leg: worked out, the larger section of the two legs (equal:
    // either), at 51.3° (atan 4/5 from the vertical).
    assert_eq!(a.onto.source, Source::WorkedOut);
    let (onto, angle) = a.angle.clone().unwrap();
    assert!(onto.starts_with('L'));
    assert!(
        (angle - (4f64 / 5.0).atan().to_degrees()).abs() < 1e-6,
        "{angle}"
    );
    assert_eq!(a.kind.value, Some(ConnectionKind::Bolted));
    assert_eq!(a.kind.source, Source::WorkedOut);
    assert_eq!(a.leg.value, Some(BoltedLeg::Long));
    assert_eq!(a.leg.source, Source::Given);
    assert_eq!(a.bolts.value.as_ref().unwrap().count, 2);
    assert_eq!(a.bolts.source, Source::Given);
    // One leg bolted: eccentric, worked out; two bolts into a stiffer
    // member (the leg, 805 mm² against the diagonal's 774): restrained,
    // worked out.
    assert_eq!(a.eccentric.value, Some(true));
    assert_eq!(a.eccentric.source, Source::WorkedOut);
    assert_eq!(a.restrained.value, Some(true));
    assert_eq!(a.restrained.source, Source::WorkedOut);
    assert_eq!(a.eccentricity.source, Source::WorkedOut);
    assert!(a.eccentricity.value.unwrap() > 0.0);
    // The two bolts 30 and 80 mm from the joint along the member, on the
    // long leg's outer face (the heel's plane).
    assert_eq!(a.bolt_points.len(), 2);
    for (p, along) in a.bolt_points.iter().zip([0.03, 0.08]) {
        let d = [0, 1, 2].map(|k| p[k] - n2.position[k]);
        let s: f64 = (0..3).map(|k| d[k] * a.direction[k]).sum();
        assert!((s - along).abs() < 1e-9, "{s}");
    }
    // The diagonal's other end: the member's one bolt, the leg assumed;
    // no connection there, so nothing worked out: not restrained, assumed
    // (as the check takes it, ohl_structures_fe::member_ends).
    let n4 = d.joint("N4").unwrap();
    let b = &n4.arrivals[0];
    assert_eq!(b.bolts.source, Source::Member);
    assert_eq!(b.leg.source, Source::Assumed);
    assert_eq!(b.restrained.value, Some(false));
    assert_eq!(b.restrained.source, Source::Assumed);
    assert_eq!(b.eccentric.source, Source::Assumed);
    assert_eq!(b.pitch.source, Source::Assumed);
    // The legs at N2 have nothing of their own: partly assumed.
    assert_eq!(n2.completeness, Completeness::PartlyAssumed);
    // N3 (a leg's end, no data at all): nothing.
    assert_eq!(d.joint("N3").unwrap().completeness, Completeness::Nothing);
}

/// Everything given at N4: complete.
#[test]
fn a_joint_given_in_full_is_complete() {
    let text = format!(
        "{TOWER}[[connection]]\nmember = \"D1\"\njoint = \"N4\"\nkind = \"pinned\"\n\
         eccentric = false\nrestrained = false\n"
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n4 = d.joint("N4").unwrap();
    assert_eq!(n4.arrivals[0].kind.value, Some(ConnectionKind::Pinned));
    assert_eq!(n4.completeness, Completeness::Complete);
}

/// A plate: its outline around the bolts onto it, in their plane.
#[test]
fn a_plate_lies_around_its_bolts() {
    let text = format!(
        "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n",
        AT_N2.replace("leg = \"long\"", "leg = \"long\"\nonto = \"G1\"")
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    assert_eq!(a.onto.source, Source::Given);
    // Onto a plate: the angle to the joint's leg all the same.
    assert!(a.angle.is_some());
    let p = &n2.plates[0];
    assert_eq!(p.members, vec!["D1".to_owned()]);
    assert!(!p.outline_given && p.outline.len() >= 3);
    // Every bolt inside the outline (its edges turning one way about the
    // bolts' axis around each bolt), and the corner's flange width covered.
    assert!(inside(&p.outline, a.bolt_axis, a.bolt_points[0]));
    for b in &a.bolt_points {
        assert!(inside(&p.outline, a.bolt_axis, *b), "{b:?} {:?}", p.outline);
    }
}

/// The distance of `q` from the nearest edge of `outline` (inside it),
/// seen along `n`, m.
fn from_edge(outline: &[[f64; 3]], n: [f64; 3], q: [f64; 3]) -> f64 {
    let k = outline.len();
    let ln = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    (0..k)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % k]);
            let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let d = [q[0] - a[0], q[1] - a[1], q[2] - a[2]];
            let c = [
                e[1] * d[2] - e[2] * d[1],
                e[2] * d[0] - e[0] * d[2],
                e[0] * d[1] - e[1] * d[0],
            ];
            let le = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt();
            ((c[0] * n[0] + c[1] * n[1] + c[2] * n[2]) / ln / le).abs()
        })
        .fold(f64::INFINITY, f64::min)
}

/// The plate drawn around its bolts keeps the design code's minimum edge
/// and end distances from each of them (the member end's, ASCE 10-97 by
/// default) and says where they come from; an outline given too small has
/// the bolts too near its edge said.
#[test]
fn a_plate_keeps_the_codes_distances_from_its_bolts() {
    let text = format!(
        "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n",
        AT_N2.replace("leg = \"long\"", "leg = \"long\"\nonto = \"G1\"")
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    let p = &n2.plates[0];
    let r = crate::bolt_rules(&text, &library(), &Default::default(), "D1", "N2", None)
        .expect("the code's rules");
    assert!(p.edges[0].contains(r.code), "{:?}", p.edges);
    assert!(p.notes.is_empty(), "{:?}", p.notes);
    assert!(matches!(p.leg.as_deref(), Some("L1" | "L2")));
    let least = r.edge.min(r.end);
    for b in &a.bolt_points {
        let e = from_edge(&p.outline, a.bolt_axis, *b);
        assert!(e >= least - 1e-6, "{e} < {least}");
    }
    // An outline given: a square of 20 mm round the first bolt — too near.
    let b = a.bolt_points[0];
    let (u, v) = (a.direction, {
        let n = a.bolt_axis;
        let d = a.direction;
        [
            n[1] * d[2] - n[2] * d[1],
            n[2] * d[0] - n[0] * d[2],
            n[0] * d[1] - n[1] * d[0],
        ]
    });
    let corner = |s: f64, t: f64| {
        let q: Vec<String> = (0..3)
            .map(|k| format!("\"{:.6} m\"", b[k] + u[k] * s + v[k] * t))
            .collect();
        format!("[{}]", q.join(", "))
    };
    let square = [
        corner(-0.02, -0.02),
        corner(0.02, -0.02),
        corner(0.02, 0.02),
        corner(-0.02, 0.02),
    ]
    .join(", ");
    let given = text.replace(
        "thickness = \"8 mm\"\n",
        &format!("thickness = \"8 mm\"\noutline = [{square}]\n"),
    );
    let d = joint_details(&given, &library(), &Default::default()).unwrap();
    let p = &d.joint("N2").unwrap().plates[0];
    assert!(p.outline_given);
    assert!(
        p.notes.iter().any(|n| n.contains("bolt 1 of D1")),
        "{:?}",
        p.notes
    );
    assert!(
        p.notes.iter().any(|n| n.contains("outside")),
        "{:?}",
        p.notes
    );
}

/// Two plates at one joint, the second welded (2026-11-03): each drawn
/// round its own members' bolts, its kind kept.
#[test]
fn a_joint_takes_two_plates_one_welded() {
    use ohl_structures::connections::PlateKind;
    let text = format!(
        "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\nkind = \"welded\"\n",
        AT_N2.replace("leg = \"long\"", "leg = \"long\"\nonto = \"G2\"")
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    assert_eq!(n2.plates.len(), 2);
    assert_eq!(n2.plates[0].kind, PlateKind::Bolted);
    assert_eq!(n2.plates[1].kind, PlateKind::Welded);
    assert_eq!(n2.plates[1].members, ["D1"]);
    assert!(n2.plates[0].members.is_empty());
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    for b in &a.bolt_points {
        assert!(inside(&n2.plates[1].outline, a.bolt_axis, *b));
    }
    // The face view says it is welded.
    let v = crate::face_view(n2, &crate::faces_of(n2)[0]);
    assert!(v.marks.iter().any(|m| matches!(
        m,
        crate::Mark::Text { text, .. } if text.contains("G2") && text.contains("welded")
    )));
}

/// Whether `q` lies inside the convex `outline` seen along `n`.
fn inside(outline: &[[f64; 3]], n: [f64; 3], q: [f64; 3]) -> bool {
    let k = outline.len();
    let side = |i: usize| {
        let (a, b) = (outline[i], outline[(i + 1) % k]);
        let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let d = [q[0] - a[0], q[1] - a[1], q[2] - a[2]];
        let c = [
            e[1] * d[2] - e[2] * d[1],
            e[2] * d[0] - e[0] * d[2],
            e[0] * d[1] - e[1] * d[0],
        ];
        c[0] * n[0] + c[1] * n[1] + c[2] * n[2]
    };
    let s: Vec<f64> = (0..k).map(side).collect();
    s.iter().all(|v| *v >= -1e-9) || s.iter().all(|v| *v <= 1e-9)
}

/// Below level 3 (no cross-sections): the arrivals, nothing more.
#[test]
fn below_level_3_the_arrivals_only() {
    let text = TOWER
        .lines()
        .filter(|l| !l.starts_with("section") && !l.starts_with("design"))
        .collect::<Vec<_>>()
        .join("\n");
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    assert!(!d.level3);
    let n2 = d.joint("N2").unwrap();
    assert_eq!(n2.arrivals.len(), 3);
    assert!(
        n2.arrivals
            .iter()
            .all(|a| a.profile.is_none() && a.bolt_points.is_empty())
    );
    assert_eq!(n2.completeness, Completeness::Nothing);
}

/// The face view of N2: seen from the face its members lie in, the two
/// bolts, the end distance and pitch dimensioned (30, 50), the labels;
/// as SVG and PDF.
#[test]
fn the_face_view_shows_the_bolts_and_their_distances() {
    use crate::{Mark, face_pdf, face_svg, face_view, faces_of};
    let d = joint_details(&format!("{TOWER}{AT_N2}"), &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    assert_eq!(faces_of(n2)[0], "y+");
    let v = face_view(n2, "y+");
    let bolts = v
        .marks
        .iter()
        .filter(|m| matches!(m, Mark::Bolt { .. }))
        .count();
    assert_eq!(bolts, 2);
    let texts: Vec<&str> = v
        .marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"30") && texts.contains(&"50"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("D1 UA-83x52x6") && t.contains("2 × M16 8.8")),
        "{texts:?}"
    );
    assert!(texts.contains(&"g 30"), "{texts:?}");
    assert!(v.bounds.iter().all(|b| b.is_finite()) && v.bounds[2] > v.bounds[0]);
    assert!(v.title.contains("Joint N2") && v.title.contains("face y+"));
    let svg = face_svg(&v);
    assert!(svg.starts_with("<svg") || svg.contains("<svg"));
    assert!(svg.contains("Joint N2"));
    assert!(face_pdf(&v).starts_with(b"%PDF"));
    // The sheet, when asked for (OHL_JOINT_SCREENSHOTS=folder).
    if let Some(dir) = ohl_core::env::var_os("OHL_JOINT_SCREENSHOTS") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::write(dir.join("face.svg"), svg).unwrap();
        std::fs::write(dir.join("face.pdf"), face_pdf(&v)).unwrap();
    }
}

/// The example tower (its own `[[section]]` tables, no library): read in
/// detail without a fault.
#[test]
fn the_example_tower_is_read_in_detail() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures/type-02.ohls");
    let text = std::fs::read_to_string(path).unwrap();
    let d = joint_details(&text, &SectionLibrary::default(), &Default::default()).unwrap();
    assert!(!d.joints.is_empty());
    assert!(d.joints.iter().any(|j| j.arrivals.len() >= 3));
}

/// The joints like N2: its mirror in the face y = −1 m (the same
/// members, cross-sections, classes and angles), the diagonal matched to
/// the diagonal; the joints with other members not.
#[test]
fn a_mirror_joint_is_like_it() {
    let mirror = r#"
[[node]]
id = "M1"
point = ["0 m", "-1 m", "0 m"]
support = "fixed"

[[node]]
id = "M2"
point = ["0 m", "-1 m", "5 m"]

[[node]]
id = "M3"
point = ["0 m", "-1 m", "10 m"]
support = "fixed"

[[node]]
id = "M4"
point = ["-4 m", "-1 m", "0 m"]
support = "fixed"

[[member]]
id = "L3"
nodes = ["M1", "M2"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "L4"
nodes = ["M2", "M3"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "D2"
nodes = ["M4", "M2"]
section = "UA-83x52x6"
design = { class = "bracing" }
"#;
    let d = joint_details(&format!("{TOWER}{mirror}"), &library(), &Default::default()).unwrap();
    let s = crate::similar_joints(&d, "N2");
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0].joint, "M2");
    assert!(s[0].pairs.contains(&("D1".to_owned(), "D2".to_owned())));
    assert_eq!(s[0].pairs.len(), 3);
    assert!(
        crate::similar_joints(&d, "N1")
            .iter()
            .all(|x| x.joint != "N2")
    );
}

/// A bolt of a library: its diameter, hole, grade and drawn sizes from
/// the library, what the connection types overriding them; an id no
/// library has: said, the member's bolts.
#[test]
fn a_library_bolt_is_taken_from_the_library() {
    let bolts = ohl_structures::bolts::BoltLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../libraries/bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let text = format!(
        "{TOWER}{}",
        AT_N2.replace(
            "bolts = { count = 2, diameter = \"16 mm\", grade = \"8.8\" }",
            "bolts = { count = 2, bolt = \"X17-example\", grade = \"own\" }"
        )
    );
    let d = joint_details(&text, &library(), &bolts).unwrap();
    let a = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    let b = a.bolts.value.as_ref().unwrap();
    assert!((b.diameter - 0.017).abs() < 1e-12);
    assert!((b.hole.unwrap() - 0.0185).abs() < 1e-12);
    assert_eq!(b.grade.as_deref(), Some("own"));
    assert_eq!(b.bolt.as_deref(), Some("X17-example"));
    assert!((b.drawn[0].unwrap() - 0.025).abs() < 1e-12);
    // The head drawn 25 mm across the flats: its corners 25/√3 from the axis.
    let faces = crate::bolt_faces(a.bolt_points[0], a.bolt_axis, b.diameter, a.grip, b.drawn);
    let head = faces.iter().find(|f| f.1 == crate::BoltPart::Head).unwrap();
    let c = head.0[0];
    let p = a.bolt_points[0];
    let along: f64 = (0..3).map(|k| (c[k] - p[k]) * a.bolt_axis[k]).sum();
    let r2: f64 = (0..3)
        .map(|k| (c[k] - p[k] - along * a.bolt_axis[k]).powi(2))
        .sum();
    assert!((r2.sqrt() - 0.025 / 3f64.sqrt()).abs() < 1e-9);
    // Not in the library: said, the member's bolts taken (as the check
    // takes them).
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let a = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    assert_eq!(a.bolts.source, Source::Member);
    assert_eq!(a.bolts.value.as_ref().unwrap().count, 1);
    assert!(a.notes.iter().any(|n| n.contains("in no bolt library")));
}

/// A plate without an outline lies in the face of the bolts onto it, in
/// the Joint view as in the export (owner 2026-11-02: "make the joint view
/// follow the export for plates").
#[test]
fn a_plate_lies_in_its_bolts_face_as_exported() {
    let text = format!(
        "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n",
        AT_N2.replace("leg = \"long\"", "leg = \"long\"\nonto = \"G1\"")
    );
    let bolts = ohl_structures::bolts::BoltLibrary::default();
    let d = joint_details(&text, &library(), &bolts).unwrap();
    let n2 = d.joint("N2").unwrap();
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    let view = &n2.plates[0].outline;
    let b0 = a.bolt_points[0];
    for q in view {
        let along: f64 = (0..3).map(|k| (q[k] - b0[k]) * a.bolt_axis[k]).sum();
        assert!(along.abs() < 1e-9, "{along}");
    }
    let mut s = crate::export_structure(&text, &library(), None, &[]).unwrap();
    crate::export_joints(&mut s, &text, &library(), &bolts).unwrap();
    let exported = &s.joint_plates[0].outline;
    assert_eq!(exported.len(), view.len());
    for (p, q) in exported.iter().zip(view) {
        assert!((0..3).all(|k| (p[k] - q[k]).abs() < 1e-9), "{p:?} {q:?}");
    }
}

/// The member view (owner 2026-11-02: "see live the impacts on the bar's
/// resistance"): the member's two ends as the joints see them, its leg
/// laid flat with the holes of both ends, its tension capacity rising
/// when an end's bolts go from one to three (the bolts' shear governing).
#[test]
fn a_member_shows_both_ends_and_its_capacity_follows_the_bolts() {
    let bolts = ohl_structures::bolts::BoltLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../libraries/bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let ends = |n2: u32, n4: u32| {
        format!(
            "{TOWER}[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nleg = \"long\"\n\
             bolts = {{ count = {n2}, bolt = \"X17-example\" }}\ngauges = [\"30 mm\"]\n\
             gauge_lines = 1\npitch = \"55 mm\"\nend_distance = \"30 mm\"\n\
             [[connection]]\nmember = \"D1\"\njoint = \"N4\"\nleg = \"long\"\n\
             bolts = {{ count = {n4}, bolt = \"X17-example\" }}\ngauges = [\"30 mm\"]\n\
             gauge_lines = 1\npitch = \"55 mm\"\nend_distance = \"30 mm\"\n"
        )
    };
    let text = ends(3, 1);
    let d = joint_details(&text, &library(), &bolts).unwrap();
    let m = crate::member_ends(&d, "D1").unwrap();
    assert_eq!(m.ends[0].as_ref().unwrap().0, "N4");
    assert_eq!(m.ends[1].as_ref().unwrap().0, "N2");
    let flat = crate::flat_leg(&m).unwrap();
    assert_eq!(flat.holes.len(), 4);
    // The first end's hole 30 mm from N4, the second's 30, 85, 140 mm
    // back from N2.
    let mut along: Vec<f64> = flat.holes.iter().map(|h| h.along).collect();
    along.sort_by(f64::total_cmp);
    let l = m.length;
    for (a, b) in along.iter().zip([0.03, l - 0.14, l - 0.085, l - 0.03]) {
        assert!((a - b).abs() < 1e-9, "{a} {b}");
    }
    assert!((flat.width - 0.083).abs() < 1e-9);
    let one = crate::member_capacity(&text, &library(), &bolts, "D1").unwrap();
    let three = crate::member_capacity(&ends(3, 3), &library(), &bolts, "D1").unwrap();
    let (t1, g1) = one.tension.clone().unwrap();
    let (t3, _) = three.tension.clone().unwrap();
    assert!(t3 > t1, "{t1} {t3}");
    assert!(g1.contains("bolt") || g1.contains("N4"), "{g1}");
    assert!(!one.joints.is_empty());
    assert!(crate::member_capacity(&text, &library(), &bolts, "nope").is_err());
}

/// Where a member's steel stops (owner 2026-11-02: "the forces should all
/// align at one point in the joint even if the member has to stop before
/// to fit it in place"): worked out so the diagonal clears the leg's heel,
/// its bolts measured from there; given, it is the given; a shift moves
/// the member and its bolts sideways (towards what it bolts onto).
#[test]
fn the_steel_stops_where_it_fits_and_a_shift_moves_the_end() {
    let bolts = ohl_structures::bolts::BoltLibrary::default();
    let d = joint_details(&format!("{TOWER}{AT_N2}"), &library(), &bolts).unwrap();
    let a = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap()
        .clone();
    let cut = a.cut.value.unwrap();
    assert!(cut >= 0.0);
    assert!(matches!(a.cut.source, Source::WorkedOut | Source::Assumed));
    // The first bolt: the cut plus the end distance (30 mm) from the joint.
    assert!(
        (a.bolt_flat[0][0] - (cut + 0.03)).abs() < 1e-9,
        "{:?}",
        a.bolt_flat
    );
    // Given: 50 mm, and a shift of 20 mm.
    let text = format!(
        "{TOWER}{}",
        AT_N2.replace(
            "leg = \"long\"",
            "leg = \"long\"\ncut = \"50 mm\"\nshift = \"20 mm\""
        )
    );
    let d = joint_details(&text, &library(), &bolts).unwrap();
    let b = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    assert_eq!(b.cut.value, Some(0.05));
    assert_eq!(b.cut.source, Source::Given);
    assert!((b.bolt_flat[0][0] - 0.08).abs() < 1e-9);
    let (s, u) = b.shift;
    assert!((s - 0.02).abs() < 1e-12);
    let u = u.unwrap();
    // Across the member (square to it), towards the leg it bolts onto.
    let along: f64 = (0..3).map(|k| u[k] * b.direction[k]).sum();
    assert!(along.abs() < 1e-9);
    let onto = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|x| Some(&x.member) == b.onto.value.as_ref())
        .unwrap();
    let towards: f64 = (0..3).map(|k| u[k] * onto.direction[k]).sum();
    assert!(towards > 0.0);
    // The member and its bolts moved by the shift (beside the seat along
    // the bolts, which is square to it).
    let moved: f64 = (0..3).map(|k| (b.offset[k] - a.offset[k]) * u[k]).sum();
    assert!((moved - 0.02).abs() < 1e-9, "{moved}");
}

/// A member bolted onto a gusset plate (owner 2026-11-02: "wouldn't it be
/// better to also have it on the plate?"): its leg bolted lies in the
/// plate's plane (the plane of the members onto it and the joint's leg),
/// so its bolts go square through the plate, and the plate lies in their
/// face.
#[test]
fn a_member_onto_a_plate_lies_flat_on_it() {
    let text = format!(
        "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n",
        AT_N2.replace("leg = \"long\"\n", "onto = \"G1\"\n")
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    let leg = n2.arrivals.iter().find(|a| a.member == "L1").unwrap();
    // The plane of the diagonal and the leg: y (the face y+).
    let n = [
        a.direction[1] * leg.direction[2] - a.direction[2] * leg.direction[1],
        a.direction[2] * leg.direction[0] - a.direction[0] * leg.direction[2],
        a.direction[0] * leg.direction[1] - a.direction[1] * leg.direction[0],
    ];
    let ln = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let cos: f64 = (0..3).map(|k| a.bolt_axis[k] * n[k] / ln).sum();
    assert!(
        cos.abs() > 0.999,
        "the bolts square through the plate: {cos}"
    );
    // The plate in the bolts' face.
    let b0 = a.bolt_points[0];
    for q in &n2.plates[0].outline {
        let off: f64 = (0..3).map(|k| (q[k] - b0[k]) * a.bolt_axis[k]).sum();
        assert!(off.abs() < 1e-9);
    }
}
