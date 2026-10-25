// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The FE results panel (Results menu): the cases of the load table
//! solved on the structure standing on the extension chosen (the
//! finite-element model of the line for a placed structure on that
//! extension, and the line's pole check for a structure with a pole —
//! through `ohl_structure_link::analyse_structure`, on a thread of its
//! own, each case under a time budget), and per case the largest
//! displacement, Σ loads against Σ reactions, the reactions, the members'
//! forces and usage and the governing member, the pole check; over the
//! cases the usage per check and the case governing it; in the view the
//! deformed shape and the members coloured by usage. Choosing another
//! extension solves again. A line case whose values lack a wire the line
//! did not solve is marked incomplete ("not checked — incomplete: S12 not
//! solved"), its usage only that of the loads there are. **Structure
//! report…** writes the run as a printable HTML page and the members'
//! results as CSV (`ohl_structure_export::report`).

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_core::budget::{Budget, CancelToken};
use ohl_render::Scene;
use ohl_structure_link::{
    CaseResult, PoleResult, RunInput, StructureRun, analyse_structure, cases_to_run,
};
use ohl_structure_loads::{LoadCaseRecipe, LoadTable};
use ohl_structures::Detail;

use crate::app::{StructureApp, error_label};
use crate::load_cases_tab::arrow;

/// Most iterations of one case.
const ITERATIONS: u64 = 200;
/// Longest time one case may take (a solve takes milliseconds).
const CASE_TIME: Duration = Duration::from_secs(30);
/// Usage from which a member is near full (as the line application's
/// Usage page).
const NEAR_FULL: f64 = 0.9;

/// The answer of a run: the cases solved, or why the run could not
/// start (the model does not read, an extension not there).
pub type Run = Result<StructureRun, String>;

/// The state of the FE results panel.
pub struct ResultsTab {
    /// The case to run (empty: every case).
    pub case: String,
    /// The last answer.
    pub results: Option<Run>,
    /// The load table's key it was computed from.
    pub ran_on: u64,
    /// The extension of the last run started (none: as written).
    pub ran_ext: Option<String>,
    /// The table and the recipes of the last run (for the report).
    ran_with: (LoadTable, Vec<LoadCaseRecipe>),
    /// How long the last run took.
    pub took: Option<Duration>,
    /// The case shown (its place in the results).
    pub shown: usize,
    /// The deformed shape drawn.
    pub deformed: bool,
    /// Its scale.
    pub scale: f64,
    /// Reactions drawn.
    pub reactions: bool,
    running: Option<(Instant, Receiver<Run>, CancelToken)>,
}

impl Default for ResultsTab {
    fn default() -> Self {
        Self {
            case: String::new(),
            results: None,
            ran_on: 0,
            ran_ext: None,
            ran_with: Default::default(),
            took: None,
            shown: 0,
            deformed: true,
            scale: 50.0,
            reactions: true,
            running: None,
        }
    }
}

/// Usage colour: red above 100 %, orange from 90 %, green below, grey
/// without a capacity.
pub fn usage_colour(usage: Option<f64>) -> [u8; 4] {
    match usage {
        Some(u) if u > 1.0 => [235, 70, 60, 255],
        Some(u) if u >= NEAR_FULL => [255, 170, 60, 255],
        Some(_) => [110, 220, 120, 255],
        None => [150, 150, 160, 255],
    }
}

