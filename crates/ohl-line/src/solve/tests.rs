// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the section solver: shapes, strings, bundles, stringing, the
//! models of the cable.

use super::*;
use ohl_core::units::{Length, Stress, Temperature};
use std::path::Path;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

#[test]
fn synthetic_line_solves_and_shapes_end_on_clamps() {
    let line = crate::tests::example();
    let case = WeatherCase::still("15 °C", Temperature::from_celsius(15.0));
    for result in solve_line(&line, &library(), &case) {
        let shape = result.unwrap();
        for (s, span) in shape.spans.iter().enumerate() {
            let (first, last) = (v(span.points[0]), v(*span.points.last().unwrap()));
            assert!((first - v(shape.clamps[s])).norm() < 1e-6);
            assert!((last - v(shape.clamps[s + 1])).norm() < 1e-6);
            assert!(span.sag > 0.0);
        }
    }
}

#[test]
fn heat_increases_sag_and_wind_blows_out() {
    let line = crate::tests::example();
    let lib = library();
    let cold = solve_line(
        &line,
        &lib,
        &WeatherCase::still("0", Temperature::from_celsius(0.0)),
    );
    let hot = solve_line(
        &line,
        &lib,
        &WeatherCase::still("80", Temperature::from_celsius(80.0)),
    );
    let sag = |r: &Result<SectionShape, LineError>| r.as_ref().unwrap().spans[0].sag;
    assert!(sag(&hot[0]) > sag(&cold[0]));

    let mut windy = WeatherCase::still("wind", Temperature::from_celsius(15.0));
    windy.wind_pressure = Stress::from_pascals(500.0);
    let calm = solve_line(
        &line,
        &lib,
        &WeatherCase::still("calm", Temperature::from_celsius(15.0)),
    );
    let blown = solve_line(&line, &lib, &windy);
    let mid = |r: &Result<SectionShape, LineError>| v(r.as_ref().unwrap().spans[0].mid_point);
    let shift = (mid(&blown[0]) - mid(&calm[0])).xy().norm();
    assert!(
        shift > 1.0,
        "wind should blow the wire sideways, moved {shift} m"
    );
    // From the right (default) to the left of the span; from the left,
    // the same amount to the right.
    let first = line.sections[0].points[..2]
        .iter()
        .map(|p| v(line.wire_point(p).unwrap().position))
        .collect::<Vec<_>>();
    let ahead = (first[1] - first[0]).xy();
    let left_of = |d: Vector3<f64>| ahead.x * d.y - ahead.y * d.x;
    let moved = mid(&blown[0]) - mid(&calm[0]);
    assert!(left_of(moved) > 0.0, "from the right: blown to the left");
    windy.wind_from = ohl_weather::WindSide::Left;
    let other = solve_line(&line, &lib, &windy);
    let back = mid(&other[0]) - mid(&calm[0]);
    assert!(left_of(back) < 0.0, "from the left: blown to the right");
    // Not the same amount: at a line angle the strings already lean one
    // way (6.26 m against 5.80 m here).
    assert!(back.xy().norm() > 1.0);
    let _ = Length::ZERO;
}

#[test]
fn a_bundle_shares_its_strings_weight() {
    // A 3-bundle under 3 × 900 N strings swings like one wire under
    // 900 N; the reported string forces are the whole bundle's.
    let lib = library();
    let mut windy = WeatherCase::still("wind", Temperature::from_celsius(15.0));
    windy.wind_pressure = Stress::from_pascals(400.0);
    let mut single = crate::tests::example();
    // Jumpers hang their weight on the clamps after stringing.
    single.jumpers.clear();
    let mut bundled = single.clone();
    bundled.sections[0].bundle = 3;
    bundled.sections[0].bundle_spacing = Some(0.4);
    for s in &mut bundled.structures {
        for a in &mut s.strings {
            a.weight *= 3.0;
        }
    }
    // Point loads are on the whole bundle too.
    for o in bundled.sections[0].objects_mut() {
        o.weight *= 3.0;
    }
    bundled.validate().unwrap();
    let a = solve_line(&single, &lib, &windy).remove(0).unwrap();
    let b = solve_line(&bundled, &lib, &windy).remove(0).unwrap();
    for (p, q) in a.clamps.iter().zip(&b.clamps) {
        assert!((v(*p) - v(*q)).norm() < 1e-9, "{p:?} {q:?}");
    }
    for (f, g) in a.strings.iter().flatten().zip(b.strings.iter().flatten()) {
        let (f, g) = (f.force, g.force);
        assert!((3.0 * f - g).abs() < 1e-6 * g.abs().max(1.0), "{f} {g}");
    }
    assert!(a.strings.iter().flatten().count() > 0);
}

#[test]
fn strings_catch_wind_and_bundles_share_point_loads() {
    let lib = library();
    let still = WeatherCase::still("still", Temperature::from_celsius(15.0));
    let mut windy = still.clone();
    windy.wind_pressure = Stress::from_pascals(400.0);
    let mut line = crate::tests::example();
    // Jumpers hang their weight on the clamps after stringing.
    line.jumpers.clear();
    assert!(line.sections[0].objects().next().is_some());
    // Wind on the strings swings the clamps further.
    let mut catching = line.clone();
    for s in &mut catching.structures {
        for a in &mut s.strings {
            a.wind_area = 0.5;
        }
    }
    let rest = solve_line(&line, &lib, &still).remove(0).unwrap();
    let a = solve_line(&line, &lib, &windy).remove(0).unwrap();
    let b = solve_line(&catching, &lib, &windy).remove(0).unwrap();
    let n = rest.clamps.len();
    let mut further = 0;
    for k in 1..n - 1 {
        let (r, p, q) = (v(rest.clamps[k]), v(a.clamps[k]), v(b.clamps[k]));
        assert!((q - r).norm() >= (p - r).norm() - 1e-9, "clamp {k}");
        further += usize::from((q - r).norm() > (p - r).norm() + 1e-4);
    }
    assert!(further > 0);
    // A 3-bundle under 3 × each point load hangs like one wire.
    let mut bundled = line.clone();
    bundled.sections[0].bundle = 3;
    bundled.sections[0].bundle_spacing = Some(0.4);
    for o in bundled.sections[0].objects_mut() {
        o.weight *= 3.0;
    }
    for s in &mut bundled.structures {
        for a in &mut s.strings {
            a.weight *= 3.0;
        }
    }
    let c = solve_line(&bundled, &lib, &still).remove(0).unwrap();
    for (p, q) in rest.clamps.iter().zip(&c.clamps) {
        assert!((v(*p) - v(*q)).norm() < 1e-6, "{p:?} {q:?}");
    }
}

/// S1's marker ball, given a size, swings its span further in the
/// wind and sags it more under ice; in still air it changes nothing.
#[test]
fn a_sized_marker_ball_catches_wind_and_ice() {
    let lib = library();
    let still = WeatherCase::still("still", Temperature::from_celsius(0.0));
    let windy = WeatherCase {
        wind_pressure: Stress::from_pascals(650.0),
        ..still.clone()
    };
    let icy = WeatherCase {
        ice_thickness: Length::from_millimeters(15.0),
        ..still.clone()
    };
    let line = crate::tests::example();
    let span = line.sections[0].objects().next().unwrap().0.span;
    let mut sized = line.clone();
    sized.sections[0].objects_mut().next().unwrap().shape = crate::Shape::Sphere { diameter: 0.6 };
    let solve = |l: &Line, c: &WeatherCase| solve_line(l, &lib, c).remove(0).unwrap();
    let mid = |s: &SectionShape| v(s.spans[span].mid_point);
    let rest = mid(&solve(&line, &still));
    assert!((mid(&solve(&sized, &still)) - rest).norm() < 1e-9);
    let (a, b) = (mid(&solve(&line, &windy)), mid(&solve(&sized, &windy)));
    assert!(
        (b - rest).xy().norm() > (a - rest).xy().norm() + 0.01,
        "{a} {b}"
    );
    let (a, b) = (mid(&solve(&line, &icy)), mid(&solve(&sized, &icy)));
    assert!(b.z < a.z - 0.01, "{a} {b}");
}

