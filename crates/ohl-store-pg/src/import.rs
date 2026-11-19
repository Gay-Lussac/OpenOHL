// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A project file into the network (`ohl network import`, plan §3.5,
//! ADR-0013): one new model, as the network's writer.
//!
//! 1. The file is read into memory, never changed (it stays the archive
//!    of its own history). A part of a line, or a file without a line, is
//!    refused.
//! 2. **One coordinate system and one grid** for the network: the first
//!    import sets them; a project in another is refused, to be
//!    reprojected first (Terrain › Coordinate system).
//! 3. **Ids unique in the network**: an element (route, alignment,
//!    alignment point, structure, section, jumper, spacer) whose id
//!    another model's main branch has is renamed `CODE-id` (`CODE-id-2`,
//!    … if that is taken too) with everything that refers to it — the
//!    session's own rename, so the line stays valid — and listed.
//! 4. The **current state of the main project line** comes in — its
//!    objects as they are (order, versions, JSON text), its spatial index,
//!    its meta rows and settings, users, edit grants and surfaces — as
//!    **one log entry** (kind `import`), and (unless asked not to) one
//!    revision "imported from FILE". The file's history, its other project
//!    lines, tags, point clouds and backgrounds stay in the file and are
//!    listed in the report (owner's answer to question 12: the current
//!    state, not the history).
//! 5. **Twin towers**: a structure within a distance (0.5 m by default)
//!    of another model's structure of the same type is listed for an
//!    admin to merge or keep — never merged here.
//! 6. The file's users join the network (`net_user`) with an assignment
//!    of the matching shipped role on the model (viewer → viewer,
//!    designer → designer, admin → line admin); rights are evaluated
//!    from phase 7.
//!
//! Everything but the revision is one transaction: a refused or failed
//! import leaves nothing behind.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use ohl_session::store::Store;
use ohl_session::store::sqlite::SqliteStore;
use ohl_session::{Command, Role, Session};
use serde::Serialize;

use crate::{NetError, Network, PgStore, check_code, lock};

/// The kinds whose ids are unique in the network.
pub const NETWORK_KINDS: [&str; 7] = [
    "route",
    "alignment",
    "alignment_point",
    "structure",
    "section",
    "jumper",
    "spacer",
];

/// Twin towers: the largest distance in plan between two structures of
/// different models taken for one tower (with the same type), m (plan
/// §3.5, an estimate to be set by the owner).
pub const TWIN_TOLERANCE: f64 = 0.5;

/// How to import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportOptions {
    /// The new model's code (the prefix of renamed ids).
    pub code: String,
    /// Who imports (a network admin; becomes an admin of the model).
    pub user: String,
    /// The model's name (default: the line's name in the file).
    pub name: Option<String>,
    /// Twin towers within this distance, m.
    pub tolerance: f64,
    /// Make the revision "imported from FILE" (a tree of every object).
    pub revision: bool,
}

impl ImportOptions {
    /// Model `code` imported by `user`, the defaults otherwise.
    pub fn new(code: &str, user: &str) -> Self {
        Self {
            code: code.to_owned(),
            user: user.to_owned(),
            name: None,
            tolerance: TWIN_TOLERANCE,
            revision: true,
        }
    }
}

/// An object renamed at import.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Renamed {
    /// Kind.
    pub kind: String,
    /// Its id in the file.
    pub from: String,
    /// Its id in the network.
    pub to: String,
}

