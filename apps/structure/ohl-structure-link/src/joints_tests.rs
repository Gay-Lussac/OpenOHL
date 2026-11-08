// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joints in detail: each value with where it comes from, the bolts
//! where the layout puts them, a plate around its bolts.

use std::path::Path;

use crate::joints::{Completeness, Source, joint_details};
use crate::{BoltedLeg, ConnectionKind, SectionLibrary};

pub(crate) fn library() -> SectionLibrary {
    SectionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap()
}

/// A leg in two members in the face y = 1 m and a diagonal to its middle.
pub(crate) const TOWER: &str = r#"id = "T"
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

/// Two plates at one joint, welded together (owner 2026-11-04: a welded
/// plate is several plates welded together): each drawn round its own
/// members' bolts, both of one welded plate; the face view says so.
#[test]
fn a_joint_takes_two_plates_welded_together() {
    let text = format!(
        "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n\
         [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\nwelded_to = [\"G1\"]\n",
        AT_N2.replace("leg = \"long\"", "leg = \"long\"\nonto = \"G2\"")
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    assert_eq!(n2.plates.len(), 2);
    assert_eq!(n2.plates[0].assembly, ["G1", "G2"]);
    assert_eq!(n2.plates[1].welded_to, ["G1"]);
    assert_eq!(n2.plates[0].welded_to, ["G2"]);
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
        crate::Mark::Text { text, .. } if text.contains("G2") && text.contains("welded to G1")
    )));
}

/// A second diagonal in the face x = 0 to the joint N2 (its own bolts).
pub(crate) const OTHER_FACE: &str = r#"
[[node]]
id = "N5"
point = ["0 m", "5 m", "0 m"]
support = "fixed"

[[member]]
id = "D2"
nodes = ["N5", "N2"]
section = "UA-83x52x6"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }
"#;

/// "Make a welded plate for every member" (owner 2026-11-04: "the
/// 'welded plate' should just mean that multiple plates are welded to be
/// able to reach every member bolts that come on the joint"): a plate per
/// face the members arrive in (D1 in y = 1, D2 in x = 0), welded
/// together; written as the edit writes it, every member's bolts lie on
/// its plate with the design code's end and edge distances in the plate
/// kept, and the two plates meet along a weld.
#[test]
fn a_welded_plate_reaches_every_members_bolts() {
    let text = format!("{TOWER}{OTHER_FACE}");
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let (plan, left) = crate::welded_plan(d.joint("N2").unwrap());
    assert!(left.is_empty(), "{left:?}");
    let groups: Vec<Vec<String>> = plan.iter().map(|p| p.members.clone()).collect();
    assert_eq!(groups, [vec!["D1".to_owned()], vec!["D2".to_owned()]]);
    // As ohl_structure_edit::connection_edits::make_welded_plates writes it.
    let mut welded = text.clone();
    for (k, g) in groups.iter().enumerate() {
        welded.push_str(&format!(
            "\n[[plate]]\nid = \"G{}\"\njoint = \"N2\"\nthickness = \"10 mm\"\n{}",
            k + 1,
            if k > 0 { "welded_to = [\"G1\"]\n" } else { "" }
        ));
        for m in g {
            welded.push_str(&format!(
                "\n[[connection]]\nmember = \"{m}\"\njoint = \"N2\"\nonto = \"G{}\"\n",
                k + 1
            ));
        }
    }
    let d = joint_details(&welded, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    assert_eq!(n2.plates.len(), 2);
    for (k, g) in groups.iter().enumerate() {
        let p = &n2.plates[k];
        assert_eq!(p.assembly, ["G1", "G2"]);
        assert!(p.notes.is_empty(), "{:?}", p.notes);
        for m in g {
            let a = n2.arrivals.iter().find(|a| &a.member == m).unwrap();
            assert_eq!(a.onto.value.as_deref(), Some(p.id.as_str()));
            assert!(!a.bolt_points.is_empty(), "{m}: no bolts");
            let r = crate::bolt_rules(&welded, &library(), &Default::default(), m, "N2", None)
                .expect("the code's rules");
            let least = r.edge.min(r.end);
            for b in &a.bolt_points {
                assert!(
                    inside(&p.outline, a.bolt_axis, *b),
                    "{m}: a bolt off {}",
                    p.id
                );
                let e = from_edge(&p.outline, a.bolt_axis, *b);
                assert!(e >= least - 1e-6, "{m} on {}: {e} < {least}", p.id);
            }
        }
    }
    // Where they meet: a weld seam, along the leg.
    let w = &n2.plates[0].welds;
    assert_eq!(w.len(), 1, "{:?}", n2.plates[0].notes);
    assert_eq!(w[0].with, "G2");
    let leg = n2.arrivals.iter().find(|a| a.member == "L1").unwrap();
    let s = [
        w[0].ends[1][0] - w[0].ends[0][0],
        w[0].ends[1][1] - w[0].ends[0][1],
        w[0].ends[1][2] - w[0].ends[0][2],
    ];
    let l = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
    let cos: f64 = (0..3).map(|k| s[k] / l * leg.direction[k]).sum();
    assert!(l > 0.05 && cos.abs() > 0.99, "seam {l} m, {cos}");
}

/// The gauge lines from the holes in one section of the member's design
/// data (owner 2026-11-04: a tower program's export gives the holes, not
/// the lines): one leg bolted, a line per hole; both legs, half of them
/// per leg; at most three; none: one, assumed.
#[test]
fn the_gauge_lines_come_from_the_holes() {
    let lines = |bolts: &str, leg: &str| {
        let text = TOWER.replace(
            "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 1 } }",
            &format!("design = {{ class = \"bracing\", {leg}bolts = {{ {bolts} }} }}"),
        );
        let d = joint_details(&text, &library(), &Default::default()).unwrap();
        let a = d
            .joint("N2")
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == "D1")
            .unwrap()
            .clone();
        (a.gauge_lines.value, a.gauge_lines.source, a.notes)
    };
    let (n, s, notes) = lines("diameter = \"16 mm\", count = 4, holes = 2", "");
    assert_eq!((n, s), (Some(2), Source::WorkedOut));
    assert!(
        notes.iter().any(|x| x.contains("from the 2 hole(s)")),
        "{notes:?}"
    );
    let (n, s, _) = lines(
        "diameter = \"16 mm\", count = 4, holes = 4",
        "connected_leg = \"both\", ",
    );
    assert_eq!((n, s), (Some(2), Source::WorkedOut));
    let (n, _, _) = lines("diameter = \"16 mm\", count = 6, holes = 5", "");
    assert_eq!(n, Some(3));
    let (n, s, _) = lines("diameter = \"16 mm\", count = 4", "");
    assert_eq!((n, s), (Some(1), Source::Assumed));
}

