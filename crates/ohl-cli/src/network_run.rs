// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network …`: a country server's database (ADR-0013, phase 4) —
//! project files imported, one model exported, the models listed, the
//! PostgreSQL store measured.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Subcommand;
use ohl_store_pg::Network;
use ohl_store_pg::import::{ImportOptions, TWIN_TOLERANCE, import};

/// The environment variable giving the database when `--database` is
/// not given (keeps a password out of the command line).
const DATABASE_ENV: &str = "OHL_DATABASE_URL";

#[derive(Subcommand)]
pub enum NetworkCommand {
    /// Import project files (.ohl) into the network, each as a new line
    /// (model): the current state of its main project line as one log
    /// entry, ids another line has renamed CODE-id, twin towers listed;
    /// prints the import report (and writes it as HTML with --report).
    /// The files are not changed.
    Import {
        /// Project files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// The line's code (one file only; default: the file's name
        /// without .ohl).
        #[arg(long)]
        line: Option<String>,
        /// The line's name (one file only; default: the line's name in the
        /// file).
        #[arg(long)]
        name: Option<String>,
        /// Database (a PostgreSQL connection string; default: the
        /// environment variable OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who imports (becomes an admin of each line).
        #[arg(long)]
        user: String,
        /// Twin towers: structures of two lines within this distance in
        /// plan (and of the same type), m.
        #[arg(long, default_value_t = TWIN_TOLERANCE)]
        tolerance: f64,
        /// No revision "imported from FILE" (faster; the log entry only).
        #[arg(long)]
        no_revision: bool,
        /// Folder for the reports (CODE-import.html and CODE-import.json
        /// per line).
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Export one line of the network as a standalone project file
    /// (today's format, with network.* meta keys).
    Export {
        /// The line's code.
        #[arg(long)]
        line: String,
        /// Project file to write (must not exist).
        out: PathBuf,
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who exports (in the audit).
        #[arg(long)]
        user: String,
    },
    /// The network's lines, its schema and size.
    Lines {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
    },
    /// Measure the PostgreSQL store on one line: open, edit, undo and redo
    /// latency (structures moved by 1 m, back and again, as `ohl synth
    /// measure`), and with --export the export; prints JSON.
    Measure {
        /// The line's code.
        #[arg(long)]
        line: String,
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// User (must be allowed to edit the line).
        #[arg(long)]
        user: String,
        /// Structures to move.
        #[arg(long, default_value_t = 5)]
        edits: usize,
        /// Time limit for the edits, undos and redos, s.
        #[arg(long, default_value_t = 600)]
        seconds: u64,
        /// Also export the line to this project file (must not exist),
        /// timed.
        #[arg(long)]
        export: Option<PathBuf>,
    },
}

fn database(given: Option<String>) -> Result<String, String> {
    given
        .or_else(|| std::env::var(DATABASE_ENV).ok())
        .ok_or_else(|| format!("no database: give --database or set {DATABASE_ENV}"))
}

pub fn run(command: NetworkCommand) -> Result<(), String> {
    match command {
        NetworkCommand::Import {
            files,
            line,
            name,
            database: db,
            user,
            tolerance,
            no_revision,
            report,
        } => {
            if files.len() > 1 && (line.is_some() || name.is_some()) {
                return Err("--line and --name go with one file only".into());
            }
            let net = Network::connect_writer(&database(db)?).map_err(|e| e.to_string())?;
            if let Some(dir) = &report {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            let started = Instant::now();
            let (mut renamed, mut twins) = (0, 0);
            for file in &files {
                let code = match &line {
                    Some(c) => c.clone(),
                    None => file
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .ok_or_else(|| format!("{}: no file name", file.display()))?,
                };
                let mut options = ImportOptions::new(&code, &user);
                options.name = name.clone();
                options.tolerance = tolerance;
                options.revision = !no_revision;
                let r =
                    import(&net, file, &options).map_err(|e| format!("{}: {e}", file.display()))?;
                print!("{}", r.text());
                renamed += r.renamed.len();
                twins += r.twins.len();
                if let Some(dir) = &report {
                    let html = dir.join(format!("{code}-import.html"));
                    std::fs::write(&html, r.html())
                        .map_err(|e| format!("{}: {e}", html.display()))?;
                    let json = dir.join(format!("{code}-import.json"));
                    std::fs::write(
                        &json,
                        serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| format!("{}: {e}", json.display()))?;
                }
            }
            net.analyse().map_err(|e| e.to_string())?;
            println!(
                "{} files in {:.1} s: {renamed} ids renamed, {twins} twin towers listed; \
                 database {:.1} MB",
                files.len(),
                started.elapsed().as_secs_f64(),
                net.size().map_err(|e| e.to_string())? as f64 / 1e6
            );
            Ok(())
        }
        NetworkCommand::Export {
            line,
            out,
            database: db,
            user,
        } => {
            let net = Network::connect(&database(db)?).map_err(|e| e.to_string())?;
            let r = ohl_store_pg::export::export(&net, &line, &out, &user)
                .map_err(|e| e.to_string())?;
            println!(
                "line {} written to {} in {:.2} s: {} object rows, {} log entries, {} revisions, \
                 {} structures",
                r.line,
                out.display(),
                r.seconds,
                r.objects,
                r.entries,
                r.revisions,
                r.structures
            );
            Ok(())
        }
        NetworkCommand::Lines { database: db } => {
            let net = Network::connect(&database(db)?).map_err(|e| e.to_string())?;
            println!(
                "schema {}, coordinate system \"{}\", database {:.1} MB",
                net.schema().map_err(|e| e.to_string())?,
                net.meta("crs")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default(),
                net.size().map_err(|e| e.to_string())? as f64 / 1e6
            );
            for l in net.lines().map_err(|e| e.to_string())? {
                println!("{}\t{}\t{}", l.id, l.name, l.source);
            }
            Ok(())
        }
        NetworkCommand::Measure {
            line,
            database: db,
            user,
            edits,
            seconds,
            export,
        } => {
            let net = Network::connect_writer(&database(db)?).map_err(|e| e.to_string())?;
            let size = net.size().map_err(|e| e.to_string())?;
            let t = Instant::now();
            let store = net.store(&line).map_err(|e| e.to_string())?;
            let session =
                ohl_session::Session::open_on(Box::new(store), &line).map_err(|e| e.to_string())?;
            let open_s = t.elapsed().as_secs_f64();
            let structures = session.line().structures.len();
            let b = ohl_synth::project::measure_session(
                session,
                size.max(0) as u64,
                (open_s * 1000.0).round() / 1000.0,
                &user,
                edits,
                Duration::from_secs(seconds),
                false,
                ohl_synth::project::Shared::No,
            )?;
            let export_s = match export {
                Some(out) => {
                    let r = ohl_store_pg::export::export(&net, &line, &out, &user)
                        .map_err(|e| e.to_string())?;
                    Some((r.seconds * 1000.0).round() / 1000.0)
                }
                None => None,
            };
            let report = serde_json::json!({
                "line": line,
                "structures": structures,
                "database_bytes": size,
                "measure": b,
                "export_s": export_s,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
    }
}
