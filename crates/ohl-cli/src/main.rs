// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl` — the OpenOHL command-line interface (formerly `ols`, still built
//! as an alias that runs this program).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ohl_core::units::UnitSystem;

mod analysis_run;
mod clearance_run;
mod code_run;
mod cost_run;
mod io_run;
mod links_run;
mod project;
mod safety_run;
mod sagtension_job;
mod sheet_run;
mod structure_app_run;
mod structure_fe;
mod surface;
mod tensions_run;
mod thermal_run;
mod versions;
mod zones_run;

#[derive(Parser)]
#[command(
    name = "ohl",
    version,
    about = "OpenOHL — open-source overhead line design (OHL-Design, OHL-Structure, ohl)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print version and build information.
    Info,
    /// Run validation cases and report against their reference results.
    Validate {
        /// Case file or directory of case files.
        #[arg(default_value = "benchmarks")]
        path: PathBuf,
        /// Also write the report as Markdown to this file.
        #[arg(long)]
        report: Option<PathBuf>,
        /// List the calculations available to validation cases and exit.
        #[arg(long)]
        list: bool,
    },
    /// Sag-tension of a tension section from a TOML job file.
    Sagtension {
        /// Job file (see examples/sagtension/).
        job: PathBuf,
        /// Units used in the report.
        #[arg(long, value_enum, default_value = "metric")]
        units: Units,
        /// Write the stringing chart as CSV (SI units) to this file.
        #[arg(long)]
        csv: Option<PathBuf>,
    },
    /// Solve a line model (.ohl.json) in 3D for one weather state.
    Line(Box<LineArgs>),
    /// Bill of materials of a line, priced with a cost file (.ohlcost).
    Bom(Box<cost_run::BomArgs>),
    /// Optimum structure spotting along a route: the least-cost structures
    /// (types, extensions, positions) keeping the ground clearance.
    Spot(Box<cost_run::SpotArgs>),
    /// Plan and profile drawings and profile exports of a solved line:
    /// SVG, DXF, PDF, PNG sheets and the profile as CSV or XML, for a
    /// station range at chosen scales. Takes every option of `ohl line`.
    Drawing(Box<DrawingArgs>),
    /// The sheet templates the drawings may use: built in, the project's,
    /// the library folder's (libraries/sheet-templates) and the plugins'
    /// (name, source, page, fields); with --out, written as .ohlsheet
    /// files, their images embedded.
    SheetTemplates {
        /// A project file (.ohl): its own templates too.
        #[arg(long)]
        project: Option<PathBuf>,
        /// A plugin directory giving templates (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
        /// Write them into this folder.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Structure and insulator libraries; structure analysis (finite
    /// elements, poles); a structure's results from OHL-Structure
    /// (`analyse`, `report`: its headless tool, a program of its own).
    #[command(alias = "structure")]
    Structures {
        #[command(subcommand)]
        command: StructuresCommand,
    },
    /// Vertical or radial clearances to the ground and LiDAR classes in
    /// the wind, by a rule file: the check of the desktop clearance
    /// windows. Exit status 0: no violation, 1: violations, 2: an error.
    Zones(Box<zones_run::ZonesArgs>),
    /// Safety volumes for work near the line: the space within a distance
    /// (by voltage) of the wires in every position of the weather cases
    /// (blowouts, wind steps, swing arc), one closed mesh per wire or
    /// circuit, written as georeferenced DXF (+ .prj), IFC4 and KML/KMZ.
    SafetyVolume(Box<safety_run::SafetyArgs>),
    /// The wire tension check: every span of every wire in each weather
    /// case of the `[[tension_rule]]` tables of a rule file — tension
    /// (maximum at the most tensioned support, or horizontal), allowed
    /// (factor × RTS), usage, pass or fail. Exit status 0: all within,
    /// 1: some above, 2: an error.
    Tensions(Box<tensions_run::TensionsArgs>),
    /// Galloping ellipses and blowout against the right of way.
    Clearance {
        #[command(subcommand)]
        command: analysis_run::ClearanceCommand,
    },
    /// LiDAR point clouds (LAS / LAZ).
    Lidar {
        #[command(subcommand)]
        command: LidarCommand,
    },
    /// Project file (.ohl): import/export, commands, undo, log, users,
    /// revisions, branches, merges, reviews.
    Project {
        #[command(subcommand)]
        command: project::ProjectCommand,
    },
    /// Coordinate systems: describe, transform points or a whole line.
    Crs {
        #[command(subcommand)]
        command: CrsCommand,
    },
    /// Point clouds stored in a project file.
    Pointcloud {
        #[command(subcommand)]
        command: PointcloudCommand,
    },
    /// File formats for import and export (built-in and from plugins).
    Formats {
        /// File-format plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
    },
    /// Read a file into a line file (.ohl.json).
    Import {
        /// File to read.
        file: PathBuf,
        /// Format id (default: from the file extension; see ohl formats).
        #[arg(long)]
        format: Option<String>,
        /// Line to update (e.g. structure positions into an existing line).
        #[arg(long)]
        into: Option<PathBuf>,
        /// Output line file.
        #[arg(long)]
        out: PathBuf,
        /// Format option KEY=VALUE (repeatable).
        #[arg(long = "option")]
        options: Vec<String>,
        /// File-format plugin directory (repeatable).
        #[arg(long)]
        plugin: Vec<PathBuf>,
    },
    /// Terrain surfaces (TINs) kept in a project file.
    Surface {
        #[command(subcommand)]
        command: surface::SurfaceCommand,
    },
    /// Thermal ratings: ampacity, conductor temperature, transient, and
    /// the clearance-limited temperature of a line (IEEE 738, CIGRE TB 601
    /// / TB 207).
    Thermal {
        #[command(subcommand)]
        command: thermal_run::ThermalCommand,
    },
    /// Design codes (plugins): weather and load cases from a parameter
    /// file.
    Code {
        #[command(subcommand)]
        command: code_run::CodeCommand,
    },
    /// Inspect plugins.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    /// Host a project for several users (server mode, pre-alpha): prints the
    /// invitation to give them; runs until stopped (Ctrl+C: every edit is
    /// already in the project file).
    Serve {
        /// Project file (.ohl).
        project: PathBuf,
        /// Address and port to listen on (UDP: QUIC).
        #[arg(long, default_value = "0.0.0.0:4433")]
        bind: std::net::SocketAddr,
        /// Host name or address in the invitation (default: this
        /// computer's address on the local network).
        #[arg(long)]
        advertise: Option<String>,
        /// Access code (default: a random one).
        #[arg(long)]
        code: Option<String>,
        /// Role of users the project does not know: viewer, designer, or
        /// none (refused).
        #[arg(long, default_value = "viewer")]
        newcomers: String,
        /// Stop after this many seconds (default: run until stopped).
        #[arg(long)]
        seconds: Option<u64>,
        /// Print the personal invitation of this user of the project
        /// (repeatable): users the project knows join only with theirs.
        #[arg(long)]
        invite: Vec<String>,
        /// Also serve the web client on this TCP port (same address as
        /// --bind): browsers join with a link. HTTPS with the server's
        /// self-signed certificate: browsers warn once; compare the
        /// fingerprint printed here before accepting it. The access code
        /// is in the link: hand it out like an invitation.
        #[arg(long, value_name = "PORT")]
        web: Option<u16>,
    },
}

