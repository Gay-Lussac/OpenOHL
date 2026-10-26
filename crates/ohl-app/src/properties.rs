// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The properties of the selection: a structure's data and attachments
//! with the sections through it, a span's section, electrical system and
//! results in every weather case.

use eframe::egui;

use crate::model::Model;
use crate::selection::Selection;

/// The button opening a structure's structure screen (its strings and
/// points beside the pylon in 3D).
pub const STRINGS_AND_POINTS: &str = "Strings and points…";

/// The button opening a structure's type in the other program,
/// OHL-Structure (its own window).
pub const OPEN_IN_STRUCTURE_APP: &str = "Open in OHL-Structure…";

fn grid(ui: &mut egui::Ui, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(*k);
                ui.label(v);
                ui.end_row();
            }
        });
}

/// Shows the properties of `selection`; clicking a related object selects
/// it.
/// `position`: the position being typed for a structure, when the model
/// can be edited (`None`: read-only). Returns an edit to perform.
/// What Properties needs for an attachment point beyond the model: the
/// structure loads computed (its moment per load case), editing allowed,
/// and where its Link… asks for the Link tool.
pub struct PointExtras<'a> {
    /// Checks › Structure loads, when computed.
    pub loads: Option<&'a [ohl_loads::CaseLoads]>,
    /// The model can be edited.
    pub editable: bool,
    /// Link… clicked: (structure, label).
    pub start_link: &'a mut Option<(String, String)>,
    /// "Cross-section here" chosen in a structure's menu: the structure.
    pub cut_at: &'a mut Option<String>,
    /// "Solve all" clicked (every weather case solved, not only those
    /// shown).
    pub solve_all: &'a mut bool,
}

