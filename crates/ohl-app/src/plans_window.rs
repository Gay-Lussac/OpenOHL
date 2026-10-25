// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Export › Plans (owner, 2026-10-24: a menu of their own, apart from the
//! plan-and-profile sheets): the **route plan** and the **situation
//! plans** (north up, over the project's drawings and orthophotos) and the
//! **phase plan** (the structures where the phase arrangement changes),
//! written for every route of the Drawings window with its sheet template
//! and title fields ([`crate::drawings::run_plans`]).

use std::path::PathBuf;

use eframe::egui;

use crate::drawings::{Input, PlanKind, PlanRequest, plan_set_inputs, run_plan_set, run_plans};
use crate::model::Model;
use crate::survey_io::Job;
use ohl_drawing::plan_set::PlanSet;

/// The papers offered.
const PAPERS: [&str; 5] = ["A0", "A1", "A2", "A3", "A4"];

/// The Plans window.
#[derive(Debug)]
pub struct PlansWindow {
    /// Shown.
    pub open: bool,
    /// The plan.
    pub kind: PlanKind,
    /// Paper.
    pub page: String,
    /// The route plan's scale (1:…).
    pub route_scale: f64,
    /// The situation plans' scale (1:…).
    pub situation_scale: f64,
    /// The project's drawings under the route and situation plans.
    pub drawings: bool,
    /// The orthophotos under them.
    pub photo: bool,
    /// The corridor drawn (at the drawings' line width).
    pub corridor: bool,
    /// Formats: `pdf`, `svg`, `png`, `dxf`.
    pub formats: Vec<String>,
    /// Output folder.
    pub out: Option<PathBuf>,
    /// Why the last start failed.
    pub error: Option<String>,
    /// The export under way, or done.
    pub job: Option<Job>,
    /// A plan set exported instead of one plan.
    pub set_mode: bool,
    /// The plugins' plan sets: plugin, name, the set or why it does not
    /// read.
    pub sets: Vec<(String, String, Result<PlanSet, String>)>,
    /// The set chosen (index in `sets`).
    pub set: usize,
    /// Export asked while its weather cases were being solved: started
    /// once they are.
    pub waiting: bool,
}

impl Default for PlansWindow {
    fn default() -> Self {
        Self {
            open: false,
            kind: PlanKind::Situation,
            page: "A3".into(),
            route_scale: 10_000.0,
            situation_scale: 1000.0,
            drawings: true,
            photo: true,
            corridor: true,
            formats: vec!["pdf".into()],
            out: None,
            error: None,
            job: None,
            set_mode: false,
            sets: Vec::new(),
            set: 0,
            waiting: false,
        }
    }
}

impl PlansWindow {
    /// Opens the window on plan `kind`.
    pub fn show(&mut self, kind: PlanKind) {
        self.kind = kind;
        self.set_mode = false;
        self.open = true;
    }

    /// Opens the window on the plan sets of the plugins `dirs` (name,
    /// folder), read now.
    pub fn show_sets(&mut self, dirs: &[(String, PathBuf)]) {
        self.sets = crate::drawings::plugin_plan_sets(dirs);
        self.set = self.set.min(self.sets.len().saturating_sub(1));
        self.set_mode = true;
        self.open = true;
    }

    /// The request of the window's settings.
    pub fn request(&self) -> PlanRequest {
        PlanRequest {
            kind: self.kind,
            page: self.page.clone(),
            scale: match self.kind {
                PlanKind::Route => self.route_scale,
                _ => self.situation_scale,
            },
            drawings: self.drawings,
            photo: self.photo,
            corridor: self.corridor,
        }
    }

