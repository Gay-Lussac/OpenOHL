// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Ambient cases for thermal ratings: `.olsta` files ("OLS thermal
//! ambient"), TOML with units in the values.
//!
//! ```toml
//! format = "ols-thermal-ambient"
//! version = 1
//! name = "Summer"
//!
//! [[case]]
//! name = "summer noon"
//! air_temperature = "40 °C"
//! wind_speed = "0.61 m/s"
//! wind_angle = "90 °"        # between wind and conductor (default 90 °;
//!                            # any angle, folded to 0–90°: 180° = along)
//! elevation = "0 m"          # default 0 m
//! # The sun from a date and time …
//! latitude = "30 °"          # north positive
//! date = "2023-06-10"        # or day_of_year = 161
//! time = "11:00"             # local solar time
//! line_azimuth = "90 °"      # from north; none: sun across the conductor
//! atmosphere = "clear"       # or "industrial"
//! # … or the irradiance on the conductor directly:
//! # solar_flux = "1000 W/m²"  (0 at night)
//! ```
//!
//! Optional per case: `emissivity`, `absorptivity` (override the cable's),
//! `albedo` (CIGRE ground reflectance, default 0.1), `clearness_ratio`
//! (CIGRE \\( N_s \\)), `roughness` (CIGRE \\( R_s \\)), `description`.

use std::path::Path;

use ols_core::units::{Angle, Length, Speed, Temperature};
use serde::Deserialize;

use crate::solar::{self, Atmosphere};

/// Value of the `format` header of `.olsta` files.
pub const AMBIENT_FILE_FORMAT: &str = "ols-thermal-ambient";
/// Latest `.olsta` format version.
pub const AMBIENT_FILE_VERSION: u32 = 1;
/// File extension of ambient case files.
pub const AMBIENT_FILE_EXTENSION: &str = "olsta";
/// Ground albedo (CIGRE) when a case gives none.
pub const DEFAULT_ALBEDO: f64 = 0.1;

/// Where the solar heating comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sun {
    /// Irradiance on the conductor's projected area, W/m² (0: night).
    Flux(f64),
    /// The sun at a place, day and time.
    Position {
        /// Latitude, degrees (north positive).
        latitude: f64,
        /// Day of the year (1 = 1 January).
        day: u32,
        /// Local solar time, hours.
        hour: f64,
        /// Azimuth of the line, degrees from north (none: the beam across
        /// the conductor).
        line_azimuth: Option<f64>,
        /// Atmosphere (IEEE flux; CIGRE clearness when none is given).
        atmosphere: Atmosphere,
        /// CIGRE clearness ratio.
        clearness: Option<f64>,
    },
}

/// One ambient case, in SI units and degrees.
#[derive(Debug, Clone, PartialEq)]
pub struct Ambient {
    /// Name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// Air temperature, °C.
    pub air_temperature: f64,
    /// Wind speed, m/s.
    pub wind_speed: f64,
    /// Angle between the wind and the conductor axis, degrees (90: across).
    pub wind_angle: f64,
    /// Elevation of the conductor above sea level, m.
    pub elevation: f64,
    /// The sun.
    pub sun: Sun,
    /// Emissivity instead of the cable's.
    pub emissivity: Option<f64>,
    /// Solar absorptivity instead of the cable's.
    pub absorptivity: Option<f64>,
    /// Ground albedo (CIGRE).
    pub albedo: f64,
    /// Surface roughness (CIGRE) instead of the default.
    pub roughness: Option<f64>,
}

