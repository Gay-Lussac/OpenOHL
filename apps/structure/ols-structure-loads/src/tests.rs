// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::Path;

use ols_structures::{LibraryItem, StructureType};

use super::*;

/// An invented structure: a suspension point S (set 1), a strain point E
/// pulling ahead (set 2), two members, one nodal load.
const STRUCTURE: &str = r#"
id = "test"
name = "test"
kind = "lattice"

[[point]]
label = "S"
point = ["5 m", "0 m", "20 m"]

[[point]]
label = "E"
point = ["-5 m", "0 m", "20 m"]

[[wire]]
label = "WS"
set = 1

[[wire]]
label = "WE"
set = 2

[[string]]
label = "s1"
top = "S"
wire = "WS"
length = "2 m"

[[string]]
label = "s2"
top = "E"
wire = "WE"
length = "2 m"
strain = true
side = "ahead"

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

[[load]]
node = "b"
force = ["1 kN", "2 kN", "-3 kN"]
"#;

const RECIPES: &str = r#"
format = "ols-structure-load-cases"
version = 1
name = "test cases"

[[load_case]]
name = "unbalanced"
description = "40 % of the largest tension, ahead"
line_case = "everyday"
strength = 0.9
self_weight = 1.1

[[load_case.load]]
what = "vertical"
factor = 1.35
points = "all"

[[load_case.load]]
what = "tension_max"
one_sided = "40 %"
side = "ahead"
sets = [1, 2]

[[load_case.load]]
value = "2 kN"
direction = "x"
points = ["S"]
note = "a value of the case"

[[load_case.load]]
what = "wire_wind"
phases = ["L1"]

[[load_case]]
name = "wind on the body"

[[load_case.load]]
pressure = "0.5 kPa"
direction = "-x"
members = "all"
"#;

fn structure() -> StructureType {
    StructureType::parse(Path::new("test.olss"), STRUCTURE).unwrap()
}

fn line() -> LineValues {
    let v = |point: Option<&str>, set: Option<u32>, quantity, value| LineValue {
        case: "everyday".into(),
        point: point.map(str::to_owned),
        set,
        quantity,
        value,
    };
    LineValues {
        points: vec![
            LinePoint {
                label: "S".into(),
                sets: vec![1],
                phases: vec!["L1".into()],
            },
            LinePoint {
                label: "E".into(),
                sets: vec![2],
                phases: vec!["L2".into()],
            },
        ],
        values: vec![
            v(Some("S"), None, LineQuantity::Vertical, 10_000.0),
            v(Some("E"), None, LineQuantity::Vertical, 8_000.0),
            v(None, Some(1), LineQuantity::TensionBack, 20_000.0),
            v(None, Some(1), LineQuantity::TensionAhead, -25_000.0),
            v(None, Some(2), LineQuantity::TensionMax, 30_000.0),
        ],
    }
}

fn row<'a>(t: &'a LoadTable, case: &str, load: usize, label: &str) -> &'a LoadRow {
    t.rows
        .iter()
        .find(|r| r.case == case && r.load == Some(load) && r.label == label)
        .unwrap_or_else(|| panic!("{case} {load} {label}: {t:#?}"))
}

#[test]
fn a_file_reads_with_units_and_refuses_typos() {
    let f = StructureLoadCases::parse(RECIPES).unwrap();
    let c = &f.cases[0];
    assert_eq!((c.strength, c.self_weight), (0.9, 1.1));
    assert_eq!(c.loads[1].one_sided, Some(0.4));
    assert_eq!(c.loads[2].value, Some(2000.0));
    assert_eq!(c.loads[2].direction, Some(Direction::Right));
    assert_eq!(f.cases[1].loads[0].pressure, Some(500.0));
    assert_eq!(f.cases[1].loads[0].members, Some(Pick::All));
    for (bad, says) in [
        (RECIPES.replace("factor = 1.35", "factr = 1.35"), "factr"),
        (
            RECIPES.replace("strength = 0.9", "strenght = 0.9"),
            "strenght",
        ),
        (
            RECIPES.replace("format = \"ols-structure-load-cases\"", "format = \"x\""),
            "\"x\"",
        ),
        (RECIPES.replace("version = 1", "version = 9"), "newer"),
        (
            RECIPES.replace("direction = \"x\"", "direction = \"sideways\""),
            "sideways",
        ),
        (
            RECIPES.replace("direction = \"x\"\n", ""),
            "needs a direction",
        ),
        (
            RECIPES.replace("side = \"ahead\"\n", ""),
            "needs a direction or a side",
        ),
        (RECIPES.replace("\"40 %\"", "\"140 %\""), "one_sided"),
        (
            RECIPES.replace("points = \"all\"", "points = \"some\""),
            "\"some\"",
        ),
        (RECIPES.replace("points = \"all\"", ""), "say where"),
        (
            RECIPES.replace("members = \"all\"", "points = \"all\""),
            "needs members",
        ),
        (RECIPES.replace("wind on the body", "unbalanced"), "twice"),
        (
            RECIPES.replace("what = \"vertical\"", "what = \"vertcal\""),
            "vertcal",
        ),
    ] {
        let e = StructureLoadCases::parse(&bad).unwrap_err();
        assert!(e.contains(says), "{says}: {e}");
    }
}

