// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Magnetic field window's model settings and views (owner,
//! 2026-10-29): the wires' pieces and how much of the line is modelled,
//! the levels drawn (a rule table: value, colour, shown), isolines from
//! above, the field in every cross-section panel open (vertical cuts:
//! what happens near buildings) and volumes over spans (isosurfaces in the
//! 3D view). Every setting says what it changes in the results and an
//! estimate of the time it takes on this computer (from the last run's
//! speed, else a short measurement); each run then says what it measured
//! (how far the drawings lie off their levels, what the wires left out
//! change).

use std::sync::Arc;

use eframe::egui;
use ohl_electrical::contour::Mesh;
use ohl_electrical::field_views::{self, CutValues, Deviation, VerticalCut};
use ohl_electrical::magnetic::{self, Wires3d};

use super::{MagneticFieldWindow, Spans};
use crate::field_overlay::{self, Level, Overlay};
use crate::job::Job;
use crate::model::Model;

/// Volumes and panel cuts reach this far above the highest wire, m.
pub const ABOVE: f64 = 10.0;

/// Until a run measured the speed here: a computation lasting more than a
/// few seconds runs at about half the speed of a short measurement on a
/// laptop (measured 2026-10-29: 1.28 M points/s falling to 0.6 M/s after
/// 5 s on all cores).
pub(crate) const SUSTAINED: f64 = 2.0;

/// Isolines from above: over every span or one, the grid's step.
#[derive(Debug, Clone, PartialEq)]
pub struct Above {
    /// One span (index into the line's spans); none: every span.
    pub span: Option<usize>,
    /// Between the grid's points, m.
    pub step: f64,
}

/// A volume over spans `from` to `to` (indices, along the line).
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeSetup {
    /// The first span.
    pub from: usize,
    /// The last.
    pub to: usize,
    /// The fine lattice's step, m.
    pub step: f64,
}

/// A cross-section panel's cut as computed: where, and the run it was
/// computed from.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelKey {
    /// The plane's point on the axis.
    pub origin: [f64; 2],
    /// To the right looking ahead.
    pub right: [f64; 2],
    /// To either side, m.
    pub width: f64,
    /// From this elevation to the next, m.
    pub heights: [f64; 2],
    /// The step, m.
    pub step: f64,
    /// The run (its generation).
    pub run: u64,
}

/// A panel cut computed: its values, how far its isolines lie off the
/// levels asked then (level, deviation), and the wires left out's change.
#[derive(Debug, Clone)]
pub struct PanelCut {
    /// Where.
    pub key: PanelKey,
    /// The values, µT.
    pub values: Arc<CutValues>,
    /// Per level: how far the isolines lie off it.
    pub deviation: Vec<(f64, Option<Deviation>)>,
    /// What the wires left out change ([`magnetic::Settings::modelled_within`]).
    pub left_out: Option<Deviation>,
    /// Pieces taken.
    pub pieces: usize,
}

/// The cuts of the cross-section panels: computed one batch at a time.
#[derive(Default)]
pub struct PanelCuts {
    /// The batch running.
    pub job: Job<Vec<PanelCut>>,
    /// The cuts computed for the panels open.
    pub done: Vec<PanelCut>,
    /// Counted up when `done` changes.
    pub generation: u64,
    /// Why the last batch failed.
    pub error: Option<String>,
    /// The batch's progress.
    pub progress: Option<Arc<field_views::Progress>>,
}

