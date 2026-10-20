// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Speed of the danger-tree checks on an invented 10 km line with a
//! million invented vegetation points (theory: How it is made fast).
//! Ignored by default; run with
//! `cargo test -p ohl-clearance --release --test danger_speed -- --ignored --nocapture`.

use std::sync::Arc;
use std::time::Instant;

use ohl_clearance::Wire;
use ohl_clearance::isoclearance::{IsoParams, isoclearance};
use ohl_clearance::positions::{Axis, PositionKind, Positions, Shape, SwingArc, WireShape};
use ohl_clearance::vegetation::{VegetationCheck, VegetationParams};
use ohl_core::budget::Budget;
use ohl_terrain::Terrain;

const LENGTH: f64 = 10_000.0;
const SPAN: f64 = 400.0;

fn ground_z(x: f64, y: f64) -> f64 {
    400.0 + 6.0 * (x / 230.0).sin() + 2.0 * (y / 35.0).cos() + 0.02 * y
}

/// The ground: a TIN of 5 m points over the corridor.
fn ground() -> Terrain {
    let mut pts = Vec::new();
    let mut x = -50.0;
    while x <= LENGTH + 50.0 {
        let mut y = -90.0;
        while y <= 90.0 {
            pts.push([x, y, ground_z(x, y)]);
            y += 5.0;
        }
        x += 5.0;
    }
    Terrain::from_xyz(&pts, None).unwrap()
}

/// Three phases and an earth wire per span, hanging by `sag` and blown
/// by `phi` (rad, positive to the right) about the attachment axis.
fn span_wires(k: usize, phi: f64, sag: f64) -> Vec<(Wire, Axis)> {
    let (x0, x1) = (k as f64 * SPAN, (k + 1) as f64 * SPAN);
    let mut out = Vec::new();
    for (p, (y, h)) in [(-6.0, 30.0), (0.0, 30.0), (6.0, 30.0), (0.0, 38.0)]
        .into_iter()
        .enumerate()
    {
        let a1 = [x0, y, ground_z(x0, 0.0) + h];
        let a2 = [x1, y, ground_z(x1, 0.0) + h];
        let points: Vec<[f64; 3]> = (0..=80)
            .map(|i| {
                let t = i as f64 / 80.0;
                let r = 4.0 * sag * t * (1.0 - t);
                [
                    a1[0] + t * (a2[0] - a1[0]),
                    y - r * phi.sin(),
                    a1[2] + t * (a2[2] - a1[2]) - r * phi.cos(),
                ]
            })
            .collect();
        let wire = Wire {
            section: format!("S{p}"),
            span: k,
            circuit: "c".into(),
            phase: format!("L{p}"),
            potential: None,
            bundle: vec![],
            points,
            ends: vec![],
            surface_radius: 0.015,
            polyline_error: 0.002,
        };
        out.push((wire, Axis::new(a1, a2).unwrap()));
    }
    out
}

/// Positions of `cases` cases: still (one position) or windy (left,
/// right and with `arc` the swing arc).
fn positions(cases: usize, windy: bool, arc: bool) -> Positions {
    let mut out = Positions {
        cases: (0..cases).map(|c| format!("case {c}")).collect(),
        ..Positions::default()
    };
    let spans = (LENGTH / SPAN) as usize;
    for k in 0..spans {
        for (p, (w, axis)) in span_wires(k, 0.0, 9.0).into_iter().enumerate() {
            let id = out.add_span(w, axis);
            for c in 0..cases {
                let sag = 9.0 + 0.15 * c as f64;
                if !windy {
                    let (w, _) = span_wires(k, 0.0, sag).swap_remove(p);
                    out.add(
                        id,
                        c,
                        PositionKind::Case,
                        Shape::Wire(WireShape::new(w, &axis)),
                    );
                    continue;
                }
                let phi = (25.0 + c as f64).to_radians();
                let (l, _) = span_wires(k, -phi, sag * 1.04).swap_remove(p);
                let (s, _) = span_wires(k, 0.0, sag).swap_remove(p);
                let (r, _) = span_wires(k, phi, sag * 1.04).swap_remove(p);
                let (l, s, r) = (
                    WireShape::new(l, &axis),
                    WireShape::new(s, &axis),
                    WireShape::new(r, &axis),
                );
                if arc {
                    let a = SwingArc::new(axis, &l, &s, &r).unwrap();
                    out.add(id, c, PositionKind::Arc, Shape::Arc(a));
                }
                out.add(id, c, PositionKind::Left, Shape::Wire(l));
                out.add(id, c, PositionKind::Right, Shape::Wire(r));
            }
        }
    }
    out
}

