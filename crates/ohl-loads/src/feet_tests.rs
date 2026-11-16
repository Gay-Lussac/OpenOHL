// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Measured foot movements (owner 2026-11-16) on the example line: the
//! lattice tower T2 (four pinned feet) with one foot settled, the pole T6
//! tilted at its base. Without a reference case the structures moved by
//! them alone are used by the level-2 solves (automatically, said); the
//! reference state includes them and goes out of date when they change;
//! the structure checks take each member's larger usage of with and
//! without them; the file keeps them (version 23).

use std::path::{Path, PathBuf};

use ohl_line::feet::{FootMovement, FootMovements};
use ohl_line::reference::{ReferenceStatus, StructureReference};

use super::reference::{FEET_LEVEL, check_reference, reference_bodies, solve_reference};
use super::*;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
}

fn example() -> (
    Line,
    ohl_cables::Library,
    ohl_structures::StructureLibrary,
    ohl_sections::SectionLibrary,
    Vec<WeatherCase>,
) {
    let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    let library = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let weather = ohl_weather::WeatherCases::load(&ex().join("weather-cases.ohlwc"))
        .unwrap()
        .cases;
    (
        line,
        library,
        types,
        ohl_sections::SectionLibrary::default(),
        weather,
    )
}

/// The settlement of T2's foot L0B (mm down) and the pole T6's tilt to
/// the right (degrees).
fn with_feet(line: &Line, settled_mm: f64, tilt_deg: f64) -> Line {
    let mut l = line.clone();
    for s in &mut l.structures {
        match s.id.as_str() {
            "T2" => {
                s.foot_movements = Some(FootMovements {
                    source: "test survey".into(),
                    date: "2026-05-12".into(),
                    feet: vec![FootMovement {
                        joint: "L0B".into(),
                        translation: [0.0, 0.0, -settled_mm / 1e3],
                        ..FootMovement::default()
                    }],
                });
            }
            "T6" => {
                s.foot_movements = Some(FootMovements {
                    feet: vec![FootMovement {
                        joint: "base".into(),
                        tilt_deg: [tilt_deg, 0.0],
                        ..FootMovement::default()
                    }],
                    ..FootMovements::default()
                });
            }
            _ => {}
        }
    }
    l
}

fn find<'a>(
    state: &'a ohl_line::reference::ReferenceState,
    s: &str,
    l: &str,
) -> Option<&'a ohl_line::reference::ReferencePoint> {
    state
        .points
        .iter()
        .find(|p| p.structure == s && p.label == l)
}