impl PanelCuts {
    /// The batch's progress bar while it runs (both field windows).
    pub fn progress_ui(&self, ui: &mut egui::Ui) {
        let Some(p) = self.progress.as_ref().filter(|_| self.job.running()) else {
            return;
        };
        let total = p.total.load(std::sync::atomic::Ordering::Relaxed).max(1);
        let done = p.done.load(std::sync::atomic::Ordering::Relaxed);
        let stage = p.stage();
        ui.add(
            egui::ProgressBar::new(done as f32 / total as f32)
                .show_percentage()
                .text(if stage.is_empty() {
                    format!("cross-section cuts: {done} of {total}")
                } else {
                    format!("cross-section cuts: {stage}")
                }),
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }
}

/// The wires' straight pieces as solved (for the estimates).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PieceStats {
    /// Pieces, every subconductor's.
    pub count: usize,
    /// The longest, m.
    pub longest: f64,
    /// The most a wire lies off them, m.
    pub off: f64,
    /// Every subconductor's length, m.
    pub length: f64,
    /// The line's length in plan along its spans, m.
    pub line: f64,
    /// Its spans.
    pub spans: usize,
}

impl PieceStats {
    /// The mean span's length, m.
    pub fn span(&self) -> f64 {
        if self.spans == 0 {
            400.0
        } else {
            self.line / self.spans as f64
        }
    }
}

/// "≈ 12 s", "≈ 3.5 min".
pub(crate) fn duration(s: f64) -> String {
    if s < 1.0 {
        "under a second".into()
    } else if s < 90.0 {
        format!("≈ {s:.0} s")
    } else if s < 5400.0 {
        format!("≈ {:.1} min", s / 60.0)
    } else {
        format!("≈ {:.1} h", s / 3600.0)
    }
}

/// The wires' straight pieces as solved in weather case `case` (index
/// into the model's cases), the earth wires' with `earth_wires`, and the
/// line's length along `spans`.
pub(crate) fn piece_stats(
    model: &Model,
    spans: &[(String, String)],
    case: usize,
    earth_wires: bool,
) -> PieceStats {
    let mut st = PieceStats::default();
    for (a, b) in spans.iter() {
        if let (Some(a), Some(b)) = (model.line.structure(a), model.line.structure(b)) {
            st.line += (b.position[0] - a.position[0]).hypot(b.position[1] - a.position[1]);
            st.spans += 1;
        }
    }
    if let Some(solved) = model.cases.get(case) {
        for shape in solved.shapes.iter() {
            let Some(section) = model.line.sections.iter().find(|s| s.id == shape.section) else {
                continue;
            };
            if section.system.is_none() && !earth_wires {
                continue;
            }
            let subs = section.bundle_offsets().len().max(1);
            for sp in &shape.spans {
                st.off = st.off.max(sp.polyline_error);
                for w in sp.points.windows(2) {
                    let l = ((w[1][0] - w[0][0]).powi(2)
                        + (w[1][1] - w[0][1]).powi(2)
                        + (w[1][2] - w[0][2]).powi(2))
                    .sqrt();
                    st.count += subs;
                    st.length += l * subs as f64;
                    st.longest = st.longest.max(l);
                }
            }
        }
    }
    st
}

/// A deviation in words.
pub fn off_text(d: &Option<Deviation>) -> String {
    match d {
        Some(d) => format!(
            "half within {:.2} %, nine in ten within {:.2} %, at most {:.1} % ({} points)",
            d.median * 100.0,
            d.p90 * 100.0,
            d.max * 100.0,
            d.points
        ),
        None => "no line".into(),
    }
}

/// Where a level lies when it draws nothing: `above`, the share of the
/// values at or above it.
pub(crate) fn nowhere(above: f64) -> &'static str {
    if above >= 1.0 {
        "the field is above it everywhere shown (widen the view to see it)"
    } else {
        "not reached"
    }
}

impl MagneticFieldWindow {
    /// The wires' pieces in the weather case chosen, listed once per
    /// version of the line and case.
    pub(super) fn piece_stats(&mut self, model: &Model) -> PieceStats {
        let key = (model.line_version, self.case);
        if let Some((k, s)) = &self.pieces_seen
            && *k == key
        {
            return *s;
        }
        let spans = self.spans(model);
        let st = piece_stats(model, &spans, self.case, self.earth_wires);
        self.pieces_seen = Some((key, st));
        st
    }

