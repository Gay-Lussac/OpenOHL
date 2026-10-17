// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Objects on the wires: what is put on a labelled point of a span or a
//! jumper ([`crate::SpanPoint`], [`crate::JumperPoint`]) — one object per
//! point — and what lies along a stretch between two points
//! ([`SpreadObject`]). Their types come from a library of point and
//! spread loads ([`ObjectLibrary`], `.olsload`): templates for the most
//! used, each object keeping its own values (taken from its type when
//! chosen, as strings keep their insulator's).
//!
//! See `docs/development/line-points-and-objects.md`.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// What an object on a point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// A body hanging on the wire: a marker ball, a light, a weight.
    #[default]
    Body,
    /// A clamp holding a jumper (a tap) or a spacer: a hinge passing
    /// forces only.
    Clamp,
}

impl ObjectKind {
    /// In words.
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::Body => "body",
            ObjectKind::Clamp => "clamp",
        }
    }
}

/// How a cylinder's axis lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Along the wire.
    #[default]
    Along,
    /// Horizontal, across the line.
    Across,
    /// Vertical.
    Vertical,
}

/// A body's shape, for the wind and the ice on it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shape {
    /// No size: no wind nor ice (a weight).
    #[default]
    Point,
    /// A sphere (a marker ball).
    Sphere {
        /// Diameter, m.
        diameter: f64,
    },
    /// A cylinder (a light, a clamp).
    Cylinder {
        /// Diameter, m.
        diameter: f64,
        /// Length, m.
        length: f64,
        /// Its axis.
        #[serde(default)]
        axis: Axis,
    },
    /// A box.
    Box {
        /// Along the wire, m.
        along: f64,
        /// Across the line, m.
        across: f64,
        /// Height, m.
        height: f64,
    },
    /// Given areas.
    Areas {
        /// Wind area (bare), m².
        wind_area: f64,
        /// Surface taking the ice, m².
        ice_area: f64,
    },
}

impl Shape {
    /// Its name in the files.
    pub fn as_str(&self) -> &'static str {
        match self {
            Shape::Point => "point",
            Shape::Sphere { .. } => "sphere",
            Shape::Cylinder { .. } => "cylinder",
            Shape::Box { .. } => "box",
            Shape::Areas { .. } => "areas",
        }
    }

    /// Its sizes hold (none negative).
    pub fn validate(&self) -> Result<(), String> {
        let ok = |x: f64| x.is_finite() && x >= 0.0;
        let fine = match *self {
            Shape::Point => true,
            Shape::Sphere { diameter } => ok(diameter),
            Shape::Cylinder {
                diameter, length, ..
            } => ok(diameter) && ok(length),
            Shape::Box {
                along,
                across,
                height,
            } => ok(along) && ok(across) && ok(height),
            Shape::Areas {
                wind_area,
                ice_area,
            } => ok(wind_area) && ok(ice_area),
        };
        if fine {
            Ok(())
        } else {
            Err(format!("{}: sizes must be ≥ 0", self.as_str()))
        }
    }

    /// Volume of ice, m³, and wind area, m², under radial ice `t` (m),
    /// the wind across the line. A sphere: a shell of
    /// \\( \pi/6\,((D+2t)^3 - D^3) \\), its iced area \\( \pi (D+2t)^2/4
    /// \\); a cylinder and a box: a layer t on their surface, their
    /// projected sizes grown by 2t; given areas: a layer S·t, the wind
    /// area grown as a sphere's of that area.
    pub fn ice_and_wind(&self, t: f64) -> (f64, f64) {
        use std::f64::consts::PI;
        let t = t.max(0.0);
        match *self {
            Shape::Point => (0.0, 0.0),
            Shape::Sphere { diameter: d } => {
                if d <= 0.0 {
                    return (0.0, 0.0);
                }
                (
                    PI / 6.0 * ((d + 2.0 * t).powi(3) - d.powi(3)),
                    PI / 4.0 * (d + 2.0 * t).powi(2),
                )
            }
            Shape::Cylinder {
                diameter: d,
                length: l,
                axis,
            } => {
                if d <= 0.0 {
                    return (0.0, 0.0);
                }
                let surface = PI * d * l + 2.0 * PI / 4.0 * d * d;
                let (dd, ll) = (d + 2.0 * t, l + 2.0 * t);
                let wind = match axis {
                    // The wind across the line sees its side.
                    Axis::Along | Axis::Vertical => dd * ll,
                    // Along its axis: its end.
                    Axis::Across => PI / 4.0 * dd * dd,
                };
                (surface * t, wind)
            }
            Shape::Box {
                along,
                across,
                height,
            } => {
                let surface = 2.0 * (along * across + along * height + across * height);
                (surface * t, (along + 2.0 * t) * (height + 2.0 * t))
            }
            Shape::Areas {
                wind_area,
                ice_area,
            } => {
                let bare = (4.0 * wind_area.max(0.0) / PI).sqrt();
                let wind = if bare > 0.0 {
                    PI / 4.0 * (bare + 2.0 * t).powi(2)
                } else {
                    0.0
                };
                (ice_area.max(0.0) * t, wind)
            }
        }
    }
}

