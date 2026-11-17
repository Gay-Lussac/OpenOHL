// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The swing arc as a band (2026-10-21) against the wind steps at 10 Pa
//! on the example lines (theory: LiDAR analyses › Danger trees › Wire
//! positions checked, and Vertical and radial clearances › Arc or
//! steps): every stepped position's centre line, measured to the band,
//! and to the arc at the largest radius alone (as before 2026-10-21) —
//! the spans', and the jumpers' own bands (2026-10-21). Since 2026-11-16
//! the band also holds its levels (intermediate solved pressures; since
//! 2026-11-16 at least one every [`BAND_SPACING`] by default): the steps
//! are solved in a run of their own and measured to the band of a run
//! without steps.

use std::path::Path;

use ohl_clearance::positions::{
    BAND_MARGIN, BAND_SPACING, BandLevels, JUMPER_BAND_MARGIN, PositionKind, PositionOptions,
    Shape, SwingArc, solve_positions,
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
/// `case`, with the band's `levels`, against its steps at 10 Pa from
/// both sides, the case's wind from `direction` (degrees, the program's
/// convention; none: as in the file, across the line).
fn measure_at(
    file: &str,
    case: &str,
    jumpers: bool,
    direction: Option<f64>,
    levels: BandLevels,
) -> Measured {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let line = ohl_line::Line::load(&root.join("examples/synthetic-line").join(file)).unwrap();
    let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
    let cases =
        ohl_weather::WeatherCases::load(&root.join("examples/synthetic-line/weather-cases.ohlwc"))
            .unwrap()
            .select(&[case.into()], &[])
            .unwrap();
    let cases: Vec<ohl_weather::WeatherCase> = cases
        .into_iter()
        .map(|c| match direction {
            Some(a) => c.with_direction(ohl_weather::WindDirection::Angle(a)),
            None => c,
        })
        .collect();
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
            swing_arc: false,
            wind_steps: Some(steps),
            jumpers,
            ..Default::default()
        },
        &Budget::new(),
    )
    .unwrap();
    assert!(positions.notes.is_empty(), "{:?}", positions.notes);
    let banded = solve_positions(
        &line,
        &library,
        &cases,
        PositionOptions {
            swing_arc: true,
            jumpers,
            band_levels: levels,
            ..Default::default()
        },
        &Budget::new(),
    )
    .unwrap();
    // (Solved pressures further apart than 200 Pa: the band's note only.)
    let gap = cases
        .iter()
        .map(|c| levels.largest_gap(c.wind_pressure.si()))
        .fold(0.0, f64::max);
    let note = ohl_clearance::positions::band_levels_note(&levels, gap);
    assert!(
        banded.notes.iter().all(|n| Some(n) == note.as_ref()),
        "{:?}",
        banded.notes
    );
    assert_eq!(note.is_some(), gap > BAND_SPACING + 10.0);
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
        let Some(arc) = banded
            .span_of(&span.wire.section, span.wire.span)
            .and_then(|k| {
                banded.spans[k]
                    .positions
                    .iter()
                    .find_map(|&i| match &banded.positions[i].shape {
                        Shape::Arc(a) => Some(a),
                        _ => None,
                    })
            })
        else {
            continue;
        };
        // (No span of the example lines is pulled to its chord; a
        // jumper's band is not asked.)
        assert!(jumpers || !arc.above_axis(), "{}", span.wire.section);
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

