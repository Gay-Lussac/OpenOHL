// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A case's two parts (line forces, loads on the structure itself): read,
//! checked, evaluated against values worked out by hand, converted from
//! the older form, mirrored and said in words. Every value is invented.

use std::path::Path;

use ohl_structures::{LibraryItem, StructureType};

use super::*;

/// Four points: A1, A2, A3 carrying sets 1, 2, 3, P none; two members.
const STRUCTURE: &str = r#"
id = "test"
name = "test"
kind = "lattice"

[[point]]
label = "A1"
point = ["5 m", "0 m", "20 m"]

[[point]]
label = "A2"
point = ["-5 m", "0 m", "20 m"]

[[point]]
label = "A3"
point = ["-5 m", "0 m", "25 m"]

[[point]]
label = "P"
point = ["0 m", "0 m", "30 m"]

[[wire]]
label = "W1"
set = 1

[[wire]]
label = "W2"
set = 2

[[wire]]
label = "W3"
set = 3

[[string]]
label = "s1"
top = "A1"
wire = "W1"
length = "2 m"

[[string]]
label = "s2"
top = "A2"
wire = "W2"
length = "2 m"

[[string]]
label = "s3"
top = "A3"
wire = "W3"
length = "2 m"

[[node]]
id = "a"
point = ["0 m", "0 m", "0 m"]

[[node]]
id = "b"
point = ["0 m", "0 m", "20 m"]

[[member]]
id = "m1"
nodes = ["a", "b"]

[[member]]
id = "m2"
nodes = ["b", "a"]
"#;

const LEFT: &str = "storm — wind from the left";
const RIGHT: &str = "storm — wind from the right";

const CASES: &str = r#"
format = "ohl-structure-load-cases"
version = 3

[[load_case]]
name = "ULS wind left"
description = """Ultimate limit state, wind from the left.
Invented factors."""
strength = 1.0

[[load_case.line]]
result = "storm — wind from the left"
vertical = 1.35
transverse = 1.5
longitudinal = 1.0
off = ["A3"]

[load_case.structure]
self_weight = 1.35

[[load_case.structure.wind]]
from = "left"
pressure = "650 Pa"
factor = 1.5

[[load_case.structure.ice]]
thickness = "20 mm"
density = "900 kg/m³"
members = "all"

[[load_case.structure.point]]
value = "2 kN"
direction = "down"
points = ["A1"]
"#;

fn structure() -> StructureType {
    StructureType::parse(Path::new("test.ohls"), STRUCTURE).unwrap()
}

/// The line's forces at A1, A2, A3 in both line results: transverse,
/// longitudinal, vertical (N), the left blowing right, the right left.
fn line() -> LineValues {
    let mut values = Vec::new();
    for (case, sign) in [(LEFT, 1.0), (RIGHT, -1.0)] {
        for (label, f) in [
            ("A1", [1000.0, 200.0, 5000.0]),
            ("A2", [800.0, -100.0, 4000.0]),
            ("A3", [700.0, 0.0, 3000.0]),
        ] {
            for (q, v) in [
                (LineQuantity::Transverse, f[0] * sign),
                (LineQuantity::Longitudinal, f[1]),
                (LineQuantity::Vertical, f[2]),
            ] {
                values.push(LineValue {
                    case: case.into(),
                    point: Some(label.into()),
                    set: None,
                    quantity: q,
                    value: v,
                });
            }
        }
    }
    let points = [("A1", 1), ("A2", 2), ("A3", 3)]
        .map(|(l, s)| LinePoint {
            label: l.into(),
            sets: vec![s],
            phases: Vec::new(),
        })
        .to_vec();
    LineValues {
        points,
        values,
        structure_wind: [(LEFT, Direction::Right), (RIGHT, Direction::Left)]
            .map(|(case, towards)| CaseWind {
                case: case.into(),
                pressure: 600.0,
                factor: 1.0,
                towards,
                origin: String::new(),
            })
            .to_vec(),
    }
}

/// Invented member data: m1 and m2 20 m long, a perimeter of 0.2 m; the
/// wind on a member is pressure × 0.1 m × 20 m along the wind, + its
/// widening × 20 m × pressure (what the link's model does is tested
/// there).
struct Fake;

impl MemberAreas for Fake {
    fn area(&self, _: &str) -> Option<f64> {
        Some(2.0)
    }

    fn member_ids(&self) -> Option<Vec<String>> {
        Some(vec!["m1".into(), "m2".into()])
    }

