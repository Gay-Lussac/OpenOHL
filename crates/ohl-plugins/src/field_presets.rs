// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Field presets from a plugin (owner 2026-10-31: "implement in the core
//! what's required for the building plugin"): the manifest's
//! `[field-input]` table (what points the host gathers for the plugin),
//! the optional `field-presets` export (where cuts and points are needed,
//! and the report files from the field's results) and the host's checks
//! of everything it gives back.
//!
//! ```toml
//! [field-input]
//! roles = ["building"]          # LiDAR class roles, by the cloud's class scheme
//! ground-roles = ["ground"]     # thinned by the host: the lowest point per cell
//! ground-cell = "2 m"
//! survey-codes = ["BLD*"]       # survey points' codes (`*`, `?`; any case)
//! reach = "100 m"               # within this of the spans, in plan
//! max-points = 2000000          # more is refused, said
//! ```
//!
//! The plugin stays a pure function: the host gathers the points before
//! the call, runs the field after it and calls `report` with the results;
//! the plugin calls nothing back.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

use crate::{Manifest, PluginError, PluginInfo, Prepared, Tier, WasmHost, classify};

mod bindings {
    wasmtime::component::bindgen!({
        world: "field-presets-export",
        path: "wit",
    });
}

mod plugin_bindings {
    wasmtime::component::bindgen!({
        world: "field-preset-plugin",
        path: "wit",
    });
}

pub use bindings::exports::ohl::plugin::field_presets::{
    Cut as PresetCut, FieldInput, FieldPreset, InputPoint, OutFile, Outline, Place,
};

/// Export name of the optional `field-presets` interface of any plugin.
pub const FIELD_PRESETS_EXPORT: &str = "ohl:plugin/field-presets@0.1.0";

/// Extension point of plugins giving field presets (the `field-presets`
/// export, 2026-10-31).
pub const FIELD_PRESETS: &str = "field-presets";

/// The most points a manifest may ask for (`max-points`): what a plugin's
/// memory may hold at all (some 60 bytes a point in the plugin).
pub const MAX_INPUT_POINTS: u64 = 50_000_000;

/// The most cuts of one preset.
pub const MAX_CUTS: usize = 1_000;

/// The most points of all the cuts of one preset, estimated before the
/// run: (2 × width / step + 1) columns times
/// ((above + [`CUT_HEIGHT_ESTIMATE`]) / step + 1) rows each.
pub const MAX_CUT_POINTS: usize = 4_000_000;

/// The height from a cut's lowest ground to its highest wire assumed by
/// the estimate of [`MAX_CUT_POINTS`], m (the real height is known only
/// in the run).
pub const CUT_HEIGHT_ESTIMATE: f64 = 100.0;

/// The most given points of one preset.
pub const MAX_PLACES: usize = 100_000;

/// The most report files.
pub const MAX_FILES: usize = 1_000;

/// `[field-input]` of a manifest: what the host gathers for the plugin's
/// `field-presets` export. Unknown keys are refused.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FieldInputTable {
    /// LiDAR class roles whose points are given (each through the cloud's
    /// class scheme; a role no class has is an error naming it).
    #[serde(default)]
    pub roles: Vec<String>,
    /// LiDAR class roles of the ground: the lowest point per
    /// `ground-cell` given.
    #[serde(default)]
    pub ground_roles: Vec<String>,
    /// The ground's cell, m (default 2 m).
    #[serde(
        default = "default_ground_cell",
        deserialize_with = "ohl_core::units::si::de::<_, ohl_core::units::Length>"
    )]
    pub ground_cell: f64,
    /// Survey points whose code matches one of these (`*` any text, `?`
    /// one character, any case).
    #[serde(default)]
    pub survey_codes: Vec<String>,
    /// Points within this of the spans in plan, m (default 100 m).
    #[serde(
        default = "default_reach",
        deserialize_with = "ohl_core::units::si::de::<_, ohl_core::units::Length>"
    )]
    pub reach: f64,
    /// More points than this (of the roles and the survey codes) are
    /// refused, said (default 2 000 000; at most [`MAX_INPUT_POINTS`]).
    #[serde(default = "default_max_points")]
    pub max_points: u64,
}

fn default_ground_cell() -> f64 {
    2.0
}

fn default_reach() -> f64 {
    100.0
}

fn default_max_points() -> u64 {
    2_000_000
}

