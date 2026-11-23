// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Versions › Offline copy › **Offline working set** (2026-11-21; ADR-0013
//! phase 12 on the desktop): a network's working set taken where there is
//! no connection and its work brought back, without the command line —
//! the four steps of `ohl network export-working-set`, `check-offline`,
//! `offline-bundle` and `import-bundle` in one window:
//!
//! 1. **Export for offline work** (joined to a network with a working
//!    set): the server's `export_working_set` on this connection; the
//!    project file it sends written where the user says (never over a
//!    file: it may hold work). An export the server marks **stale** (a
//!    change landed while it was written) is said in red with how many
//!    objects changed meanwhile — they are not in the file.
//! 2. **Check offline** (an exported file open): the network's checks
//!    with the neighbours the file holds — the server's own code
//!    ([`ohl_server::offline_checks::check_offline`], on a thread of its
//!    own: a whole route's clearances take seconds), NOT ALL CHECKED said
//!    where the file cannot give the server's answer.
//! 3. **Make the bundle** (the exported file open): only its revisions
//!    since the export ([`ohl_session::Session::offline_bundle`]);
//!    uncommitted changes refuse it.
//! 4. **Bring the work back** (joined to the network): the server's
//!    `import_bundle` — a new branch over the working set, never main;
//!    its conflicts with main and the neighbours changed since listed,
//!    never settled.
//!
//! The window follows the tool windows' rules
//! (docs/development/tool-pages.md): a sentence and its (?), User guide ↗,
//! every setting with its (?), one button row (the step's action first,
//! Cancel last), Escape cancels (a wait for the server too: its answer is
//! then said in the status line), reports and errors in boxes that scroll.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_server::{BroughtBack, ClientMsg, OfflineExported, ScopedCheck, ScopedReport, ServerMsg};
use ohl_session::offline::OfflineMeta;
use ohl_session::versions::{Bundle, short};
use ohl_session::working_set::WorkingSetSpec;

use crate::app::Ready;
use crate::tool_ui;

/// The manual's section of the window.
pub const GUIDE: (&str, &str) = (
    "user-guide/offline-working-set",
    "the-offline-working-set-window",
);

/// The reach offered first, m (the server's default: the field distance).
pub const DEFAULT_REACH: f64 = 500.0;

/// The largest reach the server takes, m (a check's 10 km, phase 13).
pub const MAX_REACH: f64 = 10_000.0;

/// The extension offered for an offline working set's bundle.
pub const BUNDLE_EXTENSION: &str = "bundle";

/// The window's steps, in their order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    /// Export the working set for offline work.
    #[default]
    Export,
    /// Check the offline file with the network's checks.
    Check,
    /// Make the bundle of the offline work.
    Bundle,
    /// Bring the work back onto a branch.
    BringBack,
}

impl Step {
    /// Every step, in order.
    pub const ALL: [Step; 4] = [Step::Export, Step::Check, Step::Bundle, Step::BringBack];

    /// Its name (the menu's and the window's).
    pub fn name(self) -> &'static str {
        match self {
            Step::Export => "1 Export for offline work",
            Step::Check => "2 Check offline",
            Step::Bundle => "3 Make the bundle",
            Step::BringBack => "4 Bring the work back",
        }
    }

    /// Its action button.
    fn action(self) -> &'static str {
        match self {
            Step::Export => "Export",
            Step::Check => "Check",
            Step::Bundle => "Make the bundle",
            Step::BringBack => "Bring back",
        }
    }
}

/// The checks of the network that run offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CheckKind {
    /// Clearances between wires, by rule.
    #[default]
    Clearances,
    /// The magnetic field with the lines near.
    Fields,
    /// Loads on towers shared with other lines.
    Loads,
}

impl CheckKind {
    const ALL: [CheckKind; 3] = [CheckKind::Clearances, CheckKind::Fields, CheckKind::Loads];

    fn name(self) -> &'static str {
        match self {
            CheckKind::Clearances => "Clearances between wires",
            CheckKind::Fields => "Magnetic field",
            CheckKind::Loads => "Loads on shared towers",
        }
    }
}

/// What the window waits for from the server.
enum WaitFor {
    /// An export, to be written to this file.
    Export(PathBuf),
    /// A bundle brought back (from this file).
    BringBack(PathBuf),
}

/// A request to the server under way.
struct Waiting {
    id: u64,
    since: Instant,
    what: WaitFor,
}

/// An export written in this session.
#[derive(Debug, Clone)]
pub struct Exported {
    /// Where.
    pub file: PathBuf,
    /// What the server said of it.
    pub report: OfflineExported,
}