    fn member_wind(
        &self,
        pressure: f64,
        towards: [f64; 3],
        widened: &dyn Fn(&str) -> f64,
    ) -> Result<BodyWind, String> {
        Ok(BodyWind {
            members: ["m1", "m2"]
                .map(|id| {
                    let a = (0.1 + widened(id)) * 20.0;
                    (id.to_owned(), Ok(towards.map(|t| t * pressure * a)))
                })
                .to_vec(),
            notes: vec!["invented member wind".into()],
        })
    }

    fn perimeter(&self, _: &str) -> Result<f64, String> {
        Ok(0.2)
    }

    fn length(&self, _: &str) -> Result<f64, String> {
        Ok(20.0)
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

fn row<'a>(t: &'a LoadTable, load: usize, label: &str) -> &'a LoadRow {
    t.rows
        .iter()
        .find(|r| r.load == Some(load) && r.label == label)
        .unwrap_or_else(|| panic!("{load} {label}: {t:#?}"))
}

#[test]
fn the_parts_read_check_and_set_the_version() {
    let set = StructureLoadCases::parse(CASES).unwrap();
    let c = &set.cases[0];
    assert_eq!(c.line.len(), 1);
    assert_eq!(c.line[0].factors(), [1.5, 1.0, 1.35]);
    assert_eq!(c.line[0].off, ["A3"]);
    let own = c.structure.as_ref().unwrap();
    assert_eq!(own.self_weight, Some(1.35));
    assert_eq!(c.self_weight_factor(), 1.35);
    assert_eq!(own.wind[0].from, WindFrom::Left);
    assert_eq!(own.wind[0].pressure, Some(650.0));
    assert!(close(own.ice[0].thickness.unwrap(), 0.02));
    assert_eq!(own.ice[0].density, Some(900.0));
    assert_eq!(own.ice[0].members, Pick::All);
    assert_eq!(own.point[0].value, 2000.0);
    assert!(c.description.contains('\n'));
    // Written at 3, read back the same.
    let again = StructureLoadCases::new("x", set.cases.clone());
    assert_eq!(again.version, Some(3));
    assert_eq!(
        StructureLoadCases::parse(&again.to_toml()).unwrap().cases,
        set.cases
    );
    // The older form alone stays at version 1.
    assert_eq!(written_version(&[LoadCaseRecipe::new("plain")]), 1);
    // A version 2 program says "newer".
    let newer = CASES.replace("version = 3", "version = 4");
    assert!(
        StructureLoadCases::parse(&newer)
            .unwrap_err()
            .contains("newer")
    );
    // Refused, with what is wrong.
    for (from, to, says) in [
        ("off = [\"A3\"]", "of = [\"A3\"]", "unknown field `of`"),
        (
            "factor = 1.5",
            "factor = 1.5\npressure_of = \"x\"",
            "pressure or pressure_of, not both",
        ),
        (
            "density = \"900 kg/m³\"\n",
            "",
            "a thickness needs a density",
        ),
        ("from = \"left\"", "from = \"up\"", "unknown variant `up`"),
        (
            "strength = 1.0",
            "strength = 1.0\nself_weight = 1.2",
            "the self weight is given twice",
        ),
        ("points = [\"A1\"]", "", "structure point 1: say where"),
        (
            "result = \"storm — wind from the left\"",
            "result = \" \"",
            "line 1: line forces without a result",
        ),
    ] {
        let e = StructureLoadCases::parse(&CASES.replace(from, to)).unwrap_err();
        assert!(e.contains(says), "{from} → {to}: {e}");
    }
}