/// The object on a point (one per point).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
// (No deny_unknown_fields: serde does not check them with a flattened
// shape.)
pub struct LineObject {
    /// Body or clamp.
    #[serde(default)]
    pub kind: ObjectKind,
    /// The library type it was taken from (its values are its own).
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    /// Weight, N (the whole bundle's).
    #[serde(default)]
    pub weight: f64,
    /// Its shape, for the wind and the ice.
    #[serde(default, flatten)]
    pub shape: Shape,
    /// Drag coefficient (none: 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag: Option<f64>,
    /// A clamp's length along the wire, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clamp_length: Option<f64>,
}

impl LineObject {
    /// A body of `weight` N and `shape`.
    pub fn body(weight: f64, shape: Shape) -> Self {
        Self {
            kind: ObjectKind::Body,
            weight,
            shape,
            ..Self::default()
        }
    }

    /// A clamp of `weight` N.
    pub fn clamp(weight: f64) -> Self {
        Self {
            kind: ObjectKind::Clamp,
            weight,
            ..Self::default()
        }
    }

    /// Its values hold.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.weight.is_finite() && self.weight >= 0.0) {
            return Err("weight must be ≥ 0".into());
        }
        if self.drag.is_some_and(|d| !(d.is_finite() && d > 0.0)) {
            return Err("drag must be > 0".into());
        }
        if self
            .clamp_length
            .is_some_and(|l| !(l.is_finite() && l >= 0.0))
        {
            return Err("clamp length must be ≥ 0".into());
        }
        self.shape.validate()
    }

    /// Downward and horizontal forces, N, on the whole bundle in a case,
    /// on a wire of bare diameter `wire_diameter` (m): its weight and its
    /// ice (the wire's radial ice on its surface, [`Shape::ice_and_wind`]);
    /// the wind on its iced area with its drag. A case with an ice load
    /// but no density has no thickness: the body takes no ice.
    pub fn forces(&self, case: &ols_weather::WeatherCase, wire_diameter: f64) -> [f64; 2] {
        let (t, rho) = case
            .radial_ice(ols_core::units::Length::from_si(wire_diameter))
            .map_or((0.0, 0.0), |(t, rho)| (t.si(), rho.si()));
        let (ice, wind) = self.shape.ice_and_wind(t);
        [
            self.weight + ice * rho * ols_core::units::STANDARD_GRAVITY,
            case.wind_pressure.si() * self.drag.unwrap_or(1.0) * wind,
        ]
    }
}

/// A load spread along a stretch of a span between two of its points
/// (e.g. a heated stretch, a cable lashed on): downward, N/m of the
/// conductor, on the whole bundle.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpreadObject {
    /// Label, unique among the section's spread objects.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// The point it starts at (a label of the section's points).
    pub from: String,
    /// The point it ends at, on the same span.
    pub to: String,
    /// The library type it was taken from.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    /// Downward load, N per metre of conductor (the whole bundle's).
    pub load: f64,
}

/// A type of the library: a template for objects on points (a body, a
/// clamp) or for spread objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// (No deny_unknown_fields: serde does not check them with a flattened
// shape.)
pub struct ObjectType {
    /// Unique id.
    pub id: String,
    /// Name shown.
    pub name: String,
    /// What it is.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// What it makes.
    pub kind: TypeKind,
    /// Weight, N (bodies and clamps).
    #[serde(default)]
    pub weight: f64,
    /// Shape (bodies, clamps; none: a point).
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    /// Drag coefficient (none: 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag: Option<f64>,
    /// A clamp's length along the wire, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clamp_length: Option<f64>,
    /// A spread type's load, N/m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<f64>,
}

/// What a library type makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    /// A body on a point.
    Body,
    /// A clamp on a point.
    Clamp,
    /// A spread object between two points.
    Spread,
}

impl ObjectType {
    /// The object on a point it makes (none: a spread type).
    pub fn object(&self) -> Option<LineObject> {
        let kind = match self.kind {
            TypeKind::Body => ObjectKind::Body,
            TypeKind::Clamp => ObjectKind::Clamp,
            TypeKind::Spread => return None,
        };
        Some(LineObject {
            kind,
            type_id: Some(self.id.clone()),
            weight: self.weight,
            shape: self.shape.unwrap_or_default(),
            drag: self.drag,
            clamp_length: self.clamp_length,
        })
    }
}

