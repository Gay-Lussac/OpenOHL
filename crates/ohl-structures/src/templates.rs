// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Structure templates (`*.ohlpt`, one per file; "pylon templates" before
//! 2026-10-21): the face of a structure (2D, across and up in its local
//! axes) as a few named dimensions, from which a structure type of level 1
//! (points, strings, wire points with their sets) is built — for
//! suspension or for strain. The library of templates grows with files:
//! those shipped in `libraries/structure-templates`, and those a project
//! keeps in its `Structure templates` folder (the folders' names before,
//! [`SHIPPED_FOLDERS`], [`PROJECT_FOLDERS`], [`BESIDE_FOLDERS`], and the
//! format id before, [`TEMPLATE_FILE_FORMAT_BEFORE`], still read).
//!
//! Where a phase hangs is an **attachment** with its **set** (the key
//! `slot`): its wire's set on a suspension type. On a strain type each
//! phase has two level tension strings along the line, their directions
//! given by their angles only: the first at 180° with set
//! `slot + first_offset`, the second at 0° with set
//! `slot + second_offset` (default 20 and 40: sets 1–20, then 21–40 and
//! 41–60). Earth wires have the same: a point on the structure and a short
//! string of their own (hanging on a suspension type, two tension strings
//! on a strain type).
//!
//! Positions are expressions of the parameters (metres): numbers, the
//! parameters' names, `+ - * /` and parentheses.
//!
//! ```toml
//! format = "ohl-pylon-template"
//! version = 1
//! id = "delta-1c"
//! name = "Delta, one circuit"
//! kind = "lattice"
//! height = "peak"                  # mast top (stick drawing)
//!
//! [strings]
//! suspension_length = "3 m"
//! suspension_weight = "1 kN"
//! strain_length = "4 m"
//! strain_weight = "1.5 kN"
//! earth_suspension_length = "0.3 m"
//! earth_suspension_weight = "0.1 kN"
//! earth_strain_length = "0.5 m"
//! earth_strain_weight = "0.2 kN"
//!
//! [[parameter]]
//! name = "arm_level"
//! default = "24 m"
//! description = "height of the crossarm"
//!
//! [[attachment]]
//! slot = 1
//! label = "L1"
//! across = "-arm_half"
//! up = "arm_level"
//!
//! [[earth]]
//! slot = 19
//! across = "0"
//! up = "peak"
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ohl_core::units::{Force, Length};
use serde::Deserialize;

use crate::LibraryItem;
use crate::structures::StructureKind;

/// File extension of structure templates.
pub const TEMPLATE_FILE_EXTENSION: &str = "ohlpt";
/// `format` of a structure template.
pub const TEMPLATE_FILE_FORMAT: &str = "ohl-structure-template";
/// `format` of a structure template before 2026-10-21 (and its `ols` name
/// before 2026-10-18): still read.
pub const TEMPLATE_FILE_FORMAT_BEFORE: &str = "ohl-pylon-template";

/// The shipped library's folder of structure templates (`libraries/…`),
/// then its name before 2026-10-21, still read.
pub const SHIPPED_FOLDERS: [&str; 2] = ["structure-templates", "pylon-templates"];
/// A project's own folder beside its line (OHL-Design), then its name
/// before 2026-10-21, still read.
pub const PROJECT_FOLDERS: [&str; 2] = ["Structure templates", "Pylon templates"];
/// The folder beside a structure file (OHL-Structure), then its name
/// before 2026-10-21, still read.
pub const BESIDE_FOLDERS: [&str; 2] = ["StructureTemplates", "PylonTemplates"];

/// The format id a template file is checked against: the one before
/// 2026-10-21 when the file gives it, else the current one.
fn expected_format(found: Option<&str>) -> &'static str {
    match found {
        Some(f) if ohl_core::files::is_format(f, TEMPLATE_FILE_FORMAT_BEFORE) => {
            TEMPLATE_FILE_FORMAT_BEFORE
        }
        _ => TEMPLATE_FILE_FORMAT,
    }
}
/// Newest format version read.
pub const TEMPLATE_FILE_VERSION: u32 = 1;

/// A named dimension of a template.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    /// Name used in the expressions (letters, digits, `_`).
    pub name: String,
    /// Value offered.
    pub default: Length,
    /// What it is, in words.
    #[serde(default)]
    pub description: String,
}

