// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure groups (owner, 2026-11-15; line file version 18): a
//! structure takes the union of the load cases of its groups, every case
//! in no group, none when its groups tick none; membership picked or by
//! rules (types, line angle, station on an alignment, dead end — bounds
//! inclusive), read from the line as it is. Lines of before (a
//! structure's own set, "none", a type's set) are read into groups.
//! Every name and number here is INVENTED (the synthetic example line).

use ohl_line::{CaseSet, GroupRules, LINE_FILE_VERSION, Line, StructureGroup};

fn example() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    ))
    .unwrap()
}

fn line() -> Line {
    Line::from_json(&example()).unwrap()
}

fn group(name: &str, cases: &[&str], rules: GroupRules) -> StructureGroup {
    StructureGroup {
        name: name.into(),
        rules,
        load_cases: cases.iter().map(|c| (*c).to_owned()).collect(),
        ..StructureGroup::default()
    }
}

fn members(line: &Line, k: usize) -> Vec<String> {
    line.case_sets().members(k).to_vec()
}

#[test]
fn a_line_without_groups_gives_every_structure_every_case() {
    let line = line();
    assert!(line.structure_groups.is_empty());
    let sets = line.case_sets();
    assert!(!sets.has_groups());
    assert_eq!(sets.in_no_group().len(), line.structures.len());
    assert!(line.takes_load_case("T2", "anything"));
    assert!(!line.takes_no_load_case("T2"));
    assert!(line.structures_without_load_cases().is_empty());
    let text = line.to_json();
    assert!(!text.contains("structure_groups"));
    assert!(!text.contains("\"version\": 18"));
}

#[test]
fn picked_structures_take_the_groups_cases_the_others_every_case() {
    let mut line = line();
    let mut g = group("suspension", &["wind", "ice"], GroupRules::default());
    g.structures = vec!["T2".into(), "T4".into()];
    line.structure_groups.push(g);
    let sets = line.case_sets();
    assert_eq!(
        *sets.of("T2"),
        CaseSet::These(vec!["wind".into(), "ice".into()])
    );
    assert!(sets.takes("T2", "wind") && !sets.takes("T2", "everyday"));
    assert_eq!(*sets.of("T1"), CaseSet::Every);
    assert!(sets.takes("T1", "everyday"));
    assert_eq!(sets.members(0), ["T2", "T4"]);
    assert_eq!(sets.groups_of("T4"), ["suspension"]);
    assert_eq!(sets.in_no_group(), ["T1", "T3", "T5", "T6", "T7"]);
    assert_eq!(
        line.load_cases_for(line.structure("T2").unwrap()),
        ["wind", "ice"]
    );
}

#[test]
fn a_structure_in_two_groups_takes_the_union() {
    let mut line = line();
    let mut a = group("A", &["wind", "ice"], GroupRules::default());
    a.structures = vec!["T2".into()];
    let mut b = group("B", &["ice", "broken"], GroupRules::default());
    b.structures = vec!["T2".into(), "T3".into()];
    line.structure_groups = vec![a, b];
    let sets = line.case_sets();
    assert_eq!(sets.of("T2").names(), ["wind", "ice", "broken"]);
    assert_eq!(sets.groups_of("T2"), ["A", "B"]);
    assert_eq!(sets.of("T3").names(), ["ice", "broken"]);
}

#[test]
fn a_group_ticking_no_case_gives_none_unless_another_group_gives_some() {
    let mut line = line();
    let mut none = group("dismantled", &[], GroupRules::default());
    none.structures = vec!["T6".into(), "T7".into()];
    let mut other = group("crossing", &["wind"], GroupRules::default());
    other.structures = vec!["T7".into()];
    line.structure_groups = vec![none, other];
    let sets = line.case_sets();
    assert!(sets.takes_none("T6"));
    assert!(!sets.takes("T6", "wind"));
    assert_eq!(sets.without_load_cases(), ["T6"]);
    // In two groups: the union, the other's case.
    assert!(!sets.takes_none("T7") && sets.takes("T7", "wind"));
    assert!(line.takes_no_load_case("T6"));
    assert_eq!(line.structures_without_load_cases(), ["T6"]);
}

#[test]
fn the_type_rule_takes_the_structures_of_those_types() {
    let mut line = line();
    line.structures[1].structure_type = Some("1A".into());
    line.structures[3].structure_type = Some("1A".into());
    line.structures[2].structure_type = Some("2B".into());
    line.structure_groups.push(group(
        "1A",
        &["wind"],
        GroupRules {
            types: vec!["1A".into()],
            ..GroupRules::default()
        },
    ));
    assert_eq!(members(&line, 0), ["T2", "T4"]);
    // Live: a type changed, the group follows.
    line.structures[2].structure_type = Some("1A".into());
    assert_eq!(members(&line, 0), ["T2", "T3", "T4"]);
}

