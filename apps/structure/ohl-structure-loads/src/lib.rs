// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Load-case recipes of a structure and its load table (the structure
//! application; specification: `docs/development/structure-building.md`
//! §2).
//!
//! A structure's **load case** ([`LoadCaseRecipe`]; owner, 2026-10-25,
//! spec `docs/development/structure-load-cases-spec.md`) has two parts:
//! **line forces** ([`LineForces`]: a line result the line sent — a
//! weather case, unfactored, a wind case twice, the wind from the left
//! and from the right — with a factor per axis, at every point or some
//! sets, points turned off) and **loads on the structure itself**
//! ([`OwnLoads`]: its self weight, the wind on its body as the line's own
//! member wind, ice or a weight per metre on members, point loads); its
//! strength factor and a description. Kept in the structure file as
//! `[[load_case]]`, or on their own in a structure load-case file
//! (`.ohlslc`, version 3) for export and import:
//!
//! ```toml
//! format = "ohl-structure-load-cases"
//! version = 3
//!
//! [[load_case]]
//! name = "ULS wind left"
//! strength = 1.0
//!
//! [[load_case.line]]
//! result = "storm — wind from the left"
//! vertical = 1.35
//! transverse = 1.5
//! longitudinal = 1.0
//! off = ["A3"]
//!
//! [load_case.structure]
//! self_weight = 1.35
//!
//! [[load_case.structure.wind]]
//! from = "left"
//! pressure_of = "storm — wind from the left"
//! factor = 1.5
//! ```
//!
//! **The older form** (read and evaluated as before): a list of loads
//! ([`LoadRecipe`]), each saying **what** (a value the line gives at the
//! placed structure, [`LineQuantity`], or a value of its own), **how** (a
//! factor, a one-sided share, a direction in the structure's axes) and
//! **where** (points by label, set, phase or side, or members with a
//! pressure on their area); [`convert_older`] turns what is exact into the
//! parts:
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
mod parts;
mod placed;
mod recipe;
mod table;

pub use parts::{
    BodyWindLoad, Converted, IceLoad, LineForces, OwnLoads, PART_BLOCK, Part, PointLoad, WindFrom,
    convert_older, describe, ice_text, line_forces_text, mirror_wind, point_text, wind_text,
};
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
    At, BodyWind, CaseWind, LinePoint, LineValue, LineValues, LoadRow, LoadTable, MemberAreas,
    NoAreas, Override, Source, Target, evaluate, evaluate_with_areas, line_pressure, model_rows,
    needs_members, resolve_line_pressures, targets,
};

#[cfg(test)]
mod parts_tests;
#[cfg(test)]
mod tests;
