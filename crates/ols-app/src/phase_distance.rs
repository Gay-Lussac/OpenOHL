// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › LeV / OLEI › Distance between conductors: the still-air distance
//! between the conductors of each span by a phase distance code
//! (`.olspd`, [`ols_clearance::phase_distance`]), in the weather cases
//! ticked (each on its own), for the circuits ticked; the insulation
//! levels from the Electrical circuits table (the LeV plugin's column).
//! Computed on demand in the background; worst pairs first; a click
//! selects the span and draws the worst station's line with d and z.

use std::collections::BTreeSet;
use std::path::PathBuf;

use eframe::egui;
use ols_clearance::phase_distance::{
    Conductor, PhaseDistanceCode, PhaseDistanceSettings, check, conductors,
};

use crate::job::{Job, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// A worst station within this of a conductor's clamp is marked "at a
/// clamp" (f about 0), m.
const NEAR_CLAMP: f64 = 2.0;

/// One pair of conductors in one weather case.
#[derive(Debug, Clone)]
pub struct Row {
    /// Weather case.
    pub case: String,
    /// Each conductor's span (section, span index).
    pub span: [(String, usize); 2],
    /// Each conductor's circuit and phase, in words.
    pub what: [String; 2],
    /// Same circuit, or towards an earth wire: C₁.
    pub same_circuit: bool,
    /// Between the same two structures (else a nearby pair).
    pub same_span: bool,
    /// Nominal voltage used, kV.
    pub kv: f64,
    /// Distance at the worst station, m.
    pub distance: f64,
    /// Required z there, m.
    pub required: f64,
    /// Conductor below z, m.
    pub length_below: f64,
    /// The worst station's points.
    pub at: [[f64; 3]; 2],
    /// Where along each conductor, m.
    pub along: [f64; 2],
    /// The worst station is within [`NEAR_CLAMP`] of a clamp.
    pub at_clamp: bool,
    /// Local sag of each, m.
    pub sag: [f64; 2],
    /// l_k of each, m.
    pub lk: [f64; 2],
    /// System angle, degrees.
    pub beta: f64,
    /// k of each.
    pub k: [f64; 2],
    /// C after the altitude, m.
    pub c: f64,
}

impl Row {
    /// d − z, m.
    pub fn margin(&self) -> f64 {
        self.distance - self.required
    }
}

/// What a check found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The code's name.
    pub code: String,
    /// The diagram used.
    pub diagram: String,
    /// Notes (cases not solved, sections not solved).
    pub notes: Vec<String>,
    /// Pairs of every case, worst first.
    pub rows: Vec<Row>,
}

/// A line to draw: the worst station of a pair, with its label.
pub type Measure = ([f64; 3], [f64; 3], String);

/// What the window asks of the application.
#[derive(Default)]
pub struct Out {
    /// A span to go to.
    pub go: Option<Selection>,
    /// The line to draw (d and z), replacing the last.
    pub measure: Option<Measure>,
}

/// The Distance between conductors window.
pub struct PhaseDistanceWindow {
    /// Shown.
    pub open: bool,
    /// The code's values file.
    pub file: PathBuf,
    /// Weather cases ticked (indices in the model's cases).
    pub cases: BTreeSet<usize>,
    /// Circuits ticked (none: all).
    pub circuits: Option<BTreeSet<String>>,
    /// The settings (circuits: from `circuits`).
    pub settings: PhaseDistanceSettings,
    /// Only pairs below z.
    pub only_below: bool,
    /// The row clicked.
    pub picked: Option<usize>,
    job: Job<Report>,
}

impl Default for PhaseDistanceWindow {
    fn default() -> Self {
        Self {
            open: false,
            file: ols_core::files::shipped_library("phase-distance").join("lev-ch.olspd"),
            cases: BTreeSet::new(),
            circuits: None,
            settings: PhaseDistanceSettings::default(),
            only_below: true,
            picked: None,
            job: Job::default(),
        }
    }
}

/// The still-air cases at 10 °C (the LeV's f), else none.
fn still_air_10(model: &Model) -> BTreeSet<usize> {
    model
        .cases
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let c = &c.case;
            (c.temperature.celsius() - 10.0).abs() < 0.01
                && c.wind_pressure.si() == 0.0
                && c.ice_thickness.si() == 0.0
                && c.ice_load.is_none_or(|l| l.si() == 0.0)
        })
        .map(|(k, _)| k)
        .collect()
}

