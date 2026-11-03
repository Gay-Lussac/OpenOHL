// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A field preset handed to the field windows (owner, 2026-11-03: the
//! Buildings window "should only offer to find the buildings and then
//! setup the presets for the other field tools, not run them itself"):
//! the cuts across the line and the places of sensitive use a plugin
//! with field presets gave, which the Magnetic field and Electric field
//! windows add to their own cuts and points when they run. Kept in each
//! window's project setting.

use eframe::egui;
use ohl_electrical::magnetic;
use serde::{Deserialize, Serialize};

/// A cut of a preset, at an alignment's station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    /// Its label (the building).
    pub label: String,
    /// The alignment.
    pub alignment: String,
    /// The station, m.
    pub station: f64,
    /// ± this across the line, m.
    pub width: f64,
    /// Up to this over the highest wire, m.
    pub above: f64,
    /// The grid's step, m.
    pub step: f64,
}

/// A place of sensitive use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// Its name.
    pub name: String,
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Elevation, m (none: `height` above the ground).
    pub z: Option<f64>,
    /// Height above the ground, m.
    pub height: Option<f64>,
}

/// A plugin's preset as the field windows keep it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    /// The plugin that gave it.
    pub plugin: String,
    /// The preset's name.
    pub name: String,
    /// Its cuts.
    pub cuts: Vec<Cut>,
    /// Its places.
    pub places: Vec<Place>,
    /// Its isolines of the magnetic field, µT.
    pub levels_ut: Vec<f64>,
    /// Its isolines of the electric field, kV/m.
    pub levels_kv_m: Vec<f64>,
}

impl Preset {
    /// From a plugin's preset.
    pub fn from_plugin(plugin: &str, p: &ohl_plugins::FieldPreset) -> Self {
        Self {
            plugin: plugin.to_owned(),
            name: p.name.clone(),
            cuts: p
                .cuts
                .iter()
                .map(|c| Cut {
                    label: c.label.clone(),
                    alignment: c.alignment.clone(),
                    station: c.station,
                    width: c.width,
                    above: c.above,
                    step: c.step,
                })
                .collect(),
            places: p
                .points
                .iter()
                .map(|q| Place {
                    name: q.name.clone(),
                    x: q.x,
                    y: q.y,
                    z: q.z,
                    height: q.height,
                })
                .collect(),
            levels_ut: p.levels_ut.clone(),
            levels_kv_m: p.levels_kv_m.clone(),
        }
    }

    /// Its cuts as the fields take them.
    pub fn cut_specs(&self) -> Vec<magnetic::CutSpec> {
        self.cuts
            .iter()
            .map(|c| magnetic::CutSpec {
                at: magnetic::CutAt::Station {
                    alignment: c.alignment.clone(),
                    station: c.station,
                },
                width: c.width,
                above: c.above,
                step: c.step,
                label: (!c.label.is_empty()).then(|| c.label.clone()),
            })
            .collect()
    }

    /// Its places as the fields take them.
    pub fn points(&self) -> Vec<magnetic::GivenPoint> {
        self.places
            .iter()
            .map(|q| magnetic::GivenPoint {
                name: q.name.clone(),
                x: q.x,
                y: q.y,
                z: q.z,
                height: q.height,
            })
            .collect()
    }

    /// In words: "building-cuts › default: 12 cuts, 48 places".
    pub fn words(&self) -> String {
        format!(
            "{} › {}: {} cut{}, {} place{}",
            self.plugin,
            self.name,
            self.cuts.len(),
            if self.cuts.len() == 1 { "" } else { "s" },
            self.places.len(),
            if self.places.len() == 1 { "" } else { "s" }
        )
    }
}

/// The levels `own` with the preset's added (sorted, each once).
pub fn with_levels(own: Vec<f64>, preset: Option<&[f64]>) -> Vec<f64> {
    let mut l: Vec<f64> = own
        .into_iter()
        .chain(preset.unwrap_or(&[]).iter().copied())
        .collect();
    l.sort_by(f64::total_cmp);
    l.dedup();
    l
}

/// A field window's row for the buildings' preset: what it holds and
/// Clear; `planes`: the window computes cuts (2D), else it says the
/// preset is used in 2D only. Returns whether it changed.
pub fn row(ui: &mut egui::Ui, preset: &mut Option<Preset>, planes: bool) -> bool {
    let mut changed = false;
    ui.label("Buildings").on_hover_text(
        "Cuts and places of sensitive use set up by Checks › Electrical › Buildings",
    );
    ui.horizontal(|ui| match preset {
        Some(p) => {
            let text = if planes {
                p.words()
            } else {
                format!("{} (used by 2D runs only)", p.words())
            };
            ui.label(text).on_hover_text(
                "Run with the window's own cuts and points: each cut across the line at its \
                 station, each place as a point",
            );
            if ui.small_button("Clear").clicked() {
                *preset = None;
                changed = true;
            }
        }
        None => {
            ui.label(
                egui::RichText::new("none (Checks › Electrical › Buildings sets them up)").weak(),
            );
        }
    });
    ui.end_row();
    changed
}