pub fn ui(
    ui: &mut egui::Ui,
    model: &Model,
    case: usize,
    selection: &mut Selection,
    position: Option<&mut Option<(String, [f64; 3])>>,
    point: PointExtras<'_>,
) -> Option<crate::edit::Action> {
    let line = &model.line;
    let mut next: Option<Selection> = None;
    let mut action: Option<crate::edit::Action> = None;
    egui::ScrollArea::vertical().show(ui, |ui| match selection.clone() {
        Selection::None => {
            ui.label("Click a structure or a wire in a view.");
            ui.separator();
            ui.heading(&line.name);
            grid(
                ui,
                "line",
                &[
                    ("CRS", line.crs.clone()),
                    ("height datum", crate::crs::describe_height_datum(model)),
                    ("structures", line.structures.len().to_string()),
                    ("sections", line.sections.len().to_string()),
                    ("jumpers", line.jumpers.len().to_string()),
                    ("systems", line.systems.len().to_string()),
                    ("weather cases", model.cases.len().to_string()),
                ],
            );
        }
        Selection::Structure(id) => {
            let Some(s) = line.structure(&id) else {
                ui.label(format!("{id}: not in the line"));
                return;
            };
            ui.heading(format!("Structure {id}")).context_menu(|ui| {
                if ui
                    .button("Cross-section here")
                    .on_hover_text(
                        "A cut across the line in the plane of its cross-arms, in a tab of its \
                         own (C: the window)",
                    )
                    .clicked()
                {
                    *point.cut_at = Some(id.clone());
                    ui.close();
                }
            });
            description(ui, &s.description);
            // Attachment points whose joint is gone (or unlinked): drawn red,
            // the structure's loads refused until they are linked again.
            let lost: Vec<String> = crate::link_tool::unlinked_points(line)
                .into_iter()
                .filter(|(st, _, _)| *st == id)
                .map(|(_, l, _)| l)
                .collect();
            if !lost.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "Unlinked attachment point(s) {}: drawn red; its loads are refused \
                         until they are linked again (Link tool, or Replace's Attachment points)",
                        lost.join(", ")
                    ),
                );
                for l in &lost {
                    if ui.link(format!("attachment point {l}")).clicked() {
                        next = Some(Selection::Point {
                            structure: id.clone(),
                            label: l.clone(),
                        });
                    }
                }
            }
            grid(
                ui,
                "structure",
                &[
                    (
                        "type",
                        s.structure_type.clone().unwrap_or_else(|| "—".into()),
                    ),
                    (
                        "detail",
                        s.structure_type
                            .as_deref()
                            .and_then(|t| model.types.as_ref()?.0.get(t))
                            .map_or("—".into(), |t| {
                                format!("{} — {}", t.detail().level(), t.detail().name())
                            }),
                    ),
                    (
                        "extension",
                        s.extension.clone().unwrap_or_else(|| "—".into()),
                    ),
                    ("height adjustment", format!("{:.2} m", s.height_adjust)),
                    ("easting", format!("{:.3} m", s.position[0])),
                    ("northing", format!("{:.3} m", s.position[1])),
                    ("ground", format!("{:.3} m", s.position[2])),
                    ("orientation", format!("{:.2}°", s.orientation_deg)),
                    (
                        "routes",
                        match (line.routes_of(&id), s.routes.is_empty()) {
                            (r, _) if r.is_empty() => "—".into(),
                            (r, true) => format!("{} (from the alignments)", r.join(", ")),
                            (r, false) => format!("{} (its own)", r.join(", ")),
                        },
                    ),
                ],
            );
            if line.routes_of(&id).is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "On no route: it stands on no alignment. Give its route(s) in its window \
                     (Edit) or the Structures table.",
                );
            }
            if let Some(position) = position {
                // Typed position: applied as one move.
                let buffer = match position {
                    Some((sid, p)) if *sid == id => p,
                    _ => &mut position.insert((id.clone(), s.position)).1,
                };
                ui.separator();
                ui.horizontal(|ui| {
                    for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                        ui.label(name);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut buffer[k])
                                .speed(0.1)
                                .max_decimals(3),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    let moved = (0..3).any(|k| (buffer[k] - s.position[k]).abs() > 1e-9);
                    if ui.add_enabled(moved, egui::Button::new("Move")).clicked() {
                        action = Some(crate::edit::Action::Move {
                            id: id.clone(),
                            position: *buffer,
                        });
                    }
                    if ui.button("Delete…").on_hover_text("Delete key").clicked() {
                        action = Some(crate::edit::Action::Delete {
                            kind: ohl_session::Kind::Structure,
                            id: id.clone(),
                            confirm: false,
                        });
                    }
                });
            }
            if let Some(t) = &s.structure_type
                && ui
                    .button(STRINGS_AND_POINTS)
                    .on_hover_text(format!(
                        "Type {t} on the structure screen: its attachment points, strings, wire \
                         points and sets beside the structure in 3D (every structure of the type \
                         follows when saved)"
                    ))
                    .clicked()
            {
                action = Some(crate::edit::Action::EditType { id: t.clone() });
            }
            if let Some(t) = &s.structure_type
                && ui
                    .button(OPEN_IN_STRUCTURE_APP)
                    .on_hover_text(format!(
                        "Open structure type {t} in OHL-Structure (its own window) with \
                         the loads the line puts on {id}; saved there, every structure of the \
                         type follows, and values typed over its load table stay with {id}"
                    ))
                    .clicked()
            {
                action = Some(crate::edit::Action::EditInStructureApp {
                    id: t.clone(),
                    structure: Some(id.clone()),
                });
            }
            if ui
                .button("Placement…")
                .on_hover_text(
                    "Where it stands (coordinates or an alignment point), its orientation, \
                     type, extension and description; Put it on the line again",
                )
                .clicked()
            {
                action = Some(crate::edit::Action::EditPlacement { id: id.clone() });
            }
            if ui
                .button("Replace…")
                .on_hover_text(
                    "Another structure type in its place (Replace structure): matching wire \
                     points are rewired, the others you map — what would be removed is listed",
                )
                .clicked()
            {
                action = Some(crate::edit::Action::ReplaceStructure { id: id.clone() });
            }
            if s.structure_type.is_none()
                && ui
                    .button(STRINGS_AND_POINTS)
                    .on_hover_text(
                        "Its points, wire points, strings and sets beside its drawing in 3D, on \
                         the structure screen (Make it a type… there saves it as a structure \
                         type)",
                    )
                    .clicked()
            {
                action = Some(crate::edit::Action::EditStructure { id: id.clone() });
            }
            if s.structure_type.is_none()
                && ui
                    .button(OPEN_IN_STRUCTURE_APP)
                    .on_hover_text(
                        "It has no structure file for OHL-Structure to open: make it one (a \
                         structure type of the project, the structure takes it) and open that \
                         in OHL-Structure",
                    )
                    .clicked()
            {
                action = Some(crate::edit::Action::MakeFileAndEdit { id: id.clone() });
            }
            ui.separator();
            ui.label(egui::RichText::new("Wire points").strong());
            egui::Grid::new("attachments").striped(true).show(ui, |ui| {
                for h in ["set", "label", "strings", "z", "weight"] {
                    ui.label(egui::RichText::new(h).small());
                }
                ui.end_row();
                for w in &s.wires {
                    ui.label(w.set.map_or(String::new(), |x| x.to_string()));
                    ui.label(&w.label);
                    let on: Vec<&ohl_line::InsulatorString> = s.strings_to(&w.label).collect();
                    ui.label(match on.len() {
                        0 => "fixed".to_owned(),
                        _ => on
                            .iter()
                            .map(|x| format!("{} from {}", x.label, x.top))
                            .collect::<Vec<_>>()
                            .join(", "),
                    });
                    ui.label(format!("{:.2}", w.position[2]));
                    ui.label(format!("{:.0} N", on.iter().map(|x| x.weight).sum::<f64>()));
                    ui.end_row();
                }
            });
            ui.separator();
            ui.label(egui::RichText::new("Spans at this structure").strong());
            for sec in &line.sections {
                for (k, w) in sec.points.windows(2).enumerate() {
                    if w[0].structure == id || w[1].structure == id {
                        let text = format!(
                            "{} span {} ({} {}): {} ➡ {}",
                            sec.id,
                            k + 1,
                            sec.circuit_name,
                            sec.phase_label,
                            w[0].structure,
                            w[1].structure
                        );
                        if ui.link(text).clicked() {
                            next = Some(Selection::Span {
                                section: sec.id.clone(),
                                span: k,
                            });
                        }
                    }
                }
            }
            let jumpers: Vec<&ohl_line::Jumper> = line
                .jumpers
                .iter()
                .filter(|jp| jp.points().any(|p| p.structure == id))
                .collect();
            if !jumpers.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Jumpers at this structure").strong());
                for jp in jumpers {
                    // Its ends here by their set; a tap by where it is.
                    let end = |e: &ohl_line::JumperEnd| match e {
                        ohl_line::JumperEnd::Wire(p) => format!("set {}", p.set),
                        ohl_line::JumperEnd::Tap { .. } => e.to_string(),
                    };
                    let text = format!("{}: {} ➡ {}", jp.id, end(&jp.from), end(&jp.to));
                    if ui.link(text).clicked() {
                        next = Some(Selection::Jumper(jp.id.clone()));
                    }
                }
            }
        }
        Selection::Attachment { structure, set } => {
            let at = ohl_line::SectionPoint::new(structure.clone(), set);
            let Some(st) = line.structure(&structure) else {
                ui.label(format!("{structure}: not in the line"));
                return;
            };
            let Some(w) = st.wire_at_set(set) else {
                ui.label(format!("{structure} set {set}: not in the line"));
                return;
            };
            ui.heading(format!("{structure} set {set}"));
            description(ui, &w.description);
            if ui.link(format!("structure {structure}")).clicked() {
                next = Some(Selection::Structure(structure.clone()));
            }
            let p3 = |p: [f64; 3]| format!("{:.3}, {:.3}, {:.3}", p[0], p[1], p[2]);
            let c = st.component(&w.label);
            let mut rows: Vec<(&str, String)> = vec![
                ("wire point", format!("{} at {}", w.label, p3(w.position))),
                (
                    "held by",
                    if c.strings.is_empty() {
                        "fixed to the structure".into()
                    } else {
                        format!("{} strings", c.strings.len())
                    },
                ),
            ];
            for &i in &c.strings {
                let x = &st.strings[i];
                rows.push((
                    "string",
                    format!(
                        "{}: {} ➡ {}, {:.3} m, {:.0} N{}{}{}",
                        x.label,
                        x.top,
                        x.wire,
                        st.string_length(x).unwrap_or(0.0),
                        x.weight,
                        x.insulator
                            .as_deref()
                            .map_or(String::new(), |i| format!(", {i}")),
                        x.compression
                            .map_or(String::new(), |c| format!(", up to {c:.0} N compression")),
                        if x.strain { ", strain" } else { "" }
                    ),
                ));
            }
            grid(
                ui,
                "attachment",
                &rows
                    .iter()
                    .map(|(a, b)| (*a, b.clone()))
                    .collect::<Vec<_>>(),
            );
            ui.separator();
            ui.label(egui::RichText::new("Spans at this attachment").strong());
            for sec in &line.sections {
                for (k, w) in sec.points.windows(2).enumerate() {
                    if (w[0] == at || w[1] == at)
                        && ui
                            .link(format!(
                                "{} span {} ({} {})",
                                sec.id,
                                k + 1,
                                sec.circuit_name,
                                sec.phase_label
                            ))
                            .clicked()
                    {
                        next = Some(Selection::Span {
                            section: sec.id.clone(),
                            span: k,
                        });
                    }
                }
            }
            for jp in &line.jumpers {
                if (jp.from == at || jp.to == at) && ui.link(format!("jumper {}", jp.id)).clicked()
                {
                    next = Some(Selection::Jumper(jp.id.clone()));
                }
            }
        }
        Selection::Point { structure, label } => {
            let Some(st) = line.structure(&structure) else {
                ui.label(format!("{structure}: not in the line"));
                return;
            };
            let heading = ui.heading(format!("{structure} point {label}"));
            // An attachment point: its link to a joint, the offset (a
            // rigid link) and its moment per load case; its menu links or
            // unlinks it.
            if let Some(p) = st.point(&label) {
                let typed = st.structure_type.is_some();
                heading.context_menu(|ui| {
                    if ui
                        .add_enabled(typed, egui::Button::new("Link to a joint… (Link tool)"))
                        .clicked()
                    {
                        *point.start_link = Some((structure.clone(), label.clone()));
                        ui.close();
                    }
                    if ui
                        .add_enabled(typed && p.is_linked(), egui::Button::new("Unlink"))
                        .on_hover_text(
                            "It stays where it is; the structure's loads are refused until it \
                             is linked again",
                        )
                        .clicked()
                    {
                        match crate::link_tool::unlink_command(model, &structure, &label) {
                            Ok((what, command)) => {
                                action = Some(crate::edit::Action::Command { what, command });
                            }
                            Err(e) => {
                                ui.colored_label(ui.visuals().error_fg_color, e);
                            }
                        }
                        ui.close();
                    }
                });
                let rows = crate::link_tool::point_rows(st, p, point.loads);
                let rows: Vec<(&str, String)> =
                    rows.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
                grid(ui, "point-link", &rows);
                if typed && point.editable {
                    ui.horizontal(|ui| {
                        if ui
                            .button("Link…")
                            .on_hover_text(
                                "The Link tool on this point: click a ringed joint of its \
                                 type (it is linked there where it is)",
                            )
                            .clicked()
                        {
                            *point.start_link = Some((structure.clone(), label.clone()));
                        }
                        if p.is_linked()
                            && ui
                                .button("Unlink")
                                .on_hover_text(
                                    "It stays where it is; the structure's loads are refused \
                                     until it is linked again",
                                )
                                .clicked()
                            && let Ok((what, command)) =
                                crate::link_tool::unlink_command(model, &structure, &label)
                        {
                            action = Some(crate::edit::Action::Command { what, command });
                        }
                    });
                }
                if p.offset != [0.0; 3] && point.loads.is_none() {
                    ui.label(
                        egui::RichText::new(
                            "Its moment per load case: Checks › Structure loads (compute)",
                        )
                        .small()
                        .weak(),
                    );
                }
            }
            if ui.link(format!("structure {structure}")).clicked() {
                next = Some(Selection::Structure(structure.clone()));
            }
            let what = match (st.point(&label), st.wire(&label)) {
                (Some(_), _) => "a point of the structure".to_owned(),
                (None, Some(w)) => match w.set {
                    Some(set) => format!("a wire point, set {set}"),
                    None => "a wire point (no set)".to_owned(),
                },
                (None, None) => "not on the structure any more".to_owned(),
            };
            ui.label(what);
            let hanging: Vec<&str> = st
                .strings
                .iter()
                .filter(|x| x.top == label)
                .map(|x| x.label.as_str())
                .collect();
            ui.label(if hanging.is_empty() {
                "No string hangs from it.".to_owned()
            } else {
                format!("Strings hanging from it: {}", hanging.join(", "))
            });
        }
        Selection::SpanPoint { section, label } => {
            ui.heading(format!("{section} span point {label}"));
            let Some(p) = line
                .sections
                .iter()
                .find(|s| s.id == section)
                .and_then(|s| s.span_points.iter().find(|p| p.label == label))
            else {
                ui.label("Not in the line any more.");
                return;
            };
            if ui
                .link(format!("section {section}, span {}", p.span + 1))
                .clicked()
            {
                next = Some(Selection::Span {
                    section: section.clone(),
                    span: p.span,
                });
            }
            description(ui, &p.description);
            let mut rows = vec![
                ("span".to_owned(), (p.span + 1).to_string()),
                ("place".to_owned(), p.at.text()),
            ];
            if let Some((_, q)) = model
                .shape(&section, case)
                .and_then(|s| s.span_points.iter().find(|(l, _)| *l == label))
            {
                rows.push(("height (case shown)".to_owned(), format!("{:.2} m", q[2])));
            }
            rows.extend(object_rows(p.object.as_ref()));
            let rows: Vec<(&str, String)> =
                rows.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
            grid(ui, "span-point", &rows);
            let tap = ohl_line::Tap::Span(ohl_line::SpanPointRef {
                section: section.clone(),
                point: label.clone(),
            });
            for j in line.jumpers.iter().filter(|j| j.taps().any(|t| *t == tap)) {
                if ui.link(format!("jumper {} (on this tap)", j.id)).clicked() {
                    next = Some(Selection::Jumper(j.id.clone()));
                }
            }
            for s in line.spacers.iter().filter(|s| {
                [&s.a, &s.b]
                    .iter()
                    .any(|e| e.section == section && e.point == label)
            }) {
                if ui.link(format!("spacer {}", s.id)).clicked() {
                    next = Some(Selection::Spacer(s.id.clone()));
                }
            }
            // The spreads it ends.
            if let Some(sec) = line.sections.iter().find(|s| s.id == section) {
                for k in sec.spreads_at(&label) {
                    if ui
                        .link(format!("spread {} (it ends here)", spread_name(sec, k)))
                        .clicked()
                    {
                        next = Some(Selection::Spread {
                            section: section.clone(),
                            index: k,
                        });
                    }
                }
            }
        }
        Selection::Spread { section, index } => {
            let Some(sec) = line.sections.iter().find(|s| s.id == section) else {
                ui.label("Not in the line any more.");
                return;
            };
            let Some(l) = sec.spreads.get(index) else {
                ui.label("Not in the line any more.");
                return;
            };
            ui.heading(format!("{section} spread {}", spread_name(sec, index)));
            let span = sec
                .span_points
                .iter()
                .find(|p| p.label == l.from)
                .map(|p| p.span);
            if let Some(span) = span
                && ui
                    .link(format!("section {section}, span {}", span + 1))
                    .clicked()
            {
                next = Some(Selection::Span {
                    section: section.clone(),
                    span,
                });
            }
            for (end, point) in [("from", &l.from), ("to", &l.to)] {
                if ui.link(format!("{end} span point {point}")).clicked() {
                    next = Some(Selection::SpanPoint {
                        section: section.clone(),
                        label: point.clone(),
                    });
                }
            }
            let bundle = sec.bundle.max(1);
            let mut rows = vec![
                ("load", format!("{:.2} N/m", l.load)),
                (
                    "on",
                    if l.per_subconductor {
                        "each subconductor".to_owned()
                    } else {
                        "the whole bundle".to_owned()
                    },
                ),
                (
                    "type",
                    l.type_id.clone().unwrap_or_else(|| "own values".into()),
                ),
            ];
            if bundle > 1 {
                rows.push((
                    "whole bundle",
                    format!("{:.2} N/m ({bundle} subconductors)", l.bundle_load(bundle)),
                ));
            }
            if let Some(stretch) = crate::selection::spread_stretch(model, case, &section, index) {
                let length: f64 = stretch
                    .windows(2)
                    .map(|w| {
                        let d = [0, 1, 2].map(|k| w[1][k] - w[0][k]);
                        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                    })
                    .sum();
                rows.push(("length (case shown)", format!("{length:.2} m")));
                rows.push((
                    "load in all (case shown)",
                    format!("{:.0} N", l.bundle_load(bundle) * length),
                ));
            }
            grid(ui, "spread", &rows);
        }
        Selection::JumperPoint { jumper, label } => {
            ui.heading(format!("jumper {jumper} point {label}"));
            let Some(p) = line
                .jumpers
                .iter()
                .find(|j| j.id == jumper)
                .and_then(|j| j.points.iter().find(|p| p.label == label))
            else {
                ui.label("Not in the line any more.");
                return;
            };
            if ui.link(format!("jumper {jumper}")).clicked() {
                next = Some(Selection::Jumper(jumper.clone()));
            }
            description(ui, &p.description);
            let mut rows = vec![("place".to_owned(), p.at.text())];
            if let Some((_, q)) = model
                .jumper_shape(&jumper, case)
                .and_then(|s| s.point_positions.iter().find(|(l, _)| *l == label))
            {
                rows.push(("height (case shown)".to_owned(), format!("{:.2} m", q[2])));
            }
            rows.extend(object_rows(p.object.as_ref()));
            let rows: Vec<(&str, String)> =
                rows.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
            grid(ui, "jumper-point", &rows);
            let tap = ohl_line::Tap::Jumper {
                jumper: jumper.clone(),
                point: label.clone(),
            };
            for j in line.jumpers.iter().filter(|j| j.taps().any(|t| *t == tap)) {
                if ui.link(format!("jumper {} (on this tap)", j.id)).clicked() {
                    next = Some(Selection::Jumper(j.id.clone()));
                }
            }
        }
        Selection::Spacer(id) => {
            ui.heading(format!("spacer {id}"));
            let Some(s) = line.spacers.iter().find(|s| s.id == id) else {
                ui.label("Not in the line any more.");
                return;
            };
            description(ui, &s.description);
            for e in [&s.a, &s.b] {
                if ui
                    .link(format!("{} span point {}", e.section, e.point))
                    .clicked()
                {
                    next = Some(Selection::SpanPoint {
                        section: e.section.clone(),
                        label: e.point.clone(),
                    });
                }
            }
            let mut rows = vec![
                (
                    "assembly",
                    s.insulator.clone().unwrap_or_else(|| "–".into()),
                ),
                (
                    "length",
                    s.length
                        .map_or_else(|| "as strung".into(), |l| format!("{l:.3} m")),
                ),
                ("weight", format!("{:.0} N", s.weight)),
                ("wind area", format!("{:.3} m²", s.wind_area)),
                (
                    "compression limit",
                    s.compression
                        .map_or_else(|| "tension only".into(), |c| format!("{c:.0} N")),
                ),
            ];
            if let Some(x) = crate::selection::spacer_state(model, case, &id) {
                rows.push(("force (case shown)", format!("{:.0} N", x.force)));
                rows.push((
                    "state",
                    if x.over {
                        "over its compression limit".into()
                    } else if x.slack {
                        "slack".into()
                    } else {
                        "ok".into()
                    },
                ));
            }
            grid(ui, "spacer", &rows);
        }
        Selection::String { structure, label } => {
            let Some(st) = line.structure(&structure) else {
                ui.label(format!("{structure}: not in the line"));
                return;
            };
            let Some(x) = st.string(&label) else {
                ui.label(format!("{structure} string {label}: not in the line"));
                return;
            };
            ui.heading(format!("{structure} string {label}"));
            description(ui, &x.description);
            if ui.link(format!("structure {structure}")).clicked() {
                next = Some(Selection::Structure(structure.clone()));
            }
            let c = st.component(&x.wire);
            let set = c.wires.iter().find_map(|&i| st.wires[i].set);
            let hangs = if st.point(&x.top).is_some() {
                format!("{} (a point of the structure)", x.top)
            } else {
                format!("{} (the bottom of another string)", x.top)
            };
            let users: Vec<&str> = line
                .jumpers
                .iter()
                .filter(|j| {
                    j.supports()
                        .iter()
                        .any(|c| c.structure_id(j) == structure && c.wire == x.wire)
                })
                .map(|j| j.id.as_str())
                .collect();
            let wire = match (st.wire(&x.wire).and_then(|w| w.set), users.is_empty()) {
                (Some(s), _) => format!("{} (set {s}: a wire is clamped here)", x.wire),
                (None, false) => format!("{} (holds jumper {})", x.wire, users.join(", ")),
                (None, true) => format!("{} (other strings hang from it)", x.wire),
            };
            let mut rows: Vec<(&str, String)> = vec![
                ("hangs from", hangs),
                ("to wire point", wire),
                (
                    "length",
                    format!("{:.3} m", st.string_length(x).unwrap_or(0.0)),
                ),
                ("weight", format!("{:.0} N", x.weight)),
                ("wind area", format!("{:.3} m²", x.wind_area)),
                (
                    "library label",
                    x.insulator.clone().unwrap_or_else(|| "—".into()),
                ),
                (
                    "clamp",
                    if line.is_dead_end(st, &x.wire) {
                        "strain (a tension string at a dead end)"
                    } else {
                        "suspension"
                    }
                    .into(),
                ),
                (
                    "compression",
                    x.compression
                        .map_or("none: tension only".into(), |c| format!("up to {c:.0} N")),
                ),
            ];
            // Its force where it is drawn (each side of the wind).
            for solved in model.cases.get(case).into_iter().flat_map(|c| c.drawn()) {
                let shapes = solved.shapes.iter().flat_map(|sh| {
                    line.sections
                        .iter()
                        .find(|s| s.id == sh.section)
                        .into_iter()
                        .flat_map(move |sec| sec.points.iter().zip(&sh.strings))
                });
                for (p, strings) in shapes {
                    for y in strings
                        .iter()
                        .filter(|y| p.structure == structure && y.label == label)
                    {
                        rows.push((
                            "force",
                            format!("{:.0} N{}", y.force, if y.slack { " (slack)" } else { "" }),
                        ));
                    }
                }
                for j in &solved.jumpers {
                    for s in j.supports.iter().filter(|s| s.structure == structure) {
                        for y in s.strings.iter().filter(|y| y.label == label) {
                            rows.push(("force", format!("{:.0} N (jumper {})", y.force, j.jumper)));
                        }
                    }
                }
            }
            if let Some(set) = set {
                rows.push(("set", set.to_string()));
            }
            grid(
                ui,
                "string",
                &rows
                    .iter()
                    .map(|(a, b)| (*a, b.clone()))
                    .collect::<Vec<_>>(),
            );
            ui.label(
                egui::RichText::new("Edit tool (or click with it): change it and where it hangs")
                    .small()
                    .weak(),
            );
        }
        Selection::AlignmentPoint { id } => {
            ui.heading(format!("Alignment point {id}"));
            let Some(a) = line.alignment_point(&id).cloned() else {
                ui.label("not in the line");
                return;
            };
            description(ui, &a.description);
            let orders = crate::editors::point_orders(line, &id);
            let on = if orders.is_empty() {
                "none (waiting to be used)".to_owned()
            } else {
                orders
                    .iter()
                    .map(|(a, k)| format!("{a} #{k}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let routes = line.point_routes(&id);
            grid(
                ui,
                "alignment",
                &[
                    ("easting", format!("{:.3} m", a.position[0])),
                    ("northing", format!("{:.3} m", a.position[1])),
                    ("height", format!("{:.3} m", a.position[2])),
                    ("on alignments (place)", on),
                    (
                        "routes",
                        if routes.is_empty() {
                            "none".to_owned()
                        } else {
                            routes.join(", ")
                        },
                    ),
                ],
            );
            match a.structure {
                Some(st) => {
                    if ui.link(format!("tied to structure {st}")).clicked() {
                        next = Some(Selection::Structure(st));
                    }
                }
                None => {
                    ui.label("free (not tied to a structure)");
                }
            }
        }
        Selection::SurveyPoint(k) => {
            ui.heading(format!("Survey point {}", k + 1));
            if let Some(p) = crate::selection::position(model, &Selection::SurveyPoint(k)) {
                grid(
                    ui,
                    "survey",
                    &[
                        ("easting", format!("{:.3} m", p[0])),
                        ("northing", format!("{:.3} m", p[1])),
                        ("height", format!("{:.3} m", p[2])),
                    ],
                );
            }
        }
        Selection::LidarPoint((q, class)) => {
            let class = &class;
            ui.heading("LiDAR point");
            let p = crate::selection::lidar_xyz(model, &q);
            let def = model.scheme.class(*class);
            grid(
                ui,
                "lidar",
                &[
                    ("easting", format!("{:.3} m", p[0])),
                    ("northing", format!("{:.3} m", p[1])),
                    ("height", format!("{:.3} m", p[2])),
                    (
                        "class",
                        format!("{} {}", class, def.map_or("", |d| d.name.as_str())),
                    ),
                    ("roles", def.map_or(String::new(), |d| d.roles.join(", "))),
                    ("scheme", model.scheme.name.clone()),
                ],
            );
            ui.label(
                egui::RichText::new(
                    "As drawn, to the millimetre. Every point drawn can be clicked; farther \
                     away the level of detail draws a share of the cloud. The Edit tool \
                     changes its coordinates and class.",
                )
                .small(),
            );
        }
        Selection::Jumper(id) => {
            let Some(j) = line.jumpers.iter().find(|j| j.id == id) else {
                ui.label(format!("{id}: not in the line"));
                return;
            };
            ui.heading(format!("Jumper {id}"));
            description(ui, &j.description);
            ui.horizontal(|ui| {
                for end in [&j.from, &j.to] {
                    if ui.link(end.to_string()).clicked() {
                        next = Some(match end {
                            ohl_line::JumperEnd::Wire(p) => {
                                Selection::Structure(p.structure.clone())
                            }
                            ohl_line::JumperEnd::Tap {
                                tap: ohl_line::Tap::Span(r),
                            } => Selection::SpanPoint {
                                section: r.section.clone(),
                                label: r.point.clone(),
                            },
                            ohl_line::JumperEnd::Tap {
                                tap: ohl_line::Tap::Jumper { jumper, point },
                            } => Selection::JumperPoint {
                                jumper: jumper.clone(),
                                label: point.clone(),
                            },
                        });
                    }
                }
            });
            // The wire points it passes through (clamps on suspension
            // strings), in order, with the lengths between its clamps.
            if !j.supports().is_empty() {
                let through: Vec<String> = j
                    .supports()
                    .iter()
                    .map(|c| format!("{} on {}", c.wire, c.structure_id(j)))
                    .collect();
                ui.label(format!("passes through {}", through.join(", then ")));
                let pieces = crate::editors::jumper_pieces(j);
                if !pieces.is_empty() {
                    let text: Vec<String> = pieces.iter().map(|l| format!("{l:.2}")).collect();
                    ui.label(
                        egui::RichText::new(format!(
                            "lengths between its clamps: {} m",
                            text.join(" + ")
                        ))
                        .small(),
                    );
                }
            }
            let section = line.jumper_section(j);
            let b = line.jumper_bundle(j);
            // Where each bundle value comes from: the jumper or its section.
            let from = |own: bool| {
                if own {
                    "set on the jumper"
                } else {
                    "from its section"
                }
            };
            grid(
                ui,
                "jumper",
                &[
                    ("wire type", j.cable.clone()),
                    (
                        "section",
                        section.map_or("—".into(), |s| {
                            format!("{} ({} {})", s.id, s.circuit_name, s.phase_label)
                        }),
                    ),
                    (
                        "bundle",
                        match (b.count, b.spacing) {
                            (1, _) => format!("single ({})", from(j.bundle.is_some())),
                            (n, Some(s)) => format!(
                                "{n} × {:.0} mm ({})",
                                s * 1000.0,
                                from(j.bundle.is_some() || j.bundle_spacing.is_some())
                            ),
                            (n, None) => format!("{n}, no spacing ({})", from(j.bundle.is_some())),
                        },
                    ),
                    (
                        "defined by",
                        match (j.length, j.sag) {
                            _ if j.rigid => "rigid: straight between its clamps".into(),
                            _ if j.tension.is_some() => format!(
                                "tension {:.0} N at its higher end at {} °C (a riser, taut)",
                                j.tension.unwrap_or_default(),
                                j.temperature
                            ),
                            (Some(l), _) => format!("length {l:.3} m at {} °C", j.temperature),
                            (_, Some(s)) => format!("sag {s:.2} m at {} °C", j.temperature),
                            _ => "—".into(),
                        },
                    ),
                    (
                        "bending stiffness",
                        match (j.bending_stiffness, model.library.get(&j.cable)) {
                            (Some(0.0), _) => "0 (hinged ends)".into(),
                            (Some(e), _) => format!("{e:.1} N·m² (set on the jumper)"),
                            (None, Some(c)) => format!(
                                "{:.1} N·m² (estimated: {:.2} × the wire's solid-bar {:.0} N·m²{})",
                                ohl_line::jumper::jumper_bending_stiffness(j, c, &line.defaults),
                                j.stiffness_factor
                                    .unwrap_or(line.defaults.jumper_stiffness_factor),
                                c.bending_stiffness_max(),
                                c.bending_stiffness_min().map_or(String::new(), |m| format!(
                                    ", not below its strands slipping, {m:.0} N·m²"
                                ))
                            ),
                            (None, None) => "—".into(),
                        },
                    ),
                    ("leaves its clamps", {
                        let d = line.defaults.jumper_exit_angle_deg;
                        format!(
                            "{:.0}° / {:.0}° below the conductor",
                            j.angle_from_deg.unwrap_or(d),
                            j.angle_to_deg.unwrap_or(d)
                        )
                    }),
                    (
                        "support string",
                        j.support().map_or("none".into(), |s| {
                            format!("{:.2} m, {:.0} N", s.length, s.weight)
                        }),
                    ),
                    (
                        "helping strings",
                        if j.supports().is_empty() {
                            "none".into()
                        } else {
                            j.supports()
                                .iter()
                                .map(|c| {
                                    format!(
                                        "{} {} at {:.2} ({:.2} m)",
                                        c.structure_id(j),
                                        c.wire,
                                        c.at,
                                        c.clamp_length
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("; ")
                        },
                    ),
                ],
            );
            ui.separator();
            ui.label(egui::RichText::new("Weather cases").strong());
            unsolved_note(ui, model, point.solve_all);
            // The case of the copy clicked (else the one shown).
            let shown = model.selection_case.unwrap_or(case);
            case_table(
                ui,
                "jumper-cases-scroll",
                &[
                    "Weather case",
                    "Length [m]",
                    "Sag [m]",
                    "Max T [N]",
                    "Of RTS [%]",
                    "Low z [m]",
                ],
                model.cases.len(),
                |ui, k| {
                    let solved = &model.cases[k];
                    let mut cells: Vec<Cell> = vec![(
                        case_name(
                            &solved.case.name,
                            k == shown,
                            model.selection_case.is_some(),
                        ),
                        None,
                    )];
                    match solved.jumpers.iter().find(|x| x.jumper == id) {
                        Some(js) => {
                            let plain = |v: String| (egui::RichText::new(v), None);
                            cells.push(plain(format!("{:.3}", js.unstressed_length)));
                            cells.push(plain(format!("{:.3}", js.sag)));
                            cells.push(plain(format!("{:.0}", js.max_tension)));
                            // Taut: stretched between its clamps; over the
                            // cable's strength: red.
                            let used = egui::RichText::new(format!(
                                "{:.0}{}",
                                js.strength_use * 100.0,
                                if js.taut { " taut" } else { "" }
                            ));
                            cells.push(if js.strength_use > 1.0 {
                                (used.color(ui.visuals().error_fg_color), None)
                            } else if js.taut {
                                (
                                    used.color(ui.visuals().warn_fg_color),
                                    Some(
                                        "Taut: shorter than its clamps' distance — \
                                         stretched, its pull holding the clamps back",
                                    ),
                                )
                            } else {
                                (used, None)
                            });
                            cells.push(plain(format!("{:.2}", js.low_point[2])));
                        }
                        None => cells.push(unsolved_cell(solved)),
                    }
                    cells
                },
            );
            // Its supports in the case shown: each clamp's strings.
            if let Some(js) = model
                .cases
                .get(case)
                .and_then(|c| c.jumpers.iter().find(|x| x.jumper == id))
                .filter(|js| !js.supports.is_empty())
            {
                ui.separator();
                ui.label(egui::RichText::new("Support strings (case shown)").strong());
                cases_box(ui, "jumper-supports-scroll", |ui| {
                    egui::Grid::new("jumper-supports")
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["clamp", "string", "force (N)", "clamp z (m)"] {
                                ui.label(egui::RichText::new(h).small());
                            }
                            ui.end_row();
                            for (k, s) in js.supports.iter().enumerate() {
                                let clamp = if s.wire.is_empty() {
                                    format!("{}", k + 1)
                                } else {
                                    format!("{} {} {}", k + 1, s.structure, s.wire)
                                };
                                for x in &s.strings {
                                    ui.label(&clamp);
                                    ui.label(if x.label.is_empty() { "—" } else { &x.label });
                                    ui.label(if x.force > 0.0 {
                                        format!("{:.0}", x.force)
                                    } else {
                                        "slack".into()
                                    });
                                    ui.label(format!("{:.2}", s.clamp[2]));
                                    ui.end_row();
                                }
                            }
                        });
                });
            }
            if let Some(why) = model
                .cases
                .get(case)
                .and_then(|c| c.failed.iter().find(|f| f.starts_with(&format!("{id}:"))))
            {
                crate::menu::error_label(ui, why);
            }
        }
        Selection::Span { section, span } => {
            let Some(sec) = line.sections.iter().find(|s| s.id == section) else {
                ui.label(format!("{section}: not in the line"));
                return;
            };
            let (a, b) = (&sec.points[span].structure, &sec.points[span + 1].structure);
            ui.heading(format!("{section} span {}", span + 1));
            description(ui, &sec.description);
            ui.horizontal(|ui| {
                for st in [a, b] {
                    if ui.link(format!("structure {st}")).clicked() {
                        next = Some(Selection::Structure(st.clone()));
                    }
                }
            });
            let phases = line.section_phases().unwrap_or_default();
            let system = phases
                .get(&section)
                .copied()
                .flatten()
                .map(|(k, p)| {
                    let s = &line.systems[k];
                    format!(
                        "{} {} — {} kV {}{}",
                        s.id,
                        s.phases[p].label,
                        s.voltage_kv,
                        match s.kind {
                            ohl_line::electrical::SystemKind::Ac => "AC",
                            ohl_line::electrical::SystemKind::Dc => "DC",
                        },
                        s.frequency_hz.map_or(String::new(), |f| format!(" {f} Hz"))
                    )
                })
                .unwrap_or_else(|| "earthed (no system)".into());
            grid(
                ui,
                "span",
                &[
                    ("wire type", sec.cable.clone()),
                    (
                        "circuit / phase",
                        format!("{} {}", sec.circuit_name, sec.phase_label),
                    ),
                    ("system", system),
                    (
                        "bundle",
                        match (sec.bundle, sec.bundle_spacing) {
                            (1, _) => "single".into(),
                            (n, Some(s)) => format!("{n} × {:.0} mm", s * 1000.0),
                            (n, None) => format!("{n} (no spacing)"),
                        },
                    ),
                    ("spans in section", (sec.points.len() - 1).to_string()),
                    (
                        "local wind",
                        match sec.wind_factor(span) {
                            1.0 => "the weather case's".to_owned(),
                            f => format!("× {f} of the weather case's"),
                        },
                    ),
                    (
                        "routes",
                        model
                            .routes_of()
                            .sections
                            .get(&section)
                            .map_or(String::new(), |r| r.join(", ")),
                    ),
                ],
            );
            if point.editable
                && let Some(a) = assign_circuit_ui(ui, model, &section)
            {
                action = Some(a);
            }
            if point.editable
                && let Some(a) = local_wind_ui(ui, sec, span)
            {
                action = Some(a);
            }
            ui.separator();
            ui.label(egui::RichText::new("Weather cases").strong());
            unsolved_note(ui, model, point.solve_all);
            // The case of the copy clicked (else the one shown); both wind
            // sides shown: the mirror after each case.
            let shown = model
                .selection_case
                .unwrap_or_else(|| model.section_case(&section, case));
            let rows: Vec<(usize, bool, &crate::model::Solved)> = model
                .cases
                .iter()
                .enumerate()
                .flat_map(|(k, c)| c.drawn().enumerate().map(move |(m, d)| (k, m > 0, d)))
                .collect();
            case_table(
                ui,
                "span-cases-scroll",
                &[
                    "Weather case",
                    "H [kN]",
                    "Max T [kN]",
                    "Sag [m]",
                    "Low z [m]",
                ],
                rows.len(),
                |_, i| {
                    let (k, mirrored, solved) = rows[i];
                    let text = if mirrored {
                        format!("{} (wind from the left)", solved.case.name)
                    } else {
                        solved.case.name.clone()
                    };
                    let mut cells: Vec<Cell> = vec![(
                        case_name(
                            &text,
                            k == shown && !mirrored,
                            model.selection_case.is_some(),
                        ),
                        None,
                    )];
                    match solved
                        .shapes
                        .iter()
                        .find(|x| x.section == section)
                        .and_then(|x| x.spans.get(span))
                    {
                        Some(sp) => cells.extend(
                            [
                                format!("{:.2}", sp.horizontal_tension / 1e3),
                                format!("{:.2}", sp.max_tension / 1e3),
                                format!("{:.3}", sp.sag),
                                format!("{:.2}", sp.low_point[2]),
                            ]
                            .map(|v| (egui::RichText::new(v), None)),
                        ),
                        None => cells.push(unsolved_cell(solved)),
                    }
                    cells
                },
            );
            // Not solved in the case of the copy clicked (else the one
            // shown): why, and where its dashed red copy comes from.
            let shown = model
                .selection_case
                .unwrap_or_else(|| model.section_case(&section, case));
            if let Some(d) = model.unsolved_drawn(&section, shown, 0) {
                crate::menu::error_label(
                    ui,
                    format!(
                        "{section} not solved in {}: {} — drawn dashed red: {}",
                        model.cases[shown].case.name, d.reason, d.from
                    ),
                );
            } else if let Some(why) = model.cases.get(case).and_then(|c| {
                c.failed
                    .iter()
                    .find(|f| f.starts_with(&format!("{section}:")))
            }) {
                crate::menu::error_label(ui, why);
            }
        }
    });
    if let Some(n) = next {
        *selection = n;
    }
    action
}

/// Height of the weather case lists in the properties, points: a fixed
/// box, scrolled, so that long lists of cases take little room.
const CASES_HEIGHT: f32 = 180.0;

/// The command putting the conductor path of section `section` into
/// circuit `circuit` (id; none: into no circuit), out of the one it was
/// in: one command (one undo). Refused when the circuit then has two
/// conductor paths of one phase that are not connected
/// ([`ohl_line::Line::assign_to_circuit`]).
pub fn assign_circuit_action(
    model: &Model,
    section: &str,
    circuit: Option<&str>,
) -> Result<crate::edit::Action, String> {
    let stored = model
        .stored_line()
        .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
    let mut line = stored.clone();
    line.adopt_circuits();
    line.assign_to_circuit(&[section.to_owned()], circuit)?;
    let command = ohl_session::line_circuits_change(stored, &line)?;
    Ok(crate::edit::Action::Command {
        what: match circuit {
            Some(c) => format!("conductor path of {section} assigned to circuit {c}"),
            None => format!("conductor path of {section} taken out of its circuit"),
        },
        command,
    })
}

/// "Assign to circuit": a circuit chosen for the selected wire's
/// conductor path (its whole path: connected sections stay together).
fn assign_circuit_ui(
    ui: &mut egui::Ui,
    model: &Model,
    section: &str,
) -> Option<crate::edit::Action> {
    let line = &model.line;
    let now = line
        .sections
        .iter()
        .find(|s| s.id == section)
        .map(|s| s.circuit_name.clone())
        .unwrap_or_default();
    let id = egui::Id::new(("assign-circuit", section));
    let mut chosen: Option<String> = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| {
        line.circuits
            .iter()
            .find(|c| c.name == now)
            .map(|c| c.id.clone())
    });
    let mut out = None;
    ui.horizontal(|ui| {
        let text = |c: &Option<String>| {
            c.as_ref()
                .and_then(|id| line.circuits.iter().find(|x| &x.id == id))
                .map_or("(no circuit)".to_owned(), |x| {
                    if x.id == x.name {
                        x.name.clone()
                    } else {
                        format!("{} ({})", x.name, x.id)
                    }
                })
        };
        egui::ComboBox::from_id_salt(("assign-circuit-box", section))
            .selected_text(text(&chosen))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, "(no circuit)");
                for c in &line.circuits {
                    ui.selectable_value(&mut chosen, Some(c.id.clone()), text(&Some(c.id.clone())));
                }
            });
        if ui
            .button("Assign to circuit…")
            .on_hover_text(
                "Puts this wire's whole conductor path (its connected sections) into the \
                 circuit chosen, out of the one it is in (one undo). A circuit holds one \
                 conductor path per phase: another path of the same phase that is not \
                 connected is refused — connect them (a jumper) to make them one. New \
                 circuits: Line › Sections › Circuits table",
            )
            .clicked()
        {
            ui.data_mut(|d| d.remove::<String>(id.with("error")));
            match assign_circuit_action(model, section, chosen.as_deref()) {
                Ok(a) => out = Some(a),
                Err(e) => {
                    ui.data_mut(|d| d.insert_temp(id.with("error"), e));
                }
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, chosen));
    if let Some(e) = ui.data(|d| d.get_temp::<String>(id.with("error"))) {
        ui.colored_label(ui.visuals().warn_fg_color, e);
    }
    out
}

