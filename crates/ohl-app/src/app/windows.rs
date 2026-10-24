// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Keys and small windows: Cross-section, Go to, the edit keys, First
//! steps, the tool windows, the Terrain menu, Help › Controls.

use super::*;

/// C opens the Cross-section window (not while typing; ⌘C / Ctrl+C is
/// not taken).
pub(super) fn cut_key(ctx: &egui::Context, r: &mut Ready) {
    if !ctx.text_edit_focused()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::C))
    {
        r.open_cut_dialog();
    }
}

/// G opens "Go to structure" (not while typing).
pub(super) fn goto_key(ctx: &egui::Context, r: &mut Ready) {
    let open_key = !ctx.text_edit_focused() && ctx.input(|i| i.key_pressed(egui::Key::G));
    if open_key && r.goto.is_none() {
        let current = match &r.selection {
            Selection::Structure(id) => Some(id.clone()),
            _ => None,
        };
        r.goto = current.or_else(|| r.model.line.structures.first().map(|s| s.id.clone()));
    }
}

/// When a tool window opens (Go to, Move, a deletion's confirmation): it
/// opens at the crosshair in first person (else at the mouse), and a
/// captured mouse is freed — a window needs the cursor.
pub(super) fn popups(ctx: &egui::Context, r: &mut Ready) {
    // A menu opened: what was going on stops, as with Escape — the mouse
    // is freed, a wire being strung and an alignment being laid out are
    // given up — so the menu's command works on the first click.
    let menu = egui::Popup::is_any_open(ctx);
    if menu && !r.menu_open {
        log::info!("a menu opened; mouse captured: {}", r.view3d.captured);
        if r.view3d.captured {
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
        r.xsections.free_mouse(ctx);
        if !r.chain.is_empty() {
            r.cancel_string_wire();
        }
        if !r.new_alignment.is_empty() {
            r.cancel_new_alignment();
        }
    }
    r.menu_open = menu;
    if std::mem::take(&mut r.release_mouse) {
        if r.view3d.captured {
            log::info!("a page opened from the 3D view: the mouse is freed");
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
        r.xsections.free_mouse(ctx);
    }
    r.popup_fresh = r.popup_opened == Some(ctx.cumulative_frame_nr());
    let open = r.goto.is_some()
        || r.coord_dialog.is_some()
        || r.route_choice.is_some()
        || r.move_dialog.is_some()
        || r.point_delete.is_some()
        || r.spread_delete.is_some()
        || r.confirm.is_some()
        || r.replace.is_some()
        || r.editor.is_some()
        || r.display_window.is_some()
        || r.lines_window.is_some();
    if open && !r.popup_open {
        let crosshair = r.view3d.rect.map(|x| x.center());
        let at = if r.view3d.captured {
            crosshair
        } else {
            ctx.pointer_latest_pos().or(crosshair)
        };
        // Kept on the screen (at least its top-left part).
        let screen = ctx.content_rect();
        let at = at.unwrap_or(egui::pos2(400.0, 200.0));
        r.popup_at = egui::pos2(
            at.x.clamp(screen.min.x, (screen.max.x - 320.0).max(screen.min.x)),
            at.y.clamp(screen.min.y, (screen.max.y - 240.0).max(screen.min.y)),
        );
        r.popup_serial += 1;
        r.popup_opened = Some(ctx.cumulative_frame_nr());
        r.popup_fresh = true;
        log::info!("a window opens at {:?}", r.popup_at);
        if r.view3d.captured {
            crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
        }
        r.xsections.free_mouse(ctx);
    }
    r.popup_open = open;
}

/// A tool window: opens next to [`Ready::popup_at`] (again each time it
/// opens, whatever a window of its id was in an earlier run: window
/// places are remembered between runs), can be dragged anywhere, also
/// partly off the screen.
pub(super) fn tool_window(
    r: &Ready,
    key: &str,
    title: impl Into<egui::WidgetText>,
) -> egui::Window<'static> {
    let at = r.popup_at + egui::vec2(16.0, 16.0);
    let w = egui::Window::new(title).id(egui::Id::new((key.to_owned(), r.popup_serial)));
    // Fixed in the frame it opens: egui (0.36) puts a movable window back
    // where it was remembered, over `current_pos`.
    let w = if r.popup_fresh {
        w.fixed_pos(at)
    } else {
        w.default_pos(at)
    };
    w.collapsible(false).resizable(false).constrain(false)
}

/// The Cross-section window: Open cut (or Enter) opens a cut in a tab of
/// its own, Escape closes. A structure clicked in a view while it is open
/// is taken.
pub(super) fn cut_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut d) = r.xsections.dialog.take() else {
        return;
    };
    d.follow(&r.selection);
    let views = r.xsections.alignments(&r.model.line);
    let mut out = crate::xsection::DialogOut::None;
    tool_window(r, "xsection", "Cross-section").show(ctx, |ui| {
        out = crate::xsection::dialog_ui(ui, &mut d, &r.model.line, &views);
    });
    if out == crate::xsection::DialogOut::None && !ctx.text_edit_focused() {
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            out = crate::xsection::DialogOut::Open;
        } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            out = crate::xsection::DialogOut::Close;
        }
    }
    match out {
        crate::xsection::DialogOut::Open => match d.spec(&views) {
            Some(spec) => r.open_cut(spec),
            None => r.xsections.dialog = Some(d),
        },
        crate::xsection::DialogOut::Close => {}
        crate::xsection::DialogOut::None => r.xsections.dialog = Some(d),
    }
}

