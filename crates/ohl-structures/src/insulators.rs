// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Insulator assembly library (`*.ohli`, one assembly per file).
//!
//! Basic kinds (DESIGN_GOALS §11):
//!
//! - `suspension`: an I-string hanging from its attachment point;
//! - `v_string`: two legs from points at ±`half_width` (transverse) to one
//!   clamp below; legs carry tension only;
//! - `strain`: a dead-end assembly along the line from the attachment point
//!   to the clamp.
//!
//! General linked assemblies (yokes, jumper support strings…) will extend
//! this as a graph of nodes and elements (DESIGN_GOALS §6).
//!
//! ```toml
//! format = "ohl-insulator"
//! version = 1
//! id = "I-380-double"
//! name = "380 kV double I-string"
//! kind = "suspension"
//! length = "5.2 m"
//! weight = "1.4 kN"
//! ```
//!
//! **Limits** (version 2, optional, [`InsulatorLimits`]): the tension the
//! assembly is rated for (`max_load = "120 kN"`; a V-string: each leg's)
//! and the swing angles it may take (`max_swing = "60 deg"` from straight
//! down in any direction, `max_swing_across` / `max_swing_along`
//! projected across / along the line). OpenOHL checks none of them: absent,
//! not given; given, a string naming the assembly takes them where it
//! gives none of its own, for a plugin or a report to compare with the
//! solved forces and swings.

use ohl_core::units::{Angle, Area, Force, Length};
use serde::Deserialize;

/// File extension of insulator assemblies.
pub const INSULATOR_FILE_EXTENSION: &str = "ohli";
/// Format identifier.
pub const INSULATOR_FILE_FORMAT: &str = "ohl-insulator";
/// Newest format version read: 2 adds the optional limits
/// (`max_load`, `max_swing`, `max_swing_across`, `max_swing_along`,
/// [`InsulatorLimits`]); version 1 files read as before.
pub const INSULATOR_FILE_VERSION: u32 = 2;

/// An insulator's axial stiffness EA when it gives none, N (the line's
/// default too: `ohl_line::solve::DEFAULT_INSULATOR_STIFFNESS`).
pub const DEFAULT_AXIAL_STIFFNESS: f64 = 5e7;

fn default_stiffness() -> Force {
    Force::from_newtons(DEFAULT_AXIAL_STIFFNESS)
}

/// Kind of assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyKind {
    /// I-string.
    Suspension,
    /// Two-leg V-string.
    VString,
    /// Dead-end (strain) assembly.
    Strain,
}

/// An insulator assembly.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsulatorAssembly {
    /// `"ohl-insulator"` (optional header).
    #[serde(default)]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default)]
    pub version: Option<u32>,
    /// Unique id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Kind.
    pub kind: AssemblyKind,
    /// Where the data comes from.
    #[serde(default)]
    pub source: Option<String>,
    /// Length from attachment point to clamp (suspension, strain), or leg
    /// length (V-string).
    pub length: Length,
    /// Total weight of the assembly.
    #[serde(default)]
    pub weight: Force,
    /// Axial stiffness of a string / leg.
    #[serde(default = "default_stiffness")]
    pub axial_stiffness: Force,
    /// Projected area for wind load.
    #[serde(default)]
    pub wind_area: Area,
    /// V-string: transverse distance from the centre to each leg attachment.
    #[serde(default)]
    pub half_width: Option<Length>,
    /// The tension it is rated for (a V-string: each leg's); not checked
    /// by OpenOHL ([`InsulatorLimits`]).
    #[serde(default)]
    pub max_load: Option<Force>,
    /// The swing it may take from straight down, any direction (not
    /// checked).
    #[serde(default)]
    pub max_swing: Option<Angle>,
    /// The swing it may take across the line, either side (not checked).
    #[serde(default)]
    pub max_swing_across: Option<Angle>,
    /// The swing it may take along the line, either way (not checked).
    #[serde(default)]
    pub max_swing_along: Option<Angle>,
}

