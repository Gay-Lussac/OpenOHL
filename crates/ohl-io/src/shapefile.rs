// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! ESRI Shapefiles (after the public *ESRI Shapefile Technical
//! Description*, 1998): `.shp` geometry, `.shx` index, `.dbf` attributes
//! (dBASE III), `.cpg` (UTF-8) and, for WGS84, `.prj`.
//!
//! Export: one shapefile per GIS layer ([`crate::gis`]: `structures`
//! PointZ, `routes` and `wires` PolyLineZ), zipped together. Import:
//! structures from a zipped shapefile (`.shp` with its `.dbf`) or a bare
//! `.shp` (points only, ids made up): points become structures, the
//! first polyline the route. Linear in the file size.

use std::io::{Read as _, Write as _};

use ohl_line::Point;

use crate::gis::{Geometry, Kind, Layer};
use crate::neutral::{Neutral, NeutralPoint};
use crate::tables::Cell;
use crate::{ExportModel, Exporter, Importer, Options};

const POINT_Z: i32 = 11;
const POLYLINE_Z: i32 = 13;

/// ESRI WKT of WGS84 (the `.prj` of longitude, latitude output).
pub const WGS84_PRJ: &str = "GEOGCS[\"GCS_WGS_1984\",DATUM[\"D_WGS_1984\",SPHEROID[\"WGS_1984\",6378137.0,298.257223563]],PRIMEM[\"Greenwich\",0.0],UNIT[\"Degree\",0.0174532925199433]]";

/// Bounding box of points: x min, y min, x max, y max, z min, z max.
fn bounds<'a>(points: impl Iterator<Item = &'a Point>) -> [f64; 6] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in points {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
        b[4] = b[4].min(p[2]);
        b[5] = b[5].max(p[2]);
    }
    if b[0] > b[2] { [0.0; 6] } else { b }
}

