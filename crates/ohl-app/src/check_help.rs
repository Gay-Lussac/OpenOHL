// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The explanations shared by the check panels — the vegetation and
//! danger-tree panel, the Vertical and Radial clearances windows and the
//! Safety volumes window — so
//! that they read as one family: a heading per group of settings with a
//! link to its user-guide section, a line or two beside each value (a
//! longer text on hover), the wind choice (none, steps, swing arc or
//! both, the sides, how many solves that means), the LiDAR class ticks
//! with their colours, and small line drawings (about 120 × 80 px, drawn
//! with the painter in the theme's colours) that follow the values typed:
//! the falling tree with its root-ball hinge and allowance, grow-in
//! around a wire, the wind positions (swing arc, steps), a tree base and
//! its nearest ground point, unbalanced ice on one span, the vertical
//! zone under a wire and the radial distance around it; for the
//! safety-volume window, the distance around a conductor, the volumes
//! per wire, circuit or system, and the mesh outside the exact volume
//! where two tubes meet.
//!
//! Every check window ends with the same **Run footer** ([`body`],
//! [`footer`], [`run_button`], [`stop_button`]): Run (Stop and the
//! progress while it runs) at the bottom left, under the scrolled body,
//! always visible however long the window's content is (owner,
//! 2026-10-21).
//!
//! And every check and result window has **one design** (owner,
//! 2026-10-27, [`window`]): a floating window that docks as a panel with
//! its 📌 (and floats again with ⬈), the same spacing ([`ITEM_SPACING`],
//! [`GRID_SPACING`]) and lists whose height the user drags ([`list`]).

use eframe::egui;
use ohl_clearance::rules::{WindSides, WindSteps};

/// Solves per windy case above which the wind choice suggests the swing
/// arc.
pub const MANY_SOLVES: usize = 40;

/// A heading of a group of settings, with a link to its section of the
/// user guide: the anchor when the link is clicked.
pub fn heading(ui: &mut egui::Ui, title: &str, anchor: &'static str) -> Option<&'static str> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.strong(title);
        if ui
            .small_button(format!("{title}: user guide ↗"))
            .on_hover_text(format!("Help › User guide: {title} (and its theory)"))
            .clicked()
        {
            out = Some(anchor);
        }
    });
    out
}

/// Space between the items of a check window (owner, 2026-10-27: room to
/// breathe; egui's default is 8 × 3 px).
pub const ITEM_SPACING: egui::Vec2 = egui::vec2(10.0, 6.0);

/// Space between the columns and the rows of a check window's tables,
/// px.
pub const GRID_SPACING: [f32; 2] = [14.0, 6.0];

/// The height a list of a check window starts with, px (before: 220–360);
/// [`list`] lets the user change it.
pub const LIST_HEIGHT: f32 = 320.0;

/// The smallest height a [`list`] is dragged to, px.
const LIST_MIN: f32 = 60.0;

/// The accessible name of the handle under a [`list`] (for screen readers
/// and tests).
pub const LIST_HANDLE: &str = "Drag to make the list taller or shorter";

/// A check or result window of the menus (owner, 2026-10-27: one design
/// for all of them): floating at first, `size` wide and high, closed with
/// its ×; **📌 Dock as a panel** in its title bar makes it a tab of the
/// main window, **⬈ Float** a window again, remembered for the session
/// ([`crate::dockable`]); the shared spacing ([`ITEM_SPACING`]). `add`
/// draws its content (usually a [`body`] and a [`footer`]); `None` when it
/// is not drawn this frame (closed, or docked behind another tab).
pub fn window<R>(
    ctx: &egui::Context,
    panel: crate::dockable::Panel,
    title: &str,
    size: [f32; 2],
    open: &mut bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    if !*open {
        return None;
    }
    let w = egui::Window::new(title)
        .default_width(size[0])
        .default_height(size[1]);
    match crate::dockable::show(ctx, panel, title, w, Some(open), |ui| {
        ui.spacing_mut().item_spacing = ITEM_SPACING;
        add(ui)
    }) {
        crate::dockable::Shown::Drawn(r) => Some(r),
        _ => None,
    }
}

/// The memory key of a list's height.
fn list_key(id: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::new(("check-list-height", id))
}

/// The height list `id` is shown at: as the user dragged it this session,
/// else `default`.
pub fn list_height(
    ctx: &egui::Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    default: f32,
) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(list_key(id)))
        .unwrap_or(default)
}

/// A list of a check window (owner, 2026-10-27): `scroll` (its scroll
/// area, as the window builds it) at most `default` px high, or as high
/// as the user dragged the handle under it — remembered per list (`id`)
/// for the session, docked or floating. A list shorter than its height
/// takes only its own.
pub fn list<R>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    default: f32,
    scroll: egui::ScrollArea,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let height = list_height(ui.ctx(), id, default);
    let out = scroll.max_height(height).show(ui, add);
    let (inner, inner_width) = (out.inner, out.inner_rect.width());
    // The handle: a short grip across, the cursor saying it resizes; as
    // wide as the list (not the space left: a window sized to its content
    // would widen itself every frame and move its buttons).
    let width = inner_width.max(40.0);
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::drag());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, LIST_HANDLE));
    let r = r
        .on_hover_cursor(egui::CursorIcon::ResizeVertical)
        .on_hover_text(LIST_HANDLE);
    let colour = if r.hovered() || r.dragged() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    for dy in [-1.5, 1.5] {
        let y = rect.center().y + dy;
        ui.painter().line_segment(
            [
                egui::pos2(rect.center().x - 16.0, y),
                egui::pos2(rect.center().x + 16.0, y),
            ],
            egui::Stroke::new(1.0, colour),
        );
    }
    if r.dragged() {
        let dy = r.drag_delta().y;
        if dy != 0.0 {
            let new = (height + dy).max(LIST_MIN);
            ui.data_mut(|d| d.insert_temp(list_key(id), new));
            ui.ctx().request_repaint();
        }
    }
    inner
}

