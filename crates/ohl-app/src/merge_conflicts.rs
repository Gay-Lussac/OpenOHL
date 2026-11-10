// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Bringing the changes of one side into a project line, as every window
//! that does it shows it (owner, 2026-11-10): Versions › Bring in
//! changes… ([`crate::project_lines`]), and the Merge page, the merge of
//! a review and the revert of a revision of Revisions, tags and reviews
//! ([`crate::versions`]).
//!
//! One [`Settling`] per window: what is brought in ([`Source`]), the
//! conflicts settled so far (kept until the merge is made or another side
//! is chosen — never dropped by a refusal) and the plan worked out for
//! them. The plan — the conflicts by object with their values in words
//! ([`crate::merge_describe`]), what the merge changes, whether the
//! result is valid — is worked out when the choices or the project
//! change, never per frame; drawing reads its texts.
//!
//! [`panel`] draws it: the heading ("Bring changes of main into phase
//! 1"), a refusal with its way out (uncommitted work: Commit first), the
//! conflicts in a list of fixed height (per object: keep this line's /
//! take the other's / keep both, a Show button, its fields in three
//! columns), what the merge changes, the engineering impact on demand.

use eframe::egui;
use ohl_session::impact::Impact;
use ohl_session::versions::{Choices, Side};

use crate::app::Ready;
pub use crate::merge_plan::{ConflictObject, Names, Plan, Refusal, Source, work_out};
use crate::model::Model;
use crate::selection::Selection;

/// With at most this many objects in conflict their fields are shown at
/// once; with more, each opens on a click.
const OPEN_AT_ONCE: usize = 3;

/// A plan and what it was worked out for.
struct Cache {
    /// The line's, the project's and the libraries' counters.
    key: (u64, u64, u64),
    choices: Choices,
    plan: Result<Plan, String>,
}

/// A merge being settled in a window.
pub struct Settling {
    /// What is brought in.
    pub source: Source,
    /// The conflicts settled so far: kept until the merge is made or
    /// another source is chosen.
    pub choices: Choices,
    /// Why the merge was refused when it was last asked for
    /// ([`Settling::refused`]); gone when the project changes after.
    pub error: Option<String>,
    /// The refusal is newer than the plan (which is worked out again
    /// right after an operation, refused or not).
    error_new: bool,
    /// Closing was asked for with choices made: asked again first.
    pub asking: bool,
    cache: Option<Box<Cache>>,
    impact: Option<Impact>,
}

impl std::fmt::Debug for Settling {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settling")
            .field("source", &self.source)
            .field("choices", &self.choices)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Settling {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.choices == other.choices
    }
}

impl Settling {
    /// `source`, nothing settled yet.
    pub fn new(source: Source) -> Self {
        Self {
            source,
            choices: Choices::new(),
            error: None,
            error_new: false,
            asking: false,
            cache: None,
            impact: None,
        }
    }

    /// Another source: the choices made for the one before are not its.
    pub fn set_source(&mut self, source: Source) {
        if source != self.source {
            *self = Self::new(source);
        }
    }

    /// The merge was asked for and refused: said in the window, the
    /// choices kept.
    pub fn refused(&mut self, why: String) {
        self.error = Some(why);
        self.error_new = true;
    }

    /// The plan as last worked out ([`panel`] works it out).
    #[cfg(test)]
    pub fn plan(&self) -> Option<&Result<Plan, String>> {
        self.cache.as_ref().map(|c| &c.plan)
    }

    /// Works the plan out again when the choices or the project changed
    /// since it was (else nothing is read).
    pub fn refresh(&mut self, model: &Model) {
        // A drag previews the line every frame without changing the
        // project: worked out when it ends.
        if model.previewing && self.cache.is_some() {
            return;
        }
        let key = (
            model.line_version,
            model.project_version,
            model.revisions.libraries,
        );
        if self
            .cache
            .as_ref()
            .is_some_and(|c| c.key == key && c.choices == self.choices)
        {
            return;
        }
        let plan = match model.session() {
            Some(s) => work_out(&self.source, s, &self.choices),
            None => Err("no project".to_owned()),
        };
        self.impact = None;
        // A refusal is said until the project changes again.
        if !std::mem::take(&mut self.error_new) {
            self.error = None;
        }
        self.cache = Some(Box::new(Cache {
            key,
            choices: self.choices.clone(),
            plan,
        }));
    }

