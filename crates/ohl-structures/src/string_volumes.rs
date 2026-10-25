// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Volumes on the strings** (roadmap O6, second part; owner 2026-10-19,
//! details confirmed; built 2026-10-25): the basic shapes an insulator
//! assembly carries — spheres, cylinders, boxes and cones — each with an
//! **electrical state**, how they move with the string (**freedoms** in
//! the string's own axes) and **several strings in one assembly**
//! (double, triple). The internal clearance check
//! (`ohl_clearance::internal`) measures the electrified shapes and the
//! conductors against the grounded shapes and the structure's body.
//!
//! In an `.ohli` file (version 4, written only when an assembly has any of
//! it):
//!
//! ```toml
//! swing_across = "free"     # free (default) or blocked
//! swing_along = "free"
//! twist = "blocked"
//! strings = 2               # several strings in one assembly (default 1)
//! arrangement = "across"    # across, along or triangle (three strings)
//! spacing = "0.45 m"        # between the strings' axes
//!
//! [[shape]]
//! name = "clamp"
//! kind = "box"              # sphere, cylinder, box or cone
//! state = "electrified"     # grounded, electrified or insulated
//! at_fraction = 1.0         # its centre along the string from its top
//! width = "0.6 m"           # box: across; depth: along; length: on the axis
//! depth = "0.2 m"
//! length = "0.15 m"
//! shared = true             # several strings: one for the assembly (a yoke)
//! ```
//!
//! A shape's **position** is its centre along the string from the string's
//! top, as a fraction of the string's length (`at_fraction`, 0 the top, 1
//! the wire point) or in metres (`at`), and an offset from the string's
//! axis in the string's own axes (`offset_across`, `offset_along`: an
//! arcing horn beside it). Its axis (cylinder, cone, box length) lies on
//! the string's axis. A cone's `diameter` is at its end nearer the
//! string's top, `end_diameter` (default 0, a point) at the other.
//!
//! A line's string may **override** a shape of its assembly by name (its
//! `shapes`: each replaces the assembly's shape of the same name, or adds
//! one): to leave one out at that string, override it as `insulated`.

use ohl_core::units::Length;
use serde::{Deserialize, Serialize};

/// A basic volume's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    /// A sphere (`diameter`).
    Sphere,
    /// A cylinder on the string's axis (`diameter`, `length`).
    Cylinder,
    /// A box (`width` across, `depth` along, `length` on the axis).
    Box,
    /// A cone or a truncated one on the string's axis (`diameter` at its
    /// end nearer the top, `end_diameter` at the other, `length`).
    Cone,
}

impl ShapeKind {
    /// Every kind, in the order the editors offer them.
    pub const ALL: [ShapeKind; 4] = [
        ShapeKind::Sphere,
        ShapeKind::Cylinder,
        ShapeKind::Box,
        ShapeKind::Cone,
    ];

    /// Its name in the files.
    pub fn as_str(self) -> &'static str {
        match self {
            ShapeKind::Sphere => "sphere",
            ShapeKind::Cylinder => "cylinder",
            ShapeKind::Box => "box",
            ShapeKind::Cone => "cone",
        }
    }

    /// From its name in the files.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text.trim())
    }
}

/// A shape's electrical state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElectricalState {
    /// At the structure's potential: top fittings, the tower-side horn.
    Grounded,
    /// At the line's potential: the clamp, the conductor-side horn, a
    /// corona ring, the yoke at the bottom.
    Electrified,
    /// The insulator itself: its length is insulation, not a clearance —
    /// drawn, not checked.
    Insulated,
}

impl ElectricalState {
    /// Every state, in the order the editors offer them.
    pub const ALL: [ElectricalState; 3] = [
        ElectricalState::Grounded,
        ElectricalState::Electrified,
        ElectricalState::Insulated,
    ];

    /// Its name in the files.
    pub fn as_str(self) -> &'static str {
        match self {
            ElectricalState::Grounded => "grounded",
            ElectricalState::Electrified => "electrified",
            ElectricalState::Insulated => "insulated",
        }
    }

    /// From its name in the files.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text.trim())
    }
}

