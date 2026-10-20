// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Sheet templates on the command line: finding one by name (built in, a
//! file, the project's, the library folder's, a plugin's), the values of
//! its fields and the revisions, the other versions of the line drawn with
//! it, and `ohl sheet-templates` (the list, and writing them out).

use std::path::{Path, PathBuf};

use ohl_drawing::template::{EXTENSION, SheetTemplate, TemplateImage};
use ohl_drawing::{Revision, SheetFields};

/// The project library kind of sheet templates (`library` objects of a
/// project line, each its template with its images embedded).
pub const KIND: &str = "sheet-templates";

/// A template found: where from, its name, the template (or why it does
/// not read).
pub struct Found {
    /// `built-in`, `project`, `library`, or `plugin NAME`.
    pub source: String,
    /// Its name (`PLUGIN/PRESET` for a plugin's).
    pub name: String,
    /// The template, or why it does not read.
    pub template: Result<SheetTemplate, String>,
}

/// The templates of a project file's current line.
fn project_templates(project: &Path) -> Vec<Found> {
    if !crate::project::is_project(project) {
        return Vec::new();
    }
    let Ok(session) = crate::project::open_existing(project) else {
        return Vec::new();
    };
    session
        .library_files(KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, text)| Found {
            source: "project".into(),
            name: name.trim_end_matches(&format!(".{EXTENSION}")).to_owned(),
            template: SheetTemplate::parse(&text, None, &[]),
        })
        .collect()
}

/// The templates of the library folder (`libraries/sheet-templates`).
fn library_templates() -> Vec<Found> {
    let dir = ohl_core::files::shipped_library(KIND);
    ohl_core::files::files_with_extension(&dir, EXTENSION)
        .into_iter()
        .map(|p| Found {
            source: "library".into(),
            name: p
                .file_stem()
                .map_or(String::new(), |s| s.to_string_lossy().into_owned()),
            template: SheetTemplate::load(&p),
        })
        .collect()
}

/// The templates the plugins in `roots` give (a plugin that does not
/// load: one entry saying why).
pub fn plugin_templates(roots: &[PathBuf]) -> Vec<Found> {
    let found = ohl_plugins::discover_sheet_templates(roots);
    if found.is_empty() {
        return Vec::new();
    }
    let host = match ohl_plugins::WasmHost::new() {
        Ok(h) => h,
        Err(e) => {
            return vec![Found {
                source: "plugins".into(),
                name: String::new(),
                template: Err(e.to_string()),
            }];
        }
    };
    let mut out = Vec::new();
    for f in found {
        let plugin = f.name();
        match host.sheet_templates(&f.dir) {
            Ok(list) => out.extend(list.into_iter().map(|t| {
                let images: Vec<TemplateImage> = t
                    .images
                    .into_iter()
                    .map(|(name, media_type, bytes)| TemplateImage {
                        name,
                        media_type,
                        bytes,
                    })
                    .collect();
                Found {
                    source: format!("plugin {plugin}"),
                    name: format!("{plugin}/{}", t.name),
                    template: SheetTemplate::parse(&t.template, t.dir.as_deref(), &images)
                        .map_err(|e| format!("plugin {plugin}, template {}: {e}", t.name)),
                }
            })),
            Err(e) => out.push(Found {
                source: format!("plugin {plugin}"),
                name: format!("{plugin}/"),
                template: Err(e.to_string()),
            }),
        }
    }
    out
}

/// Every template: built in, the project's, the library folder's, the
/// plugins'.
pub fn all(project: Option<&Path>, plugins: &[PathBuf]) -> Vec<Found> {
    let mut out = vec![Found {
        source: "built-in".into(),
        name: "default".into(),
        template: Ok(SheetTemplate::builtin()),
    }];
    if let Some(p) = project {
        out.extend(project_templates(p));
    }
    out.extend(library_templates());
    out.extend(plugin_templates(plugins));
    out
}

/// The template `spec` names: `default`, a template file, a template of
/// the project or the library folder by name, or `PLUGIN/PRESET`.
pub fn resolve(
    spec: &str,
    project: Option<&Path>,
    plugins: &[PathBuf],
) -> Result<SheetTemplate, String> {
    if spec == "default" {
        return Ok(SheetTemplate::builtin());
    }
    let path = Path::new(spec);
    if path.is_file() {
        return SheetTemplate::load(path);
    }
    let mut known = Vec::new();
    // The project's and the library's first; plugins only when named so.
    let local: Vec<Found> = project
        .map(project_templates)
        .unwrap_or_default()
        .into_iter()
        .chain(library_templates())
        .collect();
    for f in local {
        if f.name == spec {
            return f.template;
        }
        known.push(f.name);
    }
    if spec.contains('/') {
        for f in plugin_templates(plugins) {
            if f.name == spec {
                return f.template;
            }
            known.push(f.name);
        }
    }
    Err(format!(
        "sheet template \"{spec}\": not a file, nor one of default{}{} (plugins: PLUGIN/PRESET \
         with --plugin)",
        if known.is_empty() { "" } else { ", " },
        known.join(", ")
    ))
}

