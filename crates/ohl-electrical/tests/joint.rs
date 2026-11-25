// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joint system (`ohl_electrical::joint`) against the line alone in
//! its own model (`ohl_electrical::electric`) on the synthetic line, near
//! its jumpers (2026-11-25: the jumpers in the joint system), and a system
//! solved once against the same system solved with its points.

use std::path::Path;

use ohl_core::units::Temperature;
use ohl_electrical::cross_section::Sources;
use ohl_electrical::electric::{self, Settings};
use ohl_electrical::joint::{self, JointSettings, JointWire};
use ohl_electrical::magnetic::GivenPoint;
use ohl_line::Line;

fn synthetic() -> (Line, ohl_cables::Library) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let line = Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
    let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
    (line, library)
}

/// The line's wires as one part (one frequency), earth wires earthed.
fn as_one_part(wires: Vec<(Option<f64>, JointWire)>) -> Vec<JointWire> {
    wires
        .into_iter()
        .map(|(f, mut w)| {
            w.part = f.map(|_| 0);
            w
        })
        .collect()
}

/// Below each jumper (2 m under three of its points) the line alone in its
/// own model with its jumpers carrying their phase's charge, against the
/// joint system of the line alone with and without its jumpers: with them
/// the joint value is near the own model's (both models hold the jumper at
/// its phase); without them it misses the jumper's field — what the joint
/// system left out before 2026-11-25 (the larger of the two models was
/// kept, so that was safe where the own model counted them).
#[test]
fn the_jumpers_in_the_joint_system_against_the_line_alone() {
    let (line, library) = synthetic();
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let jumpers: Vec<_> = ohl_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    assert!(!jumpers.is_empty(), "no jumper");
    // 2 m under its lowest point and under its points a quarter and three
    // quarters along.
    let below: Vec<[f64; 3]> = jumpers
        .iter()
        .flat_map(|j| {
            let n = j.points.len();
            [j.low_point, j.points[n / 4], j.points[3 * n / 4]].map(|p| [p[0], p[1], p[2] - 2.0])
        })
        .collect();
    let own = |with: bool| -> Vec<f64> {
        let src = Sources {
            line: &line,
            library: &library,
            shapes: &shapes,
            temperature: case.temperature,
            ground: None,
            jumpers: &jumpers,
        };
        let s = Settings {
            profiles: false,
            points: below
                .iter()
                .enumerate()
                .map(|(k, p)| GivenPoint {
                    name: format!("J{k}"),
                    x: p[0],
                    y: p[1],
                    z: Some(p[2]),
                    height: None,
                })
                .collect(),
            ground_model: electric::GroundModel::Plane,
            jumpers: with,
            ..Settings::default()
        };
        electric::run(&src, &s, None)
            .unwrap()
            .points
            .iter()
            .map(|p| p.e_kv_m)
            .collect()
    };
    let ground = joint::feet_ground_of(&[&line]);
    let g = |x: f64, y: f64| ground(x, y);
    let joint_at = |with: bool| -> Vec<f64> {
        let mut notes = Vec::new();
        let js: &[ohl_line::jumper::JumperShape] = if with { &jumpers } else { &[] };
        let wires = as_one_part(joint::line_wires(
            &line,
            &library,
            &shapes,
            js,
            &[],
            "line",
            &mut notes,
        ));
        below
            .iter()
            .map(|p| {
                let r = joint::solve(
                    &wires,
                    1,
                    [p[0], p[1]],
                    std::slice::from_ref(p),
                    &g,
                    &JointSettings::default(),
                )
                .unwrap();
                joint::worst(&r.parts[0]) * 1e-3
            })
            .collect()
    };
    let (alone, alone_without) = (own(true), own(false));
    let (with, without) = (joint_at(true), joint_at(false));
    let (mut off_with, mut off_without) = (0.0_f64, 0.0_f64);
    for k in 0..below.len() {
        let rw = with[k] / alone[k];
        let ro = without[k] / alone[k];
        eprintln!(
            "jumper {k}: alone {:.3} (without jumpers {:.3}) kV/m; joint with {:.3}, without \
             {:.3} kV/m",
            alone[k], alone_without[k], with[k], without[k]
        );
        // The jumper below a few metres adds to the field in both models.
        assert!(alone[k] > alone_without[k], "{k}");
        assert!(with[k] > without[k], "{k}");
        off_with = off_with.max((rw - 1.0).abs());
        off_without = off_without.max((ro - 1.0).abs());
    }
    eprintln!(
        "joint against alone near the jumpers: with them at most {:.1} % off, without {:.1} %",
        off_with * 100.0,
        off_without * 100.0
    );
    assert!(off_with < off_without, "{off_with} {off_without}");
    assert!(off_with < 0.25, "{off_with}");
}

/// A system solved once and asked at points after
/// ([`joint::System::field`], [`joint::System::field_at`]) gives the
/// field of [`joint::solve`] at the same points, to the bit.
#[test]
fn a_system_solved_once_is_the_solve() {
    let (line, library) = synthetic();
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let mut notes = Vec::new();
    let wires = as_one_part(joint::line_wires(
        &line,
        &library,
        &shapes,
        &[],
        &[],
        "line",
        &mut notes,
    ));
    let ground = joint::feet_ground_of(&[&line]);
    let g = |x: f64, y: f64| ground(x, y);
    let a = shapes[0].spans[0].points[0];
    let centre = [a[0], a[1]];
    let points: Vec<[f64; 3]> = (0..40)
        .map(|k| {
            [
                a[0] + f64::from(k) * 3.0,
                a[1] - 20.0 + f64::from(k),
                a[2] - 15.0,
            ]
        })
        .collect();
    let s = JointSettings::default();
    let once = joint::solve(&wires, 1, centre, &points, &g, &s).unwrap();
    let system = joint::System::solve(&wires, 1, centre, &g, &s).unwrap();
    assert_eq!(system.pieces(), once.pieces);
    let after = system.field(&points, &g);
    for (k, p) in points.iter().enumerate() {
        assert_eq!(after[k], once.parts[k]);
        assert_eq!(system.field_at(*p, &g), once.parts[k]);
    }
}