/// The memory key of a check window's footer height.
fn footer_key(id: &str) -> egui::Id {
    egui::Id::new(("check-footer", id))
}

/// The smallest height of a check window's body, px (the footer goes
/// below it when the window is shorter).
const MIN_BODY: f32 = 80.0;

/// The body of a check window, above its Run [`footer`] (same `id`): a
/// scroll area filling the window down to the footer, so that
/// the footer stays at the bottom and never scrolls away.
pub fn body<R>(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.spacing_mut().item_spacing = ITEM_SPACING;
    let key = footer_key(id);
    let height = room(ui, id);
    // Across too: a narrow window or docked tab (the Clearances window) scrolls
    // sideways rather than cutting its rows.
    egui::ScrollArea::both()
        .id_salt((key, "body"))
        .auto_shrink([false, false])
        .max_height(height)
        .show(ui, add)
        .inner
}

/// The height a check window's body (or its Results tab,
/// [`crate::check_ui::results_area`]) has above its [`footer`] (same
/// `id`): the room left down to the footer, at least a few lines.
pub fn room(ui: &egui::Ui, id: &str) -> f32 {
    let footer = ui
        .data(|d| d.get_temp::<f32>(footer_key(id)))
        .unwrap_or(ui.spacing().interact_size.y + 3.0 * ui.spacing().item_spacing.y + 6.0);
    let below = ui.available_rect_before_wrap();
    // The room left: the window's, or what is visible of it (docked in a
    // scrolled page, or taller than the screen).
    let room = below.height().min(ui.clip_rect().bottom() - below.top());
    (room - footer).max(MIN_BODY)
}

/// The Run footer of a check window, under its [`body`] (same `id`): a
/// line, then `add` from the left (Run, Stop, the progress).
pub fn footer<R>(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let top = ui.cursor().top();
    ui.separator();
    let inner = ui.horizontal_wrapped(add).inner;
    let height = ui.cursor().top() - top;
    if height.is_finite() && height > 0.0 {
        ui.data_mut(|d| d.insert_temp(footer_key(id), height));
    }
    inner
}

/// The Run button of a footer: greyed with `why_not` on hover when it
/// cannot run (`None` and `enabled` false: a run under way). True when
/// clicked.
pub fn run_button(ui: &mut egui::Ui, enabled: bool, hover: &str, why_not: Option<&str>) -> bool {
    let enabled = enabled && why_not.is_none();
    ui.add_enabled(enabled, egui::Button::new("Run"))
        .on_hover_text(hover)
        .on_disabled_hover_text(why_not.unwrap_or("A run is under way"))
        .clicked()
}

/// The Stop button of a footer, while a run is under way. True when
/// clicked.
pub fn stop_button(ui: &mut egui::Ui, hover: &str) -> bool {
    ui.button("Stop").on_hover_text(hover).clicked()
}

/// A LiDAR class's colour swatch (as drawn in the views), before its tick.
pub fn class_swatch(ui: &mut egui::Ui, scheme: &ohl_lidar::ClassScheme, code: u8) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    if let Some(col) = crate::scene::class_color(scheme, code) {
        ui.painter()
            .rect_filled(rect, 2.0, egui::Color32::from_rgb(col[0], col[1], col[2]));
    }
}

/// The solves a windy case of `full` Pa needs with `wind` (`sides_all`:
/// the sides apply to the full pressure too, as in the clearance windows;
/// else the full pressure is solved from both sides, as in the
/// danger-tree check): still air, every pressure of the steps from each
/// side, the full blowouts.
pub fn solves_per_case(wind: &WindSteps, full: f64, sides_all: bool) -> usize {
    if full <= 0.0 {
        return 1;
    }
    let sides = wind.sides.sides().len();
    let full_sides = if sides_all { sides } else { 2 };
    let mut n = full_sides;
    if wind.stepped {
        let steps = wind.pressures(full, ohl_clearance::zone::MAX_STEPS);
        // 0 Pa once, every pressure between from each side.
        n += 1 + steps.iter().filter(|p| **p > 0.0 && **p < full).count() * sides;
    }
    if wind.arc && !wind.stepped {
        n += 1;
    }
    n
}

