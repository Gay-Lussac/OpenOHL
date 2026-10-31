// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Electric field window's model settings and views (2026-10-29), as
//! the magnetic field's ([`crate::magnetic_field::views`]): the wires'
//! pieces, the levels drawn (kV/m), the field in every cross-section
//! panel open and volumes over spans. Every setting says what it changes
//! in the results and an estimate of the time it takes on this computer
//! (from the last run's speed, else the magnetic field's short
//! measurement: a point sums every piece twice, the charges and their
//! images); each run then says how far its drawings lie off their levels.
//! Not yet: isolines from above.

use std::sync::Arc;

use eframe::egui;
use ohl_electrical::contour::Mesh;
use ohl_electrical::electric::{self, GroundPlane};
use ohl_electrical::field_views::{self, VerticalCut};
use ohl_electrical::magnetic;

use super::{ElectricFieldWindow, Spans, Stations};
use crate::field_overlay::{self, Level, Overlay};
use crate::magnetic_field::views::{
    PanelCut, PanelKey, PieceStats, SUSTAINED, VolumeSetup, duration, nowhere, off_text,
    piece_stats,
};
use crate::model::Model;

/// A point sums every piece twice: the charges and their images in the
/// ground (against the magnetic field's once).
const IMAGES: f64 = 2.0;

impl ElectricFieldWindow {
    /// The wires' pieces in the weather case chosen (the earth wires'
    /// too: they carry charge), listed once per version of the line and
    /// case.
    fn piece_stats(&mut self, model: &Model) -> PieceStats {
        let key = (model.line_version, self.case);
        if let Some((k, s)) = &self.pieces_seen
            && *k == key
        {
            return *s;
        }
        let spans = self.spans(model);
        let st = piece_stats(model, &spans, self.case, true);
        self.pieces_seen = Some((key, st));
        st
    }

    /// Seconds per piece and point: the last run's speed here, else the
    /// magnetic field's short measurement times [`IMAGES`] made slower by
    /// [`SUSTAINED`]; and where it comes from.
    fn rate(&self) -> (f64, &'static str) {
        match self.rate {
            Some(r) => (r, "at the last run's speed"),
            None => (
                magnetic::seconds_per_piece_point() * IMAGES * SUSTAINED,
                "measured briefly on this computer",
            ),
        }
    }

    /// The setup of the model and the views (sections 3 and 4; 3D).
    pub(super) fn views_ui(&mut self, ui: &mut egui::Ui, model: &Model, spans: &Spans) {
        let st = self.piece_stats(model);
        let (rate, rate_from) = self.rate();
        let pieces = match self.piece_length {
            Some(l) if l > 0.0 => (st.length / l).max(st.count as f64),
            _ => st.count as f64,
        };
        crate::check_ui::section(ui, 4, "Model", None);
        // Folded by default (owner, 2026-10-31: the setup easy to read):
        // each setting with its effect and time when opened.
        egui::CollapsingHeader::new("Wire pieces, structures' steel, the ground's tiles")
            .id_salt("electric-field-model-fold")
            .default_open(false)
            .show(ui, |ui| {
                self.model_ui(ui, model, spans, &st, (pieces, rate, rate_from))
            });
        crate::check_ui::section(ui, 5, "Views and levels", None);
        self.levels_ui(ui, rate, rate_from, pieces);
    }

