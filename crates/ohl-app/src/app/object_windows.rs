// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Display, Angles, the object editor, the deletions' confirmations, Move.

use super::*;

/// The state of the "Display weather cases" window.
#[derive(Debug, Clone)]
pub struct DisplayWindow {
    /// The cases ticked (for every section).
    pub ticks: Vec<bool>,
    /// For every section, or each section's own (Sections table).
    pub mode: crate::model::CasesShown,
    /// The wind side shown.
    pub wind: crate::model::WindDisplay,
}

/// The "Display weather cases" window: the same cases for every section
/// (ticked here; the Sections table is not changed) or each section's own
/// display cases from the Sections table; and the side the wind comes
/// from. Apply (Enter) shows them; display only — this user's choice.
/// Line › Structures › Angles around structures: which angle means ahead. Angles
/// in plan around a structure turn clockwise seen from above.
pub(super) fn angles_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut ahead) = r.angles_window else {
        return;
    };
    let (mut apply, mut close) = (false, false);
    let mut open = true;
    egui::Window::new("Angles around structures")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                "Angles in plan around a structure — the direction of a string, the compass of \
                 the structure window — turn clockwise seen from above. Which angle means ahead \
                 along the line is yours to choose for the project:",
            );
            ui.horizontal(|ui| {
                ui.label("ahead is at");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut ahead)
                        .range(0.0..=359.0)
                        .speed(1.0)
                        .suffix("°"),
                );
                for a in [0.0, 90.0, 180.0, 270.0] {
                    if ui.small_button(format!("{a:.0}°")).clicked() {
                        ahead = a;
                    }
                }
            });
            let at = |q: f64| (ahead + q).rem_euclid(360.0);
            egui::Grid::new("angles-around").show(ui, |ui| {
                for (name, q, axis) in [
                    ("ahead", 0.0, "+y"),
                    ("right", 90.0, "+x"),
                    ("back", 180.0, "−y"),
                    ("left", 270.0, "−x"),
                ] {
                    ui.label(name);
                    ui.label(format!("{:.0}°", at(q)));
                    ui.label(egui::RichText::new(format!("structure axis {axis}")).weak());
                    ui.end_row();
                }
            });
            ui.label(
                egui::RichText::new(
                    "For the whole project (every user sees the same). The structure's axes stay: \
                     x to the right of the line, y ahead, z up.",
                )
                .small()
                .weak(),
            );
            ui.horizontal(|ui| {
                apply = ui.button("Apply").clicked();
                close = ui.button("Cancel").clicked();
            });
        });
    if apply {
        if let Some(stored) = r.model.stored_line() {
            let mut line = stored.clone();
            line.display.ahead_deg = ahead.rem_euclid(360.0);
            let command = ohl_session::line_settings_change(stored, &line);
            if r.perform(crate::edit::Action::Command {
                what: format!("ahead at {:.0}° around structures", line.display.ahead_deg),
                command,
            }) {
                r.angles_window = None;
                return;
            }
        } else {
            r.status = Some(("read-only: open a project to edit".into(), true));
        }
    }
    r.angles_window = (open && !close).then_some(ahead);
}

