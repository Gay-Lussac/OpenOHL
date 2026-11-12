// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Electric field…** (roadmap O18 step 5, owner
//! 2026-10-29; built, in active development, not approved yet): the
//! electric field of the line's voltages ([`ohl_electrical::electric`]) in
//! a weather case — a voltage per circuit (the system's nominal unless
//! given; 0: voltage-free), 3D or 2D, the jumpers, the ground plane's
//! radius, the profiles' height and width, the stations, an optional
//! limit, points from a CSV — and on Run the charges per wire, the
//! profiles plotted (E against the distance from the axis), a table per
//! profile and per point with the limit's margin, the notes; Export CSV.
//! The model's pieces, the levels drawn and the views (the cross-section
//! panels, volumes) in [`views`], as for the magnetic field. Out of date
//! like the other checks when the line or the ground changes. Not yet:
//! isolines from above.
//!
//! The window's inputs are kept in the project, per project line
//! ([`SETTING`]), written when a run starts or the window closes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;
use ohl_electrical::electric::{self, CircuitVoltage, ElectricField, Settings};
use ohl_electrical::magnetic::{self, Model as FieldModel, Progress, StationSpec};
use ohl_line::electrical::SystemKind;

#[path = "electric_views.rs"]
pub mod views;

use crate::check_help as help;
use crate::check_ui;
use crate::field_common::{
    self, FieldShared, FieldStart, KeepStep, Spans, Stations, StoredCommon, beside_stations,
};
use crate::field_overlay::Level;
use crate::field_results::{self, ResultRow, RowAction};
use crate::job::save_csv;
#[cfg(test)]
use crate::magnetic_field::views::VolumeSetup;
use crate::model::Model;
use ohl_session::reports::CheckReport;

/// The answer: the field and the weather case.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The field.
    pub field: ElectricField,
    /// The weather case's name.
    pub case: String,
    /// The 3D model's charges (for the cross-section panels' cuts).
    pub charges: Option<Arc<electric::Charges3d>>,
    /// The ground plane's radius the run took, m.
    pub radius: f64,
    /// The ground model the run took.
    pub ground_model: electric::GroundModel,
    /// The volumes' surfaces (level, mesh), in their order.
    pub meshes: Vec<(f64, Arc<ohl_electrical::contour::Mesh>)>,
    /// The points computed and the pieces, for the speed.
    pub work: f64,
    /// The conductor surface gradients of the 2D cross-sections (owner,
    /// 2026-11-04: 2D only), when asked.
    pub gradient: Option<ohl_electrical::surface_gradient::SurfaceGradients>,
}

/// A duration in words (the magnetic window's).
fn magnetic_duration(s: f64) -> String {
    crate::magnetic_field::views::duration(s)
}

/// The structures' steel for the electric field (owner, 2026-10-31): a
/// structure type's members and pole at levels 2 and 3 as placed (each
/// member's radius from its section's solid, else 5 cm), else the
/// structure's own stick drawing (5 cm); the structures with neither
/// (level 1) left out, by id.
pub(crate) fn steel_of(model: &Model) -> (Vec<electric::SteelMember>, Vec<String>) {
    const DEFAULT_RADIUS: f64 = 0.05;
    let mut steel = Vec::new();
    let mut left_out = Vec::new();
    for st in &model.line.structures {
        let member = |points: Vec<[f64; 3]>, radius: f64| electric::SteelMember {
            structure: st.id.clone(),
            points,
            radius,
        };
        if let Some(b) = crate::scene::drawn_body(&model.bodies, st) {
            let at = crate::scene::type_origin(st);
            for (points, radius) in b.placed_steel(at, st.orientation_deg, DEFAULT_RADIUS) {
                steel.push(member(points, radius));
            }
        } else if !st.members.is_empty() {
            for m in &st.members {
                steel.push(member(m.clone(), DEFAULT_RADIUS));
            }
        } else {
            left_out.push(st.id.clone());
        }
    }
    (steel, left_out)
}

/// The default levels, kV/m: display only (no limit).
pub fn default_levels() -> Vec<Level> {
    vec![
        Level {
            value: 1.0,
            rgb: [70, 150, 255],
            on: true,
        },
        Level {
            value: 5.0,
            rgb: [255, 170, 40],
            on: true,
        },
        Level {
            value: 10.0,
            rgb: [230, 50, 50],
            on: false,
        },
    ]
}

/// The Electric field window.
pub struct ElectricFieldWindow {
    /// What both field windows have (reached through `Deref` from outside).
    pub common: FieldShared<Answer>,

    /// The voltage between phases given per circuit (by its label), kV;
    /// a circuit not here: its system's nominal.
    voltages: BTreeMap<String, f64>,
    /// The ground plane under a point is fitted over this distance, m.
    ground_radius: f64,
    /// The ground a tiled conducting floor (the default; owner,
    /// 2026-10-29), else a plane under each point.
    tiled: bool,
    /// The structures' steel earthed (owner, 2026-10-31: off by default).
    steel: bool,
    /// The steel counted, by line version: structures with members, their
    /// members, pieces of steel, structures left out.
    steel_seen: Option<(u64, [usize; 4])>,
    /// The floor's fine tiles, m.
    tile: f64,
    /// The tiles under the cuts across the line, m (coarser by default;
    /// owner, 2026-10-29).
    cut_tile: f64,
    /// The tiles under the volumes, m.
    volume_tile: f64,
    /// The smallest tile under the volumes whose floor fits the memory a
    /// floor may take (worked out with the volumes' estimate; owner,
    /// 2026-11-12: no choice that is refused or too large).
    volume_tile_min: f64,
    /// The volumes' floor margin, m (2026-11-01).
    volume_margin: f64,
    /// 3D: the stretches each span is cut into (2026-11-01).
    stretches: usize,
    /// 2D: the conductor surface gradients at the spans' lowest points and
    /// ends too (owner, 2026-11-04: "only for the 2D cuts for the noise").
    gradient: bool,
    /// The floors' solves of the last run against their expected time.
    floor_speed: Option<f64>,
    /// The last run's charges (3D) and its ground plane's radius.
    charges: Option<(Arc<electric::Charges3d>, f64, electric::GroundModel, f64)>,
}

impl std::ops::Deref for ElectricFieldWindow {
    type Target = FieldShared<Answer>;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

impl std::ops::DerefMut for ElectricFieldWindow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.common
    }
}

impl Default for ElectricFieldWindow {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            common: FieldShared::new(FieldStart {
                jumpers: d.jumpers,
                height: d.height,
                width: d.width,
                step: d.step,
                levels: default_levels(),
                volume_width: 50.0,
                cut_width: 60.0,
            }),
            voltages: BTreeMap::new(),
            ground_radius: d.ground_radius,
            tiled: true,
            steel: false,
            steel_seen: None,
            tile: 2.0,
            cut_tile: 8.0,
            volume_tile: 8.0,
            volume_tile_min: 2.0,
            volume_margin: electric::VOLUME_MARGIN,
            stretches: electric::STRETCHES,
            gradient: true,
            floor_speed: None,
            charges: None,
        }
    }
}

/// The project setting holding the window's inputs (per project line:
/// `app.electric_field.<line>`).
pub const SETTING: &str = "app.electric_field";

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "electric-field-window";

/// The window's own page of the user guide (owner, 2026-11-12: the
/// field windows reviewed as the vertical clearances were); its headings'
/// **user guide ↗** open its sections.
pub const GUIDE: &str = "user-guide/electric-field";

/// The window's inputs as kept in the project (JSON).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Stored {
    /// What both field windows keep.
    #[serde(flatten)]
    common: StoredCommon,

    /// kV between phases per circuit label (absent: the nominal).
    voltages: BTreeMap<String, f64>,
    ground_radius: Option<f64>,
    tiled: Option<bool>,
    /// The structures' steel earthed (2026-10-31).
    steel: bool,
    tile: Option<f64>,
    cut_tile: Option<f64>,
    volume_tile: Option<f64>,
    /// The volumes' floor margin, m (2026-11-01).
    volume_margin: Option<f64>,
    /// 3D: the stretches per span (2026-11-01; absent: the default).
    stretches: Option<usize>,
    /// 2D: the surface gradients too (2026-11-04; absent: yes).
    gradient: Option<bool>,
}

