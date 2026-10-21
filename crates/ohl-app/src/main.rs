// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl-design` — OHL-Design, the line application of OpenOHL: sag-tension,
//! clearances and line layout (formerly `ols-cad`, still built as an
//! alias that runs this program).
//!
//! ```bash
//! ohl-design examples/synthetic-line/synthetic-line.ohl.json \
//!     --terrain examples/synthetic-line/synthetic-survey.csv \
//!     --weather examples/synthetic-line/weather-cases.ohlwc
//! ```

use std::path::PathBuf;

use clap::Parser;
use eframe::egui;

mod alignment_tools;
mod annotations;
mod app;
mod area;
mod backgrounds;
mod bom;
mod check_help;
mod check_library;
mod chunks;
mod circuit_join;
mod class_colors;
mod clearance;
mod clearance_zones;
mod codes;
mod color_tool;
mod controls;
mod crs;
mod dockable;
mod drawings;
mod edit;
mod editors;
mod files;
mod galloping;
mod ground_lines;
mod handles;
mod help;
mod icons;
mod job;
mod library;
mod lidar_edit;
mod loads;
mod lod;
mod menu;
mod model;
mod next_steps;
mod octree;
mod perf;
mod phase_distance;
mod placed_editor;
mod placement;
mod plan;
mod plugins;
mod preset_cases;
mod preview3d;
mod profile;
mod project_lines;
mod properties;
mod pylon_builder;
mod python;
mod remote;
mod replace;
mod requirement_ui;
mod revision;
mod rule_editor;
mod safety_volume;
mod scene;
mod selection;
mod sheet_ui;
mod span_tool;
mod spot;
mod starter;
mod structure_app;
mod structure_desk;
mod structure_picker;
mod surfaces;
mod survey_io;
mod tables;
mod thermal;
mod tools;
mod type_workspace;
mod vegetation;
mod versions;
mod view3d;
mod wire_style;
mod wirefit;
mod working_tree;

/// The application's name, as the window title shows it.
pub const APP_NAME: &str = "OHL-Design";

/// Where the settings, the recent files and the user's plugins are kept
/// (eframe's storage id, its data folder): the name from before OpenOHL,
/// kept so that nothing is lost. The structure application reads the
/// plugins there too.
pub const STORAGE_ID: &str = "OLS CAD"; // storage id: kept

#[cfg(test)]
mod area_tests;
#[cfg(test)]
mod bench;
#[cfg(test)]
mod glyph_tests;
#[cfg(test)]
mod ground_lines_tests;
#[cfg(test)]
mod handle_tests;
#[cfg(test)]
mod lidar_tests;
#[cfg(test)]
mod member_tests;
#[cfg(test)]
mod revision_tests;
#[cfg(test)]
mod rule_editor_tests;
#[cfg(test)]
mod safety_volume_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod ux_tests;
#[cfg(test)]
mod vegetation_tests;

/// OHL-Design, the line application of OpenOHL — view and edit a line
/// model in 3D, plan and profile.
#[derive(Parser)]
#[command(name = "ohl-design", version = crate::structure_app::VERSION)]
struct Args {
    /// Line file (.ohl.json) or project file (.ohl); none: the start page
    /// (File › New project, Open).
    file: Option<PathBuf>,
    /// Wire library (file or directory); default: *.ohlw next to the file,
    /// else libraries/cables.
    #[arg(long)]
    library: Option<PathBuf>,
    /// Pylon files directory; default: Structures/ next to the file.
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator library directory.
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Weather case file (.ohlwc); default: still air at 15 °C.
    #[arg(long)]
    weather: Option<PathBuf>,
    /// Survey points for the ground (CSV / XYZ).
    #[arg(long)]
    terrain: Option<PathBuf>,
    /// Point cloud in a project file (FILE.ohl or FILE.ohl#name) or a COPC
    /// file (FILE.copc.laz); default:
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
    /// Clearance rules (.ohlr) for the Clearances tab; default: the only
    /// .ohlr next to the file.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Load cases (.ohllc) for the structure loads; default: the
    /// project's own, else the first .ohllc next to the file.
    #[arg(long)]
    load_cases: Option<PathBuf>,
    /// LiDAR class scheme (.ohlc: class names, roles, display colours)
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
            .with_title(APP_NAME)
            .with_inner_size([1500.0, 950.0])
            .with_icon(icons::window_icon()),
        ..Default::default()
    };
    let result = eframe::run_native(
        STORAGE_ID,
        options,
        Box::new(move |cc| {
            let rs = cc.wgpu_render_state.clone();
            let mut app = app::App::empty(sources.clone(), rs);
            if let Some(st) = cc.storage {
                app.recent = files::Recent::load(st);
                app.view_prefs = model::ViewPrefsStore::load(st);
                app.palette = color_tool::Palette::load(st);
                app.ahead_alone = st
                    .get_string("ahead-alone")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                backgrounds::set_converter(&st.get_string("ecw-converter").unwrap_or_default());
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
            // The click option is kept also when --controls names a scheme.
            if let Some(st) = cc.storage {
                app.controls.tools_on_left = controls::Controls::load_tools_on_left(st);
            }
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
    let scenes: Vec<ohl_render::Scene> = scene::Layer::ALL
        .into_iter()
        .filter(|l| *l != scene::Layer::OtherCases)
        .map(|l| scene::layer(&model, l, 0, &sel))
        .collect();
    let bounds = scene::layer(&model, scene::Layer::Structures, 0, &sel)
        .bounds()
        .or_else(|| scenes.iter().find_map(|s| s.bounds()))
        .ok_or("nothing to show")?;
    let mut camera = ohl_render::Camera::fit(&bounds);
    if let Some(id) = focus {
        // Its first wire point (where the wires are).
        let s = model
            .line
            .structure(id)
            .ok_or_else(|| format!("no structure {id}"))?;
        camera.target = s.wires.first().map_or(s.position, |w| w.position);
        camera.distance = distance;
        camera.pitch = 20f64.to_radians();
    }
    let refs: Vec<&ohl_render::Scene> = scenes.iter().collect();
    let (w, h) = (1600, 1000);
    let rgba = ohl_render::render_rgba(&refs, &camera, w, h)?;
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
