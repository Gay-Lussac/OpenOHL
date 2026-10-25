//! A plugin's presets edited in the project (owner, 2026-10-25): its rule
//! presets, their weather cases and its sheet templates shown value by
//! value in **Plugins › Plugin manager › Presets…**; a value changed is
//! kept in the project's library (`plugin-preset-edits/<plugin>.toml`,
//! one change), the plugin left as it is, and applied wherever the
//! presets are read: the rule presets every window offers, the sheet
//! templates drawn. The **Changes** tab lists every value changed — the
//! plugin's, the project's, whether the plugin still gives the value the
//! edit was made from — so that a check's basis can be followed.
//!
//! A value is found by its path in the preset's TOML: the keys, and in an
//! array of tables the element's `name` (`name=…`), else its place
//! (`#3`). An inline table or array is one value.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use toml_edit::{DocumentMut, Item, Table, Value};

/// The project library kind the edits are kept in.
pub const KIND: &str = "plugin-preset-edits";
/// The edits file's `format`.
const FORMAT: &str = "ohl-plugin-preset-edits";

/// One value of a preset changed in the project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Edit {
    /// The preset: `rules:<preset>`, `weather:<preset>` (its weather
    /// cases) or `sheet:<template>`.
    pub item: String,
    /// The value's path ([module](self)).
    pub path: Vec<String>,
    /// The plugin's value when it was changed (TOML).
    pub original: String,
    /// The project's value (TOML).
    pub value: String,
    /// Why, in words.
    pub note: String,
}

/// The edits of every plugin, by plugin.
pub type Edits = BTreeMap<String, Vec<Edit>>;

/// The library file name of a plugin's edits.
pub fn file_name(plugin: &str) -> String {
    format!("{plugin}.toml")
}

