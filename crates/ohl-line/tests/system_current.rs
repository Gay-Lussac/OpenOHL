// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A system's operating current (line file version 13, 2026-11-04):
//! files without one read as before and written at their version; with
//! one written at version 13 and read back; a negative current refused.

use ohl_line::{LINE_FILE_VERSION, Line};

fn example() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    ))
    .unwrap()
}

#[test]
fn older_files_read_without_a_current() {
    let line = Line::from_json(&example()).unwrap();
    assert!(line.systems.iter().all(|s| s.current_a.is_none()));
    assert_eq!(line.version, LINE_FILE_VERSION);
    let text = line.to_json();
    // Written without the key, at an older version.
    assert!(!text.contains("current_a"));
    assert!(!text.contains("\"version\": 13"));
}

#[test]
fn the_current_round_trips_at_version_13() {
    let mut line = Line::from_json(&example()).unwrap();
    line.systems[0].current_a = Some(850.0);
    let text = line.to_json();
    assert!(text.contains("\"version\": 13"));
    assert!(text.contains("\"current_a\": 850.0"));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.systems[0].current_a, Some(850.0));
    assert_eq!(back.section_currents().get("S1"), Some(&850.0));
    // Taken off again: an older version.
    line.systems[0].current_a = None;
    assert!(!line.to_json().contains("\"version\": 13"));
}

#[test]
fn a_negative_current_is_refused() {
    let text = example().replacen(
        "\"id\": \"ex220\",",
        "\"id\": \"ex220\",\n      \"current_a\": -10,",
        1,
    );
    assert!(text.contains("current_a"));
    let e = Line::from_json(&text).unwrap_err().to_string();
    assert!(e.contains("operating current"), "{e}");
}
