// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Versions menu: the project lines ([`ohl_session::lines`]) — the main
//! line and the other designs of the project (e.g. its phases), each
//! edited on its own. Each user works on one (their edits go to it) and
//! shows others next to it; a line brings in the changes of another, and
//! goes back into the main line through a review.

use eframe::egui;
use ohl_server::Op;
use ohl_session::lines::LineInfo;
use ohl_session::versions::MAIN;

use crate::app::Ready;
use crate::merge_conflicts::{Settling, Source};
use crate::scene::Layer;

/// A window of the Versions menu.
#[derive(Debug, PartialEq)]
pub enum LinesWindow {
    /// Project lines: every line, which one is worked on, which are shown.
    Manage,
    /// Start a line: its name, the line (or revision) it starts from.
    New {
        /// Name.
        name: String,
        /// The line it starts from, or the revision `at`.
        from: String,
        /// A revision it may start from (from the working tree): its id
        /// and what it is in words (its line, when, its message).
        at: Option<(String, String)>,
    },
    /// Bring in the changes of another line: which, the conflicts settled
    /// so far and what it would give ([`crate::merge_conflicts`]).
    BringIn(Settling),
    /// Submit the line worked on for review into the main line.
    Review {
        /// Note to the reviewers.
        note: String,
    },
    /// Rename a line: from, to.
    Rename {
        /// Its name.
        from: String,
        /// The new one.
        to: String,
    },
    /// Delete a line (asked again).
    Delete(String),
    /// A line's description: the line, the text as edited.
    Description {
        /// The line.
        line: String,
        /// Its description, as edited.
        text: String,
    },
}

/// Height of the box a description is edited or shown in, points: long
/// ones scroll.
pub const DESCRIPTION_HEIGHT: f32 = 160.0;

/// Shows a description in a box of fixed height that scrolls.
pub fn description_box(ui: &mut egui::Ui, id: &str, text: &str, height: f32) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if text.trim().is_empty() {
                    ui.label(egui::RichText::new("No description yet.").weak());
                } else {
                    ui.add(egui::Label::new(text).wrap());
                }
            });
    });
}

impl LinesWindow {
    /// Bring in the changes of line `from`, nothing settled yet.
    pub fn bring_in(from: impl Into<String>) -> Self {
        Self::BringIn(Settling::new(Source::BringIn { from: from.into() }))
    }
}

/// What the user may do with project lines.
struct Rights {
    /// Who they are.
    user: String,
    /// Designers and admins edit.
    edit: bool,
    /// Admins manage every line.
    admin: bool,
}

impl Rights {
    fn of(r: &Ready) -> Self {
        let role = r.model.my_role();
        Self {
            user: r.model.user_name(),
            edit: r.model.can_edit().is_ok(),
            admin: role == Some(ohl_session::Role::Admin),
        }
    }

    /// Whether they may rename or delete `line` (its creator or an admin;
    /// the main line an admin), else why not.
    fn manage(&self, line: &LineInfo) -> Result<(), String> {
        if self.admin {
            return Ok(());
        }
        if !self.edit {
            return Err("read-only".into());
        }
        if line.is_main() {
            return Err("only an admin deletes or renames the main line".into());
        }
        if line.user != self.user {
            return Err(format!(
                "{}'s line: only they or an admin delete or rename it",
                if line.user.is_empty() {
                    "the project"
                } else {
                    &line.user
                }
            ));
        }
        Ok(())
    }
}

fn lines(r: &Ready) -> Vec<LineInfo> {
    r.model
        .session()
        .and_then(|s| s.lines().ok())
        .unwrap_or_default()
}

fn shown_color(r: &Ready, name: &str) -> Option<egui::Color32> {
    r.model
        .other_lines
        .iter()
        .find(|o| o.name == name)
        .map(|o| egui::Color32::from_rgb(o.color[0], o.color[1], o.color[2]))
}

/// Shows or hides line `name` next to the line worked on.
fn show(r: &mut Ready, name: &str, on: bool) {
    if on {
        match r.model.show_line(name) {
            Ok(m) if !m.is_empty() => r.status = Some((m, false)),
            Ok(_) => {}
            Err(e) => r.status = Some((e, true)),
        }
    } else {
        r.model.other_lines.retain(|o| o.name != name);
    }
    r.view3d.dirty.insert(Layer::OtherLines);
}

