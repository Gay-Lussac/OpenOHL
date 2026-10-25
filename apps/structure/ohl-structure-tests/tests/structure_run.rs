// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure application's results through the link
//! (`ohl_structure_link::analyse_structure`): solved on the extension
//! chosen, the case's pressure on the extension's piece too, and the pole
//! check of the line (`ohl_structures_fe::check_pole`) for a structure
//! with a pole. References derived by hand in the comments.

use std::path::Path;

use ohl_structure_link::{RunInput, SectionAreas, analyse_structure, cases_to_run};
use ohl_structure_loads::{LineValues, StructureLoadCases, evaluate_with_areas};
use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::{PoleLoad, check_pole, default_budget};

/// A 10 m round tube column (frame, fixed foot) with a +2 m extension
/// piece of the same tube under it, and a case with a pressure of
/// 1000 Pa across (x) on every member, no self-weight.
const COLUMN: &str = r#"
format = "ohl-structure"
version = 3
id = "col"
name = "column"
kind = "other"

[[point]]
label = "A1"
node = "N1"

[[node]]
id = "N0"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N1"
point = ["0 m", "0 m", "10 m"]

[[member]]
id = "m1"
nodes = ["N0", "N1"]
section = "CHS"
kind = "frame"

[[extension]]
name = "+2"
dz = "2 m"

[[extension.node]]
id = "E0"
point = ["0 m", "0 m", "-2 m"]
support = "fixed"

[[extension.member]]
id = "e1"
nodes = ["E0", "N0"]
section = "CHS"
kind = "frame"

[[load_case]]
name = "wind"
self_weight = 0

[[load_case.load]]
pressure = "1000 Pa"
direction = "x"
members = "all"
"#;

const TUBE: &str = "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\n\
    density = \"7850 kg/m3\"\nyield = \"355 MPa\"\n\
    [[section]]\nid = \"CHS\"\nshape = \"circular_hollow\"\nmaterial = \"steel\"\n\
    d = \"200 mm\"\nt = \"10 mm\"\n";

fn parse(text: &str) -> StructureType {
    StructureType::parse(Path::new("s.ohls"), text).unwrap()
}

/// As written the pressure loads m1 only: 1000 Pa × 0.2 m × 10 m = 2 kN,
/// half at each end; the foot's moment 1 kN × 10 m = 10 kN·m. On +2 the
/// piece e1 takes 1000 × 0.2 × 2 = 0.4 kN too (the table lists the body
/// as written): Σ 2.4 kN, at the piece's foot E0 (N0 lost its support)
/// the moment 1 kN × 12 m + (1 + 0.2) kN × 2 m = 14.4 kN·m (as the
/// distributed loads: 2 kN × 7 m + 0.4 kN × 1 m).
#[test]
fn the_case_is_solved_on_the_extension_chosen() {
    let lib = ohl_sections::SectionLibrary::parse(TUBE).unwrap();
    let t = parse(COLUMN);
    let recipes = StructureLoadCases::from_structure(&t).unwrap();
    let areas = SectionAreas::new(&t, COLUMN, None, &lib);
    let table = evaluate_with_areas(&recipes, &t, &LineValues::default(), &areas);
    let cases = cases_to_run(&table, &recipes, None);
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].self_weight, 0.0);
    let run = |extension: Option<&str>| {
        analyse_structure(
            &RunInput {
                structure: &t,
                text: COLUMN,
                dir: None,
                sections: &lib,
                table: &table,
                recipes: &recipes,
                cases: &cases,
                extension,
            },
            &default_budget(),
        )
        .unwrap()
    };
    let written = run(None);
    assert_eq!(written.extension, None);
    assert!(written.pole.is_none());
    let r = written.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
    assert!((r.load_sum[0] - 2000.0).abs() < 1e-6, "{:?}", r.load_sum);
    assert_eq!(r.reactions.len(), 1);
    assert_eq!(r.reactions[0].joint, "N0");
    assert!((r.reactions[0].moment[1] + 10_000.0).abs() < 1e-3, "{r:?}");
    assert_eq!(written.sections.len(), 1);
    assert!((written.sections[0].length - 10.0).abs() < 1e-9);

    let on = run(Some("+2"));
    assert_eq!(on.extension.as_deref(), Some("+2"));
    assert!((on.dz - 2.0).abs() < 1e-12);
    let r = on.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(r.members.len(), 2);
    assert!((r.load_sum[0] - 2400.0).abs() < 1e-6, "{:?}", r.load_sum);
    assert_eq!(r.reactions.len(), 1);
    assert_eq!(r.reactions[0].joint, "E0");
    assert!((r.reactions[0].force[0] + 2400.0).abs() < 1e-6);
    assert!((r.reactions[0].moment[1] + 14_400.0).abs() < 1e-3, "{r:?}");
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("extension +2") && n.contains("1 member(s), 0.40 kN")),
        "{:?}",
        r.notes
    );
    assert!((on.sections[0].length - 12.0).abs() < 1e-9);
    // An extension that is not there.
    let bad = analyse_structure(
        &RunInput {
            structure: &t,
            text: COLUMN,
            dir: None,
            sections: &lib,
            table: &table,
            recipes: &recipes,
            cases: &cases,
            extension: Some("+9"),
        },
        &default_budget(),
    );
    assert!(bad.unwrap_err().contains("+9"));
}