/// The Go to window: Enter goes, Escape closes.
pub(super) fn goto_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut chosen) = r.goto.clone() else {
        return;
    };
    let mut go = false;
    let mut close = false;
    tool_window(r, "goto", "Go to structure").show(ctx, |ui| {
        egui::ComboBox::from_id_salt("goto-structure")
            .selected_text(&chosen)
            .width(220.0)
            .height(400.0)
            .show_ui(ui, |ui| {
                for s in &r.model.line.structures {
                    let text = match &s.structure_type {
                        Some(t) => format!("{}  ({t})", s.id),
                        None => s.id.clone(),
                    };
                    ui.selectable_value(&mut chosen, s.id.clone(), text);
                }
            });
        ui.horizontal(|ui| {
            go |= ui.button("Go").clicked();
            close |= ui.button("Close").clicked();
        });
    });
    go |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if go {
        r.go_to_structure(&chosen);
        r.goto = None;
    } else if close {
        r.goto = None;
    } else {
        r.goto = Some(chosen);
    }
}

/// Shortcuts (not while typing in a text field): ⌘Z undo, Shift+⌘Z redo,
/// Delete or Backspace deletes the selected structure, M holds the Move
/// tool, E opens the inventory, 1–9 choose a hotbar slot; the wheel scrolls
/// the hotbar while the mouse is captured.
pub(super) fn edit_keys(ctx: &egui::Context, r: &mut Ready) {
    use crate::tools::Tool;
    // The wire being strung: Enter ends it, Backspace takes the last point
    // back. Escape (mouse captured or not, a field focused) cancels it: the
    // points go; the tool stays held and the view keeps the mouse (the key
    // is taken: a second Escape frees the mouse). (The section window
    // opened by Enter: [`editor_window`].)
    // A handle being dragged in the 3D view: Escape gives the drag up.
    let cut_drag = r
        .xsections
        .views
        .values()
        .any(|v| v.view.handles.drag.is_some());
    if (r.view3d.handles.drag.is_some() || cut_drag)
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.view3d.handles.cancel();
        for v in r.xsections.views.values_mut() {
            v.view.handles.cancel();
        }
        r.status = Some(("drag cancelled: nothing changed".into(), false));
    }
    let windows = r.editor.is_some() || r.goto.is_some() || r.move_dialog.is_some();
    if !r.chain.is_empty()
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.cancel_string_wire();
    }
    // A jumper being laid out (Add jumper): Escape gives it up.
    if (r.jumper_from.is_some() || r.jumper_tap.is_some())
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.jumper_from = None;
        r.jumper_tap = None;
        r.jumper_via.clear();
        r.status = Some(("Add jumper: cancelled".into(), false));
    }
    // An attachment point chosen with the Link tool: Escape gives it up.
    if r.link_from.is_some()
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.link_from = None;
        r.status = Some(("Link: cancelled".into(), false));
    }
    // A spacer being laid out (Add string): Escape gives it up.
    if r.spacer_from.is_some()
        && !windows
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        r.spacer_from = None;
        r.status = Some(("Add spacer: cancelled".into(), false));
    }
    // A new alignment being laid out: the same keys.
    if !r.new_alignment.is_empty() && !windows {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            r.cancel_new_alignment();
        } else if !ctx.text_edit_focused() {
            let (enter, back) = ctx.input_mut(|i| {
                use egui::{Key, Modifiers};
                (
                    i.consume_key(Modifiers::NONE, Key::Enter),
                    i.consume_key(Modifiers::NONE, Key::Backspace),
                )
            });
            if enter && r.reroute {
                r.finish_reroute(None);
            } else if enter {
                r.finish_new_alignment();
            } else if back {
                r.new_alignment.pop();
                if r.new_alignment.is_empty() {
                    r.reroute = false;
                }
                r.alignment_drafted();
            }
        }
    }
    if !r.chain.is_empty() && !ctx.text_edit_focused() && !windows {
        let (enter, back) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            (
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Backspace),
            )
        });
        if enter {
            r.finish_chain();
        } else if back {
            r.chain.pop();
            r.update_draft();
        }
    }
    if !ctx.text_edit_focused() {
        // Redo first: Shift+⌘Z also matches ⌘Z when shift is ignored.
        let (save, redo, undo, delete, move_key, inventory, slot) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            let save = i.consume_key(Modifiers::COMMAND, Key::S);
            let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
            let undo = !redo && i.consume_key(Modifiers::COMMAND, Key::Z);
            let digits = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            (
                save,
                redo,
                undo,
                i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace),
                i.consume_key(Modifiers::NONE, Key::M),
                i.consume_key(Modifiers::NONE, Key::E),
                digits
                    .iter()
                    .position(|k| i.consume_key(Modifiers::NONE, *k)),
            )
        });
        let editable = r.model.can_edit().is_ok();
        if save {
            r.save_all();
        }
        if undo && editable {
            r.perform(crate::edit::Action::Undo);
        }
        if redo && editable {
            r.perform(crate::edit::Action::Redo);
        }
        if move_key {
            r.hotbar.hold(Tool::Move);
        }
        if r.inventory && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            r.inventory = false;
        }
        if inventory {
            r.inventory = !r.inventory;
            if r.inventory && r.view3d.captured {
                crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
            }
            if r.inventory {
                r.xsections.free_mouse(ctx);
            }
        }
        if let Some(k) = slot {
            r.hotbar.active = k;
        }
        if delete && editable {
            let target = r.selection.clone();
            r.delete_object(&target);
        }
    }
    // Captured mouse: the wheel (without Ctrl) scrolls the hotbar, wheel
    // down to the next slot.
    if r.view3d.captured {
        let (lines, points, now) = ctx.input(|i| {
            let (mut lines, mut points) = (0.0f32, 0.0f32);
            for e in &i.events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = e
                    && !modifiers.ctrl
                {
                    log::debug!("hotbar wheel: {unit:?} {delta:?}");
                    match unit {
                        egui::MouseWheelUnit::Point => points += delta.y,
                        _ => lines += delta.y.signum(),
                    }
                }
            }
            (lines, points, i.time)
        });
        if let Some(step) = r.wheel.step(lines, points, now) {
            r.hotbar.scroll(step);
        }
    }
}

