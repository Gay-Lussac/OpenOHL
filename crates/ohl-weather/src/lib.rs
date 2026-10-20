// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Weather cases and the loads they put on wires.
//!
//! A weather case combines a temperature, ice and a wind pressure. Ice is
//! given either as a **radial thickness** \\( t \\) with a density
//! \\( \rho_i \\) (default glaze ice, 913 kg/m³), or as a **load per
//! length** \\( w_i \\) (N/m) with an optional density. Applied to a bare
//! cable of diameter \\( d \\) and weight \\( w_c \\) per unit length:
//!
//! - thickness: ice cross-section
//!   \\( A_i = \frac{\pi}{4}\left[(d + 2t)^2 - d^2\right] \\), ice weight
//!   \\( w_i = \rho_i g A_i \\), iced diameter \\( D = d + 2t \\);
//! - load: \\( w_i \\) as given; the ice area
//!   \\( A_i = w_i / (\rho_i g) \\) gives the iced diameter
//!   \\( D = \sqrt{d^2 + 4 A_i / \pi} \\) (more wind area for light rime
//!   than for dense glaze of the same weight), with the case's density or,
//!   without one, glaze ice (913 kg/m³, the densest ice in use: the
//!   smallest iced diameter the load can have, never the bare one);
//! - vertical load \\( w_v = w_c + w_i \\);
//! - wind load \\( w_h = q\,D \\) on the projected iced diameter;
//! - resultant \\( w = \sqrt{w_v^2 + w_h^2} \\), acting in a plane swung by
//!   \\( \arctan(w_h / w_v) \\) from the vertical.
//!
//! This is the code-agnostic physics. Design codes (NESC, IEC 60826,
//! EN 50341, CSA, …) add their own factors, constants and case definitions
//! as plugins on top (roadmap Phase 4).

use ohl_core::units::{Angle, Area, Density, ForcePerLength, Length, Speed, Stress, Temperature};
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
    /// Free text.
    #[serde(default)]
    pub description: String,
    /// Tags for choosing cases, e.g. `["clearance", "loads"]`.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Cable temperature.
    pub temperature: Temperature,
    /// Radial ice thickness (0 for no ice).
    #[serde(default)]
    pub ice_thickness: Length,
    /// Or: ice load per length of wire (instead of a thickness).
    #[serde(default)]
    pub ice_load: Option<ForcePerLength>,
    /// Ice density (default glaze ice, 913 kg/m³): with a thickness, the
    /// ice weight; with a load, the iced diameter for the wind.
    #[serde(default)]
    pub ice_density: Option<Density>,
    /// Wind pressure on the projected (iced) diameter (0 for no wind).
    #[serde(default)]
    pub wind_pressure: Stress,
    /// Side the wind comes from, looking along the line (each span from
    /// its back to its ahead point): from the right (default) it blows the
    /// wires to the left.
    #[serde(default)]
    pub wind_from: WindSide,
    /// Air density factor Q, Pa/(m/s)²: the wind pressure of a wind
    /// velocity V is Q·V² (default [`DEFAULT_AIR_DENSITY_FACTOR`], ½ρ of air
    /// at 15 °C at sea level). Only relates pressure and velocity: the
    /// loads use the pressure.
    #[serde(default)]
    pub air_density_factor: Option<f64>,
    /// Ambient (air) temperature, for ratings; the wires use `temperature`.
    #[serde(default)]
    pub ambient_temperature: Option<Temperature>,
}

/// Air density factor when a case gives none, Pa/(m/s)² (½ · 1.225 kg/m³).
pub const DEFAULT_AIR_DENSITY_FACTOR: f64 = 0.613;

/// Side of the line the wind comes from, looking ahead along it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindSide {
    /// From the right: the wires are blown to the left.
    #[default]
    Right,
    /// From the left: the wires are blown to the right.
    Left,
}

impl WindSide {
    /// `"left"` or `"right"`.
    pub fn as_str(self) -> &'static str {
        match self {
            WindSide::Right => "right",
            WindSide::Left => "left",
        }
    }

    /// The other side.
    pub fn opposite(self) -> Self {
        match self {
            WindSide::Right => WindSide::Left,
            WindSide::Left => WindSide::Right,
        }
    }

    /// Sign of the push to the left of the line: +1 from the right.
    pub fn to_left(self) -> f64 {
        match self {
            WindSide::Right => 1.0,
            WindSide::Left => -1.0,
        }
    }
}

