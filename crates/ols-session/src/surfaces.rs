// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terrain surfaces (TINs) kept in the project file: several, by name,
//! each a mesh (points and triangles) stored compressed. Like libraries,
//! they are part of the project, not of the line: not logged, not undone.

use rusqlite::{OptionalExtension, params};

use crate::{Role, Session, SessionError, refused};

/// Table of the surfaces.
pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS surface (
    name TEXT PRIMARY KEY,
    time TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    user TEXT NOT NULL, source TEXT NOT NULL,
    points INTEGER NOT NULL, triangles INTEGER NOT NULL,
    bounds TEXT NOT NULL, data BLOB NOT NULL);";

/// What is known of a stored surface without reading it.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceInfo {
    /// Unique name.
    pub name: String,
    /// When it was stored (UTC).
    pub time: String,
    /// Who stored it.
    pub user: String,
    /// Where it comes from (file, LiDAR classes…).
    pub source: String,
    /// Number of points.
    pub points: u64,
    /// Number of triangles.
    pub triangles: u64,
    /// Extent: lowest and highest x, y, z.
    pub bounds: Option<([f64; 3], [f64; 3])>,
}

/// zstd level: fast, about 2× smaller than raw.
const LEVEL: i32 = 3;

fn encode(points: &[[f64; 3]], triangles: &[[u32; 3]]) -> Result<Vec<u8>, SessionError> {
    let mut raw = Vec::with_capacity(16 + points.len() * 24 + triangles.len() * 12);
    raw.extend_from_slice(&(points.len() as u64).to_le_bytes());
    raw.extend_from_slice(&(triangles.len() as u64).to_le_bytes());
    for p in points {
        for v in p {
            raw.extend_from_slice(&v.to_le_bytes());
        }
    }
    for t in triangles {
        for v in t {
            raw.extend_from_slice(&v.to_le_bytes());
        }
    }
    zstd::encode_all(raw.as_slice(), LEVEL).map_err(|e| SessionError::Store(e.to_string()))
}

type Mesh = (Vec<[f64; 3]>, Vec<[u32; 3]>);

fn decode(data: &[u8]) -> Result<Mesh, SessionError> {
    let bad = |m: &str| SessionError::Store(format!("surface data: {m}"));
    let raw = zstd::decode_all(data).map_err(|e| SessionError::Store(e.to_string()))?;
    if raw.len() < 16 {
        return Err(bad("too short"));
    }
    let n = u64::from_le_bytes(raw[0..8].try_into().expect("8 bytes")) as usize;
    let m = u64::from_le_bytes(raw[8..16].try_into().expect("8 bytes")) as usize;
    if raw.len() != 16 + n * 24 + m * 12 {
        return Err(bad("size does not match"));
    }
    let f = |o: usize| f64::from_le_bytes(raw[o..o + 8].try_into().expect("8 bytes"));
    let u = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().expect("4 bytes"));
    let points = (0..n)
        .map(|k| {
            let o = 16 + 24 * k;
            [f(o), f(o + 8), f(o + 16)]
        })
        .collect();
    let base = 16 + 24 * n;
    let triangles = (0..m)
        .map(|k| {
            let o = base + 12 * k;
            [u(o), u(o + 4), u(o + 8)]
        })
        .collect();
    Ok((points, triangles))
}

fn check_name(name: &str) -> Result<(), SessionError> {
    if name.trim().is_empty() || name.len() > 200 {
        return Err(refused(format!(
            "surface name \"{name}\": 1 to 200 characters"
        )));
    }
    Ok(())
}

impl Session {
    /// The surfaces kept in the project, by name.
    pub fn surfaces(&self) -> Result<Vec<SurfaceInfo>, SessionError> {
        let mut stmt = self.conn.prepare(
            "SELECT name, time, user, source, points, triangles, bounds FROM surface ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (name, time, user, source, points, triangles, bounds) = row?;
            out.push(SurfaceInfo {
                name,
                time,
                user,
                source,
                points: points as u64,
                triangles: triangles as u64,
                bounds: serde_json::from_str(&bounds).ok().flatten(),
            });
        }
        Ok(out)
    }