/// A member a quarter turn off what it bolts onto (owner 2026-11-04:
/// "some of the representation automatically built from the import seem
/// wrong in that regard"): Joint details says so, and its roll turned by
/// the turn found lays it flat. The diagonal D1 bolted by its long leg
/// lies with that leg across the face (the section's default axes): a
/// quarter turn off the leg's flange in the face.
#[test]
fn a_member_a_quarter_turn_off_is_said_and_laid_flat() {
    let at = |roll: f64| {
        let text = format!("{TOWER}roll = \"{roll} deg\"\n{AT_N2}");
        let d = joint_details(&text, &library(), &Default::default()).unwrap();
        d.joint("N2")
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == "D1")
            .unwrap()
            .clone()
    };
    let off = at(0.0);
    let f = off.flat.expect("onto the leg's flange");
    assert!((f.off - 90.0).abs() < 1.0, "{f:?}");
    assert!((f.turn.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    assert!(
        off.notes.iter().any(|n| n.contains("bolted face 90° off")),
        "{:?}",
        off.notes
    );
    // Turned: flat, nothing said; its bolts square through the flange.
    let flat = at(f.turn.to_degrees());
    let g = flat.flat.unwrap();
    assert!(g.off < 0.5, "{g:?}");
    assert!(!flat.notes.iter().any(|n| n.contains("turn it")));
    assert!(flat.bolt_axis[1].abs() > 0.999, "{:?}", flat.bolt_axis);
    // Half a turn more: flat too (the outstanding leg the other way).
    let other = at(f.turn.to_degrees() + 180.0).flat.unwrap();
    assert!(other.off < 0.5, "{other:?}");
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

/// A plate in a bolt's grip is drawn (owner 2026-11-04: "the bolts get
/// longer but without a plate"): the member's bolts clamp its thickness,
/// its outline lies in their face, and it runs through its thickness away
/// from the member — exactly where the member was moved off the joint's
/// leg to make room for it.
#[test]
fn a_plate_in_a_grip_has_an_outline_and_its_thickness() {
    let with = |onto: &str| {
        let text = format!(
            "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n",
            AT_N2.replace("leg = \"long\"\n", onto)
        );
        joint_details(&text, &library(), &Default::default()).unwrap()
    };
    let on = with("onto = \"G1\"\n");
    let off = with("");
    let n2 = on.joint("N2").unwrap();
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    let p = &n2.plates[0];
    // The grip: its own leg and the plate.
    assert!((a.grip[1] - 0.010).abs() < 1e-12, "{:?}", a.grip);
    assert!(p.outline.len() >= 3 && p.members == ["D1"]);
    // Through the thickness: against the bolts' axis (the heads on the
    // member's side).
    let n = p.through.expect("an outline: a side to extrude to");
    let cos: f64 = (0..3).map(|k| n[k] * a.bolt_axis[k]).sum();
    assert!((cos + 1.0).abs() < 1e-9, "{cos}");
    // The member moved out by the plate's thickness from where it lies
    // without the plate: the plate fills that gap.
    let b = off
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    let moved: f64 = (0..3)
        .map(|k| (a.offset[k] - b.offset[k]) * a.bolt_axis[k])
        .sum();
    assert!((moved - 0.010).abs() < 1e-9, "{moved}");
    // Nothing bolted onto it: no outline (no disc at the work point any
    // more), said; no bolt's grip takes it.
    let lone = &off.joint("N2").unwrap().plates[0];
    assert!(lone.outline.is_empty() && lone.through.is_none());
    assert!(
        lone.notes.iter().any(|x| x.contains(crate::NO_OUTLINE)),
        "{:?}",
        lone.notes
    );
    assert!((b.grip[1] - 0.010).abs() > 1e-6, "{:?}", b.grip);
}

/// The example library and two smaller equal angles (invented): 45 × 5
/// (fits in 61 × 7: at most 61 − 7 − 8 = 46 mm) and 50 × 5 (does not).
pub(crate) fn nest_library() -> SectionLibrary {
    let mut lib = library();
    let extra = SectionLibrary::parse(
        r#"format = "ohl-sections"
version = 1
name = "nest"
default_material = "steel-example"

[[section]]
id = "EA-45x5"
name = "Equal angle 45 × 5 (invented)"
shape = "equal_angle"
h = "45 mm"
t = "5 mm"
r = "6 mm"

[[section]]
id = "EA-50x5"
name = "Equal angle 50 × 5 (invented)"
shape = "equal_angle"
h = "50 mm"
t = "5 mm"
r = "6 mm"
"#,
    )
    .unwrap();
    lib.merge(extra);
    lib
}

/// L2 (45 × 5) nested in L1 (61 × 7) at N2, two 12 mm bolts on its long
/// leg 25 mm from the heel, 25 and 65 mm from the joint.
const NESTED: &str = r#"
[[connection]]
member = "L2"
joint = "N2"
onto = "L1"
face = "nested"
leg = "long"
bolts = { count = 2, diameter = "12 mm", grade = "8.8" }
gauges = ["25 mm"]
gauge_lines = 1
pitch = "40 mm"
end_distance = "25 mm"
"#;

/// [`TOWER`] with L2 of section `section` (and `more` after it).
fn with_l2(section: &str, more: &str) -> String {
    format!("{TOWER}{NESTED}").replace(
        "id = \"L2\"\nnodes = [\"N2\", \"N3\"]\nsection = \"EA-61x7\"",
        &format!("id = \"L2\"\nnodes = [\"N2\", \"N3\"]\nsection = \"{section}\"{more}"),
    )
}

fn at<'a>(d: &'a crate::JointDetails, joint: &str, member: &str) -> &'a crate::Arrival {
    d.joint(joint)
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == member)
        .unwrap()
}