/// Where a phase hangs (or an earth wire is clamped), on the face.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    /// Its slot: the set of a suspension type; strain types add the
    /// offsets.
    pub slot: u32,
    /// Label (default: `P<slot>`, earth wires `E<slot>`).
    #[serde(default)]
    pub label: String,
    /// Across the line (to the right positive), an expression, m.
    pub across: String,
    /// Up from the ground, an expression, m.
    pub up: String,
}

/// The strings a template gives its phases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strings {
    /// Suspension string length.
    #[serde(default = "default_suspension_length")]
    pub suspension_length: Length,
    /// Suspension string weight.
    #[serde(default = "default_suspension_weight")]
    pub suspension_weight: Force,
    /// Strain string length.
    #[serde(default = "default_strain_length")]
    pub strain_length: Length,
    /// Strain string weight.
    #[serde(default = "default_strain_weight")]
    pub strain_weight: Force,
    /// Earth wires: suspension string (clamp) length.
    #[serde(default = "default_earth_suspension_length")]
    pub earth_suspension_length: Length,
    /// Earth wires: suspension string weight.
    #[serde(default = "default_earth_suspension_weight")]
    pub earth_suspension_weight: Force,
    /// Earth wires: strain string length.
    #[serde(default = "default_earth_strain_length")]
    pub earth_strain_length: Length,
    /// Earth wires: strain string weight.
    #[serde(default = "default_earth_strain_weight")]
    pub earth_strain_weight: Force,
}

fn default_suspension_length() -> Length {
    Length::from_si(3.0)
}
fn default_suspension_weight() -> Force {
    Force::from_si(1000.0)
}
fn default_strain_length() -> Length {
    Length::from_si(4.0)
}
fn default_strain_weight() -> Force {
    Force::from_si(1500.0)
}
fn default_earth_suspension_length() -> Length {
    Length::from_si(0.3)
}
fn default_earth_suspension_weight() -> Force {
    Force::from_si(100.0)
}
fn default_earth_strain_length() -> Length {
    Length::from_si(0.5)
}
fn default_earth_strain_weight() -> Force {
    Force::from_si(200.0)
}

impl Default for Strings {
    fn default() -> Self {
        Self {
            suspension_length: default_suspension_length(),
            suspension_weight: default_suspension_weight(),
            strain_length: default_strain_length(),
            strain_weight: default_strain_weight(),
            earth_suspension_length: default_earth_suspension_length(),
            earth_suspension_weight: default_earth_suspension_weight(),
            earth_strain_length: default_earth_strain_length(),
            earth_strain_weight: default_earth_strain_weight(),
        }
    }
}

fn default_back() -> u32 {
    20
}
fn default_ahead() -> u32 {
    40
}

/// A pylon template.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PylonTemplate {
    /// `"ohl-pylon-template"` (optional header).
    #[serde(default)]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default)]
    pub version: Option<u32>,
    /// Unique id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// What it is, in words.
    #[serde(default)]
    pub description: String,
    /// Family of the types built.
    pub kind: StructureKind,
    /// Mast top for the stick drawing, an expression (default: the
    /// highest attachment).
    #[serde(default)]
    pub height: Option<String>,
    /// Strain types: the first string's set (direction 180°) is the slot
    /// plus this (files written before: `back_offset`).
    #[serde(default = "default_back", alias = "back_offset")]
    pub first_offset: u32,
    /// Strain types: the second string's set (direction 0°) is the slot
    /// plus this (files written before: `ahead_offset`).
    #[serde(default = "default_ahead", alias = "ahead_offset")]
    pub second_offset: u32,
    /// The strings' lengths and weights offered.
    #[serde(default)]
    pub strings: Strings,
    /// Its dimensions.
    #[serde(rename = "parameter", default)]
    pub parameters: Vec<Parameter>,
    /// Where the phases hang.
    #[serde(rename = "attachment", default)]
    pub attachments: Vec<Attachment>,
    /// Where the earth wires are clamped.
    #[serde(rename = "earth", default)]
    pub earths: Vec<Attachment>,
}

/// What a type is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Suspension strings hanging down.
    Suspension,
    /// Two strain strings per phase, back and ahead along the line.
    Strain,
}

