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

mod annotations;
mod app;
mod bom;
mod chunks;
mod class_colors;
mod clearance;
mod codes;
mod controls;
mod drawings;
mod edit;
mod editors;
mod files;
mod galloping;
mod job;
mod library;
mod loads;
mod lod;
mod menu;
mod model;
mod plan;
mod plugins;
mod profile;
mod properties;
mod python;
mod remote;
mod scene;
mod selection;
mod spot;
mod surfaces;
mod survey_io;
mod tables;
mod thermal;
mod tools;
mod vegetation;
mod view3d;
mod wirefit;

#[cfg(test)]
mod tests;

/// OLS CAD — view a line model in 3D, plan and profile.
#[derive(Parser)]
#[command(name = "ols-cad", version)]
struct Args {
    /// Line file (.ols.json) or project file (.ols); none: the start page
    /// (File › New project, Open).
    file: Option<PathBuf>,
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
    /// User editing the project (default: the login name).
    #[arg(long)]
    user: Option<String>,
    /// Clearance rules (.olsr) for the Clearances tab; default: the only
    /// .olsr next to the file.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Load cases (.olslc) for the structure loads; default: the
    /// project's own, else the first .olslc next to the file.
    #[arg(long)]
    load_cases: Option<PathBuf>,
    /// LiDAR class scheme (.olsc: class names, roles, display colours)
    /// used instead of the point cloud's own.
    #[arg(long)]
    class_scheme: Option<PathBuf>,
    /// Instead of opening a window: load, render the 3D view to this PNG
    /// file and exit (checks, documentation).
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Navigation scheme of the 3D view: orbit or minecraft (default: the
    /// one used last).
    #[arg(long)]
    controls: Option<String>,
    /// With --screenshot: look at this structure's first attachment.
    #[arg(long)]
    focus: Option<String>,
    /// With --focus: from this distance, m.
    #[arg(long, default_value_t = 250.0)]
    distance: f64,
}

fn main() -> std::process::ExitCode {
    // Warnings (e.g. a cursor lock refused by the system) and `RUST_LOG`
    // to the terminal.
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu=warn,naga=warn,eframe=warn,egui_wgpu=warn"),
    )
    .init();
    let args = Args::parse();
    let open = args.file.is_some();
    let sources = model::Sources {
        file: args.file.unwrap_or_default(),
        library: args.library,
        structures: args.structures,
        insulators: args.insulators,
        weather: args.weather,
        terrain: args.terrain,
        pointcloud: args.pointcloud,
        point_budget: args.points,
        corridor: args.corridor,
        user: args.user,
        rules: args.rules,
        load_cases: args.load_cases,
        view: None,
        class_scheme: args.class_scheme,
    };
    if let Some(path) = args.screenshot {
        if !open {
            eprintln!("error: --screenshot needs a file");
            return std::process::ExitCode::FAILURE;
        }
        return match screenshot(sources, &path, args.focus.as_deref(), args.distance) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let chosen = match &args.controls {
        Some(name) => match controls::Controls::by_name(name) {
            Some(c) => Some(c),
            None => {
                eprintln!("error: --controls {name}: use orbit or minecraft");
                return std::process::ExitCode::FAILURE;
            }
        },
        None => None,
    };
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
            let rs = cc.wgpu_render_state.clone();
            let mut app = app::App::empty(sources.clone(), rs);
            if let Some(st) = cc.storage {
                app.recent = files::Recent::load(st);
                app.view_prefs = model::ViewPrefsStore::load(st);
                app.default_class_scheme = st
                    .get_string("class-scheme")
                    .filter(|s| !s.is_empty())
                    .map(std::path::PathBuf::from);
            }
            if open {
                app.start(sources, &cc.egui_ctx);
            }
            // Given, else the one used last, else the default.
            app.controls = chosen
                .or_else(|| cc.storage.and_then(controls::Controls::load))
                .unwrap_or_default();
            log::info!("controls: {}", app.controls.name);
            if let Some(s) = cc.storage.and_then(selection::Snaps::load) {
                app.snaps = s;
            }
            if let Some(h) = cc.storage.and_then(tools::Hotbar::load) {
                app.hotbar = h;
            }
            if let Some(st) = cc.storage {
                app.lod = lod::LodSettings::load(st);
            }
            if let Some(q) = cc.storage.and_then(menu::QuickBar::load) {
                app.quickbar = q;
            }
            app.plugins = plugins::Plugins::load(cc.storage, plugins::Plugins::default_user_dir());
            Ok(Box::new(app))
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
    distance: f64,
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
        // Its first attachment's clamp (where the wires are).
        let s = model
            .line
            .structure(id)
            .ok_or_else(|| format!("no structure {id}"))?;
        camera.target = s.attachments.first().map_or(s.position, |a| a.wire_point);
        camera.distance = distance;
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
