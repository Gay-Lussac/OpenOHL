// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A searchable tick list, shared: a search field that filters the rows
//! as you type (by label or key, a substring, upper and lower case
//! alike), **All**, **None** and **Invert** acting on the rows shown only,
//! a count "n of m ticked", and the rows in a scroll box of fixed height
//! (only the rows in view are laid out, so hundreds stay quick).
//!
//! What is ticked is `None` (every row, also rows added later) or the
//! keys ticked.

use eframe::egui;

/// A row of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// What is kept when it is ticked.
    pub key: String,
    /// What the row shows.
    pub label: String,
}

impl Item {
    /// A row.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }

    /// Whether the search text `q` (lower case) finds it.
    fn found(&self, q: &str) -> bool {
        q.is_empty() || self.label.to_lowercase().contains(q) || self.key.to_lowercase().contains(q)
    }
}

/// Whether `key` is ticked.
pub fn ticked(chosen: &Option<Vec<String>>, key: &str) -> bool {
    chosen.as_ref().is_none_or(|c| c.iter().any(|x| x == key))
}

/// The rows `q` finds (indices).
pub fn filtered(items: &[Item], q: &str) -> Vec<usize> {
    let q = q.trim().to_lowercase();
    (0..items.len()).filter(|&k| items[k].found(&q)).collect()
}

/// What the buttons do to the rows `shown`: tick them all, none, or
/// invert them; the rows not shown keep their ticks. All with every row
/// shown is `None` (every row, also rows added later).
pub fn apply(items: &[Item], shown: &[usize], chosen: &mut Option<Vec<String>>, action: Action) {
    if action == Action::All && shown.len() == items.len() {
        *chosen = None;
        return;
    }
    let list = chosen.get_or_insert_with(|| items.iter().map(|x| x.key.clone()).collect());
    for &k in shown {
        let key = &items[k].key;
        let on = list.iter().any(|x| x == key);
        let want = match action {
            Action::All => true,
            Action::None => false,
            Action::Invert => !on,
        };
        if want && !on {
            list.push(key.clone());
        } else if !want && on {
            list.retain(|x| x != key);
        }
    }
}

/// A button of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Tick the rows shown.
    All,
    /// Untick them.
    None,
    /// Tick those unticked and untick the others.
    Invert,
}

/// The list: search field, buttons, count, rows in a box `height` high.
/// True when the ticks changed.
pub fn tick_list(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    items: &[Item],
    chosen: &mut Option<Vec<String>>,
    height: f32,
) -> bool {
    let id = ui.make_persistent_id(id);
    let key = id.with("search");
    let mut q: String = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    let mut changed = false;
    ui.horizontal(|ui| {
        let label = ui.label("search");
        ui.add(
            egui::TextEdit::singleline(&mut q)
                .hint_text("name or id")
                .desired_width(140.0),
        )
        .labelled_by(label.id)
        .on_hover_text(
            "Shows the rows whose name or id contains this (upper and lower case alike)",
        );
        if !q.is_empty()
            && ui
                .small_button("×")
                .on_hover_text("Clear the search")
                .clicked()
        {
            q.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(key, q.clone()));
    let shown = filtered(items, &q);
    ui.horizontal(|ui| {
        for (action, name, tip) in [
            (
                Action::All,
                "All",
                "Tick the rows shown (every row: also rows added later)",
            ),
            (Action::None, "None", "Untick the rows shown"),
            (
                Action::Invert,
                "Invert",
                "Tick the rows shown that are not, untick the others",
            ),
        ] {
            if ui.small_button(name).on_hover_text(tip).clicked() {
                apply(items, &shown, chosen, action);
                changed = true;
            }
        }
        let n = items.iter().filter(|x| ticked(chosen, &x.key)).count();
        let mut count = format!("{n} of {} ticked", items.len());
        if shown.len() < items.len() {
            count.push_str(&format!(" ({} shown)", shown.len()));
        }
        ui.label(egui::RichText::new(count).small().weak());
    });
    egui::Frame::group(ui.style())
        .inner_margin(4.0)
        .show(ui, |ui| {
            let row = ui.spacing().interact_size.y;
            egui::ScrollArea::vertical()
                .id_salt(id.with("rows"))
                .min_scrolled_height(height)
                .max_height(height)
                .auto_shrink([true, false])
                .show_rows(ui, row, shown.len(), |ui, range| {
                    ui.set_min_width(240.0);
                    if shown.is_empty() {
                        ui.label(egui::RichText::new("nothing found").small().weak());
                    }
                    for &k in &shown[range] {
                        let item = &items[k];
                        let mut on = ticked(chosen, &item.key);
                        if ui.checkbox(&mut on, &item.label).changed() {
                            let list = chosen.get_or_insert_with(|| {
                                items.iter().map(|x| x.key.clone()).collect()
                            });
                            if on {
                                list.push(item.key.clone());
                            } else {
                                list.retain(|x| x != &item.key);
                            }
                            changed = true;
                        }
                    }
                });
        });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn items() -> Vec<Item> {
        vec![
            Item::new("a1", "Alpha 1"),
            Item::new("a2", "alpha 2"),
            Item::new("b1", "Beta 1"),
        ]
    }

    /// Typed "ALPHA": two rows shown; All ticks those two only, Invert
    /// flips them, the third row keeps its tick; All with nothing typed is
    /// every row.
    #[test]
    fn search_then_all_ticks_the_rows_found_only() {
        let mut h = Harness::new_ui_state(
            |ui, chosen: &mut Option<Vec<String>>| {
                tick_list(ui, "test", &items(), chosen, 120.0);
            },
            Some(Vec::new()),
        );
        h.run();
        h.get_by_label("0 of 3 ticked");
        h.get_by_role_and_label(egui::accesskit::Role::TextInput, "search")
            .focus();
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::TextInput, "search")
            .type_text("ALPHA");
        h.run();
        h.get_by_label("0 of 3 ticked (2 shown)");
        assert!(h.query_by_label("Beta 1").is_none());
        h.get_by_label("All").click();
        h.run();
        assert_eq!(
            h.state().as_deref(),
            Some(&["a1".to_owned(), "a2".to_owned()][..])
        );
        h.get_by_label("Invert").click();
        h.run();
        assert_eq!(h.state().as_deref(), Some(&[][..]));
        // The search cleared: All is every row.
        h.get_by_label("×").click();
        h.run();
        h.get_by_label("Beta 1").click();
        h.run();
        assert_eq!(h.state().as_deref(), Some(&["b1".to_owned()][..]));
        h.get_by_label("All").click();
        h.run();
        assert_eq!(*h.state(), None);
        h.get_by_label("3 of 3 ticked");
        h.get_by_label("None").click();
        h.run();
        assert_eq!(h.state().as_deref(), Some(&[][..]));
    }

    /// Hundreds of rows: the box keeps its height (only the rows in view
    /// are laid out).
    #[test]
    fn many_rows_scroll_in_a_fixed_box() {
        let many: Vec<Item> = (0..500)
            .map(|k| Item::new(format!("S1:{k}"), format!("S1 span {k}")))
            .collect();
        let mut h = Harness::new_ui_state(
            move |ui, chosen: &mut Option<Vec<String>>| {
                tick_list(ui, "many", &many, chosen, 120.0);
            },
            None,
        );
        h.run();
        h.get_by_label("500 of 500 ticked");
        assert!(h.query_by_label("S1 span 0").is_some());
        assert!(h.query_by_label("S1 span 499").is_none());
        assert!(h.get_by_label("S1 span 0").rect().height() < 40.0);
    }
}
