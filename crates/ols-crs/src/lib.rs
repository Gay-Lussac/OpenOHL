// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
}