/// Line forces: each axis × its factor at every point carrying a wire,
/// A3 off (its row kept, no force), P (no wire) not there; sets pick.
#[test]
fn line_forces_take_a_factor_per_axis_and_points_off() {
    let cases = StructureLoadCases::parse(CASES).unwrap().cases;
    let t = evaluate_with_areas(&cases, &structure(), &line(), &Fake);
    let n = Part::Line.number(0);
    assert_eq!(n, 1001);
    let a1 = row(&t, n, "A1");
    assert_eq!(a1.source, Source::Line);
    assert!(close(a1.force[0], 1500.0) && close(a1.force[1], 200.0));
    assert!(close(a1.force[2], 1.35 * 5000.0));
    assert!(a1.note.contains("vertical × 1.35"), "{}", a1.note);
    let a3 = row(&t, n, "A3");
    assert_eq!((a3.force, a3.missing.as_deref()), ([0.0; 3], None));
    assert!(a3.note.starts_with("off"), "{}", a3.note);
    assert!(!t.rows.iter().any(|r| r.load == Some(n) && r.label == "P"));
    // Only sets 2 and 3, none off: A2 and A3.
    let mut c = cases[0].clone();
    c.line[0].sets = vec![2, 3];
    c.line[0].off.clear();
    let t = evaluate_with_areas(&[c.clone()], &structure(), &line(), &Fake);
    let labels: Vec<&str> = t
        .rows
        .iter()
        .filter(|r| r.load == Some(n))
        .map(|r| r.label.as_str())
        .collect();
    assert_eq!(labels, ["A2", "A3"]);
    assert!(close(row(&t, n, "A3").force[0], 1.5 * 700.0));
    // A result the line does not send: rows marked, no force.
    c.line[0].result = "calm".into();
    let t = evaluate_with_areas(&[c.clone()], &structure(), &line(), &Fake);
    let r = row(&t, n, "A2");
    assert_eq!(
        r.missing.as_deref(),
        Some("no line value: transverse, longitudinal, vertical in \"calm\"")
    );
    // A point off that is not there is said.
    c.line[0].off = vec!["Z9".into()];
    let t = evaluate_with_areas(&[c], &structure(), &line(), &Fake);
    assert!(row(&t, n, "Z9").missing.as_deref().unwrap().contains("off"));
}

/// The wind on the body, the ice and the point loads: rows of their own
/// parts, by hand with the invented members.
#[test]
fn the_loads_on_the_structure_itself_are_rows_of_their_parts() {
    let cases = StructureLoadCases::parse(CASES).unwrap().cases;
    let t = evaluate_with_areas(&cases, &structure(), &line(), &Fake);
    // Wind: 650 Pa × 1.5 from the left (to +x), each member 0.1 m + 2 ×
    // 20 mm of ice wide, 20 m long.
    let w = row(&t, Part::Wind.number(0), "m1");
    assert_eq!(w.source, Source::Wind);
    assert!(close(w.force[0], 650.0 * 1.5 * 0.14 * 20.0), "{w:?}");
    assert_eq!((w.force[1], w.force[2]), (0.0, 0.0));
    assert!(w.note.contains("iced: 40 mm wider"), "{}", w.note);
    assert!(w.note.contains("invented member wind"));
    // Ice: ρ g (P t + π t²) × L, down.
    let i = row(&t, Part::Ice.number(0), "m2");
    let per_m =
        900.0 * ohl_core::units::STANDARD_GRAVITY * (0.2 * 0.02 + std::f64::consts::PI * 4e-4);
    assert_eq!(i.source, Source::Ice);
    assert!(close(i.force[2], per_m * 20.0), "{i:?}");
    // A point load: 2 kN down at A1.
    let p = row(&t, Part::Point.number(0), "A1");
    assert_eq!((p.source, p.force), (Source::Point, [0.0, 0.0, 2000.0]));
    // A load per metre on one member, no widening for the wind.
    let mut c = cases[0].clone();
    let own = c.structure.as_mut().unwrap();
    own.ice[0] = IceLoad {
        thickness: None,
        density: None,
        load: Some(15.0),
        members: Pick::These(vec!["m1".into()]),
        factor: 2.0,
    };
    let t = evaluate_with_areas(&[c.clone()], &structure(), &line(), &Fake);
    assert_eq!(
        row(&t, Part::Ice.number(0), "m1").force[2],
        15.0 * 20.0 * 2.0
    );
    assert!(
        !t.rows
            .iter()
            .any(|r| r.load == Some(3001) && r.label == "m2")
    );
    assert!(close(
        row(&t, Part::Wind.number(0), "m1").force[0],
        650.0 * 1.5 * 0.1 * 20.0
    ));
    // 45°: from ahead left, to the right and back.
    let own = c.structure.as_mut().unwrap();
    own.wind[0].from = WindFrom::AheadLeft;
    let t = evaluate_with_areas(&[c.clone()], &structure(), &line(), &Fake);
    let f = row(&t, Part::Wind.number(0), "m1").force;
    assert!(f[0] > 0.0 && close(f[1], -f[0]));
    assert!(close(f[0].hypot(f[1]), 650.0 * 1.5 * 0.1 * 20.0));
    // The pressure of a line result: its weather case's, unfactored.
    let own = c.structure.as_mut().unwrap();
    own.wind[0].pressure = None;
    own.wind[0].pressure_of = Some(RIGHT.into());
    let t = evaluate_with_areas(&[c.clone()], &structure(), &line(), &Fake);
    assert!(close(
        row(&t, Part::Wind.number(0), "m1").force[0]
            .hypot(row(&t, Part::Wind.number(0), "m1").force[1]),
        600.0 * 1.5 * 0.1 * 20.0
    ));
    // Resolved for the run: the pressure written in.
    let run = resolve_line_pressures(&[c.clone()], &line());
    let w = &run[0].structure.as_ref().unwrap().wind[0];
    assert_eq!((w.pressure, w.pressure_of.as_deref()), (Some(600.0), None));
    // No line: the wind row says why.
    let t = evaluate_with_areas(&[c], &structure(), &LineValues::default(), &Fake);
    let r = row(&t, Part::Wind.number(0), "");
    assert!(r.missing.as_deref().unwrap().starts_with("no line value"));
    // No members known: rows marked, the solve leaves them out.
    let t = evaluate(&cases, &structure(), &line());
    assert!(
        row(&t, Part::Wind.number(0), "")
            .missing
            .as_deref()
            .unwrap()
            .starts_with("no member wind")
    );
    assert!(
        row(&t, Part::Ice.number(0), "m1")
            .missing
            .as_deref()
            .unwrap()
            .starts_with("no outline")
    );
}

