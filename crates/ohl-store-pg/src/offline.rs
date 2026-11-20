// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Offline working sets exported (ADR-0013, phase 12; schema 6): each
//! export's record in `offline_export` — who, which model, the server's
//! base revision, the export's own revision and its tree's text (each
//! object of the export by the hash of its JSON: no object kept again),
//! the working set's objects, routes and picks, the checks' reach. A
//! bundle brought back names the export's revision as its base: one the
//! network does not know is refused.

use ohl_session::Kind;
use ohl_session::working_set::WorkingSetSpec;

use crate::{NetError, Network, lock};

/// An offline working set exported.
#[derive(Debug, Clone, PartialEq)]
pub struct OfflineExport {
    /// Its number.
    pub id: i64,
    /// The model (line).
    pub model: String,
    /// Who exported it.
    pub user: String,
    /// When (as the project file writes times; empty before it is kept).
    pub time: String,
    /// The server's revision of the model's main line it was taken from.
    pub base: String,
    /// The export's own revision (the file's first).
    pub export: String,
    /// That revision's tree, as the file wrote it.
    pub tree: String,
    /// The working set's objects (editable offline).
    pub wanted: Vec<(Kind, String)>,
    /// Its routes.
    pub routes: Vec<String>,
    /// Its picks.
    pub spec: WorkingSetSpec,
    /// The checks' reach its neighbours were taken within, m.
    pub reach: f64,
    /// The bundles brought back from it: their heads and the branches made.
    pub brought_back: Vec<(String, String)>,
}

impl Network {
    /// A number for the next export (its revision's message names it).
    pub fn next_offline_export(&self) -> Result<i64, NetError> {
        self.require_writer()?;
        Ok(lock(&self.conn)
            .query_one("SELECT nextval('offline_export_id_seq')", &[])?
            .get(0))
    }

    /// Keeps export `e` (its number from [`Network::next_offline_export`]).
    pub fn record_offline_export(&self, e: &OfflineExport) -> Result<(), NetError> {
        self.require_writer()?;
        let wanted = serde_json::to_string(&e.wanted).expect("serialises");
        let routes = serde_json::to_string(&e.routes).expect("serialises");
        let spec = serde_json::to_string(&e.spec).expect("serialises");
        lock(&self.conn).execute(
            "INSERT INTO offline_export (id, line_id, user_name, base, export, tree, wanted,
                 routes, spec, reach)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            &[
                &e.id, &e.model, &e.user, &e.base, &e.export, &e.tree, &wanted, &routes, &spec,
                &e.reach,
            ],
        )?;
        Ok(())
    }

    /// The export whose own revision is `export` (none: not one of this
    /// network's).
    pub fn offline_export(&self, export: &str) -> Result<Option<OfflineExport>, NetError> {
        let row = lock(&self.conn).query_opt(
            "SELECT id, line_id, user_name, ohl_time(time), base, export, tree, wanted, routes,
                 spec, reach, brought_back
             FROM offline_export WHERE export = $1",
            &[&export],
        )?;
        let Some(r) = row else {
            return Ok(None);
        };
        let id: i64 = r.get(0);
        let bad = |e: serde_json::Error| NetError::Database(format!("offline export {id}: {e}"));
        Ok(Some(OfflineExport {
            id,
            model: r.get(1),
            user: r.get(2),
            time: r.get(3),
            base: r.get(4),
            export: r.get(5),
            tree: r.get(6),
            wanted: serde_json::from_str(r.get::<_, &str>(7)).map_err(bad)?,
            routes: serde_json::from_str(r.get::<_, &str>(8)).map_err(bad)?,
            spec: serde_json::from_str(r.get::<_, &str>(9)).map_err(bad)?,
            reach: r.get(10),
            brought_back: serde_json::from_str(r.get::<_, &str>(11)).map_err(bad)?,
        }))
    }

    /// Model `code`'s library pins (kind → release; none: not pinned).
    pub fn line_pins(
        &self,
        code: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, NetError> {
        let row = lock(&self.conn).query_opt(
            "SELECT COALESCE(library_pins::text, '{}') FROM line WHERE id = $1",
            &[&code],
        )?;
        let Some(r) = row else {
            return Err(NetError::Refused(format!("the network has no line {code}")));
        };
        serde_json::from_str(r.get::<_, &str>(0))
            .map_err(|e| NetError::Database(format!("line {code}'s library pins: {e}")))
    }

    /// Notes that the bundle of head `head` of export `id` came back onto
    /// branch `branch`.
    pub fn note_brought_back(&self, id: i64, head: &str, branch: &str) -> Result<(), NetError> {
        self.require_writer()?;
        let mut c = lock(&self.conn);
        let text: String = c
            .query_one(
                "SELECT brought_back FROM offline_export WHERE id = $1",
                &[&id],
            )?
            .get(0);
        let mut list: Vec<(String, String)> = serde_json::from_str(&text)
            .map_err(|e| NetError::Database(format!("offline export {id}: {e}")))?;
        list.push((head.to_owned(), branch.to_owned()));
        let text = serde_json::to_string(&list).expect("serialises");
        c.execute(
            "UPDATE offline_export SET brought_back = $2 WHERE id = $1",
            &[&id, &text],
        )?;
        Ok(())
    }
}