/// Whether a string's shapes turn with one of its rotations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freedom {
    /// They turn with the string (the default).
    #[default]
    Free,
    /// They keep their orientation in still air.
    Blocked,
}

impl Freedom {
    /// Its name in the files.
    pub fn as_str(self) -> &'static str {
        match self {
            Freedom::Free => "free",
            Freedom::Blocked => "blocked",
        }
    }

    /// From its name in the files.
    pub fn parse(text: &str) -> Option<Self> {
        [Freedom::Free, Freedom::Blocked]
            .into_iter()
            .find(|k| k.as_str() == text.trim())
    }

    /// Free.
    pub fn is_free(self) -> bool {
        self == Freedom::Free
    }
}

/// How several strings of one assembly stand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrangement {
    /// Side by side across the line (the default).
    #[default]
    Across,
    /// One behind the other along the line.
    Along,
    /// Three strings at the corners of an equilateral triangle of side
    /// `spacing`, one corner ahead.
    Triangle,
}

impl Arrangement {
    /// Every arrangement, in the order the editors offer them.
    pub const ALL: [Arrangement; 3] = [
        Arrangement::Across,
        Arrangement::Along,
        Arrangement::Triangle,
    ];

    /// Its name in the files.
    pub fn as_str(self) -> &'static str {
        match self {
            Arrangement::Across => "across",
            Arrangement::Along => "along",
            Arrangement::Triangle => "triangle",
        }
    }

    /// From its name in the files.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text.trim())
    }
}

/// Where a shape's centre is along its string, from the string's top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Along {
    /// A fraction of the string's length (0 the top, 1 the wire point).
    Fraction(f64),
    /// Metres from the top.
    Metres(f64),
}

impl Along {
    /// Metres from the top on a string of `length` m.
    pub fn metres(self, length: f64) -> f64 {
        match self {
            Along::Fraction(f) => f * length,
            Along::Metres(m) => m,
        }
    }
}

/// One basic volume on a string, sizes in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ShapeFile", into = "ShapeFile")]
pub struct StringShape {
    /// Its name, unique on the assembly (a string's override names it).
    pub name: String,
    /// What it is.
    pub kind: ShapeKind,
    /// Its electrical state.
    pub state: ElectricalState,
    /// Where its centre is along the string.
    pub at: Along,
    /// Sphere, cylinder: its diameter; cone: at its end nearer the top, m.
    pub diameter: f64,
    /// Cone: the diameter at its end nearer the wire point, m (0: a
    /// point).
    pub end_diameter: f64,
    /// Cylinder, cone, box: its length on the string's axis, m.
    pub length: f64,
    /// Box: across, m.
    pub width: f64,
    /// Box: along, m.
    pub depth: f64,
    /// Its centre's offset from the string's axis in the string's own axes
    /// (across, along), m.
    pub offset: [f64; 2],
    /// Several strings in one assembly: one shape for the assembly, on its
    /// axis (a yoke plate), else one on each string.
    pub shared: bool,
}

/// A `[[shape]]` table as written (in an `.ohli` file: values with units;
/// in a line file: numbers in SI units).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeFile {
    name: String,
    kind: ShapeKind,
    state: ElectricalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at_fraction: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    diameter: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end_diameter: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    length: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    width: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    depth: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offset_across: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offset_along: Option<Length>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    shared: bool,
}

impl TryFrom<ShapeFile> for StringShape {
    type Error = String;

