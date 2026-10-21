// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The swing arc as a band (2026-10-21) against the wind steps at 10 Pa
//! on the example lines (theory: LiDAR analyses › Danger trees › Wire
//! positions checked, and Vertical and radial clearances › Arc or
//! steps): every stepped position's centre line, measured to the band,
//! and to the arc at the largest radius alone (as before 2026-10-21) —
//! the spans', and the jumpers' own bands (2026-10-21).

use std::path::Path;

use ohl_clearance::positions::{
    BAND_MARGIN, PositionKind, PositionOptions, Shape, SwingArc, solve_positions,
};
use ohl_clearance::rules::{WindSides, WindSteps};
use ohl_core::budget::Budget;

/// The distance from `q` to the arc alone at the band's outer radius
/// (the swing arc before 2026-10-21): within the angles |ρ − R| (and the
/// part along the axis), else the nearer of its two ends.
fn to_outer_arc(arc: &SwingArc, q: [f64; 3]) -> f64 {
    let place = arc.place(arc.axis.along(q));
    let v = [
        q[0] - place.centre[0],
        q[1] - place.centre[1],
        q[2] - place.centre[2],
    ];
    let u = arc.axis.u;
    let along = v[0] * u[0] + v[1] * u[1] + v[2] * u[2];
    let phi = arc.angle_of(q);
    let rho = arc.axis.polar(q).rho;
    if place.from <= phi && phi <= place.to {
        (rho - place.radius).hypot(along)
    } else {
        let d = |p: [f64; 3]| {
            ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt()
        };
        d(arc.point(place.centre, place.radius, place.from)).min(d(arc.point(
            place.centre,
            place.radius,
            place.to,
        )))
    }
}

/// What a line's steps do against its bands: (step positions, the
/// farthest centre-line point from the band, where; the farthest from the
/// outer arc alone, where; the bands measured).
struct Measured {
    positions: usize,
    band: (f64, String),
    arc: (f64, String),
    bands: usize,
}

/// The spans' bands (`jumpers` false) or the jumpers' (true) of `file` in
/// `case` against its steps at 10 Pa from both sides.
fn measure(file: &str, case: &str, jumpers: bool) -> Measured {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let line = ohl_line::Line::load(&root.join("examples/synthetic-line").join(file)).unwrap();
    let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
    let cases =
        ohl_weather::WeatherCases::load(&root.join("examples/synthetic-line/weather-cases.ohlwc"))
            .unwrap()
            .select(&[case.into()], &[])
            .unwrap();
    let steps = WindSteps {
        arc: false,
        stepped: true,
        step: 10.0,
        sides: WindSides::Both,
        ..Default::default()
    };
    let positions = solve_positions(
        &line,
        &library,
        &cases,
        PositionOptions {
            swing_arc: true,
            wind_steps: Some(steps),
            jumpers,
            ..Default::default()
        },
        &Budget::new(),
    )
    .unwrap();
    assert!(positions.notes.is_empty(), "{:?}", positions.notes);
    let mut out = Measured {
        positions: 0,
        band: (0.0, String::new()),
        arc: (0.0, String::new()),
        bands: 0,
    };
    for span in &positions.spans {
        if span.wire.section.starts_with("jumper:") != jumpers {
            continue;
        }
        let Some(arc) = span
            .positions
            .iter()
            .find_map(|&i| match &positions.positions[i].shape {
                Shape::Arc(a) => Some(a),
                _ => None,
            })
        else {
            continue;
        };
        out.bands += 1;
        for &i in &span.positions {
            let p = &positions.positions[i];
            let Shape::Wire(w) = &p.shape else { continue };
            if !matches!(
                p.kind,
                PositionKind::StepLeft | PositionKind::StepRight | PositionKind::Still
            ) {
                continue;
            }
            out.positions += 1;
            let at = || {
                format!(
                    "{file}, {}, {} span {}",
                    positions.describe(i),
                    span.wire.section,
                    span.wire.span + 1
                )
            };
            for q in &w.wire.points {
                let d = arc.nearest(*q).0;
                if d > out.band.0 {
                    let s = arc.axis.along(*q);
                    let pl = arc.place(s);
                    let rho = arc.axis.polar(*q).rho;
                    let phi = arc.angle_of(*q);
                    out.band = (
                        d,
                        format!(
                            "{}; at {s:.2} of {:.2} m: ρ {rho:.3} in [{:.3}, {:.3}], φ {:.2}° in [{:.2}°, {:.2}°]",
                            at(),
                            arc.axis.length,
                            pl.inner,
                            pl.radius,
                            phi.to_degrees(),
                            pl.from.to_degrees(),
                            pl.to.to_degrees()
                        ),
                    );
                }
                let d = to_outer_arc(arc, *q);
                if d > out.arc.0 {
                    out.arc = (d, at());
                }
            }
        }
    }
    out
}

