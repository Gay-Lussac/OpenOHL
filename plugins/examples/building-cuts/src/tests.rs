// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Native tests on invented data: a straight line of three structures
//! (T1 at x = 0, T2 at 300 m, T3 at 600 m, along x; the right side is
//! y < 0), six phase conductors and an earth wire sagging 8 m, level
//! ground at 400 m, and three invented buildings: a 12 × 8 × 7 m box
//! 25 m right of T1–T2 at mid-span, an L-shape (20 × 20 m less a
//! 10 × 10 m corner, 300 m²) 40 m left of T2–T3, a 12 × 10 m building
//! across T2, and isolated noise points.

use crate::analysis::{InPoint, analyse, presets};
use crate::geom::{convex_hull, signed_area};
use crate::model::Model;
use crate::params::{OutlineKind, Params, TEMPLATE, matches};
use crate::report::report;

/// The invented model (`ohl-export-model`).
pub fn model_json() -> String {
    let structures: Vec<serde_json::Value> = [("T1", 0.0), ("T2", 300.0), ("T3", 600.0)]
        .iter()
        .map(|(id, x)| serde_json::json!({"id": id, "position": [x, 0.0, 400.0]}))
        .collect();
    let phases = [
        ("NL1", 5.0, 422.0, Some("sysA")),
        ("NL2", 5.0, 427.0, Some("sysA")),
        ("NL3", 5.0, 432.0, Some("sysA")),
        ("SL1", -5.0, 422.0, Some("sysB")),
        ("SL2", -5.0, 427.0, Some("sysB")),
        ("SL3", -5.0, 432.0, Some("sysB")),
        ("EW", 0.0, 438.0, None),
    ];
    let sections: Vec<serde_json::Value> = phases
        .iter()
        .map(|(id, ..)| {
            serde_json::json!({"id": id, "points": [
                {"structure": "T1"}, {"structure": "T2"}, {"structure": "T3"}]})
        })
        .collect();
    let mut wires = Vec::new();
    for (id, y, z, system) in phases {
        for (k, x0) in [0.0, 300.0].iter().enumerate() {
            let points: Vec<[f64; 3]> = (0..=60)
                .map(|i| {
                    let t = i as f64 / 60.0;
                    [x0 + 300.0 * t, y, z - 32.0 * t * (1.0 - t)]
                })
                .collect();
            wires.push(serde_json::json!({
                "section": id, "span": k, "system": system, "points": points}));
        }
    }
    serde_json::json!({
        "format": "ohl-export-model", "version": 1,
        "line": {"structures": structures, "sections": sections},
        "cases": [{"name": "test", "wires": wires}],
    })
    .to_string()
}

fn lidar(x: f64, y: f64, z: f64) -> InPoint {
    InPoint {
        x,
        y,
        z,
        class: 6,
        code: String::new(),
        line: String::new(),
    }
}

/// Points on a grid `step` apart over `x0`–`x1`, `y0`–`y1` (edges
/// included) where `keep` holds.
fn grid(
    (x0, x1): (f64, f64),
    (y0, y1): (f64, f64),
    step: f64,
    z: f64,
    keep: impl Fn(f64, f64) -> bool,
) -> Vec<InPoint> {
    let nx = ((x1 - x0) / step).round() as usize;
    let ny = ((y1 - y0) / step).round() as usize;
    let mut out = Vec::new();
    for i in 0..=nx {
        for j in 0..=ny {
            let (x, y) = (x0 + i as f64 * step, y0 + j as f64 * step);
            if keep(x, y) {
                out.push(lidar(x, y, z));
            }
        }
    }
    out
}

fn in_l(x: f64, y: f64) -> bool {
    !(x > 440.0 && y > 50.0)
}