/// The window's state.
pub struct OfflineWindow {
    /// The step shown.
    pub step: Step,
    /// 1: the checks' reach the neighbours are taken within, m.
    pub reach: f64,
    /// 1: where the exported file goes.
    pub export_file: String,
    /// 2: the check.
    pub kind: CheckKind,
    /// 2: the weather case.
    pub case: String,
    /// 2: the clearance rules' file.
    pub rules_file: String,
    /// 2: the field's distance, m.
    pub distance: f64,
    /// 2: the current for the field, A (empty: the line's).
    pub amps: String,
    /// 2: the field's limit, µT (empty: none).
    pub limit_ut: String,
    /// 2: the towers of a check of loads (empty: every shared one).
    pub structures: String,
    /// 3: where the bundle goes.
    pub bundle_out: String,
    /// 4: the bundle brought back.
    pub bundle_in: String,
    /// 4: the branch's name (empty: the server's `offline-USER-N-K`).
    pub branch: String,
    waiting: Option<Waiting>,
    checking: Option<Receiver<Result<ScopedReport, String>>>,
    /// 1: the exports written in this session (newest last).
    pub exported: Vec<Exported>,
    /// 2: the last check's report.
    pub checked: Option<Result<ScopedReport, String>>,
    /// 3: the bundle written, in words.
    pub bundled: Option<Result<String, String>>,
    /// 4: what the server did with the bundle.
    pub brought: Option<Result<BroughtBack, String>>,
    /// The last refusal (any step).
    pub error: Option<String>,
}

impl Default for OfflineWindow {
    fn default() -> Self {
        Self {
            step: Step::Export,
            reach: DEFAULT_REACH,
            export_file: String::new(),
            kind: CheckKind::Clearances,
            case: String::new(),
            rules_file: String::new(),
            distance: 500.0,
            amps: String::new(),
            limit_ut: String::new(),
            structures: String::new(),
            bundle_out: String::new(),
            bundle_in: String::new(),
            branch: String::new(),
            waiting: None,
            checking: None,
            exported: Vec::new(),
            checked: None,
            bundled: None,
            brought: None,
            error: None,
        }
    }
}

/// The working set this connection follows, when joined to a network
/// (none: not joined, or the connection ended).
fn joined_set(r: &Ready) -> Result<WorkingSetSpec, &'static str> {
    let remote = r.model.remote.as_ref().ok_or(
        "Join the network first (File › Join a network): the export and the bundle go \
         through its server",
    )?;
    let net = remote
        .network
        .as_ref()
        .ok_or("This project is hosted by a server, not a network's working set")?;
    if remote.closed.is_some() {
        return Err("The connection to the network ended: join it again (File › Join a network)");
    }
    Ok(net.spec.clone())
}

/// The open project's record of an offline working set (none: not one).
fn offline_meta(r: &Ready) -> Result<OfflineMeta, &'static str> {
    if r.model.remote.is_some() {
        return Err(
            "Open the offline working set's file first (leave the network, then File › Open): \
             this project is the network's",
        );
    }
    r.model
        .session()
        .and_then(|s| s.offline_meta().ok().flatten())
        .ok_or("Open the offline working set's file first (the one step 1 wrote: File › Open)")
}

/// What may stop step `step` now (none: it may run).
fn blocked(r: &Ready, w: &OfflineWindow, step: Step) -> Option<String> {
    if w.waiting.is_some() || w.checking.is_some() {
        return Some("Wait for the step under way (Cancel stops waiting)".into());
    }
    let path = |s: &str, what: &str| -> Result<PathBuf, String> {
        let s = s.trim();
        if s.is_empty() {
            Err(format!("Choose {what}"))
        } else {
            Ok(PathBuf::from(s))
        }
    };
    let new_file = |s: &str, what: &str| -> Result<(), String> {
        let p = path(s, what)?;
        if p.exists() {
            return Err(format!(
                "{} exists already: choose another name (a file that may hold work is never \
                 replaced)",
                p.display()
            ));
        }
        Ok(())
    };
    let r = match step {
        Step::Export => joined_set(r).map_err(str::to_owned).and_then(|_| {
            if !(w.reach > 0.0 && w.reach <= MAX_REACH) {
                return Err(format!(
                    "The reach is more than 0 and at most {MAX_REACH:.0} m"
                ));
            }
            new_file(&w.export_file, "where the file goes")
        }),
        Step::Check => offline_meta(r).map_err(str::to_owned).and_then(|_| {
            let check = w.check();
            check.map(|_| ())
        }),
        Step::Bundle => offline_meta(r)
            .map_err(str::to_owned)
            .and_then(|_| new_file(&w.bundle_out, "where the bundle goes")),
        Step::BringBack => joined_set(r)
            .map_err(str::to_owned)
            .and_then(|_| path(&w.bundle_in, "the bundle").map(|_| ())),
    };
    r.err()
}