    fn try_from(f: ShapeFile) -> Result<Self, String> {
        let what = |e: String| format!("shape \"{}\": {e}", f.name);
        let at = match (f.at, f.at_fraction) {
            (Some(m), None) => Along::Metres(m.si()),
            (None, Some(x)) => Along::Fraction(x),
            (None, None) => return Err(what("give at (m) or at_fraction".into())),
            (Some(_), Some(_)) => return Err(what("give at or at_fraction, not both".into())),
        };
        // The sizes its kind takes; any other given is refused (a typo).
        let takes: &[&str] = match f.kind {
            ShapeKind::Sphere => &["diameter"],
            ShapeKind::Cylinder => &["diameter", "length"],
            ShapeKind::Box => &["width", "depth", "length"],
            ShapeKind::Cone => &["diameter", "end_diameter", "length"],
        };
        let given = [
            ("diameter", f.diameter),
            ("end_diameter", f.end_diameter),
            ("length", f.length),
            ("width", f.width),
            ("depth", f.depth),
        ];
        for (key, v) in given {
            if v.is_some() && !takes.contains(&key) {
                return Err(what(format!("{key} is not for a {}", f.kind.as_str())));
            }
        }
        let si = |v: Option<Length>| v.map_or(0.0, |l| l.si());
        let shape = StringShape {
            kind: f.kind,
            state: f.state,
            at,
            diameter: si(f.diameter),
            end_diameter: si(f.end_diameter),
            length: si(f.length),
            width: si(f.width),
            depth: si(f.depth),
            offset: [si(f.offset_across), si(f.offset_along)],
            shared: f.shared,
            name: f.name.clone(),
        };
        shape.check().map_err(what)?;
        Ok(shape)
    }
}

impl From<StringShape> for ShapeFile {
    fn from(s: StringShape) -> Self {
        let len = |v: f64| Some(Length::from_meters(v));
        let (at, at_fraction) = match s.at {
            Along::Metres(m) => (len(m), None),
            Along::Fraction(x) => (None, Some(x)),
        };
        let some = |v: f64, used: bool| if used { len(v) } else { None };
        let k = s.kind;
        ShapeFile {
            name: s.name,
            kind: k,
            state: s.state,
            at,
            at_fraction,
            diameter: some(s.diameter, k != ShapeKind::Box),
            end_diameter: some(
                s.end_diameter,
                k == ShapeKind::Cone && s.end_diameter != 0.0,
            ),
            length: some(s.length, k != ShapeKind::Sphere),
            width: some(s.width, k == ShapeKind::Box),
            depth: some(s.depth, k == ShapeKind::Box),
            offset_across: some(s.offset[0], s.offset[0] != 0.0),
            offset_along: some(s.offset[1], s.offset[1] != 0.0),
            shared: s.shared,
        }
    }
}

impl StringShape {
    /// A shape `name` of `kind`, `state`, at the string's middle, of 0.2 m
    /// each way its kind takes (to be set).
    pub fn new(name: &str, kind: ShapeKind, state: ElectricalState) -> Self {
        StringShape {
            name: name.into(),
            kind,
            state,
            at: Along::Fraction(0.5),
            diameter: 0.2,
            end_diameter: 0.0,
            length: 0.2,
            width: 0.2,
            depth: 0.2,
            offset: [0.0; 2],
            shared: false,
        }
        .sizes_of_its_kind()
    }

    /// The same with the sizes its kind does not take set to 0 (as a file
    /// reads it back).
    pub fn sizes_of_its_kind(mut self) -> Self {
        let k = self.kind;
        if k == ShapeKind::Box {
            self.diameter = 0.0;
        } else {
            self.width = 0.0;
            self.depth = 0.0;
        }
        if k != ShapeKind::Cone {
            self.end_diameter = 0.0;
        }
        if k == ShapeKind::Sphere {
            self.length = 0.0;
        }
        self
    }

