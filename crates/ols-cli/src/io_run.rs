// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ohl formats`, `ohl import` and `ohl line --export`.

use std::path::{Path, PathBuf};

use ols_io::{ExportModel, Options, Registry};

/// The built-in formats plus those of the plugins in `plugins`.
pub fn registry(plugins: &[PathBuf]) -> Result<Registry, String> {
    let mut r = Registry::with_builtin();
    if !plugins.is_empty() {
        let host = ols_plugins::WasmHost::new().map_err(|e| e.to_string())?;
        for dir in plugins {
            r.add_plugin(&host, dir)
                .map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }
    Ok(r)
}

/// `KEY=VALUE` options, one per `--option` (a value may hold commas).
pub fn options(specs: &[String]) -> Result<Options, String> {
    specs.iter().map(|s| ols_io::parse_option(s)).collect()
}

/// Lists the formats.
pub fn formats(plugins: &[PathBuf]) -> Result<(), String> {
    let r = registry(plugins)?;
    println!(
        "{:<20} {:<8} {:<8} {:<22} description",
        "id", "", "ext", "source"
    );
    for e in r.exporters() {
        println!(
            "{:<20} {:<8} {:<8} {:<22} {}",
            e.id(),
            "export",
            e.extension(),
            e.origin(),
            e.description()
        );
    }
    for i in r.importers() {
        println!(
            "{:<20} {:<8} {:<8} {:<22} {}",
            i.id(),
            "import",
            i.extension(),
            i.origin(),
            i.description()
        );
    }
    Ok(())
}

/// Writes the exports `FILE` or `FORMAT=FILE` of a solved model.
pub fn export(
    model: &ExportModel,
    specs: &[String],
    options: &Options,
    registry: &mut Registry,
) -> Result<(), String> {
    for spec in specs {
        let (id, path) = match spec.split_once('=') {
            Some((id, path)) => (id.to_owned(), PathBuf::from(path)),
            None => (
                ols_io::format_of(Path::new(spec), None),
                PathBuf::from(spec),
            ),
        };
        let exporter = registry.exporter(&id).ok_or_else(|| {
            format!("--export {spec}: no exporter for \"{id}\" (see ohl formats)")
        })?;
        let started = std::time::Instant::now();
        let bytes = exporter
            .export(model, options)
            .map_err(|e| format!("export {}: {e}", exporter.id()))?;
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "export: {} → {} ({} bytes, {:.1?})",
            exporter.id(),
            path.display(),
            bytes.len(),
            started.elapsed()
        );
    }
    Ok(())
}

/// `ohl import`.
pub fn import(
    file: &Path,
    format: Option<&str>,
    into: Option<&Path>,
    out: &Path,
    options: &Options,
    plugins: &[PathBuf],
) -> Result<(), String> {
    let mut r = registry(plugins)?;
    let id = ols_io::format_of(file, format);
    let importer = r.importer(&id).ok_or_else(|| {
        format!("no importer for \"{id}\" (see ohl formats; choose with --format)")
    })?;
    let data = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let base = into
        .map(|p| ols_line::Line::load(p).map_err(|e| e.to_string()))
        .transpose()?;
    let line = importer
        .import(&data, base.as_ref(), options)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    line.validate().map_err(|e| e.to_string())?;
    std::fs::write(out, line.to_json()).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "import: {} ({}) → {}: {} structures, {} sections",
        file.display(),
        importer.id(),
        out.display(),
        line.structures.len(),
        line.sections.len()
    );
    Ok(())
}
