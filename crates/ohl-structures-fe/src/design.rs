// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A member's **design data** (2026-10-27): what a member-capacity method
//! needs beyond the section and the forces — effective length ratios,
//! how its ends are connected and restrained, the slenderness of its
//! legs, its bolts, its class and crossing partner. All optional; the
//! data are neutral (no design code's numbers): a method maps them to its
//! own rules (the capacity method of the check) and says what it assumed where one is
//! missing.
//!
//! In a structure file (`.ohls`, version 8) or a model file (`.ohlfe`,
//! version 4), per member:
//!
//! ```toml
//! [[member]]
//! id = "D1"
//! nodes = ["A", "B"]
//! section = "L60x6"
//! [member.design]
//! class = "bracing"                  # leg | bracing | redundant | crossing
//! crossing = "D2"                    # its crossing partner (another member's id)
//! length_ratios = [1.0, 1.0, 0.5]    # buckling length / member length about
//!                                    # the section's y, z and minor principal axes
//! end_eccentricity = "both_ends"     # none | one_end | both_ends
//! end_restraint = "none"             # none | one_end | both_ends
//! width_thickness = 10.2             # flat width / thickness of the legs
//! thickness = "6 mm"                 # of the connected legs (built-ups, `general`)
//! plies = 1                          # profiles side by side (2 for two angles back to back)
//! connected_leg = "long"             # both | long | short | none
//! bolts = { diameter = "16 mm", hole = "17.5 mm", count = 2, shear_planes = 1,
//!           shear_capacity = "60 kN", ultimate = "800 MPa", holes = 1 }
//! edge_distance = "25 mm"
//! end_distance = "27 mm"
//! spacing = "50 mm"
//! ```

use ohl_core::units::{Force, Length, Stress};
use serde::Deserialize;

/// How a member's ends are connected or restrained (the two codes a
/// capacity method reads: eccentricity, rotational restraint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ends {
    /// Neither end.
    None,
    /// One end.
    OneEnd,
    /// Both ends.
    BothEnds,
}

impl Ends {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Ends::None => "none",
            Ends::OneEnd => "one_end",
            Ends::BothEnds => "both_ends",
        }
    }
}

/// Which leg of an angle its end connections bolt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectedLeg {
    /// Both legs (the whole section takes the force).
    Both,
    /// The long leg only.
    Long,
    /// The short leg only.
    Short,
    /// No leg bolted (welded, or not a bolted connection).
    None,
}

impl ConnectedLeg {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectedLeg::Both => "both",
            ConnectedLeg::Long => "long",
            ConnectedLeg::Short => "short",
            ConnectedLeg::None => "none",
        }
    }

    /// One leg only bolted (long or short).
    pub fn one_leg(self) -> bool {
        matches!(self, ConnectedLeg::Long | ConnectedLeg::Short)
    }
}

/// What a member is for in a lattice structure (a method's slenderness
/// limits and rules depend on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberClass {
    /// A leg (main corner member).
    Leg,
    /// Bracing carrying the structure's loads.
    Bracing,
    /// A redundant (secondary member bracing others, no computed load of
    /// its own by design).
    Redundant,
    /// A crossing diagonal, braced at the crossing by its partner
    /// ([`MemberDesign::crossing`]) when that one is in tension.
    Crossing,
}

impl MemberClass {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            MemberClass::Leg => "leg",
            MemberClass::Bracing => "bracing",
            MemberClass::Redundant => "redundant",
            MemberClass::Crossing => "crossing",
        }
    }
}

/// A member's end bolts (each end alike), SI units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bolts {
    /// Bolt diameter, m.
    pub diameter: f64,
    /// Hole diameter, m (none: not given).
    pub hole: Option<f64>,
    /// Bolts at one end (0: the holes only — the net section — the
    /// bolts not checked).
    pub count: u32,
    /// Shear planes per bolt (default 1).
    pub shear_planes: u32,
    /// Ultimate shear capacity of one bolt per shear plane, N (none: from
    /// `ultimate` by the method, or not checked).
    pub shear_capacity: Option<f64>,
    /// Ultimate tensile strength of the bolt, Pa (none: not given).
    pub ultimate: Option<f64>,
    /// Holes in one cross-section (the net section; none: not given).
    pub holes: Option<u32>,
}

