// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
//! - load: \\( w_i \\) as given; with a density the ice area
//!   \\( A_i = w_i / (\rho_i g) \\) gives the iced diameter
//!   \\( D = \sqrt{d^2 + 4 A_i / \pi} \\) (more wind area for light rime
//!   than for dense glaze of the same weight); without one, wind acts on the
//!   bare diameter;
//! - vertical load \\( w_v = w_c + w_i \\);
//! - wind load \\( w_h = q\,D \\) on the projected iced diameter;
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
    /// Ice density: with a thickness, default glaze ice (913 kg/m³); with a
    /// load, gives the iced diameter for the wind (none: bare diameter).
    #[serde(default)]
    pub ice_density: Option<Density>,
    /// Wind pressure on the projected (iced) diameter (0 for no wind).
    #[serde(default)]
    pub wind_pressure: Stress,
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
        }
    }

    /// Checks the case: ice as a thickness or a load (not both), values
    /// not negative, density positive.
    pub fn validate(&self) -> Result<(), String> {
        let bad = |m: &str| Err(format!("weather case \"{}\": {m}", self.name));
        if self.ice_thickness.si() < 0.0 || self.wind_pressure.si() < 0.0 {
            return bad("ice thickness and wind pressure must be ≥ 0");
        }
        if let Some(w) = self.ice_load {
            if w.si() < 0.0 {
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

    /// Ice weight per length and iced diameter, m, on a bare diameter `d`
    /// (m).
    fn ice(&self, d: f64) -> (f64, f64) {
        match self.ice_load {
            Some(w) => {
                let iced = self.ice_density.map_or(d, |rho| {
                    let area = w.si() / (rho.si() * ols_core::units::STANDARD_GRAVITY);
                    (d * d + 4.0 * area / std::f64::consts::PI).sqrt()
                });
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
                "ice {:.2} N/m{}",
                w.newtons_per_meter(),
                self.ice_density.map_or(String::new(), |d| format!(
                    " ({:.0} kg/m³)",
                    d.kilograms_per_cubic_meter()
                ))
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
            parts.push(format!("wind {:.0} Pa", self.wind_pressure.pascals()));
        }
        parts.join(", ")
    }
}

/// File extension of weather case files.
pub const WEATHER_FILE_EXTENSION: &str = "olswc";
/// Format identifier.
pub const WEATHER_FILE_FORMAT: &str = "ols-weather-cases";

/// A set of weather cases (`*.olswc`).
///
/// ```toml
/// format = "ols-weather-cases"
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
    /// `"ols-weather-cases"` (optional header).
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

/// `*` (any text) and `?` (one character) wildcards, case-insensitive.
fn matches(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (
        pattern.to_lowercase().chars().collect(),
        text.to_lowercase().chars().collect(),
    );
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}

impl WeatherCases {
    /// Parses and checks a case file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let set: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        if let Some(f) = set.format.as_deref().filter(|f| *f != WEATHER_FILE_FORMAT) {
            return Err(format!("format \"{f}\" is not \"{WEATHER_FILE_FORMAT}\""));
        }
        if set.version.is_some_and(|v| v > 1) {
            return Err("weather case format version newer than supported (1)".into());
        }
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
        // Without a density: the weight only, wind on the bare diameter.
        let flat = WeatherCase {
            ice_density: None,
            ..as_load
        };
        let l = flat.wire_load(d, w);
        close(l.vertical.si(), reference.vertical.si(), 1e-12);
        close(l.horizontal.si(), 200.0 * 0.02814, 1e-12);
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
            format = "ols-weather-cases"
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
}