impl Ambient {
    /// Still, dark air at `air` °C (for tests and simple uses).
    pub fn still(name: &str, air: f64) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            air_temperature: air,
            wind_speed: 0.0,
            wind_angle: 90.0,
            elevation: 0.0,
            sun: Sun::Flux(0.0),
            emissivity: None,
            absorptivity: None,
            albedo: DEFAULT_ALBEDO,
            roughness: None,
        }
    }

    /// The case in a few words: "40 °C air, 0.61 m/s at 90°, sun 11:00 day 161".
    pub fn summary(&self) -> String {
        let sun = match self.sun {
            Sun::Flux(0.0) => "no sun".to_owned(),
            Sun::Flux(f) => format!("{f:.0} W/m²"),
            Sun::Position { day, hour, .. } => format!(
                "sun day {day} {:02}:{:02}",
                hour.floor() as u32,
                ((hour.fract() * 60.0).round() as u32).min(59)
            ),
        };
        format!(
            "{} °C air, wind {} m/s at {}°, {sun}",
            self.air_temperature, self.wind_speed, self.wind_angle
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFile {
    name: String,
    #[serde(default)]
    description: String,
    air_temperature: Temperature,
    wind_speed: Speed,
    #[serde(default)]
    wind_angle: Option<Angle>,
    #[serde(default)]
    elevation: Option<Length>,
    #[serde(default)]
    latitude: Option<Angle>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    day_of_year: Option<u32>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    line_azimuth: Option<Angle>,
    #[serde(default)]
    atmosphere: Option<Atmosphere>,
    #[serde(default)]
    clearness_ratio: Option<f64>,
    #[serde(default)]
    solar_flux: Option<String>,
    #[serde(default)]
    emissivity: Option<f64>,
    #[serde(default)]
    absorptivity: Option<f64>,
    #[serde(default)]
    albedo: Option<f64>,
    #[serde(default)]
    roughness: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    name: String,
    #[serde(rename = "case", default)]
    cases: Vec<CaseFile>,
}

/// "1000 W/m²" (or W/m2, kW/m²) in W/m².
fn parse_flux(text: &str) -> Result<f64, String> {
    let t = text.trim();
    let (number, factor) = [("kW/m²", 1e3), ("kW/m2", 1e3), ("W/m²", 1.0), ("W/m2", 1.0)]
        .into_iter()
        .find_map(|(u, f)| t.strip_suffix(u).map(|n| (n.trim(), f)))
        .ok_or_else(|| format!("solar_flux \"{text}\": write e.g. \"1000 W/m²\""))?;
    number
        .parse::<f64>()
        .map(|v| v * factor)
        .map_err(|_| format!("solar_flux \"{text}\": invalid number"))
}

/// "2023-06-10" → day of the year.
fn parse_date(text: &str) -> Result<u32, String> {
    let bad = || format!("date \"{text}\": write YYYY-MM-DD");
    let parts: Vec<&str> = text.trim().split('-').collect();
    let [y, m, d] = parts[..] else {
        return Err(bad());
    };
    solar::day_of_year(
        y.parse().map_err(|_| bad())?,
        m.parse().map_err(|_| bad())?,
        d.parse().map_err(|_| bad())?,
    )
}

/// "11:30" (or "11:30:15") → hours.
fn parse_time(text: &str) -> Result<f64, String> {
    let bad = || format!("time \"{text}\": write HH:MM (local solar time)");
    let parts: Vec<f64> = text
        .trim()
        .split(':')
        .map(|p| p.parse::<f64>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    let hours = match parts[..] {
        [h, m] => h + m / 60.0,
        [h, m, s] => h + m / 60.0 + s / 3600.0,
        _ => return Err(bad()),
    };
    if !(0.0..=24.0).contains(&hours) {
        return Err(bad());
    }
    Ok(hours)
}

impl CaseFile {
    fn resolve(self) -> Result<Ambient, String> {
        let unit = |name: &str, v: Option<f64>| -> Result<(), String> {
            if v.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
                return Err(format!("{name} must be between 0 and 1"));
            }
            Ok(())
        };
        unit("emissivity", self.emissivity)?;
        unit("absorptivity", self.absorptivity)?;
        unit("albedo", self.albedo)?;
        if self.wind_speed.si() < 0.0 || !self.wind_speed.si().is_finite() {
            return Err("wind_speed must be ≥ 0".into());
        }
        if self.wind_angle.is_some_and(|a| !a.degrees().is_finite()) {
            return Err("wind_angle must be a finite angle".into());
        }
        if self
            .clearness_ratio
            .is_some_and(|c| !(0.0..=1.5).contains(&c))
        {
            return Err("clearness_ratio must be between 0 and 1.5".into());
        }
        if self.roughness.is_some_and(|r| !(0.0..1.0).contains(&r)) {
            return Err("roughness must be between 0 and 1".into());
        }
        let place = self.latitude.is_some() || self.date.is_some() || self.time.is_some();
        let sun = match (&self.solar_flux, place) {
            (Some(_), true) => {
                return Err("give either solar_flux or latitude, date and time".into());
            }
            (Some(f), false) => {
                let f = parse_flux(f)?;
                if f < 0.0 {
                    return Err("solar_flux must be ≥ 0".into());
                }
                Sun::Flux(f)
            }
            (None, true) => {
                let latitude = self
                    .latitude
                    .ok_or("the sun's position needs latitude")?
                    .degrees();
                if !(-90.0..=90.0).contains(&latitude) {
                    return Err("latitude must be between -90° and 90°".into());
                }
                let day = match (&self.date, self.day_of_year) {
                    (Some(d), None) => parse_date(d)?,
                    (None, Some(n)) if (1..=366).contains(&n) => n,
                    (None, Some(_)) => return Err("day_of_year must be 1–366".into()),
                    _ => return Err("give date or day_of_year (one of them)".into()),
                };
                let hour = parse_time(self.time.as_deref().ok_or("the sun needs a time")?)?;
                Sun::Position {
                    latitude,
                    day,
                    hour,
                    line_azimuth: self.line_azimuth.map(|a| a.degrees()),
                    atmosphere: self.atmosphere.unwrap_or_default(),
                    clearness: self.clearness_ratio,
                }
            }
            (None, false) => {
                return Err(
                    "no sun given: write solar_flux (\"0 W/m²\" at night) or latitude, date and time"
                        .into(),
                );
            }
        };
        Ok(Ambient {
            name: self.name,
            description: self.description,
            air_temperature: self.air_temperature.celsius(),
            wind_speed: self.wind_speed.si(),
            wind_angle: self.wind_angle.map_or(90.0, |a| a.degrees()),
            elevation: self.elevation.map_or(0.0, |e| e.si()),
            sun,
            emissivity: self.emissivity,
            absorptivity: self.absorptivity,
            albedo: self.albedo.unwrap_or(DEFAULT_ALBEDO),
            roughness: self.roughness,
        })
    }
}

/// The ambient cases of a `.olsta` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AmbientCases {
    /// Name of the set.
    pub name: String,
    /// The cases, in file order.
    pub cases: Vec<Ambient>,
}

impl AmbientCases {
    /// Parses and checks a file's text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        ols_core::files::check_header(
            file.format.as_deref(),
            file.version,
            AMBIENT_FILE_FORMAT,
            AMBIENT_FILE_VERSION,
            "thermal ambient",
        )?;
        let mut cases: Vec<Ambient> = Vec::new();
        for c in file.cases {
            let name = c.name.clone();
            if cases.iter().any(|x| x.name == name) {
                return Err(format!("ambient case \"{name}\" is defined twice"));
            }
            cases.push(c.resolve().map_err(|e| format!("case \"{name}\": {e}"))?);
        }
        if cases.is_empty() {
            return Err("no [[case]] in the file".into());
        }
        Ok(Self {
            name: file.name,
            cases,
        })
    }

    /// Reads a file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The cases whose name matches one of `names` (`*` `?` wildcards,
    /// case-insensitive), all without names; a name matching nothing is an
    /// error.
    pub fn select(&self, names: &[String]) -> Result<Vec<Ambient>, String> {
        if names.is_empty() {
            return Ok(self.cases.clone());
        }
        let m = |n: &str, c: &Ambient| ols_core::pattern::matches(n, &c.name);
        if let Some(n) = names.iter().find(|n| !self.cases.iter().any(|c| m(n, c))) {
            return Err(format!("no ambient case matches \"{n}\""));
        }
        Ok(self
            .cases
            .iter()
            .filter(|c| names.iter().any(|n| m(n, c)))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
        format = "ols-thermal-ambient"
        version = 1
        name = "t"

        [[case]]
        name = "noon"
        air_temperature = "40 °C"
        wind_speed = "2 ft/s"
        latitude = "30 °"
        date = "2023-06-10"
        time = "11:00"
        line_azimuth = "90 °"

        [[case]]
        name = "night"
        air_temperature = "20 °C"
        wind_speed = "0.6 m/s"
        wind_angle = "45 °"
        elevation = "1000 m"
        solar_flux = "0 W/m²"
        emissivity = 0.5
    "#;

    #[test]
    fn parses_the_cases() {
        let set = AmbientCases::parse(EXAMPLE);
        // ft/s is not a speed unit: an error that names the problem.
        assert!(set.is_err());
        let set = AmbientCases::parse(&EXAMPLE.replace("2 ft/s", "0.61 m/s")).unwrap();
        assert_eq!(set.cases.len(), 2);
        let noon = &set.cases[0];
        let Sun::Position {
            latitude,
            day: 161,
            hour,
            line_azimuth: Some(azimuth),
            atmosphere: Atmosphere::Clear,
            clearness: None,
        } = noon.sun
        else {
            panic!("{:?}", noon.sun);
        };
        assert!((latitude - 30.0).abs() < 1e-9 && (azimuth - 90.0).abs() < 1e-9);
        assert_eq!(hour, 11.0);
        assert!((noon.wind_angle - 90.0).abs() < 1e-12);
        let night = &set.cases[1];
        assert_eq!(night.sun, Sun::Flux(0.0));
        assert!((night.wind_angle - 45.0).abs() < 1e-9);
        assert_eq!(night.elevation, 1000.0);
        assert_eq!(night.emissivity, Some(0.5));
        assert_eq!(set.select(&["N*".into()]).unwrap().len(), 2);
        assert!(set.select(&["dusk".into()]).is_err());
    }

    #[test]
    fn wind_angle_any_finite_value() {
        let with = |angle: &str| {
            AmbientCases::parse(&EXAMPLE.replace("2 ft/s", "0.61 m/s").replace("45 °", angle))
        };
        let night = |angle: &str| with(angle).unwrap().cases[1].wind_angle;
        assert!((night("270 °") - 270.0).abs() < 1e-9);
        assert!((night("-45 °") + 45.0).abs() < 1e-9);
        for bad in ["inf °", "NaN °", "-inf °"] {
            assert!(with(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_contradictions_and_typos() {
        let ok = EXAMPLE.replace("2 ft/s", "0.61 m/s");
        for (from, to) in [
            (
                "solar_flux = \"0 W/m²\"",
                "solar_flux = \"0 W/m²\"\nlatitude = \"10 °\"",
            ),
            ("emissivity = 0.5", "emissivity = 5"),
            ("date = \"2023-06-10\"", "date = \"2023-02-30\""),
            ("time = \"11:00\"", "time = \"eleven\""),
            ("emissivity = 0.5", "emisivity = 0.5"),
            ("solar_flux = \"0 W/m²\"", ""),
        ] {
            assert!(AmbientCases::parse(&ok.replace(from, to)).is_err(), "{to}");
        }
    }

    #[test]
    fn example_file_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/thermal-ambient.olsta");
        let set = AmbientCases::load(&path).unwrap();
        assert!(set.cases.len() >= 2);
        // The example of docs/user-guide/thermal.md.
        let doc = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user-guide/thermal.md"),
        )
        .unwrap();
        let block = &doc[doc.find("## Ambient cases").unwrap()..];
        let code = &block[block.find("```toml\n").unwrap() + 8..];
        let set = AmbientCases::parse(&code[..code.find("```").unwrap()]).unwrap();
        assert_eq!(set.cases.len(), 2);
    }
}