/// What the voltages' grid says of a circuit at its system's nominal
/// voltage.
pub const NOMINAL_VOLTAGE_LABEL: &str = "the system's nominal — not on the safe side";

/// Its hover text: the effect and what to do.
pub const NOMINAL_VOLTAGE_HELP: &str = "No highest operating voltage is given for this system and \
     none is typed here: the field is computed at the nominal voltage. The highest operating \
     voltage gives a higher field, in proportion — the result says NOT ON THE SAFE SIDE. Give \
     it in Line › Sections › Electrical systems (max kV), or type the voltage here.";

/// What the grid says of a circuit at its system's highest operating
/// voltage.
pub const HIGHEST_VOLTAGE_LABEL: &str = "the system's highest operating";

impl ElectricFieldWindow {
    /// The inputs as kept in the project.
    fn stored(&mut self, model: &Model) -> Stored {
        Stored {
            common: self.common.stored_common(model),
            voltages: self.voltages.clone(),
            ground_radius: Some(self.ground_radius),
            tiled: Some(self.tiled),
            steel: self.steel,
            tile: Some(self.tile),
            cut_tile: Some(self.cut_tile),
            volume_tile: Some(self.volume_tile),
            volume_margin: Some(self.volume_margin),
            stretches: Some(self.stretches),
            gradient: Some(self.gradient),
        }
    }

