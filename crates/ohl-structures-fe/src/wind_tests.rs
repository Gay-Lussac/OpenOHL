// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Faces, solidity and shielding on an invented square tower, against
//! values worked out by hand. Every wind factor here is invented.
//!
//! The tower: legs at (0,0), (2,0), (2,2), (0,2), m; levels z = 0, 4, 8;
//! horizontals at z = 4 and 8 on all four faces; an X in each face's two
//! panels; one plan diagonal at z = 4 from (0,0) to (2,2). Every member a
//! round bar of d = 50 mm (its width the same from every side).
//!
//! A face (say y = 0): 4 leg pieces of 4 m, 2 horizontals of 2 m, 4
//! diagonals of √20 m → Σ L = 20 + 4√20 = 37.8885 m; the members' area
//! d Σ L = 1.894427 m² over the 2 × 8 = 16 m² outline: φ = 0.1184017.
//!
//! Wind along +y, q = 1000 Pa, drag c, the members' force along y
//! (q c d L sin³θ, sin θ = 1 for the faces y = 0 and 2):
//! - face y = 0 (in front): q c d (16 + 4 + 4√20) = c × 1894.427 N;
//! - face y = 2 (behind): the same × η;
//! - the 8 diagonals of the faces x = 0 and 2 (sin²θ = 4/5): each
//!   q c d √20 (4/5)^1.5 = c × 160 N, 1280 c N;
//! - the plan diagonal (sin²θ = 1/2): q c d 2√2 (1/2)^1.5 = c × 50 N;
//! - the horizontals along y: none (along the wind).

use ohl_sections::SectionLibrary;

use crate::model::Model;
use crate::wind::{DragFrom, MemberWindSetup, faces};

const LIBRARY: &str = r#"
[[material]]
id = "steel"
elasticity = "210 GPa"
density = "7850 kg/m3"

[[section]]
id = "B50"
shape = "round_bar"
material = "steel"
d = "50 mm"
"#;

const SQRT20: f64 = 4.47213595499958;

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-12)
}

/// The invented tower as a model file.
fn tower() -> String {
    let corners = [(0, 0), (2, 0), (2, 2), (0, 2)];
    let mut t = String::from("[analysis]\nnonlinear = false\nself_weight = false\n");
    for (c, (x, y)) in corners.iter().enumerate() {
        for (l, z) in [0, 4, 8].iter().enumerate() {
            t.push_str(&format!(
                "[[node]]\nid = \"N{c}{l}\"\npoint = [{x}, {y}, {z}]\n{}",
                if l == 0 { "support = \"pinned\"\n" } else { "" }
            ));
        }
    }
    let mut member = |id: String, a: String, b: String| {
        t.push_str(&format!(
            "[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"B50\"\n"
        ));
    };
    for c in 0..4 {
        let d = (c + 1) % 4;
        for l in 0..2 {
            member(
                format!("leg{c}{l}"),
                format!("N{c}{l}"),
                format!("N{c}{}", l + 1),
            );
            member(
                format!("h{c}{}", l + 1),
                format!("N{c}{}", l + 1),
                format!("N{d}{}", l + 1),
            );
            member(
                format!("x{c}{l}a"),
                format!("N{c}{l}"),
                format!("N{d}{}", l + 1),
            );
            member(
                format!("x{c}{l}b"),
                format!("N{d}{l}"),
                format!("N{c}{}", l + 1),
            );
        }
    }
    member("plan".into(), "N01".into(), "N21".into());
    t
}

fn model() -> Model {
    let lib = SectionLibrary::parse(LIBRARY).unwrap();
    Model::parse_with(&tower(), &lib).unwrap()
}

fn factors(text: &str) -> ohl_sections::StructureWind {
    SectionLibrary::parse(text).unwrap().wind.unwrap()
}

fn total_y(w: &crate::wind::MemberWind) -> f64 {
    w.total()[1]
}

/// Face c (0: y = 0, 1: x = 2, 2: y = 2, 3: x = 0): its members' ids.
fn face_ids(m: &Model, c: usize) -> Vec<String> {
    let d = (c + 1) % 4;
    let mut ids: Vec<String> = Vec::new();
    for l in 0..2 {
        ids.push(format!("leg{c}{l}"));
        ids.push(format!("leg{d}{l}"));
        ids.push(format!("h{c}{}", l + 1));
        ids.push(format!("x{c}{l}a"));
        ids.push(format!("x{c}{l}b"));
    }
    ids.sort();
    assert!(ids.iter().all(|id| m.members.iter().any(|x| &x.id == id)));
    ids
}

