// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the interface loop at calculation level 3 ([`super::flexible`]),
//! against independent references: a span on a spring and on a pole
//! (closed form: the elastic catenary, by bisection), two spans on a
//! flexible suspension string with one span iced (the same wire model
//! solved at once with the spring as three long bars), the limits (rigid,
//! the stringing state), and the exit clauses.

use super::final_stringing_tests::one_span;
use super::flexible::*;
use super::local_tests::two_spans;
use super::*;
use nalgebra::DMatrix;
use ohl_core::units::{Length, Stress, Temperature};
use std::time::Duration;

/// The wire of `one_span` (as its test writes it).
const ONE_SPAN_WIRE: &str = "id = \"hand\"\nname = \"hand\"\nkind = \"aaac\"\n\
    source = \"invented test data\"\ndiameter = \"28.1 mm\"\nmass_per_length = \"1.6285 kg/m\"\n\
    rated_tensile_strength = \"140 kN\"\nmodel = \"linear_creep_temperature\"\n\
    creep_temperature_shift = 20.0\n\
    outer = { area = \"400 mm²\", modulus = \"86.64 GPa\", thermal_expansion = 18.8514e-6 }\n";

/// The wire of `two_spans` (as its test writes it).
const TWO_SPANS_WIRE: &str = "id = \"span-cases\"\nname = \"Synthetic homogeneous test cable\"\n\
    kind = \"aaac\"\nsource = \"Synthetic test data, not a real conductor\"\n\
    diameter = \"28 mm\"\nmass_per_length = \"1.6285 kg/m\"\n\
    rated_tensile_strength = \"120 kN\"\n\
    outer = { area = \"400 mm²\", modulus = \"86.6 GPa\", thermal_expansion = 18.9e-6 }\n";

/// A wire library holding `cable`, written as `body`.
fn library_of(cable: &Cable, body: &str) -> ohl_cables::Library {
    let mut lib = ohl_cables::Library::default();
    let text = format!("format = \"ohl-wire\"\nversion = 1\n[[cable]]\n{body}");
    lib.add_toml(std::path::Path::new("test.ohlw"), &text)
        .unwrap();
    assert_eq!(lib.get(&cable.id), Some(cable), "{:?}", lib.skipped());
    lib
}

fn still(t: f64) -> WeatherCase {
    WeatherCase::still("case", Temperature::from_celsius(t))
}

fn iced(t: f64, mm: f64) -> WeatherCase {
    let mut c = still(t);
    c.ice_thickness = Length::from_millimeters(mm);
    c
}

/// A diagonal flexibility for one point: 1/k along each line axis (0:
/// rigid that way).
fn springs(k: [f64; 3]) -> DMatrix<f64> {
    DMatrix::from_diagonal(&nalgebra::DVector::from_iterator(
        3,
        k.iter().map(|k| if *k > 0.0 { 1.0 / k } else { 0.0 }),
    ))
}

/// One structure point on springs `k` (N/m along x, y, z of the line),
/// its reference force from the stringing state.
fn on_springs(
    line: &Line,
    library: &ohl_cables::Library,
    structure: &str,
    label: &str,
    k: [f64; 3],
) -> Structures {
    let reference = reference_forces(line, library, &default_fe_budget());
    assert!(reference.failed.is_empty(), "{:?}", reference.failed);
    let f = reference.forces[&(structure.to_owned(), label.to_owned())];
    Structures {
        flexible: vec![LinearStructure {
            structure: structure.into(),
            labels: vec![label.into()],
            flexibility: springs(k),
            reference: vec![f],
            body: vec![[0.0; 3]],
            approximate: false,
            notes: Vec::new(),
        }],
        ..Structures::default()
    }
}

fn force_at(shape: &SectionShape, k: usize, label: &str) -> Point {
    shape.attachment_loads[k]
        .iter()
        .find(|s| s.label == label)
        .unwrap()
        .force
}