/// A structure that is likely the same tower as another model's.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Twin {
    /// The structure imported (its id in the network).
    pub structure: String,
    /// Its id in the file.
    pub id_in_file: String,
    /// The other model.
    pub other_line: String,
    /// The other model's structure.
    pub other: String,
    /// Distance in plan between their positions, m.
    pub distance_m: f64,
    /// Their structure type (none: modelled point by point).
    pub structure_type: Option<String>,
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportReport {
    /// The model's code.
    pub line: String,
    /// Its name.
    pub name: String,
    /// The file (its name only).
    pub file: String,
    /// The coordinate system.
    pub crs: String,
    /// Objects imported, by kind.
    pub objects: BTreeMap<String, usize>,
    /// Ids renamed.
    pub renamed: Vec<Renamed>,
    /// Twin towers, for an admin to merge or keep.
    pub twins: Vec<Twin>,
    /// Users of the file (joined the network with an assignment on the
    /// model).
    pub users: Vec<(String, String)>,
    /// Edit grants kept.
    pub grants: usize,
    /// Surfaces kept.
    pub surfaces: usize,
    /// What stayed in the file, in words.
    pub left_in_file: Vec<String>,
    /// The file's main revision it came from (`None`: not committed).
    pub source_revision: Option<String>,
    /// The revision made (`None`: not asked).
    pub revision: Option<String>,
    /// Seconds, all of it.
    pub seconds: f64,
    /// Seconds of the revision.
    pub revision_seconds: f64,
}

fn refused(m: impl Into<String>) -> NetError {
    NetError::Refused(m.into())
}

/// Rows of a table of the file (0 when it has no such table), read as the
/// session reads a file to leave it as it is: immutable when no program
/// has it open (no `-wal` beside it, so none is made), else read-only
/// beside the program writing it.
fn count_rows(path: &Path, table: &str) -> usize {
    use rusqlite::OpenFlags;
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = if Path::new(&wal).exists() {
        rusqlite::Connection::open_with_flags(path, flags)
    } else {
        let mut uri = String::from("file:");
        let text = path.to_string_lossy().replace('\\', "/");
        if text.as_bytes().get(1) == Some(&b':') {
            uri.push('/');
        }
        for c in text.chars() {
            match c {
                '%' => uri.push_str("%25"),
                '?' => uri.push_str("%3f"),
                '#' => uri.push_str("%23"),
                ' ' => uri.push_str("%20"),
                c => uri.push(c),
            }
        }
        uri.push_str("?mode=ro&immutable=1");
        rusqlite::Connection::open_with_flags(uri, flags | OpenFlags::SQLITE_OPEN_URI)
    };
    let Ok(conn) = conn else {
        return 0;
    };
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(()),
        )
        .is_ok();
    if !exists {
        return 0;
    }
    conn.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
        r.get::<_, i64>(0)
    })
    .map_or(0, |n| n.max(0) as usize)
}

/// The shipped network role of a project role.
fn network_role(role: Role) -> &'static str {
    match role {
        Role::Viewer => "viewer",
        Role::Designer => "designer",
        Role::Admin => "line_admin",
    }
}