/// The tool windows: hotbar, inventory, Go to, Move structure, the
/// editor, the confirmation of a deletion.
/// A new project's first steps: 1. its route, 2. its alignment (the New
/// alignment tool held, with a hint).
pub(super) fn first_steps_window(ctx: &egui::Context, r: &mut Ready) {
    let Some((name, description)) = &mut r.first_steps else {
        return;
    };
    let mut create = false;
    let mut later = false;
    egui::Window::new("New project: first steps")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new("1. The route").strong());
            ui.label(
                egui::RichText::new(
                    "A route is the path of the line: it is made of alignments, and the \
                     structures, wires and checks belong to it. Name it:",
                )
                .small(),
            );
            egui::Grid::new("first-route")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("name");
                    ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                    ui.end_row();
                    ui.label("description");
                    ui.add(
                        egui::TextEdit::singleline(description)
                            .desired_width(200.0)
                            .hint_text("what it is (free text)"),
                    );
                    ui.end_row();
                });
            ui.label(egui::RichText::new("2. Its alignment").strong());
            ui.label(
                egui::RichText::new(
                    "Then draw its alignment: the New alignment tool is held — right-click its \
                     points in order (a survey point, or coordinates typed), Enter to make it.",
                )
                .small(),
            );
            ui.horizontal(|ui| {
                create = ui
                    .add_enabled(
                        !name.trim().is_empty(),
                        egui::Button::new("Create the route"),
                    )
                    .clicked();
                later = ui.button("Later").clicked();
            });
        });
    if later {
        r.first_steps = None;
    }
    if create && let Some((name, description)) = r.first_steps.clone() {
        let stored = r.model.stored_line().unwrap_or(&r.model.line).clone();
        match crate::editors::new_route(&stored, &name, &name) {
            Ok((what, mut command)) => {
                if let ohl_session::Command::Put { value, .. } = &mut command {
                    value["description"] = description.trim().into();
                }
                if r.perform(crate::edit::Action::Command { what, command }) {
                    r.first_steps = None;
                    r.hotbar.hold(crate::tools::Tool::NewAlignment);
                    r.status = Some((
                        format!(
                            "Route {} made. Step 2, its alignment: right-click its points in \
                             order, Enter to make it",
                            name.trim()
                        ),
                        false,
                    ));
                }
            }
            Err(e) => r.status = Some((e, true)),
        }
    }
}

