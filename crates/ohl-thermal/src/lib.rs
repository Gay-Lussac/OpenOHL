// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Thermal rating of overhead conductors.
//!
//! The heat balance of a conductor per unit length,
//!
//! \\[ m c_p \frac{dT}{dt} = P_J + P_S - P_C - P_R, \qquad P_J = I^2 R_{ac}(T), \\]
//!
//! with Joule heating \\( P_J \\) (AC resistance of the wire library,
//! linear in temperature: through two given values, or one value scaled
//! with the material's temperature coefficient), solar heating \\( P_S \\), convective cooling
//! \\( P_C \\) (forced, low-wind or natural) and radiative cooling
//! \\( P_R \\), by one of three [`Method`]s:
//!
//! - **IEEE Std 738-2012** ([`ieee738`], solar flux [`solar::ieee_flux`]);
//! - **CIGRE TB 601** (2014) ([`cigre`], direct and diffuse irradiance
//!   with ground albedo [`solar::cigre_irradiance`]);
//! - **CIGRE TB 207** (2002), the older brochure: as TB 601 but for its air
//!   properties and low-wind rule (see [`cigre`]).
//!
//! The conductor temperature is taken uniform (no radial temperature
//! difference between core and surface).
//!
//! What it computes:
//!
//! - [`ampacity`]: the steady-state current for a maximum conductor
//!   temperature — the balance solved for I in closed form;
//! - [`temperature`]: the steady-state temperature for a current —
//!   bisection on the temperature (exit clauses: 200 iterations, 1 s;
//!   about 60 iterations to 10⁻⁶ °C, microseconds);
//! - [`transient`]: the temperature after a step of current — fourth-order
//!   Runge–Kutta in time steps of [`DEFAULT_TIME_STEP`] s (the thermal
//!   time constant of line conductors is several minutes), one iteration
//!   per step;
//! - [`limit`]: the highest conductor temperature at which a line still
//!   meets its clearances, per section.
//!
//! Ambient conditions come from `.ohlta` files ([`ambient`]).

pub mod ambient;
pub mod cigre;
pub mod ieee738;
pub mod limit;
pub mod solar;

use std::fmt;
use std::time::Duration;

use ohl_cables::{Cable, CableKind, Electrical};
use ohl_core::budget::{Budget, Stop};
use ohl_core::units::Temperature;

pub use ambient::{Ambient, AmbientCases, Sun};
pub use solar::{Atmosphere, SolarPosition};

/// Emissivity used when neither the cable nor the ambient case gives one.
pub const DEFAULT_EMISSIVITY: f64 = ohl_line::Defaults::PROGRAM.thermal_emissivity;
/// Solar absorptivity used when neither the cable nor the ambient case
/// gives one.
pub const DEFAULT_ABSORPTIVITY: f64 = ohl_line::Defaults::PROGRAM.thermal_absorptivity;
/// Surface roughness \\( R_s \\) (CIGRE) when none is given: a stranded
/// conductor with round outer wires (above 0.05).
pub const DEFAULT_ROUGHNESS: f64 = ohl_line::Defaults::PROGRAM.thermal_roughness;
/// Time step of [`transient`] by default, s.
pub const DEFAULT_TIME_STEP: f64 = ohl_line::Defaults::PROGRAM.thermal_time_step;

/// The thermal rating method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// IEEE Std 738-2012.
    #[default]
    Ieee738,
    /// CIGRE Technical Brochure 601 (2014).
    Cigre601,
    /// CIGRE Technical Brochure 207 (2002).
    Cigre207,
}

impl Method {
    /// Every method.
    pub const ALL: [Method; 3] = [Method::Ieee738, Method::Cigre601, Method::Cigre207];

    /// Its name on the command line and in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Ieee738 => "ieee738",
            Method::Cigre601 => "cigre601",
            Method::Cigre207 => "cigre207",
        }
    }

    /// Its name in reports.
    pub fn title(self) -> &'static str {
        match self {
            Method::Ieee738 => "IEEE 738-2012",
            Method::Cigre601 => "CIGRE TB 601",
            Method::Cigre207 => "CIGRE TB 207",
        }
    }

    /// Reads a name ([`Method::as_str`], case and `-`/space insensitive).
    pub fn parse(text: &str) -> Result<Self, String> {
        let t: String = text
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|m| m.as_str() == t || t == m.as_str().replace("cigre", "tb"))
            .ok_or_else(|| format!("method \"{text}\": write ieee738, cigre601 or cigre207"))
    }
}