    /// Surface `name`: its points and triangles.
    pub fn surface(&self, name: &str) -> Result<Mesh, SessionError> {
        let data: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT data FROM surface WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        decode(&data.ok_or_else(|| refused(format!("no surface \"{name}\"")))?)
    }

    /// Keeps a surface in the project (replacing one of that name).
    /// `source`: where it comes from, in words.
    pub fn put_surface(
        &self,
        user: &str,
        name: &str,
        source: &str,
        points: &[[f64; 3]],
        triangles: &[[u32; 3]],
    ) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        check_name(name)?;
        if let Some(t) = triangles
            .iter()
            .find(|t| t.iter().any(|&k| k as usize >= points.len()))
        {
            return Err(refused(format!(
                "surface \"{name}\": triangle {t:?} refers to a point beyond the {}",
                points.len()
            )));
        }
        let bounds = points.first().map(|&first| {
            points.iter().fold((first, first), |(mut lo, mut hi), p| {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
                (lo, hi)
            })
        });
        let data = encode(points, triangles)?;
        self.conn.execute(
            "INSERT INTO surface (name, user, source, points, triangles, bounds, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(name) DO UPDATE SET user = excluded.user, source = excluded.source,
                 points = excluded.points, triangles = excluded.triangles,
                 bounds = excluded.bounds, data = excluded.data,
                 time = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![
                name,
                user,
                source,
                points.len() as i64,
                triangles.len() as i64,
                serde_json::to_string(&bounds).unwrap_or_else(|_| "null".into()),
                data
            ],
        )?;
        Ok(())
    }

    /// Renames surface `from` to `to` (a new name).
    pub fn rename_surface(&self, user: &str, from: &str, to: &str) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        check_name(to)?;
        if self.surfaces()?.iter().any(|s| s.name == to) {
            return Err(refused(format!("a surface \"{to}\" exists already")));
        }
        let n = self.conn.execute(
            "UPDATE surface SET name = ?2 WHERE name = ?1",
            params![from, to],
        )?;
        if n == 0 {
            return Err(refused(format!("no surface \"{from}\"")));
        }
        Ok(())
    }

    /// Removes surface `name` from the project.
    pub fn delete_surface(&self, user: &str, name: &str) -> Result<(), SessionError> {
        self.require(user, Role::Designer)?;
        let n = self
            .conn
            .execute("DELETE FROM surface WHERE name = ?1", params![name])?;
        if n == 0 {
            return Err(refused(format!("no surface \"{name}\"")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Role, Session};

    #[test]
    fn surfaces_are_kept_renamed_and_removed() {
        let dir = std::env::temp_dir().join(format!("ols-surfaces-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.ols");
        let s = Session::open(&path).unwrap();
        s.set_user("ann", "ann", Role::Admin).unwrap();
        s.set_user("ann", "vic", Role::Viewer).unwrap();
        let points = vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 3.5]];
        let tris = vec![[0, 1, 2]];
        s.put_surface("ann", "ground", "test", &points, &tris)
            .unwrap();
        assert!(
            s.put_surface("vic", "x", "test", &points, &tris).is_err(),
            "viewer"
        );
        assert!(
            s.put_surface("ann", "bad", "test", &points, &[[0, 1, 3]])
                .is_err()
        );
        let info = s.surfaces().unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!((info[0].points, info[0].triangles), (3, 1));
        assert_eq!(info[0].bounds, Some(([0.0, 0.0, 1.0], [10.0, 10.0, 3.5])));
        s.rename_surface("ann", "ground", "LiDAR ground").unwrap();
        drop(s);
        // Read back from the file.
        let s = Session::open(&path).unwrap();
        assert_eq!(s.surface("LiDAR ground").unwrap(), (points, tris));
        assert!(s.surface("ground").is_err());
        s.delete_surface("ann", "LiDAR ground").unwrap();
        assert!(s.surfaces().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
