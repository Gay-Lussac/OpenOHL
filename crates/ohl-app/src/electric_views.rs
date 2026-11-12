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
use crate::field_common::FieldPage;
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
        let key = (model.line_version, self.common.case);
        if let Some((k, s)) = &self.common.pieces_seen
            && *k == key
        {
            return *s;
        }
        let spans = self.common.spans(model);
        let st = piece_stats(model, &spans, self.common.case, true);
        self.common.pieces_seen = Some((key, st));
        st
    }

    /// Seconds per piece and point: the last run's speed here, else the
    /// magnetic field's short measurement times [`IMAGES`] made slower by
    /// [`SUSTAINED`]; and where it comes from.
    fn rate(&self) -> (f64, &'static str) {
        match self.common.rate {
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
        let pieces = match self.common.piece_length {
            Some(l) if l > 0.0 => (st.length / l).max(st.count as f64),
            _ => st.count as f64,
        };
        let page = self.common.page;
        let flat = self.two_d_page();
        if let Some(a) = crate::check_ui::section(ui, 4, "Model", Some(page.guide())) {
            self.common.guide = Some(a);
        }
        // Folded by default (owner, 2026-10-31: the setup easy to read):
        // each setting with its effect and time when opened; only what the
        // page's method takes (owner, 2026-11-12).
        if flat {
            // 2D only: nothing of the 3D model; the profiles' time.
            crate::check_help::explain(
                ui,
                "2D only: no model setting; a profile takes a moment.",
                "The 2D-only method takes the wires' heights where they cross the profile and \
                 level ground there: the jumpers, the stretches, the ground's tiles, the steel \
                 and the wire pieces are the 3D model's.",
            );
        } else {
            egui::CollapsingHeader::new(
                "Jumpers, stretches, the ground and its tiles, steel, wire pieces",
            )
            .id_salt("electric-field-model-fold")
            .default_open(false)
            .show(ui, |ui| {
                self.model_inputs_ui(ui, model);
                self.model_ui(ui, model, spans, &st, (pieces, rate, rate_from))
            });
        }
        // The levels: drawn in cuts and volumes, not on a profile (its
        // plot draws the limit).
        if page == FieldPage::Profiles {
            return;
        }
        if let Some(a) = crate::check_ui::section(ui, 5, "Views and levels", Some(page.guide())) {
            self.common.guide = Some(a);
        }
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
                .tiles(self.common.width);
                let text = if !self.tiled {
                    "Only on the tiled floor (Ground): with the plane under each point the \
                     structures are not modelled."
                        .to_owned()
                } else {
                    format!(
                        "{with} with members, {left_out} left out; a profile's floor beside two \
                         structures {} instead of {}",
                        duration(self.floor_seconds(tiles + 2 * per)),
                        duration(self.floor_seconds(tiles))
                    )
                };
                crate::check_help::explain(
                    ui,
                    &text,
                    &format!(
                        "{with} structure(s) with members ({members} members, ≈ {per} pieces of \
                         steel each); {left_out} without (detail level 1) left out. Lowers the \
                         field near the structures, raises it at the steel; tens of metres away \
                         no change. The steel is at earth potential: it takes charges that keep it there, \
                     solved together with the floor's tiles (each member in pieces of at most \
                     3 m, its radius from its section). Left out it reads the field near the \
                     ground beside a structure higher — on the safe side; at the steel itself \
                     lower. The line's charges are taken as without the steel. Off by default \
                     (owner, 2026-10-31). Use the profiles beside the structures to see it."
                    ),
                );
                ui.end_row();
                ui.label("Wire pieces");
                ui.horizontal(|ui| {
                    let mut finer = self.common.piece_length.is_some();
                    ui.radio_value(&mut finer, false, "as solved");
                    ui.radio_value(&mut finer, true, "at most");
                    if !finer {
                        self.common.piece_length = None;
                    } else {
                        let l = self.common.piece_length.get_or_insert(1.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(l).range(0.1..=50.0).speed(0.1).suffix(" m"),
                        )
                        .on_hover_text(
                            "The longest straight piece of a wire: shorter, a field closer to the \
                             curved wire's, longer to compute in proportion",
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
                // One short line (the counts and the time), the rest behind
                // its (?) (owner, 2026-11-12: no grey paragraphs).
                let (short, text) = match self.common.piece_length {
                    Some(l) if st.longest > 0.0 => {
                        let parts = (st.longest / l).ceil().max(1.0);
                        let off = st.off / (parts * parts);
                        let times = pieces / (st.count.max(1) as f64);
                        (
                            format!(
                                "≈ {pieces:.0} pieces, at most {:.2} mm off the wire; time × {times:.1}",
                                off * 1e3
                            ),
                            format!(
                                "{solver} At most {l} m: ≈ {pieces:.0} pieces, at most {:.2} mm off; the \
                                 field changes by about the wire's offset over the distance to it (at \
                                 1 m from a conductor ≈ {:.2} %, at 10 m ≈ {:.3} %, instead of {:.2} % \
                                 and {:.3} %). Time × {times:.1}.",
                                off * 1e3,
                                off * 100.0,
                                off * 10.0,
                                st.off * 100.0,
                                st.off * 10.0,
                            ),
                        )
                    }
                    _ => (
                        format!(
                            "{} pieces, at most {:.1} mm off the wire: ≈ {:.3} % at 10 m",
                            st.count,
                            st.off * 1e3,
                            st.off * 10.0
                        ),
                        format!(
                            "{solver} The field changes by about the wire's offset over the \
                             distance to it: ≈ {:.2} % at 1 m from a conductor, ≈ {:.3} % at 10 m.",
                            st.off * 100.0,
                            st.off * 10.0
                        ),
                    ),
                };
                crate::check_help::explain(
                    ui,
                    &short,
                    &format!(
                        "{text} Each straight piece's field is exact (Coulomb's law in closed \
                         form, a uniform charge along the piece); the only error of the pieces is \
                         how far the wire curves away from them (the sagitta, falling with the \
                         square of the piece's length). Shorter pieces are placed on the curve \
                         through the solver's points; where it kinks (a spacer, a clamp) on the \
                         chord."
                    ),
                );
                ui.end_row();
                // The profiles' and points' time (their page).
                if self.common.page != FieldPage::Profiles {
                    return;
                }
                let across = (2.0 * self.common.width / self.common.step.max(0.01)).round() + 1.0;
                let profiles = match self.common.stations {
                    Stations::EverySpan => self.common.span_pick.chosen(model, spans).0.len().max(1) as f64,
                    Stations::One { .. } => 1.0,
                    Stations::Beside { .. } => {
                        2.0 * self.common.span_pick.chosen(model, spans).0.len().max(1) as f64
                    }
                };
                let given = self.common.points.as_ref().map_or(0, |p| p.1.len()) as f64;
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
            (st.span() / self.common.piece_length.unwrap_or(st.longest).max(0.1)).ceil() as usize;
        crate::field_help::pieces(ui, per_span);
    }

    /// Section 5: the levels drawn, the cuts coloured by band, and the
    /// field in any cross-section panel opened.
    fn levels_ui(&mut self, ui: &mut egui::Ui, rate: f64, rate_from: &str, pieces: f64) {
        // The levels: a rule table.
        ui.horizontal(|ui| {
            ui.label("Levels drawn [kV/m]");
            crate::check_help::question(
                ui,
                "The isolines and surfaces drawn at these values, each in its colour, ticked to \
                 draw it: display only, none is a limit (give the limit in 2 Scope; + the limit \
                 adds it here). More levels: more surfaces to draw in a volume.",
            );
        });
        let mut remove = None;
        egui::Grid::new("electric-field-levels")
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                for (k, l) in self.common.levels.iter_mut().enumerate() {
                    ui.checkbox(&mut l.on, "");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut l.value)
                            .range(0.001..=1000.0)
                            .speed(0.01)
                            .suffix(" kV/m"),
                    )
                    .on_hover_text("An isoline (a surface in a volume) drawn at this value");
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
            self.common.levels.remove(k);
        }
        ui.horizontal(|ui| {
            if ui.small_button("+ level").clicked() {
                let v = self.common.levels.last().map_or(1.0, |l| l.value * 2.0);
                self.common.levels.push(Level {
                    value: v,
                    rgb: [180, 90, 220],
                    on: true,
                });
            }
            if let Some(l) = self.common.limit
                && !self.common.levels.iter().any(|x| x.value == l)
                && ui.small_button(format!("+ the limit ({l} kV/m)")).clicked()
            {
                self.common.levels.push(Level {
                    value: l,
                    rgb: [230, 50, 50],
                    on: true,
                });
            }
            if self.common.page == FieldPage::Cuts {
                ui.checkbox(&mut self.common.fill, "colour the cuts by band");
            }
        });
        // The cross-section panels: cuts of their own (the cuts' page).
        if self.common.page != FieldPage::Cuts {
            return;
        }
        egui::Grid::new("electric-field-views")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // The cross-section panels.
                if self.common.panel.job.running() {
                    ui.label("");
                    self.common.panel.progress_ui(ui);
                    ui.end_row();
                }
                ui.label("Cross-sections");
                ui.horizontal(|ui| {
                    let mut on = self.common.in_cuts.is_some();
                    ui.checkbox(&mut on, "also in any cross-section panel I open, every")
                        .on_hover_text(
                            "Besides the cuts asked in the table: the field in every \
                             cross-section panel you open (C), computed when it opens or moves",
                        );
                    if !on {
                        self.common.in_cuts = None;
                    } else {
                        let s = self.common.in_cuts.get_or_insert(1.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(s).range(0.05..=10.0).speed(0.05).suffix(" m"),
                        )
                        .on_hover_text(
                            "The step of a panel's points: finer, isolines closer to their levels, \
                             four times the points at half the step",
                        );
                    }
                });
                ui.end_row();
                if let Some(step) = self.common.in_cuts {
                    let points = (100.0 / step + 1.0) * (70.0 / step + 1.0);
                    ui.label("");
                    crate::check_help::explain(
                        ui,
                        &format!(
                            "≈ {points:.0} points, {} per panel ({rate_from})",
                            duration(points * pieces * rate)
                        ),
                        &format!(
                            "A cut of ±50 m × 70 m, computed after a run whenever a panel opens or \
                             moves. Isolines are drawn straight between points {step} m apart (each \
                             cut says how far they lie off their levels). Open a cross-section (C) at a building: the panel shows the field in \
                         its plane, the ground, the LiDAR points and the line. Cells coloured by \
                         the highest level at or below their value (optional), the levels' \
                         isolines over them. Not under the ground. The building itself is not \
                         modelled: earthed, it lowers the field beside it and raises it at its \
                         top."
                        ),
                    );
                    ui.end_row();
                }
            });
    }

    /// The sizes of what the table asks (section 3, 3D; owner, 2026-10-31:
    /// one line under the table): the cuts' width, step and reach, the
    /// volumes' step, each with how long it takes.
    /// Why a run would be refused for its memory: a tiled floor of the
    /// run needing more than one calculation may take (Line › Calculation
    /// options; owner, 2026-11-12: such an error blocks the start). The
    /// same estimates as the window shows, the volumes' and cuts' only
    /// where there are any.
    pub(super) fn memory_block(&mut self, model: &Model, spans: &Spans) -> Option<String> {
        if !self.tiled {
            return None;
        }
        let limit = electric::floor_memory();
        let page = self.common.page;
        let mut floors = Vec::new();
        if page == FieldPage::Profiles && !self.common.two_d {
            floors.push((
                "the profiles' and points' floor",
                electric::TiledFloor {
                    fine: self.tile,
                    ..Default::default()
                }
                .tiles(self.common.width),
            ));
        }
        if page == FieldPage::Cuts && !self.common.span_pick.cut_places(model, spans).0.is_empty() {
            floors.push((
                "a cut's floor",
                electric::TiledFloor {
                    fine: self.cut_tile,
                    coarse: self.cut_tile.max(8.0),
                    ..Default::default()
                }
                .tiles(self.common.cut_width),
            ));
        }
        if let (Some(v), FieldPage::Volumes) = (&self.common.volume, page) {
            let width = v.width;
            let runs = self.common.span_pick.volume_runs(model, spans);
            if let Some(longest) = runs.iter().map(|r| r.1).max() {
                let st = self.piece_stats(model);
                let (_, tiles) = electric::volume_parts(
                    st.span() * longest as f64,
                    width,
                    self.volume_tile,
                    self.volume_margin,
                );
                floors.push(("the longest volume's floor", tiles));
            }
        }
        floors
            .into_iter()
            .find(|(_, t)| electric::floor_bytes(*t) > limit)
            .map(|(what, t)| {
                format!(
                    "{what} would need {:.1} GB, more than the {:.1} GB one calculation may take \
                     (Line › Calculation options): coarser tiles, a smaller width or reach, or \
                     the plane under each point",
                    electric::floor_bytes(t) / 1e9,
                    limit / 1e9
                )
            })
    }

    pub(super) fn sizes_ui(&mut self, ui: &mut egui::Ui, model: &Model, spans: &Spans) {
        let st = self.piece_stats(model);
        let (rate, rate_from) = self.rate();
        let pieces = match self.common.piece_length {
            Some(l) if l > 0.0 => (st.length / l).max(st.count as f64),
            _ => st.count as f64,
        };
        let n_levels = field_overlay::shown(&self.common.levels).len().max(1) as f64;
        // The cuts as the engine places them (on the alignment); why the
        // others cannot be.
        let (placed, unplaced) = self.common.span_pick.cut_places(model, spans);
        let cuts = placed.len();
        let runs = self.common.span_pick.volume_runs(model, spans);
        let ticked: usize = runs.iter().map(|r| r.1).sum();
        egui::Grid::new("electric-field-sizes")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // The cuts' page: the cuts; the volumes' page: the volumes.
                if self.common.page == FieldPage::Cuts {
                    ui.label("Cut size");
                    ui.horizontal(|ui| {
                        ui.label("±");
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.common.cut_width)
                                .range(5.0..=500.0)
                                .speed(1.0)
                                .suffix(" m"),
                        )
                        .on_hover_text(
                            "The cut's half width across the line: wider, more points (and a larger \
                             floor under it)",
                        );
                        ui.label("every");
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.common.cut_step)
                                .range(0.05..=10.0)
                                .speed(0.05)
                                .suffix(" m"),
                        )
                        .on_hover_text(
                            "The step between the cut's points: finer, isolines closer to their \
                             levels, four times the points at half the step",
                        );
                        ui.label("up to");
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.common.cut_above)
                                .range(0.0..=100.0)
                                .speed(0.5)
                                .suffix(" m"),
                        )
                        .on_hover_text(
                            "How far over the highest wire the cut reaches: higher, more points",
                        );
                        ui.label("over the highest wire");
                    });
                    ui.end_row();
                    let points =
                        (2.0 * self.common.cut_width / self.common.cut_step + 1.0) * (70.0 / self.common.cut_step + 1.0);
                    ui.label("");
                    crate::check_help::explain(
                        ui,
                        &format!(
                            "{cuts} cut(s) asked: ≈ {points:.0} points each, {} per cut ({rate_from})",
                            duration(points * pieces * rate)
                        ),
                        "Each cut asked in the table (+ station) is a vertical plane across its \
                         span, its isolines at the levels (5); Results › Open cut shows it on the \
                         wires. The cut is a vertical plane at the alignment station of its place in the \
                         span (between its structures' stations), perpendicular to the alignment — \
                         at an angle point along the bisector — as a cross-section panel opened \
                         there; from the lowest ground to this far over the highest wire; on the tiled floor \
                         its own floor of the cut tiles (Ground).",
                    );
                    ui.end_row();
                    for why in &unplaced {
                        ui.label("");
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("No cut placed: {why}."),
                        );
                        ui.end_row();
                    }
                    // The volumes: their spans ticked in the table.
                } else {
                    ui.label("Volume size");
                    ui.horizontal(|ui| {
                        let v = self.common.volume.get_or_insert(VolumeSetup {
                            step: 2.0,
                            width: self.common.width,
                        });
                        ui.label(format!(
                            "{ticked} span(s) with Volume ticked ({} volume(s)), ±",
                            runs.len(),
                        ));
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut v.width)
                                .range(1.0..=2000.0)
                                .speed(1.0)
                                .suffix(" m"),
                        )
                        .on_hover_text(
                            "The volume's half width across the line: wider, more points and a \
                             larger floor",
                        );
                        ui.label("every");
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut v.step)
                                .range(0.25..=20.0)
                                .speed(0.05)
                                .suffix(" m"),
                        )
                        .on_hover_text(
                            "The lattice step: halving it takes 4 to 8 times as long and places the \
                             surfaces closer to their levels",
                        );
                    });
                    ui.end_row();
                    if let (Some(v), true) = (&self.common.volume, ticked > 0) {
                        let n = ticked as f64;
                        let full =
                            n * (st.span() / v.step) * (2.0 * v.width / v.step) * (70.0 / v.step);
                        let share = self.common.volume_share.unwrap_or(0.5);
                        let t = full * share * pieces * rate;
                        ui.label("");
                        crate::check_help::explain(
                            ui,
                            &format!(
                                "{n:.0} span(s) of ≈ {:.0} m: ≈ {:.1} M points, ≈ {:.0} % computed \
                                 ({}), {} ({rate_from}); {} levels",
                                st.span(),
                                full / 1e6,
                                share * 100.0,
                                if self.common.volume_share.is_some() {
                                    "as in the last run"
                                } else {
                                    "a guess until a run"
                                },
                                duration(t),
                                n_levels
                            ),
                            "Halving the step takes about 4 to 8 times as long and places the \
                             surfaces closer to their levels; thin surfaces around the conductors \
                             need a fine step. Results › Show in 3D turns the 3D view to it. A \
                             coarse lattice first (4 steps apart), then the fine one only where a \
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
                            let width = self.common.volume.as_ref().map_or(self.common.width, |v| v.width);
                            let length = st.span() * longest as f64;
                            // No tile whose floor is too large (owner,
                            // 2026-11-12): the field's least, and the tile
                            // raised to it.
                            let fits = |t| {
                                electric::volume_parts(length, width, t, self.volume_margin).1
                            };
                            self.volume_tile_min =
                                electric::smallest_tile(2.0, 32.0, fits).unwrap_or(32.0);
                            if self.volume_tile < self.volume_tile_min {
                                self.volume_tile = self.volume_tile_min;
                            }
                            let (parts, tiles) = electric::volume_parts(
                                length,
                                width,
                                self.volume_tile,
                                self.volume_margin,
                            );
                            let bytes = electric::floor_bytes(tiles);
                            let text = if parts == 1 {
                                format!(
                                    "Its floor: the longest volume ({longest} span(s)) ≈ {tiles} tiles \
                                     of {} m, ≈ {:.2} GB of memory while solved, {}.",
                                    self.volume_tile,
                                    bytes / 1e9,
                                    duration(self.floor_seconds(tiles))
                                )
                            } else {
                                format!(
                                    "The longest volume ({longest} span(s)) is too large for one floor: \
                                     {parts} floors, one per part of it, each ≈ {tiles} tiles of {} m \
                                     or fewer, ≈ {:.2} GB while solved, {} in all.",
                                    self.volume_tile,
                                    bytes / 1e9,
                                    duration(parts as f64 * self.floor_seconds(tiles))
                                )
                            };
                            ui.label("");
                            if bytes > electric::floor_memory() {
                                crate::menu::error_label(
                                    ui,
                                    format!(
                                        "{text} Over the {:.0} GB a floor may take: coarser tiles \
                                         under the volumes or a smaller margin (Ground), a smaller \
                                         width, or the plane under each point.",
                                        electric::floor_memory() / 1e9
                                    ),
                                );
                            } else {
                                crate::check_help::explain(
                                    ui,
                                    &text,
                                    &format!(
                                    "A volume's floor runs along its whole run of spans and the margin \
                                     beyond (Ground) while it fits in {:.1} GB (Line › Calculation \
                                     options): its memory grows with the \
                                     run's length squared, its time faster still. A longer run is \
                                     floored in parts as long as the margin, each part's floor over \
                                     the ground within the margin of it (a line angle's other leg \
                                     too), each point of the volume taking its part's floor: measured \
                                     on level ground as close to the exact field as one floor (8 m \
                                     tiles: 2.8 % against 3.1 %).",
                                    electric::floor_memory() / 1e9
                                    ),
                                );
                            }
                            ui.end_row();
                        }
                    }
                }
            });
    }

    /// What the run will compute and about how long it takes (owner,
    /// 2026-10-31: a summary beside Run): the spans ticked, the profiles,
    /// the cuts, the volumes.
    pub(super) fn summary(&mut self, model: &Model) -> String {
        let spans = self.common.spans(model);
        let n = self.common.span_pick.chosen(model, &spans).0.len();
        let page = self.common.page;
        let profiles = match (page == FieldPage::Profiles, &self.common.stations) {
            (false, _) => 0,
            (true, Stations::EverySpan) => n,
            (true, Stations::Beside { .. }) => 2 * n,
            (true, Stations::One { .. }) => 1,
        };
        let (cuts, volumes) = match page {
            FieldPage::Cuts => (
                self.common.span_pick.cuts_asked(model, &spans).len()
                    + self.common.buildings.as_ref().map_or(0, |b| b.cuts.len()),
                0,
            ),
            FieldPage::Volumes => (0, self.common.span_pick.volume_runs(model, &spans).len()),
            FieldPage::Profiles => (0, 0),
        };
        let given = match page {
            FieldPage::Profiles => self.common.points.as_ref().map_or(0, |p| p.1.len()),
            _ => 0,
        };
        let st = self.piece_stats(model);
        let (rate, _) = self.rate();
        let pieces = match self.common.piece_length {
            Some(l) if l > 0.0 => (st.length / l).max(st.count as f64),
            _ => st.count as f64,
        };
        let across = (2.0 * self.common.width / self.common.step.max(0.01)).round() + 1.0;
        let cut_points = (2.0 * self.common.cut_width / self.common.cut_step + 1.0)
            * (70.0 / self.common.cut_step + 1.0);
        let mut seconds =
            (profiles as f64 * across + given as f64 + cuts as f64 * cut_points) * pieces * rate;
        if self.tiled {
            let floor = electric::TiledFloor {
                fine: self.tile,
                ..Default::default()
            };
            seconds += profiles as f64 * self.floor_seconds(floor.tiles(self.common.width));
            seconds += cuts as f64
                * self.floor_seconds(
                    electric::TiledFloor {
                        fine: self.cut_tile,
                        coarse: self.cut_tile.max(8.0),
                        ..Default::default()
                    }
                    .tiles(self.common.cut_width),
                );
        }
        if let Some(v) = &self.common.volume
            && volumes > 0
        {
            let ticked: usize = self
                .common
                .span_pick
                .volume_runs(model, &spans)
                .iter()
                .map(|r| r.1)
                .sum();
            let full =
                ticked as f64 * (st.span() / v.step) * (2.0 * v.width / v.step) * (70.0 / v.step);
            seconds += full * self.common.volume_share.unwrap_or(0.5) * pieces * rate;
        }
        let points = if given > 0 {
            format!(" + {given} point(s)")
        } else {
            String::new()
        };
        match page {
            FieldPage::Profiles if self.common.two_d => format!(
                "Run: 2D profiles, {profiles} profile(s){points}, 2D only (infinite line \
                 charges) — in a moment"
            ),
            FieldPage::Profiles => format!(
                "Run: 2D profiles, {profiles} profile(s){points}, from the 3D model — {}",
                duration(seconds)
            ),
            FieldPage::Cuts => format!(
                "Run: 2D cuts, {cuts} cut(s), from the 3D model — {}",
                duration(seconds)
            ),
            FieldPage::Volumes => {
                format!(
                    "Run: 3D volumes, {volumes} volume(s) — {}",
                    duration(seconds)
                )
            }
        }
    }

    /// The results of the views: what each measured.
    pub(super) fn views_results_ui(
        &self,
        ui: &mut egui::Ui,
        m: &electric::ElectricField,
        meshes: &[(f64, Arc<Mesh>)],
    ) {
        let mut mesh_at = meshes.iter();
        let levels = field_overlay::shown(&self.common.levels);
        if m.volumes.is_empty()
            && self.common.panel.done.is_empty()
            && !self.common.panel.job.running()
            && self.common.panel.error.is_none()
        {
            return;
        }
        ui.separator();
        ui.strong("Views");
        self.common.panel.progress_ui(ui);
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
        for c in &self.common.panel.done {
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
        if let Some(e) = &self.common.panel.error {
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
        self.common.panel.job.poll();
        if let Some(r) = self.common.panel.job.result.take() {
            match r {
                Ok(list) => {
                    self.common.panel.done.extend(list);
                    self.common.panel.error = None;
                }
                Err(e) => self.common.panel.error = Some(e),
            }
            self.common.panel.generation += 1;
        }
        let (Some(step), Some((charges, radius, ground_model, cut_tile))) =
            (self.common.in_cuts, self.charges.clone())
        else {
            if !self.common.panel.done.is_empty() {
                self.common.panel.done.clear();
                self.common.panel.generation += 1;
            }
            return;
        };
        let run = self.common.job.generation;
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
        let before = self.common.panel.done.len();
        self.common.panel.done.retain(|c| keys.contains(&c.key));
        if self.common.panel.done.len() != before {
            self.common.panel.generation += 1;
        }
        let missing: Vec<PanelKey> = keys
            .into_iter()
            .filter(|k| !self.common.panel.done.iter().any(|c| c.key == *k))
            .collect();
        if missing.is_empty() || self.common.panel.job.running() {
            return;
        }
        // The ground as the run took it: the terrain's, else straight
        // between the structures' feet.
        let terrain = model.ground_for_checks();
        let feet = electric::feet_ground(&model.line);
        let levels: Vec<f64> = field_overlay::shown(&self.common.levels)
            .iter()
            .map(|l| l.value)
            .collect();
        let progress = Arc::new(field_views::Progress::default());
        self.common.panel.progress = Some(progress.clone());
        self.common.panel.job.start(ctx, move || {
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
        let levels = field_overlay::shown(&self.common.levels);
        let key = (
            self.common.open,
            self.common.job.generation,
            self.common.panel.generation,
            serde_json::to_string(&levels).unwrap_or_default(),
            self.common.fill,
            stale,
            format!("{:?}", self.common.hidden),
        );
        if self.common.drawn.as_ref() == Some(&key) {
            return None;
        }
        self.common.drawn = Some(key);
        let mut o = Overlay {
            stale,
            ..Default::default()
        };
        if !self.common.open {
            return Some(o);
        }
        if let Some(Ok(a)) = &self.common.job.result {
            // The meshes in order, each volume's levels one after another.
            let per = a
                .field
                .volumes
                .first()
                .map_or(1, |v| v.values.surfaces.len().max(1));
            for (i, (level, mesh)) in a.meshes.iter().enumerate() {
                if self.common.hidden.contains(&("Volume", i / per)) {
                    continue;
                }
                if let Some(l) = levels.iter().find(|l| l.value == *level) {
                    o.surfaces.push((l.rgb, mesh.clone()));
                }
            }
            // The cuts asked in the table (owner, 2026-10-31): drawn in
            // their planes, so a panel opened there (Open cut) shows them.
            for (k, c) in a.field.cuts.iter().enumerate() {
                if self.common.hidden.contains(&("Cut", k)) {
                    continue;
                }
                o.cuts.push(field_overlay::cut_drawing(
                    &c.values,
                    &levels,
                    self.common.fill,
                ));
            }
        }
        for c in &self.common.panel.done {
            o.cuts.push(field_overlay::cut_drawing(
                &c.values,
                &levels,
                self.common.fill,
            ));
        }
        Some(o)
    }
}
