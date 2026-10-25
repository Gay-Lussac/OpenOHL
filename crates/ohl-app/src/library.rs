// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Library editors: wires (`.ohlw`), insulator assemblies (`.ohli`),
//! structure types (`.ohls`) and weather cases (`.ohlwc`). One editor for
//! all of them: the file is edited as TOML (`toml_edit`, so comments and
//! layout stay), every field as text with its units (`"28.1 mm"`), with
//! the known fields of each kind to add. A change is checked at once with
//! the library's own reader; a file is saved only when it reads, then the
//! model reads its libraries again and solves.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::model::Sources;

/// A kind of library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LibraryKind {
    /// Wires and cables (`.ohlw`, several per file).
    Cables,
    /// Insulator assemblies (`.ohli`, one per file).
    Insulators,
    /// Structure types (`.ohls`, one per file).
    StructureTypes,
    /// Weather cases (`.ohlwc`, several per file).
    WeatherCases,
    /// Load cases (`.ohllc`, several per file).
    LoadCases,
}

/// A field that can be added: key, example value (TOML), help.
type Field = (&'static str, &'static str, &'static str);

const COMPONENT: &[Field] = &[
    ("area", "\"400 mm²\"", "cross-section of this component"),
    ("modulus", "\"55 GPa\"", "elastic (final) modulus"),
    (
        "thermal_expansion",
        "23e-6",
        "linear thermal expansion, 1/K",
    ),
    (
        "strands",
        "26",
        "number of wires of this component (with a core: give both) — the least bending stiffness of a jumper",
    ),
    (
        "stress_strain",
        "{ stress_unit = \"MPa\", strain = \"percent\", basis = \"total\", initial = [0.0, 550.0, 0.0, 0.0, 0.0], creep = [0.0, 550.0, 0.0, 0.0, 0.0], final_modulus = 550.0, reference_temperature = \"20 °C\" }",
        "non-linear model: polynomials of this material, stress (stress_unit) of strain (% or unit), \
         on the total wire area (basis = \"total\": actual values × component area / total \
         area) or its own (\"component\"); initial a0…a4, creep c0…c4, final_modulus per strain \
         unit, the temperature the strand data were obtained at. Bimetallic cables: give the \
         outer and the core their own (what the outer strands do above the transition \
         temperature: the cable's bimetallic options)",
    ),
];

const CABLE: &[Field] = &[
    (
        "id",
        "\"new-cable\"",
        "unique id, used by the sections (letters, digits, - _ .)",
    ),
    ("name", "\"New wire type\"", "display name"),
    (
        "kind",
        "\"acsr\"",
        "acsr, acss, aac, aaac, acar, accc, opgw, adss, steel or other",
    ),
    ("source", "\"data sheet …\"", "where the values come from"),
    ("diameter", "\"28.1 mm\"", "overall diameter"),
    ("mass_per_length", "\"1.63 kg/m\"", "mass per length"),
    (
        "reference_temperature",
        "\"20 °C\"",
        "the data sheet's temperature of the mass per length (default 20 °C)",
    ),
    (
        "rated_tensile_strength",
        "\"120 kN\"",
        "rated tensile strength (RTS)",
    ),
    (
        "outer",
        "{ area = \"400 mm²\", modulus = \"55 GPa\", thermal_expansion = 23e-6 }",
        "outer component (the whole wire if homogeneous)",
    ),
    (
        "core",
        "{ area = \"50 mm²\", modulus = \"190 GPa\", thermal_expansion = 11.5e-6 }",
        "core component (bimetallic wires)",
    ),
    (
        "model",
        "\"nonlinear\"",
        "wire model: nonlinear (polynomials for the initial and creep behaviour of the outer \
         and core materials), linear_creep_tension (linear elastic, permanent stretch due to \
         creep proportional to the creep weather case tension), linear_creep_temperature \
         (linear elastic, permanent stretch due to creep given as a temperature increase)",
    ),
    (
        "creep_temperature_shift",
        "15.0",
        "linear models: the permanent stretch due to creep as a temperature increase, °C",
    ),
    (
        "creep_shift_tension",
        "\"25 kN\"",
        "linear_creep_tension: the creep weather case tension at which the stretch equals the \
         temperature shift (scaled by the tension)",
    ),
    (
        "bimetallic_behavior",
        "\"compression\"",
        "bimetallic wires, above the transition temperature: compression (the outer strands \
         can go into compression, up to max_virtual_compression) or bird_cage (they take no \
         compression)",
    ),
    (
        "max_virtual_compression",
        "\"68947.4 MPa\"",
        "bimetallic_behavior = compression: the largest virtual compressive stress of the outer \
         strands (actual stress × outer area / total area)",
    ),
];

const CASE: &[Field] = &[
    ("name", "\"new case\"", "unique name"),
    ("description", "\"\"", "free text"),
    (
        "tags",
        "[\"clearance\"]",
        "for choosing cases (e.g. clearance, loads, stringing)",
    ),
    ("temperature", "\"15 °C\"", "wire temperature"),
    ("ice_thickness", "\"10 mm\"", "radial ice thickness"),
    ("ice_load", "\"15 N/m\"", "or: ice load per length"),
    (
        "ice_density",
        "\"913 kg/m³\"",
        "ice density (wind diameter)",
    ),
    (
        "wind_pressure",
        "\"400 Pa\"",
        "wind pressure on the projected diameter",
    ),
];

const LOAD_CASE: &[Field] = &[
    ("name", "\"new load case\"", "unique name"),
    (
        "description",
        "\"\"",
        "what it stands for (e.g. a code clause)",
    ),
    ("weather", "\"everyday\"", "the weather case, by name"),
    (
        "condition",
        "\"initial\"",
        "the wires' condition: initial, creep, load or max_sag",
    ),
    ("vertical", "1.0", "factor on the vertical loads"),
    ("transverse", "1.0", "factor on the transverse loads"),
    ("longitudinal", "1.0", "factor on the longitudinal loads"),
    (
        "strength",
        "1.0",
        "strength (resistance) factor of the capacity checks",
    ),
    (
        "tags",
        "[\"ULS\"]",
        "for choosing cases (code, limit state…)",
    ),
    (
        "structure",
        "{ wind_pressure = \"700 Pa\", wind_factor = 1.0 }",
        "the wind on the structures' own bodies when it differs from the wires' (file version \
         2; absent: the weather case's pressure and the transverse factor)",
    ),
];

const LOAD_CASE_STRUCTURE: &[Field] = &[
    (
        "wind_pressure",
        "\"700 Pa\"",
        "the wind pressure on the structure bodies (default: the weather case's)",
    ),
    (
        "wind_factor",
        "1.0",
        "the factor on it (default: the transverse factor)",
    ),
];

const LOAD_FILE: &[Field] = &[
    ("format", "\"ohl-load-cases\"", "file format"),
    ("version", "1", "format version"),
    ("name", "\"Load cases\"", "name of the set (e.g. the code)"),
];

const WEATHER_FILE: &[Field] = &[
    ("format", "\"ohl-weather-cases\"", "file format"),
    ("version", "1", "format version"),
    ("name", "\"Weather cases\"", "name of the set"),
];

const INSULATOR: &[Field] = &[
    ("format", "\"ohl-insulator\"", "file format"),
    ("version", "1", "format version"),
    (
        "id",
        "\"new-insulator\"",
        "unique id, used by structure types and strings",
    ),
    ("name", "\"New insulator\"", "display name"),
    ("kind", "\"suspension\"", "suspension or strain"),
    (
        "legs",
        "2",
        "suspension: 2 = two strings to one clamp (needs half_width; version 3)",
    ),
    ("source", "\"\"", "where the values come from"),
    (
        "length",
        "\"4.5 m\"",
        "attachment point to clamp (two strings: each one's length)",
    ),
    ("weight", "\"1500 N\"", "total weight"),
    (
        "axial_stiffness",
        "\"50 MN\"",
        "axial stiffness of a string",
    ),
    ("wind_area", "\"0.5 m²\"", "projected area for wind"),
    (
        "half_width",
        "\"2 m\"",
        "two strings to one wire point: centre to each one's top",
    ),
    (
        "swing_across",
        "\"blocked\"",
        "free (default) or blocked: whether its shapes turn with the string's swing across (version 4)",
    ),
    (
        "swing_along",
        "\"blocked\"",
        "free (default) or blocked: whether its shapes turn with the swing along (version 4)",
    ),
    (
        "twist",
        "\"blocked\"",
        "free (default: across the conductor) or blocked (the structure's axes): its shapes about the string's axis (version 4)",
    ),
    (
        "strings",
        "2",
        "several strings in one assembly, 1 to 4 (needs spacing; version 4)",
    ),
    (
        "arrangement",
        "\"across\"",
        "several strings: across, along or triangle (three)",
    ),
    (
        "spacing",
        "\"0.45 m\"",
        "several strings: between their axes",
    ),
];

const SHAPE: &[Field] = &[
    ("name", "\"clamp\"", "unique on the assembly"),
    ("kind", "\"sphere\"", "sphere, cylinder, box or cone"),
    (
        "state",
        "\"electrified\"",
        "grounded, electrified or insulated (not a clearance target)",
    ),
    (
        "at_fraction",
        "0.5",
        "its centre along the string from its top: 0 the top, 1 the wire point (or at)",
    ),
    (
        "at",
        "\"0.3 m\"",
        "its centre from the string's top (or at_fraction)",
    ),
    (
        "diameter",
        "\"0.2 m\"",
        "sphere, cylinder; a cone's at its end nearer the top",
    ),
    ("end_diameter", "\"0 m\"", "a cone's at its other end"),
    (
        "length",
        "\"0.2 m\"",
        "on the string's axis: cylinder, cone, box",
    ),
    ("width", "\"0.2 m\"", "a box's across"),
    ("depth", "\"0.2 m\"", "a box's along"),
    (
        "offset_across",
        "\"0.25 m\"",
        "its centre off the axis, across",
    ),
    ("offset_along", "\"0 m\"", "its centre off the axis, along"),
    (
        "shared",
        "true",
        "several strings: one for the assembly, on its axis (a yoke)",
    ),
];

const STRUCTURE: &[Field] = &[
    ("format", "\"ohl-structure\"", "file format"),
    ("version", "1", "format version"),
    (
        "id",
        "\"new-type\"",
        "unique id, used by the line's structures",
    ),
    ("name", "\"New type\"", "display name"),
    ("kind", "\"lattice\"", "lattice, pole, portal or other"),
    (
        "height",
        "\"40 m\"",
        "shaft top for the stick drawing (local z)",
    ),
    (
        "foot",
        "\"-30 m\"",
        "level of the feet in local z (default 0: z = 0 is the ground)",
    ),
    ("source", "\"\"", "where the values come from"),
];

/// The parts of a structure type shown in the line application's fields:
/// its points, wire points and strings (and version 1 attachments). The
/// body and the design (joints, members, sections, materials, generators,
/// pole, extensions, load cases…) are edited in the structure application.
const STRUCTURE_BASICS: &[&str] = &["point", "wire", "string", "attachment"];

const ATTACHMENT: &[Field] = &[
    ("set", "1", "set (cross-arm) number"),
    ("phase", "1", "phase number in the set"),
    ("label", "\"\"", "label"),
    (
        "point",
        "[\"0 m\", \"0 m\", \"30 m\"]",
        "the string's top: across (right looking ahead), along (ahead), up",
    ),
    (
        "insulator",
        "\"\"",
        "insulator assembly id (or give a string)",
    ),
    ("side", "\"ahead\"", "strain: ahead or back"),
    (
        "string",
        "{ length = \"4 m\", weight = \"1000 N\" }",
        "or: the string here (length, weight, top, bottom labels)",
    ),
];

const POINT: &[Field] = &[
    (
        "label",
        "\"A1\"",
        "label, unique among points and wire points",
    ),
    (
        "point",
        "[\"0 m\", \"0 m\", \"30 m\"]",
        "position: across (right looking ahead), along (ahead), up",
    ),
    ("node", "\"\"", "or: at this joint"),
    ("member", "\"\"", "or: along this member…"),
    ("at", "0.5", "…at this fraction from its first joint"),
];

const WIRE: &[Field] = &[
    (
        "label",
        "\"W1\"",
        "label, unique among points and wire points",
    ),
    ("set", "1", "a wire is clamped here"),
    (
        "point",
        "[\"0 m\", \"0 m\", \"26 m\"]",
        "position (default: from its strings)",
    ),
    ("weight", "\"0 N\"", "a counterweight hanging at it"),
];

const STRING: &[Field] = &[
    ("label", "\"S1\"", "label"),
    ("top", "\"A1\"", "its top: a point or a wire point"),
    ("wire", "\"W1\"", "its wire point"),
    ("insulator", "\"\"", "insulator assembly id (or a length)"),
    ("length", "\"4 m\"", "length"),
    ("weight", "\"500 N\"", "weight"),
    (
        "compression",
        "\"0 N\"",
        "compression it may take (none: tension only)",
    ),
    (
        "strain",
        "true",
        "a strain string, lined up with its conductor",
    ),
    ("side", "\"ahead\"", "strain: ahead or back"),
];

const NODE: &[Field] = &[
    ("id", "\"N1\"", "joint id"),
    (
        "point",
        "[\"0 m\", \"0 m\", \"0 m\"]",
        "position in local axes",
    ),
    (
        "support",
        "\"pinned\"",
        "fixed, pinned or the directions held",
    ),
];

const MEMBER: &[Field] = &[
    ("id", "\"M1\"", "member id"),
    ("nodes", "[\"N1\", \"N2\"]", "its two joints"),
    ("section", "\"\"", "section id (a full design)"),
    ("kind", "\"truss\"", "truss, frame or cable"),
    ("k", "1.0", "effective length factor"),
];

const MATERIAL: &[Field] = &[
    ("id", "\"steel\"", "material id"),
    ("elasticity", "\"210 GPa\"", "modulus of elasticity"),
    ("density", "\"7850 kg/m3\"", "density"),
    ("yield", "\"355 MPa\"", "yield strength"),
];

const SECTION: &[Field] = &[
    ("id", "\"L90x9\"", "section id"),
    ("material", "\"steel\"", "material id"),
    ("area", "\"15 cm2\"", "area"),
    ("i_min", "\"50 cm4\"", "smallest second moment (buckling)"),
];

const EXTENSION: &[Field] = &[
    ("name", "\"+3\"", "name of the extension"),
    (
        "dz",
        "\"3 m\"",
        "body lengthened below z = 0: everything above the feet rises",
    ),
];

const BODY: &[Field] = &[(
    "points",
    "[[\"0 m\", \"0 m\", \"0 m\"], [\"0 m\", \"0 m\", \"40 m\"]]",
    "polyline in local axes",
)];

impl LibraryKind {
    /// Every kind (the Libraries menu).
    pub const ALL: [LibraryKind; 5] = [
        LibraryKind::Cables,
        LibraryKind::Insulators,
        LibraryKind::StructureTypes,
        LibraryKind::WeatherCases,
        LibraryKind::LoadCases,
    ];

    /// Its name in menus: its title and "library" (once).
    pub fn library_name(self) -> String {
        let t = self.title();
        if t.ends_with("library") {
            t.to_owned()
        } else {
            format!("{t} library")
        }
    }

    /// The examples of this kind shipped with the program (where Browse…
    /// starts in an empty library): `libraries/` and the starter template.
    pub fn shipped_examples(self) -> Option<PathBuf> {
        crate::help::shipped(match self {
            LibraryKind::Cables => "libraries/cables",
            LibraryKind::Insulators => "libraries/insulators",
            LibraryKind::StructureTypes => "examples/starter/structures",
            LibraryKind::WeatherCases => "examples/starter/weather",
            LibraryKind::LoadCases => "examples/starter/loadcases",
        })
    }

    /// Title of its page.
    pub fn title(self) -> &'static str {
        match self {
            LibraryKind::Cables => "Wires",
            LibraryKind::Insulators => "Insulators",
            // All the structures (pylons) the model uses, as a library:
            // each imported or exported as a file.
            LibraryKind::StructureTypes => "Structure library",
            LibraryKind::WeatherCases => "Weather cases",
            LibraryKind::LoadCases => "Load cases",
        }
    }

    /// File extension.
    pub fn extension(self) -> &'static str {
        match self {
            LibraryKind::Cables => ohl_cables::WIRE_FILE_EXTENSION,
            LibraryKind::Insulators => ohl_structures::insulators::INSULATOR_FILE_EXTENSION,
            LibraryKind::StructureTypes => ohl_structures::structures::STRUCTURE_FILE_EXTENSION,
            LibraryKind::WeatherCases => ohl_weather::WEATHER_FILE_EXTENSION,
            LibraryKind::LoadCases => ohl_loads::LOAD_CASE_FILE_EXTENSION,
        }
    }

    /// Several items per file: the array of tables holding them.
    fn items_key(self) -> Option<&'static str> {
        match self {
            LibraryKind::Cables => Some("cable"),
            LibraryKind::WeatherCases | LibraryKind::LoadCases => Some("case"),
            _ => None,
        }
    }

    /// Fields that can be added to a table at `context` (the array key,
    /// or `""` for the file's top level).
    fn fields(self, context: &str) -> &'static [Field] {
        match (self, context) {
            (LibraryKind::Cables, "cable") => CABLE,
            (LibraryKind::Cables, "outer" | "core") => COMPONENT,
            (LibraryKind::WeatherCases, "case") => CASE,
            (LibraryKind::WeatherCases, "") => WEATHER_FILE,
            (LibraryKind::LoadCases, "case") => LOAD_CASE,
            (LibraryKind::LoadCases, "structure") => LOAD_CASE_STRUCTURE,
            (LibraryKind::LoadCases, "") => LOAD_FILE,
            (LibraryKind::Insulators, "") => INSULATOR,
            (LibraryKind::Insulators, "shape") => SHAPE,
            (LibraryKind::StructureTypes, "") => STRUCTURE,
            (LibraryKind::StructureTypes, "attachment") => ATTACHMENT,
            (LibraryKind::StructureTypes, "extension") => EXTENSION,
            (LibraryKind::StructureTypes, "body") => BODY,
            (LibraryKind::StructureTypes, "point") => POINT,
            (LibraryKind::StructureTypes, "wire") => WIRE,
            (LibraryKind::StructureTypes, "string") => STRING,
            (LibraryKind::StructureTypes, "node") => NODE,
            (LibraryKind::StructureTypes, "member") => MEMBER,
            (LibraryKind::StructureTypes, "material") => MATERIAL,
            (LibraryKind::StructureTypes, "section") => SECTION,
            _ => &[],
        }
    }

    /// Arrays of tables a table at `context` can get.
    fn arrays(self, context: &str) -> &'static [&'static str] {
        match (self, context) {
            (LibraryKind::StructureTypes, "") => STRUCTURE_BASICS,
            (LibraryKind::Insulators, "") => &["shape"],
            _ => &[],
        }
    }

    /// Whether the top-level array or table `key` is shown in the fields
    /// (a structure type's body and design are edited in the structure
    /// application, not here).
    fn shows(self, context: &str, key: &str) -> bool {
        self != LibraryKind::StructureTypes
            || !context.is_empty()
            || STRUCTURE_BASICS.contains(&key)
    }

    /// Where its files are, as the model uses them.
    pub fn location(self, sources: &Sources) -> PathBuf {
        match self {
            LibraryKind::Cables => crate::model::library_path(sources),
            LibraryKind::Insulators => crate::model::insulators_dir(sources),
            LibraryKind::StructureTypes => crate::model::structures_dir(sources),
            LibraryKind::WeatherCases => sources
                .weather
                .clone()
                .unwrap_or_else(|| crate::model::model_dir(sources)),
            LibraryKind::LoadCases => sources
                .load_cases
                .clone()
                .unwrap_or_else(|| crate::model::model_dir(sources)),
        }
    }

    /// Its files: the file, or the files of the directory.
    pub fn files(self, sources: &Sources) -> Vec<PathBuf> {
        let at = self.location(sources);
        let dir = if at.is_dir() {
            at.clone()
        } else {
            at.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        let mut out = ohl_core::files::files_with_extension(&dir, self.extension());
        if at.is_file() && !out.contains(&at) {
            out.insert(0, at);
        }
        out
    }

    /// The directory new files go to.
    pub fn new_file_dir(self, sources: &Sources) -> PathBuf {
        let at = self.location(sources);
        if at.is_dir() || at.extension().is_none() {
            at
        } else {
            at.parent().map(Path::to_path_buf).unwrap_or_default()
        }
    }

    /// Points `sources` at `path`: the wire library or the weather cases
    /// file itself; the folder of an insulator or structure type file.
    pub fn use_for(self, sources: &mut Sources, path: &Path) {
        let folder = || path.parent().map(Path::to_path_buf);
        match self {
            LibraryKind::Cables => sources.library = Some(path.to_path_buf()),
            LibraryKind::WeatherCases => sources.weather = Some(path.to_path_buf()),
            LibraryKind::LoadCases => sources.load_cases = Some(path.to_path_buf()),
            LibraryKind::Insulators => sources.insulators = folder(),
            LibraryKind::StructureTypes => sources.structures = folder(),
        }
    }

    /// Whether the model already reads `path`.
    pub fn in_use(self, sources: &Sources, path: &Path) -> bool {
        let at = self.location(sources);
        at == path || (at.is_dir() && path.parent() == Some(at.as_path()))
    }

    /// Checks a file's text with the library's own reader.
    pub fn check(self, path: &Path, text: &str) -> Result<(), String> {
        use ohl_structures::LibraryItem;
        match self {
            LibraryKind::Cables => ohl_cables::Library::default()
                .add_toml(path, text)
                .map_err(|e| e.to_string()),
            LibraryKind::WeatherCases => ohl_weather::WeatherCases::parse(text).map(|_| ()),
            LibraryKind::LoadCases => ohl_loads::LoadCases::parse(text).map(|_| ()),
            LibraryKind::StructureTypes => ohl_structures::StructureType::parse(path, text)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            LibraryKind::Insulators => ohl_structures::InsulatorAssembly::parse(path, text)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        }
    }

    /// A new file's text.
    pub fn template(self, id: &str) -> String {
        match self {
            LibraryKind::Cables => format!(
                "# OpenOHL wire library (.ohlw)\nformat = \"ohl-wire\"\nversion = 1\n\n{}",
                self.item_template(id)
            ),
            LibraryKind::WeatherCases => format!(
                "# OpenOHL weather cases (.ohlwc)\nformat = \"ohl-weather-cases\"\nversion = 1\nname = \"{id}\"\n\n{}",
                self.item_template("everyday")
            ),
            LibraryKind::LoadCases => format!(
                "# OpenOHL load cases (.ohllc)\nformat = \"ohl-load-cases\"\nversion = 1\nname = \"{id}\"\n\n{}",
                self.item_template("everyday")
            ),
            LibraryKind::Insulators => format!(
                "format = \"ohl-insulator\"\nversion = 1\nid = \"{id}\"\nname = \"{id}\"\nkind = \"suspension\"\nlength = \"4.5 m\"\nweight = \"1500 N\"\n"
            ),
            LibraryKind::StructureTypes => format!(
                "format = \"ohl-structure\"\nversion = 1\nid = \"{id}\"\nname = \"{id}\"\nkind = \"lattice\"\n\n[[attachment]]\nset = 1\nphase = 1\nlabel = \"\"\npoint = [\"6 m\", \"0 m\", \"30 m\"]\nstring = {{ length = \"4 m\", weight = \"1000 N\" }}\n"
            ),
        }
    }

    /// A new item of a several-per-file kind.
    fn item_template(self, id: &str) -> String {
        match self {
            LibraryKind::Cables => format!(
                "[[cable]]\nid = \"{id}\"\nname = \"{id}\"\nkind = \"acsr\"\ndiameter = \"28.1 mm\"\nmass_per_length = \"1.63 kg/m\"\nrated_tensile_strength = \"120 kN\"\nouter = {{ area = \"400 mm²\", modulus = \"55 GPa\", thermal_expansion = 23e-6 }}\n"
            ),
            LibraryKind::WeatherCases => {
                format!("[[case]]\nname = \"{id}\"\ntemperature = \"15 °C\"\n")
            }
            LibraryKind::LoadCases => format!(
                "[[case]]\nname = \"{id}\"\nweather = \"everyday\"\ncondition = \"initial\"\nvertical = 1.0\ntransverse = 1.0\nlongitudinal = 1.0\n"
            ),
            _ => String::new(),
        }
    }
}

