// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The memory one calculation may take (owner, 2026-11-12: "the limit
//! for the maximum RAM of those calculations should be a parameter that
//! can be set for the whole software and it should be able to detect the
//! maximum system RAM available"): today the electric field's tiled
//! floors ([`ohl_electrical::electric::floor_memory`]). Kept with the
//! application, for every project.

use eframe::egui;
use ohl_electrical::electric;

/// The key in the application's storage, GB.
const KEY: &str = "calculation.memory_gb";

/// The most that may be set: this share of the computer's memory (the
/// rest for the system, the application and the other floors solved at
/// the same time).
const SHARE: f64 = 0.75;

/// The limit set (the electric field takes it), from the application's
/// storage.
pub fn load(storage: &dyn eframe::Storage) {
    let gb = storage
        .get_string(KEY)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0);
    electric::set_floor_memory(gb.map(|g| g.min(most_gb()) * 1e9));
}

/// Writes the limit in force.
pub fn save(storage: &mut dyn eframe::Storage) {
    storage.set_string(KEY, (electric::floor_memory() / 1e9).to_string());
}

/// The most that may be set, GB: [`SHARE`] of this computer's memory
/// (64 GB when it could not be found out).
pub fn most_gb() -> f64 {
    ohl_core::memory::system_bytes().map_or(64.0, |b| (b as f64 / 1e9 * SHARE).max(0.5))
}

/// The setting's row: the limit, this computer's memory, what it does.
pub fn ui(ui: &mut egui::Ui) {
    let mut gb = electric::floor_memory() / 1e9;
    let most = most_gb();
    ui.horizontal(|ui| {
        ui.label("Memory one calculation may take");
        let r = crate::style::field(
            ui,
            egui::DragValue::new(&mut gb)
                .range(0.5..=most)
                .speed(0.1)
                .max_decimals(1)
                .suffix(" GB"),
        )
        .on_hover_text(
            "Electric field on the tiled floor: a floor needing more is refused before the \
             run starts (coarser tiles, a smaller width or reach, or the plane under each \
             point). Larger: finer tiles allowed, but the computer slows down or stops \
             once the memory runs out. Kept for every project.",
        );
        if r.changed() {
            electric::set_floor_memory(Some(gb * 1e9));
        }
        ui.label(
            egui::RichText::new(match ohl_core::memory::system_bytes() {
                Some(b) => format!(
                    "this computer: {:.0} GB (at most {most:.1} GB offered); default {:.0} GB",
                    b as f64 / 1e9,
                    electric::FLOOR_MEMORY / 1e9
                ),
                None => format!(
                    "this computer's memory not found out; default {:.0} GB",
                    electric::FLOOR_MEMORY / 1e9
                ),
            })
            .small()
            .weak(),
        );
    });
}