/// The Versions menu (it stays open while lines are ticked): what is
/// worked on, committing, the lines, the windows of the history, the
/// offline copy, then the lines shown next to the one worked on.
pub fn menu(ui: &mut egui::Ui, r: &mut Ready) {
    let Some(current) = r.model.session().map(|s| s.current_line().to_owned()) else {
        ui.label(
            egui::RichText::new("Project lines are kept in a project (.ohl): open one").small(),
        );
        ui.separator();
        history_entries(ui, r);
        return;
    };
    let all = lines(r);
    status_label(ui, r);
    ui.separator();
    commit_entries(ui, r);
    ui.separator();
    line_entries(ui, r, &all, &current);
    ui.separator();
    history_entries(ui, r);
    ui.separator();
    ui.menu_button("Offline copy", |ui| {
        crate::offline_copy::menu(ui, r, &current);
    })
    .response
    .on_hover_text(
        "A project line carried to another copy of the project and back, as one file (a bundle)",
    );
    ui.separator();
    shown_ticks(ui, r, &all, &current);
}

/// The menu's first line: the line worked on and its uncommitted changes
/// (as last counted, [`crate::line_status`]). A label: nothing opens.
fn status_label(ui: &mut egui::Ui, r: &Ready) {
    ui.label(egui::RichText::new(r.line_status.menu_text()).strong())
        .on_hover_text(format!(
            "{}.\nYour edits go to this project line; the others are not changed.",
            r.line_status.details()
        ));
}

/// Commit, and what a commit would record.
fn commit_entries(ui: &mut egui::Ui, r: &mut Ready) {
    if ui
        .button("Commit…")
        .on_hover_text(
            "Records the line you work on and its library files as a revision, with a message \
             saying what changed and why (the History page of Revisions, tags and reviews)",
        )
        .clicked()
    {
        r.versions.open_commit();
        ui.close();
    }
    if ui
        .button("Changes since the last commit…")
        .on_hover_text(
            "What a commit would record: the line's last revision compared with the line as it \
             is now, object by object and field by field (the Changes page of Revisions, tags \
             and reviews); nothing is changed",
        )
        .clicked()
    {
        r.versions.open_changes();
        ui.close();
    }
}

/// The project lines: their window, a new one, bringing in, submitting,
/// the description of the one worked on.
fn line_entries(ui: &mut egui::Ui, r: &mut Ready, all: &[LineInfo], current: &str) {
    let rights = Rights::of(r);
    if ui
        .button("Project lines…")
        .on_hover_text(
            "Every line of the project: choose the one you work on and those shown next \
             to it; rename, delete",
        )
        .clicked()
    {
        r.lines_window = Some(LinesWindow::Manage);
        ui.close();
    }
    if ui
        .add_enabled(
            rights.edit,
            egui::Button::new(format!("New line from {current}…")),
        )
        .on_hover_text(
            "A copy of this line, to develop on its own (e.g. a phase of the project); you \
             keep working on this one until you choose the new one",
        )
        .clicked()
    {
        r.lines_window = Some(new_window(all, current));
        ui.close();
    }
    let others: Vec<&LineInfo> = all.iter().filter(|l| l.name != current).collect();
    if ui
        .add_enabled(
            rights.edit && !others.is_empty(),
            egui::Button::new("Bring in changes…"),
        )
        .on_hover_text(
            "The changes of another line into this one, e.g. corrections made on the main line \
             since this phase started: changes to different objects combine by themselves; you \
             choose where both lines changed the same one",
        )
        .clicked()
    {
        let from = others
            .iter()
            .find(|l| l.is_main())
            .unwrap_or(&others[0])
            .name
            .clone();
        r.lines_window = Some(LinesWindow::bring_in(from));
        ui.close();
    }
    if ui
        .add_enabled(
            rights.edit && current != MAIN && all.iter().any(LineInfo::is_main),
            egui::Button::new("Submit for review…"),
        )
        .on_hover_text(
            "Asks for this line to be merged into the main line, when a phase is ready: a \
             reviewer or an admin approves it, then merges it (Versions › Revisions, tags and \
             reviews › Reviews)",
        )
        .clicked()
    {
        r.lines_window = Some(LinesWindow::Review {
            note: String::new(),
        });
        ui.close();
    }
    if ui
        .button(format!("Description of {current}…"))
        .on_hover_text("What this line is for: read it, edit it (its creator or an admin)")
        .clicked()
    {
        let text = all
            .iter()
            .find(|l| l.name == current)
            .map(|l| l.description.clone())
            .unwrap_or_default();
        r.lines_window = Some(LinesWindow::Description {
            line: current.to_owned(),
            text,
        });
        ui.close();
    }
}

