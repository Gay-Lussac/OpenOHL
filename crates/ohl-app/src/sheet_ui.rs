// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The sheet template part of File › Export › Drawings…: the template
//! chosen (built in, the project's, the library folder's, a plugin's), the
//! values of its fields and the revisions, the other versions of the line
//! drawn with it in their roles, and the template's other options
//! (corridor, lines below the lowest conductor, band rows, elevation
//! labels). Kept with the drawing settings of the project line.
//!
//! The project's templates are library files of the project line (kind
//! [`KIND`], each with its images embedded): a template loaded from a
//! file is kept there, so the project travels whole.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_drawing::template::{EXTENSION, TemplateImage};
use ohl_drawing::{Revision, SheetFields, SheetTemplate};
use serde::{Deserialize, Serialize};

use crate::model::Model;

/// The project library kind of sheet templates.
pub const KIND: &str = "sheet-templates";

/// The roles offered for a version of the line (the template's own too).
pub const ROLES: [&str; 5] = ["existing", "project", "removal", "decommissioned", "other"];

/// A version of the line drawn with it: a project line, its role, its
/// label (`{name}`, `{voltage}`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionChoice {
    /// The project line.
    pub line: String,
    /// Its role (a key of the template's `[roles]`).
    pub role: String,
    /// Its label.
    pub label: String,
}

/// The weather cases of one circuit on the sheets (none: the
/// template's, else the case shown).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CircuitCases {
    /// The case its wires are drawn in (a name or a temperature, as the
    /// template's `[cases]`).
    pub drawn: Option<String>,
    /// The case of its stringing tension (the `stringing` band).
    pub stringing: Option<String>,
}

/// The sheet template and what goes with it, as the export window and the
/// drawing settings keep them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SheetChoice {
    /// The template: `default`, `project:NAME`, `library:NAME`,
    /// `plugin:PLUGIN/NAME` or `file:PATH`.
    pub template: String,
    /// The sheets turned to portrait.
    pub portrait: bool,
    /// The values of the template's fields and the revisions.
    pub fields: SheetFields,
    /// Other versions of the line drawn with it.
    pub versions: Vec<VersionChoice>,
    /// The role of the line itself ("": none).
    pub role: String,
    /// The corridor's half width, m (none: the template's).
    pub corridor: Option<f64>,
    /// Lines below the lowest conductor (the project's ground rules).
    pub conductor_clearance: bool,
    /// The template's band rows.
    pub bands: bool,
    /// The elevations' own scale (none: the horizontal one).
    pub elevation_scale: Option<f64>,
    /// What is written at each elevation.
    pub elevation_labels: Vec<String>,
    /// The elevations as stick figures (none: as the template says).
    #[serde(default)]
    pub elevation_stick: Option<bool>,
    /// The LiDAR classes drawn, codes comma separated (none: the
    /// template's `[lidar] classes`).
    pub lidar_classes: Option<String>,
    /// The weather cases per circuit (its system's name; "": the earth
    /// wires): drawn and of the stringing band, over the template's
    /// `[cases]` (2026-10-23).
    #[serde(default)]
    pub circuit_cases: std::collections::BTreeMap<String, CircuitCases>,
    /// The project's drawings (DXF) under the plan, on or off (none: as
    /// the template's `[plan_background] drawings` says).
    pub background_drawings: Option<bool>,
    /// The project's orthophotos under the plan, on or off (none: as the
    /// template's `[plan_background] orthophoto` says).
    pub background_photo: Option<bool>,
    /// The project's drawings under the key map, on or off (none: as the
    /// template's `[key_map] drawings` says; 2026-10-23).
    pub key_map_drawings: Option<bool>,
    /// The project's orthophotos under the key map, on or off (none: as
    /// the template's `[key_map] orthophoto` says; 2026-10-23).
    pub key_map_photo: Option<bool>,
}

/// LiDAR class codes written `2, 3, 5` (empty: none).
pub fn parse_classes(text: &str) -> Result<Vec<u8>, String> {
    text.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.parse::<u8>()
                .map_err(|_| format!("LiDAR class \"{t}\": a code from 0 to 255"))
        })
        .collect()
}

impl Default for SheetChoice {
    fn default() -> Self {
        Self {
            template: "default".into(),
            portrait: false,
            fields: SheetFields::default(),
            versions: Vec::new(),
            role: String::new(),
            corridor: None,
            conductor_clearance: false,
            bands: true,
            elevation_scale: None,
            elevation_labels: Vec::new(),
            elevation_stick: None,
            lidar_classes: None,
            circuit_cases: Default::default(),
            background_drawings: None,
            background_photo: None,
            key_map_drawings: None,
            key_map_photo: None,
        }
    }
}