fn rgb(c: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

fn kn(v: [f64; 3]) -> String {
    format!(
        "{:.2}; {:.2}; {:.2}",
        v[0] / 1000.0,
        v[1] / 1000.0,
        v[2] / 1000.0
    )
}

fn pct(u: Option<f64>) -> String {
    match u {
        Some(u) if u.is_finite() => format!("{:.0} %", u * 100.0),
        Some(_) => "∞".into(),
        None => "—".into(),
    }
}

/// The extension's name in words.
fn extension_text(e: Option<&str>) -> String {
    e.map_or("as written".into(), |e| format!("on extension {e}"))
}

impl ResultsTab {
    /// A run is going on.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Starts solving on its own thread (unless one runs).
    fn start(
        &mut self,
        ctx: &egui::Context,
        key: u64,
        extension: Option<String>,
        work: impl FnOnce(&Budget) -> Run + Send + 'static,
    ) {
        if self.running.is_some() {
            return;
        }
        let cancel = CancelToken::new();
        let budget = Budget::new()
            .max_iterations(ITERATIONS)
            .time_limit(CASE_TIME)
            .cancel_token(cancel.clone());
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work(&budget));
            ctx.request_repaint();
        });
        self.running = Some((Instant::now(), rx, cancel));
        self.ran_on = key;
        self.ran_ext = extension;
    }

    /// Takes the answer of a run that ended.
    pub fn poll(&mut self) {
        let Some((t, rx, _)) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the computation stopped".into()),
        };
        self.took = Some(t.elapsed());
        self.running = None;
        self.shown = 0;
        // A scale that shows the largest displacement as about 1 m.
        if let Ok(run) = &r
            && let Some(d) = run
                .cases
                .iter()
                .filter_map(|c| c.fe.as_ref()?.as_ref().ok()?.largest_displacement())
                .map(|d| d.1)
                .reduce(f64::max)
            && d > 1e-9
        {
            let s = 1.0 / d;
            let p = 10f64.powf(s.log10().floor());
            self.scale = (s / p).floor().max(1.0) * p;
        }
        self.results = Some(r);
    }

    /// The run, solved.
    pub fn run(&self) -> Option<&StructureRun> {
        self.results.as_ref()?.as_ref().ok()
    }

    /// The finite-element results of the case shown.
    pub fn shown(&self) -> Option<&CaseResult> {
        self.run()?
            .cases
            .get(self.shown)?
            .fe
            .as_ref()?
            .as_ref()
            .ok()
    }

    /// The deformed shape coloured by usage, and the reactions, of the
    /// case shown. Its key changes with what it shows.
    pub fn overlay(&self, drawn: Option<u64>) -> Option<(u64, Option<Scene>)> {
        let r = self.shown()?;
        let mut h = DefaultHasher::new();
        (
            self.ran_on,
            &self.ran_ext,
            self.shown,
            self.deformed,
            self.scale.to_bits(),
            self.reactions,
            self.took,
        )
            .hash(&mut h);
        let key = h.finish();
        if drawn == Some(key) {
            return Some((key, None));
        }
        let scale = if self.deformed { self.scale } else { 0.0 };
        let at = |k: usize| {
            let j = &r.joints[k];
            [0, 1, 2].map(|i| j.point[i] + j.displacement[i] * scale)
        };
        let mut s = Scene::new([0.0; 3]);
        for m in &r.members {
            s.polyline(&[at(m.ends[0]), at(m.ends[1])], usage_colour(m.usage), 3.0);
        }
        if self.reactions {
            let largest = r
                .reactions
                .iter()
                .map(|x| x.force.iter().map(|f| f * f).sum::<f64>().sqrt())
                .fold(0.0, f64::max);
            if largest > 0.0 {
                for x in &r.reactions {
                    if let Some(j) = r.joints.iter().find(|j| j.id == x.joint) {
                        // The largest 4 m long, from the foot down into the ground.
                        let v = x.force.map(|f| -f / largest * 4.0);
                        arrow(&mut s, j.point, v, [90, 200, 255, 255], false);
                    }
                }
            }
        }
        Some((key, Some(s)))
    }
}

impl StructureApp {
    /// A run can start: level 3, a case in the load table, none running.
    pub fn can_run_results(&mut self) -> bool {
        let Some(f) = &self.file else {
            return false;
        };
        if f.structure().detail() != Detail::Design || self.results.running() {
            return false;
        }
        self.loads.refresh(f);
        !self.loads.table().cases().is_empty()
    }

    /// The extension the structure stands on (the one the 3D view shows,
    /// when the structure has it; none: as written).
    pub fn results_extension(&self) -> Option<String> {
        let t = self.file.as_ref()?.structure();
        self.ext.shown.clone().filter(|n| t.extension(n).is_some())
    }