/// A working set in words.
fn set_text(spec: &WorkingSetSpec) -> String {
    let picks: Vec<String> = spec.picks.iter().map(|p| p.describe()).collect();
    format!("line {}: {}", spec.model, picks.join(", "))
}

/// An export's report in words; its staleness first, when stale.
pub fn export_text(e: &Exported) -> String {
    let r = &e.report;
    let mut out = format!(
        "Wrote {} ({:.1} MB): export {} of line {}, {} objects of your working set (editable), \
         {} read-only around it, {} other line(s) near ({} sections). Base revision {}, the \
         export's {}; {:.1} s on the server.",
        e.file.display(),
        r.bytes as f64 / 1e6,
        r.number,
        r.model,
        r.wanted,
        r.context,
        r.neighbour_lines,
        r.neighbour_sections,
        short(&r.base),
        short(&r.export),
        r.seconds
    );
    if r.recorded_first {
        out.push_str(
            " The main line's unrecorded changes were recorded first, by the network's server (a \
             revision).",
        );
    }
    if !r.left_out.is_empty() {
        out.push_str(&format!(
            "\nLeft out because you may not see them: {} object(s) of other lines and {} of this \
             line within the reach — checks made offline say NOT ALL CHECKED for them.",
            r.left_out.other_lines, r.left_out.this_line
        ));
    }
    out
}

/// What an export marked stale means (none: not stale).
pub fn stale_text(r: &OfflineExported) -> Option<String> {
    r.stale.then(|| {
        format!(
            "STALE: {} object(s) of your working set or within its reach changed on the server \
             while the file was written — those changes are NOT in your file (it is your working \
             set as of base revision {}). Export again for the newest state, or work on and \
             bring the work back as usual: the changes are then listed as conflicts with main or \
             as neighbours changed since.",
            r.changed_meanwhile,
            short(&r.base)
        )
    })
}

/// What a bundle brought back did, in words.
pub fn brought_text(b: &BroughtBack) -> String {
    let mut out = format!(
        "Brought back onto branch {} of line {}: {} offline revision(s), {} object(s) put and {} \
         deleted, committed; {:.1} s on the server. Merge the branch into main as any branch \
         over a working set (Versions › Revisions, tags and reviews).",
        b.branch, b.model, b.revisions, b.puts, b.deletes, b.seconds
    );
    if !b.conflicts.is_empty() {
        out.push_str(&format!(
            "\n{} object(s) you changed also changed on main since your export — the merge asks \
             you to settle them (nothing was settled): {}",
            b.conflicts.len(),
            b.conflicts.join(", ")
        ));
    }
    if b.context_changed > 0 {
        out.push_str(&format!(
            "\n{} read-only neighbour(s) changed on the server since your export: the checks you \
             made offline are out of date — check again on the network.",
            b.context_changed
        ));
    }
    out
}

/// A check's report in words (its status first).
pub fn report_text(r: &ScopedReport) -> String {
    let mut out = format!(
        "{} — {} of line {}: {} result(s), {} below what is required; {}. {:.1} s.",
        r.status, r.check, r.model, r.results, r.violations, r.reach, r.seconds
    );
    for n in &r.not_all_checked {
        out.push_str(&format!("\nNOT ALL CHECKED: {n}"));
    }
    for n in &r.notes {
        out.push_str(&format!("\n{n}"));
    }
    let shown = 200;
    for f in r.findings.iter().take(shown) {
        let against = f
            .against
            .as_deref()
            .map_or(String::new(), |a| format!(" against {a}"));
        let required = f
            .required
            .map_or(String::new(), |q| format!(" (required {q:.2})"));
        out.push_str(&format!(
            "\n{}{}: {} on {}{}: {:.2}{}",
            if f.violates { "VIOLATED " } else { "" },
            f.what,
            f.measure,
            f.on,
            against,
            f.value,
            required
        ));
    }
    if r.findings.len() > shown {
        out.push_str(&format!(
            "\n… and {} more findings",
            r.findings.len() - shown
        ));
    }
    out
}

impl OfflineWindow {
    /// The check asked (or what is missing).
    fn check(&self) -> Result<ScopedCheck, String> {
        let case = || {
            let c = self.case.trim();
            if c.is_empty() {
                Err("Choose the weather case".to_owned())
            } else {
                Ok(c.to_owned())
            }
        };
        let number = |s: &str, what: &str| -> Result<Option<f64>, String> {
            let s = s.trim();
            if s.is_empty() {
                return Ok(None);
            }
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v > 0.0)
                .map(Some)
                .ok_or_else(|| format!("{what}: a number more than 0, or empty"))
        };
        Ok(match self.kind {
            CheckKind::Clearances => {
                let file = self.rules_file.trim();
                if file.is_empty() {
                    return Err("Choose the clearance rules' file".into());
                }
                let rules = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
                ScopedCheck::Clearances {
                    case: case()?,
                    rules,
                }
            }
            CheckKind::Fields => {
                if !(self.distance > 0.0 && self.distance <= 2000.0) {
                    return Err("The field's distance is more than 0 and at most 2000 m".into());
                }
                ScopedCheck::Fields {
                    case: case()?,
                    distance: Some(self.distance),
                    amps: number(&self.amps, "Current")?,
                    limit_ut: number(&self.limit_ut, "Limit")?,
                }
            }
            CheckKind::Loads => ScopedCheck::StructureLoads {
                structures: self
                    .structures
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
            },
        })
    }
}

