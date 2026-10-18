// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::Path;

use ohl_cables::{Electrical, Library, ResistanceAt};
use ohl_core::units::{ResistancePerLength, Temperature};

use super::*;

/// Drake 26/7 ACSR as in the IEEE 738 worked example: 28.14 mm,
/// R(25 °C) = 7.283e-5 Ω/m, R(75 °C) = 8.688e-5 Ω/m, ε = α = 0.8.
fn drake() -> Conductor {
    let r = |t: f64, ohm_per_m: f64| ResistanceAt {
        temperature: Temperature::from_celsius(t),
        resistance: ResistancePerLength::from_ohms_per_meter(ohm_per_m),
    };
    Conductor {
        diameter: 0.02814,
        kind: CableKind::Acsr,
        electrical: Electrical {
            ac_resistance: vec![r(25.0, 7.283e-5), r(75.0, 8.688e-5)],
            emissivity: Some(0.8),
            solar_absorptivity: Some(0.8),
            ..Electrical::default()
        },
        heat_capacity: vec![(1066.0, 3.8e-4), (243.0, 1.0e-4)],
        roughness: 0.09,
        defaults: [0.8, 0.8, 0.1],
    }
}

/// The example's weather: 40 °C, 0.61 m/s across the line, east-west
/// line at 30° N, 11:00 solar time on 10 June, clear air, sea level.
fn example_ambient() -> Ambient {
    Ambient {
        wind_speed: 0.61,
        sun: Sun::Position {
            latitude: 30.0,
            day: 161,
            hour: 11.0,
            line_azimuth: Some(90.0),
            atmosphere: Atmosphere::Clear,
            clearness: None,
        },
        ..Ambient::still("IEEE 738 example", 40.0)
    }
}

fn close(actual: f64, expected: f64, rel: f64) {
    assert!(
        ((actual - expected) / expected).abs() <= rel,
        "actual {actual}, expected {expected}"
    );
}

#[test]
fn ieee_738_drake_example() {
    // Published results of the standard's worked example (Drake at
    // 100 °C): q_c ≈ 81.9 W/m, q_r ≈ 39.1 W/m, q_s ≈ 22.4 W/m, I ≈ 1025 A.
    let b = ampacity(Method::Ieee738, &drake(), &example_ambient(), 100.0).unwrap();
    close(b.convection, 81.93, 0.005);
    close(b.radiation, 39.1, 0.005);
    close(b.solar.heat, 22.44, 0.01);
    close(b.current, 1025.0, 0.005);
    assert_eq!(b.convection_mode, Convection::Forced);
    let sun = b.solar.sun.unwrap();
    assert!((sun.altitude - 74.8).abs() < 0.3, "{sun:?}");
    assert!((sun.azimuth - 114.0).abs() < 0.5, "{sun:?}");
    close(drake().resistance(100.0), 9.390e-5, 1e-3);
}

#[test]
fn temperature_inverts_ampacity() {
    let (c, a) = (drake(), example_ambient());
    for method in Method::ALL {
        let b = ampacity(method, &c, &a, 100.0).unwrap();
        let t = temperature(method, &c, &a, b.current, &default_steady_budget()).unwrap();
        assert!((t.temperature - 100.0).abs() < 1e-4, "{method:?}: {t:?}");
        assert!(t.net().abs() < 1e-3);
    }
    // No current, no sun: the air temperature.
    let dark = Ambient::still("dark", 25.0);
    let t = temperature(Method::Ieee738, &c, &dark, 0.0, &default_steady_budget()).unwrap();
    assert_eq!(t.temperature, 25.0);
    // The sun alone warms it a little.
    let t = temperature(Method::Cigre601, &c, &a, 0.0, &default_steady_budget()).unwrap();
    assert!(t.temperature > 40.0 && t.temperature < 60.0, "{t:?}");
    // Far too much current.
    assert!(temperature(Method::Ieee738, &c, &a, 1e6, &default_steady_budget()).is_err());
}