impl Default for FieldInputTable {
    fn default() -> Self {
        Self {
            roles: Vec::new(),
            ground_roles: Vec::new(),
            ground_cell: default_ground_cell(),
            survey_codes: Vec::new(),
            reach: default_reach(),
            max_points: default_max_points(),
        }
    }
}

impl FieldInputTable {
    /// Whether the values are usable: a ground cell and a reach above
    /// 0 m, a point limit from 1 to [`MAX_INPUT_POINTS`], no empty role
    /// or code.
    pub fn check(&self) -> Result<(), String> {
        if !(self.ground_cell.is_finite() && self.ground_cell > 0.0) {
            return Err(format!("ground-cell {} m: above 0 m", self.ground_cell));
        }
        if !(self.reach.is_finite() && self.reach > 0.0) {
            return Err(format!("reach {} m: above 0 m", self.reach));
        }
        if !(1..=MAX_INPUT_POINTS).contains(&self.max_points) {
            return Err(format!(
                "max-points {}: from 1 to {MAX_INPUT_POINTS}",
                self.max_points
            ));
        }
        for (what, list) in [
            ("roles", &self.roles),
            ("ground-roles", &self.ground_roles),
            ("survey-codes", &self.survey_codes),
        ] {
            if list.iter().any(|r| r.trim().is_empty()) {
                return Err(format!("{what}: an empty entry"));
            }
        }
        Ok(())
    }
}

fn finite_positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// Checks the presets a plugin gave before anything is used: unique,
/// non-empty names; at most [`MAX_CUTS`] cuts, each in one of `spans`
/// (either way round), its fraction from 0 to 1, width and step above
/// 0 m, `above` 0 m or more, unique labels, at most [`MAX_CUT_POINTS`]
/// points over all cuts (estimated); at most [`MAX_PLACES`] points with
/// unique names, finite coordinates and an elevation or a height of 0 m
/// or more; levels above 0; outlines of three corners or more with
/// unique ids, finite, their top not below their ground.
pub fn check_presets(presets: &[FieldPreset], spans: &[(String, String)]) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for p in presets {
        if p.name.trim().is_empty() {
            return Err("a preset without a name".into());
        }
        if !names.insert(p.name.as_str()) {
            return Err(format!("preset {} twice", p.name));
        }
        check_preset(p, spans).map_err(|e| format!("preset {}: {e}", p.name))?;
    }
    Ok(())
}

fn check_preset(p: &FieldPreset, spans: &[(String, String)]) -> Result<(), String> {
    if p.cuts.len() > MAX_CUTS {
        return Err(format!("{} cuts: at most {MAX_CUTS}", p.cuts.len()));
    }
    let mut labels = BTreeSet::new();
    let mut points = 0usize;
    for (k, c) in p.cuts.iter().enumerate() {
        let what = if c.label.is_empty() {
            format!("cut {} ({}–{})", k + 1, c.from, c.to)
        } else {
            format!("cut {}", c.label)
        };
        if !spans
            .iter()
            .any(|(a, b)| (a == &c.from && b == &c.to) || (a == &c.to && b == &c.from))
        {
            return Err(format!(
                "{what}: no span between structures {} and {}",
                c.from, c.to
            ));
        }
        if let Some(f) = c.fraction
            && !(f.is_finite() && (0.0..=1.0).contains(&f))
        {
            return Err(format!("{what}: a place in the span from 0 to 1, not {f}"));
        }
        if !finite_positive(c.width) || !finite_positive(c.step) {
            return Err(format!("{what}: a width and a step above 0 m"));
        }
        if !(c.above.is_finite() && c.above >= 0.0) {
            return Err(format!("{what}: above the wires 0 m or more"));
        }
        if !c.label.is_empty() && !labels.insert(c.label.as_str()) {
            return Err(format!("{what} twice"));
        }
        let columns = (2.0 * c.width / c.step).ceil() + 1.0;
        let rows = ((c.above + CUT_HEIGHT_ESTIMATE) / c.step).ceil() + 1.0;
        points = points.saturating_add((columns * rows).min(usize::MAX as f64) as usize);
    }
    if points > MAX_CUT_POINTS {
        return Err(format!(
            "some {points} cut points (estimated with {CUT_HEIGHT_ESTIMATE} m from the ground \
             to the highest wire): at most {MAX_CUT_POINTS}"
        ));
    }
    if p.points.len() > MAX_PLACES {
        return Err(format!("{} points: at most {MAX_PLACES}", p.points.len()));
    }
    let mut names = BTreeSet::new();
    for q in &p.points {
        if q.name.trim().is_empty() {
            return Err("a point without a name".into());
        }
        if !names.insert(q.name.as_str()) {
            return Err(format!("point {} twice", q.name));
        }
        let ok = q.x.is_finite()
            && q.y.is_finite()
            && match (q.z, q.height) {
                (Some(z), _) => z.is_finite(),
                (None, Some(h)) => h.is_finite() && h >= 0.0,
                (None, None) => false,
            };
        if !ok {
            return Err(format!(
                "point {}: x, y and an elevation or a height of 0 m or more, as numbers",
                q.name
            ));
        }
    }
    for l in p.levels_ut.iter().chain(&p.levels_kv_m) {
        if !finite_positive(*l) {
            return Err(format!("a level of {l}: above 0"));
        }
    }
    let mut ids = BTreeSet::new();
    for o in &p.outlines {
        if !ids.insert(o.id.as_str()) {
            return Err(format!("outline {} twice", o.id));
        }
        let ok = o.ring.len() >= 6
            && o.ring.len().is_multiple_of(2)
            && o.ring.iter().all(|v| v.is_finite())
            && o.ground.is_finite()
            && o.top.is_finite()
            && o.top >= o.ground;
        if !ok {
            return Err(format!(
                "outline {}: three corners or more (x, y pairs) and a top not below its ground, as numbers",
                o.id
            ));
        }
    }
    Ok(())
}

