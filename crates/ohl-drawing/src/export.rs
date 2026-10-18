// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Writes a route's drawings and profile in the formats chosen, the same
//! way for the command line and the desktop app.
//!
//! Files, for route `R` (`R-profile…` when the plan is left out):
//!
//! | format | one continuous drawing | sheets (`--page`) |
//! |---|---|---|
//! | SVG, PNG, DXF | `R.svg` … | `R-sheet-01.svg` … |
//! | PDF | `R.pdf` (one page) | `R.pdf` (a page per sheet) |
//! | DXF model space | `R-plan-model.dxf`, `R-profile-model.dxf` | the same |
//! | CSV, XML | `R-profile.csv`, `R-profile.xml` | the same |

use std::path::{Path, PathBuf};

use ohl_core::budget::Budget;

use crate::{Options, Page, RouteDrawing};

/// An output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    /// SVG sheets.
    Svg,
    /// DXF (R12) sheets and model space.
    Dxf,
    /// Vector PDF, a page per sheet.
    Pdf,
    /// PNG images of the sheets.
    Png,
    /// The profile's numbers as CSV.
    Csv,
    /// The profile's numbers as OHL profile XML.
    Xml,
}

impl Format {
    /// Every format.
    pub const ALL: [Self; 6] = [
        Self::Svg,
        Self::Dxf,
        Self::Pdf,
        Self::Png,
        Self::Csv,
        Self::Xml,
    ];

    /// Its name (and file extension).
    pub fn name(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Dxf => "dxf",
            Self::Pdf => "pdf",
            Self::Png => "png",
            Self::Csv => "csv",
            Self::Xml => "xml",
        }
    }

    /// What it writes, in a few words.
    pub fn description(self) -> &'static str {
        match self {
            Self::Svg => "SVG sheets",
            Self::Dxf => "DXF (R12): sheets in mm, plan and profile in model space",
            Self::Pdf => "PDF, a page per sheet",
            Self::Png => "PNG images of the sheets",
            Self::Csv => "profile numbers (CSV)",
            Self::Xml => "profile numbers (OHL profile XML)",
        }
    }

    /// The format named `name` (any case).
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// What to write.
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    /// Formats.
    pub formats: Vec<Format>,
    /// Sheets of this format (split at structures), or one continuous
    /// drawing.
    pub page: Option<Page>,
    /// Resolution of PNG images, dots per inch.
    pub dpi: f64,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            formats: vec![Format::Svg],
            page: None,
            dpi: 150.0,
        }
    }
}

/// What was written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Written {
    /// Files, in the order written.
    pub files: Vec<PathBuf>,
    /// Sheets per drawing format (1 for a continuous drawing).
    pub sheets: usize,
    /// Problems found (a span longer than the sheet, a profile taller than
    /// it, a lower PNG resolution).
    pub warnings: Vec<String>,
}

/// `id` as part of a file name: characters other than letters, digits,
/// `-`, `_` and `.` become `_`.
pub fn file_stem(id: &str) -> String {
    let s: String = id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "route".into() } else { s }
}

