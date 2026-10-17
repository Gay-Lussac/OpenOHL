// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exports of a level 3 structure (the structure application;
//! `docs/development/structure-exports.md`):
//!
//! - [`ifc`]: IFC4 with the members (profile solids) and the structural
//!   analysis model with its load cases (RFEM imports it as a structural
//!   analysis view; BIM viewers show the members); IFC2x3 Coordination
//!   View 2.0 with the members only (Tekla Structures converts it into its
//!   own parts);
//! - [`saf`]: SAF, the Structural Analysis Format (xlsx; RFEM, SCIA and
//!   other analysis programs);
//! - [`csv`]: neutral CSV tables for anything else.
//!
//! Each takes plain data, an [`ExportStructure`], made by
//! `ols_structure_link::export_structure` from the structure file, the
//! section library and a load table ([`structure`] here). The crate also
//! builds the command-line tool `ols-structure-export`.

pub mod csv;
pub mod ifc;
pub mod saf;
mod step;
pub mod xlsx;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use ols_sections::SectionLibrary;
pub use ols_structure_link::{
    ExportKind, ExportLoad, ExportLoadCase, ExportMaterial, ExportMember, ExportNode,
    ExportSection, ExportStructure, LoadPlace,
};
use ols_structure_loads::{LineValues, LoadCaseRecipe, LoadTable, StructureLoadCases};

pub use step::{guid, hash128, uuid};

/// An export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// IFC4: members and the analysis model with the load cases.
    Ifc,
    /// IFC4: members only (BIM viewers).
    IfcMembers,
    /// IFC2x3 Coordination View 2.0: members only (Tekla Structures).
    Ifc2x3,
    /// SAF workbook (xlsx).
    Saf,
    /// CSV tables in a folder.
    Csv,
}

impl Format {
    /// Every format, in menu order.
    pub const ALL: [Format; 5] = [
        Format::Ifc,
        Format::IfcMembers,
        Format::Ifc2x3,
        Format::Saf,
        Format::Csv,
    ];

    /// Name on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Ifc => "ifc",
            Format::IfcMembers => "ifc-members",
            Format::Ifc2x3 => "ifc2x3",
            Format::Saf => "saf",
            Format::Csv => "csv",
        }
    }

    /// From its name on the command line.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == s)
    }

    /// What it is, for menus.
    pub fn description(self) -> &'static str {
        match self {
            Format::Ifc => "IFC4 — analysis model with load cases, and members (RFEM…)",
            Format::IfcMembers => "IFC4 — members with their profiles (BIM viewers)",
            Format::Ifc2x3 => "IFC2x3 — members for Tekla Structures",
            Format::Saf => "SAF (xlsx) — analysis model with load cases (RFEM, SCIA…)",
            Format::Csv => "CSV tables (nodes, members, sections, supports, loads)",
        }
    }

    /// File extension (none: a folder).
    pub fn extension(self) -> Option<&'static str> {
        match self {
            Format::Ifc | Format::IfcMembers | Format::Ifc2x3 => Some("ifc"),
            Format::Saf => Some("xlsx"),
            Format::Csv => None,
        }
    }
}

/// SAF's material type from Young's modulus and the density (the
/// section library does not say what a material is): steel 180–230 GPa,
/// aluminium 60–80 GPa, concrete 20–50 GPa above 2000 kg/m³, timber below
/// 20 GPa and 1000 kg/m³, else other.
pub fn material_kind(m: &ExportMaterial) -> &'static str {
    let e = m.elasticity / 1e9;
    if (180.0..=230.0).contains(&e) {
        "Steel"
    } else if (60.0..=80.0).contains(&e) {
        "Aluminium"
    } else if (20.0..=50.0).contains(&e) && m.density > 2000.0 {
        "Concrete"
    } else if e < 20.0 && m.density > 0.0 && m.density < 1000.0 {
        "Timber"
    } else {
        "Other"
    }
}

/// The load table of a structure opened on its own: its recipes evaluated
/// without line values (rows needing one are marked and left out of the
/// exports), surface loads on the members' areas from `library`, and its
/// `[[load]]` tables as case `model`.
pub fn own_load_table(
    text: &str,
    library: &SectionLibrary,
) -> Result<(LoadTable, Vec<LoadCaseRecipe>), String> {
    let structure = parse(text)?;
    let recipes = StructureLoadCases::from_structure(&structure)?;
    let areas = ols_structure_link::SectionAreas::new(&structure, text, None, library);
    let table = ols_structure_loads::evaluate_with_areas(
        &recipes,
        &structure,
        &LineValues::default(),
        &areas,
    );
    Ok((table, recipes))
}

fn parse(text: &str) -> Result<ols_structures::StructureType, String> {
    use ols_structures::LibraryItem as _;
    ols_structures::StructureType::parse(Path::new("structure.olss"), text).map_err(|e| e.message)
}

/// The section library of `paths` (files `.olssec`, or folders whose
/// `.olssec` files are read in name order), merged: an id found earlier
/// wins; the ids left out are the second value.
pub fn load_sections(paths: &[PathBuf]) -> Result<(SectionLibrary, Vec<String>), String> {
    let mut library = SectionLibrary::default();
    let mut left_out = Vec::new();
    for p in paths {
        left_out.extend(library.merge(SectionLibrary::load(p)?));
    }
    Ok((library, left_out))
}

/// The structure file `text` as export data: its sections from `library`
/// first; the load cases from `table` (a CSV load table), else its own
/// recipes evaluated without line values ([`own_load_table`]).
pub fn structure(
    text: &str,
    library: &SectionLibrary,
    table: Option<&LoadTable>,
) -> Result<ExportStructure, String> {
    let (own, recipes) = own_load_table(text, library)?;
    ols_structure_link::export_structure(text, library, Some(table.unwrap_or(&own)), &recipes)
}

/// Writes `s` in `format` to `out` (a file; a folder for CSV); returns
/// the files written.
pub fn write(
    s: &ExportStructure,
    format: Format,
    out: &Path,
    time: i64,
) -> Result<Vec<PathBuf>, String> {
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let ifc_options = |schema, members, analysis| ifc::IfcOptions {
        schema,
        members,
        analysis,
        time,
    };
    let bytes = match format {
        Format::Ifc => ifc::ifc(s, &ifc_options(ifc::Schema::Ifc4, true, true))?.into_bytes(),
        Format::IfcMembers => {
            ifc::ifc(s, &ifc_options(ifc::Schema::Ifc4, true, false))?.into_bytes()
        }
        Format::Ifc2x3 => ifc::ifc(s, &ifc_options(ifc::Schema::Ifc2x3, true, false))?.into_bytes(),
        Format::Saf => saf::workbook(s)?,
        Format::Csv => {
            std::fs::create_dir_all(out).map_err(|e| io(out, e))?;
            let mut written = Vec::new();
            for (name, text) in csv::tables(s) {
                let p = out.join(name);
                std::fs::write(&p, text).map_err(|e| io(&p, e))?;
                written.push(p);
            }
            return Ok(written);
        }
    };
    std::fs::write(out, bytes).map_err(|e| io(out, e))?;
    Ok(vec![out.to_path_buf()])
}
