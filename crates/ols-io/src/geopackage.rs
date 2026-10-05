// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! OGC GeoPackage 1.2 (an SQLite file, after the public OGC GeoPackage
//! Encoding Standard): the GIS layers ([`crate::gis`]) as feature tables
//! `structures` (POINT Z), `routes` and `wires` (LINESTRING Z), with
//! `gpkg_spatial_ref_sys`, `gpkg_contents` and `gpkg_geometry_columns`.
//!
//! Geometries are GeoPackage binary (header `GP`, little-endian, an xy
//! envelope for lines) around ISO WKB. A line coordinate system
//! `EPSG:<code>` is listed under its code with organisation `EPSG` (GIS
//! programs resolve it from the code; the definition text is `undefined`
//! but for WGS84); one without a code is the undefined Cartesian system
//! (−1). No spatial index (optional in the standard). Linear in the
//! number of features.

use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::{Connection, params};

use crate::gis::{Geometry, Kind, Layer, Output};
use crate::tables::Cell;
use crate::{ExportModel, Exporter, Options};

/// OGC WKT of WGS84 (the definition of srs 4326).
const WGS84_WKT: &str = "GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563,AUTHORITY[\"EPSG\",\"7030\"]],AUTHORITY[\"EPSG\",\"6326\"]],PRIMEM[\"Greenwich\",0,AUTHORITY[\"EPSG\",\"8901\"]],UNIT[\"degree\",0.0174532925199433,AUTHORITY[\"EPSG\",\"9122\"]],AUTHORITY[\"EPSG\",\"4326\"]]";

/// `application_id` of a GeoPackage (`GPKG`).
pub const APPLICATION_ID: i32 = 0x4750_4B47;

