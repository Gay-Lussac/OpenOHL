// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tables' rows: the objects' values as text, the defaults shown, a
//! new row's cells.

use super::*;

/// Shows the values used where an object leaves a value to its default
/// (dimmed; left unset on Apply unless typed in): a section's electrical
/// system from its conductor path and the weather cases it is drawn in, a
/// strain string's estimated weight, a jumper's bundle (from its section),
/// bending stiffness (estimated from its cable) and clamp exit angles.
pub(super) fn defaults(kind: TableKind, model: &Model, case: usize, rows: &mut [Row]) {
    let line = &model.line;
    let set = |row: &mut Row, c: usize, text: String| {
        if row.cells[c].is_empty() || row.defaulted.get(c).copied().unwrap_or(false) {
            row.cells[c] = text;
            row.defaulted.resize(row.cells.len(), false);
            row.defaulted[c] = true;
        }
    };
    match kind {
        TableKind::Sections => {
            // The system of a conductor path: that of any of its sections.
            let mut system: std::collections::HashMap<&str, String> =
                std::collections::HashMap::new();
            for path in ohl_line::electrical::conductor_paths(line) {
                let given = path.iter().find_map(|id| {
                    line.sections
                        .iter()
                        .find(|s| &s.id == id)
                        .and_then(|s| s.system.clone())
                });
                if let Some(g) = given {
                    for id in &path {
                        if let Some(s) = line.sections.iter().find(|s| &s.id == id) {
                            system.insert(s.id.as_str(), g.clone());
                        }
                    }
                }
            }
            let shown: Vec<String> = model
                .shown(case)
                .iter()
                .filter_map(|k| model.cases.get(*k))
                .map(|c| c.case.name.clone())
                .collect();
            for row in rows.iter_mut() {
                set(
                    row,
                    TableKind::Sections.column("system"),
                    system
                        .get(row.key.as_str())
                        .cloned()
                        .unwrap_or_else(|| "earthed".into()),
                );
                set(
                    row,
                    TableKind::Sections.column("display cases"),
                    shown.join("; "),
                );
                // A bundle not turned: 0°.
                if row.cells[5].trim().parse::<u32>().is_ok_and(|n| n > 1) {
                    set(
                        row,
                        TableKind::Sections.column("bundle angle °"),
                        "0".into(),
                    );
                }
            }
        }
        TableKind::Strings => {
            // Weights estimated for strain strings given without one.
            let mut weight: std::collections::HashMap<(&str, String), f64> =
                std::collections::HashMap::new();
            if let Some(solved) = model.cases.get(case) {
                for shape in &solved.shapes {
                    let Some(sec) = line.sections.iter().find(|s| s.id == shape.section) else {
                        continue;
                    };
                    for ((p, w), strings) in sec
                        .points
                        .iter()
                        .zip(&shape.estimated_weights)
                        .zip(&shape.strings)
                    {
                        if let (Some(w), [x]) = (w, strings.as_slice()) {
                            weight.insert((p.structure.as_str(), x.label.clone()), *w);
                        }
                    }
                }
            }
            for row in rows.iter_mut() {
                let (structure, label) = row.key.split_once('\t').unwrap_or((&row.key, ""));
                if let Some(w) = weight.get(&(structure, label.to_owned())).copied() {
                    let w = f(w, 1);
                    row.cells[6].clear();
                    set(row, 6, w);
                }
            }
        }
        TableKind::Jumpers => {
            for row in rows.iter_mut() {
                let Some(j) = line.jumpers.iter().find(|j| j.id == row.key) else {
                    continue;
                };
                let b = line.jumper_bundle(j);
                set(row, 7, b.count.to_string());
                if b.count > 1 {
                    set(row, 8, b.spacing.map_or(String::new(), |v| f(v, 3)));
                }
                if let Some(c) = model.library.get(&j.cable) {
                    set(
                        row,
                        9,
                        f(
                            ohl_line::jumper::jumper_bending_stiffness(j, c, &line.defaults),
                            1,
                        ),
                    );
                }
                set(row, 10, f(line.defaults.jumper_stiffness_factor, 2));
                let angle = f(line.defaults.jumper_exit_angle_deg, 1);
                set(row, 11, angle.clone());
                set(row, 12, angle);
            }
        }
        TableKind::WeatherCases => {
            let q = f(model.line.defaults.air_density_factor, 3);
            for row in rows.iter_mut() {
                set(row, W_Q, q.clone());
            }
        }
        _ => {}
    }
}