/// An open library page.
pub struct LibraryPage {
    /// Which library.
    pub kind: LibraryKind,
    /// Its files.
    pub files: Vec<PathBuf>,
    /// The file open.
    pub file: Option<PathBuf>,
    /// Its content, as edited.
    doc: Option<DocumentMut>,
    /// The text as read (unsaved changes when different).
    saved: String,
    /// The item shown (several-per-file kinds).
    item: usize,
    /// Why the file does not read, as edited.
    pub error: Option<String>,
    /// What reads but is worth a look (e.g. curves not starting at zero).
    pub warning: Option<String>,
    /// What happened last (saved, reloaded…).
    pub message: Option<(String, bool)>,
    /// Fields being typed, by path (a number half typed does not parse).
    buffers: HashMap<String, String>,
    /// Structure types: the pylon workspace (instead of the fields).
    pub workspace: crate::type_workspace::Workspace,
    /// A file opened on its own (no model: nothing to use it for).
    pub standalone: bool,
}

/// What the pylon workspace of the structure types page needs from the
/// application.
pub struct PageContext<'a> {
    /// For its 3D view.
    pub rs: Option<&'a eframe::egui_wgpu::RenderState>,
    /// The line its structures are in (none: a file opened alone).
    pub line: Option<&'a ohl_line::Line>,
    /// The insulator assemblies strings may take.
    pub insulators: Option<&'a ohl_structures::InsulatorLibrary>,
    /// The angle that means ahead (the compass).
    pub ahead: f64,
}

