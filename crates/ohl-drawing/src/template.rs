// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Sheet templates (`.ohlsheet`, format `ohl-sheet-template`, version 1):
//! the page, frame, panels, pens, band rows, title block, revision table,
//! notes, legend, key map, scale bars, north arrows, the styles of the
//! line's kinds and versions, and the values a user fills in — everything
//! a company or an authority sets about plan and profile sheets. TOML,
//! lengths with their units (`"10 mm"` on paper, `"100 m"` in the
//! field), unknown keys refused.
//!
//! The built-in template [`SheetTemplate::builtin`] draws the sheets as
//! they were before templates (a title block across the bottom, neutral
//! colours); `libraries/sheet-templates/default.ohlsheet` is the same as a
//! file. Every key with its meaning: the user guide's "Sheet templates"
//! page and `examples/sheet-templates/example-a1.ohlsheet`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// File extension of sheet templates.
pub const EXTENSION: &str = "ohlsheet";
/// Format identifier.
pub const FORMAT: &str = "ohl-sheet-template";
/// Newest format version read.
pub const VERSION: u32 = 1;

/// The built-in template's text (the same as
/// `libraries/sheet-templates/default.ohlsheet`).
pub const BUILTIN: &str = include_str!("default.ohlsheet");

/// A length on paper, mm: written `"10 mm"` or `"1 cm"` (a bare number is
/// mm).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Mm(pub f64);

/// A length in the field, m: written `"100 m"` or `"1 km"` (a bare number
/// is m).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Metres(pub f64);

/// A number with a unit from `text`: the units and their factors.
fn with_unit(text: &str, units: &[(&str, f64)], default: f64) -> Result<f64, String> {
    let t = text.trim();
    let (number, factor) = units
        .iter()
        .find_map(|(u, k)| t.strip_suffix(u).map(|n| (n.trim(), *k)))
        .unwrap_or((t, default));
    let v: f64 = number
        .parse()
        .map_err(|_| format!("\"{text}\" is not a length"))?;
    if v.is_finite() {
        Ok(v * factor)
    } else {
        Err(format!("\"{text}\" is not a length"))
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NumberOrText {
    Number(f64),
    Text(String),
}

impl<'de> Deserialize<'de> for Mm {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match NumberOrText::deserialize(d)? {
            NumberOrText::Number(v) => Ok(Self(v)),
            NumberOrText::Text(t) => with_unit(&t, &[("mm", 1.0), ("cm", 10.0)], 1.0)
                .map(Self)
                .map_err(serde::de::Error::custom),
        }
    }
}

impl<'de> Deserialize<'de> for Metres {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match NumberOrText::deserialize(d)? {
            NumberOrText::Number(v) => Ok(Self(v)),
            NumberOrText::Text(t) => with_unit(&t, &[("km", 1000.0), ("m", 1.0)], 1.0)
                .map(Self)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// A size on paper that may follow from what is drawn: `"auto"`, or a
/// length.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Size {
    /// From what is drawn.
    #[default]
    Auto,
    /// This many mm.
    Fixed(f64),
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match NumberOrText::deserialize(d)? {
            NumberOrText::Number(v) => Ok(Self::Fixed(v)),
            NumberOrText::Text(t) if t.trim() == "auto" || t.trim() == "fill" => Ok(Self::Auto),
            NumberOrText::Text(t) => with_unit(&t, &[("mm", 1.0), ("cm", 10.0)], 1.0)
                .map(Self::Fixed)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// A column of the title block: a width, `"*"` (the room left, shared by
/// every such column) or `"logos"` (the logos given with the export, side
/// by side).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Column {
    /// This many mm.
    Fixed(f64),
    /// What is left.
    Rest,
    /// As wide as the export's logos.
    Logos,
}

impl<'de> Deserialize<'de> for Column {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match NumberOrText::deserialize(d)? {
            NumberOrText::Number(v) => Ok(Self::Fixed(v)),
            NumberOrText::Text(t) if t.trim() == "*" => Ok(Self::Rest),
            NumberOrText::Text(t) if t.trim() == "logos" => Ok(Self::Logos),
            NumberOrText::Text(t) => with_unit(&t, &[("mm", 1.0), ("cm", 10.0)], 1.0)
                .map(Self::Fixed)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// A pen: width on paper, colour, dash pattern (on, off, … mm). Every
/// part may be left out (the drawing's own).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PenSpec {
    /// Width.
    pub width: Option<Mm>,
    /// Colour `#rrggbb`.
    #[serde(alias = "color")]
    pub colour: Option<String>,
    /// Dash pattern, e.g. `"3 1.5"`; `""`: solid.
    pub dash: Option<String>,
}

impl PenSpec {
    /// `pen` with what this one gives.
    pub fn over(&self, mut pen: crate::model::Pen) -> crate::model::Pen {
        if let Some(w) = self.width {
            pen.width = w.0;
        }
        if let Some(c) = self.colour.as_deref().and_then(crate::model::Color::parse) {
            pen.color = c;
        }
        if let Some(d) = &self.dash {
            pen = pen.dashed(d);
        }
        pen
    }

    fn check(&self, what: &str) -> Result<(), String> {
        if let Some(c) = &self.colour
            && crate::model::Color::parse(c).is_none()
        {
            return Err(format!("{what}: colour \"{c}\" is not #rrggbb"));
        }
        if self.width.is_some_and(|w| !(w.0 >= 0.0 && w.0 <= 10.0)) {
            return Err(format!("{what}: a pen width from 0 to 10 mm"));
        }
        Ok(())
    }
}

/// Page margins.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Margins {
    /// Left (the binding edge, often wider).
    pub left: Mm,
    /// Top.
    pub top: Mm,
    /// Right.
    pub right: Mm,
    /// Bottom.
    pub bottom: Mm,
}

impl Default for Margins {
    fn default() -> Self {
        Self {
            left: Mm(5.0),
            top: Mm(5.0),
            right: Mm(5.0),
            bottom: Mm(5.0),
        }
    }
}

/// `[page]`: the sheet's format.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSpec {
    /// A standard paper size ([`crate::paper::PAPER_SIZES`]: `A1`,
    /// `A3x3`, `B2`, `ANSI D`, `ARCH E` …), `WIDTHxHEIGHT` in mm, or
    /// `"optimised"`: the size and orientation chosen per route from
    /// `sizes` ([`crate::paper`]). None: the export's format.
    pub format: Option<String>,
    /// `"landscape"` (default) or `"portrait"`; with `"optimised"` the
    /// one orientation allowed (none: both).
    pub orientation: Option<String>,
    /// With `"optimised"`: the sizes it may choose from (default A0–A4).
    pub sizes: Option<Vec<String>>,
    /// With `"optimised"`: different sizes of one standard on the sheets
    /// of a route (default no).
    pub mixed: Option<bool>,
    /// A width of its own (with `height`, instead of a format): a long
    /// format, e.g. 1189 × 420 mm.
    pub width: Option<Mm>,
    /// Its height.
    pub height: Option<Mm>,
    /// Margins between the paper's edge and the frame (default 5 mm).
    #[serde(default)]
    pub margin: Margins,
}

/// A line of the frame, `inset` inside the margins.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Border {
    /// From the margins inwards.
    #[serde(default)]
    pub inset: Mm,
    /// Line width.
    pub width: Mm,
}

/// `[frame]`: the lines around the sheet and the room inside them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameSpec {
    /// Its lines (default one of 0.5 mm on the margins).
    #[serde(default = "one_border")]
    pub border: Vec<Border>,
    /// Room between the frame and what is drawn inside it.
    #[serde(default = "five")]
    pub padding: Mm,
}

fn one_border() -> Vec<Border> {
    vec![Border {
        inset: Mm(0.0),
        width: Mm(0.5),
    }]
}

fn five() -> Mm {
    Mm(5.0)
}

impl Default for FrameSpec {
    fn default() -> Self {
        Self {
            border: one_border(),
            padding: five(),
        }
    }
}

/// `[text]`: the labels' font, size and colour.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSpec {
    /// Font: `"Helvetica"` (the PDF standard font; SVG and PNG take a
    /// sans-serif font of the computer).
    #[serde(default = "helvetica")]
    pub font: String,
    /// Size of the drawing's labels.
    #[serde(default = "label_size")]
    pub size: Mm,
    /// Colour of the texts of the sheet (title block, notes, labels).
    #[serde(default = "black", alias = "color")]
    pub colour: String,
}

fn helvetica() -> String {
    "Helvetica".into()
}

fn label_size() -> Mm {
    Mm(2.5)
}

fn black() -> String {
    "#000000".into()
}

impl Default for TextSpec {
    fn default() -> Self {
        Self {
            font: helvetica(),
            size: label_size(),
            colour: black(),
        }
    }
}

/// What lies beyond a sheet's match line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Beyond {
    /// Nothing (the default).
    #[default]
    None,
    /// The next sheet's content, faded (30 % of its colour), to the
    /// paper's edge.
    Faded,
}

impl<'de> Deserialize<'de> for Beyond {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "none" => Ok(Self::None),
            "faded" => Ok(Self::Faded),
            other => Err(serde::de::Error::custom(format!(
                "beyond_match = \"{other}\": give \"none\" or \"faded\""
            ))),
        }
    }
}