/// The windows of the history: the lines through time, the revisions,
/// the operation log.
fn history_entries(ui: &mut egui::Ui, r: &mut Ready) {
    if ui
        .button("Working tree…")
        .on_hover_text(
            "The lines through time, left to right: when each started, which exist, where \
             they were merged; admins roll a line back",
        )
        .clicked()
    {
        r.working_tree.open = true;
        ui.close();
    }
    if ui
        .button(format!("{}…", crate::versions::TITLE))
        .on_hover_text(
            "The revisions of every line, the lines and tags, what changed between two \
             revisions, merging, reviews and edit rights",
        )
        .clicked()
    {
        r.versions.open = true;
        ui.close();
    }
    if ui
        .button("Operation log…")
        .on_hover_text("Every edit, one entry each: who did what and when; admins reverse entries")
        .clicked()
    {
        r.history_open = true;
        ui.close();
    }
}

/// The lines shown next to the one worked on: a tick each (the menu
/// stays open).
fn shown_ticks(ui: &mut egui::Ui, r: &mut Ready, all: &[LineInfo], current: &str) {
    let others: Vec<&LineInfo> = all.iter().filter(|l| l.name != current).collect();
    if others.is_empty() {
        ui.label(egui::RichText::new("No other line yet").small().weak());
        return;
    }
    ui.label(
        egui::RichText::new("Shown next to it (in their colour):")
            .small()
            .weak(),
    );
    if ui
        .checkbox(
            &mut r.model.other_lines_whole,
            "whole lines (else only what differs)",
        )
        .on_hover_text(
            "Draw the project lines shown whole; by default only their sections, structures \
             and jumpers that differ from the line worked on",
        )
        .changed()
    {
        r.view3d.dirty.insert(crate::scene::Layer::OtherLines);
    }
    let mut toggle = None;
    for l in others {
        let mut on = r.model.other_lines.iter().any(|o| o.name == l.name);
        let mut text = egui::RichText::new(&l.name);
        if let Some(c) = shown_color(r, &l.name) {
            text = text.color(c);
        }
        if ui
            .checkbox(&mut on, text)
            .on_hover_text(describe(l))
            .changed()
        {
            toggle = Some((l.name.clone(), on));
        }
    }
    if let Some((name, on)) = toggle {
        show(r, &name, on);
    }
}

/// A line in words: who made it, from what.
fn describe(l: &LineInfo) -> String {
    let mut out = if l.is_main() {
        "the main line".to_owned()
    } else {
        format!(
            "made by {} on {}",
            if l.user.is_empty() { "?" } else { &l.user },
            l.time.get(..10).unwrap_or(&l.time)
        )
    };
    if !l.note.is_empty() {
        out.push_str(&format!(" — {}", l.note));
    }
    out.push_str(&format!("\n{} objects", l.objects));
    if l.protected {
        out.push_str("; protected: changed by admins or approved reviews");
    }
    out
}

fn new_window(all: &[LineInfo], from: &str) -> LinesWindow {
    let name = crate::editors::next_id(all.iter().map(|l| l.name.as_str()), "phase 0");
    LinesWindow::New {
        name,
        from: from.to_owned(),
        at: None,
    }
}

/// What the user asked for in a window of the Versions menu: done once
/// the window is drawn.
#[derive(Default)]
struct Asked {
    /// A project operation, and what it is in words.
    op: Option<(String, Op)>,
    /// Operations after it, when it was made.
    ops_after: Vec<(String, Op)>,
    /// The line to work on.
    switch: Option<String>,
    /// A line to show or hide next to the one worked on.
    toggle: Option<(String, bool)>,
    /// The window shown next.
    next: Option<LinesWindow>,
    /// The working tree is opened.
    open_tree: bool,
    /// Show of a conflict: what to select and bring the views to.
    show: Option<crate::selection::Selection>,
    /// The window closes.
    close: bool,
}