/// Reads an edits file: the plugin and its edits.
pub fn parse(text: &str) -> Result<(String, Vec<Edit>), String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    if doc.get("format").and_then(Item::as_str) != Some(FORMAT) {
        return Err(format!("not a file of format \"{FORMAT}\""));
    }
    let plugin = doc
        .get("plugin")
        .and_then(Item::as_str)
        .ok_or("no plugin")?
        .to_owned();
    let mut out = Vec::new();
    if let Some(a) = doc.get("edit").and_then(Item::as_array_of_tables) {
        for t in a.iter() {
            let s = |k: &str| {
                t.get(k)
                    .and_then(Item::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let path: Vec<String> = t
                .get("path")
                .and_then(Item::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            if s("item").is_empty() || path.is_empty() {
                return Err("an edit without its item or path".into());
            }
            out.push(Edit {
                item: s("item"),
                path,
                original: s("original"),
                value: s("value"),
                note: s("note"),
            });
        }
    }
    Ok((plugin, out))
}

/// The edits file of `plugin`.
pub fn to_text(plugin: &str, edits: &[Edit]) -> String {
    let mut doc = DocumentMut::new();
    doc.insert("format", toml_edit::value(FORMAT));
    doc.insert("version", toml_edit::value(1));
    doc.insert("plugin", toml_edit::value(plugin));
    let mut a = toml_edit::ArrayOfTables::new();
    for e in edits {
        let mut t = Table::new();
        t.insert("item", toml_edit::value(e.item.as_str()));
        let mut path = toml_edit::Array::new();
        for p in &e.path {
            path.push(p.as_str());
        }
        t.insert("path", toml_edit::value(path));
        t.insert("original", toml_edit::value(e.original.as_str()));
        t.insert("value", toml_edit::value(e.value.as_str()));
        if !e.note.is_empty() {
            t.insert("note", toml_edit::value(e.note.as_str()));
        }
        a.push(t);
    }
    if !a.is_empty() {
        doc.insert("edit", Item::ArrayOfTables(a));
    }
    doc.to_string()
}

/// Edits read lately: for which project and library revision.
type KeptEdits = Option<(PathBuf, u64, Edits, Vec<String>)>;
static KEPT: std::sync::Mutex<KeptEdits> = std::sync::Mutex::new(None);

/// The project's edits, by plugin, and the files not read (kept while the
/// project's libraries are unchanged).
pub fn of_model(model: &crate::model::Model) -> (Edits, Vec<String>) {
    let stamp = model.revisions.libraries;
    let file = model.sources.file.clone();
    if let Ok(k) = KEPT.lock()
        && let Some((f, s, e, bad)) = k.as_ref()
        && *f == file
        && *s == stamp
    {
        return (e.clone(), bad.clone());
    }
    let mut edits = Edits::new();
    let mut bad = Vec::new();
    for (name, text) in model
        .session()
        .and_then(|s| s.library_files(KIND).ok())
        .unwrap_or_default()
    {
        match parse(&text) {
            Ok((plugin, list)) => {
                edits.entry(plugin).or_default().extend(list);
            }
            Err(e) => bad.push(format!("{KIND}/{name}: {e}")),
        }
    }
    if let Ok(mut k) = KEPT.lock() {
        *k = Some((file, stamp, edits.clone(), bad.clone()));
    }
    (edits, bad)
}

/// A value of a preset's TOML.
#[derive(Debug, Clone, PartialEq)]
pub struct Leaf {
    /// Its path.
    pub path: Vec<String>,
    /// Its TOML text.
    pub text: String,
    /// A string (shown and typed without its quotes).
    pub string: bool,
}

/// A value's TOML text without its surrounding blanks and comments.
fn value_text(v: &Value) -> String {
    let mut v = v.clone();
    v.decor_mut().clear();
    v.to_string()
}

/// The element names of an array of tables: `name=…` where it has a
/// name unique in the array, else `#k`.
fn element_keys(a: &toml_edit::ArrayOfTables) -> Vec<String> {
    let names: Vec<Option<&str>> = a
        .iter()
        .map(|t| t.get("name").and_then(Item::as_str))
        .collect();
    names
        .iter()
        .enumerate()
        .map(|(k, n)| match n {
            Some(n) if names.iter().filter(|m| **m == Some(*n)).count() == 1 => {
                format!("name={n}")
            }
            _ => format!("#{k}"),
        })
        .collect()
}

/// Every value of `text` (a TOML document), in its order.
pub fn leaves(text: &str) -> Result<Vec<Leaf>, String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    let mut out = Vec::new();
    walk(doc.as_table(), &mut Vec::new(), &mut out);
    Ok(out)
}

fn walk(t: &Table, path: &mut Vec<String>, out: &mut Vec<Leaf>) {
    for (k, item) in t.iter() {
        // The file's kind and version are not values to change.
        if path.is_empty() && (k == "format" || k == "version") {
            continue;
        }
        path.push(k.to_owned());
        match item {
            Item::Value(v) => out.push(Leaf {
                path: path.clone(),
                text: value_text(v),
                string: v.is_str(),
            }),
            Item::Table(t) => walk(t, path, out),
            Item::ArrayOfTables(a) => {
                for (t, key) in a.iter().zip(element_keys(a)) {
                    path.push(key);
                    walk(t, path, out);
                    path.pop();
                }
            }
            Item::None => {}
        }
        path.pop();
    }
}

fn find_mut<'a>(t: &'a mut Table, path: &[String]) -> Option<&'a mut Item> {
    let (first, rest) = path.split_first()?;
    let item = t.get_mut(first)?;
    if rest.is_empty() {
        return Some(item);
    }
    match item {
        Item::Table(t) => find_mut(t, rest),
        Item::ArrayOfTables(a) => {
            let (sel, rest) = rest.split_first()?;
            let k = element_keys(a).iter().position(|x| x == sel)?;
            find_mut(a.get_mut(k)?, rest)
        }
        _ => None,
    }
}

/// How an edit stands against the plugin's preset as it is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Applied; the plugin gives the value it was made from.
    Applied,
    /// Applied, but the plugin's value is no longer the one it was made
    /// from (the plugin changed): its value now.
    PluginChanged(String),
    /// The plugin's preset has no such value: not applied.
    Missing,
    /// The project's value is not a TOML value: not applied.
    Invalid(String),
}

impl State {
    /// In words.
    pub fn words(&self) -> String {
        match self {
            State::Applied => "applied".into(),
            State::PluginChanged(now) => {
                format!("applied — the plugin's value changed since, now {now}")
            }
            State::Missing => "not applied: the preset has no such value (any more)".into(),
            State::Invalid(e) => format!("not applied: {e}"),
        }
    }

    /// Whether it needs a look.
    pub fn warn(&self) -> bool {
        *self != State::Applied
    }
}

