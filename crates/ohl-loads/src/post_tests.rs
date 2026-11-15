// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Post insulators (owner, 2026-11-14): the wire point a post holds is
//! fixed at its top; its load goes to the post's base point with its
//! lever, and the post's own weight and wind at its middle.

use std::path::Path;

use super::*;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

/// Three spans, T2 holding the conductor on an upright post 1.6 m tall
/// (its base A1 at 28.4 m, its top W1 at 30 m).
fn on_a_post() -> Line {
    Line::from_json(POST_LINE).unwrap()
}

const POST_LINE: &str = r#"{
  "format": "ohl-line", "version": 17, "name": "post", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 30], "set": 1}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 28.4]}],
     "strings": [{"label": "P1", "top": "A1", "wire": "W1", "length": 1.6,
                  "angle_deg": 180.0, "weight": 450.0, "wind_area": 0.3,
                  "post": true, "cantilever_load": 8000.0}],
     "wires": [{"label": "W1", "position": [300, 0, 30], "set": 1}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [600, 0, 30], "set": 1}]}
  ],
  "sections": [{"id": "S", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}}]
}"#;

fn weather() -> Vec<WeatherCase> {
    ohl_weather::WeatherCases::parse(
        "[[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"500 Pa\"\n",
    )
    .unwrap()
    .cases
}

/// The conductor's load at T2 acts at the post's top: a load of A1 with a
/// lever of 1.6 m up, its moment about A1 = lever × force; the post's own
/// weight and wind at its middle (0.8 m); the clamp does not move.
#[test]
fn a_post_carries_the_wire_to_its_base_with_its_moment() {
    let line = on_a_post();
    line.validate().unwrap();
    let w = weather();
    let cases = vec![LoadCase::from_weather("wind")];
    let r = structure_loads(&line, &library(), &w, &cases, &default_budget()).unwrap();
    let at_t2: Vec<&AttachmentLoad> = r[0]
        .loads
        .iter()
        .filter(|l| l.structure == "T2" && !l.point.is_empty())
        .collect();
    assert!(at_t2.iter().all(|l| l.point == "A1"), "{at_t2:?}");
    let top = at_t2
        .iter()
        .find(|l| (l.lever[2] - 1.6).abs() < 1e-6)
        .expect("the wire's load at the post's top");
    assert!(top.lever[0].abs() < 1e-6 && top.lever[1].abs() < 1e-6);
    assert!(top.label.contains("post"), "{}", top.label);
    // The wind across the line on the wire: a transverse load.
    assert!(top.local[0].abs() > 500.0, "{top:?}");
    let own = at_t2
        .iter()
        .find(|l| (l.lever[2] - 0.8).abs() < 1e-6)
        .expect("the post's own load at its middle");
    assert!((own.local[2] - 450.0).abs() < 1e-6, "{own:?}");
    assert!((own.local[0].abs() - 500.0 * 0.3).abs() < 1e-6, "{own:?}");
    // The moment about the base: the lever (up) × the transverse force.
    let m = crate::capacity::lever_moment(top);
    assert!((m[1] - 1.6 * top.factored[0]).abs() < 1e-6, "{m:?}");
    // The CSV gives it at the end of the post's row.
    let csv = to_csv(&r);
    let row = csv
        .lines()
        .find(|l| l.contains(&top.label))
        .expect("the post's row");
    let last: Vec<f64> = row
        .rsplit(',')
        .take(3)
        .map(|x| x.parse().unwrap())
        .collect();
    assert!((last[1] - m[1]).abs() < 0.1, "{row}");
    // The section's clamp at T2 stays at the post's top.
    let solved = ohl_line::solve::solve_line(&line, &library(), &w[0]);
    let s = solved[0].as_ref().unwrap();
    assert!((s.clamps[1][2] - 30.0).abs() < 1e-9, "{:?}", s.clamps[1]);
}

/// A post holds its wire point alone and is no strain string; its ratings
/// and exit angles are a post's only.
#[test]
fn a_post_is_checked() {
    let mut line = on_a_post();
    line.structures[1].strings[0].strain = true;
    assert!(line.validate().is_err());
    let mut line = on_a_post();
    line.structures[1].strings[0].post = false;
    assert!(line.validate().is_err(), "a cantilever load on a string");
    let mut line = on_a_post();
    line.structures[1].strings[0].exit_angle_deg = Some(120.0);
    assert!(line.validate().is_err());
    // Written as version 17 (a line without a post: older).
    assert!(on_a_post().to_json().contains("\"version\": 17"));
}

/// `on_a_post`'s T2 given by a structure type holding the same post
/// (structure file version 13, 2026-11-15): its point A1 28.4 m up, the
/// upright post P1 1.6 m tall on it.
const POST_TYPE: &str = "format = \"ohl-structure\"\nversion = 13\nid = \"PT\"\nname = \"post\"\n\
    kind = \"pole\"\n\
    [[point]]\nlabel = \"A1\"\npoint = [\"0 m\", \"0 m\", \"28.4 m\"]\n\
    [[wire]]\nlabel = \"W1\"\nset = 1\n\
    [[string]]\nlabel = \"P1\"\ntop = \"A1\"\nwire = \"W1\"\nlength = \"1.6 m\"\n\
    angle = \"180 deg\"\nweight = \"450 N\"\nwind_area = \"0.3 m²\"\npost = true\n\
    cantilever_load = \"8 kN\"\n";

