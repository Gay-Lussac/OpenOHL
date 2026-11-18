// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Orient angles from the tower's shape, on invented towers: a square and
//! a rectangular body, a crossarm, a body whose taper changes; plan
//! members laid flat, face members left; values given by hand kept unless
//! replaced; a run again changing nothing.

use std::collections::BTreeSet;
use std::fmt::Write;

use ohl_structures::{LibraryItem, StructureType};

use crate::joints_tests::library;
use crate::layout_run::LayoutRecord;
use crate::orient::{OrientAsk, OrientKind, OrientRun, orient_angles};

/// A body of four legs: `levels` (z, half widths along x and y), each
/// level's corners joined by face horizontals, each panel's faces by one
/// diagonal, and plan diagonals at the levels in `plan` (horizontal, in no
/// face). Legs class `leg`, the rest `bracing`; no rolls.
pub(crate) fn body(levels: &[(f64, f64, f64)], plan: &[usize]) -> String {
    let mut s = String::from("id = \"B\"\nname = \"b\"\nkind = \"lattice\"\n");
    let corners = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    for (i, (z, hx, hy)) in levels.iter().enumerate() {
        for (c, (sx, sy)) in corners.iter().enumerate() {
            let _ = write!(
                s,
                "\n[[node]]\nid = \"J{i}{c}\"\npoint = [\"{} m\", \"{} m\", \"{z} m\"]\n{}",
                sx * hx,
                sy * hy,
                if i == 0 { "support = \"fixed\"\n" } else { "" }
            );
        }
    }
    let mut member = |id: String, a: String, b: String, class: &str| {
        let _ = write!(
            s,
            "\n[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"EA-61x7\"\n\
             kind = \"frame\"\ndesign = {{ class = \"{class}\" }}\n"
        );
    };
    for i in 0..levels.len() - 1 {
        for c in 0..4 {
            member(
                format!("L{i}{c}"),
                format!("J{i}{c}"),
                format!("J{}{c}", i + 1),
                "leg",
            );
            member(
                format!("D{i}{c}"),
                format!("J{i}{c}"),
                format!("J{}{}", i + 1, (c + 1) % 4),
                "bracing",
            );
        }
    }
    for i in 1..levels.len() {
        for c in 0..4 {
            member(
                format!("H{i}{c}"),
                format!("J{i}{c}"),
                format!("J{i}{}", (c + 1) % 4),
                "bracing",
            );
        }
        if plan.contains(&i) {
            member(
                format!("P{i}0"),
                format!("J{i}0"),
                format!("J{i}2"),
                "bracing",
            );
            member(
                format!("P{i}1"),
                format!("J{i}1"),
                format!("J{i}3"),
                "bracing",
            );
        }
    }
    s
}

fn orient(text: &str, replace: bool) -> OrientRun {
    orient_angles(
        text,
        &library(),
        &OrientAsk {
            members: None,
            replace_hand: replace,
        },
    )
    .unwrap()
}

/// A member, its axes (x, y, z) and its ends, as placed.
type Placed = (String, [[f64; 3]; 3], [f64; 3], [f64; 3]);