#[test]
fn the_tower_has_four_side_faces_and_a_top() {
    let m = model();
    let f = faces(&m);
    // The four sides and the top; not the levels at z = 4 (legs above
    // and below), not the plane of the plan diagonal (through the body).
    assert_eq!(
        f.len(),
        5,
        "{:?}",
        f.iter().map(|f| f.normal).collect::<Vec<_>>()
    );
    let phi = 0.05 * (20.0 + 4.0 * SQRT20) / 16.0;
    let sides: Vec<_> = f.iter().filter(|f| f.normal[2].abs() < 1e-9).collect();
    assert_eq!(sides.len(), 4);
    for s in &sides {
        assert!(close(s.area, 16.0, 1e-12), "{}", s.area);
        assert!(close(s.height, 8.0, 1e-12));
        assert!(close(s.breadth(), 2.0, 1e-12));
        assert!(close(s.solidity, phi, 1e-12), "{} vs {phi}", s.solidity);
        assert_eq!(s.members.len(), 10);
    }
    let front = sides
        .iter()
        .find(|s| s.normal[1].abs() > 0.5 && s.offset.abs() < 1e-9)
        .unwrap();
    let mut ids: Vec<String> = front
        .members
        .iter()
        .map(|k| m.members[*k].id.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, face_ids(&m, 0));
    // The top: the four horizontals at z = 8, 2 × 2 m.
    let top = f.iter().find(|f| f.normal[2].abs() > 0.5).unwrap();
    assert_eq!(top.members.len(), 4);
    assert!(close(top.area, 4.0, 1e-12));
}

#[test]
fn without_factors_the_wind_is_as_before() {
    let m = model();
    let plain = m.member_wind(1000.0, [0.0, 1.0, 0.0]);
    let with = m.member_wind_with(1000.0, [0.0, 1.0, 0.0], None);
    assert_eq!(plain, with.loads);
    assert!(
        MemberWindSetup::new(&m, None).faces().is_empty(),
        "no faces needed"
    );
    // Generic drag 1: 2 × 1894.427 + 1280 + 50 N.
    let expected = 2.0 * 1000.0 * 0.05 * (20.0 + 4.0 * SQRT20) + 1280.0 + 50.0;
    assert!(close(total_y(&with), expected, 1e-12), "{}", total_y(&with));
    assert!(
        with.members
            .iter()
            .all(|r| r.drag_from == DragFrom::Generic)
    );
    assert!(with.notes[0].contains("generic drag") && with.notes[0].contains("no shielding"));
}

