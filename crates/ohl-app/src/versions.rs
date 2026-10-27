// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Versions › Revisions and branches: the project's version control and collaboration
//! (`ohl_session::versions`, `ohl_session::collab`).
//!
//! - **History**: the revisions, newest first, with their branches and
//!   tags; commit the working state with a message; make, switch, protect
//!   and delete branches (project lines; work on one); tag, start a line
//!   from or revert a revision.
//! - **Changes**: what differs between two revisions (or a revision and
//!   the working state), field by field, and its engineering impact in the
//!   weather case shown.
//! - **Merge**: a branch into the current one; conflicts listed with a
//!   choice of side for each.
//! - **Reviews**: submit a branch, approve or reject, merge approved ones.
//! - **Rights** (admins): who may edit what.
//!
//! Every change is a project operation (`ohl_server::Op`): through the
//! server when joined, so the replicas follow.

use eframe::egui;
use ohl_server::Op;
use ohl_session::Role;
use ohl_session::collab::{GRANT_KINDS, Grant, GrantEntry, Review, ReviewState};
use ohl_session::impact::{Impact, ImpactOptions};
use ohl_session::versions::{
    Choices, Diff, Head, MergeKind, MergePlan, Ref, RefDeletion, RefKind, Revision, Side, State,
    Status, diff, short,
};

use crate::app::Ready;
use crate::model::Model;

/// The pages of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Page {
    #[default]
    History,
    Changes,
    Merge,
    Reviews,
    Rights,
}

/// What the window shows, read again when the project changes.
struct Snapshot {
    key: (u64, i64, u64),
    status: Result<Status, String>,
    revisions: Vec<Revision>,
    refs: Vec<Ref>,
    reviews: Vec<Review>,
    grants: Vec<GrantEntry>,
    users: Vec<String>,
}

/// The Versions window.
#[derive(Default)]
pub struct VersionsWindow {
    /// Shown.
    pub open: bool,
    page: Page,
    serial: u64,
    /// A project line to work on (asked from the branch list).
    switch_to: Option<String>,
    snapshot: Option<Snapshot>,
    message: String,
    new_branch: String,
    new_tag: String,
    tag_note: String,
    tag_at: Option<String>,
    deleting: Option<Result<RefDeletion, String>>,
    from: String,
    to: String,
    changes: Option<Result<(String, Diff, State, State), String>>,
    impact: Option<Impact>,
    other: String,
    plan: Option<Result<MergePlan, String>>,
    choices: Choices,
    merge_message: String,
    review_branch: String,
    review_target: String,
    reviewers: String,
    review_note: String,
    comment: String,
    grant: Grant,
    grant_objects: String,
    grant_area: String,
}

fn session(model: &Model) -> Option<&ohl_session::Session> {
    match (&model.editing, &model.viewer_session) {
        (Ok(e), _) => Some(&e.session),
        (Err(_), Some(s)) => Some(s),
        _ => None,
    }
}

fn weak(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).small().weak());
}

fn error(ui: &mut egui::Ui, text: &str) {
    crate::menu::error_label(ui, text);
}

fn value(v: &Option<serde_json::Value>) -> String {
    match v {
        None => "(none)".into(),
        Some(v) => {
            let t = v.to_string();
            if t.chars().count() > 60 {
                format!("{}…", t.chars().take(60).collect::<String>())
            } else {
                t
            }
        }
    }
}

fn read(model: &Model, key: (u64, i64, u64)) -> Option<Snapshot> {
    let s = session(model)?;
    let e = |e: ohl_session::SessionError| e.to_string();
    Some(Snapshot {
        key,
        status: s.status().map_err(e),
        revisions: s.revisions().unwrap_or_default(),
        refs: s.refs().unwrap_or_default(),
        reviews: s.reviews().unwrap_or_default(),
        grants: s.grants().unwrap_or_default(),
        users: s
            .users()
            .unwrap_or_default()
            .into_iter()
            .map(|(n, _)| n)
            .collect(),
    })
}