/// The synthetic line with T2's string at set 1 (a 3.5 m I-string,
/// line along x) replaced by a V-string: two strings from points ±3 m
/// across the line (`VL`, `VR`), 500 N each.
fn with_v_string() -> Line {
    let mut line = crate::tests::example();
    // Jumpers hang their weight on the clamps after stringing.
    line.jumpers.clear();
    let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
    let wire = t2.wire_at_set(1).unwrap().label.clone();
    let i = t2.strings.iter().position(|s| s.wire == wire).unwrap();
    let c = t2.node(&t2.strings[i].top).unwrap().position();
    t2.strings.remove(i);
    for (label, dy) in [("VL", -3.0), ("VR", 3.0)] {
        t2.points.push(crate::StructurePoint {
            description: Default::default(),
            label: label.into(),
            position: [c[0], c[1] + dy, c[2]],
            ..Default::default()
        });
        t2.strings.push(crate::InsulatorString {
            label: format!("leg {label}"),
            top: label.into(),
            wire: wire.clone(),
            weight: 500.0,
            ..Default::default()
        });
    }
    line.sections[0].clear_point_loads();
    line.validate().unwrap();
    line
}

#[test]
fn v_string_holds_the_clamp_against_wind_until_a_leg_goes_slack() {
    let lib = library();
    let case = |pa: f64| {
        let mut c = WeatherCase::still("wind", Temperature::from_celsius(15.0));
        c.wind_pressure = Stress::from_pascals(pa);
        c
    };
    let solve = |line: &Line, pa: f64| solve_line(line, &lib, &case(pa)).remove(0).unwrap();
    // Movement of T2's clamp across the line (y; the line runs along x).
    let across = |a: &SectionShape, b: &SectionShape| (a.clamps[1][1] - b.clamps[1][1]).abs();

    let v_line = with_v_string();
    let calm = solve(&v_line, 0.0);
    // Still air: both legs in tension, carrying the weight span.
    let forces =
        |s: &SectionShape, k: usize| -> Vec<f64> { s.strings[k].iter().map(|x| x.force).collect() };
    assert_eq!(calm.strings[1].len(), 2);
    assert!(
        forces(&calm, 1).iter().all(|&f| f > 1000.0),
        "{:?}",
        forces(&calm, 1)
    );
    // The section's end: a strain assembly (one member) or a fixed point.
    assert!(calm.strings[0].len() <= 1);
    // Strung at 25 kN with the V clamped plumb: 25 kN at stringing.
    for span in &calm.spans {
        assert!(
            (span.horizontal_tension - 25_000.0).abs() < 25.0,
            "{}",
            span.horizontal_tension
        );
    }

    // 400 Pa: both legs still in tension, the clamp stays put across the
    // line (< 1 mm: only the leg stretch), while an I-string swings.
    let blown = solve(&v_line, 400.0);
    assert!(
        forces(&blown, 1).iter().all(|&f| f > 0.0),
        "{:?}",
        forces(&blown, 1)
    );
    let mut i_line = crate::tests::example();
    // Jumpers hang their weight on the clamps after stringing.
    i_line.jumpers.clear();
    i_line.sections[0].clear_point_loads();
    let i_across = across(&solve(&i_line, 400.0), &solve(&i_line, 0.0));
    let v_across = across(&blown, &calm);
    assert!(
        v_across < 1e-3 && i_across > 1.0,
        "V {v_across} m, I {i_across} m"
    );

    // 800 Pa: the load leans further than the windward leg (40.6° from
    // vertical), the leeward leg goes slack and the clamp swings about the
    // windward leg's top — out and up on a circle of the leg length.
    let gale = solve(&v_line, 800.0);
    let legs = &gale.strings[1];
    assert!(
        legs.iter().any(|l| l.slack && l.force == 0.0) && legs.iter().any(|l| l.force > 1000.0),
        "{legs:?}"
    );
    assert!(across(&gale, &calm) > 0.1);
    let taut = legs.iter().find(|l| !l.slack).unwrap();
    let windward = v(taut.top);
    let radius = (v(gale.clamps[1]) - windward).norm();
    assert!(
        (radius - (3.0f64.powi(2) + 3.5f64.powi(2)).sqrt()).abs() < 2e-3,
        "{radius}"
    );
}

/// The synthetic line with T2's string at set 1 replaced by a V
/// (points `VL`, `VR` ±3 m across the line, at the string's top) to a
/// tip `V` 0.5 m above the clamp, and a 0.5 m string from the tip down
/// to the clamp: a V-string with a string below.
fn with_v_and_string_below() -> Line {
    let mut line = crate::tests::example();
    let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
    let wire = t2.wire_at_set(1).unwrap().clone();
    let i = t2
        .strings
        .iter()
        .position(|s| s.wire == wire.label)
        .unwrap();
    let c = t2.node(&t2.strings[i].top).unwrap().position();
    t2.strings.remove(i);
    let w = wire.position;
    t2.wires.push(crate::WirePoint {
        label: "V".into(),
        position: [w[0], w[1], w[2] + 0.5],
        ..Default::default()
    });
    for (label, dy) in [("VL", -3.0), ("VR", 3.0)] {
        t2.points.push(crate::StructurePoint {
            description: Default::default(),
            label: label.into(),
            position: [c[0], c[1] + dy, c[2]],
            ..Default::default()
        });
        t2.strings.push(crate::InsulatorString {
            label: format!("leg {label}"),
            top: label.into(),
            wire: "V".into(),
            weight: 400.0,
            wind_area: 0.2,
            ..Default::default()
        });
    }
    t2.strings.push(crate::InsulatorString {
        label: "below".into(),
        top: "V".into(),
        wire: wire.label.clone(),
        weight: 100.0,
        ..Default::default()
    });
    line.sections[0].clear_point_loads();
    line.validate().unwrap();
    line
}

/// A V with a string below: all three strings carry the wire in still
/// air; in the wind the V's tip stays put across the line while the
/// string below swings; with the section clipped in, a heavy load in
/// one span pulls the clamp along the line (the V swings about its
/// tops) — a clamp held by a short string alone would hardly move.
#[test]
fn a_string_hanging_from_a_v_swings_below_it() {
    let lib = library();
    let case = |pa: f64| WeatherCase {
        wind_pressure: Stress::from_pascals(pa),
        ..WeatherCase::still("wind", Temperature::from_celsius(15.0))
    };
    let line = with_v_and_string_below();
    let solve = |line: &Line, pa: f64| solve_line(line, &lib, &case(pa)).remove(0).unwrap();
    let calm = solve(&line, 0.0);
    let holder = &calm.strings[1];
    assert_eq!(holder.len(), 3);
    assert!(holder.iter().all(|s| s.force > 100.0), "{holder:?}");
    let tip = |s: &SectionShape| {
        v(s.strings[1]
            .iter()
            .find(|x| x.label == "below")
            .unwrap()
            .top)
    };
    // Hangs as given (the clamp 0.5 m below the tip).
    assert!(((tip(&calm) - v(calm.clamps[1])).norm() - 0.5).abs() < 1e-3);
    // A set on the tip that no section uses (an export's own number
    // for the V) leaves it free: the legs carry the wire.
    let mut numbered = line.clone();
    let t2 = numbered
        .structures
        .iter_mut()
        .find(|s| s.id == "T2")
        .unwrap();
    t2.wires.iter_mut().find(|w| w.label == "V").unwrap().set = Some(99);
    numbered.validate().unwrap();
    let n = solve(&numbered, 0.0);
    assert!(
        n.strings[1].iter().all(|s| s.force > 100.0),
        "{:?}",
        n.strings[1]
    );
    assert!((v(n.clamps[1]) - v(calm.clamps[1])).norm() < 1e-6);
    let blown = solve(&line, 400.0);
    let across = |a: Vector3<f64>, b: Vector3<f64>| (a.y - b.y).abs();
    assert!(
        across(tip(&blown), tip(&calm)) < 2e-3,
        "the V holds its tip"
    );
    let swing = across(v(blown.clamps[1]), v(calm.clamps[1]));
    assert!(
        swing > 0.05 && swing < 0.5,
        "the string below swings {swing} m"
    );

    // Clipped in, then a 3 kN load in the middle of the first span.
    let mut clipped = line.clone();
    let cable = lib.get(&clipped.sections[0].cable).unwrap();
    let (lengths, temperature) = clip_lengths(&clipped, &clipped.sections[0], cable).unwrap();
    clipped.sections[0].stringing = Stringing::UnstressedLengths {
        lengths,
        temperature,
        sagged: None,
    };
    let before = solve(&clipped, 0.0);
    clipped.sections[0].add_point_load(&crate::legacy::SpanLoad {
        span: 0,
        fraction: 0.5,
        vertical: 3000.0,
        label: "load".into(),
        ..Default::default()
    });
    let after = solve(&clipped, 0.0);
    let along = (v(after.clamps[1]) - v(before.clamps[1])).x;
    assert!(along < -0.05, "pulled towards the loaded span: {along} m");
}

