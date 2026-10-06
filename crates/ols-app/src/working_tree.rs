// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Lines › Project lines › Working tree: the history of the project lines
//! drawn from left (oldest) to right (newest) — a lane per line, a dot per
//! revision, where each line started from another and where lines were
//! merged; the lines that exist, those deleted (grey), uncommitted work
//! (an open dot). Admins roll a line back to a revision from here.

use std::collections::HashMap;

use eframe::egui;
use ols_server::Op;
use ols_session::lines::LineInfo;
use ols_session::versions::{RefKind, Revision, short};

use crate::app::Ready;

/// A revision placed in the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The revision.
    pub revision: Revision,
    /// Its lane (index into [`Tree::lanes`]).
    pub lane: usize,
    /// Its column: its place in time (0: the oldest).
    pub col: usize,
    /// Tags naming it.
    pub tags: Vec<String>,
}

/// A lane: a project line, or the revisions of lines deleted.
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    /// The line's name (or a description for the deleted ones).
    pub name: String,
    /// The line, when it exists.
    pub line: Option<LineInfo>,
    /// Its last revision.
    pub head: Option<String>,
    /// It has work not committed (an open dot after its head).
    pub uncommitted: bool,
}

/// The tree laid out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tree {
    /// Lanes, top to bottom: the main line first.
    pub lanes: Vec<Lane>,
    /// Revisions, oldest first.
    pub nodes: Vec<Node>,
    /// Node index by revision id.
    pub index: HashMap<String, usize>,
}

impl Tree {
    /// Lays out `revisions` (newest first, as the session lists them) on a
    /// lane per line of `lines` (the main line first): each line claims
    /// the revisions of its history along first parents, the first lines
    /// first, so a line's lane starts where it left another; revisions no
    /// line claims (lines deleted) go to a last lane. `tags`: (name,
    /// revision); `uncommitted(line)`: whether it has work not committed.
    pub fn layout(
        revisions: &[Revision],
        lines: &[LineInfo],
        tags: &[(String, String)],
        uncommitted: &dyn Fn(&str) -> bool,
    ) -> Self {
        let mut tree = Tree::default();
        for (col, r) in revisions.iter().rev().enumerate() {
            tree.index.insert(r.id.clone(), col);
            tree.nodes.push(Node {
                revision: r.clone(),
                lane: usize::MAX,
                col,
                tags: tags
                    .iter()
                    .filter(|t| t.1 == r.id)
                    .map(|t| t.0.clone())
                    .collect(),
            });
        }
        for l in lines {
            let lane = tree.lanes.len();
            tree.lanes.push(Lane {
                name: l.name.clone(),
                line: Some(l.clone()),
                head: l.revision.clone(),
                uncommitted: uncommitted(&l.name),
            });
            // Its history along first parents, until a revision another
            // line claimed (where it started).
            let mut at = l.revision.clone();
            while let Some(id) = at {
                let Some(&k) = tree.index.get(&id) else {
                    break;
                };
                if tree.nodes[k].lane != usize::MAX {
                    break;
                }
                tree.nodes[k].lane = lane;
                at = tree.nodes[k].revision.parents.first().cloned();
            }
        }
        if tree.nodes.iter().any(|n| n.lane == usize::MAX) {
            let lane = tree.lanes.len();
            tree.lanes.push(Lane {
                name: "lines deleted".into(),
                line: None,
                head: None,
                uncommitted: false,
            });
            for n in &mut tree.nodes {
                if n.lane == usize::MAX {
                    n.lane = lane;
                }
            }
        }
        tree
    }

    /// Columns used (with the one of uncommitted work).
    pub fn columns(&self) -> usize {
        self.nodes.len() + usize::from(self.lanes.iter().any(|l| l.uncommitted))
    }
}

/// Lane colours, in turn (the main line first).
const COLORS: [[u8; 3]; 6] = [
    [120, 190, 255],
    [240, 170, 60],
    [120, 230, 140],
    [230, 120, 230],
    [255, 220, 90],
    [150, 220, 230],
];