/// Writes route `d`'s drawings and profile into `dir` (created) in the
/// formats of `e`, at the scales and station range of `o`.
///
/// Expected cost: a few ms per sheet for SVG, DXF and CSV, ≈ 0.1 s per
/// sheet for PDF, ≈ 0.5 s per A1 sheet at 150 dpi for PNG. Exit clause:
/// `budget` is checked before every file (time limit, cancel), and stops
/// the export with the reason.
pub fn write_route(
    d: &RouteDrawing,
    o: &Options,
    e: &Export,
    dir: &Path,
    budget: &Budget,
) -> Result<Written, String> {
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let mut run = budget.start();
    let mut written = Written::default();
    let route = file_stem(&d.route);
    let stem = if o.plan {
        route.clone()
    } else {
        format!("{route}-profile")
    };
    let drawings: Vec<crate::model::Drawing> = match &e.page {
        None => vec![crate::drawing(d, o)],
        Some(page) => {
            let sheets = crate::sheets(d, o, page);
            for s in &sheets {
                for w in &s.warnings {
                    written.warnings.push(format!("sheet {}: {w}", s.number));
                }
            }
            sheets.into_iter().map(|s| s.drawing).collect()
        }
    };
    written.sheets = drawings.len();
    let name = |k: usize, ext: &str| match e.page {
        None => format!("{stem}.{ext}"),
        Some(_) => format!("{stem}-sheet-{:02}.{ext}", k + 1),
    };
    let mut put = |file: String, bytes: &[u8], run: &mut ohl_core::budget::Run| {
        run.step().map_err(|s| format!("export {s}"))?;
        let path = dir.join(file);
        std::fs::write(&path, bytes).map_err(|err| format!("{}: {err}", path.display()))?;
        written.files.push(path);
        Ok::<(), String>(())
    };
    let mut formats = e.formats.clone();
    formats.sort();
    formats.dedup();
    let mut png_dpi: Option<f64> = None;
    for format in formats {
        match format {
            Format::Svg => {
                for (k, drawing) in drawings.iter().enumerate() {
                    put(name(k, "svg"), drawing.svg().as_bytes(), &mut run)?;
                }
            }
            Format::Dxf => {
                for (k, drawing) in drawings.iter().enumerate() {
                    put(
                        name(k, "dxf"),
                        crate::dxf::write(drawing).as_bytes(),
                        &mut run,
                    )?;
                }
                if o.plan {
                    let plan = crate::plan_model(d, o);
                    put(
                        format!("{route}-plan-model.dxf"),
                        crate::dxf::write(&plan).as_bytes(),
                        &mut run,
                    )?;
                }
                let profile = crate::profile_model(d, o);
                put(
                    format!("{route}-profile-model.dxf"),
                    crate::dxf::write(&profile).as_bytes(),
                    &mut run,
                )?;
            }
            Format::Pdf => {
                let pages: Vec<&crate::model::Drawing> = drawings.iter().collect();
                let title = format!("Route {} {}", d.route, d.name);
                put(
                    format!("{stem}.pdf"),
                    &crate::pdf::write(&pages, title.trim()),
                    &mut run,
                )?;
            }
            Format::Png => {
                for (k, drawing) in drawings.iter().enumerate() {
                    let (png, dpi) = crate::png::render(drawing, e.dpi)?;
                    if dpi < e.dpi - 0.5 {
                        png_dpi = Some(png_dpi.map_or(dpi, |p: f64| p.min(dpi)));
                    }
                    put(name(k, "png"), &png, &mut run)?;
                }
            }
            Format::Csv => put(
                format!("{route}-profile.csv"),
                crate::profile_csv(d, o).as_bytes(),
                &mut run,
            )?,
            Format::Xml => put(
                format!("{route}-profile.xml"),
                crate::profile_xml(d, o).as_bytes(),
                &mut run,
            )?,
        }
    }
    if let Some(dpi) = png_dpi {
        written.warnings.push(format!(
            "PNG drawn at {dpi:.0} dpi instead of {:.0} (at most {:.0} million pixels per image)",
            e.dpi,
            crate::png::MAX_PIXELS / 1e6
        ));
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page_format;
    use crate::tests::main_route;

    #[test]
    fn every_format_is_written() {
        let o = Options {
            vertical_scale: 500.0,
            clearance: Some(7.0),
            stations: Some([100.0, 1000.0]),
            ..Options::default()
        };
        let d = main_route(&o);
        let dir = std::env::temp_dir().join(format!("ohl-drawing-export-{}", std::process::id()));
        let e = Export {
            formats: Format::ALL.to_vec(),
            page: page_format("A3"),
            dpi: 50.0,
        };
        let w = write_route(&d, &o, &e, &dir, &Budget::new().max_iterations(100)).unwrap();
        let names: Vec<String> = w
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // 100–1000 on A3 (772 m per sheet): split at T3 (0+640).
        assert_eq!(w.sheets, 2, "{names:?}");
        for n in [
            "main-sheet-01.svg",
            "main-sheet-02.dxf",
            "main-plan-model.dxf",
            "main-profile-model.dxf",
            "main.pdf",
            "main-sheet-02.png",
            "main-profile.csv",
            "main-profile.xml",
        ] {
            assert!(names.iter().any(|x| x == n), "{n} not in {names:?}");
        }
        for p in &w.files {
            assert!(std::fs::metadata(p).unwrap().len() > 100, "{}", p.display());
        }
        // The exit clause: a budget of 3 files stops the export.
        let stopped = write_route(&d, &o, &e, &dir, &Budget::new().max_iterations(3));
        assert!(stopped.unwrap_err().contains("limit of 3 iterations"));
        // The profile alone, one continuous drawing.
        let profile = Options { plan: false, ..o };
        let e = Export {
            formats: vec![Format::Svg, Format::Dxf],
            page: None,
            dpi: 50.0,
        };
        let w = write_route(&d, &profile, &e, &dir, &Budget::new()).unwrap();
        let names: Vec<_> = w.files.iter().map(|p| p.file_name().unwrap()).collect();
        assert_eq!(
            names,
            [
                "main-profile.svg",
                "main-profile.dxf",
                "main-profile-model.dxf"
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(file_stem("a/b c"), "a_b_c");
        assert_eq!(Format::parse("PDF"), Some(Format::Pdf));
    }
}