fn header(out: &mut Vec<u8>, shape_type: i32, length_bytes: usize, b: [f64; 6]) {
    out.extend_from_slice(&9994i32.to_be_bytes());
    out.extend_from_slice(&[0; 20]);
    out.extend_from_slice(&((length_bytes / 2) as i32).to_be_bytes());
    out.extend_from_slice(&1000i32.to_le_bytes());
    out.extend_from_slice(&shape_type.to_le_bytes());
    for v in [b[0], b[1], b[2], b[3], b[4], b[5], 0.0, 0.0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn f64s(out: &mut Vec<u8>, values: impl IntoIterator<Item = f64>) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// One record's content.
fn record(g: &Geometry) -> Vec<u8> {
    let mut c = Vec::new();
    match g {
        Geometry::Point(p) => {
            c.extend_from_slice(&POINT_Z.to_le_bytes());
            f64s(&mut c, [p[0], p[1], p[2], 0.0]);
        }
        Geometry::Line(ps) => {
            let b = bounds(ps.iter());
            c.extend_from_slice(&POLYLINE_Z.to_le_bytes());
            f64s(&mut c, [b[0], b[1], b[2], b[3]]);
            c.extend_from_slice(&1i32.to_le_bytes());
            c.extend_from_slice(&(ps.len() as i32).to_le_bytes());
            c.extend_from_slice(&0i32.to_le_bytes());
            for p in ps {
                f64s(&mut c, [p[0], p[1]]);
            }
            f64s(&mut c, [b[4], b[5]]);
            f64s(&mut c, ps.iter().map(|p| p[2]));
            f64s(&mut c, [0.0, 0.0]);
            f64s(&mut c, ps.iter().map(|_| 0.0));
        }
    }
    c
}

/// The `.shp` and `.shx` of a layer.
pub fn shp_shx(layer: &Layer) -> (Vec<u8>, Vec<u8>) {
    let points = match layer.features.first() {
        Some((g, _)) => matches!(g, Geometry::Point(_)),
        None => layer.name == "structures",
    };
    let shape_type = if points { POINT_Z } else { POLYLINE_Z };
    let all = layer.features.iter().flat_map(|(g, _)| match g {
        Geometry::Point(p) => std::slice::from_ref(p).iter(),
        Geometry::Line(ps) => ps.iter(),
    });
    let b = bounds(all);
    let records: Vec<Vec<u8>> = layer.features.iter().map(|(g, _)| record(g)).collect();
    let shp_len = 100 + records.iter().map(|r| 8 + r.len()).sum::<usize>();
    let mut shp = Vec::with_capacity(shp_len);
    header(&mut shp, shape_type, shp_len, b);
    let mut shx = Vec::new();
    header(&mut shx, shape_type, 100 + 8 * records.len(), b);
    for (k, r) in records.iter().enumerate() {
        shx.extend_from_slice(&((shp.len() / 2) as i32).to_be_bytes());
        shx.extend_from_slice(&((r.len() / 2) as i32).to_be_bytes());
        shp.extend_from_slice(&(k as i32 + 1).to_be_bytes());
        shp.extend_from_slice(&((r.len() / 2) as i32).to_be_bytes());
        shp.extend_from_slice(r);
    }
    (shp, shx)
}

/// Text cut to `n` bytes at a character boundary.
fn cut(t: &str, n: usize) -> &str {
    let mut end = t.len().min(n);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    &t[..end]
}

/// The `.dbf` of a layer (text: C 254 UTF-8; numbers: N 19.6).
pub fn dbf(layer: &Layer) -> Vec<u8> {
    let widths: Vec<(u8, u8, u8)> = layer
        .columns
        .iter()
        .map(|(_, k)| match k {
            Kind::Text => (b'C', 254, 0),
            Kind::Number => (b'N', 19, 6),
        })
        .collect();
    let record_len = 1 + widths.iter().map(|w| usize::from(w.1)).sum::<usize>();
    let header_len = 32 + 32 * layer.columns.len() + 1;
    let mut out = vec![0x03, 126, 10, 5];
    out.extend_from_slice(&(layer.features.len() as u32).to_le_bytes());
    out.extend_from_slice(&(header_len as u16).to_le_bytes());
    out.extend_from_slice(&(record_len as u16).to_le_bytes());
    out.extend_from_slice(&[0; 20]);
    for ((name, _), (t, len, dec)) in layer.columns.iter().zip(&widths) {
        let mut field = [0u8; 32];
        let n = cut(name, 10).as_bytes();
        field[..n.len()].copy_from_slice(n);
        field[11] = *t;
        field[16] = *len;
        field[17] = *dec;
        out.extend_from_slice(&field);
    }
    out.push(0x0d);
    for (_, cells) in &layer.features {
        out.push(b' ');
        for (k, (_, len, _)) in widths.iter().enumerate() {
            let len = usize::from(*len);
            let text = match cells.get(k).unwrap_or(&Cell::Empty) {
                Cell::Text(t) => format!("{:<len$}", cut(t, len)),
                Cell::Number(x) if x.is_finite() => {
                    let s = format!("{x:.6}");
                    if s.len() > len {
                        format!("{:>len$}", format!("{x:.0}"))
                    } else {
                        format!("{s:>len$}")
                    }
                }
                _ => " ".repeat(len),
            };
            let bytes = text.as_bytes();
            // Multi-byte text may be wider than the field in bytes.
            out.extend_from_slice(&bytes[..bytes.len().min(len)]);
            out.extend(std::iter::repeat_n(b' ', len.saturating_sub(bytes.len())));
        }
    }
    out.push(0x1a);
    out
}

/// The layers as zipped shapefiles (`<layer>.shp`, `.shx`, `.dbf`,
/// `.cpg`, and `.prj` for WGS84).
pub fn zip_layers(layers: &[Layer], wgs84: bool) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for layer in layers {
        let (shp, shx) = shp_shx(layer);
        let mut files = vec![
            ("shp", shp),
            ("shx", shx),
            ("dbf", dbf(layer)),
            ("cpg", b"UTF-8".to_vec()),
        ];
        if wgs84 {
            files.push(("prj", WGS84_PRJ.as_bytes().to_vec()));
        }
        for (ext, bytes) in files {
            zip.start_file(format!("{}.{ext}", layer.name), opts)
                .map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// Shapefiles: export of the GIS layers (zipped), import of structures.
pub struct Shapefile;

impl Exporter for Shapefile {
    fn id(&self) -> &str {
        "shapefile"
    }
    fn description(&self) -> &str {
        "Structures, routes and wires as zipped ESRI shapefiles (line CRS)"
    }
    fn extension(&self) -> &str {
        "zip"
    }
    fn export(&mut self, model: &ExportModel, options: &Options) -> Result<Vec<u8>, String> {
        let out = crate::gis::output(model, options, false)?;
        let layers = crate::gis::layers(model, options, &out, true)?;
        zip_layers(&layers, out.geographic())
    }
}

// Reading.

/// Largest member of a zip read (2 GiB): a crafted zip claiming more is
/// refused rather than read into memory.
const MAX_ZIP_MEMBER: u64 = 2 << 30;

/// Largest uncompressed size of all the shapefile members of a zip
/// together (a small zip may unpack to many gigabytes: security review
/// 2026-10-26, M7).
const MAX_ZIP_TOTAL: u64 = 2 << 30;

/// Most shapefile members (`.shp`, `.dbf`, `.prj`) in a zip.
const MAX_ZIP_FILES: usize = 300;

fn le_f64(b: &[u8], at: usize) -> Result<f64, String> {
    at.checked_add(8)
        .and_then(|end| b.get(at..end))
        .map(|s| f64::from_le_bytes(s.try_into().expect("8 bytes")))
        .ok_or_else(|| "shapefile: record cut short".to_owned())
}

fn le_i32(b: &[u8], at: usize) -> Result<i32, String> {
    at.checked_add(4)
        .and_then(|end| b.get(at..end))
        .map(|s| i32::from_le_bytes(s.try_into().expect("4 bytes")))
        .ok_or_else(|| "shapefile: record cut short".to_owned())
}

/// The records of a `.shp`: points and polylines (all parts joined).
pub fn read_shp(shp: &[u8]) -> Result<Vec<Geometry>, String> {
    if shp.len() < 100 || i32::from_be_bytes(shp[0..4].try_into().expect("4")) != 9994 {
        return Err("not a shapefile (.shp): file code 9994 missing".into());
    }
    let mut out = Vec::new();
    let mut at = 100;
    while at + 8 <= shp.len() {
        let words = i32::from_be_bytes(shp[at + 4..at + 8].try_into().expect("4"));
        let len = usize::try_from(words)
            .ok()
            .and_then(|w| w.checked_mul(2))
            .ok_or_else(|| format!("shapefile: record length {words} not valid"))?;
        let c = (at + 8)
            .checked_add(len)
            .and_then(|end| shp.get(at + 8..end))
            .ok_or("shapefile: record cut short")?;
        at += 8 + len;
        let t = le_i32(c, 0)?;
        match t {
            0 => {}
            1 | 11 | 21 => out.push(Geometry::Point([
                le_f64(c, 4)?,
                le_f64(c, 12)?,
                if t == 11 { le_f64(c, 20)? } else { f64::NAN },
            ])),
            3 | 13 | 23 => {
                let count = |v: i32| {
                    usize::try_from(v).map_err(|_| format!("shapefile: count {v} not valid"))
                };
                let parts = count(le_i32(c, 36)?)?;
                let n = count(le_i32(c, 40)?)?;
                // Each point takes at least 16 bytes of the record and each
                // part 4: more than that cannot be there.
                if n > c.len() / 16 || parts > c.len() / 4 {
                    return Err("shapefile: record cut short".into());
                }
                let xy = 44 + 4 * parts;
                let z = xy + 16 * n + 16;
                let mut ps = Vec::with_capacity(n);
                for k in 0..n {
                    ps.push([
                        le_f64(c, xy + 16 * k)?,
                        le_f64(c, xy + 16 * k + 8)?,
                        if t == 13 {
                            le_f64(c, z + 8 * k)?
                        } else {
                            f64::NAN
                        },
                    ]);
                }
                out.push(Geometry::Line(ps));
            }
            other => return Err(format!("shapefile: shape type {other} not supported")),
        }
    }
    Ok(out)
}

/// The records of a `.dbf`: column names and text values (trimmed).
pub fn read_dbf(dbf: &[u8]) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    if dbf.len() < 32 {
        return Err("dBASE file too short".into());
    }
    let n = u32::from_le_bytes(dbf[4..8].try_into().expect("4")) as usize;
    let header = usize::from(u16::from_le_bytes(dbf[8..10].try_into().expect("2")));
    let record = usize::from(u16::from_le_bytes(dbf[10..12].try_into().expect("2")));
    let mut columns = Vec::new();
    let mut at = 32;
    while at + 32 <= header && dbf.get(at) != Some(&0x0d) {
        let d = dbf.get(at..at + 32).ok_or("dBASE header cut short")?;
        let name = d[..11].split(|&b| b == 0).next().unwrap_or(&[]);
        columns.push((
            String::from_utf8_lossy(name).into_owned(),
            usize::from(d[16]),
        ));
        at += 32;
    }
    // No more rows than the bytes after the header can hold.
    let room = dbf.len().saturating_sub(header) / record.max(1);
    if n > room {
        return Err("dBASE file cut short".into());
    }
    let mut rows = Vec::with_capacity(n);
    for k in 0..n {
        let r = k
            .checked_mul(record)
            .and_then(|o| o.checked_add(header))
            .and_then(|start| dbf.get(start..start.checked_add(record)?))
            .ok_or("dBASE file cut short")?;
        let mut pos = 1;
        let mut row = Vec::with_capacity(columns.len());
        for (_, w) in &columns {
            let v = r.get(pos..pos + w).unwrap_or(&[]);
            row.push(String::from_utf8_lossy(v).trim().to_owned());
            pos += w;
        }
        rows.push(row);
    }
    Ok((columns.into_iter().map(|c| c.0).collect(), rows))
}

/// The points and route of a shapefile: a zip with `.shp` (and `.dbf`,
/// `.prj` read when there), or a bare `.shp`. Columns: option `id` (else
/// `id`, `name`, `structure`), `type`, `extension`, `orient_deg`. In a zip
/// holding several shapefiles, the points come from the one whose name
/// says `struct` (else the first), the route from the first polyline (a
/// `routes` shapefile first).
pub fn read(data: &[u8], options: &Options) -> Result<Neutral, String> {
    if !data.starts_with(b"PK") {
        return read_points(data, &[], options);
    }
    let files = zip_members(data, MAX_ZIP_TOTAL, MAX_ZIP_FILES)?;
    let mut stems: Vec<String> = files
        .keys()
        .filter_map(|n| n.strip_suffix(".shp").map(str::to_owned))
        .collect();
    stems.sort_by_key(|s| !s.contains("struct"));
    let main = stems.first().ok_or("no .shp in the zip")?.clone();
    let get = |stem: &str, ext: &str| files.get(&format!("{stem}.{ext}")).map(Vec::as_slice);
    let mut out = read_points(
        get(&main, "shp").unwrap_or_default(),
        get(&main, "dbf").unwrap_or_default(),
        options,
    )?;
    stems.sort_by_key(|s| !s.contains("route"));
    for stem in &stems {
        if !out.route.is_empty() {
            break;
        }
        if let Some(Geometry::Line(l)) = read_shp(get(stem, "shp").unwrap_or_default())?
            .into_iter()
            .find(|g| matches!(g, Geometry::Line(_)))
        {
            out.route = l;
        }
    }
    out.crs = crs_of_prj(get(&main, "prj"));
    out.name = Some(main.rsplit('/').next().unwrap_or(&main).to_owned());
    Ok(out)
}

/// The shapefile members of a zip (`.shp`, `.dbf`, `.prj`, by lower-case
/// name), at most `max_files` of them unpacking to at most `total` bytes
/// together.
fn zip_members(
    data: &[u8],
    total: u64,
    max_files: usize,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| format!("not a zip: {e}"))?;
    let mut files = std::collections::BTreeMap::new();
    // What is left of the budget of all members together.
    let mut left = total;
    for k in 0..zip.len() {
        let mut f = zip.by_index(k).map_err(|e| e.to_string())?;
        let name = f.name().to_ascii_lowercase();
        if [".shp", ".dbf", ".prj"].iter().any(|e| name.ends_with(e)) {
            if files.len() >= max_files {
                return Err(format!(
                    "more than {max_files} shapefile members in the zip"
                ));
            }
            // Counted as they unpack (the sizes the zip states may lie).
            let cap = MAX_ZIP_MEMBER.min(left);
            let mut v = Vec::new();
            (&mut f)
                .take(cap + 1)
                .read_to_end(&mut v)
                .map_err(|e| e.to_string())?;
            if v.len() as u64 > MAX_ZIP_MEMBER {
                return Err(format!("{name} in the zip is larger than 2 GiB"));
            }
            if v.len() as u64 > left {
                return Err(format!(
                    "the shapefiles in the zip unpack to more than {} MiB together",
                    total >> 20
                ));
            }
            left -= v.len() as u64;
            files.insert(name, v);
        }
    }
    Ok(files)
}

/// `EPSG:4326` for a geographic WGS84 `.prj`; none otherwise (the line's
/// coordinates are assumed).
fn crs_of_prj(prj: Option<&[u8]>) -> Option<String> {
    let t = String::from_utf8_lossy(prj?).to_ascii_uppercase();
    (t.starts_with("GEOGCS") && t.contains("WGS")).then(|| "EPSG:4326".to_owned())
}

fn read_points(shp: &[u8], dbf: &[u8], options: &Options) -> Result<Neutral, String> {
    let geoms = read_shp(shp)?;
    let (columns, rows) = if dbf.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        read_dbf(dbf)?
    };
    let col = |names: &[&str]| {
        names
            .iter()
            .find_map(|n| columns.iter().position(|c| c.eq_ignore_ascii_case(n)))
    };
    let id = match options.get("id") {
        Some(k) => Some(col(&[k.as_str()]).ok_or_else(|| format!("no column \"{k}\""))?),
        None => col(&["id", "name", "structure"]),
    };
    let (kind, ext, orient) = (
        col(&["type"]),
        col(&["extension"]),
        col(&["orient_deg", "orientatio"]),
    );
    let mut out = Neutral::default();
    for (k, g) in geoms.into_iter().enumerate() {
        let cell = |c: Option<usize>| {
            c.and_then(|c| rows.get(k)?.get(c))
                .filter(|v| !v.is_empty())
                .cloned()
        };
        match g {
            Geometry::Point(p) => out.points.push(NeutralPoint {
                id: cell(id),
                position: p,
                structure_type: cell(kind),
                extension: cell(ext),
                orientation_deg: cell(orient).and_then(|v| v.parse().ok()),
            }),
            Geometry::Line(l) if out.route.is_empty() => out.route = l,
            Geometry::Line(_) => {}
        }
    }
    Ok(out)
}

impl Importer for Shapefile {
    fn id(&self) -> &str {
        "structures-shp"
    }
    fn description(&self) -> &str {
        "Structures from shapefile points (.shp, or zipped with .dbf), along a polyline route"
    }
    fn extension(&self) -> &str {
        "shp"
    }
    fn import(
        &mut self,
        data: &[u8],
        base: Option<&ohl_line::Line>,
        options: &Options,
    ) -> Result<ohl_line::Line, String> {
        crate::neutral::to_line(read(data, options)?, base, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.shp` of one record with the given length word and content.
    fn shp_with(words: i32, content: &[u8]) -> Vec<u8> {
        let mut shp = Vec::new();
        header(&mut shp, POLYLINE_Z, 108 + content.len(), [0.0; 6]);
        shp.extend_from_slice(&1i32.to_be_bytes());
        shp.extend_from_slice(&words.to_be_bytes());
        shp.extend_from_slice(content);
        shp
    }

    /// A polyline record's content claiming `parts` parts and `n` points.
    fn polyline(parts: i32, n: i32) -> Vec<u8> {
        let mut c = POLYLINE_Z.to_le_bytes().to_vec();
        c.extend_from_slice(&[0; 32]);
        c.extend_from_slice(&parts.to_le_bytes());
        c.extend_from_slice(&n.to_le_bytes());
        c
    }

    /// M7 (security review 2026-10-26): the members of a zip unpack to a
    /// bounded size together, and only so many of them are read.
    #[test]
    fn zip_members_are_bounded_together() {
        let zip = |members: &[(&str, usize)]| {
            let mut out = std::io::Cursor::new(Vec::new());
            let mut z = zip::ZipWriter::new(&mut out);
            for (name, n) in members {
                z.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(&vec![0u8; *n]).unwrap();
            }
            z.finish().unwrap();
            out.into_inner()
        };
        let data = zip(&[("a.shp", 600), ("a.dbf", 600), ("readme.txt", 5000)]);
        assert_eq!(zip_members(&data, 1200, 10).unwrap().len(), 2);
        let e = zip_members(&data, 1000, 10).unwrap_err();
        assert!(e.contains("together"), "{e}");
        let e = zip_members(&data, 5000, 1).unwrap_err();
        assert!(e.contains("members"), "{e}");
    }

    #[test]
    fn crafted_shp_records_error_cleanly() {
        // Negative and too long record lengths.
        assert!(read_shp(&shp_with(-1, &[])).is_err());
        assert!(read_shp(&shp_with(i32::MAX, &[])).is_err());
        // Billions of points, or a negative count, in a 44-byte record.
        for (parts, n) in [(1, i32::MAX), (1, -5), (i32::MAX, 1), (-1, 1)] {
            let c = polyline(parts, n);
            assert!(read_shp(&shp_with(c.len() as i32 / 2, &c)).is_err());
        }
    }

    /// A `.dbf` header: record count, header and record lengths.
    fn dbf_with(n: u32, header: u16, record: u16) -> Vec<u8> {
        let mut dbf = vec![0x03, 126, 10, 5];
        dbf.extend_from_slice(&n.to_le_bytes());
        dbf.extend_from_slice(&header.to_le_bytes());
        dbf.extend_from_slice(&record.to_le_bytes());
        dbf.extend_from_slice(&[0; 20]);
        dbf
    }

    #[test]
    fn crafted_dbf_errors_cleanly() {
        // Header length beyond the file, field descriptors cut short.
        let mut dbf = dbf_with(1, 4000, 10);
        dbf.extend_from_slice(&[b'A'; 40]);
        assert!(read_dbf(&dbf).is_err());
        // Billions of records claimed, none there (also of length 0).
        for record in [u16::MAX, 0] {
            let mut dbf = dbf_with(u32::MAX, 33, record);
            dbf.push(0x0d);
            assert!(read_dbf(&dbf).is_err());
        }
    }

    #[test]
    fn crafted_zip_errors_cleanly() {
        // A zip whose .shp holds a polyline claiming billions of points.
        let c = polyline(1, i32::MAX);
        let shp = shp_with(c.len() as i32 / 2, &c);
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("a.shp", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&shp).unwrap();
        let data = zip.finish().unwrap().into_inner();
        assert!(read(&data, &Options::default()).is_err());
    }
}
