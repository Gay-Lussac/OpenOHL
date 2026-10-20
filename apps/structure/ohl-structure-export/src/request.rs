// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure side run headless for another program: a **request**
//! (JSON, `ohl-structure-request`) names a structure file, its section
//! library, a load table, the extension and the cases; the **reply**
//! (JSON, `ohl-structure-reply`) gives the run's usage per check and
//! case, its notes and, for a report, the files written. The command
//! line `ohl structure analyse|report` (a core crate, which never depends
//! on the structure application) starts `ohl-structure-report --request
//! -`, writes the request to its input and reads the reply from its
//! output: the applications talk, the crates stay apart.
//!
//! ```json
//! {"format": "ohl-structure-request", "version": 1, "action": "report",
//!  "structure": "Structures/type-02.ohls", "sections": ["Sections"],
//!  "loads": null, "extension": "+3", "cases": ["wind"],
//!  "report": "type-02-report.html", "date": false}
//! ```
//!
//! The reply always comes (also for a request that cannot run: `ok`
//! false and `error`); the tool's exit status is 0 when `ok`, else 1.

use std::path::{Path, PathBuf};

use ohl_structure_link::{StructureCase, StructureRun};
use ohl_structure_loads::LoadTable;
use serde::{Deserialize, Serialize};

use crate::report::{FromText, from_text, members_csv, today};

/// `format` of a request.
pub const REQUEST_FORMAT: &str = "ohl-structure-request";
/// `format` of a reply.
pub const REPLY_FORMAT: &str = "ohl-structure-reply";
/// Version of both (the newest read).
pub const PROTOCOL_VERSION: u32 = 1;

/// What is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Solve the cases: the usage per check and case.
    Analyse,
    /// The same, and write the structure report (HTML) and the members'
    /// CSV beside it.
    Report,
}

/// A request. Relative paths are taken from the tool's working folder
/// (the one of the program that starts it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// [`REQUEST_FORMAT`].
    pub format: String,
    /// [`PROTOCOL_VERSION`] or older.
    pub version: u32,
    /// What is asked.
    pub action: Action,
    /// The structure file (`.ohls`).
    pub structure: PathBuf,
    /// Section library files (`.ohlsec`) or folders, the first giving an
    /// id wins (none: the `Sections` folder beside the structure file, if
    /// there is one).
    #[serde(default)]
    pub sections: Vec<PathBuf>,
    /// A load table (CSV, as the structure application writes it) instead
    /// of the file's own load cases evaluated without line values.
    #[serde(default)]
    pub loads: Option<PathBuf>,
    /// The height extension to stand on (none: as written).
    #[serde(default)]
    pub extension: Option<String>,
    /// The cases to solve (empty: every case of the table).
    #[serde(default)]
    pub cases: Vec<String>,
    /// `report`: the report file (`.html`); the members' CSV goes beside
    /// it as `<stem>.members.csv`.
    #[serde(default)]
    pub report: Option<PathBuf>,
    /// `report`: write today's date into it.
    #[serde(default)]
    pub date: bool,
}

/// One check of one case, or over every case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckReply {
    /// Share of the capacity used (1 = 100 %; none: no member has a
    /// capacity, or the check did not run).
    pub usage: Option<f64>,
    /// What governs, in words.
    pub governs: String,
    /// Why it did not run (none: it ran).
    pub error: Option<String>,
    /// What was left out or approximated.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A check over every case: its largest usage and the case governing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryReply {
    /// `members` (finite elements) or `pole`.
    pub check: String,
    /// The largest usage (none: no case gives one).
    pub usage: Option<f64>,
    /// The case governing.
    pub case: String,
    /// What governs there.
    pub governs: String,
    /// Cases where it did not run: (case, why).
    #[serde(default)]
    pub failed: Vec<(String, String)>,
}

/// One case of the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseReply {
    /// The case.
    pub name: String,
    /// Its largest usage over its checks (an error: none, see the check).
    pub usage: Option<f64>,
    /// The finite-element check (none: the structure has no members).
    pub members: Option<CheckReply>,
    /// The pole check (none: no pole).
    pub pole: Option<CheckReply>,
}

/// The files a report wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilesReply {
    /// The report (HTML).
    pub report: PathBuf,
    /// The members' results (CSV).
    pub members_csv: PathBuf,
}

/// A reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    /// [`REPLY_FORMAT`].
    pub format: String,
    /// [`PROTOCOL_VERSION`].
    pub version: u32,
    /// The run went through (else `error` says why).
    pub ok: bool,
    /// Why it did not.
    pub error: Option<String>,
    /// The tool and its version.
    pub software: String,
    /// The structure's id.
    pub structure: String,
    /// The extension stood on (none: as written).
    pub extension: Option<String>,
    /// Its `dz`, m.
    pub dz: f64,
    /// What applies to every case.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Per check its largest usage over the cases.
    #[serde(default)]
    pub checks: Vec<SummaryReply>,
    /// Per case.
    #[serde(default)]
    pub cases: Vec<CaseReply>,
    /// The files written (`report`).
    pub files: Option<FilesReply>,
}

impl Reply {
    /// A reply saying why the request could not run.
    pub fn failed(software: &str, error: String) -> Self {
        Self {
            format: REPLY_FORMAT.into(),
            version: PROTOCOL_VERSION,
            ok: false,
            error: Some(error),
            software: software.into(),
            structure: String::new(),
            extension: None,
            dz: 0.0,
            notes: Vec::new(),
            checks: Vec::new(),
            cases: Vec::new(),
            files: None,
        }
    }