pub(super) fn display_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::model::{CasesShown, WindDisplay};
    let Some(mut w) = r.display_window.clone() else {
        return;
    };
    let (mut apply, mut cancel) = (false, false);
    let window = tool_window(r, "display-cases", "Display weather cases");
    let panel = crate::dockable::Panel::DisplayCases;
    let shown = crate::dockable::show(ctx, panel, panel.name(), window, None, |ui| {
        ui.radio_value(
            &mut w.mode,
            CasesShown::AllSections,
            "The same weather cases for every section",
        )
        .on_hover_text("Display only: the Sections table is not changed");
        ui.add_enabled_ui(w.mode == CasesShown::AllSections, |ui| {
            ui.indent("cases", |ui| {
                // A box of fixed height, scrolled: long lists of cases keep
                // the window small.
                crate::properties::cases_box(ui, "display-cases-list", |ui| {
                    for (k, c) in r.model.cases.iter().enumerate() {
                        if let Some(t) = w.ticks.get_mut(k) {
                            ui.checkbox(t, &c.case.name).on_hover_text(c.case.summary());
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        w.ticks.iter_mut().for_each(|t| *t = true);
                    }
                    if ui.small_button("None").clicked() {
                        w.ticks.iter_mut().for_each(|t| *t = false);
                    }
                });
            });
        });
        ui.radio_value(
            &mut w.mode,
            CasesShown::PerSection,
            "Each section's display weather case (Sections table)",
        )
        .on_hover_text(
            "Sections without one: the cases ticked above (the first one when none is ticked)",
        );
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Wind");
            egui::ComboBox::from_id_salt("wind-side")
                .selected_text(w.wind.name())
                .show_ui(ui, |ui| {
                    for side in WindDisplay::ALL {
                        ui.selectable_value(&mut w.wind, side, side.name());
                    }
                });
        })
        .response
        .on_hover_text(
            "Looking ahead along the line. Both: each case with wind drawn twice; the wind from \
             the right is the one reported, checked and clicked",
        );
        ui.label(
            egui::RichText::new(
                "Display only, yours: the first case is the one reported, checked and clicked.",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            apply |= ui.button("Apply").on_hover_text("Enter").clicked();
            cancel |= ui.button("Cancel").on_hover_text("Escape").clicked();
        });
    });
    cancel |= shown == crate::dockable::Shown::Closed;
    apply |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if apply {
        let mut cases: Vec<usize> = (0..w.ticks.len()).filter(|k| w.ticks[*k]).collect();
        if cases.is_empty() {
            cases.push(r.case.min(r.model.cases.len().saturating_sub(1)));
        }
        if w.wind != r.model.wind {
            r.model.set_wind(w.wind);
        }
        r.model.cases_shown = w.mode;
        r.show_cases(cases);
        r.display_window = None;
    } else if cancel {
        r.display_window = None;
    } else {
        r.display_window = Some(w);
    }
}

/// The editor window (structure, string, section): Apply (Enter) makes one
/// command and closes it — unless the session refuses it (the reason is
/// shown, the window stays); Cancel (Escape) closes it.
/// Where a new structure goes when no place is given: the first free
/// alignment point, else by the last structure, else the model's origin.
pub(super) fn structure_spot(model: &crate::model::Model) -> [f64; 3] {
    crate::editors::alignment_points(&model.line)
        .into_iter()
        .find(|p| p.3.is_none())
        .map(|p| p.2)
        .or_else(|| model.line.structures.last().map(|s| s.position))
        .unwrap_or(model.origin)
}