/// A value typed over a part's row stays on it when an older load is
/// added before (the parts are numbered in blocks of their own); the
/// table goes out and back as CSV with the new sources.
#[test]
fn typed_values_keep_matching_the_parts_rows() {
    let cases = StructureLoadCases::parse(CASES).unwrap().cases;
    let t = evaluate_with_areas(&cases, &structure(), &line(), &Fake);
    let over = Override {
        force: [1.0, 2.0, 3.0],
        ..Override::of(row(&t, 1001, "A2"))
    };
    let mut c = cases[0].clone();
    c.loads.push(LoadRecipe {
        value: Some(5.0),
        direction: Some(Direction::Up),
        points: Some(Pick::All),
        ..LoadRecipe::default()
    });
    let mut t = evaluate_with_areas(&[c], &structure(), &line(), &Fake);
    t.apply(&[over]);
    let r = row(&t, 1001, "A2");
    assert_eq!((r.force, r.source), ([1.0, 2.0, 3.0], Source::Typed));
    assert_eq!(LoadTable::from_csv(&t.to_csv()).unwrap(), t);
    assert_eq!(Part::row_name(1001), "line 1");
    assert_eq!(Part::row_name(3002), "ice 2");
    assert_eq!(Part::row_name(4), "load 4");
    assert_eq!(Part::of(2001), Some((Part::Wind, 0)));
    // The recipes' CSV holds the older form only.
    assert!(
        StructureLoadCases::cases_to_csv(&cases)
            .unwrap_err()
            .contains(".ohlslc")
    );
}

#[test]
fn the_case_is_said_in_words() {
    let cases = StructureLoadCases::parse(CASES).unwrap().cases;
    assert_eq!(
        describe(&cases[0]),
        "storm — wind from the left: vertical × 1.35, transverse × 1.5, longitudinal × 1.0 at \
         every point (without A3) · self weight × 1.35 · wind on the body from the left, \
         650 Pa × 1.5 · ice 20 mm (900 kg/m³) on every member × 1.0 · 2 kN down at A1"
    );
    let mut c = LoadCaseRecipe::new("plain");
    c.line.push(LineForces {
        sets: vec![1, 2],
        ..LineForces::new("calm")
    });
    c.loads.push(LoadRecipe::default());
    assert_eq!(
        describe(&c),
        "calm: vertical × 1.0, transverse × 1.0, longitudinal × 1.0 at the points of sets 1, 2 \
         · self weight × 1.0 · 1 load in the older form"
    );
}

