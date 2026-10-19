// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The body generators' records (`[[generator]]`) and mirror symmetry
//! (`[[symmetry]]`) of a structure file: read from the tables the core
//! keeps unread, checked, written back. What a generator builds is in
//! [`crate::generators`]; applying and rebuilding it in
//! [`mod@crate::rebuild`].
//!
//! ```toml
//! [[generator]]
//! id = "body"
//! kind = "panels"            # legs and panels: a frustum
//! feet = "6 m"               # width at the feet (or [across, along])
//! waist = "2 m"              # width at the top of the last panel
//! # bottom = "0 m"           # level of the feet (default: the file's foot)
//! panel = [
//!   { top = "8 m", bracing = "x", horizontal = true },
//!   { top = "14 m", bracing = "k" },
//! ]
//! # Options (none written: off). Per panel: `width = "3 m"` (or [across,
//! # along]) at its top, the body's slope changing there; `redundants =
//! # "one"` or `"two"` (secondary members in its faces); `plan = "x"` or
//! # `"diamond"` (plan bracing at its top). For the whole body:
//! # plan_auto = "x"          # plan bracing where the slope changes and
//! #                          # at the crossarms' root levels
//!
//! [[generator]]
//! id = "arm1"
//! kind = "crossarm"          # from a body face to points of the structure
//! points = ["A1", "A2"]
//! shape = "triangular"       # or "rectangular"
//! # one_arm = true           # the points on one side at one level make
//! #                          # one arm (the outermost at its tip)
//!
//! [[generator]]
//! id = "peak"
//! kind = "peak"              # from the top of the body to a tip
//! points = ["E1"]            # earth-wire points (none: one tip at the centre)
//! # height = "4 m"           # with no points: the tip above the top
//!
//! [[generator]]
//! id = "back"
//! kind = "copy_face"         # a face of a panels body to the opposite face
//! body = "body"
//! face = "y-"
//!
//! [[symmetry]]
//! mirror = "xy"              # "x": about x = 0, "y": about y = 0, "xy": both
//! # groups = ["body-leg"]    # only members of these groups (default: all)
//! ```

use ohl_core::units::Length;
use ohl_structures::StructureType;
use serde::Deserialize;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

/// The format version that has what this module writes.
pub const BODY_VERSION: i64 = 3;

/// Format version 6: it has the generators' options (a panel's `width`,
/// `redundants` and `plan`, a body's `plan_auto`, a crossarm's `one_arm`)
/// and the recipes' pressures of the line (`structure_wind`,
/// `structure_wind_factored`). A file is raised to it only when it uses
/// one: a generator using an option is written
/// ([`Generator::uses_options`]), or a recipe's load takes such a
/// pressure ([`crate::raise_for_line_pressures`], after every edit).
pub const OPTIONS_VERSION: i64 = 6;

/// A bracing pattern of a panel's faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Bracing {
    /// One diagonal, from the bottom left to the top right of each face.
    Single,
    /// Both diagonals (crossing without a joint).
    #[default]
    X,
    /// Two diagonals from the bottom corners to a joint in the middle of
    /// the top edge (which then has its horizontal, split there).
    K,
    /// A diamond between joints in the middle of the four edges (legs
    /// and horizontals split there; both horizontals written).
    Diamond,
    /// One diagonal, its direction alternating from panel to panel (a
    /// zig-zag along the height).
    Warren,
}

impl Bracing {
    /// Every pattern, in the order offered.
    pub const ALL: [Bracing; 5] = [
        Bracing::Single,
        Bracing::X,
        Bracing::K,
        Bracing::Diamond,
        Bracing::Warren,
    ];

    /// As written in files.
    pub fn key(self) -> &'static str {
        match self {
            Bracing::Single => "single",
            Bracing::X => "x",
            Bracing::K => "k",
            Bracing::Diamond => "diamond",
            Bracing::Warren => "warren",
        }
    }
}