/// The wind choice of a check: the swing arc and the wind steps (both,
/// either or none — none: each case's own pressure), the step and the
/// sides; how many solves a windy case of the largest pressure `full`
/// needs, and the arc suggested when the steps need many. `sides_all`:
/// the sides apply to every position (the clearance windows), else to
/// the steps only (the danger-tree check). `jumpers`: the check includes
/// jumpers (their band margin then editable; else shown greyed, as the
/// check leaves them out). `arc`: the swing arc offered. True when
/// something changed.
pub fn wind_choice(
    ui: &mut egui::Ui,
    id: &str,
    wind: &mut WindSteps,
    sides_all: bool,
    full: f64,
    jumpers: bool,
    arc: bool,
) -> bool {
    let mut changed = false;
    if !arc {
        wind.arc = false;
    }
    ui.horizontal(|ui| {
        if arc {
            changed |= ui
                .checkbox(&mut wind.arc, "swing arc")
                .on_hover_text(
                    "Each span also anywhere on its swing arc between still air and the full \
                     blowouts: three solves per windy case, one closed-form distance per point",
                )
                .changed();
        }
        changed |= ui
            .checkbox(&mut wind.stepped, "wind stepped")
            .on_hover_text(
                "Still air and every pressure from 0 up to the case's, by the step: one solve per \
                 pressure and side",
            )
            .changed();
        ui.add_enabled_ui(wind.stepped, |ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut wind.step)
                        .speed(10.0)
                        .range(1.0..=10_000.0)
                        .suffix(" Pa"),
                )
                .on_hover_text("The step of the wind pressure")
                .changed();
        });
        ui.add_enabled_ui(sides_all || wind.stepped, |ui| {
            ui.label("from");
            let name = |s: WindSides| match s {
                WindSides::Both => "both sides",
                WindSides::Left => "the left",
                WindSides::Right => "the right",
            };
            egui::ComboBox::from_id_salt((id, "wind-sides"))
                .selected_text(name(wind.sides))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for w in WindSides::ALL {
                        changed |= ui.selectable_value(&mut wind.sides, w, name(w)).changed();
                    }
                })
                .response
                .on_hover_text("Left and right looking ahead along the line");
        });
    });
    if wind.arc {
        changed |= band_margin_ui(ui, id, &mut wind.band_margin);
        changed |= jumper_band_margin_ui(ui, id, &mut wind.jumper_band_margin, jumpers);
    }
    let what = match (wind.arc, wind.stepped) {
        (false, false) => "None ticked: each case's own pressure, blown to each side.",
        (true, false) => "Swing arc: anywhere between the blowouts (closed form).",
        (false, true) => {
            "Wind stepped: still air and every pressure up to the case's, by the step."
        }
        (true, true) => "Both: every step solved, and the swing arc between the blowouts.",
    };
    explain(
        ui,
        what,
        if sides_all {
            "Alongside each other or alone. The swing arc swings each span as one about the line \
             through its attachment points, between still air and the full blowouts of the sides \
             chosen, at every radius from the smallest to the largest of theirs, widened by \
             the band margin (conservative): every angle and radius between is covered. The \
             steps solve the wires at 0, step, 2 × step … up to the case's pressure: the exact \
             positions at those pressures, nothing between them. Left and right looking ahead \
             along the line."
        } else {
            "Alongside each other or alone. The swing arc covers every position between the two \
             blowouts (closed form); the steps solve the wires at 0, step, 2 × step … up to the \
             case's pressure, from the sides chosen, as the vertical and radial clearance windows \
             do. The full pressure is checked from both sides in any case."
        },
    );
    if full > 0.0 {
        let n = solves_per_case(wind, full, sides_all);
        let text = format!(
            "{n} solve{} for a windy case of {full:.0} Pa.",
            if n == 1 { "" } else { "s" }
        );
        if wind.stepped && n > MANY_SOLVES {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                egui::RichText::new(format!(
                    "{text} The swing arc needs 3 and covers every angle between the blowouts \
                     (a rigid swing over the band of radii: conservative, not exact between)."
                ))
                .small(),
            )
            .on_hover_text(
                "Each step is a solve of every section in the case; on a long line a fine step \
                 takes minutes. The arc swings each span as one: steps are exact at the \
                 pressures they solve, the arc covers every angle in between.",
            );
        } else {
            ui.label(egui::RichText::new(text).small().weak());
        }
    }
    changed
}

/// The swing band's margin (2026-10-21, the owner's decision: editable
/// wherever the band is used): a field in metres, with the shared
/// explanation — what it covers, that it is conservative, and that a
/// smaller value is less safe. True when changed.
pub fn band_margin_ui(ui: &mut egui::Ui, id: &str, margin: &mut f64) -> bool {
    let default = ohl_clearance::positions::BAND_MARGIN;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("band margin");
        changed |= ui
            .push_id((id, "band-margin"), |ui| {
                crate::style::field(
                    ui,
                    egui::DragValue::new(margin)
                        .speed(0.01)
                        .range(0.0..=5.0)
                        .suffix(" m"),
                )
            })
            .inner
            .on_hover_text(BAND_MARGIN_HELP)
            .changed();
        if *margin != default
            && ui
                .small_button(format!("{default} m"))
                .on_hover_text("Back to the default margin")
                .clicked()
        {
            *margin = default;
            changed = true;
        }
    });
    if *margin < default {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            egui::RichText::new(format!(
                "Smaller than the default {default} m: less safe, on your responsibility."
            ))
            .small(),
        )
        .on_hover_text(BAND_MARGIN_HELP);
    } else {
        explain(
            ui,
            "Band margin: every distance to the swing band taken this much smaller \
             (conservative).",
            BAND_MARGIN_HELP,
        );
    }
    changed
}

/// The band margin's longer explanation (hover).
pub const BAND_MARGIN_HELP: &str = "The swing band (from the smallest to the largest radius of the \
    still and blown positions) does not quite hold the wire at intermediate wind pressures: \
    measured with steps of 10 Pa on the example lines, a span's wire lies up to 0.089 m outside \
    it (the strings swing and the sag changes). The margin covers that worst excess plus an \
    allowance for other lines: 0.15 m by default, conservative — every distance to the band is \
    taken this much smaller, as if the conductor were that much thicker. The jumpers' bands have \
    a margin of their own (below). A smaller value is less safe and on your responsibility; to \
    be sure on your line, compare with the wind steps.";