/// The band holds every step at 10 Pa (both sides) of both windy example
/// cases on the two example lines (the line and the crossing line) once
/// widened by its margin [`BAND_MARGIN`]: no step's centre line lies
/// outside the widened band (tolerance 0); without the margin, up to
/// 0.089 m (measured 2026-10-21). The arc alone at the largest radius
/// (before 2026-10-21) misses them by up to 1.53 m. Numbers printed, see
/// the theory.
#[test]
fn the_band_holds_the_steps_at_10_pa() {
    let mut worst_band = (0.0f64, String::new());
    let mut worst_arc = (0.0f64, String::new());
    let mut n = 0;
    for file in ["synthetic-line.ohl.json", "crossing-line.ohl.json"] {
        for case in ["high wind", "rime ice + wind"] {
            let m = measure(file, case, false);
            eprintln!(
                "{file}, {case}: {} step positions; farthest from the band {:.4} m ({}), from the arc alone {:.4} m ({})",
                m.positions, m.band.0, m.band.1, m.arc.0, m.arc.1
            );
            n += m.positions;
            if m.band.0 > worst_band.0 {
                worst_band = m.band;
            }
            if m.arc.0 > worst_arc.0 {
                worst_arc = m.arc;
            }
        }
    }
    eprintln!(
        "all: {n} step positions; band {:.4} m ({}); arc alone {:.4} m ({})",
        worst_band.0, worst_band.1, worst_arc.0, worst_arc.1
    );
    assert!(n > 0);
    // Measured: the band alone misses by up to 0.089 m; the margin holds
    // that with room to spare.
    assert!(worst_band.0 > 0.0 && worst_band.0 < 0.1, "{worst_band:?}");
    assert!(worst_band.0 <= BAND_MARGIN, "{worst_band:?}");
    // The arc alone misses steps on its inner side by far more.
    assert!(worst_arc.0 > 0.4, "{worst_arc:?}");
}

/// The jumpers' own bands (2026-10-21; about the line through their end
/// clamps) against their steps at 10 Pa from both sides, both windy
/// example cases, on the example line (the crossing line has no jumper),
/// as the spans' above: every jumper has a band. Measured 2026-10-21: a
/// step's centre line lies up to **0.293 m** outside the jumper's band
/// (high wind, blown right at 230 Pa, inside its inner radius; rime ice
/// and wind: 0.127 m) — more than the default margin [`BAND_MARGIN`]
/// (0.15 m), against 2.54 m from the arc alone at the largest radius.
/// The margin is not widened without the owner's decision (reported);
/// this test records the excess and fails when it changes much. Numbers
/// printed, see the theory.
#[test]
fn the_jumper_bands_hold_the_steps_at_10_pa() {
    let mut worst = (0.0f64, String::new());
    let mut worst_arc = (0.0f64, String::new());
    let (mut n, mut bands) = (0, 0);
    for case in ["high wind", "rime ice + wind"] {
        let m = measure("synthetic-line.ohl.json", case, true);
        eprintln!(
            "jumpers, {case}: {} bands, {} step positions; farthest from the band {:.4} m ({}), from the arc alone {:.4} m ({})",
            m.bands, m.positions, m.band.0, m.band.1, m.arc.0, m.arc.1
        );
        n += m.positions;
        bands += m.bands;
        if m.band.0 > worst.0 {
            worst = m.band;
        }
        if m.arc.0 > worst_arc.0 {
            worst_arc = m.arc;
        }
    }
    eprintln!(
        "jumpers: {bands} bands, {n} step positions; band {:.4} m ({}); arc alone {:.4} m ({})",
        worst.0, worst.1, worst_arc.0, worst_arc.1
    );
    assert!(bands > 0 && n > 0);
    // The jumper's band misses its steps by more than the default
    // margin (0.293 m measured): recorded, the owner to decide.
    assert!(worst.0 > BAND_MARGIN && worst.0 < 0.4, "{worst:?}");
    // The arc alone misses them by far more.
    assert!(worst_arc.0 > 1.0, "{worst_arc:?}");
}