/// Above the wire's strength (owner, 2026-10-22): a span whose tension
/// at its most tensioned support exceeds the wire type's RTS is warned
/// about, by finite elements and by the ruling span; the same section
/// with the real RTS is not.
#[test]
fn a_tension_above_the_rts_is_warned_in_every_solver() {
    let mut lib = library();
    // An invented weak copy of the example's wire: RTS 5 kN.
    lib.add_toml(
            Path::new("weak.ohlw"),
            "format = \"ohl-wire\"\nversion = 1\n[[cable]]\nid = \"weak\"\nname = \"weak\"\n\
             kind = \"acsr\"\ndiameter = \"1.108 in\"\nmass_per_length = \"1.094 lbm/ft\"\n\
             rated_tensile_strength = \"5 kN\"\n\
             outer = { area = \"0.6245 in²\", modulus = \"55 GPa\", thermal_expansion = 23.0e-6, strands = 26 }\n\
             core = { area = \"0.1017 in²\", modulus = \"190 GPa\", thermal_expansion = 11.5e-6, strands = 7 }\n",
        )
        .unwrap();
    let still = WeatherCase::still("still", Temperature::from_celsius(15.0));
    let line = crate::tests::example();
    let strong = solve_line(&line, &lib, &still).remove(0).unwrap();
    assert!(!above_rts(&strong), "{:?}", strong.flags);
    for rs in [false, true] {
        let mut weak = line.clone();
        weak.sections[0].cable = "weak".into();
        if rs {
            weak.sections[0].calculation = crate::condition::Calculation::INITIAL_RS;
        }
        let shape = solve_line(&weak, &lib, &still).remove(0).unwrap();
        assert!(above_rts(&shape), "rs {rs}: {:?}", shape.flags);
        let over: Vec<&String> = shape
            .flags
            .iter()
            .filter(|f| f.contains(RTS_WARNING))
            .collect();
        // Every span is above 5 kN.
        assert_eq!(over.len(), shape.spans.len(), "{over:?}");
        let t = shape.spans[0].max_tension;
        assert!(t > 5000.0);
        assert!(
            over[0].starts_with(&format!("span 1: tension {:.2} kN", t / 1000.0)),
            "{}",
            over[0]
        );
        assert!(
            over[0].contains(&format!("{:.1} %", t / 50.0)),
            "{}",
            over[0]
        );
    }
}

/// A string's limits (not checked) come with its solved force and
/// swing: a swing beyond its maximum and a load beyond its rating flag
/// nothing; the swing follows from its top and wire point.
#[test]
fn string_limits_are_reported_not_checked() {
    let lib = library();
    let windy = WeatherCase {
        wind_pressure: Stress::from_pascals(500.0),
        ..WeatherCase::still("wind", Temperature::from_celsius(15.0))
    };
    let mut line = crate::tests::example();
    line.sections[0].clear_point_loads();
    let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
    let wire = t2
        .wires
        .iter()
        .find(|w| w.set == Some(1))
        .unwrap()
        .label
        .clone();
    let label = t2.strings_to(&wire).next().unwrap().label.clone();
    let limits = crate::StringLimits {
        max_load: Some(1.0),
        max_swing_deg: Some(0.5),
        max_swing_across_deg: None,
        max_swing_along_deg: Some(0.1),
    };
    t2.strings
        .iter_mut()
        .find(|x| x.label == label)
        .unwrap()
        .set_limits(limits);
    line.validate().unwrap();
    let shape = solve_line(&line, &lib, &windy).remove(0).unwrap();
    let x = shape.strings[1].iter().find(|x| x.label == label).unwrap();
    assert_eq!(x.limits, limits);
    assert!(x.force > 1.0 && x.swing.total_deg > 0.5, "{x:?}");
    assert!(shape.flags.is_empty(), "nothing flagged: {:?}", shape.flags);
    let o = line.structure("T2").unwrap().orientation_deg;
    let again = crate::swing(o, x.top, x.bottom);
    assert_eq!(again, x.swing);
    // Its parts: tan² total = tan² across + tan² along.
    let t = |d: f64| d.to_radians().tan().powi(2);
    assert!(
        (t(x.swing.total_deg) - t(x.swing.across_deg) - t(x.swing.along_deg)).abs() < 1e-9,
        "{:?}",
        x.swing
    );
    // Another string: no limits given.
    assert!(shape.strings[0].iter().all(|x| x.limits.is_empty()));
}

/// The swing of a string: across the line positive to the right,
/// along it positive ahead, on a structure turned any way.
#[test]
fn swing_angles_in_the_structures_axes() {
    // Local x (right) north: right of the line is +y global.
    let s = crate::swing(90.0, [0.0, 0.0, 10.0], [0.0, 1.0, 9.0]);
    assert!((s.total_deg - 45.0).abs() < 1e-9, "{s:?}");
    assert!((s.across_deg - 45.0).abs() < 1e-9, "{s:?}");
    assert!(s.along_deg.abs() < 1e-9, "{s:?}");
    // Ahead (local y) is west then.
    let s = crate::swing(90.0, [0.0, 0.0, 10.0], [-1.0, 0.0, 9.0]);
    assert!((s.along_deg - 45.0).abs() < 1e-9 && s.across_deg.abs() < 1e-9);
    let s = crate::swing(0.0, [0.0, 0.0, 10.0], [-1.0, 0.0, 10.0 - 3f64.sqrt()]);
    assert!((s.across_deg + 30.0).abs() < 1e-9, "{s:?}");
}

/// A counterweight at a suspension clamp: the string swings less in
/// the wind, the string carries the weight more.
#[test]
fn a_counterweight_holds_the_clamp_down() {
    let lib = library();
    let windy = WeatherCase {
        wind_pressure: Stress::from_pascals(500.0),
        ..WeatherCase::still("wind", Temperature::from_celsius(15.0))
    };
    let mut line = crate::tests::example();
    line.sections[0].clear_point_loads();
    let plain = solve_line(&line, &lib, &windy).remove(0).unwrap();
    let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
    let w = t2.wires.iter_mut().find(|w| w.set == Some(1)).unwrap();
    w.weight = 3000.0;
    line.validate().unwrap();
    let held = solve_line(&line, &lib, &windy).remove(0).unwrap();
    let swing = |s: &SectionShape| {
        let top = v(s.strings[1][0].top);
        let d = v(s.clamps[1]) - top;
        d.xy().norm() / -d.z
    };
    assert!(
        swing(&held) < 0.8 * swing(&plain),
        "{} {}",
        swing(&held),
        swing(&plain)
    );
    assert!(held.strings[1][0].force > plain.strings[1][0].force + 2400.0);
}

/// V legs that may take some compression: the leeward leg pushed in a
/// gale holds the clamp (less swing than a tension-only V) and is
/// flagged when pushed beyond what it may take.
#[test]
fn a_v_leg_may_take_compression_and_is_flagged_beyond_it() {
    let lib = library();
    let gale = WeatherCase {
        wind_pressure: Stress::from_pascals(800.0),
        ..WeatherCase::still("gale", Temperature::from_celsius(15.0))
    };
    let tension_only = with_v_string();
    let with = |limit: f64| {
        let mut line = with_v_string();
        let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        for s in t2.strings.iter_mut().filter(|s| s.label.starts_with("leg")) {
            s.compression = Some(limit);
        }
        solve_line(&line, &lib, &gale).remove(0).unwrap()
    };
    let free = solve_line(&tension_only, &lib, &gale).remove(0).unwrap();
    let strut = with(1e9);
    let pushed = strut.strings[1]
        .iter()
        .map(|s| s.force)
        .fold(f64::INFINITY, f64::min);
    assert!(pushed < -10.0, "a leg in compression: {pushed} N");
    assert!(strut.flags.is_empty(), "{:?}", strut.flags);
    let swing = |s: &SectionShape| (s.clamps[1][1] - free.clamps[1][1]).abs();
    assert!(swing(&strut) > 0.05, "the strut holds the clamp back");
    let weak = with(-pushed / 2.0);
    assert!(weak.strings[1].iter().any(|s| s.over));
    assert_eq!(weak.flags.len(), 1, "{:?}", weak.flags);
    assert!(weak.flags[0].contains("compression"));
}