    /// Results › Run (and ▶ Run): solves the case chosen (or every case) of
    /// the load table on the extension chosen, on its own thread; false
    /// when it cannot start.
    pub fn run_results(&mut self, ctx: &egui::Context) -> bool {
        if !self.can_run_results() {
            return false;
        }
        let extension = self.results_extension();
        let Some(f) = &self.file else {
            return false;
        };
        let structure = f.structure().clone();
        let table = self.loads.table().clone();
        // The recipes and the line cases' wind on the body, the line's
        // pressures written in (surface loads on the extension's piece, the
        // wind on a pole).
        let recipes: Vec<LoadCaseRecipe> = self.loads.run_recipes().to_vec();
        let key = self.loads.key();
        let only = (!self.results.case.is_empty()).then_some(self.results.case.as_str());
        let run = cases_to_run(&table, &recipes, only);
        let text = f.text();
        let dir = f.path().and_then(Path::parent).map(Path::to_path_buf);
        let sections = self.loads.sections.clone();
        self.results.ran_with = (table.clone(), recipes.clone());
        let ext = extension.clone();
        self.results.start(ctx, key, extension, move |budget| {
            analyse_structure(
                &RunInput {
                    structure: &structure,
                    text: &text,
                    dir: dir.as_deref(),
                    sections: &sections,
                    table: &table,
                    recipes: &recipes,
                    cases: &run,
                    extension: ext.as_deref(),
                },
                budget,
            )
        });
        true
    }

    /// Results › Cancel: stops the run going on.
    pub fn cancel_results(&mut self) {
        if let Some((_, _, cancel)) = &self.results.running {
            cancel.cancel();
        }
    }

    /// A report can be written: a run solved, of the file and table as
    /// they are.
    pub fn can_write_report(&self) -> bool {
        self.results.run().is_some()
            && !self.results.running()
            && self.results.ran_on == self.loads.key()
    }

    /// Results › Structure report…: asks where, then writes the report.
    pub(crate) fn report_dialog(&mut self) {
        let stem = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(Path::file_stem)
            .map_or("structure".into(), |n| n.to_string_lossy().into_owned());
        if let Some(out) = rfd::FileDialog::new()
            .add_filter("HTML", &["html"])
            .set_file_name(format!("{stem}-report.html"))
            .save_file()
        {
            self.message = Some(match self.write_report(&out.with_extension("html")) {
                Ok(m) => (m, false),
                Err(e) => (format!("report: {e}"), true),
            });
        }
    }

    /// Writes the report of the last run to `out` (HTML) and the members'
    /// results beside it (`<stem>.members.csv`); what was written, or why
    /// not.
    pub fn write_report(&mut self, out: &Path) -> Result<String, String> {
        let f = self.file.as_ref().ok_or("no structure open")?;
        if !self.can_write_report() {
            return Err(
                "no results of the file and the load table as they are: run the cases first".into(),
            );
        }
        let run = self.results.run().ok_or("no results")?;
        let (table, recipes) = &self.results.ran_with;
        let incomplete: Vec<(String, String)> = run
            .cases
            .iter()
            .filter_map(|c| Some((c.name.clone(), self.loads.incomplete(&c.name)?)))
            .collect();
        let file = f.path().map_or(String::new(), |p| p.display().to_string());
        let sections_from = self
            .loads
            .sections_note
            .as_ref()
            .map_or("none".to_owned(), |(n, _)| n.clone());
        let software = format!("{} {}", crate::APP_NAME, crate::VERSION);
        let date = ohl_structure_export::report::today();
        let html =
            ohl_structure_export::report::report_html(&ohl_structure_export::report::ReportInput {
                structure: f.structure(),
                file: &file,
                sections_from: &sections_from,
                table,
                recipes,
                run,
                incomplete: &incomplete,
                software: &software,
                date: Some(&date),
            });
        let csv_path = out.with_extension("members.csv");
        let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
        std::fs::write(out, html).map_err(|e| io(out, e))?;
        std::fs::write(&csv_path, ohl_structure_export::report::members_csv(run))
            .map_err(|e| io(&csv_path, e))?;
        Ok(format!(
            "report written: {}, {}",
            out.display(),
            csv_path.display()
        ))
    }