#[test]
fn the_line_angle_rule_has_inclusive_bounds_and_follows_the_line() {
    let mut line = line();
    let angle = |line: &Line| line.structure_facts()["T3"].angle_deg.unwrap();
    let rules = |from: f64, to: f64| GroupRules {
        angle_from_deg: Some(from),
        angle_to_deg: Some(to),
        ..GroupRules::default()
    };
    line.structure_groups
        .push(group("angle", &["wind"], rules(0.5, 30.0)));
    let a = angle(&line);
    assert!(a > 1.0, "{a}");
    assert!(members(&line, 0).contains(&"T3".to_owned()));
    // Its own angle as a bound, from either side: still in (inclusive).
    line.structure_groups[0].rules = rules(a, 30.0);
    assert!(members(&line, 0).contains(&"T3".to_owned()));
    line.structure_groups[0].rules = rules(0.5, a);
    assert!(members(&line, 0).contains(&"T3".to_owned()));
    // Just above: out.
    line.structure_groups[0].rules = rules(a + 0.01, 30.0);
    assert!(!members(&line, 0).contains(&"T3".to_owned()));
    // The straight structures (angle 0) and the ends are not in 0.5–30°.
    line.structure_groups[0].rules = rules(0.5, 30.0);
    assert!(!members(&line, 0).contains(&"T2".to_owned()));
    // Live: T4 moved 3 m (still on the alignment), T3's angle changes
    // and T3 leaves a group of its angle only.
    line.structure_groups[0].rules = rules(a - 1e-4, a + 1e-4);
    assert!(members(&line, 0).contains(&"T3".to_owned()));
    let t4 = line.structures.iter_mut().find(|s| s.id == "T4").unwrap();
    t4.position[1] += 3.0;
    assert!((angle(&line) - a).abs() > 0.1, "{} {a}", angle(&line));
    assert!(!members(&line, 0).contains(&"T3".to_owned()));
}

#[test]
fn the_station_rule_is_per_alignment_with_inclusive_bounds() {
    let mut line = line();
    let facts = line.structure_facts();
    // No rule needs the stations yet: not computed.
    assert!(facts["T2"].stations.is_empty());
    let rules = |from: f64, to: f64| GroupRules {
        alignment: Some("main".into()),
        station_from: Some(from),
        station_to: Some(to),
        ..GroupRules::default()
    };
    line.structure_groups
        .push(group("first km", &["wind"], rules(0.0, 640.0)));
    let facts = line.structure_facts();
    let at = |id: &str| {
        facts[id]
            .stations
            .iter()
            .find(|(a, _)| a == "main")
            .unwrap()
            .1
    };
    let t3 = at("T3");
    assert!((t3 - 640.0).abs() < 1.0, "{t3}");
    line.structure_groups[0].rules = rules(0.0, t3);
    let m = members(&line, 0);
    assert!(m.contains(&"T3".to_owned()), "{m:?}");
    assert!(m.contains(&"T1".to_owned()) && m.contains(&"T2".to_owned()));
    assert!(!m.contains(&"T4".to_owned()));
    // On another alignment: not by this rule.
    assert!(!m.contains(&"T7".to_owned()));
    line.structure_groups[0].rules = rules(t3 + 0.01, 1e6);
    assert!(!members(&line, 0).contains(&"T3".to_owned()));
}

#[test]
fn the_dead_end_rule_is_read_from_the_line() {
    let mut line = line();
    line.structure_groups.push(group(
        "dead ends",
        &["broken"],
        GroupRules {
            dead_end: Some(true),
            ..GroupRules::default()
        },
    ));
    line.structure_groups.push(group(
        "through",
        &["wind"],
        GroupRules {
            dead_end: Some(false),
            ..GroupRules::default()
        },
    ));
    // S1 and S2 end at T1 and T5, S3 at T3 and T7.
    assert_eq!(members(&line, 0), ["T1", "T3", "T5", "T7"]);
    assert_eq!(members(&line, 1), ["T2", "T4", "T6"]);
}

#[test]
fn rules_all_hold_and_picked_structures_join_them() {
    let mut line = line();
    for s in &mut line.structures {
        s.structure_type = Some("1A".into());
    }
    let mut g = group(
        "1A dead ends",
        &["broken"],
        GroupRules {
            types: vec!["1A".into()],
            dead_end: Some(true),
            ..GroupRules::default()
        },
    );
    g.structures = vec!["T2".into()];
    line.structure_groups.push(g);
    assert_eq!(members(&line, 0), ["T1", "T2", "T3", "T5", "T7"]);
    line.structures[0].structure_type = Some("2B".into());
    assert_eq!(members(&line, 0), ["T2", "T3", "T5", "T7"]);
}