/// A TAK: a suspension string from the structure down to a node, two
/// strain strings from that node to the ends of two sections. The
/// sections pull it towards the tighter one; solved together they agree
/// on where it is, and differ from either solved alone with the other's
/// clamp held.
#[test]
fn a_tak_couples_the_sections_on_it() {
    let cable = crate::tests::example().sections[0].cable.clone();
    let text = format!(
        r#"{{
            "name": "tak", "structures": [
              {{"id": "A", "position": [0, 0, 0], "wires": [{{"label": "W", "position": [0, 0, 40], "set": 1}}]}},
              {{"id": "B", "position": [300, 0, 0],
                "points": [{{"label": "K", "position": [300, 0, 45]}}],
                "wires": [
                  {{"label": "N", "position": [300, 0, 40]}},
                  {{"label": "EB", "position": [297, 0, 40], "set": 1}},
                  {{"label": "EF", "position": [303, 0, 40], "set": 2}}],
                "strings": [
                  {{"label": "k", "top": "K", "wire": "N", "weight": 500}},
                  {{"label": "b", "top": "N", "wire": "EB", "weight": 300}},
                  {{"label": "f", "top": "N", "wire": "EF", "weight": 300}}]}},
              {{"id": "C", "position": [600, 0, 0], "wires": [{{"label": "W", "position": [600, 0, 40], "set": 1}}]}}],
            "sections": [
              {{"id": "X", "cable": "{cable}", "points": [{{"structure": "A", "set": 1}}, {{"structure": "B", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 20000, "temperature": 15}}}},
              {{"id": "Y", "cable": "{cable}", "points": [{{"structure": "B", "set": 2}}, {{"structure": "C", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 16000, "temperature": 15}}}}]
            }}"#
    );
    let line = Line::from_json(&text).unwrap();
    assert_eq!(coupled_groups(&line), vec![vec![0, 1]]);
    let lib = library();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let node = |s: &SectionShape, k: usize| {
        v(s.strings[k].iter().find(|x| x.label == "k").unwrap().bottom)
    };
    let budget = default_fe_budget();
    let alone: Vec<SectionShape> = line
        .sections
        .iter()
        .map(|s| solve_section(&line, s, lib.get(&cable).unwrap(), &case, &budget).unwrap())
        .collect();
    let together: Vec<SectionShape> = solve_line(&line, &lib, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect();
    // X (first) solves the TAK, Y's wires pulling on its end; Y holds
    // its end where X puts it.
    let x = node(&together[0], 1);
    let end_of_y = v(together[0].strings[1]
        .iter()
        .find(|s| s.label == "f")
        .unwrap()
        .bottom);
    assert!((end_of_y - v(together[1].clamps[0])).norm() < 1e-9);
    assert!(together[1].strings[0].is_empty());
    // Pulled towards the tighter section X (towards A, −x), the strain
    // strings in line with the sections.
    assert!(x.x < 300.0 - 0.05, "{x}");
    assert!((node(&alone[0], 1) - x).norm() > 0.01);
    // In balance: X's model pulls Y's end as hard as Y's wires do.
    let pull_y = v(together[1].pulls[0]);
    let f = together[0].strings[1]
        .iter()
        .find(|s| s.label == "f")
        .unwrap();
    let along_f = (v(f.bottom) - v(f.top)).normalize();
    assert!(
        (pull_y.dot(&along_f) - f.force).abs() < 0.01 * f.force + 50.0,
        "{} vs {}",
        pull_y.dot(&along_f),
        f.force
    );
    // The structure's loads: once, by X.
    assert!(!together[0].attachment_loads[1].is_empty());
    assert!(together[1].attachment_loads[0].is_empty());
}

/// A TAK between a long section (four 400 m spans on suspension
/// strings) and a short taut span (80 m), strung 5 kN harder: the
/// short span pulls ~80 times harder for a move of its end than the
/// long one and the TAK's string hold it back. Held where the long
/// section put the TAK, its pull overshot in turn (the passes never
/// settled, as on the second existing line); the short span solves the
/// TAK, the long one holds its end there.
#[test]
fn a_tak_between_a_long_section_and_a_short_taut_span_settles() {
    let cable = crate::tests::example().sections[0].cable.clone();
    let suspension = |id: &str, x: f64| {
        format!(
            r#"{{"id": "{id}", "position": [{x}, 0, 0],
                    "points": [{{"label": "P", "position": [{x}, 0, 45]}}],
                    "wires": [{{"label": "W", "position": [{x}, 0, 42], "set": 1}}],
                    "strings": [{{"label": "s", "top": "P", "wire": "W", "weight": 300}}]}}"#
        )
    };
    let text = format!(
        r#"{{
            "name": "tak", "structures": [
              {{"id": "A", "position": [0, 0, 0], "wires": [{{"label": "W", "position": [0, 0, 42], "set": 1}}]}},
              {s1}, {s2}, {s3},
              {{"id": "B", "position": [1600, 0, 0],
                "points": [{{"label": "K", "position": [1600, 0, 45]}}],
                "wires": [
                  {{"label": "N", "position": [1600, 0, 42]}},
                  {{"label": "EB", "position": [1599.7, 0, 42], "set": 1}},
                  {{"label": "EF", "position": [1600.3, 0, 42], "set": 2}}],
                "strings": [
                  {{"label": "k", "top": "K", "wire": "N", "weight": 820}},
                  {{"label": "b", "top": "N", "wire": "EB"}},
                  {{"label": "f", "top": "N", "wire": "EF"}}]}},
              {{"id": "C", "position": [1680, 0, 0], "wires": [{{"label": "W", "position": [1680, 0, 42], "set": 1}}]}}],
            "sections": [
              {{"id": "X", "cable": "{cable}", "points": [{{"structure": "A", "set": 1}}, {{"structure": "S1", "set": 1}},
                {{"structure": "S2", "set": 1}}, {{"structure": "S3", "set": 1}}, {{"structure": "B", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 20000, "temperature": 15}}}},
              {{"id": "Y", "cable": "{cable}", "points": [{{"structure": "B", "set": 2}}, {{"structure": "C", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 25000, "temperature": 15}}}}]
            }}"#,
        s1 = suspension("S1", 400.0),
        s2 = suspension("S2", 800.0),
        s3 = suspension("S3", 1200.0),
    );
    let line = Line::from_json(&text).unwrap();
    assert_eq!(coupled_groups(&line), vec![vec![0, 1]]);
    let lib = library();
    for t in [-20.0, 15.0, 60.0] {
        let case = WeatherCase::still("t", Temperature::from_celsius(t));
        let together: Vec<SectionShape> = solve_line(&line, &lib, &case)
            .into_iter()
            .map(|r| r.unwrap_or_else(|e| panic!("{t} °C: {e}")))
            .collect();
        // Y, the stiffer, solves the TAK and reports its loads; X holds
        // its end where Y puts it (as Y's last pass but one put it:
        // within what X's pull changed by then, 0.1 N).
        let end_of_x = together[1].strings[0]
            .iter()
            .find(|s| s.label == "b")
            .map(|s| v(s.bottom))
            .expect("Y solves the TAK");
        assert!((end_of_x - v(together[0].clamps[4])).norm() < 1e-4);
        assert!(together[0].attachment_loads[4].is_empty());
        assert!(!together[1].attachment_loads[0].is_empty());
        // In balance: Y's model pulls X's end as hard as X's wires do.
        let pull_x = v(together[0].pulls[4]);
        let b = together[1].strings[0]
            .iter()
            .find(|s| s.label == "b")
            .unwrap();
        let along_b = (v(b.bottom) - v(b.top)).normalize();
        assert!(
            (pull_x.dot(&along_b) - b.force).abs() < 1e-3 * b.force + 1.0,
            "{t} °C: {} vs {}",
            pull_x.dot(&along_b),
            b.force
        );
    }
}