pub(super) fn editor_window(
    ctx: &egui::Context,
    r: &mut Ready,
    rs: Option<&eframe::egui_wgpu::RenderState>,
) {
    use crate::editors::{Form, Outcome};
    if !matches!(r.editor, Some(Form::Structure(_))) {
        r.structure_picker = None;
    }
    let Some(mut form) = r.editor.clone() else {
        return;
    };
    let mut outcome = None;
    let window = tool_window(r, "editor", form.title());
    let title = form.title();
    let shown = crate::dockable::show(
        ctx,
        crate::dockable::Panel::Editor,
        title,
        window,
        None,
        |ui| {
            let mut extra = crate::editors::FormExtra {
                rs,
                preview: &mut r.structure_preview,
                picker: &mut r.structure_picker,
                picker_preview: &mut r.picker_preview,
            };
            outcome = form.ui(ui, &r.model, r.case, &mut extra);
            if let Some(mates) = &r.circuit_warned {
                crate::menu::message(
                    ui,
                    egui::RichText::new(circuit_warning(mates)).small(),
                    Some(ui.visuals().warn_fg_color),
                );
            }
            if let Some((text, true)) = &r.status {
                crate::menu::error_label(ui, egui::RichText::new(text).small());
            }
        },
    );
    // Its tab closed (docked): as Cancel.
    if shown == crate::dockable::Shown::Closed {
        outcome = Some(Outcome::Cancel);
    }
    if outcome.is_none() {
        outcome = ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                Some(Outcome::Cancel)
            } else if i.key_pressed(egui::Key::Enter) {
                Some(Outcome::Apply)
            } else {
                None
            }
        });
    }
    if let Some(o) = &outcome {
        log::info!("window \"{}\": {o:?}", form.title());
    }
    let apply = |r: &mut Ready, form: Form, reset: bool| match form.command(&r.model, reset) {
        Ok((what, command)) => {
            if r.perform(crate::edit::Action::Command { what, command }) {
                r.editor = None;
                // Select what was made.
                match &form {
                    Form::Structure(f) if f.new => r.select(Selection::Structure(f.id.clone())),
                    Form::Section(f) if f.new => r.select(Selection::Span {
                        section: f.id.clone(),
                        span: 0,
                    }),
                    Form::String(f) if f.new => r.select(Selection::Attachment {
                        structure: f.structure.clone(),
                        set: f.wires.iter().find_map(|w| w.set).unwrap_or(f.set),
                    }),
                    // The point where it is now.
                    Form::Jumper(f) if f.new => r.select(Selection::Jumper(f.jumper.id.clone())),
                    Form::Points(f) => {
                        if let Some(p) = f.drawn_after(&r.model) {
                            r.select(Selection::LidarPoint(p));
                        }
                    }
                    _ => {}
                }
            } else {
                r.editor = Some(form);
            }
        }
        Err(e) => {
            r.status = Some((e, true));
            r.editor = Some(form);
        }
    };
    // A section whose circuit name, phase or system change takes the rest
    // of its circuit along: shown first, applied on a second Apply.
    let mates = match (&outcome, &form) {
        (Some(Outcome::Apply), Form::Section(f)) => r
            .model
            .stored_line()
            .map(|l| f.circuit_mates(l, &f.id))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if !mates.is_empty() && r.circuit_warned.as_ref() != Some(&mates) {
        r.circuit_warned = Some(mates);
        r.editor = Some(form);
        return;
    }
    if outcome.is_some() {
        r.circuit_warned = None;
    }
    // A new jumper joining two different circuits: the join window first
    // (as [`Ready::add_jumper`]).
    if let (Some(Outcome::Apply), Form::Jumper(f)) = (&outcome, &form)
        && f.new
        && let Some(join) = crate::circuit_join::CircuitJoin::check(
            &r.model,
            &f.jumper,
            &format!("added jumper {}", f.jumper.id),
            false,
        )
    {
        r.status = Some((
            format!(
                "Add jumper: {} joins two different circuits — choose in the window",
                f.jumper.id
            ),
            false,
        ));
        r.circuit_join = Some(join);
        r.editor = None;
        return;
    }
    match outcome {
        Some(Outcome::Apply) => apply(r, form, false),
        Some(Outcome::ResetString) => apply(r, form, true),
        Some(Outcome::PickPoints) => {
            r.editor = None;
            if let Form::Section(f) = &form
                && !f.new
            {
                r.restring = Some(f.id.clone());
            }
            r.chain.clear();
            r.hotbar.hold(crate::tools::Tool::StringWire);
            r.status = Some((
                "String wire: click the strings in line order, then Enter".into(),
                false,
            ));
        }
        Some(Outcome::NewPylon) => {
            r.pylon_builder = Some(crate::pylon_builder::PylonBuilder::new(&r.model));
            r.editor = Some(form);
        }
        Some(Outcome::Cancel) => {
            r.editor = None;
            r.status = None;
            // The section window of a wire being strung: that wire is
            // given up (String wire stays held).
            if matches!(form, Form::Section(_)) && r.hotbar.tool() == crate::tools::Tool::StringWire
            {
                r.cancel_string_wire();
            }
        }
        None => r.editor = Some(form),
    }
}

/// What a section's change also sets on its circuit's other sections.
pub(super) fn circuit_warning(mates: &[crate::editors::CircuitMate]) -> String {
    let list: Vec<String> = mates
        .iter()
        .map(|(id, what)| format!("{id} ({})", what.join(", ")))
        .collect();
    format!(
        "Also changed, the same circuit: {}. Apply again to change them all.",
        list.join(", ")
    )
}