/// Arguments of `ohl line`.
#[derive(clap::Args, Clone)]
struct LineArgs {
    /// Line file (.ohl.json) or project file (.ohl).
    file: PathBuf,
    /// Wire library: .ohlw file or directory (default: a project's own,
    /// else the shipped libraries/cables, in the current folder or next to
    /// the program).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Structure type directory (.ohls structure files), for typed structures
    /// (default: a project's own, else the `Structures` folder next to the
    /// line or project file).
    #[arg(long)]
    structures: Option<PathBuf>,
    /// Cross-section library (.ohlsec file or folder) the structure models'
    /// members refer to (default: a project's own, else the `Sections`
    /// folder next to the line or project file, if there is one).
    #[arg(long)]
    sections: Option<PathBuf>,
    /// Insulator assembly library directory (.ohli) (default: a project's
    /// own, else the shipped libraries/insulators).
    #[arg(long)]
    insulators: Option<PathBuf>,
    /// Wire temperature, °C.
    #[arg(long, default_value_t = 15.0)]
    temperature: f64,
    /// Wind pressure on the wires, Pa (perpendicular to each span).
    #[arg(long, default_value_t = 0.0)]
    wind_pressure: f64,
    /// Side the wind comes from, looking ahead along the line: left or
    /// right.
    #[arg(long, default_value = "right")]
    wind_from: String,
    /// Radial ice thickness, mm.
    #[arg(long, default_value_t = 0.0)]
    ice_thickness: f64,
    /// Or: ice load, N/m (instead of a thickness).
    #[arg(long)]
    ice_load: Option<f64>,
    /// Per what metre of the wire the finite elements carry the ice and
    /// wind: `stretched` (the real wire, exact; the default) or
    /// `unstretched` (the former treatment, as other programs appear to do
    /// it: about T/EA, 0.1 %, less ice and wind — a simplification on the
    /// unsafe side). Default: the line file's (`solver.loads_per`).
    #[arg(long, value_parser = parse_loads_per)]
    loads_per: Option<ohl_line::LoadsPer>,
    /// Ice density, kg/m³ (default 913, glaze ice): with a thickness, the
    /// ice weight; with a load, the iced diameter for the wind.
    #[arg(long)]
    ice_density: Option<f64>,
    /// Weather case file (.ohlwc; for a project, also the name of one of
    /// its own): solve and check its cases (all, or those chosen with
    /// --case / --tag) instead of the single case of --temperature,
    /// --wind-pressure and the ice options.
    #[arg(long)]
    weather: Option<PathBuf>,
    /// Weather case to run, by name (`*` and `?` wildcards; repeatable).
    #[arg(long)]
    case: Vec<String>,
    /// Weather cases to run, by tag (repeatable).
    #[arg(long)]
    tag: Vec<String>,
    /// Write a standalone 3D viewer (HTML).
    #[arg(long)]
    html: Option<PathBuf>,
    /// Write all wire points as CSV.
    #[arg(long)]
    csv: Option<PathBuf>,
    /// Write one row per span (tensions, sag, mid and low points) as CSV.
    #[arg(long)]
    spans_csv: Option<PathBuf>,
    /// Load cases (.ohllc; for a project, also the name of one of its
    /// own): the loads on the structures in each (their weather cases from
    /// --weather).
    #[arg(long, requires = "weather")]
    load_cases: Option<PathBuf>,
    /// Write the structure loads (every attachment, every load case) as CSV.
    #[arg(long, requires = "load_cases")]
    loads_csv: Option<PathBuf>,
    /// Write how much of its capacity each structure uses (its type's
    /// allowable spans and loads, structure model and pole; every load
    /// case) as CSV.
    #[arg(long, requires = "load_cases")]
    usage_csv: Option<PathBuf>,
    /// Print stationing, offsets and line angles of every route.
    #[arg(long)]
    stations: bool,
    /// Survey points (CSV / XYZ) for the terrain surface: enables the
    /// ground clearance check and draws the ground in the viewer.
    #[arg(long)]
    terrain: Option<PathBuf>,
    /// LiDAR tiles (LAS/LAZ file or directory): terrain surface built from
    /// the classes with the ground role, inside a corridor along the spans.
    #[arg(long, conflicts_with = "terrain")]
    lidar: Option<PathBuf>,
    /// Point cloud stored in a project file (`FILE.ohl`, or `FILE.ohl#name`
    /// when it holds several), or a COPC file (`FILE.copc.laz`): only the
    /// blocks or octree nodes of the corridor are read, much faster than
    /// reading tiles.
    #[arg(long, conflicts_with_all = ["terrain", "lidar"])]
    pointcloud: Option<String>,
    /// Class scheme (.ohlc) of the LiDAR; default ASPRS LAS 1.4.
    #[arg(long)]
    lidar_classes: Option<PathBuf>,
    /// Role of the classes that make the terrain surface.
    #[arg(long, default_value = "ground")]
    ground_role: String,
    /// Half width of the LiDAR corridor around the spans, m.
    #[arg(long, default_value_t = 60.0)]
    corridor: f64,
    /// Longest triangle side of the LiDAR terrain surface, m (no ground is
    /// invented across larger gaps).
    #[arg(long, default_value_t = 50.0)]
    max_edge: f64,
    /// Thin the LiDAR ground points: lowest point per square cell of this
    /// size, m.
    #[arg(long)]
    thin: Option<f64>,
    /// Required vertical ground clearance, m (a ground rule; adds to
    /// --rules).
    #[arg(long)]
    clearance: Option<f64>,
    /// Clearance rules (.ohlr; for a project, also the name of one of its
    /// own): vertical and radial clearances to the ground, LiDAR classes,
    /// other lines and wires of this line.
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Another line (.ohl.json) for wire-to-wire clearances (repeatable);
    /// solved in the same weather with the same libraries.
    #[arg(long)]
    other_line: Vec<PathBuf>,
    /// Write every clearance found (per rule, wire and measure) as CSV.
    #[arg(long)]
    clearance_csv: Option<PathBuf>,
    /// Write a plan and profile sheet (SVG) and profile CSV per route into
    /// this directory.
    #[arg(long)]
    drawings: Option<PathBuf>,
    /// Horizontal scale of the drawings: 1:2000 or 2000.
    #[arg(long, default_value = "2000", value_parser = scale)]
    hscale: f64,
    /// Vertical scale of the profile: 1:200 or 200.
    #[arg(long, default_value = "200", value_parser = scale)]
    vscale: f64,
    /// Side ground profile at this offset, m (right positive; repeatable).
    #[arg(long, allow_hyphen_values = true)]
    side_offset: Vec<f64>,
    /// Line width of the drawings, m: the half width of the plan; what is
    /// drawn along the line (a structure beside the line only within it,
    /// wires cut at it). Also --line-width.
    #[arg(long, alias = "line-width", default_value_t = 40.0)]
    plan_width: f64,
    /// Scale of the plan across the line, 1:1000 or 1000 (default: the
    /// horizontal scale).
    #[arg(long, value_parser = scale)]
    plan_scale: Option<f64>,
    /// Split the drawings over sheets of this format, at structures: a
    /// standard paper size (A0–A4, A3x3 …, B1–B4, "ANSI A"–"ANSI E",
    /// "ARCH A"–"ARCH E"; landscape), WIDTHxHEIGHT in mm, or `optimised`:
    /// the size and orientation with the fewest warnings and the least
    /// paper, from --paper-sizes.
    #[arg(long)]
    page: Option<String>,
    /// With --page optimised: the sizes it may choose from, comma-separated
    /// (default A0,A1,A2,A3,A4).
    #[arg(long, value_delimiter = ',')]
    paper_sizes: Vec<String>,
    /// With --page optimised: different sizes of one standard on a route's
    /// sheets (a short sheet on a smaller size).
    #[arg(long)]
    mixed_sizes: bool,
    /// With --page optimised: the same paper (or, with --mixed-sizes, the
    /// same standard and orientation) on every route.
    #[arg(long)]
    same_paper: bool,
    /// Clearance on paper, mm, between a sheet break and what is drawn of
    /// every other structure (0 to 20).
    #[arg(long, default_value_t = ohl_drawing::BREAK_CLEARANCE_MM, value_parser = break_clearance)]
    break_clearance: f64,
    /// Logo for the sheets' title block: FILE (SVG, PNG, JPEG) or
    /// FILE:WIDTH_MM (repeatable).
    #[arg(long)]
    logo: Vec<String>,
    /// Sheet template of the sheets: `default`, a .ohlsheet file, a
    /// template of the project or of the library folder by name, or
    /// PLUGIN/PRESET (a plugin of --plugin); its page unless --page.
    #[arg(long)]
    sheet_template: Option<String>,
    /// A value of the template's fields: KEY=VALUE, or KEY@SHEET=VALUE for
    /// one sheet (repeatable).
    #[arg(long = "field")]
    field: Vec<String>,
    /// The template's fields from a TOML file (KEY = "value",
    /// [per_sheet.KEY] SHEET = "value", [[revision]] index, date, text,
    /// by, checked).
    #[arg(long)]
    fields: Option<PathBuf>,
    /// A row of the revision table: "INDEX;DATE;TEXT;BY;CHECKED"
    /// (repeatable, oldest first).
    #[arg(long)]
    revision: Vec<String>,
    /// Orientation of the sheets: landscape or portrait (with --page
    /// optimised: the only one allowed; default both).
    #[arg(long, value_parser = ["landscape", "portrait"])]
    orientation: Option<String>,
    /// Corridor on the sheets: its edges at ± this many metres from the
    /// axis in the plan and the key map (default: the template's).
    #[arg(long)]
    corridor_width: Option<f64>,
    /// LiDAR classes drawn on the sheets as dots, by code (e.g.
    /// 2,3,5,14; default: the template's `[lidar] classes`; where: the
    /// template's `[lidar] profile` / `plan`): the points of --lidar or
    /// --pointcloud within the corridor (else the line width).
    #[arg(long, value_delimiter = ',')]
    sheet_lidar_classes: Option<Vec<u8>>,
    /// Another version of the line drawn with it: LINE=ROLE or
    /// LINE=ROLE:LABEL (repeatable) — LINE a project line (of a project
    /// file) or a line file, ROLE a role of the template's [roles]
    /// (existing, project, removal, decommissioned, other …), LABEL with
    /// {name} and {voltage}.
    #[arg(long = "version", value_name = "LINE=ROLE")]
    versions: Vec<String>,
    /// The role of the line itself on the sheets (e.g. project).
    #[arg(long)]
    role: Option<String>,
    /// Lines below the lowest conductor of each circuit, lowered by its
    /// ground clearance from the rules (--rules, --clearance).
    #[arg(long)]
    conductor_clearance: bool,
    /// Leave out the template's band rows.
    #[arg(long)]
    no_bands: bool,
    /// The project's drawings (DXF) under the sheets' plan, straightened
    /// along the line (layer OHL_BACKGROUND_DRAWINGS): alone, those the
    /// template names (else every one); or NAME,NAME… (default: the
    /// template's `[plan_background] drawings`).
    #[arg(long, value_delimiter = ',', num_args = 0.., value_name = "NAMES")]
    plan_drawings: Option<Vec<String>>,
    /// The project's orthophotos under the sheets' plan, resampled along
    /// the line (layer OHL_BACKGROUND_PHOTO; SVG, PDF, PNG): alone, the
    /// image layers the template names (else every one); or
    /// LAYER,LAYER… (default: the template's `[plan_background]
    /// orthophoto`).
    #[arg(long, value_delimiter = ',', num_args = 0.., value_name = "LAYERS")]
    plan_orthophoto: Option<Vec<String>>,
    /// Neither drawings nor orthophotos under the plan, whatever the
    /// template says.
    #[arg(long, conflicts_with_all = ["plan_drawings", "plan_orthophoto"])]
    no_plan_backgrounds: bool,
    /// Resolution of the orthophoto under the plan, dots per inch
    /// (default: the template's `[plan_background] dpi`, 200).
    #[arg(long)]
    plan_background_dpi: Option<f64>,
    /// The project's drawings (DXF) under the sheets' key map, north up
    /// (default: the template's `[key_map] drawings`).
    #[arg(long)]
    key_map_drawings: bool,
    /// The project's orthophotos under the sheets' key map, north up
    /// (default: the template's `[key_map] orthophoto`).
    #[arg(long)]
    key_map_orthophoto: bool,
    /// The project file (.ohl) whose drawings and orthophotos go under the
    /// plan (default: the line's, when it is a project file).
    #[arg(long)]
    backgrounds: Option<PathBuf>,
    /// Export the solved line (every weather case): FILE (format from the
    /// extension) or FORMAT=FILE, e.g. kmz=line.kmz (repeatable; see ohl
    /// formats).
    #[arg(long)]
    export: Vec<String>,
    /// Export format option KEY=VALUE (repeatable), e.g. crs=EPSG:2056.
    #[arg(long = "option")]
    export_options: Vec<String>,
    /// File-format plugin directory (repeatable).
    #[arg(long)]
    plugin: Vec<PathBuf>,
}

/// Arguments of `ohl drawing`.
#[derive(clap::Args)]
struct DrawingArgs {
    #[command(flatten)]
    line: LineArgs,
    /// Route to draw, by id (repeatable; default: every route).
    #[arg(long)]
    route: Vec<String>,
    /// Formats: svg, dxf, pdf, png, csv, xml (comma-separated or
    /// repeated).
    #[arg(long, value_delimiter = ',', default_value = "svg")]
    format: Vec<String>,
    /// First station drawn: metres or km+m, e.g. 0+250 (default: the
    /// route's start).
    #[arg(long, value_parser = station, allow_hyphen_values = true)]
    from: Option<f64>,
    /// Last station drawn (default: the route's end).
    #[arg(long, value_parser = station, allow_hyphen_values = true)]
    to: Option<f64>,
    /// The profile alone, without the plan.
    #[arg(long)]
    profile_only: bool,
    /// Resolution of PNG images, dots per inch.
    #[arg(long, default_value_t = 150.0)]
    dpi: f64,
    /// Structures' elevations on the plan: off, above or below the line
    /// (each structure seen along the line, a leader from where it stands;
    /// layer OHL_ELEVATIONS); default: the sheet template's, else off.
    #[arg(long, value_parser = elevation_place)]
    elevations: Option<ohl_drawing::ElevationPlace>,
    /// From the line to the elevations' base, mm on paper (default 10).
    #[arg(long)]
    elevation_offset: Option<f64>,
    /// Angle between the line and the elevations' height, degrees (90,
    /// the default: standing perpendicular to the line).
    #[arg(long, allow_hyphen_values = true)]
    elevation_rotation: Option<f64>,
    /// Scale of the elevations, 1:1000 or 1000 (default: the horizontal
    /// scale).
    #[arg(long, value_parser = scale)]
    elevation_scale: Option<f64>,
    /// What is written at each elevation: number, type, height,
    /// string_length (vertical strings), dimensions (base to the first
    /// arm, between the arms, to the top), arm_heights (comma-separated).
    #[arg(long, value_delimiter = ',')]
    elevation_labels: Vec<String>,
    /// The elevations as stick figures even where the members or a pole's
    /// outline exist (levels 2 and 3).
    #[arg(long)]
    elevation_stick: bool,
    /// Isoclearance lines on the plan every this many metres (layer
    /// OHL_ISOCLEARANCE): the ground's clearance to the wires of the cases
    /// drawn (the worst of them; a windy case blown left and right), over
    /// the line width. Needs the ground.
    #[arg(long)]
    isoclearance_interval: Option<f64>,
    /// Isoclearance lines above this clearance are not drawn, m.
    #[arg(long, default_value_t = 40.0)]
    isoclearance_max: f64,
    /// Grid spacing of the isoclearance lines, m.
    #[arg(long, default_value_t = 1.0)]
    isoclearance_spacing: f64,
    /// The isoclearance lines also along each span's swing arc between its
    /// left and right blowout.
    #[arg(long)]
    isoclearance_swing_arc: bool,
    /// Output directory (created).
    #[arg(long)]
    out: PathBuf,
}