/// Bring in changes: the line they come from, then what
/// [`crate::merge_conflicts::panel`] shows for it, and Bring in. Closing
/// with choices made is asked again.
fn bring_in_ui(
    ui: &mut egui::Ui,
    r: &Ready,
    all: &[LineInfo],
    current: &str,
    st: &mut Settling,
    asked: &mut Asked,
) {
    ui.label(format!(
        "Brings the changes made on another line into {current}: changes to different objects \
         combine by themselves; where both lines changed the same object, choose which to keep. \
         The other line is not changed. Both lines' uncommitted work is first recorded as a \
         revision of its own."
    ));
    ui.horizontal(|ui| {
        ui.label("changes of");
        let Source::BringIn { from } = &st.source else {
            return;
        };
        let mut chosen = from.clone();
        egui::ComboBox::from_id_salt("bring-in-from")
            .selected_text(chosen.as_str())
            .show_ui(ui, |ui| {
                for l in all.iter().filter(|l| l.name != current) {
                    ui.selectable_value(&mut chosen, l.name.clone(), &l.name)
                        .on_hover_text(describe(l));
                }
            })
            .response
            .on_hover_text(
                "The project line whose changes come in. Choosing another drops what was \
                 settled for this one",
            );
        // Another line: the choices made for the one before are not its.
        st.set_source(Source::BringIn { from: chosen });
    });
    ui.separator();
    let mut resp = crate::merge_conflicts::panel(ui, r, st);
    asked.show = resp.show.take();
    if st.asking {
        asked.close = crate::merge_conflicts::close_question(ui, st);
        return;
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(resp.is_ready(), egui::Button::new(st.source.apply_label()))
            .on_hover_text(st.source.apply_hover())
            .on_disabled_hover_text(resp.why_not())
            .clicked()
        {
            asked.op = Some(st.source.op(current, &st.choices, None));
        }
        if ui
            .button("Cancel")
            .on_hover_text("Nothing is changed")
            .clicked()
        {
            asked.close = st.close();
        }
    });
}

/// Project lines: every line, the one worked on, those shown; what is
/// done with them.
fn manage_ui(
    ui: &mut egui::Ui,
    r: &Ready,
    all: &[LineInfo],
    current: &str,
    rights: &Rights,
    asked: &mut Asked,
) {
    ui.label(
        "Project lines are designs of the line kept in this project — the main line and, for \
         example, the phases of the project. Each is edited on its own: your edits go to the \
         line you work on, and Undo (⌘Z) undoes your edits on that line. Others can be shown \
         next to it, in their colour.",
    );
    if r.model.remote.is_some() {
        ui.label(
            egui::RichText::new(
                "On this server every user chooses their own line; nobody else's changes.",
            )
            .small()
            .weak(),
        );
    }
    ui.separator();
    egui::Grid::new("project-lines")
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for h in ["work on", "show", "line", "made by", "objects", ""] {
                ui.label(egui::RichText::new(h).small().strong());
            }
            ui.end_row();
            for l in all {
                manage_row(ui, r, l, current, all.len(), rights, asked);
                ui.end_row();
            }
        });
    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                rights.edit,
                egui::Button::new(format!("New line from {current}…")),
            )
            .clicked()
        {
            asked.next = Some(new_window(all, current));
        }
        if ui
            .add_enabled(
                rights.edit && all.len() > 1,
                egui::Button::new("Bring in changes…"),
            )
            .clicked()
        {
            let from = all
                .iter()
                .find(|l| l.name != current && l.is_main())
                .or_else(|| all.iter().find(|l| l.name != current))
                .map(|l| l.name.clone())
                .unwrap_or_default();
            asked.next = Some(LinesWindow::bring_in(from));
        }
        if ui
            .add_enabled(
                rights.edit && current != MAIN && all.iter().any(LineInfo::is_main),
                egui::Button::new("Submit for review…"),
            )
            .clicked()
        {
            asked.next = Some(LinesWindow::Review {
                note: String::new(),
            });
        }
        if ui.button("Working tree…").clicked() {
            asked.open_tree = true;
        }
    });
}