/// The project's value of an edit as a TOML value (a string's typed
/// without quotes when the plugin's is a string).
fn parse_value(text: &str) -> Result<Value, String> {
    text.trim()
        .parse::<Value>()
        .map_err(|e| format!("\"{text}\" is not a value: {}", e.to_string().trim()))
}

/// `text` with the edits of `item` applied, and how each stands (in the
/// order of `edits`, those of other items `None`).
pub fn apply(text: &str, item: &str, edits: &[Edit]) -> (String, Vec<Option<State>>) {
    let mut states = vec![None; edits.len()];
    if !edits.iter().any(|e| e.item == item) {
        return (text.to_owned(), states);
    }
    let Ok(mut doc) = text.parse::<DocumentMut>() else {
        return (text.to_owned(), states);
    };
    for (k, e) in edits.iter().enumerate() {
        if e.item != item {
            continue;
        }
        let state = match find_mut(doc.as_table_mut(), &e.path) {
            Some(Item::Value(v)) => match parse_value(&e.value) {
                Ok(mut new) => {
                    let now = value_text(v);
                    *new.decor_mut() = v.decor().clone();
                    *v = new;
                    if now == e.original {
                        State::Applied
                    } else {
                        State::PluginChanged(now)
                    }
                }
                Err(why) => State::Invalid(why),
            },
            _ => State::Missing,
        };
        states[k] = Some(state);
    }
    (doc.to_string(), states)
}

/// The edits of a preset's rules and weather cases applied, the preset
/// checked again; its description says how many values the project
/// changed. Unchanged when none.
pub fn apply_preset(
    plugin: &str,
    p: &ohl_io::codes::RulePresetChecked,
    edits: &[Edit],
) -> Result<ohl_io::codes::RulePresetChecked, String> {
    let (ri, wi) = (format!("rules:{}", p.name), format!("weather:{}", p.name));
    let n = edits
        .iter()
        .filter(|e| e.item == ri || e.item == wi)
        .count();
    if n == 0 {
        return Ok(p.clone());
    }
    let (rules, _) = apply(&p.rules_text, &ri, edits);
    let (weather, _) = apply(&p.weather_text, &wi, edits);
    ohl_io::codes::RulePresetChecked::check(
        plugin,
        ohl_plugins::RulePreset {
            name: p.name.clone(),
            description: format!(
                "{} ({n} value{} changed in this project: Plugins › Plugin manager › Presets…)",
                p.description,
                if n == 1 { "" } else { "s" }
            ),
            rules,
            weather_cases: weather,
        },
    )
}

/// The plugins' rule presets with the project's edits (a preset an edit
/// makes invalid kept as the plugin gives it, and said).
pub fn apply_presets(
    original: &[crate::rule_editor::PluginPresets],
    edits: &Edits,
) -> (Vec<crate::rule_editor::PluginPresets>, Vec<String>) {
    let mut errors = Vec::new();
    let list = original
        .iter()
        .map(|pp| {
            let Some(e) = edits.get(&pp.plugin).filter(|e| !e.is_empty()) else {
                return pp.clone();
            };
            let presets = pp.presets.as_ref().map(|list| {
                list.iter()
                    .map(|p| match apply_preset(&pp.plugin, p, e) {
                        Ok(x) => x,
                        Err(why) => {
                            errors.push(format!(
                                "{} \"{}\": the project's values not taken ({why})",
                                pp.plugin, p.name
                            ));
                            p.clone()
                        }
                    })
                    .collect()
            });
            crate::rule_editor::PluginPresets {
                plugin: pp.plugin.clone(),
                presets: presets.map_err(Clone::clone),
            }
        })
        .collect();
    (list, errors)
}

/// The tabs of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Presets,
    Changes,
}

/// A plugin's sheet templates as it gives them: name, text.
type Sheets = Result<Vec<(String, String)>, String>;

/// The window of a plugin's presets (Plugin manager › Presets…).
#[derive(Default)]
pub struct PresetEditor {
    /// Shown.
    pub open: bool,
    /// The plugin.
    pub plugin: String,
    /// Its folder (its sheet templates read from it).
    dir: Option<PathBuf>,
    tab: Tab,
    /// The item shown (`rules:…`, `weather:…`, `sheet:…`).
    item: Option<String>,
    /// Only the values whose path or value holds it.
    filter: String,
    /// Only the values changed.
    only_changed: bool,
    /// The value being typed: item, path, text.
    typing: Option<(String, Vec<String>, String)>,
    sheets: Option<Sheets>,
    sheet_job: Option<Receiver<Sheets>>,
    status: Option<(String, bool)>,
}

