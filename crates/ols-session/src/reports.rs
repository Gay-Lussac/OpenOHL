// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The library of performed checks: a check's report kept as an object of
//! the project line it was made on (kind [`Kind::Report`]), so it belongs
//! to that line's branch and moves with it — commits, checkout, merges,
//! the bundles carrying a branch to an offline copy — and saving,
//! replacing or deleting one is a logged, undoable command like any edit.
//!
//! A report holds who made it and when, the app (and the plugin) that
//! made it, every parameter, the results (check-specific JSON), a status
//! and a note, and the **state of the line**: the log entry it was made
//! after and a fingerprint (SHA-256) of every object and project library
//! file, so that reopened it tells whether the line changed since, and
//! what. Reports are not part of the line ([`ols_line::Line`]): the line
//! file and the views never see them; a line replaced (an import) keeps
//! them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Command, Kind, Session, SessionError, refused};

/// File format id of an exported report.
pub const REPORT_FILE_FORMAT: &str = "ohl-check-report";
/// Newest version read.
pub const REPORT_FILE_VERSION: u32 = 1;
/// File extension of an exported report.
pub const REPORT_FILE_EXTENSION: &str = "ohlcr";

/// Where a report is at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    /// Saved, not looked at yet.
    #[default]
    Draft,
    /// Looked at.
    Checked,
    /// Approved.
    Approved,
}

impl ReportStatus {
    /// Every status, in order.
    pub const ALL: [Self; 3] = [Self::Draft, Self::Checked, Self::Approved];

    /// Its name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Checked => "checked",
            Self::Approved => "approved",
        }
    }
}

/// The state of the line a report was made on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineState {
    /// The project line (branch).
    pub line: String,
    /// The last log entry of that line when it was made (0: none).
    pub seq: i64,
    /// Fingerprint of every object (`kind:id`) and project library file
    /// (`library:kind/name`): SHA-256 of its JSON or text.
    pub objects: BTreeMap<String, String>,
}

/// What changed between two states of the line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changed {
    /// Objects and library files added since (`kind:id`).
    pub added: Vec<String>,
    /// Removed since.
    pub removed: Vec<String>,
    /// Changed since.
    pub changed: Vec<String>,
}

impl Changed {
    /// Nothing changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// In words: "line unchanged", or the counts by kind, e.g. "changed
    /// since: 4 sections, 2 structures changed; 1 jumper added".
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "line unchanged".into();
        }
        let counts = |keys: &[String], what: &str| -> Option<String> {
            if keys.is_empty() {
                return None;
            }
            let mut by: BTreeMap<&str, usize> = BTreeMap::new();
            for k in keys {
                *by.entry(k.split_once(':').map_or(k.as_str(), |x| x.0))
                    .or_default() += 1;
            }
            let list: Vec<String> = by.iter().map(|(k, n)| format!("{n} {k}")).collect();
            Some(format!("{} {what}", list.join(", ")))
        };
        let parts: Vec<String> = [
            counts(&self.changed, "changed"),
            counts(&self.added, "added"),
            counts(&self.removed, "removed"),
        ]
        .into_iter()
        .flatten()
        .collect();
        format!("changed since: {}", parts.join("; "))
    }
}

/// What changed from `then` to `now` (fingerprints compared).
pub fn changed_since(then: &LineState, now: &LineState) -> Changed {
    let mut out = Changed::default();
    for (k, h) in &now.objects {
        match then.objects.get(k) {
            None => out.added.push(k.clone()),
            Some(g) if g != h => out.changed.push(k.clone()),
            Some(_) => {}
        }
    }
    out.removed = then
        .objects
        .keys()
        .filter(|k| !now.objects.contains_key(*k))
        .cloned()
        .collect();
    out
}

fn report_format() -> String {
    REPORT_FILE_FORMAT.to_owned()
}

fn first_version() -> u32 {
    1
}