/// The jumpers' swing band margin (2026-10-21, the owner's decision: a
/// value of its own, default 0, entered by the user): a field in metres
/// beside the spans' margin, with the shared explanation — at 0 the
/// jumper's band is not conservative between its blowouts (a
/// Simplification on the user's responsibility; exact: the wind steps),
/// the excess measured on the example line's jumper. `enabled` false:
/// shown greyed (a check without jumpers). True when changed.
pub fn jumper_band_margin_ui(ui: &mut egui::Ui, id: &str, margin: &mut f64, enabled: bool) -> bool {
    let default = ohl_clearance::positions::JUMPER_BAND_MARGIN;
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("jumper band margin");
            changed |= ui
                .push_id((id, "jumper-band-margin"), |ui| {
                    crate::style::field(
                        ui,
                        egui::DragValue::new(margin)
                            .speed(0.01)
                            .range(0.0..=5.0)
                            .suffix(" m"),
                    )
                })
                .inner
                .on_hover_text(JUMPER_BAND_MARGIN_HELP)
                .on_disabled_hover_text(
                    "This check leaves the jumpers out: their band margin does not apply here.",
                )
                .changed();
            if *margin != default
                && ui
                    .small_button(format!("{default} m"))
                    .on_hover_text("Back to the default (no widening)")
                    .clicked()
            {
                *margin = default;
                changed = true;
            }
        });
        if enabled {
            let short = if *margin > 0.0 {
                "Jumper band margin: every distance to a jumper's band taken this much smaller \
                 (your value, on your responsibility)."
            } else {
                "Jumper band margin 0: a jumper's band is not widened and does not hold the \
                 jumper between its blowouts (on your responsibility)."
            };
            explain(ui, short, JUMPER_BAND_MARGIN_HELP);
        }
    });
    changed
}

/// The jumpers' band margin's longer explanation (hover).
pub const JUMPER_BAND_MARGIN_HELP: &str = "A jumper's swing band (about the line through its end \
    clamps, from the smallest to the largest radius of its still and blown positions) does not \
    hold the jumper at intermediate wind pressures: it swings with its clamps and changes its \
    shape. Measured with steps of 10 Pa on the example line's jumper, it lies up to 0.293 m \
    outside its band (high wind, blown right at 230 Pa). The default is 0 m, no widening: a \
    simplification on your responsibility, the band not conservative between the blowouts. \
    Enter a value for your jumpers — compare with the wind steps to find it (exact: the wind \
    steps). Separate from the spans' band margin.";

/// Size of a drawing, px.
pub const SIZE: egui::Vec2 = egui::vec2(120.0, 80.0);

/// The explanation of a setting: one short weak line and a **(?)**
/// after it, the longer text on hover of either (owner: OHL-Design UI
/// review, 2026-10-26 — the help paragraphs behind a (?), one short line
/// kept).
pub fn explain(ui: &mut egui::Ui, short: &str, long: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(egui::RichText::new(short).small().weak())
            .on_hover_text(long);
        question(ui, long);
    });
}

/// A small **(?)**: `long` on hover.
pub fn question(ui: &mut egui::Ui, long: &str) -> egui::Response {
    ui.add(
        egui::Label::new(egui::RichText::new("(?)").small().strong()).sense(egui::Sense::hover()),
    )
    .on_hover_text(long)
}

/// The colours of a drawing in the theme: lines, faint lines, the thing
/// explained, the wire.
struct Ink {
    line: egui::Color32,
    faint: egui::Color32,
    key: egui::Color32,
    wire: egui::Color32,
}

/// A drawing's canvas, named `name` for screen readers (and tests).
fn canvas(ui: &mut egui::Ui, name: &str, tip: &str) -> (egui::Painter, egui::Rect, Ink) {
    let (response, painter) = ui.allocate_painter(SIZE, egui::Sense::hover());
    let name = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &name));
    response.on_hover_text(tip);
    let v = ui.visuals();
    let rect = painter.clip_rect();
    painter.rect_filled(rect, 3.0, v.extreme_bg_color);
    let ink = Ink {
        line: v.text_color(),
        faint: v.weak_text_color(),
        key: v.warn_fg_color,
        wire: v.hyperlink_color,
    };
    (painter, rect, ink)
}

fn dashed(painter: &egui::Painter, a: egui::Pos2, b: egui::Pos2, color: egui::Color32) {
    painter.extend(egui::Shape::dashed_line(
        &[a, b],
        egui::Stroke::new(1.0, color),
        3.0,
        2.0,
    ));
}

fn text(painter: &egui::Painter, at: egui::Pos2, s: &str, color: egui::Color32) {
    painter.text(
        at,
        egui::Align2::LEFT_CENTER,
        s,
        egui::FontId::proportional(9.0),
        color,
    );
}