/// Imports the project file at `path` as a new model of `net` (the
/// network's writer): see the module's text.
pub fn import(
    net: &Network,
    path: &Path,
    options: &ImportOptions,
) -> Result<ImportReport, NetError> {
    let started = Instant::now();
    net.require_writer()?;
    let code = options.code.as_str();
    check_code(code)?;
    if net.lines()?.iter().any(|l| l.id == code) {
        return Err(refused(format!("the network has a line {code} already")));
    }
    if !(options.tolerance.is_finite() && options.tolerance >= 0.0) {
        return Err(refused(format!(
            "twin tolerance {} m: zero or more",
            options.tolerance
        )));
    }
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    // 1. The file, in memory.
    if !path.is_file() {
        return Err(refused(format!("{}: no such project file", path.display())));
    }
    let copy = SqliteStore::copy_in_memory(path)?;
    let mut session = Session::open_on(Box::new(copy), &path.to_string_lossy())?;
    if session.store().meta(ohl_session::part::PART_KEY)?.is_some() {
        return Err(refused(format!(
            "{file} holds a part of a line (a working set): import the whole line"
        )));
    }
    let main = ohl_session::versions::MAIN;
    if session.current_line() != main {
        // The file was left on another project line: its main comes in.
        session.select_line(main)?;
    }
    if !session.has_line() {
        return Err(refused(format!("{file} holds no line")));
    }
    // 2. One coordinate system and one grid.
    let crs = session.line().crs.clone();
    match net.meta("crs")? {
        Some(net_crs) if net_crs != crs => {
            return Err(refused(format!(
                "{file} is in \"{crs}\", the network in \"{net_crs}\": reproject the project \
                 first (Terrain › Coordinate system)"
            )));
        }
        _ => {}
    }
    let grid = session.grid().size.to_string();
    if let Some(net_grid) = net.meta("grid_size")?
        && net_grid != grid
    {
        return Err(refused(format!(
            "{file} has a chunk grid of {grid} m, the network {net_grid} m"
        )));
    }
    // 3. Ids clashing with other models: renamed with the code.
    let rows: Vec<_> = session
        .store()
        .object_rows()?
        .into_iter()
        .filter(|r| r.line == main)
        .collect();
    let mut file_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for r in &rows {
        if r.gone == 0 && NETWORK_KINDS.contains(&r.kind.as_str()) {
            file_ids
                .entry(r.kind.clone())
                .or_default()
                .insert(r.id.clone());
        }
    }
    let taken = |kind: &str, ids: &[String]| -> Result<BTreeSet<String>, NetError> {
        let kind = kind.to_owned();
        let ids = ids.to_vec();
        Ok(lock(&net.shared())
            .query(
                "SELECT id FROM object WHERE branch = 'main' AND gone = 0 AND kind = $1
                     AND id = ANY($2)",
                &[&kind, &ids],
            )?
            .into_iter()
            .map(|r| r.get(0))
            .collect())
    };
    let mut renamed: Vec<Renamed> = Vec::new();
    for (kind, ids) in &file_ids {
        let ids: Vec<String> = ids.iter().cloned().collect();
        let clashes = taken(kind, &ids)?;
        for id in clashes {
            let mut n = 1;
            let new_id = loop {
                let candidate = if n == 1 {
                    format!("{code}-{id}")
                } else {
                    format!("{code}-{id}-{n}")
                };
                let in_file = file_ids[kind].contains(&candidate)
                    || renamed.iter().any(|r| &r.kind == kind && r.to == candidate);
                if !in_file && taken(kind, std::slice::from_ref(&candidate))?.is_empty() {
                    break candidate;
                }
                n += 1;
            };
            renamed.push(Renamed {
                kind: kind.clone(),
                from: id,
                to: new_id,
            });
        }
    }
    if !renamed.is_empty() {
        let admin = session
            .users()?
            .into_iter()
            .find(|(_, r)| *r == Role::Admin)
            .map(|(n, _)| n)
            .ok_or_else(|| refused(format!("{file} has no admin to rename its objects as")))?;
        for r in &renamed {
            let kind = ohl_session::Kind::parse(&r.kind)?;
            session
                .apply(
                    &admin,
                    Command::Rename {
                        kind,
                        id: r.from.clone(),
                        new_id: r.to.clone(),
                    },
                )
                .map_err(|e| refused(format!("renaming {} {}: {e}", r.kind, r.from)))?;
        }
    }
    let source = session.store();
    let rows: Vec<_> = source
        .object_rows()?
        .into_iter()
        .filter(|r| r.line == main)
        .collect();
    let spatial = source.spatial_load(main)?;
    let users = session.users()?;
    let grants = source.grants()?;
    let surfaces = source.surfaces()?;
    let source_revision = source
        .refs()?
        .into_iter()
        .find(|r| r.kind == "branch" && r.name == main)
        .and_then(|r| r.revision);
    let name = options
        .name
        .clone()
        .unwrap_or_else(|| session.line().name.clone());

    // What stays in the file (read as the desktop reads a file without
    // changing it).
    let mut left = Vec::new();
    {
        let file_store = SqliteStore::open_read_only(path)?;
        let refs = file_store.refs()?;
        for (n, what) in [
            (file_store.entries()?.len(), "log entries (the history)"),
            (file_store.revisions()?.len(), "revisions"),
            (
                refs.iter()
                    .filter(|r| r.kind == "branch" && r.name != main)
                    .count(),
                "other project lines",
            ),
            (refs.iter().filter(|r| r.kind == "tag").count(), "tags"),
            (count_rows(path, "pointcloud"), "point clouds"),
            (count_rows(path, "background_image"), "background images"),
            (
                count_rows(path, "background_drawing"),
                "background drawings",
            ),
        ] {
            if n > 0 {
                left.push(format!("{n} {what}"));
            }
        }
    }
    let from_id: BTreeMap<&str, &str> = renamed
        .iter()
        .filter(|r| r.kind == "structure")
        .map(|r| (r.to.as_str(), r.from.as_str()))
        .collect();

    // 4. Into the network, as one transaction.
    let store = PgStore::new(net.shared(), code);
    store.savepoint("ohl_import")?;
    let written = (|| -> Result<Vec<Twin>, NetError> {
        lock(&net.shared()).execute(
            "INSERT INTO line (id, name, source) VALUES ($1, $2, $3)",
            &[&code, &name, &file],
        )?;
        if net.meta("crs")?.is_none() {
            net.set_meta("crs", &crs)?;
        }
        if net.meta("grid_size")?.is_none() {
            net.set_meta("grid_size", &grid)?;
        }
        for (key, value) in source.meta_rows()? {
            // The model opens on its main project line.
            let value = if key == "vc_head" {
                format!("branch:{main}")
            } else {
                value
            };
            store.set_meta(&key, &value)?;
        }
        store.prepare(Path::new(code), true)?;
        write_objects(net, code, &rows)?;
        write_spatial(net, code, &spatial)?;
        let mut users = users.clone();
        if !users.iter().any(|(n, _)| n == &options.user) {
            users.push((options.user.clone(), Role::Admin));
        }
        for (user, role) in &users {
            store.put_user(user, role.as_str())?;
            let shared = net.shared();
            let mut c = lock(&shared);
            c.execute(
                "INSERT INTO net_user (name, display) VALUES ($1, $1) ON CONFLICT DO NOTHING",
                &[user],
            )?;
            c.execute(
                "INSERT INTO assignment (subject_kind, subject, role, scope_kind, line, by_user,
                     note)
                 VALUES ('user', $1, $2, 'line', $3, $4, $5)",
                &[
                    user,
                    &network_role(*role),
                    &code,
                    &options.user,
                    &format!("from the project role {} at import", role.as_str()),
                ],
            )?;
        }
        for g in &grants {
            store.put_grant_row(g)?;
        }
        for s in &surfaces {
            let data = source.surface_data(&s.name)?.unwrap_or_default();
            store.put_surface_row(s, &data)?;
        }
        let note = format!(
            "imported from {file} ({} of its main project line, {} objects{})",
            source_revision
                .as_deref()
                .map_or("the uncommitted state".to_owned(), |r| format!(
                    "revision {}",
                    ohl_session::versions::short(r)
                )),
            rows.len(),
            if renamed.is_empty() {
                String::new()
            } else {
                format!(", {} ids renamed", renamed.len())
            }
        );
        store.append_entry(&ohl_session::store::NewEntry {
            user: &options.user,
            kind: "import",
            command: r#"{"op":"batch","commands":[]}"#.into(),
            inverse: r#"{"op":"batch","commands":[]}"#.into(),
            objects: "[]".into(),
            sections: "[]".into(),
            undoes: None,
            note: Some(note),
            line: main,
        })?;
        // The planner's statistics of the tables just grown: without them
        // the first imports into a young network join the spatial index
        // without its GiST index (minutes instead of milliseconds).
        lock(&net.shared())
            .client
            .batch_execute("ANALYZE object, spatial")?;
        // 5. Twin towers.
        let twins = twins(net, code, options.tolerance)?
            .into_iter()
            .map(|mut t| {
                t.id_in_file = from_id
                    .get(t.structure.as_str())
                    .map_or_else(|| t.structure.clone(), |s| (*s).to_owned());
                t
            })
            .collect::<Vec<_>>();
        net.audit(
            &options.user,
            "import",
            code,
            &serde_json::json!({
                "file": file,
                "objects": rows.len(),
                "renamed": renamed.len(),
                "twins": twins.len(),
            })
            .to_string(),
        )?;
        Ok(twins)
    })();
    let twins = match written {
        Ok(twins) => {
            store.release("ohl_import")?;
            twins
        }
        Err(e) => {
            let _ = store.roll_back_to("ohl_import");
            return Err(e);
        }
    };
    drop(session);

    // The revision "imported from FILE" (the line is in the network
    // already: a failure here says so).
    let t = Instant::now();
    let revision = if options.revision {
        let made = (|| -> Result<String, NetError> {
            let mut s = Session::open_on(Box::new(net.store(code)?), code)?;
            let message = match &source_revision {
                Some(r) => format!(
                    "imported from {file} at revision {}",
                    ohl_session::versions::short(r)
                ),
                None => format!("imported from {file} (uncommitted state)"),
            };
            Ok(s.commit(&options.user, &message)?.id)
        })();
        Some(made.map_err(|e| {
            NetError::Database(format!(
                "line {code} imported, its revision \"imported from {file}\" not made: {e}"
            ))
        })?)
    } else {
        None
    };
    let revision_seconds = t.elapsed().as_secs_f64();

    let mut objects: BTreeMap<String, usize> = BTreeMap::new();
    for r in &rows {
        if r.gone == 0 {
            *objects.entry(r.kind.clone()).or_default() += 1;
        }
    }
    Ok(ImportReport {
        line: code.to_owned(),
        name,
        file,
        crs,
        objects,
        renamed,
        twins,
        users: users
            .iter()
            .map(|(n, r)| (n.clone(), r.as_str().to_owned()))
            .collect(),
        grants: grants.len(),
        surfaces: surfaces.len(),
        left_in_file: left,
        source_revision,
        revision,
        seconds: started.elapsed().as_secs_f64(),
        revision_seconds,
    })
}

