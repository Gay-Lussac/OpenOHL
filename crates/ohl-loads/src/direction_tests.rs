// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Load case file version 7 (owner 2026-11-15, wind at any direction): a
//! case's wind direction and sets of them, the conversion of the bodies'
//! old angles, the bodies' direction from the wires'. Every line and value
//! here is INVENTED.

use super::v4_tests::{library, two_wires, weather};
use super::*;

/// A version-6 (or older) file: its body angle is in the old convention
/// (0° from the right, towards ahead), kept as read — the forces the old
/// ones bit for bit — and written back at its version as read. A version-7
/// file: in the program's convention. θ = 90° − θ_old.
#[test]
fn old_body_angles_are_read_in_their_convention() {
    let old = "format = \"ohl-load-cases\"\nversion = 4\n[[case]]\nname = \"a\"\n\
               weather = \"wind\"\n[case.structure]\ndirection = 30\n";
    let c = LoadCases::parse(old).unwrap();
    let d = c.cases[0].on_structure.direction.unwrap();
    assert_eq!(d, WindDirection::OldAngle(30.0));
    assert_eq!(d.degrees(), 60.0);
    let r = 30f64.to_radians();
    assert_eq!(d.towards(), [-r.cos(), -r.sin()]);
    // Written back at version 4, the old angle as read.
    let written = c.to_toml();
    assert!(written.contains("version = 4\n") && written.contains("direction = 30.0"));
    assert_eq!(LoadCases::parse(&written).unwrap().cases, c.cases);
    // Without a version: an old file.
    let bare = old.replace("version = 4\n", "");
    assert_eq!(
        LoadCases::parse(&bare).unwrap().cases[0]
            .on_structure
            .direction,
        Some(WindDirection::OldAngle(30.0))
    );
    // Version 7: the program's convention.
    let new = old.replace("version = 4", "version = 7");
    let c7 = LoadCases::parse(&new).unwrap();
    assert_eq!(
        c7.cases[0].on_structure.direction,
        Some(WindDirection::Angle(30.0))
    );
    assert_eq!(c7.written_version(), 7);
    // An old angle in a file raised to 7 (another case uses version 7):
    // written converted.
    let mut both = c.clone();
    both.cases.push(LoadCase {
        name: "b".into(),
        wind_direction: Some(WindDirection::Back),
        ..LoadCase::from_weather("wind")
    });
    let w = both.to_toml();
    assert!(
        w.contains("version = 7\n") && w.contains("direction = 60.0"),
        "{w}"
    );
    assert!(w.contains("wind_direction = \"back\""), "{w}");
}

/// The wires' direction of a load case: replaces the weather case's;
/// across the line it is the across rule from that side (the case as
/// with wind_from); refused with wind_from or with a set.
#[test]
fn a_load_case_direction_for_the_wires() {
    let w = &weather()[1];
    let mut c = LoadCase::from_weather("wind");
    c.wind_direction = Some(WindDirection::Angle(270.0));
    let left = c.wire_weather(w);
    assert_eq!(left.wind_from, ohl_weather::WindSide::Left);
    assert_eq!(left.wind_direction, None);
    let mut old = LoadCase::from_weather("wind");
    old.wind_from = Some(ohl_weather::WindSide::Left);
    assert_eq!(old.wire_weather(w), left);
    c.wind_direction = Some(WindDirection::Back);
    assert_eq!(c.wire_weather(w).oblique(), Some(WindDirection::Back));
    // The bodies follow the wires' direction, relative to the line.
    let body = c.structure_wind(w);
    assert_eq!(body.direction, WindDirection::Back);
    assert!(body.relative_to_line && body.has_longitudinal());
    // Without a direction: the across side, in the structure's axes (as
    // before).
    let plain = LoadCase::from_weather("wind").structure_wind(w);
    assert_eq!(plain.direction, WindDirection::Right);
    assert!(!plain.relative_to_line);
    // [case.structure] direction still overrides, in the structure's
    // axes.
    c.on_structure.direction = Some(WindDirection::Left);
    let own = c.structure_wind(w);
    assert_eq!(own.direction, WindDirection::Left);
    assert!(!own.relative_to_line);
    for bad in [
        "version = 7\n[[case]]\nname = \"a\"\nweather = \"w\"\nwind_from = \"left\"\nwind_direction = 0\n",
        "version = 7\n[[case]]\nname = \"a\"\nweather = \"w\"\nwind_direction = 0\nwind_directions = [0]\n",
        "version = 7\n[[case]]\nname = \"a\"\nweather = \"w\"\nwind_directions = [0, 0]\n",
        "version = 7\n[[case]]\nname = \"a\"\nweather = \"w\"\nwind_direction = \"up\"\n",
    ] {
        assert!(LoadCases::parse(bad).is_err(), "{bad}");
    }
}