impl Role {
    /// As written in a type's `[template]` record.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Suspension => "suspension",
            Role::Strain => "strain",
        }
    }

    /// Read from a type's `[template]` record.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "suspension" => Some(Role::Suspension),
            "strain" => Some(Role::Strain),
            _ => None,
        }
    }
}

/// The values a type is built with: every parameter (m), and the strings.
#[derive(Debug, Clone, PartialEq)]
pub struct Values {
    /// Each parameter's value, m.
    pub dimensions: BTreeMap<String, f64>,
    /// String length, m (suspension or strain, as the role).
    pub string_length: f64,
    /// String weight, N.
    pub string_weight: f64,
    /// Earth wires' string length, m.
    pub earth_length: f64,
    /// Earth wires' string weight, N.
    pub earth_weight: f64,
}

/// A point of the face, worked out: label, slot, (across, up), earth wire.
#[derive(Debug, Clone, PartialEq)]
pub struct FacePoint {
    /// Label.
    pub label: String,
    /// Slot.
    pub slot: u32,
    /// Across the line, m.
    pub across: f64,
    /// Up, m.
    pub up: f64,
    /// An earth wire (clamped directly).
    pub earth: bool,
}

impl LibraryItem for PylonTemplate {
    const EXTENSION: &'static str = TEMPLATE_FILE_EXTENSION;
    const KIND: &'static str = "structure template";

    fn id(&self) -> &str {
        &self.id
    }

    fn check(&self) -> Result<(), String> {
        ohl_core::files::check_header(
            self.format.as_deref(),
            self.version,
            expected_format(self.format.as_deref()),
            TEMPLATE_FILE_VERSION,
            "structure template",
        )?;
        let mut names = BTreeMap::new();
        for p in &self.parameters {
            let ok = !p.name.is_empty()
                && p.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !p.name.starts_with(|c: char| c.is_ascii_digit());
            if !ok {
                return Err(format!(
                    "parameter \"{}\": letters, digits and _ only",
                    p.name
                ));
            }
            if names.insert(p.name.as_str(), ()).is_some() {
                return Err(format!("parameter {} given twice", p.name));
            }
        }
        if self.attachments.is_empty() {
            return Err("no attachment: where do the phases hang?".into());
        }
        if self.first_offset == self.second_offset {
            return Err("first_offset and second_offset must differ".into());
        }
        // Built with the defaults, it must make a valid type both ways.
        let defaults = self.defaults(Role::Suspension);
        self.face(&defaults.dimensions)?;
        for role in [Role::Suspension, Role::Strain] {
            self.build(&self.defaults(role), role, "check", "check")?;
        }
        Ok(())
    }
}

impl PylonTemplate {
    /// The values offered: the parameters' defaults, the strings of
    /// `role`.
    pub fn defaults(&self, role: Role) -> Values {
        let (l, w) = match role {
            Role::Suspension => (
                self.strings.suspension_length,
                self.strings.suspension_weight,
            ),
            Role::Strain => (self.strings.strain_length, self.strings.strain_weight),
        };
        let (el, ew) = self.earth_strings(role);
        Values {
            dimensions: self
                .parameters
                .iter()
                .map(|p| (p.name.clone(), p.default.si()))
                .collect(),
            string_length: l.si(),
            string_weight: w.si(),
            earth_length: el,
            earth_weight: ew,
        }
    }

    /// The earth wires' string length and weight offered for `role` (m, N).
    pub fn earth_strings(&self, role: Role) -> (f64, f64) {
        match role {
            Role::Suspension => (
                self.strings.earth_suspension_length.si(),
                self.strings.earth_suspension_weight.si(),
            ),
            Role::Strain => (
                self.strings.earth_strain_length.si(),
                self.strings.earth_strain_weight.si(),
            ),
        }
    }

