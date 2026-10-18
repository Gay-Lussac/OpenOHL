// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terrain surface (TIN) formats: built-in readers (LandXML, OBJ, survey
//! points triangulated) and those of terrain plugins (`terrain-plugin`
//! world) — to the rest of OpenOHL both look the same.

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
        ols_terrain::formats::read_landxml(&crate::xml::decode(data)?).map_err(|e| e.to_string())
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

/// What [`import_into`] does with a surface whose name the project holds
/// already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhenTaken {
    /// Keep both: the new one under a free name (`name (2)`, …).
    Rename,
    /// Replace the stored one.
    Replace,
    /// Store nothing and say so.
    Refuse,
}

/// A surface stored by [`import_into`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// Its name in the project.
    pub name: String,
    /// Number of points.
    pub points: usize,
    /// Number of triangles.
    pub triangles: usize,
}

/// `base`, or `base (2)`, `base (3)`… whichever is not `taken`.
pub fn free_name(base: &str, taken: &[String]) -> String {
    let mut name = base.to_owned();
    let mut k = 2;
    while taken.contains(&name) {
        name = format!("{base} ({k})");
        k += 1;
    }
    name
}

/// Reads a surface file (format: `format`, else the extension) and keeps
/// its surfaces in the project. Each is named `name` if given, else its
/// own name, else the file's; surfaces of one file never share a name
/// (`name (2)`, …). Every surface is checked before any is stored.
#[allow(clippy::too_many_arguments)]
pub fn import_into(
    registry: &mut Registry,
    path: &Path,
    format: Option<&str>,
    options: &Options,
    session: &ols_session::Session,
    user: &str,
    name: Option<&str>,
    when_taken: WhenTaken,
) -> Result<Vec<Stored>, String> {
    let id = crate::format_of(path, format);
    let importer = registry
        .surface_importer(&id)
        .ok_or_else(|| format!("no surface format \"{id}\""))?;
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let stem = path
        .file_stem()
        .map_or("surface".into(), |s| s.to_string_lossy().into_owned());
    let meshes = importer
        .import(&data, &stem, options)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let source = format!(
        "{} ({})",
        path.file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        importer.id()
    );
    let existing: Vec<String> = session
        .surfaces()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|i| i.name)
        .collect();
    let mut taken = match when_taken {
        WhenTaken::Rename => existing.clone(),
        WhenTaken::Replace | WhenTaken::Refuse => Vec::new(),
    };
    let mut named = Vec::new();
    for m in meshes {
        let base = match name.map(str::trim) {
            Some(n) if !n.is_empty() => n.to_owned(),
            _ if m.name.is_empty() => stem.clone(),
            _ => m.name.clone(),
        };
        let name = free_name(&base, &taken);
        if when_taken == WhenTaken::Refuse && existing.contains(&name) {
            return Err(format!(
                "a surface \"{name}\" exists already (replace it, or choose another name)"
            ));
        }
        // Checked like any surface (indices, areas).
        let t = m.terrain().map_err(|e| format!("{name}: {e}"))?;
        taken.push(name.clone());
        named.push((name, t));
    }
    let mut out = Vec::new();
    for (name, t) in named {
        session
            .put_surface(user, &name, &source, t.points(), t.triangles())
            .map_err(|e| e.to_string())?;
        out.push(Stored {
            name,
            points: t.num_points(),
            triangles: t.num_triangles(),
        });
    }
    session.checkpoint().map_err(|e| e.to_string())?;
    Ok(out)
}