/// The optional limits of an insulator assembly or a structure type's
/// string, in SI units (N, degrees) — the same four a line's string
/// carries (`ohl_line::StringLimits`). **Not checked by OpenOHL**:
/// absent, not given; given, available to plugins and reports.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InsulatorLimits {
    /// The tension it is rated for, N.
    pub max_load: Option<f64>,
    /// The swing from straight down, any direction, degrees.
    pub max_swing_deg: Option<f64>,
    /// The swing across the line, either side, degrees.
    pub max_swing_across_deg: Option<f64>,
    /// The swing along the line, either way, degrees.
    pub max_swing_along_deg: Option<f64>,
}

impl InsulatorLimits {
    /// None given.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// From the fields as read.
    pub fn of(
        max_load: Option<Force>,
        max_swing: Option<Angle>,
        max_swing_across: Option<Angle>,
        max_swing_along: Option<Angle>,
    ) -> Self {
        // Degrees as written ("60 deg" is 60, not 59.999…): to 1e-9°.
        let deg = |a: Option<Angle>| a.map(|a| (a.si().to_degrees() * 1e9).round() / 1e9);
        InsulatorLimits {
            max_load: max_load.map(|f| f.si()),
            max_swing_deg: deg(max_swing),
            max_swing_across_deg: deg(max_swing_across),
            max_swing_along_deg: deg(max_swing_along),
        }
    }

    /// These, where given, else `other`'s.
    pub fn or(self, other: InsulatorLimits) -> Self {
        InsulatorLimits {
            max_load: self.max_load.or(other.max_load),
            max_swing_deg: self.max_swing_deg.or(other.max_swing_deg),
            max_swing_across_deg: self.max_swing_across_deg.or(other.max_swing_across_deg),
            max_swing_along_deg: self.max_swing_along_deg.or(other.max_swing_along_deg),
        }
    }

    /// Checks the values given: a load > 0, angles from 0 to 180°.
    pub fn check(&self) -> Result<(), String> {
        if self.max_load.is_some_and(|l| !(l.is_finite() && l > 0.0)) {
            return Err("max_load must be > 0".into());
        }
        for (name, a) in [
            ("max_swing", self.max_swing_deg),
            ("max_swing_across", self.max_swing_across_deg),
            ("max_swing_along", self.max_swing_along_deg),
        ] {
            if a.is_some_and(|a| !(0.0..=180.0).contains(&a)) {
                return Err(format!("{name} must be from 0 to 180°"));
            }
        }
        Ok(())
    }

    /// The `key = "value unit"` lines of the limits given (TOML).
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        if let Some(l) = self.max_load {
            out += &format!("max_load = \"{l} N\"\n");
        }
        for (key, a) in [
            ("max_swing", self.max_swing_deg),
            ("max_swing_across", self.max_swing_across_deg),
            ("max_swing_along", self.max_swing_along_deg),
        ] {
            if let Some(a) = a {
                out += &format!("{key} = \"{a} deg\"\n");
            }
        }
        out
    }
}

impl InsulatorAssembly {
    /// Its limits (not checked by OpenOHL).
    pub fn limits(&self) -> InsulatorLimits {
        InsulatorLimits::of(
            self.max_load,
            self.max_swing,
            self.max_swing_across,
            self.max_swing_along,
        )
    }

    /// Checks plausibility.
    pub fn validate(&self) -> Result<(), String> {
        ohl_core::files::check_header(
            self.format.as_deref(),
            self.version,
            INSULATOR_FILE_FORMAT,
            INSULATOR_FILE_VERSION,
            "insulator",
        )?;
        if !(self.length.si() > 0.0 && self.length.si() < 50.0) {
            return Err(format!("length {} must be between 0 and 50 m", self.length));
        }
        if self.weight.si() < 0.0 || self.axial_stiffness.si() <= 0.0 {
            return Err("weight must be ≥ 0 and axial stiffness > 0".into());
        }
        self.limits().check()?;
        match (self.kind, self.half_width) {
            (AssemblyKind::VString, None) => Err("a v_string needs half_width".into()),
            (AssemblyKind::VString, Some(hw)) if !(hw.si() > 0.0 && hw < self.length) => {
                Err("half_width must be positive and shorter than the leg length".into())
            }
            (AssemblyKind::Suspension | AssemblyKind::Strain, Some(_)) => {
                Err("half_width is only for v_string assemblies".into())
            }
            _ => Ok(()),
        }
    }

