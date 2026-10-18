// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Level-of-detail settings of the 3D view: per chunked layer, the render
//! distance, the distances from which levels 1 and 2 are drawn, and what
//! each level keeps (the Level of detail table), and for a point cloud
//! streamed by node ([`crate::octree`]) the point budget and the error
//! allowed on the screen — remembered per user. Display only:
//! calculations always use the full model.

use std::collections::BTreeMap;

use eframe::egui;
use ohl_render::lod::LodRule;

use crate::scene::Layer;

/// How much longer a line code's dashes are drawn at levels 0, 1 and 2
/// (phases' line codes, sections not solved): a dash of a metre and a
/// half is not seen kilometres away, and a long line has thousands of
/// them per span.
pub const DASH_SCALE: [f64; 3] = [1.0, 4.0, 16.0];

/// A body on a wire (a marker ball) at level 1: a dot this big, px (its
/// shape at level 0; clamps, bare points and jumpers' support clamps are
/// drawn at level 0 only).
pub const BODY_DOT_PX: f32 = 4.0;

/// The layers drawn in chunks with levels of detail.
pub const CHUNKED: [Layer; 8] = [
    Layer::Ground,
    Layer::Surfaces,
    Layer::Points,
    Layer::Structures,
    Layer::Wires,
    Layer::OtherCases,
    Layer::OtherLines,
    Layer::Drawings,
];

/// What levels 1 and 2 keep of a layer, by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKind {
    /// A regular grid of this spacing, m (terrain).
    Spacing,
    /// This share of the points, % (LiDAR).
    Percent,
    /// This many points per span, at least 2 (wires).
    PerSpan,
    /// Fixed: level 1 mast, arms and strings, level 2 the mast line.
    Structure,
    /// Lines simplified to this tolerance, m; lines and points smaller
    /// left out (drawings).
    Tolerance,
}

impl DetailKind {
    /// The kind of a chunked layer.
    pub fn of(layer: Layer) -> Self {
        match layer {
            Layer::Ground | Layer::Surfaces => DetailKind::Spacing,
            Layer::Points => DetailKind::Percent,
            Layer::Structures => DetailKind::Structure,
            Layer::Drawings => DetailKind::Tolerance,
            _ => DetailKind::PerSpan,
        }
    }

    /// Unit shown after the values.
    pub fn unit(self) -> &'static str {
        match self {
            DetailKind::Spacing => "m grid",
            DetailKind::Percent => "% of points",
            DetailKind::PerSpan => "points per span",
            DetailKind::Structure => "",
            DetailKind::Tolerance => "m tolerance",
        }
    }

    /// What a level keeps, in words (`level` 0, 1, 2).
    pub fn describe(self, level: usize, value: f64) -> String {
        match (self, level) {
            (DetailKind::Spacing, 0) => "every triangle".into(),
            (DetailKind::Percent, 0) => "every point".into(),
            (DetailKind::PerSpan, 0) => {
                "every point, bundles as subconductors, clamps and bodies to their shape".into()
            }
            (DetailKind::Structure, 0) => "lattice, arms and strings".into(),
            (DetailKind::Tolerance, 0) => "every vertex and point".into(),
            (DetailKind::Tolerance, _) => format!(
                "lines simplified to {value} m; lines and points smaller than that left out"
            ),
            (DetailKind::Structure, 1) => "mast, arms and strings".into(),
            (DetailKind::Structure, _) => "mast line".into(),
            (DetailKind::PerSpan, level) => format!(
                "{value:.0} points per span, one line per bundle, line codes {}× longer{}",
                DASH_SCALE[level.min(2)],
                if level == 1 { ", bodies as dots" } else { "" }
            ),
            (k, _) => format!("{value} {}", k.unit()),
        }
    }
}

/// One row of the table.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LodRow {
    /// Not drawn beyond, m.
    pub render_distance: f64,
    /// Level 1, then level 2, from these distances, m.
    pub from: [f64; 2],
    /// What levels 1 and 2 keep ([`DetailKind`]).
    pub detail: [f64; 2],
}

impl LodRow {
    /// The renderer's rule.
    pub fn rule(&self) -> LodRule {
        LodRule {
            render_distance: self.render_distance,
            from: self.from,
        }
    }

