// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Load-case recipes of a structure and its load table (the structure
//! application; specification: `docs/development/structure-building.md`
//! §2).
//!
//! A structure's **load case** ([`LoadCaseRecipe`]) is a list of loads
//! ([`LoadRecipe`]), each saying **what** (a value the line gives at the
//! placed structure, [`LineQuantity`], or a value of its own), **how** (a
//! factor, a one-sided share, a direction in the structure's axes) and
//! **where** (points by label, set, phase or side, or members with a
//! pressure on their area). Kept in the structure file as
//! `[[load_case]]`, or on their own in a structure load-case file
//! (`.ohlslc`) for export and import:
//!
//! ```toml
//! format = "ohl-structure-load-cases"
//! version = 1
//!
//! [[load_case]]
//! name = "unbalanced"
//! description = "40 % of the largest tension, ahead"
//! line_case = "everyday"      # the line's load case giving the values
//! strength = 1.0
//! self_weight = 1.0
//!
//! [[load_case.load]]
//! what = "vertical"
//! points = "all"
//!
//! [[load_case.load]]
//! what = "tension_max"
//! one_sided = "40 %"
//! side = "ahead"
//! sets = [1]
//!
//! [[load_case.load]]
//! value = "2 kN"              # a value of the case itself
//! direction = "x"             # structure axes: x right, y ahead, z up
//! points = ["A1"]
//!
//! [[load_case.load]]
//! pressure = "0.6 kPa"        # on the members' area
//! direction = "-x"
//! members = "all"
//!
//! [[load_case.load]]
//! what = "structure_wind_factored"  # the line case's wind on the body
//! members = "all"                   # (version 2), as the line's checks
//! ```
//!
//! The wind on the structure's own body comes from the line case as
//! a pressure ([`CaseWind`], [`LineValues::structure_wind`]): the case's
//! own values for the structures (`[case.structure]` of the `.ohllc`)
//! or its weather case's pressure and transverse factor — the rule of the
//! line's structure checks (`ohl_loads::LoadCase::structure_wind`).
//!
//! [`evaluate`] takes the recipes, the structure and the values of the
//! line ([`LineValues`], plain data) and gives the [`LoadTable`]: per case
//! and point (or member, or joint), transverse, longitudinal and vertical
//! (down), with where each value comes from. A value the line does not
//! give is a row marked "no line value"; a value typed over an evaluated
//! one is kept and marked ([`LoadTable::apply`]). The table and the
//! recipes are written and read as CSV (XLSX later).
//!
//! This crate knows nothing of the line: `ohl-structure-link` turns the
//! line's structure loads into [`LineValues`]. A placed structure's values
//! go from the line application to the structure application as a
//! line-values file ([`PlacedValues`], JSON, [`line_values_path`]); the
//! values typed over its table come back as `<stem>.<id>.loads.csv`
//! ([`overrides_path`]).

mod csv;
mod placed;
mod recipe;
mod table;

pub use placed::{
    CaseSource, Governing, LINE_VALUES_FILE_FORMAT, LINE_VALUES_FILE_VERSION, MissingWire,
    OtherPlace, PlacedValues, file_part, incomplete_text, line_values_path, overrides_path,
};
pub use recipe::{
    Direction, LineQuantity, LoadCaseRecipe, LoadRecipe, MODEL_CASE, Pick,
    STRUCTURE_LOAD_CASE_FILE_EXTENSION, STRUCTURE_LOAD_CASE_FILE_FORMAT,
    STRUCTURE_LOAD_CASE_FILE_VERSION, Side, StructureLoadCases, written_version,
};
pub use table::{
    At, CaseWind, LinePoint, LineValue, LineValues, LoadRow, LoadTable, MemberAreas, NoAreas,
    Override, Source, Target, evaluate, evaluate_with_areas, line_pressure, model_rows,
    resolve_line_pressures, targets,
};

#[cfg(test)]
mod tests;