impl PresetEditor {
    /// Opens the window on `plugin` (in `dir`).
    pub fn open_on(&mut self, plugin: &str, dir: Option<PathBuf>) {
        if self.plugin != plugin {
            *self = PresetEditor::default();
            self.plugin = plugin.to_owned();
        }
        self.open = true;
        self.dir = dir;
        self.sheets = None;
        self.sheet_job = None;
    }

    /// Whether the plugin's sheet templates are being read.
    pub fn reading(&self) -> bool {
        self.sheet_job.is_some()
    }

    fn read_sheets(&mut self) {
        if self.sheets.is_some() || self.sheet_job.is_some() {
            return;
        }
        let Some(dir) = self.dir.clone() else {
            self.sheets = Some(Ok(Vec::new()));
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = ohl_plugins::WasmHost::new()
                .map_err(|e| e.to_string())
                .and_then(|h| h.sheet_templates(&dir).map_err(|e| e.to_string()))
                .map(|list| list.into_iter().map(|t| (t.name, t.template)).collect());
            let _ = tx.send(r);
        });
        self.sheet_job = Some(rx);
    }

    fn poll_sheets(&mut self) {
        if let Some(rx) = &self.sheet_job
            && let Ok(r) = rx.try_recv()
        {
            self.sheets = Some(r);
            self.sheet_job = None;
        }
    }
}

/// An item of the plugin: key, title, the plugin's text.
struct Shown {
    key: String,
    title: String,
    text: String,
}

/// The plugin's items as it gives them: its rule presets (rules, weather
/// cases), its sheet templates.
fn items(
    plugin: &str,
    original: &[crate::rule_editor::PluginPresets],
    sheets: Option<&Sheets>,
) -> (Vec<Shown>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for pp in original.iter().filter(|p| p.plugin == plugin) {
        match &pp.presets {
            Ok(list) => {
                for p in list {
                    out.push(Shown {
                        key: format!("rules:{}", p.name),
                        title: format!("{} — rules", p.name),
                        text: p.rules_text.clone(),
                    });
                    if !p.weather_text.trim().is_empty() {
                        out.push(Shown {
                            key: format!("weather:{}", p.name),
                            title: format!("{} — weather cases", p.name),
                            text: p.weather_text.clone(),
                        });
                    }
                }
            }
            Err(e) => errors.push(e.clone()),
        }
    }
    match sheets {
        Some(Ok(list)) => {
            for (name, text) in list {
                out.push(Shown {
                    key: format!("sheet:{name}"),
                    title: format!("{name} — sheet template"),
                    text: text.clone(),
                });
            }
        }
        Some(Err(e)) => errors.push(format!("sheet templates: {e}")),
        None => {}
    }
    (out, errors)
}

/// How each of `edits` stands against the plugin's items as they are now.
pub fn states(edits: &[Edit], texts: &[(String, String)]) -> Vec<State> {
    let mut out = vec![State::Missing; edits.len()];
    for (key, text) in texts {
        let (_, s) = apply(text, key, edits);
        for (k, s) in s.into_iter().enumerate() {
            if let Some(s) = s {
                out[k] = s;
            }
        }
    }
    out
}

/// The edits as CSV (for the record of a check's basis).
pub fn csv(plugin: &str, edits: &[Edit], states: &[State]) -> String {
    use std::fmt::Write;
    let q = ohl_core::text::csv_field;
    let mut out = String::from("plugin,preset,value,plugin_value,project_value,status,note\n");
    for (e, s) in edits.iter().zip(states) {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{}",
            q(plugin),
            q(&e.item),
            q(&e.path.join(" › ")),
            q(&e.original),
            q(&e.value),
            q(&s.words()),
            q(&e.note)
        );
    }
    out
}

