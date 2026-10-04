// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols surface`: terrain surfaces (TINs) kept in a project file.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use ols_session::Session;

/// Terrain surface commands.
#[derive(Subcommand)]
pub enum SurfaceCommand {
    /// The surfaces of a project.
    List {
        /// Project file (.ols).
        project: PathBuf,
    },
    /// Read a file's surfaces into a project (LandXML, OBJ, survey points
    /// CSV/XYZ triangulated, or a terrain plugin's format).
    Import {
        /// File to read.
        file: PathBuf,
        /// Project file (.ols).
        #[arg(long)]
        into: PathBuf,
        /// User (a designer of the project).
        #[arg(long)]
        user: String,
        /// Name of the surface (default: its own, else the file's); with
        /// several surfaces, the others get " 2", " 3"…
        #[arg(long)]
        name: Option<String>,
        /// Format id (default: from the file extension; see --list-formats).
        #[arg(long)]
        format: Option<String>,
        /// Format option KEY=VALUE (repeatable), e.g. max_edge=50.
        #[arg(long = "option")]
        options: Vec<String>,
        /// Terrain plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
        /// Replace a surface of the same name.
        #[arg(long)]
        replace: bool,
    },
    /// Write a surface as LandXML (.xml) or OBJ (.obj).
    Export {
        /// Project file (.ols).
        project: PathBuf,
        /// Surface name.
        name: String,
        /// Output file (.xml or .obj).
        out: PathBuf,
    },
    /// Rename a surface.
    Rename {
        /// Project file (.ols).
        project: PathBuf,
        /// Surface name.
        name: String,
        /// New name.
        new_name: String,
        /// User.
        #[arg(long)]
        user: String,
    },
    /// Remove a surface from the project.
    Delete {
        /// Project file (.ols).
        project: PathBuf,
        /// Surface name.
        name: String,
        /// User.
        #[arg(long)]
        user: String,
    },
    /// The surface formats (built-in and from plugins).
    Formats {
        /// Terrain plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
    },
}

fn open(project: &Path) -> Result<Session, String> {
    if !project.is_file() {
        return Err(format!("{}: no such project", project.display()));
    }
    Session::open(project).map_err(|e| format!("{}: {e}", project.display()))
}

/// Runs a surface command.
pub fn run(command: SurfaceCommand) -> Result<(), String> {
    match command {
        SurfaceCommand::List { project } => {
            let s = open(&project)?;
            let list = s.surfaces().map_err(|e| e.to_string())?;
            if list.is_empty() {
                println!("no surface");
            }
            for i in list {
                println!(
                    "{:<30} {:>10} points {:>10} triangles  {}  {} ({})",
                    i.name,
                    i.points,
                    i.triangles,
                    i.source,
                    i.time.get(..19).unwrap_or(&i.time),
                    i.user
                );
            }
            Ok(())
        }
        SurfaceCommand::Formats { plugin } => {
            let r = crate::io_run::registry(&plugin)?;
            for i in r.surface_importers() {
                println!(
                    "{:<20} {:<6} {:<22} {}",
                    i.id(),
                    i.extension(),
                    i.origin(),
                    i.description()
                );
            }
            Ok(())
        }
        SurfaceCommand::Import {
            file,
            into,
            user,
            name,
            format,
            options,
            plugin,
            replace,
        } => {
            let started = std::time::Instant::now();
            let mut r = crate::io_run::registry(&plugin)?;
            let id = format.unwrap_or_else(|| {
                file.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase()
            });
            let options = crate::io_run::options(&options)?;
            let importer = r
                .surface_importer(&id)
                .ok_or_else(|| format!("no surface format \"{id}\" (see ols surface formats)"))?;
            let data = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let stem = file
                .file_stem()
                .map_or("surface".into(), |s| s.to_string_lossy().into_owned());
            let meshes = importer
                .import(&data, &stem, &options)
                .map_err(|e| format!("{}: {e}", file.display()))?;
            let source = format!(
                "{} ({})",
                file.file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                importer.id()
            );
            println!(
                "read {} surface(s) in {:.1?}",
                meshes.len(),
                started.elapsed()
            );
            let s = open(&into)?;
            let existing: Vec<String> = s
                .surfaces()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|i| i.name)
                .collect();
            for (k, m) in meshes.into_iter().enumerate() {
                let base = match (&name, k) {
                    (Some(n), 0) => n.clone(),
                    (Some(n), k) => format!("{n} {}", k + 1),
                    (None, _) if m.name.is_empty() => stem.clone(),
                    (None, _) => m.name.clone(),
                };
                if existing.contains(&base) && !replace {
                    return Err(format!(
                        "a surface \"{base}\" exists already (--replace, or --name)"
                    ));
                }
                // Checked like any surface (indices, areas).
                let t = m.terrain().map_err(|e| format!("{base}: {e}"))?;
                s.put_surface(&user, &base, &source, t.points(), t.triangles())
                    .map_err(|e| e.to_string())?;
                println!(
                    "  {base}: {} points, {} triangles",
                    t.num_points(),
                    t.num_triangles()
                );
            }
            s.checkpoint().map_err(|e| e.to_string())?;
            println!("stored in {} in {:.1?}", into.display(), started.elapsed());
            Ok(())
        }
        SurfaceCommand::Export { project, name, out } => {
            let s = open(&project)?;
            let (points, triangles) = s.surface(&name).map_err(|e| e.to_string())?;
            let t =
                ols_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?;
            let text = match out.extension().and_then(|e| e.to_str()) {
                Some("obj") => ols_terrain::formats::write_obj(&name, &t),
                Some("xml") => ols_terrain::formats::write_landxml(&[(&name, &t)]),
                _ => return Err("write .xml (LandXML) or .obj".into()),
            };
            std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("{name} → {}", out.display());
            Ok(())
        }
        SurfaceCommand::Rename {
            project,
            name,
            new_name,
            user,
        } => open(&project)?
            .rename_surface(&user, &name, &new_name)
            .map_err(|e| e.to_string()),
        SurfaceCommand::Delete {
            project,
            name,
            user,
        } => open(&project)?
            .delete_surface(&user, &name)
            .map_err(|e| e.to_string()),
    }
}