    /// The pieces with the piece length asked: about the length over the
    /// piece length, at least the solver's.
    fn pieces_with(&self, st: &PieceStats, length: Option<f64>) -> f64 {
        match length {
            Some(l) if l > 0.0 => (st.length / l).max(st.count as f64),
            _ => st.count as f64,
        }
    }

    /// Seconds per piece and point: the last run's speed here, else a
    /// short measurement made slower by [`SUSTAINED`]; and where it comes
    /// from.
    fn rate(&self) -> (f64, &'static str) {
        match self.rate {
            Some(r) => (r, "at the last run's speed"),
            None => (
                magnetic::seconds_per_piece_point() * SUSTAINED,
                "measured briefly on this computer",
            ),
        }
    }

    /// The setup of the model and the views (sections 3 and 4).
    pub(super) fn views_ui(&mut self, ui: &mut egui::Ui, model: &Model, spans: &Spans) {
        let st = self.piece_stats(model);
        let (rate, rate_from) = self.rate();
        let pieces = self.pieces_with(&st, self.piece_length);
        // The wires a view takes (modelled within): the share of the line
        // within the distance either side of the view.
        let line = st.line.max(1.0);
        let taken = |d: Option<f64>, extent: f64| -> f64 {
            match d {
                Some(d) => pieces * ((2.0 * d + extent) / line).min(1.0),
                None => pieces,
            }
        };
        crate::check_ui::section(ui, 3, "Model", None);
        egui::Grid::new("magnetic-field-model")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Wire pieces");
                ui.horizontal(|ui| {
                    let mut finer = self.piece_length.is_some();
                    ui.radio_value(&mut finer, false, "as solved");
                    ui.radio_value(&mut finer, true, "at most");
                    if !finer {
                        self.piece_length = None;
                    } else {
                        let l = self.piece_length.get_or_insert(1.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(l).range(0.1..=50.0).speed(0.1).suffix(" m"),
                        );
                    }
                });
                ui.end_row();
                ui.label("");
                let solver = format!(
                    "As solved: {} pieces, the longest {:.1} m; a wire lies at most {:.1} mm off them.",
                    st.count,
                    st.longest,
                    st.off * 1e3
                );
                let text = match self.piece_length {
                    Some(l) if st.longest > 0.0 => {
                        let parts = (st.longest / l).ceil().max(1.0);
                        let off = st.off / (parts * parts);
                        format!(
                            "{solver} At most {l} m: ≈ {:.0} pieces, at most {:.2} mm off; the field \
                             changes by about the wire's offset over the distance to it (at 1 m from \
                             a conductor ≈ {:.2} %, at 10 m ≈ {:.3} %, instead of {:.2} % and {:.3} %). \
                             Time × {:.1}.",
                            pieces,
                            off * 1e3,
                            off * 100.0,
                            off * 10.0,
                            st.off * 100.0,
                            st.off * 10.0,
                            pieces / (st.count.max(1) as f64)
                        )
                    }
                    _ => format!(
                        "{solver} The field changes by about the wire's offset over the distance \
                         to it: ≈ {:.2} % at 1 m from a conductor, ≈ {:.3} % at 10 m.",
                        st.off * 100.0,
                        st.off * 10.0
                    ),
                };
                crate::check_help::explain(
                    ui,
                    &text,
                    "Each straight piece's field is exact (Biot–Savart in closed form); the \
                     only error of the pieces is how far the wire curves away from them (the \
                     sagitta, falling with the square of the piece's length). Shorter pieces \
                     are placed on the curve through the solver's points; where it kinks (a \
                     spacer, a clamp) on the chord.",
                );
                ui.end_row();
                ui.label("Modelled");
                ui.horizontal(|ui| {
                    let mut near = self.modelled_within.is_some();
                    ui.radio_value(&mut near, false, "the whole line");
                    ui.radio_value(&mut near, true, "wires within");
                    if !near {
                        self.modelled_within = None;
                    } else {
                        let d = self.modelled_within.get_or_insert(2000.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(d)
                                .range(50.0..=100_000.0)
                                .speed(10.0)
                                .suffix(" m"),
                        );
                        ui.label("of a cut or volume");
                    }
                });
                ui.end_row();
                ui.label("");
                crate::check_help::explain(
                    ui,
                    &match self.modelled_within {
                        None => "Cuts and volumes take every wire of the line (the most exact; \
                                 the longest)."
                            .to_owned(),
                        Some(d) => format!(
                            "Cuts and volumes take the wires within {d:.0} m only (≈ {:.0} of {:.0} \
                             pieces for one span): quicker; each run computes the whole line's \
                             field at some of their points and says the difference (on a real \
                             line 2 km changed it by at most 0.03 %).",
                            taken(Some(d), st.span()),
                            pieces
                        ),
                    },
                    "The NISV guide asks for at least five times the 1 µT reach before and \
                     after (said for the profiles). Profiles, grids and points always take the \
                     whole line.",
                );
                ui.end_row();
            });
        // The drawings of the model: the pieces of a span, (magnetic)
        // the wires modelled.
        let per_span =
            (st.span() / self.piece_length.unwrap_or(st.longest).max(0.1)).ceil() as usize;
        ui.horizontal(|ui| {
            crate::field_help::pieces(ui, per_span);
            crate::field_help::modelled(ui, self.modelled_within);
        });
        crate::check_ui::section(ui, 4, "Views and levels", None);
        crate::field_help::cut(ui, self.in_cuts.unwrap_or(1.0));
        // The levels: a rule table.
        ui.label(egui::RichText::new("Levels drawn (µT, display only: none is a limit)").small());
        let mut remove = None;
        egui::Grid::new("magnetic-field-levels")
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                for (k, l) in self.levels.iter_mut().enumerate() {
                    ui.checkbox(&mut l.on, "");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut l.value)
                            .range(0.001..=100_000.0)
                            .speed(0.01)
                            .suffix(" µT"),
                    );
                    ui.color_edit_button_srgb(&mut l.rgb);
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove this level")
                        .clicked()
                    {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
            });
        if let Some(k) = remove {
            self.levels.remove(k);
        }
        ui.horizontal(|ui| {
            if ui.small_button("+ level").clicked() {
                let v = self.levels.last().map_or(1.0, |l| l.value * 10.0);
                self.levels.push(Level {
                    value: v,
                    rgb: [180, 90, 220],
                    on: true,
                });
            }
            if let Some(l) = self.limit
                && !self.levels.iter().any(|x| x.value == l)
                && ui.small_button(format!("+ the limit ({l} µT)")).clicked()
            {
                self.levels.push(Level {
                    value: l,
                    rgb: [230, 50, 50],
                    on: true,
                });
            }
            ui.checkbox(&mut self.fill, "colour the cuts by band");
        });
        let n_levels = field_overlay::shown(&self.levels).len().max(1) as f64;
        egui::Grid::new("magnetic-field-views")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // From above.
                ui.label("From above");
                ui.horizontal(|ui| {
                    let mut on = self.above.is_some();
                    ui.checkbox(&mut on, "isolines");
                    if !on {
                        self.above = None;
                    } else {
                        let a = self.above.get_or_insert(Above {
                            span: None,
                            step: 2.0,
                        });
                        ui.label(format!("at {} m above the ground, ±{} m, every", self.height, self.width));
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut a.step)
                                .range(0.1..=50.0)
                                .speed(0.1)
                                .suffix(" m"),
                        );
                        let name = |k: Option<usize>| match k {
                            None => "every span".to_owned(),
                            Some(k) => spans
                                .get(k)
                                .map_or(String::new(), |(f, t)| format!("{f}–{t}")),
                        };
                        egui::ComboBox::from_id_salt("magnetic-field-above-span")
                            .selected_text(name(a.span))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut a.span, None, name(None));
                                for k in 0..spans.len() {
                                    ui.selectable_value(&mut a.span, Some(k), name(Some(k)));
                                }
                            });
                    }
                });
                ui.end_row();
                if let Some(a) = &self.above {
                    let length = match a.span {
                        None => st.line,
                        Some(_) => st.span(),
                    };
                    let points = (length / a.step + 1.0) * (2.0 * self.width / a.step + 1.0);
                    ui.label("");
                    crate::check_help::explain(
                        ui,
                        &format!(
                            "≈ {points:.0} points, {} ({rate_from}). The values at the points are \
                             exact; an isoline is drawn straight between points {} m apart, so \
                             where it bends it may lie off by up to about half a step (each run \
                             says how far, per level).",
                            duration(points * pieces * rate),
                            a.step
                        ),
                        "The grids follow the ground at the profiles' height; their isolines are \
                         drawn on the plan and in the 3D view.",
                    );
                    ui.end_row();
                }
                // The cross-section panels.
                if self.panel.job.running() {
                    ui.label("");
                    self.panel.progress_ui(ui);
                    ui.end_row();
                }
                ui.label("Cross-sections");
                ui.horizontal(|ui| {
                    let mut on = self.in_cuts.is_some();
                    ui.checkbox(&mut on, "the field in every cross-section panel open, every");
                    if !on {
                        self.in_cuts = None;
                    } else {
                        let s = self.in_cuts.get_or_insert(0.5);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(s).range(0.05..=10.0).speed(0.05).suffix(" m"),
                        );
                    }
                });
                ui.end_row();
                if let Some(step) = self.in_cuts {
                    let points = (100.0 / step + 1.0) * (70.0 / step + 1.0);
                    ui.label("");
                    crate::check_help::explain(
                        ui,
                        &format!(
                            "A cut of ±50 m × 70 m: ≈ {points:.0} points, {} per cut ({rate_from}); \
                             computed after a run whenever a panel opens or moves. Isolines are \
                             drawn straight between points {step} m apart (each cut says how far \
                             they lie off their levels).",
                            duration(points * taken(self.modelled_within, 0.0) * rate)
                        ),
                        "Open a cross-section (C) at a building: the panel shows the field in \
                         its plane, the ground, the LiDAR points and the line. Cells coloured by \
                         the highest level at or below their value (optional), the levels' \
                         isolines over them. Not under the ground.",
                    );
                    ui.end_row();
                }
                // A volume.
                ui.label("Volume");
                ui.horizontal(|ui| {
                    let mut on = self.volume.is_some();
                    ui.checkbox(&mut on, "isosurfaces over spans");
                    if !on {
                        self.volume = None;
                    } else {
                        let v = self.volume.get_or_insert(VolumeSetup {
                            from: 0,
                            to: 0,
                            step: 2.0,
                        });
                        let last = spans.len().saturating_sub(1);
                        v.from = v.from.min(last);
                        v.to = v.to.clamp(v.from, last);
                        let name = |k: usize| {
                            spans
                                .get(k)
                                .map_or(String::new(), |(f, t)| format!("{f}–{t}"))
                        };
                        egui::ComboBox::from_id_salt("magnetic-field-volume-from")
                            .selected_text(name(v.from))
                            .show_ui(ui, |ui| {
                                for k in 0..spans.len() {
                                    ui.selectable_value(&mut v.from, k, name(k));
                                }
                            });
                        ui.label("to");
                        egui::ComboBox::from_id_salt("magnetic-field-volume-to")
                            .selected_text(name(v.to))
                            .show_ui(ui, |ui| {
                                for k in v.from..spans.len() {
                                    ui.selectable_value(&mut v.to, k, name(k));
                                }
                            });
                        ui.label(format!("±{} m, every", self.width));
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut v.step)
                                .range(0.25..=20.0)
                                .speed(0.05)
                                .suffix(" m"),
                        );
                    }
                });
                ui.end_row();
                if let Some(v) = &self.volume {
                    let n = (v.to + 1).saturating_sub(v.from).max(1) as f64;
                    let full =
                        n * (st.span() / v.step) * (2.0 * self.width / v.step) * (70.0 / v.step);
                    let share = self.volume_share.unwrap_or(0.5);
                    let t = full * share * taken(self.modelled_within, st.span() * n) * rate;
                    ui.label("");
                    crate::check_help::explain(
                        ui,
                        &format!(
                            "{n:.0} span(s) of ≈ {:.0} m: a lattice of ≈ {:.1} M points, of which \
                             ≈ {:.0} % computed ({}), {} ({rate_from}); {} levels. Halving the \
                             step takes about 4 to 8 times as long and places the surfaces closer \
                             to their levels: on a real line 2 m put the 1 µT surface within \
                             0.05 % for nine points in ten, the 10 µT one within 1 %, the 100 µT \
                             one (a thin tube around each conductor) within 15 % — 1 m within \
                             0.01 %, 0.3 % and 4 %. Each run says how far, per level.",
                            st.span(),
                            full / 1e6,
                            share * 100.0,
                            if self.volume_share.is_some() {
                                "as in the last run"
                            } else {
                                "a guess until a run"
                            },
                            duration(t),
                            n_levels
                        ),
                        "A coarse lattice first (4 steps apart), then the fine one only where a \
                         level passes, near the wires, and wherever a surface goes on; every \
                         lattice point is computed in full, only the surface between the points \
                         is interpolated. Drawn translucent in the 3D view, each level in its \
                         colour.",
                    );
                    ui.end_row();
                }
            });
    }

    /// The results of the views: what each measured.
    pub(super) fn views_results_ui(
        &self,
        ui: &mut egui::Ui,
        m: &magnetic::MagneticField,
        meshes: &[(f64, Arc<Mesh>)],
    ) {
        let mut mesh_at = meshes.iter();
        let levels = field_overlay::shown(&self.levels);
        if m.grids.is_empty()
            && m.volumes.is_empty()
            && self.panel.done.is_empty()
            && !self.panel.job.running()
        {
            return;
        }
        ui.separator();
        ui.strong("Views");
        self.panel.progress_ui(ui);
        let small = |ui: &mut egui::Ui, t: String| {
            ui.label(egui::RichText::new(t).small());
        };
        // From above: per level, the span whose isolines lie farthest off.
        for (k, l) in self.run_levels.iter().enumerate() {
            if m.grids.is_empty() {
                break;
            }
            let worst = m
                .grids
                .iter()
                .filter_map(|g| Some((g, g.deviation.get(k).copied().flatten()?)))
                .max_by(|a, b| a.1.max.total_cmp(&b.1.max));
            let everywhere = m
                .grids
                .iter()
                .all(|g| g.points.iter().all(|p| p.b_ut >= *l));
            small(
                ui,
                match worst {
                    Some((g, d)) => format!(
                        "From above ({} spans), {l} µT: the isolines lie off the level by {} — the farthest in {}–{}",
                        m.grids.len(),
                        off_text(&Some(d)),
                        g.from,
                        g.to
                    ),
                    None => format!(
                        "From above, {l} µT: {}",
                        nowhere(if everywhere { 1.0 } else { 0.0 })
                    ),
                },
            );
        }
        for v in &m.volumes {
            let vv = &v.values;
            small(
                ui,
                format!(
                    "Volume {}–{}: {} of {} lattice points computed ({} m apart), {} pieces{}",
                    v.from,
                    v.to,
                    vv.computed,
                    vv.full,
                    vv.bounds.step,
                    v.pieces,
                    match &v.left_out {
                        Some(d) => format!(
                            "; the wires left out change the field by {}",
                            off_text(&Some(*d))
                        ),
                        None => String::new(),
                    }
                ),
            );
            for (k, ((l, _), d)) in vv.surfaces.iter().zip(&vv.deviation).enumerate() {
                let n = mesh_at.next().map_or(0, |m| m.1.triangles.len());
                small(
                    ui,
                    if n == 0 {
                        format!(
                            "    {l} µT: no surface — {}",
                            nowhere(vv.above.get(k).copied().unwrap_or(0.0))
                        )
                    } else {
                        format!(
                            "    {l} µT: {n} triangles, off the level by {}",
                            off_text(d)
                        )
                    },
                );
            }
        }
        for c in &self.panel.done {
            small(
                ui,
                format!(
                    "Cross-section cut ({} × {} points, {} m apart, {} pieces){}",
                    c.values.offsets.len(),
                    c.values.elevations.len(),
                    c.key.step,
                    c.pieces,
                    match &c.left_out {
                        Some(d) => format!(
                            "; the wires left out change the field by {}",
                            off_text(&Some(*d))
                        ),
                        None => String::new(),
                    }
                ),
            );
            for (l, d) in &c.deviation {
                if levels.iter().any(|x| x.value == *l) {
                    small(
                        ui,
                        match d {
                            Some(_) => {
                                format!("    {l} µT: isolines off the level by {}", off_text(d))
                            }
                            None => {
                                let all = c.values.values.iter().all(|v| v >= l);
                                format!("    {l} µT: {}", nowhere(if all { 1.0 } else { 0.0 }))
                            }
                        },
                    );
                }
            }
        }
        if let Some(e) = &self.panel.error {
            crate::menu::error_label(ui, e.as_str());
        }
    }

    /// Computes the field in the cross-section panels open (their plane,
    /// half width and heights) when shown and not computed yet.
    pub fn panel_cuts(
        &mut self,
        panels: Vec<(crate::xsection::Plane, f64, [f64; 2])>,
        model: &Model,
        ctx: &egui::Context,
    ) {
        self.panel.job.poll();
        if let Some(r) = self.panel.job.result.take() {
            match r {
                Ok(list) => {
                    self.panel.done.extend(list);
                    self.panel.error = None;
                }
                Err(e) => self.panel.error = Some(e),
            }
            self.panel.generation += 1;
        }
        let (Some(step), Some(wires)) = (self.in_cuts, self.wires.clone()) else {
            if !self.panel.done.is_empty() {
                self.panel.done.clear();
                self.panel.generation += 1;
            }
            return;
        };
        let run = self.job.generation;
        let keys: Vec<PanelKey> = panels
            .iter()
            .map(|(p, w, h)| PanelKey {
                origin: p.origin,
                right: p.right(),
                width: *w,
                heights: [h[0], h[1] + 2.0],
                step,
                run,
            })
            .collect();
        let before = self.panel.done.len();
        self.panel.done.retain(|c| keys.contains(&c.key));
        if self.panel.done.len() != before {
            self.panel.generation += 1;
        }
        let missing: Vec<PanelKey> = keys
            .into_iter()
            .filter(|k| !self.panel.done.iter().any(|c| c.key == *k))
            .collect();
        if missing.is_empty() || self.panel.job.running() {
            return;
        }
        let ground = model.ground_for_checks();
        let levels: Vec<f64> = field_overlay::shown(&self.levels)
            .iter()
            .map(|l| l.value)
            .collect();
        let within = self.modelled_within;
        let progress = Arc::new(field_views::Progress::default());
        self.panel.progress = Some(progress.clone());
        self.panel.job.start(ctx, move || {
            let mut out = Vec::new();
            for key in missing {
                let cut = VerticalCut {
                    origin: key.origin,
                    right: key.right,
                    left: -key.width,
                    right_offset: key.width,
                    bottom: key.heights[0].floor(),
                    top: key.heights[1],
                    step: key.step,
                };
                let (a, b) = (cut.at(-key.width), cut.at(key.width));
                let line = field_views::Path::new(vec![[a[0], a[1], 0.0], [b[0], b[1], 0.0]])?;
                let local = within.map(|d| wires.within(&line, 0.0, d));
                let w: &Wires3d = local.as_ref().unwrap_or(&wires);
                let ut = |ps: &[[f64; 3]]| -> Vec<f64> {
                    w.field_block(ps).iter().map(|b| b.0 * 1e6).collect()
                };
                let height = |x: f64, y: f64| ground.as_ref().and_then(|t| t.height(x, y));
                progress
                    .total
                    .fetch_add(cut.points(), std::sync::atomic::Ordering::Relaxed);
                let values = field_views::vertical_cut(&cut, &ut, Some(&height), Some(&progress))?;
                let deviation = levels
                    .iter()
                    .map(|&l| (l, values.deviation(l, &ut)))
                    .collect();
                let left_out = local.as_ref().and_then(|_| {
                    let full = |ps: &[[f64; 3]]| -> Vec<f64> {
                        wires.field_block(ps).iter().map(|b| b.0 * 1e6).collect()
                    };
                    let pts: Vec<[f64; 3]> = values
                        .elevations
                        .iter()
                        .flat_map(|&z| {
                            values.offsets.iter().map(move |&s| {
                                let p = cut.at(s);
                                [p[0], p[1], z]
                            })
                        })
                        .collect();
                    relative(&pts, &full, &ut)
                });
                out.push(PanelCut {
                    key,
                    values: Arc::new(values),
                    deviation,
                    left_out,
                    pieces: w.count(),
                });
            }
            Ok(out)
        });
    }

    /// The overlay when it changed (the run, the panel cuts, the levels,
    /// open or closed, out of date or not).
    pub fn overlay_change(&mut self, model: &Model) -> Option<Overlay> {
        let stale = self.out_of_date(model).is_some();
        let levels = field_overlay::shown(&self.levels);
        let key = (
            self.open,
            self.job.generation,
            self.panel.generation,
            serde_json::to_string(&levels).unwrap_or_default(),
            self.fill,
            stale,
        );
        if self.drawn.as_ref() == Some(&key) {
            return None;
        }
        self.drawn = Some(key);
        let mut o = Overlay {
            stale,
            ..Default::default()
        };
        if !self.open {
            return Some(o);
        }
        if let Some(Ok(a)) = &self.job.result {
            for g in &a.field.grids {
                for l in &levels {
                    for line in g.isolines(l.value) {
                        o.plan.push((l.rgb, line));
                    }
                }
            }
            for (level, mesh) in &a.meshes {
                if let Some(l) = levels.iter().find(|l| l.value == *level) {
                    o.surfaces.push((l.rgb, mesh.clone()));
                }
            }
        }
        for c in &self.panel.done {
            o.cuts
                .push(field_overlay::cut_drawing(&c.values, &levels, self.fill));
        }
        Some(o)
    }
}