/// Mirror wind: left and right swapped in the name, the line results and
/// the body wind; ahead and back stay; the points off stay.
#[test]
fn mirror_wind_swaps_left_and_right() {
    let mut c = StructureLoadCases::parse(CASES).unwrap().cases.remove(0);
    let own = c.structure.as_mut().unwrap();
    own.wind.push(BodyWindLoad {
        from: WindFrom::BackLeft,
        pressure: None,
        pressure_of: Some(LEFT.into()),
        factor: 1.0,
    });
    own.wind.push(BodyWindLoad {
        from: WindFrom::Ahead,
        pressure: Some(100.0),
        pressure_of: None,
        factor: 1.0,
    });
    let m = mirror_wind(&c);
    assert_eq!(m.name, "ULS wind right");
    assert!(
        m.description
            .starts_with("Ultimate limit state, wind from the right.")
    );
    assert_eq!(m.line[0].result, RIGHT);
    assert_eq!(m.line[0].off, ["A3"]);
    let w = &m.structure.as_ref().unwrap().wind;
    assert_eq!(w[0].from, WindFrom::Right);
    assert_eq!(
        (w[1].from, w[1].pressure_of.as_deref()),
        (WindFrom::BackRight, Some(RIGHT))
    );
    assert_eq!(w[2].from, WindFrom::Ahead);
    assert_eq!(mirror_wind(&m).line, c.line);
    // A name without a side.
    assert_eq!(
        mirror_wind(&LoadCaseRecipe::new("ULS")).name,
        "ULS (mirrored)"
    );
}

/// Convert: the line's unfactored values of one line case at the same
/// points become one line forces row, a value a point load, the self
/// weight the structure's; the rest stays, said.
#[test]
fn an_older_case_converts_where_it_is_exact() {
    let text = r#"
[[load_case]]
name = "old"
line_case = "weather: ice"
self_weight = 1.2

[[load_case.load]]
what = "vertical"
factor = 1.3
points = "all"

[[load_case.load]]
what = "transverse"
factor = 0.5
points = "all"

[[load_case.load]]
what = "transverse"
factor = 0.25
points = "all"

[[load_case.load]]
what = "vertical"
sets = [2]

[[load_case.load]]
value = "2 kN"
direction = "y"
factor = 2
points = ["A1"]

[[load_case.load]]
what = "tension_max"
one_sided = "40 %"
side = "ahead"
sets = [1]

[[load_case.load]]
what = "vertical_factored"
points = "all"
"#;
    let old = StructureLoadCases::parse(text).unwrap().cases.remove(0);
    let done = convert_older(&old, &[]);
    let c = &done.case;
    assert_eq!(c.line.len(), 2);
    assert_eq!(c.line[0].result, "ice");
    assert_eq!(c.line[0].factors(), [0.75, 0.0, 1.3]);
    assert!(c.line[0].sets.is_empty());
    assert_eq!(
        (c.line[1].sets.as_slice(), c.line[1].vertical),
        (&[2][..], 1.0)
    );
    let own = c.structure.as_ref().unwrap();
    assert_eq!(own.self_weight, Some(1.2));
    assert_eq!(c.self_weight, 1.0);
    assert_eq!(c.self_weight_factor(), 1.2);
    assert_eq!(own.point[0].value, 4000.0);
    assert_eq!(own.point[0].direction, Direction::Ahead);
    // Kept: the one-sided tension and the factored value, said.
    assert_eq!(c.loads.len(), 2);
    assert_eq!(done.kept.len(), 2);
    assert!(done.kept[0].starts_with("load 6:"), "{:?}", done.kept);
    assert!(done.kept[1].contains("factored"), "{:?}", done.kept);
    assert_eq!(c.line_case.as_deref(), Some("weather: ice"));
    c.check().unwrap();
    // The same loads as before: the table of the converted case (its
    // parts) gives the older case's forces.
    let mut line = line();
    for v in &mut line.values {
        v.case = "weather: ice".into();
    }
    let before = evaluate(std::slice::from_ref(&old), &structure(), &line);
    for v in &mut line.values {
        v.case = "ice".into();
    }
    let after = evaluate(std::slice::from_ref(c), &structure(), &line);
    let sum = |t: &LoadTable, from: usize, to: usize| {
        let mut s = [0.0; 3];
        for r in t
            .rows
            .iter()
            .filter(|r| r.missing.is_none() && r.load.is_some_and(|l| (from..=to).contains(&l)))
        {
            for (a, f) in s.iter_mut().zip(r.force) {
                *a += f;
            }
        }
        s
    };
    let (b, a) = (sum(&before, 1, 5), sum(&after, 1001, 4999));
    for (x, y) in a.iter().zip(b) {
        assert!(close(*x, y), "{a:?} {b:?}");
    }
    // A weather case the line sends twice now: kept, said.
    let results = vec![
        "ice — wind from the left".to_owned(),
        "ice — wind from the right".to_owned(),
    ];
    let done = convert_older(&old, &results);
    assert!(done.case.line.is_empty());
    assert!(done.kept[0].contains("choose one"), "{:?}", done.kept);
}
