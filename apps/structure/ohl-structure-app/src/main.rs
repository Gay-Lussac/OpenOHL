// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl-structure` — OHL-Structure, the structure application of OpenOHL:
//! one structure file (`.ohls`) in its own window, built from level 1
//! (stick figure) to level 3 (full design). The menus are at the top, the
//! panels under them can be moved, closed and shown again (View).
//!
//! ```bash
//! ohl-structure examples/synthetic-line/Structures/type-02.ohls
//! ```
//!
//! The line application (`ohl-design`, OHL-Design) starts it with **Edit** on a structure
//! and reads the file again when it is saved. On a placed structure it
//! also hands over that place's loads:
//!
//! ```bash
//! ohl-structure Structures/type-02.ohls --structure T2 \
//!     --line-values Structures/type-02.T2.line-values.json --sections Sections
//! ```
//!
//! Without a file it opens on its start page: Open a structure…, the
//! files opened recently, New structure… (empty, or from a pylon
//! template). File › Libraries imports and exports the libraries a
//! structure uses; Plugins › Plugin manager shares the plugins and their
//! settings with the line application.
//!
//! `ohl-structure --version` prints the version and the build id (the git
//! commit it was built from), e.g. `ohl-structure 0.0.1 (build
//! 1a2b3c4d5e6f)`: the line application checks it before it starts this
//! program.

mod app;
mod body_tab;
mod case_editor;
mod export_menu;
mod extensions_panel;
mod generators_ui;
mod help;
mod icons;
mod in_line_tab;
mod inspector;
mod layout;
mod libraries;
mod link_tool;
mod load_cases_tab;
mod menus;
mod notices;
mod plugins_ui;
mod pole_panel;
mod results_tab;
mod section_editor;
mod start;
mod style;
mod tables;
mod tools;
mod view;

use std::path::PathBuf;

/// The application's name, as the window title shows it.
pub const APP_NAME: &str = "OHL-Structure";

/// Where its settings and recent files are kept (eframe's storage id,
/// its data folder): the name from before OpenOHL, kept so that nothing
/// is lost.
pub const STORAGE_ID: &str = "OLS Structure"; // storage id: kept

use clap::Parser;
use eframe::egui;

#[cfg(test)]
mod body_ui_tests;
#[cfg(test)]
mod case_editor_tests;
#[cfg(test)]
mod glyph_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod placed_tests;
#[cfg(test)]
mod pole_extension_ui_tests;
#[cfg(test)]
mod results_ui_tests;
#[cfg(test)]
mod section_editor_tests;
#[cfg(test)]
mod self_sufficient_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod ui_review_tests;

/// The version `--version` prints: the workspace version and the build id
/// (the git commit built from). The line application compares it with its
/// own before it starts this program: a different one means the two were
/// built from different code.
pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (build ",
    env!("OHL_BUILD_ID"),
    ")"
);

/// OHL-Structure, the structure application of OpenOHL: a structure file
/// (.ohls) in 3D and in tables, every edit undoable.
#[derive(Parser)]
#[command(name = "ohl-structure", version = VERSION)]
struct Args {
    /// Structure file (.ohls); none: open one from the window.
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
    /// Section library (a .ohlsec file or a folder of them) for the
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
            .with_title(APP_NAME)
            .with_inner_size([1300.0, 900.0])
            .with_icon(icons::window_icon()),
        ..Default::default()
    };
    let result = eframe::run_native(
        STORAGE_ID,
        options,
        Box::new(move |cc| {
            let mut app = app::StructureApp::new(cc.wgpu_render_state.clone());
            app.restore(cc.storage);
            app.ahead = args.ahead;
            app.insulators_dir = args.insulators.clone();
            app.sections_path = args.sections.clone();
            app.line_values = args.line_values.clone();
            app.loads.placed = args.structure.clone();
            // The plugins installed for the user and the shared settings
            // (the same as the line application's).
            app.plugins =
                plugins_ui::PluginsState::load(plugins_ui::PluginsState::default_user_dir());
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
        let Err(e) = super::Args::try_parse_from(["ohl-structure", "--version"]) else {
            panic!("--version parsed as a run");
        };
        assert_eq!(e.kind(), clap::error::ErrorKind::DisplayVersion);
        let text = e.to_string();
        assert_eq!(text.trim(), format!("ohl-structure {}", super::VERSION));
        assert!(super::VERSION.starts_with(env!("CARGO_PKG_VERSION")));
        assert!(super::VERSION.contains("(build "));
    }
}