/// The templates the plugins give: key `plugin:PLUGIN/NAME`, the template
/// or why it does not read, its description.
pub type PluginTemplates = Arc<Vec<(String, Result<Arc<SheetTemplate>, String>, String)>>;

/// A template offered in the window.
#[derive(Debug, Clone)]
pub struct Offer {
    /// Its key ([`SheetChoice::template`]).
    pub key: String,
    /// Its group: Built-in, Project, Library, or the plugin's name.
    pub group: String,
    /// Its name.
    pub name: String,
    /// What it is for, or why it does not read.
    pub about: String,
    /// It reads.
    pub ok: bool,
}

/// The project's templates: name, text.
fn project_texts(model: &Model) -> Vec<(String, String)> {
    model
        .session()
        .and_then(|s| s.library_files(KIND).ok())
        .unwrap_or_default()
}

/// The library folder's template files.
fn library_files() -> Vec<PathBuf> {
    ohl_core::files::files_with_extension(&ohl_core::files::shipped_library(KIND), EXTENSION)
}

fn stem(name: &str) -> String {
    name.trim_end_matches(&format!(".{EXTENSION}")).to_owned()
}

/// Every template offered: built in, the project's, the library folder's,
/// the plugins'.
pub fn offers(model: &Model, plugins: &PluginTemplates) -> Vec<Offer> {
    let mut out = vec![Offer {
        key: "default".into(),
        group: "Built-in".into(),
        name: "default".into(),
        about: "The sheets as OpenOHL draws them without a template of yours".into(),
        ok: true,
    }];
    let about = |t: &Result<SheetTemplate, String>| match t {
        Ok(t) => (t.description.clone(), true),
        Err(e) => (e.clone(), false),
    };
    for (name, text) in project_texts(model) {
        let (a, ok) = about(&SheetTemplate::parse(&text, None, &[]));
        out.push(Offer {
            key: format!("project:{}", stem(&name)),
            group: "Project".into(),
            name: stem(&name),
            about: a,
            ok,
        });
    }
    for p in library_files() {
        let name = p
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        let (a, ok) = about(&SheetTemplate::load(&p));
        out.push(Offer {
            key: format!("library:{name}"),
            group: "Library".into(),
            name,
            about: a,
            ok,
        });
    }
    for (key, t, description) in plugins.iter() {
        let full = key.trim_start_matches("plugin:");
        let (plugin, name) = full.split_once('/').unwrap_or((full, ""));
        out.push(Offer {
            key: key.clone(),
            group: plugin.to_owned(),
            name: name.to_owned(),
            about: match t {
                Ok(_) => description.clone(),
                Err(e) => e.clone(),
            },
            ok: t.is_ok(),
        });
    }
    out
}

/// The template `key` names.
/// How long a template read from a file or the project is used again
/// without reading it (the window and the preview ask every frame).
const TEMPLATES_KEPT: std::time::Duration = std::time::Duration::from_secs(1);

/// Templates read lately, by key: when, for which library revision of the
/// project, and what came out.
type Kept<T> = std::collections::HashMap<String, (std::time::Instant, u64, T)>;

/// A template read, or why not.
type Read = Result<Arc<SheetTemplate>, String>;

static RESOLVED: std::sync::Mutex<Option<Kept<Read>>> = std::sync::Mutex::new(None);
static OFFERED: std::sync::Mutex<Option<Kept<Vec<Offer>>>> = std::sync::Mutex::new(None);

/// [`resolve`] for what is drawn every frame: a template read from the
/// project, the library folder or a file kept for up to a second, and while
/// the project's libraries are unchanged (the plugins' are in memory).
pub fn resolve_kept(
    model: &Model,
    key: &str,
    plugins: &PluginTemplates,
) -> Result<Arc<SheetTemplate>, String> {
    if key.starts_with("plugin:") {
        return resolve(model, key, plugins);
    }
    let stamp = model.revisions.libraries;
    let now = std::time::Instant::now();
    if let Ok(m) = RESOLVED.lock()
        && let Some((at, s, r)) = m.as_ref().and_then(|m| m.get(key))
        && *s == stamp
        && now.duration_since(*at) < TEMPLATES_KEPT
    {
        return r.clone();
    }
    let r = resolve(model, key, plugins);
    if let Ok(mut m) = RESOLVED.lock() {
        m.get_or_insert_with(Default::default)
            .insert(key.to_owned(), (now, stamp, r.clone()));
    }
    r
}