    /// The assembly as an `.ohli` file (what [`Library::load_dir`] reads
    /// back), values in SI units; version 1 unless it gives limits.
    ///
    /// [`Library::load_dir`]: crate::Library::load_dir
    pub fn to_toml(&self) -> String {
        let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let kind = match self.kind {
            AssemblyKind::Suspension => "suspension",
            AssemblyKind::VString => "v_string",
            AssemblyKind::Strain => "strain",
        };
        // Version 1 unless it gives limits (an older program reads it).
        let version = if self.limits().is_empty() {
            1
        } else {
            INSULATOR_FILE_VERSION
        };
        let mut out = format!(
            "format = \"{INSULATOR_FILE_FORMAT}\"\nversion = {version}\nid = {}\nname = {}\nkind = \"{kind}\"\n",
            quote(&self.id),
            quote(&self.name)
        );
        if let Some(s) = &self.source {
            out += &format!("source = {}\n", quote(s));
        }
        out += &format!(
            "length = \"{} m\"\nweight = \"{} N\"\naxial_stiffness = \"{} N\"\nwind_area = \"{} m²\"\n",
            self.length.si(),
            self.weight.si(),
            self.axial_stiffness.si(),
            self.wind_area.si()
        );
        if let Some(h) = self.half_width {
            out += &format!("half_width = \"{} m\"\n", h.si());
        }
        out += &self.limits().to_toml();
        out
    }

    /// Vertical drop from the attachment point to the clamp in still air,
    /// m: the string length for suspension, the leg depth for V-strings, 0
    /// for strain assemblies — whose direction and droop are not properties
    /// of the assembly: they follow the conductor tension (a strain string
    /// on a steep down-lead hangs 47° below horizontal in real data), so the
    /// solver models them as free members of their length.
    pub fn vertical_drop(&self) -> f64 {
        match (self.kind, self.half_width) {
            (AssemblyKind::VString, Some(hw)) => {
                (self.length.si().powi(2) - hw.si().powi(2)).sqrt()
            }
            (AssemblyKind::Strain, _) => 0.0,
            _ => self.length.si(),
        }
    }
}

/// A set of insulator assemblies, by id.
pub type InsulatorLibrary = crate::Library<InsulatorAssembly>;

impl crate::LibraryItem for InsulatorAssembly {
    const EXTENSION: &'static str = INSULATOR_FILE_EXTENSION;
    const KIND: &'static str = "insulator assembly";

    fn id(&self) -> &str {
        &self.id
    }