    /// What is wrong with it (its sizes for its kind, its position), none:
    /// nothing.
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a shape needs a name".into());
        }
        let positive = |key: &str, v: f64| {
            if v.is_finite() && v > 0.0 && v < 50.0 {
                Ok(())
            } else {
                Err(format!("{key} must be above 0 m (and below 50 m)"))
            }
        };
        match self.kind {
            ShapeKind::Sphere => positive("diameter", self.diameter)?,
            ShapeKind::Cylinder => {
                positive("diameter", self.diameter)?;
                positive("length", self.length)?;
            }
            ShapeKind::Box => {
                positive("width", self.width)?;
                positive("depth", self.depth)?;
                positive("length", self.length)?;
            }
            ShapeKind::Cone => {
                positive("length", self.length)?;
                // NaN fails every comparison: refused too.
                let big = self.diameter.max(self.end_diameter);
                let ok =
                    self.diameter >= 0.0 && self.end_diameter >= 0.0 && big > 0.0 && big < 50.0;
                if !ok {
                    return Err(
                        "a cone's diameters must be 0 or more, one of them above 0 m".into(),
                    );
                }
            }
        }
        match self.at {
            Along::Fraction(x) if !(0.0..=1.0).contains(&x) => {
                Err("at_fraction must be from 0 (the top) to 1 (the wire point)".into())
            }
            Along::Metres(m) if !(m.is_finite() && m >= 0.0) => {
                Err("at must be 0 m (the top) or more".into())
            }
            _ if !self.offset.iter().all(|o| o.is_finite() && o.abs() < 50.0) => {
                Err("the offsets must be below 50 m".into())
            }
            _ => Ok(()),
        }
    }

    /// The shape as a `[[shape]]` table of an `.ohli` file (values with
    /// units).
    pub fn to_toml(&self) -> String {
        let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let mut out = format!(
            "\n[[shape]]\nname = {}\nkind = \"{}\"\nstate = \"{}\"\n",
            quote(&self.name),
            self.kind.as_str(),
            self.state.as_str()
        );
        match self.at {
            Along::Fraction(x) => out += &format!("at_fraction = {x:?}\n"),
            Along::Metres(m) => out += &format!("at = \"{m} m\"\n"),
        }
        let f = ShapeFile::from(self.clone());
        for (key, v) in [
            ("diameter", f.diameter),
            ("end_diameter", f.end_diameter),
            ("length", f.length),
            ("width", f.width),
            ("depth", f.depth),
            ("offset_across", f.offset_across),
            ("offset_along", f.offset_along),
        ] {
            if let Some(v) = v {
                out += &format!("{key} = \"{} m\"\n", v.si());
            }
        }
        if self.shared {
            out += "shared = true\n";
        }
        out
    }
}

/// The volumes an insulator assembly carries and how they move (all
/// optional: none given, the assembly reads and writes as before).
#[derive(Debug, Clone, PartialEq)]
pub struct StringVolumes {
    /// Its shapes.
    pub shapes: Vec<StringShape>,
    /// Whether the shapes swing across with the string.
    pub swing_across: Freedom,
    /// Whether they swing along with the string.
    pub swing_along: Freedom,
    /// Whether they twist about the string's axis (free: with the
    /// conductor at the wire point; blocked: with the structure's axes).
    pub twist: Freedom,
    /// The strings in the assembly (1 to 4; a line's string stands for
    /// all of them, on their middle).
    pub strings: u32,
    /// How several stand.
    pub arrangement: Arrangement,
    /// Between their axes, m (several strings).
    pub spacing: f64,
}

impl Default for StringVolumes {
    fn default() -> Self {
        StringVolumes {
            shapes: Vec::new(),
            swing_across: Freedom::Free,
            swing_along: Freedom::Free,
            twist: Freedom::Free,
            strings: 1,
            arrangement: Arrangement::Across,
            spacing: 0.0,
        }
    }
}

impl StringVolumes {
    /// None given.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// What is wrong (none: nothing): each shape, names unique, several
    /// strings with a spacing, a triangle of three.
    pub fn check(&self, length: f64) -> Result<(), String> {
        for (k, s) in self.shapes.iter().enumerate() {
            s.check()
                .map_err(|e| format!("shape \"{}\": {e}", s.name))?;
            if self.shapes[..k].iter().any(|t| t.name == s.name) {
                return Err(format!("shape \"{}\" given twice", s.name));
            }
            if let Along::Metres(m) = s.at
                && m > length + 1e-9
            {
                return Err(format!(
                    "shape \"{}\": at {m} m is beyond the string's length ({length} m)",
                    s.name
                ));
            }
        }
        if !(1..=4).contains(&self.strings) {
            return Err(format!("strings = {}: 1 to 4", self.strings));
        }
        if self.strings > 1
            && !(self.spacing.is_finite() && self.spacing > 0.0 && self.spacing < 50.0)
        {
            return Err("several strings need a spacing above 0 m".into());
        }
        if self.strings == 1 && self.spacing != 0.0 {
            return Err("spacing is for several strings (strings = 2 or more)".into());
        }
        if self.arrangement == Arrangement::Triangle && self.strings != 3 {
            return Err("a triangle is of three strings".into());
        }
        Ok(())
    }