/// The example pole structure (type-05: the steel pole of
/// steel-pole-30m.ohlp, point A1 at 26 m on the axis) with a case of
/// 5 kN across, 1 kN ahead and 8 kN down at A1: the pole check is the
/// line's, and its first-order ground moment is 26 m × √(5² + 1²) kN =
/// 132.57 kN·m (the vertical load on the axis gives none). With a
/// pressure of 800 Pa to −x on all members it is the wind on the pole:
/// the same as `check_pole` with the wind (−800, 0).
#[test]
fn the_pole_check_of_the_line() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line/Structures");
    let base = std::fs::read_to_string(dir.join("type-05.ohls"))
        .unwrap()
        .replace("version = 2", "version = 3");
    let lib = ohl_sections::SectionLibrary::default();
    let pole = ohl_structures_fe::Pole::load(&dir.join("steel-pole-30m.ohlp")).unwrap();
    let loads = "\n[[load_case]]\nname = \"c1\"\nstrength = 0.9\n\
        [[load_case.load]]\nvalue = \"5 kN\"\ndirection = \"x\"\npoints = [\"A1\"]\n\
        [[load_case.load]]\nvalue = \"1 kN\"\ndirection = \"y\"\npoints = [\"A1\"]\n\
        [[load_case.load]]\nvalue = \"8 kN\"\ndirection = \"-z\"\npoints = [\"A1\"]\n";
    for (wind, extra) in [
        ([0.0, 0.0], ""),
        (
            [-800.0, 0.0],
            "[[load_case.load]]\npressure = \"800 Pa\"\ndirection = \"-x\"\nmembers = \"all\"\n",
        ),
        // The case's wind on the body (2026-10-25): 500 Pa × 1.6 from the
        // right, the same wind on the pole; its weight on members not
        // (said).
        (
            [-800.0, 0.0],
            "[[load_case.structure.wind]]\nfrom = \"right\"\npressure = \"500 Pa\"\n\
             factor = 1.6\n[[load_case.structure.ice]]\nload = \"10 N/m\"\n",
        ),
    ] {
        let text = format!("{base}{loads}{extra}");
        let t = parse(&text);
        let recipes = StructureLoadCases::from_structure(&t).unwrap();
        let table = ohl_structure_loads::evaluate(&recipes, &t, &LineValues::default());
        let cases = cases_to_run(&table, &recipes, None);
        let run = analyse_structure(
            &RunInput {
                structure: &t,
                text: &text,
                dir: Some(&dir),
                sections: &lib,
                table: &table,
                recipes: &recipes,
                cases: &cases,
                extension: None,
            },
            &default_budget(),
        )
        .unwrap();
        assert_eq!(run.pole.as_ref().unwrap().as_ref().unwrap().id, pole.id);
        let c = &run.cases[0];
        assert!(c.fe.is_none(), "a pole without members: no model");
        let p = c.pole.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(p.wind, wind);
        let direct = check_pole(
            &pole,
            &[PoleLoad {
                point: t.point_local("A1", None).unwrap(),
                force: [5000.0, 1000.0, -8000.0],
            }],
            wind,
            0.9,
            true,
            &default_budget(),
        )
        .unwrap();
        assert_eq!(
            (p.usage, p.moment, p.height, p.ground_moment),
            (
                direct.usage,
                direct.moment,
                direct.height,
                direct.ground_moment
            )
        );
        if wind == [0.0, 0.0] {
            let hand = 26.0 * 5000f64.hypot(1000.0);
            assert!((p.ground_moment - hand).abs() < 1e-6, "{}", p.ground_moment);
            assert_eq!(p.wind_moment, 0.0);
        } else {
            assert!(p.wind_moment > 0.0);
            assert!(
                p.notes
                    .iter()
                    .any(|n| n.contains("wind on the pole: 800 Pa")
                        || n.contains("wind on the body from the right, 500 Pa × 1.6")),
                "{:?}",
                p.notes
            );
            if extra.contains("structure") {
                assert!(
                    p.notes
                        .iter()
                        .any(|n| n.contains("ice on members: not on the pole"))
                );
            }
        }
        let checks = run.checks();
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].check, "pole");
        assert_eq!(checks[0].case, "c1");
        assert_eq!(checks[0].usage, Some(p.usage));
    }
}

