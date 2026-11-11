// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A system's highest operating voltage and insulation level (line file
//! version 15, owner 2026-11-11): files without them read as before —
//! the nominal voltage only, the insulation high — and are written at
//! their version; with one written at version 15 and read back; a highest
//! operating voltage below the nominal refused.

use ohl_line::electrical::InsulationLevel;
use ohl_line::{LINE_FILE_VERSION, Line};

fn example() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    ))
    .unwrap()
}

#[test]
fn older_files_read_as_nominal_only_and_high() {
    let line = Line::from_json(&example()).unwrap();
    assert!(!line.systems.is_empty());
    for s in &line.systems {
        assert_eq!(s.highest_voltage_kv, None);
        assert_eq!(s.insulation_level, None);
        assert_eq!(s.insulation(), InsulationLevel::High);
        assert_eq!(line.system_highest_voltage(&s.id), None);
    }
    // Written without the keys, at an older version.
    let text = line.to_json();
    assert!(!text.contains("highest_voltage_kv"));
    assert!(!text.contains("insulation_level"));
    assert!(!text.contains("\"version\": 15"));
}

#[test]
fn the_two_values_round_trip_at_version_15() {
    assert_eq!(LINE_FILE_VERSION, 15);
    let mut line = Line::from_json(&example()).unwrap();
    let id = line.systems[0].id.clone();
    let nominal = line.systems[0].voltage_kv;
    // The highest operating voltage alone.
    line.systems[0].highest_voltage_kv = Some(nominal + 25.0);
    let text = line.to_json();
    assert!(text.contains("\"version\": 15"));
    assert!(text.contains("\"highest_voltage_kv\""));
    assert!(!text.contains("insulation_level"));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back, line);
    assert_eq!(back.system_highest_voltage(&id), Some(nominal + 25.0));
    assert_eq!(back.systems[0].insulation(), InsulationLevel::High);
    // The insulation level alone: low, and high set explicitly.
    line.systems[0].highest_voltage_kv = None;
    for level in [InsulationLevel::Low, InsulationLevel::High] {
        line.systems[0].insulation_level = Some(level);
        let text = line.to_json();
        assert!(text.contains("\"version\": 15"));
        assert!(text.contains(&format!("\"insulation_level\": \"{}\"", level.as_str())));
        let back = Line::from_json(&text).unwrap();
        assert_eq!(back.systems[0].insulation_level, Some(level));
        assert_eq!(back.systems[0].insulation(), level);
    }
    // Both taken off again: an older version.
    line.systems[0].insulation_level = None;
    assert!(!line.to_json().contains("\"version\": 15"));
}

#[test]
fn a_highest_voltage_below_the_nominal_is_refused() {
    let mut line = Line::from_json(&example()).unwrap();
    let nominal = line.systems[0].voltage_kv;
    // Equal to the nominal: allowed.
    line.systems[0].highest_voltage_kv = Some(nominal);
    line.validate().unwrap();
    for bad in [nominal - 1.0, 0.0] {
        line.systems[0].highest_voltage_kv = Some(bad);
        let e = line.validate().unwrap_err().to_string();
        assert!(e.contains("highest operating voltage"), "{e}");
    }
    // Not a number: refused as every number is.
    line.systems[0].highest_voltage_kv = Some(f64::NAN);
    assert!(line.validate().is_err());
    // An insulation level other than low or high is not read.
    let text = example().replacen(
        "\"id\": \"ex220\",",
        "\"id\": \"ex220\",\n      \"insulation_level\": \"medium\",",
        1,
    );
    assert!(text.contains("insulation_level"));
    assert!(Line::from_json(&text).is_err());
}

/// A plugin's own column of the same name lives among the system's
/// `fields` under `plugin.column`: the two are different keys in the
/// file and neither changes the other.
#[test]
fn a_plugin_column_of_the_same_name_is_another_value() {
    let mut line = Line::from_json(&example()).unwrap();
    line.systems[0]
        .fields
        .insert("some-plugin.insulation_level".into(), "low".into());
    // The plugin's value does not set the line's own.
    assert_eq!(line.systems[0].insulation_level, None);
    assert_eq!(line.systems[0].insulation(), InsulationLevel::High);
    // No core field used: not version 15.
    let text = line.to_json();
    assert!(!text.contains("\"version\": 15"));
    line.systems[0].insulation_level = Some(InsulationLevel::High);
    let back = Line::from_json(&line.to_json()).unwrap();
    assert_eq!(
        back.systems[0].insulation_level,
        Some(InsulationLevel::High)
    );
    assert_eq!(
        back.systems[0]
            .fields
            .get("some-plugin.insulation_level")
            .map(String::as_str),
        Some("low")
    );
}
