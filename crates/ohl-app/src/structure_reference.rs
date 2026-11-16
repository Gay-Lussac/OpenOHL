// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's reference state of the structures in OHL-Design (plan of
//! 2026-11-15, phase 5b; owner 2026-11-16; in active development):
//!
//! - **Chosen** in Line › Criteria › Calculation options under the
//!   calculation level ([`ui`]): a weather case, **Solve and keep** (the
//!   case solved at calculation level 3 on its own thread,
//!   [`ohl_loads::reference::solve_reference`], then kept with the line by
//!   one undoable command), **Remove** (one command too).
//! - **Checked** whenever the line comes from the project (an edit, its
//!   undo, a library read again, the project opened: [`check`]); a state
//!   out of date is never used, and the views and results say so. A
//!   structure dragged makes it out of date until let go.
//! - **Drawn**: the structures with a model drawn deflected in the 3D view
//!   (and wherever their bodies are drawn: the sheets) while the state is
//!   in use, the 3D view noting it ([`paint_note`]); their bodies are not
//!   stored — made again on their own thread from the stored forces
//!   ([`frame`]).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui;
use ohl_line::Line;
use ohl_line::reference::{ReferenceState, StructureReference};
use ohl_loads::reference::ReferenceSolve;
use ohl_structures_fe::drawn::BodyDeflection;
use ohl_weather::WeatherCase;

use crate::job::Job;
use crate::model::Model;

/// Why internal clearances keep the structures as drawn.
pub const INTERNAL_AS_DRAWN: &str = "internal clearances keep the structures and their strings \
    as drawn: the structure body is not deflected in the check";

/// Why the As-built wires window compares with the structures as drawn
/// (its switch unticked).
pub const LIDAR_AS_DRAWN: &str = "As-built wires: compared with the structures as drawn (its \
    switch unticked)";

/// `line`'s reference state checked against the libraries and weather
/// cases given (`types` none: no structure library; the cross-sections
/// read from `sections`); nothing without a reference case.
///
/// Expected cost: [`ohl_loads::reference::fingerprint`] (a few ms; the
/// cross-section files read).
pub fn check_with(
    line: &mut Line,
    library: &ohl_cables::Library,
    types: Option<&ohl_structures::StructureLibrary>,
    sections: &Path,
    weather: &[WeatherCase],
) {
    if line.reference.is_none() && !line.has_foot_movements() {
        return;
    }
    let empty = ohl_structures::StructureLibrary::default();
    let sections = ohl_sections::SectionLibrary::load(sections).unwrap_or_default();
    ohl_loads::reference::check_reference(
        line,
        library,
        types.unwrap_or(&empty),
        &sections,
        weather,
    );
}

/// [`check_with`] with the model's libraries and weather cases (as
/// shown, as the reference solve takes them).
pub fn check(model: &Model, line: &mut Line) {
    let weather: Vec<WeatherCase> = model.cases.iter().map(|c| c.case.clone()).collect();
    check_with(
        line,
        &model.library,
        model.types.as_ref().map(|(t, _)| t),
        &model.sections_folder(),
        &weather,
    );
}

/// The key of a state's deflected bodies (another state: made again).
fn key(state: &ReferenceState) -> String {
    format!(
        "{}|{:?}|{}",
        state.case,
        state.fingerprint,
        state.points.len()
    )
}

/// The deflected bodies of a state's flexible structures (by id), and the
/// structures whose body could not be made, with why.
type Deflections = (BTreeMap<String, BodyDeflection>, Vec<String>);

/// What OHL-Design keeps of the reference state: the solve under way, the
/// deflected bodies made for a state, the case chosen in the window.
#[derive(Default)]
pub struct ReferenceTool {
    /// The case chosen in the window (none: the line's).
    pub chosen: Option<String>,
    /// The solve of a reference case.
    pub job: Job<ReferenceSolve>,
    /// The deflected bodies of a state (its key).
    bodies: Option<(String, Arc<Deflections>)>,
    /// Bodies being made for a state (its key).
    making: Option<(String, Receiver<Deflections>)>,
    /// The last solve's message (kept, or why not).
    pub said: Option<(String, bool)>,
}

impl ReferenceTool {
    /// Starts the solve of case `case` on the model as it is.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context, case: &str) {
        let line = model.line.clone();
        let library = model.library.clone();
        let types = model
            .types
            .as_ref()
            .map(|(t, _)| t.clone())
            .unwrap_or_default();
        let sections = model.section_library().unwrap_or_default();
        let weather: Vec<WeatherCase> = model.cases.iter().map(|c| c.case.clone()).collect();
        let case = case.to_owned();
        let token = self.job.cancel_token();
        self.said = None;
        self.job.start_at(ctx, model.revision(), move || {
            ohl_loads::reference::solve_reference(
                &line,
                &library,
                &types,
                &sections,
                &weather,
                &case,
                &ohl_loads::default_budget(),
                None,
                Some(&token),
            )
        });
    }
}

