// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Plans & sheets › Route plan…, Situation plans…, Phase plan… and Plan
//! set… (owner, 2026-10-24: a menu of their own, apart from the profile
//! sheets; 2026-11-08: "one menu per type that isn't opening the same
//! window as others"): each its own window ([`PlanWindows`]), laid out as
//! the others ([`crate::export_form`]). The **route plan** and the
//! **situation plans** (north up, over the project's drawings and
//! orthophotos) and the **phase plan** (the structures where the phase
//! arrangement changes), written for every route of the Profile sheets
//! window, each plan with **its own sheet template** and its values
//! (owner, 2026-11-08: "they should all have their own template separate
//! from the profile plan but defined in a similar way"; [`PlanSheet`],
//! kept in the project) ([`crate::drawings::run_plans`]).

use std::path::PathBuf;

use eframe::egui;

use crate::drawings::{
    Input, PhaseGroup, PlanKind, PlanRequest, plan_set_inputs, run_plan_set, run_plans,
};
use crate::model::Model;
use crate::survey_io::Job;
use ohl_drawing::SheetFields;
use ohl_drawing::export::Format;
use ohl_drawing::plan_set::PlanSet;
use serde::{Deserialize, Serialize};

/// The formats the plans are written in.
/// In the Profile sheets window's order.
const FORMATS: [Format; 4] = [Format::Svg, Format::Dxf, Format::Pdf, Format::Png];

/// The papers offered.
const PAPERS: [&str; 5] = ["A0", "A1", "A2", "A3", "A4"];

/// A plan's own sheet template and the values of its fields (title
/// block, revisions), as the Profile sheets window has its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanSheet {
    /// The template: `default`, `project:NAME`, `library:NAME`,
    /// `plugin:PLUGIN/NAME` or `file:PATH` ([`crate::sheet_ui::SheetChoice::template`]).
    pub template: String,
    /// The values of its fields and the revisions.
    pub fields: SheetFields,
}

impl Default for PlanSheet {
    fn default() -> Self {
        Self {
            template: "default".into(),
            fields: SheetFields::default(),
        }
    }
}

/// The project setting a plan's sheet is kept in, per plan:
/// `app.plan_sheets.<route|situation|phase>.<project line>`.
pub const SHEET_SETTING: &str = "app.plan_sheets";

/// The four plan windows, each opened by its own menu item and keeping
/// its own settings.
#[derive(Debug)]
pub struct PlanWindows {
    /// Plans & sheets › Route plan….
    pub route: PlansWindow,
    /// Plans & sheets › Situation plans….
    pub situation: PlansWindow,
    /// Plans & sheets › Phase plan….
    pub phase: PlansWindow,
    /// Plans & sheets › Plan set….
    pub set: PlansWindow,
}

impl Default for PlanWindows {
    fn default() -> Self {
        Self {
            route: PlansWindow::of(PlanKind::Route),
            situation: PlansWindow::of(PlanKind::Situation),
            phase: PlansWindow::of(PlanKind::Phase),
            set: PlansWindow {
                set_mode: true,
                ..PlansWindow::of(PlanKind::Situation)
            },
        }
    }
}

impl PlanWindows {
    /// The window of plan `kind`.
    pub fn of(&mut self, kind: PlanKind) -> &mut PlansWindow {
        match kind {
            PlanKind::Route => &mut self.route,
            PlanKind::Situation => &mut self.situation,
            PlanKind::Phase => &mut self.phase,
        }
    }

    /// Each plan's own sheet (the plan set's drawings take them).
    pub fn sheets(&self) -> Vec<(PlanKind, PlanSheet)> {
        [&self.route, &self.situation, &self.phase]
            .iter()
            .map(|w| (w.kind, w.sheet.clone()))
            .collect()
    }

    /// Each window.
    pub fn all(&mut self) -> [&mut PlansWindow; 4] {
        [
            &mut self.route,
            &mut self.situation,
            &mut self.phase,
            &mut self.set,
        ]
    }
}