/// The Delete tool's window for an alignment point on several alignments:
/// off all of them (the point deleted) or off the ones ticked; Escape
/// closes it.
pub(super) fn point_delete_window(ctx: &egui::Context, r: &mut Ready) {
    let Some((id, mut ticks)) = r.point_delete.clone() else {
        return;
    };
    // None: still open; Some(None): delete it; Some(Some(list)): off these.
    let mut chosen: Option<Option<Vec<String>>> = None;
    let mut close = false;
    let stored = r.model.stored_line().unwrap_or(&r.model.line).clone();
    tool_window(r, "point-delete", format!("Delete alignment point {id}")).show(ctx, |ui| {
        ui.label(format!(
            "{id} is on {} alignments. Delete it from all of them, or take it off the ones ticked \
             (it stays on the others; the points around keep their order).",
            ticks.len()
        ));
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                for (a, on) in &mut ticks {
                    let route = stored
                        .alignment_by_id(a)
                        .map_or(String::new(), |x| x.route.clone());
                    ui.checkbox(on, format!("{a} (route {route})"));
                }
            });
        ui.horizontal(|ui| {
            if ui.small_button("tick all").clicked() {
                ticks.iter_mut().for_each(|t| t.1 = true);
            }
            if ui.small_button("tick none").clicked() {
                ticks.iter_mut().for_each(|t| t.1 = false);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button("Delete from all")
                .on_hover_text("The point is deleted: off every alignment")
                .clicked()
            {
                chosen = Some(None);
            }
            let ticked: Vec<String> = ticks.iter().filter(|t| t.1).map(|t| t.0.clone()).collect();
            if ui
                .add_enabled(
                    !ticked.is_empty(),
                    egui::Button::new("Take it off the ticked ones"),
                )
                .on_hover_text("Off these alignments only (all ticked: the point is deleted)")
                .clicked()
            {
                chosen = Some((ticked.len() < ticks.len()).then_some(ticked));
            }
            close |= ui.button("Cancel").clicked();
        });
    });
    close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    match chosen {
        Some(None) => {
            r.point_delete = None;
            r.perform(crate::edit::Action::Delete {
                kind: ohl_session::Kind::AlignmentPoint,
                id,
                confirm: false,
            });
        }
        Some(Some(off)) => {
            r.point_delete = None;
            let mut alignments = stored.alignments.clone();
            for a in alignments.iter_mut().filter(|a| off.contains(&a.id)) {
                a.points.retain(|p| *p != id);
            }
            match crate::editors::alignments_command(&stored, &alignments) {
                Ok(command) => {
                    r.perform(crate::edit::Action::Command {
                        what: format!("took alignment point {id} off {}", off.join(", ")),
                        command,
                    });
                }
                Err(e) => r.status = Some((e, true)),
            }
        }
        None if close => r.point_delete = None,
        None => r.point_delete = Some((id, ticks)),
    }
}

/// The window of a deletion touching spreads: a spread deleted alone or
/// with its points (those bare and used by nothing else), or a point a
/// spread ends on deleted with its spreads (a spread needs both its
/// points); what goes listed, Escape (Cancel) closes it.
pub(super) fn spread_delete_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(ask) = r.spread_delete.clone() else {
        return;
    };
    let line = r.model.stored_line().unwrap_or(&r.model.line).clone();
    let Some(sec) = line.sections.iter().find(|s| s.id == ask.section) else {
        r.spread_delete = None;
        return;
    };
    let section = sec.id.clone();
    // None: still open; Some(None): cancelled; Some(Some(..)): the points
    // and spreads to delete.
    let mut chosen: Option<Option<(Vec<String>, Vec<usize>)>> = None;
    let spread_text = |k: usize| {
        sec.spreads.get(k).map_or_else(String::new, |l| {
            format!(
                "{} from {} to {} ({} N/m{})",
                crate::properties::spread_name(sec, k),
                l.from,
                l.to,
                l.load,
                if l.per_subconductor {
                    " on each subconductor"
                } else {
                    ""
                }
            )
        })
    };
    match (&ask.spread, &ask.point) {
        (Some(k), _) => {
            let Some(l) = sec.spreads.get(*k) else {
                r.spread_delete = None;
                return;
            };
            let ends: Vec<(String, Vec<String>)> = [&l.from, &l.to]
                .into_iter()
                .map(|p| (p.clone(), point_uses(&line, sec, p, *k)))
                .collect();
            let free: Vec<String> = ends
                .iter()
                .filter(|(_, uses)| uses.is_empty())
                .map(|(p, _)| p.clone())
                .collect();
            let title = format!("Delete spread {}?", crate::properties::spread_name(sec, *k));
            tool_window(r, "spread-delete", title).show(ctx, |ui| {
                ui.label(format!("Spread {} of section {section}.", spread_text(*k)));
                ui.label("Its points:");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (p, uses) in &ends {
                            let text = if uses.is_empty() {
                                format!("  {p}: bare, nothing else uses it")
                            } else {
                                format!("  {p}: stays — {}", uses.join("; "))
                            };
                            ui.label(egui::RichText::new(text).small());
                        }
                    });
                ui.label("It is one log entry: undo brings everything back.");
                ui.horizontal(|ui| {
                    if ui
                        .button("Spread only")
                        .on_hover_text("Its points stay (bare points on the span)")
                        .clicked()
                    {
                        chosen = Some(Some((Vec::new(), vec![*k])));
                    }
                    if ui
                        .add_enabled(!free.is_empty(), egui::Button::new("Spread and its points"))
                        .on_hover_text(format!(
                            "Its points deleted too: {} (a point that holds an object or is \
                             used by something else stays)",
                            free.join(", ")
                        ))
                        .on_disabled_hover_text(
                            "Both its points hold an object or are used by something else",
                        )
                        .clicked()
                    {
                        chosen = Some(Some((free.clone(), vec![*k])));
                    }
                    if ui.button("Cancel").clicked() {
                        chosen = Some(None);
                    }
                });
            });
        }
        (None, Some(p)) => {
            let spreads = sec.spreads_at(p);
            let spacers: Vec<String> = line
                .spacers
                .iter()
                .filter(|x| {
                    [&x.a, &x.b]
                        .iter()
                        .any(|e| e.section == section && &e.point == p)
                })
                .map(|x| x.id.clone())
                .collect();
            tool_window(r, "spread-delete", format!("Delete span point {p}?")).show(ctx, |ui| {
                ui.label(format!(
                    "Span point {p} of section {section} ends {} spread(s): a spread needs both \
                     its points. Delete the point and the whole spread(s)?",
                    spreads.len()
                ));
                ui.label("This also deletes:");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for k in &spreads {
                            ui.label(
                                egui::RichText::new(format!("  spread {}", spread_text(*k)))
                                    .small(),
                            );
                        }
                        for x in &spacers {
                            ui.label(egui::RichText::new(format!("  spacer {x}")).small());
                        }
                    });
                ui.label(
                    "Their other points stay. It is one log entry: undo brings everything back.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Delete the point and its spreads").clicked() {
                        chosen = Some(Some((vec![p.clone()], spreads.clone())));
                    }
                    if ui.button("Cancel").clicked() {
                        chosen = Some(None);
                    }
                });
            });
        }
        (None, None) => {
            r.spread_delete = None;
            return;
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        chosen = Some(None);
    }
    match chosen {
        Some(Some((points, spreads))) => {
            r.spread_delete = None;
            if let Some(action) = span_delete_action(&line, &section, &points, &spreads) {
                r.perform(action);
            }
        }
        Some(None) => r.spread_delete = None,
        None => {}
    }
}

