// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Whole projects from other programs' files (`project-plugin` world): a
//! line, its library files (wires, weather cases, load cases…) and notes
//! on what was read, left out or assumed.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ohl_line::Line;
use ohl_plugins::{WasmHost, WasmProjectPlugin};

use crate::{Options, Registry};

/// A project read from another program's file.
#[derive(Debug, Clone)]
pub struct ImportedProject {
    /// The line (checked).
    pub line: Line,
    /// Library files: kind (`cables`, `weather`, `loadcases`,
    /// `structures`, `insulators`, `rules`, `galloping`, `ambient`,
    /// `costs`, `poles`, `femodels`, `sections`), file name, text.
    pub libraries: Vec<(String, String, String)>,
    /// What was read, left out or assumed.
    pub notes: Vec<String>,
}

impl ImportedProject {
    /// Clips in every section in the blocks: each span keeps the
    /// unstressed length of its stringing condition (as programs that sag
    /// a section and then solve it with fixed lengths do). The cables come
    /// from the project's wire library files. Returns how many sections
    /// were clipped in, and those that could not be (with why).
    pub fn clip_in(&mut self) -> (usize, Vec<String>) {
        let mut library = ohl_cables::Library::default();
        let mut failed = Vec::new();
        for (kind, name, text) in &self.libraries {
            if kind == "cables"
                && let Err(e) = library.add_toml(std::path::Path::new(name), text)
            {
                failed.push(format!("{name}: {e}"));
            }
        }
        let mut done = 0;
        let line = self.line.clone();
        for s in &mut self.line.sections {
            if s.stringing.clipped() {
                continue;
            }
            let Some(cable) = library.get(&s.cable) else {
                failed.push(format!(
                    "{}: wire type {} not in the wire library",
                    s.id, s.cable
                ));
                continue;
            };
            match ohl_line::solve::clip_lengths(&line, s, cable) {
                Ok((lengths, temperature)) => {
                    s.stringing = ohl_line::Stringing::UnstressedLengths {
                        lengths,
                        temperature,
                        sagged: Some(Box::new(s.stringing.clone())),
                    };
                    done += 1;
                }
                Err(e) => failed.push(format!("{}: {e}", s.id)),
            }
        }
        (done, failed)
    }
}

/// The library files that may be added to an import (owner, 2026-10-27):
/// kind in the project and file extension. The structure loads
/// (`*.loads.csv`) are not among them.
pub const LIBRARY_EXTENSIONS: [(&str, &str); 12] = [
    ("cables", ohl_cables::WIRE_FILE_EXTENSION),
    (
        "structures",
        ohl_structures::structures::STRUCTURE_FILE_EXTENSION,
    ),
    (
        "insulators",
        ohl_structures::insulators::INSULATOR_FILE_EXTENSION,
    ),
    ("weather", ohl_weather::WEATHER_FILE_EXTENSION),
    ("loadcases", ohl_loads::LOAD_CASE_FILE_EXTENSION),
    ("rules", ohl_clearance::rules::RULE_FILE_EXTENSION),
    (
        "galloping",
        ohl_clearance::galloping::GALLOPING_FILE_EXTENSION,
    ),
    ("ambient", "ohlta"),
    ("costs", "ohlcost"),
    ("poles", "ohlp"),
    ("femodels", "ohlfe"),
    ("sections", ohl_sections::SECTION_FILE_EXTENSION),
];

/// The library kind of a file, from its extension (`None`: not a library
/// file that may be added).
pub fn library_kind(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    LIBRARY_EXTENSIONS
        .iter()
        .find(|(_, e)| e.eq_ignore_ascii_case(ext))
        .map(|(k, _)| *k)
}