/// Checks the report files a plugin gave: at most [`MAX_FILES`], each
/// named without a folder (no `/`, `\`, `:`, no leading `.`, not empty),
/// names unique, a media type given.
pub fn check_files(files: &[OutFile]) -> Result<(), String> {
    if files.len() > MAX_FILES {
        return Err(format!("{} report files: at most {MAX_FILES}", files.len()));
    }
    let mut names = BTreeSet::new();
    for f in files {
        let n = &f.name;
        let ok = !n.trim().is_empty()
            && !n.starts_with('.')
            && !n.contains(['/', '\\', ':', '\0'])
            && n.trim() == n;
        if !ok {
            return Err(format!(
                "report file \"{n}\": a name without a folder (no / \\ :, not starting with .)"
            ));
        }
        if !names.insert(n.to_lowercase()) {
            return Err(format!("report file {n} twice"));
        }
        if f.media_type.trim().is_empty() {
            return Err(format!("report file {n}: no media type"));
        }
    }
    Ok(())
}

impl WasmHost {
    /// The field-presets export of the plugin in `dir`, instantiated
    /// (none: not a WebAssembly plugin, or built without it).
    fn field_presets_export(
        &self,
        dir: &Path,
    ) -> Result<Option<(Prepared, bindings::FieldPresetsExport)>, PluginError> {
        let manifest = Manifest::load(dir)?;
        if manifest.plugin.tier != Tier::Wasm || !manifest.entry_path().is_file() {
            return Ok(None);
        }
        let mut p = self.prepare(dir, None)?;
        if p.component
            .get_export_index(None, FIELD_PRESETS_EXPORT)
            .is_none()
        {
            return Ok(None);
        }
        let fail = |e, p: &Prepared, what| classify(e, &p.manifest, what, p.limits);
        let instance = self
            .linker
            .instantiate(&mut p.store, &p.component)
            .map_err(|e| fail(e, &p, "instantiation"))?;
        let export = bindings::FieldPresetsExport::new(&mut p.store, &instance)
            .map_err(|e| fail(e, &p, "instantiation"))?;
        p.store.set_epoch_deadline(p.limits.deadline_ticks());
        Ok(Some((p, export)))
    }

    /// The export or an error naming the plugin's folder.
    fn field_presets_required(
        &self,
        dir: &Path,
    ) -> Result<(Prepared, bindings::FieldPresetsExport), PluginError> {
        self.field_presets_export(dir)?.ok_or_else(|| {
            PluginError::Runtime(format!(
                "plugin in {}: no field-presets export",
                dir.display()
            ))
        })
    }