/// A plan window: one plan, or a plugin's plan set.
#[derive(Debug)]
pub struct PlansWindow {
    /// Shown.
    pub open: bool,
    /// The plan (fixed for the window).
    pub kind: PlanKind,
    /// Paper.
    pub page: String,
    /// The plan's scale (1:…).
    pub scale: f64,
    /// The project's drawings under the route and situation plans.
    pub drawings: bool,
    /// The orthophotos under them.
    pub photo: bool,
    /// The corridor drawn (at the drawings' line width).
    pub corridor: bool,
    /// Formats: PDF, SVG, PNG, DXF.
    pub formats: Vec<Format>,
    /// Output folder.
    pub out: Option<PathBuf>,
    /// Why the last start failed.
    pub error: Option<String>,
    /// The export under way, or done.
    pub job: Option<Job>,
    /// The plan set window (fixed for the window).
    pub set_mode: bool,
    /// The plugins' plan sets: plugin, name, the set or why it does not
    /// read.
    pub sets: Vec<(String, String, Result<PlanSet, String>)>,
    /// The set chosen (index in `sets`).
    pub set: usize,
    /// The set's sheets also as one PDF, `<set name>.pdf` (owner,
    /// 2026-10-25).
    pub one_pdf: bool,
    /// Export asked while its weather cases were being solved: started
    /// once they are.
    pub waiting: bool,
    /// The phase plans of alignments drawn together (owner, 2026-11-03),
    /// kept in the project (`app.phase_plans.<line>`).
    pub groups: Vec<PhaseGroup>,
    /// The Profile sheets window asked for (the route is set there).
    pub open_sheets: bool,
    /// Its own sheet template and values (none used by the plan set
    /// window: each drawing takes its plan's).
    pub sheet: PlanSheet,
    /// The plan set window: the plans' own sheets, as last given.
    pub others: Vec<(PlanKind, PlanSheet)>,
    /// A template file to load or write, asked in the window.
    pub action: Option<crate::sheet_ui::Action>,
    kept: crate::check_ui::Kept,
    kept_sheet: crate::check_ui::Kept,
}

/// The project setting the phase plans' groups are kept in.
pub const SETTING: &str = "app.phase_plans";

impl PlansWindow {
    /// The window of plan `kind`, closed.
    pub fn of(kind: PlanKind) -> Self {
        Self {
            open: false,
            kind,
            page: "A3".into(),
            scale: if kind == PlanKind::Route {
                10_000.0
            } else {
                1000.0
            },
            drawings: true,
            photo: true,
            corridor: true,
            formats: vec![Format::Pdf],
            out: None,
            error: None,
            job: None,
            set_mode: false,
            sets: Vec::new(),
            set: 0,
            one_pdf: false,
            waiting: false,
            groups: Vec::new(),
            open_sheets: false,
            sheet: PlanSheet::default(),
            others: Vec::new(),
            action: None,
            kept: Default::default(),
            kept_sheet: Default::default(),
        }
    }

    /// Opens the window.
    pub fn show(&mut self) {
        self.open = true;
    }

    /// Opens the plan set window on the plan sets of the plugins `dirs`
    /// (name, folder), read now.
    pub fn show_sets(&mut self, dirs: &[(String, PathBuf)]) {
        self.sets = crate::drawings::plugin_plan_sets(dirs);
        self.set = self.set.min(self.sets.len().saturating_sub(1));
        self.open = true;
    }

