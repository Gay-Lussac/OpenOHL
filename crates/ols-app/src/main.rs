// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols-cad` — the OLS CAD desktop application (Phase 3).
//!
//! ```bash
//! ols-cad examples/synthetic-line/synthetic-line.ols.json \
//!     --terrain examples/synthetic-line/synthetic-survey.csv \
//!     --weather examples/synthetic-line/weather-cases.olswc
//! ```

use std::path::PathBuf;

use clap::Parser;
use eframe::egui;

mod app;
mod controls;
mod model;
mod plan;
mod profile;
mod properties;
mod scene;
mod selection;
mod view3d;

#[cfg(test)]
mod tests;

/// OLS CAD — view a line model in 3D, plan and profile.
#[derive(Parser)]
#[command(name = "ols-cad", version)]
struct Args {
    /// Line file (.ols.json) or project file (.ols).
    file: PathBuf,
    /// Wire library (file or directory); default: *.olsw next to the file,
    /// else libraries/cables.
    #[arg(long)]
    library: Option<PathBuf>,
    /// Pylon files directory; default: Structures/ next to the file.
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator library directory.
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Weather case file (.olswc); default: still air at 15 °C.
    #[arg(long)]
    weather: Option<PathBuf>,
    /// Survey points for the ground (CSV / XYZ).
    #[arg(long)]
    terrain: Option<PathBuf>,
    /// Point cloud in a project file (FILE.ols or FILE.ols#name); default:
    /// the project's own.
    #[arg(long)]
    pointcloud: Option<String>,
    /// Most LiDAR points shown.
    #[arg(long, default_value_t = 2_000_000)]
    points: usize,
    /// Half width of the LiDAR corridor around the spans, m.
    #[arg(long, default_value_t = 60.0)]
    corridor: f64,
    /// Instead of opening a window: load, render the 3D view to this PNG
    /// file and exit (checks, documentation).
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// With --screenshot: look at this structure from 250 m.
    #[arg(long)]
    focus: Option<String>,
}

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let sources = model::Sources {
        file: args.file,
        library: args.library,
        structures: args.structures,
        insulators: args.insulators,
        weather: args.weather,
        terrain: args.terrain,
        pointcloud: args.pointcloud,
        point_budget: args.points,
        corridor: args.corridor,
    };
    if let Some(path) = args.screenshot {
        return match screenshot(sources, &path, args.focus.as_deref()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OLS CAD")
            .with_inner_size([1500.0, 950.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "OLS CAD",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::App::new(
                sources,
                cc.wgpu_render_state.clone(),
                &cc.egui_ctx,
            )))
        }),
    );
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Loads the model and writes the 3D view (all layers but other cases) as
/// a PNG.
fn screenshot(
    sources: model::Sources,
    path: &std::path::Path,
    focus: Option<&str>,
) -> Result<(), String> {
    let model = model::load(sources, &|m| eprintln!("{m}"))?;
    let sel = selection::Selection::None;
    let scenes: Vec<ols_render::Scene> = scene::Layer::ALL
        .into_iter()
        .filter(|l| *l != scene::Layer::OtherCases)
        .map(|l| scene::layer(&model, l, 0, &sel))
        .collect();
    let bounds = scene::layer(&model, scene::Layer::Structures, 0, &sel)
        .bounds()
        .or_else(|| scenes.iter().find_map(|s| s.bounds()))
        .ok_or("nothing to show")?;
    let mut camera = ols_render::Camera::fit(&bounds);
    if let Some(id) = focus {
        let sel = selection::Selection::Structure(id.to_owned());
        camera.target = sel
            .focus(&model, 0)
            .ok_or_else(|| format!("no structure {id}"))?;
        camera.distance = 250.0;
        camera.pitch = 20f64.to_radians();
    }
    let refs: Vec<&ols_render::Scene> = scenes.iter().collect();
    let (w, h) = (1600, 1000);
    let rgba = ols_render::render_rgba(&refs, &camera, w, h)?;
    write_png(path, w, h, &rgba)?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .map_err(|e| e.to_string())
}