/// Where the elevations go: off, above or below.
fn elevation_place(text: &str) -> Result<ohl_drawing::ElevationPlace, String> {
    ohl_drawing::ElevationPlace::parse(text)
        .ok_or_else(|| format!("{text}: write off, above or below"))
}

/// A scale denominator from `1:2000` or `2000`.
fn scale(text: &str) -> Result<f64, String> {
    ohl_drawing::parse_scale(text).ok_or_else(|| format!("{text}: write 1:2000 or 2000"))
}

/// A sheet break clearance on paper, mm (0 to the maximum).
fn break_clearance(text: &str) -> Result<f64, String> {
    let mm: f64 = text
        .trim()
        .trim_end_matches("mm")
        .trim()
        .parse()
        .map_err(|_| format!("{text}: write a clearance in mm, e.g. 2"))?;
    ohl_drawing::check_break_clearance(mm)
}

/// A station from metres or `km+m`.
fn station(text: &str) -> Result<f64, String> {
    ohl_drawing::parse_station(text).ok_or_else(|| format!("{text}: write 1250 or 1+250"))
}

/// What drawings to write, where.
struct DrawingSpec {
    /// Routes by id (empty: every route).
    routes: Vec<String>,
    /// Formats, sheets, resolution.
    export: ohl_drawing::export::Export,
    /// Station range (ends default to the route's).
    from: Option<f64>,
    to: Option<f64>,
    /// The profile alone.
    profile_only: bool,
    /// The structures' elevations on the plan, as given (none: the
    /// template's, else the defaults).
    elevations: ElevationChoice,
    /// Isoclearance lines on the plan, with the swing arcs or not.
    isoclearance: Option<(ohl_clearance::isoclearance::IsoParams, bool)>,
    /// Output directory.
    out: PathBuf,
}

/// The elevations as the command line gives them (none: not given).
#[derive(Clone, Default)]
struct ElevationChoice {
    place: Option<ohl_drawing::ElevationPlace>,
    offset: Option<f64>,
    rotation: Option<f64>,
    scale: Option<f64>,
    labels: Vec<String>,
    stick: bool,
}

impl ElevationChoice {
    /// `e` with what is given.
    fn over(&self, mut e: ohl_drawing::Elevations) -> Result<ohl_drawing::Elevations, String> {
        if let Some(p) = self.place {
            e.place = p;
        }
        if let Some(o) = self.offset {
            e.offset_mm = o;
        }
        if let Some(r) = self.rotation {
            e.rotation_deg = r;
        }
        if self.scale.is_some() {
            e.scale = self.scale;
        }
        if !self.labels.is_empty() {
            e.labels = ohl_drawing::ElevationLabels::parse(&self.labels)?;
        }
        if self.stick {
            e.stick = true;
        }
        Ok(e)
    }
}