/// The alignment tools' windows: a point's coordinates, a new alignment's
/// routes.
pub(super) fn alignment_windows(ctx: &egui::Context, r: &mut Ready) {
    if let Some(d) = &mut r.coord_dialog
        && let Some(done) = d.ui(ctx)
    {
        let (purpose, connect, structure) = (d.purpose, d.connect, d.structure.clone());
        r.coord_dialog = None;
        if let Ok(p) = done {
            let point = ohl_line::AlignmentPoint {
                structure,
                ..ohl_line::AlignmentPoint::free("", p)
            };
            if purpose == crate::alignment_tools::CoordFor::AddAlignment && !connect {
                r.add_free_point(point);
            } else {
                r.place_alignment_point(purpose, point);
            }
        }
    }
    // (The line copied only while the window is open: every frame.)
    let Some(stored) = r
        .route_choice
        .is_some()
        .then(|| r.model.stored_line().unwrap_or(&r.model.line).clone())
    else {
        return;
    };
    if let Some(c) = &mut r.route_choice
        && let Some(action) = c.ui(ctx, &stored)
    {
        use crate::alignment_tools::RouteChoiceAction;
        match action {
            RouteChoiceAction::Make { route, new } => r.make_new_alignment(route, new),
            RouteChoiceAction::Back => {
                if let Some(c) = r.route_choice.take() {
                    r.new_alignment = c.points;
                    r.alignment_drafted();
                }
            }
        }
    }
}

/// The pylon builder.
pub(super) fn pylon_windows(ctx: &egui::Context, r: &mut Ready) {
    if let Some(mut b) = r.pylon_builder.take() {
        let mut open = true;
        if let Some(crate::pylon_builder::BuilderAction::Save {
            id,
            text,
            replace,
            notes,
        }) = b.ui(ctx, &r.model, &mut open)
        {
            let again = replace.is_some();
            b.message = Some(match r.save_built_type(&id, &text, replace) {
                Ok(p) => {
                    // Built again: the structures of that type follow.
                    if again {
                        r.refresh_views();
                    }
                    b.rebuild = Some(id.clone());
                    let mut m = format!("structure type {id} written: {}", p.display());
                    if !notes.is_empty() {
                        m.push_str("\nKept, to look at:");
                        for n in &notes {
                            m.push_str(&format!("\n- {n}"));
                        }
                    }
                    (m, !notes.is_empty())
                }
                Err(e) => (e, true),
            });
        }
        if open {
            r.pylon_builder = Some(b);
        }
    }
}