/// Which convection governs the cooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convection {
    /// Forced by the wind.
    Forced,
    /// The low-wind rule of CIGRE.
    LowWind,
    /// Natural (still air).
    Natural,
}

impl Convection {
    /// In words.
    pub fn as_str(self) -> &'static str {
        match self {
            Convection::Forced => "forced",
            Convection::LowWind => "low wind",
            Convection::Natural => "natural",
        }
    }
}

/// Errors of the thermal calculations.
#[derive(Debug, Clone, PartialEq)]
pub enum ThermalError {
    /// The data does not allow the calculation.
    Invalid(String),
    /// An iteration stopped on its exit clause.
    Stopped(Stop),
}

impl fmt::Display for ThermalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThermalError::Invalid(m) => f.write_str(m),
            ThermalError::Stopped(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for ThermalError {}

impl From<Stop> for ThermalError {
    fn from(s: Stop) -> Self {
        ThermalError::Stopped(s)
    }
}

/// The acute angle, 0–90°, between the conductor axis and wind blowing at
/// `angle` degrees to it: wind along the axis either way (0°, 180°, 360°)
/// gives 0°, across it either way (90°, 270°, −90°) gives 90°.
pub fn acute_wind_angle(angle: f64) -> f64 {
    angle.to_radians().cos().abs().min(1.0).acos().to_degrees()
}

/// Thermal data of a conductor.
#[derive(Debug, Clone, PartialEq)]
pub struct Conductor {
    /// Outside diameter, m.
    pub diameter: f64,
    /// AC resistance by temperature (and the rest of the electrical data).
    pub electrical: Electrical,
    /// Kind of cable: its material gives the temperature coefficient that
    /// scales a single AC resistance value to other temperatures.
    pub kind: CableKind,
    /// Heat capacity of each material, J/(m·K) at 20 °C, and its
    /// temperature coefficient, 1/K.
    pub heat_capacity: Vec<(f64, f64)>,
    /// Surface roughness \\( R_s = d / (2(D - d)) \\) of the outer wires
    /// (CIGRE).
    pub roughness: f64,
    /// Emissivity, solar absorptivity and ground albedo where neither the
    /// cable nor the ambient case gives them (the project's,
    /// [`Conductor::with_defaults`]).
    pub defaults: [f64; 3],
}

/// Specific heat at 20 °C, J/(kg·K), its temperature coefficient, 1/K, and
/// density, kg/m³, of aluminium (IEEE 738 Table 2 values for EC
/// aluminium).
const ALUMINIUM: (f64, f64, f64) = (955.0, 3.8e-4, 2703.0);
/// The same for steel core wire.
const STEEL: (f64, f64, f64) = (476.0, 1.0e-4, 7780.0);
/// The same for a polymer composite core (rough typical values).
const COMPOSITE: (f64, f64, f64) = (1000.0, 0.0, 1900.0);

impl Conductor {
    /// The thermal data of a library cable: it needs `[cable.electrical]`
    /// with at least one AC resistance (two or more for a kind whose
    /// material is unknown, see [`Conductor::check_resistance`]). The heat capacity is estimated from
    /// the components: aluminium outer wires (steel for steel wire), a
    /// steel core (composite for ACCC), the cable's mass shared between
    /// them by area × density.
    pub fn from_cable(cable: &Cable) -> Result<Self, ThermalError> {
        let electrical = cable
            .electrical
            .clone()
            .filter(|e| !e.ac_resistance.is_empty())
            .ok_or_else(|| {
                ThermalError::Invalid(format!(
                    "cable {}: no AC resistance in its [cable.electrical] data",
                    cable.id
                ))
            })?;
        Self::check_resistance_of(&electrical, cable.kind)
            .map_err(|m| ThermalError::Invalid(format!("cable {}: {m}", cable.id)))?;
        let outer = match cable.kind {
            CableKind::Steel => STEEL,
            _ => ALUMINIUM,
        };
        let core = match cable.kind {
            CableKind::Accc => COMPOSITE,
            _ => STEEL,
        };
        let mut parts = vec![(cable.outer.area.si() * outer.2, outer)];
        if let Some(c) = &cable.core {
            parts.push((c.area.si() * core.2, core));
        }
        let estimated: f64 = parts.iter().map(|p| p.0).sum();
        let scale = cable.mass_per_length.si() / estimated;
        Ok(Self {
            diameter: cable.diameter.si(),
            electrical,
            kind: cable.kind,
            heat_capacity: parts
                .into_iter()
                .map(|(m, (c, beta, _))| (m * scale * c, beta))
                .collect(),
            roughness: DEFAULT_ROUGHNESS,
            defaults: [
                DEFAULT_EMISSIVITY,
                DEFAULT_ABSORPTIVITY,
                ambient::DEFAULT_ALBEDO,
            ],
        })
    }

    /// With a project's default values: its roughness, and its emissivity,
    /// absorptivity and albedo where the cable or the case gives none.
    pub fn with_defaults(mut self, d: &ohl_line::Defaults) -> Self {
        self.roughness = d.thermal_roughness;
        self.defaults = [
            d.thermal_emissivity,
            d.thermal_absorptivity,
            d.thermal_albedo,
        ];
        self
    }

    /// Ground albedo for `ambient` (its own, else the default).
    pub fn albedo(&self, ambient: &Ambient) -> f64 {
        ambient.albedo.unwrap_or(self.defaults[2])
    }

    /// Whether the AC resistance is known at every temperature: at least
    /// one value, and two or more unless the kind's material has a known
    /// temperature coefficient (one value is then scaled with it).
    pub fn check_resistance(&self) -> Result<(), ThermalError> {
        Self::check_resistance_of(&self.electrical, self.kind).map_err(ThermalError::Invalid)
    }

    fn check_resistance_of(e: &Electrical, kind: CableKind) -> Result<(), String> {
        match e.ac_resistance.len() {
            0 => Err("no AC resistance".into()),
            1 if kind.resistance_temperature_coefficient().is_none() => Err(format!(
                "one AC resistance value cannot be scaled to other temperatures for a \
                 cable of kind \"{}\" (unknown material): give it at two temperatures",
                format!("{kind:?}").to_lowercase()
            )),
            _ => Ok(()),
        }
    }

    /// AC resistance at `t` °C, Ω/m (see [`Electrical::ac_resistance_at`]).
    pub fn resistance(&self, t: f64) -> f64 {
        self.electrical
            .ac_resistance_at(Temperature::from_celsius(t), self.kind)
            .map_or(0.0, |r| r.si())
    }

    /// Heat capacity per length at `t` °C, J/(m·K).
    pub fn heat_capacity_at(&self, t: f64) -> f64 {
        self.heat_capacity
            .iter()
            .map(|(mc, beta)| mc * (1.0 + beta * (t - 20.0)))
            .sum()
    }

    /// Emissivity for `ambient` (its override, the cable's, the default).
    pub fn emissivity(&self, ambient: &Ambient) -> f64 {
        ambient
            .emissivity
            .or(self.electrical.emissivity)
            .unwrap_or(self.defaults[0])
    }

    /// Solar absorptivity for `ambient`.
    pub fn absorptivity(&self, ambient: &Ambient) -> f64 {
        ambient
            .absorptivity
            .or(self.electrical.solar_absorptivity)
            .unwrap_or(self.defaults[1])
    }
}

/// Solar heating of a conductor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolarGain {
    /// Heat gain, W/m.
    pub heat: f64,
    /// Irradiance on the conductor's projected area, W/m² (IEEE:
    /// \\( Q_{se} \sin\theta \\); CIGRE: \\( I_T \\)).
    pub irradiance: f64,
    /// Where the sun is (a computed position).
    pub sun: Option<SolarPosition>,
    /// Angle between the beam and the conductor, degrees.
    pub incidence: Option<f64>,
}