/// The local wind of span `span` of section `sec` (roadmap O15): a factor
/// on the weather case's wind and the spans it is set on (this one by
/// default, or a run of the section's spans), **Set** making one command
/// (one undo, [`ohl_session::Command::SetWindFactor`]).
fn local_wind_ui(
    ui: &mut egui::Ui,
    sec: &ohl_line::Section,
    span: usize,
) -> Option<crate::edit::Action> {
    // Typed values kept while the span's factor stays (an undo or a
    // table edit starts again from the new one).
    let id = egui::Id::new((
        "local-wind",
        sec.id.as_str(),
        span,
        sec.wind_factor(span).to_bits(),
    ));
    let n = sec.points.len().saturating_sub(1);
    // (factor, first span, last span), spans from 1 as shown.
    let mut v: (f64, usize, usize) =
        ui.data(|d| d.get_temp(id))
            .unwrap_or((sec.wind_factor(span), span + 1, span + 1));
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.label("Local wind ×").on_hover_text(
            "The weather case's wind on these spans times this factor, in every case with \
                 wind (e.g. 1.2 on an exposed ridge, 0.7 in a sheltered valley): the wires' \
                 blowout and tensions, the structure loads, the clearances. 1: the case's wind",
        );
        crate::style::field(
            ui,
            egui::DragValue::new(&mut v.0)
                .range(0.0..=ohl_line::MAX_WIND_FACTOR)
                .speed(0.01)
                .max_decimals(3),
        );
        ui.label("spans");
        crate::style::field(ui, egui::DragValue::new(&mut v.1).range(1..=n));
        ui.label("to");
        crate::style::field(ui, egui::DragValue::new(&mut v.2).range(v.1..=n));
        if ui
            .button("Set")
            .on_hover_text("Gives the spans the factor (one undo)")
            .clicked()
        {
            let (first, last) = (v.1.min(v.2), v.1.max(v.2));
            out = Some(crate::edit::Action::Command {
                what: format!("local wind × {} on {} spans {first} to {last}", v.0, sec.id),
                command: ohl_session::Command::SetWindFactor {
                    section: sec.id.clone(),
                    spans: [first - 1, last - 1],
                    factor: v.0,
                },
            });
        }
    });
    v.2 = v.2.max(v.1);
    ui.data_mut(|d| d.insert_temp(id, v));
    out
}