    /// Whether the plugin in `dir` has the `field-presets` export.
    pub fn has_field_presets(&self, dir: &Path) -> Result<bool, PluginError> {
        Ok(self.field_presets_export(dir)?.is_some())
    }

    /// The parameters template of the plugin in `dir` (its
    /// `field-presets` export; without it: an error naming it).
    pub fn field_parameters_template(&self, dir: &Path) -> Result<String, PluginError> {
        let (mut p, export) = self.field_presets_required(dir)?;
        export
            .ohl_plugin_field_presets()
            .call_parameters_template(&mut p.store)
            .map_err(|e| classify(e, &p.manifest, "field-presets", p.limits))
    }

    /// The presets the plugin in `dir` gives for `input`, checked
    /// ([`check_presets`]) against the line's `spans` (structure pairs).
    /// A plugin without the export, its own error and a preset that does
    /// not pass are errors naming it.
    pub fn field_presets(
        &self,
        dir: &Path,
        input: &FieldInput,
        spans: &[(String, String)],
    ) -> Result<Vec<FieldPreset>, PluginError> {
        let (mut p, export) = self.field_presets_required(dir)?;
        let name = p.manifest.plugin.name.clone();
        let given = export
            .ohl_plugin_field_presets()
            .call_field_presets(&mut p.store, input)
            .map_err(|e| classify(e, &p.manifest, "field-presets", p.limits))?
            .map_err(|e| PluginError::Runtime(format!("plugin {name}: {e}")))?;
        check_presets(&given, spans)
            .map_err(|e| PluginError::Runtime(format!("plugin {name}: {e}")))?;
        Ok(given)
    }

    /// The report files the plugin in `dir` makes of `preset` and the
    /// field's `results` (`ohl-field-results` JSON), checked
    /// ([`check_files`]).
    pub fn field_report(
        &self,
        dir: &Path,
        parameters: &str,
        preset: &FieldPreset,
        results: &str,
    ) -> Result<Vec<OutFile>, PluginError> {
        let (mut p, export) = self.field_presets_required(dir)?;
        let name = p.manifest.plugin.name.clone();
        let files = export
            .ohl_plugin_field_presets()
            .call_report(&mut p.store, parameters, preset, results)
            .map_err(|e| classify(e, &p.manifest, "field-presets", p.limits))?
            .map_err(|e| PluginError::Runtime(format!("plugin {name}: {e}")))?;
        check_files(&files).map_err(|e| PluginError::Runtime(format!("plugin {name}: {e}")))?;
        Ok(files)
    }

