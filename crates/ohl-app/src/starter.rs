// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The **starter template** (File › New project…, Starter): a new project
//! with a small set of example libraries for learning, all values
//! invented or typical, clearly marked — cables, structure types, pylon
//! templates, insulators, weather cases, load cases, clearance rules and a
//! survey of invented ground — but no route, alignment, structure or wire:
//! the tutorial (Help › Start from scratch) builds those. The files are
//! `examples/starter/` of the source tree (its README lists them), the
//! same the tutorial uses.

use std::path::{Path, PathBuf};

use ohl_structures::LibraryItem;

/// The starter's folder (`examples/starter`), if this computer has it.
pub fn dir() -> Option<PathBuf> {
    crate::help::shipped("examples/starter")
}

/// The pylon templates' folder in the starter (copied beside the project,
/// into its own templates folder).
pub const PYLON_TEMPLATES: &str = "pylon-templates";

/// The survey points of the invented ground (copied beside the project,
/// to import with Terrain › Terrain surfaces).
pub const SURVEY: &str = "starter-survey.csv";

/// The library files of the starter: (project library kind, file name,
/// text), from its folders named as the kinds (`cables`, `structures`,
/// `insulators`, `weather`, `loadcases`, `rules`, …).
pub fn library_files(dir: &Path) -> Result<Vec<(String, String, String)>, String> {
    let mut out = Vec::new();
    for (kind, ext) in crate::model::PROJECT_LIBRARIES {
        let folder = dir.join(kind);
        for f in ohl_core::files::files_with_extension(&folder, ext) {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push((kind.to_owned(), name, text));
        }
    }
    if out.is_empty() {
        return Err(format!("{}: no library file", dir.display()));
    }
    Ok(out)
}

/// The starter's files kept in the project `session` (one log entry, by
/// `user`), its pylon templates and its survey copied beside the project
/// file `project` (files there already are kept). Returns what was put
/// where, in words.
pub fn fill(
    session: &mut ohl_session::Session,
    project: &Path,
    user: &str,
    dir: &Path,
) -> Result<Vec<String>, String> {
    let files = library_files(dir)?;
    let changes: Vec<(String, String, Option<String>)> = files
        .iter()
        .map(|(k, n, t)| (k.clone(), n.clone(), Some(t.clone())))
        .collect();
    session
        .keep_library_files(user, &changes)
        .map_err(|e| e.to_string())?;
    let mut notes = vec![format!("{} library files kept in the project", files.len())];
    let beside = ohl_core::files::dir_of(project);
    let templates = beside.join("Pylon templates");
    let from = dir.join(PYLON_TEMPLATES);
    let list = ohl_core::files::files_with_extension(
        &from,
        ohl_structures::templates::PylonTemplate::EXTENSION,
    );
    if !list.is_empty() {
        std::fs::create_dir_all(&templates).map_err(|e| format!("{}: {e}", templates.display()))?;
        for f in list {
            let to = templates.join(f.file_name().unwrap_or_default());
            if !to.exists() {
                std::fs::copy(&f, &to).map_err(|e| format!("{}: {e}", to.display()))?;
            }
        }
        notes.push(format!("pylon templates in {}", templates.display()));
    }
    let survey = dir.join(SURVEY);
    let to = beside.join(SURVEY);
    if survey.is_file() && !to.exists() {
        std::fs::copy(&survey, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        notes.push(format!("survey points: {}", to.display()));
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_structures::templates::{PylonTemplate, Role};

    fn starter() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/starter"
        ))
    }

    /// The starter's structure types: (file, template, role, name).
    const TYPES: [(&str, &str, Role, &str); 2] = [
        (
            "starter-suspension",
            "delta-1c",
            Role::Suspension,
            "Starter suspension (delta, one circuit; invented)",
        ),
        (
            "starter-strain",
            "delta-1c",
            Role::Strain,
            "Starter strain (delta, one circuit; invented)",
        ),
    ];

    /// The starter's structure types are what its pylon templates build
    /// with their defaults (`OHL_WRITE_STARTER=1` writes them again).
    #[test]
    fn starter_structure_types_are_built_from_its_templates() {
        for (id, template, role, name) in TYPES {
            let path = starter()
                .join(PYLON_TEMPLATES)
                .join(format!("{template}.{}", PylonTemplate::EXTENSION));
            let t = PylonTemplate::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap();
            let text = t.build(&t.defaults(role), role, id, name).unwrap();
            let file = starter().join("structures").join(format!("{id}.ohls"));
            if ohl_core::env::var_os("OHL_WRITE_STARTER").is_some() {
                std::fs::write(&file, &text).unwrap();
            }
            assert_eq!(std::fs::read_to_string(&file).unwrap(), text, "{id}");
        }
    }

    /// Every library file of the starter reads (as the project would read
    /// it), and every kind the tutorial needs is there.
    #[test]
    fn starter_libraries_read() {
        let files = library_files(&starter()).unwrap();
        for kind in [
            "cables",
            "structures",
            "insulators",
            "weather",
            "loadcases",
            "rules",
        ] {
            assert!(files.iter().any(|(k, _, _)| k == kind), "{kind}");
        }
        for (kind, name, text) in &files {
            assert!(
                text.to_lowercase().contains("invented")
                    || text.to_lowercase().contains("typical")
                    || text.to_lowercase().contains("example"),
                "{kind}/{name} is not marked as an example"
            );
        }
        let dir = std::env::temp_dir().join(format!("ohl-starter-read-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        for (kind, name, text) in &files {
            let d = dir.join(kind);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(name), text).unwrap();
        }
        ohl_cables::Library::load_dir(&dir.join("cables")).unwrap();
        ohl_weather::WeatherCases::load(&dir.join("weather/starter-weather-cases.ohlwc")).unwrap();
        ohl_clearance::RuleSet::load(&dir.join("rules/starter-clearance-rules.ohlr")).unwrap();
        for (kind, name, text) in files.iter().filter(|(k, _, _)| k == "structures") {
            ohl_structures::StructureType::parse(Path::new(name), text)
                .unwrap_or_else(|e| panic!("{kind}/{name}: {e}"));
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