/// A weather case list in a box of fixed height, scrolled.
pub fn cases_box(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .min_scrolled_height(CASES_HEIGHT)
            .max_height(CASES_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, add);
    });
}

/// The cell of a weather case a section or jumper has no result in:
/// "—" when the case is not solved yet (why: [`unsolved_note`], said once
/// above the table), "failed" when it was solved and this failed.
fn unsolved_cell(solved: &crate::model::Solved) -> Cell {
    if solved.ready {
        (
            egui::RichText::new("failed"),
            Some("Not solved in this weather case: the reason is below the table"),
        )
    } else {
        (
            egui::RichText::new("—"),
            Some("Not solved yet: only the cases shown are solved while editing"),
        )
    }
}

/// Above a weather case table, once (owner: OHL-Design UI review,
/// 2026-10-26): why some cases have no result — only the cases shown are
/// solved while editing — and **Solve all** (`solve_all` set: every case
/// solved in the background).
fn unsolved_note(ui: &mut egui::Ui, model: &Model, solve_all: &mut bool) {
    let pending = model.cases.iter().filter(|c| !c.ready).count();
    if pending == 0 {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Not solved in {pending} weather case{}: only the cases shown are solved \
                 while editing —",
                if pending == 1 { "" } else { "s" }
            ))
            .weak(),
        );
        if !model.solving.is_empty() {
            ui.spinner();
        }
        if ui
            .link("Solve all")
            .on_hover_text(
                "Every weather case solved in the background (the views stay as they are)",
            )
            .clicked()
        {
            *solve_all = true;
        }
    });
}

