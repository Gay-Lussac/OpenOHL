// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the weather cases given per span.

use super::*;
use ohl_core::units::Temperature;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap()
}

/// Spread loads hang on the stretch of wire they cover: over a whole
/// span the sag grows; over its first half the low point moves towards
/// it. Span points are nodes on the wire, reported where they are
/// placed (by fraction or distance), on the solved wire.
#[test]
fn spread_loads_and_span_points_on_the_wire() {
    let lib = library();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let base = crate::tests::example();
    let shape = |line: &Line| solve_line(line, &lib, &case)[0].clone().unwrap();
    let plain = shape(&base);
    let span = &plain.spans[0];
    // Over the whole first span, as heavy as the wire.
    let cable = lib.get(&base.sections[0].cable).unwrap();
    let q = cable.weight().si() * f64::from(base.sections[0].bundle.max(1));
    let mut whole = base.clone();
    whole.sections[0].add_spread_load(&crate::legacy::SpreadLoad {
        span: 0,
        start: crate::SpanPlace::at_fraction(0.0),
        end: crate::SpanPlace::at_fraction(1.0),
        load: q,
        label: "as heavy as the wire".into(),
    });
    let heavy = shape(&whole);
    assert!(
        heavy.spans[0].sag > span.sag * 1.05,
        "{} {}",
        heavy.spans[0].sag,
        span.sag
    );
    // Over the first half: the low point towards it.
    let mut half = base.clone();
    half.sections[0].add_spread_load(&crate::legacy::SpreadLoad {
        span: 0,
        start: crate::SpanPlace::at_fraction(0.0),
        end: crate::SpanPlace::at_fraction(0.5),
        load: 3.0 * q,
        label: String::new(),
    });
    let lopsided = shape(&half);
    let along = |p: Point, s: &SectionShape| {
        let (a, b) = (s.clamps[0], s.clamps[1]);
        ((p[0] - a[0]).hypot(p[1] - a[1])) / ((b[0] - a[0]).hypot(b[1] - a[1]))
    };
    assert!(
        along(lopsided.spans[0].low_point, &lopsided) < along(span.low_point, &plain) - 0.02,
        "{} {}",
        along(lopsided.spans[0].low_point, &lopsided),
        along(span.low_point, &plain)
    );
    // Span points: where placed, on the wire.
    let mut pointed = base.clone();
    let horizontal = {
        let (a, b) = (plain.clamps[0], plain.clamps[1]);
        (b[0] - a[0]).hypot(b[1] - a[1])
    };
    pointed.sections[0].span_points.push(crate::SpanPoint {
        label: "P1".into(),
        span: 0,
        at: crate::SpanPlace::at_fraction(0.25),
        description: String::new(),
        object: Default::default(),
    });
    pointed.sections[0].span_points.push(crate::SpanPoint {
        label: "P2".into(),
        span: 0,
        at: crate::SpanPlace::at_distance(0.6 * horizontal),
        description: String::new(),
        object: Default::default(),
    });
    let s = shape(&pointed);
    let mine: Vec<&(String, Point)> = s
        .span_points
        .iter()
        .filter(|(l, _)| l == "P1" || l == "P2")
        .collect();
    assert_eq!(mine.len(), 2);
    for ((label, at), f) in mine.into_iter().zip([0.25, 0.6]) {
        assert!(
            (along(*at, &s) - f).abs() < 0.01,
            "{label} {}",
            along(*at, &s)
        );
        assert!(
            at[2] < s.clamps[0][2].max(s.clamps[1][2]),
            "below the clamps"
        );
    }
    // By station: half-way between the span's structures' stations.
    let (sa, sb) = pointed
        .span_stations(
            &pointed.sections[0].points[0].structure,
            &pointed.sections[0].points[1].structure,
        )
        .expect("the span on an alignment");
    let mut by_station = base.clone();
    by_station.sections[0].span_points.push(crate::SpanPoint {
        label: "S".into(),
        span: 0,
        at: crate::SpanPlace::at_station(0.5 * (sa + sb)),
        description: String::new(),
        object: Default::default(),
    });
    by_station.validate().unwrap();
    let resolved = by_station.section_with_fractions(&by_station.sections[0]);
    assert!((resolved.span_points[0].at.fraction().unwrap() - 0.5).abs() < 1e-9);
    let at = shape(&by_station).span_points[0].1;
    let st = shape(&by_station);
    assert!((along(at, &st) - 0.5).abs() < 0.02, "{}", along(at, &st));
    // A span point alone changes nothing of the shape (the sag as
    // sampled: within a millimetre).
    assert!(
        (s.spans[0].sag - span.sag).abs() < 1e-3,
        "{} {}",
        s.spans[0].sag,
        span.sag
    );
}

/// A spread on each subconductor of a 2-bundle carries twice the load
/// of one on the whole bundle: it hangs as a whole-bundle spread of
/// twice the load, lower than the one of the same load.
#[test]
fn a_spread_on_each_subconductor_carries_the_bundle_times_its_load() {
    let lib = library();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let mut base = crate::tests::example();
    base.jumpers.clear();
    base.sections[0].bundle = 2;
    base.sections[0].bundle_spacing = Some(0.4);
    let with = |load: f64, per_subconductor: bool| {
        let mut l = base.clone();
        l.sections[0].add_spread_load(&crate::legacy::SpreadLoad {
            span: 0,
            start: crate::SpanPlace::at_fraction(0.0),
            end: crate::SpanPlace::at_fraction(1.0),
            load,
            label: String::new(),
        });
        l.sections[0].spreads[0].per_subconductor = per_subconductor;
        l.validate().unwrap();
        assert_eq!(
            l.sections[0].spread_loads()[0].load,
            if per_subconductor { 2.0 * load } else { load }
        );
        solve_line(&l, &lib, &case)[0].clone().unwrap().spans[0].sag
    };
    let q = 20.0;
    let (whole, each, twice) = (with(q, false), with(q, true), with(2.0 * q, false));
    assert!(each > whole + 0.01, "{each} {whole}");
    assert!((each - twice).abs() < 1e-6, "{each} {twice}");
}
