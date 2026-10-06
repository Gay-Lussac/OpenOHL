// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Lines menu: the project lines ([`ols_session::lines`]) — the main
//! line and the other designs of the project (e.g. its phases), each
//! edited on its own. Each user works on one (their edits go to it) and
//! shows others next to it; a line brings in the changes of another, and
//! goes back into the main line through a review.

use eframe::egui;
use ols_server::Op;
use ols_session::lines::LineInfo;
use ols_session::versions::{Choices, Conflict, MAIN, Side};

use crate::app::Ready;
use crate::scene::Layer;

/// A window of the Lines menu.
#[derive(Debug, Clone, PartialEq)]
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
    /// Bring in the changes of another line.
    BringIn {
        /// The line whose changes come in.
        from: String,
        /// Conflicts settled.
        choices: Choices,
        /// What it would give, worked out for (from, choices, log entry).
        plan: Option<Box<PlanCache>>,
    },
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
}

/// A bring-in worked out: for which line, choices and log entry; the
/// conflicts and whether the result is valid ([`Session::bring_in_plan`]).
///
/// [`Session::bring_in_plan`]: ols_session::Session::bring_in_plan
pub type PlanCache = (
    String,
    Choices,
    i64,
    Result<(Vec<Conflict>, Option<String>), String>,
);

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
            admin: role == Some(ols_session::Role::Admin),
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

/// The Lines menu (it stays open while lines are ticked).
pub fn menu(ui: &mut egui::Ui, r: &mut Ready) {
    let Some(current) = r.model.session().map(|s| s.current_line().to_owned()) else {
        ui.label(
            egui::RichText::new("Project lines are kept in a project (.ols): open one").small(),
        );
        return;
    };
    let all = lines(r);
    ui.label(egui::RichText::new(format!("Working on: {current}")).strong())
        .on_hover_text("Your edits go to this project line; the others are not changed");
    ui.menu_button("Project lines", |ui| {
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
    });
    ui.separator();
    let rights = Rights::of(r);
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
        r.lines_window = Some(new_window(&all, &current));
        ui.close();
    }
    let others: Vec<&LineInfo> = all.iter().filter(|l| l.name != current).collect();
    if ui
        .add_enabled(
            rights.edit && !others.is_empty(),
            egui::Button::new("Bring in changes from another line…"),
        )
        .on_hover_text(
            "e.g. corrections made on the main line since this phase started: changes to \
             different objects combine by themselves; you choose where both lines changed the \
             same one",
        )
        .clicked()
    {
        let from = others
            .iter()
            .find(|l| l.is_main())
            .unwrap_or(&others[0])
            .name
            .clone();
        r.lines_window = Some(LinesWindow::BringIn {
            from,
            choices: Choices::new(),
            plan: None,
        });
        ui.close();
    }
    if ui
        .add_enabled(
            rights.edit && current != MAIN && all.iter().any(LineInfo::is_main),
            egui::Button::new("Submit this line for review into the main line…"),
        )
        .on_hover_text(
            "When a phase is ready: a reviewer or an admin approves it, then merges it into the \
             main line (Versions › Reviews)",
        )
        .clicked()
    {
        r.lines_window = Some(LinesWindow::Review {
            note: String::new(),
        });
        ui.close();
    }
    ui.separator();
    if others.is_empty() {
        ui.label(egui::RichText::new("No other line yet").small().weak());
        return;
    }
    ui.label(
        egui::RichText::new("Shown next to it (in their colour):")
            .small()
            .weak(),
    );
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

/// The object a conflict is about (`structure:T3` of
/// `structure:T3/position`): one choice settles all its fields.
fn object_key(c: &Conflict) -> &str {
    c.key.split('/').next().unwrap_or(&c.key)
}

/// The conflicts by object: its key, what it is, the fields.
fn by_object(conflicts: &[Conflict]) -> Vec<(String, String, Vec<String>)> {
    let mut out: Vec<(String, String, Vec<String>)> = Vec::new();
    for c in conflicts {
        let key = object_key(c).to_owned();
        let field = c.key.split_once('/').map(|(_, f)| f.replace('/', " "));
        match out.iter_mut().find(|o| o.0 == key) {
            Some(o) => o.2.extend(field),
            None => {
                let what = match &field {
                    Some(f) => c
                        .what
                        .strip_suffix(f.as_str())
                        .unwrap_or(&c.what)
                        .trim()
                        .to_owned(),
                    None => c.what.clone(),
                };
                out.push((key, what, field.into_iter().collect()));
            }
        }
    }
    out
}