/// The three buildings and 15 noise points; `centres`: the L-shape's
/// points at cell centres (its cells exactly the L).
fn buildings(centres: bool) -> Vec<InPoint> {
    let mut p = grid((144.0, 156.0), (-33.0, -25.0), 0.5, 407.0, |_, _| true);
    if centres {
        p.extend(grid((430.25, 449.75), (40.25, 59.75), 0.5, 409.0, in_l));
    } else {
        p.extend(grid((430.0, 450.0), (40.0, 60.0), 0.5, 409.0, in_l));
    }
    p.extend(grid((294.0, 306.0), (-40.0, -30.0), 0.5, 406.0, |_, _| {
        true
    }));
    for k in 0..15 {
        let x = 20.0 + 37.0 * k as f64;
        let y = if k % 2 == 0 { 70.0 } else { -70.0 };
        p.push(lidar(x, y, 401.0 + k as f64 * 0.1));
    }
    p
}

/// Level ground at 400 m, a point every 2 m.
fn ground() -> Vec<f64> {
    let mut g = Vec::new();
    for i in 0..=320 {
        for j in 0..=80 {
            g.extend([i as f64 * 2.0 - 20.0, j as f64 * 2.0 - 80.0, 400.0]);
        }
    }
    g
}

fn model() -> Model {
    Model::parse(&model_json()).unwrap()
}