#[test]
fn a_file_round_trips_through_toml_and_csv() {
    let f = StructureLoadCases::parse(RECIPES).unwrap();
    let text = f.to_toml();
    assert!(text.contains("value = \"2000 N\""), "{text}");
    assert!(text.contains("[[load_case.load]]"), "{text}");
    assert_eq!(StructureLoadCases::parse(&text).unwrap(), f);
    let mut cases = f.cases.clone();
    cases.push(LoadCaseRecipe::new("empty, with a comma"));
    cases[0].loads[2].note = "say \"hi\",\nover two lines".into();
    let csv = StructureLoadCases::cases_to_csv(&cases);
    assert_eq!(StructureLoadCases::cases_from_csv(&csv).unwrap(), cases);
    // As tables in a structure file.
    let mut s = structure();
    s.load_cases = StructureLoadCases::to_tables(&f.cases);
    assert_eq!(StructureLoadCases::from_structure(&s).unwrap(), f.cases);
}

#[test]
fn embedded_tables_are_checked() {
    let mut s = structure();
    let mut t = StructureLoadCases::to_tables(&StructureLoadCases::parse(RECIPES).unwrap().cases);
    t[0].insert("typo_key".into(), 1.into());
    s.load_cases = t;
    let e = StructureLoadCases::from_structure(&s).unwrap_err();
    assert!(e.contains("typo_key"), "{e}");
    s.load_cases = vec![toml::toml! { name = "model" }];
    let e = StructureLoadCases::from_structure(&s).unwrap_err();
    assert!(e.contains("[[load]]"), "{e}");
}

#[test]
fn targets_have_their_sets_sides_and_phases() {
    let t = targets(&structure(), &line());
    assert_eq!(t.len(), 2);
    assert_eq!(
        (t[0].label.as_str(), t[0].sets.as_slice(), t[0].side),
        ("S", &[1][..], None)
    );
    assert_eq!(
        (t[1].label.as_str(), t[1].sets.as_slice(), t[1].side),
        ("E", &[2][..], Some(Side::Ahead))
    );
    assert_eq!(t[0].phases, ["L1"]);
    assert!(
        targets(&structure(), &LineValues::default())[0]
            .phases
            .is_empty()
    );
}

#[test]
fn recipes_give_the_hand_values() {
    let f = StructureLoadCases::parse(RECIPES).unwrap();
    let t = evaluate(&f.cases, &structure(), &line());
    // 1.35 × vertical, at both points, from the line.
    assert_eq!(row(&t, "unbalanced", 1, "S").force, [0.0, 0.0, 13_500.0]);
    assert_eq!(row(&t, "unbalanced", 1, "E").force, [0.0, 0.0, 10_800.0]);
    assert_eq!(row(&t, "unbalanced", 1, "S").source, Source::Line);
    // 40 % of the largest tension, ahead: S from its set's back and
    // ahead tensions (25 kN), E its own (30 kN).
    assert_eq!(row(&t, "unbalanced", 2, "S").force, [0.0, 10_000.0, 0.0]);
    assert_eq!(row(&t, "unbalanced", 2, "E").force, [0.0, 12_000.0, 0.0]);
    // A value of the case.
    let r = row(&t, "unbalanced", 3, "S");
    assert_eq!((r.force, r.source), ([2000.0, 0.0, 0.0], Source::Recipe));
    // Phase L1 (S) has no wind value.
    let r = row(&t, "unbalanced", 4, "S");
    assert_eq!(
        r.missing.as_deref(),
        Some("no line value: wire_wind in \"everyday\"")
    );
    assert_eq!(r.force, [0.0; 3]);
    // No areas: the surface load is marked.
    assert_eq!(
        row(&t, "wind on the body", 1, "m1").missing.as_deref(),
        Some("no area")
    );
    // With areas: 500 Pa × area, towards −x.
    let areas = |id: &str| (id == "m1").then_some(2.0);
    let t2 = evaluate_with_areas(&f.cases, &structure(), &line(), &areas);
    assert_eq!(
        row(&t2, "wind on the body", 1, "m1").force,
        [-1000.0, 0.0, 0.0]
    );
    assert_eq!(
        row(&t2, "wind on the body", 1, "m2").missing.as_deref(),
        Some("no area")
    );
    // [[load]] as case "model": z up becomes vertical down.
    let m = row(&t, MODEL_CASE, 1, "b");
    assert_eq!(
        (m.at, m.force, m.source),
        (At::Joint, [1000.0, 2000.0, 3000.0], Source::Typed)
    );
    // Totals per place.
    let tot = t.totals();
    assert_eq!(
        tot[&("unbalanced".into(), At::Point, "S".into())],
        [2000.0, 10_000.0, 13_500.0]
    );
}

