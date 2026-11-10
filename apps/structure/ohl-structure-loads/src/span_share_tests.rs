// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Line forces taking a share of the back and ahead spans' pull (`back`,
//! `ahead`, percent; `.ohlslc` version 4, line-values file version 6;
//! owner 2026-11-10), against values worked out by hand. Every value is
//! invented.

use std::path::Path;

use ohl_structures::{LibraryItem, StructureType};

use super::*;

/// Three points A1, A2, A3 carrying sets 1, 2, 3.
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
"#;

fn structure() -> StructureType {
    StructureType::parse(Path::new("test.ohls"), STRUCTURE).unwrap()
}

const T: LineQuantity = LineQuantity::Transverse;
const L: LineQuantity = LineQuantity::Longitudinal;
const V: LineQuantity = LineQuantity::Vertical;
const BT: LineQuantity = LineQuantity::PullBackTransverse;
const BL: LineQuantity = LineQuantity::PullBackLongitudinal;
const AT: LineQuantity = LineQuantity::PullAheadTransverse;
const AL: LineQuantity = LineQuantity::PullAheadLongitudinal;

/// The line's values in "calm" (N; transverse, longitudinal, vertical;
/// then the back spans' pull T, L and the ahead spans' T, L):
///
/// - A1, a suspension point on a straight line: sent (0, 0, 1000), back
///   (0, −20000), ahead (0, +20000);
/// - A2, at a line angle: sent (3000, 500, 2000), back (1500, −19000),
///   ahead (1500, +19500);
/// - A3, given per set (set 3, no value at the point): sent
///   (0, 100, 500), back (0, −10000), ahead (0, +10100).
///
/// `pulls`: with the spans' pulls (a line-values file of version 6), or
/// without (an older file).
fn line(pulls: bool) -> LineValues {
    let mut values = Vec::new();
    let rows: [(Option<&str>, Option<u32>, [f64; 7]); 3] = [
        (
            Some("A1"),
            None,
            [0.0, 0.0, 1000.0, 0.0, -20_000.0, 0.0, 20_000.0],
        ),
        (
            Some("A2"),
            None,
            [3000.0, 500.0, 2000.0, 1500.0, -19_000.0, 1500.0, 19_500.0],
        ),
        (
            None,
            Some(3),
            [0.0, 100.0, 500.0, 0.0, -10_000.0, 0.0, 10_100.0],
        ),
    ];
    for (point, set, f) in rows {
        for (k, q) in [T, L, V, BT, BL, AT, AL].into_iter().enumerate() {
            if q.is_span_pull() && !pulls {
                continue;
            }
            values.push(LineValue {
                case: "calm".into(),
                point: point.map(str::to_owned),
                set,
                quantity: q,
                value: f[k],
            });
        }
    }
    LineValues {
        points: Vec::new(),
        values,
        structure_wind: Vec::new(),
    }
}

/// One case "c" whose line forces of "calm" are `l`.
fn case(l: LineForces) -> Vec<LoadCaseRecipe> {
    let mut c = LoadCaseRecipe::new("c");
    c.line.push(l);
    vec![c]
}

fn shares(back: Option<f64>, ahead: Option<f64>) -> LineForces {
    LineForces {
        back,
        ahead,
        ..LineForces::new("calm")
    }
}

fn row<'a>(t: &'a LoadTable, label: &str) -> &'a LoadRow {
    t.rows
        .iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("no row at {label}"))
}

fn close(a: [f64; 3], b: [f64; 3]) {
    for k in 0..3 {
        assert!((a[k] - b[k]).abs() < 1e-9, "{a:?} {b:?}");
    }
}

