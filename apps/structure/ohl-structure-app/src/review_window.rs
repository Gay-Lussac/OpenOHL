// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Review the joints left** (O44 L31; the owner, 2026-11-26: *"setup ways
//! to study the latest joint and lay them out. The goal is not to reach
//! 100% but not far off so that a human can relatively quickly check the
//! missing joints"*): Design › Review the joints left… (and Joint details)
//! open one tool window listing every joint the automatic layout did not
//! lay out whole, with why, grouped by reason with counts, and the joints
//! whole and left of all ([`ohl_structure_link::joint_review`]). A click
//! on a joint selects it and its members in the 3D view and opens it in
//! the Detail view, its details on the left (the owner's two panes);
//! **‹ Previous** and **Next ›** (keys P and N) step through the list;
//! from the joint, **Lay out this joint…** and **Solve the clashes…** open the
//! existing windows for it. A **Reviewed** tick per joint is stored with
//! the joint (structure file version 29) and cleared when the joint's
//! layout changes.
//!
//! The tool-window pattern (`docs/development/tool-pages.md`), its own page
//! of the manual [`REVIEW_PAGE`], with one departure said here: a tick is
//! written when it is made (each one undo step), so the window has no
//! Apply, and its last button is **Close** (Escape), not Cancel — closing
//! takes nothing back. Escape closes a window opened from it first (Lay
//! out joints, Solve the clashes), then this one.

use std::collections::BTreeSet;
use std::sync::Arc;

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_link::joint_review::{ReviewList, ReviewRow};

use crate::app::StructureApp;

/// The manual's page of the window (page, anchor).
pub const REVIEW_PAGE: (&str, &str) = ("user-guide/joint-review", REVIEW_ANCHOR);
const REVIEW_ANCHOR: &str = "review-the-joints-left";

/// The window's title.
pub const REVIEW_TITLE: &str = "Review the joints left";

/// How the list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    /// By reason: the groups in their order, the file's order within each.
    #[default]
    ByReason,
    /// By joint: the file's order.
    ByJoint,
}

/// The list read from the file: for the file's revision, with the time it
/// took (s).
pub type ReviewCache = Option<(u64, Arc<Result<ReviewList, String>>, f64)>;

/// The Review the joints left window.
#[derive(Debug, Default)]
pub struct ReviewWindow {
    /// The order of the list.
    pub order: Order,
    /// The joints ticked reviewed left out of the list.
    pub hide_reviewed: bool,
    /// The joint shown (selected, in the Detail view).
    pub current: Option<String>,
    /// The groups closed in the list (by name).
    pub closed: BTreeSet<String>,
    /// Scroll the list to the joint shown on the next frame.
    scroll: bool,
}

/// One line of the list: a group's heading or a joint.
enum Line<'a> {
    Group(&'a str, usize, usize),
    Joint(&'a ReviewRow),
}

impl ReviewWindow {
    /// The joints in the window's order (the hidden ones left out).
    pub fn order_of<'a>(&self, list: &'a ReviewList) -> Vec<&'a ReviewRow> {
        let all = match self.order {
            Order::ByReason => list.by_group(),
            Order::ByJoint => list.rows.iter().collect(),
        };
        all.into_iter()
            .filter(|r| !(self.hide_reviewed && r.reviewed))
            .collect()
    }

    /// The joint after (`step` 1) or before (−1) the one shown in the
    /// window's order; the first (or last) when none is shown. A joint
    /// shown that left the list (ticked with the reviewed ones hidden) is
    /// found by its place among all.
    pub fn step_from(&self, list: &ReviewList, step: i32) -> Option<String> {
        let shown = self.order_of(list);
        if shown.is_empty() {
            return None;
        }
        let Some(cur) = self.current.as_deref() else {
            let end = if step > 0 {
                shown[0]
            } else {
                shown[shown.len() - 1]
            };
            return Some(end.joint.clone());
        };
        if let Some(k) = shown.iter().position(|r| r.joint == cur) {
            let n = k as i64 + i64::from(step);
            return (0..shown.len() as i64)
                .contains(&n)
                .then(|| shown[n as usize].joint.clone());
        }
        // Not shown (hidden): by its place among all in the same order.
        let all: Vec<&ReviewRow> = match self.order {
            Order::ByReason => list.by_group(),
            Order::ByJoint => list.rows.iter().collect(),
        };
        let at = all.iter().position(|r| r.joint == cur)?;
        let near = |r: &&&ReviewRow| shown.iter().any(|s| s.joint == r.joint);
        if step > 0 {
            all[at + 1..].iter().find(near).map(|r| r.joint.clone())
        } else {
            all[..at].iter().rev().find(near).map(|r| r.joint.clone())
        }
    }
}