/// Opens the window at `step` (kept as it was when open already).
pub fn open(r: &mut Ready, step: Step) {
    let mut w = r.network_offline.take().unwrap_or_default();
    w.step = step;
    if w.bundle_out.is_empty() && step == Step::Bundle {
        w.bundle_out = r
            .model
            .sources
            .file
            .with_extension(BUNDLE_EXTENSION)
            .display()
            .to_string();
    }
    if w.case.is_empty() {
        w.case = r
            .model
            .cases
            .first()
            .map(|c| c.case.name.clone())
            .unwrap_or_default();
    }
    w.error = None;
    r.network_offline = Some(w);
}

/// The entries of Versions › Offline copy for a network's working set.
pub fn menu(ui: &mut egui::Ui, r: &mut Ready) {
    ui.label(
        egui::RichText::new("A network's working set")
            .small()
            .weak(),
    );
    let joined = joined_set(r).err();
    let offline = offline_meta(r).err();
    let entries = [
        (
            Step::Export,
            "Export working set for offline work…",
            "Your working set as a project file to take where there is no connection, with \
             the read-only neighbours the network's checks need",
            joined,
        ),
        (
            Step::Check,
            "Check the offline working set…",
            "The network's checks on the offline file, with the neighbours it holds",
            offline,
        ),
        (
            Step::Bundle,
            "Make the bundle of offline work…",
            "Only your revisions since the export, in one file to bring back",
            offline,
        ),
        (
            Step::BringBack,
            "Bring offline work back…",
            "The bundle onto a new branch over your working set (never main); its conflicts \
             with main listed",
            joined,
        ),
    ];
    for (step, label, tip, why_not) in entries {
        let b = ui
            .add_enabled(why_not.is_none(), egui::Button::new(label))
            .on_hover_text(tip);
        let b = match why_not {
            Some(w) => b.on_disabled_hover_text(w),
            None => b,
        };
        if b.clicked() {
            open(r, step);
            ui.close();
        }
    }
}

/// Takes the server's answers to the window: those it waits for, and
/// those of a window closed meanwhile (said in the status line).
fn take_answers(r: &mut Ready) {
    let answers = match r.model.remote.as_mut().and_then(|x| x.network.as_mut()) {
        Some(n) if !n.offline_answers.is_empty() => std::mem::take(&mut n.offline_answers),
        _ => return,
    };
    for a in answers {
        let id = match &a {
            ServerMsg::WorkingSetExported { id, .. } | ServerMsg::BundleImported { id, .. } => *id,
            _ => continue,
        };
        let w = r.network_offline.as_mut();
        let waited = w
            .as_ref()
            .and_then(|w| w.waiting.as_ref())
            .is_some_and(|x| x.id == id);
        if !waited {
            r.status = Some((late_text(&a), false));
            continue;
        }
        let Some(w) = w else { continue };
        let Some(waiting) = w.waiting.take() else {
            continue;
        };
        match (a, waiting.what) {
            (ServerMsg::WorkingSetExported { result, .. }, WaitFor::Export(file)) => {
                // The server's word on the export kept in the file: its
                // check windows say it later (stale, what changed
                // meanwhile; 2026-11-23).
                let written = result.and_then(|f| {
                    write_export(&file, &f.file)?;
                    if let Err(e) = crate::offline_neighbours::record_export(&file, &f.report) {
                        log::warn!("the export's record not kept in {}: {e}", file.display());
                    }
                    Ok(f.report)
                });
                match written {
                    Ok(report) => {
                        let e = Exported { file, report };
                        r.status = Some((
                            match stale_text(&e.report) {
                                Some(_) => format!(
                                    "offline working set exported to {} — STALE: {} object(s) \
                                     changed meanwhile",
                                    e.file.display(),
                                    e.report.changed_meanwhile
                                ),
                                None => {
                                    format!("offline working set exported to {}", e.file.display())
                                }
                            },
                            e.report.stale,
                        ));
                        w.exported.push(e);
                        w.error = None;
                    }
                    Err(e) => w.error = Some(format!("Not exported: {e}")),
                }
            }
            (ServerMsg::BundleImported { result, .. }, WaitFor::BringBack(file)) => {
                match &result {
                    Ok(b) => {
                        r.status = Some((
                            format!(
                                "offline work of {} brought back onto branch {}",
                                file.display(),
                                b.branch
                            ),
                            false,
                        ));
                        w.error = None;
                    }
                    Err(e) => w.error = Some(format!("Not brought back: {e}")),
                }
                w.brought = Some(result.map(|b| *b));
            }
            _ => {}
        }
    }
}