/// The falling tree: its trunk of height h, the root-ball hinge k·h from
/// it, the sector it sweeps (radius L = √(h² + (k·h)²)) down to the
/// ground, and the allowance A it must keep from the wire.
pub fn falling_tree(ui: &mut egui::Ui, k: f64, allowance: f64) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: falling tree",
        "A tree of height h turns about the edge of its root ball, k·h from its trunk; it sweeps \
         the sector of radius L = √(h² + (k·h)²) down to the ground. Where the sector comes \
         within A of a wire, the tree is a danger tree.",
    );
    let ground = r.bottom() - 10.0;
    let base = egui::pos2(r.left() + 22.0, ground);
    let h = 52.0_f32;
    let kh = (k.clamp(0.0, 0.5) as f32) * h;
    let hinge = egui::pos2(base.x + kh, ground);
    let len = h.hypot(kh);
    p.line_segment(
        [
            egui::pos2(r.left() + 4.0, ground),
            egui::pos2(r.right() - 4.0, ground),
        ],
        egui::Stroke::new(1.0, ink.faint),
    );
    // The trunk, the hinge.
    p.line_segment(
        [base, egui::pos2(base.x, ground - h)],
        egui::Stroke::new(2.0, ink.line),
    );
    p.circle_filled(hinge, 2.5, ink.key);
    if kh > 0.5 {
        p.line_segment([base, hinge], egui::Stroke::new(1.5, ink.key));
    }
    text(&p, egui::pos2(base.x + 2.0, ground + 5.0), "k·h", ink.key);
    // The sector swept from upright to the ground.
    let top = egui::pos2(base.x, ground - h);
    let a0 = (top.y - hinge.y).atan2(top.x - hinge.x);
    let pts: Vec<egui::Pos2> = (0..=24)
        .map(|i| {
            let a = a0 + (0.0 - a0) * i as f32 / 24.0;
            hinge + egui::vec2(a.cos(), a.sin()) * len
        })
        .collect();
    p.add(egui::Shape::line(pts, egui::Stroke::new(1.0, ink.line)));
    // The wire, A beyond the sector.
    let a = (allowance.clamp(0.0, 20.0) as f32) * 2.5;
    let dir = egui::vec2(0.7, -0.7);
    let edge = hinge + dir * len;
    let wire = edge + dir * a.max(3.0);
    p.circle_filled(wire, 3.0, ink.wire);
    dashed(&p, edge, wire, ink.key);
    text(&p, wire + egui::vec2(5.0, 0.0), "A", ink.key);
    text(
        &p,
        egui::pos2(base.x - 12.0, ground - h * 0.5),
        "h",
        ink.line,
    );
}

/// Grow-in: the wire, the required clearance c around it, then the
/// growth g until the next cut and the buffer b; a vegetation point
/// inside the outer ring grows in.
pub fn grow_in(ui: &mut egui::Ui, clearance: f64, growth: f64, buffer: f64) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: grow-in",
        "Around each wire position: the required clearance c, the growth g until the next cut, \
         the buffer b. A vegetation point closer than c + g + b grows in.",
    );
    let c = r.center() + egui::vec2(-18.0, -4.0);
    let total = (clearance + growth + buffer).max(1e-6);
    let scale = 34.0 / total as f32;
    let r1 = clearance.max(0.0) as f32 * scale;
    let r2 = r1 + growth.max(0.0) as f32 * scale;
    let r3 = r2 + buffer.max(0.0) as f32 * scale;
    p.circle_stroke(c, r1, egui::Stroke::new(1.5, ink.key));
    if r2 > r1 + 0.5 {
        p.circle_stroke(c, r2, egui::Stroke::new(1.0, ink.line));
    }
    if r3 > r2 + 0.5 {
        p.circle_stroke(c, r3, egui::Stroke::new(1.0, ink.faint));
    }
    p.circle_filled(c, 3.0, ink.wire);
    // A tree point inside the outer ring.
    let q = c + egui::vec2(r3 * 0.55, r3 * 0.6);
    p.circle_filled(q, 2.5, ink.line);
    let x = r.right() - 34.0;
    text(&p, egui::pos2(x, r.top() + 12.0), "c", ink.key);
    text(&p, egui::pos2(x, r.top() + 26.0), "+ g", ink.line);
    text(&p, egui::pos2(x, r.top() + 40.0), "+ b", ink.faint);
}

/// The wind positions of a span seen along it: still, blown left and
/// right; the swing arc between them; the steps of a stepped wind.
pub fn wind_positions(ui: &mut egui::Ui, arc: bool, steps: usize) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: wind positions",
        "Seen along the span: the wire still, and blown to the left and to the right by the \
         case's wind; the swing arc is every position between; wind steps add the positions at \
         every pressure from 0 to the case's, from each side.",
    );
    let top = egui::pos2(r.center().x, r.top() + 10.0);
    let len = 54.0;
    let phi = 0.75_f32;
    let at = |a: f32| top + egui::vec2(a.sin(), a.cos()) * len;
    if arc {
        let pts: Vec<egui::Pos2> = (0..=24)
            .map(|i| at(-phi + 2.0 * phi * i as f32 / 24.0))
            .collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(2.0, ink.key)));
    }
    for a in [-phi, 0.0, phi] {
        p.line_segment([top, at(a)], egui::Stroke::new(1.0, ink.faint));
        p.circle_filled(at(a), 3.0, ink.wire);
    }
    let n = steps.min(12);
    for i in 1..n {
        let t = i as f32 / n as f32;
        for s in [-1.0, 1.0] {
            p.circle_stroke(at(s * phi * t), 2.5, egui::Stroke::new(1.0, ink.key));
        }
    }
    text(
        &p,
        egui::pos2(r.left() + 4.0, r.top() + 8.0),
        "left",
        ink.line,
    );
    text(
        &p,
        egui::pos2(r.right() - 26.0, r.top() + 8.0),
        "right",
        ink.line,
    );
}

/// A tree base: the ground under a vegetation point is taken from the
/// ground surface only with a ground point within the distance in plan.
pub fn tree_base(ui: &mut egui::Ui, limit: Option<f64>) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: tree base",
        "The tree's base is the ground below its top, taken only when a ground point (a TIN \
         vertex: LiDAR ground or survey) lies within this distance in plan; else the point gets \
         no fall-in check.",
    );
    let ground = r.bottom() - 16.0;
    let tree = egui::pos2(r.center().x, r.top() + 12.0);
    p.line_segment(
        [
            egui::pos2(r.left() + 4.0, ground),
            egui::pos2(r.right() - 4.0, ground),
        ],
        egui::Stroke::new(1.0, ink.faint),
    );
    let base = egui::pos2(tree.x, ground);
    dashed(&p, tree, base, ink.line);
    p.circle_filled(tree, 3.0, ink.line);
    let rad = limit.map_or(48.0, |d| (d.clamp(0.0, 20.0) as f32) * 5.0 + 4.0);
    p.line_segment(
        [base - egui::vec2(rad, 0.0), base + egui::vec2(rad, 0.0)],
        egui::Stroke::new(2.0, ink.key),
    );
    for x in [-50.0_f32, -14.0, 30.0] {
        let g = egui::pos2(base.x + x, ground);
        let near = x.abs() <= rad;
        p.circle_filled(g, 2.5, if near { ink.key } else { ink.faint });
    }
    text(
        &p,
        egui::pos2(r.left() + 4.0, r.bottom() - 6.0),
        if limit.is_some() {
            "nearest ground point"
        } else {
            "no limit"
        },
        ink.key,
    );
}