    /// Closing asked for: at once when nothing was settled, else asked
    /// again first ([`close_question`]). Returns whether to close now.
    pub fn close(&mut self) -> bool {
        if self.choices.is_empty() {
            return true;
        }
        self.asking = true;
        false
    }
}

/// What the user did in a [`panel`].
#[derive(Debug, Default)]
pub struct Response {
    /// Whether the merge can be applied, else why not.
    pub ready: Option<Result<(), &'static str>>,
    /// Show was clicked: what to select and bring the views to.
    pub show: Option<Selection>,
    /// Commit first was clicked.
    pub commit_first: bool,
}

impl Response {
    /// Whether the merge can be applied.
    pub fn is_ready(&self) -> bool {
        self.ready == Some(Ok(()))
    }

    /// Why it cannot be applied.
    pub fn why_not(&self) -> &'static str {
        match self.ready {
            Some(Err(why)) => why,
            _ => "Nothing to apply",
        }
    }
}

fn weak(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).small().weak());
}

/// A merge being settled: its heading, a refusal and its way out, the
/// conflicts, what it changes, the engineering impact on demand. The
/// buttons that apply or close are the window's.
pub fn panel(ui: &mut egui::Ui, r: &Ready, st: &mut Settling) -> Response {
    let mut out = Response::default();
    st.refresh(&r.model);
    let Some(cache) = st.cache.take() else {
        return out;
    };
    match &cache.plan {
        Err(e) => {
            crate::menu::error_label(ui, e.as_str());
        }
        Ok(plan) => {
            ui.label(egui::RichText::new(&plan.names.heading).strong());
            if !plan.headline.is_empty() {
                ui.label(&plan.headline);
            }
            if let Some(refusal) = &plan.refusal {
                out.commit_first = refusal_ui(ui, refusal);
            }
            if !plan.nothing {
                conflict_list(ui, plan, &mut st.choices, &mut out);
                preview(ui, plan);
                if let Some(p) = &plan.problem {
                    crate::menu::error_label(
                        ui,
                        format!("{} would not be valid: {p}", plan.names.into),
                    );
                }
                if !plan.notes.is_empty() {
                    crate::menu::message(
                        ui,
                        egui::RichText::new(plan.notes.join("\n")).small(),
                        None,
                    );
                }
                impact_row(ui, r, plan, &mut st.impact);
            }
            out.ready = Some(plan.ready());
        }
    }
    if let Some(e) = &st.error {
        crate::menu::error_label(ui, format!("Not applied: {e}"));
        if !st.choices.is_empty() {
            weak(ui, "Your choices are kept.");
        }
    }
    st.cache = Some(cache);
    out
}

/// Why the merge cannot be made as things are; Commit first where that
/// is the way out. Returns whether it was clicked.
pub fn refusal_ui(ui: &mut egui::Ui, refusal: &Refusal) -> bool {
    let mut clicked = false;
    ui.horizontal_wrapped(|ui| {
        crate::menu::error_label(ui, refusal.text.as_str());
        if refusal.commit_here {
            clicked = ui
                .button("Commit first")
                .on_hover_text(
                    "Opens the History page with the message field ready: commit, then come \
                     back — what you settled here is kept",
                )
                .clicked();
        }
    });
    clicked
}

/// The question asked before a window with choices made is closed.
/// Returns whether to close.
pub fn close_question(ui: &mut egui::Ui, st: &mut Settling) -> bool {
    let mut close = false;
    ui.group(|ui| {
        ui.label(format!(
            "{} choice(s) made here are dropped when the window closes. Close it?",
            st.choices.len()
        ));
        ui.horizontal(|ui| {
            close = ui
                .button("Drop them and close")
                .on_hover_text("The conflicts are settled again from the start next time")
                .clicked();
            if ui
                .button("Keep settling")
                .on_hover_text("The window stays open, the choices as they are")
                .clicked()
            {
                st.asking = false;
            }
        });
    });
    close
}