/// An answer that came after its window stopped waiting, in words.
fn late_text(a: &ServerMsg) -> String {
    match a {
        ServerMsg::WorkingSetExported { result: Ok(f), .. } => format!(
            "the network exported line {} (export {}) after its window stopped waiting: no file \
             was written here — export again",
            f.report.model, f.report.number
        ),
        ServerMsg::WorkingSetExported { result: Err(e), .. } => {
            format!("the offline export was refused: {e}")
        }
        ServerMsg::BundleImported { result: Ok(b), .. } => format!(
            "offline work was brought back onto branch {} of line {}",
            b.branch, b.model
        ),
        ServerMsg::BundleImported { result: Err(e), .. } => {
            format!("the bundle was refused: {e}")
        }
        _ => String::new(),
    }
}

/// Writes the exported file (base64 from the server) to `path`, which
/// must not exist.
fn write_export(path: &Path, base64: &str) -> Result<(), String> {
    let bytes = ohl_server::base64::decode(base64)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::Write::write_all(&mut f, &bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Runs step `step` (its blockers checked before).
fn run(r: &mut Ready, w: &mut OfflineWindow, ctx: &egui::Context) {
    w.error = None;
    match w.step {
        Step::Export => {
            let Ok(spec) = joined_set(r) else { return };
            let Some(remote) = r.model.remote.as_mut() else {
                return;
            };
            let id = remote.link.next_id();
            let file = PathBuf::from(w.export_file.trim());
            match remote.link.send(ClientMsg::ExportWorkingSet {
                id,
                working_set: spec,
                reach: Some(w.reach),
            }) {
                Ok(()) => {
                    w.waiting = Some(Waiting {
                        id,
                        since: Instant::now(),
                        what: WaitFor::Export(file),
                    });
                }
                Err(e) => w.error = Some(format!("Not sent: {e}")),
            }
        }
        Step::Check => {
            let check = match w.check() {
                Ok(c) => c,
                Err(e) => {
                    w.error = Some(e);
                    return;
                }
            };
            let path = r.model.sources.file.clone();
            let (tx, rx) = channel();
            let c = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(ohl_server::offline_checks::check_offline(&path, &check));
                c.request_repaint();
            });
            w.checked = None;
            w.checking = Some(rx);
        }
        Step::Bundle => {
            let out = PathBuf::from(w.bundle_out.trim());
            w.bundled = Some(make_bundle(r, &out));
        }
        Step::BringBack => {
            let file = PathBuf::from(w.bundle_in.trim());
            let bundle = match read_bundle(&file) {
                Ok(b) => b,
                Err(e) => {
                    w.error = Some(e);
                    return;
                }
            };
            let Some(remote) = r.model.remote.as_mut() else {
                return;
            };
            let id = remote.link.next_id();
            let branch = Some(w.branch.trim().to_owned()).filter(|b| !b.is_empty());
            match remote.link.send(ClientMsg::ImportBundle {
                id,
                bundle: Box::new(bundle),
                branch,
            }) {
                Ok(()) => {
                    w.brought = None;
                    w.waiting = Some(Waiting {
                        id,
                        since: Instant::now(),
                        what: WaitFor::BringBack(file),
                    });
                }
                Err(e) => w.error = Some(format!("Not sent: {e}")),
            }
        }
    }
}

/// The bundle of the open offline working set written to `out` (must
/// not exist), in words.
fn make_bundle(r: &Ready, out: &Path) -> Result<String, String> {
    let s = r.model.session().ok_or("no project open")?;
    let b = s.offline_bundle().map_err(|e| e.to_string())?;
    let text = serde_json::to_string(&b).map_err(|e| e.to_string())?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    std::io::Write::write_all(&mut f, text.as_bytes())
        .map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(format!(
        "Wrote {} ({:.1} kB): {} revision(s) since the export, up to {}. Bring it back with \
         step 4 once joined to the network.",
        out.display(),
        text.len() as f64 / 1e3,
        b.revisions.len(),
        short(&b.head)
    ))
}

/// A bundle read from a file.
fn read_bundle(path: &Path) -> Result<Bundle, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: not a bundle: {e}", path.display()))
}

/// A file path field with its Choose… button (a save or an open dialog).
fn file_field(ui: &mut egui::Ui, value: &mut String, save: Option<String>, filter: (&str, &str)) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(380.0));
        if ui.button("Choose…").clicked() {
            let d = rfd::FileDialog::new().add_filter(filter.0, &[filter.1]);
            let picked = match save {
                Some(name) => d.set_file_name(name).save_file(),
                None => d.pick_file(),
            };
            if let Some(p) = picked {
                *value = p.display().to_string();
            }
        }
    });
}