    /// The face with `dimensions`: its attachments then its earth wires.
    pub fn face(&self, dimensions: &BTreeMap<String, f64>) -> Result<Vec<FacePoint>, String> {
        let mut out = Vec::new();
        let mut slots = BTreeMap::new();
        for (earth, list) in [(false, &self.attachments), (true, &self.earths)] {
            for a in list {
                // Back and ahead sets must not meet: slots up to the gap
                // between the offsets (and the smaller offset).
                let max = self
                    .first_offset
                    .abs_diff(self.second_offset)
                    .min(self.first_offset.min(self.second_offset));
                if a.slot == 0 || a.slot > max {
                    return Err(format!("set {}: from 1 to {max}", a.slot));
                }
                if slots.insert(a.slot, ()).is_some() {
                    return Err(format!("set {} given twice", a.slot));
                }
                let label = if a.label.is_empty() {
                    format!("{}{}", if earth { "E" } else { "P" }, a.slot)
                } else {
                    a.label.clone()
                };
                let across = eval(&a.across, dimensions)
                    .map_err(|e| format!("{label} across \"{}\": {e}", a.across))?;
                let up =
                    eval(&a.up, dimensions).map_err(|e| format!("{label} up \"{}\": {e}", a.up))?;
                if up <= 0.0 {
                    return Err(format!("{label}: {up:.2} m up, not above the ground"));
                }
                out.push(FacePoint {
                    label,
                    slot: a.slot,
                    across,
                    up,
                    earth,
                });
            }
        }
        let mut labels = BTreeMap::new();
        for p in &out {
            if labels.insert(p.label.as_str(), ()).is_some() {
                return Err(format!("label {} given twice", p.label));
            }
        }
        Ok(out)
    }

    /// The mast top with `dimensions` (none: the highest attachment).
    pub fn mast(&self, dimensions: &BTreeMap<String, f64>) -> Result<Option<f64>, String> {
        self.height
            .as_ref()
            .map(|h| eval(h, dimensions).map_err(|e| format!("height \"{h}\": {e}")))
            .transpose()
    }

    /// The text of structure type `id` (version 2, level 1) built for
    /// `role` with `values`, checked by reading it back. It records the
    /// template, the role and the values (`[template]`), to be built again.
    pub fn build(
        &self,
        values: &Values,
        role: Role,
        id: &str,
        name: &str,
    ) -> Result<String, String> {
        let face = self.face(&values.dimensions)?;
        let mast = self.mast(&values.dimensions)?;
        let top = face.iter().map(|p| p.up).fold(0.0, f64::max);
        if let Some(m) = mast
            && m + 1e-9 < top
        {
            return Err(format!(
                "the shaft top ({m:.2} m) is below the highest attachment ({top:.2} m)"
            ));
        }
        if values.string_length.is_nan() || values.string_length <= 0.0 {
            return Err("the string length must be above 0".into());
        }
        if !self.earths.is_empty() && (values.earth_length.is_nan() || values.earth_length <= 0.0) {
            return Err("the earth wires' string length must be above 0".into());
        }
        if role == Role::Suspension {
            for p in &face {
                let l = if p.earth {
                    values.earth_length
                } else {
                    values.string_length
                };
                if p.up - l <= 0.0 {
                    return Err(format!(
                        "{}: its string ({l:.2} m) reaches the ground",
                        p.label
                    ));
                }
            }
        }
        let n = |v: f64| format!("\"{} m\"", round(v));
        let mut t = String::new();
        let _ = writeln!(t, "format = \"ohl-structure\"");
        let _ = writeln!(t, "version = 2");
        let _ = writeln!(t, "id = {}", quote(id));
        let _ = writeln!(t, "name = {}", quote(name));
        let _ = writeln!(t, "kind = \"{}\"", kind_name(self.kind));
        let _ = writeln!(t, "height = {}", n(mast.unwrap_or(top)));
        let _ = writeln!(
            t,
            "source = {}",
            quote(&format!(
                "structure template {}, {}",
                self.id,
                role.as_str()
            ))
        );
        let _ = writeln!(t);
        let _ = writeln!(t, "[template]");
        let _ = writeln!(t, "id = {}", quote(&self.id));
        let _ = writeln!(t, "role = \"{}\"", role.as_str());
        let _ = writeln!(t, "string_length = {}", n(values.string_length));
        let _ = writeln!(t, "string_weight = \"{} N\"", round(values.string_weight));
        if !self.earths.is_empty() {
            let _ = writeln!(t, "earth_string_length = {}", n(values.earth_length));
            let _ = writeln!(
                t,
                "earth_string_weight = \"{} N\"",
                round(values.earth_weight)
            );
        }
        let _ = writeln!(t, "[template.values]");
        for (k, v) in &values.dimensions {
            let _ = writeln!(t, "{k} = {}", n(*v));
        }
        let point = |t: &mut String, label: &str, p: &FacePoint| {
            let _ = writeln!(t, "\n[[point]]");
            let _ = writeln!(t, "label = {}", quote(label));
            let _ = writeln!(t, "point = [{}, \"0 m\", {}]", n(p.across), n(p.up));
        };
        let wire = |t: &mut String, label: &str, set: u32| {
            let _ = writeln!(t, "\n[[wire]]");
            let _ = writeln!(t, "label = {}", quote(label));
            let _ = writeln!(t, "set = {set}");
        };
        let string = |t: &mut String,
                      label: &str,
                      top: &str,
                      w: &str,
                      direction: Option<f64>,
                      earth: bool| {
            let (l, wt) = if earth {
                (values.earth_length, values.earth_weight)
            } else {
                (values.string_length, values.string_weight)
            };
            let _ = writeln!(t, "\n[[string]]");
            let _ = writeln!(t, "label = {}", quote(label));
            let _ = writeln!(t, "top = {}", quote(top));
            let _ = writeln!(t, "wire = {}", quote(w));
            let _ = writeln!(t, "length = {}", n(l));
            let _ = writeln!(t, "weight = \"{} N\"", round(wt));
            if let Some(d) = direction {
                let _ = writeln!(t, "strain = true");
                let _ = writeln!(t, "angle = \"90 deg\"");
                let _ = writeln!(t, "direction = \"{d} deg\"");
            }
        };
        // Phases and earth wires alike: a point on the pylon, a string
        // (the earth wires' short) to a wire point with the set.
        for p in &face {
            let a = format!("A-{}", p.label);
            point(&mut t, &a, p);
            match role {
                Role::Suspension => {
                    let w = format!("W-{}", p.label);
                    wire(&mut t, &w, p.slot);
                    string(&mut t, &format!("S-{}", p.label), &a, &w, None, p.earth);
                }
                Role::Strain => {
                    let pairs = [(1, 180.0, self.first_offset), (2, 0.0, self.second_offset)];
                    for (n, direction, offset) in pairs {
                        let w = format!("W-{}-{n}", p.label);
                        wire(&mut t, &w, p.slot + offset);
                        let label = format!("S-{}-{n}", p.label);
                        string(&mut t, &label, &a, &w, Some(direction), p.earth);
                    }
                }
            }
        }
        // Read back: a valid type of level 1.
        let ty = crate::structures::StructureType::parse(std::path::Path::new("built"), &t)
            .map_err(|e| e.message)?;
        if ty.detail() != crate::Detail::Stick {
            return Err("the type built is not of level 1".into());
        }
        Ok(t)
    }
}

