// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Catenary benchmarks. Target (DESIGN_GOALS §1): re-solving a tension
//! section after an edit in < 10 ms; one span solve must be far below that.

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use ohl_core::units::{Force, ForcePerLength, Length};
use ohl_sagtension::Catenary;
use ohl_sagtension::catenary::{SpanGeometry, default_solver_budget, tension_for_sag};

fn spans() -> Vec<SpanGeometry> {
    (0..1000)
        .map(|i| {
            SpanGeometry::inclined(
                Length::from_meters(200.0 + (i % 300) as f64),
                Length::from_meters(((i % 41) as f64) - 20.0),
            )
        })
        .collect()
}

fn solve_spans(c: &mut Criterion) {
    let catenary = Catenary::new(
        Force::from_kilonewtons(28.0),
        ForcePerLength::from_newtons_per_meter(15.97),
    )
    .unwrap();
    let spans = spans();
    c.bench_function("catenary/solve_sag_length_1k_spans", |b| {
        b.iter(|| {
            let mut total = 0.0;
            for &geometry in black_box(&spans) {
                let s = catenary.solve(geometry).unwrap();
                total += s.sag().si() + s.length().si() + s.tension_right().si();
            }
            total
        })
    });
}

fn inverse(c: &mut Criterion) {
    let geometry = SpanGeometry::inclined(Length::from_meters(400.0), Length::from_meters(25.0));
    let w = ForcePerLength::from_newtons_per_meter(15.97);
    let budget = default_solver_budget();
    c.bench_function("catenary/tension_for_sag", |b| {
        b.iter(|| {
            tension_for_sag(black_box(geometry), w, Length::from_meters(9.0), &budget).unwrap()
        })
    });
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
}

criterion_group! {
    name = benches;
    config = config();
    targets = solve_spans, inverse
}
criterion_main!(benches);