#[test]
fn tension_stringing_reproduces_its_tension() {
    let mut line = crate::tests::example();
    // Jumpers hang their weight on the clamps after stringing.
    line.jumpers.clear();
    // Strung at 25 kN at 15 °C: solving at 15 °C gives 25 kN in every span,
    // within 0.1 %: sagging holds the clamps plumb along the line, the
    // solved state lets them settle longitudinally (6 N observed).
    line.sections[0].stringing = Stringing::Tension {
        horizontal_tension: 25_000.0,
        temperature: 15.0,
    };
    line.sections[0].clear_point_loads();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let shape = solve_line(&line, &library(), &case).remove(0).unwrap();
    for span in &shape.spans {
        assert!(
            (span.horizontal_tension - 25_000.0).abs() < 25.0,
            "{}",
            span.horizontal_tension
        );
    }
}

/// Sagged to 8 m in its second span: solving in the stringing state
/// gives that sag there, and the same tension in every span.
#[test]
fn sag_stringing_reproduces_its_sag() {
    let mut line = crate::tests::example();
    // Jumpers hang their weight on the clamps after stringing.
    line.jumpers.clear();
    line.sections[0].stringing = Stringing::Sag {
        sag: 8.0,
        temperature: 15.0,
        span: Some(1),
    };
    line.sections[0].clear_point_loads();
    let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let shape = solve_line(&line, &library(), &case).remove(0).unwrap();
    assert!(
        (shape.spans[1].sag - 8.0).abs() < 0.02,
        "{}",
        shape.spans[1].sag
    );
    let h0 = shape.spans[0].horizontal_tension;
    for span in &shape.spans {
        assert!((span.horizontal_tension - h0).abs() < 0.002 * h0);
    }
}

/// In the blocks the spans keep one tension in every weather case;
/// clipped in, with the same lengths at stringing, they do not under
/// ice; a structure moved changes the tension of a clipped section, not
/// the stringing tension of one in the blocks.
#[test]
fn sections_in_the_blocks_roll_clipped_ones_do_not() {
    let lib = library();
    let mut line = crate::tests::example();
    line.sections[0].clear_point_loads();
    line.sections[0].stringing = Stringing::Tension {
        horizontal_tension: 25_000.0,
        temperature: 15.0,
    };
    let cable = lib.get(&line.sections[0].cable).unwrap().clone();
    let (lengths, temperature) = clip_lengths(&line, &line.sections[0], &cable).unwrap();
    let mut clipped = line.clone();
    clipped.sections[0].stringing = Stringing::UnstressedLengths {
        lengths,
        temperature,
        sagged: Some(Box::new(line.sections[0].stringing.clone())),
    };
    let still = WeatherCase::still("15", Temperature::from_celsius(15.0));
    let mut iced = WeatherCase::still("ice", Temperature::from_celsius(-5.0));
    iced.ice_thickness = ohl_core::units::Length::from_si(0.02);
    let spread = |l: &Line, c: &WeatherCase| {
        let shape = solve_line(l, &lib, c).remove(0).unwrap();
        let h: Vec<f64> = shape.spans.iter().map(|s| s.horizontal_tension).collect();
        let (lo, hi) = h
            .iter()
            .fold((f64::MAX, 0.0f64), |(a, b), x| (a.min(*x), b.max(*x)));
        (lo, hi)
    };
    // At stringing both are the same.
    let (a, b) = spread(&line, &still);
    let (c, d) = spread(&clipped, &still);
    assert!(
        (a - c).abs() < 30.0 && (b - d).abs() < 30.0,
        "{a} {b} {c} {d}"
    );
    // Iced: one tension rolling; clipped, the strings swing to balance
    // the spans but not fully (0.1 % here).
    let (lo, hi) = spread(&line, &iced);
    let (clo, chi) = spread(&clipped, &iced);
    println!("iced: rolling {lo}–{hi}, clipped {clo}–{chi}");
    assert!(
        hi - lo < 0.2 * (chi - clo),
        "rolling {lo}–{hi}, clipped {clo}–{chi}"
    );
    assert!(chi - clo > 0.0005 * chi, "clipped {clo}–{chi}");
    // T3 moved 20 m along the line.
    let moved = |l: &Line| {
        let mut l = l.clone();
        let k = l.structures.iter().position(|s| s.id == "T3").unwrap();
        let (p, q) = (l.structures[k - 1].position, l.structures[k].position);
        let d = [q[0] - p[0], q[1] - p[1]];
        let n = d[0].hypot(d[1]);
        l.structures[k] =
            l.structures[k].moved_to([q[0] + 20.0 * d[0] / n, q[1] + 20.0 * d[1] / n, q[2]]);
        l
    };
    let (lo, hi) = spread(&moved(&line), &still);
    assert!((lo - 25_000.0).abs() < 50.0 && (hi - 25_000.0).abs() < 50.0);
    let (lo, hi) = spread(&moved(&clipped), &still);
    assert!(hi - lo > 500.0, "clipped after the move {lo}–{hi}");
}

/// The example line with S2's dead-end strings drooping 12° (as a
/// weight can hold them), given as geometry only (no weight).
fn with_drooping_dead_ends() -> Line {
    let mut line = crate::tests::example();
    let ends: Vec<crate::SectionPoint> = {
        let s2 = line.sections.iter().find(|s| s.id == "S2").unwrap();
        vec![s2.points[0].clone(), s2.points.last().unwrap().clone()]
    };
    for (end, drop) in ends.iter().zip([1.06, 1.07]) {
        let st = line
            .structures
            .iter_mut()
            .find(|s| s.id == end.structure)
            .unwrap();
        let wire = st.wire_at_set(end.set).unwrap().label.clone();
        let top = st.strings_to(&wire).next().unwrap().top.clone();
        let z = st.node(&top).unwrap().position()[2] - drop;
        st.wires
            .iter_mut()
            .find(|w| w.label == wire)
            .unwrap()
            .position[2] = z;
        // The string's geometry taken again from where it now ends.
        for x in st.strings.iter_mut().filter(|x| x.wire == wire) {
            x.length = 0.0;
        }
    }
    line.place_strings().unwrap();
    line
}

/// A strain assembly for the insulator library, of `weight` N.
fn strain_assembly(id: &str, weight: f64) -> ohl_structures::InsulatorAssembly {
    use ohl_core::units::{Area, Force, Length};
    ohl_structures::InsulatorAssembly {
        format: None,
        version: None,
        id: id.into(),
        name: id.into(),
        kind: ohl_structures::AssemblyKind::Strain,
        source: None,
        length: Length::from_meters(3.0),
        weight: Force::from_newtons(weight),
        axial_stiffness: Force::from_newtons(DEFAULT_INSULATOR_STIFFNESS),
        wind_area: Area::from_square_meters(0.0),
        half_width: None,
        max_load: None,
        max_swing: None,
        max_swing_across: None,
        max_swing_along: None,
        volumes: Default::default(),
    }
}

/// A strain string whose resolved weight is 0 counts as without a
/// weight whether or not it names an insulator: linked to a 0 N entry,
/// S2 solves exactly as unlinked (its weights estimated); an entry with
/// a real weight gives that weight (no estimate). Before 2026-10-21 the
/// 0 N link dropped the estimate (on a real line: median sag
/// differences 0.7–2.4 cm → 2–9 cm).
#[test]
fn a_zero_newton_insulator_keeps_the_estimated_weight() {
    let lib = library();
    let line = with_drooping_dead_ends();
    let s2 = line.sections.iter().find(|s| s.id == "S2").unwrap().clone();
    let cable = lib.get(&s2.cable).unwrap();
    let ends: Vec<(String, String)> = [&s2.points[0], s2.points.last().unwrap()]
        .iter()
        .map(|p| {
            let st = line.structure(&p.structure).unwrap();
            let wire = &st.wire_at_set(p.set).unwrap().label;
            (
                st.id.clone(),
                st.strings_to(wire).next().unwrap().label.clone(),
            )
        })
        .collect();
    let linked = |id: &str, weight: f64| {
        let mut insulators = ohl_structures::InsulatorLibrary::default();
        insulators
            .insert(Path::new(id), strain_assembly(id, weight))
            .unwrap();
        let mut l = line.clone();
        for (structure, label) in &ends {
            let st = l
                .structures
                .iter_mut()
                .find(|s| s.id == *structure)
                .unwrap();
            let x = st.strings.iter_mut().find(|x| x.label == *label).unwrap();
            assert_eq!(x.weight, 0.0);
            x.insulator = Some(id.into());
        }
        l.resolve(&ohl_structures::StructureLibrary::default(), &insulators)
            .unwrap()
    };
    let case = WeatherCase::still("80", Temperature::from_celsius(80.0));
    let solve = |l: &Line| solve_section(l, &s2, cable, &case, &default_fe_budget()).unwrap();
    let plain = solve(
        &line
            .resolve(
                &ohl_structures::StructureLibrary::default(),
                &ohl_structures::InsulatorLibrary::default(),
            )
            .unwrap(),
    );
    let n = plain.estimated_weights.len();
    assert!(plain.estimated_weights[0].is_some_and(|w| w > 0.0));
    assert!(plain.estimated_weights[n - 1].is_some_and(|w| w > 0.0));
    let zero = solve(&linked("Strain-3.000m-0N", 0.0));
    assert_eq!(zero.estimated_weights, plain.estimated_weights);
    assert_eq!(zero.clamps, plain.clamps);
    for (a, b) in zero.spans.iter().zip(&plain.spans) {
        assert_eq!(a.horizontal_tension, b.horizontal_tension);
        assert_eq!(a.sag, b.sag);
    }
    // A real weight overrides the estimate.
    let heavy = solve(&linked("Strain-heavy", 900.0));
    assert_eq!(heavy.estimated_weights[0], None);
    assert_eq!(heavy.estimated_weights[n - 1], None);
    assert!((heavy.clamps[0][2] - plain.clamps[0][2]).abs() > 1e-3);
}