    /// Describes a plugin of the `field-preset-plugin` world (`ohl plugin
    /// info`): none when it is built without the export.
    pub(crate) fn describe_field_preset_plugin(
        &self,
        dir: &Path,
    ) -> Result<Option<PluginInfo>, PluginError> {
        if !self.has_field_presets(dir)? {
            return Ok(None);
        }
        let mut p = self.prepare(dir, None)?;
        let instance = plugin_bindings::FieldPresetPlugin::instantiate(
            &mut p.store,
            &p.component,
            &self.linker,
        )
        .map_err(|e| classify(e, &p.manifest, "instantiation", p.limits))?;
        p.store.set_epoch_deadline(p.limits.deadline_ticks());
        let i = instance
            .call_describe(&mut p.store)
            .map_err(|e| classify(e, &p.manifest, "describe", p.limits))?;
        Ok(Some(PluginInfo {
            name: i.name,
            version: i.version,
            description: i.description,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const HEAD: &str =
        "[plugin]\nname=\"t\"\nversion=\"1\"\nsdk=\"0.3\"\ntier=\"wasm\"\nentry=\"t.wasm\"\n";

    fn parse(table: &str) -> Result<Manifest, PluginError> {
        Manifest::parse(&format!("{HEAD}{table}"), Path::new("/plugins/t"))
    }

    /// `[field-input]`: read with units, defaults where not given; none
    /// without the table; unknown keys and unusable values refused.
    #[test]
    fn manifest_field_input() {
        assert!(parse("").unwrap().field_input.is_none());
        let m = parse(
            "[field-input]\nroles = [\"building\"]\nground-roles = [\"ground\"]\nground-cell = \"500 mm\"\n\
             survey-codes = [\"BLD*\"]\nreach = \"0.1 km\"\nmax-points = 2000\n",
        )
        .unwrap();
        let f = m.field_input.unwrap();
        assert_eq!(f.roles, ["building"]);
        assert_eq!(f.ground_roles, ["ground"]);
        assert_eq!(f.survey_codes, ["BLD*"]);
        assert!((f.ground_cell - 0.5).abs() < 1e-12 && (f.reach - 100.0).abs() < 1e-9);
        assert_eq!(f.max_points, 2000);
        let d = parse("[field-input]\nroles = [\"building\"]\n")
            .unwrap()
            .field_input
            .unwrap();
        assert_eq!(
            (d.ground_cell, d.reach, d.max_points),
            (2.0, 100.0, 2_000_000)
        );
        for bad in [
            "colour = 1",
            "ground-cell = \"0 m\"",
            "reach = -5",
            "max-points = 0",
            "max-points = 60000000",
            "roles = [\"\"]",
            "reach = \"5 kg\"",
        ] {
            assert!(parse(&format!("[field-input]\n{bad}\n")).is_err(), "{bad}");
        }
    }

    fn preset() -> FieldPreset {
        FieldPreset {
            name: "p".into(),
            description: String::new(),
            cuts: vec![PresetCut {
                label: "a".into(),
                from: "T2".into(),
                to: "T1".into(),
                fraction: Some(0.5),
                width: 20.0,
                above: 5.0,
                step: 1.0,
            }],
            points: vec![Place {
                name: "q".into(),
                x: 1.0,
                y: 2.0,
                z: None,
                height: Some(1.5),
            }],
            levels_ut: vec![1.0],
            levels_kv_m: vec![],
            outlines: vec![Outline {
                id: "o".into(),
                ring: vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0],
                ground: 0.0,
                top: 5.0,
            }],
            notes: vec![],
        }
    }

    type Change = Box<dyn Fn(&mut Vec<FieldPreset>)>;

    /// Every value a plugin gives is checked before it is used.
    #[test]
    fn presets_are_checked() {
        let spans = [("T1".to_owned(), "T2".to_owned())];
        assert!(check_presets(&[preset()], &spans).is_ok());
        let cases: Vec<(&str, Change)> = vec![
            ("no span", Box::new(|p| p[0].cuts[0].to = "T3".into())),
            (
                "from 0 to 1",
                Box::new(|p| p[0].cuts[0].fraction = Some(1.5)),
            ),
            (
                "from 0 to 1",
                Box::new(|p| p[0].cuts[0].fraction = Some(f64::NAN)),
            ),
            ("step above 0", Box::new(|p| p[0].cuts[0].step = 0.0)),
            ("step above 0", Box::new(|p| p[0].cuts[0].width = -1.0)),
            ("0 m or more", Box::new(|p| p[0].cuts[0].above = -1.0)),
            (
                "twice",
                Box::new(|p| {
                    let c = p[0].cuts[0].clone();
                    p[0].cuts.push(c);
                }),
            ),
            ("cut points", Box::new(|p| p[0].cuts[0].step = 0.01)),
            (
                "at most 1000",
                Box::new(|p| {
                    let c = PresetCut {
                        label: String::new(),
                        ..p[0].cuts[0].clone()
                    };
                    p[0].cuts = vec![c; 1001];
                }),
            ),
            (
                "point q twice",
                Box::new(|p| {
                    let q = p[0].points[0].clone();
                    p[0].points.push(q);
                }),
            ),
            (
                "without a name",
                Box::new(|p| p[0].points[0].name = " ".into()),
            ),
            ("as numbers", Box::new(|p| p[0].points[0].height = None)),
            ("as numbers", Box::new(|p| p[0].points[0].x = f64::INFINITY)),
            ("level", Box::new(|p| p[0].levels_kv_m = vec![0.0])),
            ("outline o", Box::new(|p| p[0].outlines[0].ring.truncate(4))),
            ("outline o", Box::new(|p| p[0].outlines[0].top = -1.0)),
            (
                "preset p twice",
                Box::new(|p| {
                    let q = p[0].clone();
                    p.push(q);
                }),
            ),
        ];
        for (want, change) in cases {
            let mut p = vec![preset()];
            change(&mut p);
            let e = check_presets(&p, &spans).unwrap_err();
            assert!(e.contains(want), "{want}: {e}");
        }
    }

    /// Report files are named without a folder, once each.
    #[test]
    fn report_files_are_checked() {
        let file = |name: &str| OutFile {
            name: name.into(),
            media_type: "text/plain".into(),
            bytes: vec![],
        };
        assert!(check_files(&[file("a.csv"), file("b report.svg")]).is_ok());
        for bad in ["../x", "a/b", "a\\b", ".hidden", "", "c:x", " a"] {
            assert!(check_files(&[file(bad)]).is_err(), "{bad:?}");
        }
        assert!(check_files(&[file("a.csv"), file("A.csv")]).is_err());
        let mut f = file("a");
        f.media_type.clear();
        assert!(check_files(&[f]).is_err());
    }

    /// The test plugin's directory if built, `None` to skip.
    fn built(relative: &str) -> Option<PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins")
            .join(relative);
        let manifest = Manifest::load(&dir).unwrap();
        if manifest.entry_path().exists() {
            return Some(dir);
        }
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "plugin not built: {}",
            manifest.entry_path().display()
        );
        eprintln!("skipping: {relative} not built");
        None
    }

    fn input(parameters: &str) -> FieldInput {
        let point = |class: u8, code: &str| InputPoint {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            class,
            code: code.into(),
            line: String::new(),
        };
        FieldInput {
            parameters: parameters.into(),
            model: "{\"format\": \"ohl-export-model\"}".into(),
            points: vec![point(6, ""), point(0, "BLD1")],
            ground: vec![0.0, 0.0, 400.0],
        }
    }

    /// The test plugin's presets, checked; its report; the faults it is
    /// asked for refused with its name; an old plugin loads and has none.
    #[test]
    fn plugins_give_field_presets() {
        let Some(dir) = built("tests/field-presets") else {
            return;
        };
        let host = WasmHost::new().unwrap();
        assert!(host.has_field_presets(&dir).unwrap());
        let m = Manifest::load(&dir).unwrap();
        assert_eq!(m.field_input.unwrap().roles, ["building"]);
        assert!(
            host.field_parameters_template(&dir)
                .unwrap()
                .contains("Invented")
        );
        let spans = [
            ("T1".to_owned(), "T2".to_owned()),
            ("T2".to_owned(), "T3".to_owned()),
        ];
        let presets = host.field_presets(&dir, &input(""), &spans).unwrap();
        let names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["default", "points-only"]);
        let p = &presets[0];
        assert_eq!(p.cuts[0].label, "house-A");
        assert_eq!(
            p.notes[0],
            "received 2 points (1 survey, classes 6), 1 ground points"
        );
        assert_eq!(p.notes[1], "model: ohl-export-model");
        let files = host
            .field_report(&dir, "", p, "{\"format\":\"ohl-field-results\"}")
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "summary.txt");
        let text = String::from_utf8(files[0].bytes.clone()).unwrap();
        assert!(
            text.contains("preset default\ncuts 2\npoints 2\n"),
            "{text}"
        );
        assert!(text.contains("ohl-field-results true"), "{text}");
        for (bad, want) in [
            ("span", "no span between structures T1 and T3"),
            ("fraction", "from 0 to 1"),
            ("name", "point house-A twice"),
            ("fail", "asked to fail"),
        ] {
            let e = host
                .field_presets(&dir, &input(&format!("bad = \"{bad}\"")), &spans)
                .unwrap_err()
                .to_string();
            assert!(
                e.contains("field-presets") && e.contains(want),
                "{bad}: {e}"
            );
        }
        let e = host
            .field_report(&dir, "bad = \"file\"", p, "{}")
            .unwrap_err()
            .to_string();
        assert!(e.contains("../x.txt"), "{e}");
        // `ohl plugin info`: its own world, the export listed.
        let d = host.describe_any(&dir).unwrap();
        assert_eq!(d.worlds, ["field-preset-plugin"]);
        assert_eq!(d.info.name, "field-presets");
        assert!(d.details.iter().any(|l| l.contains("field-presets")));
        // A plugin built before the export: loads as it is, has none, and
        // asking for presets is an error naming it.
        if let Some(old) = built("tests/code-old") {
            assert!(!host.has_field_presets(&old).unwrap());
            assert!(host.load_code(&old).is_ok());
            assert!(host.field_presets(&old, &input(""), &spans).is_err());
            assert!(
                !host
                    .describe_any(&old)
                    .unwrap()
                    .details
                    .iter()
                    .any(|l| l.contains("field-presets"))
            );
        }
    }
}
