// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Use local conditions** (owner, 2026-11-03: "maybe it's easier to
//! remove the groups entirely and just have wind for static and wind for
//! internal clearances, with ice on the tower, and ice and wind on the
//! sections and to have the button to overwrite the values"): one switch
//! per check, in place of the structure and section overrides of the
//! weather case groups. Unticked, a check takes the weather cases as
//! written. Ticked, it computes on the line with its structures' local
//! conditions on the spans ([`ohl_line::local::localized`]): the wind for
//! strength ([`Purpose::Strength`]) or for clearances
//! ([`Purpose::Clearance`]) as the check needs, and the wire ice; each of
//! its weather cases is solved again on that line.
//!
//! **Cache.** The solves go through ohl-line's memo of whole solves
//! ([`ohl_line::solve::solve_line_memo`], the incremental solve's memo for
//! [`crate::model::Solved`]): its key is a fingerprint of every field of
//! the line solved, the library and the case — the localized line differs
//! from the model's by its spans' factors, so the key is in effect (line
//! version, purpose, case). Several checks of one purpose share the
//! solves; a line edit is another key. The memo keeps the solves of the
//! last four lines (the model's, the two localized ones and one more).
//!
//! **Time.** Each run of a check records how long it took, with or without
//! the local conditions ([`Local::finish`]); the switch's hover shows the
//! last of each (the project rule: every modelling setting shows its
//! effect and its time, measured).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_line::Line;
use ohl_line::local::{Purpose, SpanRule};

/// A run's times: the whole run, and its solves through [`Local`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunTime {
    /// The whole run.
    pub total: Duration,
    /// Its solves through [`Local::sections`] and [`Local::solved`] (zero
    /// when the check solves inside a library call).
    pub solving: Duration,
}

/// The last run time of each check: [without, with] the local conditions.
static RUNS: Mutex<BTreeMap<String, [Option<RunTime>; 2]>> = Mutex::new(BTreeMap::new());

/// The line a check computes on, with what the switch did.
#[derive(Debug)]
pub struct Local {
    /// The line: the model's, or localized.
    line: Line,
    /// Ticked.
    ticked: bool,
    /// Ticked and some structure gives local conditions: the line is
    /// localized and solved again.
    applied: bool,
    /// What the wind is taken for.
    purpose: Purpose,
    /// The structures giving local conditions.
    structures: usize,
    /// How the spans take their structures'.
    rule: SpanRule,
    /// Structures and sections still giving weather case groups.
    groups: (usize, usize),
    /// The time spent in this helper's solves (a check solving through
    /// it).
    solving: Mutex<(Duration, usize)>,
}

/// What a check computes on: `line` as it is (unticked, or no structure
/// gives local conditions — no other solve), else localized for
/// `purpose`.
pub fn prepare(line: &Line, ticked: bool, purpose: Purpose) -> Local {
    let structures = line
        .structures
        .iter()
        .filter(|s| !s.local.is_empty())
        .count();
    let applied = ticked && structures > 0;
    Local {
        line: if applied {
            ohl_line::local::localized(line, purpose)
        } else {
            line.clone()
        },
        ticked,
        applied,
        purpose,
        structures,
        rule: line.solver.local_spans,
        groups: (
            line.structures
                .iter()
                .filter(|s| !s.weather_groups.is_empty())
                .count(),
            line.sections
                .iter()
                .filter(|s| !s.weather_groups.is_empty())
                .count(),
        ),
        solving: Mutex::new((Duration::ZERO, 0)),
    }
}

/// The wind a purpose takes, in words.
pub fn wind_words(purpose: Purpose) -> &'static str {
    match purpose {
        Purpose::Strength => "wind for strength",
        Purpose::Clearance => "wind for clearances",
    }
}

impl Local {
    /// The line the check computes on.
    pub fn line(&self) -> &Line {
        &self.line
    }

    /// Whether the line is localized (ticked, and some structure gives
    /// local conditions).
    pub fn applied(&self) -> bool {
        self.applied
    }