/// A cantilever crossarm (L = 3 m along x, a frame, fixed at the mast M)
/// with attachment point A1 linked to its tip T at an offset e = 0.5 m
/// ahead (y), 10 kN down at A1: the run lists the offset's moment
/// r × F = (0, 0.5, 0) × (0, 0, −10 kN) = (−5, 0, 0) kN·m, applied at T
/// (a frame meets it), as the results panel shows it (2026-10-21; the
/// same as validation case structfe-013).
#[test]
fn the_offset_moment_of_each_point_is_reported() {
    let text = r#"
format = "ohl-structure"
version = 6
id = "arm"
name = "arm"
kind = "other"

[analysis]
nonlinear = false
self_weight = false

[[node]]
id = "M"
point = ["0 m", "0 m", "20 m"]
support = "fixed"

[[node]]
id = "T"
point = ["3 m", "0 m", "20 m"]
kind = "attach"

[[member]]
id = "arm"
nodes = ["M", "T"]
section = "CHS"
kind = "frame"

[[point]]
label = "A1"
node = "T"
offset = ["0 m", "0.5 m", "0 m"]

[[load_case]]
name = "down"
self_weight = 0

[[load_case.load]]
value = "10 kN"
direction = "-z"
points = ["A1"]
"#;
    let lib = ohl_sections::SectionLibrary::parse(TUBE).unwrap();
    let t = parse(text);
    let recipes = StructureLoadCases::from_structure(&t).unwrap();
    let table = ohl_structure_loads::evaluate(&recipes, &t, &LineValues::default());
    let cases = cases_to_run(&table, &recipes, None);
    let run = analyse_structure(
        &RunInput {
            structure: &t,
            text,
            dir: None,
            sections: &lib,
            table: &table,
            recipes: &recipes,
            cases: &cases,
            extension: None,
        },
        &default_budget(),
    )
    .unwrap();
    let r = run.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(r.points.len(), 1, "{:?}", r.points);
    let p = &r.points[0];
    assert_eq!(p.label, "A1");
    assert!((p.moment[0] + 5000.0).abs() < 1e-6, "{:?}", p.moment);
    assert!(p.moment[1].abs() < 1e-9 && p.moment[2].abs() < 1e-9);
    assert!((p.size() - 5000.0).abs() < 1e-6);
    assert!(p.applied && p.note.is_none());
    // At the mast: M_x = F·e = 5 kN·m twisting the arm.
    let m = r.reactions.iter().find(|x| x.joint == "M").unwrap();
    assert!((m.moment[0] - 5000.0).abs() < 1e-3, "{:?}", m.moment);
}