/// `[layout]`: the panels.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutSpec {
    /// Top to bottom: `"plan"`, `"profile"`, `"bands"` (the bands right
    /// under the profile).
    #[serde(default = "default_order")]
    pub order: Vec<String>,
    /// Between the panels.
    #[serde(default = "eight")]
    pub gap: Mm,
    /// The plan panel's height: `"auto"` (the line width, the corridor
    /// and the elevations at the plan's scale across the line) or a
    /// length (at least that).
    #[serde(default)]
    pub plan_height: Size,
    /// Content beyond the match line: `"none"` or `"faded"`.
    #[serde(default)]
    pub beyond_match: Beyond,
    /// Room left of the panels for the height labels and the bands'
    /// labels.
    #[serde(default = "fourteen")]
    pub axis_room: Mm,
    /// Station labels under the profile (or the bands).
    #[serde(default = "yes")]
    pub station_labels: bool,
    /// The plan panel's title (default `"Plan"`; `""`: none).
    #[serde(default = "plan_title")]
    pub plan_title: String,
    /// The profile panel's title (default `"Profile"`; `""`: none).
    #[serde(default = "profile_title")]
    pub profile_title: String,
}

fn plan_title() -> String {
    "Plan".into()
}

fn profile_title() -> String {
    "Profile".into()
}

fn default_order() -> Vec<String> {
    vec!["plan".into(), "profile".into(), "bands".into()]
}

fn eight() -> Mm {
    Mm(8.0)
}

fn fourteen() -> Mm {
    Mm(14.0)
}

fn yes() -> bool {
    true
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            order: default_order(),
            gap: eight(),
            plan_height: Size::Auto,
            beyond_match: Beyond::None,
            axis_room: fourteen(),
            station_labels: true,
            plan_title: plan_title(),
            profile_title: profile_title(),
        }
    }
}

impl LayoutSpec {
    /// Whether the plan is drawn below the profile.
    pub fn plan_below(&self) -> bool {
        let at = |k: &str| self.order.iter().position(|o| o == k);
        matches!((at("plan"), at("profile")), (Some(a), Some(b)) if a > b)
    }
}

/// What a band row under the profile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandKind {
    /// Structure names and types.
    Structures,
    /// Stations (km+m) at the structures.
    Stations,
    /// Ground height at the structures and every `every` m.
    GroundHeight,
    /// Span lengths, between the structures.
    SpanLength,
    /// Structure heights (the highest point above the ground).
    StructureHeight,
    /// The line's angle at the alignment points, degrees.
    LineAngle,
    /// The lowest wire's height above sea level in each span (the wires
    /// drawn).
    LowestWireHeight,
    /// The ground clearance found by a clearance check (none given:
    /// empty).
    GroundClearance,
    /// A value the user gives per sheet (`field`).
    Text,
    /// The stringing tension of each tension section (between two strain
    /// structures): each circuit's conductors and the earth wires, kN
    /// per subconductor, in the stringing weather case.
    Stringing,
}

impl BandKind {
    /// Every kind, as written.
    pub const ALL: [(Self, &'static str); 10] = [
        (Self::Structures, "structures"),
        (Self::Stations, "stations"),
        (Self::GroundHeight, "ground_height"),
        (Self::SpanLength, "span_length"),
        (Self::StructureHeight, "structure_height"),
        (Self::LineAngle, "line_angle"),
        (Self::LowestWireHeight, "lowest_wire_height"),
        (Self::GroundClearance, "ground_clearance"),
        (Self::Text, "text"),
        (Self::Stringing, "stringing"),
    ];
}

impl<'de> Deserialize<'de> for BandKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let t = String::deserialize(d)?;
        Self::ALL
            .iter()
            .find(|(_, n)| *n == t)
            .map(|(k, _)| *k)
            .ok_or_else(|| {
                let names: Vec<&str> = Self::ALL.iter().map(|(_, n)| *n).collect();
                serde::de::Error::custom(format!(
                    "band kind \"{t}\": give one of {}",
                    names.join(", ")
                ))
            })
    }
}

/// `[[band]]`: a row under the profile.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandSpec {
    /// What it shows.
    pub kind: BandKind,
    /// Its height.
    #[serde(default = "eight")]
    pub height: Mm,
    /// Its name, left of it.
    #[serde(default)]
    pub label: String,
    /// Values also every this many metres (ground heights).
    pub every: Option<Metres>,
    /// The user field it shows (`kind = "text"`).
    pub field: Option<String>,
    /// Decimals of its numbers (default 1).
    pub decimals: Option<usize>,
    /// What a structure's entry says (`kind = "structures"`): `{name}`,
    /// `{type}` (its type and extension), `{type_id}`, `{extension}`,
    /// `{station}`, `{height}`; a line break makes two lines (default
    /// `"{name} {type}"`).
    pub format: Option<String>,
}

/// The placeholders of a structures band row's `format`.
pub const STRUCTURE_BAND_FIELDS: [&str; 6] =
    ["name", "type", "type_id", "extension", "station", "height"];

/// Where a north arrow goes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NorthArrowSpec {
    /// `"plan"`: one per straight stretch at the plan's top edge;
    /// `"off"` (default).
    #[serde(default = "off")]
    pub place: String,
    /// Its length.
    #[serde(default = "eight")]
    pub size: Mm,
}

fn off() -> String {
    "off".into()
}

impl Default for NorthArrowSpec {
    fn default() -> Self {
        Self {
            place: off(),
            size: eight(),
        }
    }
}

/// `[scale_bar]`: graphic scales.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaleBarSpec {
    /// `"bottom"` (in the bottom row, beside the title block), `"plan"`,
    /// `"profile"` (in the panel's lower left corner) or `"off"`
    /// (default).
    #[serde(default = "off")]
    pub place: String,
    /// The lengths shown: along the line (horizontal scale) and, for the
    /// profile, up (vertical scale); none: chosen from the scales.
    #[serde(default)]
    pub lengths: Vec<Metres>,
}

impl Default for ScaleBarSpec {
    fn default() -> Self {
        Self {
            place: off(),
            lengths: Vec::new(),
        }
    }
}

/// `[legend]`: what every line type, colour and symbol drawn means,
/// made from what each sheet draws.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegendSpec {
    /// `"above_title_block"`, `"bottom"` or `"off"` (default).
    #[serde(default = "off")]
    pub place: String,
    /// Its title.
    #[serde(default = "legend_title")]
    pub title: String,
    /// Its width (default: the title block's, or 120 mm).
    pub width: Option<Mm>,
    /// Its height (the entries flow in columns; those that do not fit are
    /// warned about).
    #[serde(default = "thirty")]
    pub height: Mm,
    /// Width of a column of entries.
    #[serde(default = "column_width")]
    pub column_width: Mm,
    /// Size of its texts.
    #[serde(default = "small")]
    pub size: Mm,
    /// Its own text for each entry the export makes, by kind
    /// ([`LEGEND_LABELS`]: `axis`, `weather_case` …), with that kind's
    /// placeholders; a kind not given keeps the English default.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

/// The entries the export puts in the legend: kind, English default,
/// placeholders.
pub const LEGEND_LABELS: [(&str, &str, &[&str]); 24] = [
    ("axis", "axis of the line", &[]),
    (
        "corridor_near",
        "corridor edge, near side ({side}) ±{width} m",
        &["side", "width"],
    ),
    (
        "corridor_far",
        "corridor edge, far side ({side}) ±{width} m",
        &["side", "width"],
    ),
    ("weather_case", "weather case {case}", &["case"]),
    ("structure", "structure", &[]),
    ("structure_role", "structure ({role})", &["role"]),
    ("clearance_violation", "clearance violation", &[]),
    ("datum_step", "datum step (heights read on each side)", &[]),
    ("ground", "ground on the axis", &[]),
    ("ground_side", "ground beside the axis", &["offset"]),
    (
        "clearance_line",
        "ground + {distance} m (required clearance)",
        &["distance"],
    ),
    (
        "conductor_clearance",
        "lowest conductor of {circuit} ({voltage}) – {distance} m, rule \"{rule}\", {case}{note}",
        &["circuit", "voltage", "distance", "rule", "case", "note"],
    ),
    (
        "conductor_clearance_note",
        " — the rule names no weather case: the case drawn with the largest sag, {case}",
        &["case"],
    ),
    ("match_line", "match line (sheet's end)", &[]),
    ("isoclearance", "isoclearance {level}", &["level"]),
    ("lidar", "{name} (LiDAR class {class})", &["name", "class"]),
    ("version", "{label}: {words}", &["label", "words"]),
    ("background_drawing", "drawing {name}", &["name"]),
    ("orthophoto", "orthophoto {name}", &["name"]),
    // The elevations' words (written at each structure, not in the
    // legend).
    ("elevation_string", "string {length} m", &["length"]),
    ("elevation_arm", "{height} m", &["height"]),
    ("elevation_dimension", "{length} m", &["length"]),
    // The stringing band's words: a circuit's tension, the earth wires'
    // name.
    (
        "stringing",
        "{circuit}: {tension} kN",
        &["circuit", "tension"],
    ),
    ("earth_wires", "earth wires", &[]),
];

impl LegendSpec {
    /// The legend's text for an entry of `kind` with `values` filled in:
    /// the template's (`labels`), else the English default.
    pub fn label(&self, kind: &str, values: &[(&str, &str)]) -> String {
        let text = self.labels.get(kind).map(String::as_str).or_else(|| {
            LEGEND_LABELS
                .iter()
                .find(|(k, ..)| *k == kind)
                .map(|(_, d, _)| *d)
        });
        fill(text.unwrap_or(""), &|n, _| {
            values
                .iter()
                .find(|(k, _)| *k == n)
                .map(|(_, v)| (*v).to_owned())
        })
    }
}

fn legend_title() -> String {
    "Legend".into()
}