/// A typed structure's post loads its structure as the same post made by
/// hand: every load at T2 — the wires' at the post's top with its lever,
/// the post's own weight and wind at its middle — and the post's check
/// values are the same.
#[test]
fn a_typed_post_loads_as_one_made_by_hand() {
    let by_hand = on_a_post();
    let mut types = ohl_structures::StructureLibrary::default();
    let path = Path::new("PT.ohls");
    types
        .insert(
            path,
            <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(path, POST_TYPE)
                .unwrap(),
        )
        .unwrap();
    let mut typed = on_a_post();
    let t2 = &mut typed.structures[1];
    t2.points.clear();
    t2.strings.clear();
    t2.wires.clear();
    t2.structure_type = Some("PT".into());
    let typed = typed
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    let p1 = typed.structures[1].string("P1").unwrap();
    assert!(p1.post && p1.cantilever_load == Some(8000.0));
    let w = weather();
    let cases = vec![LoadCase::from_weather("wind")];
    let at_t2 = |line: &Line| -> Vec<AttachmentLoad> {
        let r = structure_loads(line, &library(), &w, &cases, &default_budget()).unwrap();
        r[0].loads
            .iter()
            .filter(|l| l.structure == "T2")
            .cloned()
            .collect()
    };
    let (a, b) = (at_t2(&by_hand), at_t2(&typed));
    assert_eq!(a.len(), b.len());
    assert!(a.len() >= 2, "{a:?}");
    let near =
        |x: [f64; 3], y: [f64; 3]| (0..3).all(|k| (x[k] - y[k]).abs() < 1e-6 * (1.0 + x[k].abs()));
    for (x, y) in a.iter().zip(&b) {
        assert_eq!((&x.point, &x.label), (&y.point, &y.label));
        assert!(near(x.local, y.local), "{x:?}\n{y:?}");
        assert!(
            near(x.lever, y.lever) && near(x.couple, y.couple),
            "{x:?}\n{y:?}"
        );
        assert!(near(
            x.moments_by_component()[0],
            y.moments_by_component()[0]
        ));
    }
    // The post's check values.
    let posts = |line: &Line| {
        let s = ohl_line::solve::solve_line(line, &library(), &w[0]);
        let shapes: Vec<_> = s.into_iter().map(Result::unwrap).collect();
        ohl_line::post::post_loads(line, &library(), &w[0], &shapes)
    };
    let (pa, pb) = (posts(&by_hand), posts(&typed));
    assert_eq!(pa.len(), 1);
    assert_eq!(pb.len(), 1);
    assert!((pa[0].cantilever() - pb[0].cantilever()).abs() < 1e-6 * pa[0].cantilever());
    assert!((pa[0].compression() - pb[0].compression()).abs() < 1e-6 * (1.0 + pa[0].compression()));
}

/// `on_a_post` with a jumper from the post's top (T2 W1) to a wire point
/// fixed to T2 3 m to the side and 1.5 m lower (W2, no section ends
/// there), stiff (400 N·m²; else none: hinged), held level at the post's
/// clamp.
fn with_a_jumper(stiff: bool) -> Line {
    let t2 = r#""wires": [{"label": "W1", "position": [300, 0, 30], "set": 1}]}"#;
    assert!(POST_LINE.contains(t2));
    let json = POST_LINE
        .replace(
            t2,
            r#""wires": [{"label": "W1", "position": [300, 0, 30], "set": 1},
                         {"label": "W2", "position": [300, 3, 28.5], "set": 2}]}"#,
        )
        .replace(
            r#""sections":"#,
            &format!(
                r#""jumpers": [{{"id": "J", "cable": "acsr-drake-26-7",
                    "from": {{"structure": "T2", "set": 1}}, "to": {{"structure": "T2", "set": 2}},
                    "sag": 0.5, "temperature": 15.0{}}}],
                  "sections":"#,
                if stiff {
                    r#", "bending_stiffness": 400.0"#
                } else {
                    r#", "bending_stiffness": 0.0"#
                }
            ),
        );
    Line::from_json(&json).unwrap()
}

/// The vertical load (down) at T2's point `point` with a lever of `lever`
/// m up.
fn down_at(r: &CaseLoads, point: &str, lever: f64) -> f64 {
    r.loads
        .iter()
        .filter(|l| l.structure == "T2" && l.point == point && (l.lever[2] - lever).abs() < 1e-6)
        .map(|l| l.local[2])
        .sum()
}