    /// The reply of a run.
    pub fn of(software: &str, structure: &str, run: &StructureRun) -> Self {
        Self {
            ok: true,
            error: None,
            structure: structure.into(),
            extension: run.extension.clone(),
            dz: run.dz,
            notes: run.notes.clone(),
            checks: run
                .checks()
                .into_iter()
                .map(|s| SummaryReply {
                    check: s.check.into(),
                    usage: s.usage,
                    case: s.case,
                    governs: s.governs,
                    failed: s.failed,
                })
                .collect(),
            cases: run.cases.iter().map(case_reply).collect(),
            ..Self::failed(software, String::new())
        }
    }

    /// As JSON (pretty).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("plain data")
    }
}

fn case_reply(c: &StructureCase) -> CaseReply {
    CaseReply {
        name: c.name.clone(),
        usage: c.usage().filter(|u| u.is_finite()),
        members: c.fe.as_ref().map(|r| match r {
            Ok(r) => CheckReply {
                usage: r.governing().and_then(|g| g.usage),
                governs: r.governing().map_or(String::new(), |g| {
                    format!("member {} ({})", g.id, g.governs)
                }),
                error: None,
                notes: r.notes.clone(),
            },
            Err(e) => CheckReply {
                usage: None,
                governs: String::new(),
                error: Some(e.clone()),
                notes: Vec::new(),
            },
        }),
        pole: c.pole.as_ref().map(|r| match r {
            Ok(p) => CheckReply {
                usage: Some(p.usage).filter(|u| u.is_finite()),
                governs: p.governs.clone(),
                error: None,
                notes: p.notes.clone(),
            },
            Err(e) => CheckReply {
                usage: None,
                governs: String::new(),
                error: Some(e.clone()),
                notes: Vec::new(),
            },
        }),
    }
}

/// Reads a request's text (its format and version checked).
pub fn parse_request(text: &str) -> Result<Request, String> {
    let r: Request = serde_json::from_str(text).map_err(|e| format!("request: {e}"))?;
    if r.format != REQUEST_FORMAT {
        return Err(format!(
            "request: format \"{}\" (expected {REQUEST_FORMAT})",
            r.format
        ));
    }
    if r.version > PROTOCOL_VERSION {
        return Err(format!(
            "request: version {} is newer than this tool reads ({PROTOCOL_VERSION}): rebuild \
             ohl-structure-report from the same code as ohl",
            r.version
        ));
    }
    if r.action == Action::Report && r.report.is_none() {
        return Err("request: a report needs \"report\" (the .html file to write)".into());
    }
    Ok(r)
}

/// Answers request `text`: the run's reply (a request that cannot run:
/// `ok` false with why). `software`: the tool and its version.
///
/// Expected cost: that of the run (a few milliseconds per case for a few
/// hundred members, each case bounded by 200 iterations and 30 s, as the
/// report), plus reading the files.
pub fn answer(text: &str, software: &str) -> Reply {
    match parse_request(text).and_then(|r| run(&r, software)) {
        Ok(reply) => reply,
        Err(e) => Reply::failed(software, e),
    }
}

fn run(r: &Request, software: &str) -> Result<Reply, String> {
    let file = &r.structure;
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let dir = file.parent();
    let paths: Vec<PathBuf> = if r.sections.is_empty() {
        dir.map(|d| d.join("Sections"))
            .filter(|p| p.exists())
            .into_iter()
            .collect()
    } else {
        r.sections.clone()
    };
    let (library, left_out) = crate::load_sections(&paths)?;
    let sections_from = if paths.is_empty() {
        "none (the structure's own [[section]] tables)".to_owned()
    } else {
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let (own, recipes) =
        crate::own_load_table(&text, &library).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut table = match &r.loads {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| format!("{}: {e}", p.display()))
            .and_then(|t| LoadTable::from_csv(&t).map_err(|e| format!("{}: {e}", p.display())))?,
        None => own,
    };
    if !r.cases.is_empty() {
        let known = table.cases();
        if let Some(c) = r.cases.iter().find(|c| !known.contains(&c.as_str())) {
            return Err(format!("no case \"{c}\" in the load table"));
        }
        table.rows.retain(|row| r.cases.contains(&row.case));
    }
    let date = r.date.then(today);
    let report = from_text(&FromText {
        text: &text,
        file: &file.display().to_string(),
        dir,
        sections: &library,
        sections_from: &sections_from,
        table: &table,
        recipes: &recipes,
        extension: r.extension.as_deref(),
        case: None,
        incomplete: &[],
        software,
        date: date.as_deref(),
    })
    .map_err(|e| format!("{}: {e}", file.display()))?;
    let id = structure_id(&text);
    let mut reply = Reply::of(software, &id, &report.run);
    reply.notes.extend(
        left_out
            .into_iter()
            .map(|id| format!("section library: \"{id}\" given twice, the first one kept")),
    );
    if r.action == Action::Report {
        let out = r.report.as_ref().expect("checked when read");
        let csv = out.with_extension("members.csv");
        let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
        std::fs::write(out, &report.html).map_err(|e| io(out, e))?;
        std::fs::write(&csv, members_csv(&report.run)).map_err(|e| io(&csv, e))?;
        reply.files = Some(FilesReply {
            report: out.clone(),
            members_csv: csv,
        });
    }
    Ok(reply)
}

fn structure_id(text: &str) -> String {
    use ohl_structures::LibraryItem as _;
    ohl_structures::StructureType::parse(Path::new("structure.ohls"), text)
        .map_or(String::new(), |t| t.id)
}