    /// The defaults of a layer: whole lines of tens of kilometres stay in
    /// view, the detail falls with the distance.
    pub fn default_for(layer: Layer) -> Self {
        let (render_distance, from, detail) = match layer {
            Layer::Ground | Layer::Surfaces => (30_000.0, [1_500.0, 5_000.0], [8.0, 32.0]),
            Layer::Points => (3_000.0, [500.0, 1_500.0], [25.0, 5.0]),
            Layer::Structures => (50_000.0, [2_000.0, 8_000.0], [0.0, 0.0]),
            Layer::Drawings => (10_000.0, [1_000.0, 4_000.0], [0.5, 2.0]),
            _ => (50_000.0, [1_500.0, 6_000.0], [16.0, 5.0]),
        };
        Self {
            render_distance,
            from,
            detail,
        }
    }

    /// Checks the values (distances increasing, details in range).
    pub fn check(&self, layer: Layer) -> Result<(), String> {
        let d = [self.from[0], self.from[1], self.render_distance];
        if !(d.iter().all(|v| v.is_finite() && *v >= 0.0) && d[0] <= d[1]) {
            return Err("distances must be ≥ 0, level 1 before level 2".into());
        }
        let ok = match DetailKind::of(layer) {
            DetailKind::Spacing => self.detail.iter().all(|v| (0.5..=1000.0).contains(v)),
            DetailKind::Percent => self.detail.iter().all(|v| *v > 0.0 && *v <= 100.0),
            DetailKind::PerSpan => self.detail.iter().all(|v| (2.0..=1000.0).contains(v)),
            DetailKind::Structure => true,
            DetailKind::Tolerance => self.detail.iter().all(|v| (0.01..=1000.0).contains(v)),
        };
        if ok {
            Ok(())
        } else {
            Err(format!(
                "detail out of range ({})",
                DetailKind::of(layer).unit()
            ))
        }
    }
}

/// How a point cloud streamed by node is drawn ([`crate::octree`]).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PointBudget {
    /// Points drawn at most (all nodes in view together).
    pub budget: u64,
    /// A node is refined while its points lie farther apart on the
    /// screen than this, px.
    pub max_error: f64,
}

impl Default for PointBudget {
    fn default() -> Self {
        Self {
            budget: DEFAULT_POINT_BUDGET,
            max_error: DEFAULT_MAX_ERROR,
        }
    }
}

/// Points drawn at most by default: 5 million (~100 MB on the GPU, 1.5×
/// that kept; drawn in ~16 ms by an Apple M1 Max, 60 frames a
/// second).
pub const DEFAULT_POINT_BUDGET: u64 = 5_000_000;
/// Error allowed on the screen by default, px.
pub const DEFAULT_MAX_ERROR: f64 = 1.5;

impl PointBudget {
    /// Checks the values: 100 000 to 200 million points, 0.1 to 50 px.
    pub fn check(&self) -> Result<(), String> {
        if !(100_000..=200_000_000).contains(&self.budget) {
            return Err("point budget: 0.1 to 200 million points".into());
        }
        if !(self.max_error.is_finite() && (0.1..=50.0).contains(&self.max_error)) {
            return Err("error on the screen: 0.1 to 50 px".into());
        }
        Ok(())
    }
}

/// The table: a row per chunked layer, and the point budget.
#[derive(Debug, Clone, PartialEq)]
pub struct LodSettings {
    rows: BTreeMap<&'static str, LodRow>,
    /// The point budget of a cloud streamed by node.
    pub points: PointBudget,
}

fn key(layer: Layer) -> &'static str {
    match layer {
        Layer::Ground => "terrain",
        Layer::Surfaces => "surfaces",
        Layer::Points => "lidar",
        Layer::Structures => "structures",
        Layer::Wires => "wires",
        Layer::OtherCases => "other-cases",
        Layer::OtherLines => "other-lines",
        Layer::Drawings => "drawings",
        _ => "other",
    }
}

impl Default for LodSettings {
    fn default() -> Self {
        Self {
            rows: CHUNKED
                .iter()
                .map(|l| (key(*l), LodRow::default_for(*l)))
                .collect(),
            points: PointBudget::default(),
        }
    }
}