/// A dead-end string given as geometry only (no weight): its weight is
/// estimated so that it stays where the data puts it in the stringing
/// state (S2: clipped in at 15 °C), and it moves with the weather.
#[test]
fn weightless_strain_strings_stay_put_at_stringing_and_move_with_the_weather() {
    let lib = library();
    // The example's dead-end strings droop less (5.7°) than the wire
    // pulls them down (8.4°): no weight holds them there (estimated 0;
    // freed, they drop 0.23 m). Drooping 12°, as a weight can hold them.
    let line = with_drooping_dead_ends();
    let s2 = line.sections.iter().find(|s| s.id == "S2").unwrap();
    let cable = lib.get(&s2.cable).unwrap();
    let solve =
        |case: &WeatherCase| solve_section(&line, s2, cable, case, &default_fe_budget()).unwrap();
    let data = v(line.wire_point(&s2.points[0]).unwrap().position);
    let at = |s: &SectionShape| v(s.clamps[0]);
    let stringing = solve(&WeatherCase::still("15", Temperature::from_celsius(15.0)));
    // Within a few mm: the member stretches under the wire's pull (1.6 mm
    // at 16 kN), and the data's string is not quite in line with it.
    assert!(
        (at(&stringing) - data).norm() < 5e-3,
        "{}",
        at(&stringing) - data
    );
    // A member: its force is about the conductor's tension.
    assert_eq!(stringing.strings[0].len(), 1);
    // Hot: less tension, the string droops; wind: it swings sideways.
    let hot = solve(&WeatherCase::still("80", Temperature::from_celsius(80.0)));
    assert!(
        at(&hot).z < at(&stringing).z - 0.005,
        "{}",
        at(&hot).z - at(&stringing).z
    );
    let mut windy = WeatherCase::still("wind", Temperature::from_celsius(15.0));
    windy.wind_pressure = Stress::from_pascals(600.0);
    let blown = solve(&windy);
    assert!((at(&blown) - at(&stringing)).xy().norm() > 0.01);
}

/// The synthetic non-linear test cable (invented curves, with creep).
fn with_nonlinear(mut lib: ohl_cables::Library) -> ohl_cables::Library {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ohl-sagtension/tests/data/synthetic-cable.ohlw");
    lib.add_toml(&path, &std::fs::read_to_string(&path).unwrap())
        .unwrap();
    lib
}

fn max_sag(s: &SectionShape) -> f64 {
    s.spans.iter().map(|x| x.sag).fold(0.0, f64::max)
}

#[test]
fn calculations_have_names() {
    use crate::condition::{Calculation, Condition, Method};
    for c in Calculation::ALL {
        assert_eq!(Calculation::parse(&c.label()), Some(c));
        assert_eq!(Calculation::parse(&c.name()), Some(c));
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Calculation>(&json).unwrap(), c);
    }
    assert_eq!(
        Calculation::parse("max sag fe"),
        Some(Calculation::MAX_SAG_FE)
    );
    assert_eq!(Calculation::CREEP_RS.condition(), Condition::Creep);
    assert_eq!(Calculation::CREEP_RS.method(), Method::Rs);
    assert_eq!(
        serde_json::to_string(&Calculation::CREEP_RS).unwrap(),
        "\"creep-rs\""
    );
    assert!(Calculation::default().is_default());
}

/// The loads on the structures balance the section: their sum is the
/// weight of the wires (and strings) hanging from them; one support per
/// string top, the wind pushes them sideways.
#[test]
fn attachment_loads_balance_the_section() {
    let lib = library();
    let mut line = crate::tests::example();
    line.sections[0].clear_point_loads();
    let cable = lib.get(&line.sections[0].cable).unwrap();
    let budget = default_fe_budget();
    let still = WeatherCase::still("t", Temperature::from_celsius(15.0));
    let shape = solve_section(&line, &line.sections[0], cable, &still, &budget).unwrap();
    assert_eq!(shape.attachment_loads.len(), line.sections[0].points.len());
    let bundle = f64::from(line.sections[0].bundle.max(1));
    let wires: f64 = shape
        .spans
        .iter()
        .map(|s| {
            s.points
                .windows(2)
                .map(|w| (v(w[1]) - v(w[0])).norm())
                .sum::<f64>()
        })
        .sum::<f64>()
        * cable.weight().si()
        * bundle;
    let down: f64 = shape
        .attachment_loads
        .iter()
        .flatten()
        .map(|l| -l.force[2])
        .sum();
    assert!(down >= wires * 0.999, "{down} vs wires {wires}");
    assert!(down < wires * 1.2, "{down} vs wires {wires}");
    // Wind: the structures are pushed with it.
    let mut windy = still.clone();
    windy.wind_pressure = Stress::from_pascals(500.0);
    let blown = solve_section(&line, &line.sections[0], cable, &windy, &budget).unwrap();
    let side = |s: &SectionShape| -> nalgebra::Vector2<f64> {
        s.attachment_loads
            .iter()
            .flatten()
            .map(|l| nalgebra::Vector2::new(l.force[0], l.force[1]))
            .sum()
    };
    assert!((side(&blown) - side(&shape)).norm() > 100.0);
}

/// The ruling span (one tension, clamps held) and the finite elements
/// (strings swing) agree closely on a section strung by tension.
#[test]
fn ruling_span_and_finite_elements_agree() {
    use crate::condition::Calculation;
    let lib = library();
    let mut line = crate::tests::example();
    // Point loads (S1's marker ball) are the finite elements' only.
    line.sections[0].clear_point_loads();
    let cable = lib.get(&line.sections[0].cable).unwrap();
    let budget = default_fe_budget();
    for t in [15.0, 60.0] {
        let case = WeatherCase::still("t", Temperature::from_celsius(t));
        line.sections[0].calculation = Calculation::INITIAL_FE;
        let fe = solve_section(&line, &line.sections[0], cable, &case, &budget).unwrap();
        line.sections[0].calculation = Calculation::INITIAL_RS;
        let rs = solve_section(&line, &line.sections[0], cable, &case, &budget).unwrap();
        let (a, b) = (max_sag(&fe), max_sag(&rs));
        // Within 2 %: the FE strings swing to even out the spans'
        // tensions, RS holds the clamps (1.2 % apart at 60 °C).
        assert!((a - b).abs() < 0.02 * a, "{t} °C: FE {a} RS {b}");
        // One tension in RS.
        let h: Vec<f64> = rs.spans.iter().map(|x| x.horizontal_tension).collect();
        assert!(h.iter().all(|x| (x - h[0]).abs() < 1e-9));
        assert!(rs.spans.iter().all(|x| x.points.len() == 65));
    }
    // Wind blows the RS spans sideways.
    let mut windy = WeatherCase::still("w", Temperature::from_celsius(15.0));
    windy.wind_pressure = Stress::from_pascals(500.0);
    line.sections[0].calculation = Calculation::INITIAL_RS;
    let calm = solve_section(
        &line,
        &line.sections[0],
        cable,
        &WeatherCase::still("c", Temperature::from_celsius(15.0)),
        &budget,
    )
    .unwrap();
    let blown = solve_section(&line, &line.sections[0], cable, &windy, &budget).unwrap();
    let shift = (v(blown.spans[1].mid_point) - v(calm.spans[1].mid_point))
        .xy()
        .norm();
    assert!(shift > 1.0, "{shift}");
}