/// The template a built type records: its id, role and values.
pub fn recorded(template: &toml::Table) -> Option<(String, Role, Values)> {
    let id = template.get("id")?.as_str()?.to_owned();
    let role = Role::parse(template.get("role")?.as_str()?)?;
    let length = |v: &toml::Value| v.as_str()?.parse::<Length>().ok().map(Length::si);
    let force = |v: &toml::Value| v.as_str()?.parse::<Force>().ok().map(Force::si);
    let dimensions = template
        .get("values")?
        .as_table()?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), length(v)?)))
        .collect();
    Some((
        id,
        role,
        Values {
            dimensions,
            string_length: length(template.get("string_length")?)?,
            string_weight: force(template.get("string_weight")?)?,
            // Built before earth wires had strings: none recorded (NaN:
            // the template's are used).
            earth_length: template
                .get("earth_string_length")
                .and_then(length)
                .unwrap_or(f64::NAN),
            earth_weight: template
                .get("earth_string_weight")
                .and_then(force)
                .unwrap_or(f64::NAN),
        },
    ))
}

fn kind_name(k: StructureKind) -> &'static str {
    match k {
        StructureKind::Lattice => "lattice",
        StructureKind::Pole => "pole",
        StructureKind::Portal => "portal",
        StructureKind::Other => "other",
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0 + 0.0
}

