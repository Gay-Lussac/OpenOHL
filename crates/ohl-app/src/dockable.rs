// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tool windows that can also be panels of the dock: the editor windows
//! (structure, string, section, jumper), the pylon builder, Replace
//! structure, the ground clearance windows,
//! Display weather cases. Each comes up as a floating window where it
//! always did; **Dock as a panel** (its button, top right) makes it a tab
//! of the main window — dragged anywhere there, into another group or out
//! into a window of its own, slid aside like any page — and **Float**
//! makes it a window again. It still closes when its tool is done (Apply,
//! Cancel, the tool finished) and its tab with it; closing its tab closes
//! the tool. Docked once, it comes up docked the next time.
//!
//! How: the window's content is drawn by its tool as before (in the same
//! frame, with the same state); docked, it is drawn into the rectangle its
//! tab had in the last frame, on a layer just above the tab's
//! ([`egui::Context::set_sublayer`]). The tab itself only marks the place
//! ([`record`]). The dock and the tools meet in the context's memory
//! ([`Host`]): no tool needs the dock.

use std::collections::{BTreeMap, BTreeSet};

use eframe::egui;

/// A tool window that can be docked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Panel {
    /// The editor window: a structure, string, section, jumper….
    Editor,
    /// Tools › New pylon from a template.
    PylonBuilder,
    /// Replace structure.
    Replace,
    /// Checks › Ground clearances › Vertical clearances.
    VerticalClearances,
    /// Checks › Ground clearances › Radial clearances.
    RadialClearances,
    /// View › Display weather cases.
    DisplayCases,
    /// Checks › Vegetation (the danger-tree panel).
    Vegetation,
    /// Tools › Measurements panel.
    Measurements,
    /// The Follow and snap tool's panel.
    Follow,
}

impl Panel {
    /// Every one.
    pub const ALL: [Panel; 9] = [
        Panel::Editor,
        Panel::PylonBuilder,
        Panel::Replace,
        Panel::VerticalClearances,
        Panel::RadialClearances,
        Panel::DisplayCases,
        Panel::Vegetation,
        Panel::Measurements,
        Panel::Follow,
    ];

    /// A name until its window gave its title.
    pub fn name(self) -> &'static str {
        match self {
            Panel::Editor => "Editor",
            Panel::PylonBuilder => "New structure from a template",
            Panel::Replace => "Replace structure",
            Panel::VerticalClearances => "Vertical clearances",
            Panel::RadialClearances => "Radial clearances",
            Panel::DisplayCases => "Display weather cases",
            Panel::Vegetation => "Vegetation and danger trees",
            Panel::Measurements => "Measurements",
            Panel::Follow => "Follow and snap",
        }
    }
}

/// Where a docked panel's tab was drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    layer: egui::LayerId,
    rect: egui::Rect,
    clip: egui::Rect,
    frame: u64,
}

/// What the dock and the tool windows share (kept in the context's
/// memory).
#[derive(Debug, Clone, Default)]
pub struct Host {
    /// Shown as tabs of the dock (remembered for the next time they open).
    pub docked: BTreeSet<Panel>,
    /// Shown this frame (their tool is open), with their titles.
    pub alive: BTreeMap<Panel, String>,
    /// Asked to be docked (true) or to float (false), done after the frame.
    pub requests: Vec<(Panel, bool)>,
    /// Their tab was closed: the tool closes.
    pub closed: BTreeSet<Panel>,
    slots: BTreeMap<Panel, Slot>,
}

fn id() -> egui::Id {
    egui::Id::new("dockable-host")
}

/// Reads or changes the shared state.
pub fn host<R>(ctx: &egui::Context, f: impl FnOnce(&mut Host) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<Host>(id())))
}

/// The tab of `panel` was drawn at `ui` (its whole place): the panel's
/// content goes there.
pub fn record(ui: &mut egui::Ui, panel: Panel) {
    let rect = ui.available_rect_before_wrap();
    let slot = Slot {
        layer: ui.layer_id(),
        rect,
        clip: ui.clip_rect(),
        frame: ui.ctx().cumulative_frame_nr(),
    };
    let moved = host(ui.ctx(), |h| h.slots.insert(panel, slot))
        .is_none_or(|s| s.rect != slot.rect || s.layer != slot.layer);
    if moved {
        // The content follows in the next frame.
        ui.ctx().request_repaint();
    }
    ui.allocate_rect(rect, egui::Sense::hover());
}

/// What [`show`] did.
#[derive(Debug, PartialEq)]
pub enum Shown<R> {
    /// Drawn (as a window or a panel): what `add` returned.
    Drawn(R),
    /// Docked, its tab not in front: not drawn this frame.
    Hidden,
    /// Its tab was closed by the user: close the tool.
    Closed,
}

/// The button that docks or floats it, at the right of its first row.
fn dock_button(ui: &mut egui::Ui, panel: Panel, docked: bool) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        let (text, tip) = if docked {
            (
                "⬈ Float",
                "Make it a window again (its tab can also be dragged out of the main window)",
            )
        } else {
            (
                "📌 Dock as a panel",
                "Make it a tab of the main window: drag it anywhere there, or slide it aside; it \
                 still closes when its tool is done, and comes up docked next time",
            )
        };
        if ui.small_button(text).on_hover_text(tip).clicked() {
            host(ui.ctx(), |h| h.requests.push((panel, !docked)));
        }
    });
}

/// Shows tool window `panel` titled `title`: as `window` (with `open`,
/// its close button, when given), or docked as a tab of the main window.
/// `add` draws its content either way.
pub fn show<R>(
    ctx: &egui::Context,
    panel: Panel,
    title: impl Into<String>,
    window: egui::Window<'_>,
    open: Option<&mut bool>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Shown<R> {
    let title = title.into();
    let (docked, closed, slot) = host(ctx, |h| {
        h.alive.insert(panel, title.clone());
        (
            h.docked.contains(&panel),
            h.closed.remove(&panel),
            h.slots.get(&panel).copied(),
        )
    });
    if closed {
        if let Some(o) = open {
            *o = false;
        }
        return Shown::Closed;
    }
    if !docked {
        let window = match open {
            Some(o) => window.open(o),
            None => window,
        };
        return window
            .show(ctx, |ui| {
                ui.horizontal(|ui| dock_button(ui, panel, false));
                add(ui)
            })
            .and_then(|r| r.inner)
            .map_or(Shown::Hidden, Shown::Drawn);
    }
    // Docked: drawn where its tab was in the last frame, when it was drawn.
    let Some(slot) = slot.filter(|s| s.frame + 1 >= ctx.cumulative_frame_nr()) else {
        return Shown::Hidden;
    };
    let area_id = egui::Id::new(("docked-panel", panel));
    let area = egui::Area::new(area_id)
        .order(slot.layer.order)
        .fixed_pos(slot.rect.min)
        .constrain(false);
    let layer = egui::LayerId::new(slot.layer.order, area_id);
    ctx.set_sublayer(slot.layer, layer);
    let shown = area.show(ctx, |ui| {
        ui.set_clip_rect(slot.clip.intersect(slot.rect));
        ui.set_min_size(slot.rect.size());
        ui.set_max_size(slot.rect.size());
        egui::ScrollArea::both()
            .id_salt(("docked-panel-scroll", panel))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(&title);
                    dock_button(ui, panel, true);
                });
                ui.separator();
                add(ui)
            })
            .inner
    });
    Shown::Drawn(shown.inner)
}

/// At the start of a frame: no panel is known to be shown yet.
pub fn begin_frame(ctx: &egui::Context) {
    host(ctx, |h| h.alive.clear());
}