/// Every frame: a solve arrived is kept with the line (one command), its
/// bodies with it; the bodies of the state in use made (on their own
/// thread) and drawn, none drawn deflected otherwise.
pub fn frame(r: &mut crate::app::Ready, ctx: &egui::Context) {
    let generation = r.structure_reference.job.generation;
    r.structure_reference.job.poll();
    if r.structure_reference.job.generation != generation {
        match r.structure_reference.job.result.take() {
            Some(Ok(solved)) => {
                let k = key(&solved.state);
                let state = solved.state.clone();
                r.structure_reference.bodies = Some((
                    k,
                    Arc::new((solved.bodies.clone(), solved.bodies_failed.clone())),
                ));
                let mut reference = StructureReference::new(&state.case);
                let words = format!(
                    "structures solved in case \"{}\" at calculation level {} in {:.1} s ({} \
                     passes), kept as the line's reference state",
                    state.case, state.level, state.seconds, state.passes
                );
                reference.state = Some(state);
                let command = r
                    .model
                    .editing
                    .as_ref()
                    .map(|e| e.session.reference_command(Some(reference)));
                match command {
                    Ok(command) => {
                        r.perform(crate::edit::Action::Command {
                            what: "reference state of the structures".into(),
                            command,
                        });
                        r.structure_reference.said = Some((words, false));
                    }
                    Err(why) => {
                        r.structure_reference.said = Some((format!("read-only: {why}"), true));
                    }
                }
                r.structure_reference.job.result = Some(Ok(solved));
            }
            Some(Err(e)) => {
                r.structure_reference.said = Some((format!("not solved: {e}"), true));
                r.structure_reference.job.result = Some(Err(e));
            }
            None => {}
        }
    }
    let tool = &mut r.structure_reference;
    // Bodies arriving.
    if let Some((k, rx)) = &tool.making {
        match rx.try_recv() {
            Ok(made) => {
                tool.bodies = Some((k.clone(), Arc::new(made)));
                tool.making = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => tool.making = None,
        }
    }
    let wanted = r.model.line.reference_in_use().map(|s| (key(s), s.clone()));
    let drawn = r.model.bodies.placed_count() > 0;
    match wanted {
        None => {
            if drawn {
                r.model.bodies = Arc::new(r.model.bodies.with_placed(BTreeMap::new()));
                r.view3d.dirty.insert(crate::scene::Layer::Structures);
            }
            r.model.reference_drawn = None;
        }
        Some((k, state)) => {
            let have = tool.bodies.as_ref().filter(|(b, _)| *b == k).cloned();
            match have {
                Some((_, made)) => {
                    if r.model.reference_drawn.as_deref() != Some(&k) || !drawn {
                        let mut placed = BTreeMap::new();
                        for (id, d) in &made.0 {
                            let Some(st) = r.model.line.structure(id) else {
                                continue;
                            };
                            let Some(b) = st
                                .structure_type
                                .as_deref()
                                .and_then(|t| r.model.bodies.get(t, st.extension.as_deref()))
                            else {
                                continue;
                            };
                            placed.insert(id.clone(), b.deflected(d));
                        }
                        r.model.bodies = Arc::new(r.model.bodies.with_placed(placed));
                        r.model.reference_drawn = Some(k);
                        r.view3d.dirty.insert(crate::scene::Layer::Structures);
                    }
                }
                None if tool.making.as_ref().is_none_or(|(m, _)| *m != k) => {
                    let line = r.model.line.clone();
                    let types = r
                        .model
                        .types
                        .as_ref()
                        .map(|(t, _)| t.clone())
                        .unwrap_or_default();
                    let sections = r.model.section_library().unwrap_or_default();
                    let weather: Vec<WeatherCase> =
                        r.model.cases.iter().map(|c| c.case.clone()).collect();
                    let (tx, rx) = std::sync::mpsc::channel();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let made = ohl_loads::reference::reference_bodies(
                            &line, &types, &sections, &state, &weather,
                        );
                        let _ = tx.send(made);
                        ctx.request_repaint();
                    });
                    tool.making = Some((k, rx));
                }
                None => {}
            }
        }
    }
}

