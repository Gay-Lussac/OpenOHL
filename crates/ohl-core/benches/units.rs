// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Benchmarks for `ohl_core::units`.
//!
//! `typed_vs_raw` guards the zero-cost promise of ADR-0002: arithmetic on
//! typed quantities must run as fast as on plain `f64`.

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use ohl_core::units::{Force, ForcePerLength, Length};

const SPANS: usize = 10_000;

fn typed_vs_raw(c: &mut Criterion) {
    let spans: Vec<Length> = (0..SPANS)
        .map(|i| Length::from_meters(250.0 + (i % 200) as f64))
        .collect();
    let loads: Vec<ForcePerLength> = (0..SPANS)
        .map(|i| ForcePerLength::from_newtons_per_meter(15.0 + (i % 7) as f64))
        .collect();
    let raw_spans: Vec<f64> = spans.iter().map(|l| l.si()).collect();
    let raw_loads: Vec<f64> = loads.iter().map(|w| w.si()).collect();

    let mut group = c.benchmark_group("units");
    group.bench_function("total_load_typed_10k", |b| {
        b.iter(|| {
            let total: Force = black_box(&loads)
                .iter()
                .zip(black_box(&spans))
                .map(|(&w, &l)| w * l)
                .sum();
            total
        })
    });
    group.bench_function("total_load_raw_10k", |b| {
        b.iter(|| {
            let total: f64 = black_box(&raw_loads)
                .iter()
                .zip(black_box(&raw_spans))
                .map(|(w, l)| w * l)
                .sum();
            total
        })
    });
    group.finish();
}

fn parse(c: &mut Criterion) {
    let inputs = ["1000 ft", "304.8 m", "12.5km", "-3.2e2 mm", "18 in"];
    c.bench_function("units/parse_length_5", |b| {
        b.iter(|| {
            for text in black_box(&inputs) {
                black_box(text.parse::<Length>().unwrap());
            }
        })
    });
}

/// Fixed measurement times keep the benchmark run short and bounded (about
/// 3 s per benchmark) so the CI job has a predictable duration.
fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
}

criterion_group! {
    name = benches;
    config = config();
    targets = typed_vs_raw, parse
}
criterion_main!(benches);
