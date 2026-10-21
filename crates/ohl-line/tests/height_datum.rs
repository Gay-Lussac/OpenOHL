// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's height datum (line file version 7): files of version 6 and
//! before read without one; written and read back; typed as the
//! coordinate system window takes it; an empty one refused.

use ohl_line::{HeightDatum, LINE_FILE_VERSION, Line};

fn example() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    ))
    .unwrap()
}

#[test]
fn older_files_read_without_a_height_datum() {
    let text = example();
    assert!(text.contains("\"version\": 6"));
    let line = Line::from_json(&text).unwrap();
    assert_eq!(line.height_datum, None);
    assert_eq!(line.height_datum_label(), "");
    assert_eq!(line.version, LINE_FILE_VERSION);
    assert_eq!(LINE_FILE_VERSION, 7);
    // Written without the key while none is given.
    assert!(!line.to_json().contains("height_datum"));
}

#[test]
fn the_height_datum_round_trips() {
    let mut line = Line::from_json(&example()).unwrap();
    line.height_datum = Some(HeightDatum {
        name: "LN02".into(),
        epsg: Some(5728),
    });
    let text = line.to_json();
    assert!(text.contains("\"version\": 7"));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back, line);
    assert_eq!(back.height_datum_label(), "LN02 (EPSG:5728)");
    // A name alone, a code alone.
    for (h, label) in [
        (
            HeightDatum {
                name: "LHN95".into(),
                epsg: None,
            },
            "LHN95",
        ),
        (
            HeightDatum {
                name: String::new(),
                epsg: Some(5728),
            },
            "EPSG:5728",
        ),
    ] {
        line.height_datum = Some(h);
        let back = Line::from_json(&line.to_json()).unwrap();
        assert_eq!(back.height_datum, line.height_datum);
        assert_eq!(back.height_datum_label(), label);
    }
    // Neither: refused, said.
    line.height_datum = Some(HeightDatum::default());
    let e = Line::from_json(&line.to_json()).unwrap_err().to_string();
    assert!(e.contains("height_datum"), "{e}");
    // A key it does not know: named.
    let typo = example().replacen(
        "\"crs\":",
        "\"height_datum\": {\"name\": \"LN02\", \"code\": 5728},\n  \"crs\":",
        1,
    );
    let e = Line::from_json(&typo).unwrap_err().to_string();
    assert!(e.contains("code"), "{e}");
}

#[test]
fn the_height_datum_as_typed() {
    let ln02 = HeightDatum {
        name: "LN02".into(),
        epsg: Some(5728),
    };
    assert_eq!(HeightDatum::parse(" LN02 ", "5728"), Ok(Some(ln02.clone())));
    assert_eq!(HeightDatum::parse("LN02", "EPSG:5728"), Ok(Some(ln02)));
    assert_eq!(HeightDatum::parse("", " "), Ok(None));
    assert_eq!(
        HeightDatum::parse("", "epsg:5728")
            .unwrap()
            .unwrap()
            .label(),
        "EPSG:5728"
    );
    assert!(HeightDatum::parse("LN02", "LN02").is_err());
    assert!(HeightDatum::parse("LN02", "0").is_err());
}