#[test]
fn groups_round_trip_at_version_18() {
    let mut line = line();
    let mut g = group(
        "angle 10–30°",
        &["wind", "ice"],
        GroupRules {
            types: vec!["1A".into()],
            angle_from_deg: Some(10.0),
            angle_to_deg: Some(30.0),
            alignment: Some("main".into()),
            station_from: Some(0.0),
            station_to: Some(900.0),
            dead_end: Some(false),
        },
    );
    g.description = "angle towers".into();
    g.structures = vec!["T6".into()];
    line.structure_groups.push(g);
    line.structure_groups
        .push(group("nothing", &[], GroupRules::default()));
    assert_eq!(LINE_FILE_VERSION, 18);
    let text = line.to_json();
    assert!(text.contains("\"version\": 18"), "{text}");
    assert!(text.contains("\"structure_groups\""));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.structure_groups, line.structure_groups);
    assert_eq!(back, line);
    // Without groups: an older version again.
    line.structure_groups.clear();
    assert!(!line.to_json().contains("\"version\": 18"));
}

#[test]
fn a_group_name_twice_or_a_bound_upside_down_is_refused() {
    let mut line = line();
    line.structure_groups
        .push(group("a", &[], GroupRules::default()));
    line.structure_groups
        .push(group("a", &[], GroupRules::default()));
    let e = Line::from_json(&line.to_json()).unwrap_err().to_string();
    assert!(e.contains("given twice"), "{e}");
    line.structure_groups.pop();
    line.structure_groups[0].rules.angle_from_deg = Some(30.0);
    line.structure_groups[0].rules.angle_to_deg = Some(10.0);
    let e = Line::from_json(&line.to_json()).unwrap_err().to_string();
    assert!(e.contains("from 30 is above to 10"), "{e}");
}

/// A version 14 line with a structure's own set, a structure taking none
/// and a type's set: read into groups, said in each group's description,
/// the old fields emptied, written back at version 18. With the union, a
/// structure with its own set whose type had one takes both (the safe
/// side).
#[test]
fn old_case_sets_are_read_into_groups() {
    let mut line = line();
    line.structures[1].structure_type = Some("1A".into());
    line.structures[1].load_cases = vec!["wind".into(), "ice".into()];
    line.structures[3].load_cases = vec!["wind".into(), "ice".into()];
    line.structures[4].no_load_cases = true;
    line.structures[2].structure_type = Some("1A".into());
    line.structure_loads
        .type_load_cases
        .insert("1A".into(), vec!["everyday".into()]);
    let text = line.to_json();
    assert!(text.contains("\"version\": 14"), "the old form: {text}");
    // Resolved as groups even before it is read again.
    assert!(line.takes_no_load_case("T5"));
    let back = Line::from_json(&text).unwrap();
    assert!(!back.has_old_case_sets());
    assert!(back.structures.iter().all(|s| s.load_cases.is_empty()));
    assert!(back.structures.iter().all(|s| !s.no_load_cases));
    assert!(back.structure_loads.type_load_cases.is_empty());
    let names: Vec<&str> = back
        .structure_groups
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    assert_eq!(names, ["type 1A", "T2, T4", "no load case"]);
    assert!(
        back.structure_groups[0]
            .description
            .contains("structure type 1A")
    );
    assert!(
        back.structure_groups[1]
            .description
            .contains("own load cases of T2, T4")
    );
    let sets = back.case_sets();
    assert_eq!(sets.of("T3").names(), ["everyday"]);
    assert_eq!(sets.of("T4").names(), ["wind", "ice"]);
    // T2: its own and its type's (union).
    assert_eq!(sets.of("T2").names(), ["everyday", "wind", "ice"]);
    assert!(sets.takes_none("T5"));
    assert_eq!(*sets.of("T1"), CaseSet::Every);
    assert!(back.to_json().contains("\"version\": 18"));
    // Read again: the same groups, none added twice.
    let again = Line::from_json(&back.to_json()).unwrap();
    assert_eq!(again.structure_groups, back.structure_groups);
}

#[test]
fn a_list_and_none_together_are_refused() {
    let mut line = line();
    line.structures[1].no_load_cases = true;
    line.structures[1].load_cases = vec!["wind".into()];
    let e = Line::from_json(&line.to_json()).unwrap_err().to_string();
    assert!(e.contains("no_load_cases and load_cases"), "{e}");
    assert!(e.contains("contradict"), "{e}");
}

/// The settings of Loads for structures of before 2026-11-15 are kept as
/// read, no longer used, said in one note.
#[test]
fn the_old_loads_for_structures_settings_are_said_once() {
    let mut line = line();
    assert!(line.unused_structure_loads_note().is_none());
    line.structure_loads.weather_cases = vec!["storm".into()];
    line.structure_loads.broken_cases = vec!["S1 broken".into()];
    line.structure_loads.condition = Some("creep".into());
    let note = line.unused_structure_loads_note().unwrap();
    assert!(note.contains("weather cases sent (storm)"), "{note}");
    assert!(
        note.contains("broken-wire cases sent (S1 broken)"),
        "{note}"
    );
    assert!(note.contains("no longer used"), "{note}");
    let back = Line::from_json(&line.to_json()).unwrap();
    assert_eq!(back.structure_loads.weather_cases, ["storm"]);
}