/// Redundants (secondary members) in a panel's faces: they split the
/// main diagonals (and the legs at mid-panel) into shorter buckling
/// lengths, each new joint braced to joints already held so the face
/// stays triangulated (no mechanism in its plane). The patterns per
/// bracing are in the user guide and [`crate::generators`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Redundants {
    /// None (as before).
    #[default]
    None,
    /// One level: the main diagonals split at their middle (X: at the
    /// crossing), braced to the legs' middles.
    One,
    /// Two levels: also the halves split at their middle, braced to a
    /// leg middle or a corner.
    Two,
}

impl Redundants {
    /// Every choice, in the order offered.
    pub const ALL: [Redundants; 3] = [Redundants::None, Redundants::One, Redundants::Two];

    /// As written in files.
    pub fn key(self) -> &'static str {
        match self {
            Redundants::None => "none",
            Redundants::One => "one",
            Redundants::Two => "two",
        }
    }
}

/// Plan bracing: horizontal bracing across the body at a level (the
/// level's horizontals written too).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    /// None (as before).
    #[default]
    None,
    /// Both diagonals of the plan, corner to corner (crossing without a
    /// joint).
    X,
    /// A diamond between the middles of the four horizontals (joints
    /// there, the horizontals split) and a cross from them to a joint at
    /// the centre (without it the diamond turns in plan).
    Diamond,
}

impl Plan {
    /// Every choice, in the order offered.
    pub const ALL: [Plan; 3] = [Plan::None, Plan::X, Plan::Diamond];

    /// As written in files.
    pub fn key(self) -> &'static str {
        match self {
            Plan::None => "none",
            Plan::X => "x",
            Plan::Diamond => "diamond",
        }
    }
}

/// One panel of a panels body: from the level below to its `top`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Panel {
    /// Level of its top, m.
    pub top: f64,
    /// The pattern of its four faces.
    pub bracing: Bracing,
    /// Horizontals at its top (K and diamond write those they need).
    pub horizontal: bool,
    /// The body's width at its top, across and along, m (none: on the
    /// straight line between the widths given below and above; the last
    /// panel's is the waist). Where one is given the slope may change.
    pub width: Option<[f64; 2]>,
    /// Redundants in its faces.
    pub redundants: Redundants,
    /// Plan bracing at its top.
    pub plan: Plan,
}

/// Legs and panels: the body as a frustum, square or rectangular in plan
/// (or frustums on each other, with widths given per panel).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Panels {
    /// Level of the feet, m (none: the file's `foot`).
    pub bottom: Option<f64>,
    /// Width at the feet across the line (x) and along it (y), m.
    pub feet: [f64; 2],
    /// Width at the top of the last panel (the waist), m.
    pub waist: [f64; 2],
    /// The panels, bottom to top.
    pub panels: Vec<Panel>,
    /// Plan bracing at the levels where the body's slope changes and at
    /// the levels a crossarm's roots are on (a panel's own `plan` goes
    /// first).
    pub plan_auto: Plan,
}

/// The shape of a crossarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmShape {
    /// Four chords from the body face to one tip joint at the point.
    #[default]
    Triangular,
    /// Constant depth: a bottom tip at the point, a top tip above it at
    /// the level of the upper root joints, a vertical and two diagonals.
    Rectangular,
}

impl ArmShape {
    /// As written in files.
    pub fn key(self) -> &'static str {
        match self {
            ArmShape::Triangular => "triangular",
            ArmShape::Rectangular => "rectangular",
        }
    }
}

/// Crossarms from a face of the body to points of the structure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Crossarm {
    /// The points (labels) at the arms' tips.
    pub points: Vec<String>,
    /// Their shape.
    pub shape: ArmShape,
    /// The points on one side of the body whose arms would start from the
    /// same root joints make one arm: the outermost at its tip, the others
    /// on joints along it (a section of the arm at each). Off: an arm to
    /// each point.
    pub one_arm: bool,
}

