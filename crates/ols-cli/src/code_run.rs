// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols code`: design-code plugins — list them, write a parameter file
//! template, generate weather cases and load cases from a parameter file.

use std::path::{Path, PathBuf};

use clap::Subcommand;

/// Where `ols code list` looks without `--dir`: the repository's code
/// plugins (when run from the repository).
const DEFAULT_DIR: &str = "plugins/codes";

#[derive(Subcommand)]
pub enum CodeCommand {
    /// List the design-code plugins found (and why one cannot be used).
    List {
        /// Plugin directory or directory of plugins (repeatable; default
        /// plugins/codes).
        #[arg(long)]
        dir: Vec<PathBuf>,
    },
    /// Write a plugin's parameter file template (invented placeholder
    /// values, to replace from the standard).
    Template {
        /// Design-code plugin directory.
        #[arg(long)]
        plugin: PathBuf,
        /// Output file (default: print it).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Generate weather cases (.olswc) and load cases (.olslc) from a
    /// parameter file; prints the plugin's notes.
    Generate {
        /// Design-code plugin directory.
        #[arg(long)]
        plugin: PathBuf,
        /// Parameter file (TOML; see the plugin's README).
        #[arg(long)]
        params: PathBuf,
        /// Weather case file to write.
        #[arg(long)]
        weather: PathBuf,
        /// Load case file to write.
        #[arg(long = "load-cases")]
        load_cases: PathBuf,
    },
}

/// Runs an `ols code` command.
pub fn run(command: CodeCommand) -> Result<(), String> {
    match command {
        CodeCommand::List { dir } => list(&dir),
        CodeCommand::Template { plugin, out } => template(&plugin, out.as_deref()),
        CodeCommand::Generate {
            plugin,
            params,
            weather,
            load_cases,
        } => generate(&plugin, &params, &weather, &load_cases),
    }
}

fn list(dirs: &[PathBuf]) -> Result<(), String> {
    let dirs = if dirs.is_empty() {
        vec![PathBuf::from(DEFAULT_DIR)]
    } else {
        dirs.to_vec()
    };
    let found = ols_io::codes::list(&dirs);
    if found.is_empty() {
        println!(
            "no design-code plugins in {}",
            dirs.iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    for c in found {
        println!("{:<12} {}", c.name, c.description);
        println!("{:<12} {}", "", c.dir.display());
        if let Some(p) = c.problem {
            println!("{:<12} cannot be used: {p}", "");
        }
    }
    Ok(())
}

fn host() -> Result<ols_plugins::WasmHost, String> {
    ols_plugins::WasmHost::new().map_err(|e| e.to_string())
}

fn template(plugin: &Path, out: Option<&Path>) -> Result<(), String> {
    let text = ols_io::codes::template(&host()?, plugin)?;
    match out {
        Some(p) => {
            std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))?;
            println!("wrote {}", p.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn generate(plugin: &Path, params: &Path, weather: &Path, load_cases: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(params).map_err(|e| format!("{}: {e}", params.display()))?;
    let g = ols_io::codes::generate(&host()?, plugin, &text)?;
    g.write(weather, load_cases)?;
    println!(
        "{}: {} weather cases → {}, {} load cases → {}",
        g.plugin,
        g.weather.cases.len(),
        weather.display(),
        g.load_cases.cases.len(),
        load_cases.display()
    );
    // Clearance presets next to the weather cases.
    let dir = weather.parent().unwrap_or(Path::new(""));
    for (rules, cases) in g.write_presets(dir, dir)? {
        println!(
            "  clearance preset → {} (weather cases {})",
            rules.display(),
            cases.display()
        );
    }
    for n in &g.notes {
        println!("  note: {n}");
    }
    Ok(())
}