pub(super) fn tool_windows(
    ctx: &egui::Context,
    r: &mut Ready,
    rs: Option<&eframe::egui_wgpu::RenderState>,
) {
    // The first structures of an empty line: the plan fits them.
    let on_line = !r.model.line.structures.is_empty();
    if on_line && !r.plan_on_line {
        r.plan.refit();
    }
    r.plan_on_line = on_line;
    // At the bottom of the 3D view, only while it is shown (and while the
    // inventory arranges it).
    let shown = r.view3d.drawn;
    r.view3d.drawn = false;
    r.plan.drawn = None;
    // Else at the bottom of a cut's panel shown.
    let mut cut_rect = None;
    for v in r.xsections.views.values_mut() {
        if std::mem::take(&mut v.view.drawn) && cut_rect.is_none() {
            cut_rect = v.view.rect;
        }
    }
    if shown || cut_rect.is_some() || r.inventory {
        let at = match (shown, r.view3d.rect, cut_rect) {
            (true, Some(v), _) => v.center_bottom(),
            (false, _, Some(c)) => c.center_bottom(),
            _ => ctx.content_rect().center_bottom(),
        };
        crate::tools::hotbar_ui(
            ctx,
            &mut r.hotbar,
            at,
            r.inventory,
            r.view3d.controls.tools_on_left,
        );
    }
    crate::tools::inventory_ui(ctx, &mut r.inventory, &mut r.hotbar);
    goto_window(ctx, r);
    cut_window(ctx, r);
    move_window(ctx, r);
    point_delete_window(ctx, r);
    spread_delete_window(ctx, r);
    first_steps_window(ctx, r);
    alignment_windows(ctx, r);
    crate::replace::window(ctx, r, rs);
    editor_window(ctx, r, rs);
    crate::lidar_edit::frame(ctx, r);
    display_window(ctx, r);
    angles_window(ctx, r);
    survey_window(ctx, r);
    survey_io_windows(ctx, r);
    r.drawings.show(ctx, &mut r.model, r.case);
    if std::mem::take(&mut r.model.drawings_setting_changed) {
        r.drawings.load(&r.model);
    }
    r.drawings.save(&mut r.model, ctx);
    controls_window(ctx, r);
    creep_stretch_window(ctx, r);
    crate::merge_cases::window(ctx, r);
    bimetallic_window(ctx, r);
    solver_window(ctx, r);
    circuit_join_window(ctx, r);
    defaults_window(ctx, r);
    crs_windows(ctx, r);
    if r.backgrounds.show(ctx, &r.model) {
        r.reopen = true;
    }
    crate::project_lines::window(ctx, r);
    crate::working_tree::window(ctx, r);
    crate::thermal::window(ctx, &mut r.thermal, &r.model, r.clearances.rules.as_ref());
    r.bom.show(ctx, &r.model);
    r.spot.show(ctx, &r.model);
    pylon_windows(ctx, r);
    let Some((impact, confirmed)) = r.confirm.clone() else {
        return;
    };
    let mut decision: Option<bool> = None;
    tool_window(r, "confirm", format!("Delete {}?", impact.what())).show(ctx, |ui| {
        ui.label("This also deletes or changes:");
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (title, items) in impact.groups() {
                    if items.is_empty() {
                        continue;
                    }
                    ui.label(egui::RichText::new(format!("{title} ({})", items.len())).strong());
                    for it in items {
                        ui.label(egui::RichText::new(format!("  {it}")).small());
                    }
                }
            });
        ui.label("It is one log entry: undo brings everything back.");
        ui.horizontal(|ui| {
            if ui.button("Delete all").clicked() {
                decision = Some(true);
            }
            if ui.button("Cancel").clicked() {
                decision = Some(false);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        decision = Some(false);
    }
    match decision {
        Some(true) => {
            r.perform(confirmed);
        }
        Some(false) => r.confirm = None,
        None => {}
    }
}

/// The Terrain menu: the ground surface (TIN) and the LiDAR points, with
/// the classes drawn.
pub(super) fn terrain_menu(ui: &mut egui::Ui, r: &mut Ready, out: &mut Vec<crate::menu::Command>) {
    ui.label(egui::RichText::new("Routes and alignments").small().weak());
    for kind in [
        crate::tables::TableKind::Routes,
        crate::tables::TableKind::Alignments,
        crate::tables::TableKind::AlignmentPoints,
    ] {
        let c = crate::menu::Command::Table(kind);
        if c.menu_item(ui) {
            out.push(c);
        }
    }
    ui.separator();
    ui.label(egui::RichText::new("Ground surface").small().weak());
    if ui
        .button("Terrain surfaces (TIN)…")
        .on_hover_text(
            "The surfaces kept in the project: import, build from LiDAR, the active one (the \
             ground), others shown, export, delete",
        )
        .clicked()
    {
        r.surfaces.show(&r.model);
        ui.close();
    }
    ui.separator();
    crate::menu::options_menu(ui, "Backgrounds", |ui| crate::backgrounds::menu(ui, r));
    ui.menu_button("Coordinate system", |ui| {
        ui.label(
            egui::RichText::new(format!("Now: {}", ohl_crs::describe(&r.model.line.crs)))
                .small()
                .weak(),
        );
        ui.label(
            egui::RichText::new(format!(
                "Height datum: {}",
                crate::crs::describe_height_datum(&r.model)
            ))
            .small()
            .weak(),
        );
        if ui
            .button("Define…")
            .on_hover_text(
                "Which coordinate system the project's coordinates are in, and the vertical \
                 datum of its heights (nothing moves)",
            )
            .clicked()
        {
            r.crs.define = Some(crate::crs::Define::new(&r.model));
            ui.close();
        }
        if ui
            .button("Coordinate system calculator…")
            .on_hover_text("Converts points from one coordinate system to another")
            .clicked()
        {
            r.crs.calculator = Some(crate::crs::Calculator::new(&r.model));
            ui.close();
        }
        if ui
            .add_enabled(r.crs.job.is_none(), egui::Button::new("Reproject project…"))
            .on_hover_text(
                "Moves the whole project (lines, surfaces, point clouds) into another \
                 coordinate system, as a new project file",
            )
            .clicked()
        {
            r.crs.reproject = Some(crate::crs::Reproject::new());
            ui.close();
        }
    });
    ui.separator();
    ui.menu_button("Survey data", |ui| {
        if r.model.points.is_empty() {
            ui.add_enabled(false, egui::Button::new("LiDAR: no point cloud"));
            return;
        }
        if ui
            .button("Survey data display options…")
            .on_hover_text("The LiDAR classes shown: tick or untick as many as you like")
            .clicked()
        {
            if r.survey_window.is_none() {
                r.survey_window = Some(SurveyDraft::of(&r.model));
            }
            ui.close();
        }
        if ui
            .button("Class colours…")
            .on_hover_text(
                "The display colour of the points by class number, from a class scheme file \
                 (.ohlc)",
            )
            .clicked()
        {
            r.class_window.get_or_insert_with(Default::default);
            ui.close();
        }
    });
}

/// The views whose controls Help › Controls shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlsPage {
    /// The 3D view.
    View3d,
    /// The plan.
    Plan,
    /// The profile.
    Profile,
    /// A cross-section view.
    CrossSection,
}

impl ControlsPage {
    /// Every page, in the menu's order.
    pub const ALL: [ControlsPage; 4] = [
        ControlsPage::View3d,
        ControlsPage::Plan,
        ControlsPage::Profile,
        ControlsPage::CrossSection,
    ];

    /// Name in the menu.
    pub fn name(self) -> &'static str {
        match self {
            ControlsPage::View3d => "3D",
            ControlsPage::Plan => "Plan",
            ControlsPage::Profile => "Profile",
            ControlsPage::CrossSection => "Cross-section",
        }
    }
}

