// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols project …`: the project file (`.ols`) through the session.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use ols_session::{Applied, Command, Kind, Role, Session};

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Load a line file (.ols.json) into a project file (created if
    /// missing; the first user becomes its admin).
    Import {
        /// Line file.
        line: PathBuf,
        /// Project file (.ols).
        #[arg(long)]
        into: PathBuf,
        /// User name.
        #[arg(long)]
        user: String,
    },
    /// Create a project from another program's file (a project plugin's
    /// format): the line, its libraries; prints what was read and left
    /// out.
    ImportFrom {
        /// The file.
        file: PathBuf,
        /// New project file (.ols; must not exist).
        #[arg(long)]
        into: PathBuf,
        /// User (the project's admin).
        #[arg(long)]
        user: String,
        /// Format id (default: from the file extension).
        #[arg(long)]
        format: Option<String>,
        /// Format option KEY=VALUE (repeatable).
        #[arg(long = "option")]
        options: Vec<String>,
        /// Project plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
        /// Clip the sections in: each span keeps the length of its
        /// stringing condition (as the source program solves it).
        #[arg(long)]
        clip_in: bool,
    },
    /// Write the project's line as a line file (.ols.json).
    Export {
        /// Project file.
        project: PathBuf,
        /// Output line file.
        #[arg(long)]
        out: PathBuf,
    },
    /// Apply a command (JSON, or @FILE with a JSON command).
    Apply {
        /// Project file.
        project: PathBuf,
        /// User name.
        #[arg(long)]
        user: String,
        /// Command, e.g. '{"op": "move_structure", "id": "T4", "position": [x, y, z]}'.
        command: String,
        /// Solve the line before, then re-solve only what the command
        /// touched, and report both (what the application does live).
        #[arg(long)]
        solve: bool,
        /// Wire library for --solve (default: the shipped libraries/cables).
        #[arg(long)]
        library: Option<PathBuf>,
        /// Cable temperature for --solve, °C.
        #[arg(long, default_value_t = 15.0)]
        temperature: f64,
    },
    /// Delete an object (structure, section, jumper, route). When it takes
    /// others with it, lists them and changes nothing unless --confirm.
    Delete {
        /// Project file.
        project: PathBuf,
        /// User name.
        #[arg(long)]
        user: String,
        /// Kind: structure, section, jumper or route.
        kind: String,
        /// Id.
        id: String,
        /// Delete it with everything listed.
        #[arg(long)]
        confirm: bool,
    },
    /// Undo the user's last command.
    Undo {
        /// Project file.
        project: PathBuf,
        /// User name.
        #[arg(long)]
        user: String,
    },
    /// Redo the user's last undo.
    Redo {
        /// Project file.
        project: PathBuf,
        /// User name.
        #[arg(long)]
        user: String,
    },
    /// Admin: reverse log entry SEQ, or with --after everything after it.
    Rollback {
        /// Project file.
        project: PathBuf,
        /// Admin user.
        #[arg(long)]
        user: String,
        /// Log entry.
        seq: i64,
        /// Bring the model back to its state just after SEQ.
        #[arg(long)]
        after: bool,
    },
    /// The operation log.
    Log {
        /// Project file.
        project: PathBuf,
    },
    /// Tiles of the project's chunk grid: objects touching each tile and
    /// stored points per tile.
    Tiles {
        /// Project file.
        project: PathBuf,
        /// Grid level (0 = base tiles, 256 m; 1 = 512 m; -1 = 128 m …).
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        level: i8,
    },
    /// Objects in an area, from the project's spatial index.
    Query {
        /// Project file.
        project: PathBuf,
        /// Area XMIN,YMIN,XMAX,YMAX (m, project coordinates).
        #[arg(long, allow_hyphen_values = true, conflicts_with = "tile")]
        area: Option<String>,
        /// Or a tile LEVEL/X/Y (as listed by `ols project tiles`).
        #[arg(long, allow_hyphen_values = true)]
        tile: Option<String>,
    },
    /// Admin: add a user or change a role (viewer, designer, admin).
    User {
        /// Project file.
        project: PathBuf,
        /// Admin doing it.
        #[arg(long)]
        by: String,
        /// User to add or change.
        name: String,
        /// Role.
        role: String,
    },
    /// Version control and collaboration.
    #[command(flatten)]
    Versions(crate::versions::VersionCommand),
}