fn thirty() -> Mm {
    Mm(30.0)
}

fn column_width() -> Mm {
    Mm(60.0)
}

fn small() -> Mm {
    Mm(2.0)
}

impl Default for LegendSpec {
    fn default() -> Self {
        Self {
            place: off(),
            title: legend_title(),
            width: None,
            height: thirty(),
            column_width: column_width(),
            size: small(),
            labels: BTreeMap::new(),
        }
    }
}

/// `[key_map]`: a small plan of the whole route (or a window around the
/// sheet), the sheet's stretch marked.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyMapSpec {
    /// `"bottom"` (in the bottom row) or `"off"` (default).
    #[serde(default = "off")]
    pub place: String,
    /// Its width.
    #[serde(default = "key_width")]
    pub width: Mm,
    /// Its height.
    #[serde(default = "thirty")]
    pub height: Mm,
    /// Its scale denominator (none: the route, or the window, fitted).
    pub scale: Option<f64>,
    /// `"route"` (default): the whole route; `"sheet"`: a window around
    /// the sheet's stretch, `margin` wider on every side.
    #[serde(default = "route")]
    pub extent: String,
    /// Around the sheet's stretch (`extent = "sheet"`).
    #[serde(default)]
    pub margin: Metres,
    /// The structures' names.
    #[serde(default)]
    pub structures: bool,
    /// A north arrow in its corner.
    #[serde(default = "yes")]
    pub north_arrow: bool,
    /// The corridor's edges.
    #[serde(default = "yes")]
    pub corridor: bool,
    /// The sheet's stretch: its pen.
    #[serde(default)]
    pub highlight: PenSpec,
}

fn key_width() -> Mm {
    Mm(60.0)
}

fn route() -> String {
    "route".into()
}

impl Default for KeyMapSpec {
    fn default() -> Self {
        Self {
            place: off(),
            width: key_width(),
            height: thirty(),
            scale: None,
            extent: route(),
            margin: Metres(0.0),
            structures: false,
            north_arrow: true,
            corridor: true,
            highlight: PenSpec::default(),
        }
    }
}

/// `[corridor]`: the corridor's edges in the plan at ± `width` from the
/// axis, and the ground along them in the profile.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorridorSpec {
    /// Half width, m (none: no corridor unless the export gives one).
    pub width: Option<Metres>,
    /// Its label at each edge; `{width}` is the half width.
    pub label: Option<String>,
    /// Its pen.
    #[serde(default)]
    pub pen: PenSpec,
    /// The side of the line nearest the profile's reader: `"right"`
    /// (default: the right looking along increasing stations, the plan's
    /// lower edge) or `"left"`.
    #[serde(default = "right")]
    pub near_side: String,
    /// The near edge's pen, over `pen` (in the plan, the key map and its
    /// ground line in the profile).
    #[serde(default)]
    pub near: PenSpec,
    /// The far edge's pen, over `pen`.
    #[serde(default)]
    pub far: PenSpec,
    /// The ground along both edges drawn in the profile (default on).
    #[serde(default = "yes")]
    pub profile: bool,
}

fn right() -> String {
    "right".into()
}

impl Default for CorridorSpec {
    fn default() -> Self {
        Self {
            width: None,
            label: None,
            pen: PenSpec::default(),
            near_side: right(),
            near: PenSpec::default(),
            far: PenSpec::default(),
            profile: true,
        }
    }
}

impl CorridorSpec {
    /// The offset of the near edge for a half width `w`, m (right
    /// positive); the far edge's is the opposite.
    pub fn near_offset(&self, w: f64) -> f64 {
        if self.near_side == "left" { -w } else { w }
    }

    /// The pen of the edge at `offset` (its sign), over `base`, and
    /// whether it is the near one.
    pub fn edge_pen(&self, offset: f64, base: crate::model::Pen) -> (crate::model::Pen, bool) {
        let near = (offset >= 0.0) == (self.near_side != "left");
        let spec = if near { &self.near } else { &self.far };
        (spec.over(self.pen.over(base)), near)
    }
}

/// `[centreline]`: the line's axis in the plan and the ground on it in
/// the profile.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CentrelineSpec {
    /// The axis's pen in the plan (default grey, dash-dot).
    #[serde(default)]
    pub pen: PenSpec,
    /// The ground on the axis drawn in the profile (default on; always
    /// solid).
    #[serde(default = "yes")]
    pub ground: bool,
}

impl Default for CentrelineSpec {
    fn default() -> Self {
        Self {
            pen: PenSpec::default(),
            ground: true,
        }
    }
}

/// A LiDAR class's (or group's) style on the sheets.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LidarStyle {
    /// Colour `#rrggbb` (default: the class scheme's, else grey).
    #[serde(alias = "color")]
    pub colour: Option<String>,
    /// Dot diameter (default the `[lidar]` size).
    pub size: Option<Mm>,
    /// What the legend calls it (default: the class scheme's name).
    pub label: Option<String>,
}

/// `[lidar]`: the survey's LiDAR points within the corridor, as dots.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LidarSpec {
    /// In the profile, by station and height (default off).
    #[serde(default)]
    pub profile: bool,
    /// In the plan, seen from above (default off).
    #[serde(default)]
    pub plan: bool,
    /// The classes drawn, by code (the export may choose others).
    #[serde(default)]
    pub classes: Vec<u8>,
    /// Dot diameter.
    #[serde(default = "dot")]
    pub size: Mm,
    /// One dot per class in each square cell of this size on paper (what
    /// is drawn only: nothing computed changes).
    #[serde(default = "dot_cell")]
    pub cell: Mm,
    /// Styles by class code (`"2"`) or group (`"3-5"`, `"3,4,5"`).
    #[serde(default)]
    pub styles: BTreeMap<String, LidarStyle>,
}

fn dot() -> Mm {
    Mm(0.25)
}

fn dot_cell() -> Mm {
    Mm(0.2)
}

impl Default for LidarSpec {
    fn default() -> Self {
        Self {
            profile: false,
            plan: false,
            classes: Vec::new(),
            size: dot(),
            cell: dot_cell(),
            styles: BTreeMap::new(),
        }
    }
}

/// The class codes of a `[lidar.styles]` key: `"2"`, `"3-5"`, `"3,4,5"`.
pub fn class_codes(key: &str) -> Result<Vec<u8>, String> {
    let bad = || format!("lidar.styles.\"{key}\": class codes 0–255, e.g. \"2\", \"3-5\", \"3,4\"");
    let mut out = Vec::new();
    for part in key.split(',') {
        let code = |t: &str| t.trim().parse::<u8>().map_err(|_| bad());
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (code(a)?, code(b)?);
                if a > b {
                    return Err(bad());
                }
                out.extend(a..=b);
            }
            None => out.push(code(part)?),
        }
    }
    Ok(out)
}

impl LidarSpec {
    /// The style of class `code` (the first key naming it).
    pub fn style(&self, code: u8) -> Option<&LidarStyle> {
        self.style_entry(code).map(|(_, s)| s)
    }

    /// The key (`"3-5"`) and style of class `code`.
    pub fn style_entry(&self, code: u8) -> Option<(&str, &LidarStyle)> {
        self.styles
            .iter()
            .find(|(k, _)| class_codes(k).is_ok_and(|c| c.contains(&code)))
            .map(|(k, s)| (k.as_str(), s))
    }
}

/// `[elevations]`: each structure's elevation on the plan.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElevationSpec {
    /// `"above"`, `"below"` or `"off"` (none: the export's).
    pub place: Option<String>,
    /// From the axis to the elevation's base.
    pub offset: Option<Mm>,
    /// Angle between the line and the elevation's height, degrees.
    pub rotation: Option<f64>,
    /// Its own scale denominator (none: the horizontal scale).
    pub scale: Option<f64>,
    /// Labels: `"number"`, `"type"`, `"height"`, `"string_length"`,
    /// `"dimensions"`, `"arm_heights"`.
    pub labels: Option<Vec<String>>,
    /// Size of the labels.
    pub size: Option<Mm>,
    /// The stick figure even where members or a pole outline exist.
    pub stick: Option<bool>,
    /// Its pen.
    #[serde(default)]
    pub pen: PenSpec,
}

/// A line kind's style.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindStyle {
    /// Pen.
    #[serde(flatten)]
    pub pen: PenSpec,
    /// What the legend calls it.
    pub label: Option<String>,
}

/// `[line_styles]`: the wires by kind (high voltage, low voltage,
/// telecom, earth wire) instead of by phase colour, cables dashed.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineStyles {
    /// Style by kind instead of the phase colours.
    #[serde(default)]
    pub by_kind: bool,
    /// Systems up to this voltage are low voltage, kV (none: every system
    /// with a voltage is high voltage).
    pub low_voltage_up_to_kv: Option<f64>,
    /// A section's field (`plugin.column`) naming its kind (`hv`, `lv`,
    /// `telecom`, `earth`), over the voltage.
    pub kind_field: Option<String>,
    /// A section's field naming its construction (`overhead`, `cable`).
    pub construction_field: Option<String>,
    /// Dash pattern of cables.
    pub cable_dash: Option<String>,
    /// Styles: `hv`, `lv`, `telecom`, `earth`.
    #[serde(default)]
    pub kinds: BTreeMap<String, KindStyle>,
}

