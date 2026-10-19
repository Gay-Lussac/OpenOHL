// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared by the OpenOHL design-code plugins (`code-plugin` world): reading
//! the user's parameter file, building weather cases and load cases, and
//! writing them as `.ohlwc` / `.ohllc` text; presets of the clearance
//! windows ([`Preset`], their rules written as `.ohlr` text).
//!
//! The plugins implement each code's *method* (which cases, how a wind
//! pressure follows from a wind speed, which factor goes where). Every
//! numeric value of a code — wind speeds, ice, temperatures, partial and
//! combination factors — comes from the parameter file: OpenOHL ships no
//! code tables.
//!
//! Generating is a few dozen arithmetic operations (microseconds); the
//! host's per-call time limit is the exit clause.

use serde::Deserialize;
use serde::de::DeserializeOwned;

pub use toml;

/// Implements the `code-plugin` world in a plugin crate (after its
/// `wit_bindgen::generate!`): `describe` gives `$name`, `$description` and
/// the crate's `example-parameters.toml` as the template; `generate` runs
/// `$cases` (parameter text → [`Cases`]), checks and writes the files.
#[macro_export]
macro_rules! code_plugin {
    ($name:expr, $description:expr, $cases:path) => {
        struct Code;

        impl Guest for Code {
            fn describe() -> CodeInfo {
                CodeInfo {
                    name: $name.into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    description: $description.into(),
                    parameters_template: include_str!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/example-parameters.toml"
                    ))
                    .into(),
                }
            }

            fn generate(parameters: String) -> Result<Generated, String> {
                let c = $cases(&parameters)?;
                c.check()?;
                Ok(Generated {
                    weather_cases: c.weather_text(),
                    load_cases: c.load_cases_text(),
                    clearance_presets: c
                        .presets
                        .iter()
                        .map(|p| ClearancePreset {
                            name: p.name.clone(),
                            measure: match p.measure {
                                $crate::Measure::Vertical => ClearanceMeasure::Vertical,
                                $crate::Measure::Radial => ClearanceMeasure::Radial,
                            },
                            rules: p.rules.text(),
                            weather_cases: c.preset_weather_text(p),
                        })
                        .collect(),
                    notes: c.notes,
                })
            }
        }

        export!(Code);
    };
}

/// Implements the optional `structure-wind` interface (the
/// `code-wind-plugin` world, after its `wit_bindgen::generate!`) giving
/// **none**: the plugin does not supply wind factors for structures yet
/// (drag per shape, a lattice face's drag and shielding by solidity), so
/// OpenOHL keeps its generic drag factor and no shielding, and says so.
/// A plugin that implements its code's factors replaces this with its own
/// `structure_wind`, returning a section library's text with a `[wind]`
/// table built from the parameter file (no values typed into the plugin).
#[macro_export]
macro_rules! no_structure_wind {
    () => {
        impl exports::ohl::plugin::structure_wind::Guest for Code {
            fn structure_wind(_parameters: String) -> Result<Option<String>, String> {
                Ok(None)
            }
        }
    };
}

/// Reads a parameter file into `T` (unknown keys are errors when `T`
/// says so).
pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    toml::from_str(text).map_err(|e| format!("parameter file: {e}"))
}

/// Reads a parameter file into a table (to merge an overlay first).
pub fn parse_table(text: &str) -> Result<toml::Table, String> {
    text.parse::<toml::Table>()
        .map_err(|e| format!("parameter file: {e}"))
}