/// Bimetallic cables: above the transition temperature, outer strands
/// taking no compression (bird cage) leave the core alone, expanding
/// less than the composite: a hot ACSR sags less than with the
/// aluminium in compression; in both methods.
#[test]
fn bird_cage_changes_the_hot_sag() {
    use crate::condition::Calculation;
    let lib = library();
    let budget = default_fe_budget();
    let hot = WeatherCase::still("hot", Temperature::from_celsius(180.0));
    let sag = |line: &Line, cable: &Cable| {
        max_sag(&solve_section(line, &line.sections[0], cable, &hot, &budget).unwrap())
    };
    let mut less = vec![];
    for calculation in [Calculation::INITIAL_RS, Calculation::INITIAL_FE] {
        let mut cable = lib.get("acsr-drake-26-7").unwrap().clone();
        let mut line = crate::tests::example();
        line.sections[0].clear_point_loads();
        line.sections[0].calculation = calculation;
        let free = sag(&line, &cable);
        // The line's choice for a cable without its own …
        line.bimetallic.behavior = ohl_cables::BimetallicBehavior::BirdCage;
        let by_line = sag(&line, &cable);
        // … and the cable's own, which wins over the line's.
        cable.bimetallic_behavior = Some(ohl_cables::BimetallicBehavior::Compression);
        assert!(
            (sag(&line, &cable) - free).abs() < 1e-9,
            "the cable's own wins"
        );
        line.bimetallic = Default::default();
        cable.bimetallic_behavior = Some(ohl_cables::BimetallicBehavior::BirdCage);
        let caged = sag(&line, &cable);
        assert!((by_line - caged).abs() < 1e-9, "{by_line} vs {caged}");
        assert!(
            caged < free - 0.1,
            "{calculation:?}: bird cage {caged} vs {free}"
        );
        less.push(free - caged);
    }
    // Both methods take the same off (the sags themselves differ by
    // 2 % at 180 °C: RS holds the clamps).
    assert!(
        (less[0] - less[1]).abs() < 0.1 * less[0],
        "less sag: RS {} FE {}",
        less[0],
        less[1]
    );
}

/// Creep as a temperature increase (linear model): after creep at
/// 15 °C sags as initial at 15 °C + the shift.
#[test]
fn creep_as_a_temperature_shift() {
    use crate::condition::Calculation;
    let lib = library();
    let mut cable = lib.get("acsr-drake-26-7").unwrap().clone();
    cable.model = ohl_cables::CableModel::LinearCreepTemperature;
    cable.creep_temperature_shift = Some(20.0);
    let mut line = crate::tests::example();
    let every = WeatherCase::still("everyday", Temperature::from_celsius(15.0));
    line.creep_stretch.creep_case = Some("everyday".into());
    line.set_weather_cases(std::slice::from_ref(&every));
    let budget = default_fe_budget();
    line.sections[0].calculation = Calculation::CREEP_FE;
    let crept = solve_section(&line, &line.sections[0], &cable, &every, &budget).unwrap();
    line.sections[0].calculation = Calculation::INITIAL_FE;
    let warm = solve_section(
        &line,
        &line.sections[0],
        &cable,
        &WeatherCase::still("35", Temperature::from_celsius(35.0)),
        &budget,
    )
    .unwrap();
    let (a, b) = (max_sag(&crept), max_sag(&warm));
    assert!((a - b).abs() < 0.005, "creep {a} vs +20 °C {b}");
    // No stretch after load (elastic).
    line.creep_stretch.load_cases = vec!["everyday".into()];
    line.set_weather_cases(std::slice::from_ref(&every));
    let f = final_state(
        &line,
        &line.sections[0],
        &cable,
        crate::condition::Condition::Load,
        &budget,
    )
    .unwrap();
    assert_eq!(f.stretch, 0.0);
}

/// Cables with curves: the finite elements follow the stress-strain
/// curves (secant iteration), so FE and RS agree on a span where both
/// hold — one section with no strings swinging (a dead-end to
/// dead-end span in still air): within the thermal forms' difference.
#[test]
fn finite_elements_follow_the_curves() {
    use crate::condition::Calculation;
    let lib = with_nonlinear(library());
    let cable = lib.get("synthetic").unwrap();
    let mut line = crate::tests::example();
    // The branch S3: two dead-end spans… take its first span alone.
    line.sections.retain(|s| s.id == "S3");
    let s = &mut line.sections[0];
    s.cable = "synthetic".into();
    s.points.truncate(2);
    s.stringing = Stringing::Tension {
        horizontal_tension: 20_000.0,
        temperature: 15.0,
    };
    // Wires fixed at the structures: no strings moving with the tension
    // (RS holds the clamps, FE would let strain strings swing).
    let ends = line.sections[0].points.clone();
    for p in &ends {
        let st = line
            .structures
            .iter_mut()
            .find(|x| x.id == p.structure)
            .unwrap();
        let wire = st.wire_at_set(p.set).unwrap().label.clone();
        st.strings.retain(|s| s.wire != wire);
    }
    let budget = default_fe_budget();
    let sag = |line: &Line, c: Calculation, t: f64| {
        let mut section = line.sections[0].clone();
        section.calculation = c;
        let case = WeatherCase::still("t", Temperature::from_celsius(t));
        let shape = solve_section(line, &section, cable, &case, &budget).unwrap();
        (shape.spans[0].horizontal_tension, shape.spans[0].sag)
    };
    for t in [-20.0, 15.0, 60.0] {
        let (fe, rs) = (
            sag(&line, Calculation::INITIAL_FE, t),
            sag(&line, Calculation::INITIAL_RS, t),
        );
        // Tensions within 0.1 %, sags within 1 cm (the methods measure
        // sag on slightly different chords: 5 mm here at stringing too).
        let (h_fe, h_rs) = (fe.0, rs.0);
        assert!(
            (h_fe - h_rs).abs() < 1e-3 * h_rs,
            "{t} °C: H FE {h_fe} RS {h_rs}"
        );
        assert!(
            (fe.1 - rs.1).abs() < 0.01,
            "{t} °C: sag FE {} RS {}",
            fe.1,
            rs.1
        );
    }
}

