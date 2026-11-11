// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Line parameters of the synthetic double-circuit example
//! (`examples/electrical`): cross-sections from the solved line, the
//! circuits and their stretches, and hand calculations of the positive
//! sequence values.

use std::f64::consts::PI;
use std::path::Path;

use ohl_core::units::Temperature;
use ohl_electrical::cross_section::{self, Sources, StationAt};
use ohl_electrical::earth::{EPS0, MU0};
use ohl_line::Line;
use ohl_line::electrical::{EarthReturn, MeanHeight};

fn example() -> (Line, ohl_cables::Library) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/electrical");
    let line =
        Line::from_json(&std::fs::read_to_string(dir.join("double-circuit.ohl.json")).unwrap())
            .unwrap();
    let library = ohl_cables::Library::load(&dir.join("wires.ohlw")).unwrap();
    (line, library)
}

fn shapes(
    line: &Line,
    library: &ohl_cables::Library,
    t: f64,
) -> Vec<ohl_line::solve::SectionShape> {
    let case = ohl_weather::WeatherCase::still("still", Temperature::from_celsius(t));
    ohl_line::solve::solve_line(line, library, &case)
        .into_iter()
        .map(Result::unwrap)
        .collect()
}

/// The spans, the wires in them, the solved mean against the two-thirds
/// rule, a station at mid-span against the solved low point.
#[test]
fn cross_sections_of_the_example() {
    let (line, library) = example();
    let solved = shapes(&line, &library, 40.0);
    let src = Sources {
        line: &line,
        library: &library,
        shapes: &solved,
        temperature: Temperature::from_celsius(40.0),
        ground: None,
        jumpers: &[],
    };
    let (spans, notes) = cross_section::span_cross_sections(&src, MeanHeight::Solved);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(spans.len(), 2);
    let (span, cs) = &spans[0];
    assert_eq!((span.from.as_str(), span.to.as_str()), ("T1", "T2"));
    assert_eq!(cs.wires.len(), 7);
    assert!((cs.station.span_length - 300.0).abs() < 1e-9);
    // North is 5 m to the left of the line going east: offset −5.
    let nl1 = cs.wires.iter().find(|w| w.section == "NL1").unwrap();
    assert!((nl1.offset + 5.0).abs() < 1e-6, "{}", nl1.offset);
    assert!(nl1.phase.as_ref().is_some_and(|p| p.circuit == "North"));
    let ew = cs.wires.iter().find(|w| w.section == "EW").unwrap();
    assert!(ew.phase.is_none());
    // The R at 40 °C: linear between 25 and 75 °C.
    let r40 = 0.07283e-3 + (0.08688e-3 - 0.07283e-3) * 15.0 / 50.0;
    assert!((nl1.wire.resistance.unwrap() - r40).abs() < 1e-12);
    // The mean by the two-thirds rule within a few cm of the solved mean
    // (a catenary over a ground sloping 2 m in the span).
    let (thirds, _) = cross_section::span_cross_sections(&src, MeanHeight::TwoThirdsSag);
    let t1 = thirds[0]
        .1
        .wires
        .iter()
        .find(|w| w.section == "NL1")
        .unwrap();
    assert!(
        (t1.height - nl1.height).abs() < 0.05,
        "{} {}",
        t1.height,
        nl1.height
    );
    // At mid-span: the solved shape's height there.
    let mid = cross_section::cross_section_at(&src, "T2", "T1", 0.5).unwrap();
    assert_eq!(mid.station.at, StationAt::Fraction(0.5));
    let m1 = mid.wires.iter().find(|w| w.section == "NL1").unwrap();
    let shape = solved.iter().find(|s| s.section == "NL1").unwrap();
    let low = shape.spans[0].low_point;
    // The ground at mid-span is 401 m.
    assert!(
        (m1.height - (low[2] - 401.0)).abs() < 0.05,
        "{} {}",
        m1.height,
        low[2]
    );
}

