// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl-server-app`: OHL-Server, the admin app (ROADMAP O17).

use clap::Parser;
use eframe::egui;
use ohl_server_app::app::{APP_NAME, App, STORAGE_ID};

/// OHL-Server: host a network's server, or connect to one, and administer
/// it (its state, operations, map and rights).
#[derive(Parser)]
#[command(version)]
struct Args {
    /// The user who signs in (filled into the forms).
    #[arg(long)]
    user: Option<String>,
}

/// The window icon.
const ICON: &[u8] = include_bytes!("../../../../assets/brand/ohl-server-icon.svg");

fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu=warn,naga=warn,eframe=warn,egui_wgpu=warn"),
    )
    .init();
    let args = Args::parse();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1280.0, 860.0])
            .with_icon(ohl_ui::icons::window_icon(ICON)),
        ..Default::default()
    };
    let result = eframe::run_native(
        STORAGE_ID,
        options,
        Box::new(move |cc| {
            let mut app = App::new();
            app.restore(cc.storage);
            if let Some(u) = &args.user {
                app.host_form.user.clone_from(u);
                app.connect_form.user.clone_from(u);
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
