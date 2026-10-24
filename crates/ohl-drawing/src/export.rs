// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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

use crate::model::Item;
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
    /// Sheets of the optimised size and orientation instead
    /// ([`crate::paper`]; it takes the place of `page`).
    pub optimise: Option<crate::paper::Optimise>,
    /// Resolution of PNG images, dots per inch.
    pub dpi: f64,
}

impl Export {
    /// Whether the export writes sheets (not one continuous drawing).
    pub fn paged(&self) -> bool {
        self.page.is_some() || self.optimise.is_some()
    }
}

impl Default for Export {
    fn default() -> Self {
        Self {
            formats: vec![Format::Svg],
            page: None,
            optimise: None,
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
    /// it, elevations overlapping, a lower PNG resolution).
    pub warnings: Vec<String>,
    /// The optimised layout chosen ([`Export::optimise`]), in words.
    pub paper: Option<String>,
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
    crate::check_break_clearance(o.break_clearance)?;
    if e.paged()
        && let Some(why) = o.template.missing_fields(&o.fields)
    {
        return Err(why);
    }
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let mut run = budget.start();
    let mut written = Written::default();
    let route = file_stem(&d.route);
    let stem = if o.plan {
        route.clone()
    } else {
        format!("{route}-profile")
    };
    let drawings: Vec<crate::model::Drawing> = match (&e.optimise, &e.page) {
        (Some(opt), _) => {
            let best =
                crate::paper::best(d, o, opt).ok_or("no paper size to lay the sheets out on")?;
            written.paper = Some(format!(
                "{} ({}; {} warning(s), {:.2} m² of paper, {:.0} % filled)",
                best.label,
                best.sizes(),
                best.warnings,
                best.area,
                best.fill * 100.0
            ));
            let sheets = crate::paper::draw(d, o, &best);
            for s in &sheets {
                for w in &s.warnings {
                    written
                        .warnings
                        .push(format!("sheet {} ({}): {w}", s.number, s.page.name));
                }
            }
            sheets.into_iter().map(|s| s.drawing).collect()
        }
        (None, None) => {
            let (drawing, _, warnings) = crate::layout::continuous(d, o);
            written.warnings.extend(warnings);
            vec![drawing]
        }
        (None, Some(page)) => {
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
    let paged = e.paged();
    let name = |k: usize, ext: &str| {
        if paged {
            format!("{stem}-sheet-{:02}.{ext}", k + 1)
        } else {
            format!("{stem}.{ext}")
        }
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
                let mut left_out = 0;
                for (k, drawing) in drawings.iter().enumerate() {
                    let (text, images) = crate::dxf::write_counted(drawing);
                    left_out += images;
                    put(name(k, "dxf"), text.as_bytes(), &mut run)?;
                }
                if left_out > 0 {
                    written.warnings.push(format!(
                        "DXF: {left_out} PNG/JPEG logo image(s) left out of the sheets (R12 has \
                         no images; SVG logos are written as outlines)"
                    ));
                }
                let photos = drawings
                    .iter()
                    .filter(|d| {
                        d.flat()
                            .iter()
                            .any(|(i, _)| matches!(i, Item::Image(x) if x.pixels.is_some()))
                    })
                    .count();
                if photos > 0 {
                    written.warnings.push(format!(
                        "DXF: the orthophoto under the plan left out of {photos} sheet(s) (R12 \
                         has no images; the SVG, PDF and PNG hold it)"
                    ));
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

/// Writes the sheets of one of the other plans (the phase plan, the route
/// and situation plans: [`crate::plans`]) into `dir` as `stem`-NN.svg /
/// .dxf / .png and one `stem`.pdf, the formats of `e` that hold sheets
/// (CSV and XML are the profile's: not written).
pub fn write_plan(
    sheets: &[crate::Sheet],
    e: &Export,
    stem: &str,
    title: &str,
    dir: &Path,
    budget: &Budget,
) -> Result<Written, String> {
    std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let mut run = budget.start();
    let mut written = Written {
        sheets: sheets.len(),
        ..Written::default()
    };
    for s in sheets {
        for w in &s.warnings {
            written.warnings.push(format!("sheet {}: {w}", s.number));
        }
    }
    let stem = file_stem(stem);
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
    for format in formats {
        match format {
            Format::Svg => {
                for s in sheets {
                    let name = format!("{stem}-{:02}.svg", s.number);
                    put(name, s.drawing.svg().as_bytes(), &mut run)?;
                }
            }
            Format::Dxf => {
                for s in sheets {
                    let name = format!("{stem}-{:02}.dxf", s.number);
                    put(name, crate::dxf::write(&s.drawing).as_bytes(), &mut run)?;
                }
            }
            Format::Pdf => {
                let pages: Vec<&crate::model::Drawing> =
                    sheets.iter().map(|s| &s.drawing).collect();
                put(
                    format!("{stem}.pdf"),
                    &crate::pdf::write(&pages, title),
                    &mut run,
                )?;
            }
            Format::Png => {
                for s in sheets {
                    let (png, _) = crate::png::render(&s.drawing, e.dpi)?;
                    put(format!("{stem}-{:02}.png", s.number), &png, &mut run)?;
                }
            }
            Format::Csv | Format::Xml => {}
        }
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
            ..Default::default()
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
            ..Default::default()
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