/// A peak (earth-wire peak) on the top of the body.
#[derive(Debug, Clone, PartialEq)]
pub struct Peak {
    /// Earth-wire points (labels): a tip at each; none: one tip at the
    /// centre, `height` above the top.
    pub points: Vec<String>,
    /// With no points: the tip's height above the top, m.
    pub height: f64,
}

/// A face of a frustum (by the side of the axis it faces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Right of the line (x > 0).
    XPlus,
    /// Left (x < 0).
    XMinus,
    /// Ahead (y > 0).
    YPlus,
    /// Back (y < 0).
    YMinus,
}

impl Face {
    /// Every face.
    pub const ALL: [Face; 4] = [Face::XPlus, Face::XMinus, Face::YPlus, Face::YMinus];

    /// As written in files.
    pub fn key(self) -> &'static str {
        match self {
            Face::XPlus => "x+",
            Face::XMinus => "x-",
            Face::YPlus => "y+",
            Face::YMinus => "y-",
        }
    }

    fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.key() == k)
    }

    /// The axis it faces (0: x, 1: y) and the side (±1).
    pub fn axis(self) -> (usize, f64) {
        match self {
            Face::XPlus => (0, 1.0),
            Face::XMinus => (0, -1.0),
            Face::YPlus => (1, 1.0),
            Face::YMinus => (1, -1.0),
        }
    }
}

/// A face of a panels body copied to the opposite face (the members on
/// it mirrored; those there already are not doubled).
#[derive(Debug, Clone, PartialEq)]
pub struct CopyFace {
    /// The panels generator whose face it is.
    pub body: String,
    /// The face copied.
    pub face: Face,
}

/// What a generator builds, with its inputs.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Legs and panels.
    Panels(Panels),
    /// Crossarms.
    Crossarm(Crossarm),
    /// A peak.
    Peak(Peak),
    /// Copy a face.
    CopyFace(CopyFace),
}

impl Kind {
    /// As written in files.
    pub fn key(&self) -> &'static str {
        match self {
            Kind::Panels(_) => "panels",
            Kind::Crossarm(_) => "crossarm",
            Kind::Peak(_) => "peak",
            Kind::CopyFace(_) => "copy_face",
        }
    }

    /// In words.
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Panels(_) => "legs and panels",
            Kind::Crossarm(_) => "crossarms",
            Kind::Peak(_) => "peak",
            Kind::CopyFace(_) => "copy a face",
        }
    }
}

/// A body generator: its id (the `made_by` of what it writes) and what
/// it builds.
#[derive(Debug, Clone, PartialEq)]
pub struct Generator {
    /// Id, unique among the generators.
    pub id: String,
    /// What it builds, with its inputs.
    pub kind: Kind,
}

/// A width: one length (square) or across and along.
#[derive(Deserialize)]
#[serde(untagged)]
enum WidthFile {
    Square(Length),
    Both([Length; 2]),
}

