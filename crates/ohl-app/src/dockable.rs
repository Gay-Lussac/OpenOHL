// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tool windows that can also be panels of the dock: the editor windows
//! (structure, string, section, jumper), the pylon builder, Replace
//! structure, Display weather cases, and every check and result window of
//! the menus (through [`crate::check_help::window`], owner 2026-10-27:
//! one design for all of them). Each comes up as a floating window where it
//! always did; **Dock as a panel** (📌, a small button in its title bar —
//! owner: OHL-Design UI review, 2026-10-26) makes it a tab
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
    /// Edit › Tools › New pylon from a template.
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
    /// Edit › Tools › Measurements panel.
    Measurements,
    /// The Follow and snap tool's panel.
    Follow,
    /// Checks › Safety volumes.
    SafetyVolumes,
    /// Checks › Thermal rating.
    Thermal,
    /// Checks › Electrical › Line parameters.
    LineParameters,
    /// Checks › Electrical › Magnetic field.
    MagneticField,
    /// Checks › Analyses › Galloping ellipses.
    Galloping,
    /// Checks › Analyses › Wire tensions.
    WireTensions,
    /// Checks › Analyses › String checks.
    StringChecks,
    /// Checks › Wire clearances › Internal clearances.
    InternalClearances,
    /// Checks › As-built wires.
    AsBuiltWires,
    /// Checks › (the plugin's submenu) › Distance between conductors.
    PhaseDistance,
    /// Checks › (the plugin's submenu) › Distance between conductors under
    /// unequal wind.
    UnequalWind,
    /// Checks › Performed checks.
    CheckLibrary,
    /// Checks › Compute structure loads.
    Loads,
    /// Bill of materials › Wires.
    BillOfMaterials,
    /// Checks › Wire clearances › Show results (a page until 2026-10-27).
    Clearances,
    /// Line › Structures › Spot structures.
    Spot,
}

impl Panel {
    /// Every one.
    pub const ALL: [Panel; 25] = [
        Panel::Editor,
        Panel::PylonBuilder,
        Panel::Replace,
        Panel::VerticalClearances,
        Panel::RadialClearances,
        Panel::DisplayCases,
        Panel::Vegetation,
        Panel::Measurements,
        Panel::Follow,
        Panel::SafetyVolumes,
        Panel::Thermal,
        Panel::LineParameters,
        Panel::MagneticField,
        Panel::Galloping,
        Panel::WireTensions,
        Panel::StringChecks,
        Panel::InternalClearances,
        Panel::AsBuiltWires,
        Panel::PhaseDistance,
        Panel::UnequalWind,
        Panel::CheckLibrary,
        Panel::Loads,
        Panel::BillOfMaterials,
        Panel::Clearances,
        Panel::Spot,
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
            Panel::SafetyVolumes => "Safety volumes",
            Panel::Thermal => "Thermal rating",
            Panel::LineParameters => "Line parameters",
            Panel::MagneticField => "Magnetic field",
            Panel::Galloping => "Galloping ellipses",
            Panel::WireTensions => "Wire tensions",
            Panel::StringChecks => "String checks",
            Panel::InternalClearances => "Internal clearances",
            Panel::AsBuiltWires => "As-built wires",
            Panel::PhaseDistance => "Distance between conductors",
            Panel::UnequalWind => "Under unequal wind",
            Panel::CheckLibrary => "Performed checks",
            Panel::Loads => "Structure loads",
            Panel::BillOfMaterials => "Bill of materials: wires",
            Panel::Clearances => "Clearances",
            Panel::Spot => "Spot structures",
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

/// The accessible name and the hover text of the button that docks a
/// window.
pub const DOCK: (&str, &str) = (
    "Dock as a panel",
    "Dock as a panel: a tab of the main window — drag it anywhere there, or slide it aside; \
     it still closes when its tool is done, and comes up docked next time",
);

/// The accessible name and the hover text of the button that floats a
/// docked panel.
pub const FLOAT: (&str, &str) = (
    "Float",
    "Float: a window again (its tab can also be dragged out of the main window)",
);

/// The small icon button that docks (📌) or floats (⬈) it; its name for
/// screen readers and tests, [`DOCK`] or [`FLOAT`].
fn dock_button(ui: &mut egui::Ui, panel: Panel, docked: bool) {
    let (icon, (name, tip)) = if docked {
        ("⬈", FLOAT)
    } else {
        ("📌", DOCK)
    };
    let r = ui.add(egui::Button::new(egui::RichText::new(icon).small()).frame(false));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    if r.on_hover_text(tip).clicked() {
        host(ui.ctx(), |h| h.requests.push((panel, !docked)));
    }
}

/// The dock button in the title bar of the window whose content `ui`
/// holds (left of its close button, `closable`): drawn by the content, in
/// the title bar's row above it (no layer of its own over other windows).
fn title_bar_button(ui: &mut egui::Ui, panel: Panel, closable: bool) {
    let content = ui.max_rect();
    let h = ui.spacing().interact_size.y;
    let right = content.right() - if closable { h + 2.0 } else { 0.0 };
    let top = content.top() - h - ui.spacing().item_spacing.y - 2.0;
    let rect = egui::Rect::from_min_size(egui::pos2(right - h, top), egui::vec2(h, h));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.set_clip_rect(rect.expand(2.0));
    dock_button(&mut child, panel, false);
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
        let closable = open.is_some();
        let window = match open {
            Some(o) => window.open(o),
            None => window,
        };
        let shown = window.show(ctx, |ui| {
            title_bar_button(ui, panel, closable);
            // As wide as when its dock button had a row of its own there
            // (a window's size, and so whether its lists scroll, as
            // before).
            ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
            add(ui)
        });
        return shown
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        dock_button(ui, panel, true)
                    });
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