/// The objects in conflict, in a list of fixed height; the buttons
/// settling all of them at once.
fn conflict_list(ui: &mut egui::Ui, plan: &Plan, choices: &mut Choices, out: &mut Response) {
    if plan.objects.is_empty() {
        ui.label("No conflict: everything combines by itself.");
        return;
    }
    ui.label(format!(
        "{} object(s) changed on both — keep which? ({})",
        plan.objects.len(),
        match plan.open {
            0 => "all settled".to_owned(),
            n => format!("{n} still to settle"),
        }
    ));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        crate::check_help::list(
            ui,
            "merge-conflicts",
            crate::menu::TEXT_BOX_HEIGHT,
            egui::ScrollArea::vertical()
                .id_salt("merge-conflicts")
                .auto_shrink([false, true]),
            |ui| {
                for o in &plan.objects {
                    object_row(ui, plan, o, choices, out);
                }
            },
        );
    });
    ui.horizontal(|ui| {
        let all = |choices: &mut Choices, side: Side| {
            choices.clear();
            for o in &plan.objects {
                choices.insert(o.key.clone(), side);
            }
        };
        let [this, other] = &plan.names.sides;
        if ui
            .small_button(format!("All: {}", plan.names.keep))
            .on_hover_text(format!(
                "Every object in conflict stays as it is on {this}; what {other} changed in \
                 them is not taken"
            ))
            .clicked()
        {
            all(choices, Side::Ours);
        }
        if ui
            .small_button(format!("All: {}", plan.names.take))
            .on_hover_text(format!(
                "Every object in conflict becomes as it is on {other}; what {this} changed in \
                 them is given up"
            ))
            .clicked()
        {
            all(choices, Side::Theirs);
        }
    });
}

/// One object both sides changed: what it is, the side kept, Show; its
/// fields under it.
fn object_row(
    ui: &mut egui::Ui,
    plan: &Plan,
    o: &ConflictObject,
    choices: &mut Choices,
    out: &mut Response,
) {
    let names = &plan.names;
    let id = ui.make_persistent_id(("merge-conflict", &o.key));
    let open = plan.objects.len() <= OPEN_AT_ONCE;
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open)
        .show_header(ui, |ui| {
            ui.label(egui::RichText::new(&o.what).strong())
                .on_hover_text(&o.summary);
            let was = choices.get(&o.key).copied();
            let mut side = was;
            ui.radio_value(&mut side, Some(Side::Ours), &names.keep)
                .on_hover_text(format!(
                    "The object stays as it is on {}: what {} changed in it is not taken",
                    names.sides[0], names.sides[1]
                ));
            ui.radio_value(&mut side, Some(Side::Theirs), &names.take)
                .on_hover_text(format!(
                    "The object becomes as it is on {}: what {} changed in it is given up",
                    names.sides[1], names.sides[0]
                ));
            if let Some(new_id) = &o.keep_both {
                ui.radio_value(&mut side, Some(Side::Both), "keep both")
                    .on_hover_text(format!(
                        "Each side made an object under this id. Both are kept: {}'s as it is, \
                         {}'s as \"{new_id}\" — what refers to it there (sections, jumpers, \
                         alignment points…) follows",
                        names.sides[0], names.sides[1]
                    ));
            }
            if side != was
                && let Some(s) = side
            {
                // The object's choice settles its fields: theirs go.
                let prefix = format!("{}/", o.key);
                choices.retain(|k, _| !k.starts_with(&prefix));
                choices.insert(o.key.clone(), s);
            }
            show_button(ui, plan, o, out);
            weak(ui, &o.summary);
        })
        .body(|ui| field_rows(ui, names, o, choices));
}

/// Show: the object selected and the views brought to it.
fn show_button(ui: &mut egui::Ui, plan: &Plan, o: &ConflictObject, out: &mut Response) {
    let why_not = if o.target.is_none() {
        Some("Nothing of it is drawn in the views".to_owned())
    } else if !plan.here {
        Some(format!(
            "It is on {}, not on the line you work on: work on {} to see it",
            plan.names.into, plan.names.into
        ))
    } else if !o.in_line {
        Some(match &plan.names.shown {
            Some(other) => format!(
                "It is deleted on this line. To see {other}'s: tick {other} under Versions › \
                 Shown next to it"
            ),
            None => "It is deleted on this line".to_owned(),
        })
    } else {
        None
    };
    let mut hover = "Selects it and brings the views to it (3D, plan, profile), as it is on the \
                     line you work on."
        .to_owned();
    if let Some(other) = &plan.names.shown {
        hover.push_str(&format!(
            " To see {other}'s next to it, in its colour: tick {other} under Versions › Shown \
             next to it."
        ));
    }
    let button = ui
        .add_enabled(why_not.is_none(), egui::Button::new("Show").small())
        .on_hover_text(hover)
        .on_disabled_hover_text(why_not.unwrap_or_default());
    if button.clicked() {
        out.show = o.target.clone();
    }
}

