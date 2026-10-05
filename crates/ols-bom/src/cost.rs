// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Unit costs (`*.olscost`, TOML): the user's own prices, in one currency
//! (a label only; nothing is converted).
//!
//! ```toml
//! format = "ols-cost"
//! version = 1
//! name = "Example unit costs (invented)"
//! currency = "EUR"
//!
//! [labour]
//! per_structure = 2500.0   # erecting a structure (unless its type says)
//! per_span = 1800.0        # stringing, per span between two structures
//!
//! [[structure]]
//! type = "type-02"
//! price = 38000.0          # supply, without extension
//! foundation = 9000.0      # per structure (optional)
//! erection = 4000.0        # replaces labour.per_structure (optional)
//! extensions = { "+3" = 41000.0 }   # price with each extension
//!
//! [[insulator]]
//! id = "I-220-single"      # insulator assembly id, or an inline string's
//! price = 650.0            # name as the bill of materials lists it
//!
//! [[cable]]
//! id = "acsr-drake-26-7"
//! price_per_m = 7.5
//! stringing_per_m = 1.2    # optional
//!
//! [hardware]
//! spacer_interval = "60 m" # bundles: a spacer every … along each span
//! spacer_price = 45.0
//! dampers_per_span = 2     # per subconductor and span
//! damper_price = 60.0
//!
//! [[fixed]]
//! item = "Site establishment"
//! price = 25000.0
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use ols_core::units::Length;
use serde::Deserialize;

/// `format` of a cost file.
pub const COST_FILE_FORMAT: &str = "ols-cost";
/// Newest format version read.
pub const COST_FILE_VERSION: u32 = 1;
/// File extension of cost files.
pub const COST_FILE_EXTENSION: &str = "olscost";

/// Unit costs of one structure type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructurePrice {
    /// Structure type id.
    #[serde(rename = "type")]
    pub structure_type: String,
    /// Supply price without extension (none: only its extensions are
    /// priced).
    #[serde(default)]
    pub price: Option<f64>,
    /// Supply price with each extension, by extension name.
    #[serde(default)]
    pub extensions: BTreeMap<String, f64>,
    /// Foundation, per structure.
    #[serde(default)]
    pub foundation: Option<f64>,
    /// Erection labour, per structure (replaces `labour.per_structure`).
    #[serde(default)]
    pub erection: Option<f64>,
}

/// Price of one insulator assembly.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsulatorPrice {
    /// Insulator assembly id (or an inline string's name in the bill of
    /// materials).
    pub id: String,
    /// Price per assembly.
    pub price: f64,
}

/// Prices of one cable.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CablePrice {
    /// Cable id in the wire library.
    pub id: String,
    /// Supply per metre.
    pub price_per_m: f64,
    /// Stringing labour per metre (optional).
    #[serde(default)]
    pub stringing_per_m: Option<f64>,
}

/// Labour prices that apply to every structure and span.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labour {
    /// Erecting one structure (a type's `erection` replaces it).
    #[serde(default)]
    pub per_structure: Option<f64>,
    /// Stringing one span between two structures (all its wires).
    #[serde(default)]
    pub per_span: Option<f64>,
}

/// Hardware counted by rule.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hardware {
    /// Bundles: one spacer every this distance along each span (none: no
    /// spacers counted).
    #[serde(default)]
    pub spacer_interval: Option<Length>,
    /// Price per spacer.
    #[serde(default)]
    pub spacer_price: Option<f64>,
    /// Vibration dampers per subconductor and span (0: none counted).
    #[serde(default)]
    pub dampers_per_span: u32,
    /// Price per damper.
    #[serde(default)]
    pub damper_price: Option<f64>,
}

/// A fixed item of the estimate (site establishment, design, …).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedItem {
    /// What it is.
    pub item: String,
    /// Its price.
    pub price: f64,
}

/// A cost file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostData {
    /// `"ols-cost"` (optional header).
    #[serde(default)]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default)]
    pub version: Option<u32>,
    /// Name.
    #[serde(default)]
    pub name: String,
    /// Currency label, e.g. `"EUR"`.
    #[serde(default)]
    pub currency: String,
    /// Labour prices.
    #[serde(default)]
    pub labour: Labour,
    /// Structure types.
    #[serde(rename = "structure", default)]
    pub structures: Vec<StructurePrice>,
    /// Insulator assemblies.
    #[serde(rename = "insulator", default)]
    pub insulators: Vec<InsulatorPrice>,
    /// Cables.
    #[serde(rename = "cable", default)]
    pub cables: Vec<CablePrice>,
    /// Hardware rules and prices.
    #[serde(default)]
    pub hardware: Hardware,
    /// Fixed items.
    #[serde(rename = "fixed", default)]
    pub fixed: Vec<FixedItem>,
}

