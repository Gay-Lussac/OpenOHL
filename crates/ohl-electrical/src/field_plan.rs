// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A field plan template (`.ohlfp`, format `ohl-field-plan` version 1;
//! owner 2026-11-02: "a preset template for the export plans (where we
//! can put a logo, choose the point class and dxf or tin we want to see
//! in the cut and define the limits and colours of the lines. As well as
//! the weather case of each circuit and earth wire used)"): how the
//! fields' cuts are drawn and in which weather the wires hang. A project
//! library kind (`field-plans`); the host reads it, embeds its logos and
//! hands it to a field-presets plugin's report as JSON
//! ([`FieldPlan::plugin_json`]), gathers the points of its classes and
//! the TIN or DXF section along each cut, adds its levels to the run's
//! and solves each electrical system in its weather case.
//!
//! ```toml
//! format = "ohl-field-plan"
//! version = 1
//! name = "Field plans"
//!
//! [title]
//! lines = ["Company", "{line}", "Cut {cut}: {field}"]
//! [[title.logo]]
//! file = "/path/to/logo.svg"   # SVG, PNG or JPEG on this computer
//! width = "40 mm"
//!
//! [cut]
//! classes = [{ class = 6, colour = "#8a8a8a" }]
//! band = "1 m"                 # points within this of the cut's plane
//! background = "tin"           # none, tin or dxf
//! surface = ""                 # tin: the project's terrain surface
//! drawing = ""                 # dxf: the project's background drawing
//! layers = []                  # dxf: only these layers (all when empty)
//!
//! [magnetic]
//! limit_ut = 1.0
//! [[magnetic.level]]
//! value = 1.0
//! colour = "#c53030"
//! width = 2.0
//! label = "1 µT"
//!
//! [electric]
//! limit_kv_m = 5.0
//!
//! [wires]
//! phase = "#9b2c2c"
//! earth = "#2f855a"
//!
//! [weather]
//! case = "still air 40 °C"     # every wire, unless given below
//! earth_wires = ""             # the earth wires' case
//! systems = { "220 kV North" = "…" }   # per electrical system (its id)
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

/// The format's name.
pub const FORMAT: &str = "ohl-field-plan";

/// The format's version.
pub const VERSION: u32 = 1;

/// The files' extension.
pub const EXTENSION: &str = "ohlfp";

/// The project library kind.
pub const KIND: &str = "field-plans";

/// A new plan's text: every key with its default. The limits are those
/// of the Swiss NISV (owner 2026-11-02: "Limits for the NISV are
/// 1microttsla and 5kv per meter"): give your own ordinance's.
pub const TEMPLATE: &str = r##"format = "ohl-field-plan"
version = 1
name = "Field plans"

# The title over each cut: lines of text ({line}, {cut}, {field}, {case}
# are replaced) and the logos beside them (SVG, PNG or JPEG files on this
# computer, embedded when the plans are made; width on the drawing).
[title]
lines = ["{line}", "Weather: {case}"]
logo = []

# What the cut shows besides the field: the point cloud's points of these
# classes within `band` of the cut's plane, and a TIN surface of the
# project (`surface`, empty: the field's ground) or a background drawing
# (`drawing`, its 3D lines where they cross the cut; `layers`: only those)
# — "none", "tin" or "dxf".
[cut]
classes = [{ class = 6, colour = "#8a8a8a" }]
band = "1 m"
background = "none"
surface = ""
drawing = ""
layers = []

# The limits (NISV, Switzerland: 1 µT, 5 kV/m — give your ordinance's)
# and the isolines drawn, each its colour, width (px) and label; added to
# the levels the run computes.
[magnetic]
limit_ut = 1.0
level = [
  { value = 1.0, colour = "#c53030", width = 2.5, label = "1 µT" },
  { value = 0.4, colour = "#c05621", width = 1.5, label = "0.4 µT" },
]

[electric]
limit_kv_m = 5.0
level = [
  { value = 5.0, colour = "#c53030", width = 2.5, label = "5 kV/m" },
  { value = 1.0, colour = "#2b6cb0", width = 1.5, label = "1 kV/m" },
]

# The wires' marks in the cut.
[wires]
phase = "#9b2c2c"
earth = "#2f855a"