/// `[roles.<role>]`: how a version of the line in that role is drawn.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSpec {
    /// What the legend calls it.
    pub label: Option<String>,
    /// Pen width of its wires (over the kind's or 0.35 mm).
    pub width: Option<Mm>,
    /// Factor on the pen width of its wires.
    pub weight: Option<f64>,
    /// Colour of its wires and structures (over the kind's or phase's).
    #[serde(alias = "color")]
    pub colour: Option<String>,
    /// Dash pattern of its wires.
    pub dash: Option<String>,
    /// Colour of its structures (default: `colour`, else dark grey).
    #[serde(alias = "structure_color")]
    pub structure_colour: Option<String>,
    /// Its structures' names drawn.
    pub names: Option<bool>,
}

/// `[cases]`: the weather case each kind of wire is drawn in, and the
/// stringing case of the `stringing` band, by default (the export may
/// choose per circuit). Each a weather case's name, or a temperature
/// (`"80 °C"`): the project's case at that temperature without wind or
/// ice ([`CasesSpec::pick`]). None: the case shown.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasesSpec {
    /// The phase conductors' (every circuit's).
    pub conductors: Option<String>,
    /// The earth wires'.
    pub earth_wires: Option<String>,
    /// The stringing tension's (the `stringing` band).
    pub stringing: Option<String>,
}

impl CasesSpec {
    /// Which of `cases` (name, temperature °C, still air without ice)
    /// `choice` names: the case of that name, else, when `choice` is a
    /// temperature, the first still case without ice at it (within
    /// 0.05 °C). None: none matches.
    pub fn pick(choice: &str, cases: &[(String, f64, bool)]) -> Option<usize> {
        let choice = choice.trim();
        if let Some(k) = cases.iter().position(|c| c.0 == choice) {
            return Some(k);
        }
        let t: f64 = choice
            .trim_end_matches("°C")
            .trim_end_matches("C")
            .trim_end_matches('°')
            .trim()
            .parse()
            .ok()?;
        cases.iter().position(|c| c.2 && (c.1 - t).abs() < 0.05)
    }
}

/// `[conductor_clearance]`: lines below the lowest conductor, lowered by
/// the ground clearance the project's rules require for the circuit.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConductorClearanceSpec {
    /// Drawn (default off).
    #[serde(default)]
    pub show: bool,
    /// The circuits drawn, by name (default every circuit with a rule).
    #[serde(default)]
    pub circuits: Vec<String>,
    /// `"also"` (default): as well as the line above the ground;
    /// `"instead"`: instead of it.
    #[serde(default = "also")]
    pub mode: String,
    /// Its label: `{distance}`, `{rule}`, `{circuit}`, `{case}`,
    /// `{voltage}`.
    #[serde(default = "curve_label")]
    pub label: String,
    /// Where its label goes: `"auto"` (default: clear of the structures,
    /// the wires and the ground, nearest the sheet's middle), `"middle"`
    /// (the sheet's middle, above the line, as before), `"start"` or
    /// `"end"` (the free place nearest that end of the sheet).
    #[serde(default = "auto_place")]
    pub label_at: String,
    /// Its pen.
    #[serde(default)]
    pub pen: PenSpec,
}

fn auto_place() -> String {
    "auto".into()
}

fn also() -> String {
    "also".into()
}

fn curve_label() -> String {
    "{distance} m below the lowest conductor ({rule})".into()
}

impl Default for ConductorClearanceSpec {
    fn default() -> Self {
        Self {
            show: false,
            circuits: Vec::new(),
            mode: also(),
            label: curve_label(),
            label_at: auto_place(),
            pen: PenSpec::default(),
        }
    }
}

/// Which backgrounds of the project are drawn: none (`false`, the
/// default), every one (`true`) or those named (a list).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Choice {
    /// None.
    #[default]
    Off,
    /// Every one.
    All,
    /// Those named (never empty: an empty list reads as [`Choice::Off`]).
    Named(Vec<String>),
}

impl<'de> Deserialize<'de> for Choice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            On(bool),
            Names(Vec<String>),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::On(true) => Self::All,
            Raw::On(false) => Self::Off,
            Raw::Names(n) if n.is_empty() => Self::Off,
            Raw::Names(n) => Self::Named(n),
        })
    }
}

impl Choice {
    /// Whether any is drawn.
    pub fn on(&self) -> bool {
        *self != Self::Off
    }

    /// Whether the one named `name` is drawn.
    pub fn takes(&self, name: &str) -> bool {
        match self {
            Self::Off => false,
            Self::All => true,
            Self::Named(n) => n.iter().any(|x| x == name),
        }
    }

    /// The names chosen (none: every one, or none at all).
    pub fn names(&self) -> Option<&[String]> {
        match self {
            Self::Named(n) => Some(n),
            _ => None,
        }
    }

    /// This choice switched on (`true`: the names chosen, else every
    /// one) or off.
    pub fn switched(&self, on: bool) -> Self {
        match (on, self) {
            (false, _) => Self::Off,
            (true, Self::Named(n)) => Self::Named(n.clone()),
            (true, _) => Self::All,
        }
    }
}

/// `[plan_background]`: the project's DXF drawings and orthophotos under
/// the plan, straightened along the line like everything else drawn
/// there. Display only.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanBackgroundSpec {
    /// The project's drawings (DXF): `false` (default), `true` (every
    /// one) or a list of their names.
    #[serde(default)]
    pub drawings: Choice,
    /// The drawings' texts, at their size on the ground, written along
    /// the sheet (default off).
    #[serde(default)]
    pub texts: bool,
    /// The drawings' points, as dots (default off).
    #[serde(default)]
    pub points: bool,
    /// The drawings in their own colours (default off: every line in
    /// `pen`'s colour).
    #[serde(default, alias = "own_colors")]
    pub own_colours: bool,
    /// The drawings' pen (default 0.13 mm, grey `#8c8c8c`; with
    /// `own_colours` its width and dash only).
    #[serde(default)]
    pub pen: PenSpec,
    /// The project's orthophotos: `false` (default), `true` (every image
    /// layer) or a list of image layers.
    #[serde(default)]
    pub orthophoto: Choice,
    /// The white veil over the orthophoto, 0 (none) to 1 (white): the
    /// line stays readable over it (default 0.6).
    #[serde(default = "veil")]
    pub veil: f64,
    /// The orthophoto's resolution on paper, dots per inch (default 200;
    /// 50 to 600).
    #[serde(default = "photo_dpi")]
    pub dpi: f64,
    /// The orthophoto's JPEG quality, 10 to 100 (default 75).
    #[serde(default = "photo_quality")]
    pub quality: u8,
}

fn veil() -> f64 {
    0.6
}

fn photo_dpi() -> f64 {
    200.0
}

fn photo_quality() -> u8 {
    75
}

impl Default for PlanBackgroundSpec {
    fn default() -> Self {
        Self {
            drawings: Choice::Off,
            texts: false,
            points: false,
            own_colours: false,
            pen: PenSpec::default(),
            orthophoto: Choice::Off,
            veil: veil(),
            dpi: photo_dpi(),
            quality: photo_quality(),
        }
    }
}

impl PlanBackgroundSpec {
    fn check(&self) -> Result<(), String> {
        self.pen.check("plan_background.pen")?;
        if !(0.0..=1.0).contains(&self.veil) {
            return Err("plan_background.veil: from 0 (none) to 1 (white)".into());
        }
        if !(50.0..=600.0).contains(&self.dpi) {
            return Err("plan_background.dpi: from 50 to 600 dots per inch".into());
        }
        if !(10..=100).contains(&self.quality) {
            return Err("plan_background.quality: from 10 to 100".into());
        }
        for (what, c) in [
            ("drawings", &self.drawings),
            ("orthophoto", &self.orthophoto),
        ] {
            if c.names()
                .is_some_and(|n| n.iter().any(|x| x.trim().is_empty()))
            {
                return Err(format!("plan_background.{what}: a name may not be empty"));
            }
        }
        Ok(())
    }
}

/// How a cell's content sits across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// Left (default).
    #[default]
    Left,
    /// Centre.
    Centre,
    /// Right.
    Right,
}

impl<'de> Deserialize<'de> for Align {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "left" => Ok(Self::Left),
            "centre" | "center" => Ok(Self::Centre),
            "right" => Ok(Self::Right),
            other => Err(serde::de::Error::custom(format!(
                "align = \"{other}\": give left, centre or right"
            ))),
        }
    }
}

/// `[[title_block.cell]]`: a cell of the title block's grid.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellSpec {
    /// Row and column (from 0).
    pub at: [usize; 2],
    /// Rows and columns it spans.
    #[serde(default = "one_one")]
    pub span: [usize; 2],
    /// Its text (placeholders `{field}`, lines split at `\n`).
    #[serde(default)]
    pub text: String,
    /// Size of its text.
    pub size: Option<Mm>,
    /// Bold.
    #[serde(default)]
    pub bold: bool,
    /// Across the cell.
    #[serde(default)]
    pub align: Align,
    /// An image of the template (`[[image]]` name or a file next to the
    /// template), or `"{logos}"`: the logos given with the export.
    pub image: Option<String>,
}

fn one_one() -> [usize; 2] {
    [1, 1]
}

/// Which corner the title block sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corner {
    /// Bottom right (default).
    #[default]
    BottomRight,
    /// Bottom left.
    BottomLeft,
}

impl<'de> Deserialize<'de> for Corner {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "bottom_right" => Ok(Self::BottomRight),
            "bottom_left" => Ok(Self::BottomLeft),
            other => Err(serde::de::Error::custom(format!(
                "corner = \"{other}\": give bottom_right or bottom_left"
            ))),
        }
    }
}