impl StructureApp {
    /// The joints left of the file open, read once per revision.
    pub(crate) fn review_list(&mut self) -> Option<Arc<Result<ReviewList, String>>> {
        let f = self.file.as_ref()?;
        let rev = f.revision();
        if let Some((r, l, _)) = &self.review_cache
            && *r == rev
        {
            return Some(l.clone());
        }
        let t0 = std::time::Instant::now();
        let l = Arc::new(ohl_structure_link::joint_review::review_list_of(
            f.structure(),
            f.text_ref(),
        ));
        self.review_cache = Some((rev, l.clone(), t0.elapsed().as_secs_f64()));
        Some(l)
    }

    /// Opens Review the joints left, at joint `at` (none: the joint of
    /// Joint details where it is left, else none shown).
    pub(crate) fn open_review_window(&mut self, at: Option<String>) {
        let mut w = self.review_window.take().unwrap_or_default();
        let at = at.or_else(|| self.joints.joint.clone());
        if let Some(Ok(l)) = self.review_list().as_deref() {
            w.current = at.filter(|j| l.row(j).is_some());
        }
        w.scroll = true;
        self.review_window = Some(w);
    }

    /// Joint `j` shown: selected with its members (the 3D view), in the
    /// Detail view, its details on the left.
    pub(crate) fn review_go(&mut self, j: &str) {
        let members = match self.review_list().as_deref() {
            Some(Ok(l)) => l.row(j).map(|r| r.members.clone()).unwrap_or_default(),
            _ => Vec::new(),
        };
        self.open_in_detail(Handle::Joint(j.to_owned()));
        let mut sel: Vec<Handle> = members.into_iter().map(Handle::Member).collect();
        // The joint last: the Detail view shows it (not a member).
        sel.push(Handle::Joint(j.to_owned()));
        self.selected = sel;
        if let Some(w) = self.review_window.as_mut() {
            w.current = Some(j.to_owned());
            w.scroll = true;
        }
    }

    /// Joint `j` ticked reviewed (`on`) or not: written into the file, one
    /// undo step; the tick holds the stamp of its layout now.
    pub(crate) fn set_review_tick(&mut self, j: &str, on: bool) {
        let stamp = match self.review_list().as_deref() {
            Some(Ok(l)) => l.row(j).map(|r| r.stamp.clone()),
            _ => None,
        };
        let Some(stamp) = stamp else {
            self.message = Some((format!("joint {j} is not among the joints left"), true));
            return;
        };
        let Some(f) = self.file.as_mut() else { return };
        let what = if on {
            format!("Reviewed: joint {j}")
        } else {
            format!("Not reviewed: joint {j}")
        };
        let ticks = [(j.to_owned(), on.then_some(stamp))];
        let from = f.revision();
        let r = f.edit_alone(&what, |doc, _| {
            ohl_structure_link::joint_review::set_reviewed(doc, &ticks)
        });
        // (The joint details read nothing a tick changes: kept.)
        let to = f.revision();
        self.joints.carry_details(from, to);
        self.message = Some(match r {
            Ok(_) => (format!("{what} (one undo step)"), false),
            Err(e) => (e, true),
        });
    }

