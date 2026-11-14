// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl staking`: the staking table of a line ([`ohl_line::staking`]) —
//! every structure's station, offset, line angle, spans, coordinates,
//! ground and bearing, and the feet of the typed structures.

use std::path::PathBuf;

use ohl_core::files::shipped_library;

/// Arguments of `ohl staking`.
#[derive(clap::Args)]
pub struct StakingArgs {
    /// Line file (.ohl.json) or project file (.ohl).
    file: PathBuf,
    /// Structure type directory (.ohls) for the feet (default: the
    /// `Structures` folder next to the line).
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator assembly library directory (.ohli) (default: the shipped
    /// libraries/insulators).
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Terrain file (survey points CSV / XYZ, LandXML) for the ground
    /// (default: a project's surface of the checks; none: no ground).
    #[arg(long, conflicts_with = "surface")]
    terrain: Option<PathBuf>,
    /// A terrain surface of the project file for the ground.
    #[arg(long)]
    surface: Option<String>,
    /// Write the structures as CSV here.
    #[arg(long)]
    csv: Option<PathBuf>,
    /// Write the feet as CSV here.
    #[arg(long)]
    feet_csv: Option<PathBuf>,
    /// Write the whole table as JSON here.
    #[arg(long)]
    json: Option<PathBuf>,
}

/// `ohl staking`.
pub fn run(a: &StakingArgs) -> Result<(), String> {
    let structures = crate::project::structures_dir(&a.file, a.structures.as_deref());
    let insulators = a
        .insulators
        .clone()
        .unwrap_or_else(|| shipped_library("insulators"));
    let (line, types) = crate::project::load_resolved(&a.file, &structures, &insulators)?;
    let project = a.file.extension().is_some_and(|e| e == "ohl");
    let terrain = match (&a.terrain, &a.surface) {
        (Some(path), _) => Some(ohl_terrain::Terrain::load(path).map_err(|e| e.to_string())?),
        (None, Some(name)) => {
            let session = ohl_session::Session::open(&a.file).map_err(|e| e.to_string())?;
            let (points, triangles) = session.surface(name).map_err(|e| e.to_string())?;
            Some(ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?)
        }
        (None, None) if project => {
            let session = ohl_session::Session::open(&a.file).map_err(|e| e.to_string())?;
            match session.setting("app.check_surface").ok().flatten() {
                Some(name) => {
                    let (points, triangles) = session.surface(&name).map_err(|e| e.to_string())?;
                    println!("ground: the project's surface of the checks {name}");
                    Some(
                        ohl_terrain::Terrain::from_mesh(points, triangles)
                            .map_err(|e| e.to_string())?,
                    )
                }
                None => None,
            }
        }
        (None, None) => None,
    };
    if terrain.is_none() {
        println!(
            "ground: none given — the ground columns are left empty (give --terrain or --surface)"
        );
    }
    let ground = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
    let typed = line.structures.iter().any(|s| s.structure_type.is_some());
    let table = ohl_line::staking::staking(&line, typed.then_some(&types), &ground);
    print!("{}", table.text());
    for (path, text) in [(&a.csv, table.csv()), (&a.feet_csv, table.feet_csv())] {
        if let Some(path) = path {
            std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("wrote {}", path.display());
        }
    }
    if let Some(path) = &a.json {
        let text = serde_json::to_string_pretty(&table).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