/// An object's fields in conflict: common, this line, the other side;
/// a side for one field alone where the fields combine.
fn field_rows(ui: &mut egui::Ui, names: &Names, o: &ConflictObject, choices: &mut Choices) {
    if o.rows.is_empty() {
        weak(ui, &o.summary);
        return;
    }
    egui::Grid::new(("merge-fields", &o.key))
        .striped(true)
        .spacing([14.0, 3.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("field").small().strong());
            for h in &names.columns {
                ui.label(egui::RichText::new(h).small().strong());
            }
            ui.end_row();
            for row in &o.rows {
                ui.label(&row.label);
                for t in &row.texts {
                    ui.label(t);
                }
                if let Some(key) = &row.key {
                    // The field's own choice, else the object's.
                    let was = choices
                        .get(key)
                        .or_else(|| choices.get(&o.key))
                        .copied()
                        .filter(|s| *s != Side::Both);
                    let mut side = was;
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut side,
                            Some(Side::Ours),
                            format!("{}'s", names.sides[0]),
                        )
                        .on_hover_text(format!(
                            "This field alone as it is on {} (the object's other fields as \
                                 chosen for them)",
                            names.sides[0]
                        ));
                        ui.radio_value(
                            &mut side,
                            Some(Side::Theirs),
                            format!("{}'s", names.sides[1]),
                        )
                        .on_hover_text(format!(
                            "This field alone as it is on {} (the object's other fields as \
                             chosen for them)",
                            names.sides[1]
                        ));
                    });
                    if side != was
                        && let Some(s) = side
                    {
                        choices.insert(key.clone(), s);
                    }
                }
                ui.end_row();
            }
        });
    if o.more > 0 {
        weak(ui, format!("… and {} more fields", o.more));
    }
}

/// What the merge changes in the line, before it is applied.
fn preview(ui: &mut egui::Ui, plan: &Plan) {
    let into = &plan.names.into;
    let Some(changes) = &plan.changes else {
        weak(
            ui,
            format!(
                "Everything else {} changed comes into {into} as it is. (The whole list of what \
                 changes is not worked out before a Bring in; after it: Revisions, tags and \
                 reviews › History › Diff of the new revision. Undone as a whole by reverting \
                 that revision.)",
                plan.names.sides[1]
            ),
        );
        return;
    };
    if changes.rows.is_empty() {
        ui.label(if plan.open > 0 {
            format!(
                "Nothing else changes in {into}: only the objects in conflict, as you settle \
                 them."
            )
        } else {
            format!("It changes nothing in {into} (its history records the merge).")
        });
        return;
    }
    let pending = if plan.open > 0 {
        " — with the conflicts still open kept as they are on this side"
    } else {
        ""
    };
    egui::CollapsingHeader::new(format!(
        "What it changes in {into}: {}{pending}",
        changes.summary
    ))
    .id_salt("merge-preview")
    .default_open(true)
    .show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("merge-preview-rows")
            .max_height(crate::menu::TEXT_BOX_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for row in &changes.rows {
                    ui.label(row);
                }
                if changes.more > 0 {
                    weak(ui, format!("… and {} more", changes.more));
                }
            });
    });
}

/// The engineering impact of the merge (the line before against the line
/// merged), solved when asked for.
fn impact_row(ui: &mut egui::Ui, r: &Ready, plan: &Plan, impact: &mut Option<Impact>) {
    let Some((merged, d)) = &plan.merged else {
        return;
    };
    let case = r.model.cases.get(r.case).map(|c| c.case.name.clone());
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !d.is_empty() && case.is_some(),
                egui::Button::new("Engineering impact"),
            )
            .on_hover_text(format!(
                "Solves the sections the merge touches, in {} as it is and as merged, in the \
                 weather case shown: largest sag and tension, least clearance to the ground. \
                 Nothing is changed. Up to 10 s",
                plan.names.into
            ))
            .on_disabled_hover_text("Nothing changes, or no weather case is shown")
            .clicked()
        {
            let before = match &plan.before {
                Some(b) => Some(b.clone()),
                None => r.model.session().and_then(|s| s.working_state().ok()),
            };
            if let Some(before) = before {
                *impact = crate::versions::impact_between(r, &before, merged, d);
            }
        }
        if let Some(c) = &case {
            weak(ui, format!("in \"{c}\""));
        }
    });
    if let Some(impact) = impact {
        egui::ScrollArea::vertical()
            .id_salt("merge-impact")
            .max_height(crate::menu::TEXT_BOX_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| crate::versions::impact_ui(ui, impact));
    }
}
