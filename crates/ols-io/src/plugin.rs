// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! File formats from WebAssembly plugins (`io-plugin` world).
//!
//! Each format a plugin lists becomes an [`Exporter`] or [`Importer`];
//! they share the plugin instance. Exporters receive the model as JSON
//! ([`ExportModel::to_json`]); importers return a line file (JSON), which
//! is parsed and checked like any line file. A plugin error, a trap or a
//! time or memory limit becomes the format's error message.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use ols_line::Line;
use ols_plugins::{Direction, WasmHost, WasmIoPlugin};

use crate::{ExportModel, Exporter, Importer, Options, Registry};

struct PluginFormat {
    plugin: Rc<RefCell<WasmIoPlugin>>,
    name: String,
    id: String,
    description: String,
    extension: String,
}

fn options_list(options: &Options) -> Vec<(String, String)> {
    options
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

impl Exporter for PluginFormat {
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
    fn export(&mut self, model: &ExportModel, options: &Options) -> Result<Vec<u8>, String> {
        self.plugin
            .borrow_mut()
            .export_model(&self.id, &model.to_json(), &options_list(options))
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("plugin {}: {e}", self.name))
    }
}

impl Importer for PluginFormat {
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
    fn import(
        &mut self,
        data: &[u8],
        base: Option<&Line>,
        options: &Options,
    ) -> Result<Line, String> {
        let mut options = options.clone();
        if let Some(b) = base {
            // The plugin may update the line it is given.
            options.insert("into".into(), b.to_json());
        }
        let json = self
            .plugin
            .borrow_mut()
            .import_file(&self.id, data, &options_list(&options))
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("plugin {}: {e}", self.name))?;
        Line::from_json(&json)
            .map_err(|e| format!("plugin {} returned an invalid line: {e}", self.name))
    }
}

/// Loads the plugin in `dir` and adds its formats; returns the plugin's
/// name.
pub fn register(registry: &mut Registry, host: &WasmHost, dir: &Path) -> Result<String, String> {
    let mut plugin = host.load_io(dir).map_err(|e| e.to_string())?;
    let name = plugin.describe().map_err(|e| e.to_string())?.name;
    let formats = plugin.formats().map_err(|e| e.to_string())?;
    if formats.is_empty() {
        return Err(format!("plugin {name} lists no formats"));
    }
    let plugin = Rc::new(RefCell::new(plugin));
    for f in formats {
        let format = PluginFormat {
            plugin: Rc::clone(&plugin),
            name: name.clone(),
            id: f.id.to_ascii_lowercase(),
            description: f.description,
            extension: f.extension.trim_start_matches('.').to_ascii_lowercase(),
        };
        match f.direction {
            Direction::Exporter => registry.add_exporter(Box::new(format)),
            Direction::Importer => registry.add_importer(Box::new(format)),
        }
    }
    Ok(name)
}