/// Unbalanced ice: one span iced more than its neighbours sags further.
pub fn unbalanced_ice(ui: &mut egui::Ui, on: bool) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: unbalanced ice",
        "Unbalanced ice: the span checked carries more ice than its neighbours (the case's \
         ice on it alone, more radial ice, more ice load per wire, or a load at mid-span); it \
         sags further, its strings swing towards it.",
    );
    let y = r.top() + 16.0;
    let xs = [
        r.left() + 8.0,
        r.left() + 42.0,
        r.right() - 42.0,
        r.right() - 8.0,
    ];
    for x in xs {
        p.line_segment(
            [egui::pos2(x, y - 6.0), egui::pos2(x, r.bottom() - 6.0)],
            egui::Stroke::new(1.5, ink.faint),
        );
    }
    for (k, w) in xs.windows(2).enumerate() {
        let iced = on && k == 1;
        let sag = if iced { 34.0 } else { 16.0 };
        let pts: Vec<egui::Pos2> = (0..=16)
            .map(|i| {
                let t = i as f32 / 16.0;
                egui::pos2(w[0] + (w[1] - w[0]) * t, y + 4.0 * sag * t * (1.0 - t))
            })
            .collect();
        p.add(egui::Shape::line(
            pts,
            egui::Stroke::new(
                if iced { 3.0 } else { 1.5 },
                if iced { ink.key } else { ink.wire },
            ),
        ));
    }
}

/// The vertical zone, seen along the line: the wire, the zone `width`
/// on each side straight below it, a point in it and its vertical
/// distance; `depth` written under it (the slice along the line).
pub fn vertical_zone(ui: &mut egui::Ui, width: f64, depth: f64) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: vertical zone",
        "Seen along the line: points within the width on each side of the wire (in plan) count; \
         the clearance is the height of the wire's lowest point over them (its highest for a \
         point above). Along the line the wire is taken at its lowest over the depth around each \
         point.",
    );
    let wire = egui::pos2(r.center().x, r.top() + 16.0);
    let ground = r.bottom() - 12.0;
    let w = (width.clamp(0.0, 10.0) as f32) * 5.0 + 3.0;
    p.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(wire.x - w, wire.y + 4.0),
            egui::pos2(wire.x + w, ground),
        ),
        0.0,
        ink.faint.gamma_multiply(0.25),
    );
    dashed(
        &p,
        egui::pos2(wire.x - w, wire.y),
        egui::pos2(wire.x - w, ground),
        ink.key,
    );
    dashed(
        &p,
        egui::pos2(wire.x + w, wire.y),
        egui::pos2(wire.x + w, ground),
        ink.key,
    );
    p.line_segment(
        [
            egui::pos2(r.left() + 4.0, ground),
            egui::pos2(r.right() - 4.0, ground),
        ],
        egui::Stroke::new(1.0, ink.faint),
    );
    p.circle_filled(wire, 3.0, ink.wire);
    let q = egui::pos2(wire.x + w * 0.5, ground - 14.0);
    p.circle_filled(q, 2.5, ink.line);
    p.line_segment(
        [egui::pos2(q.x, wire.y), q],
        egui::Stroke::new(1.0, ink.key),
    );
    text(
        &p,
        egui::pos2(wire.x + w + 3.0, wire.y + 10.0),
        "width",
        ink.key,
    );
    text(
        &p,
        egui::pos2(r.left() + 4.0, r.bottom() - 5.0),
        &format!("depth {depth:.2} m"),
        ink.line,
    );
}

/// The radial distance: the wire seen along the line, a point and the
/// circle of its distance.
pub fn radial_zone(ui: &mut egui::Ui) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: radial distance",
        "Seen along the line: the shortest 3D distance from the wire's surface (each subconductor \
         of a bundle) to a point or the ground, in any direction.",
    );
    let wire = egui::pos2(r.center().x - 10.0, r.center().y - 6.0);
    let q = wire + egui::vec2(26.0, 20.0);
    let d = (q - wire).length();
    p.circle_stroke(wire, d, egui::Stroke::new(1.0, ink.faint));
    p.circle_filled(wire, 3.0, ink.wire);
    p.circle_filled(q, 2.5, ink.line);
    p.line_segment([wire, q], egui::Stroke::new(1.5, ink.key));
    text(&p, wire + egui::vec2(8.0, 2.0), "d", ink.key);
}

