// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! One model of the network as a standalone project file (`ohl network
//! export`, ADR-0013 "Export of one model as a project file"): today's
//! format (session schema 6), openable by the desktop alone.
//!
//! Every row of the model is written through the store interface
//! ([`ohl_session::store::copy::copy_all`]: objects of every project
//! line, the log, the spatial index, revisions with the blobs they use,
//! branches and tags, users, edit grants, reviews, surfaces), read at one
//! snapshot of the database (a writer working meanwhile changes nothing
//! of it). Its `ols_meta` gains:
//!
//! - `network.name`: the network's name (`net_meta` `name`, may be empty);
//! - `network.line`: the model's code;
//! - `network.revision`: the model's main branch revision when exported
//!   (empty: nothing committed);
//! - `network.library_pins`: the model's library pins (JSON, kind →
//!   release; phase 9);
//! - `network.library_withdrawn`: the withdrawn entries the model uses
//!   (JSON, `kind/entry`; it keeps them, shown as withdrawn);
//! - `network.exported`: when and by whom (JSON).
//!
//! **The library entries it uses stand in the file**: a line holds the
//! files of the releases it is pinned to as its own library files
//! (phase 9), so they are copied with its rows — the file opens and
//! solves alone, nothing else of the network's library comes along.
//!
//! The export is in the audit; it needs export on the whole line
//! (phase 7).

use std::path::Path;
use std::time::Instant;

use ohl_session::store::Store;
use ohl_session::store::sqlite::SqliteStore;
use serde::Serialize;

use crate::{NetError, Network, lock};

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportReport {
    /// The model's code.
    pub line: String,
    /// The file written.
    pub file: String,
    /// Object rows (of every project line).
    pub objects: usize,
    /// Log entries.
    pub entries: usize,
    /// Revisions.
    pub revisions: usize,
    /// The main branch's revision.
    pub revision: Option<String>,
    /// Structures of the main project line (read back from the file).
    pub structures: usize,
    /// Seconds.
    pub seconds: f64,
}

/// Writes model `code` of `net` as a new project file at `path` (refused
/// when it exists), as `user`.
pub fn export(
    net: &Network,
    code: &str,
    path: &Path,
    user: &str,
) -> Result<ExportReport, NetError> {
    let started = Instant::now();
    let store = net.store(code)?;
    if path.exists() {
        return Err(NetError::Refused(format!(
            "{}: exists already",
            path.display()
        )));
    }
    let shared = net.shared();
    // One snapshot for every read.
    {
        let mut c = lock(&shared);
        if !c.steps.is_empty() {
            return Err(NetError::Refused(
                "export during a unit of work of the same connection".into(),
            ));
        }
        c.client
            .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")?;
        c.steps.push("ohl_export".into());
    }
    let copied = (|| -> Result<_, NetError> {
        let file = SqliteStore::open(path)?;
        let (_, copied) = ohl_session::store::copy::copy_all(&store, &file, path)?;
        let revision = store
            .refs()?
            .into_iter()
            .find(|r| r.kind == "branch" && r.name == ohl_session::versions::MAIN)
            .and_then(|r| r.revision);
        let (name, pins, withdrawn): (Option<String>, String, Vec<String>) = {
            let mut c = lock(&shared);
            let name = c
                .query_opt("SELECT value FROM net_meta WHERE key = 'name'", &[])?
                .map(|r| r.get(0));
            let pins = c
                .query_one(
                    "SELECT library_pins::text FROM line WHERE id = $1",
                    &[&code],
                )?
                .get(0);
            // The withdrawn entries the model uses (it keeps them).
            let withdrawn = c
                .query(
                    "SELECT DISTINCT u.kind || '/' || u.entry FROM lib_use u
                     JOIN lib_withdrawn w ON w.kind = u.kind AND w.entry = u.entry
                     WHERE u.line_id = $1 AND u.branch = 'main' ORDER BY 1",
                    &[&code],
                )?
                .into_iter()
                .map(|r| r.get(0))
                .collect();
            (name, pins, withdrawn)
        };
        let time = store.now()?;
        file.set_meta("network.name", name.as_deref().unwrap_or_default())?;
        file.set_meta("network.line", code)?;
        file.set_meta("network.revision", revision.as_deref().unwrap_or_default())?;
        file.set_meta("network.library_pins", &pins)?;
        file.set_meta(
            "network.library_withdrawn",
            &serde_json::to_string(&withdrawn).expect("serialises"),
        )?;
        file.set_meta(
            "network.exported",
            &serde_json::json!({"time": time, "user": user}).to_string(),
        )?;
        file.checkpoint()?;
        Ok((copied, revision))
    })();
    {
        let mut c = lock(&shared);
        c.steps.clear();
        let _ = c.client.batch_execute("COMMIT");
    }
    let (copied, revision) = match copied {
        Ok(x) => x,
        Err(e) => {
            // Nothing half-written left behind.
            let _ = std::fs::remove_file(path);
            for end in ["-wal", "-shm"] {
                let mut p = path.as_os_str().to_owned();
                p.push(end);
                let _ = std::fs::remove_file(p);
            }
            return Err(e);
        }
    };
    // Read back as the desktop reads it.
    let check = ohl_session::Session::open_read_only(path)?;
    let structures = check.line().structures.len();
    drop(check);
    let report = ExportReport {
        line: code.to_owned(),
        file: path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        objects: copied.objects,
        entries: copied.entries,
        revisions: copied.revisions,
        revision,
        structures,
        seconds: started.elapsed().as_secs_f64(),
    };
    net.audit(
        user,
        "export",
        code,
        &serde_json::json!({
            "file": report.file,
            "objects": report.objects,
            "revision": report.revision,
        })
        .to_string(),
    )?;
    Ok(report)
}