    /// Section 4's settings (folded): the steel, the wire pieces, the
    /// profiles' time. `(pieces, rate, rate_from)`: the pieces, the time
    /// per piece and point and where it comes from.
    fn model_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        spans: &Spans,
        st: &PieceStats,
        (pieces, rate, rate_from): (f64, f64, &str),
    ) {
        let st = *st;
        egui::Grid::new("electric-field-model")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // The structures' steel (owner, 2026-10-31: off by default).
                ui.label("Structures' steel");
                let [with, members, steel_pieces, left_out] = self.steel_counts(model);
                ui.add_enabled_ui(self.tiled, |ui| {
                    ui.checkbox(&mut self.steel, "earthed (on the tiled floor)")
                        .on_hover_text(
                            "The structures' steel at earth potential, solved with each tiled \
                             floor it stands on",
                        );
                });
                ui.end_row();
                ui.label("");
                let per = steel_pieces.checked_div(with).unwrap_or(0);
                let tiles = electric::TiledFloor {
                    fine: self.tile,
                    ..Default::default()
                }
                .tiles(self.width);
                let text = if !self.tiled {
                    "Only on the tiled floor (Ground): with the plane under each point the \
                     structures are not modelled."
                        .to_owned()
                } else {
                    format!(
                        "{with} structure(s) with members ({members} members, ≈ {per} pieces of steel \
                         each); {left_out} without (detail level 1) left out. Lowers the field \
                         near the structures, raises it at the steel; tens of metres away no \
                         change. A profile's floor beside two structures {} instead of {}.",
                        duration(self.floor_seconds(tiles + 2 * per)),
                        duration(self.floor_seconds(tiles))
                    )
                };
                crate::check_help::explain(
                    ui,
                    &text,
                    "The steel is at earth potential: it takes charges that keep it there, \
                     solved together with the floor's tiles (each member in pieces of at most \
                     3 m, its radius from its section). Left out it reads the field near the \
                     ground beside a structure higher — on the safe side; at the steel itself \
                     lower. The line's charges are taken as without the steel. Off by default \
                     (owner, 2026-10-31). Use the profiles beside the structures to see it.",
                );
                ui.end_row();
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
                            "{solver} At most {l} m: ≈ {pieces:.0} pieces, at most {:.2} mm off; the \
                             field changes by about the wire's offset over the distance to it (at \
                             1 m from a conductor ≈ {:.2} %, at 10 m ≈ {:.3} %, instead of {:.2} % \
                             and {:.3} %). Time × {:.1}.",
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
                    "Each straight piece's field is exact (Coulomb's law in closed form, a \
                     uniform charge along the piece); the only error of the pieces is how far \
                     the wire curves away from them (the sagitta, falling with the square of \
                     the piece's length). Shorter pieces are placed on the curve through the \
                     solver's points; where it kinks (a spacer, a clamp) on the chord.",
                );
                ui.end_row();
                // The profiles' and points' time.
                let across = (2.0 * self.width / self.step.max(0.01)).round() + 1.0;
                let profiles = match self.stations {
                    Stations::EverySpan => self.span_pick.chosen(model, spans).0.len().max(1) as f64,
                    Stations::One { .. } => 1.0,
                    Stations::Beside { .. } => {
                        2.0 * self.span_pick.chosen(model, spans).0.len().max(1) as f64
                    }
                };
                let given = self.points.as_ref().map_or(0, |p| p.1.len()) as f64;
                let points = profiles * across + given;
                ui.label("Profiles");
                crate::check_help::explain(
                    ui,
                    &format!(
                        "≈ {points:.0} points × {pieces:.0} pieces, each twice (the charges and \
                         their images): {} ({rate_from}).",
                        duration(points * pieces * rate)
                    ),
                    "Every value is computed in full from every piece of the line: a halved \
                     step across takes twice as long, a narrower width less.",
                );
                ui.end_row();
            });
        // The drawing of the model: the pieces of a span.
        let per_span =
            (st.span() / self.piece_length.unwrap_or(st.longest).max(0.1)).ceil() as usize;
        crate::field_help::pieces(ui, per_span);
    }

    /// Section 5: the levels drawn, the cuts coloured by band, and the
    /// field in any cross-section panel opened.
    fn levels_ui(&mut self, ui: &mut egui::Ui, rate: f64, rate_from: &str, pieces: f64) {
        crate::field_help::cut(ui, self.in_cuts.unwrap_or(1.0));
        // The levels: a rule table.
        ui.label(egui::RichText::new("Levels drawn (kV/m, display only: none is a limit)").small());
        let mut remove = None;
        egui::Grid::new("electric-field-levels")
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                for (k, l) in self.levels.iter_mut().enumerate() {
                    ui.checkbox(&mut l.on, "");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut l.value)
                            .range(0.001..=1000.0)
                            .speed(0.01)
                            .suffix(" kV/m"),
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
                let v = self.levels.last().map_or(1.0, |l| l.value * 2.0);
                self.levels.push(Level {
                    value: v,
                    rgb: [180, 90, 220],
                    on: true,
                });
            }
            if let Some(l) = self.limit
                && !self.levels.iter().any(|x| x.value == l)
                && ui.small_button(format!("+ the limit ({l} kV/m)")).clicked()
            {
                self.levels.push(Level {
                    value: l,
                    rgb: [230, 50, 50],
                    on: true,
                });
            }
            ui.checkbox(&mut self.fill, "colour the cuts by band");
        });
        egui::Grid::new("electric-field-views")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // The cross-section panels.
                if self.panel.job.running() {
                    ui.label("");
                    self.panel.progress_ui(ui);
                    ui.end_row();
                }
                ui.label("Cross-sections");
                ui.horizontal(|ui| {
                    let mut on = self.in_cuts.is_some();
                    ui.checkbox(&mut on, "also in any cross-section panel I open, every")
                        .on_hover_text(
                            "Besides the cuts asked in the table: the field in every \
                             cross-section panel you open (C), computed when it opens or moves",
                        );
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
                            duration(points * pieces * rate)
                        ),
                        "Open a cross-section (C) at a building: the panel shows the field in \
                         its plane, the ground, the LiDAR points and the line. Cells coloured by \
                         the highest level at or below their value (optional), the levels' \
                         isolines over them. Not under the ground. The building itself is not \
                         modelled: earthed, it lowers the field beside it and raises it at its \
                         top.",
                    );
                    ui.end_row();
                }
            });
    }

    /// The sizes of what the table asks (section 3, 3D; owner, 2026-10-31:
    /// one line under the table): the cuts' width, step and reach, the
    /// volumes' step, each with how long it takes.
    pub(super) fn sizes_ui(&mut self, ui: &mut egui::Ui, model: &Model, spans: &Spans) {
        let st = self.piece_stats(model);
        let (rate, rate_from) = self.rate();
        let pieces = match self.piece_length {
            Some(l) if l > 0.0 => (st.length / l).max(st.count as f64),
            _ => st.count as f64,
        };
        let n_levels = field_overlay::shown(&self.levels).len().max(1) as f64;
        let cuts = self.span_pick.cuts_asked(model, spans).len();
        let runs = self.span_pick.volume_runs(model, spans);
        let ticked: usize = runs.iter().map(|r| r.1).sum();
        egui::Grid::new("electric-field-sizes")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("2D cuts");
                ui.horizontal(|ui| {
                    ui.label("±");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.cut_width)
                            .range(5.0..=500.0)
                            .speed(1.0)
                            .suffix(" m"),
                    );
                    ui.label("every");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.cut_step)
                            .range(0.05..=10.0)
                            .speed(0.05)
                            .suffix(" m"),
                    );
                    ui.label("up to");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.cut_above)
                            .range(0.0..=100.0)
                            .speed(0.5)
                            .suffix(" m"),
                    );
                    ui.label("over the highest wire");
                });
                ui.end_row();
                let points =
                    (2.0 * self.cut_width / self.cut_step + 1.0) * (70.0 / self.cut_step + 1.0);
                ui.label("");
                crate::check_help::explain(
                    ui,
                    &format!(
                        "{cuts} cut(s) asked (+ station in the table): ≈ {points:.0} points \
                         each, {} per cut ({rate_from}). Each a vertical plane across its span, \
                         its isolines at the levels (5); Results › Open cut shows it on the wires.",
                        duration(points * pieces * rate)
                    ),
                    "The cut is perpendicular to the span (between its two structures), from \
                     the lowest ground to this far over the highest wire; on the tiled floor \
                     its own floor of the cut tiles (Ground).",
                );
                ui.end_row();
                // The volumes: their spans ticked in the table.
                ui.label("Volumes");
                ui.horizontal(|ui| {
                    let v = self.volume.get_or_insert(VolumeSetup { step: 2.0 });
                    ui.label(format!(
                        "{ticked} span(s) with Volume ticked ({} volume(s)), ±{} m, every",
                        runs.len(),
                        self.width
                    ));
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut v.step)
                            .range(0.25..=20.0)
                            .speed(0.05)
                            .suffix(" m"),
                    );
                });
                ui.end_row();
                if let (Some(v), true) = (&self.volume, ticked > 0) {
                    let n = ticked as f64;
                    let full =
                        n * (st.span() / v.step) * (2.0 * self.width / v.step) * (70.0 / v.step);
                    let share = self.volume_share.unwrap_or(0.5);
                    let t = full * share * pieces * rate;
                    ui.label("");
                    crate::check_help::explain(
                        ui,
                        &format!(
                            "{n:.0} span(s) of ≈ {:.0} m: a lattice of ≈ {:.1} M points, of which \
                             ≈ {:.0} % computed ({}), {} ({rate_from}); {} levels. Halving the \
                             step takes about 4 to 8 times as long and places the surfaces closer \
                             to their levels; thin surfaces around the conductors need a fine \
                             step. Results › Show in 3D turns the 3D view to it.",
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
                         colour. Spans with Volume ticked that follow each other make one \
                         volume (no seam at the structures).",
                    );
                    ui.end_row();
                    // The floor under the longest run (owner, 2026-10-31:
                    // laptops of 16 to 32 GB): its memory said before Run.
                    if self.tiled {
                        let longest = runs.iter().map(|r| r.1).max().unwrap_or(1);
                        let tiles = electric::Floor::tiles_under(
                            st.span() * longest as f64,
                            self.width,
                            self.volume_tile,
                        );
                        let bytes = electric::floor_bytes(tiles);
                        let text = format!(
                            "Its floor: the longest volume ({longest} span(s)) ≈ {tiles} tiles of \
                             {} m, ≈ {:.2} GB of memory while solved, {}.",
                            self.volume_tile,
                            bytes / 1e9,
                            duration(self.floor_seconds(tiles))
                        );
                        ui.label("");
                        if bytes > electric::FLOOR_MEMORY {
                            crate::menu::error_label(
                                ui,
                                format!(
                                    "{text} Over the {:.0} GB a floor may take: tick Volume in \
                                     fewer spans following each other, or coarser tiles under \
                                     the volumes (Ground), or the plane under each point.",
                                    electric::FLOOR_MEMORY / 1e9
                                ),
                            );
                        } else {
                            crate::check_help::explain(
                                ui,
                                &text,
                                "A volume's floor runs along its whole run of spans and 100 m \
                                 beyond: its memory grows with the run's length squared, its \
                                 time faster still.",
                            );
                        }
                        ui.end_row();
                    }
                }
            });
    }

    /// What the run will compute and about how long it takes (owner,
    /// 2026-10-31: a summary beside Run): the spans ticked, the profiles,
    /// the cuts, the volumes.
    pub(super) fn summary(&mut self, model: &Model) -> String {
        let spans = self.spans(model);
        let n = self.span_pick.chosen(model, &spans).0.len();
        let profiles = match self.stations {
            Stations::EverySpan => n,
            Stations::Beside { .. } => 2 * n,
            Stations::One { .. } => 1,
        };
        let (cuts, volumes) = if self.two_d {
            (0, 0)
        } else {
            (
                self.span_pick.cuts_asked(model, &spans).len(),
                self.span_pick.volume_runs(model, &spans).len(),
            )
        };
        let st = self.piece_stats(model);
        let (rate, _) = self.rate();
        let pieces = match self.piece_length {
            Some(l) if l > 0.0 => (st.length / l).max(st.count as f64),
            _ => st.count as f64,
        };
        let across = (2.0 * self.width / self.step.max(0.01)).round() + 1.0;
        let cut_points =
            (2.0 * self.cut_width / self.cut_step + 1.0) * (70.0 / self.cut_step + 1.0);
        let mut seconds = (profiles as f64 * across + cuts as f64 * cut_points) * pieces * rate;
        if self.tiled && !self.two_d {
            let floor = electric::TiledFloor {
                fine: self.tile,
                ..Default::default()
            };
            seconds += profiles as f64 * self.floor_seconds(floor.tiles(self.width));
            seconds += cuts as f64
                * self.floor_seconds(
                    electric::TiledFloor {
                        fine: self.cut_tile,
                        coarse: self.cut_tile.max(8.0),
                        ..Default::default()
                    }
                    .tiles(self.cut_width),
                );
        }
        if let Some(v) = &self.volume
            && volumes > 0
        {
            let ticked: usize = self
                .span_pick
                .volume_runs(model, &spans)
                .iter()
                .map(|r| r.1)
                .sum();
            let full = ticked as f64
                * (st.span() / v.step)
                * (2.0 * self.width / v.step)
                * (70.0 / v.step);
            seconds += full * self.volume_share.unwrap_or(0.5) * pieces * rate;
        }
        format!(
            "{n} span(s) · {profiles} profile(s) · {cuts} cut(s) · {volumes} volume(s) — {}",
            duration(seconds)
        )
    }

    /// The results of the views: what each measured.
    pub(super) fn views_results_ui(
        &self,
        ui: &mut egui::Ui,
        m: &electric::ElectricField,
        meshes: &[(f64, Arc<Mesh>)],
    ) {
        let mut mesh_at = meshes.iter();
        let levels = field_overlay::shown(&self.levels);
        if m.volumes.is_empty()
            && self.panel.done.is_empty()
            && !self.panel.job.running()
            && self.panel.error.is_none()
        {
            return;
        }
        ui.separator();
        ui.strong("Views");
        self.panel.progress_ui(ui);
        let small = |ui: &mut egui::Ui, t: String| {
            ui.label(egui::RichText::new(t).small());
        };
        for v in &m.volumes {
            let vv = &v.values;
            small(
                ui,
                format!(
                    "Volume {}–{}: {} of {} lattice points computed ({} m apart), {} pieces",
                    v.from, v.to, vv.computed, vv.full, vv.bounds.step, v.pieces,
                ),
            );
            for (k, ((l, _), d)) in vv.surfaces.iter().zip(&vv.deviation).enumerate() {
                let n = mesh_at.next().map_or(0, |m| m.1.triangles.len());
                small(
                    ui,
                    if n == 0 {
                        format!(
                            "    {l} kV/m: no surface — {}{}",
                            nowhere(vv.above.get(k).copied().unwrap_or(0.0)),
                            field_views::left_out_text(vv.left_out.get(k).copied().unwrap_or(0))
                        )
                    } else {
                        format!(
                            "    {l} kV/m: {n} triangles, off the level by {}{}",
                            off_text(d),
                            field_views::left_out_text(vv.left_out.get(k).copied().unwrap_or(0))
                        )
                    },
                );
            }
        }
        for c in &self.panel.done {
            small(
                ui,
                format!(
                    "Cross-section cut ({} × {} points, {} m apart, {} pieces)",
                    c.values.offsets.len(),
                    c.values.elevations.len(),
                    c.key.step,
                    c.pieces,
                ),
            );
            for (l, d) in &c.deviation {
                if levels.iter().any(|x| x.value == *l) {
                    small(
                        ui,
                        match d {
                            Some(_) => {
                                format!("    {l} kV/m: isolines off the level by {}", off_text(d))
                            }
                            None => {
                                let all = c.values.values.iter().all(|v| v >= l);
                                format!("    {l} kV/m: {}", nowhere(if all { 1.0 } else { 0.0 }))
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
    /// half width and heights) when shown and not computed yet, from the
    /// last run's charges and on its ground.
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
        let (Some(step), Some((charges, radius, ground_model, cut_tile))) =
            (self.in_cuts, self.charges.clone())
        else {
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
        // The ground as the run took it: the terrain's, else straight
        // between the structures' feet.
        let terrain = model.ground_for_checks();
        let feet = electric::feet_ground(&model.line);
        let levels: Vec<f64> = field_overlay::shown(&self.levels)
            .iter()
            .map(|l| l.value)
            .collect();
        let progress = Arc::new(field_views::Progress::default());
        self.panel.progress = Some(progress.clone());
        self.panel.job.start(ctx, move || {
            let ground = |x: f64, y: f64| {
                terrain
                    .as_ref()
                    .and_then(|t| t.height(x, y))
                    .unwrap_or_else(|| feet(x, y))
            };
            let plane = |ps: &[[f64; 3]]| -> Vec<f64> {
                ps.iter()
                    .map(|&p| {
                        charges.field(p, &GroundPlane::fitted(&ground, p[0], p[1], radius)) / 1e3
                    })
                    .collect()
            };
            let height = |x: f64, y: f64| Some(ground(x, y));
            let mut out = Vec::new();
            for key in missing {
                // On the tiled floor: one around the cut.
                let floor = match ground_model {
                    electric::GroundModel::Tiled(f) => {
                        let f = electric::TiledFloor {
                            fine: cut_tile,
                            coarse: cut_tile.max(f.coarse),
                            ..f
                        };
                        let tiles = f.tiles(key.width);
                        progress
                            .total
                            .fetch_add(tiles, std::sync::atomic::Ordering::Relaxed);
                        progress.set_stage(format!("a tiled floor of {tiles} tiles"));
                        let fl = electric::Floor::solve(
                            &charges,
                            electric::Frame::of_cut(key.origin, key.right),
                            0.0,
                            key.width,
                            &f,
                            &ground,
                        )?;
                        progress
                            .done
                            .fetch_add(tiles, std::sync::atomic::Ordering::Relaxed);
                        progress.set_stage(String::new());
                        Some(fl)
                    }
                    electric::GroundModel::Plane => None,
                };
                let tiled = |ps: &[[f64; 3]]| -> Vec<f64> {
                    let fl = floor.as_ref().expect("a floor");
                    ps.iter().map(|&p| fl.field(p, &charges) / 1e3).collect()
                };
                let kv: &(dyn Fn(&[[f64; 3]]) -> Vec<f64> + Sync) =
                    if floor.is_some() { &tiled } else { &plane };
                let cut = VerticalCut {
                    origin: key.origin,
                    right: key.right,
                    left: -key.width,
                    right_offset: key.width,
                    bottom: key.heights[0].floor(),
                    top: key.heights[1],
                    step: key.step,
                };
                progress
                    .total
                    .fetch_add(cut.points(), std::sync::atomic::Ordering::Relaxed);
                let values = field_views::vertical_cut(&cut, &kv, Some(&height), Some(&progress))?;
                let deviation = levels
                    .iter()
                    .map(|&l| (l, values.deviation(l, &kv)))
                    .collect();
                out.push(PanelCut {
                    key,
                    values: Arc::new(values),
                    deviation,
                    left_out: None,
                    pieces: charges.count(),
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
            for (level, mesh) in &a.meshes {
                if let Some(l) = levels.iter().find(|l| l.value == *level) {
                    o.surfaces.push((l.rgb, mesh.clone()));
                }
            }
            // The cuts asked in the table (owner, 2026-10-31): drawn in
            // their planes, so a panel opened there (Open cut) shows them.
            for c in &a.field.cuts {
                o.cuts
                    .push(field_overlay::cut_drawing(&c.values, &levels, self.fill));
            }
        }
        for c in &self.panel.done {
            o.cuts
                .push(field_overlay::cut_drawing(&c.values, &levels, self.fill));
        }
        Some(o)
    }
}