impl WeatherCase {
    /// Still air, no ice.
    pub fn still(name: &str, temperature: Temperature) -> Self {
        Self {
            name: name.to_owned(),
            description: String::new(),
            tags: Vec::new(),
            temperature,
            ice_thickness: Length::ZERO,
            ice_load: None,
            ice_density: None,
            wind_pressure: Stress::ZERO,
            wind_from: WindSide::Right,
            air_density_factor: None,
            ambient_temperature: None,
        }
    }

    /// Its air density factor Q, Pa/(m/s)² (none given: the program's).
    pub fn air_density_factor(&self) -> f64 {
        self.air_density_factor_or(DEFAULT_AIR_DENSITY_FACTOR)
    }

    /// Its air density factor Q, Pa/(m/s)², else `default` (the
    /// project's).
    pub fn air_density_factor_or(&self, default: f64) -> f64 {
        self.air_density_factor.unwrap_or(default)
    }

    /// The wind velocity of its pressure, m/s: √(P / Q).
    pub fn wind_velocity(&self) -> f64 {
        self.wind_velocity_with(DEFAULT_AIR_DENSITY_FACTOR)
    }

    /// The wind velocity of its pressure, m/s, Q its own else `default`.
    pub fn wind_velocity_with(&self, default: f64) -> f64 {
        (self.wind_pressure.si() / self.air_density_factor_or(default))
            .max(0.0)
            .sqrt()
    }

    /// Checks the case: ice as a thickness or a load (not both), values
    /// not negative, density positive.
    pub fn validate(&self) -> Result<(), String> {
        let bad = |m: &str| Err(format!("weather case \"{}\": {m}", self.name));
        let non_negative = |x: f64| x >= 0.0 && x.is_finite();
        if !non_negative(self.ice_thickness.si())
            || !non_negative(self.wind_pressure.si())
            || !self.temperature.celsius().is_finite()
        {
            return bad("ice thickness and wind pressure must be ≥ 0");
        }
        if self
            .air_density_factor
            .is_some_and(|q| !(q > 0.0 && q.is_finite()))
        {
            return bad("the air density factor must be > 0");
        }
        if let Some(w) = self.ice_load {
            if !non_negative(w.si()) {
                return bad("ice load must be ≥ 0");
            }
            if self.ice_thickness.si() > 0.0 {
                return bad("give the ice as a thickness or as a load, not both");
            }
        }
        if self
            .ice_density
            .is_some_and(|d| d.si().is_nan() || d.si() <= 0.0)
        {
            return bad("ice density must be > 0");
        }
        Ok(())
    }

    /// The ice density used: the case's, else glaze ice
    /// ([`GLAZE_ICE_DENSITY`]).
    pub fn ice_density_or_glaze(&self) -> Density {
        self.ice_density.unwrap_or_else(default_ice_density)
    }

    /// The radial ice on a wire of bare diameter `diameter`: its thickness
    /// and density, for icing other objects on it (marker balls…). None
    /// without ice. An ice load without a density is glaze ice
    /// ([`GLAZE_ICE_DENSITY`]).
    pub fn radial_ice(&self, diameter: Length) -> Option<(Length, Density)> {
        let d = diameter.si();
        let rho = self.ice_density_or_glaze();
        match self.ice_load {
            Some(_) => {
                let t = (self.ice(d).1 - d) / 2.0;
                (t > 0.0).then(|| (Length::from_si(t), rho))
            }
            None => (self.ice_thickness.si() > 0.0).then_some((self.ice_thickness, rho)),
        }
    }

    /// The iced diameter of a wire of bare diameter `diameter` in this
    /// case: \\( d + 2t \\) for a thickness, \\( \sqrt{d^2 + 4 A_i/\pi} \\)
    /// for a load (the case's density, else glaze ice), `diameter` without
    /// ice. The wind acts on it, and clearances are measured to its
    /// surface.
    pub fn iced_diameter(&self, diameter: Length) -> Length {
        Length::from_si(self.ice(diameter.si()).1)
    }

