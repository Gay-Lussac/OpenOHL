// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Annotations (phase 2 of the measuring tools and annotations spec,
//! docs/development/measuring-tools-spec.md; owner 2026-10-25): kept
//! measurements, notes and leaders, each a project object of kind
//! [`Kind::Annotation`] — **stored apart from the line** (like a check
//! report: no part of the line, its hash or its checks' state; not in the
//! line file), **shared by everyone** (each carries its author; every
//! user may add and change them, as reports), and **deleted with the
//! object it is attached to**: a command that leaves an anchor without
//! its object deletes the annotation in the same log entry
//! (`orphaned`), so one undo brings both back.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Command, Kind, SessionError, refused};
use ohl_line::Line;

/// What an annotation is attached to: an object of the line by id (it
/// follows the object; the object deleted, the annotation goes), or a
/// point by its coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "on", rename_all = "snake_case", deny_unknown_fields)]
pub enum Anchor {
    /// A structure's base.
    Structure {
        /// Its id.
        structure: String,
    },
    /// A point of a structure's members or strings, by its offset from
    /// the structure's base (easting, northing, height; it follows the
    /// structure moved, not turned).
    OnStructure {
        /// The structure.
        structure: String,
        /// From its base, m.
        offset: [f64; 3],
    },
    /// A wire point of a structure (where a wire is clamped), by its set.
    Attachment {
        /// The structure.
        structure: String,
        /// The set.
        set: u32,
    },
    /// A point of a span's wire, by its share of the span's length along
    /// the wire (as solved in the weather case measured).
    Wire {
        /// The section.
        section: String,
        /// The span (0: the section's first).
        span: usize,
        /// Share of the wire's length from its back end, 0 to 1.
        fraction: f64,
    },
    /// A point of a jumper, by its share of its length.
    Jumper {
        /// The jumper.
        jumper: String,
        /// Share of its length from its first end, 0 to 1.
        fraction: f64,
    },
    /// A span point of a section, by its label.
    SpanPoint {
        /// The section.
        section: String,
        /// Its label.
        label: String,
    },
    /// An alignment point.
    AlignmentPoint {
        /// Its id.
        id: String,
    },
    /// A LiDAR point, by its coordinates and class.
    Lidar {
        /// Easting, northing, height, m.
        at: [f64; 3],
        /// Its class.
        class: u8,
    },
    /// A point in space (the ground, a survey point, a point of the
    /// profile's plane).
    Point {
        /// Easting, northing, height, m.
        at: [f64; 3],
        /// What it was (shown).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        what: String,
    },
}

impl Anchor {
    /// Whether what it is attached to is in `line` (a point always is).
    pub fn holds(&self, line: &Line) -> bool {
        let section = |id: &str| line.sections.iter().find(|s| s.id == id);
        match self {
            Anchor::Structure { structure } | Anchor::OnStructure { structure, .. } => {
                line.structure(structure).is_some()
            }
            Anchor::Attachment { structure, set } => line
                .wire_point(&ohl_line::SectionPoint::new(structure.clone(), *set))
                .is_some(),
            Anchor::Wire {
                section: s, span, ..
            } => section(s).is_some_and(|x| *span + 1 < x.points.len()),
            Anchor::Jumper { jumper, .. } => line.jumpers.iter().any(|j| &j.id == jumper),
            Anchor::SpanPoint { section: s, label } => {
                section(s).is_some_and(|x| x.span_points.iter().any(|p| &p.label == label))
            }
            Anchor::AlignmentPoint { id } => line.alignment_points.iter().any(|p| &p.id == id),
            Anchor::Lidar { .. } | Anchor::Point { .. } => true,
        }
    }
}

/// What an annotation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    /// A kept measurement, drawn as a dimension.
    Dimension,
    /// Text with an arrow to its anchor.
    Leader,
    /// Text at its anchor.
    Note,
}

/// The value an annotation had when it was last computed (a reader
/// without the solved line shows it, saying so).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastValue {
    /// The value, as computed.
    pub value: f64,
    /// Its unit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// The weather case it was computed in.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub case: String,
}

