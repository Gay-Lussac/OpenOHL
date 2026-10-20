// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Coordinate reference systems and transformations.
//!
//! A [`Crs`] is given as `EPSG:<code>` (definitions from the EPSG registry
//! as proj4 strings) or as a proj4 string (`+proj=…`); a [`Transformation`]
//! converts points between two of them. Pure Rust ([`proj4rs`], a port of
//! proj4js): no native library, also in WebAssembly plugins.
//!
//! Conventions:
//!
//! - Geographic coordinates are **longitude, latitude in degrees** (x, y),
//!   projected ones in the CRS's units (metres for most).
//! - **Heights are kept** by default ([`Heights::Keep`]): line, survey and
//!   LiDAR heights are usually orthometric (above a geoid), and no geoid
//!   model is applied here. [`Heights::Ellipsoidal`] passes them through
//!   the datum shift as ellipsoidal heights instead.
//! - **Accuracy**: datum changes use the definitions' Helmert parameters
//!   (`+towgs84`), not grid files. For Switzerland, EPSG:2056 (LV95) ↔
//!   WGS84 matches swisstopo's official REFRAME service to under 1 mm in
//!   position and height (tested), with WGS84 taken as CHTRS95/ETRS89 like
//!   REFRAME does. Datums that need distortion grids (e.g. LV03 ↔ LV95,
//!   NAD27) are approximated by their Helmert parameters only; a PROJ-based
//!   plugin with grids can provide those behind the same interface.
//!   Projection changes within one datum are exact to the millimetre.

use std::fmt;

use proj4rs::Proj;

/// A coordinate reference system.
#[derive(Clone)]
pub struct Crs {
    name: String,
    definition: String,
    proj: Proj,
}

impl fmt::Debug for Crs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Crs")
            .field("name", &self.name)
            .field("definition", &self.definition)
            .finish()
    }
}

impl PartialEq for Crs {
    fn eq(&self, other: &Self) -> bool {
        self.definition == other.definition
    }
}

impl Crs {
    /// Parses `EPSG:2056` (case-insensitive) or a proj4 string.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let spec = spec.trim();
        let lower = spec.to_ascii_lowercase();
        if let Some(code) = lower.strip_prefix("epsg:") {
            let code: u16 = code
                .trim()
                .parse()
                .map_err(|_| format!("\"{spec}\": EPSG codes are numbers up to 65535"))?;
            let def = crs_definitions::from_code(code)
                .ok_or_else(|| format!("EPSG:{code} is not a known coordinate system"))?;
            let proj =
                Proj::from_proj_string(def.proj4).map_err(|e| format!("EPSG:{code}: {e:?}"))?;
            return Ok(Self {
                name: format!("EPSG:{code}"),
                definition: def.proj4.to_owned(),
                proj,
            });
        }
        if spec.starts_with('+') {
            let proj = Proj::from_proj_string(spec).map_err(|e| format!("\"{spec}\": {e:?}"))?;
            return Ok(Self {
                name: spec.to_owned(),
                definition: spec.to_owned(),
                proj,
            });
        }
        Err(format!(
            "\"{spec}\" is not a coordinate system: use EPSG:<code> or a proj4 string (+proj=…)"
        ))
    }

    /// `EPSG:<code>`, or the proj4 string it was given as.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its proj4 definition.
    pub fn definition(&self) -> &str {
        &self.definition
    }

    /// Whether coordinates are longitude and latitude.
    pub fn is_geographic(&self) -> bool {
        self.proj.is_latlong()
    }
}

/// The unit of a system's coordinates, as its proj4 definition gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    /// Degrees (longitude, latitude).
    Degree,
    /// A proj4 unit name (`+units=`: `m`, `ft`, `us-ft`, `link`…).
    Named(&'static str),
    /// A unit of this many metres (`+to_meter=`).
    Metres(f64),
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unit::Degree => write!(f, "degree"),
            Unit::Named(n) => write!(f, "{n}"),
            Unit::Metres(m) => write!(f, "{m} m"),
        }
    }
}