/// What the page asks of the application.
pub enum PageAction {
    /// A file was saved: read the libraries again.
    Saved(PathBuf),
    /// Use this file (or, for libraries kept one per file, its folder) for
    /// the model.
    UseForModel(PathBuf),
    /// Edit this structure file in the structure application.
    EditInStructureApp(PathBuf),
}

/// Whether the model reads `kind` from the project (its working copies).
pub fn in_project(sources: &Sources, kind: LibraryKind) -> bool {
    let at = kind.location(sources);
    at.ancestors()
        .any(|a| a.file_name().is_some_and(|n| n == "ohl-design-libraries"))
}

impl LibraryPage {
    /// A page for `kind`, with its first file open.
    pub fn new(kind: LibraryKind, sources: &Sources) -> Self {
        let files = kind.files(sources);
        let mut page = Self {
            kind,
            files,
            file: None,
            doc: None,
            saved: String::new(),
            item: 0,
            error: None,
            warning: None,
            message: None,
            buffers: HashMap::new(),
            workspace: Default::default(),
            standalone: false,
        };
        let first = match kind {
            LibraryKind::WeatherCases => sources.weather.clone(),
            LibraryKind::LoadCases => sources.load_cases.clone(),
            _ => None,
        }
        .or_else(|| page.files.first().cloned());
        if let Some(f) = first {
            page.open(&f);
        }
        page
    }

    /// Opens `path` (unsaved changes are dropped).
    pub fn open(&mut self, path: &Path) {
        self.buffers.clear();
        // A new pylon: nothing picked, the view fitted to it.
        let on = self.workspace.on;
        self.workspace = Default::default();
        self.workspace.on = on;
        self.item = 0;
        self.message = None;
        match std::fs::read_to_string(path) {
            Ok(text) => match text.parse::<DocumentMut>() {
                Ok(mut doc) => {
                    // Points a structure file places along a member (written
                    // before 2026-10-21): on a joint inserted there, as the
                    // structure reads them (shown as a change not saved).
                    if self.kind == LibraryKind::StructureTypes
                        && let Err(e) =
                            ohl_structure_edit::links::insert_joints_on_members(&mut doc)
                    {
                        self.message = Some((e, true));
                    }
                    self.saved = text;
                    self.doc = Some(doc);
                    self.file = Some(path.to_path_buf());
                    self.check();
                }
                Err(e) => {
                    self.doc = None;
                    self.file = Some(path.to_path_buf());
                    self.error = Some(format!("not TOML: {e}"));
                }
            },
            Err(e) => {
                self.doc = None;
                self.error = Some(format!("{}: {e}", path.display()));
            }
        }
    }

    /// The text as edited.
    pub fn text(&self) -> Option<String> {
        self.doc.as_ref().map(DocumentMut::to_string)
    }

    /// Whether there are unsaved changes.
    pub fn dirty(&self) -> bool {
        self.text().is_some_and(|t| t != self.saved)
    }

    /// **Edit in the structure application…** asked (on the page or on its
    /// pylon): the file, as saved; with changes here, why not.
    fn edit_in_app(&mut self) -> Option<PageAction> {
        if self.workspace.request != Some(crate::type_workspace::Request::EditInStructureApp) {
            return None;
        }
        self.workspace.request = None;
        if self.dirty() {
            log::info!("structure application: the page has unsaved changes");
            self.message = Some((
                "Save or discard the changes here first: OHL-Structure edits the \
                 file as saved"
                    .into(),
                true,
            ));
            return None;
        }
        let Some(f) = self.file.clone() else {
            self.message = Some(("no structure file open".into(), true));
            return None;
        };
        Some(PageAction::EditInStructureApp(f))
    }