/// The heel of `a` where it is drawn, structure's axes.
fn heel_of(d: &crate::JointDetails, joint: &str, a: &crate::Arrival) -> [f64; 3] {
    let p = a.profile.as_ref().unwrap();
    let g = p.angle.unwrap();
    let j = d.joint(joint).unwrap().position;
    [0, 1, 2].map(|k| j[k] + a.offset[k] + p.axes[1][k] * g.heel[0] + p.axes[2][k] * g.heel[1])
}

/// A lap joint (owner 2026-11-05): the smaller angle heel into heel in the
/// larger, its heel 7 mm (the larger's thickness) along both the larger's
/// legs from the larger's heel; the bolts clamp 5 + 7 mm; the larger runs
/// on past the work point over the bolts; it fits (45 ≤ 46 mm).
#[test]
fn an_angle_nests_in_a_larger_one() {
    let d = joint_details(
        &with_l2("EA-45x5", ""),
        &nest_library(),
        &Default::default(),
    )
    .unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let (inner, outer) = (at(&d, "N2", "L2"), at(&d, "N2", "L1"));
    assert_eq!(inner.sits.value, Some(crate::Face::Nested));
    assert_eq!(inner.sits.source, Source::Given);
    assert!(!inner.misfit, "{:?}", inner.notes);
    assert!(!outer.misfit, "{:?}", outer.notes);
    // The larger's heel, then 7 mm along each of its legs' normals.
    let q = outer.profile.as_ref().unwrap();
    let (hi, ho) = (heel_of(&d, "N2", inner), heel_of(&d, "N2", outer));
    for k in 0..3 {
        let want = ho[k] + 0.007 * (q.axes[1][k] + q.axes[2][k]);
        assert!((hi[k] - want).abs() < 1e-9, "{hi:?} {ho:?}");
    }
    // The larger not moved (it is the one nested in), its bolts here those
    // of the smaller.
    assert!(outer.offset.iter().all(|v| v.abs() < 1e-12));
    assert!(outer.bolt_points.is_empty());
    // The bolts through both legs: 5 mm and 7 mm.
    assert_eq!(inner.grip, [0.005, 0.007]);
    for (b, want) in inner.bolt_flat.iter().zip([[0.025, 0.025], [0.065, 0.025]]) {
        assert!((b[0] - want[0]).abs() < 1e-12 && (b[1] - want[1]).abs() < 1e-12);
    }
    // The larger runs on past the work point beyond the last bolt (65 mm)
    // by the end distance it keeps: worked out, below 0.
    assert_eq!(outer.cut.source, Source::WorkedOut);
    let cut = outer.cut.value.unwrap();
    assert!(cut < -0.065 - 0.012 && cut > -0.2, "{cut}");
    // The ground heel said, with the gap had it not been: 8 (1 − 1/√2) =
    // 2.3 mm.
    assert!(
        inner
            .notes
            .iter()
            .any(|n| n.contains("ground") && n.contains("2.3 mm")),
        "{:?}",
        inner.notes
    );
    // Its solid runs below the work point.
    let s = crate::member_solids(&d);
    let l1 = s.iter().find(|m| m.member == "L1").unwrap();
    assert!((l1.cut[1] - cut).abs() < 1e-12);
}

