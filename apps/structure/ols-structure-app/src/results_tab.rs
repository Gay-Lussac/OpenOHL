// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The FE results panel (Results menu): the cases of the load table
//! solved by the finite-element model (through `ols-structure-link`, on a thread of its
//! own, each case under a time budget), and per case the largest
//! displacement, Σ loads against Σ reactions, the reactions, the members'
//! forces and usage and the governing member; in the view the deformed
//! shape and the members coloured by usage. A line case whose values lack
//! a wire the line did not solve is marked incomplete ("not checked —
//! incomplete: S12 not solved"), its usage only that of the loads there
//! are.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ols_core::budget::{Budget, CancelToken};
use ols_render::Scene;
use ols_structure_link::{CaseResult, CaseRun, CaseToRun, analyse_cases};
use ols_structures::Detail;

use crate::app::{StructureApp, error_label};
use crate::load_cases_tab::arrow;

/// Most iterations of one case.
const ITERATIONS: u64 = 200;
/// Longest time one case may take (a solve takes milliseconds).
const CASE_TIME: Duration = Duration::from_secs(30);
/// Usage from which a member is near full (as the line application's
/// Usage page).
const NEAR_FULL: f64 = 0.9;

/// The answer of a run: per case its result or why it did not solve; Err:
/// the model does not read.
pub type Run = Result<Vec<CaseRun>, String>;

/// The state of the FE results panel.
pub struct ResultsTab {
    /// The case to run (empty: every case).
    pub case: String,
    /// The last answer.
    pub results: Option<Run>,
    /// The load table's key it was computed from.
    pub ran_on: u64,
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

impl ResultsTab {
    /// A run is going on.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Starts solving `cases` of the table on its own thread (unless one
    /// runs).
    fn start(
        &mut self,
        ctx: &egui::Context,
        key: u64,
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
        if let Ok(v) = &r
            && let Some(d) = v
                .iter()
                .filter_map(|(_, r)| r.as_ref().ok()?.largest_displacement().map(|d| d.1))
                .reduce(f64::max)
            && d > 1e-9
        {
            let s = 1.0 / d;
            let p = 10f64.powf(s.log10().floor());
            self.scale = (s / p).floor().max(1.0) * p;
        }
        self.results = Some(r);
    }

    /// The case shown, solved.
    pub fn shown(&self) -> Option<&CaseResult> {
        match &self.results {
            Some(Ok(v)) => v.get(self.shown)?.1.as_ref().ok(),
            _ => None,
        }
    }