# The weather case the wires hang in: `case` for every wire, unless an
# electrical system (by its id) or the earth wires have their own.
[weather]
case = ""
earth_wires = ""
systems = {}
"##;

/// A plan template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldPlan {
    /// `ohl-field-plan`.
    pub format: String,
    /// 1.
    pub version: u32,
    /// Its name.
    #[serde(default)]
    pub name: String,
    /// The title over each cut.
    #[serde(default)]
    pub title: Title,
    /// What the cut shows besides the field.
    #[serde(default)]
    pub cut: CutContent,
    /// The magnetic field's limit and isolines.
    #[serde(default)]
    pub magnetic: FieldStyle,
    /// The electric field's limit and isolines.
    #[serde(default)]
    pub electric: FieldStyle,
    /// The wires' marks.
    #[serde(default)]
    pub wires: WireColours,
    /// The weather cases the wires hang in.
    #[serde(default)]
    pub weather: Weather,
}

/// The title over each cut.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Title {
    /// Lines of text; `{line}`, `{cut}`, `{field}`, `{case}` replaced.
    #[serde(default)]
    pub lines: Vec<String>,
    /// The logos beside them.
    #[serde(default)]
    pub logo: Vec<LogoFile>,
}

/// A logo: an image file on this computer and its width on the drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogoFile {
    /// SVG, PNG or JPEG.
    pub file: String,
    /// Width on the drawing, m (default 40 mm).
    #[serde(
        default = "default_logo_width",
        deserialize_with = "ohl_core::units::si::de::<_, ohl_core::units::Length>"
    )]
    pub width: f64,
}

fn default_logo_width() -> f64 {
    0.04
}

/// What the cut shows besides the field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutContent {
    /// The point cloud's classes drawn, each its colour.
    #[serde(default)]
    pub classes: Vec<ClassColour>,
    /// Points within this of the cut's plane, m (default 1 m).
    #[serde(
        default = "default_band",
        deserialize_with = "ohl_core::units::si::de::<_, ohl_core::units::Length>"
    )]
    pub band: f64,
    /// A surface drawn along the cut.
    #[serde(default)]
    pub background: Background,
    /// `tin`: the project's terrain surface (empty: the field's ground).
    #[serde(default)]
    pub surface: String,
    /// `dxf`: the project's background drawing.
    #[serde(default)]
    pub drawing: String,
    /// `dxf`: only these layers (all when empty).
    #[serde(default)]
    pub layers: Vec<String>,
}

impl Default for CutContent {
    fn default() -> Self {
        Self {
            classes: Vec::new(),
            band: default_band(),
            background: Background::None,
            surface: String::new(),
            drawing: String::new(),
            layers: Vec::new(),
        }
    }
}

fn default_band() -> f64 {
    1.0
}

/// A point cloud class and its colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassColour {
    /// The LiDAR class.
    pub class: u8,
    /// `#rrggbb`.
    pub colour: String,
}

/// The surface drawn along a cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Background {
    /// None.
    #[default]
    None,
    /// A terrain surface of the project (a TIN).
    Tin,
    /// A background drawing's 3D lines where they cross the cut.
    Dxf,
}

/// A field's limit and isolines.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldStyle {
    /// The limit, µT (magnetic; none: no margin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_ut: Option<f64>,
    /// The limit, kV/m (electric; none: no margin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_kv_m: Option<f64>,
    /// The isolines drawn.
    #[serde(default)]
    pub level: Vec<Level>,
}

impl FieldStyle {
    /// The limit, whichever field.
    pub fn limit(&self) -> Option<f64> {
        self.limit_ut.or(self.limit_kv_m)
    }

    /// The levels' values.
    pub fn values(&self) -> Vec<f64> {
        self.level.iter().map(|l| l.value).collect()
    }
}

/// An isoline drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    /// Its value (µT or kV/m).
    pub value: f64,
    /// `#rrggbb`.
    pub colour: String,
    /// Its width, px (default 1.6).
    #[serde(default = "default_width")]
    pub width: f64,
    /// Its label (empty: the value and unit).
    #[serde(default)]
    pub label: String,
}

fn default_width() -> f64 {
    1.6
}