/// The values of the template's fields: from a fields file (TOML: keys, or
/// `[values]`, `[per_sheet.<key>]`, `[[revision]]`), then each
/// `KEY=VALUE` (`KEY@SHEET=VALUE` for one sheet), then the revisions
/// `INDEX;DATE;TEXT;BY;CHECKED`.
pub fn fields(
    file: Option<&Path>,
    given: &[String],
    revisions: &[String],
) -> Result<SheetFields, String> {
    let mut out = match file {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            SheetFields::parse(&text).map_err(|e| format!("{}: {e}", p.display()))?
        }
        None => SheetFields::default(),
    };
    for g in given {
        let (key, value) = g
            .split_once('=')
            .ok_or_else(|| format!("--field {g}: write KEY=VALUE"))?;
        match key.split_once('@') {
            Some((k, n)) => {
                let n: usize = n
                    .trim()
                    .parse()
                    .map_err(|_| format!("--field {g}: KEY@SHEET with a sheet number"))?;
                out.per_sheet
                    .entry(k.trim().to_owned())
                    .or_default()
                    .insert(n, value.to_owned());
            }
            None => {
                out.values.insert(key.trim().to_owned(), value.to_owned());
            }
        }
    }
    out.revisions
        .extend(revisions.iter().map(|r| Revision::parse(r)));
    Ok(out)
}

/// A `--version LINE=ROLE[:LABEL]`: the line (a project line's name, or a
/// line file), its role, its label (`{name}` and `{voltage}` filled when
/// drawn; default `{name}`).
pub fn parse_version(text: &str) -> Result<(String, String, String), String> {
    let (line, rest) = text
        .rsplit_once('=')
        .ok_or_else(|| format!("--version {text}: write LINE=ROLE or LINE=ROLE:LABEL"))?;
    let (role, label) = rest.split_once(':').unwrap_or((rest, "{name}"));
    if line.trim().is_empty() || role.trim().is_empty() {
        return Err(format!("--version {text}: write LINE=ROLE"));
    }
    Ok((
        line.trim().to_owned(),
        role.trim().to_owned(),
        label.to_owned(),
    ))
}

/// The line `name` names for `--version`: a project line of `project`
/// (when it is a project file holding one so named), else a line file;
/// its typed structures resolved from `structures` and `insulators`.
pub fn version_line(
    project: &Path,
    name: &str,
    structures: &Path,
    insulators: &Path,
) -> Result<ohl_line::Line, String> {
    if crate::project::is_project(project) {
        let session = crate::project::open_existing(project)?;
        if let Ok(line) = session.line_named(name) {
            let types = if line.structures.iter().any(|s| s.structure_type.is_some()) {
                ohl_structures::StructureLibrary::load_dir(structures).map_err(|e| e.to_string())?
            } else {
                ohl_structures::StructureLibrary::default()
            };
            let assemblies = ohl_structures::InsulatorLibrary::load_dir(insulators)
                .map_err(|e| e.to_string())?;
            return line
                .resolve(&types, &assemblies)
                .map_err(|e| format!("project line {name}: {e}"));
        }
    }
    let path = Path::new(name);
    if !path.is_file() {
        return Err(format!(
            "--version {name}: neither a project line of {} nor a line file",
            project.display()
        ));
    }
    let dir = crate::project::structures_dir(path, None);
    let dir = if dir.is_dir() {
        dir
    } else {
        structures.to_path_buf()
    };
    crate::project::load_resolved(path, &dir, insulators).map(|(l, _)| l)
}