fn lane_color(tree: &Tree, lane: usize) -> egui::Color32 {
    if tree.lanes[lane].line.is_none() {
        return egui::Color32::from_gray(120);
    }
    let c = COLORS[lane % COLORS.len()];
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// The Working tree window.
#[derive(Debug, Default)]
pub struct TreeWindow {
    /// Open.
    pub open: bool,
    /// The revision clicked.
    pub selected: Option<String>,
    /// The uncommitted work of a line clicked (its lane), and the commit
    /// message typed.
    pub work: Option<(usize, String)>,
    /// The lane whose description is shown (default: the line worked on).
    pub lane: Option<usize>,
    /// The line to roll back to the revision clicked, and whether it is
    /// confirmed.
    rollback: Option<(String, bool)>,
    /// The tree, laid out for (log entry, revisions, lines).
    cache: Option<(i64, usize, Vec<String>, Tree)>,
}

/// Column and lane spacing, px; the lanes' labels' width.
const COL: f32 = 34.0;
const ROW: f32 = 34.0;
const LABELS: f32 = 150.0;

/// A time as day.month.year hours:minutes (UTC).
fn when(time: &str) -> String {
    match (time.get(..10), time.get(11..16)) {
        (Some(day), Some(clock)) => {
            let d: Vec<&str> = day.split('-').rev().collect();
            format!("{} {clock}", d.join("."))
        }
        _ => time.to_owned(),
    }
}

impl TreeWindow {
    /// The tree of the model's project, laid out again when it changed.
    fn tree(&mut self, model: &crate::model::Model) -> Option<Tree> {
        let s = model.session()?;
        let seq = s.last_seq().ok()?;
        let revisions = s.revisions().ok()?;
        let lines = s.lines().ok()?;
        let names: Vec<String> = lines
            .iter()
            .map(|l| format!("{}@{:?}#{}", l.name, l.revision, l.description))
            .collect();
        let fresh = self
            .cache
            .as_ref()
            .is_some_and(|c| c.0 == seq && c.1 == revisions.len() && c.2 == names);
        if !fresh {
            let tags: Vec<(String, String)> = s
                .refs()
                .ok()?
                .into_iter()
                .filter(|x| x.kind == RefKind::Tag)
                .filter_map(|x| Some((x.name, x.revision?)))
                .collect();
            let tree = Tree::layout(&revisions, &lines, &tags, &|l| {
                s.has_uncommitted(l).unwrap_or(false)
            });
            self.cache = Some((seq, revisions.len(), names, tree));
        }
        self.cache.as_ref().map(|c| c.3.clone())
    }
}

/// The window (when open).
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    if !r.working_tree.open {
        return;
    }
    let Some(tree) = r.working_tree.tree(&r.model) else {
        r.working_tree.open = false;
        return;
    };
    let admin = r.model.my_role() == Some(ols_session::Role::Admin);
    let can_edit = r.model.can_edit().is_ok();
    let current = r.model.project_line();
    let mut open = true;
    let mut action: Option<(String, Op)> = None;
    let mut new_line_from: Option<(String, String)> = None;
    let mut describe: Option<(String, String)> = None;
    let w = &mut r.working_tree;
    egui::Window::new("Working tree")
        .open(&mut open)
        .default_size([900.0, 420.0])
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "The project lines through time, oldest on the left: a dot per revision, \
                     a lane per line (where it starts: the line it was made from); joined \
                     lanes are merges (changes brought in, reviews merged). An open dot: work \
                     not committed yet. Grey: revisions of lines deleted. Click a revision for \
                     its details.",
                )
                .small()
                .weak(),
            );
            if tree.nodes.is_empty() {
                ui.label(
                    "Nothing committed yet: the tree grows with commits, new lines, changes \
                     brought in and reviews merged.",
                );
            }
            let width = LABELS + COL * (tree.columns() as f32 + 1.0);
            let height = ROW * tree.lanes.len() as f32 + 40.0;
            egui::ScrollArea::both()
                .max_height((ui.available_height() - 280.0).max(140.0))
                .auto_shrink([false, true])
                .stick_to_right(true)
                .show(ui, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
                    let painter = ui.painter_at(rect);
                    let pos = |lane: usize, col: usize| {
                        egui::pos2(
                            rect.left() + LABELS + COL * (col as f32 + 0.5),
                            rect.top() + 16.0 + ROW * (lane as f32 + 0.5),
                        )
                    };
                    // Lanes: their name, a faint guide.
                    for (k, lane) in tree.lanes.iter().enumerate() {
                        let y = pos(k, 0).y;
                        let color = lane_color(&tree, k);
                        painter.line_segment(
                            [
                                egui::pos2(rect.left() + LABELS - 6.0, y),
                                egui::pos2(rect.right(), y),
                            ],
                            egui::Stroke::new(1.0, color.gamma_multiply(0.15)),
                        );
                        let mut name = lane.name.clone();
                        if lane.name == current && lane.line.is_some() {
                            name.push_str("  (you)");
                        }
                        let label = painter.text(
                            egui::pos2(rect.left() + 4.0, y),
                            egui::Align2::LEFT_CENTER,
                            name,
                            egui::FontId::proportional(13.0),
                            color,
                        );
                        if let Some(l) = &lane.line {
                            let hover =
                                ui.interact(label, ui.id().with(("lane", k)), egui::Sense::click());
                            if hover.clicked() {
                                w.lane = Some(k);
                            }
                            hover.on_hover_text(format!(
                                "{}{}{}",
                                if l.is_main() {
                                    "the main line".to_owned()
                                } else {
                                    format!(
                                        "made by {} on {}",
                                        if l.user.is_empty() { "?" } else { &l.user },
                                        when(&l.time)
                                    )
                                },
                                if l.note.is_empty() {
                                    String::new()
                                } else {
                                    format!(" — {}", l.note)
                                },
                                if l.revision.is_none() {
                                    "\nnothing committed yet\nclick: its description"
                                } else {
                                    "\nclick: its description"
                                }
                            ));
                        }
                    }
                    // Edges: from each parent to its child.
                    for n in &tree.nodes {
                        let to = pos(n.lane, n.col);
                        for (k, p) in n.revision.parents.iter().enumerate() {
                            let Some(&pi) = tree.index.get(p) else {
                                continue;
                            };
                            let from = pos(tree.nodes[pi].lane, tree.nodes[pi].col);
                            // The first parent in the child's colour; a
                            // merged line's in its own.
                            let color = if k == 0 {
                                lane_color(&tree, n.lane)
                            } else {
                                lane_color(&tree, tree.nodes[pi].lane)
                            };
                            let stroke = egui::Stroke::new(2.0, color);
                            if (from.y - to.y).abs() < 0.5 {
                                painter.line_segment([from, to], stroke);
                            } else {
                                let mid = (from.x + to.x) / 2.0;
                                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                                    [
                                        from,
                                        egui::pos2(mid.max(to.x - COL), from.y),
                                        egui::pos2(mid.max(to.x - COL), to.y),
                                        to,
                                    ],
                                    false,
                                    egui::Color32::TRANSPARENT,
                                    stroke,
                                ));
                            }
                        }
                    }
                    // Uncommitted work: an open dot after the head.
                    let now = tree.nodes.len();
                    for (k, lane) in tree.lanes.iter().enumerate() {
                        if !lane.uncommitted {
                            continue;
                        }
                        let at = pos(k, now);
                        let color = lane_color(&tree, k);
                        if let Some(&h) = lane.head.as_ref().and_then(|h| tree.index.get(h)) {
                            let from = pos(tree.nodes[h].lane, tree.nodes[h].col);
                            painter.add(egui::Shape::dashed_line(
                                &[from, at],
                                egui::Stroke::new(1.5, color),
                                4.0,
                                3.0,
                            ));
                        }
                        painter.circle_stroke(at, 6.0, egui::Stroke::new(2.0, color));
                        if w.work.as_ref().is_some_and(|x| x.0 == k) {
                            painter.circle_stroke(
                                at,
                                10.0,
                                egui::Stroke::new(2.0, egui::Color32::WHITE),
                            );
                        }
                        let hit = ui.interact(
                            egui::Rect::from_center_size(at, egui::vec2(18.0, 18.0)),
                            ui.id().with(("uncommitted", k)),
                            egui::Sense::click(),
                        );
                        if hit.clicked() {
                            w.selected = None;
                            w.rollback = None;
                            w.lane = Some(k);
                            w.work = Some((k, String::new()));
                        }
                        hit.on_hover_text(format!(
                            "{}: work not committed yet — click to see it",
                            lane.name
                        ));
                    }
                    // A line with no revision of its own yet: a ring where it
                    // started (on the lane of the line it came from).
                    for (k, lane) in tree.lanes.iter().enumerate() {
                        if lane.line.is_none() || tree.nodes.iter().any(|n| n.lane == k) {
                            continue;
                        }
                        let Some(&h) = lane.head.as_ref().and_then(|h| tree.index.get(h)) else {
                            continue;
                        };
                        let head = &tree.nodes[h];
                        let at = pos(k, head.col);
                        let color = lane_color(&tree, k);
                        painter.line_segment(
                            [pos(head.lane, head.col), at],
                            egui::Stroke::new(1.5, color),
                        );
                        painter.circle_filled(at, 4.0, egui::Color32::from_gray(30));
                        painter.circle_stroke(at, 4.5, egui::Stroke::new(2.0, color));
                        let hit = ui.interact(
                            egui::Rect::from_center_size(at, egui::vec2(18.0, 18.0)),
                            ui.id().with(("start", k)),
                            egui::Sense::click(),
                        );
                        if hit.clicked() {
                            w.work = None;
                            w.lane = Some(k);
                            w.selected = Some(head.revision.id.clone());
                            w.rollback = Some((lane.name.clone(), false));
                        }
                        hit.on_hover_text(format!(
                            "{} started here ({}), nothing of its own committed yet",
                            lane.name,
                            head.revision.short()
                        ));
                    }
                    // Revisions.
                    let mut last_day = String::new();
                    for n in &tree.nodes {
                        let at = pos(n.lane, n.col);
                        let color = lane_color(&tree, n.lane);
                        let selected = w.selected.as_deref() == Some(n.revision.id.as_str());
                        let head =
                            tree.lanes[n.lane].head.as_deref() == Some(n.revision.id.as_str());
                        if n.revision.parents.len() > 1 {
                            painter.circle_filled(at, 7.0, color);
                            painter.circle_filled(at, 3.0, egui::Color32::from_gray(30));
                        } else {
                            painter.circle_filled(at, if head { 7.0 } else { 5.5 }, color);
                        }
                        if selected {
                            painter.circle_stroke(
                                at,
                                10.0,
                                egui::Stroke::new(2.0, egui::Color32::WHITE),
                            );
                        }
                        if !n.tags.is_empty() {
                            painter.text(
                                at - egui::vec2(0.0, 11.0),
                                egui::Align2::CENTER_BOTTOM,
                                n.tags.join(", "),
                                egui::FontId::proportional(10.0),
                                egui::Color32::from_rgb(255, 230, 150),
                            );
                        }
                        // The day under the first revision of each day.
                        let day = n.revision.time.get(..10).unwrap_or("").to_owned();
                        if day != last_day {
                            painter.text(
                                egui::pos2(at.x, rect.bottom() - 6.0),
                                egui::Align2::LEFT_BOTTOM,
                                // Day.month.year.
                                day.split('-').rev().collect::<Vec<_>>().join("."),
                                egui::FontId::proportional(10.0),
                                egui::Color32::from_gray(150),
                            );
                            last_day = day;
                        }
                        let hit = ui.interact(
                            egui::Rect::from_center_size(at, egui::vec2(18.0, 18.0)),
                            ui.id().with(("rev", &n.revision.id)),
                            egui::Sense::click(),
                        );
                        if hit.clicked() {
                            w.work = None;
                            w.lane = Some(n.lane);
                            w.selected = Some(n.revision.id.clone());
                            w.rollback = tree.lanes[n.lane]
                                .line
                                .as_ref()
                                .map(|l| (l.name.clone(), false));
                        }
                        hit.on_hover_text(format!(
                            "{}\n{} — {}\n{}",
                            n.revision.message,
                            n.revision.author,
                            when(&n.revision.time),
                            n.revision.short()
                        ));
                    }
                });
            // The description of the line clicked (else the one worked on).
            let shown = w
                .lane
                .filter(|k| tree.lanes.get(*k).is_some_and(|l| l.line.is_some()))
                .or_else(|| tree.lanes.iter().position(|l| l.name == current));
            if let Some(lane) = shown.and_then(|k| tree.lanes.get(k))
                && let Some(info) = &lane.line
            {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Description of {}", lane.name))
                            .color(lane_color(&tree, shown.unwrap_or(0))),
                    );
                    if ui
                        .small_button("Edit…")
                        .on_hover_text("Its creator or an admin edit it; others read it")
                        .clicked()
                    {
                        describe = Some((lane.name.clone(), info.description.clone()));
                    }
                });
                crate::project_lines::description_box(
                    ui,
                    "tree-description",
                    &info.description,
                    90.0,
                );
            }
            // The revision clicked, or a line's uncommitted work.
            ui.separator();
            if let Some((k, message)) = &mut w.work {
                let Some(lane) = tree.lanes.get(*k).filter(|l| l.uncommitted) else {
                    w.work = None;
                    return;
                };
                ui.strong(format!("{}: work not committed yet", lane.name));
                if lane.name == current && can_edit {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(message)
                                .hint_text("Why: raised T12 by 3 m for the road crossing")
                                .desired_width(360.0),
                        );
                        if ui
                            .add_enabled(!message.trim().is_empty(), egui::Button::new("Commit"))
                            .on_hover_text("Records it as a revision of this line")
                            .clicked()
                        {
                            action = Some((
                                format!("{} committed", lane.name),
                                Op::Commit {
                                    message: message.trim().to_owned(),
                                },
                            ));
                        }
                    });
                } else {
                    ui.label(
                        egui::RichText::new(
                            "Committed by whoever works on the line (Lines › Project lines: \
                             work on it, then commit here); bringing in changes and \
                             submitting a review record it too.",
                        )
                        .small()
                        .weak(),
                    );
                }
                return;
            }
            let Some(id) = w.selected.clone() else {
                ui.label(
                    egui::RichText::new(
                        "Click a revision, or an open dot (work not committed), for its details.",
                    )
                    .weak(),
                );
                return;
            };
            let Some(n) = tree.index.get(&id).map(|&k| &tree.nodes[k]) else {
                w.selected = None;
                return;
            };
            ui.horizontal_wrapped(|ui| {
                ui.strong(&n.revision.message);
                ui.label(format!(
                    "— {}, {} ({}) on {}",
                    n.revision.author,
                    when(&n.revision.time),
                    n.revision.short(),
                    tree.lanes[n.lane].name
                ));
                if n.revision.parents.len() > 1 {
                    ui.label(egui::RichText::new("merge").weak());
                }
                if !n.tags.is_empty() {
                    ui.label(format!("tags: {}", n.tags.join(", ")));
                }
            });
            ui.horizontal(|ui| {
                if r.model.can_edit().is_ok()
                    && ui
                        .button("New line from it…")
                        .on_hover_text("A project line starting from this revision")
                        .clicked()
                {
                    let words = format!(
                        "{} at {} — {}",
                        tree.lanes[n.lane].name,
                        when(&n.revision.time),
                        n.revision.message
                    );
                    new_line_from = Some((id.clone(), words));
                }
                let lines: Vec<String> = tree
                    .lanes
                    .iter()
                    .filter_map(|l| l.line.as_ref().map(|x| x.name.clone()))
                    .collect();
                if !admin {
                    ui.label(
                        egui::RichText::new("Rolling a line back: admins only.")
                            .small()
                            .weak(),
                    );
                    return;
                }
                // The revision's own line at first (else the one worked on).
                let own = tree.lanes[n.lane]
                    .line
                    .as_ref()
                    .map_or_else(|| current.clone(), |l| l.name.clone());
                let (line, sure) = w.rollback.get_or_insert((own, false));
                ui.label("Line to roll back:");
                egui::ComboBox::from_id_salt("rollback-line")
                    .selected_text(line.as_str())
                    .show_ui(ui, |ui| {
                        for l in &lines {
                            ui.selectable_value(line, l.clone(), l);
                        }
                    });
                ui.label(format!("to {}", short(&id)));
                if !*sure {
                    if ui
                        .button("Roll back…")
                        .on_hover_text(
                            "The line's state becomes this revision's — recorded as a new \
                             revision, its uncommitted work first: nothing is lost, and it can \
                             be rolled back again",
                        )
                        .clicked()
                    {
                        *sure = true;
                    }
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 200, 120),
                        format!("{line} becomes as at {}: sure?", short(&id)),
                    );
                    if ui.button("Roll back").clicked() {
                        action = Some((
                            format!("project line {line} rolled back to {}", short(&id)),
                            Op::RollBackLine {
                                line: line.clone(),
                                revision: id.clone(),
                            },
                        ));
                        *sure = false;
                    }
                    if ui.button("Keep").clicked() {
                        *sure = false;
                    }
                }
            });
        });
    w.open = open;
    if let Some((line, text)) = describe {
        r.lines_window = Some(crate::project_lines::LinesWindow::Description { line, text });
    }
    if let Some((from, words)) = new_line_from {
        let names: Vec<String> = tree
            .lanes
            .iter()
            .filter_map(|l| l.line.as_ref().map(|x| x.name.clone()))
            .collect();
        r.lines_window = Some(crate::project_lines::LinesWindow::New {
            name: crate::editors::next_id(names.iter().map(String::as_str), "phase 0"),
            from: from.clone(),
            at: Some((from, words)),
        });
    }
    if let Some((what, op)) = action {
        let line = match &op {
            Op::RollBackLine { line, .. } => line.clone(),
            _ => String::new(),
        };
        r.project_op(&what, op);
        // Another line rolled back: redrawn where it is shown.
        if line != r.model.project_line() {
            r.model.refresh_lines(&[line]);
            r.view3d.dirty.insert(crate::scene::Layer::OtherLines);
        }
        r.working_tree.cache = None;
    }
}

/// The lanes of the lines by name (for tests).
#[cfg(test)]
pub fn lanes_by_name(tree: &Tree) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for n in &tree.nodes {
        out.entry(tree.lanes[n.lane].name.clone())
            .or_default()
            .push(n.revision.message.clone());
    }
    out
}