/// A performed check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckReport {
    /// [`REPORT_FILE_FORMAT`] (optional in files, as every header).
    #[serde(default = "report_format")]
    pub format: String,
    /// [`REPORT_FILE_VERSION`] (none: the first).
    #[serde(default = "first_version")]
    pub version: u32,
    /// Its name: unique among the line's reports.
    pub id: String,
    /// The check that made it, e.g. `lev-ch.phase-distance`.
    pub check: String,
    /// The check in words.
    pub title: String,
    /// When, UTC (`YYYY-MM-DDTHH:MM:SSZ`).
    pub time: String,
    /// Who.
    pub user: String,
    /// The application and its version.
    pub app: String,
    /// The plugin that made it, with its version, if one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// Where it is at.
    #[serde(default)]
    pub status: ReportStatus,
    /// A note.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The state of the line it was made on.
    pub state: LineState,
    /// The result in one line.
    pub summary: String,
    /// Every parameter (check-specific).
    pub parameters: Value,
    /// The results (check-specific).
    pub results: Value,
}

impl CheckReport {
    /// Reads an exported report.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut r: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        ols_core::files::check_header(
            Some(&r.format),
            Some(r.version),
            REPORT_FILE_FORMAT,
            REPORT_FILE_VERSION,
            "check report",
        )?;
        // Read with the id of before 2026-10-18 too; kept with the current.
        r.format = REPORT_FILE_FORMAT.into();
        if r.id.trim().is_empty() {
            return Err("a check report needs a name".into());
        }
        Ok(r)
    }

    /// As an exported file.
    pub fn to_text(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialises")
    }

    /// The command keeping it on the current line (replacing a report of
    /// its name).
    pub fn put(&self) -> Command {
        Command::Put {
            kind: Kind::Report,
            id: self.id.clone(),
            value: serde_json::to_value(self).expect("serialises"),
            ord: None,
        }
    }
}

/// The command deleting report `id`.
pub fn delete(id: &str) -> Command {
    Command::Delete {
        kind: Kind::Report,
        id: id.to_owned(),
        confirm: false,
    }
}

/// A report's value as kept, checked.
pub(crate) fn check_value(id: &str, value: &Value) -> Result<(), SessionError> {
    let r: CheckReport =
        serde_json::from_value(value.clone()).map_err(|e| refused(format!("report {id}: {e}")))?;
    if r.id != id {
        return Err(refused(format!("report {id}: its \"id\" field differs")));
    }
    Ok(())
}