/// A nested angle too large (50 > 61 − 7 − 8 = 46 mm) or turned otherwise
/// is a misfit, said.
#[test]
fn a_nested_angle_that_does_not_fit_is_said() {
    let d = joint_details(
        &with_l2("EA-50x5", ""),
        &nest_library(),
        &Default::default(),
    )
    .unwrap();
    let a = at(&d, "N2", "L2");
    assert!(a.misfit);
    assert!(
        a.notes
            .iter()
            .any(|n| n.contains("50 mm leg does not fit") && n.contains("= 46 mm")),
        "{:?}",
        a.notes
    );
    assert!(at(&d, "N2", "L1").misfit);
    // Turned a quarter: its legs not along the other's.
    let d = joint_details(
        &with_l2("EA-45x5", "\nroll = \"90 deg\""),
        &nest_library(),
        &Default::default(),
    )
    .unwrap();
    let a = at(&d, "N2", "L2");
    assert!(a.misfit);
    assert!(
        a.notes.iter().any(|n| n.contains("do not lie along")),
        "{:?}",
        a.notes
    );
}

/// The face of the leg a member bolts onto (2026-11-05): worked out from
/// how it is turned; the other face given puts it on that face, its leg
/// through the other's thickness and its own (6 + 7 mm along its bolts'
/// axis), its inner face on it — its outstanding leg through that leg: a
/// misfit, the bolts on the face they meet, from the other side.
#[test]
fn the_face_given_moves_the_member_to_it() {
    let bolts = ohl_structures::bolts::BoltLibrary::default();
    // D1 turned to lie flat on the leg's flange (see
    // a_member_a_quarter_turn_off_is_said_and_laid_flat).
    let d = joint_details(&format!("{TOWER}{AT_N2}"), &library(), &bolts).unwrap();
    let turn = at(&d, "N2", "D1").flat.unwrap().turn.to_degrees();
    let tower = format!("{TOWER}roll = \"{turn} deg\"\n");
    let d = joint_details(&format!("{tower}{AT_N2}"), &library(), &bolts).unwrap();
    let a = at(&d, "N2", "D1").clone();
    assert_eq!(a.sits.source, Source::WorkedOut, "{a:?}");
    assert!(!a.misfit);
    let worked = a.sits.value.unwrap();
    // The same face given: the same place.
    let text = format!("{tower}{AT_N2}face = \"{}\"\n", worked.as_str());
    let d = joint_details(&text, &library(), &bolts).unwrap();
    let b = at(&d, "N2", "D1");
    assert_eq!(b.sits.source, Source::Given);
    assert_eq!(b.offset, a.offset);
    assert!(!b.misfit);
    // The other face.
    let other = if worked == crate::Face::Inside {
        crate::Face::Outside
    } else {
        crate::Face::Inside
    };
    let text = format!("{tower}{AT_N2}face = \"{}\"\n", other.as_str());
    let d = joint_details(&text, &library(), &bolts).unwrap();
    let b = at(&d, "N2", "D1");
    assert_eq!(b.sits.value, Some(other));
    assert!(b.misfit, "{:?}", b.notes);
    let nb = a.bolt_axis;
    for (k, n) in nb.iter().enumerate() {
        assert!(
            (b.offset[k] - a.offset[k] + 0.013 * n).abs() < 1e-9,
            "{:?} {:?}",
            b.offset,
            a.offset
        );
        assert!((b.bolt_axis[k] + n).abs() < 1e-12);
    }
    // Its bolts on the face it meets the other by: its inner face, 6 mm
    // from its outer face: 13 − 6 = 7 mm from where they were.
    assert_eq!(b.bolt_points.len(), 2);
    for (p, q) in b.bolt_points.iter().zip(&a.bolt_points) {
        let s: f64 = (0..3).map(|k| (p[k] - q[k]) * nb[k]).sum();
        assert!((s + 0.007).abs() < 1e-9, "{s}");
    }
}