/// Versions › Revisions and branches, when open.
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    if !r.versions.open {
        return;
    }
    let mut w = std::mem::take(&mut r.versions);
    let mut open = true;
    let mut ops: Vec<(String, Op)> = Vec::new();
    egui::Window::new("Versions")
        .open(&mut open)
        .default_width(820.0)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = crate::check_help::ITEM_SPACING;
            w.ui(ui, r, &mut ops)
        });
    w.open = open;
    if let Some(name) = w.switch_to.take() {
        r.switch_line(&name);
    }
    if !ops.is_empty() {
        // Library files changed by a checkout or merge: the project is
        // opened again to read them (local projects).
        let libraries = |m: &Model| {
            session(m)
                .and_then(|s| s.working_state().ok())
                .map(|s| s.libraries)
        };
        let before = libraries(&r.model);
        for (what, op) in ops {
            r.project_op(&what, op);
        }
        if r.model.remote.is_none() && libraries(&r.model) != before {
            r.reopen = true;
        }
        w.serial += 1;
        w.plan = None;
        w.changes = None;
        w.impact = None;
        w.deleting = None;
    }
    r.versions = w;
}

impl VersionsWindow {
    fn ui(&mut self, ui: &mut egui::Ui, r: &Ready, ops: &mut Vec<(String, Op)>) {
        let Some(s) = session(&r.model) else {
            ui.label("Version control needs a project file (.ohl).");
            return;
        };
        let key = (r.model.line_version, s.last_seq().unwrap_or(0), self.serial);
        if self.snapshot.as_ref().is_none_or(|x| x.key != key) {
            self.snapshot = read(&r.model, key);
        }
        let Some(snap) = self.snapshot.take() else {
            return;
        };
        let can_edit = r.model.can_edit().is_ok();
        let admin = r.model.my_role() == Some(Role::Admin) && can_edit;
        ui.horizontal(|ui| {
            match &snap.status {
                Ok(st) => {
                    let head = match &st.head {
                        Head::Branch(b) => format!(
                            "On branch {b}{}",
                            st.revision
                                .as_deref()
                                .map(|r| format!(" at {}", short(r)))
                                .unwrap_or_else(|| " (no revision yet)".into())
                        ),
                        Head::Detached(id) => {
                            format!("Revision {} checked out (no commits here)", short(id))
                        }
                    };
                    ui.strong(head);
                    if st.changes.is_empty() {
                        ui.label("— nothing to commit");
                    } else {
                        ui.label(format!("— uncommitted: {}", st.changes.summary()));
                    }
                }
                Err(e) => error(ui, e),
            }
            if ui.small_button("Refresh").clicked() {
                self.serial += 1;
            }
        });
        ui.horizontal(|ui| {
            for (p, name) in [
                (Page::History, "History"),
                (Page::Changes, "Changes"),
                (Page::Merge, "Merge"),
                (Page::Reviews, "Reviews"),
                (Page::Rights, "Rights"),
            ] {
                ui.selectable_value(&mut self.page, p, name);
            }
        });
        ui.separator();
        match self.page {
            Page::History => self.history(ui, s, &snap, can_edit, admin, ops),
            Page::Changes => self.changes_page(ui, r, &snap),
            Page::Merge => self.merge_page(ui, &r.model, &snap, can_edit, ops),
            Page::Reviews => self.reviews_page(ui, &r.model, &snap, can_edit, ops),
            Page::Rights => self.rights_page(ui, &snap, admin, ops),
        }
        self.snapshot = Some(snap);
    }