/// The worst of [`measure_at`] over `files` × `cases` (and directions):
/// (step positions, band, arc alone, bands).
fn worst(
    files: &[&str],
    cases: &[&str],
    directions: &[Option<f64>],
    jumpers: bool,
    levels: BandLevels,
) -> Measured {
    let mut out = Measured {
        positions: 0,
        band: (0.0, String::new()),
        arc: (0.0, String::new()),
        bands: 0,
    };
    for file in files {
        for case in cases {
            for &d in directions {
                let m = measure_at(file, case, jumpers, d, levels);
                eprintln!(
                    "levels {}, {file}, {case}, {d:?}: {} bands, {} step positions; farthest from the band {:.4} m ({}), from the arc alone {:.4} m ({})",
                    levels.words(),
                    m.bands,
                    m.positions,
                    m.band.0,
                    m.band.1,
                    m.arc.0,
                    m.arc.1
                );
                assert!(m.bands > 0 && m.positions > 0, "{file} {case} {d:?}");
                out.positions += m.positions;
                out.bands += m.bands;
                if m.band.0 > out.band.0 {
                    out.band = m.band;
                }
                if m.arc.0 > out.arc.0 {
                    out.arc = m.arc;
                }
            }
        }
    }
    eprintln!(
        "levels {}: {} bands, {} step positions; band {:.4} m ({}); arc alone {:.4} m ({})",
        levels.words(),
        out.bands,
        out.positions,
        out.band.0,
        out.band.1,
        out.arc.0,
        out.arc.1
    );
    out
}

const LINES: [&str; 2] = ["synthetic-line.ohl.json", "crossing-line.ohl.json"];
const WINDY: [&str; 2] = ["high wind", "rime ice + wind"];

/// The band holds every step at 10 Pa (both sides) of both windy example
/// cases on the two example lines (the line and the crossing line) once
/// widened by its margin [`BAND_MARGIN`]: no step's centre line lies
/// outside the widened band (tolerance 0). Without the margin: the band of
/// still air and the blowouts alone (0 levels, as before 2026-11-16) up to
/// 0.091 m (0.089 m measured 2026-10-21); with a level every
/// [`BAND_SPACING`] (the default, 2026-11-16: 200 and 400 Pa for the
/// 600 Pa case, none for the 200 Pa one) 0.041 m — with 4 levels a side
/// (the default of 2026-11-16) it was 0.009 m (numbers printed, see the
/// theory). The arc alone
/// at the largest radius (before 2026-10-21) misses them by up to 1.53 m.
#[test]
fn the_band_holds_the_steps_at_10_pa() {
    let old = worst(&LINES, &WINDY, &[None], false, BandLevels::NONE);
    assert!(old.positions > 0);
    // Measured: the band alone misses by up to 0.091 m (0.089 m on
    // 2026-10-21); the margin holds that with room to spare.
    assert!(old.band.0 > 0.0 && old.band.0 < 0.1, "{:?}", old.band);
    assert!(old.band.0 <= BAND_MARGIN, "{:?}", old.band);
    // The arc alone misses steps on its inner side by far more.
    assert!(old.arc.0 > 0.4, "{:?}", old.arc);
    // With the default levels (measured 2026-11-16): 0.041 m (0.0911 m
    // without; 0.0090 m with 4 levels a side).
    let new = worst(&LINES, &WINDY, &[None], false, BandLevels::DEFAULT);
    assert!(new.band.0 < 0.05, "{:?}", new.band);
    let four = worst(&LINES, &WINDY, &[None], false, BandLevels::from_count(4));
    assert!(four.band.0 < 0.02, "{:?}", four.band);
    assert!(new.band.0 <= BAND_MARGIN, "{:?}", new.band);
}

/// The jumpers' own bands (2026-10-21; about the line through their end
/// clamps) against their steps at 10 Pa from both sides, both windy
/// example cases, on the example line (the crossing line has no jumper),
/// as the spans' above: every jumper has a band. Measured 2026-10-21 (0
/// levels): a step's centre line lies up to **0.307 m** outside the
/// jumper's band (0.293 m on 2026-10-21; high wind, blown right at
/// 230 Pa, inside its inner radius; rime ice and wind: 0.129 m), against 2.54 m from the arc alone
/// at the largest radius. The owner's decision of 2026-10-21: the jumpers'
/// band margin is a value of its own ([`JUMPER_BAND_MARGIN`]), **0 by
/// default** — a Simplification on the user's responsibility (exact: the
/// wind steps), the user entering a value for their jumpers; the hover
/// text and the theory quote this measured excess. So with the default the
/// jumper's band is *not* conservative between its blowouts: this test
/// records the excess (more than the default jumper margin) and fails when
/// it changes much; with the band's default levels (a level every 200 Pa,
/// 2026-11-16) 0.18 m on this line (0.043 m with 4 levels a side; numbers
/// printed, see the theory).
#[test]
fn the_jumper_bands_hold_the_steps_at_10_pa() {
    let old = worst(&LINES[..1], &WINDY, &[None], true, BandLevels::NONE);
    assert!(old.bands > 0 && old.positions > 0);
    // The jumper's band misses its steps (0.307 m measured): more than
    // its default margin (0: not conservative between the blowouts, the
    // user's value to enter) and the spans' default alike.
    assert_eq!(JUMPER_BAND_MARGIN, 0.0);
    assert!(old.band.0 > BAND_MARGIN, "{:?}", old.band);
    assert!(old.band.0 > 0.28 && old.band.0 < 0.31, "{:?}", old.band);
    // The arc alone misses them by far more.
    assert!(old.arc.0 > 1.0, "{:?}", old.arc);
    // With the default levels (measured 2026-11-16): 0.18 m (0.307 m
    // without; still more than the default jumper margin of 0).
    let new = worst(&LINES[..1], &WINDY, &[None], true, BandLevels::DEFAULT);
    assert!(new.band.0 < 0.2, "{:?}", new.band);
}

