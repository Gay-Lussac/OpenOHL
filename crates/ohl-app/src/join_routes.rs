// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! File › **Join routes from another project…** (roadmap O17, *One model
//! per line*; owner, 2026-11-25): a line and its branches are one model;
//! where a line was split into several project files, the routes of
//! another file are joined into the open project — one change (one undo
//! step) and one revision "joined routes from FILE"
//! ([`ohl_session::join`]).
//!
//! The window follows the tool windows' rules
//! (docs/development/tool-pages.md): a sentence and its (?), User guide ↗,
//! a numbered setup (1 the file, 2 the prefix of renamed ids, 3 the twin
//! distance), then the preview of what will be joined — the routes and
//! objects, the ids renamed, the library entries mapped (a conflict with
//! its tick to accept this project's entry), the twins with their tick
//! *make one shared tower* (none ticked: the owner, never a silent
//! change), what to look at — long lists in boxes of fixed height that
//! scroll; one button row (Join first, Cancel last); Escape cancels; the
//! window closable.

use std::collections::BTreeSet;
use std::path::PathBuf;

use eframe::egui;
use ohl_session::join::{JoinOptions, JoinPlan, Outcome, TwinState};

use crate::app::Ready;
use crate::tool_ui;

/// The manual's section of the window.
pub const GUIDE: (&str, &str) = ("user-guide/join-routes", "the-join-routes-window");

/// Height of the preview's lists, points (they scroll beyond it).
const LIST_HEIGHT: f32 = 140.0;

/// The window's state.
#[derive(Default)]
pub struct JoinWindow {
    /// Shown.
    pub open: bool,
    /// The other project file.
    pub file: Option<PathBuf>,
    /// The prefix of renamed ids.
    pub prefix: String,
    /// The twin distance, m.
    pub tolerance: f64,
    /// The twins ticked (this project's, the other's).
    pub merge: BTreeSet<(String, String)>,
    /// The library conflicts accepted.
    pub accept: BTreeSet<String>,
    /// The plan, and the line and libraries revision it was made for.
    pub plan: Option<(Result<JoinPlan, String>, (u64, u64))>,
    /// Asked to make the plan again (a setting changed).
    pub stale: bool,
    /// What went wrong, or was done.
    pub status: Option<(String, bool)>,
}

impl JoinWindow {
    /// Opens the window (a new join), on `file` when given.
    pub fn show(&mut self, file: Option<PathBuf>) {
        *self = Self {
            open: true,
            tolerance: ohl_session::join::TWIN_TOLERANCE,
            ..Self::default()
        };
        if let Some(f) = file {
            self.set_file(f);
        }
    }

    /// The other file chosen: the prefix from its name, nothing ticked.
    pub fn set_file(&mut self, file: PathBuf) {
        self.prefix = prefix_of(&file);
        self.file = Some(file);
        self.merge.clear();
        self.accept.clear();
        self.plan = None;
        self.stale = true;
        self.status = None;
    }
}

/// A prefix for renamed ids from a file's name: its letters, digits, `_`,
/// `.` and `-` (at most 32), else `J`.
pub fn prefix_of(file: &std::path::Path) -> String {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let p: String = stem
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .skip_while(|c| !c.is_ascii_alphanumeric())
        .take(32)
        .collect();
    if ohl_session::join::check_prefix(&p).is_ok() {
        p
    } else {
        "J".into()
    }
}

/// The plan made again when asked or when the project changed.
fn replan(r: &mut Ready) {
    let stamp = (r.model.line_version, r.model.revisions.libraries);
    let w = &mut r.join_routes;
    let Some(file) = w.file.clone() else {
        w.plan = None;
        return;
    };
    if !(w.stale || w.plan.as_ref().is_none_or(|(_, s)| *s != stamp)) {
        return;
    }
    w.stale = false;
    let same =
        std::fs::canonicalize(&file).ok() == std::fs::canonicalize(&r.model.sources.file).ok();
    let result = if same {
        Err("this is the open project itself: choose another project file".to_owned())
    } else {
        match r.model.session() {
            None => Err("no project open to join into".to_owned()),
            Some(s) => {
                let libs = s
                    .line_library_all(s.current_line())
                    .map_err(|e| e.to_string());
                libs.and_then(|libs| {
                    let options = JoinOptions {
                        prefix: w.prefix.trim().to_owned(),
                        tolerance: w.tolerance,
                        merge: w.merge.clone(),
                        accept: w.accept.clone(),
                    };
                    ohl_session::join::plan(&s.export(), &libs, &file, &options)
                        .map_err(|e| e.to_string())
                })
            }
        }
    };
    w.plan = Some((result, stamp));
}

