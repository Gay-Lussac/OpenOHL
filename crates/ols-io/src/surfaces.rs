// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terrain surface (TIN) formats: built-in readers (LandXML, OBJ, survey
//! points triangulated) and those of terrain plugins (`terrain-plugin`
//! world) — to the rest of OLS CAD both look the same.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use ols_plugins::{WasmHost, WasmTerrainPlugin};
use ols_terrain::formats::Mesh;

use crate::{Options, Registry};

/// Reads terrain surfaces from a file.
pub trait SurfaceImporter {
    /// Unique id, e.g. `"landxml"`.
    fn id(&self) -> &str;
    /// One-line description.
    fn description(&self) -> &str;
    /// File extension, without the dot.
    fn extension(&self) -> &str;
    /// Where it comes from.
    fn origin(&self) -> String {
        "built-in".into()
    }
    /// The surfaces of `data`; `name`: the file's name (for surfaces
    /// without their own).
    fn import(&mut self, data: &[u8], name: &str, options: &Options) -> Result<Vec<Mesh>, String>;
}

fn text(data: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(data).map_err(|e| format!("not UTF-8 text: {e}"))
}

/// The maximum triangle side option (`max_edge`, m), if given.
fn max_edge(options: &Options) -> Result<Option<f64>, String> {
    options
        .get("max_edge")
        .map(|v| {
            v.parse::<f64>()
                .ok()
                .filter(|x| *x > 0.0)
                .ok_or_else(|| format!("max_edge \"{v}\": a length in m"))
        })
        .transpose()
}

struct LandXml;

impl SurfaceImporter for LandXml {
    fn id(&self) -> &str {
        "landxml"
    }
    fn description(&self) -> &str {
        "LandXML TIN surfaces (all those of the file)"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn import(&mut self, data: &[u8], _: &str, _: &Options) -> Result<Vec<Mesh>, String> {
        ols_terrain::formats::read_landxml(text(data)?).map_err(|e| e.to_string())
    }
}

struct Obj;

impl SurfaceImporter for Obj {
    fn id(&self) -> &str {
        "obj"
    }
    fn description(&self) -> &str {
        "Wavefront OBJ mesh (x east, y north, z up)"
    }
    fn extension(&self) -> &str {
        "obj"
    }
    fn import(&mut self, data: &[u8], name: &str, _: &Options) -> Result<Vec<Mesh>, String> {
        ols_terrain::formats::read_obj(text(data)?, name)
            .map(|m| vec![m])
            .map_err(|e| e.to_string())
    }
}

/// Survey points (CSV / XYZ, codes and breaklines) triangulated.
struct Points {
    id: &'static str,
    extension: &'static str,
}

impl SurfaceImporter for Points {
    fn id(&self) -> &str {
        self.id
    }
    fn description(&self) -> &str {
        "survey points triangulated (breaklines kept; option max_edge = longest side, m)"
    }
    fn extension(&self) -> &str {
        self.extension
    }
    fn import(&mut self, data: &[u8], name: &str, options: &Options) -> Result<Vec<Mesh>, String> {
        let points = ols_terrain::read_points(text(data)?).map_err(|e| e.to_string())?;
        let t = ols_terrain::Terrain::from_points(&points, max_edge(options)?)
            .map_err(|e| e.to_string())?;
        Ok(vec![Mesh {
            name: name.to_owned(),
            points: t.points().to_vec(),
            triangles: t.triangles().to_vec(),
        }])
    }
}

/// The built-in surface formats.
pub fn register(r: &mut Registry) {
    r.add_surface_importer(Box::new(LandXml));
    r.add_surface_importer(Box::new(Obj));
    r.add_surface_importer(Box::new(Points {
        id: "points-csv",
        extension: "csv",
    }));
    r.add_surface_importer(Box::new(Points {
        id: "points-xyz",
        extension: "xyz",
    }));
}

struct PluginSurfaces {
    plugin: Rc<RefCell<WasmTerrainPlugin>>,
    name: String,
    id: String,
    description: String,
    extension: String,
}

impl SurfaceImporter for PluginSurfaces {
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
    fn import(&mut self, data: &[u8], _: &str, options: &Options) -> Result<Vec<Mesh>, String> {
        let options: Vec<(String, String)> = options
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let surfaces = self
            .plugin
            .borrow_mut()
            .import_surfaces(&self.id, data, &options)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("plugin {}: {e}", self.name))?;
        surfaces
            .into_iter()
            .map(|s| {
                if s.points.len() % 3 != 0 || s.triangles.len() % 3 != 0 {
                    return Err(format!(
                        "plugin {}: surface {}: points or triangles not in threes",
                        self.name, s.name
                    ));
                }
                Ok(Mesh {
                    name: s.name,
                    points: s.points.as_chunks::<3>().0.to_vec(),
                    triangles: s.triangles.as_chunks::<3>().0.to_vec(),
                })
            })
            .collect()
    }
}

/// Loads the terrain plugin in `dir` and adds its formats; returns the
/// plugin's name.
pub fn register_plugin(
    registry: &mut Registry,
    host: &WasmHost,
    dir: &Path,
) -> Result<String, String> {
    let mut plugin = host.load_terrain(dir).map_err(|e| e.to_string())?;
    let name = plugin.manifest().plugin.name.clone();
    let formats = plugin.formats().map_err(|e| e.to_string())?;
    if formats.is_empty() {
        return Err(format!("plugin {name} lists no formats"));
    }
    let plugin = Rc::new(RefCell::new(plugin));
    for f in formats {
        registry.add_surface_importer(Box::new(PluginSurfaces {
            plugin: Rc::clone(&plugin),
            name: name.clone(),
            id: f.id.to_ascii_lowercase(),
            description: f.description,
            extension: f.extension.trim_start_matches('.').to_ascii_lowercase(),
        }));
    }
    Ok(name)
}