/// No reference case: the structures moved by their foot movements alone,
/// solved without loads, used by every level-2 solve and said; the pole's
/// points by the rigid tilt; a line without any as before, bit for bit.
#[test]
fn foot_movements_alone_move_the_points() {
    let (line, library, types, sections, weather) = example();
    let budget = ohl_wirefe::default_fe_budget();
    let everyday = weather.iter().find(|w| w.name == "everyday").unwrap();
    let plain = ohl_line::solve::solve_line_with(&line, &library, everyday, &budget);

    // Without foot movements: nothing set, the solve as before.
    let mut none = line.clone();
    assert_eq!(
        check_reference(&mut none, &library, &types, &sections, &weather),
        None
    );
    assert!(none.feet_state.state.is_none() && none.reference_note().is_none());

    let mut moved = with_feet(&line, 20.0, 0.2);
    // Read, not checked: not used.
    assert!(moved.reference_in_use().is_none());
    assert!(moved.reference_note().unwrap().contains("not checked"));
    assert_eq!(
        check_reference(&mut moved, &library, &types, &sections, &weather),
        None
    );
    let state = moved
        .reference_in_use()
        .expect("the feet alone in use")
        .clone();
    assert_eq!(state.level, FEET_LEVEL);
    assert!(state.rigid.is_empty(), "{:?}", state.rigid);
    assert!(
        moved
            .reference_note()
            .unwrap()
            .contains("measured foot movements alone")
    );
    // T2: its points moved (the settled foot drags its side down and
    // leans it), none of T1's.
    let t2: Vec<_> = state
        .points
        .iter()
        .filter(|p| p.structure == "T2")
        .collect();
    assert!(!t2.is_empty());
    assert!(t2.iter().any(|p| p.displacement[2] < -1e-3), "{t2:?}");
    assert!(state.points.iter().all(|p| p.structure != "T1"));
    // T6: the pole's points by θ × r — the top's move to the right of
    // the line (structure x) is θ times its height.
    let t6 = &moved.structures.iter().find(|s| s.id == "T6").unwrap();
    let t = types.get(t6.structure_type.as_deref().unwrap()).unwrap();
    for p in state.points.iter().filter(|p| p.structure == "T6") {
        let local = ohl_line::links::to_structure_axes(t6.orientation_deg, p.displacement);
        let r = t.point_local(&p.label, None).unwrap();
        let want = 0.2f64.to_radians() * r[2];
        assert!(
            (local[0] - want).abs() < 1e-9,
            "{} {local:?} {want}",
            p.label
        );
    }
    // The level-2 solve takes them.
    let solved = ohl_line::solve::solve_line_with(&moved, &library, everyday, &budget);
    assert_ne!(format!("{plain:?}"), format!("{solved:?}"));
    // The bodies drawn on their moved feet: T2's settled foot down by
    // 20 mm.
    let (bodies, failed) = reference_bodies(&moved, &types, &sections, &state, &weather);
    assert!(failed.is_empty(), "{failed:?}");
    let t2 = &bodies["T2"];
    let foot = t2
        .joints
        .iter()
        .find(|(at, _)| (at[0] - 2.5).abs() < 1e-9 && (at[1] + 2.5).abs() < 1e-9 && at[2] == 0.0)
        .expect("foot L0B");
    assert!((foot.1[2] + 0.02).abs() < 1e-12, "{foot:?}");
    assert!(!bodies["T6"].pole.is_empty());

    // Set aside on purpose (the structure loads, --structure-reference
    // ignore): as drawn, bit for bit.
    let drawn = moved.as_drawn("test");
    assert!(drawn.reference_in_use().is_none());
    assert_eq!(
        format!("{plain:?}"),
        format!(
            "{:?}",
            ohl_line::solve::solve_line_with(&drawn, &library, everyday, &budget)
        )
    );
    // A dragged structure: out of date until checked again.
    let mut dragged = moved.clone();
    dragged.reference_stale("a structure being moved");
    assert!(dragged.reference_in_use().is_none());
}

/// The reference state solved with foot movements: its points' moves the
/// case's deflection plus the feet's (to the coupling's change), the feet
/// in its fingerprint — changed, it is out of date and the feet alone
/// are used, said.
#[test]
fn the_reference_state_stands_on_the_moved_feet() {
    let (line, library, types, sections, weather) = example();
    let budget = ohl_wirefe::default_fe_budget();
    let solve = |l: &Line| {
        let mut l = l.clone();
        l.reference = Some(StructureReference::new("high wind"));
        solve_reference(
            &l,
            &library,
            &types,
            &sections,
            &weather,
            "high wind",
            &budget,
            None,
            None,
        )
        .unwrap()
    };
    let plain = solve(&line);
    let moved_line = with_feet(&line, 20.0, 0.2);
    let moved = solve(&moved_line);
    assert!(plain.state.fingerprint.feet.is_empty());
    assert!(!moved.state.fingerprint.feet.is_empty());
    // The feet alone on the same line.
    let mut alone = moved_line.clone();
    check_reference(&mut alone, &library, &types, &sections, &weather);
    let feet = alone.reference_in_use().unwrap().clone();
    // The pole: its points moved by the tilt on top of the case's lean
    // (linear: the sum, to the wires' answer to the moved points).
    let mut compared = 0;
    for p in moved.state.points.iter().filter(|p| p.structure == "T6") {
        let a = find(&plain.state, "T6", &p.label).unwrap().displacement;
        let f = find(&feet, "T6", &p.label).unwrap().displacement;
        for c in 0..3 {
            let sum = a[c] + f[c];
            assert!(
                (p.displacement[c] - sum).abs() < 0.2 * f[c].abs().max(1e-3),
                "{} {c}: {} against {sum}",
                p.label,
                p.displacement[c]
            );
        }
        compared += 1;
    }
    assert!(compared > 0);
    // Kept and checked: current, said on its moved feet.
    let mut kept = moved_line.clone();
    kept.reference = Some(StructureReference {
        state: Some(moved.state.clone()),
        ..StructureReference::new("high wind")
    });
    assert_eq!(
        check_reference(&mut kept, &library, &types, &sections, &weather),
        Some(ReferenceStatus::Current)
    );
    assert!(kept.feet_state.state.is_none());
    assert!(
        kept.reference_note()
            .unwrap()
            .contains("on their measured foot movements")
    );
    // The feet changed: out of date ("the foot movements"), the feet alone
    // used instead, said.
    let mut changed = with_feet(&kept, 25.0, 0.2);
    changed.reference.clone_from(&kept.reference);
    match check_reference(&mut changed, &library, &types, &sections, &weather) {
        Some(ReferenceStatus::Stale(why)) => assert!(why.contains("the foot movements"), "{why}"),
        other => panic!("{other:?}"),
    }
    let used = changed.reference_in_use().unwrap();
    assert_eq!(used.level, FEET_LEVEL);
    let note = changed.reference_note().unwrap();
    assert!(
        note.contains("out of date") && note.contains("foot movements alone"),
        "{note}"
    );
    // A state solved before the feet existed stays current on a line
    // without them (its fingerprint reads the same).
    let mut old = line.clone();
    old.reference = Some(StructureReference {
        state: Some(plain.state.clone()),
        ..StructureReference::new("high wind")
    });
    assert_eq!(
        check_reference(&mut old, &library, &types, &sections, &weather),
        Some(ReferenceStatus::Current)
    );
}