/// File format id of a library of point and spread loads.
pub const OBJECT_LIBRARY_FORMAT: &str = "ols-point-spread-loads";
/// Newest version read.
pub const OBJECT_LIBRARY_VERSION: u32 = 1;
/// File extension.
pub const OBJECT_LIBRARY_EXTENSION: &str = "olsload";

/// A library of point and spread load types (`.olsload`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectLibrary {
    /// Format id.
    #[serde(default)]
    pub format: Option<String>,
    /// Version.
    #[serde(default)]
    pub version: Option<u32>,
    /// Name.
    #[serde(default)]
    pub name: String,
    /// The types.
    #[serde(default, rename = "type")]
    pub types: Vec<ObjectType>,
}

impl ObjectLibrary {
    /// Parses and checks a library file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let lib: ObjectLibrary = toml::from_str(text).map_err(|e| e.to_string())?;
        ols_core::files::check_header(
            lib.format.as_deref(),
            lib.version,
            OBJECT_LIBRARY_FORMAT,
            OBJECT_LIBRARY_VERSION,
            "point and spread loads",
        )?;
        lib.validate()?;
        Ok(lib)
    }

    /// Reads a library file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// As a library file.
    pub fn to_text(&self) -> String {
        let mut out = self.clone();
        out.format = Some(OBJECT_LIBRARY_FORMAT.into());
        out.version = Some(OBJECT_LIBRARY_VERSION);
        toml::to_string(&out).expect("a library serialises")
    }

    /// The type of id `id`.
    pub fn get(&self, id: &str) -> Option<&ObjectType> {
        self.types.iter().find(|t| t.id == id)
    }

    /// Its types hold: ids unique, values not negative, a spread type
    /// with a load.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        for t in &self.types {
            if t.id.is_empty() || !ids.insert(t.id.as_str()) {
                return Err(format!("type \"{}\": ids must be given and unique", t.id));
            }
            let bad = |m: String| format!("type {}: {m}", t.id);
            match t.object() {
                Some(o) => o.validate().map_err(bad)?,
                None => {
                    if !t.load.is_some_and(|l| l.is_finite() && l >= 0.0) {
                        return Err(bad("a spread type needs a load ≥ 0 N/m".into()));
                    }
                }
            }
        }
        Ok(())
    }

    /// The shipped templates (`libraries/objects/templates.olsload`).
    pub fn templates() -> Self {
        Self::parse(include_str!("../../../libraries/objects/templates.olsload"))
            .expect("the shipped templates read")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_core::units::{Length, Stress, Temperature};

    fn iced(t: f64, wind: f64) -> ols_weather::WeatherCase {
        let mut c = ols_weather::WeatherCase::still("c", Temperature::from_celsius(0.0));
        c.ice_thickness = Length::from_si(t);
        c.wind_pressure = Stress::from_si(wind);
        c
    }

    /// The shapes: a sphere as point loads were (shell of ice, iced
    /// projected circle); a cylinder along the wire its side, across its
    /// end; a box its side along × height; a point nothing.
    #[test]
    fn shapes_take_ice_and_wind() {
        use std::f64::consts::PI;
        let (d, t) = (0.6, 0.01);
        let s = LineObject::body(100.0, Shape::Sphere { diameter: d });
        let case = iced(t, 500.0);
        let rho = case
            .radial_ice(ols_core::units::Length::from_si(0.03))
            .unwrap()
            .1
            .si();
        let [down, across] = s.forces(&case, 0.03);
        let ice = PI / 6.0 * ((d + 2.0 * t).powi(3) - d.powi(3)) * rho * 9.80665;
        assert!((down - 100.0 - ice).abs() < 1e-6 * ice.max(1.0), "{down}");
        assert!((across - 500.0 * PI / 4.0 * (d + 2.0 * t).powi(2)).abs() < 1e-9);
        let side = Shape::Cylinder {
            diameter: 0.2,
            length: 0.5,
            axis: Axis::Along,
        };
        assert!((side.ice_and_wind(0.0).1 - 0.1).abs() < 1e-12);
        let end = Shape::Cylinder {
            diameter: 0.2,
            length: 0.5,
            axis: Axis::Across,
        };
        assert!((end.ice_and_wind(0.0).1 - PI * 0.01).abs() < 1e-12);
        let b = Shape::Box {
            along: 0.3,
            across: 0.2,
            height: 0.4,
        };
        assert!((b.ice_and_wind(0.0).1 - 0.12).abs() < 1e-12);
        assert_eq!(Shape::Point.ice_and_wind(0.02), (0.0, 0.0));
        assert!(
            Shape::Sphere { diameter: -1.0 }.validate().is_err()
                && LineObject {
                    drag: Some(0.0),
                    ..LineObject::default()
                }
                .validate()
                .is_err()
        );
    }

    /// The shipped templates read; written and read back the same; a
    /// type makes its object (its id kept); wrong libraries refused.
    #[test]
    fn the_library_of_templates() {
        let lib = ObjectLibrary::templates();
        assert!(lib.types.len() >= 6);
        assert_eq!(
            ObjectLibrary::parse(&lib.to_text()).unwrap().types,
            lib.types
        );
        let ball = lib
            .types
            .iter()
            .find(|t| matches!(t.shape, Some(Shape::Sphere { .. })))
            .unwrap();
        let o = ball.object().unwrap();
        assert_eq!(o.type_id.as_deref(), Some(ball.id.as_str()));
        assert!(lib.types.iter().any(|t| t.kind == TypeKind::Clamp));
        assert!(
            lib.types
                .iter()
                .any(|t| t.kind == TypeKind::Spread && t.object().is_none())
        );
        let mut twice = lib.clone();
        twice.types.push(twice.types[0].clone());
        assert!(twice.validate().is_err());
        assert!(ObjectLibrary::parse("format = \"x\"").is_err());
    }
}