    /// Ice weight per length and iced diameter, m, on a bare diameter `d`
    /// (m).
    fn ice(&self, d: f64) -> (f64, f64) {
        match self.ice_load {
            Some(w) => {
                // Without a density, glaze ice: the densest ice in use,
                // so the smallest iced diameter this load can have.
                let rho = self.ice_density_or_glaze();
                let area = w.si() / (rho.si() * ohl_core::units::STANDARD_GRAVITY);
                let iced = (d * d + 4.0 * area / std::f64::consts::PI).sqrt();
                (w.si(), iced)
            }
            None => {
                let iced = d + 2.0 * self.ice_thickness.si();
                let area = Area::from_si(std::f64::consts::FRAC_PI_4 * (iced * iced - d * d));
                let rho = self.ice_density.unwrap_or_else(default_ice_density);
                (rho.weight_per_length(area).si(), iced)
            }
        }
    }

    /// Loads on a cable of the given bare diameter and weight.
    pub fn wire_load(&self, diameter: Length, weight: ForcePerLength) -> WireLoad {
        let (ice, iced) = self.ice(diameter.si());
        WireLoad {
            vertical: weight + ForcePerLength::from_si(ice),
            horizontal: self.wind_pressure * Length::from_si(iced),
        }
    }

    /// One line describing the case, e.g. `-5 °C, ice 12.7 mm (913 kg/m³),
    /// wind 380 Pa`.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{:.1} °C", self.temperature.celsius())];
        match self.ice_load {
            Some(w) => parts.push(format!(
                "ice {:.2} N/m ({:.0} kg/m³)",
                w.newtons_per_meter(),
                self.ice_density_or_glaze().kilograms_per_cubic_meter()
            )),
            None if self.ice_thickness.si() > 0.0 => parts.push(format!(
                "ice {:.1} mm ({:.0} kg/m³)",
                self.ice_thickness.millimeters(),
                self.ice_density
                    .unwrap_or_else(default_ice_density)
                    .kilograms_per_cubic_meter()
            )),
            None => {}
        }
        if self.wind_pressure.si() > 0.0 {
            parts.push(format!(
                "wind {:.0} Pa from the {}",
                self.wind_pressure.pascals(),
                self.wind_from.as_str()
            ));
        }
        parts.join(", ")
    }
}

/// File extension of weather case files.
pub const WEATHER_FILE_EXTENSION: &str = "ohlwc";
/// Format identifier.
pub const WEATHER_FILE_FORMAT: &str = "ohl-weather-cases";
/// Newest format version read.
pub const WEATHER_FILE_VERSION: u32 = 1;

/// A set of weather cases (`*.ohlwc`).
///
/// ```toml
/// format = "ohl-weather-cases"
/// version = 1
/// name = "Project X"
///
/// [[case]]
/// name = "max temperature"
/// tags = ["clearance"]
/// temperature = "80 °C"
///
/// [[case]]
/// name = "rime ice + wind"
/// tags = ["clearance", "loads"]
/// temperature = "-5 °C"
/// ice_load = "20 N/m"
/// ice_density = "500 kg/m³"
/// wind_pressure = "200 Pa"
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherCases {
    /// `"ohl-weather-cases"` (optional header).
    #[serde(default)]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default)]
    pub version: Option<u32>,
    /// Name of the set.
    #[serde(default)]
    pub name: String,
    /// The cases.
    #[serde(rename = "case", default)]
    pub cases: Vec<WeatherCase>,
}

/// Whether `name` matches `pattern`: `*` (any text) and `?` (one
/// character) wildcards, case-insensitive.
pub fn name_matches(pattern: &str, name: &str) -> bool {
    matches(pattern, name)
}

fn matches(pattern: &str, text: &str) -> bool {
    ohl_core::pattern::matches(pattern, text)
}