/// A set of directions is one case per direction, named with it; its
/// loads are those of the separate cases, bit for bit; the structures'
/// groups know them by the set's name.
#[test]
fn a_set_of_directions_is_one_case_each() {
    let line = two_wires();
    let lib = library();
    let weather = weather();
    let budget = default_budget();
    let set = LoadCases::parse(
        "format = \"ohl-load-cases\"\nversion = 7\n[[case]]\nname = \"every\"\n\
         weather = \"wind\"\ntransverse = 1.2\nwind_directions = [0, 45, 90, \"back\"]\n",
    )
    .unwrap();
    assert_eq!(set.written_version(), 7);
    assert_eq!(LoadCases::parse(&set.to_toml()).unwrap().cases, set.cases);
    let each = expand_directions(&set.cases);
    let names: Vec<&str> = each.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "every (wind from 0° (from ahead: blowing back))",
            "every (wind from 45°)",
            "every (wind from 90° (from the right))",
            "every (wind from 180° (from behind: blowing ahead))",
        ]
    );
    assert!(each.iter().all(|c| c.set_name() == "every"));
    let together = structure_loads(&line, &lib, &weather, &set.cases, &budget).unwrap();
    assert_eq!(together.len(), 4);
    for (one, got) in each.iter().zip(&together) {
        let mut alone = one.clone();
        alone.of_set = None;
        let apart = structure_loads(&line, &lib, &weather, &[alone], &budget).unwrap();
        assert_eq!(apart[0].loads, got.loads, "{}", one.name);
    }
    // 90° is the case from the right (the across rule), bit for bit.
    let right = structure_loads(
        &line,
        &lib,
        &weather,
        &[LoadCase {
            transverse: 1.2,
            ..LoadCase::from_weather("wind")
        }],
        &budget,
    )
    .unwrap();
    assert_eq!(right[0].loads, together[2].loads);
    // Along the line (0°, 180°): the straight line's wires take no wind
    // across; the notes say the case is one direction, not the envelope.
    let across = |c: &CaseLoads| -> f64 { c.loads.iter().map(|l| l.local[0].abs()).sum() };
    assert!(across(&together[0]) < 1e-6 * across(&together[2]));
    assert!(
        together[0]
            .notes
            .iter()
            .any(|n| n.contains("not the across envelope"))
    );
}

/// A structure turned off the line's direction takes a wind relative to
/// the line in its own axes; one on the line's direction as it is.
#[test]
fn a_turned_structure_takes_the_wind_in_its_axes() {
    let mut line = two_wires();
    // A route along the structures (east).
    line.routes.push(ohl_line::Route::new("main", "main"));
    for (k, x) in [0.0, 900.0].into_iter().enumerate() {
        line.alignment_points.push(ohl_line::AlignmentPoint::free(
            &format!("P{k}"),
            [x, 0.0, 0.0],
        ));
    }
    line.alignments.push(ohl_line::Alignment::new(
        "main",
        "main",
        vec!["P0".into(), "P1".into()],
    ));
    let w = &weather()[1];
    let mut c = LoadCase::from_weather("wind");
    c.wind_direction = Some(WindDirection::Angle(30.0));
    // The line runs +x (east): a structure with orientation −90° has its
    // ahead +x — on the line's direction.
    let s = line.structures[1].clone();
    assert!(c.in_structure_axes(w, &line, &s).is_none());
    // Turned 10° anticlockwise: the wind 10° further clockwise in its axes.
    line.structures[1].orientation_deg = -80.0;
    let s = line.structures[1].clone();
    let turned = c.in_structure_axes(w, &line, &s).unwrap();
    let d = turned.on_structure.direction.unwrap().degrees();
    assert!((d - 40.0).abs() < 1e-9, "{d}");
    let body = c.structure_wind(w).on_structure(&line, &s);
    assert!((body.direction.degrees() - 40.0).abs() < 1e-9);
}