/// The 3D view's note of the reference state (bottom left, in the
/// structures' colour, no box): drawn deflected in case X, or out of
/// date (as drawn); drawn on their measured foot movements (owner
/// 2026-11-16) when those are what is used; none without a reference
/// case or foot movements.
pub fn view_note(model: &Model) -> Option<(String, bool)> {
    let making = if model.reference_drawn.is_none() {
        " — bodies being made"
    } else {
        ""
    };
    let in_use = model.line.reference_in_use();
    let feet = in_use.filter(|s| s.level == ohl_loads::reference::FEET_LEVEL);
    let feet_words = |s: &ohl_line::reference::ReferenceState| {
        format!(
            "structures drawn on their measured foot movements alone (no load){}{making}",
            if s.rigid.is_empty() {
                String::new()
            } else {
                format!(" — NOT taken at {} structure(s)", s.rigid.len())
            }
        )
    };
    let Some(r) = model.line.reference.as_ref() else {
        let s = feet?;
        let mut w = feet_words(s);
        w[..1].make_ascii_uppercase();
        return Some((w, !s.rigid.is_empty()));
    };
    Some(match in_use {
        Some(s) if feet.is_none() => (
            format!(
                "Structures drawn as deflected in case \"{}\" (calculation level {}){}{making}",
                s.case,
                s.level,
                if s.fingerprint.feet.is_empty() {
                    ""
                } else {
                    " on their measured foot movements"
                }
            ),
            false,
        ),
        _ => {
            let why = match (&r.state, &r.status) {
                (None, _) => format!("Reference case \"{}\" not solved", r.case),
                (_, ohl_line::reference::ReferenceStatus::Stale(why)) => {
                    format!("Reference case \"{}\" out of date ({why})", r.case)
                }
                _ => format!("Reference case \"{}\" not in use", r.case),
            };
            match feet {
                Some(s) => (format!("{why}: {}", feet_words(s)), true),
                None => (format!("{why}: structures as drawn"), true),
            }
        }
    })
}

