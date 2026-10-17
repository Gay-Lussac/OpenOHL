// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols bom` (bill of materials and cost estimate) and `ols spot`
//! (optimum structure spotting along a route).

use std::path::{Path, PathBuf};
use std::time::Duration;

use ols_core::files::shipped_library;

/// Arguments of `ols bom`.
#[derive(clap::Args)]
pub struct BomArgs {
    /// Line file (.ols.json) or project file (.ols).
    file: PathBuf,
    /// Wire library: .olsw file or directory (default: the shipped
    /// libraries/cables).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Structure type directory (.olss) (default: the `Structures` folder
    /// next to the line).
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Insulator assembly library directory (.olsi) (default: the shipped
    /// libraries/insulators).
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Unit costs (.olscost): price the bill of materials.
    #[arg(long)]
    cost: Option<PathBuf>,
    /// Write the bill (and prices) as CSV.
    #[arg(long)]
    csv: Option<PathBuf>,
    /// Write the bill (and prices) as JSON.
    #[arg(long)]
    json: Option<PathBuf>,
}

/// Arguments of `ols spot`.
#[derive(clap::Args)]
pub struct SpotArgs {
    /// Line file (.ols.json) or project file (.ols) with the route.
    file: PathBuf,
    /// Route id (see ols line --stations); default: the first route.
    #[arg(long)]
    route: Option<String>,
    /// Survey points (CSV / XYZ) for the ground surface.
    #[arg(long)]
    terrain: PathBuf,
    /// Structure type directory (.olss): the candidates are the types (and
    /// extensions) the cost file prices.
    #[arg(long)]
    types: Option<PathBuf>,
    /// Insulator assembly library directory (.olsi).
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Wire library: .olsw file or directory.
    #[arg(long)]
    library: Option<PathBuf>,
    /// Unit costs (.olscost).
    #[arg(long)]
    cost: PathBuf,
    /// Cable id of the wire whose clearance is kept.
    #[arg(long)]
    cable: String,
    /// Horizontal tension of the wire in the design (maximum sag)
    /// condition, kN.
    #[arg(long)]
    tension: f64,
    /// Temperature of that condition, °C (the spotted sections are strung
    /// to the tension at it, in still air).
    #[arg(long, default_value_t = 80.0)]
    temperature: f64,
    /// Smallest vertical ground clearance, m.
    #[arg(long)]
    clearance: f64,
    /// Distance between candidate stations, m.
    #[arg(long, default_value_t = 10.0)]
    step: f64,
    /// Longest span, m (default: twice the largest allowable wind span).
    #[arg(long)]
    max_span: Option<f64>,
    /// Shortest span, m.
    #[arg(long, default_value_t = 0.0)]
    min_span: f64,
    /// No structure between these stations, FROM:TO in m (repeatable).
    #[arg(long)]
    forbid: Vec<String>,
    /// Wires per span priced per metre (default: the fewest attachments
    /// of a candidate type).
    #[arg(long)]
    conductors: Option<u32>,
    /// Stop after this many seconds.
    #[arg(long, default_value_t = 300)]
    seconds: u64,
    /// Write the spotted line (.ols.json).
    #[arg(long)]
    out: Option<PathBuf>,
}

fn structure_dir(file: &Path, given: Option<&PathBuf>) -> PathBuf {
    given
        .cloned()
        .unwrap_or_else(|| file.parent().unwrap_or(Path::new(".")).join("Structures"))
}

fn libraries(
    file: &Path,
    structures: Option<&PathBuf>,
    insulators: Option<&PathBuf>,
) -> Result<
    (
        ols_structures::StructureLibrary,
        ols_structures::InsulatorLibrary,
    ),
    String,
> {
    let types = ols_structures::StructureLibrary::load_dir(&structure_dir(file, structures))
        .map_err(|e| e.to_string())?;
    let ins = insulators
        .cloned()
        .unwrap_or_else(|| shipped_library("insulators"));
    let ins = ols_structures::InsulatorLibrary::load_dir(&ins).map_err(|e| e.to_string())?;
    Ok((types, ins))
}

fn cables(library: Option<&PathBuf>) -> Result<ols_cables::Library, String> {
    let path = library
        .cloned()
        .unwrap_or_else(|| shipped_library("cables"));
    ols_cables::Library::load(&path).map_err(|e| e.to_string())
}

/// `ols bom`.
pub fn bom(args: &BomArgs) -> Result<(), String> {
    let mut line = crate::project::load_line(&args.file)?;
    if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let (types, ins) = libraries(
            &args.file,
            args.structures.as_ref(),
            args.insulators.as_ref(),
        )?;
        line = line.resolve(&types, &ins).map_err(|e| e.to_string())?;
    }
    let library = cables(args.library.as_ref())?;
    let cost = args
        .cost
        .as_deref()
        .map(ols_bom::CostData::load)
        .transpose()?;
    let hardware = cost
        .as_ref()
        .map(|c| c.hardware.clone())
        .unwrap_or_default();
    let bom = ols_bom::bill_of_materials(&line, &library, &hardware);
    let estimate = ols_bom::CostEstimate::new(bom, cost.as_ref());
    print!("{}", estimate.text());
    if let Some(path) = &args.csv {
        std::fs::write(path, estimate.csv()).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    if let Some(path) = &args.json {
        let text = serde_json::to_string_pretty(&estimate).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn range(text: &str) -> Result<[f64; 2], String> {
    let bad = || format!("--forbid {text}: give FROM:TO in metres");
    let (a, b) = text.split_once(':').ok_or_else(bad)?;
    let a: f64 = a.trim().parse().map_err(|_| bad())?;
    let b: f64 = b.trim().parse().map_err(|_| bad())?;
    if b > a { Ok([a, b]) } else { Err(bad()) }
}

/// `ols spot`.
pub fn spot(args: &SpotArgs) -> Result<(), String> {
    let line = crate::project::load_line(&args.file)?;
    let route = match &args.route {
        Some(r) => r.clone(),
        None => line
            .alignment_views()
            .first()
            .map(|r| r.id.clone())
            .ok_or("the line has no route")?,
    };
    let (types, ins) = libraries(&args.file, args.types.as_ref(), args.insulators.as_ref())?;
    let library = cables(args.library.as_ref())?;
    let cost = ols_bom::CostData::load(&args.cost)?;
    let terrain = ols_terrain::Terrain::load(&args.terrain).map_err(|e| e.to_string())?;
    let options = ols_spotting::route::RouteOptions {
        route,
        cable: args.cable.clone(),
        horizontal_tension: args.tension * 1000.0,
        temperature: args.temperature,
        clearance: args.clearance,
        step: args.step,
        max_span: args.max_span,
        min_span: args.min_span,
        forbidden: args
            .forbid
            .iter()
            .map(|f| range(f))
            .collect::<Result<_, _>>()?,
        conductors: args.conductors,
    };
    let rp = ols_spotting::route::route_problem(
        &line, &terrain, &types, &ins, &library, &cost, &options,
    )?;
    println!(
        "route {}: {:.1} m, {} candidates, {} angle point(s)",
        rp.route.id,
        rp.problem.ground.last().map_or(0.0, |g| g[0]),
        rp.problem.candidates.len(),
        rp.problem.required.len()
    );
    let budget = ols_spotting::default_budget().time_limit(Duration::from_secs(args.seconds));
    let result = ols_spotting::spot(&rp.problem, &budget).map_err(|e| e.to_string())?;
    print!(
        "{}",
        ols_spotting::route::report(&rp, &result, &cost.currency)
    );
    if let Some(path) = &args.out {
        let spotted = ols_spotting::route::spotted_line(&line, &rp, &result, &types, &options);
        spotted
            .resolve(&types, &ins)
            .map_err(|e| format!("the spotted line: {e}"))?;
        std::fs::write(path, spotted.to_json()).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "wrote {} ({} structures, {} sections)",
            path.display(),
            spotted.structures.len(),
            spotted.sections.len()
        );
    }
    Ok(())
}