/// A million vegetation points: a cleared strip of low growth (±12 m),
/// trees up to 28 m beyond, to ±60 m.
fn vegetation(n: usize) -> Vec<[f64; 3]> {
    let mut s = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = move || {
        s = s
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..n)
        .map(|_| {
            let x = rnd() * LENGTH;
            let y = rnd() * 120.0 - 60.0;
            let h = if y.abs() < 12.0 {
                rnd() * 4.0
            } else {
                rnd() * 28.0
            };
            [x, y, ground_z(x, y) + h]
        })
        .collect()
}

fn run(
    name: &str,
    ground: &Terrain,
    pts: &[[f64; 3]],
    positions: Positions,
    params: &VegetationParams,
) {
    let positions = Arc::new(positions);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    // One thread, the first 200 000 points.
    let part = &pts[..pts.len().min(200_000)];
    let mut one = VegetationCheck::with_positions(Arc::clone(&positions), &[5], params.clone());
    let t = Instant::now();
    for p in part {
        one.visit(Some(ground), p[0], p[1], p[2], 5);
    }
    let single = t.elapsed().as_secs_f64() * 1e6 / part.len() as f64;
    // Every core, every point.
    let mut all = VegetationCheck::with_positions(Arc::clone(&positions), &[5], params.clone());
    let t = Instant::now();
    all.visit_parallel(Some(ground), pts, |p| (p[0], p[1], p[2], 5), &Budget::new())
        .unwrap();
    let wall = t.elapsed();
    let work = all.work;
    let results = all.finish();
    let (grow, fall): (u64, u64) = results
        .iter()
        .fold((0, 0), |(g, f), r| (g + r.encroaching, f + r.fall_in));
    println!(
        "{name}: {} positions; one thread {single:.2} µs/point; {threads} threads {:.2} µs/point ({:.2?} for {} points); grow-in {grow}, fall-in {fall}; work {work:?}",
        positions.positions.len(),
        wall.as_secs_f64() * 1e6 / pts.len() as f64,
        wall,
        pts.len()
    );
}

#[test]
#[ignore]
fn speed() {
    let t = Instant::now();
    let ground = ground();
    println!(
        "ground: {} triangles ({:.2?})",
        ground.num_triangles(),
        t.elapsed()
    );
    let pts = vegetation(1_000_000);
    let today = VegetationParams {
        clearance: 5.0,
        growth_per_year: 0.3,
        years: 3.0,
        fall_in: true,
        ..VegetationParams::default()
    };
    let danger = VegetationParams {
        root_ball: 0.1,
        fall_allowance: 1.0,
        buffer: 0.5,
        ..today.clone()
    };
    run(
        "today's check (one case, k = 0)",
        &ground,
        &pts,
        positions(1, false, false),
        &today,
    );
    run(
        "one case, k = 0.1",
        &ground,
        &pts,
        positions(1, false, false),
        &danger,
    );
    run(
        "one windy case: left, right, swing arc, k = 0.1",
        &ground,
        &pts,
        positions(1, true, true),
        &danger,
    );
    run(
        "20 windy cases with swing arcs, k = 0.1",
        &ground,
        &pts,
        positions(20, true, true),
        &danger,
    );
    // Isoclearance: 10 km corridor, ±60 m, 5 m lines to 40 m, 1 m grid
    // refined from 8 m, the worst of 20 cases.
    let p = positions(20, true, true);
    let inside = |_x: f64, y: f64| y.abs() <= 60.0;
    let t = Instant::now();
    let iso = isoclearance(
        &p,
        &ground,
        ([0.0, -60.0], [LENGTH, 60.0]),
        &inside,
        &IsoParams::default(),
        &Budget::new(),
    )
    .unwrap();
    println!(
        "isoclearance 10 km × 120 m, 20 cases: {:.2?}; {} lines, {} of {} coarse cells refined, {} clearances computed",
        t.elapsed(),
        iso.contours.len(),
        iso.refined_cells,
        iso.coarse_cells,
        iso.nodes
    );
}