    /// Reads the inputs from the project when not read yet for the line
    /// worked on, and writes them into it when they changed — once a run
    /// starts or the window closes, not while a value is dragged or typed.
    /// A project this user cannot change keeps them for the session.
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        match self.common.keep_step(model, ctx, SETTING) {
            KeepStep::Nothing => {}
            KeepStep::Read => self.load(model),
            KeepStep::Write(key) => {
                let stored = self.stored(model);
                self.common
                    .write_inputs(model, key, &stored, "electric field inputs");
            }
        }
    }

    /// Takes the inputs kept in the project for the line worked on; what
    /// is no longer there (a case, a span) falls back to the default.
    fn load(&mut self, model: &Model) {
        let Some(session) = model.session() else {
            return;
        };
        let key = format!("{SETTING}.{}", session.current_line());
        let text = session.setting(&key).ok().flatten();
        let st: Stored = text
            .as_deref()
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default();
        self.take(model, st);
        self.common.saved = serde_json::to_string(&self.stored(model))
            .ok()
            .map(|json| (key, json));
    }

    /// Takes inputs kept (in the project or with a saved run).
    fn take(&mut self, model: &Model, st: Stored) {
        self.common.take_common(model, st.common);
        let d = Self::default();
        self.voltages = st.voltages;
        self.ground_radius = st.ground_radius.unwrap_or(d.ground_radius);
        self.tiled = st.tiled.unwrap_or(d.tiled);
        self.steel = st.steel;
        self.tile = st.tile.unwrap_or(d.tile);
        self.cut_tile = st.cut_tile.unwrap_or(d.cut_tile);
        self.volume_tile = st.volume_tile.unwrap_or(d.volume_tile);
        self.volume_margin = st.volume_margin.unwrap_or(d.volume_margin);
        self.stretches = st.stretches.unwrap_or(d.stretches);
        self.gradient = st.gradient.unwrap_or(d.gradient);
    }

    /// The steel counted ([`steel_of`]): structures with members, members,
    /// pieces of steel, structures left out — once per version of the line.
    pub(super) fn steel_counts(&mut self, model: &Model) -> [usize; 4] {
        if let Some((v, c)) = self.steel_seen
            && v == model.line_version
        {
            return c;
        }
        let (steel, left_out) = steel_of(model);
        let structures: std::collections::BTreeSet<&str> =
            steel.iter().map(|m| m.structure.as_str()).collect();
        let pieces: usize = steel
            .iter()
            .flat_map(|m| m.points.windows(2))
            .map(|w| {
                let l = ((w[1][0] - w[0][0]).powi(2)
                    + (w[1][1] - w[0][1]).powi(2)
                    + (w[1][2] - w[0][2]).powi(2))
                .sqrt();
                (l / electric::STEEL_PIECE).ceil().max(1.0) as usize
            })
            .sum();
        let c = [structures.len(), steel.len(), pieces, left_out.len()];
        self.steel_seen = Some((model.line_version, c));
        c
    }

    /// About how long a floor of `tiles` takes here, s: as measured on
    /// the floors of the last run (as expected before one).
    fn floor_seconds(&self, tiles: usize) -> f64 {
        electric::TiledFloor::seconds(tiles) * self.floor_speed.unwrap_or(1.0)
    }

    /// The ground a tiled floor or a plane under each point (tests).
    #[cfg(test)]
    pub fn set_tiled(&mut self, on: bool) {
        self.tiled = on;
    }

    /// Gives a circuit's voltage between phases, kV (tests).
    #[cfg(test)]
    pub fn set_voltage(&mut self, circuit: &str, kv: f64) {
        self.voltages.insert(circuit.to_owned(), kv);
    }

    /// A circuit's voltage given, kV (tests).
    #[cfg(test)]
    pub fn voltage_of(&self, circuit: &str) -> Option<f64> {
        self.voltages.get(circuit).copied()
    }

    /// Ticks span `k` only (tests).
    #[cfg(test)]
    pub fn tick_only(&mut self, model: &Model, k: usize) {
        let spans = self.common.spans(model);
        self.common.span_pick.only(model, &spans, k);
        // Its volume ticked too (the tests' volume over one span).
        self.common.span_pick.volume.insert(spans[k].clone());
    }

    /// The modelling settings of the inputs (owner, 2026-11-12: in 4
    /// Model, folded, with the steel and the wire pieces): the profiles'
    /// model, the jumpers, the stretches per span, the ground and its
    /// tiles.
    pub(super) fn model_inputs_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        egui::Grid::new("electric-field-model-inputs")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Profiles by");
                ui.add_enabled_ui(self.common.planes && self.common.profiles, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.common.two_d, false, "3D")
                            .on_hover_text(
                                "The wires as solved (sag, line angles, the line's length) and the \
                         jumpers as pieces of uniform charge over the ground (a tiled \
                         conducting floor by default, 4 Model)",
                            );
                        ui.selectable_value(&mut self.common.two_d, true, "2D")
                        .on_hover_text(
                            "Infinite line charges where the wires cross the profile, over level \
                         ground at the cross-section: not valid near a line angle, a structure \
                         or on sloping ground",
                        );
                    })
                });
                ui.end_row();
                ui.label("");
                help::explain(
                    ui,
                    if self.common.two_d {
                        "2D: a profile's points in a moment"
                    } else {
                        "3D: the profiles' time below"
                    },
                    "2D: the sag along the span, the line angles and the jumpers not seen \
                     (under the lowest point close to 3D on a long level span). 3D: every point \
                     sums every piece of charge twice (the charges and their images). Both take \
                     the charges of the cross-section where they are (3D: in stretches, 24 by \
                     default). 3D sees what the span does along it; 2D assumes every span \
                     infinitely long and straight, its ground level.",
                );
                ui.end_row();
                {
                    ui.label("Jumpers");
                    ui.checkbox(&mut self.common.jumpers, "carry their phase's charge")
                        .on_hover_text(
                            "A jumper at a section's end at that section's charge per metre, as \
                             solved in the weather case (3D). Off: left out (not conservative \
                             near the structures). Jumpers not at a section's end are left out, \
                             said",
                        );
                    ui.end_row();
                    // Each span's charge along it (owner, 2026-11-01: a
                    // parameter, its sensitivity in the theory).
                    ui.label("Stretches per span");
                    ui.horizontal(|ui| {
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut self.stretches)
                                .range(1..=128)
                                .speed(0.2),
                        )
                        .on_hover_text(
                            "3D: each span cut into this many stretches along it, each at the \
                             charge of its own cross-section (the charge follows the sag)",
                        );
                        help::explain(ui, "the time hardly changes", STRETCHES_EFFECT);
                    });
                    ui.end_row();
                    ui.label("Ground");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.tiled, true, "tiled floor");
                        ui.radio_value(&mut self.tiled, false, "plane under each point");
                        if self.tiled {
                            ui.label("tiles: profiles and points");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.tile)
                                    .range(1.0..=8.0)
                                    .speed(0.1)
                                    .suffix(" m"),
                            )
                            .on_hover_text(
                                "The tiles under the profiles and points: finer, a floor closer to the \
                                 terrain (4 m to 2 m moved the profiles 1–3 %), far longer (1 m about 8 \
                                 times as long)",
                            );
                            ui.label("cuts");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.cut_tile)
                                    .range(1.0..=32.0)
                                    .speed(0.1)
                                    .suffix(" m"),
                            )
                            .on_hover_text(
                                "The tiles under the cuts: finer, a closer floor, far longer",
                            );
                            ui.label("volumes");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.volume_tile)
                                    .range(self.volume_tile_min.max(2.0)..=32.0)
                                    .speed(0.1)
                                    .suffix(" m"),
                            )
                            .on_hover_text(format!(
                                "The tiles under the volumes. At least {} m here: a smaller \
                                 tile's floor would not fit the {:.0} GB a floor may take \
                                 (longest volume, its width and reach).",
                                self.volume_tile_min.max(2.0),
                                electric::floor_memory() / 1e9
                            ));
                            ui.label("reaching");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.volume_margin)
                                    .range(20.0..=1000.0)
                                    .speed(1.0)
                                    .suffix(" m"),
                            )
                            .on_hover_text(
                                "How far the floor under a volume reaches beyond it every way",
                            );
                        } else {
                            ui.label("fitted over ±");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.ground_radius)
                                    .range(0.5..=500.0)
                                    .speed(0.5)
                                    .suffix(" m"),
                            )
                            .on_hover_text(
                                "The plane under each point fitted over this distance: short, the \
                                 ground right under the point; long, the slope at large",
                            );
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    if self.tiled {
                        let floor = electric::TiledFloor {
                            fine: self.tile,
                            ..Default::default()
                        };
                        let tiles = floor.tiles(self.common.width);
                        let volume_width = self
                            .common
                            .volume
                            .as_ref()
                            .map_or(self.common.width, |v| v.width);
                        let floors = match self.common.stations {
                            Stations::EverySpan => {
                                let spans = self.common.spans(model);
                                self.common.span_pick.chosen(model, &spans).0.len()
                            }
                            Stations::Beside { .. } => {
                                let spans = self.common.spans(model);
                                2 * self.common.span_pick.chosen(model, &spans).0.len()
                            }
                            Stations::One { .. } => 1,
                        };
                        help::explain(
                            ui,
                            &format!(
                                "≈ {tiles} tiles, {} per floor, {floors} floor(s) for the \
                                 profiles; a cut ≈ {} tiles, {}; a volume over one span ≈ {} \
                                 tiles, {}",
                                magnetic_duration(self.floor_seconds(tiles)),
                                electric::TiledFloor {
                                    fine: self.cut_tile,
                                    coarse: self.cut_tile.max(8.0),
                                    ..Default::default()
                                }
                                .tiles(self.common.width),
                                magnetic_duration(
                                    self.floor_seconds(
                                        electric::TiledFloor {
                                            fine: self.cut_tile,
                                            coarse: self.cut_tile.max(8.0),
                                            ..Default::default()
                                        }
                                        .tiles(self.common.width)
                                    )
                                ),
                                electric::Floor::tiles_under(
                                    400.0,
                                    volume_width,
                                    self.volume_tile,
                                    self.volume_margin
                                ),
                                magnetic_duration(self.floor_seconds(
                                    electric::Floor::tiles_under(
                                        400.0,
                                        volume_width,
                                        self.volume_tile,
                                        self.volume_margin
                                    )
                                )),
                            ),
                            "The terrain as a conducting floor of tiles around each profile, \
                             group of points and cut (owner, 2026-10-29: \"keep the tiled \
                             implementation\"). It follows hollows and ridges: on a real line the \
                             plane read the largest field up to 8 % low on bumps and up to 24 % \
                             high in a hollow. From 4 m to 2 m tiles the profiles moved 1–3 %; 1 m \
                             tiles take about 8 times as long. Cuts across the line and volumes on \
                             coarser tiles by default (owner, 2026-10-29). Measured against the \
                             exact field on even ground, a volume's floor of 16 m tiles is 9.8 % \
                             off the largest value, of 8 m 2.9 %, of 4 m 0.5 % (some 20 times as \
                             long); 2 m tiles need the floor in parts and gain only with the full \
                             margin (with 50 m beyond: 4 m 1.2 %, 2 m 1.1 % — the margin then the \
                             limit; see the theory). Square tiles of the fine size within 30 m along the line and 10 m \
                             beyond the profiles across, 8 m beyond, to 200 m every way, each \
                             carrying a uniform charge solved so that the floor is at zero \
                             potential. Volumes take a floor of their own tiles along their \
                             whole run and 100 m beyond. The times at the speed of the last run's \
                             floors here (before one: as measured on a laptop). See the theory \
                             page, Electric field.",
                        );
                    } else {
                        help::explain(
                            ui,
                            "Quick; up to 8 % low on bumps: not on the safe side",
                            "The ground under each point a plane fitted over ± this distance, \
                             the charges' images in it: quick (no floor to solve), exact for \
                             level or evenly sloping ground; on a real line it read the largest \
                             field up to 8 % low on bumps (not conservative) and in a hollow up \
                             to 24 % high. A short distance follows the ground right under the point (a bank, \
                             a ditch), a long one the slope at large. See the theory page, \
                             Electric field.",
                        );
                    }
                    ui.end_row();
                }
            });
    }

    /// The conductor surface gradient asked (section 3: what is
    /// computed; 2D only).
    fn gradient_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("electric-field-gradient")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                // The conductor surface gradient (owner, 2026-11-04: 2D
                // only, for the noise).
                ui.label("Surface gradient");
                ui.add_enabled_ui(self.common.planes, |ui| {
                    ui.checkbox(
                        &mut self.gradient,
                        "2D, per span: lowest point and both ends",
                    )
                    .on_hover_text(GRADIENT_EFFECT);
                });
                ui.end_row();
            });
    }

    fn spans_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let spans = self.common.spans(model);
        self.common.spans_ui(ui, "electric-field", model, &spans);
        self.sizes_ui(ui, model, &spans);
        self.gradient_ui(ui);
    }

    /// The Results list (owner, 2026-10-31, validated): every profile, cut
    /// and volume of `a` with its largest value; Plot, Open cut, Show in
    /// 3D.
    fn output_rows(&self, model: &Model, a: &Answer) -> Vec<ResultRow> {
        let mut rows = Vec::new();
        for (k, p) in a.field.profiles.iter().enumerate() {
            rows.push(ResultRow {
                kind: "Profile",
                place: format!("{}–{} at {:.0} m", p.from, p.to, p.at),
                max: format!("{:.3} kV/m", p.max_kv_m),
                note: format!("at {:.0} m from the axis", p.max_offset),
                button: "Show plot",
                action: RowAction::Plot(k),
                drawn: None,
            });
        }
        for (k, c) in a.field.cuts.iter().enumerate() {
            let largest = c.values.values.iter().copied().fold(0.0, f64::max);
            rows.push(ResultRow {
                kind: "Cut",
                place: format!(
                    "{} at {:.0} m{} (span {}–{})",
                    c.alignment,
                    c.station,
                    if c.angle_point { ", angle point" } else { "" },
                    c.from,
                    c.to
                ),
                max: format!("{largest:.2} kV/m"),
                note: "at the wires; isolines at the levels".into(),
                button: "Open cut",
                action: field_results::open_cut(c, self.common.cut_width),
                drawn: Some(k),
            });
        }
        let per = a
            .field
            .volumes
            .first()
            .map_or(1, |v| v.values.surfaces.len().max(1));
        for (k, v) in a.field.volumes.iter().enumerate() {
            let meshes = a
                .meshes
                .iter()
                .skip(k * per)
                .take(per)
                .map(|m| m.1.as_ref());
            let levels: Vec<String> = v
                .values
                .surfaces
                .iter()
                .map(|s| format!("{}", s.0))
                .collect();
            let left: usize = v.values.left_out.iter().sum();
            rows.push(ResultRow {
                kind: "Volume",
                place: format!("{}–{}", v.from, v.to),
                max: format!("{} kV/m", levels.join(", ")),
                note: if left > 0 {
                    format!("{left} thin piece(s) left out")
                } else {
                    String::new()
                },
                button: "Show in 3D",
                action: match field_results::centre(meshes) {
                    Some(c) => {
                        field_results::look(model, &v.from, &v.to, c, v.values.bounds.path.length())
                    }
                    None => RowAction::None,
                },
                drawn: Some(k),
            });
        }
        rows
    }

    /// Sets the views (tests): the cross-section panels every `cuts` m, the
    /// volumes' step `volume` m (their spans: [`Self::tick_only`]).
    #[cfg(test)]
    pub fn set_views(&mut self, cuts: Option<f64>, volume: Option<f64>) {
        self.common.in_cuts = cuts;
        self.common.volume = volume.map(|step| VolumeSetup {
            step,
            width: self.common.width,
        });
        // A volume asked: 3D (2026-11-01: 3D makes the volumes only).
        self.common.planes = volume.is_none();
    }

    /// The views (tests): the panels' and the volume's step.
    #[cfg(test)]
    pub fn views(&self) -> (Option<f64>, Option<f64>) {
        (
            self.common.in_cuts,
            self.common.volume.as_ref().map(|v| v.step),
        )
    }

    /// The panels' cuts' values, kV/m (tests).
    #[cfg(test)]
    pub fn panel_values(&self) -> Vec<Arc<ohl_electrical::field_views::CutValues>> {
        self.common
            .panel
            .done
            .iter()
            .map(|c| c.values.clone())
            .collect()
    }

    /// A circuit's voltage between phases: given, else its system's
    /// nominal (none: DC, or no such system).
    fn voltage(&self, model: &Model, c: &ohl_electrical::parameters::CircuitKey) -> Option<f64> {
        self.voltage_used(model, c).map(|v| v.kv)
    }

    /// A circuit's voltage between phases as a run takes it, and where it
    /// comes from (owner, 2026-11-11): given here, else its system's
    /// highest operating voltage, else its nominal (none: DC, or no such
    /// system).
    pub fn voltage_used(
        &self,
        model: &Model,
        c: &ohl_electrical::parameters::CircuitKey,
    ) -> Option<ohl_electrical::electric::VoltageUsed> {
        let system = model.line.systems.iter().find(|s| s.id == c.system)?;
        if system.kind == SystemKind::Dc {
            return None;
        }
        Some(ohl_electrical::electric::circuit_voltage(
            system.voltage_kv,
            system.highest_voltage_kv,
            self.voltages.get(&c.label()).copied(),
        ))
    }

    /// The settings as the engine takes them.
    fn settings(&mut self, model: &Model) -> Settings {
        let spans = self.common.spans(model);
        let steel = if self.steel {
            steel_of(model)
        } else {
            (Vec::new(), Vec::new())
        };
        let stations = match &self.common.stations {
            Stations::EverySpan => match self.common.span_pick.chosen(model, &spans) {
                (_, true) => Vec::new(),
                (chosen, false) => chosen
                    .into_iter()
                    .map(|k| StationSpec {
                        from: spans[k].0.clone(),
                        to: spans[k].1.clone(),
                        fraction: None,
                    })
                    .collect(),
            },
            Stations::One { span, fraction } => spans
                .get(*span)
                .map(|(from, to)| StationSpec {
                    from: from.clone(),
                    to: to.clone(),
                    fraction: *fraction,
                })
                .into_iter()
                .collect(),
            Stations::Beside { distance } => {
                let (mut chosen, _) = self.common.span_pick.chosen(model, &spans);
                chosen.sort_unstable();
                beside_stations(model, &spans, &chosen, *distance)
            }
        };
        Settings {
            model: if self.common.two_d {
                FieldModel::TwoD
            } else {
                FieldModel::ThreeD
            },
            voltages: self
                .voltages
                .iter()
                .map(|(c, &kv)| CircuitVoltage {
                    circuit: c.clone(),
                    kv,
                })
                .collect(),
            height: self.common.height,
            width: self.common.width,
            step: self.common.step,
            stations,
            profiles: self.common.planes && self.common.profiles,
            // The CSV's points with the profiles; the buildings' places
            // in any 2D run.
            points: {
                let mut v = if self.common.planes && self.common.profiles {
                    self.common
                        .points
                        .as_ref()
                        .map(|p| p.1.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                if self.common.planes
                    && let Some(b) = &self.common.buildings
                {
                    v.extend(b.points());
                }
                v
            },
            limit_kv_m: self.common.limit,
            ground_radius: self.ground_radius,
            ground_model: if self.tiled {
                electric::GroundModel::Tiled(electric::TiledFloor {
                    fine: self.tile,
                    ..Default::default()
                })
            } else {
                electric::GroundModel::Plane
            },
            cut_tile: self.cut_tile,
            volume_tile: self.volume_tile,
            volume_margin: self.volume_margin,
            stretches: self.stretches,
            jumpers: self.common.jumpers,
            piece_length: self.common.piece_length,
            // The cuts asked in the table (owner, 2026-10-31: "for the 2D
            // cuts it should be asked at which stations we want one").
            cuts: if !self.common.planes {
                Vec::new()
            } else {
                self.common
                    .span_pick
                    .cut_places(model, &spans)
                    .0
                    .into_iter()
                    .map(|at| magnetic::CutSpec {
                        at,
                        width: self.common.cut_width,
                        above: self.common.cut_above,
                        step: self.common.cut_step,
                        label: None,
                    })
                    .chain(self.common.buildings.iter().flat_map(|b| b.cut_specs()))
                    .collect()
            },
            // A volume: one along each run of spans following each other
            // with Volume ticked (continuous across the line angles).
            volumes: match (&self.common.volume, self.common.planes) {
                (Some(v), false) => self
                    .common
                    .span_pick
                    .volume_runs(model, &spans)
                    .into_iter()
                    .map(|((from, to), _)| magnetic::VolumeSpec {
                        from,
                        to,
                        width: v.width,
                        above: crate::magnetic_field::views::ABOVE,
                        step: v.step,
                    })
                    .collect(),
                _ => Vec::new(),
            },
            levels: crate::field_preset::with_levels(
                crate::field_overlay::shown(&self.common.levels)
                    .iter()
                    .map(|l| l.value)
                    .collect(),
                self.common
                    .buildings
                    .as_ref()
                    .map(|b| b.levels_kv_m.as_slice()),
            ),
            steel: steel.0,
            steel_left_out: steel.1,
        }
    }

    /// Starts a run on the case's solved sections.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        // The inputs run with: kept in the project (the next frame).
        self.common.commit = true;
        let Some(solved) = model.cases.get(self.common.case) else {
            self.common.job.result = Some(Err("no weather case in the model".into()));
            return;
        };
        // Refused before anything starts when a floor would not fit
        // (owner, 2026-11-12), whichever way the run was asked for.
        let spans = self.common.spans(model);
        if let Some(why) = self.memory_block(model, &spans) {
            self.common.job.result = Some(Err(why));
            return;
        }
        // By the window's calculation: the case solved for the run.
        let line =
            ohl_clearance::calculation::line_for(&model.line, self.common.calculation).into_owned();
        let library = model.library.clone();
        let mut shapes = solved.shapes.clone();
        let mut jumpers = solved.jumpers.clone();
        let case = solved.case.clone();
        // A case the app has not solved yet (it solves a case when it is
        // shown): solved for the run, in the background (2026-10-31: the
        // run took no wire and gave 0 kV/m).
        let unsolved = !solved.solved_for(model.wind) || self.common.calculation.is_some();
        // The systems in cases or calculations of their own (2026-11-01).
        let plan = crate::field_systems::plan(
            model,
            &self.common.per_system,
            &solved.case,
            self.common.calculation,
        );
        let wind = model.wind;
        // The ground chosen (2026-11-01), read in the background.
        let ground_source = crate::field_ground::source(model, &self.common.ground_from);
        let settings = self.settings(model);
        // Each kind of output its own step, one after another (owner,
        // 2026-11-01: "not all at once and clearly separated").
        let steps = steps_of(&settings);
        // The surface gradients of the 2D cross-sections (2026-11-04):
        // the profiles' spans, their lowest points and both ends.
        let gradient_settings = (self.common.planes && self.gradient).then(|| {
            ohl_electrical::surface_gradient::Settings {
                voltages: settings.voltages.clone(),
                stations: settings.stations.clone(),
                ..Default::default()
            }
        });
        let progress = Arc::new(Progress::default());
        self.common.progress = Some(progress.clone());
        // A new run's outputs all drawn.
        self.common.hidden.clear();
        self.common.saving.stale = None;
        self.common.job.start_at(ctx, model.revision(), move || {
            progress.plan_steps(field_common::steps_planned(
                &steps
                    .iter()
                    .map(|(_, s)| (s.cuts.len(), s.volumes.len()))
                    .collect::<Vec<_>>(),
                unsolved,
                usize::from(gradient_settings.is_some()),
            ));
            if unsolved {
                progress.next_step(format!("solving the weather case {}", case.name));
                let s = crate::model::Solved::with_wind(&line, &library, case.clone(), wind);
                shapes = s.shapes;
                jumpers = s.jumpers;
            }
            let (ground, ground_note) = ground_source.read()?;
            let (shapes, per_notes) =
                crate::field_systems::compose(&line, &library, wind, shapes, &plan);
            let height = |x: f64, y: f64| ground.as_ref().and_then(|t| t.height(x, y));
            let g: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = ground
                .as_ref()
                .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
            let src = ohl_electrical::Sources {
                line: &line,
                library: &library,
                shapes: &shapes,
                temperature: case.temperature,
                ground: g,
                jumpers: &jumpers,
            };
            let mut field: Option<ElectricField> = None;
            let mut charges = None;
            for (what, s) in &steps {
                // The engine steps through cuts and volumes itself.
                if s.cuts.is_empty() && s.volumes.is_empty() {
                    progress.next_step(what.clone());
                }
                let (f, c) = electric::run_keeping(&src, s, Some(&progress))?;
                if c.is_some() {
                    charges = c;
                }
                field = Some(match field {
                    None => f,
                    Some(mut all) => {
                        all.profiles.extend(f.profiles);
                        all.points.extend(f.points);
                        all.cuts.extend(f.cuts);
                        all.volumes.extend(f.volumes);
                        if s.profiles {
                            all.model = f.model;
                        }
                        for n in f.notes {
                            if !all.notes.contains(&n) {
                                all.notes.push(n);
                            }
                        }
                        all
                    }
                });
            }
            let gradient = match &gradient_settings {
                Some(gs) => {
                    progress.next_step("the surface gradients (2D)");
                    Some(ohl_electrical::surface_gradient::run(&src, gs)?)
                }
                None => None,
            };
            progress.set_stage(String::new());
            let mut field = field.ok_or("nothing asked: tick a span, a cut or a volume")?;
            // Each point sums every piece twice: the charges and their
            // images.
            let work = progress.total.load(Ordering::Relaxed) as f64
                * charges.as_ref().map_or(0, |c| c.count()) as f64;
            field_common::add_run_notes(
                &mut field.notes,
                &line,
                &shapes,
                per_notes,
                ground_note,
                g.is_some(),
            );
            let meshes = crate::magnetic_field::views::take_meshes(&mut field.volumes);
            Ok(Answer {
                field,
                case: case.name.clone(),
                charges: charges.map(Arc::new),
                radius: settings.ground_radius,
                ground_model: settings.ground_model,
                meshes,
                work,
                gradient,
            })
        });
    }

    /// Gives system `key` its own weather case (tests).
    #[cfg(test)]
    pub fn set_system_case(&mut self, key: &str, case: &str) {
        self.common
            .per_system
            .insert(key.to_owned(), (Some(case.to_owned()), None));
    }

    /// Sets the name a run is saved as (tests).
    #[cfg(test)]
    pub fn set_save_name(&mut self, name: &str) {
        self.common.saving.name = name.to_owned();
    }

    /// The saved run `r` shown: its inputs set, its field and surfaces,
    /// with what changed on the line since it was saved.
    pub fn load_saved(&mut self, r: &CheckReport, model: &Model) -> Result<(), String> {
        let (case, mut field, meshes, with) =
            crate::field_saved::read::<ElectricField>(&r.results)?;
        if let Some(st) = crate::field_saved::inputs::<Stored>(&r.parameters) {
            self.take(model, st);
        }
        if !with && !field.volumes.is_empty() {
            field.notes.push(
                "Saved without its volumes' surfaces (too large): run it again to see them.".into(),
            );
        }
        let ground_model = r
            .results
            .get("ground_model")
            .and_then(|g| serde_json::from_value(g.clone()).ok())
            .unwrap_or(electric::GroundModel::Plane);
        let radius = r
            .results
            .get("radius")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(self.ground_radius);
        self.common.job.stop();
        self.common
            .job
            .stamp
            .set(crate::revision::saved_revision(&r.parameters, model));
        self.common.job.result = Some(Ok(Answer {
            field,
            case,
            charges: None,
            radius,
            ground_model,
            meshes,
            work: 0.0,
            gradient: r
                .results
                .get("gradient")
                .and_then(|g| serde_json::from_value(g.clone()).ok()),
        }));
        self.common.job.generation += 1;
        self.common.hidden.clear();
        self.charges = None;
        self.common.saving.stale = crate::field_saved::changed_since(r, model);
        self.common.saving.name = r.id.clone();
        self.common.open = true;
        Ok(())
    }

    /// The run shown as report `name` of the performed checks.
    fn check_report(&mut self, model: &Model, name: &str) -> Result<CheckReport, String> {
        let inputs = self.stored(model);
        let Some(Ok(a)) = &self.common.job.result else {
            return Err("run it first".into());
        };
        let (mut results, triangles) = crate::field_saved::results(&a.case, &a.field, &a.meshes)?;
        results["radius"] = serde_json::json!(a.radius);
        results["ground_model"] =
            serde_json::to_value(a.ground_model).map_err(|e| e.to_string())?;
        if let Some(g) = &a.gradient {
            results["gradient"] = serde_json::to_value(g).map_err(|e| e.to_string())?;
        }
        let largest = a
            .field
            .profiles
            .iter()
            .map(|p| p.max_kv_m)
            .fold(0.0, f64::max);
        let summary = format!(
            "\"{}\": {} profile(s), largest {largest:.3} kV/m{}; {} cut(s), {} volume(s){}",
            a.case,
            a.field.profiles.len(),
            a.field
                .limit_kv_m
                .map_or(String::new(), |l| format!(" (limit {l} kV/m)")),
            a.field.cuts.len(),
            a.field.volumes.len(),
            if triangles > crate::field_saved::MAX_SAVED_TRIANGLES {
                " (surfaces not saved)"
            } else {
                ""
            },
        );
        crate::field_saved::report(
            model,
            (crate::field_saved::ELECTRIC_CHECK_ID, "Electric field"),
            name,
            summary,
            crate::field_saved::parameters(&inputs, self.common.job.stamp.loads_per(model)),
            results,
        )
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) {
        let generation = self.common.job.generation;
        self.common.job.poll();
        if !self.common.job.running() {
            // The floors' speed here for the estimates (the time a floor
            // takes on this computer against the one measured).
            if let Some(f) = self.common.progress.take().and_then(|p| p.floor_speed()) {
                self.floor_speed = Some(f);
            }
        }
        if self.common.job.generation != generation {
            // A new answer: its charges for the panels, its speed and its
            // volumes' share for the estimates.
            self.charges = None;
            if let Some(Ok(a)) = &self.common.job.result {
                self.charges = a
                    .charges
                    .clone()
                    .map(|c| (c, a.radius, a.ground_model, self.cut_tile));
                if let Some(t) = self.common.job.took
                    && a.work > 0.0
                {
                    self.common.rate = Some(t.as_secs_f64() / a.work);
                }
                let (c, f) = a.field.volumes.iter().fold((0usize, 0usize), |s, v| {
                    (s.0 + v.values.computed, s.1 + v.values.full)
                });
                if f > 0 {
                    self.common.volume_share = Some(c as f64 / f as f64);
                }
            }
        }
        if !self.common.open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let stale = self.out_of_date(model);
        help::window(
            ctx,
            crate::dockable::Panel::ElectricField,
            "Electric field",
            [860.0, 760.0],
            &mut open,
            |ui| {
                let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
                let circuits = magnetic::circuits(&model.line);
                let why = if names.is_empty() {
                    Some("no weather case in the model".to_owned())
                } else if circuits.is_empty() {
                    Some("no circuit: assign the systems (Line › Electrical systems)".to_owned())
                } else if !circuits
                    .iter()
                    .any(|c| self.voltage(model, c).is_some_and(|v| v > 0.0))
                {
                    Some("every circuit voltage-free: give one a voltage (2 Scope)".to_owned())
                } else if matches!(
                    self.common.stations,
                    Stations::EverySpan | Stations::Beside { .. }
                ) && {
                    let spans = self.common.spans(model);
                    self.common.span_pick.chosen(model, &spans).0.is_empty()
                } {
                    Some("no span ticked (3 Spans)".to_owned())
                } else {
                    let spans = self.common.spans(model);
                    self.memory_block(model, &spans)
                };
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    None,
                    self.common.job.generation,
                    why.is_some() && !self.common.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        if let Some(a) =
                            check_ui::section(ui, 1, "What it checks", Some("1-what-it-checks"))
                        {
                            self.common.guide = Some(a);
                        }
                        help::explain(
                            ui,
                            "The electric field strength (kV/m, the rms of the vector) of the \
                             line's voltages, from the wires where they hang in a weather case.",
                            "The charge per metre of every wire from the cross-section where it is \
                             (q = inv(P) · V, the line parameters' potential coefficients; 3D: each \
                             span in stretches, 24 by default); 3D from the solved wires and jumpers as pieces \
                             of uniform charge over the ground as a tiled conducting floor (or a \
                             plane under each point, 4 Model), the structures' steel optional; 2D: \
                             infinite line charges over level ground. Trees and buildings are not \
                             modelled. A limit is yours to give; none is built in. Built, in \
                             active development, not approved yet.",
                        );
                        ui.horizontal(|ui| {
                            crate::field_help::profile(
                                ui,
                                crate::field_help::Field::Electric,
                                self.common.width,
                                self.common.step,
                                self.common.height,
                            );
                            crate::field_help::sources(
                                ui,
                                crate::field_help::Field::Electric,
                                self.tiled,
                            );
                        });
                        ui.separator();
                        if let Some(a) = check_ui::section(ui, 2, "Scope", Some("2-scope")) {
                            self.common.guide = Some(a);
                        }
                        check_ui::ScopePicker::new(WINDOW, &model.line)
                            .route_of(model)
                            .cases(check_ui::Cases::One(&names, &mut self.common.case))
                            .ui(ui);
                        // How the wires are solved (owner, 2026-11-01).
                        ui.horizontal(|ui| {
                            ui.label("Wires by");
                            crate::rule_table::calculation_combo(
                                ui,
                                "field-calculation",
                                &mut self.common.calculation,
                            );
                            // Its explanation behind the (?) (owner,
                            // 2026-11-12: no grey lines between fields).
                            help::question(ui, field_common::WIRES_BY_HELP);
                        });
                        crate::field_systems::ui(ui, WINDOW, model, &mut self.common.per_system);
                        crate::field_ground::ui(ui, WINDOW, model, &mut self.common.ground_from);
                        self.inputs_ui(ui, model);
                        ui.separator();
                        if let Some(a) = check_ui::section(
                            ui,
                            3,
                            "Spans and what to compute in each",
                            Some("3-spans-and-what-to-compute-in-each"),
                        ) {
                            self.common.guide = Some(a);
                        }
                        self.spans_ui(ui, model);
                        let spans = self.common.spans(model);
                        self.views_ui(ui, model, &spans);
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
                        egui::ScrollArea::both()
                            .id_salt("electric-field-results")
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.results_ui(ui, model, &stale, &mut run));
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    let running = self.common.progress.as_ref().map(|p| {
                        field_common::run_progress(p, self.common.job.elapsed().unwrap_or_default())
                    });
                    let f = check_ui::RunFooter {
                        running,
                        why_not: why.clone(),
                        hover: "Compute the field from the wires as solved in the weather case \
                                chosen",
                        scope: None,
                        info: Some(match self.common.job.took {
                            Some(t) => format!("{} · last run {t:.1?}", self.summary(model)),
                            None => self.summary(model),
                        }),
                    };
                    match check_ui::run_footer(ui, f, |_| {}) {
                        check_ui::Action::Run => run = true,
                        check_ui::Action::Stop => {
                            if let Some(p) = &self.common.progress {
                                p.cancel.store(true, Ordering::Relaxed);
                            }
                        }
                        check_ui::Action::None => {}
                    }
                });
            },
        );
        // Closed: its inputs kept in the project (the next frame).
        if self.common.open && !open {
            self.common.commit = true;
        }
        self.common.open = open;
        if run {
            self.start(model, ctx);
        }
    }

    fn inputs_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let circuits = magnetic::circuits(&model.line);
        if circuits.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No section carries a system's phase: assign the systems (Line › Electrical \
                     systems) to get circuits.",
                )
                .color(ui.visuals().warn_fg_color),
            );
        }
        egui::Grid::new("electric-field-voltages")
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                for h in ["circuit", "system", "voltage between phases", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for c in &circuits {
                    let label = c.label();
                    ui.label(&label);
                    ui.label(&c.system);
                    let used = self.voltage_used(model, c);
                    let Some(mut kv) = used.map(|v| v.kv) else {
                        ui.label(egui::RichText::new("DC: left out").weak())
                            .on_hover_text("A static field with space charge: not computed");
                        ui.label("");
                        ui.end_row();
                        continue;
                    };
                    // What it falls back on when none is typed here.
                    let system = model.line.systems.iter().find(|s| s.id == c.system);
                    let nominal = system.map_or(0.0, |s| s.voltage_kv);
                    let highest = system.and_then(|s| s.highest_voltage_kv);
                    let r = crate::style::field(
                        ui,
                        egui::DragValue::new(&mut kv)
                            .range(0.0..=2000.0)
                            .speed(1.0)
                            .suffix(" kV"),
                    )
                    .on_hover_text(
                        "rms, between phases; each phase at it over √3 to earth at its \
                         system's phase angle (L1 0°, L2 −120°, L3 +120° unless the system gives \
                         others). 0: voltage-free (at earth potential, like the earth wires)",
                    );
                    if r.changed() {
                        self.voltages.insert(label.clone(), kv);
                    }
                    ui.horizontal(|ui| {
                        use ohl_electrical::electric::VoltageSource;
                        let given = self.voltages.contains_key(&label);
                        let source = used.map(|v| v.source);
                        if kv == 0.0 {
                            ui.label(egui::RichText::new("voltage-free").weak());
                        } else if source == Some(VoltageSource::Nominal) {
                            // Said where it is chosen, as in the results.
                            ui.label(
                                egui::RichText::new(NOMINAL_VOLTAGE_LABEL)
                                    .color(ui.visuals().warn_fg_color),
                            )
                            .on_hover_text(NOMINAL_VOLTAGE_HELP);
                        } else {
                            ui.label(
                                egui::RichText::new(match source {
                                    Some(VoltageSource::Highest) => HIGHEST_VOLTAGE_LABEL,
                                    _ => "given",
                                })
                                .weak(),
                            );
                        }
                        let (back, to) = match highest {
                            Some(u) => ("highest", format!("highest operating voltage, {u} kV")),
                            None => ("nominal", format!("nominal voltage, {nominal} kV")),
                        };
                        if given
                            && ui
                                .small_button(back)
                                .on_hover_text(format!("Back to the system's {to}"))
                                .clicked()
                        {
                            self.voltages.remove(&label);
                        }
                    });
                    ui.end_row();
                }
            });
        help::explain(
            ui,
            "Default: the system's highest operating voltage, else its nominal — not on the \
             safe side, said; 0 kV: voltage-free.",
            "Each circuit at its system's highest operating voltage where the line gives one \
             (Electrical systems, max kV), else at its nominal, unless a voltage is given here. \
             The NISV guide (BAFU 2007, 8.7.3) asks the highest operating voltage, the largest \
             sag (the weather case) and, on a double circuit built to compensate, one circuit \
             voltage-free. Which governs is yours to choose; no voltage is built in or filled \
             in: the highest operating voltage is your value. A higher voltage raises the \
             field in proportion (10 % more voltage: 10 % more field); the time is the same. \
             The results always say which voltage each circuit was computed at.",
        );
        ui.add_space(4.0);
        egui::Grid::new("electric-field-settings")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Limit [kV/m]");
                ui.horizontal(|ui| {
                    let mut given = self.common.limit.is_some();
                    ui.checkbox(&mut given, "given").on_hover_text(
                        "Your limit, kV/m: the margin at every point. None is built in (an \
                         ordinance's values come from its parameter file)",
                    );
                    if !given {
                        self.common.limit = None;
                    } else {
                        let l = self.common.limit.get_or_insert(5.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(l)
                                .range(0.001..=1000.0)
                                .speed(0.01)
                                .suffix(" kV/m"),
                        );
                    }
                });
                ui.end_row();
                ui.label("Points");
                ui.horizontal(|ui| {
                    if ui
                        .button("Points from CSV…")
                        .on_hover_text(
                            "A CSV file with a header naming x, y and z (elevation) or height \
                             (above the ground), optionally name: e.g. places of sensitive use",
                        )
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("CSV", &["csv", "txt"])
                            .pick_file()
                    {
                        match std::fs::read_to_string(&path)
                            .map_err(|e| e.to_string())
                            .and_then(|t| magnetic::parse_points(&t).map(|p| (t, p)))
                        {
                            Ok((text, p)) => {
                                let name = path
                                    .file_name()
                                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                                self.common.points = Some((name, p));
                                self.common.points_text = Some(text);
                                self.common.points_error = None;
                            }
                            Err(e) => {
                                self.common.points_error = Some(format!("{}: {e}", path.display()))
                            }
                        }
                    }
                    if let Some((name, p)) = &self.common.points {
                        ui.label(format!("{} points from {name}", p.len()));
                        if ui.small_button("Clear").clicked() {
                            self.common.points = None;
                            self.common.points_text = None;
                        }
                    }
                });
                ui.end_row();
                if crate::field_preset::row(ui, &mut self.common.buildings, self.common.planes) {
                    self.common.commit = true;
                }
            });
        if let Some(e) = &self.common.points_error {
            crate::menu::error_label(ui, e.as_str());
        }
    }

    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        stale: &Option<String>,
        run: &mut bool,
    ) {
        // The outputs listed first (owner, 2026-10-31): a click shows one.
        if let Some(Ok(a)) = &self.common.job.result {
            let rows = self.output_rows(model, a);
            match field_results::list(ui, "electric-field", &rows, &mut self.common.hidden) {
                Some(RowAction::Plot(k)) => {
                    self.common.shown = k;
                    self.common.reveal = true;
                }
                Some(RowAction::Ask(r)) => self.common.requests.push(r),
                _ => {}
            }
            ui.separator();
        }
        // Save in Performed checks (owner, 2026-11-01).
        if matches!(self.common.job.result, Some(Ok(_)))
            && let Some(name) = self.common.saving.ui(ui, model)
        {
            let r = self.check_report(model, &name).map(|report| {
                self.common.apply =
                    Some((format!("saved the electric field {name}"), report.put()));
            });
            self.common.saving.outcome(&name, &r);
        }
        let answer = match &self.common.job.result {
            None => {
                ui.label("Run to compute the field.");
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return;
            }
            Some(Ok(a)) => a,
        };
        if let Some(what) = stale {
            *run |= crate::revision::banner(ui, what, !self.common.job.running());
        }
        let m = &answer.field;
        ui.horizontal(|ui| {
            ui.label(format!(
                "In \"{}\"; {}; {} m above the ground{}",
                answer.case,
                match m.model {
                    FieldModel::ThreeD => "3D (the wires as solved, images in the ground)",
                    FieldModel::TwoD => "2D (infinite line charges over level ground)",
                },
                m.height,
                m.limit_kv_m
                    .map_or(String::new(), |l| format!("; limit {l} kV/m"))
            ));
            if ui.button("Export CSV…").clicked() {
                save_csv("electric-field.csv", &electric::csv(m));
            }
        });
        if !m.charges.is_empty() {
            egui::CollapsingHeader::new(format!("Charges per wire ({})", m.charges.len()))
                .id_salt("electric-field-charges")
                .show(ui, |ui| {
                    help::list(
                        ui,
                        "electric-field-charges",
                        help::LIST_HEIGHT,
                        egui::ScrollArea::vertical().id_salt("electric-field-charges-scroll"),
                        |ui| {
                            egui::Grid::new("electric-field-charges-grid")
                                .striped(true)
                                .spacing(help::GRID_SPACING)
                                .show(ui, |ui| {
                                    for h in ["section", "span", "kV to earth", "nC/m", "Hz"] {
                                        ui.strong(h);
                                    }
                                    ui.end_row();
                                    for c in &m.charges {
                                        ui.label(&c.section);
                                        ui.label(format!("{}–{}", c.from, c.to));
                                        ui.label(format!("{:.2}", c.kv));
                                        ui.label(format!("{:.2}", c.nc_per_m));
                                        ui.label(format!("{}", c.frequency_hz));
                                        ui.end_row();
                                    }
                                });
                        },
                    );
                });
        }
        for n in &m.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        ui.label(
            egui::RichText::new(
                "Not yet: isolines from above (the plan); places of sensitive use as objects of \
                 the project.",
            )
            .small()
            .weak(),
        );
        if let Some(g) = &answer.gradient {
            gradient_ui(ui, g);
        }
        self.views_results_ui(ui, m, &answer.meshes);
        if !m.profiles.is_empty() {
            ui.separator();
            self.common.shown = self.common.shown.min(m.profiles.len() - 1);
            let header = ui
                .horizontal(|ui| {
                    ui.strong("Profile");
                    let name =
                        |p: &electric::Profile| format!("{}–{} at {:.1} m", p.from, p.to, p.at);
                    egui::ComboBox::from_id_salt("electric-field-profile")
                        .selected_text(name(&m.profiles[self.common.shown]))
                        .show_ui(ui, |ui| {
                            for (k, p) in m.profiles.iter().enumerate() {
                                ui.selectable_value(&mut self.common.shown, k, name(p));
                            }
                        });
                })
                .response
                .rect;
            let drawn = {
                let p = &m.profiles[self.common.shown];
                let values: Vec<(f64, f64)> =
                    p.points.iter().map(|q| (q.offset, q.e_kv_m)).collect();
                field_common::plot(
                    ui,
                    &field_common::ProfilePlot {
                        values: &values,
                        faint: None,
                        axis: "E kV/m",
                        unit: "kV/m",
                        hover: &|k| {
                            let q = &p.points[k];
                            format!("{:.1} m: {:.4} kV/m", q.offset, q.e_kv_m)
                        },
                    },
                    m.limit_kv_m,
                )
            };
            // The list's Show plot (2026-11-01: the plot sat below the
            // notes and views, out of sight): the results scroll to it.
            if std::mem::take(&mut self.common.reveal) {
                ui.scroll_to_rect(header.union(drawn), Some(egui::Align::TOP));
            }
            help::list(
                ui,
                "electric-field-profiles",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("electric-field-profiles-scroll"),
                |ui| {
                    egui::Grid::new("electric-field-profiles")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in ["span", "station m", "max kV/m", "at m", "margin kV/m"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (k, p) in m.profiles.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        k == self.common.shown,
                                        format!("{}–{}", p.from, p.to),
                                    )
                                    .clicked()
                                {
                                    self.common.shown = k;
                                }
                                ui.label(format!("{:.1}", p.at));
                                ui.label(format!("{:.3}", p.max_kv_m));
                                ui.label(format!("{:.1}", p.max_offset));
                                margin(ui, m.limit_kv_m.map(|l| l - p.max_kv_m));
                                ui.end_row();
                            }
                        });
                },
            );
        }
        if !m.points.is_empty() {
            ui.separator();
            ui.strong("Points");
            help::list(
                ui,
                "electric-field-points",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("electric-field-points-scroll"),
                |ui| {
                    egui::Grid::new("electric-field-points")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in [
                                "name",
                                "x m",
                                "y m",
                                "z m",
                                "height m",
                                "E kV/m",
                                "margin kV/m",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for p in &m.points {
                                ui.label(&p.name);
                                ui.label(format!("{:.2}", p.x));
                                ui.label(format!("{:.2}", p.y));
                                ui.label(format!("{:.2}", p.z));
                                ui.label(format!("{:.2}", p.height));
                                ui.label(format!("{:.4}", p.e_kv_m));
                                margin(ui, p.margin_kv_m);
                                ui.end_row();
                            }
                        });
                },
            );
        }
    }
}

