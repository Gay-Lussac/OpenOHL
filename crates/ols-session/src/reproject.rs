// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Moving a project into another coordinate system: every project line
//! and the terrain surfaces, together. (Point clouds
//! are kept by `ols-pointcloud`; the application moves them too.)

use crate::{Command, Role, Session, SessionError, objects_of, refused};

/// Moves points in place (e.g. an `ols_crs::Transformation`).
pub type Move<'a> = &'a dyn Fn(&mut [[f64; 3]]) -> Result<(), String>;

impl Session {
    /// Moves the project into coordinate system `crs` (what
    /// `ols_crs::Crs::parse` takes) with `transform`: every project line
    /// (one log entry each, kind `reproject`, which cannot be undone — the
    /// surfaces would not follow) and every surface (triangles kept).
    /// Returns the number of lines moved. Heights are whatever `transform` makes of them.
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
    ) -> Result<usize, SessionError> {
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
        // Every line and surface moved first: a failure changes nothing.
        let mut lines = Vec::new();
        for info in self.lines()? {
            let line = self.line_named(&info.name)?;
            if line.structures.is_empty() && line.sections.is_empty() && line.name.is_empty() {
                continue;
            }
            lines.push((
                info.name.clone(),
                moved(line, &format!("project line {}", info.name))?,
            ));
        }
        let mut surfaces = Vec::new();
        for s in self.surfaces()? {
            let (mut points, triangles) = self.surface(&s.name)?;
            transform(&mut points).map_err(|e| refused(format!("surface {}: {e}", s.name)))?;
            surfaces.push((s, points, triangles));
        }
        for (name, line) in &lines {
            let commands = objects_of(line)
                .into_iter()
                .map(|(kind, id, value)| Command::Put {
                    kind,
                    id,
                    value,
                    ord: None,
                })
                .collect();
            self.with_line(name, |s| {
                s.apply_as(
                    user,
                    Command::Batch { commands },
                    "reproject",
                    None,
                    false,
                    false,
                )
            })?;
        }
        for (s, points, triangles) in surfaces {
            self.put_surface(user, &s.name, &s.source, &points, &triangles)?;
        }
        Ok(lines.len())
    }
}
