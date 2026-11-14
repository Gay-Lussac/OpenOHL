// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Line › Structures › Staking table… (owner, 2026-11-14): every structure's
//! station, offset, line angle, spans, coordinates, ground and bearing,
//! and the feet of the typed structures ([`ohl_line::staking`]), from the
//! line as it is and the project's ground; CSV export. The model is not
//! changed.

use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_line::staking::{Staking, angle_text};

use crate::check_help as help;
use crate::check_ui::{Cell, Col};
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]): the line and
/// the ground.
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE_GROUND;

/// Its page of the user guide.
pub const GUIDE: &str = "user-guide/staking-table";

/// Which table is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Structures,
    Feet,
}

/// The Staking table window.
#[derive(Default)]
pub struct StakingWindow {
    /// Shown.
    pub open: bool,
    /// The last answer (or why there is none).
    pub result: Option<Result<Staking, String>>,
    running: Option<Receiver<Result<Staking, String>>>,
    tab: Tab,
    /// What the guide button did.
    status: String,
    /// What the last answer was computed from ([`crate::revision`]).
    stamp: crate::revision::Stamp,
    /// Counts the answers taken.
    arrived: u64,
    /// The table's rows, by the answers taken and the table.
    rows: crate::check_ui::CachedRows<(u64, Tab)>,
}

impl StakingWindow {
    /// Opens the window and computes.
    pub fn open(&mut self, model: &Model, ctx: &egui::Context) {
        self.open = true;
        self.start(model, ctx);
    }

    /// Whether a computation runs.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Computes the table of the model as it is now, in the background.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let line = model.line.clone();
        let types = model.types.as_ref().map(|(t, _)| t.clone());
        let ground = model.ground.clone();
        let (tx, rx) = channel();
        let c = ctx.clone();
        self.stamp.started(model.revision());
        std::thread::spawn(move || {
            let height = |x: f64, y: f64| ground.as_ref().and_then(|g| g.height(x, y));
            let mut t = ohl_line::staking::staking(&line, types.as_ref(), &height);
            if ground.is_none() {
                t.notes.insert(
                    0,
                    "no terrain surface in the project: the ground columns are empty".into(),
                );
            }
            let _ = tx.send(Ok(t));
            c.request_repaint();
        });
        self.running = Some(rx);
    }

    /// Takes the answer, when it came.
    pub(crate) fn poll(&mut self) {
        let Some(rx) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("the computation stopped".into())
            }
        };
        self.result = Some(r);
        self.arrived += 1;
        self.stamp.arrived();
        self.running = None;
    }

    /// What the last answer was computed from that changed since, in
    /// words (`None`: nothing, or no answer).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.result, Some(Ok(_)))
            .then(|| self.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The window, when open.
    pub fn show(&mut self, ctx: &egui::Context, model: &Model) {
        if !self.open {
            return;
        }
        self.poll();
        let mut open = true;
        help::window(
            ctx,
            crate::dockable::Panel::Staking,
            crate::dockable::Panel::Staking.name(),
            [1180.0, 700.0],
            &mut open,
            |ui| {
                crate::check_ui::results_area(ui, "staking-window", |ui| self.ui(ui, model));
                help::footer(ui, "staking-window", |ui| {
                    let f = crate::check_ui::RunFooter {
                        running: self.running().then(crate::check_ui::Progress::default),
                        hover: "From the line and the ground as they are now",
                        ..Default::default()
                    };
                    match crate::check_ui::run_footer(ui, f, |_| {}) {
                        crate::check_ui::Action::Run => self.start(model, ui.ctx()),
                        crate::check_ui::Action::Stop => self.running = None,
                        crate::check_ui::Action::None => {}
                    }
                });
            },
        );
        self.open = open;
    }

    fn ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                "Where each structure is set out: its station along the alignment, its \
                 coordinates and the ground there, its bearing; the feet of the typed \
                 structures.",
            );
            help::question(
                ui,
                "Station and offset: along and from the alignment, horizontally (offset \
                 positive to the right looking ahead). Line angle: R turning right, L left. \
                 Spans: horizontal, to the structures before and after it on the \
                 alignment. Elevation: the structure's centre at the ground as placed; \
                 Ground: the project's terrain surface there; Above ground: their \
                 difference. Bearing: the direction ahead, degrees clockwise from grid north \
                 (the cross-arms square to it). Feet: the supported joints of the type (with \
                 the structure's extension), legs A to D by quarter (A right-ahead, B \
                 left-ahead, C left-back, D right-back). Coordinates in the line's CRS.",
            );
            if ui
                .small_button("Staking table: user guide ↗")
                .on_hover_text("Help › User guide: Staking table")
                .clicked()
            {
                self.status = match crate::help::open_section(GUIDE, "staking-table") {
                    Ok(_) => String::new(),
                    Err(e) => format!("The user guide did not open: {e}"),
                };
            }
        });
        if !self.status.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, &self.status);
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Structures, "Structures");
            ui.selectable_value(&mut self.tab, Tab::Feet, "Feet");
        });
        ui.separator();
        if let Some(what) = self.out_of_date(model)
            && crate::revision::banner(ui, &what, !self.running())
        {
            self.start(model, ui.ctx());
        }
        match &self.result {
            None => {
                ui.label("Computing…");
            }
            Some(Err(e)) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            Some(Ok(t)) => {
                let key = (self.arrived, self.tab);
                let tab = self.tab;
                let rows = self.rows.get_or_build(key, || match tab {
                    Tab::Structures => structure_rows(t),
                    Tab::Feet => foot_rows(t),
                });
                match tab {
                    Tab::Structures => {
                        crate::check_ui::ResultTable::new(
                            "staking-structures",
                            vec![
                                Col::text("Alignment"),
                                Col::text("Structure"),
                                Col::text("Type"),
                                Col::text("Ext."),
                                Col::num("Station (m)"),
                                Col::num("Offset (m)").tip("Positive to the right looking ahead"),
                                Col::text("Line angle"),
                                Col::num("Back span (m)"),
                                Col::num("Ahead span (m)"),
                                Col::num("Easting (m)"),
                                Col::num("Northing (m)"),
                                Col::num("Elevation (m)")
                                    .tip("The structure's centre at the ground"),
                                Col::num("Ground (m)").tip("The project's terrain surface there"),
                                Col::num("Above ground (m)"),
                                Col::num("Height adj. (m)")
                                    .tip("The type's feet above the centre: foundations"),
                                Col::num("Bearing (°)")
                                    .tip("The direction ahead, clockwise from grid north"),
                            ],
                        )
                        .rows_kept(rows)
                        .no_filter()
                        .group_by(&[0])
                        .csv("staking-structures.csv")
                        .full_csv(
                            "staking-structures-full.csv",
                            "Every structure with every field, as `ohl staking --csv` writes it",
                            || t.csv(),
                        )
                        .below(70.0)
                        .ui(ui);
                    }
                    Tab::Feet => {
                        if t.feet.is_empty() {
                            ui.label(
                                "No feet: no typed structure whose type has supported joints \
                                 (level 3).",
                            );
                        } else {
                            crate::check_ui::ResultTable::new(
                                "staking-feet",
                                vec![
                                    Col::text("Structure"),
                                    Col::text("Joint"),
                                    Col::text("Leg"),
                                    Col::num("Right (m)").tip("In the structure's axes"),
                                    Col::num("Ahead (m)").tip("In the structure's axes"),
                                    Col::num("Easting (m)"),
                                    Col::num("Northing (m)"),
                                    Col::num("Elevation (m)"),
                                    Col::num("Ground (m)"),
                                    Col::num("Above ground (m)").tip("Negative: below the ground"),
                                ],
                            )
                            .rows_kept(rows)
.no_filter()
                            .group_by(&[0])
                            .csv("staking-feet.csv")
                            .full_csv(
                                "staking-feet-full.csv",
                                "Every foot with every field, as `ohl staking --feet-csv` writes it",
                                || t.feet_csv(),
                            )
                            .below(70.0)
                            .ui(ui);
                        }
                    }
                }
                for n in &t.notes {
                    ui.label(egui::RichText::new(n).small().weak());
                }
            }
        }
    }
}