    fn check(&mut self) {
        self.error = match (&self.file, self.text()) {
            (Some(path), Some(text)) => self.kind.check(path, &text).err(),
            _ => None,
        };
        // Reads, but worth a look.
        self.warning = match (&self.file, self.text(), self.kind) {
            (Some(path), Some(text), LibraryKind::Cables) if self.error.is_none() => {
                let mut lib = ohl_cables::Library::default();
                lib.add_toml(path, &text)
                    .ok()
                    .and_then(|_| ohl_cables::mechanics::curve_offset_warning(&lib))
            }
            _ => None,
        };
    }

    /// Saves the file when it reads.
    pub fn save(&mut self) -> Result<PathBuf, String> {
        self.raise_load_case_version();
        self.raise_wire_version();
        let (Some(path), Some(text)) = (self.file.clone(), self.text()) else {
            return Err("no file open".into());
        };
        self.kind.check(&path, &text)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.saved = text;
        Ok(path)
    }

    /// A load case file giving the structures' own wind (`[case.structure]`)
    /// is written at version 2 (`ohl_loads::LoadCases::written_version`):
    /// its `version` raised when lower.
    fn raise_load_case_version(&mut self) {
        if self.kind != LibraryKind::LoadCases {
            return;
        }
        let Some(text) = self.text() else {
            return;
        };
        let Ok(cases) = ohl_loads::LoadCases::parse(&text) else {
            return;
        };
        let want = i64::from(cases.written_version());
        if let Some(doc) = &mut self.doc
            && doc
                .get("version")
                .and_then(toml_edit::Item::as_integer)
                .is_some_and(|v| v < want)
        {
            doc["version"] = toml_edit::value(want);
        }
    }

    /// A wire library one of whose cables gives its `reference_temperature`
    /// is written at version 2 ([`ohl_cables::WIRE_FILE_VERSION`]): its
    /// `version` raised when lower; without one it keeps its version.
    fn raise_wire_version(&mut self) {
        if self.kind != LibraryKind::Cables {
            return;
        }
        let Some(doc) = &mut self.doc else {
            return;
        };
        let gives = doc
            .get("cable")
            .and_then(toml_edit::Item::as_array_of_tables)
            .is_some_and(|cables| {
                cables
                    .iter()
                    .any(|c| c.contains_key("reference_temperature"))
            });
        if gives
            && doc
                .get("version")
                .and_then(toml_edit::Item::as_integer)
                .is_some_and(|v| v < 2)
        {
            doc["version"] = toml_edit::value(2);
        }
    }

    /// Creates a new file at `path` (anywhere), from the kind's template,
    /// and opens it.
    pub fn create_at(&mut self, sources: &Sources, path: &Path) -> Result<(), String> {
        let path = path.with_extension(self.kind.extension());
        let stem = path
            .file_stem()
            .map_or("new".into(), |s| s.to_string_lossy().into_owned());
        let id: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let text = self.kind.template(&id);
        self.kind.check(&path, &text)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &text).map_err(|e| e.to_string())?;
        self.files = self.kind.files(sources);
        self.add_file(&path);
        Ok(())
    }

    /// A file browsed: in a project, copied into it (the copy, saved, to be
    /// kept in the project); otherwise `None` (opened where it is).
    pub fn import(&mut self, sources: &Sources, path: &Path) -> Result<Option<PathBuf>, String> {
        if !in_project(sources, self.kind) {
            return Ok(None);
        }
        let name = path.file_name().ok_or("no file name")?;
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.kind.check(path, &text)?;
        let to = self.kind.new_file_dir(sources).join(name);
        std::fs::write(&to, &text).map_err(|e| format!("{}: {e}", to.display()))?;
        self.files = self.kind.files(sources);
        self.add_file(&to);
        Ok(Some(to))
    }

    /// Opens a file from anywhere (browsed), listing it with the others.
    pub fn add_file(&mut self, path: &Path) {
        if !self.files.iter().any(|f| f == path) {
            self.files.push(path.to_path_buf());
        }
        self.open(path);
    }

    /// The items of a several-per-file kind: (label, index).
    fn items(&self) -> Vec<String> {
        let Some(key) = self.kind.items_key() else {
            return Vec::new();
        };
        self.doc
            .as_ref()
            .and_then(|d| d.get(key))
            .and_then(Item::as_array_of_tables)
            .map(|a| a.iter().map(label_of).collect())
            .unwrap_or_default()
    }

    /// Adds an item (a copy of the one shown with a new id, or the
    /// template).
    fn add_item(&mut self, copy: bool) {
        let Some(key) = self.kind.items_key() else {
            return;
        };
        let Some(doc) = &mut self.doc else {
            return;
        };
        let taken: Vec<String> = doc
            .get(key)
            .and_then(Item::as_array_of_tables)
            .map(|a| a.iter().map(label_of).collect())
            .unwrap_or_default();
        let id_key = if key == "case" { "name" } else { "id" };
        let mut table = match (copy, doc.get(key).and_then(Item::as_array_of_tables)) {
            (true, Some(a)) if a.get(self.item).is_some() => {
                a.get(self.item).expect("checked").clone()
            }
            _ => self
                .kind
                .item_template("new")
                .parse::<DocumentMut>()
                .ok()
                .and_then(|d| d.get(key)?.as_array_of_tables()?.get(0).cloned())
                .unwrap_or_default(),
        };
        let like = table
            .get(id_key)
            .and_then(Item::as_str)
            .unwrap_or("new")
            .to_owned();
        let id = crate::editors::next_id(taken.iter().map(String::as_str), &like);
        table.insert(id_key, toml_edit::value(id));
        if doc.get(key).and_then(Item::as_array_of_tables).is_none() {
            doc.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
        }
        let a = doc
            .get_mut(key)
            .and_then(Item::as_array_of_tables_mut)
            .expect("inserted");
        a.push(table);
        self.item = a.len() - 1;
        self.buffers.clear();
        self.check();
    }

    fn remove_item(&mut self) {
        let Some(key) = self.kind.items_key() else {
            return;
        };
        if let Some(a) = self
            .doc
            .as_mut()
            .and_then(|d| d.get_mut(key))
            .and_then(Item::as_array_of_tables_mut)
            && self.item < a.len()
        {
            a.remove(self.item);
            self.item = self.item.saturating_sub(1);
        }
        self.buffers.clear();
        self.check();
    }

    /// The page.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        sources: &Sources,
        cx: PageContext<'_>,
    ) -> Option<PageAction> {
        let mut action = None;
        let kind = self.kind;
        ui.horizontal_wrapped(|ui| {
            let name = |p: &Path| {
                p.file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned())
            };
            let mut chosen = self.file.clone();
            ui.add_enabled_ui(!self.dirty(), |ui| {
                egui::ComboBox::from_id_salt(("library-file", kind))
                    .selected_text(chosen.as_deref().map_or("choose a file…".into(), name))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for f in &self.files {
                            ui.selectable_value(&mut chosen, Some(f.clone()), name(f))
                                .on_hover_text(f.display().to_string());
                        }
                    });
            })
            .response
            .on_disabled_hover_text("Save or discard the changes first");
            if chosen != self.file
                && let Some(f) = chosen
            {
                self.open(&f);
            }
            let dirty = self.dirty();
            let can_save = dirty && self.error.is_none();
            if ui
                .add_enabled(can_save, egui::Button::new("Save"))
                .on_disabled_hover_text(if dirty {
                    "The file does not read: see the error"
                } else {
                    "No changes"
                })
                .clicked()
            {
                match self.save() {
                    Ok(p) => action = Some(PageAction::Saved(p)),
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if dirty
                && ui.button("Discard").clicked()
                && let Some(f) = self.file.clone()
            {
                self.open(&f);
            }
            if dirty {
                ui.colored_label(ui.visuals().warn_fg_color, "* unsaved");
            }
            ui.separator();
            let filter = (
                kind.title(),
                ohl_core::files::open_extensions(kind.extension()),
            );
            let start = kind.new_file_dir(sources);
            // An empty library: browsing starts at the examples shipped
            // with the program.
            let browse_from = if self.files.is_empty() {
                kind.shipped_examples().unwrap_or_else(|| start.clone())
            } else {
                start.clone()
            };
            if ui
                .add_enabled(!dirty, egui::Button::new("Browse…"))
                .on_hover_text(format!(
                    "Open a .{} file from anywhere on the computer (an empty library: from \
                     the examples shipped with OpenOHL, invented or typical values)",
                    kind.extension()
                ))
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(filter.0, &filter.1)
                    .set_directory(&browse_from)
                    .pick_file()
            {
                match self.import(sources, &p) {
                    Ok(Some(copy)) => action = Some(PageAction::Saved(copy)),
                    Ok(None) => self.add_file(&p),
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if ui
                .add_enabled(!dirty, egui::Button::new("New file…"))
                .on_hover_text("Create a file (where you choose) from a template")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(filter.0, &filter.1)
                    .set_directory(&start)
                    .set_file_name(format!("new.{}", kind.extension()))
                    .save_file()
            {
                // In a project: the file goes into the project (its name).
                let at = match (in_project(sources, kind), p.file_name()) {
                    (true, Some(name)) => start.join(name),
                    _ => p,
                };
                match self.create_at(sources, &at) {
                    Ok(()) if in_project(sources, kind) => {
                        action = Some(PageAction::Saved(at.with_extension(kind.extension())));
                    }
                    Ok(()) => {}
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if let (Some(f), false) = (self.file.clone(), dirty)
                && ui
                    .button("Export…")
                    .on_hover_text("Write a copy of this file where you choose (to share it)")
                    .clicked()
                && let Some(to) = rfd::FileDialog::new()
                    .add_filter(filter.0, &filter.1)
                    .set_file_name(
                        f.file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                    )
                    .save_file()
            {
                self.message = Some(match std::fs::copy(&f, &to) {
                    Ok(_) => (format!("written to {}", to.display()), false),
                    Err(e) => (format!("{}: {e}", to.display()), true),
                });
            }
            if let Some(f) = &self.file
                && !self.standalone
                && !kind.in_use(sources, f)
                && !dirty
                && ui
                    .button("Use for the model")
                    .on_hover_text(match kind {
                        LibraryKind::Cables => "Read the model's wires from this file",
                        LibraryKind::WeatherCases => "Solve the model in these weather cases",
                        LibraryKind::LoadCases => "Compute the structure loads in these load cases",
                        LibraryKind::Insulators => {
                            "Read the insulator assemblies from this file's folder"
                        }
                        LibraryKind::StructureTypes => {
                            "Read the structure types from this file's folder"
                        }
                    })
                    .clicked()
            {
                action = Some(PageAction::UseForModel(f.clone()));
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "{}: {} — values with their units, e.g. \"28.1 mm\"; the file is checked as you type",
                if self.standalone { "the folder" } else { "the model reads" },
                if in_project(sources, kind) {
                    "the project's own files (kept in the project; Export… to share one)".to_owned()
                } else {
                    kind.location(sources).display().to_string()
                }
            ))
            .small()
            .weak(),
        );
        if let Some(e) = &self.error {
            crate::menu::error_label(ui, egui::RichText::new(e).small());
        }
        if let Some(w) = &self.warning {
            ui.colored_label(ui.visuals().warn_fg_color, egui::RichText::new(w).small());
        }
        if let Some((m, error)) = &self.message {
            let t = egui::RichText::new(m).small();
            if *error {
                crate::menu::error_label(ui, t);
            } else {
                ui.colored_label(crate::menu::ok_color(ui), t);
            }
        }
        ui.separator();
        if self.doc.is_none() {
            if self.files.is_empty() {
                ui.label(format!(
                    "The {} is empty.",
                    kind.library_name().to_lowercase()
                ));
                ui.label(
                    egui::RichText::new(
                        "Browse… for a file (it starts at the examples shipped with OpenOHL: \
                         invented or typical values, not for design), or New file… from a \
                         template.",
                    )
                    .small()
                    .weak(),
                );
            } else {
                ui.label("No file open: choose one above.");
            }
            return action;
        }
        let mut changed = false;
        if kind == LibraryKind::StructureTypes {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.workspace.on, false, "Fields")
                    .on_hover_text("The file's fields: its name, points, wire points and strings");
                ui.selectable_value(&mut self.workspace.on, true, "Structure")
                    .on_hover_text(
                        "The structure in its own axes: tables of its attachment points, \
                         strings, wire points and sets on the left, the structure in 3D on the right",
                    );
                ui.separator();
                let dirty = self.dirty();
                if !self.workspace.on
                    && ui
                        .add_enabled(
                            !dirty,
                            egui::Button::new(crate::type_workspace::EDIT_IN_APP),
                        )
                        .on_hover_text(
                            "Open this structure in OHL-Structure (its own \
                             window: the body and the design); saved there, the line follows. \
                             The type alone, without loads: for a placed structure's loads use \
                             Open in OHL-Structure… on that structure (Properties, Structures \
                             table)",
                        )
                        .on_disabled_hover_text("Save or discard the changes here first")
                        .clicked()
                {
                    self.workspace.request =
                        Some(crate::type_workspace::Request::EditInStructureApp);
                }
            });
            action = action.or(self.edit_in_app());
            if self.workspace.on {
                let id = self.workspace.shown().map(|t| t.id.clone());
                let used_by = cx.line.map(|l| {
                    l.structures
                        .iter()
                        .filter(|s| s.structure_type.is_some() && s.structure_type == id)
                        .count()
                });
                let none = ohl_structures::InsulatorLibrary::default();
                let insulators = cx.insulators.unwrap_or(&none);
                let doc = self.doc.as_mut().expect("checked");
                if self.workspace.ahead != cx.ahead {
                    self.workspace.ahead = cx.ahead;
                    self.workspace.redraw();
                }
                let note = match used_by {
                    None => "opened alone (no line)".to_owned(),
                    Some(0) => "used by no structure of the line".to_owned(),
                    Some(1) => "used by 1 structure of the line (it follows when saved)".to_owned(),
                    Some(n) => {
                        format!("used by {n} structures of the line (they follow when saved)")
                    }
                };
                if self.workspace.ui(ui, doc, cx.rs, insulators, &note) {
                    self.buffers.clear();
                    self.check();
                }
                return action.or(self.edit_in_app());
            }
        }
        match kind.items_key() {
            Some(key) => {
                let items = self.items();
                egui::Panel::left(egui::Id::new(("library-items", kind)))
                    .resizable(true)
                    .default_size(200.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Add").clicked() {
                                self.add_item(false);
                            }
                            if ui.button("Duplicate").clicked() {
                                self.add_item(true);
                            }
                            if ui.button("Delete").clicked() {
                                self.remove_item();
                            }
                        });
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (k, l) in items.iter().enumerate() {
                                if ui.selectable_label(k == self.item, l).clicked() {
                                    self.item = k;
                                    self.buffers.clear();
                                }
                            }
                        });
                        // The file's own fields (name of a set of cases…).
                        if !kind.fields("").is_empty() {
                            ui.separator();
                            let doc = self.doc.as_mut().expect("checked");
                            changed |= table_ui(
                                ui,
                                kind,
                                doc.as_table_mut(),
                                "",
                                "file",
                                &mut self.buffers,
                                true,
                                None,
                            );
                        }
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        let doc = self.doc.as_mut().expect("checked");
                        if let Some(t) = doc
                            .get_mut(key)
                            .and_then(Item::as_array_of_tables_mut)
                            .and_then(|a| a.get_mut(self.item))
                        {
                            changed |= table_ui(
                                ui,
                                kind,
                                t,
                                key,
                                &format!("{key}{}", self.item),
                                &mut self.buffers,
                                false,
                                None,
                            );
                        }
                    });
                });
            }
            None => {
                egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    let doc = self.doc.as_mut().expect("checked");
                    changed |= table_ui(
                        ui,
                        kind,
                        doc.as_table_mut(),
                        "",
                        "",
                        &mut self.buffers,
                        false,
                        None,
                    );
                });
            }
        }
        if changed {
            self.message = None;
            self.check();
        }
        action
    }
}