/// The value of `+key=` in a proj4 string.
fn proj4_value(proj4: &'static str, key: &str) -> Option<&'static str> {
    proj4
        .split_whitespace()
        .find_map(|t| t.strip_prefix('+')?.strip_prefix(key)?.strip_prefix('='))
}

/// The unit of a proj4 definition: degrees for longitude-latitude, else
/// `+units=`, else `+to_meter=`, else metres (proj4's default).
fn proj4_unit(proj4: &'static str) -> Unit {
    if proj4_value(proj4, "proj") == Some("longlat") {
        Unit::Degree
    } else if let Some(u) = proj4_value(proj4, "units") {
        Unit::Named(u)
    } else if let Some(m) = proj4_value(proj4, "to_meter").and_then(|v| v.parse().ok()) {
        Unit::Metres(m)
    } else {
        Unit::Named("m")
    }
}

/// A coordinate system of the EPSG registry: its code and name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Known {
    /// EPSG code.
    pub code: u16,
    /// Name, e.g. `CH1903+ / LV95`.
    pub name: &'static str,
    /// Whether coordinates are longitude and latitude.
    pub geographic: bool,
    /// Whether coordinates are geocentric (X, Y, Z from the earth's
    /// centre): not offered for a project ([`search`] leaves them out).
    pub geocentric: bool,
    /// The unit of its coordinates.
    pub unit: Unit,
    /// Its area of use, when the registry's definition names one (most do
    /// not).
    pub area: Option<&'static str>,
}

impl Known {
    /// `EPSG:<code>`: what [`Crs::parse`] takes.
    pub fn spec(&self) -> String {
        format!("EPSG:{}", self.code)
    }
}

impl fmt::Display for Known {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.name)
    }
}

/// The first quoted string of a WKT text: the system's name.
fn wkt_name(wkt: &str) -> Option<&str> {
    let start = wkt.find('"')? + 1;
    let len = wkt[start..].find('"')?;
    Some(&wkt[start..start + len])
}

/// The first `AREA["…"]` of a WKT 2 text: the system's area of use.
fn wkt_area(wkt: &str) -> Option<&str> {
    let start = wkt.find("AREA[\"")? + 6;
    let len = wkt[start..].find('"')?;
    Some(&wkt[start..start + len]).filter(|a| !a.is_empty())
}

/// Every coordinate system of the EPSG registry this program can
/// transform, by code: those whose definition the projection library
/// reads (listed once, in a fraction of a second). Codes of the registry
/// it cannot compute (projections it does not have) are left out.
pub fn known() -> &'static [Known] {
    static LIST: std::sync::OnceLock<Vec<Known>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        (0..=u16::MAX)
            .filter_map(crs_definitions::from_code)
            .filter_map(|d| {
                let proj = Proj::from_proj_string(d.proj4).ok()?;
                Some(Known {
                    code: d.code,
                    name: wkt_name(d.wkt).unwrap_or(""),
                    geographic: proj.is_latlong(),
                    geocentric: proj4_value(d.proj4, "proj") == Some("geocent"),
                    unit: proj4_unit(d.proj4),
                    area: wkt_area(d.wkt),
                })
            })
            .collect()
    })
}