/// Batches of rows written in one statement.
const BATCH: usize = 2000;

fn write_objects(
    net: &Network,
    code: &str,
    rows: &[ohl_session::store::ObjectRow],
) -> Result<(), NetError> {
    let shared = net.shared();
    for chunk in rows.chunks(BATCH) {
        let kinds: Vec<&str> = chunk.iter().map(|r| r.kind.as_str()).collect();
        let ids: Vec<&str> = chunk.iter().map(|r| r.id.as_str()).collect();
        let ords: Vec<i64> = chunk.iter().map(|r| r.ord).collect();
        let versions: Vec<i64> = chunk.iter().map(|r| r.version).collect();
        let jsons: Vec<&str> = chunk.iter().map(|r| r.json.as_str()).collect();
        let gones: Vec<i16> = chunk.iter().map(|r| r.gone as i16).collect();
        lock(&shared)
            .execute(
                "INSERT INTO object (line_id, branch, kind, id, ord, version, json, gone)
                 SELECT $1, 'main', k, i, o, v, j, g
                 FROM unnest($2::text[], $3::text[], $4::bigint[], $5::bigint[], $6::text[],
                     $7::smallint[]) AS t(k, i, o, v, j, g)",
                &[&code, &kinds, &ids, &ords, &versions, &jsons, &gones],
            )
            .map_err(|e| match e.as_db_error() {
                Some(d) if d.constraint() == Some("object_network_id") => {
                    NetError::Refused(format!(
                        "an id is used by another line of the network ({})",
                        d.detail().unwrap_or_default()
                    ))
                }
                _ => NetError::from(e),
            })?;
    }
    Ok(())
}

