// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The spans' horizontal pulls sent with the loads
//! ([`AttachmentLoad::pull_back`], [`AttachmentLoad::pull_ahead`]) and a
//! structure that takes no load case (owner, 2026-11-10).

use std::path::Path;

use super::*;

fn library() -> ohl_cables::Library {
    ohl_cables::Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"))
        .unwrap()
}

/// Three level 300 m spans along +x (the structures' "ahead"), a
/// suspension string at T2 and T3 (3 m, 800 N), the dead-end wire points
/// fixed at T1 and T4; strung at 20 kN at 15 °C. INVENTED.
fn three_spans() -> Line {
    Line::from_json(
        r#"{
  "format": "ohl-line", "version": 6, "name": "three spans", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 27], "set": 1}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [300, 0, 27], "set": 1}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [600, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [600, 0, 27], "set": 1}]},
    {"id": "T4", "position": [900, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [900, 0, 27], "set": 1}]}
  ],
  "sections": [{"id": "S", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}, {"structure": "T4", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}}]
}"#,
    )
    .unwrap()
}

fn weather() -> Vec<WeatherCase> {
    ohl_weather::WeatherCases::parse(
        "[[case]]\nname = \"calm\"\ntemperature = \"10 °C\"\n\
         [[case]]\nname = \"wind\"\ntemperature = \"10 °C\"\nwind_pressure = \"500 Pa\"\n",
    )
    .unwrap()
    .cases
}

fn at<'a>(c: &'a CaseLoads, structure: &str) -> &'a AttachmentLoad {
    c.loads
        .iter()
        .find(|l| l.structure == structure && !l.point.is_empty())
        .unwrap_or_else(|| panic!("no load at {structure}"))
}

fn cases() -> Vec<LoadCase> {
    ["calm", "wind"]
        .into_iter()
        .map(LoadCase::from_weather)
        .collect()
}

/// Still air, a straight line: at the suspension string of T2 the back
/// span pulls back (−H along the line) and the ahead span ahead (+H),
/// together the longitudinal force sent (none: the string plumb); at a
/// dead end the one span's pull is the longitudinal force sent — T1 has
/// an ahead span only, T4 a back span only. The pulls are a share of the
/// force sent, not added to it.
#[test]
fn the_spans_pulls_in_still_air() {
    let line = three_spans();
    let r = structure_loads(&line, &library(), &weather(), &cases(), &default_budget()).unwrap();
    let calm = &r[0];
    let t2 = at(calm, "T2");
    let h = t2.pull_ahead[1];
    assert!(h > 15_000.0 && h < 25_000.0, "{h}");
    assert!((t2.pull_back[1] + h).abs() < 1e-3 * h, "{t2:?}");
    assert!(t2.pull_back[0].abs() < 1e-6 && t2.pull_ahead[0].abs() < 1e-6);
    assert!(
        (t2.pull_back[1] + t2.pull_ahead[1] - t2.local[1]).abs() < 1e-3,
        "{t2:?}"
    );
    let t1 = at(calm, "T1");
    assert_eq!(t1.pull_back, [0.0; 2]);
    assert!((t1.pull_ahead[1] - t1.local[1]).abs() < 1e-3, "{t1:?}");
    assert!(t1.pull_ahead[1] > 15_000.0);
    let t4 = at(calm, "T4");
    assert_eq!(t4.pull_ahead, [0.0; 2]);
    assert!((t4.pull_back[1] - t4.local[1]).abs() < 1e-3, "{t4:?}");
    assert!(t4.pull_back[1] < -15_000.0);
}

/// In wind the pulls are still along the spans: the wind across the
/// spans (the transverse force sent) is not in them.
#[test]
fn the_wind_across_the_span_is_not_in_the_pulls() {
    let line = three_spans();
    let r = structure_loads(&line, &library(), &weather(), &cases(), &default_budget()).unwrap();
    let t2 = at(&r[1], "T2");
    assert!(t2.local[0].abs() > 1000.0, "{t2:?}");
    // The clamps blown out by the same amount at both ends of a span keep
    // it along the line (the dead ends are fixed: a small angle only).
    assert!(
        t2.pull_back[0].abs() < 0.02 * t2.pull_back[1].abs(),
        "{t2:?}"
    );
    assert!(t2.pull_ahead[1] > 15_000.0, "{t2:?}");
}

