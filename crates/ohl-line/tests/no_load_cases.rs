// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure that takes no load case (`no_load_cases`, line file
//! version 14, owner 2026-11-10): files without one read as before and are
//! written at their version; with one written at version 14 and read
//! back; a list and none together refused.

use ohl_line::{LINE_FILE_VERSION, Line};

fn example() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    ))
    .unwrap()
}

#[test]
fn older_files_take_every_case_as_before() {
    let line = Line::from_json(&example()).unwrap();
    assert!(line.structures.iter().all(|s| !s.no_load_cases));
    assert!(line.structures_without_load_cases().is_empty());
    assert!(line.takes_load_case("T2", "anything"));
    assert!(!line.takes_no_load_case("T2"));
    let text = line.to_json();
    assert!(!text.contains("no_load_cases"));
    assert!(!text.contains("\"version\": 14"));
}

#[test]
fn a_structure_without_load_cases_round_trips_at_version_14() {
    let mut line = Line::from_json(&example()).unwrap();
    line.structures[1].no_load_cases = true;
    let id = line.structures[1].id.clone();
    assert!(!line.takes_load_case(&id, "anything"));
    assert!(line.takes_no_load_case(&id));
    assert!(line.has_load_case_sets());
    assert!(line.load_cases_for(&line.structures[1]).is_empty());
    assert_eq!(line.structures_without_load_cases(), [id.as_str()]);
    // The others as before.
    assert!(line.takes_load_case(&line.structures[0].id, "anything"));
    let text = line.to_json();
    assert_eq!(LINE_FILE_VERSION, 15);
    assert!(text.contains("\"version\": 14"));
    assert!(text.contains("\"no_load_cases\": true"));
    let back = Line::from_json(&text).unwrap();
    assert!(back.structures[1].no_load_cases);
    assert_eq!(back, line);
    // Taken off again: an older version.
    line.structures[1].no_load_cases = false;
    assert!(!line.to_json().contains("\"version\": 14"));
}

#[test]
fn a_list_and_none_together_are_refused() {
    let mut line = Line::from_json(&example()).unwrap();
    line.structures[1].no_load_cases = true;
    line.structures[1].load_cases = vec!["wind".into()];
    let e = Line::from_json(&line.to_json()).unwrap_err().to_string();
    assert!(e.contains("no_load_cases and load_cases"), "{e}");
    assert!(e.contains("contradict"), "{e}");
}