/// Merges `overlay` into `base`: tables key by key (recursively), every
/// other value replaced. Returns the keys replaced, as `a.b.c`.
pub fn merge(base: &mut toml::Table, overlay: &toml::Table, prefix: &str) -> Vec<String> {
    let mut replaced = Vec::new();
    for (k, v) in overlay {
        let path = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match (base.get_mut(k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => {
                replaced.extend(merge(b, o, &path));
            }
            _ => {
                base.insert(k.clone(), v.clone());
                replaced.push(path);
            }
        }
    }
    replaced
}

/// A table read into `T`.
pub fn from_table<T: DeserializeOwned>(table: toml::Table) -> Result<T, String> {
    T::deserialize(toml::Value::Table(table)).map_err(|e| format!("parameter file: {e}"))
}

/// A value that must be finite and in `lo..=hi`.
pub fn check(what: &str, x: f64, lo: f64, hi: f64) -> Result<f64, String> {
    if x.is_finite() && (lo..=hi).contains(&x) {
        Ok(x)
    } else {
        Err(format!("{what} = {x}: expected {lo} to {hi}"))
    }
}

/// Dynamic pressure ½ρV², Pa, of air of density `rho` (kg/m³) at speed
/// `v` (m/s).
pub fn dynamic_pressure(rho: f64, v: f64) -> f64 {
    0.5 * rho * v * v
}

/// Unit conversions for the codes written in US customary units (exact
/// definitions, NIST SP 811).
pub mod us {
    /// Pascals per pound-force per square foot.
    pub const PA_PER_PSF: f64 = 4.448_221_615_260_5 / (0.3048 * 0.3048);
    /// Millimetres per inch.
    pub const MM_PER_INCH: f64 = 25.4;
    /// Metres per second per mile per hour.
    pub const MPS_PER_MPH: f64 = 0.447_04;
    /// Newtons per metre per pound-force per foot.
    pub const N_PER_M_PER_LBF_PER_FT: f64 = 4.448_221_615_260_5 / 0.3048;
    /// Kilograms per cubic metre per pound-mass per cubic foot.
    pub const KG_M3_PER_LB_FT3: f64 = 0.453_592_37 / (0.3048 * 0.3048 * 0.3048);

    /// °F to °C.
    pub fn celsius(fahrenheit: f64) -> f64 {
        (fahrenheit - 32.0) / 1.8
    }
}

/// The wires' condition of a load case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// As strung.
    #[default]
    Initial,
    /// After creep.
    Creep,
    /// After the heaviest load.
    Load,
    /// After creep or load, whichever sags more.
    MaxSag,
}

impl Condition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Creep => "creep",
            Self::Load => "load",
            Self::MaxSag => "max_sag",
        }
    }
}

/// A weather case (SI units).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Weather {
    /// Unique name.
    pub name: String,
    /// What it stands for.
    pub description: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Wire temperature, °C.
    pub temperature: f64,
    /// Wind pressure on the projected (iced) diameter, Pa.
    pub wind_pressure: f64,
    /// Radial ice thickness, mm.
    pub ice_thickness: f64,
    /// Or ice load per length, N/m.
    pub ice_load: Option<f64>,
    /// Ice density, kg/m³.
    pub ice_density: Option<f64>,
    /// Air density factor Q (pressure = Q·V²), Pa/(m/s)².
    pub air_density_factor: Option<f64>,
}

impl Weather {
    /// Still air, no ice, at `temperature` °C.
    pub fn new(name: impl Into<String>, temperature: f64) -> Self {
        Self {
            name: name.into(),
            temperature,
            ..Self::default()
        }
    }

    /// With a description.
    pub fn describe(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }

    /// With tags.
    pub fn tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|t| (*t).to_owned()).collect();
        self
    }

    /// With a wind pressure, Pa.
    pub fn wind(mut self, pressure: f64) -> Self {
        self.wind_pressure = pressure;
        self
    }

    /// With radial ice of `thickness` mm and `density` kg/m³.
    pub fn ice_thickness(mut self, thickness: f64, density: f64) -> Self {
        self.ice_thickness = thickness;
        self.ice_load = None;
        self.ice_density = Some(density);
        self
    }

    /// With ice of `load` N/m (and a density for the iced diameter).
    pub fn ice_load(mut self, load: f64, density: Option<f64>) -> Self {
        self.ice_thickness = 0.0;
        self.ice_load = Some(load);
        self.ice_density = density;
        self
    }

    /// With an air density factor Q, Pa/(m/s)².
    pub fn air_density_factor(mut self, q: f64) -> Self {
        self.air_density_factor = Some(q);
        self
    }
}