fn report(a: &Applied) {
    println!(
        "entry {}: {} object(s) changed; sections to recalculate: {}",
        a.seq,
        a.objects.len(),
        match a.sections.len() {
            0 => "none".to_owned(),
            n if n <= 12 => a.sections.join(", "),
            n => format!("{} … ({n} sections)", a.sections[..10].join(", ")),
        }
    );
}

fn open(path: &Path) -> Result<Session, String> {
    Session::open(path).map_err(|e| e.to_string())
}

pub fn run(command: ProjectCommand) -> Result<(), String> {
    match command {
        ProjectCommand::Import { line, into, user } => {
            let line = ols_line::Line::load(&line).map_err(|e| e.to_string())?;
            let mut s = open(&into)?;
            let a = s.import(&line, &user).map_err(|e| e.to_string())?;
            println!(
                "imported \"{}\": {} structures, {} sections, {} jumpers into {}",
                line.name,
                line.structures.len(),
                line.sections.len(),
                line.jumpers.len(),
                into.display()
            );
            report(&a);
        }
        ProjectCommand::ImportFrom {
            file,
            into,
            user,
            format,
            options,
            plugin,
            clip_in,
        } => {
            let started = std::time::Instant::now();
            let mut r = crate::io_run::registry(&plugin)?;
            let made = ols_io::projects::create_project(
                &mut r,
                &file,
                format.as_deref(),
                &crate::io_run::options(&options)?,
                &into,
                &user,
                clip_in,
            )?;
            let p = &made.project;
            for n in &p.notes {
                println!("  - {n}");
            }
            println!(
                "created {}: {} structures, {} sections, {} jumpers, {} library files, in {:.1?}",
                made.path.display(),
                p.line.structures.len(),
                p.line.sections.len(),
                p.line.jumpers.len(),
                p.libraries.len(),
                started.elapsed()
            );
            report(&made.applied);
        }
        ProjectCommand::Export { project, out } => {
            let s = open(&project)?;
            if !s.has_line() {
                return Err("the project holds no line".into());
            }
            std::fs::write(&out, s.export().to_json())
                .map_err(|e| format!("{}: {e}", out.display()))?;
            println!("wrote {}", out.display());
        }
        ProjectCommand::Apply {
            project,
            user,
            command,
            solve,
            library,
            temperature,
        } => {
            let text = match command.strip_prefix('@') {
                Some(file) => std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?,
                None => command,
            };
            let command: Command =
                serde_json::from_str(&text).map_err(|e| format!("command: {e}"))?;
            let s = open(&project)?;
            if !solve {
                let mut s = s;
                report(&s.apply(&user, command).map_err(|e| e.to_string())?);
                return Ok(());
            }
            let library = library.unwrap_or_else(|| ols_core::files::shipped_library("cables"));
            let library = ols_cables::Library::load(&library).map_err(|e| e.to_string())?;
            let case = ols_weather::WeatherCase::still(
                "state",
                ols_core::units::Temperature::from_celsius(temperature),
            );
            let started = std::time::Instant::now();
            let mut live = ols_session::Live::new(s, library, &case);
            let full = started.elapsed();
            let started = std::time::Instant::now();
            let (applied, update) = live.apply(&user, command).map_err(|e| e.to_string())?;
            let incremental = started.elapsed();
            report(&applied);
            let failed = live
                .solved()
                .results(live.session().line())
                .iter()
                .filter(|(_, r)| r.is_err())
                .count();
            println!(
                "full solve of {} sections: {full:.1?}; after the command: {} re-solved, {} kept, {} dropped, {} jumpers in {incremental:.1?} (command, log and solve){}",
                update.solved + update.kept,
                update.solved,
                update.kept,
                update.dropped,
                update.jumpers,
                if failed > 0 {
                    format!("; {failed} sections fail to solve")
                } else {
                    String::new()
                }
            );
        }
        ProjectCommand::Delete {
            project,
            user,
            kind,
            id,
            confirm,
        } => {
            let kind: ols_session::Kind =
                serde_json::from_value(serde_json::Value::String(kind.clone())).map_err(|_| {
                    format!("unknown kind \"{kind}\" (structure, section, jumper, route)")
                })?;
            let mut s = open(&project)?;
            match s.apply(&user, Command::Delete { kind, id, confirm }) {
                Ok(a) => report(&a),
                Err(ols_session::SessionError::NeedsConfirmation(i)) => {
                    println!("Deleting {} {} would also affect:", i.kind.as_str(), i.id);
                    for (title, items) in i.groups() {
                        if !items.is_empty() {
                            println!("  {title}:");
                            for x in items {
                                println!("    - {x}");
                            }
                        }
                    }
                    println!(
                        "Nothing was changed. Repeat with --confirm to apply all of this (it is logged and can be undone)."
                    );
                    return Err("confirmation needed".into());
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        ProjectCommand::Undo { project, user } => {
            report(&open(&project)?.undo(&user).map_err(|e| e.to_string())?);
        }
        ProjectCommand::Redo { project, user } => {
            report(&open(&project)?.redo(&user).map_err(|e| e.to_string())?);
        }
        ProjectCommand::Rollback {
            project,
            user,
            seq,
            after,
        } => {
            let mut s = open(&project)?;
            if after {
                match s.rollback_after(&user, seq).map_err(|e| e.to_string())? {
                    Some(a) => report(&a),
                    None => println!("nothing to roll back"),
                }
            } else {
                report(&s.rollback(&user, seq).map_err(|e| e.to_string())?);
            }
        }
        ProjectCommand::Tiles { project, level } => tiles(&project, level)?,
        ProjectCommand::Query {
            project,
            area,
            tile,
        } => query(&project, area.as_deref(), tile.as_deref())?,
        ProjectCommand::Log { project } => {
            let s = open(&project)?;
            println!(
                "{:>5}  {:<24} {:<10} {:<9} {:<22} objects",
                "seq", "time (UTC)", "user", "entry", "state"
            );
            for e in s.log().map_err(|e| e.to_string())? {
                let state = match (e.undoes, e.undone_by) {
                    (_, Some(by)) => format!("reversed by {by}"),
                    (Some(u), None) => format!("reverses {u}"),
                    (None, None) => String::new(),
                };
                let objects: Vec<String> = e
                    .objects
                    .iter()
                    .take(6)
                    .map(|(k, id)| {
                        format!(
                            "{}:{id}",
                            serde_json::to_value(k).unwrap().as_str().unwrap_or("")
                        )
                    })
                    .collect();
                println!(
                    "{:>5}  {:<24} {:<10} {:<9} {:<22} {}{}",
                    e.seq,
                    e.time,
                    e.user,
                    e.kind,
                    state,
                    objects.join(" "),
                    if e.objects.len() > 6 {
                        format!(" … ({} objects)", e.objects.len())
                    } else {
                        String::new()
                    }
                );
                if let Some(note) = &e.note {
                    println!("{:>7}{note}", "");
                }
            }
        }
        ProjectCommand::Versions(c) => crate::versions::run(c)?,
        ProjectCommand::User {
            project,
            by,
            name,
            role,
        } => {
            let s = open(&project)?;
            s.set_user(&by, &name, Role::parse(&role).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            for (n, r) in s.users().map_err(|e| e.to_string())? {
                println!("{n}: {r:?}");
            }
        }
    }
    Ok(())
}

/// Loads a line from a line file or a project file (`.ols`).
pub fn load_line(path: &Path) -> Result<ols_line::Line, String> {
    if path.extension().is_some_and(|e| e == "ols") {
        let s = open(path)?;
        if !s.has_line() {
            return Err(format!("{}: the project holds no line", path.display()));
        }
        Ok(s.export())
    } else {
        ols_line::Line::load(path).map_err(|e| e.to_string())
    }
}

fn kind_name(k: Kind) -> &'static str {
    k.as_str()
}

/// `ols project tiles`.
fn tiles(path: &Path, level: i8) -> Result<(), String> {
    use ols_core::grid::{Bounds, Grid, Tile};
    use std::collections::BTreeMap;
    if !(Grid::MIN_LEVEL..=Grid::MAX_LEVEL).contains(&level) {
        return Err(format!(
            "level {level}: from {} to {}",
            Grid::MIN_LEVEL,
            Grid::MAX_LEVEL
        ));
    }
    let started = std::time::Instant::now();
    let s = open(path)?;
    let grid = *s.grid();
    let homes = s.home_tiles();
    let mut by_level: BTreeMap<i8, usize> = BTreeMap::new();
    for (t, kinds) in &homes {
        *by_level.entry(t.level).or_default() += kinds.values().sum::<usize>();
    }
    println!(
        "grid: {} m tiles at level 0; level {level}: {} m",
        grid.size,
        grid.tile_size(level)
    );
    println!(
        "objects by home level: {}",
        by_level
            .iter()
            .map(|(l, n)| format!("{l}: {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    // Objects touching each tile of the level.
    let all = ols_session::spatial::object_bounds(s.line());
    // Per tile: objects by kind, points by cloud.
    type Row = (BTreeMap<Kind, usize>, Vec<(String, u64)>);
    let mut rows: BTreeMap<Tile, Row> = BTreeMap::new();
    if let Some(extent) = all.values().copied().reduce(|a, b| a.union(&b)) {
        let range = grid.covering(&extent, level);
        if range.len() > 100_000 {
            return Err(format!(
                "{} tiles at level {level}: choose a coarser level",
                range.len()
            ));
        }
        // Tiles touched by an object (the line's own alignment, `meta`,
        // spans the whole line and is not listed).
        for (_, b) in all.iter().filter(|((k, _), _)| *k != Kind::Meta) {
            for t in grid.covering(b, level).tiles() {
                rows.entry(t).or_default();
            }
        }
        for (t, row) in rows.iter_mut() {
            for (k, _, _) in s.objects_in(&grid.bounds(t)).map_err(|e| e.to_string())? {
                *row.0.entry(k).or_default() += 1;
            }
        }
    }
    // Stored point clouds in the same file.
    let store = ols_pointcloud::Store::open(path).map_err(|e| e.to_string())?;
    let clouds = store.clouds().map_err(|e| e.to_string())?;
    for c in &clouds {
        let counts = store.tiles(c, &grid, level).map_err(|e| e.to_string())?;
        if !counts.exact {
            println!(
                "point cloud {}: {} m chunks do not nest in {} m tiles (counted by chunk centre)",
                c.name,
                c.chunk_size,
                grid.tile_size(level)
            );
        }
        for (t, n) in counts.tiles {
            rows.entry(t).or_default().1.push((c.name.clone(), n));
        }
    }
    let kinds = [Kind::Structure, Kind::Section, Kind::Jumper, Kind::Route];
    print!("  {:<20} {:>16} {:>16}", "tile", "x min", "y min");
    for k in kinds {
        print!(" {:>10}", format!("{}s", kind_name(k)));
    }
    if !clouds.is_empty() {
        print!("  points");
    }
    println!();
    for (t, (objects, points)) in &rows {
        let b: Bounds = grid.bounds(t);
        print!(
            "  {:<20} {:>16.1} {:>16.1}",
            t.to_string(),
            b.min[0],
            b.min[1]
        );
        for k in kinds {
            print!(" {:>10}", objects.get(&k).copied().unwrap_or(0));
        }
        for (name, n) in points {
            print!("  {name}: {n}");
        }
        println!();
    }
    println!("{} tiles in {:.1?}", rows.len(), started.elapsed());
    Ok(())
}

/// `ols project query`.
fn query(path: &Path, area: Option<&str>, tile: Option<&str>) -> Result<(), String> {
    use ols_core::grid::{Bounds, Tile};
    let s = open(path)?;
    let area = match (area, tile) {
        (Some(text), _) => {
            let v: Vec<f64> = text
                .split(',')
                .map(|x| x.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| format!("--area \"{text}\": use XMIN,YMIN,XMAX,YMAX"))?;
            let [x0, y0, x1, y1] = v[..] else {
                return Err(format!("--area \"{text}\": use XMIN,YMIN,XMAX,YMAX"));
            };
            Bounds::new([x0, y0], [x1, y1])
        }
        (None, Some(key)) => s.grid().bounds(&Tile::parse(key)?),
        (None, None) => return Err("give --area or --tile".into()),
    };
    let started = std::time::Instant::now();
    let found = s.objects_in(&area).map_err(|e| e.to_string())?;
    let elapsed = started.elapsed();
    println!(
        "area {:.1},{:.1} – {:.1},{:.1}: {} objects ({elapsed:.1?})",
        area.min[0],
        area.min[1],
        area.max[0],
        area.max[1],
        found.len()
    );
    println!(
        "  {:<10} {:<20} {:<16} plan bounds",
        "kind", "id", "home tile"
    );
    for (k, id, e) in found {
        let b = e.bounds;
        println!(
            "  {:<10} {:<20} {:<16} {:.1},{:.1} – {:.1},{:.1}",
            kind_name(k),
            id,
            e.tile.to_string(),
            b.min[0],
            b.min[1],
            b.max[0],
            b.max[1]
        );
    }
    Ok(())
}
