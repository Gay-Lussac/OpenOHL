// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Insulator assembly library (`*.olsi`, one assembly per file).
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
//! format = "ols-insulator"
//! version = 1
//! id = "I-380-double"
//! name = "380 kV double I-string"
//! kind = "suspension"
//! length = "5.2 m"
//! weight = "1.4 kN"
//! ```

use ols_core::units::{Area, Force, Length};
use serde::Deserialize;

/// File extension of insulator assemblies.
pub const INSULATOR_FILE_EXTENSION: &str = "olsi";
/// Format identifier.
pub const INSULATOR_FILE_FORMAT: &str = "ols-insulator";

fn default_stiffness() -> Force {
    Force::from_newtons(5e7)
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
    /// `"ols-insulator"` (optional header).
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
}

impl InsulatorAssembly {
    /// Checks plausibility.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(f) = self
            .format
            .as_deref()
            .filter(|f| *f != INSULATOR_FILE_FORMAT)
        {
            return Err(format!("format \"{f}\" is not \"{INSULATOR_FILE_FORMAT}\""));
        }
        if self.version.is_some_and(|v| v > 1) {
            return Err("insulator format version newer than supported (1)".into());
        }
        if !(self.length.si() > 0.0 && self.length.si() < 50.0) {
            return Err(format!("length {} must be between 0 and 50 m", self.length));
        }
        if self.weight.si() < 0.0 || self.axial_stiffness.si() <= 0.0 {
            return Err("weight must be ≥ 0 and axial stiffness > 0".into());
        }
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
    use super::*;
    use crate::{LibraryError, LibraryItem};
    use std::path::Path;

    fn parse(text: &str) -> Result<InsulatorAssembly, LibraryError> {
        InsulatorAssembly::parse(Path::new("t.olsi"), text)
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