/// A broken span pulls nothing: with span 2 broken T2 keeps its back
/// span's pull only — the longitudinal force sent — and T3 its ahead
/// span's.
#[test]
fn a_broken_span_sends_no_pull() {
    let line = three_spans();
    let mut case = LoadCase::from_weather("calm");
    case.set_broken(vec![BrokenWire {
        section: "S".into(),
        span: BrokenSpan::One(2),
        set: None,
        subconductors: None,
    }]);
    let r = structure_loads(&line, &library(), &weather(), &[case], &default_budget()).unwrap();
    let t2 = at(&r[0], "T2");
    assert_eq!(t2.pull_ahead, [0.0; 2]);
    assert!(t2.pull_back[1] < -1000.0, "{t2:?}");
    assert!((t2.pull_back[1] - t2.local[1]).abs() < 1e-3, "{t2:?}");
    let t3 = at(&r[0], "T3");
    assert_eq!(t3.pull_back, [0.0; 2]);
    assert!((t3.pull_ahead[1] - t3.local[1]).abs() < 1e-3, "{t3:?}");
}

/// A structure taking no load case (`no_load_cases`): no loads in any
/// case, said in every case's notes; the others as before, bit for bit.
/// In the usage it is listed, not checked, saying so — never a pass.
#[test]
fn a_structure_taking_no_load_case_is_skipped_and_said() {
    let line = three_spans();
    let (lib, weather, budget) = (library(), weather(), default_budget());
    let before = structure_loads(&line, &lib, &weather, &cases(), &budget).unwrap();
    let mut none = line.clone();
    none.structures[1].no_load_cases = true;
    let r = structure_loads(&none, &lib, &weather, &cases(), &budget).unwrap();
    for (c, b) in r.iter().zip(&before) {
        assert!(!c.loads.iter().any(|l| l.structure == "T2"));
        for s in ["T1", "T3", "T4"] {
            assert_eq!(at(c, s), at(b, s), "{s}");
        }
        let said: Vec<&String> = c
            .notes
            .iter()
            .filter(|n| n.contains(TAKES_NO_LOAD_CASE))
            .collect();
        assert_eq!(said.len(), 1, "{:?}", c.notes);
        assert!(said[0].contains("T2"), "{said:?}");
        // Not also listed as out of a load case set.
        assert!(
            !c.notes
                .iter()
                .any(|n| n.contains("not in the load case set"))
        );
    }
    assert!(
        !before[0]
            .notes
            .iter()
            .any(|n| n.contains(TAKES_NO_LOAD_CASE))
    );
    let types = ohl_structures::StructureLibrary::default();
    let usage = capacity::usage(&none, &types, &lib, &weather, &r);
    for c in &usage {
        let t2 = c
            .structures
            .iter()
            .find(|s| s.structure == "T2")
            .expect("listed");
        assert_eq!(t2.status(), capacity::UsageStatus::NotChecked);
        assert!(t2.governing().is_none());
        assert!(
            t2.notes[0].starts_with(TAKES_NO_LOAD_CASE),
            "{:?}",
            t2.notes
        );
    }
}

/// [`three_spans`] with a second wire (set 2, section R, strung at
/// 12 kN) hanging from the same point A1 of T2 and T3 by its own string.
/// INVENTED.
fn two_sets_on_a_point() -> Line {
    Line::from_json(
        r#"{
  "format": "ohl-line", "version": 6, "name": "two sets on a point", "crs": "local",
  "structures": [
    {"id": "T1", "position": [0, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [0, 0, 27], "set": 1},
               {"label": "W2", "position": [0, 0, 26], "set": 2}]},
    {"id": "T2", "position": [300, 0, 0], "orientation_deg": -90,
     "points": [{"label": "A1", "position": [300, 0, 30]}],
     "strings": [{"label": "S1", "top": "A1", "wire": "W1", "length": 3.0,
                  "weight": 800.0, "stiffness": 5e7},
                 {"label": "S2", "top": "A1", "wire": "W2", "length": 4.0,
                  "weight": 800.0, "stiffness": 5e7}],
     "wires": [{"label": "W1", "position": [300, 0, 27], "set": 1},
               {"label": "W2", "position": [300, 0, 26], "set": 2}]},
    {"id": "T3", "position": [600, 0, 0], "orientation_deg": -90,
     "wires": [{"label": "W1", "position": [600, 0, 27], "set": 1},
               {"label": "W2", "position": [600, 0, 26], "set": 2}]}
  ],
  "sections": [
   {"id": "S", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 1}, {"structure": "T2", "set": 1},
               {"structure": "T3", "set": 1}],
    "stringing": {"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}},
   {"id": "R", "cable": "acsr-drake-26-7", "bundle": 1,
    "points": [{"structure": "T1", "set": 2}, {"structure": "T2", "set": 2},
               {"structure": "T3", "set": 2}],
    "stringing": {"kind": "tension", "horizontal_tension": 12000.0, "temperature": 15.0}}]
}"#,
    )
    .unwrap()
}