/// An item's label: its id or name.
fn label_of(t: &Table) -> String {
    ["id", "name"]
        .iter()
        .find_map(|k| t.get(k).and_then(Item::as_str))
        .unwrap_or("(no id)")
        .to_owned()
}

/// A value as typed (no surrounding spaces or comments).
fn repr(v: &Value) -> String {
    let mut c = v.clone();
    c.decor_mut().clear();
    c.to_string()
}

/// The cable models (`model` of a cable): key, name in the list.
pub const CABLE_MODELS: [(&str, &str); 3] = [
    (
        "nonlinear",
        "Nonlinear (polynomials for the initial and creep behaviour of the outer and core materials)",
    ),
    (
        "linear_creep_tension",
        "Linear elastic, permanent stretch due to creep proportional to the creep weather case tension",
    ),
    (
        "linear_creep_temperature",
        "Linear elastic, permanent stretch due to creep given as a temperature increase",
    ),
];

/// Example curves of a core (steel-like, on the total area; the creep
/// curve as the initial one: no creep): to be replaced by the cable's
/// data.
const CORE_CURVES: &str = "{ stress_unit = \"MPa\", strain = \"percent\", basis = \"total\", initial = [0.0, 250.0, 0.0, 0.0, 0.0], creep = [0.0, 250.0, 0.0, 0.0, 0.0], final_modulus = 250.0, reference_temperature = \"20 °C\" }";

/// The bimetallic options (`bimetallic_behavior` of a cable): key (empty:
/// not given, the line's choice), name.
pub const BIMETALLIC_BEHAVIORS: [(&str, &str); 3] = [
    (
        "",
        "The line's choice (Criteria › Bimetallic conductor model)",
    ),
    (
        "bird_cage",
        "Outer strands do not take compression at high temperature (bird cage)",
    ),
    (
        "compression",
        "Outer strands can go into compression at high temperature",
    ),
];

/// A cable's bimetallic option (empty: not given, the line's choice).
fn bimetallic_behavior(table: &Table) -> String {
    table
        .get("bimetallic_behavior")
        .and_then(Item::as_str)
        .unwrap_or("")
        .to_owned()
}

/// A component's curves (inline or a section), and whether it has its
/// creep curve.
fn curves_of(component: Option<&Item>) -> Option<bool> {
    match component? {
        Item::Table(t) => match t.get("stress_strain")? {
            Item::Table(c) => Some(c.contains_key("creep")),
            Item::Value(Value::InlineTable(c)) => Some(c.contains_key("creep")),
            _ => Some(true),
        },
        Item::Value(Value::InlineTable(t)) => match t.get("stress_strain")? {
            Value::InlineTable(c) => Some(c.contains_key("creep")),
            _ => Some(true),
        },
        _ => Some(true),
    }
}

/// A cable's model (default `nonlinear`).
fn cable_model(table: &Table) -> String {
    table
        .get("model")
        .and_then(Item::as_str)
        .unwrap_or("nonlinear")
        .to_owned()
}