/// A load case: a weather case, a wire condition and factors.
#[derive(Debug, Clone, PartialEq)]
pub struct Load {
    /// Unique name.
    pub name: String,
    /// What it stands for (e.g. a clause).
    pub description: String,
    /// The weather case, by name.
    pub weather: String,
    /// The wires' condition.
    pub condition: Condition,
    /// Factor on the vertical loads.
    pub vertical: f64,
    /// Factor on the transverse loads.
    pub transverse: f64,
    /// Factor on the longitudinal loads.
    pub longitudinal: f64,
    /// Strength factor of the capacity checks.
    pub strength: f64,
    /// Tags.
    pub tags: Vec<String>,
}

impl Load {
    /// All factors 1, initial condition.
    pub fn new(name: impl Into<String>, weather: &Weather) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            weather: weather.name.clone(),
            condition: Condition::Initial,
            vertical: 1.0,
            transverse: 1.0,
            longitudinal: 1.0,
            strength: 1.0,
            tags: Vec::new(),
        }
    }

    /// With a description.
    pub fn describe(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }

    /// With factors on the vertical, transverse and longitudinal loads.
    pub fn factors(mut self, vertical: f64, transverse: f64, longitudinal: f64) -> Self {
        self.vertical = vertical;
        self.transverse = transverse;
        self.longitudinal = longitudinal;
        self
    }

    /// With a strength factor.
    pub fn strength(mut self, factor: f64) -> Self {
        self.strength = factor;
        self
    }

    /// In a wire condition.
    pub fn condition(mut self, condition: Condition) -> Self {
        self.condition = condition;
        self
    }

    /// With tags.
    pub fn tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|t| (*t).to_owned()).collect();
        self
    }
}

/// A weather case given as it is in the parameter file (`[[extra_weather]]`:
/// national annexes, project cases), SI units.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraWeather {
    /// Unique name.
    pub name: String,
    /// What it stands for.
    #[serde(default)]
    pub description: String,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Wire temperature, °C.
    pub temperature: f64,
    /// Wind pressure, Pa.
    #[serde(default)]
    pub wind_pressure: f64,
    /// Radial ice, mm.
    #[serde(default)]
    pub ice_thickness: f64,
    /// Or ice load, N/m.
    #[serde(default)]
    pub ice_load: Option<f64>,
    /// Ice density, kg/m³.
    #[serde(default)]
    pub ice_density: Option<f64>,
}

impl ExtraWeather {
    /// As a weather case.
    pub fn weather(&self) -> Weather {
        Weather {
            name: self.name.clone(),
            description: self.description.clone(),
            tags: self.tags.clone(),
            temperature: self.temperature,
            wind_pressure: self.wind_pressure,
            ice_thickness: self.ice_thickness,
            ice_load: self.ice_load,
            ice_density: self.ice_density,
            air_density_factor: None,
        }
    }
}

fn one() -> f64 {
    1.0
}

/// A load case given as it is in the parameter file (`[[extra_load_case]]`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraLoad {
    /// Unique name.
    pub name: String,
    /// What it stands for.
    #[serde(default)]
    pub description: String,
    /// The weather case, by name.
    pub weather: String,
    /// The wires' condition.
    #[serde(default)]
    pub condition: Condition,
    /// Factor on the vertical loads.
    #[serde(default = "one")]
    pub vertical: f64,
    /// Factor on the transverse loads.
    #[serde(default = "one")]
    pub transverse: f64,
    /// Factor on the longitudinal loads.
    #[serde(default = "one")]
    pub longitudinal: f64,
    /// Strength factor.
    #[serde(default = "one")]
    pub strength: f64,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ExtraLoad {
    /// As a load case.
    pub fn load(&self) -> Load {
        Load {
            name: self.name.clone(),
            description: self.description.clone(),
            weather: self.weather.clone(),
            condition: self.condition,
            vertical: self.vertical,
            transverse: self.transverse,
            longitudinal: self.longitudinal,
            strength: self.strength,
            tags: self.tags.clone(),
        }
    }
}