/// The safety distance, seen along the wire: the conductor, the distance
/// d around its surface and the margin beyond it (dashed).
pub fn safety_distance(ui: &mut egui::Ui, distance: f64, margin: f64) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: safety distance",
        "Seen along the wire: the volume reaches the distance d from the conductor's surface (each \
         subconductor of a bundle, iced in a case with ice), then the margin, in every direction \
         and in every wire position.",
    );
    let c = r.center() + egui::vec2(-18.0, 0.0);
    let total = (distance + margin).max(1e-6);
    let scale = 32.0 / total as f32;
    let conductor = 3.0;
    let r1 = conductor + distance.max(0.0) as f32 * scale;
    let r2 = r1 + margin.max(0.0) as f32 * scale;
    p.circle_filled(c, r1, ink.key.gamma_multiply(0.12));
    p.circle_stroke(c, r1, egui::Stroke::new(1.5, ink.key));
    if r2 > r1 + 0.5 {
        p.extend(egui::Shape::dashed_line(
            &(0..=40)
                .map(|i| {
                    let a = i as f32 / 40.0 * std::f32::consts::TAU;
                    c + egui::vec2(a.cos(), a.sin()) * r2
                })
                .collect::<Vec<_>>(),
            egui::Stroke::new(1.0, ink.line),
            3.0,
            2.0,
        ));
    }
    p.circle_filled(c, conductor, ink.wire);
    p.line_segment(
        [c + egui::vec2(conductor, 0.0), c + egui::vec2(r1, 0.0)],
        egui::Stroke::new(1.0, ink.key),
    );
    let x = r.right() - 40.0;
    text(&p, egui::pos2(x, r.top() + 14.0), "d", ink.key);
    text(&p, egui::pos2(x, r.top() + 28.0), "+ margin", ink.line);
}

/// One volume per wire, per circuit or per system: two circuits of
/// three wires (the first two of one system), and the outlines a volume
/// takes.
pub fn volume_grouping(ui: &mut egui::Ui, grouping: ohl_clearance::safety_volume::Grouping) {
    use ohl_clearance::safety_volume::Grouping;
    let (p, r, ink) = canvas(
        ui,
        "drawing: grouping",
        "Seen along the line, two systems: per wire each conductor has a volume of its own; per \
         circuit the phases of a circuit are one volume; per system every circuit of one \
         electrical system is one volume.",
    );
    let y0 = r.top() + 22.0;
    let dx = 14.0;
    let left = [
        egui::pos2(r.left() + 18.0, y0),
        egui::pos2(r.left() + 18.0, y0 + dx * 1.6),
        egui::pos2(r.left() + 18.0, y0 + dx * 3.2),
    ];
    let mid = left.map(|q| q + egui::vec2(28.0, 0.0));
    let right = left.map(|q| q + egui::vec2(78.0, 0.0));
    let blob = |pts: &[egui::Pos2]| {
        let min = pts
            .iter()
            .fold(egui::pos2(f32::MAX, f32::MAX), |a, b| a.min(*b));
        let max = pts
            .iter()
            .fold(egui::pos2(f32::MIN, f32::MIN), |a, b| a.max(*b));
        p.rect_stroke(
            egui::Rect::from_min_max(min, max).expand(6.0),
            6.0,
            egui::Stroke::new(1.2, ink.key),
            egui::StrokeKind::Middle,
        );
    };
    match grouping {
        Grouping::Wire => {
            for q in left.iter().chain(&mid).chain(&right) {
                blob(&[*q]);
            }
        }
        Grouping::Circuit => {
            blob(&left);
            blob(&mid);
            blob(&right);
        }
        Grouping::System => {
            let both: Vec<egui::Pos2> = left.iter().chain(&mid).copied().collect();
            blob(&both);
            blob(&right);
        }
    }
    for q in left.iter().chain(&mid).chain(&right) {
        p.circle_filled(*q, 3.0, ink.wire);
    }
    text(
        &p,
        egui::pos2(r.left() + 6.0, r.top() + 8.0),
        "system A",
        ink.line,
    );
    text(
        &p,
        egui::pos2(r.left() + 84.0, r.top() + 8.0),
        "B",
        ink.line,
    );
}