/// The window, when open (and the server's answers, open or not).
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    take_answers(r);
    let Some(mut w) = r.network_offline.take() else {
        return;
    };
    if let Some(rx) = &w.checking
        && let Ok(result) = rx.try_recv()
    {
        w.checking = None;
        w.checked = Some(result);
    }
    let mut open = true;
    let mut close = false;
    let mut go = false;
    egui::Window::new("Offline working set")
        .id(egui::Id::new("network-offline-window"))
        .open(&mut open)
        .collapsible(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            tool_ui::intro(
                ui,
                "Take your network working set where there is no connection and bring the work \
                 back: export it, work and check offline, make the bundle, bring it back.",
                "1 Export (joined to the network): your working set as a project file, editable, \
                 with its context and the neighbours within the reach read-only, so the \
                 network's checks run offline. 2 and 3 (the file open, offline): the checks, \
                 then the bundle of only your revisions since the export (commit first). 4 \
                 (joined again): the server checks the bundle against the export it recorded \
                 and puts your changes on a new branch over your working set — never on main — \
                 by your rights as they are then; conflicts with main are listed, never \
                 settled. Nothing is reserved for you while you are offline.",
                GUIDE,
            );
            ui.horizontal_wrapped(|ui| {
                for s in Step::ALL {
                    ui.selectable_value(&mut w.step, s, s.name());
                }
            });
            ui.separator();
            match w.step {
                Step::Export => export_ui(ui, r, &mut w),
                Step::Check => check_ui(ui, r, &mut w),
                Step::Bundle => bundle_ui(ui, r, &mut w),
                Step::BringBack => bring_back_ui(ui, r, &mut w),
            }
            if let Some(waiting) = &w.waiting {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "Waiting for the network's server ({:.0} s; long work waits behind the \
                         others' edits)… Cancel stops waiting.",
                        waiting.since.elapsed().as_secs_f64()
                    ));
                });
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            if w.checking.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking on the offline file…");
                });
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            if let Some(e) = &w.error {
                crate::menu::error_label(ui, e.as_str());
            }
            let why_not = blocked(r, &w, w.step);
            let pressed = tool_ui::buttons(
                ui,
                w.step.action(),
                why_not.is_none(),
                why_not.as_deref(),
                tool_ui::Keys::Escape,
            );
            if ui.input(|i| i.key_pressed(egui::Key::Escape))
                || pressed == Some(tool_ui::Pressed::Cancel)
            {
                close = true;
            } else if pressed == Some(tool_ui::Pressed::Action) {
                go = true;
            }
        });
    if go {
        run(r, &mut w, ctx);
    }
    if open && !close {
        r.network_offline = Some(w);
    }
}

fn export_ui(ui: &mut egui::Ui, r: &Ready, w: &mut OfflineWindow) {
    tool_ui::heading(
        ui,
        "Export for offline work",
        "The network's server writes your working set as a project file of today's format: \
         your working set editable; its context and your line's neighbours within the reach \
         read-only; other lines' wires within the reach kept for the checks. Anything you may \
         not see is left out and only counted. The export is recorded on the server (a bundle \
         is checked against it) and in the audit.",
    );
    match joined_set(r) {
        Ok(spec) => {
            ui.label(format!("Your working set: {}.", set_text(&spec)));
        }
        Err(e) => {
            ui.label(egui::RichText::new(e).weak());
        }
    }
    ui.horizontal(|ui| {
        tool_ui::label(
            ui,
            "Reach (m)",
            "How far around your working set the neighbours are taken (500 m by default, at \
             most 10 000 m): a check made offline that reaches further says NOT ALL CHECKED. A \
             larger reach makes a larger file.",
        );
        ui.add(
            egui::DragValue::new(&mut w.reach)
                .range(1.0..=MAX_REACH)
                .speed(10.0)
                .suffix(" m"),
        );
    });
    tool_ui::label(
        ui,
        "File",
        "Where the offline working set is written (a project file, .ohl). An existing file is \
         never replaced: it may hold work.",
    );
    let name = joined_set(r)
        .map(|s| format!("{}-offline.ohl", s.model))
        .unwrap_or_else(|_| "offline.ohl".into());
    file_field(ui, &mut w.export_file, Some(name), ("project", "ohl"));
    for e in w.exported.iter().rev() {
        ui.separator();
        if let Some(s) = stale_text(&e.report) {
            crate::menu::error_label(ui, egui::RichText::new(s).strong());
        }
        crate::menu::message(ui, export_text(e), None);
    }
}