fn write_spatial(
    net: &Network,
    code: &str,
    entries: &BTreeMap<(ohl_session::Kind, String), ohl_session::spatial::Entry>,
) -> Result<(), NetError> {
    let shared = net.shared();
    let all: Vec<_> = entries.iter().collect();
    for chunk in all.chunks(BATCH) {
        let kinds: Vec<&str> = chunk.iter().map(|((k, _), _)| k.as_str()).collect();
        let ids: Vec<&str> = chunk.iter().map(|((_, id), _)| id.as_str()).collect();
        let levels: Vec<i16> = chunk.iter().map(|(_, e)| i16::from(e.tile.level)).collect();
        let tx: Vec<i64> = chunk.iter().map(|(_, e)| e.tile.x).collect();
        let ty: Vec<i64> = chunk.iter().map(|(_, e)| e.tile.y).collect();
        let x0: Vec<f64> = chunk.iter().map(|(_, e)| e.bounds.min[0]).collect();
        let y0: Vec<f64> = chunk.iter().map(|(_, e)| e.bounds.min[1]).collect();
        let x1: Vec<f64> = chunk.iter().map(|(_, e)| e.bounds.max[0]).collect();
        let y1: Vec<f64> = chunk.iter().map(|(_, e)| e.bounds.max[1]).collect();
        lock(&shared).execute(
            "INSERT INTO spatial (line_id, branch, kind, id, level, tx, ty, x0, y0, x1, y1,
                 geom)
             SELECT $1, 'main', k, i, l, x, y, a, b, c, d, ST_MakeEnvelope(a, b, c, d)
             FROM unnest($2::text[], $3::text[], $4::smallint[], $5::bigint[], $6::bigint[],
                 $7::float8[], $8::float8[], $9::float8[], $10::float8[])
                 AS t(k, i, l, x, y, a, b, c, d)",
            &[&code, &kinds, &ids, &levels, &tx, &ty, &x0, &y0, &x1, &y1],
        )?;
    }
    Ok(())
}

