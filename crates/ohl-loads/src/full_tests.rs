// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Calculation level 4 (plan of 2026-11-15, §5.6): the example line at
//! levels 2, 3 and 4 (each point where its structure's full solve puts
//! it under the converged forces, the rigid solve bit for bit), and on
//! the example line with invented structures at one pole's place: a
//! linear structure (level 4 = level 3), a guyed pole whose guy goes
//! slack (against the hand model of the pole alone), a structure whose
//! full solve fails (taken linear, the line goes on), an X panel whose
//! tension-only diagonals switch under the loop.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ohl_line::solve::flexible::{
    CalculationLevel, FlexibleOptions, FlexibleSolve, FullProvider, LinearStructure, Structures,
    reference_forces, solve_line_flexible,
};
use ohl_structures_fe::condense::Attachment;
use ohl_structures_fe::model::{JointPoint, Member, MemberKind, Model, Node, Section};
use ohl_structures_fe::tangent::FullModel;
use ohl_weather::WeatherCase;

use super::*;

const E: f64 = 2.1e11;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn example() -> (
    ohl_line::Line,
    ohl_cables::Library,
    ohl_structures::StructureLibrary,
) {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let line = ohl_line::Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    let library = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    (line, library, types)
}

fn still(t: f64) -> WeatherCase {
    WeatherCase::still("case", ohl_core::units::Temperature::from_celsius(t))
}

fn iced() -> WeatherCase {
    let mut c = still(-5.0);
    c.name = "iced".into();
    c.ice_thickness = ohl_core::units::Length::from_millimeters(15.0);
    c
}

fn windy(pa: f64) -> WeatherCase {
    let mut c = still(5.0);
    c.name = format!("wind {pa} Pa");
    c.wind_pressure = ohl_core::units::Stress::from_pascals(pa);
    c
}