/// One line of the Project lines window (`count`: how many lines the
/// project has).
fn manage_row(
    ui: &mut egui::Ui,
    r: &Ready,
    l: &LineInfo,
    current: &str,
    count: usize,
    rights: &Rights,
    asked: &mut Asked,
) {
    let mine = l.name == current;
    if ui
        .radio(mine, "")
        .on_hover_text("Work on this line: your edits go to it")
        .clicked()
        && !mine
    {
        asked.switch = Some(l.name.clone());
    }
    let mut on = mine || r.model.other_lines.iter().any(|o| o.name == l.name);
    let tick = ui
        .add_enabled(!mine, egui::Checkbox::without_text(&mut on))
        .on_hover_text("Show it next to the line you work on")
        .on_disabled_hover_text("The line you work on is always shown");
    if tick.changed() {
        asked.toggle = Some((l.name.clone(), on));
    }
    let mut name = egui::RichText::new(&l.name);
    if mine {
        name = name.strong();
    } else if let Some(c) = shown_color(r, &l.name) {
        name = name.color(c);
    }
    ui.label(name).on_hover_text(describe(l));
    ui.label(if l.is_main() {
        "main line".to_owned()
    } else if l.user.is_empty() {
        "—".to_owned()
    } else {
        l.user.clone()
    });
    ui.label(l.objects.to_string());
    ui.horizontal(|ui| {
        let may = rights.manage(l);
        if ui
            .add(egui::Button::new("Description…").small())
            .on_hover_text(if l.description.trim().is_empty() {
                "No description yet".to_owned()
            } else {
                l.description.chars().take(300).collect::<String>()
            })
            .clicked()
        {
            asked.next = Some(LinesWindow::Description {
                line: l.name.clone(),
                text: l.description.clone(),
            });
        }
        if ui
            .add_enabled(may.is_ok(), egui::Button::new("Rename…").small())
            .on_disabled_hover_text(may.clone().err().unwrap_or_default())
            .clicked()
        {
            asked.next = Some(LinesWindow::Rename {
                from: l.name.clone(),
                to: l.name.clone(),
            });
        }
        let may_delete = may.and_then(|()| {
            if mine {
                Err("you work on it: choose another line first".into())
            } else if count == 1 {
                Err("the project's only line".into())
            } else {
                Ok(())
            }
        });
        if ui
            .add_enabled(may_delete.is_ok(), egui::Button::new("Delete…").small())
            .on_disabled_hover_text(may_delete.err().unwrap_or_default())
            .clicked()
        {
            asked.next = Some(LinesWindow::Delete(l.name.clone()));
        }
    });
}

/// New project line: its name, what it is a copy of.
fn new_ui(
    ui: &mut egui::Ui,
    all: &[LineInfo],
    name: &mut String,
    from: &mut String,
    at: &Option<(String, String)>,
    asked: &mut Asked,
) {
    ui.label(
        "A new project line starts as a copy of another — as it is now, changes not yet \
         committed included. From then on each is edited on its own: corrections made on the \
         other line reach the new one only when you bring them in (Versions › Bring in changes).",
    );
    // A revision in words; a line by its name.
    let shown = |from: &String| match at {
        Some((id, words)) if id == from => words.clone(),
        _ => from.clone(),
    };
    egui::Grid::new("new-line").num_columns(2).show(ui, |ui| {
        ui.label("name");
        ui.text_edit_singleline(name);
        ui.end_row();
        ui.label("copy of");
        egui::ComboBox::from_id_salt("new-line-from")
            .selected_text(shown(from))
            .width(360.0)
            .show_ui(ui, |ui| {
                if let Some((id, words)) = at {
                    ui.selectable_value(from, id.clone(), words.as_str());
                    ui.separator();
                }
                for l in all {
                    ui.selectable_value(from, l.name.clone(), format!("{} as it is now", l.name));
                }
            });
        ui.end_row();
    });
    ui.label(
        egui::RichText::new(
            "The line is yours: you and the admins may rename or delete it. You keep working \
             on the line you are on; choose the new one in Project lines.",
        )
        .small()
        .weak(),
    );
    ui.horizontal(|ui| {
        if ui.button("Create").clicked() {
            let n = name.trim().to_owned();
            asked.op = Some((
                format!("project line {n} started from {}", shown(from)),
                Op::CreateLine {
                    name: n,
                    from: from.clone(),
                },
            ));
        }
        asked.close |= ui.button("Cancel").clicked();
    });
}

