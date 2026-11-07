// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Notices: the shared ones ([`ohl_ui::notices`]), with what is OHL-Design's
//! own. An edit's message says in a short sentence what happened ("Section S1
//! changed and recalculated."); the detailed text the edit gave (its log
//! entry, the sections and weather cases solved, the time) shows on hover
//! ([`plain`]).

pub use ohl_ui::notices::Notices;

/// How often the window is drawn again while an information fades, s (not
/// in the tests: their frames come as they step).
pub const TICK: Option<f64> = if cfg!(test) { None } else { Some(0.05) };

/// A message in a short human sentence, when it is one of the detailed
/// kinds (none: it is shown as it is):
///
/// - an edit: "changed section S1 (log entry 13): 1 sections touched, 1
///   re-solved over 1 weather cases (4 others …) in 954ms" → "Section S1
///   changed and recalculated." (nothing solved again: "… changed.");
/// - Save all: "saved: project file, …" → "Saved.";
/// - solved in the background: "solved: a, b" → "Solved a, b.".
pub fn plain(text: &str) -> Option<String> {
    if let Some((what, rest)) = text.split_once(" (log entry ")
        && let Some((_, detail)) = rest.split_once("): ")
    {
        let recalculated = detail
            .split_once(" re-solved")
            .and_then(|(head, _)| head.rsplit(' ').next())
            .and_then(|n| n.parse::<usize>().ok())
            .is_some_and(|n| n > 0);
        let done = done_sentence(what);
        return Some(if recalculated {
            format!("{done} and recalculated.")
        } else {
            format!("{done}.")
        });
    }
    if text.starts_with("saved: ") && !text.contains(" — ") {
        return Some("Saved.".into());
    }
    if let Some(cases) = text.strip_prefix("solved: ") {
        return Some(format!("Solved {cases}."));
    }
    None
}

/// What an edit did, as a sentence without its full stop: "changed
/// section S1" → "Section S1 changed", "undo" → "Undone".
fn done_sentence(what: &str) -> String {
    match what {
        "undo" => return "Undone".into(),
        "redo" => return "Redone".into(),
        _ => {}
    }
    // A verb in the past first ("changed", "moved", "deleted", "added",
    // "imported", "renamed"): the object first, the verb after.
    if let Some((verb, object)) = what.split_once(' ')
        && verb.ends_with("ed")
        && !object.is_empty()
        && !object.contains(" (")
        && !object.contains(" into ")
    {
        return format!("{} {verb}", capital(object));
    }
    capital(what)
}

/// `text` with a capital first letter.
fn capital(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A message as shown (the status line, a notice): its short sentence,
/// and the full text for the hover when they differ.
pub fn shown(text: &str) -> (String, Option<&str>) {
    match plain(text) {
        Some(p) => (p, Some(text)),
        None => (text.to_owned(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An edit's message in a short sentence, the detail kept.
    #[test]
    fn an_edit_is_said_in_a_short_sentence() {
        let long = "changed section S1 (log entry 13): 1 sections touched, 1 re-solved over 1 \
                    weather cases (4 others to solve again when wanted) in 954ms";
        assert_eq!(
            plain(long).as_deref(),
            Some("Section S1 changed and recalculated.")
        );
        let (short, detail) = shown(long);
        assert_eq!(short, "Section S1 changed and recalculated.");
        assert_eq!(detail, Some(long));
        let nothing = "moved structure T3 (log entry 2): 0 sections touched, 0 re-solved over 5 \
                       weather cases (0 others to solve again when wanted) in 3ms";
        assert_eq!(plain(nothing).as_deref(), Some("Structure T3 moved."));
        assert_eq!(
            plain("undo (log entry 4): 2 sections touched, 2 re-solved over 1 weather cases (0 others to solve again when wanted) in 5ms")
                .as_deref(),
            Some("Undone and recalculated.")
        );
        assert_eq!(plain("saved: project file").as_deref(), Some("Saved."));
        assert_eq!(plain("saved: nothing — project file: busy"), None);
        assert_eq!(plain("solved: wind").as_deref(), Some("Solved wind."));
        assert_eq!(plain("display weather cases set"), None);
    }
}