/// A member's design data (all optional), SI units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemberDesign {
    /// What it is for.
    pub class: Option<MemberClass>,
    /// Its crossing partner (member index), resolved from the id.
    pub crossing: Option<usize>,
    /// Buckling length over the member's length about the section's y,
    /// z and minor principal (v) axes.
    pub length_ratios: Option<[f64; 3]>,
    /// Ends connected eccentrically (e.g. an angle bolted by one leg).
    pub end_eccentricity: Option<Ends>,
    /// Ends partly restrained against rotation (e.g. two or more bolts
    /// into a stiffer member).
    pub end_restraint: Option<Ends>,
    /// Flat width over thickness of the legs.
    pub width_thickness: Option<f64>,
    /// Thickness of the connected legs, m (built-ups, `general` profiles).
    pub thickness: Option<f64>,
    /// Profiles side by side (built-ups: bolts bear on each).
    pub plies: Option<u32>,
    /// The leg its end connections bolt.
    pub connected_leg: Option<ConnectedLeg>,
    /// Its end bolts.
    pub bolts: Option<Bolts>,
    /// Edge distance of the bolts, m.
    pub edge_distance: Option<f64>,
    /// End distance of the bolts, m.
    pub end_distance: Option<f64>,
    /// Bolt spacing, m.
    pub spacing: Option<f64>,
}

impl MemberDesign {
    /// Whether nothing is given.
    pub fn is_empty(&self) -> bool {
        *self == MemberDesign::default()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BoltsFile {
    diameter: Length,
    #[serde(default)]
    hole: Option<Length>,
    count: u32,
    #[serde(default)]
    shear_planes: Option<u32>,
    #[serde(default)]
    shear_capacity: Option<Force>,
    #[serde(default)]
    ultimate: Option<Stress>,
    #[serde(default)]
    holes: Option<u32>,
}

/// `[member.design]` as written.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct DesignFile {
    #[serde(default)]
    class: Option<MemberClass>,
    #[serde(default)]
    crossing: Option<String>,
    #[serde(default)]
    length_ratios: Option<[f64; 3]>,
    #[serde(default)]
    end_eccentricity: Option<Ends>,
    #[serde(default)]
    end_restraint: Option<Ends>,
    #[serde(default)]
    width_thickness: Option<f64>,
    #[serde(default)]
    thickness: Option<Length>,
    #[serde(default)]
    plies: Option<u32>,
    #[serde(default)]
    connected_leg: Option<ConnectedLeg>,
    #[serde(default)]
    bolts: Option<BoltsFile>,
    #[serde(default)]
    edge_distance: Option<Length>,
    #[serde(default)]
    end_distance: Option<Length>,
    #[serde(default)]
    spacing: Option<Length>,
}

impl DesignFile {
    /// The design data, the crossing partner's id apart (resolved by the
    /// model once every member is read).
    pub(crate) fn read(&self, what: &str) -> Result<(MemberDesign, Option<String>), String> {
        let positive = |name: &str, v: Option<f64>| match v {
            Some(x) if !(x.is_finite() && x > 0.0) => {
                Err(format!("{what}: design {name} must be above 0"))
            }
            _ => Ok(v),
        };
        if let Some(r) = self.length_ratios
            && r.iter().any(|x| !(x.is_finite() && *x > 0.0))
        {
            return Err(format!("{what}: design length_ratios must be above 0"));
        }
        let bolts = match &self.bolts {
            None => None,
            Some(b) => {
                if b.shear_planes == Some(0) {
                    return Err(format!(
                        "{what}: design bolts: shear_planes must be 1 or more"
                    ));
                }
                Some(Bolts {
                    diameter: positive("bolts diameter", Some(b.diameter.si()))?.unwrap_or(0.0),
                    hole: positive("bolts hole", b.hole.map(Length::si))?,
                    count: b.count,
                    shear_planes: b.shear_planes.unwrap_or(1),
                    shear_capacity: positive(
                        "bolts shear_capacity",
                        b.shear_capacity.map(Force::si),
                    )?,
                    ultimate: positive("bolts ultimate", b.ultimate.map(Stress::si))?,
                    holes: b.holes,
                })
            }
        };
        if self.plies == Some(0) {
            return Err(format!("{what}: design plies must be 1 or more"));
        }
        Ok((
            MemberDesign {
                class: self.class,
                crossing: None,
                length_ratios: self.length_ratios,
                end_eccentricity: self.end_eccentricity,
                end_restraint: self.end_restraint,
                width_thickness: positive("width_thickness", self.width_thickness)?,
                thickness: positive("thickness", self.thickness.map(Length::si))?,
                plies: self.plies,
                connected_leg: self.connected_leg,
                bolts,
                edge_distance: positive("edge_distance", self.edge_distance.map(Length::si))?,
                end_distance: positive("end_distance", self.end_distance.map(Length::si))?,
                spacing: positive("spacing", self.spacing.map(Length::si))?,
            },
            self.crossing.clone(),
        ))
    }
}