/// `ohl sheet-templates`: lists every template (source, name, page,
/// fields); with `out`, writes each as `.ohlsheet` (its images embedded)
/// into that folder.
pub fn list(project: Option<&Path>, plugins: &[PathBuf], out: Option<&Path>) -> Result<(), String> {
    let found = all(project, plugins);
    if let Some(dir) = out {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    for f in &found {
        match &f.template {
            Ok(t) => {
                let page = t.page().map_or("the export's".to_owned(), |p| {
                    format!("{} ({:.0} × {:.0} mm)", p.name, p.width, p.height)
                });
                let fields: Vec<String> = t
                    .fields
                    .iter()
                    .map(|x| format!("{}{}", x.key, if x.required { "*" } else { "" }))
                    .collect();
                println!(
                    "{:<10} {:<28} page {page}; fields: {}{}",
                    f.source,
                    f.name,
                    if fields.is_empty() {
                        "none".into()
                    } else {
                        fields.join(", ")
                    },
                    if t.description.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", t.description)
                    }
                );
                if let Some(dir) = out {
                    let file = dir.join(format!(
                        "{}.{EXTENSION}",
                        ohl_drawing::export::file_stem(&f.name.replace('/', "-"))
                    ));
                    std::fs::write(&file, t.embedded())
                        .map_err(|e| format!("{}: {e}", file.display()))?;
                }
            }
            Err(e) => println!("{:<10} {:<28} does not read: {e}", f.source, f.name),
        }
    }
    if out.is_some() {
        println!("(written, images embedded, * marks a required field)");
    } else {
        println!("(* marks a required field)");
    }
    Ok(())
}

/// The LiDAR points the sheets draw (`[lidar] profile` / `plan` and some
/// class chosen): read once from `--pointcloud` or `--lidar`, only the
/// classes drawn, within the corridor (else the line width) of the routes
/// drawn (`routes`; none: every one); the options with the class scheme's
/// names and colours (`--lidar-classes`, else the one stored with the
/// points, else ASPRS). Nothing is thinned here.
#[allow(clippy::type_complexity)]
pub fn sheet_lidar(
    line: &ohl_line::Line,
    mut options: ohl_drawing::Options,
    lidar: Option<&Path>,
    pointcloud: Option<&str>,
    scheme_file: Option<&Path>,
    routes: &[String],
) -> Result<(ohl_drawing::Options, Vec<([f64; 3], u8)>), String> {
    if !options.draws_lidar() {
        return Ok((options, Vec::new()));
    }
    let source: Box<dyn ohl_pointcloud::PointSource> = match (pointcloud, lidar) {
        (Some(spec), _) => ohl_pointcloud::open_source(spec)?,
        (None, Some(dir)) => Box::new(ohl_pointcloud::TileSource {
            tiles: ohl_lidar::tiles_in(dir).map_err(|e| e.to_string())?,
            scheme: None,
        }),
        (None, None) => {
            println!(
                "  warning: the sheet template draws LiDAR points: give --lidar or --pointcloud"
            );
            return Ok((options, Vec::new()));
        }
    };
    let scheme = match (scheme_file, source.class_scheme()) {
        (Some(p), _) => ohl_lidar::ClassScheme::load(p)?,
        (None, Some(text)) => ohl_lidar::ClassScheme::parse(&text)?,
        (None, None) => ohl_lidar::ClassScheme::asprs(),
    };
    options.class_names = scheme
        .classes
        .iter()
        .map(|c| (c.code, (c.name.clone(), c.rgb())))
        .collect();
    let classes = options.lidar_class_list();
    let width = options.lidar_half_width();
    let centrelines: Vec<Vec<[f64; 2]>> = line
        .alignment_views()
        .iter()
        .filter(|r| routes.is_empty() || routes.contains(&r.id))
        .map(|r| r.centreline(line).iter().map(|p| [p[0], p[1]]).collect())
        .collect();
    let corridor = ohl_lidar::Corridor::new(&centrelines, width);
    // Exit clause: ~1 min per 10⁸ points from LAZ; 30 min is far beyond.
    let budget = ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(1800));
    let mut points = Vec::new();
    let stats = source.visit(
        Some(&corridor),
        Some(&classes),
        &budget,
        &mut |x, y, z, c| points.push(([x, y, z], c)),
    )?;
    println!(
        "sheet LiDAR: {} — {} points of the classes {classes:?} within ±{width} m ({} decoded)",
        source.describe(),
        points.len(),
        stats.points_read
    );
    Ok((options, points))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_and_versions_parse() {
        let f = fields(
            None,
            &["project_no=P-1".into(), "remarks@2=see note".into()],
            &["A;2026-10-20;first issue;AB;CD".into()],
        )
        .unwrap();
        assert_eq!(f.values["project_no"], "P-1");
        assert_eq!(f.per_sheet["remarks"][&2], "see note");
        assert_eq!(f.revisions[0].checked, "CD");
        assert!(fields(None, &["nothing".into()], &[]).is_err());
        assert_eq!(
            parse_version("as built=existing").unwrap(),
            ("as built".into(), "existing".into(), "{name}".into())
        );
        assert_eq!(
            parse_version("other.ohl.json=other:{name} {voltage}")
                .unwrap()
                .2,
            "{name} {voltage}"
        );
        assert!(parse_version("=x").is_err());
        assert!(resolve("default", None, &[]).is_ok());
        assert!(
            resolve("no-such", None, &[])
                .unwrap_err()
                .contains("no-such")
        );
    }
}