impl WidthFile {
    fn get(&self) -> [f64; 2] {
        match self {
            WidthFile::Square(w) => [w.si(); 2],
            WidthFile::Both([a, b]) => [a.si(), b.si()],
        }
    }
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelFile {
    top: Length,
    #[serde(default)]
    bracing: Bracing,
    #[serde(default = "yes")]
    horizontal: bool,
    #[serde(default)]
    width: Option<WidthFile>,
    #[serde(default)]
    redundants: Redundants,
    #[serde(default)]
    plan: Plan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelsFile {
    #[serde(default)]
    bottom: Option<Length>,
    feet: WidthFile,
    waist: WidthFile,
    panel: Vec<PanelFile>,
    #[serde(default)]
    plan_auto: Plan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CrossarmFile {
    points: Vec<String>,
    #[serde(default)]
    shape: ArmShape,
    #[serde(default)]
    one_arm: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeakFile {
    #[serde(default)]
    points: Vec<String>,
    #[serde(default)]
    height: Option<Length>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyFaceFile {
    body: String,
    face: String,
}

fn typed<T: for<'de> Deserialize<'de>>(t: toml::Table) -> Result<T, String> {
    toml::Value::Table(t)
        .try_into()
        .map_err(|e: toml::de::Error| e.message().to_owned())
}

impl Generator {
    /// From a `[[generator]]` table.
    pub fn from_table(t: &toml::Table) -> Result<Self, String> {
        let id = t
            .get("id")
            .and_then(toml::Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or("a generator without an id")?
            .to_owned();
        let bad = |m: String| format!("generator {id}: {m}");
        let kind = t
            .get("kind")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| bad("no kind (panels, crossarm, peak, copy_face)".into()))?;
        let mut rest = t.clone();
        rest.remove("id");
        rest.remove("kind");
        let kind = match kind {
            "panels" => {
                let f: PanelsFile = typed(rest).map_err(bad)?;
                Kind::Panels(Panels {
                    bottom: f.bottom.map(Length::si),
                    feet: f.feet.get(),
                    waist: f.waist.get(),
                    panels: f
                        .panel
                        .into_iter()
                        .map(|p| Panel {
                            top: p.top.si(),
                            bracing: p.bracing,
                            horizontal: p.horizontal,
                            width: p.width.as_ref().map(WidthFile::get),
                            redundants: p.redundants,
                            plan: p.plan,
                        })
                        .collect(),
                    plan_auto: f.plan_auto,
                })
            }
            "crossarm" => {
                let f: CrossarmFile = typed(rest).map_err(bad)?;
                Kind::Crossarm(Crossarm {
                    points: f.points,
                    shape: f.shape,
                    one_arm: f.one_arm,
                })
            }
            "peak" => {
                let f: PeakFile = typed(rest).map_err(bad)?;
                Kind::Peak(Peak {
                    points: f.points,
                    height: f.height.map_or(0.0, Length::si),
                })
            }
            "copy_face" => {
                let f: CopyFaceFile = typed(rest).map_err(bad)?;
                Kind::CopyFace(CopyFace {
                    body: f.body,
                    face: Face::from_key(&f.face)
                        .ok_or_else(|| bad(format!("face \"{}\": x+, x-, y+ or y-", f.face)))?,
                })
            }
            other => {
                return Err(bad(format!(
                    "kind \"{other}\": panels, crossarm, peak or copy_face"
                )));
            }
        };
        let g = Self {
            id: id.clone(),
            kind,
        };
        g.check().map_err(bad)?;
        Ok(g)
    }

    /// Its inputs make sense.
    pub fn check(&self) -> Result<(), String> {
        match &self.kind {
            Kind::Panels(p) => {
                if p.panels.is_empty() {
                    return Err("no panel".into());
                }
                if p.feet
                    .iter()
                    .chain(&p.waist)
                    .any(|w| w.is_nan() || *w <= 0.0)
                {
                    return Err("the widths must be more than 0".into());
                }
                let mut below = p.bottom.unwrap_or(f64::NEG_INFINITY);
                for (k, panel) in p.panels.iter().enumerate() {
                    if panel.top.is_nan() || panel.top <= below + 1e-3 {
                        return Err(format!(
                            "panel {}: its top must be above the level below",
                            k + 1
                        ));
                    }
                    below = panel.top;
                    if panel
                        .width
                        .is_some_and(|w| w.iter().any(|v| v.is_nan() || *v <= 0.0))
                    {
                        return Err(format!("panel {}: its width must be more than 0", k + 1));
                    }
                }
                if p.panels.last().is_some_and(|x| x.width.is_some()) {
                    return Err(
                        "the last panel's width is the waist: give the waist instead".into(),
                    );
                }
            }
            Kind::Crossarm(c) => {
                if c.points.is_empty() {
                    return Err("no point".into());
                }
            }
            Kind::Peak(p) => {
                if p.points.is_empty() && (p.height.is_nan() || p.height <= 0.0) {
                    return Err("a height above the top, or earth-wire points".into());
                }
            }
            Kind::CopyFace(c) => {
                if c.body.trim().is_empty() {
                    return Err("the panels generator whose face it copies".into());
                }
            }
        }
        Ok(())
    }

    /// Whether its record holds one of the options of version
    /// [`OPTIONS_VERSION`] (all off: it is written as before).
    pub fn uses_options(&self) -> bool {
        match &self.kind {
            Kind::Panels(p) => {
                p.plan_auto != Plan::None
                    || p.panels.iter().any(|panel| {
                        panel.width.is_some()
                            || panel.redundants != Redundants::None
                            || panel.plan != Plan::None
                    })
            }
            Kind::Crossarm(c) => c.one_arm,
            Kind::Peak(_) | Kind::CopyFace(_) => false,
        }
    }

    /// As a `[[generator]]` table.
    pub fn to_table(&self) -> Table {
        let mut t = Table::new();
        t.insert("id", toml_edit::value(self.id.clone()));
        t.insert("kind", toml_edit::value(self.kind.key()));
        let len = |v: f64| Value::from(format!("{} m", round3(v)));
        let width = |w: [f64; 2]| {
            if (w[0] - w[1]).abs() < 5e-4 {
                Item::Value(len(w[0]))
            } else {
                Item::Value(Value::Array(Array::from_iter([len(w[0]), len(w[1])])))
            }
        };
        let labels = |l: &[String]| Item::Value(Value::Array(Array::from_iter(l.iter())));
        match &self.kind {
            Kind::Panels(p) => {
                if let Some(b) = p.bottom {
                    t.insert("bottom", Item::Value(len(b)));
                }
                t.insert("feet", width(p.feet));
                t.insert("waist", width(p.waist));
                let mut a = Array::new();
                for panel in &p.panels {
                    let mut it = InlineTable::new();
                    it.insert("top", len(panel.top));
                    it.insert("bracing", Value::from(panel.bracing.key()));
                    it.insert("horizontal", Value::from(panel.horizontal));
                    // The options only when on (a body without them is
                    // written as before).
                    if let Some(w) = panel.width
                        && let Item::Value(v) = width(w)
                    {
                        it.insert("width", v);
                    }
                    if panel.redundants != Redundants::None {
                        it.insert("redundants", Value::from(panel.redundants.key()));
                    }
                    if panel.plan != Plan::None {
                        it.insert("plan", Value::from(panel.plan.key()));
                    }
                    a.push(Value::InlineTable(it));
                }
                // One panel per line.
                for v in a.iter_mut() {
                    v.decor_mut().set_prefix("\n  ");
                }
                a.set_trailing("\n");
                a.set_trailing_comma(true);
                t.insert("panel", Item::Value(Value::Array(a)));
                if p.plan_auto != Plan::None {
                    t.insert("plan_auto", toml_edit::value(p.plan_auto.key()));
                }
            }
            Kind::Crossarm(c) => {
                t.insert("points", labels(&c.points));
                t.insert("shape", toml_edit::value(c.shape.key()));
                if c.one_arm {
                    t.insert("one_arm", toml_edit::value(true));
                }
            }
            Kind::Peak(p) => {
                if !p.points.is_empty() {
                    t.insert("points", labels(&p.points));
                }
                if p.height > 0.0 {
                    t.insert("height", Item::Value(len(p.height)));
                }
            }
            Kind::CopyFace(c) => {
                t.insert("body", toml_edit::value(c.body.clone()));
                t.insert("face", toml_edit::value(c.face.key()));
            }
        }
        t
    }
}

fn round3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// The mirror planes of a symmetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mirror {
    /// About the plane x = 0: left and right of the line.
    X,
    /// About the plane y = 0: ahead and back.
    Y,
    /// About both.
    Xy,
}

impl Mirror {
    /// Every choice.
    pub const ALL: [Mirror; 3] = [Mirror::X, Mirror::Y, Mirror::Xy];

    /// As written in files.
    pub fn key(self) -> &'static str {
        match self {
            Mirror::X => "x",
            Mirror::Y => "y",
            Mirror::Xy => "xy",
        }
    }

    /// In words.
    pub fn name(self) -> &'static str {
        match self {
            Mirror::X => "x (left ↔ right)",
            Mirror::Y => "y (ahead ↔ back)",
            Mirror::Xy => "both",
        }
    }

    /// The images it makes: the signs each image gives x, y, z.
    pub fn signs(self) -> Vec<[f64; 3]> {
        match self {
            Mirror::X => vec![[-1.0, 1.0, 1.0]],
            Mirror::Y => vec![[1.0, -1.0, 1.0]],
            Mirror::Xy => vec![[-1.0, 1.0, 1.0], [1.0, -1.0, 1.0], [-1.0, -1.0, 1.0]],
        }
    }
}

/// A mirror symmetry kept while editing.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Symmetry {
    /// The mirror planes.
    pub mirror: Mirror,
    /// Only the members of these groups (and their joints); empty: all.
    #[serde(default)]
    pub groups: Vec<String>,
}

/// The generators of `t`, in file order.
pub fn generators(t: &StructureType) -> Result<Vec<Generator>, String> {
    t.generators.iter().map(Generator::from_table).collect()
}

/// The symmetries of `t`.
pub fn symmetries(t: &StructureType) -> Result<Vec<Symmetry>, String> {
    t.symmetry
        .iter()
        .map(|s| typed(s.clone()).map_err(|e| format!("symmetry: {e}")))
        .collect()
}

/// The generators and symmetries of `t` read (ids unique), or why not.
pub fn check(t: &StructureType) -> Result<(), String> {
    let gens = generators(t)?;
    let mut seen = std::collections::BTreeSet::new();
    for g in &gens {
        if !seen.insert(g.id.as_str()) {
            return Err(format!("generator {}: given twice", g.id));
        }
    }
    symmetries(t)?;
    Ok(())
}

/// Raises the file's `version` to the one with body building (a file
/// without a version stays without).
pub fn ensure_version(doc: &mut DocumentMut) {
    if doc
        .get("version")
        .and_then(Item::as_integer)
        .is_some_and(|v| v < BODY_VERSION)
    {
        doc["version"] = toml_edit::value(BODY_VERSION);
    }
}

/// The record of generator `g` written (replacing one with its id); the
/// file raised to [`OPTIONS_VERSION`] when it uses an option.
pub fn set_generator(doc: &mut DocumentMut, g: &Generator) {
    ensure_version(doc);
    if g.uses_options() {
        crate::edits::raise_version(doc, OPTIONS_VERSION);
    }
    let table = g.to_table();
    let a = crate::edits::array(doc, "generator");
    let at = a
        .iter()
        .position(|t| t.get("id").and_then(Item::as_str) == Some(g.id.as_str()));
    match at.and_then(|k| a.get_mut(k)) {
        Some(old) => {
            // Its place and the comments before it stay.
            let (decor, position) = (old.decor().clone(), old.position());
            *old = table;
            *old.decor_mut() = decor;
            old.set_position(position);
        }
        None => a.push(table),
    }
}

/// The record of generator `id` removed (what it made stays: see
/// [`crate::rebuild::remove_generator`]).
pub fn drop_generator(doc: &mut DocumentMut, id: &str) {
    crate::edits::remove_item(doc, "generator", "id", id);
}

/// The symmetry option: `mirror` (none: no symmetry). The first
/// `[[symmetry]]` takes it (its groups stay); none: all are removed.
pub fn set_symmetry(doc: &mut DocumentMut, mirror: Option<Mirror>) {
    match mirror {
        None => {
            doc.remove("symmetry");
        }
        Some(m) => {
            ensure_version(doc);
            let a = crate::edits::array(doc, "symmetry");
            if a.is_empty() {
                a.push(Table::new());
            }
            if let Some(first) = a.get_mut(0) {
                first.insert("mirror", toml_edit::value(m.key()));
            }
        }
    }
}

/// The symmetry option as shown: the first `[[symmetry]]`'s mirror.
pub fn symmetry_option(t: &StructureType) -> Option<Mirror> {
    symmetries(t).ok()?.first().map(|s| s.mirror)
}