/// One object both lines changed: keep which side.
fn conflict_row(
    ui: &mut egui::Ui,
    (key, what, fields): &(String, String, Vec<String>),
    from: &str,
    choices: &mut Choices,
) {
    let label = ui.label(what);
    if !fields.is_empty() {
        label.on_hover_text(format!("changed on both: {}", fields.join(", ")));
    }
    let mut side = choices.get(key).copied();
    ui.radio_value(&mut side, Some(Side::Ours), "keep this line's");
    ui.radio_value(&mut side, Some(Side::Theirs), format!("take {from}'s"));
    if let Some(s) = side {
        choices.insert(key.clone(), s);
    }
    ui.end_row();
}

/// The windows of the Lines menu.
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut w) = r.lines_window.clone() else {
        return;
    };
    let Some(current) = r.model.session().map(|s| s.current_line().to_owned()) else {
        r.lines_window = None;
        return;
    };
    let all = lines(r);
    let rights = Rights::of(r);
    let mut close = false;
    // What to do once the window is drawn.
    let mut op: Option<(String, Op)> = None;
    let mut ops_after: Vec<(String, Op)> = Vec::new();
    let mut switch: Option<String> = None;
    let mut toggle: Option<(String, bool)> = None;
    let mut next: Option<LinesWindow> = None;
    let mut open_tree = false;
    let title = match &w {
        LinesWindow::Manage => "Project lines".to_owned(),
        LinesWindow::New { .. } => "New project line".to_owned(),
        LinesWindow::BringIn { .. } => format!("Bring changes into {current}"),
        LinesWindow::Review { .. } => format!("Submit {current} for review"),
        LinesWindow::Rename { from, .. } => format!("Rename project line {from}"),
        LinesWindow::Delete(n) => format!("Delete project line {n}?"),
    };
    let mut open = true;
    egui::Window::new(title)
        .id(egui::Id::new("project-lines-window"))
        .open(&mut open)
        .collapsible(false)
        .default_width(620.0)
        .show(ctx, |ui| match &mut w {
            LinesWindow::Manage => {
                ui.label(
                    "Project lines are designs of the line kept in this project — the main \
                     line and, for example, the phases of the project. Each is edited on its \
                     own: your edits go to the line you work on, and Undo (⌘Z) undoes your \
                     edits on that line. Others can be shown next to it, in their colour.",
                );
                if r.model.remote.is_some() {
                    ui.label(
                        egui::RichText::new(
                            "On this server every user chooses their own line; nobody else's \
                             changes.",
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
                        for l in &all {
                            let mine = l.name == current;
                            let mut chosen = mine;
                            if ui
                                .radio(chosen, "")
                                .on_hover_text("Work on this line: your edits go to it")
                                .clicked()
                                && !mine
                            {
                                chosen = true;
                                switch = Some(l.name.clone());
                            }
                            let _ = chosen;
                            let mut on =
                                mine || r.model.other_lines.iter().any(|o| o.name == l.name);
                            let tick = ui
                                .add_enabled(!mine, egui::Checkbox::without_text(&mut on))
                                .on_hover_text("Show it next to the line you work on")
                                .on_disabled_hover_text("The line you work on is always shown");
                            if tick.changed() {
                                toggle = Some((l.name.clone(), on));
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
                                    .add_enabled(may.is_ok(), egui::Button::new("Rename…").small())
                                    .on_disabled_hover_text(may.clone().err().unwrap_or_default())
                                    .clicked()
                                {
                                    next = Some(LinesWindow::Rename {
                                        from: l.name.clone(),
                                        to: l.name.clone(),
                                    });
                                }
                                let may_delete = may.and_then(|()| {
                                    if mine {
                                        Err("you work on it: choose another line first".into())
                                    } else if all.len() == 1 {
                                        Err("the project's only line".into())
                                    } else {
                                        Ok(())
                                    }
                                });
                                if ui
                                    .add_enabled(
                                        may_delete.is_ok(),
                                        egui::Button::new("Delete…").small(),
                                    )
                                    .on_disabled_hover_text(may_delete.err().unwrap_or_default())
                                    .clicked()
                                {
                                    next = Some(LinesWindow::Delete(l.name.clone()));
                                }
                            });
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
                        next = Some(new_window(&all, &current));
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
                        next = Some(LinesWindow::BringIn {
                            from,
                            choices: Choices::new(),
                            plan: None,
                        });
                    }
                    if ui
                        .add_enabled(
                            rights.edit && current != MAIN && all.iter().any(LineInfo::is_main),
                            egui::Button::new("Submit for review…"),
                        )
                        .clicked()
                    {
                        next = Some(LinesWindow::Review {
                            note: String::new(),
                        });
                    }
                    if ui.button("Working tree…").clicked() {
                        open_tree = true;
                    }
                });
            }
            LinesWindow::New { name, from, at } => {
                ui.label(
                    "A new project line starts as a copy of another — as it is now, changes \
                     not yet committed included. From then on each is edited on its own: \
                     corrections made on the other line reach the new one only when you bring \
                     them in (Lines › Bring in changes).",
                );
                egui::Grid::new("new-line").num_columns(2).show(ui, |ui| {
                    ui.label("name");
                    ui.text_edit_singleline(name);
                    ui.end_row();
                    ui.label("copy of");
                    // A revision in words; a line by its name.
                    let shown = match at {
                        Some((id, words)) if id == from => words.clone(),
                        _ => from.clone(),
                    };
                    egui::ComboBox::from_id_salt("new-line-from")
                        .selected_text(shown)
                        .width(360.0)
                        .show_ui(ui, |ui| {
                            if let Some((id, words)) = at {
                                ui.selectable_value(from, id.clone(), words.as_str());
                                ui.separator();
                            }
                            for l in &all {
                                ui.selectable_value(
                                    from,
                                    l.name.clone(),
                                    format!("{} as it is now", l.name),
                                );
                            }
                        });
                    ui.end_row();
                });
                ui.label(
                    egui::RichText::new(
                        "The line is yours: you and the admins may rename or delete it. You \
                         keep working on the line you are on; choose the new one in Project \
                         lines.",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        let n = name.trim().to_owned();
                        let source = match at {
                            Some((id, words)) if id == from => words.clone(),
                            _ => from.clone(),
                        };
                        op = Some((
                            format!("project line {n} started from {source}"),
                            Op::CreateLine {
                                name: n,
                                from: from.clone(),
                            },
                        ));
                    }
                    close |= ui.button("Cancel").clicked();
                });
            }
            LinesWindow::BringIn {
                from,
                choices,
                plan: cache,
            } => {
                ui.label(format!(
                    "Brings the changes made on another line into {current}: changes to \
                     different objects combine by themselves; where both lines changed the same \
                     object, choose which to keep. The other line is not changed. Both lines' \
                     uncommitted work is first recorded as a revision of its own."
                ));
                ui.horizontal(|ui| {
                    ui.label("changes of");
                    let before = from.clone();
                    egui::ComboBox::from_id_salt("bring-in-from")
                        .selected_text(from.as_str())
                        .show_ui(ui, |ui| {
                            for l in all.iter().filter(|l| l.name != current) {
                                ui.selectable_value(from, l.name.clone(), &l.name);
                            }
                        });
                    if *from != before {
                        choices.clear();
                    }
                });
                // Worked out again only when something changed.
                let seq = r
                    .model
                    .session()
                    .and_then(|s| s.last_seq().ok())
                    .unwrap_or(0);
                let fresh = cache
                    .as_ref()
                    .is_some_and(|c| c.0 == *from && c.1 == *choices && c.2 == seq);
                if !fresh {
                    let result = r
                        .model
                        .session()
                        .ok_or_else(|| "no project".to_owned())
                        .and_then(|s| s.bring_in_plan(from, choices).map_err(|e| e.to_string()));
                    *cache = Some(Box::new((from.clone(), choices.clone(), seq, result)));
                }
                let plan = cache
                    .as_ref()
                    .map(|c| c.3.clone())
                    .unwrap_or(Err(String::new()));
                let ready = match &plan {
                    Err(e) => {
                        crate::menu::error_label(ui, e.as_str());
                        false
                    }
                    Ok((conflicts, problem)) => {
                        let objects = by_object(conflicts);
                        if objects.is_empty() {
                            ui.label("No conflict: everything combines by itself.");
                        } else {
                            ui.label(format!(
                                "{} object(s) changed on both lines — keep which?",
                                objects.len()
                            ));
                            egui::ScrollArea::vertical()
                                .max_height(crate::menu::TEXT_BOX_HEIGHT)
                                .show(ui, |ui| {
                                    egui::Grid::new("bring-in-conflicts").striped(true).show(
                                        ui,
                                        |ui| {
                                            for o in &objects {
                                                conflict_row(ui, o, from, choices);
                                            }
                                        },
                                    );
                                });
                            ui.horizontal(|ui| {
                                if ui.small_button("All: keep this line's").clicked() {
                                    for o in &objects {
                                        choices.insert(o.0.clone(), Side::Ours);
                                    }
                                }
                                if ui.small_button(format!("All: take {from}'s")).clicked() {
                                    for o in &objects {
                                        choices.insert(o.0.clone(), Side::Theirs);
                                    }
                                }
                            });
                        }
                        if let Some(p) = problem {
                            crate::menu::error_label(
                                ui,
                                format!("The line would not be valid: {p}"),
                            );
                        }
                        conflicts.iter().all(|c| c.chosen.is_some()) && problem.is_none()
                    }
                };
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(ready, egui::Button::new("Bring in"))
                        .on_disabled_hover_text("Choose a side for every conflict first")
                        .clicked()
                    {
                        op = Some((
                            format!("changes of {from} brought into {current}"),
                            Op::BringIn {
                                from: from.clone(),
                                choices: choices.clone(),
                            },
                        ));
                    }
                    close |= ui.button("Cancel").clicked();
                });
            }
            LinesWindow::Review { note } => {
                ui.label(format!(
                    "Asks for {current} to be merged into the main line. Its uncommitted work is \
                     committed first. A reviewer or an admin approves it in Versions › Reviews, \
                     then merges it — the main line changes only then. Keep working on \
                     {current} meanwhile: submit again when it changed."
                ));
                ui.horizontal(|ui| {
                    ui.label("note");
                    ui.add(
                        egui::TextEdit::singleline(note)
                            .hint_text("e.g. phase 1 ready: towers 12 to 20 replaced")
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
                                branch: current.clone(),
                                target: MAIN.to_owned(),
                                reviewers: vec![],
                                note: note.trim().to_owned(),
                            },
                        );
                        if dirty {
                            op = Some((format!("{current} committed"), Op::Commit { message }));
                            ops_after.push(review);
                        } else {
                            op = Some(review);
                        }
                    }
                    close |= ui.button("Cancel").clicked();
                });
            }
            LinesWindow::Rename { from, to } => {
                ui.horizontal(|ui| {
                    ui.label("new name");
                    ui.text_edit_singleline(to);
                });
                ui.label(
                    egui::RichText::new(
                        "Its history, reviews and whoever works on it follow the new name.",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        op = Some((
                            format!("project line {from} renamed {}", to.trim()),
                            Op::RenameLine {
                                from: from.clone(),
                                to: to.trim().to_owned(),
                            },
                        ));
                    }
                    close |= ui.button("Cancel").clicked();
                });
            }
            LinesWindow::Delete(name) => {
                ui.label(format!(
                    "The line {name} and its uncommitted work are deleted (its revisions stay: \
                     Versions › New line from it brings it back). Open reviews of it are closed. \
                     Whoever works on it is moved to another line. Not undone with ⌘Z."
                ));
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        op = Some((
                            format!("project line {name} deleted"),
                            Op::DeleteLine { name: name.clone() },
                        ));
                    }
                    close |= ui.button("Cancel").clicked();
                });
            }
        });
    close |= !open || ctx.input(|i| i.key_pressed(egui::Key::Escape));
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
        let failed = match r.model.project_op(&what, o) {
            Ok(m) => {
                r.status = Some((m, false));
                false
            }
            Err(e) => {
                r.status = Some((e.to_string(), true));
                true
            }
        };
        if !failed {
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
            next = matches!(
                r.lines_window,
                Some(LinesWindow::Delete(_)) | Some(LinesWindow::Rename { .. })
            )
            .then_some(LinesWindow::Manage);
            close = next.is_none();
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