/// L1 and L2 spliced at N2 by 8 mm covers (outer and inner on the long
/// leg, outer on the short), each bolting onto it by two 16 mm bolts on
/// its long leg.
pub(crate) const SPLICED: &str = r#"
[[splice]]
id = "S1"
joint = "N2"
members = ["L1", "L2"]
[[splice.cover]]
face = "outer"
leg = "long"
thickness = "8 mm"
[[splice.cover]]
face = "inner"
leg = "long"
thickness = "8 mm"
[[splice.cover]]
face = "outer"
leg = "short"
thickness = "8 mm"

[[connection]]
member = "L1"
joint = "N2"
onto = "S1"
leg = "long"
bolts = { count = 2, diameter = "16 mm", grade = "8.8" }
gauges = ["30 mm"]
gauge_lines = 1
pitch = "50 mm"
end_distance = "30 mm"

[[connection]]
member = "L2"
joint = "N2"
onto = "S1"
leg = "long"
bolts = { count = 2, diameter = "16 mm", grade = "8.8" }
gauges = ["30 mm"]
gauge_lines = 1
pitch = "50 mm"
end_distance = "30 mm"
"#;

/// A double cover splice (owner 2026-11-05): the bolts clamp the leg and
/// the inner cover (7 + 8 mm) and the outer cover (8 mm); each cover lies
/// on its face of the leg (outer: the heel's plane; inner: 7 mm in, clear
/// of the root: from 7 + 8 = 15 mm across), round both members' bolts,
/// the same length each way from the joint.
#[test]
fn covers_lie_on_the_legs_round_the_bolts() {
    let d = joint_details(
        &format!("{TOWER}{SPLICED}"),
        &library(),
        &Default::default(),
    )
    .unwrap();
    let j = d.joint("N2").unwrap();
    assert_eq!(j.splices.len(), 1);
    let s = &j.splices[0];
    assert_eq!(s.bolted, vec!["L1".to_owned(), "L2".to_owned()]);
    assert_eq!(s.covers.len(), 3);
    for m in ["L1", "L2"] {
        let a = at(&d, "N2", m);
        assert_eq!(a.grip, [0.015, 0.008], "{m}");
        assert_eq!(a.bolt_points.len(), 2);
        assert!(a.offset.iter().all(|v| v.abs() < 1e-12));
    }
    let a = at(&d, "N2", "L1");
    let p = a.profile.as_ref().unwrap();
    let heel = heel_of(&d, "N2", a);
    // The long leg (equal legs: along y), its normal z.
    let (across, n) = (p.axes[1], p.axes[2]);
    let dotp = |u: [f64; 3], v: [f64; 3]| (0..3).map(|k| u[k] * v[k]).sum::<f64>();
    let rel = |q: [f64; 3]| [0, 1, 2].map(|k| q[k] - heel[k]);
    for (c, depth, from) in [(&s.covers[0], 0.0, 0.0), (&s.covers[1], 0.007, 0.015)] {
        assert!(c.outline.len() >= 4, "{:?}", c.notes);
        for q in &c.outline {
            assert!((dotp(rel(*q), n) - depth).abs() < 1e-9, "{}", c.words());
            let x = dotp(rel(*q), across);
            assert!(x >= from - 1e-9 && x <= 0.061 + 1e-9, "{}: {x}", c.words());
        }
        let through = c.through.unwrap();
        let sign = if c.face == crate::CoverFace::Outer {
            -1.0
        } else {
            1.0
        };
        assert!((dotp(through, n) - sign).abs() < 1e-9);
        // As long below the joint as above it, beyond the last bolts (80
        // mm) by the end distance kept.
        let along: Vec<f64> = c.outline.iter().map(|q| q[2] - 5.0).collect();
        let (lo, hi) = (
            along.iter().copied().fold(f64::INFINITY, f64::min),
            along.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!((lo + hi).abs() < 1e-9, "{lo} {hi}");
        assert!(hi > 0.08 + 0.016, "{hi}");
        assert_eq!(c.edges.len(), 2, "{:?}", c.edges);
    }
    // The short leg's cover: no bolts on it, over the members' bolts.
    let c = &s.covers[2];
    assert!(c.outline.len() >= 4);
    assert!(c.edges.is_empty());
    // The solids: three covers.
    let solids = crate::plate_solids(&d);
    assert_eq!(solids.iter().filter(|x| x.cover).count(), 3);
}

/// L1 runs on through N2 into L2 as one part: no cut, no shift, no bolts
/// there; one part of 10 m.
#[test]
fn a_continuous_member_is_one_part() {
    let text = format!(
        "{TOWER}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\n\
         onto = \"L2\"\ncut = \"40 mm\"\n"
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    for (m, o) in [("L1", "L2"), ("L2", "L1")] {
        let a = at(&d, "N2", m);
        assert_eq!(a.continuous.as_deref(), Some(o));
        assert_eq!(a.cut.value, Some(0.0));
        assert!(a.bolts.value.is_none());
        assert!(a.bolt_points.is_empty());
        assert!(a.offset.iter().all(|v| *v == 0.0));
    }
    assert!(
        at(&d, "N2", "L1")
            .notes
            .iter()
            .any(|n| n.contains("cut and shift not used")),
    );
    let chains = crate::part_chains(&d);
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].members, vec!["L1".to_owned(), "L2".to_owned()]);
    assert_eq!(chains[0].joints, vec!["N2".to_owned()]);
    assert!((chains[0].length - 10.0).abs() < 1e-9);
    assert!((chains[0].steel - 10.0).abs() < 1e-9);
    // Not one part with the diagonal (not in line): nothing.
    let text = format!(
        "{TOWER}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nkind = \"continuous\"\n\
         onto = \"L2\"\n"
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    assert!(at(&d, "N2", "D1").continuous.is_none());
    assert!(crate::part_chains(&d).is_empty());
}

/// A member end without bolts is seated on what it bolts onto as one
/// with bolts is (owner 2026-11-05: "draw them at the right place in
/// 3d"): the diagonal at N2 sits on the leg's face, not on its work line.
#[test]
fn an_end_without_bolts_is_seated_on_what_it_bolts_onto() {
    let bolted = joint_details(TOWER, &library(), &Default::default()).unwrap();
    let bare = TOWER.replace(
        r#"design = { class = "bracing", bolts = { diameter = "16 mm", count = 1 } }"#,
        r#"design = { class = "bracing" }"#,
    );
    let none = joint_details(&bare, &library(), &Default::default()).unwrap();
    let at = |d: &crate::JointDetails| {
        d.joint("N2")
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == "D1")
            .unwrap()
            .clone()
    };
    let (a, b) = (at(&bolted), at(&none));
    assert!(a.bolts.value.is_some() && b.bolts.value.is_none());
    let size = (0..3)
        .map(|k| a.offset[k] * a.offset[k])
        .sum::<f64>()
        .sqrt();
    assert!(size > 1e-3, "seated off its axis: {:?}", a.offset);
    for k in 0..3 {
        assert!((a.offset[k] - b.offset[k]).abs() < 1e-12, "{a:?} {b:?}");
    }
}