/// Solar heating of `c` in `a` by `method`.
pub fn solar_gain(method: Method, c: &Conductor, a: &Ambient) -> SolarGain {
    let alpha = c.absorptivity(a);
    match a.sun {
        Sun::Flux(flux) => SolarGain {
            heat: alpha * flux * c.diameter,
            irradiance: flux,
            sun: None,
            incidence: None,
        },
        Sun::Position {
            latitude,
            day,
            hour,
            line_azimuth,
            atmosphere,
            clearness,
        } => {
            let amplitude = match method {
                Method::Ieee738 => solar::IEEE_DECLINATION,
                _ => solar::CIGRE_DECLINATION,
            };
            let sun = solar::position(latitude, day, hour, amplitude);
            let theta = solar::incidence(&sun, line_azimuth);
            let sin_theta = theta.to_radians().sin();
            let irradiance = match method {
                Method::Ieee738 => {
                    solar::ieee_flux(sun.altitude, atmosphere)
                        * solar::ieee_elevation_factor(a.elevation)
                        * sin_theta
                }
                Method::Cigre601 | Method::Cigre207 => {
                    let ns = clearness.unwrap_or_else(|| atmosphere.clearness_ratio());
                    let (beam, diffuse) = solar::cigre_irradiance(sun.altitude, a.elevation, ns);
                    let half_pi = std::f64::consts::FRAC_PI_2;
                    let sin_h = sun.altitude.to_radians().sin().max(0.0);
                    let albedo = c.albedo(a);
                    beam * (sin_theta + half_pi * albedo * sin_h)
                        + diffuse * half_pi * (1.0 + albedo)
                }
            };
            SolarGain {
                heat: alpha * irradiance * c.diameter,
                irradiance,
                sun: Some(sun),
                incidence: Some(theta),
            }
        }
    }
}

