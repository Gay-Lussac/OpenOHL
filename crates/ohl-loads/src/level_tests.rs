// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure loads at calculation level 3 (plan of 2026-11-15, phase
//! 4): the example line rigid against level 3 (§4.8 test 6), the owner's
//! rule (never below rigid), the "to verify" rows of the theory page's
//! safe-side table, and the usage check with the redistributed forces.

use std::path::{Path, PathBuf};

use super::*;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

/// The example line (typed), its wires, weather, the example load cases
/// and three more: still air at the maximum temperature, a broken
/// conductor in every span of S1 under glaze ice, and rime ice with
/// wind.
fn example() -> (
    Line,
    ohl_cables::Library,
    ohl_structures::StructureLibrary,
    Vec<WeatherCase>,
    Vec<LoadCase>,
) {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let library = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let weather = ohl_weather::WeatherCases::load(&ex().join("weather-cases.ohlwc"))
        .unwrap()
        .cases;
    let mut cases = LoadCases::load(&ex().join("load-cases.ohllc"))
        .unwrap()
        .cases;
    let more = LoadCases::parse(
        "format = \"ohl-load-cases\"\nversion = 3\n\
         [[case]]\nname = \"hot\"\nweather = \"max temperature\"\n\
         [[case]]\nname = \"broken\"\nweather = \"glaze ice\"\nlongitudinal = 1.0\n\
         [case.broken]\nsection = \"S1\"\nspan = \"every\"\n\
         [[case]]\nname = \"rime\"\nweather = \"rime ice + wind\"\ncondition = \"creep\"\n",
    )
    .unwrap();
    cases.extend(more.cases);
    (line, library, types, weather, cases)
}

fn find<'a>(loads: &'a [AttachmentLoad], s: &str, p: &str) -> Option<&'a AttachmentLoad> {
    loads.iter().find(|l| l.structure == s && l.point == p)
}

