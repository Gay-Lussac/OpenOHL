// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of a section in the blocks over frictionless sheaves (owner
//! decision 2026-10-25) against the simplified equal-H model.

use super::*;
use ohl_core::units::{Length, Temperature};

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap()
}

/// The example line without jumpers (they are hung after stringing),
/// section S1 strung at 25 kN at 15 °C, its point loads off.
fn example() -> Line {
    let mut line = crate::tests::example();
    line.jumpers.clear();
    line.sections[0].clear_point_loads();
    line.sections[0].stringing = Stringing::Tension {
        horizontal_tension: 25_000.0,
        temperature: 15.0,
    };
    line
}

fn iced() -> WeatherCase {
    let mut c = WeatherCase::still("ice", Temperature::from_celsius(-5.0));
    c.ice_thickness = Length::from_si(0.02);
    c
}

/// The wire's tension at both ends of every span of a rolled reference:
/// each span one element of its rolled length between the rolled clamps,
/// its load the case's (per unstressed metre: no stretch to follow).
fn end_tensions(r: &Reference, cable: &Cable, case: &WeatherCase) -> Vec<(f64, f64)> {
    let ea = cable.axial_stiffness().si();
    let thermal = (cable.thermal_expansion() * (case.temperature.celsius() - r.temperature)).exp();
    r.clamps
        .windows(2)
        .enumerate()
        .map(|(s, c)| {
            let d = c[1] - c[0];
            let (own, stretched) = span_loads_split(
                case,
                cable,
                d,
                r.weight_scale[s] / thermal,
                crate::LoadsPer::Unstretched,
            );
            assert_eq!(stretched, Vector3::zeros());
            let state = CableProperties {
                unstressed_length: r.lengths[s] * thermal,
                axial_stiffness: ea,
                load: own,
            }
            .solve(d, None, &default_element_budget())
            .unwrap();
            (state.tension_i, state.tension_j)
        })
        .collect()
}

/// Over the sheaves the wire's tension is the same on both sides of every
/// suspension clamp, at stringing and under ice; the section keeps its
/// unstressed length; and the spans' horizontal tensions differ (the
/// example's spans are unequal and inclined).
#[test]
fn the_wire_tension_is_equal_on_both_sides_of_each_sheave() {
    let lib = library();
    let line = example();
    let section = &line.sections[0];
    let budget = default_fe_budget();
    let cable = &*strung_cable(section, lib.get(&section.cable).unwrap()).unwrap();
    let (layout, _) =
        estimate_strain_weights(section, holders_of(&line, section).unwrap(), cable, &budget)
            .unwrap();
    let mut reference = reference(section, &layout, cable, &budget).unwrap();
    reference.loads_per = crate::LoadsPer::Unstretched;
    let total: f64 = reference.lengths.iter().sum();
    for case in [
        WeatherCase::still("stringing", Temperature::from_celsius(15.0)),
        iced(),
    ] {
        let r = rolled(
            section,
            &layout,
            &reference,
            cable,
            Cases::one(&case),
            &budget,
        )
        .unwrap();
        let ends = end_tensions(&r, cable, &case);
        for k in 1..ends.len() {
            let (back, ahead) = (ends[k - 1].1, ends[k].0);
            assert!(
                (back - ahead).abs() < 1e-6 * back,
                "{}: clamp {k}: {back} vs {ahead}",
                case.name
            );
        }
        let sum: f64 = r.lengths.iter().sum();
        assert!((sum - total).abs() < 1e-9 * total, "{sum} vs {total}");
        // Not the equal-H sharing.
        let mut equal = reference.clone();
        equal.blocks = crate::InTheBlocks::EqualH;
        let e = rolled(section, &layout, &equal, cable, Cases::one(&case), &budget).unwrap();
        let moved = r
            .lengths
            .iter()
            .zip(&e.lengths)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(moved > 1e-4, "{}: {moved}", case.name);
    }
}