#[test]
fn methods_agree_within_a_few_percent() {
    let (c, a) = (drake(), example_ambient());
    let i = |m| ampacity(m, &c, &a, 100.0).unwrap().current;
    let ieee = i(Method::Ieee738);
    // CIGRE counts diffuse and reflected sunlight too: a little less.
    for m in [Method::Cigre601, Method::Cigre207] {
        assert!(
            (i(m) / ieee - 1.0).abs() < 0.05,
            "{m:?}: {} vs {ieee}",
            i(m)
        );
        assert!(i(m) < ieee);
    }
    // With the same solar heat and still air the convection rules differ
    // more, but stay comparable.
    let still = Ambient {
        wind_speed: 0.0,
        sun: Sun::Flux(1000.0),
        ..a
    };
    let ieee = ampacity(Method::Ieee738, &c, &still, 100.0).unwrap();
    let cigre = ampacity(Method::Cigre601, &c, &still, 100.0).unwrap();
    assert_eq!(ieee.convection_mode, Convection::Natural);
    assert_eq!(cigre.convection_mode, Convection::Natural);
    assert!((cigre.current / ieee.current - 1.0).abs() < 0.15);
}

#[test]
fn wind_along_the_line_and_night_change_the_rating() {
    let (c, a) = (drake(), example_ambient());
    for m in Method::ALL {
        let across = ampacity(m, &c, &a, 100.0).unwrap().current;
        let along = Ambient {
            wind_angle: 10.0,
            ..a.clone()
        };
        let night = Ambient {
            sun: Sun::Flux(0.0),
            ..a.clone()
        };
        assert!(ampacity(m, &c, &along, 100.0).unwrap().current < across);
        assert!(ampacity(m, &c, &night, 100.0).unwrap().current > across);
    }
    assert!(ampacity(Method::Ieee738, &c, &a, 30.0).is_err());
}

#[test]
fn wind_angles_fold_to_the_acute_angle() {
    let (c, a) = (drake(), example_ambient());
    let at = |m: Method, angle: f64| {
        let w = Ambient {
            wind_angle: angle,
            ..a.clone()
        };
        ampacity(m, &c, &w, 100.0).unwrap().current
    };
    for m in Method::ALL {
        // Along the line either way, across it either way.
        for (angle, folded) in [(180.0, 0.0), (190.0, 10.0), (-10.0, 10.0), (270.0, 90.0)] {
            close(at(m, angle), at(m, folded), 1e-9);
        }
        assert!(at(m, 180.0) < at(m, 90.0));
    }
    for (angle, folded) in [(135.0, 45.0), (-45.0, 45.0), (450.0, 90.0), (-720.0, 0.0)] {
        assert!((acute_wind_angle(angle) - folded).abs() < 1e-9, "{angle}");
    }
}

#[test]
fn single_ac_resistance_scales_with_aluminium() {
    // Drake given only at 25 °C: the resistance rises with aluminium's
    // coefficient, close to the straight line through 25 and 75 °C.
    let mut one = drake();
    one.electrical.ac_resistance.truncate(1);
    let r100 = 7.283e-5 * (1.0 + 0.00403 * 80.0) / (1.0 + 0.00403 * 5.0);
    close(one.resistance(100.0), r100, 1e-12);
    close(one.resistance(100.0), drake().resistance(100.0), 0.01);
    let a = example_ambient();
    let i_one = ampacity(Method::Ieee738, &one, &a, 100.0).unwrap().current;
    let i_two = ampacity(Method::Ieee738, &drake(), &a, 100.0)
        .unwrap()
        .current;
    close(i_one, i_two, 0.005);
    // A cable of unknown material needs two values.
    one.kind = CableKind::Other;
    assert!(ampacity(Method::Ieee738, &one, &a, 100.0).is_err());
    assert!(temperature(Method::Ieee738, &one, &a, 800.0, &default_steady_budget()).is_err());
    let mut two = drake();
    two.kind = CableKind::Other;
    assert!(ampacity(Method::Ieee738, &two, &a, 100.0).is_ok());
}