/// Each member's axes and ends, as placed.
fn axes(text: &str) -> Vec<Placed> {
    let e = crate::export_structure(text, &library(), None, &[]).unwrap();
    e.members
        .iter()
        .map(|m| {
            (
                m.id.clone(),
                m.axes,
                e.nodes[m.nodes[0]].point,
                e.nodes[m.nodes[1]].point,
            )
        })
        .collect()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The legs of `text` (ids starting with `prefix`) with their heel out
/// of the tower (both legs of the angle pointing into it, towards the
/// axis through `centre`) and each flange within `off` degrees of one of
/// the two faces meeting at the leg (given by its neighbours' lines).
fn assert_heel_out_flanges_in_faces(
    text: &str,
    prefix: &str,
    centre: impl Fn([f64; 3]) -> [f64; 3],
    neighbours: impl Fn(&str) -> [String; 2],
    off: f64,
) {
    let all = axes(text);
    let find = |id: &str| all.iter().find(|m| m.0 == id).unwrap();
    let mut n = 0;
    for (id, ax, a, b) in all.iter().filter(|m| m.0.starts_with(prefix)) {
        let mid = [0, 1, 2].map(|k| (a[k] + b[k]) / 2.0);
        let c = centre(mid);
        let inward = unit([0, 1, 2].map(|k| c[k] - mid[k]));
        // Inward: normal to the member.
        let inward = unit([0, 1, 2].map(|k| inward[k] - dot(inward, ax[0]) * ax[0][k]));
        assert!(dot(ax[1], inward) > 0.3, "{id}: y {:?} not inward", ax[1]);
        assert!(dot(ax[2], inward) > 0.3, "{id}: z {:?} not inward", ax[2]);
        // The faces: each through the member and a neighbour's middle.
        let faces: Vec<[f64; 3]> = neighbours(id)
            .iter()
            .map(|o| {
                let (_, _, p, q) = find(o);
                let om = [0, 1, 2].map(|k| (p[k] + q[k]) / 2.0);
                unit(cross(ax[0], [0, 1, 2].map(|k| om[k] - mid[k])))
            })
            .collect();
        for leg in [ax[1], ax[2]] {
            let best = faces
                .iter()
                .map(|f| dot(leg, *f).abs().asin().to_degrees())
                .fold(f64::INFINITY, f64::min);
            assert!(best <= off + 0.05, "{id}: a flange {best:.2}° off its face");
        }
        n += 1;
    }
    assert!(n > 0, "no member {prefix}…");
}

fn axis(p: [f64; 3]) -> [f64; 3] {
    [0.0, 0.0, p[2]]
}

/// The neighbours of leg `L{i}{c}`: the legs of the next and the last
/// corner in the same panel.
fn body_neighbours(id: &str) -> [String; 2] {
    let i = &id[1..2];
    let c: usize = id[2..3].parse().unwrap();
    [
        format!("L{i}{}", (c + 1) % 4),
        format!("L{i}{}", (c + 3) % 4),
    ]
}

#[test]
fn a_square_body_has_its_legs_heel_out_with_their_flanges_in_its_faces() {
    let text = body(&[(0.0, 3.0, 3.0), (5.0, 2.5, 2.5), (10.0, 2.0, 2.0)], &[1]);
    let r = orient(&text, false);
    let legs: Vec<&str> = r
        .turned
        .iter()
        .filter(|o| o.kind == OrientKind::Chord)
        .map(|o| o.member.as_str())
        .collect();
    assert_eq!(legs.len(), 8, "{legs:?} left {:?}", r.left);
    // A tapering square body: the faces meet at nearly 90° in the plane
    // normal to the leg; each flange off its face by half the difference.
    for o in r.turned.iter().filter(|o| o.kind == OrientKind::Chord) {
        assert!(o.off < 1.0, "{o:?}");
        assert_eq!(o.neighbours.len(), 2);
    }
    assert_heel_out_flanges_in_faces(&r.text, "L", axis, body_neighbours, 1.0);
    // The plan diagonals laid flat (one leg horizontal, the other down);
    // the face diagonals and horizontals laid in their faces (tuning on
    // real towers, 2026-11-18).
    let plan: BTreeSet<&str> = r
        .turned
        .iter()
        .chain(&r.same)
        .filter(|o| o.kind == OrientKind::Plan)
        .map(|o| o.member.as_str())
        .collect();
    assert_eq!(plan, BTreeSet::from(["P10", "P11"]), "{:?}", r.turned);
    for (id, ax, ..) in axes(&r.text).iter().filter(|m| m.0.starts_with('P')) {
        let down = [ax[1][2], ax[2][2]];
        assert!(
            (down[0] + 1.0).abs() < 1e-6 && down[1].abs() < 1e-6
                || (down[1] + 1.0).abs() < 1e-6 && down[0].abs() < 1e-6,
            "{id}: {ax:?}"
        );
    }
    // Marked automatic, a run again changes nothing.
    let rec = LayoutRecord::of(&StructureType::parse(std::path::Path::new("s"), &r.text).unwrap());
    assert_eq!(rec.oriented.len(), 26, "{:?}", rec.oriented);
    assert_eq!(rec.oriented["L00"].1, OrientKind::Chord);
    assert_eq!(rec.oriented["P10"].1, OrientKind::Plan);
    assert_eq!(rec.oriented["D00"].1, OrientKind::Face);
    assert_eq!(rec.oriented["H10"].1, OrientKind::Face);
    assert_in_their_faces(&r, &r.text, axis);
    let again = orient(&r.text, false);
    assert_eq!(again.text, r.text);
    assert!(again.turned.is_empty(), "{:?}", again.turned);
}

#[test]
fn a_rectangular_body_too() {
    let text = body(&[(0.0, 4.0, 2.0), (6.0, 3.0, 1.5), (12.0, 2.0, 1.0)], &[]);
    let r = orient(&text, false);
    assert_eq!(
        r.turned
            .iter()
            .filter(|o| o.kind == OrientKind::Chord)
            .count(),
        8,
        "{:?}",
        r.left
    );
    let off = r.turned.iter().map(|o| o.off).fold(0.0, f64::max);
    assert!(off < 2.0, "{off}");
    assert_heel_out_flanges_in_faces(&r.text, "L", axis, body_neighbours, off);
}

#[test]
fn a_body_whose_taper_changes_has_each_part_turned_to_its_own_faces() {
    // Steeper below 4 m (a taper change: the legs run on within 10°).
    let text = body(&[(0.0, 4.0, 4.0), (4.0, 2.5, 2.5), (10.0, 2.0, 2.0)], &[]);
    let r = orient(&text, false);
    assert_eq!(
        r.turned
            .iter()
            .filter(|o| o.kind == OrientKind::Chord)
            .count(),
        8,
        "{:?}",
        r.left
    );
    let off = r.turned.iter().map(|o| o.off).fold(0.0, f64::max);
    assert!(off < 5.0, "{off}");
    assert_heel_out_flanges_in_faces(&r.text, "L", axis, body_neighbours, off);
}

/// A box crossarm of four chords (each of two members running on through
/// a joint, class `bracing` as an import gives them) out along +x from a
/// body, its faces braced, a horizontal in its bottom face.
fn crossarm() -> String {
    let mut s = body(&[(0.0, 3.0, 3.0), (8.0, 2.0, 2.0), (10.0, 2.0, 2.0)], &[]);
    // The arm's root: the body's corners at 8 m and 10 m on its +x face
    // (J10 = (2, 2, 8), J13 = (2, −2, 8), J20 = (2, 2, 10), J23 = (2, −2,
    // 10)); its chords towards one apex (14, 0, 9): its faces plane.
    let pts = [
        ("A0", [5.0, 1.5, 8.25]),
        ("A1", [5.0, -1.5, 8.25]),
        ("A2", [5.0, 1.5, 9.75]),
        ("A3", [5.0, -1.5, 9.75]),
        ("T0", [8.0, 1.0, 8.5]),
        ("T1", [8.0, -1.0, 8.5]),
        ("T2", [8.0, 1.0, 9.5]),
        ("T3", [8.0, -1.0, 9.5]),
    ];
    for (id, p) in pts {
        let _ = write!(
            s,
            "\n[[node]]\nid = \"{id}\"\npoint = [\"{} m\", \"{} m\", \"{} m\"]\n",
            p[0], p[1], p[2]
        );
    }
    let mut member = |id: &str, a: &str, b: &str| {
        let _ = write!(
            s,
            "\n[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"EA-61x7\"\n\
             kind = \"frame\"\ndesign = {{ class = \"bracing\" }}\n"
        );
    };
    for (k, root) in ["J10", "J13", "J20", "J23"].iter().enumerate() {
        member(&format!("C{k}a"), root, &format!("A{k}"));
        member(&format!("C{k}b"), &format!("A{k}"), &format!("T{k}"));
    }
    // The arm's faces: bottom (0, 1), top (2, 3), sides (0, 2), (1, 3).
    for (id, a, b) in [
        ("AB", "A0", "A1"),
        ("AT", "A2", "A3"),
        ("AS0", "A0", "A2"),
        ("AS1", "A1", "A3"),
        ("TB", "T0", "T1"),
        ("TT", "T2", "T3"),
        ("TS0", "T0", "T2"),
        ("TS1", "T1", "T3"),
        ("XB", "J10", "A1"),
        ("XS", "J10", "A2"),
    ] {
        member(id, a, b);
    }
    s
}

#[test]
fn a_crossarm_has_its_chords_turned_into_its_faces() {
    let text = crossarm();
    let r = orient(&text, false);
    let chords: BTreeSet<&str> = r
        .turned
        .iter()
        .filter(|o| o.kind == OrientKind::Chord && o.member.starts_with('C'))
        .map(|o| o.member.as_str())
        .collect();
    assert_eq!(chords.len(), 8, "{chords:?}; left {:?}", r.left);
    // The arm's axis: along x through the middle of its section.
    // The arm's axis: from the root's middle (2, 0, 9) to the apex.
    let centre = |p: [f64; 3]| [p[0], 0.0, 9.0];
    let neighbours = |id: &str| -> [String; 2] {
        let k: usize = id[1..2].parse().unwrap();
        let part = &id[2..3];
        let (a, b) = match k {
            0 => (1, 2),
            1 => (0, 3),
            2 => (3, 0),
            _ => (2, 1),
        };
        [format!("C{a}{part}"), format!("C{b}{part}")]
    };
    assert_heel_out_flanges_in_faces(&r.text, "C", centre, neighbours, 1.0);
    // The arm's face members are not plan members of the body: those of
    // its steep sides lie in their faces; the horizontals of its bottom and
    // top faces lie flat with them.
    for o in r.turned.iter().chain(&r.same) {
        if o.member.starts_with("AS") || o.member.starts_with("TS") || o.member == "XS" {
            assert_eq!(o.kind, OrientKind::Face, "{o:?}");
        }
    }
}

#[test]
fn rolls_given_by_hand_are_kept_unless_replaced() {
    let text = body(&[(0.0, 3.0, 3.0), (5.0, 2.5, 2.5)], &[]).replacen(
        "id = \"L00\"\nnodes = [\"J00\", \"J10\"]\nsection = \"EA-61x7\"\n",
        "id = \"L00\"\nnodes = [\"J00\", \"J10\"]\nsection = \"EA-61x7\"\nroll = \"180 deg\"\n",
        1,
    );
    assert!(text.contains("roll = \"180 deg\""));
    let r = orient(&text, false);
    assert!(r.kept.iter().any(|o| o.member == "L00"), "{:?}", r.kept);
    assert!(r.turned.iter().all(|o| o.member != "L00"));
    let t = StructureType::parse(std::path::Path::new("s"), &r.text).unwrap();
    assert!(!LayoutRecord::of(&t).oriented.contains_key("L00"));
    // Replaced: turned, the roll before kept in the record.
    let r = orient(&text, true);
    let o = r.turned.iter().find(|o| o.member == "L00").unwrap();
    assert_eq!(o.before, "180 deg");
    let t = StructureType::parse(std::path::Path::new("s"), &r.text).unwrap();
    assert_eq!(LayoutRecord::of(&t).oriented["L00"].0, "180 deg");
    // A run again (the roll now automatic) changes nothing, kept or not.
    assert_eq!(orient(&r.text, false).text, r.text);
    assert_eq!(orient(&r.text, true).text, r.text);
}

#[test]
fn legs_with_fewer_than_two_neighbours_are_left_with_why() {
    // Two legs only (a peak of two legs): no corner, said.
    let text = r#"id = "P"
name = "p"
kind = "lattice"

[[node]]
id = "A"
point = ["-1 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "B"
point = ["1 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "C"
point = ["-0.5 m", "0 m", "4 m"]

[[node]]
id = "D"
point = ["0.5 m", "0 m", "4 m"]

[[member]]
id = "L1"
nodes = ["A", "C"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["B", "D"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "H"
nodes = ["C", "D"]
section = "EA-61x7"
"#;
    let r = orient(text, false);
    // The legs left; the frame's plane is a face all the same: H lies in it.
    assert!(
        r.turned.iter().all(|o| o.kind == OrientKind::Face),
        "{:?}",
        r.turned
    );
    let left: Vec<&str> = r.left.iter().map(|x| x.0.as_str()).collect();
    assert_eq!(left, ["L1", "L2"]);
    assert!(r.left[0].1.contains("fewer than two"), "{:?}", r.left);
    // Nothing else changed.
    let t = StructureType::parse(std::path::Path::new("s"), &r.text).unwrap();
    assert_eq!(
        LayoutRecord::of(&t).oriented.keys().collect::<Vec<_>>(),
        ["H"]
    );
}

#[test]
fn only_the_members_asked() {
    let text = body(&[(0.0, 3.0, 3.0), (5.0, 2.5, 2.5)], &[]);
    let r = orient_angles(
        &text,
        &library(),
        &OrientAsk {
            members: Some(BTreeSet::from(["L00".to_owned()])),
            replace_hand: false,
        },
    )
    .unwrap();
    let turned: Vec<&str> = r.turned.iter().map(|o| o.member.as_str()).collect();
    assert_eq!(turned, ["L00"]);
}

/// A portal: two square columns 16 m apart (the second a copy of the
/// first, its ids `K…`, `M…`, `E…`, `G…`), joined at their tops by a
/// beam chord (two members through a middle joint).
fn portal() -> String {
    let one = body(&[(0.0, 0.5, 0.5), (4.0, 0.5, 0.5), (8.0, 0.5, 0.5)], &[]);
    let body_part = &one[one.find("\n[[node]]").unwrap()..];
    let mut other = String::new();
    for line in body_part.lines() {
        let line = line
            .replace("\"J", "\"K")
            .replace("id = \"L", "id = \"M")
            .replace("id = \"D", "id = \"E")
            .replace("id = \"H", "id = \"G");
        match line.strip_prefix("point = [\"") {
            Some(rest) => {
                let (x, tail) = rest.split_once(" m\"").unwrap();
                let x: f64 = x.parse().unwrap();
                let _ = writeln!(other, "point = [\"{} m\"{tail}", x + 16.0);
            }
            None => {
                let _ = writeln!(other, "{line}");
            }
        }
    }
    // The beam: one chord of two members through a middle joint.
    let beam = "\n[[node]]\nid = \"Z\"\npoint = [\"8 m\", \"0.5 m\", \"8 m\"]\n\
                \n[[member]]\nid = \"B1\"\nnodes = [\"J20\", \"Z\"]\nsection = \"EA-61x7\"\n\
                \n[[member]]\nid = \"B2\"\nnodes = [\"Z\", \"K21\"]\nsection = \"EA-61x7\"\n";
    format!("{one}{other}{beam}")
}

#[test]
fn a_portals_columns_each_have_their_own_faces() {
    let text = portal();
    let r = orient(&text, false);
    let legs = r
        .turned
        .iter()
        .chain(&r.same)
        .filter(|o| {
            o.kind == OrientKind::Chord && (o.member.starts_with('L') || o.member.starts_with('M'))
        })
        .count();
    assert_eq!(legs, 16, "left {:?}", r.left);
    // Each leg heel out of its own column, its neighbours in its column.
    for o in r.turned.iter().filter(|o| o.member.starts_with('M')) {
        assert!(o.neighbours.iter().all(|n| n.starts_with('M')), "{o:?}");
    }
    let column = |p: [f64; 3]| {
        if p[0] > 8.0 {
            [16.0, 0.0, p[2]]
        } else {
            [0.0, 0.0, p[2]]
        }
    };
    assert_heel_out_flanges_in_faces(&r.text, "L", column, body_neighbours, 0.5);
}

/// Each face member (kind `face`) of `run` lies with one leg in a steep
/// face of the tower through it — its other leg along the face's normal,
/// pointing out of the tower (away from the axis through `centre`) — and
/// the leg in the face points down from the heel (an inclined member).
fn assert_in_their_faces(run: &OrientRun, text: &str, centre: impl Fn([f64; 3]) -> [f64; 3]) {
    let all = axes(text);
    let mut n = 0;
    for o in run
        .turned
        .iter()
        .chain(&run.same)
        .filter(|o| o.kind == OrientKind::Face)
    {
        let (_, ax, a, b) = all.iter().find(|m| m.0 == o.member).unwrap();
        let mid = [0, 1, 2].map(|k| (a[k] + b[k]) / 2.0);
        let c = centre(mid);
        let out = unit([mid[0] - c[0], mid[1] - c[1], 0.0]);
        // The leg out of the face: the axis most nearly horizontal-outward.
        let (normal, inface) = if dot(ax[1], out).abs() > dot(ax[2], out).abs() {
            (ax[1], ax[2])
        } else {
            (ax[2], ax[1])
        };
        assert!(dot(normal, out) > 0.5, "{}: {normal:?} not out", o.member);
        if ax[0][2].abs() > 0.1 {
            assert!(
                inface[2] <= 1e-9,
                "{}: the leg in the face {inface:?} up",
                o.member
            );
        }
        n += 1;
    }
    assert!(n > 0);
}

/// A square body whose faces are braced by crossing diagonals, each in
/// two members meeting at the crossing (so running on in line there).
fn x_braced(levels: &[(f64, f64, f64)]) -> String {
    let mut s = body(levels, &[]);
    // The single diagonals out: each face's two crossing in its middle.
    let mut keep = String::new();
    for block in s.split("\n[[member]]").collect::<Vec<_>>() {
        if block.starts_with("\nid = \"D") {
            continue;
        }
        if !keep.is_empty() {
            keep.push_str("\n[[member]]");
        }
        keep.push_str(block);
    }
    s = keep;
    let corner = |i: usize, c: usize| {
        let (z, hx, hy) = levels[i];
        let (sx, sy) = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][c];
        [sx * hx, sy * hy, z]
    };
    for i in 0..levels.len() - 1 {
        for c in 0..4 {
            let d = (c + 1) % 4;
            let (a, b, e, f) = (
                corner(i, c),
                corner(i, d),
                corner(i + 1, c),
                corner(i + 1, d),
            );
            // Where A–F and B–E cross (a trapezoid's diagonals).
            let t = {
                let u = [0, 1, 2].map(|k| f[k] - a[k]);
                let v = [0, 1, 2].map(|k| e[k] - b[k]);
                let w = [0, 1, 2].map(|k| b[k] - a[k]);
                let uv = cross(u, v);
                dot(cross(w, v), uv) / dot(uv, uv)
            };
            let x = [0, 1, 2].map(|k| a[k] + (f[k] - a[k]) * t);
            let _ = write!(
                s,
                "\n[[node]]\nid = \"X{i}{c}\"\npoint = [\"{} m\", \"{} m\", \"{} m\"]\n",
                x[0], x[1], x[2]
            );
            for (id, from, to) in [
                (format!("Da{i}{c}"), format!("J{i}{c}"), format!("X{i}{c}")),
                (
                    format!("Db{i}{c}"),
                    format!("X{i}{c}"),
                    format!("J{}{d}", i + 1),
                ),
                (format!("Ea{i}{c}"), format!("J{i}{d}"), format!("X{i}{c}")),
                (
                    format!("Eb{i}{c}"),
                    format!("X{i}{c}"),
                    format!("J{}{c}", i + 1),
                ),
            ] {
                let _ = write!(
                    s,
                    "\n[[member]]\nid = \"{id}\"\nnodes = [\"{from}\", \"{to}\"]\nsection = \"EA-61x7\"\n\
                     kind = \"frame\"\ndesign = {{ class = \"bracing\" }}\n"
                );
            }
        }
    }
    s
}

/// Tuning on real towers (2026-11-18): a face's diagonals running on in
/// line through their crossing are bracing of that face, not chords: they
/// lie in it (one leg in the face, the other out of the tower), and the
/// legs are turned as before.
#[test]
fn crossing_diagonals_running_on_lie_in_their_face_not_as_chords() {
    let text = x_braced(&[(0.0, 3.0, 3.0), (5.0, 2.5, 2.5), (10.0, 2.0, 2.0)]);
    let r = orient(&text, false);
    for o in r.turned.iter().chain(&r.same) {
        if o.member.starts_with('D') || o.member.starts_with('E') {
            assert_eq!(o.kind, OrientKind::Face, "{o:?}");
        }
    }
    let faces = r
        .turned
        .iter()
        .chain(&r.same)
        .filter(|o| {
            o.kind == OrientKind::Face && (o.member.starts_with('D') || o.member.starts_with('E'))
        })
        .count();
    assert_eq!(faces, 32, "{:?}", r.turned);
    let legs = r
        .turned
        .iter()
        .filter(|o| o.kind == OrientKind::Chord)
        .count();
    assert_eq!(legs, 8, "{:?}", r.left);
    assert_heel_out_flanges_in_faces(&r.text, "L", axis, body_neighbours, 1.0);
    assert_in_their_faces(&r, &r.text, axis);
    assert_eq!(orient(&r.text, false).text, r.text);
}