impl Session {
    /// The state of the current line now: its last log entry and the
    /// fingerprints of its objects (reports left out) and of the
    /// project's library files.
    pub fn check_state(&self) -> Result<LineState, SessionError> {
        let seq = self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM oplog WHERE line = ?1",
            [self.current_line()],
            |r| r.get(0),
        )?;
        let objects: BTreeMap<String, String> = self
            .objects
            .iter()
            .filter(|((k, _), _)| *k != Kind::Report)
            .map(|((k, id), o)| {
                // A library file by its text (`library:kind/name`, as
                // fingerprints were before libraries were objects).
                let text = match k {
                    Kind::Library => crate::library_entry(&o.value)
                        .map_or_else(|| o.value.to_string(), |(_, _, t)| t),
                    _ => o.value.to_string(),
                };
                (format!("{}:{id}", k.as_str()), crate::versions::hash(&text))
            })
            .collect();
        Ok(LineState {
            line: self.current_line().to_owned(),
            seq,
            objects,
        })
    }

    /// The current line's reports, by name (those that do not read are
    /// left out).
    pub fn reports(&self) -> Vec<CheckReport> {
        self.objects
            .iter()
            .filter(|((k, _), _)| *k == Kind::Report)
            .filter_map(|(_, o)| serde_json::from_value(o.value.clone()).ok())
            .collect()
    }

    /// The current line's report `id`.
    pub fn report(&self, id: &str) -> Option<CheckReport> {
        self.objects
            .get(&(Kind::Report, id.to_owned()))
            .and_then(|o| serde_json::from_value(o.value.clone()).ok())
    }

    /// The time now, UTC (`YYYY-MM-DDTHH:MM:SSZ`).
    pub fn now(&self) -> Result<String, SessionError> {
        Ok(self
            .conn
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now')", [], |r| {
                r.get(0)
            })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn project(name: &str) -> (PathBuf, Session) {
        let dir = std::env::temp_dir().join(format!("ols-reports-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.ohl");
        std::fs::remove_file(&path).ok();
        let mut s = Session::open(&path).unwrap();
        let line = ols_line::Line::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
        )
        .unwrap();
        s.import(&line, "ana").unwrap();
        s.set_user("ana", "ben", crate::Role::Designer).unwrap();
        s.set_user("ana", "vic", crate::Role::Viewer).unwrap();
        (path, s)
    }

    fn report(s: &Session, id: &str) -> CheckReport {
        CheckReport {
            format: REPORT_FILE_FORMAT.into(),
            version: REPORT_FILE_VERSION,
            id: id.into(),
            check: "test.check".into(),
            title: "A check".into(),
            time: s.now().unwrap(),
            user: "ben".into(),
            app: "OpenOHL test".into(),
            plugin: None,
            status: ReportStatus::Draft,
            note: String::new(),
            state: s.check_state().unwrap(),
            summary: "2 pairs below".into(),
            parameters: serde_json::json!({"spacing": 1.0}),
            results: serde_json::json!([{"d": 4.5}]),
        }
    }

    /// Saved as an object of the line (logged, undone like an edit), not
    /// part of the line; exported and read back; a viewer may not save.
    #[test]
    fn saved_with_the_line() {
        let (_p, mut s) = project("saved");
        let r = report(&s, "LeV 10 °C");
        assert!(s.apply("vic", r.put()).is_err(), "viewers may not");
        s.apply("ben", r.put()).unwrap();
        assert_eq!(s.reports(), vec![r.clone()]);
        assert_eq!(s.report("LeV 10 °C"), Some(r.clone()));
        // The line itself does not change.
        assert!(s.export().validate().is_ok());
        // An export reads back.
        assert_eq!(CheckReport::parse(&r.to_text()).unwrap(), r);
        assert!(CheckReport::parse("{\"format\": \"x\"}").is_err());
        // Undone like any edit.
        s.undo("ben").unwrap();
        assert!(s.reports().is_empty());
        s.redo("ben").unwrap();
        assert_eq!(s.reports().len(), 1);
        // Status changed: the same report replaced.
        let mut checked = r.clone();
        checked.status = ReportStatus::Checked;
        s.apply("ben", checked.put()).unwrap();
        assert_eq!(s.reports(), vec![checked]);
        // A bad report is refused.
        let bad = Command::Put {
            kind: Kind::Report,
            id: "x".into(),
            value: serde_json::json!({"id": "x"}),
            ord: None,
        };
        assert!(s.apply("ben", bad).is_err());
        s.apply("ben", delete("LeV 10 °C")).unwrap();
        assert!(s.reports().is_empty());
    }

    /// The line replaced (an import) keeps the reports; the state of the
    /// line tells what changed since a report was made.
    #[test]
    fn state_of_the_line() {
        let (_p, mut s) = project("state");
        let r = report(&s, "r");
        s.apply("ben", r.put()).unwrap();
        let now = s.check_state().unwrap();
        assert!(
            changed_since(&r.state, &now).is_empty(),
            "a report changes nothing"
        );
        assert_eq!(changed_since(&r.state, &now).summary(), "line unchanged");
        assert!(now.seq > r.state.seq);
        let p = s.line().structure("T2").unwrap().position;
        s.apply(
            "ben",
            Command::MoveStructure {
                id: "T2".into(),
                position: [p[0] + 1.0, p[1], p[2]],
            },
        )
        .unwrap();
        let c = changed_since(&r.state, &s.check_state().unwrap());
        assert!(c.changed.contains(&"structure:T2".to_owned()), "{c:?}");
        assert!(c.summary().starts_with("changed since:"), "{}", c.summary());
        // A line replaced: the report stays.
        let mut line = s.export();
        line.structures[0].description = "replaced".into();
        s.apply(
            "ben",
            Command::ReplaceLine {
                line: Box::new(line),
            },
        )
        .unwrap();
        assert_eq!(s.reports().len(), 1);
    }

    /// A report belongs to the project line (branch) it was saved on.
    #[test]
    fn on_its_line() {
        let (_p, mut s) = project("lines");
        s.create_line("ben", "phase 1", "main").unwrap();
        s.select_line("phase 1").unwrap();
        s.apply("ben", report(&s, "on phase 1").put()).unwrap();
        assert_eq!(s.check_state().unwrap().line, "phase 1");
        assert_eq!(s.reports().len(), 1);
        s.select_line("main").unwrap();
        assert!(s.reports().is_empty());
        // A line made from it takes its reports along.
        s.create_line("ben", "phase 2", "phase 1").unwrap();
        s.select_line("phase 2").unwrap();
        assert_eq!(s.reports().len(), 1);
    }
}