/// `[title_block]`: a grid of cells in a corner of the frame.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleBlockSpec {
    /// Its corner.
    #[serde(default)]
    pub corner: Corner,
    /// Its width (`"auto"`: across the whole frame).
    #[serde(default)]
    pub width: Size,
    /// Its columns.
    pub columns: Vec<Column>,
    /// Its rows' heights.
    pub rows: Vec<Mm>,
    /// Width of its outline.
    #[serde(default = "outline")]
    pub line: Mm,
    /// Lines between the cells too (else the outline only).
    #[serde(default)]
    pub grid: bool,
    /// Its cells.
    #[serde(default, rename = "cell")]
    pub cells: Vec<CellSpec>,
}

fn outline() -> Mm {
    Mm(0.4)
}

impl Default for TitleBlockSpec {
    fn default() -> Self {
        Self {
            corner: Corner::BottomRight,
            width: Size::Auto,
            columns: vec![Column::Rest],
            rows: vec![Mm(32.0)],
            line: outline(),
            grid: false,
            cells: Vec::new(),
        }
    }
}

/// A column of the revision table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionColumn {
    /// Its heading.
    pub title: String,
    /// Its width.
    pub width: Mm,
    /// What it shows: `{rev.index}`, `{rev.date}`, `{rev.text}`,
    /// `{rev.by}`, `{rev.checked}`.
    pub value: String,
}

/// `[revisions]`: the revision table, the last revisions of the project
/// line's drawing settings.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionSpec {
    /// `"above_title_block"` or `"off"` (default).
    #[serde(default = "off")]
    pub place: String,
    /// How many (the last ones).
    #[serde(default = "four")]
    pub rows: usize,
    /// Height of a row.
    #[serde(default = "five")]
    pub row_height: Mm,
    /// Its columns.
    #[serde(default)]
    pub columns: Vec<RevisionColumn>,
    /// Size of its texts.
    #[serde(default = "small")]
    pub size: Mm,
}

fn four() -> usize {
    4
}

impl Default for RevisionSpec {
    fn default() -> Self {
        Self {
            place: off(),
            rows: four(),
            row_height: five(),
            columns: Vec::new(),
            size: small(),
        }
    }
}

/// A column of the stringing table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringingColumn {
    /// Its heading.
    pub title: String,
    /// Its width.
    pub width: Mm,
    /// What it shows: `{st.from}`, `{st.to}` (the tension section's end
    /// structures, along the route), `{st.circuit}` (the earth wires: the
    /// legend's `earth_wires` word), `{st.tension}` (kN, the lowest–highest
    /// of its spans), `{st.case}` (the stringing weather case), and the
    /// sheet's fields.
    pub value: String,
}

/// `[stringing_table]`: the tension sections on the sheet, one row per
/// section and circuit (the earth wires last), with the stringing tension
/// in the stringing case (ESTI 235, 2.2.2 a).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringingTableSpec {
    /// `"above_title_block"`, `"bottom"` (in the bottom row beside the
    /// title block, after the key map) or `"off"` (default).
    #[serde(default = "off")]
    pub place: String,
    /// Rows under the heading: the room kept on every sheet (more rows
    /// than this: the last says how many are left out, with a warning).
    #[serde(default = "eight_rows")]
    pub rows: usize,
    /// Height of a row.
    #[serde(default = "five")]
    pub row_height: Mm,
    /// Its columns.
    #[serde(default)]
    pub columns: Vec<StringingColumn>,
    /// Size of its texts.
    #[serde(default = "small")]
    pub size: Mm,
}

fn eight_rows() -> usize {
    8
}

impl Default for StringingTableSpec {
    fn default() -> Self {
        Self {
            place: off(),
            rows: eight_rows(),
            row_height: five(),
            columns: Vec::new(),
            size: small(),
        }
    }
}

/// The fields of a stringing table row.
pub const STRINGING_FIELDS: [&str; 5] = ["st.from", "st.to", "st.circuit", "st.tension", "st.case"];

/// `[[note]]`: a free text on the sheet.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteSpec {
    /// Where its first line starts: from the frame's left and top edges
    /// (negative: from the right and bottom ones).
    pub at: [Mm; 2],
    /// Width its lines are broken to.
    #[serde(default = "hundred")]
    pub width: Mm,
    /// Size.
    #[serde(default = "small")]
    pub size: Mm,
    /// The text (placeholders).
    pub text: String,
    /// Bold.
    #[serde(default)]
    pub bold: bool,
}

fn hundred() -> Mm {
    Mm(100.0)
}

/// `[[field]]`: a value the user gives (the export window, `--field`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    /// Its key, used as `{key}`.
    pub key: String,
    /// What the export window calls it.
    #[serde(default)]
    pub label: String,
    /// When none is given.
    #[serde(default)]
    pub default: String,
    /// The export refuses without it.
    #[serde(default)]
    pub required: bool,
    /// One value per sheet.
    #[serde(default)]
    pub per_sheet: bool,
}

/// `[[image]]`: an image the cells name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageSpec {
    /// The name cells use, e.g. `"logo.svg"`.
    pub name: String,
    /// Its file, next to the template.
    pub file: Option<String>,
    /// Or its bytes, base64.
    pub data: Option<String>,
    /// Its type: `image/svg+xml`, `image/png`, `image/jpeg` (default: from
    /// the name).
    pub media_type: Option<String>,
}

/// A sheet template.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetTemplate {
    /// `"ohl-sheet-template"`.
    pub format: Option<String>,
    /// 1.
    pub version: Option<u32>,
    /// Its name.
    #[serde(default)]
    pub name: String,
    /// What it is for.
    #[serde(default)]
    pub description: String,
    /// The page.
    #[serde(default)]
    pub page: PageSpec,
    /// The frame.
    #[serde(default)]
    pub frame: FrameSpec,
    /// Texts.
    #[serde(default)]
    pub text: TextSpec,
    /// The panels.
    #[serde(default)]
    pub layout: LayoutSpec,
    /// Pens by layer name (`OHL_GROUND` …).
    #[serde(default)]
    pub layers: BTreeMap<String, PenSpec>,
    /// Band rows under the profile.
    #[serde(default, rename = "band")]
    pub bands: Vec<BandSpec>,
    /// North arrows.
    #[serde(default)]
    pub north_arrow: NorthArrowSpec,
    /// Graphic scales.
    #[serde(default)]
    pub scale_bar: ScaleBarSpec,
    /// The legend.
    #[serde(default)]
    pub legend: LegendSpec,
    /// The key map.
    #[serde(default)]
    pub key_map: KeyMapSpec,
    /// The corridor.
    #[serde(default)]
    pub corridor: CorridorSpec,
    /// The centreline.
    #[serde(default)]
    pub centreline: CentrelineSpec,
    /// LiDAR points.
    #[serde(default)]
    pub lidar: LidarSpec,
    /// The elevations.
    #[serde(default)]
    pub elevations: ElevationSpec,
    /// Wires by kind.
    #[serde(default)]
    pub line_styles: LineStyles,
    /// Versions of the line by role.
    #[serde(default)]
    pub roles: BTreeMap<String, RoleSpec>,
    /// Lines below the lowest conductor.
    #[serde(default)]
    pub conductor_clearance: ConductorClearanceSpec,
    /// The weather cases drawn and of the stringing band, by default.
    #[serde(default)]
    pub cases: CasesSpec,
    /// The plan's backgrounds: the project's drawings and orthophotos.
    #[serde(default)]
    pub plan_background: PlanBackgroundSpec,
    /// The title block.
    #[serde(default)]
    pub title_block: TitleBlockSpec,
    /// The revision table.
    #[serde(default)]
    pub revisions: RevisionSpec,
    /// The stringing table.
    #[serde(default)]
    pub stringing_table: StringingTableSpec,
    /// Free texts.
    #[serde(default, rename = "note")]
    pub notes: Vec<NoteSpec>,
    /// Values the user gives.
    #[serde(default, rename = "field")]
    pub fields: Vec<FieldSpec>,
    /// What a field that comes out empty is written as, by field name
    /// (`"*"`: every other field); see [`SheetTemplate::if_empty`].
    #[serde(default)]
    pub if_empty: BTreeMap<String, String>,
    /// Images.
    #[serde(default, rename = "image")]
    pub images: Vec<ImageSpec>,
    /// The images as data URIs, by name (filled when read).
    #[serde(skip)]
    pub resolved: BTreeMap<String, String>,
    /// The text it was read from.
    #[serde(skip)]
    pub source: String,
}

impl Default for SheetTemplate {
    fn default() -> Self {
        Self::builtin()
    }
}

/// The fields every sheet computes (`{name}` in texts).
pub const COMPUTED_FIELDS: [&str; 26] = [
    "project",
    "line",
    "route",
    "route_name",
    "heading",
    "title",
    "title1",
    "title2",
    "title3",
    "sheet",
    "sheets",
    "from",
    "to",
    "from_structure",
    "to_structure",
    "ends",
    "hscale",
    "vscale",
    "plan_scale",
    "exaggeration",
    "scales",
    "format",
    "page_width",
    "page_height",
    "date",
    "cases",
];

/// More computed fields: the coordinate system and the program.
pub const MORE_FIELDS: [&str; 3] = ["crs", "program", "height_datum"];

/// The computed fields that may say nothing (a station range's ends, the
/// title lines not given): left empty unless `[if_empty]` names them.
pub const MAY_BE_EMPTY: [&str; 5] = ["ends", "title", "title1", "title2", "title3"];

/// The fields of a revision row.
pub const REVISION_FIELDS: [&str; 5] =
    ["rev.index", "rev.date", "rev.text", "rev.by", "rev.checked"];