/// The example line rigid against level 3, every case: the level's
/// rigid solve is the rigid run's bit for bit; each load at a point is
/// per component the larger of the two by size (never below rigid); the
/// table of changes (largest move, largest change per structure) kept as
/// a regression; the "to verify" rows answered (theory: Safe side).
#[test]
fn the_example_line_rigid_against_level_three() {
    let (line, library, types, weather, cases) = example();
    let sections = ohl_sections::SectionLibrary::default();
    let budget = ohl_wirefe::default_fe_budget();
    let known: Vec<String> = cases.iter().map(|c| c.name.clone()).collect();
    let rigid: Vec<CaseLoads> =
        structure_loads_each(&line, &library, &weather, &cases, &known, &budget)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let tell = |_: usize, _: usize| {
        done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    };
    let setup = prepare_structure_loads(
        &line,
        &library,
        &types,
        &sections,
        &weather,
        &cases,
        &budget,
        Some(&tell),
        None,
    );
    // Every placed structure told once (those without a wire solved by
    // finite elements too).
    assert_eq!(
        done.load(std::sync::atomic::Ordering::Relaxed),
        line.structures.len()
    );
    let flexible: Vec<&str> = setup
        .setup
        .structures
        .flexible
        .iter()
        .map(|s| s.structure.as_str())
        .collect();
    assert_eq!(flexible, ["T2", "T4", "T6"]);
    let level: Vec<CaseLoads> = structure_loads_at(
        &line,
        &library,
        &weather,
        &cases,
        &known,
        &budget,
        Some(&setup),
    )
    .into_iter()
    .collect::<Result<_, _>>()
    .unwrap();
    let mut table = String::new();
    let axes = ["transverse", "longitudinal", "vertical"];
    for (r, f) in rigid.iter().zip(&level) {
        let l = f.level.as_ref().expect("computed at level 3");
        assert!(l.failed.is_none(), "{}: {:?}", f.case.name, l.failed);
        assert!(
            f.notes.iter().any(|n| n.contains("unfactored loads")),
            "{:?}",
            f.notes
        );
        // The level's rigid solve is the rigid run.
        if r.broken_each.is_empty() {
            assert_eq!(format!("{:?}", l.rigid), format!("{:?}", r.loads));
        } else {
            assert_eq!(l.rigid_each.len(), r.broken_each.len());
            for (a, b) in l.rigid_each.iter().zip(&r.broken_each) {
                assert_eq!(format!("{:?}", a.loads), format!("{:?}", b.loads));
            }
        }
        // The flexible loads at the same points as the rigid ones (the
        // points' moves are no lever: found on a real line, a moved point
        // was first taken as a post's lever, a load of its own).
        let labels = |v: &[AttachmentLoad]| -> Vec<(String, String)> {
            v.iter()
                .map(|l| (l.structure.clone(), l.label.clone()))
                .collect()
        };
        assert_eq!(labels(&l.rigid), labels(&l.flexible), "{}", f.case.name);
        for (a, b) in l.rigid_each.iter().zip(&l.flexible_each) {
            assert_eq!(labels(&a.loads), labels(&b.loads), "{}", f.case.name);
        }
        assert_eq!(labels(&r.loads), labels(&f.loads), "{}", f.case.name);
        // Never below rigid, per point and component.
        for x in &r.loads {
            let y = find(&f.loads, &x.structure, &x.point).expect("a load at every rigid point");
            for k in 0..3 {
                assert!(
                    y.factored[k].abs() >= x.factored[k].abs(),
                    "{} {} {}: {} below rigid {}",
                    f.case.name,
                    x.structure,
                    x.point,
                    y.factored[k],
                    x.factored[k]
                );
            }
        }
        // The table.
        for s in &flexible {
            let m = l.largest_move(s).map_or([0.0; 3], |m| m.local);
            let (p, k, a, b) = l.largest_change(s).unwrap_or_default();
            let _ = writeln!(
                table,
                "{:<8} {s}  move {:>7.1} {:>7.1} {:>7.1} mm   {p} {:<12} {:>9.1} → {:>9.1} N ({:+.2} %)",
                f.case.name,
                m[0] * 1e3,
                m[1] * 1e3,
                m[2] * 1e3,
                axes[k],
                a,
                b,
                if a != 0.0 {
                    (b - a) / a.abs() * 100.0
                } else {
                    0.0
                }
            );
        }
    }
    println!("{table}");
    let case = |name: &str| level.iter().position(|c| c.case.name == name).unwrap();
    // Still air as strung ("everyday", the stringing temperature): no
    // move (u = 0 exactly), the flexible loads the rigid ones.
    let everyday = level[case("everyday")].level.as_ref().unwrap();
    assert!(everyday.moves.iter().all(|m| m.local == [0.0; 3]));
    assert_eq!(
        format!("{:?}", everyday.rigid),
        format!("{:?}", everyday.flexible)
    );
    // High wind: the structures' own wind in their deflection — the
    // lattice towers' points move leeward by centimetres, the pole's by
    // decimetres; the transverse loads nearly unchanged (load
    // controlled; theory: neutral).
    let wind = level[case("wind")].level.as_ref().unwrap();
    for m in &wind.moves {
        let least = if m.structure == "T6" { 0.1 } else { 0.01 };
        assert!(m.local[0] < -least, "{m:?}");
    }
    for s in ["T2", "T4"] {
        let (_, k, a, b) = wind.largest_change(s).unwrap();
        if k == 0 {
            assert!((b - a).abs() < 0.02 * a.abs(), "{s}: {a} → {b}");
        }
    }
    // The broken conductor (S1, every span, glaze ice): the other wires
    // on the towers next to the break now share its unbalance — the
    // longitudinal load at S2's point of T2 (a phase that did not break)
    // changes from the rigid solve's when span 2 (T2–T3) breaks, where
    // rigid gives it none of the unbalance (theory: "the other phases'
    // points … not conservative" answered).
    let broken = level[case("broken")].level.as_ref().unwrap();
    let span2 = |each: &[BrokenLoads]| -> f64 {
        let b = each.iter().find(|b| b.span == 2).unwrap();
        b.loads
            .iter()
            .filter(|l| l.structure == "T2" && l.sets.contains(&11))
            .map(|l| l.factored[1])
            .sum()
    };
    let (other_rigid, other_flexible) = (span2(&broken.rigid_each), span2(&broken.flexible_each));
    println!(
        "S2 at T2, S1 span 2 broken: longitudinal {other_rigid:.1} N rigid, {other_flexible:.1} N level 3"
    );
    assert!(
        (other_flexible - other_rigid).abs() > 1.0,
        "{other_rigid} {other_flexible}"
    );
    // Farther from the break: span 1 (T1–T2) broken, the longitudinal
    // load at T4 (beyond the rigid T3), rigid and level 3 (theory:
    // "possibly not conservative", measured).
    let at_t4 = |each: &[BrokenLoads], span: usize| -> f64 {
        let b = each.iter().find(|b| b.span == span).unwrap();
        b.loads
            .iter()
            .filter(|l| l.structure == "T4")
            .map(|l| l.factored[1])
            .sum()
    };
    let (far_rigid, far_flexible) = (
        at_t4(&broken.rigid_each, 1),
        at_t4(&broken.flexible_each, 1),
    );
    println!(
        "T4, S1 span 1 broken: longitudinal {far_rigid:.1} N rigid, {far_flexible:.1} N level 3"
    );
    // The vertical loads: the largest change over every case and point.
    let mut vertical: f64 = 0.0;
    for f in &level {
        let l = f.level.as_ref().unwrap();
        for x in &l.rigid {
            if let Some(y) = find(&l.flexible, &x.structure, &x.point)
                && x.factored[2].abs() > 1.0
            {
                vertical = vertical.max(((y.factored[2] - x.factored[2]) / x.factored[2]).abs());
            }
        }
    }
    println!(
        "largest change of a vertical load: {:.3} %",
        vertical * 100.0
    );
    assert!(vertical < 0.02, "{vertical}");
    // The sags (weather cases, the structures of the load cases): hot
    // (below the stringing tension) rigid sags more; iced, rigid sags
    // less.
    let fe_line = {
        let mut l = line.clone();
        let fe = ohl_line::condition::Calculation::ALL
            .into_iter()
            .find(|c| {
                c.condition() == ohl_line::condition::Condition::Initial
                    && c.method() == ohl_line::condition::Method::Fe
            })
            .unwrap();
        for s in &mut l.sections {
            s.calculation = fe;
        }
        l.set_weather_cases(&weather);
        l
    };
    for (load_case, weather_case) in [("hot", "max temperature"), ("ice", "glaze ice")] {
        let w = weather.iter().find(|w| w.name == weather_case).unwrap();
        let solved = ohl_line::solve::flexible::solve_line_flexible(
            &fe_line,
            &library,
            w,
            &budget,
            &setup.structures_for(load_case),
            &ohl_line::solve::flexible::FlexibleOptions::default(),
        )
        .unwrap();
        let worst = solved
            .spans
            .iter()
            .max_by(|a, b| {
                (a.sag[1] - a.sag[0])
                    .abs()
                    .total_cmp(&(b.sag[1] - b.sag[0]).abs())
            })
            .unwrap();
        println!(
            "{weather_case}: largest sag change {} span {}: {:.4} → {:.4} m",
            worst.section,
            worst.span + 1,
            worst.sag[0],
            worst.sag[1]
        );
        if load_case == "hot" {
            assert!(worst.sag[1] <= worst.sag[0], "{worst:?}");
        } else {
            assert!(worst.sag[1] >= worst.sag[0], "{worst:?}");
        }
    }
}

