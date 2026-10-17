// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols-structure` — OLS Structure, the structure application of OLS CAD:
//! one structure file (`.olss`) in its own window, built from level 1
//! (stick figure) to level 3 (full design). The menus are at the top, the
//! panels under them can be moved, closed and shown again (View).
//!
//! ```bash
//! ols-structure examples/synthetic-line/Structures/type-02.olss
//! ```
//!
//! The line application (`ols-cad`) starts it with **Edit** on a structure
//! and reads the file again when it is saved. On a placed structure it
//! also hands over that place's loads:
//!
//! ```bash
//! ols-structure Structures/type-02.olss --structure T2 \
//!     --line-values Structures/type-02.T2.line-values.json --sections Sections
//! ```
//!
//! `ols-structure --version` prints the version and the build id (the git
//! commit it was built from), e.g. `ols-structure 0.0.1 (build
//! 1a2b3c4d5e6f)`: the line application checks it before it starts this
//! program.

mod app;
mod body_tab;
mod export_menu;
mod extensions_panel;
mod generators_ui;
mod in_line_tab;
mod layout;
mod load_cases_tab;
mod menus;
mod pole_panel;
mod results_tab;
mod tables;
mod view;

use std::path::PathBuf;

use clap::Parser;
use eframe::egui;

#[cfg(test)]
mod body_ui_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod placed_tests;
#[cfg(test)]
mod pole_extension_ui_tests;
#[cfg(test)]
mod tests;

/// The version `--version` prints: the workspace version and the build id
/// (the git commit built from). The line application compares it with its
/// own before it starts this program: a different one means the two were
/// built from different code.
pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (build ",
    env!("OLS_BUILD_ID"),
    ")"
);

/// OLS Structure, the structure application of OLS CAD: a structure file
/// (.olss) in 3D and in tables, every edit undoable.
#[derive(Parser)]
#[command(name = "ols-structure", version = VERSION)]
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
    /// The placed structure whose loads these are: values typed over the
    /// load table are kept as NAME.STRUCTURE.loads.csv beside the file
    /// (the line application stores them with the placed structure).
    #[arg(long)]
    structure: Option<String>,
    /// The line's values at the placed structure (a line-values file,
    /// JSON, written by the line application); read again when it
    /// changes.
    #[arg(long, value_name = "FILE")]
    line_values: Option<PathBuf>,
    /// Section library (a .olssec file or a folder of them) for the
    /// members' areas; default: the Sections folder beside the file.
    #[arg(long, value_name = "FILE|DIR")]
    sections: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu=warn,naga=warn,eframe=warn,egui_wgpu=warn"),
    )
    .init();
    let args = Args::parse();
    if args.file.is_none() && (args.structure.is_some() || args.line_values.is_some()) {
        eprintln!("error: --structure and --line-values need the structure file");
        return std::process::ExitCode::from(2);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OLS Structure")
            .with_inner_size([1300.0, 900.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "OLS Structure",
        options,
        Box::new(move |cc| {
            let mut app = app::StructureApp::new(cc.wgpu_render_state.clone());
            app.restore(cc.storage);
            app.ahead = args.ahead;
            app.insulators_dir = args.insulators.clone();
            app.sections_path = args.sections.clone();
            app.line_values = args.line_values.clone();
            app.loads.placed = args.structure.clone();
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

#[cfg(test)]
mod version_tests {
    use clap::Parser;

    /// `--version` prints the version and the build id (the line
    /// application compares it with its own).
    #[test]
    fn the_version_names_the_build() {
        let Err(e) = super::Args::try_parse_from(["ols-structure", "--version"]) else {
            panic!("--version parsed as a run");
        };
        assert_eq!(e.kind(), clap::error::ErrorKind::DisplayVersion);
        let text = e.to_string();
        assert_eq!(text.trim(), format!("ols-structure {}", super::VERSION));
        assert!(super::VERSION.starts_with(env!("CARGO_PKG_VERSION")));
        assert!(super::VERSION.contains("(build "));
    }
}