    fn check(&self) -> Result<(), String> {
        self.validate()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_assembly_is_written_and_read_back() {
        let a = InsulatorAssembly {
            format: None,
            version: None,
            id: "I \"3.5\"".into(),
            name: "I 3.500 m".into(),
            kind: AssemblyKind::VString,
            source: Some("model".into()),
            length: Length::from_meters(3.5),
            weight: Force::from_newtons(900.0),
            axial_stiffness: Force::from_newtons(4e7),
            wind_area: Area::from_square_meters(0.25),
            half_width: Some(Length::from_meters(1.5)),
            max_load: None,
            max_swing: None,
            max_swing_across: None,
            max_swing_along: None,
        };
        let b: InsulatorAssembly = toml::from_str(&a.to_toml()).unwrap();
        assert_eq!(b.id, a.id);
        assert_eq!((b.kind, b.source.clone()), (a.kind, a.source.clone()));
        for (x, y) in [
            (b.length.si(), 3.5),
            (b.weight.si(), 900.0),
            (b.axial_stiffness.si(), 4e7),
            (b.wind_area.si(), 0.25),
            (b.half_width.unwrap().si(), 1.5),
        ] {
            assert!((x - y).abs() < 1e-9, "{x} {y}");
        }
        b.validate().unwrap();
    }

    use super::*;
    use crate::{LibraryError, LibraryItem};
    use std::path::Path;

    fn parse(text: &str) -> Result<InsulatorAssembly, LibraryError> {
        InsulatorAssembly::parse(Path::new("t.ohli"), text)
    }

    #[test]
    fn suspension_and_v_string() {
        let i = parse(
            "id = \"I1\"\nname = \"I\"\nkind = \"suspension\"\nlength = \"5.2 m\"\nweight = \"1.4 kN\"",
        )
        .unwrap();
        assert_eq!(i.vertical_drop(), 5.2);
        assert_eq!(i.axial_stiffness, Force::from_newtons(5e7));
        let v = parse(
            "id = \"V1\"\nname = \"V\"\nkind = \"v_string\"\nlength = \"5 m\"\nhalf_width = \"3 m\"",
        )
        .unwrap();
        assert!((v.vertical_drop() - 4.0).abs() < 1e-12);
    }

    /// The limits (not checked by OpenOHL): absent stays absent and the
    /// file written stays version 1; given they survive the round trip in
    /// version 2; bad values and unknown keys refused; a newer version
    /// refused as newer.
    #[test]
    fn limits_are_optional_and_round_trip() {
        let base = "id = \"I1\"\nname = \"I\"\nkind = \"suspension\"\nlength = \"5 m\"\n";
        let plain = parse(base).unwrap();
        assert!(plain.limits().is_empty());
        let text = plain.to_toml();
        assert!(
            text.contains("version = 1") && !text.contains("max_"),
            "{text}"
        );
        assert!(parse(&text).unwrap().limits().is_empty());
        let given = parse(&format!(
            "{base}max_load = \"120 kN\"\nmax_swing = \"60 deg\"\nmax_swing_across = \"55 deg\"\nmax_swing_along = \"0.5 rad\"\n"
        ))
        .unwrap();
        let l = given.limits();
        assert_eq!(l.max_load, Some(120_000.0));
        assert!((l.max_swing_deg.unwrap() - 60.0).abs() < 1e-9);
        assert!((l.max_swing_across_deg.unwrap() - 55.0).abs() < 1e-9);
        assert!((l.max_swing_along_deg.unwrap() - 0.5f64.to_degrees()).abs() < 1e-9);
        let text = given.to_toml();
        assert!(text.contains("version = 2"), "{text}");
        let back = parse(&text).unwrap().limits();
        for (a, b) in [
            (back.max_load, l.max_load),
            (back.max_swing_deg, l.max_swing_deg),
            (back.max_swing_across_deg, l.max_swing_across_deg),
            (back.max_swing_along_deg, l.max_swing_along_deg),
        ] {
            assert!((a.unwrap() - b.unwrap()).abs() < 1e-9, "{a:?} {b:?}");
        }
        // Only one given: the others stay absent.
        let one = parse(&format!("{base}max_swing_along = \"30 deg\"\n")).unwrap();
        assert_eq!(one.limits().max_load, None);
        assert_eq!(one.limits().max_swing_deg, None);
        for bad in [
            "max_load = \"0 N\"",
            "max_load = \"-5 kN\"",
            "max_swing = \"200 deg\"",
            "max_swing_across = \"-1 deg\"",
            "max_swing_sideways = \"10 deg\"",
            "max_load = \"10 m\"",
        ] {
            assert!(parse(&format!("{base}{bad}\n")).is_err(), "{bad}");
        }
        let e = parse(&format!("format = \"ohl-insulator\"\nversion = 3\n{base}")).unwrap_err();
        assert!(e.to_string().contains("newer"), "{e}");
    }

    #[test]
    fn rejects_inconsistent_data() {
        assert!(parse("id = \"V\"\nname = \"V\"\nkind = \"v_string\"\nlength = \"5 m\"").is_err());
        assert!(
            parse("id = \"I\"\nname = \"I\"\nkind = \"suspension\"\nlength = \"500 m\"").is_err()
        );
        assert!(parse("id = \"I\"\nname = \"I\"\nkind = \"suspension\"\nlength = \"5 m\"\nhalf_width = \"1 m\"").is_err());
        assert!(parse("id = \"I\"\nname = \"I\"\nkind = \"post\"\nlength = \"5 m\"").is_err());
    }
}
