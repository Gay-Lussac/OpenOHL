// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Weather cases and the loads they put on wires.
//!
//! A weather case combines a temperature, a radial ice thickness and a wind
//! pressure. Applied to a bare cable of diameter \\( d \\) and weight
//! \\( w_c \\) per unit length:
//!
//! - ice cross-section \\( A_i = \frac{\pi}{4}\left[(d + 2t)^2 - d^2\right] \\),
//!   ice weight \\( w_i = \rho_i g A_i \\);
//! - vertical load \\( w_v = w_c + w_i \\);
//! - wind load \\( w_h = q\,(d + 2t) \\) on the projected iced diameter;
//! - resultant \\( w = \sqrt{w_v^2 + w_h^2} \\), acting in a plane swung by
//!   \\( \arctan(w_h / w_v) \\) from the vertical.
//!
//! This is the code-agnostic physics. Design codes (NESC, IEC 60826,
//! EN 50341, CSA, …) add their own factors, constants and case definitions
//! as plugins on top (roadmap Phase 4).

use ols_core::units::{Angle, Area, Density, ForcePerLength, Length, Speed, Stress, Temperature};
use serde::Deserialize;

/// Density of glaze ice commonly used in line design, kg/m³.
pub const GLAZE_ICE_DENSITY: f64 = 913.0;

/// Standard air density at 15 °C and sea level, kg/m³.
pub const STANDARD_AIR_DENSITY: f64 = 1.225;

fn default_ice_density() -> Density {
    Density::from_kilograms_per_cubic_meter(GLAZE_ICE_DENSITY)
}

/// A weather case.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherCase {
    /// Name, e.g. `"heavy ice + wind"`.
    pub name: String,
    /// Cable temperature.
    pub temperature: Temperature,
    /// Radial ice thickness (0 for no ice).
    #[serde(default)]
    pub ice_thickness: Length,
    /// Ice density (default: glaze ice, 913 kg/m³).
    #[serde(default = "default_ice_density")]
    pub ice_density: Density,
    /// Wind pressure on the projected (iced) diameter (0 for no wind).
    #[serde(default)]
    pub wind_pressure: Stress,
}

impl WeatherCase {
    /// Still air, no ice.
    pub fn still(name: &str, temperature: Temperature) -> Self {
        Self {
            name: name.to_owned(),
            temperature,
            ice_thickness: Length::ZERO,
            ice_density: default_ice_density(),
            wind_pressure: Stress::ZERO,
        }
    }

    /// Loads on a cable of the given bare diameter and weight.
    pub fn wire_load(&self, diameter: Length, weight: ForcePerLength) -> WireLoad {
        let d = diameter.si();
        let iced = d + 2.0 * self.ice_thickness.si();
        let ice_area = Area::from_si(std::f64::consts::FRAC_PI_4 * (iced * iced - d * d));
        WireLoad {
            vertical: weight + self.ice_density.weight_per_length(ice_area),
            horizontal: self.wind_pressure * Length::from_si(iced),
        }
    }
}

/// Load per unit length on a wire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WireLoad {
    /// Vertical load (cable + ice weight).
    pub vertical: ForcePerLength,
    /// Horizontal load (wind), perpendicular to the line.
    pub horizontal: ForcePerLength,
}

impl WireLoad {
    /// Resultant load \\( \sqrt{w_v^2 + w_h^2} \\).
    pub fn resultant(&self) -> ForcePerLength {
        ForcePerLength::from_si(self.vertical.si().hypot(self.horizontal.si()))
    }

    /// Angle of the loaded plane from the vertical (blowout angle).
    pub fn swing_angle(&self) -> Angle {
        Angle::from_radians(self.horizontal.si().atan2(self.vertical.si()))
    }
}

/// Dynamic wind pressure \\( q = \frac{1}{2}\rho V^2 \\), without any
/// design-code factor.
pub fn wind_pressure(speed: Speed, air_density: Density) -> Stress {
    Stress::from_pascals(0.5 * air_density.si() * speed.si() * speed.si())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, rel_tol: f64) {
        assert!(
            ((actual - expected) / expected).abs() <= rel_tol,
            "actual {actual}, expected {expected}"
        );
    }

    #[test]
    fn bare_still_air_is_cable_weight() {
        let w = ForcePerLength::from_newtons_per_meter(15.97);
        let load = WeatherCase::still("15 °C", Temperature::from_celsius(15.0))
            .wire_load(Length::from_millimeters(28.14), w);
        assert_eq!(load.vertical, w);
        assert_eq!(load.horizontal, ForcePerLength::ZERO);
        assert_eq!(load.resultant(), w);
        assert_eq!(load.swing_angle(), Angle::ZERO);
    }

    #[test]
    fn ice_and_wind_hand_calculation() {
        // Drake, 12.7 mm radial ice, 191.5 Pa wind:
        // iced diameter 53.54 mm; ice area π/4 (53.54² − 28.14²) mm².
        let case = WeatherCase {
            name: "ice + wind".into(),
            temperature: Temperature::from_celsius(-20.0),
            ice_thickness: Length::from_millimeters(12.7),
            ice_density: default_ice_density(),
            wind_pressure: Stress::from_pascals(191.5),
        };
        let load = case.wire_load(
            Length::from_millimeters(28.14),
            ForcePerLength::from_newtons_per_meter(15.97),
        );
        let ice_area_mm2 = std::f64::consts::FRAC_PI_4 * (53.54f64.powi(2) - 28.14f64.powi(2));
        let ice_weight = 913.0 * ice_area_mm2 * 1e-6 * 9.806_65;
        close(load.vertical.newtons_per_meter(), 15.97 + ice_weight, 1e-12);
        close(load.horizontal.newtons_per_meter(), 191.5 * 0.05354, 1e-12);
        close(
            load.resultant().newtons_per_meter(),
            (15.97 + ice_weight).hypot(191.5 * 0.05354),
            1e-12,
        );
    }

    #[test]
    fn wind_pressure_from_speed() {
        // q = ½ · 1.225 · 30² = 551.25 Pa
        let q = wind_pressure(
            Speed::from_meters_per_second(30.0),
            Density::from_kilograms_per_cubic_meter(STANDARD_AIR_DENSITY),
        );
        close(q.pascals(), 551.25, 1e-12);
    }

    #[test]
    fn reads_cases_from_toml_with_units() {
        #[derive(Deserialize)]
        struct File {
            case: Vec<WeatherCase>,
        }
        let file: File = toml::from_str(
            r#"
            [[case]]
            name = "everyday"
            temperature = "15 °C"

            [[case]]
            name = "NESC heavy (without K constant)"
            temperature = "0 °F"
            ice_thickness = "0.5 in"
            wind_pressure = "4 psf"
            "#,
        )
        .unwrap();
        assert_eq!(file.case[0].ice_thickness, Length::ZERO);
        close(file.case[1].ice_thickness.millimeters(), 12.7, 1e-12);
        // 4 psf × 47.880 26 Pa/psf (NIST SP 811)
        close(file.case[1].wind_pressure.pascals(), 191.521_04, 1e-6);
        close(
            file.case[1].ice_density.kilograms_per_cubic_meter(),
            913.0,
            1e-15,
        );
    }
}