impl TableKind {
    pub(super) fn own_rows(self, model: &Model, case: usize) -> Vec<Row> {
        let line = &model.line;
        let mut through: std::collections::HashMap<(&str, u32), usize> =
            std::collections::HashMap::new();
        let mut at_structure: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for s in &line.sections {
            for p in &s.points {
                *through.entry((p.structure.as_str(), p.set)).or_default() += 1;
            }
            let mut ids: Vec<&str> = s.points.iter().map(|p| p.structure.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            for id in ids {
                *at_structure.entry(id).or_default() += 1;
            }
        }
        let on_routes = line.structure_routes();
        let mut rows: Vec<Row> = match self {
            TableKind::Structures => line
                .structures
                .iter()
                .map(|s| Row {
                    // Routes from the alignments: dimmed.
                    defaulted: if s.routes.is_empty() {
                        let mut d = vec![false; STRUCTURES.len()];
                        d[TableKind::Structures.column("routes")] = true;
                        d
                    } else {
                        Vec::new()
                    },
                    key: s.id.clone(),
                    select: Selection::Structure(s.id.clone()),
                    cells: vec![
                        s.id.clone(),
                        s.structure_type.clone().unwrap_or_default(),
                        s.extension.clone().unwrap_or_default(),
                        f(s.orientation_deg, 3),
                        f(s.position[0], 3),
                        f(s.position[1], 3),
                        f(s.position[2], 3),
                        s.sets().count().to_string(),
                        at_structure
                            .get(s.id.as_str())
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                        if s.routes.is_empty() {
                            on_routes.get(&s.id).cloned().unwrap_or_default().join(", ")
                        } else {
                            s.routes.join(", ")
                        },
                        s.description.clone(),
                        s.weather_groups.join("; "),
                        s.soil_resistivity_ohm_m
                            .map(|r| f(r, 3))
                            .unwrap_or_default(),
                    ],
                })
                .collect(),
            TableKind::Routes => {
                let views = line.alignment_views();
                let (sections, _) = line.wire_routes();
                line.routes
                    .iter()
                    .map(|r| {
                        let on: Vec<&str> = views
                            .iter()
                            .filter(|a| a.route == r.id)
                            .map(|a| a.id.as_str())
                            .collect();
                        Row {
                            defaulted: Vec::new(),
                            key: r.id.clone(),
                            select: Selection::None,
                            cells: vec![
                                r.id.clone(),
                                r.name.clone(),
                                on.join(", "),
                                on_routes
                                    .values()
                                    .filter(|rs| rs.contains(&r.id))
                                    .count()
                                    .to_string(),
                                sections
                                    .values()
                                    .filter(|rs| rs.contains(&r.id))
                                    .count()
                                    .to_string(),
                                r.description.clone(),
                            ],
                        }
                    })
                    .collect()
            }
            TableKind::Sections => line
                .sections
                .iter()
                .map(|s| {
                    let (condition, value) = match &s.stringing {
                        Stringing::Tension {
                            horizontal_tension, ..
                        } => ("tension".to_owned(), f(horizontal_tension / 1000.0, 2)),
                        Stringing::Sag { sag, span, .. } => (
                            span.map_or("sag, ruling span".into(), |k| {
                                format!("sag, span {}", k + 1)
                            }),
                            f(*sag, 3),
                        ),
                        Stringing::UnstressedLengths { lengths, .. } => {
                            (format!("{} lengths", lengths.len()), String::new())
                        }
                    };
                    let shapes: Vec<&ohl_line::solve::SectionShape> = model
                        .section_cases(&s.id, case)
                        .into_iter()
                        .filter_map(|k| {
                            model
                                .cases
                                .get(k)?
                                .shapes
                                .iter()
                                .find(|x| x.section == s.id)
                        })
                        .collect();
                    Row {
                        defaulted: Vec::new(),
                        key: s.id.clone(),
                        select: Selection::Span {
                            section: s.id.clone(),
                            span: 0,
                        },
                        cells: vec![
                            s.id.clone(),
                            s.cable.clone(),
                            s.circuit_name.clone(),
                            s.phase_label.clone(),
                            s.system.clone().unwrap_or_default(),
                            s.bundle.to_string(),
                            s.bundle_spacing.map_or(String::new(), |v| f(v, 3)),
                            s.bundle_rotation_deg.map_or(String::new(), |v| f(v, 1)),
                            if s.stringing.clipped() {
                                "clipped in".into()
                            } else {
                                "in the blocks".into()
                            },
                            condition,
                            value,
                            f(s.stringing.temperature(), 1),
                            s.points.iter().map(point).collect::<Vec<_>>().join(", "),
                            model.display_cases.get(&s.id).map_or(String::new(), |ks| {
                                ks.iter()
                                    .filter_map(|k| model.cases.get(*k))
                                    .map(|c| c.case.name.clone())
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            }),
                            // One value per display case; a case where a
                            // span's tension exceeds the wire's RTS says so
                            // (owner, 2026-10-22).
                            shapes
                                .iter()
                                .filter_map(|x| {
                                    let sp = x.spans.first()?;
                                    let over = if ohl_line::solve::above_rts(x) {
                                        " (above RTS)"
                                    } else {
                                        ""
                                    };
                                    Some(format!("{}{over}", f(sp.horizontal_tension / 1000.0, 2)))
                                })
                                .collect::<Vec<_>>()
                                .join(" / "),
                            shapes
                                .iter()
                                .map(|x| f(x.spans.iter().map(|sp| sp.sag).fold(0.0, f64::max), 3))
                                .collect::<Vec<_>>()
                                .join(" / "),
                            s.calculation.label().to_owned(),
                            s.color.clone().unwrap_or_default(),
                            s.description.clone(),
                            s.weather_groups.join("; "),
                            s.wind_factors_text(),
                        ],
                    }
                })
                .collect(),
            TableKind::Strings => line
                .structures
                .iter()
                .flat_map(|s| {
                    let through = &through;
                    s.strings.iter().map(move |x| {
                        let sets: Vec<u32> = s
                            .component(&x.wire)
                            .wires
                            .iter()
                            .filter_map(|&i| s.wires[i].set)
                            .collect();
                        let dead_end = line.is_dead_end(s, &x.wire);
                        // Limits its insulator gives (not its own): dimmed.
                        let own = model
                            .stored_line()
                            .and_then(|l| l.structure(&s.id))
                            .and_then(|o| o.string(&x.label))
                            .map(|o| o.limits());
                        let (l, given) = (x.limits(), own.unwrap_or(x.limits()));
                        let inherited = [
                            l.max_load.is_some() && given.max_load.is_none(),
                            l.max_swing_deg.is_some() && given.max_swing_deg.is_none(),
                            l.max_swing_across_deg.is_some()
                                && given.max_swing_across_deg.is_none(),
                            l.max_swing_along_deg.is_some() && given.max_swing_along_deg.is_none(),
                        ];
                        let limit = |v: Option<f64>, d: usize| v.map_or(String::new(), |v| f(v, d));
                        Row {
                            // Not flagged: from where it is.
                            defaulted: (0..17)
                                .map(|c| {
                                    (c == 9 && !x.strain)
                                        || (11..15).contains(&c) && inherited[c - 11]
                                })
                                .collect(),
                            key: format!("{}\t{}", s.id, x.label),
                            select: match sets.first() {
                                Some(&set) => Selection::Attachment {
                                    structure: s.id.clone(),
                                    set,
                                },
                                None => Selection::Structure(s.id.clone()),
                            },
                            cells: vec![
                                s.id.clone(),
                                x.label.clone(),
                                x.top.clone(),
                                x.wire.clone(),
                                sets.iter()
                                    .map(u32::to_string)
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                s.string_length(x).map_or(String::new(), |l| f(l, 3)),
                                f(x.weight, 1),
                                x.insulator.clone().unwrap_or_default(),
                                x.compression.map_or(String::new(), |c| f(c, 0)),
                                if dead_end { "strain" } else { "suspension" }.into(),
                                if x.wind_area > 0.0 {
                                    f(x.wind_area, 3)
                                } else {
                                    String::new()
                                },
                                limit(l.max_load, 0),
                                limit(l.max_swing_deg, 2),
                                limit(l.max_swing_across_deg, 2),
                                limit(l.max_swing_along_deg, 2),
                                sets.iter()
                                    .map(|set| {
                                        through.get(&(s.id.as_str(), *set)).copied().unwrap_or(0)
                                    })
                                    .sum::<usize>()
                                    .to_string(),
                                x.description.clone(),
                            ],
                        }
                    })
                })
                .collect(),
            TableKind::Jumpers => line
                .jumpers
                .iter()
                .map(|j| Row {
                    defaulted: Vec::new(),
                    key: j.id.clone(),
                    select: Selection::Jumper(j.id.clone()),
                    cells: vec![
                        j.id.clone(),
                        end_text(&j.from),
                        end_text(&j.to),
                        j.cable.clone(),
                        j.length.map_or(String::new(), |v| f(v, 3)),
                        j.sag.map_or(String::new(), |v| f(v, 3)),
                        f(j.temperature, 1),
                        j.bundle.map_or(String::new(), |v| v.to_string()),
                        j.bundle_spacing.map_or(String::new(), |v| f(v, 3)),
                        j.bending_stiffness.map_or(String::new(), |v| f(v, 1)),
                        j.stiffness_factor.map_or(String::new(), |v| f(v, 2)),
                        j.angle_from_deg.map_or(String::new(), |v| f(v, 1)),
                        j.angle_to_deg.map_or(String::new(), |v| f(v, 1)),
                        if j.rigid { "yes" } else { "no" }.into(),
                        clamps_text(j),
                        j.description.clone(),
                    ],
                })
                .collect(),
            TableKind::Systems => line
                .systems
                .iter()
                .enumerate()
                .map(|(k, s)| Row {
                    defaulted: (0..9)
                        .map(|c| {
                            (c == 6 && s.phases.iter().all(|p| p.code.is_none()))
                                || (c == 7 && s.color.is_none())
                        })
                        .collect(),
                    key: s.id.clone(),
                    select: Selection::None,
                    cells: vec![
                        s.id.clone(),
                        s.name.clone(),
                        match s.kind {
                            ohl_line::electrical::SystemKind::Ac => "AC".into(),
                            ohl_line::electrical::SystemKind::Dc => "DC".into(),
                        },
                        f(s.voltage_kv, 1),
                        s.frequency_hz.map_or(String::new(), |v| f(v, 1)),
                        s.phases
                            .iter()
                            .map(|p| p.label.clone())
                            .collect::<Vec<_>>()
                            .join(" "),
                        (0..s.phases.len())
                            .map(|p| s.line_code(p))
                            .collect::<Vec<_>>()
                            .join(" "),
                        line.system_color(k),
                        line.sections
                            .iter()
                            .filter(|x| x.system.as_deref() == Some(s.id.as_str()))
                            .count()
                            .to_string(),
                        s.description.clone(),
                        s.owner.clone(),
                        s.number.clone(),
                    ],
                })
                .collect(),
            TableKind::ConductorPaths => circuit_rows(line),
            TableKind::Circuits => circuit_list_rows(line),
            TableKind::Alignments => line
                .alignments
                .iter()
                .enumerate()
                .map(|(k, a)| Row {
                    defaulted: Vec::new(),
                    key: a.id.clone(),
                    select: a.points.first().map_or(Selection::None, |p| {
                        Selection::AlignmentPoint { id: p.clone() }
                    }),
                    cells: vec![
                        a.id.clone(),
                        a.name.clone(),
                        a.route.clone(),
                        (k + 1).to_string(),
                        a.points.len().to_string(),
                        a.description.clone(),
                    ],
                })
                .collect(),
            TableKind::AlignmentPoints => point_rows(line),
            TableKind::Insulators => insulator_rows(model),
            TableKind::StringShapes => string_shape_rows(model),
            TableKind::WeatherCases => model
                .cases
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    let w = &c.case;
                    Row {
                        defaulted: Vec::new(),
                        key: k.to_string(),
                        select: Selection::None,
                        cells: {
                            let mut c = vec![String::new(); WEATHER.len()];
                            c[W_ORDER] = (k + 1).to_string();
                            c[W_NAME] = w.name.clone();
                            c[W_DESCRIPTION] = w.description.clone();
                            c[W_TAGS] = w.tags.join(", ");
                            c[W_Q] = w.air_density_factor.map_or(String::new(), |q| f(q, 3));
                            c[W_VELOCITY] = f(
                                w.wind_velocity_with(model.line.defaults.air_density_factor),
                                2,
                            );
                            c[W_PRESSURE] = f(w.wind_pressure.si(), 1);
                            c[W_ICE] = f(w.ice_thickness.si() * 1000.0, 1);
                            c[W_ICE_DENSITY] =
                                w.ice_density.map_or(String::new(), |v| f(v.si(), 0));
                            c[W_ICE_LOAD] = w.ice_load.map_or(String::new(), |v| f(v.si(), 2));
                            c[W_TEMP] = f(w.temperature.celsius(), 1);
                            c[W_AMBIENT] = w
                                .ambient_temperature
                                .map_or(String::new(), |t| f(t.celsius(), 1));
                            c
                        },
                    }
                })
                .collect(),
            TableKind::Spacers => spacer_rows(model, case),
            TableKind::LoadTypes => load_type_rows(model),
            TableKind::SpanObjects => span_object_rows(model),
        };
        defaults(self, model, case, &mut rows);
        rows
    }

    /// A new row's cells, before anything is typed.
    pub fn new_row(self, model: &Model, case: usize, added: &[Vec<String>]) -> Vec<String> {
        let line = &model.line;
        let n = self.columns().len();
        let mut cells = vec![String::new(); n];
        let taken_after = |ids: Vec<&str>, like: &str| {
            let added_ids: Vec<&str> = added.iter().map(|r| r[0].as_str()).collect();
            crate::editors::next_id(ids.into_iter().chain(added_ids), like)
        };
        match self {
            TableKind::Structures => {
                cells[0] = taken_after(
                    line.structures.iter().map(|s| s.id.as_str()).collect(),
                    line.structures.last().map_or("T0", |s| s.id.as_str()),
                );
                cells[3] = "0".into();
            }
            TableKind::Sections => {
                cells[0] = taken_after(
                    line.sections.iter().map(|s| s.id.as_str()).collect(),
                    line.sections.last().map_or("S0", |s| s.id.as_str()),
                );
                let like = SectionForm::new_through(model, case, Vec::new(), None);
                cells[1] = like.cable;
                cells[2] = like.circuit;
                cells[3] = like.phase_label;
                cells[4] = like.system.unwrap_or_default();
                cells[5] = like.bundle.to_string();
                cells[6] = f(like.spacing, 3);
                let col = |n: &str| TableKind::Sections.column(n);
                cells[col("status")] = "in the blocks".into();
                cells[col("calculation")] = ohl_line::condition::Calculation::default().label();
                cells[col("condition")] = "tension".into();
                cells[col("value")] = "20".into();
                cells[col("temp °C")] = "15".into();
            }
            TableKind::Strings => {
                cells[9] = "suspension".into();
            }
            TableKind::Jumpers => {
                cells[0] = taken_after(
                    line.jumpers.iter().map(|j| j.id.as_str()).collect(),
                    line.jumpers.last().map_or("J0", |j| j.id.as_str()),
                );
                cells[6] = "15".into();
                cells[13] = "no".into();
            }
            // Conductor paths follow the connections: no rows of their own.
            TableKind::ConductorPaths => {}
            TableKind::Circuits => {
                cells[0] = taken_after(line.circuits.iter().map(|c| c.id.as_str()).collect(), "C0");
                if !line.circuits.iter().any(|c| c.name == cells[0]) {
                    cells[1] = cells[0].clone();
                }
            }
            TableKind::Routes => {
                let mut taken = line.clone();
                for r in added {
                    taken.routes.push(ohl_line::Route::new(&r[0], ""));
                }
                cells[0] = crate::editors::free_route_id(&taken);
            }
            TableKind::Systems => {
                cells[0] = taken_after(
                    line.systems.iter().map(|s| s.id.as_str()).collect(),
                    "system-0",
                );
                cells[2] = "AC".into();
                cells[4] = "50".into();
                cells[5] = "L1 L2 L3".into();
            }
            TableKind::Alignments => {
                let mut taken = line.clone();
                for r in added {
                    taken
                        .alignments
                        .push(ohl_line::Alignment::new(&r[0], "", Vec::new()));
                }
                cells[0] = crate::editors::free_alignment_id(&taken);
                // The route worked on, else the only one.
                cells[2] = model
                    .work_route
                    .clone()
                    .filter(|w| line.routes.iter().any(|r| &r.id == w))
                    .or_else(|| match &line.routes[..] {
                        [only] => Some(only.id.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                cells[3] = (line.alignments.len() + added.len() + 1).to_string();
            }
            TableKind::AlignmentPoints => {
                // At the end of the first alignment, a new point id.
                let mut taken = line.clone();
                for r in added {
                    taken
                        .alignment_points
                        .push(ohl_line::AlignmentPoint::free(&r[2], [0.0; 3]));
                }
                if let Some(a) = line.alignments.first() {
                    let more = added.iter().filter(|r| r[0] == a.id).count();
                    cells[0] = a.id.clone();
                    cells[1] = (a.points.len() + more + 1).to_string();
                }
                cells[2] = taken.new_point_id();
            }
            TableKind::Insulators => {
                let ids: Vec<&str> = model
                    .types
                    .as_ref()
                    .map(|(_, i)| i.iter().map(|a| a.id.as_str()).collect())
                    .unwrap_or_default();
                cells[0] = taken_after(ids, "string-1");
                cells[1] = "Suspension string".into();
                cells[2] = "suspension".into();
                cells[3] = "3".into();
                cells[4] = "0".into();
                cells[5] = "0".into();
                cells[6] = "50000000".into();
                cells[TableKind::Insulators.column("strings")] = "1".into();
            }
            TableKind::StringShapes => {
                // On the first assembly, a free name; a sphere at the
                // middle (to be set).
                let first = model
                    .types
                    .as_ref()
                    .and_then(|(_, i)| i.iter().next())
                    .map(|a| (a.id.clone(), a.volumes.shapes.clone()));
                if let Some((id, shapes)) = first {
                    let more = added.iter().filter(|r| r[0] == id).count();
                    let names: Vec<String> = shapes
                        .iter()
                        .map(|s| s.name.clone())
                        .chain(added.iter().filter(|r| r[0] == id).map(|r| r[1].clone()))
                        .collect();
                    cells[0] = id;
                    cells[1] = (shapes.len() + more + 1..)
                        .map(|k| format!("shape {k}"))
                        .find(|n| !names.contains(n))
                        .expect("a free name");
                }
                cells[2] = "sphere".into();
                cells[3] = "electrified".into();
                cells[4] = "0.5".into();
                cells[5] = "0.2".into();
            }
            TableKind::WeatherCases => {
                cells[W_ORDER] = (model.cases.len() + added.len() + 1).to_string();
                cells[W_NAME] = format!("new case {}", added.len() + 1);
                cells[W_TEMP] = "15".into();
                cells[W_ICE] = "0".into();
                cells[W_PRESSURE] = "0".into();
            }
            // Added with tools in the views.
            TableKind::Spacers | TableKind::SpanObjects => {}
            TableKind::LoadTypes => {
                cells[1] = "project".into();
                cells[3] = "body".into();
                cells[5] = "point".into();
            }
        }
        cells
    }
}