/// Paints [`view_note`] at the bottom left of `rect`.
pub fn paint_note(painter: &egui::Painter, rect: egui::Rect, model: &Model) {
    let Some((text, warn)) = view_note(model) else {
        return;
    };
    let colour = if warn {
        egui::Color32::from_rgb(255, 190, 60)
    } else {
        egui::Color32::from_rgb(215, 215, 220)
    };
    painter.text(
        rect.left_bottom() + egui::vec2(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        text,
        egui::FontId::proportional(12.0),
        colour,
    );
}

/// The section of the Calculation options window: the reference case,
/// its state, Solve and keep, Remove — each kept by one undoable command.
pub fn ui(ui: &mut egui::Ui, r: &mut crate::app::Ready) {
    ui.horizontal(|ui| {
        ui.strong("Reference state of the structures");
        crate::check_help::question(ui, HELP);
    });
    ui.label(
        "A weather case the structures are solved in once (at calculation level 3) and kept: \
         the structures are then drawn deflected as in it, and every rigid (level 2) solve \
         takes their attachment points as moved there — e.g. the survey day's temperature and \
         wind, to make the model match the LiDAR scan.",
    );
    ui.label(
        egui::RichText::new(TOWERS_FROM_PLANS)
            .small()
            .color(ui.visuals().warn_fg_color),
    );
    let names: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let set = r.model.line.reference.as_ref().map(|x| x.case.clone());
    let tool = &mut r.structure_reference;
    let mut chosen = tool.chosen.clone().or_else(|| set.clone());
    ui.horizontal(|ui| {
        ui.label("Reference case");
        egui::ComboBox::from_id_salt("reference-case")
            .selected_text(chosen.as_deref().unwrap_or("— none —"))
            .width(240.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, "— none —");
                for n in &names {
                    ui.selectable_value(&mut chosen, Some(n.clone()), n);
                }
            });
    });
    tool.chosen = chosen.clone();
    // What the line has now.
    let note = r.model.line.reference_note();
    let in_use = r.model.line.reference_in_use().cloned();
    match (&note, &in_use) {
        (Some(n), Some(s)) if s.level == ohl_loads::reference::FEET_LEVEL => {
            let mut ids: Vec<&str> = s.points.iter().map(|p| p.structure.as_str()).collect();
            ids.dedup();
            ui.label(
                egui::RichText::new(format!(
                    "In use: the measured foot movements alone — {} points of {} structure(s) \
                     moved (solved without loads, automatically, in {:.2} s). {n}",
                    s.points.len(),
                    ids.len(),
                    s.seconds
                ))
                .small(),
            );
        }
        (Some(_), Some(s)) => {
            let most = ohl_loads::reference::moves_in_structure_axes(&r.model.line, s)
                .into_iter()
                .map(|(st, l, u)| ((u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt(), st, l))
                .fold(None::<(f64, String, String)>, |best, x| match best {
                    Some(b) if b.0 >= x.0 => Some(b),
                    _ => Some(x),
                });
            ui.label(
                egui::RichText::new(format!(
                    "In use: case \"{}\" at calculation level {}, {} points of {} structures \
                     moved{}; {} structure(s) as drawn (no model, or refused: listed in the \
                     results). Solved in {:.1} s ({} passes) on the computer that solved it.",
                    s.case,
                    s.level,
                    s.points.len(),
                    {
                        let mut ids: Vec<&str> =
                            s.points.iter().map(|p| p.structure.as_str()).collect();
                        ids.dedup();
                        ids.len()
                    },
                    most.map_or(String::new(), |(d, st, l)| format!(
                        ", the most {:.0} mm at {st} {l}",
                        d * 1e3
                    )),
                    s.rigid.len(),
                    s.seconds,
                    s.passes
                ))
                .small(),
            );
            if let Some((_, made)) = &r.structure_reference.bodies
                && !made.1.is_empty()
            {
                ui.label(
                    egui::RichText::new(format!(
                        "Drawn as drawn (their deflected body not made): {}",
                        made.1.join("; ")
                    ))
                    .small()
                    .weak(),
                );
            }
        }
        (Some(n), None) => {
            ui.colored_label(ui.visuals().warn_fg_color, egui::RichText::new(n).small());
        }
        (None, _) => {
            ui.label(
                egui::RichText::new("None: the structures as drawn (the default).")
                    .small()
                    .weak(),
            );
        }
    }
    let can_edit = r.model.can_edit().is_ok();
    let running = r.structure_reference.job.running();
    let mut solve = false;
    let mut remove = false;
    let mut stop = false;
    ui.horizontal(|ui| {
        solve = ui
            .add_enabled(
                can_edit && chosen.is_some() && !running,
                egui::Button::new("Solve and keep"),
            )
            .on_hover_text(
                "Solve the structures in the chosen case at calculation level 3 and keep the \
                 state with the line (one undoable step)",
            )
            .clicked();
        remove = ui
            .add_enabled(
                can_edit && set.is_some() && !running,
                egui::Button::new("Remove"),
            )
            .on_hover_text("The structures as drawn again (one undoable step)")
            .clicked();
        if running {
            ui.spinner();
            ui.label(format!(
                "solving… {:.0?}",
                r.structure_reference.job.elapsed().unwrap_or_default()
            ));
            stop = ui.button("Stop").clicked();
        }
    });
    if let Some((said, bad)) = &r.structure_reference.said {
        if *bad {
            crate::menu::error_label(ui, said.clone());
        } else {
            ui.label(egui::RichText::new(said).small());
        }
    }
    ui.label(
        egui::RichText::new(format!(
            "Time: once per solve — the structures set up (one condensation per structure) and \
             the case's coupled solve (2–5 line solves; on the example line 0.6 s); every later \
             rigid solve as fast as before. {}",
            crate::calc_level::beside(ohl_line::StructuresInSolve::Linear)
        ))
        .small()
        .weak(),
    );
    if stop {
        r.structure_reference.job.stop();
    }
    if solve && let Some(case) = chosen {
        let ctx = ui.ctx().clone();
        r.structure_reference.start(&r.model, &ctx, &case);
    }
    if remove {
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.reference_command(None));
        if let Ok(command) = command {
            r.structure_reference.chosen = None;
            r.perform(crate::edit::Action::Command {
                what: "reference state of the structures removed".into(),
                command,
            });
        }
    }
}

/// How towers are built (owner 2026-11-16: "the tower should not be
/// initially built from a scan but from plans then adjusted based on the
/// movement of the feet and the lean of the weather case"), shown in the
/// section.
pub const TOWERS_FROM_PLANS: &str = "Model the towers from their design plans, not from \
    scanned positions: their as-built lean comes from the measured movements of their feet \
    (each structure's form: Foot movements) and from the reference case's deflection. Points \
    placed from a scan already contain the lean: with a reference case it would be counted \
    twice.";

/// The (?) help of the section.
const HELP: &str = "The structures solved once in a weather case at calculation level 3 \
    (each structure with a model a linear stiffness at its attachment points, condensed at the \
    stringing state, its own wind and ice of the case in it) and that state kept with the line. \
    While it is current: the structures are drawn deflected as in it (3D view; the views say \
    so), and every rigid (level 2) solve — the views, the tables, the clearances, the LiDAR \
    comparison — takes each attachment point moved by its displacement there. Every other \
    case keeps that lean, not its own: NOT ON THE SAFE SIDE where a case would lean the \
    structures further (heavier ice, stronger wind) — check those at calculation level 3. \
    Not taken: the structure loads (kept as drawn, rigid: on the safe side), internal \
    clearances (the body is not deflected in the check), calculation level 3 (each case its own \
    deflection). An edit of the line, the wires, the structures or the case's weather makes it \
    out of date: it is then not used (the structures as drawn, said) until solved again. \
    Structures without a model stay as drawn, listed.";