#[cfg(test)]
mod legacy_tests {
    use crate::{Line, ObjectKind, Shape};

    /// A line file of before points and objects: its point loads become
    /// points (`L1`…) holding a body (a sphere of the load's diameter, its
    /// label the point's description), its spread loads two bare points
    /// and a spread between them; written back without the old fields; the
    /// forces the same.
    #[test]
    fn older_point_and_spread_loads_are_read_into_points_and_objects() {
        let example = Line::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/synthetic-line/synthetic-line.ols.json"),
        )
        .unwrap();
        let mut old = serde_json::to_value(&example.sections[0]).unwrap();
        let o = old.as_object_mut().unwrap();
        o.remove("span_points");
        o.remove("spreads");
        o.insert(
            "loads".into(),
            serde_json::json!([
                {"span": 0, "fraction": 0.5, "vertical": 70.0, "label": "ball", "diameter": 0.6},
                {"span": 0, "distance": 30.0, "vertical": 50.0, "wind_area": 0.2}
            ]),
        );
        o.insert(
            "spread_loads".into(),
            serde_json::json!([
                {"span": 0, "start": {"fraction": 0.1}, "end": {"distance": 50.0}, "load": 12.5, "label": "tray"}
            ]),
        );
        let text = old.to_string();
        let s: crate::Section = serde_json::from_str(&text).unwrap();
        assert_eq!(s.span_points.len(), 4);
        let l1 = &s.span_points[0];
        assert_eq!((l1.label.as_str(), l1.description.as_str()), ("L1", "ball"));
        let o = l1.object.as_ref().unwrap();
        assert_eq!(o.kind, ObjectKind::Body);
        assert_eq!((o.weight, o.shape), (70.0, Shape::Sphere { diameter: 0.6 }));
        let o2 = s.span_points[1].object.as_ref().unwrap();
        assert!(matches!(o2.shape, Shape::Areas { wind_area, .. } if wind_area == 0.2));
        assert_eq!(s.span_points[1].at.distance(), Some(30.0));
        assert_eq!(s.spreads.len(), 1);
        let r = &s.spread_loads()[0];
        assert_eq!(
            (r.start.fraction(), r.end.distance(), r.load),
            (Some(0.1), Some(50.0), 12.5)
        );
        let written = serde_json::to_string(&s).unwrap();
        assert!(!written.contains("\"loads\"") && !written.contains("spread_loads"));
        assert_eq!(serde_json::from_str::<crate::Section>(&written).unwrap(), s);
        // The same forces as the old point load.
        let old: crate::SpanLoad = serde_json::from_value(
            serde_json::json!({"span": 0, "vertical": 70.0, "diameter": 0.6}),
        )
        .unwrap();
        let mut iced =
            ols_weather::WeatherCase::still("c", ols_core::units::Temperature::from_celsius(0.0));
        iced.ice_thickness = ols_core::units::Length::from_millimeters(12.0);
        iced.wind_pressure = ols_core::units::Stress::from_pascals(300.0);
        assert_eq!(o.forces(&iced, 0.03), old.forces(&iced, 0.03));
        // The example line (older form) reads.
        assert!(example.sections[0].objects().next().is_some());
    }
}