/// Submit the line worked on for review into the main line.
fn review_ui(ui: &mut egui::Ui, r: &Ready, current: &str, note: &mut String, asked: &mut Asked) {
    ui.label(format!(
        "Asks for {current} to be merged into the main line. Its uncommitted work is committed \
         first. A reviewer or an admin approves it in Versions › Revisions, tags and reviews › \
         Reviews, then merges it — the main line changes only then. Keep working on {current} \
         meanwhile: submit again when it changed."
    ));
    ui.horizontal(|ui| {
        ui.label("note");
        ui.add(
            egui::TextEdit::singleline(note)
                .hint_text("e.g. stage 1 ready: structures 12 to 20 replaced")
                .desired_width(360.0),
        );
    });
    ui.horizontal(|ui| {
        if ui.button("Submit").clicked() {
            let dirty = r
                .model
                .session()
                .and_then(|s| s.status().ok())
                .is_some_and(|st| !st.changes.is_empty());
            let message = if note.trim().is_empty() {
                format!("{current} submitted for review")
            } else {
                note.trim().to_owned()
            };
            let review = (
                format!("{current} submitted for review into the main line"),
                Op::SubmitReview {
                    branch: current.to_owned(),
                    target: MAIN.to_owned(),
                    reviewers: vec![],
                    note: note.trim().to_owned(),
                },
            );
            if dirty {
                asked.op = Some((format!("{current} committed"), Op::Commit { message }));
                asked.ops_after.push(review);
            } else {
                asked.op = Some(review);
            }
        }
        asked.close |= ui.button("Cancel").clicked();
    });
}

/// Rename a line.
fn rename_ui(ui: &mut egui::Ui, from: &str, to: &mut String, asked: &mut Asked) {
    ui.horizontal(|ui| {
        ui.label("new name");
        ui.text_edit_singleline(to);
    });
    ui.label(
        egui::RichText::new("Its history, reviews and whoever works on it follow the new name.")
            .small()
            .weak(),
    );
    ui.horizontal(|ui| {
        if ui.button("Rename").clicked() {
            asked.op = Some((
                format!("project line {from} renamed {}", to.trim()),
                Op::RenameLine {
                    from: from.to_owned(),
                    to: to.trim().to_owned(),
                },
            ));
        }
        asked.close |= ui.button("Cancel").clicked();
    });
}

/// A line's description: edited by its creator or an admin, read by the
/// others.
fn description_ui(
    ui: &mut egui::Ui,
    all: &[LineInfo],
    rights: &Rights,
    line: &str,
    text: &mut String,
    asked: &mut Asked,
) {
    let info = all.iter().find(|l| l.name == line);
    let may = info.map_or(Err("no such line".into()), |l| rights.manage(l));
    ui.label(
        egui::RichText::new(
            "What this line is for: its scope, assumptions, status… (kept in the project, shown \
             under the working tree).",
        )
        .small()
        .weak(),
    );
    if let Err(why) = may {
        description_box(ui, "description-read", text, DESCRIPTION_HEIGHT);
        ui.label(
            egui::RichText::new(format!("Read only: {why}"))
                .small()
                .weak(),
        );
        asked.close |= ui.button("Close").clicked();
        return;
    }
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("description-edit")
            .max_height(DESCRIPTION_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .char_limit(ohl_session::lines::MAX_DESCRIPTION),
                );
            });
    });
    ui.horizontal(|ui| {
        if ui.button("Save").clicked() {
            asked.op = Some((
                format!("description of {line} saved"),
                Op::SetLineDescription {
                    line: line.to_owned(),
                    description: text.clone(),
                },
            ));
        }
        asked.close |= ui.button("Cancel").clicked();
    });
}

/// Delete a line, asked again.
fn delete_ui(ui: &mut egui::Ui, name: &str, asked: &mut Asked) {
    ui.label(format!(
        "The line {name} and its uncommitted work are deleted (its revisions stay: Versions › \
         Revisions, tags and reviews › New line from it… brings it back). Open reviews of it \
         are closed. Whoever works on it is moved to another line. Not undone with ⌘Z."
    ));
    ui.horizontal(|ui| {
        if ui.button("Delete").clicked() {
            asked.op = Some((
                format!("project line {name} deleted"),
                Op::DeleteLine {
                    name: name.to_owned(),
                },
            ));
        }
        asked.close |= ui.button("Cancel").clicked();
    });
}