    /// The FE results panel.
    pub fn results_ui(&mut self, ui: &mut egui::Ui) {
        self.results.poll();
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure().clone();
        if t.detail() != Detail::Design {
            ui.label(
                "Results need level 3: every member with a cross-section and a supported joint (or a \
                 model = \"….ohlfe\" file), or a pole the pole check can read. What is still \
                 missing is listed in the header.",
            );
            return;
        }
        self.loads.refresh(f);
        let table = self.loads.table().clone();
        let key = self.loads.key();
        let cases: Vec<String> = table.cases().into_iter().map(str::to_owned).collect();
        if !self.results.case.is_empty() && !cases.contains(&self.results.case) {
            self.results.case.clear();
        }
        let ctx = ui.ctx().clone();
        let mut run_now = false;
        let mut report = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Case:");
            let text = if self.results.case.is_empty() {
                "every case".to_owned()
            } else {
                self.results.case.clone()
            };
            egui::ComboBox::from_id_salt("results-case")
                .selected_text(text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.results.case, String::new(), "every case");
                    for c in &cases {
                        ui.selectable_value(&mut self.results.case, c.clone(), c);
                    }
                });
            if !t.extensions.is_empty() {
                ui.label("Extension:").on_hover_text(
                    "The structure solved standing on this extension (its piece and legs, as \
                     the line's model of a placed structure); the 3D view shows the same. \
                     Choosing another solves again.",
                );
                let mut chosen = self.ext.shown.clone().filter(|n| t.extension(n).is_some());
                egui::ComboBox::from_id_salt("results-extension")
                    .selected_text(chosen.as_deref().unwrap_or("as written"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, None, "as written");
                        for e in &t.extensions {
                            ui.selectable_value(
                                &mut chosen,
                                Some(e.name.clone()),
                                format!("{} (+{} m)", e.name, e.dz.si()),
                            );
                        }
                    });
                self.ext.shown = chosen;
            }
            let running = self.results.running();
            if ui
                .add_enabled(!running && !cases.is_empty(), egui::Button::new("▶ Run"))
                .on_hover_text(format!(
                    "Solve with the finite-element model and the pole check (each case at most \
                     {} s and {ITERATIONS} iterations)",
                    CASE_TIME.as_secs()
                ))
                .on_disabled_hover_text("No case in the load table, or a run is going on")
                .clicked()
            {
                run_now = true;
            }
            if let Some((started, _, cancel)) = &self.results.running {
                ui.spinner();
                ui.label(format!("solving… {:.1} s", started.elapsed().as_secs_f64()));
                if ui.button("Cancel").clicked() {
                    cancel.cancel();
                }
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            } else if let Some(took) = self.results.took {
                ui.label(
                    egui::RichText::new(format!("solved in {:.0} ms", took.as_secs_f64() * 1e3))
                        .weak(),
                );
            }
            report = ui
                .add_enabled(self.can_write_report(), egui::Button::new("Report…"))
                .on_hover_text(
                    "The structure report of these results: a printable HTML page and the \
                     members' results as CSV",
                )
                .on_disabled_hover_text("Run the cases first (results of the file as it is)")
                .clicked();
        });
        // Another extension chosen: solve again.
        if self.results.results.is_some()
            && !self.results.running()
            && self.results.ran_ext != self.results_extension()
        {
            run_now = true;
        }
        if run_now {
            self.run_results(&ctx);
        }
        if report {
            self.report_dialog();
        }
        let stale = self.results.results.is_some() && self.results.ran_on != key;
        if stale {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "The file or the load table changed since: run again for results of them.",
            );
        }
        let run = match &self.results.results {
            None => {
                ui.label(
                    "Run solves the cases of the load table (Load cases › Load table) on the \
                     structure's finite-element model, and with the pole check for a pole: a \
                     value typed there counts, rows without a value are left out. Each case runs \
                     under a time budget; Cancel stops it.",
                );
                return;
            }
            Some(Err(e)) => {
                error_label(ui, format!("not solved: {e}"));
                return;
            }
            Some(Ok(v)) => v.clone(),
        };
        ui.strong(format!(
            "Solved {}{}",
            extension_text(run.extension.as_deref()),
            if run.extension.is_some() {
                format!(" (the structure {} m higher)", run.dz)
            } else {
                String::new()
            }
        ));
        for n in &run.notes {
            ui.colored_label(ui.visuals().warn_fg_color, n);
        }
        // Per check: the largest usage and its case.
        egui::Grid::new("results-checks")
            .striped(true)
            .show(ui, |ui| {
                for h in ["check", "usage", "governing case", "what governs"] {
                    ui.strong(h);
                }
                ui.end_row();
                for s in run.checks() {
                    ui.label(if s.check == "members" {
                        "members (FE)"
                    } else {
                        "pole"
                    });
                    ui.colored_label(rgb(usage_colour(s.usage)), pct(s.usage));
                    ui.label(&s.case);
                    ui.label(&s.governs);
                    ui.end_row();
                    if !s.failed.is_empty() {
                        ui.label("");
                        ui.colored_label(ui.visuals().error_fg_color, "not solved");
                        ui.label(
                            s.failed
                                .iter()
                                .map(|(c, _)| c.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                        );
                        ui.label("");
                        ui.end_row();
                    }
                }
            });
        ui.horizontal_wrapped(|ui| {
            ui.label("Shown:");
            let names: Vec<&str> = run.cases.iter().map(|c| c.name.as_str()).collect();
            let k = self.results.shown.min(names.len().saturating_sub(1));
            egui::ComboBox::from_id_salt("results-shown")
                .selected_text(names.get(k).copied().unwrap_or(""))
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.results.shown, i, *n);
                    }
                });
            ui.checkbox(&mut self.results.deformed, "Deformed ×");
            ui.add(
                egui::DragValue::new(&mut self.results.scale)
                    .range(1.0..=100_000.0)
                    .speed(1.0),
            )
            .on_hover_text("The displacements drawn this many times larger");
            ui.checkbox(&mut self.results.reactions, "Reactions")
                .on_hover_text(
                    "Drawn as the loads on the foundations (arrows into the ground), the largest 4 m",
                );
        });
        // Every case at a glance: its usage over the checks, or why not
        // solved.
        egui::ScrollArea::vertical()
            .id_salt("results-cases")
            .max_height(70.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (k, c) in run.cases.iter().enumerate() {
                    let incomplete = self.loads.incomplete(&c.name);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.results.shown == k, c.name.as_str())
                            .clicked()
                        {
                            self.results.shown = k;
                        }
                        if let Some(text) = &incomplete {
                            // The usage of loads lacking a wire: not a pass.
                            ui.colored_label(
                                crate::load_cases_tab::INCOMPLETE,
                                format!("not checked — {text}"),
                            );
                        }
                        let mut parts = Vec::new();
                        match &c.fe {
                            Some(Ok(r)) => parts.push(match r.governing() {
                                Some(g) => format!("{} — member {}", pct(g.usage), g.id),
                                None => "no member has a capacity".into(),
                            }),
                            Some(Err(e)) => parts.push(format!("members: {e}")),
                            None => {}
                        }
                        match &c.pole {
                            Some(Ok(p)) => parts.push(format!("pole {}", pct(Some(p.usage)))),
                            Some(Err(e)) => parts.push(format!("pole: {e}")),
                            None => {}
                        }
                        let text = parts.join(" · ");
                        if incomplete.is_some() {
                            ui.weak(format!("({text} of the loads there are)"));
                        } else {
                            ui.colored_label(rgb(usage_colour(c.usage())), text);
                        }
                    });
                }
            });
        ui.separator();
        let Some(c) = run
            .cases
            .get(self.results.shown.min(run.cases.len().saturating_sub(1)))
        else {
            return;
        };
        if let Some(text) = self.loads.incomplete(&c.name) {
            ui.colored_label(
                crate::load_cases_tab::INCOMPLETE,
                format!(
                    "{}: {text} — the line's forces lack that wire: the results are \
                     incomplete (too low), not checked",
                    c.name
                ),
            );
        }
        let mut frame = None;
        egui::ScrollArea::vertical()
            .id_salt("case-results")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if let Some(p) = &c.pole {
                    match p {
                        Ok(p) => pole_ui(ui, p),
                        Err(e) => error_label(ui, format!("{}: pole check: {e}", c.name)),
                    }
                }
                match &c.fe {
                    Some(Ok(r)) => {
                        frame = offset_moments_ui(ui, r, &t);
                        case_ui(ui, r);
                    }
                    Some(Err(e)) => error_label(ui, format!("{}: {e}", c.name)),
                    None => {}
                }
            });
        if let Some(joint) = frame {
            self.make_frame_joint(&joint);
        }
    }
}