    /// The shapes of a string of this assembly whose own shapes are `own`:
    /// each of `own` replaces the shape of the same name, or is added.
    pub fn with_overrides(&self, own: &[StringShape]) -> Vec<StringShape> {
        let mut out: Vec<StringShape> = self
            .shapes
            .iter()
            .map(|s| own.iter().find(|o| o.name == s.name).unwrap_or(s).clone())
            .collect();
        out.extend(
            own.iter()
                .filter(|o| !self.shapes.iter().any(|s| s.name == o.name))
                .cloned(),
        );
        out
    }

    /// The offsets of each string's axis from the assembly's axis, in the
    /// string's own axes (across, along), m: side by side across, one
    /// behind the other along, or the corners of an equilateral triangle
    /// of side `spacing` centred on the axis (one corner ahead).
    pub fn string_offsets(&self) -> Vec<[f64; 2]> {
        let n = self.strings.max(1) as usize;
        let s = self.spacing;
        match self.arrangement {
            Arrangement::Triangle if n == 3 => {
                // Circumradius s/√3; one corner ahead (along +), two back.
                let r = s / 3f64.sqrt();
                [90.0f64, 210.0, 330.0]
                    .iter()
                    .map(|a| {
                        let a = a.to_radians();
                        [r * a.cos(), r * a.sin()]
                    })
                    .collect()
            }
            arrangement => (0..n)
                .map(|k| {
                    let o = (k as f64 - (n as f64 - 1.0) / 2.0) * s;
                    if arrangement == Arrangement::Along {
                        [0.0, o]
                    } else {
                        [o, 0.0]
                    }
                })
                .collect(),
        }
    }

