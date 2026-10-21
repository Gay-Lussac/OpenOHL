// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structures points` and `ohl structures link`: a structure's
//! attachment points and the joints they are linked to, and a point
//! linked, relinked or unlinked in a line file (2026-10-21). In a project
//! file the same is the session command `link_point` / `unlink_point`
//! through `ohl project apply`.

use std::path::{Path, PathBuf};

use ohl_core::files::shipped_library;
use ohl_line::links::{linked, match_points, unlinked};
use ohl_line::{Line, Structure, StructurePoint};
use ohl_structures::{InsulatorLibrary, StructureLibrary};

/// `ohl structures points`.
#[derive(clap::Args)]
pub struct PointsArgs {
    /// Line file (.ohl.json) or project file (.ohl).
    file: PathBuf,
    /// The structure.
    #[arg(long)]
    structure: String,
    /// Match its attachment points to this type's joints instead (a type
    /// swap): the joint at each point's position, else the nearest attach
    /// joint offered with the offset left.
    #[arg(long)]
    to: Option<String>,
    /// With --to: the new type's extension.
    #[arg(long)]
    extension: Option<String>,
    /// Structure type folder (default: Structures next to the file).
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator assembly folder (default: the shipped libraries/insulators).
    #[arg(long)]
    insulators: Option<PathBuf>,
}

/// `ohl structures link`.
#[derive(clap::Args)]
pub struct LinkArgs {
    /// Line file (.ohl.json).
    file: PathBuf,
    /// The structure (it has a type).
    #[arg(long)]
    structure: String,
    /// The attachment point's label.
    #[arg(long)]
    point: String,
    /// Link it to this joint of the type.
    #[arg(long, conflicts_with_all = ["unlink", "nearest"])]
    joint: Option<String>,
    /// With --joint: from the joint to the point, structure axes (x right
    /// of the line, y ahead, z up), m, as "x,y,z".
    #[arg(long, requires = "joint")]
    offset: Option<String>,
    /// Link it to the joint at its position, else to the nearest attach
    /// joint at the offset left (it stays where it is).
    #[arg(long, conflicts_with = "unlink")]
    nearest: bool,
    /// Unlink it: it stays where it is, its structure's loads refused
    /// until it is linked again.
    #[arg(long)]
    unlink: bool,
    /// Structure type folder (default: Structures next to the file).
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator assembly folder (default: the shipped libraries/insulators).
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// The line file written (may be the one read).
    #[arg(long)]
    out: PathBuf,
}

fn libraries(
    file: &Path,
    structures: Option<&Path>,
    insulators: Option<&Path>,
) -> Result<(StructureLibrary, InsulatorLibrary), String> {
    let dir = crate::project::structures_dir(file, structures);
    let types = StructureLibrary::load_dir(&dir).map_err(|e| e.to_string())?;
    let insulators = insulators.map_or_else(|| shipped_library("insulators"), Path::to_path_buf);
    let insulators = if insulators.is_dir() {
        InsulatorLibrary::load_dir(&insulators).map_err(|e| e.to_string())?
    } else {
        InsulatorLibrary::default()
    };
    Ok((types, insulators))
}

fn m3(v: [f64; 3]) -> String {
    format!("({:.3}, {:.3}, {:.3})", v[0], v[1], v[2])
}

/// Prints a structure's attachment points with their links, or how they
/// match another type's joints.
pub fn points(args: &PointsArgs) -> Result<(), String> {
    let line = crate::project::load_line(&args.file)?;
    let (types, insulators) = libraries(
        &args.file,
        args.structures.as_deref(),
        args.insulators.as_deref(),
    )?;
    let resolved = line
        .resolve(&types, &insulators)
        .map_err(|e| e.to_string())?;
    let s = resolved
        .structure(&args.structure)
        .ok_or_else(|| format!("no structure {}", args.structure))?;
    match &args.to {
        None => {
            println!(
                "structure {} ({}): {} attachment point(s)",
                s.id,
                s.structure_type.as_deref().unwrap_or("no type"),
                s.points.len()
            );
            println!(
                "  point        joint        offset m (x right, y ahead, z up)   length m  state"
            );
            for p in &s.points {
                let state = if p.unlinked {
                    "unlinked (loads refused)"
                } else if s.structure_type.is_none() {
                    "its own joint"
                } else if p.joint.is_some() {
                    "linked"
                } else {
                    "its own joint (version 1)"
                };
                println!(
                    "  {:<12} {:<12} {:<36} {:>8.3}  {state}",
                    p.label,
                    p.joint.as_deref().unwrap_or("-"),
                    m3(p.offset),
                    p.offset_length()
                );
            }
        }
        Some(to) => {
            let t = types
                .get(to)
                .ok_or_else(|| format!("no structure type {to}"))?;
            let mut new = line
                .structure(&args.structure)
                .cloned()
                .ok_or_else(|| format!("no structure {}", args.structure))?;
            new.structure_type = Some(to.clone());
            new.extension = args.extension.clone();
            println!("structure {} matched to the joints of {to}:", s.id);
            for m in match_points(t, &new, &s.points) {
                let what = match (&m.exact, &m.nearest) {
                    (Some(j), _) => format!("exact: joint {j}"),
                    (None, Some(o)) => format!(
                        "offered: joint {} at offset {} m ({:.3} m)",
                        o.joint,
                        m3(o.offset),
                        o.distance
                    ),
                    (None, None) if m.tie => "two attach joints equally near: choose".into(),
                    (None, None) => "no attach joint".into(),
                };
                println!("  {:<12} {what}", m.label);
            }
        }
    }
    Ok(())
}