/// A jumper ending at a post's top: its end load reaches the post's base
/// with the post's lever (before 2026-11-14 a clamp fixed to the
/// structure dropped it), and its other end — a wire point no section
/// ends at — is loaded too; stiff, held level in the post's clamp, it puts
/// a moment on the clamp, added to the base's.
#[test]
fn a_jumper_on_a_post_reaches_its_base() {
    let w = weather();
    let cases = vec![LoadCase::from_weather("wind")];
    let lib = library();
    let plain = structure_loads(&on_a_post(), &lib, &w, &cases, &default_budget()).unwrap();
    let line = with_a_jumper(true);
    line.validate().unwrap();
    let r = structure_loads(&line, &lib, &w, &cases, &default_budget()).unwrap();
    // About half its weight (≈ 3.6 m × 16 N/m) at each end: estimated,
    // as the sections take a jumper's load on their clamps.
    let more = down_at(&r[0], "A1", 1.6) - down_at(&plain[0], "A1", 1.6);
    let far = down_at(&r[0], "W2", 0.0);
    for x in [more, far] {
        assert!(x > 15.0 && x < 45.0, "{more} {far}");
    }
    // The jumper's clamp moment, on its own and in the post's loads.
    let solved = ohl_line::solve::solve_line(&line, &lib, &w[0]);
    let shapes: Vec<_> = solved.into_iter().map(Result::unwrap).collect();
    let j =
        ohl_line::jumper::solve_one_jumper(&line, &lib, &w[0], &shapes, &line.jumpers[0]).unwrap();
    let m = j.end_moments[0];
    let size = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
    assert!(size > 1.0, "{m:?}");
    assert_eq!(j.end_moments[1], [0.0; 3], "a hinged end");
    let posts = ohl_line::post::post_loads(&line, &lib, &w[0], &shapes);
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].clamp_moment, m);
    assert!(posts[0].cantilever() > 0.0 && posts[0].compression() > 0.0);
    // In the structure loads: a moment of its own at A1, with the post's
    // lever.
    let couple: Vec<&AttachmentLoad> = r[0]
        .loads
        .iter()
        .filter(|l| l.structure == "T2" && l.couple != [0.0; 3])
        .collect();
    assert_eq!(couple.len(), 1, "{couple:?}");
    assert_eq!(couple[0].point, "A1");
    let c = couple[0].couple;
    assert!(((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() - size).abs() < 1e-6 * size);
    // Without a bending stiffness: no moment, about half its weight at
    // each end.
    let line = with_a_jumper(false);
    let r = structure_loads(&line, &lib, &w, &cases, &default_budget()).unwrap();
    assert!(r[0].loads.iter().all(|l| l.couple == [0.0; 3]));
    let more = down_at(&r[0], "A1", 1.6) - down_at(&plain[0], "A1", 1.6);
    let far = down_at(&r[0], "W2", 0.0);
    for x in [more, far] {
        assert!(x > 15.0 && x < 45.0, "{more} {far}");
    }
}

/// A braced line post: a hinged post standing level 1.5 m out from A1,
/// a brace from A2 1.5 m above A1 to its top. Under the conductor's weight
/// the post pushes and the brace pulls; alone, a hinged post is refused.
#[test]
fn a_braced_post_pushes_and_its_brace_pulls() {
    let mut line = on_a_post();
    let t2 = &mut line.structures[1];
    t2.wires[0].position = [300.0, -1.5, 28.4];
    let a2 = {
        let mut a = t2.points[0].clone();
        a.label = "A2".into();
        a.position = [300.0, 0.0, 29.9];
        a
    };
    t2.points.push(a2);
    let post = &mut t2.strings[0];
    post.hinged = true;
    post.angle_deg = 90.0;
    post.length = 1.5;
    post.cantilever_load = None;
    let alone = line.clone();
    assert!(alone.validate().is_err(), "a hinged post needs a brace");
    let brace = ohl_line::InsulatorString {
        label: "B1".into(),
        top: "A2".into(),
        wire: "W1".into(),
        length: (1.5f64 * 1.5 * 2.0).sqrt(),
        weight: 200.0,
        ..Default::default()
    };
    line.structures[1].strings.push(brace);
    line.validate().unwrap();
    let still =
        ohl_weather::WeatherCases::parse("[[case]]\nname = \"still\"\ntemperature = \"10 °C\"\n")
            .unwrap()
            .cases;
    let s = ohl_line::solve::solve_line(&line, &library(), &still[0]);
    let s = s[0].as_ref().unwrap();
    let force = |label: &str| {
        s.strings[1]
            .iter()
            .find(|x| x.label == label)
            .unwrap_or_else(|| panic!("{label}: {:?}", s.strings[1]))
            .force
    };
    assert!(force("P1") < -100.0, "the post pushes: {}", force("P1"));
    assert!(force("B1") > 100.0, "the brace pulls: {}", force("B1"));
    // Statics: the brace at 45° carries the weight on the clamp, the post
    // its horizontal part.
    assert!((force("P1") + force("B1") / 2f64.sqrt()).abs() < 0.05 * force("B1"));
}