#[test]
fn a_structure_alone_has_rows_without_line_values() {
    let f = StructureLoadCases::parse(RECIPES).unwrap();
    let t = evaluate(&f.cases, &structure(), &LineValues::default());
    assert!(
        row(&t, "unbalanced", 1, "S")
            .missing
            .as_deref()
            .unwrap()
            .starts_with("no line value")
    );
    // The phase is not known without the line.
    let r = t.case("unbalanced").find(|r| r.load == Some(4)).unwrap();
    assert_eq!(r.label, "");
    assert!(
        r.missing
            .as_deref()
            .unwrap()
            .contains("phases come from the line")
    );
    // A value of the case needs no line.
    assert!(row(&t, "unbalanced", 3, "S").missing.is_none());
    assert_eq!(t.cases(), ["unbalanced", "wind on the body", MODEL_CASE]);
}

#[test]
fn typed_values_are_kept_and_marked() {
    let f = StructureLoadCases::parse(RECIPES).unwrap();
    let mut t = evaluate(&f.cases, &structure(), &line());
    let typed = vec![
        Override {
            case: "unbalanced".into(),
            load: Some(1),
            at: At::Point,
            label: "S".into(),
            force: [0.0, 0.0, 15_000.0],
        },
        Override {
            case: "client".into(),
            load: None,
            at: At::Point,
            label: "E".into(),
            force: [1.0, 2.0, 3.0],
        },
    ];
    t.apply(&typed);
    let r = row(&t, "unbalanced", 1, "S");
    assert_eq!(r.force, [0.0, 0.0, 15_000.0]);
    assert_eq!(r.source, Source::Typed);
    assert_eq!(r.evaluated, Some([0.0, 0.0, 13_500.0]));
    assert_eq!(t.case("client").count(), 1);
    assert_eq!(t.overrides(), typed);
    // Evaluated again from the recipes, the typed values come back.
    let mut again = evaluate(&f.cases, &structure(), &line());
    again.apply(&t.overrides());
    assert_eq!(again, t);
}

#[test]
fn the_table_round_trips_through_csv_and_json() {
    let f = StructureLoadCases::parse(RECIPES).unwrap();
    let mut t = evaluate(&f.cases, &structure(), &line());
    t.apply(&[Override {
        case: "unbalanced".into(),
        load: Some(2),
        at: At::Point,
        label: "E".into(),
        force: [0.1, 1.0 / 3.0, -2.5e-7],
    }]);
    t.rows[0].note = "a, \"quoted\" note".into();
    let csv = t.to_csv();
    assert_eq!(LoadTable::from_csv(&csv).unwrap(), t);
    let json = serde_json::to_string(&t).unwrap();
    assert_eq!(serde_json::from_str::<LoadTable>(&json).unwrap(), t);
    let lv = line();
    let json = serde_json::to_string(&lv).unwrap();
    assert_eq!(serde_json::from_str::<LineValues>(&json).unwrap(), lv);
    // Columns missing, unknown, or a value not read.
    for (bad, says) in [
        ("case,load\n", "no column"),
        (
            &*csv.replacen("note", "note,notes", 1),
            "unknown column \"notes\"",
        ),
        (&*csv.replacen(",point,", ",pt,", 1), "row 2"),
    ] {
        let e = LoadTable::from_csv(bad).unwrap_err();
        assert!(e.contains(says), "{says}: {e}");
    }
}