#[test]
fn the_face_behind_is_shielded_by_the_one_in_front() {
    let m = model();
    // η = 1 − φ (an invented line through (0, 1) and (0.5, 0.5)).
    let f = factors(
        "[wind]\nsource = \"invented\"\ndrag = { round_bar = 1.2 }\n\
         [wind.shielding]\nsolidity = [0, 0.5]\nfactor = [1, 0.5]\n",
    );
    let setup = MemberWindSetup::new(&m, Some(&f));
    let w = setup.wind(1000.0, [0.0, 1.0, 0.0]);
    let phi = 0.05 * (20.0 + 4.0 * SQRT20) / 16.0;
    let eta = 1.0 - phi;
    let face = 1000.0 * 0.05 * (20.0 + 4.0 * SQRT20);
    let expected = 1.2 * (face * (1.0 + eta) + 1280.0 + 50.0);
    assert!(
        close(total_y(&w), expected, 1e-12),
        "{} vs {expected}",
        total_y(&w)
    );
    // Exactly the face y = 2 is shielded, 2 m behind the face of 2 m
    // breadth: spacing ratio 1.
    let mut shielded: Vec<String> = w
        .members
        .iter()
        .filter(|r| r.shielded_by.is_some())
        .map(|r| m.members[r.member].id.clone())
        .collect();
    shielded.sort();
    assert_eq!(shielded, face_ids(&m, 2));
    for r in w.members.iter().filter(|r| r.shielded_by.is_some()) {
        assert!(close(r.shielding, eta, 1e-12));
        assert!(close(r.shielded_by.unwrap().1, 1.0, 1e-12));
        assert_eq!(r.drag_from, DragFrom::Shape);
    }
    assert!(w.notes[0].contains("invented"), "{:?}", w.notes);
    assert!(w.notes[1].contains("10 member(s)"), "{:?}", w.notes);
    // The same with the spacing ratio: rows 1 − 1.2 φ at spacing 0.5 and
    // 1 − 0.8 φ at 1.5; halfway, at 1: 1 − φ again.
    let f = factors(
        "[wind]\ndrag = { round_bar = 1.2 }\n[wind.shielding]\nsolidity = [0, 0.5]\n\
         spacing = [0.5, 1.5]\nfactor = [[1, 0.4], [1, 0.6]]\n",
    );
    let w2 = MemberWindSetup::new(&m, Some(&f)).wind(1000.0, [0.0, 1.0, 0.0]);
    assert!(close(total_y(&w2), expected, 1e-12));
    // Spacing outside the table: not shielded, and said.
    let f = factors(
        "[wind]\ndrag = { round_bar = 1.2 }\n[wind.shielding]\nsolidity = [0, 0.5]\n\
         spacing = [2, 3]\nfactor = [[0.9, 0.4], [1, 0.6]]\n",
    );
    let w3 = MemberWindSetup::new(&m, Some(&f)).wind(1000.0, [0.0, 1.0, 0.0]);
    let unshielded = 1.2 * (2.0 * face + 1280.0 + 50.0);
    assert!(close(total_y(&w3), unshielded, 1e-12));
    assert!(
        w3.notes[1].contains("outside the shielding table"),
        "{:?}",
        w3.notes
    );
}

#[test]
fn a_face_drag_by_solidity_takes_over_on_the_faces() {
    let m = model();
    // Invented: 3.0 at φ = 0.1, 2.0 at 0.2 → 3 − 10 (φ − 0.1).
    let f = factors(
        "[wind]\ndrag = { round_bar = 1.2 }\n[wind.face_drag]\nsolidity = [0.1, 0.2]\n\
         factor = [3.0, 2.0]\n",
    );
    let w = m.member_wind_with(1000.0, [0.0, 1.0, 0.0], Some(&f));
    let phi = 0.05 * (20.0 + 4.0 * SQRT20) / 16.0;
    let c = 3.0 - 10.0 * (phi - 0.1);
    let face = 1000.0 * 0.05 * (20.0 + 4.0 * SQRT20);
    // Faces y = 0 and 2 (facing the wind) by their solidity; the side
    // diagonals (edge-on faces) and the plan diagonal by their shape.
    let expected = c * 2.0 * face + 1.2 * (1280.0 + 50.0);
    assert!(
        close(total_y(&w), expected, 1e-12),
        "{} vs {expected}",
        total_y(&w)
    );
    assert_eq!(
        w.members
            .iter()
            .filter(|r| r.drag_from == DragFrom::Face)
            .count(),
        20,
        "the members of the two faces across the wind, the horizontals included"
    );
    assert!(w.notes[1].contains("no shielding"), "{:?}", w.notes);
    // A shape without a factor: the generic one, named.
    let f = factors("[wind]\ndrag = { equal_angle = 2.0 }\n");
    let w = m.member_wind_with(1000.0, [0.0, 1.0, 0.0], Some(&f));
    assert!(w.members.iter().all(|r| r.drag_from == DragFrom::Generic));
    assert!(
        w.notes[0].contains("no drag factor given for round_bar"),
        "{:?}",
        w.notes
    );
}

