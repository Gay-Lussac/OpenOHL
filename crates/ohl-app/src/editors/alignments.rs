// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The alignment point window, and alignments edited: points inserted,
//! rerouted, new routes.

use super::*;

/// An alignment point being changed.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentForm {
    /// Its id.
    pub id: String,
    /// The alignments it is on, each with its place there (from 1).
    pub orders: Vec<(String, usize)>,
    /// Position.
    pub position: [f64; 3],
    /// Tied under this structure (it follows it).
    pub structure: Option<String>,
    /// The point sits on the ground.
    pub on_ground: bool,
    /// What it is, in words.
    pub description: String,
}

/// The command writing `alignments` (the line's, edited) where they
/// differ from the stored line; those gone are deleted (their points
/// stay). One command (a batch when several).
pub fn alignments_command(
    stored: &Line,
    alignments: &[ohl_line::Alignment],
) -> Result<Command, String> {
    let mut commands = Vec::new();
    // Reordered: every alignment put at its place.
    let kept: Vec<&str> = alignments
        .iter()
        .map(|a| a.id.as_str())
        .filter(|id| stored.alignment_by_id(id).is_some())
        .collect();
    let before: Vec<&str> = stored
        .alignments
        .iter()
        .map(|a| a.id.as_str())
        .filter(|id| kept.contains(id))
        .collect();
    // …or a new one put before an existing one.
    let inserted = alignments
        .iter()
        .position(|a| stored.alignment_by_id(&a.id).is_none())
        .is_some_and(|k| k < kept.len());
    let reordered = kept != before || inserted;
    for (k, a) in alignments.iter().enumerate() {
        let same = stored.alignment_by_id(&a.id);
        if same == Some(a) && !reordered {
            continue;
        }
        commands.push(Command::Put {
            kind: Kind::Alignment,
            id: a.id.clone(),
            value: serde_json::to_value(a).map_err(|e| e.to_string())?,
            ord: (same.is_none() || reordered).then_some(k as i64),
        });
    }
    for a in &stored.alignments {
        if !alignments.iter().any(|x| x.id == a.id) {
            commands.push(Command::Delete {
                kind: Kind::Alignment,
                id: a.id.clone(),
                confirm: true,
            });
        }
    }
    Ok(match commands.len() {
        1 => commands.remove(0),
        _ => Command::Batch { commands },
    })
}

/// The alignments of `stored` with point `id` on exactly those of
/// `orders`, each at its place there (from 1; past the end: last); the
/// other points keep their order.
pub fn alignments_with_point(
    stored: &Line,
    id: &str,
    orders: &[(String, usize)],
) -> Vec<ohl_line::Alignment> {
    let mut alignments = stored.alignments.clone();
    for a in &mut alignments {
        a.points.retain(|p| p != id);
        if let Some((_, order)) = orders.iter().find(|(x, _)| *x == a.id) {
            let at = order.max(&1) - 1;
            a.points.insert(at.min(a.points.len()), id.to_owned());
        }
    }
    alignments
}

/// The place (index) along `points` where a point at `at` fits best: the
/// one adding the least length (the ends included).
pub fn best_place(points: &[[f64; 3]], at: [f64; 3]) -> usize {
    let d = |a: [f64; 3], b: [f64; 3]| (a[0] - b[0]).hypot(a[1] - b[1]);
    if points.is_empty() {
        return 0;
    }
    let mut best = (d(at, points[0]), 0);
    let n = points.len();
    let end = d(points[n - 1], at);
    if end < best.0 {
        best = (end, n);
    }
    for k in 0..n - 1 {
        let added = d(points[k], at) + d(at, points[k + 1]) - d(points[k], points[k + 1]);
        if added < best.0 {
            best = (added, k + 1);
        }
    }
    best.1
}

