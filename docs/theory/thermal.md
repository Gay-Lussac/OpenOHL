# Thermal rating

Implemented in `crates/ols-thermal`, from the published equations of
IEEE Std 738-2012 and CIGRE Technical Brochures 601 (2014) and 207 (2002).
Clause and equation numbers below refer to those documents; the text of the
standards is not reproduced.

## Heat balance

Per unit length of conductor, with a uniform conductor temperature
\\( T \\) (no radial difference between core and surface):

\\[ m c_p \frac{dT}{dt} = P_J + P_S - P_C - P_R \\]

- **Joule heating** \\( P_J = I^2 R_{ac}(T) \\). \\( R_{ac} \\) comes from
  the wire library's `[cable.electrical] ac_resistance`: linear between the
  two nearest temperatures given, extrapolated on the same line beyond them
  (IEEE 738 clause 4.4.4; skin and magnetic effects are in the AC values).
  A single value \\( R(T_1) \\) is scaled with the temperature coefficient
  \\( \alpha_{20} \\) of the cable's current-carrying material,
  \\( R(T) = R(T_1)\,(1 + \alpha_{20}(T - 20))/(1 + \alpha_{20}(T_1 - 20)) \\):
  0.00403 /K for aluminium and its alloys (AAC, AAAC, ACSR, ACSS, ACAR,
  ACCC, and OPGW, whose outer wires are aluminium alloy or aluminium-clad),
  0.0045 /K for steel wire. A cable of kind `other` or `adss` has no known
  coefficient and needs two temperatures. Holding a single value constant
  instead would understate the hot resistance by about a fifth for an
  aluminium conductor rated at 100 °C, and overstate the ampacity.
- **Solar heating** \\( P_S \\) (below).
- **Convective cooling** \\( P_C \\): forced by the wind, or natural in still
  air — the larger governs.
- **Radiative cooling** \\( P_R = \pi D \sigma \varepsilon
  [(T+273)^4 - (T_a+273)^4] \\); IEEE writes the same with
  \\( 17.8 = \pi\sigma\cdot10^8 \\) (eq. 7a).

In steady state the left side is zero. The **ampacity** for a maximum
temperature \\( T_{max} \\) is then closed-form,
\\( I = \sqrt{(P_C + P_R - P_S)/R_{ac}(T_{max})} \\) (none when the sun alone
heats the conductor beyond \\( T_{max} \\)). The **temperature for a current**
is found by bisection between the air temperature (net heat gain ≥ 0) and a
temperature where the conductor loses more than it gains (found by doubling
the rise, at most 2000 °C above the air), to \\( 10^{-6} \\) °C. Exit clauses:
200 iterations and 1 s; it takes about 60 iterations (microseconds).

The **transient** after a step of current integrates the full equation by
fourth-order Runge–Kutta, by default in 10 s steps (the thermal time
constant of overhead conductors is several minutes, so the step error is far
below 0.01 °C; halving it changes the result by less than 10⁻³ °C in the
tests). One iteration per step; exit clauses: that many steps and 10 s.
The heat capacity \\( m c_p \\) is estimated from the cable's components
(aluminium outer wires 955 J/(kg·K), steel core 476 J/(kg·K), with IEEE 738's
temperature coefficients; the cable's mass shared between them by area ×
density) unless given; for Drake this gives 1306 J/(m·K), against about
1310 J/(m·K) in the IEEE example.

## IEEE Std 738-2012

Air properties at the film temperature \\( T_{film} = (T + T_a)/2 \\):
dynamic viscosity (eq. 13a), density at the conductor elevation \\( H_e \\)
(eq. 14a), thermal conductivity (eq. 15a).

**Forced convection** — the larger of the low and high Reynolds number
forms (eqs. 3a, 3b),

\\[ q_{c1} = K_{angle}\\,[1.01 + 1.35\\,N_{Re}^{0.52}]\\,k_f\\,(T - T_a), \qquad
    q_{c2} = K_{angle}\\,0.754\\,N_{Re}^{0.6}\\,k_f\\,(T - T_a), \\]