/// A cell of [`case_table`]: its text, and what its hover says.
type Cell = (egui::RichText, Option<&'static str>);

/// Width of the case column of [`case_table`], points.
const CASE_WIDTH: f32 = 190.0;

/// Width of a value column of [`case_table`], points.
const VALUE_WIDTH: f32 = 68.0;

/// A table of `rows` rows (one per weather case) in a box of its own: the
/// heading `headers` above, the rows scrolled, only those in view laid out
/// (a line with a hundred cases and both wind sides has two hundred);
/// columns of fixed width (the case's, then the values'), so the heading
/// and the rows line up. `row` gives row `k`'s cells.
fn case_table(
    ui: &mut egui::Ui,
    id: &str,
    headers: &[&str],
    rows: usize,
    row: impl Fn(&egui::Ui, usize) -> Vec<Cell>,
) {
    let width = |c: usize| if c == 0 { CASE_WIDTH } else { VALUE_WIDTH };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let height = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
        // The units in the headers ("H [kN]"), the numbers right-aligned
        // under them (owner: OHL-Design UI review, 2026-10-26).
        let cell = |ui: &mut egui::Ui, c: usize, label: egui::Label| {
            let layout = if c == 0 {
                egui::Layout::left_to_right(egui::Align::Center)
            } else {
                egui::Layout::right_to_left(egui::Align::Center)
            };
            ui.allocate_ui_with_layout(egui::vec2(width(c), height), layout, |ui| {
                ui.set_min_size(egui::vec2(width(c), height - ui.spacing().item_spacing.y));
                ui.add(label)
            })
            .inner
        };
        ui.horizontal(|ui| {
            for (c, h) in headers.iter().enumerate() {
                cell(
                    ui,
                    c,
                    egui::Label::new(egui::RichText::new(*h).small().strong()).truncate(),
                );
            }
        });
        egui::ScrollArea::vertical()
            .id_salt(id)
            .min_scrolled_height(CASES_HEIGHT)
            .max_height(CASES_HEIGHT)
            .auto_shrink([false, false])
            .show_rows(ui, height, rows, |ui, range| {
                for k in range {
                    let cells = row(ui, k);
                    ui.horizontal(|ui| {
                        for (c, (text, hover)) in cells.into_iter().enumerate() {
                            let r = cell(ui, c, egui::Label::new(text).truncate());
                            if let Some(h) = hover {
                                r.on_hover_text(h);
                            }
                        }
                    });
                }
            });
    });
}

