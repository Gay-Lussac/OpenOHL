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
    },
    /// Delete a structure. Without --confirm, lists everything that would
    /// be deleted with it (attachments and insulators, sections, jumpers,
    /// routes) and changes nothing.
    DeleteStructure {
        /// Project file.
        project: PathBuf,
        /// User name.
        #[arg(long)]
        user: String,
        /// Structure id.
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
        if a.sections.is_empty() {
            "none".to_owned()
        } else {
            a.sections.join(", ")
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
        } => {
            let text = match command.strip_prefix('@') {
                Some(file) => std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?,
                None => command,
            };
            let command: Command =
                serde_json::from_str(&text).map_err(|e| format!("command: {e}"))?;
            let mut s = open(&project)?;
            report(&s.apply(&user, command).map_err(|e| e.to_string())?);
        }
        ProjectCommand::DeleteStructure {
            project,
            user,
            id,
            confirm,
        } => {
            let mut s = open(&project)?;
            match s.apply(&user, Command::DeleteStructure { id, confirm }) {
                Ok(a) => report(&a),
                Err(ols_session::SessionError::NeedsConfirmation(i)) => {
                    let list = |title: &str, v: &[String]| {
                        if !v.is_empty() {
                            println!("  {title}:");
                            for x in v {
                                println!("    - {x}");
                            }
                        }
                    };
                    println!("Deleting structure {} would also delete:", i.structure);
                    list("attachments and insulators", &i.attachments);
                    list("sections through it", &i.sections);
                    list("jumpers at it", &i.jumpers);
                    list("routes (fewer than two structures left)", &i.routes_deleted);
                    list("and take it out of routes", &i.routes_changed);
                    println!(
                        "Nothing was changed. Repeat with --confirm to delete all of this (it is logged and can be undone)."
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