/// [`offers`] kept as [`resolve_kept`] keeps templates.
pub fn offers_kept(model: &Model, plugins: &PluginTemplates) -> Vec<Offer> {
    let stamp = model.revisions.libraries;
    let now = std::time::Instant::now();
    // The plugins' templates are part of it: their keys too.
    let key: String = plugins
        .iter()
        .map(|(k, ..)| k.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if let Ok(m) = OFFERED.lock()
        && let Some((at, s, o)) = m.as_ref().and_then(|m| m.get(&key))
        && *s == stamp
        && now.duration_since(*at) < TEMPLATES_KEPT
    {
        return o.clone();
    }
    let o = offers(model, plugins);
    if let Ok(mut m) = OFFERED.lock() {
        let m = m.get_or_insert_with(Default::default);
        m.clear();
        m.insert(key, (now, stamp, o.clone()));
    }
    o
}

pub fn resolve(
    model: &Model,
    key: &str,
    plugins: &PluginTemplates,
) -> Result<Arc<SheetTemplate>, String> {
    let (kind, name) = key.split_once(':').unwrap_or((key, ""));
    match kind {
        "default" | "" => Ok(Arc::new(SheetTemplate::builtin())),
        "project" => project_texts(model)
            .into_iter()
            .find(|(n, _)| stem(n) == name)
            .ok_or_else(|| format!("the project has no sheet template \"{name}\""))
            .and_then(|(_, text)| SheetTemplate::parse(&text, None, &[]))
            .map(Arc::new),
        "library" => library_files()
            .into_iter()
            .find(|p| p.file_stem().is_some_and(|s| s.to_string_lossy() == name))
            .ok_or_else(|| format!("no sheet template \"{name}\" in the library folder"))
            .and_then(|p| SheetTemplate::load(&p))
            .map(Arc::new),
        "plugin" => plugins.iter().find(|(k, ..)| k == key).map_or_else(
            || {
                Err(format!(
                    "the plugin's sheet template \"{name}\" is not there (the plugin turned \
                         off or not read yet?)"
                ))
            },
            |(_, t, _)| t.clone(),
        ),
        "file" => SheetTemplate::load(std::path::Path::new(name)).map(Arc::new),
        _ => Err(format!("sheet template \"{key}\": not known")),
    }
}

/// Reads the plugins' templates in the background (WebAssembly plugins
/// are compiled: under a second each).
pub fn read_plugins(dirs: Vec<(String, PathBuf)>) -> Receiver<PluginTemplates> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let host = ohl_plugins::WasmHost::new();
        let mut out = Vec::new();
        for (plugin, dir) in dirs {
            let list = match &host {
                Ok(h) => h.sheet_templates(&dir).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            match list {
                Ok(list) => {
                    for t in list {
                        let images: Vec<TemplateImage> = t
                            .images
                            .into_iter()
                            .map(|(name, media_type, bytes)| TemplateImage {
                                name,
                                media_type,
                                bytes,
                            })
                            .collect();
                        out.push((
                            format!("plugin:{plugin}/{}", t.name),
                            SheetTemplate::parse(&t.template, t.dir.as_deref(), &images)
                                .map(Arc::new)
                                .map_err(|e| format!("plugin {plugin}, {}: {e}", t.name)),
                            t.description,
                        ));
                    }
                }
                Err(e) => out.push((format!("plugin:{plugin}/"), Err(e), String::new())),
            }
        }
        let _ = tx.send(Arc::new(out));
    });
    rx
}

/// What the template row asks of the window.
pub enum Action {
    /// Keep the template file in the project (or use it from the file).
    Load(PathBuf),
    /// Write the template chosen to a file.
    SaveAs(PathBuf),
}