/// The structure checks: the lattice tower's usage on its settled foot is
/// never below its usage without (each member's larger kept), said; the
/// pole's on its tilted base likewise.
#[test]
fn the_structure_checks_take_the_larger_of_with_and_without() {
    let (line, library, types, _, weather) = example();
    let budget = ohl_wirefe::default_fe_budget();
    let cases = LoadCases::load(&ex().join("load-cases.ohllc"))
        .unwrap()
        .cases;
    let known: Vec<String> = cases.iter().map(|c| c.name.clone()).collect();
    let loads: Vec<CaseLoads> =
        structure_loads_each(&line, &library, &weather, &cases, &known, &budget)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
    let moved = with_feet(&line, 30.0, 0.5);
    let a = capacity::usage(&line, &types, &library, &weather, &loads);
    let b = capacity::usage(&moved, &types, &library, &weather, &loads);
    let mut higher = 0;
    for (ca, cb) in a.iter().zip(&b) {
        for (sa, sb) in ca.structures.iter().zip(&cb.structures) {
            for (x, y) in [(&sa.fe, &sb.fe), (&sa.pole, &sb.pole)] {
                let (Some(x), Some(y)) = (x, y) else { continue };
                assert!(
                    y.usage >= x.usage - 1e-12,
                    "{} {}: {} below {}",
                    ca.case,
                    sa.structure,
                    y.usage,
                    x.usage
                );
                if y.usage > x.usage + 1e-9 {
                    higher += 1;
                    eprintln!(
                        "{} {}: usage {:.4} → {:.4} on its moved feet",
                        ca.case, sa.structure, x.usage, y.usage
                    );
                }
            }
            if sb.structure == "T2" {
                assert!(
                    sb.notes.iter().any(|n| n.contains("foot movements")),
                    "{:?}",
                    sb.notes
                );
            }
        }
    }
    assert!(higher > 0, "a 30 mm settlement loads some member more");
}

/// The line file: version 23 with foot movements, read back the same; a
/// line without them as before; one not valid refused.
#[test]
fn the_file_keeps_them() {
    let (line, ..) = example();
    let moved = with_feet(&line, 20.0, 0.2);
    let text = moved.to_json();
    assert!(text.contains("\"version\": 23"), "{}", &text[..80]);
    assert!(text.contains("foot_movements"));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.structures, moved.structures);
    assert!(!line.to_json().contains("\"version\": 23"));
    let mut bad = moved.clone();
    bad.structures[1].foot_movements.as_mut().unwrap().feet[0].tilt_deg = [f64::NAN, 0.0];
    assert!(bad.validate().is_err());
}

/// The reference case's wind as OHL-Design shows it (owner 2026-11-16):
/// a case without its own direction from the left is solved and
/// fingerprinted from the right, as OHL-Design solves it — the same
/// state in both programs.
#[test]
fn the_reference_case_blows_from_the_right_as_shown() {
    let (line, library, types, sections, weather) = example();
    let windy = weather.iter().find(|w| w.name == "high wind").unwrap();
    let mut left = windy.clone();
    left.wind_from = ohl_weather::WindSide::Left;
    left.wind_direction = None;
    let mut right = left.clone();
    right.wind_from = ohl_weather::WindSide::Right;
    assert_eq!(super::reference::as_shown(&left), right);
    assert_eq!(super::reference::as_shown(&right), right);
    let a = super::reference::fingerprint(&line, &library, &types, &sections, &left);
    let b = super::reference::fingerprint(&line, &library, &types, &sections, &right);
    assert_eq!(a, b);
}