/// The "Move structure" window: E, N, z (or on the ground), previewed
/// live; Apply (Enter) moves it with one command, Cancel (Escape) puts it
/// back.
pub(super) fn move_window(ctx: &egui::Context, r: &mut Ready) {
    use crate::edit::Action;
    let Some(mut d) = r.move_dialog.clone() else {
        return;
    };
    let (mut apply, mut cancel) = (false, false);
    tool_window(r, "move", format!("Move structure {}", d.id)).show(ctx, |ui| {
        egui::Grid::new("move-grid").num_columns(3).show(ui, |ui| {
            for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                ui.label(name);
                let enabled = k < 2 || !d.on_ground;
                ui.add_enabled(
                    enabled,
                    egui::DragValue::new(&mut d.position[k])
                        .speed(0.1)
                        .max_decimals(3),
                );
                ui.label(
                    egui::RichText::new(format!("Δ {:+.3} m", d.position[k] - d.start[k])).small(),
                );
                ui.end_row();
            }
        });
        ui.checkbox(&mut d.on_ground, "base on the ground");
        let horizontal = (d.position[0] - d.start[0]).hypot(d.position[1] - d.start[1]);
        ui.label(egui::RichText::new(format!("moved {horizontal:.3} m horizontally")).small());
        ui.horizontal(|ui| {
            apply |= ui.button("Apply").on_hover_text("Enter").clicked();
            cancel |= ui.button("Cancel").on_hover_text("Escape").clicked();
        });
    });
    if d.on_ground
        && let Some(z) = r.model.ground_at(d.position[0], d.position[1])
    {
        d.position[2] = z;
    }
    apply |= ctx.input(|i| i.key_pressed(egui::Key::Enter));
    cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if cancel {
        r.cancel_move_preview();
        return;
    }
    if apply {
        r.move_dialog = None;
        if d.position != d.start {
            r.perform(Action::Move {
                id: d.id,
                position: d.position,
            });
        }
        return;
    }
    if d.position != d.shown {
        d.shown = d.position;
        r.perform(Action::PreviewMove {
            id: d.id.clone(),
            position: d.position,
        });
    }
    r.move_dialog = Some(d);
}