impl CostData {
    /// Parses and checks a cost file's text.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let data: CostData = toml::from_str(text).map_err(|e| e.to_string())?;
        data.check()?;
        Ok(data)
    }

    /// Reads a cost file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_toml(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn check(&self) -> Result<(), String> {
        ols_core::files::check_header(
            self.format.as_deref(),
            self.version,
            COST_FILE_FORMAT,
            COST_FILE_VERSION,
            "cost",
        )?;
        let price = |what: String, p: f64| {
            if p.is_finite() && p >= 0.0 {
                Ok(())
            } else {
                Err(format!("{what}: price must be a number ≥ 0"))
            }
        };
        let mut seen = std::collections::BTreeSet::new();
        for s in &self.structures {
            if !seen.insert(s.structure_type.as_str()) {
                return Err(format!("structure type {} priced twice", s.structure_type));
            }
            for p in [s.price, s.foundation, s.erection].into_iter().flatten() {
                price(format!("structure type {}", s.structure_type), p)?;
            }
            for (e, p) in &s.extensions {
                price(
                    format!("structure type {} extension {e}", s.structure_type),
                    *p,
                )?;
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for i in &self.insulators {
            if !seen.insert(i.id.as_str()) {
                return Err(format!("insulator {} priced twice", i.id));
            }
            price(format!("insulator {}", i.id), i.price)?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for c in &self.cables {
            if !seen.insert(c.id.as_str()) {
                return Err(format!("cable {} priced twice", c.id));
            }
            price(format!("cable {}", c.id), c.price_per_m)?;
            if let Some(p) = c.stringing_per_m {
                price(format!("cable {} stringing", c.id), p)?;
            }
        }
        for p in [self.labour.per_structure, self.labour.per_span]
            .into_iter()
            .flatten()
        {
            price("labour".into(), p)?;
        }
        for p in [self.hardware.spacer_price, self.hardware.damper_price]
            .into_iter()
            .flatten()
        {
            price("hardware".into(), p)?;
        }
        if self
            .hardware
            .spacer_interval
            .is_some_and(|i| i.si().is_nan() || i.si() <= 0.0)
        {
            return Err("hardware.spacer_interval must be above 0".into());
        }
        for f in &self.fixed {
            price(format!("fixed item {}", f.item), f.price)?;
        }
        Ok(())
    }

    /// The prices of a structure type.
    pub fn structure(&self, structure_type: &str) -> Option<&StructurePrice> {
        self.structures
            .iter()
            .find(|s| s.structure_type == structure_type)
    }

    /// Supply price of a structure type with an extension (none: the
    /// type's base price).
    pub fn structure_supply(&self, structure_type: &str, extension: Option<&str>) -> Option<f64> {
        let s = self.structure(structure_type)?;
        match extension {
            None => s.price,
            Some(e) => s.extensions.get(e).copied(),
        }
    }

    /// Everything one structure of a type costs: supply, foundation and
    /// erection (none without a supply price).
    pub fn structure_total(&self, structure_type: &str, extension: Option<&str>) -> Option<f64> {
        let supply = self.structure_supply(structure_type, extension)?;
        let s = self.structure(structure_type)?;
        Some(
            supply
                + s.foundation.unwrap_or(0.0)
                + s.erection.or(self.labour.per_structure).unwrap_or(0.0),
        )
    }

    /// The price of an insulator assembly.
    pub fn insulator(&self, id: &str) -> Option<f64> {
        self.insulators.iter().find(|i| i.id == id).map(|i| i.price)
    }

    /// The prices of a cable.
    pub fn cable(&self, id: &str) -> Option<&CablePrice> {
        self.cables.iter().find(|c| c.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_documented_example() {
        let doc = include_str!("cost.rs")
            .lines()
            .skip_while(|l| !l.starts_with("//! ```toml"))
            .skip(1)
            .take_while(|l| !l.starts_with("//! ```"))
            .map(|l| l.trim_start_matches("//!").trim_start())
            .collect::<Vec<_>>()
            .join("\n");
        let c = CostData::from_toml(&doc).unwrap();
        assert_eq!(c.currency, "EUR");
        assert_eq!(c.structure_supply("type-02", None), Some(38000.0));
        assert_eq!(c.structure_supply("type-02", Some("+3")), Some(41000.0));
        assert_eq!(c.structure_total("type-02", None), Some(51000.0));
        assert_eq!(c.hardware.spacer_interval.unwrap().si(), 60.0);
        assert_eq!(c.cable("acsr-drake-26-7").unwrap().price_per_m, 7.5);
    }

    #[test]
    fn rejects_bad_prices_and_duplicates() {
        assert!(CostData::from_toml("[[cable]]\nid = \"a\"\nprice_per_m = -1.0").is_err());
        assert!(
            CostData::from_toml(
                "[[insulator]]\nid = \"a\"\nprice = 1.0\n[[insulator]]\nid = \"a\"\nprice = 2.0"
            )
            .is_err()
        );
        assert!(CostData::from_toml("format = \"ols-line\"").is_err());
        assert!(CostData::from_toml("unknown = 1").is_err());
    }
}
