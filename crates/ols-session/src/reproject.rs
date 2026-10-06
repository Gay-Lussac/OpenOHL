// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Moving a project into another coordinate system: the working line, the
//! project lines saved and the terrain surfaces, together. (Point clouds
//! are kept by `ols-pointcloud`; the application moves them too.)

use rusqlite::params;

use crate::{Applied, Command, Role, Session, SessionError, objects_of, refused};

/// Moves points in place (e.g. an `ols_crs::Transformation`).
pub type Move<'a> = &'a dyn Fn(&mut [[f64; 3]]) -> Result<(), String>;

impl Session {
    /// Moves the project into coordinate system `crs` (what
    /// `ols_crs::Crs::parse` takes) with `transform`: the working line (one
    /// log entry, kind `reproject`, which cannot be undone — the surfaces
    /// would not follow), every project line saved and every surface
    /// (triangles kept). Heights are whatever `transform` makes of them.
    /// Admins only: it changes everything at once.
    ///
    /// Cost: the line's objects are all written again; each surface is
    /// decoded and encoded (~50 ns per point). Run it on a copy
    /// ([`Session::copy_to`]): on an error the project may be half moved.
    pub fn reproject(
        &mut self,
        user: &str,
        crs: &str,
        transform: Move<'_>,
    ) -> Result<Applied, SessionError> {
        self.require(user, Role::Admin)?;
        let moved = |line: &ols_line::Line, what: &str| -> Result<ols_line::Line, SessionError> {
            let mut line = line.clone();
            line.transform_coordinates(transform)
                .map_err(|e| refused(format!("{what}: {e}")))?;
            line.crs = crs.to_owned();
            line.validate()
                .map_err(|e| refused(format!("{what} after the move: {e}")))?;
            Ok(line)
        };
        let line = moved(&self.line, "the working line")?;
        let mut snapshots = Vec::new();
        for s in self.snapshots()? {
            let saved = self.snapshot(&s.name)?;
            snapshots.push((
                s.name.clone(),
                moved(&saved, &format!("project line {}", s.name))?,
            ));
        }
        let mut surfaces = Vec::new();
        for s in self.surfaces()? {
            let (mut points, triangles) = self.surface(&s.name)?;
            transform(&mut points).map_err(|e| refused(format!("surface {}: {e}", s.name)))?;
            surfaces.push((s, points, triangles));
        }
        let commands = objects_of(&line)
            .into_iter()
            .map(|(kind, id, value)| Command::Put {
                kind,
                id,
                value,
                ord: None,
            })
            .collect();
        let applied = self.apply_as(
            user,
            Command::Batch { commands },
            "reproject",
            None,
            false,
            false,
        )?;
        for (name, saved) in snapshots {
            let text = serde_json::to_string(&saved).map_err(|e| refused(e.to_string()))?;
            self.conn.execute(
                "UPDATE line_snapshot SET json = ?2 WHERE name = ?1",
                params![name, text],
            )?;
        }
        for (s, points, triangles) in surfaces {
            self.put_surface(user, &s.name, &s.source, &points, &triangles)?;
        }
        Ok(applied)
    }
}