/// The data sheet's weight at its reference temperature, taken at the
/// stringing temperature (owner decision 2026-10-21). One dead-end span
/// (S3's first, its wires fixed at the structures) strung at 10 °C: in
/// still air at 10 °C the supports carry its weight, the data sheet's
/// 20 °C value per metre at 10 °C times its unstressed length,
/// \\( w e^{10\alpha} L_0 \\) (hand check), by the finite elements;
/// a cable whose data sheet gives 10 °C weighs \\( w L_0 \\). The ruling
/// span takes the additive form's \\( w / (1 - 10\alpha) \\).
#[test]
fn the_weight_is_the_data_sheets_at_the_stringing_temperature() {
    use crate::condition::Calculation;
    let lib = library();
    let mut line = crate::tests::example();
    line.sections.retain(|s| s.id == "S3");
    let s = &mut line.sections[0];
    s.points.truncate(2);
    s.calculation = Calculation::INITIAL_FE;
    s.stringing = Stringing::Tension {
        horizontal_tension: 20_000.0,
        temperature: 10.0,
    };
    let ends = line.sections[0].points.clone();
    for p in &ends {
        let st = line
            .structures
            .iter_mut()
            .find(|x| x.id == p.structure)
            .unwrap();
        let wire = st.wire_at_set(p.set).unwrap().label.clone();
        st.strings.retain(|s| s.wire != wire);
    }
    let cable = lib.get(&line.sections[0].cable).unwrap().clone();
    let alpha = cable.thermal_expansion();
    let t10 = Temperature::from_celsius(10.0);
    let budget = default_fe_budget();
    let carried = |cable: &Cable| {
        let section = &line.sections[0];
        let (lengths, _) = clip_lengths(&line, section, cable).unwrap();
        let shape = solve_section(
            &line,
            section,
            cable,
            &WeatherCase::still("10", t10),
            &budget,
        )
        .unwrap();
        let up: f64 = shape
            .attachment_loads
            .iter()
            .flatten()
            .map(|s| -s.force[2])
            .sum();
        (up, lengths[0])
    };
    let (up, l0) = carried(&cable);
    let w = cable.weight().si();
    let expected = w * (10.0 * alpha).exp() * l0;
    assert!(
        ((up - expected) / expected).abs() < 1e-9,
        "{up} vs {expected}"
    );
    // Its data sheet at 10 °C: the weight as given.
    let mut at10 = cable.clone();
    at10.reference_temperature = Some(t10);
    let (up10, l10) = carried(&at10);
    assert!(((up10 - w * l10) / (w * l10)).abs() < 1e-9, "{up10}");
    // The conversion, idempotent; the ruling span's additive form.
    let section = &line.sections[0];
    let strung = strung_cable(section, &cable).unwrap();
    assert_eq!(strung.reference_temperature, Some(t10));
    assert!(matches!(
        strung_cable(section, &strung).unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    let mut rs = section.clone();
    rs.calculation = Calculation::INITIAL_RS;
    let additive = strung_cable(&rs, &cable).unwrap().weight().si();
    assert!((additive / (w / (1.0 - 10.0 * alpha)) - 1.0).abs() < 1e-12);
}

/// The finite elements carry the ice per metre of the stretched wire
/// (owner decision 2026-10-21): on the same dead-end span under 12 mm
/// of ice the supports carry the own weight per unstressed metre and
/// the ice per metre of the span's real length — more than the ice on
/// its unstressed length by about its mean strain (hand check of the
/// vertical reactions, to the strain's spread along the span).
#[test]
fn ice_is_carried_per_stretched_metre() {
    let lib = library();
    let mut line = crate::tests::example();
    line.sections.retain(|s| s.id == "S3");
    line.sections[0].points.truncate(2);
    let ends = line.sections[0].points.clone();
    for p in &ends {
        let st = line
            .structures
            .iter_mut()
            .find(|x| x.id == p.structure)
            .unwrap();
        let wire = st.wire_at_set(p.set).unwrap().label.clone();
        st.strings.retain(|s| s.wire != wire);
    }
    let cable = lib.get(&line.sections[0].cable).unwrap();
    let section = &line.sections[0];
    let t = line.sections[0].stringing.temperature();
    let mut iced = WeatherCase::still("ice", Temperature::from_celsius(t));
    iced.ice_thickness = Length::from_millimeters(12.0);
    iced.ice_density = Some(ohl_core::units::Density::from_si(913.0));
    let ice = iced.wire_load(cable.diameter, cable.weight()).vertical.si() - cable.weight().si();
    let shape = solve_section(&line, section, cable, &iced, &default_fe_budget()).unwrap();
    let up: f64 = shape
        .attachment_loads
        .iter()
        .flatten()
        .map(|s| -s.force[2])
        .sum();
    let (lengths, _) = clip_lengths(&line, section, cable).unwrap();
    let own = strung_cable(section, cable).unwrap().weight().si() * lengths[0];
    let mean_strain = 0.5 * (shape.spans[0].horizontal_tension + shape.spans[0].max_tension)
        / cable.axial_stiffness().si();
    let expected = own + ice * lengths[0] * (1.0 + mean_strain);
    assert!(
        ((up - expected) / expected).abs() < 2e-6,
        "{up} vs {expected}: ice on the unstressed length {}",
        own + ice * lengths[0]
    );
    assert!(up > own + ice * lengths[0] * (1.0 + 0.5 * mean_strain));
    // The solver option per unstretched metre (2026-10-21, as other
    // programs): the supports carry the ice on the unstressed length
    // exactly, own + ice L0 (hand check), and the span less tension
    // and sag by about the mean strain's share of the ice.
    let mut other = line.clone();
    other.solver.loads_per = crate::LoadsPer::Unstretched;
    let section = &other.sections[0];
    let plain = solve_section(&other, section, cable, &iced, &default_fe_budget()).unwrap();
    let up_plain: f64 = plain
        .attachment_loads
        .iter()
        .flatten()
        .map(|s| -s.force[2])
        .sum();
    let expected = own + ice * lengths[0];
    assert!(
        ((up_plain - expected) / expected).abs() < 1e-9,
        "{up_plain} vs {expected}"
    );
    let less = plain.spans[0].horizontal_tension / shape.spans[0].horizontal_tension - 1.0;
    assert!(less < -1e-4 && less > -1e-3, "{less}");
    assert!(plain.spans[0].sag < shape.spans[0].sag);
    // Still air: the same either way.
    let still = WeatherCase::still("still", Temperature::from_celsius(t));
    let a = solve_section(
        &line,
        &line.sections[0],
        cable,
        &still,
        &default_fe_budget(),
    );
    let b = solve_section(&other, section, cable, &still, &default_fe_budget());
    assert_eq!(
        a.unwrap().spans[0].horizontal_tension,
        b.unwrap().spans[0].horizontal_tension
    );
}

/// The non-linear model: creep and a heavy load stretch the cable
/// permanently, so it sags more than initially; Max Sag takes the
/// larger; a condition without its weather case is said so.
#[test]
fn creep_and_load_stretch_a_nonlinear_cable() {
    use crate::condition::{Calculation, Condition};
    let lib = with_nonlinear(library());
    let cable = lib.get("synthetic").unwrap();
    let mut line = crate::tests::example();
    line.sections[0].cable = "synthetic".into();
    let budget = default_fe_budget();
    let every = WeatherCase::still("everyday", Temperature::from_celsius(15.0));
    let mut ice = WeatherCase::still("heavy ice", Temperature::from_celsius(-5.0));
    ice.ice_thickness = ohl_core::units::Length::from_millimeters(15.0);
    // Without criteria: said so.
    line.sections[0].calculation = Calculation::CREEP_FE;
    let e = solve_section(&line, &line.sections[0], cable, &every, &budget).unwrap_err();
    assert!(e.to_string().contains("Creep-Stretch"), "{e}");
    line.creep_stretch.creep_case = Some("everyday".into());
    line.creep_stretch.load_cases = vec!["everyday".into(), "heavy ice".into()];
    line.set_weather_cases(&[every.clone(), ice.clone()]);
    let creep = final_state(&line, &line.sections[0], cable, Condition::Creep, &budget).unwrap();
    let load = final_state(&line, &line.sections[0], cable, Condition::Load, &budget).unwrap();
    assert!(creep.stretch > 1e-5, "{}", creep.stretch);
    // Measured at the working tension: the aluminium's set counts too
    // (at zero tension its strands are slack: only the core's would).
    {
        use ohl_cables::mechanics::{MaterialState, Mechanics};
        use ohl_core::units::Force;
        let m = Mechanics::from_cable(cable).unwrap();
        let t = Temperature::from_celsius(15.0);
        let at_zero = m.strain(Force::ZERO, t, &creep.material).unwrap()
            - m.strain(Force::ZERO, t, &MaterialState::Initial).unwrap();
        assert!(
            creep.stretch > 1.5 * at_zero,
            "{} vs {at_zero}",
            creep.stretch
        );
    }
    assert!(load.stretch > 0.0, "{}", load.stretch);
    assert_eq!(load.case, "heavy ice", "the case stretching most");
    let hot = WeatherCase::still("hot", Temperature::from_celsius(50.0));
    let mut sag = |c: Calculation| {
        line.sections[0].calculation = c;
        max_sag(&solve_section(&line, &line.sections[0], cable, &hot, &budget).unwrap())
    };
    for (initial, creep, load, max) in [
        (
            Calculation::INITIAL_FE,
            Calculation::CREEP_FE,
            Calculation::LOAD_FE,
            Calculation::MAX_SAG_FE,
        ),
        (
            Calculation::INITIAL_RS,
            Calculation::CREEP_RS,
            Calculation::LOAD_RS,
            Calculation::MAX_SAG_RS,
        ),
    ] {
        let (i, c, l, m) = (sag(initial), sag(creep), sag(load), sag(max));
        assert!(
            c > i + 0.01 && l > i,
            "{initial:?}: initial {i} creep {c} load {l}"
        );
        assert!((m - c.max(l)).abs() < 1e-9, "max sag {m} of {c}, {l}");
    }
}
