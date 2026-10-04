// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whole projects from other programs' files (`project-plugin` world): a
//! line, its library files (wires, weather cases, load cases…) and notes
//! on what was read, left out or assumed.

use std::cell::RefCell;
use std::path::Path;
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
    /// `structures`, `insulators`), file name, text.
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
