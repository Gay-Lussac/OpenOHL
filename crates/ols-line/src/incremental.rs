// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Incremental solving: keep every section's solution and re-solve only
//! the sections an edit touched.
//!
//! A [`SolvedLine`] holds the result of each section (by id) and of each
//! jumper for one weather case. [`SolvedLine::update`] takes the edited
//! line and the ids of the sections to recalculate (from the session's
//! dirty tracking: changed sections, sections through changed
//! structures), re-solves those, solves new sections, drops deleted ones,
//! and refreshes the jumpers (cheap, and they hang on the solved clamps).
//!
//! Sections are independent finite-element models, so the result is the
//! same as solving everything again — checked by the tests after every
//! kind of edit.

use std::collections::BTreeMap;

use ols_weather::WeatherCase;
use ols_wirefe::default_fe_budget;

use crate::jumper::{JumperShape, solve_jumpers};
use crate::solve::{SectionShape, solve_section};
use crate::{Line, LineError};

/// What an update did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Update {
    /// Sections solved (changed or new).
    pub solved: usize,
    /// Sections dropped (deleted from the line).
    pub dropped: usize,
    /// Sections kept as they were.
    pub kept: usize,
    /// Jumpers solved.
    pub jumpers: usize,
}

/// Solutions of a line's sections and jumpers in one weather case.
#[derive(Debug, Clone)]
pub struct SolvedLine {
    case: WeatherCase,
    sections: BTreeMap<String, Result<SectionShape, LineError>>,
    jumpers: BTreeMap<String, Result<JumperShape, LineError>>,
}

fn solve_one(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    id: &str,
) -> Result<SectionShape, LineError> {
    let section = line
        .sections
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| LineError::Invalid(format!("no section {id}")))?;
    let cable = library.get(&section.cable).ok_or_else(|| {
        LineError::Invalid(format!(
            "section {}: cable \"{}\" not in the libraries",
            section.id, section.cable
        ))
    })?;
    solve_section(line, section, cable, case, &default_fe_budget())
}

impl SolvedLine {
    /// Solves every section and jumper of `line`.
    pub fn solve(line: &Line, library: &ols_cables::Library, case: &WeatherCase) -> Self {
        let mut solved = Self {
            case: case.clone(),
            sections: BTreeMap::new(),
            jumpers: BTreeMap::new(),
        };
        let all: Vec<String> = line.sections.iter().map(|s| s.id.clone()).collect();
        solved.update(line, library, &all);
        solved
    }

    /// The weather case.
    pub fn case(&self) -> &WeatherCase {
        &self.case
    }

    /// Brings the solutions up to date with `line` after an edit:
    /// re-solves the sections in `dirty`, solves sections not solved yet,
    /// drops sections no longer in the line, refreshes the jumpers.
    ///
    /// Expected cost: one section solve (typically < 1 ms) per dirty or new
    /// section, plus the jumpers.
    pub fn update(
        &mut self,
        line: &Line,
        library: &ols_cables::Library,
        dirty: &[String],
    ) -> Update {
        let mut update = Update::default();
        let before = self.sections.len();
        self.sections
            .retain(|id, _| line.sections.iter().any(|s| &s.id == id));
        update.dropped = before - self.sections.len();
        for section in &line.sections {
            if dirty.contains(&section.id) || !self.sections.contains_key(&section.id) {
                let result = solve_one(line, library, &self.case, &section.id);
                self.sections.insert(section.id.clone(), result);
                update.solved += 1;
            } else {
                update.kept += 1;
            }
        }
        let shapes = self.shapes(line);
        self.jumpers = line
            .jumpers
            .iter()
            .map(|j| j.id.clone())
            .zip(solve_jumpers(line, library, &self.case, &shapes))
            .collect();
        update.jumpers = self.jumpers.len();
        update
    }

    /// Solved sections in line order (failed ones left out).
    pub fn shapes(&self, line: &Line) -> Vec<SectionShape> {
        line.sections
            .iter()
            .filter_map(|s| self.sections.get(&s.id)?.as_ref().ok().cloned())
            .collect()
    }

    /// The result of every section, in line order.
    pub fn results<'a>(
        &'a self,
        line: &'a Line,
    ) -> Vec<(&'a str, &'a Result<SectionShape, LineError>)> {
        line.sections
            .iter()
            .filter_map(|s| self.sections.get(&s.id).map(|r| (s.id.as_str(), r)))
            .collect()
    }

    /// Solved jumpers in line order (failed ones left out).
    pub fn jumper_shapes(&self, line: &Line) -> Vec<JumperShape> {
        line.jumpers
            .iter()
            .filter_map(|j| self.jumpers.get(&j.id)?.as_ref().ok().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve::solve_line;
    use ols_core::units::Temperature;
    use std::path::Path;

    fn library() -> ols_cables::Library {
        ols_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    fn assert_same_as_full(line: &Line, solved: &SolvedLine) {
        let lib = library();
        let full: Vec<SectionShape> = solve_line(line, &lib, solved.case())
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(solved.shapes(line), full);
        let jumpers: Vec<JumperShape> = solve_jumpers(line, &lib, solved.case(), &full)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(solved.jumper_shapes(line), jumpers);
    }

    #[test]
    fn updates_match_a_full_solve() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut line = crate::tests::example();
        let mut solved = SolvedLine::solve(&line, &lib, &case);
        assert_same_as_full(&line, &solved);

        // Move T6 (branch only): only S3 is dirty.
        let t6 = line.structures.iter_mut().find(|s| s.id == "T6").unwrap();
        for a in &mut t6.attachments {
            a.insulator_point[0] += 3.0;
            a.wire_point[0] += 3.0;
        }
        t6.position[0] += 3.0;
        let u = solved.update(&line, &lib, &["S3".to_owned()]);
        assert_eq!((u.solved, u.kept, u.dropped), (1, 2, 0));
        assert_same_as_full(&line, &solved);

        // Change S1's stringing.
        line.sections[0].stringing = crate::Stringing::Tension {
            horizontal_tension: 23_000.0,
            temperature: 15.0,
        };
        let u = solved.update(&line, &lib, &["S1".to_owned()]);
        assert_eq!((u.solved, u.kept), (1, 2));
        assert_same_as_full(&line, &solved);

        // Delete S2, add a copy of S3 under a new id: dropped, solved.
        line.sections.remove(1);
        let mut extra = line.sections[1].clone();
        extra.id = "S3b".into();
        line.sections.push(extra);
        let u = solved.update(&line, &lib, &[]);
        assert_eq!((u.solved, u.kept, u.dropped), (1, 2, 1));
        assert_same_as_full(&line, &solved);
    }
}