    fn history(
        &mut self,
        ui: &mut egui::Ui,
        s: &ohl_session::Session,
        snap: &Snapshot,
        can_edit: bool,
        admin: bool,
        ops: &mut Vec<(String, Op)>,
    ) {
        let (head, head_rev) = match &snap.status {
            Ok(st) => (Some(st.head.clone()), st.revision.clone()),
            Err(_) => (None, None),
        };
        ui.add_enabled_ui(can_edit, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.message)
                        .hint_text("Why: raised T12 by 3 m for the road crossing")
                        .desired_width(420.0),
                );
                if ui
                    .add_enabled(!self.message.trim().is_empty(), egui::Button::new("Commit"))
                    .on_hover_text("Record the line and its libraries as a revision of this branch")
                    .clicked()
                {
                    let message = std::mem::take(&mut self.message);
                    ops.push((format!("committed \"{message}\""), Op::Commit { message }));
                }
            });
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Branches and tags").strong());
        egui::Grid::new("version-refs")
            .striped(true)
            .spacing([10.0, 3.0])
            .show(ui, |ui| {
                for x in &snap.refs {
                    let current = head == Some(Head::Branch(x.name.clone()));
                    let kind = match x.kind {
                        RefKind::Branch => "branch",
                        RefKind::Tag => "tag",
                    };
                    let name = if current {
                        egui::RichText::new(format!("{} (current)", x.name)).strong()
                    } else {
                        egui::RichText::new(&x.name)
                    };
                    ui.label(kind);
                    ui.label(name);
                    ui.label(x.revision.as_deref().map(short).unwrap_or("no revision"));
                    ui.label(if x.protected { "protected" } else { "" });
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(can_edit, |ui| {
                            if !current
                                && x.kind == RefKind::Branch
                                && ui
                                    .small_button("Work on it")
                                    .on_hover_text(
                                        "Each branch is a project line: work on this one from \
                                         now on (nothing changes; also Versions › Project lines)",
                                    )
                                    .clicked()
                            {
                                self.switch_to = Some(x.name.clone());
                            }
                            if x.kind == RefKind::Branch && admin {
                                let mut p = x.protected;
                                if ui
                                    .checkbox(&mut p, "protect")
                                    .on_hover_text("Changed only by admins or approved reviews")
                                    .changed()
                                {
                                    ops.push((
                                        format!("branch {} protection", x.name),
                                        Op::ProtectBranch {
                                            name: x.name.clone(),
                                            protected: p,
                                        },
                                    ));
                                }
                            }
                            if !current && ui.small_button("Delete…").clicked() {
                                self.deleting =
                                    Some(s.ref_deletion(&x.name).map_err(|e| e.to_string()));
                            }
                        });
                    });
                    ui.label(&x.note);
                    ui.end_row();
                }
            });
        if let Some(d) = &self.deleting {
            let mut close = false;
            ui.group(|ui| match d {
                Ok(d) => {
                    ui.label(format!("{d}."));
                    weak(
                        ui,
                        "The revisions stay in the project: a branch or tag made at the revision listed brings it back.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            ops.push((
                                format!("{} deleted", d.name),
                                Op::DeleteRef {
                                    name: d.name.clone(),
                                    confirm: true,
                                },
                            ));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Err(e) => {
                    error(ui, e);
                    close = ui.button("Close").clicked();
                }
            });
            if close {
                self.deleting = None;
            }
        }
        ui.add_enabled_ui(can_edit && head_rev.is_some(), |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_branch)
                        .hint_text("new branch, e.g. alternative B")
                        .desired_width(220.0),
                );
                if ui
                    .add_enabled(
                        !self.new_branch.trim().is_empty(),
                        egui::Button::new("Make branch"),
                    )
                    .on_hover_text("At the current revision; check it out to work on it")
                    .clicked()
                {
                    let name = std::mem::take(&mut self.new_branch);
                    ops.push((
                        format!("branch {name} made"),
                        Op::CreateBranch { name, at: None },
                    ));
                }
            });
        });
        if let Some(at) = self.tag_at.clone() {
            ui.group(|ui| {
                ui.label(format!("Tag revision {}", short(&at)));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_tag)
                            .hint_text("IFC rev B")
                            .desired_width(160.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.tag_note)
                            .hint_text("note: issued for construction")
                            .desired_width(240.0),
                    );
                    if ui
                        .add_enabled(!self.new_tag.trim().is_empty(), egui::Button::new("Tag"))
                        .clicked()
                    {
                        let name = std::mem::take(&mut self.new_tag);
                        ops.push((
                            format!("tag {name} made"),
                            Op::Tag {
                                name,
                                at: Some(at.clone()),
                                note: std::mem::take(&mut self.tag_note),
                            },
                        ));
                        self.tag_at = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.tag_at = None;
                    }
                });
            });
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("Revisions ({})", snap.revisions.len())).strong());
        if snap.revisions.is_empty() {
            weak(ui, "No revision yet: commit the working state.");
        }
        crate::check_help::list(
            ui,
            "versions-revisions",
            360.0,
            egui::ScrollArea::vertical().id_salt("revisions"),
            |ui| {
                egui::Grid::new("version-revisions")
                    .striped(true)
                    .spacing(crate::check_help::GRID_SPACING)
                    .show(ui, |ui| {
                        for rev in snap.revisions.iter().take(500) {
                            let labels: Vec<String> = snap
                                .refs
                                .iter()
                                .filter(|x| x.revision.as_deref() == Some(rev.id.as_str()))
                                .map(|x| match x.kind {
                                    RefKind::Branch => x.name.clone(),
                                    RefKind::Tag => format!("tag: {}", x.name),
                                })
                                .collect();
                            let mark = if rev.parents.len() > 1 { "◎" } else { "○" };
                            let here = head_rev.as_deref() == Some(rev.id.as_str());
                            ui.label(if here {
                                egui::RichText::new(mark).strong()
                            } else {
                                egui::RichText::new(mark).weak()
                            })
                            .on_hover_text(
                                if rev.parents.is_empty() {
                                    "first revision".to_owned()
                                } else {
                                    format!(
                                        "parents: {}",
                                        rev.parents
                                            .iter()
                                            .map(|p| short(p))
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    )
                                },
                            );
                            ui.monospace(rev.short());
                            ui.label(labels.join(", "));
                            ui.label(rev.time.get(..16).unwrap_or(&rev.time).replace('T', " "));
                            ui.label(&rev.author);
                            ui.label(&rev.message);
                            ui.horizontal(|ui| {
                                if ui
                                    .small_button("Diff")
                                    .on_hover_text("What it changed (against its first parent)")
                                    .clicked()
                                {
                                    self.from = rev.parents.first().cloned().unwrap_or_default();
                                    self.to = rev.id.clone();
                                    self.page = Page::Changes;
                                    self.changes = None;
                                    self.impact = None;
                                }
                                ui.add_enabled_ui(can_edit, |ui| {
                                    if ui.small_button("Tag…").clicked() {
                                        self.tag_at = Some(rev.id.clone());
                                    }
                                    if !here
                                        && ui
                                            .small_button("New line from it")
                                            .on_hover_text(
                                                "A project line starting from this revision, to \
                                                 work on (Versions › Project lines)",
                                            )
                                            .clicked()
                                    {
                                        let name = format!("from {}", rev.short());
                                        ops.push((
                                            format!("project line {name} started"),
                                            Op::CreateLine {
                                                name,
                                                from: rev.id.clone(),
                                            },
                                        ));
                                    }
                                    if !rev.parents.is_empty()
                                        && ui
                                            .small_button("Revert")
                                            .on_hover_text("A new revision undoing its changes")
                                            .clicked()
                                    {
                                        ops.push((
                                            format!("revision {} reverted", rev.short()),
                                            Op::Revert {
                                                revision: rev.id.clone(),
                                                message: None,
                                                choices: Choices::new(),
                                            },
                                        ));
                                    }
                                });
                            });
                            ui.end_row();
                        }
                    });
            },
        );
    }

    fn changes_page(&mut self, ui: &mut egui::Ui, r: &Ready, snap: &Snapshot) {
        let model = &r.model;
        let Some(s) = session(model) else {
            return;
        };
        weak(
            ui,
            "Compare two revisions (a branch, tag or revision id); \"To\" empty: the working state.",
        );
        ui.horizontal(|ui| {
            ui.label("From");
            ref_combo(ui, "diff-from", &mut self.from, snap, "HEAD");
            ui.label("To");
            ref_combo(ui, "diff-to", &mut self.to, snap, "working state");
            if ui.button("Compare").clicked() {
                self.impact = None;
                self.changes = Some(compare(s, &self.from, &self.to));
            }
        });
        let Some(changes) = &self.changes else {
            return;
        };
        let (title, d, a, b) = match changes {
            Ok(x) => x,
            Err(e) => {
                error(ui, e);
                return;
            }
        };
        ui.label(egui::RichText::new(format!("{title}: {}", d.summary())).strong());
        crate::check_help::list(
            ui,
            "versions-diff",
            300.0,
            egui::ScrollArea::vertical().id_salt("diff"),
            |ui| {
                for o in &d.objects {
                    let head = format!("{} {} {}", o.change.as_str(), o.kind.as_str(), o.id);
                    if o.fields.is_empty() {
                        ui.label(head);
                        continue;
                    }
                    egui::CollapsingHeader::new(head)
                        .id_salt(("diff", o.kind.as_str(), &o.id))
                        .show(ui, |ui| {
                            for f in &o.fields {
                                ui.label(format!(
                                    "{}: {} ➡ {}",
                                    f.path,
                                    value(&f.before),
                                    value(&f.after)
                                ));
                            }
                        });
                }
                for l in &d.libraries {
                    ui.label(format!(
                        "{} library file {}/{} ({} lines out, {} in)",
                        l.change.as_str(),
                        l.kind,
                        l.name,
                        l.lines.0,
                        l.lines.1
                    ));
                }
            },
        );
        let case = model.cases.get(r.case).map(|c| c.case.clone());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !d.is_empty() && case.is_some(),
                    egui::Button::new("Engineering impact"),
                )
                .on_hover_text("Solve the sections touched on both sides in the weather case shown")
                .clicked()
                && let Some(case) = &case
            {
                let cases: Vec<_> = model.cases.iter().map(|c| c.case.clone()).collect();
                let terrain = model.ground_for_checks();
                let ground = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
                let g: Option<ohl_session::impact::Ground<'_>> = match &terrain {
                    Some(_) => Some(&ground),
                    None => None,
                };
                self.impact = Some(ohl_session::impact::engineering_impact(
                    a,
                    b,
                    d,
                    &ImpactOptions {
                        case,
                        cases: &cases,
                        fallback_cables: Some(&model.library),
                        ground: g,
                        max_sections: 200,
                        time_limit: std::time::Duration::from_secs(10),
                    },
                ));
            }
            if let Some(c) = &case {
                weak(ui, format!("in \"{}\"", c.name));
            }
        });
        if let Some(impact) = &self.impact {
            impact_ui(ui, impact);
        }
    }

    fn merge_page(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        snap: &Snapshot,
        can_edit: bool,
        ops: &mut Vec<(String, Op)>,
    ) {
        let Some(s) = session(model) else {
            return;
        };
        weak(
            ui,
            "Merge another branch into the current one. Changes that do not overlap merge by themselves; for each conflict choose ours (this branch) or theirs.",
        );
        ui.horizontal(|ui| {
            ui.label("Merge");
            ref_combo(ui, "merge-other", &mut self.other, snap, "branch");
            if ui
                .add_enabled(!self.other.trim().is_empty(), egui::Button::new("Preview"))
                .clicked()
            {
                self.plan = Some(
                    s.merge_plan(&self.other, &self.choices)
                        .map_err(|e| e.to_string()),
                );
            }
        });
        let Some(plan) = &self.plan else {
            return;
        };
        let plan = match plan {
            Ok(p) => p.clone(),
            Err(e) => {
                error(ui, e);
                return;
            }
        };
        ui.label(match plan.kind {
            MergeKind::UpToDate => "Already contained: nothing to merge.".to_owned(),
            MergeKind::FastForward => format!(
                "Fast-forward to {}: {}",
                short(&plan.theirs),
                plan.changes.summary()
            ),
            MergeKind::ThreeWay => format!(
                "Three-way merge with {} (common ancestor {}): {}",
                short(&plan.theirs),
                plan.base.as_deref().map(short).unwrap_or("none"),
                plan.changes.summary()
            ),
        });
        let mut changed = false;
        if !plan.conflicts.is_empty() {
            ui.label(egui::RichText::new(format!("Conflicts ({})", plan.conflicts.len())).strong());
            egui::Grid::new("merge-conflicts")
                .striped(true)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    for h in ["what", "base", "ours", "theirs", "keep"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for c in &plan.conflicts {
                        ui.label(&c.what);
                        ui.label(value(&c.base));
                        ui.label(value(&c.ours));
                        ui.label(value(&c.theirs));
                        ui.horizontal(|ui| {
                            let mut pick = c.chosen;
                            ui.radio_value(&mut pick, Some(Side::Ours), "ours");
                            ui.radio_value(&mut pick, Some(Side::Theirs), "theirs");
                            if pick != c.chosen
                                && let Some(p) = pick
                            {
                                self.choices.insert(c.key.clone(), p);
                                changed = true;
                            }
                        });
                        ui.end_row();
                    }
                });
        }
        if changed {
            self.plan = Some(
                s.merge_plan(&self.other, &self.choices)
                    .map_err(|e| e.to_string()),
            );
        }
        if let Some(p) = &plan.problem {
            error(ui, &format!("The merged line is not valid: {p}"));
        }
        if plan.kind == MergeKind::UpToDate {
            return;
        }
        ui.add_enabled_ui(can_edit, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.merge_message)
                        .hint_text("message (default: Merge … into …)")
                        .desired_width(320.0),
                );
                let ready = plan.unresolved().is_empty() && plan.problem.is_none();
                if ui.add_enabled(ready, egui::Button::new("Merge")).clicked() {
                    let message = std::mem::take(&mut self.merge_message);
                    ops.push((
                        format!("{} merged", self.other),
                        Op::Merge {
                            other: self.other.clone(),
                            message: (!message.trim().is_empty()).then_some(message),
                            choices: std::mem::take(&mut self.choices),
                        },
                    ));
                }
            });
        });
    }

    fn reviews_page(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        snap: &Snapshot,
        can_edit: bool,
        ops: &mut Vec<(String, Op)>,
    ) {
        let me = model
            .editing
            .as_ref()
            .map(|e| e.user.clone())
            .unwrap_or_default();
        let admin = model.my_role() == Some(Role::Admin);
        ui.add_enabled_ui(can_edit, |ui| {
            ui.horizontal(|ui| {
                ui.label("Submit");
                ref_combo(ui, "review-branch", &mut self.review_branch, snap, "branch");
                ui.label("into");
                if self.review_target.is_empty() {
                    self.review_target = ohl_session::versions::MAIN.into();
                }
                ref_combo(ui, "review-target", &mut self.review_target, snap, "main");
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.reviewers)
                        .hint_text(format!(
                            "reviewers, comma-separated ({})",
                            snap.users.join(", ")
                        ))
                        .desired_width(300.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.review_note)
                        .hint_text("what to look at")
                        .desired_width(220.0),
                );
                if ui
                    .add_enabled(
                        !self.review_branch.trim().is_empty(),
                        egui::Button::new("Submit"),
                    )
                    .clicked()
                {
                    let reviewers = self
                        .reviewers
                        .split(',')
                        .map(|x| x.trim().to_owned())
                        .filter(|x| !x.is_empty())
                        .collect();
                    ops.push((
                        format!("{} submitted for review", self.review_branch),
                        Op::SubmitReview {
                            branch: self.review_branch.clone(),
                            target: self.review_target.clone(),
                            reviewers,
                            note: std::mem::take(&mut self.review_note),
                        },
                    ));
                }
            });
        });
        ui.separator();
        if snap.reviews.is_empty() {
            weak(ui, "No review yet.");
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.comment)
                .hint_text("comment for approve / reject")
                .desired_width(420.0),
        );
        crate::check_help::list(
            ui,
            "versions-reviews",
            360.0,
            egui::ScrollArea::vertical().id_salt("reviews"),
            |ui| {
                for rv in &snap.reviews {
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "#{} {} ➡ {} ({}) — {}",
                                rv.id,
                                rv.branch,
                                rv.target,
                                short(&rv.revision),
                                rv.state.as_str()
                            ))
                            .strong(),
                        );
                        weak(
                            ui,
                            format!(
                                "by {} {}; reviewers: {}",
                                rv.author,
                                rv.time.get(..16).unwrap_or(&rv.time).replace('T', " "),
                                if rv.reviewers.is_empty() {
                                    "admins".to_owned()
                                } else {
                                    rv.reviewers.join(", ")
                                }
                            ),
                        );
                        if !rv.note.is_empty() {
                            ui.label(&rv.note);
                        }
                        for n in &rv.notes {
                            ui.label(format!("{} {}: {}", n.user, n.verdict, n.comment));
                        }
                        let reviewer = rv.author != me && (admin || rv.reviewers.contains(&me));
                        // The author merges too, once somebody else approved.
                        ui.add_enabled_ui(can_edit && (reviewer || rv.author == me), |ui| {
                            ui.horizontal(|ui| {
                                if rv.state == ReviewState::Open && reviewer {
                                    for (approve, label) in [(true, "Approve"), (false, "Reject")] {
                                        if ui.button(label).clicked() {
                                            ops.push((
                                                format!(
                                                    "review #{} {}",
                                                    rv.id,
                                                    if approve { "approved" } else { "rejected" }
                                                ),
                                                Op::DecideReview {
                                                    id: rv.id,
                                                    approve,
                                                    comment: std::mem::take(&mut self.comment),
                                                },
                                            ));
                                        }
                                    }
                                }
                                if rv.state == ReviewState::Approved
                                    && ui
                                        .button("Merge")
                                        .on_hover_text(format!(
                                            "Into {} (check it out first)",
                                            rv.target
                                        ))
                                        .clicked()
                                {
                                    ops.push((
                                        format!("review #{} merged", rv.id),
                                        Op::MergeReview {
                                            id: rv.id,
                                            message: None,
                                            choices: Choices::new(),
                                        },
                                    ));
                                }
                            });
                        });
                    });
                }
            },
        );
    }

    fn rights_page(
        &mut self,
        ui: &mut egui::Ui,
        snap: &Snapshot,
        admin: bool,
        ops: &mut Vec<(String, Op)>,
    ) {
        weak(
            ui,
            "Users without grants edit what their role allows. A user (or role) with grants edits only what one of them covers; admins are never limited.",
        );
        egui::Grid::new("grants")
            .striped(true)
            .spacing([10.0, 3.0])
            .show(ui, |ui| {
                for g in &snap.grants {
                    ui.label(g.id.to_string());
                    ui.label(&g.grant.subject);
                    ui.label(g.grant.describe());
                    ui.label(&g.grant.note);
                    if ui
                        .add_enabled(admin, egui::Button::new("Revoke").small())
                        .clicked()
                    {
                        ops.push((format!("grant {} revoked", g.id), Op::Revoke { id: g.id }));
                    }
                    ui.end_row();
                }
            });
        if !admin {
            return;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Who");
            egui::ComboBox::from_id_salt("grant-subject")
                .selected_text(if self.grant.subject.is_empty() {
                    "user or role".to_owned()
                } else {
                    self.grant.subject.clone()
                })
                .show_ui(ui, |ui| {
                    for u in &snap.users {
                        ui.selectable_value(&mut self.grant.subject, u.clone(), u);
                    }
                    for role in ["viewer", "designer"] {
                        let s = format!("role:{role}");
                        ui.selectable_value(&mut self.grant.subject, s.clone(), s);
                    }
                });
            ui.label("kinds");
            for k in GRANT_KINDS {
                let mut on = self.grant.kinds.iter().any(|x| x == k);
                if ui.checkbox(&mut on, k).changed() {
                    self.grant.kinds.retain(|x| x != k);
                    if on {
                        self.grant.kinds.push(k.to_owned());
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.grant_objects)
                    .hint_text("objects: T12, structure:T13 (empty: any)")
                    .desired_width(260.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.grant_area)
                    .hint_text("area X,Y; X,Y; X,Y (empty: anywhere)")
                    .desired_width(260.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.grant.note)
                    .hint_text("note")
                    .desired_width(140.0),
            );
        });
        let area = parse_area(&self.grant_area);
        if let Err(e) = &area {
            error(ui, e);
        }
        if ui
            .add_enabled(
                !self.grant.subject.is_empty() && area.is_ok(),
                egui::Button::new("Grant"),
            )
            .clicked()
        {
            let mut grant = self.grant.clone();
            grant.objects = self
                .grant_objects
                .split(',')
                .map(|x| x.trim().to_owned())
                .filter(|x| !x.is_empty())
                .collect();
            grant.area = area.ok().flatten();
            ops.push((
                format!("{} may edit {}", grant.subject, grant.describe()),
                Op::Grant { grant },
            ));
            self.grant = Grant::default();
            self.grant_objects.clear();
            self.grant_area.clear();
        }
    }
}