/// The fields `model` needs that `table` (a cable) lacks: cable fields
/// by key, and components (`outer`, `core` if there is one) lacking their
/// curves.
fn missing_for_model(table: &Table, model: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let needs: &[&str] = match model {
        "linear_creep_tension" => &["creep_temperature_shift", "creep_shift_tension"],
        "linear_creep_temperature" => &["creep_temperature_shift"],
        _ => &[],
    };
    out.extend(
        needs
            .iter()
            .filter(|k| !table.contains_key(k))
            .map(|k| (*k).to_owned()),
    );
    if model == "nonlinear" {
        for c in ["outer", "core"] {
            let Some(component) = table.get(c) else {
                if c == "outer" {
                    out.push("outer.stress_strain".into());
                }
                continue;
            };
            match curves_of(Some(component)) {
                None => out.push(format!("{c}.stress_strain")),
                Some(false) => out.push(format!("{c}.stress_strain.creep")),
                Some(true) => {}
            }
        }
    }
    if table.contains_key("core")
        && bimetallic_behavior(table) == "compression"
        && !table.contains_key("max_virtual_compression")
    {
        out.push("max_virtual_compression".into());
    }
    out
}

/// Adds what `model` needs to a cable table, with the example values:
/// the creep shift (and tension), or the components' curves.
fn add_for_model(table: &mut Table, model: &str) {
    let example = |key: &str, fields: &[Field]| {
        fields
            .iter()
            .find(|f| f.0 == key)
            .and_then(|f| f.1.parse::<Value>().ok())
    };
    for k in missing_for_model(table, model) {
        match k.split_once('.') {
            None => {
                if let Some(v) = example(&k, CABLE) {
                    table.insert(&k, Item::Value(v));
                }
            }
            Some((component, "stress_strain.creep")) => {
                // The creep curve as the initial one (no creep) to start.
                let curves = match table.get_mut(component) {
                    Some(Item::Table(t)) => t.get_mut("stress_strain"),
                    Some(Item::Value(Value::InlineTable(t))) => {
                        if let Some(Value::InlineTable(c)) = t.get_mut("stress_strain")
                            && let Some(initial) = c.get("initial").cloned()
                        {
                            c.insert("creep", initial);
                        }
                        continue;
                    }
                    _ => None,
                };
                match curves {
                    Some(Item::Table(c)) => {
                        if let Some(initial) = c.get("initial").cloned() {
                            c.insert("creep", initial);
                        }
                    }
                    Some(Item::Value(Value::InlineTable(c))) => {
                        if let Some(initial) = c.get("initial").cloned() {
                            c.insert("creep", initial);
                        }
                    }
                    _ => {}
                }
            }
            Some((component, field)) => {
                // A core: steel-like example curves (no creep).
                let v = if component == "core" && field == "stress_strain" {
                    CORE_CURVES.parse::<Value>().ok()
                } else {
                    example(field, COMPONENT)
                };
                let Some(v) = v else {
                    continue;
                };
                match table.get_mut(component) {
                    Some(Item::Table(t)) => {
                        t.insert(field, Item::Value(v));
                    }
                    Some(Item::Value(Value::InlineTable(t))) => {
                        t.insert(field, v);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// The fields of a table that cannot be removed: what the reader needs,
/// and what the cable's model needs (`parent_model`: the cable's, for its
/// components).
fn required(
    kind: LibraryKind,
    context: &str,
    table: &Table,
    parent_model: Option<&str>,
) -> Vec<String> {
    let base: &[&str] = match (kind, context) {
        (LibraryKind::Cables, "cable") => &[
            "id",
            "name",
            "kind",
            "diameter",
            "mass_per_length",
            "rated_tensile_strength",
            "outer",
        ],
        (LibraryKind::Cables, "outer" | "core") => &["area", "modulus", "thermal_expansion"],
        (LibraryKind::Cables, "stress_strain") => &["stress_unit", "initial", "final_modulus"],
        (LibraryKind::WeatherCases, "case") => &["name", "temperature"],
        (LibraryKind::Insulators | LibraryKind::StructureTypes, "") => &["format", "version", "id"],
        _ => &[],
    };
    let mut out: Vec<String> = base.iter().map(|k| (*k).to_owned()).collect();
    match (kind, context) {
        (LibraryKind::Cables, "cable") => {
            // The core goes with the Bimetallic tick.
            if table.contains_key("core") {
                out.push("core".into());
            }
            out.extend(
                [
                    "creep_temperature_shift",
                    "creep_shift_tension",
                    "max_virtual_compression",
                ]
                .into_iter()
                .filter(|k| used_by_cable(k, table))
                .map(str::to_owned),
            );
        }
        (LibraryKind::Cables, "outer" | "core") if parent_model == Some("nonlinear") => {
            out.push("stress_strain".into());
        }
        (LibraryKind::Cables, "stress_strain") if parent_model == Some("nonlinear") => {
            out.push("creep".into());
        }
        _ => {}
    }
    out
}

/// Whether a cable field is used by `model`.
fn used_by_model(key: &str, model: &str) -> bool {
    match key {
        "creep_temperature_shift" => model != "nonlinear",
        "creep_shift_tension" => model == "linear_creep_tension",
        _ => true,
    }
}

/// Whether a cable field is used, with its model and bimetallic options
/// (unused ones are hidden; kept in the file).
fn used_by_cable(key: &str, table: &Table) -> bool {
    match key {
        "bimetallic_behavior" => table.contains_key("core"),
        "max_virtual_compression" => {
            table.contains_key("core") && bimetallic_behavior(table) == "compression"
        }
        _ => used_by_model(key, &cable_model(table)),
    }
}

/// The model row of a cable: a list of the three models; choosing one
/// adds the fields it needs. True when changed.
fn cable_model_ui(ui: &mut egui::Ui, table: &mut Table, path: &str) -> bool {
    let model = cable_model(table);
    let mut chosen = model.clone();
    let mut changed = false;
    ui.label("model")
        .on_hover_text("How sag and tension are modelled; the fields it needs are added");
    egui::ComboBox::from_id_salt(("cable-model", path))
        .selected_text(
            CABLE_MODELS
                .iter()
                .find(|m| m.0 == model)
                .map_or(model.as_str(), |m| m.1),
        )
        .width(280.0)
        .show_ui(ui, |ui| {
            for (key, name) in CABLE_MODELS {
                ui.selectable_value(&mut chosen, key.to_owned(), name);
            }
        });
    if chosen != model {
        table.insert("model", toml_edit::value(chosen.clone()));
        add_for_model(table, &chosen);
        changed = true;
    }
    ui.label("");
    ui.end_row();
    // Bimetallic: a core of another material (its own curves).
    let mut bimetallic = table.contains_key("core");
    ui.label("bimetallic")
        .on_hover_text("Different materials for the outer strands and the core");
    if ui
        .checkbox(
            &mut bimetallic,
            "Bimetallic conductor (outer strands and a core)",
        )
        .on_hover_text(
            "Ticked: a core with its own area, modulus, expansion and (non-linear model) \
             curves; unticked: the core is removed (Discard brings it back before saving)",
        )
        .changed()
    {
        if bimetallic {
            if let Some(v) = CABLE
                .iter()
                .find(|f| f.0 == "core")
                .and_then(|f| f.1.parse::<Value>().ok())
            {
                table.insert("core", Item::Value(v));
            }
            table.insert("bimetallic_behavior", toml_edit::value("compression"));
            add_for_model(table, &chosen);
        } else {
            for k in ["core", "bimetallic_behavior", "max_virtual_compression"] {
                table.remove(k);
            }
        }
        changed = true;
    }
    ui.label("");
    ui.end_row();
    // Bimetallic: what the outer strands do above the transition
    // temperature.
    if bimetallic {
        let behavior = bimetallic_behavior(table);
        let mut chosen_behavior = behavior.clone();
        ui.label("above the transition").on_hover_text(
            "The outer strands (e.g. aluminium) expand more than the core (e.g. steel): \
                 above a transition temperature they are no longer in tension",
        );
        ui.vertical(|ui| {
            for (key, name) in BIMETALLIC_BEHAVIORS {
                ui.radio_value(&mut chosen_behavior, key.to_owned(), name);
            }
            ui.label(
                egui::RichText::new(
                    "Virtual stress = actual stress × Ao / At (Ao outer strands' area, At total \
                     area)",
                )
                .small()
                .weak(),
            );
        });
        if chosen_behavior != behavior {
            if chosen_behavior.is_empty() {
                table.remove("bimetallic_behavior");
            } else {
                table.insert("bimetallic_behavior", toml_edit::value(chosen_behavior));
            }
            add_for_model(table, &chosen);
            changed = true;
        }
        ui.label("");
        ui.end_row();
    }
    let missing = missing_for_model(table, &chosen);
    if !missing.is_empty() {
        ui.label("");
        ui.horizontal(|ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("this model needs: {}", missing.join(", ")),
            );
            if ui.small_button("Add them").clicked() {
                add_for_model(table, &chosen);
                changed = true;
            }
        });
        ui.label("");
        ui.end_row();
    }
    changed
}

/// Edits one value; true when changed.
fn value_ui(
    ui: &mut egui::Ui,
    v: &mut Value,
    path: &str,
    buffers: &mut HashMap<String, String>,
) -> bool {
    match v {
        Value::String(s) => {
            let mut text = s.value().clone();
            let r = ui.add_sized([280.0, 20.0], egui::TextEdit::singleline(&mut text));
            if r.changed() {
                let decor = v.decor().clone();
                *v = Value::from(text);
                *v.decor_mut() = decor;
                return true;
            }
            false
        }
        Value::InlineTable(t) => {
            let mut changed = false;
            let keys: Vec<String> = t.iter().map(|(k, _)| k.to_owned()).collect();
            ui.vertical(|ui| {
                for k in keys {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [110.0, 20.0],
                            egui::Label::new(egui::RichText::new(&k).small()),
                        );
                        if let Some(inner) = t.get_mut(&k) {
                            changed |= value_ui(ui, inner, &format!("{path}.{k}"), buffers);
                        }
                    });
                }
            });
            changed
        }
        _ => {
            let buffer = buffers.entry(path.to_owned()).or_insert_with(|| repr(v));
            let parsed = buffer.parse::<Value>();
            let mut edit = egui::TextEdit::singleline(buffer);
            if parsed.is_err() {
                edit = edit.text_color(ui.visuals().error_fg_color);
            }
            let r = ui.add_sized([280.0, 20.0], edit);
            let mut changed = false;
            if r.changed()
                && let Ok(new) = buffer.parse::<Value>()
            {
                let decor = v.decor().clone();
                *v = new;
                *v.decor_mut() = decor;
                changed = true;
            }
            if r.lost_focus() {
                buffers.remove(path);
            }
            changed
        }
    }
}

/// Edits a table: its values (with the fields that can be added), then its
/// arrays of tables; true when changed. `context`: the key of the array it
/// is in (`""`: top level). `only_known`: show only the context's fields
/// (the file's own fields next to the items).
#[allow(clippy::too_many_arguments)]
fn table_ui(
    ui: &mut egui::Ui,
    kind: LibraryKind,
    table: &mut Table,
    context: &str,
    path: &str,
    buffers: &mut HashMap<String, String>,
    only_known: bool,
    parent_model: Option<&str>,
) -> bool {
    let fields = kind.fields(context);
    let locked = required(kind, context, table, parent_model);
    let help = |k: &str| fields.iter().find(|f| f.0 == k).map(|f| f.2);
    let mut changed = false;
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
    let mut remove: Option<String> = None;
    // A cable: its model as a list (first), the fields it needs added.
    let cable = kind == LibraryKind::Cables && context == "cable";
    egui::Grid::new(("table", path))
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            if cable {
                changed |= cable_model_ui(ui, table, path);
            }
            let model = cable.then(|| cable_model(table));
            for k in &keys {
                if only_known && help(k).is_none() {
                    continue;
                }
                // The model and bimetallic options are drawn above; fields
                // they do not use are hidden (kept in the file).
                if cable && (k == "model" || k == "bimetallic_behavior" || !used_by_cable(k, table))
                {
                    continue;
                }
                let Some(Item::Value(v)) = table.get_mut(k) else {
                    continue;
                };
                let r = ui.label(k);
                if let Some(h) = help(k) {
                    r.on_hover_text(h);
                }
                changed |= value_ui(ui, v, &format!("{path}.{k}"), buffers);
                let needed = locked.contains(k);
                if ui
                    .add_enabled(!needed, egui::Button::new("🗑").small())
                    .on_hover_text("Remove this field")
                    .on_disabled_hover_text(match (&model, k.as_str()) {
                        (_, "max_virtual_compression") => {
                            "needed: the outer strands can go into compression".to_owned()
                        }
                        (Some(m), _) if !used_by_model(k, "nonlinear") => {
                            format!("needed by the {m} model")
                        }
                        _ => "needed: it cannot be removed".to_owned(),
                    })
                    .clicked()
                {
                    remove = Some(k.clone());
                }
                ui.end_row();
            }
        });
    // Sub-tables written as sections (`[cable.outer]`, its curves…).
    let model_here = if kind == LibraryKind::Cables && context == "cable" {
        Some(cable_model(table))
    } else {
        parent_model.map(str::to_owned)
    };
    let subs: Vec<String> = keys
        .iter()
        .filter(|k| matches!(table.get(k), Some(Item::Table(_))) && kind.shows(context, k))
        .cloned()
        .collect();
    for k in subs {
        if let Some(Item::Table(sub)) = table.get_mut(&k) {
            egui::CollapsingHeader::new(&k)
                .id_salt(("sub", path, &k))
                .default_open(true)
                .show(ui, |ui| {
                    changed |= table_ui(
                        ui,
                        kind,
                        sub,
                        &k,
                        &format!("{path}.{k}"),
                        buffers,
                        false,
                        model_here.as_deref(),
                    );
                });
        }
    }
    if let Some(k) = remove {
        table.remove(&k);
        buffers.retain(|p, _| !p.starts_with(&format!("{path}.{k}")));
        changed = true;
    }
    let missing: Vec<&Field> = fields
        .iter()
        .filter(|f| {
            !table.contains_key(f.0)
                && !(cable
                    && (["model", "core", "bimetallic_behavior"].contains(&f.0)
                        || !used_by_cable(f.0, table)))
        })
        .collect();
    if !missing.is_empty() {
        let mut add: Option<&Field> = None;
        egui::ComboBox::from_id_salt(("add-field", path))
            .selected_text("add a field…")
            .show_ui(ui, |ui| {
                for f in &missing {
                    if ui.selectable_label(false, f.0).on_hover_text(f.2).clicked() {
                        add = Some(f);
                    }
                }
            });
        if let Some(f) = add
            && let Ok(v) = f.1.parse::<Value>()
        {
            table.insert(f.0, Item::Value(v));
            changed = true;
        }
    }
    if only_known {
        return changed;
    }
    // Arrays of tables (a structure type's points, strings, joints, …).
    let mut arrays: Vec<String> = keys
        .iter()
        .filter(|k| matches!(table.get(k), Some(Item::ArrayOfTables(_))) && kind.shows(context, k))
        .cloned()
        .collect();
    let hidden: Vec<&String> = keys
        .iter()
        .filter(|k| !table.get(k).is_some_and(Item::is_value) && !kind.shows(context, k))
        .collect();
    if !hidden.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "Also in the file, edited in OHL-Structure ({}): {}",
                crate::type_workspace::EDIT_IN_APP,
                hidden
                    .iter()
                    .map(|k| k.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .small()
            .weak(),
        );
    }
    for a in kind.arrays(context) {
        if !arrays.iter().any(|x| x == a) {
            arrays.push((*a).to_owned());
        }
    }
    for key in arrays {
        let n = table
            .get(&key)
            .and_then(Item::as_array_of_tables)
            .map_or(0, ArrayOfTables::len);
        egui::CollapsingHeader::new(format!("{key} ({n})"))
            .id_salt(("array", path, &key))
            .show(ui, |ui| {
                let mut remove: Option<usize> = None;
                let mut copy: Option<usize> = None;
                if let Some(a) = table.get_mut(&key).and_then(Item::as_array_of_tables_mut) {
                    for (i, t) in a.iter_mut().enumerate() {
                        let title = match (
                            t.get("set").and_then(Item::as_integer),
                            t.get("phase").and_then(Item::as_integer),
                        ) {
                            (Some(s), Some(p)) => format!(
                                "{key} {}: {s}:{p} {}",
                                i + 1,
                                t.get("label").and_then(Item::as_str).unwrap_or("")
                            ),
                            _ => format!(
                                "{key} {}: {}",
                                i + 1,
                                ["name", "label", "id"]
                                    .into_iter()
                                    .find_map(|k| t.get(k).and_then(Item::as_str))
                                    .unwrap_or("")
                            ),
                        };
                        egui::CollapsingHeader::new(title)
                            .id_salt(("entry", path, &key, i))
                            .show(ui, |ui| {
                                changed |= table_ui(
                                    ui,
                                    kind,
                                    t,
                                    &key,
                                    &format!("{path}.{key}{i}"),
                                    buffers,
                                    false,
                                    None,
                                );
                                ui.horizontal(|ui| {
                                    if ui.small_button("Duplicate").clicked() {
                                        copy = Some(i);
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                    }
                }
                if ui.small_button(format!("Add {key}")).clicked() {
                    let mut t = Table::new();
                    for f in kind.fields(&key).iter().take(4) {
                        if let Ok(v) = f.1.parse::<Value>() {
                            t.insert(f.0, Item::Value(v));
                        }
                    }
                    if table.get(&key).and_then(Item::as_array_of_tables).is_none() {
                        table.insert(&key, Item::ArrayOfTables(ArrayOfTables::new()));
                    }
                    if let Some(a) = table.get_mut(&key).and_then(Item::as_array_of_tables_mut) {
                        a.push(t);
                    }
                    changed = true;
                }
                if let Some(a) = table.get_mut(&key).and_then(Item::as_array_of_tables_mut) {
                    if let Some(i) = copy
                        && let Some(t) = a.get(i).cloned()
                    {
                        a.push(t);
                        changed = true;
                    }
                    if let Some(i) = remove {
                        a.remove(i);
                        buffers.retain(|p, _| !p.starts_with(&format!("{path}.{key}")));
                        changed = true;
                    }
                }
            });
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ohl-library-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn new_files_of_every_kind_read() {
        for k in LibraryKind::ALL {
            let path = PathBuf::from(format!("x.{}", k.extension()));
            k.check(&path, &k.template("new-one"))
                .unwrap_or_else(|e| panic!("{k:?}: {e}"));
        }
    }

    /// A wire library one of whose cables gives the data sheet's
    /// `reference_temperature` (owner decision 2026-10-21) is saved at
    /// version 2; without it the file keeps its version 1.
    #[test]
    fn a_cable_with_its_reference_temperature_is_saved_at_version_2() {
        let dir = temp("wires");
        let path = dir.join("wires.ohlw");
        std::fs::copy(root().join("libraries/cables/acsr-examples.ohlw"), &path).unwrap();
        let sources = Sources {
            file: dir.join("line.ohl.json"),
            library: Some(path.clone()),
            ..Sources::default()
        };
        let mut page = LibraryPage::new(LibraryKind::Cables, &sources);
        assert!(page.error.is_none(), "{:?}", page.error);
        page.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("version = 1\n"), "{text}");
        let doc = page.doc.as_mut().unwrap();
        doc["cable"]
            .as_array_of_tables_mut()
            .unwrap()
            .get_mut(0)
            .unwrap()["reference_temperature"] = toml_edit::value("15 °C");
        page.check();
        assert!(page.error.is_none(), "{:?}", page.error);
        page.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("version = 2\n"), "{text}");
        let mut lib = ohl_cables::Library::default();
        lib.add_toml(&path, &text).unwrap();
        let first = lib
            .iter()
            .find(|c| c.reference_temperature.is_some())
            .unwrap();
        assert!((first.mass_reference_temperature().celsius() - 15.0).abs() < 1e-12);
    }

    /// A load case given the structures' own wind (`[case.structure]`) is
    /// saved at version 2; without it the file keeps version 1.
    #[test]
    fn a_load_case_with_the_structures_wind_is_saved_at_version_2() {
        let dir = temp("loadcases");
        let path = dir.join("cases.ohllc");
        std::fs::copy(
            root().join("examples/synthetic-line/load-cases.ohllc"),
            &path,
        )
        .unwrap();
        let sources = Sources {
            file: dir.join("line.ohl.json"),
            load_cases: Some(path.clone()),
            ..Sources::default()
        };
        let mut page = LibraryPage::new(LibraryKind::LoadCases, &sources);
        assert!(page.error.is_none(), "{:?}", page.error);
        page.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("version = 1\n"), "{text}");
        let doc = page.doc.as_mut().unwrap();
        let wind: toml_edit::DocumentMut =
            "structure = { wind_pressure = \"900 Pa\", wind_factor = 1.2 }\n"
                .parse()
                .unwrap();
        doc["case"]
            .as_array_of_tables_mut()
            .unwrap()
            .get_mut(1)
            .unwrap()["structure"] = wind["structure"].clone();
        page.check();
        assert!(page.error.is_none(), "{:?}", page.error);
        page.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("version = 2\n"), "{text}");
        let read = ohl_loads::LoadCases::parse(&text).unwrap();
        assert_eq!(read.cases[1].structure.unwrap().wind_pressure, Some(900.0));
        assert!(LibraryKind::LoadCases.fields("structure").len() == 2);
    }

    /// A weather case changed keeps the file's comments; a value that does
    /// not read blocks the save.
    #[test]
    fn edits_keep_comments_and_are_checked() {
        let dir = temp("weather");
        let path = dir.join("cases.ohlwc");
        std::fs::copy(
            root().join("examples/synthetic-line/weather-cases.ohlwc"),
            &path,
        )
        .unwrap();
        let sources = Sources {
            file: dir.join("line.ohl.json"),
            weather: Some(path.clone()),
            ..Sources::default()
        };
        let mut page = LibraryPage::new(LibraryKind::WeatherCases, &sources);
        assert_eq!(page.file.as_deref(), Some(path.as_path()));
        assert!(page.error.is_none(), "{:?}", page.error);
        assert!(page.items().len() >= 4);
        let set = |page: &mut LibraryPage, v: &str| {
            let doc = page.doc.as_mut().unwrap();
            doc["case"]
                .as_array_of_tables_mut()
                .unwrap()
                .get_mut(0)
                .unwrap()["temperature"] = toml_edit::value(v);
            page.check();
        };
        set(&mut page, "hot");
        assert!(page.error.is_some());
        assert!(page.save().is_err());
        set(&mut page, "12 °C");
        assert!(page.error.is_none(), "{:?}", page.error);
        assert!(page.dirty());
        page.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# Example weather cases"));
        assert!(text.contains("temperature = \"12 °C\""));
        assert!(!page.dirty());
    }

    /// A new wire file, with a second wire next to the first.
    #[test]
    fn new_files_and_items() {
        let dir = temp("wires");
        let sources = Sources {
            file: dir.join("line.ohl.json"),
            library: Some(dir.clone()),
            ..Sources::default()
        };
        let mut page = LibraryPage::new(LibraryKind::Cables, &sources);
        assert!(page.files.is_empty());
        page.create_at(&sources, &dir.join("my-wires.ohlw"))
            .unwrap();
        assert_eq!(page.files, vec![dir.join("my-wires.ohlw")]);
        page.add_item(true);
        assert_eq!(page.items(), vec!["my-wires", "my-wires-1"]);
        assert!(page.error.is_none(), "{:?}", page.error);
        page.save().unwrap();
        let lib = ohl_cables::Library::load(&dir).unwrap();
        assert!(lib.get("my-wires-1").is_some());
        // Browsed from elsewhere: listed and opened; used for the model.
        let other = temp("wires-elsewhere").join("theirs.ohlw");
        std::fs::write(&other, LibraryKind::Cables.template("theirs")).unwrap();
        page.add_file(&other);
        assert_eq!(page.file.as_deref(), Some(other.as_path()));
        assert!(page.files.contains(&other));
        let mut s = sources.clone();
        assert!(!LibraryKind::Cables.in_use(&s, &other));
        LibraryKind::Cables.use_for(&mut s, &other);
        assert!(LibraryKind::Cables.in_use(&s, &other));
    }

    /// The cable models add the fields they need; those, and what the
    /// reader needs, cannot be removed; the bimetallic tick adds a core;
    /// the cable still reads.
    #[test]
    fn cable_models_add_and_lock_their_fields() {
        let kind = LibraryKind::Cables;
        let mut doc: DocumentMut = kind.template("c1").parse().unwrap();
        let cable = doc
            .get_mut("cable")
            .and_then(Item::as_array_of_tables_mut)
            .and_then(|a| a.get_mut(0))
            .unwrap();
        assert_eq!(cable_model(cable), "nonlinear");
        // Linear, creep ∝ tension: its shift and tension, locked.
        cable.insert("model", toml_edit::value("linear_creep_tension"));
        add_for_model(cable, "linear_creep_tension");
        assert!(missing_for_model(cable, "linear_creep_tension").is_empty());
        let locked = required(kind, "cable", cable, None);
        for k in [
            "creep_temperature_shift",
            "creep_shift_tension",
            "id",
            "outer",
        ] {
            assert!(locked.iter().any(|x| x == k), "{k} locked");
        }
        kind.check(Path::new("x.ohlw"), &doc.to_string()).unwrap();
        // Non-linear: the outer curves; with a core, its own.
        let cable = doc
            .get_mut("cable")
            .and_then(Item::as_array_of_tables_mut)
            .and_then(|a| a.get_mut(0))
            .unwrap();
        cable.insert("model", toml_edit::value("nonlinear"));
        cable.insert(
            "core",
            Item::Value(
                "{ area = \"50 mm²\", modulus = \"190 GPa\", thermal_expansion = 11.5e-6 }"
                    .parse()
                    .unwrap(),
            ),
        );
        // Bird cage: no largest compressive stress to give.
        cable.insert("bimetallic_behavior", toml_edit::value("bird_cage"));
        assert_eq!(
            missing_for_model(cable, "nonlinear"),
            vec!["outer.stress_strain", "core.stress_strain"]
        );
        add_for_model(cable, "nonlinear");
        assert!(missing_for_model(cable, "nonlinear").is_empty());
        let locked = required(kind, "cable", cable, None);
        assert!(
            locked.iter().any(|x| x == "core"),
            "goes with the tick only"
        );
        assert!(!locked.iter().any(|x| x == "creep_shift_tension"));
        let outer = Table::new();
        assert!(
            required(kind, "outer", &outer, Some("nonlinear")).contains(&"stress_strain".into())
        );
        assert!(
            !required(kind, "outer", &outer, Some("linear_creep_temperature"))
                .contains(&"stress_strain".into())
        );
        // The curves' creep curve cannot be removed (non-linear model).
        assert!(
            required(kind, "stress_strain", &Table::new(), Some("nonlinear"))
                .contains(&"creep".into())
        );
        // A core without its creep curve: it is asked for, the initial
        // curve to start.
        let cable = doc
            .get_mut("cable")
            .and_then(Item::as_array_of_tables_mut)
            .and_then(|a| a.get_mut(0))
            .unwrap();
        if let Some(Item::Value(Value::InlineTable(core))) = cable.get_mut("core")
            && let Some(Value::InlineTable(c)) = core.get_mut("stress_strain")
        {
            c.remove("creep");
        }
        assert_eq!(
            missing_for_model(cable, "nonlinear"),
            vec!["core.stress_strain.creep"]
        );
        add_for_model(cable, "nonlinear");
        assert!(missing_for_model(cable, "nonlinear").is_empty());
        // Hidden fields: the creep shifts with the non-linear model; the
        // shift tension with the temperature model.
        assert!(!used_by_cable("creep_temperature_shift", cable));
        cable.insert("model", toml_edit::value("linear_creep_temperature"));
        assert!(used_by_cable("creep_temperature_shift", cable));
        assert!(!used_by_cable("creep_shift_tension", cable));
        cable.insert("model", toml_edit::value("nonlinear"));
        // Bimetallic, compression: its largest stress, locked.
        assert!(!used_by_cable("max_virtual_compression", cable));
        assert!(!required(kind, "cable", cable, None).contains(&"max_virtual_compression".into()));
        cable.insert("bimetallic_behavior", toml_edit::value("compression"));
        assert_eq!(
            missing_for_model(cable, "nonlinear"),
            vec!["max_virtual_compression"]
        );
        add_for_model(cable, "nonlinear");
        assert!(required(kind, "cable", cable, None).contains(&"max_virtual_compression".into()));
        let text = doc.to_string();
        kind.check(Path::new("x.ohlw"), &text).unwrap();
        let mut lib = ohl_cables::Library::default();
        lib.add_toml(Path::new("x.ohlw"), &text).unwrap();
        let c = lib.iter().next().unwrap();
        assert!(
            c.outer.stress_strain.is_some() && c.core.as_ref().unwrap().stress_strain.is_some()
        );
        let core_curves = c.core.as_ref().unwrap().stress_strain.as_ref().unwrap();
        assert_eq!(core_curves.creep.as_ref(), Some(&core_curves.initial));
        assert_eq!(
            c.bimetallic_behavior,
            Some(ohl_cables::BimetallicBehavior::Compression)
        );
        assert!(c.max_virtual_compression.is_some());
    }
}