/// The command writing point `point` (new or changed) and the alignments
/// it is on (`orders`: alignment, place from 1). One command.
pub fn point_command(
    stored: &Line,
    point: &ohl_line::AlignmentPoint,
    orders: &[(String, usize)],
) -> Result<Command, String> {
    let mut commands = Vec::new();
    if stored.alignment_point(&point.id) != Some(point) {
        commands.push(Command::Put {
            kind: Kind::AlignmentPoint,
            id: point.id.clone(),
            value: serde_json::to_value(point).map_err(|e| e.to_string())?,
            ord: None,
        });
    }
    let alignments = alignments_with_point(stored, &point.id, orders);
    match alignments_command(stored, &alignments)? {
        Command::Batch { commands: c } => commands.extend(c),
        c => commands.push(c),
    }
    Ok(match commands.len() {
        1 => commands.remove(0),
        _ => Command::Batch { commands },
    })
}

/// The id a new alignment takes: `alignment 1`, `alignment 2`… (the
/// first free).
pub fn free_alignment_id(line: &Line) -> String {
    (1..)
        .map(|n| format!("alignment {n}"))
        .find(|id| !line.alignments.iter().any(|a| &a.id == id))
        .expect("a free name")
}

/// The id a new route takes: `route 1`, `route 2`… (the first free).
pub fn free_route_id(line: &Line) -> String {
    (1..)
        .map(|n| format!("route {n}"))
        .find(|id| !line.routes.iter().any(|r| &r.id == id))
        .expect("a free name")
}

/// A new route `id` named `name`: the command, what it does.
pub fn new_route(stored: &Line, id: &str, name: &str) -> Result<(String, Command), String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("a route needs a name".into());
    }
    if stored.routes.iter().any(|r| r.id == id) {
        return Err(format!("there is a route {id} already"));
    }
    let r = ohl_line::Route::new(id, name.trim());
    Ok((
        format!("new route {id}"),
        Command::Put {
            kind: Kind::Route,
            id: id.to_owned(),
            value: serde_json::to_value(&r).map_err(|e| e.to_string())?,
            ord: Some(stored.routes.len() as i64),
        },
    ))
}

/// An alignment's stored points, in order.
pub fn stored_alignment(stored: &Line, id: &str) -> Option<Vec<ohl_line::AlignmentPoint>> {
    stored.alignment_by_id(id).map(|a| stored.points_of(a))
}

/// The places (from 1) point `id` has on each alignment of `line`.
pub fn point_orders(line: &Line, id: &str) -> Vec<(String, usize)> {
    line.alignments
        .iter()
        .filter_map(|a| {
            let k = a.points.iter().position(|p| p == id)?;
            Some((a.id.clone(), k + 1))
        })
        .collect()
}

impl AlignmentForm {
    /// Alignment point `id`.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let stored = model.stored_line().unwrap_or(&model.line);
        let p = stored.alignment_point(id)?.clone();
        Some(Self {
            id: p.id.clone(),
            orders: point_orders(stored, id),
            position: p.position,
            structure: p.structure,
            on_ground: false,
            description: p.description,
        })
    }

    pub(super) fn command(&self, model: &Model) -> Result<(String, Command), String> {
        let stored = model
            .stored_line()
            .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
        let old = stored
            .alignment_point(&self.id)
            .ok_or_else(|| format!("no alignment point {}", self.id))?;
        let position = match &self.structure {
            Some(s) => {
                model
                    .line
                    .structure(s)
                    .ok_or_else(|| format!("no structure {s}"))?
                    .position
            }
            None => self.position,
        };
        let point = ohl_line::AlignmentPoint {
            id: old.id.clone(),
            description: self.description.trim().to_owned(),
            position,
            structure: self.structure.clone(),
        };
        let what = if point_orders(stored, &self.id) == self.orders {
            format!("changed alignment point {}", self.id)
        } else {
            let on: Vec<String> = self
                .orders
                .iter()
                .map(|(a, k)| format!("{a} #{k}"))
                .collect();
            if on.is_empty() {
                format!("alignment point {} now on no alignment", self.id)
            } else {
                format!("alignment point {} now on {}", self.id, on.join(", "))
            }
        };
        Ok((what, point_command(stored, &point, &self.orders)?))
    }
}