/// The usage check with the redistributed forces: at level 3 each
/// structure's finite-element and pole checks take the rigid and the
/// flexible solve each on its own and keep the worse — never below the
/// rigid usage; the governing set said in the check.
#[test]
fn the_usage_takes_the_worse_of_rigid_and_level_three() {
    let (line, library, types, weather, cases) = example();
    let sections = ohl_sections::SectionLibrary::default();
    let budget = ohl_wirefe::default_fe_budget();
    let known: Vec<String> = cases.iter().map(|c| c.name.clone()).collect();
    let rigid: Vec<CaseLoads> =
        structure_loads_each(&line, &library, &weather, &cases, &known, &budget)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
    let setup = prepare_structure_loads(
        &line, &library, &types, &sections, &weather, &cases, &budget, None, None,
    );
    let level: Vec<CaseLoads> = structure_loads_at(
        &line,
        &library,
        &weather,
        &cases,
        &known,
        &budget,
        Some(&setup),
    )
    .into_iter()
    .collect::<Result<_, _>>()
    .unwrap();
    let a = capacity::usage(&line, &types, &library, &weather, &rigid);
    let b = capacity::usage(&line, &types, &library, &weather, &level);
    let mut flexible_governs = Vec::new();
    for (ca, cb) in a.iter().zip(&b) {
        for (sa, sb) in ca.structures.iter().zip(&cb.structures) {
            assert_eq!(sa.structure, sb.structure);
            for (x, y) in [
                (&sa.fe, &sb.fe),
                (&sa.pole, &sb.pole),
                (&sa.loads, &sb.loads),
            ] {
                let (Some(x), Some(y)) = (x, y) else {
                    assert_eq!(x.is_some(), y.is_some(), "{} {}", ca.case, sa.structure);
                    continue;
                };
                assert!(
                    y.usage >= x.usage - 1e-12,
                    "{} {}: level 3 {} below rigid {}",
                    ca.case,
                    sa.structure,
                    y.usage,
                    x.usage
                );
                if y.governs.contains("(level 3)") {
                    flexible_governs.push(format!(
                        "{} {}: {:.4} → {:.4}",
                        ca.case, sa.structure, x.usage, y.usage
                    ));
                }
            }
        }
    }
    println!("level 3 governs: {flexible_governs:?}");
    // OHL-Structure gets both solves as cases of their own, each with
    // its level.
    let split = level[1].each_break();
    let names: Vec<&str> = split.iter().map(|c| c.case.name.as_str()).collect();
    assert_eq!(names, ["wind (rigid)", "wind (level 3)"]);
    assert_eq!(level[1].each_level(), [2, 3]);
    assert_eq!(rigid[1].each_level(), [2]);
}