/// back 5 %, ahead 0 %, by hand — force = sent − 0.95·back − 1.0·ahead:
///
/// - A1: L = 0 − 0.95·(−20000) − 1.0·(+20000) = 19000 − 20000 = −1000
///   (= 5 % of the back span's pull); T = 0; V = 1000 unchanged;
/// - A2: T = 3000 − 0.95·1500 − 1500 = 75; L = 500 − 0.95·(−19000) −
///   19500 = 500 + 18050 − 19500 = −950; V = 2000;
/// - A3 (per set): L = 100 − 0.95·(−10000) − 10100 = −500; V = 500.
///
/// Every row says it is a load assumption, not on the safe side.
#[test]
fn back_5_ahead_0_by_hand() {
    let t = evaluate(
        &case(shares(Some(5.0), Some(0.0))),
        &structure(),
        &line(true),
    );
    assert!(t.missing().next().is_none(), "{:?}", t.rows);
    close(row(&t, "A1").force, [0.0, -1000.0, 1000.0]);
    close(row(&t, "A2").force, [75.0, -950.0, 2000.0]);
    close(row(&t, "A3").force, [0.0, -500.0, 500.0]);
    for r in &t.rows {
        assert_eq!(r.source, Source::Line);
        assert!(
            r.note
                .contains(&format!("; NOT ON THE SAFE SIDE: {SPAN_SHARE_ASSUMPTION}")),
            "{}",
            r.note
        );
        assert!(r.note.contains("load ASSUMPTION, not an equilibrium"));
        assert!(
            r.note
                .contains("back spans' pull at 5 %, ahead spans' pull at 0 % of the solved pull"),
            "{}",
            r.note
        );
    }
}

/// A line angle, one side only — back 50 % (ahead not given: 100):
/// A2: T = 3000 − 0.5·1500 = 2250; L = 500 − 0.5·(−19000) = 10000.
/// Above 100 % is allowed and said — back 150 % at A1:
/// L = 0 − (1 − 1.5)·(−20000) = −10000.
#[test]
fn a_line_angle_and_a_share_above_100() {
    let t = evaluate(&case(shares(Some(50.0), None)), &structure(), &line(true));
    close(row(&t, "A2").force, [2250.0, 10_000.0, 2000.0]);
    let l = shares(Some(150.0), None);
    l.check().unwrap();
    let t = evaluate(&case(l.clone()), &structure(), &line(true));
    close(row(&t, "A1").force, [0.0, -10_000.0, 1000.0]);
    assert!(
        line_forces_text(&l).contains("back spans' pull at 150 % (above 100 %)"),
        "{}",
        line_forces_text(&l)
    );
}

/// 100 / 100 is the force as solved: bit for bit today's result, with or
/// without the spans' pulls sent, and no "not on the safe side" (nothing
/// is assumed).
#[test]
fn a_hundred_percent_is_todays_result_bit_for_bit() {
    let plain = evaluate(&case(LineForces::new("calm")), &structure(), &line(true));
    for pulls in [true, false] {
        let full = evaluate(
            &case(shares(Some(100.0), Some(100.0))),
            &structure(),
            &line(pulls),
        );
        assert_eq!(full.rows.len(), plain.rows.len());
        for (a, b) in full.rows.iter().zip(&plain.rows) {
            assert_eq!(a.label, b.label);
            assert_eq!(a.missing, None);
            for k in 0..3 {
                assert_eq!(a.force[k].to_bits(), b.force[k].to_bits(), "{}", a.label);
            }
            assert!(!a.note.contains("NOT ON THE SAFE SIDE"), "{}", a.note);
        }
    }
    assert!(!shares(Some(100.0), Some(100.0)).scales_span_pull());
    assert!(shares(Some(100.0), None).uses_span_shares());
}