/// Lower case, every character but letters, digits, `+`, `-` and `.` a space
/// (`CH1903+_LV95` and `CH1903+ / LV95` read alike).
fn normalized(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '+' | '-' | '.') {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The systems a project can use (geocentric ones left out) matching
/// `query`, best first: words of the code, name, area of use or unit
/// (every word must appear, in any order, case and punctuation ignored;
/// `EPSG:` before a code is ignored). An exact code comes first, then
/// an exact name, codes starting with the digits typed, names starting
/// with the query, then the rest by code. An empty query lists them all
/// by code.
pub fn search(query: &str) -> Vec<&'static Known> {
    let lower = query.trim().to_lowercase();
    let q = normalized(lower.strip_prefix("epsg:").unwrap_or(&lower));
    let words: Vec<&str> = q.split_whitespace().collect();
    let digits = (!q.is_empty() && q.chars().all(|c| c.is_ascii_digit())).then_some(q.as_str());
    let mut hits: Vec<(u8, &'static Known)> = known()
        .iter()
        .filter(|k| !k.geocentric)
        .filter_map(|k| {
            if words.is_empty() {
                return Some((4, k));
            }
            let code = k.code.to_string();
            if let Some(d) = digits {
                if code == d {
                    return Some((0, k));
                }
                if code.starts_with(d) {
                    return Some((2, k));
                }
            }
            let name = normalized(k.name);
            let hay = format!(
                "{code} {name} {} {}",
                k.area.map(normalized).unwrap_or_default(),
                normalized(&k.unit.to_string())
            );
            if !words.iter().all(|w| hay.contains(w)) {
                return None;
            }
            let rank = if name == q {
                1
            } else if name.starts_with(&q) {
                3
            } else {
                4
            };
            Some((rank, k))
        })
        .collect();
    hits.sort_by_key(|(rank, k)| (*rank, k.code));
    hits.into_iter().map(|(_, k)| k).collect()
}

/// The EPSG system with this code.
pub fn known_code(code: u16) -> Option<Known> {
    known()
        .binary_search_by_key(&code, |k| k.code)
        .ok()
        .map(|i| known()[i])
}

/// A name for a coordinate system given as [`Crs::parse`] takes it:
/// `EPSG:2056 — CH1903+ / LV95`, or the text as it is.
pub fn describe(spec: &str) -> String {
    let spec = spec.trim();
    if spec.is_empty() {
        return "not defined".into();
    }
    spec.to_ascii_lowercase()
        .strip_prefix("epsg:")
        .and_then(|c| c.trim().parse().ok())
        .and_then(known_code)
        .map_or_else(
            || spec.to_owned(),
            |k| format!("EPSG:{} — {}", k.code, k.name),
        )
}

/// The coordinate system of a `.prj` file: WKT naming its EPSG code
/// (`AUTHORITY["EPSG","2056"]` or `ID["EPSG",2056]` of the whole system),
/// WKT whose name is that of an EPSG system, or a proj4 string. Returns
/// what [`Crs::parse`] takes. Other WKT is not read (no parser here): give
/// its EPSG code instead.
pub fn from_prj(text: &str) -> Result<String, String> {
    let text = text.trim().trim_start_matches('\u{feff}');
    if text.starts_with('+') {
        Crs::parse(text)?;
        return Ok(text.to_owned());
    }
    // The system's own code is its last authority (the inner ones are its
    // datum, units…).
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let upper = compact.to_ascii_uppercase();
    let code = ["AUTHORITY[\"EPSG\",\"", "ID[\"EPSG\","]
        .iter()
        .filter_map(|key| upper.rfind(key).map(|at| at + key.len()))
        .max()
        .and_then(|at| {
            let digits: String = upper[at..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits.parse::<u16>().ok()
        });
    if let Some(code) = code {
        let spec = format!("EPSG:{code}");
        Crs::parse(&spec)?;
        return Ok(spec);
    }
    let name = wkt_name(text).ok_or("the file is neither WKT nor a proj4 string")?;
    // Some GIS programs write `CH1903+_LV95` for `CH1903+ / LV95`: compare letters,
    // digits and `+` only.
    let wanted = |n: &str| -> String {
        n.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '+')
            .collect::<String>()
            .to_ascii_lowercase()
    };
    known()
        .iter()
        .find(|k| wanted(k.name) == wanted(name))
        .map(Known::spec)
        .ok_or_else(|| {
            format!(
                "\"{name}\": no EPSG system of that name, and the file gives no EPSG code — \
                 choose the system from the EPSG list"
            )
        })
}

/// What happens to heights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Heights {
    /// Heights are kept as they are (orthometric heights; no geoid).
    #[default]
    Keep,
    /// Heights are ellipsoidal and go through the datum shift.
    Ellipsoidal,
}

/// A transformation between two coordinate systems.
#[derive(Debug, Clone)]
pub struct Transformation {
    from: Crs,
    to: Crs,
    heights: Heights,
}

impl Transformation {
    /// From `from` to `to`.
    pub fn new(from: &Crs, to: &Crs, heights: Heights) -> Self {
        Self {
            from: from.clone(),
            to: to.clone(),
            heights,
        }
    }

    /// Transforms one point (x, y, z): geographic in degrees.
    pub fn point(&self, p: [f64; 3]) -> Result<[f64; 3], String> {
        let mut out = [p];
        self.points(&mut out)?;
        Ok(out[0])
    }