#[test]
fn template_is_the_defaults() {
    assert_eq!(Params::parse(TEMPLATE).unwrap(), Params::default());
    assert_eq!(Params::parse("").unwrap(), Params::default());
    let example = Params::parse(include_str!("../example-parameters.toml")).unwrap();
    assert_eq!(example.outline, OutlineKind::Cells);
    let p = Params::parse("outline = \"cells\"\nlimit_ut = 1.0\n").unwrap();
    assert_eq!(p.outline, OutlineKind::Cells);
    for bad in [
        "colour = 1",
        "cell = 0",
        "ground_ring = [5.0, 2.0]",
        "outline = \"x\"",
        "step = -1",
    ] {
        assert!(Params::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn codes_match() {
    assert!(matches("BLD*", "bld-roof"));
    assert!(matches("*", ""));
    assert!(matches("G?D", "GND"));
    assert!(!matches("BLD*", "GND"));
    assert!(!matches("B?D", "BLLD"));
}

#[test]
fn the_model_is_read() {
    let m = model();
    let names: Vec<(&str, &str)> = m
        .spans
        .iter()
        .map(|s| (s.from.as_str(), s.to.as_str()))
        .collect();
    assert_eq!(names, [("T1", "T2"), ("T2", "T3")]);
    assert_eq!(m.wires.len(), 14);
    assert_eq!(m.wires.iter().filter(|w| !w.phase).count(), 2);
    assert_eq!(m.spans[1].start, 300.0);
    assert_eq!(m.spans[0].right, [0.0, -1.0]);
}

/// Three buildings found, the noise left out, ids in station order.
#[test]
fn three_buildings_noise_dropped() {
    let a = analyse(&Params::default(), &model(), &buildings(false), &ground()).unwrap();
    assert_eq!(a.buildings.len(), 3, "{:?}", a.notes);
    let ids: Vec<&str> = a.buildings.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, ["B001", "B002", "B003"]);
    let xs: Vec<f64> = a.buildings.iter().map(|b| b.centroid[0]).collect();
    assert!(
        xs[0] < 160.0 && (xs[1] - 300.0).abs() < 1.0 && xs[2] > 400.0,
        "{xs:?}"
    );
    assert!(
        a.notes
            .iter()
            .any(|n| n.contains("15 clusters left out") && n.contains("15 points")),
        "{:?}",
        a.notes
    );
    let b = &a.buildings[0];
    assert!((b.ground - 400.0).abs() < 1e-9 && (b.eaves - 407.0).abs() < 1e-9);
    assert_eq!(b.floors, 3);
    assert!((b.axis_distance - 25.0).abs() < 1e-6, "{}", b.axis_distance);
    // To SL1 at y = -5: 20 m in plan, the wire at 414 m at mid-span, 7 m
    // over the roof.
    assert!(
        (b.conductor_distance - 20.0f64.hypot(7.0)).abs() < 1e-6,
        "{}",
        b.conductor_distance
    );
}

/// The box's hull is 96 m² within 2 %; the L's hull (its points at the
/// cells' centres) 330.25 m², its cells' outline 300 m².
#[test]
fn footprints() {
    let a = analyse(&Params::default(), &model(), &buildings(true), &ground()).unwrap();
    assert!(
        (a.buildings[0].area - 96.0).abs() / 96.0 < 0.02,
        "{}",
        a.buildings[0].area
    );
    let l = &a.buildings[2];
    assert!((l.area - 330.25).abs() < 1e-6, "{}", l.area);
    let cells = Params {
        outline: OutlineKind::Cells,
        ..Params::default()
    };
    let a = analyse(&cells, &model(), &buildings(true), &ground()).unwrap();
    let l = &a.buildings[2];
    assert!((l.area - 300.0).abs() / 300.0 < 0.02, "{}", l.area);
    assert_eq!(l.ring.len(), 6, "{:?}", l.ring);
    // The box's points on its edges reach into the next cells: up to a
    // cell more each side.
    let area = a.buildings[0].area;
    assert!((96.0..=107.0).contains(&area), "{area}");
}

/// The building across T2: cuts in both spans, said.
#[test]
fn straddling_building() {
    let a = analyse(&Params::default(), &model(), &buildings(false), &ground()).unwrap();
    let b = &a.buildings[1];
    assert_eq!(b.spans.len(), 2);
    let labels: Vec<&str> = a.cuts.iter().map(|c| c.label.as_str()).collect();
    assert!(
        labels.iter().any(|l| l.contains("B002-T1T2-gov"))
            && labels.iter().any(|l| l.contains("B002-T2T3-gov")),
        "{labels:?}"
    );
    assert!(
        a.notes
            .iter()
            .any(|n| n.starts_with("B002: it lies beside 2 spans (T1–T2, T2–T3)")),
        "{:?}",
        a.notes
    );
    for c in a.cuts.iter().filter(|c| c.label.contains("B002")) {
        let near_t2 = if c.from == "T1" {
            c.fraction > 0.97
        } else {
            c.fraction < 0.03
        };
        assert!(near_t2, "{c:?}");
    }
}

/// The box's governing cut at mid-span, merged with its nearest-point
/// cut; wide enough for the box.
#[test]
fn governing_cut_of_the_box() {
    let a = analyse(&Params::default(), &model(), &buildings(false), &ground()).unwrap();
    let c = a
        .cuts
        .iter()
        .find(|c| c.label.starts_with("B001-gov"))
        .unwrap();
    assert_eq!(c.label, "B001-gov+B001-near");
    assert_eq!((c.from.as_str(), c.to.as_str()), ("T1", "T2"));
    assert!((c.fraction - 0.5).abs() < 0.02, "{}", c.fraction);
    assert!(c.width >= 43.0, "{}", c.width);
    let crossing = a
        .crossings
        .iter()
        .find(|n| n.starts_with("cut B001-gov+B001-near: "))
        .unwrap();
    assert!(crossing.contains("wire SL1 5.00 414.00"), "{crossing}");
    assert!(crossing.contains("earth"), "{crossing}");
    // A centroid cut on demand: merged as well (the box is 12 m long).
    let p = Params {
        centroid_cut: true,
        ..Params::default()
    };
    let a = analyse(&p, &model(), &buildings(false), &ground()).unwrap();
    assert!(
        a.cuts
            .iter()
            .any(|c| c.label == "B001-gov+B001-near+B001-ctr")
    );
}

/// The box's places: only on its side facing the axis (y = -25), three
/// floors, named by floor and height.
#[test]
fn facade_points_face_the_axis() {
    let a = analyse(&Params::default(), &model(), &buildings(false), &ground()).unwrap();
    let box_places: Vec<_> = a
        .places
        .iter()
        .filter(|p| p.name.starts_with("B001-"))
        .collect();
    assert!(!box_places.is_empty());
    for p in &box_places {
        assert!((p.y + 25.0).abs() < 1e-9, "{p:?}");
    }
    // Two corners and a midpoint (the midpoint is the nearest point too).
    assert_eq!(box_places.len(), 3 * 3, "{box_places:?}");
    assert!(box_places.iter().any(|p| p.name == "B001-F2-c1-h4.5"));
    assert!(box_places.iter().any(|p| p.name == "B001-F3-m1-h7.5"));
    // The L (left of the line): its places on its sides facing the axis
    // (y = 40 and nothing above y = 50 but its inner corner).
    for p in a.places.iter().filter(|p| p.name.starts_with("B003-")) {
        assert!(p.y <= 50.0 + 1e-9, "{p:?}");
    }
}

/// The result depends on the points, not on their order.
#[test]
fn shuffle_independent() {
    let points = buildings(false);
    let mut shuffled = points.clone();
    let mut seed: u64 = 12345;
    for i in (1..shuffled.len()).rev() {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (seed >> 33) as usize % (i + 1);
        shuffled.swap(i, j);
    }
    for outline in [OutlineKind::Hull, OutlineKind::Cells] {
        let p = Params {
            outline,
            ..Params::default()
        };
        let a = analyse(&p, &model(), &points, &ground()).unwrap();
        let b = analyse(&p, &model(), &shuffled, &ground()).unwrap();
        assert_eq!(a, b);
    }
}

/// A surveyed outline (breakline, corners at the ground): one building,
/// its height assumed and said.
#[test]
fn surveyed_outline() {
    let corners = [(100.0, 30.0), (110.0, 30.0), (110.0, 40.0), (100.0, 40.0)];
    let points: Vec<InPoint> = corners
        .iter()
        .map(|&(x, y)| InPoint {
            x,
            y,
            z: 400.0,
            class: 0,
            code: "BLD-corner".into(),
            line: "S1".into(),
        })
        .collect();
    let a = analyse(&Params::default(), &model(), &points, &ground()).unwrap();
    assert_eq!(a.buildings.len(), 1);
    let b = &a.buildings[0];
    assert!((b.area - 100.0).abs() < 1e-9);
    assert_eq!(b.floors, 2);
    assert!((b.roof - 406.0).abs() < 1e-9);
    assert!(
        a.notes.contains(&"B001: surveyed outline S1".to_owned()),
        "{:?}",
        a.notes
    );
    assert!(
        a.notes
            .iter()
            .any(|n| n.starts_with("B001: ") && n.contains("6 m high assumed")),
        "{:?}",
        a.notes
    );
}

#[test]
fn hull_of_a_square() {
    let h = convex_hull(&[
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.5, 0.5],
        [0.5, 0.0],
    ]);
    assert_eq!(h.len(), 4);
    assert!((signed_area(&h) - 1.0).abs() < 1e-12);
}

/// The report of invented results: the summary's rows, the places, a
/// cut's CSV and SVG, the GeoJSON; files named by the quantity.
#[test]
fn report_of_invented_results() {
    let p = Params {
        limit_ut: Some(1.0),
        ..Params::default()
    };
    let presets = presets(&p, &model(), &buildings(false), &ground()).unwrap();
    let preset = &presets[0];
    let cut = &preset.cuts[0];
    let offsets: Vec<f64> = (-40..=40).map(f64::from).collect();
    let elevations: Vec<f64> = (395..=440).map(f64::from).collect();
    let values: Vec<f64> = elevations
        .iter()
        .flat_map(|z| {
            offsets
                .iter()
                .map(move |s| 100.0 / (1.0 + s * s + (z - 420.0).powi(2)))
        })
        .collect();
    let points: Vec<serde_json::Value> = preset
        .points
        .iter()
        .map(|q| {
            serde_json::json!({"name": q.name, "x": q.x, "y": q.y, "z": 400.0 + q.height,
            "offset": 0.0, "height": q.height, "value": 0.5, "major": 0.6, "margin": null})
        })
        .collect();
    let results = serde_json::json!({
        "format": "ohl-field-results", "version": 1, "quantity": "B", "unit": "µT",
        "cuts": [{"label": cut.label, "from": "T1", "to": "T2", "at": cut.fraction * 300.0,
            "plane": {"origin": [cut.fraction * 300.0, 0.0], "right": [0.0, -1.0]},
            "offsets": offsets, "elevations": elevations,
            "ground": vec![400.0; 81], "values": values,
            "isolines": [{"level": 1.0, "lines": [[[-5.0, 410.0], [5.0, 410.0]]]}]}],
        "points": points, "notes": ["invented"],
    })
    .to_string();
    let files = report(&p, preset, &results).unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    for want in [
        "buildings-B.csv",
        "points-B.csv",
        "buildings.geojson",
        "notes-B.txt",
    ] {
        assert!(names.contains(&want), "{names:?}");
    }
    let base = format!("cut-{}-B", cut.label);
    assert!(names.contains(&format!("{base}.svg").as_str()), "{names:?}");
    let text = |n: &str| {
        String::from_utf8(files.iter().find(|f| f.name == n).unwrap().bytes.clone()).unwrap()
    };
    let summary = text("buildings-B.csv");
    let rows: Vec<&str> = summary.lines().collect();
    assert_eq!(rows.len(), 4, "{summary}");
    assert!(
        rows[0].ends_with("max_places_B_major_uT,limit_uT,margin_uT"),
        "{}",
        rows[0]
    );
    let b1: Vec<&str> = rows[1].split(',').collect();
    assert_eq!(b1[0], "B001");
    assert_eq!(b1[2], "96.0");
    assert_eq!(b1[9], "T1–T2");
    assert_eq!(b1[14], "1");
    // In the box's section (offsets 25–33, 400–407 m), the invented
    // field's largest value is at offset 24.5 → 25 (half a step widened).
    let sec: f64 = b1[15].parse().unwrap();
    let want = 100.0 / (1.0 + 25.0f64.powi(2) + 13.0f64.powi(2));
    assert!((sec - want).abs() < 1e-3, "{sec} {want}");
    assert_eq!(b1[17], "0.600");
    assert_eq!(b1[18], "1");
    assert_eq!(b1[19], "0.400");
    let svg = text(&format!("{base}.svg"));
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(svg.contains("B001 (largest"), "{svg}");
    assert!(svg.contains(">SL1<"), "{svg}");
    let geo: serde_json::Value = serde_json::from_str(&text("buildings.geojson")).unwrap();
    assert_eq!(
        geo["features"].as_array().unwrap().len(),
        3 + 1 + preset.points.len()
    );
    let grid = text(&format!("{base}.csv"));
    assert_eq!(grid.lines().count(), 2 + 81 * 46);
    // The electric field's files carry its letter.
    let e = results.replace("\"quantity\":\"B\"", "\"quantity\":\"E\"");
    let files = report(&p, preset, &e).unwrap();
    assert!(files.iter().any(|f| f.name == "buildings-E.csv"));
}

/// The time for a large input (run by hand: `cargo test --release
/// timing -- --ignored --nocapture`): 400 buildings of 2 500 points
/// (1 000 000) and 128 000 ground points.
#[test]
#[ignore]
fn timing_large() {
    let mut pts = Vec::new();
    for b in 0..400 {
        let x0 = 5.0 + (b / 4) as f64 * 6.0;
        let y0 = [-60.0, -35.0, 25.0, 50.0][b % 4];
        for i in 0..50 {
            for j in 0..50 {
                pts.push(lidar(
                    x0 + i as f64 * 0.08,
                    y0 + j as f64 * 0.2,
                    405.0 + (i % 7) as f64,
                ));
            }
        }
    }
    let mut g = Vec::new();
    for i in 0..640 {
        for j in 0..200 {
            g.extend([i as f64 - 20.0, j as f64 - 100.0, 400.0]);
        }
    }
    let t = std::time::Instant::now();
    let a = analyse(&Params::default(), &model(), &pts, &g).unwrap();
    eprintln!(
        "{} buildings, {} cuts, {} places in {:?}",
        a.buildings.len(),
        a.cuts.len(),
        a.places.len(),
        t.elapsed()
    );
}