/// Alignment point `p` (new without an id, else existing: shared) put in
/// the nearest segment of the nearest alignment. The command, and the
/// point's id.
pub fn insert_alignment_point(
    model: &Model,
    p: ohl_line::AlignmentPoint,
) -> Result<(String, Command, String), String> {
    let at = p.position;
    let stored = model
        .stored_line()
        .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
    let mut best: Option<(f64, String, usize)> = None;
    for a in stored
        .alignments
        .iter()
        .filter(|a| !a.points.contains(&p.id))
    {
        let points: Vec<[f64; 3]> = stored.points_of(a).iter().map(|p| p.position).collect();
        // Distance in plan to each segment (one point: to it, after it),
        // and where it goes.
        let mut here = if let [p] = &points[..] {
            ((p[0] - at[0]).hypot(p[1] - at[1]), 1)
        } else {
            (f64::INFINITY, points.len())
        };
        for k in 0..points.len().saturating_sub(1) {
            let (a, b) = (points[k], points[k + 1]);
            let d = [b[0] - a[0], b[1] - a[1]];
            let len2 = d[0] * d[0] + d[1] * d[1];
            let t = if len2 > 0.0 {
                ((at[0] - a[0]) * d[0] + (at[1] - a[1]) * d[1]) / len2
            } else {
                0.0
            };
            let tc = t.clamp(0.0, 1.0);
            let dist = (a[0] + tc * d[0] - at[0]).hypot(a[1] + tc * d[1] - at[1]);
            let index = if t < 0.0 && k == 0 {
                0
            } else if t > 1.0 && k + 2 == points.len() {
                points.len()
            } else {
                k + 1
            };
            if dist < here.0 {
                here = (dist, index);
            }
        }
        if best.as_ref().is_none_or(|b| here.0 < b.0) {
            best = Some((here.0, a.id.clone(), here.1));
        }
    }
    let (_, alignment, index) =
        best.ok_or("no alignment to add a point to: draw one first (New alignment tool)")?;
    let mut commands = Vec::new();
    let id = if p.id.is_empty() {
        let id = stored.new_point_id();
        let point = ohl_line::AlignmentPoint {
            id: id.clone(),
            ..p
        };
        commands.push(Command::Put {
            kind: Kind::AlignmentPoint,
            id: id.clone(),
            value: serde_json::to_value(&point).map_err(|e| e.to_string())?,
            ord: None,
        });
        id
    } else {
        p.id
    };
    let mut a = stored
        .alignment_by_id(&alignment)
        .ok_or("no such alignment")?
        .clone();
    a.points.insert(index.min(a.points.len()), id.clone());
    commands.push(Command::Put {
        kind: Kind::Alignment,
        id: alignment.clone(),
        value: serde_json::to_value(&a).map_err(|e| e.to_string())?,
        ord: None,
    });
    let command = match commands.len() {
        1 => commands.remove(0),
        _ => Command::Batch { commands },
    };
    Ok((
        format!(
            "point {id} now point {} of alignment {alignment}",
            index + 1
        ),
        command,
        id,
    ))
}

/// What the Edit alignment tool does with its points.
#[derive(Debug)]
pub enum Reroute {
    /// The alignment changed: what, the command.
    Done(String, Command),
    /// The first point is in the middle of its alignments: an alignment
    /// has two ends only, so the points make a new alignment.
    Branch,
}

