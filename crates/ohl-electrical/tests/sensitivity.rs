// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The field settings' sensitivity on the example double circuit
//! (`examples/electrical`, two spans), for the theory pages (owner,
//! 2026-11-01: "give indications on the cutoffs and the improvement with
//! better grids"): run by hand, release build —
//! `cargo test --release -p ohl-electrical --test sensitivity -- --ignored --nocapture`.

use std::path::Path;
use std::time::Instant;

use ohl_core::units::Temperature;
use ohl_electrical::Sources;
use ohl_electrical::electric::{self, GroundModel};
use ohl_electrical::field_views::Path as FPath;
use ohl_electrical::magnetic::{self, CircuitCurrent, CutAt, CutSpec, StationSpec, VolumeSpec};
use ohl_line::Line;

fn example() -> (Line, ohl_cables::Library) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/electrical");
    let line =
        Line::from_json(&std::fs::read_to_string(dir.join("double-circuit.ohl.json")).unwrap())
            .unwrap();
    let library = ohl_cables::Library::load(&dir.join("wires.ohlw")).unwrap();
    (line, library)
}

fn shapes(line: &Line, library: &ohl_cables::Library) -> Vec<ohl_line::solve::SectionShape> {
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    ohl_line::solve::solve_line(line, library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect()
}

fn st(from: &str, to: &str, fraction: f64) -> StationSpec {
    StationSpec {
        from: from.into(),
        to: to.into(),
        fraction: Some(fraction),
    }
}

#[test]
#[ignore]
fn sensitivity() {
    let (line, library) = example();
    let shapes = shapes(&line, &library);
    let src = Sources {
        line: &line,
        library: &library,
        shapes: &shapes,
        temperature: Temperature::from_celsius(40.0),
        ground: None,
        jumpers: &[],
    };
    // ---- Electric: stretches per span (3D, plane under each point).
    println!("ELECTRIC stretches: largest E of each profile against 64 stretches");
    let profiles = vec![
        st("T1", "T2", 0.5),
        st("T1", "T2", 0.05),
        st("T1", "T2", 0.25),
    ];
    let run = |n: usize| {
        let s = electric::Settings {
            stations: profiles.clone(),
            ground_model: GroundModel::Plane,
            stretches: n,
            ..electric::Settings::default()
        };
        let t = Instant::now();
        let f = electric::run(&src, &s, None).unwrap();
        (f, t.elapsed().as_secs_f64())
    };
    let (reference, _) = run(64);
    for n in [1, 4, 8, 16, 24, 32, 64] {
        let (f, t) = run(n);
        let d: Vec<String> = f
            .profiles
            .iter()
            .zip(&reference.profiles)
            .map(|(a, b)| format!("{:+.3} %", 100.0 * (a.max_kv_m - b.max_kv_m) / b.max_kv_m))
            .collect();
        println!(
            "  {n:>3} stretches: mid {} / near T1 {} / quarter {} — {t:.3} s",
            d[0], d[1], d[2]
        );
    }
    // ---- Electric: the volume floor's tiles, against the exact field
    // (level ground: the plane is exact) over the first span.
    println!("ELECTRIC volume tiles: largest difference over the largest value, from 1 m up");
    let (_, charges) = electric::run_keeping(
        &src,
        &electric::Settings {
            stations: vec![st("T1", "T2", 0.5)],
            ground_model: GroundModel::Plane,
            ..electric::Settings::default()
        },
        None,
    )
    .unwrap();
    let charges = charges.unwrap();
    let ground = electric::feet_ground(&line);
    let at = |id: &str| {
        let p = line.structure(id).unwrap().position;
        [p[0], p[1], p[2]]
    };
    let path = FPath::new(vec![at("T1"), at("T2")]).unwrap();
    let len = path.length();
    // Over the whole span (100 m beyond), and over one part of 60 m at
    // its middle (50 m beyond: a long volume's part) — 2 m tiles over the
    // whole span would take 3.3 GB.
    for (tile, margin, part) in [
        (16.0, 100.0, false),
        (8.0, 100.0, false),
        (4.0, 100.0, false),
        (16.0, 50.0, true),
        (8.0, 50.0, true),
        (4.0, 50.0, true),
        (2.0, 50.0, true),
    ] {
        let (t0, t1) = if part {
            (0.5 * len - 30.0, 0.5 * len + 30.0)
        } else {
            (0.0, len)
        };
        let t = Instant::now();
        let floor = electric::Floor::around(
            &charges,
            &path.plan_between(t0, t1),
            20.0 + margin,
            tile,
            &ground,
        )
        .unwrap();
        let took = t.elapsed().as_secs_f64();
        let (mut worst, mut top): (f64, f64) = (0.0, 0.0);
        let mut s = t0 + 5.0;
        while s < t1 - 5.0 {
            for k in -2..=2 {
                for h in [1.0, 3.0, 8.0] {
                    let xy = path.place(s, 10.0 * k as f64);
                    let p = [xy[0], xy[1], ground(xy[0], xy[1]) + h];
                    let exact =
                        charges.field(p, &electric::GroundPlane::fitted(&ground, p[0], p[1], 10.0));
                    top = top.max(exact);
                    worst = worst.max((floor.field(p, &charges) - exact).abs());
                }
            }
            s += 5.0;
        }
        println!(
            "  {tile:>4} m tiles, {margin:>3} m beyond{}: {:>5} tiles, {:.2} % — {took:.2} s",
            if part {
                ", a part of 60 m"
            } else {
                ", the span"
            },
            floor.tiles(),
            100.0 * worst / top
        );
    }
    // ---- Electric: the profile's floor (its fine tiles) and a cut's
    // floor, against the plane (exact on this level ground).
    println!("ELECTRIC profile floor: fine tiles, the profile's points against the exact field");
    let exact = electric::run(
        &src,
        &electric::Settings {
            stations: vec![st("T1", "T2", 0.5)],
            ground_model: GroundModel::Plane,
            ..electric::Settings::default()
        },
        None,
    )
    .unwrap();
    let top = exact.profiles[0].max_kv_m;
    // (1 m: 11 572 tiles, over the 2 GB a floor may take.)
    for fine in [4.0, 2.0] {
        let t = Instant::now();
        let f = electric::run(
            &src,
            &electric::Settings {
                stations: vec![st("T1", "T2", 0.5)],
                ground_model: GroundModel::Tiled(electric::TiledFloor {
                    fine,
                    ..Default::default()
                }),
                ..electric::Settings::default()
            },
            None,
        )
        .unwrap();
        let took = t.elapsed().as_secs_f64();
        let worst = f.profiles[0]
            .points
            .iter()
            .zip(&exact.profiles[0].points)
            .map(|(a, b)| (a.e_kv_m - b.e_kv_m).abs())
            .fold(0.0, f64::max);
        println!(
            "  {fine} m fine tiles: {:.2} % of the largest — {took:.2} s",
            100.0 * worst / top
        );
    }
    println!("ELECTRIC cut floor: tiles, the cut's points against the exact field");
    let cut = |tile: Option<f64>| {
        let s = electric::Settings {
            profiles: false,
            cuts: vec![CutSpec {
                at: CutAt::InSpan {
                    from: "T1".into(),
                    to: "T2".into(),
                    fraction: None,
                },
                width: 60.0,
                above: 10.0,
                step: 1.0,
                label: None,
            }],
            ground_model: match tile {
                None => GroundModel::Plane,
                Some(_) => GroundModel::Tiled(electric::TiledFloor::default()),
            },
            cut_tile: tile.unwrap_or(8.0),
            ..electric::Settings::default()
        };
        let t = Instant::now();
        let f = electric::run(&src, &s, None).unwrap();
        (f.cuts[0].values.values.clone(), t.elapsed().as_secs_f64())
    };
    let (exact_cut, _) = cut(None);
    let top = exact_cut
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max);
    for tile in [8.0, 4.0, 2.0] {
        let (v, took) = cut(Some(tile));
        // Above the ground only (1 m up): the points the drawing shows.
        let worst = v
            .iter()
            .zip(&exact_cut)
            .filter(|(a, b)| a.is_finite() && b.is_finite())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        println!(
            "  {tile} m tiles: {:.2} % of the largest — {took:.2} s",
            100.0 * worst / top
        );
    }
    // ---- Magnetic: a cut's step and its isolines.
    println!("MAGNETIC cut step: isolines off their level (nine in ten / largest), time per cut");
    let currents = vec![
        CircuitCurrent {
            circuit: "North".into(),
            amps: 800.0,
            reverse: false,
        },
        CircuitCurrent {
            circuit: "South".into(),
            amps: 800.0,
            reverse: false,
        },
    ];
    let levels = [1.0, 3.0, 10.0, 30.0];
    let base = magnetic::Settings {
        currents: currents.clone(),
        profiles: false,
        levels: levels.to_vec(),
        ..magnetic::Settings::default()
    };
    let mut wires = None;
    for step in [0.25, 0.5, 1.0, 2.0] {
        let s = magnetic::Settings {
            cuts: vec![CutSpec {
                at: CutAt::InSpan {
                    from: "T1".into(),
                    to: "T2".into(),
                    fraction: None,
                },
                width: 60.0,
                above: 10.0,
                step,
                label: None,
            }],
            ..base.clone()
        };
        let t = Instant::now();
        let (f, w) = magnetic::run_keeping(&src, &s, None).unwrap();
        let took = t.elapsed().as_secs_f64();
        let w = wires.get_or_insert(w.unwrap());
        let field =
            |ps: &[[f64; 3]]| -> Vec<f64> { ps.iter().map(|&p| w.field(p).0 * 1e6).collect() };
        let c = &f.cuts[0].values;
        let devs: Vec<String> = levels
            .iter()
            .map(|&l| match c.deviation(l, &field) {
                Some(d) => format!("{l} µT {:.2}/{:.2} %", 100.0 * d.p90, 100.0 * d.max),
                None => format!("{l} µT —"),
            })
            .collect();
        println!(
            "  {step:>5} m: {} points; {} — {took:.3} s",
            c.cut.points(),
            devs.join(", ")
        );
    }
    // ---- Magnetic: the wires modelled within a distance (the cut-off).
    println!("MAGNETIC modelled within: a cut's change against the whole line, time");
    for within in [Some(25.0), Some(50.0), Some(100.0), Some(200.0), None] {
        let s = magnetic::Settings {
            cuts: vec![CutSpec {
                at: CutAt::InSpan {
                    from: "T1".into(),
                    to: "T2".into(),
                    fraction: None,
                },
                width: 60.0,
                above: 10.0,
                step: 0.5,
                label: None,
            }],
            modelled_within: within,
            ..base.clone()
        };
        let t = Instant::now();
        let f = magnetic::run(&src, &s, None).unwrap();
        let took = t.elapsed().as_secs_f64();
        let d = f.cuts[0].left_out.map_or("whole line".to_owned(), |d| {
            format!(
                "nine in ten {:.3} %, largest {:.3} %",
                100.0 * d.p90,
                100.0 * d.max
            )
        });
        println!(
            "  within {within:?} m: {} pieces; {d} — {took:.3} s",
            f.cuts[0].pieces
        );
    }
    // ---- Magnetic: a volume's step.
    println!(
        "MAGNETIC volume step: surfaces off their level (nine in ten / largest), points, time"
    );
    for step in [4.0, 2.0, 1.0] {
        let s = magnetic::Settings {
            volumes: vec![VolumeSpec {
                from: "T1".into(),
                to: "T2".into(),
                width: 40.0,
                above: 10.0,
                step,
            }],
            ..base.clone()
        };
        let t = Instant::now();
        let f = magnetic::run(&src, &s, None).unwrap();
        let took = t.elapsed().as_secs_f64();
        let v = &f.volumes[0].values;
        let devs: Vec<String> = v
            .surfaces
            .iter()
            .zip(&v.deviation)
            .map(|((l, _), d)| match d {
                Some(d) => format!("{l} µT {:.2}/{:.2} %", 100.0 * d.p90, 100.0 * d.max),
                None => format!("{l} µT —"),
            })
            .collect();
        println!(
            "  {step} m: {:.1} k of {:.1} k points computed; {} — {took:.2} s",
            v.computed as f64 / 1e3,
            v.full as f64 / 1e3,
            devs.join(", ")
        );
    }
}