/// The option "Tools act on left-click" (Tools menu, Help › Controls):
/// kept with the 3D view's controls, saved with the preferences.
pub(super) fn tools_on_left_checkbox(ui: &mut egui::Ui, c: &mut crate::controls::Controls) {
    if ui
        .checkbox(&mut c.tools_on_left, "Tools act on left-click")
        .on_hover_text(
            "On: a left-click uses the tool held (Select held: it selects) and a \
             right-click selects. Off (default): a left-click selects, a right-click uses \
             the tool. Drags (orbit, look, pan) are not affected.",
        )
        .changed()
    {
        log::info!("controls: tools act on left-click {}", c.tools_on_left);
    }
}

/// A grid of input → action rows.
pub(super) fn controls_grid(ui: &mut egui::Ui, id: &str, rows: &[(String, String)]) {
    egui::Grid::new(id).striped(true).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(egui::RichText::new(k).strong());
            ui.label(v);
            ui.end_row();
        }
    });
}

/// Help › Controls › 3D / Plan / Profile: how the view is moved and
/// used (the 3D view: its navigation scheme, speed, look sensitivity).
pub(super) fn controls_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(page) = r.controls_window else {
        return;
    };
    let mut open = true;
    let row = |k: &str, v: &str| (k.to_owned(), v.to_owned());
    egui::Window::new(format!("Controls: {}", page.name()))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            tools_on_left_checkbox(ui, &mut r.view3d.controls);
            ui.separator();
            let clicks = crate::controls::click_rows(r.view3d.controls.tools_on_left);
            let select_double = if r.view3d.controls.tools_on_left {
                "right double-click"
            } else {
                "double-click"
            };
            match page {
                ControlsPage::View3d => {
                    let c = &mut r.view3d.controls;
                    ui.horizontal(|ui| {
                        ui.label("Navigation scheme");
                        egui::ComboBox::from_id_salt("controls")
                            .selected_text(c.name)
                            .show_ui(ui, |ui| {
                                for p in crate::controls::Controls::presets() {
                                    if ui.selectable_label(c.name == p.name, p.name).clicked() {
                                        log::info!("controls: {}", p.name);
                                        // Keep the chosen speed and click option.
                                        let (speed, left) = (c.speed, c.tools_on_left);
                                        *c = p;
                                        c.speed = speed;
                                        c.tools_on_left = left;
                                    }
                                }
                            });
                    });
                    ui.add(
                        egui::Slider::new(&mut c.speed, 1.0..=2000.0)
                            .text("speed, m/s")
                            .logarithmic(true),
                    );
                    if c.capture {
                        // Shown ×1000 (milliradians per pixel): a readable number.
                        let mut mrad = c.mouse_sensitivity * 1000.0;
                        if ui
                            .add(
                                egui::Slider::new(&mut mrad, 0.3..=10.0)
                                    .text("look sensitivity")
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            c.mouse_sensitivity = mrad / 1000.0;
                        }
                    }
                    ui.separator();
                    let mut rows = c.describe();
                    rows.push(row("1–9, wheel", "choose a tool of the hotbar"));
                    rows.push(row("E", "tool inventory"));
                    rows.push(row("C", "cross-section: a cut across the line (a window)"));
                    controls_grid(ui, "controls-3d", &rows);
                }
                ControlsPage::Plan => {
                    let mut rows = vec![
                        row("drag (any button)", "pan"),
                        row("scroll", "zoom at the pointer"),
                    ];
                    rows.extend(clicks);
                    rows.push(row(select_double, "select and centre on it"));
                    rows.push(row("C", "cross-section: a cut across the line (a window)"));
                    controls_grid(ui, "controls-plan", &rows);
                }
                ControlsPage::Profile => {
                    let mut rows = vec![
                        row("drag (any button)", "move along the route"),
                        row("scroll", "zoom along the route at the pointer"),
                    ];
                    rows.extend(clicks);
                    rows.push(row(select_double, "show the whole route"));
                    let select = if r.view3d.controls.tools_on_left {
                        "right"
                    } else {
                        "left"
                    };
                    rows.push((
                        format!("Alt + {select} drag"),
                        "box: the LiDAR points dotted in it, their class changed together".into(),
                    ));
                    rows.push((
                        format!("Alt + Shift + {select} drag"),
                        "lasso: the same in a free shape (Esc: give up)".into(),
                    ));
                    rows.push(row(
                        "right-click (Select held)",
                        "a menu: Cross-section here (at that station)",
                    ));
                    rows.push(row("C", "cross-section: a cut across the line (a window)"));
                    controls_grid(ui, "controls-profile", &rows);
                }
                ControlsPage::CrossSection => {
                    let mut rows = vec![row(
                        "drags, scroll, keys",
                        "as in the 3D view (its control scheme: the 3D page)",
                    )];
                    rows.extend(clicks);
                    rows.push(row(
                        "Home, Back to the cut view",
                        "the front view: looking ahead, orthographic, 1:1",
                    ));
                    rows.push(row(
                        "Orthographic / Perspective",
                        "the projection (it opens orthographic)",
                    ));
                    rows.push(row(
                        ", (comma)",
                        "move the cut one step back along the line",
                    ));
                    rows.push(row(". (full stop)", "move the cut one step ahead"));
                    rows.push(row("⏮ ⏭", "move the cut to the structure before / after"));
                    rows.push(row("C", "another cut (a window)"));
                    rows.push(row(
                        "click its outline in the 3D view",
                        "its panel to the front",
                    ));
                    controls_grid(ui, "controls-xsection", &rows);
                }
            }
        });
    if !open {
        r.controls_window = None;
    }
}