/// The circuits of the line (names of its sections' circuits), sorted.
fn circuit_names(model: &Model) -> Vec<String> {
    let mut v: Vec<String> = model
        .line
        .sections
        .iter()
        .map(|s| s.circuit.clone())
        .collect();
    v.sort();
    v.dedup();
    v
}

impl PhaseDistanceWindow {
    /// Opens the window: the still-air 10 °C cases ticked when none is.
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        self.cases.retain(|k| *k < model.cases.len());
        if self.cases.is_empty() {
            self.cases = still_air_10(model);
        }
    }

    /// The last check's report, and whether one runs.
    #[cfg(test)]
    pub fn report(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// Starts the check in the background (cases not solved yet are
    /// solved there).
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        if self.cases.is_empty() {
            self.job.result = Some(Err("tick a weather case".into()));
            return;
        }
        self.picked = None;
        let path = self.file.clone();
        let line = model.line.clone();
        let library = model.library.clone();
        let wind = model.wind;
        let cases: Vec<(crate::model::Solved, bool)> = self
            .cases
            .iter()
            .filter_map(|k| model.cases.get(*k))
            .map(|c| (c.clone(), c.solved_for(wind)))
            .collect();
        let mut settings = self.settings.clone();
        settings.circuits = self.circuits.clone();
        self.job.start(ctx, move || {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let code =
                PhaseDistanceCode::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            let diagram = code
                .diagrams
                .get(settings.diagram)
                .map_or_else(String::new, |d| d.name.clone());
            let mut notes = Vec::new();
            let mut rows = Vec::new();
            for (solved, ready) in cases {
                let solved = if ready {
                    solved
                } else {
                    crate::model::Solved::with_wind(&line, &library, solved.case.clone(), wind)
                };
                let name = solved.case.name.clone();
                notes.extend(
                    solved
                        .failed
                        .iter()
                        .map(|f| format!("\"{name}\": not solved: {f}")),
                );
                let cs = conductors(&line, &solved.shapes, &library);
                // Exit clause: about a second for a 25 km line.
                let budget =
                    ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(300));
                let pairs = check(&code, &settings, &cs, &budget)?;
                let length = |c: &Conductor| {
                    c.wire
                        .points
                        .windows(2)
                        .map(|w| {
                            (0..3)
                                .map(|i| (w[1][i] - w[0][i]).powi(2))
                                .sum::<f64>()
                                .sqrt()
                        })
                        .sum::<f64>()
                };
                let what = |c: &Conductor| {
                    if c.wire.potential.is_none() {
                        format!("{} (earth wire)", c.wire.circuit)
                    } else {
                        format!("{} {}", c.wire.circuit, c.wire.phase)
                    }
                };
                for p in pairs {
                    let (a, b) = (&cs[p.a], &cs[p.b]);
                    let w = &p.worst;
                    let near = |c: &Conductor, s: f64| s < NEAR_CLAMP || length(c) - s < NEAR_CLAMP;
                    rows.push(Row {
                        case: name.clone(),
                        span: [
                            (a.wire.section.clone(), a.wire.span),
                            (b.wire.section.clone(), b.wire.span),
                        ],
                        what: [what(a), what(b)],
                        same_circuit: p.same_circuit,
                        same_span: p.same_span,
                        kv: p.kv,
                        distance: w.distance,
                        required: w.required,
                        length_below: p.length_below,
                        at: w.at,
                        along: w.along,
                        at_clamp: near(a, w.along[0]) || near(b, w.along[1]),
                        sag: w.sag,
                        lk: w.lk,
                        beta: w.beta,
                        k: w.k,
                        c: w.c,
                    });
                }
            }
            rows.sort_by(|a, b| a.margin().total_cmp(&b.margin()));
            Ok(Report {
                code: code.name,
                diagram,
                notes,
                rows,
            })
        });
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Out {
        self.job.poll();
        let mut out = Out::default();
        let mut open = self.open;
        egui::Window::new("Distance between conductors (LeV)")
            .open(&mut open)
            .default_width(980.0)
            .show(ctx, |ui| {
                self.settings_ui(ui, model);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.job.running(), egui::Button::new("Check"))
                        .clicked()
                    {
                        self.start(model, ui.ctx());
                    }
                    self.job.status_ui(ui);
                });
                ui.separator();
                self.results_ui(ui, &mut out);
            });
        self.open = open;
        out
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let code = std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|t| PhaseDistanceCode::parse(&t).ok());
        ui.horizontal(|ui| {
            let name = self
                .file
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            ui.label(format!("values: {name}"))
                .on_hover_text(self.file.display().to_string());
            if ui.button("Choose…").clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Phase distance code", &["olspd"])
                    .set_directory(ols_core::files::shipped_library("phase-distance"))
                    .pick_file()
            {
                self.file = p;
            }
            match &code {
                Some(c) => {
                    ui.label("k diagram");
                    let shown = c
                        .diagrams
                        .get(self.settings.diagram)
                        .map_or("—".to_owned(), |d| d.name.clone());
                    egui::ComboBox::from_id_salt("phase-distance-diagram")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (k, d) in c.diagrams.iter().enumerate() {
                                ui.selectable_value(&mut self.settings.diagram, k, &d.name);
                            }
                        });
                }
                None => {
                    crate::menu::error_label(ui, "the values file does not read");
                }
            }
        });
        ui.horizontal(|ui| {
            let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
            let title = match self.cases.len() {
                0 => "Weather cases: none".to_owned(),
                1 => format!(
                    "Weather case: {}",
                    names
                        .get(*self.cases.first().expect("one"))
                        .map_or("", String::as_str)
                ),
                n => format!("Weather cases: {n}"),
            };
            crate::menu::options_menu(ui, &title, |ui| {
                ui.label(
                    egui::RichText::new("each checked on its own (LeV: f at 10 °C, still air)")
                        .small()
                        .weak(),
                );
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (k, n) in names.iter().enumerate() {
                            let mut on = self.cases.contains(&k);
                            if ui.checkbox(&mut on, n).changed() {
                                if on {
                                    self.cases.insert(k);
                                } else {
                                    self.cases.remove(&k);
                                }
                            }
                        }
                    });
            });
            let all = circuit_names(model);
            let ticked = |c: &Option<BTreeSet<String>>, n: &String| {
                c.as_ref().is_none_or(|set| set.contains(n))
            };
            let n = all.iter().filter(|n| ticked(&self.circuits, n)).count();
            let title = if n == all.len() {
                "Circuits: all".to_owned()
            } else {
                format!("Circuits: {n} of {}", all.len())
            };
            crate::menu::options_menu(ui, &title, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("Select all").clicked() {
                        self.circuits = None;
                    }
                    if ui.small_button("Select none").clicked() {
                        self.circuits = Some(BTreeSet::new());
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for name in &all {
                            let mut on = ticked(&self.circuits, name);
                            let label = if name.is_empty() {
                                "(no circuit name)"
                            } else {
                                name.as_str()
                            };
                            if ui.checkbox(&mut on, label).changed() {
                                let set = self
                                    .circuits
                                    .get_or_insert_with(|| all.iter().cloned().collect());
                                if on {
                                    set.insert(name.clone());
                                } else {
                                    set.remove(name);
                                }
                                if set.len() == all.len() {
                                    self.circuits = None;
                                }
                            }
                        }
                    });
            });
            ui.label("stations every");
            ui.add(
                egui::DragValue::new(&mut self.settings.spacing)
                    .range(0.1..=50.0)
                    .speed(0.1)
                    .suffix(" m"),
            );
            ui.checkbox(&mut self.settings.nearby, "nearby spans too")
                .on_hover_text(
                    "Also conductors not between the same two structures (another line, the \
                     next span), measured where closer than the distance",
                );
            ui.add_enabled(
                self.settings.nearby,
                egui::DragValue::new(&mut self.settings.max_distance)
                    .range(1.0..=200.0)
                    .speed(0.5)
                    .prefix("within ")
                    .suffix(" m"),
            )
            .on_hover_text("Keep it above the largest z (about 12 m at 380 kV)");
        });
        ui.label(
            egui::RichText::new(
                "Insulation level (low / high row of Table 2.4.1) per circuit: the Electrical \
                 circuits table, column of the LeV plugin; not set: high.",
            )
            .small()
            .weak(),
        );
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, out: &mut Out) {
        let report = match &self.job.result {
            None => {
                ui.label(
                    "z = k·√(f + l_k) + C between the conductors of each span, at stations \
                     along each, the closest point of the other; f the local sag, l_k from the \
                     strings at the ends, k by the system angle and swing class, C by voltage, \
                     circuits and altitude.",
                );
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e);
                return;
            }
            Some(Ok(r)) => r,
        };
        for n in &report.notes {
            crate::menu::message(ui, egui::RichText::new(n).small(), None);
        }
        let below: Vec<&Row> = report.rows.iter().filter(|r| r.margin() < 0.0).collect();
        let length: f64 = below.iter().map(|r| r.length_below).sum();
        let mut export = false;
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} — {}: {} pairs, {} below z ({length:.0} m of conductor)",
                report.code,
                report.diagram,
                report.rows.len(),
                below.len()
            ));
            ui.checkbox(&mut self.only_below, "only those");
            export = ui.button("Export CSV…").clicked();
        });
        if export {
            save_csv("phase-distance.csv", &csv(report));
        }
        let mut clicked = None;
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("phase-distance-rows")
                    .striped(true)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                        for h in [
                            "case", "span", "circuit", "other", "circuit", "C", "kV", "d m", "z m",
                            "d − z", "below m", "f m", "l_k m", "β °", "k", "C m", "",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (i, r) in report
                            .rows
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| !self.only_below || r.margin() < 0.0)
                            .take(5000)
                        {
                            let picked = self.picked == Some(i);
                            ui.label(&r.case);
                            for k in 0..2 {
                                let (s, n) = &r.span[k];
                                if ui
                                    .selectable_label(picked, format!("{s} span {}", n + 1))
                                    .on_hover_text(
                                        "Select this span and draw the worst station's line \
                                         (d and z)",
                                    )
                                    .clicked()
                                {
                                    clicked = Some((i, k));
                                }
                                ui.label(&r.what[k]);
                            }
                            ui.label(if r.same_circuit { "C₁" } else { "C₂" });
                            ui.label(format!("{:.0}", r.kv));
                            let mono = |v: f64| egui::RichText::new(format!("{v:.2}")).monospace();
                            ui.label(mono(r.distance));
                            ui.label(mono(r.required));
                            let m = mono(r.margin());
                            ui.label(if r.margin() < 0.0 {
                                m.color(egui::Color32::LIGHT_RED)
                            } else {
                                m
                            });
                            ui.label(format!("{:.0}", r.length_below));
                            ui.label(format!("{:.2} / {:.2}", r.sag[0], r.sag[1]));
                            ui.label(format!("{:.2} / {:.2}", r.lk[0], r.lk[1]));
                            ui.label(format!("{:.0}", r.beta));
                            ui.label(format!("{:.2} / {:.2}", r.k[0], r.k[1]));
                            ui.label(format!("{:.2}", r.c));
                            let mut note = Vec::new();
                            if r.at_clamp {
                                note.push("at a clamp (f ≈ 0)");
                            }
                            if !r.same_span {
                                note.push("nearby span");
                            }
                            ui.label(egui::RichText::new(note.join(", ")).small().weak());
                            ui.end_row();
                        }
                    });
            });
        if let Some((i, k)) = clicked {
            let r = &report.rows[i];
            self.picked = Some(i);
            out.go = Some(Selection::Span {
                section: r.span[k].0.clone(),
                span: r.span[k].1,
            });
            out.measure = Some((
                r.at[0],
                r.at[1],
                format!("d {:.2} m · z {:.2} m", r.distance, r.required),
            ));
        }
    }
}

