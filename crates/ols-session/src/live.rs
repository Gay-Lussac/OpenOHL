// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A live model: a session whose line is kept solved.
//!
//! Every command, undo, redo or rollback goes through the session and then
//! re-solves only the sections it made dirty ([`SolvedLine::update`]) —
//! what the application does while a user drags a structure.

use ols_line::incremental::{SolvedLine, Update};
use ols_weather::WeatherCase;

use crate::{Applied, Command, Session, SessionError};

/// A session with its line solved in one weather case.
pub struct Live {
    session: Session,
    library: ols_cables::Library,
    solved: SolvedLine,
}

impl Live {
    /// Solves the session's line fully once.
    pub fn new(session: Session, library: ols_cables::Library, case: &WeatherCase) -> Self {
        let solved = SolvedLine::solve(session.line(), &library, case);
        Self {
            session,
            library,
            solved,
        }
    }

    /// The session.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The solutions.
    pub fn solved(&self) -> &SolvedLine {
        &self.solved
    }

    fn refresh(&mut self, applied: Applied) -> (Applied, Update) {
        let update = self
            .solved
            .update(self.session.line(), &self.library, &applied.sections);
        (applied, update)
    }

    /// Applies a command and re-solves what it touched.
    pub fn apply(
        &mut self,
        user: &str,
        command: Command,
    ) -> Result<(Applied, Update), SessionError> {
        let applied = self.session.apply(user, command)?;
        Ok(self.refresh(applied))
    }

    /// Undoes the user's last command and re-solves what it touched.
    pub fn undo(&mut self, user: &str) -> Result<(Applied, Update), SessionError> {
        let applied = self.session.undo(user)?;
        Ok(self.refresh(applied))
    }

    /// Redoes the user's last undo and re-solves what it touched.
    pub fn redo(&mut self, user: &str) -> Result<(Applied, Update), SessionError> {
        let applied = self.session.redo(user)?;
        Ok(self.refresh(applied))
    }

    /// Admin: reverses a log entry and re-solves what it touched.
    pub fn rollback(&mut self, admin: &str, seq: i64) -> Result<(Applied, Update), SessionError> {
        let applied = self.session.rollback(admin, seq)?;
        Ok(self.refresh(applied))
    }

    /// Admin: back to the state after entry `seq`, re-solving what changed.
    pub fn rollback_after(
        &mut self,
        admin: &str,
        seq: i64,
    ) -> Result<Option<(Applied, Update)>, SessionError> {
        Ok(self
            .session
            .rollback_after(admin, seq)?
            .map(|applied| self.refresh(applied)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, Role};
    use ols_core::units::Temperature;
    use ols_line::Line;
    use ols_line::jumper::solve_jumpers;
    use ols_line::solve::solve_line;
    use std::path::Path;

    fn library() -> ols_cables::Library {
        ols_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    /// The live solution equals a full solve of the current line.
    fn check(live: &Live) {
        let line = live.session().line();
        let lib = library();
        let case = live.solved().case();
        let full: Vec<_> = solve_line(line, &lib, case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(live.solved().shapes(line), full);
        let jumpers: Vec<_> = solve_jumpers(line, &lib, case, &full)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(live.solved().jumper_shapes(line), jumpers);
    }

    #[test]
    fn every_edit_re_solves_only_what_it_touched() {
        let dir = std::env::temp_dir().join(format!("ols-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.ols");
        std::fs::remove_file(&path).ok();
        let line = Line::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/synthetic-line/synthetic-line.ols.json"),
        )
        .unwrap();
        let mut session = Session::open(&path).unwrap();
        session.import(&line, "ana").unwrap();
        session.set_user("ana", "ben", Role::Designer).unwrap();
        let start = session.log().unwrap().last().unwrap().seq;
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut live = Live::new(session, library(), &case);
        check(&live);

        // T6 is on the branch only: 1 of 3 sections re-solved.
        let t6 = live.session().line().structure("T6").unwrap().position;
        let (_, u) = live
            .apply(
                "ben",
                Command::MoveStructure {
                    id: "T6".into(),
                    position: [t6[0] + 2.0, t6[1], t6[2]],
                },
            )
            .unwrap();
        assert_eq!((u.solved, u.kept), (1, 2));
        check(&live);
        // T3, the Y pylon, carries all three sections.
        let t3 = live.session().line().structure("T3").unwrap().position;
        let (_, u) = live
            .apply(
                "ana",
                Command::MoveStructure {
                    id: "T3".into(),
                    position: [t3[0], t3[1] + 1.0, t3[2]],
                },
            )
            .unwrap();
        assert_eq!((u.solved, u.kept), (3, 0));
        check(&live);
        // A jumper change re-solves the two sections it hangs its weight
        // on, not the third.
        let mut j = live.session().line().jumpers[0].clone();
        j.sag = Some(2.0);
        let (_, u) = live
            .apply(
                "ben",
                Command::Put {
                    kind: Kind::Jumper,
                    id: j.id.clone(),
                    value: serde_json::to_value(&j).unwrap(),
                    ord: None,
                },
            )
            .unwrap();
        assert_eq!((u.solved, u.kept), (2, 1));
        check(&live);
        // Delete S3 (with its jumper), undo, rollback everything.
        let (_, u) = live
            .apply(
                "ben",
                Command::Delete {
                    kind: Kind::Section,
                    id: "S3".into(),
                    confirm: true,
                },
            )
            .unwrap();
        // (S1 solved again: the jumper gone, its clamp carries less.)
        assert_eq!((u.solved, u.dropped, u.jumpers), (1, 1, 0));
        check(&live);
        let (_, u) = live.undo("ben").unwrap();
        // S3 back, and S1 (its jumper back on its clamp).
        assert_eq!(u.solved, 2);
        check(&live);
        live.rollback_after("ana", start).unwrap();
        check(&live);
        assert_eq!(live.session().export(), line);
    }
}