/// The level span of `one_span` (300 m, 20 kN at 15 °C), its loads per
/// unstretched metre (the total weight fixed: the closed form exact), as
/// a bundle of `bundle`.
fn level_span(bundle: u32) -> (Line, Cable) {
    let (mut line, cable) = one_span(
        r#"{"kind": "tension", "horizontal_tension": 20000, "temperature": 15}"#,
        "initial-fe",
    );
    line.solver.loads_per = crate::LoadsPer::Unstretched;
    line.sections[0].bundle = bundle;
    if bundle > 1 {
        line.sections[0].bundle_spacing = Some(0.4);
    }
    line.validate().unwrap();
    (line, cable)
}

/// The horizontal tension of the level span with T2 on a spring `k`
/// along it, in closed form: the elastic catenary
/// \\( L(H) = \frac{2H}{w}\operatorname{asinh}\frac{wL_0}{2H} + \frac{HL_0}{EA} \\)
/// (w, L₀ of the case from the rigid solve's H and weight) equal to the
/// chord shortened by the spring, \\( L - (nH + F_{ref,x})/k \\), by
/// bisection — independent of the loop.
fn spring_closed_form(rigid: &SectionShape, ea: f64, bundle: f64, reference_x: f64, k: f64) -> f64 {
    let span = 300.0;
    let h_r = rigid.spans[0].horizontal_tension;
    let v: f64 = rigid
        .attachment_loads
        .iter()
        .flatten()
        .map(|s| -s.force[2])
        .sum::<f64>()
        / bundle;
    let l0 = span / (2.0 * h_r / v * (v / (2.0 * h_r)).asinh() + h_r / ea);
    let w = v / l0;
    let chord = |h: f64| 2.0 * h / w * (w * l0 / (2.0 * h)).asinh() + h * l0 / ea;
    // The force on T2 along x is −nH: u = (−nH − F_ref,x)/k.
    let gap = |h: f64| chord(h) - (span + (-bundle * h - reference_x) / k);
    let (mut lo, mut hi) = (100.0, 1e6);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if gap(mid) > 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Hand case 1: a level span between a rigid support and one on a spring
/// along the line (100 kN/m), single and a bundle of two, cold and hot:
/// the loop's tension is the closed form's to 10⁻⁶ (tolerance 10⁻⁷ m), and
/// to 10⁻⁴ with the default tolerances; the point's move is C (F − F_ref);
/// cold, the span slackens (tension below rigid, sag above); hot, the
/// structure springs back and tightens it (tension above rigid, sag
/// below: rigid is then conservative for the sag, the "to verify" row of
/// the theory page).
#[test]
fn a_span_on_a_spring_is_the_closed_form() {
    let k = 1e5;
    for bundle in [1_u32, 2] {
        let (line, cable) = level_span(bundle);
        let lib = library_of(&cable, ONE_SPAN_WIRE);
        let structures = on_springs(&line, &lib, "T2", "W1", [k, 0.0, 0.0]);
        let reference_x = structures.flexible[0].reference[0][0];
        assert!(
            (reference_x + f64::from(bundle) * 20_000.0).abs() < 1e-3,
            "{reference_x}"
        );
        let ea = cable.axial_stiffness().si();
        for (t, slackens) in [(-20.0, true), (50.0, false)] {
            let case = still(t);
            let tight = FlexibleOptions {
                displacement_tolerance: 1e-7,
                force_tolerance: 1e-3,
                ..FlexibleOptions::default()
            };
            let budget = default_fe_budget();
            let solved =
                solve_line_flexible(&line, &lib, &case, &budget, &structures, &tight).unwrap();
            let rigid = solved.rigid[0].as_ref().unwrap();
            let flexible = solved.sections[0].as_ref().unwrap();
            let expected = spring_closed_form(rigid, ea, f64::from(bundle), reference_x, k);
            let h = flexible.spans[0].horizontal_tension;
            assert!(
                (h - expected).abs() < 1e-6 * expected,
                "bundle {bundle}, {t} °C: H {h} against the closed form's {expected}"
            );
            let h_r = rigid.spans[0].horizontal_tension;
            assert_eq!(h < h_r, slackens, "{t} °C: {h_r} → {h}");
            assert_eq!(
                flexible.spans[0].sag > rigid.spans[0].sag,
                slackens,
                "{t} °C"
            );
            // u = C (F − F_ref).
            let p = &solved.points[0];
            let f = p.flexible.unwrap();
            assert!(
                (p.displacement[0] - (f[0] - reference_x) / k).abs() < 1e-7,
                "{p:?}"
            );
            assert_eq!(p.displacement[1], 0.0);
            assert_eq!(p.displacement[2], 0.0);
            assert!(solved.passes.len() <= 6, "{:?}", solved.passes);
            // The default tolerances: within 10⁻⁴.
            let plain = solve_line_flexible(
                &line,
                &lib,
                &case,
                &budget,
                &structures,
                &FlexibleOptions::default(),
            )
            .unwrap();
            let h = plain.sections[0].as_ref().unwrap().spans[0].horizontal_tension;
            assert!((h - expected).abs() < 1e-4 * expected, "{h} {expected}");
            assert!(plain.passes.len() <= 4, "{:?}", plain.passes);
        }
    }
}

/// A pole-supported span: T2 a cantilever pole 12 m high, its top 7 kN/m
/// both ways (3EI/h³, softer than the span along it) and stiff
/// vertically: the closed form as above (the spring along the line; the
/// span carries nothing across), decimetres of move, converged within
/// the passes.
#[test]
fn a_span_on_a_pole_is_the_closed_form() {
    let k = 7e3;
    let (line, cable) = level_span(1);
    let lib = library_of(&cable, ONE_SPAN_WIRE);
    let structures = on_springs(&line, &lib, "T2", "W1", [k, k, 1e8]);
    let reference_x = structures.flexible[0].reference[0][0];
    let ea = cable.axial_stiffness().si();
    let case = iced(-5.0, 20.0);
    let tight = FlexibleOptions {
        displacement_tolerance: 1e-7,
        force_tolerance: 1e-3,
        ..FlexibleOptions::default()
    };
    let solved = solve_line_flexible(
        &line,
        &lib,
        &case,
        &default_fe_budget(),
        &structures,
        &tight,
    )
    .unwrap();
    let rigid = solved.rigid[0].as_ref().unwrap();
    let flexible = solved.sections[0].as_ref().unwrap();
    // The vertical spring moves the point too (by ΔV/k_z, a few
    // tenths of a millimetre): the span is then no longer level by
    // that much — its effect on H far below the tolerance asked.
    let expected = spring_closed_form(rigid, ea, 1.0, reference_x, k);
    let h = flexible.spans[0].horizontal_tension;
    assert!(
        (h - expected).abs() < 1e-5 * expected,
        "H {h} against {expected}"
    );
    let u = solved.points[0].displacement;
    assert!(u[0] < -0.1, "towards the span by decimetres: {u:?}");
    assert!(u[1].abs() < 1e-9, "{u:?}");
    assert!(rigid.spans[0].horizontal_tension - h > 1000.0);
    assert!(solved.passes.len() <= 8, "{:?}", solved.passes);
}

/// The two-span section of the local tests clipped in (its lengths
/// fixed), span 1 iced and span 2 bare (local ice factors 1 and 0).
fn unbalanced() -> (Line, Cable) {
    let (mut line, cable) = two_spans();
    let (lengths, temperature) = clip_lengths(&line, &line.sections[0], &cable).unwrap();
    line.sections[0].stringing = Stringing::UnstressedLengths {
        lengths,
        temperature,
        sagged: None,
    };
    line.sections[0].ice_factors = vec![
        crate::SpanWind {
            span: 0,
            factor: 1.0,
        },
        crate::SpanWind {
            span: 1,
            factor: 0.0,
        },
    ];
    line.validate().unwrap();
    (line, cable)
}

/// The same section model the loop solves (`solve_fe`'s path for a
/// clipped-in linear cable), its point `label` at section point `k`
/// freed and held by three bars of 100 km along the line's axes, each of
/// stiffness k (EA = k L₀) and unstressed so that it carries the
/// stringing force at the drawn point: a spring u = (F − F_ref)/k — one
/// Newton solve, no interface. Returns the clamp at `k`, the force on the
/// point, the spans' horizontal tensions and the point's move.
fn monolithic(
    line: &Line,
    cable: &Cable,
    case: &WeatherCase,
    k: usize,
    label: &str,
    stiffness: [f64; 3],
    reference: Point,
) -> (Vector3<f64>, Point, Vec<f64>, Vector3<f64>) {
    let budget = default_fe_budget();
    let section = &*line.section_with_fractions(&line.sections[0]);
    assert!(section.stringing.clipped());
    let own = &*line.bimetallic.apply(cable);
    let cable = &*strung_cable(section, own).unwrap();
    let prepared = memo::prepared(line, section, cable, &budget).unwrap();
    let mut reference_state = prepared.reference.clone();
    reference_state.loads_per = line.solver.loads_per;
    reference_state.blocks = line.solver.blocks;
    let loaded = pulled(&prepared.layout, section, &Coupling::default());
    let local = LocalWinds::of(section, Cases::one(case));
    let cases = match &local {
        Some(l) => l.apply(Cases::one(case)),
        None => Cases::one(case),
    };
    let extras = Extras::of(&SpacerCoupling::default(), 1.0);
    let built = build(
        section,
        &loaded,
        &reference_state,
        cable,
        cases,
        true,
        &extras,
    )
    .unwrap();
    let node = built.supports[k]
        .iter()
        .find(|(l, _)| l == label)
        .unwrap()
        .1;
    let mut model = built.model.clone();
    model.nodes[node].fixed = false;
    let at = model.nodes[node].position;
    let long = 1e5;
    let mut bars = Vec::new();
    for axis in 0..3 {
        let mut e = Vector3::zeros();
        e[axis] = 1.0;
        model.nodes.push(Node::fixed(at + e * long));
        // The bar's tension pulls the point along +e: N = −F_e at
        // equilibrium; N = k (l − l₀) with l = L at the drawn point.
        let l0 = long + reference[axis] / stiffness[axis];
        model.trusses.push(TrussElement {
            i: node,
            j: model.nodes.len() - 1,
            unstressed_length: l0,
            axial_stiffness: stiffness[axis] * l0,
            tension_only: false,
            weight: 0.0,
        });
        bars.push(model.trusses.len() - 1);
    }
    let solution = model.solve(&budget).unwrap();
    let force = [0, 1, 2].map(|a| -solution.truss_forces[bars[a]]);
    let tensions = built
        .span_elements
        .iter()
        .map(|e| solution.cables[e[0]].horizontal)
        .collect();
    (
        solution.positions[built.clamps[k]],
        force,
        tensions,
        solution.positions[node] - at,
    )
}

/// Hand case 2 and the unbalanced case: two spans on a suspension string
/// whose top is on springs (50 kN/m along and across, 5 MN/m vertical),
/// span 1 iced: the loop agrees with the monolithic solve (the spring as
/// bars in the one wire model) to 0.1 mm and 1 N; the flexible top
/// relieves the unbalance — the longitudinal load on the structure lower
/// than rigid, the iced span sagging more, both as the monolithic solve
/// says.
#[test]
fn two_spans_on_a_flexible_string_top_are_the_monolithic_solve() {
    let (line, cable) = unbalanced();
    let lib = library_of(&cable, TWO_SPANS_WIRE);
    let k = [5e4, 5e4, 5e6];
    let structures = on_springs(&line, &lib, "T2", "A1", k);
    let reference = structures.flexible[0].reference[0];
    let case = iced(-5.0, 15.0);
    let budget = default_fe_budget();
    let solved = solve_line_flexible(
        &line,
        &lib,
        &case,
        &budget,
        &structures,
        &FlexibleOptions::default(),
    )
    .unwrap();
    let flexible = solved.sections[0].as_ref().unwrap();
    let rigid = solved.rigid[0].as_ref().unwrap();
    let (clamp, force, tensions, moved) = monolithic(&line, &cable, &case, 1, "A1", k, reference);
    let loop_clamp = v(flexible.clamps[1]);
    assert!(
        (loop_clamp - clamp).norm() < 1e-4,
        "clamp {loop_clamp} against {clamp}"
    );
    let f = force_at(flexible, 1, "A1");
    for c in 0..3 {
        assert!(
            (f[c] - force[c]).abs() < 1.0,
            "force {f:?} against {force:?}"
        );
    }
    for (s, t) in tensions.iter().enumerate() {
        let h = flexible.spans[s].horizontal_tension;
        assert!((h - t).abs() < 1.0, "span {s}: H {h} against {t}");
    }
    let u = v(solved.points[0].displacement);
    assert!((u - moved).norm() < 1e-4, "move {u} against {moved}");
    // The unbalance relieved: the iced span pulls the top towards it
    // (−x); the longitudinal load is lower than rigid, the iced span
    // sags more and the bare one less.
    let f_rigid = force_at(rigid, 1, "A1");
    assert!(f_rigid[0] < -100.0, "{f_rigid:?}");
    assert!(
        f[0].abs() < f_rigid[0].abs(),
        "longitudinal: rigid {} flexible {}",
        f_rigid[0],
        f[0]
    );
    assert!(u[0] < -1e-3, "{u}");
    assert!(flexible.spans[0].sag > rigid.spans[0].sag);
    assert!(flexible.spans[1].sag < rigid.spans[1].sag);
    // And so the monolithic solve: the same sags (its clamp, its
    // tensions agree above).
    assert!(solved.passes.len() <= 6, "{:?}", solved.passes);
    let change = solved
        .spans
        .iter()
        .find(|s| s.span == 0)
        .map(|s| s.sag[1] - s.sag[0])
        .unwrap();
    assert!(change > 0.0, "{change}");
}

/// The limits: in the stringing state the points do not move (u = 0
/// exactly, converged in pass 0, the sections the rigid solve's bit for
/// bit); a structure a million times stiffer gives the rigid results to
/// 10⁻⁶; no flexible structure: pass 0 alone, the rigid solve.
#[test]
fn the_stringing_state_and_the_rigid_limit() {
    let (line, cable) = unbalanced();
    let lib = library_of(&cable, TWO_SPANS_WIRE);
    let budget = default_fe_budget();
    let structures = on_springs(&line, &lib, "T2", "A1", [5e4, 5e4, 5e6]);
    let rigid_solve = solve_line_with(&line, &lib, &still(15.0), &budget);
    let at_stringing = solve_line_flexible(
        &line,
        &lib,
        &still(15.0),
        &budget,
        &structures,
        &FlexibleOptions::default(),
    )
    .unwrap();
    assert_eq!(at_stringing.passes.len(), 1);
    assert_eq!(at_stringing.points[0].displacement, [0.0; 3]);
    assert_eq!(
        format!("{:?}", at_stringing.sections),
        format!("{rigid_solve:?}")
    );
    assert_eq!(
        format!("{:?}", at_stringing.rigid),
        format!("{rigid_solve:?}")
    );
    let stiff = on_springs(&line, &lib, "T2", "A1", [5e10, 5e10, 5e12]);
    let case = iced(-5.0, 15.0);
    let solved = solve_line_flexible(
        &line,
        &lib,
        &case,
        &budget,
        &stiff,
        &FlexibleOptions::default(),
    )
    .unwrap();
    let rigid = solve_line_with(&line, &lib, &case, &budget);
    let (a, b) = (
        solved.sections[0].as_ref().unwrap(),
        rigid[0].as_ref().unwrap(),
    );
    for (x, y) in a.spans.iter().zip(&b.spans) {
        assert!(
            (x.horizontal_tension - y.horizontal_tension).abs() < 1e-6 * y.horizontal_tension,
            "{} {}",
            x.horizontal_tension,
            y.horizontal_tension
        );
    }
    assert_eq!(format!("{:?}", solved.rigid), format!("{rigid:?}"));
    let none = solve_line_flexible(
        &line,
        &lib,
        &case,
        &budget,
        &Structures::default(),
        &FlexibleOptions::default(),
    )
    .unwrap();
    assert_eq!(none.passes.len(), 1);
    assert_eq!(format!("{:?}", none.sections), format!("{rigid:?}"));
}

/// The exit clauses: a case stopped by the passes or by the time limit
/// fails, naming the worst point, its direction and the trend, the rigid
/// solve kept with it — never the last pass returned; a deliberately soft
/// structure fails cleanly in the same way, or converges within the
/// passes to its real equilibrium.
#[test]
fn a_case_that_does_not_converge_fails_with_its_reason() {
    let (line, cable) = level_span(1);
    let lib = library_of(&cable, ONE_SPAN_WIRE);
    let budget = default_fe_budget();
    let structures = on_springs(&line, &lib, "T2", "W1", [7e3, 7e3, 1e8]);
    let case = iced(-5.0, 20.0);
    let capped = FlexibleOptions {
        passes: 2,
        ..FlexibleOptions::default()
    };
    let e = solve_line_flexible(&line, &lib, &case, &budget, &structures, &capped).unwrap_err();
    assert_eq!(e.passes.len(), 2);
    assert!(
        e.reason
            .starts_with("not converged in 2 passes; mismatch still")
            && e.reason.contains("at T2 W1 (along the line)")
            && e.reason.contains("decreasing"),
        "{}",
        e.reason
    );
    assert_eq!(e.rigid, solve_line_with(&line, &lib, &case, &budget));
    assert!(e.to_string().contains("the rigid results stay"), "{e}");
    let timed = FlexibleOptions {
        time_limit: Duration::ZERO,
        ..FlexibleOptions::default()
    };
    let e = solve_line_flexible(&line, &lib, &case, &budget, &structures, &timed).unwrap_err();
    assert_eq!(e.passes.len(), 1);
    assert!(
        e.reason.starts_with("stopped by the time limit"),
        "{}",
        e.reason
    );
    // A deliberately soft top, 1 N/m vertically (a metre per newton):
    // stopped after 8 passes it fails cleanly, naming the vertical
    // mismatch; given its 20 it finds the real equilibrium — the top sunk
    // until its vertical load is back at the stringing state's (by tens
    // of metres, the iced span then pulling it along the line too), u = C
    // (F − F_ref) to 0.1 mm — after many halved steps.
    let soft = on_springs(&line, &lib, "T2", "W1", [7e3, 7e3, 1.0]);
    let eight = FlexibleOptions {
        passes: 8,
        ..FlexibleOptions::default()
    };
    let e = solve_line_flexible(&line, &lib, &case, &budget, &soft, &eight).unwrap_err();
    assert_eq!(e.passes.len(), 8);
    assert!(e.passes.iter().any(|p| p.halved), "{:?}", e.passes);
    assert!(
        e.reason
            .starts_with("not converged in 8 passes; mismatch still")
            && e.reason.contains("at T2 W1 (vertical)"),
        "{}",
        e.reason
    );
    let sunk = solve_line_flexible(
        &line,
        &lib,
        &case,
        &budget,
        &soft,
        &FlexibleOptions::default(),
    )
    .unwrap();
    assert!(sunk.passes.len() <= INTERFACE_PASSES);
    let p = &sunk.points[0];
    let f = p.flexible.unwrap();
    let reference = soft.flexible[0].reference[0];
    assert!(p.displacement[2] < -10.0, "{p:?}");
    assert!(
        (p.displacement[2] - (f[2] - reference[2])).abs() < 1e-4,
        "{p:?}"
    );
}

/// The rigid paths unchanged: the line solve through the new base
/// coupling (none) is the old one bit for bit, every section of the
/// example line (strings, a V, jumpers, a TAK) in still air and wind; and
/// pass 0 of the loop is the rigid solve, its stiffness left out.
#[test]
fn the_rigid_solve_is_unchanged_bit_for_bit() {
    let lib = ohl_cables::Library::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let line = crate::tests::example();
    let budget = default_fe_budget();
    let mut windy = still(-5.0);
    windy.wind_pressure = Stress::from_pascals(450.0);
    for case in [still(15.0), windy] {
        let plain = solve_line_with(&line, &lib, &case, &budget);
        let based = solve_line_based(&line, &lib, &case, &budget, &Coupling::default());
        assert_eq!(format!("{plain:?}"), format!("{based:?}"));
        let solved = solve_line_flexible(
            &line,
            &lib,
            &case,
            &budget,
            &Structures::default(),
            &FlexibleOptions::default(),
        )
        .unwrap();
        assert_eq!(format!("{plain:?}"), format!("{:?}", solved.rigid));
        assert_eq!(format!("{plain:?}"), format!("{:?}", solved.sections));
    }
}