/// `leg = "both"`: the layout's bolts on both legs, shared (the leg on
/// what it bolts onto first), each leg's through its own face (square to
/// it, on the heel's plane); `other_leg_points` gives the other leg's by
/// hand.
#[test]
fn both_legs_bolted_put_bolts_on_each_side_of_the_corner() {
    let text = format!(
        "{TOWER}{}",
        AT_N2.replace(r#"leg = "long""#, r#"leg = "both""#)
    )
    .replace(
        "count = 2, diameter = \"16 mm\", grade",
        "count = 5, diameter = \"16 mm\", grade",
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
    assert_eq!(a.leg.value, Some(BoltedLeg::Both));
    assert_eq!((a.bolt_points.len(), a.other_points.len()), (3, 2));
    let dot = |x: [f64; 3], y: [f64; 3]| (0..3).map(|k| x[k] * y[k]).sum::<f64>();
    assert!(dot(a.bolt_axis, a.other_axis).abs() < 1e-9, "square legs");
    let p = a.profile.as_ref().unwrap();
    let g = p.angle.unwrap();
    let heel: [f64; 3] = std::array::from_fn(|k| {
        n2.position[k] + a.offset[k] + p.axes[1][k] * g.heel[0] + p.axes[2][k] * g.heel[1]
    });
    for (q, axis, _) in a.bolts_with_axes() {
        let rel: [f64; 3] = std::array::from_fn(|k| q[k] - heel[k]);
        assert!(dot(rel, axis).abs() < 1e-9, "on its leg's outer face");
    }
    // The gauge given: on each leg, 30 mm from the heel.
    for q in a.bolt_flat.iter().chain(&a.other_flat) {
        assert!((q[1] - 0.03).abs() < 1e-12);
    }
    // The 3D view's bolts: all five, each with its faces.
    let solids = crate::bolt_solids(&d);
    let here: Vec<_> = solids
        .iter()
        .filter(|b| b.member == "D1" && b.joint == "N2")
        .collect();
    assert_eq!(here.len(), 5);
    assert!(
        here.iter()
            .all(|b| b.faces.len() >= 20 && (b.diameter - 0.016).abs() < 1e-12)
    );
    // The other leg's bolts by hand.
    let given = format!("{text}other_leg_points = [[\"40 mm\", \"25 mm\"]]\n");
    let d = joint_details(&given, &library(), &Default::default()).unwrap();
    let a = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    assert_eq!(a.other_flat, vec![[0.04, 0.025]]);
    assert!(a.bolt_points.is_empty(), "the leg bolted's: none given");
    // One leg bolted: other_leg_points said, not used.
    let one = given.replace(r#"leg = "both""#, r#"leg = "long""#);
    let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
        Path::new("t.ohls"),
        &one,
    )
    .unwrap();
    assert!(
        t.connection_notes()
            .iter()
            .any(|n| n.contains("other_leg_points without leg")),
        "{:?}",
        t.connection_notes()
    );
    let d = joint_details(&one, &library(), &Default::default()).unwrap();
    let a = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    assert!(a.other_points.is_empty());
}

/// Owner, 2026-11-08: "a member bolted to both [plates] with a 90°
/// difference … on 2 different plates if not welded" — both legs bolted,
/// the leg on `onto` (G1) and the other leg on a plate of its own
/// (`other_onto`, G2): G2 gathers the other leg's bolts (its outline round
/// them, square to G1's bolts), their grip takes G2's thickness, and the
/// plate checks share the member's force by the bolts on each. The same
/// with G2 welded to G1. A plate that is not there, or one leg bolted:
/// said, not used.
#[test]
fn both_legs_onto_two_plates_at_ninety_degrees() {
    let two = |welded: bool| {
        format!(
            "{TOWER}{}[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"8 mm\"\n\
             [[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"12 mm\"\n{}",
            AT_N2
                .replace(
                    "leg = \"long\"",
                    "leg = \"both\"\nonto = \"G1\"\nother_onto = \"G2\""
                )
                .replace("count = 2,", "count = 4,"),
            if welded { "welded_to = [\"G1\"]\n" } else { "" }
        )
    };
    let dot = |x: [f64; 3], y: [f64; 3]| (0..3).map(|k| x[k] * y[k]).sum::<f64>();
    for welded in [false, true] {
        let text = two(welded);
        let d = joint_details(&text, &library(), &Default::default()).unwrap();
        assert!(
            !d.notes.iter().any(|n| n.contains("other_onto")),
            "{:?}",
            d.notes
        );
        let n2 = d.joint("N2").unwrap();
        let a = n2.arrivals.iter().find(|a| a.member == "D1").unwrap();
        assert_eq!(a.other_onto.as_deref(), Some("G2"));
        assert_eq!((a.bolt_points.len(), a.other_points.len()), (2, 2));
        assert!(dot(a.bolt_axis, a.other_axis).abs() < 1e-9);
        // The other leg's bolts clamp that leg and G2.
        assert!(
            (a.other_grip[1] - 0.012).abs() < 1e-12,
            "{:?}",
            a.other_grip
        );
        assert!((a.grip[1] - 0.008).abs() < 1e-12, "{:?}", a.grip);
        let g1 = n2.plates.iter().find(|p| p.id == "G1").unwrap();
        let g2 = n2.plates.iter().find(|p| p.id == "G2").unwrap();
        assert_eq!(g1.members, ["D1"]);
        assert_eq!(g2.members, ["D1"]);
        assert!(g2.outline.len() >= 3, "{:?}", g2.notes);
        for b in &a.other_points {
            assert!(inside(&g2.outline, a.other_axis, *b));
        }
        for b in &a.bolt_points {
            assert!(inside(&g1.outline, a.bolt_axis, *b));
        }
        if welded {
            assert_eq!(g2.assembly, ["G1", "G2"]);
        }
        // The checks: each plate half the force (two bolts of four).
        let plans =
            crate::plate_checks::plate_plans(&text, &library(), &Default::default()).unwrap();
        let shares = plans.bolted_shares("D1");
        assert_eq!(shares, vec![0.5, 0.5], "welded {welded}");
    }
    // One leg bolted, or a plate not there: said, not used.
    let one = two(false).replace("leg = \"both\"", "leg = \"long\"");
    let d = joint_details(&one, &library(), &Default::default()).unwrap();
    assert!(d.notes.iter().any(|n| n.contains("other_onto without leg")));
    let a = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .cloned()
        .unwrap();
    assert!(a.other_onto.is_none());
    let gone = two(false).replace("other_onto = \"G2\"", "other_onto = \"G9\"");
    let d = joint_details(&gone, &library(), &Default::default()).unwrap();
    assert!(
        d.notes
            .iter()
            .any(|n| n.contains("other_onto G9: no plate"))
    );
}

/// Each misfit is also a [`crate::Problem`] a fix can be found for
/// (2026-11-08), in the words of its note: a nested angle too large, one
/// turned a quarter, bolts too near the edge (with the largest gauge that
/// keeps both parts' edges), too near the end (by how much).
#[test]
fn misfits_are_problems_with_what_a_fix_needs() {
    use crate::Problem;
    let kinds = |text: &str| -> Vec<(Problem, String)> {
        let d = joint_details(text, &nest_library(), &Default::default()).unwrap();
        at(&d, "N2", "L2").problems.clone()
    };
    let wide = kinds(&with_l2("EA-50x5", ""));
    assert!(
        wide.iter().any(
            |(p, t)| matches!(p, Problem::NestTooWide { host } if host == "L1")
                && t.contains("does not fit")
        ),
        "{wide:?}"
    );
    let turned = kinds(&with_l2("EA-45x5", "\nroll = \"90 deg\""));
    assert!(
        turned
            .iter()
            .any(|(p, t)| matches!(p, Problem::NestTurned { .. }) && t.contains("lie along")),
        "{turned:?}"
    );
    // Bolts 40 mm out on a 45 mm leg: too near the edge; the largest gauge
    // keeping both edges is below 40 mm and above 0.
    let edge =
        kinds(&with_l2("EA-45x5", "").replace("gauges = [\"25 mm\"]", "gauges = [\"40 mm\"]"));
    let g = edge.iter().find_map(|(p, _)| match p {
        Problem::NestEdge { gauge_max, .. } => Some(*gauge_max),
        _ => None,
    });
    assert!(g.is_some_and(|g| g > 0.0 && g < 0.04), "{edge:?}");
    // The first bolt 5 mm from the end: short by the code's end distance
    // less 5 mm.
    let end = kinds(
        &with_l2("EA-45x5", "").replace("end_distance = \"25 mm\"", "end_distance = \"5 mm\""),
    );
    assert!(
        end.iter()
            .any(|(p, _)| matches!(p, Problem::NestEnd { short, .. } if *short > 0.0)),
        "{end:?}"
    );
    // A nest that fits: none.
    assert!(kinds(&with_l2("EA-45x5", "")).is_empty());
}
