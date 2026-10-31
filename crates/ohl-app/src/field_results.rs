// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a field window computed, listed (owner, 2026-10-31, validated:
//! "in 3D … it should display it in the 3D view when we click on the
//! results"): every profile, cut and volume a row with its largest value
//! and a note, and what a click on it does — the profile plotted, the cut
//! opened in a cross-section panel on the wires, the volume shown in the
//! 3D view. The windows ask the application through [`FieldRequest`]s.
//! Shared by the Magnetic and the Electric field windows.

use eframe::egui;

use crate::model::Model;

/// What a field window asks of the application.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldRequest {
    /// A cross-section panel at `station` of `alignment`, ± `width`.
    OpenCut {
        alignment: String,
        station: f64,
        width: f64,
    },
    /// The 3D view looking at `target` from `distance`, across the line
    /// (`yaw`, rad), the field's layer shown.
    Look {
        target: [f64; 3],
        distance: f64,
        yaw: f64,
    },
}

/// What a click on a row does.
#[derive(Debug, Clone, PartialEq)]
pub enum RowAction {
    /// The profile (its index) plotted.
    Plot(usize),
    /// The request made.
    Ask(FieldRequest),
    /// Nothing (the place could not be found).
    None,
}

/// A row of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultRow {
    /// Profile, cut, volume, grid.
    pub kind: &'static str,
    /// Where: the span and the place in it.
    pub place: String,
    /// Its largest value, with its unit.
    pub max: String,
    /// What else to know.
    pub note: String,
    /// Its button's text and what it does.
    pub button: &'static str,
    pub action: RowAction,
}

/// The list: a row per output; the action of the row whose button was
/// clicked.
pub fn list(ui: &mut egui::Ui, id: &str, rows: &[ResultRow]) -> Option<RowAction> {
    let mut out = None;
    if rows.is_empty() {
        return out;
    }
    crate::check_help::list(
        ui,
        (id, "outputs"),
        180.0,
        egui::ScrollArea::vertical()
            .id_salt((id, "outputs-scroll"))
            .min_scrolled_height(crate::check_help::list_height(
                ui.ctx(),
                (id, "outputs"),
                180.0,
            )),
        |ui| {
            egui::Grid::new((id, "outputs-grid"))
                .num_columns(5)
                .striped(true)
                .spacing([14.0, 3.0])
                .show(ui, |ui| {
                    for h in ["Kind", "Where", "Largest", "Note", ""] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for r in rows {
                        ui.label(r.kind);
                        ui.label(&r.place);
                        ui.label(&r.max);
                        ui.label(egui::RichText::new(&r.note).small().weak());
                        let enabled = r.action != RowAction::None;
                        if ui
                            .add_enabled(enabled, egui::Button::new(r.button).small())
                            .clicked()
                        {
                            out = Some(r.action.clone());
                        }
                        ui.end_row();
                    }
                });
        },
    );
    out
}

/// The request opening a cross-section panel on the plane of the run's
/// cut `c` (± `width`): at its alignment station, which the panel places
/// as the engine did (one implementation, `ohl_line::route::plane_at`;
/// 2026-10-31: cuts are made on the alignment).
pub fn open_cut(c: &ohl_electrical::magnetic::FieldCut, width: f64) -> RowAction {
    RowAction::Ask(FieldRequest::OpenCut {
        alignment: c.alignment.clone(),
        station: c.station,
        width,
    })
}

/// The request looking at `target` (a volume's surfaces' centre) across
/// the span `from`–`to`, from far enough to see `extent` m of it.
pub fn look(model: &Model, from: &str, to: &str, target: [f64; 3], extent: f64) -> RowAction {
    let at = |id: &str| model.line.structure(id).map(|s| s.position);
    let (Some(a), Some(b)) = (at(from), at(to)) else {
        return RowAction::None;
    };
    let ahead = (b[1] - a[1]).atan2(b[0] - a[0]);
    RowAction::Ask(FieldRequest::Look {
        target,
        distance: (0.6 * extent).clamp(60.0, 2000.0),
        // From the side, a little ahead (the span's length seen).
        yaw: ahead + std::f64::consts::FRAC_PI_2 * 0.6,
    })
}

/// The centre of meshes' vertices; none without one.
pub fn centre<'a>(
    meshes: impl IntoIterator<Item = &'a ohl_electrical::contour::Mesh>,
) -> Option<[f64; 3]> {
    let (mut sum, mut n) = ([0.0; 3], 0usize);
    for m in meshes {
        for v in &m.vertices {
            for k in 0..3 {
                sum[k] += v[k];
            }
            n += 1;
        }
    }
    (n > 0).then(|| sum.map(|s| s / n as f64))
}