/// The surface gradients of the 2D cross-sections: one row per bundle.
fn gradient_ui(ui: &mut egui::Ui, g: &ohl_electrical::surface_gradient::SurfaceGradients) {
    let rows: usize = g.cuts.iter().map(|c| c.bundles.len()).sum();
    egui::CollapsingHeader::new(format!(
        "Conductor surface gradient, 2D ({} cross-sections, {rows} rows)",
        g.cuts.len()
    ))
    .id_salt("electric-field-gradient")
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "kV/cm rms (peak √2 times); {} line charges per subconductor, {} points round it",
                    g.charges, g.points
                ))
                .small()
                .weak(),
            );
            if ui
                .button("Export CSV…")
                .on_hover_text("Every subconductor: its largest gradient, where, its mean; its bundle's values")
                .clicked()
            {
                save_csv(
                    "surface-gradient.csv",
                    &ohl_electrical::surface_gradient::csv(g),
                );
            }
        });
        help::list(
            ui,
            "electric-field-gradient",
            help::LIST_HEIGHT,
            egui::ScrollArea::vertical().id_salt("electric-field-gradient-scroll"),
            |ui| {
                egui::Grid::new("electric-field-gradient-grid")
                    .striped(true)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        for (h, tip) in [
                            ("span", "The span's structures"),
                            ("station m", "From the span's first structure"),
                            ("where", "lowest: the lowest phase's lowest point; start, end: at the structures, the first cross-section every wire reaches; given: the station asked"),
                            ("section", "The wire's section"),
                            ("circuit", "Its circuit; earth wire: at earth potential"),
                            ("phase", "Its phase"),
                            ("n", "Subconductors"),
                            ("avg max kV/cm", "The average maximum: the mean over the subconductors of each one's largest surface gradient — what corona noise formulas take"),
                            ("max kV/cm", "The largest surface gradient of any subconductor"),
                            ("Markt–Mengele", "Markt–Mengele's average maximum from the bundle's charge (the equivalent wire), a cross-check"),
                        ] {
                            ui.strong(h).on_hover_text(tip);
                        }
                        ui.end_row();
                        for c in &g.cuts {
                            for b in &c.bundles {
                                ui.label(format!("{}–{}", c.from, c.to));
                                ui.label(format!("{:.1}", c.at));
                                ui.label(&c.place);
                                ui.label(&b.section);
                                ui.label(if b.circuit.is_empty() {
                                    "earth wire"
                                } else {
                                    &b.circuit
                                });
                                ui.label(&b.phase);
                                ui.label(b.count.to_string());
                                ui.label(format!("{:.3}", b.average_max_kv_cm));
                                ui.label(format!("{:.3}", b.max_kv_cm));
                                ui.label(format!("{:.3}", b.markt_mengele_kv_cm));
                                ui.end_row();
                            }
                        }
                    });
            },
        );
        for n in &g.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
    });
}