/// The whole line solved over sheaves against the simplified model: the
/// spans' horizontal tensions and sags change a little (printed: the
/// numbers of the decision log), the clipped-in section reproduces the
/// in-the-blocks state at stringing, and the clipping offsets at the
/// sagging temperature stay zero.
#[test]
fn the_example_over_sheaves_against_equal_h() {
    let lib = library();
    let sheaves = example();
    let mut equal = sheaves.clone();
    equal.solver.blocks = crate::InTheBlocks::EqualH;
    let mut hot = WeatherCase::still("hot", Temperature::from_celsius(80.0));
    hot.ice_thickness = Length::ZERO;
    let cases = [
        WeatherCase::still("15", Temperature::from_celsius(15.0)),
        hot,
        iced(),
    ];
    let (mut dh, mut dsag): (f64, f64) = (0.0, 0.0);
    for case in &cases {
        let a = solve_line(&sheaves, &lib, case).remove(0).unwrap();
        let b = solve_line(&equal, &lib, case).remove(0).unwrap();
        for (k, (x, y)) in a.spans.iter().zip(&b.spans).enumerate() {
            println!(
                "{}: span {}: H {:.1} vs {:.1} N, sag {:.4} vs {:.4} m",
                case.name,
                k + 1,
                x.horizontal_tension,
                y.horizontal_tension,
                x.sag,
                y.sag
            );
            dh = dh.max((x.horizontal_tension - y.horizontal_tension).abs());
            dsag = dsag.max((x.sag - y.sag).abs());
        }
    }
    println!("largest change: H {dh:.1} N, sag {:.1} mm", dsag * 1e3);
    assert!(dh > 1.0 && dh < 0.02 * 25_000.0, "{dh}");
    assert!(dsag > 1e-4 && dsag < 0.5, "{dsag}");
    // Clipped in with the sheaves' lengths at stringing: the same state.
    let section = &sheaves.sections[0];
    let cable = lib.get(&section.cable).unwrap();
    let (lengths, temperature) = clip_lengths(&sheaves, section, cable).unwrap();
    let (equal_lengths, _) = clip_lengths(&equal, section, cable).unwrap();
    let sum = |l: &[f64]| l.iter().sum::<f64>();
    assert!((sum(&lengths) - sum(&equal_lengths)).abs() < 1e-9 * sum(&lengths));
    let mut clipped = sheaves.clone();
    clipped.sections[0].stringing = Stringing::UnstressedLengths {
        lengths,
        temperature,
        sagged: Some(Box::new(section.stringing.clone())),
    };
    let a = solve_line(&sheaves, &lib, &cases[0]).remove(0).unwrap();
    let c = solve_line(&clipped, &lib, &cases[0]).remove(0).unwrap();
    // Within 2 N (1e-4): the example's strain assemblies are given as
    // geometry only, their weights estimated from each section's own
    // stringing state (the strung one at the given H with the strings
    // plumb, as before; the clipped one from its lengths): 1 N apart.
    for (x, y) in a.spans.iter().zip(&c.spans) {
        assert!(
            (x.horizontal_tension - y.horizontal_tension).abs() < 2.0,
            "{} vs {}",
            x.horizontal_tension,
            y.horizontal_tension
        );
    }
    // The sagging table over sheaves: offsets zero at the sagging
    // temperature, the spans' H their own; against the simplified model's
    // clipping offsets (clipped in 20 K warmer), printed.
    let options = |clip: Option<f64>| crate::sagging::SaggingOptions {
        clip_temperature: clip,
        ..Default::default()
    };
    let budget = default_fe_budget();
    let table = |l: &Line, clip| {
        crate::sagging::sagging_table(l, &l.sections[0], cable, &options(clip), &budget).unwrap()
    };
    let same = table(&sheaves, None);
    for s in &same.spans {
        assert!(s.clip_offset.abs() < 1e-9, "{}", s.clip_offset);
    }
    let (x, y) = (table(&sheaves, Some(35.0)), table(&equal, Some(35.0)));
    let mut doff: f64 = 0.0;
    for (k, (a, b)) in x.spans.iter().zip(&y.spans).enumerate() {
        println!(
            "sagging at 15, clipped at 35: span {}: H {:.1} vs {:.1} N, sag {:.4} vs {:.4} m, \
             offset {:+.4} vs {:+.4} m",
            k + 1,
            a.horizontal_tension,
            b.horizontal_tension,
            a.sag,
            b.sag,
            a.clip_offset,
            b.clip_offset
        );
        doff = doff.max((a.clip_offset - b.clip_offset).abs());
    }
    println!("largest change of a clipping offset: {:.1} mm", doff * 1e3);
}