/// The structures of model `code`'s main branch within `tolerance` (in
/// plan) of another model's structure of the same type, by id.
pub fn twins(net: &Network, code: &str, tolerance: f64) -> Result<Vec<Twin>, NetError> {
    if net.lines()?.iter().all(|l| l.id == code) {
        // No other line: no twin.
        return Ok(Vec::new());
    }
    let rows = lock(&net.shared()).query(
        "SELECT a.id, b.line_id, b.id,
                (oa.json::jsonb -> 'position' ->> 0)::float8, (oa.json::jsonb -> 'position' ->> 1)::float8,
                (ob.json::jsonb -> 'position' ->> 0)::float8, (ob.json::jsonb -> 'position' ->> 1)::float8,
                oa.json::jsonb ->> 'type', ob.json::jsonb ->> 'type'
         FROM spatial a
         JOIN spatial b ON b.kind = 'structure' AND b.branch = 'main' AND b.line_id <> a.line_id
             AND ST_DWithin(a.geom, b.geom, $2)
         JOIN object oa ON oa.line_id = a.line_id AND oa.branch = 'main' AND oa.kind = 'structure'
             AND oa.id = a.id AND oa.gone = 0
         JOIN object ob ON ob.line_id = b.line_id AND ob.branch = 'main' AND ob.kind = 'structure'
             AND ob.id = b.id AND ob.gone = 0
         WHERE a.line_id = $1 AND a.kind = 'structure' AND a.branch = 'main'
         ORDER BY a.id, b.line_id, b.id",
        &[&code, &tolerance],
    )?;
    let mut out = Vec::new();
    for r in rows {
        let (ax, ay, bx, by): (Option<f64>, Option<f64>, Option<f64>, Option<f64>) =
            (r.get(3), r.get(4), r.get(5), r.get(6));
        let (Some(ax), Some(ay), Some(bx), Some(by)) = (ax, ay, bx, by) else {
            continue;
        };
        let d = (ax - bx).hypot(ay - by);
        let (ta, tb): (Option<String>, Option<String>) = (r.get(7), r.get(8));
        if d <= tolerance && ta == tb {
            out.push(Twin {
                structure: r.get(0),
                id_in_file: r.get(0),
                other_line: r.get(1),
                other: r.get(2),
                distance_m: (d * 1000.0).round() / 1000.0,
                structure_type: ta,
            });
        }
    }
    Ok(out)
}