/// The heat terms at one conductor temperature, W/m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeatBalance {
    /// Conductor temperature, °C.
    pub temperature: f64,
    /// Current, A.
    pub current: f64,
    /// Joule heating \\( I^2 R_{ac}(T) \\).
    pub joule: f64,
    /// Solar heating.
    pub solar: SolarGain,
    /// Convective cooling.
    pub convection: f64,
    /// Which convection governs.
    pub convection_mode: Convection,
    /// Radiative cooling.
    pub radiation: f64,
}

impl HeatBalance {
    /// Heat gained minus heat lost, W/m (zero in steady state).
    pub fn net(&self) -> f64 {
        self.joule + self.solar.heat - self.convection - self.radiation
    }
}

/// Convective and radiative cooling at `t` °C, W/m.
fn cooling(method: Method, c: &Conductor, a: &Ambient, t: f64) -> (f64, Convection, f64) {
    let d = c.diameter;
    let ta = a.air_temperature;
    let (conv, mode) = match method {
        Method::Ieee738 => ieee738::convection(d, t, ta, a.wind_speed, a.wind_angle, a.elevation),
        Method::Cigre601 | Method::Cigre207 => cigre::convection(
            d,
            a.roughness.unwrap_or(c.roughness),
            t,
            ta,
            a.wind_speed,
            a.wind_angle,
            a.elevation,
            method == Method::Cigre207,
        ),
    };
    let rad = match method {
        Method::Ieee738 => ieee738::radiation(d, c.emissivity(a), t, ta),
        _ => cigre::radiation(d, c.emissivity(a), t, ta),
    };
    (conv, mode, rad)
}

/// The heat terms of `c` in `a` at `t` °C carrying `current` A.
pub fn heat_balance(
    method: Method,
    c: &Conductor,
    a: &Ambient,
    t: f64,
    current: f64,
) -> HeatBalance {
    let (convection, convection_mode, radiation) = cooling(method, c, a, t);
    HeatBalance {
        temperature: t,
        current,
        joule: current * current * c.resistance(t),
        solar: solar_gain(method, c, a),
        convection,
        convection_mode,
        radiation,
    }
}

/// Steady-state current at which `c` reaches `max_temperature` °C in `a`
/// (the balance solved for I).
pub fn ampacity(
    method: Method,
    c: &Conductor,
    a: &Ambient,
    max_temperature: f64,
) -> Result<HeatBalance, ThermalError> {
    c.check_resistance()?;
    if max_temperature <= a.air_temperature {
        return Err(ThermalError::Invalid(format!(
            "maximum temperature {max_temperature} °C is not above the air ({} °C)",
            a.air_temperature
        )));
    }
    let mut b = heat_balance(method, c, a, max_temperature, 0.0);
    let r = c.resistance(max_temperature);
    if r <= 0.0 {
        return Err(ThermalError::Invalid(format!(
            "AC resistance at {max_temperature} °C is not positive"
        )));
    }
    let heat = b.convection + b.radiation - b.solar.heat;
    if heat < 0.0 {
        return Err(ThermalError::Invalid(format!(
            "the sun alone heats the conductor above {max_temperature} °C"
        )));
    }
    b.current = (heat / r).sqrt();
    b.joule = heat;
    Ok(b)
}