/// A text field with a menu of the branches and tags.
fn ref_combo(ui: &mut egui::Ui, id: &str, text: &mut String, snap: &Snapshot, hint: &str) {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(160.0),
    );
    egui::ComboBox::from_id_salt(id)
        .selected_text("")
        .width(24.0)
        .show_ui(ui, |ui| {
            for x in &snap.refs {
                if x.revision.is_some() {
                    ui.selectable_value(text, x.name.clone(), &x.name);
                }
            }
        });
}

/// The states compared and their differences.
fn compare(
    s: &ohl_session::Session,
    from: &str,
    to: &str,
) -> Result<(String, Diff, State, State), String> {
    let e = |e: ohl_session::SessionError| e.to_string();
    let from = if from.trim().is_empty() {
        "HEAD"
    } else {
        from.trim()
    };
    let a_id = s.resolve_revision(from).map_err(e)?;
    let a = s.revision_state(&a_id).map_err(e)?;
    let (b_name, b) = if to.trim().is_empty() {
        ("working state".to_owned(), s.working_state().map_err(e)?)
    } else {
        let id = s.resolve_revision(to).map_err(e)?;
        (short(&id).to_owned(), s.revision_state(&id).map_err(e)?)
    };
    let d = diff(&a, &b);
    Ok((format!("{} ➡ {b_name}", short(&a_id)), d, a, b))
}