fn csv(r: &Report) -> String {
    use ols_core::text::csv_field as q;
    use std::fmt::Write as _;
    let mut out = String::from(
        "case,section,span,circuit,other_section,other_span,other_circuit,c,kv,distance_m,\
         required_m,margin_m,length_below_m,sag_m,other_sag_m,lk_m,other_lk_m,beta_deg,k,\
         other_k,c_m,at_clamp,same_span,along_m,other_along_m,x,y,z,other_x,other_y,other_z\n",
    );
    for row in &r.rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.1},{:.3},{:.3},{:.3},{:.3},{:.1},\
             {:.3},{:.3},{:.3},{},{},{:.1},{:.1},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
            q(&row.case),
            q(&row.span[0].0),
            row.span[0].1 + 1,
            q(&row.what[0]),
            q(&row.span[1].0),
            row.span[1].1 + 1,
            q(&row.what[1]),
            if row.same_circuit { "C1" } else { "C2" },
            row.kv,
            row.distance,
            row.required,
            row.margin(),
            row.length_below,
            row.sag[0],
            row.sag[1],
            row.lk[0],
            row.lk[1],
            row.beta,
            row.k[0],
            row.k[1],
            row.c,
            row.at_clamp,
            row.same_span,
            row.along[0],
            row.along[1],
            row.at[0][0],
            row.at[0][1],
            row.at[0][2],
            row.at[1][0],
            row.at[1][1],
            row.at[1][2],
        );
    }
    out
}