#[test]
fn a_table_read_back_types_only_what_differs() {
    let f = StructureLoadCases::parse(RECIPES).unwrap();
    let evaluated = evaluate(&f.cases, &structure(), &line());
    // As evaluated: nothing typed.
    assert!(evaluated.overrides_over(&evaluated).is_empty());
    let typed = vec![
        Override {
            case: "unbalanced".into(),
            load: Some(1),
            at: At::Point,
            label: "S".into(),
            force: [0.0, 0.0, 15_000.0],
        },
        Override {
            case: "client".into(),
            load: None,
            at: At::Point,
            label: "E".into(),
            force: [1.0, 2.0, 3.0],
        },
        // Over the model case's row.
        Override {
            case: MODEL_CASE.into(),
            load: Some(1),
            at: At::Joint,
            label: "b".into(),
            force: [1.0, 0.0, 0.0],
        },
    ];
    let mut t = evaluated.clone();
    t.apply(&typed);
    // Written as CSV and read back: the same overrides, the same table.
    let back = LoadTable::from_csv(&t.to_csv()).unwrap();
    let mut over = back.overrides_over(&evaluated);
    over.sort_by(|a, b| a.case.cmp(&b.case));
    let mut want = typed.clone();
    want.sort_by(|a, b| a.case.cmp(&b.case));
    assert_eq!(over, want);
    let mut again = evaluated.clone();
    again.apply(&back.overrides_over(&evaluated));
    assert_eq!(again, t);
    // Kept as a table of typed rows.
    let kept = Override::table(&typed);
    assert!(kept.rows.iter().all(|r| r.source == Source::Typed));
    let read: Vec<Override> = LoadTable::from_csv(&kept.to_csv())
        .unwrap()
        .rows
        .iter()
        .map(Override::of)
        .collect();
    assert_eq!(read, typed);
}

/// A line-values file of version 1 (no sources) still reads, every case a
/// load case; version 2 marks the weather cases sent on their own and
/// reads back the same.
#[test]
fn line_values_files_mark_the_source_of_each_case() {
    let v1 = r#"{"format": "ols-structure-line-values", "version": 1, "structure": "T2",
        "cases": ["everyday", "wind"]}"#;
    let old = PlacedValues::parse(v1).unwrap();
    assert!(old.sources.is_empty());
    assert_eq!(old.source("wind"), CaseSource::LoadCase);
    let mut new = PlacedValues::pending("T2", "line", "type-02", "");
    new.cases = vec!["wind".into(), "weather: glaze ice".into()];
    new.sources = vec![CaseSource::LoadCase, CaseSource::WeatherCase];
    let text = new.to_json();
    assert!(text.contains("\"version\": 3"), "{text}");
    assert!(text.contains("\"weather_case\""), "{text}");
    // Nothing missing: no `missing` written.
    assert!(!text.contains("missing"), "{text}");
    let read = PlacedValues::parse(&text).unwrap();
    assert_eq!(read, new);
    assert_eq!(read.source("weather: glaze ice"), CaseSource::WeatherCase);
    assert_eq!(read.source("wind").label(), "load case");
    let newer = text.replace("\"version\": 3", "\"version\": 4");
    assert!(PlacedValues::parse(&newer).unwrap_err().contains("newer"));
}

/// Version 3: per case the sections and jumpers at the structure that did
/// not solve; a case with some is incomplete, the others not; a version 2
/// file has none.
#[test]
fn line_values_files_carry_the_wires_missing_per_case() {
    let v2 = r#"{"format": "ols-structure-line-values", "version": 2, "structure": "T2",
        "cases": ["everyday", "ice"], "sources": ["load_case", "load_case"]}"#;
    let old = PlacedValues::parse(v2).unwrap();
    assert!(old.missing.is_empty());
    assert_eq!(old.incomplete("ice"), None);
    let mut new = PlacedValues::pending("T2", "line", "type-02", "");
    new.cases = vec!["everyday".into(), "ice".into()];
    new.missing = vec![MissingWire {
        case: "ice".into(),
        kind: "section".into(),
        id: "S12".into(),
        reason: "did not converge".into(),
    }];
    let text = new.to_json();
    assert!(text.contains("\"missing\""), "{text}");
    let read = PlacedValues::parse(&text).unwrap();
    assert_eq!(read, new);
    assert_eq!(
        read.incomplete("ice").as_deref(),
        Some("incomplete: S12 not solved")
    );
    assert_eq!(read.incomplete("everyday"), None);
    assert_eq!(read.incomplete_cases(), ["ice"]);
    assert_eq!(
        read.missing[0].describe(),
        "section S12 not solved (did not converge)"
    );
}
