# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/thermal/*.toml (cases marked
"reference_thermal.py").

Written from the published formulas of IEEE Std 738-2012 and CIGRE TB 601
(2014) / TB 207 (2002), deliberately differently from the Rust code:
- the sun's direction as a unit vector (east, north, up) from the
  declination, hour angle and latitude; the beam-conductor angle as the dot
  product with the conductor's direction (no azimuth table);
- the steady temperature for a current by the secant method on the heat
  balance (the Rust code bisects);
- the Nusselt tables written out as explicit if-chains.

Run: python3 benchmarks/thermal/reference_thermal.py  (standard library only)
"""

import math

# Drake 26/7 ACSR of the IEEE 738 example.
D = 0.02814
R25, R75 = 7.283e-5, 8.688e-5


def resistance(t):
    return R25 + (R75 - R25) * (t - 25.0) / 50.0


def sun_vector(lat, day, hour, amplitude):
    dec = math.radians(amplitude * math.sin(math.radians(360.0 * (284 + day) / 365.0)))
    w = math.radians(15.0 * (hour - 12.0))
    phi = math.radians(lat)
    east = -math.cos(dec) * math.sin(w)
    north = math.sin(dec) * math.cos(phi) - math.cos(dec) * math.sin(phi) * math.cos(w)
    up = math.sin(dec) * math.sin(phi) + math.cos(dec) * math.cos(phi) * math.cos(w)
    return east, north, up


def sun_angles(lat, day, hour, line_az, amplitude):
    e, n, u = sun_vector(lat, day, hour, amplitude)
    altitude = math.degrees(math.asin(u))
    azimuth = math.degrees(math.atan2(e, n)) % 360.0
    za = math.radians(line_az)
    cos_theta = e * math.sin(za) + n * math.cos(za)
    theta = math.degrees(math.acos(max(-1.0, min(1.0, cos_theta))))
    return altitude, azimuth, theta


# --- IEEE 738-2012 ---------------------------------------------------------

def ieee_qs(alt, atmosphere):
    if atmosphere == "clear":
        c = [-42.2391, 63.8044, -1.9220, 3.46921e-2, -3.61118e-4, 1.94318e-6, -4.07608e-9]
    else:
        c = [53.1821, 14.2110, 6.6138e-1, -3.1658e-2, 5.4654e-4, -4.3446e-6, 1.3236e-8]
    return sum(k * alt**i for i, k in enumerate(c)) if alt > 0 else 0.0


def ieee_terms(ts, a, solar_heat):
    ta, v, phi, he, eps = a["ta"], a["v"], a["phi"], a["he"], a["eps"]
    tf = (ts + ta) / 2
    mu = 1.458e-6 * (tf + 273) ** 1.5 / (tf + 383.4)
    rho = (1.293 - 1.525e-4 * he + 6.379e-9 * he**2) / (1 + 0.00367 * tf)
    k = 2.424e-2 + 7.477e-5 * tf - 4.407e-9 * tf**2
    re = D * rho * v / mu
    p = math.radians(phi)
    kang = 1.194 - math.cos(p) + 0.194 * math.cos(2 * p) + 0.368 * math.sin(2 * p)
    dt = ts - ta
    qc1 = kang * (1.01 + 1.35 * re**0.52) * k * dt
    qc2 = kang * 0.754 * re**0.6 * k * dt
    qcn = 3.645 * rho**0.5 * D**0.75 * dt**1.25
    qc = max(qc1, qc2, qcn) if v > 0 else qcn
    qr = 17.8 * D * eps * (((ts + 273) / 100) ** 4 - ((ta + 273) / 100) ** 4)
    return qc, qr, solar_heat


def ieee_solar(a):
    if "flux" in a:
        return a["alpha"] * a["flux"] * D, None
    alt, az, theta = sun_angles(a["lat"], a["day"], a["hour"], a["line_az"], 23.46)
    he = a["he"]
    ksolar = 1 + 1.148e-4 * he - 1.108e-8 * he**2
    qse = ieee_qs(alt, a["atm"]) * ksolar
    return a["alpha"] * qse * math.sin(math.radians(theta)) * D, (alt, az)


# --- CIGRE TB 601 / TB 207 -------------------------------------------------

def nu90(re, rs):
    if re <= 2650:
        return 0.641 * re**0.471
    if rs <= 0.05:
        return 0.178 * re**0.633
    return 0.048 * re**0.800


def angle_factor(delta):
    s = math.sin(math.radians(delta))
    if delta <= 24:
        return 0.42 + 0.68 * s**1.08
    return 0.42 + 0.58 * s**0.90


def nu_nat(x):
    if x < 1e2:
        return 1.02 * x**0.148
    if x < 1e4:
        return 0.850 * x**0.188
    return 0.480 * x**0.250


def cigre_terms(ts, a, solar_heat, tb207):
    ta, v, delta, y, eps, rs = a["ta"], a["v"], a["phi"], a["he"], a["eps"], a["rs"]
    tf = (ts + ta) / 2
    if tb207:
        lam = 2.42e-2 + 7.2e-5 * tf
        nu = (1.32e-5 + 9.5e-8 * tf) / math.exp(-1.16e-4 * y)
        pr = 0.715 - 2.5e-4 * tf
    else:
        lam = 2.368e-2 + 7.23e-5 * tf - 2.763e-8 * tf**2
        mu = (17.239 + 4.635e-2 * tf - 2.03e-5 * tf**2) * 1e-6
        rho = (1.293 - 1.525e-4 * y + 6.379e-9 * y**2) / (1 + 0.00367 * tf)
        nu = mu / rho
        pr = 1005.0 * mu / lam
    re = v * D / nu
    n90 = nu90(re, rs)
    forced = n90 * angle_factor(delta) if v > 0 else 0.0
    low = 0.0
    if 0 < v < 0.5:
        low = n90 * angle_factor(45.0) if tb207 else 0.55 * n90
    gr = 9.80665 * D**3 * (ts - ta) / ((tf + 273) * nu**2)
    nat = nu_nat(gr * pr)
    pc = math.pi * lam * (ts - ta) * max(forced, low, nat)
    pr_ = math.pi * D * 5.6697e-8 * eps * ((ts + 273) ** 4 - (ta + 273) ** 4)
    return pc, pr_, solar_heat


def cigre_solar(a):
    if "flux" in a:
        return a["alpha"] * a["flux"] * D, None
    alt, az, theta = sun_angles(a["lat"], a["day"], a["hour"], a["line_az"], 23.3)
    if alt <= 0:
        return 0.0, (alt, az)
    s = math.sin(math.radians(alt))
    ib0 = a["ns"] * 1280 * s / (s + 0.314)
    ib = ib0 * (1 + 1.4e-4 * a["he"] * (1367 / ib0 - 1))
    idf = (430.5 - 0.3288 * ib) * s
    f = a["albedo"]
    it = ib * (math.sin(math.radians(theta)) + math.pi / 2 * f * s) + idf * math.pi / 2 * (1 + f)
    return a["alpha"] * it * D, (alt, az)


def terms(method, ts, a):
    if method == "ieee738":
        qs, sun = ieee_solar(a)
        return ieee_terms(ts, a, qs), sun
    qs, sun = cigre_solar(a)
    return cigre_terms(ts, a, qs, method == "cigre207"), sun


def ampacity(method, a, tmax):
    (qc, qr, qs), sun = terms(method, tmax, a)
    return math.sqrt((qc + qr - qs) / resistance(tmax)), qc, qr, qs, sun


def temperature(method, a, current):
    f = lambda t: current**2 * resistance(t) + terms(method, t, a)[0][2] - sum(
        terms(method, t, a)[0][:2]
    )
    t0, t1 = a["ta"] + 20.0, a["ta"] + 80.0
    for _ in range(100):  # secant, fixed iteration cap
        f0, f1 = f(t0), f(t1)
        if f1 == f0:
            break
        t0, t1 = t1, t1 - f1 * (t1 - t0) / (f1 - f0)
        if abs(t1 - t0) < 1e-10:
            break
    return t1


EXAMPLE = dict(ta=40.0, v=0.61, phi=90.0, he=0.0, eps=0.8, alpha=0.8, rs=0.09,
               lat=30.0, day=161, hour=11.0, line_az=90.0, atm="clear", ns=1.0, albedo=0.1)
MOUNTAIN = dict(ta=30.0, v=0.3, phi=30.0, he=1500.0, eps=0.6, alpha=0.7, rs=0.09,
                lat=47.0, day=200, hour=14.5, line_az=30.0, atm="industrial", ns=0.9, albedo=0.2)
WINDY = dict(ta=25.0, v=3.0, phi=45.0, he=300.0, eps=0.5, alpha=0.5, rs=0.03, flux=900.0)

if __name__ == "__main__":
    for method in ("ieee738", "cigre601", "cigre207"):
        for name, a, tmax in (("example", EXAMPLE, 100.0), ("mountain", MOUNTAIN, 80.0)):
            i, qc, qr, qs, sun = ampacity(method, a, tmax)
            print(f"{method} {name} {tmax} C: I = {i:.6f} A, qc = {qc:.6f}, qr = {qr:.6f}, "
                  f"qs = {qs:.6f} W/m, sun = {sun}")
        t = temperature(method, WINDY, 1200.0)
        print(f"{method} windy 1200 A: T = {t:.6f} C")