/// `ohl drawing`: solves the line like `ohl line`, then writes the
/// drawings asked for.
fn drawing_command(args: &DrawingArgs) -> ExitCode {
    let formats: Result<Vec<_>, String> = args
        .format
        .iter()
        .map(|f| {
            ohl_drawing::export::Format::parse(f)
                .ok_or_else(|| format!("unknown format \"{f}\" (svg, dxf, pdf, png, csv or xml)"))
        })
        .collect();
    let formats = match formats {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let spec = DrawingSpec {
        routes: args.route.clone(),
        export: ohl_drawing::export::Export {
            formats,
            page: None, // from --page, read with the other drawing options
            optimise: None,
            dpi: args.dpi,
        },
        from: args.from,
        to: args.to,
        profile_only: args.profile_only,
        elevations: ElevationChoice {
            place: args.elevations,
            offset: args.elevation_offset,
            rotation: args.elevation_rotation,
            scale: args.elevation_scale,
            labels: args.elevation_labels.clone(),
            stick: args.elevation_stick,
        },
        isoclearance: args.isoclearance_interval.map(|interval| {
            (
                ohl_clearance::isoclearance::IsoParams {
                    interval,
                    max_level: args.isoclearance_max,
                    spacing: args.isoclearance_spacing,
                    ..Default::default()
                },
                args.isoclearance_swing_arc,
            )
        }),
        out: args.out.clone(),
    };
    line(&args.line, Some(&spec))
}

#[derive(Clone, Copy, ValueEnum)]
enum Units {
    Metric,
    Us,
}

#[derive(Subcommand)]
enum StructuresCommand {
    /// Derive draft structure types (.ohls) and insulator assemblies (.ohli)
    /// from a line's attachment geometry, plus a copy of the line that
    /// references them; reports how closely the types reproduce the line.
    Derive {
        /// Line file (.ohl.json).
        file: PathBuf,
        /// Output directory (created): structures/, insulators/, typed line;
        /// with --inline-strings: the project folder, structure files go to its
        /// Structures/ folder.
        #[arg(long)]
        out: PathBuf,
        /// Write simple structure files: the strings in each structure type
        /// (length, weight, top and bottom labels), no insulator library.
        #[arg(long)]
        inline_strings: bool,
    },
    /// Finite-element analysis of a structure model (a full-design .ohls,
    /// or an .ohlfe): member
    /// forces and usage under the loads of one structure (a CSV of `ohl
    /// line --loads-csv`) or the model's own loads.
    Fe(structure_fe::FeArgs),
    /// A structure's attachment points: the joints they are linked to, the
    /// offsets, which are unlinked; with --to, how they match another
    /// type's joints (exact, or the nearest attach joint offered).
    Points(links_run::PointsArgs),
    /// Link an attachment point of a typed structure to a joint of its
    /// type (at an offset), relink it, or unlink it, in a line file (a
    /// project: the command link_point / unlink_point, `ohl project
    /// apply`).
    Link(links_run::LinkArgs),
    /// Pole check (.ohlp, or the type's own [pole]): bending moment along
    /// the pole from the loads at a structure type's attachments, wind,
    /// P-Delta.
    Pole(structure_fe::PoleArgs),
    /// A structure's results as OHL-Structure gives them (its load cases
    /// solved on an extension, the pole check), from its headless tool
    /// `ohl-structure-report` (next to ohl, on the PATH or named by
    /// OHL_STRUCTURE_REPORT), asked in JSON.
    #[command(alias = "analyze")]
    Analyse(structure_app_run::StructureRunArgs),
    /// The same, and OHL-Structure's structure report (HTML) with the
    /// members' results (CSV) beside it.
    Report(structure_app_run::StructureReportArgs),
}

#[derive(Subcommand)]
enum LidarCommand {
    /// Tiles, point counts and bounds; with --classes, point counts per
    /// class (reads every point).
    Info {
        /// LAS / LAZ file or directory of tiles.
        path: PathBuf,
        /// Count points per class.
        #[arg(long)]
        classes: bool,
        /// Class scheme (.ohlc) naming the classes; default ASPRS LAS 1.4.
        #[arg(long)]
        scheme: Option<PathBuf>,
    },
    /// Classify unclassified points: ground (progressive morphological
    /// filter), then vegetation classes by height above the ground.
    Classify(analysis_run::ClassifyArgs),
    /// Vegetation near the wires of each weather case: encroaching now and
    /// after growth, and trees that could fall onto them.
    Vegetation(Box<analysis_run::VegetationArgs>),
    /// Catenaries fitted to the points of wire classes, span by span,
    /// compared with the model; the conductor temperature at the survey.
    FitWires(Box<analysis_run::FitArgs>),
}

#[derive(Subcommand)]
enum CrsCommand {
    /// What a coordinate system is (EPSG:<code> or a proj4 string).
    Info {
        /// Coordinate system.
        spec: String,
    },
    /// Transform points: X Y [Z] … (geographic: longitude latitude, degrees).
    Transform {
        /// From (e.g. EPSG:2056).
        #[arg(long)]
        from: String,
        /// To (e.g. EPSG:4326).
        #[arg(long)]
        to: String,
        /// Heights are ellipsoidal and go through the datum shift (default:
        /// kept as they are).
        #[arg(long)]
        ellipsoidal: bool,
        /// Coordinates: X Y (Z defaults to 0), repeated for more points
        /// with --points 3.
        #[arg(allow_hyphen_values = true, num_args = 2..)]
        values: Vec<f64>,
        /// Values per point: 2 (X Y) or 3 (X Y Z).
        #[arg(long, default_value_t = 2)]
        points: usize,
    },
    /// Move a whole line into another projected coordinate system.
    Line {
        /// Line file (.ohl.json).
        file: PathBuf,
        /// Its coordinate system (default: the line's `crs`).
        #[arg(long)]
        from: Option<String>,
        /// Target coordinate system (projected, metres).
        #[arg(long)]
        to: String,
        /// Output line file.
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum PointcloudCommand {
    /// Import LAS/LAZ tiles or COPC files into a project file (created if
    /// missing), with the levels of detail the views stream by.
    Import {
        /// LAS / LAZ / COPC file or directory of tiles.
        tiles: PathBuf,
        /// Project file (.ohl).
        #[arg(long)]
        into: PathBuf,
        /// Name of the point cloud in the project.
        #[arg(long, default_value = "lidar")]
        name: String,
        /// Class scheme (.ohlc) stored with the cloud.
        #[arg(long)]
        classes: Option<PathBuf>,
        /// Coordinate system (informative), e.g. EPSG:2056.
        #[arg(long, default_value = "")]
        crs: String,
        /// Chunk size, m.
        #[arg(long, default_value_t = ohl_pointcloud::DEFAULT_CHUNK)]
        chunk: f64,
    },
    /// Write a point cloud of a project file to LAS / LAZ / COPC (by
    /// extension; `.copc.laz`: COPC): LAS 1.4, point format 6, with its
    /// class scheme next to it.
    Export {
        /// Project file (.ohl).
        file: PathBuf,
        /// LAS / LAZ / COPC (`.copc.laz`) file to write.
        #[arg(long)]
        out: PathBuf,
        /// Point cloud (default: the only one).
        #[arg(long)]
        name: Option<String>,
        /// Only these classes, comma separated (e.g. 2,5,14).
        #[arg(long, value_delimiter = ',')]
        classes: Vec<u8>,
        /// Only this rectangle: xmin,ymin,xmax,ymax.
        #[arg(long, value_delimiter = ',', num_args = 4)]
        area: Vec<f64>,
    },
    /// Point clouds of a project file and their points per class; or a
    /// COPC file's octree (nodes and points per level).
    Info {
        /// Project file (.ohl) or COPC file.
        file: PathBuf,
    },
    /// Build the levels of detail of a project's point clouds where they
    /// are missing (files from before they were stored: the views then
    /// read less to draw far away).
    Octree {
        /// Project file (.ohl).
        file: PathBuf,
        /// Point cloud (default: all).
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Load a WebAssembly plugin of any world (design code, check, file
    /// format, terrain, project or the minimal one, from its manifest's
    /// extension points) from its directory and print what it reports.
    Info {
        /// Directory containing `ohl-plugin.toml`.
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Info) | None => {
            print_info();
            ExitCode::SUCCESS
        }
        Some(Command::Validate { path, report, list }) => validate(&path, report.as_deref(), list),
        Some(Command::Sagtension { job, units, csv }) => sagtension(&job, units, csv.as_deref()),
        Some(Command::Line(args)) => line(&args, None),
        Some(Command::Zones(args)) => zones_run::command(&args),
        Some(Command::Tensions(args)) => match tensions_run::run(&args) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::from(1),
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        },
        Some(Command::SafetyVolume(args)) => report(safety_run::run(*args)),
        Some(Command::Bom(args)) => report(cost_run::bom(&args)),
        Some(Command::Spot(args)) => report(cost_run::spot(&args)),
        Some(Command::Drawing(args)) => drawing_command(&args),
        Some(Command::SheetTemplates {
            project,
            plugin,
            out,
        }) => report(sheet_run::list(project.as_deref(), &plugin, out.as_deref())),
        Some(Command::Structures {
            command:
                StructuresCommand::Derive {
                    file,
                    out,
                    inline_strings,
                },
        }) => structures_derive(&file, &out, inline_strings),
        Some(Command::Structures {
            command: StructuresCommand::Fe(args),
        }) => report(structure_fe::fe(&args)),
        Some(Command::Structures {
            command: StructuresCommand::Pole(args),
        }) => report(structure_fe::pole(&args)),
        Some(Command::Structures {
            command: StructuresCommand::Points(args),
        }) => report(links_run::points(&args)),
        Some(Command::Structures {
            command: StructuresCommand::Link(args),
        }) => report(links_run::link(&args)),
        Some(Command::Structures {
            command: StructuresCommand::Analyse(args),
        }) => structure_app_run::analyse(&args),
        Some(Command::Structures {
            command: StructuresCommand::Report(args),
        }) => structure_app_run::report(&args),
        Some(Command::Plugin {
            command: PluginCommand::Info { dir },
        }) => plugin_info(&dir),
        Some(Command::Formats { plugin }) => report(io_run::formats(&plugin)),
        Some(Command::Code { command }) => report(code_run::run(command)),
        Some(Command::Surface { command }) => report(surface::run(command)),
        Some(Command::Thermal { command }) => report(thermal_run::run(command)),
        Some(Command::Serve {
            project,
            bind,
            advertise,
            code,
            newcomers,
            seconds,
            invite,
            web,
        }) => report(serve(
            &project, bind, advertise, code, &newcomers, seconds, &invite, web,
        )),
        Some(Command::Import {
            file,
            format,
            into,
            out,
            options,
            plugin,
        }) => report(io_run::options(&options).and_then(|o| {
            io_run::import(&file, format.as_deref(), into.as_deref(), &out, &o, &plugin)
        })),
        Some(Command::Pointcloud { command }) => pointcloud(command),
        Some(Command::Crs { command }) => match crs(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Project { command }) => match project::run(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Lidar {
            command:
                LidarCommand::Info {
                    path,
                    classes,
                    scheme,
                },
        }) => lidar_info(&path, classes, scheme.as_deref()),
        Some(Command::Lidar {
            command: LidarCommand::Classify(args),
        }) => report(analysis_run::classify(args)),
        Some(Command::Lidar {
            command: LidarCommand::Vegetation(args),
        }) => report(analysis_run::vegetation(*args)),
        Some(Command::Lidar {
            command: LidarCommand::FitWires(args),
        }) => report(analysis_run::fit_wires(*args)),
        Some(Command::Clearance { command }) => report(analysis_run::clearance(command)),
    }
}

/// `ohl serve`: hosts the project, prints who joins and leaves.
#[allow(clippy::too_many_arguments)]
fn serve(
    project: &std::path::Path,
    bind: std::net::SocketAddr,
    advertise: Option<String>,
    code: Option<String>,
    newcomers: &str,
    seconds: Option<u64>,
    invite: &[String],
    web: Option<u16>,
) -> Result<(), String> {
    let newcomers = match newcomers {
        "none" => None,
        r => Some(ohl_session::Role::parse(r).map_err(|e| e.to_string())?),
    };
    let server = ohl_server::server::Server::start(
        project,
        ohl_server::server::Options {
            bind,
            advertise,
            code,
            newcomers,
            web: web.map(|port| std::net::SocketAddr::new(bind.ip(), port)),
        },
    )?;
    println!("hosting {} on {}", project.display(), server.local_addr);
    println!("invitation (newcomers): {}", server.invitation);
    for user in invite {
        println!("invitation for {user}: {}", server.invitation_for(user));
    }
    if let Some(addr) = server.web_addr {
        let fp: Vec<String> = server
            .invitation
            .fingerprint
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect();
        println!(
            "web client on {addr} (HTTPS, self-signed certificate, SHA-256 {})",
            fp.join(":")
        );
        if let Some(link) = server.web_link(None) {
            println!("web link (newcomers): {link}");
        }
        for user in invite {
            if let Some(link) = server.web_link(Some(user)) {
                println!("web link for {user}: {link}");
            }
        }
    }
    let started = std::time::Instant::now();
    let mut shown: Vec<String> = Vec::new();
    loop {
        if seconds.is_some_and(|s| started.elapsed().as_secs() >= s) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        let now: Vec<String> = server
            .peers()
            .iter()
            .map(|p| format!("{} ({})", p.user, p.role.as_str()))
            .collect();
        if now != shown {
            println!(
                "connected: {}",
                if now.is_empty() {
                    "nobody".into()
                } else {
                    now.join(", ")
                }
            );
            shown = now;
        }
    }
    server.stop();
    Ok(())
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn class_scheme(path: Option<&std::path::Path>) -> Result<ohl_lidar::ClassScheme, String> {
    match path {
        Some(p) => ohl_lidar::ClassScheme::load(p),
        None => Ok(ohl_lidar::ClassScheme::asprs()),
    }
}

fn lidar_info(path: &std::path::Path, classes: bool, scheme: Option<&std::path::Path>) -> ExitCode {
    let run = || -> Result<(), String> {
        let scheme = class_scheme(scheme)?;
        let tiles = ohl_lidar::tiles_in(path).map_err(|e| e.to_string())?;
        let (mut total, mut lo, mut hi) = (0u64, [f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for tile in &tiles {
            let info = ohl_lidar::tile_info(tile).map_err(|e| e.to_string())?;
            total += info.points;
            for k in 0..3 {
                lo[k] = lo[k].min(info.bounds[0][k]);
                hi[k] = hi[k].max(info.bounds[1][k]);
            }
        }
        println!("{} tiles, {total} points", tiles.len());
        println!(
            "bounds x {:.2} – {:.2}, y {:.2} – {:.2}, z {:.2} – {:.2}",
            lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        );
        if classes {
            let started = std::time::Instant::now();
            let filter = ohl_lidar::GroundFilter {
                classes: vec![],
                ..Default::default()
            };
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(1800));
            let (_, stats) =
                ohl_lidar::read_ground(&tiles, &filter, &budget).map_err(|e| e.to_string())?;
            println!(
                "read {} points in {:.1?}",
                stats.points_read,
                started.elapsed()
            );
            println!("classes ({}):", scheme.name);
            for (class, n) in stats.classes.iter().enumerate().filter(|(_, n)| **n > 0) {
                let (name, roles) = scheme
                    .class(class as u8)
                    .map_or(("(not in the scheme)".to_owned(), String::new()), |c| {
                        (c.name.clone(), c.roles.join(", "))
                    });
                println!("  {class:>3} {n:>12}  {name:<48} {roles}");
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn pointcloud(command: PointcloudCommand) -> ExitCode {
    let run = || -> Result<(), String> {
        match command {
            PointcloudCommand::Import {
                tiles,
                into,
                name,
                classes,
                crs,
                chunk,
            } => {
                let scheme = match &classes {
                    Some(p) => {
                        let text = std::fs::read_to_string(p)
                            .map_err(|e| format!("{}: {e}", p.display()))?;
                        ohl_lidar::ClassScheme::parse(&text)?;
                        Some(text)
                    }
                    None => None,
                };
                let tiles = ohl_lidar::tiles_in(&tiles).map_err(|e| e.to_string())?;
                // Tiles far from the line: said so, read anyway.
                if let Some(s) = ohl_session::Session::open(&into)
                    .ok()
                    .filter(|s| s.has_line())
                {
                    for w in ohl_lidar::far_tile_warnings(&tiles, s.line()) {
                        eprintln!("warning: {w}");
                    }
                }
                let mut store = ohl_pointcloud::Store::open(&into).map_err(|e| e.to_string())?;
                let started = std::time::Instant::now();
                // Exit clause: ~1 min per 10⁸ points; 2 h is far beyond.
                let budget = ohl_core::budget::Budget::new()
                    .time_limit(std::time::Duration::from_secs(7200));
                let total = tiles.len();
                let (cloud, stats) = store
                    .import_tiles(
                        &name,
                        &tiles,
                        &crs,
                        scheme.as_deref(),
                        chunk,
                        ohl_pointcloud::DEFAULT_SCALE,
                        &budget,
                        |k, tile, n| {
                            eprintln!(
                                "  [{}/{total}] {} ({n} points, {:.0?})",
                                k + 1,
                                tile.file_name()
                                    .map_or(String::new(), |f| f.to_string_lossy().into_owned()),
                                started.elapsed()
                            );
                        },
                    )
                    .map_err(|e| e.to_string())?;
                let size = std::fs::metadata(&into).map_or(0, |m| m.len());
                println!(
                    "imported {} points from {} tiles into \"{}\" in {} ({:.1?}); cloud now {} points, file {:.0} MB",
                    stats.points_read,
                    stats.read,
                    cloud.name,
                    into.display(),
                    started.elapsed(),
                    cloud.points,
                    size as f64 / 1e6
                );
            }
            PointcloudCommand::Export {
                file,
                out,
                name,
                classes,
                area,
            } => {
                let started = std::time::Instant::now();
                let store = ohl_pointcloud::Store::open(&file).map_err(|e| e.to_string())?;
                let cloud = store.cloud(name.as_deref()).map_err(|e| e.to_string())?;
                let area = (area.len() == 4).then(|| ([area[0], area[1]], [area[2], area[3]]));
                // Exit clause: ~1 µs per point for LAZ; 2 h is far beyond.
                let budget = ohl_core::budget::Budget::new()
                    .time_limit(std::time::Duration::from_secs(7200));
                let classes = (!classes.is_empty()).then_some(classes.as_slice());
                let is_copc = out
                    .file_name()
                    .is_some_and(|f| f.to_string_lossy().to_lowercase().ends_with(".copc.laz"));
                let written = if is_copc {
                    store
                        .export_copc(&cloud, &out, classes, area, &budget)
                        .map_err(|e| e.to_string())?
                        .points
                } else {
                    store
                        .export_las(&cloud, &out, classes, area, &budget, |_, _| {})
                        .map_err(|e| e.to_string())?
                        .points_visited
                };
                if let Some(text) = &cloud.class_scheme {
                    let ohlc = out.with_extension(ohl_lidar::classes::CLASS_FILE_EXTENSION);
                    std::fs::write(&ohlc, text).map_err(|e| format!("{}: {e}", ohlc.display()))?;
                }
                println!(
                    "{} points of \"{}\" written to {} in {:.1?}",
                    written,
                    cloud.name,
                    out.display(),
                    started.elapsed()
                );
            }
            PointcloudCommand::Info { file } if ohl_pointcloud::copc::is_copc(&file) => {
                let f = ohl_pointcloud::copc::CopcFile::open(&file)?;
                let [lo, hi] = f.bounds;
                println!(
                    "COPC {}: {} points, point format {}, scale {} m",
                    file.display(),
                    f.points,
                    f.point_format,
                    f.scale[0]
                );
                println!(
                    "  bounds {:.3}, {:.3}, {:.3} – {:.3}, {:.3}, {:.3}",
                    lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
                );
                println!(
                    "  octree: cube of {:.1} m around {:.3}, {:.3}, {:.3}; {} nodes, depth {}; \
                     spacing {:.3} m at the root",
                    2.0 * f.info.halfsize,
                    f.info.center[0],
                    f.info.center[1],
                    f.info.center[2],
                    f.nodes.len(),
                    f.depth(),
                    f.info.spacing
                );
                println!("  level      nodes       points   spacing m");
                for (depth, nodes, points) in f.levels() {
                    println!(
                        "  {depth:>5} {nodes:>10} {points:>12}   {:>9.3}",
                        f.spacing(depth)
                    );
                }
            }
            PointcloudCommand::Info { file } => {
                let store = ohl_pointcloud::Store::open(&file).map_err(|e| e.to_string())?;
                for cloud in store.clouds().map_err(|e| e.to_string())? {
                    let scheme = match &cloud.class_scheme {
                        Some(text) => ohl_lidar::ClassScheme::parse(text)?,
                        None => ohl_lidar::ClassScheme::asprs(),
                    };
                    println!(
                        "\"{}\": {} points, {} m chunks, scale {} m, {} — classes: {}",
                        cloud.name,
                        cloud.points,
                        cloud.chunk_size,
                        cloud.scale,
                        cloud.crs,
                        scheme.name
                    );
                    let lod = store.has_lod(&cloud).map_err(|e| e.to_string())?;
                    println!(
                        "  levels of detail: {}",
                        if lod {
                            "stored"
                        } else {
                            "missing (`ohl pointcloud octree` builds them)"
                        }
                    );
                    for (class, n) in store.class_counts(&cloud).map_err(|e| e.to_string())? {
                        let (cname, roles) = scheme
                            .class(class)
                            .map_or(("(not in the scheme)".to_owned(), String::new()), |c| {
                                (c.name.clone(), c.roles.join(", "))
                            });
                        println!("  {class:>3} {n:>12}  {cname:<48} {roles}");
                    }
                }
            }
            PointcloudCommand::Octree { file, name } => {
                let mut store = ohl_pointcloud::Store::open(&file).map_err(|e| e.to_string())?;
                let clouds: Vec<_> = store
                    .clouds()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter(|c| name.as_ref().is_none_or(|n| &c.name == n))
                    .collect();
                if clouds.is_empty() {
                    return Err(format!("{}: no such point cloud", file.display()));
                }
                // Exit clause: ~30 ns per point; 2 h is far beyond.
                let budget = ohl_core::budget::Budget::new()
                    .time_limit(std::time::Duration::from_secs(7200));
                for cloud in clouds {
                    let started = std::time::Instant::now();
                    let n = store
                        .build_lod(&cloud, &budget, |done, all| {
                            eprint!("\r  \"{}\": {done}/{all} blocks", cloud.name);
                        })
                        .map_err(|e| e.to_string())?;
                    if n > 0 {
                        eprintln!();
                    }
                    println!(
                        "\"{}\": levels of detail of {n} blocks built in {:.1?}{}",
                        cloud.name,
                        started.elapsed(),
                        if n == 0 { " (all there already)" } else { "" }
                    );
                }
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn crs(command: CrsCommand) -> Result<(), String> {
    use ohl_crs::{Crs, Heights, Transformation};
    match command {
        CrsCommand::Info { spec } => {
            let c = Crs::parse(&spec)?;
            println!("{}", c.name());
            println!("  {}", c.definition());
            println!(
                "  {}",
                if c.is_geographic() {
                    "geographic: longitude, latitude in degrees"
                } else {
                    "projected"
                }
            );
        }
        CrsCommand::Transform {
            from,
            to,
            ellipsoidal,
            values,
            points,
        } => {
            if !(points == 2 || points == 3) || values.len() % points != 0 {
                return Err(format!("give the coordinates as groups of {points} values"));
            }
            let t = Transformation::new(
                &Crs::parse(&from)?,
                &Crs::parse(&to)?,
                if ellipsoidal {
                    Heights::Ellipsoidal
                } else {
                    Heights::Keep
                },
            );
            let mut pts: Vec<[f64; 3]> = values
                .chunks(points)
                .map(|c| [c[0], c[1], c.get(2).copied().unwrap_or(0.0)])
                .collect();
            t.points(&mut pts)?;
            for p in pts {
                if points == 3 {
                    println!("{:.9} {:.9} {:.4}", p[0], p[1], p[2]);
                } else {
                    println!("{:.9} {:.9}", p[0], p[1]);
                }
            }
        }
        CrsCommand::Line {
            file,
            from,
            to,
            out,
        } => {
            let mut line = ohl_line::Line::load(&file).map_err(|e| e.to_string())?;
            let from = Crs::parse(from.as_deref().unwrap_or(&line.crs))
                .map_err(|e| format!("{e} (the line's crs is \"{}\"; give --from)", line.crs))?;
            let to = Crs::parse(&to)?;
            if from.is_geographic() || to.is_geographic() {
                return Err(
                    "lines are designed in a projected coordinate system in metres; choose projected systems"
                        .into(),
                );
            }
            let t = Transformation::new(&from, &to, Heights::Keep);
            line.transform_coordinates(|pts| t.points(pts))?;
            line.crs = to.name().to_owned();
            line.validate().map_err(|e| e.to_string())?;
            std::fs::write(&out, line.to_json()).map_err(|e| format!("{}: {e}", out.display()))?;
            println!(
                "{} → {}: {} structures moved; wrote {} (grid distances change with the projection's scale: check spans and stringing)",
                from.name(),
                to.name(),
                line.structures.len(),
                out.display()
            );
        }
    }
    Ok(())
}

fn print_info() {
    println!("OpenOHL {} (ohl)", env!("CARGO_PKG_VERSION"));
    println!("core    {}", ohl_core::VERSION);
    println!("license AGPL-3.0-or-later (plugin interface: MPL-2.0)");
}

fn sagtension(job: &std::path::Path, units: Units, csv: Option<&std::path::Path>) -> ExitCode {
    let units = match units {
        Units::Metric => UnitSystem::Metric,
        Units::Us => UnitSystem::UsCustomary,
    };
    match sagtension_job::run(job, units) {
        Ok(output) => {
            print!("{}", output.report);
            if let Some(path) = csv {
                let Some(chart) = output.chart_csv else {
                    eprintln!("error: --csv given but the job has no [chart] section");
                    return ExitCode::FAILURE;
                };
                if let Err(error) = std::fs::write(path, chart) {
                    eprintln!("error: cannot write {}: {error}", path.display());
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `--loads-per`: `stretched` or `unstretched`.
fn parse_loads_per(text: &str) -> Result<ohl_line::LoadsPer, String> {
    ohl_line::LoadsPer::parse(text)
        .ok_or_else(|| format!("\"{text}\": write stretched or unstretched"))
}

fn line(args: &LineArgs, drawing_spec: Option<&DrawingSpec>) -> ExitCode {
    // A project's own libraries (unless given), and its own weather case,
    // load case and rule files by name; the working copies are removed
    // when `libs` is dropped, at the end.
    let libs = match project::Libraries::resolve(
        &args.file,
        project::GivenLibraries {
            cables: args.library.as_deref(),
            structures: args.structures.as_deref(),
            insulators: args.insulators.as_deref(),
            sections: args.sections.as_deref(),
        },
    ) {
        Ok(l) => l,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let own = |kind: &str, p: &Option<PathBuf>| p.as_deref().map(|p| libs.named(kind, p).0);
    let resolved = LineArgs {
        library: Some(libs.cables.clone()),
        structures: Some(libs.structures.clone()),
        insulators: Some(libs.insulators.clone()),
        sections: libs.sections.clone(),
        weather: own("weather", &args.weather),
        load_cases: own("loadcases", &args.load_cases),
        rules: own("rules", &args.rules),
        ..args.clone()
    };
    let args = &resolved;
    libs.print(&["cables", "structures", "insulators", "sections"]);
    let LineArgs {
        file,
        library,
        structures: structure_dir_arg,
        sections: sections_arg,
        insulators: insulator_dir,
        temperature,
        wind_pressure,
        wind_from,
        ice_thickness,
        ice_load,
        loads_per,
        ice_density,
        weather,
        case,
        tag,
        html,
        csv,
        spans_csv,
        load_cases,
        loads_csv,
        usage_csv,
        stations,
        terrain,
        lidar,
        pointcloud,
        lidar_classes,
        ground_role,
        corridor,
        max_edge,
        thin,
        clearance,
        rules,
        other_line,
        clearance_csv,
        drawings,
        hscale,
        vscale,
        side_offset,
        plan_width,
        plan_scale,
        page,
        break_clearance,
        logo,
        sheet_template,
        field,
        fields,
        revision,
        orientation,
        paper_sizes,
        mixed_sizes,
        same_paper,
        corridor_width,
        sheet_lidar_classes,
        versions,
        role,
        conductor_clearance,
        no_bands,
        plan_drawings,
        plan_orthophoto,
        no_plan_backgrounds,
        plan_background_dpi,
        key_map_drawings,
        key_map_orthophoto,
        backgrounds,
        export,
        export_options,
        plugin,
    } = args;
    use ohl_core::files::shipped_library;
    use ohl_core::units::{Density, ForcePerLength, Length, Stress, Temperature};
    let library = &library.clone().unwrap_or_else(|| shipped_library("cables"));
    let insulator_dir = &insulator_dir
        .clone()
        .unwrap_or_else(|| shipped_library("insulators"));
    use ohl_weather::{WeatherCase, WeatherCases};
    // Pylon files: given, else the project's Structures/ folder, else the
    // shared library.
    let structure_dir: &std::path::Path =
        &project::structures_dir(file, structure_dir_arg.as_deref());

    let run = || -> Result<(), String> {
        let (mut line, types) = project::load_resolved(file, structure_dir, insulator_dir)?;
        // The ice and wind per stretched or unstretched metre (2026-10-21):
        // the command line's choice over the file's, said in the header.
        if let Some(per) = loads_per {
            line.solver.loads_per = *per;
        }
        println!("calculation: {}", line.solver.loads_per.describe());
        let library = ohl_cables::Library::load(library).map_err(|e| e.to_string())?;
        // Formats first: a wrong format or plugin fails before solving.
        let export_options = io_run::options(export_options)?;
        let mut formats = if export.is_empty() {
            None
        } else {
            Some(io_run::registry(plugin)?)
        };
        let cases: Vec<WeatherCase> = match weather {
            Some(path) => {
                let set = WeatherCases::load(path)?;
                // The creep-stretch cases from the whole file.
                line.set_weather_cases(&set.cases);
                let chosen = set.select(case, tag)?;
                println!(
                    "weather: {} — {} of {} cases",
                    if set.name.is_empty() {
                        path.display().to_string()
                    } else {
                        set.name.clone()
                    },
                    chosen.len(),
                    set.cases.len()
                );
                chosen
            }
            None => {
                if !case.is_empty() || !tag.is_empty() {
                    return Err("--case and --tag choose cases of a --weather file".into());
                }
                let c = WeatherCase {
                    wind_pressure: Stress::from_pascals(*wind_pressure),
                    wind_from: match wind_from.as_str() {
                        "right" => ohl_weather::WindSide::Right,
                        "left" => ohl_weather::WindSide::Left,
                        other => {
                            return Err(format!("--wind-from {other}: write left or right"));
                        }
                    },
                    ice_thickness: Length::from_millimeters(*ice_thickness),
                    ice_load: ice_load.map(ForcePerLength::from_newtons_per_meter),
                    ice_density: ice_density.map(Density::from_kilograms_per_cubic_meter),
                    ..WeatherCase::still("command line", Temperature::from_celsius(*temperature))
                };
                c.validate()?;
                vec![c]
            }
        };
        if cases.is_empty() {
            return Err("no weather case to run".into());
        }
        // Report tables for the table formats (XLSX).
        let mut extra_tables = Vec::new();
        if let (Some(lc), Some(w)) = (load_cases, weather) {
            let set = ohl_loads::LoadCases::load(lc)?;
            let all = WeatherCases::load(w)?.cases;
            let started = std::time::Instant::now();
            let mut results = ohl_loads::structure_loads(
                &line,
                &library,
                &all,
                &set.cases,
                &ohl_wirefe::default_fe_budget(),
            )?;
            ohl_loads::add_structure_wind(&mut results, &line, &types, &all);
            println!(
                "structure loads: {} load cases in {:.1?}",
                results.len(),
                started.elapsed()
            );
            for r in &results {
                let max = |k: usize| {
                    r.loads
                        .iter()
                        .map(|l| l.factored[k].abs())
                        .fold(0.0, f64::max)
                        / 1000.0
                };
                println!(
                    "  {:<24} {:>4} supports  largest factored kN: transverse {:>8.2}  \
                     longitudinal {:>8.2}  vertical {:>8.2}{}",
                    r.case.name,
                    r.loads.len(),
                    max(0),
                    max(1),
                    max(2),
                    if r.failed.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "  ({} sections failed: {})",
                            r.failed.len(),
                            r.failed.join("; ")
                        )
                    }
                );
            }
            extra_tables.push(ohl_io::tables::Table::from_csv(
                "Structure loads",
                &ohl_loads::to_csv(&results),
            ));
            if let Some(out) = loads_csv {
                std::fs::write(out, ohl_loads::to_csv(&results))
                    .map_err(|e| format!("{}: {e}", out.display()))?;
                println!("  written to {}", out.display());
            }
            // Structure checks: the types' allowable spans and loads.
            let sections = match sections_arg
                .clone()
                .or_else(|| Some(file.parent()?.join("Sections")).filter(|d| d.is_dir()))
            {
                Some(p) => ohl_sections::SectionLibrary::load(&p)?,
                None => ohl_sections::SectionLibrary::default(),
            };
            let usage = ohl_loads::capacity::usage_with_sections(
                &line, &types, &sections, &library, &all, &results,
            );
            let governing = ohl_loads::capacity::governing(&usage);
            if !governing.is_empty() {
                let over = governing
                    .iter()
                    .filter(|(_, s)| s.governing().is_some_and(|g| g.usage > 1.0))
                    .count();
                println!(
                    "structure usage: {} structures checked, {over} over 100 %",
                    governing.len()
                );
                for (case, s) in governing.iter().take(10) {
                    let g = s.governing().expect("checked");
                    println!(
                        "  {:<12} {:>12}  {:<20} {}{}",
                        s.structure,
                        if g.usage.is_finite() {
                            format!("{:.1} %", g.usage * 100.0)
                        } else {
                            "not allowed".into()
                        },
                        case,
                        g.governs,
                        if s.is_incomplete() {
                            "  (incomplete)"
                        } else if s.is_approximate() {
                            "  (approximate)"
                        } else {
                            ""
                        }
                    );
                }
            }
            // Structures whose loads miss a wire not solved: not checked
            // in that case, whatever their usage.
            let incomplete: Vec<String> = usage
                .iter()
                .flat_map(|c| {
                    c.structures.iter().filter(|s| s.is_incomplete()).map(|s| {
                        format!("{} in {} ({})", s.structure, c.case, s.not_checked_text())
                    })
                })
                .collect();
            if !incomplete.is_empty() {
                println!(
                    "structure usage incomplete (a section or jumper not solved), {}:",
                    incomplete.len()
                );
                for line in incomplete.iter().take(20) {
                    println!("  {line}");
                }
            }
            // Structures whose finite-element check kept tension-only
            // members active (their set did not settle): approximate.
            let approximate: Vec<String> = usage
                .iter()
                .flat_map(|c| {
                    c.structures
                        .iter()
                        .filter(|s| s.is_approximate())
                        .map(|s| format!("{} in {}", s.structure, c.case))
                })
                .collect();
            if !approximate.is_empty() {
                println!(
                    "structure usage approximate (tension-only members kept active, their \
                     set did not settle; to be treated later), {}:",
                    approximate.len()
                );
                for line in approximate.iter().take(20) {
                    println!("  {line}");
                }
            }
            extra_tables.push(ohl_io::tables::Table::from_csv(
                "Structure usage",
                &ohl_loads::capacity::usage_csv(&usage),
            ));
            if let Some(out) = usage_csv {
                std::fs::write(out, ohl_loads::capacity::usage_csv(&usage))
                    .map_err(|e| format!("{}: {e}", out.display()))?;
                println!("  written to {}", out.display());
            }
        }
        if *stations {
            for alignment in line.alignment_views() {
                println!(
                    "Alignment {} {} (route {})",
                    alignment.id, alignment.name, alignment.route
                );
                println!(
                    "  {:<12} {:>11} {:>9} {:>10} {:>10}  also on routes",
                    "structure", "station m", "offset m", "angle °", "ahead m"
                );
                // Print tiny rounding values as 0.00, not -0.00.
                let clean = |x: f64| if x.abs() < 0.005 { 0.0 } else { x };
                for st in alignment.stations(&line).map_err(|e| e.to_string())? {
                    // The structure's routes other than this alignment's.
                    let others: Vec<String> = line
                        .routes_of(&st.structure)
                        .into_iter()
                        .filter(|r| *r != alignment.route)
                        .collect();
                    println!(
                        "  {:<12} {:>11.2} {:>9.2} {:>10.2} {:>10.2}  {}",
                        st.structure,
                        clean(st.station),
                        clean(st.offset),
                        clean(st.line_angle_deg),
                        clean(st.ahead_span),
                        others.join(", ")
                    );
                }
            }
        }
        let prepared = clearance_run::prepare(
            &line,
            &clearance_run::Options {
                terrain: terrain.as_deref(),
                lidar: lidar.as_deref(),
                pointcloud: pointcloud.as_deref(),
                lidar_classes: lidar_classes.as_deref(),
                ground_role,
                corridor: *corridor,
                max_edge: *max_edge,
                thin: *thin,
                rules: rules.as_deref(),
                clearance: *clearance,
                other_lines: other_line,
                structures: structure_dir,
                insulators: insulator_dir,
                project: (file.extension().and_then(|e| e.to_str()) == Some("ohl"))
                    .then_some(file.as_path()),
            },
        )?;
        let mut clearance_rows = String::from(clearance_run::CSV_HEADER);
        // Outputs (viewer, CSV, drawings) show the first case.
        let mut first: Option<(WeatherCase, Vec<ohl_line::solve::SectionShape>)> = None;
        // Every case's solution, for the drawings (one trace per case).
        let mut solved_cases: Vec<(
            WeatherCase,
            Vec<ohl_line::solve::SectionShape>,
            Vec<ohl_line::jumper::JumperShape>,
        )> = Vec::new();
        let mut case_wires = Vec::new();
        let mut envelope = ohl_clearance::Envelope::default();
        let mut extras = ohl_line::export::ViewerExtras::default();
        let mut failed = 0;
        for case in &cases {
            let started = std::time::Instant::now();
            let results = ohl_line::solve::solve_line(&line, &library, case);
            let elapsed = started.elapsed();
            println!(
                "{}: {} structures, {} sections — case \"{}\": {}",
                line.name,
                line.structures.len(),
                line.sections.len(),
                case.name,
                case.summary()
            );
            println!(
                "  {:<12} {:<22} {:>5} {:>12} {:>10} {:>6}",
                "section", "circuit / phase", "spans", "max T (kN)", "max sag", "iter"
            );
            let mut shapes = Vec::new();
            for (section, result) in line.sections.iter().zip(results) {
                match result {
                    Ok(shape) => {
                        let max_t = shape
                            .spans
                            .iter()
                            .map(|s| s.max_tension)
                            .fold(0.0, f64::max);
                        let max_sag = shape.spans.iter().map(|s| s.sag).fold(0.0, f64::max);
                        println!(
                            "  {:<12} {:<22} {:>5} {:>12.2} {:>8.2} m {:>6}",
                            section.id,
                            format!("{} {}", section.circuit_name, section.phase_label),
                            shape.spans.len(),
                            max_t / 1e3,
                            max_sag,
                            shape.iterations
                        );
                        // Slack strings of a V (or more): the clamp hangs on
                        // the others.
                        for (point, strings) in section.points.iter().zip(&shape.strings) {
                            if strings.len() >= 2 {
                                for x in strings.iter().filter(|x| x.slack) {
                                    println!(
                                        "  {:<12}   slack string {} at {} set {}",
                                        "", x.label, point.structure, point.set
                                    );
                                }
                            }
                        }
                        for flag in &shape.flags {
                            println!("  {:<12}   {flag}", "");
                        }
                        shapes.push(shape);
                    }
                    Err(error) => {
                        failed += 1;
                        println!("  {:<12} error: {error}", section.id);
                    }
                }
            }
            println!(
                "solved {} of {} sections in {elapsed:.1?}",
                shapes.len(),
                line.sections.len()
            );
            let mut jumpers = Vec::new();
            if !line.jumpers.is_empty() {
                println!(
                    "  {:<16} {:>10} {:>9} {:>11}",
                    "jumper", "length m", "sag m", "max T (N)"
                );
                for (jumper, result) in line.jumpers.iter().zip(ohl_line::jumper::solve_jumpers(
                    &line, &library, case, &shapes,
                )) {
                    match result {
                        Ok(shape) => {
                            println!(
                                "  {:<16} {:>10.3} {:>9.3} {:>11.0}{}",
                                jumper.id,
                                shape.unstressed_length,
                                shape.sag,
                                shape.max_tension,
                                shape
                                    .supports
                                    .iter()
                                    .flat_map(|s| &s.strings)
                                    .map(|s| {
                                        let name = if s.label.is_empty() {
                                            "support string"
                                        } else {
                                            s.label.as_str()
                                        };
                                        format!("   {name} {:.0} N", s.force)
                                    })
                                    .collect::<String>()
                            );
                            jumpers.push(shape);
                        }
                        Err(error) => {
                            failed += 1;
                            println!("  {:<16} error: {error}", jumper.id);
                        }
                    }
                }
            }
            case_wires.push(clearance_run::check(
                &line,
                &library,
                case,
                &shapes,
                &jumpers,
                &prepared,
                &mut clearance_rows,
                &mut extras,
                &mut envelope,
            )?);
            solved_cases.push((case.clone(), shapes.clone(), jumpers.clone()));
            if first.is_none() {
                extras.jumpers = jumpers;
                first = Some((case.clone(), shapes));
            }
        }
        clearance_run::cross_cases(
            &prepared,
            &case_wires,
            &mut clearance_rows,
            &mut extras,
            &mut envelope,
        );
        clearance_run::envelope_report(&prepared, &envelope, &mut clearance_rows);
        let (first_case, shapes) = first.expect("at least one case");
        if cases.len() > 1 && (html.is_some() || csv.is_some() || spans_csv.is_some()) {
            println!(
                "viewer and wire CSVs show the first case, \"{}\" (violation markers of all cases); drawings show every case",
                first_case.name
            );
        }
        if let Some(path) = clearance_csv {
            std::fs::write(path, &clearance_rows)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(g) = prepared
            .ground
            .as_ref()
            .filter(|g| g.num_triangles() <= ohl_line::export::MAX_VIEWER_TRIANGLES)
        {
            extras.ground = Some(g.mesh());
        }
        let ground = prepared.ground.as_ref();
        // --drawings: SVG and profile CSV of every route, as before;
        // `ohl drawing`: what it asks for.
        let legacy = drawings.as_ref().map(|dir| DrawingSpec {
            routes: Vec::new(),
            export: ohl_drawing::export::Export {
                formats: vec![
                    ohl_drawing::export::Format::Svg,
                    ohl_drawing::export::Format::Csv,
                ],
                ..Default::default()
            },
            from: None,
            to: None,
            profile_only: false,
            elevations: Default::default(),
            isoclearance: None,
            out: dir.clone(),
        });
        // Structures at levels 2 and 3: their members on their own layer
        // (profiles from the cross-section library the checks read).
        let bodies = if legacy.is_some() || drawing_spec.is_some() {
            let sections = match sections_arg
                .clone()
                .or_else(|| Some(file.parent()?.join("Sections")).filter(|d| d.is_dir()))
            {
                // Drawn only: a library that does not read leaves models
                // naming its sections unread (their members not drawn).
                Some(p) => ohl_sections::SectionLibrary::load(&p).unwrap_or_else(|e| {
                    eprintln!("warning: cross-section library not read for the drawings: {e}");
                    ohl_sections::SectionLibrary::default()
                }),
                None => ohl_sections::SectionLibrary::default(),
            };
            ohl_structures_fe::drawn::Bodies::load(&types, &sections)
        } else {
            ohl_structures_fe::drawn::Bodies::default()
        };
        let members = |line: &ohl_line::Line, id: &str| {
            let st = line.structure(id)?;
            let b = bodies.get(st.structure_type.as_deref()?, st.extension.as_deref())?;
            let at = [
                st.position[0],
                st.position[1],
                st.position[2] + st.height_adjust,
            ];
            Some(b.placed_lines(at, st.orientation_deg)).filter(|l| !l.is_empty())
        };
        // Poles at levels 2 and 3: their outline on the same layer.
        let poles = |line: &ohl_line::Line, id: &str| {
            let st = line.structure(id)?;
            let b = bodies.get(st.structure_type.as_deref()?, st.extension.as_deref())?;
            let at = [
                st.position[0],
                st.position[1],
                st.position[2] + st.height_adjust,
            ];
            Some(b.placed_pole_rings(at, st.orientation_deg)).filter(|r| !r.is_empty())
        };
        for spec in legacy.iter().chain(drawing_spec) {
            let logos = logo
                .iter()
                .map(|spec| {
                    let (file, width) = match spec.rsplit_once(':') {
                        Some((f, w)) if w.parse::<f64>().is_ok() => (f, w.parse().unwrap_or(30.0)),
                        _ => (spec.as_str(), 30.0),
                    };
                    ohl_drawing::Logo::from_file(std::path::Path::new(file), width)
                })
                .collect::<Result<Vec<_>, _>>()?;
            // The sheet template (its page unless --page), the fields.
            let template = std::sync::Arc::new(match sheet_template {
                Some(t) => sheet_run::resolve(t, Some(file), plugin)?,
                None => ohl_drawing::SheetTemplate::builtin(),
            });
            // The optimised layout (--page optimised, or the template's).
            let mut optimise = match page.as_deref() {
                Some("optimised") => {
                    let names = if paper_sizes.is_empty() {
                        ohl_drawing::paper::default_sizes()
                    } else {
                        paper_sizes.clone()
                    };
                    Some(ohl_drawing::paper::Optimise::from_names(
                        &names,
                        *mixed_sizes,
                    )?)
                }
                None if sheet_template.is_some() => template.optimise(),
                _ => None,
            };
            if let Some(o) = &mut optimise {
                match orientation.as_deref() {
                    Some("landscape") => o.portrait = false,
                    Some("portrait") => o.landscape = false,
                    _ => {}
                }
            }
            let page = match page.as_deref() {
                Some("optimised") => None,
                Some(name) => Some(ohl_drawing::page_format(name).ok_or_else(|| {
                    format!(
                        "unknown page format \"{name}\" (a standard paper size such as A1, \
                         A3x3, B2, \"ANSI D\", WIDTHxHEIGHT in mm, or optimised)"
                    )
                })?),
                None if sheet_template.is_some() => template.page(),
                None => None,
            };
            let page = match (page, orientation.as_deref()) {
                (Some(p), Some("portrait")) => Some(p.portrait()),
                (Some(p), Some("landscape")) if p.height > p.width => Some(ohl_drawing::Page {
                    name: p.name.trim_end_matches(" portrait").to_owned(),
                    width: p.height,
                    height: p.width,
                }),
                (p, _) => p,
            };
            let sheet_fields = sheet_run::fields(fields.as_deref(), field, revision)?;
            let options = ohl_drawing::Options {
                horizontal_scale: *hscale,
                vertical_scale: *vscale,
                side_offsets: side_offset.clone(),
                clearance: *clearance,
                plan_half_width: *plan_width,
                title: {
                    let mut t = vec![line.name.clone()];
                    let names: Vec<String> = solved_cases
                        .iter()
                        .enumerate()
                        .map(|(k, (c, _, _))| {
                            format!(
                                "{}{}",
                                c.name,
                                if k == 0 {
                                    " (solid)".to_owned()
                                } else {
                                    format!(" ({})", ohl_drawing::case_dash_name(k))
                                }
                            )
                        })
                        .collect();
                    t.push(if names.len() > 1 {
                        format!("Weather cases: {}", names.join(", "))
                    } else {
                        format!("{}: {}", first_case.name, first_case.summary())
                    });
                    t
                },
                plan_offset_scale: *plan_scale,
                logos,
                stations: None,
                plan: !spec.profile_only,
                break_clearance: *break_clearance,
                fields: sheet_fields,
                info: ohl_drawing::SheetInfo {
                    project: if project::is_project(file) {
                        file.file_stem()
                            .map_or(String::new(), |s| s.to_string_lossy().into_owned())
                    } else {
                        String::new()
                    },
                    line: line.name.clone(),
                    crs: line.crs.clone(),
                    date: ohl_drawing::today(),
                    cases: solved_cases
                        .iter()
                        .map(|(c, _, _)| c.name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    height_datum: line.height_datum_label(),
                },
                role: role.clone(),
                corridor: *corridor_width,
                conductor_clearance: if *conductor_clearance || template.conductor_clearance.show {
                    let (specs, notes) =
                        ohl_drawing::curves::conductor_clearances(&line, prepared.rules());
                    for n in notes {
                        println!("  warning: {n}");
                    }
                    if specs.is_empty() && *conductor_clearance {
                        println!(
                            "  warning: no line below the lowest conductor: no ground rule with a \
                             vertical distance (--rules, --clearance)"
                        );
                    }
                    specs
                } else {
                    Vec::new()
                },
                bands: !*no_bands,
                lidar_classes: sheet_lidar_classes.clone(),
                ..Default::default()
            }
            .with_template(template.clone());
            // LiDAR points on the sheets: read once, within the corridor
            // (else the line width) of every route drawn.
            let (options, sheet_lidar) = sheet_run::sheet_lidar(
                &line,
                options,
                lidar.as_deref(),
                pointcloud.as_deref(),
                lidar_classes.as_deref(),
                &spec.routes,
            )?;
            // The command line's elevations over the template's.
            let options = ohl_drawing::Options {
                elevations: spec.elevations.over(options.elevations)?,
                ..options
            };
            // The project's drawings and orthophotos under the plan.
            let (options, plan_backgrounds) = sheet_run::plan_backgrounds(
                options,
                &sheet_run::BackgroundArgs {
                    drawings: plan_drawings.as_deref(),
                    photos: plan_orthophoto.as_deref(),
                    off: *no_plan_backgrounds,
                    dpi: *plan_background_dpi,
                    key_drawings: *key_map_drawings,
                    key_photo: *key_map_orthophoto,
                },
                backgrounds
                    .as_deref()
                    .or_else(|| project::is_project(file).then_some(file.as_path())),
            )?;
            // Other versions of the line: solved in the same weather cases.
            let mut other_versions = Vec::new();
            for v in versions {
                let (given, role, label) = sheet_run::parse_version(v)?;
                let mut vline =
                    sheet_run::version_line(file, &given, structure_dir, insulator_dir)?;
                // A line file is called by its line's name.
                let name = if std::path::Path::new(&given).is_file() && !vline.name.is_empty() {
                    vline.name.clone()
                } else {
                    given
                };
                let weather: Vec<WeatherCase> =
                    solved_cases.iter().map(|(c, _, _)| c.clone()).collect();
                vline.set_weather_cases(&weather);
                let solved: Vec<_> = solved_cases
                    .iter()
                    .map(|(case, _, _)| {
                        let shapes: Vec<ohl_line::solve::SectionShape> =
                            ohl_line::solve::solve_line(&vline, &library, case)
                                .into_iter()
                                .filter_map(Result::ok)
                                .collect();
                        let jumpers: Vec<ohl_line::jumper::JumperShape> =
                            ohl_line::jumper::solve_jumpers(&vline, &library, case, &shapes)
                                .into_iter()
                                .filter_map(Result::ok)
                                .collect();
                        (case.name.clone(), shapes, jumpers)
                    })
                    .collect();
                println!(
                    "version {name} ({role}): {} structures, {} sections, solved in {} case(s)",
                    vline.structures.len(),
                    vline.sections.len(),
                    solved.len()
                );
                other_versions.push((name, role, label, vline, solved));
            }
            let mut target = ohl_drawing::export::Export {
                page: page.clone(),
                optimise: optimise.clone(),
                ..spec.export.clone()
            };
            // Isoclearance lines: once for every route of the drawing.
            let isolines = match &spec.isoclearance {
                None => Vec::new(),
                Some((iso, swing_arc)) => {
                    let ground = ground.ok_or("isoclearance lines need the ground (--terrain)")?;
                    let cases: Vec<WeatherCase> =
                        solved_cases.iter().map(|(c, _, _)| c.clone()).collect();
                    // Exit clause: an hour for the solves and the lines.
                    let budget = ohl_core::budget::Budget::new()
                        .time_limit(std::time::Duration::from_secs(3600));
                    let positions = ohl_clearance::positions::solve_positions(
                        &line,
                        &library,
                        &cases,
                        ohl_clearance::positions::PositionOptions {
                            swing_arc: *swing_arc,
                            ..Default::default()
                        },
                        &budget,
                    )?;
                    let corridor = ohl_lidar::Corridor::along_spans(&line, *plan_width);
                    let bounds = corridor
                        .bounds()
                        .ok_or("isoclearance lines: no span to draw them along")?;
                    let t = std::time::Instant::now();
                    let found = ohl_clearance::isoclearance::isoclearance(
                        &positions,
                        ground,
                        bounds,
                        &|x, y| corridor.contains(x, y),
                        iso,
                        &budget,
                    )?;
                    println!(
                        "isoclearance: {} lines every {} m up to {} m over the line width ({:.1?})",
                        found.contours.len(),
                        iso.interval,
                        iso.max_level,
                        t.elapsed()
                    );
                    found
                        .contours
                        .into_iter()
                        .map(|c| (c.level, c.points))
                        .collect::<Vec<_>>()
                }
            };
            // The template's weather cases (`[cases]`): each kind of
            // wire drawn in its own, and the stringing band's, among the
            // cases solved (--case).
            let listed: Vec<(String, f64, bool)> = solved_cases
                .iter()
                .map(|(c, _, _)| {
                    (
                        c.name.clone(),
                        c.temperature.celsius(),
                        c.wind_pressure.si() == 0.0
                            && c.ice_thickness.si() == 0.0
                            && c.ice_load.is_none(),
                    )
                })
                .collect();
            let pick = |choice: &Option<String>, what: &str| -> Result<Option<usize>, String> {
                match choice {
                    None => Ok(None),
                    Some(c) => ohl_drawing::template::CasesSpec::pick(c, &listed)
                        .map(Some)
                        .ok_or_else(|| {
                            format!(
                                "the sheet template's {what} case \"{c}\" is not among the \
                                 cases solved: add it with --case"
                            )
                        }),
                }
            };
            let tc = &template.cases;
            let (k_cond, k_earth, k_string) = (
                pick(&tc.conductors, "conductors'")?,
                pick(&tc.earth_wires, "earth wires'")?,
                pick(&tc.stringing, "stringing")?,
            );
            let mut shapes = shapes.clone();
            if k_cond.is_some() || k_earth.is_some() {
                for s in &mut shapes {
                    let earth = line
                        .sections
                        .iter()
                        .find(|x| x.id == s.section)
                        .is_some_and(|x| x.system.is_none());
                    let k = if earth { k_earth } else { k_cond };
                    if let Some(k) = k
                        && let Some(other) =
                            solved_cases[k].1.iter().find(|x| x.section == s.section)
                    {
                        *s = other.clone();
                    }
                }
            }
            let jumpers = match k_cond {
                Some(k) => solved_cases[k].2.clone(),
                None => extras.jumpers.clone(),
            };
            let routes = line.alignment_views();
            for id in &spec.routes {
                if !routes.iter().any(|r| &r.id == id) {
                    let ids: Vec<&str> = routes.iter().map(|r| r.id.as_str()).collect();
                    return Err(format!("no route \"{id}\" (routes: {})", ids.join(", ")));
                }
            }
            // Every route built first: one paper for all of them may
            // depend on each.
            let mut built = Vec::new();
            for route in routes
                .iter()
                .filter(|r| spec.routes.is_empty() || spec.routes.contains(&r.id))
            {
                let mut d = ohl_drawing::build(
                    &line,
                    route,
                    &shapes,
                    &jumpers,
                    ground,
                    &extras.markers,
                    &options,
                )?;
                if let Some(k) = k_string {
                    d.set_stringing(&line, &solved_cases[k].1, &solved_cases[k].0.name);
                }
                // The sags of the wires as drawn (each in its case).
                d.set_sags(&line, &shapes);
                d.set_members(|id| members(&line, id));
                d.set_poles(|id| poles(&line, id));
                d.set_case(&first_case.name);
                if !sheet_lidar.is_empty() {
                    d.set_lidar(
                        sheet_lidar.iter().copied(),
                        &options.lidar_class_list(),
                        options.lidar_half_width(),
                    );
                }
                plan_backgrounds.apply(&mut d, &options);
                for (name, role, label, vline, solved) in &other_versions {
                    for (k, (case, shapes, jumpers)) in solved.iter().enumerate() {
                        d.add_version(
                            vline,
                            shapes,
                            jumpers,
                            name,
                            role,
                            label,
                            options.plan_half_width,
                            (case, (k > 0).then(|| ohl_drawing::case_dash(k))),
                        );
                    }
                }
                if let Some((iso, _)) = &spec.isoclearance {
                    d.set_isoclearance(&isolines, iso.max_level);
                }
                // The other cases' wires, dashed (the envelope of positions).
                for (k, (case, case_shapes, case_jumpers)) in
                    solved_cases.iter().enumerate().skip(1)
                {
                    let other = ohl_drawing::build(
                        &line,
                        route,
                        case_shapes,
                        case_jumpers,
                        None,
                        &[],
                        &options,
                    )?;
                    for mut w in other.wires {
                        w.label = format!("{} — {}", w.label, case.name);
                        w.dash = Some(ohl_drawing::case_dash(k));
                        w.case = case.name.clone();
                        d.wires.push(w);
                    }
                }
                let options = ohl_drawing::Options {
                    stations: (spec.from.is_some() || spec.to.is_some())
                        .then(|| [spec.from.unwrap_or(0.0), spec.to.unwrap_or(d.length)]),
                    ..options.clone()
                };
                built.push((route, d, options));
            }
            if *same_paper && let Some(opt) = &optimise {
                let all: Vec<_> = built.iter().map(|(_, d, o)| (d, o)).collect();
                let common = ohl_drawing::paper::common_choice(&all, opt);
                target.optimise = Some(common);
            }
            for (route, d, options) in &built {
                // Exit clause: an hour per route (minutes are expected for
                // hundreds of PNG sheets).
                let budget = ohl_core::budget::Budget::new()
                    .time_limit(std::time::Duration::from_secs(3600));
                let written =
                    ohl_drawing::export::write_route(d, options, &target, &spec.out, &budget)?;
                for w in &written.warnings {
                    println!("  warning: {w}");
                }
                if let Some(paper) = &written.paper {
                    println!("  route {}: optimised layout {paper}", route.id);
                }
                if let Some(page) = &page {
                    println!(
                        "  route {}: {} sheet(s) {} at 1:{} / 1:{}",
                        route.id,
                        written.sheets,
                        page.name,
                        options.horizontal_scale,
                        options.vertical_scale
                    );
                }
                println!(
                    "drawing: route {} ({:.0} m, {} structures, {} wires) → {} file(s) in {}",
                    route.id,
                    d.length,
                    d.structures.len(),
                    d.wires.len(),
                    written.files.len(),
                    spec.out.display()
                );
            }
        }
        if let Some(path) = html {
            std::fs::write(
                path,
                ohl_line::export::viewer_html_with(&line, &shapes, &line.name, &extras),
            )
            .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(path) = csv {
            std::fs::write(path, ohl_line::export::wires_csv(&shapes))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(path) = spans_csv {
            std::fs::write(path, ohl_line::export::spans_csv(&shapes))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(registry) = &mut formats {
            let solved: Vec<_> = solved_cases
                .iter()
                .map(|(c, s, j)| (c.name.clone(), c.summary(), s.clone(), j.clone()))
                .collect();
            let mut model = ohl_io::ExportModel::new(&line, &solved);
            model.tables = extra_tables;
            io_run::export(&model, export, &export_options, registry)?;
        }
        if failed > 0 {
            return Err(format!("{failed} sections or jumpers failed"));
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn structures_derive(file: &std::path::Path, out: &std::path::Path, inline: bool) -> ExitCode {
    use ohl_structures::{
        InsulatorAssembly, InsulatorLibrary, LibraryItem, StructureLibrary, StructureType,
    };

    let run = || -> Result<(), String> {
        let line = ohl_line::Line::load(file).map_err(|e| e.to_string())?;
        let (derived, typed) = line.derive_libraries(inline);
        // Simple pylon files: one per type in the project's Structures/.
        let (sdir, idir) = if inline {
            (out.join("Structures"), out.join("insulators"))
        } else {
            (out.join("structures"), out.join("insulators"))
        };
        std::fs::create_dir_all(&sdir).map_err(|e| format!("{}: {e}", sdir.display()))?;
        if !inline {
            std::fs::create_dir_all(&idir).map_err(|e| format!("{}: {e}", idir.display()))?;
        }
        let mut structures = StructureLibrary::default();
        let mut insulators = InsulatorLibrary::default();
        for (id, text) in &derived.structure_types {
            let path = sdir.join(format!("{id}.ohls"));
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            structures
                .insert(
                    &path,
                    StructureType::parse(&path, text).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
        }
        for (id, text) in &derived.insulators {
            let path = idir.join(format!("{id}.ohli"));
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            insulators
                .insert(
                    &path,
                    InsulatorAssembly::parse(&path, text).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
        }
        let stem = file
            .file_name()
            .and_then(|n| n.to_str())
            .map_or("line", |n| ohl_core::files::line_file_stem(n).unwrap_or(n));
        let typed_path = out.join(format!("{stem}-typed.ohl.json"));
        std::fs::write(&typed_path, typed.to_json())
            .map_err(|e| format!("{}: {e}", typed_path.display()))?;
        // Self-check: resolve the typed line and compare with the original.
        let resolved = typed
            .resolve(&structures, &insulators)
            .map_err(|e| e.to_string())?;
        let check = ohl_line::library::compare_attachments(&line, &resolved);
        let worst = (check.worst, check.at);
        println!(
            "{} structures -> {} structure types, {} insulator assemblies",
            line.structures.len(),
            derived.structure_types.len(),
            derived.insulators.len()
        );
        println!(
            "written to {} (typed line: {})",
            out.display(),
            typed_path.display()
        );
        println!(
            "self-check: attachment points and insulator lengths reproduced within {:.3} m (largest at {})",
            worst.0, worst.1
        );
        if inline {
            println!(
                "Note: one simple structure file per type in {}; string weights come from the line when given; labels A<set>-<phase> (top) and W<set>-<phase> (bottom) can be renamed.",
                sdir.display()
            );
        } else {
            println!(
                "Note: insulator weights come from the line when given; check them, and fill in stiffness and wind areas."
            );
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn plugin_info(dir: &std::path::Path) -> ExitCode {
    // The world from the manifest's extension points.
    let result = ohl_plugins::WasmHost::new().and_then(|host| {
        let d = host.describe_any(dir)?;
        Ok((ohl_plugins::Manifest::load(dir)?, d))
    });
    match result {
        Ok((manifest, d)) => {
            println!("name         {}", d.info.name);
            println!("version      {}", d.info.version);
            println!("description  {}", d.info.description);
            println!("world        {}", d.worlds.join(", "));
            println!(
                "extensions   {}",
                if manifest.plugin.extension_points.is_empty() {
                    "none".to_owned()
                } else {
                    manifest.plugin.extension_points.join(", ")
                }
            );
            println!("tier         {}", manifest.plugin.tier);
            println!("sdk          {}", manifest.plugin.sdk);
            println!("entry        {}", manifest.entry_path().display());
            for line in &d.details {
                println!("  {line}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn validate(path: &std::path::Path, report_path: Option<&std::path::Path>, list: bool) -> ExitCode {
    use ohl_validation::{Outcome, Registry, Report, load_cases};

    let registry = Registry::with_builtin();
    if list {
        for calculation in registry.iter() {
            println!("{:<24} {}", calculation.id(), calculation.description());
        }
        return ExitCode::SUCCESS;
    }

    let cases = match load_cases(path) {
        Ok(cases) => cases,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let report = Report::run(&registry, cases);

    for result in &report.results {
        let case = &result.case.case;
        let mark = if result.passed() { "ok  " } else { "FAIL" };
        println!(
            "{mark} {:<16} {:>9.1?}  {}",
            case.id, result.elapsed, case.title
        );
        if !result.passed() {
            match &result.outcome {
                Outcome::Error(message) => println!("       error: {message}"),
                Outcome::TimedOut(limit) => println!("       exceeded time limit of {limit:?}"),
                Outcome::Ran(checks) => {
                    for check in checks.iter().filter(|c| !c.passed()) {
                        let actual = check.actual.map_or("missing".into(), |a| a.to_string());
                        println!(
                            "       {}: expected {}, got {actual}",
                            check.name, check.expected.value
                        );
                    }
                }
            }
        }
    }
    println!(
        "\n{} cases: {} passed, {} failed",
        report.results.len(),
        report.passed(),
        report.failed()
    );

    if let Some(report_path) = report_path
        && let Err(error) = std::fs::write(report_path, report.to_markdown())
    {
        eprintln!("error: cannot write {}: {error}", report_path.display());
        return ExitCode::from(2);
    }

    if report.all_passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