/// The mesh and the exact volume: a smooth part of the surface (an arc),
/// the triangles' sides outside it by at most the tolerance; where two
/// tubes meet, the sides bridge the groove.
pub fn mesh_outside(ui: &mut egui::Ui, tolerance: f64) {
    let (p, r, ink) = canvas(
        ui,
        "drawing: mesh outside",
        "The exact volume (solid) and the mesh (its triangles' sides, dashed): the mesh holds the \
         exact volume whole, never inside it, and lies outside it by at most the tolerance where \
         the volume is smooth. Where two tubes meet, the triangles bridge the groove between them \
         (up to one grid cell farther out).",
    );
    let ground = r.bottom() - 18.0;
    // Two tubes meeting: two circles' upper halves, joined at a groove.
    let ra = 24.0_f32;
    let ca = egui::pos2(r.left() + 34.0, ground);
    let cb = egui::pos2(r.left() + 70.0, ground);
    let arc = |c: egui::Pos2, from: f32, to: f32| -> Vec<egui::Pos2> {
        (0..=20)
            .map(|i| {
                let a = from + (to - from) * i as f32 / 20.0;
                c + egui::vec2(a.cos(), -a.sin()) * ra
            })
            .collect()
    };
    // Where the circles cross: x half-way, y from the radius.
    let half = (cb.x - ca.x) / 2.0;
    let meet = (ra * ra - half * half).max(0.0).sqrt();
    let a_meet = meet.atan2(half);
    let mut exact = arc(ca, std::f32::consts::PI, a_meet);
    exact.extend(arc(cb, std::f32::consts::PI - a_meet, 0.0));
    p.add(egui::Shape::line(exact, egui::Stroke::new(1.5, ink.wire)));
    // The mesh: chords pushed out by the tolerance, a side across the
    // groove.
    let out = 2.0 + (tolerance.clamp(0.0, 1.0) as f32) * 6.0;
    let mesh: Vec<egui::Pos2> = [
        (ca, std::f32::consts::PI),
        (ca, 2.4),
        (ca, 1.7),
        (cb, 1.4),
        (cb, 0.7),
        (cb, 0.0),
    ]
    .iter()
    .map(|(c, a)| *c + egui::vec2(a.cos(), -a.sin()) * (ra + out))
    .collect();
    p.extend(egui::Shape::dashed_line(
        &mesh,
        egui::Stroke::new(1.0, ink.key),
        3.0,
        2.0,
    ));
    text(
        &p,
        egui::pos2(r.left() + 4.0, r.bottom() - 8.0),
        &format!("outside by ≤ {tolerance:.2} m"),
        ink.key,
    );
    text(
        &p,
        egui::pos2((ca.x + cb.x) / 2.0 - 8.0, ground - meet - 14.0),
        "groove",
        ink.line,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Every drawing renders, in the dark and the light theme, at the
    /// values' ends (k = 0 and 0.5, no allowance, no limit, steps off).
    #[test]
    fn the_drawings_render_in_both_themes() {
        for dark in [true, false] {
            for (k, a, steps, limit) in [(0.0, 0.0, 0, None), (0.5, 20.0, 40, Some(5.0))] {
                let mut h = Harness::new_ui(move |ui| {
                    ui.ctx().set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                    falling_tree(ui, k, a);
                    grow_in(ui, 5.0, 1.0, 0.5);
                    wind_positions(ui, steps > 0, steps);
                    tree_base(ui, limit);
                    unbalanced_ice(ui, steps > 0);
                    vertical_zone(ui, a / 2.0, k);
                    radial_zone(ui);
                    safety_distance(ui, a / 4.0, k);
                    for g in ohl_clearance::safety_volume::Grouping::ALL {
                        volume_grouping(ui, g);
                    }
                    mesh_outside(ui, k);
                });
                h.run();
                for name in [
                    "drawing: falling tree",
                    "drawing: grow-in",
                    "drawing: wind positions",
                    "drawing: tree base",
                    "drawing: unbalanced ice",
                    "drawing: vertical zone",
                    "drawing: radial distance",
                    "drawing: safety distance",
                    "drawing: mesh outside",
                ] {
                    h.get_by_label(name);
                }
                assert_eq!(h.query_all_by_label("drawing: grouping").count(), 3);
            }
        }
    }

    /// The solves a choice needs: steps of 10 Pa to 650 Pa from both
    /// sides are 1 + 64 × 2 + 2 = 131 (the arc suggested); the arc alone
    /// 3; the case's pressure alone 2 (one side: 1).
    #[test]
    fn solves_per_windy_case() {
        let mut w = WindSteps {
            stepped: true,
            step: 10.0,
            ..WindSteps::default()
        };
        assert_eq!(solves_per_case(&w, 650.0, true), 131);
        assert!(solves_per_case(&w, 650.0, true) > MANY_SOLVES);
        w.stepped = false;
        assert_eq!(solves_per_case(&w, 650.0, true), 2);
        w.arc = true;
        assert_eq!(solves_per_case(&w, 650.0, true), 3);
        w.sides = WindSides::Left;
        assert_eq!(solves_per_case(&w, 650.0, true), 2);
        w.arc = false;
        assert_eq!(solves_per_case(&w, 650.0, true), 1);
        assert_eq!(solves_per_case(&w, 0.0, true), 1);
    }

    /// The band's margin in the shared wind choice (2026-10-21): shown
    /// with the swing arc only, with its explanation (conservative); a
    /// smaller value than the default is warned about, and the default
    /// is a click away.
    #[test]
    fn the_band_margin_with_the_swing_arc() {
        for (arc, margin) in [(false, 0.15), (true, 0.15), (true, 0.05)] {
            let mut h = Harness::new_ui(move |ui| {
                let mut w = WindSteps {
                    arc,
                    band_margin: margin,
                    ..WindSteps::default()
                };
                wind_choice(ui, "test", &mut w, true, 600.0, true, true);
            });
            h.run();
            assert_eq!(h.query_by_label("band margin").is_some(), arc);
            assert_eq!(
                h.query_all_by_label_contains("conservative")
                    .next()
                    .is_some(),
                arc && margin == 0.15
            );
            assert_eq!(
                h.query_all_by_label_contains("less safe, on your responsibility")
                    .next()
                    .is_some(),
                margin < 0.15
            );
            assert_eq!(h.query_by_label("0.15 m").is_some(), margin < 0.15);
            // The jumpers' own margin beside it (2026-10-21), default 0:
            // said not to hold the jumper between its blowouts.
            assert_eq!(h.query_by_label("jumper band margin").is_some(), arc);
            assert_eq!(
                h.query_all_by_label_contains("does not hold the jumper between its blowouts")
                    .next()
                    .is_some(),
                arc
            );
        }
    }

    /// The jumpers' band margin (2026-10-21, the owner's decision): a
    /// value entered is said to be the user's, and the default (0 m) is a
    /// click away; in a check without jumpers it is greyed.
    #[test]
    fn the_jumper_band_margin_is_the_users() {
        for (margin, jumpers) in [(0.35, true), (0.35, false)] {
            let mut h = Harness::new_ui(move |ui| {
                let mut w = WindSteps {
                    arc: true,
                    jumper_band_margin: margin,
                    ..WindSteps::default()
                };
                wind_choice(ui, "test", &mut w, false, 600.0, jumpers, true);
            });
            h.run();
            assert!(h.query_by_label("jumper band margin").is_some());
            assert_eq!(
                h.query_all_by_label_contains("your value, on your responsibility")
                    .next()
                    .is_some(),
                jumpers
            );
            assert!(h.query_by_label("0 m").is_some());
        }
    }
}