/// The template row: a dropdown grouped by source, Load file…, Save as
/// file….
pub fn template_ui(
    ui: &mut egui::Ui,
    choice: &mut SheetChoice,
    offers: &[Offer],
) -> Option<Action> {
    let mut action = None;
    let shown = offers
        .iter()
        .find(|o| o.key == choice.template)
        .map_or(choice.template.clone(), |o| {
            format!("{} › {}", o.group, o.name)
        });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("drawings-template")
            .selected_text(shown)
            .width(260.0)
            .show_ui(ui, |ui| {
                let mut group = String::new();
                for o in offers {
                    if o.group != group {
                        group = o.group.clone();
                        ui.label(egui::RichText::new(&group).small().strong());
                    }
                    let text = if o.ok {
                        format!("   {}", o.name)
                    } else {
                        format!("   {} ⚠", o.name)
                    };
                    let r = ui.add_enabled(
                        o.ok,
                        egui::Button::selectable(choice.template == o.key, text),
                    );
                    let r = if o.about.is_empty() {
                        r
                    } else {
                        r.on_hover_text(&o.about)
                    };
                    if r.clicked() {
                        choice.template = o.key.clone();
                    }
                }
            })
            .response
            .on_hover_text(
                "The sheet template: page, frame, title block, band rows, legend, key map, \
                 styles — built in, the project's, the library folder's (libraries/\
                 sheet-templates) or a plugin's",
            );
        if ui
            .button("Load file…")
            .on_hover_text("A .ohlsheet file, kept in the project with its images")
            .clicked()
            && let Some(p) = rfd::FileDialog::new()
                .add_filter("Sheet templates", &[EXTENSION])
                .pick_file()
        {
            action = Some(Action::Load(p));
        }
        if ui
            .button("Save as file…")
            .on_hover_text("The template chosen as a .ohlsheet file (its images embedded), to edit")
            .clicked()
            && let Some(p) = rfd::FileDialog::new()
                .add_filter("Sheet templates", &[EXTENSION])
                .set_file_name(format!(
                    "{}.{EXTENSION}",
                    stem(
                        choice
                            .template
                            .rsplit([':', '/'])
                            .next()
                            .unwrap_or("template")
                    )
                ))
                .save_file()
        {
            action = Some(Action::SaveAs(p));
        }
    });
    action
}

/// The fields of template `t` (required marked *), the per-sheet ones as
/// a small table of sheet and value.
pub fn fields_ui(ui: &mut egui::Ui, t: &SheetTemplate, f: &mut SheetFields) {
    if t.fields.is_empty() {
        ui.label(egui::RichText::new("The template asks for no values.").weak());
        return;
    }
    egui::Grid::new("drawings-fields")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for field in &t.fields {
                let label = if field.label.is_empty() {
                    field.key.clone()
                } else {
                    field.label.clone()
                };
                ui.label(if field.required {
                    format!("{label} *")
                } else {
                    label
                })
                .on_hover_text(format!("{{{}}} in the template", field.key));
                if field.per_sheet {
                    ui.vertical(|ui| {
                        let rows = f.per_sheet.entry(field.key.clone()).or_default();
                        let mut gone = None;
                        let mut moved = None;
                        for (n, value) in rows.iter_mut() {
                            ui.horizontal(|ui| {
                                let mut k = *n;
                                ui.label("sheet");
                                ui.add(egui::DragValue::new(&mut k).range(1..=999));
                                if k != *n {
                                    moved = Some((*n, k));
                                }
                                ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
                                if ui.small_button("×").clicked() {
                                    gone = Some(*n);
                                }
                            });
                        }
                        if let Some(n) = gone {
                            rows.remove(&n);
                        }
                        if let Some((from, to)) = moved
                            && !rows.contains_key(&to)
                            && let Some(v) = rows.remove(&from)
                        {
                            rows.insert(to, v);
                        }
                        if ui.small_button("+ a sheet's value").clicked() {
                            let next = rows.keys().max().map_or(1, |k| k + 1);
                            rows.insert(next, field.default.clone());
                        }
                    });
                } else {
                    let value = f
                        .values
                        .entry(field.key.clone())
                        .or_insert_with(|| field.default.clone());
                    ui.add(egui::TextEdit::singleline(value).desired_width(260.0));
                }
                ui.end_row();
            }
        });
    if let Some(why) = t.missing_fields(f) {
        ui.label(
            egui::RichText::new(why)
                .small()
                .color(ui.visuals().warn_fg_color),
        );
    }
}