/// The pulls per wire set (roadmap O22 item 7): at a point holding two
/// sets each set's back and ahead pull is kept, the two adding up to the
/// point's; a point with one set has that set's alone, equal to its own.
#[test]
fn the_pulls_per_set_add_up_to_the_points() {
    let line = two_sets_on_a_point();
    let r = structure_loads(&line, &library(), &weather(), &cases(), &default_budget()).unwrap();
    for case in &r {
        let t2 = case
            .loads
            .iter()
            .find(|l| l.structure == "T2" && l.point == "A1")
            .expect("the load at A1");
        assert_eq!(t2.sets, vec![1, 2], "{t2:?}");
        let sets: Vec<u32> = t2.pulls_by_set.iter().map(|p| p.set).collect();
        assert_eq!(sets, vec![1, 2]);
        for i in 0..2 {
            let back: f64 = t2.pulls_by_set.iter().map(|p| p.back[i]).sum();
            let ahead: f64 = t2.pulls_by_set.iter().map(|p| p.ahead[i]).sum();
            assert!((back - t2.pull_back[i]).abs() < 1e-9, "{t2:?}");
            assert!((ahead - t2.pull_ahead[i]).abs() < 1e-9, "{t2:?}");
        }
    }
    // Still air: set 1 pulls about its 20 kN each way, set 2 about its
    // 12 kN.
    let t2 = at(&r[0], "T2");
    let (one, two) = (t2.pulls_by_set[0], t2.pulls_by_set[1]);
    assert!(
        one.back[1] < -15_000.0 && one.ahead[1] > 15_000.0,
        "{one:?}"
    );
    assert!(
        two.back[1] < -9000.0 && two.back[1] > -15_000.0 && two.ahead[1] > 9000.0,
        "{two:?}"
    );
    // One set at the point: its pulls are the point's.
    let single = three_spans();
    let r = structure_loads(&single, &library(), &weather(), &cases(), &default_budget()).unwrap();
    let t2 = at(&r[0], "T2");
    assert_eq!(t2.pulls_by_set.len(), 1);
    assert_eq!(t2.pulls_by_set[0].set, 1);
    assert_eq!(t2.pulls_by_set[0].back, t2.pull_back);
    assert_eq!(t2.pulls_by_set[0].ahead, t2.pull_ahead);
}

/// Some subconductors of a bundle broken in a load case (roadmap O22 item
/// 7): the three spans as a twin bundle, one subconductor of span 2
/// broken. T2 keeps both spans' pulls — the one wire left ahead pulling
/// a little less than the two behind — and takes the difference along the
/// line, less than with the whole phase broken; the case's notes say what is solved and what
/// is not checked. More subconductors than the bundle has: the case
/// fails. Every subconductor: the whole-phase case, the same loads.
#[test]
fn some_subconductors_broken_in_a_load_case() {
    let mut line = three_spans();
    line.sections[0].bundle = 2;
    let broken = |k: Option<u32>| {
        let mut case = LoadCase::from_weather("calm");
        case.set_broken(vec![BrokenWire {
            section: "S".into(),
            span: BrokenSpan::One(2),
            set: None,
            subconductors: k,
        }]);
        structure_loads(&line, &library(), &weather(), &[case], &default_budget())
    };
    let part = broken(Some(1)).unwrap();
    let whole = broken(None).unwrap();
    let t2 = at(&part[0], "T2");
    assert!(
        t2.pull_back[1] < -1000.0 && t2.pull_ahead[1] > 1000.0,
        "{t2:?}"
    );
    assert!(
        (t2.pull_back[1] + t2.pull_ahead[1] - t2.local[1]).abs() < 1e-3,
        "{t2:?}"
    );
    // One wire of two left ahead: less pull than the two behind.
    assert!(t2.pull_ahead[1] < -t2.pull_back[1] - 100.0, "{t2:?}");
    // A smaller unbalance than the whole phase broken.
    let all = at(&whole[0], "T2");
    assert!(
        t2.local[1] < 0.0 && t2.local[1].abs() < 0.8 * all.local[1].abs(),
        "{} against {}",
        t2.local[1],
        all.local[1]
    );
    let notes = &part[0].notes;
    assert!(
        notes
            .iter()
            .any(|n| n == "broken wire: S span 2 broken (1 subconductor)"),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| {
            n.starts_with("1 of 2 subconductors of S broken: the static state after the break")
                && n.contains("NOT ON THE SAFE SIDE for the subconductors left")
        }),
        "{notes:?}"
    );
    assert!(
        !whole[0]
            .notes
            .iter()
            .any(|n| n.contains("subconductors of"))
    );
    // Every subconductor is the whole phase.
    let both = broken(Some(2)).unwrap();
    assert_eq!(at(&both[0], "T2").local, all.local);
    assert!(!both[0].notes.iter().any(|n| n.contains("subconductors of")));
    // More than the bundle has.
    let e = broken(Some(3)).unwrap_err();
    assert!(
        e.contains("3 subconductors of section S (a bundle of 2"),
        "{e}"
    );
}
