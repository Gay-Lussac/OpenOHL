// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl network …`: a country server's database (ADR-0013, phases 4–7) —
//! project files imported, one model exported, the models listed, the
//! PostgreSQL store measured; the server on the network hosted and
//! measured (`serve`, `bench`); the rights for admins (`rights …`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Subcommand;
use ohl_store_pg::Network;
use ohl_store_pg::import::{ImportOptions, TWIN_TOLERANCE, import};

/// The environment variable giving the database when `--database` is
/// not given (keeps a password out of the command line).
const DATABASE_ENV: &str = "OHL_DATABASE_URL";

/// Phase 13: TLS to the database from the environment (added to the
/// connection string unless it says them): `sslmode` (disable, require,
/// verify-ca, verify-full) and the CA file.
const SSLMODE_ENV: &str = "OHL_DATABASE_SSLMODE";
const CA_ENV: &str = "OHL_DATABASE_CA";

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
    /// Export a working set for offline work (phase 12): a project file of
    /// today's format holding the working set editable, its context and
    /// its neighbours within the checks' reach read-only (other lines' in
    /// the meta), filtered by the user's rights (what they may not see
    /// left out, counted), the export recorded and audited. Needs see and
    /// export on the working set's routes and edit on something of each.
    /// Hosts the network in this process (refused while its server runs).
    ExportWorkingSet {
        #[command(flatten)]
        pick: crate::network_offline_run::PickArgs,
        /// The checks' reach the neighbours are taken within, m (default
        /// the field distance, 500 m; at most 10 000).
        #[arg(long)]
        reach: Option<f64>,
        /// Project file to write (must not exist).
        out: PathBuf,
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who exports (the bundle comes back from them).
        #[arg(long)]
        user: String,
    },
    /// The bundle of an offline working set's work (phase 12): the main
    /// line's revisions since the export and only the changes they need
    /// (the bundle format); uncommitted changes refuse it. No database.
    OfflineBundle {
        /// The offline working set (a project file exported).
        file: PathBuf,
        /// Bundle to write (must not exist).
        #[arg(long)]
        out: PathBuf,
    },
    /// Bring an offline working set's bundle back (phase 12): verified
    /// against the export the network recorded, applied on a new branch
    /// over the working set made from the export's base (never on main),
    /// checked by the user's current rights and the rules of shared
    /// towers, committed; conflicts with main listed, never settled.
    /// Hosts the network in this process (refused while its server runs).
    ImportBundle {
        /// The bundle.
        bundle: PathBuf,
        /// The branch's name (default offline-USER-N-K).
        #[arg(long)]
        branch: Option<String>,
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who brings it back (its exporter).
        #[arg(long)]
        user: String,
    },
    /// A check on an offline working set with the neighbours its export
    /// holds (phase 12): the network's server's report, or NOT ALL CHECKED
    /// where the file cannot give it. No database; prints JSON.
    CheckOffline {
        /// The offline working set.
        file: PathBuf,
        /// clearances, fields or loads (structure loads on towers shared
        /// with other lines).
        #[arg(long)]
        check: String,
        /// The weather case (clearances, fields).
        #[arg(long)]
        case: Option<String>,
        /// The clearance rules (a rules file).
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Structures of the loads (default: those with a twin).
        #[arg(long = "structure")]
        structures: Vec<String>,
        /// The field distance, m (default 500).
        #[arg(long)]
        distance: Option<f64>,
        /// The current in every circuit, A (fields).
        #[arg(long)]
        amps: Option<f64>,
        /// The field's limit, µT.
        #[arg(long)]
        limit_ut: Option<f64>,
    },
    /// Measure phase 12 on the network: a whole route's offline working
    /// set exported (size and time), bundles of 20 and of 500 changes made
    /// and brought back. Hosts the network in this process; prints JSON.
    /// Adds exports and branches.
    BenchOffline {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// A user who may export and edit the route (the network admin).
        #[arg(long)]
        user: String,
        /// The line (default: the first with a tower shared with another).
        #[arg(long)]
        line: Option<String>,
        /// Exports timed.
        #[arg(long, default_value_t = 3)]
        rounds: usize,
        /// Folder for the files.
        #[arg(long)]
        work: PathBuf,
        /// Phase 14: then a move's latency while the route is exported
        /// with this reach (m), on the authority and on the exports'
        /// thread (adds the user bench-mover, a designer of the network).
        #[arg(long)]
        during_reach: Option<f64>,
    },
    /// The network's users, groups, roles and assignments (admins; every
    /// change through the rights' one function and into the audit).
    Rights {
        #[command(subcommand)]
        command: crate::network_rights_run::RightsCommandArgs,
    },
    /// The country libraries (phase 9): kinds locked for the whole
    /// network, requests, releases, withdrawn entries, the lines' pins,
    /// the use index, Update library pins.
    Library {
        #[command(subcommand)]
        command: crate::network_library_run::LibraryCommandArgs,
    },
    /// Measure phase 9 on the network: a library released and every line
    /// in design moved to it (Update library pins, twice: the files put,
    /// then one entry changed), the use index made and queried, a move
    /// with the use index kept. Needs the writer (no server running);
    /// prints JSON. Edits the network (library files, pins, a structure
    /// moved and back).
    BenchLibrary {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// The network admin (made one when the network has none).
        #[arg(long)]
        user: String,
        /// Uses-index queries timed.
        #[arg(long, default_value_t = 20)]
        queries: usize,
    },
    /// Measure phase 10 on the network: checks on a working set with
    /// their neighbours pulled in, computed on the server (in this
    /// process) — a whole route's clearances with the other lines' wires
    /// as targets, its field with the lines within 500 m, the loads on a
    /// tower shared with another line under each line's cases; prints
    /// JSON. Changes nothing.
    BenchScoped {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// A user who may see the whole network.
        #[arg(long)]
        user: String,
        /// Runs of each check.
        #[arg(long, default_value_t = 3)]
        runs: usize,
    },
    /// Measure phase 11 on the network's models: a commit of one changed
    /// route, a branch over a working set (diff, ahead and behind, the
    /// merge), a merge at a shared tower — with split trees and with flat
    /// ones on the same model. Needs the writer (no server running);
    /// prints JSON. Edits the network (structures moved, revisions,
    /// branches).
    BenchVersions {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// A user of the models (their admin: the importer).
        #[arg(long)]
        user: String,
        /// The models measured (default: the network's first).
        #[arg(long = "line")]
        lines: Vec<String>,
        /// Commits of the changed route timed.
        #[arg(long, default_value_t = 10)]
        rounds: usize,
        /// The trees measured, in order (`split`, `flat`).
        #[arg(long, value_delimiter = ',', default_value = "split,flat")]
        trees: Vec<String>,
    },
    /// Phase 13: back the network up into a new folder (network admins;
    /// runs beside the server): PostgreSQL's dump (custom format) in one
    /// snapshot, a manifest with the schema, the time and the hashes of the
    /// dump, of each line and of the network's own tables; recorded (a
    /// schema migration asks for a recent one).
    Backup {
        /// The folder (new, or empty).
        #[arg(long)]
        out: PathBuf,
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who backs up (in the audit).
        #[arg(long)]
        user: String,
    },
    /// Phase 13: restore a backup into an empty database (of this
    /// program's schema only), in one transaction, then the restore drill:
    /// every line and the network's tables compared with the manifest's
    /// hashes.
    Restore {
        /// The backup's folder.
        dir: PathBuf,
        /// The empty database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who restores (in the restored network's audit).
        #[arg(long)]
        user: String,
    },
    /// Phase 13: bring the network's schema forward to this program's
    /// (refused without a backup newer than the network's
    /// backup_max_age_hours, default 24, unless --i-have-a-backup; in the
    /// audit). The server is not running meanwhile.
    Migrate {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who migrates (in the audit).
        #[arg(long)]
        user: String,
        /// Migrate without a backup recorded (you made one another way;
        /// in the audit).
        #[arg(long)]
        i_have_a_backup: bool,
    },
    /// Phase 13: a network setting shown, or set (network admins; in the
    /// audit). Settings: backup_max_age_hours.
    Setting {
        /// The setting.
        key: String,
        /// Its new value (none: shown).
        value: Option<String>,
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Who changes it.
        #[arg(long)]
        user: Option<String>,
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
    /// Host the network for several users (pre-alpha, protocol 3): every
    /// line read once, each edit written to the database, clients served
    /// by interest (a desktop follows a line, a browser views tiles).
    /// Prints the invitations; runs until stopped. One server per
    /// database (a second is refused).
    Serve {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// Address and port to listen on (UDP: QUIC).
        #[arg(long, default_value = "0.0.0.0:4433")]
        bind: std::net::SocketAddr,
        /// Host name or address in the invitation (default: this
        /// computer's address on the local network).
        #[arg(long)]
        advertise: Option<String>,
        /// Secret of the general invitation (default: a random one).
        #[arg(long)]
        code: Option<String>,
        /// File keeping the key of the personal invitations (made when
        /// missing, readable by its owner only; default: a random key for
        /// this run).
        #[arg(long, value_name = "FILE")]
        keys: Option<PathBuf>,
        /// Role of users the network does not know: none (a network admits
        /// only its users, each with their personal invitation, since
        /// phase 7; another value is refused).
        #[arg(long, default_value = "none")]
        newcomers: String,
        /// Print the personal invitation of this user (repeatable).
        #[arg(long)]
        invite: Vec<String>,
        /// Also serve the web client on this TCP port (0: any free one).
        #[arg(long, value_name = "PORT")]
        web: Option<u16>,
        /// Stop after this many seconds (default: run until stopped).
        #[arg(long)]
        seconds: Option<u64>,
        /// Reader connections at most (copies of lines for desktops).
        #[arg(long, default_value_t = 4)]
        readers: usize,
        /// Threads making tile packages.
        #[arg(long, default_value_t = 2)]
        packagers: usize,
        /// Threads solving one package's sections side by side.
        #[arg(long, default_value_t = 4)]
        solve_threads: usize,
        /// Tile packages kept in memory, MB.
        #[arg(long, default_value_t = 128)]
        packages_mb: usize,
        /// Solved section shapes kept in memory, MB.
        #[arg(long, default_value_t = 256)]
        shapes_mb: usize,
        /// A heartbeat this often, ms.
        #[arg(long, default_value_t = 2000)]
        heartbeat_ms: u64,
        /// Do not solve the context ahead while the server is idle
        /// (phase 6: by default every tile is made once after a start,
        /// one at a time, its shapes kept within --shapes-mb).
        #[arg(long)]
        no_idle_solve: bool,
        /// Write the server's counters and timings (JSON) to this file
        /// every half second (measurements).
        #[arg(long, value_name = "FILE")]
        stats: Option<PathBuf>,
        /// Connections at once (more wait for a free one).
        #[arg(long, default_value_t = ohl_server::server::MAX_QUIC)]
        max_connections: usize,
        /// Connections of one address not joined yet.
        #[arg(long, default_value_t = ohl_server::server::MAX_PENDING)]
        max_per_address: usize,
        /// Phase 13: the personal invitations printed are valid this many
        /// days (default 90 since 2026-11-20, OHL-Design's sign-in window
        /// showing an expired invitation; 0: until reissued).
        #[arg(long, default_value_t = ohl_server::network::DEFAULT_INVITE_DAYS)]
        invite_days: u32,
        /// Phase 13: checks, exports and bundles waiting at most (one per
        /// user), done when no other job waits.
        #[arg(long, default_value_t = 4)]
        max_heavy_queue: usize,
        /// Phase 13: new tiles made for one client a second (and 4 times
        /// as many at once).
        #[arg(long, default_value_t = 64.0)]
        tiles_per_second: f64,
        /// Phase 13: OIDC sign-in — the identity provider's issuer
        /// (https://…); with --oidc-audience. Personal invitations still
        /// work.
        #[arg(long)]
        oidc_issuer: Option<String>,
        /// The client id the ID tokens are for.
        #[arg(long)]
        oidc_audience: Option<String>,
        /// The provider's keys' URL (default: from its discovery
        /// document).
        #[arg(long)]
        oidc_jwks_url: Option<String>,
        /// The provider's keys from a file instead (no fetching).
        #[arg(long)]
        oidc_jwks_file: Option<PathBuf>,
        /// The provider's CA (PEM; default: the Mozilla roots).
        #[arg(long)]
        oidc_ca: Option<PathBuf>,
        /// The oldest ID token taken, s.
        #[arg(long, default_value_t = 3600)]
        oidc_max_age_s: u64,
    },
    /// Measure the server on the network (the plan's targets): start,
    /// memory idle and per client, join to first view, tile packages made
    /// and from the cache, a move applied and broadcast, area queries.
    /// Runs `ohl network serve` as a process of its own, then the server
    /// in this one for what is timed inside it; prints JSON. Edits the
    /// network (structures moved back and forth).
    Bench {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// A user allowed to edit the line moved.
        #[arg(long)]
        user: String,
        /// The line whose structures are moved (default: the first).
        #[arg(long)]
        line: Option<String>,
        /// Viewers at once (half on one view, half each on its own).
        #[arg(long, default_value_t = 20)]
        viewers: usize,
        /// A view: tiles across and down.
        #[arg(long, default_value_t = 4)]
        view_w: i64,
        #[arg(long, default_value_t = 3)]
        view_h: i64,
        /// Moves timed.
        #[arg(long, default_value_t = 50)]
        moves: usize,
        /// Area queries timed.
        #[arg(long, default_value_t = 200)]
        areas: usize,
        /// Folder for the bench's files (the line's copy, the keys).
        #[arg(long)]
        work: PathBuf,
    },
    /// Measure phase 7 on the network: the rights' cost with many users
    /// (USERS users, ASSIGNMENTS assignments made by --user, the network
    /// admin): a command's rights decided, a tile's rights class and its
    /// package filtered, the clients that may not see a change worked out.
    /// Runs the server in this process; prints JSON. Edits the network
    /// (structures moved back and forth) and adds users and assignments
    /// (named rb-…; made once).
    BenchRights {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// The network admin (made one when the network has none).
        #[arg(long)]
        user: String,
        /// Users made.
        #[arg(long, default_value_t = 200)]
        users: usize,
        /// Assignments made.
        #[arg(long, default_value_t = 1000)]
        assignments: usize,
        /// Moves timed (each user).
        #[arg(long, default_value_t = 40)]
        moves: usize,
    },
    /// Measure phase 14 on the network, end to end with many clients at
    /// once: `ohl network serve` as a process of its own; editors (each a
    /// designer of one route) and viewers join together, each with a view
    /// of its own; every editor moves structures of its route at once
    /// while two whole routes' clearances and two offline working sets are
    /// asked; the joins', moves' and long work's times, the memory. Adds
    /// users named scale-… (made once); edits the network (structures
    /// moved back and forth). Prints JSON.
    BenchScale {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// The network admin (makes the users).
        #[arg(long)]
        user: String,
        /// Editors moving at once (one route each).
        #[arg(long, default_value_t = 40)]
        editors: usize,
        /// Viewers of the whole network joining with them.
        #[arg(long, default_value_t = 40)]
        viewers: usize,
        /// A view: tiles across and down.
        #[arg(long, default_value_t = 4)]
        view_w: i64,
        #[arg(long, default_value_t = 3)]
        view_h: i64,
        /// Moves of each editor at least (back and forth).
        #[arg(long, default_value_t = 40)]
        moves: usize,
        /// The editors move on until the long work is answered, at most
        /// this long, s.
        #[arg(long, default_value_t = 60)]
        edit_seconds: u64,
        /// Folder for the bench's files (the keys, the server's log).
        #[arg(long)]
        work: PathBuf,
    },
    /// Measure phase 6 on the network: working sets picked (shown before
    /// they are confirmed), a partial replica loaded, the rows of edits
    /// arriving in it, a replica joining again after edits, a route
    /// added, context tiles around a working set; then, with the server
    /// started again, the idle solve of the context case (the time to make
    /// every tile once, the memory). Runs `ohl network serve` as a process
    /// of its own; prints JSON. Edits the network (structures moved back
    /// and forth).
    BenchWorkingSet {
        /// Database (default: OHL_DATABASE_URL).
        #[arg(long)]
        database: Option<String>,
        /// A user allowed to edit the line (the editor).
        #[arg(long)]
        user: String,
        /// The line picked from (default: the first).
        #[arg(long)]
        line: Option<String>,
        /// Moves timed.
        #[arg(long, default_value_t = 40)]
        moves: usize,
        /// Longest wait for the idle solve, s (0: not measured).
        #[arg(long, default_value_t = 1800)]
        idle_wait_s: u64,
        /// Folder for the bench's files (the replicas, the keys).
        #[arg(long)]
        work: PathBuf,
    },
}