#[test]
fn transient_approaches_the_steady_state() {
    let (c, a) = (drake(), example_ambient());
    let m = Method::Ieee738;
    let budget = default_steady_budget();
    let from = temperature(m, &c, &a, 800.0, &budget).unwrap().temperature;
    let to = temperature(m, &c, &a, 1200.0, &budget).unwrap().temperature;
    let steps = transient(m, &c, &a, from, 1200.0, 4.0 * 3600.0, DEFAULT_TIME_STEP).unwrap();
    assert_eq!(steps.len(), 1441);
    assert_eq!(steps[0], (0.0, from));
    let end = steps.last().unwrap().1;
    assert!((end - to).abs() < 0.01, "{end} vs {to}");
    // Rising all the way; the first 15 min cover most of the step.
    assert!(steps.windows(2).all(|w| w[1].1 >= w[0].1 - 1e-9));
    let quarter = steps[90].1;
    assert!((quarter - from) / (to - from) > 0.5, "{quarter}");
    // Halving the step changes nothing that matters.
    let fine = transient(m, &c, &a, from, 1200.0, 900.0, 5.0).unwrap();
    assert!((fine.last().unwrap().1 - quarter).abs() < 1e-3);
    // The last step is shortened to end at the duration.
    let odd = transient(m, &c, &a, from, 1200.0, 25.0, 10.0).unwrap();
    assert_eq!(
        odd.iter().map(|s| s.0).collect::<Vec<_>>(),
        [0.0, 10.0, 20.0, 25.0]
    );
    assert!(transient(m, &c, &a, from, 1200.0, 60.0, 0.0).is_err());
}

#[test]
fn library_cable_heat_capacity_and_resistance() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables");
    let library = Library::load(&dir).unwrap();
    let cable = library.get("acsr-drake-26-7").unwrap();
    let c = Conductor::from_cable(cable).unwrap();
    // About the IEEE example's 1310 J/(m·K).
    close(c.heat_capacity_at(20.0), 1310.0, 0.02);
    close(c.diameter, 0.02814, 1e-3);
    close(c.resistance(100.0), 9.39e-5, 0.01);
    let mut bare = cable.clone();
    bare.electrical = None;
    assert!(Conductor::from_cable(&bare).is_err());
    // One AC resistance: enough for an aluminium cable, not for `other`.
    let mut one = cable.clone();
    one.electrical.as_mut().unwrap().ac_resistance.truncate(1);
    assert!(Conductor::from_cable(&one).is_ok());
    one.kind = CableKind::Other;
    assert!(Conductor::from_cable(&one).is_err());
}

#[test]
fn methods_by_name() {
    assert_eq!(Method::parse("IEEE-738").unwrap(), Method::Ieee738);
    assert_eq!(Method::parse("tb601").unwrap(), Method::Cigre601);
    assert_eq!(Method::parse("CIGRE 207").unwrap(), Method::Cigre207);
    assert!(Method::parse("ieee").is_err());
}

/// A project's default values: its roughness, and its emissivity,
/// absorptivity and albedo where neither the cable nor the case gives
/// them; the ampacity follows.
#[test]
fn the_project_defaults_are_used() {
    let mut c = drake();
    c.electrical.emissivity = None;
    c.electrical.solar_absorptivity = None;
    let a = example_ambient();
    assert_eq!(c.emissivity(&a), DEFAULT_EMISSIVITY);
    assert_eq!(c.albedo(&a), ambient::DEFAULT_ALBEDO);
    let d = ohl_line::Defaults {
        thermal_emissivity: 0.5,
        thermal_absorptivity: 0.3,
        thermal_albedo: 0.25,
        thermal_roughness: 0.05,
        ..Default::default()
    };
    let p = c.clone().with_defaults(&d);
    assert_eq!(
        (
            p.emissivity(&a),
            p.absorptivity(&a),
            p.albedo(&a),
            p.roughness
        ),
        (0.5, 0.3, 0.25, 0.05)
    );
    // The case's own albedo wins.
    let mut own = a.clone();
    own.albedo = Some(0.6);
    assert_eq!(p.albedo(&own), 0.6);
    let i = |c: &Conductor| ampacity(Method::Ieee738, c, &a, 100.0).unwrap().current;
    assert!((i(&p) - i(&c)).abs() > 1.0);
}
