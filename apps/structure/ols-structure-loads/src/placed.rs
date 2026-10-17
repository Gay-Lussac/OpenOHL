// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A placed structure's values, handed from the line application to the
//! structure application: the line-values file (JSON) and where the files
//! of a placed structure lie beside its structure file.
//!
//! ```json
//! {
//!   "format": "ols-structure-line-values",
//!   "version": 2,
//!   "structure": "T2",
//!   "line": "synthetic-line-typed",
//!   "structure_type": "type-02",
//!   "cases": ["everyday", "wind", "weather: glaze ice"],
//!   "sources": ["load_case", "load_case", "weather_case"],
//!   "values": { "points": [...], "values": [...] },
//!   "others": [{ "structure": "T5", "values": {...} }],
//!   "note": null
//! }
//! ```
//!
//! Version 2 added `sources` (the source of each case, in the order of
//! `cases`); a version 1 file has none: every case a load case.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::recipe::LineQuantity;
use crate::table::LineValues;

/// `format` of a line-values file.
pub const LINE_VALUES_FILE_FORMAT: &str = "ols-structure-line-values";
/// `version` of a line-values file (2: `sources`; version 1 files are
/// read too).
pub const LINE_VALUES_FILE_VERSION: u32 = 2;

/// Where a case of the line comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSource {
    /// A load case of the line (`.olslc`, from a file or a design code):
    /// a weather case, a wire condition and factors.
    #[default]
    LoadCase,
    /// A weather case of the line on its own, all factors 1 (Criteria ›
    /// Loads for structures in the line application).
    WeatherCase,
}

impl CaseSource {
    /// In words, for tables.
    pub fn label(self) -> &'static str {
        match self {
            Self::LoadCase => "load case",
            Self::WeatherCase => "weather case",
        }
    }
}

/// Another placed structure using the same structure file, with its
/// values (for the envelope over the places, a reporting option only).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtherPlace {
    /// The placed structure's id.
    pub structure: String,
    /// Its values.
    #[serde(default)]
    pub values: LineValues,
}

/// The values the line gives at one placed structure, as a file: which
/// structure, in which line, the line's load cases and the values; the
/// other places of the same structure file; why there are none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedValues {
    /// [`LINE_VALUES_FILE_FORMAT`].
    pub format: String,
    /// [`LINE_VALUES_FILE_VERSION`].
    pub version: u32,
    /// The placed structure's id.
    pub structure: String,
    /// The line (project line, or the line file's name).
    #[serde(default)]
    pub line: String,
    /// The structure type (file) it uses.
    #[serde(default)]
    pub structure_type: String,
    /// The line's cases, in order: its load cases, then the weather
    /// cases sent on their own.
    #[serde(default)]
    pub cases: Vec<String>,
    /// Where each of `cases` comes from, in their order (version 2;
    /// missing ones: a load case).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<CaseSource>,
    /// The values at this placed structure.
    #[serde(default)]
    pub values: LineValues,
    /// The other places of the same structure file.
    #[serde(default)]
    pub others: Vec<OtherPlace>,
    /// Why there are no values (yet): being computed, no load case file, …
    #[serde(default)]
    pub note: Option<String>,
}

impl PlacedValues {
    /// No values yet, saying why.
    pub fn pending(structure: &str, line: &str, structure_type: &str, note: &str) -> Self {
        Self {
            format: LINE_VALUES_FILE_FORMAT.into(),
            version: LINE_VALUES_FILE_VERSION,
            structure: structure.into(),
            line: line.into(),
            structure_type: structure_type.into(),
            cases: Vec::new(),
            sources: Vec::new(),
            values: LineValues::default(),
            others: Vec::new(),
            note: Some(note.into()),
        }
    }

    /// Where case `case` comes from (a case not listed, or of a version 1
    /// file: a load case).
    pub fn source(&self, case: &str) -> CaseSource {
        self.cases
            .iter()
            .position(|c| c == case)
            .and_then(|k| self.sources.get(k).copied())
            .unwrap_or_default()
    }

    /// Reads a file's text (its format and version checked).
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if v.format != LINE_VALUES_FILE_FORMAT {
            return Err(format!(
                "format \"{}\": not a line-values file ({LINE_VALUES_FILE_FORMAT})",
                v.format
            ));
        }
        if v.version > LINE_VALUES_FILE_VERSION {
            return Err(format!(
                "version {} is newer than this program reads ({LINE_VALUES_FILE_VERSION})",
                v.version
            ));
        }
        Ok(v)
    }

    /// Reads a file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The file's text.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("plain data")
    }

    /// Writes the file (through a file beside it, so a reader never sees
    /// half of it).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("json.part");
        std::fs::write(&tmp, self.to_json()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// The largest factored value of one direction at a point over the line's
/// cases.
#[derive(Debug, Clone, PartialEq)]
pub struct Governing {
    /// The point.
    pub point: String,
    /// Per direction (transverse, longitudinal, vertical down): the value
    /// largest in size, N, and its case.
    pub largest: [Option<(f64, String)>; 3],
}

impl LineValues {
    /// Per point, the factored value largest in size per direction over
    /// the cases, with its case.
    pub fn governing(&self) -> Vec<Governing> {
        let quantities = [
            LineQuantity::TransverseFactored,
            LineQuantity::LongitudinalFactored,
            LineQuantity::VerticalFactored,
        ];
        self.points
            .iter()
            .map(|p| {
                let largest = quantities.map(|q| {
                    self.values
                        .iter()
                        .filter(|v| v.quantity == q && v.point.as_deref() == Some(&p.label))
                        .max_by(|a, b| a.value.abs().total_cmp(&b.value.abs()))
                        .map(|v| (v.value, v.case.clone()))
                });
                Governing {
                    point: p.label.clone(),
                    largest,
                }
            })
            .collect()
    }
}

/// A placed structure's id as a part of a file name: letters, digits, `-`
/// and `_` kept, anything else `_`.
pub fn file_part(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn beside(structure_file: &Path, suffix: &str) -> PathBuf {
    let stem = structure_file
        .file_stem()
        .map_or("structure".into(), |s| s.to_string_lossy().into_owned());
    structure_file.with_file_name(format!("{stem}{suffix}"))
}

/// Where the values typed over the load table are kept, beside the
/// structure file: `<stem>.loads.csv` for the structure on its own,
/// `<stem>.<placed structure>.loads.csv` for one placed structure.
pub fn overrides_path(structure_file: &Path, placed: Option<&str>) -> PathBuf {
    match placed {
        None => beside(structure_file, ".loads.csv"),
        Some(id) => beside(structure_file, &format!(".{}.loads.csv", file_part(id))),
    }
}

/// Where the line application writes a placed structure's line values,
/// beside the structure file: `<stem>.<placed structure>.line-values.json`.
pub fn line_values_path(structure_file: &Path, placed: &str) -> PathBuf {
    beside(
        structure_file,
        &format!(".{}.line-values.json", file_part(placed)),
    )
}