pub(crate) fn database(given: Option<String>) -> Result<String, String> {
    let url = given
        .or_else(|| std::env::var(DATABASE_ENV).ok())
        .ok_or_else(|| format!("no database: give --database or set {DATABASE_ENV}"))?;
    // Phase 13: TLS from the environment, unless the string says it.
    let mode = std::env::var(SSLMODE_ENV)
        .ok()
        .filter(|_| !url.contains("sslmode"));
    let ca = std::env::var_os(CA_ENV)
        .map(std::path::PathBuf::from)
        .filter(|_| !url.contains("sslrootcert"));
    Ok(ohl_store_pg::connect::with_tls(
        &url,
        mode.as_deref(),
        ca.as_deref(),
    ))
}

pub fn run(command: NetworkCommand) -> Result<(), String> {
    match command {
        NetworkCommand::Rights { command } => crate::network_rights_run::run(command),
        NetworkCommand::Library { command } => crate::network_library_run::run(command),
        NetworkCommand::ExportWorkingSet {
            pick,
            reach,
            out,
            database: db,
            user,
        } => {
            let spec = pick.spec()?;
            let server = crate::network_offline_run::host(&database(db)?)?;
            let r = crate::network_offline_run::export(&server, &user, spec, reach, &out);
            server.stop();
            let r = r?;
            eprintln!(
                "offline working set {} of line {} written to {} ({:.1} MB, {:.2} s): {} objects                  editable, {} read-only, {} other line(s) with {} section(s) near{}",
                r.number,
                r.model,
                out.display(),
                r.bytes as f64 / 1e6,
                r.seconds,
                r.wanted,
                r.context,
                r.neighbour_lines,
                r.neighbour_sections,
                if r.left_out.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; left out (you may not see them): {} of other lines, {} of this line                          — checks offline say NOT ALL CHECKED",
                        r.left_out.other_lines, r.left_out.this_line
                    )
                }
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::OfflineBundle { file, out } => {
            let b = crate::network_offline_run::make_bundle(&file, &out)?;
            println!(
                "bundle of {} revision(s) written to {} ({} blobs)",
                b.revisions.len(),
                out.display(),
                b.blobs.len()
            );
            Ok(())
        }
        NetworkCommand::ImportBundle {
            bundle,
            branch,
            database: db,
            user,
        } => {
            let b = crate::network_offline_run::read_bundle(&bundle)?;
            let server = crate::network_offline_run::host(&database(db)?)?;
            let r = crate::network_offline_run::bring_back(&server, &user, b, branch);
            server.stop();
            let r = r?;
            eprintln!(
                "brought back onto branch {} of line {}: {} object(s) put, {} deleted; {}",
                r.branch,
                r.model,
                r.puts,
                r.deletes,
                if r.conflicts.is_empty() {
                    "no conflict with main".to_owned()
                } else {
                    format!(
                        "{} changed on main since the export — its merge asks to settle them: {}",
                        r.conflicts.len(),
                        r.conflicts.join(", ")
                    )
                }
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::CheckOffline {
            file,
            check,
            case,
            rules,
            structures,
            distance,
            amps,
            limit_ut,
        } => {
            let c = crate::network_offline_run::scoped_check(
                &check, case, rules, structures, distance, amps, limit_ut,
            )?;
            let r = ohl_server::offline_checks::check_offline(&file, &c)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::BenchOffline {
            database: db,
            user,
            line,
            rounds,
            work,
            during_reach,
        } => {
            let report = crate::network_offline_run::bench(
                &database(db)?,
                &user,
                line,
                rounds,
                &work,
                during_reach,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::BenchScoped {
            database: db,
            user,
            runs,
        } => {
            let report = crate::network_bench_scoped::bench(&database(db)?, &user, runs)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::BenchVersions {
            database: db,
            user,
            lines,
            rounds,
            trees,
        } => {
            let report = crate::network_bench_versions::bench(
                &database(db)?,
                &user,
                &lines,
                rounds,
                &trees,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::BenchLibrary {
            database: db,
            user,
            queries,
        } => {
            let report = crate::network_bench_library::bench(&database(db)?, &user, queries)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::Serve {
            database: db,
            bind,
            advertise,
            code,
            keys,
            newcomers,
            invite,
            web,
            seconds,
            readers,
            packagers,
            solve_threads,
            packages_mb,
            shapes_mb,
            heartbeat_ms,
            no_idle_solve,
            stats,
            max_connections,
            max_per_address,
            invite_days,
            max_heavy_queue,
            tiles_per_second,
            oidc_issuer,
            oidc_audience,
            oidc_jwks_url,
            oidc_jwks_file,
            oidc_ca,
            oidc_max_age_s,
        } => {
            let oidc = match (oidc_issuer, oidc_audience) {
                (Some(issuer), Some(audience)) => {
                    let mut c = ohl_server::oidc::OidcConfig::new(&issuer, &audience);
                    c.jwks_url = oidc_jwks_url;
                    c.jwks_file = oidc_jwks_file;
                    c.ca_file = oidc_ca;
                    c.max_age = Duration::from_secs(oidc_max_age_s);
                    Some(std::sync::Arc::new(ohl_server::oidc::Oidc::new(c)?))
                }
                (None, None) => None,
                _ => return Err("OIDC: give both --oidc-issuer and --oidc-audience".into()),
            };
            let newcomers = match newcomers.as_str() {
                "none" => None,
                r => Some(ohl_session::Role::parse(r).map_err(|e| e.to_string())?),
            };
            let started = Instant::now();
            let server = ohl_server::server::Server::start_network(
                ohl_server::server::Options {
                    bind,
                    advertise,
                    code,
                    keys,
                    newcomers,
                    web: web.map(|port| std::net::SocketAddr::new(bind.ip(), port)),
                    max_connections,
                    max_per_address,
                },
                ohl_server::network::NetOptions {
                    database: database(db)?,
                    readers,
                    packagers,
                    solve_threads,
                    packages_mb,
                    shapes_mb,
                    heartbeat: Duration::from_millis(heartbeat_ms.max(10)),
                    idle_solve: !no_idle_solve,
                    invite_days: (invite_days > 0).then_some(invite_days),
                    max_heavy_queue,
                    tiles_per_second: tiles_per_second.max(0.001),
                    tiles_burst: (4.0 * tiles_per_second).max(1.0),
                    oidc,
                    ..Default::default()
                },
            )?;
            println!("network read in {:.2} s", started.elapsed().as_secs_f64());
            crate::host("the network", server, &invite, seconds, |server| {
                if let Some(path) = &stats
                    && let Ok(json) = server.probe(ohl_server::network::Probe::Stats)
                {
                    let tmp = path.with_extension("tmp");
                    if std::fs::write(&tmp, json).is_ok() {
                        let _ = std::fs::rename(&tmp, path);
                    }
                }
            })
        }
        NetworkCommand::Bench {
            database: db,
            user,
            line,
            viewers,
            view_w,
            view_h,
            moves,
            areas,
            work,
        } => {
            let report = crate::network_bench::bench(&crate::network_bench::Bench {
                database: database(db)?,
                user,
                line,
                viewers,
                view: (view_w.max(1), view_h.max(1)),
                moves,
                areas,
                work,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::BenchRights {
            database: db,
            user,
            users,
            assignments,
            moves,
        } => {
            let report =
                crate::network_bench_rights::bench(&crate::network_bench_rights::RightsBench {
                    database: database(db)?,
                    user,
                    users,
                    assignments,
                    moves,
                })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::BenchScale {
            database: db,
            user,
            editors,
            viewers,
            view_w,
            view_h,
            moves,
            edit_seconds,
            work,
        } => {
            let report = crate::network_bench_scale::bench(&crate::network_bench_scale::Scale {
                database: database(db)?,
                user,
                editors,
                viewers,
                view: (view_w, view_h),
                moves,
                edit_seconds,
                work,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        NetworkCommand::BenchWorkingSet {
            database: db,
            user,
            line,
            moves,
            idle_wait_s,
            work,
        } => {
            let report = crate::network_bench_ws::bench(&crate::network_bench_ws::WsBench {
                database: database(db)?,
                user,
                line,
                moves,
                idle_wait: Duration::from_secs(idle_wait_s),
                work,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            Ok(())
        }
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
            // Phase 7: network admins import (any user while the network
            // has none yet: it is being set up).
            let rights = net.rights().map_err(|e| e.to_string())?;
            let admins = rights.network_admins();
            if !admins.is_empty() && !admins.contains(&user) {
                let why = format!("{user} may not import lines: network admins only");
                let _ = net.audit(&user, "refused", "import", &why);
                return Err(why);
            }
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
            // Phase 7: the export permission on the line whole (a model
            // exported is every row of it).
            let rights = net
                .rights()
                .map_err(|e| e.to_string())?
                .compile(&user, &ohl_store_pg::rights::DbResolve(&net));
            let line_whole = ohl_session::rights::Target::Line(&line);
            if !rights.allows(ohl_session::rights::Permission::Export, &line_whole) {
                let why =
                    format!("{user} may not export line {line}: it needs export on the whole line");
                let _ = net.audit(&user, "refused", &format!("export {line}"), &why);
                return Err(why);
            }
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
        NetworkCommand::Backup {
            out,
            database: db,
            user,
        } => {
            let r = ohl_store_pg::backup::backup(&database(db)?, &user, &out)
                .map_err(|e| e.to_string())?;
            let m = &r.manifest;
            println!(
                "network backed up into {} in {:.1} s (dump {:.1} s, hashes {:.1} s): schema {}, \
                 {} lines, {:.1} MB, SHA-256 {}, data of {}",
                out.display(),
                r.seconds,
                r.dump_seconds,
                r.hash_seconds,
                m.schema,
                m.models.len(),
                m.bytes as f64 / 1e6,
                m.sha256,
                m.time
            );
            Ok(())
        }
        NetworkCommand::Restore {
            dir,
            database: db,
            user,
        } => {
            let r = ohl_store_pg::backup::restore(&database(db)?, &user, &dir)
                .map_err(|e| e.to_string())?;
            println!(
                "backup of {} restored in {:.1} s (pg_restore {:.1} s, the drill {:.1} s): {} \
                 lines and the network's tables the same as backed up",
                r.manifest.time, r.seconds, r.restore_seconds, r.verify_seconds, r.models
            );
            Ok(())
        }
        NetworkCommand::Migrate {
            database: db,
            user,
            i_have_a_backup,
        } => {
            let mut net = Network::connect_as_is(&database(db)?).map_err(|e| e.to_string())?;
            let (from, to) = net
                .migrate_now(&user, i_have_a_backup)
                .map_err(|e| e.to_string())?;
            if from == to {
                println!("network schema {to}: nothing to do");
            } else if from == 0 {
                println!("network schema {to} made");
            } else {
                println!("network schema {from} brought to {to}");
            }
            Ok(())
        }
        NetworkCommand::Setting {
            key,
            value,
            database: db,
            user,
        } => {
            let net = Network::connect(&database(db)?).map_err(|e| e.to_string())?;
            match value {
                None => println!(
                    "{key} = {}",
                    net.setting(&key)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_else(|| "(default)".into())
                ),
                Some(v) => {
                    let user = user.ok_or("--user: who changes it")?;
                    net.set_setting(&user, &key, &v)
                        .map_err(|e| e.to_string())?;
                    println!("{key} = {v}");
                }
            }
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