/// The wires' marks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireColours {
    /// Phase conductors, `#rrggbb`.
    #[serde(default = "default_phase")]
    pub phase: String,
    /// Earth wires, `#rrggbb`.
    #[serde(default = "default_earth")]
    pub earth: String,
}

impl Default for WireColours {
    fn default() -> Self {
        Self {
            phase: default_phase(),
            earth: default_earth(),
        }
    }
}

fn default_phase() -> String {
    "#9b2c2c".into()
}

fn default_earth() -> String {
    "#2f855a".into()
}

/// The weather cases the wires hang in, by name.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weather {
    /// Every wire's, unless given below (empty: the run's).
    #[serde(default)]
    pub case: String,
    /// The earth wires' (empty: `case`).
    #[serde(default)]
    pub earth_wires: String,
    /// Per electrical system (its id).
    #[serde(default)]
    pub systems: BTreeMap<String, String>,
}

impl Weather {
    /// The case of electrical system `id` (`None`: the earth wires), by
    /// name; none: the run's.
    pub fn case_of(&self, id: Option<&str>) -> Option<&str> {
        let own = match id {
            Some(id) => self.systems.get(id).map(String::as_str),
            None => Some(self.earth_wires.as_str()),
        };
        own.filter(|c| !c.is_empty())
            .or(Some(self.case.as_str()).filter(|c| !c.is_empty()))
    }

    /// Every case named, each once.
    pub fn cases(&self) -> Vec<String> {
        let mut out: Vec<String> = std::iter::once(&self.case)
            .chain(std::iter::once(&self.earth_wires))
            .chain(self.systems.values())
            .filter(|c| !c.is_empty())
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

fn colour_ok(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit())
}

impl FieldPlan {
    /// The plan of `text` (TOML), checked: the format and version, the
    /// colours `#rrggbb`, the levels, widths, limits and band above 0. The
    /// error names the key.
    pub fn parse(text: &str) -> Result<Self, String> {
        let p: Self = toml::from_str(text).map_err(|e| format!("field plan: {e}"))?;
        if p.format != FORMAT {
            return Err(format!("field plan: format \"{}\", not {FORMAT}", p.format));
        }
        if p.version != VERSION {
            return Err(format!(
                "field plan: version {}, this program reads {VERSION}",
                p.version
            ));
        }
        p.check()?;
        Ok(p)
    }

    fn check(&self) -> Result<(), String> {
        let positive = |v: f64, what: &str| {
            if v.is_finite() && v > 0.0 {
                Ok(())
            } else {
                Err(format!("field plan: {what} {v}: above 0"))
            }
        };
        let colour = |c: &str, what: &str| {
            if colour_ok(c) {
                Ok(())
            } else {
                Err(format!("field plan: {what} \"{c}\": a colour #rrggbb"))
            }
        };
        for l in &self.title.logo {
            positive(l.width, "title.logo width")?;
            if l.file.trim().is_empty() {
                return Err("field plan: title.logo without a file".into());
            }
        }
        positive(self.cut.band, "cut.band")?;
        for c in &self.cut.classes {
            colour(&c.colour, &format!("cut.classes class {}", c.class))?;
        }
        if self.cut.background == Background::Dxf && self.cut.drawing.trim().is_empty() {
            return Err("field plan: cut.background dxf without cut.drawing".into());
        }
        for (name, f) in [("magnetic", &self.magnetic), ("electric", &self.electric)] {
            match (name, f.limit_ut, f.limit_kv_m) {
                ("magnetic", _, Some(_)) => {
                    return Err(
                        "field plan: magnetic.limit_kv_m: the magnetic field's limit is limit_ut"
                            .into(),
                    );
                }
                ("electric", Some(_), _) => {
                    return Err(
                        "field plan: electric.limit_ut: the electric field's limit is limit_kv_m"
                            .into(),
                    );
                }
                _ => {}
            }
            if let Some(l) = f.limit() {
                positive(l, &format!("{name} limit"))?;
            }
            for l in &f.level {
                positive(l.value, &format!("{name}.level value"))?;
                positive(l.width, &format!("{name}.level width"))?;
                colour(&l.colour, &format!("{name}.level {}", l.value))?;
            }
        }
        colour(&self.wires.phase, "wires.phase")?;
        colour(&self.wires.earth, "wires.earth")?;
        Ok(())
    }