impl LodSettings {
    /// The row of `layer` (its default when not chunked).
    pub fn row(&self, layer: Layer) -> LodRow {
        self.rows
            .get(key(layer))
            .copied()
            .unwrap_or_else(|| LodRow::default_for(layer))
    }

    /// Sets the row of `layer` when it reads; the error otherwise.
    pub fn set(&mut self, layer: Layer, row: LodRow) -> Result<(), String> {
        row.check(layer)?;
        self.rows.insert(key(layer), row);
        Ok(())
    }

    /// Saved as JSON (per user, with the window layout).
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(text) = serde_json::to_string(&self.rows) {
            storage.set_string("lod", text);
        }
        if let Ok(text) = serde_json::to_string(&self.points) {
            storage.set_string("lod-points", text);
        }
    }

    /// As saved; rows that do not read keep their defaults.
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        let mut s = Self::default();
        let saved: BTreeMap<String, LodRow> = storage
            .get_string("lod")
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        for layer in CHUNKED {
            if let Some(row) = saved.get(key(layer)) {
                let _ = s.set(layer, *row);
            }
        }
        if let Some(p) = storage
            .get_string("lod-points")
            .and_then(|t| serde_json::from_str::<PointBudget>(&t).ok())
            .filter(|p| p.check().is_ok())
        {
            s.points = p;
        }
        s
    }

    /// The Level of detail page: a row per layer, every value typed in;
    /// what was drawn in the last frame next to it; then the point budget
    /// of a cloud streamed by node (`streamed`: what it did in the last
    /// frame). Returns whether anything changed.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        stats: &dyn Fn(Layer) -> Option<ohl_render::lod::PlanStats>,
        streamed: Option<crate::octree::StreamStats>,
    ) -> bool {
        let mut changed = false;
        ui.label(
            egui::RichText::new(
                "Display only: calculations, checks and picking always use the full model. \
                 The 3D view is drawn in 256 m chunks; each chunk at the level its distance \
                 from the eye gives, nothing beyond the render distance.",
            )
            .small()
            .weak(),
        );
        egui::Grid::new("lod-table")
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                for h in [
                    "layer",
                    "render distance m",
                    "level 1 from m",
                    "level 2 from m",
                    "level 1 keeps",
                    "level 2 keeps",
                    "drawn (levels 0 / 1 / 2)",
                ] {
                    ui.label(egui::RichText::new(h).strong());
                }
                ui.end_row();
                for layer in CHUNKED {
                    let mut row = self.row(layer);
                    let kind = DetailKind::of(layer);
                    ui.label(layer.name());
                    let edit = |ui: &mut egui::Ui, v: &mut f64, range: (f64, f64)| {
                        let speed = (*v * 0.01).max(0.1);
                        ui.add(
                            egui::DragValue::new(v)
                                .range(range.0..=range.1)
                                .speed(speed),
                        )
                        .changed()
                    };
                    let mut c = false;
                    c |= edit(ui, &mut row.render_distance, (0.0, 1e7));
                    c |= edit(ui, &mut row.from[0], (0.0, 1e7));
                    c |= edit(ui, &mut row.from[1], (0.0, 1e7));
                    for k in 0..2 {
                        if kind == DetailKind::Structure {
                            ui.label(kind.describe(k + 1, 0.0));
                            continue;
                        }
                        let range = match kind {
                            DetailKind::Spacing => (0.5, 1000.0),
                            DetailKind::Percent => (0.1, 100.0),
                            DetailKind::Tolerance => (0.01, 1000.0),
                            _ => (2.0, 1000.0),
                        };
                        ui.horizontal(|ui| {
                            c |= edit(ui, &mut row.detail[k], range);
                            ui.label(egui::RichText::new(kind.unit()).small().weak());
                        })
                        .response
                        .on_hover_text(kind.describe(k + 1, row.detail[k]));
                    }
                    match (layer, streamed, stats(layer)) {
                        (Layer::Points, Some(s), _) => ui.label(format!(
                            "{} of {} nodes, {:.2} M points (budget below)",
                            s.drawn,
                            s.nodes,
                            s.points as f64 / 1e6
                        )),
                        (_, _, Some(s)) => ui.label(format!(
                            "{} / {} / {} of {} chunks{}",
                            s.drawn[0],
                            s.drawn[1],
                            s.drawn[2],
                            s.chunks,
                            if s.pending > 0 {
                                format!(", {} loading", s.pending)
                            } else {
                                String::new()
                            }
                        )),
                        (_, _, None) => ui.label(egui::RichText::new("not shown").weak()),
                    };
                    ui.end_row();
                    // Level 2 never before level 1.
                    row.from[1] = row.from[1].max(row.from[0]);
                    if c && self.set(layer, row).is_ok() {
                        changed = true;
                    }
                }
            });
        ui.separator();
        changed |= self.points_ui(ui, streamed);
        if ui.button("Defaults").clicked() {
            *self = Self::default();
            changed = true;
        }
        changed
    }

    /// The point budget of a cloud streamed by node: points drawn at most
    /// and the error allowed on the screen; what the last frame did.
    fn points_ui(
        &mut self,
        ui: &mut egui::Ui,
        streamed: Option<crate::octree::StreamStats>,
    ) -> bool {
        ui.label(egui::RichText::new("LiDAR point budget").strong());
        ui.label(
            egui::RichText::new(
                "The project's point cloud (or a COPC file) is read by node: the nodes in \
                 view, the coarsest first, refined where their points lie farther apart on \
                 the screen than the error allowed, the nearest first, until the budget is \
                 drawn. The LiDAR row's render distance applies; its levels do not. Points \
                 shown near the camera (the LiDAR settings) thin every node further.",
            )
            .small()
            .weak(),
        );
        let mut p = self.points;
        let mut millions = p.budget as f64 / 1e6;
        let mut c = false;
        egui::Grid::new("lod-points")
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("points drawn at most");
                c |= ui
                    .add(
                        egui::DragValue::new(&mut millions)
                            .range(0.1..=200.0)
                            .speed(0.1)
                            .max_decimals(1)
                            .suffix(" million"),
                    )
                    .on_hover_text("About 20 bytes of GPU memory per point; 1.5 × this is kept")
                    .changed();
                ui.end_row();
                ui.label("error on the screen");
                c |= ui
                    .add(
                        egui::DragValue::new(&mut p.max_error)
                            .range(0.1..=50.0)
                            .speed(0.05)
                            .max_decimals(2)
                            .suffix(" px"),
                    )
                    .on_hover_text("Smaller: finer nodes (more points) for the same view")
                    .changed();
                ui.end_row();
            });
        match streamed {
            Some(s) => {
                ui.label(format!(
                    "{} of {} nodes drawn, {:.2} M points ({:.2} M wanted{}); {} nodes, \
                     {:.2} M points on the GPU{}",
                    s.drawn,
                    s.nodes,
                    s.points as f64 / 1e6,
                    s.wanted as f64 / 1e6,
                    if s.capped { ", budget reached" } else { "" },
                    s.cached.0,
                    s.cached.1 as f64 / 1e6,
                    if s.pending > 0 {
                        format!(", {} loading", s.pending)
                    } else {
                        String::new()
                    }
                ));
            }
            None => {
                ui.label(egui::RichText::new("no point cloud streamed").weak());
            }
        }
        p.budget = (millions * 1e6).round() as u64;
        if c && p.check().is_ok() && p != self.points {
            self.points = p;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_check_and_default() {
        let s = LodSettings::default();
        for layer in CHUNKED {
            s.row(layer).check(layer).unwrap();
        }
        let mut s = s;
        let mut bad = s.row(Layer::Points);
        bad.detail = [0.0, 5.0];
        assert!(s.set(Layer::Points, bad).is_err());
        let mut bad = s.row(Layer::Wires);
        bad.from = [2000.0, 1000.0];
        assert!(s.set(Layer::Wires, bad).is_err());
        let mut good = s.row(Layer::Ground);
        good.detail = [4.0, 16.0];
        s.set(Layer::Ground, good).unwrap();
        assert_eq!(s.row(Layer::Ground).detail, [4.0, 16.0]);
        s.points.check().unwrap();
        let tiny = PointBudget {
            budget: 10,
            ..s.points
        };
        assert!(tiny.check().is_err());
    }
}
