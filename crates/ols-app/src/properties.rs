// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The properties of the selection: a structure's data and attachments
//! with the sections through it, a span's section, electrical system and
//! results in every weather case.

use eframe::egui;

use crate::model::Model;
use crate::selection::Selection;

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
pub fn ui(
    ui: &mut egui::Ui,
    model: &Model,
    case: usize,
    selection: &mut Selection,
    position: Option<&mut Option<(String, [f64; 3])>>,
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
            ui.heading(format!("Structure {id}"));
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
                    ("routes", line.routes_of(&id).join(", ")),
                ],
            );
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
                        ui.add(
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
                            kind: ols_session::Kind::Structure,
                            id: id.clone(),
                            confirm: false,
                        });
                    }
                });
            }
            if let Some(t) = &s.structure_type
                && ui
                    .button("Edit pylon…")
                    .on_hover_text(format!(
                        "Open type {t} in the pylon workspace (every structure of the type \
                         follows when saved)"
                    ))
                    .clicked()
            {
                action = Some(crate::edit::Action::EditType { id: t.clone() });
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
                    let on: Vec<&ols_line::InsulatorString> = s.strings_to(&w.label).collect();
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
                            "{} span {} ({} {}): {} → {}",
                            sec.id,
                            k + 1,
                            sec.circuit,
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
            let jumpers: Vec<&ols_line::Jumper> = line
                .jumpers
                .iter()
                .filter(|jp| jp.from.structure == id || jp.to.structure == id)
                .collect();
            if !jumpers.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Jumpers at this structure").strong());
                for jp in jumpers {
                    let text = format!("{}: set {} → set {}", jp.id, jp.from.set, jp.to.set);
                    if ui.link(text).clicked() {
                        next = Some(Selection::Jumper(jp.id.clone()));
                    }
                }
            }
        }
        Selection::Attachment { structure, set } => {
            let at = ols_line::SectionPoint::new(structure.clone(), set);
            let Some(st) = line.structure(&structure) else {
                ui.label(format!("{structure}: not in the line"));
                return;
            };
            let Some(w) = st.wire_at_set(set) else {
                ui.label(format!("{structure} set {set}: not in the line"));
                return;
            };
            ui.heading(format!("{structure} set {set}"));
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
                        "{}: {} → {}, {:.3} m, {:.0} N{}{}{}",
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
            grid(ui, "attachment", &rows.iter().map(|(a, b)| (*a, b.clone())).collect::<Vec<_>>());
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
                                sec.circuit,
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
                    j.supports
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
                    "insulator",
                    x.insulator.clone().unwrap_or_else(|| "—".into()),
                ),
                (
                    "dead end",
                    if x.strain { "yes (tension string)" } else { "no (suspension)" }.into(),
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
                    for y in strings.iter().filter(|y| p.structure == structure && y.label == label)
                    {
                        rows.push(("force", format!("{:.0} N{}", y.force, if y.slack { " (slack)" } else { "" })));
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
            grid(ui, "string", &rows.iter().map(|(a, b)| (*a, b.clone())).collect::<Vec<_>>());
            ui.label(
                egui::RichText::new("Edit tool (or right-click with it): change it and where it hangs")
                    .small()
                    .weak(),
            );
        }
        Selection::AlignmentPoint { route, index } => {
            ui.heading(format!("Route {route}, alignment point {}", index + 1));
            let routes = line.effective_routes();
            let r = routes.iter().find(|r| r.id == route);
            let tied = r.and_then(|r| {
                if r.alignment.is_empty() {
                    // No alignment: the points are the route's structures.
                    r.structures.get(index).cloned()
                } else {
                    r.alignment.get(index).and_then(|a| a.structure.clone())
                }
            });
            match crate::selection::alignment_point(model, &route, index) {
                Some(p) => grid(
                    ui,
                    "alignment",
                    &[
                        ("easting", format!("{:.3} m", p[0])),
                        ("northing", format!("{:.3} m", p[1])),
                        ("height", format!("{:.3} m", p[2])),
                        (
                            "points in the route",
                            r.map_or(0, |r| r.centreline(line).len()).to_string(),
                        ),
                    ],
                ),
                None => {
                    ui.label("not in the line");
                }
            }
            match tied {
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
        Selection::LidarPoint(k) => {
            ui.heading(format!("LiDAR point {k}"));
            let Some((q, class)) = model.points.get(k) else {
                ui.label("not loaded");
                return;
            };
            let p = crate::selection::lidar_xyz(model, q);
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
                    "Shown points are a thinned subset of the cloud (see the Model tab).",
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
            ui.horizontal(|ui| {
                for end in [&j.from, &j.to] {
                    if ui
                        .link(format!("{} set {}", end.structure, end.set))
                        .clicked()
                    {
                        next = Some(Selection::Structure(end.structure.clone()));
                    }
                }
            });
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
                    ("cable", j.cable.clone()),
                    (
                        "section",
                        section.map_or("—".into(), |s| {
                            format!("{} ({} {})", s.id, s.circuit, s.phase_label)
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
                                "{:.1} N·m² (estimated: {:.2} × the cable's solid-bar {:.0} N·m²{})",
                                ols_line::jumper::jumper_bending_stiffness(j, c),
                                j.stiffness_factor
                                    .unwrap_or(ols_line::jumper::DEFAULT_STIFFNESS_FACTOR),
                                c.bending_stiffness_max(),
                                c.bending_stiffness_min().map_or(String::new(), |m| format!(
                                    ", not below its strands slipping, {m:.0} N·m²"
                                ))
                            ),
                            (None, None) => "—".into(),
                        },
                    ),
                    ("leaves its clamps", {
                        let d = ols_line::jumper::DEFAULT_TERMINAL_ANGLE_DEG;
                        format!(
                            "{:.0}° / {:.0}° below the conductor",
                            j.angle_from_deg.unwrap_or(d),
                            j.angle_to_deg.unwrap_or(d)
                        )
                    }),
                    (
                        "support string",
                        j.support.as_ref().map_or("none".into(), |s| {
                            format!("{:.2} m, {:.0} N", s.length, s.weight)
                        }),
                    ),
                    (
                        "helping strings",
                        if j.supports.is_empty() {
                            "none".into()
                        } else {
                            j.supports
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
            cases_box(ui, "jumper-cases-scroll", |ui| {
                egui::Grid::new("jumper-cases")
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["case", "length (m)", "sag (m)", "max T (N)", "low z (m)"] {
                            ui.label(egui::RichText::new(h).small());
                        }
                        ui.end_row();
                        for (k, solved) in model.cases.iter().enumerate() {
                            let name = if k == case {
                                egui::RichText::new(&solved.case.name).strong()
                            } else {
                                egui::RichText::new(&solved.case.name)
                            };
                            ui.label(name);
                            match solved.jumpers.iter().find(|x| x.jumper == id) {
                                Some(js) => {
                                    ui.label(format!("{:.3}", js.unstressed_length));
                                    ui.label(format!("{:.3}", js.sag));
                                    ui.label(format!("{:.0}", js.max_tension));
                                    ui.label(format!("{:.2}", js.low_point[2]));
                                }
                                None => {
                                    ui.label("not solved");
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
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
                            ols_line::electrical::SystemKind::Ac => "AC",
                            ols_line::electrical::SystemKind::Dc => "DC",
                        },
                        s.frequency_hz.map_or(String::new(), |f| format!(" {f} Hz"))
                    )
                })
                .unwrap_or_else(|| "earthed (no system)".into());
            grid(
                ui,
                "span",
                &[
                    ("cable", sec.cable.clone()),
                    (
                        "circuit / phase",
                        format!("{} {}", sec.circuit, sec.phase_label),
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
                ],
            );
            ui.separator();
            ui.label(egui::RichText::new("Weather cases").strong());
            cases_box(ui, "span-cases-scroll", |ui| {
                egui::Grid::new("cases").striped(true).show(ui, |ui| {
                    for h in ["case", "H (kN)", "max T (kN)", "sag (m)", "low z (m)"] {
                        ui.label(egui::RichText::new(h).small());
                    }
                    ui.end_row();
                    let shown = model.section_case(&section, case);
                    // Both wind sides shown: the mirror after each case.
                    let rows =
                        model.cases.iter().enumerate().flat_map(|(k, c)| {
                            c.drawn().enumerate().map(move |(m, d)| (k, m > 0, d))
                        });
                    for (k, mirrored, solved) in rows {
                        let text = if mirrored {
                            format!("{} (wind from the left)", solved.case.name)
                        } else {
                            solved.case.name.clone()
                        };
                        let name = if k == shown && !mirrored {
                            egui::RichText::new(text).strong()
                        } else {
                            egui::RichText::new(text)
                        };
                        ui.label(name);
                        match solved
                            .shapes
                            .iter()
                            .find(|x| x.section == section)
                            .and_then(|x| x.spans.get(span))
                        {
                            Some(sp) => {
                                ui.label(format!("{:.2}", sp.horizontal_tension / 1e3));
                                ui.label(format!("{:.2}", sp.max_tension / 1e3));
                                ui.label(format!("{:.3}", sp.sag));
                                ui.label(format!("{:.2}", sp.low_point[2]));
                            }
                            None => {
                                ui.label("not solved");
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(why) = model.cases.get(case).and_then(|c| {
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