/// The revision table's rows (oldest first): edited, added (empty, or the
/// project's last version commit), deleted.
pub fn revisions_ui(ui: &mut egui::Ui, f: &mut SheetFields, last_commit: Option<Revision>) {
    let mut gone = None;
    egui::ScrollArea::vertical()
        .id_salt("drawings-revisions")
        .max_height(140.0)
        .show(ui, |ui| {
            egui::Grid::new("drawings-revisions-grid")
                .num_columns(6)
                .spacing([6.0, 3.0])
                .show(ui, |ui| {
                    for h in ["Rev", "Date", "What changed", "By", "Checked", ""] {
                        ui.label(egui::RichText::new(h).small().strong());
                    }
                    ui.end_row();
                    for (k, r) in f.revisions.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut r.index).desired_width(30.0));
                        ui.add(egui::TextEdit::singleline(&mut r.date).desired_width(80.0));
                        ui.add(egui::TextEdit::singleline(&mut r.text).desired_width(200.0));
                        ui.add(egui::TextEdit::singleline(&mut r.by).desired_width(50.0));
                        ui.add(egui::TextEdit::singleline(&mut r.checked).desired_width(50.0));
                        if ui
                            .small_button("×")
                            .on_hover_text("Delete the row")
                            .clicked()
                        {
                            gone = Some(k);
                        }
                        ui.end_row();
                    }
                });
        });
    if let Some(k) = gone {
        f.revisions.remove(k);
    }
    ui.horizontal(|ui| {
        if ui.button("Add a row").clicked() {
            let index = next_index(f.revisions.last().map(|r| r.index.as_str()));
            f.revisions.push(Revision {
                index,
                date: ohl_drawing::today(),
                ..Revision::default()
            });
        }
        let enabled = last_commit.is_some();
        if ui
            .add_enabled(enabled, egui::Button::new("Add the last version commit"))
            .on_hover_text(
                "A row from the project's last commit (Versions): its date, message and author",
            )
            .clicked()
            && let Some(c) = last_commit
        {
            let index = next_index(f.revisions.last().map(|r| r.index.as_str()));
            f.revisions.push(Revision { index, ..c });
        }
    });
}

/// The index after `last`: A → B, 3 → 4, none → A.
pub fn next_index(last: Option<&str>) -> String {
    match last.map(str::trim) {
        None | Some("") => "A".into(),
        Some(t) => {
            if let Ok(n) = t.parse::<u32>() {
                return (n + 1).to_string();
            }
            let mut chars: Vec<char> = t.chars().collect();
            match chars.last_mut() {
                Some(c) if c.is_ascii_uppercase() && *c != 'Z' => {
                    *c = char::from(*c as u8 + 1);
                    chars.into_iter().collect()
                }
                _ => format!("{t}1"),
            }
        }
    }
}

/// The last version commit of the project as a revision row.
pub fn last_commit(model: &Model) -> Option<Revision> {
    let r = model.session()?.revisions().ok()?.into_iter().next()?;
    Some(Revision {
        index: String::new(),
        date: r.time.get(..10).unwrap_or(&r.time).to_owned(),
        text: r.message.lines().next().unwrap_or_default().to_owned(),
        by: r.author,
        checked: String::new(),
    })
}

/// The other versions drawn: project lines with their roles; `lines` the
/// project lines one may add, `roles` the roles offered.
pub fn versions_ui(
    ui: &mut egui::Ui,
    choice: &mut SheetChoice,
    lines: &[String],
    roles: &[String],
) {
    let mut gone = None;
    for (k, v) in choice.versions.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(&v.line);
            egui::ComboBox::from_id_salt(("drawings-version-role", k))
                .selected_text(&v.role)
                .show_ui(ui, |ui| {
                    for r in roles {
                        ui.selectable_value(&mut v.role, r.clone(), r);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut v.label)
                    .hint_text("{name}")
                    .desired_width(140.0),
            )
            .on_hover_text("Its label in the plan and the legend: {name}, {voltage}");
            if ui.small_button("×").clicked() {
                gone = Some(k);
            }
        });
    }
    if let Some(k) = gone {
        choice.versions.remove(k);
    }
    let free: Vec<&String> = lines
        .iter()
        .filter(|l| !choice.versions.iter().any(|v| &v.line == *l))
        .collect();
    if free.is_empty() {
        if choice.versions.is_empty() {
            ui.label(
                egui::RichText::new("No other project line (Versions › Project lines)").weak(),
            );
        }
        return;
    }
    egui::ComboBox::from_id_salt("drawings-version-add")
        .selected_text("Add a project line…")
        .show_ui(ui, |ui| {
            for l in free {
                if ui.selectable_label(false, l).clicked() {
                    choice.versions.push(VersionChoice {
                        line: l.clone(),
                        role: "existing".into(),
                        label: "{name}".into(),
                    });
                }
            }
        });
}

/// The roles offered with template `t`: its own, then the usual ones.
pub fn roles(t: &SheetTemplate) -> Vec<String> {
    let mut out: Vec<String> = t.roles.keys().cloned().collect();
    for r in ROLES {
        if !out.iter().any(|x| x == r) {
            out.push(r.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_indexes_follow() {
        assert_eq!(next_index(None), "A");
        assert_eq!(next_index(Some("A")), "B");
        assert_eq!(next_index(Some("3")), "4");
        assert_eq!(next_index(Some("Z")), "Z1");
    }
}
