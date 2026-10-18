// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Example OpenOHL project plugin (`project-plugin` world): a JSON bundle
//! `{"line": <line file>, "libraries": [{"kind", "name", "text"}, …]}` —
//! the shape a reader of another program's files returns. Option
//! `name=…` renames the line.
//!
//! Build with:
//!
//! ```bash
//! cargo build --release --target wasm32-wasip2
//! ```

use serde_json::Value;

wit_bindgen::generate!({
    world: "project-plugin",
    path: "../../../crates/ols-plugins/wit",
});

use ols::plugin::io_types::Direction;
use ols::plugin::project_types::LibraryFile;

struct Bundle;

impl Guest for Bundle {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "project-bundle".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "A JSON bundle of a line and its library files".into(),
        }
    }

    fn formats() -> Vec<Format> {
        vec![Format {
            id: "ols-bundle".into(),
            description: "JSON bundle: a line and its library files".into(),
            extension: "olsb".into(),
            direction: Direction::Importer,
        }]
    }

    fn import_project(
        format: String,
        data: Vec<u8>,
        options: Vec<(String, String)>,
    ) -> Result<Project, String> {
        if format != "ols-bundle" {
            return Err(format!("unknown format {format}"));
        }
        let mut v: Value = serde_json::from_slice(&data).map_err(|e| format!("bundle: {e}"))?;
        let mut line = v.get_mut("line").ok_or("bundle: no line")?.take();
        if let Some((_, n)) = options.iter().find(|(k, _)| k == "name") {
            line["name"] = Value::from(n.clone());
        }
        let mut libraries = Vec::new();
        for f in v["libraries"].as_array().into_iter().flatten() {
            let s = |k: &str| f[k].as_str().map(str::to_owned).ok_or(format!("library file: no {k}"));
            libraries.push(LibraryFile {
                kind: s("kind")?,
                name: s("name")?,
                text: s("text")?,
            });
        }
        Ok(Project {
            line: line.to_string(),
            notes: vec![format!("{} library files", libraries.len())],
            libraries,
        })
    }
}

export!(Bundle);
