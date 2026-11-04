// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The conductor surface gradient of the synthetic double circuit
//! (`examples/electrical`) in its 2D cross-sections: where, the earth
//! wires, the voltage given, the symmetric circuits, against
//! Markt–Mengele.

use std::path::Path;

use ohl_core::units::Temperature;
use ohl_electrical::cross_section::Sources;
use ohl_electrical::electric::CircuitVoltage;
use ohl_electrical::magnetic::StationSpec;
use ohl_electrical::surface_gradient::{self, Settings, SurfaceGradients};
use ohl_line::Line;

fn gradients(settings: &Settings) -> SurfaceGradients {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/electrical");
    let line =
        Line::from_json(&std::fs::read_to_string(dir.join("double-circuit.ohl.json")).unwrap())
            .unwrap();
    let library = ohl_cables::Library::load(&dir.join("wires.ohlw")).unwrap();
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(40.0));
    let shapes: Vec<_> = ohl_line::solve::solve_line(&line, &library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let src = Sources {
        line: &line,
        library: &library,
        shapes: &shapes,
        temperature: Temperature::from_celsius(40.0),
        ground: None,
        jumpers: &[],
    };
    surface_gradient::run(&src, settings).unwrap()
}

/// Every span at its lowest point and both ends; the phases at some
/// kV/cm, the earth wires lower but not zero (their induced charge); the
/// average maximum between the subconductors' mean and the maximum, near
/// Markt–Mengele's; the voltage not given said.
#[test]
fn every_span_three_cross_sections() {
    let g = gradients(&Settings::default());
    let spans = ohl_line::Line::from_json(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/electrical/double-circuit.ohl.json"),
        )
        .unwrap(),
    )
    .map(|l| ohl_electrical::cross_section::line_spans(&l).len())
    .unwrap();
    assert_eq!(g.cuts.len(), 3 * spans, "{:?}", g.notes);
    for place in ["start", "lowest", "end"] {
        assert!(g.cuts.iter().any(|c| c.place == place));
    }
    assert!(
        g.notes.iter().any(|n| n.contains("nominal")),
        "{:?}",
        g.notes
    );
    for c in &g.cuts {
        assert!(c.potential_error < 1e-4, "{}", c.potential_error);
        let phases: Vec<_> = c.bundles.iter().filter(|b| b.kv > 0.0).collect();
        let earth: Vec<_> = c.bundles.iter().filter(|b| b.kv == 0.0).collect();
        assert!(!phases.is_empty() && !earth.is_empty());
        for b in &phases {
            assert!(
                b.average_max_kv_cm > 3.0 && b.average_max_kv_cm < 25.0,
                "{}",
                b.average_max_kv_cm
            );
            assert!(b.max_kv_cm >= b.average_max_kv_cm);
            for s in &b.subconductors {
                assert!(s.max_kv_cm > s.mean_kv_cm);
            }
            // Markt–Mengele within a few per cent of the simulation.
            let r = b.markt_mengele_kv_cm / b.average_max_kv_cm;
            assert!((r - 1.0).abs() < 0.03, "Markt–Mengele {r}");
        }
        for b in &earth {
            assert!(b.average_max_kv_cm > 0.0);
            assert!(
                b.average_max_kv_cm
                    < phases
                        .iter()
                        .map(|p| p.average_max_kv_cm)
                        .fold(f64::INFINITY, f64::min)
            );
        }
    }
    let csv = surface_gradient::csv(&g);
    assert!(csv.lines().count() > 3 * spans);
}

/// The gradient goes with the voltage, and the two circuits of the
/// symmetric double circuit have alike gradients at mid-span.
#[test]
fn proportional_and_symmetric() {
    let at = |kv: Option<f64>| {
        gradients(&Settings {
            stations: vec![StationSpec {
                from: "T1".into(),
                to: "T2".into(),
                fraction: Some(0.5),
            }],
            ends: false,
            voltages: kv
                .map(|kv| CircuitVoltage {
                    circuit: "*".into(),
                    kv,
                })
                .into_iter()
                .collect(),
            ..Settings::default()
        })
    };
    let a = at(Some(200.0));
    let b = at(Some(400.0));
    assert_eq!(a.cuts.len(), 1);
    for (x, y) in a.cuts[0].bundles.iter().zip(&b.cuts[0].bundles) {
        assert!((y.average_max_kv_cm / x.average_max_kv_cm - 2.0).abs() < 1e-9);
    }
    // Mirror pairs: a bundle at offset s and one at −s at the same
    // height have the same average maximum (the circuits symmetric).
    let c = &a.cuts[0];
    let centre = |b: &surface_gradient::BundleGradient| {
        let n = b.subconductors.len() as f64;
        (
            b.subconductors.iter().map(|s| s.offset).sum::<f64>() / n,
            b.subconductors.iter().map(|s| s.height).sum::<f64>() / n,
        )
    };
    let mut pairs = 0;
    for x in &c.bundles {
        let (sx, hx) = centre(x);
        if sx.abs() < 0.1 {
            continue;
        }
        if let Some(y) = c.bundles.iter().find(|y| {
            let (sy, hy) = centre(y);
            (sy + sx).abs() < 1e-6 && (hy - hx).abs() < 1e-6
        }) {
            pairs += 1;
            let r = y.average_max_kv_cm / x.average_max_kv_cm;
            assert!((r - 1.0).abs() < 0.05, "{} {} {r}", x.section, y.section);
        }
    }
    assert!(pairs >= 2, "no mirror pairs");
}