/// The window, when open.
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    if !r.join_routes.open {
        return;
    }
    replan(r);
    let editable = r.model.can_edit();
    let current = r.model.project_line();
    let mut open = true;
    let mut pressed = None;
    let w = &mut r.join_routes;
    egui::Window::new("Join routes from another project")
        .id(egui::Id::new("join-routes-window"))
        .open(&mut open)
        .collapsible(false)
        .default_width(720.0)
        .vscroll(false)
        .show(ctx, |ui| {
            tool_ui::intro(
                ui,
                "Join the routes of another project file into this project: one model for a line \
                 and its branches. Look at what will be joined below, then Join.",
                "A line branching (a Y or T connection) is one model, its shared towers one \
                 object each. Where it was split into several project files, the other file's \
                 main project line comes in: its routes with their alignments, structures \
                 (strings and wire points), sections, jumpers, spacers, electrical systems, \
                 circuits and structure groups. Ids clashing with this project's are renamed with \
                 the prefix. Library entries: the same reused, new ones added, one that differs \
                 is a conflict. Towers of the same type within the twin distance are listed; one \
                 ticked becomes one shared tower (this project's kept, the other line's sections \
                 re-attached by wire point label). The other file is never changed. One change \
                 (Edit › Undo takes it back) and one revision \"joined routes from FILE\".",
                GUIDE,
            );
            if let Err(why) = &editable {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            // The body scrolls; the buttons stay below it.
            let body = (ctx.content_rect().bottom() - ui.cursor().top() - 130.0).max(160.0);
            egui::ScrollArea::vertical()
                .id_salt("join-routes-body")
                .max_height(body)
                .auto_shrink([false, true])
                .show(ui, |ui| setup_and_preview(ui, w, &current));
            if let Some((s, error)) = &w.status {
                if *error {
                    crate::menu::error_label(ui, s.as_str());
                } else {
                    crate::menu::message(ui, s.as_str(), Some(ohl_ui::messages::ok_color(ui)));
                }
            }
            let why_not = match (&editable, w.plan.as_ref().map(|(p, _)| p)) {
                (Err(why), _) => Some(format!("read-only: {why}")),
                (_, None) => Some("choose the project file to join first".to_owned()),
                (_, Some(Err(e))) => Some(e.clone()),
                (_, Some(Ok(p))) if !p.can_apply() => {
                    Some(format!("refused until settled: {}", p.blocked.join("; ")))
                }
                _ => None,
            };
            pressed = tool_ui::buttons(
                ui,
                "Join",
                why_not.is_none(),
                why_not.as_deref(),
                tool_ui::Keys::Escape,
            );
        });
    if pressed.is_none()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        pressed = Some(tool_ui::Pressed::Cancel);
    }
    match pressed {
        Some(tool_ui::Pressed::Action) => match apply(r) {
            Ok(done) => {
                r.join_routes.open = false;
                r.status = Some((done, false));
            }
            Err(e) => r.join_routes.status = Some((e, true)),
        },
        Some(tool_ui::Pressed::Cancel) => {
            r.join_routes.open = false;
            r.status = Some(("Join routes: cancelled, nothing changed".into(), false));
        }
        None if !open => r.join_routes.open = false,
        None => {}
    }
}

/// The numbered setup, then the preview.
fn setup_and_preview(ui: &mut egui::Ui, w: &mut JoinWindow, current: &str) {
    // 1. The file.
    ui.label(egui::RichText::new("1. The project file to join").strong());
    ui.horizontal(|ui| {
        if ui
            .button("Choose the file…")
            .on_hover_text("Another project file (.ohl); it is read, never changed")
            .clicked()
            && let Some(p) = rfd::FileDialog::new()
                .add_filter("project", &["ohl"])
                .pick_file()
        {
            w.set_file(p);
        }
        ui.label(w.file.as_ref().map_or("no file chosen".to_owned(), |p| {
            p.file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned())
        }))
        .on_hover_text(
            w.file
                .as_ref()
                .map_or(String::new(), |p| p.display().to_string()),
        );
    });
    ui.label(format!("Joined into the project line {current}."));
    // 2. The prefix.
    ui.add_space(4.0);
    ui.label(egui::RichText::new("2. Ids renamed").strong());
    ui.horizontal(|ui| {
        tool_ui::label(
            ui,
            "Prefix",
            "An id of the other file that this project has already (a structure, section, \
             alignment point, route, electrical system, circuit, structure group…) is renamed \
             PREFIX-id (PREFIX-id-2 when that is taken too), with everything that refers to it, \
             and listed below. 1 to 32 letters, digits, _, . or -. Default: from the file's \
             name.",
        );
        if ui
            .add(egui::TextEdit::singleline(&mut w.prefix).desired_width(160.0))
            .changed()
        {
            w.stale = true;
        }
    });
    // 3. The twin distance.
    ui.add_space(4.0);
    ui.label(egui::RichText::new("3. Twins").strong());
    ui.horizontal(|ui| {
        tool_ui::label(
            ui,
            "Twin distance (m)",
            "A tower of the other file of the same type as one of this project's, at most this \
             far from it in plan, is listed as a twin (0.5 m by default, as the network import \
             finds twins). Nothing is merged unless you tick it.",
        );
        if ui
            .add(
                egui::DragValue::new(&mut w.tolerance)
                    .range(0.0..=10.0)
                    .speed(0.05)
                    .suffix(" m"),
            )
            .changed()
        {
            w.stale = true;
        }
    });
    let plan = match w.plan.as_ref().map(|(p, _)| p) {
        None => return,
        Some(Err(e)) => {
            ui.separator();
            crate::menu::error_label(ui, e.as_str());
            return;
        }
        Some(Ok(p)) => p.clone(),
    };
    ui.separator();
    preview(ui, w, &plan);
}