fn check_ui(ui: &mut egui::Ui, r: &Ready, w: &mut OfflineWindow) {
    tool_ui::heading(
        ui,
        "Check offline",
        "A check of the network's run on the open offline file with the neighbours it holds — \
         the server's own code, so the same neighbours give the server's report. Where the file \
         cannot give it (neighbours you may not see were left out, the check reaches beyond \
         what was exported, the working set moved far) it says NOT ALL CHECKED.",
    );
    match offline_meta(r) {
        Ok(m) => {
            ui.label(format!(
                "Offline working set of line {}, exported by {} on {} (base {}, reach {:.0} m).",
                m.model,
                m.user,
                m.time,
                short(&m.base),
                m.reach_m
            ));
            if !m.left_out.is_empty() {
                crate::menu::error_label(
                    ui,
                    format!(
                        "{} object(s) you may not see were left out: checks reaching them say \
                         NOT ALL CHECKED.",
                        m.left_out.other_lines + m.left_out.this_line
                    ),
                );
            }
        }
        Err(e) => {
            ui.label(egui::RichText::new(e).weak());
        }
    }
    ui.horizontal(|ui| {
        tool_ui::label(
            ui,
            "Which check",
            "Which check of the network to run on the file.",
        );
        egui::ComboBox::from_id_salt("offline-check-kind")
            .selected_text(w.kind.name())
            .show_ui(ui, |ui| {
                for k in CheckKind::ALL {
                    ui.selectable_value(&mut w.kind, k, k.name());
                }
            });
    });
    if w.kind != CheckKind::Loads {
        ui.horizontal(|ui| {
            tool_ui::label(
                ui,
                "Weather case",
                "The case of your line the wires are solved in (its weather cases).",
            );
            egui::ComboBox::from_id_salt("offline-check-case")
                .selected_text(w.case.clone())
                .show_ui(ui, |ui| {
                    for c in &r.model.cases {
                        ui.selectable_value(&mut w.case, c.case.name.clone(), &c.case.name);
                    }
                });
        });
    }
    match w.kind {
        CheckKind::Clearances => {
            tool_ui::label(
                ui,
                "Rules",
                "The clearance rules' file (.ohlr) the network checks with: distances between \
                 your wires and the neighbours' by rule.",
            );
            file_field(ui, &mut w.rules_file, None, ("rules", "ohlr"));
        }
        CheckKind::Fields => {
            ui.horizontal(|ui| {
                tool_ui::label(
                    ui,
                    "Distance (m)",
                    "How far from your line the field is computed, with the lines near (more \
                     than 0, at most 2000 m; 500 m by default).",
                );
                ui.add(
                    egui::DragValue::new(&mut w.distance)
                        .range(1.0..=2000.0)
                        .suffix(" m"),
                );
            });
            ui.horizontal(|ui| {
                tool_ui::label(
                    ui,
                    "Current (A)",
                    "The current of every circuit (empty: each line's own settings).",
                );
                ui.add(egui::TextEdit::singleline(&mut w.amps).desired_width(80.0));
                tool_ui::label(
                    ui,
                    "Limit (µT)",
                    "A limit the field is compared with (empty: none, the field is reported).",
                );
                ui.add(egui::TextEdit::singleline(&mut w.limit_ut).desired_width(80.0));
            });
        }
        CheckKind::Loads => {
            ui.horizontal(|ui| {
                tool_ui::label(
                    ui,
                    "Towers",
                    "The towers shared with other lines to check, separated by commas (empty: \
                     every shared tower of your working set): each under each line's cases, the \
                     worst kept.",
                );
                ui.add(egui::TextEdit::singleline(&mut w.structures).desired_width(260.0));
            });
        }
    }
    match &w.checked {
        Some(Ok(report)) => {
            ui.separator();
            // Green only when every part was checked and nothing is
            // below what is required.
            if !report.not_all_checked.is_empty() {
                crate::menu::error_label(ui, egui::RichText::new(&report.status).strong());
            } else if report.violations > 0 {
                crate::menu::error_label(
                    ui,
                    egui::RichText::new(format!(
                        "{}: {} result(s) below what is required",
                        report.status, report.violations
                    ))
                    .strong(),
                );
            } else {
                ui.colored_label(crate::menu::ok_color(ui), &report.status);
            }
            crate::menu::message(ui, report_text(report), None);
        }
        Some(Err(e)) => {
            ui.separator();
            crate::menu::error_label(ui, format!("Not checked: {e}"));
        }
        None => {}
    }
}