/// The axis factors apply after the shares, `sets` picks the points as
/// before: at A2 only (set 2), back 5 / ahead 0 gives (75, −950, 2000),
/// × (transverse 1.5, longitudinal 2, vertical 1.35) =
/// (112.5, −1900, 2700).
#[test]
fn the_factors_come_after_and_sets_filter() {
    let l = LineForces {
        transverse: 1.5,
        longitudinal: 2.0,
        vertical: 1.35,
        sets: vec![2],
        ..shares(Some(5.0), Some(0.0))
    };
    let t = evaluate(&case(l), &structure(), &line(true));
    assert_eq!(t.rows.len(), 1, "{:?}", t.rows);
    close(row(&t, "A2").force, [112.5, -1900.0, 2700.0]);
    // A point turned off stays off (no force, its own note).
    let l = LineForces {
        off: vec!["A1".into()],
        ..shares(Some(5.0), Some(0.0))
    };
    let t = evaluate(&case(l), &structure(), &line(true));
    assert_eq!(row(&t, "A1").force, [0.0; 3]);
    assert!(row(&t, "A1").note.starts_with("off"));
}

/// The pulls not sent (an older line-values file, a structure opened on
/// its own): no value and why — never the unscaled force. Only the side
/// that is scaled needs its pull.
#[test]
fn missing_pulls_give_no_value() {
    let t = evaluate(
        &case(shares(Some(5.0), Some(0.0))),
        &structure(),
        &line(false),
    );
    for r in &t.rows {
        assert_eq!(r.force, [0.0; 3]);
        let why = r.missing.as_deref().expect("missing");
        assert!(
            why.starts_with(
                "no line value: pull_back_transverse, pull_back_longitudinal, \
                 pull_ahead_transverse, pull_ahead_longitudinal in \"calm\""
            ),
            "{why}"
        );
        assert!(r.note.contains("NOT ON THE SAFE SIDE"));
    }
    let t = evaluate(&case(shares(None, Some(40.0))), &structure(), &line(false));
    let why = row(&t, "A1").missing.clone().unwrap();
    assert!(
        why.starts_with("no line value: pull_ahead_transverse, pull_ahead_longitudinal in"),
        "{why}"
    );
}

const FILE: &str = r#"
format = "ohl-structure-load-cases"
version = 4

[[load_case]]
name = "unbalanced"

[[load_case.line]]
result = "calm"
vertical = 1.0
transverse = 1.0
longitudinal = 1.3
sets = [1, 2]
back = 5.0
ahead = 0.0
"#;

/// Read, written and read back; version 4 only with the shares (3
/// without); a negative or non-finite share refused, saying which.
#[test]
fn files_versions_and_what_is_refused() {
    let set = StructureLoadCases::parse(FILE).unwrap();
    let l = &set.cases[0].line[0];
    assert_eq!((l.back, l.ahead), (Some(5.0), Some(0.0)));
    assert_eq!(l.sets, [1, 2]);
    let again = StructureLoadCases::new("x", set.cases.clone());
    assert_eq!(again.version, Some(4));
    let text = again.to_toml();
    assert!(
        text.contains("back = 5.0") && text.contains("ahead = 0.0"),
        "{text}"
    );
    assert_eq!(StructureLoadCases::parse(&text).unwrap().cases, set.cases);
    // Without the shares: version 3, the keys not written.
    let mut plain = set.cases.clone();
    plain[0].line[0].back = None;
    plain[0].line[0].ahead = None;
    let without = StructureLoadCases::new("x", plain);
    assert_eq!(without.version, Some(3));
    assert!(!without.to_toml().contains("back"));
    // An older load taking a pull of the line: version 4 too.
    let mut older = LoadCaseRecipe::new("older");
    older.loads.push(LoadRecipe {
        what: Some(LineQuantity::PullBackLongitudinal),
        line_case: Some("calm".into()),
        points: Some(Pick::All),
        ..LoadRecipe::default()
    });
    assert_eq!(written_version(std::slice::from_ref(&older)), 4);
    for (from, to, says) in [
        ("back = 5.0", "back = -1.0", "back = -1"),
        ("ahead = 0.0", "ahead = nan", "ahead = NaN"),
        ("ahead = 0.0", "ahead = inf", "ahead = inf"),
        ("back = 5.0", "back = \"5 %\"", "invalid type"),
        ("back = 5.0", "bak = 5.0", "unknown field `bak`"),
    ] {
        let e = StructureLoadCases::parse(&FILE.replace(from, to)).unwrap_err();
        assert!(e.contains(says), "{says}: {e}");
    }
    let e = StructureLoadCases::parse(&FILE.replace("back = 5.0", "back = -1.0")).unwrap_err();
    assert!(e.contains("load case unbalanced, line 1"), "{e}");
    assert!(e.contains("0 or more"), "{e}");
    // In a structure file's tables.
    let mut s = structure();
    s.load_cases = StructureLoadCases::to_tables(&set.cases);
    assert_eq!(StructureLoadCases::from_structure(&s).unwrap(), set.cases);
}