fn size(v: [f64; 3]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Each flexible structure's point where the structure's own full answer
/// to the converged forces puts it, to the loop's tolerance (the fixed
/// point of level 4), through fresh answers of `provider`.
fn at_the_full_answer(
    solved: &FlexibleSolve,
    linear: &[LinearStructure],
    provider: &dyn FullProvider,
) {
    let answers = provider.responses(linear);
    for (s, a) in linear.iter().zip(&answers) {
        if solved
            .full_failed
            .iter()
            .any(|f| f.structure == s.structure)
        {
            continue;
        }
        let forces: Vec<ohl_line::Point> = s
            .labels
            .iter()
            .map(|l| {
                solved
                    .points
                    .iter()
                    .find(|p| p.structure == s.structure && p.label == *l)
                    .and_then(|p| p.flexible)
                    .unwrap()
            })
            .collect();
        let u = a.answer(&forces).unwrap();
        assert_eq!(a.failed(), None, "{}", s.structure);
        for (l, d) in s.labels.iter().zip(&u.displacement) {
            let p = solved
                .points
                .iter()
                .find(|p| p.structure == s.structure && p.label == *l)
                .unwrap();
            let gap = size([0, 1, 2].map(|k| p.displacement[k] - d[k]));
            assert!(gap < 1e-4, "{} {l}: {:?} against {d:?}", s.structure, p);
        }
    }
}

/// The example line at levels 2, 3 and 4 in still air at the stringing
/// temperature, cold, iced, and two winds: level 4 converges from level
/// 3's solution in a few line solves, its rigid solve the plain one bit
/// for bit, each point where its structure's full solve puts it; at the
/// stringing state nothing moves; the lattice towers stay close to level
/// 3 (millimetres), the steel pole's P-Delta makes it move more. The
/// table is printed (theory: Structure flexibility, example numbers).
#[test]
fn the_example_line_at_level_four() {
    let (line, library, types) = example();
    let sections = ohl_sections::SectionLibrary::default();
    let budget = ohl_wirefe::default_fe_budget();
    let cases = [
        still(15.0),
        still(-20.0),
        iced(),
        windy(450.0),
        windy(600.0),
    ];
    let started = std::time::Instant::now();
    let level3 = solve_line_flexible_run(&line, &library, &types, &sections, &cases, 3);
    let t3 = started.elapsed();
    let started = std::time::Instant::now();
    let level4 = solve_line_flexible_run(&line, &library, &types, &sections, &cases, 4);
    let t4 = started.elapsed();
    assert_eq!(level4.level, CalculationLevel::Full);
    let analyses = Analyses::load(&types, &sections);
    let full = crate::full::prepare_full(
        &line,
        &types,
        analyses,
        &level4.setup.structures.flexible,
        &[],
        &[],
        None,
        None,
    );
    assert!(full.linear_only().is_empty(), "{:?}", full.linear_only());
    let provider = full.for_case(None);
    println!(
        "level 3 in {t3:.1?}, level 4 in {t4:.1?} ({} cases)",
        cases.len()
    );
    for ((case, three), four) in cases.iter().zip(&level3.cases).zip(&level4.cases) {
        let three = three.as_ref().unwrap_or_else(|e| panic!("{e}"));
        let four = four.as_ref().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(four.level, CalculationLevel::Full);
        let plain = ohl_line::solve::solve_line_with(&line, &library, case, &budget);
        assert_eq!(format!("{plain:?}"), format!("{:?}", four.rigid));
        assert!(four.full_failed.is_empty(), "{:?}", four.full_failed);
        assert!(four.line_solves() <= 10, "{:?}", four.passes);
        at_the_full_answer(four, &level4.setup.structures.flexible, &*provider.0);
        let most = |s: &FlexibleSolve, id: &str| {
            s.points
                .iter()
                .filter(|p| p.structure == id)
                .map(|p| size(p.displacement))
                .fold(0.0, f64::max)
        };
        println!(
            "{:>14}: level 3 {} line solves {:.1?}, level 4 {} line solves ({} full solves) \
             {:.1?}; largest move mm T2 {:.1}/{:.1}, T4 {:.1}/{:.1}, T6 {:.1}/{:.1}; {}",
            case.name,
            three.line_solves(),
            three.elapsed,
            four.line_solves(),
            four.structure_solves.0,
            four.elapsed,
            most(three, "T2") * 1e3,
            most(four, "T2") * 1e3,
            most(three, "T4") * 1e3,
            most(four, "T4") * 1e3,
            most(three, "T6") * 1e3,
            most(four, "T6") * 1e3,
            four.change_from_level3().unwrap_or_default()
        );
    }
    // The stringing state: one line solve, nothing moves.
    let at_stringing = level4.cases[0].as_ref().unwrap();
    assert_eq!(at_stringing.line_solves(), 1);
    assert!(
        at_stringing
            .points
            .iter()
            .all(|p| size(p.displacement) < 1e-4)
    );
}

/// `ohl_loads::solve_line_flexible` at level `n`.
fn solve_line_flexible_run(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    cases: &[WeatherCase],
    n: u8,
) -> crate::FlexibleRun {
    crate::solve_line_flexible(
        line,
        library,
        types,
        sections,
        cases,
        CalculationLevel::from_number(n).unwrap(),
        &ohl_wirefe::default_fe_budget(),
    )
}

/// An invented structure `model` in place of placed structure `id` of
/// the example line (its points all on the model's node `node`, the
/// model's axes the line's): level 3 (its tangent at the stringing
/// state) and level 4 (its full model).
fn harness(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    id: &str,
    model: Model,
    node: usize,
) -> (Structures, Structures, crate::full::FullSetup) {
    harness_on(
        line,
        library,
        types,
        id,
        model,
        node,
        Vec::new(),
        Vec::new(),
    )
}

/// [`harness`] on foot movements: `feet` prescribed on the model (level
/// 4's full solves), `moves` each point's move by them (level 3's
/// [`LinearStructure::feet`], line axes).
#[allow(clippy::too_many_arguments)]
fn harness_on(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    id: &str,
    mut model: Model,
    node: usize,
    feet: Vec<ohl_structures_fe::Prescribed>,
    moves: Vec<[f64; 3]>,
) -> (Structures, Structures, crate::full::FullSetup) {
    let budget = ohl_structures_fe::default_budget();
    let reference = reference_forces(line, library, &ohl_wirefe::default_fe_budget());
    let at: Vec<(String, ohl_line::Point)> = reference
        .forces
        .iter()
        .filter(|((s, _), _)| s == id)
        .map(|((_, l), f)| (l.clone(), *f))
        .collect();
    assert!(!at.is_empty());
    for (l, _) in &at {
        model.joint_points.push(JointPoint {
            label: l.clone(),
            node,
            offset: [0.0; 3],
        });
    }
    let points: Vec<Attachment> = at.iter().map(|(l, _)| Attachment::of_type(l)).collect();
    let forces: Vec<[f64; 3]> = at.iter().map(|(_, f)| *f).collect();
    let full = FullModel::of_model(model, points);
    let strung = full.solve(&forces, &[], None, &budget).unwrap();
    let n = at.len();
    let c = &strung.tangent;
    let mut flexibility = nalgebra::DMatrix::<f64>::zeros(3 * n, 3 * n);
    for a in 0..n {
        for b in 0..n {
            for i in 0..3 {
                for j in 0..3 {
                    flexibility[(3 * a + i, 3 * b + j)] =
                        c.flexibility[(3 * c.place_of[a] + i, 3 * c.place_of[b] + j)];
                }
            }
        }
    }
    let linear = LinearStructure {
        structure: id.into(),
        labels: at.iter().map(|(l, _)| l.clone()).collect(),
        flexibility,
        reference: forces,
        body: vec![[0.0; 3]; n],
        feet: moves,
        approximate: false,
        notes: Vec::new(),
    };
    let mut placed = line.structure(id).unwrap().clone();
    placed.orientation_deg = 0.0;
    let t = types
        .get(placed.structure_type.as_deref().unwrap())
        .unwrap()
        .clone();
    let shared = crate::full::FullShared {
        analyses: Analyses::load(types, &ohl_sections::SectionLibrary::default()),
        structures: vec![crate::full::FullStructure {
            placed,
            t,
            model: Ok((full, strung.displacement, strung.inactive)),
            feet,
            cases: Vec::new(),
        }],
        cases: Vec::new(),
        weather: Vec::new(),
        progress: Mutex::new(None),
        elapsed: std::time::Duration::ZERO,
    };
    let three = Structures {
        flexible: vec![linear],
        ..Structures::default()
    };
    let mut four = three.clone();
    let setup = crate::full::FullSetup(Arc::new(shared));
    four.full = Some(setup.for_case(None));
    (three, four, setup)
}

fn solve(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    s: &Structures,
) -> FlexibleSolve {
    solve_line_flexible(
        line,
        library,
        case,
        &ohl_wirefe::default_fe_budget(),
        s,
        &FlexibleOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// A vertical frame column `l` high fixed at its foot (node 0, the top
/// node 1), area `a`, second moment `i`; first order without
/// self-weight unless `nonlinear`.
fn column(l: f64, a: f64, i: f64, nonlinear: bool) -> Model {
    let mut m = Model {
        sections: vec![Section::elastic("s", E, a, i)],
        ..Model::default()
    };
    m.analysis.nonlinear = nonlinear;
    m.analysis.self_weight = false;
    m.nodes.push(Node::fixed("foot", [0.0, 0.0, 0.0]));
    m.nodes.push(Node::free("top", [0.0, 0.0, l]));
    m.members
        .push(Member::new("pole", MemberKind::Frame, 0, 1, 0));
    m
}

/// The sum of the wires' forces at `id`'s points in `s` (line axes, N).
fn total(s: &FlexibleSolve, id: &str) -> [f64; 3] {
    let mut f = [0.0; 3];
    for p in s.points.iter().filter(|p| p.structure == id) {
        for (x, v) in f.iter_mut().zip(p.flexible.unwrap_or_default()) {
            *x += v;
        }
    }
    f
}

/// Plan §5.6 test 1: a linear structure (first order, no tension-only
/// member) — level 4 is level 3: the full answers at level 3's solution
/// agree with it, no line solve more, the same points.
#[test]
fn a_linear_structure_at_level_four_is_level_three() {
    let (line, library, types) = example();
    let (three, four, _) = harness(
        &line,
        &library,
        &types,
        "T6",
        column(12.0, 4e-3, 3e-4, false),
        1,
    );
    for case in [iced(), windy(450.0)] {
        let a = solve(&line, &library, &case, &three);
        let b = solve(&line, &library, &case, &four);
        assert_eq!(b.line_solves(), a.line_solves(), "{:?}", b.passes);
        let last = b.passes.last().unwrap();
        assert!(last.level == 4 && !last.solved && last.displacement_residual < 1e-9 + 1e-4);
        for (p, q) in a.points.iter().zip(&b.points) {
            assert!(size([0, 1, 2].map(|k| p.displacement[k] - q.displacement[k])) < 1e-9);
        }
        assert_eq!(b.structure_solves.0, 1);
        assert!(b.full_failed.is_empty());
    }
}

/// Foot movements at level 4 (merged with phase 6, 2026-11-17): a linear
/// column whose foot is moved 30 mm sideways and 10 mm down — its full
/// solves on the moved foot, level 3 adding the same rigid move — level
/// 4 equals level 3 to 10⁻⁹ m, every point moved by the foot's move and
/// the case's deflection.
#[test]
fn level_four_takes_the_foot_movements() {
    let (line, library, types) = example();
    let foot = [0.03, 0.0, -0.01, 0.0, 0.0, 0.0];
    let n = reference_forces(&line, &library, &ohl_wirefe::default_fe_budget())
        .forces
        .keys()
        .filter(|(s, _)| s == "T6")
        .count();
    let (three, four, _) = harness_on(
        &line,
        &library,
        &types,
        "T6",
        column(12.0, 4e-3, 3e-4, false),
        1,
        vec![ohl_structures_fe::Prescribed {
            node: 0,
            displacement: foot,
        }],
        vec![[foot[0], foot[1], foot[2]]; n],
    );
    let (plain, _, _) = harness(
        &line,
        &library,
        &types,
        "T6",
        column(12.0, 4e-3, 3e-4, false),
        1,
    );
    for case in [still(15.0), windy(450.0)] {
        let a = solve(&line, &library, &case, &three);
        let b = solve(&line, &library, &case, &four);
        let c = solve(&line, &library, &case, &plain);
        assert!(b.full_failed.is_empty(), "{:?}", b.full_failed);
        for ((p, q), r) in a.points.iter().zip(&b.points).zip(&c.points) {
            assert!(size([0, 1, 2].map(|k| p.displacement[k] - q.displacement[k])) < 1e-9);
            // Moved by the foot (the wires then pull a little less or
            // more: within a few millimetres of the plain move plus it).
            let moved = [0, 1, 2].map(|k| q.displacement[k] - r.displacement[k] - foot[k]);
            assert!(size(moved) < 5e-3, "{moved:?}");
        }
    }
}

/// Plan §5.6 test 2: a pole guyed towards where the wind pushes it, the
/// guy pretensioned (taut at the stringing state): level 3 holds the top
/// with the guy, level 4 finds the guy slack — the pole alone, its top's
/// move the cantilever's F L³/3EI across and F L/EA along it under the
/// converged forces, less its stringing state's (the hand model).
#[test]
fn a_guyed_pole_whose_guy_goes_slack() {
    let (line, library, types) = example();
    let case = windy(450.0);
    // Where the wind pushes T6's top (the rigid solve's change from the
    // stringing state, across).
    let (probe, _, _) = harness(
        &line,
        &library,
        &types,
        "T6",
        column(12.0, 4e-3, 3e-5, false),
        1,
    );
    let p = solve(&line, &library, &case, &probe);
    let mut push = [0.0; 3];
    let strung: [f64; 3] = probe.flexible[0]
        .reference
        .iter()
        .fold([0.0; 3], |a, f| [a[0] + f[0], a[1] + f[1], a[2] + f[2]]);
    for pt in p.points.iter() {
        for (x, v) in push.iter_mut().zip(pt.rigid.unwrap_or_default()) {
            *x += v;
        }
    }
    let h = (push[0] - strung[0]).hypot(push[1] - strung[1]);
    let d = [(push[0] - strung[0]) / h, (push[1] - strung[1]) / h];
    let (l, a, i) = (12.0, 4e-3, 3e-5);
    let mut m = column(l, a, i, false);
    m.sections.push(Section::elastic("guy", E, 1e-4, 0.0));
    m.nodes
        .push(Node::pinned("anchor", [8.0 * d[0], 8.0 * d[1], 0.0]));
    let mut guy = Member::new("guy", MemberKind::Cable, 1, 2, 1);
    guy.pretension = 2e3;
    m.members.push(guy);
    let (three, four, _) = harness(&line, &library, &types, "T6", m, 1);
    let a3 = solve(&line, &library, &case, &three);
    let a4 = solve(&line, &library, &case, &four);
    let top3 = a3.points[0].displacement;
    let top4 = a4.points[0].displacement;
    let across = |u: [f64; 3]| u[0] * d[0] + u[1] * d[1];
    assert!(
        across(top4) > 5.0 * across(top3).abs(),
        "level 3 {top3:?}, level 4 {top4:?}"
    );
    // The hand model: the pole alone under the converged forces, less
    // the stringing state's (guy taut: the full model's own solve there).
    let f = total(&a4, "T6");
    let cantilever = l.powi(3) / (3.0 * E * i);
    let hand_top = [f[0] * cantilever, f[1] * cantilever, f[2] * l / (E * a)];
    let shared = &three.flexible[0];
    // The full model's own solve at the stringing state (the guy taut).
    let reference = &shared.reference;
    let mut fm = column(l, a, i, false);
    fm.sections.push(Section::elastic("guy", E, 1e-4, 0.0));
    fm.nodes
        .push(Node::pinned("anchor", [8.0 * d[0], 8.0 * d[1], 0.0]));
    let mut g = Member::new("guy", MemberKind::Cable, 1, 2, 1);
    g.pretension = 2e3;
    fm.members.push(g);
    for lbl in &shared.labels {
        fm.joint_points.push(JointPoint {
            label: lbl.clone(),
            node: 1,
            offset: [0.0; 3],
        });
    }
    let full = FullModel::of_model(
        fm,
        shared
            .labels
            .iter()
            .map(|x| Attachment::of_type(x))
            .collect(),
    );
    let at_strung = full
        .solve(reference, &[], None, &ohl_structures_fe::default_budget())
        .unwrap();
    let at_case = full
        .solve(
            &a4.points
                .iter()
                .map(|p| p.flexible.unwrap())
                .collect::<Vec<_>>(),
            &[],
            None,
            &ohl_structures_fe::default_budget(),
        )
        .unwrap();
    assert_eq!(at_case.out, 1, "the guy slack");
    assert_eq!(at_strung.out, 0, "the guy taut at the stringing state");
    for k in 0..3 {
        let expected = hand_top[k] - at_strung.displacement[0][k];
        assert!(
            (top4[k] - expected).abs() < 2e-4,
            "{k}: {top4:?} against the hand model {expected}"
        );
    }
    println!(
        "guyed pole, wind 450 Pa: across the line level 3 {:.1} mm, level 4 {:.1} mm ({} line \
         solves)",
        across(top3) * 1e3,
        across(top4) * 1e3,
        a4.line_solves()
    );
}

/// A structure whose full solve fails in a case (a column whose buckling
/// load lies between its stringing state's and the iced case's vertical
/// loads, P-Delta): taken linear (level 3) there, listed failing with
/// why; the line goes on and converges; in a case it carries, level 4.
#[test]
fn a_structure_failing_at_level_four_is_taken_linear() {
    let (line, library, types) = example();
    let (probe, _, _) = harness(
        &line,
        &library,
        &types,
        "T6",
        column(6.0, 4e-3, 3e-4, false),
        1,
    );
    let up = |s: &FlexibleSolve| -total(s, "T6")[2];
    let strung = -probe.flexible[0]
        .reference
        .iter()
        .map(|f| f[2])
        .sum::<f64>();
    let ice = up(&solve(&line, &library, &iced(), &probe));
    assert!(ice > 1.3 * strung, "{ice} {strung}");
    // Its buckling load (a cantilever: π²EI/4L²) between the two, short
    // (stiff across for that load).
    let l = 2.0;
    let p_cr = 0.95 * ice;
    println!("vertical load {strung:.0} N strung, {ice:.0} N iced; buckling load {p_cr:.0} N");
    let i = p_cr * 4.0 * l * l / (std::f64::consts::PI.powi(2) * E);
    let (three, four, _) = harness(&line, &library, &types, "T6", column(l, 4e-3, i, true), 1);
    let failed = solve(&line, &library, &iced(), &four);
    assert_eq!(failed.full_failed.len(), 1, "{:?}", failed.notes);
    println!("failing column: {}", failed.full_failed[0].reason);
    assert!(
        failed
            .notes
            .iter()
            .any(|n| n.contains("failed at calculation level 4"))
    );
    let linear = solve(&line, &library, &iced(), &three);
    for (p, q) in linear.points.iter().zip(&failed.points) {
        assert!(size([0, 1, 2].map(|k| p.displacement[k] - q.displacement[k])) < 2e-4);
    }
    // Where it carries the loads: level 4, nothing failed.
    let fine = solve(&line, &library, &still(-20.0), &four);
    assert!(fine.full_failed.is_empty(), "{:?}", fine.full_failed);
}

/// An X panel at the pole's place, its diagonals tension-only (no
/// compression): at the stringing state one diagonal is out; the wind
/// pushes the top so that the other goes out — the loop's full answers
/// switch the set (warm-started pass to pass) and converge, each point
/// where the panel's full solve puts it.
#[test]
fn tension_only_members_switch_under_the_loop() {
    let (line, library, types) = example();
    let case = windy(450.0);
    let (probe, _, _) = harness(
        &line,
        &library,
        &types,
        "T6",
        column(12.0, 4e-3, 3e-5, false),
        1,
    );
    let p = solve(&line, &library, &case, &probe);
    let strung: [f64; 3] = probe.flexible[0]
        .reference
        .iter()
        .fold([0.0; 3], |a, f| [a[0] + f[0], a[1] + f[1], a[2] + f[2]]);
    let f = total(&p, "T6");
    let h = (f[0] - strung[0]).hypot(f[1] - strung[1]);
    // The panel's x along where the wind pushes, less.
    let e1 = [-(f[0] - strung[0]) / h, -(f[1] - strung[1]) / h];
    let at = |x: f64, z: f64| [x * e1[0], x * e1[1], z];
    let mut m = Model {
        sections: vec![Section::elastic("s", E, 2e-3, 0.0)],
        ..Model::default()
    };
    m.analysis.nonlinear = false;
    m.analysis.self_weight = false;
    m.analysis.tension_compression = ohl_structures_fe::TensionCompression::None;
    // Across the panel's plane.
    let e2 = [-e1[1], e1[0]];
    let out = |x: f64, z: f64| [x * e1[0] + 5.0 * e2[0], x * e1[1] + 5.0 * e2[1], z];
    m.nodes = vec![
        Node::pinned("A", at(0.0, 0.0)),
        Node::pinned("B", at(4.0, 0.0)),
        Node::free("C", at(4.0, 3.0)),
        Node::free("D", at(0.0, 3.0)),
        Node::pinned("C'", out(4.0, 3.0)),
        Node::pinned("D'", out(0.0, 3.0)),
    ];
    m.members = vec![
        Member::new("AD", MemberKind::Truss, 0, 3, 0),
        Member::new("BC", MemberKind::Truss, 1, 2, 0),
        Member::new("DC", MemberKind::Truss, 3, 2, 0),
        Member::new("AC", MemberKind::Tension, 0, 2, 0),
        Member::new("BD", MemberKind::Tension, 1, 3, 0),
        // Holding C and D across the plane.
        Member::new("CC'", MemberKind::Truss, 2, 4, 0),
        Member::new("DD'", MemberKind::Truss, 3, 5, 0),
    ];
    let (three, four, setup) = harness(&line, &library, &types, "T6", m, 3);
    let a4 = solve(&line, &library, &case, &four);
    assert!(a4.full_failed.is_empty(), "{:?}", a4.full_failed);
    let provider = &four.full.as_ref().unwrap().0;
    at_the_full_answer(&a4, &four.flexible, &**provider);
    // The set switched: the stringing state's against the converged
    // forces'.
    let (model, _, strung_set) = setup.0.structures[0].model.as_ref().unwrap();
    let forces: Vec<[f64; 3]> = a4.points.iter().map(|p| p.flexible.unwrap()).collect();
    let at_case = model
        .solve(&forces, &[], None, &ohl_structures_fe::default_budget())
        .unwrap();
    println!(
        "X panel sets: stringing {:?}, case {:?}",
        &strung_set[3..5],
        &at_case.inactive[3..5]
    );
    assert_ne!(strung_set[3..5], at_case.inactive[3..5]);
    let a3 = solve(&line, &library, &case, &three);
    println!(
        "X panel, wind 450 Pa: level 3 {:?} mm, level 4 {:?} mm ({} line solves, passes {:?})",
        a3.points[0].displacement.map(|x| (x * 1e4).round() / 10.0),
        a4.points[0].displacement.map(|x| (x * 1e4).round() / 10.0),
        a4.line_solves(),
        a4.passes
            .iter()
            .map(|p| (p.level, (p.displacement_residual * 1e4).round() / 10.0))
            .collect::<Vec<_>>()
    );
}
