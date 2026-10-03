// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols project …`: the project file (`.ols`) through the session.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use ols_session::{Applied, Command, Role, Session};

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
        /// Wire library for --solve.
        #[arg(long, default_value = "libraries/cables")]
        library: PathBuf,
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
