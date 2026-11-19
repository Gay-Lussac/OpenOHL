// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Working sets saved by name (ADR-0013, phase 6; schema 3): a user's
//! picks on one model ([`ohl_session::working_set::WorkingSetSpec`]),
//! kept in `working_set` by owner and name. Only the picks are kept: the
//! extent is worked out again from them on the model as it is when the
//! working set is opened (never a copy of what it held).

use ohl_session::working_set::{Pick, WorkingSetSpec};

use crate::{NetError, Network, lock};

/// A working set saved.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedWorkingSet {
    /// Its name, model and picks.
    pub spec: WorkingSetSpec,
    /// The routes its extent was on when saved.
    pub routes: Vec<String>,
    /// When it was saved (as the project file writes times).
    pub time: String,
}

/// Longest name of a saved working set, characters.
pub const NAME_MAX: usize = 80;

impl Network {
    /// Saves `spec` under its name for `owner` (replacing one of that
    /// name), with the routes its extent is on. Refused: no name, a name
    /// longer than [`NAME_MAX`], an unknown model, no pick.
    pub fn save_working_set(
        &self,
        owner: &str,
        spec: &WorkingSetSpec,
        routes: &[String],
    ) -> Result<(), NetError> {
        self.require_writer()?;
        let name = spec.name.trim();
        if name.is_empty() || name.chars().count() > NAME_MAX {
            return Err(NetError::Refused(format!(
                "a working set's name: 1 to {NAME_MAX} characters"
            )));
        }
        if spec.picks.is_empty() {
            return Err(NetError::Refused("a working set without picks".into()));
        }
        let picks = serde_json::to_string(&spec.picks).expect("serialises");
        let routes = serde_json::to_string(routes).expect("serialises");
        let mut c = lock(&self.conn);
        let known: bool = c
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM line WHERE id = $1)",
                &[&spec.model],
            )?
            .get(0);
        if !known {
            return Err(NetError::Refused(format!(
                "the network has no line {}",
                spec.model
            )));
        }
        c.execute(
            "INSERT INTO working_set (owner, name, routes, line_id, picks, time)
             VALUES ($1, $2, ($3::text)::jsonb, $4, $5, clock_timestamp())
             ON CONFLICT (owner, name) DO UPDATE SET routes = excluded.routes,
                 line_id = excluded.line_id, picks = excluded.picks, time = excluded.time",
            &[&owner, &name, &routes, &spec.model, &picks],
        )?;
        Ok(())
    }

    /// `owner`'s saved working sets, by name. One saved before schema 3
    /// (routes only, no model) reads as its routes picked whole, on no
    /// model.
    pub fn working_sets(&self, owner: &str) -> Result<Vec<SavedWorkingSet>, NetError> {
        let rows = lock(&self.conn).query(
            "SELECT name, COALESCE(line_id, ''), picks, routes::text, ohl_time(time)
             FROM working_set WHERE owner = $1 ORDER BY name",
            &[&owner],
        )?;
        let mut out = Vec::new();
        for r in rows {
            let name: String = r.get(0);
            let bad = |e: serde_json::Error| NetError::Database(format!("working set {name}: {e}"));
            let routes: Vec<String> = serde_json::from_str(r.get::<_, &str>(3)).map_err(bad)?;
            let mut picks: Vec<Pick> = serde_json::from_str(r.get::<_, &str>(2)).map_err(bad)?;
            if picks.is_empty() {
                picks = routes
                    .iter()
                    .map(|r| Pick::Route { route: r.clone() })
                    .collect();
            }
            out.push(SavedWorkingSet {
                spec: WorkingSetSpec {
                    name,
                    model: r.get(1),
                    picks,
                },
                routes,
                time: r.get(4),
            });
        }
        Ok(out)
    }

    /// Deletes `owner`'s working set `name`; whether there was one.
    pub fn delete_working_set(&self, owner: &str, name: &str) -> Result<bool, NetError> {
        self.require_writer()?;
        Ok(lock(&self.conn).execute(
            "DELETE FROM working_set WHERE owner = $1 AND name = $2",
            &[&owner, &name],
        )? > 0)
    }
}