/// The windows of the Versions menu.
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    // (Taken, drawn and put back: nothing of it is copied.)
    let Some(mut w) = r.lines_window.take() else {
        return;
    };
    let Some(current) = r.model.session().map(|s| s.current_line().to_owned()) else {
        return;
    };
    let all = lines(r);
    let rights = Rights::of(r);
    let mut asked = Asked::default();
    let title = match &w {
        LinesWindow::Manage => "Project lines".to_owned(),
        LinesWindow::New { .. } => "New project line".to_owned(),
        LinesWindow::BringIn(_) => format!("Bring changes into {current}"),
        LinesWindow::Review { .. } => format!("Submit {current} for review"),
        LinesWindow::Rename { from, .. } => format!("Rename project line {from}"),
        LinesWindow::Delete(n) => format!("Delete project line {n}?"),
        LinesWindow::Description { line, .. } => format!("Description of {line}"),
    };
    let mut open = true;
    egui::Window::new(title)
        .id(egui::Id::new("project-lines-window"))
        .open(&mut open)
        .collapsible(false)
        .default_width(620.0)
        .show(ctx, |ui| {
            let asked = &mut asked;
            match &mut w {
                LinesWindow::Manage => manage_ui(ui, r, &all, &current, &rights, asked),
                LinesWindow::New { name, from, at } => new_ui(ui, &all, name, from, at, asked),
                LinesWindow::BringIn(st) => bring_in_ui(ui, r, &all, &current, st, asked),
                LinesWindow::Review { note } => review_ui(ui, r, &current, note, asked),
                LinesWindow::Rename { from, to } => rename_ui(ui, from, to, asked),
                LinesWindow::Description { line, text } => {
                    description_ui(ui, &all, &rights, line, text, asked)
                }
                LinesWindow::Delete(name) => delete_ui(ui, name, asked),
            }
        });
    // Closed with its cross or Esc: a Bring in with choices made asks
    // first.
    if !asked.close && (!open || ctx.input(|i| i.key_pressed(egui::Key::Escape))) {
        asked.close = match &mut w {
            LinesWindow::BringIn(st) => st.close(),
            _ => true,
        };
    }
    finish(r, w, asked);
}

/// Does what a window of the Versions menu asked for, and keeps, changes
/// or closes the window.
fn finish(r: &mut Ready, mut w: LinesWindow, asked: Asked) {
    let Asked {
        op,
        ops_after,
        switch,
        toggle,
        mut next,
        open_tree,
        show: target,
        mut close,
    } = asked;
    if let Some(target) = target {
        r.go_to(target);
    }
    if let Some((name, on)) = toggle {
        show(r, &name, on);
    }
    if let Some(name) = switch {
        r.switch_line(&name);
    }
    if open_tree {
        r.working_tree.open = true;
    }
    if let Some((what, o)) = op {
        let renamed = match &o {
            Op::RenameLine { from, to } => Some((from.clone(), to.clone())),
            _ => None,
        };
        let deleted = match &o {
            Op::DeleteLine { name } => Some(name.clone()),
            _ => None,
        };
        match r.model.project_op(&what, o) {
            Ok(m) => {
                r.status = Some((m, false));
                for (what, o) in ops_after {
                    if let Err(e) = r.model.project_op(&what, o) {
                        r.status = Some((e.to_string(), true));
                        break;
                    }
                    r.status = Some((what, false));
                }
                if let Some((from, to)) = renamed {
                    for o in &mut r.model.other_lines {
                        if o.name == from {
                            o.name = to.clone();
                        }
                    }
                }
                if let Some(name) = deleted {
                    r.model.other_lines.retain(|o| o.name != name);
                }
                r.view3d.dirty.insert(Layer::OtherLines);
                r.clearances.stale = true;
                r.refresh_views();
                // Back to the list, or closed.
                next = matches!(w, LinesWindow::Delete(_) | LinesWindow::Rename { .. })
                    .then_some(LinesWindow::Manage);
                close = next.is_none();
            }
            Err(e) => {
                // Said in the window too; what was settled is kept.
                if let LinesWindow::BringIn(st) = &mut w {
                    st.refused(e.to_string());
                }
                r.status = Some((e.to_string(), true));
            }
        }
    }
    r.lines_window = if let Some(n) = next {
        Some(n)
    } else if close {
        None
    } else {
        Some(w)
    };
}