#[test]
fn at_45_degrees_the_two_back_faces_are_shielded() {
    let m = model();
    let f = factors("[wind.shielding]\nsolidity = [0, 0.5]\nfactor = [1, 0.5]\n");
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let w = m.member_wind_with(1000.0, [s, s, 0.0], Some(&f));
    let shielded: Vec<&str> = w
        .members
        .iter()
        .filter(|r| r.shielded_by.is_some())
        .map(|r| m.members[r.member].id.as_str())
        .collect();
    // The far leg (2,2) is behind both near faces; the legs (2,0) and
    // (0,2) stand in a near face; the near leg (0,0) in both.
    for l in 0..2 {
        assert!(
            shielded.contains(&format!("leg2{l}").as_str()),
            "{shielded:?}"
        );
        for c in [0, 1, 3] {
            assert!(
                !shielded.contains(&format!("leg{c}{l}").as_str()),
                "{shielded:?}"
            );
        }
    }
    // The bracing of the far faces (x = 2: face 1, y = 2: face 2) is
    // behind; the near faces' is not.
    for c in [1, 2] {
        for id in ["x{c}0a", "x{c}1b", "h{c}1"] {
            let id = id.replace("{c}", &c.to_string());
            assert!(shielded.contains(&id.as_str()), "{id}: {shielded:?}");
        }
    }
    for c in [0, 3] {
        assert!(
            !shielded.contains(&format!("x{c}0a").as_str()),
            "{shielded:?}"
        );
    }
    // The plan diagonal is in no face: never shielded.
    assert!(!shielded.contains(&"plan"));
}

/// A tapered square tower of `panels` panels 2 m high, its half width
/// 1 m at the top and `batter` m more per metre down; horizontals at
/// every level above the feet, an X in each face's panels.
fn tapered(panels: usize, batter: f64) -> Model {
    let h = 2.0 * panels as f64;
    let mut t = String::from("[analysis]\nnonlinear = false\nself_weight = false\n");
    let signs = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for (c, (sx, sy)) in signs.iter().enumerate() {
        for l in 0..=panels {
            let z = 2.0 * l as f64;
            let w = 1.0 + batter * (h - z);
            t.push_str(&format!(
                "[[node]]\nid = \"N{c}_{l}\"\npoint = [{}, {}, {z}]\n{}",
                sx * w,
                sy * w,
                if l == 0 { "support = \"pinned\"\n" } else { "" }
            ));
        }
    }
    let mut member = |id: String, a: String, b: String| {
        t.push_str(&format!(
            "[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"B50\"\n"
        ));
    };
    for c in 0..4 {
        let d = (c + 1) % 4;
        for l in 0..panels {
            let u = l + 1;
            member(
                format!("leg{c}_{l}"),
                format!("N{c}_{l}"),
                format!("N{c}_{u}"),
            );
            member(
                format!("h{c}_{u}"),
                format!("N{c}_{u}"),
                format!("N{d}_{u}"),
            );
            member(
                format!("x{c}_{l}a"),
                format!("N{c}_{l}"),
                format!("N{d}_{u}"),
            );
            member(
                format!("x{c}_{l}b"),
                format!("N{d}_{l}"),
                format!("N{c}_{u}"),
            );
        }
    }
    let lib = SectionLibrary::parse(LIBRARY).unwrap();
    Model::parse_with(&t, &lib).unwrap()
}

/// A tapered tower of 960 members: its four sides are one face each (the
/// panels in one plane), leaning, so the far face is not parallel to the
/// near one; still behind it, every member of it shielded. Expected
/// cost: well under a second (a few milliseconds in a release build).
#[test]
fn a_tapered_tower_of_960_members() {
    let m = tapered(60, 0.02);
    assert_eq!(m.members.len(), 960);
    let f = factors("[wind.shielding]\nsolidity = [0, 0.5]\nfactor = [1, 0.5]\n");
    let started = std::time::Instant::now();
    let setup = MemberWindSetup::new(&m, Some(&f));
    let w = setup.wind(1000.0, [0.0, 1.0, 0.0]);
    let took = started.elapsed();
    assert!(took.as_secs_f64() < 5.0, "{took:?}");
    let sides = setup
        .faces()
        .iter()
        .filter(|f| f.members.len() == 300)
        .count();
    assert_eq!(sides, 4, "one face per side: {}", setup.faces().len());
    // The far side (corners 2 and 3, face 2) is shielded, all of it.
    let shielded: Vec<&str> = w
        .members
        .iter()
        .filter(|r| r.shielded_by.is_some())
        .map(|r| m.members[r.member].id.as_str())
        .collect();
    assert_eq!(shielded.len(), 300, "{}", shielded.len());
    assert!(shielded.iter().all(|id| {
        ["x2_", "h2_", "leg2_", "leg3_"]
            .iter()
            .any(|p| id.starts_with(p))
    }));
}