/// A margin, kV/m: red when below zero, a dash without a limit.
fn margin(ui: &mut egui::Ui, m: Option<f64>) {
    match m {
        Some(x) if x < 0.0 => {
            ui.label(egui::RichText::new(format!("{x:.4}")).color(ui.visuals().error_fg_color));
        }
        Some(x) => {
            ui.label(format!("{x:.4}"));
        }
        None => {
            ui.label("—");
        }
    }
}

/// A run's steps (owner, 2026-11-01): 2D — the cuts (the charges in 3D),
/// then the profiles and points (by their model); 3D — the volumes. Each
/// with what it computes in words; none empty.
fn steps_of(all: &Settings) -> Vec<(String, Settings)> {
    let none = Settings {
        profiles: false,
        stations: Vec::new(),
        points: Vec::new(),
        cuts: Vec::new(),
        volumes: Vec::new(),
        ..all.clone()
    };
    let mut out = Vec::new();
    if !all.cuts.is_empty() {
        out.push((
            format!("{} cut(s)", all.cuts.len()),
            Settings {
                model: FieldModel::ThreeD,
                cuts: all.cuts.clone(),
                ..none.clone()
            },
        ));
    }
    if all.profiles {
        out.push((
            "the profiles".to_owned(),
            Settings {
                profiles: true,
                stations: all.stations.clone(),
                points: all.points.clone(),
                ..none.clone()
            },
        ));
    }
    if !all.volumes.is_empty() {
        out.push((
            format!("{} volume(s)", all.volumes.len()),
            Settings {
                model: FieldModel::ThreeD,
                volumes: all.volumes.clone(),
                ..none
            },
        ));
    }
    out
}

/// What the surface gradient option does (its effect and time, measured).
const GRADIENT_EFFECT: &str = "The conductor surface gradient (kV/cm rms) by a charge simulation of every subconductor (8 line charges in each, within 0.01 % of 64 on line cross-sections; 360 points round it) over level, perfectly conducting ground at each cross-section: the profiles' spans at their lowest points and at both ends. Per bundle the average maximum (what corona noise formulas take), the maximum and Markt–Mengele's value. Only in 2D (not in 3D). It changes no field value; measured some milliseconds per cross-section. Off: not computed";

/// What the stretches per span change, measured (the theory's
/// sensitivity table; filled from it).
const STRETCHES_EFFECT: &str = "measured against 64, a profile's largest value near a structure: 4: 1.2 % off, 8: 0.16 %, 16: 0.03 %, 24 (default): 0.005 %; the time hardly changes";