    /// The window's title, its menu item's name.
    pub fn title(&self) -> &'static str {
        if self.set_mode {
            "Plan set"
        } else {
            self.kind.title()
        }
    }

    /// The request of the window's settings.
    pub fn request(&self) -> PlanRequest {
        PlanRequest {
            kind: self.kind,
            page: self.page.clone(),
            scale: self.scale,
            drawings: self.drawings,
            photo: self.photo,
            corridor: self.corridor,
            groups: if self.kind == PlanKind::Phase {
                self.groups.clone()
            } else {
                Vec::new()
            },
        }
    }

    /// Reads the window's own sheet (and the phase plan its groups) from
    /// the project when not read yet for the line worked on, writes them
    /// when they changed. None kept yet: it starts on `start` (the
    /// sheets' template, as the plans took before they had their own).
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context, start: &str) {
        if self.set_mode {
            return;
        }
        let setting = format!(
            "{SHEET_SETTING}.{}",
            match self.kind {
                PlanKind::Route => "route",
                PlanKind::Situation => "situation",
                PlanKind::Phase => "phase",
            }
        );
        let json = serde_json::to_string(&self.sheet).unwrap_or_default();
        let mut kept = std::mem::take(&mut self.kept_sheet);
        if let crate::check_ui::Step::Load(text) =
            kept.step(model, ctx, &setting, "plan sheet template", &json)
        {
            self.sheet = text
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_else(|| PlanSheet {
                    template: start.to_owned(),
                    ..Default::default()
                });
            kept.loaded(serde_json::to_string(&self.sheet).unwrap_or_default());
        }
        self.kept_sheet = kept;
        if self.kind == PlanKind::Phase {
            self.keep_groups(model, ctx);
        }
    }

    /// The phase plans' groups, kept as [`Self::keep`].
    fn keep_groups(&mut self, model: &mut Model, ctx: &egui::Context) {
        let json = serde_json::to_string(&self.groups).unwrap_or_default();
        let mut kept = std::mem::take(&mut self.kept);
        if let crate::check_ui::Step::Load(text) =
            kept.step(model, ctx, SETTING, "phase plans", &json)
        {
            self.groups = text
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_default();
            kept.loaded(serde_json::to_string(&self.groups).unwrap_or_default());
        }
        self.kept = kept;
    }

    /// The phase plans' groups: each a name and the alignments ticked —
    /// a route's tick takes all its alignments (owner, 2026-11-03).
    fn groups_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let views = model.alignment_views();
        ui.label(egui::RichText::new("Alignments in one plan").strong())
            .on_hover_text(
                "A phase plan of several routes and alignments, drawn one after the other in \
                 the line's order; an alignment in no plan here gets its own, as before",
            );
        let mut remove = None;
        for (k, g) in self.groups.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label("Plan");
                ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(180.0));
                if ui.small_button("Remove").clicked() {
                    remove = Some(k);
                }
            });
            egui::Grid::new(("phase-plan-group", k))
                .num_columns(2)
                .spacing([10.0, 2.0])
                .show(ui, |ui| {
                    let mut routes: Vec<String> = Vec::new();
                    for v in views.iter() {
                        if !routes.contains(&v.route) {
                            routes.push(v.route.clone());
                        }
                    }
                    for r in &routes {
                        let ids: Vec<&String> = views
                            .iter()
                            .filter(|v| &v.route == r)
                            .map(|v| &v.id)
                            .collect();
                        let name =
                            model
                                .line
                                .routes
                                .iter()
                                .find(|x| &x.id == r)
                                .map_or(r.clone(), |x| {
                                    if x.name.is_empty() {
                                        x.id.clone()
                                    } else {
                                        x.name.clone()
                                    }
                                });
                        let mut all = ids.iter().all(|id| g.alignments.contains(id));
                        if ui
                            .checkbox(&mut all, egui::RichText::new(name).strong())
                            .on_hover_text("Every alignment of the route")
                            .changed()
                        {
                            for id in &ids {
                                g.alignments.retain(|a| a != *id);
                                if all {
                                    g.alignments.push((*id).clone());
                                }
                            }
                        }
                        ui.label("");
                        ui.end_row();
                        for v in views.iter().filter(|v| &v.route == r) {
                            let mut on = g.alignments.contains(&v.id);
                            let text = if v.name.is_empty() { &v.id } else { &v.name };
                            ui.label("");
                            if ui.checkbox(&mut on, text).changed() {
                                g.alignments.retain(|a| a != &v.id);
                                if on {
                                    g.alignments.push(v.id.clone());
                                }
                            }
                            ui.end_row();
                        }
                    }
                });
        }
        if let Some(k) = remove {
            self.groups.remove(k);
        }
        if ui
            .button("Add a plan of several")
            .on_hover_text("Tick the routes and alignments drawn together in one phase plan")
            .clicked()
        {
            let n = self.groups.len() + 1;
            self.groups.push(PhaseGroup {
                name: format!("phase plan {n}"),
                alignments: Vec::new(),
            });
        }
    }

    /// The template a set's drawing of `kind` takes when the set names
    /// none: its plan's own.
    pub fn sheet_of(&self, kind: PlanKind) -> PlanSheet {
        self.others
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or_else(PlanSheet::default, |(_, s)| s.clone())
    }

    /// The weather cases the plans asked for draw in (their templates'
    /// and the Profile sheets window's per circuit): solved while the window is
    /// open, the export waiting for them. The phase plan, a schematic,
    /// takes the case shown and needs none.
    pub fn wanted(&self, model: &Model, drawings: &crate::drawings::DrawingsWindow) -> Vec<usize> {
        use ohl_drawing::plan_set::SetKind;
        let needed = |key: &str| {
            crate::sheet_ui::resolve_kept(model, key, &drawings.plugin_templates)
                .ok()
                .and_then(|t| {
                    crate::drawings::SheetCases::of(model, &t, &drawings.sheet.circuit_cases).ok()
                })
                .map(|p| p.needed())
                .unwrap_or_default()
        };
        let mut v = Vec::new();
        if self.set_mode {
            if let Some((plugin, _, Ok(set))) = self.sets.get(self.set) {
                for d in set.drawings.iter().filter(|d| d.kind != SetKind::Phase) {
                    let key = d.template.as_ref().map_or_else(
                        || match d.kind {
                            SetKind::Route => self.sheet_of(PlanKind::Route).template,
                            SetKind::Situation => self.sheet_of(PlanKind::Situation).template,
                            _ => drawings.sheet.template.clone(),
                        },
                        |t| crate::drawings::set_template_key(drawings, plugin, t),
                    );
                    v.extend(needed(&key));
                }
            }
        } else if self.kind != PlanKind::Phase {
            v = needed(&self.sheet.template);
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The window; `drawings` the Profile sheets window's settings (routes,
    /// template, title fields, cases) the plans are made with.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        case: usize,
        drawings: &crate::drawings::DrawingsWindow,
    ) {
        if let Some(job) = &mut self.job
            && crate::survey_io::job_window(ctx, job)
        {
            self.job = None;
        }
        if !self.open {
            self.waiting = false;
            return;
        }
        let mut open = true;
        let mut start = false;
        let offers = crate::sheet_ui::offers_kept(model, &drawings.plugin_templates);
        let template = (!self.set_mode).then(|| {
            crate::sheet_ui::resolve_kept(model, &self.sheet.template, &drawings.plugin_templates)
        });
        let routes = model.alignment_views();
        let route = match drawings.route.and_then(|k| routes.get(k)) {
            Some(r) => format!("{} {}", r.id, r.name),
            None => "every route".into(),
        };
        let title = self.title();
        crate::export_form::window(ctx, title, &mut open, |ui| {
            crate::export_form::intro(
                ui,
                if self.set_mode {
                    "Several drawings in one go, as a plugin's plan set lists them: each with \
                     its plan's own template (the profile sheets with Profile sheets'), the set's \
                     when it names one."
                } else {
                    match self.kind {
                        PlanKind::Route => {
                            "The whole route north up at a small scale, over the project's \
                             drawings (e.g. municipal boundaries) and orthophotos: the axis, \
                             the wires, the structures numbered, the corridor, a north arrow, \
                             a scale bar and a legend."
                        }
                        PlanKind::Situation => {
                            "The route in parts north up at a large scale, the sheets \
                             overlapping, over the project's drawings (the cadastral survey: \
                             parcels and their numbers) and orthophotos."
                        }
                        PlanKind::Phase => {
                            "Each structure's figure seen along the line, every wire labelled \
                             with its circuit (numbered, a key) and phase — only where the \
                             arrangement changes; between, \"arrangement unchanged\"."
                        }
                    }
                },
            );
            ui.add_space(4.0);
            crate::export_form::grid(ui, title, |ui| {
                ui.label("Template");
                if self.set_mode {
                    ui.label("each drawing its plan's own");
                } else {
                    ui.vertical(|ui| {
                        if let Some(a) = crate::sheet_ui::template_ui(
                            ui,
                            title,
                            &mut self.sheet.template,
                            &offers,
                        ) {
                            self.action = Some(a);
                        }
                        if let Some(Err(e)) = &template {
                            crate::menu::error_label(ui, e);
                        }
                    });
                }
                ui.end_row();
                // The route chosen in the sheets window: one way there.
                if crate::export_form::shown_row(ui, "Route", &route, Some("Profile sheets…")) {
                    self.open_sheets = true;
                }
                if self.set_mode {
                    ui.label("Plan set");
                    let shown = self
                        .sets
                        .get(self.set)
                        .map_or("—".to_owned(), |(p, n, _)| format!("{p} / {n}"));
                    egui::ComboBox::from_id_salt((title, "set"))
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (k, (p, n, _)) in self.sets.iter().enumerate() {
                                ui.selectable_value(&mut self.set, k, format!("{p} / {n}"));
                            }
                        });
                    ui.end_row();
                } else {
                    ui.label("Paper");
                    egui::ComboBox::from_id_salt((title, "paper"))
                        .selected_text(self.page.as_str())
                        .show_ui(ui, |ui| {
                            for p in PAPERS {
                                ui.selectable_value(&mut self.page, p.to_owned(), p);
                            }
                        });
                    ui.end_row();
                }
                if !self.set_mode && self.kind != PlanKind::Phase {
                    ui.label("Scale");
                    ui.horizontal(|ui| {
                        ui.label("1:");
                        let range = if self.kind == PlanKind::Route {
                            5000.0..=25_000.0
                        } else {
                            500.0..=2000.0
                        };
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.scale)
                                .range(range)
                                .speed(50.0),
                        );
                    });
                    ui.end_row();
                    ui.label("Under it");
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.drawings, "the project's drawings");
                        ui.checkbox(&mut self.photo, "orthophotos");
                        ui.checkbox(&mut self.corridor, "the corridor");
                    });
                    ui.end_row();
                }
                crate::export_form::formats_row(ui, &FORMATS, &mut self.formats);
                if self.set_mode {
                    ui.label("");
                    ui.checkbox(&mut self.one_pdf, "also as one PDF")
                        .on_hover_text(
                            "Every sheet of the set's drawings, in the set's order, in one PDF \
                             named after the set, besides the files of the formats ticked (a \
                             drawing without paper is not in it)",
                        );
                    ui.end_row();
                }
                crate::export_form::folder_row(
                    ui,
                    &mut self.out,
                    drawings.out.as_ref().map(|p| (p, "Profile sheets'")),
                );
            });
            // The window's own sections, below the settings.
            if self.set_mode {
                if self.sets.is_empty() {
                    ui.label("No plugin in use gives a plan set.");
                }
                match self.sets.get(self.set).map(|x| &x.2) {
                    Some(Ok(set)) => {
                        ui.label(egui::RichText::new(&set.description).small());
                        for d in &set.drawings {
                            ui.label(
                                egui::RichText::new(format!(
                                    "• {}{}{}{}",
                                    d.kind.as_str(),
                                    d.template
                                        .as_ref()
                                        .map_or(String::new(), |t| format!(", template {t}")),
                                    d.page.as_ref().map_or(String::new(), |p| format!(", {p}")),
                                    d.scale.map_or(String::new(), |s| format!(", 1:{s}"))
                                ))
                                .small(),
                            );
                        }
                    }
                    Some(Err(e)) => {
                        crate::menu::error_label(ui, e);
                    }
                    None => {}
                }
            } else {
                if let Some(Ok(t)) = &template {
                    egui::CollapsingHeader::new("Values of the template (title block)")
                        .id_salt((title, "values"))
                        .show(ui, |ui| {
                            crate::sheet_ui::fields_ui(ui, t, &mut self.sheet.fields)
                        });
                    if t.revisions.place != "off" {
                        egui::CollapsingHeader::new(format!(
                            "Revisions ({})",
                            self.sheet.fields.revisions.len()
                        ))
                        .id_salt((title, "revisions"))
                        .show(ui, |ui| {
                            crate::sheet_ui::revisions_ui(
                                ui,
                                &mut self.sheet.fields,
                                crate::sheet_ui::last_commit(model),
                            )
                        });
                    }
                }
                if self.kind == PlanKind::Phase {
                    ui.separator();
                    self.groups_ui(ui, model);
                }
            }
            start = crate::export_form::footer(ui, self.error.as_deref(), self.job.is_some());
        });
        // Its weather cases solved first ([`Self::wanted`]): the export
        // starts once they are.
        let unsolved: Vec<String> = self
            .wanted(model, drawings)
            .into_iter()
            .filter_map(|k| model.cases.get(k))
            .filter(|c| !c.ready)
            .map(|c| format!("\"{}\"", c.case.name))
            .collect();
        if start && !unsolved.is_empty() {
            self.waiting = true;
        }
        if self.waiting {
            if unsolved.is_empty() {
                self.waiting = false;
                start = true;
            } else {
                start = false;
                self.error = Some(format!(
                    "solving weather case {} first: the export starts then",
                    unsolved.join(", ")
                ));
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
        if start {
            let out = self.out.clone().or_else(|| drawings.out.clone());
            match out {
                None => self.error = Some("choose a folder".into()),
                Some(out) if self.formats.is_empty() && !(self.set_mode && self.one_pdf) => {
                    let _ = out;
                    self.error = Some("tick a format".into());
                }
                Some(out) if self.set_mode => {
                    let formats = self.formats.clone();
                    match self.sets.get(self.set) {
                        Some((plugin, name, Ok(set))) => {
                            let one_pdf = self.one_pdf;
                            match plan_set_inputs(
                                model,
                                case,
                                drawings,
                                &self.others,
                                plugin,
                                set,
                                &formats,
                                one_pdf,
                                out,
                            ) {
                                Ok(jobs) => {
                                    self.error = None;
                                    let set_name = set.name.clone();
                                    self.job = Some(Job::start(
                                        format!("Exporting the plan set {name}"),
                                        false,
                                        ctx,
                                        move |report| {
                                            run_plan_set(
                                                &jobs,
                                                one_pdf.then_some(set_name.as_str()),
                                                report,
                                            )
                                        },
                                    ));
                                }
                                Err(e) => self.error = Some(e),
                            }
                        }
                        Some((_, _, Err(e))) => self.error = Some(e.clone()),
                        None => self.error = Some("no plan set chosen".into()),
                    }
                }
                Some(out) => {
                    let mut w = drawings.clone();
                    // Alignments drawn together: every route read (the
                    // Profile sheets window's one route aside).
                    if self.kind == PlanKind::Phase
                        && self.groups.iter().any(|g| !g.alignments.is_empty())
                    {
                        w.route = None;
                    }
                    w.formats = self.formats.clone();
                    // Its own template and values.
                    w.sheet.template = self.sheet.template.clone();
                    w.sheet.fields = self.sheet.fields.clone();
                    let req = self.request();
                    match Input::for_plans(model, case, &w, out, &req) {
                        Ok(input) => {
                            self.error = None;
                            self.job = Some(Job::start(
                                format!("Exporting the {}", req.kind.title().to_lowercase()),
                                false,
                                ctx,
                                move |report| run_plans(&input, &req, report),
                            ));
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
        }
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The route plan, situation plans and phase plan windows on the example line: the route plan, the situation
    /// plans and the phase plan written as PDF for every route, with the
    /// Profile sheets window's settings.
    #[test]
    fn the_plans_are_written() {
        let model = crate::tests::synthetic();
        let w = crate::drawings::DrawingsWindow::default();
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-plans", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for kind in [PlanKind::Route, PlanKind::Situation, PlanKind::Phase] {
            let req = PlansWindow::of(kind).request();
            let mut ww = w.clone();
            ww.formats = vec![ohl_drawing::export::Format::Pdf];
            let input = Input::for_plans(&model, 0, &ww, dir.clone(), &req).unwrap();
            let done = run_plans(&input, &req, &|_, _| {}).unwrap();
            assert!(done.contains("files written"), "{done}");
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        for what in ["route-plan.pdf", "situation-plan.pdf", "phase-plan.pdf"] {
            assert!(names.iter().any(|n| n.ends_with(what)), "{what}: {names:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Owner, 2026-11-03: routes and alignments chosen into one phase
    /// plan — one plan of them, named after it; the alignments in no
    /// plan each their own, as before.
    #[test]
    fn alignments_drawn_together_in_one_phase_plan() {
        let model = crate::tests::synthetic();
        let views = model.line.alignment_views();
        let w = crate::drawings::DrawingsWindow {
            formats: vec![ohl_drawing::export::Format::Svg],
            ..Default::default()
        };
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-phase-groups", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut win = PlansWindow::of(PlanKind::Phase);
        win.groups = vec![PhaseGroup {
            name: "North and south".into(),
            alignments: views.iter().map(|v| v.id.clone()).collect(),
        }];
        let req = win.request();
        assert_eq!(req.groups.len(), 1);
        let input = Input::for_plans(&model, 0, &w, dir.clone(), &req).unwrap();
        let done = run_plans(&input, &req, &|_, _| {}).unwrap();
        assert!(done.contains("files written"), "{done}");
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .all(|n| n.starts_with("North-and-south-phase-plan")),
            "{names:?}"
        );
        assert!(!names.is_empty());
        // Every alignment's first structure is in it.
        let svg: String = names
            .iter()
            .map(|n| std::fs::read_to_string(dir.join(n)).unwrap())
            .collect();
        for v in &views {
            if let Some(s) = v.structures.first() {
                assert!(svg.contains(s.as_str()), "{s}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Owner, 2026-10-25: the plans asked for in a weather case not solved
    /// yet wait for it (the window asks for it), but the phase plan, a
    /// schematic, takes the case shown and never waits.
    #[test]
    fn the_phase_plan_never_waits_for_a_case() {
        let mut model = crate::tests::synthetic();
        assert!(model.cases.len() > 1);
        let k = model.cases.len() - 1;
        model.cases[k] = crate::model::Solved::pending(model.cases[k].case.clone());
        let mut w = crate::drawings::DrawingsWindow {
            formats: vec![ohl_drawing::export::Format::Pdf],
            ..Default::default()
        };
        let mut keys: Vec<String> = model
            .line
            .sections
            .iter()
            .map(crate::drawings::circuit_key)
            .collect();
        keys.dedup();
        for key in keys {
            w.sheet.circuit_cases.insert(
                key,
                crate::sheet_ui::CircuitCases {
                    drawn: Some(model.cases[k].case.name.clone()),
                    stringing: None,
                },
            );
        }
        let dir = std::env::temp_dir();
        let win = PlansWindow::of(PlanKind::Situation);
        assert_eq!(win.wanted(&model, &w), vec![k]);
        let e = Input::for_plans(&model, 0, &w, dir.clone(), &win.request()).err();
        assert!(e.is_some_and(|e| e.contains("being solved")));
        let win = PlansWindow::of(PlanKind::Phase);
        assert!(win.wanted(&model, &w).is_empty());
        assert!(Input::for_plans(&model, 0, &w, dir, &win.request()).is_ok());
    }

    /// Owner, 2026-11-08: each plan has its own template, apart from the
    /// profile sheets'. In a plan set, a plan's drawing takes its plan
    /// window's template (here one that does not read, so it is seen to
    /// be taken), the profile sheets the sheets window's.
    #[test]
    fn a_plan_takes_its_own_template() {
        let model = crate::tests::synthetic();
        let w = crate::drawings::DrawingsWindow::default();
        let set = |kind: &str| {
            PlanSet::parse(&format!(
                "format = \"ohl-plan-set\"\nversion = 1\nname = \"S\"\n\
                 [[drawing]]\nkind = \"{kind}\"\npage = \"A3\"\n"
            ))
            .unwrap()
        };
        let own = vec![(
            PlanKind::Route,
            PlanSheet {
                template: "library:no-such-template".into(),
                ..Default::default()
            },
        )];
        let dir = std::env::temp_dir();
        let run = |kind: &str, plans: &[(PlanKind, PlanSheet)]| {
            crate::drawings::plan_set_inputs(
                &model,
                0,
                &w,
                plans,
                "",
                &set(kind),
                &[Format::Pdf],
                false,
                dir.clone(),
            )
            .map(|_| ())
        };
        assert!(run("route", &[]).is_ok());
        let e = run("route", &own).unwrap_err();
        assert!(e.contains("no-such-template"), "{e}");
        // The other drawings keep theirs.
        assert!(run("situation", &own).is_ok());
        assert!(run("sheets", &own).is_ok());
        // The windows start apart, each on the built-in template.
        let mut plans = PlanWindows::default();
        plans.route.sheet.template = "library:x".into();
        assert_eq!(plans.situation.sheet.template, "default");
        assert_eq!(plans.sheets().len(), 3);
    }

    /// A plan set written in one go: the sheets and the phase plan of
    /// the example line, each with the set's paper, and all their sheets
    /// in one PDF; and the LeV plugin's set (when this checkout has it)
    /// reads.
    #[test]
    fn a_plan_set_is_written() {
        let model = crate::tests::synthetic();
        let w = crate::drawings::DrawingsWindow::default();
        let set = PlanSet::parse(
            "format = \"ohl-plan-set\"\nversion = 1\nname = \"S\"\n\
             [[drawing]]\nkind = \"sheets\"\npage = \"A3\"\n\
             [[drawing]]\nkind = \"phase\"\npage = \"A4\"\n\
             [[drawing]]\nkind = \"phase\"\npage = \"A3\"\n",
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-plan-set", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let jobs = crate::drawings::plan_set_inputs(
            &model,
            0,
            &w,
            &[],
            "",
            &set,
            &[ohl_drawing::export::Format::Pdf],
            true,
            dir.clone(),
        )
        .unwrap();
        assert_eq!(jobs.len(), 3);
        let done = run_plan_set(&jobs, Some("S"), &|_, _| {}).unwrap();
        assert!(done.contains("one PDF"), "{done}");
        assert!(done.contains("phase plan"), "{done}");
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().any(|n| n.ends_with("phase-plan.pdf")),
            "{names:?}"
        );
        assert!(
            names.iter().any(|n| n.ends_with("phase-plan-2.pdf")),
            "the second phase plan named apart: {names:?}"
        );
        // The one PDF: as many pages as the set's PDFs together.
        let count = |n: &str| {
            ohl_drawing::pdf::read_back(&std::fs::read(dir.join(n)).unwrap())
                .unwrap()
                .len()
        };
        let parts: usize = names
            .iter()
            .filter(|n| n.ends_with(".pdf") && n.as_str() != "S.pdf")
            .map(|n| count(n))
            .sum();
        assert!(names.iter().any(|n| n == "S.pdf"), "{names:?}");
        assert!(parts >= 3, "{names:?}");
        assert_eq!(count("S.pdf"), parts);
        let _ = std::fs::remove_dir_all(&dir);
        // The private design-code plugin, when this checkout has it.
        if let Some(lev) = ohl_core::env::private_plugin(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        ) {
            let name = ohl_plugins::Manifest::load(&lev)
                .map(|m| m.plugin.name)
                .unwrap_or_default();
            let sets = crate::drawings::plugin_plan_sets(&[(name, lev.clone())]);
            assert!(!sets.is_empty());
            for (_, n, s) in &sets {
                assert!(s.is_ok(), "{n}: {:?}", s.as_ref().err());
            }
            let t = ohl_drawing::template::SheetTemplate::load(
                &lev.join("sheets/esti-235-a1.ohlsheet"),
            );
            assert!(t.is_ok(), "{:?}", t.err());
        }
    }
}