/// Hand calculation with Carson's first terms and no earth wire: the
/// ideally transposed positive sequence impedance of a circuit is
/// \\( R + j\omega\frac{\mu_0}{2\pi}\ln\frac{GMD}{GMR} \\), the earth terms
/// cancelling (they are equal on and off the diagonal); \\( GMD \\) the
/// geometric mean of its three phase distances (5, 5, 10 m: the vertical
/// circuit, every wire sagging alike).
#[test]
fn positive_sequence_by_hand() {
    let (mut line, library) = example();
    line.sections.retain(|s| s.id != "EW");
    line.electrical.earth_return = EarthReturn::CarsonFirstTerms;
    let solved = shapes(&line, &library, 40.0);
    let p = ohl_electrical::run(
        &line,
        &library,
        &solved,
        Temperature::from_celsius(40.0),
        None,
    );
    assert_eq!(p.circuits.len(), 2);
    let north = p
        .circuits
        .iter()
        .find(|c| c.key.circuit == "North")
        .unwrap();
    assert_eq!(north.phases, ["L1", "L2", "L3"]);
    assert_eq!(north.stretches.len(), 1);
    assert_eq!(north.stretches[0].spans, 2);
    assert!((north.total.length - 600.0).abs() < 1e-9);
    let seq = north.sequence().unwrap();
    // The totals are over the wires' length (2026-11-11): somewhat more
    // than the 600 m between the structures (the sag, the 2 and 4 m of
    // rise and fall).
    let wire = north.total.wire_length;
    assert!(wire > 600.0 && wire < 600.0 * 1.01, "{wire}");
    let per_m = |x: f64| x / wire;
    let r40 = 0.07283e-3 + (0.08688e-3 - 0.07283e-3) * 15.0 / 50.0;
    let gmd = (5.0_f64 * 5.0 * 10.0).cbrt();
    let omega = 2.0 * PI * 50.0;
    let x1 = omega * MU0 / (2.0 * PI) * (gmd / 11.43e-3).ln();
    assert!((per_m(seq.z1_transposed.re) - r40).abs() < 1e-12);
    assert!(
        (per_m(seq.z1_transposed.im) - x1).abs() < 1e-3 * x1,
        "{} {x1}",
        per_m(seq.z1_transposed.im)
    );
    assert!((per_m(seq.b1_transposed) - omega * per_m(seq.c1_transposed)).abs() < 1e-20);
    // A vertical circuit is not transposed: its sequences are coupled.
    assert!(seq.unbalance > 0.01);
    // The two circuits are coupled: a zero sequence mutual impedance.
    assert_eq!(p.couplings.len(), 1);
    assert!(p.couplings[0].z0m().norm() > 0.0);
    assert!(
        p.notes.iter().any(|n| n.contains("assumed")),
        "{:?}",
        p.notes
    );
}

/// Hand calculation of the positive sequence capacitance of one circuit
/// alone (no other wire) with its images, the textbook formula of a
/// transposed line over a perfectly conducting earth (e.g. Glover, Sarma,
/// Overbye, *Power System Analysis and Design*, the effect of the earth on
/// capacitance):
/// \\( C_1 = 2\pi\varepsilon_0 / \left[\ln\frac{GMD}{r} -
/// \ln\frac{(H_{12}H_{23}H_{31})^{1/3}}{(H_{11}H_{22}H_{33})^{1/3}}\right] \\),
/// \\( H_{ij} \\) the distances to the images, from the cross-section's
/// mean heights. The formula averages the potential coefficients, the
/// program the capacitances (each span's in parallel): 0.7 % apart for
/// this vertical circuit (1 % allowed).
#[test]
fn positive_sequence_capacitance_by_hand() {
    let (mut line, library) = example();
    line.sections.retain(|s| s.id.starts_with('N'));
    let solved = shapes(&line, &library, 40.0);
    let p = ohl_electrical::run(
        &line,
        &library,
        &solved,
        Temperature::from_celsius(40.0),
        None,
    );
    let north = &p.circuits[0];
    let seq = north.sequence().unwrap();
    let mut c1_hand = 0.0;
    for (_, cs, _) in &p.spans {
        let w: Vec<_> = ["NL1", "NL2", "NL3"]
            .iter()
            .map(|id| cs.wires.iter().find(|w| &w.section == id).unwrap())
            .collect();
        let d = |i: usize, j: usize| (w[i].offset - w[j].offset).hypot(w[i].height - w[j].height);
        let h = |i: usize, j: usize| (w[i].offset - w[j].offset).hypot(w[i].height + w[j].height);
        let gmd = (d(0, 1) * d(1, 2) * d(2, 0)).cbrt();
        let hm = (h(0, 1) * h(1, 2) * h(2, 0)).cbrt();
        let hs = (h(0, 0) * h(1, 1) * h(2, 2)).cbrt();
        let r = 0.5 * 1.108 * 0.0254;
        // Over the wires' length in the span (the mean of the phases).
        let length = (w[0].length + w[1].length + w[2].length) / 3.0;
        assert!(length >= cs.station.span_length);
        c1_hand += 2.0 * PI * EPS0 / ((gmd / r).ln() - (hm / hs).ln()) * length;
    }
    let ours = seq.c1_transposed;
    assert!((ours - c1_hand).abs() < 1e-2 * c1_hand, "{ours} {c1_hand}");
}

