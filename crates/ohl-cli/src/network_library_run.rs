// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network library …`: the country libraries (ADR-0013, phase 9) —
//! kinds locked for the whole network, their files and releases,
//! requests for additions and their decisions, withdrawn entries, the
//! lines' pins, the use index, and *Update library pins*. Every change
//! goes through the rights' one function (`--user` is who acts) and into
//! the audit; a running server takes it before its next job. Update
//! library pins changes models: here it needs the network's writer (no
//! server running); with a server, a client asks the server for it.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::Subcommand;
use ohl_session::Session;
use ohl_session::rights::UserRights;
use ohl_store_pg::Network;
use ohl_store_pg::library::{LibraryCommand, RightsRoles, apply_pins, check_pin_rights, plan_pins};
use ohl_store_pg::rights::DbResolve;

#[derive(Subcommand)]
pub enum LibraryCommandArgs {
    /// The network's library kinds you may see: locked, files, releases,
    /// maintainers.
    Kinds {
        #[arg(long)]
        database: Option<String>,
        /// Who asks (their rights decide what is shown).
        #[arg(long)]
        user: String,
    },
    /// The files of a kind as they are now, with the entries you may see
    /// (version, first release, withdrawn).
    Files {
        /// The kind (cables, structures, insulators, weather, …).
        kind: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Lock a kind (or one file of it) for the whole network: it then
    /// changes only by requests granted and releases. --unlock: network
    /// admins only.
    Lock {
        kind: String,
        /// One file of it.
        #[arg(long)]
        file: Option<String>,
        /// Unlock instead.
        #[arg(long)]
        unlock: bool,
        /// Why.
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Put a file into the kind (unlocked kinds; a locked one in an
    /// emergency, network admins only).
    Put {
        kind: String,
        /// The library file to read.
        path: PathBuf,
        /// Its name in the network's library (default: the file's name).
        #[arg(long)]
        name: Option<String>,
        /// A network admin's change of a locked kind (in the audit).
        #[arg(long)]
        emergency: bool,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Remove a file from the kind (its releases keep it).
    Delete {
        kind: String,
        name: String,
        #[arg(long)]
        emergency: bool,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Request an entry for a kind (an addition; a maintainer may ask for
    /// a change): its draft may be used in your own branch of --line
    /// until it is decided, and blocks that branch's review and merge.
    Request {
        kind: String,
        /// A library file holding the entry.
        path: PathBuf,
        /// The file of the network's library it goes into.
        #[arg(long)]
        into: String,
        /// The entry (default: the file's only one).
        #[arg(long)]
        entry: Option<String>,
        /// Why.
        #[arg(long, default_value = "")]
        reason: String,
        /// The line it is for.
        #[arg(long)]
        line: Option<String>,
        /// Your own branch of that line where its draft is used.
        #[arg(long)]
        branch: Option<String>,
        /// The routes it is for (repeatable).
        #[arg(long)]
        route: Vec<String>,
        /// Keep it as a draft (only you see it; `submit` asks for it).
        #[arg(long)]
        draft: bool,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// The requests you may see (yours; those you maintain).
    Requests {
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Ask for a draft request.
    Submit {
        id: i64,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Take a request of yours back.
    WithdrawRequest {
        id: i64,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Grant a request (a maintainer, not its requester): its entry goes
    /// into the kind, for the next release.
    Grant {
        id: i64,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Refuse a request (a maintainer, not its requester).
    Refuse {
        id: i64,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Release the kind as it is now under a tag (maintainers). No line
    /// changes: lines move by Update library pins.
    Release {
        kind: String,
        tag: String,
        #[arg(long, default_value = "")]
        notes: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Withdraw an entry (maintainers): the lines using it keep it and are
    /// shown it; nothing may start using it.
    Withdraw {
        kind: String,
        file: String,
        entry: String,
        #[arg(long, default_value = "")]
        note: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Bring a withdrawn entry back (maintainers).
    Restore {
        kind: String,
        file: String,
        entry: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Mark a line built (pinned; its pins moved only by whoever reviews
    /// the whole line) or, with --design, in design again (following the
    /// latest releases).
    Built {
        line: String,
        /// In design again.
        #[arg(long)]
        design: bool,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// The lines you may see with their pins, the latest releases and the
    /// withdrawn entries they use.
    Status {
        #[arg(long)]
        line: Option<String>,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
        /// As JSON.
        #[arg(long)]
        json: bool,
    },
    /// The use index: where an entry is used (only what you may see).
    Uses {
        kind: String,
        entry: String,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Update library pins: a line's library files moved to the releases
    /// (as one logged command per line, the routes whose results change
    /// marked out of date). Needs the network's writer (no server
    /// running). --following: every line in design.
    UpdatePins {
        /// The line.
        #[arg(long, conflicts_with = "following")]
        line: Option<String>,
        /// Every line in design (not built).
        #[arg(long)]
        following: bool,
        /// The kinds (repeatable; default: every kind released).
        #[arg(long)]
        kind: Vec<String>,
        /// The release (one kind; default: the latest).
        #[arg(long)]
        release: Option<String>,
        /// Show what would change, change nothing.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: String,
    },
    /// Make the use index again from every line (the network's writer;
    /// a server does it for lines without one when it starts).
    Reindex {
        #[arg(long)]
        database: Option<String>,
    },
}

fn connect(db: Option<String>) -> Result<Network, String> {
    Network::connect(&crate::network_run::database(db)?).map_err(|e| e.to_string())
}

fn compiled(net: &Network, user: &str) -> Result<UserRights, String> {
    let data = net.rights().map_err(|e| e.to_string())?;
    Ok(data.compile(user, &DbResolve(net)))
}

fn change(db: Option<String>, actor: &str, c: LibraryCommand) -> Result<(), String> {
    let net = connect(db)?;
    let done = net.library_command(actor, &c).map_err(|e| e.to_string())?;
    println!("{}", done.text);
    Ok(())
}

fn read(path: &PathBuf) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn file_name(path: &std::path::Path) -> Result<String, String> {
    path.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{}: no file name", path.display()))
}

pub fn run(command: LibraryCommandArgs) -> Result<(), String> {
    use LibraryCommandArgs as A;
    match command {
        A::Kinds { database, user } => {
            let net = connect(database)?;
            let me = compiled(&net, &user)?;
            for k in net.library_kinds(&me).map_err(|e| e.to_string())? {
                println!(
                    "{}{}: {} file(s), releases {}; maintainers {}",
                    k.kind,
                    if k.locked { " (locked)" } else { "" },
                    k.files,
                    if k.releases.is_empty() {
                        "none".to_owned()
                    } else {
                        k.releases.join(", ")
                    },
                    if k.maintainers.is_empty() {
                        "none".to_owned()
                    } else {
                        k.maintainers.join(", ")
                    }
                );
            }
            Ok(())
        }
        A::Files {
            kind,
            database,
            user,
        } => {
            let net = connect(database)?;
            let me = compiled(&net, &user)?;
            for f in net.library_files(&me, &kind).map_err(|e| e.to_string())? {
                println!("{}{}", f.file, if f.locked { " (locked)" } else { "" });
                for (id, v, first, withdrawn) in f.entries {
                    println!(
                        "  {id}  {v}  {}{}",
                        first.as_deref().unwrap_or("not released"),
                        if withdrawn { "  withdrawn" } else { "" }
                    );
                }
            }
            Ok(())
        }
        A::Lock {
            kind,
            file,
            unlock,
            note,
            database,
            user,
        } => change(
            database,
            &user,
            LibraryCommand::Lock {
                kind,
                file,
                locked: !unlock,
                note,
            },
        ),
        A::Put {
            kind,
            path,
            name,
            emergency,
            database,
            user,
        } => {
            let file = match name {
                Some(n) => n,
                None => file_name(&path)?,
            };
            change(
                database,
                &user,
                LibraryCommand::PutFile {
                    kind,
                    file,
                    text: read(&path)?,
                    emergency,
                },
            )
        }
        A::Delete {
            kind,
            name,
            emergency,
            database,
            user,
        } => change(
            database,
            &user,
            LibraryCommand::DeleteFile {
                kind,
                file: name,
                emergency,
            },
        ),
        A::Request {
            kind,
            path,
            into,
            entry,
            reason,
            line,
            branch,
            route,
            draft,
            database,
            user,
        } => change(
            database,
            &user,
            LibraryCommand::Request {
                kind,
                file: into,
                text: read(&path)?,
                entry,
                reason,
                line,
                branch,
                routes: route,
                draft,
            },
        ),
        A::Requests { database, user } => {
            let net = connect(database)?;
            let me = compiled(&net, &user)?;
            for r in net.library_requests(&me).map_err(|e| e.to_string())? {
                println!(
                    "#{} {} {} into {} — {} by {} ({}){}{}",
                    r.id,
                    r.kind,
                    r.entry,
                    r.file,
                    r.state,
                    r.user,
                    r.created,
                    match (&r.line, &r.branch) {
                        (Some(l), Some(b)) => {
                            format!("; draft {} in branch {b} of line {l}", r.draft_name())
                        }
                        (Some(l), None) => format!("; for line {l}"),
                        _ => String::new(),
                    },
                    r.maintainer
                        .as_ref()
                        .map_or(String::new(), |m| format!("; decided by {m}"))
                );
                if !r.reason.is_empty() {
                    println!("    {}", r.reason);
                }
            }
            Ok(())
        }
        A::Submit { id, database, user } => change(database, &user, LibraryCommand::Submit { id }),
        A::WithdrawRequest { id, database, user } => {
            change(database, &user, LibraryCommand::WithdrawRequest { id })
        }
        A::Grant {
            id,
            note,
            database,
            user,
        } => change(database, &user, LibraryCommand::Grant { id, note }),
        A::Refuse {
            id,
            note,
            database,
            user,
        } => change(database, &user, LibraryCommand::Refuse { id, note }),
        A::Release {
            kind,
            tag,
            notes,
            database,
            user,
        } => change(
            database,
            &user,
            LibraryCommand::Release { kind, tag, notes },
        ),
        A::Withdraw {
            kind,
            file,
            entry,
            note,
            database,
            user,
        } => change(
            database,
            &user,
            LibraryCommand::WithdrawEntry {
                kind,
                file,
                entry,
                note,
            },
        ),
        A::Restore {
            kind,
            file,
            entry,
            database,
            user,
        } => change(
            database,
            &user,
            LibraryCommand::RestoreEntry { kind, file, entry },
        ),
        A::Built {
            line,
            design,
            database,
            user,
        } => change(
            database,
            &user,
            LibraryCommand::SetBuilt {
                line,
                built: !design,
            },
        ),
        A::Status {
            line,
            database,
            user,
            json,
        } => {
            let net = connect(database)?;
            let me = compiled(&net, &user)?;
            let status = net
                .library_status(&me, line.as_deref())
                .map_err(|e| e.to_string())?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&status).map_err(|e| e.to_string())?
                );
                return Ok(());
            }
            for l in status {
                println!(
                    "{}{}",
                    l.line,
                    if l.built {
                        " (built, pinned)"
                    } else {
                        " (in design, follows the latest)"
                    }
                );
                for k in l.kinds {
                    println!(
                        "  {}: {} (latest {}{}){}",
                        k.kind,
                        k.pinned.as_deref().unwrap_or("own files"),
                        k.latest.as_deref().unwrap_or("none"),
                        if k.behind > 0 {
                            format!(", {} behind", k.behind)
                        } else {
                            String::new()
                        },
                        if k.withdrawn_used.is_empty() {
                            String::new()
                        } else {
                            format!("; withdrawn entries used: {}", k.withdrawn_used.join(", "))
                        }
                    );
                }
            }
            Ok(())
        }
        A::Uses {
            kind,
            entry,
            database,
            user,
        } => {
            let net = connect(database)?;
            let me = compiled(&net, &user)?;
            let t = Instant::now();
            let uses = net.uses(&me, &kind, &entry).map_err(|e| e.to_string())?;
            for u in &uses {
                println!(
                    "{} {} {}{}",
                    u.line,
                    u.object_kind,
                    u.object,
                    if u.routes.is_empty() {
                        String::new()
                    } else {
                        format!(" (routes {})", u.routes.join(", "))
                    }
                );
            }
            println!(
                "{} use(s) you may see, in {:.1} ms",
                uses.len(),
                t.elapsed().as_secs_f64() * 1e3
            );
            Ok(())
        }
        A::UpdatePins {
            line,
            following,
            kind,
            release,
            dry_run,
            database,
            user,
        } => {
            let url = crate::network_run::database(database)?;
            let net = if dry_run {
                Network::connect(&url)
            } else {
                Network::connect_writer(&url)
            }
            .map_err(|e| {
                format!("{e} (with a server running, ask it from a client: Update library pins)")
            })?;
            let lines: Vec<String> = match (line, following) {
                (Some(l), _) => vec![l],
                (None, true) => net.following_lines().map_err(|e| e.to_string())?,
                (None, false) => return Err("name --line, or --following".into()),
            };
            let me = compiled(&net, &user)?;
            let kinds = (!kind.is_empty()).then_some(kind.as_slice());
            let started = Instant::now();
            let (mut moved, mut refused, mut files, mut stale) = (0, 0, 0, 0);
            for code in &lines {
                let t = Instant::now();
                let mut s =
                    Session::open_on(Box::new(net.store(code).map_err(|e| e.to_string())?), code)
                        .map_err(|e| format!("line {code}: {e}"))?;
                s.set_role_source(Some(Arc::new(RightsRoles {
                    line: code.clone(),
                    rights: me.clone(),
                })));
                let plan = plan_pins(&net, &mut s, code, kinds, release.as_deref())
                    .map_err(|e| format!("line {code}: {e}"))?;
                if dry_run {
                    print!("{}", plan.describe());
                    if let Err(why) = check_pin_rights(&me, &plan) {
                        println!("  (refused for {user}: {why})");
                    }
                    continue;
                }
                if !plan.moves() {
                    continue;
                }
                match apply_pins(&net, &mut s, &me, &plan) {
                    Ok(_) => {
                        moved += 1;
                        files += plan.changes().len();
                        stale += plan.stale().len();
                        println!(
                            "{} ({:.2} s): {}",
                            code,
                            t.elapsed().as_secs_f64(),
                            plan.note()
                        );
                    }
                    Err(e) => {
                        refused += 1;
                        println!("{code}: {e}");
                    }
                }
            }
            if !dry_run {
                println!(
                    "{moved} line(s) moved, {refused} refused, {files} file(s) changed, {stale} \
                     route(s) marked out of date, of {} line(s), in {:.2} s",
                    lines.len(),
                    started.elapsed().as_secs_f64()
                );
            }
            Ok(())
        }
        A::Reindex { database } => {
            let net = Network::connect_writer(&crate::network_run::database(database)?)
                .map_err(|e| e.to_string())?;
            let started = Instant::now();
            let mut rows = 0;
            let lines = net.lines().map_err(|e| e.to_string())?;
            for l in &lines {
                let s = Session::open_on(
                    Box::new(net.store(&l.id).map_err(|e| e.to_string())?),
                    &l.id,
                )
                .map_err(|e| format!("line {}: {e}", l.id))?;
                rows += net.reindex_uses(&l.id, &s).map_err(|e| e.to_string())?;
            }
            println!(
                "use index: {rows} rows for {} line(s) in {:.2} s",
                lines.len(),
                started.elapsed().as_secs_f64()
            );
            Ok(())
        }
    }
}