/// An object's description under its heading (when it has one).
fn description(ui: &mut egui::Ui, text: &str) {
    if !text.trim().is_empty() {
        ui.label(egui::RichText::new(text.trim()).italics());
    }
}

/// A weather case's name in a properties list: the case shown in bold;
/// the case of the copy clicked in a view marked and coloured.
fn case_name(name: &str, this: bool, clicked: bool) -> egui::RichText {
    match (this, clicked) {
        (true, true) => egui::RichText::new(format!("▶ {name}"))
            .strong()
            .color(egui::Color32::from_rgb(255, 210, 90)),
        (true, false) => egui::RichText::new(name).strong(),
        _ => egui::RichText::new(name),
    }
}

/// A spread's name: its label, else its number in the section.
pub fn spread_name(sec: &ohl_line::Section, k: usize) -> String {
    sec.spreads
        .get(k)
        .map(|l| l.label.trim())
        .filter(|l| !l.is_empty())
        .map_or_else(|| format!("#{}", k + 1), str::to_owned)
}

/// An object's values, as rows (none: a bare point).
fn object_rows(o: Option<&ohl_line::LineObject>) -> Vec<(String, String)> {
    let Some(o) = o else {
        return vec![("object".into(), "none (a bare point)".into())];
    };
    let mut rows = vec![
        ("object".to_owned(), o.kind.as_str().to_owned()),
        (
            "type".to_owned(),
            o.type_id.clone().unwrap_or_else(|| "own values".into()),
        ),
        ("weight".to_owned(), format!("{:.1} N", o.weight)),
    ];
    let shape = match o.shape {
        ohl_line::Shape::Point => "point (no wind, no ice)".to_owned(),
        ohl_line::Shape::Sphere { diameter } => format!("sphere, {diameter} m"),
        ohl_line::Shape::Cylinder {
            diameter,
            length,
            axis,
        } => format!("cylinder {diameter} × {length} m, {axis:?}").to_lowercase(),
        ohl_line::Shape::Box {
            along,
            across,
            height,
        } => format!("box {along} × {across} × {height} m"),
        ohl_line::Shape::Areas {
            wind_area,
            ice_area,
        } => format!("wind {wind_area} m², ice {ice_area} m²"),
    };
    rows.push(("shape".to_owned(), shape));
    if let Some(d) = o.drag {
        rows.push(("drag".to_owned(), format!("{d}")));
    }
    if let Some(l) = o.clamp_length {
        rows.push(("clamp length".to_owned(), format!("{l} m")));
    }
    rows
}
