// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl project …`: the project file (`.ohl`) through the session.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use ohl_session::{Applied, Command, Kind, Role, Session};

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Load a line file (.ohl.json) into a project file (created if
    /// missing; the first user becomes its admin).
    Import {
        /// Line file.
        line: PathBuf,
        /// Project file (.ohl).
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
        /// New project file (.ohl; must not exist).
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
    /// Read a file of any line format (see ohl formats: structure points
    /// from CSV, GeoJSON or shapefiles, LandXML, OHL XML…) into a project
    /// file: a new project gets the line, one holding a line gets it
    /// updated (structures moved or added, the route) — one logged command.
    ImportFile {
        /// The file.
        file: PathBuf,
        /// Project file (.ohl; created if missing).
        #[arg(long)]
        into: PathBuf,
        /// User (the admin of a new project).
        #[arg(long)]
        user: String,
        /// Format id (default: from the file extension).
        #[arg(long)]
        format: Option<String>,
        /// Format option KEY=VALUE (repeatable), e.g. type=tangent,
        /// crs=EPSG:2056.
        #[arg(long = "option")]
        options: Vec<String>,
        /// File-format plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
    },
    /// Write the project's line as a line file (.ohl.json).
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
        /// Wire temperature for --solve, °C.
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
        /// Or a tile LEVEL/X/Y (as listed by `ohl project tiles`).
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
            let line = ohl_line::Line::load(&line).map_err(|e| e.to_string())?;
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
            let made = ohl_io::projects::create_project(
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
            for w in p.line.system_warnings() {
                println!("  - warning: {w}");
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
        ProjectCommand::ImportFile {
            file,
            into,
            user,
            format,
            options,
            plugin,
        } => {
            let mut r = crate::io_run::registry(&plugin)?;
            let (a, line) = ohl_io::neutral::import_into_project(
                &mut r,
                &file,
                format.as_deref(),
                &crate::io_run::options(&options)?,
                &into,
                &user,
            )?;
            println!(
                "read {}: {} structures, {} sections, {} routes into {}",
                file.display(),
                line.structures.len(),
                line.sections.len(),
                line.alignment_views().len(),
                into.display()
            );
            report(&a);
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
            let library = library.unwrap_or_else(|| ohl_core::files::shipped_library("cables"));
            let library = ohl_cables::Library::load(&library).map_err(|e| e.to_string())?;
            let case = ohl_weather::WeatherCase::still(
                "state",
                ohl_core::units::Temperature::from_celsius(temperature),
            );
            let started = std::time::Instant::now();
            let mut live = ohl_session::Live::new(s, library, &case);
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
            let kind: ohl_session::Kind =
                serde_json::from_value(serde_json::Value::String(kind.clone())).map_err(|_| {
                    format!("unknown kind \"{kind}\" (structure, section, jumper, route)")
                })?;
            let mut s = open(&project)?;
            match s.apply(&user, Command::Delete { kind, id, confirm }) {
                Ok(a) => report(&a),
                Err(ohl_session::SessionError::NeedsConfirmation(i)) => {
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

/// Whether `path` names a project file (`.ohl`).
pub fn is_project(path: &Path) -> bool {
    ohl_core::files::is_project_file(path)
}

/// Opens an existing project file to read it (one that is missing is not
/// created). Opening upgrades a project file of an older build, as the
/// desktop application does.
pub fn open_existing(path: &Path) -> Result<Session, String> {
    if !path.is_file() {
        return Err(format!("{}: no such project file", path.display()));
    }
    Session::open(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The library kinds a project keeps with its line (as the desktop
/// application's `PROJECT_LIBRARIES`), each written to its folder of the
/// working copies: poles, structure models and load tables go with the
/// structure types.
const PROJECT_LIBRARY_KINDS: [(&str, &str); 13] = [
    ("cables", "cables"),
    ("structures", "structures"),
    ("insulators", "insulators"),
    ("weather", "weather"),
    ("loadcases", "loadcases"),
    ("rules", "rules"),
    ("galloping", "galloping"),
    ("ambient", "ambient"),
    ("costs", "costs"),
    ("poles", "structures"),
    ("femodels", "structures"),
    ("sections", "sections"),
    ("loads", "structures"),
];

/// The libraries given on the command line (`None`: not given).
#[derive(Default, Clone, Copy)]
pub struct GivenLibraries<'a> {
    /// `--library`: the wire library.
    pub cables: Option<&'a Path>,
    /// `--structures`.
    pub structures: Option<&'a Path>,
    /// `--insulators`.
    pub insulators: Option<&'a Path>,
    /// `--sections`.
    pub sections: Option<&'a Path>,
}

/// The libraries of `ohl line` and `ohl zones`: for a project file, those
/// it keeps with its current line (as the desktop application reads
/// them), written to a working folder that is removed when this is
/// dropped; a library given on the command line wins, and a kind the
/// project does not hold is looked up as for a line file (the shipped
/// wire and insulator libraries, the `Structures` and `Sections` folders
/// next to the file). Nothing is written to the project file.
pub struct Libraries {
    /// The working folder of the project's files (removed on drop).
    work: Option<PathBuf>,
    /// Wire library (file or folder).
    pub cables: PathBuf,
    /// Structure type folder.
    pub structures: PathBuf,
    /// Insulator assembly folder.
    pub insulators: PathBuf,
    /// Cross-section library, if any.
    pub sections: Option<PathBuf>,
    /// Where each came from, one line per kind: "cables: the project's
    /// (2 files)".
    pub sources: Vec<String>,
}

impl Drop for Libraries {
    fn drop(&mut self) {
        if let Some(w) = &self.work {
            let _ = std::fs::remove_dir_all(w);
        }
    }
}

/// "1 file", "2 files".
fn files(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

impl Libraries {
    /// The libraries of `file` (a line or project file) with those given.
    pub fn resolve(file: &Path, given: GivenLibraries) -> Result<Self, String> {
        use ohl_core::files::shipped_library;
        let mut stored: Vec<(&str, usize)> = Vec::new();
        let mut work = None;
        if is_project(file) {
            let session = open_existing(file)?;
            let mut held = Vec::new();
            for (kind, folder) in PROJECT_LIBRARY_KINDS {
                let f = session.library_files(kind).map_err(|e| e.to_string())?;
                if !f.is_empty() {
                    stored.push((kind, f.len()));
                    held.push((folder, f));
                }
            }
            if !held.is_empty() {
                let dir = working_folder(file);
                let _ = std::fs::remove_dir_all(&dir);
                // Removed on drop from here on, also when writing fails.
                work = Some(dir.clone());
                let write = || -> Result<(), String> {
                    for (folder, f) in &held {
                        let d = dir.join(folder);
                        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
                        for (name, text) in f {
                            if !ohl_session::safe_file_name(name) {
                                return Err(format!(
                                    "library file \"{name}\": not a plain file name"
                                ));
                            }
                            std::fs::write(d.join(name), text)
                                .map_err(|e| format!("{}: {e}", d.join(name).display()))?;
                        }
                    }
                    Ok(())
                };
                if let Err(e) = write() {
                    let _ = std::fs::remove_dir_all(&dir);
                    return Err(e);
                }
            }
        }
        let held = |kind: &str| stored.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n);
        let mut sources = Vec::new();
        let project = |kind: &str| -> Option<(PathBuf, String)> {
            let n = held(kind)?;
            let dir = work.as_ref()?.join(kind);
            Some((dir, format!("{kind}: the project's ({})", files(n))))
        };
        let given_text =
            |kind: &str, p: &Path, flag: &str| format!("{kind}: {} ({flag})", p.display());
        let (cables, line) = match (given.cables, project("cables")) {
            (Some(p), _) => (p.to_path_buf(), given_text("cables", p, "--library")),
            (None, Some(x)) => x,
            (None, None) => {
                let p = shipped_library("cables");
                let t = format!("cables: {} (shipped)", p.display());
                (p, t)
            }
        };
        sources.push(line);
        let (structures, line) = match (given.structures, project("structures")) {
            (Some(p), _) => (p.to_path_buf(), given_text("structures", p, "--structures")),
            (None, Some((dir, mut t))) => {
                // Poles, structure models and load tables beside them.
                for kind in ["poles", "femodels", "loads"] {
                    if let Some(n) = held(kind) {
                        t.push_str(&format!(", {kind} {}", files(n)));
                    }
                }
                (dir, t)
            }
            (None, None) => {
                let p = structures_dir(file, None);
                let t = format!("structures: {} (next to the file)", p.display());
                (p, t)
            }
        };
        sources.push(line);
        let (insulators, line) = match (given.insulators, project("insulators")) {
            (Some(p), _) => (p.to_path_buf(), given_text("insulators", p, "--insulators")),
            (None, Some(x)) => x,
            (None, None) => {
                let p = shipped_library("insulators");
                let t = format!("insulators: {} (shipped)", p.display());
                (p, t)
            }
        };
        sources.push(line);
        let (sections, line) = match (given.sections, project("sections")) {
            (Some(p), _) => (
                Some(p.to_path_buf()),
                given_text("sections", p, "--sections"),
            ),
            (None, Some((dir, t))) => (Some(dir), t),
            (None, None) => match file.parent().map(|d| d.join("Sections")) {
                Some(d) if d.is_dir() => {
                    let t = format!("sections: {} (next to the file)", d.display());
                    (Some(d), t)
                }
                _ => (None, "sections: none".to_owned()),
            },
        };
        sources.push(line);
        Ok(Libraries {
            work,
            cables,
            structures,
            insulators,
            sections,
            sources,
        })
    }

    /// A file named on the command line: the path when it exists, else
    /// the project's own file of `kind` of that name (its working copy);
    /// with "(the project's)" when it is one.
    pub fn named(&self, kind: &str, given: &Path) -> (PathBuf, bool) {
        if !given.exists()
            && let (Some(work), Some(name)) = (&self.work, given.file_name())
        {
            let own = work.join(kind).join(name);
            if own.is_file() {
                return (own, true);
            }
        }
        (given.to_path_buf(), false)
    }

    /// A file for the summary: "NAME (the project's)" for a working copy
    /// of one of the project's files, else its path.
    pub fn describe(&self, p: &Path) -> String {
        match (&self.work, p.file_name()) {
            (Some(w), Some(name)) if p.starts_with(w) => {
                format!("{} (the project's)", name.to_string_lossy())
            }
            _ => p.display().to_string(),
        }
    }

    /// The project's first file of `kind` (its working copy), if it holds
    /// one.
    pub fn first(&self, kind: &str) -> Option<PathBuf> {
        let dir = self.work.as_ref()?.join(kind);
        let mut f: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.is_file())
            .collect();
        f.sort();
        f.into_iter().next()
    }

    /// Prints where each library came from.
    pub fn print(&self, kinds: &[&str]) {
        println!("libraries:");
        for s in &self.sources {
            if kinds.iter().any(|k| s.starts_with(&format!("{k}:"))) {
                println!("  {s}");
            }
        }
    }
}

/// A working folder of its own for the libraries of project `file`.
fn working_folder(file: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let stem = file
        .file_stem()
        .map_or("project".into(), |s| s.to_string_lossy().into_owned());
    std::env::temp_dir().join(format!(
        "ohl-cli-libraries-{stem}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Loads a line from a line file or a project file (`.ohl`).
pub fn load_line(path: &Path) -> Result<ohl_line::Line, String> {
    if is_project(path) {
        let s = open_existing(path)?;
        if !s.has_line() {
            return Err(format!("{}: the project holds no line", path.display()));
        }
        Ok(s.export())
    } else {
        ohl_line::Line::load(path).map_err(|e| e.to_string())
    }
}

/// The structure type folder of `ohl line` and `ohl zones`: given, else
/// the `Structures` folder next to the line or project file.
pub fn structures_dir(file: &Path, given: Option<&Path>) -> PathBuf {
    match given {
        Some(d) => d.to_path_buf(),
        None => file.parent().unwrap_or(Path::new(".")).join("Structures"),
    }
}

/// Loads a line from a line or project file ([`load_line`]) with its typed
/// structures resolved from the structure types of `structures` and the
/// insulator assemblies of `insulators` (the structure types read only when
/// a structure has a type; the insulators always: they give the strings
/// without their own their weight, stiffness and wind area, as in the app);
/// the structure types read.
pub fn load_resolved(
    file: &Path,
    structures: &Path,
    insulators: &Path,
) -> Result<(ohl_line::Line, ohl_structures::StructureLibrary), String> {
    let line = load_line(file)?;
    // Resolved also without typed structures, as the app does: the strings'
    // weights, stiffnesses and wind areas come from their insulators.
    let types = if line.structures.iter().any(|s| s.structure_type.is_some()) {
        ohl_structures::StructureLibrary::load_dir(structures).map_err(|e| e.to_string())?
    } else {
        ohl_structures::StructureLibrary::default()
    };
    let assemblies =
        ohl_structures::InsulatorLibrary::load_dir(insulators).map_err(|e| e.to_string())?;
    let line = line
        .resolve(&types, &assemblies)
        .map_err(|e| e.to_string())?;
    Ok((line, types))
}

fn kind_name(k: Kind) -> &'static str {
    k.as_str()
}

/// `ohl project tiles`.
fn tiles(path: &Path, level: i8) -> Result<(), String> {
    use ohl_core::grid::{Bounds, Grid, Tile};
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
    let all = ohl_session::spatial::object_bounds(s.line());
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
    let store = ohl_pointcloud::Store::open(path).map_err(|e| e.to_string())?;
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

/// `ohl project query`.
fn query(path: &Path, area: Option<&str>, tile: Option<&str>) -> Result<(), String> {
    use ohl_core::grid::{Bounds, Tile};
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
