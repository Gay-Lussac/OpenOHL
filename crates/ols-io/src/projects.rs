// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whole projects from other programs' files (`project-plugin` world): a
//! line, its library files (wires, weather cases, load cases…) and notes
//! on what was read, left out or assumed.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ols_line::Line;
use ols_plugins::{WasmHost, WasmProjectPlugin};

use crate::{Options, Registry};

/// A project read from another program's file.
#[derive(Debug, Clone)]
pub struct ImportedProject {
    /// The line (checked).
    pub line: Line,
    /// Library files: kind (`cables`, `weather`, `loadcases`,
    /// `structures`, `insulators`, `rules`, `galloping`, `ambient`,
    /// `costs`, `poles`, `femodels`, `sections`), file name, text.
    pub libraries: Vec<(String, String, String)>,
    /// What was read, left out or assumed.
    pub notes: Vec<String>,
}

impl ImportedProject {
    /// Clips in every section in the blocks: each span keeps the
    /// unstressed length of its stringing condition (as programs that sag
    /// a section and then solve it with fixed lengths do). The cables come
    /// from the project's wire library files. Returns how many sections
    /// were clipped in, and those that could not be (with why).
    pub fn clip_in(&mut self) -> (usize, Vec<String>) {
        let mut library = ols_cables::Library::default();
        let mut failed = Vec::new();
        for (kind, name, text) in &self.libraries {
            if kind == "cables"
                && let Err(e) = library.add_toml(std::path::Path::new(name), text)
            {
                failed.push(format!("{name}: {e}"));
            }
        }
        let mut done = 0;
        let line = self.line.clone();
        for s in &mut self.line.sections {
            if s.stringing.clipped() {
                continue;
            }
            let Some(cable) = library.get(&s.cable) else {
                failed.push(format!(
                    "{}: cable {} not in the wire library",
                    s.id, s.cable
                ));
                continue;
            };
            match ols_line::solve::clip_lengths(&line, s, cable) {
                Ok((lengths, temperature)) => {
                    s.stringing = ols_line::Stringing::UnstressedLengths {
                        lengths,
                        temperature,
                        sagged: Some(Box::new(s.stringing.clone())),
                    };
                    done += 1;
                }
                Err(e) => failed.push(format!("{}: {e}", s.id)),
            }
        }
        (done, failed)
    }
}

/// Reads a whole project from a file.
pub trait ProjectImporter {
    /// Unique id.
    fn id(&self) -> &str;
    /// One-line description.
    fn description(&self) -> &str;
    /// File extension, without the dot.
    fn extension(&self) -> &str;
    /// Where it comes from.
    fn origin(&self) -> String;
    /// The project of `data`.
    fn import(&mut self, data: &[u8], options: &Options) -> Result<ImportedProject, String>;
}

struct PluginProject {
    plugin: Rc<RefCell<WasmProjectPlugin>>,
    name: String,
    id: String,
    description: String,
    extension: String,
}

impl ProjectImporter for PluginProject {
    fn id(&self) -> &str {
        &self.id
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn extension(&self) -> &str {
        &self.extension
    }
    fn origin(&self) -> String {
        format!("plugin {}", self.name)
    }
    fn import(&mut self, data: &[u8], options: &Options) -> Result<ImportedProject, String> {
        let options: Vec<(String, String)> = options
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let p = self
            .plugin
            .borrow_mut()
            .import_project(&self.id, data, &options)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("plugin {}: {e}", self.name))?;
        let line = Line::from_json(&p.line)
            .map_err(|e| format!("plugin {} returned an invalid line: {e}", self.name))?;
        Ok(ImportedProject {
            line,
            libraries: p
                .libraries
                .into_iter()
                .map(|f| (f.kind, f.name, f.text))
                .collect(),
            notes: p.notes,
        })
    }
}

/// Loads the project plugin in `dir` and adds its formats; returns the
/// plugin's name.
pub fn register_plugin(
    registry: &mut Registry,
    host: &WasmHost,
    dir: &Path,
) -> Result<String, String> {
    let mut plugin = host.load_project(dir).map_err(|e| e.to_string())?;
    let name = plugin.manifest().plugin.name.clone();
    let formats = plugin.formats().map_err(|e| e.to_string())?;
    if formats.is_empty() {
        return Err(format!("plugin {name} lists no formats"));
    }
    let plugin = Rc::new(RefCell::new(plugin));
    for (id, description, extension) in formats {
        registry.add_project_importer(Box::new(PluginProject {
            plugin: Rc::clone(&plugin),
            name: name.clone(),
            id: id.to_ascii_lowercase(),
            description,
            extension: extension.trim_start_matches('.').to_ascii_lowercase(),
        }));
    }
    Ok(name)
}

/// A project made by [`create_project`].
#[derive(Debug, Clone)]
pub struct CreatedProject {
    /// The project file.
    pub path: PathBuf,
    /// What was read (notes: also whether the line checks).
    pub project: ImportedProject,
    /// The line's import into the project.
    pub applied: ols_session::Applied,
}

/// Reads another program's file (format: `format`, else the extension)
/// into a new project `into` (`.ohl` added): the line and its library
/// files, the sections clipped in if asked. Refused if the project file
/// exists.
pub fn create_project(
    registry: &mut Registry,
    path: &Path,
    format: Option<&str>,
    options: &Options,
    into: &Path,
    user: &str,
    clip_in: bool,
) -> Result<CreatedProject, String> {
    let into = into.with_extension(ols_core::files::PROJECT_EXTENSION);
    if into.exists() {
        return Err(format!("{} exists already", into.display()));
    }
    let id = crate::format_of(path, format);
    let importer = registry
        .project_importer(&id)
        .ok_or_else(|| format!("no project format \"{id}\": it needs a project plugin"))?;
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut p = importer.import(&data, options)?;
    if clip_in {
        let (n, failed) = p.clip_in();
        p.notes.push(format!(
            "{n} sections clipped in at their stringing condition"
        ));
        p.notes
            .extend(failed.into_iter().map(|f| format!("not clipped in: {f}")));
    }
    if let Err(e) = p.line.validate() {
        p.notes.push(format!("the line does not check yet: {e}"));
    }
    let mut s = ols_session::Session::open(&into).map_err(|e| e.to_string())?;
    let applied = s.import(&p.line, user).map_err(|e| e.to_string())?;
    for (kind, name, text) in &p.libraries {
        s.put_library_file(user, kind, name, text)
            .map_err(|e| e.to_string())?;
    }
    s.checkpoint().map_err(|e| e.to_string())?;
    Ok(CreatedProject {
        path: into,
        project: p,
        applied,
    })
}