    /// Another tool window is open (it takes the keys first).
    fn other_tool_window(&self) -> bool {
        self.confirm.is_some()
            || self.joints.ask_open.is_some()
            || self.joint_tools.offer.is_some()
            || self.joint_tools.continuous.is_some()
            || self.layout_window.is_some()
            || self.solve_window.is_some()
            || self.orient_window.is_some()
            || self.sizing_window.is_some()
            || self.apply_window.is_some()
            || self.bolt_grade_window.is_some()
            || self.inventory
    }

    /// The Review the joints left window ([`Self::open_review_window`]).
    pub(crate) fn review_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.review_window.take() else {
            return;
        };
        if self.file.is_none() {
            return;
        }
        let Some(list) = self.review_list() else {
            return;
        };
        let took = self.review_cache.as_ref().map_or(0.0, |c| c.2);
        // The keys (not while typing, nor while a window opened from here
        // is open): N next, P back, Enter reviewed and next.
        let mut go: Option<String> = None;
        let mut tick: Option<(String, bool)> = None;
        if let Ok(l) = list.as_ref()
            && !ctx.egui_wants_keyboard_input()
            && !self.other_tool_window()
        {
            use egui::{Key, Modifiers};
            let (next, back, enter) = ctx.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::N),
                    i.consume_key(Modifiers::NONE, Key::P),
                    i.consume_key(Modifiers::NONE, Key::Enter),
                )
            });
            if next {
                go = w.step_from(l, 1);
            } else if back {
                go = w.step_from(l, -1);
            } else if enter && let Some(cur) = w.current.clone() {
                if l.row(&cur).is_some_and(|r| !r.reviewed) {
                    tick = Some((cur, true));
                }
                go = w.step_from(l, 1);
            }
        }
        let mut open = true;
        let mut close = false;
        let mut lay_out: Option<String> = None;
        let mut solve: Option<String> = None;
        let problems = w.current.clone().and_then(|j| {
            self.joint_details()
                .and_then(|d| d.as_ref().as_ref().ok().and_then(|d| d.joint(&j).cloned()))
                .map(|d| crate::joint_panel::problem_groups(&d))
        });
        egui::Window::new(REVIEW_TITLE)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ohl_ui::tool_window::intro(
                    ui,
                    "Lists the joints the automatic layout did not lay out whole, with why: click \
                     one to see it in the Detail view, step through them, and tick each one you \
                     have reviewed.",
                    "Read from the file: the joints where two or more members meet (and any other \
                     the layout's record names), each either laid out whole by a run (no member \
                     there left for a hand detail) or left. A joint left is listed with the \
                     reasons the layout recorded for its members, in the group of its first \
                     reason in the groups' order (a leg splice, bolts joining nothing, an end out \
                     of the flange's plane, a bolted leg not flat, a built-up member, then room: \
                     a clash or check after writing, a plate into steel, the pull-back, a member \
                     left as it is, one plate too small, too short); a joint the record keeps no \
                     reason for comes last. A click selects the joint and its members in the 3D \
                     view and opens it in the Detail view, its details on the left; ‹ Previous \
                     and Next › (P, N) step through the list in its order; Lay out this joint… and \
                     Solve the clashes… open those windows for the joint. Reviewed is stored \
                     with the joint (one undo step each) and cleared when the joint's layout \
                     changes (a run, an edit there, Solve the clashes): it is your mark only. \
                     Nothing is laid out or checked by this window.",
                    REVIEW_ANCHOR,
                    || crate::help::open_section(REVIEW_PAGE.0, REVIEW_PAGE.1),
                );
                let l = match list.as_ref() {
                    Ok(l) => l,
                    Err(e) => {
                        crate::app::error_label(ui, format!("The joints could not be read: {e}"));
                        return;
                    }
                };
                if !l.run {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "No layout run recorded in this file: every joint is left. Design › Lay \
                         out joints… first.",
                    );
                }
                ui.label(egui::RichText::new(l.summary()).strong());
                ui.label(
                    egui::RichText::new(format!(
                        "{} to go. Read from the file in {:.0} ms (measured; again after each \
                         change of the file).",
                        l.left() - l.reviewed(),
                        took * 1000.0
                    ))
                    .small()
                    .weak(),
                );
                // Always visible (the owner's rule for NOT ON THE SAFE SIDE).
                ui.label(
                    egui::RichText::new(
                        "NOT ON THE SAFE SIDE where you take a tick for a check: a joint ticked \
                         reviewed is still not laid out by the program where it was left — the \
                         layout's checks after writing (clashes, bolts within their edges) never \
                         ran on what you detail by hand, and the tick hides the joint from the \
                         list. Joint details' Problems and the clash colours still show it (the \
                         tick changes no check); tick a joint only once Joint details shows no \
                         problem you have not accepted.",
                    )
                    .small()
                    .color(ui.visuals().warn_fg_color),
                );
                ui.horizontal_wrapped(|ui| {
                    ohl_ui::tool_window::label(
                        ui,
                        "Order",
                        "How the list is ordered (the view only: nothing is worked out again). By \
                         reason: the groups in their order, each with its joints and how many \
                         are reviewed, a group closed by a click on its heading. By joint: the \
                         file's order.",
                    );
                    ui.radio_value(&mut w.order, Order::ByReason, "by reason");
                    ui.radio_value(&mut w.order, Order::ByJoint, "by joint");
                    ui.checkbox(&mut w.hide_reviewed, "Hide reviewed")
                        .on_hover_text(
                            "The joints ticked reviewed left out of the list and of ‹ Previous / \
                         Next › (the view only)",
                        );
                });
                // The groups' counts (by reason).
                let mut lines: Vec<Line> = Vec::new();
                let shown = w.order_of(l);
                let groups = l.groups();
                if w.order == Order::ByReason {
                    for (g, n, k) in &groups {
                        if w.hide_reviewed && n == k {
                            continue;
                        }
                        lines.push(Line::Group(g.name.as_str(), *n, *k));
                        if !w.closed.contains(&g.name) {
                            lines.extend(
                                shown
                                    .iter()
                                    .filter(|r| &r.group == g)
                                    .map(|r| Line::Joint(r)),
                            );
                        }
                    }
                } else {
                    lines.extend(shown.iter().map(|r| Line::Joint(r)));
                }
                let row_h = ui.spacing().interact_size.y + 2.0;
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("review-list")
                    .max_height(260.0)
                    .auto_shrink([false, true]);
                if std::mem::take(&mut w.scroll)
                    && let Some(cur) = w.current.as_deref()
                    && let Some(k) = lines
                        .iter()
                        .position(|x| matches!(x, Line::Joint(r) if r.joint == cur))
                {
                    scroll = scroll.vertical_scroll_offset((k as f32 - 4.0).max(0.0) * row_h);
                }
                let mut toggle_group: Option<String> = None;
                scroll.show_rows(ui, row_h, lines.len(), |ui, range| {
                    for line in &lines[range] {
                        match line {
                            Line::Group(name, n, k) => {
                                let closed = w.closed.contains(*name);
                                let text = format!(
                                    "{} {name} — {n} joint(s), {k} reviewed",
                                    if closed { "+" } else { "-" }
                                );
                                if ui
                                    .add(
                                        egui::Label::new(egui::RichText::new(text).strong())
                                            .sense(egui::Sense::click()),
                                    )
                                    .on_hover_text("Click to close or open the group")
                                    .clicked()
                                {
                                    toggle_group = Some((*name).to_owned());
                                }
                            }
                            Line::Joint(r) => {
                                ui.horizontal(|ui| {
                                    let mut on = r.reviewed;
                                    if ui
                                        .checkbox(&mut on, "")
                                        .on_hover_text(
                                            "Reviewed: stored with the joint (one undo step), \
                                             cleared when its layout changes",
                                        )
                                        .changed()
                                    {
                                        tick = Some((r.joint.clone(), on));
                                    }
                                    let first = r
                                        .reasons
                                        .first()
                                        .map_or("no reason recorded", String::as_str);
                                    let mut text = format!("{}  ·  {}", r.joint, short(first, 90));
                                    if r.reasons.len() > 1 {
                                        text.push_str(&format!("  (+{})", r.reasons.len() - 1));
                                    }
                                    let mut rich = egui::RichText::new(text).small();
                                    if r.cleared {
                                        rich = rich.color(ui.visuals().warn_fg_color);
                                    }
                                    let hover = if r.reasons.is_empty() {
                                        r.group.name.clone()
                                    } else {
                                        r.reasons.join("\n")
                                    };
                                    let current = w.current.as_deref() == Some(r.joint.as_str());
                                    if ui
                                        .selectable_label(current, rich)
                                        .on_hover_text(hover)
                                        .clicked()
                                    {
                                        go = Some(r.joint.clone());
                                    }
                                });
                            }
                        }
                    }
                });
                if let Some(g) = toggle_group
                    && !w.closed.remove(&g)
                {
                    w.closed.insert(g);
                }
                if l.rows.is_empty() {
                    ui.label("Every joint counted is laid out whole: nothing to review.");
                }
                // The joint shown.
                ui.separator();
                let cur = w.current.as_deref().and_then(|j| l.row(j));
                match cur {
                    None => {
                        ui.label(
                            egui::RichText::new(
                                "Click a joint (or press N) to show it in the Detail view.",
                            )
                            .weak(),
                        );
                    }
                    Some(r) => {
                        let place = shown
                            .iter()
                            .position(|x| x.joint == r.joint)
                            .map_or(String::new(), |k| {
                                format!(" — {} of {}", k + 1, shown.len())
                            });
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(format!("Joint {}{place}", r.joint));
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}; {}",
                                    r.group.name,
                                    if r.written {
                                        "laid out in part"
                                    } else {
                                        "not laid out"
                                    }
                                ))
                                .small(),
                            );
                        });
                        if r.cleared {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                "Ticked before: its layout has changed since, so the tick is \
                                 cleared.",
                            );
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("review-reasons")
                            .max_height(96.0)
                            .show(ui, |ui| {
                                if r.reasons.is_empty() {
                                    ui.label(
                                        egui::RichText::new(
                                            "The layout's record keeps no reason for this joint: \
                                             it found nothing it could lay out here, or no run \
                                             asked it.",
                                        )
                                        .small(),
                                    );
                                }
                                for x in &r.reasons {
                                    ui.label(egui::RichText::new(x).small());
                                }
                            });
                        let n: usize = problems
                            .as_ref()
                            .map_or(0, |g| g.iter().map(|x| x.1.len()).sum());
                        if let Some(g) = &problems {
                            let words: Vec<String> = g
                                .iter()
                                .map(|(k, v)| format!("{} {}", v.len(), k.short()))
                                .collect();
                            ui.label(
                                egui::RichText::new(if n == 0 {
                                    "Joint details: no clash or problem here now.".to_owned()
                                } else {
                                    format!("Joint details: {}", words.join(", "))
                                })
                                .small()
                                .color(if n == 0 {
                                    ui.visuals().weak_text_color()
                                } else {
                                    ui.visuals().error_fg_color
                                }),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            let back = w.step_from(l, -1);
                            let next = w.step_from(l, 1);
                            if ui
                                .add_enabled(back.is_some(), egui::Button::new("‹ Previous"))
                                .on_hover_text("P: the joint before in the list")
                                .clicked()
                            {
                                go = back;
                            }
                            if ui
                                .add_enabled(next.is_some(), egui::Button::new("Next ›"))
                                .on_hover_text("N: the joint after in the list")
                                .clicked()
                            {
                                go = next;
                            }
                            if ui
                                .button("Lay out this joint…")
                                .on_hover_text(
                                    "Lay out joints for this joint (with its similar and \
                                     symmetric joints): a preview first, Apply one undo step",
                                )
                                .clicked()
                            {
                                lay_out = Some(r.joint.clone());
                            }
                            if ui
                                .add_enabled(n > 0, egui::Button::new("Solve the clashes…"))
                                .on_hover_text(
                                    "The joint's best whole solution worked out on copies, \
                                     before and after, Apply in one undo step",
                                )
                                .on_disabled_hover_text(
                                    "No clash or problem at this joint in Joint details",
                                )
                                .clicked()
                            {
                                solve = Some(r.joint.clone());
                            }
                            let mut on = r.reviewed;
                            if ui
                                .checkbox(&mut on, "Reviewed")
                                .on_hover_text(
                                    "Stored with the joint (one undo step); cleared when its \
                                     layout changes. Your mark only: nothing is checked for it",
                                )
                                .changed()
                            {
                                tick = Some((r.joint.clone(), on));
                            }
                        });
                    }
                }
                // The button row (Close, not Cancel: the ticks are written
                // as they are made).
                ui.separator();
                ui.horizontal(|ui| {
                    let can = cur.is_some_and(|r| !r.reviewed) || w.step_from(l, 1).is_some();
                    if ui
                        .add_enabled(
                            can,
                            egui::Button::new(egui::RichText::new("Reviewed, next ›").strong()),
                        )
                        .on_hover_text("Enter: the joint shown ticked reviewed, then the next")
                        .on_disabled_hover_text("No joint shown, and none after it")
                        .clicked()
                    {
                        if let Some(r) = cur.filter(|r| !r.reviewed) {
                            tick = Some((r.joint.clone(), true));
                        }
                        go = w.step_from(l, 1);
                    }
                    if ui
                        .button("Close")
                        .on_hover_text("Escape: the ticks made stay (each was one undo step)")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if let Some((j, on)) = tick {
            self.set_review_tick(&j, on);
        }
        if let Some(j) = go {
            self.review_window = Some(w);
            self.review_go(&j);
            w = self.review_window.take().unwrap_or_default();
        }
        if let Some(j) = lay_out {
            self.open_layout_window(crate::layout_window::Scope::Joint(j));
        }
        if let Some(j) = solve {
            self.open_solve_window(&j);
        }
        if open && !close {
            self.review_window = Some(w);
        }
    }

    /// Joint details' review line: at a joint left, its **Reviewed** tick
    /// and **Review the joints left…**.
    pub(crate) fn review_box(&mut self, ui: &mut egui::Ui, joint: &str) {
        let row = match self.review_list().as_deref() {
            Some(Ok(l)) if l.run => l.row(joint).cloned(),
            _ => None,
        };
        let Some(r) = row else { return };
        let mut tick = None;
        let mut open = false;
        ui.horizontal_wrapped(|ui| {
            let mut on = r.reviewed;
            if ui
                .checkbox(&mut on, "Reviewed")
                .on_hover_text(
                    "This joint is not laid out whole by the automatic layout: tick it once you \
                     have reviewed it. Stored with the joint (one undo step), cleared when its \
                     layout changes; your mark only, nothing is checked for it",
                )
                .changed()
            {
                tick = Some(on);
            }
            if r.cleared {
                ui.label(
                    egui::RichText::new("(ticked before; its layout changed since)")
                        .small()
                        .color(ui.visuals().warn_fg_color),
                );
            }
            if ui
                .small_button("Review the joints left…")
                .on_hover_text(
                    "Every joint not laid out whole, with why, grouped by reason: step through \
                     them in the Detail view and tick each one reviewed",
                )
                .clicked()
            {
                open = true;
            }
        });
        if let Some(on) = tick {
            self.set_review_tick(joint, on);
        }
        if open {
            self.open_review_window(Some(joint.to_owned()));
        }
    }
}

/// `s` cut to at most `n` characters (an ellipsis where cut).
fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