/// An annotation, as kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    /// Its id (unique among the line's annotations).
    pub id: String,
    /// What it is.
    pub kind: AnnotationKind,
    /// A dimension's measurement: `distance`, `height`, `stations`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool: String,
    /// What it is attached to (a dimension: its ends; a leader or a note:
    /// one).
    pub anchors: Vec<Anchor>,
    /// A leader's or a note's text; a dimension's prefix.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Who made it.
    #[serde(default)]
    pub author: String,
    /// When (UTC, `YYYY-MM-DDTHH:MM:SSZ`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub time: String,
    /// The value when last computed (a dimension).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<LastValue>,
}

impl Annotation {
    /// The command keeping it (replacing one of its id).
    pub fn put(&self) -> Command {
        Command::Put {
            kind: Kind::Annotation,
            id: self.id.clone(),
            value: serde_json::to_value(self).expect("serialises"),
            ord: None,
        }
    }

    /// Whether every anchor's object is in `line`.
    pub fn holds(&self, line: &Line) -> bool {
        self.anchors.iter().all(|a| a.holds(line))
    }
}

/// The command deleting annotation `id`.
pub fn delete(id: &str) -> Command {
    Command::Delete {
        kind: Kind::Annotation,
        id: id.to_owned(),
        confirm: false,
    }
}

/// An annotation's value as kept, checked: it reads, its id matches, it
/// has an anchor (a dimension: as many as its measurement takes).
pub(crate) fn check_value(id: &str, value: &Value) -> Result<(), SessionError> {
    let a: Annotation = serde_json::from_value(value.clone())
        .map_err(|e| refused(format!("annotation {id}: {e}")))?;
    if a.id != id {
        return Err(refused(format!(
            "annotation {id}: its \"id\" field differs"
        )));
    }
    let wanted = match (a.kind, a.tool.as_str()) {
        (AnnotationKind::Dimension, "height" | "clearance") => 1,
        (
            AnnotationKind::Dimension,
            "distance" | "stations" | "wire_to_wire" | "wire_to_object",
        ) => 2,
        (AnnotationKind::Dimension, other) => {
            return Err(refused(format!(
                "annotation {id}: measurement \"{other}\" (distance, height, stations, \
                 wire_to_wire, clearance or wire_to_object)"
            )));
        }
        _ => 1,
    };
    if a.anchors.len() != wanted {
        return Err(refused(format!(
            "annotation {id}: {} anchors, {wanted} wanted",
            a.anchors.len()
        )));
    }
    if let Some(f) = a.anchors.iter().find_map(|x| match x {
        Anchor::Wire { fraction, .. } | Anchor::Jumper { fraction, .. } => Some(*fraction),
        _ => None,
    }) && !(0.0..=1.0).contains(&f)
    {
        return Err(refused(format!(
            "annotation {id}: a fraction of {f} (0 to 1)"
        )));
    }
    Ok(())
}

/// The annotations among `objects` whose anchors `line` no longer holds
/// (their object deleted, a span gone): to delete with it.
pub(crate) fn orphaned<'a>(
    objects: impl Iterator<Item = (&'a String, &'a Value)>,
    line: &Line,
) -> Vec<String> {
    objects
        .filter_map(|(id, v)| {
            let a: Annotation = serde_json::from_value(v.clone()).ok()?;
            (!a.holds(line)).then(|| id.clone())
        })
        .collect()
}

impl crate::Session {
    /// The current line's annotations (those that do not read are left
    /// out), in the order they were made.
    pub fn annotations(&self) -> Vec<Annotation> {
        let mut v: Vec<(i64, Annotation)> = self
            .objects
            .iter()
            .filter(|((k, _), _)| *k == Kind::Annotation)
            .filter_map(|(_, o)| Some((o.ord, serde_json::from_value((*o.value).clone()).ok()?)))
            .collect();
        v.sort_by_key(|(ord, _)| *ord);
        v.into_iter().map(|(_, a)| a).collect()
    }

    /// A stamp that changes whenever an annotation is added, changed or
    /// deleted (to read them again only then).
    pub fn annotations_stamp(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for ((k, id), o) in &self.objects {
            if *k == Kind::Annotation {
                (id, o.version, o.ord).hash(&mut h);
            }
        }
        self.current_line().hash(&mut h);
        h.finish()
    }
}