/// The example with its second and third structures (and their wire
/// points) raised by `rise` and twice `rise`: every span sloping alike.
fn sloping(rise: f64) -> Line {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/electrical");
    let text = std::fs::read_to_string(dir.join("double-circuit.ohl.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    for (k, s) in v["structures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        let up = rise * k as f64;
        let z = s["position"][2].as_f64().unwrap();
        s["position"][2] = (z + up).into();
        for w in s["wires"].as_array_mut().unwrap() {
            let z = w["position"][2].as_f64().unwrap();
            w["position"][2] = (z + up).into();
        }
    }
    Line::from_json(&v.to_string()).unwrap()
}

/// The totals are taken over the wires' own length (owner, 2026-11-11),
/// not the span's horizontal length: per span the mean of the circuit's
/// phases, each along the wire as solved.
///
/// By hand, on the level spans: a wire of sag f over a span L is
/// L (1 + 8f²/(3L²)) long (the parabola). On spans sloping at 10° (each
/// 300 m span rising 300 tan 10° = 52.90 m): the chord is L / cos 10°,
/// 1.543 % more, and the totals with it — the values per kilometre (of
/// wire) stay what they are.
#[test]
fn totals_over_the_wires_length() {
    let (level, library) = example();
    let run = |l: &Line| {
        let solved = shapes(l, &library, 40.0);
        let p = ohl_electrical::run(l, &library, &solved, Temperature::from_celsius(40.0), None);
        (p, solved)
    };
    let north = |p: &ohl_electrical::LineParameters| {
        p.circuits
            .iter()
            .find(|c| c.key.circuit == "North")
            .unwrap()
            .clone()
    };
    // Level (the example's 2 m and 4 m of rise and fall over 300 m aside):
    // each wire's length by the parabola, from its solved sag.
    let (p, solved) = run(&level);
    for (_, cs, _) in &p.spans {
        for w in cs.wires.iter().filter(|w| w.section.starts_with('N')) {
            let shape = solved.iter().find(|s| s.section == w.section).unwrap();
            let sp = &shape.spans[w.span];
            let (a, b) = (sp.points[0], sp.points[sp.points.len() - 1]);
            let l = (b[0] - a[0]).hypot(b[1] - a[1]);
            let chord = l.hypot(b[2] - a[2]);
            let by_hand = chord * (1.0 + 8.0 * sp.sag * sp.sag / (3.0 * chord * chord));
            // The wire runs from structure to structure here (no strain
            // strings in the example): nothing else is in its length.
            assert!(
                (w.length - by_hand).abs() < 2e-4 * by_hand,
                "{} {by_hand}",
                w.length
            );
            assert!(w.length > cs.station.span_length);
        }
    }
    let c = north(&p);
    assert!((c.total.length - 600.0).abs() < 1e-9);
    let more = c.total.wire_length / c.total.length - 1.0;
    assert!(more > 0.0005 && more < 0.005, "{more}");
    assert!(
        p.notes
            .iter()
            .any(|n| n == ohl_electrical::parameters::WIRE_LENGTH_NOTE),
        "{:?}",
        p.notes
    );
    // The totals are the values per unit length times the wires' length.
    let per_km = c.total.per_km();
    let z_total = &c.total.z;
    let k = c.total.wire_length / 1000.0;
    for i in 0..3 {
        for j in 0..3 {
            assert!((per_km.0[(i, j)] * k - z_total[(i, j)]).norm() < 1e-12);
        }
    }
    // Sloping at 10°: the totals 1/cos 10° = 1.01543 times the level
    // line's (the sag's share changing a little with the longer chord),
    // per kilometre of wire the same within the geometry's small change.
    let beta = 10.0_f64.to_radians();
    let (ps, _) = run(&sloping(300.0 * beta.tan()));
    let s = north(&ps);
    assert!((s.total.length - 600.0).abs() < 1e-9);
    let ratio = s.total.wire_length / c.total.wire_length;
    assert!((ratio - 1.0 / beta.cos()).abs() < 1.5e-3, "{ratio}");
    assert!((1.0 / beta.cos() - 1.015_43).abs() < 1e-5);
    let (a, b) = (c.sequence().unwrap(), s.sequence().unwrap());
    let per = |z: nalgebra::Complex<f64>, t: &ohl_electrical::parameters::Totals| z / t.wire_length;
    let (za, zb) = (
        per(a.z1_transposed, &c.total),
        per(b.z1_transposed, &s.total),
    );
    assert!((za - zb).norm() < 2e-3 * za.norm(), "{za} {zb}");
    // The total positive sequence impedance grows with the wires' length.
    let grown = b.z1_transposed.norm() / a.z1_transposed.norm();
    assert!((grown - ratio).abs() < 2e-3, "{grown} {ratio}");
    // Before (the horizontal length): low by 1.5 % and more on this line.
    assert!(s.total.wire_length / s.total.length > 1.0154);
    // The coupling's length follows.
    let k = &ps.couplings[0];
    assert!((k.length - 600.0).abs() < 1e-9 && k.wire_length > 1.0154 * k.length);
    // The report's rows: horizontal and wire length, the totals over the
    // wire's.
    let rows = ohl_electrical::report::rows(&ps, &[]);
    let r = rows
        .iter()
        .find(|r| r.kind == "circuit" && r.circuit == "North")
        .unwrap();
    assert!((r.length_km - 0.6).abs() < 1e-12);
    assert!((r.wire_km - s.total.wire_length / 1000.0).abs() < 1e-12);
    let (z, _) = r.totals().unwrap();
    assert!((z[1] - b.z1_transposed.im).abs() < 1e-9 * z[1]);
    let csv = ohl_electrical::report::csv(&ps, &[]);
    assert!(
        csv.lines()
            .next()
            .unwrap()
            .ends_with(",unbalance,wire_length_km")
    );
}

/// The earth wire reduced out lowers the zero sequence impedance's
/// magnitude (part of the earth current returns through it) and leaves the
/// positive sequence nearly as it is; the three earth return methods agree
/// on the positive sequence within 0.5 % and on the zero sequence within
/// 5 %.
#[test]
fn earth_wire_and_methods() {
    let (line, library) = example();
    let run = |l: &Line| {
        let solved = shapes(l, &library, 40.0);
        let p = ohl_electrical::run(l, &library, &solved, Temperature::from_celsius(40.0), None);
        let c = p
            .circuits
            .iter()
            .find(|c| c.key.circuit == "North")
            .unwrap()
            .clone();
        c.sequence().unwrap()
    };
    let with = run(&line);
    let mut bare = line.clone();
    bare.sections.retain(|s| s.id != "EW");
    let without = run(&bare);
    assert!(with.z0_transposed.norm() < without.z0_transposed.norm());
    assert!(
        (with.z1_transposed - without.z1_transposed).norm() < 0.02 * without.z1_transposed.norm()
    );
    for m in [EarthReturn::ComplexDepth, EarthReturn::CarsonFirstTerms] {
        let mut l = line.clone();
        l.electrical.earth_return = m;
        let s = run(&l);
        assert!((s.z1_transposed - with.z1_transposed).norm() < 5e-3 * with.z1_transposed.norm());
        assert!((s.z0_transposed - with.z0_transposed).norm() < 5e-2 * with.z0_transposed.norm());
    }
}

/// Soil resistivities measured at the structures (owner, 2026-10-27): a
/// span takes the geometric mean of its two structures' (one measured:
/// that one; none: the line's). Measured the same everywhere gives what
/// the line's own value gives; a span without either is said assumed.
#[test]
fn soil_resistivity_measured_at_the_structures() {
    let (line, library) = example();
    let solved = shapes(&line, &library, 40.0);
    let z0 = |l: &Line| {
        let p = ohl_electrical::run(l, &library, &solved, Temperature::from_celsius(40.0), None);
        let c = p
            .circuits
            .iter()
            .find(|c| c.key.circuit == "North")
            .unwrap()
            .clone();
        (c.sequence().unwrap().z0_transposed, p.notes)
    };
    let mut given = line.clone();
    given.electrical.soil_resistivity_ohm_m = Some(400.0);
    let mut measured = line.clone();
    for s in &mut measured.structures {
        s.soil_resistivity_ohm_m = Some(400.0);
    }
    let (a, notes_given) = z0(&given);
    let (b, notes_measured) = z0(&measured);
    assert!((a - b).norm() < 1e-12 * a.norm());
    assert!(!notes_given.iter().any(|n| n.contains("assumed")));
    assert!(!notes_measured.iter().any(|n| n.contains("assumed")));
    assert!(
        notes_measured
            .iter()
            .any(|n| n.contains("measured at 3 structures"))
    );
    // T1 100, T2 400: the first span 200 Ω·m; T3 not measured: the
    // second span takes T2's.
    let mut mixed = line.clone();
    mixed.structures[0].soil_resistivity_ohm_m = Some(100.0);
    mixed.structures[1].soil_resistivity_ohm_m = Some(400.0);
    assert_eq!(mixed.span_soil_resistivity("T1", "T2"), (200.0, false));
    assert_eq!(mixed.span_soil_resistivity("T2", "T3"), (400.0, false));
    assert_eq!(line.span_soil_resistivity("T1", "T2"), (100.0, true));
    let (_, notes) = z0(&line);
    assert!(
        notes.iter().any(|n| n.contains("not set on 2 of 2 spans")),
        "{notes:?}"
    );
    // A resistivity of 0 is refused.
    let mut zero = line.clone();
    zero.structures[0].soil_resistivity_ohm_m = Some(0.0);
    assert!(zero.validate().is_err());
}