    /// The plan as a plugin receives it (JSON, format `ohl-field-plan`
    /// 1): as the file, the logos' files replaced by `logos` (each a
    /// data URI and its width in m, in the order of `title.logo`).
    pub fn plugin_json(&self, logos: &[(String, f64)]) -> String {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(t) = v.get_mut("title").and_then(|t| t.as_object_mut()) {
            t.remove("logo");
            t.insert(
                "logos".into(),
                logos
                    .iter()
                    .map(|(href, width)| json!({"href": href, "width": width}))
                    .collect(),
            );
        }
        v.to_string()
    }
}

/// A data URI of an image file's `bytes` by its file name's extension
/// (SVG, PNG or JPEG); other kinds are an error naming the file.
pub fn data_uri(name: &str, bytes: &[u8]) -> Result<String, String> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let media = match ext.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return Err(format!("logo {name}: an SVG, PNG or JPEG file")),
    };
    Ok(format!("data:{media};base64,{}", base64(bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const B: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                out.push(B[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The template reads, with the NISV limits and its levels; the
    /// weather falls back from a system to `case`.
    #[test]
    fn template_reads() {
        let p = FieldPlan::parse(TEMPLATE).unwrap();
        assert_eq!(p.magnetic.limit_ut, Some(1.0));
        assert_eq!(p.electric.limit_kv_m, Some(5.0));
        assert_eq!(p.magnetic.values(), vec![1.0, 0.4]);
        assert!((p.cut.band - 1.0).abs() < 1e-12);
        assert_eq!(p.cut.background, Background::None);
        let mut w = p.weather.clone();
        assert_eq!(w.case_of(Some("A")), None);
        w.case = "hot".into();
        w.systems.insert("A".into(), "cold".into());
        assert_eq!(w.case_of(Some("A")), Some("cold"));
        assert_eq!(w.case_of(Some("B")), Some("hot"));
        assert_eq!(w.case_of(None), Some("hot"));
        assert_eq!(w.cases(), vec!["cold".to_owned(), "hot".to_owned()]);
    }

    /// Wrong colours, values, a DXF without a drawing and unknown keys
    /// are refused, naming the key.
    #[test]
    fn bad_plans_are_refused() {
        let with = |from: &str, to: &str| {
            assert_eq!(TEMPLATE.matches(from).count(), 1, "{from}");
            FieldPlan::parse(&TEMPLATE.replace(from, to)).unwrap_err()
        };
        assert!(with("\"#8a8a8a\"", "\"grey\"").contains("cut.classes"));
        assert!(with("band = \"1 m\"", "band = \"0 m\"").contains("cut.band"));
        assert!(with("background = \"none\"", "background = \"dxf\"").contains("cut.drawing"));
        assert!(with("limit_ut = 1.0", "limit_ut = -1.0").contains("magnetic limit"));
        assert!(with("phase = \"#9b2c2c\"", "phase = \"red\"").contains("wires.phase"));
        assert!(with("name = \"Field plans\"", "nme = \"x\"").contains("unknown field"));
        assert!(with("version = 1", "version = 2").contains("version 2"));
    }

    /// The plugin's JSON carries the logos as data URIs in place of the
    /// files.
    #[test]
    fn plugin_json_embeds_logos() {
        let mut p = FieldPlan::parse(TEMPLATE).unwrap();
        p.title.logo.push(LogoFile {
            file: "/x/logo.svg".into(),
            width: 0.03,
        });
        let uri = data_uri("logo.svg", b"<svg/>").unwrap();
        assert_eq!(uri, "data:image/svg+xml;base64,PHN2Zy8+");
        let v: serde_json::Value =
            serde_json::from_str(&p.plugin_json(&[(uri.clone(), 0.03)])).unwrap();
        assert_eq!(v["format"], FORMAT);
        assert_eq!(v["title"]["logos"][0]["href"], uri);
        assert!(v["title"].get("logo").is_none());
        assert_eq!(v["magnetic"]["limit_ut"], 1.0);
        assert!(data_uri("logo.gif", b"").is_err());
    }
}
