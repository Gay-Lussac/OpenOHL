// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl code`: design-code plugins — list them, write a parameter file
//! template, generate weather cases and load cases from a parameter file,
//! list and write a plugin's presets of clearance rules.

use std::path::{Path, PathBuf};

use clap::Subcommand;

/// Where `ohl code list` looks without `--dir`: the repository's code
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
    /// Generate weather cases (.ohlwc) and load cases (.ohllc) from a
    /// parameter file, and the plugin's wind factors for structures when
    /// it gives them (`<plugin>-wind.ohlsec` in the section library);
    /// prints the plugin's notes.
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
        /// Section library folder the wind factors for structures go
        /// into, if the plugin gives them (default: `Sections` beside the
        /// weather case file).
        #[arg(long)]
        sections: Option<PathBuf>,
    },
    /// List a plugin's presets of clearance rules (ground, objects,
    /// crossings, wire to wire: the optional `rule-presets` export), with
    /// each rule and its distances; `--out` writes them as
    /// `<plugin>-rules-<name>.ohlr` (and `.ohlwc` for their weather cases).
    RulePresets {
        /// Design-code plugin directory.
        #[arg(long)]
        plugin: PathBuf,
        /// Parameter file (TOML); none: the plugin gives what it can
        /// without one.
        #[arg(long)]
        params: Option<PathBuf>,
        /// Folder to write the presets into.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

/// Runs an `ohl code` command.
pub fn run(command: CodeCommand) -> Result<(), String> {
    match command {
        CodeCommand::List { dir } => list(&dir),
        CodeCommand::Template { plugin, out } => template(&plugin, out.as_deref()),
        CodeCommand::Generate {
            plugin,
            params,
            weather,
            load_cases,
            sections,
        } => generate(&plugin, &params, &weather, &load_cases, sections.as_deref()),
        CodeCommand::RulePresets {
            plugin,
            params,
            out,
        } => rule_presets(&plugin, params.as_deref(), out.as_deref()),
    }
}

fn list(dirs: &[PathBuf]) -> Result<(), String> {
    let dirs = if dirs.is_empty() {
        vec![PathBuf::from(DEFAULT_DIR)]
    } else {
        dirs.to_vec()
    };
    let found = ohl_io::codes::list(&dirs);
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

fn host() -> Result<ohl_plugins::WasmHost, String> {
    ohl_plugins::WasmHost::new().map_err(|e| e.to_string())
}

fn template(plugin: &Path, out: Option<&Path>) -> Result<(), String> {
    let text = ohl_io::codes::template(&host()?, plugin)?;
    match out {
        Some(p) => {
            std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))?;
            println!("wrote {}", p.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn generate(
    plugin: &Path,
    params: &Path,
    weather: &Path,
    load_cases: &Path,
    sections: Option<&Path>,
) -> Result<(), String> {
    let text = std::fs::read_to_string(params).map_err(|e| format!("{}: {e}", params.display()))?;
    let g = ohl_io::codes::generate(&host()?, plugin, &text)?;
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
    // Rule presets next to the weather cases too.
    for (rules, cases) in g.write_rule_presets(dir, dir)? {
        println!(
            "  rule preset → {}{}",
            rules.display(),
            cases.map_or(String::new(), |c| format!(
                " (weather cases {})",
                c.display()
            ))
        );
    }
    // Wind factors for structures into the section library.
    let sections = sections.map_or_else(|| dir.join("Sections"), Path::to_path_buf);
    match g.write_structure_wind(&sections)? {
        Some(p) => println!("  wind factors for structures → {}", p.display()),
        None => println!("  {}", g.structure_wind_note()),
    }
    for n in &g.notes {
        println!("  note: {n}");
    }
    Ok(())
}

/// The rule presets of `plugin` (from `params` if given), printed, and
/// written into `out` if given.
fn rule_presets(plugin: &Path, params: Option<&Path>, out: Option<&Path>) -> Result<(), String> {
    let text = match params {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        None => String::new(),
    };
    let presets = ohl_io::codes::rule_presets(&host()?, plugin, &text)?;
    let name = ohl_plugins::Manifest::load(plugin)
        .map(|m| m.plugin.name)
        .map_err(|e| e.to_string())?;
    if presets.is_empty() {
        println!("{name}: no rule presets");
    }
    for p in &presets {
        println!("{} — {}", p.name, p.description);
        for r in &p.rules.rules {
            let mut parts = Vec::new();
            if let Some(v) = &r.vertical {
                parts.push(format!("vertical {v}"));
            }
            if let Some(v) = &r.radial {
                parts.push(format!("radial {v}"));
            }
            println!("  {}: {}", r.name, parts.join("; "));
        }
        if let Some(v) = &p.rules.vegetation {
            let mut parts = Vec::new();
            if !v.classes.is_empty() {
                let c: Vec<String> = v.classes.iter().map(u8::to_string).collect();
                parts.push(format!("classes {}", c.join(", ")));
            }
            if let Some(c) = &v.clearance {
                parts.push(format!("clearance {c}"));
            }
            if let Some(a) = &v.fall_allowance {
                parts.push(format!("allowance {a}"));
            }
            if let Some(k) = v.root_ball {
                parts.push(format!("root ball {k}"));
            }
            println!("  vegetation: {}", parts.join("; "));
        }
    }
    if let Some(dir) = out {
        for (rules, cases) in ohl_io::codes::write_rule_presets(&name, &presets, dir, dir)? {
            println!(
                "wrote {}{}",
                rules.display(),
                cases.map_or(String::new(), |c| format!(", {}", c.display()))
            );
        }
    }
    Ok(())
}