/// The relative difference of `b` to `a` at up to
/// [`field_views::CHECKED`] of `pts`.
fn relative(
    pts: &[[f64; 3]],
    a: field_views::Field<'_, f64>,
    b: field_views::Field<'_, f64>,
) -> Option<Deviation> {
    let every = pts.len().div_ceil(field_views::CHECKED).max(1);
    let picked: Vec<[f64; 3]> = pts.iter().step_by(every).copied().collect();
    let x = field_views::compute_all(&picked, a, None).ok()?;
    let y = field_views::compute_all(&picked, b, None).ok()?;
    let mut d: Vec<f64> = x
        .iter()
        .zip(&y)
        .filter(|(p, _)| **p > 0.0)
        .map(|(p, q)| (p - q).abs() / p)
        .collect();
    if d.is_empty() {
        return None;
    }
    d.sort_by(f64::total_cmp);
    let at = |q: f64| d[((d.len() - 1) as f64 * q).round() as usize];
    Some(Deviation {
        points: d.len(),
        median: at(0.5),
        p90: at(0.9),
        max: at(1.0),
    })
}

/// A run's meshes taken out of its volumes (shared with the overlay, not
/// copied at every redraw).
pub fn take_meshes(volumes: &mut [magnetic::FieldVolume]) -> Vec<(f64, Arc<Mesh>)> {
    let mut out = Vec::new();
    for v in volumes {
        for (l, m) in &mut v.values.surfaces {
            out.push((*l, Arc::new(std::mem::take(m))));
        }
    }
    out
}