/// A geometry as a GeoPackage binary blob.
pub fn geometry_blob(g: &Geometry, srs_id: i32) -> Vec<u8> {
    let mut b = b"GP\0".to_vec();
    let points: &[ols_line::Point] = match g {
        Geometry::Point(p) => std::slice::from_ref(p),
        Geometry::Line(ps) => ps,
    };
    let line = matches!(g, Geometry::Line(_));
    // Flags: little-endian; envelope [minx, maxx, miny, maxy] for lines.
    b.push(if line { 0b0000_0011 } else { 0b0000_0001 });
    b.extend_from_slice(&srs_id.to_le_bytes());
    if line {
        let fold =
            |k: usize, f: fn(f64, f64) -> f64, init: f64| points.iter().map(|p| p[k]).fold(init, f);
        for v in [
            fold(0, f64::min, f64::INFINITY),
            fold(0, f64::max, f64::NEG_INFINITY),
            fold(1, f64::min, f64::INFINITY),
            fold(1, f64::max, f64::NEG_INFINITY),
        ] {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b.push(1);
    if line {
        b.extend_from_slice(&1002u32.to_le_bytes());
        b.extend_from_slice(&(points.len() as u32).to_le_bytes());
    } else {
        b.extend_from_slice(&1001u32.to_le_bytes());
    }
    for p in points {
        for v in p {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b
}

fn sql(e: rusqlite::Error) -> String {
    format!("GeoPackage: {e}")
}

/// Writes the layers into a new GeoPackage database.
pub fn write(conn: &Connection, layers: &[Layer], out: &Output) -> Result<(), String> {
    conn.execute_batch(&format!(
        "PRAGMA application_id = {APPLICATION_ID};
         PRAGMA user_version = 10200;
         CREATE TABLE gpkg_spatial_ref_sys (srs_name TEXT NOT NULL, srs_id INTEGER NOT NULL PRIMARY KEY,
           organization TEXT NOT NULL, organization_coordsys_id INTEGER NOT NULL,
           definition TEXT NOT NULL, description TEXT);
         CREATE TABLE gpkg_contents (table_name TEXT NOT NULL PRIMARY KEY, data_type TEXT NOT NULL,
           identifier TEXT UNIQUE, description TEXT DEFAULT '',
           last_change DATETIME NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
           min_x DOUBLE, min_y DOUBLE, max_x DOUBLE, max_y DOUBLE, srs_id INTEGER,
           CONSTRAINT fk_gc_r_srs_id FOREIGN KEY (srs_id) REFERENCES gpkg_spatial_ref_sys(srs_id));
         CREATE TABLE gpkg_geometry_columns (table_name TEXT NOT NULL, column_name TEXT NOT NULL,
           geometry_type_name TEXT NOT NULL, srs_id INTEGER NOT NULL, z TINYINT NOT NULL, m TINYINT NOT NULL,
           CONSTRAINT pk_geom_cols PRIMARY KEY (table_name, column_name),
           CONSTRAINT uk_gc_table_name UNIQUE (table_name),
           CONSTRAINT fk_gc_tn FOREIGN KEY (table_name) REFERENCES gpkg_contents(table_name),
           CONSTRAINT fk_gc_srs FOREIGN KEY (srs_id) REFERENCES gpkg_spatial_ref_sys (srs_id));
         INSERT INTO gpkg_spatial_ref_sys VALUES
           ('Undefined cartesian SRS', -1, 'NONE', -1, 'undefined', 'undefined cartesian coordinate reference system'),
           ('Undefined geographic SRS', 0, 'NONE', 0, 'undefined', 'undefined geographic coordinate reference system'),
           ('WGS 84 geodetic', 4326, 'EPSG', 4326, '{WGS84_WKT}', 'longitude/latitude coordinates in decimal degrees on the WGS 84 spheroid');"
    ))
    .map_err(sql)?;
    let srs_id = match out.epsg {
        Some(4326) => 4326,
        Some(code) => {
            let description = ols_crs::Crs::parse(&out.crs)
                .map(|c| c.definition().to_owned())
                .unwrap_or_default();
            conn.execute(
                "INSERT INTO gpkg_spatial_ref_sys VALUES (?1, ?2, 'EPSG', ?2, 'undefined', ?3)",
                params![out.crs, code, description],
            )
            .map_err(sql)?;
            code as i32
        }
        None => -1,
    };
    for layer in layers {
        let point = layer.name == "structures";
        let mut columns = String::new();
        for (name, kind) in &layer.columns {
            columns.push_str(&format!(
                ", \"{name}\" {}",
                if *kind == Kind::Text { "TEXT" } else { "REAL" }
            ));
        }
        conn.execute_batch(&format!(
            "CREATE TABLE \"{}\" (fid INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, geom {}{columns});",
            layer.name,
            if point { "POINT" } else { "LINESTRING" }
        ))
        .map_err(sql)?;
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let marks = vec!["?"; layer.columns.len() + 1].join(", ");
        let names: Vec<String> = layer
            .columns
            .iter()
            .map(|(n, _)| format!("\"{n}\""))
            .collect();
        let mut insert = conn
            .prepare(&format!(
                "INSERT INTO \"{}\" (geom, {}) VALUES ({marks})",
                layer.name,
                names.join(", ")
            ))
            .map_err(sql)?;
        for (g, cells) in &layer.features {
            match g {
                Geometry::Point(p) => {
                    b = [
                        b[0].min(p[0]),
                        b[1].min(p[1]),
                        b[2].max(p[0]),
                        b[3].max(p[1]),
                    ];
                }
                Geometry::Line(ps) => {
                    for p in ps {
                        b = [
                            b[0].min(p[0]),
                            b[1].min(p[1]),
                            b[2].max(p[0]),
                            b[3].max(p[1]),
                        ];
                    }
                }
            }
            let mut values: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Blob(geometry_blob(g, srs_id))];
            for k in 0..layer.columns.len() {
                values.push(match cells.get(k) {
                    Some(Cell::Text(t)) => rusqlite::types::Value::Text(t.clone()),
                    Some(Cell::Number(x)) => rusqlite::types::Value::Real(*x),
                    _ => rusqlite::types::Value::Null,
                });
            }
            insert
                .execute(rusqlite::params_from_iter(values))
                .map_err(sql)?;
        }
        let extent = |v: f64| v.is_finite().then_some(v);
        conn.execute(
            "INSERT INTO gpkg_contents (table_name, data_type, identifier, description, min_x, min_y, max_x, max_y, srs_id)
             VALUES (?1, 'features', ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                layer.name,
                format!("OLS CAD {}", layer.name),
                extent(b[0]),
                extent(b[1]),
                extent(b[2]),
                extent(b[3]),
                srs_id
            ],
        )
        .map_err(sql)?;
        conn.execute(
            "INSERT INTO gpkg_geometry_columns VALUES (?1, 'geom', ?2, ?3, 1, 0)",
            params![
                layer.name,
                if point { "POINT" } else { "LINESTRING" },
                srs_id
            ],
        )
        .map_err(sql)?;
    }
    Ok(())
}

static FILES: AtomicUsize = AtomicUsize::new(0);

/// GeoPackage: structures, routes and wires as feature tables.
pub struct GeoPackage;

impl Exporter for GeoPackage {
    fn id(&self) -> &str {
        "gpkg"
    }
    fn description(&self) -> &str {
        "Structures, routes and wires as a GeoPackage (line CRS)"
    }
    fn extension(&self) -> &str {
        "gpkg"
    }
    fn export(&mut self, model: &ExportModel, options: &Options) -> Result<Vec<u8>, String> {
        let out = crate::gis::output(model, options, false)?;
        let layers = crate::gis::layers(model, options, &out, true)?;
        // SQLite writes to a file: a temporary one, read back and removed.
        let path = std::env::temp_dir().join(format!(
            "ols-export-{}-{}.gpkg",
            std::process::id(),
            FILES.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let result = (|| {
            let conn = Connection::open(&path).map_err(sql)?;
            conn.execute_batch("BEGIN").map_err(sql)?;
            write(&conn, &layers, &out)?;
            conn.execute_batch("COMMIT").map_err(sql)?;
            conn.close().map_err(|(_, e)| sql(e))?;
            std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
        })();
        let _ = std::fs::remove_file(&path);
        result
    }
}