/// A wind at an angle (2026-11-16): the band of a case's blowouts at θ
/// and 360° − θ, and along the line (0°: one blowout, no other side),
/// against its steps at 10 Pa from both sides — the steps of a wind at an
/// angle the direction and its mirror ([`ohl_clearance::positions::from_side`]).
/// Every step's centre line within the band widened by its margin
/// [`BAND_MARGIN`], as for a wind across the line (measured 2026-11-16 on
/// the example line, both windy cases, at 30°, 60° and 0°, with 0 levels
/// and the default levels — measured again 2026-11-16 with a level every
/// 200 Pa; numbers printed, see the theory).
#[test]
fn the_band_holds_the_steps_of_a_wind_at_an_angle() {
    let at = [Some(30.0), Some(60.0), Some(0.0)];
    let old = worst(&LINES[..1], &WINDY, &at, false, BandLevels::NONE);
    assert!(old.band.0 <= BAND_MARGIN, "{:?}", old.band);
    // Measured 2026-11-16: 0.082 m without levels, 0.032 m with the
    // default (0.0045 m with 4 levels a side, 2026-11-16).
    let new = worst(&LINES[..1], &WINDY, &at, false, BandLevels::DEFAULT);
    assert!(new.band.0 < 0.04, "{:?}", new.band);
}

/// With the wind steps solved, every step is one of the band's levels
/// (2026-11-16): the band holds each step at its stations by
/// construction; between them each step's points lie within millimetres
/// of it (the levels taken at the stations of still air and the blowouts,
/// linearly between).
#[test]
fn the_band_of_a_stepped_wind_holds_its_steps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let line = ohl_line::Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json"))
        .unwrap();
    let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
    let cases =
        ohl_weather::WeatherCases::load(&root.join("examples/synthetic-line/weather-cases.ohlwc"))
            .unwrap()
            .select(&["high wind".into()], &[])
            .unwrap();
    let positions = solve_positions(
        &line,
        &library,
        &cases,
        PositionOptions {
            swing_arc: true,
            wind_steps: Some(WindSteps {
                stepped: true,
                step: 10.0,
                sides: WindSides::Both,
                ..Default::default()
            }),
            ..Default::default()
        },
        &Budget::new(),
    )
    .unwrap();
    let mut worst = 0.0f64;
    let mut levels = 0;
    for span in &positions.spans {
        let arc = span
            .positions
            .iter()
            .find_map(|&i| match &positions.positions[i].shape {
                Shape::Arc(a) => Some(a),
                _ => None,
            })
            .unwrap();
        levels = levels.max(arc.level_count());
        for &i in &span.positions {
            if let Shape::Wire(w) = &positions.positions[i].shape {
                for q in &w.wire.points {
                    worst = worst.max(arc.nearest(*q).0);
                }
            }
        }
    }
    eprintln!("stepped: {levels} levels; farthest step point from the band {worst:.5} m");
    // Measured 2026-11-16: 118 levels, 0.00012 m.
    assert!(levels > 20, "{levels}");
    assert!(worst < 0.001, "{worst}");
}