    /// The `key = value` lines and `[[shape]]` tables of an `.ohli` file
    /// (TOML), only what differs from the defaults.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        for (key, f) in [
            ("swing_across", self.swing_across),
            ("swing_along", self.swing_along),
            ("twist", self.twist),
        ] {
            if !f.is_free() {
                out += &format!("{key} = \"{}\"\n", f.as_str());
            }
        }
        if self.strings != 1 {
            out += &format!(
                "strings = {}\narrangement = \"{}\"\nspacing = \"{} m\"\n",
                self.strings,
                self.arrangement.as_str(),
                self.spacing
            );
        } else if self.arrangement != Arrangement::Across {
            out += &format!("arrangement = \"{}\"\n", self.arrangement.as_str());
        }
        for s in &self.shapes {
            out += &s.to_toml();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(text: &str) -> Result<StringShape, String> {
        let t: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
        t.try_into::<StringShape>().map_err(|e| e.to_string())
    }

    /// Each kind takes its sizes and refuses another's; the position is a
    /// fraction or metres, not both; round trips in TOML and in JSON (SI
    /// numbers).
    #[test]
    fn shapes_read_check_and_write() {
        let s = shape(
            "name = \"horn\"\nkind = \"cylinder\"\nstate = \"grounded\"\nat = \"0.1 m\"\n\
             diameter = \"16 mm\"\nlength = \"0.4 m\"\noffset_across = \"0.25 m\"",
        )
        .unwrap();
        assert_eq!(s.kind, ShapeKind::Cylinder);
        assert_eq!(s.at, Along::Metres(0.1));
        assert!((s.diameter - 0.016).abs() < 1e-12);
        assert_eq!(s.offset, [0.25, 0.0]);
        assert_eq!(s.at.metres(3.0), 0.1);
        assert_eq!(Along::Fraction(0.5).metres(3.0), 1.5);
        let back = shape(&s.to_toml().replace("[[shape]]", "")).unwrap();
        assert_eq!(back, s);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"diameter\":0.016"), "{json}");
        assert_eq!(serde_json::from_str::<StringShape>(&json).unwrap(), s);
        let cone = shape(
            "name = \"ring\"\nkind = \"cone\"\nstate = \"electrified\"\nat_fraction = 0.9\n\
             diameter = \"0.1 m\"\nend_diameter = \"0.4 m\"\nlength = \"0.2 m\"",
        )
        .unwrap();
        assert_eq!((cone.diameter, cone.end_diameter), (0.1, 0.4));
        for bad in [
            "kind = \"sphere\"\nstate = \"grounded\"\nat_fraction = 0.5\ndiameter = \"0 m\"",
            "kind = \"sphere\"\nstate = \"grounded\"\nat_fraction = 0.5\ndiameter = \"1 m\"\nlength = \"1 m\"",
            "kind = \"box\"\nstate = \"grounded\"\nat_fraction = 0.5\nwidth = \"1 m\"\ndepth = \"1 m\"",
            "kind = \"sphere\"\nstate = \"grounded\"\ndiameter = \"1 m\"",
            "kind = \"sphere\"\nstate = \"grounded\"\nat = \"1 m\"\nat_fraction = 0.5\ndiameter = \"1 m\"",
            "kind = \"sphere\"\nstate = \"grounded\"\nat_fraction = 1.5\ndiameter = \"1 m\"",
            "kind = \"sphere\"\nstate = \"live\"\nat_fraction = 0.5\ndiameter = \"1 m\"",
            "kind = \"torus\"\nstate = \"grounded\"\nat_fraction = 0.5\ndiameter = \"1 m\"",
            "kind = \"cone\"\nstate = \"grounded\"\nat_fraction = 0.5\ndiameter = \"0 m\"\nlength = \"1 m\"",
            "kind = \"sphere\"\nstate = \"grounded\"\nat_fraction = 0.5\ndiameter = \"1 m\"\nsize = 2",
        ] {
            assert!(shape(&format!("name = \"x\"\n{bad}")).is_err(), "{bad}");
        }
    }

    /// The string offsets of each arrangement; a string's own shapes
    /// override by name or add; the checks of several strings.
    #[test]
    fn arrangements_overrides_and_checks() {
        let mut v = StringVolumes {
            strings: 2,
            spacing: 0.4,
            ..Default::default()
        };
        assert_eq!(v.string_offsets(), vec![[-0.2, 0.0], [0.2, 0.0]]);
        v.arrangement = Arrangement::Along;
        assert_eq!(v.string_offsets(), vec![[0.0, -0.2], [0.0, 0.2]]);
        v.strings = 3;
        v.arrangement = Arrangement::Triangle;
        let o = v.string_offsets();
        // An equilateral triangle of side 0.4 centred on the axis.
        let d = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            assert!((d(o[a], o[b]) - 0.4).abs() < 1e-12);
        }
        assert!((o.iter().map(|p| p[0]).sum::<f64>()).abs() < 1e-12);
        assert!(o[0][1] > 0.0 && o[0][0].abs() < 1e-12);
        v.check(3.0).unwrap();
        v.strings = 2;
        assert!(v.check(3.0).is_err(), "a triangle of two");
        v.arrangement = Arrangement::Across;
        v.spacing = 0.0;
        assert!(v.check(3.0).is_err(), "no spacing");
        let one = StringVolumes {
            spacing: 0.3,
            ..Default::default()
        };
        assert!(one.check(3.0).is_err(), "spacing of one string");
        let a = StringShape::new("a", ShapeKind::Sphere, ElectricalState::Grounded);
        let b = StringShape::new("b", ShapeKind::Sphere, ElectricalState::Electrified);
        let mut with = StringVolumes {
            shapes: vec![a.clone(), b.clone()],
            ..Default::default()
        };
        with.check(3.0).unwrap();
        let mut b2 = b.clone();
        b2.state = ElectricalState::Insulated;
        let c = StringShape::new("c", ShapeKind::Box, ElectricalState::Electrified);
        let merged = with.with_overrides(&[b2.clone(), c.clone()]);
        assert_eq!(merged, vec![a.clone(), b2, c]);
        with.shapes.push(a.clone());
        assert!(with.check(3.0).is_err(), "a name twice");
        with.shapes.pop();
        with.shapes[0].at = Along::Metres(4.0);
        assert!(with.check(3.0).is_err(), "beyond the length");
    }
}