    /// The weather cases the plans asked for draw in (their templates'
    /// and the Drawings window's per circuit): solved while the window is
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
                        || drawings.sheet.template.clone(),
                        |t| crate::drawings::set_template_key(drawings, plugin, t),
                    );
                    v.extend(needed(&key));
                }
            }
        } else if self.kind != PlanKind::Phase {
            v = needed(&drawings.sheet.template);
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The window; `drawings` the Drawings window's settings (routes,
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
        egui::Window::new("Plans")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Plans beside the plan-and-profile sheets, for every route, with the \
                         sheet template, title fields and routes of Export › Drawings….",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    for k in [PlanKind::Route, PlanKind::Situation, PlanKind::Phase] {
                        if ui
                            .selectable_label(!self.set_mode && self.kind == k, k.title())
                            .clicked()
                        {
                            self.kind = k;
                            self.set_mode = false;
                        }
                    }
                    if ui
                        .selectable_label(self.set_mode, "Plan set")
                        .on_hover_text(
                            "Several drawings in one go, as a plugin's plan set lists them",
                        )
                        .clicked()
                    {
                        self.set_mode = true;
                    }
                });
                if self.set_mode {
                    if self.sets.is_empty() {
                        ui.label("No plugin in use gives a plan set.");
                    }
                    let shown = self
                        .sets
                        .get(self.set)
                        .map_or("—".to_owned(), |(p, n, _)| format!("{p} / {n}"));
                    egui::ComboBox::from_id_salt("plans-set")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (k, (p, n, _)) in self.sets.iter().enumerate() {
                                ui.selectable_value(&mut self.set, k, format!("{p} / {n}"));
                            }
                        });
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
                }
                ui.label(
                    egui::RichText::new(match self.kind {
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
                    })
                    .small(),
                );
                egui::Grid::new("plans-settings")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        if !self.set_mode {
                            ui.label("Paper");
                            egui::ComboBox::from_id_salt("plans-paper")
                                .selected_text(self.page.as_str())
                                .show_ui(ui, |ui| {
                                    for p in PAPERS {
                                        ui.selectable_value(&mut self.page, p.to_owned(), p);
                                    }
                                });
                            ui.end_row();
                        }
                        if !self.set_mode && self.kind != PlanKind::Phase {
                            ui.label("Scale 1:");
                            let (scale, range) = if self.kind == PlanKind::Route {
                                (&mut self.route_scale, 5000.0..=25_000.0)
                            } else {
                                (&mut self.situation_scale, 500.0..=2000.0)
                            };
                            ui.add(egui::DragValue::new(scale).range(range).speed(50.0));
                            ui.end_row();
                            ui.label("Under it");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.drawings, "the project's drawings");
                                ui.checkbox(&mut self.photo, "orthophotos");
                                ui.checkbox(&mut self.corridor, "the corridor");
                            });
                            ui.end_row();
                        }
                        ui.label("Formats");
                        ui.horizontal(|ui| {
                            for f in ["pdf", "svg", "png", "dxf"] {
                                let mut on = self.formats.iter().any(|x| x == f);
                                if ui.checkbox(&mut on, f.to_uppercase()).changed() {
                                    if on {
                                        self.formats.push(f.into());
                                    } else {
                                        self.formats.retain(|x| x != f);
                                    }
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Folder");
                        ui.horizontal(|ui| {
                            if ui.button("Choose…").clicked()
                                && let Some(p) = rfd::FileDialog::new().pick_folder()
                            {
                                self.out = Some(p);
                            }
                            let shown = self
                                .out
                                .as_ref()
                                .or(drawings.out.as_ref())
                                .map_or("—".to_owned(), |p| p.display().to_string());
                            ui.label(egui::RichText::new(shown).small());
                        });
                        ui.end_row();
                    });
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                start = ui
                    .add_enabled(self.job.is_none(), egui::Button::new("Export"))
                    .clicked();
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
                Some(out) if self.formats.is_empty() => {
                    let _ = out;
                    self.error = Some("tick a format".into());
                }
                Some(out) if self.set_mode => {
                    let formats: Vec<ohl_drawing::export::Format> = self
                        .formats
                        .iter()
                        .filter_map(|f| ohl_drawing::export::Format::parse(f))
                        .collect();
                    match self.sets.get(self.set) {
                        Some((plugin, name, Ok(set))) => {
                            match plan_set_inputs(model, case, drawings, plugin, set, &formats, out)
                            {
                                Ok(jobs) => {
                                    self.error = None;
                                    self.job = Some(Job::start(
                                        format!("Exporting the plan set {name}"),
                                        false,
                                        ctx,
                                        move |report| run_plan_set(&jobs, report),
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
                    w.formats = self
                        .formats
                        .iter()
                        .filter_map(|f| ohl_drawing::export::Format::parse(f))
                        .collect();
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

    /// Export › Plans on the example line: the route plan, the situation
    /// plans and the phase plan written as PDF for every route, with the
    /// Drawings window's settings.
    #[test]
    fn the_plans_are_written() {
        let model = crate::tests::synthetic();
        let w = crate::drawings::DrawingsWindow::default();
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-plans", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for kind in [PlanKind::Route, PlanKind::Situation, PlanKind::Phase] {
            let mut win = PlansWindow::default();
            win.show(kind);
            let req = win.request();
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
        let mut win = PlansWindow::default();
        win.show(PlanKind::Situation);
        assert_eq!(win.wanted(&model, &w), vec![k]);
        let e = Input::for_plans(&model, 0, &w, dir.clone(), &win.request()).err();
        assert!(e.is_some_and(|e| e.contains("being solved")));
        win.show(PlanKind::Phase);
        assert!(win.wanted(&model, &w).is_empty());
        assert!(Input::for_plans(&model, 0, &w, dir, &win.request()).is_ok());
    }

    /// A plan set written in one go: the sheets and the phase plan of
    /// the example line, each with the set's paper; and the LeV plugin's
    /// set (when this checkout has it) reads.
    #[test]
    fn a_plan_set_is_written() {
        let model = crate::tests::synthetic();
        let w = crate::drawings::DrawingsWindow::default();
        let set = PlanSet::parse(
            "format = \"ohl-plan-set\"\nversion = 1\nname = \"S\"\n\
             [[drawing]]\nkind = \"sheets\"\npage = \"A3\"\n\
             [[drawing]]\nkind = \"phase\"\npage = \"A4\"\n",
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!("ohl-app-{}-plan-set", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let jobs = crate::drawings::plan_set_inputs(
            &model,
            0,
            &w,
            "",
            &set,
            &[ohl_drawing::export::Format::Pdf],
            dir.clone(),
        )
        .unwrap();
        assert_eq!(jobs.len(), 2);
        let done = run_plan_set(&jobs, &|_, _| {}).unwrap();
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
            names.iter().filter(|n| n.ends_with(".pdf")).count() >= 2,
            "{names:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let lev = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../local-data/plugins/lev-ch");
        if lev.is_dir() {
            let sets = crate::drawings::plugin_plan_sets(&[("lev-ch".into(), lev.clone())]);
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