/// The preview of what will be joined.
fn preview(ui: &mut egui::Ui, w: &mut JoinWindow, p: &JoinPlan) {
    tool_ui::heading(
        ui,
        "What will be joined",
        "The other file's main project line as it is now (its uncommitted state too), after the \
         renaming. Nothing changes before Join.",
    );
    ui.label(format!(
        "From {} (line \"{}\"{}): routes {}.",
        p.file,
        p.other_name,
        p.source_revision
            .as_deref()
            .map_or(", uncommitted".to_owned(), |r| format!(
                ", revision {}",
                ohl_session::versions::short(r)
            )),
        p.routes
            .iter()
            .map(|(a, b)| if a == b {
                a.clone()
            } else {
                format!("{a} (as {b})")
            })
            .collect::<Vec<_>>()
            .join(", ")
    ));
    ui.label(
        p.objects
            .iter()
            .map(|(k, n)| format!("{n} {k}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    // Renamed.
    ui.add_space(4.0);
    tool_ui::heading(
        ui,
        &format!("Ids renamed ({})", p.renamed.len()),
        "Ids of the other file that this project has already, renamed with the prefix; what \
         refers to them follows.",
    );
    if !p.renamed.is_empty() {
        list_box(ui, "join-renamed", |ui| {
            for r in &p.renamed {
                ui.label(format!(
                    "{} {} ➡ {}",
                    r.kind.replace('_', " "),
                    r.from,
                    r.to
                ));
            }
        });
    }
    // Libraries.
    ui.add_space(4.0);
    tool_ui::heading(
        ui,
        &format!("Library entries ({})", p.libraries.len()),
        "As the network import maps them: an entry this project has alike is reused, a new one \
         added to this project's file of that name, one this project has with other values is \
         a conflict — the join is refused until you tick it (this project's entry then used for \
         the joined routes: their results may change) or make the entries the same. A draft's \
         file or a file that is not TOML is left in the other file.",
    );
    if !p.libraries.is_empty() {
        list_box(ui, "join-libraries", |ui| {
            for m in &p.libraries {
                match &m.outcome {
                    Outcome::Conflict { .. } => {
                        let key = m.key();
                        let mut on = w.accept.contains(&key);
                        ui.colored_label(ui.visuals().warn_fg_color, m.words());
                        if ui
                            .checkbox(&mut on, "use this project's entry for the joined routes")
                            .on_hover_text(
                                "The joined routes then take this project's entry: their results \
                                 may change. Not ticked: the join is refused.",
                            )
                            .changed()
                        {
                            if on {
                                w.accept.insert(key);
                            } else {
                                w.accept.remove(&key);
                            }
                            w.stale = true;
                        }
                    }
                    _ => {
                        ui.label(m.words());
                    }
                }
            }
        });
    }
    // Twins.
    ui.add_space(4.0);
    tool_ui::heading(
        ui,
        &format!("Twins ({})", p.twins.len()),
        "Towers of the other file of the same type within the twin distance of one of this \
         project's. Not ticked (the default): two towers. Ticked: one shared tower — this \
         project's tower kept as it is, the other line's sections, jumpers and circuit starts \
         re-attached to its wire points by label; a wire point whose label this project's tower \
         has not refuses that twin (two towers, listed). The height is the highest point above \
         the tower's ground point; the level is of the ground points.",
    );
    if !p.twins.is_empty() {
        list_box(ui, "join-twins", |ui| {
            egui::Grid::new("join-twins-grid")
                .striped(true)
                .show(ui, |ui| {
                    for h in [
                        "make one shared tower",
                        "this project",
                        "joined",
                        "apart",
                        "height",
                        "level",
                        "turned",
                        "",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for t in &p.twins {
                        let key = t.key();
                        let mut on = w.merge.contains(&key);
                        if ui
                            .checkbox(&mut on, "")
                            .on_hover_text(format!(
                                "Make {} and {} one shared tower ({} kept)",
                                t.open, t.other, t.open
                            ))
                            .changed()
                        {
                            if on {
                                w.merge.insert(key);
                            } else {
                                w.merge.remove(&key);
                            }
                            w.stale = true;
                        }
                        ui.label(&t.open);
                        ui.label(if t.joined == t.other {
                            t.other.clone()
                        } else {
                            format!("{} (as {})", t.other, t.joined)
                        });
                        ui.label(format!("{:.3} m", t.distance_m));
                        ui.label(
                            t.height_difference_m
                                .map_or("—".to_owned(), |h| format!("{h:+.2} m")),
                        );
                        ui.label(format!("{:+.2} m", t.level_difference_m));
                        ui.label(format!("{:+.1}°", t.orientation_difference_deg));
                        match &t.state {
                            TwinState::Apart => ui.label("two towers"),
                            TwinState::Merged(how) => ui
                                .colored_label(ohl_ui::messages::ok_color(ui), "one shared tower")
                                .on_hover_text(how.join("\n")),
                            TwinState::Refused(why) => ui
                                .colored_label(ui.visuals().error_fg_color, "refused")
                                .on_hover_text(why.join("\n")),
                        };
                        ui.end_row();
                    }
                });
            for t in &p.twins {
                if let TwinState::Refused(why) = &t.state {
                    crate::menu::error_label(
                        ui,
                        format!("{} and {}: refused — {}", t.open, t.other, why.join("; ")),
                    );
                }
            }
        });
    }
    // To look at, notes, left in the file, refused.
    for (title, help, list, colour) in [
        (
            "To look at",
            "What changes for the joined routes or this project's towers: load cases taken by \
             the structure groups, line settings that differ (this project's apply), library \
             entries accepted or left.",
            &p.warnings,
            Some(ui.visuals().warn_fg_color),
        ),
        ("Done too", "What else the join does.", &p.notes, None),
        (
            "Left in the other file",
            "What does not come with the join.",
            &p.left_in_file,
            None,
        ),
        (
            "Refused",
            "Why Join is not possible yet.",
            &p.blocked,
            Some(ui.visuals().error_fg_color),
        ),
    ] {
        if list.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        tool_ui::heading(ui, &format!("{title} ({})", list.len()), help);
        crate::menu::message(ui, list.join("\n"), colour);
    }
}

/// A list in a box of fixed height that scrolls.
fn list_box(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, add);
    });
}

/// Joins: the one command (the library files and the merged line), then
/// the revision "joined routes from FILE".
fn apply(r: &mut Ready) -> Result<String, String> {
    let Some((Ok(plan), _)) = r.join_routes.plan.clone() else {
        return Err("nothing to join".into());
    };
    let command = {
        let e = r
            .model
            .editing
            .as_ref()
            .map_err(|e| format!("read-only: {e}"))?;
        ohl_session::join::command(&e.session, &e.user, &plan).map_err(|e| e.to_string())?
    };
    let what = plan.message();
    if !r.perform(crate::edit::Action::Command {
        what: what.clone(),
        command,
    }) {
        return Err(r
            .status
            .clone()
            .map_or_else(|| "the join was refused".into(), |s| s.0));
    }
    // The line's types, wires… read again with the files written.
    if !plan.library_changes.is_empty() {
        r.model
            .refresh_project_libraries()
            .map_err(|e| format!("joined; the library files not read again: {e}"))?;
    }
    let revision = r
        .model
        .project_op(
            "commit",
            ohl_server::Op::Commit {
                message: what.clone(),
            },
        )
        .map_err(|e| format!("joined (Edit › Undo takes it back); the revision not made: {e}"))?;
    r.refresh_views();
    let merged = plan
        .twins
        .iter()
        .filter(|t| matches!(t.state, TwinState::Merged(_)))
        .count();
    Ok(format!(
        "{what}: {} route(s), {} id(s) renamed, {merged} shared tower(s), {} library file(s) \
         written; {revision}",
        plan.routes.len(),
        plan.renamed.len(),
        plan.library_changes.len()
    ))
}