/// The places of `{name}` and `{name:format}` in `text`: (name, format).
pub fn placeholders(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .ok_or_else(|| format!("\"{text}\": a {{ without its }}"))?;
        let inner = &after[..close];
        let (name, fmt) = inner.split_once(':').unwrap_or((inner, ""));
        out.push((name.trim().to_owned(), fmt.to_owned()));
        rest = &after[close + 1..];
    }
    Ok(out)
}

/// `text` with every `{name}` / `{name:format}` replaced by `value(name,
/// format)`; an unknown name stays as written.
pub fn fill(text: &str, value: &dyn Fn(&str, &str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = &after[..close];
        let (name, fmt) = inner.split_once(':').unwrap_or((inner, ""));
        match value(name.trim(), fmt) {
            Some(v) => out.push_str(&v),
            None => {
                out.push('{');
                out.push_str(inner);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// A number written with format `fmt`: `03` zero-padded to 3 digits, `.2`
/// two decimals; else as it is.
pub fn format_number(v: f64, fmt: &str) -> String {
    if let Some(d) = fmt.strip_prefix('.').and_then(|d| d.parse::<usize>().ok()) {
        return format!("{v:.d$}");
    }
    if let Some(w) = fmt.strip_prefix('0').and_then(|w| w.parse::<usize>().ok()) {
        return format!("{:0w$}", v.round() as i64);
    }
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// A date `YYYY-MM-DD` written with format `fmt` (`%Y`, `%m`, `%d`; empty:
/// as it is).
pub fn format_date(date: &str, fmt: &str) -> String {
    if fmt.is_empty() {
        return date.to_owned();
    }
    let mut parts = date.split('-');
    let (y, m, d) = (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
    );
    fmt.replace("%Y", y).replace("%m", m).replace("%d", d)
}

/// The media type of an image named `name` (by its extension).
fn media_type_of(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

/// An image given with a template (by a plugin, or from the project):
/// name, media type, bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateImage {
    /// The name the template uses.
    pub name: String,
    /// `image/svg+xml`, `image/png` or `image/jpeg`.
    pub media_type: String,
    /// The bytes.
    pub bytes: Vec<u8>,
}

impl SheetTemplate {
    /// The built-in template: the sheets as they were before templates.
    pub fn builtin() -> Self {
        Self::parse(BUILTIN, None, &[]).expect("the built-in sheet template reads")
    }

    /// Reads a template file (its images next to it).
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text, path.parent(), &[]).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Reads a template from `text`; its images from `dir` (files named
    /// by `[[image]]` or by the cells), from `images` (given with it), or
    /// embedded (`data`). Checked: the header, every key, the pens, the
    /// title block's grid, every placeholder (a name neither computed nor
    /// declared is an error naming it), every image.
    pub fn parse(text: &str, dir: Option<&Path>, images: &[TemplateImage]) -> Result<Self, String> {
        let mut t: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            t.format.as_deref(),
            t.version,
            FORMAT,
            VERSION,
            "sheet template",
        )?;
        t.source = text.to_owned();
        t.check()?;
        // Images: declared, given, or files named by the cells.
        let mut wanted: Vec<String> = t.images.iter().map(|i| i.name.clone()).collect();
        for c in &t.title_block.cells {
            if let Some(name) = &c.image
                && name != "{logos}"
                && !wanted.contains(name)
            {
                wanted.push(name.clone());
            }
        }
        for name in wanted {
            // (A name declared twice: the one with its data first.)
            let spec = t
                .images
                .iter()
                .filter(|i| i.name == name)
                .find(|i| i.data.is_some())
                .or_else(|| t.images.iter().find(|i| i.name == name));
            let media = spec
                .and_then(|s| s.media_type.clone())
                .or_else(|| media_type_of(&name).map(str::to_owned))
                .ok_or_else(|| format!("image \"{name}\": SVG, PNG or JPEG (give media_type)"))?;
            let bytes = if let Some(data) = spec.and_then(|s| s.data.as_ref()) {
                crate::unbase64(data).ok_or_else(|| format!("image \"{name}\": bad base64"))?
            } else if let Some(img) = images.iter().find(|i| i.name == name) {
                img.bytes.clone()
            } else {
                let file = spec.and_then(|s| s.file.clone()).unwrap_or(name.clone());
                let path = dir
                    .ok_or_else(|| format!("image \"{name}\": not given with the template"))?
                    .join(&file);
                std::fs::read(&path)
                    .map_err(|e| format!("image \"{name}\": {}: {e}", path.display()))?
            };
            t.resolved.insert(
                name.clone(),
                format!("data:{media};base64,{}", crate::base64(&bytes)),
            );
        }
        Ok(t)
    }

    /// The template's text with its images embedded (`data`), to keep in
    /// a project: one text holds it all (an image read from a file is
    /// appended as an `[[image]]` with its data, which the reader takes
    /// before the file).
    pub fn embedded(&self) -> String {
        let mut text = self.source.clone();
        for (name, uri) in &self.resolved {
            let has_data = self
                .images
                .iter()
                .any(|i| &i.name == name && i.data.is_some());
            if has_data {
                continue;
            }
            let (media, data) = uri
                .strip_prefix("data:")
                .and_then(|u| u.split_once(";base64,"))
                .unwrap_or(("", ""));
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!(
                "\n[[image]]\nname = \"{name}\"\nmedia_type = \"{media}\"\ndata = \"{data}\"\n"
            ));
        }
        text
    }

    /// The names a text may use: computed, more, declared fields.
    fn known(&self, name: &str) -> bool {
        COMPUTED_FIELDS.contains(&name)
            || MORE_FIELDS.contains(&name)
            || self.fields.iter().any(|f| f.key == name)
    }

    fn check(&self) -> Result<(), String> {
        let p = &self.page;
        if let Some(f) = &p.format
            && f != "optimised"
            && crate::page_format(f).is_none()
        {
            return Err(format!(
                "page format \"{f}\": a standard paper size (A0–A4, A3x3, B2, ANSI D, \
                 ARCH E …), WIDTHxHEIGHT in mm, or \"optimised\""
            ));
        }
        if (p.sizes.is_some() || p.mixed.is_some()) && p.format.as_deref() != Some("optimised") {
            return Err("page: sizes and mixed go with format = \"optimised\"".into());
        }
        if let Some(s) = &p.sizes {
            crate::paper::Optimise::from_names(s, false).map_err(|e| format!("page: {e}"))?;
        }
        if let Some(o) = &p.orientation
            && o != "landscape"
            && o != "portrait"
        {
            return Err(format!("orientation \"{o}\": give landscape or portrait"));
        }
        if p.width.is_some() != p.height.is_some() {
            return Err("page: give width and height together".into());
        }
        if let (Some(w), Some(h)) = (p.width, p.height)
            && !(w.0 > 50.0 && h.0 > 50.0 && w.0 <= 20_000.0 && h.0 <= 20_000.0)
        {
            return Err("page: width and height from 50 to 20 000 mm".into());
        }
        let m = &p.margin;
        if [m.left, m.top, m.right, m.bottom]
            .iter()
            .any(|v| !(0.0..=100.0).contains(&v.0))
        {
            return Err("page margins from 0 to 100 mm".into());
        }
        if self.text.font != "Helvetica" {
            return Err(format!(
                "font \"{}\": only Helvetica (the PDF standard font) for now; embedding \
                 another font is planned",
                self.text.font
            ));
        }
        for (layer, pen) in &self.layers {
            if !crate::model::Layer::ALL.iter().any(|l| l.name() == layer) {
                let names: Vec<&str> = crate::model::Layer::ALL.iter().map(|l| l.name()).collect();
                return Err(format!(
                    "layers: no layer \"{layer}\" (layers: {})",
                    names.join(", ")
                ));
            }
            pen.check(&format!("layers.{layer}"))?;
        }
        for o in &self.layout.order {
            if !["plan", "profile", "bands"].contains(&o.as_str()) {
                return Err(format!(
                    "layout.order: \"{o}\" (give plan, profile and bands)"
                ));
            }
        }
        for (what, place, allowed) in [
            ("north_arrow", &self.north_arrow.place, &["plan", "off"][..]),
            (
                "scale_bar",
                &self.scale_bar.place,
                &["bottom", "title_block", "plan", "profile", "off"][..],
            ),
            (
                "legend",
                &self.legend.place,
                &["above_title_block", "bottom", "off"][..],
            ),
            ("key_map", &self.key_map.place, &["bottom", "off"][..]),
            (
                "revisions",
                &self.revisions.place,
                &["above_title_block", "off"][..],
            ),
            (
                "stringing_table",
                &self.stringing_table.place,
                &["above_title_block", "bottom", "off"][..],
            ),
        ] {
            if !allowed.contains(&place.as_str()) {
                return Err(format!(
                    "{what}.place = \"{place}\": give {}",
                    allowed.join(", ")
                ));
            }
        }
        if !["route", "sheet"].contains(&self.key_map.extent.as_str()) {
            return Err(format!(
                "key_map.extent = \"{}\": give route or sheet",
                self.key_map.extent
            ));
        }
        if let Some(s) = self.key_map.scale
            && !(s > 0.0 && s.is_finite())
        {
            return Err("key_map.scale: a positive denominator".into());
        }
        if let Some(place) = &self.elevations.place
            && crate::ElevationPlace::parse(place).is_none()
        {
            return Err(format!(
                "elevations.place = \"{place}\": give above, below or off"
            ));
        }
        for l in self.elevations.labels.iter().flatten() {
            if !crate::ElevationLabels::NAMES.contains(&l.as_str()) {
                return Err(format!(
                    "elevations.labels: \"{l}\" (give {})",
                    crate::ElevationLabels::NAMES.join(", ")
                ));
            }
        }
        if let Some(s) = self.elevations.scale
            && !(s > 0.0 && s.is_finite())
        {
            return Err("elevations.scale: a positive denominator".into());
        }
        if !["also", "instead"].contains(&self.conductor_clearance.mode.as_str()) {
            return Err(format!(
                "conductor_clearance.mode = \"{}\": give also or instead",
                self.conductor_clearance.mode
            ));
        }
        self.conductor_clearance
            .pen
            .check("conductor_clearance.pen")?;
        self.corridor.pen.check("corridor.pen")?;
        self.elevations.pen.check("elevations.pen")?;
        self.key_map.highlight.check("key_map.highlight")?;
        for (k, s) in &self.line_styles.kinds {
            if !["hv", "lv", "telecom", "earth"].contains(&k.as_str()) {
                return Err(format!(
                    "line_styles.kinds.{k}: give hv, lv, telecom or earth"
                ));
            }
            s.pen.check(&format!("line_styles.kinds.{k}"))?;
        }
        for (r, s) in &self.roles {
            for c in [&s.colour, &s.structure_colour].into_iter().flatten() {
                if crate::model::Color::parse(c).is_none() {
                    return Err(format!("roles.{r}: colour \"{c}\" is not #rrggbb"));
                }
            }
        }
        // The title block's grid.
        let tb = &self.title_block;
        if tb.rows.is_empty() || tb.columns.is_empty() {
            return Err("title_block: give its rows and columns".into());
        }
        for c in &tb.cells {
            let [r, k] = c.at;
            let [rs, ks] = c.span;
            if rs == 0 || ks == 0 || r + rs > tb.rows.len() || k + ks > tb.columns.len() {
                return Err(format!(
                    "title_block cell at [{r}, {k}] (span [{rs}, {ks}]) is outside its {} rows \
                     and {} columns",
                    tb.rows.len(),
                    tb.columns.len()
                ));
            }
        }
        // Fields: unique keys; every placeholder known.
        for (k, f) in self.fields.iter().enumerate() {
            if f.key.is_empty() || f.key.contains(['{', '}', ':', ' ']) {
                return Err(format!(
                    "field \"{}\": a key without spaces or braces",
                    f.key
                ));
            }
            if COMPUTED_FIELDS.contains(&f.key.as_str()) || MORE_FIELDS.contains(&f.key.as_str()) {
                return Err(format!(
                    "field \"{}\" is computed: choose another key",
                    f.key
                ));
            }
            if self.fields[..k].iter().any(|g| g.key == f.key) {
                return Err(format!("field \"{}\" twice", f.key));
            }
        }
        let mut texts: Vec<(String, &str)> = tb
            .cells
            .iter()
            .map(|c| (format!("title_block cell at {:?}", c.at), c.text.as_str()))
            .collect();
        texts.extend(
            self.notes
                .iter()
                .map(|n| ("note".to_owned(), n.text.as_str())),
        );
        for (what, text) in texts {
            for (name, _) in placeholders(text).map_err(|e| format!("{what}: {e}"))? {
                if !self.known(&name) {
                    return Err(format!(
                        "{what}: unknown field {{{name}}} (computed: {}; or declare it as a \
                         [[field]])",
                        COMPUTED_FIELDS.join(", ")
                    ));
                }
            }
        }
        for (what, text, own) in [
            (
                "corridor.label",
                self.corridor.label.as_deref().unwrap_or(""),
                &["width"][..],
            ),
            (
                "conductor_clearance.label",
                self.conductor_clearance.label.as_str(),
                &["distance", "rule", "circuit", "case", "voltage"][..],
            ),
        ] {
            for (name, _) in placeholders(text).map_err(|e| format!("{what}: {e}"))? {
                if !own.contains(&name.as_str()) {
                    return Err(format!(
                        "{what}: unknown field {{{name}}} (give {})",
                        own.iter()
                            .map(|n| format!("{{{n}}}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
        for c in &self.revisions.columns {
            for (name, _) in placeholders(&c.value)? {
                if !REVISION_FIELDS.contains(&name.as_str()) && !self.known(&name) {
                    return Err(format!(
                        "revisions column \"{}\": unknown field {{{name}}} (give {})",
                        c.title,
                        REVISION_FIELDS.join(", ")
                    ));
                }
            }
        }
        for c in &self.stringing_table.columns {
            for (name, _) in placeholders(&c.value)? {
                if !STRINGING_FIELDS.contains(&name.as_str()) && !self.known(&name) {
                    return Err(format!(
                        "stringing_table column \"{}\": unknown field {{{name}}} (give {})",
                        c.title,
                        STRINGING_FIELDS.join(", ")
                    ));
                }
            }
        }
        for k in self.if_empty.keys() {
            if k != "*" && !self.known(k) {
                return Err(format!(
                    "if_empty: no field \"{k}\" (a computed or declared field, or \"*\")"
                ));
            }
        }
        for (kind, text) in &self.legend.labels {
            let Some((_, _, own)) = LEGEND_LABELS.iter().find(|(k, ..)| k == kind) else {
                let kinds: Vec<&str> = LEGEND_LABELS.iter().map(|(k, ..)| *k).collect();
                return Err(format!(
                    "legend.labels.{kind}: no such entry (entries: {})",
                    kinds.join(", ")
                ));
            };
            for (name, _) in placeholders(text).map_err(|e| format!("legend.labels.{kind}: {e}"))? {
                if !own.contains(&name.as_str()) {
                    return Err(format!(
                        "legend.labels.{kind}: unknown field {{{name}}} (give {})",
                        if own.is_empty() {
                            "none".to_owned()
                        } else {
                            own.iter()
                                .map(|n| format!("{{{n}}}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    ));
                }
            }
        }
        if !["right", "left"].contains(&self.corridor.near_side.as_str()) {
            return Err(format!(
                "corridor.near_side = \"{}\": give right or left",
                self.corridor.near_side
            ));
        }
        self.corridor.near.check("corridor.near")?;
        self.corridor.far.check("corridor.far")?;
        self.centreline.pen.check("centreline.pen")?;
        self.plan_background.check()?;
        if !["auto", "middle", "start", "end"].contains(&self.conductor_clearance.label_at.as_str())
        {
            return Err(format!(
                "conductor_clearance.label_at = \"{}\": give auto, middle, start or end",
                self.conductor_clearance.label_at
            ));
        }
        let l = &self.lidar;
        if !(l.size.0 > 0.0 && l.size.0 <= 5.0) || !(l.cell.0 >= 0.0 && l.cell.0 <= 10.0) {
            return Err("lidar: size from 0 to 5 mm, cell from 0 to 10 mm".into());
        }
        for (k, s) in &l.styles {
            class_codes(k)?;
            if let Some(c) = &s.colour
                && crate::model::Color::parse(c).is_none()
            {
                return Err(format!(
                    "lidar.styles.\"{k}\": colour \"{c}\" is not #rrggbb"
                ));
            }
            if s.size.is_some_and(|v| !(v.0 > 0.0 && v.0 <= 5.0)) {
                return Err(format!("lidar.styles.\"{k}\": a size from 0 to 5 mm"));
            }
        }
        for b in &self.bands {
            if let Some(f) = &b.format {
                if b.kind != BandKind::Structures {
                    return Err("band: `format` is for the kind structures".into());
                }
                for (name, _) in placeholders(f).map_err(|e| format!("band format: {e}"))? {
                    if !STRUCTURE_BAND_FIELDS.contains(&name.as_str()) {
                        return Err(format!(
                            "band format: unknown field {{{name}}} (give {})",
                            STRUCTURE_BAND_FIELDS
                                .iter()
                                .map(|n| format!("{{{n}}}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
            }
            if b.kind == BandKind::Text {
                let f = b
                    .field
                    .as_deref()
                    .ok_or("a band of kind text names its field")?;
                if !self.fields.iter().any(|x| x.key == f) {
                    return Err(format!("band: field \"{f}\" is not declared"));
                }
            }
        }
        Ok(())
    }

    /// The optimised layout the template asks for (`format =
    /// "optimised"`).
    pub fn optimise(&self) -> Option<crate::paper::Optimise> {
        let p = &self.page;
        if p.format.as_deref() != Some("optimised") {
            return None;
        }
        let sizes = p.sizes.clone().unwrap_or_else(crate::paper::default_sizes);
        let mut o = crate::paper::Optimise::from_names(&sizes, p.mixed.unwrap_or(false)).ok()?;
        match p.orientation.as_deref() {
            Some("landscape") => o.portrait = false,
            Some("portrait") => o.landscape = false,
            _ => {}
        }
        Some(o)
    }

    /// The page the template asks for (none: the export's, or
    /// [`Self::optimise`]).
    pub fn page(&self) -> Option<crate::Page> {
        let p = &self.page;
        let mut page = match (&p.format, p.width, p.height) {
            (_, Some(w), Some(h)) => crate::Page {
                name: format!("{:.0}x{:.0}", w.0, h.0),
                width: w.0,
                height: h.0,
            },
            (Some(f), ..) => crate::page_format(f)?,
            _ => return None,
        };
        if p.orientation.as_deref() == Some("portrait") {
            page = page.portrait();
        }
        Some(page)
    }

    /// What field `name` is written as when it comes out empty: the
    /// template's `[if_empty]` for it, else (but for the fields that may
    /// say nothing, [`MAY_BE_EMPTY`]) its `"*"`, else "unknown datum"
    /// (`height_datum`), "unknown coordinate system" (`crs`) or "–". None:
    /// it stays empty.
    pub fn if_empty(&self, name: &str) -> Option<String> {
        if let Some(t) = self.if_empty.get(name) {
            return Some(t.clone());
        }
        if MAY_BE_EMPTY.contains(&name) {
            return None;
        }
        if let Some(t) = self.if_empty.get("*") {
            return Some(t.clone());
        }
        Some(
            match name {
                "height_datum" => "unknown datum",
                "crs" => "unknown coordinate system",
                _ => "–",
            }
            .to_owned(),
        )
    }

    /// The pen of `layer`: `pen` with what the template gives.
    pub fn pen(&self, layer: crate::model::Layer, pen: crate::model::Pen) -> crate::model::Pen {
        match self.layers.get(layer.name()) {
            Some(spec) => spec.over(pen),
            None => pen,
        }
    }

    /// Why an export with `values` cannot run: the required fields left
    /// empty, named (none: it can).
    pub fn missing_fields(&self, values: &SheetFields) -> Option<String> {
        let missing: Vec<String> = self
            .fields
            .iter()
            .filter(|f| f.required)
            .filter(|f| {
                if f.per_sheet {
                    values
                        .per_sheet
                        .get(&f.key)
                        .is_none_or(|m| m.values().all(|v| v.trim().is_empty()))
                        && f.default.trim().is_empty()
                } else {
                    values
                        .values
                        .get(&f.key)
                        .map_or(f.default.as_str(), String::as_str)
                        .trim()
                        .is_empty()
                }
            })
            .map(|f| {
                if f.label.is_empty() {
                    f.key.clone()
                } else {
                    format!("{} ({})", f.label, f.key)
                }
            })
            .collect();
        (!missing.is_empty()).then(|| {
            format!(
                "the sheet template \"{}\" needs: {} — fill them in (or untick \"required\" in a \
                 draft copy of the template)",
                self.name,
                missing.join(", ")
            )
        })
    }
}

/// A row of the revision table.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, Deserialize)]
#[serde(default)]
pub struct Revision {
    /// Its index, e.g. `A`.
    pub index: String,
    /// Its date.
    pub date: String,
    /// What changed.
    pub text: String,
    /// Who made it.
    pub by: String,
    /// Who checked it.
    pub checked: String,
}

impl Revision {
    /// From `index;date;text;by;checked` (the last ones may be left out).
    pub fn parse(text: &str) -> Self {
        let mut p = text.split(';').map(|s| s.trim().to_owned());
        Self {
            index: p.next().unwrap_or_default(),
            date: p.next().unwrap_or_default(),
            text: p.next().unwrap_or_default(),
            by: p.next().unwrap_or_default(),
            checked: p.next().unwrap_or_default(),
        }
    }

    /// Its value of `rev.<name>`.
    pub fn get(&self, name: &str) -> Option<&str> {
        Some(match name {
            "rev.index" => &self.index,
            "rev.date" => &self.date,
            "rev.text" => &self.text,
            "rev.by" => &self.by,
            "rev.checked" => &self.checked,
            _ => return None,
        })
    }
}

/// What the user gives a template's fields: values, values per sheet (by
/// sheet number), revisions.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, Deserialize)]
#[serde(default)]
pub struct SheetFields {
    /// By key.
    pub values: BTreeMap<String, String>,
    /// By key, then sheet number (1-based).
    pub per_sheet: BTreeMap<String, BTreeMap<usize, String>>,
    /// The revisions, oldest first.
    pub revisions: Vec<Revision>,
}

impl SheetFields {
    /// From a TOML fields file: `[values]` (or keys at the top),
    /// `[per_sheet.<key>]` with `"<n>" = value`, `[[revision]]`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut out = Self::default();
        let as_text = |x: &toml::Value| match x {
            toml::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        for (k, x) in &v {
            match (k.as_str(), x) {
                ("values", toml::Value::Table(t)) => {
                    for (k, x) in t {
                        out.values.insert(k.clone(), as_text(x));
                    }
                }
                ("per_sheet", toml::Value::Table(t)) => {
                    for (k, x) in t {
                        let toml::Value::Table(rows) = x else {
                            return Err(format!("per_sheet.{k}: a table of sheet = value"));
                        };
                        let mut m = BTreeMap::new();
                        for (n, x) in rows {
                            let n: usize = n.parse().map_err(|_| {
                                format!("per_sheet.{k}: \"{n}\" is no sheet number")
                            })?;
                            m.insert(n, as_text(x));
                        }
                        out.per_sheet.insert(k.clone(), m);
                    }
                }
                ("revision", toml::Value::Array(rows)) => {
                    for r in rows {
                        let r: Revision = r
                            .clone()
                            .try_into()
                            .map_err(|e: toml::de::Error| format!("revision: {e}"))?;
                        out.revisions.push(r);
                    }
                }
                (k, toml::Value::Table(_) | toml::Value::Array(_)) => {
                    return Err(format!("{k}: give values, per_sheet or [[revision]]"));
                }
                (k, x) => {
                    out.values.insert(k.to_owned(), as_text(x));
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_read_with_their_units() {
        assert_eq!(
            with_unit("10 mm", &[("mm", 1.0), ("cm", 10.0)], 1.0),
            Ok(10.0)
        );
        assert_eq!(
            with_unit("1.5cm", &[("mm", 1.0), ("cm", 10.0)], 1.0),
            Ok(15.0)
        );
        assert_eq!(
            with_unit("2 km", &[("km", 1000.0), ("m", 1.0)], 1.0),
            Ok(2000.0)
        );
        assert!(with_unit("ten mm", &[("mm", 1.0)], 1.0).is_err());
    }

    #[test]
    fn placeholders_fill() {
        let v = |n: &str, f: &str| match n {
            "sheet" => Some(format_number(3.0, f)),
            "date" => Some(format_date("2026-10-20", f)),
            _ => None,
        };
        assert_eq!(
            fill("{sheet:03} of {date:%d.%m.%Y} {x}", &v),
            "003 of 20.10.2026 {x}"
        );
        assert_eq!(
            placeholders("a {b} {c:03}").unwrap(),
            [("b".into(), "".into()), ("c".into(), "03".into())]
        );
        assert!(placeholders("a {b").is_err());
    }

    #[test]
    fn the_builtin_template_reads() {
        let t = SheetTemplate::builtin();
        assert_eq!(t.name, "default");
        assert!(t.page().is_none());
        assert!(t.fields.is_empty());
        // The shipped copy is the same file.
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/sheet-templates/default.ohlsheet");
        assert_eq!(std::fs::read_to_string(shipped).unwrap(), BUILTIN);
    }

    #[test]
    fn templates_are_checked() {
        let head = "format = \"ohl-sheet-template\"\nversion = 1\n";
        let bad =
            |body: &str| SheetTemplate::parse(&format!("{head}{body}"), None, &[]).unwrap_err();
        assert!(bad("colour = 1").contains("unknown field"));
        assert!(bad("[page]\nformat = \"B7\"").contains("A0–A4"));
        assert!(bad("[page]\nsizes = [\"A3\"]").contains("optimised"));
        assert!(bad("[page]\nformat = \"optimised\"\nsizes = [\"Q9\"]").contains("Q9"));
        assert!(bad("[layers]\nOHL_NOTHING = { width = \"1 mm\" }").contains("no layer"));
        assert!(
            bad("[title_block]\ncolumns = [\"*\"]\nrows = [\"10 mm\"]\n[[title_block.cell]]\nat = [0, 0]\ntext = \"{nothing}\"")
                .contains("unknown field {nothing}")
        );
        assert!(
            bad("[title_block]\ncolumns = [\"*\"]\nrows = [\"10 mm\"]\n[[title_block.cell]]\nat = [1, 0]")
                .contains("outside")
        );
        assert!(bad("[legend]\nplace = \"left\"").contains("legend.place"));
        assert!(
            SheetTemplate::parse("format = \"ohl-sheet-template\"\nversion = 2", None, &[])
                .unwrap_err()
                .contains("newer")
        );
        // A declared field may be used; a required one left empty refuses.
        let t = SheetTemplate::parse(
            &format!(
                "{head}name = \"t\"\n[title_block]\ncolumns = [\"*\"]\nrows = [\"10 mm\"]\n[[title_block.cell]]\nat = [0, 0]\ntext = \"{{project_no}}\"\n[[field]]\nkey = \"project_no\"\nlabel = \"Project number\"\nrequired = true"
            ),
            None,
            &[],
        )
        .unwrap();
        let mut f = SheetFields::default();
        assert!(
            t.missing_fields(&f)
                .unwrap()
                .contains("Project number (project_no)")
        );
        f.values.insert("project_no".into(), "P-1".into());
        assert_eq!(t.missing_fields(&f), None);
    }

    #[test]
    fn fields_files_read() {
        let f = SheetFields::parse(
            "project_no = \"P-7\"\n[values]\ndrawn_by = \"AB\"\n[per_sheet.remarks]\n2 = \"see note\"\n[[revision]]\nindex = \"A\"\ntext = \"first issue\"",
        )
        .unwrap();
        assert_eq!(f.values["project_no"], "P-7");
        assert_eq!(f.values["drawn_by"], "AB");
        assert_eq!(f.per_sheet["remarks"][&2], "see note");
        assert_eq!(f.revisions[0].text, "first issue");
        assert_eq!(Revision::parse("B;2026-10-20;changes;LS").by, "LS");
    }
}