    /// The deformed shape coloured by usage, and the reactions, of the
    /// case shown. Its key changes with what it shows.
    pub fn overlay(&self) -> Option<(u64, Scene)> {
        let r = self.shown()?;
        let mut h = DefaultHasher::new();
        (
            self.ran_on,
            self.shown,
            self.deformed,
            self.scale.to_bits(),
            self.reactions,
            self.took,
        )
            .hash(&mut h);
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
        Some((h.finish(), s))
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

    /// Results › Run (and ▶ Run): solves the case chosen (or every case) of
    /// the load table on its own thread; false when it cannot start.
    pub fn run_results(&mut self, ctx: &egui::Context) -> bool {
        if !self.can_run_results() {
            return false;
        }
        let Some(f) = &self.file else {
            return false;
        };
        let t = f.structure();
        let table = self.loads.table().clone();
        let key = self.loads.key();
        let run: Vec<CaseToRun> = table
            .cases()
            .into_iter()
            .filter(|c| self.results.case.is_empty() || *c == self.results.case)
            .map(|c| {
                let recipe = self
                    .loads
                    .recipes()
                    .ok()
                    .and_then(|r| r.iter().find(|r| r.name == c));
                CaseToRun {
                    name: c.to_owned(),
                    strength: recipe.map_or(1.0, |r| r.strength),
                    self_weight: recipe.map_or(1.0, |r| r.self_weight),
                }
            })
            .collect();
        let structure = t.clone();
        let text = f.text();
        let dir = f.path().and_then(Path::parent).map(Path::to_path_buf);
        let sections = self.loads.sections.clone();
        self.results.start(ctx, key, move |budget| {
            analyse_cases(
                &structure,
                &text,
                dir.as_deref(),
                &sections,
                &table,
                &run,
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

    /// The FE results panel.
    pub fn results_ui(&mut self, ui: &mut egui::Ui) {
        self.results.poll();
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure();
        if t.detail() != Detail::Design {
            ui.label(
                "Results need level 3: every member with a section and a supported joint (or a \
                 model = \"….olsfe\" file). What is still missing is listed in the header.",
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
            let running = self.results.running();
            if ui
                .add_enabled(!running && !cases.is_empty(), egui::Button::new("▶ Run"))
                .on_hover_text(format!(
                    "Solve with the finite-element model (each case at most {} s and {ITERATIONS} \
                     iterations)",
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
        });
        if run_now {
            self.run_results(&ctx);
        }
        let stale = self.results.results.is_some() && self.results.ran_on != key;
        if stale {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "The file or the load table changed since: run again for results of them.",
            );
        }
        let results = match &self.results.results {
            None => {
                ui.label(
                    "Run solves the cases of the load table (Load cases › Load table) on the structure's \
                     finite-element model: a value typed there counts, rows without a value are \
                     left out. Each case runs under a time budget; Cancel stops it.",
                );
                return;
            }
            Some(Err(e)) => {
                error_label(ui, format!("the model does not read: {e}"));
                return;
            }
            Some(Ok(v)) => v.clone(),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("Shown:");
            let names: Vec<&str> = results.iter().map(|(c, _)| c.as_str()).collect();
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
        // Every case at a glance: its governing member, or why not solved.
        egui::ScrollArea::vertical()
            .id_salt("results-cases")
            .max_height(70.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (k, (name, r)) in results.iter().enumerate() {
                    let incomplete = self.loads.incomplete(name);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.results.shown == k, name.as_str())
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
                        match r {
                            Ok(r) if incomplete.is_some() => {
                                if let Some(g) = r.governing() {
                                    ui.weak(format!(
                                        "({:.0} % of the loads there are — member {})",
                                        g.usage.unwrap_or(0.0) * 100.0,
                                        g.id
                                    ));
                                }
                            }
                            Ok(r) => match r.governing() {
                                Some(g) => {
                                    let u = g.usage.unwrap_or(0.0);
                                    ui.colored_label(
                                        rgb(usage_colour(g.usage)),
                                        format!("{:.0} % — member {}", u * 100.0, g.id),
                                    );
                                }
                                None => {
                                    ui.label("no member has a capacity");
                                }
                            },
                            Err(e) => {
                                ui.colored_label(ui.visuals().error_fg_color, e);
                            }
                        }
                    });
                }
            });
        ui.separator();
        let Some((name, r)) = results.get(self.results.shown.min(results.len().saturating_sub(1)))
        else {
            return;
        };
        let r = match r {
            Ok(r) => r,
            Err(e) => {
                error_label(ui, format!("{name}: {e}"));
                return;
            }
        };
        if let Some(text) = self.loads.incomplete(name) {
            ui.colored_label(
                crate::load_cases_tab::INCOMPLETE,
                format!(
                    "{name}: {text} — the line's forces lack that wire: the results are \
                     incomplete (too low), not checked"
                ),
            );
        }
        case_ui(ui, r);
    }
}

/// One case's results.
fn case_ui(ui: &mut egui::Ui, r: &CaseResult) {
    egui::ScrollArea::vertical()
        .id_salt("case-results")
        .auto_shrink([false, true])
        .show(ui, |ui| {
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
                    members.sort_by(|a, b| {
                        b.usage.unwrap_or(-1.0).total_cmp(&a.usage.unwrap_or(-1.0))
                    });
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
                                    if m.slack { " slack" } else { "" }
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
        });
}