/// The moment of each attachment point's offset in one case (a rigid link
/// from its joint, 2026-10-21): applied at the joint where a frame member
/// meets it; left out where only trusses do, with the offer to make it a
/// frame joint (the joint returned when that is clicked).
fn offset_moments_ui(
    ui: &mut egui::Ui,
    r: &CaseResult,
    t: &ohl_structures::StructureType,
) -> Option<String> {
    if r.points.is_empty() {
        return None;
    }
    let mut frame = None;
    egui::CollapsingHeader::new(format!("Offset moments ({})", r.points.len()))
        .id_salt("offset-moments")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "The moment r × F of each attachment point at an offset from its joint (a \
                     rigid link), kN·m, structure axes",
                )
                .small()
                .weak(),
            );
            egui::Grid::new("offset-moments-grid")
                .striped(true)
                .show(ui, |ui| {
                    for h in ["point", "joint", "Mx", "My", "Mz", "|M|", "at the joint"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for p in &r.points {
                        let joint = t.point_link(&p.label).map(|(j, _)| j.to_owned());
                        ui.label(&p.label);
                        ui.label(joint.as_deref().unwrap_or("—"));
                        for m in p.moment {
                            ui.monospace(format!("{:8.2}", m / 1000.0));
                        }
                        ui.monospace(format!("{:8.2}", p.size() / 1000.0));
                        if p.applied {
                            ui.colored_label(crate::app::ok_color(ui), "applied");
                        } else {
                            ui.horizontal(|ui| {
                                ui.colored_label(ui.visuals().warn_fg_color, "left out")
                                    .on_hover_text(p.note.as_deref().unwrap_or(
                                        "only trusses meet the joint: they cannot carry a moment",
                                    ));
                                if let Some(j) = &joint
                                    && ui
                                        .small_button("Make it a frame joint")
                                        .on_hover_text(
                                            "The members meeting the joint made frames (one \
                                             undo step): they then carry the moment; their \
                                             sections need iy, iz and torsion",
                                        )
                                        .clicked()
                                {
                                    frame = Some(j.clone());
                                }
                            });
                        }
                        ui.end_row();
                    }
                });
        });
    frame
}