    /// Transforms points in place.
    pub fn points(&self, points: &mut [[f64; 3]]) -> Result<(), String> {
        let (src_geo, dst_geo) = (self.from.is_geographic(), self.to.is_geographic());
        let mut work: Vec<(f64, f64, f64)> = points
            .iter()
            .map(|p| {
                let z = match self.heights {
                    Heights::Keep => 0.0,
                    Heights::Ellipsoidal => p[2],
                };
                if src_geo {
                    (p[0].to_radians(), p[1].to_radians(), z)
                } else {
                    (p[0], p[1], z)
                }
            })
            .collect();
        proj4rs::transform::transform(&self.from.proj, &self.to.proj, work.as_mut_slice())
            .map_err(|e| format!("{} → {}: {e:?}", self.from.name, self.to.name))?;
        for (p, w) in points.iter_mut().zip(work) {
            let (x, y) = if dst_geo {
                (w.0.to_degrees(), w.1.to_degrees())
            } else {
                (w.0, w.1)
            };
            if !(x.is_finite() && y.is_finite()) {
                return Err(format!(
                    "{} → {}: point ({}, {}) has no image",
                    self.from.name, self.to.name, p[0], p[1]
                ));
            }
            p[0] = x;
            p[1] = y;
            if self.heights == Heights::Ellipsoidal {
                p[2] = w.2;
            }
        }
        Ok(())
    }