/// The Edit alignment tool: from point `a` (on an alignment) through
/// `middle` (new points without an id, or existing ones) to point `b` of
/// the same alignment — the points between `a` and `b` are replaced by
/// `middle` (they stay, on no alignment unless on another); without `b`,
/// at an end of `a`'s alignment, `middle` extends it there (after its
/// last point, or before its first). `a` in the middle of every
/// alignment it is on, without `b`: [`Reroute::Branch`].
pub fn reroute(
    stored: &Line,
    a: &str,
    b: Option<&str>,
    middle: &[ohl_line::AlignmentPoint],
) -> Result<Reroute, String> {
    // The alignment: with both points, or with `a` at an end.
    let (alignment, ia, ib) = match b {
        Some(b) => {
            let x = stored
                .alignments
                .iter()
                .find(|x| x.points.iter().any(|p| p == a) && x.points.iter().any(|p| p == b))
                .ok_or_else(|| format!("points {a} and {b} are on no alignment together"))?;
            let pos = |id: &str| x.points.iter().position(|p| p == id).expect("on it");
            (x, pos(a), Some(pos(b)))
        }
        None => {
            if middle.is_empty() {
                return Err("Edit alignment: right-click the new point(s) first".into());
            }
            let at_end = stored.alignments.iter().find_map(|x| {
                let k = x.points.iter().position(|p| p == a)?;
                (k == 0 || k + 1 == x.points.len()).then_some((x, k))
            });
            match at_end {
                Some((x, k)) => (x, k, None),
                None => return Ok(Reroute::Branch),
            }
        }
    };
    let mut commands = Vec::new();
    let mut line = stored.clone();
    let mut ids = Vec::new();
    for p in middle {
        if p.id.is_empty() {
            let id = line.new_point_id();
            let point = ohl_line::AlignmentPoint {
                id: id.clone(),
                ..p.clone()
            };
            commands.push(Command::Put {
                kind: Kind::AlignmentPoint,
                id: id.clone(),
                value: serde_json::to_value(&point).map_err(|e| e.to_string())?,
                ord: None,
            });
            line.alignment_points.push(point);
            ids.push(id);
        } else {
            ids.push(p.id.clone());
        }
    }
    let old = &alignment.points;
    let points: Vec<String> = match ib {
        Some(ib) if ia < ib => old[..=ia]
            .iter()
            .chain(&ids)
            .chain(&old[ib..])
            .cloned()
            .collect(),
        Some(ib) => {
            let back: Vec<String> = ids.iter().rev().cloned().collect();
            old[..=ib]
                .iter()
                .chain(&back)
                .chain(&old[ia..])
                .cloned()
                .collect()
        }
        // The last point (or the only one): after it.
        None if ia + 1 == old.len() => old.iter().chain(&ids).cloned().collect(),
        None => ids.iter().rev().chain(old).cloned().collect(),
    };
    if let Some(d) = points
        .iter()
        .enumerate()
        .find(|(k, p)| points[..*k].contains(p))
        .map(|x| x.1)
    {
        return Err(format!(
            "alignment {}: point {d} would be on it twice",
            alignment.id
        ));
    }
    let off: Vec<&String> = old.iter().filter(|p| !points.contains(p)).collect();
    let mut a2 = alignment.clone();
    a2.points = points;
    commands.push(Command::Put {
        kind: Kind::Alignment,
        id: a2.id.clone(),
        value: serde_json::to_value(&a2).map_err(|e| e.to_string())?,
        ord: None,
    });
    let what = match (ib, off.is_empty()) {
        (None, _) => format!(
            "alignment {}: {} point(s) added at its {}",
            a2.id,
            ids.len(),
            if ia == 0 && old.len() > 1 {
                "start"
            } else {
                "end"
            }
        ),
        (Some(_), true) => format!(
            "alignment {}: {} point(s) put between {a} and {}",
            a2.id,
            ids.len(),
            b.unwrap_or_default()
        ),
        (Some(_), false) => format!(
            "alignment {}: {} point(s) put between {a} and {}; {} taken off it (they stay)",
            a2.id,
            ids.len(),
            b.unwrap_or_default(),
            off.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let command = if commands.len() == 1 {
        commands.remove(0)
    } else {
        Command::Batch { commands }
    };
    Ok(Reroute::Done(what, command))
}

pub(super) fn alignment_ui(ui: &mut egui::Ui, f: &mut AlignmentForm, model: &Model) {
    let stored = model.stored_line().unwrap_or(&model.line);
    egui::Grid::new("alignment-form")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            ui.label(&f.id);
            ui.end_row();
            ui.label("description");
            ui.add(
                egui::TextEdit::singleline(&mut f.description)
                    .desired_width(240.0)
                    .hint_text("what it is (free text)"),
            );
            ui.end_row();
            ui.label("tied to");
            egui::ComboBox::from_id_salt("alignment-structure")
                .selected_text(f.structure.as_deref().unwrap_or("(free)"))
                .height(400.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.structure, None, "(free)");
                    for s in &model.line.structures {
                        ui.selectable_value(&mut f.structure, Some(s.id.clone()), &s.id);
                    }
                });
            ui.end_row();
            let free = f.structure.is_none();
            for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                ui.label(name);
                ui.add_enabled_ui(free && (k < 2 || !f.on_ground), |ui| {
                    value(ui, &mut f.position[k], 0.1, " m");
                });
                ui.end_row();
            }
        });
    if f.structure.is_none() {
        ui.checkbox(&mut f.on_ground, "on the ground");
        if f.on_ground
            && let Some(z) = model.ground_at(f.position[0], f.position[1])
        {
            f.position[2] = z;
        }
    } else {
        ui.label(
            egui::RichText::new("A tied point sits under its structure and follows it.").small(),
        );
    }
    ui.separator();
    ui.label("On the alignments, at its place in each (1: the first point):");
    let mut drop = None;
    egui::Grid::new("alignment-orders")
        .num_columns(3)
        .show(ui, |ui| {
            for (k, (a, order)) in f.orders.iter_mut().enumerate() {
                let n = stored.alignment_by_id(a).map_or(1, |x| {
                    x.points.len() + usize::from(!x.points.contains(&f.id))
                });
                let route = stored
                    .alignment_by_id(a)
                    .map(|x| format!(" (route {})", x.route))
                    .unwrap_or_default();
                ui.label(format!("{a}{route}"));
                ui.add(egui::DragValue::new(order).range(1..=n.max(1)).prefix("#"))
                    .on_hover_text("Its place along this alignment");
                if ui
                    .small_button("×")
                    .on_hover_text("Take it off this alignment (it stays)")
                    .clicked()
                {
                    drop = Some(k);
                }
                ui.end_row();
            }
        });
    if let Some(k) = drop {
        f.orders.remove(k);
    }
    let others: Vec<&ohl_line::Alignment> = stored
        .alignments
        .iter()
        .filter(|a| !f.orders.iter().any(|(x, _)| *x == a.id))
        .collect();
    if !others.is_empty() {
        egui::ComboBox::from_id_salt("alignment-add")
            .selected_text("+ on another alignment…")
            .show_ui(ui, |ui| {
                for a in others {
                    if ui
                        .selectable_label(false, format!("{} (route {})", a.id, a.route))
                        .clicked()
                    {
                        // Where it fits along it.
                        let pts: Vec<[f64; 3]> =
                            stored.points_of(a).iter().map(|p| p.position).collect();
                        let at = best_place(&pts, f.position);
                        f.orders.push((a.id.clone(), at + 1));
                    }
                }
            });
    }
    if f.orders.is_empty() {
        ui.label(
            egui::RichText::new("On no alignment: it waits to be used by one.")
                .small()
                .weak(),
        );
    }
    ui.label(
        egui::RichText::new(
            "The profile view follows the alignment points. A point shared by several \
             alignments (a junction) is one point: moved, it moves on all of them.",
        )
        .small(),
    );
}