fn impact_ui(ui: &mut egui::Ui, impact: &Impact) {
    egui::Grid::new("impact")
        .striped(true)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            for h in [
                "section",
                "max sag, m",
                "max tension, kN",
                "min clearance, m",
                "",
            ] {
                ui.strong(h);
            }
            ui.end_row();
            let pair = |x: Option<f64>, y: Option<f64>, k: f64| {
                let f = |v: Option<f64>| v.map_or("—".to_owned(), |v| format!("{:.2}", v * k));
                match (x, y) {
                    (Some(a), Some(b)) => format!("{} ➡ {} ({:+.2})", f(x), f(y), (b - a) * k),
                    _ => format!("{} ➡ {}", f(x), f(y)),
                }
            };
            for s in &impact.sections {
                let (x, y) = (s.before, s.after);
                ui.label(&s.section);
                ui.label(pair(x.map(|f| f.max_sag), y.map(|f| f.max_sag), 1.0));
                ui.label(pair(
                    x.map(|f| f.max_tension),
                    y.map(|f| f.max_tension),
                    1e-3,
                ));
                ui.label(pair(
                    x.and_then(|f| f.min_clearance),
                    y.and_then(|f| f.min_clearance),
                    1.0,
                ));
                ui.label(s.errors.join("; "));
                ui.end_row();
            }
        });
    if let Some(why) = &impact.stopped {
        weak(
            ui,
            format!("Stopped ({why}): {} sections not solved.", impact.skipped),
        );
    }
}

fn parse_area(text: &str) -> Result<Option<Vec<[f64; 2]>>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let pts = text
        .split(';')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let v: Vec<f64> = p
                .split(',')
                .map(ohl_core::units::parse_finite)
                .collect::<Result<_, _>>()
                .map_err(|_| format!("area: \"{}\" is not X,Y", p.trim()))?;
            match v[..] {
                [x, y] => Ok([x, y]),
                _ => Err(format!("area: \"{}\" is not X,Y", p.trim())),
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    if pts.len() < 3 {
        return Err("area: at least 3 points".into());
    }
    Ok(Some(pts))
}