/// Writes the plugin's edits into the project (one change; none: the
/// file removed).
fn save(r: &mut crate::app::Ready, plugin: &str, edits: &[Edit]) -> Result<(), String> {
    let name = file_name(plugin);
    let op = if edits.is_empty() {
        let has = r
            .model
            .session()
            .and_then(|s| s.library_files(KIND).ok())
            .unwrap_or_default()
            .iter()
            .any(|(n, _)| *n == name);
        if !has {
            return Ok(());
        }
        ohl_server::Op::DeleteLibrary {
            kind: KIND.into(),
            name,
        }
    } else {
        ohl_server::Op::PutLibrary {
            kind: KIND.into(),
            name,
            text: to_text(plugin, edits),
        }
    };
    r.model
        .project_op("preset values", op)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The value as shown in its field: a string without its quotes.
fn shown_value(leaf_string: bool, toml: &str) -> String {
    if leaf_string
        && let Ok(v) = toml.parse::<Value>()
        && let Some(s) = v.as_str()
    {
        return s.to_owned();
    }
    toml.to_owned()
}

/// The TOML of what was typed: a string's quoted when the plugin's value
/// is a string.
fn typed_value(leaf_string: bool, typed: &str) -> Result<String, String> {
    if leaf_string {
        return Ok(value_text(&Value::from(typed)));
    }
    parse_value(typed).map(|v| value_text(&v))
}

/// The window (open from the Plugin manager).
pub fn window(ctx: &egui::Context, r: &mut crate::app::Ready, plugins: &crate::plugins::Plugins) {
    if !r.preset_editor.open {
        return;
    }
    crate::rule_editor::ensure_presets(ctx, r, plugins);
    r.preset_editor.read_sheets();
    r.preset_editor.poll_sheets();
    if r.preset_editor.sheet_job.is_some() || r.rule_editor.presets.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let plugin = r.preset_editor.plugin.clone();
    let (all, bad) = of_model(&r.model);
    let mut edits = all.get(&plugin).cloned().unwrap_or_default();
    let (shown, errors) = items(
        &plugin,
        &r.rule_editor.presets.original,
        r.preset_editor.sheets.as_ref(),
    );
    let texts: Vec<(String, String)> = shown
        .iter()
        .map(|s| (s.key.clone(), s.text.clone()))
        .collect();
    let st = states(&edits, &texts);
    let read_only = r.model.editing.is_err();
    let mut changed = false;
    let mut open = true;
    let busy = r.rule_editor.presets.busy() || r.preset_editor.sheet_job.is_some();
    let preset_errors = r.rule_editor.presets.edit_errors.clone();
    egui::Window::new(format!("Presets of {plugin}"))
        .id(egui::Id::new("plugin-preset-editor"))
        .open(&mut open)
        .default_width(980.0)
        .default_height(620.0)
        .show(ctx, |ui| {
            let w = &mut r.preset_editor;
            ui.label(
                egui::RichText::new(
                    "The plugin's rule presets, their weather cases and its sheet templates, \
                     value by value. A value changed here is kept in this project (the plugin \
                     is not changed) and taken wherever the presets are read: when a preset is \
                     loaded into a rule file (rule files loaded before keep their values), and \
                     in the sheets drawn with the template. Check every value against the \
                     regulation you work to.",
                )
                .small()
                .weak(),
            );
            ui.horizontal(|ui| {
                ui.selectable_value(&mut w.tab, Tab::Presets, "Presets");
                let n = edits.len();
                let warn = st.iter().filter(|s| s.warn()).count();
                let title = match (n, warn) {
                    (0, _) => "Changes".to_owned(),
                    (n, 0) => format!("Changes ({n})"),
                    (n, w) => format!("Changes ({n}, {w} to look at)"),
                };
                ui.selectable_value(&mut w.tab, Tab::Changes, title);
                if busy {
                    ui.spinner();
                    ui.label("reading the plugin…");
                }
            });
            for e in errors.iter().chain(&bad).chain(&preset_errors) {
                crate::menu::error_label(ui, e);
            }
            if read_only {
                ui.label(
                    egui::RichText::new(
                        "The project is open read-only: the values are shown, not changed.",
                    )
                    .weak(),
                );
            }
            ui.separator();
            match w.tab {
                Tab::Presets => {
                    changed |= presets_tab(ui, w, &shown, &mut edits, &st, read_only);
                }
                Tab::Changes => {
                    changed |= changes_tab(ui, w, &plugin, &shown, &mut edits, &st, read_only);
                }
            }
            if let Some((t, error)) = &w.status {
                if *error {
                    crate::menu::error_label(ui, t);
                } else {
                    ui.label(t);
                }
            }
        });
    if !open {
        r.preset_editor.open = false;
        r.preset_editor.typing = None;
    }
    if changed {
        r.preset_editor.status = match save(r, &plugin, &edits) {
            Ok(()) => None,
            Err(e) => Some((format!("not saved: {e}"), true)),
        };
    }
}

/// The Presets tab: the items on the left, the values of the one chosen
/// on the right. True when an edit changed.
fn presets_tab(
    ui: &mut egui::Ui,
    w: &mut PresetEditor,
    shown: &[Shown],
    edits: &mut Vec<Edit>,
    states: &[State],
    read_only: bool,
) -> bool {
    let mut changed = false;
    if w.item
        .as_ref()
        .is_none_or(|k| !shown.iter().any(|s| &s.key == k))
    {
        w.item = shown.first().map(|s| s.key.clone());
    }
    ui.horizontal_top(|ui| {
        egui::ScrollArea::vertical()
            .id_salt("preset-items")
            .max_width(300.0)
            .max_height(480.0)
            .show(ui, |ui| {
                ui.set_width(290.0);
                ui.vertical(|ui| {
                    if shown.is_empty() {
                        ui.label("No preset (the plugin turned off, or none read yet).");
                    }
                    for s in shown {
                        let n = edits.iter().filter(|e| e.item == s.key).count();
                        let label = if n > 0 {
                            format!("{} ({n} changed)", s.title)
                        } else {
                            s.title.clone()
                        };
                        let text = if n > 0 {
                            egui::RichText::new(label).strong()
                        } else {
                            egui::RichText::new(label)
                        };
                        if ui
                            .selectable_label(w.item.as_ref() == Some(&s.key), text)
                            .clicked()
                        {
                            w.item = Some(s.key.clone());
                            w.typing = None;
                        }
                    }
                });
            });
        ui.separator();
        ui.vertical(|ui| {
            let Some(s) = shown.iter().find(|s| Some(&s.key) == w.item.as_ref()) else {
                return;
            };
            ui.horizontal(|ui| {
                ui.label("Find");
                ui.add(egui::TextEdit::singleline(&mut w.filter).desired_width(220.0));
                ui.checkbox(&mut w.only_changed, "only the values changed");
            });
            let leaves = match leaves(&s.text) {
                Ok(l) => l,
                Err(e) => {
                    crate::menu::error_label(ui, format!("not read: {e}"));
                    return;
                }
            };
            let find = w.filter.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("preset-values")
                .max_height(440.0)
                .show(ui, |ui| {
                    egui::Grid::new("preset-values-grid")
                        .striped(true)
                        .num_columns(4)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            ui.strong("value");
                            ui.strong("in this project");
                            ui.strong("the plugin's");
                            ui.strong("");
                            ui.end_row();
                            for leaf in &leaves {
                                let at = edits
                                    .iter()
                                    .position(|e| e.item == s.key && e.path == leaf.path);
                                if w.only_changed && at.is_none() {
                                    continue;
                                }
                                let path = leaf.path.join(" › ");
                                if !find.is_empty()
                                    && !path.to_lowercase().contains(&find)
                                    && !leaf.text.to_lowercase().contains(&find)
                                {
                                    continue;
                                }
                                let label = egui::RichText::new(&path).small();
                                ui.label(if at.is_some() { label.strong() } else { label });
                                let now = at.map_or(leaf.text.clone(), |k| edits[k].value.clone());
                                let editing = w
                                    .typing
                                    .as_ref()
                                    .is_some_and(|(i, p, _)| *i == s.key && *p == leaf.path);
                                let mut text = if editing {
                                    w.typing.as_ref().map(|t| t.2.clone()).unwrap_or_default()
                                } else {
                                    shown_value(leaf.string, &now)
                                };
                                let field = ui
                                    .add_enabled_ui(!read_only, |ui| {
                                        ui.add_sized(
                                            [300.0, 20.0],
                                            egui::TextEdit::singleline(&mut text),
                                        )
                                    })
                                    .inner;
                                if field.changed() || (editing && field.has_focus()) {
                                    w.typing =
                                        Some((s.key.clone(), leaf.path.clone(), text.clone()));
                                }
                                let done = field.lost_focus();
                                if done && editing {
                                    w.typing = None;
                                    match typed_value(leaf.string, &text) {
                                        Ok(v) if v == leaf.text => {
                                            if let Some(k) = at {
                                                edits.remove(k);
                                                changed = true;
                                            }
                                        }
                                        Ok(v) => {
                                            match at {
                                                Some(k) if edits[k].value == v => {}
                                                Some(k) => {
                                                    edits[k].value = v;
                                                    changed = true;
                                                }
                                                None => {
                                                    edits.push(Edit {
                                                        item: s.key.clone(),
                                                        path: leaf.path.clone(),
                                                        original: leaf.text.clone(),
                                                        value: v,
                                                        note: String::new(),
                                                    });
                                                    changed = true;
                                                }
                                            }
                                            w.status = None;
                                        }
                                        Err(e) => w.status = Some((e, true)),
                                    }
                                }
                                match at {
                                    Some(k) => {
                                        let l = ui
                                            .label(egui::RichText::new(&leaf.text).small().weak());
                                        if states[k].warn() {
                                            l.on_hover_text(states[k].words());
                                        }
                                        if ui
                                            .add_enabled(
                                                !read_only,
                                                egui::Button::new("Reset").small(),
                                            )
                                            .on_hover_text("Back to the plugin's value")
                                            .clicked()
                                        {
                                            edits.remove(k);
                                            changed = true;
                                        }
                                    }
                                    None => {
                                        ui.label("");
                                        ui.label("");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    });
    changed
}

/// The Changes tab: every value changed, the plugin's and the project's,
/// how each stands, a note. True when an edit changed.
fn changes_tab(
    ui: &mut egui::Ui,
    w: &mut PresetEditor,
    plugin: &str,
    shown: &[Shown],
    edits: &mut Vec<Edit>,
    states: &[State],
    read_only: bool,
) -> bool {
    let mut changed = false;
    if edits.is_empty() {
        ui.label("No value changed: the presets are taken as the plugin gives them.");
        return false;
    }
    ui.horizontal(|ui| {
        if ui.button("Save as CSV…").clicked() {
            let text = csv(plugin, edits, states);
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name(format!("{plugin}-preset-changes.csv"))
                .save_file()
            {
                w.status = Some(match std::fs::write(&path, text) {
                    Ok(()) => (format!("saved {}", path.display()), false),
                    Err(e) => (format!("{}: {e}", path.display()), true),
                });
            }
        }
        if ui
            .add_enabled(!read_only, egui::Button::new("Reset all"))
            .on_hover_text("Every value back to the plugin's")
            .clicked()
        {
            edits.clear();
            changed = true;
        }
    });
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("preset-changes")
        .max_height(460.0)
        .show(ui, |ui| {
            egui::Grid::new("preset-changes-grid")
                .striped(true)
                .num_columns(7)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for h in [
                        "preset",
                        "value",
                        "the plugin's",
                        "this project's",
                        "status",
                        "note",
                        "",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (k, e) in edits.iter_mut().enumerate() {
                        let title = shown
                            .iter()
                            .find(|s| s.key == e.item)
                            .map_or(e.item.clone(), |s| s.title.clone());
                        if ui
                            .link(&title)
                            .on_hover_text("Show it in the Presets tab")
                            .clicked()
                        {
                            w.item = Some(e.item.clone());
                            w.filter = e.path.last().cloned().unwrap_or_default();
                            w.tab = Tab::Presets;
                        }
                        ui.label(egui::RichText::new(e.path.join(" › ")).small());
                        ui.label(&e.original);
                        ui.label(egui::RichText::new(&e.value).strong());
                        let s = &states[k];
                        if s.warn() {
                            crate::menu::error_label(ui, s.words());
                        } else {
                            ui.label(s.words());
                        }
                        let note = ui.add_enabled(
                            !read_only,
                            egui::TextEdit::singleline(&mut e.note)
                                .hint_text("why")
                                .desired_width(200.0),
                        );
                        if note.lost_focus() {
                            changed = true;
                        }
                        if ui
                            .add_enabled(!read_only, egui::Button::new("Reset").small())
                            .on_hover_text("Back to the plugin's value")
                            .clicked()
                        {
                            remove = Some(k);
                        }
                        ui.end_row();
                    }
                });
        });
    if let Some(k) = remove {
        edits.remove(k);
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "p"

# Ground (a comment kept).
[[rule]]
name = "g"
target = "ground"
vertical = "7 m" # the law's
cases = ["hot"]

[[rule]]
name = "w"
target = "same_line"
radial = { base = "1.5 m", per_kv = "0.01 m" }
cases = ["hot"]
"#;

    /// The values are found by name in an array of tables; an edit is
    /// applied in place (its comment kept) and said changed when the
    /// plugin's value is no longer the one it was made from, missing when
    /// the value is gone; the file reads back.
    #[test]
    fn edits_are_applied_by_path() {
        let l = leaves(RULES).unwrap();
        let v = l
            .iter()
            .find(|x| x.path == ["rule", "name=g", "vertical"])
            .unwrap();
        assert_eq!(v.text, "\"7 m\"");
        assert!(v.string);
        let inline = l
            .iter()
            .find(|x| x.path == ["rule", "name=w", "radial"])
            .unwrap();
        assert!(!inline.string);
        let e = |path: &[&str], original: &str, value: &str| Edit {
            item: "rules:p".into(),
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            original: original.into(),
            value: value.into(),
            note: String::new(),
        };
        let edits = vec![
            e(&["rule", "name=g", "vertical"], "\"7 m\"", "\"8 m\""),
            e(
                &["rule", "name=w", "radial"],
                "{ base = \"1 m\" }",
                "{ base = \"2 m\", per_kv = \"0.01 m\" }",
            ),
            e(&["rule", "name=x", "radial"], "\"1 m\"", "\"2 m\""),
            Edit {
                item: "sheet:a".into(),
                ..e(&["name"], "\"p\"", "\"q\"")
            },
        ];
        let (text, s) = apply(RULES, "rules:p", &edits);
        assert!(text.contains("vertical = \"8 m\" # the law's"), "{text}");
        assert!(text.contains("# Ground (a comment kept)."));
        assert_eq!(s[0], Some(State::Applied));
        assert!(matches!(s[1], Some(State::PluginChanged(_))));
        assert_eq!(s[2], Some(State::Missing));
        assert_eq!(s[3], None);
        let set = ohl_clearance::rules::RuleSet::parse(&text).unwrap();
        assert_eq!(set.rules.len(), 2);
        let back = parse(&to_text("lev-ch", &edits)).unwrap();
        assert_eq!(back, ("lev-ch".to_owned(), edits.clone()));
        assert!(matches!(
            apply(RULES, "rules:p", &[e(&["name"], "\"p\"", "not toml")]).1[0],
            Some(State::Invalid(_))
        ));
    }

    /// A sheet template (the LeV plugin's, when this checkout has it) is
    /// shown value by value, and an edited label reads as a template.
    #[test]
    fn a_sheet_template_is_edited() {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../local-data/plugins/lev-ch/sheets/esti-235-a1.ohlsheet");
        let Ok(text) = std::fs::read_to_string(&file) else {
            eprintln!("skipping: no local-data/plugins/lev-ch");
            return;
        };
        let l = leaves(&text).unwrap();
        let axis = l
            .iter()
            .find(|x| x.path == ["legend", "labels", "axis"])
            .unwrap();
        let e = Edit {
            item: "sheet:esti-235-a1".into(),
            path: axis.path.clone(),
            original: axis.text.clone(),
            value: "\"Achse\"".into(),
            note: String::new(),
        };
        let (edited, s) = apply(&text, "sheet:esti-235-a1", &[e]);
        assert_eq!(s, [Some(State::Applied)]);
        assert!(edited.contains("axis = \"Achse\""));
        ohl_drawing::SheetTemplate::parse(&edited, file.parent(), &[]).unwrap();
    }

    /// What is typed: a string's without quotes; another value as TOML.
    #[test]
    fn typed_values() {
        assert_eq!(typed_value(true, "8 m").unwrap(), "\"8 m\"");
        assert_eq!(shown_value(true, "\"8 m\""), "8 m");
        assert_eq!(typed_value(false, "3.5").unwrap(), "3.5");
        assert!(typed_value(false, "a b").is_err());
    }
}