/// The structures' rows.
fn structure_rows(t: &Staking) -> Vec<crate::check_ui::Row> {
    t.rows
        .iter()
        .map(|r| {
            let alignment = if r.alignment.is_empty() {
                "(no alignment)".to_owned()
            } else {
                r.alignment.clone()
            };
            crate::check_ui::Row::new(
                vec![
                    Cell::text(alignment),
                    Cell::text(&r.structure),
                    Cell::text(r.structure_type.as_deref().unwrap_or("")),
                    Cell::text(r.extension.as_deref().unwrap_or("")),
                    Cell::num(r.station, 2),
                    Cell::num(r.offset, 2),
                    Cell::sorted(angle_text(r.line_angle_deg), r.line_angle_deg),
                    Cell::num(r.back_span, 2),
                    Cell::num(r.ahead_span, 2),
                    Cell::num(Some(r.position[0]), 3),
                    Cell::num(Some(r.position[1]), 3),
                    Cell::num(Some(r.position[2]), 3),
                    Cell::num(r.ground, 3),
                    Cell::num(r.above_ground(), 3),
                    Cell::num(Some(r.height_adjust), 3),
                    Cell::num(Some(r.bearing_deg), 2),
                ],
                false,
            )
        })
        .collect()
}

/// The feet's rows.
fn foot_rows(t: &Staking) -> Vec<crate::check_ui::Row> {
    t.feet
        .iter()
        .map(|p| {
            crate::check_ui::Row::new(
                vec![
                    Cell::text(&p.structure),
                    Cell::text(&p.joint),
                    Cell::text(&p.leg),
                    Cell::num(Some(p.local[0]), 3),
                    Cell::num(Some(p.local[1]), 3),
                    Cell::num(Some(p.position[0]), 3),
                    Cell::num(Some(p.position[1]), 3),
                    Cell::num(Some(p.position[2]), 3),
                    Cell::num(p.ground, 3),
                    Cell::num(p.above_ground(), 3),
                ],
                false,
            )
        })
        .collect()
}