/// Parses "x,y,z" (m).
fn offset(text: &str) -> Result<[f64; 3], String> {
    let v: Vec<f64> = text
        .split(',')
        .map(|x| x.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("--offset \"{text}\": three numbers, m, as x,y,z"))?;
    <[f64; 3]>::try_from(v).map_err(|_| format!("--offset \"{text}\": three numbers, as x,y,z"))
}

/// The point as it will be stored.
fn new_point(
    args: &LinkArgs,
    t: &ohl_structures::StructureType,
    stored: &Structure,
    old: &StructurePoint,
) -> Result<StructurePoint, String> {
    if args.unlink {
        return Ok(unlinked(old));
    }
    if let Some(joint) = &args.joint {
        let o = args.offset.as_deref().map(offset).transpose()?;
        return linked(
            t,
            stored,
            Some(old),
            &old.label,
            joint,
            o.unwrap_or([0.0; 3]),
        );
    }
    if args.nearest {
        let m = &match_points(t, stored, std::slice::from_ref(old))[0];
        let choice = m.proposal().or_else(|| m.offer()).ok_or_else(|| {
            format!(
                "{}: no attach joint offered for {} (give --joint)",
                stored.id, old.label
            )
        })?;
        let ohl_line::links::PointChoice::Link { joint, offset } = choice else {
            unreachable!("a proposal or an offer links");
        };
        return linked(t, stored, Some(old), &old.label, &joint, offset);
    }
    Err("give --joint, --nearest or --unlink".into())
}

/// Links, relinks or unlinks an attachment point and writes the line.
pub fn link(args: &LinkArgs) -> Result<(), String> {
    if ohl_core::files::is_project_file(&args.file) {
        return Err(format!(
            "{}: a project file — apply the command link_point or unlink_point with \
             `ohl project apply`",
            args.file.display()
        ));
    }
    let mut line = Line::load(&args.file).map_err(|e| e.to_string())?;
    let (types, insulators) = libraries(
        &args.file,
        args.structures.as_deref(),
        args.insulators.as_deref(),
    )?;
    let resolved = line
        .resolve(&types, &insulators)
        .map_err(|e| e.to_string())?;
    let id = &args.structure;
    let s = resolved
        .structure(id)
        .ok_or_else(|| format!("no structure {id}"))?;
    let type_id = s
        .structure_type
        .as_deref()
        .ok_or_else(|| format!("{id} has no type: its attachment points are their own joints"))?;
    let t = types
        .get(type_id)
        .ok_or_else(|| format!("no structure type {type_id}"))?;
    let old = s
        .point(&args.point)
        .ok_or_else(|| format!("{id}: no attachment point {}", args.point))?;
    let stored = line.structure(id).expect("resolved from it").clone();
    let p = new_point(args, t, &stored, old)?;
    println!(
        "{id} {}: {}",
        p.label,
        match (&p.joint, p.unlinked) {
            (_, true) => "unlinked".to_owned(),
            (Some(j), false) => format!("joint {j}, offset {} m", m3(p.offset)),
            (None, false) => "linked".to_owned(),
        }
    );
    let s = line
        .structures
        .iter_mut()
        .find(|s| &s.id == id)
        .expect("resolved from it");
    match s.points.iter_mut().find(|q| q.label == p.label) {
        Some(q) => *q = p,
        None => s.points.push(p),
    }
    // Still whole.
    line.resolve(&types, &insulators)
        .map_err(|e| e.to_string())?;
    std::fs::write(&args.out, line.to_json()).map_err(|e| format!("{}: {e}", args.out.display()))
}