with \\( N_{Re} = D\rho_f V_w/\mu_f \\) and the wind direction factor
\\( K_{angle} = 1.194 - \cos\varphi + 0.194\cos 2\varphi + 0.368\sin 2\varphi \\)
for the angle φ between wind and conductor axis (eq. 4a; 1 across, 0.388
along). The formula is meant for 0–90°, so any angle given (negative, or
beyond 90° or 360°) is first folded to the acute angle between the wind and
the axis, \\( \varphi' = \arccos|\cos\varphi| \\): 180° is wind along the line,
270° across it. The CIGRE angle factor below uses the same folded angle. **Natural convection** \\( q_{cn} = 3.645\\,\rho_f^{0.5} D^{0.75}
(T - T_a)^{1.25} \\) (eq. 5a). The largest of the three is used.

**Solar heating** \\( q_s = \alpha\\,Q_{se}\\,\sin\theta\\,D \\) (eq. 8): the
total heat flux \\( Q_s \\) is a sixth-degree polynomial of the solar altitude
for a clear or an industrial atmosphere (eq. 18, Table 4), times the
elevation factor \\( 1 + 1.148\cdot10^{-4}H_e - 1.108\cdot10^{-8}H_e^2 \\)
(eq. 20). The angle θ between the beam and the conductor:
\\( \cos\theta = \cos H_c \cos(Z_c - Z_l) \\) (eq. 9).

## Solar position

From the latitude φ, day of the year N and local solar time
(IEEE 738 eqs. 16–17; CIGRE TB 601 section 3.3): declination
\\( \delta = A\sin(360°(284 + N)/365) \\) with \\( A = 23.46° \\) (IEEE) or
23.3° (CIGRE), hour angle \\( \omega = 15°(t - 12) \\), altitude
\\( \sin H_c = \cos\varphi\cos\delta\cos\omega + \sin\varphi\sin\delta \\),
azimuth \\( Z_c = C + \arctan\chi \\),
\\( \chi = \sin\omega/(\sin\varphi\cos\omega - \cos\varphi\tan\delta) \\), the
constant C of IEEE's table being the quadrant `atan2` picks. Without a line
azimuth the beam is taken across the conductor (θ = 90°, the most heating).
Instead of the sun's position, a case may give the irradiance on the
conductor directly (`solar_flux`): \\( P_S = \alpha\\,S\\,D \\) by every method.

## CIGRE TB 601 and TB 207

Convection with Nusselt numbers, \\( P_C = \pi\lambda_f (T - T_a)\\,Nu \\):

- **forced, across the conductor** \\( Nu_{90} = B\\,Re^n \\), Morgan's
  coefficients: \\( Re \le 2650 \\): B = 0.641, n = 0.471; above, for a surface
  roughness \\( R_s = d/(2(D-d)) \le 0.05 \\): 0.178, 0.633; otherwise 0.048,
  0.800 (d: outer wire diameter; default \\( R_s = 0.1 \\), round outer wires);
- **wind angle** δ: \\( Nu_\delta = Nu_{90}(A_1 + B_1\sin^{m_1}\delta) \\), with
  0.42, 0.68, 1.08 up to 24° and 0.42, 0.58, 0.90 above;
- **natural** \\( Nu_{nat} = A_2(Gr\\,Pr)^{m_2} \\): 1.02, 0.148 below
  \\( Gr\\,Pr = 10^2 \\); 0.850, 0.188 up to \\( 10^4 \\); 0.480, 0.250 above
  (extrapolated past \\( 10^6 \\));
- **low wind** (below 0.5 m/s): TB 601 takes at least \\( 0.55\\,Nu_{90} \\),
  TB 207 at least the Nusselt number at δ = 45°.

The largest of forced, low-wind and natural is used. Reynolds numbers below
100 use the first range of the table.

Air properties: TB 601 — quadratic thermal conductivity and dynamic
viscosity in \\( T_{film} \\), density at elevation (the IEEE formula),
\\( Pr = c_f\mu_f/\lambda_f \\) with \\( c_f = 1005 \\) J/(kg·K); TB 207 — linear
conductivity \\( 2.42\cdot10^{-2} + 7.2\cdot10^{-5}T_f \\), kinematic viscosity
\\( 1.32\cdot10^{-5} + 9.5\cdot10^{-8}T_f \\) divided by the relative density
\\( e^{-1.16\cdot10^{-4}y} \\), \\( Pr = 0.715 - 2.5\cdot10^{-4}T_f \\).

**Solar heating** (TB 601), \\( P_S = \alpha_s I_T D \\) with

\\[ I_T = I_B\left(\sin\eta + \tfrac{\pi}{2}F\sin H_s\right) + I_d\\,\tfrac{\pi}{2}(1 + F) \\]

the direct beam \\( I_B \\) on a surface normal to it, the diffuse sky
radiation \\( I_d \\) on a horizontal surface and the ground albedo F
(default 0.1): the beam on the conductor, the sky over its upper half and
the light reflected by the ground over its lower half. Clear sky:
\\( I_B(0) = N_s\\,1280\sin H_s/(\sin H_s + 0.314) \\), at elevation y
\\( I_B = I_B(0)[1 + 1.4\cdot10^{-4}y(1367/I_B(0) - 1)] \\),
\\( I_d = (430.5 - 0.3288\\,I_B)\sin H_s \\). The clearness ratio \\( N_s \\)
is taken 1.0 for a clear and 0.8 for an industrial atmosphere unless a case
gives one (that mapping is an assumption of OpenOHL). TB 207 is computed with
the same solar model here.

**What OpenOHL leaves out:** the radial temperature difference inside the
conductor (TB 601 section 4, IEEE 738 clause 4.5), magnetic heating of
steel cores beyond what the AC resistance carries, corona and evaporative
cooling, wind turbulence, and IEEE 738's dynamic/probabilistic variants. The
temperature of the conductor is the same along each section.

## Maximum operating temperature from clearance

`ols_thermal::limit` finds, per section, the highest conductor temperature
at which its wires still meet every ground and point (LiDAR) rule of a
clearance rule file. The section is solved alone (`ols_line::solve`) in a
base weather case (still air by default, or a case of the user's) with only
its temperature changed, and checked by the rules that apply to that case
(`ols_clearance::check_case`):

1. at the lowest temperature of the range (fails: *below the range*);
2. upwards in steps (10 °C) until a check fails (none fails: *above the
   range*);
3. bisection between the last temperature that passed and the first that
   failed until they are 0.5 °C apart; the limit is the one that passed.

The span and rule failing just above the limit are reported (the
*governing* span). Clearance falls with temperature as the sag grows; the
scan stops at the first failure on its grid, so a failure that appears and
disappears again between two grid points is missed. Wire-to-wire rules are
not checked (every section heated alike hardly changes their distances).

Cost and exit clauses: with the defaults (0–150 °C) at most 21 solutions and
checks per section — milliseconds for a short section, about a second for a
long one with LiDAR points; the budget counts every solution
(`limit::default_budget`) and stops after 30 minutes; the desktop app can
cancel. The ampacity at the limit then follows per ambient case.

## Validation

`benchmarks/thermal/ratings.toml`:

- `thermal-001` — the IEEE 738 worked example (Drake 26/7 ACSR at 100 °C,
  40 °C air, 0.61 m/s across an east-west line at 30° N, 11:00 on 10 June,
  clear air): the published results are about \\( q_c = 81.93 \\),
  \\( q_r = 39.1 \\), \\( q_s = 22.44 \\) W/m and 1025 A; OpenOHL gives 82.08,
  39.11, 22.46 W/m and 1025.4 A (within 0.2 %), solar altitude 74.89° and
  azimuth 113.95°.
- `thermal-002…008` — IEEE 738, TB 601 and TB 207 at elevation, low wind at
  an angle, industrial air, high Reynolds numbers, current to temperature —
  against an independent evaluation of the same formulas
  (`benchmarks/thermal/reference_thermal.py`: sun as a vector, secant
  instead of bisection), to 10⁻⁶. These guard the implementation of the
  formulas, not the methods themselves.

No published CIGRE TB 601 worked example is used yet; in the IEEE example's
weather TB 601 gives 1001 A and TB 207 1010 A (more solar heating from the
diffuse and reflected light, slightly more convection).

## References

- IEEE Std 738-2012, *IEEE Standard for Calculating the Current-Temperature
  Relationship of Bare Overhead Conductors*.
- CIGRE Technical Brochure 601 (2014), *Guide for thermal rating
  calculations of overhead lines* (WG B2.43).
- CIGRE Technical Brochure 207 (2002), *Thermal behaviour of overhead
  conductors* (WG 22.12).
- V. T. Morgan, *Thermal Behaviour of Electrical Conductors*, Research
  Studies Press, 1991 (the Nusselt correlations).
