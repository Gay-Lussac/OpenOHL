// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exports of a level 3 structure (the structure application;
//! `docs/development/structure-exports.md`):
//!
//! - [`ifc`]: IFC4 with the members (profile solids) and the structural
//!   analysis model with its load cases (analysis programs import it as a
//!   structural analysis view; BIM viewers show the members); IFC2x3 Coordination
//!   View 2.0 with the members only (steel-detailing programs convert it
//!   into their own parts);
//! - [`saf`]: SAF, the Structural Analysis Format (xlsx; analysis
//!   programs);
//! - [`csv`]: neutral CSV tables for anything else;
//! - [`report`]: the structure report (printable HTML and the members'
//!   results as CSV) of a run of the structure's cases;
//! - [`request`]: a run asked for by another program (the command line
//!   `ohl structure analyse|report`) as a JSON request, answered as JSON
//!   (`ohl-structure-report --request -`).
//!
//! Each takes plain data, an [`ExportStructure`], made by
//! `ohl_structure_link::export_structure` from the structure file, the
//! section library and a load table ([`structure`] here). The crate also
//! builds the command-line tool `ohl-structure-export`.

pub mod csv;
pub mod ifc;
pub mod report;
pub mod request;
pub mod saf;
mod step;
pub mod xlsx;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use ohl_sections::SectionLibrary;
pub use ohl_structure_link::{
    ExportKind, ExportLoad, ExportLoadCase, ExportMaterial, ExportMember, ExportNode,
    ExportSection, ExportStructure, LoadPlace,
};
use ohl_structure_loads::{LineValues, LoadCaseRecipe, LoadTable, StructureLoadCases};

pub use step::{guid, hash128, uuid};

/// What a member's kind does not say, as text for a format without a
/// field for it (IFC4's analysis member `Description`, SAF's `Layer`):
/// `tension only, bending` for a tension-only member that also bends
/// (exported as a frame), `not checked` for a modelling member, joined by
/// `; `; empty for any other member.
pub fn member_note(m: &ExportMember) -> String {
    let mut parts = Vec::new();
    if m.kind == ExportKind::Tension && m.bending {
        parts.push("tension only, bending");
    }
    if !m.check {
        parts.push("not checked");
    }
    parts.join("; ")
}

/// An export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// IFC4: members and the analysis model with the load cases.
    Ifc,
    /// IFC4: members only (BIM viewers).
    IfcMembers,
    /// IFC2x3 Coordination View 2.0: members only (steel-detailing programs).
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
            Format::Ifc => "IFC4 — analysis model with load cases, and members (analysis programs)",
            Format::IfcMembers => "IFC4 — members with their profiles (BIM viewers)",
            Format::Ifc2x3 => "IFC2x3 — members for steel-detailing programs",
            Format::Saf => "SAF (xlsx) — analysis model with load cases (analysis programs)",
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
    let areas = ohl_structure_link::SectionAreas::new(&structure, text, None, library);
    let table = ohl_structure_loads::evaluate_with_areas(
        &recipes,
        &structure,
        &LineValues::default(),
        &areas,
    );
    Ok((table, recipes))
}

fn parse(text: &str) -> Result<ohl_structures::StructureType, String> {
    use ohl_structures::LibraryItem as _;
    ohl_structures::StructureType::parse(Path::new("structure.ohls"), text).map_err(|e| e.message)
}

/// The section library of `paths` (files `.ohlsec`, or folders whose
/// `.ohlsec` files are read in name order), merged: an id found earlier
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
    ohl_structure_link::export_structure(text, library, Some(table.unwrap_or(&own)), &recipes)
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
