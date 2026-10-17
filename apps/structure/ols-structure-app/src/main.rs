// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols-structure` — the structure application of OLS CAD: one structure
//! file (`.olss`) in its own window, built from level 1 (stick figure) to
//! level 3 (full design).
//!
//! ```bash
//! ols-structure examples/synthetic-line/Structures/type-02.olss
//! ```
//!
//! The line application (`ols-cad`) starts it with **Edit** on a structure
//! and reads the file again when it is saved.

mod app;
mod load_cases_tab;
mod results_tab;
mod tables;
mod view;

use std::path::PathBuf;

use clap::Parser;
use eframe::egui;

#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod tests;

/// OLS CAD structure application: a structure file (.olss) in 3D and in
/// tables, every edit undoable.
#[derive(Parser)]
#[command(name = "ols-structure", version)]
struct Args {
    /// Structure file (.olss); none: open one from the window.
    file: Option<PathBuf>,
    /// Insulator library folder (the assemblies strings may take);
    /// default: the folder of the file and its Insulators folder.
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// The plan angle that means ahead along the line, degrees (the
    /// compass).
    #[arg(long, default_value_t = 0.0)]
    ahead: f64,
    /// Reserved: a project file (.ols) whose structure to edit.
    #[arg(long)]
    project: Option<PathBuf>,
    /// Reserved: with --project, the project line.
    #[arg(long)]
    line: Option<String>,
    /// Reserved: with --project, the placed structure.
    #[arg(long)]
    structure: Option<String>,
}

fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu=warn,naga=warn,eframe=warn,egui_wgpu=warn"),
    )
    .init();
    let args = Args::parse();
    if args.project.is_some() || args.line.is_some() || args.structure.is_some() {
        eprintln!(
            "error: --project, --line and --structure are not built yet: open the structure's \
             file (ols-structure FILE.olss), or use Edit on the structure in the line application"
        );
        return std::process::ExitCode::from(2);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OLS CAD structure")
            .with_inner_size([1300.0, 900.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "OLS CAD structure",
        options,
        Box::new(move |cc| {
            let mut app = app::StructureApp::new(cc.wgpu_render_state.clone());
            app.ahead = args.ahead;
            app.insulators_dir = args.insulators.clone();
            if let Some(f) = &args.file {
                app.open(f);
            }
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