fn bundle_ui(ui: &mut egui::Ui, r: &Ready, w: &mut OfflineWindow) {
    tool_ui::heading(
        ui,
        "The bundle of your offline work",
        "The bundle holds only your revisions since the export and what they changed — no \
         copy of the line. Commit your work first: uncommitted changes refuse it. The \
         read-only neighbours and the library files cannot be changed offline.",
    );
    match offline_meta(r) {
        Ok(m) => {
            ui.label(format!(
                "Offline working set of line {}, exported by {} on {}.",
                m.model, m.user, m.time
            ));
            if let Some(s) = r.model.session()
                && s.has_uncommitted(s.current_line()).unwrap_or(false)
            {
                crate::menu::error_label(
                    ui,
                    "This file has uncommitted changes: commit them first (Versions › Commit…).",
                );
            }
        }
        Err(e) => {
            ui.label(egui::RichText::new(e).weak());
        }
    }
    tool_ui::label(
        ui,
        "Bundle",
        "Where the bundle is written (one file to bring back with step 4). An existing file is \
         never replaced.",
    );
    file_field(
        ui,
        &mut w.bundle_out,
        Some(format!("work.{BUNDLE_EXTENSION}")),
        ("bundle", BUNDLE_EXTENSION),
    );
    match &w.bundled {
        Some(Ok(t)) => {
            ui.separator();
            crate::menu::message(ui, t.as_str(), Some(crate::menu::ok_color(ui)));
        }
        Some(Err(e)) => {
            ui.separator();
            crate::menu::error_label(ui, format!("No bundle: {e}"));
        }
        None => {}
    }
}

fn bring_back_ui(ui: &mut egui::Ui, r: &Ready, w: &mut OfflineWindow) {
    tool_ui::heading(
        ui,
        "Bring the work back",
        "The server checks the bundle against the export it recorded and applies your changes \
         on a new branch over your working set made from the export's base — never on main — \
         by your rights as they are now and the rules of shared towers, then commits it. \
         Refused, nothing kept: a bundle changing a read-only neighbour or a library file, \
         changed by hand, of another user's export, or brought back already.",
    );
    if let Err(e) = joined_set(r) {
        ui.label(egui::RichText::new(e).weak());
    }
    tool_ui::label(ui, "Bundle", "The bundle step 3 made.");
    file_field(ui, &mut w.bundle_in, None, ("bundle", BUNDLE_EXTENSION));
    ui.horizontal(|ui| {
        tool_ui::label(
            ui,
            "Branch",
            "The new branch's name (empty: offline-USER-N-K, your name and the export's).",
        );
        ui.add(
            egui::TextEdit::singleline(&mut w.branch)
                .desired_width(200.0)
                .hint_text("offline-…"),
        );
    });
    match &w.brought {
        Some(Ok(b)) => {
            ui.separator();
            if !b.conflicts.is_empty() || b.context_changed > 0 {
                crate::menu::error_label(
                    ui,
                    egui::RichText::new(format!(
                        "Changed meanwhile on the server: {} conflict(s) with main, {} \
                         neighbour(s) changed since the export.",
                        b.conflicts.len(),
                        b.context_changed
                    ))
                    .strong(),
                );
            }
            crate::menu::message(ui, brought_text(b), None);
        }
        Some(Err(e)) => {
            ui.separator();
            crate::menu::error_label(ui, format!("Not brought back: {e}"));
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exported(stale: bool) -> OfflineExported {
        OfflineExported {
            model: "L001".into(),
            number: 3,
            bytes: 2_500_000,
            wanted: 120,
            context: 40,
            neighbour_lines: 2,
            neighbour_sections: 9,
            left_out: Default::default(),
            base: "a".repeat(64),
            export: "b".repeat(64),
            recorded_first: false,
            seconds: 0.4,
            changed_meanwhile: if stale { 5 } else { 0 },
            stale,
        }
    }

    /// A stale export says so, with what changed meanwhile; a fresh one
    /// says nothing of it; a bundle brought back lists its conflicts and
    /// the neighbours changed since.
    #[test]
    fn stale_exports_and_changes_meanwhile_in_words() {
        assert_eq!(stale_text(&exported(false)), None);
        let s = stale_text(&exported(true)).unwrap();
        assert!(
            s.starts_with("STALE: 5 object(s)") && s.contains("NOT in your file"),
            "{s}"
        );
        let e = Exported {
            file: "ws.ohl".into(),
            report: exported(false),
        };
        let t = export_text(&e);
        assert!(
            t.contains("export 3 of line L001") && t.contains("120 objects"),
            "{t}"
        );
        let b = BroughtBack {
            model: "L001".into(),
            branch: "offline-ben-3-1".into(),
            revisions: 2,
            puts: 4,
            deletes: 0,
            conflicts: vec!["structure L001-T5".into()],
            context_changed: 1,
            seconds: 0.3,
        };
        let t = brought_text(&b);
        assert!(t.contains("branch offline-ben-3-1"), "{t}");
        assert!(
            t.contains("structure L001-T5") && t.contains("nothing was settled"),
            "{t}"
        );
        assert!(t.contains("out of date"), "{t}");
    }
}