    /// The check's notes about it: what was taken ("local conditions: N
    /// structures, spans worst; solved again"), ticked without any, and
    /// the weather groups no longer used (once per run).
    pub fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.applied {
            out.push(format!(
                "local conditions ({}, wire ice): {} structure(s), spans {}; solved again",
                wind_words(self.purpose),
                self.structures,
                self.rule.as_str()
            ));
        } else if self.ticked {
            out.push(
                "local conditions ticked: no structure gives any (Structures table) — the \
                 weather cases as written"
                    .into(),
            );
        }
        let (st, sec) = self.groups;
        if st > 0 {
            out.push(format!(
                "weather groups on structures are no longer used: give local conditions ({st} \
                 structure(s) with some)"
            ));
        }
        if sec > 0 {
            out.push(format!(
                "weather groups on sections are no longer used: give local winds and local ice \
                 ({sec} section(s) with some)"
            ));
        }
        out
    }

    /// The sections of `line` (the check's line, [`Local::line`], or one
    /// made from it: by a calculation) solved in `case` (as
    /// [`ohl_line::solve::solve_line_memo_default`], from the memo when
    /// solved so before), timed.
    pub fn sections(
        &self,
        line: &Line,
        library: &ohl_cables::Library,
        case: &ohl_weather::WeatherCase,
    ) -> Vec<Result<ohl_line::solve::SectionShape, ohl_line::LineError>> {
        let t = Instant::now();
        let r = ohl_line::solve::solve_line_memo_default(line, library, case);
        self.count(t.elapsed());
        r
    }

    /// The line solved in `case` as the model solves it ([`crate::model::Solved`]:
    /// sections and jumpers, the case's wind side as given), from the
    /// memo when solved so before.
    pub fn solved(
        &self,
        library: &ohl_cables::Library,
        case: ohl_weather::WeatherCase,
    ) -> crate::model::Solved {
        let t = Instant::now();
        let r = crate::model::Solved::new_memo(&self.line, library, case);
        self.count(t.elapsed());
        r
    }

    fn count(&self, d: Duration) {
        let mut s = self
            .solving
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        s.0 += d;
        s.1 += 1;
    }

    /// The time spent in [`Local::sections`] and [`Local::solved`], and
    /// how many solves.
    pub fn solve_time(&self) -> (Duration, usize) {
        *self
            .solving
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records the run of `check` that took `took` (with or without the
    /// local conditions, as applied) for the switch's hover.
    pub fn finish(&self, check: &str, took: Duration) {
        record(
            check,
            self.applied,
            RunTime {
                total: took,
                solving: self.solve_time().0,
            },
        );
    }
}

/// Records a run of `check` (with the local conditions applied or not).
pub fn record(check: &str, applied: bool, time: RunTime) {
    let mut runs = RUNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    runs.entry(check.to_owned()).or_default()[usize::from(applied)] = Some(time);
}

/// The last run times of `check`: [without, with] the local conditions.
pub fn last_runs(check: &str) -> [Option<RunTime>; 2] {
    RUNS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(check)
        .copied()
        .unwrap_or_default()
}

/// The switch's hover: what it does for this check (`what_for`: what the
/// check takes the wind for), its effect on this line, its time.
pub fn hover(check: &str, purpose: Purpose, what_for: &str, line: &Line) -> String {
    let n = line
        .structures
        .iter()
        .filter(|s| !s.local.is_empty())
        .count();
    let effect = if n == 0 {
        "On this line no structure gives local conditions: ticking it changes nothing (no \
         other solve)."
            .to_owned()
    } else {
        format!(
            "On this line {n} structure(s) give local conditions; the spans take the {} of their \
             two structures' (Line › Criteria › Calculation options).",
            line.solver.local_spans.as_str()
        )
    };
    let fmt = |r: Option<RunTime>| {
        r.map(|r| {
            let s = |d: Duration| format!("{:.2} s", d.as_secs_f64());
            if r.solving.is_zero() {
                s(r.total)
            } else {
                format!("{} (solving {})", s(r.total), s(r.solving))
            }
        })
    };
    let [without, with] = last_runs(check);
    let time = match (fmt(without), fmt(with)) {
        (Some(a), Some(b)) => format!("Time (the last runs): {b} with, {a} without."),
        (None, Some(b)) => format!("Time: {b} with (the last run; run it unticked to compare)."),
        (Some(a), None) => format!(
            "Time: {a} without (the last run); ticked, each weather case of the check is solved \
             again on the line with the local conditions (once: the solves are shared by the \
             checks of the same wind)."
        ),
        (None, None) => "Time: not measured yet (run the check).".to_owned(),
    };
    format!(
        "Ticked: the check computes on the line with the structures' local conditions (Structures \
         table: {wind}, wire ice) on their spans, each of its weather cases solved again; a span's \
         own local winds and local ice (Sections table) kept. Unticked: the weather cases as \
         written.\nThis check takes the {wind}: {what_for}.\n{effect}\n{time}",
        wind = wind_words(purpose),
    )
}

/// The switch, in a check's footer: whether it changed.
pub fn switch(
    ui: &mut egui::Ui,
    on: &mut bool,
    check: &str,
    purpose: Purpose,
    what_for: &str,
    line: &Line,
) -> bool {
    ui.checkbox(on, "Use local conditions")
        .on_hover_text(hover(check, purpose, what_for, line))
        .changed()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The synthetic example line and its library, and its "high wind"
    /// case.
    fn synthetic() -> (Line, ohl_cables::Library, ohl_weather::WeatherCase) {
        let m = crate::tests::synthetic();
        let wind = m
            .cases
            .iter()
            .find(|c| c.case.name == "high wind")
            .unwrap()
            .case
            .clone();
        (m.line, m.library, wind)
    }

    type Shapes = Vec<Result<ohl_line::solve::SectionShape, ohl_line::LineError>>;

    /// The horizontal tension of section S1's first span.
    fn h(shapes: &Shapes) -> f64 {
        shapes[0].as_ref().unwrap().spans[0].horizontal_tension
    }

    /// Ticked with a structure's wind for strength: the line localized,
    /// the tension larger than the base's; the notes say so; the
    /// clearance purpose takes the other wind (none given: as written).
    #[test]
    fn a_localized_solve_differs_from_the_base() {
        let (mut line, library, wind) = synthetic();
        let base = prepare(&line, true, Purpose::Strength);
        assert!(!base.applied());
        line.structures[0].local.wind_static = Some(1.5);
        let local = prepare(&line, true, Purpose::Strength);
        assert!(local.applied());
        assert_eq!(local.line().sections[0].wind_factor(0), 1.5);
        let (a, b) = (
            h(&base.sections(base.line(), &library, &wind)),
            h(&local.sections(local.line(), &library, &wind)),
        );
        assert!(b > a * 1.01, "{b} > {a}");
        assert!(
            local.notes()[0].contains("1 structure(s), spans worst; solved again"),
            "{:?}",
            local.notes()
        );
        assert_eq!(local.solve_time().1, 1);
        assert!(!prepare(&line, true, Purpose::Clearance).line().sections[0].has_local_wind());
        // Unticked: the line as it is.
        let off = prepare(&line, false, Purpose::Strength);
        assert!(!off.applied() && *off.line() == line && off.notes().is_empty());
    }

    /// Ticked without any local conditions: the line as it is, nothing
    /// solved anew (the same solution as the model's line, from the
    /// memo), the notes say so; the weather groups said once.
    #[test]
    fn nothing_local_nothing_solved_again() {
        let (mut line, library, wind) = synthetic();
        let local = prepare(&line, true, Purpose::Clearance);
        assert!(!local.applied());
        assert_eq!(*local.line(), line);
        assert!(local.notes()[0].contains("no structure gives any"));
        let a = local.sections(local.line(), &library, &wind);
        let b = ohl_line::solve::solve_line_memo_default(&line, &library, &wind);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        line.structures[1].weather_groups = vec!["short".into()];
        let notes = prepare(&line, false, Purpose::Clearance).notes();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].starts_with("weather groups on structures are no longer used"));
    }

    /// The run times are kept per check and shown in the hover.
    #[test]
    fn run_times_in_the_hover() {
        let (line, _, _) = synthetic();
        let check = "local-solve-test";
        assert!(hover(check, Purpose::Strength, "x", &line).contains("not measured yet"));
        let t = |total: u64, solving: u64| RunTime {
            total: Duration::from_millis(total),
            solving: Duration::from_millis(solving),
        };
        record(check, true, t(1500, 1000));
        record(check, false, t(500, 0));
        let h = hover(check, Purpose::Strength, "x", &line);
        assert!(
            h.contains("1.50 s (solving 1.00 s) with, 0.50 s without"),
            "{h}"
        );
        assert!(h.contains("no structure gives local conditions"), "{h}");
    }
}