    /// The reverse transformation.
    pub fn inverse(&self) -> Self {
        Self::new(&self.to, &self.from, self.heights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground distance between two (lon, lat) points in degrees, m
    /// (spherical, fine for metre-level comparisons).
    fn metres(a: (f64, f64), b: (f64, f64)) -> f64 {
        let r = 6_371_000.0;
        let (dlon, dlat) = ((a.0 - b.0).to_radians(), (a.1 - b.1).to_radians());
        r * dlat.hypot(dlon * a.1.to_radians().cos())
    }

    #[test]
    fn parses_codes_and_strings() {
        let lv95 = Crs::parse("epsg:2056").unwrap();
        assert_eq!(lv95.name(), "EPSG:2056");
        assert!(!lv95.is_geographic());
        assert!(Crs::parse("EPSG:4326").unwrap().is_geographic());
        assert!(
            Crs::parse("+proj=longlat +datum=WGS84 +no_defs")
                .unwrap()
                .is_geographic()
        );
        assert!(Crs::parse("EPSG:99999").is_err());
        assert!(Crs::parse("local (invented)").is_err());
    }

    /// LV95 → WGS84 against swisstopo's official transformation service
    /// REFRAME (geodesy.geo.admin.ch, queried 2026-10-03 with altitude 0):
    /// longitude, latitude and ellipsoidal height.
    const REFRAME: [(f64, f64, f64, f64, f64); 9] = [
        (
            2600000.0,
            1200000.0,
            7.438632420871814,
            46.9510827728495,
            49.6221914421767,
        ),
        (
            2500000.0,
            1120000.0,
            6.1425255108907235,
            46.223995742656356,
            52.44100270885974,
        ),
        (
            2830000.0,
            1290000.0,
            10.504611639663038,
            47.72014215910416,
            44.32900072913617,
        ),
        (
            2490000.0,
            1290000.0,
            5.971694009386793,
            47.75129645806702,
            48.59210336674005,
        ),
        (
            2830000.0,
            1080000.0,
            10.399014105425751,
            45.83258086667149,
            49.1191911669448,
        ),
        (
            2700000.0,
            1250000.0,
            8.763207927151976,
            47.393232220228654,
            47.210552469827235,
        ),
        (
            2598500.0,
            1120600.0,
            7.419199346413073,
            46.23683808183613,
            51.41307990439236,
        ),
        (
            2560000.0,
            1180000.0,
            6.91495346539894,
            46.76997078433277,
            50.517186905257404,
        ),
        (
            2750000.0,
            1150000.0,
            9.392286411581303,
            46.484515062688246,
            48.82676905952394,
        ),
    ];

    #[test]
    fn lv95_matches_swisstopo_reframe() {
        let (lv95, wgs84) = (
            Crs::parse("EPSG:2056").unwrap(),
            Crs::parse("EPSG:4326").unwrap(),
        );
        let keep = Transformation::new(&lv95, &wgs84, Heights::Keep);
        let ellipsoidal = Transformation::new(&lv95, &wgs84, Heights::Ellipsoidal);
        for (e, n, lon, lat, h) in REFRAME {
            let p = keep.point([e, n, 500.0]).unwrap();
            let d = metres((p[0], p[1]), (lon, lat));
            assert!(d < 0.01, "({e}, {n}): {d} m from REFRAME");
            assert_eq!(p[2], 500.0, "heights are kept");
            // Ellipsoidal: the datum shift's height change, as REFRAME.
            let q = ellipsoidal.point([e, n, 0.0]).unwrap();
            assert!(
                (q[2] - h).abs() < 0.01,
                "({e}, {n}): height {} vs {h}",
                q[2]
            );
            // And back.
            let r = keep.inverse().point([lon, lat, 0.0]).unwrap();
            assert!((r[0] - e).hypot(r[1] - n) < 0.01, "({e}, {n}) back: {r:?}");
        }
    }

    #[test]
    fn utm_is_exact_and_round_trips() {
        // UTM 32N: the central meridian 9° E maps to E 500 000 m; at 45° N
        // the northing is 0.9996 × the WGS84 meridian arc (4 984 944.378 m).
        let utm = Transformation::new(
            &Crs::parse("EPSG:4326").unwrap(),
            &Crs::parse("EPSG:32632").unwrap(),
            Heights::Keep,
        );
        let p = utm.point([9.0, 45.0, 0.0]).unwrap();
        assert!((p[0] - 500_000.0).abs() < 1e-6, "{p:?}");
        assert!((p[1] - 0.9996 * 4_984_944.378).abs() < 0.01, "{p:?}");
        // Round trips LV95 → WGS84 → LV95 and LV95 → UTM → LV95: < 1 mm.
        let lv95 = Crs::parse("EPSG:2056").unwrap();
        for target in ["EPSG:4326", "EPSG:32632"] {
            let there = Transformation::new(&lv95, &Crs::parse(target).unwrap(), Heights::Keep);
            let back = there.inverse();
            let start = [2_598_496.0, 1_120_627.6, 649.5];
            let q = back.point(there.point(start).unwrap()).unwrap();
            // Heights kept: each direction shifts the datum at height 0 and
            // drops the height change (~50 m), so the round trip is not an
            // exact inverse — about 1 mm here.
            let d = (q[0] - start[0]).hypot(q[1] - start[1]);
            assert!(d < 5e-3, "{target}: {d} m");
            assert_eq!(q[2], start[2]);
        }
    }

    #[test]
    fn ellipsoidal_heights_go_through_the_datum_shift() {
        let t = Transformation::new(
            &Crs::parse("EPSG:2056").unwrap(),
            &Crs::parse("EPSG:4326").unwrap(),
            Heights::Ellipsoidal,
        );
        let p = t.point([2_600_000.0, 1_200_000.0, 500.0]).unwrap();
        // Bessel and WGS84 ellipsoids differ by ~50 m in height here.
        assert!(
            (p[2] - 500.0).abs() > 10.0 && (p[2] - 500.0).abs() < 100.0,
            "{p:?}"
        );
    }

    #[test]
    fn the_registry_is_listed_and_prj_files_read() {
        let list = known();
        assert!(list.len() > 1000, "{}", list.len());
        assert!(list.windows(2).all(|w| w[0].code < w[1].code));
        let lv95 = known_code(2056).unwrap();
        assert_eq!(lv95.to_string(), "2056: CH1903+ / LV95");
        assert!(!lv95.geographic);
        assert_eq!(lv95.unit, Unit::Named("m"));
        assert!(known_code(4326).unwrap().geographic);
        assert_eq!(known_code(4326).unwrap().unit, Unit::Degree);
        assert_eq!(describe("epsg:2056"), "EPSG:2056 — CH1903+ / LV95");
        assert_eq!(describe(""), "not defined");
        assert_eq!(describe("+proj=utm +zone=32"), "+proj=utm +zone=32");
        // WKT 1 with the code at the end (inner codes are the datum's…).
        let wkt = crs_definitions::from_code(2056).unwrap().wkt;
        assert_eq!(from_prj(wkt).unwrap(), "EPSG:2056");
        // Some GIS programs: no code, the name with underscores.
        let esri = r#"PROJCS["CH1903+_LV95",GEOGCS["GCS_CH1903+",DATUM["D_CH1903+",SPHEROID["Bessel_1841",6377397.155,299.1528128]],PRIMEM["Greenwich",0.0],UNIT["Degree",0.0174532925199433]],PROJECTION["Hotine_Oblique_Mercator_Azimuth_Center"],UNIT["Meter",1.0]]"#;
        assert_eq!(from_prj(esri).unwrap(), "EPSG:2056");
        // WKT 2.
        assert_eq!(
            from_prj(r#"PROJCRS["x", BASEGEOGCRS["y", ID["EPSG", 4150]], ID["EPSG", 2056]]"#)
                .unwrap(),
            "EPSG:2056"
        );
        assert_eq!(
            from_prj("+proj=utm +zone=32 +datum=WGS84 +units=m +no_defs\n").unwrap(),
            "+proj=utm +zone=32 +datum=WGS84 +units=m +no_defs"
        );
        assert!(from_prj(r#"PROJCS["Invented grid",UNIT["Meter",1]]"#).is_err());
        assert!(from_prj("hello").is_err());
    }

    /// Every system listed is one the library computes: each parses.
    #[test]
    fn every_system_listed_parses() {
        for k in known() {
            let crs = Crs::parse(&k.spec()).unwrap_or_else(|e| panic!("{}: {e}", k.code));
            assert_eq!(crs.is_geographic(), k.geographic, "{}", k.code);
        }
        // The library cannot compute every code of the registry: those
        // are left out.
        let all = (0..=u16::MAX)
            .filter_map(crs_definitions::from_code)
            .count();
        assert!(known().len() <= all);
        assert!(known().len() > all / 2, "{} of {all}", known().len());
        eprintln!("{} of {all} systems listed", known().len());
    }

    #[test]
    fn search_finds_by_code_name_words_and_unit() {
        let codes = |q: &str| search(q).iter().map(|k| k.code).collect::<Vec<_>>();
        // A code, with or without EPSG:, first.
        assert_eq!(codes("2056")[0], 2056);
        assert_eq!(codes("EPSG:2056")[0], 2056);
        assert_eq!(codes(" epsg: 4326 ")[0], 4326);
        // Codes starting with the digits next.
        let c = codes("205");
        assert!(c.contains(&2056));
        assert!(c[..2].iter().all(|c| c.to_string().starts_with("205")));
        // Words in any order, case and punctuation ignored.
        assert!(codes("lv95").contains(&2056));
        assert!(codes("LV95 ch1903+").contains(&2056));
        assert!(codes("CH1903+_LV95").contains(&2056));
        assert!(codes("utm 32n wgs").contains(&32632));
        assert!(!codes("utm 33n wgs").contains(&32632));
        // Names starting with the query before the rest.
        let wgs = codes("wgs 84");
        assert_eq!(wgs[0], 4326, "{:?}", &wgs[..5]);
        // The unit is searchable too.
        assert!(!search("us-ft").is_empty());
        assert!(
            search("us-ft")
                .iter()
                .all(|k| k.unit == Unit::Named("us-ft") || normalized(k.name).contains("us-ft"))
        );
        // Geocentric systems are not offered.
        assert!(search("").iter().all(|k| !k.geocentric));
        assert!(codes("no such system at all").is_empty());
    }

    #[test]
    fn units_and_areas_come_from_the_definitions() {
        assert_eq!(
            proj4_unit("+proj=longlat +datum=WGS84 +no_defs"),
            Unit::Degree
        );
        assert_eq!(
            proj4_unit("+proj=utm +zone=32 +units=us-ft"),
            Unit::Named("us-ft")
        );
        assert_eq!(proj4_unit("+proj=tmerc +to_meter=0.5"), Unit::Metres(0.5));
        assert_eq!(proj4_unit("+proj=tmerc"), Unit::Named("m"));
        assert_eq!(Unit::Metres(0.5).to_string(), "0.5 m");
        assert_eq!(
            wkt_area(r#"X["a",USAGE[SCOPE["s"],AREA["Somewhere"],BBOX[1,2,3,4]]]"#),
            Some("Somewhere")
        );
        assert_eq!(wkt_area(r#"PROJCS["a"]"#), None);
        // Some definitions of the registry carry an area of use.
        assert!(known().iter().any(|k| k.area.is_some()));
    }
}