/// Weather cases, load cases and notes generated by a code.
#[derive(Debug, Clone, Default)]
pub struct Cases {
    /// Name of both files' sets.
    pub name: String,
    /// Comment lines at the top of both files.
    pub header: Vec<String>,
    /// The weather cases.
    pub weather: Vec<Weather>,
    /// The load cases.
    pub loads: Vec<Load>,
    /// What was generated, assumed or left out.
    pub notes: Vec<String>,
    /// Presets of the clearance windows.
    pub presets: Vec<Preset>,
}

/// The clearance window a preset is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// Vertical clearances (the zone under the wire).
    Vertical,
    /// Radial clearances (the direct 3D distance).
    Radial,
}

/// A preset of a clearance window: its rules and the weather cases they
/// name.
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// Name, unique among the plugin's presets.
    pub name: String,
    /// The window.
    pub measure: Measure,
    /// The rules.
    pub rules: RuleFile,
    /// The weather cases the rules name.
    pub weather: Vec<Weather>,
}

/// One rule of a clearance rule file (`.ohlr`): to points of LiDAR
/// classes and/or the ground.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClearanceRule {
    /// Name.
    pub name: String,
    /// LiDAR classes of the targets.
    pub classes: Vec<u8>,
    /// The ground surface is a target too.
    pub ground: bool,
    /// Vertical clearance, m.
    pub vertical: Option<f64>,
    /// Radial clearance, m.
    pub radial: Option<f64>,
    /// Vertical: half width of the zone across the line, m (`None`: the
    /// window's).
    pub vertical_band: Option<f64>,
    /// Only the wires of these nominal voltages, kV (0: earthed); empty:
    /// every wire.
    pub voltages: Vec<f64>,
    /// Weather cases (names); empty: every case.
    pub cases: Vec<String>,
}

/// A clearance rule file (`.ohlr`, format `ohl-clearance-rules`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleFile {
    /// Name.
    pub name: String,
    /// Comment lines at the top.
    pub header: Vec<String>,
    /// `[wind]`: every pressure from 0 up to the case's, in steps.
    pub wind_steps: bool,
    /// `[wind]`: the step, Pa (`None`: the project's).
    pub wind_step: Option<f64>,
    /// `[wind]`: `both`, `left` or `right`.
    pub wind_sides: String,
    /// `[check]`: circuit names (empty: every circuit).
    pub circuits: Vec<String>,
    /// `[check]`: `fe` or `rs` (`None`: each section's).
    pub method: Option<String>,
    /// `[check]`: LiDAR points farther from the wires are ignored, m.
    pub search_distance: Option<f64>,
    /// The rules.
    pub rules: Vec<ClearanceRule>,
}

impl RuleFile {
    /// The rule file's text.
    pub fn text(&self) -> String {
        let mut s = String::new();
        for h in &self.header {
            s += &format!("# {h}\n");
        }
        s += &format!(
            "format = \"ohl-clearance-rules\"\nversion = 1\nname = {}\n",
            quote(&self.name)
        );
        s += &format!(
            "\n[wind]\nsteps = {}\nsides = {}\n",
            self.wind_steps,
            quote(if self.wind_sides.is_empty() {
                "both"
            } else {
                &self.wind_sides
            })
        );
        if let Some(step) = self.wind_step {
            s += &format!("step = \"{} Pa\"\n", num(step));
        }
        if !self.circuits.is_empty() || self.method.is_some() || self.search_distance.is_some() {
            s += "\n[check]\n";
            if !self.circuits.is_empty() {
                s += &format!("circuits = {}\n", tags(&self.circuits));
            }
            if let Some(m) = &self.method {
                s += &format!("method = {}\n", quote(m));
            }
            if let Some(d) = self.search_distance {
                s += &format!("search_distance = \"{} m\"\n", num(d));
            }
        }
        for r in &self.rules {
            let list = |v: &[String]| format!("[{}]", v.join(", "));
            if !r.classes.is_empty() || !r.ground {
                s += &format!(
                    "\n[[rule]]\nname = {}\ntarget = \"points\"\nclasses = {}\n",
                    quote(&r.name),
                    list(&r.classes.iter().map(u8::to_string).collect::<Vec<_>>())
                );
                s += &r.requirements();
            }
            if r.ground {
                s += &format!(
                    "\n[[rule]]\nname = {}\ntarget = \"ground\"\n",
                    quote(&r.name)
                );
                s += &r.requirements();
            }
        }
        s
    }
}

