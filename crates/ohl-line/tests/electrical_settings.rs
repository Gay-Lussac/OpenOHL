// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's electrical settings (line file version 11, roadmap O18):
//! absent from files while the defaults, written and read back, the
//! resistivity checked, the assumed default said.

use ohl_line::Line;
use ohl_line::electrical::{
    DEFAULT_SOIL_RESISTIVITY_OHM_M, EarthReturn, ElectricalSettings, MeanHeight,
};

fn example() -> Line {
    Line::from_json(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/synthetic-line/synthetic-line.ohl.json"
        ))
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn settings_round_trip_at_version_11() {
    let mut line = example();
    assert!(line.electrical.is_default());
    assert!(!line.to_json().contains("\"electrical\""));
    assert!(!line.to_json().contains("\"version\": 11"));
    assert_eq!(
        line.electrical.soil_resistivity(),
        (DEFAULT_SOIL_RESISTIVITY_OHM_M, true)
    );
    line.electrical = ElectricalSettings {
        soil_resistivity_ohm_m: Some(250.0),
        earth_return: EarthReturn::ComplexDepth,
        mean_height: MeanHeight::TwoThirdsSag,
        earthing_resistance_ohm: Some(7.5),
    };
    let text = line.to_json();
    assert!(text.contains("\"version\": 11"));
    assert!(text.contains("\"soil_resistivity_ohm_m\": 250.0"));
    assert!(text.contains("\"earth_return\": \"complex_depth\""));
    assert!(text.contains("\"mean_height\": \"two_thirds_sag\""));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.electrical, line.electrical);
    assert_eq!(back.electrical.soil_resistivity(), (250.0, false));
    assert_eq!(back.electrical.earthing_resistance(), (7.5, false));
}

/// A structure's earthing resistance (2026-10-29): its own, else the
/// line's, else assumed; written at version 11 and read back; refused
/// when not a positive number.
#[test]
fn earthing_resistances() {
    let mut line = example();
    let id = line.structures[0].id.clone();
    assert_eq!(
        line.earthing_resistance(&id),
        (ohl_line::electrical::DEFAULT_EARTHING_RESISTANCE_OHM, true)
    );
    line.electrical.earthing_resistance_ohm = Some(4.0);
    assert_eq!(line.earthing_resistance(&id), (4.0, false));
    line.electrical.earthing_resistance_ohm = None;
    line.structures[0].earthing_resistance_ohm = Some(2.5);
    assert_eq!(line.earthing_resistance(&id), (2.5, false));
    let text = line.to_json();
    assert!(text.contains("\"version\": 11"));
    assert!(text.contains("\"earthing_resistance_ohm\": 2.5"));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.structures[0].earthing_resistance_ohm, Some(2.5));
    for bad in [0.0, -1.0, f64::NAN] {
        line.structures[0].earthing_resistance_ohm = Some(bad);
        assert!(line.validate().is_err(), "{bad}");
    }
    line.structures[0].earthing_resistance_ohm = None;
    line.electrical.earthing_resistance_ohm = Some(-1.0);
    assert!(line.validate().is_err());
}

#[test]
fn a_resistivity_must_be_positive() {
    let mut line = example();
    for bad in [0.0, -5.0, f64::NAN] {
        line.electrical.soil_resistivity_ohm_m = Some(bad);
        assert!(line.validate().is_err(), "{bad}");
    }
}

#[test]
fn names_parse_back() {
    for m in EarthReturn::ALL {
        assert_eq!(EarthReturn::parse(m.as_str()), Some(m));
    }
    for m in MeanHeight::ALL {
        assert_eq!(MeanHeight::parse(m.as_str()), Some(m));
    }
    assert_eq!(MeanHeight::parse("mean"), Some(MeanHeight::TwoThirdsSag));
}