/// Exit clauses of [`temperature`]: 200 bisection steps, 1 s.
pub fn default_steady_budget() -> Budget {
    Budget::new()
        .max_iterations(200)
        .time_limit(Duration::from_secs(1))
}

/// Highest conductor temperature tried, °C above the air.
const MAX_RISE: f64 = 2000.0;

/// Steady-state temperature of `c` in `a` carrying `current` A: bisection
/// between the air temperature (net heat gain ≥ 0) and a temperature where
/// the conductor loses more than it gains, to 10⁻⁶ °C.
pub fn temperature(
    method: Method,
    c: &Conductor,
    a: &Ambient,
    current: f64,
    budget: &Budget,
) -> Result<HeatBalance, ThermalError> {
    c.check_resistance()?;
    if !(current.is_finite() && current >= 0.0) {
        return Err(ThermalError::Invalid(format!(
            "current {current} A must be ≥ 0"
        )));
    }
    let net = |t: f64| heat_balance(method, c, a, t, current).net();
    let mut run = budget.start();
    let mut lo = a.air_temperature;
    if net(lo) <= 0.0 {
        return Ok(heat_balance(method, c, a, lo, current));
    }
    let mut rise = 10.0;
    let mut hi = lo + rise;
    while net(hi) > 0.0 {
        run.step()?;
        lo = hi;
        rise *= 2.0;
        if rise > MAX_RISE {
            return Err(ThermalError::Invalid(format!(
                "no balance below {} °C: {current} A is far too much",
                a.air_temperature + MAX_RISE
            )));
        }
        hi = a.air_temperature + rise;
    }
    while hi - lo > 1e-6 {
        run.step()?;
        let mid = 0.5 * (lo + hi);
        if net(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(heat_balance(method, c, a, 0.5 * (lo + hi), current))
}

/// Conductor temperature after a step of current: from `start` °C, with
/// `current` A, every `step` s for `duration` s, by fourth-order
/// Runge–Kutta on \\( dT/dt = (P_J + P_S - P_C - P_R) / (m c_p) \\). Returns
/// (time s, temperature °C) from 0 to `duration`. Exit clauses: one
/// iteration per step (the number of steps + 1), 10 s.
#[allow(clippy::too_many_arguments)]
pub fn transient(
    method: Method,
    c: &Conductor,
    a: &Ambient,
    start: f64,
    current: f64,
    duration: f64,
    step: f64,
) -> Result<Vec<(f64, f64)>, ThermalError> {
    c.check_resistance()?;
    if !(step > 0.0 && duration >= 0.0 && duration.is_finite()) {
        return Err(ThermalError::Invalid(
            "time step must be > 0 and duration ≥ 0".into(),
        ));
    }
    if c.heat_capacity_at(20.0) <= 0.0 {
        return Err(ThermalError::Invalid("no heat capacity".into()));
    }
    let steps = (duration / step).ceil() as u64;
    let budget = Budget::new()
        .max_iterations(steps + 1)
        .time_limit(Duration::from_secs(10));
    let mut run = budget.start();
    let rate = |t: f64| heat_balance(method, c, a, t, current).net() / c.heat_capacity_at(t);
    let mut out = vec![(0.0, start)];
    let (mut time, mut t) = (0.0, start);
    while time < duration - 1e-9 {
        run.step()?;
        let h = step.min(duration - time);
        let k1 = rate(t);
        let k2 = rate(t + 0.5 * h * k1);
        let k3 = rate(t + 0.5 * h * k2);
        let k4 = rate(t + h * k3);
        t += h / 6.0 * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
        time += h;
        if !t.is_finite() {
            return Err(ThermalError::Invalid(
                "the temperature diverged: use a smaller time step".into(),
            ));
        }
        out.push((time, t));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