/// Reads library files to add to an import: kind (from the extension),
/// file name, text. Refused for a file of no library kind or one that
/// cannot be read.
pub fn read_library_files(paths: &[PathBuf]) -> Result<Vec<(String, String, String)>, String> {
    paths
        .iter()
        .map(|p| {
            let kind = library_kind(p).ok_or_else(|| {
                format!(
                    "{}: not a library file (extensions: {})",
                    p.display(),
                    LIBRARY_EXTENSIONS
                        .iter()
                        .map(|(_, e)| format!(".{e}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| format!("{}: no file name", p.display()))?;
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok((kind.to_owned(), name, text))
        })
        .collect()
}

impl ImportedProject {
    /// Adds library files the export lacks (kind, file name, text): a
    /// file of the same kind and name as one read replaces it; a note for
    /// each.
    pub fn add_libraries(&mut self, files: Vec<(String, String, String)>) {
        for (kind, name, text) in files {
            if let Some(l) = self
                .libraries
                .iter_mut()
                .find(|(k, n, _)| *k == kind && *n == name)
            {
                l.2 = text;
                self.notes.push(format!(
                    "library file {name} ({kind}) added, in place of the one read"
                ));
            } else {
                self.notes
                    .push(format!("library file {name} ({kind}) added"));
                self.libraries.push((kind, name, text));
            }
        }
    }
}

/// Reads a whole project from a file.
pub trait ProjectImporter {
    /// Unique id.
    fn id(&self) -> &str;
    /// One-line description.
    fn description(&self) -> &str;
    /// File extension, without the dot.
    fn extension(&self) -> &str;
    /// Where it comes from.
    fn origin(&self) -> String;
    /// The project of `data`.
    fn import(&mut self, data: &[u8], options: &Options) -> Result<ImportedProject, String>;
}

struct PluginProject {
    plugin: Rc<RefCell<WasmProjectPlugin>>,
    name: String,
    id: String,
    description: String,
    extension: String,
}

impl ProjectImporter for PluginProject {
    fn id(&self) -> &str {
        &self.id
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn extension(&self) -> &str {
        &self.extension
    }
    fn origin(&self) -> String {
        format!("plugin {}", self.name)
    }
    fn import(&mut self, data: &[u8], options: &Options) -> Result<ImportedProject, String> {
        let options: Vec<(String, String)> = options
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let p = self
            .plugin
            .borrow_mut()
            .import_project(&self.id, data, &options)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("plugin {}: {e}", self.name))?;
        // Safe mode (owner, 2026-10-27): what reads is imported, the rest
        // said in the notes.
        let (line, mut safe) = line_safe(&p.line)
            .map_err(|e| format!("plugin {} returned an invalid line: {e}", self.name))?;
        let mut notes = std::mem::take(&mut safe);
        notes.extend(p.notes);
        let mut libraries: Vec<(String, String, String)> = p
            .libraries
            .into_iter()
            .map(|f| (f.kind, f.name, f.text))
            .collect();
        safe_cables(&mut libraries, &mut notes);
        Ok(ImportedProject {
            line,
            libraries,
            notes,
        })
    }
}

/// The top-level lists of a line file whose items are checked one by one
/// in safe mode.
const LISTS: [&str; 10] = [
    "routes",
    "alignments",
    "alignment_points",
    "structures",
    "systems",
    "circuits",
    "sections",
    "jumpers",
    "spacers",
    "conductor_paths",
];

/// Reads a line a plugin gives, in **safe mode** (owner, 2026-10-27: an
/// export with faults is imported as far as it reads, with a warning).
/// The line as it is when it reads and checks; else, step by step, each
/// step said in the notes (`warning: …`):
///
/// 1. empty values (`null`) left out (the field's default);
/// 2. an id given twice in a list: the later ones renamed `<id>-2`, `-3`…
///    (what refers to that id keeps the first);
/// 3. an item that does not read on its own (a structure, a section…)
///    left out, by id;
/// 4. a line that reads but does not check is kept as it is (the import
///    says "the line does not check yet"), to be mended in the project.
///
/// `Err` only when nothing reads (not JSON, or not a line at all).
pub fn line_safe(json: &str) -> Result<(Line, Vec<String>), String> {
    let first = match Line::from_json(json) {
        Ok(line) => return Ok((line, Vec::new())),
        Err(e) => e.to_string(),
    };
    let mut notes = vec![format!(
        "warning: safe mode — the line as given does not read ({first}); imported as far as it \
         reads: some things may be missing or need mending"
    )];
    let mut v: serde_json::Value = serde_json::from_str(json).map_err(|_| first.clone())?;
    let nulls = strip_nulls(&mut v);
    if nulls > 0 {
        notes.push(format!(
            "warning: {nulls} empty value(s) left out (taken as not given)"
        ));
    }
    let Some(obj) = v.as_object_mut() else {
        return Err(first);
    };
    for list in LISTS {
        let Some(items) = obj.get_mut(list).and_then(|x| x.as_array_mut()) else {
            continue;
        };
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for item in items.iter_mut() {
            let Some(id) = item.get("id").and_then(|x| x.as_str()).map(str::to_owned) else {
                continue;
            };
            if seen.insert(id.clone()) {
                continue;
            }
            let mut n = 2;
            while seen.contains(&format!("{id}-{n}")) {
                n += 1;
            }
            let to = format!("{id}-{n}");
            notes.push(format!(
                "warning: {list}: id {id} given twice; the second kept as {to} (what refers to \
                 {id} keeps the first)"
            ));
            item["id"] = serde_json::Value::String(to.clone());
            seen.insert(to);
        }
    }
    // Each item on its own: a line holding only it (and the rest of the
    // file's settings) must read.
    let mut empty = obj.clone();
    for list in LISTS {
        if let Some(x) = empty.get_mut(list) {
            *x = serde_json::Value::Array(Vec::new());
        }
    }
    let reads = |line: &serde_json::Map<String, serde_json::Value>| {
        serde_json::from_value::<Line>(serde_json::Value::Object(line.clone())).is_ok()
    };
    if !reads(obj) {
        if !reads(&empty) {
            return Err(first);
        }
        for list in LISTS {
            let Some(items) = obj.get(list).and_then(|x| x.as_array()).cloned() else {
                continue;
            };
            let mut kept = Vec::new();
            for item in items {
                let mut one = empty.clone();
                one.insert(
                    list.to_owned(),
                    serde_json::Value::Array(vec![item.clone()]),
                );
                if reads(&one) {
                    kept.push(item);
                } else {
                    let id = item.get("id").and_then(|x| x.as_str()).unwrap_or("(no id)");
                    notes.push(format!("warning: {list}: {id} does not read: left out"));
                }
            }
            obj.insert(list.to_owned(), serde_json::Value::Array(kept));
        }
    }
    let mut line: Line = serde_json::from_value(serde_json::Value::Object(obj.clone()))
        .map_err(|e| e.to_string())?;
    line.upgrade_routes();
    line.read_older();
    if let Err(e) = line.place_strings() {
        notes.push(format!("warning: strings not placed: {e}"));
    }
    line.version = ohl_line::LINE_FILE_VERSION;
    mend(&mut line, &mut notes);
    Ok((line, notes))
}

/// Mends a line that reads but does not check, so the project takes it
/// (safe mode): while the check names objects, the one whose removal gets
/// past that error is left out — first a jumper at an end of a named
/// section (it is what joins two of them), then a named jumper, spacer,
/// section, then another named object — each said in the notes. Stops
/// when the line checks, or when nothing named helps (the project then
/// refuses it with the check's error). Two joined sections of different
/// systems of the same voltage: the later takes the first's system and
/// phase instead (nothing left out). Expected cost: one check per object
/// tried; at most 1000 mends.
pub(crate) fn mend(line: &mut Line, notes: &mut Vec<String>) {
    mend_preferring(line, notes, &|_: &str| false);
}

/// [`mend`], the names of the sections `preferred` says win (owner,
/// 2026-10-27: a variant imported in Replace mode is the new state): of
/// two joined sections, one preferred and one not, the other takes the
/// preferred one's system and phase — and counts as preferred from then
/// on, so the names run along the whole conductor.
pub(crate) fn mend_preferring(
    line: &mut Line,
    notes: &mut Vec<String>,
    preferred: &dyn Fn(&str) -> bool,
) {
    let mut won: std::collections::BTreeSet<String> = line
        .sections
        .iter()
        .filter(|s| preferred(&s.id))
        .map(|s| s.id.clone())
        .collect();
    use ohl_line::JumperEnd;
    for _ in 0..1000 {
        let Err(e) = line.validate() else { return };
        let msg = e.to_string();
        let named = |id: &str| {
            msg.match_indices(id).any(|(i, _)| {
                let before = msg[..i].chars().last();
                let after = msg[i + id.len()..].chars().next();
                let edge = |c: Option<char>| {
                    c.is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'))
                };
                edge(before) && edge(after)
            })
        };
        let named_sections: Vec<&ohl_line::Section> =
            line.sections.iter().filter(|s| named(&s.id)).collect();
        let at_named = |j: &ohl_line::Jumper| {
            [&j.from, &j.to].into_iter().any(|end| match end {
                JumperEnd::Wire(p) => named_sections.iter().any(|s| s.points.contains(p)),
                JumperEnd::Tap { .. } => false,
            })
        };
        let mut candidates: Vec<(&'static str, String)> = Vec::new();
        candidates.extend(
            line.jumpers
                .iter()
                .filter(|j| at_named(j) || named(&j.id))
                .map(|j| ("jumper", j.id.clone())),
        );
        candidates.extend(
            line.spacers
                .iter()
                .filter(|x| named(&x.id))
                .map(|x| ("spacer", x.id.clone())),
        );
        // The later named section first (the earlier is kept).
        candidates.extend(
            named_sections
                .iter()
                .rev()
                .map(|x| ("section", x.id.clone())),
        );
        candidates.extend(
            line.structures
                .iter()
                .filter(|x| named(&x.id))
                .map(|x| ("structure", x.id.clone())),
        );
        // Two joined sections given different systems of the same voltage
        // (an export naming one conductor two ways): the later takes the
        // first's system and phase (the preferred one's, when only one is)
        // — every wire and jumper kept.
        if let Some((a, b)) = joined_pair(&msg).map(|(a, b)| {
            if won.contains(&b) && !won.contains(&a) {
                (b, a)
            } else {
                (a, b)
            }
        }) && let (Some(sa), Some(sb)) = (
            line.sections.iter().find(|s| s.id == a).cloned(),
            line.sections.iter().find(|s| s.id == b).cloned(),
        ) {
            let kv = |s: &ohl_line::Section| {
                s.system
                    .as_ref()
                    .and_then(|id| line.systems.iter().find(|x| &x.id == id))
                    .map(|x| x.voltage_kv)
            };
            if kv(&sa).is_some() && kv(&sa) == kv(&sb) {
                let mut t = line.clone();
                if let Some(x) = t.sections.iter_mut().find(|s| s.id == b) {
                    x.system = sa.system.clone();
                    x.phase_label = sa.phase_label.clone();
                }
                let past = match t.validate() {
                    Ok(()) => true,
                    Err(e2) => e2.to_string() != msg,
                };
                if past {
                    notes.push(format!(
                        "warning: section {b} given the system and phase of section {a} ({} {}), \
                         joined to it: it had {} {}",
                        sa.system.as_deref().unwrap_or(""),
                        sa.phase_label,
                        sb.system.as_deref().unwrap_or(""),
                        sb.phase_label
                    ));
                    if won.contains(&a) {
                        won.insert(b.clone());
                    }
                    *line = t;
                    continue;
                }
            }
        }
        let mut done = false;
        for (kind, id) in candidates {
            let mut t = line.clone();
            match kind {
                "jumper" => t.jumpers.retain(|x| x.id != id),
                "spacer" => t.spacers.retain(|x| x.id != id),
                "section" => t.sections.retain(|x| x.id != id),
                _ => t.structures.retain(|x| x.id != id),
            }
            // A structure only when that ends it (what hangs on it would
            // be named next, a cascade).
            let past = match t.validate() {
                Ok(()) => true,
                Err(e2) => kind != "structure" && e2.to_string() != msg,
            };
            if past {
                notes.push(format!(
                    "warning: {kind} {id} left out: the line did not check with it ({msg})"
                ));
                *line = t;
                done = true;
                break;
            }
        }
        if !done {
            return;
        }
    }
}

/// Safe mode for the wire files a plugin gives (owner, 2026-10-27:
/// the import gets everything in, the rules are checked afterwards): the
/// files are kept as the plugin gave them; what the wire library will
/// leave out when it loads them (a cable that does not check, an id given
/// twice) is said in the notes, and the curves it holds flat.
fn safe_cables(libraries: &mut [(String, String, String)], notes: &mut Vec<String>) {
    for (kind, name, text) in libraries.iter() {
        if kind != "cables" {
            continue;
        }
        let mut lib = ohl_cables::Library::default();
        match lib.add_toml(Path::new(name.as_str()), text) {
            Ok(()) => notes.extend(
                ohl_cables::mechanics::library_warnings(&lib)
                    .into_iter()
                    .map(|w| format!("warning: wire file {name}: {w}")),
            ),
            Err(e) => notes.push(format!("warning: wire file {name} does not read ({e})")),
        }
    }
}

/// The two sections of the check's "sections A and B are joined …"
/// error.
fn joined_pair(msg: &str) -> Option<(String, String)> {
    let rest = msg.split("sections ").nth(1)?;
    let (pair, _) = rest.split_once(" are joined")?;
    let (a, b) = pair.split_once(" and ")?;
    Some((a.trim().to_owned(), b.trim().to_owned()))
}

/// Removes every `null` member of the objects in `v`; how many.
fn strip_nulls(v: &mut serde_json::Value) -> usize {
    match v {
        serde_json::Value::Object(m) => {
            let before = m.len();
            m.retain(|_, x| !x.is_null());
            let mut n = before - m.len();
            for x in m.values_mut() {
                n += strip_nulls(x);
            }
            n
        }
        serde_json::Value::Array(a) => a.iter_mut().map(strip_nulls).sum(),
        _ => 0,
    }
}

/// Loads the project plugin in `dir` and adds its formats; returns the
/// plugin's name.
pub fn register_plugin(
    registry: &mut Registry,
    host: &WasmHost,
    dir: &Path,
) -> Result<String, String> {
    let mut plugin = host.load_project(dir).map_err(|e| e.to_string())?;
    let name = plugin.manifest().plugin.name.clone();
    let formats = plugin.formats().map_err(|e| e.to_string())?;
    if formats.is_empty() {
        return Err(format!("plugin {name} lists no formats"));
    }
    let plugin = Rc::new(RefCell::new(plugin));
    for (id, description, extension) in formats {
        registry.add_project_importer(Box::new(PluginProject {
            plugin: Rc::clone(&plugin),
            name: name.clone(),
            id: id.to_ascii_lowercase(),
            description,
            extension: extension.trim_start_matches('.').to_ascii_lowercase(),
        }));
    }
    Ok(name)
}

/// A project made by [`create_project`].
#[derive(Debug, Clone)]
pub struct CreatedProject {
    /// The project file.
    pub path: PathBuf,
    /// What was read (notes: also whether the line checks).
    pub project: ImportedProject,
    /// The line's import into the project.
    pub applied: ohl_session::Applied,
}

/// Reads another program's file (format: `format`, else the extension)
/// as a project (its line and library files), the library files
/// `libraries` added ([`ImportedProject::add_libraries`]: an export cut
/// off before its cable data), the sections clipped in if asked; nothing
/// written ([`crate::import_into`] brings it into an existing project).
pub fn read_project(
    registry: &mut Registry,
    path: &Path,
    format: Option<&str>,
    options: &Options,
    libraries: &[PathBuf],
    clip_in: bool,
) -> Result<ImportedProject, String> {
    let added = read_library_files(libraries)?;
    let id = crate::format_of(path, format);
    let importer = registry
        .project_importer(&id)
        .ok_or_else(|| format!("no project format \"{id}\": it needs a project plugin"))?;
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut p = importer.import(&data, options)?;
    p.add_libraries(added);
    if clip_in {
        let (n, failed) = p.clip_in();
        p.notes.push(format!(
            "{n} sections clipped in at their stringing condition"
        ));
        p.notes
            .extend(failed.into_iter().map(|f| format!("not clipped in: {f}")));
    }
    if let Err(e) = p.line.validate() {
        p.notes.push(format!("the line does not check yet: {e}"));
    }
    Ok(p)
}

/// Reads another program's file (format: `format`, else the extension)
/// into a new project `into` (`.ohl` added): the line and its library
/// files, those of `libraries` added, the sections clipped in if asked.
/// Refused if the project file exists.
#[allow(clippy::too_many_arguments)]
pub fn create_project(
    registry: &mut Registry,
    path: &Path,
    format: Option<&str>,
    options: &Options,
    libraries: &[PathBuf],
    into: &Path,
    user: &str,
    clip_in: bool,
) -> Result<CreatedProject, String> {
    let into = into.with_extension(ohl_core::files::PROJECT_EXTENSION);
    if into.exists() {
        return Err(format!("{} exists already", into.display()));
    }
    let p = read_project(registry, path, format, options, libraries, clip_in)?;
    let mut s = ohl_session::Session::open(&into).map_err(|e| e.to_string())?;
    let applied = s.import(&p.line, user).map_err(|e| e.to_string())?;
    for (kind, name, text) in &p.libraries {
        s.put_library_file(user, kind, name, text)
            .map_err(|e| e.to_string())?;
    }
    s.checkpoint().map_err(|e| e.to_string())?;
    Ok(CreatedProject {
        path: into,
        project: p,
        applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> serde_json::Value {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text =
            std::fs::read_to_string(root.join("examples/synthetic-line/synthetic-line.ohl.json"))
                .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// Safe mode: a line with faults is read as far as it reads, each
    /// fault said (owner, 2026-10-27).
    #[test]
    fn a_faulty_line_is_read_as_far_as_it_reads() {
        // As given: read, nothing said.
        let (line, notes) = line_safe(&example().to_string()).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let sections = line.sections.len();
        // An empty value, an id twice, a section that does not read.
        let mut v = example();
        v["structures"][0]["strings"] = serde_json::Value::Null;
        let twin = v["structures"][1].clone();
        v["structures"].as_array_mut().unwrap().push(twin);
        let bad = v["sections"][0]["id"].as_str().unwrap().to_owned();
        v["sections"][0]["bundle"] = "two".into();
        let (line, notes) = line_safe(&v.to_string()).unwrap();
        let text = notes.join("\n");
        assert!(text.contains("safe mode"), "{text}");
        assert!(text.contains("empty value"), "{text}");
        assert!(text.contains("given twice"), "{text}");
        assert!(text.contains(&format!("{bad} does not read")), "{text}");
        assert_eq!(line.sections.len(), sections - 1);
        let id = v["structures"][1]["id"].as_str().unwrap();
        assert!(line.structures.iter().any(|s| s.id == format!("{id}-2")));
        // Nothing of a line: refused.
        assert!(line_safe("not json").is_err());
        assert!(line_safe("[1, 2]").is_err());
    }

    /// A wire file with faults is kept whole (the import gets everything
    /// in); what the library leaves out when it loads it is said.
    #[test]
    fn a_faulty_wire_file_is_kept_whole_and_said() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let good =
            std::fs::read_to_string(root.join("libraries/cables/acsr-examples.ohlw")).unwrap();
        let (head, blocks) = crate::import_into::entries::cable_blocks(&good).unwrap();
        let first = &blocks[0];
        let broken = first
            .text
            .replacen(&format!("id = \"{}\"", first.id), "id = \"broken\"", 1)
            .replace("diameter = ", "diameter = \"0.1 mm\"\nx_diameter = ");
        let text = format!("{head}{}{}{broken}", first.text, first.text);
        let mut libs = vec![("cables".to_owned(), "w.ohlw".to_owned(), text.clone())];
        let mut notes = Vec::new();
        safe_cables(&mut libs, &mut notes);
        assert_eq!(libs[0].2, text, "kept as given");
        let said = notes.join("\n");
        assert!(
            said.contains(&format!("wire type {} given twice", first.id)),
            "{said}"
        );
        assert!(said.contains("broken"), "{said}");
        let mut lib = ohl_cables::Library::default();
        lib.add_toml(Path::new("w.ohlw"), &libs[0].2).unwrap();
        assert!(lib.get(&first.id).is_some());
        assert_eq!(lib.skipped().len(), 2, "{:?}", lib.skipped());
    }

    /// Two joined sections (a shared attachment or a jumper) with different phases of one
    /// system: mended to one phase — the first's, or the preferred one's
    /// (a variant in Replace mode), said.
    #[test]
    fn joined_sections_take_one_phase_the_preferred_one() {
        let base: Line = serde_json::from_value(example()).unwrap();
        // Joined by a shared attachment (a strain tower's wire point both
        // end at) or a jumper: the first such pair with a system.
        let n = base.sections.len();
        let j = (0..n)
            .flat_map(|a| (0..n).map(move |b| (a, b)))
            .find(|&(a, b)| {
                let (sa, sb) = (&base.sections[a], &base.sections[b]);
                a != b
                    && sa.system.is_some()
                    && (sa.points.iter().any(|p| sb.points.contains(p))
                        || base.jumpers.iter().any(|j| match (&j.from, &j.to) {
                            (ohl_line::JumperEnd::Wire(x), ohl_line::JumperEnd::Wire(y)) => {
                                (sa.points.contains(x) && sb.points.contains(y))
                                    || (sa.points.contains(y) && sb.points.contains(x))
                            }
                            _ => false,
                        }))
            })
            .expect("two joined sections");
        let (a, b) = j;
        let system = base.sections[a].system.clone().unwrap();
        let phases: Vec<String> = base
            .systems
            .iter()
            .find(|x| x.id == system)
            .unwrap()
            .phases
            .iter()
            .map(|p| p.label.clone())
            .collect();
        let other = phases
            .iter()
            .find(|p| **p != base.sections[a].phase_label)
            .unwrap()
            .clone();
        let mut line = base.clone();
        line.sections[b].system = Some(system.clone());
        line.sections[b].phase_label = other.clone();
        assert!(line.validate().is_err());
        let (ida, idb) = (line.sections[a].id.clone(), line.sections[b].id.clone());
        // Preferred: the other one's phase wins.
        let mut m = line.clone();
        let mut notes = Vec::new();
        mend_preferring(&mut m, &mut notes, &|id: &str| id == idb);
        assert!(m.validate().is_ok(), "{notes:?}");
        assert_eq!(m.sections[a].phase_label, other);
        assert!(
            notes
                .iter()
                .any(|n| n.contains(&format!("section {ida} given"))),
            "{notes:?}"
        );
        // No preference: one phase all the same, said.
        let mut m = line.clone();
        let mut notes = Vec::new();
        mend(&mut m, &mut notes);
        assert!(m.validate().is_ok(), "{notes:?}");
        assert_eq!(m.sections[a].phase_label, m.sections[b].phase_label);
        assert!(!notes.is_empty());
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ohl-io-libs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The kind comes from the extension (`.ohlw` is not `.ohlwc`); other
    /// files are refused.
    #[test]
    fn library_kinds_from_the_extension() {
        assert_eq!(library_kind(Path::new("a/w.ohlw")), Some("cables"));
        assert_eq!(library_kind(Path::new("w.ohlwc")), Some("weather"));
        assert_eq!(library_kind(Path::new("r.OHLR")), Some("rules"));
        assert_eq!(library_kind(Path::new("t.ohls")), Some("structures"));
        assert_eq!(library_kind(Path::new("i.ohli")), Some("insulators"));
        assert_eq!(library_kind(Path::new("x.loads.csv")), None);
        assert_eq!(library_kind(Path::new("noext")), None);
        let d = dir("kinds");
        std::fs::write(d.join("w.ohlw"), "# wires\n").unwrap();
        std::fs::write(d.join("x.txt"), "").unwrap();
        let got = read_library_files(&[d.join("w.ohlw")]).unwrap();
        assert_eq!(
            got,
            vec![("cables".into(), "w.ohlw".into(), "# wires\n".into())]
        );
        assert!(
            read_library_files(&[d.join("x.txt")])
                .unwrap_err()
                .contains("not a library file")
        );
        assert!(read_library_files(&[d.join("missing.ohlw")]).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// An added file is appended, or replaces the read one of the same
    /// kind and name; each is noted.
    #[test]
    fn added_libraries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let mut p = ImportedProject {
            line,
            libraries: vec![("cables".into(), "w.ohlw".into(), "old".into())],
            notes: Vec::new(),
        };
        p.add_libraries(vec![
            ("cables".into(), "w.ohlw".into(), "new".into()),
            ("rules".into(), "r.ohlr".into(), "r".into()),
        ]);
        assert_eq!(p.libraries.len(), 2);
        assert_eq!(p.libraries[0].2, "new");
        assert_eq!(p.libraries[1].0, "rules");
        assert_eq!(p.notes.len(), 2);
        assert!(p.notes[0].contains("in place of"));
    }
}