impl ImportReport {
    /// The report in a few lines.
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let n: usize = self.objects.values().sum();
        let _ = writeln!(
            out,
            "line {} ({}) from {}: {n} objects ({}) in {:.2} s",
            self.line,
            self.name,
            self.file,
            self.objects
                .iter()
                .map(|(k, v)| format!("{v} {k}"))
                .collect::<Vec<_>>()
                .join(", "),
            self.seconds
        );
        let _ = writeln!(out, "  renamed: {}", self.renamed.len());
        for r in self.renamed.iter().take(20) {
            let _ = writeln!(out, "    {} {} -> {}", r.kind, r.from, r.to);
        }
        if self.renamed.len() > 20 {
            let _ = writeln!(out, "    … {} more", self.renamed.len() - 20);
        }
        let _ = writeln!(
            out,
            "  twin towers (for an admin to merge or keep): {}",
            self.twins.len()
        );
        for t in self.twins.iter().take(20) {
            let _ = writeln!(
                out,
                "    {} (in the file {}) and {} {}: {:.3} m",
                t.structure, t.id_in_file, t.other_line, t.other, t.distance_m
            );
        }
        if self.twins.len() > 20 {
            let _ = writeln!(out, "    … {} more", self.twins.len() - 20);
        }
        if !self.left_in_file.is_empty() {
            let _ = writeln!(out, "  left in the file: {}", self.left_in_file.join(", "));
        }
        if let Some(r) = &self.revision {
            let _ = writeln!(
                out,
                "  revision {} ({:.2} s)",
                ohl_session::versions::short(r),
                self.revision_seconds
            );
        }
        out
    }

    /// The report as an HTML page of its own (no script, no request to
    /// anywhere).
    pub fn html(&self) -> String {
        use std::fmt::Write;
        let e = escape;
        let mut out = String::new();
        let _ = write!(
            out,
            "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
             <title>Import of {}</title>\
             <style>body{{font-family:sans-serif;margin:2em}}td,th{{padding:2px 8px;\
             text-align:left}}tr:nth-child(even){{background:#f2f2f2}}</style></head><body>\
             <h1>Import of line {} — {}</h1>",
            e(&self.line),
            e(&self.line),
            e(&self.name)
        );
        let _ = write!(
            out,
            "<p>From <b>{}</b> ({}), coordinate system “{}”, in {:.2} s.</p>",
            e(&self.file),
            e(self
                .source_revision
                .as_deref()
                .map_or("its uncommitted state", |r| ohl_session::versions::short(r))),
            e(&self.crs),
            self.seconds
        );
        let _ = write!(out, "<h2>Objects</h2><table>");
        for (k, v) in &self.objects {
            let _ = write!(out, "<tr><td>{}</td><td>{v}</td></tr>", e(k));
        }
        let _ = write!(
            out,
            "</table><p>{} users, {} edit grants, {} surfaces.</p>",
            self.users.len(),
            self.grants,
            self.surfaces
        );
        let _ = write!(
            out,
            "<h2>Renamed ({})</h2><p>Ids another line of the network has, renamed with the \
             line's code (everything referring to them follows).</p><table>\
             <tr><th>Kind</th><th>In the file</th><th>In the network</th></tr>",
            self.renamed.len()
        );
        for r in &self.renamed {
            let _ = write!(
                out,
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                e(&r.kind),
                e(&r.from),
                e(&r.to)
            );
        }
        let _ = write!(
            out,
            "</table><h2>Twin towers ({})</h2><p>Structures within the tolerance of another \
             line's structure of the same type: likely one tower modelled in two projects. An \
             admin merges them or keeps both; nothing was merged.</p><table><tr><th>Structure\
             </th><th>In the file</th><th>Other line</th><th>Its structure</th><th>Distance, m\
             </th><th>Type</th></tr>",
            self.twins.len()
        );
        for t in &self.twins {
            let _ = write!(
                out,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>{}</td></tr>",
                e(&t.structure),
                e(&t.id_in_file),
                e(&t.other_line),
                e(&t.other),
                t.distance_m,
                e(t.structure_type.as_deref().unwrap_or("—"))
            );
        }
        let _ = write!(out, "</table><h2>Left in the file</h2><ul>");
        for l in &self.left_in_file {
            let _ = write!(out, "<li>{}</li>", e(l));
        }
        let _ = writeln!(
            out,
            "</ul><p>The file is kept as the archive of its history (the network keeps the \
             current state of its main project line).</p></body></html>"
        );
        out
    }
}

/// `text` safe in HTML.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