impl ClearanceRule {
    /// Its keys after the target.
    fn requirements(&self) -> String {
        let mut s = String::new();
        if let Some(v) = self.vertical {
            s += &format!("vertical = \"{} m\"\n", num(v));
        }
        if let Some(r) = self.radial {
            s += &format!("radial = \"{} m\"\n", num(r));
        }
        if let Some(b) = self.vertical_band {
            s += &format!("vertical_band = \"{} m\"\n", num(b));
        }
        if !self.voltages.is_empty() {
            let v: Vec<String> = self.voltages.iter().map(|v| format!("{v:?}")).collect();
            s += &format!("voltages = [{}]\n", v.join(", "));
        }
        if !self.cases.is_empty() {
            s += &format!("cases = {}\n", tags(&self.cases));
        }
        s
    }
}

/// TOML string.
fn quote(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

/// A number written without float noise (12 significant digits).
fn num(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    let digits = 12 - (x.abs().log10().floor() as i32 + 1);
    let scale = 10f64.powi(digits.clamp(0, 15));
    let r = (x * scale).round() / scale;
    format!("{r}")
}

fn tags(t: &[String]) -> String {
    let list: Vec<String> = t.iter().map(|s| quote(s)).collect();
    format!("[{}]", list.join(", "))
}

impl Cases {
    /// Empty, with the usual header: which plugin generated the files,
    /// from what.
    pub fn new(name: &str, plugin: &str, version: &str, method: &str) -> Self {
        Self {
            name: name.to_owned(),
            header: vec![
                format!(
                    "Generated by the OpenOHL design-code plugin {plugin} {version}: {method}."
                ),
                "Every value comes from the user's parameter file, not from the standard:"
                    .to_owned(),
                "check the parameter file against the standard and its national annex.".to_owned(),
            ],
            ..Self::default()
        }
    }

    /// Adds the cases given in the parameter file.
    pub fn add_extra(&mut self, weather: &[ExtraWeather], loads: &[ExtraLoad]) {
        self.weather
            .extend(weather.iter().map(ExtraWeather::weather));
        self.loads.extend(loads.iter().map(ExtraLoad::load));
        if !weather.is_empty() || !loads.is_empty() {
            self.notes.push(format!(
                "added as given in the parameter file: weather cases {}, load cases {}",
                weather.len(),
                loads.len()
            ));
        }
    }

    /// Checks names (unique, load cases naming a weather case) and values
    /// (not negative, factors 0 to 100, strength above 0).
    pub fn check(&self) -> Result<(), String> {
        let mut names = std::collections::BTreeSet::new();
        for w in &self.weather {
            if w.name.trim().is_empty() || !names.insert(w.name.as_str()) {
                return Err(format!("weather case \"{}\": empty or twice", w.name));
            }
            let values = [w.wind_pressure, w.ice_thickness, w.ice_load.unwrap_or(0.0)];
            if values.iter().any(|x| !(x.is_finite() && *x >= 0.0)) || !w.temperature.is_finite() {
                return Err(format!(
                    "weather case \"{}\": wind and ice must be ≥ 0",
                    w.name
                ));
            }
            if w.ice_density.is_some_and(|d| !(d > 0.0 && d.is_finite())) {
                return Err(format!("weather case \"{}\": ice density ≤ 0", w.name));
            }
        }
        let mut presets = std::collections::BTreeSet::new();
        for p in &self.presets {
            if p.name.trim().is_empty() || !presets.insert(p.name.as_str()) {
                return Err(format!("clearance preset \"{}\": empty or twice", p.name));
            }
            let check = Self {
                weather: p.weather.clone(),
                ..Self::default()
            };
            check
                .check()
                .map_err(|e| format!("clearance preset \"{}\": {e}", p.name))?;
        }
        let mut loads = std::collections::BTreeSet::new();
        for l in &self.loads {
            if l.name.trim().is_empty() || !loads.insert(l.name.as_str()) {
                return Err(format!("load case \"{}\": empty or twice", l.name));
            }
            if !names.contains(l.weather.as_str()) {
                return Err(format!(
                    "load case \"{}\": no weather case \"{}\"",
                    l.name, l.weather
                ));
            }
            for (what, f) in [
                ("vertical", l.vertical),
                ("transverse", l.transverse),
                ("longitudinal", l.longitudinal),
                ("strength", l.strength),
            ] {
                if !(f.is_finite() && (0.0..=100.0).contains(&f)) {
                    return Err(format!("load case \"{}\": {what} factor {f}", l.name));
                }
            }
            if l.strength == 0.0 {
                return Err(format!("load case \"{}\": strength factor 0", l.name));
            }
        }
        Ok(())
    }

    fn head(&self, format: &str) -> String {
        let mut s = String::new();
        for h in &self.header {
            s += &format!("# {h}\n");
        }
        s += &format!(
            "format = {}\nversion = 1\nname = {}\n",
            quote(format),
            quote(&self.name)
        );
        s
    }

    /// The weather case file (`.ohlwc`).
    pub fn weather_text(&self) -> String {
        self.weather_file(&self.weather)
    }

    /// The weather case file of a preset's cases (`.ohlwc`).
    pub fn preset_weather_text(&self, preset: &Preset) -> String {
        Self {
            name: format!("{} — {}", self.name, preset.name),
            header: self.header.clone(),
            ..Self::default()
        }
        .weather_file(&preset.weather)
    }

    fn weather_file(&self, weather: &[Weather]) -> String {
        let mut s = self.head("ohl-weather-cases");
        for w in weather {
            s += &format!("\n[[case]]\nname = {}\n", quote(&w.name));
            if !w.description.is_empty() {
                s += &format!("description = {}\n", quote(&w.description));
            }
            if !w.tags.is_empty() {
                s += &format!("tags = {}\n", tags(&w.tags));
            }
            s += &format!("temperature = \"{} °C\"\n", num(w.temperature));
            match w.ice_load {
                Some(l) => s += &format!("ice_load = \"{} N/m\"\n", num(l)),
                None if w.ice_thickness > 0.0 => {
                    s += &format!("ice_thickness = \"{} mm\"\n", num(w.ice_thickness));
                }
                None => {}
            }
            if let Some(d) = w.ice_density
                && (w.ice_load.is_some() || w.ice_thickness > 0.0)
            {
                s += &format!("ice_density = \"{} kg/m³\"\n", num(d));
            }
            if w.wind_pressure > 0.0 {
                s += &format!("wind_pressure = \"{} Pa\"\n", num(w.wind_pressure));
            }
            if let Some(q) = w.air_density_factor {
                s += &format!("air_density_factor = {}\n", num(q));
            }
        }
        s
    }

    /// The load case file (`.ohllc`).
    pub fn load_cases_text(&self) -> String {
        let mut s = self.head("ohl-load-cases");
        for l in &self.loads {
            s += &format!("\n[[case]]\nname = {}\n", quote(&l.name));
            if !l.description.is_empty() {
                s += &format!("description = {}\n", quote(&l.description));
            }
            s += &format!(
                "weather = {}\ncondition = \"{}\"\nvertical = {}\ntransverse = {}\n\
                 longitudinal = {}\nstrength = {}\n",
                quote(&l.weather),
                l.condition.as_str(),
                num(l.vertical),
                num(l.transverse),
                num(l.longitudinal),
                num(l.strength)
            );
            if !l.tags.is_empty() {
                s += &format!("tags = {}\n", tags(&l.tags));
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_without_noise() {
        assert_eq!(num(604.8000000000001), "604.8");
        assert_eq!(num(-20.0), "-20");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(num(1.0 / 1.1), "0.909090909091");
    }

    #[test]
    fn files_are_written_and_checked() {
        let w = Weather::new("wind \"A\"", 10.0)
            .wind(604.8)
            .tags(&["loads"])
            .air_density_factor(0.625);
        let i = Weather::new("ice", -5.0).ice_load(20.0, Some(500.0));
        let mut c = Cases {
            name: "test".into(),
            header: vec!["generated".into()],
            weather: vec![w.clone(), i],
            loads: vec![Load::new("LC1", &w).factors(1.0, 1.2, 1.0).strength(0.9)],
            notes: vec![],
            presets: vec![],
        };
        c.check().unwrap();
        let text = c.weather_text();
        let read: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(read["case"][0]["name"].as_str(), Some("wind \"A\""));
        assert!(text.contains("wind_pressure = \"604.8 Pa\""), "{text}");
        assert!(text.contains("ice_load = \"20 N/m\""), "{text}");
        let lc = c.load_cases_text();
        assert!(lc.contains("transverse = 1.2\nlongitudinal = 1\nstrength = 0.9"));
        c.loads.push(Load::new("LC1", &w));
        assert!(c.check().is_err());
    }

    #[test]
    fn overlays_replace_keys() {
        let mut base = parse_table("[wind]\na = 1\nb = 2\n[x]\nc = 3").unwrap();
        let overlay = parse_table("[wind]\nb = 5").unwrap();
        assert_eq!(merge(&mut base, &overlay, ""), ["wind.b"]);
        assert_eq!(base["wind"]["b"].as_integer(), Some(5));
        assert_eq!(base["wind"]["a"].as_integer(), Some(1));
    }

    #[test]
    fn us_units() {
        assert!((us::PA_PER_PSF - 47.880_258_98).abs() < 1e-6);
        assert!((us::N_PER_M_PER_LBF_PER_FT - 14.593_903).abs() < 1e-5);
        assert_eq!(us::celsius(32.0), 0.0);
    }

    #[test]
    fn presets_are_written_and_checked() {
        let hot = Weather::new("hot", 80.0);
        let rules = RuleFile {
            name: "p".into(),
            wind_sides: "both".into(),
            method: Some("fe".into()),
            search_distance: Some(30.0),
            rules: vec![ClearanceRule {
                name: "roads 380".into(),
                classes: vec![11, 12],
                ground: true,
                vertical: Some(7.5),
                vertical_band: Some(2.0),
                voltages: vec![380.0],
                cases: vec!["hot".into()],
                ..ClearanceRule::default()
            }],
            ..RuleFile::default()
        };
        let text = rules.text();
        let read: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(read["check"]["method"].as_str(), Some("fe"));
        assert_eq!(read["check"]["search_distance"].as_str(), Some("30 m"));
        assert_eq!(read["rule"].as_array().unwrap().len(), 2, "{text}");
        assert_eq!(read["rule"][0]["vertical_band"].as_str(), Some("2 m"));
        assert_eq!(read["rule"][1]["target"].as_str(), Some("ground"));
        let preset = Preset {
            name: "vertical".into(),
            measure: Measure::Vertical,
            rules,
            weather: vec![hot.clone()],
        };
        let mut c = Cases {
            name: "t".into(),
            presets: vec![preset.clone(), preset.clone()],
            ..Cases::default()
        };
        assert!(c.check().unwrap_err().contains("twice"));
        c.presets.pop();
        c.check().unwrap();
        let w = c.preset_weather_text(&c.presets[0]);
        assert!(w.contains("name = \"hot\"\ntemperature = \"80 °C\""), "{w}");
    }
}