/// The new quantities by name, in the recipes' CSV (an older load taking
/// one) and in the line-values file (JSON, version 6; a version 5 file
/// without them still reads; a newer one is refused).
#[test]
fn the_pull_quantities_round_trip_through_csv_and_json() {
    for (q, name) in [
        (BT, "pull_back_transverse"),
        (BL, "pull_back_longitudinal"),
        (AT, "pull_ahead_transverse"),
        (AL, "pull_ahead_longitudinal"),
    ] {
        assert_eq!(q.as_str(), name);
        assert_eq!(LineQuantity::parse(name), Some(q));
        assert!(q.is_span_pull() && !q.is_tension() && !q.is_pressure());
        assert!(LineQuantity::ALL.contains(&q));
    }
    assert_eq!(LineQuantity::ALL.len(), 17);
    // An older load of a pull: on its own axis, signed (A1's back span
    // pulls back: −20000 N along the line).
    let mut c = LoadCaseRecipe::new("pull");
    c.line_case = Some("calm".into());
    c.loads.push(LoadRecipe {
        what: Some(BL),
        points: Some(Pick::These(vec!["A1".into()])),
        ..LoadRecipe::default()
    });
    let cases = vec![c];
    let csv = StructureLoadCases::cases_to_csv(&cases).unwrap();
    assert!(csv.contains("pull_back_longitudinal"), "{csv}");
    assert_eq!(StructureLoadCases::cases_from_csv(&csv).unwrap(), cases);
    let t = evaluate(&cases, &structure(), &line(true));
    close(row(&t, "A1").force, [0.0, -20_000.0, 0.0]);
    // The load table with a not-safe note through CSV and JSON.
    let t = evaluate(
        &case(shares(Some(5.0), Some(0.0))),
        &structure(),
        &line(true),
    );
    assert_eq!(LoadTable::from_csv(&t.to_csv()).unwrap(), t);
    let json = serde_json::to_string(&t).unwrap();
    assert_eq!(serde_json::from_str::<LoadTable>(&json).unwrap(), t);
    // The line-values file.
    assert_eq!(LINE_VALUES_FILE_VERSION, 6);
    let mut placed = PlacedValues::pending("T2", "line", "type", "");
    placed.note = None;
    placed.cases = vec!["calm".into()];
    placed.values = line(true);
    let text = placed.to_json();
    assert!(text.contains("\"version\": 6"), "{text}");
    assert!(text.contains("\"pull_ahead_longitudinal\""), "{text}");
    assert_eq!(PlacedValues::parse(&text).unwrap(), placed);
    // A version 5 file (no pulls) reads; the shares then have no value.
    let mut old = placed.clone();
    old.values = line(false);
    let old_text = old.to_json().replace("\"version\": 6", "\"version\": 5");
    let read = PlacedValues::parse(&old_text).unwrap();
    assert_eq!(read.values.get("calm", "A1", &[1], BL), None);
    assert_eq!(read.values.get("calm", "A1", &[1], L), Some(0.0));
    let newer = text.replace("\"version\": 6", "\"version\": 7");
    assert!(PlacedValues::parse(&newer).unwrap_err().contains("newer"));
}
