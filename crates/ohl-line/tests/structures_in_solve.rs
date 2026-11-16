// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structures in the wire solve (`solver.structures`, line file
//! version 21; plan of 2026-11-15, phase 5): files without it read as
//! rigid and are written at their own version; linear and full written at
//! version 21 and read back; full structures (calculation level 4) are
//! read but refused when a run asks for their level (not built yet).

use ohl_line::solve::flexible::CalculationLevel;
use ohl_line::{LINE_FILE_VERSION, Line, StructuresInSolve};

fn example() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    ))
    .unwrap()
}

#[test]
fn older_files_read_as_rigid_and_keep_their_version() {
    assert_eq!(LINE_FILE_VERSION, 23);
    let line = Line::from_json(&example()).unwrap();
    assert_eq!(line.solver.structures, StructuresInSolve::Rigid);
    assert_eq!(line.solver.structures.level(), Ok(CalculationLevel::Rigid));
    let text = line.to_json();
    assert!(
        !text.contains("\"structures\": \""),
        "not written when rigid"
    );
    assert!(!text.contains("\"version\": 21"));
    assert_eq!(ohl_line::rigid_check_note(&line), None);
}

#[test]
fn linear_and_full_round_trip_at_version_21() {
    let mut line = Line::from_json(&example()).unwrap();
    for (choice, word, level) in [
        (
            StructuresInSolve::Linear,
            "linear",
            Some(CalculationLevel::Linear),
        ),
        (StructuresInSolve::Full, "full", None),
    ] {
        line.solver.structures = choice;
        let text = line.to_json();
        assert!(text.contains("\"version\": 21"), "{word}");
        assert!(
            text.contains(&format!("\"structures\": \"{word}\"")),
            "{word}"
        );
        let back = Line::from_json(&text).unwrap();
        assert_eq!(back, line);
        assert_eq!(StructuresInSolve::parse(word), Some(choice));
        assert_eq!(choice.as_str(), word);
        match level {
            Some(l) => assert_eq!(back.solver.structures.level(), Ok(l)),
            None => {
                let why = back.solver.structures.level().unwrap_err();
                assert!(why.contains("not built yet"), "{why}");
            }
        }
        let note = ohl_line::rigid_check_note(&back).unwrap();
        assert!(
            note.starts_with("calculation level 2 (structures rigid): this check does not take"),
            "{note}"
        );
    }
    // Back to rigid: an older version again; the wind options alone are
    // still version 20.
    line.solver.structures = StructuresInSolve::Rigid;
    assert!(!line.to_json().contains("\"version\": 21"));
    line.solver.wind.friction = Some(ohl_line::wind::Friction(0.1));
    assert!(line.to_json().contains("\"version\": 20"));
}

#[test]
fn an_unknown_choice_is_refused_with_the_choices() {
    let text = example().replacen(
        "\"solver\": {",
        "\"solver\": {\n    \"structures\": \"springy\",",
        1,
    );
    if text == example() {
        // The example writes no solver block: put one in.
        let mut v: serde_json::Value = serde_json::from_str(&example()).unwrap();
        v["solver"] = serde_json::json!({"structures": "springy"});
        let e = Line::from_json(&v.to_string()).unwrap_err().to_string();
        assert!(e.contains("springy") && e.contains("linear"), "{e}");
    } else {
        let e = Line::from_json(&text).unwrap_err().to_string();
        assert!(e.contains("springy") && e.contains("linear"), "{e}");
    }
}