/// One case's pole check.
fn pole_ui(ui: &mut egui::Ui, p: &PoleResult) {
    egui::CollapsingHeader::new(format!("Pole check — {}", pct(Some(p.usage))))
        .id_salt("pole-check")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("pole-grid").num_columns(2).show(ui, |ui| {
                ui.strong("usage");
                ui.colored_label(rgb(usage_colour(Some(p.usage))), &p.governs);
                ui.end_row();
                ui.strong("ground moment");
                ui.label(format!(
                    "{:.1} kN·m (of it wind {:.1} kN·m), first order",
                    p.ground_moment / 1000.0,
                    p.wind_moment / 1000.0
                ));
                ui.end_row();
                ui.strong("P-Delta");
                ui.label(if p.amplification == 1.0 {
                    "not computed (the pole gives no modulus)".to_owned()
                } else {
                    format!("× {:.3}", p.amplification)
                });
                ui.end_row();
                ui.strong("wind on the pole (x; y)");
                ui.label(format!("{:.0}; {:.0} Pa", p.wind[0], p.wind[1]));
                ui.end_row();
            });
            for n in &p.notes {
                ui.colored_label(ui.visuals().warn_fg_color, n);
            }
        });
}

/// One case's finite-element results.
fn case_ui(ui: &mut egui::Ui, r: &CaseResult) {
    egui::Grid::new("case-summary")
        .num_columns(2)
        .show(ui, |ui| {
            ui.strong("governing");
            match r.governing() {
                Some(g) => ui.colored_label(
                    rgb(usage_colour(g.usage)),
                    format!(
                        "member {} — {:.0} % ({})",
                        g.id,
                        g.usage.unwrap_or(0.0) * 100.0,
                        g.governs
                    ),
                ),
                None => ui.label("no member has a capacity"),
            };
            ui.end_row();
            ui.strong("largest displacement");
            ui.label(r.largest_displacement().map_or("—".into(), |(j, d)| {
                format!("{:.1} mm at joint {j}", d * 1e3)
            }));
            ui.end_row();
            ui.strong("Σ loads (x; y; z up)")
                .on_hover_text("The table's loads and the self-weight, kN, structure axes");
            ui.label(format!("{} kN", kn(r.load_sum)));
            ui.end_row();
            ui.strong("Σ reactions");
            ui.label(format!("{} kN", kn(r.reaction_sum)));
            ui.end_row();
            let scale = r.load_sum.iter().map(|x| x.abs()).fold(1.0, f64::max);
            let ok = r.imbalance() <= 1e-6 * scale;
            ui.strong("equilibrium");
            ui.colored_label(
                if ok {
                    crate::app::ok_color(ui)
                } else {
                    ui.visuals().error_fg_color
                },
                format!("out of balance {:.3} N", r.imbalance()),
            );
            ui.end_row();
            ui.strong("solver");
            ui.label(format!(
                "{}, {} iteration(s), residual {:.1e}",
                if r.nonlinear {
                    "P-Delta"
                } else {
                    "first order"
                },
                r.iterations,
                r.residual
            ));
            ui.end_row();
        });
    for n in &r.notes {
        ui.colored_label(ui.visuals().warn_fg_color, n);
    }
    egui::CollapsingHeader::new(format!("Reactions ({})", r.reactions.len()))
        .id_salt("reactions")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("reactions-grid")
                .striped(true)
                .show(ui, |ui| {
                    for h in ["joint", "Fx kN", "Fy kN", "Fz kN", "M kN·m"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for x in &r.reactions {
                        ui.label(&x.joint);
                        for f in x.force {
                            ui.monospace(format!("{:9.2}", f / 1000.0));
                        }
                        let m = x.moment.iter().map(|v| v * v).sum::<f64>().sqrt();
                        ui.monospace(format!("{:9.2}", m / 1000.0));
                        ui.end_row();
                    }
                });
        });
    egui::CollapsingHeader::new(format!("Members ({}), most used first", r.members.len()))
        .id_salt("members")
        .default_open(true)
        .show(ui, |ui| {
            let mut members: Vec<_> = r.members.iter().collect();
            members.sort_by(|a, b| b.usage.unwrap_or(-1.0).total_cmp(&a.usage.unwrap_or(-1.0)));
            egui::Grid::new("members-grid")
                .striped(true)
                .show(ui, |ui| {
                    for h in ["member", "axial kN", "usage", "governs"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for m in members {
                        ui.label(&m.id);
                        ui.monospace(format!(
                            "{:9.2}{}",
                            m.axial / 1000.0,
                            if m.slack {
                                " slack"
                            } else if m.bowed {
                                " bowed"
                            } else {
                                ""
                            }
                        ));
                        ui.colored_label(
                            rgb(usage_colour(m.usage)),
                            m.usage
                                .map_or("—".into(), |u| format!("{:.0} %", u * 100.0)),
                        );
                        ui.label(&m.governs);
                        ui.end_row();
                    }
                });
        });
}