fn quote(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

/// Evaluates `text` (numbers, names of `values`, `+ - * /`, parentheses).
pub fn eval(text: &str, values: &BTreeMap<String, f64>) -> Result<f64, String> {
    let tokens = tokens(text)?;
    let mut p = Parser {
        tokens: &tokens,
        at: 0,
        values,
    };
    let v = p.sum()?;
    if p.at != tokens.len() {
        return Err("does not read to the end".into());
    }
    if !v.is_finite() {
        return Err("not a number (a division by 0?)".into());
    }
    Ok(v)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Op(char),
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let c = chars[k];
        if c.is_whitespace() {
            k += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = k;
            while k < chars.len() && (chars[k].is_ascii_digit() || chars[k] == '.') {
                k += 1;
            }
            let s: String = chars[start..k].iter().collect();
            out.push(Token::Number(
                s.parse().map_err(|_| format!("\"{s}\" is not a number"))?,
            ));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = k;
            while k < chars.len() && (chars[k].is_ascii_alphanumeric() || chars[k] == '_') {
                k += 1;
            }
            out.push(Token::Name(chars[start..k].iter().collect()));
        } else if "+-*/()".contains(c) {
            out.push(Token::Op(c));
            k += 1;
        } else {
            return Err(format!("\"{c}\" is not allowed"));
        }
    }
    if out.is_empty() {
        return Err("empty".into());
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    values: &'a BTreeMap<String, f64>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn sum(&mut self) -> Result<f64, String> {
        let mut v = self.product()?;
        while let Some(Token::Op(c @ ('+' | '-'))) = self.peek() {
            let c = *c;
            self.at += 1;
            let r = self.product()?;
            v = if c == '+' { v + r } else { v - r };
        }
        Ok(v)
    }

    fn product(&mut self) -> Result<f64, String> {
        let mut v = self.unary()?;
        while let Some(Token::Op(c @ ('*' | '/'))) = self.peek() {
            let c = *c;
            self.at += 1;
            let r = self.unary()?;
            v = if c == '*' { v * r } else { v / r };
        }
        Ok(v)
    }

    fn unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(Token::Op('-')) => {
                self.at += 1;
                Ok(-self.unary()?)
            }
            Some(Token::Op('+')) => {
                self.at += 1;
                self.unary()
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Result<f64, String> {
        let t = self.peek().cloned().ok_or("ends too early")?;
        self.at += 1;
        match t {
            Token::Number(v) => Ok(v),
            Token::Name(n) => self
                .values
                .get(&n)
                .copied()
                .ok_or_else(|| format!("no parameter {n}")),
            Token::Op('(') => {
                let v = self.sum()?;
                match self.peek() {
                    Some(Token::Op(')')) => {
                        self.at += 1;
                        Ok(v)
                    }
                    _ => Err("a ) is missing".into()),
                }
            }
            Token::Op(c) => Err(format!("\"{c}\" in the wrong place")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Library;
    use crate::structures::StructureType;

    fn shipped() -> Library<PylonTemplate> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/structure-templates");
        Library::load_dir(&dir).unwrap()
    }

    /// The format id `ohl-structure-template` (2026-10-21) and the one
    /// before, `ohl-pylon-template` (and its `ols` name), are read alike;
    /// another is refused.
    #[test]
    fn both_format_ids_are_read() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/structure-templates/delta-1c.ohlpt");
        let text = std::fs::read_to_string(&dir).unwrap();
        let body: String = text
            .lines()
            .filter(|l| !l.starts_with("format"))
            .map(|l| format!("{l}\n"))
            .collect();
        for id in [
            "ohl-structure-template",
            "ohl-pylon-template",
            "ols-pylon-template",
        ] {
            let t = format!("format = \"{id}\"\n{body}");
            PylonTemplate::parse(std::path::Path::new("x.ohlpt"), &t).unwrap();
        }
        let t = format!("format = \"ohl-structure\"\n{body}");
        assert!(PylonTemplate::parse(std::path::Path::new("x.ohlpt"), &t).is_err());
    }

    #[test]
    fn expressions() {
        let v: BTreeMap<String, f64> = [("a".to_owned(), 2.0), ("b_2".to_owned(), 5.0)].into();
        assert_eq!(eval("a + b_2 * 2", &v).unwrap(), 12.0);
        assert_eq!(eval("-(a + 1) / 2", &v).unwrap(), -1.5);
        assert_eq!(eval(" 3.5 ", &v).unwrap(), 3.5);
        assert!(eval("a +", &v).is_err());
        assert!(eval("c", &v).unwrap_err().contains("no parameter c"));
        assert!(eval("a / 0", &v).is_err());
        assert!(eval("2 m", &v).is_err());
    }

    #[test]
    fn every_shipped_template_builds_both_ways() {
        let lib = shipped();
        assert!(lib.len() >= 5, "{}", lib.len());
        for t in lib.iter() {
            for role in [Role::Suspension, Role::Strain] {
                let text = t.build(&t.defaults(role), role, "x", "x").unwrap();
                let ty = StructureType::parse(std::path::Path::new("x"), &text).unwrap();
                assert_eq!(ty.detail(), crate::Detail::Stick, "{}", t.id);
                // Every phase gets its strings: one, or two along the line.
                let phases = t.attachments.len() + t.earths.len();
                let per = if role == Role::Suspension { 1 } else { 2 };
                assert_eq!(ty.strings.len(), phases * per, "{}", t.id);
                // The face is flat: every point at y = 0.
                assert!(ty.points.iter().all(|p| p.position()[1].si() == 0.0));
                // Every attachment point on an attach joint of its own
                // (level 1: no members), no offset (2026-10-21).
                for p in &ty.points {
                    let (j, o) = ty.point_link(&p.label).unwrap();
                    assert_eq!(o, [0.0; 3], "{} {}", t.id, p.label);
                    assert_eq!(
                        ty.joint(j).unwrap().kind,
                        Some(crate::JointKind::Attach),
                        "{} {}",
                        t.id,
                        p.label
                    );
                }
                assert_eq!(ty.attach_joints().len(), ty.points.len(), "{}", t.id);
            }
        }
    }

    #[test]
    fn sets_follow_the_slots_back_and_ahead() {
        let lib = shipped();
        let t = lib.get("danube-2c").unwrap();
        let sets = |role| {
            let text = t.build(&t.defaults(role), role, "x", "x").unwrap();
            StructureType::parse(std::path::Path::new("x"), &text)
                .unwrap()
                .sets()
        };
        assert_eq!(sets(Role::Suspension), [1, 2, 3, 4, 5, 6, 19]);
        assert_eq!(
            sets(Role::Strain),
            [21, 22, 23, 24, 25, 26, 39, 41, 42, 43, 44, 45, 46, 59]
        );
        // The earth wire hangs on a short string from its own point.
        let text = t
            .build(&t.defaults(Role::Suspension), Role::Suspension, "x", "x")
            .unwrap();
        let ty = StructureType::parse(std::path::Path::new("x"), &text).unwrap();
        let e = ty.strings.iter().find(|s| s.label == "S-E1").unwrap();
        assert_eq!(e.top, "A-E1");
        assert!((e.length.unwrap().si() - 0.3).abs() < 1e-9);
        // Strain strings point back and ahead along the line.
        let text = t
            .build(&t.defaults(Role::Strain), Role::Strain, "x", "x")
            .unwrap();
        let ty = StructureType::parse(std::path::Path::new("x"), &text).unwrap();
        assert!(
            ty.strings
                .iter()
                .all(|s| s.strain && s.side.is_none() && s.angles().0 == 90.0)
        );
    }

    #[test]
    fn values_are_recorded_and_checked() {
        let lib = shipped();
        let t = lib.get("delta-1c").unwrap();
        let mut v = t.defaults(Role::Suspension);
        let name = t.parameters[0].name.clone();
        *v.dimensions.get_mut(&name).unwrap() += 2.5;
        let text = t.build(&v, Role::Suspension, "d", "D").unwrap();
        let table: toml::Table = text.parse().unwrap();
        let (id, role, back) = recorded(table["template"].as_table().unwrap()).unwrap();
        assert_eq!((id.as_str(), role), ("delta-1c", Role::Suspension));
        assert!((back.dimensions[&name] - v.dimensions[&name]).abs() < 1e-9);
        // A string reaching the ground, a mast below an arm: refused.
        let mut low = v.clone();
        low.string_length = 500.0;
        assert!(t.build(&low, Role::Suspension, "d", "D").is_err());
        let mut bad = v;
        for x in bad.dimensions.values_mut() {
            *x = -1.0;
        }
        assert!(t.build(&bad, Role::Suspension, "d", "D").is_err());
    }
}