impl WeatherCases {
    /// Parses and checks a case file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let set: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            set.format.as_deref(),
            set.version,
            WEATHER_FILE_FORMAT,
            WEATHER_FILE_VERSION,
            "weather case",
        )?;
        let mut names = std::collections::BTreeSet::new();
        for c in &set.cases {
            if !names.insert(c.name.as_str()) {
                return Err(format!("weather case \"{}\" is defined twice", c.name));
            }
            c.validate()?;
        }
        Ok(set)
    }

    /// Reads a case file.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The cases chosen by name (`*` and `?` wildcards) or tag, in file
    /// order; all cases when neither is given. A name pattern or tag that
    /// matches nothing is an error (a typo would silently skip a case).
    pub fn select(&self, names: &[String], tags: &[String]) -> Result<Vec<WeatherCase>, String> {
        if names.is_empty() && tags.is_empty() {
            return Ok(self.cases.clone());
        }
        for n in names {
            if !self.cases.iter().any(|c| matches(n, &c.name)) {
                return Err(format!("no weather case matches \"{n}\""));
            }
        }
        for t in tags {
            if !self.cases.iter().any(|c| c.tags.iter().any(|x| x == t)) {
                return Err(format!("no weather case has the tag \"{t}\""));
            }
        }
        Ok(self
            .cases
            .iter()
            .filter(|c| {
                names.iter().any(|n| matches(n, &c.name)) || c.tags.iter().any(|t| tags.contains(t))
            })
            .cloned()
            .collect())
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

    /// The radial ice of an ice load and density weighs that load on the
    /// wire; a load without a density has none; a thickness is itself.
    #[test]
    fn radial_ice_of_a_load_weighs_it() {
        let d = Length::from_millimeters(25.0);
        let still = WeatherCase::still("ice", Temperature::from_celsius(0.0));
        assert!(still.radial_ice(d).is_none());
        let by_load = WeatherCase {
            ice_load: Some(ForcePerLength::from_newtons_per_meter(20.0)),
            ice_density: Some(Density::from_kilograms_per_cubic_meter(900.0)),
            ..still.clone()
        };
        let (t, rho) = by_load.radial_ice(d).unwrap();
        let (d, t) = (d.si(), t.si());
        let area = std::f64::consts::FRAC_PI_4 * ((d + 2.0 * t).powi(2) - d * d);
        close(
            area * rho.si() * ohl_core::units::STANDARD_GRAVITY,
            20.0,
            1e-12,
        );
        // Without a density: glaze ice of that weight.
        let weight_only = WeatherCase {
            ice_density: None,
            ..by_load
        };
        let (t, rho) = weight_only.radial_ice(Length::from_si(d)).unwrap();
        close(rho.kilograms_per_cubic_meter(), GLAZE_ICE_DENSITY, 1e-12);
        let t = t.si();
        let area = std::f64::consts::FRAC_PI_4 * ((d + 2.0 * t).powi(2) - d * d);
        close(
            area * GLAZE_ICE_DENSITY * ohl_core::units::STANDARD_GRAVITY,
            20.0,
            1e-12,
        );
        let by_thickness = WeatherCase {
            ice_thickness: Length::from_millimeters(10.0),
            ..still
        };
        let (t, rho) = by_thickness.radial_ice(Length::from_si(d)).unwrap();
        assert_eq!(t, Length::from_millimeters(10.0));
        close(rho.kilograms_per_cubic_meter(), GLAZE_ICE_DENSITY, 1e-12);
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
            ice_thickness: Length::from_millimeters(12.7),
            wind_pressure: Stress::from_pascals(191.5),
            ..WeatherCase::still("ice + wind", Temperature::from_celsius(-20.0))
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
        assert_eq!(file.case[1].ice_density, None);
    }

    #[test]
    fn ice_as_a_load_with_and_without_density() {
        let (d, w) = (
            Length::from_millimeters(28.14),
            ForcePerLength::from_newtons_per_meter(15.97),
        );
        // The ice of 12.7 mm glaze (913 kg/m³) given as its load.
        let thick = WeatherCase {
            ice_thickness: Length::from_millimeters(12.7),
            wind_pressure: Stress::from_pascals(200.0),
            ..WeatherCase::still("t", Temperature::from_celsius(-5.0))
        };
        let reference = thick.wire_load(d, w);
        let ice = reference.vertical - w;
        let as_load = WeatherCase {
            ice_thickness: Length::ZERO,
            ice_load: Some(ice),
            ice_density: Some(Density::from_kilograms_per_cubic_meter(913.0)),
            ..thick.clone()
        };
        // Same weight, same iced diameter, same wind.
        let l = as_load.wire_load(d, w);
        close(l.vertical.si(), reference.vertical.si(), 1e-12);
        close(l.horizontal.si(), reference.horizontal.si(), 1e-12);
        // The same load as light rime (300 kg/m³): more wind area.
        let rime = WeatherCase {
            ice_density: Some(Density::from_kilograms_per_cubic_meter(300.0)),
            ..as_load.clone()
        };
        assert!(rime.wire_load(d, w).horizontal > reference.horizontal);
        // Without a density: glaze ice (913 kg/m³), so the same iced
        // diameter as the 12.7 mm glaze, never the bare one.
        let flat = WeatherCase {
            ice_density: None,
            ..as_load
        };
        let l = flat.wire_load(d, w);
        close(l.vertical.si(), reference.vertical.si(), 1e-12);
        close(l.horizontal.si(), reference.horizontal.si(), 1e-12);
        close(
            flat.iced_diameter(d).millimeters(),
            28.14 + 2.0 * 12.7,
            1e-12,
        );
        assert!(l.horizontal.si() > 200.0 * 0.02814);
        // Thickness and load together: refused.
        let both = WeatherCase {
            ice_thickness: Length::from_millimeters(5.0),
            ..flat
        };
        assert!(both.validate().is_err());
    }

    #[test]
    fn case_files_and_selection() {
        let set = WeatherCases::parse(
            r#"
            format = "ohl-weather-cases"
            name = "test"
            [[case]]
            name = "everyday"
            temperature = "10 °C"
            [[case]]
            name = "max temperature"
            tags = ["clearance"]
            temperature = "80 °C"
            [[case]]
            name = "ice + wind"
            tags = ["clearance", "loads"]
            temperature = "-5 °C"
            ice_load = "20 N/m"
            ice_density = "500 kg/m³"
            wind_pressure = "200 Pa"
            "#,
        )
        .unwrap();
        let names = |v: Vec<WeatherCase>| v.into_iter().map(|c| c.name).collect::<Vec<_>>();
        assert_eq!(set.select(&[], &[]).unwrap().len(), 3);
        assert_eq!(
            names(set.select(&[], &["clearance".into()]).unwrap()),
            ["max temperature", "ice + wind"]
        );
        assert_eq!(
            names(set.select(&["MAX*".into()], &[]).unwrap()),
            ["max temperature"]
        );
        assert_eq!(
            names(set.select(&["everyday".into()], &["loads".into()]).unwrap()),
            ["everyday", "ice + wind"]
        );
        assert!(set.select(&["evryday".into()], &[]).is_err());
        assert!(set.select(&[], &["nope".into()]).is_err());
        assert!(set.cases[2].summary().contains("ice 20.00 N/m (500 kg/m³)"));
        assert!(WeatherCases::parse("[[case]]\nname = \"a\"\ntemperature = \"1 °C\"\n[[case]]\nname = \"a\"\ntemperature = \"2 °C\"").is_err());
    }

    #[test]
    fn air_density_factor_relates_velocity_and_pressure() {
        let cases = WeatherCases::parse(
            r#"
format = "ohl-weather-cases"
version = 1
name = "q"

[[case]]
name = "wind"
temperature = "10 °C"
wind_pressure = "600 Pa"
ambient_temperature = "35 °C"

[[case]]
name = "own Q"
temperature = "10 °C"
wind_pressure = "240 Pa"
air_density_factor = 0.6
"#,
        )
        .unwrap();
        let (a, b) = (&cases.cases[0], &cases.cases[1]);
        assert_eq!(a.air_density_factor(), DEFAULT_AIR_DENSITY_FACTOR);
        assert!((a.wind_velocity() - (600.0f64 / 0.613).sqrt()).abs() < 1e-12);
        assert_eq!(a.ambient_temperature.map(|t| t.celsius()), Some(35.0));
        assert!((b.wind_velocity() - 20.0).abs() < 1e-12);
        let mut bad = b.clone();
        bad.air_density_factor = Some(0.0);
        assert!(bad.validate().is_err());
    }
}
